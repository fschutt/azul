//! AzVideo — H.264 hardware-decode video player (P6).
//!
//! End-to-end, on the public azul surface:
//!   1. `VideoStartupCheck::run()` — probe `VK_KHR_video_decode_h264` readiness.
//!   2. local sample (or `http_get`) — obtain the Big Buck Bunny H.264 MP4.
//!   3. `decode_mp4_h264_bytes` — demux + Vulkan Video decode the whole clip to
//!      RGBA frames (GPU decode on the video-decode queue, copied back to CPU).
//!   4. the decoded frames are shown as an animating `<img>` driven by a
//!      per-frame Timer, so Big Buck Bunny actually *plays*. Where no Vulkan
//!      Video decoder exists the decode yields no frames and the test-pattern
//!      `VideoWidget` stands in.
//!
//! The C `azul-video` player drives the same FFI calls and adds the
//! driver-provision msgbox/autofix on top.

use azul::prelude::*;
use azul::widgets::VideoWidget;
use azul::misc::VideoConfig;
use azul::image::{ImageRef, RawImage, RawImageData, RawImageFormat};
use azul::task::TerminateTimer;
use azul::desktop::http::http_get;
use azul::desktop::extra::video_codec::VideoStartupCheck;
use azul::desktop::extra::video_codec::pipeline::decode_mp4_h264_bytes;

/// Big Buck Bunny H.264/MP4, 360p. Fetched if the local sample is absent.
const BBB_URL: &str =
    "https://test-videos.co.uk/vids/bigbuckbunny/mp4/h264/360/Big_Buck_Bunny_360_10s_2MB.mp4";
const LOCAL_SAMPLE: &str = "/tmp/video-media-samples/big-buck-bunny-360p.mp4";
/// Cap decoded frames shown (360p RGBA ≈ 0.9 MB each). Plays + loops.
const MAX_FRAMES: usize = 150;

struct VideoAppState {
    /// Pipeline status lines for the side panel.
    status: Vec<String>,
    /// Decoded frames as ready-to-render images. Empty => no HW decoder.
    frames: Vec<ImageRef>,
    /// Currently displayed frame index (advanced by the Timer, wraps to loop).
    idx: usize,
    /// Coded video size (for the display box).
    vw: u32,
    vh: u32,
}

/// Wrap a decoded RGBA8 frame as a renderable image. azul's renderers expect
/// **BGRA8** (cpurender's blit path handles BGRA8/R8 but renders RGBA8 as a gray
/// placeholder; webrender treats the bytes as BGRA), so swap R<->B in place and
/// tag the image BGRA8.
fn rgba_image(mut bytes: Vec<u8>, w: u32, h: u32) -> Option<ImageRef> {
    for px in bytes.chunks_exact_mut(4) {
        px.swap(0, 2); // RGBA -> BGRA
    }
    ImageRef::new_rawimage(RawImage {
        pixels: RawImageData::U8(bytes.into()),
        width: w as usize,
        height: h as usize,
        premultiplied_alpha: false,
        data_format: RawImageFormat::BGRA8,
        tag: b"bbb-frame".to_vec().into(),
    })
    .into_option()
}

/// Probe + fetch + demux + decode the clip; returns status lines + decoded frames.
fn run_pipeline() -> (Vec<String>, Vec<ImageRef>, u32, u32) {
    let mut out = Vec::new();
    let mut frames: Vec<ImageRef> = Vec::new();
    let (mut vw, mut vh) = (0u32, 0u32);

    // 1. Hardware-decode capability probe.
    let check = VideoStartupCheck::run();
    out.push(format!(
        "VK hardware H.264 decode: {} — {}",
        if check.hw_decode_ready { "READY" } else { "not available" },
        check.summary.as_str(),
    ));

    // 2. Obtain the clip — prefer a local sample (offline / fast), else fetch.
    let bytes: Vec<u8> = match std::fs::read(LOCAL_SAMPLE) {
        Ok(b) => {
            out.push(format!("Loaded local sample: {} bytes", b.len()));
            b
        }
        Err(_) => match http_get(BBB_URL) {
            Ok(resp) => {
                let b = resp.body.as_ref().to_vec();
                out.push(format!(
                    "HTTP GET -> {} bytes (status {})",
                    b.len(),
                    resp.status_code
                ));
                b
            }
            Err(e) => {
                out.push(format!("HTTP fetch failed: {:?}", e));
                return (out, frames, vw, vh);
            }
        },
    };

    // 3+4. Demux + Vulkan Video decode the whole clip to RGBA frames.
    match decode_mp4_h264_bytes(&bytes) {
        Ok(decoded) => {
            vw = decoded.width;
            vh = decoded.height;
            out.push(format!(
                "Demuxed H.264: {}x{} @ {:.1} fps · {} access units fed",
                decoded.width, decoded.height, decoded.fps, decoded.access_units_fed,
            ));
            for vf in decoded.frames.iter().take(MAX_FRAMES) {
                if let Some(img) = rgba_image(vf.bytes.as_slice().to_vec(), vf.width, vf.height) {
                    frames.push(img);
                }
            }
            out.push(format!(
                "Decoded {} frames ({}x{}) via {} — {}",
                decoded.frames.len(),
                vw,
                vh,
                if check.hw_decode_ready {
                    "VK Video (GPU decode, CPU copy-back)"
                } else {
                    "no HW decoder"
                },
                if frames.is_empty() {
                    "showing test pattern"
                } else {
                    "playing"
                },
            ));
        }
        Err(e) => out.push(format!("Decode failed: {}", e)),
    }

    (out, frames, vw, vh)
}

/// Per-frame Timer: advance to the next decoded frame (wrap to loop) + relayout.
extern "C" fn advance_frame(mut data: RefAny, _info: TimerCallbackInfo) -> TimerCallbackReturn {
    let mut should_update = Update::DoNothing;
    if let Some(mut s) = data.downcast_mut::<VideoAppState>() {
        if !s.frames.is_empty() {
            s.idx = (s.idx + 1) % s.frames.len();
            should_update = Update::RefreshDom;
        }
    }
    TimerCallbackReturn {
        should_terminate: TerminateTimer::Continue,
        should_update,
    }
}

/// Window-create: install the playback Timer (only if we have frames to show).
extern "C" fn startup(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let has_frames = data
        .downcast_ref::<VideoAppState>()
        .map(|s| !s.frames.is_empty())
        .unwrap_or(false);
    if has_frames {
        info.add_timer(
            TimerId::unique(),
            Timer::create(
                data.clone(),
                TimerCallback {
                    cb: advance_frame,
                    ctx: OptionRefAny::None,
                },
                info.get_system_time_fn(),
            ),
        );
    }
    Update::DoNothing
}

extern "C" fn layout(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    let (lines, frame, vw, vh, nframes, idx) = match data.downcast_ref::<VideoAppState>() {
        Some(s) => (
            s.status.clone(),
            s.frames.get(s.idx).cloned(),
            s.vw,
            s.vh,
            s.frames.len(),
            s.idx,
        ),
        None => (Vec::new(), None, 0, 0, 0, 0),
    };

    let mut root = Dom::create_body().with_css(
        "display: flex; flex-direction: column; padding: 16px; background: #0e0e14; \
         font-family: sans-serif; color: #e6e6f0;",
    );
    root = root.with_child(
        Dom::create_text("AzVideo — H.264 hardware decode (Big Buck Bunny)")
            .with_css("font-size: 22px; margin-bottom: 10px;"),
    );
    for line in &lines {
        root = root.with_child(
            Dom::create_text(line.as_str())
                .with_css("font-size: 13px; color: #b8c0d0; margin-bottom: 5px;"),
        );
    }

    // Sizing for the video box: fit ~520px wide, keep aspect.
    let (boxw, boxh) = if vw > 0 && vh > 0 {
        let scale = 520.0 / vw as f32;
        (520u32, (vh as f32 * scale) as u32)
    } else {
        (480, 270)
    };

    if let Some(img) = frame {
        let playing = format!("playing — frame {}/{}", idx + 1, nframes);
        let img_css = format!(
            "width: {}px; height: {}px; border: 2px solid #2a2a3a; border-radius: 8px; \
             overflow: hidden;",
            boxw, boxh
        );
        root = root.with_child(
            Dom::create_text(playing.as_str())
                .with_css("font-size: 12px; color: #7ad17a; margin: 10px 0 5px 0;"),
        );
        root = root.with_child(Dom::create_image(img).with_css(img_css.as_str()));
    } else {
        root = root.with_child(
            Dom::create_text("no decoded frames — test pattern (no Vulkan Video decode here):")
                .with_css("font-size: 12px; color: #6a7080; margin: 10px 0 5px 0;"),
        );
        root = root.with_child(VideoWidget::create(VideoConfig::default()).dom().with_css(
            "width: 480px; height: 270px; border: 2px solid #2a2a3a; border-radius: 8px; \
             overflow: hidden;",
        ));
    }

    root
}

fn main() {
    eprintln!("[azvideo] decoding (this can take a few seconds)…");
    let (status, frames, vw, vh) = run_pipeline();
    for line in &status {
        eprintln!("[azvideo] {}", line);
    }

    let data = RefAny::new(VideoAppState {
        status,
        frames,
        idx: 0,
        vw,
        vh,
    });
    let config = AppConfig::create();
    let app = App::create(data, config);
    let mut window = WindowCreateOptions::create(layout);
    window.create_callback = Some(Callback::create(startup)).into();
    app.run(window);
}
