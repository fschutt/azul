// Headless UI test for AzBuilder's quick export dialogs (debugger-export.js).
//
//     node scripts/debugger-ui/builder-export-smoke.mjs [--chrome <path>] [--keep] [--screenshot out.png]
//
// Serves the REAL debugger page (dll/src/desktop/shell2/common/debugger/) from a
// small HTTP server whose `POST /` MOCKS the debug server (the builder document
// ops of builder-dnd-smoke.mjs + the export ops), starts a headless Chrome of its
// own and drives the page with synthetic events:
//
//   Export menu -> "Compile CSS to…" (source, rule ticks, language, Copy,
//   Download), "HTML -> DOM (code)" (paste, language, function / app, its CSS,
//   a parse error with its line and column), "Subtree -> code" from the
//   Document toolbar and from a row's
//   context menu (mode, language, function name), "Component -> code", the
//   focus trap, Escape closing with focus back on the opener, a Delete key
//   inside a dialog NOT deleting a node, and Export > Code downloading the zip
//   the server answers as a data URI.
//
// It checks what the page SENT and what it SHOWED. The server side is tested by
//     cargo test -p azul-layout --features e2e-server --lib export_tests
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
const EMPTY_ZIP = 'data:application/zip;base64,UEsFBgAAAAAAAAAAAAAAAAAAAAAAAA==';

const args = process.argv.slice(2);
const argVal = (name) => { const i = args.indexOf(name); return i >= 0 ? args[i + 1] : null; };
const CHROME = argVal('--chrome') || process.env.CHROME
    || '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';
const KEEP = args.includes('--keep');

// ── the mock debug server ────────────────────────────────────────────────

const clone = (v) => JSON.parse(JSON.stringify(v));

const mock = {
    sent: [],
    registry: { libraries: [
        { name: 'builtin', modifiable: false, exportable: false, components: [
            { tag: 'div', display_name: 'Div' },
            { tag: 'p', display_name: 'Paragraph' },
            { tag: 'span', display_name: 'Span' },
        ] },
        { name: 'user', modifiable: true, exportable: true, components: [
            { tag: 'my-card', display_name: 'My Card' },
        ] },
    ] },
    doc: null,
};

function newDoc() {
    return { root: { uid: 0, kind: 'element', tag: 'body', attrs: {}, children: [] }, next: 1 };
}
function find(n, uid) {
    if (n.uid === uid) return n;
    for (const c of n.children) { const f = find(c, uid); if (f) return f; }
    return null;
}
function parentOf(n, uid) {
    for (let i = 0; i < n.children.length; i++) {
        if (n.children[i].uid === uid) return { parent: n, index: i };
        const f = parentOf(n.children[i], uid);
        if (f) return f;
    }
    return null;
}
const docJson = (d, active) => ({ active, can_undo: false, can_redo: false, root: clone(d.root) });

function builderOp(msg) {
    const d = mock.doc || newDoc();
    switch (msg.op) {
        case 'builder_get_document': return docJson(d, !!mock.doc);
        case 'builder_insert': {
            const parent = find(d.root, msg.parent);
            if (!parent) throw new Error(`no node with uid ${msg.parent} in the builder document`);
            const lib = msg.library || 'builtin';
            const attrs = Object.assign({}, msg.attrs || {});
            const node = lib === 'builtin'
                ? { uid: d.next++, kind: 'element', tag: msg.component, attrs, children: [] }
                : { uid: d.next++, kind: 'component', library: lib, tag: msg.component, attrs, children: [] };
            parent.children.splice(msg.index == null ? parent.children.length : msg.index, 0, node);
            mock.doc = d;
            return Object.assign(docJson(d, true), { inserted: node.uid });
        }
        case 'builder_delete': {
            const from = parentOf(d.root, msg.node);
            if (!from) throw new Error(`no node with uid ${msg.node}`);
            from.parent.children.splice(from.index, 1);
            mock.doc = d;
            return docJson(d, true);
        }
        default: return undefined;
    }
}

/** A tiny CSS "parser" for the mock: one rule per `selector { … }`. */
function rulesOf(css) {
    const rules = [];
    const re = /([^{}]+)\{([^}]*)\}/g;
    let m;
    while ((m = re.exec(css))) {
        rules.push({ index: rules.length, selector: m[1].trim(), declarations: m[2].trim(),
            classes: (m[1].match(/\.[\w-]+/g) || []).map((c) => c.slice(1)), conditional: false });
    }
    return rules;
}

const DOC_CSS = '.card { padding-top: 8px; }\n.pill { margin-top: 2px; }\n';

function exportOp(msg) {
    switch (msg.op) {
        case 'get_codegen_languages':
            // ONE list (azul_css::codegen::all_backends), `dom`: the printer does DOM export.
            return {
                languages: [
                    { id: 'rust', label: 'Rust', ext: 'rs', dom: true },
                    { id: 'c', label: 'C', ext: 'h', dom: true },
                    { id: 'cpp', label: 'C++', ext: 'hpp', dom: true },
                    { id: 'python', label: 'Python', ext: 'py', dom: true },
                    { id: 'java', label: 'Java', ext: 'java', dom: false,
                      no_dom_reason: 'the Java printer does not print DOM construction yet' },
                ],
            };
        case 'get_css_rules': {
            let css;
            if (msg.source === 'document') css = DOC_CSS;
            else if (msg.source === 'node') css = `.card { color: red }\n`;
            else if (msg.source === 'component') css = `.${msg.name} { margin-top: 1px; }\n`;
            else css = msg.css || '';
            return { css, rules: rulesOf(css), warnings: [] };
        }
        case 'compile_css': {
            const all = rulesOf(msg.css || '');
            const picked = msg.rules ? msg.rules.map((i) => all[i]).filter(Boolean) : all;
            if (msg.language === 'klingon') throw new Error('no code generator for "klingon"');
            return {
                language: msg.language, file_name: 'styles.' + (msg.language === 'cpp' ? 'hpp' : 'rs'),
                code: `// ${msg.language}: ${picked.map((r) => r.selector).join(' ')}\n`,
                warnings: [], rule_count: picked.length,
            };
        }
        case 'export_subtree_code': {
            if (msg.node !== 0 && !(mock.doc && find(mock.doc.root, msg.node))) {
                throw new Error(`no node with uid ${msg.node} in the builder document`);
            }
            const ext = { rust: 'rs', c: 'h', cpp: 'hpp', python: 'py' }[msg.language];
            const name = msg.function_name || 'render_x';
            const code = `// subtree ${msg.node} ${msg.language} ${msg.mode || 'function'} ${name}\n`;
            if (msg.mode === 'app') {
                // An app is a project; `code` is its main file.
                const main = msg.language === 'c' ? 'main.c' : 'main.' + ext;
                return {
                    language: msg.language, file_name: main, code, warnings: [],
                    files: [
                        { path: 'ui.' + ext, contents: `// ui ${msg.language}\n` },
                        { path: main, contents: code },
                        { path: 'Makefile', contents: 'app: main.c\n' },
                    ],
                };
            }
            return { language: msg.language, file_name: name + '.' + ext, code, files: [], warnings: [] };
        }
        case 'html_to_code': {
            // `class=big` (no quotes) stands for markup that does not parse.
            if (/class=big/.test(msg.html)) {
                return { language: msg.language, file_name: '', code: '', files: [], warnings: [],
                    errors: [{ message: 'Invalid attribute: Invalid quote: got b', line: 2, column: 10 }] };
            }
            const ext = { rust: 'rs', c: 'h', cpp: 'hpp', python: 'py' }[msg.language];
            const name = msg.function_name || 'render_card';
            const code = `// html ${msg.language} ${msg.mode || 'function'} ${name}${msg.css ? ' css' : ''}\n`;
            if (msg.mode === 'app') {
                const main = msg.language === 'c' ? 'main.c' : 'main.' + ext;
                const files = [{ path: 'ui.' + ext, contents: `// ui ${msg.language}\n` },
                    { path: main, contents: code }];
                if (msg.css) files.push({ path: 'styles.' + ext, contents: '// styles\n' });
                return { language: msg.language, file_name: main, code, files, warnings: [], errors: [] };
            }
            const file = name + '.' + ext;
            const files = msg.css ? [{ path: file, contents: code }, { path: 'styles.' + ext, contents: '// styles\n' }] : [];
            return { language: msg.language, file_name: file, code, files, warnings: [], errors: [] };
        }
        case 'export_component_code':
            return {
                language: msg.language, file_name: `${msg.library}_${msg.name}.rs`,
                code: `// component ${msg.library}:${msg.name} ${msg.language}\n`, warnings: [],
            };
        case 'export_component_library':
            if (msg.library === 'builtin') throw new Error("Library 'builtin' not found or is not exportable");
            return { name: msg.library, version: '1.0.0', components: [{ name: 'my-card' }] };
        case 'export_code_zip':
            return { download_url: EMPTY_ZIP, filename: `azul-export-${msg.language}.zip`,
                size_bytes: 22, file_count: 3, files: ['src/main.rs', 'Cargo.toml', 'README.md'], warnings: [] };
        default: return undefined;
    }
}

function liveHierarchy() {
    const nodes = [];
    const d = mock.doc;
    if (!d) return { root: 0, nodes: [{ index: 0, type: 'Body', tag: 'body', children: [], classes: [] }] };
    (function walk(n, parent) {
        const index = nodes.length;
        const entry = { index, type: 'Div', tag: n.tag, parent, classes: ['azb-' + n.uid], children: [], events: [] };
        nodes.push(entry);
        for (const c of n.children) entry.children.push(walk(c, index));
        return index;
    })(d.root, -1);
    return { root: 0, node_count: nodes.length, nodes };
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
            return { library: msg.library, name: msg.name, key: '0', data: PNG, empty: false,
                width: 8, height: 4, cached: false };
        case 'get_app_state': return {};
        default: {
            const v = exportOp(msg);
            if (v !== undefined) return v;
            const b = builderOp(msg);
            return b === undefined ? null : b;
        }
    }
}

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
    const profile = fs.mkdtempSync(path.join(process.env.AZB_TMP || os.tmpdir(), 'azx-chrome-'));
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
        if (await cdp.eval(expr)) return true;
        await new Promise((r) => setTimeout(r, 50));
    }
    return false;
}

const lastSent = (op) => [...mock.sent].reverse().find((m) => m.op === op);
/** Wait until what the page SENT satisfies `pred` (node side). */
async function waitSent(pred, timeoutMs = 5000) {
    const t0 = Date.now();
    while (Date.now() - t0 < timeoutMs) {
        if (pred()) return true;
        await new Promise((r) => setTimeout(r, 50));
    }
    return false;
}
const countSent = (op) => mock.sent.filter((m) => m.op === op).length;

const HELPERS = `
window.__downloads = [];
HTMLAnchorElement.prototype.click = function () {
  window.__downloads.push({ download: this.download, href: String(this.href).slice(0, 40) });
};
window.__t = {
  dlg() { return document.querySelector('.azx-dialog'); },
  kind() { const d = this.dlg(); return d ? d.dataset.kind : null; },
  code() { const p = document.querySelector('.azx-code'); return p ? p.textContent : null; },
  status() { const s = document.querySelector('.azx-status'); return s ? s.textContent : null; },
  menu(act) {
    document.querySelector('.menu-item[data-menu="export"]').click();
    const item = document.querySelector('.menu-dropdown-item[data-azx="' + act + '"]');
    item.click();
    return !!item;
  },
  key(key, mods, target) {
    const t = target || document.activeElement || document.body;
    const e = new KeyboardEvent('keydown', Object.assign({ key, bubbles: true, cancelable: true }, mods || {}));
    t.dispatchEvent(e);
    return e.defaultPrevented;
  },
  select(sel, value) {
    const s = document.querySelector(sel);
    s.value = value;
    s.dispatchEvent(new Event('change', { bubbles: true }));
  },
  row(uid) { return document.querySelector('.azb-row[data-uid="' + uid + '"]'); },
  /** An item's own label (its text, not its icon's). */
  label(item) { return [...item.childNodes].filter(n => n.nodeType === 3).map(n => n.textContent).join('').trim(); },
  /** The Export menu, top level: labels, a submenu as 'Label > item, item'. */
  exportMenu() {
    const dd = document.querySelector('.menu-item[data-menu="export"] > .menu-dropdown');
    return [...dd.children].filter(c => c.classList.contains('menu-dropdown-item')).map(c => {
      const sub = c.querySelector(':scope > .menu-submenu');
      return sub ? this.label(c) + ' > ' + [...sub.children].map(i => this.label(i)).join(', ') : this.label(c);
    });
  },
  zipItem(lang) { return document.querySelector('.menu-dropdown-item[data-azx="zip"][data-lang="' + lang + '"]'); },
  inDialog() { const d = this.dlg(); return !!d && d.contains(document.activeElement); },
};
true`;

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
        await cdp.send('Page.navigate', { url: pageUrl });
        const ready = await waitFor(cdp, `typeof app !== 'undefined' && !!window.azExport && !!window.azDnd
            && azDnd.state.mode === 'document' && !!document.querySelector('.azb-row[data-uid="0"]')`, 10000);
        check('the page loads debugger-export.js next to the builder', ready,
            await cdp.eval(`({ exp: !!window.azExport, dnd: !!window.azDnd })`).catch((e) => String(e)));
        if (!ready) throw new Error('page not ready');
        await cdp.eval(HELPERS);

        check('the Export menu offers the four quick exports',
            await cdp.eval(`['css','html','subtree','component'].every(a => !!document.querySelector('.menu-dropdown-item[data-azx="' + a + '"]'))`));
        check('..."HTML \u2192 DOM (code)" right after "Compile CSS to…"',
            await cdp.eval(`(() => { const items = [...document.querySelectorAll('.menu-dropdown-item[data-azx]')].map(i => i.dataset.azx);
                return items.indexOf('html') === items.indexOf('css') + 1; })()`));
        // B7: the Export menu, restructured (every old item has a place).
        await waitFor(cdp, `!!__t.zipItem('java')`);
        check('Export reads Compile > (CSS…, DOM…), Subtree as Component…, Components…, Code (ZIP) > …, then the page\'s own items',
            JSON.stringify((await cdp.eval('__t.exportMenu()')).map((l) => l.replace(/ > .*$/, ''))) === JSON.stringify([
                'Compile', 'Subtree as Component…', 'Components…', 'Code (ZIP)',
                'Project as JSON', 'E2E Tests (CLI format)', 'Builder document (JSON)']),
            await cdp.eval('__t.exportMenu()'));
        check('...Compile holds CSS… and DOM… (the CSS and HTML → DOM dialogs)',
            (await cdp.eval('__t.exportMenu()'))[0] === 'Compile > CSS…, DOM…'
            && await cdp.eval(`[...document.querySelectorAll('.menu-submenu .menu-dropdown-item[data-azx]')].slice(0, 2).map(i => i.dataset.azx).join() === 'css,html'`));
        check('...Code (ZIP) offers every language the server lists, in its order',
            (await cdp.eval('__t.exportMenu()'))[3] === 'Code (ZIP) > Rust, C, C++, Python, Java (no DOM export yet)',
            (await cdp.eval('__t.exportMenu()'))[3]);
        check('...a language that cannot build a UI is listed but disabled, with the server\'s reason',
            await cdp.eval(`(() => { const j = __t.zipItem('java'); return !!j && j.classList.contains('azx-disabled')
                && j.getAttribute('aria-disabled') === 'true' && /does not print DOM construction/.test(j.title)
                && !__t.zipItem('rust').classList.contains('azx-disabled'); })()`));
        const zipsBefore = countSent('export_code_zip');
        await cdp.eval(`window.__downloads = []; const j = __t.zipItem('java'); if (j) j.click(); true`);
        await new Promise((r) => setTimeout(r, 150));
        check('...clicking it exports nothing', countSent('export_code_zip') === zipsBefore);
        await cdp.eval(`(() => { const c = __t.zipItem('cpp'); if (c) c.click(); return true; })()`);
        await waitFor(cdp, `window.__downloads.length > 0`);
        check('...clicking an enabled one downloads that project',
            lastSent('export_code_zip') && lastSent('export_code_zip').language === 'cpp'
            && await cdp.eval(`window.__downloads.some(d => d.download === 'azul-export-cpp.zip')`),
            lastSent('export_code_zip'));

        check('the Document toolbar has an "export as code" button',
            await waitFor(cdp, `!!document.querySelector('#azb-toolbar [data-azx-act="export"]')`));

        // A document to export: body > div.card (1) > p (2)
        mock.doc = newDoc();
        builderOp({ op: 'builder_insert', parent: 0, component: 'div', attrs: { class: 'card' } });
        builderOp({ op: 'builder_insert', parent: 1, component: 'p', attrs: { text: 'Hello' } });
        await cdp.eval(`azDnd.refresh()`);
        await waitFor(cdp, `!!__t.row(2)`);

        // ── Compile CSS to… (no node selected: the document stylesheet) ──
        await cdp.eval(`document.getElementById('terminal-cmd').focus(); true`);
        await cdp.eval(`__t.menu('css')`);
        await waitFor(cdp, `__t.kind() === 'css' && /rust: \\.card \\.pill/.test(__t.code() || '')`);
        check('"Compile CSS to…" opens a modal dialog',
            await cdp.eval(`(() => { const d = __t.dlg(); return !!d && d.getAttribute('role') === 'dialog'
                && d.getAttribute('aria-modal') === 'true' && !!document.getElementById(d.getAttribute('aria-labelledby')); })()`));
        check('...asks the server for its languages and the document stylesheet',
            countSent('get_codegen_languages') === 1 && lastSent('get_css_rules').source === 'document',
            lastSent('get_css_rules'));
        check('...shows the CSS and one tick per rule',
            await cdp.eval(`document.querySelector('.azx-css').value.includes('.pill')
                && document.querySelectorAll('.azx-rule input:checked').length === 2`));
        check('...and compiles every rule at once (no `rules` = the whole sheet)',
            JSON.stringify(lastSent('compile_css')) === JSON.stringify({ op: 'compile_css', language: 'rust', source: 'text', css: DOC_CSS }),
            lastSent('compile_css'));
        check('...the CSS languages are the server\u2019s ONE list of code generators, all usable',
            await cdp.eval(`[...document.querySelectorAll('.azx-dialog select')].some(s =>
                [...s.options].map(o => o.value).join(',') === 'rust,c,cpp,python,java'
                && [...s.options].every(o => !o.disabled))`));

        // Untick .card: only rule 1 compiles.
        await cdp.eval(`(() => { const cb = document.querySelector('.azx-rule input[data-index="0"]'); cb.checked = false;
            cb.dispatchEvent(new Event('change', { bubbles: true })); return true; })()`);
        await waitFor(cdp, `/rust: \\.pill\\n$/.test(__t.code() || '')`);
        check('unticking a rule compiles only the ticked ones', JSON.stringify(lastSent('compile_css').rules) === '[1]'
            && await cdp.eval(`__t.code() === '// rust: .pill\\n'`), lastSent('compile_css'));

        // Another language.
        await cdp.eval(`(() => { const s = [...document.querySelectorAll('.azx-dialog select')].find(s => [...s.options].some(o => o.value === 'java'));
            s.value = 'cpp'; s.dispatchEvent(new Event('change', { bubbles: true })); return true; })()`);
        await waitFor(cdp, `(__t.code() || '').startsWith('// cpp')`);
        check('picking another language recompiles with it', lastSent('compile_css').language === 'cpp');

        // Download and Copy.
        await cdp.eval(`document.querySelector('.azx-download').click(); true`);
        check('Download saves the code under the server\u2019s file name',
            await cdp.eval(`window.__downloads.some(d => d.download === 'styles.hpp' && d.href.startsWith('blob:'))`),
            await cdp.eval(`window.__downloads`));
        await cdp.eval(`document.querySelector('.azx-copy').click(); true`);
        check('Copy reports what happened', await waitFor(cdp, `/clipboard|copy it/.test(__t.status() || '')`),
            await cdp.eval(`__t.status()`));

        // Focus trap: Tab 12 times never leaves the dialog.
        let trapped = true;
        for (let i = 0; i < 12; i++) {
            await cdp.eval(`__t.key('Tab', { shiftKey: ${i % 3 === 0} })`);
            if (!(await cdp.eval(`__t.inDialog()`))) { trapped = false; break; }
        }
        check('Tab and Shift+Tab stay inside the dialog', trapped);

        // Delete inside the dialog must not reach the builder.
        const deletesBefore = countSent('builder_delete');
        await cdp.eval(`azDnd.state.selected = 2; __t.key('Delete'); true`);
        check('a Delete key inside a dialog does not delete a document node', countSent('builder_delete') === deletesBefore);

        // Escape closes; focus back where it was.
        await cdp.eval(`__t.key('Escape')`);
        check('Escape closes the dialog and focus returns to what had it',
            await waitFor(cdp, `!__t.dlg() && document.activeElement === document.getElementById('terminal-cmd')`),
            await cdp.eval(`({ dlg: !!__t.dlg(), active: document.activeElement && document.activeElement.id })`));

        // ── Subtree -> code from the toolbar, node 1 selected ──
        await cdp.eval(`__t.row(1).click(); true`);
        await waitFor(cdp, `azDnd.state.selected === 1`);
        await cdp.eval(`(() => { const b = document.querySelector('#azb-toolbar [data-azx-act="export"]'); b.focus(); b.click(); return true; })()`);
        await waitFor(cdp, `__t.kind() === 'subtree' && /subtree 1 rust function/.test(__t.code() || '')`);
        check('the toolbar button exports the selected subtree as a render function',
            JSON.stringify(lastSent('export_subtree_code')) === JSON.stringify({ op: 'export_subtree_code', node: 1, language: 'rust' }),
            lastSent('export_subtree_code'));
        check('...and names the node it exports',
            await cdp.eval(`document.querySelector('.azx-node').textContent === 'div.card  #1'`),
            await cdp.eval(`document.querySelector('.azx-node') && document.querySelector('.azx-node').textContent`));
        check('...from the same list, the languages without DOM export disabled',
            await cdp.eval(`(() => { const s = [...document.querySelectorAll('.azx-dialog select')].find(s => [...s.options].some(o => o.value === 'java'));
                const j = [...s.options].find(o => o.value === 'java');
                return s.options.length === 5 && j.disabled && /no DOM export/.test(j.textContent)
                    && [...s.options].filter(o => o.disabled).length === 1; })()`));

        await cdp.eval(`(() => { const s = [...document.querySelectorAll('.azx-dialog select')].find(s => [...s.options].some(o => o.value === 'python'));
            s.value = 'c'; s.dispatchEvent(new Event('change', { bubbles: true })); return true; })()`);
        await waitFor(cdp, `/subtree 1 c function/.test(__t.code() || '')`);
        check('picking C regenerates in C', lastSent('export_subtree_code').language === 'c');

        await cdp.eval(`(() => { const i = document.querySelector('.azx-input'); i.value = 'build_card';
            i.dispatchEvent(new Event('input', { bubbles: true })); return true; })()`);
        await waitFor(cdp, `/build_card/.test(__t.code() || '')`);
        check('a function name goes along', lastSent('export_subtree_code').function_name === 'build_card');

        await cdp.eval(`(() => { const s = [...document.querySelectorAll('.azx-dialog select')].find(s => [...s.options].some(o => o.value === 'app'));
            s.value = 'app'; s.dispatchEvent(new Event('change', { bubbles: true })); return true; })()`);
        await waitFor(cdp, `/subtree 1 c app/.test(__t.code() || '')`);
        check('"A runnable app" asks for an app and hides the function name',
            lastSent('export_subtree_code').mode === 'app'
            && await cdp.eval(`document.querySelector('.azx-input').getClientRects().length === 0`));
        check('...shows the project\u2019s main file first and lists every file',
            await cdp.eval(`(() => { const f = document.querySelector('.azx-out-files');
                return !!f && f.getClientRects().length > 0
                    && [...f.options].map(o => o.textContent).join(',') === 'main.c,ui.h,Makefile'; })()`),
            await cdp.eval(`(() => { const f = document.querySelector('.azx-out-files'); return f && [...f.options].map(o => o.textContent); })()`));
        await cdp.eval(`(() => { const f = document.querySelector('.azx-out-files'); f.value = '1';
            f.dispatchEvent(new Event('change', { bubbles: true })); return true; })()`);
        check('...and picking a file shows it (Copy / Download take it)',
            await cdp.eval(`__t.code() === '// ui c\\n'`)
            && await cdp.eval(`(() => { document.querySelector('.azx-download').click();
                return window.__downloads.some(d => d.download === 'ui.h'); })()`),
            await cdp.eval(`__t.code()`));

        await cdp.eval(`__t.key('Escape')`);
        check('Escape returns focus to the toolbar button that opened it',
            await waitFor(cdp, `!__t.dlg() && document.activeElement && document.activeElement.dataset.azxAct === 'export'`));

        // ── the tree's context menu ──
        await cdp.eval(`(() => { const r = __t.row(2).getBoundingClientRect();
            __t.row(2).dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true, clientX: r.left + 20, clientY: r.top + 5 }));
            const item = [...document.querySelectorAll('.azd-context-menu-item')].find(i => i.textContent.includes('Subtree as Component'));
            if (item) item.click();
            return !!item; })()`);
        await waitFor(cdp, `__t.kind() === 'subtree' && /subtree 2 /.test(__t.code() || '')`);
        check('a row\u2019s context menu exports that row\u2019s subtree', lastSent('export_subtree_code').node === 2,
            lastSent('export_subtree_code'));
        await cdp.eval(`__t.key('Escape')`);
        await waitFor(cdp, `!__t.dlg()`);

        // ── Component -> code ──
        await cdp.eval(`__t.menu('component')`);
        await waitFor(cdp, `__t.kind() === 'component' && /component user:my-card/.test(__t.code() || '')`);
        check('"Component \u2192 code" opens on the first user component',
            JSON.stringify(lastSent('export_component_code')) === JSON.stringify({ op: 'export_component_code', library: 'user', name: 'my-card', language: 'c' }),
            lastSent('export_component_code'));
        check('...with the DOM language used last (C, from the subtree dialog)',
            lastSent('export_component_code').language === 'c');
        await cdp.eval(`(() => { const s = document.querySelector('.azx-dialog select'); s.value = 'builtin\\u0000p';
            s.dispatchEvent(new Event('change', { bubbles: true })); return true; })()`);
        await waitFor(cdp, `/component builtin:p/.test(__t.code() || '')`);
        check('...and another component can be picked', lastSent('export_component_code').name === 'p');
        check('...a builtin library cannot be exported as JSON (the button says so)',
            await cdp.eval(`(() => { const b = document.querySelector('.azx-dialog [data-azx-lib-json]'); return !!b && b.disabled; })()`));
        await cdp.eval(`(() => { const s = document.querySelector('.azx-dialog select'); s.value = 'user\\u0000my-card';
            s.dispatchEvent(new Event('change', { bubbles: true })); return true; })()`);
        await waitFor(cdp, `/component user:my-card/.test(__t.code() || '')`);
        await cdp.eval(`window.__downloads = []; const lj = document.querySelector('.azx-dialog [data-azx-lib-json]'); if (lj) lj.click(); true`);
        await waitFor(cdp, `window.__downloads.length > 0`);
        check('Components… also exports the component\'s library as JSON (the old "Component Library (JSON)")',
            JSON.stringify(lastSent('export_component_library')) === JSON.stringify({ op: 'export_component_library', library: 'user' })
            && await cdp.eval(`window.__downloads.some(d => d.download === 'user_components.json')`),
            { sent: lastSent('export_component_library'), dl: await cdp.eval('window.__downloads') });

        // ── HTML -> DOM (code) ──
        await cdp.eval(`__t.key('Escape')`);
        await waitFor(cdp, `!__t.dlg()`);
        await cdp.eval(`document.getElementById('terminal-cmd').focus(); true`);
        await cdp.eval(`__t.menu('html')`);
        await waitFor(cdp, `__t.kind() === 'html' && /html \\w+ function render_card/.test(__t.code() || '')`);
        check('"HTML \u2192 DOM (code)" opens with a paste area and converts what is in it',
            await cdp.eval(`!!document.querySelector('.azx-dialog textarea.azx-html')`)
            && !!lastSent('html_to_code') && lastSent('html_to_code').html.length > 0,
            lastSent('html_to_code'));
        check('...from the ONE language list, the languages without DOM export disabled',
            await cdp.eval(`(() => { const s = [...document.querySelectorAll('.azx-dialog select')].find(s => [...s.options].some(o => o.value === 'java'));
                const j = s && [...s.options].find(o => o.value === 'java');
                return !!j && j.disabled && s.options.length === 5; })()`));
        await cdp.eval(`(() => { const t = document.querySelector('.azx-html'); t.value = '<div class="card"><p>Hi</p></div>';
            t.dispatchEvent(new Event('input', { bubbles: true })); return true; })()`);
        const pasted = await waitSent(() => (lastSent('html_to_code') || {}).html === '<div class="card"><p>Hi</p></div>');
        check('pasting converts the new markup (debounced)', pasted, lastSent('html_to_code'));
        await cdp.eval(`(() => { const s = [...document.querySelectorAll('.azx-dialog select')].find(s => [...s.options].some(o => o.value === 'java'));
            s.value = 'python'; s.dispatchEvent(new Event('change', { bubbles: true })); return true; })()`);
        await waitFor(cdp, `/html python function/.test(__t.code() || '')`);
        check('picking another language converts with it', lastSent('html_to_code').language === 'python');
        await cdp.eval(`(() => { const i = document.querySelector('.azx-dialog .azx-input'); i.value = 'build_card';
            i.dispatchEvent(new Event('input', { bubbles: true })); return true; })()`);
        await waitFor(cdp, `/build_card/.test(__t.code() || '')`);
        check('a function name goes along', lastSent('html_to_code').function_name === 'build_card');
        await cdp.eval(`(() => { const c = document.querySelector('.azx-dialog input[type=checkbox]'); c.checked = true;
            c.dispatchEvent(new Event('change', { bubbles: true })); return true; })()`);
        await waitFor(cdp, `/ css/.test(__t.code() || '')`);
        check('"with its CSS" asks for the styles and lists both files',
            lastSent('html_to_code').css === true
            && await cdp.eval(`(() => { const f = document.querySelector('.azx-out-files');
                return !!f && f.getClientRects().length > 0 && [...f.options].map(o => o.textContent).join(',') === 'build_card.py,styles.py'; })()`),
            await cdp.eval(`(() => { const f = document.querySelector('.azx-out-files'); return f && [...f.options].map(o => o.textContent); })()`));
        await cdp.eval(`(() => { const s = [...document.querySelectorAll('.azx-dialog select')].find(s => [...s.options].some(o => o.value === 'app'));
            s.value = 'app'; s.dispatchEvent(new Event('change', { bubbles: true })); return true; })()`);
        await waitFor(cdp, `/html python app/.test(__t.code() || '')`);
        check('"A runnable app" answers a project with its main file first and hides the function name',
            lastSent('html_to_code').mode === 'app' && lastSent('html_to_code').function_name === undefined
            && await cdp.eval(`document.querySelector('.azx-dialog .azx-input').getClientRects().length === 0`)
            && await cdp.eval(`[...document.querySelector('.azx-out-files').options].map(o => o.textContent)[0] === 'main.py'`),
            lastSent('html_to_code'));
        await cdp.eval(`(() => { const t = document.querySelector('.azx-html'); t.value = '<div>\\n<p class=big>x</p></div>';
            t.dispatchEvent(new Event('input', { bubbles: true })); return true; })()`);
        await waitFor(cdp, `/line 2, column 10/.test(__t.status() || '')`);
        check('markup that does not parse shows its line and column, and no code',
            await cdp.eval(`/line 2, column 10: Invalid attribute/.test(__t.status())
                && document.querySelector('.azx-status').classList.contains('azx-error') && !__t.code()`),
            await cdp.eval(`__t.status()`));
        await cdp.eval(`__t.key('Escape')`);
        check('Escape closes it and focus returns to what had it',
            await waitFor(cdp, `!__t.dlg() && document.activeElement === document.getElementById('terminal-cmd')`));

        // A refusal shows as an error in the dialog, not as a crash.
        await cdp.eval(`__t.key('Escape')`);
        await waitFor(cdp, `!__t.dlg()`);
        mock.doc = newDoc();
        await cdp.eval(`azDnd.state.selected = 7; azExport.openSubtreeDialog(null, 7); true`);
        await waitFor(cdp, `__t.kind() === 'subtree' && /uid 7/.test(__t.status() || '')`);
        check('a server refusal is shown in the dialog', await cdp.eval(`/uid 7/.test(__t.status())
            && document.querySelector('.azx-status').classList.contains('azx-error')`));
        const shot = argVal('--screenshot');
        await cdp.eval(`__t.key('Escape')`);

        // ── Export > Code > Rust ──
        await cdp.eval(`window.__downloads = []; app.handlers.exportCode('rust'); true`);
        await waitFor(cdp, `window.__downloads.length > 0`);
        check('Export > Code downloads the zip the server answers as a data URI',
            await cdp.eval(`window.__downloads.some(d => d.download === 'azul-export-rust.zip' && d.href.startsWith('data:application/zip'))`)
            && lastSent('export_code_zip').language === 'rust',
            await cdp.eval(`window.__downloads`));

        if (shot) {
            await cdp.eval(`__t.menu('css'); true`);
            await waitFor(cdp, `__t.kind() === 'css' && !!__t.code()`);
            fs.writeFileSync(shot, await cdp.screenshot());
            console.log('     screenshot: ' + shot);
            await cdp.eval(`__t.key('Escape')`);
        }

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
