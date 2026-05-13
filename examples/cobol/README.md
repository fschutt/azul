# Azul — COBOL (GnuCOBOL)

🟡 **Smoke-test tier.** The codegen emits a copybook with FFI bindings
that GnuCOBOL accepts; full GUI requires user-side ENTRY paragraph
wiring that's outside the codegen scope.

## Requirements

- GnuCOBOL 3.1+ (`brew install gnu-cobol`)
- `libazul.dylib` reachable at link time

## Files

- `hello-world.cbl` — smoke test (refany round-trip).
- `azul.cpy` — generated copybook.
- `Makefile` — `cobc` build invocation.
- `libazul.dylib` — prebuilt native library.

## Notes

The user-facing API surface is a callable copybook the user `COPY`s
into their program; FN-* aliases for each binding function are
emitted but ENTRY paragraphs for callbacks remain hand-written.
