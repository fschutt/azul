---
slug: memory-profiling
title: Memory and Profiling
language: en
canonical_slug: memory-profiling
audience: external
maturity: wip
guide_order: 210
topic_only: false
short_desc: Tracking allocations and per-frame budgets — the profiler hooks and how to read its output.
prerequisites: [debugging]
tracked_files:
  - core/src/debug.rs
  - dll/src/desktop/logging.rs
  - dll/src/desktop/shell2/common/debug_server.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T00:00:00Z
---

# Memory and Profiling

> **WIP.** Probe API names and JSONL field names may shift; the env-var surface (`AZ_PROFILE`, `AZ_PROFILE_OUT`, `AZ_E2E_TEST`) is stable.

The profiler is gated by the `probe` cargo feature in `azul-layout` and driven entirely by environment variables. Without the feature it compiles to inline no-ops; with the feature on but no `AZ_PROFILE` set, every probe checks one atomic and returns. There is no static cost to leaving probes in production builds.

```bash
AZ_PROFILE=cpu ./my_app                                    # per-phase CPU dump to stderr
AZ_PROFILE=memory ./my_app                                 # heap-breakdown dump to stderr
AZ_PROFILE=heap,jsonl AZ_PROFILE_OUT=/tmp/run.jsonl ./my_app   # phase boundaries to JSONL
AZ_PROFILE=heap,jsonl,detail AZ_PROFILE_OUT=/tmp/d.jsonl ./my_app   # add per-step probes
```

## `AZ_PROFILE` tokens

`AZ_PROFILE` is a comma-separated list. Tokens are independent flags, not modes; combine freely. Whitespace is trimmed, matching is case-insensitive, unknown tokens are ignored.

| Token | Alias | Effect |
|---|---|---|
| `memory` | `mem` | Per-frame heap-breakdown dump (StyledDom, layout cache, text cache, prop cache, RSS) to stderr. |
| `cpu` | `perf` | Per-phase wall-clock timings from `Probe::span` (layout, style, cascade, paint, callbacks, …). |
| `cascade` | `css` | Top-N CSS properties by cascade-walk count per frame. |
| `heap` | — | Phase-boundary heap probes inside `regenerate_layout` (`emit_phase_heap`). Silent without `jsonl`. |
| `jsonl` | — | Format `heap` probes as JSONL to `AZ_PROFILE_OUT`. Requires `heap`. |
| `detail` | — | Opt in to fine-grained per-step probes (e.g. `rf_*` labels in `request_fonts`, `_extra` payloads). Layered on top of `heap`. |

`AZ_PROFILE_OUT=<path>` names the JSONL destination. With `jsonl` set and `AZ_PROFILE_OUT` unset, writers silently skip — benchmarks stay clean.

## What each token costs you

- **`cpu`** wraps phase functions with an `Instant::now()` pair on entry/exit and one Vec push per span. Sub-microsecond per probe; safe to leave on in CI.
- **`memory`** computes per-cache `memory_report()` summaries every frame. Non-trivial — typical 20–100 µs depending on DOM size. Use during investigation, not in steady-state runs.
- **`heap` + `jsonl`** queries `mstats().bytes_used` (macOS) or `/proc/self/statm` (Linux) at every named boundary. Roughly the cost of one syscall (~200 ns).

## What each report contains

`AZ_PROFILE=cpu` prints one block per frame to stderr, one line per `Probe::span` name with mean, p99, and call count over the frame. `AZ_PROFILE=memory` prints one block per frame with field-level byte counts pulled from each cache's `memory_report()`:

```text
StyledDom        node_count=4083  total=2.1 MiB
  node_hierarchy 4083 * 16  = 65 KiB
  node_data      4083 * 96  = 392 KiB
  styled_nodes   4083 * 24  = 98 KiB
  prop_cache    css_props=412 KiB  computed=78 KiB  compact=124 KiB
  ...
LayoutCache      tree_hot=88 KiB  warm=412 KiB  cold=64 KiB  ...
TextCache        logical=18 KiB  shaped_glyphs=240 KiB  shaped_clusters=87 KiB
RSS              78.2 MiB
```

Each cache exposes a `memory_report()` method (`StyledDom::memory_report` in `core/src/styled_dom.rs:901`, `Solver3LayoutCache::memory_report`, `TextLayoutCache::memory_report`) that returns a struct of byte counts. Your callbacks can call these directly if you want the same numbers without setting `AZ_PROFILE`.

## Heap vs RSS

The profiler exposes both numbers because they tell you different things:

- **Heap** (`malloc_heap_bytes`, `azul-layout/src/probe.rs:442`) — bytes the libc allocator currently holds. On macOS this is `mstats().bytes_used`. A growth here is a Rust-level retention: an `Arc` chain never dropped, a `Vec` never shrunk, a `Box<T>` forgotten. mmap'd regions (thread stacks, file-backed fonts, GL buffers) are *not* counted.
- **RSS** (`current_rss_bytes`, `probe.rs:387`) — kernel resident-set size, what `top` and Activity Monitor show. Includes shared library text, mmap'd fonts, GPU driver pages.
- **`phys_footprint`** (macOS only, `probe.rs:474`) — what Activity Monitor's "Physical footprint" shows; excludes shared text pages. More honest than RSS for short-lived processes.

A leak in *your* code shows up in both. A leak that shows up only in RSS but not in heap usually means an mmap'd resource (a font file, a texture) is being held longer than it needs to be.

## Phase-boundary probes

`emit_phase_heap(label)` (`probe.rs:624`) writes one line of JSONL per phase boundary inside `regenerate_layout`, with a monotonic `call` id grouping all labels from a single layout pass:

```jsonl
{"ev":"phase","label":"start","heap":18874368,"call":42}
{"ev":"phase","label":"styled_dom_built","heap":19135488,"call":42}
{"ev":"phase","label":"layout_solved","heap":19921920,"call":42}
{"ev":"phase","label":"display_list_built","heap":19987456,"call":42}
{"ev":"phase","label":"end","heap":19135488,"call":42}
```

Heap delta between adjacent labels with the same `call` id is the bytes retained by that phase. If `start` and `end` differ, that frame leaked. Run a few hundred frames, group by label, and fit a linear `field_bytes = a * iter + b` — anything with `a > 0` is a suspect.

## Hunting a leak with `AZ_E2E_TEST`

`AZ_E2E_TEST=<scenario.json>` (gated by the `e2e-test` cargo feature, defined in `dll/src/desktop/shell2/common/e2e_test.rs`) takes over `main()` to run a deterministic resize/tick scenario against `HeadlessWindow`. It probes RSS at a configurable cadence, compares against caps, and exits 0/1.

```json
{
  "name": "calc-resize-leak",
  "warmup_ticks": 10,
  "steps": [
    { "action": "resize",      "width": 800, "height": 600 },
    { "action": "resize",      "width": 600, "height": 400 },
    { "action": "resize_full", "width": 800, "height": 600 },
    { "action": "tick" }
  ],
  "loop": { "iterations": 500 },
  "rss_probes": {
    "every_n_iterations": 50,
    "warmup_skip": 0,
    "assert_growth_mib_max": 5.0,
    "assert_absolute_mib_max": 80.0,
    "memory_breakdown": true
  },
  "output": {
    "jsonl_path":    "/tmp/calc.jsonl",
    "summary_path": "/tmp/calc.summary.jsonl"
  }
}
```

`assert_growth_mib_max` is the per-run delta between baseline (after warmup) and the final RSS sample; `assert_absolute_mib_max` caps total RSS at any point. Either breach exits the process with code `1`. With `memory_breakdown: true` each probe also emits a flat `mem` event covering every measurable byte: per-StyledDom field, per layout-cache field, per text-cache field, per-manager counts. Feed the JSONL to a regression analyzer and let it pick the field whose slope grew (the calc.c regression test in `dll/tests/leak_regression.rs` is built on the same shape).

```bash
cargo run --release --features e2e-test -- \
  AZ_E2E_TEST=calc.json
# → exit 0 (under cap) or 1 (growth/absolute breach)
# → JSONL events in /tmp/calc.jsonl
```

## RefAny live counts

There is no global "live RefAny count" probe today. The closest signals are per-cache and per-manager counters exposed through `debug_counts()` and folded into `memory_breakdown` output:

| Source | What it counts |
|---|---|
| `ScrollManager::debug_counts` | scroll states, external IDs, scrollbar states |
| `HoverManager::debug_counts` | per-DOM hover points, total entries |
| `VirtualViewManager::debug_counts` | tracked VirtualViews and their pipelines |
| `GestureDragManager::debug_counts` | active gesture sessions and long-press timers |
| `RendererResources` | currently registered fonts and images |

Each `RefAny` carries an internal `RefCountInner` with strong/weak counters; for ad-hoc inspection during a debug session, query `{"op":"get_node_dataset","node_id":<id>}` to get the JSON-serialised contents and the `RefAnyMetadata` block (type name, size). A growing entry in any of the per-manager counts across iterations is the same kind of signal as heap growth: something is created per-frame and never dropped.

## A workflow for "why is this growing?"

1. Reproduce with `AZ_PROFILE=memory`. Watch the per-frame block. Identify which cache's bytes grow.
2. If heap grows but no individual cache does, the leak is outside the layout pipeline — check user code in callbacks that retain `RefAny` clones.
3. Re-run with `AZ_PROFILE=heap,jsonl,detail AZ_PROFILE_OUT=/tmp/probes.jsonl`. Group by phase label. The phase whose heap delta grows over time is where the leak lives.
4. Build a deterministic `AZ_E2E_TEST` scenario that reproduces the trigger (a resize loop, a route change, …). Set `assert_growth_mib_max` slightly under the observed growth so the regression becomes a binary signal.
5. Commit the scenario JSON next to the test that runs it (see `dll/tests/leak_regression.rs` for the pattern).

This is the same loop the framework's own test suite uses to guard against the rust-fontconfig resize-leak regression — the only difference is that the test asserts on `mstats().bytes_used` directly instead of going through `AZ_E2E_TEST`.

## Allocator selection

The `azul-layout` crate has feature flags for alternate allocators:

- `allocator_mimalloc` — `mi_collect(true)` is called before every RSS sample so freed memory is returned to the OS; otherwise mimalloc holds onto pages and inflates RSS.
- `allocator_jemalloc` — calls `mallctl("arena.0.purge")` before sampling for the same reason.

Without either feature, the system allocator is used; on macOS the default `libmalloc` is what `mstats` reads. Pick an allocator that matches your production deployment so the numbers match what users see — RSS curves change shape with the allocator.
