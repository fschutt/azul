---
slug: memory-profiling
title: Profiling
language: en
canonical_slug: memory-profiling
audience: external
maturity: wip
guide_order: 210
topic_only: false
short_desc: Tracking allocations and per-frame budgets
prerequisites: [debugging]
tracked_files:
  - core/src/debug.rs
  - dll/src/desktop/logging.rs
  - dll/src/desktop/shell2/common/debug_server.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T00:00:00Z
---

# Profiling

> **WIP.** JSONL field names may shift; the env-var surface (`AZ_PROFILE`, `AZ_PROFILE_OUT`, `AZ_E2E_TEST`) is stable.

The profiler is gated by a build feature in the layout crate and driven entirely by environment variables. Without the feature it compiles to inline no-ops; with the feature on but no `AZ_PROFILE` set, every probe checks one atomic and returns. There is no static cost to leaving probes in production builds.

```bash
AZ_PROFILE=cpu ./my_app                                            # per-phase CPU dump to stderr
AZ_PROFILE=memory ./my_app                                         # heap-breakdown dump to stderr
AZ_PROFILE=heap,jsonl AZ_PROFILE_OUT=/tmp/run.jsonl ./my_app       # phase boundaries to JSONL
AZ_PROFILE=heap,jsonl,detail AZ_PROFILE_OUT=/tmp/d.jsonl ./my_app  # add per-step probes
```

## AZ_PROFILE tokens

`AZ_PROFILE` is a comma-separated list. Tokens are independent flags, not modes; combine freely. Whitespace is trimmed, matching is case-insensitive, unknown tokens are ignored.

- `memory` (alias `mem`). Per-frame heap-breakdown dump (DOM caches, layout cache, text cache, RSS) to stderr.
- `cpu` (alias `perf`). Per-phase wall-clock timings (layout, style, cascade, paint, callbacks, etc.).
- `cascade` (alias `css`). Top-N CSS properties by cascade-walk count per frame.
- `heap`. Phase-boundary heap probes inside the layout pass. Silent without `jsonl`.
- `jsonl`. Formats `heap` probes as JSONL to `AZ_PROFILE_OUT`. Requires `heap`.
- `detail`. Opts in to fine-grained per-step probes. Layered on top of `heap`.

`AZ_PROFILE_OUT=<path>` names the JSONL destination. With `jsonl` set and `AZ_PROFILE_OUT` unset, writers silently skip — benchmarks stay clean.

## What each token costs you

- **`cpu`** wraps phase functions with a timestamp pair on entry/exit. Sub-microsecond per probe; safe to leave on in CI.
- **`memory`** computes per-cache summaries every frame. Non-trivial — typical 20–100 µs depending on DOM size. Use during investigation, not in steady-state runs.
- **`heap` + `jsonl`** queries the libc allocator at every named boundary. Roughly the cost of one syscall (~200 ns).

## What each report contains

`AZ_PROFILE=cpu` prints one block per frame to stderr, one line per phase span name with mean, p99, and call count over the frame. `AZ_PROFILE=memory` prints one block per frame with field-level byte counts pulled from each cache:

```text
StyledDom        node_count=4083  total=2.1 MiB
  node_hierarchy 4083 * 16  = 65 KiB
  node_data      4083 * 96  = 392 KiB
  styled_nodes   4083 * 24  = 98 KiB
  prop_cache     css_props=412 KiB  computed=78 KiB
  ...
LayoutCache      tree_hot=88 KiB  warm=412 KiB  cold=64 KiB
TextCache        logical=18 KiB  shaped_glyphs=240 KiB  shaped_clusters=87 KiB
RSS              78.2 MiB
```

## Heap vs RSS

The profiler exposes both numbers because they tell you different things:

- **Heap** — bytes the libc allocator currently holds. On macOS this is `mstats().bytes_used`. A growth here is a Rust-level retention: an `Arc` chain never dropped, a `Vec` never shrunk, a `Box<T>` forgotten. mmap'd regions (thread stacks, file-backed fonts, GL buffers) are not counted.
- **RSS** — kernel resident-set size, what `top` and Activity Monitor show. Includes shared library text, mmap'd fonts, GPU driver pages.
- **`phys_footprint`** (macOS only) — what Activity Monitor's "Physical footprint" shows; excludes shared text pages. More honest than RSS for short-lived processes.

A leak in your code shows up in both. A leak that shows up only in RSS but not in heap usually means an mmap'd resource (a font file, a texture) is being held longer than it needs to be.

## Phase-boundary probes

`AZ_PROFILE=heap,jsonl` writes one line of JSONL per phase boundary inside the layout pass, with a monotonic `call` id grouping all labels from a single layout pass:

```jsonl
{"ev":"phase","label":"start","heap":18874368,"call":42}
{"ev":"phase","label":"styled_dom_built","heap":19135488,"call":42}
{"ev":"phase","label":"layout_solved","heap":19921920,"call":42}
{"ev":"phase","label":"display_list_built","heap":19987456,"call":42}
{"ev":"phase","label":"end","heap":19135488,"call":42}
```

Heap delta between adjacent labels with the same `call` id is the bytes retained by that phase. If `start` and `end` differ, that frame leaked. Run a few hundred frames, group by label, and fit a linear trend per label — anything that climbs is a suspect.

## Hunting a leak with AZ_E2E_TEST

`AZ_E2E_TEST=<scenario.json>` (gated by a build feature) takes over `main()` to run a deterministic resize/tick scenario against a headless backend. It probes RSS at a configurable cadence, compares against caps, and exits 0/1.

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

`assert_growth_mib_max` is the per-run delta between baseline (after warmup) and the final RSS sample; `assert_absolute_mib_max` caps total RSS at any point. Either breach exits the process with code `1`. With `memory_breakdown: true` each probe also emits a flat `mem` event covering every measurable byte across the layout caches. Feed the JSONL to a regression analyzer and let it pick the field whose slope grew.

```bash
cargo run --release --features e2e-test -- \
  AZ_E2E_TEST=calc.json
# → exit 0 (under cap) or 1 (growth/absolute breach)
# → JSONL events in /tmp/calc.jsonl
```

## A workflow for "why is this growing?"

1. Reproduce with `AZ_PROFILE=memory`. Watch the per-frame block. Identify which cache's bytes grow.
2. If heap grows but no individual cache does, the leak is outside the layout pipeline. Check user code in callbacks that retain `RefAny` clones.
3. Re-run with `AZ_PROFILE=heap,jsonl,detail AZ_PROFILE_OUT=/tmp/probes.jsonl`. Group by phase label. The phase whose heap delta grows over time is where the leak lives.
4. Build a deterministic `AZ_E2E_TEST` scenario that reproduces the trigger (a resize loop, a route change, …). Set `assert_growth_mib_max` slightly under the observed growth so the regression becomes a binary signal.
5. Commit the scenario JSON next to the test that runs it.

## Allocator selection

The layout crate has feature flags for alternate allocators (`allocator_mimalloc`, `allocator_jemalloc`). They release freed pages to the OS before each RSS sample, otherwise the allocator holds onto pages and inflates RSS. Pick an allocator that matches your production deployment so the numbers match what users see — RSS curves change shape with the allocator.

## Coming Up Next

- [Debugging](debugging.md) — Debug overlays, the inspector, and structured logging
- [End-to-End Testing](e2e-testing.md) — Driving an Azul app from a script for tests
- [Headless Rendering](headless-rendering.md) — Running the pipeline without a window
