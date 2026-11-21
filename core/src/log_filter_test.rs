#[allow(unused_imports)]
pub use super::*;

#[cfg(test)]
mod tests {
    use super::*;

    /// The tests that read or write the process-wide level, category and echo
    /// state take this lock: in parallel they stomp each other (one test's
    /// `set_min_level(Some(Debug))` between another's `set_min_level(Some(Warn))`
    /// and its assertion made `runtime_toggles_take_effect` flaky). Poisoning
    /// is tolerated: a failed test must not fail its neighbours.
    static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// An `AZ_LOG` that is SET but empty (or `1`) means "logs, please": Debug.
    /// What a process that did not set it at all gets is `level_for`'s rule,
    /// below.
    #[test]
    fn an_empty_az_log_parses_to_debug_and_keeps_every_category() {
        let (level, overrides) = parse("");
        assert_eq!(level, Some(Level::Debug));
        assert!(overrides.is_empty());
    }

    /// A process that set none of the variables that ask for logs gets errors
    /// only (it got Debug, and with it a firehose on stderr, until 2026-11-20);
    /// one that set any of them gets what `AZ_LOG` says.
    #[test]
    fn a_run_that_asked_for_no_logs_gets_errors_only() {
        assert_eq!(level_for(false, parse("").0), Some(Level::Error));
        assert_eq!(level_for(false, parse("off").0), Some(Level::Error));
        assert_eq!(level_for(true, parse("").0), Some(Level::Debug));
        assert_eq!(level_for(true, parse("1").0), Some(Level::Debug));
        assert_eq!(level_for(true, parse("warn").0), Some(Level::Warn));
        assert_eq!(level_for(true, parse("off").0), None);
        assert!(LOG_REQUEST_VARS.contains(&"AZ_LOG"));
        assert!(LOG_REQUEST_VARS.contains(&"AZ_E2E"));
    }

    /// The probe of [`the_environment_decides_the_level_and_the_echo`]: runs
    /// only in the child process it starts, where it reads the environment
    /// the way the first log call of any process does and prints what it
    /// found.
    #[test]
    fn probe_the_state_the_environment_leaves() {
        if std::env::var_os("AZ_LOG_FILTER_PROBE").is_none() {
            return;
        }
        init_from_env();
        println!(
            "PROBE level={:?} echo={} layout={} platform={} debug_passes={} error_passes={}",
            min_level(),
            stderr_echo(),
            category_enabled(Category::Layout),
            category_enabled(Category::Platform),
            enabled(Category::Layout, Level::Debug),
            enabled(Category::Layout, Level::Error),
        );
    }

    /// What `init_from_env` leaves in a process whose environment is exactly
    /// `env`, read in a child process (the state is process-wide and set once,
    /// and the developer running the tests may have `AZ_LOG` set).
    fn probe(env: &[(&str, &str)]) -> String {
        let exe = std::env::current_exe().expect("the test binary has a path");
        let out = std::process::Command::new(exe)
            .args([
                "log_filter::log_filter_test::tests::probe_the_state_the_environment_leaves",
                "--exact",
                "--nocapture",
                "--test-threads=1",
            ])
            .env_clear()
            .env("AZ_LOG_FILTER_PROBE", "1")
            .envs(env.iter().copied())
            .output()
            .expect("re-running the test binary must succeed");
        let stdout = String::from_utf8_lossy(&out.stdout);
        stdout
            .lines()
            .find_map(|line| line.find("PROBE ").map(|at| line[at + 6..].to_string()))
            .unwrap_or_else(|| {
                panic!(
                    "the child printed no PROBE line (probe name stale?)\n--- child stdout \
                     ---\n{stdout}\n--- child stderr ---\n{}",
                    String::from_utf8_lossy(&out.stderr)
                )
            })
    }

    /// REGRESSION (2026-11-20, a 612 MB log of a two-minute editor session):
    /// with `AZ_LOG` unset an ordinary lean build printed the whole Debug
    /// firehose (3.4 million lines, 60 s of the 124 s). A process that asked
    /// for no logs is at Error and echoes nothing else; every way of asking
    /// still works.
    #[test]
    fn the_environment_decides_the_level_and_the_echo() {
        // (environment, level, echo to stderr, Layout enabled, Debug passes)
        let cases: &[(&[(&str, &str)], &str, bool, bool, bool)] = &[
            // Asked for nothing: errors, no echo.
            (&[], "Some(Error)", false, true, false),
            // The other AZ_ variables do not ask for logs.
            (&[("AZ_PROFILE", "cpu"), ("AZ_BACKEND", "gpu")], "Some(Error)", false, true, false),
            // AZ_LOG set at all turns the echo on.
            (&[("AZ_LOG", "debug")], "Some(Debug)", true, true, true),
            (&[("AZ_LOG", "1")], "Some(Debug)", true, true, true),
            (&[("AZ_LOG", "trace")], "Some(Trace)", true, true, true),
            (&[("AZ_LOG", "info")], "Some(Info)", true, true, false),
            (&[("AZ_LOG", "warn")], "Some(Warn)", true, true, false),
            (&[("AZ_LOG", "error")], "Some(Error)", true, true, false),
            (&[("AZ_LOG", "off")], "None", false, true, false),
            // ... with its category overrides.
            (&[("AZ_LOG", "debug,-layout")], "Some(Debug)", true, false, false),
            // The other variables that ask for logs: Debug, the echo is the
            // shell's call (a debug server counts as a sink there).
            (&[("AZ_DEBUG", "8765")], "Some(Debug)", false, true, true),
            (&[("AZ_E2E", "scenario.json")], "Some(Debug)", false, true, true),
            (&[("AZ_RECORD", "record.txt")], "Some(Debug)", false, true, true),
            (&[("AZ_LOG_FILE", "log.txt")], "Some(Debug)", false, true, true),
            // AZ_LOG_STDERR says it outright, either way.
            (&[("AZ_LOG_STDERR", "1")], "Some(Debug)", true, true, true),
            (&[("AZ_LOG_STDERR", "0")], "Some(Debug)", false, true, true),
            (&[("AZ_LOG", "debug"), ("AZ_LOG_STDERR", "0")], "Some(Debug)", false, true, true),
            // Off is off, the echo included.
            (&[("AZ_LOG", "off"), ("AZ_LOG_STDERR", "1")], "None", false, true, false),
        ];
        for (env, level, echo, layout, debug_passes) in cases {
            let got = probe(env);
            let want = format!(
                "level={level} echo={echo} layout={layout} platform=true debug_passes={debug_passes} \
                 error_passes={}",
                *level != "None" && layout.to_owned()
            );
            assert_eq!(got, want, "environment {env:?}");
        }
    }

    #[test]
    fn off_spellings_disable_logging() {
        for raw in [
            "off", "0", "false", "none", "no", "disable", "disabled", "  OFF  ",
        ] {
            assert_eq!(parse(raw).0, None, "{raw} should disable logging");
        }
    }

    #[test]
    fn level_names_parse() {
        assert_eq!(parse("trace").0, Some(Level::Trace));
        assert_eq!(parse("debug").0, Some(Level::Debug));
        assert_eq!(parse("info").0, Some(Level::Info));
        assert_eq!(parse("warn").0, Some(Level::Warn));
        assert_eq!(parse("error").0, Some(Level::Error));
    }

    #[test]
    fn unknown_token_falls_back_to_debug_rather_than_silence() {
        // Silence-on-typo is the failure mode this whole module exists to stop.
        assert_eq!(parse("verbose-ish").0, Some(Level::Debug));
    }

    #[test]
    fn category_overrides_parse_in_both_directions() {
        let (level, overrides) = parse("warn,+platform,-layout");
        assert_eq!(level, Some(Level::Warn));
        assert_eq!(
            overrides,
            vec![(Category::Platform, true), (Category::Layout, false)]
        );
    }

    #[test]
    fn levels_are_ordered_so_higher_severity_passes_a_higher_threshold() {
        assert!((Level::Error as u8) > (Level::Warn as u8));
        assert!((Level::Warn as u8) > (Level::Info as u8));
        assert!((Level::Info as u8) > (Level::Debug as u8));
        assert!((Level::Debug as u8) > (Level::Trace as u8));
    }

    #[test]
    fn category_bits_are_distinct() {
        let cats = [
            Category::General,
            Category::Window,
            Category::EventLoop,
            Category::Input,
            Category::Layout,
            Category::Text,
            Category::DisplayList,
            Category::Rendering,
            Category::Resources,
            Category::Callbacks,
            Category::Timer,
            Category::DebugServer,
            Category::Platform,
        ];
        assert_eq!(cats.len(), CATEGORY_COUNT);
        let mut seen = 0u32;
        for c in cats {
            let bit = 1u32 << (c as u8);
            assert_eq!(seen & bit, 0, "duplicate bit for {c:?}");
            seen |= bit;
        }
    }

    #[test]
    fn runtime_toggles_take_effect() {
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        set_min_level(Some(Level::Warn));
        assert!(!DEBUG.load(Ordering::Relaxed));
        assert!(WARN.load(Ordering::Relaxed));
        assert!(ERROR.load(Ordering::Relaxed));
        assert!(level_enabled(Level::Error));
        assert!(!level_enabled(Level::Debug));

        set_min_level(Some(Level::Trace));
        assert!(TRACE.load(Ordering::Relaxed));
        assert!(level_enabled(Level::Trace));

        set_min_level(None);
        assert!(!level_enabled(Level::Error), "None must silence even Error");
        assert!(!ERROR.load(Ordering::Relaxed));

        // Restore a sane default for any other test in this binary.
        set_min_level(Some(Level::Debug));
    }

    #[test]
    fn silencing_one_category_leaves_the_others() {
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        set_min_level(Some(Level::Debug));
        set_category(Category::Layout, false);
        assert!(
            !enabled(Category::Layout, Level::Error),
            "silenced category ignores level"
        );
        assert!(enabled(Category::Platform, Level::Debug));
        set_category(Category::Layout, true);
        assert!(enabled(Category::Layout, Level::Debug));
    }
}
