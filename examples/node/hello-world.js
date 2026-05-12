// examples/node/hello-world.js
//
// Node port of examples/c/hello-world.c, written against the
// idiomatic `azul` wrapper layer. Same data model (a counter), same
// behaviour (mouse click increments, layout rebuilds the DOM).
//
// Callbacks go through libazul's host-invoker plumbing
// (`AzCallback_createFromHostHandle`, `AzApp_setCallbackInvoker`)
// so koffi never needs to synthesize a struct-by-value trampoline
// for the user function. The wrapper layer's consuming builders
// (`with_child` / `with_button_type` / etc.) do the right thing
// under koffi: each call moves the receiver and any by-value
// wrapper args into the C call, unregisters them from their
// `FinalizationRegistry`, and wraps the returned struct in a fresh
// wrapper instance. So a chain like
// `Dom.create_body().with_child(label).with_child(button.dom())`
// is safe — no double-free on the moved-from steps.
//
// Run with:
//     node hello-world.js   (after `npm install` in this dir)
//     bun  run hello-world.js
//     deno run --allow-ffi --unstable-ffi hello-world.js

'use strict';

const azul = require('./azul.js');
const {
    App, AppConfig, Button, Dom,
    CssProperty, CssPropertyWithConditions, StyleFontSize,
    refanyCreate, refanyGet, registerCallback,
} = azul;
const lib = azul.__lib;

// AzString constructor: copy a JS UTF-8 string into an AzString.
// The wrapper layer doesn't yet auto-convert JS strings, so we use
// a tiny helper. The result is an `azul.String` wrapper instance.
function azStr(s) {
    const buf = Buffer.from(s, 'utf8');
    return new azul.String(lib.AzString_fromUtf8(buf, buf.length));
}

// ── Data model ────────────────────────────────────────────────────────
// Plain JS object held alive by libazul via the host-handle table.

const model = { counter: 5 };
const data  = refanyCreate(model);

// ── Callbacks ─────────────────────────────────────────────────────────

function onClick(dataPtr, _info) {
    const m = refanyGet(dataPtr);
    if (m == null) return 0; // Update.DoNothing
    m.counter += 1;
    return 1; // Update.RefreshDom
}

function layout(dataPtr, _info) {
    const m = refanyGet(dataPtr);
    if (m == null) return Dom.create_body();

    // Counter label wrapped in a font-size-32 div.
    const label = Dom.create_div()
        .with_css_property(
            CssPropertyWithConditions.simple(
                CssProperty.font_size(StyleFontSize.px(32.0))))
        .with_child(Dom.create_text(azStr(String(m.counter))));

    // Increment button.
    const button = Button.create(azStr('Increase counter'))
        .with_button_type(1 /* ButtonType.Primary */)
        .with_on_click(lib.AzRefAny_clone(data), onClick);

    return Dom.create_body()
        .with_child(label)
        .with_child(button.dom());
}

// ── Main ──────────────────────────────────────────────────────────────
//
// We build the window via `AzWindowCreateOptions_default()` plus
// direct field assignment rather than `WindowCreateOptions.create(layout)`
// because the latter goes through `AzWindowCreateOptions_create`, which
// expects an `AzLayoutCallbackType` (a raw fn pointer) and would
// discard the host-invoker `ctx` carrying our dispatch handle. The
// assignment to `window_state.layout_callback` takes the full
// `AzLayoutCallback` struct (cb + ctx) so dispatch reaches our JS fn.
//
// Without this, an uncaught exception from inside a koffi-registered
// callback aborts the process (SIGABRT) before the host-invoker
// thunk's own try/catch can log it. The handler is a no-op for
// healthy GUI runs.
process.on('uncaughtException', (e) => {
    console.error('[azul] uncaught:', e && e.stack ? e.stack : e);
});

const window = lib.AzWindowCreateOptions_default();
window.window_state.layout_callback = registerCallback('LayoutCallback', layout);
window.window_state.title           = lib.AzString_fromUtf8(Buffer.from('Hello World'), 11);
window.window_state.size.dimensions.width  = 400.0;
window.window_state.size.dimensions.height = 300.0;
// NoTitleAutoInject: OS draws close/min/max buttons; framework
// auto-injects a Titlebar with drag support.
window.window_state.flags.decorations         = 1; // WindowDecorations.NoTitleAutoInject
window.window_state.flags.background_material = 3; // WindowBackgroundMaterial.Sidebar

const app = App.create(data, AppConfig.create());
app.run(window);
