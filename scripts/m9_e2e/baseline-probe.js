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

// Parse a wasm module's custom "name" section -> {funcIndex: name}.
function parseWasmFnNames(buf) {
    const names = {};
    let p = 8; // skip magic + version
    function uleb() { let r = 0, s = 0, b; do { b = buf[p++]; r |= (b & 0x7f) << s; s += 7; } while (b & 0x80); return r >>> 0; }
    try {
        while (p < buf.length) {
            const id = buf[p++];
            const size = uleb();
            const end = p + size;
            if (id === 0) { // custom section
                const nlen = uleb();
                const sname = buf.slice(p, p + nlen).toString();
                p += nlen;
                if (sname === 'name') {
                    while (p < end) {
                        const sub = buf[p++];
                        const subsize = uleb();
                        const subend = p + subsize;
                        if (sub === 1) { // function names
                            const cnt = uleb();
                            for (let i = 0; i < cnt; i++) {
                                const idx = uleb();
                                const l = uleb();
                                names[idx] = buf.slice(p, p + l).toString();
                                p += l;
                            }
                        }
                        p = subend;
                    }
                }
            } else if (id === 7) { // export section (name section is often stripped)
                const cnt = uleb();
                for (let i = 0; i < cnt; i++) {
                    const l = uleb();
                    const nm = buf.slice(p, p + l).toString();
                    p += l;
                    const kind = buf[p++];
                    const idx = uleb();
                    if (kind === 0 && !names[idx]) names[idx] = 'export:' + nm; // func export
                }
            }
            p = end;
        }
    } catch (e) { /* best-effort */ }
    return names;
}

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
    if (trapErr && trapErr.stack) {
        const fnNames = parseWasmFnNames(miniBytes);
        const frames = [...trapErr.stack.matchAll(/wasm-function\[(\d+)\]/g)].map(m => parseInt(m[1]));
        console.log('--- trap call stack (innermost first) ---');
        for (const idx of frames) {
            const nm = fnNames[idx] || '(no name)';
            console.log('  func[' + idx + '] = ' + nm);
        }
    }

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
    // M12.5f alloc instrumentation (BumpAlloc/Realloc write these):
    //   0x40030 = last requested size (i64 lo), 0x40038 = total alloc count
    const lastSize = rd(0x40030) + rd(0x40034) * 4294967296;
    const allocCount = rd(0x40038) + rd(0x4003C) * 4294967296;
    console.log('  last_alloc_size = ' + lastSize + ' (' + (lastSize / (1024 * 1024)).toFixed(2) + ' MiB)');
    console.log('  alloc_count     = ' + allocCount + '  -> huge last_size = single bad alloc; huge count = runaway loop');
    if (typeof mini.AzStartup_getDbgNc === 'function') {
        const dbg = k => (mini.AzStartup_getDbgNc(k * 2) >>> 0) + (mini.AzStartup_getDbgNc(k * 2 + 1) >>> 0) * 4294967296;
        console.log('--- build_compact_cache captured args (AZ_DBG_NC static) ---');
        console.log('  self ptr        = 0x' + dbg(0).toString(16) + '   (valid cache addr ~heap, or garbage?)');
        console.log('  self.node_count = ' + dbg(1) + '   (want 1; if huge → cache built with bad count)');
        console.log('  node_data.ptr   = 0x' + dbg(2).toString(16));
        console.log('  node_data.len   = ' + dbg(3) + '   (want 1)');
        console.log('--- create_from_compact_dom TOP: compact_dom.node_data as it ARRIVES (4/5/6 overwritten here) ---');
        console.log('  node_data.len[4]   = ' + dbg(4) + '   (want 1; if heap-ptr → scrambled IN TRANSIT: CompactDom sret/by-value move)');
        console.log('  node_data.ptr[5]   = 0x' + dbg(5).toString(16) + '   (want heap, NOT 0)');
        console.log('  compact_dom.len[6] = ' + dbg(6) + '   (want 1)');
        console.log('  (convert-build capture was: len=1, ptr=heap — CORRECT; so if scrambled here, transit is the bug)');
        console.log('--- css_property_cache.node_count BRACKET (want 1 throughout; first non-1 = the scrambling op) ---');
        console.log('  [7] after empty()               = ' + dbg(7));
        console.log('  [8] after restyle()             = ' + dbg(8));
        console.log('  [9] after apply_ua_css()        = ' + dbg(9));
        console.log('  [10] after compute_inherited()  = ' + dbg(10));
        console.log('  [11] before build_compact_cache = ' + dbg(11));
        console.log('  M12.5y &node_count ADDR: [7t]=0x' + (dbg(41) >>> 0).toString(16) +
            ' [9t]=0x' + (dbg(43) >>> 0).toString(16) + ' [11t]=0x' + (dbg(45) >>> 0).toString(16) +
            '  &cache[dumpt]=0x' + (dbg(44) >>> 0).toString(16) +
            '  (DIFFER => lift computes the local cache addr inconsistently)');
        console.log('  M12.5y DROP-ISOLATION: after drop(css) node_count=' + dbg(46) +
            ' &node_count=0x' + (dbg(47) >>> 0).toString(16) +
            '  (=1 & 0x2ef98 => drop innocent, apply_ua_css corrupts; else drop corrupts)');
        const lo = k => mini.AzStartup_getDbgNc(k * 2) >>> 0;
        const cap = k => (dbg(k) >= 0x100000000 ? String(lo(k)) : '(not captured)');
        const self15 = dbg(15), sp13 = dbg(13);
        const delta = self15 - sp13;
        console.log('--- M12.5u frame-overlap test (push count=' + dbg(6) + ') ---');
        console.log('  [15] self (cache) addr          = 0x' + self15.toString(16));
        console.log('  [13] ~apply_ua_css SP           = 0x' + sp13.toString(16));
        console.log('  self - SP                       = ' + delta + ' (0x' + (delta>>>0).toString(16) + ')'
            + (delta > 0 && delta < 512 ? '  <<< OVERLAP! self inside apply_ua_css frame' : (delta >= 512 ? '  (self above frame; no overlap)' : '  (self below SP)')));
        console.log('  [12] node_count after push #1   = ' + cap(12) + '   (0 = corrupted)');
        console.log('  [13] node_count after push #2   = ' + cap(13));
        console.log('  [14] node_count after push #3+  = ' + cap(14) + '   (last push; 0 = a realloc push corrupted)');
        console.log('--- M12.5v cache-byte dump (self+k*8, from create_from; non-perturbing) ---');
        let allzeroish = true;
        for (let j = 0; j < 20; j++) {
            const v = dbg(16 + j);
            if (v !== 0) allzeroish = false;
            let tag = '';
            if (j === 0) tag = '  <- node_count (want 1)';
            else if (j === 4) tag = '  <- cascaded_props FlatVecVec.build (Vec cap)';
            else if (j === 17) tag = '  <- self+0x88 = StatefulCssProperty.prop_type if slot-copy hit self';
            console.log('  cache[+0x' + (j * 8).toString(16).padStart(2, '0') + '] = 0x' + v.toString(16) + tag);
        }
    }

    // ===== M12.5y store-address tracer ring buffer @0x41000 =====
    // (populated only when the server lifted with AZ_LOG_STORES=<dep-stem>)
    {
        const logCount = rd(0x41000);
        if (logCount > 0) {
            const g = (typeof mini.AzStartup_getDbgNc === 'function') ? (k => mini.AzStartup_getDbgNc(k) >>> 0) : (() => 0);
            const selfAddr = g(30);          // dbg(15) lo = cache base (apply_ua_css view)
            const ncAddr = g(82);            // dbg(41) lo = &node_count
            const cacheBase = g(84);         // dbg(42) lo = &cache
            const ncOff = (ncAddr && cacheBase) ? (ncAddr - cacheBase) : -1;
            console.log('--- M12.5y store tracer (count=' + logCount +
                ', cache=0x' + cacheBase.toString(16) + ', &node_count=0x' + ncAddr.toString(16) +
                ' (offset ' + ncOff + '=0x' + (ncOff >>> 0).toString(16) + ')) ---');
            const shown = Math.min(logCount, 3500);
            const ncWrites = [];   // exact &node_count writes (with value)
            const spanWrites = []; // writes whose [addr, addr+val) spans node_count (memset/memcpy len)
            for (let i = 0; i < shown; i++) {
                const a = rd((0x41010 + i * 16) >>> 0);
                const id = rd((0x41010 + i * 16 + 4) >>> 0);
                const dt = rd((0x41010 + i * 16 + 8) >>> 0);
                const val = rd((0x41010 + i * 16 + 12) >>> 0);
                if (ncAddr && a === ncAddr) ncWrites.push({ i, id, dt, val });
                // spanning: addr <= ncAddr < addr+val, and val is a plausible length (< 0x10000, > 8)
                if (ncAddr && a <= ncAddr && (a + val) > ncAddr && val > 8 && val < 0x10000) spanWrites.push({ i, id, dt, val, a });
            }
            console.log('  >>> EXACT WRITES TO &node_count (0x' + ncAddr.toString(16) + ') [value shown]:');
            if (ncWrites.length === 0) console.log('     (none)');
            for (const w of ncWrites) console.log('     [' + w.i + '] dep=0x' + w.dt.toString(16) + ' id=' + w.id + ' val=0x' + w.val.toString(16));
            console.log('  >>> SPANNING WRITES (memset/memcpy [addr,addr+len) covers node_count) — likely the bulk-zero corruptor:');
            if (spanWrites.length === 0) console.log('     (none)');
            for (const w of spanWrites) console.log('     [' + w.i + '] dep=0x' + w.dt.toString(16) + ' id=' + w.id +
                ' addr=0x' + w.a.toString(16) + ' len=0x' + w.val.toString(16) + ' (-> 0x' + (w.a + w.val).toString(16) + ')');
        }
        // Also: dump every write whose addr is inside the cache struct, with value,
        // for manual inspection of the zeroing sequence.
        if (logCount > 0) {
            const cacheBase2 = (typeof mini.AzStartup_getDbgNc === 'function') ? (mini.AzStartup_getDbgNc(84) >>> 0) : 0;
            const shown = Math.min(logCount, 3500);
            const inCache = [];
            const spTraj = [];   // addr==0xF0000 marker = an SP-slot store; val = the SP value
            for (let i = 0; i < shown; i++) {
                const a = rd((0x41010 + i * 16) >>> 0);
                const id = rd((0x41010 + i * 16 + 4) >>> 0), dt = rd((0x41010 + i * 16 + 8) >>> 0), val = rd((0x41010 + i * 16 + 12) >>> 0);
                if (a === 0xF0000) spTraj.push({ i, id, dt, val });
                else if (cacheBase2 && a >= cacheBase2 && a < cacheBase2 + 0x198) inCache.push({ i, off: a - cacheBase2, id, dt, val });
            }
            console.log('  >>> SP TRAJECTORY (every SP-slot store, execution order; watch for SP not restored across a call):');
            for (const w of spTraj) console.log('     [' + w.i + '] dep=0x' + w.dt.toString(16) + ' id=' + w.id + ' SP=0x' + w.val.toString(16));
            // apply_ua_css frame dump: apply SP = cacheBase-8416, frame at cacheBase-0x2290..-0x20E0.
            // The alloc-result check %60 is spilled at SP+16; if its value is 0, apply takes the
            // alloc-error path. Dump writes in [cacheBase-0x2400, cacheBase-0x2080] with values.
            const afLo = (cacheBase2 - 0x2400) >>> 0, afHi = (cacheBase2 - 0x2080) >>> 0;
            console.log('  >>> apply_ua_css FRAME writes [0x' + afLo.toString(16) + '..0x' + afHi.toString(16) + '] (val=0 at an alloc-result spill => error path):');
            for (let i = 0; i < shown; i++) {
                const a = rd((0x41010 + i * 16) >>> 0);
                if (a >= afLo && a < afHi) {
                    const id = rd((0x41010 + i * 16 + 4) >>> 0), dt = rd((0x41010 + i * 16 + 8) >>> 0), val = rd((0x41010 + i * 16 + 12) >>> 0);
                    console.log('     [' + i + '] 0x' + a.toString(16) + ' dep=0x' + dt.toString(16) + ' id=' + id + ' val=0x' + val.toString(16));
                }
            }
            console.log('  >>> ALL writes inside the cache struct [cacheBase+off], execution order:');
            for (const w of inCache) console.log('     [' + w.i + '] +0x' + w.off.toString(16) + ' dep=0x' + w.dt.toString(16) + ' id=' + w.id + ' val=0x' + w.val.toString(16));
            // apply_ua_css callee-saved SAVE AREA: apply SP = cacheBase-8416, frame SP -=0x1b0,
            // x25 saved at frame SP+0x168 = cacheBase-0x2128. If this slot is written AFTER the
            // prologue with a garbage value, the epilogue restores garbage → caller's X25 (cache base) corrupted.
            const x25slot = (cacheBase2 - 0x2128) >>> 0;
            const saveLo = (cacheBase2 - 0x2200) >>> 0, saveHi = (cacheBase2 - 0x2000) >>> 0;
            const saveWrites = [];
            for (let i = 0; i < shown; i++) {
                const a = rd((0x41010 + i * 16) >>> 0);
                if (a >= saveLo && a < saveHi) saveWrites.push({ i, a, id: rd((0x41010+i*16+4)>>>0), dt: rd((0x41010+i*16+8)>>>0), val: rd((0x41010+i*16+12)>>>0) });
            }
            console.log('  >>> apply_ua_css SAVE-AREA writes [' + saveLo.toString(16) + '..' + saveHi.toString(16) + '], x25slot=0x' + x25slot.toString(16) + ', execution order:');
            for (const w of saveWrites) console.log('     [' + w.i + '] 0x' + w.a.toString(16) + (w.a===x25slot?' (X25 SLOT)':'') + ' dep=0x' + w.dt.toString(16) + ' id=' + w.id + ' val=0x' + w.val.toString(16));
        } else {
            console.log('--- M12.5y store tracer: count=0 ---');
        }
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

    const mv_ptr = rd(0x4002C);
    // TestMultiVec repr(C): m@0, a:Vec@8{cap@8,ptr@16,len@24},
    // b:Vec@32{len@48}, c:Vec@56{len@72}, t@80.
    const mvWords = [];
    for (let i = 0; i < 24; i++) mvWords.push(rd((mv_ptr + i * 4) >>> 0));
    console.log('--- make_test_multivec (multi-Vec struct via sret) ---');
    console.log('  mv_ptr  = 0x' + (mv_ptr >>> 0).toString(16));
    console.log('  m[0]    = 0x' + mvWords[0].toString(16) + ' (expect aaaaaaaa)');
    console.log('  a.len[6]  = ' + mvWords[6] + ' (expect 2)  a.ptr[4]=0x' + mvWords[4].toString(16));
    console.log('  b.len[12] = ' + mvWords[12] + ' (expect 3)  b.ptr[10]=0x' + mvWords[10].toString(16));
    console.log('  c.len[18] = ' + mvWords[18] + ' (expect 1)  c.ptr[16]=0x' + mvWords[16].toString(16));
    console.log('  t[20]   = 0x' + mvWords[20].toString(16) + ' (expect cccccccc)');
    console.log('  raw24   = ' + mvWords.map(x => x.toString(16)).join(' '));

    const av_ptr = rd(0x40040);
    // TestAzVecStruct repr(C): m@0, v:U32Vec@8 {ptr@8,len@16,cap@24,destr@32}, t@40.
    const av = [];
    for (let i = 0; i < 12; i++) av.push(rd((av_ptr + i * 4) >>> 0));
    console.log('--- make_test_azvec (impl_vec! AzVec via from_vec/sret) ---');
    console.log('  av_ptr  = 0x' + (av_ptr >>> 0).toString(16));
    console.log('  m[0]      = 0x' + av[0].toString(16) + ' (expect aaaaaaaa)');
    console.log('  v.ptr[2]  = 0x' + av[2].toString(16) + ' hi[3]=0x' + av[3].toString(16) + '  (expect heap ptr, NOT 0)');
    console.log('  v.len[4]  = ' + av[4] + ' (expect 3; if huge/ptr → SCRAMBLED = AzVec mis-lift CONFIRMED)');
    console.log('  v.cap[6]  = ' + av[6] + ' (expect >=3)');
    console.log('  t[10]     = 0x' + av[10].toString(16) + ' (expect cccccccc)');
    console.log('  raw12   = ' + av.map(x => x.toString(16)).join(' '));
    if (av[2] !== 0 && av[4] === 3) console.log('  >>> AzVec OK — suspect WRONG, look elsewhere');
    else if (av[2] === 0 || av[4] > 1000000) console.log('  >>> AzVec SCRAMBLED — root cause #2 CONFIRMED');

    const bv_ptr = rd(0x40044);
    // TestBigVecStruct repr(C): marker@0, v:Vec<BigElem>@8 {cap@8,ptr@16,len@24}, tail@32.
    const bw = [];
    for (let i = 0; i < 12; i++) bw.push(rd((bv_ptr + i * 4) >>> 0));
    console.log('--- make_test_bigvec (Vec<large droppable elem>) ---');
    console.log('  bv_ptr  = 0x' + (bv_ptr >>> 0).toString(16));
    console.log('  marker[0] = 0x' + bw[0].toString(16) + ' (expect aaaaaaaa)');
    console.log('  v.cap[2]  = ' + bw[2] + '  v.ptr[4]= 0x' + bw[4].toString(16) + '  v.len[6]= ' + bw[6] + ' (expect cap>=2, ptr=heap, len=2)');
    console.log('  tail[8]   = 0x' + bw[8].toString(16) + ' (expect cccccccc)');
    console.log('  raw12   = ' + bw.map(x => x.toString(16)).join(' '));
    if (bw[4] === 0 || bw[6] > 1000000) console.log('  >>> BigVec SCRAMBLED — complex-element Vec path CONFIRMED as root cause #2');
    else if (bw[4] !== 0 && bw[6] === 2) console.log('  >>> BigVec OK — complex-element Vec also works; bug is even more specific');

    const rv_ptr = rd(0x40048);
    // TestRecVec repr(C): m@0, v:Vec<u32>@8 {cap@8,ptr@16,len@24}, t@32.
    const rw = [];
    for (let i = 0; i < 10; i++) rw.push(rd((rv_ptr + i * 4) >>> 0));
    console.log('--- make_test_recvec (RECURSIVE Vec accumulation) ---');
    console.log('  rv_ptr  = 0x' + (rv_ptr >>> 0).toString(16));
    console.log('  m[0]      = 0x' + rw[0].toString(16) + ' (expect aaaaaaaa)');
    console.log('  v.cap[2]  = ' + rw[2] + '  v.ptr[4]= 0x' + rw[4].toString(16) + '  v.len[6]= ' + rw[6] + ' (expect cap>=5, ptr=heap, len=5)');
    console.log('  t[8]      = 0x' + rw[8].toString(16) + ' (expect cccccccc)');
    console.log('  raw10   = ' + rw.map(x => x.toString(16)).join(' '));
    if (rw[4] === 0 || rw[6] > 1000000) console.log('  >>> RECVEC SCRAMBLED — RECURSION+Vec-accum is root cause #2 CONFIRMED!');
    else if (rw[4] !== 0 && rw[6] === 5) console.log('  >>> RECVEC OK — recursion+Vec-accum also works; even more specific');

    const u8_ptr = rd(0x4004C);
    // TestU128 repr(C): m@0, a:[u128;2]@16 (a[0]@16, a[1]@32), t@48.
    const uw = [];
    for (let i = 0; i < 14; i++) uw.push(rd((u8_ptr + i * 4) >>> 0));
    console.log('--- make_test_u128 (SMOKING-GUN: apply_ua_css 1u128<<d pattern) ---');
    console.log('  u8_ptr  = 0x' + (u8_ptr >>> 0).toString(16));
    console.log('  m[0]    = 0x' + uw[0].toString(16) + ' (expect aaaaaaaa)');
    console.log('  a[0]@16 = 0x' + uw[4].toString(16) + ' ' + uw[5].toString(16) + ' ' + uw[6].toString(16) + ' ' + uw[7].toString(16) + ' (expect 20 0 0 0 = 1<<5)');
    console.log('  a[1]@32 = 0x' + uw[8].toString(16) + ' ' + uw[9].toString(16) + ' ' + uw[10].toString(16) + ' ' + uw[11].toString(16) + ' (expect 400 0 0 0 = 1<<10)');
    console.log('  t[12]   = 0x' + uw[12].toString(16) + ' (expect cccccccc)');
    if (uw[4] === 0x20 && uw[8] === 0x400 && uw[12] === 0xcccccccc) console.log('  >>> U128 OK — inline u128 lift is FINE, look elsewhere in apply_ua_css');
    else console.log('  >>> U128 SCRAMBLED — inline-u128 NEON-Q lift is root cause #2 CONFIRMED!');

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
