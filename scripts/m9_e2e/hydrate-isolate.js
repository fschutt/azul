// Isolate AzStartup_hydrateJson: is the U8Vec::drop trap data-dependent?
//
// Usage: node --experimental-websocket hydrate-isolate.js <url> [waitMs] [json]
//   json defaults to '{}'. Run it once per input.
// Requires a Chromium/Edge on :9222 (--headless=new --remote-debugging-port=9222).
//
// The boot traps in <U8Vec as Drop>::drop, reached from the app's registered
// deserializer, on a destructor pointer that is not a code address. `json` is
// dropped on EVERY path out of that deserializer - including the early return
// when serde rejects the input - so running hydrateJson with input that cannot
// possibly deserialize separates two very different faults:
//
//   traps on `{}` too  -> the drop is broken unconditionally. The U8Vec that
//                         Json::parse_bytes builds always carries a bad
//                         destructor: an engine/lift defect.
//   `{}` returns 0     -> the drop is fine in general and the fault is
//                         data-dependent, inside the real payload.
//
// ONE PROBE PER PAGE LOAD, and that is not a style choice. A wasm export that
// traps never restores the shadow-stack global, so the stack pointer is left
// inside a spent frame and every later export dies with "memory access out of
// bounds" - measuring the poisoned stack instead of the bug. Probing after the
// boot trap gave exactly that. So the only clean-stack call available is the
// loader's FIRST hydrateJson, and this substitutes its payload rather than
// adding a call after it.
//
// The substitution works by returning a Proxy over the mini's exports from a
// hooked WebAssembly.instantiateStreaming: `instance.exports` is sealed and
// cannot be monkey-patched, but the loader only ever sees what the hook
// returns. `window.__azProbe` is published at the END of azBootstrap and so is
// unavailable whenever the boot fails - which is always, here.
const CDP = process.env.AZ_CDP || 'http://127.0.0.1:9222';
const URL = process.argv[2] || 'http://127.0.0.1:8801/';
const WAIT = parseInt(process.argv[3] || '25000', 10);
const JSON_TEXT = process.argv[4] || '{}';

const HOOK = `(() => {
  window.__azResult = null;
  const PAYLOAD = ${JSON.stringify(JSON_TEXT)};
  // A PLAIN OBJECT COPY, not a Proxy: instance.exports properties are
  // non-writable AND non-configurable, so a Proxy get-trap is required by spec
  // to return the real value and cannot substitute. The loader only does
  // property access on whatever the hook returns, so a copy works.
  const wrap = (exports) => {
    const o = {};
    for (const k of Object.keys(exports)) o[k] = exports[k];
    const real = exports.AzStartup_hydrateJson;
    o.AzStartup_hydrateJson = function (state, ptr, len) {
      if (window.__azResult) return real(state, ptr, len);
      const rec = { payload: PAYLOAD, state: state, loaderLen: len };
      try {
        const bytes = new TextEncoder().encode(PAYLOAD);
        const p = exports.AzStartup_alloc(bytes.length);
        rec.alloc = p;
        if (!p) { rec.outcome = 'alloc returned 0'; window.__azResult = rec; return 0; }
        new Uint8Array(exports.memory.buffer, p, bytes.length).set(bytes);
        const r = real(state, p, bytes.length);
        rec.outcome = 'RETURNED ' + r + (r ? ' (OK)' : ' (0 = failed cleanly, NO TRAP)');
      } catch (e) {
        rec.outcome = 'TRAPPED -> ' + (e && e.message ? e.message : String(e));
      }
      window.__azResult = rec;
      return 0;
    };
    return o;
  };
  const keep = (res) => {
    try {
      const inst = res && (res.instance || res);
      if (inst && inst.exports && typeof inst.exports.AzStartup_hydrateJson === 'function') {
        return { instance: { exports: wrap(inst.exports) }, module: res.module };
      }
    } catch (e) {}
    return res;
  };
  const origS = WebAssembly.instantiateStreaming;
  if (origS) {
    WebAssembly.instantiateStreaming = function (...a) { return origS.apply(this, a).then(keep); };
  }
  const orig = WebAssembly.instantiate;
  WebAssembly.instantiate = function (...a) {
    const r = orig.apply(this, a);
    return (r && typeof r.then === 'function') ? r.then(keep) : keep(r);
  };
})()`;

(async () => {
    const tab = await (await fetch(`${CDP}/json/new?about:blank`, { method: 'PUT' })).json();
    const ws = new WebSocket(tab.webSocketDebuggerUrl);
    await new Promise((res, rej) => { ws.onopen = res; ws.onerror = rej; setTimeout(rej, 10000); });
    let id = 0;
    const pend = new Map();
    const logs = [];
    ws.onmessage = e => {
        const m = JSON.parse(e.data);
        if (m.id && pend.has(m.id)) { pend.get(m.id)(m); pend.delete(m.id); }
        if (m.method === 'Runtime.consoleAPICalled') {
            logs.push((m.params.args || [])
                .map(a => (a.value !== undefined ? String(a.value) : String(a.description || a.type)))
                .join(' '));
        }
    };
    const send = (method, params) => {
        const i = ++id;
        ws.send(JSON.stringify({ id: i, method, params }));
        return new Promise(r => pend.set(i, r));
    };
    await send('Runtime.enable', {});
    await send('Page.enable', {});
    await send('Page.addScriptToEvaluateOnNewDocument', { source: HOOK });
    await send('Page.navigate', { url: URL });
    await new Promise(r => setTimeout(r, WAIT));

    const r = await send('Runtime.evaluate', {
        expression: 'JSON.stringify(window.__azResult)', returnByValue: true,
    });
    console.log('input: ' + JSON_TEXT);
    const unmatched = logs.filter(l => l.includes('unmatched indirect dispatches')).slice(-1)[0];
    const failed = logs.filter(l => l.includes('bootstrap FAILED')).slice(-1)[0];
    console.log('===== RESULT =====');
    const v = r && r.result && r.result.result && r.result.result.value;
    if (v && v !== 'null') {
        const o = JSON.parse(v);
        console.log('  alloc   : ' + o.alloc);
        console.log('  outcome : ' + o.outcome);
    } else {
        console.log('  <hydrateJson was never called - the loader skipped the JSON path>');
    }
    if (unmatched) console.log('  ' + unmatched.trim());
    if (failed) console.log('  boot: ' + failed.split('\n')[0].trim());
    ws.close();
})();
