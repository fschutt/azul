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

    /**
     * Builtins that are the document's structure or <head> content: a builder
     * document IS a <body>, so they are not in the palette. Every other
     * builtin with nothing to show on its own (<br>, <option>, <source>, ...)
     * stays droppable; its card says "no visual" with the server's reason
     * (`thumbOf`).
     */
    var NOT_IN_PALETTE = ['html', 'head', 'title', 'body', 'meta', 'link', 'script', 'style', 'base'];
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

    /**
     * THE rule for a drop on a node's row (the tree) or on its box (the
     * window's picture): `dropZone` at `relY`, and where INTO is refused (a
     * <div> in a <p>, a row into its own subtree) the row's halves instead.
     * `{zone, msg}`, `msg` null when nothing may drop there.
     */
    function rowDrop(payload, row, relY, docRoot) {
        var zone = dropZone(relY, row.node, row.parent == null);
        var msg = dropMessage(payload, dropTarget(row, zone), docRoot);
        if (!msg && zone === 'into' && row.parent != null) {
            zone = relY < 0.5 ? 'before' : 'after';
            msg = dropMessage(payload, dropTarget(row, zone), docRoot);
        }
        return { zone: zone, msg: msg };
    }

    /** Where a row of `depth` starts (its toggle): the rows and the drop line both use it. */
    function indentPx(depth) {
        return depth * 16 + 4;
    }

    /** Where the label of a row of `depth` starts: after its 16px toggle (.tree-toggle). */
    function labelPx(depth) {
        return indentPx(depth) + 16;
    }

    /**
     * Where the drop line goes for `zone` on the row of `uid`, among the
     * visible `rows` (`flatten`): at the gap the node lands in, at the depth
     * it lands at. `{anchor, edge, depth}`: the line is on the `edge` ('top' |
     * 'bottom') of the row `anchor`. BEFORE: the row's top. AFTER: below its
     * whole visible subtree (a sibling lands after all of it). INTO: appended,
     * so below the last visible descendant, one level deeper.
     */
    function dropLine(rows, uid, zone) {
        var i = -1;
        for (var k = 0; k < rows.length; k++) if (rows[k].uid === uid) { i = k; break; }
        if (i < 0) return null;
        var row = rows[i];
        if (zone === 'before') return { anchor: row.uid, edge: 'top', depth: row.depth };
        var last = i;
        while (last + 1 < rows.length && rows[last + 1].depth > row.depth) last++;
        return { anchor: rows[last].uid, edge: 'bottom', depth: zone === 'into' ? row.depth + 1 : row.depth };
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

    /** Palette entries from `get_component_registry`, minus the builtins NOT_IN_PALETTE. */
    function paletteEntries(registry) {
        var out = [];
        ((registry && registry.libraries) || []).forEach(function (lib) {
            (lib.components || []).forEach(function (c) {
                var tag = c.tag || c.name;
                if (!tag) return;
                if (lib.name === 'builtin' && NOT_IN_PALETTE.indexOf(tag) !== -1) return;
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

    /**
     * A `get_component_thumbnail` answer as its card keeps it: the picture, or
     * none - with the reason when it is a builtin that has nothing to show on
     * its own (`no_visual`, builder.rs), which the card says instead of an
     * empty box.
     */
    function thumbOf(v) {
        if (v && v.data) return { data: v.data, width: v.width, height: v.height };
        return { empty: true, noVisual: (v && typeof v.no_visual === 'string' && v.no_visual) || null };
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

    // ── B5: the builder's markers ──

    /**
     * The live node (`get_node_hierarchy`) of document node `uid`: the one
     * the server answers with `builder_uid` (B5 keeps the marker class
     * `azb-<uid>` out of `classes`), or - from a server before that - the one
     * carrying the marker class. Null if it is not mounted.
     */
    function liveNodeOf(nodes, uid) {
        var list = nodes || [];
        var mark = 'azb-' + uid;
        for (var i = 0; i < list.length; i++) {
            var n = list[i];
            if (n && (n.builder_uid === uid || (n.classes || []).indexOf(mark) !== -1)) return n;
        }
        return null;
    }

    // ── B5: duplicate, the document file ──

    /** "Duplicate": the message for `uid`, or null (the root, an unknown node). */
    function duplicateMessage(docRoot, uid) {
        if (!docRoot || uid == null || uid === 0 || !findNode(docRoot, uid)) return null;
        return { op: 'builder_duplicate', node: uid };
    }

    /**
     * An opened `document.json` as the message that loads it. Throws with
     * the reason for text that is not JSON, not a document, or another
     * format (the server refuses those too, but the file name is known here).
     */
    function documentLoadMessage(text) {
        var v;
        try { v = JSON.parse(text); } catch (e) { throw new Error('the file is not JSON (' + e.message + ')'); }
        if (!v || typeof v !== 'object' || Array.isArray(v)) {
            throw new Error('the file is not a builder document (an object with "root")');
        }
        if (v.format != null && v.format !== 'azul-builder-document') {
            throw new Error('the file is "' + v.format + '", not an "azul-builder-document"');
        }
        return { op: 'builder_load_document', document: v };
    }

    // ── B5: drops onto the window canvas ──

    /**
     * The window point (logical px) under client (x, y) on the window's
     * picture, shown at `rect` (its client rect); `logical` = the window's
     * logical size (`get_state`). The picture's own pixel size (it is the
     * CPU render at the window's DPI) plays no part. Null without a size.
     */
    function canvasPoint(clientX, clientY, rect, logical) {
        if (!rect || !logical || !logical.width || !logical.height || !rect.width || !rect.height) return null;
        var clamp = function (v, max) { return Math.max(0, Math.min(max, v)); };
        return {
            x: clamp((clientX - rect.left) * logical.width / rect.width, logical.width),
            y: clamp((clientY - rect.top) * logical.height / rect.height, logical.height),
        };
    }

    /**
     * Where a drop at a `builder_hit_test` answer lands: the tree's own rule
     * on the node under the pointer (`dropZone` on `rel_y`); where INTO is
     * refused (a <div> in a <p>) it falls back to before / after by halves;
     * outside every document node it is the end of <body>.
     * `{uid, zone, msg}`, `msg` null when the drop is not allowed; null
     * without a payload.
     */
    function canvasDrop(payload, hit, docRoot) {
        var p = normalizePayload(payload);
        if (!p || !docRoot) return null;
        var row = hit && hit.hit && hit.uid != null ? rowOf(docRoot, hit.uid) : null;
        if (!row) {
            return { uid: 0, zone: 'into', msg: dropMessage(p, { parent: 0, index: null }, docRoot) };
        }
        var at = rowDrop(p, row, typeof hit.rel_y === 'number' ? hit.rel_y : 0.5, docRoot);
        return { uid: row.uid, zone: at.zone, msg: at.msg };
    }

    /**
     * The drop indicator over the picture (px in the picture's box, shown at
     * `shown` {width, height}): the node's box for INTO, a 2px line on its top
     * / bottom edge for BEFORE / AFTER, the whole window for <body> (no rect).
     */
    function canvasIndicator(hit, zone, shown, logical) {
        if (!shown || !logical || !logical.width || !logical.height) return null;
        if (!hit || !hit.rect) return { zone: 'into', left: 0, top: 0, width: shown.width, height: shown.height };
        var sx = shown.width / logical.width;
        var sy = shown.height / logical.height;
        var left = hit.rect.x * sx;
        var top = hit.rect.y * sy;
        var width = hit.rect.width * sx;
        var height = hit.rect.height * sy;
        if (zone === 'before') return { zone: zone, left: left, top: top - 1, width: width, height: 2 };
        if (zone === 'after') return { zone: zone, left: left, top: top + height - 1, width: width, height: 2 };
        return { zone: 'into', left: left, top: top, width: width, height: height };
    }

    var logic = {
        NOT_IN_PALETTE: NOT_IN_PALETTE, VOID: VOID, AUTO_CLOSE: AUTO_CLOSE,
        acceptsChildren: acceptsChildren, canContain: canContain, dropZone: dropZone, flatten: flatten,
        dropTarget: dropTarget, rowDrop: rowDrop, dropLine: dropLine, indentPx: indentPx, labelPx: labelPx,
        findNode: findNode, rowOf: rowOf,
        isSelfOrDescendant: isSelfOrDescendant, normalizePayload: normalizePayload,
        dropMessage: dropMessage, stepMessage: stepMessage, insertTarget: insertTarget,
        paletteEntries: paletteEntries, thumbOf: thumbOf, suggestComponentName: suggestComponentName,
        sanitizeComponentName: sanitizeComponentName,
        // B5
        componentDef: componentDef, propertyRows: propertyRows, propertyMessage: propertyMessage,
        attrString: attrString, typedValue: typedValue, editableType: editableType,
        stylesheetMessage: stylesheetMessage, sheetText: sheetText,
        canvasPoint: canvasPoint, canvasDrop: canvasDrop, canvasIndicator: canvasIndicator,
        liveNodeOf: liveNodeOf, duplicateMessage: duplicateMessage, documentLoadMessage: documentLoadMessage,
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
        rows: [],                // the tree's visible rows (flatten), as rendered
        expandTimer: null,
        thumbs: {},              // 'lib:name' -> 'pending' | thumbOf(answer)
        thumbQueue: [],
        thumbBusy: 0,
        thumbGen: 0,             // bumped when the page's mode changes: older answers are dropped
        palette: [],
        paletteFilter: '',
        observer: null,
        liveCache: null,         // last get_node_hierarchy value
        registry: null,          // last get_component_registry value (the panel's data models)
        sheetDirty: false,       // the stylesheet editor holds text not applied yet
        canvas: {                // the window's picture in the Inspector (drop target)
            open: true,          // the picture is unfolded
            logical: null,       // the window's logical size {width, height} (get_state)
            over: false,         // a drag is over the picture
            want: null,          // the latest hover point still to hit-test
            busy: false,         // a hover hit test is in flight
            drop: null,          // canvasDrop() of the last hover answer
            timer: null,         // pending refresh
        },
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
            case 'builder_duplicate': return 'Duplicate node ' + msg.node;
            case 'builder_load_document': return 'Load the document';
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
        // The window re-mounts the document: show the new picture.
        if (S.mode === 'document') scheduleCanvas();
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
        if (S.mode === 'document') scheduleCanvas();
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
        S.rows = flatten(S.doc.root, S.collapsed);
        S.rows.forEach(function (row) {
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
        el.draggable = row.parent != null;

        var indent = document.createElement('span');
        indent.className = 'tree-indent';
        indent.style.width = indentPx(row.depth) + 'px';
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
            // Onto its own label: still this row. Anywhere else: the next
            // row's dragover draws again (dragleave comes first).
            if (e.relatedTarget && el.contains(e.relatedTarget)) return;
            clearIndicators();
        });
        el.addEventListener('drop', function (e) { onRowDrop(e, row, el); });
        return el;
    }

    function clearIndicators() {
        document.querySelectorAll('.azb-drop-before, .azb-drop-after, .azb-drop-into').forEach(function (n) {
            n.classList.remove('azb-drop-before', 'azb-drop-after', 'azb-drop-into');
        });
        var c = document.getElementById('dom-tree-container');
        if (c) {
            c.classList.remove('azb-drop-end');
            c.querySelectorAll('.azb-drop-marker').forEach(function (m) { m.remove(); });
        }
    }

    /**
     * The drop line (`dropLine`) for `zone` on the row of `uid`: a 2px line
     * with a ring at its start, from where the landed node's label will start,
     * over the gap it lands in. Positioned in the tree's own box, so it scrolls with it.
     */
    function showDropLine(uid, zone) {
        var c = document.getElementById('dom-tree-container');
        if (!c) return;
        var line = dropLine(S.rows || [], uid, zone);
        var anchor = line ? c.querySelector('.azb-row[data-uid="' + line.anchor + '"]') : null;
        var m = c.querySelector('.azb-drop-marker');
        if (!anchor) {
            if (m) m.remove();
            return;
        }
        if (!m) {
            m = document.createElement('div');
            m.className = 'azb-drop-marker';
            m.setAttribute('aria-hidden', 'true');
            c.appendChild(m);
        }
        m.dataset.zone = zone;
        m.style.left = labelPx(line.depth) + 'px';
        m.style.top = (anchor.offsetTop + (line.edge === 'bottom' ? anchor.offsetHeight : 0) - 1) + 'px';
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
        var at = rowDrop(payload, row, (e.clientY - rect.top) / Math.max(rect.height, 1), S.doc.root);
        var zone = at.zone;
        clearIndicators();
        if (!at.msg) {
            e.dataTransfer.dropEffect = 'none';
            S.dropAt = null;
            return;
        }
        e.preventDefault();
        e.stopPropagation();
        e.dataTransfer.dropEffect = payload.type === 'builder-node' ? 'move' : 'copy';
        el.classList.add('azb-drop-' + zone);
        showDropLine(row.uid, zone);
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
        var payload = readPayload(e);
        // The zone the indicator showed, else the rule at the drop point.
        var zone = S.dropAt && S.dropAt.uid === row.uid
            ? S.dropAt.zone
            : rowDrop(payload, row, (e.clientY - rect.top) / Math.max(rect.height, 1), S.doc && S.doc.root).zone;
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
            showDropLine(0, 'into');
            e.dataTransfer.dropEffect = payload.type === 'builder-node' ? 'move' : 'copy';
        });
        c.addEventListener('dragleave', function (e) {
            if (e.target === c) clearIndicators();
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
     * Every mounted document element carries the marker `azb-<uid>` (answered
     * as `builder_uid`); find the live node for the detail panel (CSS, layout,
     * screenshot) without re-rendering the tree.
     */
    async function showLiveDetail(uid) {
        try {
            var h = S.liveCache || (S.liveCache = await call({ op: 'get_node_hierarchy' }));
            var nodes = (h && h.nodes) || [];
            var live = liveNodeOf(nodes, uid);
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

    /** Copy a node and its subtree right after it; the copy gets selected. */
    function duplicateNode(uid) {
        var msg = S.doc ? duplicateMessage(S.doc.root, uid) : null;
        return msg ? send(msg) : null;
    }

    // ── the document as a file (Export / Import menus) ──

    function menuEntry(act, icon, label) {
        var item = document.createElement('div');
        item.className = 'menu-dropdown-item';
        item.dataset.azbMenu = act;
        var i = document.createElement('span');
        i.className = 'material-icons mi';
        i.textContent = icon;
        item.appendChild(i);
        item.appendChild(document.createTextNode(label));
        return item;
    }

    function injectDocumentMenus() {
        var exp = document.querySelector('.menu-item[data-menu="export"] > .menu-dropdown');
        var imp = document.querySelector('.menu-item[data-menu="import"] > .menu-dropdown');
        if (!exp || !imp || document.getElementById('azb-document-input')) return;
        var sep = function () {
            var d = document.createElement('div');
            d.className = 'menu-dropdown-separator';
            return d;
        };
        var save = menuEntry('save-document', 'description', 'Builder document (JSON)');
        save.addEventListener('click', saveDocumentFile);
        exp.appendChild(sep());
        exp.appendChild(save);

        var input = document.createElement('input');
        input.type = 'file';
        input.id = 'azb-document-input';
        input.className = 'hidden';
        input.accept = '.json,application/json';
        input.addEventListener('change', function () { loadDocumentFile(input); });
        document.body.appendChild(input);
        var load = menuEntry('load-document', 'description', 'Builder document (JSON)\u2026');
        load.addEventListener('click', function () { input.value = ''; input.click(); });
        imp.appendChild(sep());
        imp.appendChild(load);
    }

    /** Export > Builder document (JSON): `document.json`, the file project_save writes. */
    async function saveDocumentFile() {
        try {
            var file = await call({ op: 'builder_save_document' });
            if (typeof root._downloadJSON !== 'function') throw new Error('no download helper on this page');
            root._downloadJSON(file, 'document.json');
            app.log('Builder document saved as document.json', 'info');
        } catch (err) {
            app.log('Save the builder document: ' + err.message, 'error');
        }
    }

    /** Import > Builder document (JSON)…: replaces the document as one edit (Ctrl/Cmd+Z undoes it). */
    function loadDocumentFile(input) {
        var f = input.files && input.files[0];
        if (!f) return;
        var reader = new FileReader();
        reader.onload = function () {
            var msg;
            try {
                msg = documentLoadMessage(String(reader.result || ''));
            } catch (err) {
                app.log('Open ' + f.name + ': ' + err.message, 'error');
                return;
            }
            if (S.mode !== 'document') setMode('document');
            S.sheetDirty = false;
            send(msg, 'Load ' + f.name);
        };
        reader.onerror = function () { app.log('Open ' + f.name + ': cannot read the file', 'error'); };
        reader.readAsText(f);
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
            items.push({ icon: 'content_copy', label: 'Duplicate', action: function () { duplicateNode(node.uid); } });
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
        } else if (mod && (key === 'd' || key === 'D') && S.selected != null && S.selected !== 0) {
            e.preventDefault();
            duplicateNode(S.selected);
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
        var done = t && t !== 'pending' ? t : null;
        thumb.innerHTML = '';
        thumb.removeAttribute('title');
        thumb.classList.toggle('azb-loading', !done);
        thumb.classList.toggle('azb-novisual', !!(done && done.noVisual));
        if (done && done.data) {
            var img = document.createElement('img');
            img.alt = entry.label;
            img.draggable = false;
            img.src = done.data;
            thumb.appendChild(img);
            return;
        }
        if (done && done.noVisual) {
            // Nothing to show on its own (<br>, <option>, <source>, ...): say
            // so rather than draw an empty box; the reason is the tooltip.
            var nv = document.createElement('span');
            nv.className = 'azb-thumb-novisual';
            nv.textContent = 'no visual';
            thumb.appendChild(nv);
            thumb.title = 'No visual: ' + done.noVisual;
            return;
        }
        var letter = document.createElement('span');
        letter.className = 'azb-thumb-letter';
        letter.textContent = done ? '<' + entry.component + '>' : (entry.label || '?').charAt(0).toUpperCase();
        if (done) letter.classList.add('azb-thumb-tag');
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

    /** Whether the page shows its dark palette (`app.mode`, debugger.js). */
    function pageIsDark() {
        return !!(app.mode && app.mode.isDark());
    }

    function loadThumb(entry) {
        var k = entryKey(entry);
        var gen = S.thumbGen;
        S.thumbBusy++;
        // In the page's mode: a dark page shows the components as they look
        // in dark mode, on a dark background (builder.rs `thumbnail`).
        call({ op: 'get_component_thumbnail', library: entry.library, name: entry.component,
               width: THUMB_WIDTH, dpi: THUMB_DPI, dark: pageIsDark() })
            .then(function (t) { if (gen === S.thumbGen) S.thumbs[k] = thumbOf(t); })
            .catch(function () { if (gen === S.thumbGen) S.thumbs[k] = thumbOf(null); })
            .then(function () {
                S.thumbBusy--;
                if (gen === S.thumbGen) {
                    document.querySelectorAll('.azb-card').forEach(function (card) {
                        if (card.dataset.key === k) fillThumb(card.querySelector('.azb-thumb'), entry);
                    });
                }
                pumpThumbs();
            });
    }

    /**
     * The page switched between light and dark: every card asks for its
     * picture again, in the new mode. An answer still on its way for the old
     * mode is dropped (`thumbGen`).
     */
    function repaintThumbs() {
        S.thumbGen++;
        S.thumbs = {};
        S.thumbQueue = [];
        var container = document.getElementById('palette-component-list');
        if (container && S.palette.length) drawPalette(container);
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
        main.appendChild(buildCanvas());
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
        var pane = document.getElementById('azb-canvas');
        if (pane) {
            var appearing = docMode && pane.classList.contains('hidden');
            pane.classList.toggle('hidden', !docMode);
            if (appearing) scheduleCanvas(0);
        }
        if (docMode) {
            renderProps();
            renderSheet();
        }
    }

    // ── the window canvas ──
    //
    // A browser drag cannot land in the native window itself: the shells
    // register it for FILE drops only. So the Inspector shows the window as a
    // picture - its own CPU rendering (`take_screenshot`), refreshed after
    // every edit - and a drop or a click on the picture is a point in the
    // window: `builder_hit_test` maps it to the document node under it
    // through its `azb-<uid>` marker, and the tree's own rule places the drop.

    var CANVAS_KEY = 'azul_builder_canvas_open';

    function buildCanvas() {
        try { S.canvas.open = localStorage.getItem(CANVAS_KEY) !== '0'; } catch (e) { /* private mode */ }
        var pane = document.createElement('div');
        pane.id = 'azb-canvas';
        pane.className = 'azb-canvas hidden';
        var bar = document.createElement('div');
        bar.className = 'azb-toolbar azb-canvas-bar';
        var title = document.createElement('span');
        title.className = 'azb-canvas-title';
        title.textContent = 'Window';
        var info = document.createElement('span');
        info.id = 'azb-canvas-info';
        info.className = 'azb-canvas-info';
        info.textContent = 'Drop components on the window; click it to select.';
        var spacer = document.createElement('span');
        spacer.className = 'azb-spacer';
        var refresh = button('canvas-refresh', 'refresh', 'Refresh the picture of the window');
        refresh.addEventListener('click', function () { refreshCanvas(); });
        var fold = button('canvas-fold', S.canvas.open ? 'expand_less' : 'expand_more', 'Show / hide the window');
        fold.addEventListener('click', function () {
            S.canvas.open = !S.canvas.open;
            try { localStorage.setItem(CANVAS_KEY, S.canvas.open ? '1' : '0'); } catch (e) { /* ignore */ }
            fold.querySelector('.material-icons').textContent = S.canvas.open ? 'expand_less' : 'expand_more';
            stage.classList.toggle('hidden', !S.canvas.open);
            if (S.canvas.open) refreshCanvas();
        });
        [title, info, spacer, refresh, fold].forEach(function (n) { bar.appendChild(n); });

        var stage = document.createElement('div');
        stage.id = 'azb-canvas-stage';
        stage.className = 'azb-canvas-stage' + (S.canvas.open ? '' : ' hidden');
        var frame = document.createElement('div');
        frame.className = 'azb-canvas-frame';
        var img = document.createElement('img');
        img.id = 'azb-canvas-img';
        img.alt = '';
        img.draggable = false;
        var mark = document.createElement('div');
        mark.id = 'azb-canvas-mark';
        mark.className = 'azb-canvas-mark hidden';
        frame.appendChild(img);
        frame.appendChild(mark);
        stage.appendChild(frame);
        img.addEventListener('dragover', onCanvasDragOver);
        img.addEventListener('dragleave', function () {
            S.canvas.over = false;
            drawCanvasMark(null, null);
        });
        img.addEventListener('drop', onCanvasDrop);
        img.addEventListener('click', onCanvasClick);
        pane.appendChild(bar);
        pane.appendChild(stage);
        return pane;
    }

    function scheduleCanvas(ms) {
        if (S.canvas.timer) clearTimeout(S.canvas.timer);
        // After an edit the window re-mounts on its next frame: wait for it.
        S.canvas.timer = setTimeout(function () {
            S.canvas.timer = null;
            refreshCanvas();
        }, ms == null ? 250 : ms);
    }

    async function refreshCanvas() {
        var img = document.getElementById('azb-canvas-img');
        var info = document.getElementById('azb-canvas-info');
        if (!img || S.mode !== 'document' || !S.canvas.open) return;
        try {
            var st = await app.api.post({ op: 'get_state' });
            var ws = (st && (st.window_state || (st.data && (st.data.value || st.data)))) || {};
            if (ws.logical_width > 0 && ws.logical_height > 0) {
                S.canvas.logical = { width: ws.logical_width, height: ws.logical_height };
            }
            var shot = await call({ op: 'take_screenshot' });
            var data = shot && (typeof shot === 'string' ? shot : shot.data);
            if (data) img.src = data;
            if (info && S.canvas.logical) {
                info.textContent = Math.round(S.canvas.logical.width) + ' × '
                    + Math.round(S.canvas.logical.height) + ' - drop components here, click to select';
            }
        } catch (err) {
            if (info) info.textContent = 'No picture of the window: ' + err.message;
        }
    }

    function round1(v) { return Math.round(v * 10) / 10; }

    function canvasPointOf(e) {
        var img = document.getElementById('azb-canvas-img');
        return img ? canvasPoint(e.clientX, e.clientY, img.getBoundingClientRect(), S.canvas.logical) : null;
    }

    function hitTest(pt) {
        return call({ op: 'builder_hit_test', x: round1(pt.x), y: round1(pt.y) });
    }

    function drawCanvasMark(zone, hit) {
        var mark = document.getElementById('azb-canvas-mark');
        var img = document.getElementById('azb-canvas-img');
        if (!mark || !img) return;
        var r = zone ? canvasIndicator(hit, zone, { width: img.clientWidth, height: img.clientHeight },
            S.canvas.logical) : null;
        if (!r) {
            mark.className = 'azb-canvas-mark hidden';
            mark.dataset.zone = '';
            return;
        }
        mark.className = 'azb-canvas-mark azb-canvas-' + r.zone;
        mark.dataset.zone = r.zone;
        mark.style.left = r.left + 'px';
        mark.style.top = r.top + 'px';
        mark.style.width = r.width + 'px';
        mark.style.height = r.height + 'px';
    }

    /** Hit-test the latest hover point; one request in flight, the newest point wins. */
    function probeCanvas(pt, payload) {
        S.canvas.want = { pt: pt, payload: payload };
        if (S.canvas.busy) return;
        S.canvas.busy = true;
        (async function () {
            while (S.canvas.want) {
                var w = S.canvas.want;
                S.canvas.want = null;
                var hit;
                try { hit = await hitTest(w.pt); } catch (e) { break; }
                if (!S.canvas.over) break;
                S.canvas.drop = canvasDrop(w.payload, hit, S.doc && S.doc.root);
                drawCanvasMark(S.canvas.drop && S.canvas.drop.msg ? S.canvas.drop.zone : null, hit);
            }
            S.canvas.busy = false;
        })();
    }

    function onCanvasDragOver(e) {
        if (S.mode !== 'document' || !S.doc) return;
        var payload = currentPayload(e);
        var pt = payload ? canvasPointOf(e) : null;
        if (!pt) return;
        e.preventDefault();
        var fresh = !S.canvas.over;
        S.canvas.over = true;
        var last = S.canvas.lastPt;
        if (fresh || !last || Math.abs(last.x - pt.x) >= 2 || Math.abs(last.y - pt.y) >= 2) {
            S.canvas.lastPt = pt;
            probeCanvas(pt, payload);
        }
        var ok = !S.canvas.drop || !!S.canvas.drop.msg;
        e.dataTransfer.dropEffect = !ok ? 'none' : payload.type === 'builder-node' ? 'move' : 'copy';
    }

    async function onCanvasDrop(e) {
        if (S.mode !== 'document' || !S.doc) return;
        e.preventDefault();
        var payload = readPayload(e);
        var pt = canvasPointOf(e);
        S.canvas.over = false;
        S.canvas.want = null;
        S.canvas.drop = null;
        S.canvas.lastPt = null;
        drawCanvasMark(null, null);
        endDrag();
        if (!payload || !pt) return;
        var hit;
        try {
            hit = await hitTest(pt);
        } catch (err) {
            app.log('Drop on the window: ' + err.message, 'error');
            return;
        }
        var drop = canvasDrop(payload, hit, S.doc && S.doc.root);
        if (!drop || !drop.msg) {
            app.log('Cannot drop there: the node under the pointer cannot take it', 'warning');
            return;
        }
        await send(drop.msg);
    }

    async function onCanvasClick(e) {
        if (S.mode !== 'document' || !S.doc) return;
        var pt = canvasPointOf(e);
        if (!pt) return;
        try {
            var hit = await hitTest(pt);
            if (hit && hit.hit && S.doc && findNode(S.doc.root, hit.uid)) select(hit.uid);
        } catch (err) {
            app.log('Select on the window: ' + err.message, 'error');
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
        C.builder_hit_test = { desc: 'The builder document node at a window point', examples: ['/builder_hit_test x 100 y 40'],
            params: [{ name: 'x', type: 'number', value: 0 }, { name: 'y', type: 'number', value: 0 }] };
        C.builder_duplicate = { desc: 'Duplicate a builder node and its subtree (undoable)', examples: ['/builder_duplicate node 1'],
            params: [{ name: 'node', type: 'number', value: 1 }] };
        C.builder_save_document = { desc: 'The builder document as a file (document.json)', examples: ['/builder_save_document'], params: [] };
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
            '.azb-seg button.active{background:var(--accent);color:var(--on-accent)}',
            '.azb-spacer{flex:1}',
            '.azb-icon{background:transparent;border:0;color:var(--text-main);cursor:pointer;padding:2px;border-radius:3px;display:inline-flex;align-items:center}',
            '.azb-icon:hover:not(:disabled){background:var(--bg-hover)}',
            '.azb-icon:disabled{opacity:.35;cursor:default}',
            '.azb-icon .material-icons{font-size:16px}',
            '.azb-doc-tree{position:relative}',
            '.azb-row{position:relative}',
            '.azb-row.azb-dragging{opacity:.4}',
            // Drop indicators: INTO tints and outlines the whole row; every
            // zone draws the line where the node lands (showDropLine).
            '.azb-row.azb-drop-into{background:rgba(0,122,204,.32);background:color-mix(in srgb,var(--accent) 32%,transparent);box-shadow:inset 0 0 0 1px var(--accent)}',
            '.azb-drop-marker{position:absolute;right:4px;height:2px;background:var(--accent);border-radius:1px;pointer-events:none;z-index:2}',
            '.azb-drop-marker::before{content:"";position:absolute;left:-4px;top:-3px;width:8px;height:8px;box-sizing:border-box;border:2px solid var(--accent);border-radius:50%;background:var(--bg-sidebar)}',
            '.azb-hint{color:var(--text-muted);font:11px/1.4 sans-serif;padding:10px 12px}',
            '.azb-palette-filter{width:100%;box-sizing:border-box;background:var(--bg-input);color:var(--text-main);border:1px solid var(--border);border-radius:3px;padding:3px 6px;font-size:11px;margin:2px 0 4px}',
            '.azb-palette-lib{font-size:10px;color:var(--text-muted);text-transform:uppercase;letter-spacing:.04em;margin:6px 0 3px}',
            '.azb-palette-grid{display:grid;grid-template-columns:repeat(auto-fill,minmax(78px,1fr));gap:6px}',
            '.azb-card{display:flex;flex-direction:column;border:1px solid var(--border);border-radius:4px;background:var(--bg-panel);cursor:grab;overflow:hidden;user-select:none}',
            '.azb-card:hover{border-color:var(--accent)}',
            '.azb-card:active{cursor:grabbing}',
            '.azb-thumb{height:48px;background:var(--thumb-bg);display:flex;align-items:center;justify-content:center;overflow:hidden}',
            '.azb-thumb img{max-width:100%;max-height:100%;object-fit:contain;display:block}',
            '.azb-thumb.azb-loading{background:var(--thumb-loading)}',
            '.azb-thumb-letter{color:var(--text-muted);font:600 18px/1 sans-serif}',
            '.azb-thumb-letter.azb-thumb-tag{font:11px/1 monospace}',
            '.azb-thumb.azb-novisual{background:var(--bg-panel)}',
            '.azb-thumb-novisual{font:10px/1.3 sans-serif;color:var(--text-muted);border:1px dashed var(--border);border-radius:3px;padding:1px 6px;white-space:nowrap}',
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
            '.azb-canvas{flex:0 0 auto;display:flex;flex-direction:column;background:var(--bg-sidebar);border-bottom:1px solid var(--border)}',
            '.azb-canvas-title{font-size:11px;font-weight:bold;text-transform:uppercase;padding:0 6px 0 4px}',
            '.azb-canvas-info{font-size:11px;color:var(--text-muted);white-space:nowrap;overflow:hidden;text-overflow:ellipsis;min-width:0}',
            '.azb-canvas-stage{padding:10px;overflow:auto;max-height:42vh;text-align:center;background:repeating-conic-gradient(var(--checker-a) 0 25%,var(--checker-b) 0 50%) 0 0/16px 16px}',
            '.azb-canvas-frame{position:relative;display:inline-block;max-width:100%;line-height:0;box-shadow:0 1px 6px rgba(0,0,0,.5)}',
            '.azb-canvas-frame img{display:block;max-width:100%;max-height:calc(42vh - 20px);width:auto;height:auto;cursor:crosshair}',
            '.azb-canvas-mark{position:absolute;pointer-events:none;box-sizing:border-box}',
            '.azb-canvas-mark.azb-canvas-into{background:rgba(0,122,204,.18);border:1px solid var(--accent)}',
            '.azb-canvas-mark.azb-canvas-before,.azb-canvas-mark.azb-canvas-after{background:var(--accent);border-radius:1px}',
        ].join('\n');
        document.head.appendChild(s);
    }

    // ── install ──

    function install() {
        injectStyle();
        injectToolbar();
        injectInspectorLayout();
        injectDocumentMenus();
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
        if (app.mode && app.mode.onChange) app.mode.onChange(repaintThumbs);
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
