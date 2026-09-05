# Windows thread-locals are unimplemented in the lift

Status: root-caused and implemented; boot effect not yet measured. This is the
blocker that follows the pc-seeding fixes (see `web-lifted-pc-seeding.md`).

## Symptom

The boot fails with `RuntimeError: unreachable` and **zero** unmatched
dispatches. Recorder `0x40048` (NeverLift_reached) holds the synth address of
`std::thread::local::panic_access_error`: lifted code read a `thread_local!`,
got garbage, and `std` panicked into a stub that is deliberately not lifted.

## Mechanism

Windows `thread_local!` compiles to a TEB read. From a real lifted body
(`azul_core::prop_cache::CssPropertyCache::get_property`):

```llvm
%GSBASE = getelementptr inbounds %struct.State, ptr %state, i32 0,i32 0,i32 5,i32 5,...
%130 = load i64, ptr %GSBASE
%131 = add i64 88, %130          ; 88 = 0x58 → TEB.ThreadLocalStoragePointer
%call.i.i40 = call i64 @__remill_read_memory_64(memory, %131)
store i64 %call.i.i40, ptr %R10
```

`%GSBASE` is a State field that azul never initialises, so it is **0** and the
read lands at linear address `0x58` — inside the wasm stack zone. The TLS array
pointer is garbage, so the block pointer is garbage, so the variable is garbage.

23 lifted functions reference `%GSBASE` in one build. The only thread-local
support that exists today is the **macOS** TLV path (`AZ_TLV_MAGIC_PC` →
`tlv_get_addr`, `SymbolTable::tlv_tls_base_synth`). There is no Windows
equivalent — the only other `tls` in `dll/src/web/` is HTTPS certificate config.

## The State offset, derived and cross-checked

`AddressSpace` interleaves a padding qword before every base, exactly like
`GPR`:

```
_0, ss_base, _1, es_base, _2, gs_base, _3, fs_base, _4, ds_base, _5, cs_base
```

so LLVM field **5 is `gs_base`**, matching the GEP. `X86State` lays out as
ArchState 16, vec 2048, aflag 16, rflag 8, seg 24, addr 96, gpr 272 …, putting
`addr` at 2112 and `gpr` at 2208.

**GSBASE = 2112 + 40 = 2152.**

Two independent checks: `gpr` at 2208 matches the verified `pcs::RET`/`pcs::ARG`
anchors, and the total 96+3264+16+128 = 3504 matches remill's own
`static_assert` on `sizeof(X86State)`.

## What the image provides

Data directory 9 of `AzWriter.exe`:

| field | value |
|---|---|
| StartAddressOfRawData | rva `0x1229de0` |
| EndAddressOfRawData | rva `0x122a279` |
| raw template size | **1177 bytes**, in `.rdata` |
| AddressOfIndex | rva `0x123e0e8` |
| SizeOfZeroFill | 0 |

The template is part of the image, so its synth address is computable with the
existing rebase and it is mirrorable by the existing machinery.

## The fix

wasm is single-threaded, so "thread-local" collapses to "one global block", and
the template can serve as that block directly — writes to it are exactly the
semantics one thread wants.

`emit_win_tls_init` (in `transpiler_remill.rs`) emits three kinds of store into
the export wrapper, just after the SP seed:

| address | value | why |
|---|---|---|
| `AZ_TEB_ADDR + 0x58` = `0x42058` | `AZ_TLS_ARRAY_ADDR` | what `gs:[0x58]` fetches |
| `AZ_TLS_ARRAY_ADDR + 8·i`, i<8 | template synth | indexed by `__tls_index` |
| `state_buf + pcs::GSBASE` | `AZ_TEB_ADDR` | makes `gs:` resolve at all |

`0x42000` sits above the recorders (`0x40000`–`0x40A08`) and above the coverage
bitmap (`AZ_COV_BASE` `0x41000` + one byte per lifted fn), and below the image
band at `synth_base`.

`__tls_index` lives in the image at rva `0x123e0e8` and is written by the real
loader, so the mirrored copy reads 0 — index 0 is the live one. The other seven
entries cost 7 stores and remove a whole class of wild-pointer failure if some
statically-linked component picks a different index.

**Initialise at runtime in the wrapper, not with a data segment.** The mini
currently has ZERO data segments, which is what makes lazily-instantiated chunks
safe (see `web-lift-wasm-size.md`); adding one to carry a TEB would silently
reintroduce the hazard that the chunk plan depends on being absent.

The wrapper is the only site that allocates a State buffer — every deeper frame
receives `%state` from its caller — so seeding there covers the whole call tree.

### Why the template and not a zeroed scratch block

A zeroed block would be *nearly* right: `LocalKey`'s state byte 0 means
Uninitialized, so lazy initialization would still run. But a `thread_local!`
with a non-zero const initializer takes its value from the template, and a
zeroed block would silently hand it 0. Pointing at the template is correct
whether or not that `.rdata` window turns out to be mirrored; if it is not, the
behaviour degrades to exactly the zeroed-block case rather than breaking.

## Is there a second blocker of the same kind?

No — GSBASE was the only one. Scanning every lifted body for `%struct.State`
field pointers that are `load`ed with no `store` anywhere in the same body:

| field | bodies reading | never set | verdict |
|---|---|---|---|
| `GSBASE` | 92 | **92** | the bug fixed here |
| `BP` / `SI` / `DI` | 823 / 578 / 410 | 465 / 360 / 261 | false positive: 16-bit aliases of RBP/RSI/RDI, which *are* stored under their full names |
| `ST4` / `ST5` | 4 | 4 | x87 stack reads in 4 bodies; real but not a base address |
| `MM0` / `ST0` | 8 / 4 | 0 | stored before use |

So the wrapper's seed list — SP, the ABI arg slots, GSBASE — is complete for
every field used as a base address. The x87 reads are worth a look if long
double math ever misbehaves, but they cannot produce a wild pointer.

## Is `0x42000` safe from the wasm stacks? Yes — measured

Non-mini wasms get tiled stack slots: `STACK_BASE_FIRST` 192 KiB, stride 128
KiB, so slot *n*'s stack pointer starts at `0x30000 + n·0x20000` and grows
**down**. A boot hands out 24 slots, and slot 1 (`SP 0x50000`) descending a full
export frame would cover the recorders, the coverage bitmap and the TEB — so
this needed checking rather than assuming.

It does not happen, because slots are handed out per *link*, not per shipped
module, and only **two** wasms exist in a run's scratch:

| module | slot | SP | frame (one export, `0x20DC0`) |
|---|---|---|---|
| `azul-mini.wasm` | 0 | `0x30000` | `[0xF240 .. 0x30000]` |
| `transitive-lift.wasm` | last (23) | `0x310000` | `[0x2EF240 .. 0x310000]` |

Mini descends *away* from `0x40000`, and transitive-lift sits far above it.
Nothing reaches `0x42000`. `tls-probe.js` still reads `TEB + 0x58` back and
reports a mismatch explicitly, so if this ever changes it surfaces as
`*** expected 0x42200` rather than as a phantom TLS failure. The fallback if it
does: 1528 free bytes between the recorder block (`0x40A08`) and the coverage
bitmap (`0x41000`), which is far more than the 160 this needs.

Worth noting separately, since it fell out of the same measurement: one export
frame is `0x20DC0` (134,592 B) but the slot stride is `0x20000` (131,072 B), so
every slot's frame overruns its own stride by exactly `state_size`. And
transitive-lift's frame `[0x2EF240..0x310000]` lands *inside* the image band
(which starts at `0x101900`), corresponding to `.text` rva `0x1EE940..0x20F700`.
Whether those pages are in the mirror set is not established here, so the
practical impact is unknown — but the overlap is real and pre-existing.

## The tagging trap this fix fell into

`tag_state_accesses` stamps every *untagged* load/store in the helper IR with
the HOST scope and `!noalias` guest, because everything there normally targets
`%state_buf` / `%stack_buf`. A store reaching guest memory through `inttoptr`
gets the same stamp, which is a false `noalias` claim against the lifted body's
guest access to the same address. The first cut of this fix emitted:

```llvm
store i64 270848, ptr %teb_slot, align 8, !alias.scope !90005, !noalias !90004
```

— correct IR, wrong metadata, and the seed would have measured as "no effect".
The tagger skips pre-tagged lines, so the fix is to emit the guest tag
(`!alias.scope !90004, !noalias !90005`) directly, plus `volatile` to match
every other guest write here.

Auditing the whole helper corpus for this shape (`inttoptr`-derived pointer,
host tag): 16,141 sites, of which 16,137 were this seed × 1793 wrappers. Two
others remain, both pre-existing:

* **`sub_e82f70`** (`std::env::var_os → None`) writes a 24-byte `None` to the
  guest sret buffer with three host-tagged stores. A lifted caller reads that
  buffer guest-tagged. **Genuine bug of the same class**, not on the path this
  fix addresses; left alone deliberately rather than folded into a TLS change.
* **`sub_3ad850`** loads a stack arg at `SP+40`. That address is inside
  `%stack_buf`, which the wrapper prologue writes as a host alloca, so the host
  tag is consistent with its writer — not a bug.

## Synth addresses shift between builds — always recalibrate

Two consecutive runs, differing only by an edit to `transpiler_remill.rs`:

| run | template rva | logged synth | delta |
|---|---|---|---|
| 62 | `0x1229de0` | `0x132a6e0` | `0x100900` |
| 63 | `0x122b760` | `0x132a760` | `0xff000` |

Both the rva **and** the synth delta moved. The rva moves because **AzWriter
lifts itself**: the lift host and the lifted image are the same binary, so any
edit under `dll/src/web/` relinks the image whose addresses are being computed.
The delta moves too, so `synth = synth_base + rva - 0x1000` is not reliable even
though run 63 happens to match it.

Practically: a synth address noted in one run is invalid in the next as soon as
any source changed. Take the delta from that run's own `win-tls` log line and
apply it to the rva read from that run's own exe. `tls-probe.js` takes the
`__tls_index` synth as `argv[4]` for exactly this reason; its default is only a
starting point.

## Verifying it

The wrapper emitter logs its decision once per lift:

```
[azul-web] win-tls: template synth=… len=… → TEB 0x42000, slots 0x42200, GSBASE off 2152
```

— an empty emission is otherwise indistinguishable from a working one in a boot
log. Then boot and read recorder `0x40048`: if it stays 0, no NeverLift stub was
reached. The 23 GSBASE-referencing bodies are the population that changes
behaviour.
