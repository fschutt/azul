# Azul — PHP (native extension via ext-php-rs)

PHP bindings for the [Azul](https://azul.rs) GUI framework. Two paths
coexist:

1. **php-extension** (`hello-world-ext.php`) — native Zend extension
   compiled from Rust via `ext-php-rs`. Supports closures, host-invoker
   callbacks, the full thing.
2. **php-ffi** (`hello-world.php`) — pure `php-ffi`. POD-only;
   `php-ffi` rejects closures-to-fnpointer, so no callback support.
   Kept as a smoke test.

## Status

🟡 **Host-invoker smoke layer** verified (build + load + Azul\Dom
class round-trip). **Full App::run is pending Phase 51** Dom-builders
+ App.run codegen.

## Requirements (extension path)

- PHP 8.5+
- The `ext-php-rs` Rust crate (vendored in libazul under the
  `php-extension` Cargo feature)
- Xcode Command Line Tools (provides libclang at
  `/Library/Developer/CommandLineTools/usr/lib`)
- Rust nightly or stable

## Build the extension

```sh
LIBCLANG_PATH=/Library/Developer/CommandLineTools/usr/lib \
DYLD_FALLBACK_LIBRARY_PATH=$LIBCLANG_PATH \
RUSTFLAGS="-C link-arg=-undefined -C link-arg=dynamic_lookup" \
    cargo build -p azul-dll --features php-extension
cp ../../target/debug/libazul.dylib libazul-ext.dylib
```

On Linux replace the macOS `dynamic_lookup` RUSTFLAGS with
`-C link-arg=-Wl,--unresolved-symbols=ignore-in-object-files`.

## Run

```sh
php -d extension=./libazul-ext.dylib hello-world-ext.php
```

## What works today

- 25 `azul_*` functions exposed via `get_extension_funcs('azul-dll')`.
- `azul_refany_create` / `azul_refany_get` round-trip Ruby/JSON
  values through libazul's host-handle table.
- `azul_register_<kind>_callback(string $name)` stashes a named PHP
  function; `azul_invoke_callback($id, $args_json)` round-trips it
  through `ZendCallable::try_from_name`.
- `Azul\Dom::createBody()->nodeCount()` PHP-class round-trip via
  `#[php_class]`.

## What's pending (Phase 51)

- Dom builder methods (`createDiv`, `withChild`, `withCss`, etc.)
  on the PHP class.
- `Azul\App::create($data, $config)` + `Azul\App::run(WCO)`.
- Smart `Azul\WindowCreateOptions::create(callable $layout)`.

## Files

- `hello-world-ext.php` — extension-path smoke test.
- `hello-world.php` — legacy php-ffi smoke test.
- `Azul.php` — generated PHP class shims (6.3 MB).
- `composer.json` — composer metadata.
- `libazul-ext.dylib` — prebuilt extension (91 MB; debug build).
- `libazul.dylib` — prebuilt cdylib for the legacy ffi path.

## Notes

PHP is the only binding where the codegen lives in
`doc/src/codegen/v2/lang_php_ext.rs` rather than `lang_php/`. The
ext-php-rs route is the only way to get host-invoker (closure)
callbacks into PHP — standard `php-ffi` rejects them.

## Recent updates (2026-05-15/16)

- **R15 consume mechanism** (commit `7f39e0c03`): `$this->ptr = null`
  in the codegen-emitted consume helper clears the wrapper's
  internal pointer so `__destruct` skips the `Az<X>_delete` call.
  Also closes the self-by-value double-free in the
  `WindowCreateOptions::create` smart factory.
