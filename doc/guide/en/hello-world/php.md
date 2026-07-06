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
  - examples/php/hello-world-ext.php
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

1. **`php-extension`** (`hello-world-ext.php`) — a native Zend extension
   compiled from Rust via [`ext-php-rs`](https://github.com/davidcole1340/ext-php-rs).
   The Zend engine can call a C function pointer back into PHP, so the
   host-invoker pattern (as used by Lua / Ruby / Perl / OCaml) works: libazul
   fires a per-kind Rust invoker, which calls your PHP functions by name. **This
   is the full-GUI path** and the one the counter demo uses.
2. **`php-ffi`** (`hello-world.php`) — pure [`php-ffi`](https://www.php.net/manual/en/book.ffi.php).
   POD-only: `php-ffi` rejects closure-to-function-pointer conversion, so it can
   render a static DOM but **cannot install callbacks**.

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
php -d extension=../../target/phpext/release/libazul.dylib hello-world-ext.php
```

## The program

`examples/php/hello-world-ext.php` builds a counter: a `32px` label showing the
count and an "Increase counter" button that bumps it.

```php
<?php
$model = ['counter' => 5];              // any PHP value works as the data model

$on_click = function ($data, $info) {
    $m = Azul\refany_get($data);
    if ($m === null) return Azul\AzUpdate\DoNothing();
    $m['counter']++;
    Azul\refany_set($data, $m);         // write the mutated model back
    return Azul\AzUpdate\RefreshDom();
};

$layout = function ($data, $info) {
    $m = Azul\refany_get($data);
    $body = Azul\AzDom_createBody();
    // ... build div{font-size:32px} > text(counter) + Button ...
    return $body;                       // returns a raw AzDom record
};
```

### How callbacks work

* **`Azul\refany_create($value)`** wraps a PHP value in an `AzRefAny` (an opaque
  host handle); **`Azul\refany_get($data)`** recovers it inside a callback. Pass
  the same model to the app and to the button so both see the same counter.
* **`Azul\register_callback('<Kind>', $fn)`** returns the matching `Az<Kind>`
  record. The generated invoker fires `$fn` and writes its return value back
  through the callback out-pointer — a layout callback returns a `Dom`, an
  on-click callback returns an `AzUpdate`.
* The `LayoutCallback` is installed into `window_state.layout_callback` before
  `App.run`.

## Status

The counter demo passes on the `php-extension` path (`ext-php-rs`), counter
5 → 6 → 8. The extension must be built with a **clean** target dir — incremental
rebuilds can leave a stale `zend_module_entry` that PHP rejects as an "invalid
library". See `examples/php/README.md` for the two-path split and the
`php-ffi` (POD-only, no-callback) fallback.
