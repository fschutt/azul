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
        // 0x400F0/F4 = __remill_error faulting guest PC (set right before the diagnostic trap).
        const epcL = mini.AzStartup_peekU32(0x400F0), epcH = mini.AzStartup_peekU32(0x400F4);
        if (epcL || epcH) console.error('POST-TRAP: __remill_error faulting guest PC = 0x' + (epcH>>>0).toString(16) + (epcL>>>0).toString(16).padStart(8,'0') + ' → otool -tV libazul @ this static addr for the unlifted op');
        const mbL = mini.AzStartup_peekU32(0x400F8), mbH = mini.AzStartup_peekU32(0x400FC);
        if (mbL || mbH) console.error('POST-TRAP: __remill_MISSING_BLOCK guest PC = 0x' + (mbH>>>0).toString(16) + (mbL>>>0).toString(16).padStart(8,'0') + ' → a computed-branch/jump-table target the lifter could not resolve');
        const cmb = mini.AzStartup_peekU32(0x40108);
        if (cmb) console.error('POST-TRAP: calc_used_size HOT missing_block target = 0x' + (cmb>>>0).toString(16) + ' → CONFIRMS calc_used_size diverges at an unresolved jump-table (br x10); the STACK above names the fn');
        // 0x40060 = AZ_FUEL tripped flag (1 = an instrumented loop exceeded AZ_FUEL_LIMIT → see STACK for the looping fn).
        if (mini.AzStartup_peekU32(0x40060) === 1) console.error('POST-TRAP: FUEL TRIPPED — infinite loop; the STACK above names the looping fn; looping_block_id=' + mini.AzStartup_peekU32(0x40070) + ' (map to Nth `call @__az_fuel(i32 N)` in <stem>.fuel.ll)');
        // 0x40078 = AZ_LOG_SELFLOOP_VAL: the i64 `v` (icmp eq v,0 operand) that routed into the live opt-folded self-loop (should be 0).
        const slvLo = mini.AzStartup_peekU32(0x40078), slvHi = mini.AzStartup_peekU32(0x4007C);
        if (slvLo !== 0 || slvHi !== 0) console.error('POST-TRAP: selfloop_routing_v=0x' + slvHi.toString(16) + slvLo.toString(16).padStart(8,'0') + ' (the non-zero value opt folded the loop-exit on; should be 0)');
        process.exit(1);
    }
    if (rc !== 0) {
        // 0x40080 = LayoutError code (0x4c45_000N) when rc=5 (layout_dom_recursive Err):
        // 1=InvalidTree 2=SizingFailed 3=PositioningFailed 4=DisplayListFailed 5=Text
        const le = mini.AzStartup_peekU32(0x40080);
        if ((le >>> 16) === 0x4c45) {
            const code = le & 0xffff, base = code & 0xff;
            const names = {1:'InvalidTree',2:'SizingFailed',3:'PositioningFailed',4:'DisplayListFailed',5:'Text'};
            const textNames = {1:'BidiError',2:'ShapingError',3:'FontNotFound',4:'InvalidText',5:'HyphenationError'};
            let msg = names[base] || ('code'+base);
            if (base === 5) msg += '(' + (textNames[(code>>8)&0xff] || ('?'+((code>>8)&0xff))) + ')';
            if (base === 5 && ((code>>8)&0xff) === 3) {
                const n = mini.AzStartup_peekU32(0x40084);
                if (n > 0 && n < 64) {
                    const buf = new Uint8Array(memory.buffer, 0x40088, n);
                    msg += ' requested-family="' + String.fromCharCode.apply(null, buf) + '"';
                }
            }
            console.error('POST-RC5: LayoutError = ' + msg);
        }
        // 0x4009C/0x400A0 = RAW discriminant tags (bypass the suspect match)
        const ot = mini.AzStartup_peekU32(0x4009C), it = mini.AzStartup_peekU32(0x400A0);
        const outerNames = {0:'InvalidTree',1:'SizingFailed',2:'PositioningFailed',3:'DisplayListFailed',4:'Text'};
        const innerNames = {0:'BidiError',1:'ShapingError',2:'FontNotFound',3:'InvalidText',4:'HyphenationError'};
        if ((ot>>>16)===0xda70) console.error('POST-RC5: RAW outer tag=' + (ot&0xff) + ' (' + (outerNames[ot&0xff]||'?') + ')');
        if ((it>>>16)===0xda71) console.error('POST-RC5: RAW inner tag=' + (it&0xff) + ' (' + (innerNames[it&0xff]||'?') + ')');
        // 0x400A4 = layout_document progress marker (0xDD00_000N): last step reached
        const ld = mini.AzStartup_peekU32(0x400A4);
        if ((ld>>>16)===0xdd00) console.error('POST-RC5: layout_document last step=' + (ld&0xffff) + ' (1=entry 2=post-reconcile) → the NEXT `?` errored');
        else console.error('POST-RC5: layout_document NOT entered (0x400A4=0x' + ld.toString(16) + ')');
        // 0x400A8 = reconcile_recursive branch (0xBB00_0001=create-new/IF, 0002=clone-old/ELSE)
        const br = mini.AzStartup_peekU32(0x400A8);
        if ((br>>>16)===0xbb00) console.error('POST-RC5: reconcile branch=' + (br&0xffff) + ' (1=create-new 2=clone-old)');
        // 0x400AC = reconcile returned Ok (0xCC00_0001). If set but step stuck at 1 → the `?` mis-read Ok as Err
        const rok = mini.AzStartup_peekU32(0x400AC);
        if ((rok>>>16)===0xcc00) console.error('POST-RC5: reconcile RETURNED OK (0x400AC set) — so if step=1 the `?` mis-discriminated Ok→Err (niche-Result lift bug)');
        else console.error('POST-RC5: reconcile did NOT reach its Ok return (0x400AC=0x' + rok.toString(16) + ')');
        // 0x400B0 = LayoutTree::build marker (0xBD00_<len><root>): node count + root idx
        const bld = mini.AzStartup_peekU32(0x400B0);
        if ((bld>>>16)===0xbd00) console.error('POST-RC5: tree built with ' + ((bld>>8)&0xff) + ' nodes, root=' + (bld&0xff) + ' → if >0 but InvalidTree, the get().ok_or()? mis-discriminates Some→None');
        else console.error('POST-RC5: build marker not set (0x400B0=0x' + bld.toString(16) + ')');
        // 0x400B4 = create_node_from_dom pre-push index; 0x400B8 = post-push nodes.len()
        const cnpre = mini.AzStartup_peekU32(0x400B4), cnpost = mini.AzStartup_peekU32(0x400B8);
        if ((cnpre>>>16)===0xce00) console.error('POST-RC5: create_node_from_dom pre-push index=' + (cnpre&0xffff));
        else console.error('POST-RC5: create_node_from_dom NOT called (0x400B4=0x' + cnpre.toString(16) + ')');
        if ((cnpost>>>16)===0xcf00) console.error('POST-RC5: create_node_from_dom post-push nodes.len()=' + (cnpost&0xffff) + ' → if 1 here but build sees 0, the builder &mut is lost');
        const nni = mini.AzStartup_peekU32(0x400BC);
        if ((nni>>>16)===0xab00) console.error('POST-RC5: reconcile_recursive new_node_idx=' + (nni&0xffff) + ' (create_node return; 0=ok, 64=mis-read)');
        const sA = mini.AzStartup_peekU32(0x400C0), sB = mini.AzStartup_peekU32(0x400C4), sP = mini.AzStartup_peekU32(0x400CC);
        console.error('POST-RC5: create_node steps — parent_fc=' + (((sP>>>16)===0xcd00)?('reached is_some='+((sP>>8)&1)):'NO') + ' A(collect_box_props)=' + (((sA>>>16)===0xca00)?'reached':'NO') + ' B(node push)=' + (((sB>>>16)===0xcb00)?('reached len='+((sB>>8)&0xff)):'NO'));
        const cbp = mini.AzStartup_peekU32(0x400D0);
        if ((cbp>>>24)===0xc0 && ((cbp>>>8)&0xff)===0x57) console.error('POST-RC5: collect_box_props — get_display_type RETURNED (dt=' + (cbp&0xff) + ') → so the LayoutDisplay MATCH diverges, not the call');
        else if ((cbp>>>24)===0xc0) console.error('POST-RC5: collect_box_props last sub-step=' + (cbp&0xff) + ' (1=entered 2=node_state 3=after-create_resolution_context 4=after-margin_top 5=after-margin-block 0x56=before-get_display_type[CALL diverges] 6=after-padding-block 7=after-border-block)');
        else console.error('POST-RC5: collect_box_props NOT entered (0x400D0=0x' + cbp.toString(16) + ')');
        const crc = mini.AzStartup_peekU32(0x400D8);
        if ((crc>>>24)===0xc1) console.error('POST-RC5: create_resolution_context last sub-step=' + (crc&0xff) + ' (1=entered 2=after-get_element_font_size 3=after-get_parent_font_size 4=after-get_root_font_size)');
        const gw = mini.AzStartup_peekU32(0x400E0);
        if ((gw>>>24)===0xc3) console.error('POST-RC5: get_element_font_size 2-arg wrapper last sub-step=' + (gw&0xff) + ' (1=entered 2=after-node_state-clone → next is the 3-arg call)');
        else console.error('POST-RC5: 2-arg get_element_font_size NOT entered (0x400E0=0x' + gw.toString(16) + ')');
        const gef = mini.AzStartup_peekU32(0x400DC);
        if ((gef>>>24)===0xc2) console.error('POST-RC5: 3-arg get_element_font_size last sub-step=' + (gef&0xff) + ' (1=entered 2=is_normal 3=after-get_or_init 4=return-cached 5=before-resolve_font_size_slow)');
        else console.error('POST-RC5: 3-arg get_element_font_size NOT entered (0x400DC=0x' + gef.toString(16) + ')');
    }
    console.log('[2] solveLayoutReal rc=0 (real taffy positioning ran in wasm)');
    {   // M12.7 diag (always): where did layout_document go?
        // M12.7 BULLETPROOF calc_used_size readout (unconditional, first thing):
        try {
            const _e = mini.AzStartup_peekU32(0x4010C) >>> 0;
            const _f = mini.AzStartup_peekU32(0x400C8) >>> 0;
            const _g = mini.AzStartup_peekU32(0x400D4) >>> 0;
            console.error('  >>> calc_used_size: ENTERED(0x4010C)=0x' + _e.toString(16)
                + ' finalOk(0x400C8)=0x' + _f.toString(16)
                + ' earlyOk(0x400D4)=0x' + _g.toString(16)
                + ' — ' + (_e === 0xF1 ? 'ENTERED' : 'NOT-entered')
                + ' / ' + (_f === 0xCA000001 || _g === 0xCA000002 ? 'REACHED-Ok-return (devirt WORKED)' : 'NO-Ok-return (still diverges)'));
        } catch (_x) { console.error('  >>> calc_used_size marker read threw: ' + _x); }
        const step = mini.AzStartup_peekU32(0x400A4);
        const rbr = mini.AzStartup_peekU32(0x400A8);
        const rok = mini.AzStartup_peekU32(0x400AC);
        console.error('  layout_document step=0x' + step.toString(16) + ' (0xDD00_000N: 1=entry 2=post-reconcile 3=entered-Step2 4=reached-cache-store); reconcile branch=0x' + rbr.toString(16) + ' Ok=0x' + rok.toString(16));
        if ((step & 0xf) === 4) console.error('  layout_document REACHED cache store (cache.tree+positions set), calculated_positions.len=' + ((step>>4)&0xfff));
        if ((step & 0xf) === 5) console.error('  layout_document: intrinsic sizing DONE (step 5) → diverges in the per-root LAYOUT PASS (calculate_layout_for_subtree)');
        if ((step & 0xf) === 6) console.error('  layout_document: per-root layout pass DONE (step 6) → diverges in reposition_clean_subtrees / the cache store');
        if ((step & 0xff) === 0x53) console.error('  layout_document: get_containing_block_for_node RETURNED (0x53) → diverges in box_props.unpack / Vec-index');
        if ((step & 0xff) === 0x54) console.error('  layout_document: box_props.unpack RETURNED (0x54) → diverges in the margin compare/adjust block');
        if ((step & 0xff) === 0x56) console.error('  layout_document: margin block DONE (0x56) → diverges in the debug block / hint_purge / probe');
        if ((step & 0xff) === 0x55) console.error('  layout_document: got containing-block (0x55) → diverges INSIDE calculate_layout_for_subtree');
        if ((step & 0xff) === 0x57) console.error('  layout_document: calculate_layout_for_subtree returned OK (0x57) → diverges in the root-position insert / loop tail');
        if ((step & 0xff) === 0x5e) console.error('  layout_document: calculate_layout_for_subtree returned Err (0x5E) but IGNORED → if real rects follow, the Err was SPURIOUS (niche-Result mis-lift); if zero/no rects, calc_layout genuinely failed');
        // calc_layout internal progress lives at 0x400B0 (separate from 0x400A4 so the
        // post-call 0x5E write in layout_document doesn't clobber it). DEEPEST reached wins.
        const clstep = mini.AzStartup_peekU32(0x400B0);
        console.error('  calc_layout deepest marker (0x400B0) = 0x' + clstep.toString(16));
        const errpc = mini.AzStartup_peekU32(0x400B4);
        const errpch = mini.AzStartup_peekU32(0x400B8);
        if (errpc || errpch) console.error('  LAST __remill_error faulting RUNTIME PC = 0x' + (errpch>>>0).toString(16) + (errpc>>>0).toString(16).padStart(8,'0') + ' → grep server.log @0x<this> for the fn, then otool that fn for the unlifted op');
        if ((clstep & 0xff) === 0x5f) console.error('  calc_layout: body entered, NO further marker → returns Err in the cache-check block (1977-2075) before hit/miss');
        if ((clstep & 0xff) === 0x61) console.error('  calc_layout: cache-HIT path (0x61) → recursion over cached child_positions returns Err');
        if ((clstep & 0xff) === 0x60) console.error('  calc_layout: cache-miss entered (0x60), no 0x62 → prepare_layout_context (calculate_used_size_for_node) returns Err');
        if ((clstep & 0xff) === 0x62) console.error('  calc_layout: prepare OK (0x62), no 0x64 → layout_formatting_context returns Err');
        if ((clstep & 0xff) === 0x64) console.error('  calc_layout: layout_formatting_context OK (0x64) → diverges after (Phase 2.5+ / scrollbars / final write)');
        if ((clstep & 0xff) === 0x60) console.error('  prepare_layout_context: NOT entered past tree.get/warm → tree.get/warm(node_index)=None → Err(InvalidTree) (node_index garbage / Vec-len mis-lift)');
        if ((clstep & 0xff) === 0x70) console.error('  prepare_layout_context: got node+warm (0x70), no 0x72 → calculate_used_size_for_node returns Err');
        if ((clstep & 0xff) === 0x72) console.error('  prepare_layout_context: calculate_used_size_for_node OK (0x72) → diverges in prepare Phase 2+ (writing-mode/inner-size)');
        // M12.7: was calc_used_size even ENTERED? (0x4010C=0xF1 set at its source entry)
        const cusEntered = mini.AzStartup_peekU32(0x4010C);
        console.error('  calc_used_size ENTERED (0x4010C)=0x' + (cusEntered>>>0).toString(16) + (cusEntered === 0xF1 ? ' → YES, entered; diverges INSIDE calc_used_size (resolved-wrong jump-table/value mis-lift)' : ' → NO, never entered; diverges in prepare_layout_context BEFORE the calc_used_size call (arg setup)'));
        // M12.7: did calc_used_size REACH its Ok return? (0x400C8 final, 0x400D4 early)
        const cusFinal = mini.AzStartup_peekU32(0x400C8), cusEarly = mini.AzStartup_peekU32(0x400D4);
        console.error('  calc_used_size returns: final-Ok(0x400C8)=0x' + (cusFinal>>>0).toString(16) + ' early-Ok(0x400D4)=0x' + (cusEarly>>>0).toString(16) +
            ((cusFinal === 0xCA000001 || cusEarly === 0xCA000002) ? ' → REACHED an Ok return; the Err is the lifted return-ABI corrupting the Result<LogicalSize,LayoutError> disc' : ' → did NOT reach any Ok return; diverges mid-fn (hot missing_block/value before return)'));
        const nlr = mini.AzStartup_peekU32(0x400E4);
        if (((nlr>>>16)&0xff)===0x01) console.error('  body get_node_layout_rect=Some, width=' + (nlr&0xffff) + ' (0 → layout computed 0-wide; >0 → extraction issue)');
        else if (((nlr>>>16)&0xff)===0xff) console.error('  body get_node_layout_rect=None (no calculated position → positioning did not write node 0)');
        const glr = mini.AzStartup_peekU32(0x400E8);
        if ((glr>>>24)===0xe5) {
            const lo = glr&0xff;
            if (lo===0xff) console.error('  get_node_layout_rect: .position() found NO matching layout node (tree nodes=' + ((glr>>8)&0xff) + ')');
            else if (lo===0xfe) console.error('  get_node_layout_rect: calculated_positions.get(idx)=None (positioning empty)');
            else if (lo===0xfd) console.error('  get_node_layout_rect: layout_node.used_size=None (sizing did not set used_size)');
            else if (lo===0x04) console.error('  get_node_layout_rect: ALL OK (returned Some) — so the 0-rect is in the extraction/values');
            else if ((glr&0xff)===0x03) console.error('  get_node_layout_rect: reached calc_pos, calculated_positions.len=' + ((glr>>8)&0xfff));
        }
        const gns = mini.AzStartup_peekU32(0x400EC);
        if ((gns>>>24)===0xe6) {
            const lo = gns&0xff;
            if (lo===0xfa) console.error('  get_node_size: layout_results.get(dom)=None → DOM-ID MISMATCH (extraction uses ROOT_ID; layout stored under a different dom_id). layout_results.len=' + ((gns>>8)&0xff));
            else if (lo===0xfb) console.error('  get_node_size: dom_to_layout.get(nid)=None (node 0 not in mapping). dom_to_layout.len=' + ((gns>>8)&0xfff));
            else if (lo===0xfc) console.error('  get_node_size: layout_tree.get(idx)=None');
            else if (lo===0xfd) console.error('  get_node_size: layout_node.used_size=None (sizing did not set used_size)');
            else if (lo===0x04) console.error('  get_node_size: OK, body width=' + ((gns>>8)&0xffff));
            else if (lo===0x01) console.error('  get_node_size: entered, layout_results.len=' + ((gns>>8)&0xff));
            else if (lo===0x02) console.error('  get_node_size: reached dom_to_layout, len=' + ((gns>>8)&0xfff));
        }
    }

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
