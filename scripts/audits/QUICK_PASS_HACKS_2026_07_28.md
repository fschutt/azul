# Quick-pass hack audit — azul @ master

Read-only static audit, 2026-07-28. Board tasks #10 / #11.

**Moving target.** `master` advanced three times during this audit
(`9d1e62ffc` → `f7ee53088` → `0a5c2ceba`) as other agents landed e2e and CI work. Every
finding below was **re-verified against the tree at the time it was written**, and the two
headline findings (§1 reftest, §2 integration tests) were re-confirmed at `0a5c2ceba`.
`.github/workflows/rust.yml`, `scripts/build_registry_mirrors.sh` and `layout/src/text3` were
being edited concurrently; the sparse-checkout / registry-mirror incident is already fixed
in-tree (`8c2278bc6`, `05e727e5d`) and is listed under SOLID, not as a finding.

**Nothing in this repo was modified.** The only file written is this report.

**Claims checked and rejected** (recorded so the coverage is honest — every finding below was
re-traced by me rather than taken on trust):
* *"`css/tests/css-parser.rs` has a test commented out with a 'not important to fix' note"* —
  **that file does not exist.** Dropped.
* *"1,543 integration tests run in NO CI job"* — my own first reading. Tracing
  `scripts/coverage.sh` showed they **do** run there; the finding was rewritten (§2) into the
  more precise and more damning version.
* *"`tests/e2e/test_export_code.sh` and `undo-redo.sh` are missing"* — an `ls | head` artefact
  on my side. Both exist and are tracked; all 14 CI-referenced scripts verified present.
* *"`layout/tests/contenteditable_e2e.rs` is silently skipped on unmet features"* — all five of
  its `required-features` are reachable from azul-layout's defaults. Not skipped.
* *"`assert_eq!(x, x)` appears 59 times"* — true, but 14+ sampled are all deliberate,
  `#[allow(clippy::eq_op)]`-annotated algebraic-property tests with correctness companions.
  Not a finding.

---

## Severity index

**Part A — CI / release pipeline**

| # | Severity | Finding | Where |
|---|---|---|---|
| 1 | **CRITICAL** | `azul-doc reftest` returns `Ok(())` no matter how many reftests fail — and `test_heavy` is a deploy gate | `doc/src/reftest/mod.rs:194` |
| 2 | **CRITICAL** | 1,452 integration tests gated only by `coverage` (release profile, no debug-assertions); 93 more run nowhere, excluded on a false comment | `rust.yml:761`, `scripts/coverage.sh:110` |
| 3 | **CRITICAL** | `--gate-shipped` cannot fail when a toolchain is absent — 16/16 shipped langs have a SKIP path, 15 setup steps are `continue-on-error` | `scripts/e2e_language_matrix.sh:2234` |
| 4 | **HIGH** | "BLOCKING" dep-justification gate passes vacuously on empty `crates.txt` | `scripts/check_dep_justifications.py:145` |
| 5 | **HIGH** | `supply_chain` (cargo-deny) discards its own exit code 8× via `\|\| true`, then is `continue-on-error` | `rust.yml:4204,4215` |
| 6 | **HIGH** | ASan gate accepts its own 30 s timeout as success — and its exit-env is a no-op on `headless`, so the timeout is the *normal* path | `rust.yml:977` |
| 7 | **HIGH** | 11 of 20 `deploy_pages.needs` entries are `continue-on-error: true` and cannot block | `rust.yml:3220` |
| 8 | **HIGH** | Double-drop runtime gate, ASan, Miri-on-dll, `c_compile_check`, export-code E2E are all outside `needs` | `rust.yml:3220` |
| 17 | **HIGH** | 4 of 12 Rust examples silently skipped by `cargo check --examples` (unmet `required-features`) | `rust.yml:1485`, `examples/rust/Cargo.toml:55,76,81,86` |
| C1 | **HIGH** | `dockery.yml` shows two green checks over a build its own Dockerfile says cannot succeed | `docker/Dockerfile:61`, `dockery.yml:61,122` |
| 18 | **MEDIUM** | `css_double_drop` (issue-#15 double-free repro) is compile-checked but never executed | `dll/examples/css_double_drop.rs`, `rust.yml:156` |
| 9 | **MEDIUM** | `build_binaries` cache key omits root `Cargo.toml`/`dll/Cargo.toml`/`api.json` — skips the very gates that guard them | `rust.yml:1244` |
| 10 | **MEDIUM** | Failed packager ⇒ channel silently absent ⇒ site still prints the dead `pip install --index-url` line | `rust.yml:3978` |
| 11 | **MEDIUM** | 0-byte placeholder binaries survive into the published release; all 48 downloads are `continue-on-error` | `rust.yml:3141,3276` |
| 12 | **MEDIUM** | `sanitizers`: timeout = "pass", build failure = a table row, TSan set `halt_on_error=0` so it can never fail | `rust.yml:4448` |
| 13 | **MEDIUM** | WASM `Instant::now()` gate greps only the fully-qualified path and accepts any `#[cfg` | `rust.yml:165` |
| 14 | **MEDIUM** | `dll/tests/leak_regression.rs` has never run in CI on any platform | `dll/tests/leak_regression.rs:24` |
| C2 | **MEDIUM** | `strip_staticlib.sh` skips a missing archive without setting `rc` — the bitcode assertion no-ops | `scripts/strip_staticlib.sh:86` |
| 15 | **LOW** | Miri gate is a name filter that exits 0 on zero matches | `rust.yml:812` |
| 16 | **LOW** | `build_binaries` symbol gates are `[ -f ] \|\| continue` loops — vacuous if the path moves | `rust.yml:1651,1691` |
| C3 | **LOW** | `docker/Dockerfile` prelift diagnostic can never fire (no `pipefail`) | `docker/Dockerfile:133` |
| C4 | **LOW** | 5 dev scripts print PASS without checking anything (none currently CI-wired) | `scripts/{run_memtest,test_all_examples,test_dom_inspection,test_export_code,test_cpp_examples}.sh` |

**Part B — E2E scenario runner** (already heavily hardened; these are the residue)

| # | Severity | Finding | Where |
|---|---|---|---|
| B1 | **HIGH** | `click`/`click_node` `send_ok(success:false)` on an unresolved target — the step loop reads only `Ok`, so it passes | `layout/src/e2e/full.rs:9721,11557` |
| B2 | **HIGH** | `set_app_state` reports success on all three failure paths | `layout/src/e2e/full.rs:12195,12214,12223` |
| B3 | **MEDIUM** | `assert_work_bounded` accepts zero bounds (the guard `assert_damage` has) | `layout/src/e2e/full.rs:5184` |
| B4 | **MEDIUM** | `assert_manager_invariants` has no "something was checked" guard | `layout/src/e2e/full.rs:6000` |
| B5 | **MEDIUM** | `assert_scroll` with neither `x` nor `y` never compares a position | `layout/src/e2e/full.rs:4243` |
| B6 | *suspected* | `assert_state_machines_idle` has no liveness precondition, unlike its two siblings | `layout/src/e2e/full.rs:5525` |

**Part D — Rust test suite**

| # | Severity | Finding | Where |
|---|---|---|---|
| D1 | **HIGH** | `azul-doc`'s entire test suite (≈180 tests) has never run — no `cargo test -p azul-doc` exists | `doc/src/**` |
| D2 | **HIGH** | 3 `DISABLED_*` files (43 tests) build into empty 0-test binaries that print `test result: ok` | `layout/Cargo.toml:321`, `layout/tests/test_hint_vs_freetype.rs:1` |
| D3 | **MEDIUM** | `tests/src/layout.rs` — 37 tests orphaned by a missing `mod` in `lib.rs` | `tests/src/lib.rs` |
| D4 | **MEDIUM** | CI-live test whose only assertion is a tautology, in a file that re-implements the function under test | `dll/tests/kitchen_sink_integration.rs:20,103` |
| D5 | **MEDIUM** | 49 `dll/` tests (27%) never compile — `web`/`pdf`/`map-tiles`/`video-native`/Windows/macOS gates | `dll/src/**` |
| D6 | **MEDIUM** | 10 `layout/tests/` files (≈185 tests) never call real azul code — incl. 8 regression tests for named bugs | `layout/tests/regression_font_size_bugs.rs` et al. |
| D7 | **MEDIUM** | Zero-assertion tests: 35/44 in one css file; a gstreamer early-return skips every assertion in CI | `css/tests/test_parser_robustness.rs`, `dll/…/video_codec/mod.rs:576` |
| D8 | **MEDIUM** | `menubar_item_clip.rs` loops assert per-item with no non-empty guard — siblings in the same file have it | `layout/tests/menubar_item_clip.rs:46,252` |
| D9 | **LOW** | 3 `catch_unwind` `Err` arms only `eprintln!`; 3 more use it as the entire check | `layout/src/widgets/{screencap,capture_common,video}.rs` |
| D10 | **LOW** | `unwrap_or` in test helpers masks a poisoned mutex / failed glyph lookup | `layout/src/widgets/drop_down.rs:579`, `cpurender/raster.rs:3593` |
| D11 | **LOW** | Phantom cargo features gate real code (`table_layout`, `xml` — declared nowhere) | `core/src/styled_dom.rs:1181`, `core/src/dom.rs:5778` |

**Part E — engine: silent fallbacks and dead state**

| # | Severity | Finding | Where |
|---|---|---|---|
| E1 | **HIGH** | `dispatch_pending_lifecycle_events` → `bool` ("caller should regenerate") discarded at all 7 call sites | `dll/…/common/event.rs:4150` |
| E2 | **HIGH** | e2e runner drops 5 of 10 `DefaultAction` variants — Enter-on-button and scroll-key paths have zero coverage | `layout/src/e2e/runner.rs:2402` |
| E3 | **HIGH** | `pending_focus_request` fully dead; its e2e assertion is an absence check that can never fail | `layout/src/managers/focus_cursor.rs:57`, `e2e/full.rs:5601` |
| E4 | **MEDIUM** | `gesture.pad_state`: reader wired, **writer** has zero production callers — `get_wacom_pad()` returns `None` forever | `layout/src/managers/gesture.rs:970` |
| E5 | **MEDIUM** | `determine_events_from_managers` has zero production callers; its only integration test is D2-disabled | `layout/src/event_determination.rs:66` |
| E6 | **MEDIUM** | `check_(layout_)properties_changed` zero callers — and a doc comment claims the cache consults it | `core/src/prop_cache.rs:2404`, `layout/src/callbacks.rs:573` |
| E7 | **MEDIUM** | macOS native text input drops `RegenerateLayoutIncremental` — the only one of ~16 sites that does | `dll/…/macos/events.rs:723` |
| E8 | **MEDIUM** | X11 swallows `regenerate_layout()` on resize + DPI change; the same file escalates it loudly elsewhere | `dll/…/x11/mod.rs:2660,2753,3170` |
| E9 | **MEDIUM** | macOS native Cmd+Z/Cmd+Shift+Z updates state but never sets `display_list_dirty` | `dll/…/macos/mod.rs:6755,6776` |
| E10 | **LOW** | headless `frame_needs_regeneration` is written, never read or cleared — tautologically `true` | `dll/…/headless/mod.rs:894,1046,1119` |
| E11 | **LOW** | `SubmitForm`/`CloseModal`/`SelectAllText` are advertised in doc comments, implemented as a comment | `dll/…/common/event.rs:5171` |
| E12 | **LOW** | Zero-caller `validate_class_definition`, `ensure_chrome_references` in `doc/` | `doc/src/print.rs:429`, `doc/src/reftest/regression.rs:553` |

---

## If you fix five things

1. **§1** — make `azul-doc reftest` exit non-zero on failure. One `Ok(())` → a count check. It is
   a named deploy gate with literally no failure path.
2. **§2 + D1** — add `--tests` to `test_lib` and a `cargo test -p azul-doc --lib` step. Recovers
   ~1,545 + ~180 test functions into a blocking gate, and will immediately surface D6/D7.
3. **§3** — add a per-OS `REQUIRED_LANGS` table to `e2e_language_matrix.sh` so a shipped binding
   cannot go green by being SKIPped. The `WINDOWS_NONGATING_LANGS` mechanism is already there;
   it just needs the other direction.
4. **§5 + §4** — stop `|| true`-ing `cargo deny check advisories`, and make
   `check_dep_justifications.py` fail on an empty crate list. Two small diffs, two live
   supply-chain gates restored.
5. **E1** — honour `dispatch_pending_lifecycle_events`' return value. One `if` per platform;
   fixes a real cross-platform "mount callback's refresh is dropped" bug.

---

## CONFIRMED

### 1. CRITICAL — `azul-doc reftest` can never fail

`doc/src/reftest/mod.rs:163-194`

```rust
            Err(e) => {
                println!("  ERROR: {}", e);      // per-test error: printed, swallowed
            }
...
    let passed_tests = final_enhanced_results.iter().filter(|r| r.passed).count();
...
    println!("Passed: {}/{}", passed_tests, final_enhanced_results.len());

    Ok(())                                       // <-- unconditional
}
```

`passed_tests` is computed, printed, written into `results.json` — and then discarded.
`run_reftests` returns `Ok(())` whether 0 or all tests passed. The CI step is:

```yaml
      - name: Run reftests
        run: cargo run -r -p azul-doc reftest      # rust.yml:840
```

**Scenario in which it passes while broken:** every single reftest renders a blank page.
`passed_tests = 0`, the report says `Passed: 0/312`, `azul-doc` exits 0, the step is green,
`test_heavy` is green — and `test_heavy` **is** in `deploy_pages.needs`, so the release ships.
The same applies if Chrome is missing: every comparison takes the `Err(e) => println!` arm.
`find_test_files` returning an empty vec is likewise a `Passed: 0/0` green.

This is the single highest-value fix in the report: it is a named gate, in the deploy's
`needs` list, that has no failure path at all.

**Fix:** in the `["reftest"]` arm of `doc/src/main.rs:1562`, have `run_reftests` return the
`(passed, total)` pair and `std::process::exit(1)` when `passed != total` **or** `total == 0`.
Also promote the per-test `Err(e)` arm to a counted failure rather than a `println!`.

---

### 2. CRITICAL — 1,452 integration tests are gated only by `coverage`, on the exact profile the workflow itself calls a false green — and 93 more run nowhere at all

`rust.yml:759-762`

```yaml
      - name: Run workspace lib tests (debug-assertions + overflow-checks ON)
        run: |
          cargo test -p azul-css -p azul-core -p azul-layout --lib \
            --features azul-core/serde-json,azul-layout/json \
            --no-fail-fast
```

`--lib` restricts `test_lib` — *the* declared test gate — to the crates' **unit** tests.
Everything under `*/tests/` is an integration-test target and is excluded. Counted `#[test]`
functions:

| location | files | `#[test]` fns | `test_lib`? | `coverage`? |
|---|---:|---:|---|---|
| `core/tests/` | 29 | **441** | no | yes |
| `css/tests/` | 9 | **162** | no | yes |
| `layout/tests/` | 102 | **951** | no — `icu_parity` (8) and `e2e_json` (1) have their own jobs | yes, minus 93 (below) |
| `dll/tests/` | 8 | 46 | yes (`cd dll && cargo test`, rust.yml:141) | n/a |

Totals: **1,554** integration `#[test]` fns in core+css+layout. 9 have dedicated jobs
(`icu_parity`, `e2e_json`). Of the remaining 1,545: **93 run in no job at all** and
**1,452 run only under `coverage`**.

I initially read this as "they run nowhere"; tracing `scripts/coverage.sh` corrected it, and
the corrected version is arguably worse because it is precisely the failure mode the workflow
documents in its own comment. `coverage.sh:103-160` **does** run them —
`cargo test --profile coverage -p azul-css --lib --tests`, same for `azul-core`, and for
`azul-layout` it enumerates test targets from `cargo metadata` (a deliberate fix for a
top-level-glob bug, see its comment at :122-126). `run_tests()` propagates the exit status and
the script is `set -euo pipefail`, so a red integration test **does** fail the `coverage` job,
which is in `deploy_pages.needs` and is not `continue-on-error`.

**But `[profile.coverage]` `inherits = "release"`** (root `Cargo.toml`), so
`debug-assertions = false` and `overflow-checks = false`. That is verbatim the condition
`rust.yml:699-722` identifies as a false green and created `test_lib` to fix:

> *"coverage.sh builds with `--profile coverage`, which `inherits = "release"` … so
> `debug-assertions = false` … Every `#[cfg(debug_assertions)]` test is then compiled OUT of
> the binary — silently, with the same "N passed" line … The `coverage` job stays, but for
> instrumentation only — it is not the test gate and must never be treated as one."*

The fix was applied to unit tests and **not** to integration tests, which were left in exactly
the state that paragraph condemns — while the same paragraph asserts `coverage` is not a gate,
so nobody is watching it as one.

**Worse, 93 of them run in ZERO CI jobs.** `scripts/coverage.sh:110-118`:

```bash
# Exclude slow integration tests (>10s in debug) that blow up under coverage
# instrumentation. These still run in the `test_lib` CI job without coverage.
SLOW_TESTS=(
  "test_scrollbar_detection"  "flexbox_integration"  "inline_gradient_border"
  "cache_and_dirty_propagation"  "inline_block_text"  "ifc_caching"
  "margin_escape_regression"
)
```

**That comment is false.** `test_lib` uses `--lib`; it has never run a single one of these.
Counted: 20 + 19 + 8 + 18 + 5 + 18 + 5 = **93 `#[test]` functions excluded from coverage on
the strength of a guarantee that does not exist.** These are layout-cache, dirty-propagation,
scrollbar-detection and margin-collapse regression suites — core engine behaviour.

Every `cargo test` invocation in the whole `.github/workflows/` tree (8 of them, verified
exhaustively):

```
rust.yml:141   cd dll && cargo test
rust.yml:761   cargo test -p azul-css -p azul-core -p azul-layout --lib …
rust.yml:782   cargo test --release -p azul-layout --test e2e_json --features e2e-server
rust.yml:812   cargo +nightly miri test --lib -p azul-core -- refany::
rust.yml:992   cargo +nightly miri test --lib -p azul-dll -- …
rust.yml:1052  cargo test -p azul-layout --test icu_parity …
rust.yml:1056  cargo test -p azul-layout --test icu_parity … --no-run
rust.yml:1184  cargo test -p azul-dll --test xml_to_rust_compilation --test kitchen_sink_integration --features xml
```

**Fix:** add `--tests` to the `test_lib` step (or a second step with the same feature set), so
integration tests run on the dev profile alongside the unit tests; then delete the `SLOW_TESTS`
exclusion's false justification — with `test_lib` genuinely running them, the comment becomes
true and the 93 are covered.

*Checked and clean:* `layout/tests/contenteditable_e2e.rs` carries
`required-features = ["cpurender", "text_layout", "font_loading", "a11y", "widgets"]`
(`layout/Cargo.toml:357-360`) — I verified every one is reachable from azul-layout's default
feature set (`font_loading_multithreaded = ["font_loading", …]`, `layout/Cargo.toml:221`), so
it is **not** silently skipped by coverage.sh. `coverage.sh:148-153` also explicitly handles
`e2e_json`'s `required-features`, with a comment naming the silent-skip trap.

---

### 3. CRITICAL — the language-binding gate cannot fail when the toolchain is missing

`scripts/e2e_language_matrix.sh:2234-2238`

```bash
if [ "$GATE_SHIPPED" = 1 ] && [ "$shipped_fails" -gt 0 ]; then
  echo "--gate-shipped: ${shipped_fails} SHIPPED binding(s) FAILED ->${shipped_failed_list} -> exiting nonzero." >&2
  exit 1
fi
…
exit 0
```

`shipped_fails` counts only rows whose status is `FAILS`. A missing toolchain yields `SKIP`,
and **SKIP never gates** (stated explicitly at `:2224-2232`). I enumerated the guard in each
shipped language's recipe: **16 of 16 SHIPPED-tier languages have a toolchain-absent → `skip`
early return**, e.g.

```bash
scripts/e2e_language_matrix.sh:1245   have scalac || { skip scala "scalac not installed (coursier/setup-action)"; return; }
scripts/e2e_language_matrix.sh:1288   have zig    || { skip zig "zig not installed (mlugg/setup-zig)"; return; }
scripts/e2e_language_matrix.sh:1554   … skip haskell "cabal/ghc not installed (haskell-actions/setup)"; return
scripts/e2e_language_matrix.sh:1526   … skip ocaml "dune/ocaml not installed (ocaml/setup-ocaml + opam ctypes)"; return
```

Now pair that with `e2e_native`: **15 toolchain-setup steps in that job are
`continue-on-error: true`** — rust.yml:2270, 2277, 2296, 2306, 2319, 2328, 2340, 2350, 2360,
2368, 2377, 2387, 2397, 2403, 2411 (setup-ocaml, opam ctypes, setup-odin, setup-nim,
setup-dlang, setup-racket, setup-swift Linux, setup-swift Windows, setup-zig, setup-go macOS,
coursier/scala, setup-haskell macOS, install-crystal, setup-v, setup-julia).

**Scenario in which it passes while broken:** `coursier/setup-action` has an upstream outage.
`scalac` is not on PATH. `lang_scala` records `SKIP`. `shipped_fails` stays 0. The job named
`AZ_E2E scripting (ubuntu-22.04)` goes green, `deploy_pages` proceeds, and Scala — a SHIPPED
binding the file itself annotates *"SHIPPED — must prove green on all three OSes"*
(rust.yml:2374) — was never compiled or run. In the limit, all 24 scripting-family rows can
be SKIP and the gate still exits 0. This is precisely the retired
`AZ_E2E scripting (windows)` / cabal incident, structurally unchanged.

Note the contrast with `e2e_headless` (rust.yml:2149-2185), which was hardened against exactly
this ("PROOF OF EXECUTION. Exit 0 alone is not a gate: a runner that ran nothing would also
exit 0"). That hardening was never applied to the language matrix.

**Fix:** add the mirror image of `WINDOWS_NONGATING_LANGS` — a per-OS `REQUIRED_LANGS` table
of (lang, OS) pairs that MUST report `WORKS`; fail if any required row is `SKIP`. The exception
mechanism already exists, it is just only wired in the permissive direction.

---

### 4. HIGH — the "BLOCKING" dependency-justification gate passes vacuously

`scripts/check_dep_justifications.py:144-151`

```python
    members = collect_member_files(args.paths)
    if not members:
        # No crate lists means the dep-tree generation produced nothing — we
        # can't verify the gate, so fail loudly rather than pass vacuously.
        …
        return 1
```

The guard tests for the **existence of files**, not for their **content**. The producer
(rust.yml:4501-4514) is:

```bash
          gen() { # <label> <cargo-tree args...>
            local label="$1"; shift
            cargo tree "$@" -e normal,build > "deptree/${label}.${os}.tree.txt" 2>/dev/null || true
            cargo tree "$@" -e normal,build --prefix none 2>/dev/null \
              | sed -E 's/ \(.*\)//; s/ v[0-9].*//' | grep -E '^[a-z0-9]' | sort -u \
              > "deptree/${label}.${os}.crates.txt" || true
          }
```

The `>` redirection **creates the file before `cargo tree` runs**, so a failing `cargo tree`
leaves a present-but-empty `crates.txt`. `members` is then non-empty (4 files), `present` is
the empty set, `missing` is empty, and the script prints
`0 unique crates · ✅ 0 unjustified` and returns 0.

**Scenario in which it passes while broken:** the `Generate DLL API bindings` step in
`dep_tree` is itself `continue-on-error: true` (rust.yml:4419). Codegen fails → `cargo tree
-p azul-dll --features build-dll` fails → empty file → the gate that is supposed to force a
review line for every new transitive dependency reports success while inspecting nothing.
An attacker-or-accident-introduced dependency lands unreviewed.

**Fix:** after building `present`, `if not present: return 1`. Better: fail per member whose
`crates.txt` is empty, and drop the `|| true` from `gen()` (or `set -o pipefail` + check).

---

### 5. HIGH — cargo-deny reports nothing, twice, and then is ignored anyway

`rust.yml:4198-4223`

```yaml
            for chk in advisories bans licenses sources; do
              echo "=== cargo deny check $chk ==="
              cargo deny --color never check "$chk" 2>&1 || true
              echo ''
            done
          } > stats/cargo-deny.txt
```
…and the identical loop again into `$GITHUB_STEP_SUMMARY`. The exit code of every one of the
eight `cargo deny check` invocations is discarded with `|| true`, and nothing downstream ever
reads `stats/cargo-deny.txt` for the string `error`. On top of that the whole job carries
`continue-on-error: true` (rust.yml:4189), and it is not in `deploy_pages.needs`.

**Scenario in which it passes while broken:** a RUSTSEC advisory lands against a transitive
dependency. `cargo deny check advisories` exits 1. `|| true` eats it. The check named
"Supply chain (cargo-deny)" is green. The advisory text is visible only to whoever opens the
run Summary and reads it. Three independent layers of suppression on a security gate.

**Fix:** keep the report generation, but capture the status:
`rc=0; cargo deny … || rc=$?` per check, accumulate, and `exit $rc` at the end; then drop
`continue-on-error: true` at least for `advisories`. (`bans`/`licenses`/`sources` are
defensible as advisory-only; `advisories` is not.)

---

### 6. HIGH — the ASan gate passes via its own timeout, and its exit trigger is dead code

`rust.yml:966-977`

```yaml
      - name: Run C hello-world with ASan
        working-directory: examples/c
        env:
          …
          ASAN_OPTIONS: detect_leaks=0,abort_on_error=1
          AZ_EXIT_SUCCESS_AFTER_FRAME_RENDER: "1"
          AZ_BACKEND: headless
        run: ${{ matrix.timeout_cmd }} 30 ./hello-world-asan || [ $? -eq 124 ]
```

Two compounding problems, both traced:

1. `AZ_EXIT_SUCCESS_AFTER_FRAME_RENDER` is read **only** by the windows, macos, x11 and
   wayland backends — `grep -rl` across the whole tree returns exactly
   `dll/src/desktop/shell2/{windows,macos,linux/x11,linux/wayland}/mod.rs`. The `headless`
   backend (`dll/src/desktop/shell2/headless/mod.rs:1258`) never reads it; it enters a
   "condvar-based blocking event loop" and its only early-exit env var is
   `AZ_HEADLESS_SNAPSHOT_PATH` (`:1288`), which this step does not set.
2. Therefore the process **always** runs to the 30 s wall and is killed → rc 124 → the
   `|| [ $? -eq 124 ]` clause converts that into success.

**Scenario in which it passes while broken:** the app renders nothing, hangs on the first
layout, or deadlocks — all indistinguishable from the normal path, all rc 124, all green.
The step retains *some* value (an ASan abort inside the first 30 s still yields rc 134 and
fails), but it has never once verified that a frame rendered, which is what the env var and
the step's placement imply.

The same rc-124-is-pass rule is in `sanitizers` (see §12).

**Fix:** either teach the headless backend to honour `AZ_EXIT_SUCCESS_AFTER_FRAME_RENDER`
(a 5-line change next to the existing `AZ_HEADLESS_SNAPSHOT_PATH` handler) and then drop
`|| [ $? -eq 124 ]`; or set `AZ_HEADLESS_SNAPSHOT_PATH=/tmp/asan.png` and assert the PNG
exists and is non-empty afterwards.

---

### 7. HIGH — 11 of 20 `deploy_pages.needs` entries can never block the deploy

`rust.yml:3220`. A job with `continue-on-error: true` always reports *success* to its
dependents, so listing it in `needs` is sequencing, not gating. Job-level
`continue-on-error` mapped to jobs:

| in `needs` | job | job-level `continue-on-error` | can block? |
|---|---|---|---|
| ✓ | test_lib | — | **yes** |
| ✓ | test_heavy | — | yes (but see §1 — its reftest step cannot fail) |
| ✓ | build_binaries | — | **yes** |
| ✓ | cross_build_binaries | `${{ matrix.experimental \|\| false }}` (rust.yml:377) | partial — 5 solid legs block, 3 exotic don't |
| ✓ | build_rust9x | `false` (rust.yml:563) | **yes** |
| ✓ | build_linux_packages | — | **yes** |
| ✓ | build_website_skeleton | — | **yes** |
| ✓ | coverage | — | **yes** |
| ✓ | e2e_native | — | yes (but see §3) |
| ✓ | e2e_headless | — | **yes** |
| ✓ | build_demos | `true` (1778) | **no** |
| ✓ | docs_pdf | `true` (4544) | **no** |
| ✓ | build_mobile | `true` (2508) | **no** |
| ✓ | build_mobile_apps | `true` (2688) | **no** |
| ✓ | build_mobile_apps_android | `true` (2778) | **no** |
| ✓ | pypi | `true` (4793) | **no** |
| ✓ | npm | `true` (5019) | **no** |
| ✓ | rubygems | `true` (5110) | **no** |
| ✓ | nuget | `true` (5203) | **no** |
| ✓ | maven-central | `true` (5289) | **no** |

The job's own comment acknowledges this for the five packagers ("*All five … are
continue-on-error, so a packager failure can delay but never block the deploy*") but **not**
for `build_demos`, `docs_pdf`, `build_mobile*`, which are listed with no caveat and read as
gates. `docs_pdf` failing means the release page's `guide.pdf` is simply absent — copied with
`find … -exec cp … \; 2>/dev/null || true` at rust.yml:3637.

**Fix:** annotate the four unacknowledged ones in the comment, and for `build_demos` /
`docs_pdf` decide whether "demo binaries and the guide PDF silently missing from a release"
is acceptable. If not, drop their `continue-on-error`.

---

### 8. HIGH — the double-drop runtime gate, ASan, Miri-on-dll and export-code E2E do not gate the release

`rust.yml:3220`. Jobs **absent** from `deploy_pages.needs` entirely:

`lint_and_check`, `feature_matrix_check`, `cross_compile_check`, `ffi_safety_tests`,
`icu_parity`, `c_compile_check`, `export_code_e2e`, `clippy`, `dep_tree`, `supply_chain`,
`unsafe_audit`, `sanitizers`.

The consequential ones:

* **`lint_and_check`** contains `Double-drop invariant check (azul-doc)` (rust.yml:149) and
  `Double-drop runtime gate (appconfig_double_drop)` (rust.yml:156) — described in-file as
  *"the hard guard that the field-glue-delegation fix … stays correct"* — plus
  `cd dll && cargo test` (the only run of `dll/tests/`). A red double-free gate does not stop
  a release.
* **`ffi_safety_tests`** is ASan on the C hello-world + Miri on `azul-dll`. Also non-gating.
* **`export_code_e2e`** compiles the exported page in four languages. Non-gating.
* **`c_compile_check`** proves `azul.h` still compiles under gcc/clang/cl/clang-cl. Non-gating —
  so a release can ship a header no C compiler accepts.

**Fix:** add `lint_and_check`, `ffi_safety_tests`, `c_compile_check` and `export_code_e2e` to
`deploy_pages.needs`.

---

### 9. MEDIUM — `build_binaries`' cache key omits exactly the files its gates guard

`rust.yml:1244` (key) + `1250-1269` (the `skip=true` short-circuit):

```yaml
          key: build-${{ env.CACHE_BUST }}-${{ matrix.os }}-${{ hashFiles('Cargo.lock') }}-${{ hashFiles('css/src/**/*.rs', 'core/src/**/*.rs', 'layout/src/**/*.rs', 'dll/src/**/*.rs', 'dll/build.rs') }}
```

Not hashed: the **root `Cargo.toml`** (where `[profile.release] panic = "abort"` lives),
`dll/Cargo.toml` (feature definitions), `api.json`, `doc/src/**` (codegen). When the key hits,
`skip=true` and *every* subsequent step is `if: steps.cache-valid.outputs.skip != 'true'` —
including `Assert panic=abort` (1646), `Verify libazul.so is python-free` (1583),
`Assert cdylib exports only the azul C API` (1673), `Python pyclass teardown gate` (1598) and
`Build Rust examples` (1485).

**Scenario in which it passes while broken:** someone edits `[profile.release]` in the root
`Cargo.toml` and flips `panic` back to unwind. No hashed file changed → exact cache hit →
`skip=true` → the `Assert panic=abort` gate is *skipped*, and the **stale cached artifacts**
are uploaded and shipped. The gate written specifically to catch a profile-inheritance
regression is bypassed by exactly that regression. Same story for an `api.json`-only change:
stale `target/codegen` (which is inside the cached path) is uploaded, so the release ships
headers and bindings that do not match `api.json`.

**Fix:** add `Cargo.toml`, `dll/Cargo.toml`, `api.json` and `doc/src/**/*.rs` to the
`hashFiles(…)` set for this cache key.

---

### 10. MEDIUM — a failed packager still ships the dead `pip install` line

`rust.yml:3971-3993`. The mirror-existence guard added after the `/ui/azul` incident is:

```bash
          check_channel() { # $1 = artifact dir, $2 = expected mirror dir
            if [ -d "$1" ] && [ -n "$(ls -A "$1" 2>/dev/null)" ] && [ ! -d "$2" ]; then
              echo "::error::$1 has artifacts but $2 was not built — the install commands for this channel would 404"
```

It only fires **when the artifact is present**. But `pypi`, `npm`, `rubygems`, `nuget` and
`maven-central` are all `continue-on-error: true`, so a failed packager produces *no*
artifact — the guard stays silent, `website/ui/azul` is never created, and the deploy is green.

Meanwhile the install text is static: it comes from `api.json:2785-2823`
(`pip install azul --index-url $HOSTNAME/ui`), not from what actually got built. So the exact
user-visible symptom of the original incident — the site telling users to `pip install` from a
path that 404s — recurs through the other door.

**Fix:** make the assertion unconditional (`website/ui/{azul,npm,gems,nuget,maven}` must
exist, period), or have the docgen omit a channel's install tab when its mirror is absent.

---

### 11. MEDIUM — 0-byte placeholder binaries can ship

`rust.yml:3141-3152`:

```bash
          touch doc/target/deploy/release/0.2.0/libazul.so
          touch doc/target/deploy/release/0.2.0/libazul.linux.a
          …
```

and the merge step (rust.yml:3661-3707) overwrites them with, 50 times over:

```bash
          find artifacts-linux -name "libazul.so" -exec cp {} "$RELEASE_DIR/" \; 2>/dev/null || true
```

**All 48 artifact-download steps in `deploy_pages` (rust.yml:3270-3620) are
`continue-on-error: true`** — including the three non-optional ones,
`azul-linux-amd64` / `azul-macos-amd64` / `azul-windows-amd64` (:3276, :3283, :3290). So a
missing artifact leaves the target dir absent, `find` matches nothing, `cp` never runs, the
`|| true` guarantees success, and the **0-byte placeholder remains** in the published tree.
I grepped lines 3197-4185 for `exit 1` / `::error::`: the only assertion in the whole deploy
job is the registry-mirror one from §10.

`build_binaries` is in `needs` and does block, so this needs the narrower trigger of "build
succeeded but the upload produced nothing" — plausible because
`Upload Artifacts` (rust.yml:1726) has **no `if-no-files-found: error`** (§16) and so defaults
to a warning — or an artifact-name drift. Blast radius is a published release whose
`libazul.so` download is 0 bytes.

**Fix:** after the merge, iterate the expected release filenames and fail on any that is
still zero bytes (`[ -s "$f" ]`) — the `Bundle dll + demos` step at rust.yml:4004 already uses
`[ -s "$lib" ]`, so the idiom is right there.

---

### 12. MEDIUM — `sanitizers`: timeout is "pass", build failure is a table row, TSan can never fail

`rust.yml:4441-4459`

```bash
          run_san() { # <example> <label> <san-flags>
            …
            if ! clang -g -O1 $flags … -o "/tmp/$ex-san" 2>/tmp/cc.log; then
              emit "$ex" "$label" "build-fail"; return
            fi
            local rc=0
            ASAN_OPTIONS=detect_leaks=0:abort_on_error=1 UBSAN_OPTIONS=halt_on_error=1 \
            TSAN_OPTIONS=halt_on_error=0 LD_LIBRARY_PATH=target/debug \
              timeout 30 "/tmp/$ex-san" || rc=$?
            if [ "$rc" -eq 0 ] || [ "$rc" -eq 124 ]; then
              emit "$ex" "$label" "pass"
```

Four independent suppressions in fifteen lines: (a) a compile failure emits the string
`build-fail` into a markdown table and returns 0; (b) `rc == 124` (the timeout — which, per §6,
is the *normal* outcome under `AZ_BACKEND: headless`) is reported as `pass`; (c)
`TSAN_OPTIONS=halt_on_error=0` means a data race never changes the exit code, so the
`ThreadSan` row is unconditionally `pass`; (d) the job is `continue-on-error: true` and not in
`deploy_pages.needs`. Nothing ever greps `stats/sanitizers.txt` for `FAIL` or `build-fail`.

**Fix:** accumulate a failure flag across `run_san` calls and `exit 1` at the end; treat
`build-fail` and (once §6 is fixed) `124` as failures; drop `halt_on_error=0` for TSan or
declare TSan explicitly informational in its own row label.

---

### 13. MEDIUM — the WASM `Instant::now()` gate greps the wrong pattern and accepts any `#[cfg`

`rust.yml:165-203`

```bash
            done < <(grep -rn 'std::time::Instant::now()' "$dir" --include='*.rs' || true)
…
              START=$((LNO > 30 ? LNO - 30 : 1))
              CONTEXT=$(sed -n "${START},${LNO}p" "$FILE")
              if ! echo "$CONTEXT" | grep -q '#\[cfg'; then
```

Two weaknesses, both verified against the current tree:

* **Pattern.** It matches only the fully-qualified `std::time::Instant::now()` — 15 hits in
  `css/src core/src layout/src`, of which 5 are comments. The idiomatic
  `use std::time::Instant;` + `Instant::now()` form, and the aliased
  `use std::time::Instant as StdInstant;` + `StdInstant::now()` form (`core/src/task.rs:32`,
  used at `:264`, `:266`, `:1190`), are **invisible to it**. A new
  `use std::time::Instant;` + `Instant::now()` in un-gated code passes the check silently.
* **Gate detection.** Any `#[cfg` within the preceding 30 lines satisfies it —
  `#[cfg(test)]`, `#[cfg(target_os = "windows")]`, `#[cfg(feature = "svg")]` all count, none
  of which excludes WASM.

I checked the actual current call sites and they are all legitimately gated
(`#[cfg(feature = "std")]` at `layout/src/window.rs:2161/5150/5179/5978/6032/7744`,
`#[cfg(all(test, feature = "std"))]` at `layout/src/managers/scroll_state.rs:1396`,
`#[cfg(not(target_family = "wasm"))]` at `layout/src/font.rs:222`, and `probe.rs`'s module-level
`#[cfg(all(feature = "probe", not(target_family = "wasm"), not(feature = "web_lift")))]`), so
this is a **latent** hole, not a live bug — the check is currently green *and* correct, by
luck.

The `|| true` at :195 is **legitimate** — `grep` exits 1 on no-match and that must not abort
the loop.

**Fix:** widen the pattern to `\bInstant::now\(\)` plus a scan of `use std::time::Instant`
aliases, and require the enclosing `#[cfg]` to actually mention `wasm` or `feature = "std"`
rather than accepting any `#[cfg` token.

---

### 14. MEDIUM — `dll/tests/leak_regression.rs` has never run in CI

`dll/tests/leak_regression.rs:24-29`

```rust
#![cfg(all(
    test,
    feature = "build-dll",
    feature = "e2e-test",
    target_os = "macos"
))]
```

The only CI invocation that could reach it is `cd dll && cargo test` (rust.yml:141), which runs
on `ubuntu-22.04` with default features — neither `build-dll` nor `e2e-test` nor macOS. There is
no macOS `cargo test -p azul-dll` job anywhere in the workflow tree. A whole-file `#![cfg]`
that evaluates false compiles to **zero** tests and reports `0 passed`, indistinguishably from
a passing run.

**Fix:** add a macOS leg that runs `cargo test -p azul-dll --features build-dll,e2e-test
--test leak_regression`, or delete the file so it stops reading as coverage.

---

### 15. LOW — the Miri gate is a name filter that exits 0 on zero matches

`rust.yml:812`

```yaml
          cargo +nightly miri test --lib -p azul-core -- refany::
```

`cargo test -- <filter>` with no matching test names exits 0 and prints `0 passed; 0 failed`.
Today `core/src/refany.rs` has 50 `#[test]` fns so the filter is live, but renaming or moving
the module (e.g. `refany` → `ref_any`, or into a submodule path) silently reduces the gate to
nothing while `test_heavy` — a `deploy_pages.needs` entry — stays green.

The same applies to `rust.yml:992` (`--skip desktop::extra` … on `azul-dll`), though there the
filters are exclusions, which fail safe.

**Fix:** pipe through a count assertion, e.g.
`… | tee m.txt; grep -qE 'test result: ok\. [1-9][0-9]* passed' m.txt`.

---

### 16. LOW — `build_binaries` symbol gates are `[ -f ] || continue` loops

`rust.yml:1646-1660` and `1673-1706`:

```bash
          for f in target/prod-release/libazul.so  target/prod-release/libazuldbg.so \
                   … target/prod-release/azul.so; do
            [ -f "$f" ] || continue
```

If none of the listed paths exist the loop body never executes, nothing is printed, and the
step exits 0. Likewise `Verify libazul.so is python-free` (rust.yml:1583):

```bash
          PY=$(nm -D target/prod-release/libazul.so 2>/dev/null \
            | grep -E '\b(Py|_Py)[A-Za-z0-9_]*' || true)
          if [ -n "$PY" ]; then … exit 1; fi
          echo "OK: libazul.so is python-free (clean az_* C ABI)"
```

A missing file makes `nm` fail into `/dev/null`, `PY` empty, and the step prints
`OK: libazul.so is python-free`. In practice the preceding `cargo build` would have failed
first, so this is a **latent** hazard that bites on a profile/target-dir rename — but that is
exactly the kind of change that has bitten this repo before. The final `Upload Artifacts`
(rust.yml:1726) has no `if-no-files-found: error`, so it would only warn.

**Fix:** assert at least one file matched (`n=0; … n=$((n+1)); [ "$n" -gt 0 ] || exit 1`), and
set `if-no-files-found: error` on the release-artifact upload (the website-skeleton uploads at
rust.yml:3179/3189 already do).

---

### 17. HIGH — 4 of 12 Rust examples are silently skipped by `cargo check --examples`

`rust.yml:1485`

```yaml
      - name: Build Rust examples
        if: steps.cache-valid.outputs.skip != 'true'
        run: cargo check --verbose --examples
```

This is the `required-features` trap from the brief, applied to `--examples` instead of
`--test`. `examples/rust/Cargo.toml` declares `default = ["link-static"]` — and **nothing
else**. Four of its twelve `[[example]]` targets require features that are therefore off:

| example | `required-features` | line |
|---|---|---|
| `opengl` | `["serde"]` | `examples/rust/Cargo.toml:55` |
| `icu_demo` | `["icu"]` | `:76` |
| `fluent_demo` | `["fluent", "icu"]` | `:81` |
| `http_zip_demo` | `["fluent", "http", "zip"]` | `:86` |

Cargo skips a target with unmet `required-features` **silently — no warning, no diagnostic,
exit 0**. So **33% of the shipped Rust examples have never been compile-checked by CI.** These
are the user-facing API surface the guide and website link to.

`opengl` compounds it: it is in the screenshot loop at `rust.yml:1496`
(`for example in hello-world widgets opengl infinity async xhtml calc`), and that step is
`continue-on-error: true` with `|| echo "Screenshot for $example failed"` (:1499). So a broken
`opengl` example is invisible twice — never compiled, and its screenshot failure swallowed.

Also skipped for the same reason: `dll/Cargo.toml:898-901`'s `transpile_fn`
(`required-features = ["web", "link-static"]`; `web` is not in azul-dll's defaults).

**Fix:** name the feature sets explicitly, e.g.
`cargo check -p azul-examples --examples --features serde,icu,fluent,http,zip` — or add a
per-example check step. Naming them makes a removed feature an error instead of a skip, the
same idiom `test_lib` already uses for `e2e_json` (rust.yml:782).

---

### 18. MEDIUM — `css_double_drop` is compiled but never run

`dll/examples/css_double_drop.rs:1-17` is a 200,000-iteration allocator-abuse repro for the
`CssPropertyCachePtr` double-free (issue #15):

> *"Looping many times means a double-free corrupts the allocator and aborts (`free(): double
> free detected` / SIGSEGV) almost immediately; a clean run to completion proves the fix
> holds."*

Its sibling is wired up as a hard gate — `rust.yml:155-156`:

```yaml
      - name: Double-drop runtime gate (appconfig_double_drop)
        run: cargo run -r -p azul-dll --example appconfig_double_drop --features link-static
```

`css_double_drop` gets no such step. Grepping `.github/` and `scripts/` for it returns only
two mentions in a handoff markdown. Because `link-static` **is** in azul-dll's defaults, it is
picked up by `cargo check --examples` — so it **compiles** in CI and never **runs**. A
`cargo check` proves nothing about a double free; only executing the 200k loop does.

**Scenario in which it passes while broken:** the `ManuallyDrop` + `run_destructor` gating in
`core/src/prop_cache.rs` regresses. `css_double_drop` still type-checks, CI stays green, and
the issue-#15 double free is back in a shipped release.

**Fix:** add a step mirroring rust.yml:156:
`cargo run -r -p azul-dll --example css_double_drop --features link-static`. (And note
`lint_and_check`, where both live, is not in `deploy_pages.needs` — see §8.)

---

## Part A — what I checked and found SOLID

So the owner can judge coverage, not just hits. All verified by reading the code, not assumed.

* **`e2e_headless` (rust.yml:2124-2185)** — the model gate in this repo. Refuses an empty
  `e2e/` dir; cross-checks `--list` output against the file count; parses the summary line and
  requires `passed+failed+xfailed+xpassed == expected` so a silently-skipped scenario fails;
  fails on any FAIL or XPASS; `set +e`/`PIPESTATUS[0]` so `tee` cannot mask the exit code.
  Its comment names the exact trap it was built against. **Every other runner-style gate in
  the file should be measured against this one.**
* **Script existence** — all 14 script paths referenced from `rust.yml` exist, are tracked,
  and are executable. The `build_registry_mirrors.sh` sparse-checkout hole is **fixed**
  (rust.yml:3265-3268 now names it explicitly, with the incident written up at :3253-3262).
* **`azul-doc check`** (`doc/src/main.rs:302`) — `anyhow::bail!` on any problem. Correct.
* **`azul-doc autoreview autodoc-check --strict`** (`doc/src/reftest/autodoc.rs:1306`) —
  returns `Err` when strict and stale. Correct.
* **`test_lib`'s `e2e_json` step** (rust.yml:782) — names the `required-features` target
  explicitly so a removed feature *errors* instead of silently skipping, with the rationale
  written out at :766-781. Exactly the right idiom.
* **`icu_parity` cache-skip** — gates on `cache-hit == 'true'` (exact key only), so a
  `restore-keys` prefix hit cannot skip the run. Correct handling of a subtle trap.
* **`cross_build_binaries`** — `continue-on-error: ${{ matrix.experimental || false }}`; the
  5 shipping targets block, only ppc64/s390x/riscv64 are non-gating, and that is documented.
* **`clippy`** — `set -o pipefail` + `-D warnings`, explicitly *not* `continue-on-error`, with
  a comment saying so. The `grep -c … || echo 0` in the summary step is cosmetic only.
* **`build_website_skeleton`** — both uploads use `if-no-files-found: error`.
* **`publish_safety`** — real analysis: the `[patch]` check, `cargo package --verify azul-css`,
  and an `include_bytes!`/`include_str!` tarball-membership check driven by an actual publish
  failure. `set -euo pipefail` on the packaging loop.
* **Publish-job secret gating** (crates-io :4822, pypi :4947, npm :5146, rubygems :5237,
  nuget :5319, maven :5504, aur :5567) — all six use the same explicit
  `if [ -z "$TOKEN" ]; then echo "::notice::… skipping publish"` pattern. A missing secret is a
  configuration state, not a swallowed failure, and it is signalled. Consistent and correct.
  *(Observation, not a finding: a `deploy` run with every token unset publishes nothing and is
  fully green with only `::notice::` annotations.)*
* **`dep_tree`'s justification gate** — the *idea* is right (every transitive crate needs a
  written reason) and the script has a vacuity guard; it is just the wrong vacuity guard (§4).
* **`[profile.coverage]` / generated-test inflation** — the 2.28x coverage inflation is
  **fixed** (`scripts/coverage.sh:173,195`: `--ignore "*/tests/*"` plus
  `--excl-start '^\s*mod autotest_generated\s*\{'`).
* **Tautological-assertion spot check** — grepped `css/src core/src layout/src dll/src` plus
  all four `*/tests/` trees for `assert!(… >= 0)`, `is_ok() || is_err()`, `assert!(true`. Only
  **2 real hits**, both in feature-gated stub arms:
  `dll/tests/xml_to_rust_compilation.rs:627` (`assert!(true);`) and
  `dll/tests/kitchen_sink_integration.rs:153`
  (`assert!(true, "XML compilation requires 'xml' feature")`). Every `>= 0` hit I traced is on
  a genuinely **signed** value and is a real check — `core/src/gl_fxaa.rs:558-559`
  (`braces: i32`), `layout/src/widgets/map.rs:1795` (`visible_tile_range -> (i32,i32,i32,i32)`).
  `core/src/path_parser.rs:1938` (`assert!(r.is_ok() || r.is_err(), "must return, not panic")`)
  is a tautology as written, but the real assertion is that the line is reached at all, so it
  is not a false green.
* **`#[ignore]` inventory** — 11 ignored tests, all in `dll/src/desktop/`, all with an explicit
  written reason: `display.rs:1308,1326,1339` and `menu.rs:498,518,541,563,585`
  (*"Requires main thread and real display hardware"*),
  `extra/video_codec/provision.rs:1850,1888` (*"needs a real Linux desktop"*),
  `extra/screencap/dmabuf.rs:658` (*"requires a GPU / libEGL"*). All legitimate — none is a
  disabled failing test. *Observation, not a finding:* no CI job runs `cargo test -- --ignored`
  on hardware, so these 11 are permanently unexercised. That is a coverage gap, not a false
  green.
* **`required-features` inventory** — 11 declarations workspace-wide, all enumerated. Handled
  correctly: `layout/Cargo.toml:365` (`e2e_json`, named explicitly at rust.yml:782 and
  coverage.sh:152), `layout/Cargo.toml:360` (`contenteditable_e2e`, all five features reachable
  from defaults), `dll/Cargo.toml:906-921` (four `link-static` examples, and `link-static` is
  in azul-dll's defaults). Handled **incorrectly**: the five in §17/§18.

---

# Part B — E2E scenario runner

Audited `layout/src/e2e/{full,runner,report}.rs`, `doc/src/e2erun.rs` and the 61 committed
JSON fixtures. **The runner has already been through an adversarial hardening pass** — every
specific bug named in the brief (`assert_changed` on dimension mismatch, `assert_damage` typo
fall-through, `assert_idle_stable`/`assert_work_bounded` liveness, `touch_state`,
manufactured `needs_update`, the `CallbackChange` catch-all) is fixed and carries a
regression comment. 6 residual findings, all lower severity than Part A.

### B1. HIGH — `click` / `click_node` report `pass` when the target does not exist

`layout/src/e2e/full.rs:9721-9728` (`DebugEvent::Click`) and `:11557-11563`
(`DebugEvent::ClickNode`):

```rust
                None => {
                    let response = ClickNodeResponse {
                        success: false,
                        message: "Could not resolve click target (no matching node or position)"
                            .to_string(),
                    };
                    send_ok(request, None, Some(ResponseData::ClickNode(response)));
                }
```

The step loop (`resume_e2e_continuation_inner`, full.rs:6902-6993) maps any
`Ok(DebugResponseData::Ok{..})` to `status: "pass"` **without reading the payload**, so
`success: false` is invisible. `{"op":"click","selector":".typo"}` passes while queueing no
window-state change at all.

This is inconsistent with every sibling node-addressed op — I verified `FocusNode`
(full.rs:9409-9423), `ScrollNodeBy`/`ScrollNodeTo`/`ScrollIntoView` (11237-11433),
`GetNodeLayout` (10380-10433), `InsertNode`/`DeleteNode`/`SetNodeText`/`SetNodeClasses`/
`SetNodeCssOverride` (12756-13062) and `TextInput` (12411-12439) all correctly `send_err`.
`FocusNode`'s own comment states the rationale verbatim: *"a test that focused a node that does
not exist … would run its whole keyboard timeline against no focus at all and blame the
engine."*

**Currently masked**, not live: all 4 committed fixtures using `click` follow it with a state
assertion (e.g. `assert_text` in `tests/e2e/hello_world_counter.json`). It becomes a false
green the moment a scenario clicks for a side effect only — and the generated corpus is far
larger than the committed one.

**Fix:** `send_err` in both `None` branches, matching the siblings.

### B2. HIGH — `set_app_state` reports success on failure

`layout/src/e2e/full.rs:12189` (`DebugEvent::SetAppState`). All three failure paths — not
deserializable (:12195-12200), JSON parse error (:12223-12229), restore error (:12214-12220) —
`send_ok(… AppStateSetResponse { success: false, … })`. Same class as B1; currently masked in
`tests/e2e/undo_redo.json` by a following `assert_app_state`.

### B3. MEDIUM — `assert_work_bounded` accepts zero bounds

`layout/src/e2e/full.rs:5184-5313`. Verified directly: the function has the liveness
precondition (`frames_since_reset == 0` → fail, :5209-5218) and the depth-cap check, but
**no** "at least one of `min_/max_/exact_{relayouts,dom_regens,layout_passes}` was supplied"
guard. `assert_damage` (:4660-4730) has exactly that guard, with the comment *"no constraint
given … otherwise this assertion passes unconditionally."*

`{"op":"assert_work_bounded"}` with no params passes on any window that rendered one frame and
didn't hit the depth cap, while emitting a convincing message
(`"N event iteration(s), M DOM regen(s) … depth cap not hit"`) that asserts nothing about the
amount of work.

**Fix:** copy `assert_damage`'s `CONSTRAINTS.iter().any(…)` guard.

### B4. MEDIUM — `assert_manager_invariants` has no "something was checked" guard

`layout/src/e2e/full.rs:5695-6015`. It accumulates a `checked` counter but never tests
`checked == 0` before returning pass (:6000-6006), unlike `assert_dom`,
`assert_resource_counts`, `assert_damage` and `assert_composition`, which all reject a
zero-constraint invocation. `{"op":"assert_manager_invariants","managers":[],"cross":["X5"]}`
on a window with no active text selection passes with
`"assert_manager_invariants: 0 key(s)/invariant(s) hold"`. Masked under the default parameter
set (X2/X9 always increment `checked`); a narrowed `cross`/`managers` selection reopens it.

### B5. MEDIUM — `assert_scroll` with neither `x` nor `y` never compares a position

`layout/src/e2e/full.rs:4180-4246`. Both are optional; with neither supplied the function
resolves the selector, confirms the node has *some* scroll state, and returns `pass` (:4243-4245).
`{"op":"assert_scroll","selector":"#foo"}` passes for any scroll offset, including one produced
by a completely broken scroll implementation.

### B6. SUSPECTED — `assert_state_machines_idle` has no liveness precondition

`layout/src/e2e/full.rs:5525-5680`. `assert_idle_stable` and `assert_work_bounded` both require
`frames_since_reset >= 1` before evaluating an absence condition, with an explicit comment:
*"an assertion of absence passes for free when the machinery that would produce the thing never
ran at all."* `assert_state_machines_idle` has no such gate, so on a freshly-mounted window
where no drag/gesture/scroll/focus/edit ever occurred it passes trivially with *"every state
machine settled"*. Whether that is a bug depends on the assertion's intended contract
("is everything idle now" vs "did an interaction end cleanly") — nothing in the runner
enforces the pairing.

### Verified SOLID in the runner (audit coverage)

* **All 99 `DebugEvent` variants** have an explicit arm in `process_debug_event`; enumerated
  programmatically, zero missing. The single remaining `_ =>` (full.rs:13932) is documented as
  provably unreachable and paired with `doc/src/gene2e.rs`'s static "zombie" scanner that
  refuses to generate a test for an unhandled op.
* **All 71 `CallbackChange` variants** have an explicit arm in `Runner::apply_user_change` with
  **no `_` fallback at all** — compiler-enforced exhaustiveness. 16 variants call
  `self.unsupported(…)`, which `unsupported_to_failure()` (runner.rs:2762-2790) turns into a
  hard scenario FAIL that overrides the scenario's own assertions.
* **All 22 `assert_*` evaluators** read in full: each uses `reject_unknown_params` to hard-fail
  on a typo'd key, and an unknown assertion name falls to a loud `fail`.
* `assert_idle_stable` / `assert_work_bounded` correctly require `frames_since_reset >= 1`.
* `assert_changed` / `assert_damage_covers_changes` / `assert_damage_sound` correctly `fail`
  (not skip) on dimension mismatch, missing `cpurender`, or a missing `vs` snapshot.
* `touch_state` **is** now read — `ModifyWindowState`'s `anything_changed` gate
  (runner.rs:1233) includes `touch_state_changed`, with a comment citing the "48 corpus lines
  executed nothing" bug.
* No input op manufactures `needs_update`; state changes are detected via `anything_changed`.
* The 61 committed JSON fixtures use 65 distinct op/assert values, **zero orphaned** against
  the handlers.
* `doc/src/e2erun.rs::run()` refuses to report "0 passed" as green for an empty/typo'd
  selection.
* `RefAnyUndoManager` returns `false` → step fail on a no-op commit, unserializable state, or
  empty history.

### Coverage caveats

The **generated** corpus (~9,500 lines/commit from `scripts/gen_e2e_cases.py` +
`doc/src/gene2e.rs`) is not present as JSON in this checkout, so the "zero orphaned ops"
result covers the 61 committed fixtures only. ~20 of ~90 non-assertion query/mutation
`DebugEvent` handlers were spot-checked rather than adversarially probed.

---

# Part C — secondary workflows, Dockerfiles, `scripts/`

Audited `rust9x.yml`, `dockery.yml`, `docker-base.yml`, `Dockerfile`, `docker/Dockerfile`, and
all 59 shell scripts under `scripts/` (37 top-level + 22 in subdirs), excluding
`build_registry_mirrors.sh`. Corpus totals: **174 `|| true`/`|| :` sites, 367 `2>/dev/null`
redirects, 22 bare `exit 0`, 18 of 37 top-level scripts with no `set -e`.** The large majority
of the `|| true` / `2>/dev/null` hits are **legitimate** (optional `apt`/`brew` installs,
best-effort cleanup, tool discovery — e.g. every `_apt_install`/`_brew_install` helper in
`e2e_language_matrix.sh`) and are not itemised. Only real swallowed-verification cases follow.

**Script-existence sweep (the `build_registry_mirrors.sh` failure mode):** all 14 script paths
referenced from `rust.yml` exist, are git-tracked, and the `.sh` ones are `+x` — verified
individually. `rust9x.yml`, `dockery.yml`, `docker-base.yml` and both Dockerfiles invoke
**zero** scripts from `scripts/` (all inline `run:` blocks), so that failure mode cannot occur
there. Cross-script references inside `scripts/` all resolve.

### C1. HIGH — `dockery.yml` reports two green checks over a build its own Dockerfile says cannot succeed

`docker/Dockerfile:61-63` states in-repo: *"remill's cmake does `find_package(XED CONFIG)` …
Until then the image build fails at this step … the web-base image is experimental and not yet
green."* No `VCPKG_ROOT`/`CMAKE_TOOLCHAIN_FILE` is ever set (`dockery.yml`'s `build-args:`
passes only `AZUL_REF`), and no cxx-common package is installed, so `cmake -G Ninja -B …`
(`docker/Dockerfile:67-70`) reliably fails to configure.

Both jobs then absorb it: `dockery.yml:61` (`build`) and `:122` (`manifest`) are
`continue-on-error: true`, and `manifest` additionally has a graceful "nothing published →
`exit 0`" path at `:143`. **Net effect: this workflow is structurally incapable of failing.**
The only way to learn it is broken is to read the Dockerfile comments. It is a documented
stopgap rather than a hidden hack, but it matches the audit definition exactly.

Note `deploy_pages` *dispatches* this workflow at rust.yml:4166 (`continue-on-error: true`,
`|| echo "::warning::…"`), so the site deploy also cannot observe the failure.

**Fix:** wire `VCPKG_ROOT`/cxx-common so it can succeed, or replace the job-level
`continue-on-error` with a step that runs `docker manifest inspect` and turns "nothing
published" into a visible signal.

### C2. MEDIUM — `strip_staticlib.sh` skips a missing archive without setting `rc`

`scripts/strip_staticlib.sh:86`

```bash
  [ -f "$f" ] || { echo "skip (missing): $f"; continue; }
```

This script is **CI-wired in four places** (rust.yml:526, 1388, 1392, 2637) and its stated job
is to strip embedded ThinLTO bitcode *and* **assert none survived** (rust.yml:1372-1376). A
requested archive that does not exist is skipped silently and does not set `rc=1`. Two of the
call sites compound it:

* `rust.yml:1391` — `[ -f "$A" ] || exit 0` before even calling the script: a missing
  `libazul.a` makes the whole step a green no-op.
* `rust.yml:523-526` — `shopt -s nullglob` drops the `*azul*.a` glob when nothing matches, but
  the literal `target/…/azul.lib` argument survives and is then silently skipped.

**Scenario:** an upstream build step stops producing `libazul.a` for a target. The bitcode
assertion never runs, the archive-size regression it guards (708 MB shipped android `.a`, per
the comment) can recur unnoticed.

**Fix:** a missing input should set `rc=1` unless the caller passes an explicit `--optional`.
The rest of the script is solid — real post-strip byte-scan via `assert_no_bitcode`, correct
`rc` aggregation for `llvm-objcopy` failures.

### C3. LOW — `docker/Dockerfile` prelift diagnostic is dead code

`docker/Dockerfile:133-136`

```dockerfile
RUN AZ_BACKEND="web-prelift://127.0.0.1:0" … /src/azul/target/prod-release/azul-examples 2>&1 | tail -40 \
    || echo "[prelift] harness not present — cache will warm on first real request"
```

No `SHELL [… "-o", "pipefail" …]` is set in this Dockerfile, so the pipeline's status is
`tail`'s (effectively always 0). The `|| echo` fallback — written specifically to report
"harness not present/crashed" — **can never fire**. The stage is documented best-effort and
does not gate the image, so this only silences a diagnostic.

**Fix:** `SHELL ["/bin/bash","-o","pipefail","-c"]` for the stage, or check `${PIPESTATUS[0]}`.

### C4. LOW — five dev scripts print PASS without checking anything (none currently CI-wired)

Verified by grep that **none** of these appear in any workflow file. They are manual tools
today; each is a landmine the moment someone wires it in.

* `scripts/run_memtest.sh` — header says *"Intended for the CI memtest matrix."* If `gdb` is
  absent the segfault check is skipped and **not** counted as a failure (:33-35); `peak_rss_kb()`
  pipes `/usr/bin/time -v … | grep … | head -1` with `set -u` but no `pipefail` (:38-41), so a
  binary that crashes during the large-N run yields an empty RSS reading that falls into the
  "(could not measure RSS)" branch (:56) rather than FAIL. Net: prints `MEMTEST PASS`, exits 0,
  for a binary that segfaults.
* `scripts/test_all_examples.sh:46-51` — greps `cargo build` stdout for `^error` instead of
  checking its exit code; an OOM-killed or `timeout`-killed compile prints no `error:` line and
  is reported PASS. And `:178-182` records a failing `python3 -m py_compile` as **SKIP**, which
  never feeds the `$FAILED` counter that gates the final `exit 1` (:216).
* `scripts/test_dom_inspection.sh` — six checks print red ✗ glyphs on mismatch ("node count
  wrong", "TEXT RENDERING BUG?") but the file contains **no `exit` statement at all**; it always
  ends `Done!` → exit 0.
* `scripts/test_export_code.sh` (the `scripts/` copy — distinct from the CI-gated
  `tests/e2e/test_export_code.sh`) — `:146` prints `=== ALL TESTS PASSED ===`
  unconditionally. Nothing checks that `main.rs` was exported, that the export `status` was
  `ok`, or that the `div` component was found; each just prints a diagnostic and continues.
* `scripts/test_cpp_examples.sh:108,111` and `scripts/build_cpp_examples.sh:177,182` — the
  `set -e` + `((VAR++))` landmine. `((x++))` returns the *pre*-increment value as its exit
  status, so at 0→1 it "fails" and `errexit` kills the script. Reproduced:
  `bash -c 'set -e; x=0; if true; then ((x++)); echo reached; fi'` → exit 1, nothing printed.
  `test_cpp_examples.sh` therefore tests **at most one** `.cpp` file; `build_cpp_examples.sh`
  aborts at the first failing C++ standard directory, silently skipping cpp11..cpp23. Both
  still exit non-zero (fails safe), but neither tests what it claims.
  `scripts/build_all.sh` uses the same idiom and is **not** affected — every `run_build` call is
  wrapped in `|| true`, which suspends `errexit` for the call.

**Fix:** `VAR=$((VAR+1))` in the two C++ scripts; exit-code checks instead of stdout greps;
a failure counter + `exit 1` in the rest.

### Verified SOLID in Part C

* `docker-base.yml` and `dockery.yml` both branch on `github.event_name` **before** consulting
  `github.event.inputs.*` — they do **not** have the NULL-on-`push` bug that skipped
  `deploy_pages`.
* `rust9x.yml` — real post-build assertion (`[ ! -f rustc.exe ] && exit 1`, :84-87) and
  `set -euo pipefail` on the publish step. The one `2>/dev/null || true`
  (`cp bootstrap.rust9x.toml bootstrap.toml`, :71) is backstopped by that assertion.
* `scripts/coverage.sh` — `set -euo pipefail`, and its `run_tests()` helper carries an in-code
  comment documenting a *prior* bug of exactly this class (`… 2>&1 | tail -3` discarded the
  `failures:` block) plus the fix. Generated-test exclusion (the 2.28x inflation) is fixed via
  `--excl-start '^\s*mod autotest_generated\s*\{'` (:195) and `--ignore "*/tests/*"` (:173).
* `scripts/e2e_language_matrix.sh` (2,244 lines) — repeatedly self-documents and fixes this bug
  class: an EPIPE false-positive from `echo | grep -q` (:363-367), an `if`-masks-`$?` timeout
  bug (:2037-2041), retry-once for flaky shipped bindings, `record()`-or-default-to-FAILS
  sidecars. Structurally the strongest hardening in the repo — which is what makes the
  `SKIP`-never-gates hole (§3) stand out.
* `scripts/cross_check.sh`, `scripts/probe_az_debug.sh`, `scripts/mobile-check-all.sh` —
  correct exit-code aggregation and real pass/fail tallies.
* `azul-doc check` (`doc/src/main.rs:302-303`) — `anyhow::bail!("azul-doc check failed: {}
  problem(s)")`. Correct, and a direct counter-example to `azul-doc reftest` (§1) in the same
  binary.
* `azul-doc autoreview autodoc-check --strict`
  (`doc/src/reftest/autodoc.rs:1306-1309`) — returns `Err` when strict and stale. Correct.
* `deny.toml` — `[advisories]` leaves vulnerabilities at deny, and the two `ignore` entries are
  unmaintained-only with written justifications and fix paths. So §5's `|| true` really is
  suppressing a live gate, not a pre-neutered one.

---


# Part D — Rust test suite

~14,380 test functions screened across `css/`, `core/`, `layout/` (excl. `text3`), `dll/`,
`doc/`, `tests/`, `e2e/`, `tools/`, `examples/`. Findings below are the ones I re-verified
directly. **The dominant theme is the same as §2: tests that exist but never execute.**

### D1. HIGH — `azul-doc`'s entire test suite has never run

Grepped every workflow and `scripts/*.sh` for `cargo test`/`nextest` naming `azul-doc`:
**zero hits.** All 39 `azul-doc` CI invocations are `cargo run -r -p azul-doc <subcommand>` or
`cargo build --release -p azul-doc` — the binary, never the tests. `doc/src` contains
**192 `#[test]` sites** (≈180 real after excluding ones inside generated-code string literals
and doc comments), the largest being `doc/src/gene2e.rs` (41) and
`doc/src/autofix/type_index.rs` (22).

`azul-doc` is the codegen + reftest + e2e driver — the tool that produces `azul.h`, every
language binding, and the reftest report. Its own tests have never been executed.
`doc/Cargo.toml` declares no `[features]`, so a `cargo test -p azul-doc --lib` step is a
one-liner with no feature plumbing.

### D2. HIGH — three `DISABLED_*` test files compile to empty 0-test binaries and report `ok`

`layout/Cargo.toml:321-323` declares three features that are enabled **nowhere** (verified by
grep across all `*.toml`, `*.yml`, `*.sh` — only the declarations and one comment):

```toml
DISABLED_event_tests = []
DISABLED_hint_vs_freetype = []
DISABLED_selection_tests = []
```

Each gates an entire file via a crate-level attribute, e.g.
`layout/tests/test_hint_vs_freetype.rs:1`:

```rust
#![cfg(feature = "DISABLED_hint_vs_freetype")]
```

The sting: `test_hint_vs_freetype` is a registered `[[test]]` target
(`layout/Cargo.toml:341-343`) with **no `required-features`**. So cargo does *not* skip it —
it **builds it**, the crate-level `cfg` evaluates false, and the result is a test binary
containing zero tests that prints `running 0 tests … test result: ok`. That is a green line in
the output for 36 disabled tests.

| file | tests | stated reason |
|---|---:|---|
| `layout/tests/test_hint_vs_freetype.rs` | 36 | font-hinting vs FreeType parity |
| `layout/tests/selection.rs` | 5 | *"disabled pending API export"* |
| `layout/tests/event_determination.rs` | 2 | *"disabled pending API export"* |

**Fix:** export the missing functions/types and re-enable, or delete. In the current state they
provide zero signal in either direction while looking like coverage.

### D3. MEDIUM — `tests/src/layout.rs`: 37 tests orphaned by a missing `mod`

`tests/src/lib.rs` wires every sibling explicitly:

```rust
mod css; mod css_parser; mod dom; mod font_gc; mod layout_test;
mod script; mod text_layout; mod ui; mod word_wrap; mod xml;
```

There is **no `mod layout;`** — but `tests/src/layout.rs` exists and contains **37 `#[test]`
functions**. (`layout_test` is a different file, `layout-test.rs`.) The file is never compiled,
so there is not even a dead-code warning. `tests/` is also `exclude`d from the root workspace
(`Cargo.toml`), so nothing in CI builds this crate at all.

A second instance: `tests/test_xml_inline_parsing.rs` (2 tests) sits at the crate root rather
than `tests/tests/`, where cargo auto-discovers integration targets, so it is never picked up;
one of the two is additionally behind a `#[cfg(feature = "xml")]` that `tests/Cargo.toml` never
declares (always false — feature-namespace confusion with azul-layout's `xml`).

### D4. MEDIUM — a CI-live test whose only assertion is a tautology, in a file that re-implements the function under test

`dll/tests/kitchen_sink_integration.rs` — run by CI at rust.yml:1184 **and** by
`export_code_e2e`. Line 13-24 defines a **local copy** of the function under test:

```rust
    /// Simulate the compile_to_rust function from kitchen_sink.rs
    fn compile_to_rust(xml_content: &str) -> String {
        let parsed = match parse_xml_string(xml_content) {
            Ok(parsed) => parsed,
            Err(e) => {
                return format!(
                    "// Error parsing XML:\n// {}\n\nfn main() {{\nprintln!(\"XML Parse \
                     Error\");\n}}",
                    e
                );
            }
        };
```

The error template **contains `fn main()`**. So the assertion at `:103` —

```rust
        assert!(result.contains("Error parsing XML") || result.contains("fn main()"));
```

— is true in **every** possible outcome: on success the generated code has `fn main()`; on a
parse error the string has both. It is the test's only assertion, so a `compile_to_rust` that
unconditionally returned its own hard-coded error string would pass. `:85`'s
`assert!(result.contains("main") || result.contains("Class"))` is the same shape (mitigated —
that test has one other real assertion), and `:153` is `assert!(true, "XML compilation requires
'xml' feature")`.

Compounding it: because the helper is a *simulation*, the real `kitchen_sink.rs`
`compile_to_rust` is not exercised by this file at all.

### D5. MEDIUM — 49 `dll/` tests never compile in CI (feature/OS gates)

CI runs exactly two dll test invocations, both on `ubuntu-22.04`: `cd dll && cargo test`
(default features) and `cargo test -p azul-dll --test xml_to_rust_compilation --test
kitchen_sink_integration --features xml`. Tracing each gate to its `mod` declaration site:

| gate | modules (tests) |
|---|---|
| `web` | `web/config.rs`(16), `web/server.rs`(5), `web/hydration.rs`(3), `web/symbol_table.rs`(5) |
| `pdf` | `extra/pdf/mod.rs`(3) |
| `map-tiles` | `extra/map/mvt.rs`(4), `extra/map/svg.rs`(3) |
| `video-native` | `extra/video_codec/{demux,pipeline}.rs`(4) |
| `target_os="windows"` | `shell2/windows/{mod,dlopen}.rs`(2) |
| `target_os="macos"`/`ios` | `shell2/macos/{mod,events}.rs`(2), `videotoolbox.rs`(1) |

**49 of dll's ~180 tests (27%) never compile** — strictly worse than `#[ignore]`, which at
least reports as "ignored". This is also the mechanism behind §14
(`dll/tests/leak_regression.rs`).

**Same mechanism in `azul-core` [verified]:** `core/src/url.rs` has 38 `#[test]` fns, of which
**~20 sit behind `#[cfg(feature = "url")]`**. `azul-core`'s default is `["std"]` only
(`core/Cargo.toml:37`); `url` reaches it solely through `azul-layout/http`
(`layout/Cargo.toml:288`), which is **not** in azul-layout's defaults. `test_lib` passes
`--features azul-core/serde-json,azul-layout/json` — neither pulls it in. So half of the
`Url::parse`/`Url::join` suite never compiles in the blocking gate.

### D6. MEDIUM — 10 `layout/tests/` files (≈185 tests) never call real azul code

Every one builds local literals and asserts against its own hand-rolled arithmetic, never
invoking the `azul_layout`/`azul_core` API the filename claims to cover. Representative:

* `layout/tests/caption_positioning.rs:174` —
  `let caption_count = if has_caption {1} else {0}; assert!(caption_count <= 1); assert_eq!(caption_count, 1);`
  — asserts on a value it hard-coded two lines earlier.
* `layout/tests/regression_font_size_bugs.rs` (8 tests) — regression tests for three *named,
  real* production bugs (`core/src/prop_cache.rs::append()`,
  `layout/src/solver3/getters.rs::get_style_properties()`) that **never call the buggy
  functions**; verification is hand arithmetic on `NodeId`. If either bug regressed, none of the
  8 would notice.
* `layout/tests/h1_p_margin_collapse.rs` — defines a **local duplicate** `fn collapse_margins()`
  instead of importing the real `azul_layout::solver3::fc::collapse_margins`, which the sibling
  file `margin_collapsing.rs` correctly tests.
* `layout/tests/table_width_and_alignment.rs` — comments say *"Simulate the fixed algorithm"*;
  one test is `assert_ne!(20.0, 8.0)`.
* Also: `anonymous_nodes.rs` (31), `empty_cells.rs` (18, one with no runtime assertion at all),
  `visibility_collapse.rs` (37, operates on a local `HashSet<usize>`), `window_tests.rs` (7),
  `test_text_layout.rs` (3, builds a real `StyledDom` then only `eprintln!`s it),
  `table_cell_width.rs::test_table_auto_width_fills_parent` (runs real layout, then only
  `eprintln!`s the rects despite a doc comment claiming to verify the width).

These are currently invisible because of §2 — they don't run. Fixing §2 will turn them red or
reveal them as no-ops, which is the point.

### D7. MEDIUM — zero-assertion tests concentrated in a few files

* `css/tests/test_parser_robustness.rs` — **35 of 44 tests** parse CSS then `let _ = result;`
  with no assertion on rule count, values, or warnings, despite the file's doc comment claiming
  to verify warnings (e.g. :9-14, :65-70, :205-215, :406-432).
* `css/tests/test_system_style.rs` — the entire file is `println!`s and a `match` whose both
  arms only print. **Zero `assert!`/`assert_eq!` anywhere**, even with the `io` feature on.
* `core/tests/cascade.rs:196,209` — `if let PATTERN = val { println!(…) }` with no `else`, so a
  wrong variant silently no-ops; `:222` only `println!`s the expected result.
* `core/tests/events.rs:53,435`; `core/tests/refany.rs:158`;
  `core/tests/reconciliation/fingerprint.rs:348` (name promises non-zero, body never asserts
  it); `core/tests/reconciliation/{state_preservation.rs:128,text_reconciliation.rs:184,217}`
  (`let _ = …; // Just verify no panic`);
  `core/tests/{dom_constructor.rs:279,dom_a11y.rs:220,dom_manipulation.rs:391}`
  (self-documented *"passes if it compiles"*).
* `core/src/styled_dom.rs:362` `test_css_styling_with_nested_divs` — builds real CSS + DOM,
  calls `add_component_css`, discards into `_styled_dom`, asserts nothing. **This one is in
  `--lib`, so it does run.**
* `dll/src/desktop/extra/video_codec/mod.rs:576-602` `screen_recorder_smoke` — has real
  assertions, but all of them sit *after* an early `return` that fires when gstreamer is
  unavailable. CI's `apt-get` never installs gstreamer, so **every CI run takes the
  early-return path and silently skips every assertion**, reporting as passed (unlike this
  file's sibling hardware tests, which are honestly `#[ignore]`d).

### D8. MEDIUM — `menubar_item_clip.rs`: missing non-empty guard, proven by its own siblings

`layout/tests/menubar_item_clip.rs`, `test_real_menubar_widget_not_clipped` (~:46-126) and
`test_probe_words_glyph_counts` (~:252-330):

```rust
    for w in &item_widths { assert!(*w > 30.0) }
```

with no check that `item_widths` is non-empty — a layout pass that produces **zero** matching
nodes passes vacuously. That this is an oversight rather than a style choice is proven inside
the same file: two *other* tests add `assert_eq!(item_widths.len(), 3, …)` immediately after the
identical collection step.

### D9. LOW — `catch_unwind` whose `Err` arm only logs

Three near-identical sites where the `Err` arm of a `catch_unwind` match is `eprintln!` rather
than `panic!`/`assert!`, so a regression from "safely rejects" to "crashes" still passes:
`layout/src/widgets/screencap.rs:1251-1267`, `layout/src/widgets/capture_common.rs:973-994`,
`layout/src/widgets/video.rs:2091-2110`. Three more use `catch_unwind` as the *entire* check for
a degenerate-range case with no companion value assertion
(`layout/src/widgets/slider.rs:1287-1308,1453-1472`,
`layout/src/widgets/number_input.rs:1575-1603`) — a rewrite that returns garbage instead of
clamping passes.

### D10. LOW — `unwrap_or` masking a broken path in test helpers

* `layout/src/widgets/drop_down.rs:579-584` and `layout/src/widgets/menubar.rs:339-344` — an
  identical `take_changes()` helper swallows a poisoned mutex into an empty `Vec`,
  indistinguishable from "genuinely no changes".
* `layout/src/cpurender/raster.rs:3593` — `parsed.lookup_glyph_index(c).unwrap_or(0)` in a
  pixel-diff helper: a font whose lookups all fail renders `.notdef` for every glyph instead of
  failing loudly.

### D11. LOW — phantom cargo features gating real code

`core/src/styled_dom.rs:1181` gates on `#[cfg(feature = "table_layout")]` — a feature declared
in **no `Cargo.toml` in the repo** (verified: grep of `core`, `layout`, `dll` manifests returns
nothing). The gated block calls `crate::dom_table::generate_anonymous_table_elements`, a module
that **does not exist** under `core/src`. It compiles only because the cfg is permanently false.
`core/src/dom.rs:5778,5789` similarly gate on `"xml"`, which `core/Cargo.toml` never declares.

Not a false-green by itself, but it is dead code that reads as a feature, and the day someone
adds a `table_layout` feature the crate stops compiling.

### Verified CLEAN in Part D — negative results worth recording

* **Upper-bound-only assertions: essentially clean.** Two agents sampled ~67 of ~120 bound-check
  candidates across `solver3`, `managers`, `cpurender`, `widgets`. **No bare "checks a max and
  nothing else" instance found** — every bound was paired with an exact-value companion or was a
  deliberate degenerate-input guard. Sole exception: `dll/src/web/hydration.rs:186-192`
  (`assert!(bytes.len() < 1024)`, no lower bound) — moot, since the `web` feature is never
  enabled in CI (D5).
* **`assert_eq!(x, x)` self-comparisons: 59 raw hits, verified legitimate.** Manually sampled
  14+ across 10 files; every one is a deliberate algebraic-property test (Eq/Ord reflexivity
  under NaN, getter idempotence), and in each case a companion correctness test sits in the same
  file. `layout/src/window_state.rs:467` even carries
  `#[allow(clippy::eq_op)] // "a == a" IS the reflexivity check being asserted`. This is a
  disciplined pattern, not a hack. Four are the *entire* test with no nearby correctness
  companion and merit a second look, no more:
  `css/src/props/property.rs:8664`, `core/src/profile.rs:726`, `core/src/lib.rs:614`,
  `core/src/geom.rs:871`.
* **`.len() >= 0` (always-true unsigned): zero hits workspace-wide.** `|| true` in Rust source:
  zero hits. `assert!(true)`: only the 2 deliberate feature-off stubs in `dll/tests/` plus 2 in
  vendored `webrender/`.
* **`catch_unwind` at the FFI/production boundary is clean** — `core/src/{refany,host_invoker,
  style,id,db}.rs`, `dll/src/desktop/{wr_translate2,shell2/*/accessibility}.rs`,
  `dll/src/web/server.rs` all pair it with concrete value assertions on the `Ok` branch.
* **The `*_never_panics` family** across `css/src/props/*`, `core/src/{transform,geom,gl,dom,
  xml,resources}.rs`, `layout/src/{headless,solver3,icu_macos,probe,font,http}.rs` genuinely
  fails on panic and is consistently named/commented as testing exactly that. Disclosed, not
  hidden. Same for `core/src/path_parser.rs:1938`, whose comment states *"The assertion is
  termination itself."*
* **`layout/tests/e2e_json.rs` is exemplary** — it asserts `!tests.is_empty()` with the comment
  *"an empty selection is a broken path, not a green run."* That is the defence this whole audit
  is looking for, already implemented correctly.
* **`examples/`** — only 3 test fns (`examples/azul-paint/src/lib.rs`), all with real content and
  explicit negative controls. **`e2e/` and `tools/`** — zero Rust test functions (JSON scenarios
  and shell scripts only), correctly out of scope.
* Bulk of `layout/src/{solver3,managers,widgets,cpurender,xml}` and
  `layout/tests/{table_layout,margin_collapsing,list_marker_counter,session_regression,
  resize_relayout_bug,image_flex_grow}.rs` — sampled broadly, consistently real
  positive-and-negative-control assertions. **The failures are concentrated in specific files,
  not spread evenly.**

---

# Part E — engine: silent fallbacks and dead state

Scope: `css/src`, `core/src`, `layout/src` (excl. `text3`), `dll/src`, `doc/src`. Volumes
screened: ~1,043 wildcard match arms across ~15-26 command/event enums; ~148
`validate_*`/`check_*`/`ensure_*`/`verify_*` definitions; ~50-110 discarded `Result`/`Option`
sites; ~50-86 dirty-flag sites. 13 confirmed; the ones I re-verified myself are marked.

### E1. HIGH — `dispatch_pending_lifecycle_events`' return value is discarded at all 7 call sites **[verified]**

`dll/src/desktop/shell2/common/event.rs:4150`:

```rust
    fn dispatch_pending_lifecycle_events(&mut self) -> bool {
```

Its own doc comment: *"Returning `true` means at least one callback reported
`Update::Refresh(Dom)` and the caller should regenerate again."* I enumerated every call site —
**all seven throw it away**:

```
dll/src/desktop/shell2/android/mod.rs:210        let _ = self.dispatch_pending_lifecycle_events();
dll/src/desktop/shell2/windows/mod.rs:1419       let _ = …
dll/src/desktop/shell2/ios/mod.rs:1199           let _ = …
dll/src/desktop/shell2/macos/mod.rs:4238         let _ = …
dll/src/desktop/shell2/linux/x11/mod.rs:3376     let _ = …
dll/src/desktop/shell2/linux/wayland/mod.rs:3619 let _ = …
dll/src/desktop/shell2/headless/mod.rs:1009      self.dispatch_pending_lifecycle_events();   // bare
```

**Scenario in which it passes while broken:** a `Mount`/`AfterMount` callback that
synchronously seeds derived state and returns `Update::RefreshDom` — a common pattern — has its
second-regeneration request silently dropped **on every platform**. The UI only catches up when
an unrelated later event happens to trigger another pass. `dll/tests/headless_lifecycle.rs`
covers that the events *reach* callbacks, not that a returned `Refresh` causes a re-regen, so
nothing detects this.

**Fix:** `if self.dispatch_pending_lifecycle_events() { self.regenerate_layout()?; }` at each
site, or make the function do the re-regen itself and return `()`.

### E2. HIGH — `run_keyboard_default_action` drops 5 of 10 `DefaultAction` variants in the e2e runner

`layout/src/e2e/runner.rs:2402-2426`, matching `DefaultAction` (`core/src/events.rs:987`,
10 variants):

```rust
    match &action.action {
        DefaultAction::FocusNext | FocusPrevious | FocusFirst | FocusLast => { … }
        DefaultAction::ClearFocus => { … }
        _ => (ProcessEventResult::DoNothing, false),
    }
```

Silently drops `ActivateFocusedElement`, `ScrollFocusedContainer`, `SubmitForm`, `CloseModal`,
`SelectAllText`. The first two are **actively produced** by `determine_keyboard_default_action`
(`layout/src/default_actions.rs`) and **explicitly handled in production** at
`dll/src/desktop/shell2/common/event.rs:5109` (`ActivateFocusedElement`) and `:5113`
(`ScrollFocusedContainer`).

**Scenario in which it passes while broken:** any e2e scenario pressing Enter/Space on a focused
button, or Arrow/PageDown/Home/End on a focused scroll container, silently no-ops in the
headless harness while the real app activates/scrolls. **The production code path for those two
variants has zero e2e coverage — delete it and every scenario still passes.** This is the same
class as the retired `touch_state` bug, one level up.

**Fix:** make the match exhaustive (as `apply_user_change` already is — see Part B SOLID) and
route the unhandled variants through `self.unsupported(…)`, which `unsupported_to_failure()`
already turns into a hard scenario failure.

### E3. HIGH — `focus.pending_focus_request` is fully dead, and its e2e assertion can never fail **[verified]**

`layout/src/managers/focus_cursor.rs:57` — `request_focus_change()` (`:100`) and
`take_focus_request()` (`:105`) are called **only** from that file's own `#[cfg(test)]` module
(test mod begins at `:531`; all call sites are at `:725-742`). Zero production callers.
Real focus changes bypass the queue entirely via `CallbackChange::SetFocusTarget` →
`FocusManager::set_focused_node()`.

The one non-test reader is `layout/src/e2e/full.rs:5601`:

```rust
    if lw.focus_manager.pending_focus_request.is_some() {
        …fail: "focus_manager.pending_focus_request is still Some — a focus change was queued and …"
```

**This is a textbook absence-assertion with no positive control:** the field is never `Some` in
production, so the check passes unconditionally and forever. It reads as coverage of the focus
queue and is worth exactly nothing.

**Fix:** either wire `request_focus_change` into the real focus path, or delete the field, the
two methods and the e2e assertion together.

### E4. MEDIUM — `gesture.pad_state`: the gap moved rather than closed **[verified]**

Read side is now wired: `layout/src/callbacks.rs:3322`
(`CallbackInfo::get_wacom_pad()` → `get_gesture_drag_manager().get_pad_state().copied()`).
But I enumerated every `update_pad_state` call site and there is exactly **one**:
`layout/src/managers/gesture.rs:3069` — inside that file's `#[cfg(test)]` module (which begins
at `:1771`). **No Windows/macOS/Linux tablet backend ever feeds real data in.**

Net effect: a public, documented, callable API (`get_wacom_pad()`) that returns `None` forever
on every platform — indistinguishable from "no pad connected", so no user or test can tell the
feature is unimplemented.

**Fix:** call `update_pad_state` from the platform pad backends, or mark `get_wacom_pad()`
clearly unimplemented so callers do not silently branch on a permanent `None`.

### E5. MEDIUM — `determine_events_from_managers`: zero production callers, and its only integration test is one of the permanently-disabled files **[verified]**

`layout/src/event_determination.rs:66`. All 11 references: the definition, its own
architecture-diagram comment (`:42`), 8 of its own unit tests (`:1753-1843`), and
`layout/tests/event_determination.rs:68` — **which is `DISABLED_event_tests` from §D2 and never
compiles.**

Its own top-of-file diagram (`:24-45`) documents it as *the* production pipeline step
(`Platform Input → … → determine_events_from_managers() → … → dispatch_events()`). That is
false: production calls **`determine_all_events`** (`:268`, from
`dll/src/desktop/shell2/common/event.rs:4343`), a fully separate reimplementation whose body
never calls `determine_events_from_managers`.

So there are two independent event-determination implementations; the one with the
architecture diagram and 8 green unit tests is the one nothing runs. Those passing tests are
active misinformation about a path production never takes.

### E6. MEDIUM — `check_properties_changed` / `check_layout_properties_changed` have zero callers, and a doc comment claims otherwise **[verified]**

`core/src/prop_cache.rs:2404` and `:2421` (both `pub(crate)`). Every other reference is one of
their own unit tests at `:5213-5267`. Zero production callers.

Worse, `layout/src/callbacks.rs:573` states: *"the property cache already consults it in
`check_layout_properties_changed`"* — **false today**. The real relayout gate on
viewport/breakpoint changes is `viewport_breakpoint_changed()`
(`css/src/dynamic_selector.rs:1020`), called identically from all four desktop platforms, and
it does a coarse **unconditional full relayout** on any breakpoint crossing. The targeted,
property-type-filtered optimisation these two functions implement is entirely unused, while a
comment tells the next reader it is load-bearing.

### E7. MEDIUM — macOS native text input drops `RegenerateLayoutIncremental`

`dll/src/desktop/shell2/macos/events.rs:723-736`, `handle_text_input` — the real
`NSTextInputClient::insertText:` / IME-commit path (called from `macos/mod.rs:1283` and `:1951`):

```rust
    match event_result {
        EventProcessResult::RegenerateDisplayList => { self.common.frame_needs_regeneration = true; … }
        EventProcessResult::UpdateDisplayList     => { self.common.display_list_dirty = true; … }
        EventProcessResult::RequestRedraw         => { self.request_redraw(); }
        _ => {}
    }
```

`RegenerateLayoutIncremental` is reachable here: `overall_result` becomes
`ProcessEventResult::ShouldIncrementalRelayout` at `events.rs:711` whenever
`apply_text_changeset()` reports `needs_relayout`, and `convert_process_result` maps that to
`RegenerateLayoutIncremental` (`events.rs:110`). Every one of the ~15 *other* sites matching
this enum in `macos/mod.rs` (`:197, 229, 256, 283, 310, 456, …`) calls
`self.apply_incremental_relayout_result()` for that arm. `handle_text_input` is the only one
that does not.

**Scenario:** typing into a content-sized field (auto-grow textarea, or any node sized by its
text) via native macOS input never re-runs layout; the field's visible size updates only when an
unrelated event later triggers a pass.

### E8. MEDIUM — X11 swallows `regenerate_layout()` failures on resize and DPI change

`dll/src/desktop/shell2/linux/x11/mod.rs:2660` (`ConfigureNotify`/resize) and `:2753` (DPI
change) both do `self.regenerate_layout().ok();`. In the **same file**, the per-frame path at
`:3491` does:

```rust
    if let Err(e) = self.regenerate_layout() {
        return Err(WindowError::PlatformError(format!("Layout failed: {}", e)))
    }
```

for the identical call. So X11 escalates loudly in one place and silently discards in the two
places most likely to produce a genuinely different layout. Also `:3170`
(`apply_size_to_content`, menu/tooltip auto-sizing) does `let _ = self.regenerate_layout();` and
then immediately reads the "natural content size" off whatever `layout_tree` survives — a failed
regen silently sizes a menu or tooltip from stale data, with no log anywhere.

### E9. MEDIUM — macOS native Cmd+Z / Cmd+Shift+Z updates state but not pixels

`dll/src/desktop/shell2/macos/mod.rs:6755` (`perform_undo`) and `:6776` (`perform_redo`) — the
`NSResponder` `undo:`/`redo:` selector handlers:

```rust
    let _ = self.apply_system_change(&SystemChange::UndoTextEdit { target });
    unsafe { msg_send![&*self.window, setViewsNeedDisplay: true]; }
```

`SystemChange::UndoTextEdit`/`RedoTextEdit` (`common/event.rs:2899-2954`) return
`ProcessEventResult::ShouldUpdateDisplayListCurrentWindow` on success, which is what sets
`display_list_dirty`. Discarding it bypasses `apply_activation_pass_result()` (`mod.rs:6586`),
substituting a bare AppKit `setViewsNeedDisplay` that azul's own render-decision logic
(`mod.rs:5975/5984`, which gates regeneration on `self.common.display_list_dirty`) never looks
at. Contrast the sibling `edit_command()` (`mod.rs:356`, driving the Edit-menu Undo and
Cut/Copy/Paste/SelectAll), which correctly calls `apply_activation_pass_result(result)` for the
identical `SystemChange`.

**Scenario:** native keyboard Undo/Redo updates the styled DOM and selection but the visible
pixels do not refresh until some unrelated event happens to set `display_list_dirty`.

### E10. LOW — headless `frame_needs_regeneration` is permanently `true`

`dll/src/desktop/shell2/headless/mod.rs` — set `true` at construction (`:894`) and
unconditionally at the end of **both** `regenerate_layout()` (`:1046`) and `relayout_only()`
(`:1119`), with **no read and no clear anywhere in the file**. Every interactive backend
(`run.rs:1173/1181`, `android/mod.rs:237/410`, `ios/mod.rs:174/1226`,
`windows/mod.rs:696/2830-2836`, macOS via `events.rs:5975-5993`) reads this exact field to
decide whether to regenerate/present, then clears it. Headless is the only implementation that
writes and never consumes it.

Not an active false pass today — the e2e harness correctly uses the separate
`frame_report.dom_regenerations` counter. But it is a standard-named field that is
tautologically `true` from frame one: the next engineer who reaches for
`frame_needs_regeneration` in a headless assertion gets an always-true answer.

### E11. LOW — dormant-but-advertised `DefaultAction` arms

`dll/src/desktop/shell2/common/event.rs:5171-5174`:

```rust
    DefaultAction::SubmitForm { .. } |
    DefaultAction::CloseModal { .. } |
    DefaultAction::SelectAllText => {
        // Placeholder for future implementation
    }
```

No active loss today (the producer never constructs `SubmitForm`/`CloseModal` — its own comments
say *"For now, no action"* / *"Could close modal here"*), but the public `DefaultAction` enum's
doc comments advertise Enter-submits-form and Escape-closes-modal as implemented, and this
exhaustive-looking match compiles cleanly either way. Whoever wires the producer will find the
result vanishes here.

### E12. LOW — zero-caller tooling gates in `doc/`

* `doc/src/print.rs:429` `validate_class_definition` — `pub fn`, zero callers repo-wide.
* `doc/src/reftest/regression.rs:553` `ensure_chrome_references` — zero callers;
  `process_commit` (the real regression-pipeline entry) never calls it. A new test file added
  without a manually-generated Chrome reference PNG silently never gets one — which compounds §1.

### SUSPECTED (not traced to a conclusion)

* `GeolocationManager.last_error` (`layout/src/managers/geolocation.rs:78`) is written for real
  (`dll/src/desktop/shell2/common/capability_pump.rs:85`) but has no `CallbackInfo` accessor —
  only `get_location_fix()`, no `get_location_error()`. The struct's own doc comment
  (`:86-89`) says *"Layout-internal (not FFI-exposed) until the `CallbackInfo` getter lands with
  the Phase C geolocation item"* — a **documented pending feature**, not a hidden bug. Worth
  flagging only because today an app's `GeolocationError` callback fires with no way to read why.
* `MacOSEvent::from_nsevent(&event)` allegedly computed on every native Cocoa event
  (`macos/mod.rs:6518`, `run.rs:699-700`) but never read inside `process_event`. Not re-verified.
* `layout/src/solver3/display_list.rs` `get_paint_rect(…).unwrap_or_default()` at
  `:2700/2771/2838/2889/2907/2954/3014` — not traced far enough to say whether `None` can occur
  for a node that legitimately has a rect.

### Verified SOLID in Part E

* **Exhaustive, no-catch-all matches** on the enums that matter: `CallbackChange` in
  `apply_user_change` (both `dll/src/desktop/shell2/common/event.rs:1402` and its e2e port
  `layout/src/e2e/runner.rs:1196`, ~71 variants each — compiler-enforced); `SystemChange` in
  `apply_system_change` (`event.rs:2717`, self-documented *"compile error on missing variant"*);
  `AccessibilityAction` dispatch (`layout/src/window.rs:5403`, 20 variants); `KeyringRequest`
  across all 5 platform backends; `ThreadReceiveMsg`; the
  `ProcessEventResult`→`EventProcessResult` conversion.
* **`text_edit_manager.display_list_dirty` — FIXED.** Cleared unconditionally after every DL
  rebuild (`layout/src/window.rs:1973` and `:6630`), with a comment documenting the old latch
  bug verbatim. Its only reader is the e2e regression guard (`e2e/full.rs:5585`) that it never
  sticks — a diagnostic consumer rather than a functional one, but it *is* read. One of the
  three named precedents, genuinely resolved.
* **Invalidation is not manufactured** on the paths that matter: `apply_user_change`'s
  `ModifyWindowState` arm gates `mark_frame_needs_regeneration()` behind `anything_changed`, a
  real 7-field diff; `scroll_state.rs`'s `needs_repaint` only fires inside
  `if let Some(anim) = …`; `check_scrollbar_change` (`solver3/cache.rs:2235`) is a real
  before/after diff, unit-tested in both directions.
* **Write-only sweep came back clean** for `scroll_state.pending_wheel_event`/`needs_repaint`,
  `gpu_state.scrollbar_fade_active`, `text_input.pending_changeset`,
  `focus_cursor.pending_contenteditable_focus`, `geolocation.active_config`/`latest_fix`,
  `sensors`/`biometric`/`keyring.pending_event`, `permission.last_subscriber`.
* **Zero-caller sweep came back clean** (real callers found) for `ensure_primary_valid`,
  `check_if_scrollable`, `ensure_layout_window_initialized`, `ensure_chains_nonempty`,
  `check_scrollbar_change`, `check_reinvoke_condition`, `verify_nodetype_match`,
  `check_mouse_button`, `check_if_value_is_css_var`, `check_and_queue_virtual_view_reinvoke`,
  `validate_field_name`/`validate_exported_field(s)`, `check_fluent_syntax(_bytes)`.
* **Discarded-`Result` sweep clean** for platform `dlopen` probes (always followed by
  `if let Some`), debug-server socket tuning, and `RefAny` refcount CAS discards.
* `core/src/gl.rs:1201/1217` `check_shader_compile`/`check_program_link` are zero-caller but
  carry explicit `#[allow(dead_code)]` + *"retained for diagnostics"* — documented, not silent.
* `AccessibilityAction::ShowTooltip | HideTooltip` and `CustomAction`
  (`layout/src/window.rs:5796-5802`) are no-ops but each carries an explicit `// TODO` — a
  documented gap, not a hidden one.

### Not covered

~120 of the 148 `validate_*`/`check_*` definitions outside the high-risk keyword sample; the
bulk of `.unwrap_or_default()` in `layout/src/window.rs` / `callbacks.rs` (~30, not individually
traced); `layout/src/text3` (excluded — another agent held it).

---
