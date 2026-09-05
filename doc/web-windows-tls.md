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

## Verifying it

The wrapper emitter logs its decision once per lift:

```
[azul-web] win-tls: template synth=… len=… → TEB 0x42000, slots 0x42200, GSBASE off 2152
```

— an empty emission is otherwise indistinguishable from a working one in a boot
log. Then boot and read recorder `0x40048`: if it stays 0, no NeverLift stub was
reached. The 23 GSBASE-referencing bodies are the population that changes
behaviour.
