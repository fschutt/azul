//! `azul-doc e2e <path>` — run e2e JSON scenarios headlessly.
//!
//! This is the in-process equivalent of `AZ_E2E=<path> ./some_app`: it drives
//! the *same* debug-server op dispatcher (now living in `azul_layout::e2e`)
//! against a headless `LayoutWindow`, and prints the *same* cargo-test-style
//! verdict report via `azul_layout::e2e::render_report`.
//!
//! Why this exists: the `AZ_E2E` path needs a linked host binary (a "hello
//! world" app to drive), and the CI gate runs one OS process per JSON file in a
//! serial bash loop. This subcommand needs no host binary and runs a whole
//! directory in ONE process, which is what makes a 13k-file corpus tractable.
//!
//! `AZ_E2E` itself is unchanged — this is an additional front-end over the same
//! runner, not a replacement.

use std::path::PathBuf;

use anyhow::{bail, Context};
use azul_layout::e2e::{load_e2e_tests, render_report, run_e2e_test};

/// Parsed `e2e` subcommand options.
pub struct E2eRunOptions {
    /// File or directory of `*.json` scenarios.
    pub path: PathBuf,
    /// Only run tests whose name contains this substring.
    pub filter: Option<String>,
    /// List the tests that would run, then exit.
    pub list: bool,
}

impl E2eRunOptions {
    /// Parse `e2e <path> [--filter <substr>] [--list]`.
    ///
    /// # Errors
    ///
    /// Fails when `<path>` is missing or an unknown flag is supplied.
    pub fn parse(args: &[&str]) -> anyhow::Result<Self> {
        let mut path: Option<PathBuf> = None;
        let mut filter = None;
        let mut list = false;

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
                "usage: azul-doc e2e <file-or-dir.json> [--filter <substr>] [--list]",
            )?,
            filter,
            list,
        })
    }
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

    if opts.list {
        for t in &tests {
            println!("{}", t.name);
        }
        return Ok(0);
    }

    eprintln!("\nrunning {} e2e scenario(s)", tests.len());

    let results: Vec<_> = tests.iter().map(run_e2e_test).collect();
    let (report, verdict) = render_report(&tests, &results);
    eprint!("{report}");

    Ok(verdict.exit_code())
}
