// examples/node/hello-world.js
//
// Node port of examples/c/hello-world.c.
//
// Same data model (a counter), same behaviour (mouse click increments,
// layout rebuilds the DOM). Goes through libazul's host-invoker
// plumbing (`AzCallback_createFromHostHandle` / `AzApp_setCallbackInvoker`)
// so koffi never needs to synthesize a struct-by-value trampoline.
//
// ── Why this file uses `azul.__lib.*` directly ────────────────────────
//
// The auto-generated wrapper classes in `azul.js` are JS objects with
// a `_ptr` field holding the underlying koffi struct value. Two koffi
// behaviours we have to route around:
//
//   1. koffi treats struct-typed JS values as input-only for `T *` args.
//      `lib.AzDom_addChild(body, child)` looks like a mutator but koffi
//      copies `body`'s bytes into a temp, passes the temp's address to C,
//      and never copies the result back. JS-side `body` is unchanged.
//      → use the consuming `*_with_*` variants (return a fresh struct).
//
//   2. The wrapper classes register every constructed value with a
//      `FinalizationRegistry` that calls `<Type>_delete` on GC. Builder
//      chains leave moved-from structs in JS-land that the registry then
//      double-frees. The raw `azul.__lib.*` path skips that machinery —
//      every struct is a plain koffi value, the framework owns
//      everything once App.run takes over.
//
// ── Why we override the LayoutCallback invoker ────────────────────────
//
// The codegen-emitted `AzLayoutCallbackInvoker` silently drops struct
// returns (it only encodes numeric ones, for the Update enum). Layout
// returns AzDom, so we install our own invoker that does
// `koffi.encode(outPtr, 'AzDom', ret)` instead. Same for our
// per-language `_layoutHandles` so the dispatch can find the JS fn.
//
// Run with:
//     node hello-world.js   (after `npm install` in this dir)
//     bun  run hello-world.js
//     deno run --allow-ffi --unstable-ffi hello-world.js

'use strict';

const azul  = require('./azul.js');
const lib   = azul.__lib;
const koffi = azul.__ffi.koffi;

console.log(`[azul] runtime adapter: ${azul.__runtime}`);

// Without this, an uncaught exception from inside a koffi-registered
// callback aborts the process (SIGABRT) before the host-invoker thunk's
// own try/catch surfaces it. The handler is otherwise a no-op for
// healthy GUI runs.
process.on('uncaughtException', (e) => {
    console.error('[azul] uncaught:', e && e.stack ? e.stack : e);
});

// ── Helpers ───────────────────────────────────────────────────────────

function azStr(s) {
    const buf = Buffer.from(s, 'utf8');
    return lib.AzString_fromUtf8(buf, buf.length);
}

// ── LayoutCallback invoker that handles AzDom struct returns ──────────
//
// Bootstraps `_ensureHostInvokerInit` (releaser + per-kind invokers) via
// a throwaway register_callback, then overwrites the layout slot.
azul.registerCallback('LayoutCallback', () => lib.AzDom_createBody());
const _layoutHandles = Object.create(null);
let _nextLayoutId = 1_000_000n;

const layoutProto = azul.__ffi.proto(
    'AzLayoutCallbackInvokerV2', 'void',
    ['uint64_t', 'void *', 'void *', 'void *'],
);
const layoutInvoker = azul.__ffi.callback(layoutProto, (id, dataPtr, _infoPtr, outPtr) => {
    try {
        const fn = _layoutHandles[String(id)];
        if (!fn) return;
        const dom = fn(dataPtr, _infoPtr);
        if (dom != null) koffi.encode(outPtr, 'AzDom', dom);
    } catch (e) {
        console.error('[azul] layout invoker error:', e && e.stack ? e.stack : e);
    }
});
lib.AzApp_setLayoutCallbackInvoker(layoutInvoker);

function registerLayout(fn) {
    const id = _nextLayoutId++;
    _layoutHandles[String(id)] = fn;
    return lib.AzLayoutCallback_createFromHostHandle(id);
}

// ── Data model ────────────────────────────────────────────────────────

const model = { counter: 5 };
const data  = azul.refanyCreate(model);

// ── Callback: button click ────────────────────────────────────────────

function onClick(dataPtr, _info) {
    const m = azul.refanyGet(dataPtr);
    if (m == null) return 0; /* Update.DoNothing */
    m.counter += 1;
    return 1; /* Update.RefreshDom */
}

// ── Layout callback ───────────────────────────────────────────────────

function layout(dataPtr, _info) {
    const m = azul.refanyGet(dataPtr);
    if (m == null) return lib.AzDom_createBody();

    // Counter label, wrapped in a font-size-32 div. Consuming builder
    // chain so koffi's input-only struct semantics don't bite us.
    const counterText = lib.AzDom_createText(azStr(String(m.counter)));
    const fontSize    = lib.AzStyleFontSize_px(32.0);
    const cssProp     = lib.AzCssProperty_fontSize(fontSize);
    const cond        = lib.AzCssPropertyWithConditions_simple(cssProp);
    let labelWrapper  = lib.AzDom_createDiv();
    labelWrapper      = lib.AzDom_withCssProperty(labelWrapper, cond);
    labelWrapper      = lib.AzDom_withChild(labelWrapper, counterText);

    // Increment button.
    let button       = lib.AzButton_create(azStr('Increase counter'));
    button           = lib.AzButton_withButtonType(button, 1 /* ButtonType.Primary */);
    const onClickCb  = azul.registerCallback('Callback', onClick);
    const dataClone  = lib.AzRefAny_clone(data);
    button           = lib.AzButton_withOnClick(button, dataClone, onClickCb);
    const buttonDom  = lib.AzButton_dom(button);

    // Body.
    let body = lib.AzDom_createBody();
    body     = lib.AzDom_withChild(body, labelWrapper);
    body     = lib.AzDom_withChild(body, buttonDom);
    return body;
}

// ── Main ──────────────────────────────────────────────────────────────
//
// Lua-style window setup: start from `_default()`, plug in the layout
// callback through the host-invoker path (`AzLayoutCallback_createFromHostHandle`)
// so ctx is preserved, then set the window-state fields directly.
// `AzWindowCreateOptions_create(fnptr)` would discard ctx because the
// C ABI accepts only `AzLayoutCallbackType` (= `void *`).

const window = lib.AzWindowCreateOptions_default();
window.window_state.layout_callback = registerLayout(layout);
window.window_state.title           = azStr('Hello World');
window.window_state.size.dimensions.width  = 400.0;
window.window_state.size.dimensions.height = 300.0;
// NoTitleAutoInject: OS draws close/min/max buttons; framework
// auto-injects a Titlebar with drag support.
window.window_state.flags.decorations         = 1 /* WindowDecorations.NoTitleAutoInject */;
window.window_state.flags.background_material = 3 /* WindowBackgroundMaterial.Sidebar */;

const app = lib.AzApp_create(data, lib.AzAppConfig_create());
lib.AzApp_run(app, window);
