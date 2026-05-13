# Azul — Ruby

Ruby bindings for the [Azul](https://azul.rs) GUI framework via the
`ffi` gem.

## Status

✅ **Full GUI E2E** — counter probe 5→8 via `AZ_DEBUG` verified.

## Requirements

- Ruby 2.6+ (system Ruby on macOS works)
- `ffi` gem (`gem install --user-install ffi -v 1.15.5`)
- `libazul.dylib` in the working directory

## Build + Run

```sh
ruby -I. hello-world.rb
```

(`-I.` tells Ruby to look in the current dir for `azul.rb`.)

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
  (via `read_array_of_<type>` for primitives, slot-walk for structs).
- Enum constants: `Azul::Update::RefreshDom`, `Azul::ButtonType::Primary`.

## Notes

- `CssProperty` is a tagged-union without a Ruby wrapper class today.
  Use `Azul::Native.az_css_property_*` directly for now.
- The codegen's `_register_callback` helper auto-fires inside the
  consuming-form builders (`with_on_click` etc.), so the smart
  `on_click` only wraps `data` — no double-register.

## Files

- `hello-world.rb` — 69-line Python-quality port.
- `azul.rb` — generated bindings.
- `Gemfile` — gem dependencies.
- `libazul.dylib` — prebuilt native library.
