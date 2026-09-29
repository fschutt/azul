// Shared plumbing for the headless debugger-page smoke tests in scripts/debugger-ui/:
// serve the REAL page (dll/src/desktop/shell2/common/debugger/) with a mocked
// `POST /`, start a headless Chrome of its own, count checks.
//
// builder-extras-smoke.mjs uses it. builder-dnd-smoke.mjs, builder-project-smoke.mjs
// and builder-export-smoke.mjs still carry their own copies of these helpers
// (startServer / startChrome / stopChrome / check / waitFor / iconFont) and can
// move onto this module without changing a check.

import http from 'node:http';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import zlib from 'node:zlib';
import { spawn } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { Cdp, newTab } from '../../e2e-web/lib/cdp.mjs';

const HERE = path.dirname(fileURLToPath(import.meta.url));
export const ROOT = path.resolve(HERE, '../../..');
export const ASSETS = path.join(ROOT, 'dll/src/desktop/shell2/common/debugger');

export const args = process.argv.slice(2);
export const argVal = (name) => { const i = args.indexOf(name); return i >= 0 ? args[i + 1] : null; };
export const CHROME = argVal('--chrome') || process.env.CHROME
    || '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';
export const KEEP = args.includes('--keep');

export const clone = (v) => JSON.parse(JSON.stringify(v));

/** The icon font the real server embeds (`azul-doc codegen all` output), if built. */
function iconFont() {
    for (const dir of [ROOT, process.env.AZUL_ROOT].filter(Boolean)) {
        const br = path.join(dir, 'target/codegen/material_icons.ttf.br');
        if (fs.existsSync(br)) return zlib.brotliDecompressSync(fs.readFileSync(br));
    }
    return null;
}

/**
 * Serve the debugger page; `POST /` answers `handle(msg)`: a value is sent as
 * `{status: 'ok', data: {type: 'json', value}}`, `null` as a bare ok, a throw as
 * `{status: 'error', message}` (what the real server's `send_err` answers).
 */
export function serveDebugger(handle) {
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
                const value = handle(JSON.parse(body));
                out = { status: 'ok', request_id: 1 };
                if (value !== null && value !== undefined) out.data = { type: 'json', value };
            } catch (e) {
                out = { status: 'error', message: e.message };
            }
            res.writeHead(200, { 'Content-Type': 'application/json' });
            res.end(JSON.stringify(out));
        });
    });
    return new Promise((resolve) => server.listen(0, '127.0.0.1', () => resolve(server)));
}

/** SIGTERM, wait for the exit, SIGKILL after 3 s: never leave a headless Chrome behind. */
export async function stopChrome(chrome) {
    const proc = chrome.proc;
    if (proc.exitCode === null && proc.signalCode === null) {
        const exited = new Promise((resolve) => proc.once('exit', resolve));
        proc.kill('SIGTERM');
        const hard = setTimeout(() => { try { proc.kill('SIGKILL'); } catch { /* gone */ } }, 3000);
        await exited;
        clearTimeout(hard);
    }
    if (!KEEP) { try { fs.rmSync(chrome.profile, { recursive: true, force: true }); } catch { /* ignore */ } }
}

/** A headless Chrome with its own profile (under $AZB_TMP or the OS temp dir). */
export async function startChrome() {
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

/** A tab on `url`, with console and exception capture. */
export async function openPage(chrome, url) {
    const tab = await newTab(chrome.base, 'about:blank');
    const cdp = new Cdp(tab.webSocketDebuggerUrl);
    await cdp.connect();
    await cdp.send('Runtime.enable');
    await cdp.send('Page.enable');
    await cdp.send('Page.navigate', { url });
    return cdp;
}

export async function waitFor(cdp, expr, timeoutMs = 5000) {
    const t0 = Date.now();
    while (Date.now() - t0 < timeoutMs) {
        if (await cdp.eval(expr)) return true;
        await new Promise((r) => setTimeout(r, 50));
    }
    return false;
}

// ── a mock of the builder document ops (layout/src/e2e/builder.rs) ─────────

const AUTO_CLOSE_P = ['div', 'p', 'h1', 'ul', 'ol', 'table', 'section', 'header', 'footer'];
const DEFAULT_TEXT = { p: 'Paragraph text', h1: 'Heading 1', button: 'Button' };

export function findNode(n, uid) {
    if (n.uid === uid) return n;
    for (const c of n.children) { const f = findNode(c, uid); if (f) return f; }
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

/**
 * The builder document as builder.rs keeps it, closely enough for the page:
 * `ops(msg)` answers the `builder_*` ops (undefined for any other op), with
 * snapshot undo, uids that are never reused and the refusals the page shows.
 */
export function builderMock(registry) {
    const m = { doc: null, registry };
    const newDoc = () => ({ root: { uid: 0, kind: 'element', tag: 'body', attrs: {}, children: [] },
        stylesheet: '', next: 1, undo: [], redo: [] });
    const docJson = (d, active) => ({ active, can_undo: d.undo.length > 0, can_redo: d.redo.length > 0,
        root: clone(d.root), stylesheet: d.stylesheet });
    m.ops = function (msg) {
        const d = m.doc || newDoc();
        // An undo step is the tree AND the stylesheet (builder.rs `Snapshot`).
        const snap = () => ({ root: clone(d.root), stylesheet: d.stylesheet });
        const checkpoint = () => { d.undo.push(snap()); d.redo = []; };
        const commit = (extra) => { m.doc = d; return Object.assign(docJson(d, true), extra || {}); };
        switch (msg.op) {
            case 'builder_get_document': return docJson(d, !!m.doc);
            case 'builder_insert': {
                const parent = findNode(d.root, msg.parent);
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
                const node = findNode(d.root, msg.node);
                const target = findNode(d.root, msg.parent);
                if (!node || !target) throw new Error('no such node');
                if (findNode(node, msg.parent)) throw new Error(`cannot move node ${msg.node} into its own descendant`);
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
                const node = findNode(d.root, msg.node);
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
                to.push(snap());
                const s = from.pop();
                d.root = s.root;
                d.stylesheet = s.stylesheet;
                return commit();
            }
            case 'builder_hit_test': {
                // The mock window lays <body>'s children out as 40px rows
                // across a 400px wide window; below them nothing is hit.
                const i = Math.floor(msg.y / 40);
                const kid = d.root.children[i];
                if (!m.doc || !kid || msg.y < 0) return { hit: false, x: msg.x, y: msg.y, uid: null };
                return { hit: true, x: msg.x, y: msg.y, uid: kid.uid, node: i + 2,
                    rect: { x: 0, y: i * 40, width: 400, height: 40 },
                    rel_x: msg.x / 400, rel_y: (msg.y - i * 40) / 40 };
            }
            case 'builder_get_stylesheet':
                return { active: !!m.doc, stylesheet: d.stylesheet, css: d.stylesheet, rules: [], warnings: [] };
            case 'builder_set_stylesheet': {
                if (msg.css !== d.stylesheet) { checkpoint(); d.stylesheet = msg.css; }
                return commit({ warnings: /\{[^}]*$/.test(msg.css) ? ['unclosed block'] : [] });
            }
            case 'builder_reset': m.doc = null; return { active: false };
            default: return undefined;
        }
    };
    return m;
}

/** `check(name, cond, detail)` prints ok / FAIL; `finish()` prints the tally and exits. */
export function checks() {
    let passed = 0;
    const failures = [];
    return {
        check(name, cond, detail) {
            if (cond) { passed++; console.log('ok   ' + name); }
            else { failures.push(name); console.log('FAIL ' + name + (detail !== undefined ? '\n     ' + JSON.stringify(detail) : '')); }
        },
        finish() {
            console.log(`\n${passed} passed, ${failures.length} failed`);
            process.exit(failures.length ? 1 : 0);
        },
    };
}
