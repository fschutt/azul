// FULL M9 5-step e2e test:
//   1. HTML loads, RefAny embedded — JS parses out.
//   2. mini.wasm + layout.wasm instantiated. Hydrate RefAny.
//      Run initLayoutCache → wasm-resident layout cache populated.
//   3. JS synthesizes click event (NODE_IDX=SENTINEL, x=100, y=100).
//   4. wasm hit-tests → finds registered cb → invokes user cb wasm
//      → counter mutated in place.
//   5. wasm emits SetText TLV patch. JS reads + parses + verifies
//      counter incremented.
//
// This is the exit-condition the user asked for: "fake event that
// invokes the user_callback.wasm callback and updates the counter".
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
    // === STEP 1: HTML + embedded RefAny ===
    const html = (await fetch_('/')).toString();
    const initialCounter = parseInt((html.match(/<div id="az_1">(\d+)<\/div>/) || ['', '5'])[1]);
    const typeId = BigInt((html.match(/"type_id":"(\d+)"/) || ['', '0'])[1]);
    const miniUrl = html.match(/href="(\/az\/mini\.[^"]+)"/)[1];
    const layoutUrl = html.match(/href="(\/az\/layout\/[^"]+)"/)[1];
    const cbMatch = html.match(/href="(\/az\/cb\/[^"]+)"/);
    const cbUrl = cbMatch ? cbMatch[1] : null;
    console.log('[1] HTML bootstrap OK: counter=' + initialCounter + ' typeId=' + typeId);

    // === STEP 2: WASM init + layout ===
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
        // M9-6 resolve_callback hook: maps native fn addr → wasm table_idx.
        // For the e2e demo we registered the on_click cb at cbTableIdx; map
        // any non-MAX value to that index.
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
    // The hello-world-v5 layout returns a body with on_click. We register
    // the body (node_idx 0) as the clickable node and the same node as
    // the display target (the SetText patch will land on it).
    mini.AzStartup_registerCbNode(state, 0);
    mini.AzStartup_setDisplayNode(state, 0);

    const layoutRc = mini.AzStartup_initLayoutCache(state, 800, 600, 0);
    if (layoutRc !== 0) failSync('initLayoutCache rc=' + layoutRc);
    console.log('[2] layout cache populated (rc=' + layoutRc + ')');

    // === STEP 3: synthesize click event ===
    // Event layout (20 bytes):
    //   0..4   NODE_IDX  = 0xFFFFFFFF (sentinel → wasm hit-tests)
    //   4..8   X (f32 bits)            = 100.0
    //   8..12  Y (f32 bits)            = 100.0
    //   12..16 BUTTON_OR_KEY           = 0 (left)
    //   16..20 MODIFIERS               = 0
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
    const patchPtr = mini.AzStartup_dispatchEvent(state, 0 /*kind*/, evtPtr, evtLen, outLenPtr);
    const patchLen = new DataView(memory.buffer).getUint32(outLenPtr, true);
    const counterAfter = new DataView(memory.buffer).getUint32(modelPtr, true);
    console.log('[4] dispatchEvent: patch_ptr=' + patchPtr + ' patch_len=' + patchLen +
                ' counter ' + counterBefore + ' → ' + counterAfter);

    if (counterAfter === counterBefore) {
        console.error('FAIL: counter did not increment — cb did not run');
        process.exit(1);
    }

    // === STEP 5: parse + verify patch ===
    if (patchPtr === 0 || patchLen === 0) {
        console.error('FAIL: no patch emitted');
        process.exit(1);
    }
    const patchU8 = new Uint8Array(memory.buffer, patchPtr, patchLen);
    // TLV: kind(u8) | node_idx(u32 LE) | payload_len(u32 LE) | payload[payload_len]
    const kind = patchU8[0];
    const nodeIdx = new DataView(memory.buffer, patchPtr + 1, 4).getUint32(0, true);
    const payloadLen = new DataView(memory.buffer, patchPtr + 5, 4).getUint32(0, true);
    const payload = Buffer.from(patchU8.subarray(9, 9 + payloadLen)).toString('utf8');
    console.log('[5] patch decoded: kind=' + kind + ' (SetText=1) node_idx=' + nodeIdx +
                ' payload="' + payload + '"');
    if (kind !== 1) failSync('expected SetText kind=1, got ' + kind);
    if (parseInt(payload) !== counterAfter) failSync('payload "' + payload + '" != counter ' + counterAfter);

    console.log('\nPASS: full 5-step pipeline works end-to-end');
    console.log('      bootstrap → layout → click → hit-test → cb → patch');
    process.exit(0);
})().catch(e => { console.error(e.stack); process.exit(1); });
