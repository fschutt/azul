//! The window, in the look of Spotify's 2010 desktop player (`look.rs`): charcoal panes edge to
//! edge under glossy grey bars, the lime of the old logo for what plays and for Play. The
//! player's own hand everywhere, in every theme (`HAND`: the platform's UI sans), and nothing in
//! it is selectable text but the search field.
//!
//! - THE TOOL BAR is the window's title bar (the window is `NoTitle`, there is no title row):
//!   back and forward (round, glossy), the search field, the status on the right; the bar moves
//!   the window and a double click on it zooms, clear of the window's own controls.
//! - THE SIDEBAR: Play Queue; LIBRARY - Recently Added, Artists, Albums, Songs, Genres; PLAYLISTS -
//!   the user's, then "New Playlist"; at its foot the cover of the song that plays.
//! - THE PAGE (a VirtualView: only the lines in view are built, `page.rs` says what they are): a
//!   grid of square covers (Recently Added, Albums, a genre's albums), round initials (Artists),
//!   coloured tiles (Genres); an album's header - big cover, title, artist, year, Play and
//!   Shuffle - over its songs; an artist's albums, each cover beside its songs; dense striped
//!   song tables (Songs, a playlist, the search's songs, the queue). A song under the pointer
//!   shows a play icon (the page is re-rendered in place, not the window); a double-click plays
//!   from it; a right click offers Play Next, Add to Queue, Add to Playlist.
//! - THE NOW-PLAYING BAR: previous, the big play / pause, next; the song and its artist; the time
//!   played, the seek bar, the length; shuffle, repeat, the queue; mute, the volume, the meter.
//!
//! Album art: the library keeps no pictures yet, so a cover is the album's initials on a colour
//! of its own (`art.rs`).

use azul::{
    callbacks::{
        ButtonOnClickCallbackType, CallbackType, SeekBarOnSeekCallbackType,
        SliderOnValueChangeCallbackType, TextInputOnTextInputCallbackType,
    },
    css::{ApplicationEventFilter, DarkLightMode},
    dialog::{FileDialog, FileOpenResult},
    dom::TabIndex,
    menu::{Menu, MenuItem, StringMenuItem},
    option::OptionString,
    prelude::*,
    shells::{ShellEmptyState, ShellThemeAccent, ShellThemeScope},
    str::String as AzString,
    widgets::{
        Button, LevelMeter, MediaControlsAction, OnTextInputReturn, SeekBar, Slider, SliderState,
        TextInputState, TextInputValid, UiTheme,
    },
};
use azul_appkit::ui as kit;

use crate::{
    app::{self, LibraryRef, Music, FOLDER_SETTING},
    art, ids,
    library::Library,
    look::{self, Look},
    page::{
        self, CardKind, Hero, HeroKind, Hover, Line, Page, Rows, SortKey, View, ALBUM_ROW_H,
        BLOCK_COVER, BLOCK_HEAD_H, CARD_GAP, GENRE_RATIO, PAD_X,
    },
    queue::Repeat,
};

/// The sidebar's width, px.
const SIDEBAR_W: f32 = 220.0;
/// The tool bar's height, px: it is the window's title bar.
const TOOLBAR_H: f32 = 40.0;
/// A column that fills what it is given.
const COLUMN: &str = "display: flex; flex-direction: column; min-height: 0px; min-width: 0px;";
/// The player's own hand, on the window's root AND on the page's root: a VirtualView's DOM is
/// styled on its own and inherits nothing from the window, so the page's tables, which set no
/// family, fell to the engine's default serif. The platform's UI sans (`system:ui`: SF on macOS,
/// Segoe UI on Windows, the desktop's font on Linux) at the tables' 12px, in every theme - a
/// theme's chrome hand (flora sets its scopes in Garamond) stops here. And the player is chrome,
/// not text: `user-select` inherits, so no label, cell or title is selectable (the search field
/// says `text` again), and the pointer stays an arrow over the words.
const HAND: &str = "font-family: system:ui; font-size: 12px; user-select: none; cursor: default;";
/// A control in the tool bar - the window's title bar - keeps its press: the framework's walk up
/// to the bar's `drag` stops at it.
const NO_DRAG: &str = "-azul-app-region: no-drag;";

// ==== Small parts ====

/// One line of text, cut with an ellipsis when it does not fit.
fn text(t: &str, css: &str) -> Dom {
    Dom::create_p_with_text(t).with_css(format!(
        "margin: 0px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; {css}"
    ))
}

/// A line of text in a box of its own, the text node marked `marker` (rewritten in place by the
/// playback timer).
fn live_text(t: &str, marker: AzString, css: &str) -> Dom {
    Dom::create_div()
        .with_id(marker.clone())
        .with_css(css)
        .with_child(
            Dom::create_text_do_not_use_without_block_level_wrapper(t)
                .with_marker(OptionString::Some(marker)),
        )
}

/// A round glossy button (2010's transport): `icon` on a grey gloss, lime when `on` (a toggle
/// that is on); it does `cb` with `data` and is named `name` for assistive technology. `cb`
/// `None`: shown greyed, doing nothing (Back with no page to go back to).
fn round_button(
    look: &Look,
    (icon, name): (&str, &str),
    size: f32,
    on: bool,
    id: AzString,
    action: Option<(RefAny, CallbackType)>,
) -> Dom {
    let icon_px = (size * 0.58).round();
    let radius = size / 2.0;
    let ink = if on { look.accent } else { look.button_text };
    let enabled = action.is_some();
    let face = if enabled {
        format!(":hover {{ background: {}; }}", look.button_hover)
    } else {
        String::from("opacity: 0.35;")
    };
    let mut button = Dom::create_div()
        .with_id(id)
        .with_css(format!(
            "display: flex; flex-shrink: 0; width: {size}px; height: {size}px; margin-left: 6px; \
             cursor: pointer; {NO_DRAG}"
        ))
        .with_accessibility_name(name)
        // The box takes the click, the focus and the name; its icon is the face (a `:hover` on
        // a leaf lights only itself).
        .with_child(Dom::create_icon(icon).with_css(format!(
            "display: flex; align-items: center; justify-content: center; box-sizing: \
             border-box; width: {size}px; height: {size}px; border-radius: {radius}px; \
             font-size: {icon_px}px; color: {ink}; background: {}; border: 1px solid {}; {face}",
            look.button, look.button_rim
        )));
    if let Some((data, cb)) = action {
        button = button
            .with_tab_index(TabIndex::Auto)
            .with_callback(EventFilter::Hover(HoverEventFilter::Click), data, cb);
    }
    button
}

/// A pill-shaped button with an icon and a word: the lime Play (`primary`) or a grey one.
fn pill(
    look: &Look,
    (icon, label): (&str, &str),
    primary: bool,
    id: AzString,
    data: RefAny,
    cb: CallbackType,
) -> Dom {
    let (ground, ink, rim) = if primary {
        (look.play, look.on_play, "rgba(0, 0, 0, 0.5)")
    } else {
        (look.button, look.button_text, look.button_rim)
    };
    Dom::create_div()
        .with_id(id)
        .with_css(format!(
            "display: flex; flex-direction: row; align-items: center; flex-shrink: 0; height: \
             28px; padding: 0px 16px 0px 10px; margin-right: 10px; box-sizing: border-box; \
             border-radius: 14px; border: 1px solid {rim}; background: {ground}; cursor: \
             pointer;"
        ))
        .with_tab_index(TabIndex::Auto)
        .with_accessibility_name(label)
        .with_callback(EventFilter::Hover(HoverEventFilter::Click), data, cb)
        .with_child(
            Dom::create_icon(icon)
                .with_css(format!("font-size: 18px; color: {ink}; margin-right: 6px;")),
        )
        .with_child(text(
            label,
            &format!("font-size: 12px; font-weight: 700; color: {ink};"),
        ))
}

/// A cover with no picture: the initials of `label` on the colour of `seed`, square (an album,
/// a genre, a playlist) or round (an artist).
fn cover(seed: &str, label: &str, size: f32, round: bool, look: &Look) -> Dom {
    let radius = if round { size / 2.0 } else { 2.0 };
    let font = (size * 0.3).round().max(9.0);
    let initials = art::initials(label);
    let face = if initials.is_empty() {
        Dom::create_icon("music_note").with_css(format!(
            "font-size: {font}px; color: rgba(255, 255, 255, 0.85);"
        ))
    } else {
        Dom::create_p_with_text(initials.as_str()).with_css(format!(
            "margin: 0px; font-size: {font}px; font-weight: 700; color: rgba(255, 255, 255, \
             0.92);"
        ))
    };
    Dom::create_div()
        .with_css(format!(
            "position: relative; display: flex; align-items: center; justify-content: center; \
             flex-shrink: 0; box-sizing: border-box; width: {size}px; height: {size}px; \
             border-radius: {radius}px; border: 1px solid rgba(0, 0, 0, 0.55); background: {}; \
             box-shadow: {}; overflow: hidden;",
            art::background(seed),
            look.shadow
        ))
        .with_child(face)
}

/// The green play button over a cover or a tile under the pointer: plays `pick`'s songs.
fn cover_play(look: &Look, title: &str, pick: RefAny) -> Dom {
    Dom::create_div()
        .with_class(ids::CARD_PLAY)
        .with_css(
            "position: absolute; right: 8px; bottom: 8px; display: flex; width: 36px; height: \
             36px; cursor: pointer;",
        )
        .with_tab_index(TabIndex::Auto)
        .with_accessibility_name(format!("Play {title}"))
        .with_callback(
            EventFilter::Hover(HoverEventFilter::Click),
            pick,
            on_card_play,
        )
        .with_child(Dom::create_icon("play_arrow").with_css(format!(
            "display: flex; align-items: center; justify-content: center; box-sizing: \
             border-box; width: 36px; height: 36px; border-radius: 18px; font-size: 24px; \
             color: {}; background: {}; border: 1px solid rgba(0, 0, 0, 0.5); box-shadow: {}; \
             :hover {{ background: {}; }}",
            look.on_play, look.play, look.shadow, look.play_hover
        )))
}

// ==== The window ====

/// The window: the tool bar (its title bar), the sidebar beside the page, the now-playing bar -
/// in the theme scope, in the player's own hand; the keys and the desktop's media requests on
/// the body.
pub extern "C" fn layout(mut data: RefAny, info: LayoutCallbackInfo) -> Dom {
    // Reading the mode makes a light / dark switch rebuild the window.
    let dark = matches!(info.get_mode(), DarkLightMode::Dark);
    let look = look::of(dark);
    let window = (info.get_window_width(), info.get_window_height());
    let app = data.clone();
    if let Some(mut s) = data.downcast_mut::<Music>() {
        s.dark = dark;
        if window.0 > 0.0 && window.1 > 0.0 {
            s.window = window;
        }
    }
    let Some(guard) = data.downcast_ref::<Music>() else {
        return Dom::create_body();
    };
    let s = &*guard;
    let middle = Dom::create_div()
        .with_css("display: flex; flex-direction: row; flex-grow: 1; min-height: 0px;")
        .with_child(sidebar(s, &app, look))
        .with_child(main_pane(s, &app, look));
    let root = Dom::create_div()
        .with_css(format!(
            "{COLUMN} {HAND} flex-grow: 1; background: {}; color: {};",
            look.page, look.text
        ))
        .with_child(toolbar(s, &app, look))
        .with_child(middle)
        .with_child(now_playing_bar(s, &app, look));
    ShellThemeScope::create(root)
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

/// The tool bar, which IS the window's title bar (the window is `NoTitle`; a title row over it
/// only said "AzMusic" and took 28px): back, forward, the search field; the status on the
/// right. The bar moves the window - `-azul-app-region: drag`, which the framework hands to the
/// window manager, and on which a double click zooms (maximizes) - and leaves the window's own
/// controls their room: the traffic lights' width before Back on macOS, the software controls'
/// after the status on Linux, nothing on Windows (its caption stays above). Those are azul's
/// `TabsInTitlebar::platform()` offsets, the ones the ribbon apps' tab strips take. Its buttons
/// and the search field say `no-drag`: a press on them is theirs.
fn toolbar(s: &Music, app: &RefAny, look: &Look) -> Dom {
    let chrome = kit::tabs_in_titlebar();
    let back = (!s.back.is_empty()).then(|| (app.clone(), on_back as CallbackType));
    let forward = (!s.forward.is_empty()).then(|| (app.clone(), on_forward as CallbackType));
    let search = TextInput::create_search()
        .with_text(s.query.as_str())
        .with_placeholder("Search")
        .with_accessibility_name("Search the library")
        // The flat field in every theme: flora writes a field's value in Garamond, and the
        // player's hand is a sans.
        .with_theme(UiTheme::Flat)
        .with_on_text_input(app.clone(), on_search as TextInputOnTextInputCallbackType)
        .dom()
        .with_id(ids::SEARCH)
        // The one text in the player a user may select (`HAND` says none), and a control of the
        // title bar.
        .with_css(format!("{NO_DRAG} user-select: text;"));
    let status = if s.status.is_empty() {
        s.with_library(|library| page::count(library.tracks.len(), "song"))
    } else {
        s.status.clone()
    };
    Dom::create_div()
        .with_id(ids::TOOLBAR)
        .with_css(format!(
            "display: flex; flex-direction: row; align-items: center; flex-shrink: 0; height: \
             {TOOLBAR_H}px; padding: 0px {}px 0px {}px; box-sizing: border-box; background: {}; \
             border-bottom: 1px solid {}; -azul-app-region: drag;",
            12.0 + chrome.right,
            6.0 + chrome.left,
            look.bar,
            look.line
        ))
        .with_child(round_button(
            look,
            ("navigate_before", "Back"),
            26.0,
            false,
            ids::BACK,
            back,
        ))
        .with_child(round_button(
            look,
            ("navigate_next", "Forward"),
            26.0,
            false,
            ids::FORWARD,
            forward,
        ))
        .with_child(
            Dom::create_div()
                .with_css("width: 240px; flex-shrink: 0; margin-left: 14px;")
                .with_child(search),
        )
        .with_child(Dom::create_div().with_css("flex-grow: 1;"))
        .with_child(
            text(
                &status,
                &format!("max-width: 50%; font-size: 11px; color: {};", look.muted),
            )
            .with_id(ids::STATUS),
        )
}

// ==== The sidebar ====

/// A small heading in the sidebar.
fn sidebar_heading(label: &str, look: &Look) -> Dom {
    text(
        label,
        &format!(
            "flex-shrink: 0; padding: 14px 12px 4px 12px; font-size: 10px; font-weight: 700; \
             letter-spacing: 1px; color: {};",
            look.muted
        ),
    )
}

/// An entry of the sidebar: its icon and its name; the current one raised.
fn nav_item(
    look: &Look,
    app: &RefAny,
    (icon, label): (&str, &str),
    current: bool,
    view: View,
) -> Dom {
    let (ground, weight, icon_ink) = if current {
        (format!("background: {};", look.sidebar_current), 700, look.text)
    } else {
        (
            format!(":hover {{ background: {}; }}", look.sidebar_hover),
            400,
            look.muted,
        )
    };
    Dom::create_div()
        .with_css(format!(
            "display: flex; flex-direction: row; align-items: center; flex-shrink: 0; height: \
             26px; padding: 0px 12px; cursor: pointer; {ground}"
        ))
        .with_tab_index(TabIndex::Auto)
        .with_accessibility_name(label)
        .with_callback(
            EventFilter::Hover(HoverEventFilter::Click),
            RefAny::new(GoPick {
                app: app.clone(),
                view,
                remember: false,
            }),
            on_go,
        )
        .with_child(Dom::create_icon(icon).with_css(format!(
            "font-size: 16px; color: {icon_ink}; margin-right: 8px; flex-shrink: 0;"
        )))
        .with_child(text(
            label,
            &format!("font-size: 12px; font-weight: {weight}; color: {};", look.text),
        ))
}

/// The sidebar: the queue, the library, the playlists; "New Playlist"; the cover of the song
/// that plays.
fn sidebar(s: &Music, app: &RefAny, look: &Look) -> Dom {
    let mut list = Dom::create_div().with_id(ids::SIDEBAR).with_css(
        "display: flex; flex-direction: column; flex-grow: 1; min-height: 0px; overflow-y: \
         auto; padding: 6px 0px;",
    );
    list.add_child(
        nav_item(
            look,
            app,
            ("playlist_play", "Play Queue"),
            s.view == View::Queue,
            View::Queue,
        )
        .with_id(ids::NAV_QUEUE),
    );
    list.add_child(sidebar_heading("LIBRARY", look));
    let library = [
        ("schedule", "Recently Added", ids::NAV_RECENT, View::RecentlyAdded),
        ("person", "Artists", ids::NAV_ARTISTS, View::Artists),
        ("album", "Albums", ids::NAV_ALBUMS, View::Albums),
        ("music_note", "Songs", ids::NAV_SONGS, View::Songs),
        ("category", "Genres", ids::NAV_GENRES, View::Genres),
    ];
    for (icon, label, id, view) in library {
        let current = s.view == view;
        list.add_child(nav_item(look, app, (icon, label), current, view).with_id(id));
    }
    list.add_child(sidebar_heading("PLAYLISTS", look));
    for p in &s.playlists {
        let view = View::Playlist(p.id.clone());
        let current = s.view == view;
        list.add_child(
            nav_item(look, app, ("queue_music", p.name.as_str()), current, view)
                .with_class(ids::NAV_PLAYLIST),
        );
    }
    let new_playlist = Dom::create_div()
        .with_id(ids::NAV_NEW_PLAYLIST)
        .with_css(format!(
            "display: flex; flex-direction: row; align-items: center; flex-shrink: 0; height: \
             30px; padding: 0px 12px; border-top: 1px solid {}; cursor: pointer; :hover {{ \
             background: {}; }}",
            look.line, look.sidebar_hover
        ))
        .with_tab_index(TabIndex::Auto)
        .with_accessibility_name("New Playlist")
        .with_callback(
            EventFilter::Hover(HoverEventFilter::Click),
            app.clone(),
            app::on_new_playlist,
        )
        .with_child(Dom::create_icon("add").with_css(format!(
            "font-size: 16px; color: {}; margin-right: 8px;",
            look.muted
        )))
        .with_child(text(
            "New Playlist",
            &format!("font-size: 12px; color: {};", look.text),
        ));
    let mut column = Dom::create_div().with_css(format!(
        "{COLUMN} width: {SIDEBAR_W}px; flex-shrink: 0; background: {}; border-right: 1px solid \
         {};",
        look.sidebar, look.line
    ));
    column.add_child(list);
    column.add_child(new_playlist);
    if let Some(t) = s.heard() {
        // 2010's player kept the cover of the song that plays at the foot of the sidebar; a
        // click opens its album.
        let (artist, title) = page::album_key(&t);
        let size = SIDEBAR_W - 1.0;
        column.add_child(
            Dom::create_div()
                .with_id(ids::SIDEBAR_COVER)
                .with_css(format!(
                    "flex-shrink: 0; height: {size}px; border-top: 1px solid {}; cursor: \
                     pointer;",
                    look.line
                ))
                .with_accessibility_name(format!("The album {title}"))
                .with_callback(
                    EventFilter::Hover(HoverEventFilter::Click),
                    RefAny::new(GoPick {
                        app: app.clone(),
                        view: View::Album {
                            artist: artist.clone(),
                            title: title.clone(),
                        },
                        remember: true,
                    }),
                    on_go,
                )
                .with_child(cover(&page::seed(&artist, &title), &title, size, false, look)),
        );
    }
    column
}

// ==== The page's pane ====

/// The pane right of the sidebar: the settings page, the empty state, or the page.
fn main_pane(s: &Music, app: &RefAny, look: &Look) -> Dom {
    let mut pane = Dom::create_div().with_id(ids::MAIN).with_css(format!(
        "{COLUMN} flex-grow: 1; background: {};",
        look.page
    ));
    if kit::settings_open(&s.kit) {
        pane.add_child(kit::settings_page(&s.kit, vec![library_section(s, app)]));
    } else if s.with_library(|library| library.tracks.is_empty()) {
        pane.add_child(empty_library(s, app));
    } else {
        pane.add_child(page_view(s, app));
    }
    pane
}

/// No music yet: scan the music folder, or try the sample tones.
fn empty_library(s: &Music, app: &RefAny) -> Dom {
    let detail = if s.loaded {
        format!(
            "Scan your music folder ({}) or try the six sample tones.",
            s.music_folder().display()
        )
    } else {
        String::from("Reading the library\u{2026}")
    };
    // Flat in every theme: flora sets its shells in Garamond and its buttons in Garamond
    // capitals, and the player's hand is a sans.
    Dom::create_div()
        .with_css("display: flex; flex-direction: column; flex-grow: 1;")
        .with_child(
            ShellEmptyState::create("No music yet")
                .with_icon("library_music")
                .with_detail(detail.as_str())
                .with_action_label("Scan the music folder")
                .with_on_action(app.clone(), on_scan as ButtonOnClickCallbackType)
                .with_theme(UiTheme::Flat)
                .dom()
                .with_id(ids::EMPTY),
        )
        .with_child(
            Button::create("Use the sample library")
                .with_on_click(app.clone(), on_sample as ButtonOnClickCallbackType)
                .with_theme(UiTheme::Flat)
                .dom()
                .with_css("align-self: center; margin: 8px;"),
        )
}

/// The page's id (per page: a new page starts at its top), its class and its name.
fn page_names(s: &Music) -> (AzString, Option<AzString>, String) {
    match &s.view {
        View::RecentlyAdded => (ids::RECENT, None, String::from("Recently Added")),
        View::Artists => (ids::ARTISTS, None, String::from("Artists")),
        View::Albums => (ids::ALBUMS, None, String::from("Albums")),
        View::Songs => (ids::SONGS, None, String::from("Songs")),
        View::Genres => (ids::GENRES, None, String::from("Genres")),
        View::Search => (ids::SEARCH_RESULTS, None, String::from("Search results")),
        View::Queue => (ids::QUEUE, None, String::from("Play Queue")),
        View::Album { artist, title } => (
            ids::numbered("album", s.catalog.album(artist, title).unwrap_or(0)),
            Some(ids::PAGE_ALBUM),
            title.clone(),
        ),
        View::Artist(name) => (
            ids::numbered("artist", s.catalog.artist(name).unwrap_or(0)),
            Some(ids::PAGE_ARTIST),
            name.clone(),
        ),
        View::Genre(name) => (
            ids::numbered("genre", s.catalog.genre(name).unwrap_or(0)),
            Some(ids::PAGE_GENRE),
            name.clone(),
        ),
        View::Playlist(id) => (
            ids::named(&format!("playlist-{id}")),
            Some(ids::PAGE_PLAYLIST),
            s.playlists
                .iter()
                .find(|p| p.id == *id)
                .map_or_else(|| String::from("Playlist"), |p| p.name.clone()),
        ),
    }
}

/// What the page's VirtualView hands its callback: the app.
struct PageRef {
    app: RefAny,
}

/// The page: a VirtualView that builds the lines in view (`render_page`).
fn page_view(s: &Music, app: &RefAny) -> Dom {
    let (id, class, name) = page_names(s);
    let mut view = Dom::create_virtual_view(RefAny::new(PageRef { app: app.clone() }), render_page)
        .with_id(id)
        .with_class(ids::PAGE)
        .with_marker(OptionString::Some(ids::CONTENT))
        .with_accessibility_name(name)
        .with_css("flex-grow: 1; min-height: 0px; width: 100%;")
        .with_callback(
            EventFilter::Hover(HoverEventFilter::MouseLeave),
            app.clone(),
            on_page_leave,
        );
    if let Some(class) = class {
        view = view.with_class(class);
    }
    view
}

/// A rect of the page, px.
fn rect(x: f32, y: f32, w: f32, h: f32) -> LogicalRect {
    LogicalRect::create(LogicalPosition::create(x, y), LogicalSize::create(w, h))
}

/// What every line of a page is drawn with.
struct Ctx<'a> {
    s: &'a Music,
    app: &'a RefAny,
    library: &'a Library,
    look: &'static Look,
    /// A card's width.
    card: f32,
    /// The library's index of the song that plays.
    heard: Option<usize>,
    /// The queue is this page's songs (its Play button pauses).
    plays_page: bool,
}

/// Whether the queue holds exactly the songs `tracks` (the page is what plays).
fn plays_page(s: &Music, library: &Library, tracks: &[usize]) -> bool {
    !tracks.is_empty()
        && s.queue.items.len() == tracks.len()
        && s.queue.items.iter().zip(tracks).all(|(id, t)| {
            library
                .tracks
                .get(*t)
                .is_some_and(|track| track.id == *id)
        })
}

/// The page's VirtualView: the lines in view and a screen either side, nothing more.
extern "C" fn render_page(mut data: RefAny, info: VirtualViewCallbackInfo) -> VirtualViewReturn {
    let Some(mut app) = data.downcast_ref::<PageRef>().map(|p| p.app.clone()) else {
        return VirtualViewReturn::default();
    };
    let handle = app.clone();
    let Some(guard) = app.downcast_ref::<Music>() else {
        return VirtualViewReturn::default();
    };
    let s = &*guard;
    let size = info.bounds.get_logical_size();
    let width = size.width.max(1.0);
    let height = size.height.max(1.0);
    let (columns, card) = page::grid(width);
    let mut library_ref = s.library.clone();
    let Some(library_guard) = library_ref.downcast_ref::<LibraryRef>() else {
        return VirtualViewReturn::default();
    };
    let library = &library_guard.library;
    let built: Page = s.page(library, columns);
    let tops = page::tops(&built.lines, card);
    let total = tops.last().copied().unwrap_or(0.0).max(1.0);
    let y = info.scroll_offset.y.max(0.0);
    let (first, end) = page::slice(&tops, (y - height).max(0.0), y + 2.0 * height);
    let ctx = Ctx {
        s,
        app: &handle,
        library,
        look: look::of(s.dark),
        card,
        heard: s.heard_index(library),
        plays_page: plays_page(s, library, &built.tracks),
    };
    // The page's DOM inherits nothing from the window: its root states the player's hand again.
    let mut root = Dom::create_div().with_css(format!(
        "display: flex; flex-direction: column; width: {width}px; {HAND} color: {};",
        ctx.look.text
    ));
    for i in first..end {
        root.add_child(line_dom(&ctx, &built.lines[i], tops[i + 1] - tops[i]));
    }
    let top = tops.get(first).copied().unwrap_or(0.0);
    let bottom = tops.get(end).copied().unwrap_or(total);
    VirtualViewReturn::with_dom(
        root,
        rect(0.0, top, width, (bottom - top).max(1.0)),
        rect(0.0, 0.0, width, total),
    )
}

/// A line the pointer clears the song or card under it on (a header, a heading, the space at
/// the end).
fn quiet(dom: Dom, ctx: &Ctx<'_>) -> Dom {
    dom.with_callback(
        EventFilter::Hover(HoverEventFilter::MouseEnter),
        ctx.app.clone(),
        on_quiet_enter,
    )
}

/// One line of the page, `height` px.
fn line_dom(ctx: &Ctx<'_>, line: &Line, height: f32) -> Dom {
    match line {
        Line::Hero(hero) => hero_line(ctx, hero, height),
        Line::Title {
            title,
            detail,
            play,
        } => title_line(ctx, title, detail, *play, height),
        Line::Heading(label) => quiet(
            Dom::create_div()
                .with_css(format!(
                    "display: flex; flex-direction: row; align-items: flex-end; flex-shrink: 0; \
                     height: {height}px; padding: 0px {PAD_X}px 6px {PAD_X}px; box-sizing: \
                     border-box;"
                ))
                .with_child(text(
                    label,
                    &format!("font-size: 14px; font-weight: 700; color: {};", ctx.look.text),
                )),
            ctx,
        ),
        Line::Columns(rows) => columns_line(ctx, *rows, height),
        Line::Track { row, track, rows } => {
            let number = ctx
                .library
                .tracks
                .get(*track)
                .map_or(row + 1, |t| track_number(t.track_no, *rows, row + 1));
            track_row(ctx, (*row, *track), *rows, number, (PAD_X, height))
        }
        Line::AlbumBlock {
            album,
            first_row,
            count,
        } => album_block(ctx, *album, *first_row, *count, height),
        Line::Cards { kind, items } => cards_line(ctx, *kind, items, height),
        Line::Empty(why) => quiet(
            Dom::create_div()
                .with_css(format!(
                    "display: flex; align-items: center; justify-content: center; \
                     flex-shrink: 0; height: {height}px; padding: 0px {PAD_X}px;"
                ))
                .with_child(text(
                    why,
                    &format!("font-size: 13px; color: {};", ctx.look.muted),
                )),
            ctx,
        ),
        Line::End => quiet(
            Dom::create_div().with_css(format!("flex-shrink: 0; height: {height}px;")),
            ctx,
        ),
    }
}

/// The number a song's row shows: its track number on an album's page, its place otherwise.
fn track_number(track_no: u32, rows: Rows, place: usize) -> usize {
    match rows {
        Rows::Album if track_no > 0 => usize::try_from(track_no).unwrap_or(place),
        _ => place,
    }
}

/// Play (or Pause, when the page is what plays) and Shuffle.
fn page_buttons(ctx: &Ctx<'_>) -> Dom {
    let playing = ctx.plays_page && ctx.s.state.playing && !ctx.s.state.finished;
    let play = if playing {
        ("pause", "Pause")
    } else {
        ("play_arrow", "Play")
    };
    Dom::create_div()
        .with_css("display: flex; flex-direction: row; align-items: center; margin-top: 14px;")
        .with_child(pill(
            ctx.look,
            play,
            true,
            ids::PAGE_PLAY,
            ctx.app.clone(),
            on_page_play,
        ))
        .with_child(pill(
            ctx.look,
            ("shuffle", "Shuffle"),
            false,
            ids::PAGE_SHUFFLE,
            ctx.app.clone(),
            on_page_shuffle,
        ))
}

/// The header of an album, an artist, a genre or a playlist: the big cover beside the title,
/// who made it, the year and the length, Play and Shuffle.
fn hero_line(ctx: &Ctx<'_>, hero: &Hero, height: f32) -> Dom {
    let look = ctx.look;
    let title_px = if hero.title.chars().count() > 28 {
        22
    } else {
        30
    };
    let mut about = Dom::create_div()
        .with_css(
            "display: flex; flex-direction: column; justify-content: flex-end; flex-grow: 1; \
             min-width: 0px; margin-left: 20px;",
        )
        .with_child(text(
            hero.kind.label(),
            &format!(
                "font-size: 10px; font-weight: 700; letter-spacing: 1px; color: {};",
                look.muted
            ),
        ))
        .with_child(text(
            &hero.title,
            &format!(
                "margin-top: 4px; font-size: {title_px}px; font-weight: 700; color: {};",
                look.text
            ),
        ));
    if !hero.by.is_empty() {
        about.add_child(
            Dom::create_div()
                .with_css(
                    "display: flex; flex-direction: row; align-items: center; margin-top: 4px;",
                )
                .with_child(text(
                    "by\u{a0}",
                    &format!("font-size: 13px; color: {};", look.muted),
                ))
                .with_child(
                    text(
                        &hero.by,
                        &format!(
                            "font-size: 13px; font-weight: 700; color: {}; cursor: pointer; \
                             :hover {{ text-decoration: underline; }}",
                            look.text
                        ),
                    )
                    .with_tab_index(TabIndex::Auto)
                    .with_accessibility_name(format!("The artist {}", hero.by))
                    .with_callback(
                        EventFilter::Hover(HoverEventFilter::Click),
                        RefAny::new(GoPick {
                            app: ctx.app.clone(),
                            view: View::Artist(hero.by.clone()),
                            remember: true,
                        }),
                        on_go,
                    ),
                ),
        );
    }
    about.add_child(text(
        &hero.detail,
        &format!("margin-top: 4px; font-size: 11px; color: {};", look.muted),
    ));
    about.add_child(page_buttons(ctx));
    quiet(
        Dom::create_div()
            .with_css(format!(
                "display: flex; flex-direction: row; align-items: flex-end; flex-shrink: 0; \
                 height: {height}px; padding: 24px {PAD_X}px; box-sizing: border-box; \
                 background: linear-gradient(to bottom, {}, {});",
                look.header_top, look.page
            ))
            .with_child(cover(
                &hero.seed,
                &hero.title,
                160.0,
                hero.kind == HeroKind::Artist,
                look,
            ))
            .with_child(about),
        ctx,
    )
}

/// A library page's title and how much it holds; Play and Shuffle for a song list.
fn title_line(ctx: &Ctx<'_>, title: &str, detail: &str, play: bool, height: f32) -> Dom {
    let look = ctx.look;
    let mut line = Dom::create_div()
        .with_css(format!(
            "display: flex; flex-direction: row; align-items: center; flex-shrink: 0; height: \
             {height}px; padding: 0px {PAD_X}px; box-sizing: border-box; background: \
             linear-gradient(to bottom, {}, {});",
            look.header_top, look.page
        ))
        .with_child(
            Dom::create_div()
                .with_css("display: flex; flex-direction: column; flex-grow: 1; min-width: 0px;")
                .with_child(text(
                    title,
                    &format!("font-size: 22px; font-weight: 700; color: {};", look.text),
                ))
                .with_child(text(
                    detail,
                    &format!("margin-top: 2px; font-size: 11px; color: {};", look.muted),
                )),
        );
    if play {
        line.add_child(
            Dom::create_div()
                .with_css("display: flex; flex-shrink: 0; margin-bottom: 14px;")
                .with_child(page_buttons(ctx)),
        );
    }
    quiet(line, ctx)
}

/// The css of a song table's cell that takes `grow` shares of the width.
fn grow_cell(grow: u32) -> String {
    format!(
        "flex-grow: {grow}; flex-basis: 0px; min-width: 0px; padding-right: 12px; box-sizing: \
         border-box;"
    )
}

/// The css of the lead cell (the number, the play icon) and the time cell.
const LEAD_CELL: &str = "display: flex; flex-direction: row; align-items: center; \
                         justify-content: flex-end; width: 36px; padding-right: 12px; \
                         box-sizing: border-box; flex-shrink: 0;";
const TIME_CELL: &str = "display: flex; flex-direction: row; align-items: center; \
                         justify-content: flex-end; width: 56px; padding-right: 12px; \
                         box-sizing: border-box; flex-shrink: 0;";

/// The column titles of a song list, in the 2010 gradient bar; on the songs page a click
/// sorts by the column (again: the other way round).
fn columns_line(ctx: &Ctx<'_>, rows: Rows, height: f32) -> Dom {
    let look = ctx.look;
    let sortable = ctx.s.view == View::Songs;
    let sort = ctx.s.sort;
    let head = |label: &str, key: SortKey, css: &str| -> Dom {
        let mut cell = Dom::create_div()
            .with_css(format!(
                "display: flex; flex-direction: row; align-items: center; height: {height}px; {css}"
            ))
            .with_child(text(
                label,
                &format!("font-size: 11px; color: {};", look.text),
            ));
        if sortable && sort.key == key && key != SortKey::Library {
            let arrow = if sort.descending {
                "arrow_drop_down"
            } else {
                "arrow_drop_up"
            };
            cell.add_child(
                Dom::create_icon(arrow)
                    .with_css(format!("font-size: 16px; color: {};", look.text)),
            );
        }
        if sortable {
            cell = cell
                .with_css("cursor: pointer;")
                .with_accessibility_name(format!("Sort by {label}"))
                .with_callback(
                    EventFilter::Hover(HoverEventFilter::Click),
                    RefAny::new(SortPick {
                        app: ctx.app.clone(),
                        key,
                    }),
                    on_sort,
                );
        }
        cell
    };
    let mut line = Dom::create_div()
        .with_css(format!(
            "display: flex; flex-direction: row; align-items: center; flex-shrink: 0; height: \
             {height}px; padding: 0px {PAD_X}px; box-sizing: border-box; background: {}; \
             border-top: 1px solid {}; border-bottom: 1px solid {};",
            look.columns, look.line, look.line
        ))
        .with_child(head("#", SortKey::Library, LEAD_CELL))
        .with_child(head("Track", SortKey::Title, grow_cell(4).as_str()));
    match rows {
        Rows::Table => {
            line.add_child(head("Artist", SortKey::Artist, grow_cell(3).as_str()));
            line.add_child(head("Time", SortKey::Time, TIME_CELL));
            line.add_child(head("Album", SortKey::Album, grow_cell(3).as_str()));
        }
        Rows::Album => line.add_child(head("Time", SortKey::Time, TIME_CELL)),
    }
    quiet(line, ctx)
}

/// A song's row: `(row, track)` its place in the page's songs and its library index; `number`
/// what its lead shows; `(pad, height)` its side padding and height. Striped; the song under the
/// pointer shows a play icon, the song that plays the speaker and the lime.
fn track_row(
    ctx: &Ctx<'_>,
    (row, track): (usize, usize),
    rows: Rows,
    number: usize,
    (pad, height): (f32, f32),
) -> Dom {
    let look = ctx.look;
    let s = ctx.s;
    let Some(t) = ctx.library.tracks.get(track) else {
        return Dom::create_div().with_css(format!("flex-shrink: 0; height: {height}px;"));
    };
    let hovered = s.hover == Hover::Row(row);
    let heard = ctx.heard == Some(track);
    let ground = if s.selected == Some(row) {
        look.selected
    } else if hovered {
        look.hover
    } else if row % 2 == 1 {
        look.stripe
    } else {
        look.page
    };
    let ink = if heard { look.accent } else { look.text };
    let pick = || {
        RefAny::new(RowPick {
            app: ctx.app.clone(),
            row,
            track,
        })
    };
    let title = t.display_title();
    let lead = if hovered {
        let (icon, word) = if heard && s.state.playing {
            ("pause", "Pause")
        } else {
            ("play_arrow", "Play")
        };
        Dom::create_icon(icon)
            .with_class(ids::ROW_PLAY)
            .with_css(format!(
                "font-size: 16px; color: {}; cursor: pointer; :hover {{ color: {}; }}",
                look.text, look.accent
            ))
            .with_tab_index(TabIndex::Auto)
            .with_accessibility_name(format!("{word} {title}"))
            .with_callback(
                EventFilter::Hover(HoverEventFilter::Click),
                pick(),
                on_row_play,
            )
    } else if heard {
        let icon = if s.state.playing {
            "volume_up"
        } else {
            "volume_mute"
        };
        Dom::create_icon(icon).with_css(format!("font-size: 14px; color: {};", look.accent))
    } else {
        text(
            &number.to_string(),
            &format!("font-size: 11px; color: {};", look.muted),
        )
    };
    let cell = |content: &str, grow: u32| {
        text(
            content,
            &format!("{} color: {ink};", grow_cell(grow)),
        )
    };
    let time = Dom::create_div().with_css(TIME_CELL).with_child(text(
        SeekBar::media_time(t.duration_s).as_str(),
        &format!("font-size: 11px; color: {};", look.muted),
    ));
    let mut line = Dom::create_div()
        .with_class(ids::TRACK)
        .with_css(format!(
            "display: flex; flex-direction: row; align-items: center; flex-shrink: 0; height: \
             {height}px; padding: 0px {pad}px; box-sizing: border-box; background: {ground}; \
             cursor: default;"
        ))
        .with_accessibility_name(title.clone())
        .with_callback(
            EventFilter::Hover(HoverEventFilter::MouseEnter),
            pick(),
            on_row_enter,
        )
        .with_callback(
            EventFilter::Hover(HoverEventFilter::Click),
            pick(),
            on_row_click,
        )
        .with_callback(
            EventFilter::Hover(HoverEventFilter::DoubleClick),
            pick(),
            on_row_double,
        )
        .with_callback(
            EventFilter::Hover(HoverEventFilter::RightMouseUp),
            pick(),
            on_row_menu,
        )
        .with_child(Dom::create_div().with_css(LEAD_CELL).with_child(lead))
        .with_child(cell(title.as_str(), 4));
    match rows {
        Rows::Table => {
            line.add_child(cell(t.artist.as_str(), 3));
            line.add_child(time);
            line.add_child(cell(t.album.as_str(), 3));
        }
        Rows::Album => line.add_child(time),
    }
    line
}

/// An album on an artist's page: its cover beside its title, its year and its songs.
fn album_block(ctx: &Ctx<'_>, album: usize, first_row: usize, count: usize, height: f32) -> Dom {
    let look = ctx.look;
    let Some(a) = ctx.s.catalog.albums.get(album) else {
        return Dom::create_div().with_css(format!("flex-shrink: 0; height: {height}px;"));
    };
    let open = || {
        RefAny::new(GoPick {
            app: ctx.app.clone(),
            view: View::Album {
                artist: a.artist.clone(),
                title: a.title.clone(),
            },
            remember: true,
        })
    };
    let mut songs = Dom::create_div()
        .with_css(
            "display: flex; flex-direction: column; flex-grow: 1; min-width: 0px; margin-left: \
             20px;",
        )
        .with_child(
            Dom::create_div()
                .with_css(format!(
                    "display: flex; flex-direction: column; justify-content: center; \
                     flex-shrink: 0; height: {BLOCK_HEAD_H}px;"
                ))
                .with_child(
                    text(
                        &a.title,
                        &format!(
                            "font-size: 16px; font-weight: 700; color: {}; cursor: pointer; \
                             :hover {{ text-decoration: underline; }}",
                            look.text
                        ),
                    )
                    .with_tab_index(TabIndex::Auto)
                    .with_accessibility_name(format!("The album {}", a.title))
                    .with_callback(EventFilter::Hover(HoverEventFilter::Click), open(), on_go),
                )
                .with_child(text(
                    &a.year,
                    &format!("font-size: 11px; color: {};", look.muted),
                )),
        );
    for (k, track) in a.tracks.iter().take(count).enumerate() {
        let number = ctx
            .library
            .tracks
            .get(*track)
            .map_or(k + 1, |t| track_number(t.track_no, Rows::Album, k + 1));
        songs.add_child(track_row(
            ctx,
            (first_row + k, *track),
            Rows::Album,
            number,
            (8.0, ALBUM_ROW_H),
        ));
    }
    Dom::create_div()
        .with_css(format!(
            "display: flex; flex-direction: row; align-items: flex-start; flex-shrink: 0; \
             height: {height}px; padding: 0px {PAD_X}px; box-sizing: border-box;"
        ))
        .with_child(
            Dom::create_div()
                .with_css("flex-shrink: 0; cursor: pointer;")
                .with_accessibility_name(format!("The album {}", a.title))
                .with_callback(EventFilter::Hover(HoverEventFilter::Click), open(), on_go)
                .with_child(cover(&page::album_seed(a), &a.title, BLOCK_COVER, false, look)),
        )
        .with_child(songs)
}

/// A row of cards.
fn cards_line(ctx: &Ctx<'_>, kind: CardKind, items: &[usize], height: f32) -> Dom {
    let mut line = Dom::create_div().with_css(format!(
        "display: flex; flex-direction: row; align-items: flex-start; flex-shrink: 0; height: \
         {height}px; padding: 0px {PAD_X}px; box-sizing: border-box;"
    ));
    for (k, item) in items.iter().enumerate() {
        let gap = if k + 1 < items.len() { CARD_GAP } else { 0.0 };
        line.add_child(card(ctx, kind, *item, gap));
    }
    line
}

/// A card: an album's square cover, an artist's round initials or a genre's tile, its name and
/// a line under it; under the pointer a green play button on the cover.
fn card(ctx: &Ctx<'_>, kind: CardKind, item: usize, gap: f32) -> Dom {
    let look = ctx.look;
    let size = ctx.card;
    let catalog = &ctx.s.catalog;
    let (title, under, seed) = match kind {
        CardKind::Album => match catalog.albums.get(item) {
            Some(a) => (a.title.clone(), a.artist.clone(), page::album_seed(a)),
            None => return Dom::create_div(),
        },
        CardKind::Artist => match catalog.artists.get(item) {
            Some((name, songs)) => (name.clone(), page::count(*songs, "song"), name.clone()),
            None => return Dom::create_div(),
        },
        CardKind::Genre => match catalog.genres.get(item) {
            Some((name, songs)) => (name.clone(), page::count(*songs, "song"), name.clone()),
            None => return Dom::create_div(),
        },
    };
    let pick = || {
        RefAny::new(CardPick {
            app: ctx.app.clone(),
            kind,
            index: item,
        })
    };
    let hovered = ctx.s.hover == Hover::Card(kind, item);
    let mut picture = if kind == CardKind::Genre {
        // A genre's tile: its name on its colour.
        let tile_h = (size * GENRE_RATIO).round();
        Dom::create_div()
            .with_css(format!(
                "position: relative; display: flex; flex-direction: column; justify-content: \
                 flex-end; box-sizing: border-box; width: {size}px; height: {tile_h}px; padding: \
                 12px; border-radius: 3px; border: 1px solid rgba(0, 0, 0, 0.55); background: \
                 {}; box-shadow: {};",
                art::background(&seed),
                look.shadow
            ))
            .with_child(text(
                &title,
                "font-size: 17px; font-weight: 700; color: rgba(255, 255, 255, 0.95);",
            ))
    } else {
        cover(&seed, &title, size, kind == CardKind::Artist, look)
    };
    if hovered {
        picture.add_child(cover_play(look, &title, pick()));
    }
    let align = if kind == CardKind::Artist {
        "text-align: center;"
    } else {
        ""
    };
    let mut card = Dom::create_div()
        .with_class(ids::CARD)
        .with_css(format!(
            "display: flex; flex-direction: column; flex-shrink: 0; width: {size}px; \
             margin-right: {gap}px; cursor: pointer;"
        ))
        .with_tab_index(TabIndex::Auto)
        .with_accessibility_name(title.clone())
        .with_callback(EventFilter::Hover(HoverEventFilter::Click), pick(), on_card)
        .with_callback(
            EventFilter::Hover(HoverEventFilter::MouseEnter),
            pick(),
            on_card_enter,
        )
        .with_child(picture);
    if kind != CardKind::Genre {
        card.add_child(text(
            &title,
            &format!(
                "margin-top: 8px; font-size: 12px; font-weight: 700; color: {}; {align}",
                look.text
            ),
        ));
        card.add_child(text(
            &under,
            &format!("margin-top: 2px; font-size: 11px; color: {}; {align}", look.muted),
        ));
    }
    card
}

// ==== The now-playing bar ====

/// The now-playing bar: the transport on the left (2010's), the song, the time played, the seek
/// bar and the length; shuffle, repeat and the queue; mute, the volume and the meter.
fn now_playing_bar(s: &Music, app: &RefAny, look: &Look) -> Dom {
    let heard = s.heard();
    let act = |action: MediaControlsAction| -> Option<(RefAny, CallbackType)> {
        Some((
            RefAny::new(Act {
                app: app.clone(),
                action,
            }),
            on_act as CallbackType,
        ))
    };
    let playing = s.state.playing && !s.state.finished;
    let play = if playing {
        ("pause", "Pause")
    } else {
        ("play_arrow", "Play")
    };
    let transport = Dom::create_div()
        .with_id(ids::CONTROLS)
        .with_css("display: flex; flex-direction: row; align-items: center; flex-shrink: 0;")
        .with_child(round_button(
            look,
            ("skip_previous", "Previous"),
            32.0,
            false,
            ids::PREVIOUS,
            act(MediaControlsAction::Previous),
        ))
        .with_child(round_button(
            look,
            play,
            42.0,
            false,
            ids::PLAY,
            act(MediaControlsAction::PlayPause),
        ))
        .with_child(round_button(
            look,
            ("skip_next", "Next"),
            32.0,
            false,
            ids::NEXT,
            act(MediaControlsAction::Next),
        ));
    let mut song = Dom::create_div().with_css(
        "display: flex; flex-direction: column; justify-content: center; width: 190px; \
         flex-shrink: 1; min-width: 60px; margin-left: 16px;",
    );
    if let Some(t) = &heard {
        song.add_child(text(
            &t.display_title(),
            &format!("font-size: 12px; font-weight: 700; color: {};", look.text),
        )
        .with_id(ids::NOW_TITLE));
        song.add_child(
            text(
                &t.artist,
                &format!(
                    "margin-top: 2px; font-size: 11px; color: {}; cursor: pointer; :hover {{ \
                     text-decoration: underline; }}",
                    look.muted
                ),
            )
            .with_id(ids::NOW_ARTIST)
            .with_callback(
                EventFilter::Hover(HoverEventFilter::Click),
                RefAny::new(GoPick {
                    app: app.clone(),
                    view: View::Artist(t.filed_artist()),
                    remember: true,
                }),
                on_go,
            ),
        );
    }
    let position = s.state.position_s.max(0.0);
    let duration = s.state.duration_s.max(0.0);
    let time_css = format!(
        "flex-shrink: 0; width: 40px; font-size: 11px; color: {};",
        look.muted
    );
    let seek = Dom::create_div()
        .with_css(
            "display: flex; flex-direction: row; align-items: center; flex-grow: 1; min-width: \
             120px; margin: 0px 16px;",
        )
        .with_child(live_text(
            SeekBar::media_time(position).as_str(),
            ids::ELAPSED,
            &format!("{time_css} text-align: right;"),
        ))
        .with_child(
            Dom::create_div()
                .with_css(
                    "display: flex; flex-direction: row; align-items: center; flex-grow: 1; \
                     min-width: 0px;",
                )
                .with_child(
                    SeekBar::create(position, duration)
                        .with_show_times(false)
                        .with_accessibility_name("Position in the song")
                        .with_on_seek(app.clone(), app::on_seek as SeekBarOnSeekCallbackType)
                        .dom()
                        .with_id(ids::SEEK)
                        .with_marker(OptionString::Some(ids::SEEK))
                        .with_css("flex-grow: 1;"),
                ),
        )
        .with_child(live_text(
            SeekBar::media_time(duration).as_str(),
            ids::TOTAL,
            &time_css,
        ));
    let (repeat_icon, repeat_name) = match s.queue.repeat {
        Repeat::Off => ("repeat", "Repeat is off"),
        Repeat::All => ("repeat", "Repeating all"),
        Repeat::One => ("repeat_one", "Repeating this song"),
    };
    let shuffle_name = if s.queue.shuffle {
        "Shuffle is on"
    } else {
        "Shuffle is off"
    };
    let volume_icon = if s.volume <= 0.0 {
        ("volume_off", "Sound on")
    } else if s.volume < 0.5 {
        ("volume_down", "Mute")
    } else {
        ("volume_up", "Mute")
    };
    Dom::create_div()
        .with_id(ids::NOW_PLAYING)
        .with_css(format!(
            "display: flex; flex-direction: row; align-items: center; flex-shrink: 0; height: \
             64px; padding: 0px 14px 0px 8px; box-sizing: border-box; background: {}; \
             border-top: 1px solid {}; color: {};",
            look.bar, look.line, look.text
        ))
        .with_child(transport)
        .with_child(song)
        .with_child(seek)
        .with_child(round_button(
            look,
            ("shuffle", shuffle_name),
            28.0,
            s.queue.shuffle,
            ids::SHUFFLE,
            act(MediaControlsAction::Shuffle),
        ))
        .with_child(round_button(
            look,
            (repeat_icon, repeat_name),
            28.0,
            s.queue.repeat != Repeat::Off,
            ids::REPEAT,
            act(MediaControlsAction::Repeat),
        ))
        .with_child(round_button(
            look,
            ("queue_music", "Play Queue"),
            28.0,
            s.view == View::Queue,
            ids::QUEUE_BUTTON,
            Some((app.clone(), on_queue as CallbackType)),
        ))
        .with_child(Dom::create_div().with_css("width: 14px; flex-shrink: 0;"))
        .with_child(round_button(
            look,
            volume_icon,
            28.0,
            false,
            ids::MUTE,
            Some((app.clone(), on_mute as CallbackType)),
        ))
        .with_child(
            // The slider's track is 200 px whatever its box (the widget's own size).
            Dom::create_div()
                .with_css("display: flex; width: 200px; flex-shrink: 0; margin-left: 8px;")
                .with_child(
                    Slider::create(s.volume, 0.0, 1.0)
                        .with_accessibility_name("Volume")
                        .with_on_value_change(
                            app.clone(),
                            on_volume as SliderOnValueChangeCallbackType,
                        )
                        .dom()
                        .with_id(ids::VOLUME),
                ),
        )
        .with_child(
            Dom::create_div()
                .with_css(
                    "display: flex; flex-direction: row; align-items: center; width: 44px; \
                     flex-shrink: 0; margin-left: 12px;",
                )
                .with_child(
                    LevelMeter::create(0.0)
                        .with_accessibility_name("Level")
                        .dom()
                        .with_id(ids::LEVEL)
                        .with_marker(OptionString::Some(ids::LEVEL))
                        .with_css("flex-grow: 1;"),
                ),
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
            "AzMusic reads MP3, AAC / M4A, ALAC, FLAC, Ogg Vorbis, Opus (Apple), WAV and AIFF \
             where they are; only the library (tags) and the playlists are kept in the data \
             folder.",
        ));
    kit::AppSection {
        category: 0,
        title: String::from("Library"),
        content,
    }
}

// ==== Handlers ====

/// What a sidebar entry, a cover, an artist's name opens; `remember`: Back returns here.
struct GoPick {
    app: RefAny,
    view: View,
    remember: bool,
}

extern "C" fn on_go(mut data: RefAny, _info: CallbackInfo) -> Update {
    let Some((mut app, view, remember)) = data
        .downcast_ref::<GoPick>()
        .map(|p| (p.app.clone(), p.view.clone(), p.remember))
    else {
        return Update::DoNothing;
    };
    let Some(mut s) = app.downcast_mut::<Music>() else {
        return Update::DoNothing;
    };
    // The sidebar leaves the settings page too.
    kit::close_settings(&s.kit);
    app::go(&mut s, view, remember);
    Update::RefreshDom
}

extern "C" fn on_back(mut data: RefAny, _info: CallbackInfo) -> Update {
    let Some(mut s) = data.downcast_mut::<Music>() else {
        return Update::DoNothing;
    };
    if app::go_back(&mut s) {
        Update::RefreshDom
    } else {
        Update::DoNothing
    }
}

extern "C" fn on_forward(mut data: RefAny, _info: CallbackInfo) -> Update {
    let Some(mut s) = data.downcast_mut::<Music>() else {
        return Update::DoNothing;
    };
    if app::go_forward(&mut s) {
        Update::RefreshDom
    } else {
        Update::DoNothing
    }
}

/// The search field: the results page while it holds text, back where it was when emptied.
extern "C" fn on_search(
    mut data: RefAny,
    _info: CallbackInfo,
    state: TextInputState,
) -> OnTextInputReturn {
    let query = state.get_text().as_str().to_string();
    let update = match data.downcast_mut::<Music>() {
        Some(mut s) => {
            s.query = query.clone();
            if query.trim().is_empty() {
                if s.view == View::Search && !app::go_back(&mut s) {
                    app::go(&mut s, View::RecentlyAdded, false);
                }
            } else if s.view != View::Search {
                app::go(&mut s, View::Search, true);
            } else {
                s.selected = None;
                s.hover = Hover::None;
            }
            let found = s.with_library(|library| library.search(&query).len());
            println!("AZMUSIC_SEARCH {found}");
            Update::RefreshDom
        }
        None => Update::DoNothing,
    };
    OnTextInputReturn {
        update,
        valid: TextInputValid::Yes,
    }
}

/// What the pointer is over in the page changed: the page is re-rendered in place (only when
/// it did change - never a loop).
fn set_hover(mut app: RefAny, info: &mut CallbackInfo, hover: Hover) -> Update {
    let changed = match app.downcast_mut::<Music>() {
        Some(mut s) => {
            let changed = s.hover != hover;
            s.hover = hover;
            changed
        }
        None => false,
    };
    if changed {
        app::rerender_page(info);
    }
    Update::DoNothing
}

extern "C" fn on_quiet_enter(data: RefAny, mut info: CallbackInfo) -> Update {
    set_hover(data, &mut info, Hover::None)
}

extern "C" fn on_page_leave(data: RefAny, mut info: CallbackInfo) -> Update {
    set_hover(data, &mut info, Hover::None)
}

/// A song's row: the app, its place in the page's songs, its library index.
struct RowPick {
    app: RefAny,
    row: usize,
    track: usize,
}

fn row_pick(data: &mut RefAny) -> Option<(RefAny, usize, usize)> {
    data.downcast_ref::<RowPick>()
        .map(|p| (p.app.clone(), p.row, p.track))
}

extern "C" fn on_row_enter(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((app, row, _)) = row_pick(&mut data) else {
        return Update::DoNothing;
    };
    set_hover(app, &mut info, Hover::Row(row))
}

/// A click selects the song (Enter plays it).
extern "C" fn on_row_click(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, row, _)) = row_pick(&mut data) else {
        return Update::DoNothing;
    };
    let changed = match app.downcast_mut::<Music>() {
        Some(mut s) => {
            let changed = s.selected != Some(row);
            s.selected = Some(row);
            changed
        }
        None => false,
    };
    if changed {
        app::rerender_page(&mut info);
    }
    Update::DoNothing
}

/// Plays the page from the row (a double-click, the row's play icon).
fn play_row(data: &mut RefAny, info: &mut CallbackInfo) -> Update {
    let Some((mut app, row, _)) = row_pick(data) else {
        return Update::DoNothing;
    };
    let handle = app.clone();
    let Some(mut s) = app.downcast_mut::<Music>() else {
        return Update::DoNothing;
    };
    s.selected = Some(row);
    let tracks = s.page_tracks();
    let plays = s.with_library(|library| plays_page(&s, library, &tracks));
    let heard = s.with_library(|library| s.heard_index(library));
    if plays && heard == tracks.get(row).copied() && !s.state.finished {
        // The song that plays: play / pause it.
        app::transport(&mut s, info, &handle, MediaControlsAction::PlayPause, 0.0);
    } else {
        app::play_page_from(&mut s, info, &handle, row);
    }
    Update::RefreshDom
}

extern "C" fn on_row_double(mut data: RefAny, mut info: CallbackInfo) -> Update {
    play_row(&mut data, &mut info)
}

extern "C" fn on_row_play(mut data: RefAny, mut info: CallbackInfo) -> Update {
    info.stop_propagation();
    play_row(&mut data, &mut info)
}

/// What a song's right-click menu does.
#[derive(Debug, Clone)]
enum RowAction {
    Play,
    PlayNext,
    Enqueue,
    /// Into the playlist with this id.
    AddTo(String),
    /// Into a new playlist.
    AddToNew,
    /// Out of the playlist with this id (the row's entry).
    Remove(String),
    GoToAlbum,
    GoToArtist,
}

/// A menu entry's payload: the app, the song (its row, its id) and what to do.
struct MenuPick {
    app: RefAny,
    row: usize,
    track: String,
    action: RowAction,
}

/// A right click on a song: it is selected and its menu opens - Play, Play Next, Add to Queue,
/// Add to Playlist (a new one or one of the user's), Remove from this Playlist, Go to Album,
/// Go to Artist.
extern "C" fn on_row_menu(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, row, track)) = row_pick(&mut data) else {
        return Update::DoNothing;
    };
    let handle = app.clone();
    let menu = {
        let Some(mut s) = app.downcast_mut::<Music>() else {
            return Update::DoNothing;
        };
        s.selected = Some(row);
        let Some(id) = s.with_library(|library| library.tracks.get(track).map(|t| t.id.clone()))
        else {
            return Update::DoNothing;
        };
        let item = |label: &str, action: RowAction| {
            MenuItem::string(StringMenuItem::create(label).with_callback(
                RefAny::new(MenuPick {
                    app: handle.clone(),
                    row,
                    track: id.clone(),
                    action,
                }),
                on_menu,
            ))
        };
        let mut playlists = vec![item("New Playlist", RowAction::AddToNew)];
        if !s.playlists.is_empty() {
            playlists.push(MenuItem::separator());
        }
        for p in &s.playlists {
            playlists.push(item(p.name.as_str(), RowAction::AddTo(p.id.clone())));
        }
        let mut items = vec![
            item("Play", RowAction::Play),
            item("Play Next", RowAction::PlayNext),
            item("Add to Queue", RowAction::Enqueue),
            MenuItem::separator(),
            MenuItem::string(StringMenuItem::create("Add to Playlist").with_children(playlists)),
        ];
        if let View::Playlist(pid) = &s.view {
            items.push(item("Remove from this Playlist", RowAction::Remove(pid.clone())));
        }
        items.push(MenuItem::separator());
        items.push(item("Go to Album", RowAction::GoToAlbum));
        items.push(item("Go to Artist", RowAction::GoToArtist));
        Menu::create(items)
    };
    app::rerender_page(&mut info);
    let _opened = info.open_menu_for_hit_node(menu);
    Update::DoNothing
}

extern "C" fn on_menu(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, row, track, action)) = data
        .downcast_ref::<MenuPick>()
        .map(|m| (m.app.clone(), m.row, m.track.clone(), m.action.clone()))
    else {
        return Update::DoNothing;
    };
    let handle = app.clone();
    let Some(mut s) = app.downcast_mut::<Music>() else {
        return Update::DoNothing;
    };
    match action {
        RowAction::Play => app::play_page_from(&mut s, &mut info, &handle, row),
        RowAction::PlayNext => app::play_next(&mut s, &mut info, &handle, &track),
        RowAction::Enqueue => app::enqueue(&mut s, &mut info, &handle, &track),
        RowAction::AddTo(id) => {
            app::add_to_playlist(&handle, &mut s, &mut info, Some(id.as_str()), &track);
        }
        RowAction::AddToNew => app::add_to_playlist(&handle, &mut s, &mut info, None, &track),
        RowAction::Remove(id) => app::remove_from_playlist(&handle, &mut s, &mut info, &id, row),
        RowAction::GoToAlbum => {
            if let Some(t) = s.track(&track) {
                let (artist, title) = page::album_key(&t);
                app::go(&mut s, View::Album { artist, title }, true);
            }
        }
        RowAction::GoToArtist => {
            if let Some(t) = s.track(&track) {
                app::go(&mut s, View::Artist(t.filed_artist()), true);
            }
        }
    }
    Update::RefreshDom
}

/// A card: the app, what it shows, which one.
struct CardPick {
    app: RefAny,
    kind: CardKind,
    index: usize,
}

fn card_pick(data: &mut RefAny) -> Option<(RefAny, CardKind, usize)> {
    data.downcast_ref::<CardPick>()
        .map(|p| (p.app.clone(), p.kind, p.index))
}

/// The page a card opens.
fn card_view(s: &Music, kind: CardKind, index: usize) -> Option<View> {
    match kind {
        CardKind::Album => s.catalog.albums.get(index).map(|a| View::Album {
            artist: a.artist.clone(),
            title: a.title.clone(),
        }),
        CardKind::Artist => s
            .catalog
            .artists
            .get(index)
            .map(|(name, _)| View::Artist(name.clone())),
        CardKind::Genre => s
            .catalog
            .genres
            .get(index)
            .map(|(name, _)| View::Genre(name.clone())),
    }
}

/// The songs a card's play button plays.
fn card_tracks(s: &Music, kind: CardKind, index: usize) -> Vec<usize> {
    let catalog = &s.catalog;
    match kind {
        CardKind::Album => catalog
            .albums
            .get(index)
            .map_or_else(Vec::new, |a| a.tracks.clone()),
        CardKind::Artist => catalog.artists.get(index).map_or_else(Vec::new, |(name, _)| {
            catalog.tracks_of(&catalog.albums_of(name))
        }),
        CardKind::Genre => catalog.genres.get(index).map_or_else(Vec::new, |(name, _)| {
            s.with_library(|library| {
                (0..library.tracks.len())
                    .filter(|i| library.tracks[*i].genre.trim() == name)
                    .collect()
            })
        }),
    }
}

extern "C" fn on_card(mut data: RefAny, _info: CallbackInfo) -> Update {
    let Some((mut app, kind, index)) = card_pick(&mut data) else {
        return Update::DoNothing;
    };
    let Some(mut s) = app.downcast_mut::<Music>() else {
        return Update::DoNothing;
    };
    match card_view(&s, kind, index) {
        Some(view) => {
            app::go(&mut s, view, true);
            Update::RefreshDom
        }
        None => Update::DoNothing,
    }
}

extern "C" fn on_card_play(mut data: RefAny, mut info: CallbackInfo) -> Update {
    info.stop_propagation();
    let Some((mut app, kind, index)) = card_pick(&mut data) else {
        return Update::DoNothing;
    };
    let handle = app.clone();
    let Some(mut s) = app.downcast_mut::<Music>() else {
        return Update::DoNothing;
    };
    let tracks = card_tracks(&s, kind, index);
    app::play_tracks(&mut s, &mut info, &handle, &tracks);
    Update::RefreshDom
}

extern "C" fn on_card_enter(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((app, kind, index)) = card_pick(&mut data) else {
        return Update::DoNothing;
    };
    set_hover(app, &mut info, Hover::Card(kind, index))
}

/// The page's Play: pauses / plays on when the page is what plays, plays it from the top
/// otherwise.
extern "C" fn on_page_play(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let handle = data.clone();
    let Some(mut s) = data.downcast_mut::<Music>() else {
        return Update::DoNothing;
    };
    let tracks = s.page_tracks();
    let plays = s.with_library(|library| plays_page(&s, library, &tracks));
    if plays && s.queue.current().is_some() && !s.state.finished {
        app::transport(&mut s, &mut info, &handle, MediaControlsAction::PlayPause, 0.0);
    } else {
        app::play_tracks(&mut s, &mut info, &handle, &tracks);
    }
    Update::RefreshDom
}

extern "C" fn on_page_shuffle(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let handle = data.clone();
    let Some(mut s) = data.downcast_mut::<Music>() else {
        return Update::DoNothing;
    };
    let tracks = s.page_tracks();
    app::shuffle_tracks(&mut s, &mut info, &handle, &tracks);
    Update::RefreshDom
}

/// A click on a column title of the songs page: sorted by it (again: the other way round).
struct SortPick {
    app: RefAny,
    key: SortKey,
}

extern "C" fn on_sort(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, key)) = data
        .downcast_ref::<SortPick>()
        .map(|p| (p.app.clone(), p.key))
    else {
        return Update::DoNothing;
    };
    if let Some(mut s) = app.downcast_mut::<Music>() {
        s.sort = s.sort.toggled(key);
        s.selected = None;
        s.hover = Hover::None;
    }
    app::rerender_page(&mut info);
    Update::DoNothing
}

/// A transport button: the app and what it asks for.
struct Act {
    app: RefAny,
    action: MediaControlsAction,
}

extern "C" fn on_act(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, action)) = data
        .downcast_ref::<Act>()
        .map(|a| (a.app.clone(), a.action))
    else {
        return Update::DoNothing;
    };
    let handle = app.clone();
    let Some(mut s) = app.downcast_mut::<Music>() else {
        return Update::DoNothing;
    };
    app::transport(&mut s, &mut info, &handle, action, 0.0);
    Update::RefreshDom
}

/// The queue button: the queue's page, or back from it.
extern "C" fn on_queue(mut data: RefAny, _info: CallbackInfo) -> Update {
    let Some(mut s) = data.downcast_mut::<Music>() else {
        return Update::DoNothing;
    };
    kit::close_settings(&s.kit);
    if s.view == View::Queue {
        if !app::go_back(&mut s) {
            app::go(&mut s, View::RecentlyAdded, false);
        }
    } else {
        app::go(&mut s, View::Queue, true);
    }
    Update::RefreshDom
}

extern "C" fn on_mute(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let handle = data.clone();
    let Some(mut s) = data.downcast_mut::<Music>() else {
        return Update::DoNothing;
    };
    app::toggle_mute(&mut s, &mut info, &handle);
    Update::RefreshDom
}

/// The volume slider moved: the player follows (the window is not rebuilt while it moves).
extern "C" fn on_volume(mut data: RefAny, mut info: CallbackInfo, state: SliderState) -> Update {
    let handle = data.clone();
    let Some(mut s) = data.downcast_mut::<Music>() else {
        return Update::DoNothing;
    };
    app::transport(
        &mut s,
        &mut info,
        &handle,
        MediaControlsAction::Volume,
        state.value,
    );
    Update::DoNothing
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

    #[test]
    fn an_albums_song_shows_its_track_number_and_a_tables_song_its_place() {
        assert_eq!(track_number(7, Rows::Album, 2), 7);
        assert_eq!(track_number(0, Rows::Album, 2), 2, "no number: its place");
        assert_eq!(track_number(7, Rows::Table, 2), 2);
    }

    #[test]
    fn the_cells_of_a_row_and_of_the_column_titles_share_their_widths() {
        // The column titles line up with the cells because both use the same css.
        assert!(LEAD_CELL.contains("width: 36px"));
        assert!(TIME_CELL.contains("width: 56px"));
        assert!(grow_cell(4).contains("flex-grow: 4"));
    }
}
