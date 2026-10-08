//! The pages of the media center and what their galleries hold: the start strip; a library's
//! page (music, pictures, videos, movies, recorded tv, recently played) with its views (albums ·
//! artists · genres · songs, folders · date taken, ...); a group's page (an album's songs, a
//! folder's pictures); search; now playing; the picture viewer; the video. Back goes to the page
//! before, with its focus where it was. Plain Rust, tested without a window.

use crate::{
    gallery::TileKind,
    history::History,
    library::{self, Group, Library, Shelf},
};

/// A library's page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Section {
    Music,
    Pictures,
    Videos,
    Movies,
    Tv,
    Recent,
}

impl Section {
    pub const ALL: [Section; 6] = [
        Section::Music,
        Section::Pictures,
        Section::Videos,
        Section::Movies,
        Section::Tv,
        Section::Recent,
    ];

    /// The page's big title (Media Center writes it in lower case).
    #[must_use]
    pub const fn title(self) -> &'static str {
        match self {
            Section::Music => "music",
            Section::Pictures => "pictures",
            Section::Videos => "videos",
            Section::Movies => "movies",
            Section::Tv => "recorded tv",
            Section::Recent => "recently played",
        }
    }

    /// The name `--screen` opens it by.
    #[must_use]
    pub const fn screen_name(self) -> &'static str {
        match self {
            Section::Music => "music",
            Section::Pictures => "pictures",
            Section::Videos => "videos",
            Section::Movies => "movies",
            Section::Tv => "tv",
            Section::Recent => "recent",
        }
    }

    /// The library the page shows (`None`: the recent files).
    #[must_use]
    pub const fn shelf(self) -> Option<Shelf> {
        match self {
            Section::Music => Some(Shelf::Music),
            Section::Pictures => Some(Shelf::Pictures),
            Section::Videos | Section::Movies => Some(Shelf::Videos),
            Section::Tv => Some(Shelf::Tv),
            Section::Recent => None,
        }
    }

    /// The views across the top of the page.
    #[must_use]
    pub const fn views(self) -> &'static [View] {
        match self {
            Section::Music => &[View::Albums, View::Artists, View::Genres, View::Songs],
            Section::Pictures => &[View::Folders, View::Months, View::SlideShow],
            Section::Videos => &[View::Folders, View::Months, View::Titles],
            Section::Movies => &[View::Titles, View::Months],
            Section::Tv => &[View::Months, View::Titles],
            Section::Recent => &[View::Recent],
        }
    }

    /// The page's place among [`Section::ALL`].
    #[must_use]
    pub fn index(self) -> usize {
        Section::ALL.iter().position(|s| *s == self).unwrap_or(0)
    }

    /// The page by its `--screen` name.
    #[must_use]
    pub fn by_screen_name(name: &str) -> Option<Section> {
        Section::ALL.into_iter().find(|s| s.screen_name() == name)
    }
}

/// A view of a library's page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum View {
    Albums,
    Artists,
    Genres,
    Songs,
    Folders,
    /// By the month (the date taken, the date recorded, the date added).
    Months,
    Titles,
    Recent,
    /// Every picture; Enter on its word plays them as a slide show (the last word of the
    /// pictures' row).
    SlideShow,
}

impl View {
    /// The word in the views row, on `section`'s page.
    #[must_use]
    pub const fn label(self, section: Section) -> &'static str {
        match (self, section) {
            (View::Albums, _) => "albums",
            (View::Artists, _) => "artists",
            (View::Genres, _) => "genres",
            (View::Songs, _) => "songs",
            (View::Folders, _) => "folders",
            (View::Months, Section::Pictures) => "date taken",
            (View::Months, Section::Tv) => "date recorded",
            (View::Months, Section::Movies) => "date added",
            (View::Months, _) => "date",
            (View::Titles, _) => "title",
            (View::Recent, _) => "recently played",
            (View::SlideShow, _) => "play slide show",
        }
    }
}

/// One tile of a gallery.
#[derive(Debug, Clone, PartialEq)]
pub enum Tile {
    /// A group of a library's items (an album, an artist, a folder, a month): opens its page.
    Group { shelf: Shelf, group: Group },
    /// An item of a library: a song plays, a picture shows, a video opens.
    Item { shelf: Shelf, index: usize },
    /// A recent file (an index into the history).
    Recent(usize),
    /// The file dialog for a video.
    OpenFile,
}

impl Tile {
    /// A key for the tile that stays the same from one build of the window to the next (its id).
    #[must_use]
    pub fn key(&self, library: &Library, history: &History) -> String {
        match self {
            // Two albums of one title by two artists are two tiles.
            Tile::Group { shelf, group } => format!(
                "g-{shelf:?}-{}",
                short_hash(&format!("{}\u{1}{}", group.title, group.subtitle))
            ),
            Tile::Item { shelf, index } => {
                let path = library
                    .shelf(*shelf)
                    .items
                    .get(*index)
                    .map_or("", |i| i.path.as_str());
                format!("i-{shelf:?}-{}", short_hash(path))
            }
            Tile::Recent(i) => format!(
                "r-{}",
                short_hash(history.entries.get(*i).map_or("", |e| e.path.as_str()))
            ),
            Tile::OpenFile => String::from("open-file"),
        }
    }
}

/// A short steady hash of `text` (for ids).
#[must_use]
pub fn short_hash(text: &str) -> String {
    let hash = text.bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    });
    format!("{hash:016x}")
}

/// What the gallery of `section` shows in `view`.
#[must_use]
pub fn tiles(section: Section, view: View, library: &Library, history: &History) -> Vec<Tile> {
    let groups = |shelf: Shelf, groups: Vec<Group>| -> Vec<Tile> {
        groups
            .into_iter()
            .map(|group| Tile::Group { shelf, group })
            .collect()
    };
    let items = |shelf: Shelf, indices: Vec<usize>| -> Vec<Tile> {
        indices
            .into_iter()
            .map(|index| Tile::Item { shelf, index })
            .collect()
    };
    match section {
        Section::Recent => {
            let mut out = vec![Tile::OpenFile];
            out.extend((0..history.entries.len()).map(Tile::Recent));
            out
        }
        Section::Music => {
            let songs = &library.music.items;
            match view {
                View::Artists => groups(Shelf::Music, library::artists(songs)),
                View::Genres => groups(Shelf::Music, library::genres(songs)),
                View::Songs => items(Shelf::Music, library::by_title(songs)),
                _ => groups(Shelf::Music, library::albums(songs)),
            }
        }
        Section::Pictures => {
            let pictures = &library.pictures.items;
            match view {
                View::Months => groups(Shelf::Pictures, library::months(pictures)),
                // The slide show's view: every picture (Enter on the word plays them all).
                View::SlideShow => items(Shelf::Pictures, library::by_title(pictures)),
                _ => groups(Shelf::Pictures, library::folders(pictures)),
            }
        }
        Section::Videos => {
            let videos = &library.videos.items;
            match view {
                View::Months => groups(Shelf::Videos, library::months(videos)),
                View::Titles => items(Shelf::Videos, library::by_title(videos)),
                _ => groups(Shelf::Videos, library::folders(videos)),
            }
        }
        Section::Movies => {
            let videos = &library.videos.items;
            let movies = library::movies(videos);
            match view {
                View::Months => {
                    let mut by_date = movies;
                    by_date.sort_by_key(|i| std::cmp::Reverse(videos[*i].modified_s));
                    items(Shelf::Videos, by_date)
                }
                _ => items(Shelf::Videos, movies),
            }
        }
        Section::Tv => {
            let shows = &library.tv.items;
            match view {
                View::Titles => items(Shelf::Tv, library::by_title(shows)),
                _ => {
                    let mut by_date = library::by_title(shows);
                    by_date.sort_by_key(|i| std::cmp::Reverse(shows[*i].modified_s));
                    items(Shelf::Tv, by_date)
                }
            }
        }
    }
}

/// The tiles of a group's page: its items.
#[must_use]
pub fn group_tiles(shelf: Shelf, group: &Group) -> Vec<Tile> {
    group
        .items
        .iter()
        .map(|index| Tile::Item {
            shelf,
            index: *index,
        })
        .collect()
}

/// The size of a gallery's tiles.
#[must_use]
pub fn tile_kind(tiles: &[Tile]) -> TileKind {
    match tiles.iter().find(|t| !matches!(t, Tile::OpenFile)) {
        Some(Tile::Item {
            shelf: Shelf::Music,
            ..
        }) => TileKind::Song,
        Some(Tile::Item {
            shelf: Shelf::Pictures,
            ..
        })
        | Some(Tile::Group {
            shelf: Shelf::Pictures,
            ..
        }) => TileKind::Picture,
        Some(Tile::Group {
            shelf: Shelf::Music,
            ..
        }) => TileKind::Square,
        _ => TileKind::Video,
    }
}

/// A page of the media center.
#[derive(Debug, Clone, PartialEq)]
pub enum Screen {
    Start,
    Section(Section),
    /// A group's items (an album's songs, a folder's pictures).
    Group {
        section: Section,
        shelf: Shelf,
        group: Group,
    },
    Search,
    /// An address (a video on a web server, played while it downloads).
    Address,
    NowPlaying,
    /// The picture viewer and the slide show.
    Picture,
    /// A video (opening, or playing).
    Video,
}

impl Screen {
    /// A stable name of the page (its id).
    #[must_use]
    pub fn key(&self) -> String {
        match self {
            Screen::Start => String::from("page-start"),
            Screen::Section(s) => format!("page-{}", s.screen_name()),
            Screen::Group { group, .. } => format!(
                "page-group-{}",
                short_hash(&format!("{}\u{1}{}", group.title, group.subtitle))
            ),
            Screen::Search => String::from("page-search"),
            Screen::Address => String::from("page-address"),
            Screen::NowPlaying => String::from("page-now-playing"),
            Screen::Picture => String::from("page-picture"),
            Screen::Video => String::from("page-video"),
        }
    }

    /// Whether the page stands on the blue ground (the viewer, the video and now playing's
    /// picture are on black).
    #[must_use]
    pub const fn on_ground(&self) -> bool {
        !matches!(self, Screen::Picture | Screen::Video)
    }
}

/// Where the keyboard is on a page: Tab and Shift+Tab move between the page's zones (round),
/// the arrows within one, Enter does what the zone's focused part does. The app draws the
/// focus of the zone the keys are in - one glow on the window, never the engine's ring.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Zone {
    /// The top band's buttons: back (and fullscreen over a video).
    Corner,
    /// The page itself: the strip, a gallery and its views, the settings; on the media pages
    /// the media (the arrows seek or step, Enter plays and pauses).
    #[default]
    Content,
    /// The transport's round buttons (now playing, the picture viewer, a video).
    Transport,
    /// The now-playing inset, bottom left.
    Inset,
}

impl Zone {
    /// Its name in `AZPLAYER_ZONE`.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Zone::Corner => "corner",
            Zone::Content => "content",
            Zone::Transport => "transport",
            Zone::Inset => "inset",
        }
    }
}

/// The zones of `screen` in Tab order (`inset`: the now-playing inset shows on it): the top
/// band's buttons (every page but the start strip), the page, the transport (the pages of what
/// plays), the inset.
#[must_use]
pub fn zones(screen: &Screen, inset: bool) -> Vec<Zone> {
    let mut out = Vec::with_capacity(4);
    if *screen != Screen::Start {
        out.push(Zone::Corner);
    }
    out.push(Zone::Content);
    if matches!(screen, Screen::NowPlaying | Screen::Picture | Screen::Video) {
        out.push(Zone::Transport);
    }
    if inset {
        out.push(Zone::Inset);
    }
    out
}

/// The zone the keys are in: `zone` where the page has it, else the page itself.
#[must_use]
pub fn zone_in(zone: Zone, zones: &[Zone]) -> Zone {
    if zones.contains(&zone) {
        zone
    } else {
        Zone::Content
    }
}

/// The zone Tab (`forward`) or Shift+Tab goes to from `zone`, round.
#[must_use]
pub fn next_zone(zone: Zone, zones: &[Zone], forward: bool) -> Zone {
    let n = zones.len();
    if n == 0 {
        return Zone::Content;
    }
    let here = zone_in(zone, zones);
    let i = zones.iter().position(|z| *z == here).unwrap_or(0);
    let j = if forward { (i + 1) % n } else { (i + n - 1) % n };
    zones[j]
}

/// Where the focus is on a page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Focus {
    /// The views row has the focus (not the gallery).
    pub on_views: bool,
    /// The view shown (an index into the section's views).
    pub view: usize,
    /// The focused tile (on a settings page: the focused row).
    pub index: usize,
    /// The first column in view.
    pub first_col: usize,
    /// The zone the keys are in.
    pub zone: Zone,
    /// The focused button of the top band (0: back).
    pub corner: usize,
    /// The focused button of the transport (`None`: play / pause).
    pub transport: Option<usize>,
}

/// A page in the history of the window: what it is and where its focus was.
#[derive(Debug, Clone, PartialEq)]
pub struct Place {
    pub screen: Screen,
    pub focus: Focus,
}

impl Place {
    #[must_use]
    pub fn new(screen: Screen) -> Place {
        Place {
            screen,
            focus: Focus::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::Item;

    fn library() -> Library {
        let mut lib = Library::default();
        for (title, album) in [("One", "A"), ("Two", "A"), ("Three", "B")] {
            lib.music.items.push(Item {
                path: format!("/m/{title}.mp3"),
                title: title.into(),
                album: album.into(),
                artist: "X".into(),
                ..Item::default()
            });
        }
        lib.videos.items.push(Item {
            path: "/v/short.mp4".into(),
            title: "short".into(),
            duration_s: 60.0,
            ..Item::default()
        });
        lib.videos.items.push(Item {
            path: "/v/film.mp4".into(),
            title: "film".into(),
            duration_s: 7200.0,
            ..Item::default()
        });
        lib
    }

    #[test]
    fn each_page_has_its_views_and_its_gallery() {
        let lib = library();
        let history = History::default();
        let albums = tiles(Section::Music, View::Albums, &lib, &history);
        assert_eq!(albums.len(), 2);
        assert_eq!(tile_kind(&albums), TileKind::Square);
        let songs = tiles(Section::Music, View::Songs, &lib, &history);
        assert_eq!(songs.len(), 3);
        assert_eq!(tile_kind(&songs), TileKind::Song);
        let movies = tiles(Section::Movies, View::Titles, &lib, &history);
        assert_eq!(movies, vec![Tile::Item { shelf: Shelf::Videos, index: 1 }], "only the film");
        let videos = tiles(Section::Videos, View::Titles, &lib, &history);
        assert_eq!(videos.len(), 2);
        assert_eq!(tile_kind(&videos), TileKind::Video);
        let recent = tiles(Section::Recent, View::Recent, &lib, &history);
        assert_eq!(recent, vec![Tile::OpenFile], "the open tile even when nothing was played");
        assert_eq!(View::Months.label(Section::Pictures), "date taken");
        assert_eq!(Section::Pictures.views().last(), Some(&View::SlideShow));
        assert_eq!(Section::by_screen_name("tv"), Some(Section::Tv));
        assert_eq!(Section::by_screen_name("nope"), None);
    }

    #[test]
    fn a_tiles_key_stays_the_same_across_builds_and_differs_between_tiles() {
        let lib = library();
        let history = History::default();
        let a = tiles(Section::Music, View::Songs, &lib, &history);
        let b = tiles(Section::Music, View::Songs, &lib, &history);
        assert_eq!(a[0].key(&lib, &history), b[0].key(&lib, &history));
        assert_ne!(a[0].key(&lib, &history), a[1].key(&lib, &history));
        assert_eq!(short_hash("x"), short_hash("x"));
        assert_eq!(short_hash("x").len(), 16);
        let group = &match &tiles(Section::Music, View::Albums, &lib, &history)[0] {
            Tile::Group { group, .. } => group.clone(),
            other => panic!("an album: {other:?}"),
        };
        assert_eq!(group_tiles(Shelf::Music, group).len(), 2);
        assert!(Screen::Start.on_ground() && !Screen::Video.on_ground());
        assert_ne!(Screen::Section(Section::Music).key(), Screen::Section(Section::Tv).key());
    }

    /// "pressing tab can also make the back button disappear" (2026-10-08): Tab walks the
    /// page's zones - the back button among them - round, never off the page.
    #[test]
    fn tab_walks_a_pages_zones_round_and_shift_tab_walks_back() {
        let music = Screen::Section(Section::Music);
        assert_eq!(zones(&music, false), vec![Zone::Corner, Zone::Content]);
        assert_eq!(
            zones(&music, true),
            vec![Zone::Corner, Zone::Content, Zone::Inset],
            "the inset while music plays"
        );
        assert_eq!(zones(&Screen::Start, false), vec![Zone::Content], "no back on the strip");
        assert_eq!(
            zones(&Screen::NowPlaying, false),
            vec![Zone::Corner, Zone::Content, Zone::Transport]
        );
        let z = zones(&music, false);
        assert_eq!(next_zone(Zone::Content, &z, true), Zone::Corner, "Tab: round to back");
        assert_eq!(next_zone(Zone::Corner, &z, true), Zone::Content);
        assert_eq!(next_zone(Zone::Content, &z, false), Zone::Corner, "Shift+Tab");
        let np = zones(&Screen::NowPlaying, false);
        assert_eq!(next_zone(Zone::Content, &np, true), Zone::Transport);
        assert_eq!(next_zone(Zone::Transport, &np, true), Zone::Corner);
        assert_eq!(next_zone(Zone::Corner, &np, false), Zone::Transport);
        // A zone the page does not have (the inset after the music stopped): the page.
        assert_eq!(zone_in(Zone::Inset, &z), Zone::Content);
        assert_eq!(zone_in(Zone::Transport, &zones(&Screen::Start, false)), Zone::Content);
        assert_eq!(Focus::default().zone, Zone::Content, "a page opens on itself");
        assert_eq!(Zone::Corner.word(), "corner");
    }
}
