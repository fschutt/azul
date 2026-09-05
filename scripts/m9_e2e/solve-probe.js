// solve-probe.js (2026-08-13) — isolate whether solveLayoutReal's trap is caused by
// STUBBED __remill memory intrinsics. azul-mini.wasm IMPORTS __remill_read_memory_32,
// __remill_write_memory_32, __remill_compare_exchange_memory_64/8, __remill_atomic_begin/
// end (the transpiler did NOT inline them to native wasm loads/stores for some fns). The
// browser loader + full-cycle.js stub write_memory to a NO-OP (dropped writes) → stale
// reads → panic ("unreachable"). Here we provide REAL impls for ALL of them and re-run.
// Bootstrap mirrors layout-real.js. If solveLayoutReal now returns rc=0 → root cause is
// the dropped-write intrinsics (fix = inline them in the transpiler OR impl in loader_js.rs).
const http = require('http');
function fetch_(p){return new Promise((r,j)=>http.get('http://127.0.0.1:'+(process.env.AZ_PORT||8800)+p,x=>{const c=[];x.on('data',b=>c.push(b));x.on('end',()=>r(Buffer.concat(c)));x.on('error',j);}));}
function fail(m){console.error('FAIL:',m);process.exit(1);}
(async()=>{
  const html=(await fetch_('/')).toString();
  const initialCounter=parseInt((html.match(/<div id="az_1">(\d+)<\/div>/)||['','5'])[1]);
  const typeId=BigInt((html.match(/"type_id":"(\d+)"/)||['','0'])[1]);
  const miniUrl=html.match(/href="(\/az\/mini\.[^"]+)"/)[1];
  const layoutUrl=html.match(/href="(\/az\/layout\/[^"]+)"/)[1];
  const cbMatch=html.match(/href="(\/az\/cb\/[^"]+)"/);
  const [miniBytes,layoutBytes,cbBytes]=await Promise.all([fetch_(miniUrl),fetch_(layoutUrl),cbMatch?fetch_(cbMatch[1]):Promise.resolve(null)]);
  const table=new WebAssembly.Table({initial:64,element:'anyfunc'});
  let cbTableIdx=-1, memory=null;
  const memset=(d,v,n)=>{new Uint8Array(memory.buffer).fill(v&0xFF,Number(d),Number(d)+Number(n));return d;};
  const memcpy=(d,s,n)=>{new Uint8Array(memory.buffer).copyWithin(Number(d),Number(s),Number(s)+Number(n));return d;};
  // FULL, CORRECT __remill memory intrinsics (the point of this probe):
  const REMILL = {
    __remill_read_memory_8:  (m,a)=>new DataView(memory.buffer).getUint8(Number(a)),
    __remill_read_memory_16: (m,a)=>new DataView(memory.buffer).getUint16(Number(a),true),
    __remill_read_memory_32: (m,a)=>new DataView(memory.buffer).getUint32(Number(a),true),
    __remill_read_memory_64: (m,a)=>new DataView(memory.buffer).getBigUint64(Number(a),true),
    __remill_write_memory_8: (m,a,v)=>{new DataView(memory.buffer).setUint8(Number(a),Number(v)&0xFF);return m;},
    __remill_write_memory_16:(m,a,v)=>{new DataView(memory.buffer).setUint16(Number(a),Number(v)&0xFFFF,true);return m;},
    __remill_write_memory_32:(m,a,v)=>{new DataView(memory.buffer).setUint32(Number(a),v>>>0,true);return m;},
    __remill_write_memory_64:(m,a,v)=>{new DataView(memory.buffer).setBigUint64(Number(a),BigInt.asUintN(64,BigInt(v)),true);return m;},
    __remill_atomic_begin:(m)=>m, __remill_atomic_end:(m)=>m,
    __remill_barrier_load_load:(m)=>m,__remill_barrier_load_store:(m)=>m,__remill_barrier_store_load:(m)=>m,__remill_barrier_store_store:(m)=>m,
    __remill_compare_exchange_memory_64:(m,a,ep,d)=>{const dv=new DataView(memory.buffer);const A=Number(a),E=Number(ep);const act=dv.getBigUint64(A,true);if(act===dv.getBigUint64(E,true))dv.setBigUint64(A,BigInt.asUintN(64,BigInt(d)),true);dv.setBigUint64(E,act,true);return m;},
    __remill_compare_exchange_memory_32:(m,a,ep,d)=>{const dv=new DataView(memory.buffer);const A=Number(a),E=Number(ep);const act=dv.getUint32(A,true);if(act===dv.getUint32(E,true))dv.setUint32(A,d>>>0,true);dv.setUint32(E,act,true);return m;},
    __remill_compare_exchange_memory_8:(m,a,ep,d)=>{const u8=new Uint8Array(memory.buffer);const A=Number(a),E=Number(ep);const act=u8[A];if(act===u8[E])u8[A]=Number(d)&0xFF;u8[E]=act;return m;},
  };
  const AZ_MATH={fmaxf:(a,b)=>a!==a?b:(b!==b?a:Math.max(a,b)),fminf:(a,b)=>a!==a?b:(b!==b?a:Math.min(a,b)),fmax:(a,b)=>a!==a?b:(b!==b?a:Math.max(a,b)),fmin:(a,b)=>a!==a?b:(b!==b?a:Math.min(a,b)),roundf:x=>Math.sign(x)*Math.round(Math.abs(x)),round:x=>Math.sign(x)*Math.round(Math.abs(x)),fabsf:Math.abs,fabs:Math.abs,sqrtf:Math.sqrt,sqrt:Math.sqrt,floorf:Math.floor,floor:Math.floor,ceilf:Math.ceil,ceil:Math.ceil,truncf:Math.trunc,trunc:Math.trunc,powf:Math.pow,pow:Math.pow};
  const MASK=0xFFFFFFFFFFFFFFFFn;
  const az128=(op)=>(sret,aLo,aHi,bLo,bHi)=>{const dv=new DataView(memory.buffer);
    const a=(BigInt.asUintN(64,BigInt(aHi))<<64n)|BigInt.asUintN(64,BigInt(aLo));
    const b=(BigInt.asUintN(64,BigInt(bHi))<<64n)|BigInt.asUintN(64,BigInt(bLo));
    const r=op(a,b); dv.setBigUint64(Number(sret),r&MASK,true); dv.setBigUint64(Number(sret)+8,(r>>64n)&MASK,true);};
  const ARITH={ __multi3:az128((a,b)=>BigInt.asUintN(128,a*b)), __udivti3:az128((a,b)=>b===0n?0n:a/b),
    __umodti3:az128((a,b)=>b===0n?0n:a%b), __divti3:az128((a,b)=>b===0n?0n:BigInt.asUintN(128,BigInt.asIntN(128,a)/BigInt.asIntN(128,b))) };
  const realEnv=Object.assign({__indirect_function_table:table,memset,memcpy,memmove:memcpy,
    __az_resolve_callback:(addr)=>addr===0xFFFFFFFF?0xFFFFFFFF:(cbTableIdx>=0?cbTableIdx:0xFFFFFFFF)}, ARITH, REMILL, AZ_MATH);
  const seen=new Set();
  const stubFor=n=>{ if(/write_memory|barrier|exception_clear|atomic_end|atomic_begin/.test(n))return ()=>0; if(!seen.has(n)){console.log('[STUB0] '+n);seen.add(n);} return /_f64\b/.test(n)?()=>0:(/_64\b/.test(n)?()=>0n:()=>0); };
  const h=env=>({get:(_,p)=>typeof p==='string'?(Object.prototype.hasOwnProperty.call(env,p)?env[p]:stubFor(p)):undefined,has:()=>true});
  const {instance:miniI}=await WebAssembly.instantiate(miniBytes,{env:new Proxy({},h(realEnv))});
  const mini=miniI.exports; memory=mini.memory;
  const cbEnv=Object.assign({memory,__indirect_function_table:table,memset,memcpy,memmove:memcpy},ARITH,REMILL,AZ_MATH);
  if(cbBytes){const {instance:cbI}=await WebAssembly.instantiate(cbBytes,{env:new Proxy({},h(cbEnv))});table.grow(1);cbTableIdx=table.length-1;table.set(cbTableIdx,cbI.exports.callback);}
  const {instance:layoutI}=await WebAssembly.instantiate(layoutBytes,{env:new Proxy({},h(cbEnv))});
  table.grow(1);const layoutTableIdx=table.length-1;table.set(layoutTableIdx,layoutI.exports.callback);
  const state=mini.AzStartup_init(0,0);
  const modelPtr=mini.AzStartup_alloc(4);
  new DataView(memory.buffer).setUint32(modelPtr,initialCounter,true);
  const refanyPtr=mini.AzStartup_hydrate(Number(typeId&0xFFFFFFFFn),Number((typeId>>32n)&0xFFFFFFFFn),modelPtr,4);
  mini.AzStartup_setRefAny(state,refanyPtr);
  mini.AzStartup_setLayoutCbTableIdx(state,layoutTableIdx);
  mini.AzStartup_setModelPtr(state,modelPtr);
  mini.AzStartup_registerCbNode(state,0);
  mini.AzStartup_setDisplayNode(state,0);
  const W=800,H=600;
  if(mini.AzStartup_initLayoutCache(state,W,H,0)!==0)fail('initLayoutCache');
  if(mini.AzStartup_hydrateStyledDom(state)!==0)fail('hydrateStyledDom');
  console.log('[1] cascade ok: styled node_count='+mini.AzStartup_getStyledDomNodeCount(state));
  let rc;
  try { rc=mini.AzStartup_solveLayoutReal(state,W,H); }
  catch(e){ console.error('[2] solveLayoutReal STILL TRAPPED: '+e.message); console.error((e.stack||'').split('\n').slice(0,6).join('\n')); process.exit(2); }
  console.log('[2] solveLayoutReal RETURNED rc='+rc);
  const rectsLen=mini.AzStartup_getPositionedRectsLen(state);
  const rectsPtr=mini.AzStartup_getPositionedRectsPtr(state);
  console.log('[3] rects: ptr='+rectsPtr+' len='+rectsLen);
  if(rectsPtr && rectsLen>0){const dv=new DataView(memory.buffer);for(let i=0;i<rectsLen;i++){const o=rectsPtr+i*16;console.log('  rect['+i+']: x='+dv.getUint32(o,true)+' y='+dv.getUint32(o+4,true)+' w='+dv.getUint32(o+8,true)+' h='+dv.getUint32(o+12,true));}}
  console.log(rc===0?'\nPASS: solveLayoutReal returns with real __remill memory intrinsics':'\nrc!=0 but no trap');
  process.exit(rc===0?0:3);
})().catch(e=>{console.error('HARNESS-ERR',e.stack);process.exit(1);});
