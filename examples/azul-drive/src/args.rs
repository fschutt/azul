//! AzDrive's command line: `--screen <this-pc|quick-access|home|settings>`,
//! `--theme <flat|flora>`, `--mode <light|dark>`, `--layout <name>`,
//! `--sample` (sample files in the Home drive), like AzWriter's switches.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ViewLayout;

    fn parse(args: &[&str]) -> Result<Args, String> {
        Args::parse(args.iter().map(|s| s.to_string()))
    }

    #[test]
    fn no_switches_start_where_the_settings_say() {
        let args = parse(&[]).unwrap();
        assert_eq!(args.screen, Screen::Default);
        assert_eq!(args.theme, None);
        assert_eq!(args.dark, None);
        assert_eq!(args.layout, None);
        assert!(!args.sample);
    }

    #[test]
    fn every_switch_reads_its_value_with_a_space_or_an_equals_sign() {
        let args = parse(&[
            "--screen",
            "home",
            "--theme=flora",
            "--mode",
            "dark",
            "--layout",
            "large-icons",
            "--sample",
        ])
        .unwrap();
        assert_eq!(args.screen, Screen::Home);
        assert_eq!(args.theme.as_deref(), Some("flora"));
        assert_eq!(args.dark, Some(true));
        assert_eq!(args.layout, Some(ViewLayout::LargeIcons));
        assert!(args.sample);
        assert_eq!(parse(&["--mode", "light"]).unwrap().dark, Some(false));
        assert_eq!(
            parse(&["--screen", "quick-access"]).unwrap().screen,
            Screen::QuickAccess
        );
        assert_eq!(
            parse(&["--screen=settings"]).unwrap().screen,
            Screen::Settings
        );
    }

    #[test]
    fn a_wrong_switch_or_value_says_what_is_expected() {
        assert!(parse(&["--screen", "moon"]).unwrap_err().contains("this-pc"));
        assert!(parse(&["--theme", "neon"]).unwrap_err().contains("flora"));
        assert!(parse(&["--mode"]).unwrap_err().contains("--mode"));
        assert!(parse(&["--layout", "huge"]).unwrap_err().contains("details"));
        assert!(parse(&["--what"]).unwrap_err().contains("unknown"));
        assert!(parse(&["--help"]).unwrap_err().contains("USAGE"));
    }

    #[test]
    fn the_sample_files_land_in_the_home_folder_and_a_second_run_keeps_them() {
        let dir = std::env::temp_dir().join(format!("azdrive-sample-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let written = write_sample(&dir).unwrap();
        assert!(written >= 6, "{written}");
        assert!(dir.join("Documents").join("notes.txt").is_file());
        assert!(dir.join("Pictures").join("gradient.png").is_file());
        assert!(dir.join("Projects").is_dir(), "an empty folder too");
        std::fs::write(dir.join("Documents").join("notes.txt"), b"mine").unwrap();
        write_sample(&dir).unwrap();
        assert_eq!(
            std::fs::read(dir.join("Documents").join("notes.txt")).unwrap(),
            b"mine",
            "the user's file is never overwritten"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
