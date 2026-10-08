//! The window, in the look of Windows Media Center (colours, light and motion in `look.rs`):
//!
//! - THE MENUS on the deep blue ground and its drifting light: the start strip (the categories
//!   down the window, the focused one on the middle row with its items beside it), a library's
//!   page (its big lower-case title, the views row, the gallery of tiles running to the right),
//!   a group's page, search, now playing (the cover, the song, the seek bar, what comes next);
//!   the Media Center orb and the back button top left, the clock top right, the now-playing
//!   inset bottom left while music plays.
//! - THE PICTURE VIEWER and the slide show, on black: one picture at a time, the next one
//!   cross-fading in with Ken Burns' slow pan and zoom.
//! - THE STAGE: a video full window, the chrome over it (back, the title; the seek bar and the
//!   round transport bottom right), hidden while it plays and the pointer rests. It is in the
//!   window while the video opens - hidden, under the menus - so its first picture is decoded
//!   before anything moves (`curtain.rs`).
//!
//! Every part that moves has an id: the engine knows it again in the next build and slides it
//! from where it was, and a page that comes or goes plays its entrance or its exit.

use azul::{
    callbacks::{SeekBarOnSeekCallbackType, TextInputOnTextInputCallbackType},
    css::Css,
    dom::{AttributeType, TabIndex},
    image::RawImageFormat,
    menu::{Menu, MenuItem, StringMenuItem},
    option::OptionString,
    prelude::*,
    shells::{ShellThemeAccent, ShellThemeScope},
    str::String as AzString,
    video::{VideoConfig, VideoPhase, VideoSource},
    widgets::{SeekBar, TextInput, VideoWidget},
};
use azul_appkit::ui as kit;

use crate::{
    app::{self, cover_key, full_key, thumb_key, Art, Player},
    curtain::{Curtain, Stage},
    gallery::{Grid, TileKind},
    ids,
    library::{self, Shelf, Status},
    look::{self, Light},
    media::{self, Command, CommandRef, Music, VideoSession},
    nav::{self, Act, ActRef},
    options::{self, Category, Row},
    pages::{self, Place, Screen, Section, Tile, Zone},
    settings,
    strip::{Entry, CATEGORIES},
};

// ==== Geometry ====

/// The window's top band: no title row is drawn, the band moves the window (logical px).
pub const BAND_H: f32 = 52.0;
/// A page's big title, under the band.
pub const TITLE_TOP: f32 = 56.0;
/// A line under a page's title (a group's artist, the views row, the search field).
const UNDER_TITLE: f32 = TITLE_TOP + 74.0;
/// A library page's gallery: its distance from the page's left, top and bottom (logical px).
pub const GALLERY_LEFT: f32 = 56.0;
pub const GALLERY_TOP: f32 = 190.0;
pub const GALLERY_BOTTOM: f32 = 56.0;

/// The start strip: a category's row, the focused one's, its items.
const ROW_H: f32 = 48.0;
const FOCUSED_ROW_H: f32 = 150.0;
const NAME_W: f32 = 250.0;
const STRIP_TILE_W: f32 = 150.0;
const STRIP_TILE_H: f32 = 96.0;
const STRIP_GAP: f32 = 14.0;

/// The column every screen fills.
const COLUMN: &str = "display: flex; flex-direction: column; flex-grow: 1; min-height: 0px;";

/// What moves the window (the framework hands a press on it to the window manager, a double
/// click zooms the window), and a control in it, which is its own.
const DRAG: &str = "-azul-app-region: drag;";
const NO_DRAG: &str = "-azul-app-region: no-drag;";

// ==== The window ====

/// The window: the menus, the stage, the picture viewer in the theme scope; the menu bar, the
/// keys, the mouse's back button on the body.
pub extern "C" fn layout(mut data: RefAny, info: LayoutCallbackInfo) -> Dom {
    let _mode = info.get_mode();
    let window = (info.get_window_width(), info.get_window_height());
    let app = data.clone();
    if let Some(mut s) = data.downcast_mut::<Player>() {
        if window.0 > 0.0 && window.1 > 0.0 {
            s.window = window;
        }
    }
    let Some(guard) = data.downcast_ref::<Player>() else {
        return Dom::create_body();
    };
    let s = &*guard;
    // No title row: the Media Center runs edge to edge, its top band moves the window
    // (`corner`). The settings are its own pages, never the desktop's Options dialog.
    let column = Dom::create_div().with_css(COLUMN).with_child(root(s, &app));
    let mut body = ShellThemeScope::create(column)
        .with_accent(ShellThemeAccent::Leaf)
        .body();
    // The menu bar only where it is the system's (macOS's, above every window): in the window
    // it would be a desktop strip over the ten-foot pages - their strip and keys do it all.
    if !s.fullscreen && cfg!(target_os = "macos") {
        body = body.with_menu_bar(menu_bar(s, &app));
    }
    body.with_callback(
        EventFilter::Window(WindowEventFilter::VirtualKeyDown),
        app.clone(),
        nav::on_key,
    )
    .with_callback(
        EventFilter::Hover(HoverEventFilter::BackMouseUp),
        RefAny::new(CommandRef {
            app: app.clone(),
            command: Command::Back,
        }),
        media::on_command,
    )
    .with_callback(
        EventFilter::Hover(HoverEventFilter::MouseMove),
        app,
        nav::on_pointer,
    )
}

/// The root: black; the stage (a video) under the menus while it opens, alone once it plays.
fn root(s: &Player, app: &RefAny) -> Dom {
    let stage = s
        .video
        .as_ref()
        .map_or(Curtain::Closed.stage(), |v| v.curtain.stage());
    let mut root = Dom::create_div()
        .with_id(ids::ROOT)
        .with_css(
            "position: relative; display: block; flex-grow: 1; min-height: 0px; overflow: \
             hidden; background: #000000; color: #ffffff;",
        )
        .with_component_css(Css::from_string(look::KEYFRAMES));
    if let Some(video) = s.video.as_ref() {
        root.add_child(video_stage(s, video, app, stage));
    }
    if stage.menus_mounted {
        root.add_child(menus(s, app, stage));
    }
    root
}

/// The menus: the ground, the page, the corner pieces, the inset, a notice.
fn menus(s: &Player, app: &RefAny, stage: Stage) -> Dom {
    let place = s.menus_place();
    // Before a video the menus go to black as one with the ground (what is not text or an icon -
    // a tile's face, a panel - goes with the blue).
    let mut menus = Dom::create_div()
        .with_id(ids::MENUS)
        .with_css(format!("{} {}", look::FILL, look::ground_fade(stage)));
    if place.screen.on_ground() {
        menus.add_child(ground(s, place));
    }
    menus.add_child(page(s, app, place, stage));
    menus.add_child(corner(s, app, place, stage));
    if place.screen != Screen::NowPlaying && place.screen != Screen::Picture {
        if let Some(inset) = now_playing_inset(s, app, stage) {
            menus.add_child(inset);
        }
    }
    if let Some(note) = opening_note(s, stage) {
        menus.add_child(note);
    }
    if let Some((text, _)) = s.notice.as_ref() {
        menus.add_child(notice(text, stage));
    }
    menus
}

/// The light of a page: each page, and each category of the strip, has its own.
fn light_seed(s: &Player, place: &Place) -> usize {
    match &place.screen {
        Screen::Start => s.strip.row,
        Screen::Section(section) => 7 + section.index(),
        Screen::Group { section, .. } => 13 + section.index(),
        Screen::Search => 21,
        Screen::Address => 22,
        Screen::NowPlaying => 23,
        Screen::Settings => 24,
        Screen::SettingsPage(category) => 25 + category.index(),
        Screen::Picture | Screen::Video => 0,
    }
}

/// The ground: Media Center's blue, flora's shafts drifting slowly over it, the bloom off the
/// upper-left corner, the far corner falling away. Fades to black LAST before a video (with
/// the menus it is in).
fn ground(s: &Player, place: &Place) -> Dom {
    let light = Light::for_seed(light_seed(s, place));
    Dom::create_div()
        .with_id(ids::GROUND)
        .with_css(format!("{} {}", look::FILL, look::GROUND_BASE))
        .with_child(Dom::create_div().with_id(ids::RAYS_FAR).with_css(format!(
            "position: absolute; left: -14%; top: -16%; width: 128%; height: 132%; {} \
             opacity: {:.2}; transform: translate({:.1}px, 0px); {}",
            look::RAYS_FAR,
            light.far_opacity,
            light.far_x,
            look::RAYS_MOTION
        )))
        .with_child(Dom::create_div().with_id(ids::RAYS_NEAR).with_css(format!(
            "position: absolute; left: -12%; top: -14%; width: 124%; height: 128%; {} \
             opacity: {:.2}; transform: translate({:.1}px, {:.1}px); {}",
            look::RAYS_NEAR,
            light.near_opacity,
            light.near_x,
            light.near_y,
            look::RAYS_MOTION
        )))
        .with_child(
            Dom::create_div()
                .with_id(ids::BLOOM)
                .with_css(format!("{} {}", look::FILL, look::GROUND_BLOOM)),
        )
        .with_child(
            Dom::create_div()
                .with_id(ids::FALLOFF)
                .with_css(format!("{} {}", look::FILL, look::GROUND_FALLOFF)),
        )
}

/// The page shown: its own id (the engine's entrance and exit go by it), its motion.
fn page(s: &Player, app: &RefAny, place: &Place, stage: Stage) -> Dom {
    // No exit while a video's curtain is down: the black a page left would cover the picture.
    let exits = s.video.is_none();
    let content = match &place.screen {
        Screen::Start => start_page(s, app, stage),
        Screen::Section(section) => section_page(s, app, place, *section, stage),
        Screen::Group { section, group, .. } => {
            let title = group.title.to_lowercase();
            let sub = format!("{} · {}", section.title(), group.subtitle);
            gallery_page(s, app, place, &title, Some(&sub), stage)
        }
        Screen::Search => search_page(s, app, place, stage),
        Screen::Address => address_page(s, app, stage),
        Screen::NowPlaying => now_playing_page(s, app, stage),
        Screen::Picture => picture_page(s, app),
        Screen::Video => Dom::create_div(),
        Screen::Settings => settings_list_page(s, app, place, stage),
        Screen::SettingsPage(category) => settings_page(s, app, place, *category, stage),
    };
    content
        .with_id(ids::id(&place.screen.key()))
        .with_css(format!("{} {}", look::FILL, look::page_motion(exits)))
}

/// A line of text in a block of its own, the text node marked `marker` (rewritten in place).
fn live_text(text: String, marker: AzString, css: &str) -> Dom {
    Dom::create_div().with_css(css).with_child(
        Dom::create_text_do_not_use_without_block_level_wrapper(text)
            .with_marker(OptionString::Some(marker)),
    )
}

/// A paragraph of text in `css`, with the curtain's text fade.
fn text(content: &str, css: &str, stage: Stage) -> Dom {
    Dom::create_p_with_text(content).with_css(format!(
        "margin: 0px; {css} {}",
        look::text_fade(stage)
    ))
}

/// A page's big lower-case title (Media Center's "music"), under the top band.
fn page_title(title: &str, stage: Stage) -> Dom {
    text(
        title,
        &format!(
            "position: absolute; left: 56px; top: {TITLE_TOP:.0}px; {} color: {};",
            look::PAGE_TITLE,
            look::INK
        ),
        stage,
    )
}

/// An icon in `css`, with the curtain's icon fade.
fn icon(name: &str, css: &str, stage: Stage) -> Dom {
    Dom::create_icon(name).with_css(format!("{css} {}", look::icon_fade(stage)))
}

/// A part that does `act` when clicked and takes the focus under the pointer (and on Tab).
fn act_part(dom: Dom, app: &RefAny, act: Act, name: &str) -> Dom {
    let payload = |act: &Act| {
        RefAny::new(ActRef {
            app: app.clone(),
            act: act.clone(),
        })
    };
    dom.with_tab_index(TabIndex::Auto)
        .with_accessibility_name(name)
        .with_callback(
            EventFilter::Hover(HoverEventFilter::Click),
            payload(&act),
            nav::on_act,
        )
        .with_callback(
            EventFilter::Hover(HoverEventFilter::MouseEnter),
            payload(&act),
            nav::on_hover,
        )
        .with_callback(
            EventFilter::Focus(FocusEventFilter::FocusReceived),
            payload(&act),
            nav::on_focus,
        )
}

// ==== The top band: back, the clock; it moves the window ====

/// The room the window's own controls take in the top band (logical px, left and right):
/// macOS's traffic lights before the back button (`TabsInTitlebar::platform()`, the room the
/// ribbon apps' tabs leave them), the software controls' after the clock on Linux; none in
/// fullscreen.
fn window_controls(s: &Player) -> (f32, f32) {
    if s.fullscreen {
        return (0.0, 0.0);
    }
    let chrome = kit::tabs_in_titlebar();
    (chrome.left.max(0.0), chrome.right.max(0.0))
}

/// The window's top band - no title row is drawn, the Media Center runs edge to edge: the band
/// moves the window (`-azul-app-region: drag`; a double click on it zooms the window), with the
/// back button on its left - right of the traffic lights on macOS; it shows when the pointer
/// moves and hides when it rests (in place) - and the clock on its right, each its own
/// (`no-drag`).
fn corner(s: &Player, app: &RefAny, place: &Place, stage: Stage) -> Dom {
    let (lights, controls) = window_controls(s);
    let mut left = Dom::create_div().with_css(format!(
        "position: absolute; left: {:.0}px; top: 9px; display: flex; flex-direction: row; \
         align-items: center;",
        18.0 + lights
    ));
    if place.screen != Screen::Start {
        // Lit while Tab has the keyboard on it (and then it stays: `Player::chrome_held`).
        let lit = s.zone() == Zone::Corner && s.place().focus.corner == 0;
        left.add_child(
            Dom::create_div()
                .with_id(ids::CORNER)
                .with_marker(OptionString::Some(ids::CORNER))
                .with_css(format!(
                    "display: flex; flex-direction: row; opacity: {}; animation: opacity 300ms \
                     ease-out;",
                    if s.controls_shown { 1 } else { 0 }
                ))
                .with_child(round_button(
                    app,
                    Command::Back,
                    ("arrow_back", "Back"),
                    34.0,
                    ids::BACK,
                    stage,
                    lit,
                )),
        );
    }
    // `cursor: default`: what gives the band its place in the hit test (a press on it must
    // land on it, not on the page under it).
    let mut band = Dom::create_div()
        .with_id(ids::BAND)
        .with_css(format!(
            "position: absolute; left: 0px; top: 0px; right: 0px; height: {BAND_H}px; cursor: \
             default; {DRAG}"
        ))
        .with_child(left);
    // The clock, unless the settings took it away (it fades in and out).
    if s.options.is_on(options::SHOW_CLOCK) {
        band.add_child(
            live_text(
                s.clock_text.clone(),
                ids::CLOCK_TEXT,
                &format!(
                    "position: absolute; right: {:.0}px; top: 13px; font-size: 24px; \
                     font-weight: 300; color: {}; {NO_DRAG} {} -azul-animation-in: azp-fade-in \
                     300ms ease-out; -azul-animation-out: azp-fade-out 300ms ease-in;",
                    30.0 + controls,
                    look::INK,
                    look::text_fade(stage)
                ),
            )
            .with_id(ids::CLOCK),
        );
    }
    band
}

/// A round glass button doing `command`: an icon, named for assistive technology (`face`: the
/// icon and the name); the glow under the pointer, and the keyboard's ring while `lit` (Tab
/// brought the keys to it: the app's own focus, which fades in and out - never the engine's
/// ring).
fn round_button(
    app: &RefAny,
    command: Command,
    face: (&str, &str),
    size: f32,
    id: AzString,
    stage: Stage,
    lit: bool,
) -> Dom {
    let (icon_name, name) = face;
    let icon_px = (size * 0.5).round();
    let radius = size / 2.0;
    // The box takes the click and the name; its icon is the face, the ring is over it.
    Dom::create_div()
        .with_id(id)
        .with_css(format!(
            "position: relative; display: flex; flex-shrink: 0; width: {size}px; height: \
             {size}px; margin-left: 8px; cursor: pointer; border-radius: {radius}px; {NO_DRAG}"
        ))
        .with_tab_index(TabIndex::Auto)
        .with_accessibility_name(name)
        .with_callback(
            EventFilter::Hover(HoverEventFilter::Click),
            RefAny::new(CommandRef {
                app: app.clone(),
                command,
            }),
            media::on_command,
        )
        .with_child(Dom::create_icon(icon_name).with_css(format!(
            "{} width: {size}px; height: {size}px; border-radius: {radius}px; font-size: \
             {icon_px}px; {}",
            look::ROUND,
            look::icon_fade(stage)
        )))
        .with_child(Dom::create_div().with_css(look::ring(lit, size, stage)))
}

/// The music playing, small, bottom left (a click: now playing).
fn now_playing_inset(s: &Player, app: &RefAny, stage: Stage) -> Option<Dom> {
    let music = s.music.as_ref()?;
    let song = music.current_item(s);
    let title = song.map_or_else(
        || {
            music
                .current()
                .map(|p| library::file_title(std::path::Path::new(p)))
                .unwrap_or_default()
        },
        |i| i.title.clone(),
    );
    let artist = song.map(library::Item::filed_artist).unwrap_or_default();
    let art = song
        .filter(|i| i.has_cover)
        .and_then(|i| s.art.get(&cover_key(&i.path)).cloned().flatten());
    let face = art_face(art, &title, 56.0, 56.0, "music_note", stage);
    // In on a spring, out with a fade (Stop dismisses it); lit while Tab has the keys on it.
    let lit = s.zone() == Zone::Inset;
    let inset = Dom::create_div()
        .with_id(ids::INSET)
        .with_css(
            "position: absolute; left: 24px; bottom: 22px; width: 300px; height: 64px; display: \
             flex; flex-direction: row; align-items: center; padding: 4px; box-sizing: \
             border-box; border-radius: 6px; background: rgba(4, 18, 43, 0.55); border: 1px \
             solid rgba(255, 255, 255, 0.18); cursor: pointer; -azul-animation-in: azp-rise-in \
             360ms spring; -azul-animation-out: azp-fade-out 240ms ease-in; :hover { border: \
             1px solid rgba(255, 255, 255, 0.7); }",
        )
        .with_child(Dom::create_div().with_css(look::bar(lit, 6.0)))
        .with_child(
            Dom::create_div()
                .with_css(
                    "position: relative; width: 56px; height: 56px; flex-shrink: 0; overflow: \
                     hidden; border-radius: 3px;",
                )
                .with_child(face),
        )
        .with_child(
            Dom::create_div()
                .with_css(
                    "display: flex; flex-direction: column; margin-left: 10px; min-width: 0px; \
                     flex-grow: 1;",
                )
                .with_child(text(
                    if music.playing() { "now playing" } else { "paused" },
                    &format!("font-size: 11px; color: {};", look::INK_FAINT),
                    stage,
                ))
                .with_child(text(
                    &title,
                    "font-size: 15px; white-space: nowrap; overflow: hidden;",
                    stage,
                ))
                .with_child(text(
                    &artist,
                    &format!(
                        "font-size: 12px; color: {}; white-space: nowrap; overflow: hidden;",
                        look::INK_DIM
                    ),
                    stage,
                )),
        );
    Some(act_part(inset, app, Act::NowPlaying, "Now playing"))
}

/// While a video opens behind the menus: its name and "opening", bottom left.
fn opening_note(s: &Player, stage: Stage) -> Option<Dom> {
    let video = s.video.as_ref()?;
    if !video.curtain.prerolling() {
        return None;
    }
    Some(
        Dom::create_div()
            .with_id(ids::NOTE)
            .with_css(
                "position: absolute; left: 0px; right: 0px; bottom: 26px; display: flex; \
                 justify-content: center; -azul-animation-in: azp-fade-in 400ms ease-out;",
            )
            .with_child(text(
                &format!("opening {}\u{2026}", video.title()),
                &format!("font-size: 18px; color: {};", look::INK_DIM),
                stage,
            )),
    )
}

/// A sentence for a moment, over the bottom of the page.
fn notice(content: &str, stage: Stage) -> Dom {
    Dom::create_div()
        .with_css(
            "position: absolute; left: 0px; right: 0px; bottom: 70px; display: flex; \
             justify-content: center; -azul-animation-in: azp-rise-in 300ms spring; \
             -azul-animation-out: azp-fade-out 300ms ease-in;",
        )
        .with_child(text(
            content,
            "padding: 10px 20px; border-radius: 6px; font-size: 16px; background: rgba(4, 18, \
             43, 0.86); border: 1px solid rgba(255, 255, 255, 0.35);",
            stage,
        ))
}

// ==== The start strip ====

/// Whether `entry` can do anything here, and if not why.
fn entry_reason(s: &Player, entry: &Entry) -> Option<String> {
    entry
        .never
        .map(str::to_string)
        .or_else(|| nav::missing(s, entry.action))
}

/// The start strip: the categories down the window, the focused one on the middle row (its name
/// big, its items beside it); the column slides to keep the focused category in the middle.
/// How long the strip glides to a new focus (a spring: it settles with its speed when the
/// next arrow comes before it has).
const STRIP_GLIDE_MS: u32 = 420;

fn start_page(s: &Player, app: &RefAny, stage: Stage) -> Dom {
    let height = s.window.1.max(200.0);
    let width = s.window.0.max(320.0);
    let focused = s.strip.row;
    // The focused category's middle at 52 % of the height.
    #[allow(clippy::cast_precision_loss)]
    let top = height * 0.52 - focused as f32 * ROW_H - FOCUSED_ROW_H / 2.0;
    let left = (width * 0.04).round();
    let items_left = left + NAME_W + 24.0;
    // The strip glides to the focused category (Media Center's, Kodi's): the column, each
    // row and the item row declare their moves - nothing slides undeclared.
    let mut column = Dom::create_div().with_id(ids::STRIP).with_css(format!(
        "position: absolute; left: 0px; right: 0px; top: {top:.1}px; display: flex; \
         flex-direction: column; animation: move {STRIP_GLIDE_MS}ms spring;"
    ));
    for (row, category) in CATEGORIES.iter().enumerate() {
        let is_focused = row == focused;
        let h = if is_focused { FOCUSED_ROW_H } else { ROW_H };
        let name = Dom::create_p_with_text(category.name)
            .with_css(format!(
                "position: absolute; left: {left}px; width: {NAME_W}px; top: 0px; height: {h}px; \
                 margin: 0px; display: flex; align-items: center; justify-content: flex-end; \
                 font-size: {}px; font-weight: 300; color: {}; white-space: nowrap; cursor: \
                 pointer; {} animation: opacity {}ms ease-in, color 220ms ease-out, font-size \
                 260ms ease-out; :hover {{ color: #ffffff; }}",
                if is_focused { 38 } else { 26 },
                if is_focused { look::INK } else { look::INK_FAINT },
                look::text_fade(stage),
                crate::curtain::TEXT_FADE_MS
            ))
            .with_callback(
                EventFilter::Hover(HoverEventFilter::Click),
                RefAny::new(ActRef {
                    app: app.clone(),
                    act: Act::Category(row),
                }),
                nav::on_act,
            );
        let mut row_dom = Dom::create_div()
            .with_id(ids::id(&format!("row-{row}")))
            .with_css(format!(
                "position: relative; height: {h}px; flex-shrink: 0; animation: move \
                 {STRIP_GLIDE_MS}ms spring, height 260ms ease-out;"
            ))
            .with_child(name);
        if is_focused {
            row_dom.add_child(strip_items(s, app, row, items_left, width, stage));
        }
        column.add_child(row_dom);
    }
    Dom::create_div()
        .with_child(column)
        .with_callback(
            EventFilter::Hover(HoverEventFilter::Scroll),
            app.clone(),
            nav::on_wheel,
        )
}

/// The focused category's items, side by side; they slide in one after another when the
/// category comes to the middle, and the row slides left to keep the focused one in view.
fn strip_items(
    s: &Player,
    app: &RefAny,
    row: usize,
    items_left: f32,
    width: f32,
    stage: Stage,
) -> Dom {
    let category = &CATEGORIES[row];
    let col = s.strip.col();
    let step = STRIP_TILE_W + STRIP_GAP;
    #[allow(clippy::cast_precision_loss)]
    let focused_right = items_left + (col + 1) as f32 * step;
    let shift = (focused_right - (width - 48.0)).max(0.0).round();
    let mut items = Dom::create_div()
        .with_id(ids::id(&format!("items-{row}")))
        .with_css(format!(
            "position: absolute; left: {:.1}px; top: 14px; height: {}px; display: flex; \
             flex-direction: row; animation: move {STRIP_GLIDE_MS}ms spring;",
            items_left - shift,
            STRIP_TILE_H + 36.0
        ));
    for (i, entry) in category.entries.iter().enumerate() {
        let focused = i == col && s.zone() == Zone::Content;
        let reason = entry_reason(s, entry);
        let enabled = reason.is_none();
        let face_bg = if enabled {
            "background: linear-gradient(to bottom, rgba(255, 255, 255, 0.16), rgba(255, 255, \
             255, 0.04)); border: 1px solid rgba(255, 255, 255, 0.22);"
        } else {
            "background: rgba(255, 255, 255, 0.04); border: 1px solid rgba(255, 255, 255, 0.1);"
        };
        let mut tile = Dom::create_div()
            .with_id(ids::id(&format!("item-{row}-{i}")))
            .with_class(ids::TILE)
            .with_css(format!(
                "position: relative; width: {STRIP_TILE_W}px; height: {}px; margin-right: \
                 {STRIP_GAP}px; flex-shrink: 0; cursor: pointer; {}",
                STRIP_TILE_H + 36.0,
                look::tile_motion(i)
            ))
            .with_child(
                Dom::create_div()
                    .with_css(format!(
                        "position: absolute; left: 0px; top: 0px; width: {STRIP_TILE_W}px; \
                         height: {STRIP_TILE_H}px; box-sizing: border-box; border-radius: 4px; \
                         display: flex; align-items: center; justify-content: center; {face_bg} \
                         {}",
                        look::face(focused, look::FOCUS_SCALE, stage)
                    ))
                    .with_child(icon(
                        entry.icon,
                        &format!(
                            "font-size: 46px; color: {};",
                            if enabled {
                                "#ffffff"
                            } else {
                                "rgba(255, 255, 255, 0.35)"
                            }
                        ),
                        stage,
                    )),
            )
            .with_child(Dom::create_div().with_css(look::glow(
                focused,
                look::FOCUS_SCALE,
                STRIP_TILE_W,
                STRIP_TILE_H,
                4.0,
                stage,
            )))
            .with_child(text(
                entry.label,
                &format!(
                    "position: absolute; left: 0px; top: {}px; width: {STRIP_TILE_W}px; \
                     text-align: center; font-size: 16px; color: {}; white-space: nowrap;",
                    STRIP_TILE_H + 8.0,
                    if focused && enabled {
                        look::INK
                    } else if enabled {
                        look::INK_DIM
                    } else {
                        look::INK_FAINT
                    }
                ),
                stage,
            ));
        if focused {
            if let Some(why) = reason.as_ref() {
                tile.add_child(text(
                    why,
                    &format!(
                        "position: absolute; left: 0px; top: {}px; width: 260px; font-size: \
                         12px; color: {};",
                        STRIP_TILE_H + 30.0,
                        look::INK_FAINT
                    ),
                    stage,
                ));
            }
        }
        let name = match reason.as_ref() {
            Some(why) => format!("{} (not available: {why})", entry.label),
            None => entry.label.to_string(),
        };
        items.add_child(act_part(tile, app, Act::Strip(row, i), &name));
    }
    items
}

// ==== A library's page, a group's page, search ====

/// A library's page: its title, the views row, the gallery.
fn section_page(s: &Player, app: &RefAny, place: &Place, section: Section, stage: Stage) -> Dom {
    let mut page = gallery_page(s, app, place, section.title(), None, stage);
    page.add_child(views_row(s, app, place, section, stage));
    page
}

/// The views row: albums · artists · genres · songs (the view shown bright, the focus's glow
/// when the row has the keyboard).
fn views_row(s: &Player, app: &RefAny, place: &Place, section: Section, stage: Stage) -> Dom {
    let _ = s;
    let mut row = Dom::create_div().with_id(ids::VIEWS).with_css(format!(
        "position: absolute; left: 60px; top: {:.0}px; display: flex; flex-direction: row; \
         align-items: center;",
        UNDER_TITLE - 2.0
    ));
    for (i, view) in section.views().iter().enumerate() {
        let shown = i == place.focus.view;
        let focused = shown && place.focus.on_views && s.zone() == Zone::Content;
        let word = Dom::create_p_with_text(view.label(section)).with_css(format!(
            "margin: 0px 26px 0px 0px; padding: 2px 8px; border-radius: 4px; font-size: 21px; \
             cursor: pointer; color: {}; {} {} :hover {{ color: #ffffff; }}",
            if shown { look::INK } else { look::INK_FAINT },
            if focused {
                "box-shadow: 0px 0px 14px 2px rgba(118, 196, 255, 0.8); background: rgba(118, \
                 196, 255, 0.18);"
            } else {
                ""
            },
            look::text_fade(stage)
        ));
        row.add_child(act_part(
            word.with_id(ids::id(&format!("view-{i}"))),
            app,
            Act::View(i),
            view.label(section),
        ));
    }
    row
}

/// A page with a gallery: the big title (and a line under it), the tiles in view, the status
/// line (how many, the scan, why it is empty).
fn gallery_page(
    s: &Player,
    app: &RefAny,
    place: &Place,
    title: &str,
    subtitle: Option<&str>,
    stage: Stage,
) -> Dom {
    let tiles = s.page_tiles(place);
    let grid = s.grid(&tiles);
    let mut page = Dom::create_div()
        .with_child(page_title(title, stage))
        .with_callback(
            EventFilter::Hover(HoverEventFilter::Scroll),
            app.clone(),
            nav::on_wheel,
        );
    if let Some(sub) = subtitle {
        page.add_child(text(
            sub,
            &format!(
                "position: absolute; left: 62px; top: {UNDER_TITLE:.0}px; font-size: 18px; \
                 color: {};",
                look::INK_DIM
            ),
            stage,
        ));
    }
    let offset = grid.offset(place.focus.first_col);
    // The sheet glides a column at a time as the focus nears the edge (declared: a move on
    // the strip's spring), the tiles on it ride along.
    let mut sheet = Dom::create_div().with_id(ids::SHEET).with_css(format!(
        "position: absolute; left: {:.1}px; top: 0px; right: 0px; bottom: 0px; animation: move \
         {STRIP_GLIDE_MS}ms spring;",
        -offset
    ));
    let content_lit = s.zone() == Zone::Content;
    for i in grid.built(place.focus.first_col, tiles.len()) {
        let focused = !place.focus.on_views && i == place.focus.index && content_lit;
        sheet.add_child(gallery_tile(s, app, &tiles[i], i, focused, &grid, stage));
    }
    page.add_child(
        Dom::create_div()
            .with_id(ids::GALLERY)
            .with_css(format!(
                "position: absolute; left: {GALLERY_LEFT}px; right: 0px; top: {GALLERY_TOP}px; \
                 bottom: {GALLERY_BOTTOM}px; overflow: hidden;"
            ))
            .with_child(sheet),
    );
    page.add_child(
        text(
            &status_line(s, place, tiles.len()),
            &format!(
                "position: absolute; left: 62px; bottom: 20px; font-size: 14px; color: {};",
                look::INK_FAINT
            ),
            stage,
        )
        .with_id(ids::STATUS),
    );
    if let Some(empty) = empty_sentence(s, place, tiles.len()) {
        page.add_child(
            text(
                &empty,
                &format!(
                    "position: absolute; left: 62px; top: {}px; right: 60px; font-size: 22px; \
                     font-weight: 300; color: {};",
                    GALLERY_TOP + 8.0,
                    look::INK_DIM
                ),
                stage,
            )
            .with_id(ids::EMPTY),
        );
    }
    page
}

/// The library a page shows, if it shows one.
fn page_shelf(place: &Place) -> Option<Shelf> {
    match &place.screen {
        Screen::Section(section) => section.shelf(),
        Screen::Group { shelf, .. } => Some(*shelf),
        _ => None,
    }
}

/// The status line: how many, and whether the folder is being looked through.
fn status_line(s: &Player, place: &Place, count: usize) -> String {
    let what = match &place.screen {
        Screen::Section(Section::Music) => {
            let view = Section::Music.views()[place.focus.view.min(3)];
            match view {
                pages::View::Artists => library::count(count, "artist", "artists"),
                pages::View::Genres => library::count(count, "genre", "genres"),
                pages::View::Songs => library::count(count, "song", "songs"),
                _ => library::count(count, "album", "albums"),
            }
        }
        Screen::Section(Section::Recent) => {
            library::count(count.saturating_sub(1), "file played", "files played")
        }
        Screen::Search if s.query.trim().is_empty() => String::new(),
        Screen::Search => library::count(count, "found", "found"),
        _ => library::count(count, "item", "items"),
    };
    let Some(shelf) = page_shelf(place) else {
        return what;
    };
    let shelved = s.library.shelf(shelf);
    let mut line = what;
    if shelved.status == Status::Scanning {
        line.push_str(" \u{b7} looking through the folder\u{2026}");
    }
    if shelved.cut {
        line.push_str(&format!(
            " \u{b7} the first {} files of the folder",
            library::MAX_FILES
        ));
    }
    line
}

/// Why a gallery is empty (honest: the folder, what it takes).
fn empty_sentence(s: &Player, place: &Place, count: usize) -> Option<String> {
    if count > 0 {
        return None;
    }
    if place.screen == Screen::Search {
        return Some(if s.query.trim().is_empty() {
            String::from("Type to search the music, the pictures and the videos.")
        } else {
            format!("Nothing found for \u{201c}{}\u{201d}.", s.query.trim())
        });
    }
    let shelf = page_shelf(place)?;
    let shelved = s.library.shelf(shelf);
    let folder = s.folders.shown(shelf);
    let kind = match (&place.screen, shelf) {
        (Screen::Section(Section::Movies), _) => "movies (videos of 40 minutes or more)",
        (_, Shelf::Music) => "music",
        (_, Shelf::Pictures) => "pictures",
        (_, Shelf::Videos) => "videos",
        (_, Shelf::Tv) => "recorded TV",
    };
    Some(match shelved.status {
        Status::Scanning | Status::Unknown => format!("Looking for {kind} in {folder}\u{2026}"),
        Status::Missing => format!(
            "AzPlayer reads {kind} from {folder}, which is not there. Add another folder in \
             settings, library setup."
        ),
        Status::Ready => match shelf {
            Shelf::Music => format!(
                "There is no music in {folder} yet. Songs saved there (MP3, AAC, FLAC, Ogg, \
                 WAV) show here."
            ),
            Shelf::Pictures => format!(
                "There are no pictures in {folder} yet. Photos saved there (JPEG, PNG, GIF, \
                 WebP) show here."
            ),
            Shelf::Videos => format!(
                "There are no {kind} in {folder} yet. MP4 and MOV videos (H.264) saved there \
                 show here."
            ),
            Shelf::Tv => format!(
                "There is no recorded TV. Recordings saved as MP4 in {folder} show here."
            ),
        },
    })
}

/// What a tile shows: its title, the line under it, its picture (when one was made), the icon
/// it has without one.
fn describe(s: &Player, tile: &Tile) -> (String, String, Art, &'static str) {
    match tile {
        Tile::Group { shelf, group } => {
            let art = app::tile_art(s, tile).and_then(|job| s.art.get(&job.key).cloned().flatten());
            let icon_name = match shelf {
                Shelf::Music => "album",
                Shelf::Pictures => "photo_library",
                _ => "video_library",
            };
            (group.title.clone(), group.subtitle.clone(), art, icon_name)
        }
        Tile::Item { shelf, index } => {
            let Some(item) = s.items(*shelf).get(*index) else {
                return (String::new(), String::new(), None, "help");
            };
            match shelf {
                Shelf::Music => {
                    let mut line = item.filed_artist();
                    if item.duration_s > 0.0 {
                        line.push_str(" \u{b7} ");
                        line.push_str(SeekBar::media_time(item.duration_s).as_str());
                    }
                    (item.title.clone(), line, None, "music_note")
                }
                Shelf::Pictures => {
                    let art = s.art.get(&thumb_key(&item.path)).cloned().flatten();
                    (item.title.clone(), item.folder.clone(), art, "photo")
                }
                Shelf::Videos | Shelf::Tv => {
                    let line = if item.duration_s > 0.0 {
                        SeekBar::media_time(item.duration_s).as_str().to_string()
                    } else {
                        item.folder.clone()
                    };
                    (item.title.clone(), line, None, "movie")
                }
            }
        }
        Tile::Recent(i) => {
            let Some(entry) = s.history.entries.get(*i) else {
                return (String::new(), String::new(), None, "history");
            };
            let resume = s.history.resume_at(&entry.path);
            let line = if resume > 0.0 {
                format!("resume at {}", SeekBar::media_time(resume).as_str())
            } else {
                String::from("play from the start")
            };
            (entry.title(), line, None, "movie")
        }
        Tile::OpenFile => (
            String::from("open a file"),
            String::from("MP4 or MOV"),
            None,
            "folder_open",
        ),
    }
}

/// A tile's face: its picture filling it (cut to fill, the middle kept), or an icon and the
/// initials on a colour of its own.
fn art_face(art: Art, title: &str, w: f32, h: f32, icon_name: &str, stage: Stage) -> Dom {
    match art {
        Some((image, iw, ih)) if iw > 0.0 && ih > 0.0 => {
            let scale = (w / iw).max(h / ih);
            let (dw, dh) = (iw * scale, ih * scale);
            Dom::create_image(image).with_css(format!(
                "position: absolute; left: {:.1}px; top: {:.1}px; width: {dw:.1}px; height: \
                 {dh:.1}px; {}",
                (w - dw) / 2.0,
                (h - dh) / 2.0,
                look::icon_fade(stage)
            ))
        }
        _ => Dom::create_div()
            .with_css(format!(
                "position: absolute; left: 0px; top: 0px; width: {w}px; height: {h}px; \
                 background: {}; display: flex; flex-direction: column; align-items: center; \
                 justify-content: center;",
                look::tile_colour(title)
            ))
            .with_child(icon(
                icon_name,
                &format!(
                    "font-size: {:.0}px; color: rgba(255, 255, 255, 0.85);",
                    (h * 0.36).clamp(18.0, 54.0)
                ),
                stage,
            )),
    }
}

/// One tile of a gallery, where the grid puts it: the face, the glow over it when focused, the
/// caption under it.
fn gallery_tile(
    s: &Player,
    app: &RefAny,
    tile: &Tile,
    index: usize,
    focused: bool,
    grid: &Grid,
    stage: Stage,
) -> Dom {
    let (x, y) = grid.position(index);
    let (aw, ah, caption_h) = grid.kind.art();
    let (title, subtitle, art, icon_name) = describe(s, tile);
    let key = tile.key(&s.library, &s.history);
    let id = if *tile == Tile::OpenFile {
        ids::OPEN
    } else {
        ids::id(&format!("tile-{key}"))
    };
    let mut dom = Dom::create_div()
        .with_id(id)
        .with_class(ids::TILE)
        // In with a fade when it comes into the built columns; no exit: a tile leaves the
        // built columns out of view (and an exit would cost a render of the whole last frame).
        .with_css(format!(
            "position: absolute; left: {x:.1}px; top: {y:.1}px; width: {aw}px; height: {:.1}px; \
             cursor: pointer; -azul-animation-in: azp-fade-in 260ms ease-out;",
            ah + caption_h
        ));
    if grid.kind == TileKind::Song {
        // A song: a row of text with a note, the whole row the face.
        dom.add_child(
            Dom::create_div()
                .with_css(format!(
                    "position: absolute; left: 0px; top: 0px; width: {aw}px; height: {ah}px; \
                     box-sizing: border-box; border-radius: 4px; display: flex; flex-direction: \
                     row; align-items: center; padding: 0px 12px; background: rgba(255, 255, \
                     255, {}); {}",
                    if focused { "0.16" } else { "0.05" },
                    look::face(focused, look::FOCUS_SCALE_ROW, stage)
                ))
                .with_child(icon(
                    icon_name,
                    "font-size: 22px; color: rgba(255, 255, 255, 0.7); margin-right: 12px;",
                    stage,
                ))
                .with_child(
                    Dom::create_div()
                        .with_css("display: flex; flex-direction: column; min-width: 0px;")
                        .with_child(text(
                            &title,
                            &format!(
                                "font-size: 16px; color: {}; white-space: nowrap; overflow: \
                                 hidden;",
                                look::caption_ink(focused)
                            ),
                            stage,
                        ))
                        .with_child(text(
                            &subtitle,
                            &format!(
                                "font-size: 12px; color: {}; white-space: nowrap; overflow: \
                                 hidden;",
                                look::INK_FAINT
                            ),
                            stage,
                        )),
                ),
        );
        dom.add_child(Dom::create_div().with_css(look::glow(
            focused,
            look::FOCUS_SCALE_ROW,
            aw,
            ah,
            4.0,
            stage,
        )));
    } else {
        dom.add_child(
            Dom::create_div()
                .with_css(format!(
                    "position: absolute; left: 0px; top: 0px; width: {aw}px; height: {ah}px; \
                     border-radius: 4px; overflow: hidden; background: #0b2a55; {}",
                    look::face(focused, look::FOCUS_SCALE, stage)
                ))
                .with_child(art_face(art, &title, aw, ah, icon_name, stage)),
        );
        dom.add_child(Dom::create_div().with_css(look::glow(
            focused,
            look::FOCUS_SCALE,
            aw,
            ah,
            4.0,
            stage,
        )));
        dom.add_child(
            Dom::create_div()
                .with_css(format!(
                    "position: absolute; left: 0px; top: {:.1}px; width: {aw}px; display: flex; \
                     flex-direction: column;",
                    ah + 6.0
                ))
                .with_child(text(
                    &title,
                    &format!(
                        "font-size: 15px; color: {}; white-space: nowrap; overflow: hidden;",
                        look::caption_ink(focused)
                    ),
                    stage,
                ))
                .with_child(text(
                    &subtitle,
                    &format!(
                        "font-size: 12px; color: {}; white-space: nowrap; overflow: hidden;",
                        look::INK_FAINT
                    ),
                    stage,
                )),
        );
        if let Tile::Recent(i) = tile {
            if let Some(entry) = s.history.entries.get(*i) {
                #[allow(clippy::cast_possible_truncation)]
                let watched = (entry.progress() * 100.0).round() as u32;
                dom.add_child(
                    Dom::create_div()
                        .with_css(format!(
                            "position: absolute; left: 0px; top: {:.1}px; width: {aw}px; height: \
                             3px; background: rgba(255, 255, 255, 0.2);",
                            ah - 3.0
                        ))
                        .with_child(Dom::create_div().with_css(format!(
                            "height: 3px; width: {watched}%; background: {};",
                            look::ACCENT
                        ))),
                );
            }
        }
    }
    let name = if subtitle.is_empty() {
        title.clone()
    } else {
        format!("{title}, {subtitle}")
    };
    act_part(dom, app, Act::Tile(index), &name)
}

/// Search: the field (it takes the keys while it has the focus) and the gallery of what matches.
fn search_page(s: &Player, app: &RefAny, place: &Place, stage: Stage) -> Dom {
    let field = TextInput::create_search()
        .with_text(s.query.as_str())
        .with_placeholder("type to search the music, pictures and videos")
        .with_accessibility_name("Search")
        .with_on_text_input(app.clone(), nav::on_search as TextInputOnTextInputCallbackType)
        .dom()
        .with_id(ids::SEARCH_FIELD)
        // Found again when Tab brings the keys back to the page (`nav::tab`).
        .with_marker(OptionString::Some(ids::SEARCH_FIELD))
        .with_attribute(AttributeType::Autofocus)
        .with_css(format!(
            "position: absolute; left: 60px; top: {:.0}px; width: 460px;",
            UNDER_TITLE - 4.0
        ));
    let mut page = gallery_page(s, app, place, "search", None, stage);
    page.add_child(field);
    page
}

/// The sample address the address page offers (the clip AzWidgets' Video card plays).
pub const SAMPLE_ADDRESS: &str =
    "https://test-videos.co.uk/vids/bigbuckbunny/mp4/h264/360/Big_Buck_Bunny_360_10s_2MB.mp4";

/// Open an address: the field (Enter plays what it names), a play button, a sample; the video
/// plays while it downloads.
fn address_page(s: &Player, app: &RefAny, stage: Stage) -> Dom {
    let field = TextInput::create_url()
        .with_text(s.address.as_str())
        .with_placeholder("https://\u{2026}/video.mp4")
        .with_accessibility_name("The video's address")
        .with_on_text_input(app.clone(), nav::on_address as TextInputOnTextInputCallbackType)
        .dom()
        .with_id(ids::ADDRESS_FIELD)
        .with_attribute(AttributeType::Autofocus)
        .with_css("width: 560px;");
    let play = Dom::create_div()
        .with_id(ids::ADDRESS_PLAY)
        .with_css(format!(
            "display: flex; width: 44px; height: 44px; margin-left: 14px; cursor: pointer; \
             border-radius: 22px; :focus {{ box-shadow: 0px 0px 14px 2px rgba(118, 196, 255, \
             0.95); }}"
        ))
        .with_child(Dom::create_icon("play_arrow").with_css(format!(
            "{} width: 44px; height: 44px; border-radius: 22px; font-size: 24px; {}",
            look::ROUND,
            look::icon_fade(stage)
        )));
    let sample = Dom::create_p_with_text("try: Big Buck Bunny (10 seconds, 360p)").with_css(format!(
        "margin: 0px; padding: 4px 10px; border-radius: 4px; font-size: 16px; cursor: pointer; \
         color: {}; {} :hover {{ color: #ffffff; }} :focus {{ box-shadow: 0px 0px 14px 2px \
         rgba(118, 196, 255, 0.8); }}",
        look::ACCENT,
        look::text_fade(stage)
    ));
    Dom::create_div()
        .with_child(page_title("open an address", stage))
        .with_child(
            Dom::create_div()
                .with_css(
                    "position: absolute; left: 60px; top: 146px; display: flex; flex-direction: \
                     row; align-items: center;",
                )
                .with_child(field)
                .with_child(act_part(play, app, Act::OpenAddress, "Play the address")),
        )
        .with_child(text(
            "An MP4 or MOV video (H.264) on a web server plays while it downloads; the sound \
             starts with the picture.",
            &format!(
                "position: absolute; left: 62px; top: 212px; right: 60px; font-size: 18px; \
                 font-weight: 300; color: {};",
                look::INK_DIM
            ),
            stage,
        ))
        .with_child(
            Dom::create_div()
                .with_css("position: absolute; left: 54px; top: 262px;")
                .with_child(act_part(
                    sample.with_id(ids::ADDRESS_SAMPLE),
                    app,
                    Act::Sample,
                    "Play the sample video, Big Buck Bunny",
                )),
        )
}

// ==== The settings ====

/// The settings' list: a category's row; its width.
const CATEGORY_H: f32 = 58.0;
const SETTINGS_LIST_W: f32 = 470.0;

/// The settings: their big title, the categories down the page in big type, the focused one on
/// Media Center's bar of light - ONE bar, gliding from one category to the next (a move on a
/// spring) - and what the focused one holds beside the list.
#[allow(clippy::cast_precision_loss)]
fn settings_list_page(s: &Player, app: &RefAny, place: &Place, stage: Stage) -> Dom {
    let lit = s.zone() == Zone::Content;
    let focused = place.focus.index.min(Category::ALL.len() - 1);
    let top = UNDER_TITLE + 4.0;
    let mut list = Dom::create_div()
        .with_id(ids::id("settings-list"))
        .with_css(format!(
            "position: absolute; left: 44px; top: {top:.0}px; width: {SETTINGS_LIST_W}px; \
             height: {:.0}px;",
            CATEGORY_H * Category::ALL.len() as f32
        ));
    list.add_child(
        Dom::create_div()
            .with_id(ids::id("settings-bar"))
            .with_css(format!(
                "position: absolute; left: 0px; top: {:.0}px; width: {SETTINGS_LIST_W}px; \
                 height: {CATEGORY_H}px; animation: move 260ms spring;",
                focused as f32 * CATEGORY_H
            ))
            .with_child(Dom::create_div().with_css(look::bar(lit, 4.0))),
    );
    for (i, category) in Category::ALL.iter().enumerate() {
        let ink = if i == focused && lit {
            look::INK
        } else {
            look::INK_DIM
        };
        let row = Dom::create_div()
            .with_id(ids::id(&format!("settings-{}", category.key())))
            .with_css(format!(
                "position: absolute; left: 0px; top: {:.0}px; width: {SETTINGS_LIST_W}px; \
                 height: {CATEGORY_H}px; display: flex; flex-direction: row; align-items: \
                 center; cursor: pointer;",
                i as f32 * CATEGORY_H
            ))
            .with_child(icon(
                category.icon(),
                &format!("width: 34px; margin-left: 16px; font-size: 26px; color: {ink};"),
                stage,
            ))
            .with_child(text(
                category.title(),
                &format!(
                    "margin-left: 14px; font-size: 32px; font-weight: 300; white-space: nowrap; \
                     color: {ink};"
                ),
                stage,
            ));
        list.add_child(act_part(row, app, Act::SettingsCategory(i), category.title()));
    }
    Dom::create_div()
        .with_child(page_title("settings", stage))
        .with_child(list)
        .with_child(
            text(
                Category::ALL[focused].blurb(),
                &format!(
                    "position: absolute; left: {:.0}px; top: {:.0}px; right: 60px; font-size: \
                     21px; font-weight: 300; color: {};",
                    44.0 + SETTINGS_LIST_W + 50.0,
                    top + 12.0,
                    look::INK_DIM
                ),
                stage,
            )
            .with_id(ids::id("settings-blurb")),
        )
}

/// A control's mark (its check box, its radio button, its icon), what it says and a hint at
/// its end (a folder: Enter removes it), as the draft's `values` have it.
fn control_face(row: &Row, values: &options::Options, focused: bool) -> (Dom, String, &'static str) {
    let mark_css = "display: flex; align-items: center; justify-content: center; width: 28px; \
                    height: 28px; margin-left: 16px; flex-shrink: 0; box-sizing: border-box;";
    match row {
        Row::Check { key, label } => {
            let on = values.is_on(key);
            let mark = Dom::create_div()
                .with_css(format!(
                    "{mark_css} border-radius: 4px; border: 2px solid rgba(255, 255, 255, 0.9); \
                     background-color: {}; animation: background-color 160ms ease-out;",
                    if on { look::ACCENT } else { "rgba(255, 255, 255, 0.06)" }
                ))
                .with_child(Dom::create_icon("check").with_css(format!(
                    "font-size: 22px; color: #ffffff; opacity: {}; animation: opacity 140ms \
                     ease-out;",
                    if on { 1 } else { 0 }
                )));
            (mark, (*label).to_string(), "")
        }
        Row::Radio { key, value, label } => {
            let chosen = values.get(key) == *value;
            let mark = Dom::create_div()
                .with_css(format!(
                    "{mark_css} border-radius: 14px; border: 2px solid rgba(255, 255, 255, 0.9);"
                ))
                .with_child(Dom::create_div().with_css(format!(
                    "width: 14px; height: 14px; border-radius: 7px; background: #ffffff; \
                     opacity: {}; transform: scale({}); animation: opacity 140ms ease-out, \
                     transform 200ms spring-snappy;",
                    if chosen { 1 } else { 0 },
                    if chosen { 1.0 } else { 0.4 }
                )));
            (mark, (*label).to_string(), "")
        }
        Row::Folder { path, .. } => (
            Dom::create_icon("folder").with_css(format!(
                "{mark_css} font-size: 26px; color: rgba(255, 255, 255, 0.85);"
            )),
            path.display().to_string(),
            if focused { "Enter removes it" } else { "" },
        ),
        Row::Button { label, icon, .. } => (
            Dom::create_icon(*icon).with_css(format!(
                "{mark_css} font-size: 26px; color: {};",
                look::ACCENT
            )),
            label.clone(),
            "",
        ),
        Row::Heading(t) | Row::Note(t) => (Dom::create_div(), t.clone(), ""),
    }
}

/// A category of the settings: its big title over "settings", its rows - check boxes, radio
/// lists, folders, buttons, each a whole row of big type to land on - scrolled to keep the
/// focused one in view (the rows glide), the bar of light gliding behind the focused one; save
/// and cancel (about: ok) at the bottom right, each lit when the keys are on it.
#[allow(clippy::cast_precision_loss, clippy::too_many_lines)]
fn settings_page(
    s: &Player,
    app: &RefAny,
    place: &Place,
    category: Category,
    stage: Stage,
) -> Dom {
    let rows = settings::page_rows(s, category);
    let at = options::focusable(&rows);
    let n = at.len();
    let buttons = category.buttons();
    let focus = place.focus.index.min((n + buttons.len()).saturating_sub(1));
    let lit = s.zone() == Zone::Content;
    let values = s.draft.as_ref().map_or(&s.options, |d| &d.options);
    let top = UNDER_TITLE + 40.0;
    let bottom = 104.0;
    let visible = (s.window.1 - top - bottom).max(120.0);
    let focused_row = at.get(focus).copied();
    let scroll = options::scroll_for(&rows, focused_row, visible);
    // Where each row stands on the sheet, and the focused one's place for the bar.
    let mut placed = Vec::with_capacity(rows.len());
    let mut y = 0.0_f32;
    let mut bar = (0.0_f32, 52.0_f32);
    for (i, row) in rows.iter().enumerate() {
        if Some(i) == focused_row {
            bar = (y, row.height());
        }
        placed.push(y);
        y += row.height();
    }
    let mut sheet = Dom::create_div()
        .with_id(ids::id("settings-rows"))
        .with_css(format!(
            "position: absolute; left: 0px; right: 0px; top: {:.0}px; height: {y:.0}px; \
             animation: move 300ms spring;",
            -scroll
        ));
    // The bar of light: ONE, gliding to the focused row (it fades while a button has the keys).
    sheet.add_child(
        Dom::create_div()
            .with_id(ids::id("settings-focus"))
            .with_css(format!(
                "position: absolute; left: 0px; right: 0px; top: {:.0}px; height: {:.0}px; \
                 animation: move 260ms spring;",
                bar.0, bar.1
            ))
            .with_child(Dom::create_div().with_css(look::bar(lit && focused_row.is_some(), 4.0))),
    );
    let mut k = 0;
    for (i, row) in rows.iter().enumerate() {
        let at_css = format!(
            "position: absolute; left: 0px; right: 0px; top: {:.0}px; height: {:.0}px;",
            placed[i],
            row.height()
        );
        match row {
            Row::Heading(t) => sheet.add_child(text(
                t,
                &format!(
                    "{at_css} padding: 16px 0px 0px 16px; box-sizing: border-box; font-size: 20px; \
                     color: {};",
                    look::ACCENT
                ),
                stage,
            )),
            Row::Note(t) => sheet.add_child(text(
                t,
                &format!(
                    "{at_css} padding: 4px 16px 0px 16px; box-sizing: border-box; overflow: \
                     hidden; font-size: 17px; line-height: 24px; color: {};",
                    look::INK_DIM
                ),
                stage,
            )),
            _ => {
                let focused = Some(i) == focused_row && lit;
                let ink = if focused {
                    look::INK
                } else {
                    "rgba(255, 255, 255, 0.82)"
                };
                let (mark, label, hint) = control_face(row, values, focused);
                let mut dom = Dom::create_div()
                    .with_id(ids::id(&format!("setting-{k}")))
                    .with_css(format!(
                        "{at_css} display: flex; flex-direction: row; align-items: center; \
                         cursor: pointer;"
                    ))
                    .with_child(mark)
                    .with_child(text(
                        &label,
                        &format!(
                            "margin-left: 16px; flex-grow: 1; min-width: 0px; font-size: 22px; \
                             white-space: nowrap; overflow: hidden; color: {ink};"
                        ),
                        stage,
                    ));
                if !hint.is_empty() {
                    dom.add_child(text(
                        hint,
                        &format!(
                            "margin-right: 18px; font-size: 15px; white-space: nowrap; color: {};",
                            look::INK_DIM
                        ),
                        stage,
                    ));
                }
                sheet.add_child(act_part(dom, app, Act::Setting(k), &label));
                k += 1;
            }
        }
    }
    let mut button_row = Dom::create_div()
        .with_id(ids::id("settings-buttons"))
        .with_css(
            "position: absolute; right: 60px; bottom: 36px; display: flex; flex-direction: row;",
        );
    for (j, label) in buttons.iter().enumerate() {
        let index = n + j;
        let on = lit && focus == index;
        let button = Dom::create_div()
            .with_id(ids::id(&format!("settings-{label}")))
            .with_css(
                "position: relative; width: 150px; height: 50px; margin-left: 16px; display: \
                 flex; align-items: center; justify-content: center; cursor: pointer; \
                 border-radius: 4px; background: linear-gradient(to bottom, rgba(255, 255, 255, \
                 0.16), rgba(255, 255, 255, 0.04)); border: 1px solid rgba(255, 255, 255, 0.3);",
            )
            .with_child(Dom::create_div().with_css(look::bar(on, 4.0)))
            .with_child(text(
                label,
                &format!(
                    "position: relative; font-size: 21px; color: {};",
                    if on { look::INK } else { look::INK_DIM }
                ),
                stage,
            ));
        button_row.add_child(act_part(button, app, Act::Setting(index), label));
    }
    Dom::create_div()
        .with_child(page_title(category.title(), stage))
        .with_child(text(
            "settings",
            &format!(
                "position: absolute; left: 62px; top: {UNDER_TITLE:.0}px; font-size: 18px; color: \
                 {};",
                look::INK_DIM
            ),
            stage,
        ))
        .with_child(
            Dom::create_div()
                .with_css(format!(
                    "position: absolute; left: 44px; right: 60px; top: {top:.0}px; bottom: \
                     {bottom:.0}px; overflow: hidden;"
                ))
                .with_child(sheet),
        )
        .with_child(button_row)
}

// ==== Now playing ====

/// Now playing: the cover large, the song, the album, the seek bar between the times, what
/// comes next; the transport bottom right (it hides while the music plays and the pointer
/// rests).
fn now_playing_page(s: &Player, app: &RefAny, stage: Stage) -> Dom {
    let mut page = Dom::create_div();
    let Some(music) = s.music.as_ref() else {
        page.add_child(page_title("now playing", stage));
        page.add_child(text(
            "Nothing is playing. Choose music library or play all on the start screen.",
            &format!(
                "position: absolute; left: 62px; top: {UNDER_TITLE:.0}px; font-size: 22px; \
                 font-weight: 300; color: {};",
                look::INK_DIM
            ),
            stage,
        ));
        return page;
    };
    let song = music.current_item(s);
    let title = song.map_or_else(
        || {
            music
                .current()
                .map(|p| library::file_title(std::path::Path::new(p)))
                .unwrap_or_default()
        },
        |i| i.title.clone(),
    );
    let artist = song.map(library::Item::filed_artist).unwrap_or_default();
    let album = song.map(library::Item::album_or_unknown).unwrap_or_default();
    let art = song
        .filter(|i| i.has_cover)
        .and_then(|i| s.art.get(&cover_key(&i.path)).cloned().flatten());
    let art_size = (s.window.1 * 0.42).clamp(180.0, 340.0).round();
    let left = (s.window.0 * 0.07).round();
    let info_left = left + art_size + 36.0;
    page.add_child(
        Dom::create_div()
            .with_id(ids::id(&format!("np-art-{}", pages::short_hash(&title))))
            .with_css(format!(
                "position: absolute; left: {left}px; top: 96px; width: {art_size}px; height: \
                 {art_size}px; overflow: hidden; border-radius: 4px; box-shadow: 0px 10px 30px \
                 rgba(0, 0, 0, 0.55); -azul-animation-in: azp-page-in 420ms spring;"
            ))
            .with_child(art_face(art, &album, art_size, art_size, "album", stage)),
    );
    let position = music.state.position_s.max(0.0);
    let duration = music.state.duration_s.max(0.0);
    let time_css = format!(
        "flex-shrink: 0; min-width: 52px; font-size: 13px; color: {}; {}",
        look::INK_DIM,
        look::text_fade(stage)
    );
    page.add_child(
        Dom::create_div()
            .with_css(format!(
                "position: absolute; left: {info_left}px; right: 40px; top: 104px; display: \
                 flex; flex-direction: column;"
            ))
            .with_child(text(
                if music.playing() { "now playing" } else { "paused" },
                &format!("font-size: 14px; color: {};", look::ACCENT),
                stage,
            ))
            .with_child(text(
                &title,
                "font-size: 38px; font-weight: 300; margin-top: 6px; white-space: nowrap; \
                 overflow: hidden;",
                stage,
            ))
            .with_child(text(
                &artist,
                &format!("font-size: 22px; color: {}; margin-top: 4px;", look::INK),
                stage,
            ))
            .with_child(text(
                &album,
                &format!("font-size: 18px; color: {}; margin-top: 2px;", look::INK_DIM),
                stage,
            ))
            .with_child(
                Dom::create_div()
                    .with_css(
                        "display: flex; flex-direction: row; align-items: center; margin-top: \
                         26px; max-width: 520px;",
                    )
                    .with_child(
                        live_text(
                            SeekBar::media_time(position).as_str().to_string(),
                            ids::ELAPSED,
                            &time_css,
                        )
                        .with_id(ids::ELAPSED),
                    )
                    .with_child(
                        SeekBar::create(position, duration)
                            .with_show_times(false)
                            .with_accessibility_name("Position in the song")
                            .with_on_seek(app.clone(), media::ON_SEEK)
                            .dom()
                            .with_id(ids::SEEK)
                            .with_marker(OptionString::Some(ids::SEEK))
                            .with_css("flex-grow: 1; margin: 0px 12px;"),
                    )
                    .with_child(
                        live_text(
                            SeekBar::media_time(duration).as_str().to_string(),
                            ids::TOTAL,
                            &format!("{time_css} text-align: right;"),
                        )
                        .with_id(ids::TOTAL),
                    ),
            )
            .with_child(up_next(s, music, stage)),
    );
    page.add_child(transport_bar(s, app, Shelf::Music, stage));
    page
}

/// "up next": the next songs of the queue.
fn up_next(s: &Player, music: &Music, stage: Stage) -> Dom {
    let mut list = Dom::create_div()
        .with_css("display: flex; flex-direction: column; margin-top: 30px;")
        .with_child(text(
            "up next",
            &format!("font-size: 14px; color: {};", look::ACCENT),
            stage,
        ));
    let songs = s.items(Shelf::Music);
    for path in music.paths.iter().skip(music.index + 1).take(5) {
        let title = songs.iter().find(|i| &i.path == path).map_or_else(
            || library::file_title(std::path::Path::new(path)),
            |i| format!("{} \u{b7} {}", i.title, i.filed_artist()),
        );
        list.add_child(text(
            &title,
            &format!(
                "font-size: 16px; color: {}; margin-top: 6px; white-space: nowrap; overflow: \
                 hidden;",
                look::INK_DIM
            ),
            stage,
        ));
    }
    if music.index + 1 >= music.paths.len() {
        list.add_child(text(
            "the end of the queue",
            &format!("font-size: 16px; color: {}; margin-top: 6px;", look::INK_FAINT),
            stage,
        ));
    }
    list
}

/// The round transport bottom right (Media Center puts it there), shown and hidden in place
/// (`BAR`).
fn transport_bar(s: &Player, app: &RefAny, what: Shelf, stage: Stage) -> Dom {
    Dom::create_div()
        .with_id(ids::BAR)
        .with_marker(OptionString::Some(ids::BAR))
        .with_css(format!(
            "position: absolute; right: 28px; bottom: 24px; display: flex; flex-direction: row; \
             opacity: {}; animation: opacity 300ms ease-out;",
            if s.controls_shown { 1 } else { 0 }
        ))
        .with_child(transport(s, app, what, stage))
}

/// The transport's buttons (`media::transport_buttons`): the one the keyboard is on has its
/// ring while Tab has the keys in the transport (Left / Right walk it, Enter presses).
fn transport(s: &Player, app: &RefAny, what: Shelf, stage: Stage) -> Dom {
    let buttons = media::transport_buttons(s, what);
    let keys_here = s.zone() == Zone::Transport;
    let focused = media::transport_focus(&buttons, s.place().focus.transport);
    let mut controls = Dom::create_div().with_id(ids::CONTROLS).with_css(
        "display: flex; flex-direction: row; align-items: center; flex-shrink: 0;",
    );
    for (i, b) in buttons.iter().enumerate() {
        if b.gap {
            controls.add_child(Dom::create_div().with_css("width: 22px; flex-shrink: 0;"));
        }
        controls.add_child(round_button(
            app,
            b.command,
            (b.icon, b.name),
            b.size,
            ids::id(b.id),
            stage,
            keys_here && i == focused,
        ));
    }
    controls
}

// ==== The picture viewer and the slide show ====

/// The picture viewer on black: the picture as large as fits, the next one cross-fading in with
/// Ken Burns' pan and zoom while the slide show plays; its name and number bottom left, the
/// transport bottom right.
fn picture_page(s: &Player, app: &RefAny) -> Dom {
    let stage = Curtain::Closed.stage();
    let mut page = Dom::create_div().with_css("background: #000000;");
    let viewer = &s.viewer;
    let Some(path) = viewer.paths.get(viewer.index) else {
        return page;
    };
    let (w, h) = (s.window.0.max(100.0), s.window.1.max(100.0));
    let full = s.art.get(&full_key(path)).cloned().flatten();
    let thumb = s.art.get(&thumb_key(path)).cloned().flatten();
    let mut slide = Dom::create_div()
        .with_id(ids::id(&format!("slide-{}", viewer.shown)))
        .with_css(format!(
            "{} {}",
            look::FILL,
            if viewer.playing && s.options.is_on(options::KEN_BURNS) {
                look::slide_motion(viewer.shown, media::slide_s(s))
            } else if viewer.playing {
                // The settings turned pan and zoom off: a plain cross-fade.
                String::from(
                    "-azul-animation-in: azp-fade-in 900ms ease-in-out; -azul-animation-out: \
                     azp-fade-out 900ms ease-in-out no-clip;",
                )
            } else {
                String::from(
                    "-azul-animation-in: azp-fade-in 260ms ease-out; -azul-animation-out: \
                     azp-fade-out 260ms ease-in no-clip;",
                )
            }
        ));
    match full.or(thumb) {
        Some((image, iw, ih)) if iw > 0.0 && ih > 0.0 => {
            // As large as fits, the middle of the window.
            let scale = (w / iw).min(h / ih);
            let (dw, dh) = (iw * scale, ih * scale);
            slide.add_child(Dom::create_image(image).with_css(format!(
                "position: absolute; left: {:.1}px; top: {:.1}px; width: {dw:.1}px; height: \
                 {dh:.1}px;",
                (w - dw) / 2.0,
                (h - dh) / 2.0
            )));
        }
        _ => {
            slide.add_child(text(
                "loading the picture\u{2026}",
                &format!(
                    "position: absolute; left: 0px; right: 0px; top: 48%; text-align: center; \
                     font-size: 18px; color: {};",
                    look::INK_DIM
                ),
                stage,
            ));
        }
    }
    page.add_child(slide);
    page.add_child(
        Dom::create_div()
            .with_id(ids::TOP)
            .with_marker(OptionString::Some(ids::TOP))
            .with_css(format!(
                "position: absolute; left: 28px; bottom: 26px; display: flex; flex-direction: \
                 column; opacity: {}; animation: opacity 300ms ease-out;",
                if s.controls_shown { 1 } else { 0 }
            ))
            .with_child(text(
                &library::file_title(std::path::Path::new(path)),
                "font-size: 20px;",
                stage,
            ))
            .with_child(
                text(
                    &format!("{} of {}", viewer.index + 1, viewer.paths.len()),
                    &format!("font-size: 13px; color: {};", look::INK_DIM),
                    stage,
                )
                .with_id(ids::PICTURE_CAPTION),
            ),
    );
    page.add_child(transport_bar(s, app, Shelf::Pictures, stage));
    page.with_callback(
        EventFilter::Hover(HoverEventFilter::DoubleClick),
        app.clone(),
        nav::on_double_click,
    )
}

// ==== The stage: a video ====

/// The stage: the video filling it (black around it), the chrome, the OSD and a note over it.
/// Hidden while the video opens; faded in from black when picture and sound start.
fn video_stage(s: &Player, video: &VideoSession, app: &RefAny, stage: Stage) -> Dom {
    let config = VideoConfig {
        // A web address plays while it downloads (range requests, a window ahead).
        source: match media::web_address(&video.path) {
            Some(url) => VideoSource::Url(url),
            None => VideoSource::File(AzString::from(video.path.as_str())),
        },
        timestamp: video.seek_s,
        autoplay: true,
        looping: false,
        // Held while the curtain is down: the first picture is decoded and shown (hidden), the
        // clock does not run until both picture and sound are ready.
        paused: video.paused,
        // NV12 (4:2:0 in two planes) as the decoder makes it: 1.5 bytes a pixel against BGRA's
        // 4 through the worker's scaler, the write-back's copy and the upload; the renderer
        // converts it as it draws. The frame's own matrix and range travel with it; the Rec.709
        // here only says "NV12".
        output_format: RawImageFormat::NV12Rec709Video,
    };
    let video_dom = VideoWidget::create(config)
        .with_on_status(app.clone(), media::on_video_status)
        .dom()
        .with_id(ids::VIDEO)
        .with_css("width: 100%; height: 100%;")
        .with_callback(
            EventFilter::Hover(HoverEventFilter::DoubleClick),
            app.clone(),
            nav::on_double_click,
        );
    let open = video.curtain == Curtain::Open;
    let mut dom = Dom::create_div()
        .with_id(ids::STAGE)
        .with_css(format!(
            "{} display: flex; background-color: #000000; overflow: hidden; {} {}",
            look::FILL,
            look::picture_fade(stage),
            if open {
                "-azul-animation-out: azp-fade-out 320ms ease-in;"
            } else {
                ""
            }
        ))
        .with_child(video_dom);
    if open {
        if video.status.phase == VideoPhase::Failed {
            dom.add_child(
                Dom::create_p_with_text(
                    format!(
                        "This video does not play here: {}",
                        video.status.message.as_str()
                    )
                    .as_str(),
                )
                .with_id(ids::NOTE)
                .with_css(format!("{} left: 32px; top: 88px;", look::PANEL)),
            );
        }
        let osd_text = s
            .osd
            .as_ref()
            .map_or_else(|| String::from("\u{a0}"), |(t, _)| t.clone());
        dom.add_child(
            live_text(
                osd_text,
                ids::OSD_TEXT,
                &format!(
                    "{} top: 88px; right: 32px; opacity: {}; animation: opacity 200ms ease-out;",
                    look::PANEL,
                    if s.osd.is_some() { 1 } else { 0 }
                ),
            )
            .with_id(ids::OSD)
            .with_marker(OptionString::Some(ids::OSD)),
        );
        dom.add_child(top_strip(s, video, app));
        dom.add_child(bottom_strip(s, video, app));
    }
    dom
}

/// The top strip over the picture: back on the left (right of the traffic lights), the title,
/// fullscreen on the right. It is the window's top band while a video plays: it moves the
/// window, its buttons are their own.
fn top_strip(s: &Player, video: &VideoSession, app: &RefAny) -> Dom {
    let stage = Curtain::Closed.stage();
    let (full_icon, full_name) = if s.fullscreen {
        ("fullscreen_exit", "Leave fullscreen")
    } else {
        ("fullscreen", "Fullscreen")
    };
    let (lights, controls) = window_controls(s);
    Dom::create_div()
        .with_id(ids::TOP)
        .with_marker(OptionString::Some(ids::TOP))
        .with_css(format!(
            "position: absolute; left: 0px; top: 0px; right: 0px; height: 72px; display: flex; \
             flex-direction: row; align-items: center; padding: 0px {:.0}px 0px {:.0}px; \
             background: linear-gradient(to bottom, rgba(0, 0, 0, 0.75), rgba(0, 0, 0, 0)); \
             opacity: {}; animation: opacity 300ms ease-out; cursor: default; {DRAG}",
            24.0 + controls,
            16.0 + lights,
            if s.controls_shown { 1 } else { 0 }
        ))
        .with_child(round_button(
            app,
            Command::Back,
            ("arrow_back", "Back"),
            40.0,
            ids::BACK,
            stage,
            corner_lit(s, 0),
        ))
        .with_child(
            Dom::create_p_with_text(video.title().as_str())
                .with_id(ids::TITLE)
                .with_css(
                    "flex-grow: 1; min-width: 0px; margin: 0px 0px 0px 18px; font-size: 22px; \
                     font-weight: 300; color: #ffffff; white-space: nowrap; overflow: hidden;",
                ),
        )
        .with_child(round_button(
            app,
            Command::Fullscreen,
            (full_icon, full_name),
            40.0,
            ids::FULLSCREEN,
            stage,
            corner_lit(s, 1),
        ))
}

/// Whether the top band's button `index` (`media::corner_buttons`) has the keyboard's ring.
fn corner_lit(s: &Player, index: usize) -> bool {
    s.zone() == Zone::Corner && s.place().focus.corner == index
}

/// The bottom strip over the picture: the seek bar between the times, the transport bottom
/// right.
fn bottom_strip(s: &Player, video: &VideoSession, app: &RefAny) -> Dom {
    let stage = Curtain::Closed.stage();
    let position = f64::from(video.status.position_s).max(0.0);
    let duration = f64::from(video.status.duration_s).max(0.0);
    let time_css = "flex-shrink: 0; min-width: 56px; font-size: 13px; color: #ffffff;";
    let seek_row = Dom::create_div()
        .with_css("display: flex; flex-direction: row; align-items: center;")
        .with_child(
            live_text(
                SeekBar::media_time(position).as_str().to_string(),
                ids::ELAPSED,
                time_css,
            )
            .with_id(ids::ELAPSED),
        )
        .with_child(
            SeekBar::create(position, duration)
                .with_show_times(false)
                .with_accessibility_name("Position in the video")
                .with_on_seek(app.clone(), media::on_seek as SeekBarOnSeekCallbackType)
                .dom()
                .with_id(ids::SEEK)
                .with_marker(OptionString::Some(ids::SEEK))
                .with_css("flex-grow: 1; margin: 0px 14px;"),
        )
        .with_child(
            live_text(
                SeekBar::media_time(duration).as_str().to_string(),
                ids::TOTAL,
                &format!("{time_css} text-align: right;"),
            )
            .with_id(ids::TOTAL),
        );
    // Over the picture the transport sits bottom right in the bottom strip, under the seek row;
    // the strip shows and hides as one (`BAR`).
    let controls = Dom::create_div()
        .with_css(
            "display: flex; flex-direction: row; justify-content: flex-end; margin-top: 12px;",
        )
        .with_child(transport(s, app, Shelf::Videos, stage));
    Dom::create_div()
        .with_id(ids::BAR)
        .with_marker(OptionString::Some(ids::BAR))
        .with_css(format!(
            "position: absolute; left: 0px; right: 0px; bottom: 0px; display: flex; \
             flex-direction: column; padding: 32px 28px 18px 28px; background: \
             linear-gradient(to bottom, rgba(0, 0, 0, 0), rgba(0, 0, 0, 0.85)); opacity: {}; \
             animation: opacity 300ms ease-out;",
            if s.controls_shown { 1 } else { 0 }
        ))
        .with_child(seek_row)
        .with_child(controls)
}

// ==== The menu bar ====

/// A menu item opening a library's page.
pub struct PageRef {
    pub app: RefAny,
    pub section: Section,
}

extern "C" fn on_menu_page(mut data: RefAny, _info: CallbackInfo) -> Update {
    let Some((app, section)) = data
        .downcast_ref::<PageRef>()
        .map(|p| (p.app.clone(), p.section))
    else {
        return Update::DoNothing;
    };
    let mut app_ref = app.clone();
    if let Some(mut s) = app_ref.downcast_mut::<Player>() {
        if s.place().screen != Screen::Section(section) {
            nav::go(&mut s, Screen::Section(section));
        }
    }
    Update::RefreshDom
}

/// The menu bar: Media, Playback, Audio, View.
fn menu_bar(s: &Player, app: &RefAny) -> Menu {
    let item = |label: &str, command: Command| {
        MenuItem::string(StringMenuItem::create(label).with_callback(
            RefAny::new(CommandRef {
                app: app.clone(),
                command,
            }),
            media::on_command,
        ))
    };
    let page = |label: &str, section: Section| {
        MenuItem::string(StringMenuItem::create(label).with_callback(
            RefAny::new(PageRef {
                app: app.clone(),
                section,
            }),
            on_menu_page,
        ))
    };
    let menu = |label: &str, items: Vec<MenuItem>| {
        MenuItem::string(StringMenuItem::create(label).with_children(items))
    };
    let recent: Vec<MenuItem> = s
        .history
        .entries
        .iter()
        .take(10)
        .map(|e| {
            MenuItem::string(StringMenuItem::create(e.title()).with_callback(
                RefAny::new(RecentRef {
                    app: app.clone(),
                    path: e.path.clone(),
                }),
                on_menu_recent,
            ))
        })
        .collect();
    let mut media_items = vec![item("Open File\u{2026}", Command::Open)];
    if !recent.is_empty() {
        media_items.push(menu("Open Recent", recent));
    }
    media_items.push(MenuItem::separator());
    media_items.push(item("Start", Command::Home));
    media_items.push(item("Refresh Libraries", Command::Refresh));
    media_items.push(item("Settings\u{2026}", Command::Settings));
    Menu::create(vec![
        menu("Media", media_items),
        menu(
            "Playback",
            vec![
                item("Play / Pause", Command::PlayPause),
                item("Stop", Command::Stop),
                MenuItem::separator(),
                item("Previous", Command::Previous),
                item("Next", Command::Next),
                item("Back 10 Seconds", Command::Rewind),
                item("Forward 30 Seconds", Command::Forward),
                MenuItem::separator(),
                item("Shuffle", Command::Shuffle),
            ],
        ),
        menu(
            "Audio",
            vec![
                item(if s.muted { "Sound On" } else { "Mute" }, Command::Mute),
                item("Volume Up", Command::VolumeUp),
                item("Volume Down", Command::VolumeDown),
            ],
        ),
        menu(
            "View",
            vec![
                page("Music", Section::Music),
                page("Pictures", Section::Pictures),
                page("Videos", Section::Videos),
                page("Movies", Section::Movies),
                page("Recorded TV", Section::Tv),
                page("Recently Played", Section::Recent),
                MenuItem::separator(),
                item(
                    if s.fullscreen {
                        "Leave Fullscreen"
                    } else {
                        "Fullscreen"
                    },
                    Command::Fullscreen,
                ),
            ],
        ),
    ])
}

/// A recent file of the menu.
pub struct RecentRef {
    pub app: RefAny,
    pub path: String,
}

/// A recent file from the menu: it opens (resuming where it was left).
extern "C" fn on_menu_recent(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((app, path)) = data
        .downcast_ref::<RecentRef>()
        .map(|r| (r.app.clone(), r.path.clone()))
    else {
        return Update::DoNothing;
    };
    media::open_video(&app, &mut info, &path);
    Update::RefreshDom
}
