# Azul — Ruby

Ruby bindings for the [Azul](https://azul.rs) GUI framework via the
`ffi` gem.

## Status

✅ **Full GUI E2E** — counter probe 5→8 via `AZ_DEBUG` verified
(the e2e harness copies the current `target/codegen/azul.rb` over the
vendored copy before running, so the pass tracks the live artifact).

## Requirements

- Ruby 2.6+ (system Ruby on macOS works)
- `ffi` gem (`gem install --user-install ffi -v 1.15.5`)
- `libazul.dylib` in the working directory

## Build + Run

```sh
AZ_LIB_DIR=. ruby -I. hello-world.rb
```

(`-I.` tells Ruby to look in the current dir for `azul.rb`; `AZ_LIB_DIR=.`
points the loader at `libazul.dylib`/`libazul.so` in the working directory.)

## What's idiomatic

- `Azul::WindowCreateOptions.create_with_layout(lambda)` smart
  factory; accepts a Proc, a lambda, or `&block`.
- `Azul::Button.create(...).with_button_type(...).on_click(model, fn)`
  — `model` is any Ruby object (auto-refany-wrapped via
  `Azul::RefAny.wrap`); `fn` is a Proc.
- `Azul::String#to_s` decodes the UTF-8 bytes into a Ruby `String`.
- `Azul::Option<T>#to_opt` returns `nil` or the payload.
- `Azul::Result<T,E>#unwrap` raises on Err, returns Ok payload.
- `Azul::Vec<T>#to_a` reads the elements into a Ruby `Array`
  (via `read_array_of_<type>` for primitives, per-elem clone for structs).
- Enum constants: `Azul::Update::RefreshDom`, `Azul::ButtonType::Primary`.
- Fluent `.with(opts_hash)` builder: any struct wrapper accepts a
  nested-hash opts argument, recursively assigns FFI fields, and
  auto-converts Ruby strings to AzString. Drops the
  `window.ptr[:window_state][:title] = Azul._az_string(...)` drilling.

## Recent updates (2026-05-15/16)

- **Memory-safety arc closed** (commits `654b8cbd8` Option/Result
  delete+clone, `bb06ba101` Vec iter clone, plus `Azul._consume`
  finalizer-disarm in the JVM/CLR pass).
- **CC-4 `.with(opts)` fluent builder** (commit `fa0b5f06b`):
  drops field-drilling boilerplate from hello-world (62 → 67 LOC
  but reads cleanly as nested hash).

## Notes

- Style nodes with plain CSS strings: `Azul::Dom.create_text('5').with_css('font-size: 32px;')`.
  (No need to build `CssProperty` unions via `Azul::Native.az_css_property_*` —
  `Dom#with_css` parses the string for you.)
- The codegen's `_register_callback` helper auto-fires inside the
  consuming-form builders (`with_on_click` etc.), so the smart
  `on_click` only wraps `data` — no double-register.

## Files

- `hello-world.rb` — 48-line counter example (Python parity).
- `azul.rb` — generated bindings.
- `Gemfile` — gem dependencies.
- `libazul.dylib` — prebuilt native library.
