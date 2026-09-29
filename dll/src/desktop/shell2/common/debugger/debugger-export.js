/**
 * AzBuilder quick exports — loaded after debugger.js and debugger-dnd.js.
 *
 * Three small transient dialogs (Escape closes, focus returns to what opened
 * them), text in / text out — no zip:
 *
 *   - Compile CSS to…    pick a stylesheet (the selected node's style, the
 *                        document's, a component's, or pasted text), keep
 *                        some of its rules, pick one of the server's CSS code
 *                        generators; the code shows in a copyable,
 *                        downloadable panel.
 *   - Subtree → code     the selected document subtree as a render function
 *                        (or a runnable app) in Rust / C / C++ / Python.
 *   - Component → code   a component as code: its render function (a
 *                        converted component's texts are its parameters),
 *                        and its registration.
 *
 * Opened from the Export menu, from a toolbar button in the Document tree
 * and from the tree's context menu. Also replaces Export > Code's handler:
 * the page expected a binary zip, the server answers a data URI (the zip
 * never downloaded).
 *
 * Server messages: get_codegen_languages, get_css_rules, compile_css,
 * export_subtree_code, export_component_code, get_component_registry,
 * export_code_zip (layout/src/e2e/export.rs).
 *
 * The pure logic at the top has no DOM dependency and is unit-tested under
 * node:
 *     node dll/src/desktop/shell2/common/debugger/debugger-export.test.js
 */
(function (root) {
    'use strict';

    // =====================================================================
    // Pure logic
    // =====================================================================

    /** The DOM languages, if the server cannot be asked. */
    var DOM_FALLBACK = [
        { id: 'rust', label: 'Rust', ext: 'rs' },
        { id: 'c', label: 'C', ext: 'c' },
        { id: 'cpp', label: 'C++', ext: 'cpp' },
        { id: 'python', label: 'Python', ext: 'py' },
    ];

    /** A server language list, de-duplicated and well-formed; `fallback` if it is empty. */
    function languageOptions(list, fallback) {
        var out = [];
        (Array.isArray(list) ? list : []).forEach(function (l) {
            if (!l || typeof l.id !== 'string' || !l.id) return;
            if (out.some(function (o) { return o.id === l.id; })) return;
            out.push({ id: l.id, label: l.label || l.id, ext: l.ext || 'txt' });
        });
        return out.length ? out : (fallback || []).slice();
    }

    /** The remembered language if the server still has it, else the first. */
    function pickLanguage(options, remembered) {
        if (remembered && options.some(function (o) { return o.id === remembered; })) return remembered;
        return options.length ? options[0].id : null;
    }

    /** The get_css_rules / compile_css fields of a stylesheet source. */
    function cssSourceFields(src) {
        switch (src && src.kind) {
            case 'document': return { source: 'document' };
            case 'node': return { source: 'node', node: src.node };
            case 'component': return { source: 'component', library: src.library, name: src.name };
            default: return { source: 'text', css: (src && src.css) || '' };
        }
    }

    function rulesMessage(src) {
        var msg = { op: 'get_css_rules' };
        var f = cssSourceFields(src);
        Object.keys(f).forEach(function (k) { msg[k] = f[k]; });
        return msg;
    }

    /**
     * compile_css for what the dialog SHOWS: the CSS text on screen (the user
     * may have edited what the source resolved to) and the rules left ticked.
     * `selected`: rule indices. All of them → no `rules` (the whole sheet,
     * @keyframes included). None → null: nothing to send.
     */
    function compileCssMessage(language, css, selected, total) {
        if (!selected || !selected.length) return null;
        var msg = { op: 'compile_css', language: language, source: 'text', css: css };
        if (selected.length !== total) {
            msg.rules = selected.slice().sort(function (a, b) { return a - b; });
        }
        return msg;
    }

    function subtreeMessage(uid, language, mode, fnName) {
        var msg = { op: 'export_subtree_code', node: uid, language: language };
        if (mode === 'app') msg.mode = 'app';
        if (fnName && fnName.trim()) msg.function_name = fnName.trim();
        return msg;
    }

    function componentMessage(choice, language) {
        return { op: 'export_component_code', library: choice.library, name: choice.name, language: language };
    }

    /** Every component the registry lists, user libraries first: [{library, name, label}]. */
    function componentChoices(registry) {
        var user = [];
        var builtin = [];
        ((registry && registry.libraries) || []).forEach(function (lib) {
            (lib.components || []).forEach(function (c) {
                if (!c || !c.tag) return;
                var label = lib.name + ':' + c.tag;
                if (c.display_name && c.display_name !== c.tag) label += ' — ' + c.display_name;
                (lib.name === 'builtin' ? builtin : user).push({ library: lib.name, name: c.tag, label: label });
            });
        });
        return user.concat(builtin);
    }

    /**
     * What "Component → code" opens on: the selected document instance, else
     * the component open in the Components view, else the first user one.
     */
    function defaultComponentChoice(choices, docNode, viewLibrary, viewTag) {
        function find(lib, name) {
            for (var i = 0; i < choices.length; i++) {
                if (choices[i].library === lib && choices[i].name === name) return choices[i];
            }
            return null;
        }
        if (docNode && docNode.kind === 'component') {
            var a = find(docNode.library, docNode.tag);
            if (a) return a;
        }
        if (viewLibrary && viewTag) {
            var b = find(viewLibrary, viewTag);
            if (b) return b;
        }
        return choices[0] || null;
    }

    function truncate(s, n) {
        s = String(s || '');
        n = n || 24;
        return s.length > n ? s.slice(0, n - 1) + '…' : s;
    }

    /** A document node as the dialog names it: `div#main.card  #3`. */
    function nodeLabel(node) {
        if (!node || node.uid === 0) return 'the whole document';
        if (node.kind === 'text') return 'text "' + truncate(node.text) + '"  #' + node.uid;
        var s = node.kind === 'component' ? node.library + ':' + node.tag : node.tag;
        var a = node.attrs || {};
        if (a.id) s += '#' + String(a.id).trim().split(/\s+/)[0];
        if (a['class']) s += '.' + String(a['class']).trim().split(/\s+/).join('.');
        return s + '  #' + node.uid;
    }

    /** Focus trap: the index Tab (Shift+Tab: backwards) moves to among `count`. */
    function trapIndex(count, current, backwards) {
        if (count <= 0) return -1;
        if (current < 0) return backwards ? count - 1 : 0;
        return backwards ? (current - 1 + count) % count : (current + 1) % count;
    }

    /** An op answer's `value`, or throw its error message. */
    function unwrap(res, op) {
        if (!res || res.status !== 'ok') {
            throw new Error((res && res.message) || ('"' + op + '" failed'));
        }
        if (res.data && res.data.value !== undefined) return res.data.value;
        return res.data || null;
    }

    /** The rule list rows: selector, declaration count, the declarations as a tooltip. */
    function ruleRows(rules) {
        return (rules || []).map(function (r) {
            var decls = String(r.declarations || '').split(';').filter(function (d) { return d.trim(); });
            return {
                index: r.index,
                selector: r.selector || '(no selector)',
                title: r.declarations || '',
                count: decls.length,
                conditional: !!r.conditional,
            };
        });
    }

    var logic = {
        DOM_FALLBACK: DOM_FALLBACK,
        languageOptions: languageOptions,
        pickLanguage: pickLanguage,
        cssSourceFields: cssSourceFields,
        rulesMessage: rulesMessage,
        compileCssMessage: compileCssMessage,
        subtreeMessage: subtreeMessage,
        componentMessage: componentMessage,
        componentChoices: componentChoices,
        defaultComponentChoice: defaultComponentChoice,
        nodeLabel: nodeLabel,
        trapIndex: trapIndex,
        unwrap: unwrap,
        ruleRows: ruleRows,
    };

    if (typeof module !== 'undefined' && module.exports) module.exports = logic;
    // Under node (the unit test) there is no page and no `app`: logic only.
    if (typeof document === 'undefined' || typeof app === 'undefined') return;

    // =====================================================================
    // Browser UI
    // =====================================================================

    var LANG_KEY = { dom: 'azul_builder_export_lang_dom', css: 'azul_builder_export_lang_css' };

    var S = {
        languages: null,   // {dom: [...], css: [...]} once the server answered
        open: null,        // the dialog on screen
        seq: 0,
    };

    async function call(msg) {
        return unwrap(await app.api.post(msg), msg.op);
    }

    function remembered(kind) {
        try { return localStorage.getItem(LANG_KEY[kind]); } catch (e) { return null; }
    }

    function remember(kind, id) {
        try { localStorage.setItem(LANG_KEY[kind], id); } catch (e) { /* private mode */ }
    }

    async function languages() {
        if (S.languages) return S.languages;
        try {
            var v = await call({ op: 'get_codegen_languages' });
            S.languages = {
                dom: languageOptions(v && v.dom, DOM_FALLBACK),
                css: languageOptions(v && v.css, []),
            };
            return S.languages;
        } catch (e) {
            app.log('Export: the server did not list its languages (' + e.message + ')', 'warning');
            return { dom: DOM_FALLBACK.slice(), css: [] };
        }
    }

    /** The builder document's selected node (debugger-dnd.js), if any. */
    function selectedDocNode() {
        var d = root.azDnd;
        if (!d || !d.state || !d.state.doc || d.state.selected == null) return null;
        return d.logic.findNode(d.state.doc.root, d.state.selected);
    }

    function docNode(uid) {
        var d = root.azDnd;
        if (!d || !d.state || !d.state.doc) return null;
        return d.logic.findNode(d.state.doc.root, uid);
    }

    // ── small DOM helpers ──

    function el(tag, cls, text) {
        var n = document.createElement(tag);
        if (cls) n.className = cls;
        if (text != null) n.textContent = text;
        return n;
    }

    function textButton(label, icon, cls) {
        var b = el('button', 'azx-btn' + (cls ? ' ' + cls : ''));
        b.type = 'button';
        if (icon) {
            var i = el('span', 'material-icons', icon);
            i.setAttribute('aria-hidden', 'true');
            b.appendChild(i);
        }
        b.appendChild(el('span', null, label));
        return b;
    }

    function field(labelText, control) {
        var wrap = el('label', 'azx-field');
        wrap.appendChild(el('span', 'azx-field-label', labelText));
        wrap.appendChild(control);
        return wrap;
    }

    function selectOf(options, value) {
        var s = el('select', 'azx-select');
        options.forEach(function (o) {
            var opt = el('option', null, o.label);
            opt.value = o.id;
            if (o.disabled) opt.disabled = true;
            s.appendChild(opt);
        });
        if (value != null) s.value = value;
        return s;
    }

    function debounce(fn, ms) {
        var t = null;
        return function () {
            var args = arguments;
            clearTimeout(t);
            t = setTimeout(function () { fn.apply(null, args); }, ms);
        };
    }

    function download(fileName, text) {
        var blob = new Blob([text], { type: 'text/plain;charset=utf-8' });
        var url = URL.createObjectURL(blob);
        downloadUrl(fileName, url);
        setTimeout(function () { URL.revokeObjectURL(url); }, 1000);
    }

    function downloadUrl(fileName, url) {
        var a = document.createElement('a');
        a.href = url;
        a.download = fileName;
        a.style.display = 'none';
        document.body.appendChild(a);
        a.click();
        a.remove();
    }

    function copyText(text) {
        if (navigator.clipboard && navigator.clipboard.writeText) {
            return navigator.clipboard.writeText(text).then(function () { return true; }, function () {
                return legacyCopy(text);
            });
        }
        return Promise.resolve(legacyCopy(text));
    }

    function legacyCopy(text) {
        var ta = el('textarea');
        ta.value = text;
        ta.setAttribute('readonly', '');
        ta.style.position = 'fixed';
        ta.style.opacity = '0';
        document.body.appendChild(ta);
        ta.select();
        var ok = false;
        try { ok = document.execCommand('copy'); } catch (e) { ok = false; }
        ta.remove();
        return ok;
    }

    // ── the dialog shell ──

    function openDialog(title, kind, opener) {
        closeDialog();
        var backdrop = el('div', 'azx-backdrop');
        var dlg = el('div', 'azx-dialog');
        dlg.setAttribute('role', 'dialog');
        dlg.setAttribute('aria-modal', 'true');
        dlg.dataset.kind = kind;
        var titleId = 'azx-title-' + (++S.seq);
        dlg.setAttribute('aria-labelledby', titleId);

        var head = el('div', 'azx-head');
        var h = el('h2', 'azx-title', title);
        h.id = titleId;
        var close = el('button', 'azx-close');
        close.type = 'button';
        close.title = 'Close (Esc)';
        close.setAttribute('aria-label', 'Close');
        close.appendChild(el('span', 'material-icons', 'close'));
        close.addEventListener('click', closeDialog);
        head.appendChild(h);
        head.appendChild(close);

        var body = el('div', 'azx-body');
        dlg.appendChild(head);
        dlg.appendChild(body);
        backdrop.appendChild(dlg);
        backdrop.addEventListener('mousedown', function (e) {
            if (e.target === backdrop) closeDialog();
        });
        // Keys typed in the dialog are the dialog's: the builder's shortcuts
        // (Delete, Ctrl+Z) listen on the document and must not act behind it.
        dlg.addEventListener('keydown', function (e) { e.stopPropagation(); });
        document.body.appendChild(backdrop);

        S.open = {
            kind: kind,
            backdrop: backdrop,
            dlg: dlg,
            body: body,
            opener: opener && opener !== document.body ? opener : null,
            run: null,
            token: 0,
        };
        return S.open;
    }

    function closeDialog() {
        var d = S.open;
        if (!d) return;
        S.open = null;
        d.backdrop.remove();
        var o = d.opener;
        if (o && o.isConnected && typeof o.focus === 'function') {
            try { o.focus(); } catch (e) { /* not focusable any more */ }
        }
    }

    function focusables(dlg) {
        var list = dlg.querySelectorAll('button, [href], input, select, textarea, [tabindex]');
        return Array.prototype.filter.call(list, function (n) {
            return !n.disabled && n.tabIndex >= 0 && n.getClientRects().length > 0;
        });
    }

    /** Capture phase, so it sees every key while a dialog is up. */
    function onKeyCapture(e) {
        var d = S.open;
        if (!d) return;
        if (e.key === 'Escape') {
            e.preventDefault();
            e.stopPropagation();
            closeDialog();
            return;
        }
        if (e.key === 'Tab') {
            var f = focusables(d.dlg);
            var i = trapIndex(f.length, f.indexOf(document.activeElement), e.shiftKey);
            e.preventDefault();
            e.stopPropagation();
            if (i >= 0) f[i].focus();
            return;
        }
        if (e.key === 'Enter' && (e.metaKey || e.ctrlKey) && d.run) {
            e.preventDefault();
            e.stopPropagation();
            d.run();
            return;
        }
        if (!d.dlg.contains(e.target)) e.stopPropagation();
    }

    /** The code panel: file name, Copy, Download, the code, a status line. */
    function outputPanel(parent) {
        var wrap = el('div', 'azx-out');
        var bar = el('div', 'azx-out-bar');
        var name = el('span', 'azx-out-name');
        var spacer = el('span', 'azx-spacer');
        var copy = textButton('Copy', 'content_copy', 'azx-copy');
        var dl = textButton('Download', 'download', 'azx-download');
        bar.appendChild(name);
        bar.appendChild(spacer);
        bar.appendChild(copy);
        bar.appendChild(dl);
        var pre = el('pre', 'azx-code');
        pre.tabIndex = 0;
        pre.setAttribute('aria-label', 'Generated code');
        var status = el('div', 'azx-status');
        status.setAttribute('role', 'status');
        status.setAttribute('aria-live', 'polite');
        wrap.appendChild(bar);
        wrap.appendChild(pre);
        wrap.appendChild(status);
        parent.appendChild(wrap);
        var out = { name: name, pre: pre, status: status, copy: copy, dl: dl, file: '', code: '' };
        copy.addEventListener('click', function () {
            if (!out.code) return;
            copyText(out.code).then(function (ok) {
                setStatus(out, ok ? 'Copied to the clipboard.' : 'The browser refused: select the code and copy it.', ok ? '' : 'warn');
            });
        });
        dl.addEventListener('click', function () {
            if (out.code) download(out.file || 'code.txt', out.code);
        });
        setCode(out, '', '');
        return out;
    }

    function setCode(out, file, code, warnings) {
        out.file = file || '';
        out.code = code || '';
        out.name.textContent = file || '';
        out.pre.textContent = code || '';
        out.pre.classList.toggle('azx-empty', !code);
        out.copy.disabled = !code;
        out.dl.disabled = !code;
        var w = (warnings || []).filter(Boolean);
        setStatus(out, w.join('\n'), w.length ? 'warn' : '');
    }

    function setStatus(out, text, kind) {
        out.status.textContent = text || '';
        out.status.className = 'azx-status' + (kind ? ' azx-' + kind : '');
    }

    function busy(out, on) {
        out.pre.classList.toggle('azx-busy', !!on);
    }

    /** Run `fn` unless the dialog closed or a newer request superseded it. */
    async function request(d, out, fn) {
        var token = ++d.token;
        busy(out, true);
        try {
            var v = await fn();
            if (S.open !== d || token !== d.token) return null;
            return v;
        } catch (e) {
            if (S.open === d && token === d.token) {
                setCode(out, '', '', []);
                setStatus(out, e.message, 'error');
            }
            return null;
        } finally {
            if (S.open === d && token === d.token) busy(out, false);
        }
    }

    function languageSelect(options, kind) {
        var s = selectOf(options, pickLanguage(options, remembered(kind)));
        s.addEventListener('change', function () { remember(kind, s.value); });
        return s;
    }

    function labelOf(options, id) {
        for (var i = 0; i < options.length; i++) if (options[i].id === id) return options[i].label;
        return id;
    }

    // ── Compile CSS to… ──

    async function openCssDialog(opener, preset) {
        var d = openDialog('Compile CSS to…', 'css', opener);
        var langs = await languages();
        if (S.open !== d) return;
        var registry = null;
        try { registry = await call({ op: 'get_component_registry' }); } catch (e) { registry = null; }
        if (S.open !== d) return;
        var components = componentChoices(registry);

        var sel = selectedDocNode();
        var sources = [
            { id: 'node', label: sel ? 'Selected node — ' + nodeLabel(sel) : 'Selected node (select one in the Document tree)', disabled: !sel },
            { id: 'document', label: 'Document stylesheet' },
            { id: 'component', label: 'A component’s CSS', disabled: !components.length },
            { id: 'text', label: 'Paste / type CSS' },
        ];
        var first = (preset && preset.source) || (sel && sel.uid !== 0 ? 'node' : 'document');
        var srcSel = selectOf(sources, first);
        var compSel = selectOf(components.map(function (c) {
            return { id: c.library + '\u0000' + c.name, label: c.label };
        }));
        var compField = field('Component', compSel);

        var row1 = el('div', 'azx-row');
        row1.appendChild(field('Stylesheet', srcSel));
        row1.appendChild(compField);
        d.body.appendChild(row1);

        var split = el('div', 'azx-split');
        var ta = el('textarea', 'azx-css');
        ta.spellcheck = false;
        ta.setAttribute('aria-label', 'CSS');
        ta.placeholder = '.card { padding: 8px; }';
        var rulesBox = el('div', 'azx-rules');
        var rulesHead = el('div', 'azx-rules-head');
        rulesHead.appendChild(el('span', 'azx-rules-title', 'Rules'));
        var all = textButton('All', null, 'azx-mini');
        var none = textButton('None', null, 'azx-mini');
        rulesHead.appendChild(all);
        rulesHead.appendChild(none);
        var rulesList = el('div', 'azx-rules-list');
        rulesList.setAttribute('role', 'group');
        rulesList.setAttribute('aria-label', 'Rules to compile');
        rulesBox.appendChild(rulesHead);
        rulesBox.appendChild(rulesList);
        split.appendChild(ta);
        split.appendChild(rulesBox);
        d.body.appendChild(split);

        var row2 = el('div', 'azx-row');
        var langSel = languageSelect(langs.css, 'css');
        var go = textButton('Compile', 'play_arrow', 'azx-primary');
        go.title = 'Compile (Ctrl/Cmd+Enter)';
        row2.appendChild(field('Language', langSel));
        row2.appendChild(el('span', 'azx-spacer'));
        row2.appendChild(go);
        d.body.appendChild(row2);
        var out = outputPanel(d.body);

        var rules = [];

        function currentSource() {
            var k = srcSel.value;
            if (k === 'node') return { kind: 'node', node: sel ? sel.uid : 0 };
            if (k === 'component') {
                var parts = String(compSel.value || '').split('\u0000');
                return { kind: 'component', library: parts[0], name: parts[1] };
            }
            if (k === 'document') return { kind: 'document' };
            return { kind: 'text', css: ta.value };
        }

        function renderRules(list) {
            rules = ruleRows(list);
            rulesList.innerHTML = '';
            if (!rules.length) {
                rulesList.appendChild(el('div', 'azx-hint', 'No rules.'));
                return;
            }
            rules.forEach(function (r) {
                var lab = el('label', 'azx-rule');
                lab.title = r.title;
                var cb = el('input');
                cb.type = 'checkbox';
                cb.checked = true;
                cb.dataset.index = String(r.index);
                cb.addEventListener('change', function () { compile(); });
                lab.appendChild(cb);
                lab.appendChild(el('code', 'azx-rule-sel', r.selector));
                lab.appendChild(el('span', 'azx-rule-count', r.count + (r.conditional ? ' · @' : '')));
                rulesList.appendChild(lab);
            });
        }

        function ticked() {
            return Array.prototype.filter.call(rulesList.querySelectorAll('input[type=checkbox]'), function (c) {
                return c.checked;
            }).map(function (c) { return Number(c.dataset.index); });
        }

        async function compile() {
            if (!langs.css.length) {
                setStatus(out, 'The server has no CSS code generator.', 'error');
                return;
            }
            if (!ta.value.trim()) {
                setCode(out, '', '');
                setStatus(out, 'This stylesheet is empty.', '');
                return;
            }
            var msg = compileCssMessage(langSel.value, ta.value, ticked(), rules.length);
            if (!msg) {
                setCode(out, '', '');
                setStatus(out, 'Tick at least one rule.', '');
                return;
            }
            var v = await request(d, out, function () { return call(msg); });
            if (!v) return;
            setCode(out, v.file_name, v.code, v.warnings);
            if (!(v.warnings || []).length) {
                setStatus(out, (v.rule_count != null ? v.rule_count : ticked().length) + ' rule(s) → '
                    + labelOf(langs.css, v.language || langSel.value) + '.', 'ok');
            }
        }

        async function refreshRules() {
            var v = await request(d, out, function () { return call(rulesMessage({ kind: 'text', css: ta.value })); });
            if (!v) return;
            renderRules(v.rules);
            await compile();
        }

        async function loadSource() {
            compField.style.display = srcSel.value === 'component' ? '' : 'none';
            var src = currentSource();
            if (src.kind === 'text') {
                ta.focus();
                return refreshRules();
            }
            var v = await request(d, out, function () { return call(rulesMessage(src)); });
            if (!v) return;
            ta.value = v.css || '';
            renderRules(v.rules);
            await compile();
        }

        d.run = compile;
        srcSel.addEventListener('change', loadSource);
        compSel.addEventListener('change', loadSource);
        langSel.addEventListener('change', compile);
        go.addEventListener('click', compile);
        ta.addEventListener('input', debounce(function () {
            if (S.open === d) refreshRules();
        }, 300));
        all.addEventListener('click', function () {
            rulesList.querySelectorAll('input[type=checkbox]').forEach(function (c) { c.checked = true; });
            compile();
        });
        none.addEventListener('click', function () {
            rulesList.querySelectorAll('input[type=checkbox]').forEach(function (c) { c.checked = false; });
            compile();
        });

        if (preset && preset.library && preset.name) {
            compSel.value = preset.library + '\u0000' + preset.name;
        }
        srcSel.focus();
        await loadSource();
    }

    // ── Subtree → code ──

    async function openSubtreeDialog(opener, uid) {
        if (uid == null) {
            var s = selectedDocNode();
            uid = s ? s.uid : 0;
        }
        var node = docNode(uid);
        var d = openDialog('Subtree → code', 'subtree', opener);
        var langs = await languages();
        if (S.open !== d) return;

        var info = el('div', 'azx-info');
        info.appendChild(el('span', 'azx-muted', 'Node: '));
        info.appendChild(el('code', 'azx-node', nodeLabel(node || { uid: uid, kind: 'element', tag: 'node', attrs: {} })));
        d.body.appendChild(info);

        var row = el('div', 'azx-row');
        var mode = selectOf([
            { id: 'function', label: 'A render function' },
            { id: 'app', label: 'A runnable app' },
        ], 'function');
        var name = el('input', 'azx-input');
        name.type = 'text';
        name.placeholder = 'render_…';
        name.spellcheck = false;
        var nameField = field('Function name', name);
        var langSel = languageSelect(langs.dom, 'dom');
        row.appendChild(field('Export as', mode));
        row.appendChild(nameField);
        row.appendChild(field('Language', langSel));
        d.body.appendChild(row);
        var out = outputPanel(d.body);

        async function generate() {
            nameField.style.display = mode.value === 'app' ? 'none' : '';
            var msg = subtreeMessage(uid, langSel.value, mode.value, name.value);
            var v = await request(d, out, function () { return call(msg); });
            if (!v) return;
            setCode(out, v.file_name, v.code, v.warnings);
            if (!(v.warnings || []).length) {
                setStatus(out, labelOf(langs.dom, v.language) + ': ' + v.file_name, 'ok');
            }
        }

        d.run = generate;
        mode.addEventListener('change', generate);
        langSel.addEventListener('change', generate);
        name.addEventListener('input', debounce(function () { if (S.open === d) generate(); }, 350));
        langSel.focus();
        await generate();
    }

    // ── Component → code ──

    async function openComponentDialog(opener, preset) {
        var d = openDialog('Component → code', 'component', opener);
        var langs = await languages();
        if (S.open !== d) return;
        var registry = null;
        try { registry = await call({ op: 'get_component_registry' }); } catch (e) {
            app.log('Component → code: ' + e.message, 'error');
        }
        if (S.open !== d) return;
        var choices = componentChoices(registry);

        var view = null;
        var comps = (app.state.componentData && app.state.componentData.components) || [];
        if (app.state.selectedComponentIdx != null && comps[app.state.selectedComponentIdx]) {
            view = comps[app.state.selectedComponentIdx].tag;
        }
        var initial = preset || defaultComponentChoice(choices, selectedDocNode(), app.state.selectedLibrary, view);

        var row = el('div', 'azx-row');
        var compSel = selectOf(choices.map(function (c) {
            return { id: c.library + '\u0000' + c.name, label: c.label };
        }), initial ? initial.library + '\u0000' + initial.name : null);
        var langSel = languageSelect(langs.dom, 'dom');
        row.appendChild(field('Component', compSel));
        row.appendChild(field('Language', langSel));
        d.body.appendChild(row);
        d.body.appendChild(el('div', 'azx-hint',
            'Its render function (a converted component’s texts and attributes are its parameters) '
            + 'and, for Rust / C / C++, the registration of its library.'));
        var out = outputPanel(d.body);

        async function generate() {
            if (!compSel.value) {
                setCode(out, '', '');
                setStatus(out, 'There are no components.', '');
                return;
            }
            var parts = compSel.value.split('\u0000');
            var msg = componentMessage({ library: parts[0], name: parts[1] }, langSel.value);
            var v = await request(d, out, function () { return call(msg); });
            if (!v) return;
            setCode(out, v.file_name, v.code, v.warnings);
            if (!(v.warnings || []).length) {
                setStatus(out, labelOf(langs.dom, v.language) + ': ' + v.file_name, 'ok');
            }
        }

        d.run = generate;
        compSel.addEventListener('change', generate);
        langSel.addEventListener('change', generate);
        compSel.focus();
        await generate();
    }

    // ── Export > Code ──

    /**
     * Replaces debugger.js's `exportCode`: `export_code_zip` answers JSON with
     * a `data:` URI; the old handler waited for a binary body, then read
     * `files` from the JSON — which a zip answer does not have — and logged
     * "No files generated".
     */
    async function exportCode(language) {
        try {
            var v = await call({ op: 'export_code_zip', language: language });
            if (!v || !v.download_url) throw new Error('the server sent no archive');
            downloadUrl(v.filename || ('azul-export-' + language + '.zip'), v.download_url);
            app.log('Exported the ' + language + ' project: ' + (v.files || []).join(', '), 'info');
            (v.warnings || []).forEach(function (w) { app.log('Export: ' + w, 'warning'); });
        } catch (e) {
            app.log('Code export failed: ' + e.message, 'error');
        }
    }

    // ── entry points: menu, toolbar, tree context menu ──

    function menuItem(icon, label, act) {
        var item = el('div', 'menu-dropdown-item');
        item.dataset.azx = act;
        var i = el('span', 'material-icons mi', icon);
        item.appendChild(i);
        item.appendChild(document.createTextNode(label));
        return item;
    }

    function openFromMenu(act) {
        // A click on a menu item does not move focus: what had it gets it back.
        var opener = document.activeElement;
        if (act === 'css') return openCssDialog(opener);
        if (act === 'subtree') return openSubtreeDialog(opener, null);
        if (act === 'component') return openComponentDialog(opener);
    }

    function injectMenu() {
        var dd = document.querySelector('.menu-item[data-menu="export"] > .menu-dropdown');
        if (!dd || dd.querySelector('[data-azx]')) return;
        var items = [
            menuItem('style', 'Compile CSS to…', 'css'),
            menuItem('account_tree', 'Subtree → code…', 'subtree'),
            menuItem('widgets', 'Component → code…', 'component'),
        ];
        var sep = el('div', 'menu-dropdown-separator');
        var first = dd.firstChild;
        items.forEach(function (it) {
            it.addEventListener('click', function () { openFromMenu(it.dataset.azx); });
            dd.insertBefore(it, first);
        });
        dd.insertBefore(sep, first);
    }

    function injectToolbarButton() {
        var bar = document.getElementById('azb-toolbar');
        if (!bar || bar.querySelector('[data-azx-act]')) return !!bar;
        var b = el('button', 'azb-icon');
        b.type = 'button';
        b.dataset.azxAct = 'export';
        b.title = 'Export the selected subtree as code';
        b.setAttribute('aria-label', 'Export the selected subtree as code');
        b.appendChild(el('span', 'material-icons', 'code'));
        b.addEventListener('click', function () { openSubtreeDialog(b, null); });
        var reset = bar.querySelector('button[data-act="reset"]');
        bar.insertBefore(b, reset || null);
        return true;
    }

    /**
     * The Document tree's context menu (debugger-dnd.js) is built inside the
     * row's own listener; note which row was right-clicked (capture phase,
     * before it) and add the export items when that menu opens.
     */
    function hookRowMenu() {
        var cm = app.widgets && app.widgets.ContextMenu;
        var tree = document.getElementById('dom-tree-container');
        if (!cm || !tree || cm.__azxHooked) return;
        var pending = null;
        tree.addEventListener('contextmenu', function (e) {
            var row = e.target && e.target.closest ? e.target.closest('.azb-row') : null;
            pending = row && row.dataset.uid != null ? Number(row.dataset.uid) : null;
            setTimeout(function () { pending = null; }, 0);
        }, true);
        var show = cm.show;
        cm.show = function (x, y, items) {
            if (pending != null && Array.isArray(items)) {
                var uid = pending;
                pending = null;
                var node = docNode(uid);
                var extra = [{ separator: true },
                    { icon: 'code', label: 'Export as code…', action: function () { openSubtreeDialog(null, uid); } }];
                if (node && node.kind === 'component') {
                    extra.push({ icon: 'widgets', label: 'Component → code…', action: function () {
                        openComponentDialog(null, { library: node.library, name: node.tag });
                    } });
                }
                if (node && node.kind !== 'text') {
                    extra.push({ icon: 'style', label: 'Compile its CSS to…', action: function () {
                        openCssDialog(null, { source: 'node' });
                    } });
                }
                items = items.concat(extra);
            }
            return show.call(this, x, y, items);
        };
        cm.__azxHooked = true;
    }

    // ── styles ──

    function injectStyle() {
        if (document.getElementById('azx-style')) return;
        var s = document.createElement('style');
        s.id = 'azx-style';
        s.textContent = [
            '.azx-backdrop{position:fixed;inset:0;background:rgba(0,0,0,.55);z-index:2500;display:flex;align-items:center;justify-content:center}',
            '.azx-dialog{background:var(--bg-sidebar);color:var(--text-main);border:1px solid var(--border);border-radius:6px;box-shadow:0 12px 40px rgba(0,0,0,.6);width:min(860px,94vw);max-height:90vh;display:flex;flex-direction:column;font-size:12px}',
            '.azx-head{display:flex;align-items:center;gap:8px;padding:8px 12px;border-bottom:1px solid var(--border)}',
            '.azx-title{font-size:13px;font-weight:600;flex:1;margin:0}',
            '.azx-close{background:transparent;border:0;color:var(--text-muted);cursor:pointer;border-radius:3px;display:inline-flex;padding:2px}',
            '.azx-close:hover{background:var(--bg-hover);color:var(--text-main)}',
            '.azx-close .material-icons{font-size:18px}',
            '.azx-body{padding:10px 12px 12px;display:flex;flex-direction:column;gap:8px;overflow:auto;min-height:0}',
            '.azx-row{display:flex;align-items:flex-end;gap:10px;flex-wrap:wrap}',
            '.azx-spacer{flex:1}',
            '.azx-field{display:flex;flex-direction:column;gap:3px;min-width:0}',
            '.azx-field-label{font-size:10px;text-transform:uppercase;letter-spacing:.04em;color:var(--text-muted)}',
            '.azx-select,.azx-input{background:var(--bg-input);color:var(--text-main);border:1px solid var(--border);border-radius:3px;padding:3px 6px;font-size:12px;max-width:360px}',
            '.azx-input{width:180px;font-family:monospace}',
            '.azx-btn{display:inline-flex;align-items:center;gap:4px;background:var(--bg-input);color:var(--text-main);border:1px solid var(--border);border-radius:3px;padding:3px 8px;font-size:12px;cursor:pointer}',
            '.azx-btn .material-icons{font-size:14px}',
            '.azx-btn:hover:not(:disabled){background:var(--bg-hover)}',
            '.azx-btn:disabled{opacity:.4;cursor:default}',
            '.azx-primary{background:var(--accent);border-color:var(--accent);color:#fff}',
            '.azx-primary:hover:not(:disabled){background:#1a8ad8}',
            '.azx-mini{padding:0 6px;font-size:10px}',
            '.azx-dialog :focus-visible{outline:2px solid var(--accent);outline-offset:1px}',
            '.azx-split{display:grid;grid-template-columns:minmax(0,1fr) 220px;gap:8px;min-height:130px}',
            '.azx-css{background:var(--bg-panel);color:var(--text-main);border:1px solid var(--border);border-radius:3px;font:12px/1.4 monospace;padding:6px;resize:vertical;min-height:130px}',
            '.azx-rules{border:1px solid var(--border);border-radius:3px;display:flex;flex-direction:column;min-height:0;max-height:220px}',
            '.azx-rules-head{display:flex;align-items:center;gap:4px;padding:3px 6px;border-bottom:1px solid var(--border)}',
            '.azx-rules-title{flex:1;font-size:10px;text-transform:uppercase;letter-spacing:.04em;color:var(--text-muted)}',
            '.azx-rules-list{overflow:auto;padding:2px 0}',
            '.azx-rule{display:flex;align-items:center;gap:6px;padding:2px 6px;cursor:pointer}',
            '.azx-rule:hover{background:var(--bg-hover)}',
            '.azx-rule-sel{flex:1;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;color:var(--tag-color)}',
            '.azx-rule-count{color:var(--text-muted);font-size:10px}',
            '.azx-out{display:flex;flex-direction:column;border:1px solid var(--border);border-radius:3px;min-height:0}',
            '.azx-out-bar{display:flex;align-items:center;gap:6px;padding:4px 6px;border-bottom:1px solid var(--border)}',
            '.azx-out-name{font-family:monospace;color:var(--text-muted)}',
            '.azx-code{margin:0;padding:8px;font:12px/1.45 monospace;white-space:pre;overflow:auto;max-height:46vh;min-height:120px;background:var(--bg-panel);user-select:text}',
            '.azx-code.azx-empty{color:var(--text-muted)}',
            '.azx-code.azx-busy{opacity:.5}',
            '.azx-status{padding:3px 8px;min-height:18px;white-space:pre-wrap;color:var(--text-muted);border-top:1px solid var(--border)}',
            '.azx-status.azx-error{color:var(--error)}',
            '.azx-status.azx-warn{color:var(--warning)}',
            '.azx-status.azx-ok{color:var(--success)}',
            '.azx-info{display:flex;align-items:center;gap:6px}',
            '.azx-muted,.azx-hint{color:var(--text-muted)}',
            '.azx-node{color:var(--tag-color)}',
        ].join('\n');
        document.head.appendChild(s);
    }

    // ── install ──

    function install() {
        injectStyle();
        injectMenu();
        document.addEventListener('keydown', onKeyCapture, true);
        app.handlers.exportCode = exportCode;
        // The Document toolbar and the tree exist once debugger-dnd.js set
        // them up (it runs before this file).
        if (!injectToolbarButton()) {
            var tries = 0;
            var t = setInterval(function () {
                if (injectToolbarButton() || ++tries > 50) clearInterval(t);
            }, 100);
        }
        hookRowMenu();
    }

    install();
    root.azExport = {
        logic: logic,
        state: S,
        openCssDialog: openCssDialog,
        openSubtreeDialog: openSubtreeDialog,
        openComponentDialog: openComponentDialog,
        closeDialog: closeDialog,
        exportCode: exportCode,
    };
})(typeof window !== 'undefined' ? window : globalThis);
