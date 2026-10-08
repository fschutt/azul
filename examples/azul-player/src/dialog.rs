//! The overlays at work (`overlay.rs` is their model, `ui.rs` their look): what opens them -
//! the more-info keys (Ctrl+D, I, the menu key) and the right button on a tile, a strip item or
//! the inset; Back on now playing while the music is paused; a video that does not play; the
//! strip's about and open an address - their keys, and what each choice does.
//!
//! On stdout: `AZPLAYER_OVERLAY <name> <open|closed>`, `AZPLAYER_FOCUS choice <label>`,
//! `AZPLAYER_CHOICE <label>`, `AZPLAYER_QUEUE <n>`, `AZPLAYER_DELETED <path>`.

use std::path::Path;

use azul::{
    dom::{KeyModifiers, VirtualKeyCode},
    prelude::*,
};

use crate::{
    app::{self, Player},
    library::{self, Shelf},
    media::{self, Command},
    nav,
    overlay::{self, choice, Do, Kind, Overlay},
    pages::{Screen, Tile, Zone},
    settings,
};

/// Opens `o` over the page (one before it goes), and says so.
pub fn open(s: &mut Player, o: Overlay) {
    close(s);
    println!("AZPLAYER_OVERLAY {} open", o.name);
    s.overlay = Some(o);
}

/// Closes the overlay, and says so.
pub fn close(s: &mut Player) {
    if let Some(o) = s.overlay.take() {
        println!("AZPLAYER_OVERLAY {} closed", o.name);
    }
}

// ==== More info ====

/// The more info of what the keys are on: the inset's (and now playing's) song, the strip's
/// item, a gallery's tile. `None`: there is nothing to tell (the views row, a settings page).
#[must_use]
pub fn more_info(s: &Player) -> Option<Overlay> {
    let place = s.place();
    if s.zone() == Zone::Inset || place.screen == Screen::NowPlaying {
        return now_playing_info(s);
    }
    match &place.screen {
        Screen::Start if s.zone() == Zone::Content => {
            let entry = s.strip.entry();
            let mut lines = vec![s.strip.category().name.to_string()];
            let why = entry
                .never
                .map(str::to_string)
                .or_else(|| nav::missing(s, entry.action));
            if let Some(why) = why {
                lines.push(why);
            }
            Some(overlay::more_info(
                entry.label,
                lines,
                vec![choice("open", "chevron_right", Do::Strip(entry.action))],
            ))
        }
        Screen::Section(_) | Screen::Group { .. } | Screen::Search
            if s.zone() == Zone::Content && !place.focus.on_views =>
        {
            tile_info(s, place.focus.index)
        }
        _ => None,
    }
}

/// The more info of tile `index` of the page: play, add to the queue, the slide show from it,
/// play from the start, forget it, delete it - what fits the tile.
fn tile_info(s: &Player, index: usize) -> Option<Overlay> {
    let tiles = s.page_tiles(s.place());
    let tile = tiles.get(index)?;
    let (title, subtitle, _, _) = crate::ui::describe(s, tile);
    let mut lines = Vec::new();
    if !subtitle.is_empty() {
        lines.push(subtitle);
    }
    let resumes = |path: &str| s.history.resume_at(path) > 0.0;
    let choices = match tile {
        Tile::Group {
            shelf: Shelf::Music,
            group,
        } => {
            lines.push(library::count(group.items.len(), "song", "songs"));
            vec![
                choice("play", "play_arrow", Do::Play(index)),
                choice("add to queue", "queue_music", Do::Queue(index)),
                choice("open", "chevron_right", Do::Open(index)),
            ]
        }
        Tile::Group {
            shelf: Shelf::Pictures,
            group,
        } => {
            lines.push(library::count(group.items.len(), "picture", "pictures"));
            vec![
                choice("play slide show", "slideshow", Do::SlideShow(index)),
                choice("open", "chevron_right", Do::Open(index)),
            ]
        }
        Tile::Group { .. } => vec![choice("open", "chevron_right", Do::Open(index))],
        Tile::Item {
            shelf: Shelf::Music,
            ..
        } => vec![
            choice("play", "play_arrow", Do::Open(index)),
            choice("add to queue", "queue_music", Do::Queue(index)),
        ],
        Tile::Item {
            shelf: Shelf::Pictures,
            index: item,
        } => {
            let path = s.items(Shelf::Pictures).get(*item)?.path.clone();
            vec![
                choice("view", "photo", Do::Open(index)),
                choice("play slide show from here", "slideshow", Do::SlideShow(index)),
                choice("delete", "delete", Do::AskDelete(path)),
            ]
        }
        Tile::Item { shelf, index: item } => {
            let path = s.items(*shelf).get(*item)?.path.clone();
            let mut c = vec![choice("play", "play_arrow", Do::Open(index))];
            if resumes(&path) {
                c.push(choice("play from the start", "replay", Do::FromStart(path.clone())));
            }
            c.push(choice("delete", "delete", Do::AskDelete(path)));
            c
        }
        Tile::Recent(i) => {
            let path = s.history.entries.get(*i)?.path.clone();
            let mut c = vec![choice("play", "play_arrow", Do::Open(index))];
            if resumes(&path) {
                c.push(choice("play from the start", "replay", Do::FromStart(path.clone())));
            }
            c.push(choice(
                "remove from recently played",
                "playlist_remove",
                Do::Forget(path),
            ));
            c
        }
        Tile::OpenFile => vec![choice("open a file", "folder_open", Do::Open(index))],
    };
    Some(overlay::more_info(&title, lines, choices))
}

/// The more info of the music that plays (the inset, now playing): its page, pause / play,
/// stop - which ends it and takes the inset away.
fn now_playing_info(s: &Player) -> Option<Overlay> {
    let music = s.music.as_ref()?;
    let song = music.current_item(s);
    let title = song.map_or_else(
        || {
            music
                .current()
                .map(|p| library::file_title(Path::new(p)))
                .unwrap_or_default()
        },
        |i| i.title.clone(),
    );
    let mut lines = Vec::new();
    if let Some(i) = song {
        lines.push(format!("{} \u{b7} {}", i.filed_artist(), i.album_or_unknown()));
    }
    let playing = music.playing();
    lines.push(String::from(if playing { "now playing" } else { "paused" }));
    let mut choices = Vec::new();
    if s.place().screen != Screen::NowPlaying {
        choices.push(choice("now playing", "queue_music", Do::NowPlaying));
    }
    choices.push(if playing {
        choice("pause", "pause", Do::PlayPause)
    } else {
        choice("play", "play_arrow", Do::PlayPause)
    });
    choices.push(choice("stop", "stop", Do::Stop));
    Some(overlay::more_info(&title, lines, choices))
}

/// The more info of what the keys are on, over the page (nothing when there is nothing to tell).
pub fn open_more_info(app: &RefAny) -> Update {
    let mut app_ref = app.clone();
    let Some(mut s) = app_ref.downcast_mut::<Player>() else {
        return Update::DoNothing;
    };
    match more_info(&s) {
        Some(o) => {
            open(&mut s, o);
            Update::RefreshDom
        }
        None => Update::DoNothing,
    }
}

/// About AzPlayer, as a dialog over the page.
pub fn open_about(s: &mut Player) {
    let facts = settings::facts(s);
    let about = app::ABOUT;
    open(
        s,
        overlay::about(vec![
            format!("AzPlayer {}", facts.version),
            about.summary.to_string(),
            format!("{} license. The data is in {}.", about.license, facts.data),
        ]),
    );
}

// ==== The keys and the pointer ====

/// A key while an overlay shows: Up / Down (a dialog's buttons also Left / Right) and Tab move
/// the focus, Enter chooses, Back / Escape / Backspace do the overlay's "no"; the address
/// dialog's field keeps its letters, Backspace and its caret. Every other key does nothing (the
/// overlay is modal). `None`: no overlay shows.
pub fn key(
    app: &RefAny,
    info: &mut CallbackInfo,
    key: VirtualKeyCode,
    modifiers: KeyModifiers,
) -> Option<Update> {
    let (kind, back, field_empty) = {
        let mut app_ref = app.clone();
        let s = app_ref.downcast_ref::<Player>()?;
        let o = s.overlay.as_ref()?;
        (o.kind, o.back.clone(), s.address.is_empty())
    };
    let typing = kind == Kind::Address;
    let step = |forward: bool| -> Update {
        let mut app_ref = app.clone();
        let Some(mut s) = app_ref.downcast_mut::<Player>() else {
            return Update::DoNothing;
        };
        let Some(o) = s.overlay.as_mut() else {
            return Update::DoNothing;
        };
        if !o.step(forward) {
            return Update::DoNothing;
        }
        if let Some(c) = o.chosen() {
            println!("AZPLAYER_FOCUS choice {}", c.label);
        }
        Update::RefreshDom
    };
    let update = match key {
        VirtualKeyCode::Back if typing && !field_empty => return Some(Update::DoNothing),
        VirtualKeyCode::Escape | VirtualKeyCode::Back => {
            info.prevent_default();
            choose(app, info, back)
        }
        VirtualKeyCode::Return | VirtualKeyCode::NumpadEnter => {
            info.prevent_default();
            press(app, info, None)
        }
        VirtualKeyCode::Tab => {
            info.prevent_default();
            let mut app_ref = app.clone();
            if let Some(mut s) = app_ref.downcast_mut::<Player>() {
                if let Some(o) = s.overlay.as_mut() {
                    o.cycle(!modifiers.shift);
                    if let Some(c) = o.chosen() {
                        println!("AZPLAYER_FOCUS choice {}", c.label);
                    }
                }
            }
            Update::RefreshDom
        }
        VirtualKeyCode::Up | VirtualKeyCode::Down => {
            info.prevent_default();
            step(key == VirtualKeyCode::Down)
        }
        VirtualKeyCode::Left | VirtualKeyCode::Right if kind == Kind::Dialog => {
            info.prevent_default();
            step(key == VirtualKeyCode::Right)
        }
        // The field's caret and letters.
        _ if typing => return Some(Update::DoNothing),
        _ => {
            info.prevent_default();
            Update::DoNothing
        }
    };
    Some(update)
}

/// Chooses the overlay's choice `index` (`None`: the focused one): it says so, the overlay
/// closes (or a question takes its place) and the choice is done.
pub fn press(app: &RefAny, info: &mut CallbackInfo, index: Option<usize>) -> Update {
    let act = {
        let mut app_ref = app.clone();
        let Some(mut s) = app_ref.downcast_mut::<Player>() else {
            return Update::DoNothing;
        };
        let Some(o) = s.overlay.as_mut() else {
            return Update::DoNothing;
        };
        if let Some(i) = index {
            o.point(i);
        }
        let Some(c) = o.chosen() else {
            return Update::DoNothing;
        };
        println!("AZPLAYER_CHOICE {}", c.label);
        c.act.clone()
    };
    choose(app, info, act)
}

/// The pointer over choice `index`: the focus goes to it. `true`: it moved.
pub fn point(s: &mut Player, index: usize) -> bool {
    let Some(o) = s.overlay.as_mut() else {
        return false;
    };
    let moved = o.point(index);
    if moved {
        if let Some(c) = o.chosen() {
            println!("AZPLAYER_FOCUS choice {}", c.label);
        }
    }
    moved
}

/// The overlay's "no" (Back, Escape, a click beside the panel).
pub fn dismiss(app: &RefAny, info: &mut CallbackInfo) -> Update {
    let back = {
        let mut app_ref = app.clone();
        let Some(s) = app_ref.downcast_ref::<Player>() else {
            return Update::DoNothing;
        };
        match s.overlay.as_ref() {
            Some(o) => o.back.clone(),
            None => return Update::DoNothing,
        }
    };
    choose(app, info, back)
}

// ==== What a choice does ====

/// Does `act`: the overlay closes first - or a question takes its place (delete, remove a
/// folder).
#[allow(clippy::too_many_lines)]
pub fn choose(app: &RefAny, info: &mut CallbackInfo, act: Do) -> Update {
    {
        let mut app_ref = app.clone();
        let Some(mut s) = app_ref.downcast_mut::<Player>() else {
            return Update::DoNothing;
        };
        match &act {
            Do::AskDelete(path) => {
                let title = library::file_title(Path::new(path));
                open(&mut s, overlay::confirm_delete(path, &title));
                return Update::RefreshDom;
            }
            Do::AskRemoveFolder(shelf, path) => {
                open(&mut s, overlay::confirm_remove_folder(*shelf, path));
                return Update::RefreshDom;
            }
            Do::RemoveFolder(shelf, path) => {
                close(&mut s);
                settings::remove_folder(&mut s, *shelf, Path::new(path));
                return Update::RefreshDom;
            }
            Do::Forget(path) => {
                close(&mut s);
                s.history.remove(path);
                println!("AZPLAYER_HISTORY {}", s.history.entries.len());
                s.notice("Removed from recently played (the file stays).");
                app::save_history(app, &s, info);
            }
            _ => close(&mut s),
        }
    }
    let update = match act {
        Do::Close
        | Do::Forget(_)
        | Do::AskDelete(_)
        | Do::AskRemoveFolder(..)
        | Do::RemoveFolder(..) => Update::RefreshDom,
        Do::Back => nav::go_back(app, info),
        Do::Open(i) => nav::open_tile(app, info, i),
        Do::Play(i) => play_tile(app, info, i, false),
        Do::SlideShow(i) => play_tile(app, info, i, true),
        Do::Queue(i) => {
            let paths = {
                let mut app_ref = app.clone();
                let Some(s) = app_ref.downcast_ref::<Player>() else {
                    return Update::DoNothing;
                };
                tile_songs(&s, i)
            };
            media::queue_music(app, info, paths);
            Update::RefreshDom
        }
        Do::FromStart(path) => {
            {
                let mut app_ref = app.clone();
                if let Some(mut s) = app_ref.downcast_mut::<Player>() {
                    // Left at the start: it opens there.
                    s.history.set_position(&path, 0.0, 0.0);
                }
            }
            media::open_video(app, info, &path);
            Update::RefreshDom
        }
        Do::Delete(path) => delete_file(app, info, &path),
        Do::PlayPause => media::run(app, info, Command::PlayPause),
        Do::Stop => media::run(app, info, Command::Stop),
        Do::NowPlaying => nav::open_now_playing(app, info),
        Do::StopAndBack => {
            let _ = media::run(app, info, Command::Stop);
            nav::go_back(app, info)
        }
        Do::Strip(action) => nav::activate(app, info, action),
        Do::PlayAddress => nav::open_address(app, info, None),
        Do::PlaySample => nav::open_address(app, info, Some(crate::ui::SAMPLE_ADDRESS)),
        Do::Quit => media::run(app, info, Command::Quit),
    };
    app::clamp_focus(app);
    app::request_art(app, info);
    update
}

/// The songs of tile `index` of the page: an album's (artist's, genre's), or the song.
fn tile_songs(s: &Player, index: usize) -> Vec<String> {
    let tiles = s.page_tiles(s.place());
    let songs = s.items(Shelf::Music);
    match tiles.get(index) {
        Some(Tile::Group {
            shelf: Shelf::Music,
            group,
        }) => group
            .items
            .iter()
            .filter_map(|i| songs.get(*i))
            .map(|i| i.path.clone())
            .collect(),
        Some(Tile::Item {
            shelf: Shelf::Music,
            index,
        }) => songs.get(*index).map(|i| vec![i.path.clone()]).unwrap_or_default(),
        _ => Vec::new(),
    }
}

/// Plays what tile `index` holds: an album's songs (now playing), a folder's pictures - or the
/// pictures of the page from this one - as a slide show (`slideshow`), else what Enter does.
fn play_tile(app: &RefAny, info: &mut CallbackInfo, index: usize, slideshow: bool) -> Update {
    let (songs, pictures) = {
        let mut app_ref = app.clone();
        let Some(s) = app_ref.downcast_ref::<Player>() else {
            return Update::DoNothing;
        };
        let tiles = s.page_tiles(s.place());
        match tiles.get(index) {
            Some(Tile::Group {
                shelf: Shelf::Music,
                ..
            }) => (tile_songs(&s, index), None),
            Some(Tile::Group {
                shelf: Shelf::Pictures,
                group,
            }) => {
                let all = s.items(Shelf::Pictures);
                let paths: Vec<String> = group
                    .items
                    .iter()
                    .filter_map(|i| all.get(*i))
                    .map(|i| i.path.clone())
                    .collect();
                (Vec::new(), Some((paths, 0)))
            }
            Some(Tile::Item {
                shelf: Shelf::Pictures,
                ..
            }) => {
                // The page's pictures, from this one.
                let all = s.items(Shelf::Pictures);
                let mut paths = Vec::new();
                let mut start = 0;
                for (i, t) in tiles.iter().enumerate() {
                    if let Tile::Item {
                        shelf: Shelf::Pictures,
                        index: item,
                    } = t
                    {
                        if let Some(p) = all.get(*item) {
                            if i == index {
                                start = paths.len();
                            }
                            paths.push(p.path.clone());
                        }
                    }
                }
                (Vec::new(), Some((paths, start)))
            }
            _ => (Vec::new(), None),
        }
    };
    if !songs.is_empty() {
        media::play_music(app, info, songs, 0, true);
        return Update::RefreshDom;
    }
    match pictures {
        Some((paths, start)) => {
            let mut app_ref = app.clone();
            if let Some(mut s) = app_ref.downcast_mut::<Player>() {
                media::show_pictures(&mut s, paths, start, slideshow);
            }
            Update::RefreshDom
        }
        None => nav::open_tile(app, info, index),
    }
}

/// Deletes the library file `path` (asked first, `overlay::confirm_delete`): never what plays,
/// never a file that is not a library's; it leaves the library and the history. What went
/// wrong is a dialog.
fn delete_file(app: &RefAny, info: &mut CallbackInfo, path: &str) -> Update {
    let mut app_ref = app.clone();
    let Some(mut s) = app_ref.downcast_mut::<Player>() else {
        return Update::DoNothing;
    };
    let shelf = Shelf::ALL
        .into_iter()
        .find(|shelf| s.items(*shelf).iter().any(|i| i.path == path));
    let Some(shelf) = shelf else {
        s.notice("Only a file of a library is deleted here.");
        return Update::RefreshDom;
    };
    let plays = s.video.as_ref().is_some_and(|v| v.path == path)
        || s.music.as_ref().is_some_and(|m| m.paths.iter().any(|p| p == path));
    if plays {
        s.notice("It is playing or queued: stop it first.");
        return Update::RefreshDom;
    }
    match std::fs::remove_file(path) {
        Ok(()) => {
            s.library.shelf_mut(shelf).items.retain(|i| i.path != path);
            s.history.remove(path);
            println!("AZPLAYER_DELETED {path}");
            let title = library::file_title(Path::new(path));
            s.notice(&format!("{title} was deleted."));
            app::save_library(app, &s, info);
            app::save_history(app, &s, info);
        }
        Err(e) => open(
            &mut s,
            overlay::error(
                "the file was not deleted",
                vec![path.to_string(), e.to_string()],
            ),
        ),
    }
    Update::RefreshDom
}
