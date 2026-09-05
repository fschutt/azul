# azul web e2e harness — "mini puppeteer"

Runs the **existing desktop AZ_E2E JSON scenarios** (`tests/e2e/*.json`, format
`E2eTest` in `dll/src/desktop/shell2/common/debug_server/full.rs`) against the
**web backend** (x86-64 lifted to wasm, served by the built-in server) in a real
headless Edge/Chrome over raw-WebSocket CDP. Implements Phase 1 + the keyboard
subset of Phase 2 of `scripts/web-e2e-harness-plan.md`.

No npm dependencies. Node 20's global `WebSocket` needs
`--experimental-websocket`; `run.mjs` re-execs itself with the flag when the
global is missing, so plain `node run.mjs` works.

## Prerequisites (the harness starts NEITHER of these)

1. **App server** on the base URL (default `http://127.0.0.1:8800`) — the app
   under test launched with `AZ_BACKEND=web://127.0.0.1:8800 AZ_LIFT_CACHE=1
   REMILL_LIFT_BIN=...` and printing `Listening on`
   (full recipe: `scripts/m9_e2e/cdp_gate.sh:15-26`; cold lifts take minutes).
2. **Browser** with CDP on `:9222`:

   ```
   "C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe" ^
       --headless=new --remote-debugging-port=9222 --disable-gpu ^
       --no-first-run --user-data-dir=%TEMP%\az-edge-headless about:blank
   ```

If either is missing the harness prints the recipe above and exits 2.

## Usage

```
node scripts/e2e-web/run.mjs <spec.json | dir-of-specs> [options]

node scripts/e2e-web/run.mjs scripts/e2e-web/specs/smoke.json         # plumbing gate
node scripts/e2e-web/run.mjs tests/e2e/hello_world_counter.json       # the P1 gate
node scripts/e2e-web/run.mjs tests/e2e --filter counter               # dir + name filter
```

| option | default | meaning |
|---|---|---|
| `--url <base>` | `http://127.0.0.1:8800` | app server |
| `--cdp <base>` | `http://127.0.0.1:9222` | DevTools endpoint |
| `--golden-dir <dir>` | `scripts/e2e-web/golden/` | golden PNG tree (commit these) |
| `--out-dir <dir>` | `target/web_e2e/` | artifacts: screenshots, diffs, console logs, `results.json` |
| `--update-golden` | off | overwrite existing goldens with this run's pixels |
| `--filter <regex>` | — | run only matching test names |
| `--isolate` | off | fresh tab per named test (default: fresh tab per spec **file**) |
| `--settle-cap-ms` | 5000 | max wait for `__az_pending == 0` per settle |
| `--settle-fallback-ms` | 500 | fixed delay when the loader has no `__az_settled` |
| `--boot-timeout-ms` | 30000 | max wait for loader bootstrap |

Exit codes: `0` all pass · `1` any test failed · `2` infrastructure error.

Per test the harness writes `<out>/<spec>/<test>/NN_<op>*.png` (a screenshot at
**every assertion point**, `_actual`/`_expected`/`_diff` for failed screenshot
asserts, `final.png`), `<out>/<spec>/console.log` (full console + exceptions),
and a machine-readable `<out>/results.json` (desktop `E2eTestResult`-shaped,
plus `settle`, `skip` reasons, and golden bookkeeping).

## Tab lifecycle (why per-FILE, not per-test)

Desktop runs all tests of a file sequentially against one app instance — later
tests build on earlier state (the whole contenteditable suite chains focus and
typed text). A page load resets the wasm app, so the default is **one fresh tab
per spec file**, tests run in order inside it. `--isolate` gives every named
test its own tab — only correct for self-contained tests like
`hello_world_counter`.

## Settle protocol + the fallback caveat

After navigation and after every input step the harness calls
`window.__az_settled(cb)` — the loader-owned signal that fires when
`window.__az_pending` (count of in-flight async work: bootstrap, wasm/font/shard
fetches) is 0 across two rAF ticks (`dll/src/web/loader_js.rs:41-62`). Dispatch
→ TLV-patch application is synchronous, so "settled + one presented frame"
means the DOM the next assert sees is final.

**Caveat:** only servers (re)started after 2026-08-17 serve a loader with
`__az_pending`/`__az_settled`. Against an older server the harness detects the
missing hook and falls back to a **fixed delay** (`--settle-fallback-ms`,
default 500 ms) — timing-heuristic, so slow shard fetches can race asserts. The
run prints a NOTE when the fallback was used; restart the server from a current
build to get deterministic settling. A settle **timeout** (pending never
reaches 0 within `--settle-cap-ms`) fails the boot phase loudly; mid-test it is
recorded on the step (`settle.mode: "timeout"` + a loud log line) without
aborting, since a stuck counter usually means a loader accounting gap, not app
misbehaviour.

## How coordinates map through wasm rects

Web pages emitted by the backend mirror every azul node as `id="az_N"` (DFS
order). For node-addressed actions (`click`/`scroll_node_by` by `selector`,
`text`, or `node_id`) the harness resolves the DOM element, takes `N` from its
`az_N` id, and reads entry `N` of the **wasm positioned-rects cache**
(`__azProbe.mini.AzStartup_getPositionedRectsLen/Ptr(state)`, 4×u32 = x,y,w,h
per node) — that is azul's own solved geometry, the hit-test source of truth.
Entries with `w==0`, `h==0`, or **bit 31 set in any field are sentinels** and
are skipped. If the cache is unavailable (solve gated/trapped) or the entry is
a sentinel, it falls back to the element's `getBoundingClientRect()` center
(pattern proven by `scripts/cdp_click_hw.js`). Each resolution is logged:
`resolved {...} -> (x,y) via wasm-rect az_N | element-center`.

Selector translation (mirror emission rules in `dll/src/web/html_render.rs`):
scenario `#foo` (user id) → tried as `[data-az-id="foo"]` first (user ids are
remapped onto that attribute), then verbatim; classes/tags/attribute selectors
pass through; `text` targets the deepest element whose trimmed `textContent`
matches exactly (substring fallback); `node_id: N` → `#az_N` (best effort — DFS
counter, prefer selector/text in portable scenarios).

## Golden workflow

- `assert_screenshot` steps compare against
  `<golden-dir>/<name>.png` where `<name>` is the scenario's `reference` path
  with everything up to `reference_images/` (or `reference_images_web/`)
  stripped — desktop references are cpurender pixels and are **not** compared
  against browser pixels; the web keeps a parallel golden tree under the same
  relative names.
- **Missing golden ⇒ saved from the current run and the step PASSES** with a
  `baseline CREATED` note (mirrors desktop auto-baseline behaviour,
  `full.rs:3878-3888`). `results.json` counts `baselines_created` — CI should
  treat a nonzero count as a failure of the golden set, not of the app.
- `--update-golden` overwrites existing goldens (use after an intended visual
  change, then commit `scripts/e2e-web/golden/`).
- Compare semantics are the cpurender `pixel_diff` port
  (`layout/src/cpurender/pixmap.rs:424-513`): a pixel differs when **any**
  RGBA channel delta exceeds `threshold` (step param, default 2); pass when
  `diff_ratio <= max_diff_ratio` (default 0.0). Per-step web-only overrides via
  the desktop-ignored `"x-web"` object:
  `{"x-web": {"threshold": 24, "max_diff_ratio": 0.02, "mask": [[x,y,w,h]], "settle_ms": 250, "skip": true}}`
  — `mask` rectangles are excluded from comparison (blinking caret, AA-noisy
  text), `skip` skips the step on web only.
- Failures write `NN_<op>_actual.png`, `NN_<op>_expected.png`, and a
  `NN_<op>_diff.png` heatmap (red = differing pixels, blue tint = masked).

## What is portable (P1 + P2 subset)

Actions: `click` (x/y, selector, text, node_id), `double_click`, `mouse_move`,
`mouse_down`, `mouse_up`, `scroll`, `scroll_node_by`, `scroll_into_view`,
`resize`, `wait`, `wait_frame`, `relayout`, `redraw`, `key_down`/`key_up`
(azul VirtualKeyCode names, held-modifier bracketing like `LShift`+`Left`),
`text_input` (CDP `Input.insertText` on the focused element), `focus`,
`take_screenshot`. Queries (`get_state`, `get_focus_state`,
`get_selection_state`, `find_node_by_text`, `hit_test`, `get_node_layout`,
`get_dom*`, `get_scroll_states`) run as **diagnostics** — recorded in results,
never failing. Assertions: `assert_text`, `assert_exists`, `assert_not_exists`,
`assert_node_count`, `assert_layout` (gBCR; `"x-web": {"source": "wasm"}` reads
the wasm rect instead), `assert_scroll`, `assert_css` (px/color normalization;
un-normalizable value forms SKIP with a warning), `assert_screenshot`.

Recorded as **SKIP** (non-portable, plan §2.4): touch/pen/gesture ops,
`assert_app_state`/`get_app_state`/`set_app_state` (no wasm state-serializer
export yet), DOM-mutation ops, `take_native_screenshot`, window
`move`/`close`/`blur`/`dpi_changed`.

Any uncaught page exception (`RuntimeError` = wasm trap; frames are tagged
`@wasm`) fails the step/test it occurred in; exceptions during boot fail the
whole spec.

## Files

```
run.mjs               CLI entry: spec discovery, tab lifecycle, results, summary
lib/cdp.mjs           raw-WebSocket CDP client + console/exception capture
lib/driver.mjs        op → CDP step interpreter + settle logic
lib/asserts.mjs       assert_* evaluators + golden compare
lib/keymap.mjs        azul VirtualKeyCode names → CDP key events
lib/page-helpers.mjs  injected in-page bundle: selector translation, wasm rects
lib/png.mjs           pure-JS PNG decode/encode + cpurender-semantics pixel diff
specs/smoke.json      synthetic plumbing gate (load → settle → screenshot)
golden/               committed golden PNGs (created on first assert_screenshot run)
```
