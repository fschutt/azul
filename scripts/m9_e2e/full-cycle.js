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
        // __remill_* intrinsics the lifted SOLVE needs — the probe previously STUBBED
        // these to 0/no-op, so a CAS-retry loop in the solve never succeeded → spin-loop
        // HANG (no [2d]). Provide real impls (mem = threaded Memory token, returned as-is).
        __remill_read_memory_32: (mem, addr) => new DataView(memory.buffer).getUint32(Number(addr), true),
        __remill_atomic_begin: (mem) => mem,
        __remill_atomic_end: (mem) => mem,
        __remill_compare_exchange_memory_64: (mem, addr, expPtr, desired) => {
            const dv = new DataView(memory.buffer), a = Number(addr), e = Number(expPtr);
            const actual = dv.getBigUint64(a, true);
            if (actual === dv.getBigUint64(e, true)) dv.setBigUint64(a, BigInt.asUintN(64, BigInt(desired)), true);
            dv.setBigUint64(e, actual, true); return mem;
        },
        __remill_compare_exchange_memory_8: (mem, addr, expPtr, desired) => {
            const u8 = new Uint8Array(memory.buffer), a = Number(addr), e = Number(expPtr);
            const actual = u8[a];
            if (actual === u8[e]) u8[a] = Number(desired) & 0xFF;
            u8[e] = actual; return mem;
        },
    };
    const AZ_MATH = { fmaxf:(a,b)=>a!==a?b:(b!==b?a:Math.max(a,b)), fminf:(a,b)=>a!==a?b:(b!==b?a:Math.min(a,b)), fmax:(a,b)=>a!==a?b:(b!==b?a:Math.max(a,b)), fmin:(a,b)=>a!==a?b:(b!==b?a:Math.min(a,b)), roundf:x=>Math.sign(x)*Math.round(Math.abs(x)), round:x=>Math.sign(x)*Math.round(Math.abs(x)), fabsf:Math.abs, fabs:Math.abs, sqrtf:Math.sqrt, sqrt:Math.sqrt, floorf:Math.floor, floor:Math.floor, ceilf:Math.ceil, ceil:Math.ceil, truncf:Math.trunc, trunc:Math.trunc, powf:Math.pow, pow:Math.pow };
        const loggedStubs = new Set();
        const stubFor = n => {
            if (AZ_MATH[n]) return AZ_MATH[n];
            if (/write_memory|barrier|exception_clear/.test(n)) return () => {};
            if (!loggedStubs.has(n)) { console.log('[STUB-0] ' + n); loggedStubs.add(n); }
            return /_f64\b/.test(n) ? () => 0 : (/_64\b/.test(n) ? () => 0n : () => 0);
        };
    const h = env => ({
        get: (_, p) => typeof p === 'string'
            ? (Object.prototype.hasOwnProperty.call(env, p) ? env[p] : stubFor(p))
            : undefined,
        has: () => true,
    });

    // [AZ_PATCH_STACK=<bytes>] DIAGNOSTIC: patch mini's stack-pointer global[0]
    // i32.const init to a larger value. The transpiler inits mini's SP at 192 KiB
    // (STACK_BASE_FIRST), but each lifted wrapper frame is 128 KiB (STACK_BUF_SIZE),
    // so 2 nested wrappers (hydrate→AzStartup_alloc) overflow → the State-init store
    // traps OOB. If raising the SP makes hydrate succeed, the fix = bigger wasm stack.
    function patchStackPointer(bytes, newSP) {
        bytes = Buffer.from(bytes); let p = 8;
        const rLEB = () => { let r=0,s=0,b; do{b=bytes[p++];r|=(b&0x7f)<<s;s+=7;}while(b&0x80); return r>>>0; };
        const uLEB = (v) => { const a=[]; do{let b=v&0x7f; v>>>=7; if(v)b|=0x80; a.push(b);}while(v); return Buffer.from(a); };
        const sLEB = (v) => { const a=[]; let more=true; while(more){let b=v&0x7f; v>>=7; if((v===0&&!(b&0x40))||(v===-1&&(b&0x40)))more=false; else b|=0x80; a.push(b&0xff);} return Buffer.from(a); };
        while (p < bytes.length) {
            const id = bytes[p++]; const sizeStart = p; const size = rLEB(); const cs = p; p = cs + size;
            if (id === 6) {
                let q = cs; const cR = () => { let r=0,s=0,b; do{b=bytes[q++];r|=(b&0x7f)<<s;s+=7;}while(b&0x80); return r>>>0; };
                cR(); q += 2; const op = bytes[q++]; const vs = q; const oldV = cR(); const ve = q;
                const nv = sLEB(newSP); const newSize = size - (ve - vs) + nv.length;
                console.log('[patch-sp] global[0] SP init ' + oldV + ' -> ' + newSP + ' (op=0x' + op.toString(16) + ')');
                return Buffer.concat([bytes.slice(0, sizeStart), uLEB(newSize), bytes.slice(cs, vs), nv, bytes.slice(ve, cs + size), bytes.slice(cs + size)]);
            }
        }
        return bytes;
    }
    let mb = miniBytes;
    if (process.env.AZ_PATCH_STACK) mb = patchStackPointer(miniBytes, parseInt(process.env.AZ_PATCH_STACK));
    const { instance: miniI } = await WebAssembly.instantiate(mb, {
        env: new Proxy({}, h(realEnv)),
    });
    const mini = miniI.exports;
    memory = mini.memory;
    const cbEnv = { ...realEnv, memory };  // layout.wasm needs the same __remill_* impls (read_memory_32/CAS/atomic)

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

    // [AZ_DUMP_REGTRACE] dump the reg-trace ring right after [2] (Button::dom ran
    // inside initLayoutCache here, BEFORE the hydrate that later crashes). Show
    // the SP (id 99) progression — if SP drifts (doesn't return to its entry
    // value after the call sequence), the sret-ptr reload reads the wrong slot.
    if (process.env.AZ_DUMP_REGTRACE) {
        const dvr = new DataView(memory.buffer);
        const CNT = dvr.getUint32(983024, true), base = 983040, N = Math.min(CNT, 8191);
        const nm = id => ({99:'SP',0:'RAX',50:'RCX',52:'RDX',53:'R8',54:'RBX',55:'R12',56:'R14',57:'R13',58:'RBP',59:'RSI',61:'R15',70:'RDI'}[id] || ('r'+id));
        console.log('[regtrace@2] count=' + CNT + ' (showing SP=id99 stores)');
        let first=null, last=null, n=0;
        for (let k = 0; k < N; k++) {
            const id = dvr.getUint32(base+k*8, true), v = dvr.getUint32(base+k*8+4, true);
            if (id === 99) { if(first===null)first=v; last=v; if(n<40)console.log('  ['+k+'] SP=0x'+v.toString(16)); n++; }
        }
        console.log('[regtrace@2] SP stores=' + n + ' first=0x' + (first||0).toString(16) + ' last=0x' + (last||0).toString(16) + ' net_drift=' + ((last||0)-(first||0)));
    }

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
            // [AZ_DUMP_DOM] Dump the root Dom blob the layout cb wrote via sret,
            // as 8-byte words, so we can spot the garbage children-DomVec
            // {ptr@0,len@8,cap@16} (heap ptrs are ~0xa0xxxxx). Dom = { root:
            // NodeData, children: DomVec, css: CssVec, est: usize }.
            if (process.env.AZ_DUMP_DOM === '1') {
                const dv = new DataView(memory.buffer);
                const memSize = memory.buffer.byteLength;
                // [PROBE 0x40800] hello-world-probe2.exe captured the REAL AzButton
                // input bytes (256B) before AzButton_dom — dump as hex for the harness.
                const ibytes = new Uint8Array(memory.buffer, 0x40800, 256);
                const ihex = [...ibytes].map(b => b.toString(16).padStart(2, '0')).join('');
                if (!/^0+$/.test(ihex)) console.log('[BTNINPUT] ' + ihex);
                // children DomVec is at offset size_of::<NodeData>(); the dump
                // showed a heap ptr at +152, so CHILDREN_OFF=152; Dom = NodeData
                // (152) + DomVec(32) + CssVec(32) + usize(8) = 224.
                const CHILDREN_OFF = +(process.env.AZ_CHILD_OFF || 152);
                const DOM_SIZE = +(process.env.AZ_DOM_SIZE || 224);
                const HEAP_LO = 0x6000000;
                const bad = [];
                function walk(node, depth, path) {
                    if (depth > 12) return;
                    const disc = dv.getUint32(node, true);
                    const cptr = dv.getUint32(node + CHILDREN_OFF, true);
                    const clen = dv.getUint32(node + CHILDREN_OFF + 8, true);
                    const ccap = dv.getUint32(node + CHILDREN_OFF + 16, true);
                    // Empty Vec (len==0) is VALID regardless of ptr (Rust dangling
                    // ptr = align). Only a non-empty Vec with an out-of-heap ptr or
                    // an absurd len is garbage. count_az_dom_nodes recurses ONLY
                    // when ptr!=0 && len>0, so that's the exact OOB condition.
                    const willRecurse = cptr !== 0 && clen !== 0;
                    const ptrBad = willRecurse && (cptr < HEAP_LO || cptr + clen * DOM_SIZE + 24 > memSize);
                    const lenBad = clen > 100000;
                    const flag = (ptrBad || lenBad) ? '  <<< GARBAGE (will OOB)' : '';
                    console.log('  '.repeat(depth) + '[' + path + '] @0x' + node.toString(16) + ' disc=' + disc +
                        ' children{ptr=0x' + cptr.toString(16) + ' len=' + clen + ' cap=' + ccap + '}' + flag);
                    if (flag) { bad.push({ node, cptr, clen, path }); return; }
                    if (!willRecurse) return;
                    for (let i = 0; i < Math.min(clen, 16); i++) walk(cptr + i * DOM_SIZE, depth + 1, path + '.' + i);
                }
                console.log('[dom] walk from root @0x' + domPtr.toString(16) + ' (CHILD_OFF=' + CHILDREN_OFF + ' DOM_SIZE=' + DOM_SIZE + '):');
                walk(domPtr, 0, 'root');
                bad.forEach(b => {
                    console.log('[dom] GARBAGE node @0x' + b.node.toString(16) + ' path=' + b.path + ' children.ptr=0x' + b.cptr.toString(16) + ' len=' + b.clen);
                    // What does the garbage children.len/.cap point AT? (identify the overflow/shift source)
                    for (const lbl of ['len', 'cap']) {
                        const addr = Number(lbl === 'len' ? b.clen : dv.getBigUint64(b.node + CHILDREN_OFF + 16, true));
                        if (addr > 0x6000000 && addr + 48 < memSize) {
                            const w = []; for (let o = 0; o < 48; o += 8) w.push('0x' + dv.getBigUint64(addr + o, true).toString(16));
                            console.log('  [' + lbl + ' points @0x' + addr.toString(16) + ']: ' + w.join(' '));
                        }
                    }
                });
                // Full word dump of the two body children (valid div vs garbage
                // button) — same DOM_SIZE-byte Dom struct, side by side, to see
                // if the button slot is all-zero (missing write) or shifted.
                const c0 = dv.getUint32(domPtr + CHILDREN_OFF, true); // body.children.ptr
                const dumpNode = (base, tag) => {
                    const ws = [];
                    for (let o = 0; o < DOM_SIZE; o += 8) {
                        const lo = dv.getUint32(base + o, true), hi = dv.getUint32(base + o + 4, true);
                        ws.push('+' + String(o).padStart(3) + ':0x' + hi.toString(16).padStart(8, '0') + lo.toString(16).padStart(8, '0'));
                    }
                    console.log('[dom] ' + tag + ' @0x' + base.toString(16) + ':\n  ' + ws.join('\n  '));
                };
                if (c0) {
                    const divNode = c0, btnNode = c0 + DOM_SIZE;
                    const divTxt = dv.getUint32(divNode + CHILDREN_OFF, true);   // root.0.0 valid text "5"
                    const btnTxt = dv.getUint32(btnNode + CHILDREN_OFF, true);   // root.1.0 garbage button text
                    dumpNode(divTxt, 'root.0.0 VALID text child @0x' + divTxt.toString(16));
                    dumpNode(btnTxt, 'root.1.0 GARBAGE button text child @0x' + btnTxt.toString(16));
                    // Compare the style:Css vec @+88 of valid div-text vs button-text.
                    [[divTxt,'div'],[btnTxt,'btn']].forEach(([n,t]) => {
                        const p = dv.getUint32(n + 88, true), l = dv.getUint32(n + 96, true), c = dv.getUint32(n + 104, true);
                        let body = '';
                        if (p > 0x100000 && p + 64 < memSize) { const w=[]; for(let o=0;o<64;o+=8) w.push('0x'+dv.getBigUint64(p+o,true).toString(16)); body=' data['+w.join(' ')+']'; }
                        console.log('[styvec ' + t + '] @+88 {ptr=0x' + p.toString(16) + ' len=' + l + ' cap=' + c + '}' + body);
                    });
                    // [AZ_FIX_PADDING=1] DIAGNOSTIC: zero the node_type field's upper 7
                    // bytes (keep byte0 = the u8 disc) on every node. If hydrate then
                    // SUCCEEDS, the heap-ptr padding in the button-text node_type is the
                    // OOB trigger → the lifted cascade reads node_type wider than u8.
                    // [AZ_FIX_STYLE=1] zero the button-text's style:Css stylesheets vec
                    // (+88 ptr/+96 len/+104 cap) → empty Css. If hydrate then SUCCEEDS, the
                    // cascade's style-application on the button-text's len=4 style is the OOB.
                    if (process.env.AZ_FIX_STYLE === '1' && btnTxt) {
                        const u8 = new Uint8Array(memory.buffer);
                        for (let k = 88; k < 112; k++) u8[btnTxt + k] = 0;
                        console.log('[fixstyle] zeroed btnTxt.style vec @+88..112 → empty Css');
                    }
                    // [AZ_FIX_STYLE_ALL=1] same, but for every node in the tree.
                    if (process.env.AZ_FIX_STYLE_ALL === '1') {
                        const u8 = new Uint8Array(memory.buffer);
                        [domPtr, divNode, btnNode, divTxt, btnTxt].forEach(n => { if (n) for (let k = 88; k < 112; k++) u8[n + k] = 0; });
                        console.log('[fixstyle-all] zeroed style vec on all 5 nodes');
                    }
                    if (process.env.AZ_FIX_PADDING === '1') {
                        const u8 = new Uint8Array(memory.buffer);
                        const zpad = (base, tag) => {
                            if (!base) return;
                            const disc = u8[base];
                            for (let k = 1; k < 8; k++) u8[base + k] = 0;
                            console.log('[fixpad] ' + tag + ' @0x' + base.toString(16) + ' disc=0x' + disc.toString(16) + ' upper7 zeroed');
                        };
                        zpad(domPtr, 'root'); zpad(divNode, 'root.0'); zpad(btnNode, 'root.1');
                        zpad(divTxt, 'root.0.0'); zpad(btnTxt, 'root.1.0');
                    }
                }
            }
            try {
                const hRc = mini.AzStartup_hydrateStyledDom(state);
                const nodeCount = (typeof mini.AzStartup_getDomNodeCount === 'function')
                    ? mini.AzStartup_getDomNodeCount(state) : -1;
                console.log('[2c] hydrateStyledDom rc=' + hRc + ' node_count=' + nodeCount);
            } catch (e) {
                const dv = new DataView(memory.buffer);
                const bumpAfter = dv.getUint32(BUMP_OFF, true);
                // count_az_dom_nodes's built-in probe: 0x406E4 = root node_type
                // disc, 0x406E8 = first child node_type disc (written at count==1
                // before the OOB). Sane discs = the structure is partially valid.
                const rootDisc = dv.getUint32(0x406E4, true), childDisc = dv.getUint32(0x406E8, true);
                const hx = (a) => '0x' + dv.getUint32(a, true).toString(16);
                console.log('[2c-markers] bodychildlen=' + hx(0x40620) + ' body.rules=' + hx(0x40624) +
                    ' child0.rules=' + hx(0x40628) + ' child0.childlen=' + hx(0x4062c) + ' precascade=' + hx(0x40634));
                console.log('[2c-stack] PRIMARY hydrate trap:\n' + (e.stack || e));
                console.log('[2c] hydrateStyledDom TRAPPED: ' + e.message +
                    ' — bump 0x' + bumpBefore.toString(16) + ' → 0x' + bumpAfter.toString(16) +
                    (bumpAfter !== bumpBefore ? ' (advanced)' : '') +
                    ' | probe rootDisc=' + rootDisc + ' firstChildDisc=' + childDisc);
                new DataView(memory.buffer).setUint32(BUMP_OFF, bumpBefore, true);
            }
            // [2d solver] After hydrate, run the real layout solver (the browser
            // path) + report positioned-rects count — the cache geometric hit-test
            // reads. Exercises the full post-hydrate path end-to-end.
            try {
                const solveFn = mini.AzStartup_solveLayoutReal || mini.AzStartup_solveLayout;
                if (typeof solveFn === 'function') {
                    const solveRc = solveFn(state, 800, 600);
                    const solved = (typeof mini.AzStartup_isLayoutSolved === 'function') ? mini.AzStartup_isLayoutSolved(state) : -1;
                    const rectsLen = (typeof mini.AzStartup_getPositionedRectsLen === 'function') ? mini.AzStartup_getPositionedRectsLen(state) : -1;
                    console.log('[2d] solveLayout rc=' + solveRc + ' solved=' + solved + ' rects_len=' + rectsLen);
                    const dv3 = new DataView(memory.buffer); const u3 = (a) => dv3.getUint32(a, true);
                    console.log('[2d-sse-ok] sse1movemask(0x' + u3(0x40718).toString(16) + ' want ffff) pcmpeqb(0x' + u3(0x4071C).toString(16) + ' want 15) set1(0x' + u3(0x40720).toString(16) + ' want ffff) memset+movdqu(0x' + u3(0x40724).toString(16) + ' want ffff) memsetByte(0x' + u3(0x40728).toString(16) + ' want ff) HM(' + u3(0x4072C) + ' want 2)');
                    console.log('[2d-solvemarkers] css(0x' + u3(0x40578).toString(16) + ') fontParse(0x' + u3(0x40670).toString(16) + ') resolveChain(0x' + u3(0x40690).toString(16) + ')');
                } else { console.log('[2d] solveLayout: no solver export'); }
            } catch (e) {
                const dv2 = new DataView(memory.buffer);
                const m = (a) => '0x' + dv2.getUint32(a, true).toString(16);
                // eventloop.rs solver markers: css cache, font-parse pre-with_memory_fonts,
                // font-parse post-with_memory_fonts (set only if WMF didn't trap), resolve chain.
                console.log('[2d-markers] css(40578)=' + m(0x40578) + ' fontParsePre(40670)=' + m(0x40670) +
                    ' fontParsePostWMF(40650)=' + m(0x40650) + ' resolveChain(40690)=' + m(0x40690));
                console.log('[2d-p0] HashMap<String,u32> step(40870)=' + m(0x40870) + ' len(40874)=' + dv2.getUint32(0x40874, true) + ' get(40878)=' + dv2.getUint32(0x40878, true) +
                    '  [A0A00001=insert TRAPPED (minimal real-HashMap repro); A0A00002=insert OK len?=2; A0A00003=get OK get?=1]');
                console.log('[2d-isolation] step(406b0)=' + m(0x406b0) + ' probeCss(406b4)=' + dv2.getUint32(0x406b4, true) +
                    ' probeUni(406b8)=' + dv2.getUint32(0x406b8, true) + '  [B0B00001=probe TRAPPED→expand/fuzzy; B0B00002=probe OK→trap is find_unicode_fallbacks]');
                console.log('[2d-probe2] step(406bc)=' + m(0x406bc) + ' nIds(406c0)=' + dv2.getUint32(0x406c0, true) +
                    ' metaOk(406c4)=' + dv2.getUint32(0x406c4, true) + ' querySome(406c8)=' + dv2.getUint32(0x406c8, true) +
                    '  [C0C00001=get_metadata TRAPPED; C0C00002=query_internal TRAPPED; C0C00003=both OK→outer sort]');
                console.log('[2d-probe3] step(406cc)=' + m(0x406cc) + ' urLen(406d0)=' + dv2.getUint32(0x406d0, true) +
                    ' urSum(406d4)=' + dv2.getUint32(0x406d4, true) + '  [D0D00001=.len() TRAPPED; D0D00002=iterate TRAPPED; D0D00003=unicode_ranges OK]');
                console.log('[2d-probe4] step(406d8)=' + m(0x406d8) + ' fmsLen(406dc)=' + dv2.getUint32(0x406dc, true) +
                    ' fmsSum(406e0)=' + dv2.getUint32(0x406e0, true) + '  [E0E00001=collect TRAPPED; E0E00002=iter-elem TRAPPED; E0E00003=Vec<FontMatch> collect OK]');
                console.log('[2d-probe5] step(406e4)=' + m(0x406e4) + ' fmsLen(406e8)=' + dv2.getUint32(0x406e8, true) +
                    ' fmsSum(406ec)=' + dv2.getUint32(0x406ec, true) + '  [F0F00001=in_place collect TRAPPED; F0F00002=iter-elem TRAPPED; F0F00003=OK]');
                console.log('[2d-probe7] step(406f0)=' + m(0x406f0) + ' uniLen(406f4)=' + dv2.getUint32(0x406f4, true) +
                    '  [F7F70001=empty-fam resolve TRAPPED→query_internal/coverage; F7F70002=PASSED→bug is existing_prefixes/sort]');
                console.log('[2d-fuf] find_unicode marker(40830)=' + m(0x40830) +
                    '  [FB000002=before query_internal(trap=query_internal); FB000003=after qi; FB000005=before coverage(trap=coverage loop); FB000006=done]');
                console.log('[2d-cov] step(40838)=' + m(0x40838) + ' candN(4083C)=' + dv2.getUint32(0x4083C, true) +
                    ' uncovN(40840)=' + dv2.getUint32(0x40840, true) + ' candUrLen(40844)=' + dv2.getUint32(0x40844, true) +
                    ' candUrPtr(40848)=' + m(0x40848) + ' candFbLen(4084C)=' + dv2.getUint32(0x4084C, true) + ' candFbPtr(40858)=' + m(0x40858) +
                    '  [step=(ci<<8)|sub: FF0001=done; candFbLen/Ptr GARBAGE (not 0/small)→dropped fallbacks-init store=the bug; chain.clone() then OOBs]');
                console.log('[2d-post] step(40850)=' + m(0x40850) + ' chainUFLen(40854)=' + dv2.getUint32(0x40854, true) + ' chainCSSLen(4085C)=' + dv2.getUint32(0x4085C, true) +
                    '  [CC000001=find_unicode ret; 04=before chain build; CC000010=uncached ret; 11=before chain.clone(); 13=clone OK before cache.insert; 12=insert done → 11=clone traps, 13=insert traps]');
                console.log('[2d-p8] step(40860)=' + m(0x40860) + ' clonedLen(40864)=' + dv2.getUint32(0x40864, true) + ' sum(40868)=' + dv2.getUint32(0x40868, true) +
                    '  [C8C80001=Vec<FontMatch>::clone TRAPPED (minimal repro); C8C80002=iter-clone TRAPPED; C8C80003=OK]');
                const u = (a) => dv2.getUint32(a, true);
                console.log('[2d-hashmap] sanity BTm(' + u(0x40700) + ',' + u(0x40704) + '=2,161) BTmStr(' + u(0x40708) + ',' + u(0x4070C) + '=2,20) Vec(' + u(0x40710) + ',' + u(0x40714) + '=2,5)');
                console.log('[2d-sse] sse1movemask(0x' + u(0x40718).toString(16) + ' want ffff) pcmpeqb(0x' + u(0x4071C).toString(16) + ' want 15) set1(0x' + u(0x40720).toString(16) + ' want ffff) memset+movdqu(0x' + u(0x40724).toString(16) + ' want ffff) memsetByte(0x' + u(0x40728).toString(16) + ' want ff) HM(' + u(0x4072C) + ' want 2)  [0xdead000N=trapped before]');
                console.log('[2d] solveLayout TRAPPED: ' + (e.stack || e.message));
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
