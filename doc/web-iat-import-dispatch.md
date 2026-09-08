# Untranslated IAT imports silently no-op in lifted wasm

Status: root cause, found on AzWriter's boot · Fix not yet implemented.

## Symptom

AzWriter hydration reaches marker 4 (the app's own `doc_state_from_json`
running inside the lifted wasm), then traps in `unwrap_failed`. Twenty
unmatched indirect dispatches happen inside that one call.

## What the recorded PCs actually are

The dispatcher masks `%pc` to 32 bits, so a recorded PC cannot by itself say
whether the guest read a synth, a native image address, or something else.
Scanning the module for 8-byte values whose low half matches settles it —
every one of them carries high32 `0x00007ffd`, the Windows system-DLL band
(the guest exe is at `0x7ff7f6…`):

| masked PC | full value | resolves to | ring hits |
|---|---|---|---|
| `0x975726d0` | `0x7ffd975726d0` | `KERNEL32!GetProcessHeap` | 8 |
| `0x97572da0` | `0x7ffd97572da0` | `KERNEL32!HeapFree` | 5 |
| `0x989ba7d0` | `0x7ffd989ba7d0` | `ntdll!RtlReAllocateHeap` | 2 |
| `0x872f1fd0` | `0x7ffd872f1fd0` | `VCRUNTIME140!memcmp` | 1 |

Each value occurs **exactly once** in the module — one IAT slot each.

## Root cause

The import address table is mirrored into guest memory with its **load-time
resolved** addresses. Those point into KERNEL32 / ntdll / VCRUNTIME140, which
are not tracked images (`native=[0x7ff7f6f71000..0x7ff7f81ad1d4]`, one image),
so `native_to_synth` returns `None` and the pointer is left raw. When lifted
code loads an IAT slot and calls through it, the dispatcher sees a `0x7ffd…`
value, matches no case, and **returns as if the call had succeeded**.

`rewrite_iat_calls` handles the *direct* `call [rip+iat]` form and was verified
clean. This is the other form: the slot loaded into a register and called
indirectly, which only the dispatcher sees.

Note this is a different layer from the allocator fix already in place.
`BumpAllocWinHeap` intercepts the *Rust wrapper*
`std::sys::alloc::windows::process_heap_alloc`, so allocation works. Free,
realloc, `GetProcessHeap` and `memcmp` reach the IAT directly and are dropped
— `memcmp` in particular makes serde's field-name matching answer wrongly,
which is how a deserializer that should return `Ok` ends up unwrapping an
`Err`.

## Why nothing caught it

* The **audit's natptr scan counts pointers inside the image range only**, so
  pointers to *other modules* are invisible to it. That is the blind spot to
  close first — it would have made this a build-time report.
* The dispatcher's default arm returns rather than traps, so the failure
  surfaces far from its cause (see the first/ring recorders at 0x409B0/0x409C0,
  added for exactly this reason — they are what made this diagnosable).

## Fix

Preferred, because the dispatcher is already the choke point every one of
these calls arrives at: **emit dispatcher cases for intercepted imports.** For
each import the runtime already has a helper for, resolve its runtime address
server-side (it is in our own IAT), mask to 32 bits, and route it:

| import | route to |
|---|---|
| `HeapFree`, `RtlFreeHeap` | the `BumpDealloc` helper body |
| `HeapReAlloc`, `RtlReAllocateHeap` | the `BumpRealloc` helper body |
| `HeapAlloc`, `RtlAllocateHeap` | the `BumpAllocWinHeap` helper body |
| `GetProcessHeap` | a stub returning a fixed non-zero handle |
| `memcmp`, `memcpy`, `memset`, `memmove` | the existing libc helper bodies |

Collision guard as with the truncated-native aliases: skip a label that is
itself a valid synth.

Then close the detection gap: any mirrored 8-byte value that lands inside a
*loaded module other than a tracked image* and is not one of the routed
imports should be an audit finding, not silence. Reaching one at runtime is a
wrong answer by construction.

## The interceptions only fire if the whole function is lifted

`ImportIntercept::{ProcessHeap, HeapAlloc, HeapFree, HeapReAlloc}` are what make
the Rust allocator shims work on the bump heap: the shim is lifted *for real*,
and its two indirect transfers land on the intercepted imports rather than on
Windows.

`__rust_dealloc` is a 44-byte body of exactly that shape:

```
+0x12  ff 15 98 ba 08 00  call [rip+0x8ba98]   -> GetProcessHeap
+0x25  48 ff 25 94 ba 08  jmp  [rip+0x8ba94]   -> HeapFree
```

So a truncated lift breaks the interception silently. When the symbol size was
wrong (16 for a 44-byte function, because `__rdl_dealloc` and `__rust_dealloc`
are ICF-folded and the gap-to-next-symbol heuristic landed mid-instruction), the
body stopped at `+0x12` — *before* either transfer — and emitted a missing block
there instead. It presented as an unmatched dispatch at `entry+0x12` that no
dispatcher case could ever satisfy, because the problem was not a missing case.

Two consequences worth keeping:

* the force-enqueued allocator shims read `max(e.size, LIFT_READ_WINDOW)`, not
  `e.size` — a larger window is safe because remill follows control flow from
  the entry and stops at the terminator;
* if an intercepted import ever appears not to fire, check that the calling
  function was lifted to its full extent before suspecting the interception
  table. `scripts/m9_e2e/dump_dealloc.py` decodes a function from a confirmed
  entry for exactly this.

## Coverage is the wrong question — reachability is the right one

248 is what the PE imports, not what lifted code calls. Every un-intercepted
import that lifted code *reaches* becomes an unmatched dispatch at a raw native
address; the rest are irrelevant. For several runs each missing entry cost one
35-minute lift to discover, because the only oracle was the trap itself.

`scripts/m9_e2e/import_gap.py` narrowed that by listing un-intercepted imports,
but only by name heuristic over all 248 — which says nothing about whether
lifted code ever calls them. Measured against the answer below, it flagged eight
names that are never reached and missed every one of the ten that are.

`scripts/m9_e2e/import_reach.py` answers it statically, with no relift:

1. the lift scratch holds one `<Name>_<va>.lifted.ll` per walked function, so
   the filenames **are** the walked set;
2. `.pdata` gives each function's exact `[begin, end)` — authoritative, and free
   of the ICF truncation that gap-to-next-symbol sizing suffers (that sizing is
   what truncated `__rust_dealloc` to 16 bytes);
3. inside those extents, scan for `call`/`jmp [rip+d]` **and** `mov r64, [rip+d]`
   — the address-taken form, which took the reached set from 12 to 18;
4. a target landing exactly on a live IAT slot names the import, and the scratch
   filename names the **call site**, which is what makes the result a decision
   rather than a list.

A walked address that is not itself a `.pdata` begin is mapped to the extent
containing it: those are ICF-folded aliases and thunks, one walked function in
six, and skipping them was a real hole.

The module base is recovered by vote over `(walked VA − .pdata begin)`
candidates. The winner's margin and the share of the walked set landing on a
begin are printed, so a scratch from a *different build* shows up as a low share
rather than as confident wrong output.

### The answer, against the mini's complete 4885-function walk

**20 of 248 imports are reached.** Ten of them had no case, and the call sites
decided the semantics:

| Import | Call site | Answer | Why it is exact, not a guess |
|---|---|---|---|
| `FormatMessageW` | `windows_result::HRESULT::message` | `0` | "no text for this code"; the caller then prints the numeric code |
| `LoadLibraryExA` | same | `NULL` | `windows-result` probes for a module to pull NTSTATUS text from and falls back |
| `SysStringLen` | `windows_result::Error::message` | `0` | defined return for `NULL`, and COM is never initialised so `NULL` is the only case |
| `SysFreeString` | same | no-op | documented no-op for `NULL` |
| `GetErrorInfo` | `windows_result::Error::from` | `S_FALSE` | documented "no error object" |
| `GetLastError` | `Once::call`, `Error::from_win32` | `0` | nothing here sets a last error |
| `CloseHandle` | thread/file `Drop` glue, 11 sites | `TRUE` | no handle was ever opened |
| `GetCurrentProcess` | `layout::probe` | `(HANDLE)-1` | the pseudo-handle is a constant on real Windows too |
| `K32GetProcessMemoryInfo` | same | `FALSE` | `probe.rs` reads `if .. == 0 { return None }` — a branch the caller has |
| `CoCreateInstance` | cpal WASAPI `OnceLock` | `REGDB_E_CLASSNOTREG` | there is no COM registry — and it **must** fail: `S_OK` with no object hands back a null interface pointer |
| `GetProcAddress` | std's dbghelp backtrace symbolizer | `NULL` | there is no dbghelp to load, and `backtrace-rs` is written around the probe failing |

Five of these sit in the error-message **formatter**, which is the worst place a
trap can be: the boot dies formatting the message instead of showing the error
it was reporting. Two sit in layout, which runs on hydrate.

### Deliberately left to trap

`kernel32!WideCharToMultiByte`, reached from the same symbolizer function as
`GetProcAddress`. The two decisions are not inconsistent: `GetProcAddress` asks
a question about the environment and the environment's answer is "no", whereas
`WideCharToMultiByte` **computes** something, and a stubbed `0` would be silent
corruption of a real conversion — worse than the trap it replaces. It also sits
past the symbolizer's success path, so with `GetProcAddress` returning `NULL` it
is not reached. A trap there is the signal that the path went live and the
conversion has to be implemented rather than answered.

Also still held, with no evidence of reachability: `QueryPerformanceCounter`,
`GetSystemTimeAsFileTime`. When justified, a time interception needs a **counter
slot** — a fixed value makes every measured duration zero — not a constant.

### A stub is quieter than a trap

Replacing a trap with a plausible value is exactly how a live wrong path goes
silent, so every `NoOsStub` records its dispatcher label at `0x40088` and bumps a
count at `0x40090`; `state-regs.js` prints both, and the lift log's
`M12.7: IAT import <DLL>!<fn> → 0x<label>` lines name the label back. A non-zero
count is not a failure — but five of the eleven only run when something upstream
already failed, so it is worth reading.

A name in the gap list is a **candidate, not a bug**. It only matters once
lifted code reaches it, and each still needs its own judgement about what the
right answer is — a zero-stub is correct for a wake and wrong for `memcpy`.
