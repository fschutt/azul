//! `azul-doc e2e <path>` — run e2e JSON scenarios headlessly.
//!
//! This is the in-process equivalent of `AZ_E2E=<path> ./some_app`: it drives
//! the *same* debug-server op dispatcher (now living in `azul_layout::e2e`)
//! against a headless `LayoutWindow`, and prints the *same* cargo-test-style
//! verdict report via `azul_layout::e2e::render_report`.
//!
//! Why this exists: the `AZ_E2E` path needs a linked host binary (a "hello
//! world" app to drive), and the CI gate used to run one OS process per JSON
//! file in a serial bash loop. This subcommand needs no host binary and runs a
//! whole directory in ONE process, IN PARALLEL, which is what makes a 13k-file
//! corpus tractable. It is what the `e2e_headless` CI job runs.
//!
//! Parallelism is sound because a scenario owns its state: `run_e2e_test` builds
//! its own `LayoutWindow` (so its own DOM mount override, E2E scratch —
//! frame/resource snapshots, composition trace, presented frame — and its own
//! frame-report reset generation) and its own `E2eSession`, and it starts by
//! resetting the E2E test clock, which is scoped to the running THREAD
//! (`azul_core::task`). Rayon runs a scenario's closure start-to-finish on one
//! worker, so `tick_ms` moves that scenario's clock and no other's.
//!
//! `AZ_E2E` itself is unchanged — this is an additional front-end over the same
//! runner, not a replacement.

use std::path::PathBuf;

use anyhow::{bail, Context};
use azul_layout::e2e::{load_e2e_tests, render_report, run_e2e_test, E2eTest, E2eTestResult};
use rayon::prelude::*;

/// Parsed `e2e` subcommand options.
pub struct E2eRunOptions {
    /// File or directory of `*.json` scenarios.
    pub path: PathBuf,
    /// Only run tests whose name contains this substring.
    pub filter: Option<String>,
    /// List the tests that would run, then exit.
    pub list: bool,
    /// Worker threads for the parallel phase. `None` = available parallelism.
    pub jobs: Option<usize>,
    /// `--keep-going`: a red step does not end its scenario. The runner goes on
    /// and the report lists EVERY red step of every scenario (default: the first
    /// red step ends the scenario, and the later ones are never seen).
    pub keep_going: bool,
}

impl E2eRunOptions {
    /// Parse `e2e <path> [--filter <substr>] [--list] [--jobs <n>] [--keep-going]`.
    ///
    /// # Errors
    ///
    /// Fails when `<path>` is missing, `--jobs` is not a positive integer, or an
    /// unknown flag is supplied.
    pub fn parse(args: &[&str]) -> anyhow::Result<Self> {
        let mut path: Option<PathBuf> = None;
        let mut filter = None;
        let mut list = false;
        let mut jobs = None;
        let mut keep_going = false;

        let mut i = 0;
        while i < args.len() {
            match args[i] {
                "--filter" => {
                    let v = args.get(i + 1).context("--filter needs a value")?;
                    filter = Some((*v).to_string());
                    i += 2;
                }
                "--list" => {
                    list = true;
                    i += 1;
                }
                "--keep-going" => {
                    keep_going = true;
                    i += 1;
                }
                "--jobs" | "-j" => {
                    let v = args.get(i + 1).context("--jobs needs a value")?;
                    let n: usize = v
                        .parse()
                        .with_context(|| format!("--jobs expects a number, got '{v}'"))?;
                    if n == 0 {
                        bail!("--jobs must be >= 1");
                    }
                    jobs = Some(n);
                    i += 2;
                }
                other if other.starts_with('-') => bail!("unknown flag for `e2e`: {other}"),
                other => {
                    if path.is_some() {
                        bail!("`e2e` takes a single path (got a second: {other})");
                    }
                    path = Some(PathBuf::from(other));
                    i += 1;
                }
            }
        }

        Ok(Self {
            path: path.context(USAGE)?,
            filter,
            list,
            jobs,
            keep_going,
        })
    }
}

/// The usage line of `azul-doc e2e`, shared by the parse error and the help.
pub const USAGE: &str =
    "usage: azul-doc e2e <file-or-dir.json> [--filter <substr>] [--list] [--jobs <n>] [--keep-going]";

/// Turn every scenario of `tests` into one that does not stop at its first red
/// step (`config.continue_on_failure`), so the report lists ALL of them.
///
/// The default is first-failure-only on purpose: a step after a red one runs
/// against whatever the red one left behind, and CI wants the first cause. A
/// REVIEW wants the opposite. A scenario with five red assertions showed one,
/// and the other four - often a different root cause each - stayed hidden until
/// the first was fixed (the first `gen-e2e` batch: every later assertion of
/// 32 scenarios).
pub fn keep_going(tests: &mut [E2eTest]) {
    for t in tests {
        t.config.continue_on_failure = true;
    }
}

/// Run `tests` in parallel on `jobs` threads (`None` = available parallelism)
/// and return their results in the order of `tests`: `results[k]` is the result
/// of `tests[k]`, whatever their names are. A caller that has to attribute a
/// result to the FILE its test came from (`gen-e2e --review-batch`) pairs by
/// this position - never by name, which two scenarios can share.
///
/// # Errors
///
/// Fails only when the thread pool cannot be built.
pub fn run_tests_in_order(
    tests: &[E2eTest],
    jobs: Option<usize>,
) -> anyhow::Result<Vec<E2eTestResult>> {
    let jobs = jobs.unwrap_or_else(|| {
        std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get)
    });

    // Every scenario runs in parallel: each owns its `LayoutWindow` (mount
    // override, E2E scratch, frame-report reset generation), its `E2eSession`
    // and - since the test clock became thread-scoped and `run_e2e_test` resets
    // it - its own clock. Rayon never migrates a running closure, so a scenario
    // stays on the worker whose clock it is ticking. `collect` on an indexed
    // parallel iterator keeps the input order.
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(jobs)
        .thread_name(|i| format!("e2e-{i}"))
        .build()
        .context("failed to build the e2e thread pool")?;
    Ok(pool.install(|| tests.par_iter().map(run_e2e_test).collect::<Vec<_>>()))
}

/// Run `tests` in parallel on `jobs` threads (`None` = available parallelism)
/// and return their results SORTED BY NAME.
///
/// The execution core of this subcommand, factored out so that a second
/// front-end — `gen-e2e --review-batch`, which runs the scenarios it has just
/// generated — drives the scenarios through *this* code rather than growing a
/// second runner. Sorting here (not at the call site) is what makes the result
/// order independent of rayon's completion order, which `render_report` relies
/// on for a deterministic report.
///
/// # Errors
///
/// Fails only when the thread pool cannot be built.
pub fn run_tests(tests: &[E2eTest], jobs: Option<usize>) -> anyhow::Result<Vec<E2eTestResult>> {
    let mut results = run_tests_in_order(tests, jobs)?;

    // The report must not depend on how the run was scheduled.
    results.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(results)
}

/// Run the scenarios and print a cargo-test-style report.
///
/// Returns the process exit code (0 green, 1 red) rather than exiting, so the
/// caller controls teardown.
///
/// # Errors
///
/// Fails when the path cannot be read or a fixture is not valid `E2eTest` JSON.
pub fn run(opts: &E2eRunOptions) -> anyhow::Result<i32> {
    let mut tests = load_e2e_tests(&opts.path).map_err(|e| anyhow::anyhow!(e))?;

    if let Some(f) = opts.filter.as_deref() {
        tests.retain(|t| t.name.contains(f));
    }

    if opts.keep_going {
        keep_going(&mut tests);
    }

    if tests.is_empty() {
        // An empty selection is almost always a typo'd path/filter, not a green
        // run — say so loudly instead of printing "0 passed" and exiting 0.
        bail!(
            "no e2e tests selected from '{}'{}",
            opts.path.display(),
            opts.filter
                .as_deref()
                .map_or_else(String::new, |f| format!(" with filter '{f}'"))
        );
    }

    // Sort up front so BOTH the report order and the pairing in `render_report`
    // are independent of directory-iteration order and of completion order.
    tests.sort_by(|a, b| a.name.cmp(&b.name));

    if opts.list {
        for t in &tests {
            println!("{}", t.name);
        }
        return Ok(0);
    }

    let jobs = opts.jobs.unwrap_or_else(|| {
        std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get)
    });

    eprintln!(
        "\nrunning {} e2e scenario(s) in parallel on {jobs} thread(s){}",
        tests.len(),
        if opts.keep_going {
            " (--keep-going: every red step is reported, not only the first of each scenario)"
        } else {
            ""
        },
    );

    let started = std::time::Instant::now();
    let results = run_tests(&tests, Some(jobs))?;
    let wall = started.elapsed();

    let (report, verdict) = render_report(&tests, &results);
    eprint!("{report}");
    eprintln!("wall clock: {:.2}s", wall.as_secs_f64());

    Ok(verdict.exit_code())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A headless scenario: mount, wait for the frame, then `asserts`.
    fn scenario(name: &str, asserts: &[&str]) -> E2eTest {
        let mut steps = vec![
            r#"{"op":"mount","html":["<body><div id=\"a\"></div></body>"]}"#.to_string(),
            r#"{"op":"wait_frame"}"#.to_string(),
        ];
        steps.extend(asserts.iter().map(|a| (*a).to_string()));
        serde_json::from_str(&format!(
            r#"{{"name":"{name}","setup":{{"window_width":200,"window_height":100,"dpi":96}},
                "steps":[{}]}}"#,
            steps.join(",")
        ))
        .expect("a valid scenario")
    }

    const PRESENT: &str = r##"{"op":"assert_exists","selector":"#a"}"##;
    const ABSENT_1: &str = r##"{"op":"assert_exists","selector":"#nope1"}"##;
    const ABSENT_2: &str = r##"{"op":"assert_exists","selector":"#nope2"}"##;

    fn red_steps(r: &E2eTestResult) -> Vec<usize> {
        r.steps
            .iter()
            .filter(|s| s.status == "fail")
            .map(|s| s.step_index)
            .collect()
    }

    #[test]
    fn keep_going_is_a_flag_and_the_default_stays_first_failure_only() {
        let o = E2eRunOptions::parse(&["e2e", "--keep-going"]).unwrap();
        assert!(o.keep_going);
        assert_eq!(o.path, PathBuf::from("e2e"));
        // flag order does not matter
        assert!(
            E2eRunOptions::parse(&["--keep-going", "e2e", "--jobs", "2"])
                .unwrap()
                .keep_going
        );
        // without the flag nothing changes
        assert!(
            !E2eRunOptions::parse(&["e2e", "--jobs", "2"])
                .unwrap()
                .keep_going
        );
        // and it is documented
        let e = E2eRunOptions::parse(&[])
            .err()
            .expect("no path is an error")
            .to_string();
        assert!(e.contains("--keep-going"), "{e}");
        assert!(USAGE.contains("--keep-going"));
    }

    /// G6. A scenario with two red assertions: the default stops at the first
    /// and the second is never seen; `--keep-going` reports both, and the
    /// verdict is still FAIL.
    #[test]
    fn keep_going_reports_every_red_step_of_a_scenario() {
        let tests = vec![scenario("two_red", &[ABSENT_1, PRESENT, ABSENT_2])];

        let first_only = run_tests_in_order(&tests, Some(1)).unwrap();
        assert_eq!(first_only[0].status, "fail");
        assert_eq!(
            red_steps(&first_only[0]),
            [2],
            "the default ends at the first red step"
        );
        assert_eq!(first_only[0].steps.len(), 3, "steps after it did not run");

        let mut all = tests.clone();
        keep_going(&mut all);
        let every = run_tests_in_order(&all, Some(1)).unwrap();
        assert_eq!(every[0].status, "fail", "the verdict is still red");
        assert_eq!(
            red_steps(&every[0]),
            [2, 4],
            "both red assertions are reported"
        );
        let (report, verdict) = render_report(&all, &every);
        assert!(verdict.gate_failed());
        for needle in [
            "step 2: assert_exists",
            "step 4: assert_exists",
            "#nope1",
            "#nope2",
        ] {
            assert!(report.contains(needle), "{needle}:\n{report}");
        }
        // the green step between them is not reported as red
        assert!(!report.contains("step 3:"), "{report}");
    }

    /// G1's other half: results come back in the order of the tests, whatever
    /// the tests are called, so a caller can pair them with their FILES.
    #[test]
    fn results_come_back_in_the_order_of_the_tests_even_when_names_repeat() {
        // Three scenarios, ONE name, three different outcomes.
        let tests = vec![
            scenario("same_name", &[PRESENT]),
            scenario("same_name", &[ABSENT_1]),
            scenario("same_name", &[PRESENT, PRESENT]),
        ];
        let results = run_tests_in_order(&tests, Some(3)).unwrap();
        let status: Vec<&str> = results.iter().map(|r| r.status.as_str()).collect();
        assert_eq!(status, ["pass", "fail", "pass"]);
        let steps: Vec<usize> = results.iter().map(|r| r.step_count).collect();
        assert_eq!(steps, [3, 3, 4]);

        // ...while `run_tests` (the CLI) sorts them by name for a stable report.
        let sorted = run_tests(&tests, Some(3)).unwrap();
        assert_eq!(sorted.len(), 3);
    }
}
