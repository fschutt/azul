//! The routing rules of AzMeet for rooms of three and more, free of azul types so
//! `cargo test -p AzMeet` checks them without a network: who sends whose media to whom, which
//! renditions a sender encodes and which one each viewer gets, the capacity report every
//! participant sends (a `ConnectionSync`), the envelope a forwarded packet travels in, the uplink
//! estimate, and the lines of the network panel.
//!
//! # Plan
//!
//! Every participant reports its capacity and the tiles it shows ([`Sync`]). Each one feeds the
//! same reports to azul's `IrohLoadBalancer`, which ranks them into the same backbone everywhere
//! (with the room's mesh cap: up to that many people everyone is on it). From the participants and
//! the backbone, [`Plan`] builds one tree per origin:
//!
//! - a backbone peer sends its own media to every other backbone peer and to its leaves;
//! - a leaf sends its own media once, to the backbone peer it is attached to (leaves in key order,
//!   round robin over the backbone, best first), and receives everything from that peer;
//! - a leaf's parent passes the leaf's media on to the other backbone peers and its other leaves,
//!   and every backbone peer passes what it receives on to its own leaves.
//!
//! With everyone on the backbone (a room within the mesh cap) the trees are the full mesh. A
//! stream takes at most three hops: leaf, its parent, another backbone peer, that peer's leaf.
//!
//! # Renditions
//!
//! A viewer asks for the height its tile needs (`IrohTileRole::rendition_height`, 0 for a hidden
//! tile). The sender encodes the smallest and the largest height asked for, so at most two, and
//! each viewer gets the smallest encoded one at least as tall as its tile ([`assign`]): H.264 when
//! the sender encodes it and the viewer decodes it, else JPEG. A forwarder passes each child only
//! the streams someone at or below that child gets ([`Plan::next_hops`]). Nobody asking means
//! nothing is encoded.
//!
//! # Wire, little endian
//!
//! ```text
//! [6][version 1][flags u8][count u8][uplink_kbps u32][stability u16, thousandths]   ConnectionSync
//!    then count times [origin u64][track u8][height u16]                   the tiles it shows
//! [7][version 1][track u32][origin u64][from u64] then the inner message or frame      relayed
//! ```
//!
//! Sync flags: bit 0 on battery, bit 1 relay only, bit 2 opted out of forwarding, bit 3 sends
//! audio, bit 4 sends its camera, bit 5 sends its screen. Syncs are reliable messages to every
//! connected peer, sent on connect, on every change and every two seconds; bytes after the listed
//! tiles are ignored, so later versions can append fields.
//!
//! A relayed item names the stream's origin and who wrote the inner bytes: the origin for media
//! (a video packet or an audio frame), the viewer for a keyframe request or an acknowledgement,
//! which travel toward the origin. `track` is the audio track for an audio frame, else the video
//! track of the inner packet or control. Relayed H.264 packets and controls ride messages; relayed
//! audio and JPEG frames ride frames, each origin and rendition on its own frame track
//! ([`frame_track`]), so on the latest-wins frame path no origin's frame replaces another's.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

/// Kind byte of a `ConnectionSync`.
pub const KIND_SYNC: u8 = 6;
/// Kind byte of a relayed message or frame.
pub const KIND_RELAY: u8 = 7;
/// Version byte of a `ConnectionSync`.
pub const SYNC_VERSION: u8 = 1;
/// Version byte of a relayed item.
pub const RELAY_VERSION: u8 = 1;
/// Bytes before a relayed item's inner bytes.
pub const RELAY_HEADER_BYTES: usize = 22;
/// Rooms of up to this many people send everything directly (`AZMEET_MESH_CAP`); the design's
/// value is 8, a smaller one lets three people try the backbone.
pub const DEFAULT_MESH_CAP: u32 = 4;
/// What a participant reports before it measured anything (kbit/s).
pub const DEFAULT_UPLINK_KBPS: u32 = 4000;
/// Uplink estimates are reported in these steps (kbit/s), so small changes move nothing.
pub const UPLINK_STEPS: [u32; 10] = [
    250, 500, 1000, 2000, 4000, 8000, 16_000, 32_000, 64_000, 100_000,
];
/// Intervals a new uplink step must be measured in a row before it is reported.
pub const STICKY_INTERVALS: u32 = 3;
/// Intervals the stability counts (30 s at one every 2 s).
pub const STABILITY_WINDOW: usize = 15;
/// Packets lost in one interval that make it unstable.
pub const LOSS_SPIKE: u64 = 3;
/// An RTT above twice the path's lowest, and at least this much above it (us), makes an interval
/// unstable.
pub const RTT_JUMP_US: u64 = 20_000;

/// A participant's key in the plan: FNV-1a of its endpoint id, trimmed and lower-cased, so every
/// side computes the same key.
pub fn peer_key(node_id: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in node_id.trim().bytes() {
        hash ^= u64::from(byte.to_ascii_lowercase());
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

// ---- ConnectionSync ----

/// A tile the sender of a [`Sync`] shows: `origin`'s `track` at `height` (0: hidden).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Want {
    pub origin: u64,
    pub track: u32,
    pub height: u16,
}

/// What a participant reports about itself every two seconds (a `ConnectionSync`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Sync {
    /// Estimated upload capacity (kbit/s).
    pub uplink_kbps: u32,
    /// Share of recent intervals without a loss spike or an RTT jump, in thousandths.
    pub stability_permille: u16,
    pub on_battery: bool,
    /// No direct path to anyone: every connection goes through a relay.
    pub relay_only: bool,
    /// The user declined to forward other people's media.
    pub opted_out: bool,
    /// The microphone is on.
    pub sends_audio: bool,
    pub sends_camera: bool,
    pub sends_screen: bool,
    /// The tiles it shows.
    pub wants: Vec<Want>,
}

impl Sync {
    /// The height this participant shows `origin`'s `track` at; 0 when it does not show it.
    pub fn want(&self, origin: u64, track: u32) -> u16 {
        self.wants
            .iter()
            .find(|w| w.origin == origin && w.track == track)
            .map_or(0, |w| w.height)
    }
}

const SYNC_HEADER_BYTES: usize = 10;
const WANT_BYTES: usize = 11;

/// The message carrying `sync`. More than 255 tiles keep the first 255.
pub fn encode_sync(sync: &Sync) -> Vec<u8> {
    let wants = &sync.wants[..sync.wants.len().min(usize::from(u8::MAX))];
    let flags = [
        sync.on_battery,
        sync.relay_only,
        sync.opted_out,
        sync.sends_audio,
        sync.sends_camera,
        sync.sends_screen,
    ]
    .into_iter()
    .enumerate()
    .fold(0u8, |acc, (bit, on)| if on { acc | 1 << bit } else { acc });
    let mut out = Vec::with_capacity(SYNC_HEADER_BYTES + WANT_BYTES * wants.len());
    out.extend_from_slice(&[KIND_SYNC, SYNC_VERSION, flags, wants.len() as u8]);
    out.extend_from_slice(&sync.uplink_kbps.to_le_bytes());
    out.extend_from_slice(&sync.stability_permille.min(1000).to_le_bytes());
    for w in wants {
        out.extend_from_slice(&w.origin.to_le_bytes());
        out.push(w.track.min(u32::from(u8::MAX)) as u8);
        out.extend_from_slice(&w.height.to_le_bytes());
    }
    out
}

/// Reads a `ConnectionSync`; `None` for another kind or version, or too few bytes for its tiles.
pub fn decode_sync(bytes: &[u8]) -> Option<Sync> {
    let head = bytes.get(..SYNC_HEADER_BYTES)?;
    if head[0] != KIND_SYNC || head[1] != SYNC_VERSION {
        return None;
    }
    let flags = head[2];
    let count = usize::from(head[3]);
    let mut wants = Vec::with_capacity(count);
    for i in 0..count {
        let at = SYNC_HEADER_BYTES + i * WANT_BYTES;
        let w = bytes.get(at..at + WANT_BYTES)?;
        wants.push(Want {
            origin: u64::from_le_bytes(<[u8; 8]>::try_from(&w[..8]).ok()?),
            track: u32::from(w[8]),
            height: u16::from_le_bytes([w[9], w[10]]),
        });
    }
    let bit = |n: u8| flags & (1 << n) != 0;
    Some(Sync {
        uplink_kbps: u32::from_le_bytes([head[4], head[5], head[6], head[7]]),
        stability_permille: u16::from_le_bytes([head[8], head[9]]).min(1000),
        on_battery: bit(0),
        relay_only: bit(1),
        opted_out: bit(2),
        sends_audio: bit(3),
        sends_camera: bit(4),
        sends_screen: bit(5),
        wants,
    })
}

// ---- Relayed items ----

/// A message or frame one participant passes on for another.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Relayed<'a> {
    /// The audio track for an audio frame, else the video track the inner bytes are about.
    pub track: u32,
    /// Whose stream it is.
    pub origin: u64,
    /// Who wrote the inner bytes: the origin for media, the viewer for a control.
    pub from: u64,
    /// The message or frame as its writer sent it.
    pub inner: &'a [u8],
}

/// `inner` wrapped for passing on.
pub fn encode_relay(track: u32, origin: u64, from: u64, inner: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(RELAY_HEADER_BYTES + inner.len());
    out.extend_from_slice(&[KIND_RELAY, RELAY_VERSION]);
    out.extend_from_slice(&track.to_le_bytes());
    out.extend_from_slice(&origin.to_le_bytes());
    out.extend_from_slice(&from.to_le_bytes());
    out.extend_from_slice(inner);
    out
}

/// Reads a relayed item; `None` for another kind or version, a short header, or nothing inside.
pub fn decode_relay(bytes: &[u8]) -> Option<Relayed<'_>> {
    let head = bytes.get(..RELAY_HEADER_BYTES)?;
    if head[0] != KIND_RELAY || head[1] != RELAY_VERSION {
        return None;
    }
    let inner = &bytes[RELAY_HEADER_BYTES..];
    if inner.is_empty() {
        return None;
    }
    Some(Relayed {
        track: u32::from_le_bytes(<[u8; 4]>::try_from(&head[2..6]).ok()?),
        origin: u64::from_le_bytes(<[u8; 8]>::try_from(&head[6..14]).ok()?),
        from: u64::from_le_bytes(<[u8; 8]>::try_from(&head[14..22]).ok()?),
        inner,
    })
}

/// The iroh frame track the `height` rendition of `track` rides: lane 0 for this side's own
/// streams, lane n (from 1) for the n-th origin this side relays. iroh keeps only the newest frame
/// per frame track, so no rendition or origin may share one. This side's own audio (height 0) keeps
/// its track number.
pub fn frame_track(lane: u32, track: u32, height: u16) -> u32 {
    lane.wrapping_mul(0x1000)
        .wrapping_add(rung(height) * 0x10)
        .wrapping_add(track & 0xf)
}

/// 0 for no rendition, else 1 to 4 up the ladder.
fn rung(height: u16) -> u32 {
    match height {
        0 => 0,
        1..=90 => 1,
        91..=180 => 2,
        181..=360 => 3,
        _ => 4,
    }
}

// ---- Renditions ----

/// Width of the 16:9 rendition `height` pixels tall, rounded to even (160, 320, 640, 1280).
pub fn rendition_width(height: u16) -> u32 {
    (u32::from(height) * 16 / 9 + 1) & !1
}

/// The heights to encode for the heights viewers asked for (0: hidden): none, or the smallest and
/// the largest.
pub fn encode_set(asked: &[u16]) -> Vec<u16> {
    let shown = || asked.iter().copied().filter(|h| *h > 0);
    match (shown().min(), shown().max()) {
        (Some(low), Some(high)) if low == high => vec![low],
        (Some(low), Some(high)) => vec![low, high],
        _ => Vec::new(),
    }
}

/// The encoded height a viewer needing `need` gets: the smallest at least as tall, else the
/// tallest; `None` for a hidden tile or nothing encoded.
pub fn pick(encoded: &[u16], need: u16) -> Option<u16> {
    if need == 0 {
        return None;
    }
    let taller = encoded.iter().copied().filter(|h| *h >= need).min();
    taller.or_else(|| encoded.iter().copied().max())
}

/// One stream of a sender's track: a rendition in one codec.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Stream {
    pub height: u16,
    /// H.264, else JPEG.
    pub h264: bool,
}

/// One viewer of a sender's track.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Viewer {
    pub key: u64,
    /// The height its tile needs; 0 when it does not show the track.
    pub need: u16,
    /// It decodes H.264.
    pub h264: bool,
}

/// Which stream each viewer gets: the rendition [`pick`] chooses from [`encode_set`], in H.264
/// when the sender encodes it (`sender_h264`) and the viewer decodes it. Viewers that do not show
/// the track get nothing.
pub fn assign(viewers: &[Viewer], sender_h264: bool) -> BTreeMap<u64, Stream> {
    let asked: Vec<u16> = viewers.iter().map(|v| v.need).collect();
    let encoded = encode_set(&asked);
    viewers
        .iter()
        .filter_map(|v| {
            let height = pick(&encoded, v.need)?;
            Some((
                v.key,
                Stream {
                    height,
                    h264: sender_h264 && v.h264,
                },
            ))
        })
        .collect()
}

/// The distinct streams of an assignment, in order.
pub fn streams(assigned: &BTreeMap<u64, Stream>) -> Vec<Stream> {
    let set: BTreeSet<Stream> = assigned.values().copied().collect();
    set.into_iter().collect()
}

/// The distinct heights of an assignment, smallest first: the renditions to produce.
pub fn heights(assigned: &BTreeMap<u64, Stream>) -> Vec<u16> {
    let set: BTreeSet<u16> = assigned.values().map(|s| s.height).collect();
    set.into_iter().collect()
}

// ---- Plan ----

/// Who sends whose media to whom, from the participants and the backbone. The same inputs give
/// the same plan on every side, in whatever order they come.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Plan {
    /// Every participant, by key.
    peers: Vec<u64>,
    /// The forwarders, best first.
    backbone: Vec<u64>,
    /// Each leaf's backbone parent.
    attach: BTreeMap<u64, u64>,
}

impl Plan {
    /// The plan for `peers` with the forwarders `backbone` (best first, as
    /// `IrohLoadBalancer::backbone_peer` lists them). Backbone entries that are no participant are
    /// ignored; with no forwarder at all everyone sends directly (the full mesh).
    pub fn new(peers: &[u64], backbone: &[u64]) -> Plan {
        let set: BTreeSet<u64> = peers.iter().copied().collect();
        let peers: Vec<u64> = set.iter().copied().collect();
        let mut chosen: Vec<u64> = Vec::new();
        for p in backbone {
            if set.contains(p) && !chosen.contains(p) {
                chosen.push(*p);
            }
        }
        if chosen.is_empty() {
            chosen = peers.clone();
        }
        let attach = peers
            .iter()
            .filter(|p| !chosen.contains(p))
            .enumerate()
            .map(|(i, leaf)| (*leaf, chosen[i % chosen.len()]))
            .collect();
        Plan {
            peers,
            backbone: chosen,
            attach,
        }
    }

    /// Every participant, by key.
    pub fn peers(&self) -> &[u64] {
        &self.peers
    }

    pub fn contains(&self, peer: u64) -> bool {
        self.peers.binary_search(&peer).is_ok()
    }

    pub fn is_backbone(&self, peer: u64) -> bool {
        self.backbone.contains(&peer)
    }

    /// Everyone forwards: every stream goes directly from its origin to every viewer.
    pub fn is_mesh(&self) -> bool {
        self.attach.is_empty()
    }

    /// The backbone peer a leaf uploads to and receives from; `None` for a backbone peer.
    pub fn parent_of_leaf(&self, leaf: u64) -> Option<u64> {
        self.attach.get(&leaf).copied()
    }

    fn leaves_of(&self, parent: u64) -> impl Iterator<Item = u64> + '_ {
        self.attach
            .iter()
            .filter(move |(_, p)| **p == parent)
            .map(|(leaf, _)| *leaf)
    }

    /// Whom `at` sends `origin`'s media to: its own media when `at` is the origin, else what it
    /// passes on. Sorted.
    pub fn children(&self, at: u64, origin: u64) -> Vec<u64> {
        if !self.contains(at) || !self.contains(origin) {
            return Vec::new();
        }
        let others = |me: u64| self.backbone.iter().copied().filter(move |b| *b != me);
        let mut out: Vec<u64> = if at == origin {
            match self.attach.get(&origin) {
                Some(parent) => vec![*parent],
                None => others(origin).chain(self.leaves_of(origin)).collect(),
            }
        } else if !self.is_backbone(at) {
            Vec::new()
        } else if self.attach.get(&origin) == Some(&at) {
            others(at)
                .chain(self.leaves_of(at).filter(|leaf| *leaf != origin))
                .collect()
        } else {
            self.leaves_of(at).collect()
        };
        out.sort_unstable();
        out
    }

    /// Whom `viewer` gets `origin`'s media from: the origin, or the peer passing it on. `None` for
    /// the origin itself or someone not in the plan.
    pub fn parent(&self, viewer: u64, origin: u64) -> Option<u64> {
        if viewer == origin || !self.contains(viewer) || !self.contains(origin) {
            return None;
        }
        if let Some(own) = self.attach.get(&viewer) {
            // A leaf gets everything from its parent.
            return Some(*own);
        }
        match self.attach.get(&origin) {
            // A backbone peer gets a leaf's media from the leaf's parent.
            Some(parent) if *parent != viewer => Some(*parent),
            _ => Some(origin),
        }
    }

    /// Whether `origin`'s media reaches `viewer` through `at` (or `at` is the viewer).
    pub fn reaches(&self, at: u64, origin: u64, viewer: u64) -> bool {
        let mut hop = viewer;
        for _ in 0..=self.peers.len() {
            if hop == at {
                return true;
            }
            match self.parent(hop, origin) {
                Some(up) => hop = up,
                None => return false,
            }
        }
        false
    }

    /// Whom `at` sends `stream` of `origin` to: its children with a viewer of that stream at or
    /// below them (`assigned` is what [`assign`] gave `origin`'s viewers).
    pub fn next_hops(
        &self,
        at: u64,
        origin: u64,
        stream: Stream,
        assigned: &BTreeMap<u64, Stream>,
    ) -> Vec<u64> {
        self.children(at, origin)
            .into_iter()
            .filter(|child| {
                assigned
                    .iter()
                    .any(|(viewer, got)| *got == stream && self.reaches(*child, origin, *viewer))
            })
            .collect()
    }

    /// Every hop of `origin`'s tree as (from, to), sorted.
    pub fn edges(&self, origin: u64) -> Vec<(u64, u64)> {
        let mut out = Vec::new();
        for at in &self.peers {
            for child in self.children(*at, origin) {
                out.push((*at, child));
            }
        }
        out
    }
}

// ---- Uplink estimate ----

/// One connection's selected path as iroh reports it (`IrohPeerStats`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PathSample {
    pub bytes_sent: u64,
    pub lost_packets: u64,
    pub rtt_us: u64,
    pub cwnd_bytes: u64,
    pub direct: bool,
}

/// This participant's uplink and stability, from its connections' path statistics every interval:
/// the uplink is the larger of what was sent (bytes per second over all connections) and what one
/// path's congestion window allows (cwnd / RTT), in [`UPLINK_STEPS`], and a new step is reported
/// only after [`STICKY_INTERVALS`] intervals in a row, so the backbone does not flap. A pinned value
/// (`AZMEET_UPLINK_KBPS`) is reported instead of the estimate.
#[derive(Debug, Default)]
pub struct CapacityEstimator {
    pinned: Option<u32>,
    last: BTreeMap<u64, PathSample>,
    last_ms: Option<u64>,
    lowest_rtt: BTreeMap<u64, u64>,
    history: VecDeque<bool>,
    measured: Option<u32>,
    reported: Option<u32>,
    /// A step measured but not reported yet, and in how many intervals in a row.
    candidate: Option<(u32, u32)>,
    relay_only: bool,
}

/// The largest step at or below `kbps`, at least the smallest.
fn uplink_step(kbps: u64) -> u32 {
    UPLINK_STEPS
        .iter()
        .rev()
        .copied()
        .find(|step| kbps >= u64::from(*step))
        .unwrap_or(UPLINK_STEPS[0])
}

impl CapacityEstimator {
    pub fn new(pinned: Option<u32>) -> Self {
        CapacityEstimator {
            pinned,
            ..CapacityEstimator::default()
        }
    }

    /// One interval: every connection's path (by connection handle) at `now_ms`.
    pub fn sample(&mut self, now_ms: u64, paths: &[(u64, PathSample)]) {
        self.relay_only = !paths.is_empty() && paths.iter().all(|(_, p)| !p.direct);
        let elapsed = self.last_ms.map(|at| now_ms.saturating_sub(at));
        let mut sent_kbps = 0u64;
        let mut window_kbps = 0u64;
        let mut stable = true;
        let mut compared = false;
        for (conn, now) in paths {
            if now.rtt_us > 0 {
                window_kbps = window_kbps.max(now.cwnd_bytes.saturating_mul(8000) / now.rtt_us);
                let lowest = self.lowest_rtt.entry(*conn).or_insert(now.rtt_us);
                *lowest = (*lowest).min(now.rtt_us);
                let jump = (*lowest)
                    .saturating_mul(2)
                    .max(lowest.saturating_add(RTT_JUMP_US));
                if now.rtt_us > jump {
                    stable = false;
                }
            }
            if let (Some(before), Some(ms)) = (self.last.get(conn), elapsed) {
                compared = true;
                if ms > 0 {
                    // Bits per millisecond are kbit/s.
                    sent_kbps += now.bytes_sent.saturating_sub(before.bytes_sent) * 8 / ms;
                }
                if now.lost_packets.saturating_sub(before.lost_packets) >= LOSS_SPIKE {
                    stable = false;
                }
            }
        }
        self.last = paths.iter().copied().collect();
        let live: BTreeSet<u64> = self.last.keys().copied().collect();
        self.lowest_rtt.retain(|conn, _| live.contains(conn));
        self.last_ms = Some(now_ms);
        if paths.is_empty() {
            return;
        }
        if compared {
            if self.history.len() == STABILITY_WINDOW {
                self.history.pop_front();
            }
            self.history.push_back(stable);
        }
        let step = uplink_step(sent_kbps.max(window_kbps));
        self.measured = Some(step);
        match self.reported {
            None => self.reported = Some(step),
            Some(current) if current == step => self.candidate = None,
            Some(_) => {
                let seen = match self.candidate {
                    Some((pending, times)) if pending == step => times + 1,
                    _ => 1,
                };
                if seen >= STICKY_INTERVALS {
                    self.reported = Some(step);
                    self.candidate = None;
                } else {
                    self.candidate = Some((step, seen));
                }
            }
        }
    }

    /// The uplink to report (kbit/s): the pinned value, else the estimate, else
    /// [`DEFAULT_UPLINK_KBPS`].
    pub fn uplink_kbps(&self) -> u32 {
        self.pinned.or(self.reported).unwrap_or(DEFAULT_UPLINK_KBPS)
    }

    /// The step measured in the last interval, whatever is reported.
    pub fn measured_kbps(&self) -> Option<u32> {
        self.measured
    }

    pub fn is_pinned(&self) -> bool {
        self.pinned.is_some()
    }

    /// Share of the last [`STABILITY_WINDOW`] intervals without a loss spike or an RTT jump, in
    /// tenths (as thousandths); 1000 before any interval.
    pub fn stability_permille(&self) -> u16 {
        let total = self.history.len();
        if total == 0 {
            return 1000;
        }
        let stable = self.history.iter().filter(|s| **s).count();
        (((stable * 10 + total / 2) / total) * 100) as u16
    }

    /// Every connection goes through a relay.
    pub fn relay_only(&self) -> bool {
        self.relay_only
    }
}

// ---- Network panel ----

/// "50 Mbps", "1.5 Mbps", "500 kbps".
pub fn kbps_label(kbps: u32) -> String {
    if kbps < 1000 {
        format!("{kbps} kbps")
    } else if kbps % 1000 == 0 {
        format!("{} Mbps", kbps / 1000)
    } else {
        format!("{:.1} Mbps", f64::from(kbps) / 1000.0)
    }
}

/// "camera 360p H.264", "screen 90p JPEG".
pub fn stream_label(source: &str, stream: Stream) -> String {
    let codec = if stream.h264 { "H.264" } else { "JPEG" };
    format!("{source} {}p {codec}", stream.height)
}

/// The panel's summary: "Network: 3 people, mesh cap 2: backbone Ben, Cleo; Ada uploads to Ben",
/// or "Network: 2 people, full mesh (mesh cap 4)".
pub fn plan_line(plan: &Plan, mesh_cap: u32, name: &dyn Fn(u64) -> String) -> String {
    let people = match plan.peers.len() {
        1 => String::from("1 person"),
        n => format!("{n} people"),
    };
    if plan.is_mesh() {
        return format!("Network: {people}, full mesh (mesh cap {mesh_cap})");
    }
    let backbone: Vec<String> = plan.backbone.iter().map(|p| name(*p)).collect();
    let mut leaves: Vec<String> = plan
        .attach
        .iter()
        .map(|(leaf, parent)| format!("{} uploads to {}", name(*leaf), name(*parent)))
        .collect();
    leaves.sort();
    format!(
        "Network: {people}, mesh cap {mesh_cap}: backbone {}; {}",
        backbone.join(", "),
        leaves.join(", ")
    )
}

/// Every origin's tree by name, sorted: "Routes: Ada: Ada>Ben, Ben>Cleo | Ben: Ben>Ada, Ben>Cleo |
/// Cleo: Ben>Ada, Cleo>Ben". Every side that agrees on the plan shows the same line.
pub fn routes_line(plan: &Plan, name: &dyn Fn(u64) -> String) -> String {
    let mut trees: Vec<(String, String)> = plan
        .peers
        .iter()
        .map(|origin| {
            let mut hops: Vec<String> = plan
                .edges(*origin)
                .into_iter()
                .map(|(from, to)| format!("{}>{}", name(from), name(to)))
                .collect();
            hops.sort();
            let hops = if hops.is_empty() {
                String::from("-")
            } else {
                hops.join(", ")
            };
            (name(*origin), hops)
        })
        .collect();
    trees.sort();
    let parts: Vec<String> = trees
        .into_iter()
        .map(|(origin, hops)| format!("{origin}: {hops}"))
        .collect();
    format!("Routes: {}", parts.join(" | "))
}

/// This participant's part: "You: leaf, uploading once to Ben", "You: backbone, forwarding for
/// others", "You: full mesh, sending to everyone directly".
pub fn role_line(plan: &Plan, me: u64, name: &dyn Fn(u64) -> String) -> String {
    if !plan.contains(me) {
        String::from("You: not planned yet")
    } else if plan.is_mesh() {
        String::from("You: full mesh, sending to everyone directly")
    } else {
        match plan.parent_of_leaf(me) {
            Some(parent) => format!("You: leaf, uploading once to {}", name(parent)),
            None => String::from("You: backbone, forwarding for others"),
        }
    }
}

/// One peer of the network panel.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PeerRow {
    pub name: String,
    /// Direct (else relayed) and the RTT in ms, from the last statistics.
    pub path: Option<(bool, f64)>,
    /// On the backbone, else a leaf; `None` before its first report.
    pub backbone: Option<bool>,
    /// What it reports as its uplink.
    pub uplink_kbps: Option<u32>,
    /// What this side sends to it.
    pub to: Vec<String>,
    /// What this side gets from it.
    pub from: Vec<String>,
}

/// "Ben · direct 0.4 ms · backbone · up 50 Mbps · to Ben: camera 360p H.264, audio · from Ben:
/// Ben camera 360p H.264, Ben audio".
pub fn peer_line(row: &PeerRow) -> String {
    let mut parts = vec![row.name.clone()];
    parts.push(match row.path {
        Some((true, rtt)) => format!("direct {rtt:.1} ms"),
        Some((false, rtt)) => format!("relayed {rtt:.1} ms"),
        None => String::from("no path yet"),
    });
    parts.push(String::from(match row.backbone {
        Some(true) => "backbone",
        Some(false) => "leaf",
        None => "no report yet",
    }));
    if let Some(up) = row.uplink_kbps {
        parts.push(format!("up {}", kbps_label(up)));
    }
    let list = |items: &[String]| {
        if items.is_empty() {
            String::from("nothing")
        } else {
            items.join(", ")
        }
    };
    parts.push(format!("to {}: {}", row.name, list(&row.to)));
    parts.push(format!("from {}: {}", row.name, list(&row.from)));
    parts.join(" · ")
}

#[cfg(test)]
mod tests {
    use super::*;

    const ADA: u64 = 10;
    const BEN: u64 = 20;
    const CLEO: u64 = 30;

    fn name(key: u64) -> String {
        match key {
            ADA => String::from("Ada"),
            BEN => String::from("Ben"),
            CLEO => String::from("Cleo"),
            other => format!("p{other}"),
        }
    }

    /// Ada, Ben and Cleo above a mesh cap of 2: Ben and Cleo forward, Ada is Ben's leaf.
    fn three() -> Plan {
        Plan::new(&[ADA, BEN, CLEO], &[BEN, CLEO])
    }

    fn stream(height: u16) -> Stream {
        Stream { height, h264: true }
    }

    fn path(bytes_sent: u64, lost_packets: u64, rtt_us: u64, cwnd_bytes: u64) -> PathSample {
        PathSample {
            bytes_sent,
            lost_packets,
            rtt_us,
            cwnd_bytes,
            direct: true,
        }
    }

    #[test]
    fn a_peer_key_is_the_same_for_the_same_endpoint_id_on_every_side() {
        assert_eq!(peer_key("ab12CD"), peer_key(" ab12cd\n"));
        assert_ne!(peer_key("ab12cd"), peer_key("ab12ce"));
        // FNV-1a 64 of "a".
        assert_eq!(peer_key("a"), 0xaf63_dc4c_8601_ec8c);
    }

    #[test]
    fn a_connection_sync_round_trips_through_the_wire_format() {
        let sync = Sync {
            uplink_kbps: 50_000,
            stability_permille: 900,
            on_battery: true,
            relay_only: false,
            opted_out: true,
            sends_audio: true,
            sends_camera: true,
            sends_screen: false,
            wants: vec![
                Want {
                    origin: 0x0102_0304_0506_0708,
                    track: 1,
                    height: 360,
                },
                Want {
                    origin: 7,
                    track: 2,
                    height: 90,
                },
            ],
        };
        let bytes = encode_sync(&sync);
        assert_eq!(bytes.len(), 10 + 2 * 11);
        assert_eq!(&bytes[..4], &[KIND_SYNC, SYNC_VERSION, 0b01_1101, 2]);
        assert_eq!(&bytes[4..8], &50_000u32.to_le_bytes());
        assert_eq!(&bytes[8..10], &900u16.to_le_bytes());
        assert_eq!(&bytes[10..18], &[8, 7, 6, 5, 4, 3, 2, 1]);
        assert_eq!(&bytes[18..21], &[1, 0x68, 0x01]);
        assert_eq!(decode_sync(&bytes), Some(sync.clone()));
        assert_eq!(sync.want(7, 2), 90);
        assert_eq!(sync.want(7, 1), 0);
        // Later versions may append fields.
        let mut longer = bytes.clone();
        longer.extend_from_slice(&[9, 9]);
        assert_eq!(decode_sync(&longer), Some(sync));
        assert_eq!(decode_sync(&bytes[..bytes.len() - 1]), None);
        assert_eq!(decode_sync(&[KIND_SYNC, 2, 0, 0, 0, 0, 0, 0, 0, 0]), None);
        assert_eq!(decode_sync(&[5, 1]), None);
    }

    #[test]
    fn a_relayed_item_names_its_origin_and_writer_and_keeps_the_inner_bytes() {
        let inner = [3u8, 1, 0, 0, 0, 90, 0];
        let bytes = encode_relay(1, ADA, CLEO, &inner);
        assert_eq!(bytes.len(), RELAY_HEADER_BYTES + inner.len());
        assert_eq!(&bytes[..2], &[KIND_RELAY, RELAY_VERSION]);
        assert_eq!(
            decode_relay(&bytes),
            Some(Relayed {
                track: 1,
                origin: ADA,
                from: CLEO,
                inner: &inner,
            })
        );
        assert_eq!(decode_relay(&encode_relay(1, ADA, ADA, &[])), None);
        assert_eq!(decode_relay(&bytes[..RELAY_HEADER_BYTES - 1]), None);
        assert_eq!(decode_relay(&inner), None);
    }

    #[test]
    fn relayed_origins_and_renditions_ride_separate_frame_tracks() {
        // This side's own audio keeps the audio track.
        assert_eq!(frame_track(0, 3, 0), 3);
        let mut seen = BTreeSet::new();
        for lane in 0..4 {
            for track in 1..=3 {
                for height in [0, 90, 180, 360, 720] {
                    assert!(
                        seen.insert(frame_track(lane, track, height)),
                        "lane {lane} track {track} height {height} shares a frame track"
                    );
                }
            }
        }
    }

    #[test]
    fn a_rendition_is_sixteen_by_nine_with_even_sides() {
        let widths: Vec<u32> = [90, 180, 360, 720]
            .iter()
            .map(|h| rendition_width(*h))
            .collect();
        assert_eq!(widths, vec![160, 320, 640, 1280]);
        assert_eq!(rendition_width(100) % 2, 0);
    }

    #[test]
    fn a_sender_encodes_the_smallest_and_the_largest_height_asked_for() {
        assert_eq!(encode_set(&[]), Vec::<u16>::new());
        assert_eq!(encode_set(&[0, 0]), Vec::<u16>::new());
        assert_eq!(encode_set(&[360, 0, 360]), vec![360]);
        assert_eq!(encode_set(&[180, 90, 360, 0]), vec![90, 360]);
        assert_eq!(pick(&[90, 360], 90), Some(90));
        assert_eq!(pick(&[90, 360], 180), Some(360));
        assert_eq!(pick(&[90, 360], 720), Some(360));
        assert_eq!(pick(&[90, 360], 0), None);
        assert_eq!(pick(&[], 180), None);
    }

    #[test]
    fn each_viewer_gets_the_smallest_encoded_rendition_at_least_as_tall_as_its_tile() {
        let viewers = [
            Viewer {
                key: BEN,
                need: 360,
                h264: true,
            },
            Viewer {
                key: CLEO,
                need: 90,
                h264: true,
            },
            Viewer {
                key: 40,
                need: 180,
                h264: false,
            },
            Viewer {
                key: 50,
                need: 0,
                h264: true,
            },
        ];
        let got = assign(&viewers, true);
        assert_eq!(got.len(), 3, "a hidden tile gets nothing");
        assert_eq!(got[&BEN], stream(360));
        assert_eq!(got[&CLEO], stream(90));
        assert_eq!(
            got[&40],
            Stream {
                height: 360,
                h264: false,
            },
            "180 is not encoded: the next taller rendition, as JPEG to a peer without H.264"
        );
        assert_eq!(heights(&got), vec![90, 360]);
        assert_eq!(streams(&got).len(), 3);
        let without = assign(&viewers, false);
        assert!(
            without.values().all(|s| !s.h264),
            "a sender without H.264 sends JPEG"
        );
    }

    #[test]
    fn within_the_mesh_cap_everyone_sends_to_everyone_directly() {
        let plan = Plan::new(&[ADA, BEN, CLEO], &[CLEO, ADA, BEN]);
        assert!(plan.is_mesh());
        assert_eq!(plan.children(ADA, ADA), vec![BEN, CLEO]);
        assert_eq!(plan.children(BEN, ADA), Vec::<u64>::new());
        for origin in [ADA, BEN, CLEO] {
            for viewer in [ADA, BEN, CLEO] {
                let expected = (viewer != origin).then_some(origin);
                assert_eq!(plan.parent(viewer, origin), expected);
            }
        }
    }

    #[test]
    fn above_the_mesh_cap_a_leaf_uploads_once_to_its_backbone_parent() {
        let plan = three();
        assert!(!plan.is_mesh());
        assert_eq!(plan.parent_of_leaf(ADA), Some(BEN));
        assert_eq!(plan.children(ADA, ADA), vec![BEN], "Ada uploads once");
        assert_eq!(
            plan.children(BEN, ADA),
            vec![CLEO],
            "Ben passes Ada's media on"
        );
        assert_eq!(plan.children(CLEO, ADA), Vec::<u64>::new());
        assert_eq!(plan.parent(BEN, ADA), Some(ADA));
        assert_eq!(plan.parent(CLEO, ADA), Some(BEN));
    }

    #[test]
    fn a_leaf_receives_everything_through_its_parent() {
        let plan = three();
        assert_eq!(plan.parent(ADA, BEN), Some(BEN));
        assert_eq!(plan.parent(ADA, CLEO), Some(BEN));
        assert_eq!(plan.children(CLEO, CLEO), vec![BEN]);
        assert_eq!(plan.children(BEN, CLEO), vec![ADA]);
        assert_eq!(plan.children(BEN, BEN), vec![ADA, CLEO]);
    }

    #[test]
    fn every_viewer_is_reached_exactly_once_within_three_hops() {
        let rooms: Vec<(Vec<u64>, Vec<u64>)> = vec![
            (vec![1, 2, 3], vec![2, 3]),
            (vec![1, 2, 3, 4, 5, 6, 7], vec![5, 2, 7]),
            (vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11], vec![9]),
            (vec![4, 5, 6, 7], vec![4, 5, 6, 7]),
        ];
        for (peers, backbone) in rooms {
            let plan = Plan::new(&peers, &backbone);
            for origin in &peers {
                let edges = plan.edges(*origin);
                for viewer in &peers {
                    let incoming = edges.iter().filter(|(_, to)| to == viewer).count();
                    if viewer == origin {
                        assert_eq!(incoming, 0, "the origin gets its own media back");
                        continue;
                    }
                    assert_eq!(
                        incoming, 1,
                        "{viewer} gets {origin}'s media {incoming} times"
                    );
                    let parent = plan.parent(*viewer, *origin).unwrap();
                    assert!(plan.children(parent, *origin).contains(viewer));
                    let mut hops = 1;
                    let mut at = parent;
                    while at != *origin {
                        at = plan.parent(at, *origin).unwrap();
                        hops += 1;
                    }
                    assert!(hops <= 3, "{hops} hops from {origin} to {viewer}");
                    assert!(plan.reaches(*origin, *origin, *viewer));
                }
            }
        }
    }

    #[test]
    fn leaves_spread_over_the_backbone_in_key_order() {
        let plan = Plan::new(&[30, 10, 50, 20, 40], &[50, 40]);
        assert!(plan.is_backbone(50) && plan.is_backbone(40) && !plan.is_backbone(30));
        assert_eq!(plan.parent_of_leaf(10), Some(50));
        assert_eq!(plan.parent_of_leaf(20), Some(40));
        assert_eq!(plan.parent_of_leaf(30), Some(50));
        assert_eq!(plan.parent_of_leaf(50), None);
    }

    #[test]
    fn the_same_participants_in_any_order_give_the_same_plan() {
        let a = Plan::new(&[ADA, BEN, CLEO], &[BEN, CLEO]);
        let b = Plan::new(&[CLEO, ADA, BEN, ADA], &[BEN, 99, CLEO, BEN]);
        assert_eq!(a, b, "duplicates and unknown forwarders change nothing");
        assert_eq!(a.peers(), &[ADA, BEN, CLEO]);
    }

    #[test]
    fn with_nobody_able_to_forward_the_room_falls_back_to_the_full_mesh() {
        let plan = Plan::new(&[ADA, BEN, CLEO], &[]);
        assert!(plan.is_mesh());
        assert_eq!(plan.children(CLEO, CLEO), vec![ADA, BEN]);
        assert_eq!(Plan::new(&[], &[]).children(ADA, ADA), Vec::<u64>::new());
    }

    #[test]
    fn a_forwarder_passes_each_child_only_the_rendition_someone_below_it_gets() {
        let plan = three();
        // Ben shows Ada in a grid tile (360p), Cleo as a thumbnail (90p).
        let mut assigned = BTreeMap::new();
        assigned.insert(BEN, stream(360));
        assigned.insert(CLEO, stream(90));
        assert_eq!(plan.next_hops(ADA, ADA, stream(360), &assigned), vec![BEN]);
        assert_eq!(
            plan.next_hops(ADA, ADA, stream(90), &assigned),
            vec![BEN],
            "Ada uploads both renditions once, to Ben"
        );
        assert_eq!(plan.next_hops(BEN, ADA, stream(90), &assigned), vec![CLEO]);
        assert_eq!(
            plan.next_hops(BEN, ADA, stream(360), &assigned),
            Vec::<u64>::new()
        );
        assert_eq!(
            plan.next_hops(BEN, ADA, stream(180), &assigned),
            Vec::<u64>::new()
        );
        // Cleo decodes no H.264: she gets JPEG, and no H.264 passes Ben for her.
        assigned.insert(
            CLEO,
            Stream {
                height: 90,
                h264: false,
            },
        );
        assert_eq!(
            plan.next_hops(BEN, ADA, stream(90), &assigned),
            Vec::<u64>::new()
        );
        assert_eq!(
            plan.next_hops(
                BEN,
                ADA,
                Stream {
                    height: 90,
                    h264: false,
                },
                &assigned
            ),
            vec![CLEO]
        );
    }

    #[test]
    fn the_network_panel_names_the_backbone_and_every_route() {
        let plan = three();
        assert_eq!(
            plan_line(&plan, 2, &name),
            "Network: 3 people, mesh cap 2: backbone Ben, Cleo; Ada uploads to Ben"
        );
        assert_eq!(
            routes_line(&plan, &name),
            "Routes: Ada: Ada>Ben, Ben>Cleo | Ben: Ben>Ada, Ben>Cleo | Cleo: Ben>Ada, Cleo>Ben"
        );
        assert_eq!(
            role_line(&plan, ADA, &name),
            "You: leaf, uploading once to Ben"
        );
        assert_eq!(
            role_line(&plan, CLEO, &name),
            "You: backbone, forwarding for others"
        );
        assert_eq!(role_line(&plan, 99, &name), "You: not planned yet");
        let mesh = Plan::new(&[ADA, BEN], &[ADA, BEN]);
        assert_eq!(
            plan_line(&mesh, 4, &name),
            "Network: 2 people, full mesh (mesh cap 4)"
        );
        assert_eq!(
            routes_line(&mesh, &name),
            "Routes: Ada: Ada>Ben | Ben: Ben>Ada"
        );
        assert_eq!(
            role_line(&mesh, BEN, &name),
            "You: full mesh, sending to everyone directly"
        );
    }

    #[test]
    fn a_peer_line_says_path_role_uplink_and_renditions() {
        let row = PeerRow {
            name: String::from("Ben"),
            path: Some((true, 0.44)),
            backbone: Some(true),
            uplink_kbps: Some(50_000),
            to: vec![stream_label("camera", stream(90)), String::from("audio")],
            from: vec![String::from("Cleo camera 360p H.264")],
        };
        assert_eq!(
            peer_line(&row),
            "Ben · direct 0.4 ms · backbone · up 50 Mbps · to Ben: camera 90p H.264, audio · \
             from Ben: Cleo camera 360p H.264"
        );
        let unknown = PeerRow {
            name: String::from("Cleo"),
            ..PeerRow::default()
        };
        assert_eq!(
            peer_line(&unknown),
            "Cleo · no path yet · no report yet · to Cleo: nothing · from Cleo: nothing"
        );
        assert_eq!(kbps_label(1500), "1.5 Mbps");
        assert_eq!(kbps_label(500), "500 kbps");
    }

    #[test]
    fn the_uplink_estimate_follows_what_was_sent_and_the_congestion_window() {
        let mut up = CapacityEstimator::new(None);
        assert_eq!(up.uplink_kbps(), DEFAULT_UPLINK_KBPS);
        // 15 000 bytes per 30 ms round trip: 4000 kbit/s.
        up.sample(0, &[(1, path(0, 0, 30_000, 15_000))]);
        assert_eq!(up.uplink_kbps(), 4000);
        // Then 2.5 MB in 2 s (10 Mbit/s, the 8000 step): reported after three intervals in a row.
        let mut sent = 0;
        for (i, reported) in [4000, 4000, 8000].into_iter().enumerate() {
            sent += 2_500_000;
            up.sample(2000 * (i as u64 + 1), &[(1, path(sent, 0, 30_000, 15_000))]);
            assert_eq!(up.measured_kbps(), Some(8000));
            assert_eq!(up.uplink_kbps(), reported, "interval {i}");
        }
        // One odd interval does not move the report.
        up.sample(8000, &[(1, path(sent, 0, 30_000, 15_000))]);
        assert_eq!(up.uplink_kbps(), 8000);
    }

    #[test]
    fn a_pinned_uplink_is_reported_whatever_is_measured() {
        let mut up = CapacityEstimator::new(Some(1000));
        up.sample(0, &[(1, path(0, 0, 100, 1_000_000))]);
        assert_eq!(up.uplink_kbps(), 1000);
        assert!(up.is_pinned());
        assert_eq!(up.measured_kbps(), Some(100_000));
    }

    #[test]
    fn stability_counts_the_intervals_without_loss_spikes_or_rtt_jumps() {
        let mut up = CapacityEstimator::new(None);
        assert_eq!(up.stability_permille(), 1000);
        up.sample(0, &[(1, path(0, 0, 10_000, 15_000))]);
        // Five intervals: one with a loss spike, one with the RTT far above its lowest.
        let lost = [0, 5, 5, 5, 5, 5];
        let rtt = [10_000, 10_000, 10_000, 45_000, 10_000, 10_000];
        for i in 1..6 {
            up.sample(2000 * i as u64, &[(1, path(0, lost[i], rtt[i], 15_000))]);
        }
        assert_eq!(up.stability_permille(), 600);
    }

    #[test]
    fn a_participant_with_no_direct_path_is_relay_only() {
        let mut up = CapacityEstimator::new(None);
        up.sample(0, &[]);
        assert!(!up.relay_only());
        let relayed = PathSample {
            direct: false,
            ..path(0, 0, 50_000, 15_000)
        };
        up.sample(2000, &[(1, relayed), (2, relayed)]);
        assert!(up.relay_only());
        up.sample(4000, &[(1, relayed), (2, path(0, 0, 50_000, 15_000))]);
        assert!(!up.relay_only());
    }
}
