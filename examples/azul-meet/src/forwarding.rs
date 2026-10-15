//! Rooms of three and more: the capacity reports, the routing plan every participant computes alike
//! from them, and the forwarding of media for the peers below this one (the rules are `routes.rs`).

use super::*;

/// The name of the participant with key `key`: this side's own, a peer's, or a short key.
pub(crate) fn name_of(s: &MeetState, key: u64) -> String {
    if key == s.me {
        return s.name.clone();
    }
    match s.remotes.iter().find(|r| r.key == key) {
        Some(r) => remote_name(s, &r.node_id),
        None => format!("{key:016x}")[..8].to_string(),
    }
}

/// The name of the peer behind connection `handle`.
pub(crate) fn name_of_handle(s: &MeetState, handle: u64) -> String {
    match s.remotes.iter().find(|r| r.handle == handle) {
        Some(r) => remote_name(s, &r.node_id),
        None => format!("connection {handle}"),
    }
}

/// The connection to the participant with key `key`, when connected.
pub(crate) fn handle_of(s: &MeetState, key: u64) -> Option<u64> {
    s.remotes.iter().find(|r| r.key == key).map(|r| r.handle)
}

/// The connections to the participants `keys` that are connected.
pub(crate) fn handles_of(s: &MeetState, keys: &[u64]) -> Vec<u64> {
    keys.iter().filter_map(|key| handle_of(s, *key)).collect()
}

/// "camera 360p", "screen 90p"; the track's name alone for no rendition.
pub(crate) fn rendition_label(track: u32, height: u16) -> String {
    let source = track_slot(track).map_or("video", |slot| SOURCES[slot]);
    if height == 0 {
        source.to_string()
    } else {
        format!("{source} {height}p")
    }
}

/// The last report of the participant with key `key`: what this side told the others, or what a
/// peer told this side.
pub(crate) fn sync_of(s: &MeetState, key: u64) -> Option<&routes::Sync> {
    if key == s.me {
        return Some(&s.sync);
    }
    s.remotes.iter().find(|r| r.key == key)?.sync.as_ref()
}

/// The height `viewer` shows `origin`'s `track` at, from its report; 0 when it does not show it.
pub(crate) fn want_of(s: &MeetState, viewer: u64, origin: u64, track: u32) -> u16 {
    sync_of(s, viewer).map_or(0, |sync| sync.want(origin, track))
}

/// Whether the participant with key `key` decodes H.264: this side's own probe, a peer's caps
/// message; `None` while a peer's caps have not arrived.
pub(crate) fn decodes_h264_of(s: &MeetState, key: u64) -> Option<bool> {
    if key == s.me {
        return Some(s.video.decodes_h264);
    }
    s.remotes.iter().find(|r| r.key == key).and_then(|r| r.h264)
}

pub(crate) fn encodes_h264_of(s: &MeetState, key: u64) -> bool {
    if key == s.me {
        return s.video.encoder.is_ok();
    }
    s.remotes
        .iter()
        .any(|r| r.key == key && r.encodes == Some(true))
}

/// Which stream each of `peers` gets of `origin`'s `track` (`routes::assign`), in the codec
/// `video_wire::wire_codec` picks.
pub(crate) fn assignment_among(
    s: &MeetState,
    peers: &[u64],
    origin: u64,
    track: u32,
) -> BTreeMap<u64, routes::Stream> {
    let sender_h264 = encodes_h264_of(s, origin);
    let viewers: Vec<routes::Viewer> = peers
        .iter()
        .copied()
        .filter(|viewer| *viewer != origin)
        .filter_map(|viewer| {
            // A viewer whose caps have not arrived gets nothing yet - not JPEG.
            let codec = video_wire::wire_codec(sender_h264, decodes_h264_of(s, viewer))?;
            Some(routes::Viewer {
                key: viewer,
                need: want_of(s, viewer, origin, track),
                h264: codec == Codec::H264,
            })
        })
        .collect();
    routes::assign(&viewers, sender_h264)
}

/// Which stream each participant of the plan gets of `origin`'s `track`.
pub(crate) fn assignment(s: &MeetState, origin: u64, track: u32) -> BTreeMap<u64, routes::Stream> {
    assignment_among(s, s.plan.peers(), origin, track)
}

/// Whether this side gets (and shows) the `height` rendition of `origin`'s `track`.
pub(crate) fn shows(s: &MeetState, origin: u64, track: u32, height: u16) -> bool {
    assignment(s, origin, track)
        .get(&s.me)
        .is_some_and(|got| got.height == height)
}

/// The heights of this side's `track` someone shows, smallest first; none while the track is off.
pub(crate) fn my_renditions(s: &MeetState, track: u32) -> Vec<u16> {
    let on = match track {
        CAMERA_TRACK => s.cam_on,
        SCREEN_TRACK => s.screen_on,
        _ => false,
    };
    if !on || s.remotes.is_empty() {
        return Vec::new();
    }
    routes::heights(&assignment(s, s.me, track))
}

/// This side's (track, height) streams someone shows.
pub(crate) fn my_streams(s: &MeetState) -> BTreeSet<(u32, u16)> {
    let mut out = BTreeSet::new();
    for track in [CAMERA_TRACK, SCREEN_TRACK] {
        for height in my_renditions(s, track) {
            out.insert((track, height));
        }
    }
    out
}

/// Closes the encoders of the renditions nobody shows any more (a paused encoder costs nothing;
/// the next start opens a fresh one, which begins with a keyframe), and the decoders of the
/// renditions this side no longer shows: a tile that moved to another rung left its old
/// decoder open for as long as the call lasted (a VideoToolbox session, its output buffers at
/// the tile's size and its XPC connection each), and the sender may have stopped that stream
/// altogether, so no packet ever came to close it. A stream shown again gets a fresh decoder,
/// which starts at a keyframe (`take_video`).
pub(crate) fn stop_culled(s: &mut MeetState) {
    let shown = my_streams(s);
    for (key, out) in s.video_out.iter_mut() {
        if !shown.contains(key) {
            out.stop();
        }
    }
    let stale: Vec<(usize, (u32, u16))> = s
        .remotes
        .iter()
        .enumerate()
        .flat_map(|(i, r)| {
            r.received
                .iter()
                .filter(|(_, input)| input.decoder.is_some())
                .filter(|((track, height), _)| !shows(s, r.key, *track, *height))
                .map(move |(key, _)| (i, *key))
        })
        .collect();
    for (i, key) in stale {
        if let Some(input) = s.remotes[i].received.get_mut(&key) {
            input.decoder = None;
            input.shown = false;
        }
    }
}

/// The participant pinned to the stage: the one `AZMEET_STAGE` names, while connected.
pub(crate) fn pinned_key(s: &MeetState) -> Option<u64> {
    if s.stage_name.is_empty() {
        return None;
    }
    s.remotes
        .iter()
        .find(|r| remote_name(s, &r.node_id).eq_ignore_ascii_case(&s.stage_name))
        .map(|r| r.key)
}

/// What the call view shows (`tiles::arrange`): a shared screen on the stage, else in speaker
/// view the pinned participant, the active speaker or the first one; the other tiles in the
/// filmstrip (with a stage) or the gallery.
pub(crate) fn arrangement(s: &MeetState) -> tiles::Arrangement {
    let others: Vec<tiles::Participant> = s
        .remotes
        .iter()
        .map(|r| tiles::Participant {
            key: r.key,
            sharing: r.sync.as_ref().is_some_and(|sync| sync.sends_screen),
        })
        .collect();
    let view = match s.view {
        ViewMode::Grid => tiles::View::Gallery,
        ViewMode::Speaker => tiles::View::Speaker,
    };
    tiles::arrange(
        s.me,
        s.screen_on,
        &others,
        view,
        pinned_key(s),
        s.speaker.current(),
    )
}

/// A tile's role in the arrangement and the height of its box until it is laid out; `None` for
/// a picture the view does not show (a screen nobody shares).
pub(crate) fn tile_role(arr: &tiles::Arrangement, tile: tiles::Tile) -> Option<(IrohTileRole, f32)> {
    if arr.stage == Some(tile) {
        return Some((IrohTileRole::Stage, STAGE_H));
    }
    if !arr.tiles.contains(&tile) {
        return None;
    }
    Some(if arr.stage.is_some() {
        (IrohTileRole::Filmstrip, FILMSTRIP_H)
    } else {
        (IrohTileRole::Gallery, TILE_H)
    })
}

/// What this side shows: for every peer's camera and screen, the rendition its tile needs, from
/// the tile's role and its laid-out height (the box's height until it is laid out); nothing for a
/// tile that is hidden, drawn tiny, or not shown at all (`tiles::tile_need`).
pub(crate) fn my_wants(s: &MeetState) -> Vec<routes::Want> {
    let room_size = u32::try_from(s.remotes.len() + 1).unwrap_or(u32::MAX);
    let arr = arrangement(s);
    let mut wants = Vec::new();
    for r in &s.remotes {
        for (slot, track, kind) in [
            (0usize, CAMERA_TRACK, tiles::TileKind::Camera),
            (1, SCREEN_TRACK, tiles::TileKind::Screen),
        ] {
            let tile = tiles::Tile { key: r.key, kind };
            let need = tile_role(&arr, tile).map_or(0, |(role, box_height)| {
                let height = tiles::tile_need(
                    r.tile_visible[slot],
                    r.tile_height[slot],
                    box_height,
                    s.scale,
                );
                // No higher than the quality picked in the settings.
                role.rendition_height(height, s.scale, room_size)
                    .min(QUALITY_CAPS[s.quality.min(QUALITY_CAPS.len() - 1)])
            });
            wants.push(routes::Want {
                origin: r.key,
                track,
                height: u16::try_from(need).unwrap_or(u16::MAX),
            });
        }
    }
    wants
}

/// Recomputes this side's report; true when it changed.
pub(crate) fn refresh_sync(s: &mut MeetState) -> bool {
    let sync = routes::Sync {
        uplink_kbps: s.capacity.uplink_kbps(),
        stability_permille: s.capacity.stability_permille(),
        on_battery: s.on_battery,
        relay_only: s.capacity.relay_only(),
        opted_out: s.opted_out,
        sends_audio: s.mic_on,
        sends_camera: s.cam_on,
        sends_screen: s.screen_on,
        wants: my_wants(s),
    };
    let changed = sync != s.sync;
    s.sync = sync;
    changed
}

/// Tells the peers behind `handles` this side's report.
pub(crate) fn send_sync(s: &MeetState, handles: &[u64]) {
    send_message_to(s, handles, &routes::encode_sync(&s.sync));
}

/// Tells the peers behind `handles` whether this side decodes and encodes H.264, and whether it
/// decodes Opus.
pub(crate) fn send_caps(s: &MeetState, handles: &[u64]) {
    let caps = video_wire::encode_caps(
        s.video.decodes_h264,
        s.video.encoder.is_ok(),
        s.decodes_opus,
    );
    send_message_to(s, handles, &caps);
}

/// Something this side reports may have changed (a peer came or went, a toggle, the layout, the
/// statistics): tells everyone when the report changed (or anyway, with `always`), plans again, and
/// closes the encoders nobody needs.
pub(crate) fn network_changed(s: &mut MeetState, always: bool) {
    if refresh_sync(s) || always {
        send_sync(s, &all_peers(s));
    }
    replan(s);
    stop_culled(s);
}

/// A report as `IrohLoadBalancer` takes it.
///
/// TODO(client health): the `on_battery` bit becomes the peer's client health (azul-appkit's
/// `client_health`, only the number on the wire) - ROADMAP.md, "Client health".
pub(crate) fn capacity_of(key: u64, sync: &routes::Sync) -> IrohPeerCapacity {
    let mut capacity = IrohPeerCapacity::create(key, sync.uplink_kbps);
    capacity.stability = f32::from(sync.stability_permille) / 1000.0;
    capacity.on_battery = sync.on_battery;
    capacity.relay_only = sync.relay_only;
    capacity.opted_out = sync.opted_out;
    capacity
}

/// What the room's streams add up to when every viewer gets its own copy (kbit/s): audio from
/// every open microphone to everyone else, and every video delivery at its rendition's bitrate.
/// The load balancer grows the backbone until it carries 1.5 times this.
pub(crate) fn fanout_kbps(s: &MeetState, peers: &[u64]) -> u64 {
    let viewers = u64::try_from(peers.len().saturating_sub(1)).unwrap_or(u64::MAX);
    let mut total = 0;
    for origin in peers {
        let Some(sync) = sync_of(s, *origin) else {
            continue;
        };
        if sync.sends_audio {
            total += AUDIO_KBPS * viewers;
        }
        for (track, on) in [
            (CAMERA_TRACK, sync.sends_camera),
            (SCREEN_TRACK, sync.sends_screen),
        ] {
            if on {
                total += assignment_among(s, peers, *origin, track)
                    .values()
                    .map(|stream| {
                        u64::from(IrohLoadBalancer::rendition_kbps(u32::from(stream.height)))
                    })
                    .sum::<u64>();
            }
        }
    }
    total
}

/// Plans again from every report: this side's own and each peer's last. Every side feeds the same
/// reports to the load balancer and gets the same plan. True when the plan changed.
pub(crate) fn replan(s: &mut MeetState) -> bool {
    let mut peers = vec![s.me];
    peers.extend(s.remotes.iter().filter(|r| r.sync.is_some()).map(|r| r.key));
    let mut balancer = IrohLoadBalancer::create();
    balancer.set_mesh_cap(s.mesh_cap);
    balancer.set_peer(capacity_of(s.me, &s.sync));
    for r in &s.remotes {
        if let Some(sync) = r.sync.as_ref() {
            balancer.set_peer(capacity_of(r.key, sync));
        }
    }
    let count = balancer.select_backbone(fanout_kbps(s, &peers));
    let backbone: Vec<u64> = (0..count)
        .filter_map(|index| balancer.backbone_peer(index).into_option())
        .collect();
    let plan = routes::Plan::new(&peers, &backbone);
    if plan == s.plan {
        return false;
    }
    s.plan = plan;
    let view: &MeetState = s;
    let name = |key: u64| name_of(view, key);
    eprintln!(
        "[azmeet] {}: {}",
        view.name,
        routes::plan_line(&view.plan, view.mesh_cap, &name)
    );
    eprintln!(
        "[azmeet] {}: {}",
        view.name,
        routes::routes_line(&view.plan, &name)
    );
    true
}

/// The laid-out height of every peer's video tiles and the window's scale, so each tile asks for
/// the rendition it is drawn at.
pub(crate) fn measure_tiles(s: &mut MeetState, info: &mut CallbackInfo) {
    let dpi = info.get_current_window_state().size.dpi;
    if dpi > 0 {
        s.scale = dpi as f32 / 96.0;
    }
    let scale = s.scale.max(0.5);
    for r in s.remotes.iter_mut() {
        for (slot, track) in [(0usize, CAMERA_TRACK), (1, SCREEN_TRACK)] {
            let marker = tile_marker(r.handle, track);
            let node = info
                .get_node_id_by_marker(AzString::from(marker.as_str()))
                .into_option();
            // A tile not laid out yet counts as visible (its box's height asks).
            r.tile_visible[slot] = match &node {
                Some(node) => info.is_node_visible(node.clone()),
                None => true,
            };
            let size = node.and_then(|node| info.get_node_size(node).into_option());
            r.tile_height[slot] = size.map(|size| size.height);
            r.tile_px[slot] = size.and_then(|size| {
                let w = (size.width * scale).round() as u32;
                let h = (size.height * scale).round() as u32;
                (w > 0 && h > 0).then_some((w, h))
            });
        }
    }
}

/// Every 2 seconds: the tiles' heights, the paths' statistics into the uplink estimate, the report
/// to everyone (a peer that missed one gets it again), and the plan.
pub(crate) fn network_tick(s: &mut MeetState, endpoint: &IrohEndpoint, info: &mut CallbackInfo) {
    measure_tiles(s, info);
    let now = now_ms(s);
    // The peers whose path turned direct or relayed since the last statistics.
    let mut turned: Vec<(String, bool)> = Vec::new();
    let paths: Vec<(u64, routes::PathSample)> = s
        .remotes
        .iter_mut()
        .map(|r| {
            let stats = endpoint.peer_stats(r.handle);
            if r.path.map(|(direct, _)| direct) != Some(stats.direct) {
                turned.push((r.node_id.clone(), stats.direct));
            }
            r.path = Some((stats.direct, stats.rtt_us as f64 / 1000.0));
            let sample = routes::PathSample {
                bytes_sent: stats.bytes_sent,
                lost_packets: stats.lost_packets,
                rtt_us: stats.rtt_us,
                cwnd_bytes: stats.cwnd_bytes,
                direct: stats.direct,
            };
            (r.handle, sample)
        })
        .collect();
    // For scripts: each peer's path whenever it turns (`AZMEET_PATH Ben relayed`); with
    // `--relay-only` it never says `direct`.
    for (node_id, direct) in turned {
        println!(
            "AZMEET_PATH {} {}",
            remote_name(s, &node_id),
            if direct { "direct" } else { "relayed" }
        );
    }
    s.capacity.sample(now, &paths);
    network_changed(s, true);
}

/// The routing settings of this run (see the module docs): the mesh cap, a pinned uplink, the
/// forwarding opt-out, battery, and the view.
pub(crate) fn configure_network(s: &mut MeetState) {
    s.mesh_cap = setting("AZMEET_MESH_CAP")
        .and_then(|v| v.parse().ok())
        .unwrap_or(routes::DEFAULT_MESH_CAP);
    let pinned = setting("AZMEET_UPLINK_KBPS")
        .and_then(|v| v.parse::<u32>().ok())
        .filter(|kbps| *kbps > 0);
    s.capacity = routes::CapacityEstimator::new(pinned);
    s.opted_out = setting("AZMEET_NO_FORWARD").is_some_and(|v| v == "1");
    s.on_battery = setting("AZMEET_ON_BATTERY").is_some_and(|v| v == "1");
    s.view = if setting("AZMEET_LAYOUT").is_some_and(|v| v.eq_ignore_ascii_case("speaker")) {
        ViewMode::Speaker
    } else {
        ViewMode::Grid
    };
    s.stage_name = setting("AZMEET_STAGE").unwrap_or_default();
    s.panel = match setting("AZMEET_PANEL").as_deref() {
        Some("chat") => SidePanel::Chat,
        Some("statistics") => SidePanel::Statistics,
        Some("closed") => SidePanel::Closed,
        _ => SidePanel::People,
    };
    if let Some(endpoint) = s.endpoint.as_ref() {
        s.me = routes::peer_key(endpoint.endpoint_id().as_str());
    }
    refresh_sync(s);
    eprintln!(
        "[azmeet] {}: mesh cap {}, reporting up {}{}{}",
        s.name,
        s.mesh_cap,
        routes::kbps_label(s.sync.uplink_kbps),
        if pinned.is_some() {
            " (--uplink-kbps)"
        } else {
            " until measured"
        },
        if s.opted_out {
            ", not forwarding for others"
        } else {
            ""
        }
    );
}

/// A peer's report arrived: plan again. True when the window changes: a screen shared or no
/// longer shared moves the tiles (`arrangement`), and a change in what this side must capture
/// changes the capture widgets' consumers (`my_streams`, built into the DOM). Nothing else in a
/// report (the uplink, the stability, the tiles the peer shows) changes a pixel here, so it no
/// longer rebuilds the whole window - every peer re-sends its report on each change and every 2
/// seconds, and each one that differed used to.
pub(crate) fn apply_sync(s: &mut MeetState, conn: u64, sync: routes::Sync) -> bool {
    let streams = my_streams(s);
    let Some(remote) = s.remotes.iter_mut().find(|r| r.handle == conn) else {
        return false;
    };
    if remote.sync.as_ref() == Some(&sync) {
        return false;
    }
    let shared_before = remote.sync.as_ref().is_some_and(|before| before.sends_screen);
    let sharing_moved = shared_before != sync.sends_screen;
    remote.sync = Some(sync);
    replan(s);
    stop_culled(s);
    sharing_moved || my_streams(s) != streams
}

/// Media that reached this side: as its origin sent it, or already wrapped by a forwarder.
#[derive(Clone, Copy)]
pub(crate) enum Carried<'a> {
    Direct(&'a [u8]),
    Relayed(&'a [u8]),
}

impl Carried<'_> {
    /// The bytes to pass on: a forwarder's envelope as it came, or the origin's bytes wrapped.
    pub(crate) fn envelope(self, track: u32, origin: u64) -> Vec<u8> {
        match self {
            Carried::Direct(inner) => routes::encode_relay(track, origin, origin, inner),
            Carried::Relayed(envelope) => envelope.to_vec(),
        }
    }
}

/// A frame or message from connection `event.peer`: media (the peer's own, or passed on for
/// someone), a control, a report (`ConnectionSync`), or the audio state. True when the window
/// changes.
pub(crate) fn receive_item(
    s: &mut MeetState,
    endpoint: &IrohEndpoint,
    event: &IrohEvent,
    pictures: &mut BTreeMap<(u64, u32), RawImage>,
) -> bool {
    let conn = event.peer;
    let bytes = event.data.as_slice();
    if event.kind == IrohEventKind::Message {
        // A direct call's hello, answer or roster, from a peer or a connection still waiting.
        if let Some(msg) = direct::decode(bytes) {
            return direct_call::on_message(s, conn, msg);
        }
        // A chat message over the peers.
        if let Some(record) = chatroom::ChatRecord::decode(bytes) {
            return peer_chat::on_record(s, endpoint, conn, record);
        }
    }
    let Some(sender) = s.remotes.iter().find(|r| r.handle == conn).map(|r| r.key) else {
        // A connection waiting for its signed announcement: what it says about itself is kept
        // for when it is confirmed (`settle_pending`); its media is not taken.
        let about_itself = routes::decode_sync(bytes).is_some()
            || matches!(video_wire::decode_message(bytes), Some(Message::Control(_)))
            || matches!(
                audio::decode_control(bytes),
                Some(audio::Control::State(_))
            );
        if about_itself {
            if let Some(p) = s.pending.iter_mut().find(|p| p.handle == conn) {
                if p.held.len() == PENDING_HELD {
                    p.held.remove(0);
                }
                p.held.push(bytes.to_vec());
            }
        }
        return false;
    };
    if let Some(relayed) = routes::decode_relay(bytes) {
        return receive_relayed(s, endpoint, conn, relayed, bytes, pictures);
    }
    if event.kind == IrohEventKind::Frame && event.track == AUDIO_TRACK {
        let stage_moved = receive_audio(s, sender, bytes);
        pass_on_audio(s, endpoint, sender, conn, Carried::Direct(bytes));
        return stage_moved && speaker_moved(s);
    }
    if let Some(sync) = routes::decode_sync(bytes) {
        return apply_sync(s, conn, sync);
    }
    match video_wire::decode_message(bytes) {
        Some(Message::Packet(header, payload)) => {
            let new_tile = take_video(s, endpoint, sender, conn, &header, payload, pictures);
            pass_on_video(s, endpoint, sender, conn, &header, Carried::Direct(bytes));
            new_tile
        }
        Some(Message::Control(control)) => apply_video_control(s, conn, sender, None, control),
        // A frame of no known kind, or a malformed one.
        None if event.kind == IrohEventKind::Frame => false,
        None => apply_link_event(s, event),
    }
}

/// An item a forwarder passed on through connection `conn` (`envelope` is all of it): someone's
/// media, a keyframe request on its way to the origin, or a child's acknowledgement of a stream
/// this side passes on.
pub(crate) fn receive_relayed(
    s: &mut MeetState,
    endpoint: &IrohEndpoint,
    conn: u64,
    relayed: routes::Relayed<'_>,
    envelope: &[u8],
    pictures: &mut BTreeMap<(u64, u32), RawImage>,
) -> bool {
    let origin = relayed.origin;
    if origin == s.me {
        // A viewer further down asks for a keyframe of this side's own stream.
        return match video_wire::decode_control(relayed.inner) {
            Some(control @ Control::KeyframeRequest { .. }) => {
                apply_video_control(s, conn, relayed.from, Some(conn), control)
            }
            _ => false,
        };
    }
    if relayed.track == AUDIO_TRACK {
        let stage_moved = receive_audio(s, origin, relayed.inner);
        pass_on_audio(s, endpoint, origin, conn, Carried::Relayed(envelope));
        return stage_moved && speaker_moved(s);
    }
    match video_wire::decode_message(relayed.inner) {
        Some(Message::Packet(header, payload)) => {
            let new_tile = take_video(s, endpoint, origin, conn, &header, payload, pictures);
            pass_on_video(
                s,
                endpoint,
                origin,
                conn,
                &header,
                Carried::Relayed(envelope),
            );
            new_tile
        }
        Some(Message::Control(Control::KeyframeRequest { track, height })) => {
            let request = PassedRequest {
                origin,
                requester: relayed.from,
                track,
                height,
            };
            pass_request_on(s, endpoint, conn, &request, envelope);
            false
        }
        Some(Message::Control(Control::Received { track, seq, height })) => {
            if let Some(window) = s.relay.windows.get_mut(&(conn, origin, track, height)) {
                window.acked(seq);
            }
            false
        }
        _ => false,
    }
}

/// The frame lane of `origin`'s passed-on frames (`routes::frame_track`), from 1.
pub(crate) fn relay_lane(s: &mut MeetState, origin: u64) -> u32 {
    let next = u32::try_from(s.relay.lanes.len() + 1).unwrap_or(u32::MAX);
    *s.relay.lanes.entry(origin).or_insert(next)
}

/// The connections of `keys`, without the origin and the connection the item came through.
pub(crate) fn relay_targets(s: &MeetState, origin: u64, came_from: u64, keys: &[u64]) -> Vec<u64> {
    let origin_handle = handle_of(s, origin);
    handles_of(s, keys)
        .into_iter()
        .filter(|handle| *handle != came_from && Some(*handle) != origin_handle)
        .collect()
}

/// `origin`'s audio frame passed on to this side's children in origin's tree, on origin's lane.
pub(crate) fn pass_on_audio(
    s: &mut MeetState,
    endpoint: &IrohEndpoint,
    origin: u64,
    came_from: u64,
    carried: Carried<'_>,
) {
    let children = s.plan.children(s.me, origin);
    let targets = relay_targets(s, origin, came_from, &children);
    if targets.is_empty() {
        return;
    }
    let track = routes::frame_track(relay_lane(s, origin), AUDIO_TRACK, 0);
    let envelope = carried.envelope(AUDIO_TRACK, origin);
    s.relay.frames += targets.len() as u64;
    for handle in targets {
        endpoint.send_frame(handle, track, envelope.clone());
    }
}

/// A packet of `origin`'s video passed on to the children with a viewer of its stream below them
/// (`routes::Plan::next_hops`): H.264 as a message through a window per child (a child that falls
/// behind resumes at a keyframe, which is asked for upstream), JPEG as a frame on origin's lane.
pub(crate) fn pass_on_video(
    s: &mut MeetState,
    endpoint: &IrohEndpoint,
    origin: u64,
    came_from: u64,
    header: &video_wire::Header,
    carried: Carried<'_>,
) {
    let stream = routes::Stream {
        height: header.height,
        h264: header.codec == Codec::H264,
    };
    let assigned = assignment(s, origin, header.track);
    let hops = s.plan.next_hops(s.me, origin, stream, &assigned);
    let targets = relay_targets(s, origin, came_from, &hops);
    if targets.is_empty() {
        return;
    }
    let envelope = carried.envelope(header.track, origin);
    match header.codec {
        Codec::H264 => {
            let mut resync = false;
            for handle in targets {
                let window = s
                    .relay
                    .windows
                    .entry((handle, origin, header.track, header.height))
                    .or_insert_with(video_wire::SendWindow::new);
                if window.offer(header.seq, header.keyframe) {
                    endpoint.send_message(handle, envelope.clone());
                    s.relay.packets += 1;
                }
                resync |= window.wants_keyframe();
            }
            if resync {
                ask_upstream(s, endpoint, origin, came_from, header.track, header.height);
            }
        }
        Codec::Jpeg => {
            let track = routes::frame_track(relay_lane(s, origin), header.track, header.height);
            s.relay.frames += targets.len() as u64;
            for handle in targets {
                endpoint.send_frame(handle, track, envelope.clone());
            }
        }
    }
}

/// A child waits for a keyframe of a stream this side passes on: ask the way the stream comes
/// (through `via`), at most once per `video_wire::REQUEST_RETRY_MS`.
pub(crate) fn ask_upstream(
    s: &mut MeetState,
    endpoint: &IrohEndpoint,
    origin: u64,
    via: u64,
    track: u32,
    height: u16,
) {
    let now = now_ms(s);
    let key = (origin, track, height);
    let recent = s
        .relay
        .asked
        .get(&key)
        .is_some_and(|at| now.saturating_sub(*at) < video_wire::REQUEST_RETRY_MS);
    if recent {
        return;
    }
    s.relay.asked.insert(key, now);
    let request = video_wire::encode_keyframe_request(track, height);
    send_control_up(s, endpoint, origin, via, track, &request);
}

/// A control about `origin`'s stream, sent back through `via`, the connection the stream comes
/// through: as it is when that is the origin, else wrapped, naming the origin and this side.
pub(crate) fn send_control_up(
    s: &MeetState,
    endpoint: &IrohEndpoint,
    origin: u64,
    via: u64,
    track: u32,
    control: &[u8],
) {
    if handle_of(s, origin) == Some(via) {
        endpoint.send_message(via, control.to_vec());
    } else {
        endpoint.send_message(via, routes::encode_relay(track, origin, s.me, control));
    }
}

/// A keyframe request on its way from `requester` to `origin`.
pub(crate) struct PassedRequest {
    pub(crate) origin: u64,
    pub(crate) requester: u64,
    pub(crate) track: u32,
    pub(crate) height: u16,
}

/// Passes a keyframe request on toward its origin, through the connection this side gets that
/// stream from (else its parent in the origin's tree), as it came (`envelope`).
pub(crate) fn pass_request_on(
    s: &mut MeetState,
    endpoint: &IrohEndpoint,
    came_from: u64,
    request: &PassedRequest,
    envelope: &[u8],
) {
    let through = s
        .remotes
        .iter()
        .find(|r| r.key == request.origin)
        .and_then(|r| r.received.get(&(request.track, request.height)))
        .map(|input| input.via);
    let upstream = through.or_else(|| {
        s.plan
            .parent(s.me, request.origin)
            .and_then(|parent| handle_of(s, parent))
    });
    let Some(upstream) = upstream.filter(|handle| *handle != came_from) else {
        return;
    };
    endpoint.send_message(upstream, envelope.to_vec());
    s.relay.requests += 1;
    eprintln!(
        "[azmeet] {}: passing {}'s keyframe request for {}'s {} on to {}",
        s.name,
        name_of(s, request.requester),
        name_of(s, request.origin),
        rendition_label(request.track, request.height),
        name_of_handle(s, upstream)
    );
}

/// A peer left: the windows of what this side passed on to it or for it go.
pub(crate) fn forget_relaying(s: &mut MeetState, gone: &Remote) {
    s.relay
        .windows
        .retain(|(child, origin, _, _), _| *child != gone.handle && *origin != gone.key);
    s.relay
        .asked
        .retain(|(origin, _, _), _| *origin != gone.key);
}

/// The network panel: the plan, every origin's route, this side's role and report, what it passed
/// on, and one line per peer. Empty while nobody else is here.
pub(crate) fn network_lines(s: &MeetState) -> Vec<String> {
    if s.remotes.is_empty() {
        return Vec::new();
    }
    let name = |key: u64| name_of(s, key);
    let mut lines = vec![
        routes::plan_line(&s.plan, s.mesh_cap, &name),
        routes::routes_line(&s.plan, &name),
        routes::role_line(&s.plan, s.me, &name),
        report_line(s),
    ];
    if s.relay.packets + s.relay.frames + s.relay.requests > 0 {
        lines.push(format!(
            "Forwarded: {} packets, {} frames, {} keyframe requests passed on",
            s.relay.packets, s.relay.frames, s.relay.requests
        ));
    }
    for r in &s.remotes {
        lines.push(routes::peer_line(&peer_row(s, r)));
    }
    lines
}

/// "You report: up 1 Mbps (pinned; measured 100 Mbps) · stability 100%".
pub(crate) fn report_line(s: &MeetState) -> String {
    let mut line = format!("You report: up {}", routes::kbps_label(s.sync.uplink_kbps));
    match (s.capacity.is_pinned(), s.capacity.measured_kbps()) {
        (true, Some(measured)) => line.push_str(&format!(
            " (pinned; measured {})",
            routes::kbps_label(measured)
        )),
        (true, None) => line.push_str(" (pinned)"),
        (false, Some(_)) => line.push_str(" (measured)"),
        (false, None) => line.push_str(" (not measured yet)"),
    }
    line.push_str(&format!(" · stability {}%", s.sync.stability_permille / 10));
    if s.sync.relay_only {
        line.push_str(" · relay only");
    }
    if s.sync.opted_out {
        line.push_str(" · not forwarding for others");
    }
    if s.sync.on_battery {
        line.push_str(" · on battery");
    }
    line
}

/// The network panel's line about peer `r`: its path, its part in the plan, what it reports, what
/// this side sends it (its own streams, then what it passes on for others), and what this side
/// gets through it.
pub(crate) fn peer_row(s: &MeetState, r: &Remote) -> routes::PeerRow {
    let me = s.me;
    let mut own = Vec::new();
    let mut passed = Vec::new();
    let mut from = Vec::new();
    for origin in s.plan.peers().iter().copied() {
        let Some(sync) = sync_of(s, origin) else {
            continue;
        };
        let origin_name = name_of(s, origin);
        let through_r = origin != me && s.plan.parent(me, origin) == Some(r.key);
        let mut sends = |item: String| {
            if origin == me {
                own.push(item);
            } else {
                passed.push(format!("{origin_name} {item}"));
            }
        };
        if sync.sends_audio {
            if s.plan.children(me, origin).contains(&r.key) {
                sends(String::from("audio"));
            }
            if through_r {
                from.push(format!("{origin_name} audio"));
            }
        }
        for (slot, track, on) in [
            (0usize, CAMERA_TRACK, sync.sends_camera),
            (1, SCREEN_TRACK, sync.sends_screen),
        ] {
            if !on {
                continue;
            }
            let assigned = assignment(s, origin, track);
            for stream in routes::streams(&assigned) {
                if s.plan
                    .next_hops(me, origin, stream, &assigned)
                    .contains(&r.key)
                {
                    sends(routes::stream_label(SOURCES[slot], stream));
                }
            }
            if through_r {
                if let Some(stream) = assigned.get(&me) {
                    from.push(format!(
                        "{origin_name} {}",
                        routes::stream_label(SOURCES[slot], *stream)
                    ));
                }
            }
        }
    }
    passed.sort();
    from.sort();
    own.extend(passed);
    routes::PeerRow {
        name: remote_name(s, &r.node_id),
        path: r.path,
        backbone: r.sync.as_ref().map(|_| s.plan.is_backbone(r.key)),
        uplink_kbps: r.sync.as_ref().map(|sync| sync.uplink_kbps),
        to: own,
        from,
    }
}
