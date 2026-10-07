//! The settings page's model - Outlook 2010's Options dialog, for every Azlin app: its
//! categories, the line over each category's options, the ids scripts find them by, and what
//! Cancel puts back. Plain Rust, tested without a window; `ui::settings_page` draws it.
//!
//! ```text
//! +--------------+---------------------------------------------------+
//! | Reading      | (icon) General options for working with AzNotes.  |  the header line
//! |--------------|                                                   |
//! |##General#####| Appearance                                        |  a section's band
//! | Data         |    Theme      [ Flat | Flora ]                    |  its rows
//! | Shortcuts    |    Mode       [ System | Light | Dark ]           |
//! |--------------|                                                   |
//! | About        |                                                   |
//! +--------------+---------------------------------------------------+
//!                                                      [ OK ] [ Cancel ]
//! ```
//!
//! A change takes effect at once (the theme repaints, `settings.json` is written), as on the
//! old page; OK keeps the changes and closes the page, Cancel (and Escape) puts back the
//! settings the page found ([`Snapshot`]) and closes it.

use crate::{
    args::{AppArgs, ModePref, Theme},
    settings::AppSettings,
};

/// The kit's categories, after the app's own: Appearance is "General", as in Outlook.
pub const KIT_CATEGORIES: [&str; 4] = ["General", "Data", "Shortcuts", "About"];

/// Category names older builds and callers use, and the category they are now.
pub const CATEGORY_ALIASES: [(&str, &str); 1] = [("Appearance", "General")];

/// What a category of the page is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Category {
    /// One of the app's own categories (an index into the app's categories).
    App(usize),
    /// The kit's General: the theme and the mode.
    General,
    /// The kit's Data: the data folder.
    Data,
    /// The kit's Shortcuts: the app's keyboard shortcuts, then the kit's.
    Shortcuts,
    /// The kit's About: the facts and the About box.
    About,
}

impl Category {
    /// The category at `index` of a page whose first `app_count` categories are the app's.
    #[must_use]
    pub fn of(index: usize, app_count: usize) -> Category {
        match index.checked_sub(app_count) {
            None => Category::App(index),
            Some(0) => Category::General,
            Some(1) => Category::Data,
            Some(2) => Category::Shortcuts,
            Some(_) => Category::About,
        }
    }

    /// The icon of the header line (a name of azul's icon set, Material Icons).
    #[must_use]
    pub fn icon(self) -> &'static str {
        match self {
            Category::App(_) => "tune",
            Category::General => "settings",
            Category::Data => "folder",
            Category::Shortcuts => "keyboard",
            Category::About => "info",
        }
    }

    /// The header line over the category's options ("General options for working with
    /// AzNotes."); `label` is the category's name in the list, `app` the app's name.
    #[must_use]
    pub fn header(self, label: &str, app: &str) -> String {
        match self {
            Category::App(_) => format!("{label} options for working with {app}."),
            Category::General => format!("General options for working with {app}."),
            Category::Data => format!("Where {app} keeps your data."),
            Category::Shortcuts => format!("The keyboard shortcuts of {app}."),
            Category::About => format!("The version, the licence and the data folder of {app}."),
        }
    }
}

/// Every category of the page: the app's, then the kit's.
#[must_use]
pub fn categories(app_categories: &[String]) -> Vec<String> {
    app_categories
        .iter()
        .cloned()
        .chain(KIT_CATEGORIES.iter().map(|c| (*c).to_string()))
        .collect()
}

/// The position of the category named `name`: the exact name first (an app's own category of
/// that name), else the category an old name stands for ("Appearance" is "General").
#[must_use]
pub fn category_index(categories: &[String], name: &str) -> Option<usize> {
    categories.iter().position(|c| c == name).or_else(|| {
        let (_, now) = CATEGORY_ALIASES.iter().find(|(old, _)| *old == name)?;
        categories.iter().position(|c| c == now)
    })
}

/// The DOM id of a category in the list, for scripts: `appkit-category-general`,
/// `appkit-category-keyboard-shortcuts` (lower case, a dash for each run of other characters).
#[must_use]
pub fn category_id(name: &str) -> String {
    let mut slug = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c.to_ascii_lowercase());
        } else if !slug.is_empty() && !slug.ends_with('-') {
            slug.push('-');
        }
    }
    while slug.ends_with('-') {
        slug.pop();
    }
    format!("appkit-category-{slug}")
}

/// The settings as the page found them when it opened: what Cancel puts back.
#[derive(Clone, Debug, PartialEq)]
pub struct Snapshot {
    settings: AppSettings,
    /// The `--theme` switch then (choosing a theme on the page drops it).
    theme_switch: Option<Theme>,
    /// The `--mode` switch then.
    mode_switch: Option<ModePref>,
}

/// What putting a [`Snapshot`] back changed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Restored {
    /// The settings differ from the file the page's changes wrote: write it again.
    pub save: bool,
    /// The theme and the mode to show again, if the page had changed them.
    pub look: Option<(Theme, ModePref)>,
}

impl Snapshot {
    /// The settings and the switches as they are now.
    #[must_use]
    pub fn take(settings: &AppSettings, args: &AppArgs) -> Snapshot {
        Snapshot {
            settings: settings.clone(),
            theme_switch: args.theme,
            mode_switch: args.mode,
        }
    }

    /// Puts the settings and the switches back as they were.
    #[must_use]
    pub fn restore(self, settings: &mut AppSettings, args: &mut AppArgs) -> Restored {
        let shown = settings.effective(args);
        let save = *settings != self.settings;
        *settings = self.settings;
        args.theme = self.theme_switch;
        args.mode = self.mode_switch;
        let again = settings.effective(args);
        Restored {
            save,
            look: (again != shown).then_some(again),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn the_apps_categories_come_first_then_general_data_shortcuts_about() {
        let all = categories(&names(&["Reading", "Library"]));
        assert_eq!(all, names(&["Reading", "Library", "General", "Data", "Shortcuts", "About"]));
        assert_eq!(Category::of(0, 2), Category::App(0));
        assert_eq!(Category::of(1, 2), Category::App(1));
        assert_eq!(Category::of(2, 2), Category::General);
        assert_eq!(Category::of(3, 2), Category::Data);
        assert_eq!(Category::of(4, 2), Category::Shortcuts);
        assert_eq!(Category::of(5, 2), Category::About);
        assert_eq!(Category::of(9, 2), Category::About, "past the end: the last one");
        assert_eq!(Category::of(0, 0), Category::General, "an app without categories");
    }

    #[test]
    fn appearance_is_found_as_general_but_an_exact_name_wins() {
        let all = categories(&names(&["Reading"]));
        assert_eq!(category_index(&all, "Appearance"), Some(1));
        assert_eq!(category_index(&all, "General"), Some(1));
        assert_eq!(category_index(&all, "Shortcuts"), Some(3));
        assert_eq!(category_index(&all, "Reading"), Some(0));
        assert_eq!(category_index(&all, "Nowhere"), None);
        let own = categories(&names(&["Appearance"]));
        assert_eq!(category_index(&own, "Appearance"), Some(0), "the app's own category");
    }

    #[test]
    fn the_header_line_names_the_app_like_outlooks() {
        assert_eq!(
            Category::General.header("General", "AzNotes"),
            "General options for working with AzNotes."
        );
        assert_eq!(
            Category::App(0).header("Reading", "AzReader"),
            "Reading options for working with AzReader."
        );
        for c in [
            Category::App(0),
            Category::General,
            Category::Data,
            Category::Shortcuts,
            Category::About,
        ] {
            assert!(c.header("X", "AzApp").contains("AzApp"), "{c:?}");
            assert!(!c.icon().is_empty(), "{c:?}");
        }
    }

    #[test]
    fn a_category_id_is_its_name_in_lower_case_with_dashes() {
        assert_eq!(category_id("General"), "appkit-category-general");
        assert_eq!(category_id("Keyboard shortcuts"), "appkit-category-keyboard-shortcuts");
        assert_eq!(category_id(" Mail & Calendar "), "appkit-category-mail-calendar");
        assert_eq!(category_id(""), "appkit-category-");
    }

    #[test]
    fn cancel_puts_back_the_theme_the_mode_and_the_apps_values() {
        let mut settings = AppSettings::default();
        settings.set("speed", "1000");
        let mut args = AppArgs::default();
        let snapshot = Snapshot::take(&settings, &args);
        settings.theme = Theme::Flora;
        settings.mode = ModePref::Dark;
        settings.set("speed", "2000");
        let restored = snapshot.restore(&mut settings, &mut args);
        assert_eq!(
            restored,
            Restored {
                save: true,
                look: Some((Theme::Flat, ModePref::System)),
            }
        );
        assert_eq!(settings.get("speed"), Some("1000"));
        assert_eq!(settings.theme, Theme::Flat);
    }

    #[test]
    fn cancel_without_a_change_writes_and_repaints_nothing() {
        let mut settings = AppSettings::default();
        let mut args = AppArgs::default();
        let snapshot = Snapshot::take(&settings, &args);
        let restored = snapshot.restore(&mut settings, &mut args);
        assert_eq!(
            restored,
            Restored {
                save: false,
                look: None,
            }
        );
    }

    #[test]
    fn cancel_brings_back_a_theme_switch_the_page_had_dropped() {
        // Started with --theme flora; the page's Flat dropped the switch (ui::choose_theme).
        let mut settings = AppSettings::default();
        let mut args = AppArgs {
            theme: Some(Theme::Flora),
            ..AppArgs::default()
        };
        let snapshot = Snapshot::take(&settings, &args);
        settings.theme = Theme::Flat;
        args.theme = None;
        let restored = snapshot.restore(&mut settings, &mut args);
        assert_eq!(args.theme, Some(Theme::Flora));
        assert_eq!(restored.look, Some((Theme::Flora, ModePref::System)));
        assert!(!restored.save, "the file never changed: Flat was its theme already");
    }
}
