// Headless UI test for AzBuilder's B5 extras (debugger-dnd.js): the properties
// panel, the document stylesheet, drops onto the window canvas, the hidden
// `azb-<uid>` markers, duplicate and the document file.
//
//     node scripts/debugger-ui/builder-extras-smoke.mjs [--chrome <path>] [--keep] [--screenshot out.png]
//
// Serves the REAL debugger page with a MOCK of the debug server's builder ops
// (lib/smoke.mjs), starts a headless Chrome of its own and drives the page
// with synthetic events. It checks what the page SENT and what it SHOWED. The
// server side is tested by
//     cargo test -p azul-layout --features e2e-server --lib builder
//
// Needs node >= 21 (global WebSocket) and a Chrome / Chromium / Edge binary
// (default: the macOS Google Chrome path; `--chrome` or $CHROME to override).
// Exit code 0 = all checks passed.

import fs from 'node:fs';
import { argVal, builderMock, checks, clone, openPage, serveDebugger, startChrome, stopChrome, waitFor } from './lib/smoke.mjs';

const PNG = 'data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNkYAAAAAYAAjCB0C8AAAAASUVORK5CYII=';

// ── the mock debug server ────────────────────────────────────────────────

const registry = { libraries: [
    { name: 'builtin', modifiable: false, components: [
        { tag: 'div', display_name: 'Div', data_model: [] },
        { tag: 'p', display_name: 'Paragraph', data_model: [] },
        { tag: 'span', display_name: 'Span', data_model: [] },
    ] },
    { name: 'user', modifiable: true, components: [
        { tag: 'card', display_name: 'Card', data_model: [
            { name: 'text', field_type: 'String', default: 'Title', required: false, description: 'Text of the <h1>' },
            { name: 'href', field_type: 'String', default: 'https://e.com', required: false, description: '' },
            { name: 'wide', field_type: 'bool', default: 'false', required: false, description: '' },
            { name: 'on_click', field_type: 'Callback(Update)', default: null, required: false, description: '' },
        ] },
    ] },
] };

const builder = builderMock(registry);
const sent = [];

// The window's CPU picture: 800x600 px for a 400x300 logical window (dpi 2),
// so the page must map through the LOGICAL size, not the picture's pixels.
const WINDOW_PICTURE = 'data:image/svg+xml,' + encodeURIComponent(
    '<svg xmlns="http://www.w3.org/2000/svg" width="800" height="600">'
    + '<rect width="800" height="600" fill="#fff"/><rect y="0" width="800" height="80" fill="#cde"/></svg>');

function handle(msg) {
    sent.push(msg);
    switch (msg.op) {
        case 'get_state': return { logical_width: 400, logical_height: 300, hidpi_factor: 2 };
        case 'take_screenshot': return { data: WINDOW_PICTURE };
        case 'get_component_registry': return clone(registry);
        case 'get_libraries':
            return { libraries: registry.libraries.map((l) => ({
                name: l.name, modifiable: !!l.modifiable, component_count: l.components.length })) };
        case 'get_component_thumbnail':
            return { library: msg.library, name: msg.name, key: '0', data: PNG, empty: false,
                width: 8, height: 4, cached: false };
        case 'get_node_hierarchy': return { root: 0, node_count: 1,
            nodes: [{ index: 0, type: 'Body', tag: 'body', parent: -1, children: [], classes: [] }] };
        case 'get_app_state': return {};
        default: {
            const v = builder.ops(msg);
            return v === undefined ? null : v;
        }
    }
}

const lastSent = (op) => [...sent].reverse().find((m) => m.op === op);
const countSent = (op) => sent.filter((m) => m.op === op).length;
const same = (a, b) => JSON.stringify(a) === JSON.stringify(b);

const HELPERS = `
window.__t = {
  row(uid) { return document.querySelector('.azb-row[data-uid="' + uid + '"]'); },
  prop(name) { return document.querySelector('#azb-props [data-prop="' + name + '"]'); },
  propInput(name) { const r = this.prop(name); return r && r.querySelector('input, textarea, select'); },
  props() { return [...document.querySelectorAll('#azb-props [data-prop]')].map(r => r.dataset.prop); },
  edit(name, value) {
    const i = this.propInput(name);
    if (!i) return false;
    i.focus();
    if (i.type === 'checkbox') { i.checked = !!value; i.dispatchEvent(new Event('change', { bubbles: true })); return true; }
    i.value = value;
    i.dispatchEvent(new Event('input', { bubbles: true }));
    i.dispatchEvent(new Event('change', { bubbles: true }));
    return true;
  },
  key(key, mods) {
    document.body.dispatchEvent(new KeyboardEvent('keydown', Object.assign({ key, bubbles: true, cancelable: true }, mods || {})));
  },
  visible(id) { const e = document.getElementById(id); return !!e && e.offsetParent !== null; },
  canvasImg() { return document.getElementById('azb-canvas-img'); },
  /** Client coordinates of the LOGICAL window point (x, y) on the picture. */
  at(x, y) {
    const r = this.canvasImg().getBoundingClientRect();
    return { clientX: r.left + x * r.width / 400, clientY: r.top + y * r.height / 300 };
  },
  /** Start dragging a palette card and hover the picture at (x, y): accepted? */
  canvasOver(key, x, y) {
    const src = [...document.querySelectorAll('.azb-card')].find(c => c.dataset.key === key);
    window.__dt = new DataTransfer();
    window.__src = src;
    src.dispatchEvent(new DragEvent('dragstart', { bubbles: true, cancelable: true, dataTransfer: __dt }));
    const o = Object.assign({ bubbles: true, cancelable: true, dataTransfer: __dt }, this.at(x, y));
    this.canvasImg().dispatchEvent(new DragEvent('dragenter', o));
    const over = new DragEvent('dragover', o);
    this.canvasImg().dispatchEvent(over);
    return over.defaultPrevented;
  },
  canvasDrop(x, y) {
    const o = Object.assign({ bubbles: true, cancelable: true, dataTransfer: __dt }, this.at(x, y));
    this.canvasImg().dispatchEvent(new DragEvent('drop', o));
    __src.dispatchEvent(new DragEvent('dragend', { bubbles: true, cancelable: true, dataTransfer: __dt }));
    return true;
  },
  mark() {
    const m = document.getElementById('azb-canvas-mark');
    return m && !m.classList.contains('hidden') ? m.dataset.zone : null;
  },
  typeSheet(text) {
    const t = document.getElementById('azb-sheet-text');
    if (!t) return false;
    t.focus();
    t.value = text;
    t.dispatchEvent(new Event('input', { bubbles: true }));
    return true;
  },
};
true`;

async function main() {
    const { check, finish } = checks();
    const server = await serveDebugger(handle);
    const chrome = await startChrome();
    let cdp;
    try {
        cdp = await openPage(chrome, `http://127.0.0.1:${server.address().port}/`);
        const ready = await waitFor(cdp, `typeof app !== 'undefined' && !!window.azDnd
            && azDnd.state.mode === 'document' && document.querySelectorAll('.azb-card').length > 0
            && !!document.querySelector('.azb-row[data-uid="0"]')`, 10000);
        check('the page opens AzBuilder in the Document view', ready,
            await cdp.eval(`({ mode: window.azDnd && azDnd.state.mode })`).catch((e) => String(e)));
        if (!ready) throw new Error('page not ready');
        await cdp.eval(HELPERS);

        // ── 1. the properties panel ──
        check('a Properties side panel sits next to the node detail, and asks for a selection',
            await cdp.eval(`__t.visible('azb-side') && document.getElementById('azb-props').textContent.includes('Select a node')`),
            await cdp.eval(`({ side: !!document.getElementById('azb-side'), text: (document.getElementById('azb-props') || {}).textContent })`));
        await cdp.eval(`azDnd.send({ op: 'builder_insert', parent: 0, component: 'p' }); true`);
        await waitFor(cdp, `!!__t.row(1) && __t.props().length > 0`);
        check('the inserted <p> is selected and the panel shows its text, id, classes and style',
            same(await cdp.eval(`__t.props()`), ['text', 'id', 'class', 'style'])
            && await cdp.eval(`__t.propInput('text').value === 'Paragraph text'`),
            await cdp.eval(`__t.props()`));
        check('...in the look of the other panels (the field rows of the Components view)',
            await cdp.eval(`__t.prop('class').classList.contains('azd-field-row')
                && !!__t.prop('class').querySelector('.azd-type-badge')`));

        await cdp.eval(`__t.edit('class', 'lead')`);
        await waitFor(cdp, `(__t.row(1).textContent || '').includes('.lead')`);
        check('editing a field sends builder_set_attribute and the tree shows it',
            same(lastSent('builder_set_attribute'), { op: 'builder_set_attribute', node: 1, name: 'class', value: 'lead' })
            && await cdp.eval(`__t.row(1).textContent.includes('.lead')`), lastSent('builder_set_attribute'));
        const beforeSame = countSent('builder_set_attribute');
        await cdp.eval(`__t.edit('class', 'lead')`);
        await new Promise((r) => setTimeout(r, 150));
        check('an unchanged field sends nothing', countSent('builder_set_attribute') === beforeSame);

        await cdp.eval(`__t.edit('text', '')`);
        await waitFor(cdp, `__t.propInput('text') && __t.propInput('text').value === ''
            && !azDnd.logic.findNode(azDnd.state.doc.root, 1).attrs.text`);
        check('emptying a field removes the attribute (no value in the message)',
            same(lastSent('builder_set_attribute'), { op: 'builder_set_attribute', node: 1, name: 'text' }),
            lastSent('builder_set_attribute'));

        await cdp.eval(`document.activeElement && document.activeElement.blur(); __t.key('z', { ctrlKey: true }); true`);
        await waitFor(cdp, `__t.propInput('text') && __t.propInput('text').value === 'Paragraph text'`);
        check('Ctrl+Z undoes a panel edit, and the panel shows the value again',
            countSent('builder_undo') === 1 && await cdp.eval(`__t.propInput('text').value === 'Paragraph text'`));

        await cdp.eval(`azDnd.send({ op: 'builder_insert', parent: 0, library: 'user', component: 'card', attrs: { text: 'Hi' } }); true`);
        await waitFor(cdp, `!!__t.row(2) && !!__t.prop('href')`);
        check('a component instance shows its arguments (its data model), then class / id / style',
            same(await cdp.eval(`__t.props()`), ['text', 'href', 'wide', 'on_click', 'class', 'id', 'style'])
            && await cdp.eval(`document.querySelector('#azb-props').textContent.includes('Arguments')`),
            await cdp.eval(`__t.props()`));
        check("...its own value, the default as the placeholder of an argument it does not set",
            await cdp.eval(`__t.propInput('text').value === 'Hi' && __t.propInput('href').value === ''
                && __t.propInput('href').placeholder === 'https://e.com'`));
        check('...a bool argument is a checkbox, a callback is read-only',
            await cdp.eval(`__t.propInput('wide').type === 'checkbox'
                && __t.prop('on_click').classList.contains('azb-prop-readonly')`));
        await cdp.eval(`__t.edit('href', 'https://azul.rs')`);
        await waitFor(cdp, `azDnd.logic.findNode(azDnd.state.doc.root, 2).attrs.href === 'https://azul.rs'`);
        check('an argument edit sets that argument on the instance',
            same(lastSent('builder_set_attribute'), { op: 'builder_set_attribute', node: 2, name: 'href', value: 'https://azul.rs' }),
            lastSent('builder_set_attribute'));
        await cdp.eval(`__t.edit('wide', true)`);
        await waitFor(cdp, `azDnd.logic.findNode(azDnd.state.doc.root, 2).attrs.wide === 'true'`);
        check('ticking a bool argument sends "true"',
            same(lastSent('builder_set_attribute'), { op: 'builder_set_attribute', node: 2, name: 'wide', value: 'true' }),
            lastSent('builder_set_attribute'));

        await cdp.eval(`__t.row(1).click(); true`);
        await waitFor(cdp, `__t.props()[0] === 'text' && !__t.prop('href')`);
        check('selecting another row shows that node', await cdp.eval(`!!__t.prop('style') && !__t.prop('href')`));

        // ── 2. the document's stylesheet ──
        check('a Stylesheet editor sits in the side panel, empty for a new document',
            await cdp.eval(`__t.visible('azb-sheet-text') && document.getElementById('azb-sheet-text').value === ''`));
        await cdp.eval(`__t.typeSheet('.lead { color: red; }')`);
        check('typing marks it as not applied yet (nothing is sent)',
            await cdp.eval(`document.getElementById('azb-sheet').classList.contains('azb-dirty')`)
            && countSent('builder_set_stylesheet') === 0);
        await cdp.eval(`document.getElementById('azb-sheet-apply').click(); true`);
        await waitFor(cdp, `azDnd.state.doc.stylesheet === '.lead { color: red; }'`);
        check('Apply sends builder_set_stylesheet with the text',
            same(lastSent('builder_set_stylesheet'), { op: 'builder_set_stylesheet', css: '.lead { color: red; }' })
            && await cdp.eval(`!document.getElementById('azb-sheet').classList.contains('azb-dirty')`),
            lastSent('builder_set_stylesheet'));
        await cdp.eval(`__t.typeSheet('.lead { color: blue; }');
            document.getElementById('azb-sheet-text').dispatchEvent(new KeyboardEvent('keydown',
                { key: 'Enter', ctrlKey: true, bubbles: true, cancelable: true })); true`);
        await waitFor(cdp, `azDnd.state.doc.stylesheet === '.lead { color: blue; }'`);
        check('Ctrl+Enter in the editor applies too',
            countSent('builder_set_stylesheet') === 2 && lastSent('builder_set_stylesheet').css === '.lead { color: blue; }');
        const undos = countSent('builder_undo');
        await cdp.eval(`document.activeElement && document.activeElement.blur(); __t.key('z', { ctrlKey: true }); true`);
        await waitFor(cdp, `document.getElementById('azb-sheet-text').value === '.lead { color: red; }'`);
        check('Ctrl+Z undoes the stylesheet like any edit, and the editor shows the earlier text',
            countSent('builder_undo') === undos + 1
            && await cdp.eval(`document.getElementById('azb-sheet-text').value === '.lead { color: red; }'`));
        await cdp.eval(`__t.typeSheet('.draft { }'); azDnd.send({ op: 'builder_insert', parent: 0, component: 'span' }); true`);
        await waitFor(cdp, `!!__t.row(3)`);
        check('text not applied yet survives other edits',
            await cdp.eval(`document.getElementById('azb-sheet-text').value === '.draft { }'`));
        await cdp.eval(`__t.typeSheet('.x {'); document.getElementById('azb-sheet-apply').click(); true`);
        await waitFor(cdp, `document.getElementById('azb-sheet-status').textContent.includes('unclosed block')`);
        check("the parser's warnings show under the editor",
            await cdp.eval(`document.getElementById('azb-sheet-status').textContent.includes('unclosed block')`),
            await cdp.eval(`document.getElementById('azb-sheet-status').textContent`));

        // ── 3. drops onto the window canvas ──
        // Document now: body > p(1), user:card(2), span(3) - 40px rows in the mock window.
        await waitFor(cdp, `!!__t.canvasImg() && __t.canvasImg().naturalWidth === 800`);
        check('the Inspector shows the window as a picture (take_screenshot), mapped by its logical size (get_state)',
            await cdp.eval(`__t.visible('azb-canvas') && __t.canvasImg().naturalWidth === 800`)
            && countSent('take_screenshot') > 0 && countSent('get_state') > 0,
            { shots: countSent('take_screenshot'), state: countSent('get_state') });
        const shots = countSent('take_screenshot');

        let accepted = await cdp.eval(`__t.canvasOver('builtin:div', 100, 60)`);
        await waitFor(cdp, `__t.mark() !== null`);
        const probe = lastSent('builder_hit_test');
        check('hovering the picture hit-tests the window point under the pointer (builder_hit_test)',
            accepted && probe && Math.abs(probe.x - 100) < 1.5 && Math.abs(probe.y - 60) < 1.5, probe);
        check('...and shows where the drop lands: AFTER the instance (a leaf, lower half)',
            await cdp.eval(`__t.mark()`) === 'after', await cdp.eval(`__t.mark()`));
        await cdp.eval(`__t.canvasDrop(100, 60)`);
        await waitFor(cdp, `!!__t.row(4)`);
        check('dropping inserts there, like the tree drop (builder_insert after the instance)',
            same(lastSent('builder_insert'), { op: 'builder_insert', parent: 0, component: 'div', index: 2 })
            && await cdp.eval(`__t.mark() === null`), lastSent('builder_insert'));
        // body > p(1), card(2), div(4), span(3)
        await cdp.eval(`__t.canvasOver('builtin:span', 100, 20)`);
        await waitFor(cdp, `__t.mark() === 'into'`);
        await cdp.eval(`__t.canvasDrop(100, 20)`);
        await waitFor(cdp, `!!__t.row(5)`);
        check('the middle of a container drops INTO it',
            same(lastSent('builder_insert'), { op: 'builder_insert', parent: 1, component: 'span' }),
            lastSent('builder_insert'));
        await cdp.eval(`__t.canvasOver('builtin:div', 100, 25)`);
        await waitFor(cdp, `__t.mark() === 'after'`);
        await cdp.eval(`__t.canvasDrop(100, 25)`);
        await waitFor(cdp, `!!__t.row(6)`);
        check('a <div> over the middle of a <p> goes after it (INTO would be undone by the parser)',
            same(lastSent('builder_insert'), { op: 'builder_insert', parent: 0, component: 'div', index: 1 }),
            lastSent('builder_insert'));
        await cdp.eval(`__t.canvasOver('builtin:p', 100, 280)`);
        await waitFor(cdp, `__t.mark() === 'into'`);
        await cdp.eval(`__t.canvasDrop(100, 280)`);
        await waitFor(cdp, `!!__t.row(7)`);
        check('below every node the drop appends to <body>',
            same(lastSent('builder_insert'), { op: 'builder_insert', parent: 0, component: 'p' }),
            lastSent('builder_insert'));
        check('the picture follows the edits (a new take_screenshot after them)',
            await waitFor(cdp, `true`) && countSent('take_screenshot') > shots,
            { before: shots, after: countSent('take_screenshot') });
        // body > p(1), div(6), card(2), div(4), span(3), p(7): (100, 100) is the card.
        await cdp.eval(`(() => { const p = __t.at(100, 100);
            __t.canvasImg().dispatchEvent(new MouseEvent('click', Object.assign({ bubbles: true }, p))); return true; })()`);
        await waitFor(cdp, `azDnd.state.selected === 2 && !!__t.prop('href')`);
        check('clicking the picture selects the node under the pointer (tree and Properties)',
            await cdp.eval(`azDnd.state.selected === 2 && __t.row(2).classList.contains('selected') && !!__t.prop('href')`));

        // ── Live DOM hides the builder panels ──
        await cdp.eval(`document.querySelector('.azb-seg button[data-mode=live]').click(); true`);
        const hidden = await waitFor(cdp, `!__t.visible('azb-side') && !__t.visible('azb-canvas')`);
        await cdp.eval(`document.querySelector('.azb-seg button[data-mode=document]').click(); true`);
        const shown = await waitFor(cdp, `__t.visible('azb-side') && __t.visible('azb-canvas')`);
        check('Live DOM hides the builder panels, Document shows them again', hidden && shown);

        const shot = argVal('--screenshot');
        if (shot) {
            fs.writeFileSync(shot, await cdp.screenshot());
            console.log('     screenshot: ' + shot);
        }
        check('no page exceptions', cdp.exceptions.length === 0, cdp.exceptions);
        const errors = cdp.console.filter((c) => c.kind === 'error');
        check('no console errors', errors.length === 0, errors);
    } finally {
        if (cdp) cdp.close();
        await stopChrome(chrome);
        server.close();
    }
    finish();
}

main().catch((e) => { console.error(e); process.exit(2); });
