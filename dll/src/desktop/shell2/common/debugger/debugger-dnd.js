/**
 * AzBuilder drag and drop — loaded after debugger.js.
 *
 * Qt-Creator-style editing of the builder DOCUMENT (the tree the server
 * mounts over the native window, layout/src/e2e/builder.rs):
 *
 *   - "Document" tree in the DOM Explorer (a "Live DOM" toggle keeps the old
 *     inspector): rows are document nodes with stable uids.
 *   - Palette of every registered component with a thumbnail rendered by the
 *     NATIVE CPU renderer (`get_component_thumbnail`, cached server-side); the
 *     thumbnail is the drag image.
 *   - Drop indicators: a line BEFORE / AFTER a row, or the row highlighted for
 *     INTO. Drop a palette card to insert, drag a row to move it.
 *   - Delete / Backspace, Cmd/Ctrl+Z, Shift+Cmd/Ctrl+Z (or Ctrl+Y), arrows.
 *   - Context menu: insert, edit text / classes / id, move up / down,
 *     "Convert to component…" (the subtree becomes a template component that
 *     shows up in the palette and drops again).
 *
 * Server messages: builder_get_document, builder_insert, builder_move,
 * builder_delete, builder_set_attribute, builder_undo, builder_redo,
 * builder_reset, builder_convert_to_component, get_component_thumbnail,
 * get_component_registry, get_libraries, get_node_hierarchy.
 *
 * The pure logic at the top (drop zones, drop targets, the message a drop
 * sends) has no DOM dependency and is unit-tested under node:
 *     node dll/src/desktop/shell2/common/debugger/debugger-dnd.test.js
 */
(function (root) {
    'use strict';

    // =====================================================================
    // Pure logic
    // =====================================================================

    /** Builtins that draw nothing / are document structure: not in the palette. */
    var NON_VISUAL = ['html', 'head', 'title', 'body', 'meta', 'link', 'script', 'style', 'base'];
    /** The XML parser's void elements: they take no children. */
    var VOID = ['area', 'base', 'br', 'col', 'embed', 'hr', 'img', 'input', 'link', 'meta',
        'param', 'source', 'track', 'wbr'];

    /**
     * The XML parser's HTML auto-close rules (mirrors builder.rs AUTO_CLOSE):
     * `<p><div/></p>` parses as two siblings, so the server refuses the nesting.
     */
    var AUTO_CLOSE = {
        p: ['address', 'article', 'aside', 'blockquote', 'div', 'dl', 'fieldset', 'footer', 'form',
            'h1', 'h2', 'h3', 'h4', 'h5', 'h6', 'header', 'hr', 'main', 'nav', 'ol', 'p', 'pre',
            'section', 'table', 'ul'],
        li: ['li'], td: ['td', 'th', 'tr'], th: ['td', 'th', 'tr'], tr: ['tr'],
        option: ['option', 'optgroup'], optgroup: ['optgroup'], dd: ['dd', 'dt'], dt: ['dd', 'dt'],
    };

    /** Whether a document node can take children (mirrors builder.rs). */
    function acceptsChildren(node) {
        return !!node && node.kind === 'element' && VOID.indexOf(node.tag) === -1;
    }

    /** Whether `parent` can take a child element `childTag` (null: not an element). */
    function canContain(parent, childTag) {
        if (!acceptsChildren(parent)) return false;
        var closers = childTag ? AUTO_CLOSE[parent.tag] : null;
        return !closers || closers.indexOf(childTag) === -1;
    }

    /**
     * The drop zone for a pointer at `relY` (0 = top edge of the row, 1 = bottom
     * edge). A container splits into before (top quarter) / into / after (bottom
     * quarter); a leaf into before / after halves; the root only takes "into".
     */
    function dropZone(relY, node, isRoot) {
        if (isRoot) return 'into';
        if (acceptsChildren(node)) {
            if (relY < 0.25) return 'before';
            if (relY > 0.75) return 'after';
            return 'into';
        }
        return relY < 0.5 ? 'before' : 'after';
    }

    /** The document as tree rows: {uid, parent, index, depth, node}. */
    function flatten(rootNode, collapsed) {
        var rows = [];
        if (!rootNode) return rows;
        (function walk(node, parent, index, depth) {
            rows.push({ uid: node.uid, parent: parent, index: index, depth: depth, node: node });
            if (collapsed && collapsed.has && collapsed.has(node.uid)) return;
            (node.children || []).forEach(function (c, i) { walk(c, node.uid, i, depth + 1); });
        })(rootNode, null, 0, 0);
        return rows;
    }

    /**
     * Where a drop in `zone` of `row` inserts: the parent's uid and the child
     * slot as the user SEES it (before the move) — exactly what builder_insert /
     * builder_move take. `index: null` appends.
     */
    function dropTarget(row, zone) {
        if (zone === 'into' || row.parent == null) return { parent: row.uid, index: null };
        return { parent: row.parent, index: zone === 'before' ? row.index : row.index + 1 };
    }

    function findNode(node, uid) {
        if (!node) return null;
        if (node.uid === uid) return node;
        var kids = node.children || [];
        for (var i = 0; i < kids.length; i++) {
            var f = findNode(kids[i], uid);
            if (f) return f;
        }
        return null;
    }

    /** The row whose node has this uid, with its parent and index. */
    function rowOf(rootNode, uid) {
        var rows = flatten(rootNode, null);
        for (var i = 0; i < rows.length; i++) if (rows[i].uid === uid) return rows[i];
        return null;
    }

    function isSelfOrDescendant(rootNode, ancestorUid, uid) {
        var a = findNode(rootNode, ancestorUid);
        return !!a && !!findNode(a, uid);
    }

    /**
     * Normalise a drag payload: palette cards, the Components view list and the
     * old inspector palette all put JSON in text/plain, some without `type`.
     */
    function normalizePayload(p) {
        if (!p || typeof p !== 'object') return null;
        if (p.type === 'builder-node') return typeof p.uid === 'number' ? p : null;
        if (p.component) {
            return { type: 'component', library: p.library || 'builtin', component: p.component };
        }
        return null;
    }

    /** The element tag a payload inserts (null for text / library components). */
    function payloadTag(payload, docRoot) {
        if (payload.type === 'component') {
            var builtin = !payload.library || payload.library === 'builtin';
            return builtin && payload.component !== '#text' ? payload.component : null;
        }
        var n = docRoot ? findNode(docRoot, payload.uid) : null;
        return n && n.kind === 'element' ? n.tag : null;
    }

    /** The server message a drop sends, or null when the drop is not allowed. */
    function dropMessage(payload, target, docRoot) {
        payload = normalizePayload(payload);
        if (!payload || !target || target.parent == null) return null;
        var parentNode = docRoot ? findNode(docRoot, target.parent) : null;
        if (docRoot && !canContain(parentNode, payloadTag(payload, docRoot))) return null;
        var msg;
        if (payload.type === 'component') {
            msg = { op: 'builder_insert', parent: target.parent, component: payload.component };
            if (payload.library && payload.library !== 'builtin') msg.library = payload.library;
        } else {
            if (payload.uid === 0) return null;
            if (docRoot && isSelfOrDescendant(docRoot, payload.uid, target.parent)) return null;
            msg = { op: 'builder_move', node: payload.uid, parent: target.parent };
        }
        if (target.index != null) msg.index = target.index;
        return msg;
    }

    /** Move a node one slot up (-1) or down (+1) among its siblings. */
    function stepMessage(docRoot, uid, dir) {
        var row = rowOf(docRoot, uid);
        if (!row || row.parent == null) return null;
        var siblings = findNode(docRoot, row.parent).children || [];
        if (dir < 0) {
            if (row.index === 0) return null;
            return { op: 'builder_move', node: uid, parent: row.parent, index: row.index - 1 };
        }
        if (row.index >= siblings.length - 1) return null;
        // Slots count as seen before the move: after the next sibling is +2.
        return { op: 'builder_move', node: uid, parent: row.parent, index: row.index + 2 };
    }

    /**
     * Where a palette double-click of `childTag` inserts: into the selected
     * node if it can take it, else after it; nothing selected: end of <body>.
     */
    function insertTarget(docRoot, selectedUid, childTag) {
        var row = selectedUid != null ? rowOf(docRoot, selectedUid) : null;
        if (!row) return { parent: 0, index: null };
        if (canContain(row.node, childTag || null)) return { parent: row.uid, index: null };
        return dropTarget(row, 'after');
    }

    /** Palette entries from `get_component_registry`, minus non-visual builtins. */
    function paletteEntries(registry) {
        var out = [];
        ((registry && registry.libraries) || []).forEach(function (lib) {
            (lib.components || []).forEach(function (c) {
                var tag = c.tag || c.name;
                if (!tag) return;
                if (lib.name === 'builtin' && NON_VISUAL.indexOf(tag) !== -1) return;
                out.push({
                    library: lib.name,
                    component: tag,
                    label: c.display_name || tag,
                    description: c.description || '',
                });
            });
        });
        return out;
    }

    /** A component name to suggest for converting `node`: its first class, id or tag. */
    function suggestComponentName(node) {
        var attrs = (node && node.attrs) || {};
        var base = (attrs['class'] || '').split(/\s+/)[0] || attrs.id || (node && node.tag) || '';
        base = String(base).toLowerCase().replace(/[^a-z0-9_-]+/g, '-')
            .replace(/^[^a-z]+/, '').replace(/[-_]+$/, '');
        return base || 'component';
    }

    /** Sanitise what the user typed as a component name (builder.rs rule). */
    function sanitizeComponentName(s) {
        return String(s || '').trim().toLowerCase().replace(/[^a-z0-9_-]+/g, '-')
            .replace(/^[^a-z]+/, '').replace(/-+$/, '');
    }

    // ── B5: the properties panel ──

    /** What the panel offers on every element, in this order (`text` is its text). */
    var ELEMENT_ATTRS = ['text', 'id', 'class', 'style'];
    /** What an instance takes besides its arguments: builder.rs puts them on its root. */
    var PASSTHROUGH = ['class', 'id', 'style'];
    /** Argument types one attribute can carry (builder.rs `data_model_with_args`). */
    var EDITABLE_TYPES = ['String', 'Bool', 'I32', 'I64', 'U32', 'U64', 'Usize', 'F32', 'F64', 'ColorU'];

    /** The registry entry (`get_component_registry`) of `library:name`, or null. */
    function componentDef(registry, library, name) {
        var libs = (registry && registry.libraries) || [];
        for (var i = 0; i < libs.length; i++) {
            if (libs[i].name !== library) continue;
            var comps = libs[i].components || [];
            for (var j = 0; j < comps.length; j++) {
                if ((comps[j].tag || comps[j].name) === name) return comps[j];
            }
        }
        return null;
    }

    /**
     * The rows of the properties panel for a document node:
     * `{name, group, fieldType, value, default, description, required}`.
     * `value` null = the node does not set it (an argument then takes its
     * default). A text node has its text; an element its text, id, classes,
     * style and whatever else it carries; an instance its arguments (the
     * component's data model, `def`), then class / id / style.
     */
    function propertyRows(node, def) {
        if (!node) return [];
        var attrs = node.attrs || {};
        if (node.kind === 'text') {
            return [{ name: 'text', group: 'attribute', fieldType: 'String', value: node.text || '',
                default: '', description: 'The text', required: false }];
        }
        var rows = [];
        var taken = {};
        var attrRow = function (name) {
            taken[name] = true;
            rows.push({ name: name, group: 'attribute', fieldType: 'String',
                value: attrs[name] != null ? String(attrs[name]) : '', default: '', description: '',
                required: false });
        };
        if (node.kind === 'component') {
            ((def && def.data_model) || []).forEach(function (f) {
                if (!f || !f.name || taken[f.name]) return;
                taken[f.name] = true;
                rows.push({ name: f.name, group: 'argument', fieldType: f.field_type || 'String',
                    value: attrs[f.name] != null ? String(attrs[f.name]) : null,
                    default: f.default != null ? String(f.default) : '',
                    description: f.description || '', required: !!f.required });
            });
            PASSTHROUGH.forEach(function (n) { if (!taken[n]) attrRow(n); });
        } else {
            var noText = node.uid === 0 || VOID.indexOf(node.tag) !== -1;
            ELEMENT_ATTRS.forEach(function (n) { if (!(n === 'text' && noText)) attrRow(n); });
        }
        Object.keys(attrs).sort().forEach(function (n) { if (!taken[n]) attrRow(n); });
        return rows;
    }

    /**
     * The message an edit of `name` to `value` sends, or null when nothing
     * changes. Empty removes the attribute (an argument falls back to its
     * default); a text node's text may be empty.
     */
    function propertyMessage(node, name, value) {
        if (!node || !name) return null;
        value = value == null ? '' : String(value);
        if (node.kind === 'text') {
            if (value === (node.text || '')) return null;
            return { op: 'builder_set_attribute', node: node.uid, name: 'text', value: value };
        }
        var current = (node.attrs || {})[name];
        if (value === '') {
            return current == null ? null : { op: 'builder_set_attribute', node: node.uid, name: name };
        }
        if (value === current) return null;
        return { op: 'builder_set_attribute', node: node.uid, name: name, value: value };
    }

    function hex2(n) { return ('0' + ((Number(n) || 0) & 255).toString(16)).slice(-2); }

    /** A field widget's typed value (`{type, value}`) as the attribute text the server parses. */
    function attrString(v) {
        if (v == null) return '';
        if (typeof v !== 'object') return String(v);
        if (v.type === 'None') return '';
        var x = v.value;
        if (v.type === 'Bool') return x ? 'true' : 'false';
        if (v.type === 'ColorU' && x && typeof x === 'object') {
            var a = x.a == null ? 255 : x.a;
            return '#' + hex2(x.r) + hex2(x.g) + hex2(x.b) + (a === 255 ? '' : hex2(a));
        }
        return x == null ? '' : String(x);
    }

    /** The attribute text as a field widget's value of `type`; null: nothing to show. */
    function typedValue(s, type) {
        if (s == null || s === '') return null;
        s = String(s);
        switch (type) {
            case 'Bool': return /^(true|1|yes|on)$/i.test(s.trim());
            case 'I32': case 'I64': case 'U32': case 'U64': case 'Usize': {
                var n = parseInt(s, 10);
                return isNaN(n) ? null : n;
            }
            case 'F32': case 'F64': {
                var f = parseFloat(s);
                return isNaN(f) ? null : f;
            }
            case 'ColorU': {
                var m = /^#?([0-9a-f]{6})([0-9a-f]{2})?$/i.exec(s.trim());
                if (!m) return null;
                var h = parseInt(m[1], 16);
                return { r: (h >> 16) & 255, g: (h >> 8) & 255, b: h & 255, a: m[2] ? parseInt(m[2], 16) : 255 };
            }
            default: return s;
        }
    }

    /** Whether the panel edits an argument of this (parsed) type as one attribute. */
    function editableType(type) { return EDITABLE_TYPES.indexOf(type) !== -1; }

    // ── B5: the document's stylesheet ──

    /** The message "Apply" sends for `text`, or null when the document has it already. */
    function stylesheetMessage(doc, text) {
        if (!doc) return null;
        text = text == null ? '' : String(text);
        return text === (doc.stylesheet || '') ? null : { op: 'builder_set_stylesheet', css: text };
    }

    /**
     * What the editor shows: the document's stylesheet (after an undo, a load,
     * an apply), unless it holds text the user has not applied yet.
     */
    function sheetText(shown, docSheet, dirty) {
        return dirty ? shown : (docSheet || '');
    }

    var logic = {
        NON_VISUAL: NON_VISUAL, VOID: VOID, AUTO_CLOSE: AUTO_CLOSE,
        acceptsChildren: acceptsChildren, canContain: canContain, dropZone: dropZone, flatten: flatten,
        dropTarget: dropTarget, findNode: findNode, rowOf: rowOf,
        isSelfOrDescendant: isSelfOrDescendant, normalizePayload: normalizePayload,
        dropMessage: dropMessage, stepMessage: stepMessage, insertTarget: insertTarget,
        paletteEntries: paletteEntries, suggestComponentName: suggestComponentName,
        sanitizeComponentName: sanitizeComponentName,
        // B5
        componentDef: componentDef, propertyRows: propertyRows, propertyMessage: propertyMessage,
        attrString: attrString, typedValue: typedValue, editableType: editableType,
        stylesheetMessage: stylesheetMessage, sheetText: sheetText,
    };

    if (typeof module !== 'undefined' && module.exports) module.exports = logic;
    // Under node (the unit test) there is no page and no `app`: logic only.
    if (typeof document === 'undefined' || typeof app === 'undefined') return;

    // =====================================================================
    // Browser UI
    // =====================================================================

    var MODE_KEY = 'azul_builder_tree_mode';
    var DRAG_MIME = 'application/x-azul-builder';
    var THUMB_WIDTH = 140;
    var THUMB_DPI = 2;
    var THUMB_CONCURRENCY = 2;

    var S = {
        mode: null,              // 'document' | 'live'
        doc: null,               // {active, can_undo, can_redo, root}
        selected: null,          // uid
        collapsed: new Set(),
        drag: null,              // payload of the drag in progress (dataTransfer is unreadable in dragover)
        dropAt: null,            // {uid, zone} under the pointer
        expandTimer: null,
        thumbs: {},              // 'lib:name' -> {data, width, height} | 'pending' | 'empty'
        thumbQueue: [],
        thumbBusy: 0,
        palette: [],
        paletteFilter: '',
        observer: null,
        liveCache: null,         // last get_node_hierarchy value
        registry: null,          // last get_component_registry value (the panel's data models)
        sheetDirty: false,       // the stylesheet editor holds text not applied yet
    };

    var origRefreshSidebar = app.handlers.refreshSidebar;
    var origRenderDomTree = app.ui.renderDomTree;

    // ── server ──

    async function call(msg) {
        var res = await app.api.post(msg);
        if (!res || res.status !== 'ok') {
            throw new Error((res && res.message) || ('"' + msg.op + '" failed'));
        }
        if (res.data && res.data.value !== undefined) return res.data.value;
        return res.data || null;
    }

    function describeMsg(msg) {
        switch (msg.op) {
            case 'builder_insert': return 'Insert ' + (msg.library ? msg.library + ':' : '') + msg.component;
            case 'builder_move': return 'Move node ' + msg.node;
            case 'builder_delete': return 'Delete node ' + msg.node;
            case 'builder_set_attribute': return 'Set ' + msg.name + ' on node ' + msg.node;
            case 'builder_undo': return 'Undo';
            case 'builder_redo': return 'Redo';
            default: return msg.op;
        }
    }

    /** Send an edit; the answer is the new document — render it at once. */
    async function send(msg, what) {
        what = what || describeMsg(msg);
        try {
            var doc = await call(msg);
            setDoc(doc, msg);
            app.log(what + ': ok', 'info');
            return doc;
        } catch (err) {
            var quiet = /nothing to (undo|redo)/.test(err.message);
            app.log(what + ': ' + err.message, quiet ? 'info' : 'error');
            return null;
        }
    }

    function setDoc(doc, msg) {
        if (!doc || !doc.root) return;
        S.doc = doc;
        S.liveCache = null;
        if (doc.inserted != null) S.selected = doc.inserted;
        else if (msg && msg.op === 'builder_move') S.selected = msg.node;
        if (S.selected != null && !findNode(doc.root, S.selected)) S.selected = null;
        if (S.mode === 'document') renderDocumentTree();
        updateToolbar();
    }

    async function refreshDocument() {
        try {
            var doc = await call({ op: 'builder_get_document' });
            if (doc && doc.root) { S.doc = doc; }
        } catch (err) {
            S.doc = null;
            app.log('Builder document: ' + err.message, 'error');
        }
        renderDocumentTree();
        updateToolbar();
    }

    // ── mode ──

    async function decideMode() {
        var saved = null;
        try { saved = localStorage.getItem(MODE_KEY); } catch (e) { /* private mode */ }
        if (saved === 'document' || saved === 'live') { S.mode = saved; return; }
        try {
            var doc = await call({ op: 'builder_get_document' });
            S.doc = doc && doc.root ? doc : null;
            // AzBuilder's own window (an empty body) or a builder that already
            // took the window over: edit. Any other app: inspect.
            S.mode = S.doc && (S.doc.active || !(S.doc.root.children || []).length) ? 'document' : 'live';
        } catch (e) {
            S.mode = 'live';
        }
    }

    function setMode(mode) {
        if (mode === S.mode) return;
        S.mode = mode;
        try { localStorage.setItem(MODE_KEY, mode); } catch (e) { /* ignore */ }
        updateToolbar();
        var c = document.getElementById('dom-tree-container');
        if (c) c.classList.toggle('azb-doc-tree', mode === 'document');
        if (mode === 'document') refreshDocument();
        else origRefreshSidebar.call(app.handlers);
    }

    // ── toolbar ──

    function button(act, icon, title) {
        var b = document.createElement('button');
        b.className = 'azb-icon';
        b.dataset.act = act;
        b.title = title;
        b.innerHTML = '<span class="material-icons">' + icon + '</span>';
        return b;
    }

    function injectToolbar() {
        var sidebar = document.getElementById('sidebar-inspector');
        var tree = document.getElementById('dom-tree-container');
        if (!sidebar || !tree || document.getElementById('azb-toolbar')) return;
        var bar = document.createElement('div');
        bar.id = 'azb-toolbar';
        bar.className = 'azb-toolbar';
        var seg = document.createElement('div');
        seg.className = 'azb-seg';
        [['document', 'Document', 'Edit the builder document (drop components here)'],
         ['live', 'Live DOM', 'Inspect the live DOM of the window']].forEach(function (m) {
            var b = document.createElement('button');
            b.dataset.mode = m[0];
            b.textContent = m[1];
            b.title = m[2];
            b.addEventListener('click', function () { setMode(m[0]); });
            seg.appendChild(b);
        });
        bar.appendChild(seg);
        var spacer = document.createElement('span');
        spacer.className = 'azb-spacer';
        bar.appendChild(spacer);
        bar.appendChild(button('undo', 'undo', 'Undo (Ctrl/Cmd+Z)'));
        bar.appendChild(button('redo', 'redo', 'Redo (Shift+Ctrl/Cmd+Z)'));
        bar.appendChild(button('convert', 'widgets', 'Convert the selected subtree to a component'));
        bar.appendChild(button('delete', 'delete', 'Delete the selected node (Del)'));
        bar.appendChild(button('reset', 'restart_alt', 'Discard the document and give the window back to the app'));
        bar.addEventListener('click', function (e) {
            var b = e.target.closest ? e.target.closest('button[data-act]') : null;
            if (!b || b.disabled) return;
            runAction(b.dataset.act);
        });
        sidebar.insertBefore(bar, tree);
    }

    function updateToolbar() {
        var bar = document.getElementById('azb-toolbar');
        if (!bar) return;
        bar.querySelectorAll('.azb-seg button').forEach(function (b) {
            b.classList.toggle('active', b.dataset.mode === S.mode);
        });
        var docMode = S.mode === 'document' && !!S.doc;
        var sel = docMode && S.selected != null && S.selected !== 0 ? findNode(S.doc.root, S.selected) : null;
        var set = function (act, enabled) {
            var b = bar.querySelector('button[data-act="' + act + '"]');
            if (b) { b.disabled = !enabled; b.style.display = S.mode === 'document' ? '' : 'none'; }
        };
        set('undo', docMode && !!S.doc.can_undo);
        set('redo', docMode && !!S.doc.can_redo);
        set('convert', !!sel && sel.kind === 'element');
        set('delete', !!sel);
        set('reset', docMode && !!S.doc.active);
        // Every change of mode, document or selection passes here.
        refreshPanels();
    }

    function runAction(act) {
        if (act === 'undo') return send({ op: 'builder_undo' });
        if (act === 'redo') return send({ op: 'builder_redo' });
        if (act === 'delete') return deleteNode(S.selected);
        if (act === 'convert') return convertToComponent(S.selected);
        if (act === 'reset') {
            if (!confirm('Discard the builder document and give the window back to the app? (This cannot be undone.)')) return;
            return call({ op: 'builder_reset' }).then(function () {
                S.selected = null;
                app.log('Builder document discarded', 'info');
                return refreshDocument();
            }).catch(function (err) { app.log('Reset: ' + err.message, 'error'); });
        }
    }

    // ── the document tree ──

    function renderDocumentTree() {
        var container = document.getElementById('dom-tree-container');
        if (!container || S.mode !== 'document') return;
        container.classList.add('azb-doc-tree');
        container.innerHTML = '';
        if (!S.doc || !S.doc.root) {
            container.innerHTML = '<div class="placeholder-text">No builder document — is the app running with AZ_DEBUG?</div>';
            return;
        }
        flatten(S.doc.root, S.collapsed).forEach(function (row) {
            container.appendChild(buildRow(row));
        });
        if (!(S.doc.root.children || []).length) {
            var hint = document.createElement('div');
            hint.className = 'azb-hint';
            hint.textContent = 'Drag a component from the palette below onto <body> — or double-click it.';
            container.appendChild(hint);
        }
        var sel = container.querySelector('.tree-row.selected');
        if (sel && sel.scrollIntoView) sel.scrollIntoView({ block: 'nearest' });
    }

    function labelFor(node) {
        var label = document.createElement('span');
        label.className = 'tree-label';
        var attrs = node.attrs || {};
        if (node.kind === 'text') {
            var t = document.createElement('span');
            t.className = 'tree-text-content';
            t.textContent = '"' + (node.text || '') + '"';
            label.appendChild(t);
            return label;
        }
        if (node.kind === 'component') {
            var badge = document.createElement('span');
            badge.className = 'tree-component-badge';
            badge.style.marginLeft = '0';
            badge.textContent = node.library + ':' + node.tag;
            label.appendChild(badge);
        } else {
            var tag = document.createElement('span');
            tag.className = 'tree-tag';
            tag.textContent = node.tag;
            label.appendChild(tag);
        }
        if (attrs.id) {
            var id = document.createElement('span');
            id.className = 'tree-id';
            id.textContent = ' #' + attrs.id;
            label.appendChild(id);
        }
        if (attrs['class']) {
            var cls = document.createElement('span');
            cls.className = 'tree-class';
            cls.textContent = ' .' + attrs['class'].trim().split(/\s+/).join('.');
            label.appendChild(cls);
        }
        if (attrs.text) {
            var txt = document.createElement('span');
            txt.className = 'tree-text-content';
            txt.style.marginLeft = '6px';
            txt.textContent = '"' + attrs.text + '"';
            label.appendChild(txt);
        }
        return label;
    }

    function buildRow(row) {
        var node = row.node;
        var el = document.createElement('div');
        el.className = 'tree-row azb-row'
            + (S.selected === node.uid ? ' selected' : '')
            + (node.kind === 'component' ? ' component-root' : '');
        el.dataset.uid = node.uid;
        el.dataset.type = node.kind === 'text' ? 'text' : 'element';
        el.style.setProperty('--azb-indent', (row.depth * 16 + 20) + 'px');
        el.draggable = row.parent != null;

        var indent = document.createElement('span');
        indent.className = 'tree-indent';
        indent.style.width = (row.depth * 16 + 4) + 'px';
        el.appendChild(indent);

        var toggle = document.createElement('span');
        toggle.className = 'tree-toggle';
        if ((node.children || []).length) {
            toggle.textContent = S.collapsed.has(node.uid) ? '▶' : '▼';
            toggle.addEventListener('click', function (e) {
                e.stopPropagation();
                if (S.collapsed.has(node.uid)) S.collapsed.delete(node.uid); else S.collapsed.add(node.uid);
                renderDocumentTree();
            });
        } else {
            toggle.innerHTML = '&nbsp;';
        }
        el.appendChild(toggle);
        el.appendChild(labelFor(node));

        el.addEventListener('click', function (e) { e.stopPropagation(); select(node.uid); });
        el.addEventListener('dblclick', function (e) { e.stopPropagation(); editText(node.uid); });
        el.addEventListener('contextmenu', function (e) {
            e.preventDefault();
            e.stopPropagation();
            select(node.uid);
            showRowMenu(e, row);
        });

        // Drag a row to move it.
        el.addEventListener('dragstart', function (e) {
            if (row.parent == null) { e.preventDefault(); return; }
            S.drag = { type: 'builder-node', uid: node.uid };
            e.dataTransfer.setData(DRAG_MIME, JSON.stringify(S.drag));
            e.dataTransfer.setData('text/plain', JSON.stringify(S.drag));
            e.dataTransfer.effectAllowed = 'move';
            el.classList.add('azb-dragging');
        });
        el.addEventListener('dragend', function () {
            el.classList.remove('azb-dragging');
            endDrag();
        });

        // Drop on a row: before / into / after.
        el.addEventListener('dragover', function (e) { onRowDragOver(e, row, el); });
        el.addEventListener('dragleave', function (e) {
            if (e.relatedTarget && el.contains(e.relatedTarget)) return;
            el.classList.remove('azb-drop-before', 'azb-drop-after', 'azb-drop-into');
        });
        el.addEventListener('drop', function (e) { onRowDrop(e, row, el); });
        return el;
    }

    function clearIndicators() {
        document.querySelectorAll('.azb-drop-before, .azb-drop-after, .azb-drop-into').forEach(function (n) {
            n.classList.remove('azb-drop-before', 'azb-drop-after', 'azb-drop-into');
        });
        var c = document.getElementById('dom-tree-container');
        if (c) c.classList.remove('azb-drop-end');
    }

    function endDrag() {
        S.drag = null;
        S.dropAt = null;
        if (S.expandTimer) { clearTimeout(S.expandTimer); S.expandTimer = null; }
        clearIndicators();
    }

    function currentPayload(e) {
        if (S.drag) return S.drag;
        // A drag that started outside this module (the Components view list):
        // its data is unreadable until drop, so assume a component.
        var types = (e && e.dataTransfer && e.dataTransfer.types) || [];
        for (var i = 0; i < types.length; i++) {
            if (types[i] === 'text/plain' || types[i] === DRAG_MIME) {
                return { type: 'component', component: '?' };
            }
        }
        return null;
    }

    function readPayload(e) {
        if (S.drag) return S.drag;
        var dt = e.dataTransfer;
        var raw = (dt && (dt.getData(DRAG_MIME) || dt.getData('text/plain'))) || '';
        try { return normalizePayload(JSON.parse(raw)); } catch (err) { return null; }
    }

    function onRowDragOver(e, row, el) {
        if (!S.doc) return;
        var payload = currentPayload(e);
        if (!payload) return;
        var rect = el.getBoundingClientRect();
        var zone = dropZone((e.clientY - rect.top) / Math.max(rect.height, 1), row.node, row.parent == null);
        var msg = dropMessage(payload, dropTarget(row, zone), S.doc.root);
        clearIndicators();
        if (!msg) {
            e.dataTransfer.dropEffect = 'none';
            S.dropAt = null;
            return;
        }
        e.preventDefault();
        e.stopPropagation();
        e.dataTransfer.dropEffect = payload.type === 'builder-node' ? 'move' : 'copy';
        el.classList.add('azb-drop-' + zone);
        var same = S.dropAt && S.dropAt.uid === row.uid && S.dropAt.zone === zone;
        S.dropAt = { uid: row.uid, zone: zone };
        // Hovering INTO a collapsed row opens it, as file managers do.
        if (!same) {
            if (S.expandTimer) { clearTimeout(S.expandTimer); S.expandTimer = null; }
            if (zone === 'into' && S.collapsed.has(row.uid)) {
                S.expandTimer = setTimeout(function () {
                    S.expandTimer = null;
                    S.collapsed.delete(row.uid);
                    renderDocumentTree();
                }, 600);
            }
        }
    }

    async function onRowDrop(e, row, el) {
        e.preventDefault();
        e.stopPropagation();
        var rect = el.getBoundingClientRect();
        var zone = S.dropAt && S.dropAt.uid === row.uid
            ? S.dropAt.zone
            : dropZone((e.clientY - rect.top) / Math.max(rect.height, 1), row.node, row.parent == null);
        var payload = readPayload(e);
        endDrag();
        var msg = dropMessage(payload, dropTarget(row, zone), S.doc && S.doc.root);
        if (msg) await send(msg);
    }

    /** Below the last row: append to <body>. */
    function installContainerDrop() {
        var c = document.getElementById('dom-tree-container');
        if (!c) return;
        c.addEventListener('dragover', function (e) {
            if (S.mode !== 'document' || !S.doc) return;
            if (e.target !== c && !(e.target.classList && e.target.classList.contains('azb-hint'))) return;
            var payload = currentPayload(e);
            if (!dropMessage(payload, { parent: 0, index: null }, S.doc.root)) return;
            e.preventDefault();
            clearIndicators();
            c.classList.add('azb-drop-end');
            e.dataTransfer.dropEffect = payload.type === 'builder-node' ? 'move' : 'copy';
        });
        c.addEventListener('dragleave', function (e) {
            if (e.target === c) c.classList.remove('azb-drop-end');
        });
        c.addEventListener('drop', async function (e) {
            if (S.mode !== 'document' || !S.doc) return;
            if (e.target !== c && !(e.target.classList && e.target.classList.contains('azb-hint'))) return;
            e.preventDefault();
            var payload = readPayload(e);
            endDrag();
            var msg = dropMessage(payload, { parent: 0, index: null }, S.doc.root);
            if (msg) await send(msg);
        });
    }

    // ── selection, detail panel ──

    function select(uid) {
        S.selected = uid;
        var c = document.getElementById('dom-tree-container');
        if (c) {
            c.querySelectorAll('.azb-row.selected').forEach(function (r) { r.classList.remove('selected'); });
            var r = c.querySelector('.azb-row[data-uid="' + uid + '"]');
            if (r) r.classList.add('selected');
        }
        updateToolbar();
        showLiveDetail(uid);
        // B4: the project viewer (debugger-project.js) follows the selection
        // (an instance selects its component file).
        document.dispatchEvent(new CustomEvent('azb:select', { detail: { uid: uid } }));
    }

    /**
     * Every mounted document element carries the class `azb-<uid>`; find the
     * live node for the detail panel (CSS, layout, screenshot) without
     * re-rendering the tree.
     */
    async function showLiveDetail(uid) {
        try {
            var h = S.liveCache || (S.liveCache = await call({ op: 'get_node_hierarchy' }));
            var nodes = (h && h.nodes) || [];
            var mark = 'azb-' + uid;
            var live = null;
            for (var i = 0; i < nodes.length; i++) {
                if ((nodes[i].classes || []).indexOf(mark) !== -1) { live = nodes[i]; break; }
            }
            if (!live || S.selected !== uid) return;
            app.state.hierarchy = nodes;
            app.state.hierarchyRoot = h.root != null ? h.root : 0;
            app.state.selectedNodeId = live.index;
            app.ui.renderNodeDetail(live);
        } catch (e) { /* not mounted yet: no detail */ }
    }

    // ── edits ──

    async function deleteNode(uid) {
        if (uid == null || uid === 0 || !S.doc) return null;
        var row = rowOf(S.doc.root, uid);
        var doc = await send({ op: 'builder_delete', node: uid });
        // Keep working where the node was: select its parent.
        if (doc && row && row.parent != null && findNode(doc.root, row.parent)) {
            select(row.parent);
        }
        return doc;
    }

    /** An element's / text node's text, or a component instance's `text` argument. */
    function editText(uid) {
        var node = S.doc ? findNode(S.doc.root, uid) : null;
        if (!node) return null;
        if (node.kind === 'element' && VOID.indexOf(node.tag) !== -1) return null;
        var current = node.kind === 'text' ? (node.text || '') : ((node.attrs || {}).text || '');
        var value = prompt(node.kind === 'component' ? 'Argument "text":' : 'Text:', current);
        if (value === null) return null;
        return send({ op: 'builder_set_attribute', node: uid, name: 'text', value: value });
    }

    function editAttr(uid, name, title) {
        var node = S.doc ? findNode(S.doc.root, uid) : null;
        if (!node) return null;
        var value = prompt((title || name) + ' (empty removes it):', (node.attrs || {})[name] || '');
        if (value === null) return null;
        var msg = { op: 'builder_set_attribute', node: uid, name: name };
        if (value.trim() !== '') msg.value = value;
        return send(msg);
    }

    async function pickLibrary() {
        try {
            var v = await call({ op: 'get_libraries' });
            var libs = ((v && v.libraries) || []).filter(function (l) { return l.modifiable; });
            if (libs.length === 1) return libs[0].name;
            if (libs.length > 1) {
                var names = libs.map(function (l) { return l.name; });
                var chosen = prompt('Add the component to which library? (' + names.join(', ') + ')', names[0]);
                if (chosen === null) return null;
                chosen = chosen.trim();
                return chosen || names[0];
            }
        } catch (e) { /* fall through */ }
        return 'user';
    }

    async function convertToComponent(uid) {
        if (!S.doc || uid == null || uid === 0) return;
        var node = findNode(S.doc.root, uid);
        if (!node || node.kind !== 'element') {
            app.log('Convert to component: select an element (not text, not an instance)', 'warning');
            return;
        }
        var typed = prompt('Component name (lowercase, e.g. "my-card"):', suggestComponentName(node));
        if (typed === null) return;
        var name = sanitizeComponentName(typed);
        if (!name) { app.log('Convert to component: "' + typed + '" is not a usable name', 'error'); return; }
        var library = await pickLibrary();
        if (!library) return;
        var doc = await send({ op: 'builder_convert_to_component', node: uid, library: library, name: name },
            'Convert node ' + uid + ' to ' + library + ':' + name);
        if (doc && doc.component) {
            var fields = (doc.component.fields || []).map(function (f) { return f.name; });
            app.log('Component ' + library + ':' + name + ' created'
                + (fields.length ? ' with parameters ' + fields.join(', ') : ''), 'info');
            delete S.thumbs[library + ':' + name];
            renderPalette();
            if (app.state.libraryList) app.handlers.loadLibraries();
        }
    }

    function insertEntry(entry) {
        if (!S.doc) return null;
        var payload = { type: 'component', library: entry.library, component: entry.component };
        var target = insertTarget(S.doc.root, S.selected,
            entry.library === 'builtin' ? entry.component : null);
        var msg = dropMessage(payload, target, S.doc.root);
        if (!msg) {
            app.log('Cannot insert ' + entry.component + ' there (the selection cannot contain it)', 'warning');
            return null;
        }
        return send(msg);
    }

    function showRowMenu(e, row) {
        var node = row.node;
        var isRoot = row.parent == null;
        var items = [];
        if (acceptsChildren(node)) {
            [['div', 'Insert <div> inside'], ['p', 'Insert <p> inside'], ['span', 'Insert <span> inside'],
             ['button', 'Insert <button> inside']].forEach(function (it) {
                items.push({ icon: 'add', label: it[1], action: function () {
                    send({ op: 'builder_insert', parent: node.uid, component: it[0] });
                } });
            });
            items.push({ icon: 'text_fields', label: 'Insert text inside', action: function () {
                var t = prompt('Text:', 'Text');
                if (t !== null) send({ op: 'builder_insert', parent: node.uid, component: '#text', attrs: { text: t } });
            } });
            items.push({ separator: true });
        }
        if (node.kind !== 'element' || VOID.indexOf(node.tag) === -1) {
            items.push({ icon: 'edit', label: 'Edit text…', action: function () { editText(node.uid); } });
        }
        if (node.kind !== 'text') {
            items.push({ icon: 'label', label: 'Set classes…', action: function () { editAttr(node.uid, 'class', 'Classes'); } });
            items.push({ icon: 'tag', label: 'Set id…', action: function () { editAttr(node.uid, 'id', 'Id'); } });
        }
        if (!isRoot) {
            var up = stepMessage(S.doc.root, node.uid, -1);
            var down = stepMessage(S.doc.root, node.uid, +1);
            if (up) items.push({ icon: 'arrow_upward', label: 'Move up', action: function () { send(up); } });
            if (down) items.push({ icon: 'arrow_downward', label: 'Move down', action: function () { send(down); } });
            if (node.kind === 'element') {
                items.push({ separator: true });
                items.push({ icon: 'widgets', label: 'Convert to component…', action: function () { convertToComponent(node.uid); } });
            }
            items.push({ separator: true });
            items.push({ icon: 'delete', label: 'Delete', danger: true, action: function () { deleteNode(node.uid); } });
        }
        if (items.length && app.widgets && app.widgets.ContextMenu) {
            app.widgets.ContextMenu.show(e.clientX, e.clientY, items);
        }
    }

    function onKeyDown(e) {
        if (S.mode !== 'document' || app.state.currentView !== 'inspector' || !S.doc) return;
        var t = e.target;
        var tag = t && t.tagName ? t.tagName.toLowerCase() : '';
        if (tag === 'input' || tag === 'textarea' || tag === 'select' || (t && t.isContentEditable)) return;
        var mod = e.metaKey || e.ctrlKey;
        var key = e.key;
        if (mod && (key === 'z' || key === 'Z')) {
            e.preventDefault();
            send({ op: e.shiftKey ? 'builder_redo' : 'builder_undo' });
        } else if (mod && (key === 'y' || key === 'Y')) {
            e.preventDefault();
            send({ op: 'builder_redo' });
        } else if ((key === 'Delete' || key === 'Backspace') && S.selected != null && S.selected !== 0) {
            e.preventDefault();
            deleteNode(S.selected);
        } else if ((key === 'ArrowDown' || key === 'ArrowUp') && S.selected != null) {
            var rows = flatten(S.doc.root, S.collapsed);
            for (var i = 0; i < rows.length; i++) {
                if (rows[i].uid !== S.selected) continue;
                var next = rows[i + (key === 'ArrowDown' ? 1 : -1)];
                if (next) { e.preventDefault(); select(next.uid); }
                break;
            }
        } else if ((key === 'F2' || key === 'Enter') && S.selected != null) {
            e.preventDefault();
            editText(S.selected);
        }
    }

    // ── palette ──

    function entryKey(entry) { return entry.library + ':' + entry.component; }

    async function renderPalette() {
        var container = document.getElementById('palette-component-list');
        if (!container) return;
        var reg;
        try {
            reg = await call({ op: 'get_component_registry' });
        } catch (err) {
            container.innerHTML = '<div class="placeholder-text" style="font-size:11px">Failed to load components.</div>';
            return;
        }
        S.palette = paletteEntries(reg);
        S.registry = reg;
        // Fresh pictures on every reload; the server answers from its cache
        // unless the component changed.
        S.thumbs = {};
        S.thumbQueue = [];
        drawPalette(container);
        // An instance's arguments come from the registry's data models.
        refreshPanels();
    }

    function drawPalette(container) {
        container.innerHTML = '';
        container.style.maxHeight = '45vh';
        if (!S.palette.length) {
            container.innerHTML = '<div class="placeholder-text" style="font-size:11px">No components.</div>';
            return;
        }
        var filter = document.createElement('input');
        filter.type = 'text';
        filter.className = 'azb-palette-filter';
        filter.placeholder = 'Filter components…';
        filter.value = S.paletteFilter;
        var grids = document.createElement('div');
        filter.addEventListener('input', function () {
            S.paletteFilter = filter.value;
            drawCards(grids, container);
        });
        container.appendChild(filter);
        container.appendChild(grids);
        drawCards(grids, container);
    }

    function drawCards(grids, container) {
        grids.innerHTML = '';
        if (S.observer) { S.observer.disconnect(); S.observer = null; }
        if (typeof IntersectionObserver !== 'undefined') {
            S.observer = new IntersectionObserver(function (entries) {
                entries.forEach(function (en) {
                    if (!en.isIntersecting) return;
                    var card = en.target;
                    S.observer.unobserve(card);
                    var entry = card._azbEntry;
                    if (entry) enqueueThumb(entry);
                });
            }, { root: container, rootMargin: '64px' });
        }
        var f = S.paletteFilter.trim().toLowerCase();
        var byLib = {};
        var order = [];
        S.palette.forEach(function (entry) {
            if (f && (entry.label + ' ' + entry.component + ' ' + entry.library).toLowerCase().indexOf(f) === -1) return;
            if (!byLib[entry.library]) { byLib[entry.library] = []; order.push(entry.library); }
            byLib[entry.library].push(entry);
        });
        if (!order.length) {
            grids.innerHTML = '<div class="placeholder-text" style="font-size:11px">No matching components.</div>';
            return;
        }
        order.forEach(function (lib) {
            var h = document.createElement('div');
            h.className = 'azb-palette-lib';
            h.textContent = lib;
            grids.appendChild(h);
            var grid = document.createElement('div');
            grid.className = 'azb-palette-grid';
            byLib[lib].forEach(function (entry) {
                var card = buildCard(entry);
                grid.appendChild(card);
                if (S.observer) S.observer.observe(card); else enqueueThumb(entry);
            });
            grids.appendChild(grid);
        });
    }

    function buildCard(entry) {
        var card = document.createElement('div');
        card.className = 'azb-card';
        card.draggable = true;
        card.dataset.key = entryKey(entry);
        card._azbEntry = entry;
        card.title = entry.label + '  (' + entry.library + ':' + entry.component + ')'
            + (entry.description ? '\n' + entry.description : '')
            + '\nDrag into the Document tree, or double-click to insert at the selection.';
        var thumb = document.createElement('div');
        thumb.className = 'azb-thumb';
        fillThumb(thumb, entry);
        var label = document.createElement('div');
        label.className = 'azb-card-label';
        label.textContent = entry.label;
        card.appendChild(thumb);
        card.appendChild(label);

        card.addEventListener('dragstart', function (e) {
            S.drag = { type: 'component', library: entry.library, component: entry.component };
            e.dataTransfer.setData(DRAG_MIME, JSON.stringify(S.drag));
            e.dataTransfer.setData('text/plain', JSON.stringify(S.drag));
            e.dataTransfer.effectAllowed = 'copy';
            // The native preview IS the drag image, as in Qt Creator.
            var img = thumb.querySelector('img');
            if (img && img.complete && img.naturalWidth && e.dataTransfer.setDragImage) {
                e.dataTransfer.setDragImage(img, Math.min(24, img.width / 2), Math.min(16, img.height / 2));
            }
            // Dropping needs the document view.
            if (S.mode !== 'document') setMode('document');
        });
        card.addEventListener('dragend', endDrag);
        card.addEventListener('dblclick', function () {
            if (S.mode !== 'document') { setMode('document'); return; }
            insertEntry(entry);
        });
        return card;
    }

    function fillThumb(thumb, entry) {
        var t = S.thumbs[entryKey(entry)];
        thumb.innerHTML = '';
        thumb.classList.toggle('azb-loading', t === 'pending' || t === undefined);
        if (t && t !== 'pending' && t !== 'empty' && t.data) {
            var img = document.createElement('img');
            img.alt = entry.label;
            img.draggable = false;
            img.src = t.data;
            thumb.appendChild(img);
            return;
        }
        var letter = document.createElement('span');
        letter.className = 'azb-thumb-letter';
        letter.textContent = t === 'empty' ? '<' + entry.component + '>' : (entry.label || '?').charAt(0).toUpperCase();
        if (t === 'empty') letter.classList.add('azb-thumb-tag');
        thumb.appendChild(letter);
    }

    function enqueueThumb(entry) {
        var k = entryKey(entry);
        if (S.thumbs[k]) return;
        S.thumbs[k] = 'pending';
        S.thumbQueue.push(entry);
        pumpThumbs();
    }

    function pumpThumbs() {
        while (S.thumbBusy < THUMB_CONCURRENCY && S.thumbQueue.length) {
            loadThumb(S.thumbQueue.shift());
        }
    }

    function loadThumb(entry) {
        var k = entryKey(entry);
        S.thumbBusy++;
        call({ op: 'get_component_thumbnail', library: entry.library, name: entry.component,
               width: THUMB_WIDTH, dpi: THUMB_DPI })
            .then(function (t) {
                S.thumbs[k] = t && t.data ? { data: t.data, width: t.width, height: t.height } : 'empty';
            })
            .catch(function () { S.thumbs[k] = 'empty'; })
            .then(function () {
                S.thumbBusy--;
                document.querySelectorAll('.azb-card').forEach(function (card) {
                    if (card.dataset.key === k) fillThumb(card.querySelector('.azb-thumb'), entry);
                });
                pumpThumbs();
            });
    }

    // ── B5: the Inspector's builder panels ──
    //
    // In Document mode the Inspector's editor area is a row: the node detail
    // (debugger.js) on the left, the builder's side panel on the right
    // (Properties). Live DOM mode hides the builder's part.

    var GROUP_TITLES = { argument: 'Arguments', attribute: 'Attributes' };

    function injectInspectorLayout() {
        var view = document.getElementById('view-inspector');
        var detail = document.getElementById('node-detail-panel');
        if (!view || !detail || document.getElementById('azb-side')) return;
        var row = document.createElement('div');
        row.id = 'azb-inspector-row';
        row.className = 'azb-inspector-row';
        var main = document.createElement('div');
        main.id = 'azb-inspector-main';
        main.className = 'azb-inspector-main';
        view.insertBefore(row, detail);
        main.appendChild(detail);
        row.appendChild(main);

        var side = document.createElement('div');
        side.id = 'azb-side';
        // Shown once the mode is known (refreshPanels).
        side.className = 'azb-side hidden';
        var head = document.createElement('div');
        head.className = 'sidebar-header';
        head.innerHTML = '<span>Properties</span><span id="azb-props-what" class="azb-props-what"></span>';
        var props = document.createElement('div');
        props.id = 'azb-props';
        props.className = 'azb-props';
        side.appendChild(head);
        side.appendChild(props);
        side.appendChild(buildSheetEditor());
        row.appendChild(side);
    }

    /** Show / hide the builder panels for the mode and re-render them. */
    function refreshPanels() {
        var docMode = S.mode === 'document';
        var side = document.getElementById('azb-side');
        if (side) side.classList.toggle('hidden', !docMode);
        if (docMode) {
            renderProps();
            renderSheet();
        }
    }

    // ── the document's stylesheet ──

    var SHEET_HINT = "The document's own CSS: applied after the components' CSS, saved with "
        + 'the document, exported as the app’s stylesheet. Ctrl/Cmd+Enter applies.';

    function buildSheetEditor() {
        var box = document.createElement('div');
        box.id = 'azb-sheet';
        box.className = 'azb-sheet';
        var head = document.createElement('div');
        head.className = 'sidebar-header azb-sheet-head';
        head.innerHTML = '<span>Stylesheet <span class="azb-sheet-dot" title="Not applied yet">●</span></span>';
        var apply = document.createElement('button');
        apply.id = 'azb-sheet-apply';
        apply.className = 'btn-sm';
        apply.type = 'button';
        apply.textContent = 'Apply';
        apply.title = 'Apply to the document (one undo step)';
        apply.addEventListener('click', applySheet);
        head.appendChild(apply);
        var text = document.createElement('textarea');
        text.id = 'azb-sheet-text';
        text.className = 'azb-sheet-text';
        text.spellcheck = false;
        text.placeholder = '.card { padding: 12px; }';
        text.addEventListener('input', function () {
            S.sheetDirty = true;
            box.classList.add('azb-dirty');
        });
        text.addEventListener('keydown', function (e) {
            var mod = e.ctrlKey || e.metaKey;
            if (mod && (e.key === 'Enter' || e.key === 's' || e.key === 'S')) {
                e.preventDefault();
                applySheet();
            } else if (e.key === 'Tab' && !mod && !e.altKey && !e.shiftKey) {
                e.preventDefault();
                var s = text.selectionStart;
                text.value = text.value.slice(0, s) + '    ' + text.value.slice(text.selectionEnd);
                text.selectionStart = text.selectionEnd = s + 4;
                text.dispatchEvent(new Event('input'));
            }
        });
        var status = document.createElement('div');
        status.id = 'azb-sheet-status';
        status.className = 'azb-sheet-status';
        status.textContent = SHEET_HINT;
        box.appendChild(head);
        box.appendChild(text);
        box.appendChild(status);
        return box;
    }

    function renderSheet() {
        var text = document.getElementById('azb-sheet-text');
        var box = document.getElementById('azb-sheet');
        if (!text || !box) return;
        var want = sheetText(text.value, S.doc && S.doc.stylesheet, S.sheetDirty);
        // Only when it differs: re-setting the value would move the caret.
        if (text.value !== want) text.value = want;
        box.classList.toggle('azb-dirty', S.sheetDirty);
    }

    function sheetStatus(message, kind) {
        var status = document.getElementById('azb-sheet-status');
        if (!status) return;
        status.textContent = message;
        status.className = 'azb-sheet-status' + (kind ? ' azb-sheet-' + kind : '');
    }

    async function applySheet() {
        var text = document.getElementById('azb-sheet-text');
        if (!text || !S.doc) return;
        var msg = stylesheetMessage(S.doc, text.value);
        if (!msg) {
            S.sheetDirty = false;
            renderSheet();
            sheetStatus('Unchanged: the document already has this stylesheet.');
            return;
        }
        // Clean BEFORE the answer renders, so the editor takes the applied text.
        S.sheetDirty = false;
        var doc = await send(msg, 'Set the document stylesheet');
        if (!doc) {
            S.sheetDirty = true;
            renderSheet();
            sheetStatus('Not applied - see the terminal.', 'error');
            return;
        }
        var warnings = doc.warnings || [];
        if (warnings.length) {
            sheetStatus('Applied, but the parser skipped: ' + warnings.join('; '), 'warning');
        } else {
            sheetStatus('Applied to the window.', 'ok');
        }
    }

    function nodeTitle(node) {
        var what = node.kind === 'text' ? '#text'
            : node.kind === 'component' ? node.library + ':' + node.tag : '<' + node.tag + '>';
        return what + '  ·  uid ' + node.uid;
    }

    /** The focused field of the panel, to put the caret back after a re-render. */
    function panelFocus(box) {
        var ae = document.activeElement;
        if (!ae || !box.contains(ae)) return null;
        var row = ae.closest ? ae.closest('[data-prop]') : null;
        if (!row) return null;
        var f = { name: row.dataset.prop, start: null, end: null };
        try { f.start = ae.selectionStart; f.end = ae.selectionEnd; } catch (e) { /* checkbox */ }
        return f;
    }

    function restoreFocus(box, f) {
        if (!f) return;
        var rows = box.querySelectorAll('[data-prop]');
        for (var i = 0; i < rows.length; i++) {
            if (rows[i].dataset.prop !== f.name) continue;
            var input = rows[i].querySelector('input, textarea, select');
            if (!input) return;
            input.focus();
            try { if (f.start != null) input.setSelectionRange(f.start, f.end); } catch (e) { /* not text */ }
            return;
        }
    }

    function renderProps() {
        var box = document.getElementById('azb-props');
        var what = document.getElementById('azb-props-what');
        if (!box) return;
        var node = S.doc && S.selected != null ? findNode(S.doc.root, S.selected) : null;
        var focus = panelFocus(box);
        box.innerHTML = '';
        if (what) what.textContent = node ? nodeTitle(node) : '';
        if (!node) {
            var hint = document.createElement('div');
            hint.className = 'azb-hint';
            hint.textContent = 'Select a node in the Document tree to edit its text, id and classes '
                + '- and, for a component instance, its arguments.';
            box.appendChild(hint);
            return;
        }
        var def = node.kind === 'component' ? componentDef(S.registry, node.library, node.tag) : null;
        var group = null;
        propertyRows(node, def).forEach(function (row) {
            if (row.group !== group) {
                group = row.group;
                var h = document.createElement('div');
                h.className = 'azb-props-group';
                h.textContent = GROUP_TITLES[group] || group;
                box.appendChild(h);
            }
            box.appendChild(propertyEditor(node.uid, row));
        });
        if (node.kind === 'component' && !def) {
            var note = document.createElement('div');
            note.className = 'azb-hint';
            note.textContent = 'The component ' + node.library + ':' + node.tag
                + ' is not registered: its arguments are unknown.';
            box.appendChild(note);
        }
        restoreFocus(box, focus);
    }

    /**
     * One row: the Components view's field editor (`app.widgets.FieldEditor`:
     * label, type badge, input). An edit is committed on `change` (Enter,
     * leaving the field, a checkbox click) as ONE builder_set_attribute - one
     * undo step - not on every keystroke.
     */
    function propertyEditor(uid, row) {
        var W = app.widgets;
        var ft = W && W._parseFieldType ? W._parseFieldType(row.fieldType) : { type: 'String' };
        var editable = editableType(ft.type);
        var el;
        if (!editable || !W || !W.FieldEditor) {
            // Not one attribute (a callback, a slot, a list…): shown, set in code.
            el = document.createElement('div');
            el.className = 'azd-field-row azb-prop-readonly';
            var label = document.createElement('label');
            label.className = 'azd-field-label';
            label.textContent = row.name;
            el.appendChild(label);
            if (W && W.TypeBadge) el.appendChild(W.TypeBadge.render({ fieldType: ft }));
            var ro = document.createElement('span');
            ro.className = 'azb-prop-note';
            ro.textContent = 'set in code';
            el.appendChild(ro);
            el.title = row.name + ': ' + row.fieldType + ' - an argument of this type is not one attribute';
        } else {
            // A String argument the instance does not set shows its default as
            // the placeholder; any other type shows the value in effect.
            var raw = row.value != null ? row.value : (ft.type === 'String' ? null : row.default);
            var pending = null;
            el = W.FieldEditor.render({
                name: row.name,
                fieldType: ft,
                default: row.default || '',
                description: row.description || '',
                required: !!row.required,
            }, { value: typedValue(raw, ft.type) }, {
                onChange: function (_, v) { pending = attrString(v); },
            });
            el.addEventListener('change', function () {
                if (pending === null || !S.doc) return;
                var node = findNode(S.doc.root, uid);
                var msg = node ? propertyMessage(node, row.name, pending) : null;
                pending = null;
                if (msg) send(msg);
            });
            el.addEventListener('keydown', function (e) {
                if (e.key === 'Enter' && e.target && e.target.tagName === 'INPUT') {
                    e.preventDefault();
                    e.target.blur();
                } else if (e.key === 'Escape') {
                    pending = null;
                    renderProps();
                }
            });
        }
        el.dataset.prop = row.name;
        el.classList.add('azb-prop');
        return el;
    }

    // ── slash commands ──

    function registerSchema() {
        var C = app.schema && app.schema.commands;
        if (!C) return;
        C.builder_get_document = { desc: 'Builder document (tree with node uids)', examples: ['/builder_get_document'], params: [] };
        C.builder_insert = { desc: 'Insert into the builder document', examples: ['/builder_insert parent 0 component p', '/builder_insert parent 0 component card library user index 0'],
            params: [{ name: 'parent', type: 'number', value: 0 }, { name: 'component', type: 'text', placeholder: 'p' },
                     { name: 'index', type: 'number', placeholder: '', optional: true }, { name: 'library', type: 'text', placeholder: 'builtin', optional: true }] };
        C.builder_move = { desc: 'Move a builder node', examples: ['/builder_move node 2 parent 0 index 0'],
            params: [{ name: 'node', type: 'number', value: 1 }, { name: 'parent', type: 'number', value: 0 }, { name: 'index', type: 'number', placeholder: '', optional: true }] };
        C.builder_delete = { desc: 'Delete a builder node', examples: ['/builder_delete node 1'], params: [{ name: 'node', type: 'number', value: 1 }] };
        C.builder_set_attribute = { desc: 'Set an attribute of a builder node', examples: ['/builder_set_attribute node 1 name text value Hello'],
            params: [{ name: 'node', type: 'number', value: 1 }, { name: 'name', type: 'text', placeholder: 'text' }, { name: 'value', type: 'text', placeholder: 'Hello', optional: true }] };
        C.builder_get_stylesheet = { desc: "The builder document's own stylesheet", examples: ['/builder_get_stylesheet'], params: [] };
        C.builder_set_stylesheet = { desc: "Set the builder document's own stylesheet (undoable)", examples: ['/builder_set_stylesheet css ".card { padding: 8px; }"'],
            params: [{ name: 'css', type: 'text', placeholder: '.card { padding: 8px; }' }] };
        C.builder_undo = { desc: 'Undo the last builder edit', examples: ['/builder_undo'], params: [] };
        C.builder_redo = { desc: 'Redo the last undone builder edit', examples: ['/builder_redo'], params: [] };
        C.builder_reset = { desc: 'Discard the builder document', examples: ['/builder_reset'], params: [] };
        C.builder_convert_to_component = { desc: 'Convert a builder subtree to a component', examples: ['/builder_convert_to_component node 1 library user name my-card'],
            params: [{ name: 'node', type: 'number', value: 1 }, { name: 'library', type: 'text', placeholder: 'user' }, { name: 'name', type: 'text', placeholder: 'my-card' }] };
        C.get_component_thumbnail = { desc: 'Native thumbnail of a component (PNG)', examples: ['/get_component_thumbnail library builtin name button'],
            params: [{ name: 'library', type: 'text', placeholder: 'builtin' }, { name: 'name', type: 'text', placeholder: 'button' },
                     { name: 'width', type: 'number', placeholder: '140', optional: true }, { name: 'dpi', type: 'number', placeholder: '2', optional: true }] };
    }

    // ── styles ──

    function injectStyle() {
        if (document.getElementById('azb-style')) return;
        var s = document.createElement('style');
        s.id = 'azb-style';
        s.textContent = [
            '.azb-toolbar{display:flex;align-items:center;gap:2px;padding:4px 6px;border-bottom:1px solid var(--border);flex-shrink:0}',
            '.azb-seg{display:inline-flex;border:1px solid var(--border);border-radius:4px;overflow:hidden}',
            '.azb-seg button{background:transparent;color:var(--text-muted);border:0;padding:2px 8px;font-size:11px;cursor:pointer}',
            '.azb-seg button.active{background:var(--accent);color:#fff}',
            '.azb-spacer{flex:1}',
            '.azb-icon{background:transparent;border:0;color:var(--text-main);cursor:pointer;padding:2px;border-radius:3px;display:inline-flex;align-items:center}',
            '.azb-icon:hover:not(:disabled){background:var(--bg-hover)}',
            '.azb-icon:disabled{opacity:.35;cursor:default}',
            '.azb-icon .material-icons{font-size:16px}',
            '.azb-doc-tree{position:relative}',
            '.azb-row{position:relative}',
            '.azb-row.azb-dragging{opacity:.4}',
            '.azb-row.azb-drop-before::before,.azb-row.azb-drop-after::after{content:"";position:absolute;left:var(--azb-indent,20px);right:4px;height:2px;background:var(--accent);pointer-events:none;border-radius:1px}',
            '.azb-row.azb-drop-before::before{top:-1px}',
            '.azb-row.azb-drop-after::after{bottom:-1px}',
            '.azb-row.azb-drop-into{background:rgba(0,122,204,.22);box-shadow:inset 0 0 0 1px var(--accent)}',
            '.azb-doc-tree.azb-drop-end{box-shadow:inset 0 -2px 0 var(--accent)}',
            '.azb-hint{color:var(--text-muted);font:11px/1.4 sans-serif;padding:10px 12px}',
            '.azb-palette-filter{width:100%;box-sizing:border-box;background:var(--bg-input);color:var(--text-main);border:1px solid var(--border);border-radius:3px;padding:3px 6px;font-size:11px;margin:2px 0 4px}',
            '.azb-palette-lib{font-size:10px;color:var(--text-muted);text-transform:uppercase;letter-spacing:.04em;margin:6px 0 3px}',
            '.azb-palette-grid{display:grid;grid-template-columns:repeat(auto-fill,minmax(78px,1fr));gap:6px}',
            '.azb-card{display:flex;flex-direction:column;border:1px solid var(--border);border-radius:4px;background:var(--bg-panel);cursor:grab;overflow:hidden;user-select:none}',
            '.azb-card:hover{border-color:var(--accent)}',
            '.azb-card:active{cursor:grabbing}',
            '.azb-thumb{height:48px;background:#fff;display:flex;align-items:center;justify-content:center;overflow:hidden}',
            '.azb-thumb img{max-width:100%;max-height:100%;object-fit:contain;display:block}',
            '.azb-thumb.azb-loading{background:#f3f3f3}',
            '.azb-thumb-letter{color:#9a9a9a;font:600 18px/1 sans-serif}',
            '.azb-thumb-letter.azb-thumb-tag{font:11px/1 monospace}',
            '.azb-card-label{font-size:10px;padding:2px 4px;white-space:nowrap;overflow:hidden;text-overflow:ellipsis;text-align:center;color:var(--text-main)}',
            // B5: the Inspector's builder panels (the page's tokens; rows are the Components view's field rows)
            '.azb-inspector-row{flex:1;display:flex;min-height:0;overflow:hidden}',
            '.azb-inspector-main{flex:1;display:flex;flex-direction:column;min-width:0;overflow:hidden}',
            '.azb-side{width:290px;flex-shrink:0;display:flex;flex-direction:column;overflow:hidden;background:var(--bg-sidebar);border-left:1px solid var(--border)}',
            '.azb-props-what{font-weight:400;text-transform:none;color:var(--text-muted);font-family:Consolas,Monaco,"Courier New",monospace;white-space:nowrap;overflow:hidden;text-overflow:ellipsis;margin-left:8px}',
            '.azb-props{flex:1;overflow:auto;min-height:60px}',
            '.azb-props-group{font-size:10px;text-transform:uppercase;letter-spacing:.04em;color:var(--text-muted);padding:8px 8px 3px;border-bottom:1px solid var(--border)}',
            '.azb-prop .azd-field-label{flex:0 0 84px;color:var(--attr-color)}',
            '.azb-prop .azd-input-string,.azb-prop .azd-input-int,.azb-prop .azd-input-float{background:var(--bg-input);color:var(--text-main)}',
            '.azb-prop-readonly .azd-field-label{color:var(--text-muted)}',
            '.azb-prop-note{flex:1;font-size:11px;color:var(--text-muted);font-style:italic}',
            '.azb-sheet{flex:0 0 38%;display:flex;flex-direction:column;min-height:120px;border-top:1px solid var(--border)}',
            '.azb-sheet-head .btn-sm{text-transform:none;font-weight:400}',
            '.azb-sheet-dot{color:var(--warning);font-size:9px;visibility:hidden}',
            '.azb-sheet.azb-dirty .azb-sheet-dot{visibility:visible}',
            '.azb-sheet-text{flex:1;min-height:60px;margin:0 8px;resize:none;background:var(--bg-input);color:var(--text-main);border:1px solid var(--border);border-radius:3px;padding:6px;font:12px/18px Consolas,Monaco,"Courier New",monospace;tab-size:4;white-space:pre;overflow:auto}',
            '.azb-sheet-text:focus{border-color:var(--accent)}',
            '.azb-sheet-status{font-size:11px;line-height:1.4;color:var(--text-muted);padding:5px 8px 8px}',
            '.azb-sheet-ok{color:var(--success)}',
            '.azb-sheet-warning{color:var(--warning)}',
            '.azb-sheet-error{color:var(--error)}',
        ].join('\n');
        document.head.appendChild(s);
    }

    // ── install ──

    function install() {
        injectStyle();
        injectToolbar();
        injectInspectorLayout();
        installContainerDrop();
        registerSchema();

        app.handlers.refreshSidebar = async function () {
            if (S.mode === null) await decideMode();
            updateToolbar();
            var c = document.getElementById('dom-tree-container');
            if (c) c.classList.toggle('azb-doc-tree', S.mode === 'document');
            if (S.mode === 'document') {
                await refreshDocument();
                app.handlers._loadPaletteComponents();
                return;
            }
            return origRefreshSidebar.apply(this, arguments);
        };
        // The live tree must not paint over the document view (nodeSelected
        // and the collapse toggles re-render it).
        app.ui.renderDomTree = function (hierarchy, rootIdx) {
            if (S.mode === 'document') {
                app.state.hierarchy = hierarchy;
                app.state.hierarchyRoot = rootIdx;
                return;
            }
            return origRenderDomTree.apply(this, arguments);
        };
        app.handlers._loadPaletteComponents = function () { return renderPalette(); };
        document.addEventListener('keydown', onKeyDown);
        // Every drag starts clean: a row re-rendered mid-drag never gets its
        // `dragend`, and a drag from outside this module sets no payload.
        document.addEventListener('dragstart', function () { S.drag = null; }, true);
    }

    install();
    root.azDnd = {
        logic: logic,
        state: S,
        setMode: setMode,
        refresh: function () { return app.handlers.refreshSidebar(); },
        renderPalette: renderPalette,
        send: send,
        // B4: the project viewer selects a component file's instance.
        select: select,
    };
})(typeof window !== 'undefined' ? window : globalThis);
