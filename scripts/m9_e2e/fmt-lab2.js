// fmt-lab2.js — stage-2 isolation: call `<char as Display>::fmt` (impl$21)
// DIRECTLY with a hand-crafted Formatter, against the served mini.
//
// Prior findings: pieces-only fmt::write works; EVERY rt::Argument-dispatched
// formatter fails ({char},{u8},{u32},{u32:x}); the {str} "success" was rustc
// folding the literal into pieces (args.len=0 — verified in probeFmt's native
// code). write_char in isolation is fully correct. This test discriminates:
//   impl$21(&'x', &crafted_formatter) returns Err  → impl$21's own body
//   returns Ok + appends 'x'                       → the fault is fmt::write's
//     arg-loop handoff (Formatter construction or the dispatch call itself)
//
// Formatter layout (from the impl$18 native disasm): +0 buf.data,
// +8 buf.vtable, then options. We zero the options region entirely
// (width/precision None, flags 0) and give fill=' ' at every plausible slot.
//
// Wrapper ABI: (i64 arg0_lo→RCX, i64 arg0_hi→R8, i32 arg1→RDX), ret=RAX.
const fs = require('fs');
const [MINI] = process.argv.slice(2);
const DEP_CHARFMT = '7ffd0c899b20';
const VTABLE_SYNTH = 0x6de068;

(async () => {
    const bytes = fs.readFileSync(MINI);
    let memory = null;
    const dv = () => new DataView(memory.buffer);
    const realEnv = {
        __remill_read_memory_8: (m, a) => dv().getUint8(Number(a)),
        __remill_read_memory_16: (m, a) => dv().getUint16(Number(a), true),
        __remill_read_memory_32: (m, a) => dv().getUint32(Number(a), true),
        __remill_read_memory_64: (m, a) => dv().getBigUint64(Number(a), true),
        __remill_write_memory_8: (m, a, v) => { dv().setUint8(Number(a), Number(v) & 0xFF); return m; },
        __remill_write_memory_16: (m, a, v) => { dv().setUint16(Number(a), Number(v) & 0xFFFF, true); return m; },
        __remill_write_memory_32: (m, a, v) => { dv().setUint32(Number(a), Number(v) >>> 0, true); return m; },
        __remill_write_memory_64: (m, a, v) => { dv().setBigUint64(Number(a), BigInt.asUintN(64, BigInt(v)), true); return m; },
        __remill_atomic_begin: m => m, __remill_atomic_end: m => m,
        memset: (d, c, n) => { new Uint8Array(memory.buffer).fill(Number(c) & 0xFF, Number(d), Number(d) + Number(n)); return d; },
        memcpy: (d, s, n) => { new Uint8Array(memory.buffer).copyWithin(Number(d), Number(s), Number(s) + Number(n)); return d; },
        memmove: (d, s, n) => { new Uint8Array(memory.buffer).copyWithin(Number(d), Number(s), Number(s) + Number(n)); return d; },
        __indirect_function_table: new WebAssembly.Table({ initial: 64, element: 'anyfunc' }),
    };
    const h = base => ({
        get(t, name) {
            if (name in base) return base[name];
            const big = /_(64)\b/.test(name) && !/compare_exchange|write_memory/.test(name);
            return () => (big ? 0n : 0);
        },
    });
    const { instance } = await WebAssembly.instantiate(bytes, { env: new Proxy({}, h(realEnv)) });
    const mini = instance.exports;
    memory = mini.memory;
    mini.AzStartup_resetBumpHeap(160 * 1024 * 1024);
    const alloc = n => mini.AzStartup_alloc(n) >>> 0;
    const d = dv();

    // Guest String (layout cap,ptr,len — verified) with 8-byte buffer, len=0.
    const buf = alloc(16);
    const str = alloc(24);
    d.setBigUint64(str, 8n, true);
    d.setBigUint64(str + 8, BigInt(buf), true);
    d.setBigUint64(str + 16, 0n, true);

    // char value 'x' (char == u32)
    const ch = alloc(4);
    d.setUint32(ch, 0x78, true);

    // Crafted Formatter: 64 bytes, zeroed; +0 buf.data=&String, +8 buf.vtable.
    const f = alloc(64);
    new Uint8Array(memory.buffer).fill(0, f, f + 64);
    d.setBigUint64(f, BigInt(str), true);
    d.setBigUint64(f + 8, BigInt(VTABLE_SYNTH), true);
    // fill char ' ' at the two plausible option slots (harmless if wrong)
    d.setUint32(f + 16, 0x20, true);

    const dep = mini['__az_dep_' + DEP_CHARFMT];
    if (!dep) { console.log('NO EXPORT'); process.exit(1); }
    let rc;
    try {
        rc = dep(BigInt(ch), 0n, f) >>> 0; // RCX=&char, RDX(arg1)=&Formatter
    } catch (e) {
        console.log('TRAPPED:', e.message);
        process.exit(0);
    }
    const len = d.getBigUint64(str + 16, true);
    const b0 = new Uint8Array(memory.buffer)[buf];
    console.log(`impl$21::fmt('x', crafted Formatter): rc(RAX)=0x${rc.toString(16)}  String.len=${len}  buf[0]=0x${b0.toString(16)}${b0 === 0x78 ? " ('x' ✓)" : ''}`);
    console.log(rc === 0 && Number(len) === 1 && b0 === 0x78
        ? 'VERDICT: impl$21 body is CORRECT → fault is in fmt::write arg-loop handoff'
        : 'VERDICT: impl$21 itself misbehaves with a well-formed Formatter');
})().catch(e => { console.log('LAB-ERR:', e.message || e); process.exit(1); });
