// init-probe.mjs <mini.wasm> — instantiate a served mini standalone and call
// AzStartup_init, then read the bump-allocator + NeverLift recorders.
//
// Why: AzWriter's bootstrap dies in AzStartup_init at handle_alloc_error (an
// allocation returned null) with an otherwise CLEAN lift audit. The bump
// helper has no failure path of its own, so the question is whether the alloc
// reached it at all (cursor/count move?) and what size it asked for.
import fs from 'fs';

const MINI = process.argv[2];
const bytes = fs.readFileSync(MINI);
let memory = null;
const dv = () => new DataView(memory.buffer);

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
    __indirect_function_table: new WebAssembly.Table({ initial: 4096, element: 'anyfunc' }),
};
const stub = new Set();
const env = new Proxy({}, {
    has: () => true,
    get(_t, name) {
        if (name in real) return real[name];
        if (!stub.has(name)) { stub.add(name); }
        return /_64\b/.test(String(name)) ? () => 0n : () => 0;
    },
});

const { instance } = await WebAssembly.instantiate(bytes, { env });
const m = instance.exports;
memory = m.memory;
const u32 = a => dv().getUint32(a, true);
const u64 = a => dv().getBigUint64(a, true);

console.log('exports:', Object.keys(m).length, '| memory pages:', memory.buffer.byteLength / 65536,
    '=', (memory.buffer.byteLength / 1048576).toFixed(1), 'MiB');
console.log('pre-init  bump cursor(40020)=0x' + u32(0x40020).toString(16),
    'last_size(40030)=' + u64(0x40030), 'count(40038)=' + u64(0x40038));

try {
    const st = m.AzStartup_init();
    console.log('AzStartup_init -> 0x' + (st >>> 0).toString(16));
} catch (e) {
    console.log('AzStartup_init TRAPPED:', e.message);
    const frames = String(e.stack || '').split('\n').slice(0, 4).join(' | ');
    console.log('  frames:', frames);
}
console.log('post-init bump cursor(40020)=0x' + u32(0x40020).toString(16),
    'last_size(40030)=' + u64(0x40030), 'count(40038)=' + u64(0x40038));
console.log('NeverLift marker(40048)=0x' + u32(0x40048).toString(16),
    ' trap marker(40048hi/262216)=0x' + u64(262216).toString(16));
console.log('unk(40158)=' + u32(0x40158), 'upc(40900)=0x' + u32(0x40900).toString(16),
    'mb_count(400FC)=' + u32(0x400FC));
if (stub.size) console.log('zero-stubbed imports (' + stub.size + '):', [...stub].slice(0, 12).join(', '));
