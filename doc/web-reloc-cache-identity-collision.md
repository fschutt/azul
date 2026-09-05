# The relocation cache spliced stale pc-relative displacements

Status: two distinct defects, found in that order.

1. `@delta` resolved cross-function targets by the caller's move distance.
   Fixed by the probe-space membership test.
2. **A masked field that the template never re-resolves.** This one is the
   reason `<str as Display>::fmt` still jumped to a wrong address after (1)
   landed. Fixed by the `@disp` slot kind below.

## The mechanism

The relocation-canonical lift cache reuses one function's lifted IR in a later
build by templating: mask the address-derived fields, record a *slot identity*
per masked site, and at translate time re-resolve each identity to this build's
address and splice it back in.

`reloc_canonicalize` masks a cross-function branch by **zeroing its `rel32` out
of the hashed key** and recording the target by symbol identity. Two builds
whose callee moved relative to its caller therefore hash the same and hit.

`reloc_templateize` builds the template by diffing two probe lifts of the same
bytes at two lift addresses, and slots exactly the tokens that **differ**
between them.

Those two rules contradict each other. A pc-relative displacement is derived
from the bytes, so both probes decode it to the *same* number — it is never a
slot, and stays in the template as literal text. Masked out of the key but
never re-resolved is a stale value by construction.

## Evidence

`AZ_RELOC_VERIFY=1` fresh-lifts every translated function and byte-diffs it.
Every divergence has the same shape — a wrong constant in pc arithmetic:

```
DIVERGENCE in AzStartup_setFallbackFont
  fresh:   %65 = add i64 %64, 523480
  trans:   %65 = add i64 %64, 614312
```

Sampling 400 stored templates, of the `add i64 %N, <constant>` sites in them:

| | count |
|---|---|
| covered by a slot | **0** |
| frozen literal text | **2,726** |

Slots only ever covered `sub_<hex>` *name* tokens. That asymmetry explains why
tail-call thunks are the poster child for this bug in both of its occurrences:
remill renders a direct `call` as `call @sub_<hex>`, which IS a differing token
and IS slotted correctly, but renders a tail `jmp` as pc arithmetic plus a
missing-block — pure literal text.

The AzWriter boot trap: `core::fmt::impl$75::fmt<str$>` is 32 bytes and ends in
one `jmp rel32`. The bytes encode displacement `0xda251e`, targeting
`<str as Display>::fmt`. The cached template carried `0xdc3ace`, and

```
0x101c82 + 0xdc3ace = 0xec5450
```

is 272 bytes inside `hashbrown::RawTable::reserve_rehash`. Because dispatcher
cases are keyed by function *entry* addresses, no case can exist for a
mid-function address, so the branch surfaced as an unmatched dispatch rather
than as a call to the wrong function — which is why it read as a discovery bug
for many runs. It is not: the target was never discoverable because it is not a
real branch target of that function at all.

## Fix

A new slot kind, `@disp`, closes the gap that made the field unrepairable:

```
<off> <len> D @disp:<next_off_hex>:<fingerprinted identity>
```

`RelocSite` now records `next_off`, the offset one past the instruction
carrying the field — the point a pc-relative displacement is measured from.
`reloc_templateize` reconstructs each masked site's displacement as
`old_target - (lift_addr + next_off)`, finds that value as a token in the IR,
and slots it under the same fingerprinted identity the `s` slots use.
`reloc_translate` resolves that identity to this build's address and re-derives
`target - (new_lift_addr + next_off)`. Displacements are signed, so `D` slots
splice as `i64`; a backward branch renders `-N`.

Where the field cannot be slotted safely the whole function is refused — a miss
costs one remill invocation, a wrong splice costs silent wrong code. It is
refused when the value occurs more than once in the IR (no unambiguous splice
point), when two sites resolve to the same token, or when the target has no
verifiable identity. A displacement that does not appear in the IR at all is
not frozen and needs nothing, so the direct-call case keeps its hit rate.

`LIFT_CACHE_VERSION` is bumped 9 → 10. Every v9 template froze its
displacements and is unrepairable; the old entries must not be read.

## What is NOT the bug

An earlier revision of this note blamed callee-name swaps, where three distinct
callees translated to `sub_bf2270`. Benign: the bytes show all four are
ICF/monomorphization duplicates and byte-identical.

It also blamed the exact-symbol identity for carrying no fingerprint. That
cannot cause a wrong splice either — `reloc_translate` falls to `return None`
for identities it does not handle, so a bare `name+0x{off}` is a cache *miss*.
It costs hit rate, nothing more.

The `@rel` class is sound: sweeping every `@rel` slot in the cache (3,086,415
of them) found none whose offset reaches beyond its own function.

## Operational rule

`AZ_RELOC_VERIFY=1` is not optional for a debugging run — `scripts/m9_e2e/
azwriter_verify.sh` is the runner that sets it. It roughly doubles remill for
one run; a silent wrong-address lift costs far more. Both defects here were
found by it and neither was visible without it.

The invariant it enforces, worth stating directly because the code cannot
express it locally: **every byte masked out of the canonical key must
correspond to a slot that is re-resolved at translate time.** Masking is a
promise to re-resolve. `reloc_canonicalize` and `reloc_templateize` decide that
independently, and a field that falls between them is silently stale.
