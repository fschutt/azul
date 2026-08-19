// init-probe2.mjs <mini.wasm> — like init-probe but mirrors the LOADER's real
// boot order: resetBumpHeap(160 MiB) first, then AzStartup_init. Isolates
// "the allocator was never armed" from "the alloc never reached the helper".
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
const env = new Proxy({}, {
    has: () => true,
    get: (_t, n) => (n in real ? real[n] : (/_64\b/.test(String(n)) ? () => 0n : () => 0)),
});

const { instance } = await WebAssembly.instantiate(bytes, { env });
const m = instance.exports;
memory = m.memory;
const u32 = a => dv().getUint32(a, true);
const u64 = a => dv().getBigUint64(a, true);
const dump = tag => console.log(`${tag} cursor=0x${u32(0x40020).toString(16)} last_size=${u64(0x40030)} count=${u64(0x40038)}`);

dump('boot     ');
m.AzStartup_resetBumpHeap(160 * 1024 * 1024);
dump('post-reset');
try {
    const p = m.AzStartup_alloc(64) >>> 0;
    console.log('alloc(64) -> 0x' + p.toString(16));
} catch (e) { console.log('alloc TRAPPED:', e.message); }
dump('post-alloc');
try {
    const st = m.AzStartup_init();
    console.log('AzStartup_init -> 0x' + (st >>> 0).toString(16));
} catch (e) {
    console.log('AzStartup_init TRAPPED:', e.message);
    console.log('  frames:', String(e.stack || '').split('\n').slice(1, 4).join(' | '));
}
dump('post-init ');
console.log('NeverLift marker(262216)=0x' + u64(262216).toString(16),
    ' unk=' + u32(0x40158), ' mb=' + u32(0x400FC));
