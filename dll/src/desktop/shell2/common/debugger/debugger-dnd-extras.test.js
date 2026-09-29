// Unit tests for the B5 logic of debugger-dnd.js: the properties panel, the
// document stylesheet panel, drops onto the window canvas, duplicate.
//
//     node dll/src/desktop/shell2/common/debugger/debugger-dnd-extras.test.js
//
// No dependencies. Under node debugger-dnd.js exports only its pure logic:
// which rows the properties panel shows for a node, which server message an
// edit in it sends, where a drop on the window picture lands, and so on.
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

const el = (uid, tag, attrs, children) =>
    ({ uid, kind: 'element', tag, attrs: attrs || {}, children: children || [] });
const text = (uid, t) => ({ uid, kind: 'text', tag: '#text', text: t, attrs: {}, children: [] });
const inst = (uid, library, tag, attrs) =>
    ({ uid, kind: 'component', library, tag, attrs: attrs || {}, children: [] });

// A registry entry as `get_component_registry` answers it.
const CARD = {
    tag: 'card', display_name: 'Card',
    data_model: [
        { name: 'text', field_type: 'String', default: 'Title', required: false, description: 'Text of the <h1>' },
        { name: 'href', field_type: 'String', default: 'https://e.com', required: false, description: '' },
        { name: 'wide', field_type: 'bool', default: 'false', required: false, description: '' },
        { name: 'on_click', field_type: 'Callback(Update)', default: null, required: false, description: '' },
    ],
};
const REGISTRY = { libraries: [
    { name: 'builtin', components: [{ tag: 'p' }, { tag: 'div' }] },
    { name: 'user', components: [CARD] },
] };

const names = (rows) => rows.map((r) => r.name);

// ── 1. the properties panel ─────────────────────────────────────────────

test('an element shows its text, id, classes and style first, then its other attributes', () => {
    const rows = L.propertyRows(el(3, 'a', { href: 'x', text: 'More', class: 'btn', 'data-k': '1' }), null);
    assert.deepStrictEqual(names(rows), ['text', 'id', 'class', 'style', 'data-k', 'href']);
    const byName = Object.fromEntries(rows.map((r) => [r.name, r]));
    assert.strictEqual(byName.text.value, 'More');
    assert.strictEqual(byName.id.value, '', 'an attribute the node does not have is empty');
    assert.strictEqual(byName.class.value, 'btn');
    assert.ok(rows.every((r) => r.group === 'attribute' && r.fieldType === 'String'));
});

test('a void element and the <body> have no text row; a text node has only its text', () => {
    assert.deepStrictEqual(names(L.propertyRows(el(4, 'br'), null)), ['id', 'class', 'style']);
    assert.deepStrictEqual(names(L.propertyRows(el(0, 'body'), null)), ['id', 'class', 'style']);
    const t = L.propertyRows(text(5, 'tail'), null);
    assert.deepStrictEqual(names(t), ['text']);
    assert.strictEqual(t[0].value, 'tail');
    assert.deepStrictEqual(L.propertyRows(null, null), []);
});

test('a component instance shows its arguments (the data model), then class / id / style', () => {
    const def = L.componentDef(REGISTRY, 'user', 'card');
    assert.strictEqual(def, CARD, 'looked up by library and tag');
    assert.strictEqual(L.componentDef(REGISTRY, 'user', 'nope'), null);
    const rows = L.propertyRows(inst(7, 'user', 'card', { text: 'Hi', class: 'big' }), def);
    assert.deepStrictEqual(names(rows), ['text', 'href', 'wide', 'on_click', 'class', 'id', 'style']);
    const byName = Object.fromEntries(rows.map((r) => [r.name, r]));
    assert.strictEqual(byName.text.group, 'argument');
    assert.strictEqual(byName.text.value, 'Hi', "the instance's own argument");
    assert.strictEqual(byName.href.value, null, 'not given: the default applies');
    assert.strictEqual(byName.href.default, 'https://e.com');
    assert.strictEqual(byName.wide.fieldType, 'bool', 'the server type string, parsed by the widget');
    assert.strictEqual(byName.class.group, 'attribute');
    assert.strictEqual(byName.class.value, 'big');
});

test('an instance of an unknown component still shows what it has', () => {
    const rows = L.propertyRows(inst(8, 'gone', 'x', { title: 'T' }), null);
    assert.deepStrictEqual(names(rows), ['class', 'id', 'style', 'title']);
});

test('an edit sends builder_set_attribute; empty removes the attribute; unchanged sends nothing', () => {
    const a = el(3, 'a', { href: 'x', text: 'More' });
    assert.deepStrictEqual(L.propertyMessage(a, 'text', 'Less'),
        { op: 'builder_set_attribute', node: 3, name: 'text', value: 'Less' });
    assert.deepStrictEqual(L.propertyMessage(a, 'href', ''),
        { op: 'builder_set_attribute', node: 3, name: 'href' }, 'no value: removed');
    assert.strictEqual(L.propertyMessage(a, 'href', 'x'), null, 'unchanged');
    assert.strictEqual(L.propertyMessage(a, 'id', ''), null, 'removing what is not there');
    // A text node's only attribute is its text; emptying it keeps the node.
    assert.deepStrictEqual(L.propertyMessage(text(5, 'tail'), 'text', ''),
        { op: 'builder_set_attribute', node: 5, name: 'text', value: '' });
    // An argument: set, or removed so the default applies again.
    const c = inst(7, 'user', 'card', { text: 'Hi' });
    assert.deepStrictEqual(L.propertyMessage(c, 'href', 'https://azul.rs'),
        { op: 'builder_set_attribute', node: 7, name: 'href', value: 'https://azul.rs' });
    assert.deepStrictEqual(L.propertyMessage(c, 'text', ''),
        { op: 'builder_set_attribute', node: 7, name: 'text' });
});

test('a typed widget value becomes the attribute text the server parses, and back', () => {
    assert.strictEqual(L.attrString({ type: 'String', value: 'a b' }), 'a b');
    assert.strictEqual(L.attrString({ type: 'Bool', value: true }), 'true');
    assert.strictEqual(L.attrString({ type: 'Bool', value: false }), 'false');
    assert.strictEqual(L.attrString({ type: 'I32', value: -4 }), '-4');
    assert.strictEqual(L.attrString({ type: 'F32', value: 1.5 }), '1.5');
    assert.strictEqual(L.attrString({ type: 'ColorU', value: { r: 255, g: 0, b: 16, a: 255 } }), '#ff0010');
    assert.strictEqual(L.attrString({ type: 'ColorU', value: { r: 0, g: 0, b: 0, a: 128 } }), '#00000080');
    assert.strictEqual(L.attrString({ type: 'None' }), '');
    assert.strictEqual(L.attrString(null), '');
    // ...and the attribute text as the widget's value (null: nothing to show).
    assert.strictEqual(L.typedValue('true', 'Bool'), true);
    assert.strictEqual(L.typedValue('0', 'Bool'), false);
    assert.strictEqual(L.typedValue('12', 'I32'), 12);
    assert.strictEqual(L.typedValue('x', 'I32'), null);
    assert.strictEqual(L.typedValue('2.5', 'F64'), 2.5);
    assert.deepStrictEqual(L.typedValue('#ff0010', 'ColorU'), { r: 255, g: 0, b: 16, a: 255 });
    assert.deepStrictEqual(L.typedValue('#00000080', 'ColorU'), { r: 0, g: 0, b: 0, a: 128 });
    assert.strictEqual(L.typedValue('', 'String'), null);
    assert.strictEqual(L.typedValue('hi', 'String'), 'hi');
});

test('only the argument types one attribute can carry are editable in the panel', () => {
    ['String', 'Bool', 'I32', 'I64', 'U32', 'U64', 'Usize', 'F32', 'F64', 'ColorU'].forEach((t) =>
        assert.ok(L.editableType(t), t));
    ['Callback', 'Option', 'Vec', 'StyledDom', 'RefAny', 'StructRef', 'EnumRef', 'ImageRef'].forEach((t) =>
        assert.ok(!L.editableType(t), t));
});

// ── 2. the document's stylesheet ────────────────────────────────────────

test('applying the stylesheet sends builder_set_stylesheet, unless the text is what the document has', () => {
    const doc = { root: el(0, 'body'), stylesheet: '.a { color: red; }' };
    assert.deepStrictEqual(L.stylesheetMessage(doc, '.a { color: blue; }'),
        { op: 'builder_set_stylesheet', css: '.a { color: blue; }' });
    assert.strictEqual(L.stylesheetMessage(doc, '.a { color: red; }'), null);
    assert.deepStrictEqual(L.stylesheetMessage(doc, ''), { op: 'builder_set_stylesheet', css: '' },
        'emptying it is an edit too');
    assert.strictEqual(L.stylesheetMessage({ root: el(0, 'body') }, ''), null,
        'an old server answers no stylesheet: empty');
    assert.strictEqual(L.stylesheetMessage(null, 'x'), null);
});

test('the editor follows the document (undo, load) but never overwrites text not applied yet', () => {
    assert.strictEqual(L.sheetText('.a{}', '.b{}', false), '.b{}', 'clean: the document wins');
    assert.strictEqual(L.sheetText('.a{} /* typing */', '.b{}', true), '.a{} /* typing */', 'dirty: kept');
    assert.strictEqual(L.sheetText('x', undefined, false), '', 'no stylesheet in the answer: empty');
});

console.log('\n' + passed + ' passed, ' + failed + ' failed');
process.exit(failed ? 1 : 0);
