# The relocation cache translated cross-function targets by the wrong delta

Status: root cause identified and evidenced · fix specified, not yet implemented.

## What went wrong

The relocation-canonical lift cache reuses one function's lifted IR in a later
build by templating: mask the address-derived fields, record a *slot identity*
per masked site, and at translate time re-resolve each identity to this build's
address and splice it back in.

One identity kind, `@delta`, resolves as:

```rust
old_va.wrapping_add(new_lift_addr.wrapping_sub(old_lift_addr))
```

That is: *move the target by however far this function moved*. It is correct
only when the target moved exactly as far as the function containing it —
true for a target inside the same function, and a gamble for anything else.
Two functions move independently between builds, so a cross-function `@delta`
slot produces a well-formed, plausible, **wrong** address.

## Evidence

`AZ_RELOC_VERIFY=1` fresh-lifts every translated function and byte-diffs it.
The thunk that blocked AzWriter's boot for roughly ten runs:

```
DIVERGENCE in core::fmt::impl$75::fmt<str$>
  fresh:   %26 = add i64 %25, 3965326
  trans:   %26 = add i64 %25, 5980222
```

`<str as Display>::fmt` is 32 bytes and ends in a direct `jmp rel32` whose
bytes are `e9 8e 81 3c 00` — displacement 3,965,326, targeting
`Formatter::pad`. The fresh lift reproduces that exactly. The cached
translation used 5,980,222 and jumped into `Vec::Drain::drop` instead. The
error, 2,014,896, is the drift between how far the caller moved and how far
`Formatter::pad` moved.

Scale: **66,285 manifests carry a `@delta` slot**, including `s`-kind slots,
which are *callee names* translated by the caller's own delta.

Across 183 divergence pairs from one run:

| class | count | harmful? |
|---|---|---|
| value differs | 171 | **yes** — a wrong address, the class above |
| operator differs (`sub` vs `add`) | 12 | **yes** — a splice replaces a token but cannot change surrounding text, so sign-flip sites are untemplatable |
| callee name differs | some | **no** — see below |

## What is NOT the bug

An earlier revision of this note blamed the callee-name swaps, where three
distinct callees all translated to `sub_bf2270`. That is benign. Checking the
bytes shows all four are ICF/monomorphization duplicates and byte-identical, so
calling any of them is equivalent — the "ICF-equivalent duplicate-copy swap"
class already known to be harmless.

It also blamed the exact-symbol identity for carrying no fingerprint. That
cannot cause a wrong splice either: `reloc_translate` handles `@rel`, `@delta`
and `near:` and everything else falls to `return None`, so a bare
`name+0x{off}` identity is a cache *miss*, not a mistranslation. It costs hit
rate, nothing more.

The `@rel` class is sound too — sweeping every `@rel` slot in the cache
(3,086,415 of them) found none whose offset reaches beyond its own function.

## Fix

1. **Restrict `@delta` to intra-function targets**, where it degenerates to
   `@rel` and is correct by construction. A cross-function target must use a
   verifiable identity (symbol plus content fingerprint) or disqualify the
   function from templating. A miss costs one remill invocation; a wrong splice
   costs a silent call to the wrong address.
2. **Make sign-flip sites untemplatable**: if the operator around a slot
   differs between the two probe lifts, the function cannot be templated.
3. **Widen the `near:` fingerprint for code targets** to `min(size, 64)` bytes
   plus the symbol size — 16 bytes of a Rust prologue is not distinguishing.

Then re-run with `AZ_RELOC_VERIFY=1` and require the divergence count to reach
~0 before trusting the cache again.

## Operational rule

`AZ_RELOC_VERIFY=1` is not optional for a debugging run. It was switched off on
every AzWriter run to save remill time, which is precisely why this survived so
long and cost several days of forensics on symptoms. It roughly doubles remill
for one run; a silent wrong-address lift costs far more.
