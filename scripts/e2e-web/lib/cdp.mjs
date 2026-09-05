// CDP (Chrome DevTools Protocol) client over the global WebSocket.
//
// Part of the web e2e harness (scripts/web-e2e-harness-plan.md §4.1 "lib/cdp.js").
// Follows the proven raw-WebSocket patterns of scripts/cdp_click_hw.js and
// scripts/m9_e2e/cdp_screenshot.js: HTTP /json/* endpoints for tab lifecycle,
// one WebSocket per tab for commands + events, console/exception capture for
// the whole tab lifetime.
//
// Node 20 (C:/Users/felix/tools/node/node.exe = v20.18.1) only provides the
// global WebSocket behind --experimental-websocket; run.mjs re-execs itself
// with that flag when the global is missing, so this module can just assume it.

const DEFAULT_SEND_TIMEOUT_MS = 30_000;

/** GET <cdp>/json/version — returns the version object, or null if nothing answers. */
export async function checkBrowser(cdpBase, timeoutMs = 3000) {
    try {
        const ctl = new AbortController();
        const t = setTimeout(() => ctl.abort(), timeoutMs);
        const res = await fetch(`${cdpBase}/json/version`, { signal: ctl.signal });
        clearTimeout(t);
        if (!res.ok) return null;
        return await res.json();
    } catch {
        return null;
    }
}

/** PUT <cdp>/json/new?<url> — returns the tab descriptor {id, webSocketDebuggerUrl, ...}. */
export async function newTab(cdpBase, url = 'about:blank') {
    const res = await fetch(`${cdpBase}/json/new?${encodeURIComponent(url)}`, { method: 'PUT' });
    if (!res.ok) throw new Error(`CDP /json/new failed: HTTP ${res.status}`);
    const tab = await res.json();
    if (!tab.webSocketDebuggerUrl) throw new Error('CDP /json/new returned no webSocketDebuggerUrl');
    return tab;
}

/** GET <cdp>/json/close/<id> — best-effort. */
export async function closeTab(cdpBase, tabId) {
    try { await fetch(`${cdpBase}/json/close/${tabId}`); } catch { /* best-effort */ }
}

/** Format a Runtime.exceptionThrown params.exceptionDetails into one line with
 *  wasm-tagged frames (pattern: scripts/m9_e2e/cdp_screenshot.js:33-39). */
export function formatException(d) {
    const frames = ((d.stackTrace && d.stackTrace.callFrames) || [])
        .map(f => (f.functionName || '<anon>') + ((f.url || '').includes('.wasm') ? '@wasm' : ''))
        .slice(0, 8)
        .join(' <- ');
    const head = String((d.exception && d.exception.description) || d.text || 'exception')
        .split('\n')[0];
    return frames ? `${head}  ||  ${frames}` : head;
}

/**
 * One CDP session (= one tab's WebSocket).
 *
 * Captures for the whole connection lifetime:
 *   .console    — [{ts, kind, text}] from Runtime.consoleAPICalled + Log.entryAdded
 *   .exceptions — [{ts, text}] from Runtime.exceptionThrown (formatted, wasm frames tagged)
 */
export class Cdp {
    constructor(wsUrl) {
        this.wsUrl = wsUrl;
        this.ws = null;
        this.nextId = 1;
        this.pending = new Map();
        this.console = [];
        this.exceptions = [];
        this.eventHandlers = new Map(); // method -> [fn]
    }

    connect(timeoutMs = 10_000) {
        return new Promise((resolve, reject) => {
            const ws = new WebSocket(this.wsUrl);
            const to = setTimeout(() => reject(new Error('CDP WebSocket connect timeout')), timeoutMs);
            ws.onopen = () => { clearTimeout(to); this.ws = ws; resolve(); };
            ws.onerror = e => { clearTimeout(to); reject(new Error(`CDP WebSocket error: ${e && e.message || 'connect failed'}`)); };
            ws.onmessage = ev => this._onMessage(ev);
            ws.onclose = () => {
                for (const [, p] of this.pending) p.reject(new Error('CDP WebSocket closed'));
                this.pending.clear();
            };
        });
    }

    _onMessage(ev) {
        let m;
        try { m = JSON.parse(ev.data); } catch { return; }
        if (m.id && this.pending.has(m.id)) {
            const p = this.pending.get(m.id);
            this.pending.delete(m.id);
            clearTimeout(p.timer);
            if (m.error) p.reject(new Error(`CDP ${p.method}: ${JSON.stringify(m.error)}`));
            else p.resolve(m.result);
            return;
        }
        const ts = Date.now();
        if (m.method === 'Runtime.consoleAPICalled') {
            const text = (m.params.args || [])
                .map(a => (a.value !== undefined ? String(a.value) : String(a.description || a.type)))
                .join(' ');
            this.console.push({ ts, kind: m.params.type || 'log', text });
        } else if (m.method === 'Log.entryAdded') {
            const e = m.params.entry || {};
            this.console.push({ ts, kind: `browser-${e.level || 'log'}`, text: `${e.source || ''}: ${e.text || ''}` });
        } else if (m.method === 'Runtime.exceptionThrown') {
            this.exceptions.push({ ts, text: formatException(m.params.exceptionDetails || {}) });
        }
        const hs = this.eventHandlers.get(m.method);
        if (hs) for (const h of hs) { try { h(m.params); } catch { /* handler errors are not ours */ } }
    }

    on(method, fn) {
        if (!this.eventHandlers.has(method)) this.eventHandlers.set(method, []);
        this.eventHandlers.get(method).push(fn);
    }

    send(method, params = {}, timeoutMs = DEFAULT_SEND_TIMEOUT_MS) {
        return new Promise((resolve, reject) => {
            if (!this.ws || this.ws.readyState !== 1) return reject(new Error(`CDP not connected (send ${method})`));
            const id = this.nextId++;
            const timer = setTimeout(() => {
                this.pending.delete(id);
                reject(new Error(`CDP ${method}: no reply after ${timeoutMs} ms`));
            }, timeoutMs);
            this.pending.set(id, { resolve, reject, timer, method });
            this.ws.send(JSON.stringify({ id, method, params }));
        });
    }

    /**
     * Runtime.evaluate wrapper. Returns the by-value result.
     * Throws if the expression itself threw (message includes the page-side error).
     */
    async eval(expression, { awaitPromise = false, timeoutMs = DEFAULT_SEND_TIMEOUT_MS } = {}) {
        const r = await this.send('Runtime.evaluate', {
            expression,
            returnByValue: true,
            awaitPromise,
        }, timeoutMs);
        if (r.exceptionDetails) {
            throw new Error(`page evaluate threw: ${formatException(r.exceptionDetails)}`);
        }
        return r.result ? r.result.value : undefined;
    }

    /** Page.captureScreenshot → PNG Buffer. */
    async screenshot(timeoutMs = 20_000) {
        const r = await this.send('Page.captureScreenshot', { format: 'png' }, timeoutMs);
        if (!r || !r.data) throw new Error('Page.captureScreenshot returned no data');
        return Buffer.from(r.data, 'base64');
    }

    close() {
        try { if (this.ws) this.ws.close(); } catch { /* already closed */ }
    }
}

/** The relaunch recipe printed when nothing answers on :9222 (from scripts/m9_e2e/cdp_gate.sh:43-52). */
export function browserRecipe(cdpBase) {
    return [
        `No browser answering on ${cdpBase} — start a headless Edge/Chrome with:`,
        '',
        '  "C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe" ^',
        '      --headless=new --remote-debugging-port=9222 --disable-gpu ^',
        '      --no-first-run --user-data-dir=%TEMP%\\az-edge-headless about:blank',
        '',
        '(bash: see the relaunch block in scripts/m9_e2e/cdp_gate.sh:43-52)',
    ].join('\n');
}
