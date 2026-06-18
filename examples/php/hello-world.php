<?php
// php -dffi.enable=true hello-world.php

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
