// M9-2 Layout-cb probe script.
//
// Mirrors /tmp/e2e.js's bootstrap, then drives the layout cb directly:
//   1. Fetch HTML, extract URLs + type_id + initial counter.
//   2. Instantiate mini.wasm + cb.wasm + layout.wasm sharing memory/table.
//   3. Hydrate the wasm-side RefAny.
//   4. Call AzStartup_buildLayoutInfo(800, 600, 0) → infoPtr.
//   5. Allocate a 256-byte destination buffer for the returned AzDom.
//   6. Call layout.callback(refany_lo, refany_hi, info_ptr, out_ptr).
//   7. Verify status == 0 and dump the first 64 bytes of out_ptr.
//
// Expected: status=0, out_ptr filled with non-zero bytes (Dom struct).
// A pure-zeros out_ptr usually means the cb trapped silently.
const http = require('http');
function fetch_(p) {
    return new Promise((res, rej) => {
        http.get('http://127.0.0.1:8800' + p, r => {
            const c = [];
            r.on('data', x => c.push(x));
            r.on('end', () => res(Buffer.concat(c)));
            r.on('error', rej);
        });
    });
}

(async () => {
    const html = (await fetch_('/')).toString();
    const counterMatch = html.match(/<div id="az_1">(\d+)<\/div>/);
    const initialCounter = counterMatch ? parseInt(counterMatch[1]) : 0;
    const typeId = BigInt((html.match(/"type_id":"(\d+)"/) || ['', '0'])[1]);
    const miniUrl = html.match(/href="(\/az\/mini\.[^"]+)"/)[1];
    const cbMatch = html.match(/href="(\/az\/cb\/[^"]+)"/);
    const cbUrl = cbMatch ? cbMatch[1] : null;
    const layoutUrl = html.match(/href="(\/az\/layout\/[^"]+)"/)[1];
    console.log('mini:', miniUrl);
    console.log('cb:', cbUrl);
    console.log('layout:', layoutUrl);

    const fetches = [fetch_(miniUrl), fetch_(layoutUrl)];
    if (cbUrl) fetches.push(fetch_(cbUrl));
    const fetched = await Promise.all(fetches);
    const miniBytes = fetched[0];
    const layoutBytes = fetched[1];
    const cbBytes = cbUrl ? fetched[2] : null;
    console.log('sizes: mini=' + miniBytes.length +
                ' cb=' + (cbBytes ? cbBytes.length : 'n/a') +
                ' layout=' + layoutBytes.length);

    const table = new WebAssembly.Table({ initial: 64, element: 'anyfunc' });
    const resolveCalls = [];
    // Real libc-ish stubs the lifted wasm needs (env imports show
    // memset/memcpy/memmove as missing). The wasm `env.memset(dest,
    // val, n)` signature expects dest returned; stubbing to () => 0
    // makes downstream pointer-arithmetic-on-return rely on 0 → trap
    // when subsequently dereferenced.
    function memsetImpl(dest, val, n) {
        const u8 = new Uint8Array(memory.buffer);
        for (let i = 0; i < n; i++) u8[dest + i] = val & 0xFF;
        return dest;
    }
    function memcpyImpl(dest, src, n) {
        const u8 = new Uint8Array(memory.buffer);
        u8.copyWithin(dest, src, src + n);
        return dest;
    }
    function memmoveImpl(dest, src, n) {
        const u8 = new Uint8Array(memory.buffer);
        u8.copyWithin(dest, src, src + n);
        return dest;
    }
    let memory = null;
    const realEnv = {
        __indirect_function_table: table,
        __az_resolve_callback: (fnAddr) => {
            resolveCalls.push(fnAddr);
            return 0xFFFFFFFF;
        },
        memset: (...a) => memsetImpl(...a),
        memcpy: (...a) => memcpyImpl(...a),
        memmove: (...a) => memmoveImpl(...a),
    };
    // Trace every import call so we can identify what the lifted code
    // is actually invoking right before a trap.
    let lastImport = null;
    const traceCalls = process.env.PROBE_TRACE === '1';
    const wrap = (name, fn) => (...args) => {
        lastImport = { name, args };
        if (traceCalls) console.log('imp:', name, '(', args.map(a => typeof a === 'bigint' ? a.toString() + 'n' : a).join(', '), ')');
        return fn(...args);
    };
    const AZ_MATH = { fmaxf:(a,b)=>a!==a?b:(b!==b?a:Math.max(a,b)), fminf:(a,b)=>a!==a?b:(b!==b?a:Math.min(a,b)), fmax:(a,b)=>a!==a?b:(b!==b?a:Math.max(a,b)), fmin:(a,b)=>a!==a?b:(b!==b?a:Math.min(a,b)), roundf:x=>Math.sign(x)*Math.round(Math.abs(x)), round:x=>Math.sign(x)*Math.round(Math.abs(x)), fabsf:Math.abs, fabs:Math.abs, sqrtf:Math.sqrt, sqrt:Math.sqrt, floorf:Math.floor, floor:Math.floor, ceilf:Math.ceil, ceil:Math.ceil, truncf:Math.trunc, trunc:Math.trunc, powf:Math.pow, pow:Math.pow };
        const stubFor = n => AZ_MATH[n] || (/write_memory|barrier|exception_clear/.test(n)
        ? wrap(n, () => {})
        : (/_f64\b/.test(n) ? wrap(n, () => 0) : (/_64\b/.test(n) ? wrap(n, () => 0n) : wrap(n, () => 0))));
    const h = env => ({
        get: (_, p) => {
            if (typeof p !== 'string') return undefined;
            if (Object.prototype.hasOwnProperty.call(env, p)) {
                const v = env[p];
                return typeof v === 'function' ? wrap(p, v) : v;
            }
            return stubFor(p);
        },
        has: () => true,
    });

    const { instance: miniI } = await WebAssembly.instantiate(miniBytes, {
        env: new Proxy({}, h(realEnv)),
    });
    const mini = miniI.exports;
    memory = mini.memory;
    const pages = memory.buffer.byteLength / 65536;
    console.log('mini memory: ' + pages + ' pages (' +
                (pages * 64 / 1024).toFixed(0) + ' MiB)');
    mini.AzStartup_init(0, 0);

    const cbEnv = {
        memory, __indirect_function_table: table,
        memset: memsetImpl, memcpy: memcpyImpl, memmove: memmoveImpl,
    };
    if (cbBytes) {
        const { instance: cbI } = await WebAssembly.instantiate(cbBytes, {
            env: new Proxy({}, h(cbEnv)),
        });
        table.grow(1);
        table.set(table.length - 1, cbI.exports.callback);
    }

    // M9-2: instantiate the layout wasm with the SAME env wiring as cb wasms.
    const { instance: layoutI } = await WebAssembly.instantiate(layoutBytes, {
        env: new Proxy({}, h(cbEnv)),
    });
    const layoutCb = layoutI.exports.callback;
    if (typeof layoutCb !== 'function') {
        console.error('FAIL: layout wasm has no `callback` export');
        process.exit(1);
    }
    console.log('layout cb signature OK');

    // Hydrate the wasm-side RefAny.
    const modelPtr = mini.AzStartup_alloc(4);
    new DataView(memory.buffer).setUint32(modelPtr, initialCounter, true);
    const refanyPtr = mini.AzStartup_hydrate(
        Number(typeId & 0xFFFFFFFFn),
        Number((typeId >> 32n) & 0xFFFFFFFFn),
        modelPtr,
        4,
    );
    console.log('hydrate ok: refany=' + refanyPtr + ' model=' + modelPtr +
                ' counter=' + initialCounter);

    // M9-3 flow: register the layout cb in the shared table, tell mini
    // about its table_idx + the hydrated refany, then call initLayoutCache.
    // Phase 3 verifies the wasm-resident layout pipeline works without
    // JS having to construct LayoutCallbackInfo or pass the cb directly.
    table.grow(1);
    const layoutTableIdx = table.length - 1;
    table.set(layoutTableIdx, layoutCb);
    console.log('layout cb at table[' + layoutTableIdx + ']');

    // Get the state pointer (returned by AzStartup_init at the top of
    // this script). Currently 0 because we didn't capture it — fix:
    let stateForInit = 0;  // mini.AzStartup_init() returned a ptr; capture
    // (Above we did `mini.AzStartup_init(0, 0)` without capture; redo.)
    // Re-init to get a fresh state ptr. Subsequent calls just consume
    // the new state.
    stateForInit = mini.AzStartup_init(0, 0);
    console.log('state ptr:', stateForInit);

    if (typeof mini.AzStartup_setLayoutCbTableIdx === 'function') {
        mini.AzStartup_setLayoutCbTableIdx(stateForInit, layoutTableIdx);
    } else {
        console.error('FAIL: mini.AzStartup_setLayoutCbTableIdx not exported');
        process.exit(1);
    }
    if (typeof mini.AzStartup_setRefAny === 'function') {
        mini.AzStartup_setRefAny(stateForInit, refanyPtr);
    } else {
        console.error('FAIL: mini.AzStartup_setRefAny not exported');
        process.exit(1);
    }

    // Sanity check: directly invoke the layoutCb (bypassing mini's
    // __az_call_indirect_layout4 bridge). If this works but
    // initLayoutCache traps, the trap is in the bridge.
    try {
        const directInfo = mini.AzStartup_alloc(512);
        const directOut = mini.AzStartup_alloc(256);
        const directRc = layoutCb(BigInt(refanyPtr), 0n, directInfo, directOut);
        console.log('DIRECT layoutCb rc=' + directRc + ' (info=' + directInfo + ' out=' + directOut + ')');
        const u8d = new Uint8Array(memory.buffer, directOut, 64);
        const anyD = Array.from(u8d).some(b => b !== 0);
        console.log('direct out has data?', anyD);
    } catch (e) {
        console.error('DIRECT layoutCb trap:', e.message);
    }

    // initLayoutCache: builds LayoutCallbackInfo internally, allocates
    // dest buffer, calls layout cb via __az_call_indirect_layout4.
    let initRc;
    let initTrapped = false;
    try {
        initRc = mini.AzStartup_initLayoutCache(stateForInit, 800, 600, 0);
    } catch (e) {
        initTrapped = true;
        console.error('initLayoutCache TRAP:', e.message);
        console.error('STACK:', e.stack);
        console.error('LAST IMPORT BEFORE TRAP:', lastImport);
    }
    if (!initTrapped) {
        console.log('initLayoutCache rc=' + initRc);
    }

    const lastStatus = (typeof mini.AzStartup_getLastLayoutStatus === 'function')
        ? mini.AzStartup_getLastLayoutStatus(stateForInit) : 'n/a';
    const domPtr = (typeof mini.AzStartup_getCurrentDomPtr === 'function')
        ? mini.AzStartup_getCurrentDomPtr(stateForInit) : 0;
    console.log('current_dom_ptr=' + domPtr + ' last_layout_status=' + lastStatus);

    // Dump bytes at current_dom_ptr (where the layout cb wrote its
    // returned AzDom).
    const dv = new DataView(memory.buffer);
    if (domPtr) {
        const after = [];
        for (let i = 0; i < 64; i += 8) {
            after.push(dv.getBigUint64(domPtr + i, true).toString(16));
        }
        console.log('current_dom[0..64]:', after.join(' '));
    }
    console.log('resolve_callback was called ' + resolveCalls.length + ' times');

    // PASS criterion: initLayoutCache returned 0 AND current_dom_ptr
    // is non-zero AND at least one u64 at that pointer is non-zero.
    let pass = false;
    if (!initTrapped && initRc === 0 && domPtr !== 0) {
        const u8 = new Uint8Array(memory.buffer, domPtr, 64);
        for (let i = 0; i < u8.length; i++) {
            if (u8[i] !== 0) { pass = true; break; }
        }
    }
    if (pass) {
        console.log('PROBE PASS: initLayoutCache=0, current_dom populated');
        process.exit(0);
    } else {
        console.log('PROBE FAIL: trapped=' + initTrapped + ' rc=' + initRc + ' domPtr=' + domPtr);
        process.exit(1);
    }
})().catch(e => { console.error(e.stack); process.exit(1); });
