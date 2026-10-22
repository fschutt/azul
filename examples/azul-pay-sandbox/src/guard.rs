//! What keeps the sandbox on this computer and out of releases: loopback names and addresses
//! only (for the Host of every request and for the webhook URL), and no start in a release build.

use std::net::IpAddr;

use crate::Error;

/// Whether a host (no port) names this computer: `localhost`, a 127.0.0.0/8 address or `::1`.
fn loopback_name(host: &str) -> bool {
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    let bare = host.strip_prefix('[').and_then(|h| h.strip_suffix(']')).unwrap_or(host);
    bare.parse::<IpAddr>().is_ok_and(|ip| ip.is_loopback())
}

/// Whether a `Host` header names this computer (`127.0.0.1:8784`, `localhost`, `[::1]:8784`).
/// Anything else is refused, which is also what makes DNS rebinding fail: a public name that
/// resolves to 127.0.0.1 still says its own name here.
pub fn loopback_host(header: &str) -> bool {
    let h = header.trim();
    if h.is_empty() || h.contains('@') || h.contains('/') {
        return false;
    }
    let host = if let Some(rest) = h.strip_prefix('[') {
        // [::1] or [::1]:8784
        match rest.split_once(']') {
            Some((inner, port)) if port.is_empty() || port.strip_prefix(':').is_some_and(valid_port) => {
                format!("[{inner}]")
            }
            _ => return false,
        }
    } else {
        match h.rsplit_once(':') {
            Some((name, port)) if valid_port(port) => name.to_string(),
            Some(_) => return false,
            None => h.to_string(),
        }
    };
    loopback_name(&host)
}

fn valid_port(p: &str) -> bool {
    !p.is_empty() && p.len() <= 5 && p.bytes().all(|b| b.is_ascii_digit()) && p.parse::<u16>().is_ok()
}

/// A webhook URL that stays on this computer, split for the plain HTTP client.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoopbackUrl {
    /// The host as written (`127.0.0.1`, `localhost`, `[::1]`).
    pub host: String,
    pub port: u16,
    /// The path and query, `/` at least.
    pub path: String,
}

/// `url` when it is `http://<loopback>[:port]/...` and nothing else: no other scheme (the
/// sandbox speaks plain HTTP on this computer), no userinfo, no public name.
pub fn loopback_webhook_url(url: &str) -> Result<LoopbackUrl, Error> {
    let refuse = || Error::NotLoopback(format!("the webhook URL {url:?} is not http:// on this computer"));
    let rest = url.trim().strip_prefix("http://").ok_or_else(refuse)?;
    let (authority, path) = match rest.find(['/', '?', '#']) {
        Some(i) => (rest.get(..i).unwrap_or(""), rest.get(i..).unwrap_or("/")),
        None => (rest, "/"),
    };
    if authority.contains('@') || !loopback_host(authority) {
        return Err(refuse());
    }
    let (host, port) = if let Some(inner) = authority.strip_prefix('[') {
        let (h, p) = inner.split_once(']').ok_or_else(refuse)?;
        (format!("[{h}]"), p.strip_prefix(':').unwrap_or(""))
    } else {
        match authority.rsplit_once(':') {
            Some((h, p)) => (h.to_string(), p),
            None => (authority.to_string(), ""),
        }
    };
    let port = if port.is_empty() { 80 } else { port.parse::<u16>().map_err(|_| refuse())? };
    let path = if path.starts_with('/') { path.to_string() } else { format!("/{path}") };
    Ok(LoopbackUrl { host, port, path })
}

/// A release build never runs the sandbox: `debug` is `cfg!(debug_assertions)`. (A
/// `compile_error!` would break every workspace-wide release build.)
pub fn refuse_in_release(debug: bool) -> Result<(), Error> {
    if debug {
        Ok(())
    } else {
        Err(Error::Release)
    }
}
