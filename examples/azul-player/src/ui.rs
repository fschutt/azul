//! The window: azul's MediaShell, the player variant - the stage (the video, the OSD over it) or the
//! library (the recent files), and the controls bar under it (hidden while the video plays and the
//! pointer rests; gone in fullscreen with them).

use azul::{
    callbacks::{
        ButtonOnClickCallbackType, MediaControlsOnActionCallbackType, SeekBarOnSeekCallbackType,
    },
    image::RawImageFormat,
    option::OptionString,
    prelude::*,
    shells::{MediaShell, ShellEmptyState, ShellThemeAccent, ShellThemeScope},
    str::String as AzString,
    video::{VideoConfig, VideoPhase, VideoSource},
    widgets::{Button, ButtonType, MediaControls, ProgressBar, SeekBar, VideoWidget},
};
use azul_appkit::ui as kit;

use crate::{
    app::{self, Player, RecentPick, SPEC},
    ids,
};

/// The window: the shell in the theme scope; the keys and the pointer on the body.
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
    let settings = kit::settings_open(&s.kit);
    let content = if settings {
        kit::settings_page(&s.kit, Vec::new())
    } else if s.file.is_some() {
        stage(s, &app)
    } else {
        library(s, &app)
    };
    let bar = if settings || !s.controls_shown {
        Dom::create_div()
    } else {
        controls_bar(s, &app)
    };
    let mut office = MediaShell::create_player(content, bar).office_shell();
    if !s.fullscreen {
        office = office.with_title_row(kit::title_row(&title(s)));
    }
    let column = Dom::create_div()
        .with_css("display: flex; flex-direction: column; flex-grow: 1; min-height: 0px;")
        .with_child(office.dom());
    ShellThemeScope::create(column)
        .with_accent(ShellThemeAccent::Leaf)
        .body()
        .with_callback(
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

/// The title row's text: the app, and the file playing.
fn title(s: &Player) -> String {
    let file = s.title();
    if file.is_empty() {
        SPEC.name.to_string()
    } else {
        format!("{file} - {}", SPEC.name)
    }
}

/// The stage: the video filling it (black around it), the OSD and a note over it.
fn stage(s: &Player, app: &RefAny) -> Dom {
    let path = s.file.clone().unwrap_or_default();
    let config = VideoConfig {
        source: VideoSource::File(AzString::from(path.as_str())),
        timestamp: s.seek_s,
        autoplay: true,
        looping: false,
        paused: s.paused,
        output_format: RawImageFormat::BGRA8,
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
        VideoPhase::Loading => Some(String::from("Opening...")),
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
                .with_css(
                    "position: absolute; left: 24px; bottom: 24px; margin: 0px; padding: 8px \
                     12px; border-radius: 6px; font-size: 13px; color: #ffffff; \
                     background-color: rgba(0, 0, 0, 0.6);",
                ),
        );
    }
    if let Some((text, _)) = &s.osd {
        stage.add_child(
            Dom::create_p_with_text(text.as_str())
                .with_id(ids::OSD)
                .with_css(
                    "position: absolute; top: 24px; right: 24px; margin: 0px; padding: 8px 14px; \
                     border-radius: 6px; font-size: 16px; color: #ffffff; \
                     background-color: rgba(0, 0, 0, 0.6);",
                ),
        );
    }
    stage
}

/// The library: "Open a video" and the recent files, each with how far it was watched.
fn library(s: &Player, app: &RefAny) -> Dom {
    let mut column = Dom::create_div().with_id(ids::LIBRARY).with_css(
        "display: flex; flex-direction: column; flex-grow: 1; min-height: 0px; overflow-y: auto; \
         padding: 16px;",
    );
    column.add_child(
        Button::with_type("Open a video...", ButtonType::Primary)
            .with_icon("folder_open")
            .with_on_click(app.clone(), app::on_open as ButtonOnClickCallbackType)
            .dom()
            .with_id(ids::OPEN)
            .with_css("align-self: flex-start; margin-bottom: 16px;"),
    );
    if s.history.entries.is_empty() {
        column.add_child(
            ShellEmptyState::create("No videos yet")
                .with_icon("movie")
                .with_detail(
                    "Open an MP4 or MOV file (H.264 picture, AAC sound). AzPlayer remembers \
                     where you stopped.",
                )
                .dom(),
        );
        return column;
    }
    column.add_child(
        Dom::create_p_with_text("Continue watching")
            .with_css("margin: 0px 0px 8px 0px; font-size: 12px; font-weight: 600; opacity: 0.7;"),
    );
    for entry in &s.history.entries {
        let resume = s.history.resume_at(&entry.path);
        let label = if resume > 0.0 {
            format!("Resume at {}", SeekBar::media_time(resume).as_str())
        } else {
            String::from("Play")
        };
        #[allow(clippy::cast_possible_truncation)]
        let progress = (entry.progress() * 100.0) as f32;
        column.add_child(
            Dom::create_div()
                .with_css(
                    "display: flex; flex-direction: row; align-items: center; padding: 8px 0px; \
                     border-bottom: 1px solid system:separator;",
                )
                .with_child(
                    Dom::create_div()
                        .with_css(
                            "display: flex; flex-direction: column; flex-grow: 1; min-width: 0px; \
                             margin-right: 12px;",
                        )
                        .with_child(
                            Dom::create_p_with_text(entry.title().as_str())
                                .with_css("margin: 0px 0px 4px 0px; font-size: 14px;"),
                        )
                        .with_child(
                            ProgressBar::create(progress)
                                .with_accessibility_name("Watched")
                                .dom()
                                .with_css("max-width: 320px;"),
                        ),
                )
                .with_child(
                    Button::with_type(label.as_str(), ButtonType::Default)
                        .with_icon("play_arrow")
                        .with_on_click(
                            RefAny::new(RecentPick {
                                app: app.clone(),
                                path: entry.path.clone(),
                            }),
                            app::on_recent as ButtonOnClickCallbackType,
                        )
                        .dom(),
                ),
        );
    }
    column
}

/// The controls bar: the title, the transport (with the skips and the volume), the seek bar,
/// fullscreen, back to the library.
fn controls_bar(s: &Player, app: &RefAny) -> Dom {
    let mut bar = Dom::create_div().with_id(ids::BAR).with_css(
        "display: flex; flex-direction: row; align-items: center; padding: 6px 12px; flex-grow: 1;",
    );
    if s.file.is_none() {
        bar.add_child(
            Dom::create_p_with_text("Nothing playing")
                .with_id(ids::TITLE)
                .with_css("margin: 0px; font-size: 13px; opacity: 0.7;"),
        );
        return bar;
    }
    bar.add_child(
        Dom::create_p_with_text(s.title().as_str())
            .with_id(ids::TITLE)
            .with_css(
                "margin: 0px 12px 0px 0px; font-size: 13px; font-weight: 600; width: 180px; \
                 white-space: nowrap; overflow: hidden; flex-shrink: 0;",
            ),
    );
    let playing = !s.paused && s.status.phase != VideoPhase::Ended;
    bar.add_child(
        MediaControls::create(playing)
            .with_show_skip(true)
            .with_volume(if s.muted { 0.0 } else { s.volume })
            .with_accessibility_name("Player")
            .with_on_action(
                app.clone(),
                app::on_controls as MediaControlsOnActionCallbackType,
            )
            .dom()
            .with_id(ids::CONTROLS),
    );
    bar.add_child(
        SeekBar::create(
            f64::from(s.status.position_s),
            f64::from(s.status.duration_s),
        )
        .with_accessibility_name("Position in the video")
        .with_on_seek(app.clone(), app::on_seek as SeekBarOnSeekCallbackType)
        .dom()
        .with_id(ids::SEEK)
        .with_marker(OptionString::Some(ids::SEEK))
        .with_css("flex-grow: 1; margin: 0px 12px;"),
    );
    let mut full = Button::with_type("", ButtonType::Link).with_icon(if s.fullscreen {
        "fullscreen_exit"
    } else {
        "fullscreen"
    });
    full.alt = AzString::from(if s.fullscreen {
        "Leave fullscreen"
    } else {
        "Fullscreen"
    });
    bar.add_child(
        full.with_on_click(app.clone(), app::on_fullscreen as ButtonOnClickCallbackType)
            .dom()
            .with_id(ids::FULLSCREEN),
    );
    let mut back = Button::with_type("", ButtonType::Link).with_icon("video_library");
    back.alt = AzString::from("Back to the library");
    bar.add_child(
        back.with_on_click(app.clone(), app::on_close_file as ButtonOnClickCallbackType)
            .dom(),
    );
    bar
}
