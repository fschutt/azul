/**
 * AzBuilder project viewer — loaded after debugger.js (next to debugger-dnd.js).
 *
 * A Qt-Creator-style project: a FOLDER on disk (opened through the debug
 * server, layout/src/e2e/project.rs) shown as a tree with an editor:
 *
 *   - "Project" activity: the tree (folders, file icons, context menu:
 *     new file / new folder / rename / delete / copy path), editor tabs with
 *     syntax highlighting (CSS, JSON, XML/HTML, Rust, C/C++, JS, Python,
 *     Markdown), Ctrl/Cmd+S saves, the status bar says what the save applied.
 *   - A compact Project section under the Inspector's palette: the same tree
 *     next to the Document tree.
 *   - Sync: selecting a component file (components/<library>/<name>.json)
 *     selects its palette card and its first instance in the Document; selecting
 *     an instance or a palette card selects its file. A component file drags
 *     into the Document tree like a palette card.
 *   - Project menu: open / create, Save Project (the document, every user
 *     component, the E2E tests and snapshots), Load Project, Export / Import ZIP,
 *     Close. The last project re-opens after a reload, and loads into a fresh
 *     AzBuilder window (an empty body, no document).
 *   - The App State camera button saves a snapshot (it used to call a
 *     function that does not exist), into the project too.
 *
 * Server messages: project_info, project_open, project_close, project_list,
 * project_read_file, project_write_file, project_create, project_rename,
 * project_delete, project_save, project_load, project_export_zip,
 * project_import_zip, builder_get_document.
 *
 * The pure logic at the top has no DOM dependency and is unit-tested under node:
 *     node dll/src/desktop/shell2/common/debugger/debugger-project.test.js
 */
(function (root) {
    'use strict';

    // =====================================================================
    // Pure logic
    // =====================================================================

    /** `components/<library>/<name>.json` → `library:name`, else null. */
    function componentKeyOf(path) {
        var m = /^components\/([^/]+)\/([^/]+)\.json$/i.exec(String(path || ''));
        return m ? m[1] + ':' + m[2] : null;
    }

    function componentPathOf(library, name) {
        return 'components/' + library + '/' + name + '.json';
    }

    /** What a project path is — the tree's icons and the sync key off it. */
    function roleOf(path) {
        var p = String(path || '');
        if (p === 'azul-project.json') return 'manifest';
        if (p === 'document.json') return 'document';
        if (componentKeyOf(p)) return 'component';
        var lower = p.toLowerCase();
        if (lower.indexOf('styles/') === 0 && /\.css$/.test(lower)) return 'stylesheet';
        if (lower.indexOf('tests/') === 0 && /\.json$/.test(lower)) return 'test';
        if (lower.indexOf('snapshots/') === 0 && /\.json$/.test(lower)) return 'snapshot';
        if (lower.indexOf('export/') === 0) return 'export';
        return 'other';
    }

    function extOf(path) {
        var base = baseName(path);
        var i = base.lastIndexOf('.');
        return i > 0 ? base.slice(i + 1).toLowerCase() : '';
    }

    var LANGS = {
        css: 'css', json: 'json', xml: 'xml', ui: 'xml', svg: 'xml', html: 'html', htm: 'html',
        rs: 'rust', js: 'js', mjs: 'js', cjs: 'js', ts: 'js', c: 'c', h: 'c', cpp: 'cpp', cc: 'cpp',
        cxx: 'cpp', hpp: 'cpp', hh: 'cpp', py: 'python', md: 'markdown', markdown: 'markdown',
        toml: 'text', txt: 'text',
    };

    /** The editor's highlighting language for a path. */
    function languageOf(path) {
        return LANGS[extOf(path)] || 'text';
    }

    function isImagePath(path) {
        return /^(png|jpe?g|gif|webp|bmp|ico|svg)$/.test(extOf(path));
    }

    function escapeHtml(s) {
        return String(s == null ? '' : s)
            .replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
            .replace(/"/g, '&quot;').replace(/'/g, '&#39;');
    }

    // Highlighting rules: [token class, regex source]. No capturing groups in
    // a source (only (?:…) and lookarounds): group i of the combined regex is
    // rule i. `null` = consume without a class (keeps keywords out of words).
    var STRING = '"(?:[^"\\\\\\n]|\\\\.)*"|\'(?:[^\'\\\\\\n]|\\\\.)*\'';
    var C_COMMENT = '\\/\\/[^\\n]*|\\/\\*[\\s\\S]*?(?:\\*\\/|$)';
    var NUMBER = '\\b\\d[\\d_]*(?:\\.\\d+)?(?:[eE][+-]?\\d+)?[A-Za-z0-9]*';
    function words(list) { return '\\b(?:' + list.join('|') + ')\\b'; }
    var KW = {
        rust: ['as', 'async', 'await', 'break', 'const', 'continue', 'crate', 'dyn', 'else', 'enum',
            'extern', 'false', 'fn', 'for', 'if', 'impl', 'in', 'let', 'loop', 'match', 'mod', 'move',
            'mut', 'pub', 'ref', 'return', 'self', 'Self', 'static', 'struct', 'super', 'trait', 'true',
            'type', 'unsafe', 'use', 'where', 'while', 'Some', 'None', 'Ok', 'Err'],
        c: ['auto', 'bool', 'break', 'case', 'char', 'const', 'continue', 'default', 'do', 'double',
            'else', 'enum', 'extern', 'false', 'float', 'for', 'if', 'int', 'long', 'NULL', 'return',
            'short', 'signed', 'sizeof', 'static', 'struct', 'switch', 'true', 'typedef', 'union',
            'unsigned', 'void', 'while', '#include', '#define'],
        cpp: ['auto', 'bool', 'break', 'case', 'char', 'class', 'const', 'constexpr', 'continue',
            'default', 'delete', 'do', 'double', 'else', 'enum', 'false', 'float', 'for', 'if', 'int',
            'namespace', 'new', 'nullptr', 'override', 'private', 'protected', 'public', 'return',
            'static', 'struct', 'switch', 'template', 'this', 'true', 'typename', 'using', 'virtual',
            'void', 'while'],
        js: ['async', 'await', 'break', 'case', 'catch', 'class', 'const', 'continue', 'default',
            'else', 'export', 'false', 'for', 'function', 'if', 'import', 'in', 'let', 'new', 'null',
            'of', 'return', 'switch', 'this', 'throw', 'true', 'try', 'typeof', 'undefined', 'var',
            'while'],
        python: ['and', 'as', 'class', 'def', 'elif', 'else', 'except', 'False', 'for', 'from', 'if',
            'import', 'in', 'is', 'lambda', 'None', 'not', 'or', 'pass', 'raise', 'return', 'self',
            'True', 'try', 'while', 'with', 'yield'],
    };
    function codeRules(lang, comment) {
        return [
            ['comment', comment || C_COMMENT],
            ['string', STRING],
            ['keyword', words(KW[lang])],
            ['number', NUMBER],
            [null, '[A-Za-z_]\\w*'],
        ];
    }
    var RULES = {
        css: [
            ['comment', '\\/\\*[\\s\\S]*?(?:\\*\\/|$)'],
            ['string', STRING],
            ['keyword', '@[\\w-]+'],
            ['prop', '(?<=(?:^|[{;])\\s*)-?[A-Za-z][\\w-]*(?=\\s*:)'],
            ['number', '#[0-9a-fA-F]{3,8}(?![\\w-])'],
            [null, '[A-Za-z_][\\w-]*'],
            ['number', '-?(?:\\d+\\.?\\d*|\\.\\d+)(?:[A-Za-z]+|%)?'],
        ],
        json: [
            ['attr', '"(?:[^"\\\\\\n]|\\\\.)*"(?=\\s*:)'],
            ['string', '"(?:[^"\\\\\\n]|\\\\.)*"'],
            ['keyword', '\\b(?:true|false|null)\\b'],
            ['number', '-?\\d+(?:\\.\\d+)?(?:[eE][+-]?\\d+)?'],
        ],
        xml: [
            ['comment', '<!--[\\s\\S]*?(?:-->|$)'],
            ['tag', '<\\/?[A-Za-z][\\w:.-]*'],
            ['tag', '\\/?>'],
            ['attr', '[A-Za-z_:][\\w:.-]*(?=\\s*=)'],
            ['string', '"[^"]*"|\'[^\']*\''],
            ['placeholder', '\\{[A-Za-z_]\\w*\\}'],
        ],
        rust: codeRules('rust'),
        c: codeRules('c'),
        cpp: codeRules('cpp'),
        js: codeRules('js'),
        python: codeRules('python', '#[^\\n]*'),
        markdown: [
            ['keyword', '^#{1,6} [^\\n]*'],
            ['string', '`[^`\\n]*`'],
            ['comment', '^> [^\\n]*'],
        ],
    };
    RULES.html = RULES.xml;
    var COMPILED = {};

    /** HTML for `code` with its tokens in spans. Every character is escaped. */
    function highlight(code, lang) {
        var src = String(code == null ? '' : code);
        var rules = RULES[lang];
        if (!rules || src.length > 400000) return escapeHtml(src);
        var re = COMPILED[lang];
        if (!re) {
            re = new RegExp(rules.map(function (r) { return '(' + r[1] + ')'; }).join('|'), 'gm');
            COMPILED[lang] = re;
        }
        re.lastIndex = 0;
        var out = '';
        var last = 0;
        var m;
        while ((m = re.exec(src)) !== null) {
            if (m[0] === '') { re.lastIndex++; continue; }
            var cls = null;
            for (var i = 1; i < m.length; i++) {
                if (m[i] !== undefined) { cls = rules[i - 1][0]; break; }
            }
            out += escapeHtml(src.slice(last, m.index));
            out += cls ? '<span class="azp-tok-' + cls + '">' + escapeHtml(m[0]) + '</span>' : escapeHtml(m[0]);
            last = m.index + m[0].length;
        }
        return out + escapeHtml(src.slice(last));
    }

    /** The tree as rows; a folder's children show only when it is expanded. */
    function flattenTree(tree, expanded) {
        var rows = [];
        if (!tree) return rows;
        (function walk(entry, depth) {
            var kids = entry.kind === 'dir' ? (entry.children || []) : [];
            var open = entry.kind === 'dir' && !!expanded && expanded.has(entry.path);
            rows.push({
                path: entry.path, name: entry.name, kind: entry.kind, depth: depth, entry: entry,
                hasChildren: kids.length > 0, expanded: open,
            });
            if (open) kids.forEach(function (c) { walk(c, depth + 1); });
        })(tree, 0);
        return rows;
    }

    function findEntry(tree, path) {
        if (!tree) return null;
        if (tree.path === path) return tree;
        var kids = tree.children || [];
        for (var i = 0; i < kids.length; i++) {
            if (path === kids[i].path || path.indexOf(kids[i].path + '/') === 0) {
                var f = findEntry(kids[i], path);
                if (f) return f;
            }
        }
        return null;
    }

    function hasPath(tree, path) { return !!findEntry(tree, path); }

    function parentPath(path) {
        var i = String(path).lastIndexOf('/');
        return i < 0 ? '' : path.slice(0, i);
    }

    function baseName(path) {
        var s = String(path || '');
        return s.slice(s.lastIndexOf('/') + 1);
    }

    function stem(path) {
        var b = baseName(path);
        var i = b.lastIndexOf('.');
        return i > 0 ? b.slice(0, i) : b;
    }

    /** The folders above a path, root ('') first. */
    function ancestorsOf(path) {
        var out = [''];
        var parts = String(path || '').split('/');
        for (var i = 1; i < parts.length; i++) out.push(parts.slice(0, i).join('/'));
        return out;
    }

    function joinPath(dir, name) { return dir ? dir + '/' + name : name; }

    function renamedPath(path, newName) { return joinPath(parentPath(path), newName); }

    /** A new file / folder name is ONE plain segment. */
    function isValidName(name) {
        if (typeof name !== 'string') return false;
        var n = name.trim();
        return n !== '' && n !== '.' && n !== '..' && !/[\/\\\0]/.test(n);
    }

    function slug(s, fallback) {
        var out = String(s || '').toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-+|-+$/g, '');
        return out || fallback || 'item';
    }

    function titleCase(name) {
        return String(name || '').split(/[-_\s]+/).filter(Boolean)
            .map(function (w) { return w.charAt(0).toUpperCase() + w.slice(1); }).join(' ');
    }

    /** A browser-side E2E test as its project file (the CLI format). */
    function testFile(t) {
        return {
            name: t.name,
            steps: (t.steps || []).map(function (s) {
                var step = { op: s.op };
                var params = s.params || {};
                Object.keys(params).forEach(function (k) { if (k !== 'op') step[k] = params[k]; });
                return step;
            }),
        };
    }

    /** A project test file as a browser-side test. */
    function testFromFile(json, pathOrName) {
        var name = (json && json.name) || stem(pathOrName) || 'Test';
        return {
            name: name,
            steps: ((json && json.steps) || []).map(function (s) {
                var params = {};
                Object.keys(s || {}).forEach(function (k) { if (k !== 'op') params[k] = s[k]; });
                return { op: s.op, params: params, breakpoint: false };
            }),
        };
    }

    /** One unique file per test: tests/<slug>.json. */
    function testPaths(tests) {
        var used = {};
        return (tests || []).map(function (t) {
            var base = slug(t && t.name, 'test');
            var name = base;
            var n = 2;
            while (used[name]) name = base + '-' + (n++);
            used[name] = true;
            return 'tests/' + name + '.json';
        });
    }

    /** Project tests replace browser tests of the same name (keeping their id). */
    function mergeTests(current, loaded) {
        var out = (current || []).slice();
        (loaded || []).forEach(function (t, i) {
            var at = -1;
            for (var j = 0; j < out.length; j++) if (out[j].name === t.name) { at = j; break; }
            if (at >= 0) {
                out[at] = Object.assign({}, t, { id: out[at].id });
            } else {
                out.push(Object.assign({}, t, { id: Date.now() + i + Math.random() }));
            }
        });
        return out;
    }

    function snapshotPath(alias) { return 'snapshots/' + slug(alias, 'snapshot') + '.json'; }

    function snapshotFile(alias, state) { return { alias: alias, state: state }; }

    function snapshotFromFile(json, path) {
        if (json && typeof json === 'object' && !Array.isArray(json)
            && Object.prototype.hasOwnProperty.call(json, 'alias')
            && Object.prototype.hasOwnProperty.call(json, 'state')) {
            return { alias: String(json.alias), state: json.state };
        }
        return { alias: stem(path), state: json };
    }

    /** What a new file starts with, by where it is created. */
    function newFileTemplate(path) {
        var key = componentKeyOf(path);
        if (key) {
            var lib = key.split(':')[0];
            var name = key.split(':')[1];
            return JSON.stringify({
                format: 'azul-component', version: 1, library: lib, name: name,
                display_name: titleCase(name), description: '',
                fields: [{ name: 'text', type: 'String', default: titleCase(name), description: 'The text' }],
                css: '.' + name + ' { padding: 4px; }',
                template: '<div class="' + name + '">{text}</div>',
            }, null, 2) + '\n';
        }
        var role = roleOf(path);
        if (role === 'stylesheet' || extOf(path) === 'css') {
            return '/* ' + baseName(path) + ' - styles/*.css apply to the builder document, in path order */\n';
        }
        if (role === 'test') {
            return JSON.stringify({ name: stem(path), steps: [{ op: 'get_state' }] }, null, 2) + '\n';
        }
        if (role === 'snapshot') {
            return JSON.stringify({ alias: stem(path), state: {} }, null, 2) + '\n';
        }
        if (path === 'document.json') {
            return JSON.stringify({ format: 'azul-builder-document', version: 1,
                root: { kind: 'element', tag: 'body', attrs: {}, children: [] } }, null, 2) + '\n';
        }
        return '';
    }

    /** The Material icon for a tree entry. */
    function iconOf(entry, expanded) {
        if (!entry) return 'description';
        if (entry.kind === 'link') return 'link';
        if (entry.kind === 'dir') return expanded ? 'folder_open' : 'folder';
        switch (roleOf(entry.path)) {
            case 'component': return 'widgets';
            case 'stylesheet': return 'palette';
            case 'document': return 'account_tree';
            case 'test': return 'bug_report';
            case 'snapshot': return 'photo_camera';
            case 'manifest': return 'settings';
            default: break;
        }
        if (isImagePath(entry.path)) return 'image';
        switch (languageOf(entry.path)) {
            case 'json': return 'data_object';
            case 'markdown': return 'article';
            case 'text': return 'description';
            case 'js': return 'javascript';
            default: return 'code';
        }
    }

    var logic = {
        roleOf: roleOf, componentKeyOf: componentKeyOf, componentPathOf: componentPathOf,
        languageOf: languageOf, isImagePath: isImagePath, escapeHtml: escapeHtml, highlight: highlight,
        flattenTree: flattenTree, findEntry: findEntry, hasPath: hasPath, parentPath: parentPath,
        baseName: baseName, ancestorsOf: ancestorsOf, joinPath: joinPath, renamedPath: renamedPath,
        isValidName: isValidName, slug: slug, testFile: testFile, testFromFile: testFromFile,
        testPaths: testPaths, mergeTests: mergeTests, snapshotPath: snapshotPath,
        snapshotFile: snapshotFile, snapshotFromFile: snapshotFromFile,
        newFileTemplate: newFileTemplate, iconOf: iconOf,
    };

    if (typeof module !== 'undefined' && module.exports) module.exports = logic;
    // Under node (the unit test) there is no page and no `app`: logic only.
    if (typeof document === 'undefined' || typeof app === 'undefined') return;

    // =====================================================================
    // Browser UI
    // =====================================================================

    var STORE_KEY = 'azul_builder_project';
    var PATH_MIME = 'application/x-azul-project-path';
    var MAX_RECENT = 8;

    var S = {
        ready: false,
        info: null,              // project_info / project_open answer
        tree: null,
        expanded: new Set(['']),
        selected: null,          // selected tree path
        tabs: [],                // {path, lang, content, text, dirty, binary, size}
        active: null,            // active tab path
        last: null,              // the project to re-open after a reload
        recent: [],
        miniOpen: true,
        syncing: false,
        dragPath: null,
    };

    function isOpen() { return !!(S.info && S.info.open); }

    // ── persistence ──

    function loadStore() {
        try {
            var s = JSON.parse(localStorage.getItem(STORE_KEY) || '{}');
            S.last = s.last || null;
            S.recent = Array.isArray(s.recent) ? s.recent.slice(0, MAX_RECENT) : [];
            if (s.miniOpen === false) S.miniOpen = false;
            if (Array.isArray(s.expanded)) S.expanded = new Set(s.expanded.concat(['']));
        } catch (e) { /* private mode */ }
    }

    function saveStore() {
        try {
            localStorage.setItem(STORE_KEY, JSON.stringify({
                last: S.last, recent: S.recent, miniOpen: S.miniOpen,
                expanded: Array.from(S.expanded).slice(0, 200),
            }));
        } catch (e) { /* ignore */ }
    }

    function remember(rootPath) {
        S.last = rootPath;
        S.recent = [rootPath].concat(S.recent.filter(function (r) { return r !== rootPath; })).slice(0, MAX_RECENT);
        saveStore();
    }

    // ── server ──

    async function call(msg) {
        var res = await app.api.post(msg);
        if (!res || res.status !== 'ok') {
            throw new Error((res && res.message) || ('"' + msg.op + '" failed'));
        }
        if (res.data && res.data.value !== undefined) return res.data.value;
        return res.data || null;
    }

    function setStatus(text, kind) {
        var el = document.getElementById('azp-status');
        if (!el) return;
        el.textContent = text || '';
        el.className = 'azp-status' + (kind ? ' azp-status-' + kind : '');
    }

    function report(what, err) {
        app.log(what + ': ' + err.message, 'error');
        setStatus(what + ': ' + err.message, 'error');
    }

    // ── open / close ──

    function adopt(info) {
        S.info = info;
        S.tree = info && info.tree ? info.tree : null;
        if (info && info.open && info.root) remember(info.root);
        renderAll();
    }

    async function openProject(path, create) {
        path = String(path || '').trim();
        if (!path) { setStatus('Type the project folder first', 'error'); return null; }
        if (isOpen() && !confirmDiscard()) return null;
        var msg = { op: 'project_open', path: path };
        if (create) msg.create = true;
        try {
            var info = await call(msg);
            S.tabs = [];
            S.active = null;
            S.selected = null;
            adopt(info);
            var what = (info.created ? 'Created project ' : 'Opened project ') + (info.name || '') + ' (' + info.root + ')';
            app.log(what, 'info');
            setStatus(what, 'ok');
            return info;
        } catch (err) {
            report(create ? 'Create project' : 'Open project', err);
            return null;
        }
    }

    function confirmDiscard() {
        var dirty = S.tabs.filter(function (t) { return t.dirty; });
        if (!dirty.length) return true;
        return confirm('Discard unsaved changes in ' + dirty.map(function (t) { return t.path; }).join(', ') + '?');
    }

    async function closeProject() {
        if (!isOpen()) return;
        if (!confirmDiscard()) return;
        try {
            var info = await call({ op: 'project_close' });
            S.tabs = [];
            S.active = null;
            S.selected = null;
            S.tree = null;
            S.info = info && typeof info === 'object' ? info : { open: false };
            S.info.open = false;
            // An explicit close: a reload must not bring it back.
            S.last = null;
            saveStore();
            renderAll();
            app.log('Project closed', 'info');
        } catch (err) { report('Close project', err); }
    }

    async function refreshTree() {
        if (!isOpen()) return;
        try {
            var v = await call({ op: 'project_list' });
            if (v && v.tree) S.tree = v.tree;
        } catch (err) { report('Project tree', err); }
        renderTrees();
    }

    // ── file operations ──

    function askName(title, current) {
        var name = prompt(title, current || '');
        if (name === null) return null;
        name = name.trim();
        if (!isValidName(name)) {
            app.log('"' + name + '" is not a file name (one name, no "/" or "\\")', 'error');
            setStatus('"' + name + '" is not a file name', 'error');
            return null;
        }
        return name;
    }

    async function newFile(dir) {
        var name = askName('New file in ' + (dir || 'the project root') + ':',
            dir === 'styles' ? 'app.css' : (dir === 'tests' ? 'new-test.json' : ''));
        if (!name) return;
        var path = joinPath(dir, name);
        try {
            await call({ op: 'project_create', path: path, content: newFileTemplate(path) });
            ancestorsOf(path).forEach(function (a) { S.expanded.add(a); });
            await refreshTree();
            select(path, 'tree');
            await openFile(path, { noSwitch: app.state.currentView !== 'project' });
            app.log('Created ' + path, 'info');
        } catch (err) { report('New file', err); }
    }

    async function newFolder(dir) {
        var name = askName('New folder in ' + (dir || 'the project root') + ':', '');
        if (!name) return;
        var path = joinPath(dir, name);
        try {
            await call({ op: 'project_create', path: path, directory: true });
            ancestorsOf(path).concat([path]).forEach(function (a) { S.expanded.add(a); });
            await refreshTree();
            select(path, 'tree');
        } catch (err) { report('New folder', err); }
    }

    async function renamePath(path) {
        if (!path) return;
        var name = askName('Rename ' + path + ' to:', baseName(path));
        if (!name) return;
        var to = renamedPath(path, name);
        if (to === path) return;
        try {
            await call({ op: 'project_rename', from: path, to: to });
            S.tabs.forEach(function (t) {
                if (t.path === path || t.path.indexOf(path + '/') === 0) {
                    if (S.active === t.path) S.active = to + t.path.slice(path.length);
                    t.path = to + t.path.slice(path.length);
                    t.lang = languageOf(t.path);
                }
            });
            if (S.expanded.has(path)) S.expanded.add(to);
            await refreshTree();
            select(to, 'tree');
            renderEditor();
            app.log('Renamed ' + path + ' → ' + to, 'info');
        } catch (err) { report('Rename', err); }
    }

    async function deletePath(path) {
        if (!path) return;
        var entry = findEntry(S.tree, path);
        var what = entry && entry.kind === 'dir' ? 'the folder ' + path + ' and everything in it' : path;
        if (!confirm('Delete ' + what + '? This cannot be undone.')) return;
        try {
            await call({ op: 'project_delete', path: path });
            S.tabs = S.tabs.filter(function (t) { return !(t.path === path || t.path.indexOf(path + '/') === 0); });
            if (S.active && !S.tabs.some(function (t) { return t.path === S.active; })) {
                S.active = S.tabs.length ? S.tabs[S.tabs.length - 1].path : null;
            }
            if (S.selected === path || (S.selected && S.selected.indexOf(path + '/') === 0)) S.selected = null;
            await refreshTree();
            renderEditor();
            app.log('Deleted ' + path, 'info');
        } catch (err) { report('Delete', err); }
    }

    async function movePath(path, dir) {
        var to = joinPath(dir, baseName(path));
        if (to === path || dir === path || dir.indexOf(path + '/') === 0) return;
        try {
            await call({ op: 'project_rename', from: path, to: to });
            S.tabs.forEach(function (t) {
                if (t.path === path || t.path.indexOf(path + '/') === 0) {
                    if (S.active === t.path) S.active = to + t.path.slice(path.length);
                    t.path = to + t.path.slice(path.length);
                }
            });
            S.expanded.add(dir);
            await refreshTree();
            select(to, 'tree');
            renderEditor();
        } catch (err) { report('Move', err); }
    }

    function copyPath(path) {
        var full = (S.info && S.info.root ? S.info.root + '/' : '') + path;
        try { navigator.clipboard.writeText(full); } catch (e) { /* no clipboard */ }
        app.log('Path: ' + full, 'info');
    }

    // ── editor ──

    function tabOf(path) {
        for (var i = 0; i < S.tabs.length; i++) if (S.tabs[i].path === path) return S.tabs[i];
        return null;
    }

    async function openFile(path, opts) {
        opts = opts || {};
        var entry = findEntry(S.tree, path);
        if (entry && entry.kind !== 'file') return null;
        if (!opts.noSwitch && app.state.currentView !== 'project') app.ui.switchView('project');
        var tab = tabOf(path);
        if (!tab) {
            try {
                var f = await call({ op: 'project_read_file', path: path });
                tab = {
                    path: path, lang: languageOf(path), size: f.size,
                    binary: !!f.binary, content: f.content || '', text: f.content || '', dirty: false,
                };
                S.tabs.push(tab);
            } catch (err) { report('Open ' + path, err); return null; }
        }
        S.active = path;
        renderEditor();
        return tab;
    }

    function closeTab(path) {
        var tab = tabOf(path);
        if (!tab) return;
        if (tab.dirty && !confirm('Discard the unsaved changes in ' + path + '?')) return;
        var i = S.tabs.indexOf(tab);
        S.tabs.splice(i, 1);
        if (S.active === path) S.active = S.tabs.length ? S.tabs[Math.min(i, S.tabs.length - 1)].path : null;
        renderEditor();
    }

    function describeApplied(res, path) {
        if (res.applied === 'stylesheet') return 'stylesheet applied to the window';
        if (res.applied === 'component') return 'component ' + (componentKeyOf(path) || '') + ' re-registered, instances updated';
        if (res.applied === 'document') return 'document loaded into the window';
        return '';
    }

    async function saveTab(tab) {
        if (!tab || tab.binary) return false;
        var text = tab.text;
        try {
            var res = await call({ op: 'project_write_file', path: tab.path, content: text });
            tab.content = text;
            tab.dirty = tab.text !== tab.content;
            var applied = describeApplied(res || {}, tab.path);
            if (res && res.apply_error) {
                var msg = 'Saved ' + tab.path + ', but not applied: ' + res.apply_error;
                app.log(msg, 'error');
                setStatus(msg, 'error');
            } else {
                var ok = 'Saved ' + tab.path + (applied ? ' - ' + applied : '');
                app.log(ok, 'info');
                setStatus(ok, 'ok');
            }
            if (res && res.applied === 'component' && root.azDnd) {
                root.azDnd.renderPalette();
                root.azDnd.refresh();
                if (app.state.libraryList) app.handlers.loadLibraries();
            } else if (res && res.applied === 'document' && root.azDnd) {
                root.azDnd.refresh();
            }
            renderTabs();
            refreshTree();
            return true;
        } catch (err) {
            report('Save ' + tab.path, err);
            return false;
        }
    }

    async function saveActive() {
        return saveTab(tabOf(S.active));
    }

    // ── project save / load ──

    async function saveProject() {
        if (!isOpen()) { promptOpen(); return; }
        try {
            for (var i = 0; i < S.tabs.length; i++) {
                if (S.tabs[i].dirty) await saveTab(S.tabs[i]);
            }
            var res = await call({ op: 'project_save' });
            var written = (res && res.written) || [];
            var tests = app.state.tests || [];
            var paths = testPaths(tests);
            for (var t = 0; t < tests.length; t++) {
                await call({ op: 'project_write_file', path: paths[t],
                    content: JSON.stringify(testFile(tests[t]), null, 2) + '\n' });
                written.push(paths[t]);
            }
            var snaps = app.state.snapshots || {};
            var aliases = Object.keys(snaps);
            for (var s = 0; s < aliases.length; s++) {
                var sp = snapshotPath(aliases[s]);
                await call({ op: 'project_write_file', path: sp,
                    content: JSON.stringify(snapshotFile(aliases[s], snaps[aliases[s]]), null, 2) + '\n' });
                written.push(sp);
            }
            ((res && res.errors) || []).forEach(function (e) { app.log('Save project: ' + e, 'warning'); });
            // Show what was saved.
            written.forEach(function (p) { ancestorsOf(p).forEach(function (a) { S.expanded.add(a); }); });
            saveStore();
            await refreshTree();
            var msg = 'Saved project ' + (S.info.name || '') + ': ' + written.length + ' file(s)';
            app.log(msg + ' - ' + written.join(', '), 'info');
            setStatus(msg, 'ok');
        } catch (err) { report('Save project', err); }
    }

    async function windowIsFresh() {
        try {
            var doc = await call({ op: 'builder_get_document' });
            return !!doc && !!doc.root && !doc.active && !(doc.root.children || []).length;
        } catch (e) { return false; }
    }

    async function loadProject(opts) {
        opts = opts || {};
        if (!isOpen()) { promptOpen(); return; }
        if (!opts.silent) {
            var d = root.azDnd && root.azDnd.state.doc;
            var busy = d && d.active && d.root && (d.root.children || []).length;
            if (busy && !confirm('Load the project into the window? Its document.json replaces the document you are editing.')) return;
        }
        try {
            var res = (await call({ op: 'project_load' })) || {};
            await refreshTree();
            var tests = [];
            var snapshots = 0;
            var files = flattenTree(S.tree, new Set(ancestorsOf('tests/x').concat(['tests', 'snapshots'])))
                .filter(function (r) { return r.kind === 'file'; });
            for (var i = 0; i < files.length; i++) {
                var role = roleOf(files[i].path);
                if (role !== 'test' && role !== 'snapshot') continue;
                try {
                    var f = await call({ op: 'project_read_file', path: files[i].path });
                    var json = JSON.parse(f.content);
                    if (role === 'test') {
                        tests.push(testFromFile(json, files[i].path));
                    } else {
                        var snap = snapshotFromFile(json, files[i].path);
                        app.state.snapshots[snap.alias] = snap.state;
                        snapshots++;
                    }
                } catch (e) { app.log('Load ' + files[i].path + ': ' + e.message, 'warning'); }
            }
            if (tests.length) {
                app.state.tests = mergeTests(app.state.tests, tests);
                if (app.ui.renderTestList) app.ui.renderTestList();
            }
            if (app.handlers.save) app.handlers.save();
            if (app.ui.renderSnapshots) app.ui.renderSnapshots();
            if (root.azDnd) {
                root.azDnd.renderPalette();
                await root.azDnd.refresh();
            }
            (res.errors || []).forEach(function (e) { app.log('Load project: ' + e, 'warning'); });
            var msg = 'Loaded project ' + (S.info.name || '') + ': '
                + (res.components || []).length + ' component(s), '
                + (res.stylesheets || []).length + ' stylesheet(s), '
                + (res.document ? 'the document, ' : 'no document.json, ')
                + tests.length + ' test(s), ' + snapshots + ' snapshot(s)';
            app.log(msg, (res.errors || []).length ? 'warning' : 'info');
            setStatus(msg, (res.errors || []).length ? 'error' : 'ok');
        } catch (err) { report('Load project', err); }
    }

    async function exportZip() {
        if (!isOpen()) { promptOpen(); return; }
        try {
            var res = await call({ op: 'project_export_zip' });
            var a = document.createElement('a');
            a.href = res.download_url;
            a.download = res.filename || 'project.zip';
            a.click();
            app.log('Exported ' + a.download + ' (' + res.file_count + ' files, ' + res.size_bytes + ' bytes)', 'info');
        } catch (err) { report('Export ZIP', err); }
    }

    function importZip() {
        if (!isOpen()) { promptOpen(); return; }
        var input = document.getElementById('azp-zip-input');
        if (input) input.click();
    }

    function onZipChosen(input) {
        var file = input.files && input.files[0];
        if (!file) return;
        var reader = new FileReader();
        reader.onload = async function (e) {
            try {
                var res = await call({ op: 'project_import_zip', data: e.target.result });
                await refreshTree();
                app.log('Imported ' + file.name + ': ' + ((res && res.written) || []).length + ' file(s)', 'info');
                if (confirm('Load the imported project into the window now?')) await loadProject({ silent: true });
            } catch (err) { report('Import ZIP', err); }
        };
        reader.readAsDataURL(file);
        input.value = '';
    }

    function promptOpen() {
        app.ui.switchView('project');
        var input = document.getElementById('azp-open-path');
        if (input) { input.focus(); input.select(); }
        setStatus('Open or create a project folder first', 'error');
    }

    async function menuOpen() {
        if (!isOpen()) { promptOpen(); return; }
        var p = prompt('Open project folder (it is created if it does not exist):', S.info.root || '');
        if (p === null || !p.trim()) return;
        var info = await openProject(p, false);
        if (!info && confirm('There is no project at ' + p + '. Create one there?')) await openProject(p, true);
    }

    // ── snapshots (the App State camera button) ──

    async function writeSnapshot(alias) {
        if (!isOpen()) return;
        try {
            await call({ op: 'project_write_file', path: snapshotPath(alias),
                content: JSON.stringify(snapshotFile(alias, app.state.snapshots[alias]), null, 2) + '\n' });
            refreshTree();
        } catch (err) { report('Save snapshot to the project', err); }
    }

    function saveSnapshotButton() {
        if (!app.state.appStateJson) {
            app.log('Load the app state first (the refresh icon), then take a snapshot of it', 'warning');
            return;
        }
        var n = Object.keys(app.state.snapshots || {}).length + 1;
        var alias = prompt('Snapshot name:', 'snapshot-' + n);
        if (alias === null) return;
        alias = alias.trim();
        if (!alias) return;
        app._saveSnapshot(alias);
        writeSnapshot(alias);
    }

    // ── selection + sync ──

    function select(path, source) {
        S.selected = path;
        renderTrees();
        if (source !== 'doc') syncFromTree(path);
    }

    function markPaletteCard(key) {
        document.querySelectorAll('.azb-card.azp-linked').forEach(function (c) { c.classList.remove('azp-linked'); });
        if (!key) return;
        document.querySelectorAll('.azb-card').forEach(function (c) {
            if (c.dataset.key === key) {
                c.classList.add('azp-linked');
                if (c.scrollIntoView) c.scrollIntoView({ block: 'nearest' });
            }
        });
    }

    function firstInstance(node, library, name) {
        if (!node) return null;
        if (node.kind === 'component' && node.library === library && node.tag === name) return node.uid;
        var kids = node.children || [];
        for (var i = 0; i < kids.length; i++) {
            var f = firstInstance(kids[i], library, name);
            if (f != null) return f;
        }
        return null;
    }

    /** A component file selected: its palette card and its first instance. */
    function syncFromTree(path) {
        var key = componentKeyOf(path);
        markPaletteCard(key);
        var dnd = root.azDnd;
        if (!key || !dnd || !dnd.state.doc || !dnd.select) return;
        var uid = firstInstance(dnd.state.doc.root, key.split(':')[0], key.split(':')[1]);
        if (uid == null) return;
        S.syncing = true;
        try { dnd.select(uid); } finally { S.syncing = false; }
    }

    /** A component selected elsewhere: its file in the tree. */
    function syncToTree(library, name) {
        var key = library + ':' + name;
        markPaletteCard(key);
        var path = componentPathOf(library, name);
        if (!hasPath(S.tree, path)) return false;
        ancestorsOf(path).forEach(function (a) { S.expanded.add(a); });
        S.selected = path;
        renderTrees();
        return true;
    }

    function clearComponentSelection() {
        markPaletteCard(null);
        if (S.selected && roleOf(S.selected) === 'component') {
            S.selected = null;
            renderTrees();
        }
    }

    function onDocumentSelect(e) {
        if (S.syncing) return;
        var dnd = root.azDnd;
        var uid = e && e.detail ? e.detail.uid : null;
        var node = dnd && dnd.state.doc && uid != null ? dnd.logic.findNode(dnd.state.doc.root, uid) : null;
        if (node && node.kind === 'component') syncToTree(node.library, node.tag);
        else clearComponentSelection();
    }

    function onPaletteClick(e) {
        var card = e.target && e.target.closest ? e.target.closest('.azb-card') : null;
        if (!card || !card.dataset.key) return;
        var i = card.dataset.key.indexOf(':');
        var library = card.dataset.key.slice(0, i);
        var name = card.dataset.key.slice(i + 1);
        if (library === 'builtin') { clearComponentSelection(); return; }
        // The file only: the Document selection stays where the next
        // double-click on a card inserts.
        syncToTree(library, name);
    }

    // ── rendering: trees ──

    function renderTrees() {
        renderTree(document.getElementById('azp-tree'), false);
        renderTree(document.getElementById('azp-mini-tree'), true);
        renderHeader();
    }

    function renderTree(container, mini) {
        if (!container) return;
        container.innerHTML = '';
        if (!isOpen() || !S.tree) {
            if (mini) {
                var hint = document.createElement('div');
                hint.className = 'azp-hint';
                hint.innerHTML = 'No project open. <a href="#" class="azp-link">Open or create one…</a>';
                hint.querySelector('a').addEventListener('click', function (e) { e.preventDefault(); promptOpen(); });
                container.appendChild(hint);
            }
            return;
        }
        var rootEntry = Object.assign({}, S.tree, { name: (S.info && S.info.name) || S.tree.name });
        flattenTree(rootEntry, S.expanded).forEach(function (row) { container.appendChild(buildRow(row, mini)); });
        var sel = container.querySelector('.azp-row.selected');
        if (sel && sel.scrollIntoView) sel.scrollIntoView({ block: 'nearest' });
    }

    function buildRow(row, mini) {
        var el = document.createElement('div');
        el.className = 'tree-row azp-row' + (S.selected === row.path ? ' selected' : '')
            + (row.path === '' ? ' azp-root' : '');
        el.dataset.path = row.path;
        el.dataset.kind = row.kind;
        el.draggable = row.path !== '';

        var indent = document.createElement('span');
        indent.className = 'tree-indent';
        indent.style.width = (row.depth * 14 + 4) + 'px';
        el.appendChild(indent);

        var toggle = document.createElement('span');
        toggle.className = 'tree-toggle';
        if (row.kind === 'dir' && row.hasChildren) {
            toggle.textContent = row.expanded ? '▼' : '▶';
            toggle.addEventListener('click', function (e) { e.stopPropagation(); toggleDir(row.path); });
        } else {
            toggle.innerHTML = '&nbsp;';
        }
        el.appendChild(toggle);

        var icon = document.createElement('span');
        icon.className = 'material-icons azp-icon azp-role-' + (row.kind === 'dir' ? 'dir' : roleOf(row.path));
        icon.textContent = iconOf(row.entry, row.expanded);
        el.appendChild(icon);

        var name = document.createElement('span');
        name.className = 'azp-name';
        name.textContent = row.name;
        el.appendChild(name);

        var key = componentKeyOf(row.path);
        if (key) {
            var badge = document.createElement('span');
            badge.className = 'azp-badge';
            badge.textContent = key;
            el.appendChild(badge);
        }
        var tab = tabOf(row.path);
        if (tab && tab.dirty) {
            var dot = document.createElement('span');
            dot.className = 'azp-dot';
            dot.textContent = '●';
            dot.title = 'Unsaved changes';
            el.appendChild(dot);
        }
        el.title = row.path || (S.info && S.info.root) || '';

        el.addEventListener('click', function (e) {
            e.stopPropagation();
            if (row.kind === 'dir' && row.path !== '' && !mini) toggleDir(row.path);
            select(row.path, 'tree');
            if (!mini && row.kind === 'file') openFile(row.path);
        });
        el.addEventListener('dblclick', function (e) {
            e.stopPropagation();
            if (row.kind === 'file') openFile(row.path);
            else toggleDir(row.path);
        });
        el.addEventListener('contextmenu', function (e) {
            e.preventDefault();
            e.stopPropagation();
            select(row.path, 'tree');
            showMenu(e, row);
        });

        el.addEventListener('dragstart', function (e) {
            if (row.path === '') { e.preventDefault(); return; }
            S.dragPath = row.path;
            e.dataTransfer.setData(PATH_MIME, row.path);
            e.dataTransfer.effectAllowed = 'copyMove';
            var k = componentKeyOf(row.path);
            if (k && root.azDnd) {
                // A component file drops into the Document tree like a palette card.
                var payload = { type: 'component', library: k.split(':')[0], component: k.split(':')[1] };
                e.dataTransfer.setData('text/plain', JSON.stringify(payload));
                root.azDnd.state.drag = payload;
                if (root.azDnd.state.mode !== 'document' && root.azDnd.setMode) root.azDnd.setMode('document');
            }
        });
        el.addEventListener('dragend', function () {
            S.dragPath = null;
            if (root.azDnd) root.azDnd.state.drag = null;
            clearDropMarks();
        });
        if (row.kind === 'dir') {
            el.addEventListener('dragover', function (e) {
                var from = S.dragPath;
                if (!from || from === row.path || row.path.indexOf(from + '/') === 0 || parentPath(from) === row.path) return;
                e.preventDefault();
                e.stopPropagation();
                e.dataTransfer.dropEffect = 'move';
                clearDropMarks();
                el.classList.add('azp-drop-into');
            });
            el.addEventListener('dragleave', function () { el.classList.remove('azp-drop-into'); });
            el.addEventListener('drop', function (e) {
                var from = S.dragPath;
                if (!from) return;
                e.preventDefault();
                e.stopPropagation();
                clearDropMarks();
                movePath(from, row.path);
            });
        }
        return el;
    }

    function clearDropMarks() {
        document.querySelectorAll('.azp-drop-into').forEach(function (n) { n.classList.remove('azp-drop-into'); });
    }

    function toggleDir(path, openOnly) {
        if (S.expanded.has(path) && !openOnly) S.expanded.delete(path);
        else S.expanded.add(path);
        saveStore();
        renderTrees();
    }

    function onTreeKey(e) {
        if (!isOpen() || !S.tree) return;
        var rows = flattenTree(S.tree, S.expanded);
        var idx = -1;
        for (var i = 0; i < rows.length; i++) if (rows[i].path === S.selected) { idx = i; break; }
        var row = idx >= 0 ? rows[idx] : null;
        var handled = true;
        switch (e.key) {
            case 'ArrowDown': if (idx < rows.length - 1) select(rows[idx + 1].path, 'tree'); break;
            case 'ArrowUp': if (idx > 0) select(rows[idx - 1].path, 'tree'); break;
            case 'ArrowRight': if (row && row.kind === 'dir') toggleDir(row.path, true); break;
            case 'ArrowLeft':
                if (row && row.kind === 'dir' && row.expanded && row.path !== '') toggleDir(row.path);
                else if (row && row.path !== '') select(parentPath(row.path), 'tree');
                break;
            case 'Enter': if (row && row.kind === 'file') openFile(row.path); else if (row) toggleDir(row.path); break;
            case 'F2': if (row && row.path !== '') renamePath(row.path); break;
            case 'Delete':
            case 'Backspace': if (row && row.path !== '') deletePath(row.path); break;
            default: handled = false;
        }
        if (handled) {
            // The Document tree listens for Delete / arrows on the whole page.
            e.preventDefault();
            e.stopPropagation();
        }
    }

    function showMenu(e, row) {
        var items = [];
        var isDir = row.kind === 'dir';
        var dir = isDir ? row.path : parentPath(row.path);
        if (!isDir) {
            items.push({ icon: 'open_in_new', label: 'Open', action: function () { openFile(row.path); } });
            var key = componentKeyOf(row.path);
            if (key) {
                items.push({ icon: 'my_location', label: 'Select in Document', action: function () { syncFromTree(row.path); } });
                items.push({ icon: 'add', label: 'Insert into Document', action: function () { insertInstance(key); } });
            }
            items.push({ separator: true });
        }
        items.push({ icon: 'note_add', label: 'New file…', action: function () { newFile(dir); } });
        items.push({ icon: 'create_new_folder', label: 'New folder…', action: function () { newFolder(dir); } });
        if (row.path !== '') {
            items.push({ separator: true });
            items.push({ icon: 'drive_file_rename_outline', label: 'Rename… (F2)', action: function () { renamePath(row.path); } });
            items.push({ icon: 'delete', label: 'Delete…', danger: true, action: function () { deletePath(row.path); } });
        }
        items.push({ separator: true });
        items.push({ icon: 'content_copy', label: 'Copy path', action: function () { copyPath(row.path); } });
        if (row.path === '') {
            items.push({ icon: 'refresh', label: 'Refresh', action: function () { refreshTree(); } });
            items.push({ icon: 'save', label: 'Save Project', action: saveProject });
            items.push({ icon: 'upload_file', label: 'Load Project', action: function () { loadProject(); } });
        }
        if (app.widgets && app.widgets.ContextMenu) app.widgets.ContextMenu.show(e.clientX, e.clientY, items);
    }

    function insertInstance(key) {
        var dnd = root.azDnd;
        if (!dnd || !dnd.state.doc) return;
        var parent = 0;
        var sel = dnd.state.selected != null ? dnd.logic.findNode(dnd.state.doc.root, dnd.state.selected) : null;
        if (sel && dnd.logic.acceptsChildren(sel)) parent = sel.uid;
        dnd.send({ op: 'builder_insert', parent: parent, component: key.split(':')[1], library: key.split(':')[0] });
    }

    // ── rendering: header, welcome, editor ──

    function renderHeader() {
        var nameEl = document.getElementById('azp-project-name');
        if (nameEl) {
            nameEl.textContent = isOpen() ? (S.info.name || '') : 'No project';
            nameEl.title = isOpen() ? S.info.root : '';
        }
        var miniName = document.getElementById('azp-mini-name');
        if (miniName) miniName.textContent = isOpen() ? '— ' + (S.info.name || '') : '';
        var welcome = document.getElementById('azp-welcome');
        var tree = document.getElementById('azp-tree');
        if (welcome) welcome.classList.toggle('hidden', isOpen());
        if (tree) tree.classList.toggle('hidden', !isOpen());
        document.querySelectorAll('#azp-toolbar button[data-needs-project]').forEach(function (b) { b.disabled = !isOpen(); });
    }

    function renderWelcome() {
        var box = document.getElementById('azp-welcome');
        if (!box) return;
        box.innerHTML = '';
        var h = document.createElement('div');
        h.className = 'azp-welcome-title';
        h.textContent = 'Open a project folder';
        box.appendChild(h);
        var p = document.createElement('p');
        p.className = 'azp-welcome-text';
        p.textContent = 'A project is a folder on the machine AzBuilder runs on: the document, one file per component, '
            + 'stylesheets, E2E tests and snapshots. Type its path (or a new one to create it).';
        box.appendChild(p);
        var input = document.createElement('input');
        input.type = 'text';
        input.id = 'azp-open-path';
        input.className = 'azp-input';
        input.spellcheck = false;
        input.value = S.last || (S.info && S.info.suggested) || '';
        input.placeholder = '/path/to/project';
        input.addEventListener('keydown', function (e) {
            if (e.key === 'Enter') { e.preventDefault(); openProject(input.value, false); }
        });
        box.appendChild(input);
        var btns = document.createElement('div');
        btns.className = 'azp-welcome-buttons';
        var open = document.createElement('button');
        open.id = 'azp-open-btn';
        open.className = 'btn-sm';
        open.textContent = 'Open';
        open.title = 'Open an existing folder';
        open.addEventListener('click', function () { openProject(input.value, false); });
        var create = document.createElement('button');
        create.id = 'azp-create-btn';
        create.className = 'btn-sm';
        create.textContent = 'Create';
        create.title = 'Create the folder (if needed) with the project skeleton';
        create.addEventListener('click', function () { openProject(input.value, true); });
        btns.appendChild(open);
        btns.appendChild(create);
        box.appendChild(btns);
        if (S.info && S.info.cwd) {
            var cwd = document.createElement('div');
            cwd.className = 'azp-welcome-text';
            cwd.textContent = 'The app runs in ' + S.info.cwd + ' (relative paths start there).';
            box.appendChild(cwd);
        }
        if (S.recent.length) {
            var rh = document.createElement('div');
            rh.className = 'azp-welcome-sub';
            rh.textContent = 'Recent';
            box.appendChild(rh);
            S.recent.forEach(function (r) {
                var a = document.createElement('div');
                a.className = 'azp-recent';
                a.dataset.path = r;
                a.title = 'Open ' + r;
                a.innerHTML = '<span class="material-icons">folder</span>';
                var t = document.createElement('span');
                t.textContent = r;
                a.appendChild(t);
                a.addEventListener('click', function () { openProject(r, false); });
                box.appendChild(a);
            });
        }
    }

    function renderTabs() {
        var bar = document.getElementById('azp-tabs');
        if (!bar) return;
        bar.innerHTML = '';
        S.tabs.forEach(function (tab) {
            var t = document.createElement('div');
            t.className = 'azp-tab' + (tab.path === S.active ? ' active' : '') + (tab.dirty ? ' azp-dirty' : '');
            t.dataset.path = tab.path;
            t.title = tab.path;
            var icon = document.createElement('span');
            icon.className = 'material-icons azp-icon';
            icon.textContent = iconOf({ kind: 'file', path: tab.path });
            var name = document.createElement('span');
            name.textContent = baseName(tab.path);
            var close = document.createElement('span');
            close.className = 'azp-tab-close';
            close.textContent = tab.dirty ? '●' : '×';
            close.title = 'Close';
            close.addEventListener('click', function (e) { e.stopPropagation(); closeTab(tab.path); });
            t.appendChild(icon);
            t.appendChild(name);
            t.appendChild(close);
            t.addEventListener('click', function () { S.active = tab.path; renderEditor(); });
            t.addEventListener('auxclick', function (e) { if (e.button === 1) closeTab(tab.path); });
            bar.appendChild(t);
        });
        if (app.state.currentView === 'project') {
            var title = document.getElementById('tab-title');
            if (title) title.innerText = S.active ? baseName(S.active) : 'Project';
        }
    }

    function renderEditor() {
        renderTabs();
        var tab = tabOf(S.active);
        var empty = document.getElementById('azp-editor-empty');
        var editor = document.getElementById('azp-editor');
        var preview = document.getElementById('azp-preview');
        var info = document.getElementById('azp-file-info');
        if (!editor) return;
        if (empty) empty.classList.toggle('hidden', !!tab);
        editor.classList.toggle('hidden', !tab || tab.binary);
        if (preview) {
            preview.classList.toggle('hidden', !tab || !tab.binary);
            preview.innerHTML = '';
            if (tab && tab.binary) {
                if (isImagePath(tab.path)) {
                    var img = document.createElement('img');
                    var ext = extOf(tab.path);
                    var mime = ext === 'svg' ? 'image/svg+xml' : 'image/' + (ext === 'jpg' ? 'jpeg' : ext);
                    img.src = 'data:' + mime + ';base64,' + tab.content;
                    img.alt = tab.path;
                    preview.appendChild(img);
                } else {
                    var msg = document.createElement('div');
                    msg.className = 'azp-hint';
                    msg.textContent = tab.path + ' is a binary file (' + tab.size + ' bytes); the editor opens text files.';
                    preview.appendChild(msg);
                }
            }
        }
        if (info) info.textContent = tab ? tab.path + '  ·  ' + tab.lang + (tab.binary ? '  ·  binary' : '') : '';
        if (!tab || tab.binary) { renderTrees(); return; }
        var ta = document.getElementById('azp-editor-text');
        if (ta.value !== tab.text) ta.value = tab.text;
        ta.dataset.lang = tab.lang;
        updateHighlight();
        renderTrees();
    }

    function updateHighlight() {
        var tab = tabOf(S.active);
        var code = document.getElementById('azp-editor-code');
        var gutter = document.getElementById('azp-gutter');
        var ta = document.getElementById('azp-editor-text');
        if (!tab || !code || !ta) return;
        // The trailing newline keeps the last line as tall as the textarea's.
        code.innerHTML = highlight(tab.text, tab.lang) + '\n';
        if (gutter) {
            var n = tab.text.split('\n').length;
            var lines = [];
            for (var i = 1; i <= n; i++) lines.push(i);
            gutter.textContent = lines.join('\n') + '\n';
        }
        syncScroll();
    }

    function syncScroll() {
        var ta = document.getElementById('azp-editor-text');
        var code = document.getElementById('azp-editor-code');
        var gutter = document.getElementById('azp-gutter');
        if (!ta) return;
        if (code) { code.scrollTop = ta.scrollTop; code.scrollLeft = ta.scrollLeft; }
        if (gutter) gutter.scrollTop = ta.scrollTop;
    }

    function onEditorInput() {
        var tab = tabOf(S.active);
        var ta = document.getElementById('azp-editor-text');
        if (!tab || !ta) return;
        var wasDirty = tab.dirty;
        tab.text = ta.value;
        tab.dirty = tab.text !== tab.content;
        updateHighlight();
        if (wasDirty !== tab.dirty) { renderTabs(); renderTrees(); }
    }

    function onEditorKey(e) {
        var ta = e.target;
        if ((e.ctrlKey || e.metaKey) && (e.key === 's' || e.key === 'S')) {
            e.preventDefault();
            e.stopPropagation();
            saveActive();
            return;
        }
        if (e.key === 'Tab' && !e.ctrlKey && !e.metaKey && !e.altKey) {
            e.preventDefault();
            var start = ta.selectionStart;
            var end = ta.selectionEnd;
            ta.value = ta.value.slice(0, start) + '    ' + ta.value.slice(end);
            ta.selectionStart = ta.selectionEnd = start + 4;
            onEditorInput();
        }
    }

    function renderAll() {
        renderWelcome();
        renderTrees();
        renderEditor();
        var mini = document.getElementById('azp-mini-body');
        var icon = document.getElementById('azp-mini-toggle');
        if (mini) mini.classList.toggle('hidden', !S.miniOpen);
        if (icon) icon.textContent = S.miniOpen ? 'expand_less' : 'expand_more';
    }

    // ── injection ──

    function el(tag, attrs, html) {
        var n = document.createElement(tag);
        Object.keys(attrs || {}).forEach(function (k) { n.setAttribute(k, attrs[k]); });
        if (html) n.innerHTML = html;
        return n;
    }

    function injectActivity() {
        var bar = document.querySelector('.activity-bar');
        if (!bar || bar.querySelector('[data-view="project"]')) return;
        var icon = el('div', { 'class': 'activity-icon', 'data-view': 'project', title: 'Project (files)' },
            '<span class="material-icons">folder_open</span>');
        icon.addEventListener('click', function () { app.ui.switchView('project'); });
        var first = bar.querySelector('[data-view="inspector"]');
        if (first && first.nextSibling) bar.insertBefore(icon, first.nextSibling);
        else bar.appendChild(icon);
    }

    function toolButton(icon, title, action, needsProject) {
        var b = el('button', { 'class': 'azb-icon', title: title }, '<span class="material-icons">' + icon + '</span>');
        if (needsProject) b.setAttribute('data-needs-project', '1');
        b.addEventListener('click', action);
        return b;
    }

    function injectSidebar() {
        var panel = document.getElementById('sidebar-panel');
        if (!panel || document.getElementById('sidebar-project')) return;
        var side = el('div', { id: 'sidebar-project', 'class': 'sidebar-content hidden azp-side' });
        var bar = el('div', { id: 'azp-toolbar', 'class': 'azb-toolbar azp-toolbar' });
        var name = el('span', { id: 'azp-project-name', 'class': 'azp-project-name' });
        bar.appendChild(name);
        bar.appendChild(toolButton('note_add', 'New file (in the selected folder)', function () {
            var sel = S.selected || '';
            var e = findEntry(S.tree, sel);
            newFile(e && e.kind === 'dir' ? sel : parentPath(sel));
        }, true));
        bar.appendChild(toolButton('create_new_folder', 'New folder', function () {
            var sel = S.selected || '';
            var e = findEntry(S.tree, sel);
            newFolder(e && e.kind === 'dir' ? sel : parentPath(sel));
        }, true));
        bar.appendChild(toolButton('refresh', 'Refresh the tree', function () { refreshTree(); }, true));
        bar.appendChild(toolButton('unfold_less', 'Collapse all', function () {
            S.expanded = new Set(['']);
            saveStore();
            renderTrees();
        }, true));
        bar.appendChild(toolButton('save', 'Save Project (document, components, tests, snapshots)', saveProject, true));
        bar.appendChild(toolButton('upload_file', 'Load Project into the window', function () { loadProject(); }, true));
        side.appendChild(bar);
        side.appendChild(el('div', { id: 'azp-welcome', 'class': 'azp-welcome' }));
        var tree = el('div', { id: 'azp-tree', 'class': 'dom-tree azp-tree', tabindex: '0' });
        tree.addEventListener('keydown', onTreeKey);
        tree.addEventListener('contextmenu', function (e) {
            if (e.target === tree && S.tree) { e.preventDefault(); showMenu(e, { path: '', kind: 'dir', entry: S.tree }); }
        });
        side.appendChild(tree);
        panel.appendChild(side);
    }

    function injectMini() {
        var host = document.getElementById('sidebar-inspector');
        if (!host || document.getElementById('azp-inspector-project')) return;
        var box = el('div', { id: 'azp-inspector-project', 'class': 'azp-mini' });
        var head = el('div', { 'class': 'sidebar-header azp-mini-head', title: 'The open project (click to fold)' },
            '<span>Project <span id="azp-mini-name" class="azp-mini-name"></span></span>'
            + '<span class="material-icons" id="azp-mini-toggle" style="font-size:14px">expand_less</span>');
        head.addEventListener('click', function () {
            S.miniOpen = !S.miniOpen;
            saveStore();
            renderAll();
        });
        var body = el('div', { id: 'azp-mini-body', 'class': 'azp-mini-body' });
        var tree = el('div', { id: 'azp-mini-tree', 'class': 'dom-tree azp-tree', tabindex: '0' });
        tree.addEventListener('keydown', onTreeKey);
        body.appendChild(tree);
        box.appendChild(head);
        box.appendChild(body);
        host.appendChild(box);
    }

    function injectView() {
        var content = document.querySelector('.editor-content');
        if (!content || document.getElementById('view-project')) return;
        var view = el('div', { id: 'view-project', 'class': 'hidden azp-view' });
        view.appendChild(el('div', { id: 'azp-tabs', 'class': 'azp-tabs' }));
        var empty = el('div', { id: 'azp-editor-empty', 'class': 'azp-empty' });
        empty.innerHTML = '<div class="azp-empty-title">AzBuilder project</div>'
            + '<p>Open a file from the tree. Saving (Ctrl/Cmd+S) writes it to disk; saving a file the builder uses also applies it:</p>'
            + '<ul>'
            + '<li><code>styles/*.css</code> - the project stylesheets, applied to the window in path order</li>'
            + '<li><code>components/&lt;library&gt;/&lt;name&gt;.json</code> - one file per component: fields, CSS, template; '
            + 'every instance updates</li>'
            + '<li><code>document.json</code> - the builder document</li>'
            + '<li><code>tests/*.json</code>, <code>snapshots/*.json</code> - E2E tests and app-state snapshots '
            + '(<code>AZ_E2E=&lt;project&gt;/tests</code> runs the tests)</li>'
            + '</ul>'
            + '<p>Project &gt; Save Project writes the document, the components, the tests and the snapshots; '
            + 'Load Project brings them back.</p>';
        view.appendChild(empty);
        var editor = el('div', { id: 'azp-editor', 'class': 'azp-editor hidden' });
        editor.appendChild(el('pre', { id: 'azp-gutter', 'class': 'azp-gutter', 'aria-hidden': 'true' }));
        var wrap = el('div', { 'class': 'azp-code-wrap' });
        wrap.appendChild(el('pre', { id: 'azp-editor-code', 'class': 'azp-code', 'aria-hidden': 'true' }));
        var ta = el('textarea', { id: 'azp-editor-text', 'class': 'azp-text', spellcheck: 'false', wrap: 'off',
            autocomplete: 'off', autocapitalize: 'off' });
        ta.addEventListener('input', onEditorInput);
        ta.addEventListener('scroll', syncScroll);
        ta.addEventListener('keydown', onEditorKey);
        wrap.appendChild(ta);
        editor.appendChild(wrap);
        view.appendChild(editor);
        view.appendChild(el('div', { id: 'azp-preview', 'class': 'azp-preview hidden' }));
        var status = el('div', { 'class': 'azp-statusbar' });
        status.appendChild(el('span', { id: 'azp-status', 'class': 'azp-status' }));
        status.appendChild(el('span', { id: 'azp-file-info', 'class': 'azp-file-info' }));
        view.appendChild(status);
        content.appendChild(view);
        var zip = el('input', { type: 'file', id: 'azp-zip-input', 'class': 'hidden', accept: '.zip,application/zip' });
        zip.addEventListener('change', function () { onZipChosen(zip); });
        document.body.appendChild(zip);
    }

    function injectMenu() {
        var bar = document.getElementById('menubar');
        if (!bar || bar.querySelector('[data-menu="project"]')) return;
        var menu = el('div', { 'class': 'menu-item', 'data-menu': 'project' });
        menu.appendChild(document.createTextNode('Project'));
        var drop = el('div', { 'class': 'menu-dropdown' });
        var item = function (act, icon, label, action) {
            var i = el('div', { 'class': 'menu-dropdown-item', 'data-act': act },
                '<span class="material-icons mi">' + icon + '</span>');
            i.appendChild(document.createTextNode(label));
            i.addEventListener('click', function () { action(); });
            drop.appendChild(i);
        };
        var sep = function () { drop.appendChild(el('div', { 'class': 'menu-dropdown-separator' })); };
        item('open', 'folder_open', 'Open / New Project…', menuOpen);
        item('save', 'save', 'Save Project', saveProject);
        item('load', 'upload_file', 'Load Project', function () { loadProject(); });
        sep();
        item('export-zip', 'archive', 'Export Project as ZIP', exportZip);
        item('import-zip', 'unarchive', 'Import ZIP into Project…', importZip);
        sep();
        item('close', 'close', 'Close Project', closeProject);
        menu.appendChild(drop);
        bar.insertBefore(menu, bar.firstChild);
    }

    function injectStyle() {
        if (document.getElementById('azp-style')) return;
        var s = document.createElement('style');
        s.id = 'azp-style';
        s.textContent = [
            '.azp-side{display:flex;flex-direction:column;overflow:hidden}',
            '.azp-side.hidden{display:none!important}',
            '.azp-toolbar{gap:2px}',
            '.azp-project-name{flex:1;font-size:11px;font-weight:600;color:var(--text-main);white-space:nowrap;overflow:hidden;text-overflow:ellipsis;padding-left:2px}',
            '.azp-tree{flex:1;overflow:auto;outline:none;padding-bottom:12px}',
            '.azp-tree:focus .azp-row.selected{box-shadow:inset 0 0 0 1px var(--accent)}',
            '.azp-row .azp-icon{font-size:15px;margin-right:4px;color:var(--text-muted)}',
            '.azp-row .azp-role-dir{color:#dcb67a}',
            '.azp-row .azp-role-component{color:#c586c0}',
            '.azp-row .azp-role-stylesheet{color:#4fc1ff}',
            '.azp-row .azp-role-document{color:var(--success)}',
            '.azp-row .azp-role-test{color:#d7ba7d}',
            '.azp-row .azp-role-snapshot{color:#9cdcfe}',
            '.azp-row.azp-root .azp-name{font-weight:600}',
            '.azp-name{white-space:nowrap;overflow:hidden;text-overflow:ellipsis}',
            '.azp-badge{margin-left:6px;font-size:10px;padding:0 4px;border-radius:3px;background:rgba(197,134,192,.18);color:#c586c0;white-space:nowrap}',
            '.azp-dot{margin-left:6px;color:var(--warning);font-size:9px}',
            '.azp-row.azp-drop-into{background:rgba(0,122,204,.22);box-shadow:inset 0 0 0 1px var(--accent)}',
            '.azb-card.azp-linked{border-color:#c586c0;box-shadow:0 0 0 2px rgba(197,134,192,.55)}',
            '.azp-hint{color:var(--text-muted);font:11px/1.5 sans-serif;padding:8px 10px}',
            '.azp-link{color:var(--accent);cursor:pointer}',
            '.azp-welcome{padding:10px 12px;overflow:auto}',
            '.azp-welcome-title{font-size:13px;font-weight:600;margin-bottom:6px}',
            '.azp-welcome-text{font-size:11px;line-height:1.5;color:var(--text-muted);margin:4px 0 8px}',
            '.azp-welcome-sub{font-size:10px;text-transform:uppercase;letter-spacing:.04em;color:var(--text-muted);margin:12px 0 4px}',
            '.azp-welcome-buttons{display:flex;gap:6px;margin:6px 0}',
            '.azp-input{width:100%;background:var(--bg-input);color:var(--text-main);border:1px solid var(--border);border-radius:3px;padding:4px 6px;font:12px monospace}',
            '.azp-recent{display:flex;align-items:center;gap:4px;font:11px monospace;padding:3px 4px;border-radius:3px;cursor:pointer;color:var(--text-main);overflow:hidden;white-space:nowrap;text-overflow:ellipsis}',
            '.azp-recent:hover{background:var(--bg-hover)}',
            '.azp-recent .material-icons{font-size:14px;color:#dcb67a}',
            '.azp-mini{border-top:1px solid var(--border);display:flex;flex-direction:column;flex-shrink:0}',
            '.azp-mini-head{cursor:pointer}',
            '.azp-mini-name{color:var(--text-muted);font-weight:400;text-transform:none}',
            '.azp-mini-body{max-height:28vh;overflow:auto}',
            '.azp-mini-body .azp-tree{padding-bottom:4px}',
            '.azp-view{height:100%;display:flex;flex-direction:column;overflow:hidden}',
            '.azp-view.hidden{display:none!important}',
            '.azp-tabs{display:flex;background:var(--bg-sidebar);border-bottom:1px solid var(--border);overflow-x:auto;flex-shrink:0;min-height:28px}',
            '.azp-tab{display:flex;align-items:center;gap:4px;padding:4px 8px 4px 10px;font-size:12px;color:var(--text-muted);border-right:1px solid var(--border);cursor:pointer;white-space:nowrap;user-select:none}',
            '.azp-tab.active{background:var(--bg-panel);color:var(--text-main);box-shadow:inset 0 1px 0 var(--accent)}',
            '.azp-tab .azp-icon{font-size:14px}',
            '.azp-tab-close{margin-left:4px;width:14px;text-align:center;border-radius:3px;color:var(--text-muted)}',
            '.azp-tab-close:hover{background:var(--bg-hover);color:var(--text-main)}',
            '.azp-tab.azp-dirty .azp-tab-close{color:var(--warning)}',
            '.azp-empty{padding:24px 28px;color:var(--text-muted);font-size:12px;line-height:1.6;overflow:auto}',
            '.azp-empty-title{font-size:15px;color:var(--text-main);margin-bottom:8px}',
            '.azp-empty ul{margin:6px 0 10px 18px}',
            '.azp-empty code{color:var(--string-color)}',
            '.azp-editor{flex:1;display:flex;overflow:hidden;background:var(--bg-panel)}',
            '.azp-editor.hidden{display:none!important}',
            '.azp-gutter,.azp-code,.azp-text{font:12px/18px Consolas,Monaco,"Courier New",monospace;tab-size:4;white-space:pre;margin:0;padding:8px}',
            '.azp-gutter{width:52px;flex-shrink:0;text-align:right;color:#5a5a5a;overflow:hidden;border-right:1px solid var(--border);user-select:none;padding-right:8px}',
            '.azp-code-wrap{position:relative;flex:1;overflow:hidden}',
            '.azp-code{position:absolute;inset:0;overflow:hidden;color:var(--text-main);pointer-events:none}',
            '.azp-text{position:absolute;inset:0;width:100%;height:100%;border:0;resize:none;background:transparent;color:transparent;caret-color:#fff;overflow:auto;z-index:1}',
            '.azp-text::selection{background:rgba(0,122,204,.4);color:transparent}',
            '.azp-tok-comment{color:#6a9955}',
            '.azp-tok-string{color:var(--string-color)}',
            '.azp-tok-number{color:#b5cea8}',
            '.azp-tok-keyword{color:#569cd6}',
            '.azp-tok-tag{color:#569cd6}',
            '.azp-tok-attr{color:var(--attr-color)}',
            '.azp-tok-prop{color:var(--attr-color)}',
            '.azp-tok-placeholder{color:#c586c0;font-weight:600}',
            '.azp-preview{flex:1;overflow:auto;padding:16px;background:repeating-conic-gradient(#2a2a2a 0 25%,#333 0 50%) 0 0/16px 16px}',
            '.azp-preview.hidden{display:none!important}',
            '.azp-preview img{max-width:100%;image-rendering:pixelated}',
            '.azp-statusbar{display:flex;gap:12px;justify-content:space-between;padding:3px 10px;font-size:11px;background:var(--bg-sidebar);border-top:1px solid var(--border);flex-shrink:0;min-height:22px}',
            '.azp-status{color:var(--text-muted);overflow:hidden;text-overflow:ellipsis;white-space:nowrap}',
            '.azp-status-ok{color:var(--success)}',
            '.azp-status-error{color:var(--error)}',
            '.azp-file-info{color:var(--text-muted);white-space:nowrap}',
        ].join('\n');
        document.head.appendChild(s);
    }

    function registerSchema() {
        var C = app.schema && app.schema.commands;
        if (!C) return;
        var P = function (name, ph, optional) { return { name: name, type: 'text', placeholder: ph, optional: !!optional }; };
        C.project_info = { desc: 'The open project (tree) or the suggested folder', examples: ['/project_info'], params: [] };
        C.project_open = { desc: 'Open (create: true = create) a project folder', examples: ['/project_open path ~/my-app create true'],
            params: [P('path', '~/my-app'), { name: 'create', type: 'text', placeholder: 'true', optional: true }] };
        C.project_close = { desc: 'Close the project', examples: ['/project_close'], params: [] };
        C.project_list = { desc: 'The project file tree', examples: ['/project_list'], params: [] };
        C.project_read_file = { desc: 'Read a project file', examples: ['/project_read_file path styles/app.css'], params: [P('path', 'styles/app.css')] };
        C.project_write_file = { desc: 'Write a project file (applies styles / components / document)',
            examples: ['/project_write_file path styles/app.css content "body { color: red }"'],
            params: [P('path', 'styles/app.css'), P('content', 'body { }')] };
        C.project_create = { desc: 'Create a file or folder', examples: ['/project_create path notes directory true'],
            params: [P('path', 'notes'), { name: 'directory', type: 'text', placeholder: 'true', optional: true }] };
        C.project_rename = { desc: 'Rename / move inside the project', examples: ['/project_rename from a.css to styles/a.css'],
            params: [P('from', 'a.css'), P('to', 'styles/a.css')] };
        C.project_delete = { desc: 'Delete a file or folder', examples: ['/project_delete path notes'], params: [P('path', 'notes')] };
        C.project_save = { desc: 'Save the document and user components into the project', examples: ['/project_save'], params: [] };
        C.project_load = { desc: 'Load components, stylesheets and the document', examples: ['/project_load'], params: [] };
        C.project_export_zip = { desc: 'The project as a ZIP (data URI)', examples: ['/project_export_zip'], params: [] };
        C.project_import_zip = { desc: 'Unpack a ZIP (base64) into the project', examples: ['/project_import_zip data UEsDB…'], params: [P('data', 'base64')] };
    }

    // ── install ──

    function install() {
        loadStore();
        injectStyle();
        injectMenu();
        injectActivity();
        injectSidebar();
        injectMini();
        injectView();
        registerSchema();
        renderAll();

        // The Project view: the stock switcher knows three views and hides
        // them all for a fourth; show ours on top.
        var origSwitchView = app.ui.switchView;
        app.ui.switchView = function (view) {
            var r = origSwitchView.apply(this, arguments);
            var on = view === 'project';
            var side = document.getElementById('sidebar-project');
            var main = document.getElementById('view-project');
            if (side) side.classList.toggle('hidden', !on);
            if (main) main.classList.toggle('hidden', !on);
            if (on) {
                var t = document.getElementById('sidebar-title');
                if (t) t.innerText = 'Project';
                var tab = document.getElementById('tab-title');
                if (tab) tab.innerText = S.active ? baseName(S.active) : 'Project';
                var appstate = document.getElementById('appstate-panel');
                if (appstate) {
                    appstate.classList.add('hidden');
                    var rz = appstate.nextElementSibling;
                    if (rz && rz.classList.contains('resizer')) rz.classList.add('hidden');
                }
                renderEditor();
            }
            return r;
        };

        // The App State camera button called `app.handlers._saveSnapshot`,
        // which did not exist (the function is `app._saveSnapshot(alias)`).
        app.handlers._saveSnapshot = saveSnapshotButton;

        // Ctrl/Cmd+S anywhere in the Project view saves the active file.
        document.addEventListener('keydown', function (e) {
            if (app.state.currentView !== 'project') return;
            if ((e.ctrlKey || e.metaKey) && (e.key === 's' || e.key === 'S')) {
                e.preventDefault();
                saveActive();
            }
        });
        // Sync with the builder: document selection, palette clicks.
        document.addEventListener('azb:select', onDocumentSelect);
        document.addEventListener('click', onPaletteClick, true);
        window.addEventListener('beforeunload', function (e) {
            if (S.tabs.some(function (t) { return t.dirty; })) { e.preventDefault(); e.returnValue = ''; }
        });

        // After the page connected: adopt the server's project, or re-open
        // the last one (and load it into a fresh AzBuilder window).
        var origInit = app.init;
        app.init = async function () {
            var r = await origInit.apply(this, arguments);
            try { await start(); } catch (e) { /* no project support on this server */ }
            S.ready = true;
            return r;
        };
    }

    async function start() {
        if (app.config.isMock) { renderAll(); return; }
        var info = await call({ op: 'project_info' });
        S.info = info || { open: false };
        if (S.info.open) {
            adopt(S.info);
            return;
        }
        renderAll();
        if (!S.last) return;
        var last = S.last;
        try {
            var opened = await call({ op: 'project_open', path: last });
            adopt(opened);
            app.log('Re-opened project ' + (opened.name || '') + ' (' + opened.root + ')', 'info');
            if (await windowIsFresh()) await loadProject({ silent: true });
        } catch (err) {
            app.log('Could not re-open the last project ' + last + ': ' + err.message, 'warning');
            renderAll();
        }
    }

    install();
    root.azProject = {
        logic: logic,
        state: S,
        open: openProject,
        close: closeProject,
        save: saveProject,
        load: loadProject,
        refresh: refreshTree,
        openFile: openFile,
        select: select,
    };
})(typeof window !== 'undefined' ? window : globalThis);
