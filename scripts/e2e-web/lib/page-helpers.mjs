// In-page helper bundle, injected once per tab via Runtime.evaluate.
//
// Part of the web e2e harness (scripts/web-e2e-harness-plan.md §4.2 selector
// translation, §3.2 wasm-export probes). Everything runs inside the page and
// must be ES5-ish, self-contained, and free of backticks / ${ (it is shipped
// as a JS template literal).
//
// Selector translation (verified against dll/src/web/html_render.rs in the
// plan doc): user `id` is remapped to `data-az-id` on the emitted mirror, so
// a scenario selector "#foo" must try [data-az-id="foo"] first; every azul
// node also has the DFS id az_N, which indexes the wasm positioned-rects
// cache (4 u32 = x,y,w,h per node; sentinel = any coord with bit 31 set —
// same rules as scripts/cdp_click_hw.js:45-56).

export const PAGE_HELPERS = `
(function() {
    if (window.__azE2E) return 'already-installed';
    var H = {};

    // --- selector translation -------------------------------------------
    H.candidates = function(sel) {
        var out = [];
        if (typeof sel === 'string' && sel.indexOf('#') !== -1) {
            // user id -> data-az-id remap (html_render.rs:366-369), applied
            // per ID TOKEN so compound selectors ("#foo > div", "div#foo.bar")
            // translate too; #az_N mirror ids stay literal.
            var remapped = sel.replace(/#(?!az_)([A-Za-z_][\\w-]*)/g, '[data-az-id="$1"]');
            if (remapped !== sel) out.push(remapped);
        }
        // Desktop's "body" selects the azul root node; the mirror emits that
        // root as div#az_0 inside <body><div id="az-body"> (verified against
        // the served HTML: az_0 carries the 8px body margin). So scenario
        // "body > div" must resolve as "#az_0 > div", never as the real
        // <body> whose first div is the az-body wrapper.
        if (typeof sel === 'string' && /^body(?![\w-])/.test(sel)) {
            out.push(sel.replace(/^body/, '#az_0'));
        }
        out.push(sel);
        return out;
    };
    H.q = function(sel) {
        var cands = H.candidates(sel);
        for (var i = 0; i < cands.length; i++) {
            try {
                var el = document.querySelector(cands[i]);
                if (el) return el;
            } catch (e) { /* invalid selector variant */ }
        }
        return null;
    };
    H.count = function(sel) {
        var cands = H.candidates(sel);
        for (var i = 0; i < cands.length; i++) {
            try {
                var n = document.querySelectorAll(cands[i]).length;
                if (n > 0) return n;
            } catch (e) { /* invalid selector variant */ }
        }
        return 0;
    };

    // --- text addressing: deepest element whose trimmed textContent
    //     matches exactly; fallback: deepest containing it ---------------
    H.byText = function(text) {
        var all = document.body ? document.body.getElementsByTagName('*') : [];
        var exact = null, contains = null;
        for (var i = 0; i < all.length; i++) {
            var el = all[i];
            var tag = el.tagName;
            // The loader <script> sits inside <body>; its source must never
            // match user-facing text.
            if (tag === 'SCRIPT' || tag === 'STYLE' || tag === 'NOSCRIPT') continue;
            var t = (el.textContent || '').trim();
            if (t === text) exact = el;              // later = deeper in doc order
            else if (t.indexOf(text) !== -1) contains = el;
        }
        return exact || contains;
    };

    // --- wasm positioned-rects cache ------------------------------------
    H.wasmRect = function(n) {
        var P = window.__azProbe;
        if (!P || !P.mini || typeof P.mini.AzStartup_getPositionedRectsLen !== 'function') return null;
        try {
            var len = P.mini.AzStartup_getPositionedRectsLen(P.state) >>> 0;
            var ptr = P.mini.AzStartup_getPositionedRectsPtr(P.state) >>> 0;
            if (!(len > 0 && ptr > 0 && n >= 0 && n < len)) return null;
            var dv = new DataView(P.memory.buffer);
            var o = ptr + 16 * n;
            var x = dv.getUint32(o, true), y = dv.getUint32(o + 4, true);
            var w = dv.getUint32(o + 8, true), h = dv.getUint32(o + 12, true);
            if (w === 0 || h === 0) return null;
            if (((x | y | w | h) >>> 31) !== 0) return null; // sentinel/flag form
            return { x: x, y: y, w: w, h: h };
        } catch (e) { return null; }
    };

    H.azIdOf = function(el) {
        var e = el && el.closest ? el.closest('[id^="az_"]') : null;
        if (!e) return null;
        var m = /^az_(\\d+)$/.exec(e.id);
        return m ? parseInt(m[1], 10) : null;
    };

    H.elInfo = function(el) {
        if (!el) return null;
        var r = el.getBoundingClientRect();
        return {
            azId: H.azIdOf(el),
            id: el.id || null,
            dataAzId: el.getAttribute ? el.getAttribute('data-az-id') : null,
            tag: el.tagName ? el.tagName.toLowerCase() : null,
            rect: { x: r.x, y: r.y, w: r.width, h: r.height },
            text: (el.textContent || '').trim().slice(0, 200)
        };
    };

    // --- target resolution: {selector | text | node_id} -> click point --
    // Coordinate policy: prefer the wasm rect for the node (source of truth
    // for azul geometry; document coords -> viewport via scroll offset),
    // fall back to the element's getBoundingClientRect center.
    H.resolve = function(t) {
        var el = null, how = '';
        if (t.selector != null) { el = H.q(t.selector); how = 'selector'; }
        else if (t.text != null) { el = H.byText(t.text); how = 'text'; }
        else if (t.node_id != null) { el = document.getElementById('az_' + t.node_id); how = 'node_id'; }
        if (!el) return { found: false, how: how };
        var info = H.elInfo(el);
        var pt = null, src = '';
        if (info.azId != null) {
            var wr = H.wasmRect(info.azId);
            if (wr) {
                pt = { x: wr.x + wr.w / 2 - window.scrollX, y: wr.y + wr.h / 2 - window.scrollY };
                src = 'wasm-rect az_' + info.azId;
            }
        }
        if (!pt) {
            pt = { x: info.rect.x + info.rect.w / 2, y: info.rect.y + info.rect.h / 2 };
            src = 'element-center';
        }
        return { found: true, how: how, x: pt.x, y: pt.y, src: src, info: info };
    };

    // --- diagnostics -----------------------------------------------------
    H.stateInfo = function() {
        var P = window.__azProbe;
        var out = {
            url: location.href,
            title: document.title,
            domNodes: document.querySelectorAll('*').length,
            azNodes: document.querySelectorAll('[id^="az_"]').length,
            bodyTextLen: (document.body && document.body.innerText || '').length,
            probe: !!P,
            pending: (typeof window.__az_pending === 'number') ? window.__az_pending : null
        };
        if (P && P.mini) {
            try {
                if (typeof P.mini.AzStartup_isLayoutSolved === 'function')
                    out.layoutSolved = P.mini.AzStartup_isLayoutSolved(P.state);
                if (typeof P.mini.AzStartup_getPositionedRectsLen === 'function')
                    out.rectsLen = P.mini.AzStartup_getPositionedRectsLen(P.state) >>> 0;
                if (typeof P.mini.AzStartup_isStyledDomHydrated === 'function')
                    out.hydrated = P.mini.AzStartup_isStyledDomHydrated(P.state);
            } catch (e) { out.probeError = String(e && e.message); }
        }
        return out;
    };
    H.focusInfo = function() {
        var a = document.activeElement;
        return {
            active: a ? H.elInfo(a) : null,
            isBody: a === document.body,
            contenteditable: !!(a && a.isContentEditable)
        };
    };
    H.selectionInfo = function() {
        var s = window.getSelection ? window.getSelection() : null;
        return s ? { text: s.toString().slice(0, 500), rangeCount: s.rangeCount, type: s.type } : null;
    };

    // --- css normalization for assert_css --------------------------------
    // Returns {ok:true, actual, expected} when both sides normalize into a
    // comparable form (px number or canonical color), else {ok:false}.
    H.cssCompare = function(sel, prop, expected) {
        var el = H.q(sel);
        if (!el) return { ok: true, found: false };
        var actual = getComputedStyle(el)[prop];
        if (actual === undefined || actual === null) return { ok: false, reason: 'unknown property', actual: null };
        var exp = String(expected).trim(), act = String(actual).trim();
        if (act.toLowerCase() === exp.toLowerCase()) return { ok: true, found: true, pass: true, actual: act };
        // px-number tolerance compare
        var mA = /^(-?[0-9.]+)px$/.exec(act), mE = /^(-?[0-9.]+)(px)?$/.exec(exp);
        if (mA && mE) {
            return { ok: true, found: true, pass: Math.abs(parseFloat(mA[1]) - parseFloat(mE[1])) <= 0.5, actual: act };
        }
        // color canonicalization via a scratch element
        var scratch = document.createElement('div');
        scratch.style.color = exp;
        if (scratch.style.color !== '') {
            document.body.appendChild(scratch);
            var canon = getComputedStyle(scratch).color;
            document.body.removeChild(scratch);
            var actCanon = act;
            if (!/^rgb/.test(actCanon)) {
                scratch.style.color = actCanon;
                document.body.appendChild(scratch);
                actCanon = getComputedStyle(scratch).color;
                document.body.removeChild(scratch);
            }
            if (canon) return { ok: true, found: true, pass: actCanon === canon, actual: act };
        }
        // desktop expects Rust Debug strings for many props (plan §4.5) —
        // those can never string-match browser values: report un-normalizable.
        return { ok: false, reason: 'no normalizer for value form', actual: act };
    };

    window.__azE2E = H;
    return 'installed';
})()
`;
