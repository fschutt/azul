// M12.7 probe — bootstraps like layout-real.js but reads markers + rects
// DIRECTLY from wasm memory (azul-mobile lacks the AzStartup_peekU32 export).
const http = require('http');
function fetch_(p) {
    return new Promise((r,j) => http.get('http://127.0.0.1:8800' + p, x => {
        const c = []; x.on('data',b=>c.push(b)); x.on('end',()=>r(Buffer.concat(c))); x.on('error', j);
    }));
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
        fetch_(miniUrl), fetch_(layoutUrl), cbUrl ? fetch_(cbUrl) : Promise.resolve(null),
    ]);
    const table = new WebAssembly.Table({ initial: 64, element: 'anyfunc' });
    let cbTableIdx = -1, memory = null;
    function memsetImpl(d, v, n) { new Uint8Array(memory.buffer).fill(v & 0xFF, d, d + n); return d; }
    function memcpyImpl(d, s, n) { new Uint8Array(memory.buffer).copyWithin(d, s, s + n); return d; }
    const realEnv = {
        __indirect_function_table: table,
        __az_resolve_callback: (addr) => (addr === 0xFFFFFFFF) ? 0xFFFFFFFF : (cbTableIdx >= 0 ? cbTableIdx : 0xFFFFFFFF),
        memset: memsetImpl, memcpy: memcpyImpl, memmove: memcpyImpl,
    };
    const AZ_MATH = { fmaxf:(a,b)=>a!==a?b:(b!==b?a:Math.max(a,b)), fminf:(a,b)=>a!==a?b:(b!==b?a:Math.min(a,b)), fmax:(a,b)=>a!==a?b:(b!==b?a:Math.max(a,b)), fmin:(a,b)=>a!==a?b:(b!==b?a:Math.min(a,b)), roundf:x=>Math.sign(x)*Math.round(Math.abs(x)), round:x=>Math.sign(x)*Math.round(Math.abs(x)), fabsf:Math.abs, fabs:Math.abs, sqrtf:Math.sqrt, sqrt:Math.sqrt, floorf:Math.floor, floor:Math.floor, ceilf:Math.ceil, ceil:Math.ceil, truncf:Math.trunc, trunc:Math.trunc, powf:Math.pow, pow:Math.pow };
        const stubFor = n => AZ_MATH[n] || (/write_memory|barrier|exception_clear/.test(n) ? () => {}
        : (/_f64\b/.test(n) ? () => 0 : (/_64\b/.test(n) ? () => 0n : () => 0)));
    const h = env => ({ get: (_, p) => typeof p === 'string'
        ? (Object.prototype.hasOwnProperty.call(env, p) ? env[p] : stubFor(p)) : undefined, has: () => true });
    const { instance: miniI } = await WebAssembly.instantiate(miniBytes, { env: new Proxy({}, h(realEnv)) });
    const mini = miniI.exports;
    memory = mini.memory;
    const cbEnv = { memory, __indirect_function_table: table, memset: memsetImpl, memcpy: memcpyImpl, memmove: memcpyImpl };
    if (cbBytes) {
        const { instance: cbI } = await WebAssembly.instantiate(cbBytes, { env: new Proxy({}, h(cbEnv)) });
        table.grow(1); cbTableIdx = table.length - 1; table.set(cbTableIdx, cbI.exports.callback);
    }
    const { instance: layoutI } = await WebAssembly.instantiate(layoutBytes, { env: new Proxy({}, h(cbEnv)) });
    table.grow(1); const layoutTableIdx = table.length - 1; table.set(layoutTableIdx, layoutI.exports.callback);

    const state = mini.AzStartup_init(0, 0);
    const modelPtr = mini.AzStartup_alloc(4);
    new DataView(memory.buffer).setUint32(modelPtr, initialCounter, true);
    const refanyPtr = mini.AzStartup_hydrate(Number(typeId & 0xFFFFFFFFn), Number((typeId >> 32n) & 0xFFFFFFFFn), modelPtr, 4);
    mini.AzStartup_setRefAny(state, refanyPtr);
    mini.AzStartup_setLayoutCbTableIdx(state, layoutTableIdx);
    mini.AzStartup_setModelPtr(state, modelPtr);
    mini.AzStartup_registerCbNode(state, 0);
    mini.AzStartup_setDisplayNode(state, 0);
    const vw = 800, vh = 600;
    if (mini.AzStartup_initLayoutCache(state, vw, vh, 0) !== 0) { console.error('initLayoutCache failed'); process.exit(1); }
    if (mini.AzStartup_hydrateStyledDom(state) !== 0) { console.error('hydrateStyledDom failed'); process.exit(1); }
    console.log('[1] cascade node_count=' + mini.AzStartup_getStyledDomNodeCount(state));

    const dv = new DataView(memory.buffer);
    const peek = a => dv.getUint32(a, true) >>> 0;
    const hx = a => '0x' + peek(a).toString(16);
    let rc;
    try { rc = mini.AzStartup_solveLayoutReal(state, vw, vh); }
    catch (e) { console.error('TRAP: ' + e.message + '\n' + e.stack); rc = 'TRAP'; }
    console.log('[2] solveLayoutReal rc=' + rc + '  isLayoutSolved=' + mini.AzStartup_isLayoutSolved(state));

    console.log('\n=== MARKERS (read direct from wasm memory) ===');
    console.log('  calc_used_size ENTERED 0x4010C=' + hx(0x4010C) + (peek(0x4010C)===0xF1?' (YES)':' (NO)'));
    console.log('  calc_used_size finalOk 0x400C8=' + hx(0x400C8) + (peek(0x400C8)===0xCA000001?' (REACHED Ok)':''));
    console.log('  calc_layout phase     0x400B0=' + hx(0x400B0) + '  (DD00_xx: 5F=body 60=miss 62=prepOk 64=lfcOk 70=node+warm 72=cus-Ok)');
    // 0x40110 = tree BUILD: 0xBD_<d2l_len>_<nodes_len>_<root> (PRE-clone dom_to_layout.len)
    { const b = peek(0x40110); if ((b>>>24)===0xBD) console.log('  tree BUILD (pre-clone)  0x40110=' + hx(0x40110) + '  → dom_to_layout.len=' + ((b>>16)&0xff) + ' nodes.len=' + ((b>>8)&0xff) + ' root=' + (b&0xff));
      else console.log('  tree BUILD            0x40110=' + hx(0x40110) + ' (marker not set → build() not reached / not lifted)'); }
    // 0x400EC = get_node_size: E6_..XX  (01+lr.len, FA=lr-miss, 02+d2l.len, FB=d2l-miss, FC, FD, 04+width)
    { const g = peek(0x400EC), lo = g&0xff;
      let m = 'lr.len='+((g>>8)&0xff);
      if (lo===0xfa) m='FA layout_results.get(ROOT)=None (lr.len='+((g>>8)&0xff)+')';
      else if (lo===0xfb) m='FB dom_to_layout.get(node0)=None — POST-CLONE d2l.len='+((g>>8)&0xfff);
      else if (lo===0xfc) m='FC layout_tree.get(idx)=None';
      else if (lo===0xfd) m='FD used_size=None';
      else if (lo===0x04) m='OK body width='+((g>>8)&0xffff);
      else if (lo===0x02) m='reached d2l, post-clone d2l.len='+((g>>8)&0xfff);
      else if (lo===0x01) m='entry, lr.len='+((g>>8)&0xff);
      console.log('  get_node_size         0x400EC=' + hx(0x400EC) + '  → ' + m); }
    // 0x400E8 = get_node_position: E5_..XX
    { const p = peek(0x400E8), lo = p&0xff;
      let m = '?';
      if (lo===0xfa) m='FA layout_results.get(ROOT)=None';
      else if (lo===0xfb) m='FB dom_to_layout.get(node0)=None (d2l.len='+((p>>8)&0xfff)+')';
      else if (lo===0xfe) m='FE calculated_positions.get(idx)=None (calc_pos.len='+((p>>8)&0xfff)+')';
      else if (lo===0x04) m='OK';
      console.log('  get_node_position     0x400E8=' + hx(0x400E8) + '  → ' + m); }
    // 0x40144 = raw arg viewport_w (wrapper); 0x40148 = ws.width after LogicalSize store (wrapper)
    console.log('  WRAPPER viewport_w arg(0x40144)=' + peek(0x40144) + '  ws.width after store(0x40148)=' + peek(0x40148)
        + (peek(0x40144)===0 ? '  → ARG 0 (wasm→ARM arg threading lost viewport_w)'
           : (peek(0x40148)===0 ? '  → arg OK but WS.WIDTH 0 (as-f32 UCVTF or LogicalSize store mis-lifted in WRAPPER)'
              : '  → wrapper established 800; loss is DOWNSTREAM in layout_dom_recursive read/propagation')));
    // M12.7 width-loss bracket inside layout_document: 0x401E0 viewport arg, 0x401E4 cb_size, 0x401E8 ctx.viewport_size
    { const va=peek(0x401E0), cbs=peek(0x401E4), cvs=peek(0x401E8);
      console.log('  layout_document BRACKET  viewport-arg(0x401E0)=' + va + '  cb_size(0x401E4)=' + cbs + '  ctx.viewport_size(0x401E8)=' + cvs
        + (va===0 ? '  → HFA ARG lost (LogicalRect viewport arg = 0 across the lifted call)'
           : cbs===0 ? '  → arg OK but get_containing_block_for_node returned cb_size=0 (HFA RETURN lost)'
           : cvs===0 ? '  → cb OK but ctx.viewport_size=0 (viewport.size→ctx copy lost)'
           : '  → all 800 in layout_document; loss is in calculate_layout_for_subtree→calc_used_size arg passing')); }
    // 0x401EC/F0 = containing_block_size + ctx.viewport_size AT prepare_layout_context (right before the calc_used_size call)
    { const pcb=peek(0x401EC), pvp=peek(0x401F0);
      console.log('  prepare_layout_ctx BRACKET  cb(0x401EC)=' + pcb + '  viewport(0x401F0)=' + pvp
        + ((pcb!==0||pvp!==0) ? '  → 800 HERE but calc_used_size reads 0 ⟹ HFA(LogicalSize) ARG passing lost at the calc_used_size call'
           : '  → already 0 here ⟹ loss is upstream in calculate_layout_for_subtree→prepare_layout_context')); }
    // 0x40228 = cb.width inside the auto_block_inline_size helper; 0x4022C = aw (subtraction) before .max
    { const hcb=peek(0x40228), haw=peek(0x4022C);
      console.log('  auto_block helper  cb.width(0x40228)=' + hcb + '  aw-before-max(0x4022C)=' + haw
        + (hcb!==800 ? '  → ✗ cb POINTER/deref into the helper lost it (GP-arg or stack-ptr-across-call)'
           : haw!==800 ? '  → cb=800 but subtraction gave ' + haw + ' (box-model read mis-lift)'
           : '  → cb=800, aw=800 in helper; the .max or out-ptr write/read loses it')); }
    // M12.7 round-trip bisection: 0x40240=auto_w as the Block arm reads it back after the
    // helper call; 0x40244=resolved_width after the match + .max(0.0)
    { const aw=peek(0x40240), rw=peek(0x40244);
      console.log('  Block-arm round-trip  auto_w-after-call(0x40240)=' + aw + '  resolved_width-after-max(0x40244)=' + rw
        + (aw!==800 ? '  → ✗✗ auto_w round-trip BROKEN: helper wrote 800 to FP-184 but caller reloads ' + aw + ' (out-ptr/stack/FP mismatch)'
           : rw!==800 ? '  → ✗ auto_w=800 but resolved_width=' + rw + ' (match-arm phi/binding or .max(0.0) mis-lift)'
           : '  → ✓ auto_w=800, resolved_width=800; loss is between here and apply_width_constraints')); }
    // M12.7 apply_width_constraints clamp bracket: 0x40230=tentative_width (in),
    // 0x40234=min_width, 0x40238=max_width sentinel (0xFFFFFFFF=None), 0x4023C=result (out)
    { const tw=peek(0x40230), mn=peek(0x40234), mx=peek(0x40238), res=peek(0x4023C);
      const mxs = (mx===0xFFFFFFFF ? 'None' : 'Some('+mx+')');
      console.log('  apply_width_constraints  tentative(0x40230)=' + tw + '  min(0x40234)=' + mn
        + '  max(0x40238)=' + mxs + '  result(0x4023C)=' + res
        + (tw!==800 ? '  → ✗ tentative≠800: loss is BEFORE the clamp (calc .max / helper round-trip)'
           : (mx!==0xFFFFFFFF && mx===0) ? '  → ✗✗ max_width=Some(0) ZEROED it — get_css_max_width mis-lifts (CSS getter returns corrupted prop)'
           : res!==800 ? '  → ✗ clamp dropped it despite tentative=800 (min='+mn+' max='+mxs+')'
           : '  → ✓ apply_width_constraints returns 800; loss is in box-sizing/from_main_cross')); }
    // 0x401F4 = containing_block_size.width AS RECEIVED inside calc_used_size (via the by-ref pointer)
    { const rcb=peek(0x401F4);
      console.log('  calc_used_size RECEIVED cb.width(0x401F4)=' + rcb
        + (rcb===800 ? '  → ✓ BY-REF DELIVERED IT (any width=0 is a compute mis-lift inside calc)'
           : rcb===0 ? '  → ✗ by-ref pointer/deref STILL 0 (GP-pointer arg or stack-ptr-across-call issue)'
           : '  → unexpected ' + rcb)); }
    // 0x40200 Block-arm reached; 0x401F8 available_width; 0x401FC box-model sum
    { const arm=peek(0x40200), aw=peek(0x401F8), bm=peek(0x401FC);
      console.log('  calc_used_size COMPUTE  Block-arm(0x40200)=0x' + arm.toString(16) + '  available_width(0x401F8)=' + aw + '  box-model-sum(0x401FC)=' + bm
        + (arm!==0xb10c0000 ? '  → Block arm NOT reached (display mis-lifted to a different arm)'
           : bm!==0 ? '  → box-model sum nonzero ⟹ &BoxProps deref/unpack mis-lift (garbage margins)'
           : aw===800 ? '  → available_width=800 ✓ (loss is .max/downstream)'
           : '  → available_width=' + aw + ' from cb=800, box-model=0 (the subtraction itself mis-lifts!)')); }
    // post-available_width chain: 0x40204 constrained_width, 0x40208 border_box_width, 0x4020C result.width, 0x40210 final_used_size (caller)
    { const cw=peek(0x40204), bbw=peek(0x40208), rw=peek(0x4020C), fus=peek(0x40210);
      console.log('  calc_used_size CHAIN  constrained(0x40204)=' + cw + '  border_box(0x40208)=' + bbw + '  result.width(0x4020C)=' + rw + '  final_used(caller 0x40210)=' + fus
        + (cw!==800 ? '  → apply_width_constraints dropped it (f32 arg/return mis-lift)'
           : bbw!==800 ? '  → box-sizing dropped it'
           : rw!==800 ? '  → from_main_cross dropped it (f32 args or LogicalSize HFA return)'
           : fus!==800 ? '  → calc_used_size Ok(LogicalSize) HFA RETURN mis-lifts (caller got 0)'
           : '  → 800 all the way to the caller ✓ (loss is in the tree store / get_node_size)')); }
    // 0x40214 resolved_width after .max(0.0); 0x40218 tentative_width received inside apply_width_constraints
    { const rwm=peek(0x40214), awc=peek(0x40218);
      console.log('  resolved_width.max(0.0)(0x40214)=' + rwm + '  apply_width_constraints RECEIVED(0x40218)=' + awc
        + (rwm!==800 ? '  → ✗ the arm `.max(0.0)` (fmaxnm) or match→resolved_width binding mis-lifts (800→' + rwm + ')'
           : awc!==800 ? '  → resolved_width=800 but apply_width_constraints got ' + awc + ' ⟹ single-f32 ARG mis-lifts across the call'
           : '  → 800 into apply_width_constraints; its return or internal logic drops it')); }
    // 0x40114/18/1C = calc_used_size width source: cb.width(in) viewport.width(in) result.width(out)
    console.log('  calc_used_size widths   cb=' + peek(0x40114) + ' viewport=' + peek(0x40118) + ' result=' + peek(0x4011C)
        + (peek(0x40114)===0 ? '  → CB WIDTH 0 (viewport not propagating to containing block)'
           : (peek(0x4011C)===0 ? '  → CB ok but RESULT 0 (resolved-width compute/float-lift zeroes it)' : '  → widths OK')));
    // 0x40120 resolved_width (px) ; 0x40124 0xD1_<dispByte>_<flags> bit0=Inline bit1=w_auto bit2=h_auto
    { const rw = peek(0x40120); const df = peek(0x40124);
      console.log('  resolved_width(0x40120)=' + rw + (df>>>24===0xD1 ? '  display=' + ((df>>8)&0xff) + ' isInline=' + (df&1) + ' w_auto=' + ((df>>1)&1) + ' h_auto=' + ((df>>2)&1) : '')
        + (rw===0 ? '  → RESOLVED_WIDTH 0 (css_width auto/stretch resolution zeroed it)' : '  → resolved OK, bug downstream (border-box/main-cross)')); }
    { const ml=peek(0x40128)|0, mr=peek(0x4012C)|0, pl=peek(0x40130)|0, pr=peek(0x40134)|0, bl=peek(0x40138)|0, aw=peek(0x4013C)|0;
      const big = v => (v>>>0) > 0x100000 || v < -0x100000;
      console.log('  box-model: margin.l='+ml+' margin.r='+mr+' pad.l='+pl+' pad.r='+pr+' border.l='+bl+' avail='+aw
        + ((big(ml)||big(mr)||big(pl)||big(pr)||big(bl)) ? '  → GARBAGE (pointer-like value in a box-model slot → mis-lifted store)' : (aw<=0 ? '  → margins legitimately eat the width' : '  → box-model OK'))); }
    { const bw = peek(0x40140), rw = peek(0x40120);
      console.log('  auto-width arm return(0x40140)=' + bw + ' vs resolved_width(0x40120)=' + rw
        + (bw>0 && rw===0 ? '  → ARM ok but RESOLVED lost it (match phi/spill mis-lift)' : (bw>0 && rw>0 ? '  → FIXED (value carried)' : (bw===0 ? '  → arm itself returned 0 (.max mis-lift in context)' : '')))); }
    console.log('  body rect marker      0x400E4=' + hx(0x400E4) + '  (E4_01=Some E4_FF=None)');
    console.log('  RAW LayoutError outer 0x4009C=' + hx(0x4009C) + '  inner 0x400A0=' + hx(0x400A0));
    // 0x400F0/F4 = last __remill_error PC + count; 0x400F8/FC = last missing_block PC + count
    { const epc=peek(0x400F0), ecnt=peek(0x400F4), mpc=peek(0x400F8), mcnt=peek(0x400FC);
      const nat = s => '0x'+(((s>>>0) - 0x110000 + 0x1098fc000) >>> 0 === 0 ? 0 : ((s>>>0) - 0x110000 + 0x1098fc000)).toString(16);
      console.log('  __remill_error  count=' + ecnt + ' lastPC=' + hx(0x400F0) + (epc? '  → native '+nat(epc)+' (otool libazul @ this for the unlifted instr)':''));
      console.log('  __remill_missing_block count=' + mcnt + ' lastPC=' + hx(0x400F8) + (mpc? '  → native '+nat(mpc):''));
      // 0x40160 + i*4 = 16-slot ring of distinct missing_block TARGET PCs (synth);
      // libazul vmaddr = synthPC - 0x110000 (otool target/release/libazul.dylib @ that).
      const nslot = Math.min(mcnt, 16);
      for (let i = 0; i < nslot; i++) { const pc = peek(0x40160 + i*4);
        if (pc) console.log('    mb[' + i + '] synth=0x' + pc.toString(16) + '  libazul-vmaddr=0x' + ((pc - 0x110000)>>>0).toString(16)); }
      // 0x4014C = count of SILENT indirect calls (no-op'd __remill_function_call);
      // 0x401A0 + i*4 = ring of their 16 most-recent target PCs (synth).
      const fcc = peek(0x4014C);
      console.log('  __remill_function_call (SILENT indirect calls, no-op today) count=' + fcc);
      const fslot = Math.min(fcc, 16);
      for (let i = 0; i < fslot; i++) { const pc = peek(0x401A0 + i*4);
        if (pc) console.log('    fc[' + i + '] synth=0x' + pc.toString(16) + '  libazul-vmaddr=0x' + ((pc - 0x110000)>>>0).toString(16)); }
      // 0x40158 = count of indirect targets the dispatcher could NOT resolve (fell to no-op).
      console.log('  @__az_indirect_dispatch UNRESOLVED count=' + peek(0x40158)
        + (peek(0x40158)===0 && (mcnt+fcc)>0 ? '  → (0 may mean weak no-op default won — see WEAK below)' : ''));
      // 0x4015C = weak no-op default hits. >0 ⟹ strong dispatcher .o did NOT override it.
      console.log('  @__az_indirect_dispatch WEAK-default hits=' + peek(0x4015C)
        + (peek(0x4015C)>0 ? '  → ✗ strong override FAILED (indirect calls stayed no-op)'
           : (peek(0x40158)>0 ? '  → ✓ strong dispatcher active' : ''))); }

    const rptr = mini.AzStartup_getPositionedRectsPtr(state), rlen = mini.AzStartup_getPositionedRectsLen(state);
    console.log('\n=== RECTS (ptr=' + rptr + ' len=' + rlen + ') ===');
    for (let i = 0; i < rlen; i++) {
        const o = rptr + i*16;
        console.log('  rect[' + i + ']: x=' + dv.getUint32(o,true) + ' y=' + dv.getUint32(o+4,true) + ' w=' + dv.getUint32(o+8,true) + ' h=' + dv.getUint32(o+12,true));
    }

    // === AZ_LOG_STORES ring buffer (count @0x41000, entries @0x41010+k*16: addr,id,deptag,val) ===
    const scnt = peek(0x41000);
    if (scnt > 0) {
        const cap = Math.min(scnt, 3500);
        console.log('\n=== STORE TRACE (' + scnt + ' in-window stores, showing ' + cap + ') ===');
        // Optional filters via env: AZ_STORE_FILTER_VAL (decimal, e.g. 800), AZ_STORE_ID (decimal id)
        const fval = process.env.AZ_STORE_FILTER_VAL ? (process.env.AZ_STORE_FILTER_VAL>>>0) : null;
        const fid  = process.env.AZ_STORE_ID ? (process.env.AZ_STORE_ID>>>0) : null;
        let shown = 0;
        for (let k = 0; k < cap; k++) {
            const o = 0x41010 + k*16;
            const addr = peek(o), id = peek(o+4), deptag = peek(o+8), val = dv.getUint32(o+12,true)>>>0;
            if (fval !== null && val !== fval) continue;
            if (fid !== null && id !== fid) continue;
            const sval = (val===0xDEADBEEF)?'<non-int>':(val===0xBEEF0000)?'<mem>':((val>>>0)+' (0x'+(val>>>0).toString(16)+(val|0<0?'':'')+')');
            console.log('  #' + k + ' id=' + id + ' addr=0x' + addr.toString(16) + ' val=' + sval);
            if (++shown > 600) { console.log('  ... (truncated at 600 shown)'); break; }
        }
        // Summarize ids storing the viewport width as int 800 OR f32 800.0 (bits
        // 0x44480000=1145569280), and ids storing 0 (== 0.0f32 too). The FIRST
        // id storing 800/800.0 then a LATER id storing 0 brackets the 800→0 loss.
        const F800 = 1145569280; // 0x44480000 = 800.0f32
        const saw800 = new Set(), saw0 = new Set();
        for (let k = 0; k < cap; k++) {
            const o = 0x41010 + k*16, id = peek(o+4), val = dv.getUint32(o+12,true)>>>0;
            if (val === 800 || val === F800) saw800.add(id); if (val === 0) saw0.add(id);
        }
        console.log('  ids that stored 800 or 800.0f32: [' + [...saw800].sort((a,b)=>a-b).join(',') + ']');
        console.log('  ids that stored 0/0.0f32       : [' + [...saw0].sort((a,b)=>a-b).slice(0,40).join(',') + (saw0.size>40?',...':'') + ']');
        // First in-buffer occurrence (k order = runtime store order) of 800 and of 0.
        let firstk800=-1, firstk0=-1;
        for (let k = 0; k < cap; k++) { const val = dv.getUint32(0x41010+k*16+12,true)>>>0;
            if (firstk800<0 && (val===800||val===F800)) firstk800=k; if (firstk0<0 && val===0) firstk0=k; }
        console.log('  first store of 800/800.0 at k=' + firstk800 + ' (id=' + (firstk800<0?'-':peek(0x41010+firstk800*16+4)) + '); first store of 0 at k=' + firstk0);
    } else {
        console.log('\n(no store trace — AZ_LOG_STORES not set or no in-window stores)');
    }
    process.exit(0);
})().catch(e => { console.error(e.stack); process.exit(1); });
