//! AzVideo — H.264 hardware-decode video player (P6).
//!
//! End-to-end, on the public azul surface:
//!   1. `VideoStartupCheck::run()` — probe `VK_KHR_video_decode_h264` readiness.
//!   2. local sample (or `http_get`) — obtain the Big Buck Bunny H.264 MP4.
//!   3. `decode_mp4_h264_bytes` — demux + Vulkan Video decode the whole clip to
//!      RGBA frames (GPU decode on the video-decode queue, copied back to CPU).
//!   4. the decoded frames are handed to a `VideoWidget` via `with_frames`. Its
//!      background worker cycles them through the shared GL `present_frame` path
//!      (the same proven GL-texture route the camera/screencap widgets use), so
//!      Big Buck Bunny actually *plays*. Where no Vulkan Video decoder exists the
//!      decode yields no frames and the widget's built-in test pattern stands in.
//!
//! The C `azul-video` player drives the same FFI calls and adds the
//! driver-provision msgbox/autofix on top.

use azul::prelude::*;
use azul::widgets::VideoWidget;
use azul::misc::VideoConfig;
use azul::desktop::http::http_get;
use azul::desktop::extra::video_codec::VideoStartupCheck;
use azul::desktop::extra::video_codec::pipeline::decode_mp4_h264_bytes;

/// Big Buck Bunny H.264/MP4, 360p. Fetched if the local sample is absent.
const BBB_URL: &str =
    "https://test-videos.co.uk/vids/bigbuckbunny/mp4/h264/360/Big_Buck_Bunny_360_10s_2MB.mp4";
const LOCAL_SAMPLE: &str = "/tmp/video-media-samples/big-buck-bunny-360p.mp4";
/// Cap decoded frames replayed (360p RGBA ≈ 0.9 MB each). The widget loops them.
const MAX_FRAMES: usize = 150;

struct VideoAppState {
    /// Pipeline status lines for the side panel.
    status: Vec<String>,
    /// Decoded frames as a `RefAny` holding a `Vec<VideoFrame>`, handed to the
    /// `VideoWidget`; `None` when no HW decoder produced any (→ test pattern).
    frames: OptionRefAny,
    /// Coded video size (for the display box).
    vw: u32,
    vh: u32,
}

/// Probe + fetch + demux + decode the clip; returns status lines, the decoded
/// frames wrapped as a `RefAny<Vec<VideoFrame>>` (or `None`), and the coded size.
fn run_pipeline() -> (Vec<String>, OptionRefAny, u32, u32) {
    let mut out = Vec::new();
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
                return (out, OptionRefAny::None, vw, vh);
            }
        },
    };

    // 3+4. Demux + Vulkan Video decode the whole clip to RGBA frames.
    match decode_mp4_h264_bytes(&bytes) {
        Ok(decoded) => {
            vw = decoded.width;
            vh = decoded.height;
            let total = decoded.frames.len();
            out.push(format!(
                "Demuxed H.264: {}x{} @ {:.1} fps · {} access units fed",
                decoded.width, decoded.height, decoded.fps, decoded.access_units_fed,
            ));

            // Cap the replayed set (each 360p RGBA frame ≈ 0.9 MB). Keep `clip`
            // as the *inferred* `Vec<VideoFrame>` type from `decoded.frames`
            // (the real `azul_core::video::VideoFrame`) — never name it as the
            // FFI-mirror `azul::widgets::VideoFrame`, or the widget worker's
            // `downcast_ref::<Vec<VideoFrame>>` would fail the TypeId check.
            let mut clip = decoded.frames;
            clip.truncate(MAX_FRAMES);
            let produced = !clip.is_empty();

            out.push(format!(
                "Decoded {} frames ({}x{}) via {} — {}",
                total,
                vw,
                vh,
                if check.hw_decode_ready {
                    "VK Video (GPU decode, CPU copy-back)"
                } else {
                    "no HW decoder"
                },
                if produced { "playing" } else { "showing test pattern" },
            ));

            let frames = if produced {
                // RefAny::new here delegates to azul_core's RefAny::new, storing
                // TypeId::of::<Vec<VideoFrame>>() — matches the worker downcast.
                OptionRefAny::Some(RefAny::new(clip))
            } else {
                OptionRefAny::None
            };
            (out, frames, vw, vh)
        }
        Err(e) => {
            out.push(format!("Decode failed: {}", e));
            (out, OptionRefAny::None, vw, vh)
        }
    }
}

extern "C" fn layout(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    let (lines, frames, vw, vh) = match data.downcast_ref::<VideoAppState>() {
        Some(s) => (s.status.clone(), s.frames.clone(), s.vw, s.vh),
        None => (Vec::new(), OptionRefAny::None, 0, 0),
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
    let box_css = format!(
        "width: {}px; height: {}px; border: 2px solid #2a2a3a; border-radius: 8px; \
         overflow: hidden;",
        boxw, boxh
    );

    // Decoded frames → VideoWidget (GL present_frame path); none → test pattern.
    // Match by reference + clone the inner RefAny (OptionRefAny is Drop, so we
    // can't move the inner value out of it).
    match &frames {
        OptionRefAny::Some(frames) => {
            root = root.with_child(
                Dom::create_text("playing decoded frames via VideoWidget (GL present_frame)")
                    .with_css("font-size: 12px; color: #7ad17a; margin: 10px 0 5px 0;"),
            );
            root = root.with_child(
                VideoWidget::create(VideoConfig::default())
                    .with_frames(frames.clone())
                    .dom()
                    .with_css(box_css.as_str()),
            );
        }
        OptionRefAny::None => {
            root = root.with_child(
                Dom::create_text("no decoded frames — test pattern (no Vulkan Video decode here):")
                    .with_css("font-size: 12px; color: #6a7080; margin: 10px 0 5px 0;"),
            );
            root = root.with_child(
                VideoWidget::create(VideoConfig::default())
                    .dom()
                    .with_css(box_css.as_str()),
            );
        }
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
        vw,
        vh,
    });
    let config = AppConfig::create();
    let app = App::create(data, config);
    let window = WindowCreateOptions::create(layout);
    app.run(window);
}
