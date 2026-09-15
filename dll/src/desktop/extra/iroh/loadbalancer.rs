//! Forwarder selection and tile culling for rooms that route media over peers instead of a server.
//!
//! Pure functions of what the peers report, so every peer that runs them on the same input
//! reaches the same plan.

use core::ffi::c_void;

use azul_css::OptionU64;

const SHARED_FORWARDING_ROOM: u32 = 8;
const UPLINK_HEADROOM: f64 = 0.85;
const FANOUT_COVER: f64 = 1.5;
const BATTERY_PENALTY: f32 = 0.3;
const LADDER: [(u32, u32); 4] = [(90, 120), (180, 250), (360, 600), (720, 1500)];

/// What a peer can contribute to forwarding, as measured or reported by that peer.
#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct IrohPeerCapacity {
    /// Connection handle or roster index identifying the peer.
    pub peer: u64,
    /// Estimated upload capacity in kbit/s.
    pub uplink_kbps: u32,
    /// Share of recent intervals without loss spikes or RTT jumps, 0.0 to 1.0.
    pub stability: f32,
    pub on_battery: bool,
    /// Reachable only through a relay, as browsers are.
    pub relay_only: bool,
    /// The user declined to forward other people's media.
    pub opted_out: bool,
}

impl IrohPeerCapacity {
    /// A stable, mains-powered, directly reachable peer with `uplink_kbps` of upload.
    pub fn create(peer: u64, uplink_kbps: u32) -> Self {
        IrohPeerCapacity {
            peer,
            uplink_kbps,
            stability: 1.0,
            on_battery: false,
            relay_only: false,
            opted_out: false,
        }
    }

    /// Forwarding score: uplink times stability, cut to 30 percent on battery, 0 when relay-only or opted out.
    pub fn score(&self) -> f32 {
        if self.relay_only || self.opted_out {
            return 0.0;
        }
        let battery = if self.on_battery {
            BATTERY_PENALTY
        } else {
            1.0
        };
        self.uplink_kbps as f32 * self.stability.clamp(0.0, 1.0) * battery
    }
}

/// How a video tile is shown, which caps the resolution it requests.
#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum IrohTileRole {
    /// The presentation or main speaker.
    Stage,
    /// Active speakers next to the stage.
    Side,
    /// Recent speakers in the strip.
    Filmstrip,
    /// A tile of the paged gallery.
    Gallery,
    /// A tile the viewer pinned.
    Pinned,
}

impl IrohTileRole {
    /// Largest rendition height this role may request in a room of `room_size` people.
    pub fn max_height(&self, room_size: u32) -> u32 {
        use IrohTileRole::*;
        match (room_size, self) {
            (0..=8, Filmstrip) => 360,
            (0..=8, _) => 720,
            (9..=25, Stage | Pinned) => 720,
            (9..=25, Side | Gallery) => 360,
            (9..=25, Filmstrip) => 180,
            (26..=64, Stage | Pinned) => 720,
            (26..=64, Side) => 360,
            (26..=64, Filmstrip | Gallery) => 180,
            (_, Stage) => 720,
            (_, Pinned) => 360,
            (_, Side) => 180,
            (_, Filmstrip | Gallery) => 90,
        }
    }

    /// Rendition height (90, 180, 360 or 720) to request for a tile `tile_height` logical pixels tall; 0 for a hidden tile.
    pub fn rendition_height(&self, tile_height: f32, scale_factor: f32, room_size: u32) -> u32 {
        let needed = tile_height * scale_factor.max(1.0);
        if needed.is_nan() || needed <= 0.0 {
            return 0;
        }
        let capped = needed.min(self.max_height(room_size) as f32);
        LADDER
            .iter()
            .map(|(height, _)| *height)
            .find(|height| capped <= *height as f32)
            .unwrap_or(LADDER[LADDER.len() - 1].0)
    }
}

/// Chooses which peers of a room forward media for everyone else.
#[repr(C)]
#[derive(Debug)]
pub struct IrohLoadBalancer {
    pub ptr: *mut c_void,
    pub run_destructor: bool,
}

#[derive(Debug, Clone, Default)]
struct Room {
    peers: Vec<IrohPeerCapacity>,
    backbone: Vec<u64>,
}

impl Clone for IrohLoadBalancer {
    fn clone(&self) -> Self {
        Self::from_room(self.room().cloned().unwrap_or_default())
    }
}

impl Default for IrohLoadBalancer {
    fn default() -> Self {
        Self::create()
    }
}

impl Drop for IrohLoadBalancer {
    fn drop(&mut self) {
        if self.run_destructor && !self.ptr.is_null() {
            drop(unsafe { Box::from_raw(self.ptr.cast::<Room>()) });
        }
        self.ptr = core::ptr::null_mut();
        self.run_destructor = false;
    }
}

impl IrohLoadBalancer {
    fn from_room(room: Room) -> Self {
        IrohLoadBalancer {
            ptr: Box::into_raw(Box::new(room)).cast::<c_void>(),
            run_destructor: true,
        }
    }

    fn room(&self) -> Option<&Room> {
        unsafe { self.ptr.cast::<Room>().as_ref() }
    }

    fn room_mut(&mut self) -> Option<&mut Room> {
        unsafe { self.ptr.cast::<Room>().as_mut() }
    }

    /// An empty room.
    pub fn create() -> Self {
        Self::from_room(Room::default())
    }

    /// Adds `capacity`, replacing the earlier record of the same peer.
    pub fn set_peer(&mut self, capacity: IrohPeerCapacity) {
        let Some(room) = self.room_mut() else { return };
        match room.peers.iter_mut().find(|p| p.peer == capacity.peer) {
            Some(existing) => *existing = capacity,
            None => room.peers.push(capacity),
        }
    }

    /// Removes a peer and drops it from the backbone. Returns whether it was present.
    pub fn remove_peer(&mut self, peer: u64) -> bool {
        let Some(room) = self.room_mut() else {
            return false;
        };
        room.backbone.retain(|p| *p != peer);
        let before = room.peers.len();
        room.peers.retain(|p| p.peer != peer);
        room.peers.len() != before
    }

    /// Number of peers in the room.
    pub fn peer_count(&self) -> usize {
        self.room().map_or(0, |room| room.peers.len())
    }

    /// Chooses forwarders able to carry `fanout_kbps` of culled demand. Returns how many were chosen.
    pub fn select_backbone(&mut self, fanout_kbps: u64) -> usize {
        let Some(room) = self.room_mut() else {
            return 0;
        };
        room.backbone = backbone(&room.peers, fanout_kbps);
        room.backbone.len()
    }

    /// The forwarder at `index`, best score first, after `select_backbone`.
    pub fn backbone_peer(&self, index: usize) -> OptionU64 {
        self.room()
            .and_then(|room| room.backbone.get(index).copied())
            .into()
    }

    /// Whether `peer` was chosen as a forwarder by the last `select_backbone`.
    pub fn is_backbone(&self, peer: u64) -> bool {
        self.room()
            .is_some_and(|room| room.backbone.contains(&peer))
    }

    /// Minimum forwarder count for a room of `room_size`: everyone up to 8 people, then max(ceil(sqrt N), ceil(N / 8)).
    pub fn backbone_size(room_size: u32) -> u32 {
        if room_size <= SHARED_FORWARDING_ROOM {
            return room_size;
        }
        let sqrt = (room_size as f64).sqrt().ceil() as u32;
        sqrt.max(room_size.div_ceil(8))
    }

    /// Bitrate in kbit/s the planner assumes for a rendition of `height` pixels.
    pub fn rendition_kbps(height: u32) -> u32 {
        LADDER
            .iter()
            .find(|(h, _)| height <= *h)
            .map_or(LADDER[LADDER.len() - 1].1, |(_, kbps)| *kbps)
    }
}

fn backbone(peers: &[IrohPeerCapacity], fanout_kbps: u64) -> Vec<u64> {
    let mut ranked: Vec<&IrohPeerCapacity> = peers.iter().filter(|p| p.score() > 0.0).collect();
    ranked.sort_by(|a, b| {
        b.score()
            .total_cmp(&a.score())
            .then_with(|| a.peer.cmp(&b.peer))
    });
    let room_size = u32::try_from(peers.len()).unwrap_or(u32::MAX);
    let mut chosen = (IrohLoadBalancer::backbone_size(room_size) as usize).min(ranked.len());
    let usable = |count: usize| {
        ranked[..count]
            .iter()
            .map(|p| p.uplink_kbps as f64 * UPLINK_HEADROOM)
            .sum::<f64>()
    };
    while chosen < ranked.len() && usable(chosen) < FANOUT_COVER * fanout_kbps as f64 {
        chosen += 1;
    }
    ranked[..chosen].iter().map(|p| p.peer).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn room(uplinks: &[u32]) -> IrohLoadBalancer {
        let mut lb = IrohLoadBalancer::create();
        for (i, up) in uplinks.iter().enumerate() {
            lb.set_peer(IrohPeerCapacity::create(i as u64 + 1, *up));
        }
        lb
    }

    #[test]
    fn small_rooms_share_forwarding() {
        assert_eq!(IrohLoadBalancer::backbone_size(1), 1);
        assert_eq!(IrohLoadBalancer::backbone_size(8), 8);
        assert_eq!(IrohLoadBalancer::backbone_size(9), 3);
        assert_eq!(IrohLoadBalancer::backbone_size(64), 8);
        assert_eq!(IrohLoadBalancer::backbone_size(256), 32);
    }

    #[test]
    fn backbone_prefers_strong_uplinks_and_grows_with_demand() {
        let uplinks: Vec<u32> = (1..=16).map(|i| i * 1000).collect();
        let mut lb = room(&uplinks);
        assert_eq!(lb.select_backbone(0), 4);
        assert_eq!(lb.backbone_peer(0), OptionU64::Some(16));
        assert!(lb.is_backbone(13) && !lb.is_backbone(12));
        let grown = lb.select_backbone(60_000);
        assert!(grown > 4);
        let usable: f64 = (0..grown)
            .map(|i| match lb.backbone_peer(i) {
                OptionU64::Some(p) => p as f64 * 1000.0 * UPLINK_HEADROOM,
                OptionU64::None => 0.0,
            })
            .sum();
        assert!(usable >= FANOUT_COVER * 60_000.0);
    }

    #[test]
    fn relay_only_battery_and_opted_out_peers_rank_last_or_never() {
        let mut lb = room(&[5000, 5000, 5000]);
        let mut phone = IrohPeerCapacity::create(2, 9000);
        phone.on_battery = true;
        lb.set_peer(phone);
        let mut browser = IrohPeerCapacity::create(3, 50_000);
        browser.relay_only = true;
        lb.set_peer(browser);
        assert_eq!(lb.select_backbone(0), 2);
        assert_eq!(lb.backbone_peer(0), OptionU64::Some(1));
        assert_eq!(lb.backbone_peer(1), OptionU64::Some(2));
        assert!(!lb.is_backbone(3));
    }

    #[test]
    fn tiles_request_the_smallest_sufficient_rendition() {
        assert_eq!(IrohTileRole::Gallery.rendition_height(100.0, 1.0, 4), 180);
        assert_eq!(IrohTileRole::Gallery.rendition_height(400.0, 2.0, 4), 720);
        assert_eq!(IrohTileRole::Gallery.rendition_height(400.0, 2.0, 100), 90);
        assert_eq!(IrohTileRole::Pinned.rendition_height(1080.0, 1.0, 100), 360);
        assert_eq!(IrohTileRole::Stage.rendition_height(0.0, 1.0, 4), 0);
        assert_eq!(IrohLoadBalancer::rendition_kbps(180), 250);
    }
}
