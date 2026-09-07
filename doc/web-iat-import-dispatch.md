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
