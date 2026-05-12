<?php
// examples/php/hello-world-ext.php
//
// ──────────────────────────────────────────────────────────────────────
// PHP EXTENSION SMOKE TEST (host-invoker tier)
// ──────────────────────────────────────────────────────────────────────
// Where `hello-world.php` exercises the legacy php-ffi binding (which
// rejects closure-to-fnpointer and so caps out at the POD wrappers),
// this script exercises the native PHP/Zend extension that lives in
// `dll/src/php_extension.rs` under the `php-extension` Cargo feature.
//
// The extension is loaded by the Zend engine before script start, so
// it can pin libffi closures and route user PHP callables back through
// the host-invoker pattern — the same pattern used by Lua / Ruby / Perl
// / OCaml / Node / Pascal / Fortran / Ada.
//
// Build the extension:
//
//     LIBCLANG_PATH=/Library/Developer/CommandLineTools/usr/lib \
//     DYLD_FALLBACK_LIBRARY_PATH=$LIBCLANG_PATH \
//     RUSTFLAGS="-C link-arg=-undefined -C link-arg=dynamic_lookup" \
//       cargo build --release -p azul-dll --features php-extension
//
// Then run this script with the extension loaded:
//
//     php -d extension=path/to/libazul.dylib hello-world-ext.php
//
// On Linux replace .dylib with .so and pass
//     RUSTFLAGS="-C link-arg=-Wl,--unresolved-symbols=ignore-in-object-files"
// instead of the macOS dynamic_lookup flag.

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

// 1. Register the releaser with libazul. Idempotent — safe to call
// multiple times per request.
azul_host_invoker_init();
echo "[azul] azul_host_invoker_init() registered releaser.\n";

// 2. RefAny round-trip — proves the host-invoker handle table is
// reachable from PHP. Values are JSON-encoded for storage (Zvals are
// per-request-rooted and would dangle if held in a global table).
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

// 3. Codegen-driven per-kind register helpers. Phase 48 emits one
// `azul_register_<kind>_callback(string $name) : int` for every
// host-invoker callback kind. The function stashes the named PHP
// function in CALLBACKS and returns its handle id. Phase 50 wires
// the registered ids through libazul's AzApp_setGenericInvoker so
// libazul fires the PHP function when (e.g.) a button is clicked.

function on_button_click_smoke(string $args_json): string {
    $args = json_decode($args_json, true);
    return json_encode(['handled' => true, 'received' => $args]);
}

$button_cb_id = azul_register_button_on_click_callback('on_button_click_smoke');
$layout_cb_id = azul_register_layout_callback('on_button_click_smoke');
echo "[azul] azul_register_button_on_click_callback('on_button_click_smoke') = $button_cb_id.\n";

// 4. azul_invoke_callback — round-trips a stashed callable through
// the Zend executor from a Rust trampoline. This is the smoke layer
// for the full host-invoker dispatch path: libazul's generic-invoker
// trampoline (Phase 50) will call into the same lookup + try_call
// sequence, but synthesized from libazul's static thunks at runtime.
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

// 5. Dom class — Phase 51 introduces `Azul\Dom` as a real PHP class
// wrapping the C-ABI AzDom struct. Construct a body + a div + read
// back nodeCount through the Zend object boundary, proving:
//   * #[php_class] AzulDom registered via module.class::<...>()
//   * Static constructors (createBody/createDiv) reach libazul
//   * Instance methods call the C-ABI with `&self.inner`
//   * Drop fires AzDom_delete on PHP-side garbage collection
$body = Azul\Dom::createBody();
$div  = Azul\Dom::createDiv();
if ($body->nodeCount() !== 1 || $div->nodeCount() !== 1) {
    fwrite(STDERR, "[azul] FAIL: dom nodeCount mismatch: body=" . $body->nodeCount()
        . ", div=" . $div->nodeCount() . "\n");
    exit(1);
}
echo "[azul] Azul\\Dom::createBody()->nodeCount() = " . $body->nodeCount() . " (PHP class round-trip).\n";

echo "[azul] PHP host-invoker init phase completed successfully.\n";
echo "[azul] (azul_host_invoker_init also wired AzApp_setGenericInvoker —\n";
echo "[azul]  libazul's static callback thunks now route through the\n";
echo "[azul]  Rust trampoline into ZendCallable::try_call automatically.\n";
echo "[azul]  Full App::run dispatch lands when the Dom builders codegen\n";
echo "[azul]  ships in Phase 51.)\n";
