# Windows thread-locals are unimplemented in the lift

Status: root-caused, not yet fixed. This is the blocker that follows the
pc-seeding fixes (see `web-lifted-pc-seeding.md`).

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

## Proposed fix

wasm is single-threaded, so "thread-local" collapses to "one global block", and
the template can serve as that block directly — writes to it are exactly the
semantics one thread wants.

1. Reserve a small region above the recorders (they occupy `0x40000`–`0x40A08`,
   and `AZ_COV_BASE` is `0x41000`): a TEB stub plus a TLS array.
2. In the export wrapper, before calling the body, store
   * `TEB + 0x58` = address of the TLS array
   * `tls_array[i]` = synth of the TLS template, for a handful of low indices
     (`__tls_index` for a main EXE is normally 0)
   * `state_buf + 2152` (GSBASE) = the TEB address
3. Nothing else changes.

**Initialise at runtime in the wrapper, not with a data segment.** The mini
currently has ZERO data segments, which is what makes lazily-instantiated chunks
safe (see `web-lift-wasm-size.md`); adding one to carry a TEB would silently
reintroduce the hazard that the chunk plan depends on being absent.

## Verifying it

Boot and read recorder `0x40048`. If it stays 0, no NeverLift stub was reached.
The 23 GSBASE-referencing bodies are the population that changes behaviour.
