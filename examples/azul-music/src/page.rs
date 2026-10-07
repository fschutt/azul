//! The page the content shows: LINES of known heights (a header, a row of album covers, a song,
//! an album beside its songs), built from the library for the view the sidebar chose. Plain Rust,
//! tested without a window: `ui.rs` draws only the lines in view, inside a VirtualView, so a
//! library of ten thousand songs builds forty rows, not ten thousand.

use crate::{
    library::{Album, Library, Track},
    playlists::Playlist,
};

// ==== What the content shows ====

/// What the content shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum View {
    /// The albums, the newest first.
    RecentlyAdded,
    Artists,
    Albums,
    Songs,
    Genres,
    /// An album, by its (filed) artist and its title.
    Album { artist: String, title: String },
    /// An artist, by name: its albums, each beside its songs.
    Artist(String),
    /// A genre, by name.
    Genre(String),
    /// A playlist, by id.
    Playlist(String),
    /// The search field's results.
    Search,
    /// What plays now and next.
    Queue,
}

/// What a card shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardKind {
    /// An album (an index into `Catalog::albums`).
    Album,
    /// An artist (an index into `Catalog::artists`).
    Artist,
    /// A genre (an index into `Catalog::genres`).
    Genre,
}

/// What the pointer is over in the page: a song (its row) or a card.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Hover {
    #[default]
    None,
    /// A song, by its row in the page's songs.
    Row(usize),
    Card(CardKind, usize),
}

// ==== The songs page's order ====

/// The column the songs page is sorted by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortKey {
    /// The library's order (artist, album, disc, track).
    #[default]
    Library,
    Title,
    Artist,
    Album,
    Time,
}

/// How the songs page is sorted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SongSort {
    pub key: SortKey,
    pub descending: bool,
}

impl SongSort {
    /// A click on the column `key`: that column ascending, the other way round when it was
    /// sorted by it already. The `#` column (`Library`) is always the library's order.
    #[must_use]
    pub fn toggled(self, key: SortKey) -> SongSort {
        if key == SortKey::Library {
            return SongSort::default();
        }
        SongSort {
            key,
            descending: self.key == key && !self.descending,
        }
    }
}

/// The library's songs in `sort` order (ties keep the library's order).
#[must_use]
pub fn sorted_songs(library: &Library, sort: SongSort) -> Vec<usize> {
    let tracks = &library.tracks;
    let mut order: Vec<usize> = (0..tracks.len()).collect();
    let fold = |s: &str| s.trim().to_lowercase();
    match sort.key {
        SortKey::Library => {}
        SortKey::Title => {
            let keys: Vec<String> = tracks
                .iter()
                .map(|t| fold(t.display_title().as_str()))
                .collect();
            order.sort_by(|a, b| keys[*a].cmp(&keys[*b]));
        }
        SortKey::Artist => {
            let keys: Vec<String> = tracks
                .iter()
                .map(|t| {
                    if t.artist.trim().is_empty() {
                        fold(t.filed_artist().as_str())
                    } else {
                        fold(t.artist.as_str())
                    }
                })
                .collect();
            order.sort_by(|a, b| keys[*a].cmp(&keys[*b]));
        }
        SortKey::Album => {
            let keys: Vec<(String, u32, u32)> = tracks
                .iter()
                .map(|t| (fold(t.album.as_str()), t.disc_no, t.track_no))
                .collect();
            order.sort_by(|a, b| keys[*a].cmp(&keys[*b]));
        }
        SortKey::Time => {
            order.sort_by(|a, b| {
                tracks[*a]
                    .duration_s
                    .partial_cmp(&tracks[*b].duration_s)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
        }
    }
    if sort.descending {
        order.reverse();
    }
    order
}

// ==== The catalog: albums, artists, genres ====

/// The library's albums, artists and genres, made once per library (not per frame).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Catalog {
    pub albums: Vec<Album>,
    /// The artists (filed artist) and their song counts, by name.
    pub artists: Vec<(String, usize)>,
    /// The genres and their song counts, by name.
    pub genres: Vec<(String, usize)>,
}

impl Catalog {
    #[must_use]
    pub fn of(library: &Library) -> Catalog {
        Catalog {
            albums: library.albums(),
            artists: library.artists(),
            genres: library.genres(),
        }
    }

    /// The album `title` of `artist`.
    #[must_use]
    pub fn album(&self, artist: &str, title: &str) -> Option<usize> {
        self.albums
            .iter()
            .position(|a| a.artist == artist && a.title == title)
    }

    /// The artist called `name`.
    #[must_use]
    pub fn artist(&self, name: &str) -> Option<usize> {
        self.artists.iter().position(|(n, _)| n == name)
    }

    /// The genre called `name`.
    #[must_use]
    pub fn genre(&self, name: &str) -> Option<usize> {
        self.genres.iter().position(|(n, _)| n == name)
    }

    /// The albums, the newest first (albums added together keep their order).
    #[must_use]
    pub fn recent(&self) -> Vec<usize> {
        let mut order: Vec<usize> = (0..self.albums.len()).collect();
        order.sort_by(|a, b| self.albums[*b].added_s.cmp(&self.albums[*a].added_s));
        order
    }

    /// The albums of `artist`, the newest year first (albums without a year last).
    #[must_use]
    pub fn albums_of(&self, artist: &str) -> Vec<usize> {
        let mut found: Vec<usize> = (0..self.albums.len())
            .filter(|i| self.albums[*i].artist == artist)
            .collect();
        found.sort_by(|a, b| self.albums[*b].year.cmp(&self.albums[*a].year));
        found
    }

    /// The albums with a song of `genre`.
    #[must_use]
    pub fn albums_in(&self, library: &Library, genre: &str) -> Vec<usize> {
        (0..self.albums.len())
            .filter(|i| {
                self.albums[*i]
                    .tracks
                    .iter()
                    .any(|t| library.tracks[*t].genre.trim() == genre)
            })
            .collect()
    }

    /// The songs of the albums `albums`, one album after the other.
    #[must_use]
    pub fn tracks_of(&self, albums: &[usize]) -> Vec<usize> {
        albums
            .iter()
            .filter_map(|a| self.albums.get(*a))
            .flat_map(|a| a.tracks.iter().copied())
            .collect()
    }

    /// The albums whose title or artist holds every word of `query` (any case).
    #[must_use]
    pub fn search_albums(&self, query: &str) -> Vec<usize> {
        let words = words(query);
        if words.is_empty() {
            return Vec::new();
        }
        (0..self.albums.len())
            .filter(|i| {
                let a = &self.albums[*i];
                holds_all(&format!("{} {}", a.title, a.artist), &words)
            })
            .collect()
    }

    /// The artists whose name holds every word of `query` (any case).
    #[must_use]
    pub fn search_artists(&self, query: &str) -> Vec<usize> {
        let words = words(query);
        if words.is_empty() {
            return Vec::new();
        }
        (0..self.artists.len())
            .filter(|i| holds_all(&self.artists[*i].0, &words))
            .collect()
    }
}

fn words(query: &str) -> Vec<String> {
    query.split_whitespace().map(str::to_lowercase).collect()
}

fn holds_all(hay: &str, words: &[String]) -> bool {
    let hay = hay.to_lowercase();
    words.iter().all(|w| hay.contains(w.as_str()))
}

/// The seed of the art (the colour) of the album `title` of `artist`.
#[must_use]
pub fn seed(artist: &str, title: &str) -> String {
    format!("{artist}\u{1f}{title}")
}

/// The seed of an album's art.
#[must_use]
pub fn album_seed(album: &Album) -> String {
    seed(&album.artist, &album.title)
}

/// The album `track` is filed under - its artist and its title - as `Library::albums` groups
/// the songs (a song without an album is in "Unknown album").
#[must_use]
pub fn album_key(track: &Track) -> (String, String) {
    let album = track.album.trim();
    let title = if album.is_empty() {
        "Unknown album"
    } else {
        album
    };
    (track.filed_artist(), title.to_string())
}

// ==== Words ====

/// "1 song", "12 songs".
#[must_use]
pub fn count(n: usize, word: &str) -> String {
    if n == 1 {
        format!("1 {word}")
    } else {
        format!("{n} {word}s")
    }
}

/// A playing time in words: "48 min", "2 hr 5 min" (a few seconds are a minute).
#[must_use]
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub fn duration_words(seconds: f64) -> String {
    let seconds = seconds.max(0.0);
    let minutes = ((seconds / 60.0).round() as u64).max(u64::from(seconds > 0.0));
    if minutes >= 60 {
        format!("{} hr {} min", minutes / 60, minutes % 60)
    } else {
        format!("{minutes} min")
    }
}

/// "6 songs, 1 min" for the songs `tracks` of `library`.
#[must_use]
pub fn songs_detail(library: &Library, tracks: &[usize]) -> String {
    let seconds: f64 = tracks
        .iter()
        .filter_map(|t| library.tracks.get(*t))
        .map(|t| t.duration_s)
        .sum();
    format!(
        "{}, {}",
        count(tracks.len(), "song"),
        duration_words(seconds)
    )
}

// ==== The lines of a page ====

/// The page's side padding, px.
pub const PAD_X: f32 = 16.0;
/// A big header (album, artist, genre, playlist).
pub const HERO_H: f32 = 208.0;
/// A library page's title.
pub const TITLE_H: f32 = 72.0;
/// A section's title.
pub const HEADING_H: f32 = 40.0;
/// The column titles of a song list.
pub const COLUMNS_H: f32 = 24.0;
/// A song in a table (songs, playlists, search, queue, genre).
pub const TABLE_ROW_H: f32 = 22.0;
/// A song of an album.
pub const ALBUM_ROW_H: f32 = 24.0;
/// The cover beside an album's songs (an artist's page).
pub const BLOCK_COVER: f32 = 150.0;
/// The album's title and year over its songs there.
pub const BLOCK_HEAD_H: f32 = 44.0;
/// The space under an album there.
pub const BLOCK_GAP: f32 = 28.0;
/// The narrowest card.
pub const CARD_MIN: f32 = 150.0;
/// The space between two cards, and under a row of them.
pub const CARD_GAP: f32 = 20.0;
/// A card's title and line under its cover.
pub const CARD_TEXT_H: f32 = 40.0;
/// A genre's tile: its height for its width.
pub const GENRE_RATIO: f32 = 0.6;
/// A page with nothing to show.
pub const EMPTY_H: f32 = 120.0;
/// The space under the last line.
pub const END_H: f32 = 24.0;

/// How a song list looks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rows {
    /// #, title, artist, time, album.
    Table,
    /// #, title, time (an album's own songs).
    Album,
}

impl Rows {
    /// A row's height.
    #[must_use]
    pub const fn height(self) -> f32 {
        match self {
            Rows::Table => TABLE_ROW_H,
            Rows::Album => ALBUM_ROW_H,
        }
    }
}

/// What a big header shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeroKind {
    Album,
    Artist,
    Genre,
    Playlist,
}

impl HeroKind {
    /// The small word over the title.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            HeroKind::Album => "ALBUM",
            HeroKind::Artist => "ARTIST",
            HeroKind::Genre => "GENRE",
            HeroKind::Playlist => "PLAYLIST",
        }
    }
}

/// A big header: the art, the title, who made it, how long it is.
#[derive(Debug, Clone, PartialEq)]
pub struct Hero {
    pub kind: HeroKind,
    pub title: String,
    /// The artist of an album ("" otherwise).
    pub by: String,
    /// The year, the songs, the time.
    pub detail: String,
    /// What the art's colour is made from.
    pub seed: String,
}

/// One line of a page.
#[derive(Debug, Clone, PartialEq)]
pub enum Line {
    Hero(Hero),
    /// A library page's title; `play`: with Play and Shuffle.
    Title {
        title: String,
        detail: String,
        play: bool,
    },
    /// A section's title.
    Heading(String),
    /// The column titles of a song list.
    Columns(Rows),
    /// A song: `row` its place in the page's songs, `track` the library's index.
    Track { row: usize, track: usize, rows: Rows },
    /// An album's cover beside its songs (rows `first_row..first_row + count`).
    AlbumBlock {
        album: usize,
        first_row: usize,
        count: usize,
    },
    /// A row of cards.
    Cards { kind: CardKind, items: Vec<usize> },
    /// Nothing to show, and why.
    Empty(String),
    /// The space under the last line.
    End,
}

impl Line {
    /// The line's height, px, with cards `card` px wide.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn height(&self, card: f32) -> f32 {
        match self {
            Line::Hero(_) => HERO_H,
            Line::Title { .. } => TITLE_H,
            Line::Heading(_) => HEADING_H,
            Line::Columns(_) => COLUMNS_H,
            Line::Track { rows, .. } => rows.height(),
            Line::AlbumBlock { count, .. } => {
                BLOCK_COVER.max(BLOCK_HEAD_H + *count as f32 * ALBUM_ROW_H) + BLOCK_GAP
            }
            Line::Cards {
                kind: CardKind::Genre,
                ..
            } => (card * GENRE_RATIO).round() + CARD_GAP,
            Line::Cards { .. } => card + CARD_TEXT_H + CARD_GAP,
            Line::Empty(_) => EMPTY_H,
            Line::End => END_H,
        }
    }
}

/// How many cards a row holds in a page `width` px wide, and how wide each is.
#[must_use]
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]
pub fn grid(width: f32) -> (usize, f32) {
    let avail = (width - 2.0 * PAD_X).max(CARD_MIN);
    let columns = ((avail + CARD_GAP) / (CARD_MIN + CARD_GAP)).floor().max(1.0);
    let card = ((avail - CARD_GAP * (columns - 1.0)) / columns).floor();
    (columns as usize, card)
}

/// Where each line starts, and (last) the page's height.
#[must_use]
pub fn tops(lines: &[Line], card: f32) -> Vec<f32> {
    let mut tops = Vec::with_capacity(lines.len() + 1);
    let mut y = 0.0;
    tops.push(y);
    for line in lines {
        y += line.height(card);
        tops.push(y);
    }
    tops
}

/// The lines that reach into `from..to` (px down the page) as `first..end`; at least one line
/// when there is one.
#[must_use]
pub fn slice(tops: &[f32], from: f32, to: f32) -> (usize, usize) {
    let n = tops.len().saturating_sub(1);
    if n == 0 {
        return (0, 0);
    }
    let starts = &tops[..n];
    let first = starts
        .partition_point(|t| *t <= from)
        .saturating_sub(1)
        .min(n - 1);
    let end = starts.partition_point(|t| *t < to).max(first + 1).min(n);
    (first, end)
}

/// What a page is built from.
pub struct PageInput<'a> {
    pub view: &'a View,
    pub library: &'a Library,
    pub catalog: &'a Catalog,
    pub playlists: &'a [Playlist],
    /// The search field's text.
    pub query: &'a str,
    /// The queue from the current song on (track ids).
    pub queue: &'a [String],
    pub sort: SongSort,
}

/// A page: its lines, and its songs in order (what Play plays, what a row's number counts).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Page {
    pub lines: Vec<Line>,
    pub tracks: Vec<usize>,
}

impl Page {
    fn rows(&mut self, tracks: Vec<usize>, rows: Rows) {
        let first = self.tracks.len();
        for (k, track) in tracks.iter().enumerate() {
            self.lines.push(Line::Track {
                row: first + k,
                track: *track,
                rows,
            });
        }
        self.tracks.extend(tracks);
    }

    fn cards(&mut self, kind: CardKind, items: &[usize], columns: usize) {
        for chunk in items.chunks(columns.max(1)) {
            self.lines.push(Line::Cards {
                kind,
                items: chunk.to_vec(),
            });
        }
    }
}

/// The page of `input.view`, with `columns` cards a row.
#[must_use]
pub fn build(input: &PageInput<'_>, columns: usize) -> Page {
    let lib = input.library;
    let cat = input.catalog;
    let mut page = Page::default();
    match input.view {
        View::RecentlyAdded | View::Albums => {
            let (title, order) = if *input.view == View::Albums {
                ("Albums", (0..cat.albums.len()).collect::<Vec<usize>>())
            } else {
                ("Recently Added", cat.recent())
            };
            page.lines.push(Line::Title {
                title: title.to_string(),
                detail: count(order.len(), "album"),
                play: false,
            });
            page.cards(CardKind::Album, &order, columns);
            page.tracks = cat.tracks_of(&order);
        }
        View::Artists => {
            let all: Vec<usize> = (0..cat.artists.len()).collect();
            page.lines.push(Line::Title {
                title: String::from("Artists"),
                detail: count(all.len(), "artist"),
                play: false,
            });
            page.cards(CardKind::Artist, &all, columns);
            page.tracks = (0..lib.tracks.len()).collect();
        }
        View::Genres => {
            let all: Vec<usize> = (0..cat.genres.len()).collect();
            page.lines.push(Line::Title {
                title: String::from("Genres"),
                detail: count(all.len(), "genre"),
                play: false,
            });
            if all.is_empty() {
                page.lines
                    .push(Line::Empty(String::from("No song of the library names a genre.")));
            }
            page.cards(CardKind::Genre, &all, columns);
            page.tracks = (0..lib.tracks.len()).collect();
        }
        View::Songs => {
            let songs = sorted_songs(lib, input.sort);
            page.lines.push(Line::Title {
                title: String::from("Songs"),
                detail: songs_detail(lib, &songs),
                play: true,
            });
            page.lines.push(Line::Columns(Rows::Table));
            page.rows(songs, Rows::Table);
        }
        View::Album { artist, title } => match cat.album(artist, title) {
            Some(i) => {
                let a = &cat.albums[i];
                let mut detail = Vec::new();
                if !a.year.is_empty() {
                    detail.push(a.year.clone());
                }
                detail.push(songs_detail(lib, &a.tracks));
                page.lines.push(Line::Hero(Hero {
                    kind: HeroKind::Album,
                    title: a.title.clone(),
                    by: a.artist.clone(),
                    detail: detail.join(" \u{b7} "),
                    seed: album_seed(a),
                }));
                page.lines.push(Line::Columns(Rows::Album));
                page.rows(a.tracks.clone(), Rows::Album);
            }
            None => page
                .lines
                .push(Line::Empty(String::from("This album is no longer in the library."))),
        },
        View::Artist(name) => {
            let albums = cat.albums_of(name);
            let tracks = cat.tracks_of(&albums);
            page.lines.push(Line::Hero(Hero {
                kind: HeroKind::Artist,
                title: name.clone(),
                by: String::new(),
                detail: format!(
                    "{} \u{b7} {}",
                    count(albums.len(), "album"),
                    songs_detail(lib, &tracks)
                ),
                seed: name.clone(),
            }));
            if albums.is_empty() {
                page.lines
                    .push(Line::Empty(String::from("This artist is no longer in the library.")));
            }
            for album in albums {
                let songs = cat.albums[album].tracks.clone();
                page.lines.push(Line::AlbumBlock {
                    album,
                    first_row: page.tracks.len(),
                    count: songs.len(),
                });
                page.tracks.extend(songs);
            }
        }
        View::Genre(name) => {
            let songs: Vec<usize> = (0..lib.tracks.len())
                .filter(|i| lib.tracks[*i].genre.trim() == name)
                .collect();
            let albums = cat.albums_in(lib, name);
            page.lines.push(Line::Hero(Hero {
                kind: HeroKind::Genre,
                title: name.clone(),
                by: String::new(),
                detail: songs_detail(lib, &songs),
                seed: name.clone(),
            }));
            if !albums.is_empty() {
                page.lines.push(Line::Heading(String::from("Albums")));
                page.cards(CardKind::Album, &albums, columns);
            }
            if songs.is_empty() {
                page.lines
                    .push(Line::Empty(String::from("No song of the library is in this genre.")));
            } else {
                page.lines.push(Line::Heading(String::from("Songs")));
                page.lines.push(Line::Columns(Rows::Table));
                page.rows(songs, Rows::Table);
            }
        }
        View::Playlist(id) => match input.playlists.iter().find(|p| p.id == *id) {
            Some(p) => {
                let songs: Vec<usize> = p.tracks.iter().filter_map(|t| lib.index_of(t)).collect();
                page.lines.push(Line::Hero(Hero {
                    kind: HeroKind::Playlist,
                    title: p.name.clone(),
                    by: String::new(),
                    detail: songs_detail(lib, &songs),
                    seed: p.id.clone(),
                }));
                if songs.is_empty() {
                    page.lines.push(Line::Empty(String::from(
                        "Empty. Right-click a song anywhere to add it here.",
                    )));
                } else {
                    page.lines.push(Line::Columns(Rows::Table));
                    page.rows(songs, Rows::Table);
                }
            }
            None => page
                .lines
                .push(Line::Empty(String::from("This playlist is gone."))),
        },
        View::Search => {
            let query = input.query.trim();
            let songs = if query.is_empty() {
                Vec::new()
            } else {
                lib.search(query)
            };
            let albums = cat.search_albums(query);
            let artists = cat.search_artists(query);
            page.lines.push(Line::Title {
                title: if query.is_empty() {
                    String::from("Search")
                } else {
                    format!("\u{201c}{query}\u{201d}")
                },
                detail: format!(
                    "{}, {}, {}",
                    count(artists.len(), "artist"),
                    count(albums.len(), "album"),
                    count(songs.len(), "song")
                ),
                play: false,
            });
            if songs.is_empty() && albums.is_empty() && artists.is_empty() {
                page.lines.push(Line::Empty(if query.is_empty() {
                    String::from("Type what you are looking for in the search field.")
                } else {
                    String::from("Nothing in the library matches.")
                }));
            }
            if !artists.is_empty() {
                page.lines.push(Line::Heading(String::from("Artists")));
                page.cards(CardKind::Artist, &artists, columns);
            }
            if !albums.is_empty() {
                page.lines.push(Line::Heading(String::from("Albums")));
                page.cards(CardKind::Album, &albums, columns);
            }
            if !songs.is_empty() {
                page.lines.push(Line::Heading(String::from("Songs")));
                page.lines.push(Line::Columns(Rows::Table));
                page.rows(songs, Rows::Table);
            }
        }
        View::Queue => {
            let songs: Vec<usize> = input
                .queue
                .iter()
                .filter_map(|t| lib.index_of(t))
                .collect();
            page.lines.push(Line::Title {
                title: String::from("Play Queue"),
                detail: songs_detail(lib, &songs),
                play: false,
            });
            if songs.is_empty() {
                page.lines.push(Line::Empty(String::from(
                    "Nothing is queued. Play an album, a playlist or a song.",
                )));
            } else {
                page.lines.push(Line::Heading(String::from("Now Playing")));
                page.lines.push(Line::Columns(Rows::Table));
                page.rows(songs[..1].to_vec(), Rows::Table);
                if songs.len() > 1 {
                    page.lines.push(Line::Heading(String::from("Next Up")));
                    page.rows(songs[1..].to_vec(), Rows::Table);
                }
            }
        }
    }
    page.lines.push(Line::End);
    page
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::Track;

    fn track(title: &str, artist: &str, album: &str, no: u32, seconds: f64) -> Track {
        Track {
            id: format!("id-{title}"),
            path: format!("/music/{artist}/{album}/{no:02} {title}.flac"),
            title: title.into(),
            artist: artist.into(),
            album: album.into(),
            album_artist: artist.into(),
            track_no: no,
            disc_no: 1,
            duration_s: seconds,
            ..Track::default()
        }
    }

    /// Two albums of Northlight Quartet (2024 and 2019), one of Ada Park (Ambient).
    fn library() -> Library {
        let mut tracks = vec![
            track("First Light", "Northlight Quartet", "Blue Hour", 1, 562.0),
            track("Harbour Walk", "Northlight Quartet", "Blue Hour", 2, 337.0),
            track("Old Pier", "Northlight Quartet", "Early Days", 1, 200.0),
            track("Low Tide", "Ada Park", "Field Notes", 1, 241.0),
            track("Salt and Cedar", "Ada Park", "Field Notes", 2, 180.0),
        ];
        for t in &mut tracks[0..2] {
            t.year = "2024".into();
            t.genre = "Jazz".into();
            t.added_s = 100;
        }
        tracks[2].year = "2019".into();
        tracks[2].genre = "Jazz".into();
        tracks[2].added_s = 300;
        for t in &mut tracks[3..5] {
            t.genre = "Ambient".into();
            t.added_s = 200;
        }
        Library {
            version: 1,
            folder: "/music".into(),
            tracks,
        }
    }

    fn input<'a>(
        view: &'a View,
        library: &'a Library,
        catalog: &'a Catalog,
        playlists: &'a [Playlist],
        queue: &'a [String],
    ) -> PageInput<'a> {
        PageInput {
            view,
            library,
            catalog,
            playlists,
            query: "",
            queue,
            sort: SongSort::default(),
        }
    }

    fn titles(lib: &Library, tracks: &[usize]) -> Vec<String> {
        tracks.iter().map(|t| lib.tracks[*t].title.clone()).collect()
    }

    #[test]
    fn the_songs_sort_by_a_column_and_back_and_the_number_column_is_the_librarys_order() {
        let lib = library();
        let by_title = SongSort::default().toggled(SortKey::Title);
        assert_eq!(
            titles(&lib, &sorted_songs(&lib, by_title)),
            vec!["First Light", "Harbour Walk", "Low Tide", "Old Pier", "Salt and Cedar"]
        );
        let back = by_title.toggled(SortKey::Title);
        assert!(back.descending, "a second click turns the order round");
        assert_eq!(
            titles(&lib, &sorted_songs(&lib, back))[0],
            "Salt and Cedar"
        );
        let by_time = back.toggled(SortKey::Time);
        assert!(!by_time.descending, "another column starts ascending");
        assert_eq!(titles(&lib, &sorted_songs(&lib, by_time))[0], "Salt and Cedar");
        assert_eq!(by_time.toggled(SortKey::Library), SongSort::default());
        assert_eq!(sorted_songs(&lib, SongSort::default()), vec![0, 1, 2, 3, 4]);
    }

    #[test]
    fn the_catalog_finds_recent_albums_an_artists_albums_by_year_and_a_genres_albums() {
        let lib = library();
        let cat = Catalog::of(&lib);
        let names = |albums: Vec<usize>| -> Vec<String> {
            albums.iter().map(|a| cat.albums[*a].title.clone()).collect()
        };
        assert_eq!(
            names(cat.recent()),
            vec!["Early Days", "Field Notes", "Blue Hour"]
        );
        assert_eq!(
            names(cat.albums_of("Northlight Quartet")),
            vec!["Blue Hour", "Early Days"],
            "the newest year first"
        );
        assert_eq!(
            names(cat.albums_in(&lib, "Jazz")),
            vec!["Blue Hour", "Early Days"]
        );
        assert_eq!(cat.genres.len(), 2);
        assert_eq!(names(cat.search_albums("ada")), vec!["Field Notes"]);
        assert_eq!(cat.search_artists("north").len(), 1);
        assert!(cat.search_albums("  ").is_empty(), "an empty query finds nothing");
        let blue = cat.album("Northlight Quartet", "Blue Hour").expect("the album");
        assert_eq!(titles(&lib, &cat.tracks_of(&[blue])), vec!["First Light", "Harbour Walk"]);
    }

    #[test]
    fn a_song_is_filed_under_its_albums_artist_and_title() {
        let mut t = track("X", "Guest", "  ", 1, 1.0);
        t.album_artist = "Various Artists".into();
        assert_eq!(
            album_key(&t),
            ("Various Artists".to_string(), "Unknown album".to_string())
        );
        let lib = library();
        let cat = Catalog::of(&lib);
        let (artist, title) = album_key(&lib.tracks[0]);
        let found = cat.album(&artist, &title).expect("the key finds the song's album");
        assert_eq!(seed(&artist, &title), album_seed(&cat.albums[found]));
    }

    #[test]
    fn time_and_counts_read_as_words() {
        assert_eq!(duration_words(0.0), "0 min");
        assert_eq!(duration_words(20.0), "1 min", "a few seconds are a minute");
        assert_eq!(duration_words(48.0 * 60.0), "48 min");
        assert_eq!(duration_words(125.0 * 60.0), "2 hr 5 min");
        assert_eq!(count(1, "song"), "1 song");
        assert_eq!(count(3, "album"), "3 albums");
        let lib = library();
        assert_eq!(songs_detail(&lib, &[0, 1]), "2 songs, 15 min");
    }

    #[test]
    fn a_row_of_cards_fills_the_width_and_never_drops_below_one_card() {
        let (columns, card) = grid(1000.0);
        assert_eq!(columns, 5);
        assert!(card >= CARD_MIN);
        let n = columns as f32;
        assert!(2.0 * PAD_X + n * card + (n - 1.0) * CARD_GAP <= 1000.0);
        assert_eq!(grid(100.0).0, 1, "a narrow page still shows a card a row");
    }

    #[test]
    fn the_lines_in_view_are_found_by_their_tops() {
        let lines = vec![
            Line::Title {
                title: "Songs".into(),
                detail: String::new(),
                play: true,
            },
            Line::Columns(Rows::Table),
            Line::Track {
                row: 0,
                track: 0,
                rows: Rows::Table,
            },
            Line::Track {
                row: 1,
                track: 1,
                rows: Rows::Table,
            },
            Line::End,
        ];
        let tops = tops(&lines, 150.0);
        let head = TITLE_H + COLUMNS_H;
        assert_eq!(
            tops,
            vec![0.0, TITLE_H, head, head + 22.0, head + 44.0, head + 44.0 + END_H]
        );
        assert_eq!(slice(&tops, 0.0, 10.0), (0, 1));
        assert_eq!(slice(&tops, TITLE_H + 1.0, head + 23.0), (1, 4));
        assert_eq!(slice(&tops, 0.0, 10_000.0), (0, 5));
        assert_eq!(slice(&tops, 9_000.0, 10_000.0), (4, 5), "past the end: the last line");
        assert_eq!(slice(&[0.0], 0.0, 10.0), (0, 0));
    }

    #[test]
    fn an_albums_page_is_its_header_over_its_songs_in_track_order() {
        let lib = library();
        let cat = Catalog::of(&lib);
        let view = View::Album {
            artist: "Northlight Quartet".into(),
            title: "Blue Hour".into(),
        };
        let page = build(&input(&view, &lib, &cat, &[], &[]), 4);
        let Line::Hero(hero) = &page.lines[0] else {
            panic!("a header first: {:?}", page.lines[0]);
        };
        assert_eq!(hero.kind, HeroKind::Album);
        assert_eq!(hero.title, "Blue Hour");
        assert_eq!(hero.by, "Northlight Quartet");
        assert_eq!(hero.detail, "2024 \u{b7} 2 songs, 15 min");
        assert_eq!(page.lines[1], Line::Columns(Rows::Album));
        assert_eq!(
            page.lines[2],
            Line::Track {
                row: 0,
                track: 0,
                rows: Rows::Album
            }
        );
        assert_eq!(titles(&lib, &page.tracks), vec!["First Light", "Harbour Walk"]);
        assert_eq!(page.lines.last(), Some(&Line::End));
        let gone = View::Album {
            artist: "Nobody".into(),
            title: "Nothing".into(),
        };
        let page = build(&input(&gone, &lib, &cat, &[], &[]), 4);
        assert!(matches!(page.lines[0], Line::Empty(_)));
        assert!(page.tracks.is_empty());
    }

    #[test]
    fn an_artists_page_puts_each_album_beside_its_songs_and_counts_the_rows_through() {
        let lib = library();
        let cat = Catalog::of(&lib);
        let view = View::Artist("Northlight Quartet".into());
        let page = build(&input(&view, &lib, &cat, &[], &[]), 4);
        let blocks: Vec<(String, usize, usize)> = page
            .lines
            .iter()
            .filter_map(|l| match l {
                Line::AlbumBlock {
                    album,
                    first_row,
                    count,
                } => Some((cat.albums[*album].title.clone(), *first_row, *count)),
                _ => None,
            })
            .collect();
        assert_eq!(
            blocks,
            vec![("Blue Hour".to_string(), 0, 2), ("Early Days".to_string(), 2, 1)]
        );
        assert_eq!(
            titles(&lib, &page.tracks),
            vec!["First Light", "Harbour Walk", "Old Pier"]
        );
        let block = Line::AlbumBlock {
            album: 0,
            first_row: 0,
            count: 10,
        };
        assert_eq!(
            block.height(150.0),
            BLOCK_HEAD_H + 10.0 * ALBUM_ROW_H + BLOCK_GAP,
            "a long album is as tall as its songs"
        );
    }

    #[test]
    fn the_grid_pages_cut_their_cards_into_rows() {
        let lib = library();
        let cat = Catalog::of(&lib);
        let page = build(&input(&View::Albums, &lib, &cat, &[], &[]), 2);
        let rows: Vec<&Vec<usize>> = page
            .lines
            .iter()
            .filter_map(|l| match l {
                Line::Cards {
                    kind: CardKind::Album,
                    items,
                } => Some(items),
                _ => None,
            })
            .collect();
        assert_eq!(rows, vec![&vec![0, 1], &vec![2]]);
        assert_eq!(page.tracks.len(), 5, "Play plays every album");
        let recent = build(&input(&View::RecentlyAdded, &lib, &cat, &[], &[]), 8);
        assert_eq!(
            recent.lines[1],
            Line::Cards {
                kind: CardKind::Album,
                items: cat.recent()
            }
        );
    }

    #[test]
    fn a_playlists_page_skips_songs_gone_from_the_library_and_says_when_it_is_empty() {
        let lib = library();
        let cat = Catalog::of(&lib);
        let mut p = Playlist::new("p1".into(), "Mix".into());
        p.add(&["id-Low Tide".to_string(), "gone".to_string(), "id-First Light".to_string()]);
        let playlists = vec![p, Playlist::new("p2".into(), "Empty".into())];
        let view = View::Playlist("p1".into());
        let page = build(&input(&view, &lib, &cat, &playlists, &[]), 4);
        assert_eq!(titles(&lib, &page.tracks), vec!["Low Tide", "First Light"]);
        let empty = View::Playlist("p2".into());
        let page = build(&input(&empty, &lib, &cat, &playlists, &[]), 4);
        assert!(page.lines.iter().any(|l| matches!(l, Line::Empty(_))));
        assert!(page.tracks.is_empty());
    }

    #[test]
    fn search_shows_the_artists_albums_and_songs_that_match() {
        let lib = library();
        let cat = Catalog::of(&lib);
        let mut page_input = input(&View::Search, &lib, &cat, &[], &[]);
        page_input.query = "tide";
        let page = build(&page_input, 4);
        assert_eq!(titles(&lib, &page.tracks), vec!["Low Tide"]);
        page_input.query = "ada";
        let page = build(&page_input, 4);
        assert!(page.lines.contains(&Line::Heading("Artists".into())));
        assert!(page.lines.contains(&Line::Heading("Albums".into())));
        assert_eq!(page.tracks.len(), 2, "the artist's name finds their songs");
        page_input.query = "nothing like this";
        let page = build(&page_input, 4);
        assert!(page.lines.iter().any(|l| matches!(l, Line::Empty(_))));
    }

    #[test]
    fn the_queue_page_is_what_plays_now_then_what_comes_next() {
        let lib = library();
        let cat = Catalog::of(&lib);
        let queue = vec!["id-Low Tide".to_string(), "id-Old Pier".to_string()];
        let page = build(&input(&View::Queue, &lib, &cat, &[], &queue), 4);
        assert_eq!(titles(&lib, &page.tracks), vec!["Low Tide", "Old Pier"]);
        assert!(page.lines.contains(&Line::Heading("Now Playing".into())));
        assert!(page.lines.contains(&Line::Heading("Next Up".into())));
        let page = build(&input(&View::Queue, &lib, &cat, &[], &[]), 4);
        assert!(page.lines.iter().any(|l| matches!(l, Line::Empty(_))));
    }
}
