// Read back every link in the Windows thread-local chain after a boot, so a
// failure says WHICH link broke instead of just "it still traps".
//
// The lifted sequence is:
//     r10 = [gs:0x58]         TEB.ThreadLocalStoragePointer  -> the slot array
//     eax = [rip+__tls_index] the module's slot number
//     rax = [r10 + rax*8]     that module's TLS block
//     ... = [rax + off]       the variable
//
// The wrapper seeds a TEB stub, a slot array, and State.gs_base. Two things it
// CANNOT know statically:
//
//   * whether the .data page holding __tls_index is in the mirror set at all.
//     Unmirrored reads 0, which happens to be the slot that is seeded, so an
//     unmirrored index is benign - but it must be distinguished from a mirrored
//     one that reads large, which would index past the seeded slots.
//   * whether the .rdata page holding the TLS template is mirrored. If it is
//     not, the block reads as zeros; that is still a workable initial state for
//     LocalKey (state 0 = Uninitialized) but silently drops any non-zero const
//     initializer.
//
// Reading memory needs no wasm call, so this works even after a boot trap -
// unlike anything that would call an export on a poisoned shadow stack.
//
// Usage: node --experimental-websocket tls-probe.js <url> [waitMs] [indexSynthHex]
const CDP = process.env.AZ_CDP || 'http://127.0.0.1:9222';
const URL = process.argv[2] || 'http://127.0.0.1:8801/';
const WAIT = parseInt(process.argv[3] || '25000', 10);
// synth of __tls_index, rva 0x123e0e8.
//
// Derived EMPIRICALLY, not from `synth_base + rva - 0x1000`: that formula
// predicts 0x1328de0 for the TLS template, but the lift's own one-shot log line
// reports synth=0x132a6e0 for template rva 0x1229de0 - a delta of 0x100900, not
// 0xff000. An image rebase is linear, so the same delta applies here:
//
//     0x123e0e8 + 0x100900 = 0x133e9e8
//
// If a future run logs a different template synth, recompute with that delta.
// Pass argv[4] to override.
const IDX = parseInt(process.argv[4] || '133e9e8', 16);

const TEB = 0x42000, TEB_SLOT = TEB + 0x58, ARRAY = 0x42200, SLOTS = 8;
const REC_NEVERLIFT = 0x40048;

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

const PROBE = `(() => {
  const caps = window.__azCaptured || [];
  const mini = caps.find(e => e && typeof e.AzStartup_hydrateJson === 'function');
  if (!mini || !mini.memory) return JSON.stringify({ error: 'no mini/memory captured' });
  const buf = mini.memory.buffer;
  const dv = new DataView(buf);
  const u32 = (a) => (a + 4 <= buf.byteLength) ? dv.getUint32(a, true) : null;
  const u64 = (a) => {
    if (a + 8 > buf.byteLength) return null;
    return (BigInt(dv.getUint32(a + 4, true)) << 32n) | BigInt(dv.getUint32(a, true));
  };
  const out = {
    bytes: buf.byteLength,
    tebSlot: String(u64(${TEB_SLOT})),
    index: u32(${IDX}),
    neverlift: u32(${REC_NEVERLIFT}),
    slots: [],
  };
  for (let i = 0; i < ${SLOTS}; i++) out.slots.push(String(u64(${ARRAY} + i * 8)));
  // Follow the chain the way lifted code does, using the index we just read.
  const idx = out.index;
  if (idx !== null) {
    const slotAddr = ${ARRAY} + idx * 8;
    out.chainSlotAddr = slotAddr;
    out.chainBlock = (slotAddr + 8 <= buf.byteLength) ? String(u64(slotAddr)) : null;
    if (out.chainBlock !== null) {
      const blk = Number(out.chainBlock);
      out.blockHead = [];
      for (let i = 0; i < 8; i++) out.blockHead.push(String(u64(blk + i * 8)));
      // offset 912 is the field azul_core::task::get_system_time_libstd reads.
      out.block912 = String(u64(blk + 912));
    }
  }
  return JSON.stringify(out);
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
    const r = await send('Runtime.evaluate', { expression: PROBE, returnByValue: true });
    const v = r && r.result && r.result.result && r.result.result.value;
    if (!v) { console.log('no result: ' + JSON.stringify(r && r.result)); ws.close(); return; }
    const o = JSON.parse(v);
    if (o.error) { console.log('ERROR: ' + o.error); ws.close(); return; }

    const hex = (s) => '0x' + BigInt(s).toString(16);
    console.log('memory        : ' + o.bytes + ' bytes');
    console.log('TEB+0x58      : ' + hex(o.tebSlot) + (BigInt(o.tebSlot) === BigInt(ARRAY)
        ? '   OK (points at the slot array)' : '   *** expected 0x' + ARRAY.toString(16)));
    console.log('__tls_index   : ' + o.index + (o.index === 0
        ? '   (slot 0 - either unmirrored or the loader index)'
        : (o.index < SLOTS ? '   (within the seeded slots)'
                           : '   *** BEYOND the ' + SLOTS + ' seeded slots')));
    console.log('slots[0..' + (SLOTS - 1) + ']  : ' + o.slots.map(hex).join(' '));
    if (o.chainBlock !== null && o.chainBlock !== undefined) {
        console.log('chain block   : ' + hex(o.chainBlock) + '  (via slot @0x' +
                    o.chainSlotAddr.toString(16) + ')');
        if (o.blockHead) console.log('block[0..7]   : ' + o.blockHead.map(hex).join(' '));
        if (o.block912) console.log('block+912     : ' + hex(o.block912) +
                                    '   (the field get_system_time_libstd reads)');
        const allZero = (o.blockHead || []).every(x => BigInt(x) === 0n);
        if (allZero) console.log('  note: block head is all zero - the .rdata template page');
        if (allZero) console.log('        is probably NOT mirrored; LocalKey still lazy-inits.');
    }
    console.log('NeverLift rec : ' + (o.neverlift === 0 ? '0  (no NeverLift stub reached)'
        : '0x' + (o.neverlift >>> 0).toString(16) + '  <- name it with name-synth.py'));
    ws.close();
})();
