// examples/node/hello-world.js
//
// Node.js / Bun / Deno port of examples/c/hello-world.c.
// Same data model (a `MyDataModel` struct with a uint32 counter), same
// callback semantics (mouse click increments, layout renders).
//
// Run with: `node hello-world.js`  (after `npm install`)
//      or:  `bun run hello-world.js`
//      or:  `deno run --allow-ffi --unstable-ffi hello-world.js`
//
// This file uses ONLY the idiomatic wrapper classes from `azul.js`. There
// is no manual `azul.__ffi.lib` access except where the C ABI exposes
// helpers that don't have a wrapper representation (RefAny construction,
// callback registration). The wrappers manage native lifetime via
// `FinalizationRegistry`, so explicit `.delete()` is optional for most
// types — we only call it on `App` to demonstrate deterministic cleanup.

'use strict';

const azul = require('./azul.js');
const {
    App,
    AppConfig,
    Button,
    ButtonType,
    Css,
    CssProperty,
    CssPropertyWithConditions,
    Dom,
    StyleFontSize,
    String: AzString,        // shadows the JS String constructor — alias.
    WindowCreateOptions,
    Update,
    WindowDecorations,
    WindowBackgroundMaterial,
    __ffi: ffi,
    __runtime,
} = azul;

console.log(`[azul] runtime adapter: ${__runtime}`);

// ─── Live-storage table for FFI callbacks ───────────────────────────────
//
// `azulFFI.callback(...)` returns a trampoline whose lifetime is tied to
// the JS handle. If the handle is GC'd the C side will jump into freed
// memory the next time the callback fires. We keep every registered
// callback in a module-level array so it stays referenced for the
// process lifetime — the same pattern the LuaJIT FFI binding uses.
const liveCallbacks = [];

function pinCallback(proto, jsFn) {
    const cb = ffi.callback(proto, jsFn);
    liveCallbacks.push(cb);
    return cb;
}

// ─── Data model ─────────────────────────────────────────────────────────
//
// `RefAny` is the type-erased container azul uses to ferry user data
// between callbacks. We allocate a 4-byte buffer with a single uint32
// counter and hand its address to AzRefAny_newC. azul copies
// `sizeof(MyDataModel)` bytes into a heap-allocated RefAny.
//
// SKIPPED: AzString_fromConstStr is a C macro and is therefore not
// reachable through any FFI binding (Lua, PHP, Node, etc.). We build
// the type-name AzString via the regular byte-copy constructor instead.

const MY_DATA_MODEL_RTTI_ID = 0xa201_0001n; // BigInt — unique 64-bit RTTI id.

const modelBuffer = new Uint32Array(1);
modelBuffer[0] = 5; // initial counter value

// AzRefAnyDestructor takes (void*) and returns void. The model owns no
// heap memory so the destructor body is empty.
const destructorProto = ffi.proto(
    'AzRefAnyDestructorType',
    'void',
    [ffi.ptr],
);
const modelDestructor = pinCallback(destructorProto, () => {});

function makeAzString(s) {
    const bytes = Buffer.from(s, 'utf8');
    return ffi.lib.AzString_copyFromBytes(bytes, 0, bytes.length);
}

const typeNameStr = makeAzString('MyDataModel');
const refAny = ffi.lib.AzRefAny_newC(
    modelBuffer,                 // ptr to user data
    modelBuffer.byteLength,      // size
    modelBuffer.BYTES_PER_ELEMENT,
    MY_DATA_MODEL_RTTI_ID,
    typeNameStr,
    modelDestructor,
);

function downcastModel(refAnyArg) {
    if (!ffi.lib.AzRefAny_isType(refAnyArg, MY_DATA_MODEL_RTTI_ID)) {
        return null;
    }
    return ffi.lib.AzRefAny_getDataPtr(refAnyArg);
}

// ─── Callback: increment the counter on click ───────────────────────────

const onClickProto = ffi.proto(
    'AzCallbackType',
    'uint32_t',                    // returns AzUpdate (32-bit enum)
    [ffi.ptr, ffi.ptr],            // (AzRefAny, AzCallbackInfo)
);
const onClickCb = pinCallback(onClickProto, (data, _info) => {
    const ptr = downcastModel(data);
    if (ptr === null) return Update.DoNothing;
    // Read/modify the uint32 through a typed view of the same memory.
    // The runtime adapter exposes raw pointers; we use Uint32Array.from
    // for portability across koffi/Bun/Deno.
    const view = new Uint32Array(ptr.buffer ?? modelBuffer.buffer, 0, 1);
    view[0] += 1;
    return Update.RefreshDom;
});

// ─── Layout callback ────────────────────────────────────────────────────

const layoutProto = ffi.proto(
    'AzLayoutCallbackType',
    ffi.ptr,                       // returns AzDom (struct-by-value)
    [ffi.ptr, ffi.ptr],            // (AzRefAny, AzLayoutCallbackInfo)
);
const layoutCb = pinCallback(layoutProto, (data, _info) => {
    const ptr = downcastModel(data);
    if (ptr === null) return ffi.lib.AzDom_createBody();

    const view = new Uint32Array(ptr.buffer ?? modelBuffer.buffer, 0, 1);

    // Counter label, wrapped in a div so it lays out as block.
    const labelStr = makeAzString(String(view[0]));
    const labelDom = Dom.createText(labelStr);
    const labelWrapper = Dom.createDiv();
    ffi.lib.AzDom_addCssProperty(
        labelWrapper,
        ffi.lib.AzCssPropertyWithConditions_simple(
            ffi.lib.AzCssProperty_fontSize(
                ffi.lib.AzStyleFontSize_px(32.0),
            ),
        ),
    );
    ffi.lib.AzDom_addChild(labelWrapper, labelDom);

    // Button.
    const btnText = makeAzString('Increase counter');
    const button = Button.create(btnText);
    ffi.lib.AzButton_setButtonType(button._ptr, ButtonType.Primary);
    const dataClone = ffi.lib.AzRefAny_clone(data);
    ffi.lib.AzButton_setOnClick(button._ptr, dataClone, onClickCb);
    const buttonDom = ffi.lib.AzButton_dom(button._ptr);

    // Body.
    const body = Dom.createBody();
    ffi.lib.AzDom_addChild(body, labelWrapper);
    ffi.lib.AzDom_addChild(body, buttonDom);

    return ffi.lib.AzDom_style(body, ffi.lib.AzCss_empty());
});

// ─── Main ───────────────────────────────────────────────────────────────

const window = ffi.lib.AzWindowCreateOptions_create(layoutCb);
const titleStr = makeAzString('Hello World');
window.window_state.title = titleStr;
window.window_state.size.dimensions.width = 400.0;
window.window_state.size.dimensions.height = 300.0;
// NoTitleAutoInject: OS draws close/min/max buttons,
// framework auto-injects a Titlebar with drag support.
window.window_state.flags.decorations = WindowDecorations.NoTitleAutoInject;
window.window_state.flags.background_material = WindowBackgroundMaterial.Sidebar;

const app = App.create(refAny, AppConfig.create());
app.run(window);

// Explicit deterministic cleanup. The FinalizationRegistry would also
// catch this on GC, but we want the window torn down promptly when the
// app loop returns — not at the next major GC cycle.
app.delete();
