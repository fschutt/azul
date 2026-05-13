# Azul — Node.js / Bun / Deno

JavaScript bindings for the [Azul](https://azul.rs) GUI framework
via [koffi](https://koffi.dev/) (Node) / `bun:ffi` (Bun) /
`Deno.UnsafeCallback` (Deno).

## Status

✅ **Full GUI E2E** — counter probe 5→8 via `AZ_DEBUG` verified
under Node.js + koffi. Bun and Deno paths share the same `azul.js`
but Bun-specific gates (`runtime !== 'node-koffi'`) skip the
koffi-only decode paths.

## Requirements

- Node.js 16+
- `koffi` package (`npm install koffi`)
- `libazul.dylib` in the working directory

## Build + Run

```sh
node hello-world.js
```

For Bun: `bun run hello-world.js`. For Deno:
`deno run --allow-ffi --unstable-ffi hello-world.js`.

## What's idiomatic

- `azul.WindowCreateOptions.createWithLayout(fn)` smart factory.
  Hides the host-invoker register + the
  `opts.window_state.layout_callback = cb` splice.
- `button.onClick(data, fn)` — wraps `data` via `refanyCreate`
  and `fn` via `registerCallback('Callback', fn)` internally.
- `azulStr.toString()` — UTF-8 decode (Node/koffi only).
- `azul.Update.RefreshDom` enum constants (top-level on the module).
- `azul.optionToNullable(opt)` — module-level helper since koffi
  unions can't carry methods.
- `azul.resultUnwrap(res, name)` — throws on Err.

## Caveats

- AzOption / AzResult / AzVec accessor methods aren't attached to
  the koffi union types (koffi materialises them as plain objects
  with no prototype). Use the `azul.optionToNullable` etc. module
  helpers instead of `.toNullable()`.
- `process.on('uncaughtException', ...)` in `hello-world.js` is a
  safety net for koffi callback exceptions — the host-invoker
  thunk's own try/catch catches most of them, but SIGABRT before
  return still wants a logger.

## Files

- `hello-world.js` — 108-line idiomatic port (uses smart factory implicitly via direct field assignment).
- `azul.js` — 6.8 MB generated binding (covers Node/Bun/Deno runtimes).
- `package.json` — koffi dependency.
- `libazul.dylib` — prebuilt native library.
