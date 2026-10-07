//! The start strip: Media Center's first screen. The categories are stacked vertically - extras,
//! pictures + videos, music, movies, tv, tasks - the focused one on the middle row with its items
//! laid out beside it; Up / Down move between the categories, Left / Right along the items (each
//! category keeps the item it was left on), Enter opens. What each item does is an [`Action`];
//! an item that cannot do anything here says why (`disabled`). Plain Rust, tested without a
//! window.

/// What an item of the start strip (or of a menu) does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Action {
    /// The extras library: other programs (none here).
    ExtrasLibrary,
    /// The files played last, each resuming where it was left.
    Recent,
    PictureLibrary,
    /// A slide show of every picture.
    PlayFavorites,
    VideoLibrary,
    MusicLibrary,
    /// Every song, shuffled.
    PlayAll,
    Radio,
    Search,
    MovieLibrary,
    /// The file dialog for a video.
    OpenFile,
    /// A video at an address (played while it downloads).
    OpenAddress,
    RecordedTv,
    LiveTvSetup,
    Settings,
    /// Fullscreen ("media only": the window is the media center).
    MediaOnly,
    /// Scan the folders again.
    Refresh,
    About,
    /// Closes AzPlayer.
    Close,
}

/// One item of a category.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Entry {
    pub action: Action,
    /// The caption under the tile (Media Center writes them in lower case).
    pub label: &'static str,
    /// The tile's icon (a Material icon name).
    pub icon: &'static str,
    /// Why it does nothing here; `None` = it works. (What depends on the libraries is decided
    /// by the app: [`Action::needs`].)
    pub never: Option<&'static str>,
}

/// One category of the strip.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Category {
    /// The name on the strip ("pictures + videos").
    pub name: &'static str,
    pub entries: &'static [Entry],
}

const fn entry(action: Action, label: &'static str, icon: &'static str) -> Entry {
    Entry {
        action,
        label,
        icon,
        never: None,
    }
}

const fn never(action: Action, label: &'static str, icon: &'static str, why: &'static str) -> Entry {
    Entry {
        action,
        label,
        icon,
        never: Some(why),
    }
}

/// The strip, top to bottom.
pub const CATEGORIES: [Category; 6] = [
    Category {
        name: "extras",
        entries: &[
            never(
                Action::ExtrasLibrary,
                "extras library",
                "extension",
                "No extras are installed on this computer.",
            ),
            entry(Action::Recent, "recently played", "history"),
        ],
    },
    Category {
        name: "pictures + videos",
        entries: &[
            entry(Action::PictureLibrary, "picture library", "photo_library"),
            entry(Action::PlayFavorites, "play favorites", "slideshow"),
            entry(Action::VideoLibrary, "video library", "video_library"),
        ],
    },
    Category {
        name: "music",
        entries: &[
            entry(Action::MusicLibrary, "music library", "library_music"),
            entry(Action::PlayAll, "play all", "shuffle"),
            never(
                Action::Radio,
                "radio",
                "radio",
                "There is no radio tuner on this computer.",
            ),
            entry(Action::Search, "search", "search"),
        ],
    },
    Category {
        name: "movies",
        entries: &[
            entry(Action::MovieLibrary, "movie library", "movie"),
            entry(Action::OpenFile, "open a file", "folder_open"),
            entry(Action::OpenAddress, "open an address", "language"),
        ],
    },
    Category {
        name: "tv",
        entries: &[
            entry(Action::RecordedTv, "recorded tv", "live_tv"),
            never(
                Action::LiveTvSetup,
                "live tv setup",
                "settings_input_antenna",
                "There is no TV tuner on this computer.",
            ),
        ],
    },
    Category {
        name: "tasks",
        entries: &[
            entry(Action::Settings, "settings", "settings"),
            entry(Action::MediaOnly, "media only", "fullscreen"),
            entry(Action::Refresh, "refresh libraries", "refresh"),
            entry(Action::About, "about", "info"),
            entry(Action::Close, "close", "power_settings_new"),
        ],
    },
];

/// The category the strip opens on (music).
pub const START_ROW: usize = 2;

/// What an action needs from the libraries to do anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Needs {
    Nothing,
    Songs,
    Pictures,
}

impl Action {
    /// What the action needs from the libraries (a "play all" of no songs does nothing).
    #[must_use]
    pub fn needs(self) -> Needs {
        match self {
            Action::PlayAll => Needs::Songs,
            Action::PlayFavorites => Needs::Pictures,
            _ => Needs::Nothing,
        }
    }

    /// The entry of this action on the strip.
    #[must_use]
    pub fn entry(self) -> Option<&'static Entry> {
        CATEGORIES
            .iter()
            .flat_map(|c| c.entries.iter())
            .find(|e| e.action == self)
    }
}

/// Where the focus stands on the strip: the category, and the item each category was left on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StripFocus {
    pub row: usize,
    cols: [usize; 6],
}

impl Default for StripFocus {
    fn default() -> Self {
        Self {
            row: START_ROW,
            cols: [0; 6],
        }
    }
}

impl StripFocus {
    /// The focused item of the focused category.
    #[must_use]
    pub fn col(&self) -> usize {
        self.cols.get(self.row).copied().unwrap_or(0)
    }

    /// The item category `row` was left on.
    #[must_use]
    pub fn col_of(&self, row: usize) -> usize {
        self.cols.get(row).copied().unwrap_or(0)
    }

    /// The focused category.
    #[must_use]
    pub fn category(&self) -> &'static Category {
        &CATEGORIES[self.row.min(CATEGORIES.len() - 1)]
    }

    /// The focused item.
    #[must_use]
    pub fn entry(&self) -> &'static Entry {
        let entries = self.category().entries;
        &entries[self.col().min(entries.len() - 1)]
    }

    /// One category up (`false`: already at the top).
    pub fn up(&mut self) -> bool {
        if self.row == 0 {
            return false;
        }
        self.row -= 1;
        true
    }

    /// One category down (`false`: already at the bottom).
    pub fn down(&mut self) -> bool {
        if self.row + 1 >= CATEGORIES.len() {
            return false;
        }
        self.row += 1;
        true
    }

    /// One item left (`false`: already the first).
    pub fn left(&mut self) -> bool {
        let col = self.col();
        if col == 0 {
            return false;
        }
        self.cols[self.row] = col - 1;
        true
    }

    /// One item right (`false`: already the last).
    pub fn right(&mut self) -> bool {
        let col = self.col();
        if col + 1 >= self.category().entries.len() {
            return false;
        }
        self.cols[self.row] = col + 1;
        true
    }

    /// Focus `row`, item `col` (a pointer over it); clamped into the strip.
    pub fn set(&mut self, row: usize, col: usize) {
        self.row = row.min(CATEGORIES.len() - 1);
        let last = CATEGORIES[self.row].entries.len() - 1;
        self.cols[self.row] = col.min(last);
    }

    /// Focus the item of `action` (`false`: it is not on the strip).
    pub fn focus_action(&mut self, action: Action) -> bool {
        for (row, category) in CATEGORIES.iter().enumerate() {
            if let Some(col) = category.entries.iter().position(|e| e.action == action) {
                self.set(row, col);
                return true;
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_strip_opens_on_music_and_moves_between_categories_and_items() {
        let mut f = StripFocus::default();
        assert_eq!(f.category().name, "music");
        assert_eq!(f.entry().action, Action::MusicLibrary);
        assert!(f.right());
        assert!(f.right());
        assert_eq!(f.entry().label, "radio");
        assert!(f.up());
        assert_eq!(f.category().name, "pictures + videos");
        assert_eq!(f.col(), 0, "each category keeps its own item");
        assert!(f.down());
        assert_eq!(f.entry().label, "radio", "and comes back to it");
        assert!(f.right());
        assert!(!f.right(), "search is the last music item");
        for _ in 0..10 {
            f.down();
        }
        assert_eq!(f.category().name, "tasks");
        assert!(!f.down());
        for _ in 0..10 {
            f.up();
        }
        assert_eq!(f.row, 0);
        assert!(!f.up());
        assert!(!f.left());
    }

    #[test]
    fn every_item_does_something_or_says_why_not() {
        for category in CATEGORIES {
            assert!(!category.entries.is_empty(), "{} has items", category.name);
            for e in category.entries {
                assert_eq!(e.label, e.label.to_lowercase(), "Media Center writes lower case");
                if let Some(why) = e.never {
                    assert!(why.ends_with('.'), "{} says why in a sentence", e.label);
                }
            }
        }
        assert!(Action::Radio.entry().and_then(|e| e.never).is_some());
        assert_eq!(Action::PlayAll.needs(), Needs::Songs);
        assert_eq!(Action::PlayFavorites.needs(), Needs::Pictures);
        assert_eq!(Action::Settings.needs(), Needs::Nothing);
    }

    #[test]
    fn a_pointer_or_an_action_focuses_an_item_clamped_into_the_strip() {
        let mut f = StripFocus::default();
        f.set(99, 99);
        assert_eq!(f.category().name, "tasks");
        assert_eq!(f.entry().action, Action::Close);
        assert!(f.focus_action(Action::VideoLibrary));
        assert_eq!((f.row, f.col()), (1, 2));
        assert_eq!(f.col_of(5), 4, "tasks kept its item");
    }
}
