// M11 Sprint 2 acceptance gate.
//
// Verifies AzStartup_hitTest walks the positioned-rect cache that
// AzStartup_solveLayout populated, and returns the correct node_idx
// when the synthesized point falls inside a rect.
//
// Layout strategy (S1.C placeholder): each node gets a rect
// `(0, node_idx * 30, viewport_w, 30)`. So:
//   - Click at (10, 5) → node 0.
//   - Click at (10, 35) → node 1 (if exists).
//   - Click at (10, 1000) → no rect → fallback to last registered.
//
// Backed by hello-world-v5.bin (1 node — body — running on :8800).

const http = require('http');
function fetch_(p) {
    return new Promise((r,j) => http.get('http://127.0.0.1:8800' + p, x => {
        const c = []; x.on('data',b=>c.push(b)); x.on('end',()=>r(Buffer.concat(c))); x.on('error', j);
    }));
}
function fail(msg) { console.error('FAIL:', msg); process.exit(1); }

(async () => {
    const html = (await fetch_('/')).toString();
    const initialCounter = parseInt((html.match(/<div id="az_1">(\d+)<\/div>/) || ['', '5'])[1]);
    const typeId = BigInt((html.match(/"type_id":"(\d+)"/) || ['', '0'])[1]);
    const miniUrl = html.match(/href="(\/az\/mini\.[^"]+)"/)[1];
    const layoutUrl = html.match(/href="(\/az\/layout\/[^"]+)"/)[1];
    const cbMatch = html.match(/href="(\/az\/cb\/[^"]+)"/);
    const cbUrl = cbMatch ? cbMatch[1] : null;

    const [miniBytes, layoutBytes, cbBytes] = await Promise.all([
        fetch_(miniUrl),
        fetch_(layoutUrl),
        cbUrl ? fetch_(cbUrl) : Promise.resolve(null),
    ]);

    const table = new WebAssembly.Table({ initial: 64, element: 'anyfunc' });
    let cbTableIdx = -1;
    let memory = null;
    function memsetImpl(d, v, n) { new Uint8Array(memory.buffer).fill(v & 0xFF, d, d + n); return d; }
    function memcpyImpl(d, s, n) { new Uint8Array(memory.buffer).copyWithin(d, s, s + n); return d; }
    const realEnv = {
        __indirect_function_table: table,
        __az_resolve_callback: (addr) => {
            if (addr === 0xFFFFFFFF) return 0xFFFFFFFF;
            return cbTableIdx >= 0 ? cbTableIdx : 0xFFFFFFFF;
        },
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
    const cbEnv = { memory, __indirect_function_table: table, memset: memsetImpl, memcpy: memcpyImpl, memmove: memcpyImpl };

    if (cbBytes) {
        const { instance: cbI } = await WebAssembly.instantiate(cbBytes, {
            env: new Proxy({}, h(cbEnv)),
        });
        table.grow(1); cbTableIdx = table.length - 1;
        table.set(cbTableIdx, cbI.exports.callback);
    }
    const { instance: layoutI } = await WebAssembly.instantiate(layoutBytes, {
        env: new Proxy({}, h(cbEnv)),
    });
    table.grow(1);
    const layoutTableIdx = table.length - 1;
    table.set(layoutTableIdx, layoutI.exports.callback);

    const state = mini.AzStartup_init(0, 0);
    const modelPtr = mini.AzStartup_alloc(4);
    new DataView(memory.buffer).setUint32(modelPtr, initialCounter, true);
    const refanyPtr = mini.AzStartup_hydrate(
        Number(typeId & 0xFFFFFFFFn),
        Number((typeId >> 32n) & 0xFFFFFFFFn),
        modelPtr, 4,
    );
    mini.AzStartup_setRefAny(state, refanyPtr);
    mini.AzStartup_setLayoutCbTableIdx(state, layoutTableIdx);
    mini.AzStartup_setModelPtr(state, modelPtr);
    mini.AzStartup_registerCbNode(state, 0);
    mini.AzStartup_setDisplayNode(state, 0);

    const viewportW = 800;
    const viewportH = 600;
    if (mini.AzStartup_initLayoutCache(state, viewportW, viewportH, 0) !== 0)
        fail('initLayoutCache failed');
    if (mini.AzStartup_hydrateStyledDom(state) !== 0)
        fail('hydrateStyledDom failed');
    if (mini.AzStartup_solveLayout(state, viewportW, viewportH) !== 0)
        fail('solveLayout failed');
    const rectsLen = mini.AzStartup_getPositionedRectsLen(state);
    if (rectsLen < 1) fail('positioned rects len=' + rectsLen);
    console.log('[1] layout solved: rects_len=' + rectsLen);

    // === Inspect the rect cache directly via JS ===
    const rectsPtr = mini.AzStartup_getPositionedRectsPtr(state);
    if (rectsPtr === 0) fail('positioned rects ptr=0');
    const dv = new DataView(memory.buffer);
    for (let i = 0; i < rectsLen && i < 4; i++) {
        const off = rectsPtr + i * 16;
        const x = dv.getUint32(off, true);
        const y = dv.getUint32(off + 4, true);
        const w = dv.getUint32(off + 8, true);
        const hh = dv.getUint32(off + 12, true);
        console.log('  rect[' + i + ']: x=' + x + ' y=' + y + ' w=' + w + ' h=' + hh);
    }

    // === Hit-test at known-inside point: (10, 5) → node 0 ===
    // M11 Sprint 2 wire format: encode coords as plain u32 pixels
    // (Math.floor of clientX/clientY), not f32 bits.
    const inside = mini.AzStartup_hitTest(state, 10, 5);
    if (inside !== 0) {
        fail('hit-test at (10, 5) returned ' + inside + '; expected 0 (node 0)');
    }
    console.log('[2] hit-test at (10, 5) → node ' + inside + ' ✓');

    // === Hit-test at known-outside point (large y) ===
    // Should fall back to last_registered_cb_node_idx = 0.
    const outside = mini.AzStartup_hitTest(state, 10, 99999);
    console.log('[3] hit-test at (10, 99999) → node ' + outside +
                ' (fallback to last-registered)');

    console.log('\nPASS: M11 Sprint 2 hit-test walks positioned rects');
    process.exit(0);
})().catch(e => { console.error(e.stack); process.exit(1); });
