// fmt-lab3.js — isolated test of core::str::count::do_count_chars, the SWAR
// char counter the Formatter width path (and ONLY the width path) uses.
// Probe v4: every fmt family passes except {:>5} width-padding — this either
// convicts or acquits the counter in one call per length.
//
// do_count_chars(ptr: RCX, len: RDX) -> usize in RAX.
// Wrapper ABI: (arg0_lo->RCX, arg0_hi->R8, arg1->RDX) -> i32(RAX).
const fs = require('fs');
const [MINI, DEP] = process.argv.slice(2);

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
    const dep = mini['__az_dep_' + DEP];
    if (!dep) { console.log('NO EXPORT __az_dep_' + DEP); process.exit(1); }

    // ASCII strings of several lengths (crossing the SWAR chunk thresholds:
    // the scalar path handles short strings, the usize-chunk path kicks in
    // above ~2*usize), plus one multi-byte UTF-8 case.
    const cases = [
        ['255', 3], ['a', 1], ['hello', 5], ['0123456789abcdef', 16],
        ['x'.repeat(32), 32], ['y'.repeat(100), 100],
        ['éé', 2 /* 2 chars, 4 bytes */],
    ];
    for (const [s, expect] of cases) {
        const b = Buffer.from(s, 'utf8');
        const p = mini.AzStartup_alloc(b.length + 8) >>> 0;
        new Uint8Array(memory.buffer).set(b, p);
        let rc;
        try { rc = dep(BigInt(p), 0n, b.length) >>> 0; }
        catch (e) { console.log(`len=${b.length} "${s.slice(0, 12)}": TRAPPED ${e.message}`); continue; }
        console.log(`bytes=${String(b.length).padStart(3)} chars_expected=${String(expect).padStart(3)} got=${rc}${rc === expect ? ' ✓' : '  ✗✗✗'}`);
    }
})().catch(e => { console.log('LAB-ERR:', e.message || e); process.exit(1); });
