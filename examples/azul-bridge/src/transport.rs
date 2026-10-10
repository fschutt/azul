//! How the bridge's requests to the drive and the token server leave the process: ureq with
//! rustls, the pure-Rust RustCrypto provider and the Mozilla roots - exactly the HTTP client
//! azul-layout builds for the apps, without libazul. Redirects are not followed (a signed S3
//! request must not go anywhere else) and every status comes back as an answer.

use std::{sync::Arc, time::Duration};

use azul_storage::{HttpCall, HttpReply, Method, Transport};

/// A request (and its answer's body) may take this long.
pub const TIMEOUT: Duration = Duration::from_secs(120);
/// An answer's body is at most this big (S3's largest single PUT).
pub const MAX_BODY: u64 = 5 * 1024 * 1024 * 1024;

/// azul-storage's [`Transport`] over ureq.
#[derive(Debug, Clone)]
pub struct UreqTransport {
    agent: ureq::Agent,
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
        let agent = ureq::Agent::config_builder()
            .tls_config(tls)
            .http_status_as_error(false)
            .max_redirects(0)
            .timeout_global(Some(TIMEOUT))
            .build()
            .new_agent();
        UreqTransport { agent }
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
        .map_err(|e| e.to_string())?;
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
