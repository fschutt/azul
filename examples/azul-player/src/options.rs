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
//! shown: AzPlayer is always its own dark Media Center. Plain Rust, tested without a window.

use std::{collections::BTreeMap, path::PathBuf};

use crate::{
    gallery::Step,
    library::Shelf,
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

    /// What it holds, beside the list.
    #[must_use]
    pub const fn blurb(self) -> &'static str {
        match self {
            Category::General => "The clock, the back button and the controls, the look.",
            Category::Pictures => "What the picture library shows first; the slide show.",
            Category::Music => "What the music library shows first; play all.",
            Category::Videos => "What the video library shows first; resuming, skipping.",
            Category::Tv => "Recorded TV; live TV.",
            Category::Library => "The folders each library is read from.",
            Category::Startup => "What AzPlayer opens on; media only.",
            Category::About => "AzPlayer, its data, the keys.",
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

/// One option: its key in the settings file, its default, the values it takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Spec {
    pub key: &'static str,
    pub default: &'static str,
    pub values: &'static [&'static str],
}

const YES_NO: &[&str] = &["true", "false"];

/// Every option.
pub const SPECS: [Spec; 14] = [
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
        match names.len() {
            0 => String::from("no folder"),
            1 => names[0].clone(),
            n => format!("{} and {}", names[..n - 1].join(", "), names[n - 1]),
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
    /// A check box: the yes / no option `key`.
    Check {
        key: &'static str,
        label: &'static str,
    },
    /// One choice of a radio list: the option `key` takes `value`.
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

    /// What it says (for `AZPLAYER_FOCUS setting`).
    #[must_use]
    pub fn label(&self) -> String {
        match self {
            Row::Heading(t) | Row::Note(t) => t.clone(),
            Row::Check { label, .. } | Row::Radio { label, .. } => (*label).to_string(),
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

/// The rows of `category`'s page; library setup lists `folders` (the draft's).
#[must_use]
pub fn rows(category: Category, folders: &Folders, facts: &Facts) -> Vec<Row> {
    let h = |t: &str| Row::Heading(t.to_string());
    let n = |t: &str| Row::Note(t.to_string());
    let mut out = Vec::new();
    match category {
        Category::General => {
            out.push(h("the pointer"));
            out.push(Row::Check {
                key: HIDE_CHROME,
                label: "hide the back button and the controls when the pointer rests",
            });
            out.push(h("the top of the window"));
            out.push(Row::Check {
                key: SHOW_CLOCK,
                label: "show the clock",
            });
            out.push(h("the look"));
            out.push(n(&format!(
                "AzPlayer is always Media Center's deep blue. The other Azlin apps show {}, {} - \
                 their look is chosen in any of them.",
                facts.theme, facts.mode
            )));
        }
        Category::Pictures => {
            out.push(h("the picture library opens on"));
            out.extend(radios(
                PICTURES_VIEW,
                &[("folders", "folders"), ("months", "date taken")],
            ));
            out.push(h("slide show: each picture shows for"));
            out.extend(radios(
                SLIDE_SECONDS,
                &[
                    ("3", "3 seconds"),
                    ("6", "6 seconds"),
                    ("10", "10 seconds"),
                    ("20", "20 seconds"),
                ],
            ));
            out.push(Row::Check {
                key: KEN_BURNS,
                label: "pan and zoom (else a plain cross-fade)",
            });
        }
        Category::Music => {
            out.push(h("the music library opens on"));
            out.extend(radios(
                MUSIC_VIEW,
                &[
                    ("albums", "albums"),
                    ("artists", "artists"),
                    ("genres", "genres"),
                    ("songs", "songs"),
                ],
            ));
            out.push(h("play all"));
            out.push(Row::Check {
                key: SHUFFLE_ALL,
                label: "shuffle the songs (else album by album)",
            });
        }
        Category::Videos => {
            out.push(h("the video library opens on"));
            out.extend(radios(
                VIDEOS_VIEW,
                &[("folders", "folders"), ("months", "date"), ("titles", "title")],
            ));
            out.push(h("playing"));
            out.push(Row::Check {
                key: RESUME,
                label: "resume a video where it was stopped",
            });
            out.push(h("skip back"));
            out.extend(radios(
                SKIP_BACK,
                &[("5", "5 seconds"), ("10", "10 seconds"), ("30", "30 seconds")],
            ));
            out.push(h("skip forward"));
            out.extend(radios(
                SKIP_FORWARD,
                &[("10", "10 seconds"), ("30", "30 seconds"), ("60", "a minute")],
            ));
        }
        Category::Tv => {
            out.push(h("live tv"));
            out.push(n(
                "There is no TV tuner on this computer: live TV needs one. Recorded TV is read \
                 from the folders of library setup.",
            ));
            out.push(h("recorded tv opens on"));
            out.extend(radios(
                TV_VIEW,
                &[("months", "date recorded"), ("titles", "title")],
            ));
        }
        Category::Library => {
            for shelf in Shelf::ALL {
                out.push(h(&format!("{} folders", shelf.word())));
                for path in folders.of(shelf) {
                    out.push(Row::Folder {
                        shelf,
                        path: path.clone(),
                    });
                }
                out.push(Row::Button {
                    label: format!("add a folder to {}", shelf.word()),
                    icon: "create_new_folder",
                    press: Press::AddFolder(shelf),
                });
            }
            out.push(h("the libraries"));
            out.push(Row::Button {
                label: String::from("look through the folders again"),
                icon: "refresh",
                press: Press::Rescan,
            });
            out.push(n(
                "Removing a folder leaves its files where they are. Photo libraries and hidden \
                 files are left out.",
            ));
        }
        Category::Startup => {
            out.push(h("azplayer opens on"));
            out.extend(radios(
                START_SCREEN,
                &[
                    ("start", "the start screen"),
                    ("music", "music"),
                    ("pictures", "pictures"),
                    ("videos", "videos"),
                    ("recent", "recently played"),
                ],
            ));
            out.push(h("the window"));
            out.push(Row::Check {
                key: START_FULLSCREEN,
                label: "open in media only (full screen)",
            });
        }
        Category::About => {
            out.push(h(&format!("azplayer {}", facts.version)));
            out.push(n(
                "A media center in the look of Windows Media Center: music, pictures and slide \
                 shows, videos and movies. Part of the Azlin apps, built with azul. MIT license.",
            ));
            out.push(n(&format!(
                "{} songs, {} pictures, {} videos, {} recordings. The data is in {}.",
                facts.counts[0], facts.counts[1], facts.counts[2], facts.counts[3], facts.data
            )));
            out.push(h("the keys"));
            out.push(n(
                "The arrows move, Enter opens, Backspace or Escape goes back, Home is the start \
                 screen. Tab goes to the back button, the controls and what plays.",
            ));
            out.push(n(
                "Ctrl+D, I or the menu key: more info. Space plays and pauses, M mutes, F is \
                 full screen, Ctrl+, the settings.",
            ));
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
