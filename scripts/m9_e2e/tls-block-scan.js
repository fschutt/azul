// Scan the synthetic TLS block for non-zero bytes, and specifically for the
// value 2.
//
// Rust's lazy thread-local reads a state byte and calls panic_access_error when
// it equals 2 (State::Destroyed); the lifted IR shows exactly that shape -
// `read8(block + N)` then `cmp eax, 2` / `jne past_the_panic`. The seeded chain
// resolves (gs:[0x58] -> slot array -> __tls_index=0 -> block), and the block
// should therefore read all zeros = State::Initial = lazily initialize.
//
// If it does read all zeros, then whichever thread-local panicked is NOT
// reading through this block, and the seed is not the remaining problem. If a 2
// is present, this says where.
//
// Reads memory only, so it works after a boot trap.
//
// Usage: node --experimental-websocket tls-block-scan.js <url> [waitMs] [blockHex] [len]
const CDP = process.env.AZ_CDP || 'http://127.0.0.1:9222';
const URL = process.argv[2] || 'http://127.0.0.1:8801/';
const WAIT = parseInt(process.argv[3] || '25000', 10);
const BLOCK = parseInt(process.argv[4] || '132ace0', 16);
const LEN = parseInt(process.argv[5] || '1177', 10);

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

const SCAN = `(() => {
  const caps = window.__azCaptured || [];
  const mini = caps.find(e => e && typeof e.AzStartup_hydrateJson === 'function');
  if (!mini || !mini.memory) return JSON.stringify({ error: 'no mini/memory captured' });
  const buf = mini.memory.buffer;
  if (${BLOCK} + ${LEN} > buf.byteLength) return JSON.stringify({ error: 'block beyond memory' });
  const u8 = new Uint8Array(buf, ${BLOCK}, ${LEN});
  const nz = [], twos = [];
  for (let i = 0; i < u8.length; i++) {
    if (u8[i] !== 0) nz.push([i, u8[i]]);
    if (u8[i] === 2) twos.push(i);
  }
  return JSON.stringify({ bytes: buf.byteLength, nz: nz.slice(0, 64), nzCount: nz.length, twos: twos.slice(0, 32), twoCount: twos.length });
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
    const r = await send('Runtime.evaluate', { expression: SCAN, returnByValue: true });
    const v = r && r.result && r.result.result && r.result.result.value;
    if (!v) { console.log('no result: ' + JSON.stringify(r && r.result)); ws.close(); return; }
    const o = JSON.parse(v);
    if (o.error) { console.log('ERROR: ' + o.error); ws.close(); return; }
    console.log('block 0x' + BLOCK.toString(16) + ' len ' + LEN + '  (memory ' + o.bytes + ' B)');
    console.log('non-zero bytes : ' + o.nzCount);
    console.log('bytes equal 2  : ' + o.twoCount + (o.twoCount ? '   <-- State::Destroyed' : '   (no Destroyed state here)'));
    if (o.nzCount) {
        console.log('first non-zero offsets:');
        for (const [off, val] of o.nz) console.log('  +0x' + off.toString(16).padStart(3, '0') + ' = ' + val);
    }
    if (o.twoCount) console.log('offsets holding 2: ' + o.twos.map(x => '+0x' + x.toString(16)).join(' '));
    ws.close();
})();
