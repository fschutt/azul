// M11 Sprint 3 acceptance gate.
//
// Verifies that the wasm-side patch builder (`AzStartup_buildPatch`)
// emits TLV bytes the JS decoder can parse for every Sprint 3 kind:
//   1 SetText, 2 SetAttr, 4 SetInlineStyle, 5 RemoveNode,
//   6 InsertNode, 11 AddClass.
//
// Builds each patch into a wasm-allocated buffer, copies the bytes
// out, then parses with the same TLV format the loader.js decoder
// uses (kind:u8 | node_idx:u32 LE | payload_len:u32 LE | payload).
//
// Backed by hello-world-v5.bin running on :8800.

const http = require('http');
function fetch_(p) {
    return new Promise((r,j) => http.get('http://127.0.0.1:8800' + p, x => {
        const c = []; x.on('data',b=>c.push(b)); x.on('end',()=>r(Buffer.concat(c))); x.on('error', j);
    }));
}
function fail(msg) { console.error('FAIL:', msg); process.exit(1); }

(async () => {
    const html = (await fetch_('/')).toString();
    const miniUrl = html.match(/href="(\/az\/mini\.[^"]+)"/)[1];
    const miniBytes = await fetch_(miniUrl);

    const table = new WebAssembly.Table({ initial: 64, element: 'anyfunc' });
    let memory = null;
    function memsetImpl(d, v, n) { new Uint8Array(memory.buffer).fill(v & 0xFF, d, d + n); return d; }
    function memcpyImpl(d, s, n) { new Uint8Array(memory.buffer).copyWithin(d, s, s + n); return d; }
    const realEnv = {
        __indirect_function_table: table,
        __az_resolve_callback: () => 0xFFFFFFFF,
        memset: memsetImpl, memcpy: memcpyImpl, memmove: memcpyImpl,
    };
    const AZ_MATH = { fmaxf:(a,b)=>a!==a?b:(b!==b?a:Math.max(a,b)), fminf:(a,b)=>a!==a?b:(b!==b?a:Math.min(a,b)), fmax:(a,b)=>a!==a?b:(b!==b?a:Math.max(a,b)), fmin:(a,b)=>a!==a?b:(b!==b?a:Math.min(a,b)), roundf:x=>Math.sign(x)*Math.round(Math.abs(x)), round:x=>Math.sign(x)*Math.round(Math.abs(x)), fabsf:Math.abs, fabs:Math.abs, sqrtf:Math.sqrt, sqrt:Math.sqrt, floorf:Math.floor, floor:Math.floor, ceilf:Math.ceil, ceil:Math.ceil, truncf:Math.trunc, trunc:Math.trunc, powf:Math.pow, pow:Math.pow };
        const stubFor = n => AZ_MATH[n] || (/write_memory|barrier|exception_clear/.test(n)
        ? () => {}
        : (/_f64\b/.test(n) ? () => 0 : (/_64\b/.test(n) ? () => 0n : () => 0)));
    const h = env => ({
        get: (_, p) => typeof p === 'string'
            ? (Object.prototype.hasOwnProperty.call(env, p) ? env[p] : stubFor(p))
            : undefined,
        has: () => true,
    });

    const { instance: miniI } = await WebAssembly.instantiate(miniBytes, {
        env: new Proxy({}, h(realEnv)),
    });
    const mini = miniI.exports;
    memory = mini.memory;

    if (typeof mini.AzStartup_buildPatch !== 'function') {
        fail('AzStartup_buildPatch not exported');
    }

    // Helper: write payload bytes, build patch, decode header.
    function buildAndDecode(kind, nodeIdx, payloadBytes) {
        const bufCap = 1024;
        const out = mini.AzStartup_alloc(bufCap);
        if (out === 0) fail('alloc for patch buf failed');
        let payloadPtr = 0;
        const payloadLen = payloadBytes.length;
        if (payloadLen > 0) {
            payloadPtr = mini.AzStartup_alloc(payloadLen);
            if (payloadPtr === 0) fail('alloc for payload failed');
            new Uint8Array(memory.buffer, payloadPtr, payloadLen).set(payloadBytes);
        }
        const total = mini.AzStartup_buildPatch(
            out, bufCap, kind, nodeIdx, payloadPtr, payloadLen,
        );
        if (total === 0) fail('buildPatch returned 0 for kind=' + kind);
        if (total !== 9 + payloadLen) {
            fail('buildPatch total=' + total + ' expected ' + (9 + payloadLen) +
                 ' for kind=' + kind);
        }
        // Decode header.
        const view = new DataView(memory.buffer);
        const k = view.getUint8(out);
        const n = view.getUint32(out + 1, true);
        const pl = view.getUint32(out + 5, true);
        if (k !== kind) fail('kind round-trip: built ' + kind + ', got ' + k);
        if (n !== nodeIdx) fail('node_idx round-trip: built ' + nodeIdx + ', got ' + n);
        if (pl !== payloadLen) fail('payload_len round-trip: built ' + payloadLen + ', got ' + pl);
        // Decode payload.
        const payload = new Uint8Array(memory.buffer, out + 9, payloadLen);
        for (let i = 0; i < payloadLen; i++) {
            if (payload[i] !== payloadBytes[i]) {
                fail('payload[' + i + '] round-trip: built ' + payloadBytes[i] +
                     ', got ' + payload[i] + ' for kind=' + kind);
            }
        }
        return total;
    }

    const enc = new TextEncoder();

    // Kind 1 — SetText
    buildAndDecode(1, 5, enc.encode('hello world'));
    console.log('[1] kind=1 SetText: round-trip ✓');

    // Kind 2 — SetAttr (name\0value\0)
    const attr = new Uint8Array(11);
    attr.set(enc.encode('class')); attr[5] = 0;
    attr.set(enc.encode('btn'), 6); attr[9] = 0; attr[10] = 0; // padding
    // (simpler: just round-trip arbitrary bytes; the JS decoder
    // does the name/value split, but this gate only checks bytes)
    buildAndDecode(2, 3, attr);
    console.log('[2] kind=2 SetAttr: round-trip ✓');

    // Kind 4 — SetInlineStyle
    buildAndDecode(4, 7, enc.encode('color: red;'));
    console.log('[3] kind=4 SetInlineStyle: round-trip ✓');

    // Kind 5 — RemoveNode (empty payload)
    buildAndDecode(5, 9, new Uint8Array(0));
    console.log('[4] kind=5 RemoveNode (empty): round-trip ✓');

    // Kind 6 — InsertNode (parent:u32 | html)
    const ins = new Uint8Array(4 + 5);
    new DataView(ins.buffer).setUint32(0, 2, true); // parent_idx = 2
    ins.set(enc.encode('<div>'), 4);
    buildAndDecode(6, 10, ins);
    console.log('[5] kind=6 InsertNode: round-trip ✓');

    // Kind 11 — AddClass
    buildAndDecode(11, 4, enc.encode('selected'));
    console.log('[6] kind=11 AddClass: round-trip ✓');

    console.log('\nPASS: M11 Sprint 3 patch builder round-trips 6 TLV kinds');
    process.exit(0);
})().catch(e => { console.error(e.stack); process.exit(1); });
