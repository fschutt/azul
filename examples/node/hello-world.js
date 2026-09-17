'use strict';

const azul = require('azul'); // Use require('./azul.js') if downloaded manually
const model = { counter: 5 };

function onClick(data, _info) {
    data.counter += 1;
    return azul.Update.RefreshDom;
}

function layout(data, _info) {
    const label = azul.Dom.createPWithText(String(data.counter))
        .withCss('font-size: 32px; margin: 0;');

    const button = azul.Button.create('Increase counter')
        .setButtonType(azul.ButtonType.Primary)
        .withOnClick(data, onClick)
        .dom();

    return azul.Dom.createBody()
        .addChild(label)
        .addChild(button);
}

const window = azul.WindowCreateOptions.create(layout).with({
    windowState: {
        title: 'Hello World',
        size: { dimensions: { width: 400.0, height: 300.0 } }
    },
});

azul.App.create(model, azul.AppConfig.create()).run(window);
