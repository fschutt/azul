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

---

# Measured: delivered bytes, not raw bytes

The shipped wasm is already `--strip-all`'d — zero name/debug sections, 98% code
— so stripping has nothing left to give. Compression has plenty.

Controlled A/B, each run's own objects through the same `wasm-ld` with the same
flags:

| | run 39 (no DSE, no cut) | run 41 (both) | change |
|---|---|---|---|
| raw wasm | 91.57 MB | 64.68 MB | **-29.4%** |
| **brotli -q11 (delivered)** | **9.08 MB** | **6.70 MB** | **-26.2%** |
| gzip -9 | — | 12.35 MB | — |
| brotli -q5 (on-the-fly) | — | 8.79 MB | — |

Two methodology traps, both of which I fell into first:

- **Do not compare against an older prebuilt wasm.** Against a build from a
  different day the same work measured -0.3% delivered, which would have
  supported a wrong conclusion ("compression already eats the wins"). Linking
  both runs' own objects with identical flags shows -26.2%. The wins survive
  compression roughly proportionally.
- **An offline link omits the data segments.** azul injects mirrored data
  separately (`inject_user_binary_data_segments`), so an offline `wasm-ld`
  produces a code-only module. The A/B is valid because both sides omit it
  equally, but the absolute figure understates the real artifact by ~1.4 MB raw.

Useful consequence: **a size number does not require a completed gate run.**
`wasm-ld --no-entry --allow-undefined --export-dynamic --strip-all
--gc-sections --initial-memory=536870912 @objs.txt -o out.wasm` links a run's
scratch objects in seconds. The response file needs Windows paths, one quoted
path per line.

## Why 36,500 functions — it is not the framework

Engine logic is **7.8% of the function count**. The lift target is the full
desktop DLL, so it drags in an embedded SQL database (turso, 1668 fns), a GPU
renderer (webrender, 872), TLS (rustls, 855), a regex engine (695), Vulkan
(498), PDF (406), accessibility (315) — and the lifter's **own** dependencies:
`iced_x86`, its x86 disassembler, and `goblin`, its PE parser, were being
compiled into the payload they exist to produce.

Measured, not projected: deleting the desktop/webrender family from run 39's
dependency graph orphans **1,393 functions = 24.09 MB = 25.4%** of that run.

A second root: `api_surface_roots` seeds a BFS root for *every* symbol starting
with `Az`, with no allowlist. 3,721 of those are auto-derived trait shims, and
`Az<T>_toDbgString` alone is 1,406 functions — each a `format!("{:#?}")` that
roots the entire formatting tree.

Three hypotheses died under measurement:

- **ICF is worth 0.00 MB.** MSVC already ran `/OPT:ICF` (3,280 duplicate names
  folded into 1,357 addresses) and the walk is address-keyed, so it inherits
  that. Even after normalising remill's baked-in PC constant, folding recovers
  0.03%. Do not build it.
- **azul's own generics are not the problem** — only 499 of 12,501 azul
  functions carry generic arguments. It is the `#[derive]`s and the sheer type
  count (1,440 distinct `Az*` types).
- **Drop glue is 6.8% of count, not the largest category**, contrary to the
  usual Rust rule of thumb, and it is a leaf with no fan-out.

## Chunking, not sharding

Per-function shards would be actively worse. The measured size model is
`obj ~= 2117 + 20.6 x native`, i.e. **~2.1 KB of fixed overhead per function** —
about 76 MB of pure overhead at 36,000 functions, before any real code. Shards
also destroy brotli's cross-function dictionary (the ~10x ratio comes from
repetition *across* functions) and cost one fetch plus one `instantiate` each.

The right shape is **2 to 5 chunks**: one eager core holding the first-paint hot
set, and a few lazily fetched chunks grouped by feature. The mechanism exists
and is dormant behind `AZ_ENABLE_SHARDS` — `BoundaryImport` emits an import,
`--allow-undefined` makes it an env import, and the loader already has an
`azBoundarySymbols` map.

Worth noting the argument that survives even if transfer size does not move
much: the browser still has to **compile** what it receives. 34 MB of wasm is a
multi-second compile even when it arrives as 4 MB on the wire, so splitting
helps time-to-first-paint independently of bytes.

## Excluded by policy

`is_browser_excluded_crate` (distinct from `is_platform_native`, which is code
that *cannot* run in wasm) routes to `NeverLift` the crates whose capability the
browser already provides: turso to browser storage, regex to `RegExp`, accesskit
to the DOM, TLS to `fetch`, plus GPU/gamepad/native-dialog loaders, transport
compression, and the lifter's own `iced_x86`/`goblin`. Matching is on the whole
leading crate name; a test pins that `StyleFilter::ash_blur` and `derive_style`
survive `ash` and `der`.

Measured on the app-mode lift with **crate-anchored** matching, the excluded
payload actually present is **47 functions / 0.174 MB / 0.43%** — essentially
nothing. turso, rustls, regex and accesskit are not reached at all in app mode.
The list therefore matters for the **full-surface prelift**, not for an app
build, where the desktop-shell cut has already done the work.

> An earlier revision of this section claimed 370 functions / 4.03 MB / 6.0%,
> "mostly rustls at 2.85 MB". That was a substring bug in the audit script, not
> a measurement: the pattern `ring::` matches `alloc::string::` — the tail of
> `"string::"` — so 86 functions of ordinary Rust string handling were booked as
> a TLS stack. There is no rustls in the lift at all. This is the second time an
> unanchored substring produced a phantom saving (the first matched `Display`
> inside `DisplayList`), which is why `wasm-payload-audit.py` extracts the
> leading crate name the way the classifier does.

`webrender` is deliberately **not** on the policy list, and the dependency graph
says it does not need to be. Of its 51 lifted functions, 41 have no caller at
all, and every external caller but one is `azul::desktop::*` —
`shell2::common::layout::generate_frame`, `wr_translate2::generate_frame`,
`Win32Window::regenerate_layout_inner`, `extra::media_keys::*`. Once the desktop
shell is `NeverLift`, webrender falls out on its own; run 41 confirms it drops
from 266 functions to 40. The single engine-side edge is
`azul_core::compact::apply_css_property_to_compact` reaching a
`webrender_build::shader` `From` impl, worth 5.8 KB.

That is the pattern to prefer generally: cutting a **reachability root** removes
a subsystem for free, where a policy exclusion has to be argued and maintained
per crate.

## Tooling defect worth remembering

`wasm-size-report.py` and `wasm-dep-report.py` anchored the function-name
capture on `(\S+)`. MSVC renders nested generics **with a space** —
`Vec<Box<T> >` — so 1,197 of run 39's 5,251 functions (23%, 20.6 MB) never
matched, biased toward exactly the non-generic names least affected by
monomorphization. Fixed to a non-greedy capture anchored on ` addr='. The DSE
result re-checked with the fix: **-22.1% over 2,718 functions**, against -23.4%
over 859 before.

Two related traps: join runs by **function name**, never by `__az_dep_<hex>`
(a rebuild shifts every address), and filter zero-byte objects — orphaned `llc`
children keep writing after an abort, and a truncated object reads as a 100%
reduction.

## Open: the link deadlock

Run 41 wedged after the walk — zero CPU, zero I/O, zero page faults over 30s,
two threads, no children. Ruled out: the linker (`wasm-ld` links the same 4,967
objects offline in seconds with the full production flag set), the spawn
watchdog's coverage (registration is correctly before `spawn()`), and re-entrant
`FFI_LOCK` (no holder calls another; the in-process link path is behind a
feature the gate does not build). `set_lift_phase` markers now bracket the
region so the next occurrence names its own phase.
