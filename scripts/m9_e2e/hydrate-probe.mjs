// hydrate-probe.mjs <mini.wasm> <state.json> <deserFnDecimal>
//
// Replays the loader's JSON-hydration path against a saved mini, standalone:
//   resetBumpHeap -> init -> registerStateDeserializer -> hydrateJson
// and prints the step markers (0x40980) plus the trap marker (262216), so a
// failure inside hydration names its step without a browser round-trip.
//
// The env provides the SAME helpers the loader does — notably __multi3 /
// __udivti3, whose absence makes every float path trap and looks exactly
// like a mis-lift (see weblift-fast-debug-loop.md).
import fs from 'fs';

const [MINI, JSON_PATH, DESER] = process.argv.slice(2);
const bytes = fs.readFileSync(MINI);
const jsonText = fs.readFileSync(JSON_PATH, 'utf8');
const deserFn = BigInt(DESER);

let memory = null;
const dv = () => new DataView(memory.buffer);
const u32 = a => dv().getUint32(a, true);
const u64 = a => dv().getBigUint64(a, true);

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
    __remill_atomic_begin: m => m,
    __remill_atomic_end: m => m,
    memset: (d, c, n) => { new Uint8Array(memory.buffer).fill(Number(c) & 0xFF, Number(d), Number(d) + Number(n)); return d; },
    memcpy: (d, s, n) => { new Uint8Array(memory.buffer).copyWithin(Number(d), Number(s), Number(s) + Number(n)); return d; },
    memmove: (d, s, n) => { new Uint8Array(memory.buffer).copyWithin(Number(d), Number(s), Number(s) + Number(n)); return d; },
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

const { instance } = await WebAssembly.instantiate(bytes, { env });
const m = instance.exports;
memory = m.memory;

m.AzStartup_resetBumpHeap(160 * 1024 * 1024);
const state = m.AzStartup_init(0, 0) >>> 0;
console.log('state=0x' + state.toString(16));

const enc = new TextEncoder().encode(jsonText);
const jptr = m.AzStartup_alloc(enc.length) >>> 0;
new Uint8Array(memory.buffer, jptr, enc.length).set(enc);
console.log('json bytes=' + enc.length + ' at 0x' + jptr.toString(16));

m.AzStartup_registerStateDeserializer(state, deserFn);
try {
    const r = m.AzStartup_hydrateJson(state, jptr, enc.length) >>> 0;
    console.log('hydrateJson -> 0x' + r.toString(16) + (r ? '  ✓ RefAny built' : '  (0 = failed)'));
} catch (e) {
    console.log('hydrateJson TRAPPED: ' + e.message);
    console.log('  frames: ' + String(e.stack || '').split('\n').slice(1, 4).join(' | '));
}
console.log('step marker(40980)=' + u32(0x40980) +
    '  [1 entered 2 deser 3 parsed 4 calling 5 Ok 6 stored 0xE0 app-Err]');
console.log('trap marker(262216)=0x' + u64(262216).toString(16) +
    '  unk=' + u32(0x40158) + ' upc=0x' + u32(0x40900).toString(16) +
    ' mb=' + u32(0x400FC));
// The FIRST unmatched dispatch is the causal one; later PCs are produced by a
// caller already running on a garbage return value.
const unk = u32(0x40158);
if (unk) {
    const ring = [];
    for (let i = 0; i < 16 && i < unk; i++) ring.push('0x' + u32(0x409C0 + 4 * i).toString(16));
    console.log('unmatched dispatches: ' + unk +
        '  first=0x' + u32(0x409B0).toString(16) +
        '  last=0x' + u32(0x40900).toString(16) +
        '  ring: ' + ring.join(' '));
}
if (stubbed.size) console.log('zero-stubbed: ' + [...stubbed].join(', '));
