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

echo "[azul] PHP extension binding init phase completed successfully.\n";
echo "[azul] (Full host-invoker surface — azul_refany_create,\n";
echo "[azul]  azul_register_callback, the Dom builders — lands when the\n";
echo "[azul]  PHP codegen pass writes to target/codegen/php_api.rs.)\n";
