# Azul — Pascal (FPC / Lazarus)

⊘ **Blocked on libazul-side fix.** Codegen is complete and correct;
`hello-world` builds and runs through the host-invoker init, then
`AzApp_run` crashes inside libazul's webrender
`SceneBuilder::build_item` before the first paint. Reproduces with
an empty default WCO — libazul-side, not codegen. See
`memory/pascal_codegen_2026_05_13.md` for the full diagnosis.

## What works

- Build (`fpc -Mobjfpc -Sh -Fl. -k-L. -k-lazul hello-world.pas`).
- Host-invoker init (refany round-trip, releaser registration).
- Struct layouts match the C ABI byte-for-byte (cbool→ByteBool,
  repr(C, u8) tag width, DestructorOrClone field inclusion all
  fixed in commit `1f7f84a90`).

## What doesn't

- `AzApp_run` exits with `EAccessViolation` deep in libazul's
  webrender code on every macOS run. Will resume from this state
  once the libazul agent's macOS/aarch64 fix lands.

## Files

- `hello-world.pas` — full-GUI port (subclassing `TAzLayoutCallbackInvoker`).
- `azul.pas` — generated bindings.
- `hello-world.lpi` — Lazarus project file.
- `libazul.dylib` — prebuilt native library.
