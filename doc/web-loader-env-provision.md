# Every env import has three halves, and all three drifted

A lifted wasm reaches the browser through a contract with three parts. A name
has to be:

1. **declared** in the emitted IR, so the call has something to bind to;
2. **bound by the loader** — by the import builder for *the module that imports
   it*, not by some other module's builder;
3. **listed in the audit**, so an unbound name is refused rather than served.

Miss (1) and the lift fails loudly. Miss (2) and the loader's Proxy answers with
a shape-appropriate zero: silent, and wrong in whatever way a zero is wrong for
that function. Miss (3) and nothing tells you (2) happened.

All three had drifted, and each was found by checking a *different* half against
the others.

## The audit's complaint was the small half

Run 72 lifted 4885 functions and then refused to serve:

```
✗ F3 mini/layout/cb: 4 env import(s) the loader does not implement
  (zero-stubbed at runtime): env.log2, env.log10, env.log2f, env.log10f
```

Those four were half (3): the loader had bound `log2`/`log10` since the CRT
transcendentals were routed, and `import_is_provided` had never been updated.

## The defect behind it

`azMakeMiniImports` carried the libc math table. `azCallbackImports` — which
instantiates the **layout** wasm and every callback wasm — carried none of it.
So every math libcall in those modules fell through to the Proxy and returned 0.

The mini's own comment on that table states the cost exactly:

> These MUST be real — the layout solver floors every used size with
> `.max(0.0)`, so a 0-returning stub zeroes ALL widths/heights.

The audit could not see it because `import_is_provided` is **one global list**.
It answers "is this name provided *somewhere*", not "by the module that imports
it". A whole module missing the whole table is invisible to a check with that
granularity, while four names missing from its own list are fatal. A check with
the wrong granularity is not a weak check, it is a blind one.

The table now lives once, in `azMathEnv()`, and both import builders assign it.
One definition is what makes "a module is missing the table" unrepresentable
rather than merely unlikely — two copies is exactly how this happened.

## Then the list turned out to be aspirational

Listing what the modules actually import (`scripts/m9_e2e/wasm-imports.py`)
turned up the opposite failure. `PROVIDED_ENV` claimed `__divti3`, `__umodti3`,
`__modti3` and the whole inverse-trig family were provided. **The loader
implemented none of them.**

`transitive-lift.wasm` really imports `__divti3`, so this was live, not
hypothetical — and it is the worse direction:

> A name listed as provided but unbound is worse than an unlisted one. Unlisted
> surfaces as a loud F3; listed-but-unbound is a silent zero.

Worse still for `__divti3`'s shape: it returns its `i128` result through an
`sret` pointer, so the Proxy stub returns 0 **and writes nothing**, leaving the
caller to read whatever was already in that memory.

Fixed by implementing them rather than by trimming the list — these are names
lifted code can legitimately need, and the implementations are exact:

- BigInt `/` and `%` truncate toward zero and `%` keeps the dividend's sign,
  which is C's rule for signed integers.
- JS `Math` is IEEE double math, so the inverse trig is exact, not approximated.
- A zero divisor returns 0, matching `azUdivti3`: real compiler-rt is UB there,
  and killing the page is worse than a defined wrong answer.

## `__remill_*` was blanket-approved

`import_is_provided` returned `true` for the whole `__remill_` prefix, so it
never mentioned that `azRemillIntrinsics` binds 13 names while the artifacts
import 19. The six that fall to the Proxy:

```
__remill_async_hyper_call
__remill_read_io_port_8    __remill_read_io_port_32
__remill_write_io_port_8   __remill_write_io_port_32
__remill_undefined_8
```

These are IN/OUT and hypercall semantics, which userland Rust never reaches, so
they are **not implemented here**. What changed is that the assumption is now
visible: they are named in a `W6` warning instead of being built into a prefix
test. A zero is right for a value-returning intrinsic and wrong for one that
threads remill's Memory token through, and that distinction should be a decision
someone made, not a side effect of string matching.

Kept a warning rather than a refusal: a fatal here would refuse to serve over
names nothing calls.

## What stops it recurring

Two hand-maintained lists drifting apart is the defect, so the fix is a test,
not vigilance. `provided_env_matches_loader` asserts every name in
`PROVIDED_ENV` and `REMILL_BOUND` appears as a property key in
`generate_loader_js()`'s output. It catches exactly the direction that used to
be silent — the audit claiming something the loader does not bind.

It does **not** catch the other direction (the loader binds something the audit
does not list), because that one already surfaces loudly as F3.

## Running the drift test

```
cargo test --release -p azul-dll --features web-transpiler --lib provided_env_matches_loader
```

Note the package. The test lives in crate `azul`, which is package **azul-dll** —
running it against `-p AzWriter` prints

```
test result: ok. 0 passed; 0 failed; 0 ignored; 42 filtered out
```

because the filter matches nothing in AzWriter's own lib. **"0 passed" is a
failure**, not a pass, and it looks exactly like success in a scrollback. Also do
not pass `--no-default-features`: azul-layout then loses the feature that
provides `azul_layout::icu` and the build fails before any test runs.

Passing looks like `1 passed; 0 failed; ...; 2154 filtered out`.

## Checking a module by hand

`scripts/m9_e2e/wasm-imports.py <file.wasm> [...]` lists a module's imports,
collapsing the `sub_<hex>` boundary imports so the hand-written names stay
readable. Comparing that against the builder that instantiates that particular
module is the check with the right granularity — it is what found all three of
the above.

Artifacts live in the run's scratch: `azul-mini.wasm` and
`transitive-lift.wasm` under `%TEMP%\azul-web-transpiler-<pid>`. They are
written even on a run the audit refuses to serve.
