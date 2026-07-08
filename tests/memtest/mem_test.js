// Memory test for the azul Node.js (koffi) binding. See tests/memtest/README.md.
//
// The harness measures RSS/gdb externally; this file just exercises the
// create/consume/DROP paths in a loop and exits 0. No event loop (App.run
// needs a display and hangs headless). Native objects are freed by koffi via
// a FinalizationRegistry, so the loop drops references and nudges the GC.
//
// Run (matches examples/node/hello-world.js):
//   node --expose-gc mem_test.js      (--expose-gc lets us reclaim promptly)

'use strict';

let azul;
try { azul = require('azul'); } catch (_) { azul = require('./azul.js'); }
const { App, AppConfig, Dom, refanyCreate } = azul;

const N = parseInt(process.env.AZ_MEMTEST_N || '200000', 10);

process.on('uncaughtException', (e) => {
    console.error('[azul] uncaught:', e && e.stack ? e.stack : e);
    process.exit(1);
});

// 1. The consume-by-value DROP path: App.create consumes the RefAny +
//    AppConfig. Build it without running (no window).
{
    let app = App.create(refanyCreate({ counter: 5 }), AppConfig.create());
    app = null;
}

// 2. Leak loop: create/destroy droppable objects N times. koffi frees them on
//    GC, so drop the refs and periodically nudge the collector when available.
for (let i = 0; i < N; i++) {
    let cfg = AppConfig.create();
    let dom = Dom.create_body();
    cfg = null;
    dom = null;
    if (typeof global.gc === 'function' && (i & 0x3fff) === 0) global.gc();
}
if (typeof global.gc === 'function') global.gc();

console.log(`memtest node OK (N=${N})`);
process.exit(0);
