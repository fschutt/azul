---
slug: e2e-testing
title: End-to-End Testing
language: en
canonical_slug: e2e-testing
audience: external
maturity: wip
guide_order: 220
topic_only: false
short_desc: Driving an Azul app from a script for tests
prerequisites: [debugging]
tracked_files:
  - core/src/debug.rs
  - dll/src/desktop/logging.rs
  - dll/src/desktop/shell2/common/debug_server.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T00:00:00Z
---

# End-to-End Testing

## Introduction

*WIP.* Assertion vocabulary, JSON keys, and the in-browser test designer all work today; field names may shift as the assertion engine grows.

A test is a JSON object with a name, optional setup, and an ordered list of steps. A step is one debug command (`click`, `text_input`, `resize`, …) or one assertion (`assert_text`, `assert_layout`, …). The runner drives a real `App` instance and evaluates each assertion against the live DOM. Two ways to run:

- `AZ_E2E=<file> ./my_app` boots your application headlessly and runs the JSON file as a single batch. Prints cargo-test-style output, exits `0` if every test passes, `1` otherwise.
- `POST /e2e/run` from a script or the in-browser inspector queues the same payload against a running app.

## A first test

```json
{
  "name": "Button click increments counter",
  "setup": {
    "window_width": 800,
    "window_height": 600,
    "dpi": 96
  },
  "steps": [
    { "op": "wait_frame" },
    { "op": "click", "selector": "button" },
    { "op": "wait_frame" },
    { "op": "assert_text", "selector": "p", "expected": "6" },
    { "op": "take_screenshot" }
  ]
}
```

Run it:

```bash
AZ_BACKEND=headless AZ_E2E=test.json ./my_app
```

Output mirrors `cargo test`:

```text
running 1 test
test "Button click increments counter" ... ok

test result: ok. 1 passed; 0 failed; finished in 0.21s
```

A test file is either one test object or an array of them.

## File and step shape

```json
{
  "name": "string",
  "description": "string?",
  "config": {
    "continue_on_failure": false,
    "delay_between_steps_ms": 0
  },
  "setup": {
    "window_width":  800,
    "window_height": 600,
    "dpi":           96,
    "app_state":    { "any": "json" }
  },
  "steps": [
    { "op": "...", "screenshot": false, "<params>": "..." }
  ]
}
```

`config.continue_on_failure` keeps running steps after the first failure (still reports the test as failed). `config.delay_between_steps_ms` inserts a sleep, useful for visually inspecting a test that runs against a visible window. `setup.app_state` puts each test into a known state without restarting the process.

`screenshot: true` on any step captures the rendered window after the step and embeds the base64 PNG in that step's result.

## Step operations

Every `op` accepted by `POST /` (covered in [Debugging](debugging.md)) is a valid step. Plus the assertion ops below.

- `assert_text` (params: `selector`, `expected`). First-match text content equals `expected`.
- `assert_exists` (params: `selector`). At least one node matches.
- `assert_not_exists` (params: `selector`). No node matches.
- `assert_node_count` (params: `selector`, `expected` int). Exactly `expected` nodes match.
- `assert_layout` (params: `selector`, `property`, `expected`, `tolerance?`). `property` is `x`, `y`, `width`, or `height`. Default tolerance `0.5` px.
- `assert_css` (params: `selector`, `property`, `expected`). Computed CSS value equals `expected`.
- `assert_app_state` (params: `path`, `expected`). Dot-path against the JSON-serialised app data.
- `assert_scroll` (params: `selector`, `x?`, `y?`, `tolerance?`). Scroll position of the first match.
- `assert_screenshot` (params: `reference`, `threshold?`, `max_diff_ratio?`, `save_actual?`). Compares the current screenshot against a reference PNG.

Selector resolution accepts CSS selectors (`.btn`, `#counter`, `div > span`), explicit `node_id` integers, or a `text` substring match. Pick whichever is least brittle. `selector` is preferred because the inspector can build them by clicking nodes in the DOM tree.

## Step results

Each step returns:

```json
{
  "step_index": 1,
  "op": "click",
  "status": "pass",
  "duration_ms": 12,
  "logs": ["[Input] click at (200, 300)", "..."],
  "screenshot": "data:image/png;base64,...",
  "response": { "...command-specific data..." },
  "error": null
}
```

A test rolls steps up:

```json
{
  "name": "Button click increments counter",
  "status": "pass",
  "duration_ms": 156,
  "step_count": 5,
  "steps_passed": 5,
  "steps_failed": 0,
  "steps": [ ... ],
  "final_screenshot": "data:image/png;base64,..."
}
```

`final_screenshot` is the screenshot from the last step that requested one, or `null`.

## Driving with curl

The debug server's `POST /` accepts a `run_e2e_tests` op that bypasses the file-based runner:

```bash
curl -s -X POST http://127.0.0.1:8765/ \
     -H 'Content-Type: application/json' \
     -d @tests.json | jq '.data.value.results'
```

Default request timeout is 30 s; tests that take longer must either pass `"timeout_secs": 600` in the request body or use `AZ_E2E=` (which uses a 600 s timeout internally). The Hello World shell driver in [Debugging](debugging.md) shows the same wait-frame / click / read-back pattern that an E2E step performs internally. You can build a test scenario incrementally as a curl script, then crystallise it into a JSON file once it works.

## Continuation across relayout

Some steps (`resize`, `set_node_text`, `delete_node`) require a relayout pass to complete before the next step can read the resulting state. The runner detects these and yields back to the timer; the test resumes on the next tick. From the test author's perspective this is invisible: write `resize` followed by `assert_layout` and the runner handles the suspension.

This is why `AZ_E2E` requires the application to reach the event loop. The test cannot make progress while the timer is not running. With `AZ_BACKEND=headless` (or `AZUL_HEADLESS=1`) the event loop runs without an OS window, which is the standard CI configuration.

## CI integration

A typical workflow:

1. Build the application once: `cargo build --release`.
2. Run the test bundle headlessly: `AZ_BACKEND=headless AZ_E2E=tests/smoke.json ./target/release/my_app`.
3. Process exit code is the test verdict. Use it as the CI step's exit code.
4. For screenshot diffs, set `assert_screenshot` steps with a `reference` PNG path; commit the references to the repo and update them via a `BLESS=1` workflow when intentional UI changes land.

For per-PR feedback, every shell script in [`tests/e2e/`](https://github.com/fschutt/azul/tree/master/tests/e2e) is a `curl`-against-`AZ_DEBUG` driver that became a JSON test once it was stable.

The language-binding board ([`scripts/e2e_language_matrix.sh`](https://github.com/fschutt/azul/blob/master/scripts/e2e_language_matrix.sh)) runs the same counter scenario against every binding and prints a status table. On a `--gate-shipped` failure it now dumps the tail of each failed binding's build/run log **and** its `AZ_RECORD` trace, so CI logs explain the failure without a local repro.

## Language toolchain prerequisites

Each binding drives the prebuilt library (built with `--features build-dll,debug-server`, which compiles in the `AZ_E2E` runner) and needs its own toolchain to build and run the hello-world. Missing toolchains report `⊘ SKIP` (they do not gate); a present-but-broken toolchain reports `✗ FAILS`. The shipped tier:

| Binding | Toolchain | Notes / common gotchas |
|---|---|---|
| Rust   | `cargo`/`rustc` 1.88+ | The example links the prebuilt DLL with `--no-default-features --features link-dynamic` (not `link-static`, which rebuilds Azul without the `debug-server` E2E runner). On Windows the import lib must be reachable as both `azul.dll.lib` and `azul.lib`. |
| C      | `clang`/`gcc`; MinGW on Windows (links `azul.dll.lib`) | Include the generated `azul.h`. |
| C++    | `clang++`/`g++` with `-std=c++20` (also 03–23) | Include `azul20.hpp`. Wrapper types are move-only RAII; pass owning values with `std::move` — never copy a moved-from wrapper. |
| Python | CPython 3.10+ | Uses the `azul` extension module (a separate build), not FFI. |
| Node   | `node` + the `koffi` npm package | macOS strips `DYLD_*` from the hardened `node`, so `koffi` must find the lib by an explicit path or the lib must sit next to the script. |
| C#     | .NET SDK (`dotnet`) | P/Invoke; the native lib must be loadable next to the build output. |
| **Java**   | **JDK 17+** and **Maven (`mvn`)** + **JNA 5.14** | The build targets release 11, so a JDK older than 11 fails with `invalid target release: 11` — pin JDK 17 (e.g. `actions/setup-java@v4` with Temurin 17). On macOS the JVM needs `-XstartOnFirstThread`. |
| **Kotlin** | **JDK 17+**, `kotlinc` (or Gradle) + **JNA 5.14** | Same JDK requirement as Java; `kotlinc` compiles `Azul.kt` + `HelloWorld.kt` against the JNA jar. |
| **Ruby**   | `ruby` + the **`ffi` gem** | `require 'ffi'` fails with `LoadError` unless the gem is installed: `gem install ffi`. It is **not** preinstalled on CI runners. |
| **Lua**    | **LuaJIT** (the stock PUC `lua` has no FFI) | Needs an FFI-complete LuaJIT — an old LuaJIT (e.g. Ubuntu's `2.1.0-beta3`) raises `NYI: cannot call this C function (yet)` on struct-by-value calls; use a current LuaJIT build. |
| Go     | `go` toolchain + a C compiler (cgo) | — |

`AZ_LOG` is on by default (see [Debugging](debugging.md)), so a binding that builds but exits early will print the platform-layer trace on stderr, which the board captures.

## Recording a test from the inspector

The in-browser inspector at `http://localhost:<port>/` includes an E2E designer. Click "Record", interact with the running window normally, and the inspector captures each click, text input, scroll, and resize as a step. Click "Stop", review the steps, and either run them in place or export to JSON for `AZ_E2E`.

The recorder uses the same selector resolver as `assert_*`, so the captured steps are robust to layout shifts as long as your DOM has stable IDs and classes.

## Limitations

- A test runs against the *current* application instance — there is no per-test sandbox. Use `setup.app_state` to put the app in a known state before each test.
- `take_native_screenshot` returns the actual framebuffer of the running window. Pixel-identical comparison across platforms is unrealistic; use `assert_screenshot` with a `max_diff_ratio` tolerance, or pin the diff job to one platform in CI.
- The runner does not multi-thread tests. Each test runs to completion before the next starts. If you need parallel runs, spawn N processes on N ports.

## Coming Up Next

- [Debugging](debugging.md) — Debug overlays, the inspector, and structured logging
- [Headless Rendering](headless-rendering.md) — Running the pipeline without a window
- [Code Generation](code-generation.md) — How `azul-doc` regenerates bindings from `api.json`
