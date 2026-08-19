// hydrate-phases.mjs <mini.wasm> <state.json> <deserFnDecimal>
//
// Same replay as hydrate-probe.mjs, but samples the runtime recorders BETWEEN
// phases instead of only at the end. The end-state reading cannot tell an
// unmatched dispatch that happened during `init` (benign — init returns a good
// state) from one that happened during `hydrateJson` (the actual failure), and
// the cumulative counters make it look like one number either way.
//
// Prints a delta line per phase, so a non-zero `unk` or `mb` is attributed to
// the phase that caused it, and dumps the missing-block ring (0x40160, 16 u32)
// which records the swallowed control flow in order.
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

// Snapshot of every recorder that attributes a fault to a phase.
const snap = () => ({
    unk: u32(0x40158), upc: u32(0x40900),
    mb: u32(0x400FC), mbpc: u32(0x400F8),
    weak: u32(0x4041C), trap: u64(262216),
});
let prev = null;
const phase = (label) => {
    const s = snap();
    if (prev) {
        const d = [];
        if (s.unk !== prev.unk) d.push(`unk +${s.unk - prev.unk} (last upc=0x${s.upc.toString(16)})`);
        if (s.mb !== prev.mb) d.push(`missing_block +${s.mb - prev.mb} (last pc=0x${s.mbpc.toString(16)})`);
        if (s.weak !== prev.weak) d.push(`weak_dispatch +${s.weak - prev.weak}`);
        if (s.trap !== prev.trap) d.push(`TRAP marker=0x${s.trap.toString(16)}`);
        console.log(`  [${label}] ` + (d.length ? d.join(', ') : 'clean'));
    }
    prev = s;
    return s;
};

m.AzStartup_resetBumpHeap(160 * 1024 * 1024);
phase('reset');
const state = m.AzStartup_init(0, 0) >>> 0;
console.log('state=0x' + state.toString(16));
phase('init');

const enc = new TextEncoder().encode(jsonText);
const jptr = m.AzStartup_alloc(enc.length) >>> 0;
new Uint8Array(memory.buffer, jptr, enc.length).set(enc);
console.log('json bytes=' + enc.length + ' at 0x' + jptr.toString(16));
phase('alloc+write json');

m.AzStartup_registerStateDeserializer(state, deserFn);
phase('registerStateDeserializer');

try {
    const r = m.AzStartup_hydrateJson(state, jptr, enc.length) >>> 0;
    console.log('hydrateJson -> 0x' + r.toString(16) + (r ? '  ✓ RefAny built' : '  (0 = failed)'));
} catch (e) {
    console.log('hydrateJson TRAPPED: ' + e.message);
    console.log('  frames: ' + String(e.stack || '').split('\n').slice(1, 4).join(' | '));
}
phase('hydrateJson');

console.log('step marker(40980)=' + u32(0x40980) +
    '  [1 entered 2 deser 3 parsed 4 calling 5 Ok 6 stored 0xE0 app-Err]');

// The ring records missing blocks in order; fn-ENTRY pcs are usually benign
// record-then-complete tails, MID-fn pcs are swallowed control flow.
const ring = [];
for (let i = 0; i < 16; i++) {
    const v = u32(0x40160 + 4 * i);
    if (v) ring.push('0x' + v.toString(16));
}
if (ring.length) console.log('missing-block ring: ' + ring.join(' '));
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
