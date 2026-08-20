<?php

declare(strict_types=1);

if (!extension_loaded('azul-dll')) {
    fwrite(STDERR, "[azul] FAIL: 'azul-dll' extension not loaded. Pass "
        . "-d extension=/abs/path/to/libazul.dylib on the php command line.\n");
    exit(1);
}

azul_counter_init();

$model_id = azul_refany_create(json_encode(['counter' => 5]));

$GLOBALS['azul_onclick_id'] = azul_register_callback('on_click');
$layout_id                  = azul_register_layout_callback('layout');

function on_click(int $data): int
{
    $m = json_decode(azul_refany_get($data), true);
    $m['counter'] = ($m['counter'] ?? 0) + 1;
    azul_refany_set($data, json_encode($m));
    return 1; // Update::RefreshDom
}

function layout(int $data): \Azul\Dom
{
    $m       = json_decode(azul_refany_get($data), true);
    $counter = $m['counter'] ?? 0;

    $div = \Azul\Dom::createDiv();
    $div->addChild(\Azul\Dom::createTextDoNotUseWithoutBlockLevelWrapper((string) $counter));

    $btn = \Azul\Dom::createDiv();
    $btn->addChild(\Azul\Dom::createTextDoNotUseWithoutBlockLevelWrapper('Increase counter'));
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
