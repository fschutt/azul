// jsonfmt.mjs <mini.wasm> [JsonType]
//
// Call `<Json as Display>::fmt` DIRECTLY, one level below doc_state_from_json,
// to localise the unmatched dispatch: if it fires here the fault is inside
// this function; if this returns cleanly the fault is above it.
//
// fmt(&self, f: &mut Formatter) -> fmt::Result  =>  RCX = &Json, RDX = &Formatter,
// which is exactly the __az_dep_ wrapper shape (lo -> RCX, arg1 -> RDX).
//
// Formatter is NOT repr(C); the empirically established shape from earlier
// fmt-lab work is {+0 buf.data, +8 buf.vtable, options after, all-zero options
// = defaults}. The vtable is located by SCANNING guest memory for the
// <String as fmt::Write> shape [drop_in_place, size=0x18, align=8, write_str,
// write_char, write_fmt] rather than hard-coding an address that moves.
import fs from 'fs';
const MINI = process.argv[2];
const VT_NAME = process.argv[3] || 'Bool';
const FMT_EXPORT = process.argv[4] || '__az_dep_7ff6940c68f0';

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
if (typeof m[FMT_EXPORT] !== 'function') { console.log(`export ${FMT_EXPORT} missing`); process.exit(2); }
m.AzStartup_resetBumpHeap(160 * 1024 * 1024);
m.AzStartup_init(0, 0);

// Locate a <String as fmt::Write> vtable: [drop, 0x18, 8, write_str, ...].
let vtable = -1;
{
    const d = dv(), lim = Math.min(memory.buffer.byteLength, 0x0A000000);
    for (let a = 0x100000; a + 48 < lim; a += 8) {
        if (d.getBigUint64(a + 8, true) !== 0x18n) continue;
        if (d.getBigUint64(a + 16, true) !== 0x8n) continue;
        const drop = d.getBigUint64(a, true), ws = d.getBigUint64(a + 24, true);
        if (drop > 0x1000n && drop < 0x2000000n && ws > 0x1000n && ws < 0x2000000n) { vtable = a; break; }
    }
}
console.log(vtable < 0 ? 'no String-Write vtable found' : `vtable @0x${vtable.toString(16)}`);
if (vtable < 0) process.exit(1);

const JSON_TYPE = { Null: 0, Bool: 1, Number: 2, String: 3, Array: 4, Object: 5 };
const txt = m.AzStartup_alloc(8) >>> 0; u8().set([0x78], txt);            // "x"
const json = m.AzStartup_alloc(128) >>> 0; u8().fill(0, json, json + 128);
dv().setUint32(json + 0, JSON_TYPE[VT_NAME], true);
dv().setBigUint64(json + 8, BigInt(txt), true);
dv().setBigUint64(json + 16, 1n, true);
dv().setBigUint64(json + 24, 1n, true);
dv().setUint8(json + 32, 1);                    // NoDestructor
dv().setFloat64(json + 48, 1.5, true);
dv().setUint8(json + 56, 1);

// String buffer for the writer: (ptr, cap, len)
const sbuf = m.AzStartup_alloc(64) >>> 0; u8().fill(0, sbuf, sbuf + 64);
const sdata = m.AzStartup_alloc(256) >>> 0; u8().fill(0, sdata, sdata + 256);
dv().setBigUint64(sbuf + 0, BigInt(sdata), true);
dv().setBigUint64(sbuf + 8, 256n, true);
dv().setBigUint64(sbuf + 16, 0n, true);

const fmt = m.AzStartup_alloc(128) >>> 0; u8().fill(0, fmt, fmt + 128);
dv().setBigUint64(fmt + 0, BigInt(sbuf), true);
dv().setBigUint64(fmt + 8, BigInt(vtable), true);

const before = { unk: dv().getUint32(0x40158, true), trap: dv().getBigUint64(262216, true) };
try {
    const rc = m[FMT_EXPORT](BigInt(json), 0n, fmt);
    const len = Number(dv().getBigUint64(sbuf + 16, true));
    const out = new TextDecoder().decode(u8().slice(sdata, sdata + Math.min(len, 64)));
    console.log(`Display::fmt(${VT_NAME}) RETURNED rc=${rc}  wrote ${len} byte(s): ${JSON.stringify(out)}`);
} catch (e) {
    console.log(`Display::fmt(${VT_NAME}) TRAPPED: ${e.message}`);
}
const after = { unk: dv().getUint32(0x40158, true), trap: dv().getBigUint64(262216, true) };
console.log(`  unk ${before.unk} -> ${after.unk}` +
    (after.unk > before.unk ? `  first=0x${dv().getUint32(0x409B0, true).toString(16)}` : '') +
    (after.trap !== before.trap ? `  TRAP=0x${after.trap.toString(16)}` : ''));
