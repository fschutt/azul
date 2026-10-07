//! The window, in the look of Windows Media Center: the deep blue ground, the light, thin text,
//! the round glass buttons with a glow under the pointer, the green orb.
//!
//! - THE LIBRARY (nothing playing): the orb, "videos" in big thin letters, and the recent files
//!   as a gallery of tiles (the first tile opens a file); each tile resumes where it was left.
//! - THE STAGE (a file open): the picture fills the window, black around it. The CHROME lies over
//!   the picture - a top strip (back, the orb, fullscreen) and a bottom strip (the seek bar with
//!   the times, the title, the transport: stop, from the start, back 10 s, play / pause, forward
//!   30 s, and the volume: mute, down, up) - and is shown and hidden IN PLACE (`app.rs`), so the
//!   video box never changes size and the window is not rebuilt while the video plays.
//! - THE MENU BAR (the native one on macOS and Windows): Media, Playback, Audio, Video, View.
//!
//! Icons everywhere, no text on the buttons: each one is named for assistive technology.

use azul::{
    callbacks::SeekBarOnSeekCallbackType,
    dom::TabIndex,
    image::RawImageFormat,
    menu::{Menu, MenuItem, StringMenuItem},
    option::{OptionColorU, OptionString},
    prelude::*,
    shells::{MediaShell, ShellThemeAccent, ShellThemeScope},
    str::String as AzString,
    video::{VideoConfig, VideoPhase, VideoSource},
    widgets::{SeekBar, Titlebar, VideoWidget},
};
use azul_appkit::ui as kit;

use crate::{
    app::{self, Command, CommandRef, Player, RecentPick, SPEC},
    ids,
};

// ==== The look ====

/// The column every screen fills.
const COLUMN: &str = "display: flex; flex-direction: column; flex-grow: 1; min-height: 0px;";
/// The ground: Media Center's deep blue, lighter at the top.
const GROUND: &str = "background: linear-gradient(to bottom, #1c4f91, #0b2858, #04122b);";
/// The top of the ground (the title row over the library continues it).
const GROUND_TOP: (u8, u8, u8) = (0x1c, 0x4f, 0x91);
// A `:hover` block in `with_css` reaches every ELEMENT of the node's subtree that is hovered (the
// bare declarations stay on the node itself). So the face that lights up is always an ICON - its
// subtree is the glyph's text, which no `*` rule styles - and never a box with element children:
// the glow cannot leak onto a label or a nested box.

/// A round glass button's face (on its icon): a white rim, a faint shine, a blue glow under the
/// pointer.
const ROUND: &str = "display: flex; align-items: center; justify-content: center; \
     box-sizing: border-box; flex-shrink: 0; cursor: pointer; color: #ffffff; border: 2px solid \
     rgba(255, 255, 255, 0.8); background: linear-gradient(to bottom, rgba(255, 255, 255, 0.28), \
     rgba(255, 255, 255, 0.04)); :hover { border: 2px solid #ffffff; background: \
     linear-gradient(to bottom, #7cc8ff, #1d6fd0); box-shadow: 0px 0px 12px rgba(95, 180, 255, \
     0.9); }";
/// The green orb (Media Center's start button), drawn with a play mark.
const ORB: &str = "display: flex; align-items: center; justify-content: center; flex-shrink: 0; \
     border: 2px solid rgba(255, 255, 255, 0.85); background: linear-gradient(to bottom, \
     #8fe063, #2f9a1c);";
/// A gallery tile: the picture over the title.
const TILE: &str = "display: flex; flex-direction: column; width: 200px; margin: 0px 18px 18px \
     0px; cursor: pointer;";
/// A tile's picture (on its icon): lit up and glowing under the pointer.
const TILE_ART: &str = "display: flex; align-items: center; justify-content: center; \
     box-sizing: border-box; height: 112px; border-radius: 4px; font-size: 48px; color: \
     rgba(255, 255, 255, 0.88); border: 2px solid rgba(255, 255, 255, 0.25); background: \
     linear-gradient(to bottom, #2b5f9e, #102e5c); :hover { border: 2px solid #ffffff; \
     background: linear-gradient(to bottom, #3d7cc4, #164384); box-shadow: 0px 0px 14px \
     rgba(110, 185, 255, 0.85); }";
/// A panel over the picture (the OSD, a note).
const PANEL: &str = "position: absolute; margin: 0px; padding: 10px 18px; border-radius: 6px; \
     font-size: 16px; color: #ffffff; background: rgba(4, 18, 43, 0.78); border: 1px solid \
     rgba(255, 255, 255, 0.35);";

/// `visibility` for a part shown or hidden in place.
fn visibility(shown: bool) -> &'static str {
    if shown {
        "visibility: visible;"
    } else {
        "visibility: hidden;"
    }
}

// ==== The window ====

/// The window: the screen in the theme scope; the menu bar, the keys and the pointer on the body.
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
        column.add_child(if s.file.is_some() {
            stage(s, &app)
        } else {
            library(s, &app)
        });
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
        app::on_key,
    )
    .with_callback(
        EventFilter::Hover(HoverEventFilter::MouseMove),
        app,
        app::on_pointer,
    )
}

/// The title row (the window is `NoTitle`): the ground's blue over the library, black over the
/// picture, the title in light ink.
fn title_row(s: &Player) -> Dom {
    let file = s.title();
    let title = if file.is_empty() {
        SPEC.name.to_string()
    } else {
        format!("{file} - {}", SPEC.name)
    };
    let ground = if s.file.is_some() {
        ColorU::rgb(0, 0, 0)
    } else {
        ColorU::rgb(GROUND_TOP.0, GROUND_TOP.1, GROUND_TOP.2)
    };
    let mut bar = Titlebar::create(title.as_str())
        .with_background(ground)
        .with_background_inactive(ground)
        .without_border_bottom();
    bar.title_color = ColorU::rgb(0xe6, 0xf0, 0xff);
    bar.title_color_inactive = OptionColorU::Some(ColorU::rgb(0x9a, 0xb4, 0xd6));
    bar.dom()
}

/// A round glass button doing `command`: an icon, named `name` for assistive technology.
fn round_button(
    app: &RefAny,
    command: Command,
    icon: &str,
    name: &str,
    size: f32,
    id: AzString,
) -> Dom {
    let icon_px = (size * 0.5).round();
    let radius = size / 2.0;
    // The box takes the click, the focus and the name; its icon is the face.
    Dom::create_div()
        .with_id(id)
        .with_css(format!(
            "display: flex; flex-shrink: 0; width: {size}px; height: {size}px; margin-left: 8px; \
             cursor: pointer;"
        ))
        .with_tab_index(TabIndex::Auto)
        .with_accessibility_name(name)
        .with_callback(
            EventFilter::Hover(HoverEventFilter::Click),
            RefAny::new(CommandRef {
                app: app.clone(),
                command,
            }),
            app::on_command,
        )
        .with_child(Dom::create_icon(icon).with_css(format!(
            "{ROUND} width: {size}px; height: {size}px; border-radius: {radius}px; font-size: \
             {icon_px}px;"
        )))
}

/// The green orb, `size` px, `margin_left` px from what is before it.
fn orb(size: f32, margin_left: f32) -> Dom {
    let radius = size / 2.0;
    let icon_px = (size * 0.6).round();
    Dom::create_div()
        .with_css(format!(
            "{ORB} width: {size}px; height: {size}px; border-radius: {radius}px; margin-left: \
             {margin_left}px;"
        ))
        .with_child(Dom::create_icon("play_arrow").with_css(format!(
            "font-size: {icon_px}px; color: #ffffff;"
        )))
}

/// A line of text in a block of its own, the text node marked `marker` (rewritten in place).
fn live_text(text: String, marker: AzString, css: &str) -> Dom {
    Dom::create_div().with_css(css).with_child(
        Dom::create_text_do_not_use_without_block_level_wrapper(text)
            .with_marker(OptionString::Some(marker)),
    )
}

// ==== The stage ====

/// The stage: the video filling it (black around it), the chrome, the OSD and a note over it.
fn stage(s: &Player, app: &RefAny) -> Dom {
    let path = s.file.clone().unwrap_or_default();
    let config = VideoConfig {
        source: VideoSource::File(AzString::from(path.as_str())),
        timestamp: s.seek_s,
        autoplay: true,
        looping: false,
        paused: s.paused,
        // NV12 (4:2:0 in two planes) as the decoder makes it: 1.5 bytes a pixel against BGRA's
        // 4 through the worker's scaler, the write-back's copy and the upload, and the GPU's YUV
        // shader converts it as it draws (the CPU renderer converts the rows it paints). The
        // frame's own matrix and range travel with it; the Rec.709 here only says "NV12".
        output_format: RawImageFormat::NV12Rec709Video,
    };
    let video = VideoWidget::create(config)
        .with_on_status(app.clone(), app::on_video_status)
        .dom()
        .with_id(ids::VIDEO)
        .with_css("width: 100%; height: 100%;")
        .with_callback(
            EventFilter::Hover(HoverEventFilter::DoubleClick),
            app.clone(),
            app::on_video_double_click,
        );
    let mut stage = Dom::create_div()
        .with_id(ids::STAGE)
        .with_css(
            "position: relative; display: flex; flex-grow: 1; min-height: 0px; \
             background-color: #000000; overflow: hidden;",
        )
        .with_child(video);
    let note = match s.status.phase {
        VideoPhase::Loading => Some(String::from("Opening\u{2026}")),
        VideoPhase::Failed => Some(format!(
            "This video does not play here: {}",
            s.status.message.as_str()
        )),
        _ => None,
    };
    if let Some(note) = note {
        stage.add_child(
            Dom::create_p_with_text(note.as_str())
                .with_id(ids::NOTE)
                .with_css(format!("{PANEL} left: 32px; top: 88px;")),
        );
    }
    // The OSD is always there, shown and hidden in place.
    let osd_text = s
        .osd
        .as_ref()
        .map_or_else(|| String::from("\u{a0}"), |(text, _)| text.clone());
    stage.add_child(
        live_text(
            osd_text,
            ids::OSD_TEXT,
            &format!(
                "{PANEL} top: 88px; right: 32px; {}",
                visibility(s.osd.is_some())
            ),
        )
        .with_id(ids::OSD)
        .with_marker(OptionString::Some(ids::OSD)),
    );
    stage.add_child(top_strip(s, app));
    stage.add_child(bottom_strip(s, app));
    stage
}

/// The top strip over the picture: back and the orb on the left, fullscreen on the right.
fn top_strip(s: &Player, app: &RefAny) -> Dom {
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
             linear-gradient(to bottom, rgba(0, 0, 0, 0.75), rgba(0, 0, 0, 0)); {}",
            visibility(s.controls_shown)
        ))
        .with_child(round_button(
            app,
            Command::Library,
            "arrow_back",
            "Back to the library",
            40.0,
            ids::BACK,
        ))
        .with_child(orb(40.0, 10.0))
        .with_child(Dom::create_div().with_css("flex-grow: 1;"))
        .with_child(round_button(
            app,
            Command::Fullscreen,
            full_icon,
            full_name,
            40.0,
            ids::FULLSCREEN,
        ))
}

/// The bottom strip over the picture: the seek bar between the times; the title on the left of
/// the transport and the volume.
fn bottom_strip(s: &Player, app: &RefAny) -> Dom {
    let position = f64::from(s.status.position_s).max(0.0);
    let duration = f64::from(s.status.duration_s).max(0.0);
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
                .with_on_seek(app.clone(), app::on_seek as SeekBarOnSeekCallbackType)
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
    let playing = !s.paused && s.status.phase != VideoPhase::Ended;
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
    let transport = Dom::create_div()
        .with_id(ids::CONTROLS)
        .with_css("display: flex; flex-direction: row; align-items: center; flex-shrink: 0;")
        .with_child(round_button(app, Command::Stop, "stop", "Stop", 40.0, ids::STOP))
        .with_child(round_button(
            app,
            Command::Restart,
            "skip_previous",
            "From the start",
            40.0,
            ids::RESTART,
        ))
        .with_child(round_button(
            app,
            Command::Rewind,
            "fast_rewind",
            "Back 10 seconds",
            40.0,
            ids::REWIND,
        ))
        .with_child(round_button(
            app,
            Command::PlayPause,
            play_icon,
            play_name,
            58.0,
            ids::PLAY,
        ))
        .with_child(round_button(
            app,
            Command::Forward,
            "fast_forward",
            "Forward 30 seconds",
            40.0,
            ids::FORWARD,
        ))
        .with_child(Dom::create_div().with_css("width: 28px; flex-shrink: 0;"))
        .with_child(round_button(app, Command::Mute, mute_icon, mute_name, 34.0, ids::MUTE))
        .with_child(round_button(
            app,
            Command::VolumeDown,
            "remove",
            "Volume down",
            34.0,
            ids::VOLUME_DOWN,
        ))
        .with_child(round_button(
            app,
            Command::VolumeUp,
            "add",
            "Volume up",
            34.0,
            ids::VOLUME_UP,
        ));
    let control_row = Dom::create_div()
        .with_css("display: flex; flex-direction: row; align-items: center; margin-top: 12px;")
        .with_child(
            Dom::create_p_with_text(s.title().as_str())
                .with_id(ids::TITLE)
                .with_css(
                    "flex-grow: 1; min-width: 0px; margin: 0px 16px 0px 0px; font-size: 22px; \
                     font-weight: 300; color: #ffffff; white-space: nowrap; overflow: hidden;",
                ),
        )
        .with_child(transport);
    Dom::create_div()
        .with_id(ids::BAR)
        .with_marker(OptionString::Some(ids::BAR))
        .with_css(format!(
            "position: absolute; left: 0px; right: 0px; bottom: 0px; display: flex; \
             flex-direction: column; padding: 32px 28px 18px 28px; background: \
             linear-gradient(to bottom, rgba(0, 0, 0, 0), rgba(0, 0, 0, 0.85)); {}",
            visibility(s.controls_shown)
        ))
        .with_child(seek_row)
        .with_child(control_row)
}

// ==== The library ====

/// The library: the orb, "videos", and the gallery - "Open" first, then the recent files, each
/// with how far it was watched.
fn library(s: &Player, app: &RefAny) -> Dom {
    let mut gallery = Dom::create_div()
        .with_css("display: flex; flex-direction: row; flex-wrap: wrap; align-items: flex-start;");
    gallery.add_child(
        Dom::create_div()
            .with_id(ids::OPEN)
            .with_css(TILE)
            .with_tab_index(TabIndex::Auto)
            .with_accessibility_name("Open a video")
            .with_callback(
                EventFilter::Hover(HoverEventFilter::Click),
                RefAny::new(CommandRef {
                    app: app.clone(),
                    command: Command::Open,
                }),
                app::on_command,
            )
            .with_child(Dom::create_icon("folder_open").with_css(TILE_ART))
            .with_child(
                Dom::create_p_with_text("open a video")
                    .with_css("margin: 8px 0px 0px 0px; font-size: 15px; color: #ffffff;"),
            ),
    );
    for entry in &s.history.entries {
        let resume = s.history.resume_at(&entry.path);
        let label = if resume > 0.0 {
            format!("resume at {}", SeekBar::media_time(resume).as_str())
        } else {
            String::from("play")
        };
        let title = entry.title();
        #[allow(clippy::cast_possible_truncation)]
        let watched = (entry.progress() * 100.0).round() as u32;
        gallery.add_child(
            Dom::create_div()
                .with_class(ids::TILE)
                .with_css(TILE)
                .with_tab_index(TabIndex::Auto)
                .with_accessibility_name(format!("Play {title}"))
                .with_callback(
                    EventFilter::Hover(HoverEventFilter::Click),
                    RefAny::new(RecentPick {
                        app: app.clone(),
                        path: entry.path.clone(),
                    }),
                    app::on_recent,
                )
                .with_child(Dom::create_icon("movie").with_css(TILE_ART))
                .with_child(
                    Dom::create_div()
                        .with_css(
                            "height: 3px; margin-top: 6px; background: rgba(255, 255, 255, \
                             0.2);",
                        )
                        .with_child(Dom::create_div().with_css(format!(
                            "height: 3px; width: {watched}%; background: #5fb4ff;"
                        ))),
                )
                .with_child(Dom::create_p_with_text(title.as_str()).with_css(
                    "margin: 6px 0px 0px 0px; font-size: 15px; color: #ffffff; white-space: \
                     nowrap; overflow: hidden;",
                ))
                .with_child(
                    Dom::create_p_with_text(label.as_str())
                        .with_css("margin: 2px 0px 0px 0px; font-size: 12px; color: #9cc9f5;"),
                ),
        );
    }
    let mut column = Dom::create_div()
        .with_id(ids::LIBRARY)
        .with_css(format!(
            "{COLUMN} {GROUND} overflow-y: auto; padding: 28px 56px; color: #ffffff;"
        ))
        .with_child(
            Dom::create_div()
                .with_css("display: flex; flex-direction: row; align-items: center;")
                .with_child(orb(48.0, 0.0))
                .with_child(Dom::create_p_with_text(SPEC.name).with_css(
                    "margin: 0px 0px 0px 14px; font-size: 16px; color: rgba(255, 255, 255, \
                     0.8);",
                )),
        )
        .with_child(Dom::create_p_with_text("videos").with_css(
            "margin: 22px 0px 18px 0px; font-size: 52px; font-weight: 300; color: rgba(255, \
             255, 255, 0.95);",
        ));
    column.add_child(gallery);
    if s.history.entries.is_empty() {
        column.add_child(
            Dom::create_p_with_text("No videos yet")
                .with_css("margin: 8px 0px 0px 0px; font-size: 18px; color: #cfe3ff;"),
        );
        column.add_child(
            Dom::create_p_with_text("MP4 or MOV - H.264 picture, AAC sound.").with_css(
                "margin: 4px 0px 0px 0px; font-size: 13px; color: rgba(207, 227, 255, 0.7);",
            ),
        );
    }
    column
}

// ==== The menu bar ====

/// The menu bar: Media, Playback, Audio, Video, View.
fn menu_bar(s: &Player, app: &RefAny) -> Menu {
    let item = |label: &str, command: Command| {
        MenuItem::string(StringMenuItem::create(label).with_callback(
            RefAny::new(CommandRef {
                app: app.clone(),
                command,
            }),
            app::on_command,
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
                RefAny::new(RecentPick {
                    app: app.clone(),
                    path: e.path.clone(),
                }),
                app::on_recent,
            ))
        })
        .collect();
    let mut media = vec![item("Open File\u{2026}", Command::Open)];
    if !recent.is_empty() {
        media.push(menu("Open Recent", recent));
    }
    media.push(MenuItem::separator());
    media.push(item("Library", Command::Library));
    media.push(item("Settings\u{2026}", Command::Settings));
    Menu::create(vec![
        menu("Media", media),
        menu(
            "Playback",
            vec![
                item("Play / Pause", Command::PlayPause),
                item("Stop", Command::Stop),
                MenuItem::separator(),
                item("From the Start", Command::Restart),
                item("Back 10 Seconds", Command::Rewind),
                item("Forward 30 Seconds", Command::Forward),
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
            "Video",
            vec![item(
                if s.fullscreen {
                    "Leave Fullscreen"
                } else {
                    "Fullscreen"
                },
                Command::Fullscreen,
            )],
        ),
        menu("View", vec![item("Library", Command::Library)]),
    ])
}
