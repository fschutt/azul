# The relocation cache spliced the wrong callee

Status: root cause identified and evidenced · fix specified, not yet implemented.

## What went wrong

The relocation-canonical lift cache reuses one function's lifted IR for another
build by templating: mask the address-derived fields, record a *slot identity*
per masked site, and at translate time re-resolve each identity to this build's
address and splice it back in.

If two different targets produce the **same identity string**, the splice puts
the first-seen target into every function that shares it. The result is a
function that calls something else entirely — and it is silent, because the
spliced IR is perfectly well-formed.

## Evidence

`AZ_RELOC_VERIFY=1` fresh-lifts every translated function and byte-diffs.
Across 183 divergence pairs from one run:

| class | count | example |
|---|---|---|
| value only | 171 | `add i64 %x, 7133545` vs `add i64 %x, 10663641` |
| operator differs | 12 | fresh `sub i64 %x, 1372531` vs translated `add i64 %x, 832189` |
| **wrong callee** | — | fresh `@sub_bfb0d0` / `@sub_bfd1d0` / `@sub_bf23b0` → **all** translated to `@sub_bf2270` |

Three distinct callees collapsing onto one is a collision, not an arithmetic
slip.

Concretely, this is what broke AzWriter's boot for roughly ten runs.
`<str as Display>::fmt` is 32 bytes and ends in a direct `jmp rel32` whose bytes
(`e9 8e 81 3c 00`) target `Formatter::pad`. Its cached IR jumped somewhere else
entirely — the lifted IR provably disagreed with its own instruction bytes.

## Cause, in the source

`transpiler_remill.rs`, identity construction:

```rust
// exact symbol hit — NO fingerprint
return format!("{}+0x{:x}", e.canonical_name, off);

// anonymous data — fingerprinted
return format!("near:{}+0x{:x}:{}", e.canonical_name, off,
               fnv1a64_hex(pointee /* 16 bytes */));
```

Only the anonymous-data branch verifies content. The exact-symbol branch keys
purely on `name + offset` — and **duplicate-named monomorphizations and
ICF-folded copies share a `canonical_name`**. Translate-side lookup by name
returns a single address, so every caller of every same-named copy is handed
the same one.

Tiny tail-call thunks are the worst case: their masked bytes are identical, so
they also share the canonical byte hash, leaving the ident hash as the only
discriminator — exactly the thing that collides.

A second, weaker hole: even where a fingerprint exists, **16 bytes of a
function prologue is not distinguishing**. Rust prologues repeat.

## Fix

1. **Fingerprint the exact-symbol identity too.** Append a content hash and the
   symbol size, so a name collision fails verification at translate time and
   misses into a fresh lift. A miss costs one remill invocation; a wrong splice
   costs a silent wrong-address call.
2. **Widen the fingerprint for code targets** to `min(size, 64)` bytes plus the
   size, so repeated prologues stop matching.
3. **Make sign-flip sites untemplatable.** A splice replaces a token but cannot
   change the text around it, so a site whose operator differs between the two
   probe lifts must disqualify the function rather than be templated.

Then re-run with `AZ_RELOC_VERIFY=1` and require the divergence count to reach
~0 before trusting the cache again.

## Operational rule

`AZ_RELOC_VERIFY=1` is not optional for a debugging run. It was switched off on
every AzWriter run to save remill time, which is precisely why this survived so
long and cost several days of forensics on symptoms. It roughly doubles remill
for one run; a silent wrong-address lift costs far more than that.
