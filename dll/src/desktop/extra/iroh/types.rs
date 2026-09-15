//! Plain data of the `azul.iroh` API, shared by the native engine and the wasm stub.

use azul_css::{impl_option, impl_option_inner, AzString, U8Vec};

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
    /// Application protocol name. Only peers that bind the same name can connect.
    pub alpn: AzString,
    /// Relay fallback for peers without a direct path.
    pub relay_mode: IrohRelayMode,
    /// Relay server URL, used when `relay_mode` is `Custom`.
    pub relay_url: AzString,
    /// 32-byte secret key that fixes the endpoint id. Empty generates a new identity.
    pub secret_key: U8Vec,
    /// Local UDP port, 0 for any free port.
    pub port: u16,
    /// Largest inbound frame or message in bytes. Larger ones are dropped.
    pub max_frame_bytes: u32,
}

impl IrohConfig {
    /// Config for the protocol `alpn`: default relays, a new identity, any free port, 16 MiB frames.
    pub fn create(alpn: AzString) -> Self {
        IrohConfig {
            alpn,
            relay_mode: IrohRelayMode::Default,
            relay_url: AzString::from_const_str(""),
            secret_key: U8Vec::from_const_slice(&[]),
            port: 0,
            max_frame_bytes: 16 * 1024 * 1024,
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

    /// Returns the config with a persisted 32-byte identity, as read from `IrohEndpoint::secret_key`.
    pub fn with_secret_key(mut self, secret_key: U8Vec) -> Self {
        self.secret_key = secret_key;
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
}
