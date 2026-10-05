//! The Video card: Big Buck Bunny in the `<video>` widget.
//!
//! The widget does the work. It downloads the clip, decodes it on a
//! background thread, shows the frames, and reports what it is doing through
//! its `on_status` hook: loading, the first frame on screen (held as a poster,
//! because the card asks for `autoplay: false`), the position about four
//! times a second while it plays, the end, or why it cannot play. The card
//! keeps the last report and draws its controls from it.
//!
//! Play and pause are the widget's `paused` flag and a seek is its
//! `timestamp`: the card changes them and returns `RefreshDom`, and the
//! widget passes the change on to its decoder when it is rebuilt.

use azul::{
    dom::{AccessibilityInfo, AccessibilityRole, OnVideoStatusCallback, TabIndex, VirtualKeyCode},
    image::RawImageFormat,
    option::{OptionCursorNodePosition, OptionLogicalRect, OptionVirtualKeyCode},
    prelude::*,
    url::Url,
    video::{VideoConfig, VideoPhase, VideoSource, VideoStatus},
    widgets::{UiTheme, VideoWidget},
};

/// Big Buck Bunny: 10 s of 640 x 360 H.264 in an MP4, the clip
/// `examples/c/video.c` plays.
const BBB_HOST: &str = "test-videos.co.uk";
const BBB_PATH: &str = "/vids/bigbuckbunny/mp4/h264/360/Big_Buck_Bunny_360_10s_2MB.mp4";

/// How far Left and Right on the focused seek bar move, in seconds.
const SEEK_STEP_S: f32 = 1.0;

// Every colour here is a `system:` colour, like the rest of the page: the
// stage is the field surface, the play badge the desktop's accent, the notes
// the card surface - light and dark with one value each. The stage is the
// clip's own 16:9, so the decoder scales the frames without stretching them.

/// The video's frame: the widget fills it, the overlay lies on top of it.
const STAGE_CSS: &str =
    "position: relative; width: 480px; height: 270px; border-radius: 8px; overflow: hidden; \
     background-color: system:control-background;";
/// The overlay over the whole video is a button: a click anywhere on the
/// video plays or pauses it, and so do Space and Enter once it has focus.
const OVERLAY_CSS: &str =
    "position: absolute; top: 0px; left: 0px; width: 480px; height: 270px; display: flex; \
     align-items: center; justify-content: center; padding: 0px; border-width: 0px; cursor: \
     pointer;";
/// The round play button over the poster.
const BADGE_CSS: &str =
    "display: flex; align-items: center; justify-content: center; width: 72px; height: 72px; \
     border-radius: 36px; background-color: system:accent;";
const BADGE_ICON_CSS: &str = "font-size: 44px; color: system:accent-text;";
/// A note over the video: loading, or why it cannot play.
const NOTE_CSS: &str =
    "display: flex; flex-direction: column; align-items: center; max-width: 400px; padding: \
     12px 16px; border-radius: 8px; background-color: system:window-background;";
const NOTE_TITLE_CSS: &str =
    "font-size: 14px; font-weight: bold; color: system:text; margin-bottom: 4px;";
const NOTE_DETAIL_CSS: &str =
    "font-size: 12px; color: system:secondary-text; text-align: center; margin: 0px;";
/// The row under the video: play/pause, the time, the seek bar.
const CONTROLS_CSS: &str =
    "display: flex; flex-direction: row; align-items: center; gap: 10px; width: 480px; \
     margin-top: 8px;";
const TOGGLE_CSS: &str =
    "display: flex; align-items: center; justify-content: center; width: 32px; height: 32px; \
     padding: 0px; border-width: 0px; border-radius: 16px; background-color: \
     system:button-face; cursor: pointer;";
const TOGGLE_ICON_CSS: &str = "font-size: 22px; color: system:button-text;";
const TIME_CSS: &str =
    "font-size: 12px; color: system:secondary-text; font-family: system:monospace;";
/// The seek bar: a track with the played part filled in, which a click (or
/// an arrow key, once it has focus) moves. Taller than the track, so it is
/// easy to hit.
const SEEK_CSS: &str =
    "flex-grow: 1; display: flex; flex-direction: column; justify-content: center; height: 24px; \
     cursor: pointer;";
const TRACK_CSS: &str =
    "display: flex; flex-direction: row; height: 6px; border-radius: 3px; overflow: hidden; \
     background-color: system:separator;";
/// The played part; its width is appended per layout.
const FILL_CSS: &str = "height: 6px; border-radius: 3px; background-color: system:accent;";

/// What the card remembers between layouts.
struct VideoCard {
    /// Asked of the widget: hold (`true`) or play. Starts held, so the first
    /// frame is a poster under the play button.
    paused: bool,
    /// Asked of the widget: the last seek target, in seconds.
    seek_s: f32,
    /// What the widget last reported.
    status: VideoStatus,
}

/// The card's state, which the page keeps between layouts.
pub fn new_state() -> RefAny {
    RefAny::new(VideoCard {
        paused: true,
        seek_s: 0.0,
        status: VideoStatus {
            message: "".into(),
            position_s: 0.0,
            duration_s: 0.0,
            phase: VideoPhase::Loading,
        },
    })
}

/// The Video card, drawn from the state [`new_state`] made. `theme` is the
/// page's widget theme: the widget's "no signal" poster is drawn in it.
pub fn card(state: &RefAny, theme: UiTheme) -> Dom {
    let mut state_ref = state.clone();
    let (paused, seek_s, status) = match state_ref.downcast_ref::<VideoCard>() {
        Some(c) => (c.paused, c.seek_s, c.status.clone()),
        None => return Dom::create_div(),
    };

    let config = VideoConfig {
        source: VideoSource::Url(Url::from_parts("https", BBB_HOST, 443, BBB_PATH)),
        timestamp: seek_s,
        autoplay: false,
        looping: false,
        paused,
        output_format: RawImageFormat::BGRA8,
    };
    let video = VideoWidget::create(config)
        .with_on_status(
            state.clone(),
            on_video_status,
        )
        .with_theme(theme)
        .dom()
        .with_css("width: 100%; height: 100%;");

    // What a press does, for a screen reader: the card's own request, so it
    // is right even before the widget has answered.
    let overlay_name = if !paused {
        "Pause video"
    } else if matches!(status.phase, VideoPhase::Ended) {
        "Play video again"
    } else if matches!(status.phase, VideoPhase::Failed) {
        "Try to play the video again"
    } else {
        "Play video"
    };
    let mut overlay = Dom::create_node(NodeType::Button)
        .with_accessibility_name(overlay_name)
        .with_css(OVERLAY_CSS);
    match status.phase {
        VideoPhase::Loading => overlay.add_child(note(
            "Loading Big Buck Bunny…",
            "Downloading the clip and decoding its first frame.",
        )),
        VideoPhase::Failed => overlay.add_child(note(
            "Could not play the video",
            &format!("{} Click to try again.", status.message.as_str()),
        )),
        VideoPhase::Paused => overlay.add_child(badge("play_arrow")),
        VideoPhase::Ended => overlay.add_child(badge("replay")),
        // While it plays the overlay is invisible, and still pauses on a
        // click.
        VideoPhase::Playing => {}
    }
    overlay.add_callback(
        EventFilter::Hover(HoverEventFilter::Click),
        state.clone(),
        on_toggle,
    );

    let stage = Dom::create_div()
        .with_css(STAGE_CSS)
        .with_child(video)
        .with_child(overlay);

    let mut toggle = Dom::create_node(NodeType::Button)
        .with_accessibility_name(if paused { "Play" } else { "Pause" })
        .with_css(TOGGLE_CSS)
        .with_child(
            Dom::create_icon(if paused { "play_arrow" } else { "pause" })
                .with_css(TOGGLE_ICON_CSS),
        );
    toggle.add_callback(
        EventFilter::Hover(HoverEventFilter::Click),
        state.clone(),
        on_toggle,
    );

    let time = Dom::create_span_with_text(time_text(&status)).with_css(TIME_CSS);

    // The fill's width is concatenated, not formatted, so no style string in
    // this file carries a placeholder (the theme check parses them all).
    let played = format!("{:.2}", progress(&status) * 100.0);
    let fill_css = [FILL_CSS, " width: ", played.as_str(), "%;"].concat();
    // A seek bar IS a slider (the arrow keys move it), and its value is the
    // position a screen reader reads out.
    let mut seek_bar = Dom::create_div()
        .with_css(SEEK_CSS)
        .with_tab_index(TabIndex::Auto)
        .with_accessibility_info(
            AccessibilityInfo::named("Seek", AccessibilityRole::Slider)
                .with_value(time_text(&status)),
        )
        .with_child(
            Dom::create_div()
                .with_css(TRACK_CSS)
                .with_child(Dom::create_div().with_css(fill_css.as_str())),
        );
    seek_bar.add_callback(
        EventFilter::Hover(HoverEventFilter::Click),
        state.clone(),
        on_seek_click,
    );
    seek_bar.add_callback(
        EventFilter::Focus(FocusEventFilter::VirtualKeyDown),
        state.clone(),
        on_seek_key,
    );

    let controls = Dom::create_div()
        .with_css(CONTROLS_CSS)
        .with_child(toggle)
        .with_child(time)
        .with_child(seek_bar);

    let player = Dom::create_div()
        .with_css("display: flex; flex-direction: column; align-items: flex-start;")
        .with_child(stage)
        .with_child(controls);

    super::section(
        "Video",
        vec![super::labelled(
            "Big Buck Bunny, played by the <video> widget",
            player,
        )],
    )
}

/// The round play button over the poster.
fn badge(icon: &str) -> Dom {
    Dom::create_div()
        .with_css(BADGE_CSS)
        .with_child(Dom::create_icon(icon).with_css(BADGE_ICON_CSS))
}

/// A note over the video: loading, or why it cannot play.
fn note(title: &str, detail: &str) -> Dom {
    Dom::create_div()
        .with_css(NOTE_CSS)
        .with_child(Dom::create_span_with_text(title).with_css(NOTE_TITLE_CSS))
        .with_child(Dom::create_p_with_text(detail).with_css(NOTE_DETAIL_CSS))
}

/// How far through the video the position is, `0.0..=1.0`; `0.0` while the
/// length is unknown.
fn progress(status: &VideoStatus) -> f32 {
    let p = if status.duration_s > 0.0 {
        status.position_s / status.duration_s
    } else {
        0.0
    };
    if p.is_finite() {
        p.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

/// `m:ss / m:ss`, with `-:--` for a length not known yet.
fn time_text(status: &VideoStatus) -> String {
    let total = if status.duration_s > 0.0 {
        clock(status.duration_s)
    } else {
        String::from("-:--")
    };
    format!("{} / {}", clock(status.position_s), total)
}

/// Seconds as `m:ss`.
fn clock(seconds: f32) -> String {
    let whole = if seconds.is_finite() && seconds > 0.0 {
        seconds as u32
    } else {
        0
    };
    format!("{}:{:02}", whole / 60, whole % 60)
}

/// The widget reported where the video stands: keep it, and redraw.
extern "C" fn on_video_status(mut data: RefAny, _: CallbackInfo, status: VideoStatus) -> Update {
    let Some(mut card) = data.downcast_mut::<VideoCard>() else {
        return Update::DoNothing;
    };
    // A video that stopped by itself - at its end, or on a failure - is held
    // now: the next press of play is a change of `paused` the widget passes
    // on, which plays it again or retries it.
    if matches!(status.phase, VideoPhase::Ended | VideoPhase::Failed) {
        card.paused = true;
    }
    card.status = status;
    Update::RefreshDom
}

/// Play or pause: the overlay over the video, and the button under it.
extern "C" fn on_toggle(mut data: RefAny, _: CallbackInfo) -> Update {
    match data.downcast_mut::<VideoCard>() {
        Some(mut card) => {
            card.paused = !card.paused;
            Update::RefreshDom
        }
        None => Update::DoNothing,
    }
}

/// A click on the seek bar: jump to that point of the video.
extern "C" fn on_seek_click(mut data: RefAny, info: CallbackInfo) -> Update {
    let OptionCursorNodePosition::Some(cursor) = info.get_cursor_relative_to_node() else {
        return Update::DoNothing;
    };
    let OptionLogicalRect::Some(rect) = info.get_hit_node_rect() else {
        return Update::DoNothing;
    };
    if !(rect.size.width > 0.0) {
        return Update::DoNothing;
    }
    let fraction = (cursor.x / rect.size.width).clamp(0.0, 1.0);
    seek(&mut data, |_, duration| fraction * duration)
}

/// Left and Right on the focused seek bar: a step back or forward.
extern "C" fn on_seek_key(mut data: RefAny, info: CallbackInfo) -> Update {
    let keyboard = info.get_current_keyboard_state();
    let step = match &keyboard.current_virtual_keycode {
        OptionVirtualKeyCode::Some(VirtualKeyCode::Left) => -SEEK_STEP_S,
        OptionVirtualKeyCode::Some(VirtualKeyCode::Right) => SEEK_STEP_S,
        _ => return Update::DoNothing,
    };
    seek(&mut data, |position, _| position + step)
}

/// Ask the widget to seek to `target(position, duration)`, clamped into the
/// video. There is nothing to seek in before the length is known.
fn seek(data: &mut RefAny, target: impl FnOnce(f32, f32) -> f32) -> Update {
    let Some(mut card) = data.downcast_mut::<VideoCard>() else {
        return Update::DoNothing;
    };
    let duration = card.status.duration_s;
    if !(duration > 0.0) {
        return Update::DoNothing;
    }
    let mut to = target(card.status.position_s, duration);
    if !to.is_finite() {
        return Update::DoNothing;
    }
    to = to.clamp(0.0, duration);
    // The widget seeks on a CHANGE of `timestamp`: nudge a repeat of the last
    // target, so a second click on the same spot still seeks.
    if (to - card.seek_s).abs() < 0.0005 {
        to = if to + 0.001 <= duration {
            to + 0.001
        } else {
            to - 0.001
        };
    }
    card.seek_s = to;
    Update::RefreshDom
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(position_s: f32, duration_s: f32) -> VideoStatus {
        VideoStatus {
            message: "".into(),
            position_s,
            duration_s,
            phase: VideoPhase::Playing,
        }
    }

    #[test]
    fn a_video_past_an_hour_shows_its_hours() {
        assert_eq!(time_text(&status(3725.0, 7322.0)), "1:02:05 / 2:02:02");
        assert_eq!(time_text(&status(72.0, 562.0)), "1:12 / 9:22");
    }
}
