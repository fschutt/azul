//! Plain data of the `azul.iroh` API, shared by the native engine and the wasm stub.

use azul_core::tls::TrustRoots;
use azul_css::{
    impl_option, impl_vec, impl_vec_clone, impl_vec_debug, impl_vec_partialeq, AzString,
    OptionStringVec, StringVec, U8Vec,
};

/// Where an endpoint may relay traffic when no direct UDP path to a peer exists.
#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum IrohRelayMode {
    /// Direct paths only: peers must reach each other over the local network or public addresses.
    Disabled,
    /// The public relay servers operated by n0.
    Default,
    /// The relay server named in `IrohConfig::relay_url`.
    Custom,
}

impl Default for IrohRelayMode {
    fn default() -> Self {
        IrohRelayMode::Default
    }
}

/// Settings for `IrohEndpoint::bind`.
#[repr(C)]
#[derive(Debug, Clone, PartialEq)]
pub struct IrohConfig {
    // The fields are ordered by decreasing alignment (azul-doc autofix refuses a struct whose
    // order wastes padding): the 8-byte ones, the 4-byte ones, then the 2- and 1-byte ones.
    /// The certificate authorities the relays' (and any other non-iroh) TLS is checked
    /// against: the app's set by default (`AppConfig::trust_roots`), the public roots, or the
    /// ones it pins - minus any it distrusts - so a relay with a forged certificate is
    /// refused. Iroh's own connections are authenticated by the peers' keys and need no
    /// authority.
    pub trust_roots: TrustRoots,
    /// The endpoint ids an incoming connection is accepted from. `None` (the default): every
    /// peer that knows the protocol name, and the app decides whom it trusts. `Some`: only
    /// these (an app's paired devices); any other peer is refused before any of its streams is
    /// read, and an `Error` event names it. An empty list accepts nobody (an endpoint that only
    /// dials). `IrohEndpoint::set_allowed_peers` changes it while bound and
    /// `IrohEndpoint::clear_allowed_peers` goes back to accepting every peer.
    pub allowed_peers: OptionStringVec,
    /// Application protocol name. Only peers that bind the same name can connect.
    pub alpn: AzString,
    /// Relay server URL, used when `relay_mode` is `Custom`.
    pub relay_url: AzString,
    /// What this endpoint says about itself to the computers on the local network, next to its
    /// endpoint id and its addresses: a short text the apps agree on (a project id, a display
    /// name). Empty (the default): nothing but the id and the addresses is announced. At most 245
    /// bytes of UTF-8 (what a DNS TXT string holds after its key): control characters and
    /// bidirectional overrides are removed, a longer text is cut at a character boundary and
    /// `IrohEndpoint::local_discovery_status` says so. Announced only while `local_discovery` is
    /// on and the endpoint announces itself (see [`IrohConfig::with_local_discovery`]), to the
    /// local link and nowhere else; `IrohEndpoint::set_advertised` changes it while bound.
    pub advertised: AzString,
    /// 32-byte secret key that fixes the endpoint id. Empty generates a new identity.
    pub secret_key: U8Vec,
    /// Relay fallback for peers without a direct path.
    pub relay_mode: IrohRelayMode,
    /// Largest inbound frame or message in bytes. Larger ones are dropped.
    pub max_frame_bytes: u32,
    /// Local UDP port, 0 for any free port.
    pub port: u16,
    /// Never use a direct path: no UDP socket is bound and no hole is punched, so every packet
    /// goes through the relay of `relay_mode` (a network that blocks UDP, or a test that proves
    /// the relay carries the traffic). Needs a relay; `port` does not apply.
    pub relay_only: bool,
    /// Find peers on the local network by their endpoint id (mDNS), without a ticket's saved
    /// addresses or a relay, and announce this endpoint there. On by default; see
    /// [`IrohConfig::with_local_discovery`] for what is announced and how to turn it off.
    pub local_discovery: bool,
}

impl IrohConfig {
    /// Config for the protocol `alpn`: default relays, a new identity, any free port, 16 MiB frames,
    /// local-network discovery on.
    pub fn create(alpn: AzString) -> Self {
        IrohConfig {
            alpn,
            relay_mode: IrohRelayMode::Default,
            relay_only: false,
            relay_url: AzString::from_const_str(""),
            advertised: AzString::from_const_str(""),
            secret_key: U8Vec::from_const_slice(&[]),
            trust_roots: TrustRoots::app_default(),
            allowed_peers: OptionStringVec::None,
            port: 0,
            max_frame_bytes: 16 * 1024 * 1024,
            local_discovery: true,
        }
    }

    /// Returns the config with `relay_mode` replaced.
    pub fn with_relay_mode(mut self, relay_mode: IrohRelayMode) -> Self {
        self.relay_mode = relay_mode;
        self
    }

    /// Returns the config relaying through the server at `url`.
    pub fn with_relay_url(mut self, url: AzString) -> Self {
        self.relay_mode = IrohRelayMode::Custom;
        self.relay_url = url;
        self
    }

    /// Returns the config that sends every packet through the relay (`relay_only`): no direct path, no hole punching.
    pub fn with_relay_only(mut self, relay_only: bool) -> Self {
        self.relay_only = relay_only;
        self
    }

    /// Returns the config with a persisted 32-byte identity, as read from `IrohEndpoint::secret_key`.
    pub fn with_secret_key(mut self, secret_key: U8Vec) -> Self {
        self.secret_key = secret_key;
        self
    }

    /// Returns the config whose relays' TLS trusts `trust_roots` (a pinned set: a relay under
    /// any other authority is refused).
    pub fn with_trust_roots(mut self, trust_roots: TrustRoots) -> Self {
        self.trust_roots = trust_roots;
        self
    }

    /// Returns the config that accepts incoming connections from these endpoint ids only (an
    /// app's paired devices); an empty list accepts nobody.
    pub fn with_allowed_peers(mut self, peers: StringVec) -> Self {
        self.allowed_peers = OptionStringVec::Some(peers);
        self
    }

    /// Returns the config bound to the local UDP `port`.
    pub fn with_port(mut self, port: u16) -> Self {
        self.port = port;
        self
    }

    /// Returns the config accepting inbound frames and messages up to `max_frame_bytes`.
    pub fn with_max_frame_bytes(mut self, max_frame_bytes: u32) -> Self {
        self.max_frame_bytes = max_frame_bytes;
        self
    }

    /// Returns the config that tells the computers on the local network `text` about this
    /// endpoint (see [`IrohConfig::advertised`]): an app that finds its other instances there
    /// puts a short project id and a display name in it. Nothing is announced by default.
    pub fn with_advertised(mut self, text: AzString) -> Self {
        self.advertised = text;
        self
    }

    /// Returns the config with local-network discovery switched on or off. On (the default), the
    /// endpoint finds peers on the local network by their endpoint id alone (mDNS), so a phone
    /// finds a Mac whose address changed without the ticket's saved addresses or a relay, and it
    /// announces itself there: its endpoint id (a public key) and its network addresses, to the
    /// local link only (multicast, never routed; its relay is not announced). Turn it off for a
    /// server or an app that must not announce itself to the network it sits on. An endpoint
    /// that accepts nobody (`with_allowed_peers` with an empty list) only looks and does not
    /// announce, and `relay_only` never uses it. When the network or the OS refuses multicast
    /// the endpoint works without it; `IrohEndpoint::local_discovery_status` says why.
    pub fn with_local_discovery(mut self, local_discovery: bool) -> Self {
        self.local_discovery = local_discovery;
        self
    }
}

/// What an `IrohEvent` reports.
#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum IrohEventKind {
    /// The endpoint knows its addresses. `text` is its ticket.
    Ready,
    /// A connection was dialed or accepted. `text` is the remote endpoint id.
    PeerConnected,
    /// A connection closed. `text` is the reason.
    PeerDisconnected,
    /// The newest frame of a track. Frames that arrived while it was pending were skipped.
    Frame,
    /// A reliable message, delivered in the order the peer sent it.
    Message,
    /// A bind, dial or transport failure. `text` describes it.
    Error,
    /// An endpoint on the local network announced itself, or what it announced changed. `text`
    /// is its endpoint id, `data` the text it advertises (UTF-8, may be empty);
    /// `IrohEndpoint::nearby_peers` has its addresses. It is never dialed by itself.
    NearbyAppeared,
    /// An endpoint that was reported as `NearbyAppeared` is no longer announcing itself. `text`
    /// is its endpoint id.
    NearbyGone,
}

impl Default for IrohEventKind {
    fn default() -> Self {
        IrohEventKind::Error
    }
}

/// One event from `IrohEndpoint::recv`. Fields that do not apply to `kind` are zero or empty.
#[repr(C)]
#[derive(Debug, Clone, PartialEq)]
pub struct IrohEvent {
    /// Connection handle, 0 when the event concerns no peer.
    pub peer: u64,
    /// Send counter of a `Frame` or `Message`. Gaps between frames are skipped frames.
    pub sequence: u64,
    /// Ticket, remote endpoint id or error text, depending on `kind`.
    pub text: AzString,
    /// Payload of a `Frame` or `Message`.
    pub data: U8Vec,
    pub kind: IrohEventKind,
    /// Track of a `Frame`.
    pub track: u32,
}

impl IrohEvent {
    pub(crate) fn new(kind: IrohEventKind, peer: u64, text: String) -> Self {
        IrohEvent {
            peer,
            sequence: 0,
            text: AzString::from_string(text),
            data: U8Vec::from_const_slice(&[]),
            kind,
            track: 0,
        }
    }

    pub(crate) fn payload(
        kind: IrohEventKind,
        peer: u64,
        track: u32,
        sequence: u64,
        data: Vec<u8>,
    ) -> Self {
        IrohEvent {
            peer,
            sequence,
            text: AzString::from_const_str(""),
            data: U8Vec::from_vec(data),
            kind,
            track,
        }
    }
}

impl_option!(
    IrohEvent,
    OptionIrohEvent,
    copy = false,
    [Debug, Clone, PartialEq]
);

/// One endpoint on the local network, as `IrohEndpoint::nearby_peers` lists it: what it
/// announced, cut down (see [`IrohConfig::advertised`]) and not dialed.
#[repr(C)]
#[derive(Debug, Clone, PartialEq)]
pub struct IrohNearbyPeer {
    /// Its endpoint id: what `IrohEndpoint::connect` takes to dial it.
    pub endpoint_id: AzString,
    /// What it advertises about itself (its `IrohConfig::advertised`), cleaned; empty when it
    /// advertises nothing.
    pub advertised: AzString,
    /// The `ip:port` addresses it announced that a dial can use, at most 16.
    pub addresses: StringVec,
}

impl_option!(
    IrohNearbyPeer,
    OptionIrohNearbyPeer,
    copy = false,
    [Debug, Clone, PartialEq]
);
impl_vec!(
    IrohNearbyPeer,
    IrohNearbyPeerVec,
    IrohNearbyPeerVecDestructor,
    IrohNearbyPeerVecDestructorType,
    IrohNearbyPeerVecSlice,
    OptionIrohNearbyPeer
);
impl_vec_debug!(IrohNearbyPeer, IrohNearbyPeerVec);
impl_vec_clone!(
    IrohNearbyPeer,
    IrohNearbyPeerVec,
    IrohNearbyPeerVecDestructor
);
impl_vec_partialeq!(IrohNearbyPeer, IrohNearbyPeerVec);

/// Path and throughput statistics of one connection.
#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq, Default)]
pub struct IrohPeerStats {
    /// Whether the connection is still open.
    pub connected: bool,
    /// Whether traffic flows over a direct UDP path rather than a relay.
    pub direct: bool,
    /// Smoothed round-trip time of the selected path, in microseconds.
    pub rtt_us: u64,
    /// Congestion window of the selected path, in bytes.
    pub cwnd_bytes: u64,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub lost_packets: u64,
    pub frames_sent: u64,
    pub frames_received: u64,
    /// Outbound frames replaced by a newer frame of the same track before they left.
    pub frames_skipped: u64,
    /// Messages `send_message` accepted for this peer that have not been handed to the
    /// connection yet: the outgoing backlog. It grows while the link is slower than the sender.
    pub messages_queued: u64,
    /// Payload bytes of `messages_queued`.
    pub message_bytes_queued: u64,
}

#[cfg(test)]
#[path = "types_test.rs"]
mod tests;
