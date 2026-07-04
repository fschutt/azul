# Azul — Node.js / Bun / Deno

JavaScript bindings for the [Azul](https://azul.rs) GUI framework
via [koffi](https://koffi.dev/) (Node) / `bun:ffi` (Bun) /
`Deno.UnsafeCallback` (Deno).

## Status

✅ **Full GUI E2E (Node.js + koffi, macOS)** — counter probe 5→8 via
`AZ_DEBUG`, re-verified 2026-07-04 in the repo harness (the harness
copies `libazul.dylib` next to `azul.js`; the regenerated loader now
resolves the library the same way for end-user installs). Two
2026-07-04 review blockers are fixed in codegen: the
`koffi.load('azul')` bare-name loader (dlopen does no lib-prefix/suffix
mangling) and the smart `on_*` setter double-registration TypeError.

⚠️ Bun and Deno share the same `azul.js` but are **experimental**:
their invoker branches do not write callback return values back to
native memory yet (`runtime === 'node-koffi'` gates), so real apps
should use Node.js for now.

## Requirements

- Node.js 16+
- `koffi` package (`npm install koffi`)
- `libazul.dylib` (macOS) / `libazul.so` (Linux) / `azul.dll` (Windows)

The library is resolved in this order: `$AZ_LIB` (explicit file path),
the directory containing `azul.js`, `$AZ_LIB_DIR`, the working
directory, then the system loader search path. Dropping the library
next to `azul.js` is the simplest setup.

## Build + Run

```sh
npm install koffi   # once
node hello-world.js
```

For Bun: `bun run hello-world.js`. For Deno:
`deno run --allow-ffi --unstable-ffi hello-world.js` (both
experimental, see Status).

## What's idiomatic

- `azul.WindowCreateOptions.createWithLayout(fn)` smart factory.
  Hides the host-invoker register + the
  `opts.window_state.layout_callback = cb` splice.
- `button.on_click(data, fn)` — wraps `data` via `refanyCreate`
  and `fn` via `registerCallback('ButtonOnClickCallback', fn)`
  internally.
- `azulStr.toString()` — UTF-8 decode (Node/koffi only).
- `azul.Update.RefreshDom` enum constants (top-level on the module).
- `azul.optionToNullable(opt)` — module-level helper since koffi
  unions can't carry methods.
- `azul.resultUnwrap(res, name)` — throws on Err.
- Fluent `.with(opts)` builder on every struct wrapper: recursively
  assigns nested koffi struct fields, auto-converting JS strings
  to AzString. Drops the
  `window.window_state.title = azul._azString('...')` drilling.

## Recent updates

- **2026-07-04 review fixes (codegen)**: platform-filename loader
  with same-dir / `AZ_LIB_DIR` / cwd resolution; `registerCallback`
  passes through already-registered callback structs (unbreaks every
  smart `on_*` setter); invoker catch blocks re-write a safe default
  return (`Update.DoNothing` / default `Dom`); duplicate
  `toString(instance)` emission and phantom `instance` params dropped;
  package.json bumped to 0.2.0.
- **Memory-safety arc closed** (2026-05): `_consume` (`8241735fd`),
  Option/Result delete+clone (`f935bf50e`), Vec iter clone
  (`e56d41caf`), static-factory consume (`8241735fd`).
- **CC-4 `.with(opts)` builder** (`070a3c946`): see "What's
  idiomatic" above.

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

- `hello-world.js` — ~56-line idiomatic counter example.
- `azul.js` — 9.2 MB generated binding (Node verified; Bun/Deno experimental).
- `package.json` — koffi dependency.
- `libazul.dylib` — prebuilt native library.
