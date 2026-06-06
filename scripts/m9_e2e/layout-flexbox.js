// M12.8 layout-CORRECTNESS gate: lifted (ARM->wasm) solver == native solver.
//
// Backed by examples/c/web-flexbox-simple.bin (5 nodes: body + flex container
// + 3 flex-grow children, pure boxes, NO fonts) on :8800. Drives the SAME
// bootstrap as layout-real.js through AzStartup_solveLayoutReal, reads the
// positioned rects, and asserts they EXACTLY equal the native reference rects in
// scripts/m9_e2e/flexbox-ref.json (produced by
// `cargo test -p azul-layout --test web_flexbox_simple_ref`). Equality proves the
// lift computes identical box geometry — "the layout is the same as what we
// calculate". flexbox-simple is 0px-vs-Chrome, so the rects are also correct
// browser geometry.
const http = require('http');
const fs = require('fs');
const path = require('path');

const REF = JSON.parse(fs.readFileSync(path.join(__dirname, 'flexbox-ref.json'), 'utf8'));
const EXPECT = REF.rects; // [[x,y,w,h], ...] node-indexed

function fetch_(p) {
    return new Promise((r,j) => http.get('http://127.0.0.1:8800' + p, x => {
        const c = []; x.on('data',b=>c.push(b)); x.on('end',()=>r(Buffer.concat(c))); x.on('error', j);
    }));
}
function fail(msg) { console.error('FAIL:', msg); process.exit(1); }

(async () => {
    const html = (await fetch_('/')).toString();
    const initialCounter = parseInt((html.match(/<div id="az_1">(\d+)<\/div>/) || ['', '0'])[1]);
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
    // remill memory-access helpers. The transpiler emits these `alwaysinline`
    // (transpiler_remill.rs:5781+) — `read_N(mem,addr)=load[addr]`,
    // `write_N(mem,addr,val)=store[addr];ret mem` — but they sometimes LEAK as
    // unresolved wasm imports (not inlined at every call site). Unprovided, the
    // generic stub returns 0 → every leaked memory READ yields 0 and silently
    // corrupts the lifted layout (same class as the fmaxf libcall bug). Provide
    // real impls (host supplies wasm imports). `addr` is the direct linear-memory
    // offset; the `mem` token is opaque and returned unchanged for writes.
    const dvOf = () => new DataView(memory.buffer);
    // [az-fix KEEP] __multi3 = compiler-rt 128-bit multiply, LEAKED as a wasm import (127 uses
    // in layout, also in mini) — same class as the fmaxf/fminf/roundf libcall leak. Unprovided,
    // stubFor returns 0 AND never writes the sret → every u128 multiply (Vec/Layout overflow
    // checks, ratio math) yields GARBAGE → corrupt alloc sizes/lengths. Real impl: result→[sret].
    // wasm sig (i32 sret, i64 aLo, i64 aHi, i64 bLo, i64 bHi) -> nil.
    const multi3 = (sret, aLo, aHi, bLo, bHi) => {
        const a = (BigInt.asUintN(64, BigInt(aHi)) << 64n) | BigInt.asUintN(64, BigInt(aLo));
        const b = (BigInt.asUintN(64, BigInt(bHi)) << 64n) | BigInt.asUintN(64, BigInt(bLo));
        const p = BigInt.asUintN(128, a * b);
        const dv = dvOf();
        dv.setBigUint64(Number(sret), p & 0xFFFFFFFFFFFFFFFFn, true);
        dv.setBigUint64(Number(sret) + 8, p >> 64n, true);
    };
    const REMILL_MEM = {
        __multi3: multi3,
        __remill_read_memory_8:  (m, a) => dvOf().getUint8(Number(a)),
        __remill_read_memory_16: (m, a) => dvOf().getUint16(Number(a), true),
        __remill_read_memory_32: (m, a) => dvOf().getUint32(Number(a), true),
        __remill_read_memory_64: (m, a) => dvOf().getBigUint64(Number(a), true),
        __remill_write_memory_8:  (m, a, v) => { dvOf().setUint8(Number(a), v & 0xff); return m; },
        __remill_write_memory_16: (m, a, v) => { dvOf().setUint16(Number(a), v & 0xffff, true); return m; },
        __remill_write_memory_32: (m, a, v) => { const an = Number(a);
            // [az-diag REVERT] LIVE-log layout step-marker writes (0x40700..0x40720) so we see the
            // LAST phase before a synchronous HANG in solveLayoutReal (the JS can't peek mid-hang).
            if (process.env.AZ_TRACE_STEPS === '1' && an >= 0x40700 && an <= 0x40720) process.stderr.write('[step] *0x'+an.toString(16)+'=0x'+((v>>>0).toString(16))+'\n');
            dvOf().setUint32(an, v >>> 0, true); return m; },
        __remill_write_memory_64: (m, a, v) => { dvOf().setBigUint64(Number(a), BigInt.asUintN(64, BigInt(v)), true); return m; },
    };
    const realEnv = {
        ...REMILL_MEM,
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

    const { instance: miniI } = await WebAssembly.instantiate(miniBytes, { env: new Proxy({}, h(realEnv)) });
    const mini = miniI.exports;
    memory = mini.memory;
    const AZ_BUMP_BASE = 96 * 1024 * 1024; // 100663296 (0x6000000)
    const cbEnv = { ...REMILL_MEM, memory, __indirect_function_table: table, memset: memsetImpl, memcpy: memcpyImpl, memmove: memcpyImpl };

    if (cbBytes) {
        const { instance: cbI } = await WebAssembly.instantiate(cbBytes, { env: new Proxy({}, h(cbEnv)) });
        table.grow(1); cbTableIdx = table.length - 1;
        table.set(cbTableIdx, cbI.exports.callback);
    }
    const { instance: layoutI } = await WebAssembly.instantiate(layoutBytes, { env: new Proxy({}, h(cbEnv)) });
    table.grow(1);
    const layoutTableIdx = table.length - 1;
    table.set(layoutTableIdx, layoutI.exports.callback);

    // Seed the bump heap base AFTER every module instantiation (each module's
    // data segments re-init shared memory, clobbering an earlier seed) and BEFORE
    // the first alloc (AzStartup_init's EventloopState). The lifted
    // `@__az_bump_ptr` (init 96 MiB) is sometimes dropped to ~0 by the link.
    if (typeof mini.AzStartup_resetBumpHeap === 'function') {
        mini.AzStartup_resetBumpHeap(AZ_BUMP_BASE);
        console.log('[boot] seeded bump heap → 0x' + (mini.AzStartup_snapshotBumpHeap() >>> 0).toString(16) + ' (post-instantiation, pre-init)');
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

    const viewportW = REF.viewport[0];
    const viewportH = REF.viewport[1];
    // Probe the bump allocator (initLayoutCache allocs 512 + 4096 for info/out ptrs).
    const pa1 = mini.AzStartup_alloc(512);
    const pa2 = mini.AzStartup_alloc(4096);
    const bumpFn = mini.AzStartup_snapshotBumpHeap;
    console.log('[probe] modelPtr=0x' + (modelPtr >>> 0).toString(16) + ' alloc(512)=0x' + (pa1 >>> 0).toString(16) + ' alloc(4096)=0x' + (pa2 >>> 0).toString(16) + ' bump=0x' + ((bumpFn ? bumpFn() : 0) >>> 0).toString(16));
    const memMiB = (memory.buffer.byteLength / 1048576).toFixed(1);
    console.log('[probe] mini.wasm memory = ' + memory.buffer.byteLength + ' bytes (' + memMiB + ' MiB); bump heap base = 96 MiB → need >=128 MiB');
    const pk = a => (typeof mini.AzStartup_peekU32 === 'function') ? (mini.AzStartup_peekU32(a) >>> 0).toString(16) : '?';
    console.log('[probe] bump markers: allocCallCount(0x40038)=' + pk(0x40038) + ' lastAllocSize(0x40030)=' + pk(0x40030) + ' lastRetPtr(0x40040)=' + pk(0x40040));
    let ilc;
    try {
        ilc = mini.AzStartup_initLayoutCache(state, viewportW, viewportH, 0);
    } catch (e) {
        // DIAG (2026-06-02 cb-OOB): the lifted layout cb (DOM construction) trapped.
        // Dump bump-alloc progress so we can see whether an alloc returned a bad
        // pointer right before the OOB (untranslated native ptr / heap exhaustion).
        console.error('initLayoutCache TRAPPED in lifted cb: ' + e.message);
        if (typeof mini.AzStartup_peekU32 === 'function') {
            const D = a => mini.AzStartup_peekU32(a) >>> 0;
            console.error('  cb progress MARK(0x40570) = ' + D(0x40570) + ' (web-flexbox-probe.c: 2=w/h 3=+box-sizing 4=+padding 5=+disp/%w/h 6=+border-width 7=+border-style 8=+border-color 10..12=items 99=complete)');
            // 2026-06-02 cb-OOB: trap is `i64.load(*(u32*)82344)` @cb+0x3a88 — cb loads a
            // pointer from static data addr 82344 and derefs it. Dump that pointer + a few
            // neighbours: >512MiB (0x20000000) or huge = untranslated native ptr (mirror miss).
            const p82344 = D(82344);
            console.error('  *(u32*)82344(0x141a8) = 0x' + p82344.toString(16) + (p82344 > 0x20000000 || p82344 === 0 ? '  ← BAD ptr (untranslated native / null) → deref OOB' : '  (in-range)'));
            console.error('  data@82336..82360: ' + [82336,82340,82344,82348,82352,82356,82360].map(a=>'['+a+']=0x'+D(a).toString(16)).join(' '));
            console.error('  memory now = ' + (memory.buffer.byteLength/1048576).toFixed(1) + ' MiB (' + memory.buffer.byteLength + ' B)');
            console.error('  bump POST-trap: allocCalls(0x40038)=' + D(0x40038) + ' lastAllocSize(0x40030)=' + D(0x40030) + ' lastRetPtr(0x40040)=0x' + D(0x40040).toString(16) + ' bumpNow=0x' + ((bumpFn?bumpFn():0)>>>0).toString(16));
            const epcL = D(0x400F0), epcH = D(0x400F4);
            if (epcL || epcH) console.error('  __remill_error PC=0x' + epcH.toString(16) + epcL.toString(16).padStart(8,'0'));
            const mbL = D(0x400F8), mbH = D(0x400FC);
            if (mbL || mbH) console.error('  __remill_MISSING_BLOCK PC=0x' + mbH.toString(16) + mbL.toString(16).padStart(8,'0'));
        }
        console.error(e.stack);
        process.exit(2);
    }
    if (ilc !== 0) fail('initLayoutCache rc=' + ilc + ' (1=null,2=cbidx,3=refany,4=alloc,100+X=cb returned X)');
    const curDom = (typeof mini.AzStartup_getCurrentDomPtr === 'function') ? mini.AzStartup_getCurrentDomPtr(state) : -1;
    const cbStat = (typeof mini.AzStartup_getLastLayoutStatus === 'function') ? mini.AzStartup_getLastLayoutStatus(state) : -1;
    let domHex = '';
    if (curDom > 0) {
        const b = new Uint8Array(memory.buffer, curDom, 48);
        domHex = Array.from(b).map(x => x.toString(16).padStart(2, '0')).join('');
    }
    console.log('[0] lifted cb ran: current_dom_ptr=0x' + (curDom >>> 0).toString(16) + ' cb_status=' + cbStat);
    console.log('    AzDom[0..48]=' + domHex + (domHex && /^0+$/.test(domHex) ? '  ← ALL ZERO (cb wrote nothing → X8/sret stomp)' : ''));
    const dumpHydrateProbe = () => {
        if (typeof mini.AzStartup_peekU32 !== 'function') return;
        const D = a => mini.AzStartup_peekU32(a) >>> 0;
        const reached = D(0x40620), bodyR = D(0x40624), btnR = D(0x40628), btnC = D(0x4062C), convN = D(0x40634), convBtnR = D(0x40630);
        console.error('  [hydrate PROBE] reached(0x40620)=0x' + reached.toString(16)
            + ((reached >>> 16) === 0x4042 ? (' (body.children=' + (reached & 0xFFFF) + ')') : ' ← probe NOT reached')
            + ' | body.style.rules=' + bodyR
            + ' | button.style.rules(0x40628)=' + (btnR === 0xBEEF0000 ? '0xBEEF → reading button.style OOB → BUILD corrupt (lifted cb)' : ((btnR>>>16)===0x4200 ? (btnR & 0xFFFF)+' (build OK)' : '0x'+btnR.toString(16)))
            + ' | button.children=' + btnC
            + ' | cascadeMarker(0x40634)=' + (convN === 0xCA500000 ? '0xCA50_0000 → real cascade StyledDom::create TRAPPED (node_type-move broke convert)' : convN === 0xCA500001 ? '0xCA50_0001 → real cascade SURVIVED ✓ (node_type-move OK)' : convN === 0xC0DE0000 ? '0xC0DE → old clone/convert OOB' : '0x' + convN.toString(16))
            + ' | converted button.style.rules=' + convBtnR);
    };
    const LENIENT = process.env.AZ_LENIENT === '1';
    // SKIP the diagnostic hydrate probe entirely: its convert_dom_into_compact_dom(dom_ref.clone())
    // @eventloop.rs:1137 traps (fragile Dom deep-clone) and the partial alloc CORRUPTS the bump heap,
    // which then OOBs the next AzStartup_alloc. solveLayoutReal runs the REAL cascade independently.

    // === WEB-LIFT DISC-RESTORE (JS-side) ===
    // The lifted cascade drops data-variant node_type discriminants (NodeType::Text 177->0) via
    // into_library_owned_nodetype's sret store. We CANNOT fix this in Rust: any edit to the cascade
    // or hydrate path re-codegens the lifted cascade into an OOB-trapping shape (LTO/lift instability).
    // So we patch it in JS with ZERO Rust perturbation: walk the cb's Dom for the real discs NOW
    // (before hydrate consumes it), then write them into the styled_dom's node_data AFTER hydrate.
    let capturedDiscs = [];
    // Struct layout consts extracted NATIVELY (core::mem::offset_of!; these structs have no cfg-gated
    // fields → build-independent). HARDCODED rather than read from a Rust export because adding ANY
    // function to the dylib deterministically re-codegens the lifted cascade into an OOB trap.
    let structConsts = { SZ_DOM: 240, OFF_CHILDREN: 152, SZ_ND: 152, OFF_NT: 0, OFF_NODEDATA: 48 };
    if (typeof mini.AzStartup_getCurrentDomPtr === 'function') {
        const dv = new DataView(memory.buffer), u8 = new Uint8Array(memory.buffer);
        const domPtr = mini.AzStartup_getCurrentDomPtr(state) >>> 0;
        // DFS pre-order (matches convert_dom_into_compact_dom node-id order): node disc, then children
        // left-to-right. Dom.root is a NodeData @ offset 0; node_type disc @ OFF_NT. children: DomVec
        // @ OFF_CHILDREN { ptr@0, len@8 }; each child Dom is SZ_DOM bytes.
        const seen = new Set();
        (function walk(nodePtr, depth) {
            if (!nodePtr || depth > 64 || seen.has(nodePtr)) return; seen.add(nodePtr);
            capturedDiscs.push(u8[nodePtr + structConsts.OFF_NT]);
            // [az-diag REVERT] read the CB-DOM (pre-cascade) Text node's AzString raw fields.
            // If correct here (len=5 "Hello") but garbage in styled_dom → the CASCADE
            // (create_from_dom) corrupts the AzString; if garbage here → cb-build does.
            if (u8[nodePtr + structConsts.OFF_NT] === 177) {
                const azPtr = Number(dv.getBigUint64(nodePtr + 8, true));
                const azLen = Number(dv.getBigUint64(nodePtr + 16, true));
                const azCap = Number(dv.getBigUint64(nodePtr + 24, true));
                // [az-diag REVERT] NodeType::Text holds BoxOrStatic<AzString> (a HEAP PTR), NOT a
                // raw AzString — so node+8/+16 are the BoxOrStatic tag/ptr, NOT the AzString. Deref
                // the box (node+16) to read the REAL AzString.
                const boxPtr = azLen & 0xFFFFFFFF;
                if (boxPtr > 0x6000000 && (boxPtr + 24) < memory.buffer.byteLength) {
                    const bP = Number(dv.getBigUint64(boxPtr, true)) & 0xFFFFFFFF;
                    const bL = Number(dv.getBigUint64(boxPtr + 8, true));
                    const bC = Number(dv.getBigUint64(boxPtr + 16, true));
                    let bt = ''; if (bL > 0 && bL < 64 && bP > 0 && (bP + bL) < memory.buffer.byteLength) for (let k = 0; k < bL; k++) { const c = u8[bP + k]; bt += (c >= 32 && c < 127) ? String.fromCharCode(c) : '.'; }
                    console.log('[boxed-azstring] BoxOrStatic ptr@node+16=0x' + boxPtr.toString(16) + ' → boxed AzString {ptr=0x' + bP.toString(16) + ' len=' + bL + ' cap=' + bC + '} text="' + bt + '" → ' + (bL === 5 && bt === 'Hello' ? '✓✓✓ TEXT IS CORRECT (the layout hang is downstream: box-deref/shaping)' : 'still wrong'));
                }
                let txt = ''; const p = azPtr & 0xFFFFFFFF;
                if (azLen > 0 && azLen < 64 && p > 0 && (p + azLen) < memory.buffer.byteLength) for (let k = 0; k < azLen; k++) { const c = u8[p + k]; txt += (c >= 32 && c < 127) ? String.fromCharCode(c) : '.'; }
                console.log('[cb-dom-azstring] CB-DOM (pre-cascade) Text @0x' + nodePtr.toString(16) + ' AzString: ptr=0x' + azPtr.toString(16) + ' len=' + azLen + ' cap=' + azCap + ' text="' + txt + '" (expect len=5 "Hello")');
                // [az-diag REVERT] raw hex of the NodeType (48 bytes @ nodePtr) to see the real layout.
                let hx = ''; for (let k = 0; k < 48; k++) hx += u8[nodePtr + k].toString(16).padStart(2, '0');
                console.log('[cb-dom-azstring]   NodeType[0..48]=' + hx);
                // scan ALL low memory (user-binary band 0x10000.. AND heap 0x6000000..) for "Hello".
                const needle = [72, 101, 108, 108, 111]; const hits = [];
                const scan = (lo, hi) => { for (let a = lo; a < hi && hits.length < 8; a++) { let m = true; for (let k = 0; k < 5; k++) if (u8[a + k] !== needle[k]) { m = false; break; } if (m) hits.push('0x' + a.toString(16)); } };
                scan(0x10000, 0x400000);          // user-binary data band (mirrored __cstring/__const)
                scan(0x6000000, 0x6200000);       // bump heap (copyFromBytes destination)
                console.log('[cb-dom-azstring]   "Hello" found at: ' + (hits.length ? hits.join(' ') : 'NOT FOUND in user-binary band OR heap → literal NOT mirrored AND not copied'));
            }
            const dvec = nodePtr + structConsts.OFF_CHILDREN;
            const cptr = Number(dv.getBigUint64(dvec, true) & 0xFFFFFFFFn);
            const clen = Number(dv.getBigUint64(dvec + 8, true));
            for (let i = 0; i < clen && i < 4096; i++) walk(cptr + i * structConsts.SZ_DOM, depth + 1);
        })(domPtr, 0);
        console.log('[disc-restore-js] consts SZ_DOM=' + structConsts.SZ_DOM + ' OFF_CHILDREN=' + structConsts.OFF_CHILDREN
            + ' SZ_ND=' + structConsts.SZ_ND + ' OFF_NT=' + structConsts.OFF_NT + ' OFF_NODEDATA=' + structConsts.OFF_NODEDATA
            + ' | captured ' + capturedDiscs.length + ' discs=[' + capturedDiscs.join(',') + '] (177=Text)');
    }

    const SKIP_HYDRATE = process.env.AZ_SKIP_HYDRATE === '1';
    let hrc, hydrateTrapped = SKIP_HYDRATE;
    if (SKIP_HYDRATE) console.error('[skip] hydrate DIAGNOSTIC probe skipped (heap-corrupting clone); solveLayoutReal runs the real cascade');
    if (!SKIP_HYDRATE) {
    try {
        hrc = mini.AzStartup_hydrateStyledDom(state);
    } catch (e) {
        dumpHydrateProbe();
        console.error('hydrateStyledDom TRAPPED (cascade StyledDom::create): ' + e.message);
        console.error('HYDRATE TRAP STACK:\n' + e.stack);
        hydrateTrapped = true;
    }
    }
    if (!hydrateTrapped) {
    dumpHydrateProbe();
    if (hrc !== 0) fail('hydrateStyledDom rc=' + hrc + ' (1=null state, 2=current_dom_ptr==0, 3=tree walk 0 nodes); current_dom_ptr=0x' + (curDom >>> 0).toString(16) + ' cb_status=' + cbStat);
    const styledNodes = mini.AzStartup_getStyledDomNodeCount(state);
    console.log('[1] cascade ok: styled_dom node_count=' + styledNodes + ' (expected ' + EXPECT.length + ')');
    // PRE-CASCADE inline-CSS-parse probe: did the lifted Css::parse_inline produce rule blocks?
    // RELIABLE inline-rule count (state field via getter, not a fixed-addr marker):
    if (typeof mini.AzStartup_getRootRuleCount === 'function') {
        const rcRaw = mini.AzStartup_getRootRuleCount(state) >>> 0;
        const rc = rcRaw >> 8, pcTag = rcRaw & 0xff;
        console.log('[1] body root.style.rules.len=' + rc + ', PRE-cascade rule[0] CssProperty tag=' + pcTag + (
            pcTag === 53 ? ' (=Height 53 → cb STORED a correct CssProperty → the CASCADE CLONE zeroes it)'
            : pcTag === 0 ? ' (=0 → cb storage/sret ALREADY zeroed the CssProperty before the cascade)'
            : pcTag === 0xff ? ' (no rules)' : ' (tag ' + pcTag + ')'));
    }
    if (typeof mini.AzStartup_getBodyHeightRaw === 'function') {
        const hr = mini.AzStartup_getBodyHeightRaw(state) >>> 0;
        let msg;
        if ((hr >>> 16) === 0xCE00) { const tag = hr & 0xff; msg = 'EXPLICIT-LOOP prop[0]=' + tag + (tag === 53 ? ' (=Height 53 → explicit loop yields CORRECT ref, but apply jump-table did not set height → fix=if-let dispatch)' : tag === 0 ? ' (=0 → explicit loop ALSO yields bad/zeroed ref → deeper than flat_map)' : ' (tag ' + tag + ')'); }
        else if ((hr >>> 16) === 0xCD00) { const p0 = (hr >> 8) & 0xff, p8 = hr & 0xff; msg = 'a-vs-b: prop[0]=' + p0 + ' prop[8]=' + p8 + (p8 === 53 ? ' (prop[8]=Height53 → prop points at CssDeclaration, OFF by payload offset → mechanism (b) enum-payload offset mis-lift)' : p8 === 0 ? ' (both 0 → CssProperty ZEROED → mechanism (a) construction zeroes the tag)' : ' (prop[8]=' + p8 + ' → other offset)'); }
        else if ((hr >>> 16) === 0xCC00) { const it = (hr >> 8) & 0xff, tag = hr & 0xff; msg = 'RAW-TAG: iter=' + it + ' tag=' + tag; }
        else if (hr === 0xDEAD0000) msg = 'compact_cache NONE (not built during cascade)';
        else if (hr === 0xABCD0000) msg = 'Height if-let MATCHED → discriminant read WORKS (bug is is_normal-continue or apply jump-table)';
        else if (hr === 1) msg = 'inline loop yielded ONLY 1 prop (width) — height NEVER reached → iter_inline_properties flat_map mis-lifts (stops after rule 1)';
        else if (hr === 2) msg = '2 props yielded but Height if-let did NOT match → CssProperty DISCRIMINANT READ mis-lifts';
        else if (hr > 0 && hr < 16) msg = 'inline loop yielded ' + hr + ' props, Height if-let not matched';
        else if (hr >= 0xFFFFFFF9) msg = 'SENTINEL/auto → inline loop did NOT iterate (iter_inline_properties yielded 0 despite rules.len=2)';
        else msg = 'compact height = ' + (((hr|0) >> 4) / 1000) + 'px (metric ' + (hr & 0xF) + ') → cascade APPLIED it; loss is in getter/solver';
        console.log('[1] body compact_cache height_raw=0x' + hr.toString(16) + ' → ' + msg);
    }
    if (typeof mini.AzStartup_getStyledNode0Rules === 'function') {
        const sr = mini.AzStartup_getStyledNode0Rules(state) >>> 0;
        console.log('[1] POST-conversion compact node_data[0].style.rules.len = ' + (sr === 0xDEAD0000 ? 'NONE' : sr) + (sr === 2
            ? ' → conversion PRESERVED rules → loss is in build_compact_cache loop/apply'
            : (sr === 0 ? ' → conversion DROPPED rules (lifted clone) → root cause'
            : ' → unexpected')));
    }
    if (typeof mini.AzStartup_testGetProperty === 'function') {
        const gp = mini.AzStartup_testGetProperty(state) >>> 0;
        const ctag = gp & 0xff;
        console.log('[1] eventloop-construct CssProperty::Height tag = ' + ctag + (ctag === 53
            ? ' → CssProperty CONSTRUCTION works in lift → cb-path/clone zeroes it (storage/copy bug)'
            : ctag === 0 ? ' → CssProperty CONSTRUCTION itself zeroes the discriminant (M11/M12 struct-construct-reads-zero class)'
            : ' → tag ' + ctag + ' (unexpected)'));
    }
    if (styledNodes !== EXPECT.length && !LENIENT) fail('styled node_count ' + styledNodes + ' != expected ' + EXPECT.length);
    } // end if (!hydrateTrapped) — hydrate is a diagnostic; solveLayoutReal runs the real cascade

    // === WEB-LIFT DISC-RESTORE (JS-side), part 2: patch styled_dom.node_data discriminants ===
    if (structConsts && capturedDiscs.length && typeof mini.AzStartup_getStyledDomPtr === 'function') {
        const styledPtr = mini.AzStartup_getStyledDomPtr(state) >>> 0;
        if (!styledPtr) {
            console.log('[disc-restore-js] styled_dom ptr is 0 (hydrate did not produce a styled_dom) — cannot patch');
        } else {
            const dv = new DataView(memory.buffer), u8 = new Uint8Array(memory.buffer);
            const ndVec = styledPtr + structConsts.OFF_NODEDATA;     // NodeDataVec within StyledDom
            const ndPtr = Number(dv.getBigUint64(ndVec, true) & 0xFFFFFFFFn);   // .ptr @ 0
            const ndLen = Number(dv.getBigUint64(ndVec + 8, true));            // .len @ 8
            let restored = 0, boxLost = 0;
            const report = [];
            for (let i = 0; i < capturedDiscs.length && i < ndLen; i++) {
                const ntp = ndPtr + i * structConsts.SZ_ND + structConsts.OFF_NT; // node_type disc
                const cur = u8[ntp];
                const boxPtr = Number(dv.getBigUint64(ntp + 8, true) & 0xFFFFFFFFn); // payload ptr @ +8
                if (cur !== capturedDiscs[i]) {
                    if (boxPtr !== 0) { u8[ntp] = capturedDiscs[i]; restored++; report.push('n' + i + ':' + cur + '->' + capturedDiscs[i] + '(box=0x' + boxPtr.toString(16) + ')'); }
                    else { boxLost++; report.push('n' + i + ':disc=' + cur + ' want ' + capturedDiscs[i] + ' BUT box@8=0 (LOST)'); }
                }
            }
            console.log('[disc-restore-js] styled_dom node_data: ptr=0x' + ndPtr.toString(16) + ' len=' + ndLen
                + ' | restored=' + restored + ' boxLost=' + boxLost + (report.length ? ' | ' + report.join(' ') : ' (all discs already correct)'));
            // [az-diag REVERT] read each Text node's AzString RAW fields DIRECTLY from styled_dom
            // memory (NO lifted code). NodeType repr(C,u8): disc@0, AzString payload@8 (ptr@8,
            // len@16, cap@24). Definitive: if len=5/ptr~0x6xxxxxx/text="Hello" the AzString is
            // FINE in styled_dom → the lifted extract_text_from_node mis-lifts the ACCESS; if
            // garbage → the AzString PAYLOAD is corrupted in styled_dom (cascade/cb-build).
            for (let i = 0; i < capturedDiscs.length && i < ndLen; i++) {
                if (capturedDiscs[i] !== 177) continue; // Text
                const base = ndPtr + i * structConsts.SZ_ND;
                const azPtr = Number(dv.getBigUint64(base + 8, true));
                const azLen = Number(dv.getBigUint64(base + 16, true));
                const azCap = Number(dv.getBigUint64(base + 24, true));
                let txt = '';
                const p = azPtr & 0xFFFFFFFF;
                if (azLen > 0 && azLen < 64 && p > 0 && (p + azLen) < memory.buffer.byteLength) {
                    for (let k = 0; k < azLen; k++) { const c = u8[p + k]; txt += (c >= 32 && c < 127) ? String.fromCharCode(c) : '.'; }
                }
                console.log('[az-string-js] Text node ' + i + ' AzString: ptr=0x' + azPtr.toString(16) + ' len=' + azLen + ' cap=' + azCap + ' text="' + txt + '" (expect ptr~0x6xxxxxx len=5 "Hello")');
            }
        }
    }

    // WEB-FONT-VIA-JS (2026-06-02): the embedded font const isn't reliably mirrored into the
    // lifted wasm (read by dynamic index → only the header lands). Provide the TTF at runtime:
    // alloc a wasm buffer, copy the font into wasm linear memory, register it. solveLayoutReal
    // then reads REAL bytes (no const-mirror / synth-mapping dependence).
    if (typeof mini.AzStartup_setFallbackFont === 'function') {
        const fontBytes = fs.readFileSync(path.join(__dirname, '../../doc/fonts/SourceSerifPro-Regular.ttf'));
        const fptr = mini.AzStartup_alloc(fontBytes.length) >>> 0;
        new Uint8Array(memory.buffer).set(fontBytes, fptr);
        mini.AzStartup_setFallbackFont(fptr, fontBytes.length);
        console.log('[font-js] registered fallback font: ptr=0x' + fptr.toString(16) + ' len=' + fontBytes.length
            + ' (mem ' + (memory.buffer.byteLength/1048576).toFixed(1) + ' MiB)');
    } else {
        console.log('[font-js] AzStartup_setFallbackFont NOT exported — falling back to the (partial) const');
    }

    // [az-diag REVERT] EARLY cb-return read (before solveLayoutReal, which HANGS on the corrupt
    // huge len). Shows what AzString_copyFromBytes RETURNED (web-text-min.c 0x40760/64/68) so we
    // can split copy_from_bytes-empty-return vs createText-corrupts. AZ_EARLY_EXIT=1 stops here.
    if (typeof mini.AzStartup_peekU32 === 'function') {
        const D = a => mini.AzStartup_peekU32(a) >>> 0;
        const cbRan = D(0x40760);
        console.error('EARLY cb-return: marker(0x40760)=0x' + cbRan.toString(16)
            + (cbRan === 0xc0de ? (' → cb ran | s.ptr(0x40764)=0x' + D(0x40764).toString(16)
                + ' s.len(0x40768)=' + D(0x40768)
                + (D(0x40768) === 5 && D(0x40764) > 0x6000000 ? ' → ✓✓ copy_from_bytes RETURNED CORRECT {heap,5} → createText corrupts the node'
                   : ' → copy_from_bytes RETURNED {ptr=0x' + D(0x40764).toString(16) + ',len=' + D(0x40768) + '} (still wrong)')) : ' → cb-return not set'));
        const extRan = D(0x40750);
        console.error('EARLY extern-input: marker(0x40750)=0x' + extRan.toString(16)
            + (extRan === 0xe0e0 ? (' | len(0x40754)=' + D(0x40754) + ' ptr(0x40758)=0x' + D(0x40758).toString(16)) : ''));
    }
    if (process.env.AZ_EARLY_EXIT === '1') { console.error('[AZ_EARLY_EXIT] stopping before solveLayoutReal (hangs on corrupt len)'); process.exit(0); }

    if (typeof mini.AzStartup_solveLayoutReal !== 'function') fail('AzStartup_solveLayoutReal not exported (build/lift gap)');
    let rc;
    try {
        rc = mini.AzStartup_solveLayoutReal(state, viewportW, viewportH);
        // [az-diag REVERT] scan_cursor fuel-cap marker (layout COMPLETED, not hung)
        { const fc = mini.AzStartup_peekU32(0x40790) >>> 0;
          console.error('FUEL-CAP scan_cursor(0x40790)=0x' + fc.toString(16) + (fc === 0x5ca90001
            ? ' → ✓✓ HIT: the scan_cursor loop (cache.rs:5966) WAS the infinite loop (len_utf8/cursor mis-lifted to 0). Layout completed via cap.'
            : ' → NOT hit → the infinite loop is a DIFFERENT loop (not scan_cursor)')); }
        // [az-diag REVERT] shaping phase/glyph readout on SUCCESS (layout completed, no hang)
        { const shp = mini.AzStartup_peekU32(0x40700) >>> 0, sph = mini.AzStartup_peekU32(0x407A0) >>> 0;
          console.error('SUCCESS shaping probe(0x40700)=0x' + shp.toString(16) + ' phase(0x407A0)=' + sph
            + (shp>0 && shp<0x1000 ? ' → ✓✓✓ SHAPING COMPLETED with ' + shp + ' glyphs (text MEASURED)' : '')); }
        // [az-diag g51 REVERT] BISECTION: last layout-phase marker @0x40704 (mod.rs). Tells WHERE
        // InvalidTree fires: <0x71=in reconcile(520); 0x71=in intrinsic-clear/compute_counters(524-558);
        // 0x72=in remap/mark_dirty/ctx/early-exit(560-741); 0x80=in the layout loop pre-sizing(742-762);
        // 0x90=reached calculate_intrinsic_sizes (InvalidTree is NOT in the pre-sizing path).
        { const ph = mini.AzStartup_peekU32(0x40704) >>> 0;
          const seg = ph===0x90?'reached calc_intrinsic (past pre-sizing)':ph===0x80?'InvalidTree in layout-loop 742-762':ph===0x72?'InvalidTree in remap/mark_dirty/ctx/early-exit 560-741':ph===0x71?'InvalidTree in intrinsic-clear/compute_counters 524-558':'InvalidTree IN reconcile_and_invalidate (520) — marker never written';
          console.error('BISECT layout-phase(0x40704)=0x' + ph.toString(16) + ' → ' + seg); }
        // [g73] the phase marker is at 0x60704 (free band); 0x40704 overlaps the wasm stack → garbage.
        { const ph6 = mini.AzStartup_peekU32(0x60704) >>> 0;
          const seg6 = {0x71:'compute_counters/intrinsic-clear (mod530)',0x72:'compute_counters done (mod573)',0x80:'layout-loop pre-sizing (mod757)',0x81:'about to collect_and_resolve_chains (win912)',0x82:'chains resolved (win920)',0x83:'about to load_missing (win979)',0x84:'load_missing done (win988)',0x85:'font-block done (win1010)',0x90:'about to calculate_intrinsic_sizes (mod799)',0x91:'calc_intrinsic DONE (mod819)',0xA1:'collect_inline done→measure (sizing596)',0xA2:'measure_intrinsic_widths DONE (sizing639)'}[ph6] || ('0x'+ph6.toString(16));
          console.error('BISECT [g73] layout-phase(0x60704)=0x' + ph6.toString(16) + ' → LAST PHASE: ' + seg6); }
        // [g73] inline-content localization (all free-band 0x60xxx, read live): phase 0xA0 = inside
        // collect_inline_content (sizing.rs:594). These pinpoint WHERE in the IFC collection it bails.
        { const P=a=>mini.AzStartup_peekU32(a)>>>0;
          const ip6=P(0x6071C), tl6=P(0x60714), tp6=P(0x60718), ni=P(0x60720), ci=P(0x60728),
                tr=P(0x60730), tn=P(0x60734), trs=P(0x60738), plc=P(0x60708), plcc=P(0x6070C);
          const recNi=P(0x60754), ifcCnt=P(0x60758), ifcNi=P(0x6075C);
          const ipn={0:'NOT reached collect_inline_content_recursive (InvalidTree BEFORE it)',0xB1:'entry/extract_text',0xB2:'extract returned (len@714 must be 5)',0xB3:'split done',0xB4:'content.extend done (past split)',0xB6:'DOM-children loop done→process_layout_children',0xB8:'collect_inline COMPLETE (recursion Ok)',0xBAD:'*** entry tree.get(node_index)=None → InvalidTree (node@0x60754) ***',0xA3:'DOM-child text path',0xB5:'AzString-field probe'}[ip6]||('0x'+ip6.toString(16));
          console.error('BISECT [g73] inline-phase(0x6071C)=0x'+ip6.toString(16)+' → '+ipn);
          console.error('BISECT [g75] IFC-sizer calls(0x60758)='+ifcCnt+' lastIFCnode(0x6075C)='+ifcNi+' | last recursion node_index(0x60754)='+recNi+(ip6===0xBAD?' ← FAILING node_index':''));
          { const cres=P(0x60760); console.error('BISECT [g76] collect_inline_content RESULT(0x60760)=0x'+cres.toString(16)+' → '+(cres===1?'Ok (B8 path returned Ok correctly = #2 FIXED)':cres===0xEE?'*** Err DESPITE B8 = Result<Vec,_> RETURN MIS-LIFT (Ok→Err) ***':'not set')); }
          { const ph=P(0x60704),cl=P(0x60768),lf=P(0x6076C);
            console.error('BISECT [g79] pre-measure: phase(0x60704)=0x'+ph.toString(16)+(ph===0xA15?' (reached pre-measure → #2 fixed, collect Ok)':'')+' | font_chain_cache.len(0x60768)='+cl+' loaded_fonts.len(0x6076C)='+lf+(cl===0&&ph===0xA15?'  ← *** CHAIN EMPTY at shaping = #4 is ROOT of #3 hang ***':cl>0?'  ← chain NON-empty (hang is a different empty map)':'')); }
          { const tag=P(0x606C0),nc=P(0x606C4),ufk=P(0x606C8),rt=P(0x606CC),n1b0=P(0x606D0),n1b1=P(0x606D4),n1b2=P(0x606D8),n1b4=P(0x606DC),n0b0=P(0x606E0);
            console.error('BISECT [g80b] collect_font_stacks: tag(0x606C0)=0x'+tag.toString(16)+' node_count(0x606C4)='+nc+' unique_font_keys.len(0x606C8)='+ufk+' raw_text#(0x606CC)='+rt+' | n1.nt[0,1,2,4]=['+n1b0+','+n1b1+','+n1b2+','+n1b4+'] n0.nt[0]='+n0b0+(ufk===0?'  ← *** Phase1 matched 0 text nodes (node_type 177 check fails → 0 chains) ***':'  ← Phase1 matched '+ufk+' → Phase2/resolution drops them')); }
          { const a=P(0x60770),b=P(0x60774),c=P(0x60778),d=P(0x6077C);
            console.error('BISECT [g80] chain pipeline (window.rs): afterResolve(0x60770)='+a+' afterLastResort(0x60774)='+b+' fc_chains afterIntoFontconfig(0x60778)='+c+' font_chain_cache afterSet(0x6077C)='+d+'  → '+(a===0?'collect_and_resolve produced 0 chains (resolution/collect_font_stacks empty)':c===0&&b>0?'into_fontconfig_chains DROPPED them (BTreeMap rebuild mis-lift)':d===0&&c>0?'set_font_chain_cache DROPPED them':a>0?'chains survive pipeline — drop is elsewhere/early-exit':'font block not reached (0x60770 unset → early-exit skipped resolution)')); }
          console.error('BISECT [g73] textlen(0x60714)='+tl6+' textptr(0x60718)=0x'+tp6.toString(16)+' | last node_index(0x60720)='+ni+' child_index(0x60728)='+ci);
          console.error('BISECT [g73] process_layout_children: node(0x60708)=0x'+plc.toString(16)+' lastChild(0x6070C)='+plcc);
          console.error('BISECT [g73] LayoutTree@sizing: root(0x60730)='+tr+' nodes.len(0x60734)='+tn+' get(root).is_some(0x60738)='+trs); }
        // [az-diag g51 REVERT] inline-content phase (sizing.rs:994/1001/1013/1039): 0=never reached
        // collect_inline_content_recursive (InvalidTree is in calculate_intrinsic_recursive:226 /
        // node sizers 330/633/748, BEFORE inline collection); B1=at extract_text; B2=extract returned
        // (len@0x40714 must be 5 for "Hello"); B3=split done; A3=DOM-child text path. 0x40714=text len,
        // 0x40718=text ptr (0/garbage = the Text AzString deref mis-lift confirmed at the sizing stage).
        { const ip = mini.AzStartup_peekU32(0x4071C) >>> 0, tl = mini.AzStartup_peekU32(0x40714) >>> 0, tp = mini.AzStartup_peekU32(0x40718) >>> 0;
          const ipn = {0:'NOT reached (InvalidTree before inline collection)',0xB1:'at extract_text',0xB2:'extract returned',0xB3:'split done',0xA3:'DOM-child text path'}[ip] || ('0x'+ip.toString(16));
          console.error('BISECT inline-phase(0x4071C)=0x' + ip.toString(16) + ' (' + ipn + ') | text.len(0x40714)=' + tl + ' text.ptr(0x40718)=0x' + tp.toString(16) + (tl===5?' ✓len5':(ip>=0xB1?' ✗LEN-GARBAGE (AzString mis-lift)':''))); }
        // [az-diag g52 REVERT] intrinsic recursion: last node_index entered (0x40720) + last
        // child_index recursed (0x40728). If the stray-child hypothesis holds, 0x40728 is an
        // OUT-OF-RANGE index (>= node_count=2). With the g52 guard, sizing should now advance
        // past it (inline-phase above should leave 0 → reach B1/B2).
        { const ni = mini.AzStartup_peekU32(0x40720) >>> 0, ci = mini.AzStartup_peekU32(0x40728) >>> 0;
          console.error('BISECT intrinsic-recursion: last node_index(0x40720)=' + ni + ' last child_index(0x40728)=' + ci + (ci>=2?' ← STRAY (>= node_count 2)':'')); }
        // [az-diag g53 REVERT] DECISIVE LayoutTree probe: root(0x40730), nodes.len(0x40734),
        // tree.get(root).is_some(0x40738). If len=0 or is_some=0 → reconcile built a BROKEN tree
        // (root cause = reconcile, not sizing); InvalidTree@229 is tree.get(root)=None.
        { const rt = mini.AzStartup_peekU32(0x40730) >>> 0, nl = mini.AzStartup_peekU32(0x40734) >>> 0, gs = mini.AzStartup_peekU32(0x40738) >>> 0;
          console.error('BISECT LayoutTree: root(0x40730)=' + rt + ' nodes.len(0x40734)=' + nl + ' get(root).is_some(0x40738)=' + gs + (nl===0||gs===0?' ← ✗ BROKEN TREE from reconcile (NOT a sizing bug)':' ← tree OK, InvalidTree is deeper')); }
        // [az-diag g54 REVERT] new_tree.nodes.len() at two points: 0x40740=right after reconcile,
        // 0x40744=at the layout loop body. 0 after reconcile → reconcile built nothing (the bug).
        // 2 after reconcile but 0 at sizing → emptied/mis-lifted downstream.
        { const n1 = mini.AzStartup_peekU32(0x40740) >>> 0, n2 = mini.AzStartup_peekU32(0x40744) >>> 0;
          console.error('BISECT new_tree.nodes.len: post-reconcile(0x40740)=' + n1 + ' loop-body(0x40744)=' + n2 + (n1===0?' ← RECONCILE built 0 nodes (root cause = reconcile)':(n2===0?' ← emptied AFTER reconcile':' ← nodes present pre-sizing, lost at sizing call'))); }
        // [az-diag g54] create_node_from_dom call-count (0x40500, pre-existing marker @layout_tree.rs:2020)
        // + dom_id ring (0x40504+i*4). count=0 → reconcile_recursive NEVER created nodes (bug in
        // reconcile_recursive dispatch). count>0 but nodes.len=0 → push LOST in builder &mut threading
        // (or build() drops them). Reveals whether the bug is node-CREATION or node-RETENTION.
        { const cc = mini.AzStartup_peekU32(0x40500) >>> 0;
          let ring = []; for (let i=0;i<Math.min(cc,14);i++){ const v = mini.AzStartup_peekU32(0x40504+i*4)>>>0; if(v) ring.push('dom'+(v&0xffff)); }
          console.error('BISECT create_node_from_dom: callCount(0x40500)=' + cc + ' ring=[' + ring.join(',') + ']' + (cc===0?' ← reconcile NEVER created nodes (bug=reconcile_recursive)':' ← nodes CREATED but lost before build (bug=builder threading/build)')); }
        // [az-diag g55] &mut new_tree pointer: caller(0x40748) vs callee(0x4075C). caller-len(0x4074C).
        { const pc = mini.AzStartup_peekU32(0x40748) >>> 0, le = mini.AzStartup_peekU32(0x4074C) >>> 0, pe = mini.AzStartup_peekU32(0x4075C) >>> 0;
          console.error('BISECT &mut tree ptr: caller(0x40748)=0x' + pc.toString(16) + ' (len=' + le + ') vs callee(0x4075C)=0x' + pe.toString(16) + (pc===pe?' ← SAME ptr → nodes-field-offset mis-lift in callee':' ← DIFFERENT ptr → &mut arg MIS-PASSED across lifted call')); }
        // [az-diag g63] MARKER-CLOBBER TEST: dense mod.rs loop-body markers (0x40744/4C/0x407C0/C4/0x40748)
        // REMOVED. If 0x407B0 (sizing entry nodes.len) now reads 2, those marker WRITES were clobbering
        // new_tree's stack slot (wasm stack grows into 0x407xx during deep layout). post-reconcile=0x40740.
        { const nPostRecon = mini.AzStartup_peekU32(0x60740) >>> 0, nEntry = mini.AzStartup_peekU32(0x607B0) >>> 0, nLine142 = mini.AzStartup_peekU32(0x60734) >>> 0;
          console.error('BISECT g68 [markers→0x60xxx free band] nodes.len: postReconcile(0x60740)=' + nPostRecon + ' sizingEntry(0x607B0)=' + nEntry + ' line142(0x60734)=' + nLine142
            + (nEntry>=1 && nEntry<100 ? '  ← ✓✓ new_tree SURVIVED into sizing! dense markers WERE clobbering' : '  ← still 0 at sizing (real corruption or other markers clobber)')); }
        // [az-diag g66→g70 free-band] right AFTER the clone: src new_tree(0x607C0) vs clone cache.tree(0x607C4).
        { const nSrc = mini.AzStartup_peekU32(0x607C0) >>> 0, nClone = mini.AzStartup_peekU32(0x607C4) >>> 0;
          console.error('BISECT g70 at-clone: src new_tree(0x607C0)=' + nSrc + ' clone cache.tree(0x607C4)=' + nClone
            + (nSrc===2 && nClone===1 ? '  ← ✓ Vec::clone MIS-LIFTS (drops node) → MOVE-based refactor will work' : (nSrc===nClone && nSrc<2 ? '  ← corruption reached line 758 (heisenbug) → move won\'t help' : (nSrc===2 && nClone===2 ? '  ← clone OK at 758; corruption is 758→786 (reaches heap) → move won\'t help' : '')))); }
        // [az-diag g70 RELIABLE finer bisect] which op in 770→803 corrupts new_tree (markers in free band).
        { const nAtClone = mini.AzStartup_peekU32(0x607C0) >>> 0, nAfterCP = mini.AzStartup_peekU32(0x60780) >>> 0, nAfterReset = mini.AzStartup_peekU32(0x60784) >>> 0, nAfterSpan = mini.AzStartup_peekU32(0x60788) >>> 0, nBeforeCall = mini.AzStartup_peekU32(0x60748) >>> 0;
          let culprit = nAtClone!==2 ? 'already bad at-clone' : nAfterCP!==2 ? 'the in-loop calculated_positions.clone() (783-785)' : nAfterReset!==2 ? 'reset_peak() (790)' : nAfterSpan!==2 ? 'Probe::span("calc_intrinsic_sizes") (791)' : nBeforeCall!==2 ? 'between Span and the call (the 0x90 marker / panic-region)' : 'survived to before-call!';
          console.error('BISECT g70 RELIABLE: atClone(0x607C0)=' + nAtClone + ' afterCPclone(0x60780)=' + nAfterCP + ' afterReset(0x60784)=' + nAfterReset + ' afterSpan(0x60788)=' + nAfterSpan + ' beforeCall(0x60748)=' + nBeforeCall + ' → CORRUPTOR = ' + culprit); }
        // [az-diag g65→g70 free-band] stack new_tree (0x60748) vs HEAP cache.tree (0x6074C) right before sizing call.
        { const nStack = mini.AzStartup_peekU32(0x60748) >>> 0, nHeap = mini.AzStartup_peekU32(0x6074C) >>> 0;
          console.error('BISECT g70 before-call: stack new_tree(0x60748)=' + nStack + ' HEAP cache.tree(0x6074C)=' + nHeap
            + (nHeap===2 && nStack!==2 ? '  ← ✓✓✓ PATH B WORKS: heap survives, stack corrupted → do the full cache.tree refactor' : (nHeap===2 && nStack===2 ? '  ← both OK here (corruption is later/inside sizing)' : '  ← heap ALSO corrupted ('+nHeap+') → wild store reaches heap; path B won\'t help'))); }
    } catch (e) {
        { const fc = mini.AzStartup_peekU32(0x40790) >>> 0; if (fc) console.error('FUEL-CAP scan_cursor(0x40790)=0x' + fc.toString(16) + ' (set, then trapped downstream)'); }
        console.error('solveLayoutReal TRAPPED: ' + e.message);
        console.error('STACK:\n' + e.stack);
        if (typeof mini.AzStartup_peekU32 === 'function') {
            // [font-precheck] parse-check that runs BEFORE with_memory_fonts (which traps).
            const fpt = mini.AzStartup_peekU32(0x40670) >>> 0;
            if (fpt) console.error('[font-precheck] tag=0x' + fpt.toString(16)
                + (fpt === 0x600D0001 ? ' PARSE OK' : fpt === 0x600DDEAD ? ' PARSE FAILED' : '')
                + ' upem=' + (mini.AzStartup_peekU32(0x40674)>>>0)
                + ' ascender=' + (mini.AzStartup_peekU32(0x40678)|0)
                + ' descender=' + (mini.AzStartup_peekU32(0x4067C)|0));
            const epcL = mini.AzStartup_peekU32(0x400F0), epcH = mini.AzStartup_peekU32(0x400F4);
            if (epcL || epcH) console.error('POST-TRAP: __remill_error faulting guest PC = 0x' + (epcH>>>0).toString(16) + (epcL>>>0).toString(16).padStart(8,'0') + ' → grep server.log for this addr → otool the fn for the unlifted op');
            const mbL = mini.AzStartup_peekU32(0x400F8), mbH = mini.AzStartup_peekU32(0x400FC);
            if (mbL || mbH) console.error('POST-TRAP: __remill_MISSING_BLOCK guest PC = 0x' + (mbH>>>0).toString(16) + (mbL>>>0).toString(16).padStart(8,'0') + ' → an unresolved computed-branch / jump-table target');
            // RING DUMP (2026-06-02): is dealloc the SOLE missing_block, or one of many?
            const D = a => mini.AzStartup_peekU32(a) >>> 0;
            console.error('POST-TRAP counters: missing_block=' + D(262396) + ' err=' + D(262388) + ' fn_call=' + D(262476) + ' weak-dispatch=' + D(262492));
            // BumpAlloc helper records last alloc: size@0x40030, count@0x40038, retptr@0x40040, live bump-ptr@0x400 (pre-existing helper markers — show the garbage size on a memory.fill OOB).
            console.error('POST-TRAP bump: lastAllocSize(0x40030)=0x' + D(0x40030).toString(16) + ' allocCallCount(0x40038)=' + D(0x40038) + ' lastRetPtr(0x40040)=0x' + D(0x40040).toString(16) + ' liveBumpPtr(0x400)=0x' + D(0x400).toString(16));
            const mbring = []; for (let i = 0; i < 16; i++) { const v = D(262496 + i*4); if (v) mbring.push('0x' + v.toString(16)); }
            if (mbring.length) console.error('POST-TRAP missing_block ring: ' + mbring.join(' '));
            // [az-diag REVERT g111] built InlineContent disc-in-memory check (mark 0x607BC should = c0de0111).
            console.error('POST-TRAP cli-disc: out.len(0x607A0)=0x' + D(0x607A0).toString(16) + ' out.ptr(0x607A4)=0x' + D(0x607A4).toString(16) + ' out[0].disc(0x607A8)=0x' + D(0x607A8).toString(16) + ' discHi(0x607B8)=0x' + D(0x607B8).toString(16) + ' sizeof_IC(0x607AC)=0x' + D(0x607AC).toString(16) + ' mark(0x607BC)=0x' + D(0x607BC).toString(16));
            const erring = []; for (let i = 0; i < 16; i++) { const v = D(262560 + i*4); if (v) erring.push('0x' + v.toString(16)); }
            if (erring.length) console.error('POST-TRAP error/fn-call ring: ' + erring.join(' '));
            // [az-diag NO-REBUILD] dump the collected InlineContent (104 B) at cli-disc out.ptr to
            // find StyledRun.text String {ptr,cap,len}: if String.len is huge (~0x600xxxx) the runaway
            // is calculate_id(&content) hashing a corrupt-len String (text-extract mis-lift, NOT a
            // Vec-len mis-lift in measure). String should be at +8: ptr@+8, cap@+16, len@+24.
            // [g115] Stage-1 logical_items len: li.len after create_logical_items vs Arc<Vec> len.
            // If li_local.len huge → create_logical_items out-param bug; if li=1 but logical_items huge
            // → Arc::new len mis-lift; if both=1 and no trap → cache-chain was the bug (FIXED).
            console.error('POST-TRAP g115 Stage-1: li_local.len(0x607C0)=0x' + D(0x607C0).toString(16) + ' logical_items.len(0x607C4)=0x' + D(0x607C4).toString(16) + ' logical_items.ptr(0x607C8)=0x' + D(0x607C8).toString(16) + ' mark(0x607CC)=0x' + D(0x607CC).toString(16) + ' [STALE if cli traps]');
            // [g116] content.len/as_ptr INSIDE create_logical_items (written BEFORE the OOB clone, so VALID).
            const clen = D(0x607D0), cptr = D(0x607D4);
            console.error('POST-TRAP g116 create_logical_items ENTRY: content.len(0x607D0)=0x' + clen.toString(16) + (clen===1?' ✓len=1':' ✗MIS-LIFT len!=1 → boundary len corrupt') + ' content.as_ptr(0x607D4)=0x' + cptr.toString(16) + ' mark(0x607D8)=0x' + D(0x607D8).toString(16));
            // [g118] Stage-2 visual_items + Stage-3 shaped_items (cache-bypassed). marks confirm how far we got.
            console.error('POST-TRAP g118 Stage-2 visual_items: len(0x607E0)=0x' + D(0x607E0).toString(16) + ' ptr(0x607E4)=0x' + D(0x607E4).toString(16) + ' mark(0x607E8)=0x' + D(0x607E8).toString(16) + (D(0x607E8)===0xc0de0444?' ✓Stage-2 DONE':' ✗Stage-2 not reached'));
            console.error('POST-TRAP g118 Stage-3 shaped_items: len(0x607EC)=0x' + D(0x607EC).toString(16) + ' mark(0x607F0)=0x' + D(0x607F0).toString(16) + (D(0x607F0)===0xc0de0555?' ✓Stage-3 DONE (text SHAPED!)':' ✗Stage-3 not reached'));
            // [g125] shape_text_correctly: stc-entered(0x60864), font.shape_text RETURNED glyph count (0x60868, high bit set if returned).
            const gx = D(0x60868);
            console.error('POST-TRAP g125 shape_text_correctly: stc-entered(0x60864)=0x' + D(0x60864).toString(16) + ' | font.shape_text(0x60868)=' + ((gx&0x80000000)?('RETURNED '+(gx&0x7fffffff)+' glyphs'+((gx&0x7fffffff)>1000?' ✗HUGE→glyph-buffer len MIS-LIFT':' ✓reasonable→grouping/jump-table')):('NOT returned (trap inside allsorts)')));
            // [g129] collect_and_measure tuple-sret. STORE-side at the 3 return paths: 6639(0x608D0), 6810(0x608E0), 7196(0x608B0). READ-side in layout_ifc (0x608BC).
            { const p7196L=D(0x608B0),p7196M=D(0x608B8), p6639L=D(0x608D0),p6639P=D(0x608D4),p6639M=D(0x608D8), p6810L=D(0x608E0),p6810P=D(0x608E4),p6810M=D(0x608E8), rLen=D(0x608BC), rPtr=D(0x608C0), rMk=D(0x608C4);
              let store=null;
              if(p6639M===0xc0deaaa2) store={path:'6639 children-done',len:p6639L,ptr:p6639P};
              else if(p6810M===0xc0deaaa3) store={path:'6810 text-node-root',len:p6810L,ptr:p6810P};
              else if(p7196M===0xc0de0aaa) store={path:'7196 final',len:p7196L,ptr:0};
              if(p6810M===0xc0deaaa3) {} // already handled
              if(D(0x60908)===0xc0deaaa0) store={path:'6419 no-DOM-ID fallback',len:D(0x60904),ptr:0};
              const ent=D(0x608FC), ifcIdx=D(0x60900), okerr=D(0x608F0), okLen=D(0x608F4), okPtr=D(0x608F8), slot=D(0x608C8);
              console.error('POST-TRAP g129 ENTRY: _impl entered='+(ent===0xc0de0e47?'YES ifc_root_index='+ifcIdx:'NO (not reached!)'));
              console.error('POST-TRAP g129 WRAPPER: _impl returned '+(okerr===0xc0de0cc1?('OK len='+okLen+' ptr=0x'+(okPtr>>>0).toString(16)):okerr===0xc0de0ee1?'*** ERR *** (but caller took Ok branch → DISCRIMINANT MIS-READ!)':'(marker not set)'));
              { const rw0=D(0x60910), rw2=D(0x60914), rslot=D(0x60918);
                console.error('POST-TRAP g129b RAW Result slot(&result_raw=0x'+(rslot>>>0).toString(16)+'): word0/Vec.ptr=0x'+(rw0>>>0).toString(16)+' word2/Vec.len=0x'+(rw2>>>0).toString(16)
                  +' → '+(rw0===0?'word0=0 = _impl WROTE the Err niche HERE, but `?` took Ok ⇒ NICHE-READ MIS-LIFT':'word0!=0 (0x'+(rw0>>>0).toString(16)+') = STALE/garbage ⇒ _impl wrote ELSEWHERE (SLOT MISMATCH)')); }
              console.error('POST-TRAP g129 STORE-side path: '+(store?('path='+store.path+' content.len='+store.len+' ptr=0x'+(store.ptr>>>0).toString(16)):'NO Ok-path marker fired'));
              console.error('POST-TRAP g129 READ-side (layout_ifc): inline_content.len(0x608BC)='+rLen+' ptr(0x608C0)=0x'+rPtr.toString(16)+' slot&inline_content(0x608C8)=0x'+(slot>>>0).toString(16)+(rMk===0xc0de0bbb?' ✓fired':' ✗NOT-fired')); }
            // [g126] shape_text_internal's OWN glyphs.len at its Ok return (0x60700; 0xE0000000=entered-not-returned).
            const sti = D(0x60700);
            console.error('POST-TRAP g126 shape_text_internal: count(0x60700)=' + (sti===0xe0000000?'0xE0000000 (entered, did NOT reach Ok)':('0x'+sti.toString(16)+' ('+sti+')')) + ((sti!==0xe0000000 && sti<1000 && (gx&0x7fffffff)>1000)?' ★ internal OK but caller HUGE → RETURN-CHAIN sret len mis-lift (out-param the chain)':(sti>1000?' ★ internal ALSO huge → bug INSIDE shape_text_internal':'')));
            const icp = D(0x607A4);
            if (icp) {
                const w = []; for (let i = 0; i < 26; i++) w.push(D(icp + i*4) >>> 0);
                console.error('POST-TRAP InlineContent@0x' + icp.toString(16) + ' words=[' + w.map(x=>'0x'+x.toString(16)).join(',') + ']');
                console.error('POST-TRAP   StyledRun.text @+8: ptr=0x' + w[2].toString(16) + ' cap=0x' + w[4].toString(16) + ' len=0x' + w[6].toString(16) + (w[6]===5?' ✓len=5':' ✗CORRUPT-LEN → calculate_id hashes garbage'));
            }
            // AZ_TAG_UNREACHABLE: id of the firing unreachable (store 0x554e0000|id → 0x40050)
            const ut = D(0x40050); if ((ut >>> 16) === 0x554e) console.error('POST-TRAP unreachable-tag id=' + (ut & 0xffff) + ' → Nth unreachable in <stem>.untag.ll');
            // [az-diag REVERT] shaping probe @0x40700: 0=not reached, 0xE0000000=panicked INSIDE shaping, <0x1000=completed w/ that glyph count
            const shp = D(0x40700);
            console.error('POST-TRAP shaping probe(0x40700)=0x' + shp.toString(16) + (shp===0?' (shape_text_internal NOT reached)':shp===0xe0000000?' (shaping ENTERED but panicked inside)':' (shaping COMPLETED, glyphs='+shp+')'));
            // [az-diag REVERT] shaping PHASE @0x407A0: 1=raw-walk-start 2=raw-done 3=gsub-done 4=gpos-done 5=forinfo-done; CAP flag @0x407A4: 0x5CA90002=char-walk loop stalled, 0x5CA90003=for-info loop stalled
            { const sph = D(0x407A0), scap = D(0x407A4);
              const sphn = {0:'shaping-inner NOT reached',1:'raw-walk STARTED (hung/trapped IN char-walk)',2:'raw-walk DONE (next: gsub::apply)',3:'gsub DONE (next: gpos::apply)',4:'gpos DONE (next: for-info)',5:'for-info DONE (shaping complete)'}[sph] || ('0x'+sph.toString(16));
              console.error('POST-TRAP shaping PHASE(0x407A0)=' + sph + ' → ' + sphn + (scap?(' | FUEL-CAP(0x407A4)=0x'+scap.toString(16)+(scap===0x5ca90002?' ✓✓ CHAR-WALK was the infinite loop (UTF-8 len advance mis-lifts to 0)':scap===0x5ca90003?' ✓✓ FOR-INFO was the infinite loop':'')):' (no cap fired)')); }
            // [az-diag REVERT] from_provider font-parse PHASE @0x407B0: 1=pre-HEAD 2=HEAD 3=MAXP 4=HHEA 5=pdf_metrics 6=Font::new 7=cmap 8=built
            { const fp = D(0x407B0);
              const fpn = {0:'from_provider NOT reached',1:'pre-HEAD (OOB in HEAD table_data/read)',2:'HEAD done (OOB in MAXP)',3:'MAXP done (OOB in HMTX/VMTX/VHEA/HHEA)',4:'HHEA done (OOB in pdf_metrics)',5:'pdf_metrics done (OOB in allsorts Font::new)',6:'Font::new done (OOB in GSUB/GPOS/gdef/kern/CMAP read)',7:'cmap done (OOB in hash/struct-build/space-predecode)',8:'BUILT+returning (font parse OK!)'}[fp] || ('0x'+fp.toString(16));
              console.error('POST-TRAP from_provider PHASE(0x407B0)=' + fp + ' → ' + fpn); }
            // [az-diag REVERT] layout-fn localization @0x40704: 0x10=layout_bfc, 0x20=layout_ifc, 0x30=translate_to_text3_constraints (last entered)
            const lf = D(0x40704);
            const lfn = {0x80:'ldr ENTRY',0x81:'IN collect_and_resolve',0x82:'collect_and_resolve done → panic in last-resort/probe/load-setup',0x83:'IN load_missing_for_chains',0x84:'load_missing done → panic in set_font_chain_cache/after',0x85:'font block done → panic before calc_intrinsic_sizes (reconcile/cache-remap)',0x90:'IN calculate_intrinsic_sizes → recursing (panic before ifc_root entry)',0x91:'calc_intrinsic done → panic in layout loop',0xA0:'IN collect_inline_content (calculate_ifc_root_intrinsic_sizes)',0xA1:'collect_inline_content done → panic IN measure_intrinsic_widths (logical/BiDi/shape)',0xA2:'measure_intrinsic_widths done → panic after',0x10:'layout_bfc',0x20:'layout_ifc',0x30:'translate_to_text3_constraints'}[lf] || (lf===0?'(none reached)':'0x'+lf.toString(16));
            console.error('POST-TRAP layout-fn(0x40704)=0x' + lf.toString(16) + ' → last entered: ' + lfn);
            // [az-diag REVERT] text-extract probe @0x4071C: 0xB1=about to extract_text_from_node, 0xB2=extracted (0x40714=text.len, expect 5), 0xB3=split done
            const tx = D(0x4071C);
            const txn = {0xb1:'about-to-extract; B5 not set → as_str() built but to_string() OOB (AzString len garbage)',0xb5:'AzString ptr/len CAPTURED → to_string() then OOB',0xb2:'extracted text → panic IN split_text_for_whitespace',0xb3:'split done'}[tx] || '0x'+tx.toString(16);
            console.error('POST-TRAP text-extract probe(0x4071C)=0x' + tx.toString(16) + ' → ' + txn + (tx>=0xb5?(' | AzString RAW: len(0x40714)='+D(0x40714)+' (expect 5) lenHi(0x40724)=0x'+D(0x40724).toString(16)+' ptr(0x40718)=0x'+D(0x40718).toString(16)+' (expect bump ~0x6xxxxxx) cap(0x40720)='+D(0x40720)):''));
            // [az-diag REVERT] cb-return probe @0x40760: did the cb run IN WASM (layout markers work?)
            // + what AzString did AzString_copyFromBytes RETURN (before createText)?
            const cbRan = D(0x40760);
            console.error('POST-TRAP cb-return probe: marker(0x40760)=0x' + cbRan.toString(16)
              + (cbRan === 0xc0de
                  ? (' → ✓ cb RAN IN WASM + layout markers WORK | returned AzString.ptr(0x40764)=0x' + D(0x40764).toString(16)
                     + ' len(0x40768)=' + D(0x40768) + ' (expect ptr~0x6xxxxxx, len=5) → '
                     + (D(0x40768) === 5 && D(0x40764) > 0x6000000 ? 'copy_from_bytes RETURNED CORRECT → createText/handoff corrupts'
                        : 'copy_from_bytes RETURNED GARBAGE → the body/sret is the bug'))
                  : ' → ✗ cb marker NOT set: either layout-wasm markers DONT reach mini-memory (probe is a false signal) OR the cb never ran in wasm'));
            // [az-diag REVERT] extern AzString_copyFromBytes probe @0x40750: len(X2) BEFORE the shift
            const extRan = D(0x40750);
            console.error('POST-TRAP extern-copyFromBytes probe: marker(0x40750)=0x' + extRan.toString(16)
              + (extRan === 0xe0e0
                  ? (' → REACHED | len(0x40754)=' + D(0x40754) + ' (expect 5) ptr(0x40758)=0x' + D(0x40758).toString(16)
                     + ' start(0x4075C)=' + D(0x4075C) + ' → '
                     + (D(0x40754) === 5 ? 'cb PASSED len=5 CORRECTLY → loss is THREADING (wrapper-shift/dispatcher/relocator, X2→X3)'
                        : 'cb passed len=' + D(0x40754) + ' (NOT 5) → the CB-SETUP lift lost it before the extern'))
                  : ' → NOT reached (extern not called in wasm?)'));
            // [az-diag REVERT] dispatcher-entry X1 probe @0x40580 (splits extern→dispatcher vs dispatcher→body)
            const dispX1Sent = D(0x40584);
            console.error('POST-TRAP dispatcher-entry X1 probe: sentinel(0x40584)=0x' + dispX1Sent.toString(16)
              + (dispX1Sent === 0xd05d
                  ? (' → FIRED | X1@dispatcher(0x40580)=0x' + D(0x40580).toString(16)
                     + (D(0x40580) === 0x13f80 ? ' = 0x13f80 ✓ → X1 PRESERVED to the dispatcher → the loss is dispatcher→body (the `call @sub_` of the dep loses %state)'
                        : ' (NOT 0x13f80) → X1 already wrong at the dispatcher'))
                  : ' → NOT fired (no dispatch had X1 in [0x10000,0x1000000) → X1 was already HIGH before the dispatcher = extern→dispatcher/tail-call lost it)'));
            // [az-diag REVERT] copy_from_bytes ptr-band-GATED probe @0x40728..0x40748
            const cfbHit = D(0x40748) === 0xabcd;
            console.error('POST-TRAP copy_from_bytes ptr-band probe: ' + (cfbHit
              ? ('REACHED → INPUT len(0x40740)='+D(0x40740)+' (expect 5) ptr(0x40744)=0x'+D(0x40744).toString(16)+' (expect 0x13f80; if 0 → literal addr mis-computed) | U8Vec ptr(0x40728)=0x'+D(0x40728).toString(16)+' len(0x4072C)='+D(0x4072C)+' (expect heap+5) | String ptr(0x40730)=0x'+D(0x40730).toString(16)+' len(0x40734)='+D(0x40734)+' | RESULT ptr(0x40738)=0x'+D(0x40738).toString(16)+' len(0x4073C)='+D(0x4073C)+' → FIRST garbage field = the mis-lift step')
              : 'NOT reached → len arg lost (len!=5 in wasm) OR copy_from_bytes never ran'));
            // [az-diag REVERT] AzStartup_initLayoutCache static read-back (WASM-ONLY-safe)
            const rb = D(0x4074C), rbAddr = D(0x40750);
            console.error('POST-TRAP static read-back: AZ_WASM_CB_ACTIVE after store(0x4074C)=' + rb
              + (rb === 1 ? ' → ✓ store+load CONNECT within mini (so the gate failing earlier = cross-MODULE mini↔cb-wasm)' : rb === 0 ? ' → ✗ store does NOT persist even within mini (store mis-lift)' : ' (unexpected)')
              + ' | &static(0x40750)=0x' + rbAddr.toString(16));
            // [az-diag REVERT] chain probe @0x406B0: tag, nchains(B4), total_fonts(B8), nreg(BC)
            console.error('POST-TRAP chain probe: tag(0x406B0)=0x' + D(0x406B0).toString(16) + ' nchains(0x406B4)=' + D(0x406B4) + ' total_fonts(0x406B8)=' + D(0x406B8) + ' nreg(0x406BC)=' + D(0x406BC));
            const lr406 = D(0x406C0); if (lr406) console.error('POST-TRAP last-resort-loop(0x406C0)=0x' + lr406.toString(16) + ' → ' + ({0xc0de0001:'pre-loop (panic in chains.values_mut() iter setup)',0xc0de0002:'in-body, computing total (panic = chain.css_fallbacks.iter mis-lift)',0xc0de0003:'total computed OK (panic in if-branch)',0xc0de0004:'total==0, about to fc_cache.list() (panic IN list())',0xc0de0005:'list.first()=Some, about to clone+push (panic = pattern.unicode_ranges.clone() mis-lift)',0xc0de0006:'push OK',0xc0de0007:'loop fully done (panic in nchains/total_fonts/nreg sum block window.rs:947-953)'}[lr406] || 'unknown'));
            // [az-diag REVERT] solveLayoutReal step @0x40710: 0=before/in DIAG-probe, 0x50=DIAG done, 0x60=LayoutWindow::new done, 0x70=about to layout_dom_recursive
            const sl = D(0x40710);
            const sln = {0x50:'DIAG-probe done → panic in LayoutWindow::new',0x60:'LayoutWindow::new done → panic before layout_dom_recursive call',0x70:'panic INSIDE layout_dom_recursive (pre-bfc)'}[sl] || (sl===0?'panic in/before the font-resolve DIAG probe':'0x'+sl.toString(16));
            console.error('POST-TRAP solveLayoutReal step(0x40710)=0x' + sl.toString(16) + ' → ' + sln);
            // DIAG font-resolve probe (0x40690 tag): did it complete? resolve_char('H') Some?
            console.error('POST-TRAP font-resolve DIAG: tag(0x40690)=0x' + D(0x40690).toString(16) + ' resolve_char_H(0x4069C)=' + D(0x4069C) + ' nfonts(0x406A4)=' + D(0x406A4) + ' nlist(0x406A8)=' + D(0x406A8));
        }
        process.exit(1);
    }
    // DIAG (multi-node sizing 2026-06-02): always dump remill error/missing_block counters
    // (transpiler records them @262384-262496). __remill_error count>0 => an unlifted ARM
    // instr fired → early ret → tree not built → InvalidTree. PCs → otool to ID the opcode.
    if (typeof mini.AzStartup_peekU32 === 'function') {
        const D = a => mini.AzStartup_peekU32(a) >>> 0;
        console.log('[diag] rc=' + rc + ' | __remill_error count=' + D(262388) + ' lastPC=0x' + D(262384).toString(16)
            + ' | missing_block count=' + D(262396) + ' lastPC=0x' + D(262392).toString(16) + ' | weak-dispatch=' + D(262492));
        const ring = []; for (let i = 0; i < 16; i++) { const v = D(262496 + i*4); if (v) ring.push('0x' + v.toString(16)); }
        if (ring.length) console.log('[diag] missing_block ring: ' + ring.join(' '));
        const ering = []; for (let i = 0; i < 16; i++) { const v = D(262560 + i*4); if (v) ering.push('0x' + v.toString(16)); }
        if (ering.length) console.log('[diag] __remill_error ring: ' + ering.join(' '));
        // tree-build (children-None) markers
        const cnc = D(262400); if (cnc) console.log('[diag] create_node_from_dom calls=' + cnc + (cnc === 5 ? ' (all 5 ✓)' : ' (≠5 — tree incomplete!)'));
        const cnr = []; for (let i = 0; i < 14; i++) { const v = D(262404 + i*4); if ((v >>> 24) === 0xDD) cnr.push('n' + (v & 0xffff)); }
        if (cnr.length) console.log('[diag] layout-node dom_ids: ' + cnr.join(' '));
        const cc = []; for (let i = 0; i < 8; i++) { const v = D(262464 + i*4); const t = v >>> 24; if (t === 0xC0) cc.push('p' + i + '=NO-first-child'); else if (t === 0xCC) cc.push('p' + i + '=' + (v & 0xffff) + 'ch'); }
        if (cc.length) console.log('[diag] collect_children: ' + cc.join(' '));
        // compact-cache dump (CSS→taffy): SENTINEL 0xffffffff = unset
        const comp = D(263544);
        if (comp === (0xC000ABED >>> 0)) console.log('[diag] compact_cache = NONE (not built in lift!)');
        else if (comp === (0xCC5E0000 >>> 0)) {
            const cd = [];
            for (let i = 0; i < 5; i++) { const b = 263552 + i*16; const w = D(b), h = D(b+4), disp = D(b+8); if (w||h||disp) cd.push('n'+i+' w=0x'+w.toString(16)+' h=0x'+h.toString(16)+' disp='+(disp&0xff)); }
            console.log('[diag] compact_cache present: ' + cd.join(' | '));
        }
        // [g119 SUCCESS-PATH measure markers] g116 cli-entry (0x607D0), g118 Stage-2 (0x607E8=0xc0de0444),
        // Stage-3/shape (0x607F0=0xc0de0555). Tells us how far measure_intrinsic_widths got (no trap → these
        // aren't read in POST-TRAP). NOTE 0x607C0/C4 collide w/ old g70 markers; 0x607D0+ are clean.
        const Dh = a => mini.AzStartup_peekU32(a) >>> 0;
        console.log('[g119 measure-path] cli-entry content.len(0x607D0)=0x' + Dh(0x607D0).toString(16) + ' | Stage-2 mark(0x607E8)=0x' + Dh(0x607E8).toString(16) + (Dh(0x607E8)===0xc0de0444?' ✓visual built':'') + ' visual.len(0x607E0)=0x' + Dh(0x607E0).toString(16) + ' | Stage-3 mark(0x607F0)=0x' + Dh(0x607F0).toString(16) + (Dh(0x607F0)===0xc0de0555?' ✓✓ SHAPED!':' ✗ shape not done') + ' shaped.len(0x607EC)=0x' + Dh(0x607EC).toString(16));
        // [g132 VERIFY] layout_ifc output.overflow_size (IFC line-box bounds set from main_frag.bounds()).
        // height>0 proves the text LAID OUT (positioned into a line box), not just shaped.
        { const f32 = b => new Float32Array(new Uint32Array([b>>>0]).buffer)[0];
          const ow = Dh(0x60670), oh = Dh(0x60674), np = Dh(0x60678), mk = Dh(0x6067C);
          console.log('[g132 lays-out] overflow_size = ' + f32(ow).toFixed(2) + ' x ' + f32(oh).toFixed(2)
            + ' | positions.len=' + np + ' | mark=0x' + mk.toString(16)
            + (mk===0xc0de0132 ? (f32(oh) > 0 ? '  ✓✓✓ TEXT LAYS OUT (h>0)' : '  ✗ h=0 (shaped but line-box bounds zero)') : '  ✗ layout_ifc Phase-4 not reached')); }
        // [g133] WHERE does positioning's layout_ifc return early? collect result + len (0x60680/84), layout_flow arm (0x60688).
        { const cl = Dh(0x60680), cm = Dh(0x60684), lf = Dh(0x60688), le = Dh(0x6068C);
          console.log('[g133 early-return] collect inline_content.len(0x60680)=' + cl
            + ' | collect result(0x60684)=' + (cm===0xc0de0680?'Ok ✓':cm===0xee?'*** Err *** → layout_ifc returns Err here':'0x'+cm.toString(16)+' (not reached)')
            + ' | layout_flow(0x60688)=' + (lf===0xc0de0688?'Ok ✓':lf===0xee?('*** Err *** (errword=0x'+le.toString(16)+') → zero-sized, text NOT positioned'):'0x'+lf.toString(16)+' (not reached)')); }
        // [g134] out-param pointer match + did _impl complete? entry(0x60690)=impl content ptr, 0x60694=entry mark|ifc_idx,
        // 0x60698=_impl's content.len at final return, 0x6069C=reached-final-return, 0x606A0=caller inline_content ptr.
        { const ip = Dh(0x60690), em = Dh(0x60694), il = Dh(0x60698), fr = Dh(0x6069C), cp = Dh(0x606A0);
          console.log('[g134 outparam] _impl content.ptr(0x60690)=0x' + ip.toString(16) + ' | caller content.ptr(0x606A0)=0x' + cp.toString(16)
            + (ip===cp && ip!==0 ? ' ✓SAME Vec' : ' ✗DIFFERENT → &mut out-param ptr MIS-LIFTS (stack-addr class)')
            + ' | _impl entry(0x60694)=0x' + em.toString(16) + ' (ifc_root_index=' + (em & 0xffff) + ')'
            + ' | _impl final-return(0x6069C)=' + (fr===0xc0de069c?('REACHED, _impl content.len='+il+(il>0?' → data mis-lifts to caller (caller sees 0)':' → _impl ALSO collected 0')):'NOT reached → real early ? Err in _impl')); }
        // [g135] which tree.get fails (6449 vs 6706) + tree validity at _impl entry.
        { const seq = Dh(0x606A4), nl = Dh(0x606A8), gs = Dh(0x606AC), rt = Dh(0x606B0);
          console.log('[g135 tree.get] _impl tree.nodes.len(0x606A8)=' + nl + ' | tree.root(0x606B0)=' + rt
            + ' | tree.get(idx).is_some(0x606AC)=' + (gs===0xc0de0001?'TRUE ✓':gs===0xc0de0000?'FALSE ✗ (tree.get mis-lifts despite valid tree!)':'0x'+gs.toString(16))
            + ' | last-passed-? (0x606A4)=' + (seq===0x6706?'past 6706 → Err is AFTER (in DOM loop)':seq===0x6449?'past 6449, Err AT 6706 tree.get':seq===0?'Err AT 6449 tree.get (first one)':'0x'+seq.toString(16))); }
        // [g136] DOM loop: dom_children.len(0x606B4), first-child node_type(0x606B8), text-branch content.len(0x606BC), seq(0x606A4).
        { const dc = Dh(0x606B4), nt = Dh(0x606B8), tl = Dh(0x606BC), sq = Dh(0x606A4);
          const ntName = nt===0xc0de7e70?'Text ✓':nt===0xc0ded11f?'Div':nt===0xc0deb0d1?'Body':nt===0xc0de0000?'Other/UNRECOGNIZED':('0x'+nt.toString(16)+' (not set)');
          const seqName = sq===0x6905?'TEXT branch (pushed, content.len='+tl+')':sq===0x6942?'NON-TEXT branch → ? Err (calc_used_size/layout_fc)':sq===0x6896?'loop entered but text SKIPPED (mis-classified + no layout node → continue)':sq===0x6863?'reached dom_children, loop NOT entered (empty?)':'0x'+sq.toString(16);
          console.log('[g136 DOMloop] dom_children.len(0x606B4)=' + dc + (dc===0?' ✗ az_children EMPTY (no text collected!)':' ✓') + ' | first-child node_type(0x606B8)=' + ntName + ' | last-seq=' + seqName); }
        // [g121] shape_visual_items font path: which font_stack arm + chain-cache get result.
        const arm = Dh(0x60820), getr = Dh(0x60828);
        console.log('[g121 shape-path] font_stack arm(0x60820)=' + (arm===1?'Ref':arm===2?'Stack':'0x'+arm.toString(16)) + ' | chain_cache.len(0x60824)=0x' + Dh(0x60824).toString(16) + ' | chain.get(0x60828)=' + (getr===1?'Some ✓':getr===0xEE?'None ✗SKIP (key mismatch → no shape)':'0x'+getr.toString(16)) + ' | reached-fallback(0x6082C)=0x' + Dh(0x6082C).toString(16));
        const cpath = Dh(0x60830);
        console.log('[g122 chain-resolve] path(0x60830)=' + (cpath===1?'get/Ord ✓':cpath===2?'find/Eq (Ord BROKEN)':cpath===3?'only-chain (Ord+Eq BROKEN)':cpath===0xEE?'NONE (no chains)':'0x'+cpath.toString(16)));
        // [g123] shape_with_font_fallback fast path → shape_text_correctly → font.shape_text (the allsorts shaper).
        const seg = Dh(0x60850), gl = Dh(0x60868);
        console.log('[g123 shaper] segments(0x60850)=0x' + seg.toString(16) + ' | first(0x60854)=' + (Dh(0x60854)===1?'Some':Dh(0x60854)===0xEE?'None(0 segs)':'0x'+Dh(0x60854).toString(16)) + ' | loaded_fonts.get(0x60858)=' + (Dh(0x60858)===1?'Some':Dh(0x60858)===0xEE?'MISS':'0x'+Dh(0x60858).toString(16)) + ' | reached-stc(0x60860)=0x' + Dh(0x60860).toString(16) + ' | stc-entered(0x60864)=0x' + Dh(0x60864).toString(16) + ' | font.shape_text(0x60868)=' + ((gl&0x80000000)?('RETURNED '+(gl&0x7fffffff)+' glyphs'):('✗NOT RETURNED (unlifted shaper / __remill_error)')));
    }
    if (typeof mini.AzStartup_peekU32 === 'function') {
        const cl = mini.AzStartup_peekU32(0x40098);
        if ((cl >>> 16) === 0x4c52)
            console.log('[2] layout_dom_recursive claimed is_err=' + (cl & 1) + ' (rc=' + rc + ')');
        // RELIABLE LayoutError tag: raw bytes (no lifted match). byte0 likely the tag.
        const rb = new Uint8Array(memory.buffer, 0x40120, 12);
        const tagNames = {0:'InvalidTree',1:'SizingFailed',2:'PositioningFailed',3:'DisplayListFailed',4:'Text'};
        console.log('[2] LayoutError raw bytes = [' + Array.from(rb).map(x=>x.toString(16).padStart(2,'0')).join(' ') + '] → byte0=' + rb[0] + ' (' + (tagNames[rb[0]]||'?') + ')');
        // [font] direct parse-check (written by solveLayoutReal's font setup, eventloop.rs).
        { const D = a => mini.AzStartup_peekU32(a) >>> 0; const S = a => (mini.AzStartup_peekU32(a) | 0);
          const txt = () => { let s=''; for (const a of [0x40658,0x4065c,0x40660]){ const v=D(a); for(let k=0;k<4;k++){ const c=(v>>(8*k))&0xff; if(c) s+=String.fromCharCode(c); } } return s; };
          const ft = D(0x40650);
          console.log('[font] parse(0x40650)=0x' + ft.toString(16)
            + (ft === 0xf0510001 ? (' → PARSE OK: upem=' + D(0x40654) + ' ascender=' + S(0x40658) + ' descender=' + S(0x4065c) + (S(0x40658) === 0 ? '  ← ASCENDER=0 → metric-table read mis-lifts (B2)' : '  ← metrics NONZERO → parse fine → text-0 is the CHAIN (B1)'))
            : ft === 0xf051dead ? (' → from_bytes None (PARSE FAILED): warns=' + D(0x40654) + ' msg="' + txt() + '" (len=0→no-warning ? step like HEAD; "Failed to read font data"→FontData read; "Failed to get table"→table_provider)')
            : ' → marker 0 (not reached)')); }
        // [font-resolve] VERIFY-ROOT-CAUSE: did "serif" resolve to a font? (eventloop probe)
        { const D = a => mini.AzStartup_peekU32(a) >>> 0;
          const rt = D(0x40690);
          if (rt === 0x5E5E0001) {
            const css = D(0x40694), uni = D(0x40698), hres = D(0x4069C), fid = D(0x406A0);
            const nf = D(0x406A4), nlist = D(0x406A8), nameLen = D(0x406AC);
            console.log('[font-resolve] serif chain: css_fallback_fonts=' + css + ' unicode_fallbacks=' + uni
              + ' | resolve_char(H)=' + (hres ? ('FONT 0x' + fid.toString(16) + ' ✓ → resolution OK, height-0 is SHAPING') : 'None ✗'));
            console.log('[font-resolve] fc_cache.len()=' + nf + ' | list().len()=' + nlist + ' | first.name.len=' + nameLen
              + (nf >= 1 && nlist === 0 ? ' → ✗ Vec ITERATION mis-lifts (len ok, iter empty)'
                 : nf >= 1 && nlist >= 1 && nameLen === 0 ? ' → ✗ Vec CONTENT garbage (iter ok, name empty)'
                 : nf >= 1 && nlist >= 1 ? ' → ✓ registration+iter+content OK → query_matches/find_unicode_fallbacks is the issue'
                 : ' → ✗ not registered'));
          } else console.log('[font-resolve] probe not reached (0x40690=0x' + rt.toString(16) + ')');
        }
        // [layout-chains] WASM-ONLY probe in window.rs (the layout's actual chains)
        { const D = a => mini.AzStartup_peekU32(a) >>> 0;
          const rt = D(0x406B0);
          if (rt === 0x5E5E0002) {
            const nc = D(0x406B4), tf = D(0x406B8), nr = D(0x406BC);
            console.log('[layout-chains] chains.chains.len()=' + nc + ' | Σfonts=' + tf + ' | fc_cache.list().len()=' + nr
              + (nc === 0 ? ' → ✗✗ NO CHAINS (collect_font_stacks returned no stack for the text node — last-resort cant add)'
                 : tf === 0 ? ' → ✗ chains exist but EMPTY + last-resort didnt add'
                 : ' → ✓ font IS in a chain → load_missing should load it (look downstream: load_fonts/shaping)'));
          } else console.log('[layout-chains] window.rs not reached (0x406B0=0x' + rt.toString(16) + ')');
        }
        // [collect-stacks] WASM-ONLY probe: did collect_font_stacks match the text node?
        { const D = a => mini.AzStartup_peekU32(a) >>> 0;
          const rt = D(0x406C0);
          if (rt === 0x5E5E0003) {
            const ncount = D(0x406C4), keys = D(0x406C8), rawtext = D(0x406CC);
            const b0 = D(0x406D0), b1 = D(0x406D4), b2 = D(0x406D8), b4 = D(0x406DC), n0b0 = D(0x406E0);
            const boxptr = D(0x406EC);
            const has177 = (b0===177||b1===177||b2===177||b4===177);
            console.log('[collect-stacks] node_count=' + ncount + ' | text-keys-matched=' + keys
              + ' | n1.node_type bytes[0,1,2,4]=[' + b0 + ',' + b1 + ',' + b2 + ',' + b4 + '] | n0[0]=' + n0b0
              + ' | n1.boxptr@8=0x' + boxptr.toString(16) + (boxptr !== 0 ? ' (box SURVIVED ✓)' : ' (box LOST ✗ — disc-restore insufficient)')
              + (keys >= 1 ? ' → ✓ TEXT MATCHED' : has177 ? ' → 177(Text) present at offset≠0 → FIELD-OFFSET mis-lift' : b0===0 ? ' → ✗ disc=0 DROPPED (NodeType::Text disc store dropped at build)' : ' → disc=' + b0 + ' unexpected')
              + (keys === 0 && rawtext >= 1 ? ' → ✗✗ matches!(NodeType::Text) FAILS though raw disc DIFFERS → ENUM-MATCH MIS-LIFT'
                 : keys === 0 ? ' → ✗ text node not matched (node_type == body disc, or no text node)'
                 : ' → ✓ text node matched → issue is Phase 2/downstream'));
          } else console.log('[collect-stacks] not reached (0x406C0=0x' + rt.toString(16) + ')');
          const dr = D(0x406F0);
          if ((dr >>> 16) === 0xD15C) console.log('[disc-restore] eventloop restored ' + (dr & 0xffff) + ' node_type disc(s) on styled_dom (0=box lost or none dropped)');
        }
        // [cb-dom] WASM-ONLY: the cb's Dom node_type discs BEFORE cascade (hydrate path)
        { const D = a => mini.AzStartup_peekU32(a) >>> 0;
          const rootDisc = D(0x406E4), childDisc = D(0x406E8);
          console.log('[cb-dom] cb-Dom root(body).disc=' + rootDisc + ' | first-child(text).disc=' + childDisc
            + (childDisc === 177 ? ' → child=177 ✓ createText OK → DROP IS IN THE CASCADE (StyledDom::create copy of data-variant)'
               : childDisc === 0 ? ' → child=0 ✗ → AzDom_createText DROPPED node_type (X8/sret build-lift)'
               : ' → child=' + childDisc + ' (root should be 2=Body)'));
        }
        const ph = mini.AzStartup_peekU32(0x401C0);
        if ((ph >>> 16) === 0xDD00) {
            const step = ph & 0xffff;
            const PH = {0x0001:'start',0x0002:'after reconcile_and_invalidate',0x0003:'after step3',0x0005:'loop pre-subtree',0x0053:'sub53',0x0054:'sub54',0x0056:'sub56',0x0055:'sub55',0x0057:'clr OK',0x005E:'clr ERR',0x0006:'after reposition_clean_subtrees',0x0061:'pre adjust_relative_positions',0x0062:'pre adjust_sticky_positions',0x0063:'pre position_out_of_flow',0x0065:'after display-list',0x0004:'final (positions written)'};
            const base = step & 0xff, node = (step >> 8) & 0xff;
            const ARP = {0x71:'adjust_relative ENTERED',0x72:'per-node PRE get_position_type',0x73:'per-node POST get_position_type',0x74:'adjust_relative COMPLETED (returned Ok)'};
            let phName;
            if (base === 0x64) phName = 'pre display-list check (SKIP_DISPLAY_LIST=' + ((step >> 8) & 1) + ')';
            else if (ARP[base]) phName = ARP[base] + ' (node ' + node + ')';
            else phName = PH[step] || '?';
            console.log('[2] LAST phase = 0x' + step.toString(16).padStart(4,'0') + ' (' + phName + ') → Err in the NEXT step');
            const pm = mini.AzStartup_peekU32(0x401C4);
            if ((pm >>> 16) === 0xDD22) {
                const POS = {0:'Static',1:'Relative',2:'Absolute',3:'Fixed',4:'Sticky'};
                console.log('[2] last get_position_type: node ' + ((pm >> 8) & 0xff) + ' → ' + (POS[pm & 0xff] || ('raw' + (pm & 0xff))) + ' (Static→should early-continue)');
            }
            // NEW probes: 0x401C8 = tree.nodes.len() captured BEFORE the loop; 0x401CC = loop fully exited.
            const arpLen = mini.AzStartup_peekU32(0x401C8);
            if ((arpLen >>> 16) === 0xDD33) console.log('[2] adjust_relative tree.nodes.len() = ' + (arpLen & 0xffff) + ' (len-read SUCCEEDED before loop)');
            else console.log('[2] adjust_relative tree.nodes.len() NOT captured (raw=0x' + arpLen.toString(16) + ') → for-loop header (Vec::len) is where it dies');
            const arpExit = mini.AzStartup_peekU32(0x401CC);
            console.log('[2] adjust_relative loop-exit marker (0x401CC) = 0x' + arpExit.toString(16) + (arpExit === 0xDD44AAAA ? ' (loop FULLY exited)' : ' (loop did NOT exit)'));
        }
        const tn = mini.AzStartup_peekU32(0x4009C);
        if ((tn >>> 24) === 0x54) {
            const nn = tn & 0xffffff;
            console.log('[2] layout_cache.tree node count = ' + (nn === 0xffffff ? 'None (tree NOT built)' : nn + (nn === 5 ? ' (built OK)' : ' (UNEXPECTED)')));
        }
        const tc = mini.AzStartup_peekU32(0x40140);
        if ((tc >>> 24) === 0x4e) {
            const cnt = Math.min(tc & 0xffffff, 6);
            const FC = {0:'Block',1:'Inline',2:'InlineBlock',3:'Flex',4:'Float',5:'OutOfFlow',6:'Table'};
            console.log('[2] PER-NODE tree state (used_size / FC / parent):');
            for (let i = 0; i < cnt; i++) {
                const b = 0x40150 + i*16;
                const w = mini.AzStartup_peekU32(b), h = mini.AzStartup_peekU32(b+4);
                const fc = mini.AzStartup_peekU32(b+8) & 0xff, par = mini.AzStartup_peekU32(b+12);
                const sz = (w === 0xffffffff) ? 'used_size=None' : ('used_size=' + w + 'x' + h);
                console.log('    node[' + i + '] ' + sz + '  FC=' + (FC[fc]||('?'+fc)) + '  parent=' + (par === 0xffffffff ? 'root' : par));
            }
        }
        // The bytes look like [ptr, _, len] of a String error message. Follow the
        // ptr candidates and dump ASCII to read the actual error text.
        const dvv = new DataView(memory.buffer);
        for (const off of [0, 4, 8]) {
            const ptr = dvv.getUint32(0x40120 + off, true);
            if (ptr > 0x1000 && ptr < memory.buffer.byteLength - 64) {
                const bs = new Uint8Array(memory.buffer, ptr, 48);
                const ascii = Array.from(bs).map(c => (c >= 32 && c < 127) ? String.fromCharCode(c) : '.').join('');
                console.log('    ptr@+' + off + '=0x' + ptr.toString(16) + ' → "' + ascii + '"');
            }
        }
    }
    if (rc !== 0) {
        if (typeof mini.AzStartup_peekU32 === 'function') {
            const le = mini.AzStartup_peekU32(0x40080);
            if ((le >>> 16) === 0x4c45) {
                const names = {1:'InvalidTree',2:'SizingFailed',3:'PositioningFailed',4:'DisplayListFailed',5:'Text'};
                const subNames = {1:'BidiError',2:'ShapingError',3:'FontNotFound',4:'InvalidText',5:'HyphenationError'};
                let msg = names[le & 0xff] || ('code' + (le & 0xff));
                if ((le & 0xff) === 5) msg += '(' + (subNames[(le >> 8) & 0xff] || ('sub' + ((le >> 8) & 0xff))) + ')';
                console.error('POST-RC' + rc + ': LayoutError = ' + msg);
            }
            const hm = mini.AzStartup_peekU32(0x40084);
            if ((hm >>> 16) === 0x484d) {
                const v = hm & 0xffff;
                console.error('POST-RC' + rc + ': std::HashMap self-test read-back = ' + v + (v === 42 ? ' → HashMaps WORK' : ' → HashMaps BROKEN'));
            }
            const famLen = mini.AzStartup_peekU32(0x40100);
            if (famLen > 0 && famLen < 64) {
                const fb = new Uint8Array(memory.buffer, 0x40104, famLen);
                console.error('POST-RC' + rc + ': FontNotFound family = "' + new TextDecoder().decode(fb) + '"');
            }
            const fm = mini.AzStartup_peekU32(0x40088);
            if ((fm >>> 28) === 0xF) {
                const parsed = (fm >>> 12) & 0xfff, chains = fm & 0xfff;
                console.error('POST-RC' + rc + ': font_manager parsed_fonts=' + parsed + ' font_chain_cache=' + chains +
                    (chains === 0 ? ' → NO chains resolved (collect_and_resolve_font_chains failed)' :
                     parsed === 0 ? ' → chains OK but 0 fonts LOADED (font load/PARSE failed — allsorts mis-lift)' :
                     ' → fonts loaded; FontNotFound is in the per-node lookup'));
            }
        }
        fail('solveLayoutReal rc=' + rc + ' (1=null,2=no-styled,3=0-nodes,4=LayoutWindow,5=layout Err,6=alloc)');
    }

    const rectsLen = mini.AzStartup_getPositionedRectsLen(state);
    const rectsPtr = mini.AzStartup_getPositionedRectsPtr(state);
    if (rectsPtr === 0 || rectsLen < 1) fail('no positioned rects (ptr=' + rectsPtr + ' len=' + rectsLen + ')');
    if (rectsLen !== EXPECT.length && !LENIENT) fail('rects_len ' + rectsLen + ' != expected ' + EXPECT.length);

    const dv = new DataView(memory.buffer);
    const got = [];
    for (let i = 0; i < rectsLen; i++) {
        const off = rectsPtr + i * 16;
        got.push([dv.getUint32(off, true), dv.getUint32(off + 4, true), dv.getUint32(off + 8, true), dv.getUint32(off + 12, true)]);
    }
    if (LENIENT) {
        console.log('[lenient] rects (' + rectsLen + '): ' + got.map(r => '(' + r.join(',') + ')').join(' '));
        process.exit(0);
    }

    const LABELS = ['body', 'container', 'item1(grow1)', 'item2(grow2)', 'item3(grow3)'];
    let mismatch = 0;
    console.log('  node                wasm (x,y,w,h)         native ref (x,y,w,h)');
    for (let i = 0; i < EXPECT.length; i++) {
        const g = got[i], e = EXPECT[i];
        const ok = g[0]===e[0] && g[1]===e[1] && g[2]===e[2] && g[3]===e[3];
        if (!ok) mismatch++;
        console.log('  [' + i + '] ' + (LABELS[i]||'').padEnd(14) +
            ('(' + g.join(',') + ')').padEnd(22) + ' ' +
            ('(' + e.join(',') + ')').padEnd(22) + (ok ? '  ✓' : '  ✗ MISMATCH'));
    }
    // Independent sanity: a real flex solve (all non-zero, not the block-flow stub).
    const allZero = got.every(r => r.every(v => v === 0));
    if (allZero) fail('all rects (0,0,0,0) — layout_document did not run');
    const stubShaped = got.every((r, i) => r[0] === 0 && r[1] === i * 30 && r[2] === viewportW && r[3] === 30);
    if (stubShaped) fail('rects match the block-flow STUB — real solver not wired');

    if (mismatch > 0) fail(mismatch + ' of ' + EXPECT.length + ' rects differ from the native solver — lift is NOT faithful');
    console.log('\nPASS: lifted wasm layout == native solver, all ' + EXPECT.length + ' rects exact (flex-grow 1:2:3 split 128/250/372) ✓');
    process.exit(0);
})().catch(e => { console.error(e.stack); process.exit(1); });
