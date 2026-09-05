// named-solve-probe.js (2026-08-13) — load the NAME-KEPT re-linked mini wasm from disk
// (/c/rb/named-mini.wasm, re-linked from the working 12976 scratch .o) and run
// solveLayoutReal so the V8 trap stack shows FUNCTION NAMES (the served wasm is stripped).
// Bootstrap mirrors full-cycle.js (incl. the critical AzStartup_resetBumpHeap(160MB) that
// seeds the bump heap — without it AzStartup_init's Box::new returns offset 0 → null state
// → init traps, which is why solve-probe.js/layout-real.js trapped at init). No server
// needed. Layout cb is stubbed with a mini export (solve traps early, before any layout cb).
const fs = require('fs');
const MINI = process.env.AZ_MINI_FILE || 'C:/rb/named-mini.wasm';
const TYPEID = BigInt(process.env.AZ_TYPEID || '447680');
const COUNTER = parseInt(process.env.AZ_COUNTER || '5');
(async () => {
  const miniBytes = fs.readFileSync(MINI);
  const table = new WebAssembly.Table({ initial: 64, element: 'anyfunc' });
  let memory = null;
  const num = x => (typeof x === 'bigint' ? Number(x) : x);
  const memset = (d,v,n)=>{ new Uint8Array(memory.buffer).fill(num(v)&0xFF, num(d), num(d)+num(n)); return d; };
  const memcpy = (d,s,n)=>{ new Uint8Array(memory.buffer).copyWithin(num(d), num(s), num(s)+num(n)); return d; };
  const MASK=0xFFFFFFFFFFFFFFFFn;
  const az128=(op)=>(sret,aLo,aHi,bLo,bHi)=>{const dv=new DataView(memory.buffer);
    const a=(BigInt.asUintN(64,BigInt(aHi))<<64n)|BigInt.asUintN(64,BigInt(aLo));
    const b=(BigInt.asUintN(64,BigInt(bHi))<<64n)|BigInt.asUintN(64,BigInt(bLo));
    const r=op(a,b); dv.setBigUint64(num(sret),r&MASK,true); dv.setBigUint64(num(sret)+8,(r>>64n)&MASK,true);};
  const REMILL = {
    __remill_read_memory_8:(m,a)=>new DataView(memory.buffer).getUint8(num(a)),
    __remill_read_memory_16:(m,a)=>new DataView(memory.buffer).getUint16(num(a),true),
    __remill_read_memory_32:(m,a)=>new DataView(memory.buffer).getUint32(num(a),true),
    __remill_read_memory_64:(m,a)=>new DataView(memory.buffer).getBigUint64(num(a),true),
    __remill_write_memory_8:(m,a,v)=>{new DataView(memory.buffer).setUint8(num(a),num(v)&0xFF);return m;},
    __remill_write_memory_16:(m,a,v)=>{new DataView(memory.buffer).setUint16(num(a),num(v)&0xFFFF,true);return m;},
    __remill_write_memory_32:(m,a,v)=>{new DataView(memory.buffer).setUint32(num(a),v>>>0,true);return m;},
    __remill_write_memory_64:(m,a,v)=>{new DataView(memory.buffer).setBigUint64(num(a),BigInt.asUintN(64,BigInt(v)),true);return m;},
    __remill_atomic_begin:(m)=>m,__remill_atomic_end:(m)=>m,
    __remill_barrier_load_load:(m)=>m,__remill_barrier_load_store:(m)=>m,__remill_barrier_store_load:(m)=>m,__remill_barrier_store_store:(m)=>m,
    __remill_compare_exchange_memory_64:(m,a,ep,d)=>{const dv=new DataView(memory.buffer),A=num(a),E=num(ep);const act=dv.getBigUint64(A,true);if(act===dv.getBigUint64(E,true))dv.setBigUint64(A,BigInt.asUintN(64,BigInt(d)),true);dv.setBigUint64(E,act,true);return m;},
    __remill_compare_exchange_memory_32:(m,a,ep,d)=>{const dv=new DataView(memory.buffer),A=num(a),E=num(ep);const act=dv.getUint32(A,true);if(act===dv.getUint32(E,true))dv.setUint32(A,d>>>0,true);dv.setUint32(E,act,true);return m;},
    __remill_compare_exchange_memory_8:(m,a,ep,d)=>{const u8=new Uint8Array(memory.buffer),A=num(a),E=num(ep);const act=u8[A];if(act===u8[E])u8[A]=num(d)&0xFF;u8[E]=act;return m;},
  };
  const ARITH={__multi3:az128((a,b)=>BigInt.asUintN(128,a*b)),__udivti3:az128((a,b)=>b===0n?0n:a/b),__umodti3:az128((a,b)=>b===0n?0n:a%b),__divti3:az128((a,b)=>b===0n?0n:BigInt.asUintN(128,BigInt.asIntN(128,a)/BigInt.asIntN(128,b)))};
  const AZ_MATH={fmaxf:(a,b)=>a!==a?b:(b!==b?a:Math.max(a,b)),fminf:(a,b)=>a!==a?b:(b!==b?a:Math.min(a,b)),fmax:(a,b)=>a!==a?b:(b!==b?a:Math.max(a,b)),fmin:(a,b)=>a!==a?b:(b!==b?a:Math.min(a,b)),roundf:x=>Math.sign(x)*Math.round(Math.abs(x)),round:x=>Math.sign(x)*Math.round(Math.abs(x)),fabsf:Math.abs,fabs:Math.abs,sqrtf:Math.sqrt,sqrt:Math.sqrt,floorf:Math.floor,floor:Math.floor,ceilf:Math.ceil,ceil:Math.ceil,truncf:Math.trunc,trunc:Math.trunc,powf:Math.pow,pow:Math.pow};
  let cbTableIdx=-1;
  const realEnv=Object.assign({__indirect_function_table:table,memset,memcpy,memmove:memcpy,
    __az_resolve_callback:(addr)=>num(addr)===0xFFFFFFFF?0xFFFFFFFF:(cbTableIdx>=0?cbTableIdx:0xFFFFFFFF)},ARITH,REMILL,AZ_MATH);
  const seen=new Set();
  const stubFor=n=>{ if(/write_memory|barrier|exception_clear|atomic_begin|atomic_end/.test(n))return ()=>0; if(!seen.has(n)){console.log('[STUB0] '+n);seen.add(n);} return /_f64\b/.test(n)?()=>0:(/_64\b/.test(n)?()=>0n:()=>0); };
  const h=env=>({get:(_,p)=>typeof p==='string'?(Object.prototype.hasOwnProperty.call(env,p)?env[p]:stubFor(p)):undefined,has:()=>true});
  const { instance } = await WebAssembly.instantiate(miniBytes, { env: new Proxy({}, h(realEnv)) });
  const mini = instance.exports; memory = mini.memory;
  // stub layout cb slot with a mini export (solve traps before invoking it)
  table.grow(1); const layoutTableIdx = table.length - 1; table.set(layoutTableIdx, mini.AzStartup_free || mini.AzStartup_alloc);
  if (typeof mini.AzStartup_resetBumpHeap === 'function') mini.AzStartup_resetBumpHeap(160*1024*1024);
  const state = mini.AzStartup_init(0,0);
  console.log('[0] init state=' + state);
  const modelPtr = mini.AzStartup_alloc(4);
  new DataView(memory.buffer).setUint32(modelPtr, COUNTER, true);
  const refanyPtr = mini.AzStartup_hydrate(Number(TYPEID & 0xFFFFFFFFn), Number((TYPEID>>32n)&0xFFFFFFFFn), modelPtr, 4);
  mini.AzStartup_setRefAny(state, refanyPtr);
  mini.AzStartup_setLayoutCbTableIdx(state, layoutTableIdx);
  mini.AzStartup_setModelPtr(state, modelPtr);
  mini.AzStartup_registerCbNode(state, 0);
  mini.AzStartup_setDisplayNode(state, 0);
  const W=800,H=600;
  const lc = mini.AzStartup_initLayoutCache(state, W, H, 0);
  console.log('[1] initLayoutCache rc=' + lc);
  let hy; try { hy = mini.AzStartup_hydrateStyledDom(state); console.log('[2] hydrateStyledDom rc=' + hy + ' nodes=' + (mini.AzStartup_getStyledDomNodeCount?mini.AzStartup_getStyledDomNodeCount(state):'?')); }
  catch(e){ console.error('[2] hydrate TRAPPED (NAMED):\n' + (e.stack||e.message)); process.exit(2); }
  try {
    const rc = mini.AzStartup_solveLayoutReal(state, W, H);
    console.log('[3] solveLayoutReal RETURNED rc=' + rc + ' rects=' + (mini.AzStartup_getPositionedRectsLen?mini.AzStartup_getPositionedRectsLen(state):'?'));
    console.log(rc===0 ? '\nPASS: solveLayoutReal returns' : '\nrc!=0');
  } catch(e) {
    console.error('[3] solveLayoutReal TRAPPED (NAMED STACK):\n' + (e.stack||e.message));
    process.exit(3);
  }
})().catch(e=>{console.error('HARNESS-ERR', e.stack); process.exit(1);});
