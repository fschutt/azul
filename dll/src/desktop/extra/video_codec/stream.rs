//! Streaming H.264 decode worker for [`VideoWidget`](azul_layout::widgets::video::VideoWidget).
//!
//! Runs the Vulkan-Video decode on a background framework `Thread` (OFF the main
//! thread), exactly like the map widget's `tile_fetch_worker`. Frames are decoded
//! incrementally — there is NO up-front decode — and presented by WALL-CLOCK, so
//! late frames are dropped and the window opens immediately while playback speed
//! stays independent of decode/render speed. Pass [`video_decode_worker`] to
//! [`VideoWidget::dom_with_decoder`](azul_layout::widgets::video::VideoWidget::dom_with_decoder)
//! wrapped in a `ThreadCallback`, exactly like `MapWidget::dom_with_fetch`.

use azul_core::refany::RefAny;
use azul_core::task::ThreadReceiver;
use azul_layout::thread::{
    ThreadCallback, ThreadReceiveMsg, ThreadSender, ThreadWriteBackMsg, WriteBackCallback,
};
use azul_layout::widgets::video::video_writeback;

/// FFI entry point the `VideoWidget::dom()` shim calls — wires the off-main
/// streaming decode worker, mirroring `map_widget_dom`. The worker lives here in
/// `azul-dll` (it pulls the gpu-video / mp4 dep tree kept out of `azul-layout`),
/// so `VideoWidget::dom()` in layout can only produce a placeholder; the real
/// streaming decode is injected here via the layout-internal `dom_with_decoder`
/// plumbing. The decode itself is `video-native`-gated inside the worker; this
/// wrapper (and the worker fn) are always present so the `unified` path resolves
/// in every `cabi_internal` build.
pub fn video_widget_dom(
    widget: azul_layout::widgets::video::VideoWidget,
) -> azul_core::dom::Dom {
    widget.dom_with_decoder(ThreadCallback {
        cb: video_decode_worker,
        ctx: azul_core::refany::OptionRefAny::None,
    })
}

/// Background decode worker. `init` is a `RefAny` holding the source:
/// - a `String` — a URL, fetched via an HTTP **range request**, or
/// - a `Vec<u8>` — raw MP4 bytes (e.g. a bundled/local sample).
///
/// It decodes the clip incrementally on this thread and streams frames to the
/// widget's `<img>` via `WriteBack` → `video_writeback` → `present_frame`,
/// paced by wall-clock with frame dropping.
pub extern "C" fn video_decode_worker(init: RefAny, sender: ThreadSender, _recv: ThreadReceiver) {
    #[cfg(all(
        feature = "video-native",
        target_arch = "x86_64",
        any(target_os = "linux", target_os = "windows")
    ))]
    decode_stream(init, sender);

    #[cfg(not(all(
        feature = "video-native",
        target_arch = "x86_64",
        any(target_os = "linux", target_os = "windows")
    )))]
    {
        // No Vulkan-Video decoder on this target: the widget keeps its placeholder.
        let _ = (init, sender);
    }
}

#[cfg(all(
    feature = "video-native",
    target_arch = "x86_64",
    any(target_os = "linux", target_os = "windows")
))]
fn decode_stream(mut init: RefAny, mut sender: ThreadSender) {
    use azul_core::video::{OptionVideoFrame, VideoFrame};
    use azul_css::U8Vec;
    use std::time::{Duration, Instant};

    let log = std::env::var("AZ_VIDEO_FRAMELOG").is_ok();

    // 1. The thread-init is the `VideoConfig`; match its typed source → MP4 bytes
    //    (URL via range request / local file / in-memory bytes). No RefAny downcast
    //    ambiguity — the source is strongly typed.
    use azul_core::video::VideoSource;
    let config = match init.downcast_ref::<azul_core::video::VideoConfig>() {
        Some(c) => c.clone(),
        None => {
            if log {
                eprintln!("[vstream] init is not a VideoConfig");
            }
            return;
        }
    };
    let bytes: Vec<u8> = match &config.source {
        VideoSource::Url(u) => {
            if log {
                eprintln!("[vstream] fetching (Range: bytes=0-) {}", u.as_str());
            }
            match fetch_ranged(u.as_str()) {
                Some(b) => b,
                None => {
                    if log {
                        eprintln!("[vstream] fetch FAILED");
                    }
                    return;
                }
            }
        }
        VideoSource::File(p) => {
            if log {
                eprintln!("[vstream] reading file {}", p.as_str());
            }
            match std::fs::read(p.as_str()) {
                Ok(b) => b,
                Err(e) => {
                    if log {
                        eprintln!("[vstream] file read FAILED: {}", e);
                    }
                    return;
                }
            }
        }
        VideoSource::Bytes(b) => b.as_ref().to_vec(),
    };
    if log {
        eprintln!("[vstream] got {} bytes", bytes.len());
    }

    // 2. Demux to H.264 Annex-B access units.
    let demuxed = match super::demux::demux_mp4_h264(&bytes) {
        Ok(d) => d,
        Err(e) => {
            if log {
                eprintln!("[vstream] demux FAILED: {}", e);
            }
            return;
        }
    };
    let total = demuxed.chunks.len();
    if total == 0 {
        if log {
            eprintln!("[vstream] demux produced 0 chunks");
        }
        return;
    }
    let fps = if demuxed.fps > 0.0 { demuxed.fps } else { 30.0 };
    if log {
        eprintln!("[vstream] demuxed {} chunks @ {:.1} fps", total, fps);
    }

    // 3. Open the VK decoder and stream-decode, presenting by wall-clock.
    let decoder = super::VideoDecoder::open(false /* h264 */);
    let mut decoded: Vec<VideoFrame> = Vec::new();
    let mut chunk_idx = 0usize;
    let mut last_idx = usize::MAX;
    let start = Instant::now();

    loop {
        // Decode one access unit per iteration (drain every frame it yields),
        // flushing the reorder buffer after the final chunk.
        if chunk_idx < total {
            let mut f = decoder.decode(U8Vec::from_vec(demuxed.chunks[chunk_idx].annexb.clone()));
            while let OptionVideoFrame::Some(frame) = f {
                decoded.push(frame);
                f = decoder.next_frame();
            }
            chunk_idx += 1;
            if chunk_idx == total {
                let mut f = decoder.flush();
                while let OptionVideoFrame::Some(frame) = f {
                    decoded.push(frame);
                    f = decoder.next_frame();
                }
            }
        }

        if !decoded.is_empty() {
            let target = (start.elapsed().as_secs_f32() * fps) as usize;
            // Still decoding → clamp to the newest decoded frame (decode-bound).
            // Fully decoded → loop the clip by wall-clock, dropping late frames.
            let idx = if chunk_idx < total {
                target.min(decoded.len() - 1)
            } else {
                target % decoded.len()
            };
            if idx != last_idx {
                if log && idx % 30 == 0 {
                    eprintln!(
                        "[vstream] t={:.2}s decoded={}/{} present_frame={}",
                        start.elapsed().as_secs_f32(),
                        decoded.len(),
                        total,
                        idx
                    );
                }
                last_idx = idx;
                let sent = sender.send(ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg::new(
                    WriteBackCallback::new(video_writeback),
                    RefAny::new(decoded[idx].clone()),
                )));
                if !sent {
                    break; // widget gone → stop the thread
                }
            }
        }

        // Once fully decoded, pace so we don't busy-spin; while still decoding,
        // loop fast (the decode itself is the work) to catch up to real time.
        if chunk_idx >= total {
            std::thread::sleep(Duration::from_millis(8));
        }
    }
}

/// Fetch `url` via an HTTP **range request** (`Range: bytes=0-`). BBB is small so
/// a single open-ended range fetches the whole clip in one 206 response; the point
/// is that loading goes through a real range request (progressive byte-range
/// streaming is a future refinement).
#[cfg(all(
    feature = "video-native",
    target_arch = "x86_64",
    any(target_os = "linux", target_os = "windows")
))]
fn fetch_ranged(url: &str) -> Option<Vec<u8>> {
    use azul_css::AzString;
    use azul_layout::http::{HttpRequestConfig, ResultU8VecHttpError};
    let cfg = HttpRequestConfig::new().with_header("Range", "bytes=0-");
    match cfg.download_bytes(AzString::from(url.to_string())) {
        ResultU8VecHttpError::Ok(b) => Some(b.as_slice().to_vec()),
        ResultU8VecHttpError::Err(_) => None,
    }
}
