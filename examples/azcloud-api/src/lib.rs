//! azcloud-api: the `azcloud` command line and the two transports it plugs into azcloud-kit,
//! which holds everything else (the account, the state folder, the settings, the bucket, the
//! sync, the links):
//!
//! - [`https::HttpsTransport`]: azul-storage's `Transport` over reqwest (the apps send through
//!   azul's HTTP client instead; the command line links no libazul).
//! - [`iroh_lane::IrohLane`] (feature `iroh`): the kit's `IrohDialer` - "S3 over iroh", the
//!   same signed requests on a QUIC stream to a node, through azlin-client's iroh transport;
//!   the kit probes it and falls back to HTTPS.
//!
//! Both run their futures on the command line's one tokio runtime and block the calling thread
//! until the answer is there: the kit's calls are blocking, made from the main thread and the
//! kit's own threads, never from inside the runtime.

use std::time::Duration;

use azcloud_kit::settings::OsDirs;
use azul_storage::{HttpCall, HttpReply, Transport};
use tokio::runtime::Handle;

/// How long one HTTPS request may take (a part of a big upload included).
pub const HTTPS_TIMEOUT: Duration = Duration::from_secs(120);

/// This user's OS folders (azul's `FilePath` returns the same ones: both use `dirs`), so the
/// data root here is the one the apps write.
#[must_use]
pub fn os_dirs() -> OsDirs {
    OsDirs {
        home: dirs::home_dir(),
        config: dirs::config_dir(),
        data: dirs::data_dir(),
    }
}

/// The reply parts of a transport answer.
fn reply_of(status: u16, headers: Vec<(String, String)>, body: Vec<u8>) -> HttpReply {
    HttpReply {
        status,
        headers,
        body,
    }
}

pub mod https {
    //! HTTPS (and HTTP to this computer) through reqwest, blocking on the runtime.

    use super::*;

    /// reqwest's client on the command line's runtime, as azul-storage's blocking `Transport`.
    #[derive(Clone)]
    pub struct HttpsTransport {
        client: reqwest::Client,
        runtime: Handle,
    }

    impl HttpsTransport {
        /// A client whose requests take at most `timeout`, run on `runtime`.
        ///
        /// # Errors
        ///
        /// When the HTTP client cannot be built.
        pub fn new(runtime: Handle, timeout: Duration) -> anyhow::Result<HttpsTransport> {
            azlin_client::install_crypto_provider();
            let client = reqwest::Client::builder()
                .timeout(timeout)
                .pool_max_idle_per_host(32)
                .build()?;
            Ok(HttpsTransport { client, runtime })
        }
    }

    impl Transport for HttpsTransport {
        fn send(&self, call: &HttpCall) -> Result<HttpReply, String> {
            let method = reqwest::Method::from_bytes(call.method.as_str().as_bytes())
                .map_err(|e| e.to_string())?;
            let mut request = self.client.request(method, &call.url);
            for (name, value) in &call.headers {
                request = request.header(name, value);
            }
            if !call.content_type.is_empty() {
                request = request.header("content-type", &call.content_type);
            }
            if !call.body.is_empty() {
                request = request.body(call.body.clone());
            }
            self.runtime.block_on(async move {
                let response = request.send().await.map_err(|e| e.to_string())?;
                let status = response.status().as_u16();
                let headers = response
                    .headers()
                    .iter()
                    .map(|(k, v)| (k.as_str().to_string(), v.to_str().unwrap_or("").to_string()))
                    .collect();
                let body = response.bytes().await.map_err(|e| e.to_string())?.to_vec();
                Ok::<HttpReply, String>(reply_of(status, headers, body))
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        io::{BufRead, BufReader, Write},
        net::TcpListener,
        sync::mpsc,
    };

    use azul_storage::{transport::is_dns_failure, HttpCall, Method, Transport};

    use super::{https::HttpsTransport, HTTPS_TIMEOUT};

    /// An HTTP server on this computer answering `ok` to one request; the request's head lines.
    fn server() -> (u16, mpsc::Receiver<Vec<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let Ok((mut stream, _)) = listener.accept() else {
                return;
            };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut head = Vec::new();
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                    break;
                }
                head.push(line.trim_end().to_string());
            }
            let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok");
            let _ = tx.send(head);
        });
        (port, rx)
    }

    /// DNS down: a node whose name does not resolve is reached at the addresses the failover
    /// hands the transport (reqwest's resolve_to_addrs), the request still naming the host.
    #[test]
    fn a_name_that_does_not_resolve_is_reached_at_its_fallback_address_under_its_name() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let transport = HttpsTransport::new(runtime.handle().clone(), HTTPS_TIMEOUT).unwrap();
        let (port, heads) = server();
        let call = HttpCall {
            method: Method::Get,
            url: format!("http://n2.azlin-dns-test.invalid:{port}/d-1/a.txt"),
            headers: Vec::new(),
            body: Vec::new(),
            content_type: String::new(),
        };
        let why = transport.send(&call).unwrap_err();
        assert!(is_dns_failure(&why), "{why}");
        assert!(!transport.fallback_addresses("n2.azlin-dns-test.invalid", &[String::from("no")]));
        assert!(transport.fallback_addresses(
            "n2.azlin-dns-test.invalid",
            &[String::from("127.0.0.1")]
        ));
        let reply = transport.clone().send(&call).unwrap();
        assert_eq!(reply.status, 200);
        assert_eq!(reply.body, b"ok");
        let head = heads.recv().unwrap();
        assert!(
            head.iter()
                .any(|l| l.eq_ignore_ascii_case(&format!("host: n2.azlin-dns-test.invalid:{port}"))),
            "{head:?}"
        );
    }
}

#[cfg(feature = "iroh")]
pub mod iroh_lane {
    //! S3 over iroh: the kit's `IrohDialer`, through azlin-client's iroh transport (one HTTP/1.1
    //! request and its answer per QUIC stream, to a node reached by its Ed25519 id).

    use anyhow::anyhow;
    use azcloud_kit::transport::{IrohDialer, IrohTarget};
    use azlin_proto::s3req::SignedRequest;

    use super::*;

    /// Dials nodes on the command line's runtime.
    pub struct IrohLane {
        runtime: Handle,
    }

    impl IrohLane {
        #[must_use]
        pub fn new(runtime: Handle) -> IrohLane {
            IrohLane { runtime }
        }
    }

    impl IrohDialer for IrohLane {
        fn dial(
            &self,
            target: &IrohTarget,
            relay: Option<&str>,
        ) -> Result<Box<dyn Transport>, String> {
            let node = iroh_addr(target, relay).map_err(|e| format!("{e:#}"))?;
            let endpoint = self
                .runtime
                .block_on(iroh_endpoint(relay))
                .map_err(|e| format!("{e:#}"))?;
            Ok(Box::new(IrohTransport {
                endpoint,
                node,
                runtime: self.runtime.clone(),
            }))
        }
    }

    /// One endpoint and the node it dials; dropped = the endpoint closed (a goodbye to the
    /// node instead of a timeout on its side).
    struct IrohTransport {
        endpoint: iroh::Endpoint,
        node: iroh::EndpointAddr,
        runtime: Handle,
    }

    impl Transport for IrohTransport {
        fn send(&self, call: &HttpCall) -> Result<HttpReply, String> {
            let mut headers = call.headers.clone();
            if !call.content_type.is_empty() {
                headers.push((String::from("content-type"), call.content_type.clone()));
            }
            let request = SignedRequest {
                method: call.method.as_str().to_string(),
                url: call.url.clone(),
                headers,
                body: call.body.clone(),
            };
            let (status, headers, body) = self
                .runtime
                .block_on(azlin_client::iroh_transport::send(
                    &self.endpoint,
                    self.node.clone(),
                    request,
                ))
                .map_err(|e| format!("{e:#}"))?;
            Ok(reply_of(status, headers, body))
        }
    }

    impl Drop for IrohTransport {
        fn drop(&mut self) {
            let endpoint = self.endpoint.clone();
            let _ = self.runtime.block_on(async move { endpoint.close().await });
        }
    }

    /// The client endpoint with the configured relay: `default` = n0's relays and
    /// discovery (`presets::N0`), an address = that relay only, `off` (or none) =
    /// no relay, direct addresses only.
    async fn iroh_endpoint(relay: Option<&str>) -> anyhow::Result<iroh::Endpoint> {
        use iroh::endpoint::presets;
        let builder = match relay {
            Some("default") => iroh::Endpoint::builder(presets::N0),
            Some("off") | None => {
                iroh::Endpoint::builder(presets::Minimal).relay_mode(iroh::RelayMode::Disabled)
            }
            Some(url) => {
                let url: iroh::RelayUrl = url
                    .parse()
                    .map_err(|e| anyhow!("the relay {url} is no relay address: {e}"))?;
                iroh::Endpoint::builder(presets::Minimal).relay_mode(iroh::RelayMode::custom([url]))
            }
        };
        builder
            .bind()
            .await
            .map_err(|e| anyhow!("the iroh endpoint cannot start: {e}"))
    }

    /// The node's address: its id, its UDP socket when known, and the relay when one is
    /// configured. A node's own relay is its home relay too, and with no address lookup
    /// (anything but `default`) the dial has nothing else to go by: a node behind NAT, or a VM
    /// whose sockets are inside the guest, is only reachable there.
    fn iroh_addr(target: &IrohTarget, relay: Option<&str>) -> anyhow::Result<iroh::EndpointAddr> {
        let id: iroh::EndpointId = target
            .id
            .trim()
            .parse()
            .map_err(|e| anyhow!("{} is no iroh endpoint id: {e}", target.id))?;
        let mut addr = iroh::EndpointAddr::new(id);
        if let Some(socket) = target.addr.as_deref() {
            let socket: std::net::SocketAddr = socket
                .parse()
                .map_err(|e| anyhow!("{socket} is no ip:port: {e}"))?;
            addr = addr.with_ip_addr(socket);
        }
        if let Some(url) = relay.filter(|r| !matches!(*r, "off" | "default")) {
            let url: iroh::RelayUrl = url
                .parse()
                .map_err(|e| anyhow!("the relay {url} is no relay address: {e}"))?;
            addr = addr.with_relay_url(url);
        }
        Ok(addr)
    }
}
