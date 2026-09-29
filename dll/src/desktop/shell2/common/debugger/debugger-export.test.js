// Unit tests for the pure logic of debugger-export.js (AzBuilder's quick
// export dialogs).
//
//     node dll/src/desktop/shell2/common/debugger/debugger-export.test.js
//
// No dependencies. Under node the module exports only its logic: which
// language a dialog opens on, which message each dialog sends, which
// component "Component → code" starts from, and the focus trap.
'use strict';

const assert = require('assert');
const L = require('./debugger-export.js');

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

test('the server language list is used as it comes, de-duplicated; the DOM list falls back to the four targets', () => {
    const list = L.languageOptions([
        { id: 'rust', label: 'Rust', ext: 'rs' },
        { id: 'rust', label: 'dup' },
        { id: '' },
        null,
        { id: 'go' },
    ], []);
    assert.deepStrictEqual(list, [
        { id: 'rust', label: 'Rust', ext: 'rs' },
        { id: 'go', label: 'go', ext: 'txt' },
    ]);
    assert.deepStrictEqual(L.languageOptions(undefined, L.DOM_FALLBACK).map((l) => l.id),
        ['rust', 'c', 'cpp', 'python']);
    assert.deepStrictEqual(L.languageOptions([], []), [], 'no CSS generators: nothing to offer');
});

test('a dialog opens on the language used last, if the server still has it', () => {
    const opts = L.languageOptions([{ id: 'rust' }, { id: 'cpp' }], []);
    assert.strictEqual(L.pickLanguage(opts, 'cpp'), 'cpp');
    assert.strictEqual(L.pickLanguage(opts, 'cobol'), 'rust');
    assert.strictEqual(L.pickLanguage(opts, null), 'rust');
    assert.strictEqual(L.pickLanguage([], 'rust'), null);
});

test('each stylesheet source asks get_css_rules for what the server can resolve', () => {
    assert.deepStrictEqual(L.rulesMessage({ kind: 'node', node: 7 }),
        { op: 'get_css_rules', source: 'node', node: 7 });
    assert.deepStrictEqual(L.rulesMessage({ kind: 'document' }),
        { op: 'get_css_rules', source: 'document' });
    assert.deepStrictEqual(L.rulesMessage({ kind: 'component', library: 'ui', name: 'pill' }),
        { op: 'get_css_rules', source: 'component', library: 'ui', name: 'pill' });
    assert.deepStrictEqual(L.rulesMessage({ kind: 'text', css: '.a{}' }),
        { op: 'get_css_rules', source: 'text', css: '.a{}' });
    assert.deepStrictEqual(L.rulesMessage(null), { op: 'get_css_rules', source: 'text', css: '' });
});

test('compile_css sends the CSS on screen and only the rules left ticked', () => {
    const css = '.a{color:red} .b{color:blue} .c{color:green}';
    assert.deepStrictEqual(L.compileCssMessage('rust', css, [2, 0], 3),
        { op: 'compile_css', language: 'rust', source: 'text', css, rules: [0, 2] });
    // All ticked: the whole sheet (no `rules`, so @keyframes stay in).
    assert.deepStrictEqual(L.compileCssMessage('rust', css, [0, 1, 2], 3),
        { op: 'compile_css', language: 'rust', source: 'text', css });
    assert.strictEqual(L.compileCssMessage('rust', css, [], 3), null, 'nothing ticked: nothing to send');
});

test('Subtree -> code sends the node, and the mode / name only when they are not the default', () => {
    assert.deepStrictEqual(L.subtreeMessage(3, 'c', 'function', '  '),
        { op: 'export_subtree_code', node: 3, language: 'c' });
    assert.deepStrictEqual(L.subtreeMessage(0, 'rust', 'app', 'ignored_by_the_server_for_apps'),
        { op: 'export_subtree_code', node: 0, language: 'rust', mode: 'app',
          function_name: 'ignored_by_the_server_for_apps' });
    assert.deepStrictEqual(L.subtreeMessage(5, 'python', 'function', ' build_card '),
        { op: 'export_subtree_code', node: 5, language: 'python', function_name: 'build_card' });
});

test('Component -> code lists user components first and opens on the selected instance', () => {
    const registry = { libraries: [
        { name: 'builtin', components: [{ tag: 'div', display_name: 'Div' }, { tag: 'p', display_name: 'Paragraph' }] },
        { name: 'user', components: [{ tag: 'my-card', display_name: 'My Card' }, { tag: 'badge', display_name: 'badge' }] },
    ] };
    const choices = L.componentChoices(registry);
    assert.deepStrictEqual(choices.map((c) => c.library + ':' + c.name),
        ['user:my-card', 'user:badge', 'builtin:div', 'builtin:p']);
    assert.strictEqual(choices[0].label, 'user:my-card — My Card');
    assert.strictEqual(choices[1].label, 'user:badge', 'no repeated display name');

    const instance = { uid: 4, kind: 'component', library: 'user', tag: 'badge', attrs: {}, children: [] };
    assert.strictEqual(L.defaultComponentChoice(choices, instance, 'builtin', 'p').name, 'badge');
    assert.strictEqual(L.defaultComponentChoice(choices, null, 'builtin', 'p').name, 'p',
        'else the one open in the Components view');
    assert.strictEqual(L.defaultComponentChoice(choices, null, null, null).name, 'my-card',
        'else the first user component');
    assert.strictEqual(L.defaultComponentChoice([], null, null, null), null);
    assert.deepStrictEqual(L.componentMessage(choices[0], 'cpp'),
        { op: 'export_component_code', library: 'user', name: 'my-card', language: 'cpp' });
});

test('a node is named by its tag, id and classes', () => {
    assert.strictEqual(L.nodeLabel({ uid: 3, kind: 'element', tag: 'div', attrs: { id: 'main x', class: ' card  big ' } }),
        'div#main.card.big  #3');
    assert.strictEqual(L.nodeLabel({ uid: 0, kind: 'element', tag: 'body', attrs: {} }), 'the whole document');
    assert.strictEqual(L.nodeLabel(null), 'the whole document');
    assert.strictEqual(L.nodeLabel({ uid: 7, kind: 'component', library: 'user', tag: 'my-card', attrs: {} }),
        'user:my-card  #7');
    assert.ok(L.nodeLabel({ uid: 9, kind: 'text', text: 'a very long text that goes on and on' }).startsWith('text "a very long'));
});

test('Tab cycles inside the dialog, Shift+Tab backwards, and wraps around', () => {
    assert.strictEqual(L.trapIndex(3, 0, false), 1);
    assert.strictEqual(L.trapIndex(3, 2, false), 0, 'wraps forward');
    assert.strictEqual(L.trapIndex(3, 0, true), 2, 'wraps backward');
    assert.strictEqual(L.trapIndex(3, -1, false), 0, 'focus outside: first');
    assert.strictEqual(L.trapIndex(3, -1, true), 2, 'focus outside, backwards: last');
    assert.strictEqual(L.trapIndex(0, -1, false), -1);
});

test('an op answer is unwrapped to its value, and a refusal throws its message', () => {
    assert.deepStrictEqual(L.unwrap({ status: 'ok', data: { type: 'json', value: { code: 'x' } } }, 'op'), { code: 'x' });
    assert.throws(() => L.unwrap({ status: 'error', message: 'no node with uid 9' }, 'op'), /uid 9/);
    assert.throws(() => L.unwrap(null, 'compile_css'), /compile_css/);
});

test('rule rows count declarations and mark conditional rules', () => {
    const rows = L.ruleRows([
        { index: 0, selector: '.a', declarations: 'color: red; margin-top: 1px;', conditional: false },
        { index: 1, selector: '', declarations: '', conditional: true },
    ]);
    assert.deepStrictEqual(rows.map((r) => [r.index, r.selector, r.count, r.conditional]),
        [[0, '.a', 2, false], [1, '(no selector)', 0, true]]);
});

console.log(`\n${passed} passed, ${failed} failed`);
process.exit(failed ? 1 : 0);
