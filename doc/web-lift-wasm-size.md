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

- **Panic / formatting machinery.** 738 functions, 2.6 MB, ~6%. Worth stubbing
  (a trap is cheaper than the panic path) but not transformative — and only
  **11 functions** are reachable *exclusively* through panics. `fmt`/`Debug` is
  genuinely live: markdown rendering, CSS value formatting, PDF text.
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
- Stubbing panics for the ~6%.
- `zf` needs real liveness (584 genuine readers), unlike the zero-load flags.
