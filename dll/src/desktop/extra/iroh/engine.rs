//! The iroh engine behind `IrohEndpoint`: one tokio runtime for the process, tasks per connection.
//!
//! Wire format, one unidirectional QUIC stream per frame and one per direction for messages:
//! frame stream `[1][track u32][sequence u64][payload until FIN]`,
//! message stream `[2]` then repeated `[sequence u64][length u32][payload]`, all little endian.

use std::{
    collections::{BTreeMap, VecDeque},
    net::{Ipv4Addr, SocketAddr},
    ops::Bound,
    str::FromStr,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex, MutexGuard, OnceLock, PoisonError,
    },
};

use iroh::{
    endpoint::{presets, Connection, ReadToEndError, RecvStream, SendStream, VarInt},
    Endpoint, EndpointAddr, EndpointId, RelayMode, RelayUrl, SecretKey, Watcher,
};
use iroh_tickets::endpoint::EndpointTicket;
use tokio::{
    runtime::Runtime,
    sync::{mpsc, Notify},
};

use super::types::{IrohConfig, IrohEvent, IrohEventKind, IrohPeerStats, IrohRelayMode};

const FRAME_STREAM: u8 = 1;
const MESSAGE_STREAM: u8 = 2;
const MESSAGE_PRIORITY: i32 = 1;
const CLOSED_BY_APP: u32 = 0;
const PROTOCOL_VIOLATION: u32 = 1;

fn runtime() -> Result<&'static Runtime, String> {
    static RUNTIME: OnceLock<Result<Runtime, String>> = OnceLock::new();
    RUNTIME
        .get_or_init(|| {
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .thread_name("azul-iroh")
                .enable_all()
                .build()
                .map_err(|e| format!("could not start the iroh runtime: {e}"))
        })
        .as_ref()
        .map_err(Clone::clone)
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

pub(super) struct Engine {
    endpoint: Endpoint,
    alpn: Vec<u8>,
    state: Arc<State>,
}

#[derive(Default)]
struct State {
    max_frame: usize,
    events: Mutex<VecDeque<IrohEvent>>,
    frames: Mutex<BTreeMap<(u64, u32), IrohEvent>>,
    frame_cursor: Mutex<(u64, u32)>,
    peers: Mutex<BTreeMap<u64, Arc<Peer>>>,
    next_peer: AtomicU64,
    frame_sequences: Mutex<BTreeMap<u32, u64>>,
    message_sequence: AtomicU64,
}

struct Peer {
    id: u64,
    conn: Connection,
    tracks: Mutex<BTreeMap<u32, Arc<Slot>>>,
    messages: mpsc::UnboundedSender<(u64, Vec<u8>)>,
    newest: Mutex<BTreeMap<u32, u64>>,
    frames_sent: AtomicU64,
    frames_received: AtomicU64,
    frames_skipped: AtomicU64,
}

#[derive(Default)]
struct Slot {
    pending: Mutex<Option<Outbound>>,
    wake: Notify,
}

struct Outbound {
    sequence: u64,
    data: Arc<[u8]>,
}

impl Engine {
    pub(super) fn bind(config: &IrohConfig) -> Result<Engine, String> {
        let runtime = runtime()?;
        let alpn = config.alpn.as_str().as_bytes().to_vec();
        if alpn.is_empty() {
            return Err("IrohConfig.alpn is empty".to_string());
        }
        let relay_mode = match config.relay_mode {
            IrohRelayMode::Disabled => RelayMode::Disabled,
            IrohRelayMode::Default => RelayMode::Default,
            IrohRelayMode::Custom => {
                let url = config.relay_url.as_str();
                let url = RelayUrl::from_str(url)
                    .map_err(|e| format!("invalid relay url {url:?}: {e}"))?;
                RelayMode::custom([url])
            }
        };
        let mut builder = Endpoint::builder(presets::Minimal)
            .alpns(vec![alpn.clone()])
            .relay_mode(relay_mode);
        let key = config.secret_key.as_ref();
        if !key.is_empty() {
            let key: &[u8; 32] = key.try_into().map_err(|_| {
                format!("IrohConfig.secret_key must be 32 bytes, got {}", key.len())
            })?;
            builder = builder.secret_key(SecretKey::from_bytes(key));
        }
        if config.port != 0 {
            builder = builder
                .bind_addr(SocketAddr::from((Ipv4Addr::UNSPECIFIED, config.port)))
                .map_err(|e| format!("invalid port {}: {e}", config.port))?;
        }
        let endpoint = runtime
            .block_on(builder.bind())
            .map_err(|e| format!("could not bind the endpoint: {e}"))?;
        let state = Arc::new(State {
            max_frame: config.max_frame_bytes as usize,
            next_peer: AtomicU64::new(1),
            ..State::default()
        });
        runtime.spawn(accept_connections(endpoint.clone(), state.clone()));
        runtime.spawn(announce_ready(endpoint.clone(), state.clone()));
        Ok(Engine {
            endpoint,
            alpn,
            state,
        })
    }

    pub(super) fn is_bound(&self) -> bool {
        !self.endpoint.is_closed()
    }

    pub(super) fn endpoint_id(&self) -> String {
        self.endpoint.id().to_string()
    }

    pub(super) fn secret_key(&self) -> Vec<u8> {
        self.endpoint.secret_key().to_bytes().to_vec()
    }

    pub(super) fn ticket(&self) -> String {
        EndpointTicket::new(self.endpoint.addr()).to_string()
    }

    pub(super) fn connect(&self, ticket: &str) -> Result<(), String> {
        let ticket = ticket.trim();
        let addr = match EndpointTicket::from_str(ticket) {
            Ok(ticket) => EndpointAddr::from(ticket),
            Err(ticket_error) => EndpointId::from_str(ticket)
                .map(EndpointAddr::new)
                .map_err(|_| format!("not an endpoint ticket or id: {ticket_error}"))?,
        };
        if addr.id == self.endpoint.id() {
            return Err("the ticket belongs to this endpoint".to_string());
        }
        let runtime = runtime()?;
        let endpoint = self.endpoint.clone();
        let alpn = self.alpn.clone();
        let state = self.state.clone();
        runtime.spawn(async move {
            match endpoint.connect(addr, &alpn).await {
                Ok(conn) => adopt(&state, conn),
                Err(e) => state.push(IrohEvent::new(
                    IrohEventKind::Error,
                    0,
                    format!("could not connect: {e}"),
                )),
            }
        });
        Ok(())
    }

    pub(super) fn send_frame(&self, peer: Option<u64>, track: u32, data: &[u8]) -> bool {
        let targets: Vec<Arc<Peer>> = {
            let peers = lock(&self.state.peers);
            match peer {
                Some(id) => peers.get(&id).cloned().into_iter().collect(),
                None => peers.values().cloned().collect(),
            }
        };
        let Ok(runtime) = runtime() else {
            return false;
        };
        if targets.is_empty() {
            return false;
        }
        let sequence = {
            let mut sequences = lock(&self.state.frame_sequences);
            let next = sequences.entry(track).or_insert(0);
            *next += 1;
            *next
        };
        let data: Arc<[u8]> = Arc::from(data);
        for peer in targets {
            peer.queue(
                runtime,
                track,
                Outbound {
                    sequence,
                    data: data.clone(),
                },
            );
        }
        true
    }

    pub(super) fn send_message(&self, peer: u64, data: &[u8]) -> bool {
        if u32::try_from(data.len()).is_err() {
            return false;
        }
        let Some(peer) = lock(&self.state.peers).get(&peer).cloned() else {
            return false;
        };
        let sequence = self.state.message_sequence.fetch_add(1, Ordering::Relaxed) + 1;
        peer.messages.send((sequence, data.to_vec())).is_ok()
    }

    pub(super) fn disconnect(&self, peer: u64) -> bool {
        let Some(peer) = lock(&self.state.peers).get(&peer).cloned() else {
            return false;
        };
        peer.conn.close(
            VarInt::from_u32(CLOSED_BY_APP),
            b"disconnected by the application",
        );
        true
    }

    pub(super) fn peer_count(&self) -> usize {
        lock(&self.state.peers).len()
    }

    pub(super) fn peer_endpoint_id(&self, peer: u64) -> Option<String> {
        lock(&self.state.peers)
            .get(&peer)
            .map(|peer| peer.conn.remote_id().to_string())
    }

    pub(super) fn peer_stats(&self, peer: u64) -> IrohPeerStats {
        let Some(peer) = lock(&self.state.peers).get(&peer).cloned() else {
            return IrohPeerStats::default();
        };
        let totals = peer.conn.stats();
        let mut stats = IrohPeerStats {
            connected: peer.conn.close_reason().is_none(),
            bytes_sent: totals.udp_tx.bytes,
            bytes_received: totals.udp_rx.bytes,
            lost_packets: totals.lost_packets,
            frames_sent: peer.frames_sent.load(Ordering::Relaxed),
            frames_received: peer.frames_received.load(Ordering::Relaxed),
            frames_skipped: peer.frames_skipped.load(Ordering::Relaxed),
            ..IrohPeerStats::default()
        };
        let paths = peer.conn.paths();
        if let Some(path) = paths.iter().find(|path| path.is_selected()) {
            stats.direct = path.is_ip();
            stats.rtt_us = u64::try_from(path.rtt().as_micros()).unwrap_or(u64::MAX);
            stats.cwnd_bytes = path.stats().cwnd;
        }
        stats
    }

    pub(super) fn recv(&self) -> Option<IrohEvent> {
        if let Some(event) = lock(&self.state.events).pop_front() {
            return Some(event);
        }
        let mut frames = lock(&self.state.frames);
        let mut cursor = lock(&self.state.frame_cursor);
        let key = frames
            .range((Bound::Excluded(*cursor), Bound::Unbounded))
            .next()
            .or_else(|| frames.iter().next())
            .map(|(key, _)| *key)?;
        *cursor = key;
        frames.remove(&key)
    }

    pub(super) fn close(&self) {
        if self.endpoint.is_closed() {
            return;
        }
        let endpoint = self.endpoint.clone();
        if let Ok(runtime) = runtime() {
            runtime.spawn(async move { endpoint.close().await });
        }
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        self.close();
    }
}

impl State {
    fn push(&self, event: IrohEvent) {
        lock(&self.events).push_back(event);
    }

    fn offer_frame(&self, peer: &Peer, track: u32, sequence: u64, data: Vec<u8>) {
        {
            let mut newest = lock(&peer.newest);
            if newest.get(&track).is_some_and(|seen| *seen >= sequence) {
                return;
            }
            newest.insert(track, sequence);
        }
        let mut frames = lock(&self.frames);
        if lock(&self.peers).contains_key(&peer.id) {
            let event = IrohEvent::payload(IrohEventKind::Frame, peer.id, track, sequence, data);
            frames.insert((peer.id, track), event);
        }
    }
}

impl Peer {
    fn queue(self: &Arc<Self>, runtime: &Runtime, track: u32, frame: Outbound) {
        let slot = lock(&self.tracks)
            .entry(track)
            .or_insert_with(|| {
                let slot = Arc::new(Slot::default());
                runtime.spawn(send_frames(self.clone(), track, slot.clone()));
                slot
            })
            .clone();
        if lock(&slot.pending).replace(frame).is_some() {
            self.frames_skipped.fetch_add(1, Ordering::Relaxed);
        }
        slot.wake.notify_one();
    }
}

fn adopt(state: &Arc<State>, conn: Connection) {
    let id = state.next_peer.fetch_add(1, Ordering::Relaxed);
    let (messages, outbox) = mpsc::unbounded_channel();
    let peer = Arc::new(Peer {
        id,
        conn: conn.clone(),
        tracks: Mutex::default(),
        messages,
        newest: Mutex::default(),
        frames_sent: AtomicU64::new(0),
        frames_received: AtomicU64::new(0),
        frames_skipped: AtomicU64::new(0),
    });
    lock(&state.peers).insert(id, peer.clone());
    state.push(IrohEvent::new(
        IrohEventKind::PeerConnected,
        id,
        conn.remote_id().to_string(),
    ));
    tokio::spawn(send_messages(conn.clone(), outbox));
    tokio::spawn(receive_streams(state.clone(), peer));
    let state = state.clone();
    tokio::spawn(async move {
        let reason = conn.closed().await;
        lock(&state.peers).remove(&id);
        lock(&state.frames).retain(|(peer, _), _| *peer != id);
        state.push(IrohEvent::new(
            IrohEventKind::PeerDisconnected,
            id,
            reason.to_string(),
        ));
    });
}

async fn accept_connections(endpoint: Endpoint, state: Arc<State>) {
    while let Some(incoming) = endpoint.accept().await {
        let state = state.clone();
        tokio::spawn(async move {
            match incoming.await {
                Ok(conn) => adopt(&state, conn),
                Err(e) => state.push(IrohEvent::new(
                    IrohEventKind::Error,
                    0,
                    format!("could not accept a connection: {e}"),
                )),
            }
        });
    }
}

async fn announce_ready(endpoint: Endpoint, state: Arc<State>) {
    let mut addr = endpoint.watch_addr();
    loop {
        let current = addr.get();
        if !current.is_empty() {
            let ticket = EndpointTicket::new(current).to_string();
            state.push(IrohEvent::new(IrohEventKind::Ready, 0, ticket));
            return;
        }
        if addr.updated().await.is_err() {
            return;
        }
    }
}

async fn send_frames(peer: Arc<Peer>, track: u32, slot: Arc<Slot>) {
    loop {
        tokio::select! {
            _ = slot.wake.notified() => {}
            _ = peer.conn.closed() => return,
        }
        let Some(frame) = lock(&slot.pending).take() else {
            continue;
        };
        let Ok(mut stream) = peer.conn.open_uni().await else {
            return;
        };
        let mut header = [0u8; 13];
        header[0] = FRAME_STREAM;
        header[1..5].copy_from_slice(&track.to_le_bytes());
        header[5..].copy_from_slice(&frame.sequence.to_le_bytes());
        if stream.write_all(&header).await.is_err() || stream.write_all(&frame.data).await.is_err()
        {
            continue;
        }
        if stream.finish().is_ok() {
            peer.frames_sent.fetch_add(1, Ordering::Relaxed);
        }
    }
}

async fn send_messages(conn: Connection, mut outbox: mpsc::UnboundedReceiver<(u64, Vec<u8>)>) {
    let mut stream: Option<SendStream> = None;
    loop {
        let (sequence, data) = tokio::select! {
            next = outbox.recv() => match next {
                Some(message) => message,
                None => break,
            },
            _ = conn.closed() => return,
        };
        if stream.is_none() {
            let Ok(mut opened) = conn.open_uni().await else {
                return;
            };
            let _ = opened.set_priority(MESSAGE_PRIORITY);
            if opened.write_all(&[MESSAGE_STREAM]).await.is_err() {
                return;
            }
            stream = Some(opened);
        }
        let Some(open) = stream.as_mut() else {
            return;
        };
        let mut header = [0u8; 12];
        header[..8].copy_from_slice(&sequence.to_le_bytes());
        header[8..].copy_from_slice(&(data.len() as u32).to_le_bytes());
        if open.write_all(&header).await.is_err() || open.write_all(&data).await.is_err() {
            return;
        }
    }
    if let Some(mut open) = stream {
        let _ = open.finish();
    }
}

async fn receive_streams(state: Arc<State>, peer: Arc<Peer>) {
    while let Ok(stream) = peer.conn.accept_uni().await {
        tokio::spawn(receive_stream(state.clone(), peer.clone(), stream));
    }
}

async fn receive_stream(state: Arc<State>, peer: Arc<Peer>, mut stream: RecvStream) {
    let mut kind = [0u8; 1];
    if stream.read_exact(&mut kind).await.is_err() {
        return;
    }
    match kind[0] {
        FRAME_STREAM => {
            let mut header = [0u8; 12];
            if stream.read_exact(&mut header).await.is_err() {
                return;
            }
            let track = u32::from_le_bytes([header[0], header[1], header[2], header[3]]);
            let mut sequence = [0u8; 8];
            sequence.copy_from_slice(&header[4..]);
            let data = match stream.read_to_end(state.max_frame).await {
                Ok(data) => data,
                Err(ReadToEndError::TooLong) => {
                    let _ = stream.stop(VarInt::from_u32(PROTOCOL_VIOLATION));
                    return;
                }
                Err(ReadToEndError::Read(_)) => return,
            };
            peer.frames_received.fetch_add(1, Ordering::Relaxed);
            state.offer_frame(&peer, track, u64::from_le_bytes(sequence), data);
        }
        MESSAGE_STREAM => loop {
            let mut header = [0u8; 12];
            if stream.read_exact(&mut header).await.is_err() {
                return;
            }
            let mut sequence = [0u8; 8];
            sequence.copy_from_slice(&header[..8]);
            let length =
                u32::from_le_bytes([header[8], header[9], header[10], header[11]]) as usize;
            if length > state.max_frame {
                let _ = stream.stop(VarInt::from_u32(PROTOCOL_VIOLATION));
                return;
            }
            let mut data = vec![0u8; length];
            if stream.read_exact(&mut data).await.is_err() {
                return;
            }
            state.push(IrohEvent::payload(
                IrohEventKind::Message,
                peer.id,
                0,
                u64::from_le_bytes(sequence),
                data,
            ));
        },
        _ => {
            let _ = stream.stop(VarInt::from_u32(PROTOCOL_VIOLATION));
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use azul_css::AzString;

    use super::*;

    fn local_endpoint() -> Engine {
        let config = IrohConfig::create(AzString::from_const_str("azul/iroh-test/1"))
            .with_relay_mode(IrohRelayMode::Disabled);
        Engine::bind(&config).expect("a relay-less endpoint binds")
    }

    fn next_of(engine: &Engine, kind: IrohEventKind) -> IrohEvent {
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            match engine.recv() {
                Some(event) if event.kind == kind => return event,
                Some(event) => {
                    assert_ne!(event.kind, IrohEventKind::Error, "{}", event.text.as_str())
                }
                None => {
                    assert!(Instant::now() < deadline, "no {kind:?} event within 20 s");
                    std::thread::sleep(Duration::from_millis(5));
                }
            }
        }
    }

    #[test]
    fn frames_and_messages_cross_a_local_connection() {
        let host = local_endpoint();
        let guest = local_endpoint();
        let ticket = next_of(&host, IrohEventKind::Ready).text;
        guest.connect(ticket.as_str()).expect("the ticket parses");
        let host_side = next_of(&host, IrohEventKind::PeerConnected).peer;
        let guest_side = next_of(&guest, IrohEventKind::PeerConnected).peer;
        assert_eq!(
            guest.peer_endpoint_id(guest_side).as_deref(),
            Some(host.endpoint_id().as_str())
        );

        assert!(guest.send_frame(Some(guest_side), 7, b"first frame"));
        let frame = next_of(&host, IrohEventKind::Frame);
        assert_eq!(frame.peer, host_side);
        assert_eq!(frame.track, 7);
        assert_eq!(frame.data.as_ref(), b"first frame");

        let messages: [&[u8]; 3] = [b"one", b"two", b"three"];
        for message in messages {
            assert!(host.send_message(host_side, message));
        }
        for message in messages {
            assert_eq!(
                next_of(&guest, IrohEventKind::Message).data.as_ref(),
                message
            );
        }

        assert!(guest.peer_stats(guest_side).connected);
        assert!(guest.disconnect(guest_side));
        assert_eq!(
            next_of(&host, IrohEventKind::PeerDisconnected).peer,
            host_side
        );
    }

    #[test]
    fn a_malformed_ticket_is_rejected_before_dialing() {
        let endpoint = local_endpoint();
        assert!(endpoint.connect("not a ticket").is_err());
        assert!(endpoint.connect(&endpoint.endpoint_id()).is_err());
    }
}
