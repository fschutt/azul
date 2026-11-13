//! AzPlayer's settings, Media Center's way (Windows Media Center's "settings": a page of big
//! lower-case categories, each opening a page of large, remote-friendly controls - check boxes,
//! radio lists, buttons, each a whole row to land on - with save and cancel at its bottom
//! right). The categories: general, pictures, music, videos, tv, library setup (each library's
//! folders: add, remove, look through them again), start-up & window, about.
//!
//! This is the model: the categories and their rows, the options' values - kept in the kit's
//! settings file (`player/settings.json`, the shared appkit file) under `player.*`, the library
//! folders under `player.folders.<library>` - and the DRAFT a category's page edits until save
//! (cancel and Back drop it). The look and light/dark mode the shared Azlin config owns are only
//! shown: AzPlayer is always its own dark Media Center. The words are said in the language of
//! the layout pass (a check box's and a choice's label is a key, said by [`Row::label`]); the
//! categories' titles and the buttons stay their English names (ids, markers), said by
//! `crate::l10n::word`. Plain Rust, tested without a window.

use std::{collections::BTreeMap, path::PathBuf};

use azul_appkit::l10n::{and_list, t, t_args, t_label, Arg};

use crate::{
    gallery::Step,
    library::{count, Shelf},
    pages::{Section, View},
};

// ==== The categories ====

/// A category of the settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Category {
    General,
    Pictures,
    Music,
    Videos,
    Tv,
    Library,
    Startup,
    About,
}

impl Category {
    /// The settings' list, top to bottom.
    pub const ALL: [Category; 8] = [
        Category::General,
        Category::Pictures,
        Category::Music,
        Category::Videos,
        Category::Tv,
        Category::Library,
        Category::Startup,
        Category::About,
    ];

    /// Its title (Media Center writes them in lower case).
    #[must_use]
    pub const fn title(self) -> &'static str {
        match self {
            Category::General => "general",
            Category::Pictures => "pictures",
            Category::Music => "music",
            Category::Videos => "videos",
            Category::Tv => "tv",
            Category::Library => "library setup",
            Category::Startup => "start-up & window",
            Category::About => "about",
        }
    }

    /// Its name in ids and markers (`page-settings-<key>`).
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Category::General => "general",
            Category::Pictures => "pictures",
            Category::Music => "music",
            Category::Videos => "videos",
            Category::Tv => "tv",
            Category::Library => "library",
            Category::Startup => "startup",
            Category::About => "about",
        }
    }

    /// Its icon (a Material icon name).
    #[must_use]
    pub const fn icon(self) -> &'static str {
        match self {
            Category::General => "tune",
            Category::Pictures => "photo_library",
            Category::Music => "library_music",
            Category::Videos => "video_library",
            Category::Tv => "live_tv",
            Category::Library => "folder_open",
            Category::Startup => "power_settings_new",
            Category::About => "info",
        }
    }

    /// What it holds, beside the list (a key).
    #[must_use]
    pub const fn blurb(self) -> &'static str {
        match self {
            Category::General => "azplayer-blurb-general",
            Category::Pictures => "azplayer-blurb-pictures",
            Category::Music => "azplayer-blurb-music",
            Category::Videos => "azplayer-blurb-videos",
            Category::Tv => "azplayer-blurb-tv",
            Category::Library => "azplayer-blurb-library",
            Category::Startup => "azplayer-blurb-startup",
            Category::About => "azplayer-blurb-about",
        }
    }

    /// Its place in [`Category::ALL`].
    #[must_use]
    pub fn index(self) -> usize {
        Category::ALL.iter().position(|c| *c == self).unwrap_or(0)
    }

    /// The buttons at the bottom right of its page: save and cancel, or (about) ok.
    #[must_use]
    pub const fn buttons(self) -> &'static [&'static str] {
        match self {
            Category::About => &["ok"],
            _ => &["save", "cancel"],
        }
    }
}

// ==== The options ====

/// The clock in the top band.
pub const SHOW_CLOCK: &str = "player.show_clock";
/// The back button and the transport hide while the pointer rests (always over a playing
/// video).
pub const HIDE_CHROME: &str = "player.hide_chrome";
/// The view each library's page opens on.
pub const PICTURES_VIEW: &str = "player.pictures_view";
pub const MUSIC_VIEW: &str = "player.music_view";
pub const VIDEOS_VIEW: &str = "player.videos_view";
pub const TV_VIEW: &str = "player.tv_view";
/// How long a slide shows, seconds; pan and zoom (Ken Burns) or a plain cross-fade.
pub const SLIDE_SECONDS: &str = "player.slide_seconds";
pub const KEN_BURNS: &str = "player.ken_burns";
/// "play all" shuffles the songs (else album by album).
pub const SHUFFLE_ALL: &str = "player.shuffle_all";
/// A video opens where it was left.
pub const RESUME: &str = "player.resume";
/// How far the skip buttons and Left / Right jump, seconds.
pub const SKIP_BACK: &str = "player.skip_back";
pub const SKIP_FORWARD: &str = "player.skip_forward";
/// The page AzPlayer opens on (a `--screen` switch wins for a run).
pub const START_SCREEN: &str = "player.start_screen";
/// AzPlayer opens in media only (fullscreen).
pub const START_FULLSCREEN: &str = "player.start_fullscreen";
/// The language of the words: the kit's setting (`azul_appkit::settings::LANGUAGE_KEY`, the
/// one every Azlin app keeps; `system`, `en`, `de`).
pub const LANGUAGE: &str = azul_appkit::settings::LANGUAGE_KEY;

/// One option: its key in the settings file, its default, the values it takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Spec {
    pub key: &'static str,
    pub default: &'static str,
    pub values: &'static [&'static str],
}

const YES_NO: &[&str] = &["true", "false"];

/// Every option.
pub const SPECS: [Spec; 15] = [
    Spec {
        key: SHOW_CLOCK,
        default: "true",
        values: YES_NO,
    },
    Spec {
        key: HIDE_CHROME,
        default: "true",
        values: YES_NO,
    },
    Spec {
        key: PICTURES_VIEW,
        default: "folders",
        values: &["folders", "months"],
    },
    Spec {
        key: MUSIC_VIEW,
        default: "albums",
        values: &["albums", "artists", "genres", "songs"],
    },
    Spec {
        key: VIDEOS_VIEW,
        default: "folders",
        values: &["folders", "months", "titles"],
    },
    Spec {
        key: TV_VIEW,
        default: "months",
        values: &["months", "titles"],
    },
    Spec {
        key: SLIDE_SECONDS,
        default: "6",
        values: &["3", "6", "10", "20"],
    },
    Spec {
        key: KEN_BURNS,
        default: "true",
        values: YES_NO,
    },
    Spec {
        key: SHUFFLE_ALL,
        default: "true",
        values: YES_NO,
    },
    Spec {
        key: RESUME,
        default: "true",
        values: YES_NO,
    },
    Spec {
        key: SKIP_BACK,
        default: "10",
        values: &["5", "10", "30"],
    },
    Spec {
        key: SKIP_FORWARD,
        default: "30",
        values: &["10", "30", "60"],
    },
    Spec {
        key: START_SCREEN,
        default: "start",
        values: &["start", "music", "pictures", "videos", "recent"],
    },
    Spec {
        key: START_FULLSCREEN,
        default: "false",
        values: YES_NO,
    },
    Spec {
        key: LANGUAGE,
        default: "system",
        values: &["system", "en", "de"],
    },
];

/// The option `key`'s spec.
#[must_use]
pub fn spec(key: &str) -> Option<&'static Spec> {
    SPECS.iter().find(|s| s.key == key)
}

/// The options' values (a value not set is its default).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Options {
    values: BTreeMap<&'static str, String>,
}

impl Options {
    /// The options from the settings file's values: a value an option does not take is its
    /// default (a file from a newer build, a hand edit).
    #[must_use]
    pub fn read(values: &BTreeMap<String, String>) -> Options {
        let mut options = Options::default();
        for spec in &SPECS {
            if let Some(v) = values.get(spec.key) {
                options.set(spec.key, v.trim());
            }
        }
        options
    }

    /// The value of `key` ("" for an option there is not).
    #[must_use]
    pub fn get(&self, key: &str) -> &str {
        match self.values.get(key) {
            Some(v) => v.as_str(),
            None => spec(key).map_or("", |s| s.default),
        }
    }

    /// Sets `key` to `value` when it is an option and takes the value. `true`: it changed.
    pub fn set(&mut self, key: &str, value: &str) -> bool {
        let Some(spec) = spec(key) else {
            return false;
        };
        if !spec.values.contains(&value) || self.get(key) == value {
            return false;
        }
        self.values.insert(spec.key, value.to_string());
        true
    }

    /// A yes / no option.
    #[must_use]
    pub fn is_on(&self, key: &str) -> bool {
        self.get(key) == "true"
    }

    /// Turns a yes / no option over. Its new value.
    pub fn toggle(&mut self, key: &str) -> bool {
        let on = !self.is_on(key);
        self.set(key, if on { "true" } else { "false" });
        on
    }

    /// A number of seconds.
    #[must_use]
    pub fn seconds(&self, key: &str) -> u64 {
        self.get(key).parse().unwrap_or(0)
    }

    /// What `self` has that `before` has not: what a save writes into the file.
    #[must_use]
    pub fn changes(&self, before: &Options) -> Vec<(&'static str, String)> {
        SPECS
            .iter()
            .filter(|s| self.get(s.key) != before.get(s.key))
            .map(|s| (s.key, self.get(s.key).to_string()))
            .collect()
    }

    /// The view `section`'s page opens on (an index into its views).
    #[must_use]
    pub fn view_of(&self, section: Section) -> usize {
        let key = match section {
            Section::Music => MUSIC_VIEW,
            Section::Pictures => PICTURES_VIEW,
            Section::Videos => VIDEOS_VIEW,
            Section::Tv => TV_VIEW,
            Section::Movies | Section::Recent => return 0,
        };
        let view = match self.get(key) {
            "artists" => View::Artists,
            "genres" => View::Genres,
            "songs" => View::Songs,
            "months" => View::Months,
            "titles" => View::Titles,
            "folders" => View::Folders,
            _ => View::Albums,
        };
        section
            .views()
            .iter()
            .position(|v| *v == view)
            .unwrap_or(0)
    }
}

// ==== The library folders ====

/// The folders the libraries read: each library one or more (library setup adds and removes
/// them).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Folders {
    pub music: Vec<PathBuf>,
    pub pictures: Vec<PathBuf>,
    pub videos: Vec<PathBuf>,
    pub tv: Vec<PathBuf>,
}

impl Folders {
    /// The folders of `shelf`.
    #[must_use]
    pub fn of(&self, shelf: Shelf) -> &[PathBuf] {
        match shelf {
            Shelf::Music => &self.music,
            Shelf::Pictures => &self.pictures,
            Shelf::Videos => &self.videos,
            Shelf::Tv => &self.tv,
        }
    }

    /// The folders of `shelf`, to change.
    pub fn of_mut(&mut self, shelf: Shelf) -> &mut Vec<PathBuf> {
        match shelf {
            Shelf::Music => &mut self.music,
            Shelf::Pictures => &mut self.pictures,
            Shelf::Videos => &mut self.videos,
            Shelf::Tv => &mut self.tv,
        }
    }

    /// The folders of `shelf` for a sentence ("/m and /n").
    #[must_use]
    pub fn shown(&self, shelf: Shelf) -> String {
        let names: Vec<String> = self
            .of(shelf)
            .iter()
            .map(|p| p.display().to_string())
            .collect();
        if names.is_empty() {
            t("azplayer-no-folder")
        } else {
            and_list(names)
        }
    }

    /// Adds `folder` to `shelf` (not twice). `true`: added.
    pub fn add(&mut self, shelf: Shelf, folder: PathBuf) -> bool {
        let list = self.of_mut(shelf);
        if folder.as_os_str().is_empty() || list.contains(&folder) {
            return false;
        }
        list.push(folder);
        true
    }

    /// Removes `folder` from `shelf`. `true`: removed.
    pub fn remove(&mut self, shelf: Shelf, folder: &std::path::Path) -> bool {
        let list = self.of_mut(shelf);
        let before = list.len();
        list.retain(|p| p != folder);
        list.len() != before
    }
}

/// The key a library's folders are kept under.
#[must_use]
pub const fn folders_key(shelf: Shelf) -> &'static str {
    match shelf {
        Shelf::Music => "player.folders.music",
        Shelf::Pictures => "player.folders.pictures",
        Shelf::Videos => "player.folders.videos",
        Shelf::Tv => "player.folders.tv",
    }
}

/// The folders as kept: one a line.
#[must_use]
pub fn folders_text(list: &[PathBuf]) -> String {
    list.iter()
        .map(|p| p.display().to_string())
        .collect::<Vec<_>>()
        .join("\n")
}

/// The libraries' folders the settings file keeps (an empty list: the library's default).
#[must_use]
pub fn saved_folders(values: &BTreeMap<String, String>) -> Folders {
    let mut folders = Folders::default();
    for shelf in Shelf::ALL {
        if let Some(text) = values.get(folders_key(shelf)) {
            *folders.of_mut(shelf) = text
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(PathBuf::from)
                .collect();
        }
    }
    folders
}

// ==== A category's page ====

/// What a button of library setup does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Press {
    /// Adds a folder to a library (the folder dialog).
    AddFolder(Shelf),
    /// Looks through every library's folders again.
    Rescan,
}

/// A row of a category's page.
#[derive(Debug, Clone, PartialEq)]
pub enum Row {
    /// A heading over the rows after it.
    Heading(String),
    /// A sentence: what something means, a fact.
    Note(String),
    /// A check box: the yes / no option `key` (`label`: a key).
    Check {
        key: &'static str,
        label: &'static str,
    },
    /// One choice of a radio list: the option `key` takes `value` (`label`: a key, given the
    /// value as `$count` - "{ $count } seconds").
    Radio {
        key: &'static str,
        value: &'static str,
        label: &'static str,
    },
    /// A library folder: Enter asks to remove it.
    Folder { shelf: Shelf, path: PathBuf },
    /// A button.
    Button {
        label: String,
        icon: &'static str,
        press: Press,
    },
}

impl Row {
    /// Whether the keys can land on it.
    #[must_use]
    pub fn focusable(&self) -> bool {
        !matches!(self, Row::Heading(_) | Row::Note(_))
    }

    /// Its height on the page (logical px): every row of a kind as high, so the page knows
    /// where the focused one is.
    #[must_use]
    pub fn height(&self) -> f32 {
        match self {
            Row::Heading(_) => 46.0,
            Row::Note(_) => 54.0,
            _ => 52.0,
        }
    }

    /// What it says, in the language of the layout pass (also `AZPLAYER_FOCUS setting`).
    #[must_use]
    pub fn label(&self) -> String {
        match self {
            Row::Heading(said) | Row::Note(said) => said.clone(),
            Row::Check { label, .. } => t_label(label),
            Row::Radio { value, label, .. } => t_args(
                label,
                &[("count", Arg::from(value.parse::<i64>().unwrap_or(0)))],
            ),
            Row::Folder { path, .. } => path.display().to_string(),
            Row::Button { label, .. } => label.clone(),
        }
    }
}

/// What the general and about pages say about the rest of the world.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Facts {
    /// AzPlayer's version.
    pub version: String,
    /// The look the shared Azlin config gives the other apps ("Flora", "Dark").
    pub theme: String,
    pub mode: String,
    /// Where AzPlayer's data is.
    pub data: String,
    /// How many songs, pictures, videos and recordings the libraries hold.
    pub counts: [usize; 4],
}

/// A radio list's choices.
fn radios(key: &'static str, choices: &[(&'static str, &'static str)]) -> Vec<Row> {
    choices
        .iter()
        .map(|&(value, label)| Row::Radio { key, value, label })
        .collect()
}

/// The rows of `category`'s page; library setup lists `folders` (the draft's). Headings and
/// notes are said here, in the language of the layout pass.
#[must_use]
pub fn rows(category: Category, folders: &Folders, facts: &Facts) -> Vec<Row> {
    let h = |key: &str| Row::Heading(t(key));
    let n = |key: &str| Row::Note(t(key));
    let library = |shelf: Shelf| Arg::from(crate::l10n::word(shelf.word()));
    let mut out = Vec::new();
    match category {
        Category::General => {
            out.push(h("azplayer-setting-pointer"));
            out.push(Row::Check {
                key: HIDE_CHROME,
                label: "azplayer-setting-hide-chrome",
            });
            out.push(h("azplayer-setting-top-of-window"));
            out.push(Row::Check {
                key: SHOW_CLOCK,
                label: "azplayer-setting-show-clock",
            });
            out.push(h("azplayer-setting-look"));
            out.push(Row::Note(t_args(
                "azplayer-setting-look-note",
                &[
                    ("theme", Arg::from(&facts.theme)),
                    ("mode", Arg::from(&facts.mode)),
                ],
            )));
            out.push(h("azplayer-setting-language"));
            out.extend(radios(
                LANGUAGE,
                &[
                    ("system", "azplayer-language-system"),
                    ("en", "kit-language-english"),
                    ("de", "kit-language-german"),
                ],
            ));
        }
        Category::Pictures => {
            out.push(h("azplayer-setting-pictures-open-on"));
            out.extend(radios(
                PICTURES_VIEW,
                &[
                    ("folders", "azplayer-word-folders"),
                    ("months", "azplayer-word-date-taken"),
                ],
            ));
            out.push(h("azplayer-setting-slide-for"));
            out.extend(radios(
                SLIDE_SECONDS,
                &[
                    ("3", "azplayer-seconds"),
                    ("6", "azplayer-seconds"),
                    ("10", "azplayer-seconds"),
                    ("20", "azplayer-seconds"),
                ],
            ));
            out.push(Row::Check {
                key: KEN_BURNS,
                label: "azplayer-setting-ken-burns",
            });
        }
        Category::Music => {
            out.push(h("azplayer-setting-music-open-on"));
            out.extend(radios(
                MUSIC_VIEW,
                &[
                    ("albums", "azplayer-word-albums"),
                    ("artists", "azplayer-word-artists"),
                    ("genres", "azplayer-word-genres"),
                    ("songs", "azplayer-word-songs"),
                ],
            ));
            out.push(h("azplayer-word-play-all"));
            out.push(Row::Check {
                key: SHUFFLE_ALL,
                label: "azplayer-setting-shuffle-all",
            });
        }
        Category::Videos => {
            out.push(h("azplayer-setting-videos-open-on"));
            out.extend(radios(
                VIDEOS_VIEW,
                &[
                    ("folders", "azplayer-word-folders"),
                    ("months", "azplayer-word-date"),
                    ("titles", "azplayer-word-title"),
                ],
            ));
            out.push(h("azplayer-setting-playing"));
            out.push(Row::Check {
                key: RESUME,
                label: "azplayer-setting-resume",
            });
            out.push(h("azplayer-setting-skip-back"));
            out.extend(radios(
                SKIP_BACK,
                &[
                    ("5", "azplayer-seconds"),
                    ("10", "azplayer-seconds"),
                    ("30", "azplayer-seconds"),
                ],
            ));
            out.push(h("azplayer-setting-skip-forward"));
            out.extend(radios(
                SKIP_FORWARD,
                &[
                    ("10", "azplayer-seconds"),
                    ("30", "azplayer-seconds"),
                    ("60", "azplayer-setting-a-minute"),
                ],
            ));
        }
        Category::Tv => {
            out.push(h("azplayer-setting-live-tv"));
            out.push(n("azplayer-setting-no-tuner"));
            out.push(h("azplayer-setting-tv-open-on"));
            out.extend(radios(
                TV_VIEW,
                &[
                    ("months", "azplayer-word-date-recorded"),
                    ("titles", "azplayer-word-title"),
                ],
            ));
        }
        Category::Library => {
            for shelf in Shelf::ALL {
                out.push(Row::Heading(t_args(
                    "azplayer-setting-folders-of",
                    &[("library", library(shelf))],
                )));
                for path in folders.of(shelf) {
                    out.push(Row::Folder {
                        shelf,
                        path: path.clone(),
                    });
                }
                out.push(Row::Button {
                    label: t_args(
                        "azplayer-setting-add-folder",
                        &[("library", library(shelf))],
                    ),
                    icon: "create_new_folder",
                    press: Press::AddFolder(shelf),
                });
            }
            out.push(h("azplayer-setting-libraries"));
            out.push(Row::Button {
                label: t("azplayer-setting-rescan"),
                icon: "refresh",
                press: Press::Rescan,
            });
            out.push(n("azplayer-setting-folders-note"));
        }
        Category::Startup => {
            out.push(h("azplayer-setting-opens-on"));
            out.extend(radios(
                START_SCREEN,
                &[
                    ("start", "azplayer-setting-start-screen"),
                    ("music", "azplayer-word-music"),
                    ("pictures", "azplayer-word-pictures"),
                    ("videos", "azplayer-word-videos"),
                    ("recent", "azplayer-word-recently-played"),
                ],
            ));
            out.push(h("azplayer-setting-window"));
            out.push(Row::Check {
                key: START_FULLSCREEN,
                label: "azplayer-setting-start-fullscreen",
            });
        }
        Category::About => {
            out.push(Row::Heading(t_args(
                "azplayer-setting-about-title",
                &[("version", Arg::from(&facts.version))],
            )));
            out.push(n("azplayer-setting-about-note"));
            let counts = and_list(vec![
                count("azplayer-count-songs", facts.counts[0]),
                count("azplayer-count-pictures", facts.counts[1]),
                count("azplayer-count-videos", facts.counts[2]),
                count("azplayer-count-recordings", facts.counts[3]),
            ]);
            out.push(Row::Note(t_args(
                "azplayer-setting-about-counts",
                &[
                    ("counts", Arg::from(counts)),
                    ("data", Arg::from(&facts.data)),
                ],
            )));
            out.push(h("azplayer-setting-keys"));
            out.push(n("azplayer-setting-keys-note"));
            out.push(n("azplayer-setting-keys-note-more"));
        }
    }
    out
}

/// The places of the rows the keys can land on.
#[must_use]
pub fn focusable(rows: &[Row]) -> Vec<usize> {
    rows.iter()
        .enumerate()
        .filter(|(_, r)| r.focusable())
        .map(|(i, _)| i)
        .collect()
}

/// Where the keys go from `index` on a page with `n` rows to land on and `b` buttons after
/// them (`index` n.. is a button): Up / Down along the rows, Down from the last row to the
/// first button, Left / Right along the buttons, Up from a button to the last row. Nothing
/// goes round.
#[must_use]
pub fn step_page(index: usize, n: usize, b: usize, step: Step) -> usize {
    let last = (n + b).saturating_sub(1);
    let index = index.min(last);
    match step {
        Step::Up if index >= n => n.checked_sub(1).unwrap_or(index),
        Step::Up => index.saturating_sub(1),
        Step::Down if index < n => index + 1,
        Step::Left if index > n => index - 1,
        Step::Right if index >= n && index < last => index + 1,
        _ => index,
    }
}

/// Where the focused row stands: the page's rows scrolled by this much (logical px) keep it in
/// a view `visible` high.
#[must_use]
pub fn scroll_for(rows: &[Row], focused_row: Option<usize>, visible: f32) -> f32 {
    let Some(row) = focused_row else {
        return 0.0;
    };
    let top: f32 = rows.iter().take(row).map(Row::height).sum();
    let bottom = top + rows.get(row).map_or(0.0, Row::height);
    (bottom + 24.0 - visible).max(0.0)
}

/// What a category's page edits until save: the options and the library folders.
#[derive(Debug, Clone, PartialEq)]
pub struct Draft {
    pub category: Category,
    pub options: Options,
    pub folders: Folders,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn values(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect()
    }

    #[test]
    fn the_options_read_their_values_and_keep_the_defaults_for_the_rest() {
        crate::l10n::in_english();
        let o = Options::read(&values(&[
            (SHOW_CLOCK, "false"),
            (SLIDE_SECONDS, "10"),
            (SKIP_BACK, "7"),
            ("player.nonsense", "x"),
        ]));
        assert!(!o.is_on(SHOW_CLOCK));
        assert_eq!(o.seconds(SLIDE_SECONDS), 10);
        assert_eq!(o.seconds(SKIP_BACK), 10, "a value it does not take is the default");
        assert!(o.is_on(RESUME), "a value not set is the default");
        assert_eq!(o.get("player.nonsense"), "");
        let mut m = o.clone();
        assert!(m.set(MUSIC_VIEW, "songs"));
        assert!(!m.set(MUSIC_VIEW, "songs"), "no change");
        assert!(!m.set(MUSIC_VIEW, "movies"), "not a music view");
        assert!(m.toggle(SHOW_CLOCK));
        let changes = m.changes(&o);
        assert_eq!(changes.len(), 2, "what a save writes: {changes:?}");
        assert!(changes.contains(&(SHOW_CLOCK, String::from("true"))));
        assert!(changes.contains(&(MUSIC_VIEW, String::from("songs"))));
        assert_eq!(m.view_of(Section::Music), 3, "songs");
        assert_eq!(o.view_of(Section::Pictures), 0, "folders");
        assert_eq!(Options::default().view_of(Section::Tv), 0, "date recorded");
    }

    #[test]
    fn the_folders_round_trip_through_the_settings_file() {
        crate::l10n::in_english();
        let mut f = Folders::default();
        assert!(f.add(Shelf::Pictures, PathBuf::from("/p")));
        assert!(f.add(Shelf::Pictures, PathBuf::from("/q/Trip")));
        assert!(!f.add(Shelf::Pictures, PathBuf::from("/p")), "not twice");
        let text = folders_text(f.of(Shelf::Pictures));
        let back = saved_folders(&values(&[(folders_key(Shelf::Pictures), text.as_str())]));
        assert_eq!(back.pictures, f.pictures);
        assert!(back.music.is_empty(), "nothing kept: the default folder");
        assert_eq!(f.shown(Shelf::Pictures), "/p and /q/Trip");
        assert!(f.remove(Shelf::Pictures, std::path::Path::new("/p")));
        assert!(!f.remove(Shelf::Pictures, std::path::Path::new("/p")));
        assert_eq!(f.shown(Shelf::Music), "no folder");
    }

    #[test]
    fn every_page_has_rows_to_land_on_and_its_buttons() {
        crate::l10n::in_english();
        let facts = Facts::default();
        let mut folders = Folders::default();
        folders.music.push(PathBuf::from("/m"));
        for category in Category::ALL {
            let rows = rows(category, &folders, &facts);
            assert!(!rows.is_empty(), "{} has rows", category.title());
            assert_eq!(category.title(), category.title().to_lowercase());
            if category != Category::About {
                assert!(!focusable(&rows).is_empty(), "{} has controls", category.title());
                assert_eq!(category.buttons(), &["save", "cancel"]);
            }
            for row in &rows {
                if let Row::Check { key, .. } | Row::Radio { key, .. } = row {
                    assert!(spec(key).is_some(), "{key} is an option");
                }
                if let Row::Radio { key, value, .. } = row {
                    assert!(spec(key).unwrap().values.contains(value), "{key} takes {value}");
                }
            }
        }
        let library = rows(Category::Library, &folders, &facts);
        assert!(library.contains(&Row::Folder {
            shelf: Shelf::Music,
            path: PathBuf::from("/m")
        }));
        assert_eq!(Category::About.buttons(), &["ok"]);
        assert_eq!(Category::Startup.index(), 6);
    }

    #[test]
    fn the_keys_walk_the_rows_then_the_buttons_and_never_round() {
        crate::l10n::in_english();
        // Three rows, save and cancel.
        assert_eq!(step_page(0, 3, 2, Step::Up), 0);
        assert_eq!(step_page(0, 3, 2, Step::Down), 1);
        assert_eq!(step_page(2, 3, 2, Step::Down), 3, "the last row: down to save");
        assert_eq!(step_page(3, 3, 2, Step::Right), 4, "save: right to cancel");
        assert_eq!(step_page(4, 3, 2, Step::Right), 4);
        assert_eq!(step_page(4, 3, 2, Step::Left), 3);
        assert_eq!(step_page(3, 3, 2, Step::Left), 3);
        assert_eq!(step_page(4, 3, 2, Step::Up), 2, "a button: up to the last row");
        assert_eq!(step_page(3, 3, 2, Step::Down), 3);
        // No rows (about): the button.
        assert_eq!(step_page(0, 0, 1, Step::Up), 0);
        assert_eq!(step_page(0, 0, 1, Step::Down), 0);
        let rows = vec![
            Row::Heading(String::new()),
            Row::Check {
                key: SHOW_CLOCK,
                label: "",
            },
        ];
        assert_eq!(scroll_for(&rows, Some(1), 1000.0), 0.0, "in view: no scroll");
        assert!(scroll_for(&rows, Some(1), 60.0) > 0.0, "below the view: scrolled to it");
    }
}
