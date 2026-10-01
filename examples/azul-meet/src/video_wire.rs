//! The pure video rules of AzMeet, free of azul types so `cargo test -p AzMeet` checks them without
//! a window, a camera or a codec: the packet header, the control messages, when a receiver may
//! decode and when it asks for a keyframe, when a sender forces one, how far a sender may run ahead
//! of a slow peer, when an encoder or a decoder is found not to work, the test pattern, and the
//! lines the window shows.
//!
//! # Transport
//!
//! H.264 packets travel as reliable iroh messages (`IrohEndpoint::send_message`): delivered in
//! order and never dropped while the connection lives. The iroh frame path cannot carry an
//! inter-frame codec: a newer frame replaces an unsent one at the sender, the receiver keeps only
//! the newest frame of a track, and every frame is its own QUIC stream, so a large keyframe loses
//! the race to the small P-frame after it and is then discarded as older. JPEG frames stand alone,
//! so JPEG keeps the latest-wins frame path, where skipping a stale frame is what one wants.
//!
//! A packet can still be missing on the reliable path: the sender stops sending to a peer whose
//! link does not keep up ([`SendWindow`]), and the debug button drops one. So the receiver checks
//! the sequence numbers: after a gap it decodes nothing until the next keyframe and asks the sender
//! for one ([`ReceiveTrack`]); the sender forces one ([`KeyframePolicy`]).
//!
//! # Packet (a message, or a frame of the camera / screen track), little endian
//!
//! ```text
//! [kind u8 = 2][version u8 = 2][codec u8: 1 JPEG, 2 H.264][flags u8: bit 0 keyframe]
//! [track u32][seq u32][frame_no u32][height u16][reserved u16 = 0]
//! then the payload (a JPEG file, or H.264 Annex B)
//! ```
//!
//! `height` names the rendition (90, 180, 360 or 720 lines, see `routes.rs`): each rendition of a
//! track is a stream of its own, with its own encoder, numbers and keyframes.
//! `seq` counts the packets of one codec on one rendition: consecutive, so a gap is a missing
//! packet.
//! `frame_no` counts the frames the sender captured on the track while someone listened, whatever
//! the codec: it may jump (frames the encoder skipped are no loss), so gaps are found on `seq`.
//! One packet is one encoded picture: what one `VideoEncoder::recv_packet` returns (an access unit
//! from VideoToolbox, with SPS and PPS ahead of every IDR slice).
//!
//! # Control messages (reliable), one kind byte first
//!
//! ```text
//! [3][track u32][height u16]           keyframe request: the receiver waits for a keyframe (a PLI)
//! [4][track u32][seq u32][height u16]  received: every H.264 packet through `seq` arrived
//! [5][flags u8]   the sender of this message decodes H.264 (bit 0; JPEG always), encodes it (bit 1)
//! ```
//!
//! A control names the rendition it is about; a missing height reads as 0. Kind 1 is the audio
//! state message (`audio.rs`), kinds 6 and 7 the sync and the relay envelope (`routes.rs`).
//! Unknown kinds are not video messages; bytes after a known message are ignored, so later versions
//! can append fields.

use std::collections::VecDeque;

/// Kind byte of a video packet.
pub const KIND_VIDEO: u8 = 2;
/// Kind byte of a keyframe request.
pub const KIND_KEYFRAME_REQUEST: u8 = 3;
/// Kind byte of a "received through" acknowledgement.
pub const KIND_RECEIVED: u8 = 4;
/// Kind byte of the "what I decode" message.
pub const KIND_CAPS: u8 = 5;
/// Version byte of a video packet.
pub const WIRE_VERSION: u8 = 2;
/// Bytes before a packet's payload.
pub const HEADER_BYTES: usize = 20;
/// A keyframe at least this often (ms), whether or not anyone asked.
pub const PERIODIC_KEYFRAME_MS: u64 = 3000;
/// Forced keyframes at most this often (ms): requests from several peers, or repeated ones, are
/// answered by one keyframe.
pub const MIN_FORCED_GAP_MS: u64 = 500;
/// A receiver waiting for a keyframe asks again after this long (ms).
pub const REQUEST_RETRY_MS: u64 = 1000;
/// A receiver acknowledges every this many H.264 packets, and every keyframe.
pub const ACK_EVERY: u32 = 5;
/// Unacknowledged H.264 packets a sender allows one peer; past that the peer is paused until it
/// has caught up, and resumes at a keyframe.
pub const MAX_IN_FLIGHT: usize = 24;
/// Frames an encoder may take without one packet coming out before it is found not to work.
pub const ENCODER_INERT_AFTER: u32 = 8;
/// H.264 packets a decoder may take (from a keyframe on) without one picture coming out before it
/// is found not to work.
pub const DECODER_INERT_AFTER: u32 = 30;
/// A sequence number this far behind the expected one means the sender numbers from scratch.
pub const RESTART_AFTER_BACKWARD: u32 = 1000;
/// Encoders reopened in a row without a keyframe coming out, after which H.264 is given up.
pub const MAX_REOPENS: u32 = 3;
/// Packets of frames submitted before a forced one that may still come out ahead of it: the
/// encoder works on its own thread, with up to three frames queued and one in hand.
pub const OUTPUT_LAG_PACKETS: u32 = 4;
/// Frames per second of the test pattern.
pub const PATTERN_FPS: u32 = 15;
/// Pixels the test pattern's bars move left per frame.
pub const PATTERN_STEP: u32 = 4;

/// How a packet's picture is encoded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Codec {
    /// A JPEG file: every packet stands alone.
    Jpeg,
    /// H.264 Annex B: a P-frame needs every packet since the last keyframe.
    H264,
}

impl Codec {
    /// The codec byte of the header.
    pub fn byte(self) -> u8 {
        match self {
            Codec::Jpeg => 1,
            Codec::H264 => 2,
        }
    }

    /// The codec a header byte names.
    pub fn from_byte(byte: u8) -> Option<Codec> {
        match byte {
            1 => Some(Codec::Jpeg),
            2 => Some(Codec::H264),
            _ => None,
        }
    }

    /// As the window names it.
    pub fn label(self) -> &'static str {
        match self {
            Codec::Jpeg => "JPEG",
            Codec::H264 => "H.264",
        }
    }

    fn index(self) -> usize {
        match self {
            Codec::Jpeg => 0,
            Codec::H264 => 1,
        }
    }
}

/// The header of a video packet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Header {
    pub codec: Codec,
    /// A decoder can start here (every JPEG; an H.264 packet with an IDR slice).
    pub keyframe: bool,
    pub track: u32,
    /// Packets of this codec on this track so far: consecutive.
    pub seq: u32,
    /// Frames captured on this track so far: may jump.
    pub frame_no: u32,
    /// The rendition's height in lines; 0 for none named.
    pub height: u16,
}

const FLAG_KEYFRAME: u8 = 1;

/// The packet carrying `payload` under `header`.
pub fn encode_packet(header: &Header, payload: &[u8]) -> Vec<u8> {
    let flags = if header.keyframe { FLAG_KEYFRAME } else { 0 };
    let mut out = Vec::with_capacity(HEADER_BYTES + payload.len());
    out.extend_from_slice(&[KIND_VIDEO, WIRE_VERSION, header.codec.byte(), flags]);
    out.extend_from_slice(&header.track.to_le_bytes());
    out.extend_from_slice(&header.seq.to_le_bytes());
    out.extend_from_slice(&header.frame_no.to_le_bytes());
    out.extend_from_slice(&header.height.to_le_bytes());
    out.extend_from_slice(&[0, 0]);
    out.extend_from_slice(payload);
    out
}

fn le_u32(bytes: &[u8], at: usize) -> Option<u32> {
    let b = bytes.get(at..at + 4)?;
    Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

fn le_u16(bytes: &[u8], at: usize) -> Option<u16> {
    let b = bytes.get(at..at + 2)?;
    Some(u16::from_le_bytes([b[0], b[1]]))
}

/// Reads a packet: its header and payload. `None` for another kind, version or codec, a short
/// header, or no payload. Flag bits other than the keyframe bit are ignored.
pub fn decode_packet(bytes: &[u8]) -> Option<(Header, &[u8])> {
    let head = bytes.get(..HEADER_BYTES)?;
    if head[0] != KIND_VIDEO || head[1] != WIRE_VERSION {
        return None;
    }
    let codec = Codec::from_byte(head[2])?;
    let payload = &bytes[HEADER_BYTES..];
    if payload.is_empty() {
        return None;
    }
    let header = Header {
        codec,
        keyframe: head[3] & FLAG_KEYFRAME != 0,
        track: le_u32(head, 4)?,
        seq: le_u32(head, 8)?,
        frame_no: le_u32(head, 12)?,
        height: le_u16(head, 16)?,
    };
    Some((header, payload))
}

/// A video control message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Control {
    /// The receiver waits for a keyframe on the `height` rendition of `track`.
    KeyframeRequest { track: u32, height: u16 },
    /// Every H.264 packet of the `height` rendition of `track` through `seq` arrived.
    Received { track: u32, seq: u32, height: u16 },
    /// Whether the sender of this message decodes H.264, and whether it encodes it.
    Caps { h264: bool, encodes: bool },
}

const CAPS_H264: u8 = 1;
const CAPS_ENCODES: u8 = 2;

/// The request for a keyframe on the `height` rendition of `track`.
pub fn encode_keyframe_request(track: u32, height: u16) -> Vec<u8> {
    let mut out = vec![KIND_KEYFRAME_REQUEST];
    out.extend_from_slice(&track.to_le_bytes());
    out.extend_from_slice(&height.to_le_bytes());
    out
}

/// The acknowledgement of every H.264 packet of the `height` rendition of `track` through `seq`.
pub fn encode_received(track: u32, seq: u32, height: u16) -> Vec<u8> {
    let mut out = vec![KIND_RECEIVED];
    out.extend_from_slice(&track.to_le_bytes());
    out.extend_from_slice(&seq.to_le_bytes());
    out.extend_from_slice(&height.to_le_bytes());
    out
}

/// The message saying whether this side decodes H.264 and whether it encodes it.
pub fn encode_caps(h264: bool, encodes: bool) -> Vec<u8> {
    let mut flags = 0;
    if h264 {
        flags |= CAPS_H264;
    }
    if encodes {
        flags |= CAPS_ENCODES;
    }
    vec![KIND_CAPS, flags]
}

/// Reads a control message; `None` for another kind or a short message. A missing rendition
/// height reads as 0.
pub fn decode_control(bytes: &[u8]) -> Option<Control> {
    match *bytes.first()? {
        KIND_KEYFRAME_REQUEST => Some(Control::KeyframeRequest {
            track: le_u32(bytes, 1)?,
            height: le_u16(bytes, 5).unwrap_or(0),
        }),
        KIND_RECEIVED => Some(Control::Received {
            track: le_u32(bytes, 1)?,
            seq: le_u32(bytes, 5)?,
            height: le_u16(bytes, 9).unwrap_or(0),
        }),
        KIND_CAPS => {
            let flags = *bytes.get(1)?;
            Some(Control::Caps {
                h264: flags & CAPS_H264 != 0,
                encodes: flags & CAPS_ENCODES != 0,
            })
        }
        _ => None,
    }
}

/// A video message, as it arrives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Message<'a> {
    Packet(Header, &'a [u8]),
    Control(Control),
}

/// Reads a message or a frame of the camera / screen track; `None` when it is no video message
/// (the audio state message, an unknown kind, or a malformed one).
pub fn decode_message(bytes: &[u8]) -> Option<Message<'_>> {
    if bytes.first() == Some(&KIND_VIDEO) {
        let (header, payload) = decode_packet(bytes)?;
        return Some(Message::Packet(header, payload));
    }
    decode_control(bytes).map(Message::Control)
}

/// Whether an H.264 Annex B chunk holds an IDR slice (NAL unit type 5), so a decoder can start
/// there. Start codes are three or four bytes; emulation prevention keeps `00 00 01` out of NAL
/// payloads, so every match is a NAL unit.
pub fn h264_is_keyframe(annexb: &[u8]) -> bool {
    annexb
        .windows(4)
        .any(|w| w[0] == 0 && w[1] == 0 && w[2] == 1 && w[3] & 0x1f == 5)
}

/// What a receiver saw on one track.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ReceiveStats {
    /// Packets that arrived in order (late ones not counted).
    pub packets: u64,
    /// Of those, keyframes.
    pub keyframes: u64,
    /// Places where packets were missing.
    pub gaps: u64,
    /// Packets older than one already taken.
    pub late: u64,
    /// Packets not decoded because a keyframe was awaited.
    pub dropped: u64,
    /// Keyframe requests sent.
    pub requests: u64,
    /// Pictures the decoder gave back.
    pub decoded: u64,
}

/// What to do with a packet that arrived.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Verdict {
    /// Hand the payload to the decoder.
    pub decode: bool,
    /// Send the sender a keyframe request for the track.
    pub request_keyframe: bool,
    /// Send the sender "received through this sequence number" (H.264 only).
    pub ack: Option<u32>,
    /// The sender started numbering anew: drop the decoder, a new stream begins.
    pub restart: bool,
}

/// One codec's stream on a track.
#[derive(Debug, Clone, Copy, Default)]
struct Stream {
    /// The sequence number the next packet should have; `None` before the first.
    expected: Option<u32>,
    /// A packet is missing, or the stream began without a keyframe: nothing is decoded until one.
    waiting: bool,
    /// When a keyframe was last asked for while waiting.
    requested_at: Option<u64>,
    /// Packets since the last acknowledgement.
    unacked: u32,
    /// Packets handed to the decoder since the stream began.
    fed: u32,
    /// Pictures that came back since the stream began.
    out: u64,
}

/// The receiving rules of one track of one peer: packets are decoded in order; after a gap nothing
/// is decoded until a keyframe, which is asked for (again after [`REQUEST_RETRY_MS`] while still
/// waiting). JPEG and H.264 packets are two streams with their own numbers, so the last JPEG frames
/// that arrive after a switch to H.264 disturb nothing.
#[derive(Debug, Default)]
pub struct ReceiveTrack {
    /// Indexed by [`Codec::index`].
    streams: [Stream; 2],
    /// The codec of the packet decoded last.
    current: Option<Codec>,
    stats: ReceiveStats,
}

impl ReceiveTrack {
    pub fn new() -> Self {
        ReceiveTrack::default()
    }

    /// Takes in a packet's header, `now_ms` on the receiver's clock, and says what to do.
    pub fn on_packet(&mut self, header: &Header, now_ms: u64) -> Verdict {
        let mut verdict = Verdict::default();
        let stream = &mut self.streams[header.codec.index()];
        if let Some(expected) = stream.expected {
            let ahead = header.seq.wrapping_sub(expected) as i32;
            if ahead < 0 && ahead.unsigned_abs() <= RESTART_AFTER_BACKWARD {
                self.stats.late += 1;
                return verdict;
            }
            if ahead < 0 {
                // Far behind: the sender numbers from scratch, a new stream begins.
                *stream = Stream::default();
                verdict.restart = true;
            } else if ahead > 0 {
                self.stats.gaps += 1;
                if !header.keyframe {
                    stream.waiting = true;
                }
            }
        }
        if stream.expected.is_none() && !header.keyframe {
            stream.waiting = true;
        }
        stream.expected = Some(header.seq.wrapping_add(1));
        self.stats.packets += 1;
        if header.keyframe {
            self.stats.keyframes += 1;
            stream.waiting = false;
            stream.requested_at = None;
        }
        if header.codec == Codec::H264 {
            stream.unacked += 1;
            if header.keyframe || stream.unacked >= ACK_EVERY {
                stream.unacked = 0;
                verdict.ack = Some(header.seq);
            }
        }
        if stream.waiting {
            self.stats.dropped += 1;
            let due = stream
                .requested_at
                .map_or(true, |at| now_ms.saturating_sub(at) >= REQUEST_RETRY_MS);
            if due {
                stream.requested_at = Some(now_ms);
                self.stats.requests += 1;
                verdict.request_keyframe = true;
            }
            return verdict;
        }
        stream.fed = stream.fed.saturating_add(1);
        self.current = Some(header.codec);
        verdict.decode = true;
        verdict
    }

    /// Records the pictures the decoder gave back for the packet decoded last.
    pub fn decoded(&mut self, pictures: u64) {
        self.stats.decoded += pictures;
        if let Some(codec) = self.current {
            self.streams[codec.index()].out += pictures;
        }
    }

    /// The codec of the packet decoded last.
    pub fn codec(&self) -> Option<Codec> {
        self.current
    }

    pub fn stats(&self) -> ReceiveStats {
        self.stats
    }

    /// Whether the H.264 decoder took [`DECODER_INERT_AFTER`] packets from a keyframe on and gave
    /// back nothing: a decoder that opens but does not decode (no backend on this build).
    pub fn decoder_is_inert(&self) -> bool {
        let h264 = &self.streams[Codec::H264.index()];
        h264.out == 0 && h264.fed >= DECODER_INERT_AFTER
    }
}

/// What a sender's keyframe rules did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct KeyframeStats {
    /// Keyframes that came out of the encoder.
    pub keyframes: u64,
    /// Keyframes forced because someone asked (a request, a new peer, a peer catching up).
    pub on_request: u64,
    /// Keyframes forced by the [`PERIODIC_KEYFRAME_MS`] floor.
    pub periodic: u64,
    /// Encoders reopened.
    pub reopened: u64,
}

/// When a sender forces a keyframe: for a fresh encoder, when asked (at most once per
/// [`MIN_FORCED_GAP_MS`]), and when [`PERIODIC_KEYFRAME_MS`] passed without one. An encoder that
/// answers a forced frame with a P-frame cannot force keyframes: it is reopened, since a new
/// encoder starts with a keyframe; [`MAX_REOPENS`] reopens in a row without one give H.264 up.
///
/// This expects one packet per submitted frame, in order, as VideoToolbox gives - but not at
/// once: the encoder works on its own thread, so the packets of up to [`OUTPUT_LAG_PACKETS`]
/// frames submitted before a forced one may come out ahead of it.
#[derive(Debug, Default)]
pub struct KeyframePolicy {
    /// Someone asked for a keyframe that has not come out yet.
    pending: bool,
    last_keyframe_ms: Option<u64>,
    last_forced_ms: Option<u64>,
    /// A keyframe was forced: how many more P-frames (of frames submitted before it) may come out
    /// before its keyframe must.
    awaiting: Option<u32>,
    reopen: bool,
    reopens_in_a_row: u32,
    stats: KeyframeStats,
}

impl KeyframePolicy {
    pub fn new() -> Self {
        KeyframePolicy::default()
    }

    /// A keyframe request, a new peer, or a peer that must catch up.
    pub fn request(&mut self) {
        self.pending = true;
    }

    /// Whether to force a keyframe for the frame submitted `now_ms`.
    pub fn should_force(&mut self, now_ms: u64) -> bool {
        let fresh = self.last_keyframe_ms.is_none();
        let spaced = self
            .last_forced_ms
            .map_or(true, |at| now_ms.saturating_sub(at) >= MIN_FORCED_GAP_MS);
        let asked = self.pending && spaced;
        let periodic = self
            .last_keyframe_ms
            .is_some_and(|at| now_ms.saturating_sub(at) >= PERIODIC_KEYFRAME_MS);
        if !(fresh || asked || periodic) {
            return false;
        }
        if asked {
            self.stats.on_request += 1;
        } else if periodic {
            self.stats.periodic += 1;
        }
        self.last_forced_ms = Some(now_ms);
        // Counted from the first forced frame still waiting for its keyframe.
        self.awaiting = Some(self.awaiting.unwrap_or(OUTPUT_LAG_PACKETS));
        true
    }

    /// A packet came out of the encoder `now_ms`.
    pub fn on_output(&mut self, keyframe: bool, now_ms: u64) {
        if keyframe {
            self.stats.keyframes += 1;
            self.last_keyframe_ms = Some(now_ms);
            self.pending = false;
            self.awaiting = None;
            self.reopens_in_a_row = 0;
        } else {
            match self.awaiting {
                // A P-frame of a frame submitted before the forced one.
                Some(left) if left > 0 => self.awaiting = Some(left - 1),
                // Forced, and its own packet was a P-frame: this encoder cannot force keyframes.
                Some(_) => {
                    self.awaiting = None;
                    self.reopen = true;
                }
                None => {}
            }
        }
    }

    /// The encoder must be closed and opened again.
    pub fn must_reopen(&self) -> bool {
        self.reopen
    }

    /// The encoder was reopened (or opened at a new size): its first frame is forced.
    pub fn reopened(&mut self) {
        self.reopen = false;
        self.awaiting = None;
        self.last_keyframe_ms = None;
        self.reopens_in_a_row += 1;
        self.stats.reopened += 1;
    }

    /// [`MAX_REOPENS`] reopens in a row brought no keyframe: give H.264 up.
    pub fn is_broken(&self) -> bool {
        self.reopens_in_a_row >= MAX_REOPENS
    }

    pub fn stats(&self) -> KeyframeStats {
        self.stats
    }
}

/// How far a sender runs ahead of one peer on one track (H.264). A new peer, or one that fell
/// [`MAX_IN_FLIGHT`] packets behind, gets nothing until a keyframe; one that fell behind first
/// catches up to half of that, then asks for the keyframe it resumes at. So a slow link never
/// queues more than [`MAX_IN_FLIGHT`] packets, and never gets a P-frame it cannot decode.
#[derive(Debug, Default)]
pub struct SendWindow {
    /// The peer got a keyframe and every packet since.
    synced: bool,
    /// Sequence numbers sent and not acknowledged, oldest first.
    unacked: VecDeque<u32>,
    skipped: u64,
}

impl SendWindow {
    pub fn new() -> Self {
        SendWindow::default()
    }

    /// Whether to send the packet `seq` to this peer.
    pub fn offer(&mut self, seq: u32, keyframe: bool) -> bool {
        if self.unacked.len() >= MAX_IN_FLIGHT {
            // The link does not keep up: pause, and resume at a keyframe once it caught up.
            self.synced = false;
            self.skipped += 1;
            return false;
        }
        if !self.synced && !keyframe {
            self.skipped += 1;
            return false;
        }
        self.synced = true;
        self.unacked.push_back(seq);
        true
    }

    /// The peer received every packet through `seq`.
    pub fn acked(&mut self, seq: u32) {
        while let Some(first) = self.unacked.front() {
            if (seq.wrapping_sub(*first) as i32) < 0 {
                break;
            }
            self.unacked.pop_front();
        }
    }

    /// The peer waits for a keyframe, and its link has room for one.
    pub fn wants_keyframe(&self) -> bool {
        !self.synced && self.unacked.len() <= MAX_IN_FLIGHT / 2
    }

    /// Packets sent and not yet acknowledged.
    pub fn in_flight(&self) -> usize {
        self.unacked.len()
    }

    /// Packets not sent to this peer.
    pub fn skipped(&self) -> u64 {
        self.skipped
    }
}

/// Whether an encoder that opened actually encodes: `VideoEncoder::open` hands out an open handle
/// that never yields a packet where no backend is built in.
#[derive(Debug, Default)]
pub struct EncoderHealth {
    submitted: u32,
    produced: u32,
}

impl EncoderHealth {
    /// A frame went into the encoder.
    pub fn submitted(&mut self) {
        self.submitted = self.submitted.saturating_add(1);
    }

    /// A packet came out.
    pub fn produced(&mut self) {
        self.produced = self.produced.saturating_add(1);
    }

    /// [`ENCODER_INERT_AFTER`] frames went in and nothing came out.
    pub fn is_inert(&self) -> bool {
        self.produced == 0 && self.submitted >= ENCODER_INERT_AFTER
    }
}

/// The test pattern's bars, left to right: white, yellow, cyan, green, magenta, red, blue, black.
const BARS: [[u8; 3]; 8] = [
    [235, 235, 235],
    [235, 235, 16],
    [16, 235, 235],
    [16, 235, 16],
    [235, 16, 235],
    [235, 16, 16],
    [16, 16, 235],
    [16, 16, 16],
];

/// Frame `index` of the test pattern (`AZMEET_TEST_PATTERN=1`, and every headless run): eight
/// colour bars moving [`PATTERN_STEP`] pixels left per frame, as tightly packed RGBA. Empty for a
/// zero size.
pub fn test_pattern(width: u32, height: u32, index: u32) -> Vec<u8> {
    let (w, h) = (width as usize, height as usize);
    if w == 0 || h == 0 {
        return Vec::new();
    }
    let shift = (u64::from(index) * u64::from(PATTERN_STEP) % u64::from(width)) as usize;
    let mut row = Vec::with_capacity(w * 4);
    for x in 0..w {
        let bar = BARS[(x + shift) % w * BARS.len() / w];
        row.extend_from_slice(&[bar[0], bar[1], bar[2], 255]);
    }
    row.repeat(h)
}

/// Hands out the test pattern's frames by wall time, like a camera would.
#[derive(Debug)]
pub struct PatternClock {
    fps: u32,
    last: Option<u64>,
}

impl PatternClock {
    pub fn new(fps: u32) -> Self {
        PatternClock { fps, last: None }
    }

    /// The index of the frame due `elapsed_ms` after the clock started, when it was not handed out
    /// yet; after a stall the frames missed are skipped.
    pub fn next(&mut self, elapsed_ms: u64) -> Option<u32> {
        let due = elapsed_ms.saturating_mul(u64::from(self.fps)) / 1000;
        if self.last.is_some_and(|last| due <= last) {
            return None;
        }
        self.last = Some(due);
        Some(due as u32)
    }
}

/// The codec a viewer gets a sender's stream in: H.264 when the sender encodes it and the viewer
/// decodes it, JPEG when one of them cannot; `None` while the viewer's caps have not arrived -
/// nothing is sent yet. JPEG is the fallback of a side without H.264 (no encoder off Apple
/// today), never the default: a Mac peer whose caps came a moment after its first frames used
/// to get those as JPEG.
pub fn wire_codec(sender_encodes: bool, viewer_decodes: Option<bool>) -> Option<Codec> {
    let decodes = viewer_decodes?;
    Some(if sender_encodes && decodes {
        Codec::H264
    } else {
        Codec::Jpeg
    })
}

/// The window's codec line: "Video: H.264 (VideoToolbox)", with "; JPEG to Ben (no H.264 decoder)"
/// for peers that cannot decode it, or "Video: JPEG (no encoder)". `encoder` is the H.264 backend,
/// or why there is none.
pub fn codec_line(encoder: Result<&str, &str>, jpeg_to: &[String]) -> String {
    match encoder {
        Ok(backend) if jpeg_to.is_empty() => format!("Video: H.264 ({backend})"),
        Ok(backend) => format!(
            "Video: H.264 ({backend}); JPEG to {} (no H.264 decoder)",
            jpeg_to.join(", ")
        ),
        Err(why) => format!("Video: JPEG ({why})"),
    }
}

/// The window's line for one peer's incoming track: "Video from Ben (camera): H.264, decoded 300,
/// keyframes 12, gaps 1, dropped 3, keyframe requests 1".
pub fn video_line(name: &str, source: &str, codec: Codec, stats: &ReceiveStats) -> String {
    format!(
        "Video from {name} ({source}): {}, decoded {}, keyframes {}, gaps {}, dropped {}, keyframe \
         requests {}",
        codec.label(),
        stats.decoded,
        stats.keyframes,
        stats.gaps,
        stats.dropped,
        stats.requests
    )
}

/// The window's line for one outgoing track: "Sending camera: 450 H.264 packets, 0 JPEG frames,
/// 8 keyframes, 2 on request, 5 periodic, 0 reopens, 1 dropped on purpose".
pub fn send_line(
    source: &str,
    h264_packets: u64,
    jpeg_frames: u64,
    keys: &KeyframeStats,
    dropped: u64,
) -> String {
    format!(
        "Sending {source}: {h264_packets} H.264 packets, {jpeg_frames} JPEG frames, {} keyframes, \
         {} on request, {} periodic, {} reopens, {dropped} dropped on purpose",
        keys.keyframes, keys.on_request, keys.periodic, keys.reopened
    )
}
#[cfg(test)]
mod tests {
    use super::*;

    fn h264(seq: u32, keyframe: bool) -> Header {
        Header {
            codec: Codec::H264,
            keyframe,
            track: 1,
            seq,
            frame_no: seq,
            height: 180,
        }
    }

    fn jpeg(seq: u32) -> Header {
        Header {
            codec: Codec::Jpeg,
            keyframe: true,
            track: 1,
            seq,
            frame_no: seq,
            height: 180,
        }
    }

    /// Feeds `(header, now_ms)` in turn; per packet whether it was decoded and whether a keyframe
    /// was asked for.
    fn feed(rx: &mut ReceiveTrack, packets: &[(Header, u64)]) -> Vec<(bool, bool)> {
        packets
            .iter()
            .map(|(header, now)| {
                let verdict = rx.on_packet(header, *now);
                (verdict.decode, verdict.request_keyframe)
            })
            .collect()
    }

    /// Pixel `x` of the first row of an RGBA frame `width` wide.
    fn pixel(frame: &[u8], x: usize) -> [u8; 4] {
        [
            frame[x * 4],
            frame[x * 4 + 1],
            frame[x * 4 + 2],
            frame[x * 4 + 3],
        ]
    }

    #[test]
    fn a_video_packet_names_its_rendition_and_round_trips_through_the_wire_format() {
        let header = Header {
            codec: Codec::H264,
            keyframe: true,
            track: 2,
            seq: 0x0102_0304,
            frame_no: 77,
            height: 360,
        };
        let payload: &[u8] = &[0, 0, 0, 1, 0x65, 0x88, 0x84];
        let bytes = encode_packet(&header, payload);
        assert_eq!(&bytes[..4], &[KIND_VIDEO, 2, 2, 1]);
        assert_eq!(&bytes[4..8], &[2, 0, 0, 0]);
        assert_eq!(&bytes[8..12], &[4, 3, 2, 1]);
        assert_eq!(&bytes[12..16], &[77, 0, 0, 0]);
        assert_eq!(
            &bytes[16..20],
            &[0x68, 0x01, 0, 0],
            "360 lines, then two reserved bytes"
        );
        assert_eq!(bytes.len(), 20 + payload.len());
        assert_eq!(decode_packet(&bytes), Some((header, payload)));
        assert_eq!(
            decode_message(&bytes),
            Some(Message::Packet(header, payload))
        );

        let frame = jpeg(9);
        let bytes = encode_packet(&frame, b"\xff\xd8jpeg");
        assert_eq!(&bytes[..4], &[KIND_VIDEO, WIRE_VERSION, 1, 1]);
        assert_eq!(decode_packet(&bytes), Some((frame, &b"\xff\xd8jpeg"[..])));

        let p_frame = h264(3, false);
        let bytes = encode_packet(&p_frame, &[0, 0, 1, 0x41]);
        assert_eq!(bytes[3], 0, "a P-frame has no keyframe flag");
        assert_eq!(decode_packet(&bytes).map(|(h, _)| h), Some(p_frame));
    }

    #[test]
    fn a_malformed_or_foreign_packet_is_refused() {
        let good = encode_packet(&h264(1, true), b"x");
        assert!(decode_packet(&good).is_some());
        let mut bad: Vec<Vec<u8>> = vec![
            Vec::new(),
            good[..HEADER_BYTES - 1].to_vec(),
            // A header without a payload.
            good[..HEADER_BYTES].to_vec(),
        ];
        // Another kind, an older and a newer version, no codec, an unknown codec.
        for (index, value) in [(0, 9), (1, 1), (1, 3), (2, 0), (2, 3)] {
            let mut changed = good.clone();
            changed[index] = value;
            bad.push(changed);
        }
        for bytes in bad {
            assert_eq!(decode_packet(&bytes), None, "{bytes:?}");
        }
        // Flag bits other than the keyframe bit are for later versions.
        let mut flagged = good.clone();
        flagged[3] = 0b1000_0001;
        assert_eq!(decode_packet(&flagged).map(|(h, _)| h.keyframe), Some(true));
        flagged[3] = 0b1000_0000;
        assert_eq!(
            decode_packet(&flagged).map(|(h, _)| h.keyframe),
            Some(false)
        );
    }

    #[test]
    fn keyframe_requests_acknowledgements_and_caps_are_short_messages_naming_the_rendition() {
        assert_eq!(encode_keyframe_request(2, 90), vec![3, 2, 0, 0, 0, 90, 0]);
        assert_eq!(
            decode_control(&encode_keyframe_request(2, 90)),
            Some(Control::KeyframeRequest {
                track: 2,
                height: 90
            })
        );
        assert_eq!(
            encode_received(1, 0xdead_beef, 360),
            vec![4, 1, 0, 0, 0, 0xef, 0xbe, 0xad, 0xde, 0x68, 0x01]
        );
        assert_eq!(
            decode_control(&encode_received(1, 0xdead_beef, 360)),
            Some(Control::Received {
                track: 1,
                seq: 0xdead_beef,
                height: 360
            })
        );
        // Without a height the control is about no rendition in particular.
        assert_eq!(
            decode_control(&[3, 2, 0, 0, 0]),
            Some(Control::KeyframeRequest {
                track: 2,
                height: 0
            })
        );
        assert_eq!(encode_caps(true, false), vec![5, 1]);
        assert_eq!(encode_caps(false, false), vec![5, 0]);
        assert_eq!(encode_caps(true, true), vec![5, 3]);
        assert_eq!(
            decode_control(&encode_caps(true, false)),
            Some(Control::Caps {
                h264: true,
                encodes: false
            })
        );
        assert_eq!(
            decode_control(&encode_caps(false, true)),
            Some(Control::Caps {
                h264: false,
                encodes: true
            })
        );
        assert_eq!(
            decode_message(&encode_caps(true, true)),
            Some(Message::Control(Control::Caps {
                h264: true,
                encodes: true
            }))
        );
        // Later versions may append fields.
        assert_eq!(
            decode_control(&[3, 2, 0, 0, 0, 90, 0, 9, 9]),
            Some(Control::KeyframeRequest {
                track: 2,
                height: 90
            })
        );
        for short in [
            &[3_u8, 2, 0][..],
            &[4, 1, 0, 0, 0, 1][..],
            &[5][..],
            &[][..],
        ] {
            assert_eq!(decode_control(short), None, "{short:?}");
        }
        // The audio state message ([1][flags]) is no video message.
        assert_eq!(decode_control(&[1, 1]), None);
        assert_eq!(decode_message(&[1, 1]), None);
        assert_eq!(decode_message(&[KIND_VIDEO, 1]), None);
    }

    #[test]
    fn an_h264_chunk_is_a_keyframe_when_it_holds_an_idr_slice() {
        let sps_pps_idr: &[u8] = &[
            0, 0, 0, 1, 0x67, 0x42, 0x00, 0x1e, // SPS
            0, 0, 0, 1, 0x68, 0xce, 0x3c, 0x80, // PPS
            0, 0, 0, 1, 0x65, 0x88, 0x84, 0x00, // IDR slice
        ];
        assert!(h264_is_keyframe(sps_pps_idr));
        // Three-byte start code, nal_ref_idc 1.
        assert!(h264_is_keyframe(&[0, 0, 1, 0x25, 0xb8]));
        // A P-slice, even after parameter sets, is none.
        assert!(!h264_is_keyframe(&[0, 0, 0, 1, 0x41, 0x9a, 0x02]));
        assert!(!h264_is_keyframe(&[
            0, 0, 0, 1, 0x67, 0x42, 0, 0, 0, 1, 0x68, 0xce, 0, 0, 0, 1, 0x41, 0x9a
        ]));
        // 0x65 without a start code in front is payload, not a NAL header.
        assert!(!h264_is_keyframe(&[
            0, 0, 0, 1, 0x41, 0x65, 0x65, 0x01, 0x65
        ]));
        assert!(!h264_is_keyframe(&[]));
        assert!(!h264_is_keyframe(&[0, 0, 1]));
    }

    #[test]
    fn packets_in_order_are_decoded_and_acknowledged_every_few_packets() {
        let mut rx = ReceiveTrack::new();
        assert_eq!(
            rx.on_packet(&h264(1, true), 0),
            Verdict {
                decode: true,
                request_keyframe: false,
                ack: Some(1),
                restart: false
            },
            "a keyframe is decoded and acknowledged at once"
        );
        let mut acks = Vec::new();
        for seq in 2..=11 {
            let verdict = rx.on_packet(&h264(seq, false), u64::from(seq) * 66);
            assert!(verdict.decode && !verdict.request_keyframe && !verdict.restart);
            acks.extend(verdict.ack);
        }
        assert_eq!(acks, vec![1 + ACK_EVERY, 1 + 2 * ACK_EVERY]);
        let stats = rx.stats();
        assert_eq!(stats.packets, 11);
        assert_eq!(stats.keyframes, 1);
        assert_eq!(
            (stats.gaps, stats.dropped, stats.requests, stats.late),
            (0, 0, 0, 0)
        );
        assert_eq!(rx.codec(), Some(Codec::H264));
    }

    #[test]
    fn a_gap_drops_every_packet_until_the_next_keyframe_and_asks_for_one() {
        let mut rx = ReceiveTrack::new();
        let got = feed(
            &mut rx,
            &[
                (h264(1, true), 0),
                (h264(2, false), 66),
                (h264(3, false), 133),
                // 4 is lost.
                (h264(5, false), 200),
                (h264(6, false), 266),
                (h264(7, true), 333),
                (h264(8, false), 400),
            ],
        );
        assert_eq!(
            got,
            vec![
                (true, false),
                (true, false),
                (true, false),
                (false, true),
                (false, false),
                (true, false),
                (true, false),
            ]
        );
        let stats = rx.stats();
        assert_eq!((stats.gaps, stats.dropped, stats.requests), (1, 2, 1));
        assert_eq!(stats.keyframes, 2);
    }

    #[test]
    fn a_keyframe_right_after_a_gap_is_decoded_without_a_request() {
        let mut rx = ReceiveTrack::new();
        let got = feed(
            &mut rx,
            &[
                (h264(1, true), 0),
                (h264(2, false), 66),
                (h264(4, true), 200),
                (h264(5, false), 266),
            ],
        );
        assert_eq!(got, vec![(true, false); 4]);
        let stats = rx.stats();
        assert_eq!((stats.gaps, stats.dropped, stats.requests), (1, 0, 0));
    }

    #[test]
    fn while_waiting_the_request_is_repeated_at_most_once_a_second() {
        let mut rx = ReceiveTrack::new();
        let got = feed(
            &mut rx,
            &[
                (h264(1, true), 0),
                (h264(3, false), 100),
                (h264(4, false), 500),
                (h264(5, false), 1099),
                (h264(6, false), 1100),
                (h264(7, false), 1500),
                (h264(8, false), 2100),
            ],
        );
        let requests: Vec<bool> = got.iter().map(|(_, asked)| *asked).collect();
        assert_eq!(requests, vec![false, true, false, false, true, false, true]);
        assert!(got.iter().skip(1).all(|(decode, _)| !decode));
        assert_eq!(rx.stats().requests, 3);
        // The keyframe ends the wait; the next loss asks at once.
        assert!(rx.on_packet(&h264(9, true), 2150).decode);
        assert!(rx.on_packet(&h264(11, false), 2200).request_keyframe);
    }

    #[test]
    fn a_stream_that_begins_without_a_keyframe_waits_for_one() {
        let mut rx = ReceiveTrack::new();
        let got = feed(
            &mut rx,
            &[
                (h264(10, false), 0),
                (h264(11, false), 66),
                (h264(12, true), 133),
                (h264(13, false), 200),
            ],
        );
        assert_eq!(
            got,
            vec![(false, true), (false, false), (true, false), (true, false)]
        );
        assert_eq!(rx.stats().gaps, 0, "the first packet is no gap");
    }

    #[test]
    fn every_jpeg_frame_stands_alone_so_a_lost_one_costs_nothing() {
        let mut rx = ReceiveTrack::new();
        for (seq, now) in [(1, 0), (2, 66), (4, 200), (5, 266)] {
            let verdict = rx.on_packet(&jpeg(seq), now);
            assert!(verdict.decode, "JPEG frame {seq} is decoded");
            assert!(!verdict.request_keyframe);
            assert_eq!(verdict.ack, None, "JPEG frames are not acknowledged");
        }
        let stats = rx.stats();
        assert_eq!(
            (stats.gaps, stats.dropped, stats.requests, stats.keyframes),
            (1, 0, 0, 4)
        );
        assert_eq!(rx.codec(), Some(Codec::Jpeg));
    }

    #[test]
    fn a_late_packet_is_dropped_and_a_sender_numbering_anew_starts_a_new_stream() {
        let mut rx = ReceiveTrack::new();
        assert!(rx.on_packet(&h264(5000, true), 0).decode);
        assert!(rx.on_packet(&h264(5001, false), 66).decode);
        assert_eq!(
            rx.on_packet(&h264(4990, false), 100),
            Verdict::default(),
            "an old packet is neither decoded nor acknowledged"
        );
        assert_eq!(rx.stats().late, 1);
        let restart = rx.on_packet(&h264(1, true), 133);
        assert!(restart.restart && restart.decode);
        assert!(rx.on_packet(&h264(2, false), 200).decode);
        assert_eq!(rx.stats().gaps, 0);

        // Numbering anew with a P-frame: a new stream that waits for its keyframe.
        let mut rx = ReceiveTrack::new();
        rx.on_packet(&h264(5000, true), 0);
        let verdict = rx.on_packet(&h264(1, false), 66);
        assert!(verdict.restart && !verdict.decode && verdict.request_keyframe);
    }

    #[test]
    fn jpeg_and_h264_are_two_streams_that_leave_each_other_alone() {
        let mut rx = ReceiveTrack::new();
        assert!(rx.on_packet(&h264(1, true), 0).decode);
        assert!(rx.on_packet(&h264(2, false), 66).decode);
        // A JPEG frame sent before the switch arrives late: shown, and the H.264 stream goes on.
        assert!(rx.on_packet(&jpeg(40), 70).decode);
        assert_eq!(rx.codec(), Some(Codec::Jpeg));
        let verdict = rx.on_packet(&h264(3, false), 133);
        assert!(verdict.decode && !verdict.request_keyframe && !verdict.restart);
        assert!(rx.on_packet(&jpeg(42), 140).decode);
        assert!(rx.on_packet(&h264(4, false), 200).decode);
        assert_eq!(rx.codec(), Some(Codec::H264));
        let stats = rx.stats();
        assert_eq!((stats.gaps, stats.dropped, stats.requests), (1, 0, 0));
    }

    #[test]
    fn a_decoder_that_never_gives_a_picture_back_is_found_inert() {
        let mut rx = ReceiveTrack::new();
        rx.on_packet(&h264(1, true), 0);
        rx.decoded(0);
        for seq in 2..DECODER_INERT_AFTER {
            rx.on_packet(&h264(seq, false), u64::from(seq) * 66);
            rx.decoded(0);
            assert!(!rx.decoder_is_inert(), "packet {seq}");
        }
        rx.on_packet(&h264(DECODER_INERT_AFTER, false), 3000);
        rx.decoded(0);
        assert!(rx.decoder_is_inert());

        let mut working = ReceiveTrack::new();
        working.on_packet(&h264(1, true), 0);
        working.decoded(1);
        for seq in 2..100 {
            working.on_packet(&h264(seq, false), u64::from(seq) * 66);
            working.decoded(0);
        }
        assert!(!working.decoder_is_inert());
        assert_eq!(working.stats().decoded, 1);

        // Dropped packets never reach the decoder, and JPEG has no decoder to judge.
        let mut waiting = ReceiveTrack::new();
        let mut jpeg_only = ReceiveTrack::new();
        for seq in 1..100 {
            waiting.on_packet(&h264(seq, false), u64::from(seq) * 66);
            jpeg_only.on_packet(&jpeg(seq), u64::from(seq) * 66);
            jpeg_only.decoded(0);
        }
        assert!(!waiting.decoder_is_inert());
        assert!(!jpeg_only.decoder_is_inert());
    }

    #[test]
    fn a_new_encoder_starts_with_a_forced_keyframe() {
        let mut policy = KeyframePolicy::new();
        assert!(policy.should_force(0));
        // Nothing came out yet (an encoder may lag): the next frame is forced too.
        assert!(policy.should_force(66));
        policy.on_output(true, 70);
        assert!(!policy.should_force(133));
        assert_eq!(policy.stats().keyframes, 1);
        assert_eq!((policy.stats().on_request, policy.stats().periodic), (0, 0));
    }

    #[test]
    fn the_sender_forces_a_keyframe_when_asked_but_at_most_twice_a_second() {
        let mut policy = KeyframePolicy::new();
        assert!(policy.should_force(0));
        policy.on_output(true, 0);
        assert!(!policy.should_force(66));
        policy.on_output(false, 66);
        // Two peers ask at once: one keyframe answers both.
        policy.request();
        policy.request();
        assert!(policy.should_force(700));
        policy.on_output(true, 700);
        assert!(!policy.should_force(766));
        policy.on_output(false, 766);
        // Asked again within half a second of the last forced keyframe: it waits its turn.
        policy.request();
        assert!(!policy.should_force(833));
        policy.on_output(false, 833);
        assert!(policy.should_force(1200));
        policy.on_output(true, 1200);
        assert!(!policy.should_force(1266), "the request was answered");
        let stats = policy.stats();
        assert_eq!(
            (stats.keyframes, stats.on_request, stats.periodic),
            (3, 2, 0)
        );
        assert!(!policy.must_reopen());
    }

    #[test]
    fn a_keyframe_is_forced_every_three_seconds_as_a_floor() {
        let mut policy = KeyframePolicy::new();
        assert!(policy.should_force(0));
        policy.on_output(true, 0);
        for now in (66..PERIODIC_KEYFRAME_MS).step_by(66) {
            assert!(!policy.should_force(now), "{now} ms");
            policy.on_output(false, now);
        }
        assert!(policy.should_force(PERIODIC_KEYFRAME_MS));
        policy.on_output(true, PERIODIC_KEYFRAME_MS);
        assert_eq!(policy.stats().periodic, 1);
        // A keyframe the encoder made on its own moves the floor too.
        assert!(!policy.should_force(4000));
        policy.on_output(true, 4000);
        assert!(!policy.should_force(6500));
        policy.on_output(false, 6500);
        assert!(policy.should_force(7000));
        assert_eq!(policy.stats().periodic, 2);
    }

    /// The encoder works on its own thread: a forced frame's packet comes out after the packets
    /// of the frames queued before it, which are P-frames. Those are no reason to reopen it.
    #[test]
    fn a_forced_keyframe_that_comes_out_a_few_packets_later_is_no_reason_to_reopen() {
        let mut policy = KeyframePolicy::new();
        assert!(policy.should_force(0));
        policy.on_output(true, 0);
        policy.request();
        assert!(policy.should_force(600));
        for i in 0..u64::from(OUTPUT_LAG_PACKETS) {
            policy.on_output(false, 600 + i);
            assert!(!policy.must_reopen(), "packet {i} of a frame queued before the forced one");
        }
        policy.on_output(true, 700);
        assert!(!policy.must_reopen());
        assert!(!policy.should_force(766), "the request was answered");
    }

    #[test]
    fn an_encoder_that_ignores_a_forced_keyframe_is_reopened() {
        let mut policy = KeyframePolicy::new();
        assert!(policy.should_force(0));
        policy.on_output(true, 0);
        policy.request();
        assert!(policy.should_force(600));
        // The packets of the frames queued before it, then the forced frame's own: a P-frame.
        for i in 0..=u64::from(OUTPUT_LAG_PACKETS) {
            policy.on_output(false, 600 + i);
        }
        assert!(policy.must_reopen());
        policy.reopened();
        assert!(!policy.must_reopen());
        // The new encoder's first frame is forced, and the request is still open.
        assert!(policy.should_force(633));
        policy.on_output(true, 633);
        assert!(!policy.is_broken());
        assert_eq!(policy.stats().reopened, 1);

        // Reopened three times in a row and never a keyframe: H.264 is given up.
        let mut broken = KeyframePolicy::new();
        for round in 0..u64::from(MAX_REOPENS) {
            assert!(!broken.is_broken());
            for i in 0..=u64::from(OUTPUT_LAG_PACKETS) {
                // A fresh encoder forces every frame until a keyframe comes out.
                assert!(broken.should_force(round * 100 + i));
                broken.on_output(false, round * 100 + i);
            }
            assert!(broken.must_reopen());
            broken.reopened();
        }
        assert!(broken.is_broken());
    }

    /// H.264 when both sides do it, JPEG when one cannot, nothing while the viewer's caps are
    /// unknown: a Mac never gets JPEG because its caps arrived a moment late.
    #[test]
    fn a_viewer_whose_caps_have_not_arrived_gets_nothing_not_jpeg() {
        assert_eq!(wire_codec(true, None), None);
        assert_eq!(wire_codec(false, None), None);
        assert_eq!(wire_codec(true, Some(true)), Some(Codec::H264));
        assert_eq!(wire_codec(true, Some(false)), Some(Codec::Jpeg));
        assert_eq!(wire_codec(false, Some(true)), Some(Codec::Jpeg));
    }

    #[test]
    fn a_new_peer_gets_nothing_before_a_keyframe() {
        let mut window = SendWindow::new();
        assert!(window.wants_keyframe());
        assert!(!window.offer(1, false));
        assert!(!window.offer(2, false));
        assert!(window.offer(3, true));
        assert!(!window.wants_keyframe());
        assert!(window.offer(4, false));
        assert_eq!(window.in_flight(), 2);
        assert_eq!(window.skipped(), 2);
    }

    #[test]
    fn a_peer_that_falls_behind_is_paused_and_resumes_at_a_keyframe() {
        let mut window = SendWindow::new();
        let max = MAX_IN_FLIGHT as u32;
        assert!(window.offer(1, true));
        for seq in 2..=max {
            assert!(window.offer(seq, false), "packet {seq}");
        }
        assert_eq!(window.in_flight(), MAX_IN_FLIGHT);
        // Nothing acknowledged: the link is full.
        assert!(!window.offer(max + 1, false));
        assert!(!window.offer(max + 2, true), "not even a keyframe fits");
        assert!(!window.wants_keyframe(), "it first catches up");
        window.acked(max / 2);
        assert_eq!(window.in_flight(), MAX_IN_FLIGHT / 2);
        assert!(window.wants_keyframe());
        assert!(
            !window.offer(max + 3, false),
            "a P-frame it could not decode"
        );
        assert!(window.offer(max + 4, true));
        assert!(window.offer(max + 5, false));
        assert_eq!(window.skipped(), 3);
    }

    #[test]
    fn acknowledgements_release_the_window_in_order() {
        let mut window = SendWindow::new();
        for (seq, keyframe) in [(1, true), (2, false), (3, false), (4, false)] {
            assert!(window.offer(seq, keyframe));
        }
        window.acked(2);
        assert_eq!(window.in_flight(), 2);
        window.acked(1);
        assert_eq!(
            window.in_flight(),
            2,
            "an older acknowledgement changes nothing"
        );
        window.acked(10);
        assert_eq!(window.in_flight(), 0);

        // Sequence numbers wrap.
        let mut wrapping = SendWindow::new();
        for (seq, keyframe) in [
            (u32::MAX - 1, true),
            (u32::MAX, false),
            (0, false),
            (1, false),
        ] {
            assert!(wrapping.offer(seq, keyframe));
        }
        wrapping.acked(0);
        assert_eq!(wrapping.in_flight(), 1);
    }

    #[test]
    fn an_encoder_that_gives_nothing_back_is_found_inert() {
        let mut health = EncoderHealth::default();
        for _ in 1..ENCODER_INERT_AFTER {
            health.submitted();
        }
        assert!(!health.is_inert());
        health.submitted();
        assert!(health.is_inert());

        let mut working = EncoderHealth::default();
        working.submitted();
        working.produced();
        for _ in 0..100 {
            working.submitted();
        }
        assert!(!working.is_inert());
    }

    #[test]
    fn the_test_pattern_is_eight_colour_bars_moving_left() {
        let first = test_pattern(320, 180, 0);
        assert_eq!(first.len(), 320 * 180 * 4);
        assert!(first.chunks_exact(4).all(|p| p[3] == 255), "opaque");
        assert_eq!(pixel(&first, 0), [235, 235, 235, 255], "white first");
        assert_eq!(pixel(&first, 40), [235, 235, 16, 255], "then yellow");
        assert_eq!(pixel(&first, 319), [16, 16, 16, 255], "black last");
        assert_eq!(
            &first[..320 * 4],
            &first[320 * 4..640 * 4],
            "every row alike"
        );
        let third = test_pattern(320, 180, 3);
        assert_ne!(first, third);
        for x in 0..320 {
            let moved = (x + 3 * PATTERN_STEP as usize) % 320;
            assert_eq!(pixel(&third, x), pixel(&first, moved), "x {x}");
        }
        assert!(test_pattern(0, 180, 1).is_empty());
        assert!(test_pattern(320, 0, 1).is_empty());
    }

    #[test]
    fn the_pattern_clock_hands_out_each_frame_once_and_skips_after_a_stall() {
        let mut clock = PatternClock::new(15);
        assert_eq!(clock.next(0), Some(0));
        assert_eq!(clock.next(10), None);
        assert_eq!(clock.next(66), None);
        assert_eq!(clock.next(67), Some(1));
        assert_eq!(clock.next(1000), Some(15), "frames 2 to 14 are skipped");
        assert_eq!(clock.next(1000), None);
    }

    #[test]
    fn the_codec_line_says_h264_or_why_jpeg() {
        assert_eq!(
            codec_line(Ok("VideoToolbox"), &[]),
            "Video: H.264 (VideoToolbox)"
        );
        assert_eq!(
            codec_line(Err("no encoder"), &[]),
            "Video: JPEG (no encoder)"
        );
        assert_eq!(
            codec_line(Ok("VideoToolbox"), &[String::from("Ben")]),
            "Video: H.264 (VideoToolbox); JPEG to Ben (no H.264 decoder)"
        );
        assert_eq!(
            codec_line(Ok("VT"), &[String::from("Ben"), String::from("Cy")]),
            "Video: H.264 (VT); JPEG to Ben, Cy (no H.264 decoder)"
        );
        assert_eq!(
            codec_line(Err("no encoder"), &[String::from("Ben")]),
            "Video: JPEG (no encoder)"
        );
    }

    #[test]
    fn the_video_and_sending_lines_count_what_happened() {
        let stats = ReceiveStats {
            packets: 310,
            keyframes: 12,
            gaps: 1,
            late: 0,
            dropped: 3,
            requests: 1,
            decoded: 300,
        };
        assert_eq!(
            video_line("Ben", "camera", Codec::H264, &stats),
            "Video from Ben (camera): H.264, decoded 300, keyframes 12, gaps 1, dropped 3, \
             keyframe requests 1"
        );
        let keys = KeyframeStats {
            keyframes: 8,
            on_request: 2,
            periodic: 5,
            reopened: 0,
        };
        assert_eq!(
            send_line("camera", 450, 0, &keys, 1),
            "Sending camera: 450 H.264 packets, 0 JPEG frames, 8 keyframes, 2 on request, 5 \
             periodic, 0 reopens, 1 dropped on purpose"
        );
    }
}
