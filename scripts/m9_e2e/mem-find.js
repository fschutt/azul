// Find a value in the wasm linear memory and dump what surrounds it.
//
// Usage: node --experimental-websocket mem-find.js <url> <hexvalue> [waitMs]
//   e.g. mem-find.js http://127.0.0.1:8801/ 874180
//
// The boot traps on an unmatched indirect dispatch to a CONSTANT address - the
// same value for `{}`, `123`, `"hi"` and `null`, so it is not recycled heap
// (that would vary) but a deterministic value read from a fixed place. The
// question is what structure holds it and at what offset: a value sitting at
// +0x20 of a U8Vec-shaped record means the destructor field really was written
// wrong, whereas the same value at some other offset means the DROP was handed
// the wrong pointer and is reading a neighbouring field.
//
// Reading linear memory needs no wasm call, so - unlike calling an export -
// this works fine after the boot trap has poisoned the shadow stack.
//
// The scan runs IN the page over a Uint32Array view and returns only hits;
// shipping 512 MB over CDP would not work.
const CDP = process.env.AZ_CDP || 'http://127.0.0.1:9222';
const URL = process.argv[2] || 'http://127.0.0.1:8801/';
const VALUE = parseInt(process.argv[3] || '874180', 16);
const WAIT = parseInt(process.argv[4] || '25000', 10);

const HOOK = `(() => {
  window.__azCaptured = [];
  const keep = (res) => {
    try {
      const inst = res && (res.instance || res);
      if (inst && inst.exports) window.__azCaptured.push(inst.exports);
    } catch (e) {}
    return res;
  };
  const origS = WebAssembly.instantiateStreaming;
  if (origS) WebAssembly.instantiateStreaming = function (...a) { return origS.apply(this, a).then(keep); };
  const orig = WebAssembly.instantiate;
  WebAssembly.instantiate = function (...a) {
    const r = orig.apply(this, a);
    return (r && typeof r.then === 'function') ? r.then(keep) : keep(r);
  };
})()`;

const SCAN = (value) => `(() => {
  const caps = window.__azCaptured || [];
  const mini = caps.find(e => e && typeof e.AzStartup_hydrateJson === 'function');
  if (!mini || !mini.memory) return JSON.stringify({ error: 'no mini/memory captured' });
  const buf = mini.memory.buffer;
  const u32 = new Uint32Array(buf);
  const TARGET = ${value};
  const hits = [];
  for (let i = 0; i + 1 < u32.length && hits.length < 24; i++) {
    if (u32[i] !== TARGET) continue;
    // count as a 64-bit occurrence when the high half is zero
    const wide = (u32[i + 1] === 0);
    const byteOff = i * 4;
    const ctx = [];
    for (let j = -6; j <= 6; j++) {
      const k = i + j * 2;
      if (k < 0 || k + 1 >= u32.length) { ctx.push(null); continue; }
      ctx.push([(j * 8), u32[k], u32[k + 1]]);
    }
    hits.push({ off: byteOff, wide: wide, ctx: ctx });
  }
  return JSON.stringify({ bytes: buf.byteLength, hits: hits });
})()`;

(async () => {
    const tab = await (await fetch(`${CDP}/json/new?about:blank`, { method: 'PUT' })).json();
    const ws = new WebSocket(tab.webSocketDebuggerUrl);
    await new Promise((res, rej) => { ws.onopen = res; ws.onerror = rej; setTimeout(rej, 10000); });
    let id = 0;
    const pend = new Map();
    ws.onmessage = e => {
        const m = JSON.parse(e.data);
        if (m.id && pend.has(m.id)) { pend.get(m.id)(m); pend.delete(m.id); }
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

    const r = await send('Runtime.evaluate', { expression: SCAN(VALUE), returnByValue: true });
    const v = r && r.result && r.result.result && r.result.result.value;
    if (!v) { console.log('no result: ' + JSON.stringify(r && r.result)); ws.close(); return; }
    const o = JSON.parse(v);
    if (o.error) { console.log('ERROR: ' + o.error); ws.close(); return; }
    console.log('linear memory: ' + (o.bytes / 1048576).toFixed(1) + ' MB');
    console.log('hits for 0x' + VALUE.toString(16) + ': ' + o.hits.length +
                (o.hits.length >= 24 ? ' (capped)' : ''));
    for (const h of o.hits) {
        console.log('');
        console.log('  @0x' + h.off.toString(16) + (h.wide ? '  (64-bit, high half zero)' : '  (32-bit only)'));
        for (const c of h.ctx) {
            if (!c) continue;
            const [rel, lo, hi] = c;
            const mark = rel === 0 ? '  <== ' : '      ';
            const q = (BigInt(hi) << 32n) | BigInt(lo >>> 0);
            console.log('   ' + mark + (rel < 0 ? '-' : '+') + '0x' +
                        Math.abs(rel).toString(16).padStart(2, '0') +
                        '  0x' + q.toString(16).padStart(16, '0'));
        }
    }
    ws.close();
})();
