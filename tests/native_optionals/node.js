// Node: `null` where an Option is asked for, `null` or the value back from
// fields and downcasts, a thrown Error for an Err (inside a callback the
// binding logs it and the callback does nothing), and the resume callback's
// result as a RefAny that `downcast` takes. Run by
// scripts/native_optionals_smoke.sh.
'use strict';

const azul = require('./azul.js');

// The guide's file-dialog example, as written there (hello-world/node.md).
function onChooseFolder(model, _info) {
    azul.FileDialog.openDirectory('Choose a folder', null, model, onPickFolder);
    return azul.Update.DoNothing;
}

function onPickFolder(model, _info, result) {
    const path = azul.FileOpenResult.downcast(result)?.path;
    if (path == null) return azul.Update.DoNothing; // not a folder pick, or cancelled
    model.folder = path.inner;
    return azul.Update.RefreshDom;
}

function onOpenLink(model, _info) {
    azul.Url.parse(model.link); // an Err throws: logged, the callback does nothing
    return azul.Update.RefreshDom;
}

function check(ok, what) {
    if (!ok) {
        console.error('FAIL: ' + what);
        process.exit(1);
    }
}

// A field: null in, null out; a value in, the value out.
const icons = azul.IconVariants.createDefault();
check(icons.light === null, 'a default IconVariants has no light icon');
icons.light = 'light.png';
check(icons.light === 'light.png', 'Some in, Some out');
icons.light = null;
check(icons.light === null, 'None in, None out');

// A class payload: a Dom moved in, a Dom read back, null clears it.
const view = azul.VirtualViewReturn.createDefault();
check(view.dom === null, 'a default VirtualViewReturn has no Dom');
view.dom = azul.Dom.createBody();
check(view.dom !== null, 'a Dom moved into an Option field reads back');
view.dom = null;
check(view.dom === null, 'null clears the field');

// A result: Ok is the value, Err a thrown Error.
check(azul.Url.parse('https://azul.rs/guide') != null, 'Url.parse Ok');
let threw = false;
try {
    azul.Url.parse('no scheme');
} catch (_e) {
    threw = true;
}
check(threw, 'Url.parse Err throws');

azul.Button.create('Open').withOnClick({ folder: '', link: 'no scheme' }, onOpenLink);
check(typeof onChooseFolder === 'function', 'the guide callbacks exist');
console.log('node native optionals: ok');
