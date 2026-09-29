// Unit tests for the pure drag-and-drop logic of debugger-dnd.js.
//
//     node dll/src/desktop/shell2/common/debugger/debugger-dnd.test.js
//
// No dependencies. The UI half of debugger-dnd.js needs a page and `app`;
// under node the module exports only its logic, which is what decides WHERE a
// drop lands and WHICH server message it sends. The last group replays drops
// through a copy of builder.rs's move rule ("the index is the slot as the user
// sees it before the move") to check that what the indicator shows is where
// the node ends up.
'use strict';

const assert = require('assert');
const L = require('./debugger-dnd.js');

let failed = 0;
let passed = 0;
function test(name, fn) {
    try {
        fn();
        passed++;
        console.log('ok   ' + name);
    } catch (e) {
        failed++;
        console.log('FAIL ' + name + '\n     ' + (e && e.message ? e.message : e));
    }
}

// body(0) > [ p(1) "A", div(3) > [ span(4) ], p(2) "B", "tail"(5), br(6), user:card(7) ]
function doc() {
    const el = (uid, tag, children, attrs) =>
        ({ uid, kind: 'element', tag, attrs: attrs || {}, children: children || [] });
    return el(0, 'body', [
        el(1, 'p', [], { text: 'A' }),
        el(3, 'div', [el(4, 'span')]),
        el(2, 'p', [], { text: 'B' }),
        { uid: 5, kind: 'text', tag: '#text', text: 'tail', attrs: {}, children: [] },
        el(6, 'br'),
        { uid: 7, kind: 'component', library: 'user', tag: 'card', attrs: {}, children: [] },
    ]);
}

const rowsOf = (root) => {
    const m = {};
    L.flatten(root, null).forEach((r) => { m[r.uid] = r; });
    return m;
};

test('a container row splits into before / into / after, a leaf into halves, the root is into only', () => {
    const d = doc();
    const div = L.findNode(d, 3);
    assert.strictEqual(L.dropZone(0.1, div, false), 'before');
    assert.strictEqual(L.dropZone(0.5, div, false), 'into');
    assert.strictEqual(L.dropZone(0.9, div, false), 'after');
    for (const uid of [5, 6, 7]) {
        const n = L.findNode(d, uid);
        assert.strictEqual(L.dropZone(0.3, n, false), 'before', 'leaf ' + uid);
        assert.strictEqual(L.dropZone(0.7, n, false), 'after', 'leaf ' + uid);
        assert.strictEqual(L.dropZone(0.5, n, false), 'after', 'leaf ' + uid);
    }
    assert.strictEqual(L.dropZone(0.05, d, true), 'into');
    assert.strictEqual(L.dropZone(0.95, d, true), 'into');
});

test('text nodes, void elements and component instances take no children', () => {
    const d = doc();
    assert.ok(L.acceptsChildren(L.findNode(d, 0)));
    assert.ok(L.acceptsChildren(L.findNode(d, 3)));
    assert.ok(!L.acceptsChildren(L.findNode(d, 5)));
    assert.ok(!L.acceptsChildren(L.findNode(d, 6)));
    assert.ok(!L.acceptsChildren(L.findNode(d, 7)));
});

test('rows carry parent and slot, collapsed subtrees are skipped', () => {
    const d = doc();
    const rows = L.flatten(d, null);
    assert.deepStrictEqual(rows.map((r) => r.uid), [0, 1, 3, 4, 2, 5, 6, 7]);
    const span = rows.find((r) => r.uid === 4);
    assert.deepStrictEqual([span.parent, span.index, span.depth], [3, 0, 2]);
    const collapsed = L.flatten(d, new Set([3]));
    assert.deepStrictEqual(collapsed.map((r) => r.uid), [0, 1, 3, 2, 5, 6, 7]);
});

test('a drop target is (parent, slot as seen), or (row, append) for into', () => {
    const r = rowsOf(doc());
    assert.deepStrictEqual(L.dropTarget(r[2], 'before'), { parent: 0, index: 2 });
    assert.deepStrictEqual(L.dropTarget(r[1], 'after'), { parent: 0, index: 1 });
    assert.deepStrictEqual(L.dropTarget(r[3], 'into'), { parent: 3, index: null });
    assert.deepStrictEqual(L.dropTarget(r[0], 'before'), { parent: 0, index: null });
});

test('a palette drop sends builder_insert, naming the library only when it is not builtin', () => {
    const d = doc();
    const r = rowsOf(d);
    assert.deepStrictEqual(
        L.dropMessage({ type: 'component', library: 'builtin', component: 'p' }, L.dropTarget(r[2], 'before'), d),
        { op: 'builder_insert', parent: 0, component: 'p', index: 2 });
    assert.deepStrictEqual(
        L.dropMessage({ type: 'component', library: 'user', component: 'card' }, L.dropTarget(r[3], 'into'), d),
        { op: 'builder_insert', parent: 3, component: 'card', library: 'user' });
    // The old inspector palette's payload had no `type`.
    assert.deepStrictEqual(
        L.dropMessage({ library: 'builtin', component: 'div' }, { parent: 0, index: null }, d),
        { op: 'builder_insert', parent: 0, component: 'div' });
});

test('nothing drops into a text node, a void element or a component instance', () => {
    const d = doc();
    for (const uid of [5, 6, 7]) {
        assert.strictEqual(
            L.dropMessage({ type: 'component', component: 'p' }, { parent: uid, index: null }, d), null,
            'into ' + uid);
    }
});

test('a row drag sends builder_move and refuses its own subtree and the root', () => {
    const d = doc();
    const r = rowsOf(d);
    assert.deepStrictEqual(
        L.dropMessage({ type: 'builder-node', uid: 1 }, L.dropTarget(r[2], 'after'), d),
        { op: 'builder_move', node: 1, parent: 0, index: 3 });
    assert.strictEqual(L.dropMessage({ type: 'builder-node', uid: 3 }, L.dropTarget(r[3], 'into'), d), null);
    assert.strictEqual(L.dropMessage({ type: 'builder-node', uid: 3 }, L.dropTarget(r[4], 'before'), d), null);
    assert.strictEqual(L.dropMessage({ type: 'builder-node', uid: 0 }, { parent: 3, index: null }, d), null);
    // Before / after ITSELF is allowed (and a no-op).
    assert.ok(L.dropMessage({ type: 'builder-node', uid: 3 }, L.dropTarget(r[3], 'before'), d));
    assert.strictEqual(L.normalizePayload({ type: 'builder-node' }), null);
    assert.strictEqual(L.normalizePayload('nonsense'), null);
});

test('move up / down step over exactly one sibling', () => {
    const d = doc();
    assert.strictEqual(L.stepMessage(d, 1, -1), null, 'the first child cannot go up');
    assert.deepStrictEqual(L.stepMessage(d, 1, +1), { op: 'builder_move', node: 1, parent: 0, index: 2 });
    assert.deepStrictEqual(L.stepMessage(d, 2, -1), { op: 'builder_move', node: 2, parent: 0, index: 1 });
    assert.strictEqual(L.stepMessage(d, 7, +1), null, 'the last child cannot go down');
    assert.strictEqual(L.stepMessage(d, 0, +1), null, 'the root does not move');
});

test('a palette double-click inserts into a selection that can take it, else after it', () => {
    const d = doc();
    assert.deepStrictEqual(L.insertTarget(d, null, 'p'), { parent: 0, index: null });
    assert.deepStrictEqual(L.insertTarget(d, 3, 'p'), { parent: 3, index: null });
    // A <p> takes a <span> but not a <div> (HTML would close the <p>).
    assert.deepStrictEqual(L.insertTarget(d, 2, 'span'), { parent: 2, index: null });
    assert.deepStrictEqual(L.insertTarget(d, 2, 'div'), { parent: 0, index: 3 });
    // Leaves: after them.
    assert.deepStrictEqual(L.insertTarget(d, 5, 'p'), { parent: 0, index: 4 });
    assert.deepStrictEqual(L.insertTarget(d, 7, null), { parent: 0, index: 6 });
});

test('drops that HTML would re-nest are refused, like the server refuses them', () => {
    const d = doc();
    const r = rowsOf(d);
    // A <div> into a <p>: HTML ends the <p> first.
    assert.strictEqual(L.dropMessage({ type: 'component', component: 'div' }, L.dropTarget(r[1], 'into'), d), null);
    assert.ok(L.dropMessage({ type: 'component', component: 'span' }, L.dropTarget(r[1], 'into'), d));
    // Text and library components are not elements the rule knows.
    assert.ok(L.dropMessage({ type: 'component', component: '#text' }, L.dropTarget(r[1], 'into'), d));
    assert.ok(L.dropMessage({ type: 'component', library: 'user', component: 'card' }, L.dropTarget(r[1], 'into'), d));
    // Moving the <div> row into a <p> is the same refusal.
    assert.strictEqual(L.dropMessage({ type: 'builder-node', uid: 3 }, L.dropTarget(r[2], 'into'), d), null);
    assert.ok(L.canContain(L.findNode(d, 3), 'div'));
    assert.ok(!L.canContain(L.findNode(d, 6), null), 'a void element takes nothing');
});

test('the palette hides the non-visual builtins and keeps every library component', () => {
    const entries = L.paletteEntries({ libraries: [
        { name: 'builtin', components: [{ tag: 'html' }, { tag: 'p', display_name: 'Paragraph' }, { tag: 'script' }] },
        { name: 'user', components: [{ tag: 'card', display_name: 'Card', description: 'x' }] },
    ] });
    assert.deepStrictEqual(entries.map((e) => e.library + ':' + e.component + '=' + e.label),
        ['builtin:p=Paragraph', 'user:card=Card']);
    assert.deepStrictEqual(L.paletteEntries(null), []);
});

test('the suggested component name comes from the class, the id, then the tag', () => {
    assert.strictEqual(L.suggestComponentName({ tag: 'div', attrs: { class: 'Card big', id: 'x' } }), 'card');
    assert.strictEqual(L.suggestComponentName({ tag: 'div', attrs: { id: 'Main_Nav' } }), 'main_nav');
    assert.strictEqual(L.suggestComponentName({ tag: 'section', attrs: {} }), 'section');
    assert.strictEqual(L.suggestComponentName({ tag: '', attrs: { class: '123' } }), 'component');
    assert.strictEqual(L.sanitizeComponentName('  My Fancy Card! '), 'my-fancy-card');
    assert.strictEqual(L.sanitizeComponentName('42'), '');
});

// ── The indicator shows where the node ends up ──
//
// A copy of BuilderDocument::move_node (layout/src/e2e/builder.rs): remove,
// then insert at the slot, minus one when the node left an earlier slot of the
// same parent.
function applyMove(root, msg) {
    const parentOf = (n, uid) => {
        for (let i = 0; i < (n.children || []).length; i++) {
            if (n.children[i].uid === uid) return { parent: n, index: i };
            const f = parentOf(n.children[i], uid);
            if (f) return f;
        }
        return null;
    };
    const from = parentOf(root, msg.node);
    const node = from.parent.children.splice(from.index, 1)[0];
    const target = L.findNode(root, msg.parent);
    let at = msg.index == null ? target.children.length : msg.index;
    if (from.parent.uid === msg.parent && msg.index != null && from.index < at) at -= 1;
    target.children.splice(Math.min(at, target.children.length), 0, node);
}

function order(root, uid) { return L.findNode(root, uid).children.map((c) => c.uid); }

test('every drop of a row lands where its indicator was drawn', () => {
    // [1, 3, 2, 5, 6, 7] under body; each case: drag X to zone Z of row R.
    const cases = [
        [1, 2, 'after', [3, 2, 1, 5, 6, 7]],
        [1, 2, 'before', [3, 1, 2, 5, 6, 7]],
        [2, 1, 'before', [2, 1, 3, 5, 6, 7]],
        [7, 3, 'after', [1, 3, 7, 2, 5, 6]],
        [1, 1, 'before', [1, 3, 2, 5, 6, 7]],
        [1, 1, 'after', [1, 3, 2, 5, 6, 7]],
        [6, 7, 'after', [1, 3, 2, 5, 7, 6]],
    ];
    for (const [uid, onto, zone, expected] of cases) {
        const d = doc();
        const msg = L.dropMessage({ type: 'builder-node', uid }, L.dropTarget(rowsOf(d)[onto], zone), d);
        assert.ok(msg, `drag ${uid} ${zone} ${onto} is allowed`);
        applyMove(d, msg);
        assert.deepStrictEqual(order(d, 0), expected, `drag ${uid} ${zone} ${onto}`);
    }
    // Into a container: appended as its last child.
    const d = doc();
    applyMove(d, L.dropMessage({ type: 'builder-node', uid: 2 }, L.dropTarget(rowsOf(d)[3], 'into'), d));
    assert.deepStrictEqual(order(d, 3), [4, 2]);
    assert.deepStrictEqual(order(d, 0), [1, 3, 5, 6, 7]);
    // Out of a container, after it.
    const e = doc();
    applyMove(e, L.dropMessage({ type: 'builder-node', uid: 4 }, L.dropTarget(rowsOf(e)[3], 'after'), e));
    assert.deepStrictEqual(order(e, 3), []);
    assert.deepStrictEqual(order(e, 0), [1, 3, 4, 2, 5, 6, 7]);
});

// ── B7: what a drag shows before the drop ──

test('where INTO is refused (a <div> in a <p>) a row falls back to BEFORE / AFTER by halves', () => {
    const d = doc();
    const r = rowsOf(d);
    const div = { type: 'component', library: 'builtin', component: 'div' };
    // p(1) is a container, but not for a <div>: the middle of its row is its halves.
    let at = L.rowDrop(div, r[1], 0.4, d);
    assert.strictEqual(at.zone, 'before');
    assert.deepStrictEqual(at.msg, { op: 'builder_insert', parent: 0, component: 'div', index: 0 });
    at = L.rowDrop(div, r[1], 0.6, d);
    assert.strictEqual(at.zone, 'after');
    assert.deepStrictEqual(at.msg, { op: 'builder_insert', parent: 0, component: 'div', index: 1 });
    // A <span> may go INTO the <p>: no fallback.
    at = L.rowDrop({ type: 'component', component: 'span' }, r[1], 0.5, d);
    assert.strictEqual(at.zone, 'into');
    assert.deepStrictEqual(at.msg, { op: 'builder_insert', parent: 1, component: 'span' });
    // A row into its own subtree stays refused (no fallback into the refusal).
    at = L.rowDrop({ type: 'builder-node', uid: 3 }, r[4], 0.5, d);
    assert.strictEqual(at.msg, null);
});

test('the drop line sits at the gap the node lands in, at the depth it lands at', () => {
    const d = doc();
    const rows = L.flatten(d, null);
    // BEFORE: the row's top edge, at its own depth.
    assert.deepStrictEqual(L.dropLine(rows, 2, 'before'), { anchor: 2, edge: 'top', depth: 1 });
    // AFTER a leaf: its bottom edge.
    assert.deepStrictEqual(L.dropLine(rows, 1, 'after'), { anchor: 1, edge: 'bottom', depth: 1 });
    // AFTER an expanded container: below its WHOLE subtree, at the container's depth.
    assert.deepStrictEqual(L.dropLine(rows, 3, 'after'), { anchor: 4, edge: 'bottom', depth: 1 });
    // INTO: appended, so below the last descendant, one level deeper.
    assert.deepStrictEqual(L.dropLine(rows, 3, 'into'), { anchor: 4, edge: 'bottom', depth: 2 });
    // INTO a container without children: right under it, one level deeper.
    assert.deepStrictEqual(L.dropLine(rows, 4, 'into'), { anchor: 4, edge: 'bottom', depth: 3 });
    // INTO the root: after the last row, at depth 1.
    assert.deepStrictEqual(L.dropLine(rows, 0, 'into'), { anchor: 7, edge: 'bottom', depth: 1 });
    // A collapsed container: its children are not rows, the line is right under it.
    const folded = L.flatten(d, new Set([3]));
    assert.deepStrictEqual(L.dropLine(folded, 3, 'into'), { anchor: 3, edge: 'bottom', depth: 2 });
    assert.deepStrictEqual(L.dropLine(folded, 3, 'after'), { anchor: 3, edge: 'bottom', depth: 1 });
    assert.strictEqual(L.dropLine(rows, 99, 'into'), null);
});

test('a dropped component appears exactly at the drop line', () => {
    // Replays builder_insert (splice at index / append) and looks the new row up.
    const insert = (root, msg) => {
        const parent = L.findNode(root, msg.parent);
        const node = { uid: 100, kind: 'element', tag: msg.component, attrs: {}, children: [] };
        parent.children.splice(msg.index == null ? parent.children.length : msg.index, 0, node);
    };
    const span = { type: 'component', component: 'span' };
    for (const [onto, relY] of [[3, 0.1], [3, 0.5], [3, 0.9], [4, 0.5], [2, 0.9], [7, 0.2], [0, 0.5]]) {
        const d = doc();
        const before = L.flatten(d, null);
        const row = before.find((r) => r.uid === onto);
        const at = L.rowDrop(span, row, relY, d);
        assert.ok(at.msg, `span onto ${onto} at ${relY} is allowed`);
        const line = L.dropLine(before, onto, at.zone);
        insert(d, at.msg);
        const after = L.flatten(d, null);
        const i = after.findIndex((r) => r.uid === 100);
        const where = `span ${at.zone} ${onto}`;
        assert.strictEqual(after[i].depth, line.depth, where + ': depth');
        if (line.edge === 'bottom') assert.strictEqual(after[i - 1].uid, line.anchor, where + ': right below the line\'s row');
        else assert.strictEqual(after[i + 1].uid, line.anchor, where + ': right above the line\'s row');
    }
});

test('the indent of a depth is the one place rows and the line take it from', () => {
    assert.strictEqual(L.indentPx(0), 4);
    assert.strictEqual(L.indentPx(1), 20);
    assert.strictEqual(L.indentPx(3), 52);
    // The line starts where the landed node's label will: after the 16px toggle.
    assert.strictEqual(L.labelPx(0), 20);
    assert.strictEqual(L.labelPx(2), 52);
});

console.log('\n' + passed + ' passed, ' + failed + ' failed');
process.exit(failed ? 1 : 0);
