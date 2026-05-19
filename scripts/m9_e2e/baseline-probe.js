// M12.5d baseline + diagnostic probe.
//
// Bootstraps hello-world-v5 exactly like styled-dom-hydrate.js, runs
// initLayoutCache + hydrateStyledDom, then dumps the fixed-address
// diagnostic regions that `AzStartup_hydrateStyledDom` writes:
//
//   0x40018 -> test_ptr (make_test_struct heap)            [sret oracle]
//   0x4010C -> ptr_val  (boxed StyledDom heap ptr)
//   0x40200 -> first 80 bytes of the cascade-output struct [cascade]
//
// Prints:
//   - make_test_struct: nonzero count / 64 + pattern-correct count / 64
//   - cascade output: nonzero u32 count in first 80 bytes + hex dump
//   - getStyledDomNodeCount / getStyledDomPtr
//
// Usage: node scripts/m9_e2e/baseline-probe.js   (server on :8800)

const http = require('http');
function fetch_(p) {
    return new Promise((r, j) => http.get('http://127.0.0.1:8800' + p, x => {
        const c = []; x.on('data', b => c.push(b)); x.on('end', () => r(Buffer.concat(c))); x.on('error', j);
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
        fetch_(miniUrl), fetch_(layoutUrl),
        cbUrl ? fetch_(cbUrl) : Promise.resolve(null),
    ]);

    const table = new WebAssembly.Table({ initial: 64, element: 'anyfunc' });
    let cbTableIdx = -1;
    let memory = null;
    function memsetImpl(d, v, n) { new Uint8Array(memory.buffer).fill(v & 0xFF, d, d + n); return d; }
    function memcpyImpl(d, s, n) { new Uint8Array(memory.buffer).copyWithin(d, s, s + n); return d; }
    const realEnv = {
        __indirect_function_table: table,
        __az_resolve_callback: (addr) => (addr === 0xFFFFFFFF) ? 0xFFFFFFFF : (cbTableIdx >= 0 ? cbTableIdx : 0xFFFFFFFF),
        memset: memsetImpl, memcpy: memcpyImpl, memmove: memcpyImpl,
    };
    const stubFor = n => /write_memory|barrier|exception_clear/.test(n)
        ? () => {} : (/_f64\b/.test(n) ? () => 0 : (/_64\b/.test(n) ? () => 0n : () => 0));
    const h = env => ({
        get: (_, p) => typeof p === 'string'
            ? (Object.prototype.hasOwnProperty.call(env, p) ? env[p] : stubFor(p)) : undefined,
        has: () => true,
    });

    const { instance: miniI } = await WebAssembly.instantiate(miniBytes, { env: new Proxy({}, h(realEnv)) });
    const mini = miniI.exports;
    memory = mini.memory;
    const cbEnv = { memory, __indirect_function_table: table, memset: memsetImpl, memcpy: memcpyImpl, memmove: memcpyImpl };

    if (cbBytes) {
        const { instance: cbI } = await WebAssembly.instantiate(cbBytes, { env: new Proxy({}, h(cbEnv)) });
        table.grow(1); cbTableIdx = table.length - 1; table.set(cbTableIdx, cbI.exports.callback);
    }
    const { instance: layoutI } = await WebAssembly.instantiate(layoutBytes, { env: new Proxy({}, h(cbEnv)) });
    table.grow(1);
    const layoutTableIdx = table.length - 1;
    table.set(layoutTableIdx, layoutI.exports.callback);

    const state = mini.AzStartup_init(0, 0);
    const modelPtr = mini.AzStartup_alloc(4);
    new DataView(memory.buffer).setUint32(modelPtr, initialCounter, true);
    const refanyPtr = mini.AzStartup_hydrate(Number(typeId & 0xFFFFFFFFn), Number((typeId >> 32n) & 0xFFFFFFFFn), modelPtr, 4);
    mini.AzStartup_setRefAny(state, refanyPtr);
    mini.AzStartup_setLayoutCbTableIdx(state, layoutTableIdx);
    mini.AzStartup_setModelPtr(state, modelPtr);
    mini.AzStartup_registerCbNode(state, 0);
    mini.AzStartup_setDisplayNode(state, 0);

    const layoutRc = mini.AzStartup_initLayoutCache(state, 800, 600, 0);
    if (layoutRc !== 0) fail('initLayoutCache rc=' + layoutRc);
    const domPtr = mini.AzStartup_getCurrentDomPtr(state);
    if (domPtr === 0) fail('initLayoutCache succeeded but current_dom_ptr=0');

    let hydrateRc = 'TRAP';
    let trapErr = null;
    try {
        hydrateRc = mini.AzStartup_hydrateStyledDom(state);
    } catch (e) {
        trapErr = e;
    }
    console.log('hydrateRc =', hydrateRc, trapErr ? ('(TRAP: ' + (trapErr.message || trapErr) + ')') : '');

    // ===== Diagnostic dumps =====
    const dv = new DataView(memory.buffer);
    const rd = a => dv.getUint32(a >>> 0, true);

    // Progress markers (written by hydrate IN ORDER) — localize a trap:
    //   0x40020 fixed_store(state)       [step 1, before cascade]
    //   0x40014 ptr_val+8                 [step 2, AFTER cascade returns+boxes]
    //   0x40018 test_ptr (make_test_struct)        [step 3]
    //   0x4001C sc_ptr   (make_test_struct_subcall)[step 4]
    //   0x40028 vs_ptr   (make_test_vec_struct)    [step 5]
    //   0x40104 0xDEADDEAD cascade-dump marker     [step 6, late]
    const marks = [
        ['0x40020 fixed_store', 0x40020],
        ['0x40014 cascade ptr+8', 0x40014],
        ['0x40018 test_ptr', 0x40018],
        ['0x4001C sc_ptr', 0x4001C],
        ['0x40028 vs_ptr', 0x40028],
        ['0x40104 late marker', 0x40104],
    ];
    console.log('--- hydrate progress markers (0=not reached) ---');
    for (const [label, addr] of marks) {
        console.log('  ' + label + ' = 0x' + (rd(addr) >>> 0).toString(16));
    }
    if (typeof mini.AzStartup_snapshotBumpHeap === 'function') {
        const bp = mini.AzStartup_snapshotBumpHeap() >>> 0;
        console.log('  bump_ptr   = 0x' + bp.toString(16) + ' (' + (bp / (1024 * 1024)).toFixed(1) +
            ' MiB; heap base 96 MiB, mem ' + (memory.buffer.byteLength / (1024 * 1024)).toFixed(0) + ' MiB)');
    }

    const test_ptr = rd(0x40018);
    let mtsNonzero = 0, mtsCorrect = 0;
    const mtsFirst = [];
    for (let i = 0; i < 64; i++) {
        const v = rd((test_ptr + i * 4) >>> 0);
        if (v !== 0) mtsNonzero++;
        if (v === ((0xA0000000 | i) >>> 0)) mtsCorrect++;
        if (i < 8) mtsFirst.push('0x' + v.toString(16).padStart(8, '0'));
    }
    console.log('--- make_test_struct (sret oracle) ---');
    console.log('  test_ptr   = 0x' + (test_ptr >>> 0).toString(16));
    console.log('  nonzero    = ' + mtsNonzero + '/64');
    console.log('  pattern-ok = ' + mtsCorrect + '/64  (expect 0xA0000000|i)');
    console.log('  first8     = ' + mtsFirst.join(' '));

    const sc_ptr = rd(0x4001C);
    let scNonzero = 0, scCorrect = 0;
    const scFirst = [];
    for (let i = 0; i < 64; i++) {
        const v = rd((sc_ptr + i * 4) >>> 0);
        if (v !== 0) scNonzero++;
        if (v === ((0xA0000000 | i) >>> 0)) scCorrect++;
        if (i < 8) scFirst.push('0x' + v.toString(16).padStart(8, '0'));
    }
    console.log('--- make_test_struct_subcall (sret across sub-calls) ---');
    console.log('  sc_ptr     = 0x' + (sc_ptr >>> 0).toString(16));
    console.log('  nonzero    = ' + scNonzero + '/64');
    console.log('  pattern-ok = ' + scCorrect + '/64  (expect 0xA0000000|i)');
    console.log('  first8     = ' + scFirst.join(' '));

    const vs_ptr = rd(0x40028);
    const vw = [];
    for (let i = 0; i < 10; i++) vw.push(rd((vs_ptr + i * 4) >>> 0));
    const vptr = vw[2];
    const velems = [];
    if (vptr) for (let i = 0; i < 3; i++) velems.push('0x' + rd((vptr + i * 4) >>> 0).toString(16).padStart(8, '0'));
    console.log('--- make_test_vec_struct (droppable Vec via sret) ---');
    console.log('  vs_ptr     = 0x' + (vs_ptr >>> 0).toString(16));
    console.log('  marker[0]  = 0x' + vw[0].toString(16) + '   (expect aaaaaaaa)');
    console.log('  v.ptr[2]   = 0x' + vw[2].toString(16) + '  hi[3]=0x' + vw[3].toString(16));
    console.log('  v.cap[4]   = ' + vw[4] + '  v.len[6]= ' + vw[6]);
    console.log('  tail[8]    = 0x' + vw[8].toString(16) + '   (expect cccccccc)');
    console.log('  raw10      = ' + vw.map(x => '0x' + x.toString(16)).join(' '));
    console.log('  v.elems    = ' + velems.join(' ') + '   (expect 0xbbbb0001..3)');

    const ptr_val = rd(0x4010C);
    let casNonzero = 0;
    const casDump = [];
    for (let i = 0; i < 20; i++) {
        const v = rd((0x40200 + i * 4) >>> 0);
        if (v !== 0) casNonzero++;
        casDump.push('0x' + v.toString(16).padStart(8, '0'));
    }
    console.log('--- cascade output (StyledDom first 80 bytes @0x40200) ---');
    console.log('  ptr_val    = 0x' + (ptr_val >>> 0).toString(16));
    console.log('  nonzero    = ' + casNonzero + '/20 u32');
    console.log('  dump       = ' + casDump.slice(0, 10).join(' '));
    console.log('             + ' + casDump.slice(10).join(' '));

    const styledPtr = mini.AzStartup_getStyledDomPtr(state);
    const styledCount = mini.AzStartup_getStyledDomNodeCount(state);
    const domCount = mini.AzStartup_getDomNodeCount(state);
    console.log('--- node counts ---');
    console.log('  getStyledDomPtr       = 0x' + (styledPtr >>> 0).toString(16));
    console.log('  getStyledDomNodeCount = ' + styledCount + '   <-- THE blocker: want >= 1');
    console.log('  getDomNodeCount(walk) = ' + domCount);

    process.exit(0);
})().catch(e => { console.error(e.stack); process.exit(1); });
