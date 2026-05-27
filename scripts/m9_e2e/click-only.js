const http = require("http");
async function fetch_(p) { return new Promise((res, rej) => {
    http.get("http://127.0.0.1:8800" + p, r => {
        const c = [];
        r.on("data", x => c.push(x));
        r.on("end", () => res(Buffer.concat(c)));
        r.on("error", rej);
    });
}); }
(async () => {
    const html = (await fetch_("/")).toString();
    const initialCounter = parseInt(html.match(/<div id="az_1">(\d+)<\/div>/)[1]);
    const typeId = BigInt(html.match(/"type_id":"(\d+)"/)[1]);
    const miniUrl = html.match(/href="(\/az\/mini\.[^"]+)"/)[1];
    const cbUrl = html.match(/href="(\/az\/cb\/[^"]+)"/)[1];
    const miniBytes = await fetch_(miniUrl);
    const cbBytes = await fetch_(cbUrl);
    const table = new WebAssembly.Table({ initial: 64, element: "anyfunc" });
    const realEnv = { __indirect_function_table: table, __az_resolve_callback: () => 0xFFFFFFFF };
    const AZ_MATH = { fmaxf:(a,b)=>a!==a?b:(b!==b?a:Math.max(a,b)), fminf:(a,b)=>a!==a?b:(b!==b?a:Math.min(a,b)), fmax:(a,b)=>a!==a?b:(b!==b?a:Math.max(a,b)), fmin:(a,b)=>a!==a?b:(b!==b?a:Math.min(a,b)), roundf:x=>Math.sign(x)*Math.round(Math.abs(x)), round:x=>Math.sign(x)*Math.round(Math.abs(x)), fabsf:Math.abs, fabs:Math.abs, sqrtf:Math.sqrt, sqrt:Math.sqrt, floorf:Math.floor, floor:Math.floor, ceilf:Math.ceil, ceil:Math.ceil, truncf:Math.trunc, trunc:Math.trunc, powf:Math.pow, pow:Math.pow };
        const stubFor = n => AZ_MATH[n] || (/write_memory|barrier|exception_clear/.test(n) ? (() => {}) : (/_f64\b/.test(n) ? (() => 0) : (/_64\b/.test(n) ? (() => 0n) : (() => 0))));
    const h = env => ({ get: (_, p) => typeof p === "string" ? (Object.prototype.hasOwnProperty.call(env, p) ? env[p] : stubFor(p)) : undefined, has: () => true });
    const { instance: miniI } = await WebAssembly.instantiate(miniBytes, { env: new Proxy({}, h(realEnv)) });
    const mini = miniI.exports;
    const memory = mini.memory;
    mini.AzStartup_init(0, 0);
    const modelPtr = mini.AzStartup_alloc(4);
    new DataView(memory.buffer).setUint32(modelPtr, initialCounter, true);
    const refanyPtr = mini.AzStartup_hydrate(Number(typeId & 0xFFFFFFFFn), Number((typeId >> 32n) & 0xFFFFFFFFn), modelPtr, 4);
    const cbEnv = { memory, __indirect_function_table: table };
    const { instance: cbI } = await WebAssembly.instantiate(cbBytes, { env: new Proxy({}, h(cbEnv)) });
    const cb = cbI.exports.callback;
    const dv = new DataView(memory.buffer);
    const infoPtr = mini.AzStartup_alloc(256);
    let last = initialCounter;
    for (let i = 1; i <= 7; i++) {
        const r = cb(BigInt(refanyPtr), 0n, infoPtr);
        const c = dv.getUint32(modelPtr, true);
        console.log("click " + i + ": Update=" + r + " counter " + last + " -> " + c + (c === last + 1 ? " OK" : " FAIL"));
        last = c;
    }
    process.exit(last === initialCounter + 7 ? 0 : 1);
})().catch(e => { console.error(e.stack); process.exit(1); });
