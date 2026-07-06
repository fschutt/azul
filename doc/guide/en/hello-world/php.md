---
slug: hello-world/php
title: Hello World [PHP]
language: en
canonical_slug: hello-world/php
audience: external
maturity: wip
guide_order: 28
topic_only: false
prerequisites: [hello-world]
tracked_files:
  - api.json
  - examples/php/hello-world.php
default-search-keys:
  - App
  - AppConfig
  - Dom
  - Button
  - WindowCreateOptions
  - Update
---

# Hello World [PHP]

## Introduction

PHP has two binding paths, and only one supports callbacks:

1. **`php-extension`** (`hello-world.php`) — a native Zend extension
   compiled from Rust via [`ext-php-rs`](https://github.com/davidcole1340/ext-php-rs).
   The Zend engine can call a C function pointer back into PHP, so the
   host-invoker pattern (as used by Lua / Ruby / Perl / OCaml) works: libazul
   fires a per-kind Rust invoker, which calls your PHP functions **by name**.
   **This is the full-GUI path** and the one the counter demo uses.
2. **`php-ffi`** — pure [`php-ffi`](https://www.php.net/manual/en/book.ffi.php).
   POD-only: `php-ffi` rejects closure-to-function-pointer conversion, so it can
   render a static DOM but **cannot install callbacks**. See
   `examples/php/hello-world-ext.php` for a smoke test that exercises the
   extension's round-trip primitives without running the event loop.

## Installation

You need **PHP 8.1+**. The `php-extension` path is compiled as a separate
`azul-dll` target so it doesn't clobber the desktop `libazul` the other
examples use. On macOS the extension needs `LIBCLANG_PATH` and a
dynamic-lookup link flag (the symbols resolve against the host PHP at load):

```sh
LIBCLANG_PATH=/Library/Developer/CommandLineTools/usr/lib \
DYLD_FALLBACK_LIBRARY_PATH=$LIBCLANG_PATH \
RUSTFLAGS="-C link-arg=-undefined -C link-arg=dynamic_lookup" \
  cargo build --release -p azul-dll \
    --features php-extension,debug-server --target-dir target/phpext
```

On macOS, freshly built dylibs must be re-signed before a hardened-runtime PHP
will load them:

```sh
codesign -f -s - target/phpext/release/libazul.dylib
```

## Running

Load the extension and run the driver:

```sh
cd examples/php
php -d extension=../../target/phpext/release/libazul.dylib hello-world.php
```

## The program

`examples/php/hello-world.php` builds a counter: a label showing the count and
a clickable "Increase counter" element that bumps it. Because the extension
routes callbacks back into PHP **by function name**, the callbacks are
top-level named functions (`on_click`, `layout`) — not closures.

```php
<?php
azul_counter_init();

// The model is JSON-encoded behind a host-handle id (Zvals are per-request
// rooted, so a raw value can't be held in libazul's global handle table).
$model_id = azul_refany_create(json_encode(['counter' => 5]));

$GLOBALS['azul_onclick_id'] = azul_register_callback('on_click');
$layout_id                  = azul_register_layout_callback('layout');

function on_click(int $data): int {
    $m = json_decode(azul_refany_get($data), true);
    $m['counter'] = ($m['counter'] ?? 0) + 1;
    azul_refany_set($data, json_encode($m));   // write the mutated model back
    return 1;                                  // Update::RefreshDom
}

function layout(int $data): \Azul\Dom {
    $m   = json_decode(azul_refany_get($data), true);
    $div = \Azul\Dom::createDiv();
    $div->addChild(\Azul\Dom::createText((string) ($m['counter'] ?? 0)));

    $btn = \Azul\Dom::createDiv();
    $btn->addChild(\Azul\Dom::createText('Increase counter'));
    $btn->onClick($data, $GLOBALS['azul_onclick_id']);

    $body = \Azul\Dom::createBody();
    $body->addChild($div);
    $body->addChild($btn);
    return $body;                              // returns an Azul\Dom object
}

$wco = \Azul\WindowCreateOptions::create($layout_id);
$app = \Azul\App::create($model_id, \Azul\AppConfig::create());
$app->run($wco);
```

### How callbacks work

* **`azul_refany_create($json)`** stores a value behind an `AzRefAny` host
  handle and returns its integer id; **`azul_refany_get($id)`** /
  **`azul_refany_set($id, $json)`** read and write it inside a callback. Pass
  the same id to the app and to the clickable node so both see the same counter.
* **`azul_register_callback('<fn>')`** and
  **`azul_register_layout_callback('<fn>')`** stash a named PHP function and
  return its handle id. libazul fires a per-kind Rust invoker that calls that
  PHP function through the Zend executor — the on-click function returns an
  `int` (`Update`), the layout function returns an `Azul\Dom`.
* The `LayoutCallback` id is threaded through
  `\Azul\WindowCreateOptions::create($layout_id)` before `App::run`.

## Status

The counter demo passes on the `php-extension` path (`ext-php-rs`), counter
5 → 6 → 8. The extension must be built with a **clean** target dir — incremental
rebuilds can leave a stale `zend_module_entry` that PHP rejects as an "invalid
library". See `examples/php/README.md` for the two-path split and the
`php-ffi` (POD-only, no-callback) fallback.
