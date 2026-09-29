// Headless UI test for AzBuilder's project viewer (debugger-project.js).
//
//     node scripts/debugger-ui/builder-project-smoke.mjs [--chrome <path>] [--keep] [--screenshot out.png]
//
// Serves the REAL debugger page (dll/src/desktop/shell2/common/debugger/) from a
// small HTTP server whose `POST /` is a MOCK of the debug server — the builder
// ops (as in builder-dnd-smoke.mjs) plus the `project_*` ops over an in-memory
// folder — starts a headless Chrome of its own (own profile, no window, no real
// input) and drives the page:
//
//   the Project activity and its welcome form, create a project, the tree
//   (folders, icons, context menu: new file / rename / delete), the editor
//   (tabs, highlighting that never turns file text into markup, Ctrl+S saves,
//   what the save applied), Save / Load Project (the document, components,
//   E2E tests and snapshots), selection sync (component file <-> palette card
//   <-> document instance), dragging a component file into the document,
//   Export ZIP, the App State camera button, and re-opening the last project
//   after a reload (and loading it into a fresh window).
//
// It checks what the page SENT and what it RENDERED from the answers. The
// server side is tested by
//     cargo test -p azul-layout --features e2e-server --lib project
//
// Needs node >= 21 (global WebSocket) and a Chrome / Chromium / Edge binary
// (default: the macOS Google Chrome path; `--chrome` or $CHROME to override).
// Exit code 0 = all checks passed.

import http from 'node:http';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import zlib from 'node:zlib';
import { spawn } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { Cdp, newTab } from '../e2e-web/lib/cdp.mjs';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(HERE, '../..');
const ASSETS = path.join(ROOT, 'dll/src/desktop/shell2/common/debugger');
const PNG = 'data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNkYAAAAAYAAjCB0C8AAAAASUVORK5CYII=';

const args = process.argv.slice(2);
const argVal = (name) => { const i = args.indexOf(name); return i >= 0 ? args[i + 1] : null; };
const CHROME = argVal('--chrome') || process.env.CHROME
    || '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';
const KEEP = args.includes('--keep');

// ── the mock debug server: builder ops ───────────────────────────────────

const clone = (v) => JSON.parse(JSON.stringify(v));

const mock = {
    sent: [],
    registry: { libraries: [{ name: 'builtin', modifiable: false, components: [
        { tag: 'div', display_name: 'Div' },
        { tag: 'p', display_name: 'Paragraph' },
        { tag: 'span', display_name: 'Span' },
        { tag: 'button', display_name: 'Button' },
    ] }] },
    doc: null,
    stylesheet: '',
};

function newDoc() {
    return { root: { uid: 0, kind: 'element', tag: 'body', attrs: {}, children: [] }, next: 1, undo: [], redo: [] };
}
function find(n, uid) {
    if (n.uid === uid) return n;
    for (const c of n.children) { const f = find(c, uid); if (f) return f; }
    return null;
}
function docJson(d, active) {
    return { active, can_undo: d.undo.length > 0, can_redo: d.redo.length > 0, root: clone(d.root) };
}
function registerComponent(lib, name, extra) {
    let l = mock.registry.libraries.find((x) => x.name === lib);
    if (!l) { l = { name: lib, modifiable: true, components: [] }; mock.registry.libraries.push(l); }
    l.components = l.components.filter((c) => c.tag !== name);
    l.components.push(Object.assign({ tag: name, display_name: name }, extra || {}));
}

function builderOp(msg) {
    const d = mock.doc || newDoc();
    const checkpoint = () => { d.undo.push(clone(d.root)); d.redo = []; };
    const commit = (extra) => { mock.doc = d; return Object.assign(docJson(d, true), extra || {}); };
    switch (msg.op) {
        case 'builder_get_document': return docJson(d, !!mock.doc);
        case 'builder_insert': {
            const parent = find(d.root, msg.parent);
            if (!parent) throw new Error(`no node with uid ${msg.parent}`);
            const lib = msg.library || 'builtin';
            const attrs = Object.assign({}, msg.attrs || {});
            const node = lib === 'builtin'
                ? { uid: 0, kind: 'element', tag: msg.component, attrs, children: [] }
                : { uid: 0, kind: 'component', library: lib, tag: msg.component, attrs, children: [] };
            checkpoint();
            node.uid = d.next++;
            parent.children.splice(msg.index == null ? parent.children.length : msg.index, 0, node);
            return commit({ inserted: node.uid });
        }
        case 'builder_convert_to_component': {
            const node = find(d.root, msg.node);
            if (!node || node.kind !== 'element') throw new Error('not an element');
            registerComponent(msg.library, msg.name);
            checkpoint();
            Object.assign(node, { kind: 'component', library: msg.library, tag: msg.name, attrs: {}, children: [] });
            return commit({ component: { library: msg.library, name: msg.name, fields: [] } });
        }
        default: return undefined;
    }
}

function liveHierarchy() {
    const nodes = [];
    const d = mock.doc;
    if (!d) return { root: 0, nodes: [{ index: 0, type: 'Body', tag: 'body', children: [], classes: [] }] };
    (function walk(n, parent) {
        const index = nodes.length;
        const entry = { index, type: n.kind === 'text' ? 'Text' : 'Div', tag: n.tag, parent,
            classes: n.kind === 'text' ? [] : ['azb-' + n.uid], children: [], events: [] };
        nodes.push(entry);
        for (const c of n.children) entry.children.push(walk(c, index));
        return index;
    })(d.root, -1);
    return { root: 0, node_count: nodes.length, nodes };
}

// ── the mock debug server: a project folder in memory ────────────────────

const disk = {
    root: null,                 // the open project's path
    known: new Set(),           // folders that exist
    files: new Map(),           // 'rel/path' -> text
    dirs: new Set(),            // 'rel/dir'
};

function rel(p) {
    if (typeof p !== 'string') throw new Error('path missing');
    if (p.includes('\0')) throw new Error(`${JSON.stringify(p)} contains a NUL byte`);
    const u = p.replace(/\\/g, '/');
    if (u.startsWith('/') || /^[A-Za-z]:/.test(u)) throw new Error(`${JSON.stringify(p)} is an absolute path`);
    const parts = u.split('/').filter((s) => s && s !== '.');
    if (parts.includes('..')) throw new Error(`${JSON.stringify(p)} contains '..'; project paths may not leave the project root`);
    return parts.join('/');
}
function needOpen() { if (!disk.root) throw new Error('no project is open; open a folder first (project_open)'); }
function addDirs(p) { const parts = p.split('/'); for (let i = 1; i < parts.length; i++) disk.dirs.add(parts.slice(0, i).join('/')); }
function exists(p) { return disk.files.has(p) || disk.dirs.has(p); }
function writeFile(p, text) { addDirs(p); disk.files.set(p, text); }
function projectName() { return disk.root ? disk.root.split('/').pop() : ''; }

function treeOf() {
    function children(prefix) {
        const kids = new Map();
        const consider = (p, kind) => {
            if (prefix && !p.startsWith(prefix + '/')) return;
            const rest = prefix ? p.slice(prefix.length + 1) : p;
            if (!rest || rest.includes('/')) return;
            kids.set(rest, kind);
        };
        for (const d of disk.dirs) consider(d, 'dir');
        for (const f of disk.files.keys()) consider(f, 'file');
        const entries = [...kids.entries()].sort((a, b) =>
            (a[1] === 'dir' ? 0 : 1) - (b[1] === 'dir' ? 0 : 1) || a[0].localeCompare(b[0]));
        return entries.map(([name, kind]) => {
            const p = prefix ? prefix + '/' + name : name;
            return kind === 'dir'
                ? { name, path: p, kind: 'dir', children: children(p) }
                : { name, path: p, kind: 'file', size: disk.files.get(p).length };
        });
    }
    return { name: projectName(), path: '', kind: 'dir', children: children('') };
}

function info() {
    const v = { open: !!disk.root, cwd: '/home/u', suggested: '/home/u/AzBuilderProject' };
    if (disk.root) Object.assign(v, { root: disk.root, name: projectName(), manifest: { format: 'azul-project' }, tree: treeOf() });
    return v;
}

function applyWritten(p, text) {
    const out = { applied: null };
    if (p.startsWith('styles/') && p.endsWith('.css')) {
        mock.stylesheet = text;
        out.applied = 'stylesheet';
    } else if (/^components\/[^/]+\/[^/]+\.json$/.test(p)) {
        try {
            const c = JSON.parse(text);
            const [, lib, file] = p.split('/');
            registerComponent(c.library || lib, c.name || file.replace(/\.json$/, ''), { description: c.description || '' });
            out.applied = 'component';
        } catch (e) { out.apply_error = 'not JSON: ' + e.message; }
    } else if (p === 'document.json') {
        try {
            const j = JSON.parse(text);
            mock.doc = newDoc();
            mock.doc.root = renumber(j.root || j, { n: 0 });
            out.applied = 'document';
        } catch (e) { out.apply_error = 'not JSON: ' + e.message; }
    }
    return out;
}
function renumber(n, c) {
    const out = Object.assign({}, n, { uid: c.n++ });
    out.children = (n.children || []).map((k) => renumber(k, c));
    return out;
}
function stripUids(n) {
    const o = Object.assign({}, n);
    delete o.uid;
    o.children = (n.children || []).map(stripUids);
    return o;
}

function projectOp(msg) {
    switch (msg.op) {
        case 'project_info': return info();
        case 'project_open': {
            const p = String(msg.path || '').trim();
            if (!p) throw new Error('name the project folder to open');
            let created = false;
            if (!disk.known.has(p)) {
                if (!msg.create) throw new Error(`no folder at ${p}`);
                disk.known.add(p);
            }
            disk.root = p;
            if (msg.create) {
                ['components', 'styles', 'tests', 'snapshots'].forEach((d) => disk.dirs.add(d));
                if (!disk.files.has('azul-project.json')) {
                    writeFile('azul-project.json', JSON.stringify({ format: 'azul-project', version: 1, name: projectName() }));
                    created = true;
                }
            }
            return Object.assign(info(), { created });
        }
        case 'project_close': disk.root = null; return info();
        case 'project_list': needOpen(); return { root: disk.root, name: projectName(), tree: treeOf() };
        case 'project_read_file': {
            needOpen();
            const p = rel(msg.path);
            if (!disk.files.has(p)) throw new Error(`cannot read ${JSON.stringify(p)}: No such file`);
            const content = disk.files.get(p);
            return { path: p, size: content.length, binary: false, content };
        }
        case 'project_write_file': {
            needOpen();
            const p = rel(msg.path);
            if (!p || disk.dirs.has(p)) throw new Error(`${JSON.stringify(p)} is a folder`);
            writeFile(p, msg.content);
            return Object.assign({ path: p, size: msg.content.length, written: true }, applyWritten(p, msg.content));
        }
        case 'project_create': {
            needOpen();
            const p = rel(msg.path);
            if (!p) throw new Error('the project root already exists');
            if (exists(p)) throw new Error(`${JSON.stringify(p)} already exists`);
            if (msg.directory) { addDirs(p + '/x'); disk.dirs.add(p); } else writeFile(p, msg.content || '');
            return { path: p, kind: msg.directory ? 'dir' : 'file' };
        }
        case 'project_rename': {
            needOpen();
            const from = rel(msg.from);
            const to = rel(msg.to);
            if (!from || !to) throw new Error('the project root itself cannot be renamed');
            if (!exists(from)) throw new Error(`${JSON.stringify(from)} does not exist`);
            if (exists(to)) throw new Error(`${JSON.stringify(to)} already exists`);
            for (const [k, v] of [...disk.files]) {
                if (k === from || k.startsWith(from + '/')) { disk.files.delete(k); writeFile(to + k.slice(from.length), v); }
            }
            for (const d of [...disk.dirs]) {
                if (d === from || d.startsWith(from + '/')) { disk.dirs.delete(d); disk.dirs.add(to + d.slice(from.length)); }
            }
            return { from, to };
        }
        case 'project_delete': {
            needOpen();
            const p = rel(msg.path);
            if (!p) throw new Error('the project root itself cannot be deleted');
            if (!exists(p)) throw new Error(`${JSON.stringify(p)} does not exist`);
            for (const k of [...disk.files.keys()]) if (k === p || k.startsWith(p + '/')) disk.files.delete(k);
            for (const d of [...disk.dirs]) if (d === p || d.startsWith(p + '/')) disk.dirs.delete(d);
            return { path: p, deleted: true };
        }
        case 'project_save': {
            needOpen();
            const written = [];
            const d = mock.doc || newDoc();
            writeFile('document.json', JSON.stringify({ format: 'azul-builder-document', version: 1, root: stripUids(d.root) }, null, 2));
            written.push('document.json');
            for (const l of mock.registry.libraries) {
                if (!l.modifiable) continue;
                for (const c of l.components) {
                    const p = `components/${l.name}/${c.tag}.json`;
                    writeFile(p, JSON.stringify({ format: 'azul-component', version: 1, library: l.name, name: c.tag,
                        display_name: c.display_name, description: '', fields: [], css: '', template: '<div class="x">{text}</div>' }, null, 2));
                    written.push(p);
                }
            }
            return { written, errors: [], tree: treeOf() };
        }
        case 'project_load': {
            needOpen();
            const components = [];
            for (const [p, text] of disk.files) {
                const m = /^components\/([^/]+)\/([^/]+)\.json$/.exec(p);
                if (!m) continue;
                const c = JSON.parse(text);
                registerComponent(c.library || m[1], c.name || m[2]);
                components.push((c.library || m[1]) + ':' + (c.name || m[2]));
            }
            let document = null;
            if (disk.files.has('document.json')) {
                const j = JSON.parse(disk.files.get('document.json'));
                mock.doc = newDoc();
                mock.doc.root = renumber(j.root, { n: 0 });
                mock.doc.next = 100;
                document = docJson(mock.doc, true);
            }
            const stylesheets = [...disk.files.keys()].filter((p) => p.startsWith('styles/') && p.endsWith('.css')).sort();
            return { document, components, stylesheets, errors: [] };
        }
        case 'project_export_zip':
            needOpen();
            return { download_url: 'data:application/zip;base64,UEsFBgAAAAAAAAAAAAAAAAAAAAAAAA==',
                filename: projectName() + '.zip', size_bytes: 22, file_count: disk.files.size };
        case 'project_import_zip':
            needOpen();
            writeFile('styles/imported.css', '/* imported */');
            return { written: ['styles/imported.css'], tree: treeOf() };
        default: return undefined;
    }
}

function handle(msg) {
    mock.sent.push(msg);
    switch (msg.op) {
        case 'get_state': return {};
        case 'get_node_hierarchy': return liveHierarchy();
        case 'get_component_registry': return clone(mock.registry);
        case 'get_libraries':
            return { libraries: mock.registry.libraries.map((l) => ({
                name: l.name, modifiable: !!l.modifiable, component_count: l.components.length })) };
        case 'get_component_thumbnail':
            return { library: msg.library, name: msg.name, key: '0', data: PNG, empty: false, width: 8, height: 4, cached: false };
        case 'get_app_state': return {};
        default: {
            if (msg.op.startsWith('project_')) {
                const v = projectOp(msg);
                return v === undefined ? null : v;
            }
            const v = builderOp(msg);
            return v === undefined ? null : v;
        }
    }
}

/** The icon font the real server embeds (`azul-doc codegen all` output), if built. */
function iconFont() {
    for (const dir of [ROOT, process.env.AZUL_ROOT].filter(Boolean)) {
        const br = path.join(dir, 'target/codegen/material_icons.ttf.br');
        if (fs.existsSync(br)) return zlib.brotliDecompressSync(fs.readFileSync(br));
    }
    return null;
}

function startServer() {
    const types = { '.html': 'text/html', '.js': 'application/javascript', '.css': 'text/css' };
    const font = iconFont();
    const server = http.createServer((req, res) => {
        if (req.method === 'GET' && req.url === '/material-icons.ttf') {
            if (!font) { res.writeHead(404); res.end(); return; }
            res.writeHead(200, { 'Content-Type': 'font/ttf' });
            res.end(font);
            return;
        }
        if (req.method === 'GET') {
            const name = req.url === '/' ? 'debugger.html' : req.url.replace(/^\//, '');
            const file = path.join(ASSETS, path.basename(name));
            if (!fs.existsSync(file)) { res.writeHead(404); res.end(); return; }
            res.writeHead(200, { 'Content-Type': (types[path.extname(file)] || 'application/octet-stream') + '; charset=utf-8' });
            res.end(fs.readFileSync(file));
            return;
        }
        let body = '';
        req.on('data', (c) => { body += c; });
        req.on('end', () => {
            let out;
            try {
                const msg = JSON.parse(body);
                const value = handle(msg);
                out = { status: 'ok', request_id: 1 };
                if (value !== null) out.data = { type: 'json', value };
            } catch (e) {
                out = { status: 'error', message: e.message };
            }
            res.writeHead(200, { 'Content-Type': 'application/json' });
            res.end(JSON.stringify(out));
        });
    });
    return new Promise((resolve) => server.listen(0, '127.0.0.1', () => resolve(server)));
}

// ── the browser ─────────────────────────────────────────────────────────

/** SIGTERM, wait for the exit, SIGKILL after 3 s: never leave a headless Chrome behind. */
async function stopChrome(proc) {
    if (proc.exitCode !== null || proc.signalCode !== null) return;
    const exited = new Promise((resolve) => proc.once('exit', resolve));
    proc.kill('SIGTERM');
    const hard = setTimeout(() => { try { proc.kill('SIGKILL'); } catch { /* gone */ } }, 3000);
    await exited;
    clearTimeout(hard);
}

async function startChrome() {
    const port = 9400 + Math.floor(Math.random() * 400);
    const profile = fs.mkdtempSync(path.join(process.env.AZB_TMP || os.tmpdir(), 'azp-chrome-'));
    const proc = spawn(CHROME, ['--headless=new', `--remote-debugging-port=${port}`, `--user-data-dir=${profile}`,
        '--no-first-run', '--no-default-browser-check', '--disable-gpu', '--window-size=1400,900', 'about:blank'],
        { stdio: 'ignore' });
    const base = `http://127.0.0.1:${port}`;
    for (let i = 0; i < 100; i++) {
        try { const r = await fetch(base + '/json/version'); if (r.ok) return { proc, base, profile }; } catch { /* not yet */ }
        await new Promise((r) => setTimeout(r, 100));
    }
    proc.kill();
    throw new Error(`no Chrome answering on ${base} (binary: ${CHROME})`);
}

// ── checks ──────────────────────────────────────────────────────────────

let passed = 0;
const failures = [];
function check(name, cond, detail) {
    if (cond) { passed++; console.log('ok   ' + name); }
    else { failures.push(name); console.log('FAIL ' + name + (detail !== undefined ? '\n     ' + JSON.stringify(detail) : '')); }
}

async function waitFor(cdp, expr, timeoutMs = 5000) {
    const t0 = Date.now();
    while (Date.now() - t0 < timeoutMs) {
        try { if (await cdp.eval(expr)) return true; } catch { /* page not ready */ }
        await new Promise((r) => setTimeout(r, 50));
    }
    return false;
}

const lastSent = (op) => [...mock.sent].reverse().find((m) => m.op === op);
const countSent = (op) => mock.sent.filter((m) => m.op === op).length;
const sentSince = (mark, op) => mock.sent.slice(mark).filter((m) => m.op === op);

const HELPERS = `
window.__t = {
  row(p, where) { return document.querySelector((where || '#azp-tree') + ' .azp-row[data-path="' + p + '"]'); },
  rows(where) { return [...document.querySelectorAll((where || '#azp-tree') + ' .azp-row')].map(r => r.dataset.path); },
  docRow(uid) { return document.querySelector('.azb-row[data-uid="' + uid + '"]'); },
  card(key) { return [...document.querySelectorAll('.azb-card')].find(c => c.dataset.key === key); },
  ctx(el, label) {
    const r = el.getBoundingClientRect();
    el.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true, clientX: r.left + 10, clientY: r.top + 4 }));
    const item = [...document.querySelectorAll('.azd-context-menu-item')].find(i => i.textContent.includes(label));
    if (item) item.click();
    return !!item;
  },
  menu(act) {
    const item = document.querySelector('.menu-item[data-menu="project"] .menu-dropdown-item[data-act="' + act + '"]');
    if (item) item.click();
    return !!item;
  },
  type(text) {
    const ta = document.getElementById('azp-editor-text');
    ta.value = text;
    ta.dispatchEvent(new Event('input', { bubbles: true }));
    return true;
  },
  save() {
    const ta = document.getElementById('azp-editor-text');
    ta.dispatchEvent(new KeyboardEvent('keydown', { key: 's', ctrlKey: true, bubbles: true, cancelable: true }));
    return true;
  },
  drag(src, dst, rel) {
    const dt = new DataTransfer();
    const r = dst.getBoundingClientRect();
    const o = { bubbles: true, cancelable: true, dataTransfer: dt, clientX: r.left + 12, clientY: r.top + r.height * rel };
    src.dispatchEvent(new DragEvent('dragstart', { bubbles: true, cancelable: true, dataTransfer: dt }));
    dst.dispatchEvent(new DragEvent('dragenter', o));
    const over = new DragEvent('dragover', o);
    dst.dispatchEvent(over);
    const accepted = over.defaultPrevented;
    if (accepted) dst.dispatchEvent(new DragEvent('drop', o));
    src.dispatchEvent(new DragEvent('dragend', { bubbles: true, cancelable: true, dataTransfer: dt }));
    return { accepted };
  },
  visible(id) { const e = document.getElementById(id); return !!e && !e.classList.contains('hidden') && e.offsetParent !== null; },
};
window.__downloads = [];
HTMLAnchorElement.prototype.click = function () { window.__downloads.push({ href: this.href, download: this.download }); };
true`;

async function openPage(cdp, url) {
    await cdp.send('Page.navigate', { url });
    return waitFor(cdp, `typeof app !== 'undefined' && !!window.azProject && !!window.azDnd
        && azProject.state.ready === true && !!document.querySelector('.azb-row[data-uid="0"]')`, 10000);
}

async function main() {
    const server = await startServer();
    const pageUrl = `http://127.0.0.1:${server.address().port}/`;
    const chrome = await startChrome();
    let cdp;
    try {
        const tab = await newTab(chrome.base, 'about:blank');
        cdp = new Cdp(tab.webSocketDebuggerUrl);
        await cdp.connect();
        await cdp.send('Runtime.enable');
        await cdp.send('Page.enable');
        const ready = await openPage(cdp, pageUrl);
        check('the page loads debugger-project.js next to the builder', ready,
            await cdp.eval(`({ p: !!window.azProject, d: !!window.azDnd, ready: window.azProject && azProject.state.ready })`).catch(String));
        if (!ready) throw new Error('page not ready');
        await cdp.eval(HELPERS);

        // 1. The Project activity: a welcome form while nothing is open.
        check('the activity bar has a Project icon',
            await cdp.eval(`!!document.querySelector('.activity-icon[data-view="project"]')`));
        await cdp.eval(`document.querySelector('.activity-icon[data-view="project"]').click(); true`);
        check('...which shows the project sidebar and the editor view, and hides the others',
            await waitFor(cdp, `__t.visible('sidebar-project') && __t.visible('view-project')
                && !__t.visible('view-inspector') && !__t.visible('sidebar-inspector')`));
        check('with no project open, the welcome form suggests a folder from the server',
            await waitFor(cdp, `(document.getElementById('azp-open-path') || {}).value === '/home/u/AzBuilderProject'`),
            await cdp.eval(`(document.getElementById('azp-open-path') || {}).value`));

        // 2. Create a project.
        await cdp.eval(`document.getElementById('azp-open-path').value = '/home/u/demo';
            document.getElementById('azp-create-btn').click(); true`);
        await waitFor(cdp, `__t.rows().includes('azul-project.json')`);
        check('"Create" sends project_open {create: true}',
            JSON.stringify(lastSent('project_open')) === JSON.stringify({ op: 'project_open', path: '/home/u/demo', create: true }),
            lastSent('project_open'));
        check('...and the tree shows the skeleton, folders first',
            await cdp.eval(`JSON.stringify(__t.rows()) === JSON.stringify(['', 'components', 'snapshots', 'styles', 'tests', 'azul-project.json'])`),
            await cdp.eval('__t.rows()'));
        check('rows carry icons for what they are',
            await cdp.eval(`__t.row('styles').querySelector('.azp-icon').textContent === 'folder'
                && __t.row('azul-project.json').querySelector('.azp-icon').textContent === 'settings'`));

        // 3. New file from the context menu of a folder.
        await cdp.eval(`window.prompt = () => 'app.css'; true`);
        check('the folder context menu offers "New file…"', await cdp.eval(`__t.ctx(__t.row('styles'), 'New file')`));
        await waitFor(cdp, `!!__t.row('styles/app.css')`);
        const created = lastSent('project_create');
        check('...which sends project_create inside that folder', created && created.path === 'styles/app.css' && !created.directory, created);
        check('...and opens the new file in an editor tab',
            await waitFor(cdp, `!!document.querySelector('.azp-tab.active[data-path="styles/app.css"]')
                && document.getElementById('azp-editor-text').value === ${JSON.stringify(created ? created.content || '' : '')}`));

        // 4. Edit + Ctrl+S.
        await cdp.eval(`__t.type('.card { color: #ff0000; width: 10px; }'); true`);
        check('typing marks the tab dirty', await cdp.eval(`document.querySelector('.azp-tab.active').classList.contains('azp-dirty')`));
        check('the highlight layer follows the text (a CSS property is marked)',
            await waitFor(cdp, `!!document.querySelector('#azp-editor-code .azp-tok-prop')`));
        await cdp.eval(`__t.save(); true`);
        await waitFor(cdp, `!document.querySelector('.azp-tab.active').classList.contains('azp-dirty')`);
        const wrote = lastSent('project_write_file');
        check('Ctrl+S sends project_write_file with the editor text',
            wrote && wrote.path === 'styles/app.css' && wrote.content === '.card { color: #ff0000; width: 10px; }', wrote);
        check('...and the status says the stylesheet was applied to the window',
            await waitFor(cdp, `/applied/i.test(document.getElementById('azp-status').textContent)
                && /stylesheet/i.test(document.getElementById('azp-status').textContent)`),
            await cdp.eval(`document.getElementById('azp-status').textContent`));

        // 5. File text never becomes markup.
        await cdp.eval(`window.prompt = () => 'evil.html'; __t.ctx(__t.row(''), 'New file'); true`);
        await waitFor(cdp, `!!document.querySelector('.azp-tab.active[data-path="evil.html"]')`);
        await cdp.eval(`__t.type('<img src=x onerror="window.__pwned = 1"><script>window.__pwned = 2</script>'); true`);
        await new Promise((r) => setTimeout(r, 300));
        check('a file\'s text is shown, never run: no <img>/<script> in the highlight layer',
            await cdp.eval(`document.querySelectorAll('#azp-editor-code img, #azp-editor-code script').length === 0
                && window.__pwned === undefined && document.getElementById('azp-editor-code').textContent.includes('<img src=x')`));

        // 6. Rename (F2) and a refused name.
        await cdp.eval(`__t.row('evil.html').click(); true`);
        let mark = mock.sent.length;
        await cdp.eval(`window.prompt = () => '../escape.html';
            document.getElementById('azp-tree').dispatchEvent(new KeyboardEvent('keydown', { key: 'F2', bubbles: true })); true`);
        await new Promise((r) => setTimeout(r, 200));
        check('a new name with a path in it is refused before anything is sent',
            sentSince(mark, 'project_rename').length === 0);
        await cdp.eval(`window.prompt = () => 'safe.html';
            document.getElementById('azp-tree').dispatchEvent(new KeyboardEvent('keydown', { key: 'F2', bubbles: true })); true`);
        await waitFor(cdp, `!!__t.row('safe.html')`);
        check('F2 renames the selected file (project_rename) and its open tab follows',
            JSON.stringify(lastSent('project_rename')) === JSON.stringify({ op: 'project_rename', from: 'evil.html', to: 'safe.html' })
            && await waitFor(cdp, `!!document.querySelector('.azp-tab[data-path="safe.html"]') && !document.querySelector('.azp-tab[data-path="evil.html"]')`),
            lastSent('project_rename'));

        // 7. Delete from the context menu.
        await cdp.eval(`window.confirm = () => true; __t.ctx(__t.row('safe.html'), 'Delete'); true`);
        await waitFor(cdp, `!__t.row('safe.html')`);
        check('"Delete" sends project_delete and closes the file\'s tab',
            JSON.stringify(lastSent('project_delete')) === JSON.stringify({ op: 'project_delete', path: 'safe.html' })
            && await cdp.eval(`!document.querySelector('.azp-tab[data-path="safe.html"]')`), lastSent('project_delete'));

        // 8. Build something, convert it, Save Project.
        await cdp.eval(`app.ui.switchView('inspector'); true`);
        await cdp.eval(`azDnd.send({ op: 'builder_insert', parent: 0, component: 'div' }); true`);
        await waitFor(cdp, `!!__t.docRow(1)`);
        await cdp.eval(`azDnd.send({ op: 'builder_convert_to_component', node: 1, library: 'user', name: 'card' }); true`);
        await waitFor(cdp, `!!__t.docRow(1) && __t.docRow(1).textContent.includes('user:card')`);
        await cdp.eval(`azDnd.renderPalette(); true`);
        await waitFor(cdp, `!!__t.card('user:card')`);
        mark = mock.sent.length;
        check('the Project menu has "Save Project"', await cdp.eval(`__t.menu('save')`));
        await waitFor(cdp, `!!__t.row('components/user/card.json', '#azp-mini-tree')`);
        check('Save Project sends project_save',
            sentSince(mark, 'project_save').length === 1);
        const testWrites = sentSince(mark, 'project_write_file').filter((m) => m.path.startsWith('tests/'));
        check('...and writes every E2E test as tests/<name>.json (CLI format)',
            testWrites.length >= 1 && testWrites[0].path === 'tests/test-1.json'
            && JSON.parse(testWrites[0].content).steps[0].op === 'get_state', testWrites);
        check('the Inspector\'s Project section shows the saved component file',
            await cdp.eval(`!!__t.row('components/user/card.json', '#azp-mini-tree') && !!__t.row('document.json', '#azp-mini-tree')`),
            await cdp.eval(`__t.rows('#azp-mini-tree')`));

        // 9. Selection sync: component file -> palette card + document instance.
        await cdp.eval(`__t.row('components/user/card.json', '#azp-mini-tree').click(); true`);
        check('selecting a component file highlights its palette card',
            await waitFor(cdp, `__t.card('user:card').classList.contains('azp-linked')`));
        check('...and selects its instance in the document',
            await waitFor(cdp, `azDnd.state.selected === 1 && __t.docRow(1).classList.contains('selected')`),
            await cdp.eval('azDnd.state.selected'));

        // 10. ...and back: document instance -> component file.
        await cdp.eval(`__t.docRow(0).click(); true`);
        await waitFor(cdp, `!__t.row('components/user/card.json', '#azp-mini-tree').classList.contains('selected')`);
        await cdp.eval(`__t.docRow(1).click(); true`);
        check('selecting an instance in the document selects its component file in the tree',
            await waitFor(cdp, `__t.row('components/user/card.json', '#azp-mini-tree').classList.contains('selected')`));
        await cdp.eval(`__t.docRow(0).click(); true`);
        await cdp.eval(`__t.card('builtin:div').click(); true`);
        await cdp.eval(`__t.card('user:card').click(); true`);
        check('clicking a palette card selects its component file too',
            await waitFor(cdp, `__t.row('components/user/card.json', '#azp-mini-tree').classList.contains('selected')`));

        // 11. Drag a component file into the document.
        const r = await cdp.eval(`__t.drag(__t.row('components/user/card.json', '#azp-mini-tree'), __t.docRow(0), 0.5)`);
        await waitFor(cdp, `!!__t.docRow(2)`);
        check('a component file drags from the tree into the document (builder_insert)',
            r.accepted && JSON.stringify(lastSent('builder_insert')) === JSON.stringify({ op: 'builder_insert', parent: 0, component: 'card', library: 'user' }),
            { r, sent: lastSent('builder_insert') });

        // 12. Open the component file and save it: the component is re-registered.
        await cdp.eval(`__t.row('components/user/card.json', '#azp-mini-tree').dispatchEvent(new MouseEvent('dblclick', { bubbles: true })); true`);
        check('double-clicking a file opens it in the Project editor',
            await waitFor(cdp, `__t.visible('view-project') && !!document.querySelector('.azp-tab.active[data-path="components/user/card.json"]')
                && document.getElementById('azp-editor-text').value.includes('azul-component')`));
        mark = mock.sent.length;
        await cdp.eval(`__t.type(document.getElementById('azp-editor-text').value.replace('"description": ""', '"description": "Edited"')); __t.save(); true`);
        const compStatus = await waitFor(cdp, `/component/i.test(document.getElementById('azp-status').textContent)`);
        await new Promise((res) => setTimeout(res, 300));
        check('saving a component file re-registers it and reloads the palette',
            compStatus && sentSince(mark, 'project_write_file').length === 1
            && sentSince(mark, 'get_component_registry').length >= 1,
            { status: await cdp.eval(`document.getElementById('azp-status').textContent`), sent: mock.sent.slice(mark).map((m) => m.op) });

        // 13. Load Project: the document, and the browser-side tests + snapshots.
        disk.files.set('tests/extra.json', JSON.stringify({ name: 'Extra', steps: [{ op: 'get_state' }] }));
        disk.files.set('snapshots/s1.json', JSON.stringify({ alias: 'S1', state: { a: 1 } }));
        mark = mock.sent.length;
        await cdp.eval(`window.confirm = () => true; __t.menu('load'); true`);
        check('Load Project sends project_load',
            await waitFor(cdp, `app.state.tests.some(t => t.name === 'Extra')`) && sentSince(mark, 'project_load').length === 1,
            mock.sent.slice(mark).map((m) => m.op));
        check('...and brings the project\'s E2E tests and snapshots into the page',
            await cdp.eval(`app.state.tests.some(t => t.name === 'Extra' && t.steps[0].op === 'get_state')
                && JSON.stringify(app.state.snapshots.S1) === '{"a":1}'`));

        // 14. Export ZIP.
        await cdp.eval(`__t.menu('export-zip'); true`);
        check('Export Project as ZIP downloads the archive the server built',
            await waitFor(cdp, `window.__downloads.some(d => d.download === 'demo.zip' && d.href.startsWith('data:application/zip'))`),
            await cdp.eval('window.__downloads'));

        // `--screenshot <file.png>`: the Project view with a file open.
        const shot = argVal('--screenshot');
        if (shot) {
            await cdp.eval(`azProject.openFile('styles/app.css'); true`);
            await waitFor(cdp, `!!document.querySelector('.azp-tab.active[data-path="styles/app.css"]')`);
            fs.writeFileSync(shot, await cdp.screenshot());
            console.log('     screenshot: ' + shot);
        }

        // 15. A reload keeps the project the server has open.
        mark = mock.sent.length;
        let again = await openPage(cdp, pageUrl);
        await cdp.eval(HELPERS);
        check('after a reload the page shows the project the server still has open',
            again && await waitFor(cdp, `!!__t.row('components/user/card.json', '#azp-mini-tree')`)
            && sentSince(mark, 'project_open').length === 0 && sentSince(mark, 'project_load').length === 0,
            mock.sent.slice(mark).map((m) => m.op));

        // 16. The App State camera button (it used to call a function that did not exist).
        await cdp.eval(`app.state.appStateJson = { count: 5 }; window.prompt = () => 'snap one'; true`);
        mark = mock.sent.length;
        await cdp.eval(`document.querySelector('#appstate-panel [title="Save Snapshot"]').click(); true`);
        check('the camera button saves a named snapshot',
            await waitFor(cdp, `JSON.stringify(app.state.snapshots['snap one']) === '{"count":5}'`));
        await new Promise((res) => setTimeout(res, 300));
        check('...into the open project as well',
            sentSince(mark, 'project_write_file').some((m) => m.path === 'snapshots/snap-one.json'
                && JSON.parse(m.content).alias === 'snap one'), sentSince(mark, 'project_write_file'));

        // 17. The app restarted (no project open on the server): a reload
        //     re-opens the last project without creating anything...
        disk.root = null;
        mark = mock.sent.length;
        again = await openPage(cdp, pageUrl);
        await cdp.eval(HELPERS);
        let reopen = sentSince(mark, 'project_open');
        check('a reload re-opens the last project (project_open without create)',
            again && await waitFor(cdp, `!!__t.row('document.json', '#azp-mini-tree')`)
            && reopen.length === 1 && reopen[0].path === '/home/u/demo' && !reopen[0].create, reopen);
        check('...but does not load it over a document the window already shows',
            sentSince(mark, 'project_load').length === 0, mock.sent.slice(mark).map((m) => m.op));

        // ...and LOADS it into a fresh AzBuilder window (an empty body).
        mock.doc = null;
        disk.root = null;
        mark = mock.sent.length;
        again = await openPage(cdp, pageUrl);
        await cdp.eval(HELPERS);
        check('a fresh window gets the last project loaded into it',
            again && await waitFor(cdp, `!!__t.docRow(1)`) && sentSince(mark, 'project_load').length === 1,
            mock.sent.slice(mark).map((m) => m.op));

        // 18. Close: the welcome form again, and a reload no longer re-opens it.
        await cdp.eval(`window.confirm = () => true; __t.menu('close'); true`);
        check('Close Project sends project_close and shows the welcome form again',
            await waitFor(cdp, `!!document.getElementById('azp-open-path')`) && countSent('project_close') === 1);
        mark = mock.sent.length;
        again = await openPage(cdp, pageUrl);
        await cdp.eval(HELPERS);
        await new Promise((res) => setTimeout(res, 300));
        check('after an explicit close a reload opens nothing, and lists the project under Recent',
            again && sentSince(mark, 'project_open').length === 0
            && await waitFor(cdp, `[...document.querySelectorAll('.azp-recent')].some(r => r.dataset.path === '/home/u/demo')`),
            mock.sent.slice(mark).map((m) => m.op));

        check('no page exceptions', cdp.exceptions.length === 0, cdp.exceptions);
        const errors = cdp.console.filter((c) => c.kind === 'error');
        check('no console errors', errors.length === 0, errors);
    } finally {
        if (cdp) cdp.close();
        await stopChrome(chrome.proc);
        server.close();
        if (!KEEP) { try { fs.rmSync(chrome.profile, { recursive: true, force: true }); } catch { /* ignore */ } }
    }
    console.log(`\n${passed} passed, ${failures.length} failed`);
    process.exit(failures.length ? 1 : 0);
}

main().catch((e) => { console.error(e); process.exit(2); });
