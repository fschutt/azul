# A lifted body must be entered with its OWN address

Status: fixed. Two of four seeding sites were wrong; both are corrected and the
symptom is gone by experiment.

## The contract

remill lifts a function at an address and emits, in its entry block:

```llvm
define ptr @sub_de51f0(ptr %state, i64 %program_counter, ptr %memory) {
  ...
  store i64 %program_counter, ptr %NEXT_PC
```

Nothing later overwrites that seed with a constant. Every instruction then does
`load NEXT_PC → store PC → add <len> → store NEXT_PC`, so the **entire** PC chain
— and with it every rip-relative address the body computes, including jump-table
bases and `lea` of `.rdata` — is derived from the `pc` argument.

So `pc` must be the address the body was lifted at. Anything else silently
skews every rip-relative address by the difference.

## The four seeding sites

| site | passes | was |
|---|---|---|
| export wrapper | `i64 {lift_addr}` | correct |
| recursive-bl forwarder | `i64 {lift_addr}` on x86-64 | correct |
| `__az_indirect_dispatch` cases | the switch **label** | **wrong** |
| direct `call @sub_<hex>` | remill's raw target operand | **wrong** |

The forwarder's own comment already read "pc→entry on x86", so the rule was
known — it simply had not been applied to the other two.

### The dispatcher

`dispatcher_csynths` returns `(case_label, body_csynth)` and deliberately emits a
case for **both** the raw address and the `resolve_synth`'d canonical, because a
tail-call arrives with the raw synth while an fn-pointer arrives with the
canonical. Both cases call the same `@sub_<canonical>`. Measured on one build:
9,832 cases, of which **4,916 (exactly 50%) had `label != callee`**, every one
with the same skew `0xe8fc1000` — i.e. the masked-native form. Passing `%pcm`
seeded half of all dispatch entries with an address belonging to something else.

### Direct calls

remill emits a direct call as `call @sub_<target>(state, <target>, memory)`,
where the operand is a load of `State.rip` and therefore the raw target. That is
correct until `rewrite_sub_names_to_canonical` chases a PLT stub or drop shim and
retargets the **callee** to its canonical body — the name changes, the operand
does not. Verified by diffing one function's raw against its patched IR:

```
RAW                 PATCHED
@sub_3633e0 x5  →   @sub_de5230 x5
@sub_363350 x2  →   @sub_c32670 x2
@sub_101cb0 x6  →   @sub_e7b500 x6
```

`seed_direct_calls_with_callee_pc` rewrites the operand to the callee's address,
unconditionally and **after** the rename.

## Why the symptom was so hard to place

`app_state_from_json` called a `Vec` drop shim that resolves to `U8Vec::drop`.
Seeded with the shim's address, that body's destructor jump-table `lea` computed
a base in the `.text` of an *unlifted* webrender function. `.text` is not
mirrored, so the table read returned **zero**, leaving `base + table[tag]` equal
to the bad base — an address matching no switch case.

The reported PC therefore named neither the caller nor any real target, and the
value appeared in **no `.ll` file, no heap object and no mirror**, because it was
computed from a bad seed and never stored anywhere. Every search for it came back
empty, which is what made it look like memory corruption for several rounds.

## Evidence it is fixed

| | before | after |
|---|---|---|
| unmatched indirect dispatches | 1 (≈12 runs running) | **0** |
| state-pointer recorder | written | 0 — the `unk` arm never ran |
| boot depth | dispatch → U8Vec::drop → app_state_from_json | three frames deeper |
| raw / brotli | 28,323,570 / 2,944,107 | 28,341,706 / 2,948,275 |

Size is unaffected (+0.06% raw, +0.14% brotli), so the fix is free.

## Diagnosing the next one

The boot still fails, now as an `unreachable` with **zero** dispatch misses.
Recorder `0x40048` (NeverLift_reached) holds `0xf049c0` =
`std::thread::local::panic_access_error`: lifted code touched a `thread_local!`,
the slot read as uninitialised and `std` panicked into a NeverLift stub. The
dispatcher already emulates macOS TLV (`AZ_TLV_MAGIC_PC` → `tlv_get_addr`);
Windows TLS uses `__tls_index` and the GS-segment `_tls_array`.

**Read `0x40048` first on any `unreachable` with no unmatched dispatch** — it
names the NeverLift function outright.
