//! Which transport a drive's requests take: iroh first when a node's iroh id is known, HTTPS
//! as the fallback.
//!
//! - `auto` (the default): when the build can dial iroh (an [`IrohDialer`] is plugged in) and
//!   there is a node to dial, one cheap signed request (a HEAD) probes it; an answer of any
//!   status means the pipe works and the session goes over iroh. No answer within 10 s, or an
//!   error, means HTTPS, and `transport.json` remembers it: the next runs take HTTPS straight
//!   away and try iroh again five minutes after the failure. A request that fails over iroh
//!   later is retried over HTTPS - unless a second probe shows iroh is fine, i.e. the error was
//!   the server's answer and HTTPS would only repeat it.
//! - `iroh`: iroh or nothing (an end-to-end run's iroh leg must not pass over HTTPS by
//!   accident).
//! - `https`: HTTPS only (the block endpoint, then the direct node URLs and what
//!   `x-azlin-alt-endpoints` taught: the [`crate::bucket::Bucket`]'s failover).
//!
//! Where the iroh node comes from: `--iroh-node` / `AZLIN_IROH_NODE` (with `--iroh-addr` /
//! `AZLIN_IROH_ADDR`, the node's UDP socket, so no discovery is needed), else the node list of
//! the last credential refresh when it names an `iroh_id` (or `sign_pubkey`) and `iroh_addrs`.
//! The client's relay is the configured one (`endpoints.relay`): `off`, `default` or an
//! address.
//!
//! The kit links no iroh: "S3 over iroh" sends the same signed HTTP requests over a QUIC
//! stream, so an iroh build plugs in an [`IrohDialer`] that makes an azul-storage
//! [`azul_storage::Transport`] to a node, and the kit does the rest.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::settings::Settings;

/// How long a probe waits for iroh's answer.
pub const PROBE_TIMEOUT: Duration = Duration::from_secs(10);
/// After iroh failed, how long `auto` keeps to HTTPS before it tries again.
pub const IROH_RETRY_SECS: i64 = 300;
/// The key a probe asks for: never written (`.azlin/` is bookkeeping), so the
/// answer is a 404 over a working pipe.
pub const PROBE_KEY: &str = ".azlin/probe";
/// Why `auto` cannot use iroh without a node to dial.
pub const NO_IROH_NODE: &str = "no node's iroh id is known: pass --iroh-node (AZLIN_IROH_NODE); \
                                the token server's node list carries no iroh ids yet";

/// The transport asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TransportPref {
    Auto,
    Iroh,
    Https,
}

impl TransportPref {
    /// `auto`, `iroh` (also `quic`), `https` (also `http`).
    #[must_use]
    pub fn parse(name: &str) -> Option<TransportPref> {
        match name.trim().to_ascii_lowercase().as_str() {
            "auto" => Some(TransportPref::Auto),
            "iroh" | "quic" => Some(TransportPref::Iroh),
            "https" | "http" => Some(TransportPref::Https),
            _ => None,
        }
    }

    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            TransportPref::Auto => "auto",
            TransportPref::Iroh => "iroh",
            TransportPref::Https => "https",
        }
    }
}

/// The transport a request takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Lane {
    Iroh,
    Https,
}

impl Lane {
    /// `iroh` or `https`.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Lane::Iroh => "iroh",
            Lane::Https => "https",
        }
    }
}

/// A node to dial over iroh.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct IrohTarget {
    /// Its endpoint id: the node's Ed25519 key (`sign_pubkey`), hex or base32.
    pub id: String,
    /// Its UDP socket (`ip:port`), when known.
    pub addr: Option<String>,
    /// Where the two came from.
    pub source: String,
}

/// What `transport.json` keeps between runs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransportMemory {
    pub lane: Lane,
    pub reason: String,
    pub decided_at: i64,
    /// When iroh last failed (`auto` keeps to HTTPS for five minutes after).
    #[serde(default)]
    pub iroh_failed_at: Option<i64>,
    #[serde(default)]
    pub iroh_error: Option<String>,
}

/// What to do before any request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Decision {
    /// HTTPS, for this reason.
    Https(String),
    /// Probe iroh (and use it when it answers).
    Probe,
}

/// The decision before any request: pure, so the tests walk every case.
/// `iroh_built` says whether this build can dial iroh.
///
/// # Errors
///
/// When iroh was asked for and cannot be had.
pub fn decide(
    pref: TransportPref,
    iroh_built: bool,
    target: Option<&IrohTarget>,
    memory: Option<&TransportMemory>,
    now: i64,
) -> Result<Decision, String> {
    match pref {
        TransportPref::Https => Ok(Decision::Https(String::from(
            "https was asked for (--transport / AZCLOUD_TRANSPORT)",
        ))),
        TransportPref::Iroh => {
            if !iroh_built {
                return Err(String::from(
                    "iroh was asked for, but this azcloud was built without the iroh feature",
                ));
            }
            if target.is_none() {
                return Err(format!("iroh was asked for, but {NO_IROH_NODE}"));
            }
            Ok(Decision::Probe)
        }
        TransportPref::Auto => {
            if !iroh_built {
                return Ok(Decision::Https(String::from(
                    "this azcloud was built without the iroh feature",
                )));
            }
            if target.is_none() {
                return Ok(Decision::Https(String::from(NO_IROH_NODE)));
            }
            if let Some((failed, error)) = memory.and_then(|m| {
                m.iroh_failed_at
                    .map(|at| (at, m.iroh_error.clone().unwrap_or_default()))
            }) {
                let ago = now - failed;
                if (0..IROH_RETRY_SECS).contains(&ago) {
                    return Ok(Decision::Https(format!(
                        "iroh failed {ago} s ago ({error}); it is tried again {} s after a failure",
                        IROH_RETRY_SECS
                    )));
                }
            }
            Ok(Decision::Probe)
        }
    }
}

/// The node to dial: the pinned one (`--iroh-node` / `AZLIN_IROH_NODE`), else
/// the first node of the node list that names its iroh id.
#[must_use]
pub fn iroh_target(settings: &Settings, nodes: &[Value]) -> Option<IrohTarget> {
    if let Some(id) = settings.iroh_node.value.as_deref() {
        return Some(IrohTarget {
            id: id.to_string(),
            addr: settings.iroh_addr.value.clone(),
            source: settings.iroh_node.source.label(),
        });
    }
    target_from_nodes(nodes)
}

/// The first ready node of a node list that names its iroh id (`iroh_id`,
/// else `sign_pubkey`), with its first IPv4 socket of `iroh_addrs`.
#[must_use]
pub fn target_from_nodes(nodes: &[Value]) -> Option<IrohTarget> {
    nodes.iter().find_map(|n| {
        if n["ready"].as_bool() == Some(false) {
            return None;
        }
        let id = n["iroh_id"]
            .as_str()
            .or_else(|| n["sign_pubkey"].as_str())
            .filter(|id| !id.is_empty())?;
        let addr = n["iroh_addrs"].as_array().and_then(|addrs| {
            addrs
                .iter()
                .filter_map(Value::as_str)
                .find(|a| a.contains('.'))
                .map(String::from)
        });
        Some(IrohTarget {
            id: id.to_string(),
            addr,
            source: format!("the node list ({})", n["name"].as_str().unwrap_or("a node")),
        })
    })
}

/// What the drive uses and why (`azcloud transport`).
#[derive(Clone, Debug, Serialize)]
pub struct TransportReport {
    pub preference: TransportPref,
    pub lane: Lane,
    pub reason: String,
    pub iroh_built: bool,
    pub iroh_target: Option<IrohTarget>,
    pub relay: Option<String>,
    /// The S3 endpoint and where it came from.
    pub endpoint: String,
    pub endpoint_source: String,
    /// Whether this run probed iroh.
    pub probed: bool,
    /// Set when a request fell back from iroh to HTTPS during the run.
    pub fell_back: Option<String>,
}
