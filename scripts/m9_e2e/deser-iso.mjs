// deser-iso.mjs <mini.wasm>
//
// Call the app's lifted state deserializer DIRECTLY, bypassing hydrateJson.
//
// hydrateJson traps at marker 4 (inside doc_state_from_json) for EVERY input,
// including `{}` — so the failure is input-independent and the question is
// only: does the callee break on its own, or is it handed a bad argument?
// Calling it with a Json we control answers that in one shot.
//
// Wrapper ABI for __az_dep_<native>: (i64 arg0_lo -> RCX, i64 arg0_hi -> R8,
// i32 arg1 -> RDX) -> i32 = RAX.  doc_state_from_json(json: Json) ->
// ResultRefAnyString has BOTH a large by-value arg and a large return, so on
// Win64 RCX is the sret pointer and RDX is a pointer to the Json.
import fs from 'fs';
const MINI = process.argv[2];
const DESER_EXPORT = process.argv[3] || '__az_dep_7ff694164640';

let memory = null;
const dv = () => new DataView(memory.buffer);
const u8 = () => new Uint8Array(memory.buffer);
const wide = (name) => (sret, aLo, aHi, bLo, bHi) => {
    const d = dv(), M = 0xFFFFFFFFFFFFFFFFn;
    const a = (BigInt.asUintN(64, BigInt(aHi)) << 64n) | BigInt.asUintN(64, BigInt(aLo));
    const b = (BigInt.asUintN(64, BigInt(bHi)) << 64n) | BigInt.asUintN(64, BigInt(bLo));
    const r = name === 'mul' ? BigInt.asUintN(128, a * b) : (b === 0n ? 0n : a / b);
    d.setBigUint64(Number(sret), r & M, true);
    d.setBigUint64(Number(sret) + 8, (r >> 64n) & M, true);
};
const real = {
    __remill_read_memory_8: (m, a) => dv().getUint8(Number(a)),
    __remill_read_memory_16: (m, a) => dv().getUint16(Number(a), true),
    __remill_read_memory_32: (m, a) => dv().getUint32(Number(a), true),
    __remill_read_memory_64: (m, a) => dv().getBigUint64(Number(a), true),
    __remill_write_memory_8: (m, a, v) => { dv().setUint8(Number(a), Number(v) & 0xFF); return m; },
    __remill_write_memory_16: (m, a, v) => { dv().setUint16(Number(a), Number(v) & 0xFFFF, true); return m; },
    __remill_write_memory_32: (m, a, v) => { dv().setUint32(Number(a), Number(v) >>> 0, true); return m; },
    __remill_write_memory_64: (m, a, v) => { dv().setBigUint64(Number(a), BigInt.asUintN(64, BigInt(v)), true); return m; },
    __remill_atomic_begin: m => m, __remill_atomic_end: m => m,
    memset: (d, c, n) => { u8().fill(Number(c) & 0xFF, Number(d), Number(d) + Number(n)); return d; },
    memcpy: (d, s, n) => { u8().copyWithin(Number(d), Number(s), Number(s) + Number(n)); return d; },
    memmove: (d, s, n) => { u8().copyWithin(Number(d), Number(s), Number(s) + Number(n)); return d; },
    sqrtf: Math.sqrt, sqrt: Math.sqrt,
    __multi3: wide('mul'), __udivti3: wide('div'),
    __indirect_function_table: new WebAssembly.Table({ initial: 4096, element: 'anyfunc' }),
};
const stubbed = new Set();
const env = new Proxy({}, {
    has: () => true,
    get: (_t, n) => {
        if (n in real) return real[n];
        stubbed.add(String(n));
        return /_64\b/.test(String(n)) ? () => 0n : () => 0;
    },
});

const { instance } = await WebAssembly.instantiate(fs.readFileSync(MINI), { env });
const m = instance.exports;
memory = m.memory;
if (typeof m[DESER_EXPORT] !== 'function') {
    console.log(`export ${DESER_EXPORT} missing`); process.exit(2);
}

m.AzStartup_resetBumpHeap(160 * 1024 * 1024);
const state = m.AzStartup_init(0, 0) >>> 0;
console.log('state=0x' + state.toString(16));

const recorders = () => ({
    unk: dv().getUint32(0x40158, true),
    upc: dv().getUint32(0x40900, true),
    first: dv().getUint32(0x409B0, true),
    mb: dv().getUint32(0x400FC, true),
    trap: dv().getBigUint64(262216, true),
});
const before = recorders();

// A zeroed Json: value_type = 0 and an all-zero internal. Whatever variant 0
// is, Display's arms either write a literal or read a zeroed AzString — no
// pointer chasing into unmapped memory either way.
const jsonPtr = m.AzStartup_alloc(256) >>> 0;
u8().fill(0, jsonPtr, jsonPtr + 256);
const sret = m.AzStartup_alloc(256) >>> 0;
u8().fill(0, sret, sret + 256);
console.log(`json=0x${jsonPtr.toString(16)} sret=0x${sret.toString(16)}`);

try {
    const rc = m[DESER_EXPORT](BigInt(sret), 0n, jsonPtr);
    console.log(`deserializer RETURNED rc=${rc}`);
    const w = [];
    for (let i = 0; i < 6; i++) w.push('0x' + dv().getBigUint64(sret + i * 8, true).toString(16));
    console.log('  sret words: ' + w.join(' '));
} catch (e) {
    console.log('deserializer TRAPPED: ' + e.message);
    console.log('  frames: ' + String(e.stack || '').split('\n').slice(1, 4).join(' | '));
}
const after = recorders();
console.log(`unk ${before.unk} -> ${after.unk}` +
    (after.unk > before.unk ? `  first=0x${after.first.toString(16)} last=0x${after.upc.toString(16)}` : '') +
    `   missing_block ${before.mb} -> ${after.mb}` +
    (after.trap !== before.trap ? `   TRAP marker=0x${after.trap.toString(16)}` : '   (no trap marker)'));
if (stubbed.size) console.log('zero-stubbed: ' + [...stubbed].join(', '));
