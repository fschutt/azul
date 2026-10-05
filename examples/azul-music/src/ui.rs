//! The window: azul's MediaShell - the sidebar (the library, the playlists), the content (the
//! songs table, the albums, the artists, a playlist, the empty state), the now-playing bar - in the
//! theme scope, with the settings page's own section (the music folder).

use azul::{
    callbacks::{
        ButtonOnClickCallbackType, DataTableDataSourceCallbackType, DataTableOnEventCallbackType,
        MediaControlsOnActionCallbackType, SeekBarOnSeekCallbackType,
    },
    css::ApplicationEventFilter,
    dialog::{FileDialog, FileOpenResult},
    option::OptionString,
    prelude::*,
    shells::{MediaShell, ShellEmptyState, ShellThemeAccent, ShellThemeScope},
    str::String as AzString,
    widgets::{
        Button, ButtonType, DataTable, DataTableCell, DataTableCellRef, DataTableColumn,
        DataTableEvent, DataTableEventKind, DataTableSortKind, LevelMeter, MediaControls,
        MediaRepeat, SeekBar,
    },
};
use azul_appkit::ui as kit;

use crate::{
    app::{self, LibraryRef, Music, View, FOLDER_SETTING, SPEC},
    ids,
    library::Library,
    queue::Repeat,
};

/// The px the chrome takes over and under the songs table (title row, now-playing bar, status).
const CHROME_HEIGHT: f32 = 150.0;
/// The px the sidebar takes beside the table.
const SIDEBAR_WIDTH: f32 = 240.0;

/// The window: the MediaShell in the theme scope; the keys and the desktop's media requests on
/// the body.
pub extern "C" fn layout(mut data: RefAny, info: LayoutCallbackInfo) -> Dom {
    // Reading the mode makes a light / dark switch rebuild the window.
    let _mode = info.get_mode();
    let window = (info.get_window_width(), info.get_window_height());
    let app = data.clone();
    if let Some(mut s) = data.downcast_mut::<Music>() {
        if window.0 > 0.0 && window.1 > 0.0 {
            s.window = window;
        }
    }
    let Some(guard) = data.downcast_ref::<Music>() else {
        return Dom::create_body();
    };
    let s = &*guard;
    let content = if kit::settings_open(&s.kit) {
        kit::settings_page(&s.kit, vec![library_section(s, &app)])
    } else {
        content(s, &app)
    };
    let shell = MediaShell::create(sidebar(s, &app), content, now_playing_bar(s, &app));
    let column = Dom::create_div()
        .with_css("display: flex; flex-direction: column; flex-grow: 1; min-height: 0px;")
        .with_child(
            shell
                .office_shell()
                .with_title_row(kit::title_row(SPEC.name))
                .dom(),
        );
    ShellThemeScope::create(column)
        .with_accent(ShellThemeAccent::Leaf)
        .body()
        .with_callback(
            EventFilter::Window(WindowEventFilter::VirtualKeyDown),
            app.clone(),
            app::on_key,
        )
        .with_callback(
            EventFilter::Application(ApplicationEventFilter::MediaControl),
            app,
            app::on_media_control,
        )
}

// ==== The sidebar ====

/// A sidebar entry: a link button, the current one primary.
fn nav(
    label: &str,
    id: AzString,
    current: bool,
    data: RefAny,
    action: ButtonOnClickCallbackType,
) -> Dom {
    let kind = if current {
        ButtonType::Primary
    } else {
        ButtonType::Link
    };
    Button::with_type(label, kind)
        .with_on_click(data, action)
        .dom()
        .with_id(id)
        .with_css("margin: 2px 8px; align-self: stretch;")
}

fn section_label(text: &str) -> Dom {
    Dom::create_p_with_text(text)
        .with_css("margin: 10px 12px 4px 12px; font-size: 11px; font-weight: 600; opacity: 0.7;")
}

/// The sidebar: the library, then the playlists.
fn sidebar(s: &Music, app: &RefAny) -> Dom {
    let mut column = Dom::create_div()
        .with_css("display: flex; flex-direction: column; padding: 8px 0px; overflow-y: auto;");
    column.add_child(section_label("Library"));
    column.add_child(nav(
        "Songs",
        ids::NAV_SONGS,
        s.view == View::Songs,
        app.clone(),
        on_nav_songs,
    ));
    column.add_child(nav(
        "Albums",
        ids::NAV_ALBUMS,
        s.view == View::Albums,
        app.clone(),
        on_nav_albums,
    ));
    column.add_child(nav(
        "Artists",
        ids::NAV_ARTISTS,
        s.view == View::Artists,
        app.clone(),
        on_nav_artists,
    ));
    column.add_child(section_label("Playlists"));
    for p in &s.playlists {
        let current = s.view == View::Playlist(p.id.clone());
        let kind = if current {
            ButtonType::Primary
        } else {
            ButtonType::Link
        };
        column.add_child(
            Button::with_type(p.name.as_str(), kind)
                .with_on_click(
                    RefAny::new(PlaylistPick {
                        app: app.clone(),
                        id: p.id.clone(),
                    }),
                    on_nav_playlist as ButtonOnClickCallbackType,
                )
                .dom()
                .with_css("margin: 2px 8px; align-self: stretch;"),
        );
    }
    column.add_child(nav(
        "New playlist from the queue",
        ids::NAV_NEW_PLAYLIST,
        false,
        app.clone(),
        app::on_new_playlist,
    ));
    column
}

// ==== The content ====

/// The content: the empty state, the songs, the albums, the artists or a playlist.
fn content(s: &Music, app: &RefAny) -> Dom {
    let library = s.library();
    if library.tracks.is_empty() {
        let detail = if s.loaded {
            format!(
                "Scan your music folder ({}) or try the six sample tones.",
                s.music_folder().display()
            )
        } else {
            String::from("Reading the library...")
        };
        return Dom::create_div()
            .with_css("display: flex; flex-direction: column; flex-grow: 1;")
            .with_child(
                ShellEmptyState::create("No music yet")
                    .with_icon("library_music")
                    .with_detail(detail.as_str())
                    .with_action_label("Scan the music folder")
                    .with_on_action(app.clone(), on_scan as ButtonOnClickCallbackType)
                    .dom()
                    .with_id(ids::EMPTY),
            )
            .with_child(
                Button::create("Use the sample library")
                    .with_on_click(app.clone(), on_sample as ButtonOnClickCallbackType)
                    .dom()
                    .with_css("align-self: center; margin: 8px;"),
            );
    }
    match &s.view {
        View::Songs => songs_table(s, app, &library),
        View::Albums => albums_list(app, &library),
        View::Artists => artists_list(app, &library),
        View::Playlist(id) => playlist_list(s, app, &library, id),
    }
}

/// The songs table's columns.
#[must_use]
pub fn columns() -> Vec<DataTableColumn> {
    vec![
        DataTableColumn::create("Title", 260.0, DataTableSortKind::Text),
        DataTableColumn::create("Artist", 200.0, DataTableSortKind::Text),
        DataTableColumn::create("Album", 200.0, DataTableSortKind::Text),
        DataTableColumn::create("#", 50.0, DataTableSortKind::Number),
        DataTableColumn::create("Time", 70.0, DataTableSortKind::Number),
        DataTableColumn::create("Genre", 120.0, DataTableSortKind::Text),
        DataTableColumn::create("Year", 70.0, DataTableSortKind::Number),
    ]
}

/// The data callback: cell `at` of the library (the app's row).
pub extern "C" fn cell(mut source: RefAny, at: DataTableCellRef) -> DataTableCell {
    let Some(lib) = source.downcast_ref::<LibraryRef>() else {
        return DataTableCell::empty();
    };
    let Some(t) = lib.library.tracks.get(at.row as usize) else {
        return DataTableCell::empty();
    };
    let (text, value) = match at.column {
        0 => (t.display_title(), 0.0),
        1 => (t.artist.clone(), 0.0),
        2 => (t.album.clone(), 0.0),
        3 => (
            if t.track_no > 0 {
                t.track_no.to_string()
            } else {
                String::new()
            },
            f64::from(t.track_no),
        ),
        4 => (
            SeekBar::media_time(t.duration_s).as_str().to_string(),
            t.duration_s,
        ),
        5 => (t.genre.clone(), 0.0),
        _ => (t.year.clone(), t.year.parse::<f64>().unwrap_or(0.0)),
    };
    let cell = DataTableCell {
        text: AzString::from(text),
        value,
    };
    cell
}

/// Every action in the table: the view is stored; Enter / a double-click plays from that row.
extern "C" fn on_table_event(
    mut app: RefAny,
    mut info: CallbackInfo,
    event: DataTableEvent,
) -> Update {
    let handle = app.clone();
    let Some(mut s) = app.downcast_mut::<Music>() else {
        return Update::DoNothing;
    };
    s.table = event.view.clone();
    if event.kind == DataTableEventKind::Activate {
        app::play_from_table(&mut s, &mut info, &handle, event.cell.row);
    }
    Update::RefreshDom
}

fn songs_table(s: &Music, app: &RefAny, library: &Library) -> Dom {
    let rows = u32::try_from(library.tracks.len()).unwrap_or(u32::MAX);
    let (width, height) = s.window;
    DataTable::create(columns(), rows)
        .with_id(ids::SONGS)
        .with_accessibility_name("Songs")
        .with_view(s.table.clone())
        .with_viewport(
            (width - SIDEBAR_WIDTH).max(300.0),
            (height - CHROME_HEIGHT).max(160.0),
        )
        .with_show_filter_row(true)
        .with_read_only(true)
        .with_data_source(s.library.clone(), cell as DataTableDataSourceCallbackType)
        .with_on_event(app.clone(), on_table_event as DataTableOnEventCallbackType)
        .dom()
}

/// What a list row's Play button plays.
struct ListPlay {
    app: RefAny,
    ids: Vec<String>,
}

/// A row of a list: a title, a detail, a Play button.
fn list_row(title: &str, detail: &str, play: ListPlay) -> Dom {
    Dom::create_div()
        .with_css(
            "display: flex; flex-direction: row; align-items: center; padding: 6px 12px; \
             border-bottom: 1px solid system:separator;",
        )
        .with_child(
            Dom::create_div()
                .with_css("display: flex; flex-direction: column; flex-grow: 1; min-width: 0px;")
                .with_child(
                    Dom::create_p_with_text(title).with_css("margin: 0px; font-size: 14px;"),
                )
                .with_child(
                    Dom::create_p_with_text(detail)
                        .with_css("margin: 0px; font-size: 12px; color: system:secondary-text;"),
                ),
        )
        .with_child(
            Button::with_type("Play", ButtonType::Default)
                .with_icon("play_arrow")
                .with_on_click(RefAny::new(play), on_play_list as ButtonOnClickCallbackType)
                .dom(),
        )
}

fn scroll_list(id: AzString, rows: Vec<Dom>) -> Dom {
    Dom::create_div()
        .with_id(id)
        .with_css(
            "display: flex; flex-direction: column; flex-grow: 1; overflow-y: auto; min-height: 0px;",
        )
        .with_children(rows.into())
}

fn albums_list(app: &RefAny, library: &Library) -> Dom {
    // TODO(WIDGETS9A): IconGrid of covers (AudioFileInfo::cover per album).
    let rows = library
        .albums()
        .into_iter()
        .map(|a| {
            let year = if a.year.is_empty() {
                String::new()
            } else {
                format!(" - {}", a.year)
            };
            let detail = format!(
                "{} - {} songs, {}{year}",
                a.artist,
                a.tracks.len(),
                SeekBar::media_time(a.duration_s).as_str()
            );
            let ids = a
                .tracks
                .iter()
                .map(|i| library.tracks[*i].id.clone())
                .collect();
            list_row(
                &a.title,
                &detail,
                ListPlay {
                    app: app.clone(),
                    ids,
                },
            )
        })
        .collect();
    scroll_list(ids::ALBUMS, rows)
}

fn artists_list(app: &RefAny, library: &Library) -> Dom {
    let albums = library.albums();
    let rows = library
        .artists()
        .into_iter()
        .map(|(artist, count)| {
            let ids = albums
                .iter()
                .filter(|a| a.artist == artist)
                .flat_map(|a| a.tracks.iter().map(|i| library.tracks[*i].id.clone()))
                .collect();
            list_row(
                &artist,
                &format!("{count} songs"),
                ListPlay {
                    app: app.clone(),
                    ids,
                },
            )
        })
        .collect();
    scroll_list(ids::ARTISTS, rows)
}

fn playlist_list(s: &Music, app: &RefAny, library: &Library, id: &str) -> Dom {
    let Some(playlist) = s.playlists.iter().find(|p| p.id == id) else {
        return scroll_list(ids::PLAYLIST, Vec::new());
    };
    let mut rows = vec![list_row(
        &playlist.name,
        &format!("{} songs", playlist.tracks.len()),
        ListPlay {
            app: app.clone(),
            ids: playlist.tracks.clone(),
        },
    )];
    for (n, track_id) in playlist.tracks.iter().enumerate() {
        if let Some(t) = library.index_of(track_id).map(|i| &library.tracks[i]) {
            rows.push(list_row(
                &format!("{}. {}", n + 1, t.display_title()),
                &format!(
                    "{} - {}",
                    t.artist,
                    SeekBar::media_time(t.duration_s).as_str()
                ),
                ListPlay {
                    app: app.clone(),
                    ids: playlist.tracks[n..].to_vec(),
                },
            ));
        }
    }
    scroll_list(ids::PLAYLIST, rows)
}

// ==== The now-playing bar ====

/// The now-playing bar: the track, the controls, the seek bar, the meter; the status under it.
fn now_playing_bar(s: &Music, app: &RefAny) -> Dom {
    let (title, artist) = match s.heard() {
        Some(t) => (t.display_title(), t.artist.clone()),
        None => (String::from("Nothing playing"), String::new()),
    };
    let repeat = match s.queue.repeat {
        Repeat::Off => MediaRepeat::Off,
        Repeat::All => MediaRepeat::All,
        Repeat::One => MediaRepeat::One,
    };
    let volume = if s.player.is_some() {
        s.state.volume
    } else {
        1.0
    };
    let mut bar = Dom::create_div().with_id(ids::NOW_PLAYING).with_css(
        "display: flex; flex-direction: row; align-items: center; padding: 6px 12px; flex-grow: 1;",
    );
    bar.add_child(
        Dom::create_div()
            .with_css(
                "display: flex; flex-direction: column; width: 200px; flex-shrink: 0; min-width: 0px;",
            )
            .with_child(Dom::create_p_with_text(title.as_str()).with_id(ids::NOW_TITLE).with_css(
                "margin: 0px; font-size: 13px; font-weight: 600; white-space: nowrap; overflow: hidden;",
            ))
            .with_child(
                Dom::create_p_with_text(artist.as_str())
                    .with_id(ids::NOW_ARTIST)
                    .with_css("margin: 0px; font-size: 12px; white-space: nowrap; overflow: hidden;"),
            ),
    );
    bar.add_child(
        MediaControls::create(s.state.playing)
            .with_shuffle_repeat(s.queue.shuffle, repeat)
            .with_volume(volume)
            .with_accessibility_name("Player")
            .with_on_action(
                app.clone(),
                app::on_controls as MediaControlsOnActionCallbackType,
            )
            .dom()
            .with_id(ids::CONTROLS),
    );
    bar.add_child(
        SeekBar::create(s.state.position_s, s.state.duration_s)
            .with_accessibility_name("Position in the song")
            .with_on_seek(app.clone(), app::on_seek as SeekBarOnSeekCallbackType)
            .dom()
            .with_id(ids::SEEK)
            .with_marker(OptionString::Some(ids::SEEK))
            .with_css("flex-grow: 1; margin: 0px 12px;"),
    );
    bar.add_child(
        LevelMeter::create(0.0)
            .with_accessibility_name("Level")
            .dom()
            .with_id(ids::LEVEL)
            .with_marker(OptionString::Some(ids::LEVEL))
            .with_css("width: 80px; flex-grow: 0;"),
    );
    let status = if s.status.is_empty() {
        format!("{} songs", s.library().tracks.len())
    } else {
        s.status.clone()
    };
    Dom::create_div()
        .with_css("display: flex; flex-direction: column; flex-grow: 1;")
        .with_child(bar)
        .with_child(
            Dom::create_p_with_text(status.as_str())
                .with_id(ids::STATUS)
                .with_css("margin: 0px 12px 4px 12px; font-size: 11px; opacity: 0.75;"),
        )
}

// ==== The settings page's own section ====

/// The music folder: choose it, scan it.
fn library_section(s: &Music, app: &RefAny) -> kit::AppSection {
    let folder = s.music_folder();
    let content = Dom::create_div()
        .with_css("display: flex; flex-direction: column;")
        .with_child(kit::row(
            "Music folder",
            Dom::create_div()
                .with_css("display: flex; flex-direction: row; align-items: center;")
                .with_child(
                    Dom::create_p_with_text(folder.display().to_string().as_str())
                        .with_id(ids::FOLDER)
                        .with_css("margin: 0px 8px 0px 0px; font-size: 13px;"),
                )
                .with_child(
                    Button::create("Choose...")
                        .with_on_click(app.clone(), on_choose_folder as ButtonOnClickCallbackType)
                        .dom(),
                ),
        ))
        .with_child(kit::row(
            "Library",
            Button::create(if s.scanning {
                "Scanning..."
            } else {
                "Scan the music folder"
            })
            .with_on_click(app.clone(), on_scan as ButtonOnClickCallbackType)
            .dom()
            .with_id(ids::SCAN),
        ))
        .with_child(kit::note(
            "AzMusic reads MP3, AAC / M4A, ALAC, FLAC, Ogg Vorbis, Opus (Apple), WAV and AIFF where \
             they are; only the library (tags) and the playlists are kept in the data folder.",
        ));
    kit::AppSection {
        category: 0,
        title: String::from("Library"),
        content,
    }
}

// ==== Small handlers ====

fn set_view(data: &mut RefAny, view: View) -> Update {
    if let Some(mut s) = data.downcast_mut::<Music>() {
        s.view = view;
    }
    Update::RefreshDom
}

extern "C" fn on_nav_songs(mut data: RefAny, _info: CallbackInfo) -> Update {
    set_view(&mut data, View::Songs)
}

extern "C" fn on_nav_albums(mut data: RefAny, _info: CallbackInfo) -> Update {
    set_view(&mut data, View::Albums)
}

extern "C" fn on_nav_artists(mut data: RefAny, _info: CallbackInfo) -> Update {
    set_view(&mut data, View::Artists)
}

/// What a playlist entry in the sidebar opens.
struct PlaylistPick {
    app: RefAny,
    id: String,
}

extern "C" fn on_nav_playlist(mut data: RefAny, _info: CallbackInfo) -> Update {
    let Some((mut app, id)) = data
        .downcast_ref::<PlaylistPick>()
        .map(|p| (p.app.clone(), p.id.clone()))
    else {
        return Update::DoNothing;
    };
    set_view(&mut app, View::Playlist(id))
}

extern "C" fn on_play_list(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, ids)) = data
        .downcast_ref::<ListPlay>()
        .map(|p| (p.app.clone(), p.ids.clone()))
    else {
        return Update::DoNothing;
    };
    let handle = app.clone();
    let Some(mut s) = app.downcast_mut::<Music>() else {
        return Update::DoNothing;
    };
    app::play_ids(&mut s, &mut info, &handle, ids, 0);
    Update::RefreshDom
}

extern "C" fn on_scan(data: RefAny, mut info: CallbackInfo) -> Update {
    app::start_scan(&data, &mut info)
}

extern "C" fn on_sample(data: RefAny, mut info: CallbackInfo) -> Update {
    app::write_sample(&data, &mut info);
    Update::RefreshDom
}

extern "C" fn on_choose_folder(data: RefAny, _info: CallbackInfo) -> Update {
    let _request = FileDialog::open_directory(
        "Choose the music folder",
        OptionString::None,
        data,
        on_folder_chosen,
    );
    Update::DoNothing
}

extern "C" fn on_folder_chosen(mut data: RefAny, mut info: CallbackInfo, result: RefAny) -> Update {
    let Some(picked) = FileOpenResult::downcast(result).into_option() else {
        return Update::DoNothing;
    };
    let Some(path) = picked.path.into_option() else {
        return Update::DoNothing;
    };
    let folder = path.inner.as_str().to_string();
    let Some(kit_ref) = data.downcast_ref::<Music>().map(|s| s.kit.clone()) else {
        return Update::DoNothing;
    };
    kit::set_value(&kit_ref, &mut info, FOLDER_SETTING, &folder);
    app::start_scan(&data, &mut info)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::Track;

    #[test]
    fn the_songs_table_has_title_artist_album_number_time_genre_year() {
        let titles: Vec<String> = columns()
            .iter()
            .map(|c| c.title.as_str().to_string())
            .collect();
        assert_eq!(
            titles,
            vec!["Title", "Artist", "Album", "#", "Time", "Genre", "Year"]
        );
    }

    #[test]
    fn a_cell_is_the_tracks_text_and_its_sort_value() {
        let mut library = Library::default();
        library.tracks.push(Track {
            id: "a".into(),
            path: "/m/a.flac".into(),
            title: "First Light".into(),
            track_no: 1,
            duration_s: 562.0,
            year: "2024".into(),
            ..Track::default()
        });
        let source = RefAny::new(LibraryRef { library });
        let time = cell(source.clone(), DataTableCellRef { row: 0, column: 4 });
        assert_eq!(time.text.as_str(), "9:22");
        assert_eq!(time.value, 562.0);
        let title = cell(source.clone(), DataTableCellRef { row: 0, column: 0 });
        assert_eq!(title.text.as_str(), "First Light");
        let past = cell(source, DataTableCellRef { row: 9, column: 0 });
        assert_eq!(past.text.as_str(), "");
    }
}
