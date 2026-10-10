//! Shared E2E result reporting + fixture loading.
//!
//! The verdict tally and the cargo-test-style output are lifted from the DLL's
//! `shell2/run.rs` (the `AZ_E2E` printer) so that every front-end — the
//! `AZ_E2E=<path>` binary path, `azul-doc e2e <dir>`, and the in-crate fixture
//! test — reports *identically*. Keeping one copy here is the whole point: the
//! gate's notion of "green" must not depend on which entry point ran it.
//!
//! Verdict semantics (unchanged from `run.rs`): a test carrying
//! `"expect": "fail"` inverts its raw result —
//!
//! | raw    | `expect` | verdict | fails the gate? |
//! |--------|----------|---------|-----------------|
//! | pass   | none     | PASS    | no              |
//! | fail   | none     | FAIL    | **yes**         |
//! | fail   | `"fail"` | XFAIL   | no              |
//! | pass   | `"fail"` | XPASS   | **yes**         |
//!
//! XPASS is red on purpose: the guarded bug is fixed, so the marker must go.
//!
//! A test whose `only_on` gate excludes this host reports the raw status
//! `"skip"` (with its `skip_reason`) and ran no step. Its verdict is SKIP
//! whatever its `expect` says - it neither failed nor passed - and a skip
//! never fails the gate. The summary counts it as `skipped`.

use alloc::{format, string::String, vec::Vec};

use super::{E2eTest, E2eTestResult};

/// Tally of per-test verdicts for one run.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct E2eVerdict {
    /// Clean pass (no `expect` marker).
    pub passed: usize,
    /// Genuine failure.
    pub failed: usize,
    /// Expected failure (`expect: fail` and it failed).
    pub xfail: usize,
    /// Unexpected pass (`expect: fail` but it passed) — a gate failure.
    pub xpass: usize,
    /// Not run on this host (its `only_on` gate) — never a gate failure.
    pub skipped: usize,
}

impl E2eVerdict {
    /// `true` when the run should be considered red (`FAIL` or `XPASS` present).
    #[must_use]
    pub const fn gate_failed(&self) -> bool {
        self.failed + self.xpass > 0
    }

    /// Process exit code: 1 when red, 0 when green.
    #[must_use]
    pub const fn exit_code(&self) -> i32 {
        if self.gate_failed() {
            1
        } else {
            0
        }
    }

    /// The tally back out of the `test result:` line [`render_report`]
    /// wrote (ANSI colour included), for a process that reads another's run
    /// - the AZ_E2E directory dispatcher reading a child's, `azul-doc mobile`
    /// reading a device log. `None` when `line` is not a summary line.
    #[must_use]
    pub fn parse_summary(line: &str) -> Option<Self> {
        let (_, tail) = line.split_once("test result:")?;
        let mut v = Self::default();
        let mut any = false;
        // "<word>. 3 passed; 1 failed; 0 xfailed; 0 xpassed; 2 skipped; 0 measured; ..."
        for part in tail.split(';') {
            let mut words = part.split_whitespace().rev();
            let (Some(name), Some(count)) = (words.next(), words.next()) else {
                continue;
            };
            let Ok(count) = count.parse::<usize>() else {
                continue;
            };
            let slot = match name {
                "passed" => &mut v.passed,
                "failed" => &mut v.failed,
                "xfailed" => &mut v.xfail,
                "xpassed" => &mut v.xpass,
                "skipped" => &mut v.skipped,
                _ => continue,
            };
            *slot = count;
            any = true;
        }
        any.then_some(v)
    }
}

/// Whether `test` is marked as a known failure.
fn expects_fail(test: &E2eTest) -> bool {
    test.expect.as_deref() == Some("fail")
}

/// Render one cargo-test-style `test <name> ... <verdict>` line.
fn verdict_line(name: &str, verdict: &str, colour: &str, duration_ms: u64, suffix: &str) -> String {
    format!("test {name} ... \x1b[{colour}m{verdict}\x1b[0m ({duration_ms} ms){suffix}")
}

/// Tally `results` (paired with the `tests` they came from) and render the full
/// cargo-test-style report: one line per test, a `failures:` block detailing the
/// red ones, and the trailing `test result:` summary.
///
/// Returns the rendered report plus the tally, so callers choose the sink
/// (stderr for the binary/gate, stdout for a CLI) without this module needing
/// `std::io`.
#[must_use]
pub fn render_report(tests: &[E2eTest], results: &[E2eTestResult]) -> (String, E2eVerdict) {
    let mut out = String::new();
    let mut v = E2eVerdict::default();
    // (result, verdict) pairs that make the gate red.
    let mut gate_failures: Vec<(&E2eTestResult, &'static str)> = Vec::new();

    out.push('\n');

    for result in results {
        // Pair by name; a fixture whose name is absent is treated as unmarked.
        let marked = tests
            .iter()
            .find(|t| t.name == result.name)
            .is_some_and(expects_fail);
        let raw_pass = result.status == "pass";

        // A skip ran nothing, so its `expect` marker has nothing to invert.
        if result.status == "skip" {
            v.skipped += 1;
            let reason = result.skip_reason.as_deref().unwrap_or("not run on this host");
            out.push_str(&verdict_line(
                &result.name,
                "SKIP",
                "33",
                result.duration_ms,
                &format!(" ({reason})"),
            ));
            out.push('\n');
            continue;
        }

        let line = match (raw_pass, marked) {
            (true, false) => {
                v.passed += 1;
                verdict_line(&result.name, "PASS", "32", result.duration_ms, "")
            }
            (false, false) => {
                v.failed += 1;
                gate_failures.push((result, "FAIL"));
                verdict_line(&result.name, "FAIL", "31", result.duration_ms, "")
            }
            (false, true) => {
                v.xfail += 1;
                verdict_line(
                    &result.name,
                    "XFAIL",
                    "33",
                    result.duration_ms,
                    " (expected failure)",
                )
            }
            (true, true) => {
                v.xpass += 1;
                gate_failures.push((result, "XPASS"));
                verdict_line(
                    &result.name,
                    "XPASS",
                    "31",
                    result.duration_ms,
                    " (unexpectedly passed — remove the \"expect\":\"fail\" marker)",
                )
            }
        };
        out.push_str(&line);
        out.push('\n');
    }

    out.push('\n');

    if !gate_failures.is_empty() {
        out.push_str("failures:\n\n");
        for (f, verdict) in &gate_failures {
            out.push_str(&format!("---- {} ({verdict}) ----\n", f.name));
            if *verdict == "XPASS" {
                out.push_str(
                    "  test passed but is marked \"expect\":\"fail\" — the bug it guards is \
                     fixed; remove the marker\n",
                );
            }
            for step in &f.steps {
                if step.status == "fail" {
                    out.push_str(&format!(
                        "  step {}: {} → FAILED: {}\n",
                        step.step_index,
                        step.op,
                        step.error.as_deref().unwrap_or("unknown error")
                    ));
                }
            }
            out.push('\n');
        }
        out.push_str("failures:\n");
        for (f, verdict) in &gate_failures {
            out.push_str(&format!("    {} ({verdict})\n", f.name));
        }
        out.push('\n');
    }

    let word = if v.gate_failed() {
        "\x1b[31mFAILED\x1b[0m"
    } else {
        "\x1b[32mok\x1b[0m"
    };
    out.push_str(&format!(
        "test result: {word}. {} passed; {} failed; {} xfailed; {} xpassed; {} skipped; 0 \
         measured; 0 filtered out\n",
        v.passed, v.failed, v.xfail, v.xpass, v.skipped
    ));

    (out, v)
}

/// Load every e2e test referenced by `path`.
///
/// A DIRECTORY loads each `*.json` inside it in sorted (deterministic) order; a
/// FILE loads just that one. A file holds one test object or an ARRAY of them
/// (`examples/azul-widgets/e2e/` keeps a demo's scenarios together). Returns a
/// `Result` instead of calling `process::exit`, so it is usable from a library
/// and from a CLI that wants to report the error itself; the DLL's `AZ_E2E`
/// runner (`run.rs`) loads through it too.
///
/// # Errors
///
/// Returns a human-readable message if `path` cannot be stat'ed or read, or if
/// any fixture is not valid `E2eTest` JSON.
#[cfg(feature = "std")]
pub fn load_e2e_tests(path: &std::path::Path) -> Result<Vec<E2eTest>, String> {
    use alloc::vec;

    let meta = std::fs::metadata(path)
        .map_err(|e| format!("cannot stat E2E path '{}': {e}", path.display()))?;

    let files: Vec<std::path::PathBuf> = if meta.is_dir() {
        let mut files: Vec<std::path::PathBuf> = std::fs::read_dir(path)
            .map_err(|e| format!("cannot read E2E directory '{}': {e}", path.display()))?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("json"))
            .collect();
        // Deterministic run order.
        files.sort();
        files
    } else {
        vec![path.to_path_buf()]
    };

    let mut tests = Vec::with_capacity(files.len());
    for file in files {
        let src = std::fs::read_to_string(&file)
            .map_err(|e| format!("cannot read '{}': {e}", file.display()))?;
        // Decided by the first character, so a broken test inside an array
        // reports ITS error rather than "expected a struct".
        let parsed = if src.trim_start().starts_with('[') {
            serde_json::from_str::<Vec<E2eTest>>(&src)
        } else {
            serde_json::from_str::<E2eTest>(&src).map(|test| vec![test])
        };
        tests.extend(
            parsed.map_err(|e| format!("invalid E2E JSON in '{}': {e}", file.display()))?,
        );
    }

    Ok(tests)
}
