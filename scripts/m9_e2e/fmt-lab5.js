// fmt-lab5.js — test Display<String-VARIABLE> as a real rt::Argument, the one
// fmt family the probe never exercised (the {str} stage was compile-folded to
// pieces-only). Craft the full fmt::write call: hand-built Arguments + pieces +
// rt::Argument in guest memory, real vtable, both String-fmt thunk candidates.
//
// fmt::write ABI (read from probeFmt's native): RCX = &out_String (the dyn
// data ptr), RDX = dyn-Write VTABLE, R8 = &Arguments.
// Wrapper ABI: arg0_lo->RCX, arg0_hi->R8, arg1(i32)->RDX.
// Arguments layout (from probeFmt's native): {+0 pieces_ptr, +8 pieces_len,
// +0x10 args_ptr, +0x18 args_len, +0x20 placeholders=0}.
const fs = require('fs');
const [MINI] = process.argv.slice(2);
const DEP_FMT_WRITE = '7ffd0d3a7040';
const VTABLE_SYNTH = 0x16FC8D8;
const CANDIDATES = [['impl$23', 0x26DE40], ['impl$24', 0x2D63E0]];

(async () => {
    const bytes = fs.readFileSync(MINI);
    let memory = null;
    const dv = () => new DataView(memory.buffer);
    const azMulti3 = (sret, aLo, aHi, bLo, bHi) => {
        const d = dv(); const mask = 0xFFFFFFFFFFFFFFFFn;
        const a = (BigInt.asUintN(64, BigInt(aHi)) << 64n) | BigInt.asUintN(64, BigInt(aLo));
        const b = (BigInt.asUintN(64, BigInt(bHi)) << 64n) | BigInt.asUintN(64, BigInt(bLo));
        const p = BigInt.asUintN(128, a * b);
        d.setBigUint64(Number(sret), p & mask, true);
        d.setBigUint64(Number(sret) + 8, (p >> 64n) & mask, true);
    };
    const azUdivti3 = (sret, aLo, aHi, bLo, bHi) => {
        const d = dv(); const mask = 0xFFFFFFFFFFFFFFFFn;
        const a = (BigInt.asUintN(64, BigInt(aHi)) << 64n) | BigInt.asUintN(64, BigInt(aLo));
        const b = (BigInt.asUintN(64, BigInt(bHi)) << 64n) | BigInt.asUintN(64, BigInt(bLo));
        const q = b === 0n ? 0n : (a / b);
        d.setBigUint64(Number(sret), q & mask, true);
        d.setBigUint64(Number(sret) + 8, (q >> 64n) & mask, true);
    };
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
        __multi3: azMulti3, __udivti3: azUdivti3,
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
    const dep = mini['__az_dep_' + DEP_FMT_WRITE];
    if (!dep) { console.log('NO fmt::write EXPORT'); process.exit(1); }
    const u8 = () => new Uint8Array(memory.buffer);

    for (const [name, fnSynth] of CANDIDATES) {
        const d = dv();
        // out String s = (cap 32, ptr buf, len 0)
        const buf = alloc(32);
        const s = alloc(24);
        d.setBigUint64(s, 32n, true);
        d.setBigUint64(s + 8, BigInt(buf), true);
        d.setBigUint64(s + 16, 0n, true);
        // dom_name String = "Div"
        const nameBuf = alloc(8);
        u8().set(Buffer.from('Div'), nameBuf);
        const nameStr = alloc(24);
        d.setBigUint64(nameStr, 8n, true);
        d.setBigUint64(nameStr + 8, BigInt(nameBuf), true);
        d.setBigUint64(nameStr + 16, 3n, true);
        // piece "P"
        const pieceBuf = alloc(4);
        u8().set(Buffer.from('P'), pieceBuf);
        const pieces = alloc(16);
        d.setBigUint64(pieces, BigInt(pieceBuf), true);
        d.setBigUint64(pieces + 8, 1n, true);
        // rt::Argument { value=&nameStr, fn=fnSynth }
        const rtArg = alloc(16);
        d.setBigUint64(rtArg, BigInt(nameStr), true);
        d.setBigUint64(rtArg + 8, BigInt(fnSynth), true);
        // Arguments
        const args = alloc(40);
        d.setBigUint64(args, BigInt(pieces), true);
        d.setBigUint64(args + 8, 1n, true);
        d.setBigUint64(args + 16, BigInt(rtArg), true);
        d.setBigUint64(args + 24, 1n, true);
        d.setBigUint64(args + 32, 0n, true);

        let rc;
        try { rc = dep(BigInt(s), BigInt(args), VTABLE_SYNTH) >>> 0; }
        catch (e) { console.log(`[${name}] TRAPPED: ${e.message}`); continue; }
        const len = Number(d.getBigUint64(s + 16, true));
        const text = Buffer.from(u8().slice(buf, buf + Math.min(len, 32))).toString('utf8');
        console.log(`[${name} @0x${fnSynth.toString(16)}] rc(AL)=0x${(rc & 0xff).toString(16)}  s.len=${len}  s="${text}"  ${rc === 0 && text === 'PDiv' ? '✓ CORRECT' : '✗'}`);
    }
})().catch(e => { console.log('LAB-ERR:', e.message || e); process.exit(1); });
