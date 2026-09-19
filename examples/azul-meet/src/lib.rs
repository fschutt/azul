use azul::{
    app::RendererOptions,
    audio::{AudioConfig, AudioDeviceList, AudioDeviceListResult, AudioFrame},
    callbacks::{
        CallbackInfo, TimerCallbackInfo, TimerCallbackReturn, UpdateImageType,
    },
    camera::CameraConfig,
    css::{LogicalSize, PhysicalPositionI32, Srgb, WindowPosition},
    dom::{Callback, DomNodeId, NodeId},
    error::{ResultRawImageDecodeImageError, ResultU8VecEncodeImageError},
    image::{ImageRef, RawImage, RawImageData, RawImageFormat},
    iroh::{IrohConfig, IrohEndpoint, IrohEvent, IrohEventKind, IrohRelayMode},
    option::{OptionRendererOptions, OptionString},
    prelude::*,
    screen::ScreenCaptureConfig,
    str::String as AzString,
    task::{Timer, TimerId},
    time::{Duration, SystemTimeDiff},
    vec::{U8Vec, U8VecRef},
    widgets::{
        CameraWidget, ConsumerFrame, FrameConsumer, MicrophoneWidget, ProgressBar,
        ScreenCaptureWidget,
    },
    window::{HwAcceleration, Vsync},
};

const ALPN: &str = "azmeet/mjpeg/1";
const CAMERA_TRACK: u32 = 1;
const SCREEN_TRACK: u32 = 2;
const FEED_W: u32 = 320;
const FEED_H: u32 = 180;
const JPEG_QUALITY: u8 = 75;
const PUMP_MS: u64 = 15;
const STATS_EVERY_TICKS: u32 = 130;

struct MeetState {
    link: String,
    name: String,
    peer_name: String,
    backend: &'static str,
    endpoint: Option<IrohEndpoint>,
    guest: Option<IrohEndpoint>,
    remote: Option<u64>,
    remote_tracks: [bool; 2],
    link_status: String,
    ticks: u32,
    mic_on: bool,
    cam_on: bool,
    screen_on: bool,
    mic_level: f32,
    meter_bar: Option<DomNodeId>,
    mics: Vec<String>,
    speakers: Vec<String>,
    devices_requested: bool,
}

impl MeetState {
    fn new(link: &str, name: &str, peer_name: &str, backend: &'static str) -> Self {
        MeetState {
            link: link.to_string(),
            name: name.to_string(),
            peer_name: peer_name.to_string(),
            backend,
            endpoint: None,
            guest: None,
            remote: None,
            remote_tracks: [false; 2],
            link_status: String::from("binding"),
            ticks: 0,
            mic_on: false,
            cam_on: false,
            screen_on: false,
            mic_level: 0.0,
            meter_bar: None,
            mics: Vec::new(),
            speakers: Vec::new(),
            devices_requested: false,
        }
    }

    fn marker(&self, track: u32) -> String {
        format!("azmeet-{}-track-{track}", self.name)
    }
}

struct Room {
    peers: Vec<RefAny>,
}

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

fn track_slot(track: u32) -> Option<usize> {
    match track {
        CAMERA_TRACK => Some(0),
        SCREEN_TRACK => Some(1),
        _ => None,
    }
}

fn remote_video_tile(marker: &str) -> Dom {
    Dom::create_div().with_css(TILE).with_child(
        Dom::create_image(ImageRef::null_image(
            FEED_W as usize,
            FEED_H as usize,
            RawImageFormat::RGBA8,
            U8VecRef::from(&[][..]),
        ))
        .with_marker(OptionString::Some(AzString::from(marker)))
        .with_css("width: 100%; height: 100%;"),
    )
}

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

extern "C" fn layout_first(data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    peer_layout(data, 0)
}

extern "C" fn layout_second(data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    peer_layout(data, 1)
}

fn room_peer(data: &RefAny, index: usize) -> Option<RefAny> {
    let mut data = data.clone();
    let room = data.downcast_ref::<Room>()?;
    room.peers.get(index).cloned()
}

fn peer_layout(data: RefAny, index: usize) -> Dom {
    match room_peer(&data, index) {
        Some(peer) => meet_layout(peer),
        None => Dom::create_body(),
    }
}

fn feed_consumer(track: u32) -> FrameConsumer {
    FrameConsumer::create(track, FEED_W, FEED_H)
}

struct LayoutSnapshot {
    link: String,
    name: String,
    peer_name: String,
    backend: &'static str,
    linked: bool,
    remote_markers: Vec<String>,
    link_status: String,
    mic: bool,
    cam: bool,
    screen: bool,
    mic_level: f32,
    mics: Vec<String>,
    speakers: Vec<String>,
}

fn snapshot(s: &MeetState) -> LayoutSnapshot {
    LayoutSnapshot {
        link: s.link.clone(),
        name: s.name.clone(),
        peer_name: s.peer_name.clone(),
        backend: s.backend,
        linked: s.endpoint.is_some(),
        remote_markers: [CAMERA_TRACK, SCREEN_TRACK]
            .into_iter()
            .filter(|track| track_slot(*track).is_some_and(|slot| s.remote_tracks[slot]))
            .map(|track| s.marker(track))
            .collect(),
        link_status: s.link_status.clone(),
        mic: s.mic_on,
        cam: s.cam_on,
        screen: s.screen_on,
        mic_level: s.mic_level,
        mics: s.mics.clone(),
        speakers: s.speakers.clone(),
    }
}

fn meet_layout(mut data: RefAny) -> Dom {
    let Some(view) = data.downcast_ref::<MeetState>().map(|s| snapshot(&s)) else {
        return Dom::create_body();
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

    let self_tile = if view.cam {
        Dom::create_div().with_css(TILE).with_child(
            CameraWidget::create(CameraConfig::default())
                .with_consumer(feed_consumer(CAMERA_TRACK))
                .with_on_consumer_frame(data.clone(), send_feed_frame)
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
    if view.screen {
        grid = grid.with_child(
            Dom::create_div().with_css(TILE).with_child(
                ScreenCaptureWidget::create(ScreenCaptureConfig::default())
                    .with_consumer(feed_consumer(SCREEN_TRACK))
                    .with_on_consumer_frame(data.clone(), send_feed_frame)
                    .dom()
                    .with_css("width: 100%; height: 100%;"),
            ),
        );
    }
    if !view.linked {
        grid = grid
            .with_child(participant("Alice"))
            .with_child(participant("Bob"))
            .with_child(participant("Carol"));
    } else if view.remote_markers.is_empty() {
        grid = grid.with_child(participant(&format!(
            "{} · waiting for video",
            view.peer_name
        )));
    } else {
        for marker in &view.remote_markers {
            grid = grid.with_child(remote_video_tile(marker));
        }
    }

    let toolbar = Dom::create_div()
        .with_css("display: flex; justify-content: center; padding: 14px; background: #15151c;")
        .with_child(
            Dom::create_div()
                .with_css(if view.mic { BTN_ON } else { BTN })
                .with_child(Dom::create_span_with_text(if view.mic {
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
                .with_css(if view.cam { BTN_ON } else { BTN })
                .with_child(Dom::create_span_with_text(if view.cam {
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
                .with_css(if view.screen { BTN_ON } else { BTN })
                .with_child(Dom::create_span_with_text(if view.screen {
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

    let link_line = if view.linked {
        format!("{FEED_W}x{FEED_H} JPEG over iroh · {}", view.link_status)
    } else {
        view.link_status.clone()
    };
    let devices_panel = Dom::create_div()
        .with_css(
            "display: flex; justify-content: center; padding: 10px 12px 16px 12px; background: \
             #0e0e14; border-top: 1px solid #222;",
        )
        .with_child(device_col("Microphones", &view.mics))
        .with_child(device_col("Speakers", &view.speakers))
        .with_child(device_col("Video link", &[link_line]));

    let mut body = Dom::create_body().with_css(
        "display: flex; flex-direction: column; height: 100%; margin: 0; background: #0e0e14; \
         font-family: sans-serif; color: #e6e6f0;",
    );
    let header = if view.linked {
        format!(
            "AzMeet · meeting {} · {} ({})",
            view.link, view.name, view.backend
        )
    } else {
        format!("AzMeet · meeting {}", view.link)
    };
    body = body.with_child(
        Dom::create_span_with_text(header.as_str())
            .with_css("padding: 12px; font-size: 18px; background: #15151c;"),
    );
    if view.mic {
        body = body.with_child(
            MicrophoneWidget::create(AudioConfig {
                sample_rate: 48_000,
                channels: 1,
            })
            .with_on_frame(
                data.clone(),
                mic_on_frame,
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
                    ProgressBar::create(view.mic_level)
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

extern "C" fn send_feed_frame(
    mut data: RefAny,
    _info: CallbackInfo,
    frame: ConsumerFrame,
) -> Update {
    let track = frame.consumer.id;
    if track_slot(track).is_none() {
        return Update::DoNothing;
    }
    let Some(endpoint) = data
        .downcast_ref::<MeetState>()
        .filter(|s| s.remote.is_some())
        .and_then(|s| s.endpoint.clone())
    else {
        return Update::DoNothing;
    };
    let image = RawImage {
        pixels: RawImageData::U8(frame.frame.bytes.clone()),
        width: frame.frame.width as usize,
        height: frame.frame.height as usize,
        premultiplied_alpha: false,
        data_format: RawImageFormat::RGBA8,
        tag: U8Vec::create(),
    };
    if let ResultU8VecEncodeImageError::Ok(jpeg) = image.encode_jpeg(JPEG_QUALITY) {
        endpoint.broadcast_frame(track, jpeg);
    }
    Update::DoNothing
}

fn short_id(id: &str) -> &str {
    id.get(..10).unwrap_or(id)
}

fn apply_link_event(s: &mut MeetState, event: &IrohEvent) -> bool {
    match event.kind {
        IrohEventKind::Ready => {
            eprintln!("[azmeet] {}: iroh endpoint ready", s.name);
            s.link_status = String::from("waiting for a peer");
            if let Some(guest) = s.guest.take() {
                guest.connect(event.text.clone());
            }
            true
        }
        IrohEventKind::PeerConnected => {
            let id = short_id(event.text.as_str());
            eprintln!("[azmeet] {}: connected to {id}", s.name);
            s.remote = Some(event.peer);
            s.link_status = format!("connected to {id}");
            true
        }
        IrohEventKind::PeerDisconnected if s.remote == Some(event.peer) => {
            s.remote = None;
            s.remote_tracks = [false; 2];
            s.link_status = format!("disconnected: {}", event.text.as_str());
            true
        }
        IrohEventKind::Error => {
            eprintln!("[azmeet] {}: iroh error: {}", s.name, event.text.as_str());
            s.link_status = format!("error: {}", event.text.as_str());
            true
        }
        _ => false,
    }
}

fn show_remote_frame(data: &mut RefAny, info: &mut TimerCallbackInfo, frame: &IrohEvent) -> bool {
    let Some(slot) = track_slot(frame.track) else {
        return false;
    };
    let Some((marker, shown)) = data.downcast_mut::<MeetState>().map(|mut s| {
        let shown = s.remote_tracks[slot];
        s.remote_tracks[slot] = true;
        (s.marker(frame.track), shown)
    }) else {
        return false;
    };
    if !shown {
        return true;
    }
    let ResultRawImageDecodeImageError::Ok(decoded) =
        RawImage::decode_image_bytes_any(U8VecRef::from(frame.data.as_ref()))
    else {
        return false;
    };
    let Some(image) = ImageRef::create_rawimage(decoded).into_option() else {
        return false;
    };
    let Some(node) = info
        .callback_info
        .get_node_id_by_marker(AzString::from(marker.as_str()))
        .into_option()
    else {
        return false;
    };
    let index = node.node.into_raw();
    if index > 0 {
        info.callback_info.change_node_image(
            node.dom,
            NodeId { inner: index - 1 },
            image,
            UpdateImageType::Content,
        );
    }
    false
}

extern "C" fn pump_link(mut data: RefAny, mut info: TimerCallbackInfo) -> TimerCallbackReturn {
    let Some(endpoint) = data
        .downcast_ref::<MeetState>()
        .and_then(|s| s.endpoint.clone())
    else {
        return TimerCallbackReturn::continue_unchanged();
    };
    let mut refresh = false;
    let mut frames = Vec::new();
    while let Some(event) = endpoint.recv().into_option() {
        if event.kind == IrohEventKind::Frame {
            frames.push(event);
        } else if let Some(mut s) = data.downcast_mut::<MeetState>() {
            refresh |= apply_link_event(&mut s, &event);
        }
    }
    for frame in &frames {
        refresh |= show_remote_frame(&mut data, &mut info, frame);
    }
    if let Some(mut s) = data.downcast_mut::<MeetState>() {
        s.ticks = s.ticks.wrapping_add(1);
        if let Some(peer) = s.remote.filter(|_| s.ticks % STATS_EVERY_TICKS == 0) {
            let stats = endpoint.peer_stats(peer);
            s.link_status = format!(
                "{} · RTT {:.1} ms · sent {} · received {} · skipped {}",
                if stats.direct { "direct" } else { "relayed" },
                stats.rtt_us as f64 / 1000.0,
                stats.frames_sent,
                stats.frames_received,
                stats.frames_skipped
            );
            eprintln!("[azmeet] {}: {}", s.name, s.link_status);
            refresh = true;
        }
    }
    if refresh {
        TimerCallbackReturn::continue_and_refresh_dom()
    } else {
        TimerCallbackReturn::continue_unchanged()
    }
}

extern "C" fn startup_first(data: RefAny, info: CallbackInfo) -> Update {
    start_pumping(data, info, 0)
}

extern "C" fn startup_second(data: RefAny, info: CallbackInfo) -> Update {
    start_pumping(data, info, 1)
}

fn start_pumping(data: RefAny, mut info: CallbackInfo, index: usize) -> Update {
    let Some(peer) = room_peer(&data, index) else {
        return Update::DoNothing;
    };
    let get_time = info.get_system_time_fn();
    info.add_timer(
        TimerId::unique(),
        Timer::create(
            peer,
            pump_link,
            get_time,
        )
        .with_interval(Duration::System(SystemTimeDiff::from_millis(PUMP_MS))),
    );
    Update::DoNothing
}

fn renderer(hw_accel: HwAcceleration) -> OptionRendererOptions {
    OptionRendererOptions::Some(RendererOptions {
        vsync: Vsync::Enabled,
        srgb: Srgb::DontCare,
        hw_accel,
    })
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

fn bind_endpoint() -> IrohEndpoint {
    IrohEndpoint::bind(IrohConfig::create(ALPN).with_relay_mode(IrohRelayMode::Disabled))
}

fn bind_failure(endpoint: &IrohEndpoint) -> String {
    match endpoint.recv().into_option() {
        Some(event) if event.kind == IrohEventKind::Error => event.text.as_str().to_string(),
        _ => String::from("the iroh endpoint did not bind"),
    }
}

pub fn start() {
    let link = gen_link();
    let ada_link = bind_endpoint();
    let ben_link = bind_endpoint();
    let peers = if ada_link.is_bound() && ben_link.is_bound() {
        eprintln!(
            "[azmeet] meeting {link}: Ada (CPU window, camera) and Ben (GPU window, screen share) over iroh"
        );
        let mut ada = MeetState::new(&link, "Ada", "Ben", "CPU");
        ada.guest = Some(ben_link.clone());
        ada.endpoint = Some(ada_link);
        ada.cam_on = true;
        let mut ben = MeetState::new(&link, "Ben", "Ada", "GPU");
        ben.endpoint = Some(ben_link);
        ben.screen_on = true;
        vec![RefAny::new(ada), RefAny::new(ben)]
    } else {
        let reason = bind_failure(&ada_link);
        eprintln!("[azmeet] joined meeting {link} without a peer link: {reason}");
        let mut solo = MeetState::new(&link, "You", "", "");
        solo.link_status = reason;
        vec![RefAny::new(solo)]
    };
    let linked = peers.len() == 2;
    let mut app = App::create(RefAny::new(Room { peers }), AppConfig::create());
    let mut first = WindowCreateOptions::create(layout_first);
    first.create_callback = Some(Callback::create(startup_first)).into();
    if linked {
        first.window_state.size.dimensions = LogicalSize::create(740.0, 640.0);
        first.window_state.title = AzString::from("AzMeet · Ada (CPU)");
        first.window_state.position =
            WindowPosition::Initialized(PhysicalPositionI32 { x: 20, y: 40 });
        first.renderer = renderer(HwAcceleration::Disabled);
        let mut second = WindowCreateOptions::create(layout_second);
        second.create_callback = Some(Callback::create(startup_second)).into();
        second.window_state.size.dimensions = LogicalSize::create(740.0, 640.0);
        second.window_state.title = AzString::from("AzMeet · Ben (GPU)");
        second.window_state.position =
            WindowPosition::Initialized(PhysicalPositionI32 { x: 960, y: 40 });
        second.renderer = renderer(HwAcceleration::Enabled);
        app.add_window(second);
    } else {
        first.window_state.size.dimensions = LogicalSize::create(1100.0, 720.0);
    }
    app.run(first);
}

#[cfg(target_os = "android")]
#[ctor::ctor]
fn azul_android_init() {
    start();
}
