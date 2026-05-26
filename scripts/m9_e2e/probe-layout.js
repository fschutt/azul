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
    const stubFor = n => /write_memory|barrier|exception_clear/.test(n) ? () => {}
        : (/_f64\b/.test(n) ? () => 0 : (/_64\b/.test(n) ? () => 0n : () => 0));
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
      console.log('  __remill_missing_block count=' + mcnt + ' lastPC=' + hx(0x400F8) + (mpc? '  → native '+nat(mpc):'')); }

    const rptr = mini.AzStartup_getPositionedRectsPtr(state), rlen = mini.AzStartup_getPositionedRectsLen(state);
    console.log('\n=== RECTS (ptr=' + rptr + ' len=' + rlen + ') ===');
    for (let i = 0; i < rlen; i++) {
        const o = rptr + i*16;
        console.log('  rect[' + i + ']: x=' + dv.getUint32(o,true) + ' y=' + dv.getUint32(o+4,true) + ' w=' + dv.getUint32(o+8,true) + ' h=' + dv.getUint32(o+12,true));
    }
    process.exit(0);
})().catch(e => { console.error(e.stack); process.exit(1); });
