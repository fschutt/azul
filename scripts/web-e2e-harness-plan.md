# Web E2E Harness Plan — "mini puppeteer" for cross-backend scenario testing

Status: P1 + P2-keyboard-subset IMPLEMENTED (2026-08-18) under `scripts/e2e-web/`
(task-directed dir name; §4.1 below still says `scripts/web_e2e/` — the layout is
otherwise as designed). Entry point: `node scripts/e2e-web/run.mjs <spec.json|dir>`;
usage, golden workflow, and the fallback-settle caveat: `scripts/e2e-web/README.md`.
Verified live 2026-08-18 against the running :8800 hello-world + Edge :9222:
`tests/e2e/hello_world_counter.json` PASSES 9/9 (clicks targeted via the wasm
positioned-rects cache, counter 5→6→8 through the lifted cb); golden
bootstrap → compare-pass → deliberate-fail-with-diff-heatmap all exercised.
One selector-translation rule was added beyond §4.2: desktop `body` ≡ mirror
`#az_0` (the az root div carries the body 8px margin; the real `<body>` wraps it
in `#az-body`). Not yet built: §5 cross-backend screenshot tiers (web-local
goldens only), §6 `run_all.sh` lane runner, `"x-web": {"allow_trap"}`.
Historical planning notes below are unchanged; claims verified against the
cited file:line as of 2026-08-17.

Goal: execute the **same JSON e2e scenarios** that already drive the desktop
backend against the **web backend** (native x86-64 lifted to wasm, served by the
built-in web server) in a real Chromium/Edge over CDP — actions, assertions, and
(eventually) screenshot comparison — so behaviour can be verified identical
between desktop and web.

---

## 1. Summary + recommendation

There are two distinct JSON "e2e" formats in the tree:

| | `AZ_E2E_TEST` | `AZ_E2E` |
|---|---|---|
| File | `dll/src/desktop/shell2/common/e2e_test.rs` | `dll/src/desktop/shell2/common/debug_server/full.rs` |
| Purpose | deterministic resize/tick **memory/perf** rig (RSS leak probes) | **behavioural** scenario runner: input events + assertions |
| Steps | `resize`, `tick`, `resize_full`, `sleep_ms` only (e2e_test.rs:39-52) | full `DebugEvent` op vocabulary + `assert_*` ops (full.rs:1526-2100, 3348-3358) |
| Assertions | RSS growth/absolute ceilings only (e2e_test.rs:63-79) | text/exists/count/layout/css/app_state/scroll/screenshot (full.rs:3372-3382) |
| Corpus | none in `tests/e2e/` (grep for `"action"` finds zero scenario files; only doc references) | **all** of `tests/e2e/*.json` (`hello_world_counter.json`, `contenteditable_overflow_test.json`, `scrolling_headless.json`, `widgets_headless_test.json`, `widgets_native_test.json`, `undo_redo.json`) |

**Recommendation: `AZ_E2E` (the `E2eTest` format) is the portable substrate.**
It is the only format with input actions and assertions, the entire scenario
corpus already uses it, `scripts/e2e_language_matrix.sh` already treats it as
the cross-language conformance format (e2e_language_matrix.sh:12-24), and its
step shape — `{"op": "...", ...params}` with unknown fields tolerated via
`#[serde(flatten)]` (full.rs:3263-3272) — is transport-agnostic and trivially
interpretable by a JS driver. `AZ_E2E_TEST` stays what it is: a memory rig.
**No new format and no fork**: the web driver interprets a documented *portable
subset* of the same files, plus an optional, desktop-ignored `"x-web"` extension
object per step (safe because extra fields land unread in the flattened
`params: serde_json::Value`).

The mini-puppeteer is a small Node script suite under `scripts/web_e2e/`
(new dir; `scripts/m9_e2e/` is untouched), following the proven CDP patterns in
`scripts/m9_e2e/cdp_screenshot.js` and `scripts/cdp_click_hw.js`: raw WebSocket
CDP (node `--experimental-websocket`, node at `C:/Users/felix/tools/node/node.exe`),
tab per scenario via `PUT /json/new`, `Input.dispatchMouseEvent`/
`Input.dispatchKeyEvent`/`Input.insertText` for actions,
`Runtime.evaluate` + the `window.__azProbe` wasm-export hook for assertions,
`Page.captureScreenshot` for checkpoints.

Phases: **P1** driver + DOM/export assertions on existing mouse scenarios
(`hello_world_counter.json` is the gate — `scripts/cdp_click_hw.js` already
proves the underlying path end-to-end). **P2** keyboard/text-entry scenarios
(keyboard listeners are already wired in the web loader — see §3.1 — the work is
the key-name mapping and focus semantics, not new loader plumbing). **P3**
desktop-vs-web screenshot comparison with masks + perceptual tolerance.
**P4** CI matrix.

---

## 2. Existing-format inventory (verified)

### 2.1 `AZ_E2E_TEST` — headless deterministic scenario runner

Env var `AZ_E2E_TEST=<file>` (e2e_test.rs:117-123), dispatched from `run.rs`
startup (run.rs:304, 1136-1138); takes over `main()`, constructs a
`HeadlessWindow`, and exits 0/1 (e2e_test.rs:133-322). Gated behind cargo
feature `e2e-test` (e2e_test.rs:15).

Full JSON schema (e2e_test.rs:39-111):

```jsonc
{
  "name": "string",                    // required
  "warmup_ticks": 0,                   // u32, default 0
  "steps": [                           // tag = "action", snake_case
    { "action": "resize",      "width": 800.0, "height": 600.0 },  // incremental_relayout
    { "action": "resize_full", "width": 800.0, "height": 600.0 },  // + regenerate_layout
    { "action": "tick" },                                          // regenerate_layout
    { "action": "sleep_ms",    "ms": 16 }
  ],
  "loop": { "iterations": 1000, "steps_range": [0, 4] },  // optional; half-open slice
  "rss_probes": {                       // optional
    "every_n_iterations": 100, "warmup_skip": 0,
    "assert_growth_mib_max": 10.0, "assert_absolute_mib_max": 200.0,
    "memory_breakdown": false
  },
  "output": { "jsonl_path": "-", "summary_path": "-" }    // "-" = stderr
}
```

Emits JSONL `baseline`/`probe`/`mem`/`summary` events; pass/fail is purely RSS
ceilings (e2e_test.rs:296-321). **No input events, no DOM assertions** — not a
candidate for cross-backend behaviour testing.

### 2.2 `AZ_E2E` — debug-server-dispatched assertion scenarios

Transport has two entry points into the same queue:

1. **Batch runner**: `AZ_E2E=<file>` (run.rs:52-57). `setup_e2e_runner`
   (run.rs:71-125) parses the file as `Vec<E2eTest>` *or* a single `E2eTest`,
   pushes one `DebugEvent::RunE2eTests` onto the spmc debug queue via
   `queue_e2e_tests` (full.rs:2529-2569), and a waiter thread prints
   cargo-test-style output and exits 0/1 (600 s timeout, run.rs:107).
   Combine with `AZ_BACKEND=headless` for CI (run.rs:6-27). This is what
   `scripts/e2e_language_matrix.sh` gates on (`test result: ok`).
2. **Interactive HTTP**: `AZ_DEBUG=<port>` starts a plain HTTP server
   (TcpListener, full.rs:2596-2609); each `POST /` body is one JSON
   `DebugEvent` — `{"op": "click", ...}` — answered with a JSON response.
   The `tests/e2e/*.sh` runners drive it with `curl` (e.g. focus.sh:156,
   170-177). `RunE2eTests` itself is also a regular op (full.rs:1867-1878).

Steps execute on the app's event-loop timer with a continuation mechanism that
**yields for relayout between steps** (`resume_e2e_continuation`,
full.rs:3941-3962) — this is why `hello_world_counter.json`'s description says
asserts "see the refreshed DOM".

**Test container schema** (full.rs:3218-3272):

```jsonc
[
  {
    "name": "string",                          // required
    "description": "string?",
    "config": { "continue_on_failure": false, "delay_between_steps_ms": 0 },
    "setup": {                                 // optional
      "window_width": 800, "window_height": 600, "dpi": 96,   // defaults, full.rs:3250-3254
      "app_state": { }                         // set_app_state before first step
    },
    "steps": [
      { "op": "<op-name>", "screenshot": false, /* flattened params */ }
    ]
  }
]
```

`E2eStep.screenshot: bool` captures a screenshot after the step into the
result (full.rs:3266-3268, 3300-3302). Results are structured
`E2eTestResult`/`E2eStepResult` JSON (full.rs:3277-3307).

**Action/query op vocabulary** — `DebugEvent`, `#[serde(tag = "op",
rename_all = "snake_case")]` (full.rs:1523-2100). The subset relevant here:

- Mouse: `mouse_move{x,y}`, `mouse_down{x,y,button}`, `mouse_up{x,y,button}`,
  `click{x?,y?,selector?,node_id?,text?,button}` (four addressing modes,
  full.rs:1544-1562), `double_click{x,y}`, `scroll{x,y,delta_x,delta_y}`
- Keyboard: `key_down{key,modifiers}`, `key_up{key,modifiers}`,
  `text_input{text}` (full.rs:1577-1589). `modifiers` =
  `{shift,ctrl,alt,meta}` bools (full.rs:2342-2350); `key` strings parsed
  case-insensitively by `parse_virtual_keycode` (full.rs:4402-4437+): letters,
  digits, `"tab"`, `"return"`, `"space"`, `"escape"`, `"back"`, `"lshift"`,
  `"lcontrol"`, arrows `"left"`/`"right"`/..., etc.
- Touch/pen/gesture: `touch_start/move/end/cancel`, `pen_down/move/up`,
  `swipe`, `pinch`, `rotate`, `long_press` (full.rs:1594-1675)
- Window: `resize{width,height}`, `move{x,y}`, `focus`, `blur`, `close`,
  `dpi_changed{dpi}` (full.rs:1678-1691)
- Scrolling: `scroll_node_by{selector|node_id|text, delta_x, delta_y}`,
  `scroll_node_to{...,x,y}`, `scroll_into_view{...,block,inline,behavior}`
  (full.rs:1741-1780)
- Timing: `wait{ms}`, `wait_frame`, `relayout`, `redraw` (full.rs:1834-1841)
- Queries (return data, useful as diagnostics in results): `get_state`,
  `get_dom`, `hit_test{x,y}`, `get_html_string`, `get_node_css_properties`,
  `get_node_layout`, `get_all_nodes_layout`, `get_dom_tree`,
  `get_display_list`, `get_scroll_states`, `find_node_by_text{text}`,
  `get_scrollbar_info`, `get_selection_state`, `get_focus_state`
  (→ `FocusStateResponse` with selector + text_content, full.rs:1463-1490),
  `get_cursor_state` (full.rs:1492-1519), `get_app_state`, `set_app_state`,
  `take_screenshot`, `take_native_screenshot` (full.rs:1694-1865)
- DOM mutation (debug-only, not portable): `insert_node`, `delete_node`,
  `set_node_text`, `set_node_classes`, `set_node_css_override`
  (full.rs:1882-1927), plus the component-registry/preview ops (full.rs:1934+)

**Assertion ops** (evaluated against live DOM state, full.rs:3340-3400):

| op | params | desktop implementation |
|---|---|---|
| `assert_text` | `selector`, `expected` | node text via `get_node_text_content`, fallback `NodeType::Text` (full.rs:3407-3461) |
| `assert_exists` / `assert_not_exists` | `selector` | selector match count (full.rs:3465-3508) |
| `assert_node_count` | `selector`, `expected:number` | (full.rs:3512-3539) |
| `assert_layout` | `selector`, `property` (x/y/width/height), `expected`, `tolerance?` (default 0.5) | `callback_info.get_node_rect` (full.rs:3544-3604) |
| `assert_css` | `selector`, `property`, `expected` | computed property, compared as Rust `Debug` string (full.rs:3650-3652 — note: **not** browser CSS syntax) |
| `assert_app_state` | `path`, `expected` | RefAny JSON via serialize_fn (full.rs:3379) |
| `assert_scroll` | `selector`, `x?`, `y?`, `tolerance?` | (full.rs:3380) |
| `assert_screenshot` | `reference`, `threshold?` (default 2), `max_diff_ratio?` (default 0.0), `save_actual?` | CPU-render PNG via `callback_info.take_screenshot`, `cpurender::pixel_diff`; **auto-saves baseline when reference is missing** (full.rs:3833-3931, baseline at 3878-3888); requires `cpurender` feature |

### 2.3 Scenario corpus + existing runners

- `tests/e2e/*.json` — all `AZ_E2E` format (op-based). Ops actually used in the
  corpus: `get_state`, `get_dom_tree`, `get_focus_state`, `get_selection_state`,
  `get_cursor_state`, `find_node_by_text`, `click` (by x/y and by text),
  `mouse_down/move/up`, `key_down/up` (incl. `LShift`, `LControl`, `Back`,
  `Tab`, `Left`, `A`), `text_input`, `scroll_node_by` (selector
  `[contenteditable]`), `resize`, `wait`, `assert_text`, `assert_screenshot`.
- `tests/e2e/*.c` + `tests/e2e/*.sh` — C test apps compiled against
  `target/release/azul.dll` + curl-driven interactive runners against
  `AZ_DEBUG` (focus.sh:98-177 shows the whole pattern: build, launch with
  `AZ_DEBUG=8765`, poll `POST {"op":"get_state"}`, drive, `take_native_screenshot`
  base64 → PNG at focus.sh:226-231, cleanup via `{"op":"close"}`).
- `scripts/e2e_language_matrix.sh` — runs `AZ_E2E=tests/e2e/hello_world_counter.json`
  + `AZ_BACKEND=headless` across all 26 language bindings; WORKS iff
  `test result: ok` and exit 0 (lines 12-24). **This makes `AZ_E2E` the de facto
  cross-implementation conformance format already** — the web backend becomes
  "the 27th column" of the same matrix.
- `scripts/test_all_e2e.sh` — macOS binding matrix using an `AZ_DEBUG` counter
  probe (5 → 8 after three clicks) per language (lines 2-9, 34-54). Its
  start/probe/teardown/report shape is the model for the web lane runner (§6).
- `layout/tests/e2e_pixel_diff.rs` — in-process CPU-render pixel-diff precedent:
  baseline bootstrap on first run, `_actual.png` saved on failure, per-test
  thresholds 0-3 (lines 62-129; thresholds at 140, 172, 200).
- `layout/tests/contenteditable_e2e.rs` — in-process text-input pipeline tests
  with a private `pixel_diff_count` (RGB-only, lines 56-71) and screenshots to
  `layout/test_output/contenteditable_e2e/`.
- `layout/src/cpurender/pixmap.rs` — the shared diff engine: per-channel RGBA
  delta; a pixel "differs" if any channel delta > `threshold`; result carries
  `diff_count/total_pixels/max_delta`, `diff_ratio()` (lines 424-513).
- `doc/aztest` — **not** a JSON harness: `azinput`, a KWin-Wayland fake-input
  injector (real compositor-level pointer/keyboard, Linux-only;
  doc/aztest/src/main.rs:1-19). Irrelevant to the web lane; noted to avoid
  confusion.

### 2.4 Substrate decision

`AZ_E2E` (`E2eTest`) — for the reasons in §1. The web driver defines a
**portability profile** over it:

- **Portable ops (P1)**: `click` (selector/text/x,y), `mouse_down/up/move`,
  `double_click`, `scroll`, `scroll_node_by`, `resize`, `wait`, `wait_frame`,
  plus queries `get_focus_state`/`get_selection_state`-as-diagnostics.
- **Portable ops (P2)**: `key_down/up`, `text_input`, `scroll_into_view`.
- **Portable assertions**: `assert_text`, `assert_exists`, `assert_not_exists`,
  `assert_node_count`, `assert_layout`, `assert_scroll`, `assert_screenshot`
  (P3 for cross-backend; web-local baseline before that).
- **Non-portable (web driver reports SKIP, not FAIL)**: touch/pen/gesture ops
  (loader has no touch listeners yet), `assert_app_state` / `get_app_state`
  (no wasm-side state serializer export today — see §3.2), DOM-mutation ops,
  component-registry ops, `take_native_screenshot`.
- **`"x-web"` step extension** (ignored by desktop — lands in flattened
  `params`): `{"x-web": {"skip": true | "selector": "...", "mask": [[x,y,w,h]],
  "settle_ms": 500, "threshold": 24, "max_diff_ratio": 0.02}}` for per-step
  web-side overrides without forking scenario files.

---

## 3. Gap analysis: desktop vs web

### 3.1 Event dispatch — what the web runtime already handles

The web backend (`AZ_BACKEND=web://ip:port`, dll/src/web/config.rs:96-105,
server "Listening on" at dll/src/web/mod.rs:1276) serves pre-rendered HTML in
which every azul node is a real DOM element `id="az_N"` (DFS-order counter,
html_render.rs:333, 384) with user classes passed through
(html_render.rs:370-373, 386), user `id` remapped to `data-az-id`
(html_render.rs:366-369!), boolean attributes like `contenteditable` emitted
verbatim (html_render.rs:374-375), per-node CSS emitted as `#az_N { ... }`
rules from azul's computed-property cache (html_render.rs:492-515), and
callback nodes carrying `data-az-cb`/`data-az-ev` (html_render.rs:429-437).

`loader.js` (generated from dll/src/web/loader_js.rs) wires **all of these**
document/window listeners (loader_js.rs:984-1049):
`click`, `mousedown`, `mouseup`, `dblclick`, **`keydown`, `keyup`**
(target-routed, :991-992), `focusin`, `focusout`, `mousemove`, `wheel`,
`mouseenter`, `mouseleave`, `contextmenu`, **`input`** (text entry →
`azDispatchWithText`, :1037-1041), window `scroll` and `resize` (:1044-1049;
resize also re-runs `AzStartup_solveLayout`, :1122-1124).

> **Correction to the task brief**: keyboard events ARE already wired in
> loader_js.rs. The Phase-2 prerequisite is not loader wiring but (a) the
> CDP-side key synthesis producing real `keydown`/`input` events on a focused
> contenteditable element, and (b) key-code semantics matching desktop
> (`azDispatch` packs `domEvent.keyCode` — legacy numeric — at offset 12,
> loader_js.rs:823; desktop parses names into `VirtualKeyCode`, full.rs:4404).

Each listener packs a 24+-byte event buffer (node_idx | x | y |
button-or-keyCode | modifier bits, ints only — loader_js.rs:802-852) and calls
the lifted `AzStartup_dispatchEvent` with a kind code
(loader_js.rs:43-58): click=0, mousedown=1, mouseup=2, mousemove=3, dblclick=4,
wheel=5, keydown=6, keyup=7, focusin=8, focusout=9, resize=10, scroll=11,
mouseenter=12, mouseleave=13, contextmenu=14. Returned TLV patches (SetText,
SetAttr, SetInlineStyle, Remove/InsertNode, Focus, ScrollTo, Add/RemoveClass…)
are applied to the live DOM (loader_js.rs:854-972) — so **DOM assertions
observe callback effects directly**.

### 3.2 Assertion surface available from JS/CDP

- **DOM**: `Runtime.evaluate` — `document.querySelector`, `textContent`,
  `getBoundingClientRect`, `getComputedStyle`, `document.activeElement`,
  `scrollTop/Left`. Covers assert_text/exists/node_count/layout/scroll and
  focus checks.
- **wasm exports**: the loader publishes `window.__azProbe = {mini, state,
  memory, table, ...}` (loader_js.rs:633-644), so `Runtime.evaluate` can call
  the lifted diagnostics directly:
  `AzStartup_isLayoutSolved` (eventloop.rs:1749), `AzStartup_getPositionedRectsLen`
  (:1760), `AzStartup_getPositionedRectsPtr` (4×u32 = 16 bytes/node, :1768-1777,
  readable via `new DataView(__azProbe.memory.buffer)`), `AzStartup_hitTest`
  (:892), `AzStartup_isStyledDomHydrated` (:1226), `AzStartup_getDomNodeCount`
  (:1239), `AzStartup_peekU` (:1944).
- **console**: `Runtime.consoleAPICalled` / `Runtime.exceptionThrown` streams
  (pattern: cdp_screenshot.js:28-39). The loader logs one line per dispatch
  (`[azul-web] dispatch kind=…`, loader_js.rs:843-844) — usable both as a
  settle signal and as a hard-fail trigger (any `RuntimeError` = lift bug).
- **Screenshots**: `Page.captureScreenshot` + `Emulation.setDeviceMetricsOverride`
  (cdp_screenshot.js:47-64).

### 3.3 Gap table

| Capability | Desktop (AZ_E2E) | Web today | Gap / plan |
|---|---|---|---|
| click / mouse / dblclick / wheel | full.rs:1528-1574 | listeners wired; **proven end-to-end** by scripts/cdp_click_hw.js (counter increments through lifted wasm) | none — P1 |
| resize | `resize` op | `Emulation.setDeviceMetricsOverride` → window resize listener → dispatch + re-solve (loader_js.rs:1047-1049, 1104-1125) | verify override fires `resize` in headless (it does when `mobile:false` changes metrics); else `window.dispatchEvent(new Event('resize'))` fallback — P1 |
| key_down / key_up | VirtualKeyCode names (full.rs:4404) | `keydown`/`keyup` wired (loader_js.rs:991-992), payload = numeric `keyCode` | key-name → CDP `Input.dispatchKeyEvent` map (§4.4) — P2 |
| text_input | `TextInput{text}` op | `input` listener → `azDispatchWithText` sends **whole current value** (loader_js.rs:1037-1041, 1058-1082) | CDP `Input.insertText` on a focused `[contenteditable]` element produces `input` natively — P2. Semantics differ (desktop = per-keystroke commit; web = whole-value) — assert on **outcome** (DOM text), not on event counts |
| focus semantics | wasm-side focus mgr; `get_focus_state` returns selector + contenteditable + text (full.rs:1477-1490) | `focusin/out` forwarded (loader_js.rs:996-997); Focus patch calls `el.focus()` (:940-943); Tab-navigation logic lives wasm-side | web assert via `document.activeElement.id` (`az_N`); Tab-key parity depends on lifted focus manager — P2 test target |
| scroll_node_by | selector-addressed (full.rs:1741-1750) | element `scrollTo` patch (:945-951) + wheel listener; driver can also `el.scrollBy()` + dispatch wheel at element center | map to `Input.dispatchMouseEvent{type:mouseWheel}` at node center — P1/P2 |
| assert_layout | azul solver rects (`get_node_rect`) | browser `getBoundingClientRect` (what the user sees — the browser lays out the emitted CSS); wasm rects via `getPositionedRects*` **currently gated**: `solveLayoutReal` traps (garbage-pointer deref) and hydrate/solve is wrapped in try/catch behind the `false &&`-style gate (loader_js.rs:571-624) | default to gBCR; optional cross-check vs wasm rects once the LIVE lift blocker is fixed |
| assert_css | Rust `Debug` string of computed prop | `getComputedStyle` (browser syntax) — but page CSS is *generated from azul's computed cache* (html_render.rs:492-515) | value-format translation table for the few props the corpus uses; mark non-portable until then |
| assert_app_state / get_app_state | RefAny serialize_fn | **no wasm export** to serialize state back out (eventloop.rs export list has no serializer) | P2+: propose optional `AzStartup_serializeState` export; until then SKIP on web |
| assert_screenshot | CPU-render PNG, deterministic, auto-baseline (full.rs:3854, 3878-3888) | `Page.captureScreenshot` — browser raster (Skia, ClearType AA, its own scrollbars) | P1: web-local baselines; P3: cross-backend compare with masks + perceptual tolerance (§5) |
| selection / cursor state | `get_selection_state`, `get_cursor_state` | wasm-internal; no export; browser `window.getSelection()` reflects browser selection only | diagnostics-only on web for now |
| touch / pen / gestures | full.rs:1594-1675 | loader has **no** touch listeners ("Skipping touch/drag/composition", loader_js.rs:981) | out of scope; SKIP |
| headless screenshot for references | YES — `AZ_BACKEND=headless` + `assert_screenshot`/`take_screenshot` (cpurender) and `take_native_screenshot` (base64, focus.sh:226-231) | n/a | desktop reference lane needs **no new code** (§5.1) |

---

## 4. Mini-puppeteer design

### 4.1 Architecture

New directory `scripts/web_e2e/` (leaves `scripts/m9_e2e/` untouched):

```
scripts/web_e2e/
  run_scenario.js      # CLI entry: one scenario file → exit 0/1
  lib/cdp.js           # ~120 LOC: fetch /json/new, WebSocket send/recv,
                       #   event subscription (pattern = cdp_screenshot.js:16-46)
  lib/edge.js          # launch/attach: reuse a running :9222, else spawn
                       #   msedge --headless=new --remote-debugging-port=<port>
                       #   --user-data-dir=<scratch> about:blank
                       #   (pattern: HANDOFF_FABLE_web_lift_x86_windows_2026_06_13.md:81)
  lib/driver.js        # step interpreter: op → CDP (table §4.3), settle logic
  lib/asserts.js       # assertion evaluators (table §4.5)
  lib/keymap.js        # azul VirtualKeyCode names → CDP key events (§4.4)
  lib/png.js           # P3: minimal PNG decode/encode (zlib is in node core;
                       #   ~200 LOC, no npm deps) + pixel_diff port with the
                       #   exact semantics of cpurender/pixmap.rs:465-513
  run_all.sh           # web lane runner (§6)
```

Invocation:

```
C:/Users/felix/tools/node/node.exe --experimental-websocket \
  scripts/web_e2e/run_scenario.js \
  --scenario tests/e2e/hello_world_counter.json \
  --url http://127.0.0.1:8802/ \
  --cdp http://127.0.0.1:9222 \
  --artifacts target/web_e2e/hello_world_counter \
  [--screenshot-mode capture-only|web-baseline|cross-backend] \
  [--filter <test-name-regex>]
```

Lifecycle per scenario file: create tab (`PUT /json/new?about:blank`) →
`Runtime.enable`, `Page.enable`, `Log.enable` → apply `setup`
(`Emulation.setDeviceMetricsOverride{width,height,deviceScaleFactor:dpi/96,mobile:false}`,
full.rs:3238-3254 defaults 800×600@96) → `Page.navigate` to the app URL →
**boot settle**: wait for the loader's `bootstrap complete` console line
(loader_js.rs:649) or `window.__azProbe` to exist, cap 30 s (wasm shard fetch +
instantiation takes seconds — cdp_click_hw.js:28 uses a blunt 6 s sleep; we wait
on the explicit signal instead) → run each `E2eTest`'s steps → emit
`E2eTestResult`-shaped JSON (full.rs:3277-3307 — same shape as desktop, so one
report format) → close tab (`/json/close/<id>`, cdp_click_hw.js:54).

Console lines and exceptions are captured for the whole tab lifetime and written
to the artifact dir; any `Runtime.exceptionThrown` containing `RuntimeError`
(wasm trap) marks the running step failed unless `"x-web": {"allow_trap": true}`.

### 4.2 Node addressing / selector translation

Desktop selectors run against azul's styled DOM; web selectors run against the
emitted HTML mirror. Translation rules (all verified against html_render.rs):

| scenario addressing | web resolution |
|---|---|
| `"selector": "#foo"` (user id) | `[data-az-id="foo"]` (html_render.rs:366-369) |
| `"selector": ".btn"`, tag, `[contenteditable]`, `"body > div"` | as-is via `document.querySelector` (classes/bool attrs pass through, :370-375; body/root structure mirrors the azul DOM) |
| `"text": "Increase counter"` | JS scan: deepest element whose trimmed `textContent` matches, click its center via gBCR (pattern proven in cdp_click_hw.js:39-48) |
| `"node_id": N` | `#az_N` — **best effort**: `az_N` is a DFS emission counter (html_render.rs:333), which matches azul `NodeId` indices for the single root DOM but is not contractually identical; prefer selector/text in portable scenarios, warn when `node_id` is used |

### 4.3 Step → CDP mapping table

`(cx, cy)` = center of resolved node's `getBoundingClientRect`, else the given
`x,y` (CSS px; deviceScaleFactor handles DPI).

| op | CDP realization |
|---|---|
| `mouse_move{x,y}` | `Input.dispatchMouseEvent{type:"mouseMoved",x,y}` |
| `mouse_down{x,y,button}` | `Input.dispatchMouseEvent{type:"mousePressed",x,y,button,buttons,clickCount:1}` |
| `mouse_up{x,y,button}` | `…{type:"mouseReleased",…}` |
| `click{…}` | resolve target → `mouseMoved` → `mousePressed` → `mouseReleased` (clickCount 1) — exact pattern of cdp_click_hw.js:45-47; browser synthesizes `click` which the loader forwards as kind 0 |
| `double_click{x,y}` | press/release ×2 with `clickCount:2` (browser emits `dblclick` → kind 4) |
| `scroll{x,y,delta_x,delta_y}` | `Input.dispatchMouseEvent{type:"mouseWheel",x,y,deltaX,deltaY}` (loader wheel listener → kind 5) |
| `scroll_node_by{selector,…}` | resolve node → `mouseWheel` at its center; fallback `Runtime.evaluate el.scrollBy(dx,dy)` (loader window-scroll listener at :1044 only covers page scroll — element-level scroll parity is a P2 verification item) |
| `scroll_into_view{…}` | `Runtime.evaluate el.scrollIntoView({block,inline,behavior})` |
| `key_down{key,modifiers}` | `Input.dispatchKeyEvent{type:"keyDown"}` with fields from keymap (§4.4); modifiers bitmask (Alt=1,Ctrl=2,Meta=4,Shift=8) |
| `key_up{key}` | `…{type:"keyUp"}` |
| `text_input{text}` | ensure focus (previous Tab/click steps; else `el.focus()` on `document.activeElement` check) → `Input.insertText{text}` — generates the `input` event → `azDispatchWithText` (loader_js.rs:1037-1041). Per-char alternative (`keyDown`+`char`+`keyUp`) behind `"x-web":{"per_key":true}` for cursor-sensitive tests |
| `resize{width,height}` | `Emulation.setDeviceMetricsOverride{width,height,…}` → loader resize path (:1047-1049); verify with `Runtime.evaluate innerWidth/innerHeight`; fallback: dispatch synthetic `resize` event |
| `wait{ms}` | plain sleep |
| `wait_frame` | `Runtime.evaluate awaitPromise: new Promise(r=>requestAnimationFrame(()=>requestAnimationFrame(r)))` (double-rAF = one presented frame) |
| `relayout` / `redraw` | `Runtime.evaluate __azProbe.mini.AzStartup_solveLayout(__azProbe.state, innerWidth, innerHeight)` / no-op |
| `focus` / `blur` (window) | `Emulation.setFocusEmulationEnabled` / no-op (headless has no real window focus) — diagnostics-only |
| `get_*` queries | evaluated best-effort (DOM/`__azProbe`), stored in step result `response`, never fail the step |
| touch/pen/gesture ops, DOM-mutation ops | `SKIP` (recorded in result) |

**Settle strategy** after every input step (§7 risk 2). Two facts make this
deterministic rather than heuristic:

- azul's dispatch→patch path is **synchronous JS** (DOM listener → wasm
  `AzStartup_dispatchEvent` → TLV patches applied before the listener
  returns), so once the input event has been dispatched, the DOM mutation is
  already done — a double-rAF then guarantees one presented frame. rAF *is*
  the frame-complete signal for the synchronous case.
- the only work rAF cannot see is **promise-based**: a per-callback shard
  fetch, a `WebAssembly.instantiate`, and (future) async boundary-API calls.
  We own loader.js, so instead of network-idle heuristics the runtime should
  *report* this: export a pending-work counter (`window.__az_pending`,
  incremented on dispatch entry / fetch start, decremented after
  patches-applied / promise resolution — a ~10-line loader_js.rs addition).

Settle = `await __az_pending == 0` (cap 2 s) → double-rAF → honor authored
`wait` steps unchanged. Until the counter lands in loader.js, fall back to
counting `[azul-web] dispatch kind=` console lines (loader logs each dispatch,
loader_js.rs:843-844) with a 250 ms cap before the double-rAF.

### 4.4 Key-name mapping (azul VirtualKeyCode → CDP)

Desktop scenarios use `parse_virtual_keycode` names (full.rs:4404+). The keymap
translates to CDP `Input.dispatchKeyEvent` fields (`key`, `code`,
`windowsVirtualKeyCode`, `text` for printable chars):

| scenario `key` | CDP `key` / `code` / VK |
|---|---|
| `Tab` | `Tab` / `Tab` / 9 |
| `Return` | `Enter` / `Enter` / 13 (+ `text:"\r"`) |
| `Space` | `" "` / `Space` / 32 (+ `text:" "`) |
| `Escape` | `Escape` / `Escape` / 27 |
| `Back` | `Backspace` / `Backspace` / 8 |
| `Left`/`Right`/`Up`/`Down` | `ArrowLeft`… / `ArrowLeft`… / 37-40 |
| `LShift`/`RShift` | `Shift` / `ShiftLeft`|`ShiftRight` / 16 — also OR into `modifiers` for subsequent events until key_up (matches desktop scenarios that bracket with key_down/key_up, contenteditable_overflow_test.json:135-141) |
| `LControl` | `Control` / `ControlLeft` / 17 (same bracketing) |
| `A`-`Z`, `0`-`9` | letter / `KeyA`… / 65+ (+ `text` when no ctrl/alt) |

The driver tracks held modifier keys itself (desktop scenarios express
Shift+Left as explicit `key_down LShift … key_up LShift`), passing the
accumulated `modifiers` bitmask on every intervening event.

### 4.5 Assertion mapping table

| op | web evaluation |
|---|---|
| `assert_text` | `Runtime.evaluate`: resolve selector (§4.2) → normalized `textContent.trim()` === expected |
| `assert_exists` / `assert_not_exists` / `assert_node_count` | `document.querySelectorAll(sel').length` |
| `assert_layout` | `getBoundingClientRect()` property vs expected ± tolerance (default 0.5 as desktop). Optional `"x-web":{"source":"wasm"}` → read rect from `getPositionedRectsPtr` memory once the solve gate lifts |
| `assert_scroll` | `el.scrollLeft/scrollTop` ± tolerance |
| `assert_css` | `getComputedStyle(el)[prop]` with a per-property value normalizer (px numbers, colors → rgb()); properties without a normalizer → SKIP with warning (desktop compares Rust `Debug` strings, full.rs:3652 — direct string equality can never match) |
| `assert_app_state` | SKIP (no export; §3.3) — counted separately in the report so scenarios stay honest |
| `assert_screenshot` | `Page.captureScreenshot{format:"png"}` → mode: `capture-only` (always pass, save PNG), `web-baseline` (compare/bootstrap against `layout/tests/reference_images_web/<same relative name>`, exact same auto-baseline behaviour as desktop full.rs:3878-3888), `cross-backend` (P3, §5) |
| console expectation (new, web-only, via `"x-web"` on any step): `{"expect_console": "regex"}` / global `"forbid_exceptions": true` (default) | matched against the captured stream |

Failure artifacts per step: `NN_<op>_actual.png`, `NN_expected.png`,
`NN_diff.png`, `console.log`, `result.json` under
`target/web_e2e/<scenario>/<test-name>/`.

### 4.6 Worked example — `tests/e2e/hello_world_counter.json` (P1 gate)

Desktop (left: verbatim scenario steps; runs via `AZ_E2E=… AZ_BACKEND=headless
./hello-world`) vs what the web driver does:

| # | scenario step | web driver actions |
|---|---|---|
| 0 | *(implicit setup)* | tab → `Emulation.setDeviceMetricsOverride{800,600,dsf:1}` → `Page.navigate http://127.0.0.1:8802/` → wait `bootstrap complete` console line (≤30 s) |
| 1 | `assert_text selector="body > div" expected="5"` | `Runtime.evaluate document.querySelector("body > div").textContent.trim()` — the emitted mirror keeps body>div structure (counter div is `#az_1`; cdp_click_hw.js:32 reads it the same way) → expect `"5"` |
| 2 | `click text="Increase counter"` | evaluate: find deepest element with text "Increase counter" → gBCR center → `Input.dispatchMouseEvent` mouseMoved/mousePressed/mouseReleased (left, clickCount 1). Browser fires click → loader kind 0 → lifted `AzStartup_dispatchEvent` → SetText TLV patch applied (loader_js.rs:826-848, 891-897) |
| 3 | `wait ms=300` | sleep 300 ms (plus dispatch-log settle) |
| 4 | `assert_text … expected="6"` | as step 1 → `"6"` |
| 5-8 | two more clicks + wait | same as 2-3 |
| 9 | `assert_text … expected="8"` | as step 1 → `"8"` |
| — | *(result)* | print `E2eTestResult` JSON + cargo-style line `test hello_world_counter_increment ... ok`; exit 0 |

Phase-2 preview, from `tests/e2e/contenteditable_overflow_test.json`:
`ce_focus_first_input` (`key_down Tab` → `Input.dispatchKeyEvent` Tab; assert
`document.activeElement.id` changed + matches `[contenteditable]`),
`ce_type_short_text` (`text_input "Hello"` → `Input.insertText`; assert node
text), `ce_mouse_drag_select` (`mouse_down 100,50` → `mouseMoved 300,50` →
`mouse_up` → selection is wasm-internal: web asserts via screenshot checkpoint +
`window.getSelection()` diagnostic only). Its `assert_screenshot` steps run in
`web-baseline` mode until P3.

---

## 5. Screenshot-comparison design

### 5.1 Desktop reference generation — nothing new needed

The desktop already produces deterministic headless screenshots: `AZ_E2E` +
`AZ_BACKEND=headless` with the `cpurender` feature renders via
`callback_info.take_screenshot` and auto-saves any missing `reference` PNG
(full.rs:3854, 3878-3888); `save_actual` gives the per-run copy. The corpus
already points at `layout/tests/reference_images/**` (e.g.
contenteditable_overflow_test.json:11). The "desktop reference lane" is just a
script that runs the scenario desktop-headless first with `save_actual`
redirected into the artifact dir — zero Rust changes. (`take_native_screenshot`
base64 also exists for non-headless runs, focus.sh:226-231.)

### 5.2 Comparing web pixels to desktop pixels

Two different rasterizers (azul cpurender vs Skia + ClearType), same logical
geometry, same embedded fallback font (served at `/az/fallback.ttf` and
registered wasm-side, loader_js.rs:530-546 — glyph *shapes and metrics* match;
*antialiasing* does not). Therefore a tiered strategy, reusing the exact
`pixel_diff` semantics (per-channel delta > threshold ⇒ pixel differs;
`diff_ratio ≤ max_diff_ratio` ⇒ pass; pixmap.rs:465-513) ported to
`lib/png.js`:

1. **Tier G (geometry, default P3)** — mask out text: build the mask
   automatically from the union of text-node rects (browser side:
   `getBoundingClientRect` of all text-bearing `#az_N`; desktop side: same
   nodes via `get_all_nodes_layout` in the reference run) plus manual
   `"x-web".mask` rects. Compare unmasked pixels with `threshold ≈ 3` (the
   corpus' AA tolerance) and `max_diff_ratio 0.5-2 %`. Catches layout shifts,
   missing boxes, wrong colors, clipping bugs.
2. **Tier T (text regions)** — inside the text mask compare structurally:
   4×4 box-downsampled luminance diff with a loose threshold (~24) and
   `max_diff_ratio ≈ 2-5 %`. Catches "wrong/absent/mispositioned text" while
   ignoring AA style. (Downsample-then-diff is cheap and dependency-free;
   SSIM can be added later if it proves too loose.)
3. **Dimension guard** — exact, as today (`dimensions_match`,
   pixel_diff dimension branch): device metrics must equal the scenario's
   `setup` size at dsf = dpi/96.
4. Per-step overrides via `"x-web"` (`threshold`, `max_diff_ratio`, `mask`) so
   noisy steps (blinking cursor! — `get_cursor_state.is_visible`,
   full.rs:1516) can widen tolerance or mask the caret column; cursor-blink
   masking is mandatory for the contenteditable suite.

Diff artifacts: heatmap PNG (red = differing px), written next to the pair.
Baseline policy mirrors e2e_pixel_diff.rs:77-87: missing web baseline ⇒ save +
pass with `[baseline]` note; CI treats new baselines as failures unless
`--allow-baseline`.

Known non-comparable surfaces (documented SKIP, mask, or scenario-level
exclusion): native scrollbar pixels (browser draws its own; azul draws its
own), IME/caret rendering, `:hover`-dependent AA on rounded borders.

---

## 6. Runner integration — the web lane

`scripts/web_e2e/run_all.sh` (shape follows `scripts/test_all_e2e.sh` +
`scripts/m9_e2e/cdp_gate.sh`):

1. **Build** (skippable via `AZ_SKIP_BUILD=1` when `target/...azul.dll` is
   fresh): `cargo build -p azul-dll --release --no-default-features --features
   "build-dll web web-transpiler"` … exactly the cdp_gate.sh:6-14 recipe
   (RUSTC_BOOTSTRAP=1, `-Z build-std`, target `x86_64-pc-windows-msvc`), copy
   dll+pdb next to the test app.
2. **Start app/server**: per scenario app (`examples/c/hello-world.exe`,
   contenteditable app, …) with `AZ_BACKEND=web://127.0.0.1:8802
   AZ_LIFT_CACHE=1 REMILL_LIFT_BIN=… AZ_MINI_MAX_DEPTH=16384
   AZ_CB_MAX_DEPTH=8192`; poll the server log for `Listening on`
   (dll/src/web/mod.rs:1276) — cold lift can take many minutes, warm cache
   seconds (cdp_gate.sh:20-26 polls up to ~67 min cold).
3. **Start/attach Edge**: reuse `:9222` if `GET /json/version` answers, else
   spawn `msedge --headless=new --remote-debugging-port=9222
   --user-data-dir=<scratch>\edge-prof --no-first-run about:blank`.
4. **Run**: manifest of `(scenario.json, app binary, phase)` rows; invoke
   `run_scenario.js` per row; collect per-scenario exit codes.
5. **Teardown**: close tabs, kill the app process it started (never a
   pre-existing one), leave Edge if it was pre-existing.
6. **Report**: PASS/FAIL/SKIP board + artifact dir path, exit 1 on any FAIL
   (test_all_e2e.sh:172-186 style).
7. **Dual-lane mode** (P3): run desktop-headless first (references), then web,
   then cross-compare — one combined report.

## 6.5 E2E mock protocol — deterministic OS results (maintainer direction)

The first boundary interceptions (file open/load for AzWriter) must be
DETERMINISTIC under e2e so CDP can verify that what the browser shows stays
in sync with what the wasm side says (selection, cursor movement, rendered
content). One contract, three parties:

1. **The page global** — `window.__az_e2e_mock`, absent in production. Shape:

   ```js
   window.__az_e2e_mock = {
     file_open:  { path: "e2e://docs/sample.txt" },          // dialog result
     file_read:  { "e2e://docs/sample.txt": { b64: "..." } }, // path → content
     // later: save_file, color_pick, http: { "<url>": {status, b64} }, …
   };
   ```

2. **The JS boundary impls** consult it FIRST: a mocked `open_file` request
   resolves immediately with the predefined path (no picker, no gesture
   requirement); a mocked read serves the canned bytes. Unmocked requests in
   an e2e context fail loudly (`status: "unmocked"`), never fall through to
   real pickers — a hanging modal is the worst e2e outcome. The resume path
   is the NORMAL one (`AzStartup_completeRequest`) so the mock exercises the
   entire §4.1 resumable machinery except the browser-API call itself —
   which is exactly the split the sync-verification tests need.

3. **The harness op** — `{"op": "mock", "set": {…}}` merges into the global
   via CDP `Runtime.evaluate` before subsequent steps; specs stay
   self-contained. Desktop lane: the SAME spec op maps to the desktop
   runner's env-based mock (`AZ_E2E_TEST` already forces deterministic
   biometric/keyring paths — extend that mechanism to dialogs), keeping one
   spec valid in both lanes.

Sequencing note: this protocol needs the dialog/file boundaries to be
JS-implemented (`BoundaryJsImport`, boundary plan §5.1-§5.2). Current gates
run legacy bundled mode ("manifest has no boundary shards"), where dialog
calls stub to honest-cancel — the first interception milestone is therefore
(a) classify the dialog/file-read Az fns as `BoundaryJsImport`, (b) emit
their JS trampolines with mock-first logic, (c) land the `mock` op here.

---

## 7. Phased plan (rough sizes)

**Phase 1 — scenario driver + DOM/export assertions (mouse-based)**
`cdp.js`, `edge.js`, `driver.js` (mouse/resize/wait ops), `asserts.js`
(text/exists/count/layout/scroll), settle logic, result JSON, `run_all.sh` with
`hello_world_counter.json` + a resize scenario. Screenshot `capture-only`.
Gate: hello_world counter 5→6→8 green in a real Edge headless.
Size: ~600-800 LOC JS + ~150 LOC sh; **2-4 days**. No Rust changes.

**Phase 2 — keyboard + contenteditable**
`keymap.js`, `insertText`, modifier tracking, focus assertions; port the
14-test contenteditable suite with `"x-web"` annotations; verify element-level
scroll (`scroll_node_by`) and the whole-value `input` semantics; decide on
`AzStartup_serializeState` export for `assert_app_state` (small eventloop.rs
addition, optional). Size: ~300 LOC JS (+~60 LOC Rust if the export lands);
**3-5 days**, dominated by semantics verification, not code.

**Phase 3 — screenshot comparison desktop-vs-web**
`png.js` (decode/encode/diff, port of pixmap.rs semantics), auto text-mask
builder, tiered compare, heatmap artifacts, desktop-reference lane in
`run_all.sh`, baseline policy. Size: ~500 LOC JS + ~100 LOC sh; **3-6 days**
including tolerance tuning on the scrolling/widgets suites.

**Phase 4 — full matrix in CI**
GitHub workflow job (Windows runner: Edge preinstalled; warm lift cache is the
schedule-limiting problem — cache `target/` lift artifacts keyed on dll hash),
scenario manifest covering hello_world + contenteditable + scrolling + widgets,
flake quarantine list, wire into `e2e_language_matrix.sh` as the "web" column.
**2-4 days** + soak time.

---

## 8. Open questions / risks

1. **The LIVE web-lift blocker gates wasm-side layout assertions.**
   `AzStartup_solveLayoutReal` still derefs a garbage pointer
   (deep x86 value-flow mis-lift), so hydrate/solve runs under try/catch and
   the positioned-rects cache may be empty (loader_js.rs:571-624). P1 therefore
   asserts on the **browser DOM only** (which is sufficient — patches mutate
   the real DOM) and treats `__azProbe` rect probes as optional. If the blocker
   persists, hit-testing for *coordinate-only* clicks falls back to the
   registered-cb node (loader_js.rs:588-589) — fine for single-button apps,
   wrong for multi-target apps ⇒ prefer selector/text-addressed clicks in
   portable scenarios.
2. **Timing determinism / settling.** Desktop's runner is synchronous with
   explicit relayout yields (full.rs:3941+). The browser's dispatch→patch path
   is ALSO synchronous (so double-rAF is a real frame-complete signal for it —
   see §4.3); only promise-based work (shard fetch, instantiation, future
   async boundary calls) is invisible to rAF. Mitigation: the runtime-owned
   `__az_pending` counter (§4.3) makes settling deterministic; the residual
   risk shrinks to "counter coverage is incomplete" — audit every `fetch`/
   `then` in loader_js.rs when wiring it, and keep the quarantine list for
   anything missed.
3. **Screenshot comparability.** Different rasterizers, ClearType vs azul AA,
   browser-drawn scrollbars, caret blink. Mitigated by §5's masks + tiered
   thresholds + identical fallback font; still expect per-scenario tolerance
   tuning and a few permanently masked regions. Exact (`threshold 0`)
   cross-backend equality is out of reach by design — the goal is
   "same geometry, same text, same colors", not byte-identical pixels.
4. **Addressing drift.** `az_N` DFS ids vs desktop `NodeId`, user `#id` →
   `data-az-id` remap (html_render.rs:366-369), desktop's own selector engine
   vs `querySelector`, `assert_css` Debug-format strings. Mitigated by the
   translation layer (§4.2/§4.5) and by lint-warning any scenario that uses
   `node_id` addressing or un-normalizable CSS assertions.
5. **Keyboard/text semantics.** Desktop `text_input` commits per call; web
   `input` events carry the whole current value (loader_js.rs:1040,
   1058-1082); IME composition, key auto-repeat, and `keyCode` (legacy numeric)
   vs `VirtualKeyCode` differences. Mitigated by asserting outcomes (DOM text,
   focus) rather than event traces, and per-key mode via `"x-web"` where cursor
   position matters.
6. **DPI.** Corpus assumes 96 dpi (dsf 1). `Emulation.setDeviceMetricsOverride`
   with `deviceScaleFactor: dpi/96` should extend this, but hi-dpi screenshot
   compare multiplies AA noise — keep P3 at dsf 1, treat `dpi_changed` as
   non-portable until needed.
7. **Runtime cost of the lift.** Cold lift of a scenario app takes minutes to
   an hour (cdp_gate.sh polls 400×10 s); the web lane is only practical with
   `AZ_LIFT_CACHE=1` and per-app cache reuse — CI must cache lift artifacts or
   pin a prebuilt app set.
8. **Open**: should `focus`/`blur` window ops map to
   `Emulation.setFocusEmulationEnabled` (affects `:focus` styling in headless)?
   Should the driver auto-insert a settle after `resize` (desktop re-solves
   synchronously, web re-solves via the resize listener + `solveLayout` call,
   loader_js.rs:1122-1124)? Both need one experiment each in P1.
