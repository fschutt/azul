<?php
// Full-GUI counter on the native PHP/Zend extension (feature `php-extension`,
// ext-php-rs) — NOT php-ffi. libazul fires a per-kind Rust invoker that calls
// these PHP functions by name (host-invoker pattern).
// Load the extension with `php -d extension=/abs/path/to/libazul.dylib`.

declare(strict_types=1);

if (!extension_loaded('azul-dll')) {
    fwrite(STDERR, "[azul] FAIL: 'azul-dll' extension not loaded. Pass "
        . "-d extension=/abs/path/to/libazul.dylib on the php command line.\n");
    exit(1);
}

azul_counter_init();

// The model: a plain PHP array stored as a JSON snapshot behind a host-handle
// id. on_click mutates it via azul_refany_set; layout reads it.
$model_id = azul_refany_create(json_encode(['counter' => 5]));

$GLOBALS['azul_onclick_id'] = azul_register_callback('on_click');
$layout_id                  = azul_register_layout_callback('layout');

// Bump the counter, return Update::RefreshDom (=1) to trigger a relayout.
function on_click(int $data): int
{
    $m = json_decode(azul_refany_get($data), true);
    $m['counter'] = ($m['counter'] ?? 0) + 1;
    azul_refany_set($data, json_encode($m));
    return 1; // Update::RefreshDom
}

// Build body > [ div > text(counter), div.on_click > text(label) ]. The
// clickable element is a plain DOM node (not the Button widget) with an
// on-click callback attached.
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
