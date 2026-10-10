//! How the bridge's requests to the drive and the token server leave the process: ureq with
//! rustls, the pure-Rust RustCrypto provider and the Mozilla roots - exactly the HTTP client
//! azul-layout builds for the apps, without libazul. Redirects are not followed (a signed S3
//! request must not go anywhere else) and every status comes back as an answer.
//!
//! DNS down: the drive's failover hands every transport the addresses of the nodes (and of the
//! block host) before each request ([`Transport::fallback_addresses`]); a name whose lookup
//! fails (or gives up after [`DNS_LOOKUP_TIMEOUT`]) is reached there, the request and TLS still
//! naming the host - what azul-layout's client does for the apps (its `FallbackResolver`, which
//! the bridge cannot link without libazul). A name without addresses fails with
//! [`azul_storage::transport::DNS_FAILED`] in front.

use std::{
    collections::BTreeMap,
    net::{IpAddr, SocketAddr},
    sync::{Arc, Mutex},
    time::Duration,
};

use azul_storage::{transport::DNS_FAILED, HttpCall, HttpReply, Method, Transport};
use ureq::unversioned::{
    resolver::{DefaultResolver, ResolvedSocketAddrs, Resolver},
    transport::{DefaultConnector, NextTimeout},
};

/// A request (and its answer's body) may take this long.
pub const TIMEOUT: Duration = Duration::from_secs(120);
/// An answer's body is at most this big (S3's largest single PUT).
pub const MAX_BODY: u64 = 5 * 1024 * 1024 * 1024;
/// How long one DNS lookup may take before the fallback addresses are tried.
pub const DNS_LOOKUP_TIMEOUT: Duration = Duration::from_secs(8);
/// The most fallback addresses of one host (what ureq's resolver answers at most).
const MAX_ADDRESSES: usize = 16;

/// Host (lowercase, no brackets) -> its fallback addresses, each with the port it names.
type Fallback = Arc<Mutex<BTreeMap<String, Vec<(IpAddr, Option<u16>)>>>>;

/// A host as its fallback addresses are kept under.
fn host_key(host: &str) -> String {
    host.trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .to_ascii_lowercase()
}

/// `192.0.2.7`, `2001:db8::1`, `[2001:db8::1]`, `192.0.2.7:8443`, `[2001:db8::1]:8443`.
fn parse_address(address: &str) -> Option<(IpAddr, Option<u16>)> {
    let address = address.trim();
    if let Ok(socket) = address.parse::<SocketAddr>() {
        return Some((socket.ip(), Some(socket.port())));
    }
    address
        .trim_start_matches('[')
        .trim_end_matches(']')
        .parse::<IpAddr>()
        .ok()
        .map(|ip| (ip, None))
}

/// A lookup that failed with no fallback address: its words start with [`DNS_FAILED`].
#[derive(Debug)]
struct DnsFailure(String);

impl std::fmt::Display for DnsFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for DnsFailure {}

/// The system's lookup first; when it fails, the host's fallback addresses.
#[derive(Debug)]
struct FallbackResolver {
    inner: DefaultResolver,
    fallback: Fallback,
}

impl Resolver for FallbackResolver {
    fn resolve(
        &self,
        uri: &ureq::http::Uri,
        config: &ureq::config::Config,
        timeout: NextTimeout,
    ) -> Result<ResolvedSocketAddrs, ureq::Error> {
        let error = match self.inner.resolve(uri, config, timeout) {
            Ok(found) => return Ok(found),
            Err(error @ ureq::Error::BadUri(_)) => return Err(error),
            Err(error) => error,
        };
        let host = uri.host().map(host_key).unwrap_or_default();
        let known = self
            .fallback
            .lock()
            .ok()
            .and_then(|known| known.get(&host).cloned())
            .filter(|addresses| !addresses.is_empty());
        let Some(known) = known else {
            return Err(ureq::Error::Other(Box::new(DnsFailure(format!(
                "{DNS_FAILED}: {host} ({error})"
            )))));
        };
        let default_port = if uri.scheme_str() == Some("https") {
            443
        } else {
            80
        };
        let port = uri.port_u16().unwrap_or(default_port);
        let mut out = self.inner.empty();
        for (ip, named) in known.into_iter().take(MAX_ADDRESSES) {
            out.push(SocketAddr::new(ip, named.unwrap_or(port)));
        }
        Ok(out)
    }
}

/// A transport error in words; a lookup that failed starts with [`DNS_FAILED`].
fn error_text(e: &ureq::Error) -> String {
    match e {
        ureq::Error::Other(inner) if inner.downcast_ref::<DnsFailure>().is_some() => {
            inner.to_string()
        }
        other => other.to_string(),
    }
}

/// azul-storage's [`Transport`] over ureq.
#[derive(Debug, Clone)]
pub struct UreqTransport {
    agent: ureq::Agent,
    /// Where a host is reached when its name does not resolve (the failover's addresses).
    fallback: Fallback,
}

impl Default for UreqTransport {
    fn default() -> UreqTransport {
        UreqTransport::new()
    }
}

impl UreqTransport {
    #[must_use]
    pub fn new() -> UreqTransport {
        let tls = ureq::tls::TlsConfig::builder()
            .provider(ureq::tls::TlsProvider::Rustls)
            .unversioned_rustls_crypto_provider(Arc::new(rustls_rustcrypto::provider()))
            .root_certs(ureq::tls::RootCerts::WebPki)
            .build();
        let config = ureq::Agent::config_builder()
            .tls_config(tls)
            .http_status_as_error(false)
            .max_redirects(0)
            .timeout_global(Some(TIMEOUT))
            .timeout_resolve(Some(DNS_LOOKUP_TIMEOUT))
            .build();
        let fallback = Fallback::default();
        let agent = ureq::Agent::with_parts(
            config,
            DefaultConnector::default(),
            FallbackResolver {
                inner: DefaultResolver::default(),
                fallback: fallback.clone(),
            },
        );
        UreqTransport { agent, fallback }
    }
}

impl Transport for UreqTransport {
    fn send(&self, call: &HttpCall) -> Result<HttpReply, String> {
        let url = call.url.as_str();
        let response = match call.method {
            Method::Get | Method::Head | Method::Delete => {
                let mut request = match call.method {
                    Method::Get => self.agent.get(url),
                    Method::Head => self.agent.head(url),
                    _ => self.agent.delete(url),
                };
                for (name, value) in &call.headers {
                    request = request.header(name.as_str(), value.as_str());
                }
                request.call()
            }
            Method::Put | Method::Post | Method::Patch => {
                let mut request = match call.method {
                    Method::Put => self.agent.put(url),
                    Method::Post => self.agent.post(url),
                    _ => self.agent.patch(url),
                };
                for (name, value) in &call.headers {
                    request = request.header(name.as_str(), value.as_str());
                }
                if !call.content_type.is_empty() {
                    request = request.header("Content-Type", call.content_type.as_str());
                }
                request.send(&call.body[..])
            }
            other => {
                return Err(format!(
                    "the bridge's HTTP client does not send {} requests",
                    other.as_str()
                ))
            }
        }
        .map_err(|e| error_text(&e))?;
        let status = response.status().as_u16();
        let headers: Vec<(String, String)> = response
            .headers()
            .iter()
            .filter_map(|(name, value)| {
                value
                    .to_str()
                    .ok()
                    .map(|v| (name.as_str().to_string(), v.to_string()))
            })
            .collect();
        let body = if call.method == Method::Head {
            Vec::new()
        } else {
            response
                .into_body()
                .into_with_config()
                .limit(MAX_BODY)
                .read_to_vec()
                .map_err(|e| e.to_string())?
        };
        Ok(HttpReply {
            status,
            headers,
            body,
        })
    }

    /// This transport reaches `host` at `addresses` whenever its name does not resolve (the
    /// request and TLS still name `host`); `false` when no address was usable.
    fn fallback_addresses(&self, host: &str, addresses: &[String]) -> bool {
        let parsed: Vec<(IpAddr, Option<u16>)> =
            addresses.iter().filter_map(|a| parse_address(a)).collect();
        let host = host_key(host);
        if parsed.is_empty() || host.is_empty() {
            return false;
        }
        let Ok(mut known) = self.fallback.lock() else {
            return false;
        };
        let kept = known.entry(host).or_default();
        for address in parsed {
            if !kept.contains(&address) && kept.len() < MAX_ADDRESSES {
                kept.push(address);
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};

    use super::*;
    use crate::net::bind_loopback;

    /// A one-request HTTP server: answers `reply` and hands back the request it read.
    fn server(reply: &'static [u8]) -> (String, std::sync::mpsc::Receiver<Vec<u8>>) {
        let listener = bind_loopback(0).unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
            let mut request = Vec::new();
            let mut buf = [0u8; 4096];
            // The head, then as much body as Content-Length says.
            loop {
                let n = stream.read(&mut buf).unwrap_or(0);
                if n == 0 {
                    break;
                }
                request.extend_from_slice(&buf[..n]);
                if let Some(end) = request.windows(4).position(|w| w == b"\r\n\r\n") {
                    let head = String::from_utf8_lossy(&request[..end]).to_ascii_lowercase();
                    let length = head
                        .lines()
                        .find_map(|l| l.strip_prefix("content-length:"))
                        .and_then(|v| v.trim().parse::<usize>().ok())
                        .unwrap_or(0);
                    if request.len() >= end + 4 + length {
                        break;
                    }
                }
            }
            stream.write_all(reply).unwrap();
            let _ = tx.send(request);
        });
        (url, rx)
    }

    #[test]
    fn a_signed_put_goes_out_with_its_headers_and_body_and_any_status_comes_back() {
        let (url, requests) = server(b"HTTP/1.1 403 Forbidden\r\nContent-Length: 6\r\nX-Amz-Request-Id: r1\r\n\r\ndenied");
        let call = HttpCall {
            method: Method::Put,
            url: format!("{url}/bucket/mail/Inbox/a.eml"),
            headers: vec![
                (String::from("x-amz-date"), String::from("20261001T083000Z")),
                (String::from("authorization"), String::from("AWS4-HMAC-SHA256 test")),
            ],
            body: b"message".to_vec(),
            content_type: String::from("message/rfc822"),
        };
        let reply = UreqTransport::new().send(&call).unwrap();
        assert_eq!(reply.status, 403);
        assert_eq!(reply.body, b"denied");
        assert_eq!(reply.header("x-amz-request-id"), Some("r1"));
        let request = String::from_utf8(requests.recv().unwrap()).unwrap();
        let lower = request.to_ascii_lowercase();
        assert!(request.starts_with("PUT /bucket/mail/Inbox/a.eml HTTP/1.1\r\n"), "{request}");
        assert!(lower.contains("x-amz-date: 20261001t083000z"), "{request}");
        assert!(lower.contains("content-type: message/rfc822"), "{request}");
        assert!(request.ends_with("\r\n\r\nmessage"), "{request}");
    }

    #[test]
    fn a_redirect_is_not_followed_and_no_answer_is_an_error() {
        let (url, _requests) = server(b"HTTP/1.1 301 Moved Permanently\r\nLocation: http://192.0.2.1/x\r\nContent-Length: 0\r\n\r\n");
        let call = HttpCall {
            method: Method::Get,
            url: format!("{url}/x"),
            headers: Vec::new(),
            body: Vec::new(),
            content_type: String::new(),
        };
        let reply = UreqTransport::new().send(&call).unwrap();
        assert_eq!(reply.status, 301);
        let nobody = HttpCall {
            url: String::from("http://127.0.0.1:1/x"),
            ..call
        };
        assert!(UreqTransport::new().send(&nobody).is_err());
    }

    /// DNS down: a name that does not resolve is reached at the addresses the failover handed
    /// over (the request still names the host), and without them it fails as a DNS failure.
    #[test]
    fn a_name_that_does_not_resolve_is_reached_at_its_fallback_address_under_its_name() {
        let (url, requests) = server(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok");
        let port = url.rsplit(':').next().unwrap().to_string();
        let call = HttpCall {
            method: Method::Get,
            url: format!("http://n2.azlin-dns-test.invalid:{port}/bucket/a.txt"),
            headers: Vec::new(),
            body: Vec::new(),
            content_type: String::new(),
        };
        let transport = UreqTransport::new();
        let why = transport.send(&call).unwrap_err();
        assert!(
            azul_storage::transport::is_dns_failure(&why)
                && why.starts_with(azul_storage::transport::DNS_FAILED),
            "{why}"
        );
        assert!(!transport.fallback_addresses("n2.azlin-dns-test.invalid", &[String::from("no")]));
        assert!(transport.fallback_addresses("N2.azlin-dns-test.invalid", &[String::from("127.0.0.1")]));
        let reply = transport.send(&call).unwrap();
        assert_eq!(reply.status, 200);
        let request = String::from_utf8(requests.recv().unwrap()).unwrap();
        assert!(
            request
                .to_ascii_lowercase()
                .contains(&format!("host: n2.azlin-dns-test.invalid:{port}")),
            "the request still names the host: {request}"
        );
        assert!(
            UreqTransport::new().send(&call).is_err(),
            "another transport of its own knows no address"
        );
    }
}
