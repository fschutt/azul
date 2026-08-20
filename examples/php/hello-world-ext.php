<?php

declare(strict_types=1);

echo "[azul] PHP extension smoke test starting.\n";

if (!extension_loaded('azul-dll')) {
    fwrite(STDERR, "[azul] FAIL: 'azul-dll' extension not loaded. "
        . "Pass -d extension=/path/to/libazul.dylib on the php command line.\n");
    exit(1);
}
echo "[azul] 'azul-dll' extension loaded by the Zend engine.\n";

$version = azul_version();
if ($version !== '0.0.7') {
    fwrite(STDERR, "[azul] FAIL: azul_version() returned '$version', expected '0.0.7'.\n");
    exit(1);
}
echo "[azul] azul_version() = $version (round-tripped through Zend ext call).\n";

azul_host_invoker_init();
echo "[azul] azul_host_invoker_init() registered releaser.\n";

$model = ["counter" => 5, "label" => "hello, php"];
$id = azul_refany_create(json_encode($model));
echo "[azul] azul_refany_create(model) stored handle id=$id.\n";

$recovered_json = azul_refany_get($id);
if ($recovered_json === null) {
    fwrite(STDERR, "[azul] FAIL: azul_refany_get($id) returned null.\n");
    exit(1);
}
$recovered = json_decode($recovered_json, true);
if ($recovered['counter'] !== 5 || $recovered['label'] !== 'hello, php') {
    fwrite(STDERR, "[azul] FAIL: refany round-trip lost data: "
        . var_export($recovered, true) . "\n");
    exit(1);
}
echo "[azul] azul_refany_get round-trip succeeded; counter="
    . $recovered['counter'] . ", label='" . $recovered['label'] . "'.\n";

function on_button_click_smoke(string $args_json): string {
    $args = json_decode($args_json, true);
    return json_encode(['handled' => true, 'received' => $args]);
}

$button_cb_id = azul_register_button_on_click_callback('on_button_click_smoke');
$layout_cb_id = azul_register_layout_callback('on_button_click_smoke');
echo "[azul] azul_register_button_on_click_callback('on_button_click_smoke') = $button_cb_id.\n";

$result = azul_invoke_callback($button_cb_id, json_encode(['click_x' => 42, 'click_y' => 17]));
if ($result === null) {
    fwrite(STDERR, "[azul] FAIL: azul_invoke_callback($button_cb_id) returned null.\n");
    exit(1);
}
$parsed = json_decode($result, true);
if (!is_array($parsed) || !($parsed['handled'] ?? false) || ($parsed['received']['click_x'] ?? -1) !== 42) {
    fwrite(STDERR, "[azul] FAIL: callback round-trip lost data: $result\n");
    exit(1);
}
echo "[azul] azul_invoke_callback round-trip: PHP fn fired from Rust, returned $result.\n";

$fn_count = count(get_extension_funcs('azul-dll'));
echo "[azul] codegen exposed $fn_count PHP functions; full register+invoke path live.\n";

$body = Azul\Dom::createBody();
$div  = Azul\Dom::createDiv();
if ($body->nodeCount() !== 1 || $div->nodeCount() !== 1) {
    fwrite(STDERR, "[azul] FAIL: dom nodeCount mismatch: body=" . $body->nodeCount()
        . ", div=" . $div->nodeCount() . "\n");
    exit(1);
}
echo "[azul] Azul\\Dom::createBody()->nodeCount() = " . $body->nodeCount() . " (PHP class round-trip).\n";

$model_id     = azul_refany_create('counter:5');
$layout_cb_id = azul_register_layout_callback('layout');
$wco          = Azul\WindowCreateOptions::default();
$cfg          = Azul\AppConfig::create();
$app          = Azul\App::create($model_id, $cfg);
echo "[azul] Azul\\App::create(refany={$model_id}, cfg) → AzulApp object.\n";
echo "[azul] Azul\\App has run() method: " . (method_exists($app, 'run') ? 'YES' : 'NO') . ".\n";
echo "[azul] Phase 51 host-invoker + Dom + App chain reachable from PHP.\n";

function layout($args_json) { return '{"unused":"smoke test"}'; }

echo "[azul] (Full App.run with layout-callback splice still needs the\n";
echo "[azul]  WindowCreateOptions.layout_callback wiring; the smart\n";
echo "[azul]  create(callable) factory is the next codegen step.)\n";
