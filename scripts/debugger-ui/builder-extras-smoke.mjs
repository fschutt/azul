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

function handle(msg) {
    sent.push(msg);
    switch (msg.op) {
        case 'get_state': return {};
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

        // ── Live DOM hides the builder panels ──
        await cdp.eval(`document.querySelector('.azb-seg button[data-mode=live]').click(); true`);
        const hidden = await waitFor(cdp, `!__t.visible('azb-side')`);
        await cdp.eval(`document.querySelector('.azb-seg button[data-mode=document]').click(); true`);
        const shown = await waitFor(cdp, `__t.visible('azb-side')`);
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
