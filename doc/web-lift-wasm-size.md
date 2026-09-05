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

---

# Correction: the real artifact is half what the offline link suggested

The delivered figures above were measured on an **offline link with
`--export-dynamic`**, which exports every symbol and therefore **defeats
`--gc-sections`**. The production link exports 44 non-`__az_dep_` symbols plus a
selected subset of deps, so the collector can do its job.

Measured on the real artifact — run 41's own `azul-mini.wasm`:

| | raw | brotli -q11 | ratio |
|---|---|---|---|
| **shipped mini (run 41)** | **34.13 MB** | **3.56 MB** | 9.6x |
| my offline `--export-dynamic` link, same objects | 64.68 MB | 6.70 MB | 9.7x |

So AzWriter's engine wasm delivers as **3.56 MB**, not 6.70 MB. The relative
run-39-vs-run-41 comparison stands — both sides used identical flags — but every
absolute number from an offline link is roughly 2x too large. **Link with the
production export list, or measure the artifact the pipeline actually wrote.**

Also corrected: **run 41's link did NOT hang.** `azul-mini.wasm` was written at
the same second the log went silent, so wasm-ld completed and the deadlock is in
**post-link processing** — after `FONT-MIRROR`, which collects the mirror pages
*for* `inject_user_binary_data_segments`. That function has loops but no
blocking primitive, and the process showed zero CPU, so a loop cannot explain
it. The phase markers now bracket exactly this window.

# What actually owns the mini (measured per root, exclusive)

Two premises that guided earlier ticks are wrong for this artifact, and the
call graph in the run log settles both. Every `dep:` line carries
`(pulled in by <caller>)`, so the whole walk graph is recoverable: 12,206 edges
over 4,162 nodes for the mini. For a root category C the number that matters is
`exclusive(C) = reachable(C) - reachable(all other roots)` — the bytes deleting
C would actually remove. A root whose subtree is shared with the boot path is
free to keep no matter how ugly its name is.

Measured on the mini walk (4,525 functions, 52.10 MB of objects):

| root, exclusive subtree | MB | fns | % of mini |
|---|---|---|---|
| `AzStartup_solveLayoutReal` | 14.18 | 969 | 27.2% |
| **`azul_layout::window::virtual_view_measure_dom_trampoline`** | **10.57** | **508** | **20.3%** |
| `azwriter::web_state::app_state_from_json` | 4.37 | 227 | 8.4% |
| `azul_core::icon::resolve_icons_in_dom_inner` | 0.29 | 16 | 0.6% |
| all non-`Az` roots together | 21.37 | 2,005 | 41.0% |

**Wrong premise 1: `api_surface_roots` is not this artifact's problem.** It seeds
a root per `Az` symbol only in FULL mode. This build runs `mode=app`, where the
mini has **1,083 roots, 37 of them `Az*`**, and `Az*_toDbgString` and `AzPdf_*`
seed **zero** roots. Filtering the API root set is still right for a full lift;
it is worth nothing here. The `Az*` roots' entire exclusive cost is 14.43 MB and
27.7%, essentially all of it `solveLayoutReal`, which is genuine boot path.

**Wrong premise 2: PDF is not in the payload.** `web-lift-static` does not enable
`azul/pdf`, so no PDF crate is lifted at all. The whole run contains six
pdf-named symbols (`AzPdf_computePagination`, `AzPdf_fromDomInCallback`,
`Pdf::compute_pagination`, `ParsedFont::parse_pdf_font_metrics`,
`azwriter::on_export_pdf`, `azwriter::pdf_bytes`) and none of them roots the
mini. Splitting PDF out is the right instinct applied to the wrong payload.

**The real lever is the measure-DOM trampoline: 10.57 MB, 20.3% of the mini.**
It is a *root* in the walk graph, which is exactly the structural property the
chunk plan requires — being a root means **no static call edge reaches it**, so
it is entered only through `__az_indirect_dispatch`. That is CH2
"measure/virtualize", already classified lazy. Nothing about the boot path needs
it resident.

# The chunk plan (measured on run 41)

Four chunks, disjoint over 4,426 functions:

| chunk | holds | wasm | fns |
|---|---|---|---|
| **CH0 boot-core** (eager) | init, JSON hydrate, markdown to DOM, CSS parse, cascade, solver3, taffy, display list | 14.34 MB | 1,715 |
| **CH1 shape** (awaited, not lazy) | text3, allsorts, rust_fontconfig, font, glyph cache | 8.28 MB | 916 |
| **CH2 measure/virtualize** (lazy) | measure_dom, scratch_layout, layout_document/bfc/ifc, flexbox/grid | 6.56 MB | 548 |
| **CH3 cold/diagnostics** (lazy) | core::fmt, Debug/Display, dead desktop code | 3.37 MB | 1,247 |

**The uncomfortable result: 89.8% of the mass is on the boot path.** Before
AzWriter paints a pixel it must parse JSON, parse markdown, parse CSS, build the
DOM, cascade, load and shape fonts, and solve layout. There is no small hot set,
so **chunking buys latency, not download size** — CH0 can execute while CH1
streams. Bytes never fetched on a normal first paint: CH2 + CH3, 30.5%.

The structural facts that make it work:

- **CH0 to CH2 and CH0 to CH3 have ZERO static call edges.** Both are entered
  only through `__az_indirect_dispatch`, so the whole lazy-loading problem is
  concentrated in one switch rather than spread over thousands of call sites.
- **A lazy chunk calling into the always-resident core is free.** Only
  core-to-lazy edges need boundary machinery, and there are none.
- **`__az_indirect_dispatch` currently names all 4,965 bodies** (4,965
  `declare`, 9,948 switch cases). That is why `--gc-sections` cannot strip
  anything today, and it must be split per chunk.

## Hazards that must be handled before shipping this

1. **Data segments clobber the live heap.** Every `instantiate` replays that
   module's data segments over the *shared* memory, and the loader already had
   to move init/hydrate below all instantiation for exactly this reason. A
   lazily instantiated chunk violates that invariant by construction. Needs
   disjoint per-chunk mirror bands asserted at build time, or lazy chunks with
   no data segments at all.
2. **The async problem is solved at the JS export boundary, not in the shim.** A
   boundary import is a synchronous wasm to JS to wasm call and cannot await. But
   every path into a lazy chunk starts at a JS-called export, and JS *is* async
   there — so `await chunkReady(k)` before the export, prefetch chunks at
   bootstrap, and let the residual miss be a trap (CH2, an assertion that should
   never fire) or an existing stub (CH3, a no-op formatter degrades a log line).
3. **CH2's value is structurally fragile.** The same seam measures 11.99 MB in
   run 41 but 1.61 MB in run 39, because run 39 lifted `run_track_frames` — a
   *second* caller of `layout_document`. One extra caller collapses the seam 7x
   and silently moves ~10 MB back into the eager core. Whatever ships must fail
   the build if CH0's in-edge count to CH2 exceeds 1.

## Seams measured and killed

grid (0.06 MB), diagnostics inside the boot closure (0.13 MB), desktop-dead code
(0.13 MB), raster/webrender (0.14 MB) — all too small, consistently in two runs.
Table layout has 2.80 MB behind it but 174 crossing edges over 110 callees: the
widest cut measured for the least mass.

**The dead desktop code is a bug, not a seam.** ~0.95 MB of `pdb`, `cpal`,
`keyring`, `wasapi` and `std::sys::pal::windows::pipe` is in the wasm *only*
because `.rdata` fn-pointer harvesting enqueued it. Fix it at the harvest site —
which is what `plausible_object_extent` now does — rather than spending a chunk
on it.

**Before building any of this: run `AZ_FN_COVERAGE`.** The first-paint core above
is inferred from the call graph, and a runtime measurement supersedes it for one
build's cost.

---

# Where the expansion actually is

Two corrections to earlier numbers in this document, both from measuring the
linked artifact instead of the object files.

**Expansion is ~9x, not 17.7x.** The 17.7x came from summing `.o` bytes. Every
object contains the lifted function TWICE - once as the body `sub_<hex>` and
once inlined into the export wrapper `__az_dep_<hex>`, because
`inject_alwaysinline` marks the body `alwaysinline` so the wrapper absorbs it.
Counting the linked mini instead: 30.51 MB of wasm from ~3.4 MB of native
`.text`, so **~9x**. rustc's own x86-to-wasm ratio is ~3x, so the gap is 3x,
not 6x. Matching rustc would put the mini near 10 MB raw / ~1.1 MB brotli -
which is exactly the stated target, and makes expansion the lever that decides
whether the target is reachable at all.

**The double compile is a BUILD cost, not a size cost.** The linked mini has
3,831 defined functions for 3,791 lifted ones - about 1.01 per function - so
`--gc-sections` already drops the redundant body once the wrapper has inlined
it. `obj / mini = 1.98x` is that GC, not shipped duplication. What it does cost
is build time: llc compiles every function twice, and on the largest modules
that is two 85,000-line functions per object across 3,791 objects. Since llc
spawns are also what wedged runs 39 and 40 on the CreateProcess lock, halving
them is worth doing on its own merits - but do not expect bytes from it.

Worth stating plainly because I nearly reported the opposite: seeing the body
and the wrapper at 50.2% and 49.7% of every module looks exactly like a 2x size
bug. It is not. Count functions in the linked wasm before believing it.

**`__az_indirect_dispatch` is 6 lines in a per-function module**, not the giant
switch - the big one lives in its own object (399 KB, 1.3% of the mini) and
overrides the weak no-op. An earlier note here implying the dispatcher was
inlined everywhere was a measurement artifact: the span was computed as
"everything after its `define`", which swept up the functions that follow it.

# The engine bundle is carrying app payload

`azul-mini` is meant to be the engine half, but the lift seeds one extra
fn-pointer root - `azwriter::web_state::app_state_from_json` - and **6.62 MB
across 247 functions is reachable only through it**, 12.9% of the reachable
mass:

| crate | MB |
|---|---|
| azul_css (the app's own CSS parsing) | 2.10 |
| pulldown_cmark (markdown) | 1.77 |
| alloc | 0.68 |
| core | 0.47 |
| serde_json | 0.33 |
| azul_simplecss | 0.32 |
| azwriter | 0.26 |

This is the cleanest cut available - one root, no shared mass to untangle, and
app code by definition. Every app should get its own hydration chunk; the engine
mini should keep none of it. Simpler than the four-way CH0..CH3 split, and it is
what "azul-mini = the engine half" actually means.

> Pair a log with its OWN scratch dir. Export names are `__az_dep_<native_hex>`
> and every rebuild shifts the image base, so a mismatched pair resolves no
> sizes at all and reads as "nothing there" rather than as an error. This bit me
> once in this very analysis.

---

# Can the 1 MB target be hit? An honest accounting

Every number below is measured on a real artifact unless marked otherwise.

**Where it stands.** The last boot-*verified* size is run 41's 34.13 MB raw /
3.56 MB brotli. Runs 42 and 44 measured lower (3.23 and 2.95 MB) but both
included a fn-pointer seed bound that dropped a real dispatch target, so the
page trapped at boot - those numbers were partly won by deleting code that was
needed and must not be quoted until a booting build reproduces them.

**What the remaining backlog is worth**, at measured values:

| lever | measured effect | status |
|---|---|---|
| CFG-liveness flag DSE | -13% on the largest function | shipped |
| `%PC` liveness, calls transparent | a further ~4% | measured, unshipped |
| app payload out of the engine mini | 6.62 MB of 51.27 MB reachable = ~13% | designed |
| browser-excluded crates | 0.43% in app mode | shipped |
| pathological-expansion outliers | 1.19% of objects | not worth it |
| `wasm-opt -Oz` | **+2.9% delivered** (worse) | opt-in only |

Compounding the unshipped ones onto a ~3.5 MB honest baseline lands near
**2.9-3.0 MB**. That is not 1 MB, and no combination of the remaining backlog
gets there.

**The gap is expansion, and it is architectural.** The linked mini is ~9x its
native `.text`; rustc's own x86-to-wasm ratio is ~3x. Matching rustc would put
the mini at ~10 MB raw / ~1.1 MB brotli - which is the target, almost exactly.
So the target is reachable if and only if the expansion is closed, and closing
it is not a matter of more IR peepholes.

The cause is visible in what the wasm is made of. In a median lifted function:

| | share of instructions |
|---|---|
| `local.get` / `local.set` / `local.tee` | **48.8%** |
| loads | 11.7% |
| stores | 11.6% |
| arithmetic | 7.0% |
| calls | 0.3% |

Half of it is SSA shuffling, and 86% of the surviving stores target `%state` or
`%state_buf`. `%state_buf` is an `alloca` in the export wrapper - the lifted
body is deliberately `alwaysinline`d into it precisely so SROA could promote
that alloca - and SROA does not fire, because the body passes the same pointer
to ~200 lifted callees. The State escapes, so every register lives in memory,
so every access is a GEP plus a load or store plus the locals to carry them.

The passes in this document work *around* that escape one field at a time
(flags via ABI liveness, PC via its argument). Each is worth low double digits.
Closing it properly means changing the lifted ABI so callees take the live
registers as scalar arguments and return them, instead of sharing one State
buffer - at which point SROA promotes the whole thing and the shuffling
collapses. That is the change that buys 3x; nothing smaller does.

**Recommendation.** Treat ~2.9 MB as the floor for the current architecture,
bank the remaining backlog to reach it, and scope the ABI change separately
rather than expecting the incremental passes to close a 3x gap. Also worth
noting: the delivered size is already *below* rustc's own output for comparable
code (2.95 vs 4.23 MB brotli for printpdf), so the 1 MB target is not "catch up
with native" - it is "beat native by 4x", which is a different and much harder
statement.

---

# State of play: what is measured, what is blocked

## The blocker

Every run since the deadlock fix completes the lift and then **traps at boot**,
so no size number since run 41 is quotable. The trap is an indirect call to
synth `0xec5450`, and that address is in **`.rdata`**:

```
.text    rva 0x00001000 .. 0x00e047ca   EXEC,READ,CODE
.rdata   rva 0x00e05000 .. 0x01236944   READ,INITDATA   <-- the target
```

So it is a **data address being called as a function pointer** — a mirroring or
pointer-translation problem, not a discovery problem. Three runs were spent on
the discovery side (the fn-ptr seed bound, `plausible_object_extent`, the
`NeverLift` classifiers) before that was established, all of it wasted.

It appears in runs 44, 45 and 47 across three different discovery
configurations, which is what makes it look pre-existing rather than caused by
the size work.

Narrowed to three exports by the loader sequence: `registerCbNodeKind`,
`setLayoutCbTableIdx`, `setRefAny` are the calls between `AzStartup_init` (which
logs) and `setFallbackFont` (which has its own catch). `setRefAny` is the
suspicious one — `AzRefAny` carries destructor and clone function pointers,
exactly the shape that calls a pointer loaded from `.rdata`.

## What is measured but unverified

| lever | measured | state |
|---|---|---|
| private flag storage | **-43.0%** on one function | shipped, unverified |
| `%PC` privatization | a further -11.8% (to -54.8%) | **backed out** — caused unaligned dispatch targets |
| CFG-liveness flag DSE | -13.0% on one function | shipped |
| state-store DSE | -22.1% over 2,718 functions | shipped |

Artifact-level, with the same discovery config: run 46 (flags + `%PC`) linked a
28.99 MB mini against run 47's (flags only) 30.67 MB, so `%PC` alone is worth
**5.8% of the mini** — and run 46 did that on 4,719 functions where run 45 had
3,797, i.e. it absorbed 24% more code and still came out smaller.

## Two diagnostics that changed how this is debugged

**Ask what section an address is in before anything else.** `0xec5450` is
16-byte aligned, so an alignment check called it "a plausible function entry"
and sent the hunt in the wrong direction for three runs. `.rdata` tables are
16-byte aligned exactly like code. Alignment is a hint; the section is decisive.

**Use one run's log.** Synth addresses are assigned per image band and every
build lays out differently — the same `dragon::mul_pow10` appears at three
different synth addresses across saved logs. Merging logs does not blur an
answer, it invents one: it produced a confident, entirely false identification.
The gate now rotates its log so run N stays diagnosable after run N+1.

## Naming any synth address

1. `synth == RVA` (confirmed: the band delta from 60 neighbours was unanimously
   the ImageBase, 0x140000000).
2. `llvm-symbolizer --obj=<exe> --demangle` on ImageBase + RVA.
3. PE section lookup for the code/data verdict.

Use the *current* build's `AzWriter.{exe,pdb}`, never an older dump.

A release mini cannot resolve a trap frame at all — it strips both the
`__az_dep_*` exports and the name section. `AZ_WASM_DEBUG=1` keeps them, and
that path had been broken since before this work: wasm-ld rejects
`--keep-section=name`, so every debug link produced an 8-byte stub.

---

# State of play: what is measured, what is blocked

## The blocker

Every run since the deadlock fix completes the lift and then **traps at boot**,
so no size number since run 41 is quotable. The trap is an indirect call to
synth `0xec5450`, and that address is in **`.rdata`**:

```
.text    rva 0x00001000 .. 0x00e047ca   EXEC,READ,CODE
.rdata   rva 0x00e05000 .. 0x01236944   READ,INITDATA   <-- the target
```

So it is a **data address being called as a function pointer** — a mirroring or
pointer-translation problem, not a discovery problem. Three runs were spent on
the discovery side (the fn-ptr seed bound, `plausible_object_extent`, the
`NeverLift` classifiers) before that was established, all of it wasted.

It appears in runs 44, 45 and 47 across three different discovery
configurations, which is what makes it look pre-existing rather than caused by
the size work.

Narrowed to three exports by the loader sequence: `registerCbNodeKind`,
`setLayoutCbTableIdx`, `setRefAny` are the calls between `AzStartup_init` (which
logs) and `setFallbackFont` (which has its own catch). `setRefAny` is the
suspicious one — `AzRefAny` carries destructor and clone function pointers,
exactly the shape that calls a pointer loaded from `.rdata`.

## What is measured but unverified

| lever | measured | state |
|---|---|---|
| private flag storage | **-43.0%** on one function | shipped, unverified |
| `%PC` privatization | a further -11.8% (to -54.8%) | **backed out** — caused unaligned dispatch targets |
| CFG-liveness flag DSE | -13.0% on one function | shipped |
| state-store DSE | -22.1% over 2,718 functions | shipped |

Artifact-level, with the same discovery config: run 46 (flags + `%PC`) linked a
28.99 MB mini against run 47's (flags only) 30.67 MB, so `%PC` alone is worth
**5.8% of the mini** — and run 46 did that on 4,719 functions where run 45 had
3,797, i.e. it absorbed 24% more code and still came out smaller.

## Two diagnostics that changed how this is debugged

**Ask what section an address is in before anything else.** `0xec5450` is
16-byte aligned, so an alignment check called it "a plausible function entry"
and sent the hunt in the wrong direction for three runs. `.rdata` tables are
16-byte aligned exactly like code. Alignment is a hint; the section is decisive.

**Use one run's log.** Synth addresses are assigned per image band and every
build lays out differently — the same `dragon::mul_pow10` appears at three
different synth addresses across saved logs. Merging logs does not blur an
answer, it invents one: it produced a confident, entirely false identification.
The gate now rotates its log so run N stays diagnosable after run N+1.

## Naming any synth address

1. `synth == RVA` (confirmed: the band delta from 60 neighbours was unanimously
   the ImageBase, 0x140000000).
2. `llvm-symbolizer --obj=<exe> --demangle` on ImageBase + RVA.
3. PE section lookup for the code/data verdict.

Use the *current* build's `AzWriter.{exe,pdb}`, never an older dump.

A release mini cannot resolve a trap frame at all — it strips both the
`__az_dep_*` exports and the name section. `AZ_WASM_DEBUG=1` keeps them, and
that path had been broken since before this work: wasm-ld rejects
`--keep-section=name`, so every debug link produced an 8-byte stub.

---

# The boot trap is not the size work — proven

Disabling every hand-written IR pass (`AZ_NO_IR_PASSES=1`: the state-store DSE,
the CFG-liveness flag DSE and flag privatization) produces the **identical**
bug: the same constant `15488080` in the same three functions, stored to the
same `__remill_missing_block` recorder at `0x400F8` and passed to the same
dispatcher. The trap predates all of it, and the measured wins — -22.1%, -13%
and -43% respectively — stand once boot is fixed.

Proven **statically**. That run died on the `llc` spawn wedge before it ever
served, but it had already linked its mini, so grepping its `.opt.ll` answered
the question with no boot at all. When a run dies late, check what it already
produced before writing it off.

## What the trap actually is

`Display for str` (and the `String`/`alloc::string` equivalents) lift to:

```llvm
%v   = load i64 [%p]        ; str.ptr
%v13 = load i64 [%p+8]      ; str.len
store %v   -> RCX           ; set up arguments
store %v13 -> RDX
store i64 15488080 -> PC    ; tail-call target
… __remill_missing_block …
```

A thunk that sets up arguments and **tail-jumps**. remill resolved the target to
a compile-time constant, so it was a *direct* jump — which means
`x86_scan::tail_jmp_targets` saw it. That function's own documentation explains
what then happened: targets are "filtered through the SymbolTable, whose lookup
is exact (by-address), so an intra-fn `jmp` to a mid-fn label yields None and
drops out."

So a tail-call target that matches no symbol exactly is **silently dropped**,
never lifted, gets no dispatcher case, and the missing-block path fires at
runtime. The fix direction is to stop dropping such targets silently — lift them
as synthetic functions, or at minimum report them.

One thing to resolve first: `0xec5450` read as an RVA lands past `.text`, which
is impossible for a jump target. Either it belongs to a different synth band
(another module) or the bytes fed to remill for these thunks were wrong.

## Two process notes that cost time

**Check the server is actually up, and that the mini hash changed, before
believing a boot result.** A boot test run against a dead server returned a
complete, plausible trap stack — from the *previous* run's cached page. The
tells were `HTTP 000` and a wasm URL hash identical to the earlier run's.

**Debug-link runs are wedge-prone.** `--lto-O0` IR is much larger, `llc` runs
long, and that is what tripped the 314-second `CreateProcess` wedge. Use a normal
link unless symbol names are genuinely needed.
