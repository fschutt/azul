// M12.7 layout-correctness gate.
//
// Drives the REAL layout solver (`AzStartup_solveLayoutReal`) instead
// of the block-flow stub (`AzStartup_solveLayout`) and verifies it:
//   1. lifts + runs without trapping (rc == 0),
//   2. produces one positioned rect per StyledDom node,
//   3. produces geometry from the actual taffy solver, NOT the stub's
//      fixed `(0, node_idx*30, viewport_w, 30)` rows.
//
// Bootstrap mirrors hit-test.js exactly through hydrateStyledDom; the
// only change is solveLayoutReal in place of solveLayout.
//
// Backed by hello-world-v5.bin (1 node — bare body — on :8800).
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
    const styledNodes = mini.AzStartup_getStyledDomNodeCount(state);
    console.log('[1] cascade ok: styled_dom node_count=' + styledNodes);

    // === The real layout solver ===
    if (typeof mini.AzStartup_solveLayoutReal !== 'function')
        fail('AzStartup_solveLayoutReal not exported (build/lift gap)');
    let rc;
    try {
        rc = mini.AzStartup_solveLayoutReal(state, viewportW, viewportH);
    } catch (e) {
        console.error('solveLayoutReal TRAPPED: ' + e.message);
        console.error('STACK:\n' + e.stack);
        // BumpAlloc/Realloc helper debug: 0x40030=last size (i64), 0x40038=call count.
        if (typeof mini.AzStartup_peekU32 === 'function') {
            const sizeLo = mini.AzStartup_peekU32(0x40030);
            const sizeHi = mini.AzStartup_peekU32(0x40034);
            const cnt = mini.AzStartup_peekU32(0x40038);
            console.error('POST-TRAP: last_alloc_size=0x' + sizeHi.toString(16) + sizeLo.toString(16).padStart(8,'0') +
                          ' (' + (sizeHi * 4294967296 + sizeLo) + ')  alloc_call_count=' + cnt);
        }
        if (typeof mini.AzStartup_snapshotBumpHeap === 'function') {
            console.error('POST-TRAP: bump_ptr=0x' + mini.AzStartup_snapshotBumpHeap().toString(16));
        }
        // 0x40040 = last BumpAlloc returned ptr; 0x40048 = NeverLift sym addr reached (0=none).
        const retLo = mini.AzStartup_peekU32(0x40040), retHi = mini.AzStartup_peekU32(0x40044);
        const nlLo = mini.AzStartup_peekU32(0x40048), nlHi = mini.AzStartup_peekU32(0x4004C);
        console.error('POST-TRAP: last_alloc_ret=0x' + retHi.toString(16) + retLo.toString(16).padStart(8,'0') +
                      '  NeverLift_reached=0x' + nlHi.toString(16) + nlLo.toString(16).padStart(8,'0') +
                      ' (0x37993a0=handle_alloc_error)');
        // 0x40050 = AZ_TAG_UNREACHABLE marker: 0x554e0000|id of the live unreachable.
        const u = mini.AzStartup_peekU32(0x40050);
        if ((u >>> 16) === 0x554e) console.error('POST-TRAP: live_unreachable id=' + (u & 0xffff) + ' (map to Nth `unreachable` in <stem>.untag.ll)');
        else console.error('POST-TRAP: unreachable_marker=0x' + u.toString(16) + ' (not set / not tagged)');
        // 0x40060 = AZ_FUEL tripped flag (1 = an instrumented loop exceeded AZ_FUEL_LIMIT → see STACK for the looping fn).
        if (mini.AzStartup_peekU32(0x40060) === 1) console.error('POST-TRAP: FUEL TRIPPED — infinite loop; the STACK above names the looping fn; looping_block_id=' + mini.AzStartup_peekU32(0x40070) + ' (map to Nth `call @__az_fuel(i32 N)` in <stem>.fuel.ll)');
        // 0x40078 = AZ_LOG_SELFLOOP_VAL: the i64 `v` (icmp eq v,0 operand) that routed into the live opt-folded self-loop (should be 0).
        const slvLo = mini.AzStartup_peekU32(0x40078), slvHi = mini.AzStartup_peekU32(0x4007C);
        if (slvLo !== 0 || slvHi !== 0) console.error('POST-TRAP: selfloop_routing_v=0x' + slvHi.toString(16) + slvLo.toString(16).padStart(8,'0') + ' (the non-zero value opt folded the loop-exit on; should be 0)');
        process.exit(1);
    }
    if (rc !== 0) fail('solveLayoutReal rc=' + rc + ' (see status codes in eventloop.rs)');
    console.log('[2] solveLayoutReal rc=0 (real taffy positioning ran in wasm)');

    const rectsLen = mini.AzStartup_getPositionedRectsLen(state);
    const rectsPtr = mini.AzStartup_getPositionedRectsPtr(state);
    if (rectsPtr === 0 || rectsLen < 1) fail('no positioned rects (ptr=' + rectsPtr + ' len=' + rectsLen + ')');
    if (styledNodes > 0 && rectsLen !== styledNodes)
        fail('rects_len ' + rectsLen + ' != styled node_count ' + styledNodes);

    const dv = new DataView(memory.buffer);
    let stubShaped = true; // stub: every rect is (0, i*30, viewportW, 30)
    let anyNonZero = false; // a real solve produces ≥1 sized rect; all-zero = it didn't run
    for (let i = 0; i < rectsLen; i++) {
        const off = rectsPtr + i * 16;
        const x = dv.getUint32(off, true);
        const y = dv.getUint32(off + 4, true);
        const w = dv.getUint32(off + 8, true);
        const hh = dv.getUint32(off + 12, true);
        console.log('  rect[' + i + ']: x=' + x + ' y=' + y + ' w=' + w + ' h=' + hh);
        if (!(x === 0 && y === i * 30 && w === viewportW && hh === 30)) stubShaped = false;
        if (x !== 0 || y !== 0 || w !== 0 || hh !== 0) anyNonZero = true;
    }
    if (!anyNonZero) fail('all rects are (0,0,0,0) — layout_document did not run (e.g. an early-return before it); not real geometry');
    if (stubShaped) fail('rects look exactly like the block-flow STUB — real solver not wired');
    console.log('[3] geometry is from the real solver (not the stub rows) ✓');

    console.log('\nPASS: M12.7 real layout solver lifts + positions in wasm');
    process.exit(0);
})().catch(e => { console.error(e.stack); process.exit(1); });
