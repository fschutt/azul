//! Moving around the media center: the pages (forward into a library, a group, now playing;
//! Back - Backspace, Escape, the back button - everywhere), the focus (the arrows, the wheel, the
//! pointer, Tab), and what Enter or a click on an item does. Every move is a rebuild of the window,
//! which is where the motion comes from: the strip's column slides to its new place on the
//! engine's spring, a tile that comes slides in, the focus glow and scale ease over.

use azul::{
    dom::{NodeId, VirtualKeyCode},
    prelude::*,
    widgets::{OnTextInputReturn, TextInputState, TextInputValid},
};
use azul_appkit::ui as kit;

use crate::{
    app::{self, Player},
    gallery::{self, Step},
    library::Shelf,
    media::{self, Command},
    pages::{Place, Screen, Section, Tile, View},
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
}

/// A part's payload: the app and what it asks for.
pub struct ActRef {
    pub app: RefAny,
    pub act: Act,
}

/// A wheel step this far (logical px of scrolling) moves the focus by one.
const WHEEL_STEP: f32 = 60.0;

// ==== Pages ====

/// Goes to `screen` (the page before keeps its focus, for Back).
pub fn go(s: &mut Player, screen: Screen) {
    println!("AZPLAYER_PAGE {}", screen.key());
    s.nav.push(Place::new(screen));
}

/// Back a page: a slide show stops, the start strip stays. `false`: at the start already.
pub fn back_in(s: &mut Player) -> bool {
    if s.nav.len() <= 1 {
        return false;
    }
    let left = s.nav.pop();
    if let Some(Place {
        screen: Screen::Picture,
        ..
    }) = left
    {
        s.viewer.playing = false;
    }
    println!("AZPLAYER_PAGE {}", s.place().screen.key());
    true
}

/// Back (a button, Backspace, Escape): a video closes, else the page before comes back.
pub fn back(app: &RefAny, info: &mut CallbackInfo) -> Update {
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
            s.folders.music.display()
        )),
        Needs::Pictures if s.items(Shelf::Pictures).is_empty() => Some(format!(
            "There are no pictures in {}.",
            s.folders.pictures.display()
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
            let paths = {
                let mut app_ref = app.clone();
                let Some(s) = app_ref.downcast_ref::<Player>() else {
                    return Update::DoNothing;
                };
                s.items(Shelf::Music)
                    .iter()
                    .map(|i| i.path.clone())
                    .collect::<Vec<_>>()
            };
            let paths = media::shuffled(paths, None, now_seed());
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
            let why = format!("There are no pictures in {}.", s.folders.pictures.display());
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
        Screen::Start => match step {
            Step::Up => s.strip.up(),
            Step::Down => s.strip.down(),
            Step::Left => s.strip.left(),
            Step::Right => s.strip.right(),
        },
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
                }
                f.on_views = on_views;
                return true;
            }
            match gallery::step(&grid, focus.index, tiles.len(), step) {
                Some(index) => {
                    let first_col = grid.scrolled(focus.first_col, index, tiles.len());
                    let f = &mut s.place_mut().focus;
                    f.index = index;
                    f.first_col = first_col;
                    true
                }
                None if step == Step::Up && views > 0 => {
                    s.place_mut().focus.on_views = true;
                    true
                }
                None => false,
            }
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
    let changed = f.index != index || f.on_views || f.first_col != first_col;
    f.index = index;
    f.on_views = false;
    f.first_col = first_col;
    changed
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
                }
            }
            open_tile(&app, &mut info, index)
        }
        Act::NowPlaying => {
            let mut app_ref = app.clone();
            if let Some(mut s) = app_ref.downcast_mut::<Player>() {
                if s.place().screen != Screen::NowPlaying {
                    go(&mut s, Screen::NowPlaying);
                }
            }
            app::request_art(&app, &mut info);
            Update::RefreshDom
        }
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
                before != (s.strip.row, s.strip.col())
            }
            Act::Tile(index) => focus_tile(&mut s, index),
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
    if !s.controls_shown {
        s.controls_shown = true;
        app::show_chrome(&mut info, true);
    }
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

/// The window's keys: the kit's first (the settings page), then the media center's - the
/// transport on any page, Back, Home, the arrows, Enter. While the settings show, their own.
pub extern "C" fn on_key(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let app = data.clone();
    let Some((kit_ref, fullscreen)) = data
        .downcast_ref::<Player>()
        .map(|s| (s.kit.clone(), s.fullscreen))
    else {
        return Update::DoNothing;
    };
    let key = info
        .get_current_keyboard_state()
        .current_virtual_keycode
        .into_option();
    // Escape leaves fullscreen before the kit sees it.
    if !(fullscreen && key == Some(VirtualKeyCode::Escape)) {
        if let Some(update) = kit::handle_key(&kit_ref, &mut info) {
            return update;
        }
    }
    let Some(key) = key else {
        return Update::DoNothing;
    };
    let modifiers = info.get_key_modifiers();
    if key == VirtualKeyCode::O && modifiers.primary_down() {
        info.prevent_default();
        return media::on_open(app, info);
    }
    if kit::settings_open(&kit_ref) {
        return Update::DoNothing;
    }
    let (screen, searching, transport) = {
        let Some(s) = data.downcast_ref::<Player>() else {
            return Update::DoNothing;
        };
        let screen = s.place().screen.clone();
        let gallery = matches!(
            screen,
            Screen::Start | Screen::Section(_) | Screen::Group { .. } | Screen::Search
        );
        let searching = screen == Screen::Search && !s.query.is_empty();
        (screen, searching, media::transport_key(&s, key, gallery))
    };
    // The search field keeps its letters, Space and Backspace.
    let typing = screen == Screen::Search
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
    let step = match key {
        VirtualKeyCode::Up => Step::Up,
        VirtualKeyCode::Down => Step::Down,
        VirtualKeyCode::Left => Step::Left,
        VirtualKeyCode::Right => Step::Right,
        _ => return Update::DoNothing,
    };
    // Left / Right seek in what plays full-window (a minute with Shift); Up / Down were the
    // volume (the transport above).
    if matches!(screen, Screen::Video | Screen::NowPlaying) {
        let by = if modifiers.shift { 60.0 } else { media::REWIND_S };
        let seconds = match step {
            Step::Left => -by,
            Step::Right => by,
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
