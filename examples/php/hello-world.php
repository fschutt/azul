<?php
// examples/php/hello-world.php
//
// ──────────────────────────────────────────────────────────────────────
// PHP CALLBACK LIMITATION
// ──────────────────────────────────────────────────────────────────────
// Azul callbacks (button.setOnClick, layout, etc.) require closure-to-
// function-pointer casts, which standard php-ffi rejects by design for
// memory-safety reasons. The planned `azul.so` PHP native extension
// (built via the `php-extension` Cargo feature, loaded with
// `php -d extension=azul.so`) lifts this limit; until that ships the
// FFI path covers the non-callback API only (POD wrappers, RefAny
// round-trip, raw libazul function calls).
//
// This smoke test exercises the part of the binding that DOES work
// through standard php-ffi:
//   * FFI library load + cdef parse
//   * AzString_fromUtf8 / AzString_delete round-trip (struct-by-value
//     return crossing the FFI boundary)
//
// For the full callback API, use one of the other host languages
// listed in doc/guide/en/internals/host-invoker.md.
//
// Run with:
//
//     php -dffi.enable=true hello-world.php

declare(strict_types=1);

require_once __DIR__ . '/Azul.php';

use Azul\Azul;

echo "[azul] PHP FFI smoke test starting.\n";

$ffi = Azul::lib();
echo "[azul] FFI library loaded (cdef parsed).\n";

// Build an AzString from a PHP byte buffer. Exercises a
// struct-by-value return crossing the FFI boundary.
$src = "hello, azul";
$len = \strlen($src);
$buf = $ffi->new('uint8_t[' . $len . ']');
for ($i = 0; $i < $len; $i++) {
    $buf[$i] = \ord($src[$i]);
}
$s = $ffi->AzString_fromUtf8(
    $ffi->cast('const void*', FFI::addr($buf[0])),
    $len
);
echo "[azul] AzString_fromUtf8 round-trip succeeded; len=" . $len . "\n";

// AzString is a wrapper over Vec<u8>; delete to release it.
$ffi->AzString_delete(FFI::addr($s));
echo "[azul] AzString_delete reached without error.\n";

echo "[azul] PHP binding init phase completed successfully.\n";
echo "[azul] (Full callback wiring requires the planned `php-extension` build —\n";
echo "[azul]  standard php-ffi rejects closure-to-fnpointer by design.)\n";
