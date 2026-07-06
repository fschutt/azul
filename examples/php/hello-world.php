<?php
// examples/php/hello-world.php
//
// Full-GUI counter, built on the NATIVE PHP/Zend extension (feature
// `php-extension`, ext-php-rs) — NOT the legacy php-ffi binding. The Zend
// engine can call a C function pointer back into PHP, so the host-invoker
// pattern (used by Lua / Ruby / Perl / OCaml / …) works here: libazul fires
// a per-kind Rust invoker, which calls these PHP functions by name.
//
// Build the extension (SEPARATE target dir so it doesn't clobber the
// desktop dll the other examples use):
//
//     LIBCLANG_PATH=/Library/Developer/CommandLineTools/usr/lib \
//     DYLD_FALLBACK_LIBRARY_PATH=$LIBCLANG_PATH \
//     RUSTFLAGS="-C link-arg=-undefined -C link-arg=dynamic_lookup" \
//       cargo build --release -p azul-dll \
//         --features php-extension,debug-server --target-dir target/phpext
//
// Run (desktop):
//
//     php -d extension=/abs/path/to/target/phpext/release/libazul.dylib \
//         hello-world.php
//
// On Linux swap .dylib for .so and use
//     RUSTFLAGS="-C link-arg=-Wl,--unresolved-symbols=ignore-in-object-files"

declare(strict_types=1);

if (!extension_loaded('azul-dll')) {
    fwrite(STDERR, "[azul] FAIL: 'azul-dll' extension not loaded. Pass "
        . "-d extension=/abs/path/to/libazul.dylib on the php command line.\n");
    exit(1);
}

// Install the per-kind layout + button-click invokers (and the releaser).
azul_counter_init();

// The model: a plain PHP array, stored as a JSON snapshot behind a
// host-handle id. on_click mutates it via azul_refany_set; layout reads it.
$model_id = azul_refany_create(json_encode(['counter' => 5]));

// Register the two PHP callables by name; each returns a host-handle id the
// extension mints a wrapper struct from. The click handler uses the generic
// `Callback` kind (attached to a plain DOM node) rather than the Button
// widget.
$GLOBALS['azul_onclick_id'] = azul_register_callback('on_click');
$layout_id                  = azul_register_layout_callback('layout');

/**
 * Button click handler. Receives the model host-handle id, increments the
 * counter, and returns Update::RefreshDom (=1) to trigger a relayout.
 */
function on_click(int $data): int
{
    $m = json_decode(azul_refany_get($data), true);
    $m['counter'] = ($m['counter'] ?? 0) + 1;
    azul_refany_set($data, json_encode($m));
    return 1; // Update::RefreshDom
}

/**
 * Layout callback. Receives the model host-handle id, reads the counter, and
 * returns a DOM: body > [ div > text(counter), div.on_click > text(label) ].
 *
 * The clickable "Increase counter" element is a plain DOM node (not the
 * Button widget) with a Hover/left-mouse-up callback attached.
 */
function layout(int $data): \Azul\Dom
{
    $m       = json_decode(azul_refany_get($data), true);
    $counter = $m['counter'] ?? 0;

    $div = \Azul\Dom::createDiv();
    $div->addChild(\Azul\Dom::createText((string) $counter));

    $btn = \Azul\Dom::createDiv();
    $btn->addChild(\Azul\Dom::createText('Increase counter'));
    $btn->onClick($data, $GLOBALS['azul_onclick_id']);

    $body = \Azul\Dom::createBody();
    $body->addChild($div);
    $body->addChild($btn);
    return $body;
}

$wco = \Azul\WindowCreateOptions::create($layout_id);
$cfg = \Azul\AppConfig::create();
$app = \Azul\App::create($model_id, $cfg);
$app->run($wco);
