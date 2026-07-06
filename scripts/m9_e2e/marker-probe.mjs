// marker-probe.mjs — read the lifted solver's progress markers LIVE while the
// synchronous layout solve hangs.
//
// WHY: scripts/m9_e2e/full-cycle.js runs AzStartup_solveLayoutReal synchronously
// in node; on the SwissTable hang it never returns, so the fixed-address markers
// it writes (eventloop.rs: DIAG 0x40700+, PROBE0 0x40870, solve 0x40578+) can't
// be read post-return. Here the solve runs in a Worker on a SHARED wasm memory;
// the main thread polls the markers from the SharedArrayBuffer during the hang.
//
// STAGE=1 (default): mini.wasm only, NO hydrate. solveLayoutReal runs its DIAG
//   block (BTreeMap/Vec/SSE/HashMap<u32,u32>, 0x40700-0x4072C) UNCONDITIONALLY
//   before the current_dom_styled_ptr==0 -> return 2 check, so this isolates the
//   SSE primitives the hashbrown probe uses with a minimal harness.
// STAGE=2: full chain (mini + layout, initLayoutCache + hydrate) so PROBE0
//   (HashMap<String,u32> insert, 0x40870) and the real solve run too.
//
// Usage: AZ_PORT=8800 STAGE=2 HANG_MS=12000 node scripts/m9_e2e/marker-probe.mjs
import { Worker, isMainThread, parentPort, workerData } from 'worker_threads';
import http from 'http';
import { patchSharedMemory } from './wasm-share-mem.mjs';

const AZ_PORT = process.env.AZ_PORT || '8800';
const STAGE = parseInt(process.env.STAGE || '2', 10);
const HANG_MS = parseInt(process.env.HANG_MS || '12000', 10);

function fetch_(p) {
  return new Promise((r, j) => http.get('http://127.0.0.1:' + AZ_PORT + p, x => {
    const c = []; x.on('data', b => c.push(b)); x.on('end', () => r(Buffer.concat(c))); x.on('error', j);
  }).on('error', j));
}

// ----- marker map (addr -> {name, want, fmt}) -----
const HEX = v => '0x' + (v >>> 0).toString(16);
const MARKERS = [
  // [AZ-DIAG REVERT] load_fonts_from_disk root-cause (getters.rs:4302): the garbage failed.len OOB
  [0x607B0, 'load_fonts iters', '-', 'hex'],
  [0x607B4, 'load_fonts none(no-bytes)', '-', 'hex'],
  [0x607B8, 'load_fonts ok', '-', 'hex'],
  [0x607BC, 'load_fonts err', '-', 'hex'],
  [0x607C0, 'failed.len INSIDE load_fonts', 0, 'hex'],
  [0x607C4, 'failed.cap', '-', 'hex'],
  [0x607C8, 'failed.ptr lo32', '-', 'hex'],
  [0x607CC, 'failed.len AT load_missing ret', 0, 'hex'],
  [0x607D0, 'failed.ptr AT load_missing ret', '-', 'hex'],
  [0x607D4, 'failed.len IN window.rs (drop)', 0, 'hex'],
  [0x607D8, 'failed.ptr IN window.rs (drop)', '-', 'hex'],
  [0x607DC, 'guest RSP before load_missing', '-', 'hex'],
  [0x607E0, 'guest RSP after  load_missing', '-', 'hex'],
  [0x607E4, '&failed (caller read addr)', '-', 'hex'],
  // DIAG block (eventloop.rs ~1506): sentinel 0xDEAD_000N if the slot was never
  // reached. These run FIRST, unconditionally.
  [0x40700, 'BTreeMap<u32,u32>.len', 2, 'dec'],
  [0x40704, 'BTm.sum', 161, 'dec'],
  [0x40708, 'BTreeMap<String,u32>.len', 2, 'dec'],
  [0x4070C, 'BTmStr.sum', 20, 'dec'],
  [0x40710, 'Vec<String>.len', 2, 'dec'],
  [0x40714, 'Vec.sum', 5, 'dec'],
  [0x40718, 'SSE movemask(all_ff)', 0xffff, 'hex'],
  [0x4071C, 'SSE pcmpeqb mask', 0x15, 'hex'],
  [0x40720, 'SSE set1(0x80) movemask', 0xffff, 'hex'],
  [0x40724, 'SSE memset(0xFF)+movdqu+movemask', 0xffff, 'hex'],
  [0x40728, 'SSE memset byte20', 0xff, 'hex'],
  [0x4072C, 'HashMap<u32,u32>.len (h6)', 2, 'dec'],
  // css cache / font parse
  [0x40578, 'css compact_cache tag', 0xCC5E0000, 'hex'],
  [0x40670, 'ParsedFont tag', 0x600D0001, 'hex'],
  [0x40674, 'font units_per_em', null, 'dec'],
  // PROBE0: HashMap<String,u32> real insert (only runs in STAGE 2, post-hydrate)
  [0x40870, 'PROBE0 step', 0xA0A00003, 'hex'],
  [0x40874, 'PROBE0 hm.len', 2, 'dec'],
  [0x40878, 'PROBE0 get(serif)', 1, 'dec'],
  // solve / font-resolution markers
  [0x40690, 'resolveChain', null, 'hex'],
  [0x40830, 'find_unicode marker', null, 'hex'],
  // coverage-loop sub-markers (vendored rust-fontconfig find_unicode_fallbacks)
  [0x40838, 'cov step (ci<<8)|sub', null, 'hex'],
  [0x4083C, 'cov candidates.len', null, 'dec'],
  [0x40840, 'cov uncovered.len', null, 'dec'],
  [0x40844, 'cov candidate.ur.len', null, 'dec'],
  [0x40848, 'cov candidate.ur.ptr lo32', null, 'hex'],
  [0x4084C, 'cov candidate.fallbacks.len', null, 'dec'],
  [0x40858, 'cov candidate.fallbacks.ptr', null, 'hex'],
  // resolve_font_chain post-find_unicode flow (2d-post): CC000001=fuf ret;
  // 02/03=LAST-RESORT; 04=before chain build; CC000010=uncached ret; FF=insert skipped
  [0x40850, '2d-post step', null, 'hex'],
  [0x40854, 'chain.unicode_fallbacks.len', null, 'dec'],
  [0x4085C, 'chain.css_fallbacks.len', null, 'dec'],
  // PROBE8 Vec<FontMatch>::clone (C8C80001 trap / C8C80003 ok)
  [0x40860, 'PROBE8 clone step', null, 'hex'],
  // layout_document viewport struct-arg read (800.0=0x44480000, 600.0=0x44160000)
  [0x40900, 'layout_document viewport.width(f32 bits)', 0x44480000, 'hex'],
  [0x40904, 'layout_document viewport.height(f32 bits)', 0x44160000, 'hex'],
  [0x40908, 'layout_document viewport.origin.x', 0, 'hex'],
  [0x4090C, 'layout_document viewport.origin.y', 0, 'hex'],
  // viewport-loss chain (800.0=0x44480000): SOURCE(solveLayoutReal ws) → impl-construct → impl-pre-call → layout_document
  [0x40910, 'CHAIN ws.size.width (solveLayoutReal)', 0x44480000, 'hex'],
  [0x40914, 'CHAIN ws.size.height', 0x44160000, 'hex'],
  [0x40918, 'CHAIN viewport.width (impl construct)', 0x44480000, 'hex'],
  [0x4091C, 'CHAIN viewport.height (impl construct)', 0x44160000, 'hex'],
  [0x40920, 'CHAIN viewport.width (impl pre-call)', 0x44480000, 'hex'],
  [0x40924, 'CHAIN viewport.height (impl pre-call)', 0x44160000, 'hex'],
];

function dumpMarkers(sab, label) {
  const dv = new DataView(sab);
  const u = a => dv.getUint32(a, true);
  console.log(`\n===== MARKERS (${label}) =====`);
  let lastNamed = null;
  for (const [addr, name, want, fmt] of MARKERS) {
    const v = u(addr);
    const isSentinel = (v & 0xFFFF0000) === 0xDEAD0000;
    const shown = fmt === 'hex' ? HEX(v) : String(v >>> 0);
    let verdict = '';
    if (isSentinel) verdict = `  <- NOT REACHED (sentinel ${HEX(v)})`;
    else if (want !== null) verdict = (v >>> 0) === (want >>> 0) ? '  OK' : `  <<< MISMATCH (want ${fmt === 'hex' ? HEX(want) : want})`;
    if (!isSentinel && v !== 0) lastNamed = name;
    console.log(`  ${HEX(addr)} ${name.padEnd(34)} = ${shown}${verdict}`);
  }
  console.log(`  (last non-zero marker: ${lastNamed})`);
  return lastNamed;
}

// AZ_REG_TRACE ring dump: counter@983024, base@983040, 8192×8B (reg_id@+0, val_lo32@+4).
const REG_NAME = { 99: 'RSP', 0: 'RAX', 50: 'RCX', 52: 'RDX', 53: 'R8', 62: 'R9', 54: 'RBX', 55: 'R12', 56: 'R14', 57: 'R13', 58: 'RBP', 59: 'RSI', 61: 'R15', 70: 'RDI' };
function dumpRegTrace(sab, tail = 60) {
  const dv = new DataView(sab);
  const cnt = dv.getUint32(983024, true), base = 983040, N = Math.min(cnt, 8191);
  console.log(`\n===== AZ_REG_TRACE ring: count=${cnt} (showing last ${Math.min(tail, N)} + all HUGE >=256MB) =====`);
  const huge = [];
  for (let k = 0; k < N; k++) {
    const id = dv.getUint32(base + k * 8, true), v = dv.getUint32(base + k * 8 + 4, true);
    if (v >= 0x10000000) huge.push(`  [${k}] ${REG_NAME[id] || ('r' + id)}=0x${v.toString(16)} (${(v / 1048576).toFixed(1)}MB)`);
  }
  console.log('  --- HUGE (>=256MB) register values (garbage-pointer suspects) ---');
  console.log(huge.length ? huge.join('\n') : '  (none)');
  console.log('  --- last ' + Math.min(tail, N) + ' reg stores ---');
  for (let k = Math.max(0, N - tail); k < N; k++) {
    const id = dv.getUint32(base + k * 8, true), v = dv.getUint32(base + k * 8 + 4, true);
    console.log(`  [${k}] ${REG_NAME[id] || ('r' + id)}=0x${v.toString(16)}`);
  }
}

// AZ_READ_TRACE ring: counter@917488, base@917504, 16384×4B (ctrl-probe-load addr i32).
function dumpReadTrace(sab, tail = 30) {
  const dv = new DataView(sab);
  const cnt = dv.getUint32(917488, true), base = 917504, N = Math.min(cnt, 16383);
  console.log(`\n===== AZ_READ_TRACE (hashbrown ctrl-probe load addrs): count=${cnt} =====`);
  const huge = [];
  for (let k = 0; k < N; k++) { const a = dv.getUint32(base + k * 4, true); if (a >= 0x10000000) huge.push(`  [${k}] 0x${a.toString(16)} (${(a / 1048576).toFixed(1)}MB)`); }
  console.log('  --- ctrl reads >=256MB (garbage ctrl base) ---');
  console.log(huge.length ? huge.join('\n') : '  (none)');
  console.log('  --- last ' + Math.min(tail, N) + ' ctrl-read addrs ---');
  for (let k = Math.max(0, N - tail); k < N; k++) { const a = dv.getUint32(base + k * 4, true); console.log(`  [${k}] 0x${a.toString(16)}`); }
}
// AZ_WRITE_TRACE ring: counter@851952, base@851968, 16384×8B (addr@+0, val@+4).
function dumpWriteTrace(sab, tail = 30) {
  const dv = new DataView(sab);
  const cnt = dv.getUint32(851952, true), base = 851968, N = Math.min(cnt, 16383);
  console.log(`\n===== AZ_WRITE_TRACE (guest i64 writes): count=${cnt} =====`);
  console.log('  --- ctrl/bucket_mask write-backs (stack addr 0x10000-0x30000, heap-ptr val >=0x6000000) ---');
  const ctrlw = [];
  for (let k = 0; k < N; k++) { const a = dv.getUint32(base + k * 8, true), v = dv.getUint32(base + k * 8 + 4, true); if (a >= 0x10000 && a < 0x30000 && v >= 0x6000000) ctrlw.push(`  [${k}] [0x${a.toString(16)}] = 0x${v.toString(16)} (${(v / 1048576).toFixed(1)}MB)`); }
  console.log(ctrlw.length ? ctrlw.slice(-20).join('\n') : '  (none)');
  console.log('  --- last ' + Math.min(tail, N) + ' i64 writes (addr=val) ---');
  for (let k = Math.max(0, N - tail); k < N; k++) { const a = dv.getUint32(base + k * 8, true), v = dv.getUint32(base + k * 8 + 4, true); console.log(`  [${k}] [0x${a.toString(16)}]=0x${v.toString(16)}`); }
}

// =========================================================================
// MAIN THREAD
// =========================================================================
if (isMainThread) {
  const html = (await fetch_('/')).toString();
  const initialCounter = parseInt((html.match(/<div id="az_1">(\d+)<\/div>/) || ['', '5'])[1]);
  const typeId = (html.match(/"type_id":"(\d+)"/) || ['', '0'])[1];
  const miniUrl = html.match(/href="(\/az\/mini\.[^"]+)"/)[1];
  const layoutUrl = html.match(/href="(\/az\/layout\/[^"]+)"/)[1];
  console.log(`[probe] stage=${STAGE} hang_ms=${HANG_MS} counter=${initialCounter} typeId=${typeId}`);
  console.log(`[probe] mini=${miniUrl} layout=${layoutUrl}`);

  const [miniBytesRaw, layoutBytesRaw] = await Promise.all([fetch_(miniUrl), fetch_(layoutUrl)]);
  let fontBytes = new Uint8Array(0);
  try { const fb = await fetch_('/az/fallback.ttf'); if (fb.length > 0) fontBytes = new Uint8Array(fb); } catch {}

  console.log(`[probe] patching mini (${miniBytesRaw.length}B) + layout (${layoutBytesRaw.length}B) to shared memory...`);
  const mini = await patchSharedMemory(miniBytesRaw);
  const layout = await patchSharedMemory(layoutBytesRaw);
  console.log(`[probe] patched: mini count=${mini.count} layout count=${layout.count}`);

  const worker = new Worker(new URL(import.meta.url), {
    workerData: {
      miniBytes: mini.bytes, layoutBytes: layout.bytes, fontBytes,
      typeId, initialCounter, stage: STAGE,
    },
  });

  let sab = null, finished = false;
  worker.on('message', m => {
    if (m.type === 'sab') { sab = m.sab; console.log('[probe] received shared memory; polling markers...'); }
    else if (m.type === 'log') console.log('[worker] ' + m.text);
    else if (m.type === 'done') { finished = true; console.log(`[worker] solveLayoutReal RETURNED rc=${m.rc} (no hang)`); }
    else if (m.type === 'error') { finished = true; console.log(`[worker] ERROR: ${m.text}`); }
  });
  worker.on('error', e => { console.log('[probe] worker error: ' + e.message); });

  // poll until HANG_MS elapsed or the worker finishes; dump every ~3s
  const t0 = Date.now();
  let lastDumpT = -3000;
  while (Date.now() - t0 < HANG_MS && !finished) {
    await new Promise(r => setTimeout(r, 500));
    const el = Date.now() - t0;
    if (sab && el - lastDumpT >= 3000) { dumpMarkers(sab, `t+${(el / 1000).toFixed(1)}s`); lastDumpT = el; }
  }
  if (sab) {
    const last = dumpMarkers(sab, finished ? 'FINAL (returned)' : 'FINAL (HUNG)');
    if (process.env.AZ_REGTRACE_DUMP === '1') dumpRegTrace(sab, parseInt(process.env.AZ_REGTRACE_TAIL || '60', 10));
    if (process.env.AZ_RWTRACE_DUMP === '1') { dumpReadTrace(sab); dumpWriteTrace(sab); }
    console.log(`\n[VERDICT] ${finished ? 'solve returned — no hang at this stage' : 'HUNG'} — last marker written: ${last}`);
  } else {
    console.log('[probe] never received shared memory (worker died during setup?)');
  }
  await worker.terminate();
  process.exit(0);
}

// =========================================================================
// WORKER THREAD
// =========================================================================
if (!isMainThread) {
  const { miniBytes, layoutBytes, fontBytes, typeId, initialCounter, stage } = workerData;
  const log = t => parentPort.postMessage({ type: 'log', text: t });
  let memory = null;
  let dbgState = 0;
  const dv = () => new DataView(memory.buffer);
  const u8 = () => new Uint8Array(memory.buffer);

  const azMulti3 = (sret, aLo, aHi, bLo, bHi) => {
    const d = dv(), mask = 0xFFFFFFFFFFFFFFFFn;
    const a = (BigInt.asUintN(64, BigInt(aHi)) << 64n) | BigInt.asUintN(64, BigInt(aLo));
    const b = (BigInt.asUintN(64, BigInt(bHi)) << 64n) | BigInt.asUintN(64, BigInt(bLo));
    const p = BigInt.asUintN(128, a * b);
    d.setBigUint64(Number(sret), p & mask, true); d.setBigUint64(Number(sret) + 8, (p >> 64n) & mask, true);
  };
  const azUdivti3 = (sret, aLo, aHi, bLo, bHi) => {
    const d = dv(), mask = 0xFFFFFFFFFFFFFFFFn;
    const a = (BigInt.asUintN(64, BigInt(aHi)) << 64n) | BigInt.asUintN(64, BigInt(aLo));
    const b = (BigInt.asUintN(64, BigInt(bHi)) << 64n) | BigInt.asUintN(64, BigInt(bLo));
    const q = b === 0n ? 0n : a / b;
    d.setBigUint64(Number(sret), q & mask, true); d.setBigUint64(Number(sret) + 8, (q >> 64n) & mask, true);
  };
  const AZ_MATH = { fmaxf: (a, b) => a !== a ? b : (b !== b ? a : Math.max(a, b)), fminf: (a, b) => a !== a ? b : (b !== b ? a : Math.min(a, b)), fmax: (a, b) => a !== a ? b : (b !== b ? a : Math.max(a, b)), fmin: (a, b) => a !== a ? b : (b !== b ? a : Math.min(a, b)), roundf: x => Math.sign(x) * Math.round(Math.abs(x)), round: x => Math.sign(x) * Math.round(Math.abs(x)), fabsf: Math.abs, fabs: Math.abs, sqrtf: Math.sqrt, sqrt: Math.sqrt, floorf: Math.floor, floor: Math.floor, ceilf: Math.ceil, ceil: Math.ceil, truncf: Math.trunc, trunc: Math.trunc, powf: Math.pow, pow: Math.pow };
  const table = new WebAssembly.Table({ initial: 64, element: 'anyfunc' });
  let cbTableIdx = -1;
  // DIAG: catch garbage (>256MB) addresses flowing through memory-op imports.
  const HUGE = 0x10000000;
  let hugeLogged = 0;
  const chkAddr = (tag, addr, extra) => {
    const a = Number(addr);
    if (a >= HUGE && hugeLogged < 12) { hugeLogged++; log('[HUGE-ADDR] ' + tag + ' addr=0x' + a.toString(16) + ' (' + (a / 1048576).toFixed(1) + 'MB)' + (extra || '')); }
    return a;
  };
  const realEnv = {
    __indirect_function_table: table,
    __az_resolve_callback: addr => addr === 0xFFFFFFFF ? 0xFFFFFFFF : (cbTableIdx >= 0 ? cbTableIdx : 0xFFFFFFFF),
    memset: (d, v, n) => { chkAddr('memset', d, ' n=' + n); u8().fill(v & 0xFF, d, d + n); return d; },
    memcpy: (d, s, n) => { chkAddr('memcpy.d', d, ' n=' + n); chkAddr('memcpy.s', s, ' n=' + n); u8().copyWithin(d, s, s + n); return d; },
    memmove: (d, s, n) => { chkAddr('memmove.d', d, ' n=' + n); chkAddr('memmove.s', s, ' n=' + n); u8().copyWithin(d, s, s + n); return d; },
    __multi3: azMulti3, __udivti3: azUdivti3,
    __remill_read_memory_32: (mem, addr) => dv().getUint32(chkAddr('read32', addr), true),
    __remill_write_memory_32: (mem, addr, val) => { dv().setUint32(chkAddr('write32', addr), Number(val) >>> 0, true); return mem; },
    __remill_atomic_begin: mem => mem, __remill_atomic_end: mem => mem,
    __remill_compare_exchange_memory_64: (mem, addr, expPtr, desired) => {
      const d = dv(), a = Number(addr), e = Number(expPtr); const actual = d.getBigUint64(a, true);
      if (actual === d.getBigUint64(e, true)) d.setBigUint64(a, BigInt.asUintN(64, BigInt(desired)), true);
      d.setBigUint64(e, actual, true); return mem;
    },
    __remill_compare_exchange_memory_8: (mem, addr, expPtr, desired) => {
      const u = u8(), a = Number(addr), e = Number(expPtr); const actual = u[a];
      if (actual === u[e]) u[a] = Number(desired) & 0xFF; u[e] = actual; return mem;
    },
  };
  const loggedStubs = new Set();
  const stubFor = n => {
    if (AZ_MATH[n]) return AZ_MATH[n];
    if (/write_memory|barrier|exception_clear/.test(n)) return () => {};
    if (!loggedStubs.has(n)) { log('[STUB-0] ' + n); loggedStubs.add(n); }
    return /_f64\b/.test(n) ? () => 0 : (/_64\b/.test(n) ? () => 0n : () => 0);
  };
  const proxyEnv = env => new Proxy({}, {
    get: (_, p) => typeof p === 'string' ? (Object.prototype.hasOwnProperty.call(env, p) ? env[p] : stubFor(p)) : undefined,
    has: () => true,
  });

  try {
    const { instance: miniI } = await WebAssembly.instantiate(miniBytes, { env: proxyEnv(realEnv) });
    const mini = miniI.exports;
    memory = mini.memory;
    if (!(memory.buffer instanceof SharedArrayBuffer)) throw new Error('mini.memory.buffer is NOT a SharedArrayBuffer — patch failed');
    parentPort.postMessage({ type: 'sab', sab: memory.buffer });
    log(`mini instantiated; memory ${(memory.buffer.byteLength / 1048576) | 0}MB shared`);

    const cbEnv = { ...realEnv, memory };  // layout.wasm imports env.memory → the shared mem
    let layoutTableIdx = -1;
    if (stage >= 2) {
      const { instance: layoutI } = await WebAssembly.instantiate(layoutBytes, { env: proxyEnv(cbEnv) });
      table.grow(1); layoutTableIdx = table.length - 1; table.set(layoutTableIdx, layoutI.exports.callback);
      log('layout instantiated @table[' + layoutTableIdx + ']');
    }

    if (typeof mini.AzStartup_resetBumpHeap === 'function') mini.AzStartup_resetBumpHeap(160 * 1024 * 1024);
    const state = mini.AzStartup_init(0, 0);
    dbgState = state;
    const modelPtr = mini.AzStartup_alloc(4);
    dv().setUint32(modelPtr, initialCounter, true);
    const tid = BigInt(typeId);
    const refanyPtr = mini.AzStartup_hydrate(Number(tid & 0xFFFFFFFFn), Number((tid >> 32n) & 0xFFFFFFFFn), modelPtr, 4);
    mini.AzStartup_setRefAny(state, refanyPtr);
    if (layoutTableIdx >= 0) mini.AzStartup_setLayoutCbTableIdx(state, layoutTableIdx);
    mini.AzStartup_setModelPtr(state, modelPtr);
    mini.AzStartup_registerCbNode(state, 0);
    mini.AzStartup_setDisplayNode(state, 0);
    if (stage >= 2 && fontBytes.length > 0 && typeof mini.AzStartup_setFallbackFont === 'function') {
      const fp = mini.AzStartup_alloc(fontBytes.length); u8().set(fontBytes, fp);
      mini.AzStartup_setFallbackFont(fp, fontBytes.length); log('fallback font registered (' + fontBytes.length + 'B)');
    }

    if (stage >= 2) {
      const rc = mini.AzStartup_initLayoutCache(state, 800, 600, 0);
      log('initLayoutCache rc=' + rc);
      const domPtr = typeof mini.AzStartup_getCurrentDomPtr === 'function' ? mini.AzStartup_getCurrentDomPtr(state) : 0;
      log('current_dom_ptr=' + domPtr);
      if (domPtr && typeof mini.AzStartup_hydrateStyledDom === 'function') {
        const hRc = mini.AzStartup_hydrateStyledDom(state);
        log('hydrateStyledDom rc=' + hRc);
      }
    }

    // DIAG: bump-heap / memory-boundary check. If the OOB is local0+offset just
    // past the 512MB initial memory, growing to max (1GB) makes it disappear →
    // memory-size/exhaustion issue (likely a garbage-sized upstream alloc); if it
    // still OOBs at the same offset, it's a garbage local0 / lift value-flow bug.
    const pagesNow = memory.buffer.byteLength / 65536;
    if (process.env.AZ_GROW === '1') {
      try { const r = memory.grow(16384 - pagesNow); log('memory.grow ' + pagesNow + '→' + (memory.buffer.byteLength / 65536) + ' pages (r=' + r + ')'); }
      catch (e) { log('grow failed: ' + e.message); }
    }
    const bumpPtr = new DataView(memory.buffer).getUint32(262176, true);
    log('bump ptr before solve = 0x' + bumpPtr.toString(16) + ' (' + (bumpPtr / 1048576).toFixed(1) + 'MB), mem=' + (memory.buffer.byteLength / 1048576) + 'MB');
    log('>>> calling solveLayoutReal (may hang) ...');
    dv().setUint32(264192, 0, true); // reset 0x40A00 so solveLayoutReal's wrapper publishes ITS %state_buf (not a callback's)
    const solveFn = mini.AzStartup_solveLayoutReal || mini.AzStartup_solveLayout;
    const rc = solveFn(state, 800, 600);
    parentPort.postMessage({ type: 'done', rc });
  } catch (e) {
    let diag = '';
    try {
      const d = dv();
      const sb = d.getUint32(264192, true); // %state_buf addr published by the wrapper marker @0x40A00
      const rip = d.getBigUint64(sb + 2472, true);
      const rbx = d.getBigUint64(sb + 2232, true);
      const rsp = d.getBigUint64(sb + 2312, true);
      const r14 = d.getBigUint64(sb + 2440, true);
      const rsi = d.getBigUint64(sb + 2280, true);
      const r12 = d.getBigUint64(sb + 2408, true);
      const fa = (Number(rbx & 0xFFFFFFFFn) + 16) >>> 0;
      diag = `\n[diag-at-trap] state_buf=0x${sb.toString(16)} RIP=0x${rip.toString(16)} RBX=0x${rbx.toString(16)} RSI=0x${rsi.toString(16)} RSP=0x${rsp.toString(16)} R12=0x${r12.toString(16)} R14=0x${r14.toString(16)} faultAddr(RBX+16)=0x${fa.toString(16)}`;
      // [AZ-DIAG REVERT] guest shadow stack persists after trap (linear mem intact, unlike stale State regs).
      // [rsp+0x120]=Vec data ptr spill (rbx reload @0xbfae21), nearby = len. RSP is fresh (written at last call).
      const rspL = Number(rsp & 0xFFFFFFFFn);
      let stk = '\n[diag-stack] rsp_lin=0x' + rspL.toString(16);
      // failed Vec sret target @[rsp+0x598..0x5b0] = {ptr@598, cap@5a0, len@5a8}; ptr-copy @[rsp+0x120]
      for (const off of [0x110,0x118,0x120,0x128,0x590,0x598,0x5a0,0x5a8,0x5b0]) {
        try { const v = d.getBigUint64(rspL + off, true); stk += `\n   [rsp+0x${off.toString(16)}]=0x${v.toString(16)}`; } catch (e3) { stk += `\n   [rsp+0x${off.toString(16)}]=<oob>`; }
      }
      diag += stk;
    } catch (e2) { diag = '\n[diag-read-failed] ' + e2.message; }
    parentPort.postMessage({ type: 'error', text: ((e && e.stack) || String(e)) + diag });
  }
}
