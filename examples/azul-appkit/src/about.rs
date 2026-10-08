//! What an app's About section says (build ledger F6).
//!
//! The facts, as label / value rows, so the settings page's About section
//! and the About box (azul's `AboutDialog`, `ui::about_modal`) show the same
//! thing.

use std::path::Path;

/// The facts of one app.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AboutInfo {
    /// `AzCalculator`.
    pub name: &'static str,
    /// The crate version (`env!("CARGO_PKG_VERSION")`).
    pub version: &'static str,
    /// One sentence: what the app is for.
    pub summary: &'static str,
    /// `MIT`.
    pub license: &'static str,
    /// The app's folder in the data root (`calculator`).
    pub app_folder: &'static str,
}

/// The About rows: version, license, where the data lives, the toolkit,
/// the icons the kit gives every app (Haiku's, the flora theme's -
/// `ui::app_config` registers them).
#[must_use]
pub fn about_rows(info: &AboutInfo, data_root: &Path) -> Vec<(String, String)> {
    let (icons, icons_license) = azul_icons_haiku::CREDIT;
    vec![
        ("Version".to_string(), info.version.to_string()),
        ("License".to_string(), info.license.to_string()),
        (
            "Data folder".to_string(),
            crate::data::local_path(data_root, info.app_folder)
                .display()
                .to_string(),
        ),
        ("Built with".to_string(), "azul (azul.rs)".to_string()),
        ("Icons".to_string(), format!("{icons}, {icons_license}")),
    ]
}

/// The window title: the app's name, with the screen when it is not the main one.
#[must_use]
pub fn window_title(app: &str, screen: Option<&str>) -> String {
    match screen.map(str::trim).filter(|s| !s.is_empty()) {
        Some(screen) => format!("{screen} - {app}"),
        None => app.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const INFO: AboutInfo = AboutInfo {
        name: "AzTest",
        version: "0.1.0",
        summary: "tests",
        license: "MIT",
        app_folder: "test",
    };

    #[test]
    fn the_about_rows_name_the_version_license_and_data_folder() {
        let rows = about_rows(&INFO, Path::new("/data"));
        let labels: Vec<&str> = rows.iter().map(|(l, _)| l.as_str()).collect();
        assert_eq!(
            labels,
            vec!["Version", "License", "Data folder", "Built with", "Icons"]
        );
        // Haiku's icons are MIT: every app that ships them says so.
        assert_eq!(rows[4].1, "Haiku's icon set (Haiku, Inc.), MIT");
        assert_eq!(rows[0].1, "0.1.0");
        assert_eq!(
            rows[2].1,
            std::path::PathBuf::from("/data/test").display().to_string()
        );
    }

    #[test]
    fn the_title_names_the_screen_before_the_app() {
        assert_eq!(window_title("AzTest", None), "AzTest");
        assert_eq!(
            window_title("AzTest", Some("Settings")),
            "Settings - AzTest"
        );
        assert_eq!(window_title("AzTest", Some(" ")), "AzTest");
    }
}
