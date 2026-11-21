// AzBuilder drag and drop against the REAL debug server: no mock.
//
//     node scripts/debugger-ui/builder-dnd-live.mjs [--app <path>] [--lib <dir>] [--chrome <path>] [--keep]
//
// Starts AzBuilder windowless with its debug server (`AZ_BACKEND=headless
// AZ_DEBUG=<port>`), opens the builder page it serves in a headless Chrome of
// its own, and drags palette cards and tree rows with synthetic DragEvents -
// the same gestures as builder-dnd-smoke.mjs. Every step is checked twice:
// what the page renders, and what the SERVER's document says
// (`builder_get_document`, asked directly), plus that the app's live DOM
// (`get_node_hierarchy`) shows the inserted elements.
//
// Defaults: --app target/release/AzBuilder, --lib target/azul-lib (the dylib
// the app links). Build them with
//     cargo build --release -p azul-dll --features build-dll   (then stage the dylib)
//     AZ_LINK_PATH=$PWD/target/azul-lib cargo build --release -p AzBuilder
// Needs node >= 21 and a Chrome / Chromium / Edge binary. Exit code 0 = all passed.

import path from 'node:path';
import fs from 'node:fs';
import { spawn } from 'node:child_process';
import { ROOT, argVal, KEEP, startChrome, stopChrome, openPage, waitFor } from './lib/smoke.mjs';

const APP = path.resolve(argVal('--app') || path.join(ROOT, 'target/release/AzBuilder'));
const LIB = path.resolve(argVal('--lib') || path.join(ROOT, 'target/azul-lib'));
const PORT = 18000 + Math.floor(Math.random() * 2000);
const BASE = `http://127.0.0.1:${PORT}`;

let passed = 0;
const failures = [];
function check(name, cond, detail) {
    if (cond) { passed++; console.log('ok   ' + name); }
    else { failures.push(name); console.log('FAIL ' + name + (detail !== undefined ? '\n     ' + JSON.stringify(detail).slice(0, 600) : '')); }
}

/** One op straight to the app's debug server, unwrapped like the page's `call`. */
async function op(msg) {
    const res = await fetch(BASE + '/', { method: 'POST', body: JSON.stringify(msg) });
    const json = await res.json();
    if (json.status !== 'ok') throw new Error(`${msg.op}: ${json.message || JSON.stringify(json).slice(0, 200)}`);
    return json.data && json.data.value !== undefined ? json.data.value : (json.data || null);
}

/** The server document's tree as `tag(child,child)` - uids left out. */
function shape(n) {
    const tag = n.kind === 'component' && n.library !== 'builtin' ? `${n.library}:${n.tag}` : (n.tag || '?');
    const kids = (n.children || []).filter((c) => c.tag !== '#text').map(shape);
    return kids.length ? `${tag}(${kids.join(',')})` : tag;
}

async function serverShape() {
    const doc = await op({ op: 'builder_get_document' });
    return doc && doc.root ? shape(doc.root) : null;
}

/** The tags of the app's live DOM, depth first. */
async function liveTags() {
    const h = await op({ op: 'get_node_hierarchy' });
    return (h.nodes || []).map((n) => String(n.tag || n.type || '').toLowerCase());
}

const HELPERS = `
window.__t = {
  row(uid) { return document.querySelector('.azb-row[data-uid="' + uid + '"]'); },
  card(key) { return [...document.querySelectorAll('.azb-card')].find(c => c.dataset.key === key); },
  rows() { return [...document.querySelectorAll('#dom-tree-container .azb-row')].map(r => Number(r.dataset.uid)); },
  uidOf(tag) { const n = [...document.querySelectorAll('#dom-tree-container .azb-row')].find(r => r.textContent.trim().toLowerCase().startsWith('<' + tag) || r.dataset.tag === tag); return n ? Number(n.dataset.uid) : null; },
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
  /** Held over \`dst\`, then cancelled (dragend, no drop). */
  hoverOnly(src, dst, rel) {
    const dt = new DataTransfer();
    const r = dst.getBoundingClientRect();
    const o = { bubbles: true, cancelable: true, dataTransfer: dt, clientX: r.left + 12, clientY: r.top + r.height * rel };
    src.dispatchEvent(new DragEvent('dragstart', { bubbles: true, cancelable: true, dataTransfer: dt }));
    dst.dispatchEvent(new DragEvent('dragenter', o));
    const over = new DragEvent('dragover', o);
    dst.dispatchEvent(over);
    const indicator = ['before', 'after', 'into'].find(z => dst.classList.contains('azb-drop-' + z)) || null;
    src.dispatchEvent(new DragEvent('dragend', { bubbles: true, cancelable: true, dataTransfer: dt }));
    return { indicator, accepted: over.defaultPrevented };
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

async function startApp() {
    if (!fs.existsSync(APP)) throw new Error(`no app at ${APP} (see the header for how to build it)`);
    const env = Object.assign({}, process.env, {
        AZ_BACKEND: 'headless', AZ_DEBUG: String(PORT), DYLD_LIBRARY_PATH: LIB, LD_LIBRARY_PATH: LIB,
    });
    const log = [];
    const proc = spawn(APP, [], { env, stdio: ['ignore', 'pipe', 'pipe'] });
    proc.stdout.on('data', (d) => log.push(String(d)));
    proc.stderr.on('data', (d) => log.push(String(d)));
    for (let i = 0; i < 300; i++) {
        if (proc.exitCode !== null) throw new Error(`the app exited (${proc.exitCode}):\n${log.join('').slice(-2000)}`);
        try { await op({ op: 'get_state' }); return { proc, log }; } catch { /* not yet */ }
        await new Promise((r) => setTimeout(r, 100));
    }
    proc.kill();
    throw new Error(`no debug server on ${BASE}:\n${log.join('').slice(-2000)}`);
}

async function main() {
    const app = await startApp();
    const chrome = await startChrome();
    let cdp;
    try {
        check('the app starts with an empty builder document', (await serverShape()) === 'body', await serverShape());

        cdp = await openPage(chrome, BASE + '/');
        const ready = await waitFor(cdp, `typeof app !== 'undefined' && !!window.azDnd
            && azDnd.state.mode === 'document' && document.querySelectorAll('.azb-card').length > 0
            && !!document.querySelector('.azb-row[data-uid="0"]')`, 20000);
        check('the served page opens AzBuilder in the Document view with a palette', ready,
            await cdp.eval(`({ mode: window.azDnd && azDnd.state.mode, cards: document.querySelectorAll('.azb-card').length })`).catch((e) => String(e)));
        if (!ready) throw new Error('page not ready');
        await cdp.eval(HELPERS);

        // 1. palette <p> -> <body>: INTO.
        let r = await cdp.eval(`__t.drag(__t.card('builtin:p'), __t.row(0), 0.5)`);
        check('a <p> card dropped on <body> is accepted as INTO', r.indicator === 'into' && r.accepted, r);
        await waitFor(cdp, `__t.rows().length === 2`, 5000);
        check('...the server document holds it', (await serverShape()) === 'body(p)', await serverShape());
        check('...and the page shows the new row', await cdp.eval(`__t.rows().length === 2`), await cdp.eval('__t.rows()'));
        await new Promise((res) => setTimeout(res, 300));
        check('...and the app\'s live DOM renders a <p>', (await liveTags()).includes('p'), await liveTags());

        // 2. palette <div> -> top of the <p> row: BEFORE.
        const pUid = await cdp.eval(`__t.rows()[1]`);
        r = await cdp.eval(`__t.drag(__t.card('builtin:div'), __t.row(${pUid}), 0.1)`);
        check('a <div> card on the top of the <p> row is BEFORE', r.indicator === 'before' && r.accepted, r);
        await waitFor(cdp, `__t.rows().length === 3`, 5000);
        check('...the server document has <div> before <p>', (await serverShape()) === 'body(div,p)', await serverShape());

        // 3. A <div> never goes INTO a <p>: the middle of the row is its
        //    halves (B7); the drag is cancelled, nothing reaches the server.
        const before = await serverShape();
        r = await cdp.eval(`__t.hoverOnly(__t.card('builtin:div'), __t.row(${pUid}), 0.5)`);
        check('a <div> over the middle of a <p> is never INTO it (AFTER instead)', r.accepted && r.indicator === 'after', r);
        check('...and a cancelled drag leaves the server document unchanged', (await serverShape()) === before, await serverShape());

        // 4. Move the <p> row into the <div>.
        const divUid = await cdp.eval(`__t.rows()[1]`);
        r = await cdp.eval(`__t.drag(__t.row(${pUid}), __t.row(${divUid}), 0.5)`);
        check('dragging the <p> row INTO the <div> row is accepted', r.indicator === 'into' && r.accepted, r);
        await waitFor(cdp, `JSON.stringify(__t.childUids(${divUid})) === '[${pUid}]'`, 5000);
        check('...the server document nests it', (await serverShape()) === 'body(div(p))', await serverShape());

        // 5. A <span> into the nested <p>.
        r = await cdp.eval(`__t.drag(__t.card('builtin:span'), __t.row(${pUid}), 0.5)`);
        await waitFor(cdp, `__t.rows().length === 4`, 5000);
        check('a <span> card INTO the <p> inserts a child', r.accepted && (await serverShape()) === 'body(div(p(span)))', await serverShape());

        // 6. Undo (Ctrl+Z) and redo.
        await cdp.eval(`__t.key('z', { ctrlKey: true, metaKey: navigator.platform.startsWith('Mac') }); true`);
        await waitFor(cdp, `__t.rows().length === 3`, 5000);
        check('Ctrl/Cmd+Z undoes the last drop on the server', (await serverShape()) === 'body(div(p))', await serverShape());

        // 7. Delete the <div> subtree with the Delete key.
        await cdp.eval(`__t.row(${divUid}).click(); true`);
        await cdp.eval(`__t.key('Delete'); true`);
        await waitFor(cdp, `__t.rows().length === 1`, 5000);
        check('Delete removes the selected subtree on the server', (await serverShape()) === 'body', await serverShape());

        const errors = await cdp.eval(`(window.__azbErrors || []).length`).catch(() => 0);
        check('no uncaught page errors', !errors, errors);
    } finally {
        if (cdp) cdp.close?.();
        if (!KEEP) await stopChrome(chrome);
        app.proc.kill();
    }
    console.log(`\n${passed} passed, ${failures.length} failed`);
    if (failures.length) { console.log('failed:\n  ' + failures.join('\n  ')); process.exit(1); }
}

main().catch((e) => { console.error(e); process.exit(2); });
