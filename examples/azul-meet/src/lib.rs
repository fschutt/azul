use azul::{
    audio::{AudioConfig, AudioDeviceList, AudioDeviceListResult, AudioFrame},
    callbacks::CallbackInfo,
    camera::CameraConfig,
    css::LogicalSize,
    dom::{DomNodeId, OnAudioFrameCallback, OnConsumerFrameCallback},
    option::OptionRefAny,
    prelude::*,
    screen::ScreenCaptureConfig,
    str::String as AzString,
    widgets::{
        CameraWidget, ConsumerFrame, FrameConsumer, MicrophoneWidget, ProgressBar,
        ScreenCaptureWidget,
    },
};

struct MeetState {
    link: String,
    mic_on: bool,
    cam_on: bool,
    screen_on: bool,
    mic_level: f32,
    meter_bar: Option<DomNodeId>,
    mics: Vec<String>,
    speakers: Vec<String>,
    devices_requested: bool,
    remote_frames: u64,
    remote_bytes: u64,
}

const REMOTE_VIEW_ID: u32 = 1;
const REMOTE_VIEW_W: u32 = 320;
const REMOTE_VIEW_H: u32 = 180;

const METER_FLOOR_DB: f32 = -60.0;

fn mic_level_percent(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let mean_square = samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32;
    let rms = mean_square.sqrt();
    let db = 20.0 * rms.max(1e-6).log10();
    ((db - METER_FLOOR_DB) / -METER_FLOOR_DB * 100.0).clamp(0.0, 100.0)
}

const TILE: &str = "width: 300px; height: 200px; margin: 8px; border-radius: 10px; background: \
                    #2b2b38; display: flex; align-items: center; justify-content: center; color: \
                    #99a; font-size: 17px; overflow: hidden;";
const BTN: &str = "padding: 10px 18px; margin: 0 6px; border-radius: 8px; background: #3a3a4a; \
                   color: #e6e6f0; font-size: 14px; white-space: nowrap; flex-shrink: 0;";
const BTN_ON: &str = "padding: 10px 18px; margin: 0 6px; border-radius: 8px; background: #2f6db0; \
                      color: #ffffff; font-size: 14px; white-space: nowrap; flex-shrink: 0;";

fn participant(name: &str) -> Dom {
    Dom::create_div()
        .with_css(TILE)
        .with_child(Dom::create_span_with_text(name))
}

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

extern "C" fn on_devices_enumerated(
    mut data: RefAny,
    _info: CallbackInfo,
    result: RefAny,
) -> Update {
    let Some(answer) = AudioDeviceListResult::downcast(result).into_option() else {
        return Update::DoNothing;
    };
    let mic_slice: &[AzString] = answer.devices.inputs.as_ref();
    let mics: Vec<String> = mic_slice.iter().map(|s| s.as_str().to_string()).collect();
    let spk_slice: &[AzString] = answer.devices.outputs.as_ref();
    let speakers: Vec<String> = spk_slice.iter().map(|s| s.as_str().to_string()).collect();
    eprintln!(
        "[azmeet] {} mic(s), {} speaker(s) detected",
        mics.len(),
        speakers.len()
    );
    if let Some(mut s) = data.downcast_mut::<MeetState>() {
        s.mics = mics;
        s.speakers = speakers;
    }
    Update::RefreshDom
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

    let first_layout = data
        .downcast_mut::<MeetState>()
        .map(|mut s| {
            let first = !s.devices_requested;
            s.devices_requested = true;
            first
        })
        .unwrap_or(false);
    if first_layout {
        let _request = AudioDeviceList::enumerate(data.clone(), on_devices_enumerated);
    }

    let self_tile = if cam {
        Dom::create_div().with_css(TILE).with_child(
            CameraWidget::create(CameraConfig::default())
                .with_consumer(FrameConsumer::create(REMOTE_VIEW_ID, REMOTE_VIEW_W, REMOTE_VIEW_H))
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

    let devices_panel = Dom::create_div()
        .with_css(
            "display: flex; justify-content: center; padding: 10px 12px 16px 12px; background: \
             #0e0e14; border-top: 1px solid #222;",
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
        "display: flex; flex-direction: column; height: 100%; margin: 0; background: #0e0e14; \
         font-family: sans-serif; color: #e6e6f0;",
    );
    body = body.with_child(
        Dom::create_span_with_text(format!("AzMeet · meeting {}", link).as_str())
            .with_css("padding: 12px; font-size: 18px; background: #15151c;"),
    );
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
        body = body.with_child(
            Dom::create_div()
                .with_css(
                    "display: flex; flex-direction: row; align-items: center; padding: 6px 12px; \
                     background: #15151c;",
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

extern "C" fn meter_mounted(mut data: RefAny, info: CallbackInfo) -> Update {
    if let Some(mut s) = data.downcast_mut::<MeetState>() {
        s.meter_bar = Some(info.get_hit_node());
    }
    Update::DoNothing
}

extern "C" fn meter_unmounted(mut data: RefAny, _info: CallbackInfo) -> Update {
    if let Some(mut s) = data.downcast_mut::<MeetState>() {
        s.meter_bar = None;
    }
    Update::DoNothing
}

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
        if (s.mic_level - level).abs() < 0.5 {
            return Update::DoNothing;
        }
        s.mic_level = level;
        let Some(bar) = s.meter_bar else {
            return Update::DoNothing;
        };
        bar
    };
    ProgressBar::update_progress(info, bar, level);
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

pub fn start() {
    let link = gen_link();
    let mics: Vec<String> = Vec::new();
    let speakers: Vec<String> = Vec::new();
    eprintln!("[azmeet] joined meeting {link} (camera/mic/screen off - toggle in the toolbar)");

    let data = RefAny::new(MeetState {
        link,
        mic_on: false,
        mic_level: 0.0,
        meter_bar: None,
        cam_on: false,
        screen_on: false,
        mics,
        speakers,
        devices_requested: false,
        remote_frames: 0,
        remote_bytes: 0,
    });
    let config = AppConfig::create();
    let app = App::create(data, config);
    let mut window = WindowCreateOptions::create(layout);
    window.window_state.size.dimensions = LogicalSize::create(1100.0, 720.0);
    app.run(window);
}

#[cfg(target_os = "android")]
#[ctor::ctor]
fn azul_android_init() {
    start();
}
