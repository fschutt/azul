// M11 Sprint 6 — bench runner.
//
// Bootstraps the running azul-bench-flat.bin server, instantiates
// the mini.wasm + layout.wasm + per-cb wasms, then runs a hot loop
// of dispatchEvent calls to measure round-trip latency + patch
// byte volume.
//
// Reports JSON to stdout for downstream comparison scripts.
//
// Usage:
//   ./examples/c/azul-bench-flat.bin &
//   node scripts/bench-runner.js [--ops N] [--warmup K]

const http = require('http');
function fetch_(p) {
    return new Promise((r,j) => http.get('http://127.0.0.1:8800' + p, x => {
        const c = []; x.on('data',b=>c.push(b)); x.on('end',()=>r(Buffer.concat(c))); x.on('error', j);
    }));
}

const args = new Map();
for (let i = 2; i < process.argv.length; i += 2) {
    args.set(process.argv[i].replace(/^--/, ''), process.argv[i + 1]);
}
const N_OPS = parseInt(args.get('ops') || '1000', 10);
const N_WARMUP = parseInt(args.get('warmup') || '100', 10);

(async () => {
    const html = (await fetch_('/')).toString();
    const initialCounter = parseInt(
        (html.match(/<div id="az_1">(\d+)<\/div>/) || ['', '0'])[1]);
    const typeIdMatch = html.match(/"type_id":"(\d+)"/);
    const typeId = BigInt(typeIdMatch ? typeIdMatch[1] : '0');
    const miniUrl = html.match(/href="(\/az\/mini\.[^"]+)"/)[1];
    const layoutUrl = html.match(/href="(\/az\/layout\/[^"]+)"/)[1];
    const cbUrls = [];
    const cbRe = /href="(\/az\/cb\/[^"]+)"/g;
    let m;
    while ((m = cbRe.exec(html)) !== null) cbUrls.push(m[1]);

    const [miniBytes, layoutBytes] = await Promise.all([
        fetch_(miniUrl), fetch_(layoutUrl),
    ]);
    const cbBytes = await Promise.all(cbUrls.map(u => fetch_(u)));

    const table = new WebAssembly.Table({ initial: 64, element: 'anyfunc' });
    let cbTableIdx = 0;
    let memory = null;
    function memsetImpl(d, v, n) { new Uint8Array(memory.buffer).fill(v & 0xFF, d, d + n); return d; }
    function memcpyImpl(d, s, n) { new Uint8Array(memory.buffer).copyWithin(d, s, s + n); return d; }
    const realEnv = {
        __indirect_function_table: table,
        __az_resolve_callback: () => cbTableIdx,
        memset: memsetImpl, memcpy: memcpyImpl, memmove: memcpyImpl,
    };
    const stubFor = n => /write_memory|barrier|exception_clear/.test(n)
        ? () => {} : (/(?:_64|_f64)\b/.test(n) ? () => 0n : () => 0);
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

    for (let i = 0; i < cbBytes.length; i++) {
        const { instance: cbI } = await WebAssembly.instantiate(cbBytes[i], {
            env: new Proxy({}, h(cbEnv)),
        });
        table.grow(1);
        const slot = table.length - 1;
        table.set(slot, cbI.exports.callback);
        if (i === 0) cbTableIdx = slot; // route resolveCallback to first cb
    }
    const { instance: layoutI } = await WebAssembly.instantiate(layoutBytes, {
        env: new Proxy({}, h(cbEnv)),
    });
    table.grow(1);
    const layoutTableIdx = table.length - 1;
    table.set(layoutTableIdx, layoutI.exports.callback);

    const state = mini.AzStartup_init(0, 0);
    const modelPtr = mini.AzStartup_alloc(16);
    // BenchModel = { next_id: u32, selected_id: u32, row_count: u32 }
    const mView = new DataView(memory.buffer);
    mView.setUint32(modelPtr, 1, true);      // next_id
    mView.setUint32(modelPtr + 4, 0, true);  // selected_id
    mView.setUint32(modelPtr + 8, 0, true);  // row_count
    const refanyPtr = mini.AzStartup_hydrate(
        Number(typeId & 0xFFFFFFFFn),
        Number((typeId >> 32n) & 0xFFFFFFFFn),
        modelPtr, 16,
    );
    mini.AzStartup_setRefAny(state, refanyPtr);
    mini.AzStartup_setLayoutCbTableIdx(state, layoutTableIdx);
    mini.AzStartup_setModelPtr(state, modelPtr);
    mini.AzStartup_registerCbNode(state, 0);
    mini.AzStartup_setDisplayNode(state, 0);

    const initRc = mini.AzStartup_initLayoutCache(state, 800, 600, 0);
    if (initRc !== 0) {
        console.error('initLayoutCache rc=' + initRc);
        process.exit(1);
    }
    mini.AzStartup_hydrateStyledDom(state);
    mini.AzStartup_solveLayout(state, 800, 600);

    // Event buffer (reused).
    const evtPtr = mini.AzStartup_alloc(256);
    const outLenPtr = mini.AzStartup_alloc(4);
    const dv = new DataView(memory.buffer);
    function setEvent(x, y) {
        dv.setUint32(evtPtr + 0,  0xFFFFFFFF, true);
        dv.setUint32(evtPtr + 4,  x, true);
        dv.setUint32(evtPtr + 8,  y, true);
        dv.setUint32(evtPtr + 12, 0, true);
        dv.setUint32(evtPtr + 16, 0, true);
    }

    // Warm up.
    setEvent(10, 5);
    for (let i = 0; i < N_WARMUP; i++) {
        mini.AzStartup_dispatchEvent(state, 0, evtPtr, 256, outLenPtr);
    }

    // Measure.
    let totalPatchBytes = 0;
    const t0 = process.hrtime.bigint();
    for (let i = 0; i < N_OPS; i++) {
        mini.AzStartup_dispatchEvent(state, 0, evtPtr, 256, outLenPtr);
        totalPatchBytes += dv.getUint32(outLenPtr, true);
    }
    const t1 = process.hrtime.bigint();
    const elapsed_ns = Number(t1 - t0);
    const elapsed_ms = elapsed_ns / 1e6;
    const per_op_us = (elapsed_ns / N_OPS) / 1e3;

    const counter = dv.getUint32(modelPtr + 8, true); // row_count after dispatches

    const out = {
        bench: 'azul-flat',
        n_ops: N_OPS,
        warmup: N_WARMUP,
        elapsed_ms: elapsed_ms.toFixed(2),
        per_op_us: per_op_us.toFixed(3),
        ops_per_sec: Math.round(1e6 / per_op_us),
        total_patch_bytes: totalPatchBytes,
        avg_patch_bytes: (totalPatchBytes / N_OPS).toFixed(2),
        final_row_count: counter,
    };
    console.log(JSON.stringify(out, null, 2));
    process.exit(0);
})().catch(e => { console.error(e.stack); process.exit(1); });
