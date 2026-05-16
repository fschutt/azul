# Azul — Smalltalk (GNU Smalltalk / Pharo)

⊘ **Pharo Tonel layout blocker.** Codegen emits a `Azul.st` file
that GNU Smalltalk (`gst`) accepts for the smoke layer, but Pharo's
Tonel package format expects a directory layout the codegen doesn't
currently produce.

## Status

- GNU Smalltalk smoke test runs.
- Pharo full-GUI: blocked on the Tonel-package emission rewrite.

## Files

- `HelloWorld.st` — smoke test.
- `Azul.st` — generated bindings (single-file).
- `libazul.dylib` — prebuilt native library.

## Recent updates (2026-05-15/16)

- **R14 consume mechanism** (commit `7f39e0c03`): `handle := nil`
  in the codegen-emitted consume method clears the wrapper's
  ExternalHandle slot so finalisation skips the
  `Az<X>_delete` call. Closes the double-free for by-value C calls.
