// fmt-lab4.js — run AzStartup_probeFmt in the lab with the FULL helper env
// (__multi3/__udivti3 included — float fmt needs the 128-bit helpers), then
// dump the missing-block recorder (count @0x400FC, last PC @0x400F8, ring
// @0x40160 x16) and the dispatch counters. The width-pad stage's Err comes
// from a SILENTLY RETURNING missing_block; the ring names its PC.
const fs = require('fs');
const [MINI] = process.argv.slice(2);

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

    const ringAt = () => [...Array(16)].map((_, i) => dv().getUint32(0x40160 + i * 4, true));
    const cntAt = () => dv().getUint32(0x400FC, true);
    const preCnt = cntAt(); const preRing = ringAt();
    let rc;
    try { rc = mini.AzStartup_probeFmt(255) >>> 0; } catch (e) { rc = 'TRAP:' + e.message; }
    const marks = [...Array(13)].map((_, i) => dv().getUint32(0x40910 + i * 4, true).toString(16));
    console.log('probeFmt rc=' + rc);
    console.log('marks:', marks.join(' '));
    console.log(`missing_block count ${preCnt} → ${cntAt()}, last PC=0x${dv().getUint32(0x400F8, true).toString(16)}`);
    const post = ringAt();
    const newPCs = post.filter((v, i) => v !== preRing[i] && v !== 0);
    console.log('ring new PCs:', newPCs.map(v => '0x' + v.toString(16)).join(' ') || '(none)');
    console.log(`unk=${dv().getUint32(0x40158, true)} upc=0x${dv().getUint32(0x40900, true).toString(16)} weak=${dv().getUint32(0x4041C, true)}`);
})().catch(e => { console.log('LAB-ERR:', e.message || e); process.exit(1); });
