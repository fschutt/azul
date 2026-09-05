// Identify the guest registers at the first unmatched dispatch, empirically.
//
// Usage: node --experimental-websocket state-regs.js <url> [waitMs]
//
// The dispatcher records the STATE POINTER at 0x40A00 on the first miss (and
// the missed PC at 0x409B0, the ring at 0x409C0). This reads both back, dumps
// the X86State register file, and labels it.
//
// The offsets below are VERIFIED against remill's State.h (see the table further
// down), not extrapolated - an earlier guess from lifted-IR GEP indices put RIP
// at 2472 instead of 2480 and made RCX and RIP read the same value, which
// contradicted the `movq %rsi,%rcx` preceding the failing call.
//
// Two independently-known anchors are still checked at runtime, because a
// verified table is worth nothing if the recorded pointer is wrong:
//
//   RIP  == the recorded missed PC (0x409B0)
//   RSP  ∈ the guest stack band (~0x2f000..0x30000)
//
// Slots matching an anchor are flagged, and every non-zero slot is printed so
// the file can be read off directly.
const CDP = process.env.AZ_CDP || 'http://127.0.0.1:9222';
const URL = process.argv[2] || 'http://127.0.0.1:8801/';
const WAIT = parseInt(process.argv[3] || '25000', 10);

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

const READ = `(() => {
  const caps = window.__azCaptured || [];
  const mini = caps.find(e => e && typeof e.AzStartup_hydrateJson === 'function');
  if (!mini || !mini.memory) return JSON.stringify({ error: 'no mini/memory captured' });
  const buf = mini.memory.buffer;
  const u32 = new Uint32Array(buf);
  const rd32 = (a) => u32[a >>> 2];
  const out = {
    count:    rd32(262488),   // 0x40158 miss count
    firstPC:  rd32(264624),   // 0x409B0 first missed PC
    lastPC:   rd32(264448),   // 0x40900 last missed PC
    statePtr: rd32(264704),   // 0x40A00 state pointer (new)
    ring:     [],
    slots:    [],
  };
  for (let i = 0; i < 16; i++) out.ring.push(rd32(264640 + i * 4));
  if (out.statePtr) {
    // 2100..2500 covers the GP register file with room either side.
    for (let off = 2100; off < 2500; off += 4) {
      const a = out.statePtr + off;
      if (a + 8 > buf.byteLength) break;
      out.slots.push([off, rd32(a), rd32(a + 4)]);
    }
  }
  return JSON.stringify(out);
})()`;

// CONFIRMED layout, read off remill's include/remill/Arch/X86/Runtime/State.h.
// `struct alignas(8) GPR` interleaves an 8-byte padding field before every
// register ("Prevents LLVM from casting a GPR into an i64"), so the stride is
// 16, not 8:
//     _0, rax, _1, rbx, _2, rcx, _3, rdx, _4, rsi, _5, rdi,
//     _6, rsp, _7, rbp, _8, r8 ... _15, r15, _16, rip
// Anchoring rax at pcs::RET (2216) puts the GPR base at 2208, which reproduces
// ALL FOUR pcs::ARG values exactly (rcx 2248, rdx 2264, r8 2344, r9 2360) - so
// this is verified, not extrapolated.
//
// Note RIP is 2480, NOT 2472: an earlier uniform-8-bytes-per-GEP-index guess
// missed the extra `_16` field sitting between r15 and rip, and every register
// conclusion drawn from that mapping was wrong.
const GPR = 2208;
const KNOWN = {
    [GPR + 8]: 'RAX', [GPR + 24]: 'RBX', [GPR + 40]: 'RCX', [GPR + 56]: 'RDX',
    [GPR + 72]: 'RSI', [GPR + 88]: 'RDI', [GPR + 104]: 'RSP', [GPR + 120]: 'RBP',
    [GPR + 136]: 'R8', [GPR + 152]: 'R9', [GPR + 168]: 'R10', [GPR + 184]: 'R11',
    [GPR + 200]: 'R12', [GPR + 216]: 'R13', [GPR + 232]: 'R14', [GPR + 248]: 'R15',
    [GPR + 272]: 'RIP',
};

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
    const r = await send('Runtime.evaluate', { expression: READ, returnByValue: true });
    const v = r && r.result && r.result.result && r.result.result.value;
    if (!v) { console.log('no result: ' + JSON.stringify(r && r.result)); ws.close(); return; }
    const o = JSON.parse(v);
    if (o.error) { console.log('ERROR: ' + o.error); ws.close(); return; }

    console.log('unmatched dispatches : ' + o.count);
    console.log('first missed PC      : 0x' + (o.firstPC >>> 0).toString(16));
    console.log('last  missed PC      : 0x' + (o.lastPC >>> 0).toString(16));
    console.log('state pointer        : 0x' + (o.statePtr >>> 0).toString(16));
    console.log('ring                 : ' + o.ring.filter(x => x).map(x => '0x' + (x >>> 0).toString(16)).join(' '));
    if (!o.statePtr) {
        console.log('');
        console.log('state pointer is 0 - either no miss happened, or this build predates');
        console.log('the recorder (it is written only on the FIRST miss).');
        ws.close();
        return;
    }
    console.log('');
    console.log('X86State register file (anchors: RIP == first missed PC, RSP in 0x2f000..0x31000)');
    for (const [off, lo, hi] of o.slots) {
        if (off % 8 !== 0) continue;
        const isReg = KNOWN[off] !== undefined;
        const q = (BigInt(hi >>> 0) << 32n) | BigInt(lo >>> 0);
        if (q === 0n && !isReg) continue;
        let tag = KNOWN[off] ? ('  ' + KNOWN[off] + ' (known)') : '';
        if ((lo >>> 0) === (o.firstPC >>> 0) && hi === 0) tag += '  <== RIP CONFIRMED (== missed PC)';
        const n = Number(q);
        if (hi === 0 && n >= 0x2f000 && n <= 0x31000) tag += '  <== stack-band value (RSP/RBP?)';
        if (hi === 0 && n >= 0x0a000000 && n <= 0x0b000000) tag += '  <== bump-heap pointer';
        console.log('  +' + String(off).padStart(4) + '  0x' + q.toString(16).padStart(16, '0') + tag);
    }
    ws.close();
})();
