//! S3 over iroh for the apps: a [`Transport`] through azul's iroh endpoint
//! (`IrohEndpoint::request`) - the same signed request as over HTTPS, as HTTP/1.1 bytes
//! ([`crate::http1`]) on a bidirectional QUIC stream of ALPN [`S3_OVER_IROH_ALPN`], to an Azlin
//! node dialed by its endpoint id at its iroh sockets (the `iroh_addrs` of the token server's
//! node list: no discovery and no DNS). Feature `azul`.
//!
//! The engine waits on its own runtime, so no UI pump is needed (unlike `AzulTransport`): a
//! headless app (the bridge) can send through it too. Blocking: call it from a worker thread,
//! never from a UI callback. The endpoints are the process's, one per relay setting, bound on
//! first use and kept: every node's connection is kept by the engine for the next request.

use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex, OnceLock, PoisonError},
};

use azul::{
    error::ResultU8VecString,
    iroh::{IrohConfig, IrohEndpoint, IrohRelayMode},
    str::String as AzString,
    vec::{StringVec, U8Vec},
};

use crate::{
    http1::{decode_reply, encode_request, S3_OVER_IROH_ALPN},
    HttpCall, HttpReply, Method, Transport,
};

/// Seconds one request over iroh may take (a part of a big upload included).
pub const IROH_TIMEOUT_SECS: u32 = 120;
/// The largest answer taken (a whole object of a GET without ranges).
pub const MAX_ANSWER_BYTES: u32 = 1 << 30;

/// The library's endpoint handle, shared by the threads of the process.
struct Shared(IrohEndpoint);

// SAFETY: the handle points to the library's `Arc` of the endpoint: its engine (an iroh
// endpoint on the library's own runtime) and its queues are behind mutexes; every call takes
// `&self`, and a copy is the library's clone of the `Arc`.
unsafe impl Send for Shared {}
unsafe impl Sync for Shared {}

fn endpoints() -> &'static Mutex<BTreeMap<String, Arc<Shared>>> {
    static ENDPOINTS: OnceLock<Mutex<BTreeMap<String, Arc<Shared>>>> = OnceLock::new();
    ENDPOINTS.get_or_init(|| Mutex::new(BTreeMap::new()))
}

/// The relay setting as the endpoints are kept under: `off` (none given), `default` or an
/// address.
fn relay_key(relay: Option<&str>) -> String {
    match relay.map(str::trim) {
        None | Some("") | Some("off") => String::from("off"),
        Some(other) => other.to_string(),
    }
}

/// The process's client endpoint for S3 over iroh with the relay setting `relay` (`off`: direct
/// sockets only; `default`: n0's relays; an address: that relay), bound on first use.
fn endpoint(relay: Option<&str>) -> Result<Arc<Shared>, String> {
    let key = relay_key(relay);
    let mut known = endpoints().lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(bound) = known.get(&key) {
        if bound.0.is_bound() {
            return Ok(bound.clone());
        }
    }
    let config = IrohConfig::create(S3_OVER_IROH_ALPN).with_max_frame_bytes(MAX_ANSWER_BYTES);
    let config = match key.as_str() {
        "off" => config.with_relay_mode(IrohRelayMode::Disabled),
        "default" => config.with_relay_mode(IrohRelayMode::Default),
        url => config.with_relay_url(url),
    };
    let bound = IrohEndpoint::bind(config);
    if !bound.is_bound() {
        let why = bound
            .recv()
            .into_option()
            .map(|event| event.text.as_str().to_string())
            .unwrap_or_else(|| String::from("the iroh endpoint did not bind"));
        return Err(why);
    }
    let shared = Arc::new(Shared(bound));
    known.insert(key, shared.clone());
    Ok(shared)
}

/// S3 over iroh to one node.
pub struct AzulIrohTransport {
    endpoint: Arc<Shared>,
    id: String,
    addrs: Vec<String>,
    /// The node's home relay (the configured relay when it is an address), else empty.
    relay_url: String,
    timeout_secs: u32,
}

impl std::fmt::Debug for AzulIrohTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AzulIrohTransport")
            .field("id", &self.id)
            .field("addrs", &self.addrs)
            .field("relay_url", &self.relay_url)
            .finish_non_exhaustive()
    }
}

/// A transport to the node `id` (its endpoint id, the node's Ed25519 key) at `addrs` (`ip:port`),
/// through the process's endpoint of the relay setting `relay` (`off` or `None`, `default`, an
/// address - which is the node's home relay too): what an app's iroh dialer hands azcloud-kit's
/// lane. Connects on the first request.
///
/// # Errors
///
/// When the endpoint cannot bind (a build without the iroh engine).
pub fn dial(id: &str, addrs: &[String], relay: Option<&str>) -> Result<Box<dyn Transport>, String> {
    let endpoint = endpoint(relay)?;
    let key = relay_key(relay);
    let relay_url = if matches!(key.as_str(), "off" | "default") {
        String::new()
    } else {
        key
    };
    Ok(Box::new(AzulIrohTransport {
        endpoint,
        id: id.trim().to_string(),
        addrs: addrs.to_vec(),
        relay_url,
        timeout_secs: IROH_TIMEOUT_SECS,
    }))
}

impl Transport for AzulIrohTransport {
    fn send(&self, call: &HttpCall) -> Result<HttpReply, String> {
        let request = encode_request(call)?;
        let addresses = StringVec::from_vec(
            self.addrs
                .iter()
                .map(|a| AzString::from(a.as_str()))
                .collect(),
        );
        let answer = self.endpoint.0.request(
            self.id.as_str(),
            addresses,
            self.relay_url.as_str(),
            U8Vec::from(request),
            false,
            self.timeout_secs,
        );
        match answer {
            ResultU8VecString::Ok(bytes) => {
                decode_reply(bytes.as_slice(), call.method == Method::Head)
            }
            ResultU8VecString::Err(why) => Err(format!("over iroh: {}", why.as_str())),
        }
    }
}
