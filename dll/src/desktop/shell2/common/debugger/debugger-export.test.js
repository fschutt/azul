// Unit tests for the pure logic of debugger-export.js (AzBuilder's quick
// export dialogs).
//
//     node dll/src/desktop/shell2/common/debugger/debugger-export.test.js
//
// No dependencies. Under node the module exports only its logic: the Export
// menu (Compile > CSS… / DOM…, Subtree as Component…, Components…, Code (ZIP)
// > every language), the ONE language list, which language a dialog opens on,
// which message each dialog sends, which component "Components…" starts from
// and whether its library exports as JSON, how a parse error reads, and the
// focus trap.
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

test('the server language list is used as it comes, de-duplicated; there is no second list in the page', () => {
    const list = L.languageOptions([
        { id: 'rust', label: 'Rust', ext: 'rs', dom: true },
        { id: 'rust', label: 'dup' },
        { id: '' },
        null,
        { id: 'go' },
    ], []);
    assert.deepStrictEqual(list, [
        { id: 'rust', label: 'Rust', ext: 'rs', dom: true },
        { id: 'go', label: 'go', ext: 'txt', dom: false },
    ]);
    assert.strictEqual(L.DOM_FALLBACK, undefined, 'no hard-coded language list: azul_css::codegen::all_backends() is THE list');
    assert.deepStrictEqual(L.languageOptions(undefined), []);
    assert.deepStrictEqual(L.languageOptions([], []), [], 'no code generators: nothing to offer');
});

test('get_codegen_languages is ONE list: the CSS dialog offers all of it, the DOM dialogs disable what does no DOM export', () => {
    const v = L.dialogLanguages({ languages: [
        { id: 'rust', label: 'Rust', ext: 'rs', dom: true, no_dom_reason: null },
        { id: 'c', label: 'C', ext: 'h', dom: true },
        { id: 'java', label: 'Java', ext: 'java', dom: false,
          no_dom_reason: 'the Java printer does not print DOM construction yet' },
    ] });
    assert.deepStrictEqual(v.css.map((l) => l.id), ['rust', 'c', 'java']);
    assert.deepStrictEqual(v.dom.map((l) => l.id), ['rust', 'c', 'java'], 'the same list');
    assert.deepStrictEqual(v.dom.map((l) => !!l.disabled), [false, false, true]);
    assert.strictEqual(v.dom[2].label, 'Java (no DOM export yet)');
    assert.strictEqual(v.dom[2].reason, 'the Java printer does not print DOM construction yet',
        'the server says why (azul_core::codegen::dom_warning)');
    assert.strictEqual(v.dom[0].reason, undefined);
    // A remembered language the DOM dialog cannot use falls back to a usable one.
    assert.strictEqual(L.pickLanguage(v.dom, 'java'), 'rust');
    assert.strictEqual(L.pickLanguage(v.css, 'java'), 'java');
    // No answer: nothing to offer (the dialogs say the server did not list its languages).
    assert.deepStrictEqual(L.dialogLanguages(null), { css: [], dom: [] });
    // An older server without reasons: a generic one.
    const old = L.dialogLanguages({ languages: [{ id: 'cobol', label: 'COBOL', dom: false }] });
    assert.strictEqual(old.dom[0].reason, 'its printer does not export a DOM yet');
});

test('a dialog opens on the language used last, if the server still has it', () => {
    const opts = L.languageOptions([{ id: 'rust' }, { id: 'cpp' }], []);
    assert.strictEqual(L.pickLanguage(opts, 'cpp'), 'cpp');
    assert.strictEqual(L.pickLanguage(opts, 'klingon'), 'rust');
    assert.strictEqual(L.pickLanguage(opts, null), 'rust');
    assert.strictEqual(L.pickLanguage([], 'rust'), null);
});

test('an app answer is a project: its files, the one in file_name first', () => {
    const v = {
        file_name: 'main.rs',
        code: 'fn main() {}',
        files: [
            { path: 'Cargo.toml', contents: '[package]' },
            { path: 'src/ui.rs', contents: 'pub fn render_ui() -> Dom' },
            { path: 'src/main.rs', contents: 'fn main() {}' },
            { path: 'broken' },
        ],
    };
    assert.deepStrictEqual(L.projectFiles(v).map((f) => f.path), ['src/main.rs', 'Cargo.toml', 'src/ui.rs']);
    assert.deepStrictEqual(L.projectFiles({ file_name: 'x.rs', code: '', files: [] }), []);
    assert.deepStrictEqual(L.projectFiles({ file_name: 'x.rs', code: '' }), []);
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

test('HTML -> code sends the pasted text, and the mode / name / CSS only when they are not the default', () => {
    assert.deepStrictEqual(L.htmlMessage('<p>x</p>', 'rust', 'function', '  ', false),
        { op: 'html_to_code', html: '<p>x</p>', language: 'rust' });
    assert.deepStrictEqual(L.htmlMessage('<p>x</p>', 'c', 'function', ' build ', true),
        { op: 'html_to_code', html: '<p>x</p>', language: 'c', function_name: 'build', css: true });
    assert.deepStrictEqual(L.htmlMessage('<p>x</p>', 'python', 'app', 'not for an app', false),
        { op: 'html_to_code', html: '<p>x</p>', language: 'python', mode: 'app' });
    assert.strictEqual(L.htmlMessage('  \n ', 'rust', 'function', '', false), null,
        'nothing pasted: nothing to send');
});

test('a parse error names its line and column', () => {
    assert.strictEqual(L.parseErrorText({ message: 'Invalid attribute', line: 4, column: 10 }),
        'line 4, column 10: Invalid attribute');
    assert.strictEqual(L.parseErrorText({ message: 'unclosed root node', line: null, column: null }),
        'unclosed root node');
    assert.strictEqual(L.parseErrorText({ line: 2 }), 'line 2: the markup does not parse');
    assert.strictEqual(L.parseErrorText(null), '');
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

// ── B7: the Export menu ──

test('the Export menu is Compile > (CSS, DOM), Subtree as Component, Components, Code (ZIP) > every language', () => {
    const langs = L.dialogLanguages({ languages: [
        { id: 'rust', label: 'Rust', ext: 'rs', dom: true },
        { id: 'go', label: 'Go', ext: 'go', dom: true },
        { id: 'cobol', label: 'COBOL', ext: 'cob', dom: false, no_dom_reason: 'the COBOL printer does not print DOM construction yet' },
    ] });
    const menu = L.exportMenu(langs);
    const shape = menu.map((m) => m.submenu ? m.label + ' > ' + m.items.map((i) => i.label).join(', ') : m.label);
    assert.deepStrictEqual(shape, [
        'Compile > CSS…, DOM…',
        'Subtree as Component…',
        'Components…',
        'Code (ZIP) > Rust, Go, COBOL (no DOM export yet)',
    ]);
    // Each item is the action of an existing dialog (the smoke clicks them by these).
    assert.deepStrictEqual(menu[0].items.map((i) => i.act), ['css', 'html']);
    assert.deepStrictEqual([menu[1].act, menu[2].act], ['subtree', 'component']);
    // Code (ZIP): the ONE list; what cannot build a UI stays listed, disabled, with its reason.
    const zip = menu[3].items;
    assert.deepStrictEqual(zip.map((i) => [i.act, i.lang, !!i.disabled]),
        [['zip', 'rust', false], ['zip', 'go', false], ['zip', 'cobol', true]]);
    assert.strictEqual(zip[2].reason, 'the COBOL printer does not print DOM construction yet');
    // No language list from the server: one disabled line saying so, never a guessed list.
    const none = L.exportMenu(L.dialogLanguages(null))[3].items;
    assert.strictEqual(none.length, 1);
    assert.ok(none[0].disabled && none[0].lang == null && /did not list/.test(none[0].label), JSON.stringify(none));
});

test('Components exports a user library as JSON; a builtin library cannot be', () => {
    const registry = { libraries: [
        { name: 'builtin', exportable: false, components: [{ tag: 'p' }] },
        { name: 'user', exportable: true, components: [{ tag: 'card' }] },
        { name: 'old' },
    ] };
    assert.strictEqual(L.libraryExportable(registry, 'user'), true);
    assert.strictEqual(L.libraryExportable(registry, 'builtin'), false);
    assert.strictEqual(L.libraryExportable(registry, 'old'), true, 'a registry without the flag: any non-builtin');
    assert.strictEqual(L.libraryExportable(registry, 'nope'), false);
    assert.strictEqual(L.libraryExportable(null, 'user'), false);
});

console.log(`\n${passed} passed, ${failed} failed`);
process.exit(failed ? 1 : 0);
