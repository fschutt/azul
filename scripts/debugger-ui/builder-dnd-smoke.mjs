// Headless UI test for AzBuilder's drag and drop (debugger-dnd.js).
//
//     node scripts/debugger-ui/builder-dnd-smoke.mjs [--chrome <path>] [--keep]
//
// Serves the REAL debugger page (dll/src/desktop/shell2/common/debugger/) from a
// small HTTP server whose `POST /` is a MOCK of the debug server's builder ops,
// starts a headless Chrome of its own (own profile, no window, no real input),
// and drives the page with synthetic DragEvents / key events:
//
//   palette card -> tree row (into / before / after), a refused drop (<div> into
//   <p>), row -> row (move), a move into its own subtree (refused), Delete,
//   Ctrl+Z, right-click "Convert to component…", the new component back from the
//   palette, the Document / Live DOM switch.
//
// It checks what the page SENT (the server messages) and what it RENDERED from
// the answers. The mock follows builder.rs closely enough for the page; the
// server itself is tested by
//     cargo test -p azul-layout --features e2e-server --lib builder
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

// ── the mock debug server ────────────────────────────────────────────────

const clone = (v) => JSON.parse(JSON.stringify(v));
const DEFAULT_TEXT = { p: 'Paragraph text', h1: 'Heading 1', button: 'Button' };
const AUTO_CLOSE_P = ['div', 'p', 'h1', 'ul', 'ol', 'table', 'section', 'header', 'footer'];

const mock = {
    sent: [],
    registry: { libraries: [{ name: 'builtin', modifiable: false, components: [
        { tag: 'html', display_name: 'HTML' },
        { tag: 'div', display_name: 'Div' },
        { tag: 'p', display_name: 'Paragraph' },
        { tag: 'span', display_name: 'Span' },
        { tag: 'h1', display_name: 'Heading 1' },
        { tag: 'button', display_name: 'Button' },
    ] }] },
    doc: null,
};

function newDoc() {
    return { root: { uid: 0, kind: 'element', tag: 'body', attrs: {}, children: [] }, next: 1, undo: [], redo: [] };
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
const isContainer = (n) => n && n.kind === 'element' && !['br', 'hr', 'img', 'input'].includes(n.tag);
function docJson(d, active) {
    return { active, can_undo: d.undo.length > 0, can_redo: d.redo.length > 0, root: clone(d.root) };
}

function builderOp(msg) {
    const d = mock.doc || newDoc();
    const checkpoint = () => { d.undo.push(clone(d.root)); d.redo = []; };
    const commit = (extra) => { mock.doc = d; return Object.assign(docJson(d, true), extra || {}); };
    switch (msg.op) {
        case 'builder_get_document': return docJson(d, !!mock.doc);
        case 'builder_insert': {
            const parent = find(d.root, msg.parent);
            if (!parent) throw new Error(`no node with uid ${msg.parent} in the builder document`);
            if (!isContainer(parent)) throw new Error(`node ${msg.parent} cannot have children`);
            const lib = msg.library || 'builtin';
            if (lib === 'builtin' && parent.tag === 'p' && AUTO_CLOSE_P.includes(msg.component)) {
                throw new Error(`a <${msg.component}> cannot go inside a <p>`);
            }
            const attrs = Object.assign({}, msg.attrs || {});
            let node;
            if (msg.component === '#text') {
                node = { uid: 0, kind: 'text', tag: '#text', text: attrs.text || '', attrs: {}, children: [] };
            } else if (lib === 'builtin') {
                if (attrs.text == null && DEFAULT_TEXT[msg.component]) attrs.text = DEFAULT_TEXT[msg.component];
                node = { uid: 0, kind: 'element', tag: msg.component, attrs, children: [] };
            } else {
                node = { uid: 0, kind: 'component', library: lib, tag: msg.component, attrs, children: [] };
            }
            const at = msg.index == null ? parent.children.length : msg.index;
            if (at > parent.children.length) throw new Error(`index ${at} is past the end`);
            checkpoint();
            node.uid = d.next++;
            parent.children.splice(at, 0, node);
            return commit({ inserted: node.uid });
        }
        case 'builder_move': {
            if (msg.node === 0) throw new Error('the document root cannot be moved');
            const node = find(d.root, msg.node);
            const target = find(d.root, msg.parent);
            if (!node || !target) throw new Error('no such node');
            if (find(node, msg.parent)) throw new Error(`cannot move node ${msg.node} into its own descendant`);
            if (!isContainer(target)) throw new Error(`node ${msg.parent} cannot have children`);
            checkpoint();
            const from = parentOf(d.root, msg.node);
            from.parent.children.splice(from.index, 1);
            let at = msg.index == null ? target.children.length : msg.index;
            if (from.parent.uid === msg.parent && msg.index != null && from.index < at) at -= 1;
            target.children.splice(Math.min(at, target.children.length), 0, node);
            return commit();
        }
        case 'builder_delete': {
            if (msg.node === 0) throw new Error('the document root cannot be deleted');
            const from = parentOf(d.root, msg.node);
            if (!from) throw new Error(`no node with uid ${msg.node}`);
            checkpoint();
            from.parent.children.splice(from.index, 1);
            return commit();
        }
        case 'builder_set_attribute': {
            const node = find(d.root, msg.node);
            if (!node) throw new Error(`no node with uid ${msg.node}`);
            checkpoint();
            if (node.kind === 'text') node.text = msg.value || '';
            else if (msg.value == null) delete node.attrs[msg.name];
            else node.attrs[msg.name] = msg.value;
            return commit();
        }
        case 'builder_undo':
        case 'builder_redo': {
            const [from, to] = msg.op === 'builder_undo' ? [d.undo, d.redo] : [d.redo, d.undo];
            if (!from.length) throw new Error(msg.op === 'builder_undo' ? 'nothing to undo' : 'nothing to redo');
            to.push(clone(d.root));
            d.root = from.pop();
            return commit();
        }
        case 'builder_reset': mock.doc = null; return { active: false };
        case 'builder_convert_to_component': {
            const node = find(d.root, msg.node);
            if (!node || node.kind !== 'element') throw new Error('not an element');
            let lib = mock.registry.libraries.find((l) => l.name === msg.library);
            if (!lib) { lib = { name: msg.library, modifiable: true, components: [] }; mock.registry.libraries.push(lib); }
            if (lib.components.some((c) => c.tag === msg.name)) throw new Error('already exists');
            lib.components.push({ tag: msg.name, display_name: msg.name, description: 'Converted' });
            checkpoint();
            node.kind = 'component';
            node.library = msg.library;
            node.tag = msg.name;
            node.attrs = {};
            node.children = [];
            return commit({ component: { library: msg.library, name: msg.name, fields: [{ name: 'text', default: 'x' }] } });
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

async function startChrome() {
    const port = 9400 + Math.floor(Math.random() * 400);
    const profile = fs.mkdtempSync(path.join(process.env.AZB_TMP || os.tmpdir(), 'azb-chrome-'));
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
window.__t = {
  row(uid) { return document.querySelector('.azb-row[data-uid="' + uid + '"]'); },
  card(key) { return [...document.querySelectorAll('.azb-card')].find(c => c.dataset.key === key); },
  rows() { return [...document.querySelectorAll('#dom-tree-container .azb-row')].map(r => Number(r.dataset.uid)); },
  drag(src, dst, rel) {
    const dt = new DataTransfer();
    const r = dst.getBoundingClientRect();
    const o = { bubbles: true, cancelable: true, dataTransfer: dt, clientX: r.left + 12, clientY: r.top + r.height * rel };
    src.dispatchEvent(new DragEvent('dragstart', { bubbles: true, cancelable: true, dataTransfer: dt }));
    dst.dispatchEvent(new DragEvent('dragenter', o));
    const over = new DragEvent('dragover', o);
    dst.dispatchEvent(over);
    const indicator = ['before', 'after', 'into'].find(z => dst.classList.contains('azb-drop-' + z)) || null;
    const accepted = over.defaultPrevented;
    if (accepted) dst.dispatchEvent(new DragEvent('drop', o));
    src.dispatchEvent(new DragEvent('dragend', { bubbles: true, cancelable: true, dataTransfer: dt }));
    return { indicator, accepted };
  },
  key(key, mods) {
    document.body.dispatchEvent(new KeyboardEvent('keydown', Object.assign({ key, bubbles: true, cancelable: true }, mods || {})));
  },
  childUids(uid) {
    const n = azDnd.logic.findNode(azDnd.state.doc.root, uid);
    return n ? n.children.map(c => c.uid) : null;
  },
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
        const ready = await waitFor(cdp, `typeof app !== 'undefined' && !!window.azDnd
            && azDnd.state.mode === 'document' && document.querySelectorAll('.azb-card').length > 0
            && !!document.querySelector('.azb-row[data-uid="0"]')`, 10000);
        check('the page loads debugger-dnd.js and opens AzBuilder in the Document view', ready,
            await cdp.eval(`({ mode: window.azDnd && azDnd.state.mode, cards: document.querySelectorAll('.azb-card').length })`).catch(e => String(e)));
        if (!ready) throw new Error('page not ready');
        await cdp.eval(HELPERS);
        const layout = await cdp.eval(`(() => {
            const rect = (el) => { if (!el) return null; const r = el.getBoundingClientRect(); return [Math.round(r.top), Math.round(r.height)]; };
            const chain = []; let n = __t.row(0);
            while (n && n !== document.documentElement) { chain.push((n.id || n.className || n.tagName) + ':' + getComputedStyle(n).display + ':' + Math.round(n.getBoundingClientRect().height)); n = n.parentElement; }
            return { chain, vh: innerHeight, row0: rect(__t.row(0)), tree: rect(document.getElementById('dom-tree-container')),
                     palette: rect(document.getElementById('palette-component-list')), card: rect(document.querySelector('.azb-card')) };
        })()`);
        check('the tree rows and the palette are laid out (drop zones need real row heights)',
            layout.row0 && layout.row0[1] >= 16 && layout.card && layout.card[1] > 0, layout);

        check('the palette leaves out non-visual builtins (<html>)',
            await cdp.eval(`!__t.card('builtin:html') && !!__t.card('builtin:p')`));
        check('palette thumbnails are requested from the native renderer and shown',
            await waitFor(cdp, `document.querySelectorAll('.azb-card .azb-thumb img').length >= 3`)
            && countSent('get_component_thumbnail') >= 3, countSent('get_component_thumbnail'));

        // 1. card -> the <body> row: INTO.
        let r = await cdp.eval(`__t.drag(__t.card('builtin:p'), __t.row(0), 0.5)`);
        check('dropping a card on <body> shows the INTO indicator and is accepted', r.indicator === 'into' && r.accepted, r);
        await waitFor(cdp, `__t.rows().length === 2`);
        check('...and sends builder_insert {parent: 0, component: "p"} (append)',
            JSON.stringify(lastSent('builder_insert')) === JSON.stringify({ op: 'builder_insert', parent: 0, component: 'p' }),
            lastSent('builder_insert'));
        check('...and the tree renders the answer with the new row selected',
            await cdp.eval(`JSON.stringify(__t.rows()) === '[0,1]' && __t.row(1).classList.contains('selected')
                && __t.row(1).textContent.includes('Paragraph text')`));

        // 2. card -> top quarter of the <p> row: BEFORE.
        r = await cdp.eval(`__t.drag(__t.card('builtin:div'), __t.row(1), 0.1)`);
        check('the top quarter of a row is BEFORE', r.indicator === 'before' && r.accepted, r);
        await waitFor(cdp, `__t.rows().length === 3`);
        check('...and inserts at that slot (index 0)',
            JSON.stringify(lastSent('builder_insert')) === JSON.stringify({ op: 'builder_insert', parent: 0, component: 'div', index: 0 })
            && await cdp.eval(`JSON.stringify(__t.rows()) === '[0,2,1]'`), lastSent('builder_insert'));

        // 3. A <div> into a <p> is refused before anything is sent.
        const before = mock.sent.length;
        r = await cdp.eval(`__t.drag(__t.card('builtin:div'), __t.row(1), 0.5)`);
        check('a <div> INTO a <p> is refused (no indicator, no message)',
            !r.accepted && r.indicator === null && mock.sent.length === before, r);

        // 4. A <span> into the <p> is fine.
        r = await cdp.eval(`__t.drag(__t.card('builtin:span'), __t.row(1), 0.5)`);
        await waitFor(cdp, `__t.rows().length === 4`);
        check('a <span> INTO the <p> inserts as its child',
            r.indicator === 'into' && JSON.stringify(lastSent('builder_insert')) === JSON.stringify({ op: 'builder_insert', parent: 1, component: 'span' }),
            lastSent('builder_insert'));

        // 5. Drag the <p> row into the <div> row: builder_move.
        r = await cdp.eval(`__t.drag(__t.row(1), __t.row(2), 0.5)`);
        await waitFor(cdp, `JSON.stringify(__t.childUids(2)) === '[1]'`);
        check('dragging a row INTO another sends builder_move {node, parent}',
            r.indicator === 'into' && JSON.stringify(lastSent('builder_move')) === JSON.stringify({ op: 'builder_move', node: 1, parent: 2 }),
            lastSent('builder_move'));
        check('...and the tree shows it nested', await cdp.eval(`JSON.stringify(__t.childUids(2)) === '[1]'
            && JSON.stringify(__t.rows()) === '[0,2,1,3]'`), await cdp.eval('__t.rows()'));

        // 6. The <div> into its own grandchild: refused.
        const before2 = mock.sent.length;
        r = await cdp.eval(`__t.drag(__t.row(2), __t.row(3), 0.5)`);
        check('a row cannot be dropped into its own subtree', !r.accepted && mock.sent.length === before2, r);

        // 7. Delete key on the selected <span>.
        await cdp.eval(`__t.row(3).click(); true`);
        await cdp.eval(`__t.key('Delete'); true`);
        await waitFor(cdp, `!__t.row(3)`);
        check('Delete removes the selected node (builder_delete) and selects its parent',
            JSON.stringify(lastSent('builder_delete')) === JSON.stringify({ op: 'builder_delete', node: 3 })
            && await cdp.eval(`azDnd.state.selected === 1`), lastSent('builder_delete'));

        // 8. Ctrl+Z brings it back.
        await cdp.eval(`__t.key('z', { ctrlKey: true }); true`);
        await waitFor(cdp, `!!__t.row(3)`);
        check('Ctrl+Z undoes it (builder_undo)', countSent('builder_undo') === 1 && await cdp.eval(`!!__t.row(3)`));
        check('the toolbar tracks undo / redo',
            await cdp.eval(`!document.querySelector('#azb-toolbar [data-act=redo]').disabled
                && !document.querySelector('#azb-toolbar [data-act=undo]').disabled`));

        // 9. Right-click the <div> -> Convert to component…
        await cdp.eval(`window.prompt = () => 'my-card'; true`);
        await cdp.eval(`(() => { const r = __t.row(2).getBoundingClientRect();
            __t.row(2).dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true, clientX: r.left + 20, clientY: r.top + 5 }));
            const item = [...document.querySelectorAll('.azd-context-menu-item')].find(i => i.textContent.includes('Convert to component'));
            if (item) item.click();
            return !!item; })()`);
        await waitFor(cdp, `!!__t.card('user:my-card')`);
        const conv = lastSent('builder_convert_to_component');
        check('"Convert to component…" sends builder_convert_to_component for the subtree',
            conv && conv.node === 2 && conv.library === 'user' && conv.name === 'my-card', conv);
        check('...the tree shows the instance and the palette the new component',
            await cdp.eval(`__t.row(2).querySelector('.tree-component-badge').textContent === 'user:my-card'
                && !!__t.card('user:my-card')`));

        // 10. ...and it drops again.
        r = await cdp.eval(`__t.drag(__t.card('user:my-card'), __t.row(0), 0.5)`);
        await waitFor(cdp, `__t.rows().length === 3`);
        check('the new component drops from the palette (library named in the message)',
            JSON.stringify(lastSent('builder_insert')) === JSON.stringify({ op: 'builder_insert', parent: 0, component: 'my-card', library: 'user' }),
            lastSent('builder_insert'));

        // 11. The bottom half of a leaf row (an instance) is AFTER; within the
        //     same parent the slot is counted before the move.
        r = await cdp.eval(`__t.drag(__t.row(2), __t.row(4), 0.9)`);
        await waitFor(cdp, `JSON.stringify(__t.childUids(0)) === '[4,2]'`);
        check('the bottom half of a leaf row is AFTER, and the node lands there',
            r.indicator === 'after'
            && JSON.stringify(lastSent('builder_move')) === JSON.stringify({ op: 'builder_move', node: 2, parent: 0, index: 2 })
            && await cdp.eval(`JSON.stringify(__t.childUids(0)) === '[4,2]'`), { r, sent: lastSent('builder_move') });

        // 12. Double-click a card: insert after the selected leaf.
        await cdp.eval(`__t.card('builtin:h1').dispatchEvent(new MouseEvent('dblclick', { bubbles: true })); true`);
        await waitFor(cdp, `__t.rows().length === 4`);
        check('double-clicking a card inserts it after the selected leaf',
            JSON.stringify(lastSent('builder_insert')) === JSON.stringify({ op: 'builder_insert', parent: 0, component: 'h1', index: 2 }),
            lastSent('builder_insert'));

        // `--screenshot <file.png>`: the page mid-drag, a BEFORE indicator showing.
        const shot = argVal('--screenshot');
        if (shot) {
            await cdp.eval(`(() => {
                const dt = new DataTransfer(); const dst = __t.row(2); const r = dst.getBoundingClientRect();
                __t.card('builtin:button').dispatchEvent(new DragEvent('dragstart', { bubbles: true, cancelable: true, dataTransfer: dt }));
                dst.dispatchEvent(new DragEvent('dragover', { bubbles: true, cancelable: true, dataTransfer: dt, clientX: r.left + 12, clientY: r.top + 2 }));
                return true; })()`);
            fs.writeFileSync(shot, await cdp.screenshot());
            await cdp.eval(`__t.card('builtin:button').dispatchEvent(new DragEvent('dragend', { bubbles: true })); true`);
            console.log('     screenshot: ' + shot);
        }

        // 13. Live DOM and back.
        await cdp.eval(`document.querySelector('.azb-seg button[data-mode=live]').click(); true`);
        const live = await waitFor(cdp, `document.querySelectorAll('#dom-tree-container .tree-row').length > 0
            && document.querySelectorAll('#dom-tree-container .azb-row').length === 0`);
        await cdp.eval(`document.querySelector('.azb-seg button[data-mode=document]').click(); true`);
        const back = await waitFor(cdp, `document.querySelectorAll('#dom-tree-container .azb-row').length === 4`);
        check('the Live DOM / Document switch swaps the tree and comes back', live && back);

        check('no page exceptions', cdp.exceptions.length === 0, cdp.exceptions);
        const errors = cdp.console.filter((c) => c.kind === 'error');
        check('no console errors', errors.length === 0, errors);
    } finally {
        if (cdp) cdp.close();
        chrome.proc.kill();
        server.close();
        if (!KEEP) { try { fs.rmSync(chrome.profile, { recursive: true, force: true }); } catch { /* ignore */ } }
    }
    console.log(`\n${passed} passed, ${failures.length} failed`);
    process.exit(failures.length ? 1 : 0);
}

main().catch((e) => { console.error(e); process.exit(2); });
