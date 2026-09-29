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
//   Download), "Subtree -> code" from the Document toolbar and from a row's
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
        { name: 'builtin', modifiable: false, components: [
            { tag: 'div', display_name: 'Div' },
            { tag: 'p', display_name: 'Paragraph' },
            { tag: 'span', display_name: 'Span' },
        ] },
        { name: 'user', modifiable: true, components: [
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
            return {
                dom: [{ id: 'rust', label: 'Rust', ext: 'rs' }, { id: 'c', label: 'C', ext: 'c' },
                      { id: 'cpp', label: 'C++', ext: 'cpp' }, { id: 'python', label: 'Python', ext: 'py' }],
                css: [{ id: 'rust', label: 'Rust', ext: 'rs' }, { id: 'cpp', label: 'C++', ext: 'cpp' }],
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
            if (msg.language === 'cobol') throw new Error('no CSS code generator for "cobol"');
            return {
                language: msg.language, file_name: 'styles.' + (msg.language === 'cpp' ? 'cpp' : 'rs'),
                code: `// ${msg.language}: ${picked.map((r) => r.selector).join(' ')}\n`,
                warnings: [], rule_count: picked.length,
            };
        }
        case 'export_subtree_code': {
            if (msg.node !== 0 && !(mock.doc && find(mock.doc.root, msg.node))) {
                throw new Error(`no node with uid ${msg.node} in the builder document`);
            }
            const ext = { rust: 'rs', c: 'c', cpp: 'cpp', python: 'py' }[msg.language];
            const name = msg.function_name || 'render_x';
            return {
                language: msg.language,
                file_name: msg.mode === 'app' ? 'main.' + ext : name + '.' + ext,
                code: `// subtree ${msg.node} ${msg.language} ${msg.mode || 'function'} ${name}\n`,
                warnings: [],
            };
        }
        case 'export_component_code':
            return {
                language: msg.language, file_name: `${msg.library}_${msg.name}.rs`,
                code: `// component ${msg.library}:${msg.name} ${msg.language}\n`, warnings: [],
            };
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

        check('the Export menu offers the three quick exports',
            await cdp.eval(`['css','subtree','component'].every(a => !!document.querySelector('.menu-dropdown-item[data-azx="' + a + '"]'))`));
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
        check('...the CSS languages are the server\u2019s CSS code generators',
            await cdp.eval(`[...document.querySelectorAll('.azx-dialog select')].some(s =>
                [...s.options].map(o => o.value).join(',') === 'rust,cpp')`));

        // Untick .card: only rule 1 compiles.
        await cdp.eval(`(() => { const cb = document.querySelector('.azx-rule input[data-index="0"]'); cb.checked = false;
            cb.dispatchEvent(new Event('change', { bubbles: true })); return true; })()`);
        await waitFor(cdp, `/rust: \\.pill\\n$/.test(__t.code() || '')`);
        check('unticking a rule compiles only the ticked ones', JSON.stringify(lastSent('compile_css').rules) === '[1]'
            && await cdp.eval(`__t.code() === '// rust: .pill\\n'`), lastSent('compile_css'));

        // Another language.
        await cdp.eval(`(() => { const s = [...document.querySelectorAll('.azx-dialog select')].find(s => [...s.options].some(o => o.value === 'cpp') && s.options.length === 2);
            s.value = 'cpp'; s.dispatchEvent(new Event('change', { bubbles: true })); return true; })()`);
        await waitFor(cdp, `(__t.code() || '').startsWith('// cpp')`);
        check('picking another language recompiles with it', lastSent('compile_css').language === 'cpp');

        // Download and Copy.
        await cdp.eval(`document.querySelector('.azx-download').click(); true`);
        check('Download saves the code under the server\u2019s file name',
            await cdp.eval(`window.__downloads.some(d => d.download === 'styles.cpp' && d.href.startsWith('blob:'))`),
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

        await cdp.eval(`__t.key('Escape')`);
        check('Escape returns focus to the toolbar button that opened it',
            await waitFor(cdp, `!__t.dlg() && document.activeElement && document.activeElement.dataset.azxAct === 'export'`));

        // ── the tree's context menu ──
        await cdp.eval(`(() => { const r = __t.row(2).getBoundingClientRect();
            __t.row(2).dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true, clientX: r.left + 20, clientY: r.top + 5 }));
            const item = [...document.querySelectorAll('.azd-context-menu-item')].find(i => i.textContent.includes('Export as code'));
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
