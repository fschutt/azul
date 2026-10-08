//! Moving around the media center: the pages (forward into a library, a group, now playing;
//! Back - Backspace, Escape, the back button - everywhere), the focus (the arrows, the wheel, the
//! pointer, Tab), and what Enter or a click on an item does. Every move is a rebuild of the window,
//! which is where the motion comes from: the strip's column slides to its new place on the
//! engine's spring, a tile that comes slides in, the focus glow and scale ease over.

use azul::{
    dom::{FocusTarget, NodeId, VirtualKeyCode},
    prelude::*,
    widgets::{OnTextInputReturn, TextInputState, TextInputValid},
};

use crate::{
    app::{self, Player},
    dialog,
    gallery::{self, Step},
    library::Shelf,
    media::{self, Command, CommandRef},
    options::{self, Category},
    overlay,
    pages::{self, Place, Screen, Section, Tile, View, Zone},
    settings,
    strip::{Action, Needs, CATEGORIES},
};

/// What a click or the pointer over a part of the window asks for.
#[derive(Debug, Clone, PartialEq)]
pub enum Act {
    /// An item of the start strip (category, item).
    Strip(usize, usize),
    /// A category's name on the strip.
    Category(usize),
    /// A word of a library's views row.
    View(usize),
    /// A tile of the gallery shown.
    Tile(usize),
    /// The now-playing inset.
    NowPlaying,
    /// A choice of the overlay (more info's row, a dialog's button).
    Choice(usize),
    /// Beside the overlay's panel: its Back.
    Dismiss,
    /// A category of the settings' list.
    SettingsCategory(usize),
    /// A part of a settings page: a row the keys land on, or a button after them.
    Setting(usize),
}

/// A part's payload: the app and what it asks for.
pub struct ActRef {
    pub app: RefAny,
    pub act: Act,
}

/// A wheel step this far (logical px of scrolling) moves the focus by one.
const WHEEL_STEP: f32 = 60.0;

// ==== Pages ====

/// Goes to `screen` (the page before keeps its focus, for Back). A library's page opens on the
/// view the settings choose.
pub fn go(s: &mut Player, screen: Screen) {
    println!("AZPLAYER_PAGE {}", screen.key());
    let mut place = Place::new(screen);
    if let Screen::Section(section) = place.screen {
        place.focus.view = s.options.view_of(section);
    }
    s.nav.push(place);
}

/// Back a page: a slide show stops, a settings page's draft goes (cancel), the start strip
/// stays. `false`: at the start already.
pub fn back_in(s: &mut Player) -> bool {
    if s.nav.len() <= 1 {
        return false;
    }
    match s.nav.pop() {
        Some(Place {
            screen: Screen::Picture,
            ..
        }) => s.viewer.playing = false,
        Some(Place {
            screen: Screen::SettingsPage(category),
            ..
        }) => {
            if s.draft.take().is_some() {
                println!("AZPLAYER_SETTINGS cancel {}", category.key());
            }
        }
        _ => {}
    }
    println!("AZPLAYER_PAGE {}", s.place().screen.key());
    true
}

/// Back (the back button, Backspace, Escape): leaving now playing while the music is PAUSED
/// asks first - stop it (its inset goes), or keep it paused for later - else `go_back`.
pub fn back(app: &RefAny, info: &mut CallbackInfo) -> Update {
    {
        let mut app_ref = app.clone();
        let Some(mut s) = app_ref.downcast_mut::<Player>() else {
            return Update::DoNothing;
        };
        let paused = s.place().screen == Screen::NowPlaying
            && s.music.as_ref().is_some_and(|m| !m.playing());
        if paused {
            let song = s
                .music
                .as_ref()
                .and_then(|m| {
                    m.current_item(&s).map(|i| i.title.clone()).or_else(|| {
                        m.current()
                            .map(|p| crate::library::file_title(std::path::Path::new(p)))
                    })
                })
                .unwrap_or_default();
            dialog::open(&mut s, overlay::music_paused(&song));
            return Update::RefreshDom;
        }
    }
    go_back(app, info)
}

/// Back without a question: a video closes, else the page before comes back.
pub fn go_back(app: &RefAny, info: &mut CallbackInfo) -> Update {
    let is_video = {
        let mut app_ref = app.clone();
        let Some(s) = app_ref.downcast_ref::<Player>() else {
            return Update::DoNothing;
        };
        s.place().screen == Screen::Video
    };
    if is_video {
        media::close_video(app, info);
        let mut app_ref = app.clone();
        if let Some(s) = app_ref.downcast_ref::<Player>() {
            println!("AZPLAYER_PAGE {}", s.place().screen.key());
        }
        app::request_art(app, info);
        return Update::RefreshDom;
    }
    let moved = {
        let mut app_ref = app.clone();
        let Some(mut s) = app_ref.downcast_mut::<Player>() else {
            return Update::DoNothing;
        };
        back_in(&mut s)
    };
    if moved {
        app::request_art(app, info);
        Update::RefreshDom
    } else {
        Update::DoNothing
    }
}

/// The start strip (the Media Center button).
pub fn home(app: &RefAny, info: &mut CallbackInfo) -> Update {
    media::close_video(app, info);
    let mut app_ref = app.clone();
    if let Some(mut s) = app_ref.downcast_mut::<Player>() {
        s.nav.truncate(1);
        s.viewer.playing = false;
        if s.draft.take().is_some() {
            println!("AZPLAYER_SETTINGS cancel home");
        }
        println!("AZPLAYER_PAGE {}", s.place().screen.key());
    }
    Update::RefreshDom
}

/// A seed from the clock (shuffles).
fn now_seed() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(11, |d| u64::from(d.subsec_nanos()) ^ d.as_secs())
}

/// Why `action` has nothing to work on (an empty library), if it has not.
#[must_use]
pub fn missing(s: &Player, action: Action) -> Option<String> {
    match action.needs() {
        Needs::Songs if s.items(Shelf::Music).is_empty() => Some(format!(
            "There is no music in {}.",
            s.folders.shown(Shelf::Music)
        )),
        Needs::Pictures if s.items(Shelf::Pictures).is_empty() => Some(format!(
            "There are no pictures in {}.",
            s.folders.shown(Shelf::Pictures)
        )),
        _ => None,
    }
}

/// What an item of the start strip does.
pub fn activate(app: &RefAny, info: &mut CallbackInfo, action: Action) -> Update {
    // An item that cannot work here says why, and does nothing else.
    {
        let mut app_ref = app.clone();
        let Some(mut s) = app_ref.downcast_mut::<Player>() else {
            return Update::DoNothing;
        };
        if let Some(why) = action.entry().and_then(|e| e.never) {
            s.notice(why);
            return Update::RefreshDom;
        }
        if let Some(why) = missing(&s, action) {
            s.notice(&why);
            return Update::RefreshDom;
        }
        println!("AZPLAYER_ACTION {action:?}");
    }
    let page = |screen: Screen| -> Update {
        let mut app_ref = app.clone();
        if let Some(mut s) = app_ref.downcast_mut::<Player>() {
            go(&mut s, screen);
        }
        Update::RefreshDom
    };
    let update = match action {
        Action::ExtrasLibrary | Action::Radio | Action::LiveTvSetup => Update::DoNothing,
        Action::Recent => page(Screen::Section(Section::Recent)),
        Action::PictureLibrary => page(Screen::Section(Section::Pictures)),
        Action::VideoLibrary => page(Screen::Section(Section::Videos)),
        Action::MusicLibrary => page(Screen::Section(Section::Music)),
        Action::MovieLibrary => page(Screen::Section(Section::Movies)),
        Action::RecordedTv => page(Screen::Section(Section::Tv)),
        Action::Search => page(Screen::Search),
        Action::PlayAll => {
            // Shuffled, or album by album: as the settings say.
            let paths = {
                let mut app_ref = app.clone();
                let Some(s) = app_ref.downcast_ref::<Player>() else {
                    return Update::DoNothing;
                };
                let songs = s.items(Shelf::Music);
                if s.options.is_on(options::SHUFFLE_ALL) {
                    let all = songs.iter().map(|i| i.path.clone()).collect::<Vec<_>>();
                    media::shuffled(all, None, now_seed())
                } else {
                    crate::library::songs_in_album_order(songs)
                        .into_iter()
                        .map(|i| songs[i].path.clone())
                        .collect()
                }
            };
            media::play_music(app, info, paths, 0, true);
            Update::RefreshDom
        }
        Action::PlayFavorites => {
            let mut app_ref = app.clone();
            if let Some(mut s) = app_ref.downcast_mut::<Player>() {
                let paths: Vec<String> = s
                    .items(Shelf::Pictures)
                    .iter()
                    .map(|i| i.path.clone())
                    .collect();
                let paths = media::shuffled(paths, None, now_seed());
                media::show_pictures(&mut s, paths, 0, true);
            }
            Update::RefreshDom
        }
        Action::OpenFile => media::run(app, info, Command::Open),
        // Open an address: Media Center's dialog over the strip (the field, play, a sample).
        Action::OpenAddress => {
            let mut app_ref = app.clone();
            if let Some(mut s) = app_ref.downcast_mut::<Player>() {
                dialog::open(&mut s, overlay::address());
            }
            Update::RefreshDom
        }
        Action::Settings => media::run(app, info, Command::Settings),
        Action::MediaOnly => media::run(app, info, Command::Fullscreen),
        Action::Refresh => media::run(app, info, Command::Refresh),
        Action::About => media::run(app, info, Command::About),
        Action::Close => media::run(app, info, Command::Quit),
    };
    app::request_art(app, info);
    update
}

/// What Enter or a click on tile `index` of the page shown does: a group opens its page, a
/// song plays (the songs of its page, from it on), a picture shows (the pictures of its page,
/// from it on), a video opens, a recent file resumes.
pub fn open_tile(app: &RefAny, info: &mut CallbackInfo, index: usize) -> Update {
    enum Then {
        Nothing,
        Music(Vec<String>, usize),
        Pictures(Vec<String>, usize),
        Video(String),
        Open,
    }
    let then = {
        let mut app_ref = app.clone();
        let Some(mut s) = app_ref.downcast_mut::<Player>() else {
            return Update::DoNothing;
        };
        let place = s.place().clone();
        let tiles = s.page_tiles(&place);
        let Some(tile) = tiles.get(index).cloned() else {
            return Update::DoNothing;
        };
        let section = match place.screen {
            Screen::Section(section) | Screen::Group { section, .. } => section,
            _ => Section::Music,
        };
        match tile {
            Tile::Group { shelf, group } => {
                println!("AZPLAYER_GROUP {}", group.title);
                go(
                    &mut s,
                    Screen::Group {
                        section,
                        shelf,
                        group,
                    },
                );
                Then::Nothing
            }
            Tile::Item { shelf, index: item } => {
                // The items of the same library on this page, in its order: what plays or
                // shows from the tile on.
                let mut paths = Vec::new();
                let mut start = 0;
                for (i, t) in tiles.iter().enumerate() {
                    let Tile::Item {
                        shelf: other_shelf,
                        index: other,
                    } = t
                    else {
                        continue;
                    };
                    if *other_shelf != shelf {
                        continue;
                    }
                    if let Some(it) = s.items(shelf).get(*other) {
                        if i == index {
                            start = paths.len();
                        }
                        paths.push(it.path.clone());
                    }
                }
                let path = s.items(shelf).get(item).map(|i| i.path.clone());
                match shelf {
                    Shelf::Music => Then::Music(paths, start),
                    Shelf::Pictures => Then::Pictures(paths, start),
                    Shelf::Videos | Shelf::Tv => path.map_or(Then::Nothing, Then::Video),
                }
            }
            Tile::Recent(i) => s
                .history
                .entries
                .get(i)
                .map_or(Then::Nothing, |e| Then::Video(e.path.clone())),
            Tile::OpenFile => Then::Open,
        }
    };
    let update = match then {
        Then::Nothing => Update::RefreshDom,
        Then::Music(paths, start) => {
            media::play_music(app, info, paths, start, true);
            Update::RefreshDom
        }
        Then::Pictures(paths, start) => {
            let mut app_ref = app.clone();
            if let Some(mut s) = app_ref.downcast_mut::<Player>() {
                media::show_pictures(&mut s, paths, start, false);
            }
            Update::RefreshDom
        }
        Then::Video(path) => {
            media::open_video(app, info, &path);
            Update::RefreshDom
        }
        Then::Open => media::run(app, info, Command::Open),
    };
    app::request_art(app, info);
    update
}

/// The views row's word `view` of the page shown: a view shows (the gallery from its start),
/// "play slide show" plays every picture of the library.
pub fn open_view(app: &RefAny, info: &mut CallbackInfo, view: usize) -> Update {
    let mut app_ref = app.clone();
    let Some(mut s) = app_ref.downcast_mut::<Player>() else {
        return Update::DoNothing;
    };
    let Screen::Section(section) = s.place().screen else {
        return Update::DoNothing;
    };
    let views = section.views();
    let Some(v) = views.get(view).copied() else {
        return Update::DoNothing;
    };
    if v == View::SlideShow {
        let paths: Vec<String> = s
            .items(Shelf::Pictures)
            .iter()
            .map(|i| i.path.clone())
            .collect();
        if paths.is_empty() {
            let why = format!("There are no pictures in {}.", s.folders.shown(Shelf::Pictures));
            s.notice(&why);
        } else {
            media::show_pictures(&mut s, paths, 0, true);
        }
        drop(s);
        app::request_art(app, info);
        return Update::RefreshDom;
    }
    let focus = &mut s.place_mut().focus;
    focus.view = view;
    focus.index = 0;
    focus.first_col = 0;
    focus.on_views = true;
    println!("AZPLAYER_VIEW {}", v.label(section));
    drop(s);
    app::request_art(app, info);
    Update::RefreshDom
}

// ==== The focus ====

/// Moves the focus on the page shown by an arrow: the strip's categories and items, a gallery's
/// tiles (Up from its first row reaches the views row), the views. `true`: it moved.
pub fn move_focus(s: &mut Player, step: Step) -> bool {
    let place = s.place().clone();
    match place.screen {
        Screen::Start => {
            let moved = match step {
                Step::Up => s.strip.up(),
                Step::Down => s.strip.down(),
                Step::Left => s.strip.left(),
                Step::Right => s.strip.right(),
            };
            if moved {
                println!(
                    "AZPLAYER_FOCUS {} / {}",
                    s.strip.category().name,
                    s.strip.entry().label
                );
            }
            moved
        }
        Screen::Section(_) | Screen::Group { .. } | Screen::Search => {
            let tiles = s.page_tiles(&place);
            let grid = s.grid(&tiles);
            let views = match place.screen {
                Screen::Section(section) => section.views().len(),
                _ => 0,
            };
            let focus = place.focus;
            if focus.on_views {
                let moved = match step {
                    Step::Left if focus.view > 0 => Some((focus.view - 1, true)),
                    Step::Right if focus.view + 1 < views => Some((focus.view + 1, true)),
                    Step::Down if !tiles.is_empty() => Some((focus.view, false)),
                    _ => None,
                };
                let Some((view, on_views)) = moved else {
                    return false;
                };
                let f = &mut s.place_mut().focus;
                // Moving along the views row shows each view at once (Media Center's way); the
                // slide show plays on Enter.
                if view != focus.view {
                    f.view = view;
                    f.index = 0;
                    f.first_col = 0;
                    if let Screen::Section(section) = place.screen {
                        println!("AZPLAYER_VIEW {}", section.views()[view].label(section));
                    }
                }
                f.on_views = on_views;
                if !on_views {
                    println!("AZPLAYER_FOCUS tile {} column {}", f.index, f.first_col);
                }
                return true;
            }
            match gallery::step(&grid, focus.index, tiles.len(), step) {
                Some(index) => {
                    let first_col = grid.scrolled(focus.first_col, index, tiles.len());
                    let f = &mut s.place_mut().focus;
                    f.index = index;
                    f.first_col = first_col;
                    println!("AZPLAYER_FOCUS tile {index} column {first_col}");
                    true
                }
                None if step == Step::Up && views > 0 => {
                    s.place_mut().focus.on_views = true;
                    true
                }
                None => false,
            }
        }
        // The settings' categories, down the page.
        Screen::Settings => {
            let i = place.focus.index.min(Category::ALL.len() - 1);
            let j = match step {
                Step::Up => i.saturating_sub(1),
                Step::Down => (i + 1).min(Category::ALL.len() - 1),
                Step::Left | Step::Right => i,
            };
            if j == place.focus.index {
                return false;
            }
            s.place_mut().focus.index = j;
            println!("AZPLAYER_FOCUS category {}", Category::ALL[j].title());
            true
        }
        // A category's page: its rows, then save and cancel.
        Screen::SettingsPage(category) => {
            let rows = settings::page_rows(s, category);
            let n = options::focusable(&rows).len();
            let j = options::step_page(place.focus.index, n, category.buttons().len(), step);
            if j == place.focus.index {
                return false;
            }
            s.place_mut().focus.index = j;
            println!(
                "AZPLAYER_FOCUS setting {}",
                settings::focus_label(&rows, category, j)
            );
            true
        }
        _ => false,
    }
}

/// Focuses tile `index` of the page shown (the pointer over it).
fn focus_tile(s: &mut Player, index: usize) -> bool {
    let place = s.place().clone();
    let tiles = s.page_tiles(&place);
    if index >= tiles.len() {
        return false;
    }
    let grid = s.grid(&tiles);
    let first_col = grid.scrolled(place.focus.first_col, index, tiles.len());
    let f = &mut s.place_mut().focus;
    let changed =
        f.index != index || f.on_views || f.first_col != first_col || f.zone != Zone::Content;
    f.index = index;
    f.on_views = false;
    f.first_col = first_col;
    // The pointer and the keys move ONE focus: a tile under the pointer takes it from the
    // back button or the inset.
    f.zone = Zone::Content;
    changed
}

// ==== The zones: Tab, and the keys within a zone ====

/// Tab (`forward`) or Shift+Tab: the keys go to the page's next zone, round - the back button,
/// the page, the transport, the inset. The zone's part lights (the app's glow); the pointer's
/// chrome shows, and stays while it has the keys (`Player::chrome_held`). A field keeps the
/// keys only while its page has them (the search field).
fn tab(app: &RefAny, info: &mut CallbackInfo, forward: bool) -> Update {
    let mut app_ref = app.clone();
    let Some(mut s) = app_ref.downcast_mut::<Player>() else {
        return Update::DoNothing;
    };
    let zones = s.zones();
    let from = s.zone();
    let to = pages::next_zone(from, &zones, forward);
    s.place_mut().focus.zone = to;
    println!("AZPLAYER_ZONE {}", to.word());
    if s.chrome_held() {
        app::set_chrome(&mut s, info, true);
    }
    if from == Zone::Content && to != Zone::Content {
        info.clear_focus();
    } else if to == Zone::Content && s.place().screen == Screen::Search {
        if let Some(field) = info
            .get_node_id_by_marker(crate::ids::SEARCH_FIELD)
            .into_option()
        {
            info.set_focus(FocusTarget::Id(field));
        }
    }
    Update::RefreshDom
}

/// An arrow in a zone that is not the page: Left / Right walk the top band's buttons or the
/// transport's; Down from the top band and Up from the inset go to the page. `None`: the
/// arrow is not the zone's (the transport's Up / Down are the volume).
fn move_in_zone(app: &RefAny, zone: Zone, step: Step) -> Option<Update> {
    let mut app_ref = app.clone();
    let mut s = app_ref.downcast_mut::<Player>()?;
    let screen = s.place().screen.clone();
    let along = |i: usize, n: usize| -> usize {
        match step {
            Step::Right => (i + 1).min(n.saturating_sub(1)),
            _ => i.saturating_sub(1),
        }
    };
    match (zone, step) {
        (Zone::Corner, Step::Left | Step::Right) => {
            let n = media::corner_buttons(&screen).len();
            let i = s.place().focus.corner.min(n.saturating_sub(1));
            let j = along(i, n);
            if j == i {
                return Some(Update::DoNothing);
            }
            s.place_mut().focus.corner = j;
            println!("AZPLAYER_FOCUS corner {j}");
        }
        (Zone::Transport, Step::Left | Step::Right) => {
            let what = media::transport_of(&screen)?;
            let buttons = media::transport_buttons(&s, what);
            let i = media::transport_focus(&buttons, s.place().focus.transport);
            let j = along(i, buttons.len());
            if j == i {
                return Some(Update::DoNothing);
            }
            s.place_mut().focus.transport = Some(j);
            println!("AZPLAYER_FOCUS button {}", buttons[j].id);
        }
        (Zone::Corner, Step::Down) | (Zone::Inset, Step::Up) => {
            s.place_mut().focus.zone = Zone::Content;
            println!("AZPLAYER_ZONE content");
        }
        (Zone::Transport, _) => return None,
        _ => return Some(Update::DoNothing),
    }
    Some(Update::RefreshDom)
}

/// Enter in a zone that is not the page: the top band's button, the transport's, the inset
/// (now playing) does what a click does.
fn press_zone(app: &RefAny, info: &mut CallbackInfo, zone: Zone) -> Update {
    let command = {
        let mut app_ref = app.clone();
        let Some(s) = app_ref.downcast_ref::<Player>() else {
            return Update::DoNothing;
        };
        let screen = s.place().screen.clone();
        match zone {
            Zone::Corner => {
                let buttons = media::corner_buttons(&screen);
                buttons.get(s.place().focus.corner).copied()
            }
            Zone::Transport => media::transport_of(&screen).map(|what| {
                let buttons = media::transport_buttons(&s, what);
                buttons[media::transport_focus(&buttons, s.place().focus.transport)].command
            }),
            Zone::Inset | Zone::Content => None,
        }
    };
    match (zone, command) {
        (Zone::Inset, _) => open_now_playing(app, info),
        (_, Some(command)) => media::run(app, info, command),
        _ => Update::DoNothing,
    }
}

/// The step an arrow key is.
fn arrow_step(key: VirtualKeyCode) -> Option<Step> {
    match key {
        VirtualKeyCode::Up => Some(Step::Up),
        VirtualKeyCode::Down => Some(Step::Down),
        VirtualKeyCode::Left => Some(Step::Left),
        VirtualKeyCode::Right => Some(Step::Right),
        _ => None,
    }
}

/// Now playing (the inset, Enter on it).
pub fn open_now_playing(app: &RefAny, info: &mut CallbackInfo) -> Update {
    let mut app_ref = app.clone();
    if let Some(mut s) = app_ref.downcast_mut::<Player>() {
        if s.place().screen != Screen::NowPlaying {
            go(&mut s, Screen::NowPlaying);
        }
    }
    app::request_art(app, info);
    Update::RefreshDom
}

/// Enter on the page shown.
fn enter(app: &RefAny, info: &mut CallbackInfo) -> Update {
    let (screen, focus, action) = {
        let mut app_ref = app.clone();
        let Some(s) = app_ref.downcast_ref::<Player>() else {
            return Update::DoNothing;
        };
        (
            s.place().screen.clone(),
            s.place().focus,
            s.strip.entry().action,
        )
    };
    match screen {
        Screen::Start => activate(app, info, action),
        Screen::Section(_) | Screen::Group { .. } | Screen::Search => {
            if focus.on_views {
                open_view(app, info, focus.view)
            } else {
                open_tile(app, info, focus.index)
            }
        }
        Screen::NowPlaying | Screen::Picture | Screen::Video => {
            media::run(app, info, Command::PlayPause)
        }
        Screen::Settings => {
            let mut app_ref = app.clone();
            if let Some(mut s) = app_ref.downcast_mut::<Player>() {
                if let Some(category) = Category::ALL.get(focus.index) {
                    settings::open_category(&mut s, *category);
                }
            }
            Update::RefreshDom
        }
        Screen::SettingsPage(_) => settings::press(app, info, focus.index),
    }
}

/// Plays the address in the field (or `sample`): a web address opens like a file - behind the
/// curtain, read while it downloads; anything else says what an address must be.
pub fn open_address(app: &RefAny, info: &mut CallbackInfo, sample: Option<&str>) -> Update {
    let text = {
        let mut app_ref = app.clone();
        let Some(mut s) = app_ref.downcast_mut::<Player>() else {
            return Update::DoNothing;
        };
        if let Some(sample) = sample {
            s.address = sample.to_string();
        }
        let text = s.address.trim().to_string();
        if media::web_address(&text).is_none() {
            // The dialog stays (comes back) to have it corrected.
            dialog::open(&mut s, overlay::address());
            s.notice("An address starts with http:// or https:// and names an MP4 or MOV video.");
            return Update::RefreshDom;
        }
        text
    };
    println!("AZPLAYER_ADDRESS {text}");
    media::open_video(app, info, &text);
    Update::RefreshDom
}

/// The address field: what it holds.
pub extern "C" fn on_address(
    mut data: RefAny,
    _info: CallbackInfo,
    state: TextInputState,
) -> OnTextInputReturn {
    if let Some(mut s) = data.downcast_mut::<Player>() {
        s.address = state.get_text().as_str().to_string();
    }
    OnTextInputReturn {
        update: Update::DoNothing,
        valid: TextInputValid::Yes,
    }
}

// ==== The parts' callbacks ====

/// A click on a part: the strip's item does its action, a category comes to the middle, a view
/// shows, a tile opens, the inset goes to now playing.
pub extern "C" fn on_act(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((app, act)) = data
        .downcast_ref::<ActRef>()
        .map(|a| (a.app.clone(), a.act.clone()))
    else {
        return Update::DoNothing;
    };
    match act {
        Act::Strip(row, col) => {
            let action = {
                let mut app_ref = app.clone();
                let Some(mut s) = app_ref.downcast_mut::<Player>() else {
                    return Update::DoNothing;
                };
                s.strip.set(row, col);
                s.strip.entry().action
            };
            activate(&app, &mut info, action)
        }
        Act::Category(row) => {
            let mut app_ref = app.clone();
            let Some(mut s) = app_ref.downcast_mut::<Player>() else {
                return Update::DoNothing;
            };
            let col = s.strip.col_of(row);
            s.strip.set(row, col);
            Update::RefreshDom
        }
        Act::View(view) => open_view(&app, &mut info, view),
        Act::Tile(index) => {
            {
                let mut app_ref = app.clone();
                if let Some(mut s) = app_ref.downcast_mut::<Player>() {
                    focus_tile(&mut s, index);
                };
            }
            open_tile(&app, &mut info, index)
        }
        Act::NowPlaying => open_now_playing(&app, &mut info),
        Act::Choice(i) => dialog::press(&app, &mut info, Some(i)),
        Act::Dismiss => dialog::dismiss(&app, &mut info),
        Act::SettingsCategory(i) => {
            let mut app_ref = app.clone();
            if let Some(mut s) = app_ref.downcast_mut::<Player>() {
                s.place_mut().focus.index = i;
                if let Some(category) = Category::ALL.get(i) {
                    settings::open_category(&mut s, *category);
                }
            }
            Update::RefreshDom
        }
        Act::Setting(i) => settings::press(&app, &mut info, i),
    }
}

/// The pointer comes over a part: the focus goes to it (Media Center's way: the pointer and the
/// keys move one focus). Nothing is rebuilt when it was there already.
pub extern "C" fn on_hover(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((app, act)) = data
        .downcast_ref::<ActRef>()
        .map(|a| (a.app.clone(), a.act.clone()))
    else {
        return Update::DoNothing;
    };
    let changed = {
        let mut app_ref = app.clone();
        let Some(mut s) = app_ref.downcast_mut::<Player>() else {
            return Update::DoNothing;
        };
        match act {
            Act::Strip(row, col) => {
                let before = (s.strip.row, s.strip.col());
                // The pointer over another category's item does not pull the strip around:
                // only the focused category's items take the focus.
                if row == s.strip.row {
                    s.strip.set(row, col);
                }
                // One focus: the strip takes it back from the inset.
                let zoned = s.place().focus.zone != Zone::Content;
                s.place_mut().focus.zone = Zone::Content;
                zoned || before != (s.strip.row, s.strip.col())
            }
            Act::Tile(index) => focus_tile(&mut s, index),
            // An overlay's choice under the pointer takes its focus.
            Act::Choice(i) => dialog::point(&mut s, i),
            // The settings: the pointer over a category or a row brings the bar of light.
            Act::SettingsCategory(i) | Act::Setting(i) => {
                let f = &mut s.place_mut().focus;
                let changed = f.index != i || f.zone != Zone::Content;
                f.index = i;
                f.zone = Zone::Content;
                changed
            }
            // The pointer over a view's word only lights it (`:hover`); a click shows it.
            _ => false,
        }
    };
    if changed {
        app::request_art(&app, &mut info);
        Update::RefreshDom
    } else {
        Update::DoNothing
    }
}

/// A part took the keyboard focus (Tab): the app's focus follows it.
pub extern "C" fn on_focus(data: RefAny, info: CallbackInfo) -> Update {
    on_hover(data, info)
}

/// The right button on a part (a strip item, a tile, the inset): the focus goes to it and its
/// more info opens - Media Center's, never a desktop context menu.
pub extern "C" fn on_more(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((app, act)) = data
        .downcast_ref::<ActRef>()
        .map(|a| (a.app.clone(), a.act.clone()))
    else {
        return Update::DoNothing;
    };
    {
        let mut app_ref = app.clone();
        let Some(mut s) = app_ref.downcast_mut::<Player>() else {
            return Update::DoNothing;
        };
        if s.overlay.is_some() {
            return Update::DoNothing;
        }
        match act {
            Act::Strip(row, col) => {
                s.strip.set(row, col);
                s.place_mut().focus.zone = Zone::Content;
            }
            Act::Tile(index) => {
                focus_tile(&mut s, index);
            }
            Act::NowPlaying => s.place_mut().focus.zone = Zone::Inset,
            _ => return Update::DoNothing,
        }
    }
    info.prevent_default();
    dialog::open_more_info(&app)
}

/// The inset's stop button: the music ends and the inset goes - the click is the button's
/// alone (it does not also open now playing).
pub extern "C" fn on_inset_stop(mut data: RefAny, mut info: CallbackInfo) -> Update {
    info.stop_propagation();
    let Some(app) = data.downcast_ref::<CommandRef>().map(|c| c.app.clone()) else {
        return Update::DoNothing;
    };
    media::run(&app, &mut info, Command::Stop)
}

/// The wheel over the strip or a gallery: a category, or a column, per notch.
pub extern "C" fn on_wheel(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let hit = info.get_hit_node();
    let node = NodeId::create(hit.node.into_raw().saturating_sub(1));
    let Some(delta) = info.get_scroll_delta(hit.dom, node).into_option() else {
        return Update::DoNothing;
    };
    let app = data.clone();
    let moved = {
        let Some(mut s) = data.downcast_mut::<Player>() else {
            return Update::DoNothing;
        };
        let start = s.place().screen == Screen::Start;
        // The strip runs down, a gallery across: either wheel axis steps it.
        let travel = if delta.y.abs() >= delta.x.abs() {
            delta.y
        } else {
            delta.x
        };
        s.wheel += travel;
        let mut moved = false;
        while s.wheel.abs() >= WHEEL_STEP {
            let forward = s.wheel > 0.0;
            s.wheel -= WHEEL_STEP.copysign(s.wheel);
            let step = match (start, forward) {
                (true, true) => Step::Down,
                (true, false) => Step::Up,
                (false, true) => Step::Right,
                (false, false) => Step::Left,
            };
            moved |= move_focus(&mut s, step);
        }
        moved
    };
    info.prevent_default();
    if moved {
        app::request_art(&app, &mut info);
        Update::RefreshDom
    } else {
        Update::DoNothing
    }
}

/// The pointer moved over the window: the chrome and the corner's back button show, in place.
pub extern "C" fn on_pointer(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(mut s) = data.downcast_mut::<Player>() else {
        return Update::DoNothing;
    };
    s.activity();
    app::set_chrome(&mut s, &mut info, true);
    Update::DoNothing
}

/// A double-click on the picture: fullscreen on / off.
pub extern "C" fn on_double_click(data: RefAny, mut info: CallbackInfo) -> Update {
    media::run(&data, &mut info, Command::Fullscreen)
}

/// The search field: the results follow the words.
pub extern "C" fn on_search(
    mut data: RefAny,
    _info: CallbackInfo,
    state: TextInputState,
) -> OnTextInputReturn {
    let query = state.get_text().as_str().to_string();
    let update = match data.downcast_mut::<Player>() {
        Some(mut s) => {
            let found = app::search_tiles(&s.library, &query).len();
            s.query = query;
            let f = &mut s.place_mut().focus;
            f.index = 0;
            f.first_col = 0;
            f.on_views = false;
            println!("AZPLAYER_SEARCH {found}");
            Update::RefreshDom
        }
        None => Update::DoNothing,
    };
    OnTextInputReturn {
        update,
        valid: TextInputValid::Yes,
    }
}

// ==== The keys ====

/// The settings' list (`None`) or the page of a category, from a key: a video closes first (its
/// place is kept), the settings are a page of the media center.
fn open_settings(app: &RefAny, info: &mut CallbackInfo, category: Option<Category>) -> Update {
    media::close_video(app, info);
    let mut app_ref = app.clone();
    if let Some(mut s) = app_ref.downcast_mut::<Player>() {
        match category {
            Some(c) => settings::open_category(&mut s, c),
            None => settings::open(&mut s),
        }
    }
    Update::RefreshDom
}

/// The window's keys: the media center's - the transport on any page, Back, Home, Tab between
/// the page's zones, the arrows, Enter; the settings (Ctrl+, , F1 the keys).
pub extern "C" fn on_key(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let app = data.clone();
    let Some(fullscreen) = data.downcast_ref::<Player>().map(|s| s.fullscreen) else {
        return Update::DoNothing;
    };
    let key = info
        .get_current_keyboard_state()
        .current_virtual_keycode
        .into_option();
    let Some(key) = key else {
        return Update::DoNothing;
    };
    let modifiers = info.get_key_modifiers();
    // An overlay (more info, a dialog) takes the keys while it shows: it is modal.
    if let Some(update) = dialog::key(&app, &mut info, key, modifiers) {
        return update;
    }
    if key == VirtualKeyCode::O && modifiers.primary_down() {
        info.prevent_default();
        return media::on_open(app, info);
    }
    // More info (Media Center's remote button): the menu key, Ctrl+D.
    if key == VirtualKeyCode::Apps || (key == VirtualKeyCode::D && modifiers.ctrl) {
        info.prevent_default();
        return dialog::open_more_info(&app);
    }
    // The settings are AzPlayer's own pages: Ctrl+, (Cmd+, on macOS) their list, F1 the keys.
    if key == VirtualKeyCode::Comma && modifiers.primary_down() {
        info.prevent_default();
        return open_settings(&app, &mut info, None);
    }
    if key == VirtualKeyCode::F1 {
        info.prevent_default();
        return open_settings(&app, &mut info, Some(Category::About));
    }
    // Tab and Shift+Tab: the page's zones (the back button, the page, the transport, the
    // inset) - the app's own walk, never the engine's over every focusable box (which put the
    // keys on a hidden back button: "pressing tab can also make the back button disappear").
    if key == VirtualKeyCode::Tab && !modifiers.ctrl && !modifiers.alt {
        info.prevent_default();
        return tab(&app, &mut info, !modifiers.shift);
    }
    let (screen, searching, transport, zone) = {
        let Some(s) = data.downcast_ref::<Player>() else {
            return Update::DoNothing;
        };
        let screen = s.place().screen.clone();
        // A page whose arrows move its focus (not the volume).
        let gallery = matches!(
            screen,
            Screen::Start
                | Screen::Section(_)
                | Screen::Group { .. }
                | Screen::Search
                | Screen::Settings
                | Screen::SettingsPage(_)
        );
        let zone = s.zone();
        // The search field with words in it (and the keys): Backspace edits it rather than
        // going back.
        let searching = zone == Zone::Content && screen == Screen::Search && !s.query.is_empty();
        (screen, searching, media::transport_key(&s, key, gallery), zone)
    };
    // A zone that is not the page: its Enter and its arrows.
    if zone != Zone::Content {
        if matches!(key, VirtualKeyCode::Return | VirtualKeyCode::NumpadEnter) {
            info.prevent_default();
            return press_zone(&app, &mut info, zone);
        }
        if let Some(step) = arrow_step(key) {
            if let Some(update) = move_in_zone(&app, zone, step) {
                info.prevent_default();
                return update;
            }
        }
    }
    // The search field keeps its letters, Space and Backspace.
    let typing = zone == Zone::Content
        && screen == Screen::Search
        && !matches!(
            key,
            VirtualKeyCode::Up
                | VirtualKeyCode::Down
                | VirtualKeyCode::Left
                | VirtualKeyCode::Right
                | VirtualKeyCode::Return
                | VirtualKeyCode::NumpadEnter
                | VirtualKeyCode::Escape
                | VirtualKeyCode::Back
        );
    if typing {
        return Update::DoNothing;
    }
    // More info: I as well, where no field takes the letter.
    if key == VirtualKeyCode::I && !modifiers.primary_down() && !modifiers.alt {
        info.prevent_default();
        return dialog::open_more_info(&app);
    }
    match key {
        VirtualKeyCode::Escape if fullscreen => {
            info.prevent_default();
            return media::run(&app, &mut info, Command::Fullscreen);
        }
        VirtualKeyCode::Back if searching => return Update::DoNothing,
        VirtualKeyCode::Back | VirtualKeyCode::Escape => {
            info.prevent_default();
            return back(&app, &mut info);
        }
        VirtualKeyCode::Home => {
            info.prevent_default();
            return home(&app, &mut info);
        }
        VirtualKeyCode::Return | VirtualKeyCode::NumpadEnter => {
            info.prevent_default();
            return enter(&app, &mut info);
        }
        _ => {}
    }
    if let Some(command) = transport {
        info.prevent_default();
        return media::run(&app, &mut info, command);
    }
    let Some(step) = arrow_step(key) else {
        return Update::DoNothing;
    };
    // Left / Right move the caret of the search field.
    if screen == Screen::Search && matches!(step, Step::Left | Step::Right) {
        return Update::DoNothing;
    }
    // Left / Right skip in what plays full-window as far as the settings say (a minute with
    // Shift); Up / Down were the volume (the transport above).
    if matches!(screen, Screen::Video | Screen::NowPlaying) {
        let mut skip = |forward: bool| -> f64 {
            data.downcast_ref::<Player>()
                .map_or(0.0, |s| media::skip_s(&s, forward))
        };
        let seconds = match (step, modifiers.shift) {
            (Step::Left, true) => -60.0,
            (Step::Right, true) => 60.0,
            (Step::Left, false) => skip(false),
            (Step::Right, false) => skip(true),
            _ => return Update::DoNothing,
        };
        info.prevent_default();
        return media::seek_by(&app, &mut info, seconds);
    }
    if screen == Screen::Picture {
        info.prevent_default();
        let command = match step {
            Step::Left | Step::Up => Command::Previous,
            Step::Right | Step::Down => Command::Next,
        };
        return media::run(&app, &mut info, command);
    }
    let moved = {
        let Some(mut s) = data.downcast_mut::<Player>() else {
            return Update::DoNothing;
        };
        s.activity();
        move_focus(&mut s, step)
    };
    info.prevent_default();
    if moved {
        app::request_art(&app, &mut info);
        Update::RefreshDom
    } else {
        Update::DoNothing
    }
}

/// The categories' count (for the window).
#[must_use]
pub fn categories() -> usize {
    CATEGORIES.len()
}
