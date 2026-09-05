// Isolate AzStartup_hydrateJson: is the U8Vec::drop trap data-dependent?
//
// Usage: node --experimental-websocket scripts/m9_e2e/hydrate-isolate.js <url> [waitMs]
// Requires a Chromium/Edge on :9222 (--headless=new --remote-debugging-port=9222).
//
// The boot traps in <U8Vec as Drop>::drop, reached from the app's registered
// deserializer, on a destructor pointer that is not a code address. `json` is
// dropped on EVERY path out of that deserializer - including the early return
// when serde rejects the input - so calling hydrateJson again with input that
// cannot possibly deserialize separates two very different faults:
//
//   traps on `{}` too  -> the drop is broken unconditionally. The U8Vec that
//                         `Json::parse_bytes` builds always carries a bad
//                         destructor, and this is an engine/lift defect.
//   `{}` returns 0     -> the drop is fine in general and the fault is
//                         data-dependent, somewhere in the real payload.
//
// wasm traps do not poison the instance, so this runs fine after the boot trap.
// azMini / azState / azMemory are top-level `var`s in the loader, hence on window.
const CDP = process.env.AZ_CDP || 'http://127.0.0.1:9222';
const URL = process.argv[2] || 'http://127.0.0.1:8801/';
const WAIT = parseInt(process.argv[3] || '25000', 10);

const PROBE = `(() => {
  const out = [];
  const log = (s) => out.push(String(s));
  if (typeof azMini !== 'object' || !azMini) return 'FATAL: azMini not present';
  if (!azState) return 'FATAL: azState is 0 (init never ran)';
  log('azState=' + azState);
  const deserFn = (window.__azDeserFn || 0);
  log('deserFn=' + deserFn + ' (0x' + deserFn.toString(16) + ')');

  const call = (label, text) => {
    let ptr = 0;
    try {
      const bytes = new TextEncoder().encode(text);
      ptr = azMini.AzStartup_alloc(bytes.length);
      if (!ptr) { log(label + ': alloc FAILED'); return; }
      new Uint8Array(azMemory.buffer, ptr, bytes.length).set(bytes);
      if (deserFn) azMini.AzStartup_registerStateDeserializer(azState, BigInt(deserFn));
      const r = azMini.AzStartup_hydrateJson(azState, ptr, bytes.length);
      log(label + ': returned ' + r + (r ? '  (OK)' : '  (0 = failed, but NO TRAP)'));
    } catch (e) {
      log(label + ': TRAPPED -> ' + (e && e.message ? e.message : e));
    }
  };

  // 1. Input serde cannot accept. Reaches the early return, which drops \`json\`.
  call('empty-object {}', '{}');
  // 2. Not even valid JSON - fails earlier, in Json::parse_bytes, before the
  //    deserializer is called at all. Isolates parse_bytes from the drop.
  call('malformed  <<<',  '<<<not json>>>');
  // 3. A plausible-shaped payload, to see whether shape matters.
  call('plausible',       JSON.stringify({ counter: 0, markdown: '', zoom_percent: 100.0 }));
  return out.join('\\n');
})()`;

(async () => {
    const tab = await (await fetch(`${CDP}/json/new?${encodeURIComponent(URL)}`, { method: 'PUT' })).json();
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
    await new Promise(r => setTimeout(r, WAIT));

    // The deserializer address is in the page payload; surface it for the probe.
    await send('Runtime.evaluate', {
        expression: `window.__azDeserFn = (function(){
            try { const m = document.documentElement.innerHTML.match(/"deserialize_fn"\\s*:\\s*(\\d+)/);
                  return m ? parseInt(m[1],10) : 0; } catch(e) { return 0; }
        })()`, returnByValue: true,
    });

    const r = await send('Runtime.evaluate', { expression: PROBE, returnByValue: true, awaitPromise: false });
    console.log('===== BOOT CONSOLE (last 12) =====');
    logs.slice(-12).forEach(l => l.split('\n').forEach((x, i) => console.log((i ? '      ' : '  ') + x)));
    console.log('===== HYDRATE ISOLATION =====');
    const v = r && r.result && r.result.result;
    if (v && v.value !== undefined) {
        String(v.value).split('\n').forEach(l => console.log('  ' + l));
    } else {
        console.log('  <no value> ' + JSON.stringify(r && r.result));
    }
    ws.close();
})();
