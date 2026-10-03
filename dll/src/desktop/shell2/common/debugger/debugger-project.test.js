// Unit tests for the pure logic of debugger-project.js (AzBuilder's project
// tree and editor).
//
//     node dll/src/desktop/shell2/common/debugger/debugger-project.test.js
//
// No dependencies. The UI half of debugger-project.js needs a page and `app`;
// under node the module exports only its logic: what a project path IS (a
// component file, a stylesheet, a test…), how the tree flattens into rows,
// how a file is highlighted (and that its text can never become markup), and
// how the browser-side state (E2E tests, snapshots) maps to project files.
'use strict';

const assert = require('assert');
const L = require('./debugger-project.js');

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

// The tree project_list answers for a small project.
function tree() {
    const f = (path, size) => ({ name: path.split('/').pop(), path, kind: 'file', size: size || 1 });
    const d = (path, children) => ({ name: path.split('/').pop(), path, kind: 'dir', children });
    return {
        name: 'demo', path: '', kind: 'dir', children: [
            d('components', [d('components/user', [f('components/user/card.json')])]),
            d('styles', [f('styles/app.css')]),
            d('tests', []),
            f('azul-project.json'),
            f('document.json'),
        ],
    };
}

test('every project path has a role the tree and the sync key off', () => {
    assert.strictEqual(L.roleOf('azul-project.json'), 'manifest');
    assert.strictEqual(L.roleOf('document.json'), 'document');
    assert.strictEqual(L.roleOf('components/user/card.json'), 'component');
    assert.strictEqual(L.roleOf('styles/app.css'), 'stylesheet');
    assert.strictEqual(L.roleOf('styles/theme/dark.css'), 'stylesheet');
    assert.strictEqual(L.roleOf('tests/login.json'), 'test');
    assert.strictEqual(L.roleOf('snapshots/empty.json'), 'snapshot');
    assert.strictEqual(L.roleOf('export/rust/main.rs'), 'export');
    assert.strictEqual(L.roleOf('notes.txt'), 'other');
    // Only a file at components/<library>/<name>.json is a component.
    assert.strictEqual(L.roleOf('components/card.json'), 'other');
    assert.strictEqual(L.roleOf('styles/readme.md'), 'other');
});

test('a component file and a palette key name each other', () => {
    assert.strictEqual(L.componentKeyOf('components/user/card.json'), 'user:card');
    assert.strictEqual(L.componentKeyOf('components/user/sub/card.json'), null);
    assert.strictEqual(L.componentKeyOf('styles/app.css'), null);
    assert.strictEqual(L.componentPathOf('user', 'card'), 'components/user/card.json');
    assert.strictEqual(L.componentKeyOf(L.componentPathOf('lib-2', 'my_card')), 'lib-2:my_card');
});

test('the editor picks its language from the extension', () => {
    const cases = {
        'styles/app.css': 'css', 'document.json': 'json', 'ui/form.xml': 'xml',
        'index.html': 'html', 'export/rust/src/main.rs': 'rust', 'a.js': 'js', 'a.mjs': 'js',
        'main.c': 'c', 'main.h': 'c', 'main.cpp': 'cpp', 'main.hpp': 'cpp', 'app.py': 'python',
        'README.md': 'markdown', 'notes.txt': 'text', 'Makefile': 'text',
    };
    for (const [p, lang] of Object.entries(cases)) assert.strictEqual(L.languageOf(p), lang, p);
});

test('file text is always escaped: a file can never inject markup into the page', () => {
    const evil = '<script>alert(1)</script><img src=x onerror=alert(2)> & "q"';
    for (const lang of ['text', 'css', 'json', 'xml', 'html', 'rust', 'js', 'markdown']) {
        const html = L.highlight(evil, lang);
        assert.ok(!/<script/i.test(html), lang + ': raw <script> in ' + html);
        assert.ok(!/<img/i.test(html), lang + ': raw <img> in ' + html);
        // Stripping our own spans gives back the escaped text.
        const text = html.replace(/<\/?span[^>]*>/g, '');
        assert.strictEqual(text, L.escapeHtml(evil), lang);
    }
});

test('highlighting marks the tokens a reader looks for', () => {
    const css = L.highlight('/* c */\n.card > h1 { color: #ff0000; width: 12px; }\n@media print {}', 'css');
    assert.ok(/azp-tok-comment[^>]*>\/\* c \*\//.test(css), css);
    assert.ok(/azp-tok-prop[^>]*>color/.test(css), css);
    assert.ok(/azp-tok-number[^>]*>#ff0000/.test(css), css);
    assert.ok(/azp-tok-number[^>]*>12px/.test(css), css);
    assert.ok(/azp-tok-keyword[^>]*>@media/.test(css), css);

    const json = L.highlight('{ "name": "card", "n": 2, "ok": true }', 'json');
    assert.ok(/azp-tok-attr[^>]*>&quot;name&quot;/.test(json), json);
    assert.ok(/azp-tok-string[^>]*>&quot;card&quot;/.test(json), json);
    assert.ok(/azp-tok-number[^>]*>2/.test(json), json);
    assert.ok(/azp-tok-keyword[^>]*>true/.test(json), json);

    const xml = L.highlight('<!-- x --><div class="a">{text}</div>', 'xml');
    assert.ok(/azp-tok-comment[^>]*>&lt;!-- x --&gt;/.test(xml), xml);
    assert.ok(/azp-tok-tag[^>]*>&lt;div/.test(xml), xml);
    assert.ok(/azp-tok-attr[^>]*>class/.test(xml), xml);
    assert.ok(/azp-tok-string[^>]*>&quot;a&quot;/.test(xml), xml);
    assert.ok(/azp-tok-placeholder[^>]*>\{text\}/.test(xml), xml);

    const rs = L.highlight('fn main() { let s = "x"; } // done', 'rust');
    assert.ok(/azp-tok-keyword[^>]*>fn/.test(rs) && /azp-tok-keyword[^>]*>let/.test(rs), rs);
    assert.ok(/azp-tok-comment[^>]*>\/\/ done/.test(rs), rs);
    // A keyword inside a string or comment stays part of it.
    assert.ok(!/azp-tok-keyword[^>]*>let<\/span>&quot;/.test(L.highlight('"let"', 'rust')));
});

test('the tree flattens into rows: folders first as given, collapsed folders hide their children', () => {
    const t = tree();
    let rows = L.flattenTree(t, new Set(['']));
    assert.deepStrictEqual(rows.map((r) => r.path),
        ['', 'components', 'styles', 'tests', 'azul-project.json', 'document.json']);
    assert.strictEqual(rows[0].depth, 0);
    assert.strictEqual(rows[1].depth, 1);
    assert.ok(rows[1].hasChildren && !rows[1].expanded);
    assert.ok(!rows[3].hasChildren, 'an empty folder has nothing to expand');
    rows = L.flattenTree(t, new Set(['', 'components', 'components/user']));
    assert.deepStrictEqual(rows.map((r) => r.path).slice(0, 4),
        ['', 'components', 'components/user', 'components/user/card.json']);
    assert.strictEqual(rows[3].depth, 3);
    // The root collapsed: only the root row.
    assert.deepStrictEqual(L.flattenTree(t, new Set()).map((r) => r.path), ['']);
});

test('paths: find, parents, ancestors, join, rename', () => {
    const t = tree();
    assert.strictEqual(L.findEntry(t, 'styles/app.css').kind, 'file');
    assert.strictEqual(L.findEntry(t, 'styles/nope.css'), null);
    assert.strictEqual(L.findEntry(t, '').kind, 'dir');
    assert.ok(L.hasPath(t, 'components/user/card.json'));
    assert.strictEqual(L.parentPath('components/user/card.json'), 'components/user');
    assert.strictEqual(L.parentPath('document.json'), '');
    assert.deepStrictEqual(L.ancestorsOf('components/user/card.json'), ['', 'components', 'components/user']);
    assert.strictEqual(L.baseName('components/user/card.json'), 'card.json');
    assert.strictEqual(L.joinPath('', 'a.css'), 'a.css');
    assert.strictEqual(L.joinPath('styles', 'a.css'), 'styles/a.css');
    assert.strictEqual(L.renamedPath('styles/a.css', 'b.css'), 'styles/b.css');
    assert.strictEqual(L.renamedPath('a.css', 'b.css'), 'b.css');
});

test('a new name is one plain segment (the server re-checks every path anyway)', () => {
    for (const ok of ['app.css', 'my card.json', 'a-b_c.rs', '.gitignore']) assert.ok(L.isValidName(ok), ok);
    for (const bad of ['', '.', '..', 'a/b', 'a\\b', 'x\0y', '   ']) assert.ok(!L.isValidName(bad), JSON.stringify(bad));
});

test('E2E tests round-trip through their project files (one test per file, CLI format)', () => {
    const t = {
        id: 17, name: 'Log in', steps: [
            { op: 'click', params: { selector: '#login' }, breakpoint: true },
            { op: 'assert_text', params: { selector: '#msg', expected: 'Hi' }, breakpoint: false },
        ],
    };
    const file = L.testFile(t);
    assert.deepStrictEqual(file, {
        name: 'Log in', steps: [
            { op: 'click', selector: '#login' },
            { op: 'assert_text', selector: '#msg', expected: 'Hi' },
        ],
    });
    const back = L.testFromFile(file, 'fallback');
    assert.strictEqual(back.name, 'Log in');
    assert.deepStrictEqual(back.steps.map((s) => [s.op, s.params]),
        [['click', { selector: '#login' }], ['assert_text', { selector: '#msg', expected: 'Hi' }]]);
    assert.strictEqual(L.testFromFile({ steps: [] }, 'tests/smoke.json').name, 'smoke');
    assert.deepStrictEqual(
        L.testPaths([{ name: 'Log in' }, { name: 'log-in' }, { name: '' }, { name: '../x' }]),
        ['tests/log-in.json', 'tests/log-in-2.json', 'tests/test.json', 'tests/x.json']);
});

test('the project tests replace browser tests of the same name and keep the others', () => {
    const cur = [{ id: 1, name: 'A', steps: [] }, { id: 2, name: 'B', steps: [] }];
    const loaded = [{ name: 'B', steps: [{ op: 'get_state', params: {} }] }, { name: 'C', steps: [] }];
    const merged = L.mergeTests(cur, loaded);
    assert.deepStrictEqual(merged.map((t) => t.name), ['A', 'B', 'C']);
    assert.strictEqual(merged[1].steps.length, 1);
    assert.strictEqual(merged[1].id, 2, 'a replaced test keeps its id (the selection stays on it)');
    assert.ok(merged[2].id != null && merged[2].id !== 1 && merged[2].id !== 2);
});

test('snapshots round-trip through their project files', () => {
    assert.strictEqual(L.snapshotPath('Empty cart!'), 'snapshots/empty-cart.json');
    const f = L.snapshotFile('Empty cart!', { items: [] });
    assert.deepStrictEqual(L.snapshotFromFile(f, 'snapshots/empty-cart.json'), { alias: 'Empty cart!', state: { items: [] } });
    // A hand-written file: the whole JSON is the state, the file name the alias.
    assert.deepStrictEqual(L.snapshotFromFile({ count: 3 }, 'snapshots/three.json'), { alias: 'three', state: { count: 3 } });
});

test('a new file starts from a template that fits where it is created', () => {
    const comp = JSON.parse(L.newFileTemplate('components/user/badge.json'));
    assert.strictEqual(comp.format, 'azul-component');
    assert.strictEqual(comp.library, 'user');
    assert.strictEqual(comp.name, 'badge');
    assert.ok(typeof comp.template === 'string' && comp.template.includes('{text}'));
    assert.ok(Array.isArray(comp.fields) && comp.fields[0].name === 'text');
    assert.ok(/\/\*/.test(L.newFileTemplate('styles/theme.css')));
    const t = JSON.parse(L.newFileTemplate('tests/smoke.json'));
    assert.strictEqual(t.name, 'smoke');
    assert.ok(Array.isArray(t.steps));
    assert.strictEqual(L.newFileTemplate('notes.txt'), '');
});

test('icons: folders open and close, roles and extensions get their own', () => {
    assert.strictEqual(L.iconOf({ kind: 'dir', path: 'x' }, false), 'folder');
    assert.strictEqual(L.iconOf({ kind: 'dir', path: 'x' }, true), 'folder_open');
    assert.strictEqual(L.iconOf({ kind: 'file', path: 'components/user/card.json' }), 'widgets');
    assert.strictEqual(L.iconOf({ kind: 'file', path: 'styles/app.css' }), 'palette');
    assert.strictEqual(L.iconOf({ kind: 'file', path: 'document.json' }), 'account_tree');
    assert.strictEqual(L.iconOf({ kind: 'file', path: 'tests/a.json' }), 'bug_report');
    assert.strictEqual(L.iconOf({ kind: 'file', path: 'shot.png' }), 'image');
    assert.strictEqual(L.iconOf({ kind: 'link', path: 'escape' }), 'link');
    assert.ok(L.isImagePath('a/b.PNG') && L.isImagePath('x.svg') && !L.isImagePath('x.css'));
});

console.log(`\n${passed} passed, ${failed} failed`);
process.exit(failed ? 1 : 0);
