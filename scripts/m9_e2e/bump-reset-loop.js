// M10-C1 acceptance gate: 100 dispatch cycles in a row should leave
// `@__az_bump_ptr` unchanged (modulo the snapshot we take after the
// persistent setup). This validates that JS-driven snapshot/reset
// round-tripping actually clamps the wasm bump heap and that the
// dispatch path tolerates repeated reset → re-bump sequences without
// state corruption.
//
// Driven against `hello-world-v5.bin` (the v5 layout-cb gate baseline
// used by full-cycle.js). Boots once, snapshots after setup, then
// loops N times: synthesize click, dispatch, read patch, verify
// counter increment, reset.
//
// Run AFTER starting the server, same env knobs as full-cycle.js.
const http = require('http');
function fetch_(p) {
    return new Promise((r, j) => http.get('http://127.0.0.1:8800' + p, x => {
        const c = []; x.on('data', b => c.push(b)); x.on('end', () => r(Buffer.concat(c))); x.on('error', j);
    }));
}
function failSync(msg) { console.error('FAIL:', msg); process.exit(1); }

const CYCLES = parseInt(process.env.AZ_CYCLES || '100', 10);

(async () => {
    const html = (await fetch_('/')).toString();
    const initialCounter = parseInt((html.match(/<div id="az_1">(\d+)<\/div>/) || ['', '5'])[1]);
    const typeId = BigInt((html.match(/"type_id":"(\d+)"/) || ['', '0'])[1]);
    const miniUrl = html.match(/href="(\/az\/mini\.[^"]+)"/)[1];
    const layoutUrl = html.match(/href="(\/az\/layout\/[^"]+)"/)[1];
    const cbMatch = html.match(/href="(\/az\/cb\/[^"]+)"/);
    const cbUrl = cbMatch ? cbMatch[1] : null;

    const [miniBytes, layoutBytes, cbBytes] = await Promise.all([
        fetch_(miniUrl), fetch_(layoutUrl), cbUrl ? fetch_(cbUrl) : Promise.resolve(null),
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
    const stubFor = n => /write_memory|barrier|exception_clear/.test(n)
        ? () => {}
        : (/_f64\b/.test(n) ? () => 0 : (/_64\b/.test(n) ? () => 0n : () => 0));
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
        table.grow(1);
        cbTableIdx = table.length - 1;
        table.set(cbTableIdx, cbI.exports.callback);
    }
    const { instance: layoutI } = await WebAssembly.instantiate(layoutBytes, {
        env: new Proxy({}, h(cbEnv)),
    });
    table.grow(1);
    const layoutTableIdx = table.length - 1;
    table.set(layoutTableIdx, layoutI.exports.callback);

    if (typeof mini.AzStartup_snapshotBumpHeap !== 'function') {
        failSync('mini.AzStartup_snapshotBumpHeap not exported (M10-C1 missing?)');
    }
    if (typeof mini.AzStartup_resetBumpHeap !== 'function') {
        failSync('mini.AzStartup_resetBumpHeap not exported (M10-C1 missing?)');
    }

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

    const layoutRc = mini.AzStartup_initLayoutCache(state, 800, 600, 0);
    if (layoutRc !== 0) failSync('initLayoutCache rc=' + layoutRc);

    // Snapshot AFTER all the persistent setup is done. Subsequent
    // dispatches will allocate transient (event + patch) buffers in the
    // bump heap; we reset to this snapshot at the end of each cycle
    // so the heap doesn't grow unboundedly.
    const snapshot = mini.AzStartup_snapshotBumpHeap();
    console.log('[bump-loop] snapshot at boot: ' + snapshot);

    let bumpDriftMax = 0;
    let counter = initialCounter;
    for (let i = 0; i < CYCLES; i++) {
        const evtLen = 20;
        const evtPtr = mini.AzStartup_alloc(evtLen);
        const evtDv = new DataView(memory.buffer, evtPtr, evtLen);
        evtDv.setUint32(0, 0xFFFFFFFF, true);
        evtDv.setFloat32(4, 100.0, true);
        evtDv.setFloat32(8, 100.0, true);
        evtDv.setUint32(12, 0, true);
        evtDv.setUint32(16, 0, true);

        const outLenPtr = mini.AzStartup_alloc(4);
        new DataView(memory.buffer).setUint32(outLenPtr, 0, true);
        const counterBefore = new DataView(memory.buffer).getUint32(modelPtr, true);
        const patchPtr = mini.AzStartup_dispatchEvent(state, 0, evtPtr, evtLen, outLenPtr);
        const patchLen = new DataView(memory.buffer).getUint32(outLenPtr, true);
        const counterAfter = new DataView(memory.buffer).getUint32(modelPtr, true);

        if (counterAfter !== counterBefore + 1) {
            failSync('cycle ' + i + ': counter did not increment ('
                + counterBefore + ' → ' + counterAfter + ')');
        }
        if (patchPtr === 0 || patchLen === 0) {
            failSync('cycle ' + i + ': no patch emitted');
        }
        // Validate patch payload BEFORE resetting (it lives in the
        // about-to-be-reclaimed region of the bump heap).
        const patchU8 = new Uint8Array(memory.buffer, patchPtr, patchLen);
        const payloadLen = new DataView(memory.buffer, patchPtr + 5, 4).getUint32(0, true);
        const payload = Buffer.from(patchU8.subarray(9, 9 + payloadLen)).toString('utf8');
        if (parseInt(payload) !== counterAfter) {
            failSync('cycle ' + i + ': patch payload "' + payload + '" != counter ' + counterAfter);
        }
        counter = counterAfter;

        // Reset the bump heap. Next cycle's allocations restart at the
        // snapshot offset so the heap doesn't grow.
        mini.AzStartup_resetBumpHeap(snapshot);

        // Snapshot after reset — should equal the saved snapshot.
        const afterReset = mini.AzStartup_snapshotBumpHeap();
        const drift = afterReset - snapshot;
        if (drift !== 0) {
            failSync('cycle ' + i + ': bump drift after reset: ' + drift
                + ' (snapshot=' + snapshot + ', after=' + afterReset + ')');
        }
        if (Math.abs(drift) > bumpDriftMax) bumpDriftMax = Math.abs(drift);
    }

    console.log('[bump-loop] cycles=' + CYCLES + ' counter ' + initialCounter
        + ' → ' + counter + ', max bump drift after reset: ' + bumpDriftMax);
    if (counter !== initialCounter + CYCLES) {
        failSync('expected counter ' + (initialCounter + CYCLES) + ', got ' + counter);
    }
    console.log('\nPASS: ' + CYCLES + ' dispatch cycles, bump pointer stable at snapshot');
    process.exit(0);
})().catch(e => { console.error(e.stack); process.exit(1); });
