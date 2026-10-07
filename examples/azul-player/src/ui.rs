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
    option::{OptionColorU, OptionString},
    prelude::*,
    shells::{MediaShell, ShellThemeAccent, ShellThemeScope},
    str::String as AzString,
    video::{VideoConfig, VideoPhase, VideoSource},
    widgets::{SeekBar, TextInput, Titlebar, VideoWidget},
};
use azul_appkit::ui as kit;

use crate::{
    app::{self, cover_key, full_key, thumb_key, Art, Player},
    args::SPEC,
    curtain::{Curtain, Stage},
    gallery::{Grid, TileKind},
    ids,
    library::{self, Shelf, Status},
    look::{self, Light},
    media::{self, Command, CommandRef, Music, VideoSession},
    nav::{self, Act, ActRef},
    pages::{self, Place, Screen, Section, Tile},
    strip::{Entry, CATEGORIES},
};

// ==== Geometry ====

/// A library page's gallery: its distance from the page's left, top and bottom (logical px).
pub const GALLERY_LEFT: f32 = 56.0;
pub const GALLERY_TOP: f32 = 168.0;
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
    let column = if kit::settings_open(&s.kit) {
        // The settings: the kit's page in the theme's own chrome.
        let page = kit::settings_page(&s.kit, Vec::new());
        let mut office = MediaShell::create_player(page, Dom::create_div()).office_shell();
        if !s.fullscreen {
            office = office.with_title_row(kit::title_row(SPEC.name));
        }
        Dom::create_div().with_css(COLUMN).with_child(office.dom())
    } else {
        let mut column = Dom::create_div().with_css(COLUMN);
        if !s.fullscreen {
            column.add_child(title_row(s));
        }
        column.add_child(root(s, &app));
        column
    };
    let mut body = ShellThemeScope::create(column)
        .with_accent(ShellThemeAccent::Leaf)
        .body();
    if !s.fullscreen {
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

/// The title row (the window is `NoTitle`): the ground's blue over the menus, black over a
/// picture or a video, the title in light ink.
fn title_row(s: &Player) -> Dom {
    let playing = s.video.as_ref().map(VideoSession::title);
    let title = match playing {
        Some(t) if !t.is_empty() => format!("{t} - {}", SPEC.name),
        _ => SPEC.name.to_string(),
    };
    let ground = if s.place().screen.on_ground() && s.video.is_none() {
        ColorU::rgb(0x1b, 0x5a, 0xa6)
    } else {
        ColorU::rgb(0, 0, 0)
    };
    let mut bar = Titlebar::create(title.as_str())
        .with_background(ground)
        .with_background_inactive(ground)
        .without_border_bottom();
    bar.title_color = ColorU::rgb(0xe6, 0xf0, 0xff);
    bar.title_color_inactive = OptionColorU::Some(ColorU::rgb(0x9a, 0xb4, 0xd6));
    bar.dom()
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

// ==== The corner pieces: the orb, back, the clock ====

/// The Media Center orb (the start strip) and the back button top left - the back button shows
/// when the pointer moves and hides when it rests (in place) - and the clock top right.
fn corner(s: &Player, app: &RefAny, place: &Place, stage: Stage) -> Dom {
    let mut left = Dom::create_div().with_css(
        "position: absolute; left: 18px; top: 14px; display: flex; flex-direction: row; \
         align-items: center;",
    );
    if place.screen != Screen::Start {
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
                    "arrow_back",
                    "Back",
                    34.0,
                    ids::BACK,
                    stage,
                )),
        );
    }
    left.add_child(orb(app, 38.0, 10.0, stage));
    let clock = live_text(
        s.clock_text.clone(),
        ids::CLOCK_TEXT,
        &format!(
            "position: absolute; right: 30px; top: 18px; font-size: 24px; font-weight: 300; \
             color: {}; {}",
            look::INK,
            look::text_fade(stage)
        ),
    )
    .with_id(ids::CLOCK);
    Dom::create_div()
        .with_id(ids::LOGO)
        .with_css(look::FILL.replace("bottom: 0px;", "height: 0px;"))
        .with_child(left)
        .with_child(clock)
}

/// The green orb (Media Center's start button): the start strip.
fn orb(app: &RefAny, size: f32, margin_left: f32, stage: Stage) -> Dom {
    let radius = size / 2.0;
    let icon_px = (size * 0.6).round();
    Dom::create_div()
        .with_tab_index(TabIndex::Auto)
        .with_accessibility_name("Start")
        .with_css(format!(
            "{} width: {size}px; height: {size}px; border-radius: {radius}px; margin-left: \
             {margin_left}px; cursor: pointer; {} :hover {{ box-shadow: 0px 0px 16px rgba(150, \
             240, 120, 0.9); }} :focus {{ box-shadow: 0px 0px 16px rgba(150, 240, 120, 0.9); }}",
            look::ORB,
            look::icon_fade(stage)
        ))
        .with_callback(
            EventFilter::Hover(HoverEventFilter::Click),
            RefAny::new(CommandRef {
                app: app.clone(),
                command: Command::Home,
            }),
            media::on_command,
        )
        .with_child(Dom::create_icon("play_arrow").with_css(format!(
            "font-size: {icon_px}px; color: #ffffff;"
        )))
}

/// A round glass button doing `command`: an icon, named `name` for assistive technology; the
/// glow under the pointer and while it has the keyboard focus.
fn round_button(
    app: &RefAny,
    command: Command,
    icon_name: &str,
    name: &str,
    size: f32,
    id: AzString,
    stage: Stage,
) -> Dom {
    let icon_px = (size * 0.5).round();
    let radius = size / 2.0;
    // The box takes the click, the focus and the name; its icon is the face.
    Dom::create_div()
        .with_id(id)
        .with_css(format!(
            "display: flex; flex-shrink: 0; width: {size}px; height: {size}px; margin-left: \
             8px; cursor: pointer; border-radius: {radius}px; :focus {{ box-shadow: 0px 0px \
             14px 2px rgba(118, 196, 255, 0.95); }}"
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
    let inset = Dom::create_div()
        .with_id(ids::INSET)
        .with_css(
            "position: absolute; left: 24px; bottom: 22px; width: 300px; height: 64px; display: \
             flex; flex-direction: row; align-items: center; padding: 4px; box-sizing: \
             border-box; border-radius: 6px; background: rgba(4, 18, 43, 0.55); border: 1px \
             solid rgba(255, 255, 255, 0.18); cursor: pointer; -azul-animation-in: azp-rise-in \
             360ms spring; :hover { border: 1px solid rgba(255, 255, 255, 0.7); } :focus { \
             box-shadow: 0px 0px 14px 2px rgba(118, 196, 255, 0.85); }",
        )
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
fn start_page(s: &Player, app: &RefAny, stage: Stage) -> Dom {
    let title_row = if s.fullscreen { 0.0 } else { 32.0 };
    let height = (s.window.1 - title_row).max(200.0);
    let width = s.window.0.max(320.0);
    let focused = s.strip.row;
    // The focused category's middle at 52 % of the height.
    #[allow(clippy::cast_precision_loss)]
    let top = height * 0.52 - focused as f32 * ROW_H - FOCUSED_ROW_H / 2.0;
    let left = (width * 0.04).round();
    let items_left = left + NAME_W + 24.0;
    let mut column = Dom::create_div().with_id(ids::STRIP).with_css(format!(
        "position: absolute; left: 0px; right: 0px; top: {top:.1}px; display: flex; \
         flex-direction: column;"
    ));
    for (row, category) in CATEGORIES.iter().enumerate() {
        let is_focused = row == focused;
        let h = if is_focused { FOCUSED_ROW_H } else { ROW_H };
        let name = Dom::create_p_with_text(category.name)
            .with_css(format!(
                "position: absolute; left: {left}px; width: {NAME_W}px; top: 0px; height: {h}px; \
                 margin: 0px; display: flex; align-items: center; justify-content: flex-end; \
                 font-size: {}px; font-weight: 300; color: {}; white-space: nowrap; cursor: \
                 pointer; {} :hover {{ color: #ffffff; }}",
                if is_focused { 38 } else { 26 },
                if is_focused { look::INK } else { look::INK_FAINT },
                look::text_fade(stage)
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
                "position: relative; height: {h}px; flex-shrink: 0;"
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
             flex-direction: row;",
            items_left - shift,
            STRIP_TILE_H + 36.0
        ));
    for (i, entry) in category.entries.iter().enumerate() {
        let focused = i == col;
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
    let mut row = Dom::create_div().with_id(ids::VIEWS).with_css(
        "position: absolute; left: 60px; top: 106px; display: flex; flex-direction: row; \
         align-items: center;",
    );
    for (i, view) in section.views().iter().enumerate() {
        let shown = i == place.focus.view;
        let focused = shown && place.focus.on_views;
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
        .with_child(text(
            title,
            &format!(
                "position: absolute; left: 56px; top: 30px; {} color: {};",
                look::PAGE_TITLE,
                look::INK
            ),
            stage,
        ))
        .with_callback(
            EventFilter::Hover(HoverEventFilter::Scroll),
            app.clone(),
            nav::on_wheel,
        );
    if let Some(sub) = subtitle {
        page.add_child(text(
            sub,
            &format!(
                "position: absolute; left: 62px; top: 110px; font-size: 18px; color: {};",
                look::INK_DIM
            ),
            stage,
        ));
    }
    let offset = grid.offset(place.focus.first_col);
    let mut sheet = Dom::create_div().with_id(ids::SHEET).with_css(format!(
        "position: absolute; left: {:.1}px; top: 0px; right: 0px; bottom: 0px;",
        -offset
    ));
    for i in grid.built(place.focus.first_col, tiles.len()) {
        let focused = !place.focus.on_views && i == place.focus.index;
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
    let folder = s.folders.of(shelf).display().to_string();
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
            "AzPlayer reads {kind} from {folder}, which is not there. Choose another folder \
             with --{}-dir.",
            match shelf {
                Shelf::Music => "music",
                Shelf::Pictures => "pictures",
                Shelf::Videos => "videos",
                Shelf::Tv => "tv",
            }
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
        .with_css(format!(
            "position: absolute; left: {x:.1}px; top: {y:.1}px; width: {aw}px; height: {:.1}px; \
             cursor: pointer; -azul-animation-in: azp-fade-in 260ms ease-out; \
             -azul-animation-out: azp-tile-out 140ms ease-in;",
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
        .with_attribute(AttributeType::Autofocus)
        .with_css("position: absolute; left: 60px; top: 104px; width: 460px;");
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
        .with_child(text(
            "open an address",
            &format!(
                "position: absolute; left: 56px; top: 30px; {} color: {};",
                look::PAGE_TITLE,
                look::INK
            ),
            stage,
        ))
        .with_child(
            Dom::create_div()
                .with_css(
                    "position: absolute; left: 60px; top: 120px; display: flex; flex-direction: \
                     row; align-items: center;",
                )
                .with_child(field)
                .with_child(act_part(play, app, Act::OpenAddress, "Play the address")),
        )
        .with_child(text(
            "An MP4 or MOV video (H.264) on a web server plays while it downloads; the sound \
             starts with the picture.",
            &format!(
                "position: absolute; left: 62px; top: 186px; right: 60px; font-size: 18px; \
                 font-weight: 300; color: {};",
                look::INK_DIM
            ),
            stage,
        ))
        .with_child(
            Dom::create_div()
                .with_css("position: absolute; left: 54px; top: 236px;")
                .with_child(act_part(
                    sample.with_id(ids::ADDRESS_SAMPLE),
                    app,
                    Act::Sample,
                    "Play the sample video, Big Buck Bunny",
                )),
        )
}

// ==== Now playing ====

/// Now playing: the cover large, the song, the album, the seek bar between the times, what
/// comes next; the transport bottom right (it hides while the music plays and the pointer
/// rests).
fn now_playing_page(s: &Player, app: &RefAny, stage: Stage) -> Dom {
    let mut page = Dom::create_div();
    let Some(music) = s.music.as_ref() else {
        page.add_child(text(
            "now playing",
            &format!("position: absolute; left: 56px; top: 30px; {}", look::PAGE_TITLE),
            stage,
        ));
        page.add_child(text(
            "Nothing is playing. Choose music library or play all on the start screen.",
            &format!(
                "position: absolute; left: 62px; top: 130px; font-size: 22px; font-weight: 300; \
                 color: {};",
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

/// The transport's buttons: for music, shuffle · previous · rewind · play / pause · forward ·
/// next · stop and the volume; for a video stop · from the start · rewind · play / pause ·
/// forward and the volume; for the slide show previous · play / pause · next and the music.
fn transport(s: &Player, app: &RefAny, what: Shelf, stage: Stage) -> Dom {
    let playing = match what {
        Shelf::Music => s.music.as_ref().is_some_and(Music::playing),
        Shelf::Pictures => s.viewer.playing,
        _ => s
            .video
            .as_ref()
            .is_some_and(|v| !v.paused && v.status.phase != VideoPhase::Ended),
    };
    let (play_icon, play_name) = if playing {
        ("pause", "Pause")
    } else {
        ("play_arrow", "Play")
    };
    let (mute_icon, mute_name) = if s.muted {
        ("volume_off", "Sound on")
    } else {
        ("volume_up", "Mute")
    };
    let b = |command: Command, icon_name: &str, name: &str, size: f32, id: AzString| {
        round_button(app, command, icon_name, name, size, id, stage)
    };
    let mut controls = Dom::create_div().with_id(ids::CONTROLS).with_css(
        "display: flex; flex-direction: row; align-items: center; flex-shrink: 0;",
    );
    match what {
        Shelf::Pictures => {
            controls.add_child(b(Command::Previous, "skip_previous", "Previous picture", 40.0, ids::PREVIOUS));
            controls.add_child(b(Command::SlideShow, play_icon, if playing { "Pause the slide show" } else { "Play the slide show" }, 54.0, ids::PLAY));
            controls.add_child(b(Command::Next, "skip_next", "Next picture", 40.0, ids::NEXT));
            controls.add_child(Dom::create_div().with_css("width: 22px; flex-shrink: 0;"));
            let music_name = if s.music.is_some() {
                "Stop the music"
            } else {
                "Music under the slide show"
            };
            controls.add_child(b(Command::SlideMusic, "library_music", music_name, 34.0, ids::MUSIC));
        }
        Shelf::Music => {
            controls.add_child(b(Command::Shuffle, "shuffle", "Shuffle", 34.0, ids::SHUFFLE));
            controls.add_child(b(Command::Previous, "skip_previous", "Previous song", 40.0, ids::PREVIOUS));
            controls.add_child(b(Command::Rewind, "fast_rewind", "Back 10 seconds", 40.0, ids::REWIND));
            controls.add_child(b(Command::PlayPause, play_icon, play_name, 56.0, ids::PLAY));
            controls.add_child(b(Command::Forward, "fast_forward", "Forward 30 seconds", 40.0, ids::FORWARD));
            controls.add_child(b(Command::Next, "skip_next", "Next song", 40.0, ids::NEXT));
            controls.add_child(b(Command::Stop, "stop", "Stop", 40.0, ids::STOP));
        }
        _ => {
            controls.add_child(b(Command::Stop, "stop", "Stop", 40.0, ids::STOP));
            controls.add_child(b(Command::Restart, "skip_previous", "From the start", 40.0, ids::RESTART));
            controls.add_child(b(Command::Rewind, "fast_rewind", "Back 10 seconds", 40.0, ids::REWIND));
            controls.add_child(b(Command::PlayPause, play_icon, play_name, 56.0, ids::PLAY));
            controls.add_child(b(Command::Forward, "fast_forward", "Forward 30 seconds", 40.0, ids::FORWARD));
        }
    }
    if what != Shelf::Pictures {
        controls.add_child(Dom::create_div().with_css("width: 22px; flex-shrink: 0;"));
        controls.add_child(b(Command::Mute, mute_icon, mute_name, 34.0, ids::MUTE));
        controls.add_child(b(Command::VolumeDown, "remove", "Volume down", 34.0, ids::VOLUME_DOWN));
        controls.add_child(b(Command::VolumeUp, "add", "Volume up", 34.0, ids::VOLUME_UP));
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
    let title_row = if s.fullscreen { 0.0 } else { 32.0 };
    let (w, h) = (s.window.0.max(100.0), (s.window.1 - title_row).max(100.0));
    let full = s.art.get(&full_key(path)).cloned().flatten();
    let thumb = s.art.get(&thumb_key(path)).cloned().flatten();
    let mut slide = Dom::create_div()
        .with_id(ids::id(&format!("slide-{}", viewer.shown)))
        .with_css(format!(
            "{} {}",
            look::FILL,
            if viewer.playing {
                look::slide_motion(viewer.shown, media::SLIDE_S)
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

/// The top strip over the picture: back and the orb on the left, the title, fullscreen on the
/// right.
fn top_strip(s: &Player, video: &VideoSession, app: &RefAny) -> Dom {
    let stage = Curtain::Closed.stage();
    let (full_icon, full_name) = if s.fullscreen {
        ("fullscreen_exit", "Leave fullscreen")
    } else {
        ("fullscreen", "Fullscreen")
    };
    Dom::create_div()
        .with_id(ids::TOP)
        .with_marker(OptionString::Some(ids::TOP))
        .with_css(format!(
            "position: absolute; left: 0px; top: 0px; right: 0px; height: 72px; display: flex; \
             flex-direction: row; align-items: center; padding: 0px 24px 0px 16px; background: \
             linear-gradient(to bottom, rgba(0, 0, 0, 0.75), rgba(0, 0, 0, 0)); opacity: {}; \
             animation: opacity 300ms ease-out;",
            if s.controls_shown { 1 } else { 0 }
        ))
        .with_child(round_button(app, Command::Back, "arrow_back", "Back", 40.0, ids::BACK, stage))
        .with_child(orb(app, 40.0, 10.0, stage))
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
            full_icon,
            full_name,
            40.0,
            ids::FULLSCREEN,
            stage,
        ))
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
