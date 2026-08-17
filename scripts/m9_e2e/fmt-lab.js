// fmt-lab.js — call individual LIFTED functions in isolation via their
// __az_dep_<native> export wrappers, against a saved mini.wasm.
//
// Motivation (2026-08-17): the staged probe shows fmt::write returning Err for
// {char}/{u8}/{u32}/{u32:x} but Ok for {str} and raw write_str, with dispatch
// counters clean. This lab calls `<String as fmt::Write>::write_char` DIRECTLY
// with a hand-built guest String, removing every intermediate layer: if the
// isolated call already returns Err (nonzero RAX), the mis-lift is inside
// write_char itself; if it returns Ok, the caller side (Formatter/char::fmt or
// the return-value path through the dispatcher) is at fault.
//
// The dep wrapper ABI (verified in __az_dep_*.opt.ll): (i64 arg0_lo, i64
// arg0_hi, i32 arg1) -> i32, mapped to RCX(2248), RDX(2264), R8; return = RAX.
//
// Usage: node fmt-lab.js <mini.wasm> <depHex_write_char>
const fs = require('fs');

const [MINI, DEP] = process.argv.slice(2);

(async () => {
    const bytes = fs.readFileSync(MINI);
    let memory = null;
    const dv = () => new DataView(memory.buffer);

    // Minimal real env — same semantics full-cycle.js uses.
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
        __remill_compare_exchange_memory_8: (m, a, e, d) => {
            const u8 = new Uint8Array(memory.buffer); a = Number(a); e = Number(e);
            const actual = u8[a]; if (actual === u8[e]) u8[a] = Number(d) & 0xFF; u8[e] = actual; return m;
        },
        __remill_compare_exchange_memory_32: (m, a, e, d) => {
            const x = dv(); a = Number(a); e = Number(e);
            const actual = x.getUint32(a, true);
            if (actual === x.getUint32(e, true)) x.setUint32(a, Number(d) >>> 0, true);
            x.setUint32(e, actual, true); return m;
        },
        __remill_compare_exchange_memory_64: (m, a, e, d) => {
            const x = dv(); a = Number(a); e = Number(e);
            const actual = x.getBigUint64(a, true);
            if (actual === x.getBigUint64(e, true)) x.setBigUint64(a, BigInt.asUintN(64, BigInt(d)), true);
            x.setBigUint64(e, actual, true); return m;
        },
        memset: (d, c, n) => { new Uint8Array(memory.buffer).fill(Number(c) & 0xFF, Number(d), Number(d) + Number(n)); return d; },
        memcpy: (d, s, n) => { new Uint8Array(memory.buffer).copyWithin(Number(d), Number(s), Number(s) + Number(n)); return d; },
        memmove: (d, s, n) => { new Uint8Array(memory.buffer).copyWithin(Number(d), Number(s), Number(s) + Number(n)); return d; },
    };
    const h = base => ({
        get(t, name) {
            if (name in base) return base[name];
            // i64-returning stubs must hand back BigInt; everything else 0.
            const big = /_(64)\b/.test(name) && !/compare_exchange|write_memory/.test(name);
            return (...a) => (big ? 0n : 0);
        },
    });

    realEnv.__indirect_function_table = new WebAssembly.Table({ initial: 64, element: 'anyfunc' });
    const { instance } = await WebAssembly.instantiate(bytes, { env: new Proxy({}, h(realEnv)) });
    const mini = instance.exports;
    memory = mini.memory;
    if (typeof mini.AzStartup_resetBumpHeap === 'function') mini.AzStartup_resetBumpHeap(160 * 1024 * 1024);

    const dep = mini['__az_dep_' + DEP];
    if (!dep) { console.log('NO EXPORT __az_dep_' + DEP); process.exit(1); }

    const alloc = n => mini.AzStartup_alloc(n) >>> 0;

    // Try both plausible String layouts. cap=8 pre-allocated → no-grow push.
    for (const layout of ['ptr,cap,len', 'cap,ptr,len']) {
        const buf = alloc(8);
        const str = alloc(24);
        const d = dv();
        if (layout === 'ptr,cap,len') {
            d.setBigUint64(str, BigInt(buf), true);
            d.setBigUint64(str + 8, 8n, true);
            d.setBigUint64(str + 16, 0n, true);
        } else {
            d.setBigUint64(str, 8n, true);
            d.setBigUint64(str + 8, BigInt(buf), true);
            d.setBigUint64(str + 16, 0n, true);
        }
        // Snapshot the ENTIRE linear memory: a store through a garbage-high-bits
        // address wraps to i32 and can land anywhere in the 512 MB.
        const u8 = () => new Uint8Array(memory.buffer);
        const preAll = Buffer.from(memory.buffer.slice(0));
        let rc;
        try {
            rc = dep(BigInt(str), 0n, 0x78) >>> 0; // write_char: RCX=str, RDX(arg1)=char
        } catch (e) {
            console.log(`[${layout}] TRAPPED: ${e.message}`);
            continue;
        }
        const w = [0, 8, 16].map(o => dv().getBigUint64(str + o, true));
        const b0 = u8()[buf];
        console.log(`[${layout}] rc(RAX)=0x${rc.toString(16)}  struct=[0x${w[0].toString(16)}, 0x${w[1].toString(16)}, 0x${w[2].toString(16)}]  buf[0]=0x${b0.toString(16)}${b0 === 0x78 ? " ('x' ✓)" : ''}`);
        {
            const postAll = Buffer.from(memory.buffer.slice(0));
            let shown = 0;
            for (let o = 0; o < postAll.length && shown < 12; o++) {
                if (preAll[o] !== postAll[o]) {
                    console.log(`    DIFF addr 0x${o.toString(16)}  0x${preAll[o].toString(16)} → 0x${postAll[o].toString(16)}${postAll[o] === 0x78 ? "  ← 'x'!" : ''}`);
                    shown++;
                }
            }
            if (!shown) console.log('    (no memory diffs at all)');
            let xs = 0;
            for (let o = 0; o < postAll.length; o++) {
                if (postAll[o] === 0x78 && preAll[o] !== 0x78) {
                    console.log(`    'x' LANDED at 0x${o.toString(16)}`);
                    if (++xs >= 4) break;
                }
            }
            if (!xs) console.log("    'x' byte written NOWHERE in the entire 512 MB");
        }
    }
})().catch(e => { console.log('LAB-ERR:', e.message || e); process.exit(1); });
