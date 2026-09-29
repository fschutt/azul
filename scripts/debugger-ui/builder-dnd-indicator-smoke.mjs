// Headless UI test for the drop indicators of AzBuilder's Document tree
// (debugger-dnd.js): what a drag SHOWS before anything is dropped.
//
//     node scripts/debugger-ui/builder-dnd-indicator-smoke.mjs [--shots <tag>] [--chrome <path>] [--keep]
//
// Serves the REAL debugger page with a MOCK of the builder ops (lib/smoke.mjs),
// starts a headless Chrome of its own, drags palette cards over tree rows with
// synthetic `dragstart` / `dragenter` / `dragover` (no drop) and checks:
//
//   * INTO is unmistakable: the whole row is tinted and outlined, and a line one
//     indent level deeper shows where the new child lands (after the row's
//     last visible descendant: `into` appends);
//   * BEFORE / AFTER are a line at the row's own indent, at the gap the node
//     lands in (AFTER an expanded container is after its whole subtree);
//   * nothing is left behind by dragleave, drop or dragend.
//
// `--shots <tag>` also writes the page mid-drag, light and dark
// (`prefers-color-scheme` emulated), to
// scripts/debugger-ui/screenshots/dnd-<tag>-<mode>-<case>.png.
//
// Needs node >= 21 (global WebSocket) and a Chrome / Chromium / Edge binary.
// Exit code 0 = all checks passed.

import fs from 'node:fs';
import path from 'node:path';
import { ROOT, argVal, builderMock, checks, clone, openPage, serveDebugger, startChrome, stopChrome, waitFor } from './lib/smoke.mjs';

const PNG = 'data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNkYAAAAAYAAjCB0C8AAAAASUVORK5CYII=';
const SHOTS_DIR = path.join(ROOT, 'scripts/debugger-ui/screenshots');

// ── the mock debug server ────────────────────────────────────────────────

const registry = { libraries: [{ name: 'builtin', modifiable: false, components: [
    { tag: 'div', display_name: 'Div' },
    { tag: 'p', display_name: 'Paragraph' },
    { tag: 'span', display_name: 'Span' },
    { tag: 'section', display_name: 'Section' },
    { tag: 'ul', display_name: 'Unordered List' },
    { tag: 'li', display_name: 'List Item' },
] }] };

const builder = builderMock(registry);

// body(0) > section(1) > [p(2) "Intro", div(3) > span(4) "inner"], ul(5) > [li(6) "One", li(7) "Two"], p(8) "Tail"
for (const [parent, component, text] of [
    [0, 'section'], [1, 'p', 'Intro'], [1, 'div'], [3, 'span', 'inner'],
    [0, 'ul'], [5, 'li', 'One'], [5, 'li', 'Two'], [0, 'p', 'Tail'],
]) {
    builder.ops({ op: 'builder_insert', parent, component, attrs: text ? { text } : {} });
}

function liveHierarchy() {
    const nodes = [{ index: 0, type: 'Html', tag: 'html', parent: -1, children: [], classes: [] }];
    (function walk(n, parent) {
        const index = nodes.length;
        const text = n.kind === 'text';
        const entry = { index, type: text ? 'Text' : 'Div', tag: text ? undefined : n.tag, parent,
            children: [], classes: [], events: [] };
        if (!text) entry.builder_uid = n.uid;
        nodes.push(entry);
        nodes[parent].children.push(index);
        for (const c of n.children) walk(c, index);
    })(builder.doc.root, 0);
    return { root: 0, node_count: nodes.length, nodes };
}

const sent = [];

function handle(msg) {
    sent.push(msg);
    switch (msg.op) {
        case 'get_state': return { logical_width: 400, logical_height: 300, hidpi_factor: 1 };
        case 'get_component_registry': return clone(registry);
        case 'get_libraries':
            return { libraries: registry.libraries.map((l) => ({
                name: l.name, modifiable: !!l.modifiable, component_count: l.components.length })) };
        case 'get_component_thumbnail':
            return { library: msg.library, name: msg.name, key: '0', data: PNG, empty: false,
                width: 8, height: 4, cached: false };
        case 'get_node_hierarchy': return liveHierarchy();
        case 'get_app_state': return {};
        default: {
            const v = builder.ops(msg);
            return v === undefined ? null : v;
        }
    }
}

// ── the page side ────────────────────────────────────────────────────────

const HELPERS = `
window.__t = {
  tree() { return document.getElementById('dom-tree-container'); },
  row(uid) { return document.querySelector('.azb-row[data-uid="' + uid + '"]'); },
  card(key) { return [...document.querySelectorAll('.azb-card')].find(c => c.dataset.key === key); },
  opts(dst, rel) {
    const r = dst.getBoundingClientRect();
    return { bubbles: true, cancelable: true, dataTransfer: this.dt, clientX: r.left + 60, clientY: r.top + r.height * rel };
  },
  /** Start dragging \`src\` and hold it over \`dst\` at \`rel\` (0 = top edge, 1 = bottom): no drop. */
  hover(src, dst, rel) {
    this.dt = new DataTransfer();
    this.src = src;
    src.dispatchEvent(new DragEvent('dragstart', { bubbles: true, cancelable: true, dataTransfer: this.dt }));
    return this.over(dst, rel);
  },
  /** The drag in progress moves over \`dst\`. */
  over(dst, rel) {
    dst.dispatchEvent(new DragEvent('dragenter', this.opts(dst, rel)));
    const ev = new DragEvent('dragover', this.opts(dst, rel));
    dst.dispatchEvent(ev);
    return ev.defaultPrevented;
  },
  leave(dst, to) {
    dst.dispatchEvent(new DragEvent('dragleave', { bubbles: true, cancelable: true, dataTransfer: this.dt, relatedTarget: to || null }));
  },
  drop(dst, rel) { dst.dispatchEvent(new DragEvent('drop', this.opts(dst, rel))); },
  end() { this.src.dispatchEvent(new DragEvent('dragend', { bubbles: true, cancelable: true, dataTransfer: this.dt })); },
  /** Geometry relative to the tree's content box (scroll included). */
  rel(el) {
    const t = this.tree().getBoundingClientRect();
    const r = el.getBoundingClientRect();
    return { top: r.top - t.top + this.tree().scrollTop, bottom: r.bottom - t.top + this.tree().scrollTop,
             left: r.left - t.left, height: r.height, width: r.width };
  },
  /** The row's own indent: where its toggle starts (depth * 16 + 4 px of .tree-indent). */
  indentOf(uid) { const ind = this.row(uid).querySelector('.tree-indent'); return this.rel(ind).left + ind.getBoundingClientRect().width; },
  /** What the tree shows right now. */
  state() {
    const zones = [];
    document.querySelectorAll('.azb-drop-before, .azb-drop-after, .azb-drop-into').forEach(el => {
      ['before', 'after', 'into'].forEach(z => { if (el.classList.contains('azb-drop-' + z)) zones.push({ uid: Number(el.dataset.uid), zone: z }); });
    });
    const m = document.querySelector('#dom-tree-container .azb-drop-marker');
    const shown = !!m && getComputedStyle(m).display !== 'none' && m.getBoundingClientRect().width > 0;
    return { zones, end: this.tree().classList.contains('azb-drop-end'),
             marker: shown ? Object.assign({ zone: m.dataset.zone || null }, this.rel(m)) : null };
  },
  /** The computed look of a row: tinted (a background) and outlined (outline or box-shadow). */
  look(uid) {
    const cs = getComputedStyle(this.row(uid));
    const bg = cs.backgroundColor;
    const tinted = bg !== 'rgba(0, 0, 0, 0)' && bg !== 'transparent';
    const outlined = (cs.outlineStyle !== 'none' && parseFloat(cs.outlineWidth) > 0) || (cs.boxShadow && cs.boxShadow !== 'none');
    return { bg, tinted, outlined, boxShadow: cs.boxShadow, outline: cs.outlineStyle + ' ' + cs.outlineWidth };
  },
};
true`;

const near = (a, b, tol = 3) => typeof a === 'number' && typeof b === 'number' && Math.abs(a - b) <= tol;

async function shoot(cdp, tag, mode, name) {
    const clip = await cdp.eval(`(() => { const r = __t.tree().getBoundingClientRect();
        return { x: Math.max(0, r.left - 4), y: Math.max(0, r.top - 4), width: Math.min(r.width, 420) + 8, height: Math.min(r.height, 230) + 8 }; })()`);
    const r = await cdp.send('Page.captureScreenshot', { format: 'png', clip: Object.assign({ scale: 2 }, clip) });
    fs.mkdirSync(SHOTS_DIR, { recursive: true });
    const file = path.join(SHOTS_DIR, `dnd-${tag}-${mode}-${name}.png`);
    fs.writeFileSync(file, Buffer.from(r.data, 'base64'));
    return path.relative(ROOT, file);
}

async function main() {
    const { check, finish } = checks();
    const server = await serveDebugger(handle);
    const chrome = await startChrome();
    let cdp;
    try {
        cdp = await openPage(chrome, `http://127.0.0.1:${server.address().port}/`);
        const ready = await waitFor(cdp, `typeof app !== 'undefined' && !!window.azDnd
            && azDnd.state.mode === 'document' && !!document.querySelector('.azb-row[data-uid="8"]')
            && !!document.querySelector('.azb-card')`, 10000);
        check('the page opens the builder document (9 rows) and the palette', ready);
        if (!ready) throw new Error('page not ready');
        await cdp.eval(HELPERS);

        // First, on the fresh document: the page mid-drag, before anything is dropped.
        const tag = argVal('--shots');
        if (tag) {
            const cases = [
                ['into-container', `__t.hover(__t.card('builtin:span'), __t.row(1), 0.5)`],
                ['into-empty', `__t.hover(__t.card('builtin:span'), __t.row(6), 0.5)`],
                ['before', `__t.hover(__t.card('builtin:p'), __t.row(8), 0.1)`],
                ['after-container', `__t.hover(__t.card('builtin:p'), __t.row(5), 0.9)`],
                ['into-refused', `__t.hover(__t.card('builtin:div'), __t.row(2), 0.6)`],
            ];
            for (const mode of ['light', 'dark']) {
                await cdp.send('Emulation.setEmulatedMedia', { features: [{ name: 'prefers-color-scheme', value: mode }] });
                for (const [name, expr] of cases) {
                    await cdp.eval(expr);
                    console.log('     screenshot: ' + await shoot(cdp, tag, mode, name));
                    await cdp.eval('__t.end(); true');
                }
            }
            await cdp.send('Emulation.setEmulatedMedia', { features: [] });
        }


        // ── INTO an expanded container with children ──
        let accepted = await cdp.eval(`__t.hover(__t.card('builtin:span'), __t.row(1), 0.5)`);
        let s = await cdp.eval('__t.state()');
        const plain = await cdp.eval('__t.look(8)');
        const into = await cdp.eval('__t.look(1)');
        check('INTO a container is accepted and marks only that row', accepted
            && JSON.stringify(s.zones) === JSON.stringify([{ uid: 1, zone: 'into' }]), s);
        check('INTO tints the whole row (a background no plain row has)', into.tinted && into.bg !== plain.bg, { into, plain });
        check('INTO outlines the whole row', into.outlined, into);
        let geo = await cdp.eval(`({ childIndent: __t.indentOf(2), lastDesc: __t.rel(__t.row(4)).bottom })`);
        check('INTO shows where the child lands: a line one level deeper, after the row\'s last descendant',
            s.marker && s.marker.zone === 'into' && near(s.marker.left, geo.childIndent, 4)
            && near(s.marker.top + s.marker.height / 2, geo.lastDesc, 3), { marker: s.marker, want: geo });
        await cdp.eval('__t.end(); true');
        s = await cdp.eval('__t.state()');
        check('dragend leaves nothing behind', s.zones.length === 0 && !s.marker && !s.end, s);

        // ── INTO a container without children ──
        accepted = await cdp.eval(`__t.hover(__t.card('builtin:span'), __t.row(6), 0.5)`);
        s = await cdp.eval('__t.state()');
        geo = await cdp.eval(`({ childIndent: __t.indentOf(6) + 16, bottom: __t.rel(__t.row(6)).bottom })`);
        check('INTO an empty container: the line sits right under the row, one level deeper',
            accepted && s.marker && s.marker.zone === 'into' && near(s.marker.left, geo.childIndent, 4)
            && near(s.marker.top + s.marker.height / 2, geo.bottom, 3), { marker: s.marker, want: geo });

        // Moving on to another row's BEFORE zone: the INTO highlight does not stay behind.
        accepted = await cdp.eval(`__t.leave(__t.row(6), __t.row(8)); __t.over(__t.row(8), 0.1)`);
        s = await cdp.eval('__t.state()');
        geo = await cdp.eval(`({ indent: __t.indentOf(8), top: __t.rel(__t.row(8)).top })`);
        check('moving to the next row moves the indicator (no stale INTO)',
            JSON.stringify(s.zones) === JSON.stringify([{ uid: 8, zone: 'before' }]), s);
        check('BEFORE is a line at the row\'s own indent, at its top edge',
            accepted && s.marker && s.marker.zone === 'before' && near(s.marker.left, geo.indent, 4)
            && near(s.marker.top + s.marker.height / 2, geo.top, 3), { marker: s.marker, want: geo });
        check('BEFORE does not tint the row', !(await cdp.eval('__t.look(8)')).tinted);
        await cdp.eval(`__t.leave(__t.row(8), document.getElementById('palette-component-list') || document.body); true`);
        s = await cdp.eval('__t.state()');
        check('dragleave out of the tree leaves nothing behind', s.zones.length === 0 && !s.marker, s);
        await cdp.eval('__t.end(); true');

        // ── INTO refused (a <div> in a <p>): the halves of the row instead ──
        accepted = await cdp.eval(`__t.hover(__t.card('builtin:div'), __t.row(2), 0.4)`);
        s = await cdp.eval('__t.state()');
        check('where INTO is refused (a <div> in a <p>) the middle of the row falls back to BEFORE / AFTER',
            accepted && JSON.stringify(s.zones) === JSON.stringify([{ uid: 2, zone: 'before' }]) && s.marker
            && s.marker.zone === 'before', s);
        const before3 = sent.length;
        await cdp.eval(`__t.drop(__t.row(2), 0.4); __t.end(); true`);
        await waitFor(cdp, `!!__t.row(9)`);
        check('...and the drop lands there (before the <p>, inside the <section>)',
            JSON.stringify(sent.slice(before3).find((m) => m.op === 'builder_insert'))
                === JSON.stringify({ op: 'builder_insert', parent: 1, component: 'div', index: 0 }),
            sent.slice(before3));

        // ── AFTER an expanded container: after its whole subtree ──
        accepted = await cdp.eval(`__t.hover(__t.card('builtin:p'), __t.row(5), 0.9)`);
        s = await cdp.eval('__t.state()');
        geo = await cdp.eval(`({ indent: __t.indentOf(5), subtreeEnd: __t.rel(__t.row(7)).bottom })`);
        check('AFTER an expanded container is a line at its indent below its whole subtree (where it lands)',
            accepted && s.marker && s.marker.zone === 'after' && near(s.marker.left, geo.indent, 4)
            && near(s.marker.top + s.marker.height / 2, geo.subtreeEnd, 3), { marker: s.marker, want: geo });
        await cdp.eval(`__t.drop(__t.row(5), 0.9); __t.end(); true`);
        await waitFor(cdp, `!!__t.row(10)`);
        s = await cdp.eval('__t.state()');
        check('a drop leaves nothing behind', s.zones.length === 0 && !s.marker && !s.end, s);

        // ── the empty space below the rows: append to <body> ──
        await cdp.eval(`(() => { __t.dt = new DataTransfer(); __t.src = __t.card('builtin:div');
            __t.src.dispatchEvent(new DragEvent('dragstart', { bubbles: true, cancelable: true, dataTransfer: __t.dt }));
            const c = __t.tree(); const r = c.getBoundingClientRect();
            const o = { bubbles: true, cancelable: true, dataTransfer: __t.dt, clientX: r.left + 60, clientY: r.bottom - 4 };
            c.dispatchEvent(new DragEvent('dragenter', o)); c.dispatchEvent(new DragEvent('dragover', o)); return true; })()`);
        s = await cdp.eval('__t.state()');
        geo = await cdp.eval(`(() => { const rows = [...document.querySelectorAll('#dom-tree-container .azb-row')];
            return { indent: __t.indentOf(1), end: __t.rel(rows[rows.length - 1]).bottom }; })()`);
        check('below the rows: a line at <body>\'s child indent after the last row',
            s.marker && near(s.marker.left, geo.indent, 4) && near(s.marker.top + s.marker.height / 2, geo.end, 3),
            { marker: s.marker, want: geo });
        await cdp.eval(`__t.leave(__t.tree(), document.body); true`);
        s = await cdp.eval('__t.state()');
        check('...and leaving clears it', !s.end && !s.marker, s);
        await cdp.eval('__t.end(); true');

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
