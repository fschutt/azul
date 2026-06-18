//! AzMeet — a Google-Meet-style demo on the public `azul::` surface.
//!
//! Showcases the "heavy-stateful widget" pattern: the toolbar toggles flip
//! booleans in the app state and return `RefreshDom`, so the DOM **gains or loses**
//! a `CameraWidget` / `ScreenCaptureWidget` / `MicrophoneWidget` on each toggle —
//! and the widget's `AfterMount` then starts (or its `Drop` stops) the underlying
//! capture. The local user is a camera/screen tile; remote participants are grey
//! placeholders. Auto-joins a fake session (a generated "meeting link" hash).
//!
//! (Camera/screen tiles render their live frames on the GPU backend; on the CPU
//! backend they show the widget placeholder. Sending the captured media to remote
//! peers is the `WebTransport` follow-up — see doc/SUPER_PLAN_0.2.0.md.)

use azul::misc::{AudioConfig, CameraConfig, ScreenCaptureConfig};
use azul::prelude::*;
use azul::widgets::{CameraWidget, MicrophoneWidget, ScreenCaptureWidget};

struct MeetState {
    /// The fake "meeting link" (a generated hash), shown in the header.
    link: String,
    mic_on: bool,
    cam_on: bool,
    screen_on: bool,
}

const TILE: &str = "width: 300px; height: 200px; margin: 8px; border-radius: 10px; \
    background: #2b2b38; display: flex; align-items: center; justify-content: center; \
    color: #99a; font-size: 17px; overflow: hidden;";
const BTN: &str = "padding: 10px 18px; margin: 0 6px; border-radius: 8px; \
    background: #3a3a4a; color: #e6e6f0; font-size: 14px;";
const BTN_ON: &str = "padding: 10px 18px; margin: 0 6px; border-radius: 8px; \
    background: #2f6db0; color: #ffffff; font-size: 14px;";

fn participant(name: &str) -> Dom {
    Dom::create_div().with_css(TILE).with_child(Dom::create_text(name))
}

extern "C" fn layout(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    let (link, mic, cam, screen) = match data.downcast_ref::<MeetState>() {
        Some(s) => (s.link.clone(), s.mic_on, s.cam_on, s.screen_on),
        None => return Dom::create_body(),
    };

    // --- self tile: a live CameraWidget when on, else a grey placeholder ---
    let self_tile = if cam {
        Dom::create_div().with_css(TILE).with_child(
            CameraWidget::create(CameraConfig::default())
                .dom()
                .with_css("width: 100%; height: 100%;"),
        )
    } else {
        Dom::create_div()
            .with_css(TILE)
            .with_child(Dom::create_text("You · camera off"))
    };

    // --- video grid: self + (optional) screen-share + remote placeholders ---
    let mut grid = Dom::create_div().with_css(
        "display: flex; flex-wrap: wrap; flex-grow: 1; align-content: flex-start; \
         justify-content: center; padding: 12px;",
    );
    grid = grid.with_child(self_tile);
    if screen {
        grid = grid.with_child(Dom::create_div().with_css(TILE).with_child(
            ScreenCaptureWidget::create(ScreenCaptureConfig::default())
                .dom()
                .with_css("width: 100%; height: 100%;"),
        ));
    }
    grid = grid
        .with_child(participant("Alice"))
        .with_child(participant("Bob"))
        .with_child(participant("Carol"));

    // --- toolbar: mic / camera / screen toggles ---
    let toolbar = Dom::create_div()
        .with_css("display: flex; justify-content: center; padding: 14px; background: #15151c;")
        .with_child(
            Dom::create_div()
                .with_css(if mic { BTN_ON } else { BTN })
                .with_child(Dom::create_text(if mic { "Mute" } else { "Unmute mic" }))
                .with_callback(
                    EventFilter::Hover(HoverEventFilter::MouseUp),
                    data.clone(),
                    mic_toggle,
                ),
        )
        .with_child(
            Dom::create_div()
                .with_css(if cam { BTN_ON } else { BTN })
                .with_child(Dom::create_text(if cam { "Stop video" } else { "Start video" }))
                .with_callback(
                    EventFilter::Hover(HoverEventFilter::MouseUp),
                    data.clone(),
                    cam_toggle,
                ),
        )
        .with_child(
            Dom::create_div()
                .with_css(if screen { BTN_ON } else { BTN })
                .with_child(Dom::create_text(if screen { "Stop share" } else { "Share screen" }))
                .with_callback(
                    EventFilter::Hover(HoverEventFilter::MouseUp),
                    data.clone(),
                    screen_toggle,
                ),
        );

    let mut body = Dom::create_body().with_css(
        "display: flex; flex-direction: column; height: 100%; margin: 0; \
         background: #0e0e14; font-family: sans-serif; color: #e6e6f0;",
    );
    body = body.with_child(
        Dom::create_text(format!("AzMeet · meeting {}", link).as_str())
            .with_css("padding: 12px; font-size: 18px; background: #15151c;"),
    );
    // While unmuted, a (visually tiny) MicrophoneWidget captures audio — its
    // AfterMount starts the mic, its Drop (on RefreshDom when muted) stops it.
    if mic {
        body = body.with_child(
            MicrophoneWidget::create(AudioConfig {
                sample_rate: 48_000,
                channels: 1,
            })
            .dom()
            .with_css("width: 1px; height: 1px; overflow: hidden;"),
        );
    }
    body.with_child(grid).with_child(toolbar)
}

extern "C" fn mic_toggle(mut data: RefAny, _info: CallbackInfo) -> Update {
    if let Some(mut s) = data.downcast_mut::<MeetState>() {
        s.mic_on = !s.mic_on;
    }
    Update::RefreshDom
}
extern "C" fn cam_toggle(mut data: RefAny, _info: CallbackInfo) -> Update {
    if let Some(mut s) = data.downcast_mut::<MeetState>() {
        s.cam_on = !s.cam_on;
    }
    Update::RefreshDom
}
extern "C" fn screen_toggle(mut data: RefAny, _info: CallbackInfo) -> Update {
    if let Some(mut s) = data.downcast_mut::<MeetState>() {
        s.screen_on = !s.screen_on;
    }
    Update::RefreshDom
}

/// A fake "meeting link" hash (auto-join). Uses the wall clock so each launch
/// gets a distinct code (xxx-xxxx-xxx).
fn gen_link() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!(
        "{:03x}-{:04x}-{:03x}",
        (n & 0xfff) as u16,
        ((n >> 12) & 0xffff) as u16,
        ((n >> 28) & 0xfff) as u16,
    )
}

fn main() {
    let link = gen_link();
    eprintln!("[azmeet] joined meeting {link} (camera/mic/screen off — toggle in the toolbar)");
    let data = RefAny::new(MeetState {
        link,
        mic_on: false,
        cam_on: false,
        screen_on: false,
    });
    let config = AppConfig::create();
    let app = App::create(data, config);
    let window = WindowCreateOptions::create(layout);
    app.run(window);
}
