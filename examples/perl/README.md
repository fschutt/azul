# Azul — Perl

🟡 **Host-invoker smoke layer works.** Full GUI E2E blocked on
codegen for callback-return marshalling — `lang_perl/managed.rs`
invoker drops `out_ptr` from the user sub and declares the
Platypus closure return as `'void'`. See
`memory/perl_layout_callback_2026_05_13.md` for the two-path fix
plan.

## Requirements

- Perl 5.30+ (Homebrew `/opt/homebrew/bin/perl` recommended)
- `FFI::Platypus` 2.x

## Run

```sh
/opt/homebrew/bin/perl hello-world.pl
```

(System Perl on macOS lacks write permission to its site_perl;
install Platypus into Homebrew Perl.)

## Files

- `hello-world.pl` — smoke test (AzString round-trip + RefAny).
- `lib/Azul.pm` — generated bindings.
- `libazul.dylib` — prebuilt native library.

## Recent updates (2026-05-15/16)

- **R13 consume mechanism** (commit `7f39e0c03`): `$$self = undef`
  in the codegen-emitted consume helper invalidates the wrapper's
  internal ref slot so the DESTROY method's `Az<X>_delete` is a
  no-op. Closes the double-free for by-value C calls.
