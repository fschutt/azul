// Step interpreter: AZ_E2E scenario op → CDP realization.
//
// Part of the web e2e harness (scripts/web-e2e-harness-plan.md §4.3 mapping
// table, §4.4 keymap use). Action + query ops live here; assert_* ops live in
// asserts.mjs; run.mjs routes between them and owns artifacts/results.
//
// Settle protocol: after every input op we wait for the loader-owned signal
// `window.__az_settled(cb)` (pending async work == 0 across two rAF ticks,
// dll/src/web/loader_js.rs:41-62). Servers started before 2026-08-17 serve a
// loader WITHOUT it — the driver then falls back to a fixed delay
// (opts.settleFallbackMs). A settle TIMEOUT (pending never reached 0) is
// reported loudly to the caller, which records it on the step.

import { PAGE_HELPERS } from './page-helpers.mjs';
import { lookupKey, modifiersToBits } from './keymap.mjs';

const sleep = ms => new Promise(r => setTimeout(r, ms));

/** Ops after which the driver settles (they can trigger dispatch → patches → async work). */
export const INPUT_OPS = new Set([
    'click', 'double_click', 'mouse_move', 'mouse_down', 'mouse_up',
    'scroll', 'scroll_node_by', 'scroll_into_view',
    'key_down', 'key_up', 'text_input',
    'resize', 'relayout', 'focus',
]);

/** Non-portable on web (plan §2.4): recorded as SKIP, never FAIL. */
const SKIP_OPS = new Set([
    'touch_start', 'touch_move', 'touch_end', 'touch_cancel',
    'pen_down', 'pen_move', 'pen_up',
    'swipe', 'pinch', 'rotate', 'long_press',
    'insert_node', 'delete_node', 'set_node_text', 'set_node_classes',
    'set_node_css_override',
    'get_app_state', 'set_app_state',
    'take_native_screenshot',
    'blur', 'move', 'close', 'dpi_changed',
]);

const BUTTON_BIT = { left: 1, right: 2, middle: 4 };

export class Driver {
    /**
     * @param cdp   connected lib/cdp.mjs Cdp instance
     * @param opts  {settleCapMs, settleFallbackMs, log(line)}
     */
    constructor(cdp, opts = {}) {
        this.cdp = cdp;
        this.settleCapMs = opts.settleCapMs ?? 5000;
        this.settleFallbackMs = opts.settleFallbackMs ?? 500;
        this.log = opts.log || (() => {});
        this.heldMods = 0;   // CDP bitmask from held modifier keys (LShift…LControl brackets)
        this.buttons = 0;    // held mouse buttons bitmask
        this.metrics = { width: 800, height: 600, dsf: 1 };
    }

    async installHelpers() {
        const r = await this.cdp.eval(PAGE_HELPERS);
        this.log(`page helpers: ${r}`);
    }

    async setMetrics(width, height, dsf = 1) {
        this.metrics = { width, height, dsf };
        await this.cdp.send('Emulation.setDeviceMetricsOverride', {
            width, height, deviceScaleFactor: dsf, mobile: false,
        });
    }

    /**
     * Boot settle: wait until the loader announces itself — `window.__azProbe`
     * exists (installed right before the '[azul-web] bootstrap complete' log,
     * loader_js.rs:661-677) or that console line was seen. Then run a normal
     * settle to cover font/shard fetches.
     * @returns {ok, ms, reason?}
     */
    async bootWait(timeoutMs = 30_000) {
        const t0 = Date.now();
        while (Date.now() - t0 < timeoutMs) {
            let probe = false;
            try {
                probe = await this.cdp.eval('typeof window.__azProbe !== "undefined"');
            } catch { /* navigation race — retry */ }
            const bootLine = this.cdp.console.some(c => c.text.includes('bootstrap complete'));
            if (probe || bootLine) {
                const settle = await this.settle('boot');
                return { ok: true, ms: Date.now() - t0, settle };
            }
            if (this.cdp.exceptions.length > 0 &&
                this.cdp.console.some(c => c.text.includes('bootstrap FAILED'))) {
                return { ok: false, ms: Date.now() - t0, reason: 'loader reported bootstrap FAILED' };
            }
            await sleep(250);
        }
        return { ok: false, ms: Date.now() - t0, reason: `no __azProbe / 'bootstrap complete' within ${timeoutMs} ms` };
    }

    /**
     * Settle via __az_settled with a cap; fixed-delay fallback when the page's
     * loader predates the settle protocol.
     * @returns {mode: 'settled'|'fallback'|'timeout', ms}
     */
    async settle(tag = '') {
        const t0 = Date.now();
        let verdict;
        try {
            verdict = await this.cdp.eval(
                `(function(cap){
                    if (typeof window.__az_settled !== 'function') return Promise.resolve('missing');
                    return new Promise(function(res){
                        var done = false;
                        var t = setTimeout(function(){ if (!done) { done = true; res('timeout'); } }, cap);
                        try {
                            window.__az_settled(function(){
                                if (!done) { done = true; clearTimeout(t); res('settled'); }
                            });
                        } catch (e) { if (!done) { done = true; clearTimeout(t); res('error:' + (e && e.message)); } }
                    });
                })(${this.settleCapMs})`,
                { awaitPromise: true, timeoutMs: this.settleCapMs + 5000 },
            );
        } catch (e) {
            verdict = `error:${e.message}`;
        }
        if (verdict === 'settled') return { mode: 'settled', ms: Date.now() - t0 };
        if (verdict === 'missing' || String(verdict).startsWith('error:')) {
            // Loader without the settle protocol (pre-2026-08-17 server) — fixed delay.
            await sleep(this.settleFallbackMs);
            return { mode: 'fallback', ms: Date.now() - t0, detail: String(verdict) };
        }
        // 'timeout' — __az_pending never reached 0. LOUD, but the caller decides.
        this.log(`SETTLE TIMEOUT${tag ? ` (${tag})` : ''}: __az_pending stuck > 0 for ${this.settleCapMs} ms`);
        return { mode: 'timeout', ms: Date.now() - t0 };
    }

    /** Resolve {selector|text|node_id} → viewport click point via the wasm rect
     *  cache (element-center fallback). Throws when the target does not exist. */
    async resolveTarget(params) {
        const t = { selector: params.selector, text: params.text, node_id: params.node_id };
        const r = await this.cdp.eval(`window.__azE2E.resolve(${JSON.stringify(t)})`);
        if (!r || !r.found) {
            throw new Error(`target not found (${JSON.stringify(t)})`);
        }
        this.log(`resolved ${JSON.stringify(t)} -> (${Math.round(r.x)},${Math.round(r.y)}) via ${r.src}`);
        return r;
    }

    async mouseEvent(type, x, y, { button = 'none', clickCount = 0, deltaX = 0, deltaY = 0, extraMods = 0 } = {}) {
        const p = {
            type, x: Math.round(x), y: Math.round(y),
            modifiers: this.heldMods | extraMods,
            button, buttons: this.buttons, clickCount,
        };
        if (type === 'mouseWheel') { p.deltaX = deltaX; p.deltaY = deltaY; }
        await this.cdp.send('Input.dispatchMouseEvent', p);
    }

    async clickAt(x, y, button = 'left', extraMods = 0, clickCount = 1) {
        const bit = BUTTON_BIT[button] || 1;
        await this.mouseEvent('mouseMoved', x, y, { extraMods });
        this.buttons |= bit;
        await this.mouseEvent('mousePressed', x, y, { button, clickCount, extraMods });
        this.buttons &= ~bit;
        await this.mouseEvent('mouseReleased', x, y, { button, clickCount, extraMods });
    }

    async keyEvent(direction, keyName, explicitMods) {
        const extraMods = modifiersToBits(explicitMods);
        const k = lookupKey(keyName, this.heldMods | extraMods);
        if (!k) throw new Error(`unknown key name "${keyName}" (see lib/keymap.mjs)`);
        if (direction === 'down' && k.mod) this.heldMods |= k.mod;
        if (direction === 'up' && k.mod) this.heldMods &= ~k.mod;
        const p = {
            // puppeteer convention: keyDown only when it produces text, else rawKeyDown
            type: direction === 'down' ? (k.text ? 'keyDown' : 'rawKeyDown') : 'keyUp',
            modifiers: this.heldMods | extraMods,
            key: k.key, code: k.code,
            windowsVirtualKeyCode: k.vk, nativeVirtualKeyCode: k.vk,
        };
        if (direction === 'down' && k.text) { p.text = k.text; p.unmodifiedText = k.text; }
        await this.cdp.send('Input.dispatchKeyEvent', p);
        return k;
    }

    /**
     * Run one action/query step.
     * @returns {{status:'pass'|'fail'|'skip', message?:string, response?:object, wantScreenshot?:boolean}}
     */
    async runStep(op, params) {
        if (SKIP_OPS.has(op)) {
            return { status: 'skip', message: `op "${op}" is not portable to the web backend (plan §2.4)` };
        }
        switch (op) {
            // ---- mouse ---------------------------------------------------
            case 'mouse_move': {
                await this.mouseEvent('mouseMoved', params.x || 0, params.y || 0);
                return { status: 'pass' };
            }
            case 'mouse_down': {
                const button = params.button || 'left';
                this.buttons |= (BUTTON_BIT[button] || 1);
                await this.mouseEvent('mousePressed', params.x || 0, params.y || 0, { button, clickCount: 1 });
                return { status: 'pass' };
            }
            case 'mouse_up': {
                const button = params.button || 'left';
                this.buttons &= ~(BUTTON_BIT[button] || 1);
                await this.mouseEvent('mouseReleased', params.x || 0, params.y || 0, { button, clickCount: 1 });
                return { status: 'pass' };
            }
            case 'click': {
                let x = params.x, y = params.y, src = 'coords', target = null;
                if (x == null || y == null) {
                    const r = await this.resolveTarget(params);
                    x = r.x; y = r.y; src = r.src; target = r.info;
                }
                await this.clickAt(x, y, params.button || 'left');
                return { status: 'pass', response: { x, y, src, target } };
            }
            case 'double_click': {
                let x = params.x, y = params.y;
                if (x == null || y == null) {
                    const r = await this.resolveTarget(params);
                    x = r.x; y = r.y;
                }
                await this.clickAt(x, y, params.button || 'left', 0, 1);
                await this.clickAt(x, y, params.button || 'left', 0, 2);
                return { status: 'pass', response: { x, y } };
            }
            // ---- scrolling -----------------------------------------------
            case 'scroll': {
                // CDP deltaX/deltaY carry DOM WheelEvent sign (positive deltaY
                // scrolls content down) — same convention as the scenario files.
                await this.mouseEvent('mouseWheel', params.x || 0, params.y || 0,
                    { deltaX: params.delta_x || 0, deltaY: params.delta_y || 0 });
                return { status: 'pass' };
            }
            case 'scroll_node_by': {
                const r = await this.resolveTarget(params);
                await this.mouseEvent('mouseWheel', r.x, r.y,
                    { deltaX: params.delta_x || 0, deltaY: params.delta_y || 0 });
                return { status: 'pass', response: { at: { x: r.x, y: r.y }, src: r.src } };
            }
            case 'scroll_into_view': {
                const t = { selector: params.selector, text: params.text, node_id: params.node_id };
                const o = {
                    block: params.block || 'start',
                    inline: params.inline || 'nearest',
                    behavior: params.behavior === 'smooth' ? 'smooth' : 'auto',
                };
                const r = await this.cdp.eval(
                    `(function(t, o){ var H = window.__azE2E;
                        var el = t.selector != null ? H.q(t.selector)
                               : t.text != null ? H.byText(t.text)
                               : t.node_id != null ? document.getElementById('az_' + t.node_id) : null;
                        if (!el) return { found: false };
                        el.scrollIntoView(o);
                        return { found: true, info: H.elInfo(el) };
                    })(${JSON.stringify(t)}, ${JSON.stringify(o)})`);
                if (!r.found) return { status: 'fail', message: `scroll_into_view: target not found ${JSON.stringify(t)}` };
                return { status: 'pass', response: r.info };
            }
            // ---- keyboard ------------------------------------------------
            case 'key_down': {
                const k = await this.keyEvent('down', params.key, params.modifiers);
                return { status: 'pass', response: { key: k.key, code: k.code, heldMods: this.heldMods } };
            }
            case 'key_up': {
                const k = await this.keyEvent('up', params.key, params.modifiers);
                return { status: 'pass', response: { key: k.key, code: k.code, heldMods: this.heldMods } };
            }
            case 'text_input': {
                // Input.insertText inserts into the focused editable element and
                // fires the `input` event the loader forwards (azDispatchWithText,
                // loader_js.rs:1104-1110). Desktop commits per call; web `input`
                // carries the whole value — assert on outcomes, not event counts.
                const focus = await this.cdp.eval('window.__azE2E.focusInfo()');
                await this.cdp.send('Input.insertText', { text: String(params.text ?? '') });
                return { status: 'pass', response: { focusBefore: focus } };
            }
            // ---- window --------------------------------------------------
            case 'resize': {
                const w = Math.round(params.width), h = Math.round(params.height);
                await this.setMetrics(w, h, this.metrics.dsf);
                // Chromium fires window `resize` when the override changes the
                // viewport; verify and fall back to a synthetic event if not.
                const got = await this.cdp.eval('[window.innerWidth, window.innerHeight]');
                if (!got || got[0] !== w || got[1] !== h) {
                    await this.cdp.eval('window.dispatchEvent(new Event("resize"))');
                }
                return { status: 'pass', response: { innerSize: got } };
            }
            case 'focus': {
                await this.cdp.send('Emulation.setFocusEmulationEnabled', { enabled: true });
                return { status: 'pass' };
            }
            // ---- timing --------------------------------------------------
            case 'wait': {
                await sleep(Number(params.ms) || 0);
                return { status: 'pass' };
            }
            case 'wait_frame': {
                await this.cdp.eval(
                    'new Promise(r => requestAnimationFrame(() => requestAnimationFrame(() => r("frame"))))',
                    { awaitPromise: true });
                return { status: 'pass' };
            }
            case 'relayout': {
                const r = await this.cdp.eval(
                    `(function(){ var P = window.__azProbe;
                        if (!P || !P.mini) return 'no-probe';
                        var f = P.mini.AzStartup_solveLayoutReal || P.mini.AzStartup_solveLayout;
                        if (typeof f !== 'function') return 'no-solver';
                        try { return 'rc=' + f(P.state, window.innerWidth, window.innerHeight); }
                        catch (e) { return 'trap: ' + (e && e.message); }
                    })()`);
                return { status: 'pass', response: { solve: r } };
            }
            case 'redraw': {
                return { status: 'pass', message: 'no-op on web (browser paints)' };
            }
            // ---- screenshots (capture handled by run.mjs) ----------------
            case 'take_screenshot': {
                return { status: 'pass', wantScreenshot: true };
            }
            // ---- queries → diagnostics (never fail the step) -------------
            case 'get_state':
                return { status: 'pass', response: await this.cdp.eval('window.__azE2E.stateInfo()') };
            case 'get_focus_state':
                return { status: 'pass', response: await this.cdp.eval('window.__azE2E.focusInfo()') };
            case 'get_selection_state':
                return { status: 'pass', response: await this.cdp.eval('window.__azE2E.selectionInfo()') };
            case 'get_cursor_state':
                return { status: 'pass', response: { note: 'cursor state is wasm-internal; no export on web yet (plan §3.3)' } };
            case 'find_node_by_text': {
                const r = await this.cdp.eval(
                    `(function(t){ var el = window.__azE2E.byText(t);
                        return el ? window.__azE2E.elInfo(el) : null; })(${JSON.stringify(String(params.text ?? ''))})`);
                return { status: 'pass', response: r };
            }
            case 'hit_test': {
                const r = await this.cdp.eval(
                    `(function(x, y){ var el = document.elementFromPoint(x, y);
                        return el ? window.__azE2E.elInfo(el) : null;
                    })(${Number(params.x) || 0}, ${Number(params.y) || 0})`);
                return { status: 'pass', response: r };
            }
            case 'get_node_layout': {
                const r = await this.cdp.eval(
                    `(function(s){ var el = window.__azE2E.q(s);
                        return el ? window.__azE2E.elInfo(el) : null; })(${JSON.stringify(String(params.selector ?? ''))})`);
                return { status: 'pass', response: r };
            }
            case 'get_html_string':
            case 'get_dom':
            case 'get_dom_tree': {
                const html = await this.cdp.eval('document.body ? document.body.outerHTML : ""');
                const s = String(html || '');
                return { status: 'pass', response: { length: s.length, head: s.slice(0, 4096) } };
            }
            case 'get_scroll_states': {
                const r = await this.cdp.eval(
                    `(function(){ var out = [];
                        var els = document.querySelectorAll('[id^="az_"]');
                        for (var i = 0; i < els.length; i++) {
                            var e = els[i];
                            if (e.scrollTop || e.scrollLeft) {
                                out.push({ id: e.id, scrollTop: e.scrollTop, scrollLeft: e.scrollLeft });
                            }
                        }
                        out.push({ id: 'window', scrollTop: window.scrollY, scrollLeft: window.scrollX });
                        return out;
                    })()`);
                return { status: 'pass', response: r };
            }
            default: {
                if (op.startsWith('get_')) {
                    return { status: 'pass', response: { skipped: true, reason: `query "${op}" not implemented on web; recorded as no-op diagnostic` } };
                }
                return { status: 'skip', message: `unknown/unsupported op "${op}" on the web driver` };
            }
        }
    }
}
