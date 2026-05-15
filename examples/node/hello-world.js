// examples/node/hello-world.js — Node port of examples/c/hello-world.c.
// Run: node hello-world.js   (after `npm install`)
//      bun  run hello-world.js
//      deno run --allow-ffi --unstable-ffi hello-world.js

'use strict';

const azul = require('./azul.js');
const {
    App, AppConfig, Button, ButtonType, Dom,
    CssProperty, CssPropertyWithConditions, StyleFontSize,
    Update, WindowBackgroundMaterial, WindowCreateOptions, WindowDecorations,
    refanyCreate, refanyGet,
} = azul;
const model = { counter: 5 };

function onClick(dataPtr, _info) {
    const m = refanyGet(dataPtr);
    if (m == null) return Update.DoNothing;
    m.counter += 1;
    return Update.RefreshDom;
}

function layout(dataPtr, _info) {
    const m = refanyGet(dataPtr);
    if (m == null) return Dom.create_body();

    const label = Dom.create_div()
        .with_css_property(
            CssPropertyWithConditions.simple(
                CssProperty.font_size(StyleFontSize.px(32.0))))
        .with_child(Dom.create_text(String(m.counter)));

    const button = Button.create('Increase counter')
        .with_button_type(ButtonType.Primary)
        .on_click(model, onClick);

    return Dom.create_body()
        .with_child(label)
        .with_child(button.dom());
}

// Catch uncaught exceptions from inside koffi-registered callbacks
// before they SIGABRT the process via the libffi trampoline.
process.on('uncaughtException', (e) => {
    console.error('[azul] uncaught:', e && e.stack ? e.stack : e);
});

const window = WindowCreateOptions.createWithLayout(layout).with({
    window_state: {
        title: 'Hello World',
        size: { dimensions: { width: 400.0, height: 300.0 } },
        flags: {
            decorations: WindowDecorations.NoTitleAutoInject,
            background_material: WindowBackgroundMaterial.Sidebar,
        },
    },
});

App.create(refanyCreate(model), AppConfig.create()).run(window);
