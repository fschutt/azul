'use strict';

let azul;
try { azul = require('./azul.js'); } catch (_) { azul = require('azul'); }
const {
    App, AppConfig, Button, ButtonType, Dom, Update, WindowCreateOptions,
} = azul;
const model = { counter: 5 };

function onClick(data, _info) {
    data.counter += 1;
    return Update.RefreshDom;
}

function layout(data, _info) {
    const label = Dom.createPWithText(String(data.counter))
        .withCss('font-size: 32px; margin: 0;');

    const button = Button.create('Increase counter')
        .setButtonType(ButtonType.Primary)
        .withOnClick(data, onClick)
        .dom();

    return Dom.createBody()
        .addChild(label)
        .addChild(button);
}

process.on('uncaughtException', (e) => {
    console.error('[azul] uncaught:', e && e.stack ? e.stack : e);
});

const window = WindowCreateOptions.create(layout).with({
    windowState: {
        title: 'Hello World',
        size: { dimensions: { width: 400.0, height: 300.0 } }
    },
});

App.create(model, AppConfig.create()).run(window);
