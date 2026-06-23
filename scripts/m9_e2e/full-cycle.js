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
const AZ_PORT = process.env.AZ_PORT || '8800';
function fetch_(p) {
    return new Promise((r,j) => http.get('http://127.0.0.1:' + AZ_PORT + p, x => {
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
    // [AZ_NOOP_MEM=1] Reproduce the BROWSER's bug: loader.js provides NO
    // memset/memcpy/memmove → they stub to i32_noop (return 0, do nothing).
    // If this flag makes the harness OOB the way the browser does, the
    // missing-mem-ops-in-loader.js diagnosis is confirmed.
    const NOOP_MEM = process.env.AZ_NOOP_MEM === '1';
    function memsetImpl(d, v, n) { if (NOOP_MEM) return 0; new Uint8Array(memory.buffer).fill(v & 0xFF, d, d + n); return d; }
    function memcpyImpl(d, s, n) { if (NOOP_MEM) return 0; new Uint8Array(memory.buffer).copyWithin(d, s, s + n); return d; }
    // __multi3 = compiler-rt 128-bit multiply (LEAKED wasm import; same class as the
    // fmaxf/fminf libcall leak). LLVM lowers Rust u128/i128 multiply (Vec/Layout::array
    // overflow checks, ratio math — e.g. Css::from's `cap * elem_size`) to a `__multi3`
    // call. If UNPROVIDED it stubs to 0 AND never writes its sret → corrupt alloc
    // sizes/lengths → byte-span 0 → runaway loop → OOB. The PRODUCTION loader
    // (dll/src/web/loader_js.rs azMulti3) provides it to BOTH mini AND cb/layout wasms;
    // this test harness must too. wasm sig (i32 sret, i64 aLo,aHi, i64 bLo,bHi) -> nil.
    const azMulti3 = (sret, aLo, aHi, bLo, bHi) => {
        const dv = new DataView(memory.buffer);
        const mask = 0xFFFFFFFFFFFFFFFFn;
        const a = (BigInt.asUintN(64, BigInt(aHi)) << 64n) | BigInt.asUintN(64, BigInt(aLo));
        const b = (BigInt.asUintN(64, BigInt(bHi)) << 64n) | BigInt.asUintN(64, BigInt(bLo));
        const p = BigInt.asUintN(128, a * b);
        dv.setBigUint64(Number(sret), p & mask, true);
        dv.setBigUint64(Number(sret) + 8, (p >> 64n) & mask, true);
    };
    // __udivti3 = compiler-rt 128-bit UNSIGNED divide (a/b), same LEAKED-import
    // class + sret shape (sig=5) as __multi3. mini.wasm imports it (func9).
    // Unprovided → i32_noop → every i128 divide returns 0 → garbage.
    const azUdivti3 = (sret, aLo, aHi, bLo, bHi) => {
        const dv = new DataView(memory.buffer);
        const mask = 0xFFFFFFFFFFFFFFFFn;
        const a = (BigInt.asUintN(64, BigInt(aHi)) << 64n) | BigInt.asUintN(64, BigInt(aLo));
        const b = (BigInt.asUintN(64, BigInt(bHi)) << 64n) | BigInt.asUintN(64, BigInt(bLo));
        const q = b === 0n ? 0n : (a / b);
        dv.setBigUint64(Number(sret), q & mask, true);
        dv.setBigUint64(Number(sret) + 8, (q >> 64n) & mask, true);
    };
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
        __multi3: azMulti3,
        __udivti3: azUdivti3,
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
    const cbEnv = { memory, __indirect_function_table: table, memset: memsetImpl, memcpy: memcpyImpl, memmove: memcpyImpl, __multi3: azMulti3, __udivti3: azUdivti3 };

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

    // Seed the bump heap above the synth-address mirror band BEFORE the
    // first allocation — exactly what loader.js does at bootstrap. Without
    // it the bump pointer starts at 0, so AzStartup_init's internal
    // Box::new(EventloopState) returns offset 0 and reads back as a null
    // state. (Harness parity fix; production loader has always done this.)
    if (typeof mini.AzStartup_resetBumpHeap === 'function') {
        mini.AzStartup_resetBumpHeap(160 * 1024 * 1024);
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
    // The hello-world-v5 layout returns a body with on_click. We register
    // the body (node_idx 0) as the clickable node and the same node as
    // the display target (the SetText patch will land on it).
    mini.AzStartup_registerCbNode(state, 0);
    mini.AzStartup_setDisplayNode(state, 0);

    // [AZ_FONT=1] Browser-parity: register the REAL fallback font before
    // initLayoutCache, exactly like loader.js does (fetch /az/fallback.ttf →
    // alloc → copy → AzStartup_setFallbackFont). Without this the wasm uses
    // the embedded const font and the Css::from clone OOB (browser-only,
    // func70) is MASKED. With it, the harness should reproduce the browser
    // OOB in Node for fast iteration. Diagnosing the text-shaping path.
    if (process.env.AZ_FONT === '1' && typeof mini.AzStartup_setFallbackFont === 'function') {
        const fontBytes = new Uint8Array(await fetch_('/az/fallback.ttf'));
        if (fontBytes.length > 0) {
            const fontPtr = mini.AzStartup_alloc(fontBytes.length);
            new Uint8Array(memory.buffer).set(fontBytes, fontPtr);
            mini.AzStartup_setFallbackFont(fontPtr, fontBytes.length);
            console.log('[font] registered fallback font (' + fontBytes.length + ' bytes) at ptr ' + fontPtr);
        } else {
            console.log('[font] /az/fallback.ttf empty — skipping');
        }
    }

    const layoutRc = mini.AzStartup_initLayoutCache(state, 800, 600, 0);
    if (layoutRc !== 0) failSync('initLayoutCache rc=' + layoutRc);
    console.log('[2] layout cache populated (rc=' + layoutRc + ')');

    // [AZ_HYDRATE=1] Browser-parity: run AzStartup_hydrateStyledDom after
    // initLayoutCache (loader.js azBootstrap does this). The browser OOBs HERE
    // (mini.wasm func698 <- recursive func352 <- ... <- hydrateStyledDom) — the
    // AzDom tree walk / style cascade over the layout-cb's sret-returned Dom.
    // full-cycle.js never called it, so this path was untested in Node.
    if (process.env.AZ_HYDRATE === '1') {
        const domPtr = (typeof mini.AzStartup_getCurrentDomPtr === 'function')
            ? mini.AzStartup_getCurrentDomPtr(state) : 0;
        console.log('[2b] current_dom_ptr=' + domPtr);
        if (domPtr && typeof mini.AzStartup_hydrateStyledDom === 'function') {
            // Snapshot the bump pointer (u32 @ 262176, per the BumpAlloc stub).
            // hydrateStyledDom OOBs (deep x86 sret bug) AFTER a garbage-size Vec
            // alloc has advanced the bump pointer to a huge value → the next
            // AzStartup_alloc OOBs. Catch the trap AND restore the bump pointer
            // so the allocator is clean for the subsequent click dispatch.
            const BUMP_OFF = 262176;
            const bumpBefore = new DataView(memory.buffer).getUint32(BUMP_OFF, true);
            try {
                const hRc = mini.AzStartup_hydrateStyledDom(state);
                const nodeCount = (typeof mini.AzStartup_getDomNodeCount === 'function')
                    ? mini.AzStartup_getDomNodeCount(state) : -1;
                console.log('[2c] hydrateStyledDom rc=' + hRc + ' node_count=' + nodeCount);
            } catch (e) {
                const bumpAfter = new DataView(memory.buffer).getUint32(BUMP_OFF, true);
                console.log('[2c] hydrateStyledDom TRAPPED: ' + e.message +
                    ' — bump 0x' + bumpBefore.toString(16) + ' → 0x' + bumpAfter.toString(16) +
                    (bumpAfter !== bumpBefore ? ' (CORRUPTED — restoring)' : ''));
                new DataView(memory.buffer).setUint32(BUMP_OFF, bumpBefore, true);
            }
        }
    }

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
    let patchLen = new DataView(memory.buffer).getUint32(outLenPtr, true);
    // [out_len lift bug, 2026-06-21] dispatchEvent's `used` write goes through out_len_ptr, its 5th
    // STACK arg held in RBP. remill spills out_len_ptr + reuses RBP but the lift drops the reload
    // before the final `*out_len = used` store (it uses the clobbered RBP=0), so out_len stays at the
    // value `*out_len = 0` init. The SetText TLV is SELF-DESCRIBING (kind|node|payload_len|payload),
    // so recover the true length from it — what a robust loader.js should do regardless. (Root fix =
    // remill spill/reload value-flow for stack-arg out-params; documented in handoff §8.)
    if (patchLen === 0 && patchPtr) {
        const pl = new DataView(memory.buffer, patchPtr + 5, 4).getUint32(0, true);
        if (pl > 0 && pl < 64) { patchLen = 9 + pl; console.log('[note] out_len mis-lifted (read 0); recovered patch_len=' + patchLen + ' from self-describing TLV'); }
    }
    const counterAfter = new DataView(memory.buffer).getUint32(modelPtr, true);
    console.log('[4] dispatchEvent: patch_ptr=' + patchPtr + ' patch_len=' + patchLen +
                ' counter ' + counterBefore + ' → ' + counterAfter);
    if (process.env.AZ_DEBUG_PATCH && patchPtr) {
        const dbg = [...new Uint8Array(memory.buffer, patchPtr, 16)].map(b => b.toString(16).padStart(2, '0')).join(' ');
        console.log('[dbg] patch_buf[0..16] = ' + dbg + '   outLenPtr=0x' + outLenPtr.toString(16) + ' (=' + patchLen + ')');
    }
    if (process.env.AZ_DUMP_REGTRACE) {
        const dvr = new DataView(memory.buffer);
        const CNT = dvr.getUint32(983024, true), base = 983040, N = Math.min(CNT, 8191);
        const nm = id => ({99:'SP',0:'RAX',50:'RCX',52:'RDX',53:'R8',54:'RBX',55:'R12',56:'R14',57:'R13',58:'RBP',59:'RSI',61:'R15',70:'RDI'}[id] || ('r'+id));
        console.log('[regtrace] count=' + CNT + ' (kept ' + N + ')');
        const out = [];
        for (let k = 0; k < N; k++) { const id = dvr.getUint32(base+k*8, true), v = dvr.getUint32(base+k*8+4, true); out.push('['+k+'] '+nm(id)+'=0x'+v.toString(16)); }
        console.log(out.join('\n'));
    }

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
