#[allow(unused_imports)]
pub use super::*;

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use super::*;

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

    /// What a filter that was configured from the environment `env` answers, in the words
    /// `the_environment_decides_the_level_and_the_echo` lists them: the level, the echo, whether
    /// Layout and Platform are on, whether a Layout Debug record and a Layout Error record pass.
    fn configured_by(env: &[(&str, &str)]) -> String {
        let var = |name: &str| {
            env.iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| OsString::from(*value))
        };
        let filter = LogFilter::new();
        filter.apply(&LogSettings::from_environment(&var));
        format!(
            "level={:?} echo={} layout={} platform={} debug_passes={} error_passes={}",
            filter.min_level(),
            filter.stderr_echo(),
            filter.category_enabled(Category::Layout),
            filter.category_enabled(Category::Platform),
            filter.enabled(Category::Layout, Level::Debug),
            filter.enabled(Category::Layout, Level::Error),
        )
    }

    /// REGRESSION (2026-11-20, a 612 MB log of a two-minute editor session):
    /// with `AZ_LOG` unset an ordinary lean build printed the whole Debug
    /// firehose (3.4 million lines, 60 s of the 124 s). A process that asked for
    /// no logs is at Error and echoes nothing else; every way of asking
    /// still works.
    ///
    /// The environment is handed to the settings, and the settings to a filter of this
    /// test's own: nothing here reads or writes the process's environment or its filter.
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
            let want = format!(
                "level={level} echo={echo} layout={layout} platform=true debug_passes={debug_passes} \
                 error_passes={}",
                *level != "None" && layout.to_owned()
            );
            assert_eq!(configured_by(env), want, "environment {env:?}");
        }
    }

    /// The two readings of the process's environment agree: the level the filter is configured to
    /// and the level the `log` facade's own logger takes (`level_from_env`).
    #[test]
    fn the_settings_of_the_real_environment_name_the_level_the_facade_takes() {
        let settings = LogSettings::from_environment(&|name| std::env::var_os(name));
        assert_eq!(settings.level, level_from_env());
    }

    /// A filter configured by settings that leave the echo open (`None`) keeps the echo it had.
    #[test]
    fn settings_that_name_no_echo_leave_the_echo_as_it_is() {
        let filter = LogFilter::new();
        filter.set_stderr_echo(true);
        filter.apply(&LogSettings {
            level: Some(Level::Warn),
            overrides: Vec::new(),
            stderr_echo: None,
        });
        assert!(filter.stderr_echo());
        filter.apply(&LogSettings {
            level: Some(Level::Warn),
            overrides: Vec::new(),
            stderr_echo: Some(false),
        });
        assert!(!filter.stderr_echo());
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

    /// A filter nothing configured passes nothing, and says so for every level and category.
    #[test]
    fn a_new_filter_passes_nothing() {
        let filter = LogFilter::new();
        assert_eq!(filter.min_level(), None);
        assert!(!filter.level_enabled(Level::Error));
        assert!(!filter.enabled(Category::General, Level::Error));
        assert!(!filter.stderr_echo());
    }

    /// The level and the echo of one filter move no other: the tests that flip them take a filter
    /// each, and the process's is not among them.
    #[test]
    fn runtime_toggles_take_effect() {
        let filter = LogFilter::new();
        let other = LogFilter::new();
        filter.set_min_level(Some(Level::Warn));
        assert!(filter.level_enabled(Level::Warn));
        assert!(filter.level_enabled(Level::Error));
        assert!(!filter.level_enabled(Level::Info));
        assert!(!filter.level_enabled(Level::Debug));

        filter.set_min_level(Some(Level::Trace));
        assert!(filter.level_enabled(Level::Trace));

        filter.set_min_level(None);
        assert!(!filter.level_enabled(Level::Error), "None must silence even Error");

        assert_eq!(other.min_level(), None, "a filter of its own is not moved by another");
    }

    #[test]
    fn silencing_one_category_leaves_the_others() {
        let filter = LogFilter::new();
        filter.set_min_level(Some(Level::Debug));
        filter.set_category(Category::Layout, false);
        assert!(
            !filter.enabled(Category::Layout, Level::Error),
            "silenced category ignores level"
        );
        assert!(filter.enabled(Category::Platform, Level::Debug));
        filter.set_category(Category::Layout, true);
        assert!(filter.enabled(Category::Layout, Level::Debug));
    }
}
