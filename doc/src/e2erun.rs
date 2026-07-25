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
//! its own `LayoutWindow` (so its own DOM mount override and E2E scratch —
//! frame/resource snapshots, composition trace, presented frame) and its own
//! `E2eSession`. The single remaining exception is the process-global test clock
//! — see [`uses_global_clock`].
//!
//! `AZ_E2E` itself is unchanged — this is an additional front-end over the same
//! runner, not a replacement.

use std::path::PathBuf;

use anyhow::{bail, Context};
use azul_layout::e2e::{load_e2e_tests, render_report, run_e2e_test, E2eTest, E2eTestResult};
use rayon::prelude::*;

/// The op that advances the process-global E2E test clock
/// (`azul_core::task::advance_test_clock_ms`).
const GLOBAL_CLOCK_OP: &str = "tick_ms";

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
}

impl E2eRunOptions {
    /// Parse `e2e <path> [--filter <substr>] [--list] [--jobs <n>]`.
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
            path: path.context(
                "usage: azul-doc e2e <file-or-dir.json> [--filter <substr>] [--list] [--jobs <n>]",
            )?,
            filter,
            list,
            jobs,
        })
    }
}

/// Whether `test` advances the process-global E2E test clock.
///
/// `tick_ms` calls `azul_core::task::advance_test_clock_ms`, a process-wide
/// monotonic offset added to every `Instant::now()` the engine sees. It is the
/// ONE piece of shared mutable state a scenario can still write, and it is not
/// per-window-able: the read path is `GetSystemTimeCallback`, a bare
/// `extern "C" fn()` in the public C API with nowhere to carry a window.
///
/// A concurrent tick would jump an unrelated scenario's clock forward and
/// complete its in-flight animations early — a silently wrong verdict, which is
/// exactly what this suite exists to catch. So these scenarios run SERIALLY,
/// before the parallel phase; during the parallel phase the offset is constant,
/// and a constant shift is invisible (only deltas matter).
fn uses_global_clock(test: &E2eTest) -> bool {
    test.steps.iter().any(|s| s.op == GLOBAL_CLOCK_OP)
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

    // Each scenario owns its state — its own `LayoutWindow` (hence its own
    // mount override and E2E scratch) and its own `E2eSession`. The only
    // exception is the global test clock; see `uses_global_clock`.
    let (serial, parallel): (Vec<&E2eTest>, Vec<&E2eTest>) =
        tests.iter().partition(|t| uses_global_clock(t));

    eprintln!(
        "\nrunning {} e2e scenario(s) — {} parallel on {jobs} thread(s), {} serial (global test \
         clock)",
        tests.len(),
        parallel.len(),
        serial.len(),
    );

    let mut results: Vec<E2eTestResult> =
        serial.into_iter().map(|t| run_e2e_test(t)).collect();

    if !parallel.is_empty() {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(jobs)
            .thread_name(|i| format!("e2e-{i}"))
            .build()
            .context("failed to build the e2e thread pool")?;
        results.extend(
            pool.install(|| {
                parallel
                    .par_iter()
                    .map(|t| run_e2e_test(t))
                    .collect::<Vec<_>>()
            }),
        );
    }

    // Completion order is nondeterministic; the report must not be.
    results.sort_by(|a, b| a.name.cmp(&b.name));

    let (report, verdict) = render_report(&tests, &results);
    eprint!("{report}");

    Ok(verdict.exit_code())
}
