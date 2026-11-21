//! The call's video: the camera and the screen encoded per rendition someone shows (H.264 where an
//! encoder works, JPEG otherwise; the rules are `video_wire.rs`), each peer's streams decoded into
//! its tiles, keyframe requests after a loss.

use super::*;

/// What this machine does with H.264, found once at start by [`probe_video`].
#[derive(Clone)]
pub(crate) struct VideoSupport {
    /// The H.264 encoder's backend ("VideoToolbox") when it encodes here; else why JPEG is sent.
    pub(crate) encoder: Result<String, String>,
    /// This machine decodes H.264: tested with a keyframe where it also encodes, else the
    /// platform's word, judged again on the first stream (`ReceiveTrack::decoder_is_inert`).
    pub(crate) decodes_h264: bool,
}

/// The sending side of one local video track (the camera or the screen share).
pub(crate) struct VideoOut {
    /// The H.264 encoder, opened with the first frame some peer takes as H.264.
    pub(crate) encoder: Option<VideoEncoder>,
    /// The frame size the encoder was opened for.
    pub(crate) size: (u32, u32),
    pub(crate) health: video_wire::EncoderHealth,
    pub(crate) keyframes: video_wire::KeyframePolicy,
    /// The `seq` of the last H.264 packet and of the last JPEG frame; the `frame_no` of the last
    /// frame captured while someone listened. They go on across a stop, so a peer sees one stream.
    pub(crate) h264_seq: u32,
    pub(crate) jpeg_seq: u32,
    pub(crate) frame_no: u32,
    pub(crate) h264_packets: u64,
    pub(crate) jpeg_frames: u64,
    /// Packets dropped with the "Drop a video packet" button.
    pub(crate) dropped: u64,
    /// The `frame_no` of every frame in the encoder whose packet has not come out yet, oldest
    /// first (the encoder works on its own thread).
    pub(crate) in_encoder: std::collections::VecDeque<u32>,
    /// What the encoder may spend: the rendition's ladder rate, lowered while a receiver's link
    /// does not keep up ([`adapt_rates`]).
    pub(crate) rate: rate::RateControl,
}

impl VideoOut {
    /// The sending side of a rendition `rendition` lines tall.
    pub(crate) fn new(rendition: u16) -> Self {
        VideoOut {
            encoder: None,
            size: (0, 0),
            health: video_wire::EncoderHealth::default(),
            keyframes: video_wire::KeyframePolicy::new(),
            h264_seq: 0,
            jpeg_seq: 0,
            frame_no: 0,
            h264_packets: 0,
            jpeg_frames: 0,
            dropped: 0,
            in_encoder: std::collections::VecDeque::new(),
            rate: rate::RateControl::new(IrohLoadBalancer::rendition_kbps(u32::from(rendition))),
        }
    }

    /// The track stopped (camera or share off, or Leave): the encoder closes, and the next start
    /// opens a fresh one, which begins with a keyframe.
    pub(crate) fn stop(&mut self) {
        self.encoder = None;
        self.in_encoder.clear();
        self.health = video_wire::EncoderHealth::default();
    }
}

/// The receiving side of one of a peer's video streams (a track in one rendition).
pub(crate) struct VideoIn {
    pub(crate) rules: video_wire::ReceiveTrack,
    /// Opened with the first H.264 packet to decode.
    pub(crate) decoder: Option<VideoDecoder>,
    /// The codec of the packet that arrived last.
    pub(crate) seen: Option<Codec>,
    /// The connection the last packet came through: the origin's own, or a forwarder's.
    pub(crate) via: u64,
    /// This side shows the stream; else it only passes it on.
    pub(crate) shown: bool,
    /// The size the decoder was last asked to hand frames out at (the tile's device pixels).
    pub(crate) output_size: Option<(u32, u32)>,
}

impl VideoIn {
    pub(crate) fn new() -> Self {
        VideoIn {
            rules: video_wire::ReceiveTrack::new(),
            decoder: None,
            seen: None,
            via: 0,
            shown: false,
            output_size: None,
        }
    }
}

/// Milliseconds on this participant's clock.
pub(crate) fn now_ms(s: &MeetState) -> u64 {
    s.clock.elapsed().as_millis() as u64
}

/// A video frame as an image to encode or show, in its own format (NV12,
/// BGRA8 or RGBA8). Video is opaque, for which straight == premultiplied: no
/// per-pixel multiply when it is loaded, no conversion.
pub(crate) fn frame_image(frame: VideoFrame) -> RawImage {
    RawImage {
        pixels: RawImageData::U8(frame.bytes),
        width: frame.width as usize,
        height: frame.height as usize,
        premultiplied_alpha: true,
        data_format: frame.format,
        tag: U8Vec::create(),
    }
}

/// Frame `index` of the test pattern, `width` x `height`.
pub(crate) fn pattern_frame(index: u32, width: u32, height: u32) -> VideoFrame {
    VideoFrame {
        width,
        height,
        bytes: U8Vec::from(video_wire::test_pattern(width, height, index)),
        format: RawImageFormat::RGBA8,
    }
}

/// Finds out once, at start, what this machine does with H.264: an encoder that opens must turn
/// a test frame into a keyframe, and a decoder must turn that keyframe back into a picture.
/// `--video-codec jpeg` (`AZMEET_VIDEO_CODEC=jpeg`) switches H.264 off.
pub(crate) fn probe_video() -> VideoSupport {
    if setting("AZMEET_VIDEO_CODEC").is_some_and(|v| v.eq_ignore_ascii_case("jpeg")) {
        return VideoSupport {
            encoder: Err(String::from("H.264 switched off")),
            decodes_h264: false,
        };
    }
    let backend = VideoEncoder::backend_name().as_str().to_string();
    let probed = if backend == "none" {
        None
    } else {
        probe_encode()
    };
    let decodes_h264 = match &probed {
        Some((keyframe, _)) => probe_decode(keyframe),
        None => PlatformCapability::video_codec().available,
    };
    VideoSupport {
        // "VideoToolbox, hardware": the codec line says where the encoding runs.
        encoder: probed
            .map(|(_, hardware)| {
                format!("{backend}, {}", if hardware { "hardware" } else { "software" })
            })
            .ok_or_else(|| String::from("no encoder")),
        decodes_h264,
    }
}

/// A keyframe of the test pattern from a fresh H.264 encoder, and whether that encoder runs in
/// hardware; `None` where nothing, or no keyframe, comes out. `VideoEncoder::open` hands out an
/// open handle that never yields a packet where no backend is built in, so opening proves
/// nothing.
pub(crate) fn probe_encode() -> Option<(Vec<u8>, bool)> {
    let mut encoder = VideoEncoder::open(FEED_W, FEED_H, false, VIDEO_KBPS);
    if !encoder.is_open() {
        return None;
    }
    let mut chunk = Vec::new();
    for index in 0..3 {
        encoder.encode(pattern_frame(index, FEED_W, FEED_H), true);
        // The encoder works on its own thread: wait for this frame's packets.
        encoder.flush();
        while let Some(packet) = encoder.recv_packet().into_option() {
            chunk.extend_from_slice(packet.as_slice());
        }
        if !chunk.is_empty() {
            break;
        }
    }
    let hardware = encoder.is_hardware();
    encoder.close();
    video_wire::h264_is_keyframe(&chunk).then_some((chunk, hardware))
}

/// Whether a fresh decoder turns `keyframe` back into a picture.
pub(crate) fn probe_decode(keyframe: &[u8]) -> bool {
    let mut decoder = VideoDecoder::open(false);
    if !decoder.is_open() {
        return false;
    }
    decoder.set_output_format(VIDEO_FORMAT);
    decoder.decode(U8Vec::from(keyframe.to_vec()));
    // The decoder works on its own thread: `flush` waits for the picture.
    let decoded = decoder.flush().into_option().is_some();
    decoder.close();
    decoded
}

/// The video settings of this run: `--test-pattern` (`AZMEET_TEST_PATTERN=1`) makes the camera a
/// test pattern, switches it on, and shows the "Drop a video packet" button; a headless run uses
/// test patterns too (off until switched on), so it never opens a camera or a screen.
pub(crate) fn configure_video(s: &mut MeetState, support: &VideoSupport) {
    let pattern = setting_on("AZMEET_TEST_PATTERN");
    let devices = devices_allowed();
    s.video = support.clone();
    s.pattern_video = pattern || !devices;
    if s.pattern_video && s.pattern_still.is_none() {
        let (width, height) = PATTERN_STILL;
        s.pattern_still =
            ImageRef::create_rawimage(frame_image(pattern_frame(0, width, height))).into_option();
    }
    s.video_debug = pattern;
    if pattern {
        s.cam_on = true;
    }
    eprintln!(
        "[azmeet] {}: {}; decodes H.264: {}",
        s.name,
        codec_status(s),
        if s.video.decodes_h264 { "yes" } else { "no" }
    );
    if !devices {
        eprintln!(
            "[azmeet] {}: no camera or screen is opened: both are test patterns",
            s.name
        );
    } else if pattern {
        eprintln!(
            "[azmeet] {}: the camera is a test pattern (--test-pattern)",
            s.name
        );
    }
}

/// The codec line: H.264 and its backend (and JPEG to the peers that cannot decode it), or why
/// JPEG.
pub(crate) fn codec_status(s: &MeetState) -> String {
    let jpeg_to: Vec<String> = s
        .remotes
        .iter()
        .filter(|r| r.h264 == Some(false))
        .map(|r| remote_name(s, &r.node_id))
        .collect();
    video_wire::codec_line(s.video.encoder.as_deref().map_err(String::as_str), &jpeg_to)
}

/// Sends a captured frame (NV12 from the camera or the screen, RGBA from the test pattern) of the
/// `rendition` of `track` to whom the plan says gets it: as H.264 to those assigned H.264
/// (reliable messages, so nothing between two keyframes goes missing), as JPEG to the others (a
/// latest-wins frame each: every JPEG stands alone). A leaf sends each rendition once, to its
/// backbone parent. The frame is handed to the encoder as it is (moved, not copied, unless JPEG
/// peers need it too).
pub(crate) fn send_video(s: &mut MeetState, track: u32, rendition: u16, frame: VideoFrame) {
    let (width, frame_height) = (frame.width, frame.height);
    if track_slot(track).is_none() || s.remotes.is_empty() {
        return;
    }
    let Some(endpoint) = s.endpoint.clone() else {
        return;
    };
    let mut h264_peers = stream_targets(s, track, rendition, true);
    let mut jpeg_peers = stream_targets(s, track, rendition, false);
    if h264_peers.is_empty() && jpeg_peers.is_empty() {
        return;
    }
    // H.264 wants even sides; 16 pixels is the smallest VideoToolbox takes.
    let fits = width % 2 == 0 && frame_height % 2 == 0 && width >= 16 && frame_height >= 16;
    if !fits || s.video.encoder.is_err() {
        jpeg_peers.append(&mut h264_peers);
    }
    let out = s
        .video_out
        .entry((track, rendition))
        .or_insert_with(|| VideoOut::new(rendition));
    out.frame_no = out.frame_no.wrapping_add(1);
    // The encoder takes the frame by value: it is moved there, and copied
    // only when JPEG peers need it as well.
    let mut frame = Some(frame);
    if !h264_peers.is_empty() {
        let for_encoder = if jpeg_peers.is_empty() {
            frame.take()
        } else {
            frame.clone()
        };
        let sent = for_encoder
            .map(|f| send_h264(s, &endpoint, track, rendition, f, &h264_peers))
            .unwrap_or(false);
        if !sent {
            // No working encoder for this frame: these peers get JPEG (from the next frame on
            // when this one went into the encoder that just failed).
            jpeg_peers.extend(h264_peers);
        }
    }
    if !jpeg_peers.is_empty() {
        if let Some(frame) = frame {
            send_jpeg(s, &endpoint, track, rendition, frame, &jpeg_peers);
        }
    }
}

/// Hands the frame to the H.264 encoder of the `rendition` of `track` (it encodes on its own
/// thread) and sends whatever packets it has ready to the peers that get the stream
/// ([`drain_h264`]). `peers` (connection handles) are this frame's H.264 receivers: a new one, or
/// one that fell behind, makes the frame a keyframe. False when no encoder works (it did not
/// open, gives nothing back, or ignores keyframe requests): H.264 is given up, the frame goes as
/// JPEG, and everyone hears that this side no longer encodes it.
pub(crate) fn send_h264(
    s: &mut MeetState,
    endpoint: &IrohEndpoint,
    track: u32,
    rendition: u16,
    frame: VideoFrame,
    peers: &[u64],
) -> bool {
    let (width, height) = (frame.width, frame.height);
    let key = (track, rendition);
    let now = now_ms(s);
    // A new peer, or one that fell behind, starts at a keyframe.
    let resync = s.remotes.iter().any(|r| {
        peers.contains(&r.handle) && r.sent.get(&key).map_or(true, |w| w.wants_keyframe())
    });
    let opened = {
        let Some(out) = s.video_out.get_mut(&key) else {
            return false;
        };
        if resync {
            out.keyframes.request();
        }
        let resized = out.encoder.is_some() && out.size != (width, height);
        if out.keyframes.must_reopen() || resized {
            out.encoder = None;
            out.in_encoder.clear();
            out.keyframes.reopened();
        }
        if out.encoder.is_none() {
            // A re-opened encoder keeps the rate the link was found to carry.
            let kbps = out.rate.rate_kbps();
            let encoder = VideoEncoder::open(width, height, false, kbps);
            if encoder.is_open() {
                // For scripts (`scripts/azmeet_cpu.py`): where this rendition encodes.
                println!(
                    "AZMEET_ENCODER {} {width}x{height} {}",
                    rendition_label(track, rendition).replace(' ', "-"),
                    if encoder.is_hardware() {
                        "hardware"
                    } else {
                        "software"
                    }
                );
                out.encoder = Some(encoder);
                out.size = (width, height);
                out.health = video_wire::EncoderHealth::default();
                // A new encoder begins with a keyframe.
                out.keyframes.request();
            }
        }
        let frame_no = out.frame_no;
        match out.encoder.as_ref() {
            None => false,
            Some(encoder) => {
                let force = out.keyframes.should_force(now);
                if encoder.encode(frame, force) {
                    out.health.submitted();
                    out.in_encoder.push_back(frame_no);
                } else {
                    // The encoder still works on earlier frames: this one is dropped, and a
                    // keyframe it was to carry is forced on the next frame it takes.
                    out.keyframes.not_taken();
                }
                true
            }
        }
    };
    if !opened {
        give_up_h264(s, "the H.264 encoder did not open");
        return false;
    }
    drain_h264(s, endpoint, key)
}

/// Sends the packets the encoder of `key` (track, rendition height) has ready to the peers that
/// get that stream as H.264 now (each through its [`video_wire::SendWindow`]), numbered in the
/// order they come out. Called after every frame handed to it, and on every pump: the encoder
/// works on its own thread, so a frame's packets come out a little later. False when the
/// encoder turned out not to work (see [`send_h264`]).
pub(crate) fn drain_h264(s: &mut MeetState, endpoint: &IrohEndpoint, key: (u32, u16)) -> bool {
    let (track, rendition) = key;
    let now = now_ms(s);
    let (packets, failure) = {
        let Some(out) = s.video_out.get_mut(&key) else {
            return true;
        };
        let Some(encoder) = out.encoder.as_mut() else {
            return true;
        };
        let mut packets = Vec::new();
        while let Some(chunk) = encoder.recv_packet().into_option() {
            out.health.produced();
            let keyframe = video_wire::h264_is_keyframe(chunk.as_slice());
            out.keyframes.on_output(keyframe, now);
            out.h264_seq = out.h264_seq.wrapping_add(1);
            let frame_no = out.in_encoder.pop_front().unwrap_or(out.frame_no);
            let header = video_wire::Header {
                codec: Codec::H264,
                keyframe,
                track,
                seq: out.h264_seq,
                frame_no,
                height: rendition,
            };
            packets.push((header, video_wire::encode_packet(&header, chunk.as_slice())));
        }
        let failure = if out.health.is_inert() {
            Some("the H.264 encoder gives no packets back")
        } else if out.keyframes.is_broken() {
            Some("the H.264 encoder ignores keyframe requests")
        } else {
            None
        };
        (packets, failure)
    };
    if let Some(why) = failure {
        give_up_h264(s, why);
        return false;
    }
    if packets.is_empty() {
        return true;
    }
    let peers = stream_targets(s, track, rendition, true);
    for (header, packet) in packets {
        if !header.keyframe && s.drop_video.remove(&key) {
            if let Some(out) = s.video_out.get_mut(&key) {
                out.dropped += 1;
            }
            eprintln!(
                "[azmeet] {}: dropped H.264 packet {} of the {} on purpose",
                s.name,
                header.seq,
                rendition_label(track, rendition)
            );
            continue;
        }
        if let Some(out) = s.video_out.get_mut(&key) {
            out.h264_packets += 1;
        }
        for r in s.remotes.iter_mut().filter(|r| peers.contains(&r.handle)) {
            let window = r
                .sent
                .entry(key)
                .or_insert_with(video_wire::SendWindow::new);
            if window.offer(header.seq, header.keyframe) {
                endpoint.send_message(r.handle, packet.clone());
            }
        }
    }
    true
}

/// Every pump: the packets every encoder made since the last one go out.
pub(crate) fn drain_encoders(s: &mut MeetState, endpoint: &IrohEndpoint) {
    let keys: Vec<(u32, u16)> = s
        .video_out
        .iter()
        .filter(|(_, out)| out.encoder.is_some())
        .map(|(key, _)| *key)
        .collect();
    for key in keys {
        drain_h264(s, endpoint, key);
    }
    adapt_rates(s);
}

/// Every pump: each H.264 encoder's rate follows the worst queue (packets sent, not yet
/// acknowledged) among the peers that get its stream now ([`rate::RateControl`]); a new rate goes
/// to the encoder, which spends it from its next frame on - no restart, no keyframe.
pub(crate) fn adapt_rates(s: &mut MeetState) {
    let now = now_ms(s);
    let keys: Vec<(u32, u16)> = s
        .video_out
        .iter()
        .filter(|(_, out)| out.encoder.is_some())
        .map(|(key, _)| *key)
        .collect();
    for key in keys {
        let (track, rendition) = key;
        let peers = stream_targets(s, track, rendition, true);
        let in_flight: Vec<usize> = s
            .remotes
            .iter()
            .filter(|r| peers.contains(&r.handle))
            .filter_map(|r| r.sent.get(&key))
            .map(|window| window.in_flight())
            .collect();
        let Some(out) = s.video_out.get_mut(&key) else {
            continue;
        };
        for queued in in_flight {
            out.rate.observe(queued);
        }
        let Some(kbps) = out.rate.tick(now) else {
            continue;
        };
        if let Some(encoder) = out.encoder.as_ref() {
            let _ = encoder.set_bitrate(kbps);
        }
        // For scripts: the rate each rendition spends now.
        println!(
            "AZMEET_RATE {} {kbps}",
            rendition_label(track, rendition).replace(' ', "-")
        );
    }
}

/// No H.264 encoder works here (`why`): every encoder closes, JPEG from now on, and every peer
/// hears that this side no longer encodes H.264.
pub(crate) fn give_up_h264(s: &mut MeetState, why: &str) {
    eprintln!("[azmeet] {}: {why}: sending JPEG from now on", s.name);
    for out in s.video_out.values_mut() {
        out.stop();
    }
    s.video.encoder = Err(String::from(why));
    send_caps(s, &all_peers(s));
}

/// The connection handles that get this side's `rendition` of `track` as H.264 (`h264`) or as
/// JPEG, by the current plan: everyone assigned that stream in the full mesh, the backbone parent
/// for a leaf.
pub(crate) fn stream_targets(s: &MeetState, track: u32, rendition: u16, h264: bool) -> Vec<u64> {
    let assigned = assignment(s, s.me, track);
    let stream = routes::Stream {
        height: rendition,
        h264,
    };
    handles_of(s, &s.plan.next_hops(s.me, s.me, stream, &assigned))
}

/// Sends the frame as JPEG to `peers`, a latest-wins frame each, on the rendition's own frame track.
pub(crate) fn send_jpeg(
    s: &mut MeetState,
    endpoint: &IrohEndpoint,
    track: u32,
    rendition: u16,
    frame: VideoFrame,
    peers: &[u64],
) {
    let key = (track, rendition);
    // `encode_jpeg` takes any frame format (NV12 is converted to RGB for it).
    let ResultU8VecEncodeImageError::Ok(jpeg) = frame_image(frame).encode_jpeg(JPEG_QUALITY)
    else {
        return;
    };
    let header = {
        let Some(out) = s.video_out.get_mut(&key) else {
            return;
        };
        out.jpeg_seq = out.jpeg_seq.wrapping_add(1);
        video_wire::Header {
            codec: Codec::Jpeg,
            keyframe: true,
            track,
            seq: out.jpeg_seq,
            frame_no: out.frame_no,
            height: rendition,
        }
    };
    if s.drop_video.remove(&key) {
        if let Some(out) = s.video_out.get_mut(&key) {
            out.dropped += 1;
        }
        eprintln!(
            "[azmeet] {}: dropped JPEG frame {} of the {} on purpose",
            s.name,
            header.seq,
            rendition_label(track, rendition)
        );
        return;
    }
    if let Some(out) = s.video_out.get_mut(&key) {
        out.jpeg_frames += 1;
    }
    let packet = video_wire::encode_packet(&header, jpeg.as_slice());
    let frame_track = routes::frame_track(0, track, rendition);
    for handle in peers {
        endpoint.send_frame(*handle, frame_track, packet.clone());
    }
}

/// A video packet of `origin`'s stream that reached this side through connection `via` (the
/// origin's own, or a forwarder's): through the stream's rules, which acknowledge it and ask for
/// keyframes back the way it came, then, when this side shows that rendition, its decoder; the
/// newest picture goes into `pictures`. A stream this side only passes on is followed the same way
/// (the sender's window waits for the acknowledgements) but not decoded. Returns whether a tile
/// appeared.
pub(crate) fn take_video(
    s: &mut MeetState,
    endpoint: &IrohEndpoint,
    origin: u64,
    via: u64,
    header: &video_wire::Header,
    payload: &[u8],
    pictures: &mut BTreeMap<(u64, u32), RawImage>,
) -> bool {
    let Some(slot) = track_slot(header.track) else {
        return false;
    };
    let now = now_ms(s);
    let mine = shows(s, origin, header.track, header.height);
    let Some(remote) = s.remotes.iter_mut().find(|r| r.key == origin) else {
        return false;
    };
    let handle = remote.handle;
    let tile_px = remote.tile_px[slot];
    let new_tile = mine && !remote.tracks[slot];
    if mine {
        remote.tracks[slot] = true;
    }
    let input = remote
        .received
        .entry((header.track, header.height))
        .or_insert_with(VideoIn::new);
    if mine && !input.shown {
        // Shown from now on: a fresh decoder, which starts at a keyframe.
        input.rules = video_wire::ReceiveTrack::new();
        input.decoder = None;
    }
    input.shown = mine;
    input.seen = Some(header.codec);
    input.via = via;
    let verdict = input.rules.on_packet(header, now);
    if verdict.restart {
        input.decoder = None;
    }
    let mut controls = Vec::new();
    if let Some(seq) = verdict.ack {
        controls.push(video_wire::encode_received(
            header.track,
            seq,
            header.height,
        ));
    }
    if verdict.request_keyframe {
        controls.push(video_wire::encode_keyframe_request(
            header.track,
            header.height,
        ));
    }
    let picture = if mine && verdict.decode {
        decode_picture(input, header.codec, payload, tile_px)
    } else {
        None
    };
    let inert = mine && input.rules.decoder_is_inert();
    if inert {
        input.decoder = None;
    }
    for control in controls {
        send_control_up(s, endpoint, origin, via, header.track, &control);
    }
    if let Some(picture) = picture {
        pictures.insert((handle, header.track), picture);
    }
    if inert && s.video.decodes_h264 {
        s.video.decodes_h264 = false;
        eprintln!(
            "[azmeet] {}: the H.264 decoder gives no pictures back: asking everyone for JPEG",
            s.name
        );
        send_caps(s, &all_peers(s));
    }
    new_tile
}

/// Decodes one packet of a shown stream: a JPEG's picture at once; an H.264 packet goes to the
/// stream's decoder, whose picture comes out on a later pump ([`drain_decoders`]), as NV12 at the
/// size of the tile that shows it (`tile_px`, device pixels): scaled once, on the GPU, and never
/// converted to RGB on the CPU.
pub(crate) fn decode_picture(
    input: &mut VideoIn,
    codec: Codec,
    payload: &[u8],
    tile_px: Option<(u32, u32)>,
) -> Option<RawImage> {
    let (picture, pictures) = match codec {
        Codec::Jpeg => match RawImage::decode_image_bytes_any(U8VecRef::from(payload)) {
            ResultRawImageDecodeImageError::Ok(image) => (Some(image), 1),
            _ => (None, 0),
        },
        Codec::H264 => {
            if input.decoder.is_none() {
                // A fresh decoder (the first packet, a restart, a stream shown again) knows no
                // size yet: the one the previous decoder was told is not its own, and keeping it
                // left every restarted stream at the stream's size, scaled by the renderer.
                input.output_size = None;
            }
            let decoder = input.decoder.get_or_insert_with(|| {
                let decoder = VideoDecoder::open(false);
                decoder.set_output_format(VIDEO_FORMAT);
                decoder
            });
            if input.output_size != tile_px {
                // Applied by the decoder at the stream's next keyframe.
                let (w, h) = tile_px.unwrap_or((0, 0));
                decoder.set_output_size(w, h);
                input.output_size = tile_px;
            }
            // Decoded on the decoder's own thread: the picture comes out on a later pump
            // (`drain_decoders`).
            decoder.decode(U8Vec::from(payload.to_vec()));
            (None, 0)
        }
    };
    input.rules.decoded(pictures);
    picture
}

/// Every pump: the pictures every decoder finished since the last one (they decode on their own
/// threads); the newest of each shown stream goes onto its tile.
pub(crate) fn drain_decoders(s: &mut MeetState, pictures: &mut BTreeMap<(u64, u32), RawImage>) {
    for r in s.remotes.iter_mut() {
        let handle = r.handle;
        for ((track, _), input) in r.received.iter_mut() {
            if !input.shown {
                continue;
            }
            let Some(decoder) = input.decoder.as_mut() else {
                continue;
            };
            let mut newest = None;
            let mut count = 0;
            while let Some(frame) = decoder.recv_frame().into_option() {
                count += 1;
                newest = Some(frame);
            }
            if count > 0 {
                input.rules.decoded(count);
            }
            if let Some(frame) = newest {
                pictures.insert((handle, *track), frame_image(frame));
            }
        }
    }
}

/// A control about this side's own stream, from the peer with key `requester`, arriving on
/// connection `conn`; `via` is set when a forwarder passed it on. True when the window changes.
pub(crate) fn apply_video_control(
    s: &mut MeetState,
    conn: u64,
    requester: u64,
    via: Option<u64>,
    control: Control,
) -> bool {
    match control {
        Control::KeyframeRequest { track, height } => {
            let Some(slot) = track_slot(track) else {
                return false;
            };
            // A request without a height (none named) asks every rendition of the track.
            for ((out_track, out_height), out) in s.video_out.iter_mut() {
                if *out_track == track && (height == 0 || *out_height == height) {
                    out.keyframes.request();
                }
            }
            let who = name_of(s, requester);
            let what = if height == 0 {
                String::from(SOURCES[slot])
            } else {
                rendition_label(track, height)
            };
            match via {
                Some(through) => eprintln!(
                    "[azmeet] {}: {who} asked for a keyframe ({what}, via {})",
                    s.name,
                    name_of_handle(s, through)
                ),
                None => eprintln!("[azmeet] {}: {who} asked for a keyframe ({what})", s.name),
            }
            false
        }
        Control::Received { track, seq, height } => {
            if let Some(remote) = s.remotes.iter_mut().find(|r| r.handle == conn) {
                if let Some(window) = remote.sent.get_mut(&(track, height)) {
                    window.acked(seq);
                }
            }
            false
        }
        Control::Caps {
            h264,
            encodes,
            opus,
        } => {
            let Some(remote) = s.remotes.iter_mut().find(|r| r.handle == conn) else {
                return false;
            };
            remote.opus = Some(opus);
            if remote.h264 == Some(h264) && remote.encodes == Some(encodes) {
                return false;
            }
            if remote.h264 != Some(h264) {
                // The peer's H.264 starts afresh, at a keyframe.
                remote.sent.clear();
            }
            remote.h264 = Some(h264);
            remote.encodes = Some(encodes);
            let node_id = remote.node_id.clone();
            let yes = |on: bool| if on { "yes" } else { "no" };
            eprintln!(
                "[azmeet] {}: {} decodes H.264: {}, encodes it: {}, decodes Opus: {}",
                s.name,
                remote_name(s, &node_id),
                yes(h264),
                yes(encodes),
                yes(opus)
            );
            true
        }
    }
}

/// Every pump while the camera or the screen share is a test pattern: the frame due now, in every
/// rendition someone shows, sent like a captured one.
pub(crate) fn pump_pattern(s: &mut MeetState) {
    if !s.pattern_video || s.remotes.is_empty() {
        return;
    }
    let elapsed = now_ms(s);
    for (slot, track) in [(0usize, CAMERA_TRACK), (1, SCREEN_TRACK)] {
        let heights = my_renditions(s, track);
        if heights.is_empty() {
            continue;
        }
        let Some(index) = s.pattern_clocks[slot].next(elapsed) else {
            continue;
        };
        for height in heights {
            let width = routes::rendition_width(height);
            // The screen's bars sit half a frame further on, so the two tiles differ.
            let shift = slot as u32 * width / 2 / video_wire::PATTERN_STEP;
            let frame = pattern_frame(index.wrapping_add(shift), width, u32::from(height));
            send_video(s, track, height, frame);
        }
    }
}

/// The devices panel's video lines: what each rendition of each local track sent, and what arrived
/// of each stream this side shows ("via Ben" when a forwarder passed it on).
pub(crate) fn video_lines(s: &MeetState) -> Vec<String> {
    let mut lines = Vec::new();
    for ((track, height), out) in &s.video_out {
        if out.h264_packets + out.jpeg_frames + out.dropped > 0 {
            let mut line = video_wire::send_line(
                &rendition_label(*track, *height),
                out.h264_packets,
                out.jpeg_frames,
                &out.keyframes.stats(),
                out.dropped,
            );
            if out.encoder.is_some() {
                line.push_str(&format!(", H.264 at {}", out.rate.label()));
            }
            lines.push(line);
        }
    }
    for r in &s.remotes {
        let name = remote_name(s, &r.node_id);
        for ((track, height), input) in &r.received {
            let stats = input.rules.stats();
            let Some(codec) = input.rules.codec().or(input.seen) else {
                continue;
            };
            // A rendition this side no longer shows keeps no line.
            if !input.shown || stats.packets == 0 || !shows(s, r.key, *track, *height) {
                continue;
            }
            let mut source = rendition_label(*track, *height);
            if input.via != r.handle {
                source.push_str(&format!(" via {}", name_of_handle(s, input.via)));
            }
            lines.push(video_wire::video_line(&name, &source, codec, &stats));
        }
    }
    lines
}

/// The periodic log: the video lines, how far this side runs ahead of each peer on each stream,
/// what arrived late, and the streams it passes on.
pub(crate) fn video_log_lines(s: &MeetState) -> Vec<String> {
    let mut lines = vec![codec_status(s)];
    lines.extend(video_lines(s));
    for r in &s.remotes {
        let name = remote_name(s, &r.node_id);
        for ((track, height), window) in &r.sent {
            if window.in_flight() + window.skipped() as usize > 0 {
                lines.push(format!(
                    "{} to {name}: {} in flight, {} not sent",
                    rendition_label(*track, *height),
                    window.in_flight(),
                    window.skipped()
                ));
            }
        }
        for ((track, height), input) in &r.received {
            let late = input.rules.stats().late;
            if late > 0 {
                lines.push(format!(
                    "{} from {name}: {late} late",
                    rendition_label(*track, *height)
                ));
            }
        }
    }
    for ((child, origin, track, height), window) in &s.relay.windows {
        if window.in_flight() + window.skipped() as usize > 0 {
            lines.push(format!(
                "{}'s {} passed on to {}: {} in flight, {} not sent",
                name_of(s, *origin),
                rendition_label(*track, *height),
                name_of_handle(s, *child),
                window.in_flight(),
                window.skipped()
            ));
        }
    }
    lines
}

/// The "Drop a video packet" button (`AZMEET_TEST_PATTERN=1`): the next packet of every rendition
/// this side sends is not sent, so everyone who gets one sees a gap.
pub(crate) extern "C" fn on_drop_video_packet(mut data: RefAny, _info: CallbackInfo) -> Update {
    if let Some(mut guard) = data.downcast_mut::<MeetState>() {
        let s = &mut *guard;
        s.drop_video = my_streams(s);
    }
    Update::DoNothing
}
