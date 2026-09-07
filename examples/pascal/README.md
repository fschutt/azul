# Azul — Pascal (FPC / Lazarus)

Full-GUI counter example, e2e-gated (`scripts/e2e_language_matrix.sh pascal`).

**The macOS "EAccessViolation in AzApp_run" is root-caused and fixed
(2026-09-07).** It was never a memory bug. The Free Pascal runtime unmasks
the InvalidOp, ZeroDivide and Overflow FPU exceptions at program start,
while libazul (Rust + C, IEEE-754 default environment) treats NaN and
±inf as ordinary values. The first `inf - inf` inside taffy's layout cache
compare trapped. On aarch64-darwin the kernel delivers a trapped FP
exception as SIGILL (ESR class 0x2C), which the FPC RTL prints as
`EAccessViolation: Access violation`. The generated `azul.pas` now calls
`SetExceptionMask([...all six...])` in its `initialization` block, exactly
what Delphi programs do before calling OpenGL/DirectX. If you re-enable FP
traps in your own code, do it only around code that never calls into azul.

## What works

- Build (`fpc -Mobjfpc -Sh -Fl. hello-world.pas`; `azul.pas` carries
  `{$linklib azul}`, so no `-k-lazul` is needed — only `-Fl.` for the
  library search path).
- Host-invoker init (refany round-trip, releaser registration).
- Struct layouts match the C ABI byte-for-byte (cbool→ByteBool,
  repr(C, u8) tag width, DestructorOrClone field inclusion all
  fixed in commit `1f7f84a90`).

## Files

- `hello-world.pas` — full-GUI port (subclassing `TAzLayoutCallbackInvoker`).
- `azul.pas` — generated bindings.
- `hello-world.lpi` — Lazarus project file.
- `libazul.dylib` / `libazul.so` / `azul.dll` — the native library, copied here by the e2e script (not tracked).

## Recent updates (2026-05-15/16)

- **R8 consume mechanism** (commit `dbc7d82b9`): `FOwned := False`
  in the codegen-emitted consume helper disarms the Pascal record's
  finalizer for by-value C calls. Drops the double-free risk from
  consuming-self method bodies.
