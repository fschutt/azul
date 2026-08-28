
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unset_defaults_to_debug_and_keeps_every_category() {
        let (level, overrides) = parse("");
        assert_eq!(level, Some(Level::Debug));
        assert!(overrides.is_empty());
    }

    #[test]
    fn off_spellings_disable_logging() {
        for raw in ["off", "0", "false", "none", "no", "disable", "disabled", "  OFF  "] {
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
        assert_eq!(overrides, vec![(Category::Platform, true), (Category::Layout, false)]);
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
            Category::General, Category::Window, Category::EventLoop, Category::Input,
            Category::Layout, Category::Text, Category::DisplayList, Category::Rendering,
            Category::Resources, Category::Callbacks, Category::Timer, Category::DebugServer,
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
        set_min_level(Some(Level::Debug));
        set_category(Category::Layout, false);
        assert!(!enabled(Category::Layout, Level::Error), "silenced category ignores level");
        assert!(enabled(Category::Platform, Level::Debug));
        set_category(Category::Layout, true);
        assert!(enabled(Category::Layout, Level::Debug));
    }
}
