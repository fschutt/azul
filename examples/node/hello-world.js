// examples/node/hello-world.js
//
// Node.js / Bun / Deno port of examples/c/hello-world.c.
// Same data model (a counter), same callback semantics (mouse click
// increments the counter, layout rebuilds the DOM each frame).
//
// Built against the host-invoker runtime helpers in `azul.js` (see
// `lang_node/managed.rs`):
//
//   * `azul.refanyCreate(value)` wraps an arbitrary JS value in an AzRefAny
//     held alive by the framework's refcount.
//   * `azul.registerCallback(kind, fn)` returns the matching `Az<Kind>`
//     cdata struct (cb = static thunk in libazul, ctx = host-handle RefAny).
//
// koffi (Node) is libffi-backed and shares the struct-by-value callback
// limit with LuaJIT and ruby-ffi; the host-invoker plumbing routes user
// callbacks through pointer-arg signatures so the cast is always legal.
// Bun and Deno can synthesize struct-by-value via JSCallback /
// UnsafeCallback, but go through the same path here for uniformity.
//
// Run with:
//     node hello-world.js   (after `npm install` in this dir)
//     bun  run hello-world.js
//     deno run --allow-ffi --unstable-ffi hello-world.js

'use strict';

const azul = require('./azul.js');
const {
    App,
    AppConfig,
    Button,
    ButtonType,
    CssProperty,
    CssPropertyWithConditions,
    Dom,
    StyleFontSize,
    String: AzString,        // shadows the JS String constructor — alias.
    WindowCreateOptions,
    Update,
    WindowDecorations,
    WindowBackgroundMaterial,
    registerCallback,
    refanyCreate,
    refanyGet,
    __runtime,
    __lib: lib,
} = azul;

console.log(`[azul] runtime adapter: ${__runtime}`);

// ── Data model ────────────────────────────────────────────────────────
//
// `azul.refanyCreate(value)` stashes the value in a process-wide id-keyed
// hash and returns an AzRefAny whose payload is just the id. The
// destructor that fires on the last clone calls back through
// `AzApp_setHostHandleReleaser` to drop the entry.

const model = { counter: 5 };
const data  = refanyCreate(model);

// ── Callback: button click ────────────────────────────────────────────

function onClick(dataPtr, _infoPtr) {
    const m = refanyGet(dataPtr);
    if (m === null) return Update.DoNothing;
    m.counter += 1;
    return Update.RefreshDom;
}

// ── Callback: layout (rebuilds DOM each frame) ───────────────────────

function layout(dataPtr, _infoPtr) {
    const m = refanyGet(dataPtr);
    if (m === null) return Dom.createBody();

    // Counter label, wrapped in a div so the font-size sticks.
    const labelText    = AzString.fromString(String(m.counter));
    const label        = Dom.createText(labelText);
    const labelWrapper = Dom.createDiv();
    labelWrapper.addCssProperty(
        CssPropertyWithConditions.simple(
            CssProperty.fontSize(StyleFontSize.px(32.0))));
    labelWrapper.addChild(label);

    // Increment button. Until lang_node/wrappers.rs learns to substitute
    // callback args via registerCallback automatically, we wire it here.
    const button = Button.create(AzString.fromString('Increase counter'));
    button.setButtonType(ButtonType.Primary);
    const cb = registerCallback('Callback', onClick);
    button.setOnClick(lib.AzRefAny_clone(dataPtr), cb);
    const buttonDom = button.dom();

    // Body.
    const body = Dom.createBody();
    body.addChild(labelWrapper);
    body.addChild(buttonDom);
    return body;
}

// ── Main ──────────────────────────────────────────────────────────────

const layoutCb = registerCallback('LayoutCallback', layout);

// WindowCreateOptions::create takes a *raw* LayoutCallbackType, not the
// wrapper struct. We bypass it via _default + direct field assignment so
// the host-handle ctx survives — same fix lang_lua applies in its emitter.
// Note: `default` is JS-reserved; the codegen renames to `default_`.
const window = WindowCreateOptions.default_();
window.window_state.layout_callback = layoutCb;

window.window_state.title = AzString.fromString('Hello World');
window.window_state.size.dimensions.width  = 400.0;
window.window_state.size.dimensions.height = 300.0;

// NoTitleAutoInject: OS draws close/min/max buttons; framework
// auto-injects a Titlebar with drag support.
window.window_state.flags.decorations         = WindowDecorations.NoTitleAutoInject;
window.window_state.flags.background_material = WindowBackgroundMaterial.Sidebar;

const app = App.create(data, AppConfig.create());
app.run(window);
// FinalizationRegistry calls AzApp_delete when `app` is GC'd. For
// deterministic cleanup, call `app.delete()` explicitly.
