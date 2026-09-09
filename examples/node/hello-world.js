'use strict';

let azul;
try { azul = require('./azul.js'); } catch (_) { azul = require('azul'); }
const {
    App, AppConfig, Button, ButtonType, Dom, Update, WindowCreateOptions,
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

    const label = Dom.create_p_with_text(String(m.counter))
        .with_css('font-size: 32px; margin: 0;');

    const button = Button.create('Increase counter')
        .with_button_type(ButtonType.Primary)
        .on_click(model, onClick);

    return Dom.create_body()
        .with_child(label)
        .with_child(button.dom());
}

process.on('uncaughtException', (e) => {
    console.error('[azul] uncaught:', e && e.stack ? e.stack : e);
});

const window = WindowCreateOptions.createWithLayout(layout).with({
    window_state: {
        title: 'Hello World',
        size: { dimensions: { width: 400.0, height: 300.0 } },
        flags: {
        },
    },
});

App.create(refanyCreate(model), AppConfig.create()).run(window);
