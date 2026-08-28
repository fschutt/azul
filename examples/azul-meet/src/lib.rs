//! AzMeet — a Google-Meet-style demo on the public `azul::` surface.
//!
//! Showcases the "heavy-stateful widget" pattern: the toolbar toggles flip
//! booleans in the app state and return `RefreshDom`, so the DOM **gains or loses**
//! a `CameraWidget` / `ScreenCaptureWidget` / `MicrophoneWidget` on each toggle —
//! and the widget's `AfterMount` then starts (or its `Drop` stops) the underlying
//! capture. The local user is a camera/screen tile; remote participants are grey
//! placeholders. Auto-joins a fake session (a generated "meeting link" hash).
//! A settings strip lists the real audio devices (`AudioDeviceList::enumerate`).
//!
//! (Camera/screen tiles render their live frames on the GPU backend; on the CPU
//! backend they show the widget placeholder. Sending the captured media to remote
//! peers + per-device routing are the `WebTransport` / device-selection follow-ups
//! — see doc/SUPER_PLAN_0.2.0.md.)

use azul::audio::AudioConfig;
use azul::audio::AudioDeviceList;
use azul::camera::CameraConfig;
use azul::css::{CssProperty, LayoutWidth, LogicalSize, PixelValue};
use azul::dom::{DomNodeId, OnAudioFrameCallback, OnConsumerFrameCallback};
use azul::option::OptionRefAny;
use azul::prelude::*;
use azul::screen::ScreenCaptureConfig;
use azul::str::String as AzString;
use azul::widgets::{
    AudioFrame, CameraWidget, ConsumerFrame, FrameConsumer, MicrophoneWidget, ProgressBar,
    ScreenCaptureWidget,
};

struct MeetState {
    /// The fake "meeting link" (a generated hash), shown in the header.
    link: String,
    mic_on: bool,
    cam_on: bool,
    screen_on: bool,
    /// The microphone level last shown on the meter, in whole percent
    /// (0 = silence / −60 dBFS and below, 100 = full scale). Kept so a frame
    /// that lands on the same percent does not touch the DOM at all.
    mic_level: f32,
    /// The level meter's `ProgressBar` container node, recorded at its
    /// `AfterMount` and cleared at `BeforeUnmount`. A thread writeback (which
    /// is what an audio frame arrives as) has no hit node, so the meter must
    /// be found by id rather than relative to "the node this event hit".
    meter_bar: Option<DomNodeId>,
    /// Enumerated audio devices (shown in the settings strip).
    mics: Vec<String>,
    speakers: Vec<String>,
    /// What the remote participants' cut of our camera would have cost on
    /// the wire: frames and bytes handed to `camera_frame_for_remote`.
    remote_frames: u64,
    remote_bytes: u64,
}

/// The remote participants' view of our camera. ONE capture serves two
/// sizes: the self tile gets a cut at its own device size, and this consumer
/// gets 320x180 - what a meeting would encode and send, cut off the main
/// thread from the same frame (`CameraWidget::with_consumer`). The camera is
/// opened at the size covering both, never at its 1080p default.
const REMOTE_VIEW_ID: u32 = 1;
const REMOTE_VIEW_W: u32 = 320;
const REMOTE_VIEW_H: u32 = 180;

/// The quietest level the meter shows, in dBFS. Speech into a laptop mic
/// sits around −30…−15 dBFS; −60 is the room's noise floor. A linear RMS
/// scale would park every spoken word in the bottom tenth of the bar.
const METER_FLOOR_DB: f32 = -60.0;

/// Microphone level as the meter shows it: the chunk's RMS in dBFS, mapped
/// linearly from [`METER_FLOOR_DB`] (0 %) to 0 dBFS (100 %).
fn mic_level_percent(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let mean_square = samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32;
    let rms = mean_square.sqrt();
    // 1e-6 ≈ −120 dBFS: keeps log10 finite on digital silence.
    let db = 20.0 * rms.max(1e-6).log10();
    ((db - METER_FLOOR_DB) / -METER_FLOOR_DB * 100.0).clamp(0.0, 100.0)
}

const TILE: &str = "width: 300px; height: 200px; margin: 8px; border-radius: 10px; \
    background: #2b2b38; display: flex; align-items: center; justify-content: center; \
    color: #99a; font-size: 17px; overflow: hidden;";
// `white-space: nowrap` + `flex-shrink: 0`: without them the toolbar's flex
// line shrinks the buttons below their label width and the text breaks
// mid-phrase — "Unmute" on one line, "mic" on the next. A control's label is
// not prose; it must never wrap.
const BTN: &str = "padding: 10px 18px; margin: 0 6px; border-radius: 8px; \
    background: #3a3a4a; color: #e6e6f0; font-size: 14px; \
    white-space: nowrap; flex-shrink: 0;";
const BTN_ON: &str = "padding: 10px 18px; margin: 0 6px; border-radius: 8px; \
    background: #2f6db0; color: #ffffff; font-size: 14px; \
    white-space: nowrap; flex-shrink: 0;";

fn participant(name: &str) -> Dom {
    Dom::create_div()
        .with_css(TILE)
        .with_child(Dom::create_span_with_text(name))
}

/// One column of the settings strip: a device-kind heading + the device names.
fn device_col(title: &str, devices: &[String]) -> Dom {
    let mut col =
        Dom::create_div().with_css("display: flex; flex-direction: column; margin: 0 28px;");
    col = col.with_child(
        Dom::create_span_with_text(title)
            .with_css("font-size: 13px; color: #8890a8; margin-bottom: 4px;"),
    );
    if devices.is_empty() {
        col = col.with_child(
            Dom::create_span_with_text("(none detected)").with_css("font-size: 13px; color: #667;"),
        );
    } else {
        for d in devices {
            col = col.with_child(
                Dom::create_span_with_text(d.as_str())
                    .with_css("font-size: 13px; color: #ccd; padding: 2px 0;"),
            );
        }
    }
    col
}

extern "C" fn layout(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    let (link, mic, cam, screen, mic_level, mics, speakers, remote) =
        match data.downcast_ref::<MeetState>() {
            Some(s) => (
                s.link.clone(),
                s.mic_on,
                s.cam_on,
                s.screen_on,
                s.mic_level,
                s.mics.clone(),
                s.speakers.clone(),
                (s.remote_frames, s.remote_bytes),
            ),
            None => return Dom::create_body(),
        };

    // --- self tile: a live CameraWidget when on, else a grey placeholder ---
    let self_tile = if cam {
        Dom::create_div().with_css(TILE).with_child(
            CameraWidget::create(CameraConfig::default())
                // "Client Bob wants 320x180": a second consumer of the same
                // capture, cut per frame off the main thread.
                .with_consumer(FrameConsumer::new(REMOTE_VIEW_ID, REMOTE_VIEW_W, REMOTE_VIEW_H))
                .with_on_consumer_frame(
                    data.clone(),
                    OnConsumerFrameCallback {
                        cb: camera_frame_for_remote,
                        callable: OptionRefAny::None,
                    },
                )
                .dom()
                .with_css("width: 100%; height: 100%;"),
        )
    } else {
        Dom::create_div()
            .with_css(TILE)
            .with_child(Dom::create_span_with_text("You · camera off"))
    };

    // --- video grid: self + (optional) screen-share + remote placeholders ---
    let mut grid = Dom::create_div().with_css(
        "display: flex; flex-wrap: wrap; flex-grow: 1; align-content: flex-start; \
         justify-content: center; padding: 12px;",
    );
    grid = grid.with_child(self_tile);
    if screen {
        grid = grid.with_child(
            Dom::create_div().with_css(TILE).with_child(
                ScreenCaptureWidget::create(ScreenCaptureConfig::default())
                    .dom()
                    .with_css("width: 100%; height: 100%;"),
            ),
        );
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
                .with_child(Dom::create_span_with_text(if mic {
                    "Mute"
                } else {
                    "Unmute mic"
                }))
                .with_callback(
                    EventFilter::Hover(HoverEventFilter::MouseUp),
                    data.clone(),
                    mic_toggle,
                ),
        )
        .with_child(
            Dom::create_div()
                .with_css(if cam { BTN_ON } else { BTN })
                .with_child(Dom::create_span_with_text(if cam {
                    "Stop video"
                } else {
                    "Start video"
                }))
                .with_callback(
                    EventFilter::Hover(HoverEventFilter::MouseUp),
                    data.clone(),
                    cam_toggle,
                ),
        )
        .with_child(
            Dom::create_div()
                .with_css(if screen { BTN_ON } else { BTN })
                .with_child(Dom::create_span_with_text(if screen {
                    "Stop share"
                } else {
                    "Share screen"
                }))
                .with_callback(
                    EventFilter::Hover(HoverEventFilter::MouseUp),
                    data.clone(),
                    screen_toggle,
                ),
        );

    // --- settings strip: the real enumerated audio devices ---
    let devices_panel = Dom::create_div()
        .with_css(
            "display: flex; justify-content: center; padding: 10px 12px 16px 12px; \
             background: #0e0e14; border-top: 1px solid #222;",
        )
        .with_child(device_col("Microphones", &mics))
        .with_child(device_col("Speakers", &speakers))
        .with_child(device_col(
            "Outgoing video",
            &[format!(
                "{}x{} · {} frames · {:.1} MB (camera on)",
                REMOTE_VIEW_W,
                REMOTE_VIEW_H,
                remote.0,
                remote.1 as f64 / 1_048_576.0
            )],
        ));

    let mut body = Dom::create_body().with_css(
        "display: flex; flex-direction: column; height: 100%; margin: 0; \
         background: #0e0e14; font-family: sans-serif; color: #e6e6f0;",
    );
    body = body.with_child(
        Dom::create_span_with_text(format!("AzMeet · meeting {}", link).as_str())
            .with_css("padding: 12px; font-size: 18px; background: #15151c;"),
    );
    // While unmuted, a (visually tiny) MicrophoneWidget captures audio — its
    // AfterMount starts the mic, its Drop (on RefreshDom when muted) stops it.
    // Every captured chunk goes to `mic_on_frame`, which drives the level
    // meter below it.
    if mic {
        body = body.with_child(
            MicrophoneWidget::create(AudioConfig {
                sample_rate: 48_000,
                channels: 1,
            })
            .with_on_frame(
                data.clone(),
                OnAudioFrameCallback {
                    cb: mic_on_frame,
                    callable: OptionRefAny::None,
                },
            )
            .dom()
            .with_css("width: 1px; height: 1px; overflow: hidden;"),
        );
        // Meter row: label + ProgressBar. The bar publishes its percentage as
        // its accessibility value on every build; `mic_on_frame` keeps both
        // the fill and that value current between builds. Its mount hooks
        // hand the bar's node to the state so the audio callback can find it.
        body = body.with_child(
            Dom::create_div()
                .with_css(
                    "display: flex; flex-direction: row; align-items: center; \
                     padding: 6px 12px; background: #15151c;",
                )
                .with_child(Dom::create_span_with_text("Mic level").with_css(
                    "font-size: 13px; color: #8890a8; margin-right: 10px; white-space: nowrap;",
                ))
                .with_child(
                    ProgressBar::create(mic_level)
                        .dom()
                        .with_css("width: 200px;")
                        .with_callback(
                            EventFilter::Component(ComponentEventFilter::AfterMount),
                            data.clone(),
                            meter_mounted,
                        )
                        .with_callback(
                            EventFilter::Component(ComponentEventFilter::BeforeUnmount),
                            data.clone(),
                            meter_unmounted,
                        ),
                ),
        );
    }
    body.with_child(grid)
        .with_child(toolbar)
        .with_child(devices_panel)
}

/// The meter's `ProgressBar` container is on screen: remember its node.
extern "C" fn meter_mounted(mut data: RefAny, info: CallbackInfo) -> Update {
    if let Some(mut s) = data.downcast_mut::<MeetState>() {
        s.meter_bar = Some(info.get_hit_node());
    }
    Update::DoNothing
}

/// The meter is going away (mute): forget the node so a late audio frame
/// cannot write into whatever takes its place.
extern "C" fn meter_unmounted(mut data: RefAny, _info: CallbackInfo) -> Update {
    if let Some(mut s) = data.downcast_mut::<MeetState>() {
        s.meter_bar = None;
    }
    Update::DoNothing
}

/// One captured audio chunk → the level meter, LIVE: no `RefreshDom`.
///
/// A rebuild per chunk (~50 per second) would re-run `layout()` and reconcile
/// every tile at audio rate; the meter is three properties on three nodes, so
/// the callback sets exactly those. The bar container is the node
/// `meter_mounted` recorded; its FILL is its first child and the remaining
/// space the fill's next sibling — the `ProgressBar` widget's documented shape
/// (container → bar + remaining). The container is also what carries the
/// accessibility value, so a screen reader hears the level a sighted user
/// sees, updated at the same moment.
/// The remote participants' cut of every camera frame (320x180, RGBA8). A
/// meeting encodes and sends it here; the demo only accounts for it - the
/// settings strip shows the tally on its next rebuild (no per-frame DOM
/// churn: `DoNothing`).
extern "C" fn camera_frame_for_remote(
    mut data: RefAny,
    _info: CallbackInfo,
    frame: ConsumerFrame,
) -> Update {
    if frame.consumer.id != REMOTE_VIEW_ID {
        return Update::DoNothing;
    }
    if let Some(mut s) = data.downcast_mut::<MeetState>() {
        s.remote_frames += 1;
        s.remote_bytes += frame.frame.bytes.as_ref().len() as u64;
    }
    Update::DoNothing
}

extern "C" fn mic_on_frame(mut data: RefAny, mut info: CallbackInfo, frame: AudioFrame) -> Update {
    let level = mic_level_percent(frame.samples.as_ref()).round();
    let bar = {
        let Some(mut s) = data.downcast_mut::<MeetState>() else {
            return Update::DoNothing;
        };
        // Same whole percent as last time: nothing to draw, nothing to say.
        if (s.mic_level - level).abs() < 0.5 {
            return Update::DoNothing;
        }
        s.mic_level = level;
        let Some(bar) = s.meter_bar else {
            return Update::DoNothing;
        };
        bar
    };
    let Some(fill) = info.get_first_child(bar).into_option() else {
        return Update::DoNothing;
    };
    let Some(remaining) = info.get_next_sibling(fill).into_option() else {
        return Update::DoNothing;
    };

    info.set_css_property(
        fill,
        CssProperty::const_width(LayoutWidth::Px(PixelValue::percent(level))),
    );
    info.set_css_property(
        remaining,
        CssProperty::const_width(LayoutWidth::Px(PixelValue::percent(100.0 - level))),
    );
    info.set_accessibility_value(bar, format!("{level:.0}%"));
    Update::DoNothing
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

/// Start the app. On desktop/iOS this blocks; on Android `App::run` only
/// stashes the window options for libazul's `android_main` to pick up.
pub fn start() {
    let link = gen_link();
    let devs = AudioDeviceList::enumerate();
    let mic_slice: &[AzString] = devs.inputs.as_ref();
    let mics: Vec<String> = mic_slice.iter().map(|s| s.as_str().to_string()).collect();
    let spk_slice: &[AzString] = devs.outputs.as_ref();
    let speakers: Vec<String> = spk_slice.iter().map(|s| s.as_str().to_string()).collect();
    eprintln!(
        "[azmeet] joined meeting {link} — {} mic(s), {} speaker(s) detected \
         (camera/mic/screen off — toggle in the toolbar)",
        mics.len(),
        speakers.len()
    );

    let data = RefAny::new(MeetState {
        link,
        mic_on: false,
        mic_level: 0.0,
        meter_bar: None,
        cam_on: false,
        screen_on: false,
        mics,
        speakers,
        remote_frames: 0,
        remote_bytes: 0,
    });
    let config = AppConfig::create();
    let app = App::create(data, config);
    let mut window = WindowCreateOptions::create(layout);
    window.window_state.size.dimensions = LogicalSize::create(1100.0, 720.0);
    app.run(window);
}

// Android has no `main()`: the OS loads this cdylib and calls libazul's
// `android_main` through the android-activity glue, which reads the window
// options `App::run` stashed — so `start()` must run at `System.loadLibrary`
// time, before `ANativeActivity_onCreate`.
#[cfg(target_os = "android")]
#[ctor::ctor]
fn azul_android_init() {
    start();
}
