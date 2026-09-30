//! Streaming H.264 decode worker for [`VideoWidget`](azul_layout::widgets::video::VideoWidget).
//!
//! Runs the decode on a background framework `Thread` (OFF the main thread),
//! exactly like the map widget's `tile_fetch_worker`. Frames are decoded
//! incrementally — there is NO up-front decode — and presented by a media
//! clock ([`VideoPlayback`](azul_layout::widgets::video::VideoPlayback)), so
//! late frames are dropped and the window opens immediately while playback
//! speed stays independent of decode/render speed.
//! [`ensure_video_decoder`] registers [`video_decode_worker`]; a `VideoWidget`
//! picks it up when it mounts.
//!
//! The worker runs wherever the MP4 demuxer is compiled (`video-native`): the
//! decode itself goes through [`VideoDecoder`](super::VideoDecoder), which
//! picks the platform's engine — Vulkan Video on x86_64 Linux/Windows,
//! `VideoToolbox` on Apple. Whatever stops a video (no engine in this build, a
//! failed download, a file that is no H.264 MP4, a decoder that yields no
//! frame) is reported to the widget as a [`VideoStatus`] with
//! [`VideoPhase::Failed`](azul_core::video::VideoPhase::Failed) and a message,
//! never as silence.

use azul_core::{refany::RefAny, task::ThreadReceiver, video::VideoStatus};
use azul_layout::{
    thread::{
        ThreadCallback, ThreadReceiveMsg, ThreadSender, ThreadWriteBackMsg, WriteBackCallback,
    },
    widgets::video::video_status_writeback,
};
#[cfg(feature = "video-native")]
use azul_layout::widgets::video::{video_writeback, VideoPlayback};

/// FFI entry point the `VideoWidget::dom()` shim calls, mirroring
/// `map_widget_dom`: makes sure the streaming decode worker is registered, then
/// builds the widget, which installs the worker when it mounts. The worker lives
/// here in `azul-dll` (it pulls the gpu-video / mp4 dep tree kept out of
/// `azul-layout`). The decode itself is `video-native`-gated inside the worker;
/// this wrapper (and the worker fn) are always present so the `unified` path
/// resolves in every `cabi_internal` build.
pub fn video_widget_dom(widget: azul_layout::widgets::video::VideoWidget) -> azul_core::dom::Dom {
    ensure_video_decoder();
    widget.dom()
}

/// Install [`video_decode_worker`] as the decode worker every `VideoWidget`
/// picks up when it mounts, once. Called from the shared per-frame layout pass
/// (like the map's tile fetcher) and again defensively from
/// [`video_widget_dom`].
pub fn ensure_video_decoder() {
    static DONE: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    DONE.get_or_init(|| {
        let _ = azul_layout::widgets::video::register_video_decoder(ThreadCallback {
            cb: video_decode_worker,
            ctx: azul_core::refany::OptionRefAny::None,
        });
    });
}

/// Tell the widget where its video stands (`video_status_writeback` stores
/// it and hands it to the app's `on_status` hook). `false` once the widget is
/// gone.
fn send_status(sender: &mut ThreadSender, status: VideoStatus) -> bool {
    sender.send(ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg::new(
        WriteBackCallback::new(video_status_writeback),
        RefAny::new(status),
    )))
}

/// Background decode worker. `init` is a `RefAny` holding a
/// [`VideoDecodeInit`](azul_layout::widgets::video::VideoDecodeInit): the
/// `VideoConfig` whose source is a URL (fetched via an HTTP **range request**,
/// through the init's `HttpClient` when the widget has one), a file, or raw MP4
/// bytes.
///
/// It decodes the clip incrementally on this thread and streams frames to the
/// widget's `<img>` via `WriteBack` → `video_writeback`, paced by a media clock
/// with frame dropping, and reports every change of state via `WriteBack` →
/// `video_status_writeback`.
pub extern "C" fn video_decode_worker(init: RefAny, sender: ThreadSender, recv: ThreadReceiver) {
    #[cfg(feature = "video-native")]
    decode_stream(init, sender, recv);

    #[cfg(not(feature = "video-native"))]
    {
        // No demuxer in this build: the widget can never show a frame. Say so
        // on stderr once per process, and to the widget, which hands it to
        // the app's `on_status` hook.
        static ANNOUNCE: std::sync::Once = std::sync::Once::new();
        let reason = "This build of azul has no `video-native` feature, so H.264 video is \
                      compiled out. Rebuild with: cargo build -p azul-dll --features \
                      build-dll,video-native";
        ANNOUNCE.call_once(|| eprintln!("[azul][video] VideoWidget: {reason}"));
        let mut sender = sender;
        let _ = send_status(
            &mut sender,
            VideoStatus::failed(azul_css::AzString::from(reason.to_string())),
        );
        let _ = (init, recv);
    }
}

/// `VideoToolbox` decodes synchronously and hands each access unit's frame back
/// at once, in DECODE order; gpu-video reorders to DISPLAY order itself. The
/// worker places each frame by presentation time either way (see
/// [`VideoPlayback::push_frame`]), but only a decode-order decoder lets it
/// read that time off the access unit the frame came from.
#[cfg(feature = "video-native")]
const DECODER_EMITS_DECODE_ORDER: bool = cfg!(any(target_os = "macos", target_os = "ios"));

/// One control message from the widget, decoded.
#[cfg(feature = "video-native")]
enum Control {
    /// Nothing is waiting.
    Idle,
    /// The widget is gone: stop.
    Terminate,
    /// `NodeResized`: the new target size in device pixels.
    Resize((u32, u32)),
    /// A changed `VideoConfig::timestamp`.
    Seek(f32),
    /// A changed `VideoConfig::source`: start over with it.
    Source(azul_core::video::VideoSource),
    /// A flipped `VideoConfig::paused`.
    Transport(azul_layout::widgets::video::VideoTransport),
    /// A message this worker does not know.
    Other,
}

/// The next control message from the widget, without blocking.
#[cfg(feature = "video-native")]
fn next_control(recv: &mut ThreadReceiver) -> Control {
    use azul_core::{
        task::{OptionThreadSendMsg, ThreadSendMsg},
        video::VideoSource,
    };
    use azul_layout::widgets::video::VideoTransport;

    match recv.recv() {
        OptionThreadSendMsg::None => Control::Idle,
        OptionThreadSendMsg::Some(ThreadSendMsg::TerminateThread) => Control::Terminate,
        OptionThreadSendMsg::Some(ThreadSendMsg::Custom(mut r)) => {
            if let Some(size) = r.downcast_ref::<(u32, u32)>().map(|t| *t) {
                Control::Resize(size)
            } else if let Some(ts) = r.downcast_ref::<f32>().map(|t| *t) {
                Control::Seek(ts)
            } else if let Some(src) = r.downcast_ref::<VideoSource>().map(|s| (*s).clone()) {
                Control::Source(src)
            } else if let Some(t) = r.downcast_ref::<VideoTransport>().map(|t| *t) {
                Control::Transport(t)
            } else {
                Control::Other
            }
        }
        OptionThreadSendMsg::Some(_) => Control::Other,
    }
}

/// What the worker keeps across decode sessions: what the app asked for.
#[cfg(feature = "video-native")]
struct Requested {
    source: azul_core::video::VideoSource,
    /// Where to start (a seek that arrived before the frames did).
    start_s: f32,
    paused: bool,
    looping: bool,
    /// The widget's size in device pixels, once it said.
    target_size: Option<(u32, u32)>,
}

/// After a failure: wait for the app to act. `None`: stop the worker. `Some`:
/// start a new session - on a new source, or on the same one when the app
/// pressed play again (a retry).
#[cfg(feature = "video-native")]
fn wait_for_retry(recv: &mut ThreadReceiver, requested: &mut Requested) -> Option<()> {
    use azul_layout::widgets::video::VideoTransport;

    loop {
        match next_control(recv) {
            Control::Terminate => return None,
            Control::Source(src) => {
                requested.source = src;
                requested.start_s = 0.0;
                return Some(());
            }
            Control::Transport(VideoTransport::Resume) => {
                requested.paused = false;
                return Some(());
            }
            Control::Transport(VideoTransport::Pause) => requested.paused = true,
            Control::Seek(ts) => requested.start_s = ts,
            Control::Resize(size) => {
                if size.0 > 0 && size.1 > 0 {
                    requested.target_size = Some(size);
                }
            }
            Control::Other => {}
            Control::Idle => std::thread::sleep(std::time::Duration::from_millis(50)),
        }
    }
}

/// Fetch, read or take the MP4 bytes and demux them. `Err` is a message for
/// the user.
#[cfg(feature = "video-native")]
fn load_stream(
    source: &azul_core::video::VideoSource,
    client: &azul_layout::http::OptionHttpClient,
    log: bool,
) -> Result<super::demux::DemuxedH264, String> {
    use azul_core::video::VideoSource;

    if let Err(why) = super::decode_engine() {
        return Err(format!("This build of azul cannot decode H.264 video: {why}."));
    }
    let bytes: Vec<u8> = match source {
        VideoSource::Url(u) => {
            if log {
                eprintln!("[vstream] fetching (Range: bytes=0-) {}", u.as_str());
            }
            fetch_ranged(u.as_str(), client)
                .map_err(|e| format!("Could not download the video: {e}"))?
        }
        VideoSource::File(p) => {
            if log {
                eprintln!("[vstream] reading file {}", p.as_str());
            }
            std::fs::read(p.as_str())
                .map_err(|e| format!("Could not read the video file {}: {e}", p.as_str()))?
        }
        VideoSource::Bytes(b) => b.as_ref().to_vec(),
    };
    if log {
        eprintln!("[vstream] got {} bytes", bytes.len());
    }
    let demuxed = super::demux::demux_mp4_h264(&bytes)
        .map_err(|e| format!("This is not a playable H.264 MP4 video ({e})."))?;
    if demuxed.chunks.is_empty() {
        return Err(String::from("The video has no frames."));
    }
    Ok(demuxed)
}

#[cfg(feature = "video-native")]
#[allow(clippy::too_many_lines)] // one decode session, read top to bottom
fn decode_stream(mut init: RefAny, mut sender: ThreadSender, mut recv: ThreadReceiver) {
    use std::time::{Duration, Instant};

    use azul_core::video::{OptionVideoFrame, VideoFrame};
    use azul_css::{AzString, U8Vec};
    use azul_layout::widgets::video::{VideoDecodeInit, VideoTransport};

    let log = std::env::var("AZ_VIDEO_FRAMELOG").is_ok();

    // 1. The thread-init is the `VideoConfig`; match its typed source → MP4 bytes (URL via range
    //    request / local file / in-memory bytes). No RefAny downcast ambiguity — the source is
    //    strongly typed.
    let (config, client) = match init.downcast_ref::<VideoDecodeInit>() {
        Some(i) => (i.config.clone(), i.client.clone()),
        None => {
            if log {
                eprintln!("[vstream] init is not a VideoDecodeInit");
            }
            return;
        }
    };
    let mut requested = Requested {
        source: config.source.clone(),
        start_s: config.timestamp,
        // `autoplay: false` holds the first frame as a poster.
        paused: config.paused || !config.autoplay,
        looping: config.looping,
        target_size: None,
    };
    // The media clock's time base: seconds since the worker started.
    let epoch = Instant::now();
    let now = || epoch.elapsed().as_secs_f64();

    // The widget can swap the source live (merge → ThreadSendMsg::Custom(VideoSource));
    // re-init the decode for the new source. What the app asked for persists across sources.
    'session: loop {
        if !send_status(&mut sender, VideoStatus::loading()) {
            return; // widget gone
        }
        let demuxed = match load_stream(&requested.source, &client, log) {
            Ok(d) => d,
            Err(message) => {
                if log {
                    eprintln!("[vstream] {message}");
                }
                if !send_status(&mut sender, VideoStatus::failed(AzString::from(message))) {
                    return;
                }
                match wait_for_retry(&mut recv, &mut requested) {
                    Some(()) => continue 'session,
                    None => return,
                }
            }
        };
        let total = demuxed.chunks.len();
        let fps = if demuxed.fps > 0.0 { demuxed.fps } else { 30.0 };
        let frame_s = 1.0 / fps;
        // Presentation times in seconds, from the first frame SHOWN (a B-frame
        // stream's first frame is shown a little after 0; its edit list, which
        // the demuxer does not read, is what moves it there).
        let first_ms = demuxed
            .chunks
            .iter()
            .map(|c| c.pts_ms)
            .fold(f64::INFINITY, f64::min);
        let chunk_pts: Vec<f32> = demuxed
            .chunks
            .iter()
            .map(|c| ((c.pts_ms - first_ms) / 1000.0) as f32)
            .collect();
        let mut display_pts = chunk_pts.clone();
        display_pts.sort_by(f32::total_cmp);
        let duration = display_pts.last().map_or(0.0, |last| last + frame_s);
        if log {
            eprintln!("[vstream] demuxed {total} chunks @ {fps:.1} fps, {duration:.2} s");
        }

        // 2. Open the platform decoder and stream-decode, presenting by the media clock.
        let decoder = super::VideoDecoder::open(false /* h264 */);
        if !decoder.is_open() {
            let message = "The H.264 decoder did not open on this machine.";
            if log {
                eprintln!("[vstream] {message}");
            }
            if !send_status(&mut sender, VideoStatus::failed(AzString::from(message))) {
                return;
            }
            match wait_for_retry(&mut recv, &mut requested) {
                Some(()) => continue 'session,
                None => return,
            }
        }
        let mut playback =
            VideoPlayback::new(requested.start_s, requested.paused, requested.looping);
        playback.set_duration(duration);
        playback.set_frame_interval(frame_s);
        let mut chunk_idx = 0usize;
        // Frames handed back so far, in the order they came.
        let mut emitted = 0usize;
        // The newest presentation time decoded since the last (re)start: the
        // schedule's decode gate reads it (`wants_frame`, `restart_wanted`).
        let mut decoded_until: Option<f32> = None;
        // The restart under way, so a target the clock keeps moving past is
        // restarted for once, not on every pass until its frames arrive.
        let mut restart_target: Option<f32> = None;
        // The display-order time of the n-th frame a display-order decoder hands back.
        let display_order_pts = |n: usize| -> f32 {
            display_pts
                .get(n)
                .copied()
                .unwrap_or_else(|| n as f32 * frame_s)
        };

        loop {
            // Drain control messages from the main thread (non-blocking).
            loop {
                match next_control(&mut recv) {
                    Control::Idle => break,
                    Control::Terminate => return,
                    Control::Resize(size) => {
                        if size.0 > 0 && size.1 > 0 && requested.target_size != Some(size) {
                            requested.target_size = Some(size);
                            playback.invalidate(); // re-present at the new size
                            if log {
                                eprintln!("[vstream] resize → target {}x{}", size.0, size.1);
                            }
                        }
                    }
                    Control::Seek(ts) => {
                        requested.start_s = ts;
                        playback.seek(ts, now());
                        restart_target = None;
                        if log {
                            eprintln!("[vstream] seek → {ts:.2}s");
                        }
                    }
                    Control::Source(src) => {
                        if log {
                            eprintln!("[vstream] source change → re-init");
                        }
                        requested.source = src;
                        requested.start_s = 0.0;
                        continue 'session;
                    }
                    Control::Transport(VideoTransport::Pause) => {
                        requested.paused = true;
                        playback.pause(now());
                    }
                    Control::Transport(VideoTransport::Resume) => {
                        requested.paused = false;
                        playback.resume(now());
                    }
                    Control::Other => {}
                }
            }

            // A seek back past the kept frames, a loop wrap, or a jump far
            // ahead: decode again from the keyframe at or before the target.
            match playback.restart_wanted(now(), decoded_until) {
                Some(target) if restart_target.is_none() => {
                    let k = keyframe_at_or_before(&demuxed.chunks, &chunk_pts, target, frame_s);
                    // What the decoder still holds is the old run's.
                    let mut f = decoder.flush();
                    while let OptionVideoFrame::Some(_) = f {
                        f = decoder.next_frame();
                    }
                    chunk_idx = k;
                    emitted = display_pts.partition_point(|p| *p < chunk_pts[k]);
                    decoded_until = None;
                    restart_target = Some(target);
                    playback.restart_decode();
                    if log {
                        eprintln!(
                            "[vstream] restart at chunk {k} ({:.2}s) for {target:.2}s",
                            chunk_pts[k]
                        );
                    }
                }
                Some(_) => {} // decoding towards it
                None => restart_target = None,
            }

            // Decode one access unit per pass while the schedule wants one
            // (the poster while held, a lookahead while playing - never the
            // whole clip), draining every frame it yields, and flushing the
            // reorder buffer after the final chunk.
            let decoding = chunk_idx < total && playback.wants_frame(now(), decoded_until);
            if decoding {
                let _accepted =
                    decoder.decode(U8Vec::from_vec(demuxed.chunks[chunk_idx].annexb.clone()));
                let mut nth = 0usize;
                let mut f = decoder.next_frame();
                while let OptionVideoFrame::Some(frame) = f {
                    let pts = if DECODER_EMITS_DECODE_ORDER {
                        chunk_pts[chunk_idx] + nth as f32 * frame_s
                    } else {
                        display_order_pts(emitted)
                    };
                    playback.push_frame(pts, frame);
                    decoded_until = Some(decoded_until.map_or(pts, |d| d.max(pts)));
                    emitted += 1;
                    nth += 1;
                    f = decoder.next_frame();
                }
                chunk_idx += 1;
                if chunk_idx == total {
                    let mut f = decoder.flush();
                    while let OptionVideoFrame::Some(frame) = f {
                        let pts = display_order_pts(emitted);
                        playback.push_frame(pts, frame);
                        decoded_until = Some(decoded_until.map_or(pts, |d| d.max(pts)));
                        emitted += 1;
                        f = decoder.next_frame();
                    }
                    playback.finish();
                    if emitted == 0 {
                        // The engine opened and took every access unit, and
                        // not one frame came out.
                        let message = format!(
                            "The {} H.264 decoder produced no picture from this video \
                             (the hardware decoder could not be opened, or does not support \
                             this stream).",
                            super::backend()
                        );
                        if log {
                            eprintln!("[vstream] {message}");
                        }
                        if !send_status(&mut sender, VideoStatus::failed(AzString::from(message)))
                        {
                            return;
                        }
                        match wait_for_retry(&mut recv, &mut requested) {
                            Some(()) => continue 'session,
                            None => return,
                        }
                    }
                }
            }

            // Present the frame the clock is on, and tell the app what changed.
            let tick = playback.tick(now());
            if let Some(index) = tick.present {
                if let Some(frame) = playback.frame(index) {
                    // Scale to the widget's requested size OFF this thread so the UI does
                    // no interpolation (the `<img>` shows it 1:1).
                    let frame: VideoFrame = match requested.target_size {
                        Some((tw, th)) if (tw, th) != (frame.width, frame.height) => {
                            scale_frame_bilinear(frame, tw, th)
                        }
                        _ => frame.clone(),
                    };
                    let sent = sender.send(ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg::new(
                        WriteBackCallback::new(video_writeback),
                        RefAny::new(frame),
                    )));
                    if !sent {
                        return; // widget gone → stop the worker entirely
                    }
                }
            }
            if let Some(status) = tick.status {
                if log {
                    eprintln!(
                        "[vstream] {:?} {:.2}/{:.2}s decoded={}/{} held={}",
                        status.phase,
                        status.position_s,
                        status.duration_s,
                        chunk_idx,
                        total,
                        playback.frames_held()
                    );
                }
                if !send_status(&mut sender, status) {
                    return;
                }
            }

            // The frames the clock has passed go (the one on screen stays).
            let _ = playback.trim(now());

            // Pace when there is nothing to decode (the gate is shut, or the
            // clip is in): slower while held, when only a control message can
            // change anything. While decoding, loop fast - the decode itself
            // is the work - to stay ahead of the clock.
            if !decoding {
                let idle_ms = if requested.paused { 30 } else { 8 };
                std::thread::sleep(Duration::from_millis(idle_ms));
            }
        }
    }
}

/// The decode-order index of the last keyframe shown at or before
/// `target_s` (the first chunk when none is), where a restarted decode
/// begins: an IDR needs no earlier frame.
#[cfg(feature = "video-native")]
fn keyframe_at_or_before(
    chunks: &[super::demux::H264Chunk],
    chunk_pts: &[f32],
    target_s: f32,
    frame_s: f32,
) -> usize {
    (0..chunks.len().min(chunk_pts.len()))
        .rev()
        .find(|&i| chunks[i].is_keyframe && chunk_pts[i] <= target_s + frame_s * 0.5)
        .unwrap_or(0)
}

/// Resize a decoded `VideoFrame` to `tw`×`th`, on the decode thread, so the
/// UI renderer doesn't have to interpolate (the `<img>` shows it 1:1). The
/// one frame scaler (`image_scale::resample_frame`): the frame keeps its
/// format, and a downscale area-averages instead of skipping pixels.
#[cfg(feature = "video-native")]
fn scale_frame_bilinear(
    src: &azul_core::video::VideoFrame,
    tw: u32,
    th: u32,
) -> azul_core::video::VideoFrame {
    use azul_css::U8Vec;
    use azul_layout::image_scale::{frame_output_format, resample_frame, SrcImage};
    let view = SrcImage {
        bytes: src.bytes.as_ref(),
        format: src.format,
        width: src.width,
        height: src.height,
    };
    let out = resample_frame(&view, tw, th);
    if out.is_empty() {
        return src.clone();
    }
    azul_core::video::VideoFrame::with_format(
        tw,
        th,
        U8Vec::from_vec(out),
        frame_output_format(src.format),
    )
}

/// Fetch `url` via an HTTP **range request** (`Range: bytes=0-`). BBB is small so
/// a single open-ended range fetches the whole clip in one 206 response; the point
/// is that loading goes through a real range request (progressive byte-range
/// streaming is a future refinement). `Err` is the transport's own description.
#[cfg(feature = "video-native")]
fn fetch_ranged(url: &str, client: &azul_layout::http::OptionHttpClient) -> Result<Vec<u8>, String> {
    use azul_css::AzString;
    use azul_layout::http::{HttpRequestConfig, OptionHttpClient, ResultU8VecHttpError};
    let mut cfg = HttpRequestConfig::new().with_header("Range", "bytes=0-");
    if let OptionHttpClient::Some(client) = client {
        cfg = cfg.with_client(client.clone());
    }
    match cfg.download_bytes_blocking(AzString::from(url.to_string())) {
        ResultU8VecHttpError::Ok(b) => Ok(b.as_slice().to_vec()),
        ResultU8VecHttpError::Err(e) => Err(e.to_string()),
    }
}

#[cfg(test)]
mod stream_tests {
    use std::sync::{mpsc::channel, Mutex, PoisonError};

    use azul_core::{
        refany::RefAny,
        task::{
            OptionThreadSendMsg, ThreadReceiver, ThreadReceiverDestructorCallback,
            ThreadReceiverInner, ThreadRecvCallback, ThreadSendMsg,
        },
        video::{VideoConfig, VideoPhase, VideoSource, VideoStatus},
    };
    use azul_layout::{
        http::OptionHttpClient,
        thread::{
            ThreadReceiveMsg, ThreadSendCallback, ThreadSender, ThreadSenderDestructorCallback,
            ThreadSenderInner,
        },
        widgets::video::VideoDecodeInit,
    };

    /// Every `VideoStatus` the worker wrote back. A send callback is a plain
    /// C fn pointer, so a static is the only place it can put them.
    static STATUSES: Mutex<Vec<VideoStatus>> = Mutex::new(Vec::new());

    extern "C" fn record_status(_sender: *const core::ffi::c_void, msg: ThreadReceiveMsg) -> bool {
        if let ThreadReceiveMsg::WriteBack(mut wb) = msg {
            if let Some(status) = wb.refany.downcast_ref::<VideoStatus>() {
                STATUSES
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .push((*status).clone());
            }
        }
        true
    }

    extern "C" fn sender_drop_noop(_: *mut ThreadSenderInner) {}
    extern "C" fn receiver_drop_noop(_: *mut ThreadReceiverInner) {}

    /// Answers every poll with "terminate": a worker that waits for a retry
    /// after a failure returns at once instead of hanging the test.
    extern "C" fn recv_terminate(_: *const core::ffi::c_void) -> OptionThreadSendMsg {
        OptionThreadSendMsg::Some(ThreadSendMsg::TerminateThread)
    }

    /// A video that cannot play says why. Four bytes are no MP4: whichever
    /// way this build fails - no decoder compiled in, or a demux error - the
    /// widget must hear a `Failed` status with a message for the user, not
    /// silence. Silence was all it got: the worker returned without a word,
    /// and on macOS it was compiled out entirely, so the AzWidgets video card
    /// could only ever show a grey tile.
    #[test]
    fn a_video_that_cannot_play_says_why() {
        let (tx, _rx) = channel::<ThreadReceiveMsg>();
        let sender = ThreadSender::new(ThreadSenderInner {
            ptr: Box::new(tx),
            send_fn: ThreadSendCallback { cb: record_status },
            destructor: ThreadSenderDestructorCallback {
                cb: sender_drop_noop,
            },
        });
        let (_ctl, ctl_rx) = channel::<ThreadSendMsg>();
        let recv = ThreadReceiver::new(ThreadReceiverInner {
            ptr: Box::new(ctl_rx),
            recv_fn: ThreadRecvCallback { cb: recv_terminate },
            destructor: ThreadReceiverDestructorCallback {
                cb: receiver_drop_noop,
            },
        });
        let init = RefAny::new(VideoDecodeInit {
            config: VideoConfig::new(VideoSource::Bytes(vec![0xde_u8, 0xad, 0xbe, 0xef].into())),
            client: OptionHttpClient::None,
        });

        super::video_decode_worker(init, sender, recv);

        let statuses = STATUSES
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        let failed = statuses
            .iter()
            .find(|s| s.phase == VideoPhase::Failed)
            .unwrap_or_else(|| panic!("no Failed status, the worker reported only {statuses:?}"));
        assert!(
            !failed.message.as_str().is_empty(),
            "a failure must say why, in words for the user"
        );
    }
}
