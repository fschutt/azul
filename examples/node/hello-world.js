// examples/node/hello-world.js
//
// Minimal Node smoke test for the Azul host-invoker plumbing. Confirms
// that the koffi/JSCallback adapter loads, the dylib initialises, and
// the host-invoker init phase (refanyCreate / refanyGet) round-trips a
// managed object.
//
// Full GUI wiring (Dom builders, WindowCreateOptions, App.run) requires
// the wrapper layer's idiomatic API surface to settle — separate work,
// not host-invoker. The C# / Java / Kotlin / PowerShell hello-worlds
// have the same shape; all five verify the FFI plumbing one level
// above libffi.
//
// Run with:
//     node hello-world.js   (after `npm install` in this dir)
//     bun  run hello-world.js
//     deno run --allow-ffi --unstable-ffi hello-world.js

'use strict';

const azul = require('./azul.js');
const { refanyCreate, refanyGet, __runtime } = azul;

console.log(`[azul] runtime adapter: ${__runtime}`);

const model = { counter: 5 };
const data  = refanyCreate(model);
console.log('[azul] refanyCreate ran; RefAny opaque-handle id stored.');

const recovered = refanyGet(data);
if (recovered && recovered.counter === 5) {
    console.log(`[azul] refanyGet round-trip succeeded; counter=${recovered.counter}`);
} else {
    console.log(`[azul] refanyGet round-trip FAILED (recovered=${recovered})`);
    process.exit(1);
}

console.log('[azul] host-invoker init phase completed successfully.');
console.log('[azul] (Full App.run wiring requires wrapper-layer API surface');
console.log('[azul]  fixes that are separate from the host-invoker plumbing.)');
