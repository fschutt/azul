# Why the lifted wasm is large, and what actually shrinks it

Measured on AzWriter (run 39), with `scripts/m9_e2e/wasm-size-report.py` and
`scripts/m9_e2e/wasm-dep-report.py`.

## The headline number

rustc compiles printpdf + its HTML solver to **2.8 MB** of wasm. Lifting the
same engine from x86 produces **~100 MB**. That ~35× is not one bad component —
it is the per-function expansion of lifting, and it is uniform across the
library (azul_layout 24%, azul_css 20%, azul_core 15%).

One function makes it concrete — `LayoutWindow::layout_dom_recursive_impl`:

| stage | size | factor |
|---|---|---|
| native x86 `.text` | 38 KB | — |
| lifted LLVM IR | 8.0 MB | **206×** |
| wasm object | 681 KB | **18×** |

## What it is NOT

Two intuitions we both had, and the measurements that killed them:

- **Panic machinery — already collected.** The classifier routes the whole
  panic family to `FnClass::NeverLift` (they trap instead of being lifted), so
  of the 74.2 MB actually lifted, panic code is **7 functions and 0.01 MB —
  0.0%**. There is no win left here; it was taken.
- **Formatting machinery.** ~3.4 MB, **4.6%** — real, but live: markdown
  rendering, CSS value formatting and PDF text all format at runtime. In MSVC
  demangling these appear as `impl$N::fmt`, which does not say whether the
  trait is `Debug` (droppable) or `Display` (needed), so there is no cheap
  name-based cut.

  ⚠ An earlier revision of this document claimed "panic + fmt, 738 functions,
  2.6 MB, ~6%" and listed panic stubbing as an open win. Both halves were
  wrong. The bucket conflated already-trapped panic code with live formatting,
  and its regex matched `Display` inside *type* names — `DisplayList`,
  `DisplayListItem` — so 0.17 MB of core layout code was counted as
  stubbable formatting. `wasm-size-report.py` has been fixed.
- **`volatile` blocking optimization.** De-volatilizing every store in the
  8 MB whale and re-running `opt -O2` + `llc` moved the object from
  549,237 → 546,010 bytes. **0.6%.** The stores are not volatile-blocked.

## What it actually is

After `-O2`, 38,895 stores survive in that one function. They are not heap or
stack traffic — they are **CPU-state writes**:

| location | stores | loads | dead? |
|---|---|---|---|
| `%PC` (program counter) | 11,702 | 20 | ~all |
| `%af` (aux carry flag) | 2,938 | **0** | all |
| `%pf` (parity flag) | 2,147 | **0** | all |
| `%cf` (carry flag) | 2,086 | **0** | all |
| `%sf` (sign flag) | 2,127 | 10 | ~all |
| `%of` (overflow flag) | 2,119 | 3 | ~all |
| `%zf` (zero flag) | 2,148 | 584 | most |

**~18,900 of 38,895 stores (49%) write values nothing ever reads.**

remill models each x86 instruction's full CPU effect: it restores the program
counter before every basic block and recomputes all six arithmetic flags after
every `add`/`sub`/`cmp`, whether or not anything consumes them. In wasm there
is no program counter at all, and a flag is only meaningful if a later branch
reads it.

### Why LLVM cannot remove them by itself

The `State` pointer **escapes** — it is passed to every lifted callee
(`sub_x(ptr %state, i64 %pc, ptr %memory)`). So `opt` must assume some callee
loads `%af`, and dead-store elimination is blocked. This is an
alias-analysis limit, not a missing pass; SROA/mem2reg cannot promote an
escaping alloca.

## The lever: metadata LLVM cannot derive, which we already have

Every byte we lift is **rustc output**. That licenses two facts the optimizer
has no way to prove:

1. **The program counter has no meaning in wasm.** A callee receives its PC as
   an explicit argument (`sub_x(%state, i64 %112, %memory)`) and overwrites
   `%PC` at its own entry. It never reads the caller's stored PC.
2. **x86 arithmetic flags never cross a call boundary in Rust.** The SysV /
   Windows-x64 ABIs treat EFLAGS as undefined across calls, and rustc never
   emits a function that begins by consuming the caller's flags. So a flag
   store is dead unless a branch *in the same function* reads it.

These hold for the whole engine (`azul-mini.wasm`) because it is entirely our
own Rust. They are weaker for arbitrary application code, so the pass is
applied where the guarantee holds.

## Implemented

`strip_dead_state_stores` — a post-`opt` IR pass (same family as the existing
self-loop and SP-restore rewriters):

- **Field-level DCE**: a state register with *zero* loads in the module has all
  its stores deleted (`af`, `pf`, `cf` — ~7,200 stores in the whale).
- **Located DSE for `%PC`**: a PC store is deleted when no PC load and no call
  occurs before the next PC store, so the 20 real `icmp` readers keep their
  values.

Expected: roughly half the surviving stores in a typical lifted function.

## Not worth doing (measured)

- Removing `volatile` (0.6%).
- Text-concatenating modules to drop the `llvm-link` spawn — unsafe, and
  unrelated to size (see `doc/web-lift-env.md` and the pipeline notes).
- Cross-function merged compilation: miscompiles above ~30 functions.

## Still open

- **Full-surface base image.** 36,500 functions at 18× is ~600 MB of wasm, so
  size — not lift time — gates a shippable `AZ_LIFT_MODE=full` image. The
  state-store passes are the prerequisite.
- Formatting is 4.6% and mostly live; a cut needs Debug-vs-Display
  discrimination that the demangled names do not carry.
- `zf` needs real liveness (584 genuine readers), unlike the zero-load flags.

---

# Code splitting: making the lift unit smaller than the function

The passes above make each lifted function cheaper. This is the other axis:
**lift and ship fewer of them.**

## Why granularity is fixed at Rust compile time, not lift time

The lifter's unit of work is a **symbol** — an `(address, size)` pair from the
PE symbol table. The discovery walk starts at roots and follows call edges, and
whatever it reaches, it lifts *whole*. Everything rustc inlined into a function
is inside that unit and cannot be excluded: there is no such thing as lifting
half a function.

So a 907 KB wasm object is all-or-nothing. If it contains one cold error path
that no web user ever hits, that path still costs its full lifted size, because
after inlining it is not addressable as anything separate.

Measured on the AzWriter run — 5,235 functions, 94.8 MB of objects:

| slice | wasm | share |
|---|---|---|
| top 10 functions | 6.6 MB | 7% |
| top 100 | 25.1 MB | 26% |
| top 500 (9.5% of functions) | 48.7 MB | **51%** |
| top 1000 | 63.6 MB | 67% |
| the 122 functions over 100 KB | 27.4 MB | 29% |
| the 1,888 functions under 5 KB | 5.1 MB | 5% |

Half the payload lives in 500 functions. Those are exactly the
heavily-inlined, heavily-monomorphized ones — and the ones most likely to
contain cold paths welded to hot ones.

## The boundary

`#[inline(never)] #[no_mangle] pub extern "C"` on an internal helper produces a
**distinct symbol with its own address and size**. That single change buys
three separate things:

1. **Strip.** A boundary the discovery walk never reaches is never lifted at
   all. Cold paths behind a call become invisible unless something calls them.
2. **Split.** `FnClass::BoundaryImport` already turns a function into a wasm
   *import* instead of lifted code. Point that import at a JS loader that
   fetches a second module on first call, and it is genuine lazy code
   splitting — `azul-mini.wasm` ships the hot core, the rest arrives on demand.
3. **Shrink the caller.** Pulling a cold path out stops rustc inlining it in,
   so the whale itself gets smaller. This is the effect that compounds: the
   38 KB native whale is 38 KB *because* everything got inlined into it.

A fourth benefit falls out: **cache granularity**. The reloc-canonical cache
keys on a function's bytes. A 907 KB whale re-lifts whenever anything inside it
changes; ten 90 KB functions re-lift only the one that actually changed.
Smaller units directly improve the "only re-lift what we touched" behaviour.

## Where to cut

Two different licences apply, and they must not be confused:

- **Cold-path boundaries** (strip / split) are a size decision. Wrong guess =
  a lazy fetch on a path we thought was cold. Cheap to be wrong.
- **Browser-substitution boundaries** (replace the body with a JS round-trip —
  unicode tables, shaping, image decode) are a *semantic* decision. The browser
  must compute the same answer, or layout silently diverges. Expensive to be
  wrong; each one needs its output proven equivalent, not assumed.

Candidates, cheapest first: error/panic formatting paths, `Debug`/`Display`
impls on engine types, PDF and DOCX export (not needed to render a document),
font-fallback chains, rarely-used CSS property parsing.

## Caveats worth stating before cutting

- `extern "C"` changes the ABI: it loses Rust's niche optimisations and can
  force aggregates through memory. On a hot path that is a real regression.
  `#[inline(never)]` alone keeps the Rust ABI but the symbol can still be
  merged by ICF — only `#[no_mangle]` guarantees a distinct addressable symbol.
- A boundary inside a hot loop costs a real call per iteration.
- So: cut on paths measured cold, and re-measure both size *and* frame time.

## Sequencing

1. State-store DSE (done — 28% on the measured function).
2. ~~Panic stubbing~~ — already done via `NeverLift`; measured 0.0% left.
3. Cold-path boundaries in the top-500 list: strip first, since it needs no
   runtime machinery, only a boundary the walk does not reach.
4. Lazy split via `BoundaryImport` + a JS module loader.
5. Browser substitution, each with an equivalence test.

---

# Measured: the state-store pass, end to end

Run 39 (no pass) vs run 40 (pass), joined **by function name** — a rebuild
shifts every address, so the `__az_dep_<hex>` object names do not match across
runs and joining on them silently compares nothing.

Over the 859 functions both runs completed:

| | wasm |
|---|---|
| without the pass | 11.485 MB |
| with the pass | 8.797 MB |
| **reduction** | **2.688 MB — 23.4%** |

813 shrank, 39 unchanged, 7 grew (all under 1 KB — removing stores shifts
register allocation slightly). Consistent with the 28% measured offline on the
single largest function.

> Objects still being written when a run aborts read as 0 bytes and look like a
> 100% reduction. Filter empty objects before believing any total.

## Why not more — the `noalias` question

Reasonable theory: `opt` is blind because the pipeline strips every alias
annotation (`strip_alias_scope_metadata`, `strip_noalias_from_sub_args`). The
strip is real and deliberate — remill marks State registers and guest memory
mutually non-aliasing, which is true on hardware (separate address spaces) and
**false in wasm**, where both live in one linear memory and a guest pointer
truncated to 32 bits can land on the State struct. EarlyCSE trusted it and
forwarded a register load across a volatile guest store, producing garbage
`Vec`/`String` lengths.

But restoring it is worth **nothing**. Same module, one copy with its 10,180
`!alias.scope`/`!noalias` annotations intact and one stripped, both through
`opt -O2` + `llc`:

| | wasm `.o` | surviving stores |
|---|---|---|
| with alias metadata | 89,007 B | 2,865 |
| stripped | 89,007 B | 2,865 |

Byte-identical. Two reasons:

1. **Rust's `noalias` was never available to us.** It exists in rustc's IR
   before codegen. We lift *machine code* — by then there are no `&mut`
   references, and no backend records which pointers were unique. Every alias
   annotation in the lifted IR is remill's own synthesis, not Rust's.
2. **The State problem is escape, not aliasing.** `noalias` says "this pointer
   does not alias others". It does not say "the callee does not read this
   field". The State pointer is passed to every lifted callee, so DSE must
   assume some callee loads `%af` no matter what the alias metadata claims.

That is exactly why the ABI argument works where metadata cannot: "rustc never
emits a function that reads its caller's flags" is a fact about the *callee's
behaviour*, which no aliasing annotation can express.

Re-running `opt` after the pass — on the theory that the arithmetic feeding the
deleted stores is left behind as garbage — recovers a further 0.4% (391,808 vs
394,059 B). `llc` already collects it during instruction selection. Not worth a
second subprocess per function.

---

# How an entire desktop windowing stack got into a wasm build

Worth recording as its own failure mode, because no amount of per-function
optimization would have found it.

`azul::desktop::shell2` was ~20 MB of the reachable closure, and the single
biggest lifted function in the build was
`PlatformWindow::process_window_events_inner<Win32Window>` at 906 KB. None of
it is reachable: all 13 lifted `TypedEventHandlerBox` functions and the Win32
window procedure have **zero call sites** in the run's own dependency log.

They arrive through the fn-ptr discovery seed. `riprel_accesses` mirrors
`LEA_MIRROR_WINDOW` = 1024 bytes at every `lea rip+X`, so switch jump tables
are complete for devirtualization — correct, and it must stay. The same regions
are then scanned for 8-aligned qwords resolving to a function entry, and each
is enqueued as reachable. But a `lea` that materializes a *small* constant
still mirrors 1024 bytes, so the window runs into whatever sits next in
`.rdata`. In a desktop Windows binary, that is a COM vtable:
`alloc::raw_vec::do_reserve_and_handle` — a `Vec` growth helper — enqueued 18
fn-ptr targets, one of them a WinRT delegate vtable, and the Win32 event loop
followed from there.

**Mirroring a generous range is harmless. Treating every function pointer
inside it as reachable code is not.** One over-approximate byte range became a
whole subsystem.

Two checks now guard it:

- **F6 (fatal)** — native-platform code was lifted. `is_platform_native` routes
  OS windowing, WinRT/COM and Cocoa to `NeverLift`, so they trap loudly instead
  of being lifted. F6 firing means the classifier has a gap.
- **W5 (warning)** — a function reached *only* through a mirrored data window:
  no call, no address-take anywhere. Not automatically wrong, since a genuine
  indirect-only vtable slot looks identical — it is the signal to check.

The ordering matters and is pinned by a test:
`std::sys::alloc::windows::process_heap_alloc` contains `windows::` and is the
surviving allocator after LTO. If the platform rule ran before the allocator
rules, every allocation would become a trap and nothing would boot.

The narrower fix — seeding only from a *run* of consecutive function pointers,
which is what a real vtable looks like — is still open. The classifier bounds
the damage; it does not fix the seed.
