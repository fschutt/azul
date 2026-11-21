<?php
// Copy target/codegen/Azul.php and libazul here, then: php -d ffi.enable=1 main.php
require_once __DIR__ . '/styles.php';

$style_btn = style_btn();
echo 'style_btn: ' . $style_btn->len . " properties\n";
