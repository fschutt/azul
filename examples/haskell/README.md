# Azul — Haskell

⊘ **C shim layer needed.** GHC's FFI rejects struct-by-value returns
from foreign-imported functions, which AzDom / AzString / AzOptionRefAny
all use. A per-callback-kind C shim that converts by-value returns to
out-pointer writes is the standard workaround; not yet emitted by
`lang_haskell/managed.rs`.

## Status

- Smoke test compiles and runs.
- Full GUI: blocked on the C shim layer (~2–3 days of focused codegen
  work).

## Files

- `Main.hs` — smoke test.
- `azul.hs` — generated bindings (partial — struct-return functions
  are flagged but not callable).
- `cabal` / `stack.yaml` build configs.
- `libazul.dylib` — prebuilt native library.
