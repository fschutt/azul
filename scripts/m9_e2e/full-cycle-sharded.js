// M10-D: full 5-step e2e test against the SHARDED build mode.
//
// Differences from full-cycle.js (which targets the bundled mode):
//
//   1. Fetches /az/manifest.json to discover the shard URL set
//      (mini + layout + callbacks + boundaries).
//   2. Pre-loads every boundary shard and registers each shard's
//      `body_export` (`sub_<canonical_synth_hex>`) into the shared
//      env namespace so cb / layout wasms can resolve their
//      env-imports to real bodies instead of stub-noops.
//   3. Otherwise mirrors the same 5-step bootstrap → layout → click
//      → cb → patch cycle as full-cycle.js.
//
// Pre-conditions: server must have been launched with
//   AZ_ENABLE_SHARDS=1 (and AZ_BUNDLED_LEGACY must NOT be set) so
//   the api.json Framework symbols classify as BoundaryImport and
//   ship as separate /az/fn/<name>.<hash>.wasm shards.
//
// Backed by hello-world-v5.bin (body + AzDom_addCallback on_click).
const http = require('http');
function fetch_(p) {
    return new Promise((r,j) => http.get('http://127.0.0.1:8800' + p, x => {
        const c = []; x.on('data',b=>c.push(b)); x.on('end',()=>r(Buffer.concat(c))); x.on('error', j);
    }));
}

function failSync(msg) { console.error('FAIL:', msg); process.exit(1); }

(async () => {
    // === STEP 0: Manifest discovery ===
    const manifestBytes = await fetch_('/az/manifest.json');
    let manifest;
    try {
        manifest = JSON.parse(manifestBytes.toString());
    } catch (e) {
        failSync('manifest parse: ' + e.message);
    }
    if (manifest.version !== 1) {
        failSync('manifest version mismatch: ' + manifest.version);
    }
    console.log('[0] manifest OK: ' + manifest.boundaries.length + ' boundaries, ' +
                manifest.callbacks.length + ' callbacks, ' +
                manifest.layout.length + ' layout wasms');

    // === STEP 1: HTML + embedded RefAny ===
    const html = (await fetch_('/')).toString();
    const initialCounter = parseInt((html.match(/<div id="az_1">(\d+)<\/div>/) || ['', '5'])[1]);
    const typeId = BigInt((html.match(/"type_id":"(\d+)"/) || ['', '0'])[1]);
    console.log('[1] HTML bootstrap OK: counter=' + initialCounter + ' typeId=' + typeId);

    // === STEP 2: Set up shared env, instantiate mini, load boundaries ===
    const table = new WebAssembly.Table({ initial: 64, element: 'anyfunc' });
    let cbTableIdx = -1;
    let memory = null;
    function memsetImpl(d, v, n) { new Uint8Array(memory.buffer).fill(v & 0xFF, d, d + n); return d; }
    function memcpyImpl(d, s, n) { new Uint8Array(memory.buffer).copyWithin(d, s, s + n); return d; }

    // Boundary symbol map populated as shards instantiate.
    const boundarySymbols = new Map();

    const realEnv = {
        __indirect_function_table: table,
        __az_resolve_callback: (addr) => {
            if (addr === 0xFFFFFFFF) return 0xFFFFFFFF;
            return cbTableIdx >= 0 ? cbTableIdx : 0xFFFFFFFF;
        },
        memset: memsetImpl,
        memcpy: memcpyImpl,
        memmove: memcpyImpl,
    };
    const stubFor = n => /write_memory|barrier|exception_clear/.test(n)
        ? () => {}
        : (/(?:_64|_f64)\b/.test(n) ? () => 0n : () => 0);
    const envHandler = {
        get: (_, p) => {
            if (typeof p !== 'string') return undefined;
            if (Object.prototype.hasOwnProperty.call(realEnv, p)) return realEnv[p];
            // M10-D: prefer boundary shard exports over stub-noops.
            if (boundarySymbols.has(p)) return boundarySymbols.get(p);
            return stubFor(p);
        },
        has: () => true,
    };
    const envProxy = { env: new Proxy({}, envHandler) };

    const miniBytes = await fetch_(manifest.mini.url);
    const { instance: miniI } = await WebAssembly.instantiate(miniBytes, envProxy);
    const mini = miniI.exports;
    memory = mini.memory;
    realEnv.memory = memory;
    console.log('[2a] mini.wasm instantiated (' + miniBytes.length + ' bytes)');

    // M10-D: parallel-fetch + instantiate every boundary shard. Each
    // shard's `body_export` becomes an entry in boundarySymbols so
    // subsequent cb / layout instantiations resolve their
    // env-imports through it.
    let boundaryTotalBytes = 0;
    await Promise.all(manifest.boundaries.map(async (b) => {
        const bytes = await fetch_(b.url);
        boundaryTotalBytes += bytes.length;
        const { instance } = await WebAssembly.instantiate(bytes, envProxy);
        const bodyFn = instance.exports[b.body_export];
        if (typeof bodyFn !== 'function') {
            failSync('boundary ' + b.name + ' missing export ' + b.body_export);
        }
        boundarySymbols.set(b.body_export, bodyFn);
    }));
    console.log('[2b] ' + boundarySymbols.size + ' boundary shards loaded (' +
                boundaryTotalBytes + ' bytes total)');

    // Cb + layout wasms — resolve via manifest.
    let cbBytes = null;
    if (manifest.callbacks.length > 0) {
        cbBytes = await fetch_(manifest.callbacks[0].url);
        const { instance: cbI } = await WebAssembly.instantiate(cbBytes, envProxy);
        table.grow(1);
        cbTableIdx = table.length - 1;
        table.set(cbTableIdx, cbI.exports.callback);
    }
    if (manifest.layout.length === 0) failSync('manifest has no layout wasm');
    const layoutBytes = await fetch_(manifest.layout[0].url);
    const { instance: layoutI } = await WebAssembly.instantiate(layoutBytes, envProxy);
    table.grow(1);
    const layoutTableIdx = table.length - 1;
    table.set(layoutTableIdx, layoutI.exports.callback);
    console.log('[2c] cb + layout instantiated (cb=' +
                (cbBytes ? cbBytes.length : 0) + ', layout=' + layoutBytes.length + ' bytes)');

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
    console.log('[2d] layout cache populated (rc=' + layoutRc + ')');

    // === STEP 3: synthesize click event ===
    const evtLen = 20;
    const evtPtr = mini.AzStartup_alloc(evtLen);
    const evtDv = new DataView(memory.buffer, evtPtr, evtLen);
    evtDv.setUint32(0, 0xFFFFFFFF, true);
    evtDv.setFloat32(4, 100.0, true);
    evtDv.setFloat32(8, 100.0, true);
    evtDv.setUint32(12, 0, true);
    evtDv.setUint32(16, 0, true);
    console.log('[3] click event synthesized at (100,100) with NODE_IDX=SENTINEL');

    // === STEP 4: dispatch → hit-test → cb invoke ===
    const outLenPtr = mini.AzStartup_alloc(4);
    new DataView(memory.buffer).setUint32(outLenPtr, 0, true);
    const counterBefore = new DataView(memory.buffer).getUint32(modelPtr, true);
    const patchPtr = mini.AzStartup_dispatchEvent(state, 0, evtPtr, evtLen, outLenPtr);
    const patchLen = new DataView(memory.buffer).getUint32(outLenPtr, true);
    const counterAfter = new DataView(memory.buffer).getUint32(modelPtr, true);
    console.log('[4] dispatchEvent: patch_ptr=' + patchPtr + ' patch_len=' + patchLen +
                ' counter ' + counterBefore + ' → ' + counterAfter);

    if (counterAfter === counterBefore) {
        failSync('counter did not increment — cb did not run');
    }

    // === STEP 5: parse + verify patch ===
    if (patchPtr === 0 || patchLen === 0) failSync('no patch emitted');
    const patchU8 = new Uint8Array(memory.buffer, patchPtr, patchLen);
    const kind = patchU8[0];
    const nodeIdx = new DataView(memory.buffer, patchPtr + 1, 4).getUint32(0, true);
    const payloadLen = new DataView(memory.buffer, patchPtr + 5, 4).getUint32(0, true);
    const payload = Buffer.from(patchU8.subarray(9, 9 + payloadLen)).toString('utf8');
    console.log('[5] patch decoded: kind=' + kind + ' (SetText=1) node_idx=' + nodeIdx +
                ' payload="' + payload + '"');
    if (kind !== 1) failSync('expected SetText kind=1, got ' + kind);
    if (parseInt(payload) !== counterAfter) failSync('payload "' + payload + '" != counter ' + counterAfter);

    console.log('\nPASS: SHARDED full 5-step pipeline works end-to-end');
    console.log('      manifest → mini → boundaries → cb → layout → dispatch → patch');
    process.exit(0);
})().catch(e => { console.error(e.stack); process.exit(1); });
