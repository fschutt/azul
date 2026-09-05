// Dump wasm linear memory at a synth address, to compare the MIRROR against
// the PE file it was built from.
//
// Usage: node --experimental-websocket mem-dump.js <url> <synthHex> [count] [waitMs]
//   e.g. mem-dump.js http://127.0.0.1:8801/ 12f5f90 16
//
// U8Vec::drop selects its destructor arm through a .rdata jump table of int32
// offsets. That table was ruled out as the source of the bad dispatch address
// by reading it OUT OF THE PE FILE - but the lifted code reads it from the
// mirrored copy in linear memory, so the elimination only holds if the mirror
// matches the file. This prints the mirrored words so the two can be compared
// directly; a mirror that differs would mean the data mirror, not the lifter,
// produced the wrong branch target.
//
// Reading memory needs no wasm call, so this works after the boot trap.
const CDP = process.env.AZ_CDP || 'http://127.0.0.1:9222';
const URL = process.argv[2] || 'http://127.0.0.1:8801/';
const ADDR = parseInt(process.argv[3] || '12f5f90', 16);
const COUNT = parseInt(process.argv[4] || '16', 10);
const WAIT = parseInt(process.argv[5] || '25000', 10);

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

const DUMP = `(() => {
  const caps = window.__azCaptured || [];
  const mini = caps.find(e => e && typeof e.AzStartup_hydrateJson === 'function');
  if (!mini || !mini.memory) return JSON.stringify({ error: 'no mini/memory captured' });
  const buf = mini.memory.buffer;
  const base = ${ADDR}, n = ${COUNT};
  if (base + n * 4 > buf.byteLength) return JSON.stringify({ error: 'address beyond memory' });
  const i32 = new Int32Array(buf, base, n);
  return JSON.stringify({ base: base, words: Array.from(i32) });
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
    const r = await send('Runtime.evaluate', { expression: DUMP, returnByValue: true });
    const v = r && r.result && r.result.result && r.result.result.value;
    if (!v) { console.log('no result: ' + JSON.stringify(r && r.result)); ws.close(); return; }
    const o = JSON.parse(v);
    if (o.error) { console.log('ERROR: ' + o.error); ws.close(); return; }
    console.log('mirror @0x' + o.base.toString(16) + ':');
    o.words.forEach((w, i) => {
        const target = (o.base + w) >>> 0;
        console.log('  [' + String(i).padStart(2) + '] ' + String(w).padStart(12) +
                    '   base+w = 0x' + target.toString(16));
    });
    ws.close();
})();
