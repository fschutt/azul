#!/usr/bin/env bash
#
# scripts/check.sh — run, locally, exactly what CI's cheap (pre-E2E) gates run.
#
# WHY THIS EXISTS
# ---------------
# The obvious local commands are a strict SUBSET of what CI runs, and the
# difference is silent:
#
#   * `cargo test -p azul-layout --lib` runs ~7285 tests and prints SUCCESS
#     while compiling NONE of `layout/src/e2e/` — the whole module is
#     `#[cfg(feature = "e2e-server")]` (layout/src/lib.rs). 19 tests, including
#     the manager-accounting gates, vanish with no diagnostic.
#   * `cargo test -p azul-layout` silently SKIPS `--test e2e_json`, because that
#     target declares `required-features = ["e2e-server"]` and cargo skips an
#     unsatisfiable target with exit code 0 and no warning.
#   * `cargo test -p azul-dll --lib` costs ~1m40s of build before the first
#     assertion, so it is the one people drop from their loop — and it holds the
#     48 headless-backend tests.
#
# Running THIS script is the local equivalent of "will the cheap half of CI go
# green". It never skips a stage silently: every stage prints PASS/FAIL/SKIP
# with a reason, and the final verdict lists them all.
#
# USAGE: see --help.
#
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

# --------------------------------------------------------------------------
# Stage registry.  name|tier|description
#   tier fast = in the default run (target: under ~5 min warm)
#   tier slow = only with --slow / --only (minutes of azul-dll build)
# Ordered fastest-time-to-first-failure first.
# --------------------------------------------------------------------------
STAGES=(
  "contracts|fast|release contracts: naming, widget wiring, sparse checkout (preflight job)"
  "arch-lint|fast|shell2 architecture greps (content_state_lint job)"
  "member-coverage|fast|no workspace member may join untested (check_crates job)"
  "check|fast|cargo check azul-css / azul-core / azul-layout"
  "clippy|fast|clippy -D warnings on core/css/layout --all-targets"
  "doc-check|fast|azul-doc check (double-drop invariant + guide links)"
  "binding-syntax|fast|syntax/type-check generated bindings: zig, go, ocaml, ruby, lua, php, node"
  "doc-tests|fast|cargo test -p azul-doc --bins"
  "unit-tests|fast|css+core+layout+webrender --lib, WITH the feature gates CI uses"
  "css-io|fast|--test test_system_style --features io (required-features gate)"
  "integration-tests|slow|css+core+layout+webrender --tests (93 targets; all.rs is 28m)"
  "e2e-json|slow|--test e2e_json (required-features = e2e-server)"
  "dll-tests|slow|cargo test -p azul-dll --lib --features build-dll"
  "dll-default|slow|cd dll && cargo test (the ubuntu lint_and_check job)"
  "leak-regression|slow|--test leak_regression (macOS-only cfg)"
)

usage() {
  cat <<'EOF'
scripts/check.sh — mirror CI's cheap gates locally.

  ./scripts/check.sh                  run every FAST stage (target: <5 min warm)
  ./scripts/check.sh --slow           fast stages PLUS the slow tier (integration + azul-dll)
  ./scripts/check.sh --only NAME[,..] run just these stages (fast or slow)
  ./scripts/check.sh --list           print the stage table and exit
  ./scripts/check.sh --keep-going     run every stage even after one fails
  ./scripts/check.sh --help

STAGES (measured warm, on an M-series Mac)
  fast tier (the default run, and it is the WHOLE default).
  ~4.5 min in TRUE steady state (nothing changed since the last run). The
  first run after switching feature sets — including a `--slow` run right
  before this one — adds ~3 min of azul-layout rebuild; see FEATURE THRASH.
    arch-lint         the three shell2 architecture greps CI runs first     0s
    member-coverage   scripts/workspace_test_coverage.sh                     1s
    check             cargo check -p azul-css -p azul-core -p azul-layout   5s
    clippy            cargo clippy -p azul-core -p azul-css -p azul-layout
                        --all-targets -- -D warnings                        20s
    doc-check         cargo run -r -p azul-doc check                        5s
    doc-tests         cargo test -p azul-doc --bins --no-fail-fast          130s
    unit-tests        cargo test -p azul-css -p azul-core -p azul-layout
                        --lib --features azul-core/serde-json,\
                        azul-core/url,azul-layout/json,\
                        azul-layout/e2e-server, -p webrender, and
                        AZ_REQUIRE_TEST_FONTS=1                             60s
    css-io            cargo test -p azul-css --features io
                        --test test_system_style                             6s

  slow tier (NOT in the default run; --slow or --only <name>):
    integration-tests same crates, `--tests` instead of `--lib`.            34m
                      93 targets, but the time is three of them:
                      layout/tests/all.rs 28m, e2e_json 3.6m,
                      contenteditable_e2e 1m. Run before you push.
    e2e-json          cargo test -p azul-layout --test e2e_json
                        --features e2e-server                               3.6m
                      Named explicitly so a removed feature ERRORS. A bare
                      `cargo test -p azul-layout` SKIPS this target in
                      silence, exit code 0, because it declares
                      required-features = ["e2e-server"].
    dll-tests         cargo test -p azul-dll --lib --features build-dll
                      ~1m40s of build before the first assertion, which is
                      why it is the stage people drop. It holds the 48
                      desktop::shell2::headless tests and the host backend's.
    dll-default       cd dll && cargo test  (default features; the ubuntu
                      lint_and_check invocation, and the ONLY place the 119
                      desktop::shell2::linux tests ever run)
    leak-regression   cargo test -p azul-dll --features build-dll,e2e-test
                        --test leak_regression   (whole-file cfg is macOS)

FEATURE THRASH
  CI runs `check`/`clippy` on DEFAULT features and the test stages on the
  feature set above, in separate jobs with separate caches. Locally they share
  one target dir, so switching between them rebuilds azul-layout (~6 min).
  The stage order above groups the default-feature stages first to pay that
  cost exactly once per run. Do not reorder casually.

THE 28 `#[ignore]`d TESTS, AND WHY NO JOB RUNS `-- --ignored` (triaged 2026-08-20)
  There is deliberately no `cargo test -- --ignored` step anywhere, because
  every one of the 28 was checked and NONE of them would be green:

    14 in dll/  hardware. The 5 menu:: ones panic inside display.rs:443 —
                real display enumeration — on a Mac with a display attached,
                let alone on a headless runner, and the 3 display:: ones fail
                the same way; 2 video_codec::provision need a real Linux
                desktop's /lib/modules + apt metadata; 1 screencap::dmabuf
                needs a GPU/libEGL; 1 headless:: is a documented
                damage-tracking gap. Measured: 9 of the 14 even compile on
                macOS, and 9 of 9 fail.
    12 in layout/tests/text3/  RED, and not for want of hardware. They run
                headless in milliseconds and fail 12/12: each pins a hard-coded
                coordinate from the OLD text3 generation. Someone must
                adjudicate each one (stale expectation vs. real regression);
                the reason string on each now says so and says it was measured.
     1 flex_intrinsic_text::frame_around_overflow_hidden_strip_shrinks_too
                RED: the documented taffy 0.10 nested-scroll-container gap.
     1 ribbon_tab_whitespace::probe_tab_wrap_and_caption_centering_across_widths
                GREEN in 37s, but it asserts NOTHING — it prints the widths at
                which a label wraps. The LAW it probes IS enforced by the
                non-ignored test directly below it. Not worth 801 layouts of
                runner time.

  So `-- --ignored` cannot be a gate today. Fix the 12 text3 pins and it can be,
  for that target.

NOT COVERED HERE (needs a runner, a display, or another OS):
  reftests, the JSON E2E scenario corpus under e2e/, Miri, the cross-compile
  matrix, icu_parity, coretext_autoregression, the language-binding matrix,
  the demo builds, cargo-deny. This is the CHEAP half of CI, on purpose.

KNOWN-RED ON macOS TODAY (this script reports them RED; that is correct —
a check script that hides its own known failures has the disease it cures):
  * integration-tests / unit-tests:
      file::autotest_generated::path_canonicalize_requires_existence_and_returns_absolute
      file::autotest_generated::traversal_above_root_and_repeated_slashes_resolve_to_root
    Two pre-existing macOS path failures (7356 passed / 2 failed).
  * dll-tests:
      desktop::shell2::headless::tests::real_ribbon_resize_sweep_matches_fresh_at_every_step
    Fails deterministically on macOS in ~2s.
  (The three `clippy::missing_const_for_fn` errors in layout/src/probe.rs that
  used to be listed here are FIXED — the `#[allow]`s landed in 72c888bbc and
  this stage is green on macOS. Left as a note so the next reader does not go
  looking for a failure that is not there.)
    (rss_census, malloc_trim, allocator_stats). These fire ONLY off Linux: on
    Linux those functions have real bodies, so CI's ubuntu clippy job is green
    and a Mac is red. The inverse of the usual gap, same root cause — a gate
    whose result depends on the host. Fix is a targeted #[allow] on the three.
EOF
}

# --------------------------------------------------------------------------
MODE=fast
ONLY=""
KEEP_GOING=0
while [ $# -gt 0 ]; do
  case "$1" in
    --help|-h) usage; exit 0 ;;
    --list)
      printf '%-18s %-5s %s\n' STAGE TIER DESCRIPTION
      for s in "${STAGES[@]}"; do
        IFS='|' read -r n t d <<<"$s"
        printf '%-18s %-5s %s\n' "$n" "$t" "$d"
      done
      exit 0 ;;
    --slow) MODE=all ;;
    --only) ONLY="${2:-}"; [ -n "$ONLY" ] || { echo "--only needs a value" >&2; exit 2; }; shift ;;
    --only=*) ONLY="${1#--only=}" ;;
    --keep-going) KEEP_GOING=1 ;;
    *) echo "unknown argument: $1 (try --help)" >&2; exit 2 ;;
  esac
  shift
done

LOGDIR="${TMPDIR:-/tmp}/azul-check-$$"
mkdir -p "$LOGDIR"
RESULTS=()
FAILED=0

selected() {
  local name="$1" tier="$2"
  if [ -n "$ONLY" ]; then
    case ",$ONLY," in *",$name,"*) return 0 ;; *) return 1 ;; esac
  fi
  [ "$tier" = fast ] && return 0
  [ "$MODE" = all ] && return 0
  return 1
}

record() { RESULTS+=("$1|$2|$3"); }

# Guard: a test binary that compiled to nothing exits 0 with "running 0 tests".
# CI already asserts this for azul-dll (`::error::azul-dll --lib compiled to
# ZERO tests`); the same assertion belongs on every suite that can shrink to
# nothing behind a cfg.
assert_ran_tests() {
  local log="$1" what="$2"
  if grep -qE '^running 0 tests' "$log"; then
    echo "  !! $what compiled to ZERO tests — the gate proved nothing" >&2
    return 1
  fi
  return 0
}

# Guard: assert a named test module actually appeared in the output.
assert_module_ran() {
  local log="$1" prefix="$2"
  if ! grep -qE "^test $prefix" "$log"; then
    echo "  !! no '$prefix*' test ran — that module is cfg'd out of this build" >&2
    return 1
  fi
  return 0
}

run_stage() {
  local name="$1" desc="$2"; shift 2
  local log="$LOGDIR/$name.log"
  local start; start=$SECONDS
  echo "=== [$name] $desc"
  if "$@" >"$log" 2>&1; then
    local dur=$((SECONDS-start))
    echo "PASS  $name  (${dur}s)"
    record "$name" PASS "${dur}s"
  else
    local dur=$((SECONDS-start))
    echo "FAIL  $name  (${dur}s)   log: $log"
    tail -40 "$log" | sed 's/^/      | /'
    record "$name" FAIL "${dur}s  log: $log"
    FAILED=1
    [ "$KEEP_GOING" = 1 ] || { verdict; exit 1; }
  fi
}

# ---------------------------------------------------------------- stage bodies

stage_contracts() {
  # The class checks CI runs first. Cheap enough (<1s, no compilation) that it
  # belongs ahead of even the greps: a naming or binding mismatch here means the
  # release ships a 404 or a widget that never receives data, and neither shows
  # up in any compiler or test until an hour of CI has burned.
  python3 scripts/preflight_contracts.py
}

stage_arch_lint() {
  local bad=0
  # Backends must not touch content state.
  local v
  v=$(grep -rnE 'set_node_type|image_cache|dirty_text_nodes|cpu_image_callback_results' \
        dll/src/desktop/shell2 --include='*.rs' \
      | grep -v '/common/' | grep -vE '^[^:]*:[0-9]+:[[:space:]]*//' || true)
  if [ -n "$v" ]; then
    echo "Platform backends may not touch content state (route through LayoutWindow::apply_content_change):"
    echo "$v"; bad=1
  fi
  # Backends must not hand-write the event-diff baseline.
  v=$(grep -rnE 'previous_window_state[[:space:]]*=[^=]' \
        dll/src/desktop/shell2 --include='*.rs' \
      | grep -v '/common/' | grep -vE '^[^:]*:[0-9]+:[[:space:]]*//' || true)
  if [ -n "$v" ]; then
    echo "Do not assign previous_window_state directly; use the PlatformWindow helpers:"
    echo "$v"; bad=1
  fi
  # Backends must not block the UI thread on I/O.
  v=$(grep -rnE '\.recv\(\)' dll/src/desktop/shell2 --include='*.rs' \
      | grep -v '/common/' | grep -vE '^[^:]*:[0-9]+:[[:space:]]*//' \
      | grep -vE 'while let Ok\([^)]*\) = [A-Za-z_][A-Za-z0-9_.]*\.recv\(\)' || true)
  if [ -n "$v" ]; then
    echo "Unbounded .recv() in a backend — the UI thread must not block:"
    echo "$v"; bad=1
  fi
  return $bad
}

# `cargo metadata` + a grep, i.e. seconds. Mirrors check_crates' step of the
# same name: every workspace member must be accounted for in
# scripts/workspace_test_members.txt, so a new member cannot arrive untested.
stage_member_coverage() {
  "$REPO_ROOT/scripts/workspace_test_coverage.sh"
}

stage_check() {
  # `&&`-chained on purpose: run_stage calls these from an `if`, which
  # suppresses errexit inside the function body, so a bare sequence would
  # return only the LAST command's status and hide an earlier failure.
  cargo check -p azul-css \
    && cargo check -p azul-core \
    && cargo check -p azul-layout
}

stage_binding_syntax() {
  # The derive-parity gate greps generated bindings for NAMES, so it reports
  # green on code that does not compile - that is not hypothetical, it shipped
  # three non-compiling PHP methods and a Zig parameter-shadowing error before
  # anyone looked. Nothing else in this repo builds the non-Rust bindings, so
  # these two cheap syntax checks are the only thing standing between a codegen
  # change and a broken artifact.
  #
  # Both are pure parsers: no libazul, no cgo, no linking. Skipped rather than
  # failed when the toolchain is absent, so CI images without them stay green.
  local gen="target/codegen"
  if [ ! -d "$gen" ]; then
    echo "  (skip: $gen missing - run 'cargo run --release -p azul-doc codegen all')"
    return 0
  fi

  local rc=0
  if command -v zig >/dev/null 2>&1; then
    if zig ast-check "$gen/azul.zig"; then
      echo "  zig ast-check: ok"
    else
      echo "  zig ast-check: FAILED" >&2
      rc=1
    fi
  else
    echo "  (skip zig: not installed)"
  fi

  # OCaml gets a full TYPE check, not just a parse: `ocamlc -stop-after parsing`
  # accepted a file that referenced `ffi_az_style_cursor_partial_eq` 42k lines
  # before it was defined, and OCaml is order-sensitive. Only the type checker
  # caught that, and a second error where a recursive element is an opaque
  # pointer rather than a `Ctypes.structure`.
  if command -v ocamlfind >/dev/null 2>&1 && ocamlfind query ctypes >/dev/null 2>&1; then
    local omldir
    omldir="$(mktemp -d)"
    cp "$gen/azul.mli" "$gen/azul.ml" "$omldir/" 2>/dev/null
    if (cd "$omldir" \
          && ocamlfind ocamlc -package ctypes,ctypes.foreign -c azul.mli \
          && ocamlfind ocamlc -package ctypes,ctypes.foreign -c azul.ml) >/dev/null 2>&1; then
      echo "  ocaml typecheck: ok"
    else
      echo "  ocaml typecheck: FAILED" >&2
      (cd "$omldir" && ocamlfind ocamlc -package ctypes,ctypes.foreign -c azul.mli \
        && ocamlfind ocamlc -package ctypes,ctypes.foreign -c azul.ml) 2>&1 | head -20 >&2
      rc=1
    fi
    rm -rf "$omldir"
  else
    echo "  (skip ocaml: ocamlfind or ctypes not installed)"
  fi

  # Every remaining checker follows one rule: PROBE it on a known-good sample
  # before trusting its verdict. `command -v node` succeeds on this machine and
  # then node aborts with a missing dylib - exit 134, which a naive gate would
  # report as "your generated JavaScript is broken". A tool that cannot check
  # its own trivial sample gets skipped, not believed.
  probe_and_check() {
    local label="$1" probe_cmd="$2" real_cmd="$3"
    if ! eval "$probe_cmd" >/dev/null 2>&1; then
      echo "  (skip $label: checker unavailable or not working here)"
      return 0
    fi
    local out
    if out="$(eval "$real_cmd" 2>&1)"; then
      echo "  $label: ok"
    else
      echo "  $label: FAILED" >&2
      echo "$out" | head -20 >&2
      rc=1
    fi
  }

  local tmp; tmp="$(mktemp -d)"
  printf 'x = 1\n'        > "$tmp/probe.rb"
  printf 'local x = 1\n'  > "$tmp/probe.lua"
  printf '<?php $x = 1;\n' > "$tmp/probe.php"
  printf 'var x = 1;\n'   > "$tmp/probe.js"

  probe_and_check "ruby -c"   "ruby -c '$tmp/probe.rb'"       "ruby -c '$gen/azul.rb'"
  probe_and_check "luajit"    "luajit -bl '$tmp/probe.lua' /dev/null" \
                              "luajit -bl '$gen/azul.lua' /dev/null"
  probe_and_check "php -l"    "php -l '$tmp/probe.php'"       "php -l '$gen/Azul.php'"
  if [ -f "$gen/node/azul.js" ]; then
    probe_and_check "node --check" "node --check '$tmp/probe.js'" \
                                   "node --check '$gen/node/azul.js'"
  fi
  rm -rf "$tmp"

  if command -v gofmt >/dev/null 2>&1; then
    # `gofmt -e` prints syntax errors; formatting differences go to stdout as
    # filenames, which is why the error stream is what decides the verdict.
    local goerr
    goerr="$(gofmt -e "$gen/go" 2>&1 >/dev/null)"
    if [ -z "$goerr" ]; then
      echo "  gofmt -e: ok"
    else
      echo "  gofmt -e: FAILED" >&2
      echo "$goerr" >&2
      rc=1
    fi
  else
    echo "  (skip go: gofmt not installed)"
  fi

  return $rc
}

stage_doc_tests() {
  local log="$LOGDIR/doc-tests.raw"
  # `--bins`, not `--lib`: azul-doc is binary-only.
  cargo test -p azul-doc --bins --no-fail-fast 2>&1 | tee "$log"
  local rc=${PIPESTATUS[0]}
  # --strict: this names its targets (`--bins`), so an empty harness is always
  # a bug and no allowlist entry may excuse it.
  "$REPO_ROOT/scripts/zero_test_guard.sh" --strict "$log" || rc=1
  return $rc
}

stage_doc_check() {
  # Double-drop invariant over api.json + the generated mirror, plus the
  # guide-pointer lint. Same command CI's lint_and_check job runs.
  cargo run -r -p azul-doc check
}

# The feature list is NOT optional garnish, and it is the single most
# important line in this file. `cargo test -p azul-layout --lib` runs 7285
# tests and prints SUCCESS having compiled NONE of layout/src/e2e/ — the whole
# module is `#[cfg(feature = "e2e-server")]`. With the feature on it is 7304,
# and the 19 that appear include the manager-accounting gates and
# `non_interference_can_fail`, the proof that the non-interference primitive
# CAN go red. `azul-core/serde-json` + `azul-layout/json` do the same job for
# the JSON parse/serialise modules.
# `azul-core/url` joined the list on 2026-08-20: the 35 tests in core/src/url.rs
# are all `#[cfg(feature = "url")]` and NO job passed that feature, so they ran
# nowhere in CI and nowhere here either.
CI_TEST_FEATURES=azul-core/serde-json,azul-core/url,azul-layout/json,azul-layout/e2e-server

# The packages CI's test_lib job names. `webrender` joined on 2026-08-20 — it is
# a workspace member (a path dependency of azul-dll, which is how cargo made it
# one) whose 96 unit tests were in no job at all. It needs no generated bindings.
CI_TEST_PACKAGES=(-p azul-css -p azul-core -p azul-layout -p webrender)

stage_unit_tests() {
  local log="$LOGDIR/unit-tests.raw"
  AZ_REQUIRE_TEST_FONTS=1 \
  cargo test "${CI_TEST_PACKAGES[@]}" --lib \
    --features "$CI_TEST_FEATURES" --no-fail-fast 2>&1 | tee "$log"
  local rc=${PIPESTATUS[0]}
  # NOTE: no `assert_ran_tests` here — `zero_test_guard.sh` below subsumes it
  # AND honours scripts/zero_test_targets.txt, which the old helper cannot.
  # If this assertion ever fires, someone dropped the feature and the suite
  # silently shrank. That is the exact failure mode this script exists for.
  assert_module_ran "$log" 'e2e::' || rc=1
  assert_module_ran "$log" 'url::' || rc=1
  # --strict: `--lib` produces exactly one harness per crate; none of them may
  # be empty, and the allowlist (written for feature-gated INTEGRATION targets)
  # must not be able to excuse one that is.
  "$REPO_ROOT/scripts/zero_test_guard.sh" --strict "$log" || rc=1
  return $rc
}

# One test, entirely inside `#[cfg(feature = "io")]`. Until its `[[test]]` entry
# gained `required-features = ["io"]` (2026-08-20), a default `cargo test -p
# azul-css` built it as an EMPTY binary and printed a green `running 0 tests`.
# Named explicitly so a removed `io` feature ERRORS instead of vanishing.
stage_css_io() {
  local log="$LOGDIR/css-io.raw"
  cargo test -p azul-css --features io --test test_system_style 2>&1 | tee "$log"
  local rc=${PIPESTATUS[0]}
  assert_ran_tests "$log" "azul-css --test test_system_style --features io" || rc=1
  return $rc
}

stage_integration_tests() {
  local log="$LOGDIR/integration-tests.raw"
  # `--tests`, i.e. everything under */tests/. 93 targets. Almost all of them
  # finish in hundredths of a second; the cost is concentrated in three:
  #   layout/tests/all.rs             ~28 min
  #   layout/tests/e2e_json.rs        ~3.6 min  (also its own stage, by name)
  #   layout/tests/contenteditable_e2e.rs ~1 min
  # That is why this is the slow tier and not the fast one.
  AZ_REQUIRE_TEST_FONTS=1 \
  cargo test "${CI_TEST_PACKAGES[@]}" --tests \
    --features "$CI_TEST_FEATURES" --no-fail-fast 2>&1 | tee "$log"
  local rc=${PIPESTATUS[0]}
  # `assert_ran_tests` deliberately NOT used here: this stage builds
  # layout/tests/test_hint_vs_freetype.rs, which is dead on purpose and whose
  # empty harness is allowlisted. The old helper flags any `running 0 tests`
  # and would fail the stage for it; the guard reads the allowlist.
  "$REPO_ROOT/scripts/zero_test_guard.sh" "$log" || rc=1
  return $rc
}

stage_e2e_json() {
  local log="$LOGDIR/e2e-json.raw"
  cargo test -p azul-layout --test e2e_json --features e2e-server 2>&1 | tee "$log"
  local rc=${PIPESTATUS[0]}
  assert_ran_tests "$log" "e2e_json" || rc=1
  return $rc
}

stage_clippy() {
  cargo clippy -p azul-core -p azul-css -p azul-layout --all-targets -- -D warnings
}

stage_dll_tests() {
  local log="$LOGDIR/dll-tests.raw"
  cargo test -p azul-dll --lib --features build-dll --no-fail-fast 2>&1 | tee "$log"
  local rc=${PIPESTATUS[0]}
  assert_ran_tests "$log" "azul-dll --lib" || rc=1
  # The headless backend is platform-independent by construction; if its tests
  # are missing the build is not the one CI gates on.
  assert_module_ran "$log" 'desktop::shell2::headless::' || rc=1
  # And the HOST backend's own tests must be in there, or this run says nothing
  # about the platform you are actually developing on.
  case "$(uname -s)" in
    Darwin) assert_module_ran "$log" 'desktop::shell2::macos::'   || rc=1 ;;
    Linux)  assert_module_ran "$log" 'desktop::shell2::linux::'   || rc=1 ;;
    MINGW*|MSYS*|CYGWIN*)
            assert_module_ran "$log" 'desktop::shell2::windows::' || rc=1 ;;
  esac
  return $rc
}

stage_dll_default() {
  local log="$LOGDIR/dll-default.raw"
  ( cd dll && cargo test --no-fail-fast ) 2>&1 | tee "$log"
  local rc=${PIPESTATUS[0]}
  assert_ran_tests "$log" "dll default-feature tests" || rc=1
  return $rc
}

stage_leak_regression() {
  if [ "$(uname -s)" != Darwin ]; then
    echo "  -- leak_regression: whole-file cfg is target_os=\"macos\"; nothing to run here."
    return 0
  fi
  local log="$LOGDIR/leak-regression.raw"
  cargo test -p azul-dll --features build-dll,e2e-test --test leak_regression 2>&1 | tee "$log"
  local rc=${PIPESTATUS[0]}
  assert_ran_tests "$log" "leak_regression" || rc=1
  return $rc
}

verdict() {
  echo
  echo "──────────────────────────────────────────────────────────────────"
  for r in "${RESULTS[@]}"; do
    IFS='|' read -r n s d <<<"$r"
    printf '%-6s %-18s %s\n' "$s" "$n" "$d"
  done
  echo "──────────────────────────────────────────────────────────────────"
  if [ "$FAILED" = 0 ]; then
    echo "VERDICT: GREEN"
    if [ "$MODE" != all ] && [ -z "$ONLY" ]; then
      echo "NOTE: the SLOW tier did not run, so this green does NOT cover:"
      echo "      integration-tests  (93 targets under */tests/, incl. all.rs)"
      echo "      e2e-json           (the JSON scenario corpus)"
      echo "      dll-tests          (48 headless + host-backend tests)"
      echo "      dll-default / leak-regression"
      echo "      Run --slow before you tell anyone the batteries are green."
    fi
  else
    echo "VERDICT: RED — see the FAIL lines above. Logs in $LOGDIR"
  fi
}

# --------------------------------------------------------------------------
echo "azul check — repo $REPO_ROOT"
if [ -n "$ONLY" ]; then echo "mode: only=$ONLY"; else echo "mode: $MODE"; fi
echo

for s in "${STAGES[@]}"; do
  IFS='|' read -r name tier desc <<<"$s"
  if ! selected "$name" "$tier"; then
    if [ -z "$ONLY" ] && [ "$tier" = slow ]; then
      echo "SKIP  $name  (slow tier — rerun with --slow or --only $name)"
      record "$name" SKIP "slow tier, not selected"
    fi
    continue
  fi
  case "$name" in
    contracts)       run_stage "$name" "$desc" stage_contracts ;;
    arch-lint)       run_stage "$name" "$desc" stage_arch_lint ;;
    member-coverage) run_stage "$name" "$desc" stage_member_coverage ;;
    check)           run_stage "$name" "$desc" stage_check ;;
    doc-tests)       run_stage "$name" "$desc" stage_doc_tests ;;
    doc-check)       run_stage "$name" "$desc" stage_doc_check ;;
    binding-syntax)  run_stage "$name" "$desc" stage_binding_syntax ;;
    css-io)          run_stage "$name" "$desc" stage_css_io ;;
    unit-tests)        run_stage "$name" "$desc" stage_unit_tests ;;
    integration-tests) run_stage "$name" "$desc" stage_integration_tests ;;
    e2e-json)        run_stage "$name" "$desc" stage_e2e_json ;;
    clippy)          run_stage "$name" "$desc" stage_clippy ;;
    dll-tests)       run_stage "$name" "$desc" stage_dll_tests ;;
    dll-default)     run_stage "$name" "$desc" stage_dll_default ;;
    leak-regression) run_stage "$name" "$desc" stage_leak_regression ;;
  esac
done

verdict
[ "$FAILED" = 0 ] || exit 1
