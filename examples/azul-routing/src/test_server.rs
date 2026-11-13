//! A small HTTP server on a local port for the tests of the online sources: it answers every
//! request with what its responder says and keeps every request it read.

use std::{
    io::{BufRead, BufReader, Read, Write},
    net::TcpListener,
    sync::{Arc, Mutex},
};

/// One request the server read.
#[derive(Debug, Clone, Default)]
pub struct Seen {
    pub method: String,
    /// The path with its query, as sent (`/api/v6/plan?fromPlace=...`).
    pub target: String,
    /// The headers, their names in lower case.
    pub headers: Vec<(String, String)>,
    pub body: String,
}

impl Seen {
    /// The value of header `name` (lower case).
    #[must_use]
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(n, _)| n == name).map(|(_, v)| v.as_str())
    }

    /// The decoded value of query parameter `name`.
    #[must_use]
    pub fn query(&self, name: &str) -> Option<String> {
        let (_, query) = self.target.split_once('?')?;
        query.split('&').find_map(|pair| {
            let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
            (decode(k) == name).then(|| decode(v))
        })
    }
}

/// `%2C` and `+` back to what they encode.
fn decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("");
                match u8::from_str_radix(hex, 16) {
                    Ok(b) => {
                        out.push(b);
                        i += 2;
                    }
                    Err(_) => out.push(b'%'),
                }
            }
            b => out.push(b),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// What the server answers: a status, extra headers, a body.
pub type Answer = (u16, Vec<(String, String)>, String);

/// A running server.
pub struct Server {
    /// `http://127.0.0.1:<port>`.
    pub base: String,
    /// Every request it read, in order.
    pub seen: Arc<Mutex<Vec<Seen>>>,
}

impl Server {
    /// Starts a server that answers with `respond`.
    pub fn start(respond: impl Fn(&Seen) -> Answer + Send + 'static) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a port");
        let base = format!("http://{}", listener.local_addr().expect("an address"));
        let seen = Arc::new(Mutex::new(Vec::new()));
        let log = seen.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let mut reader = BufReader::new(stream.try_clone().expect("a stream"));
                let mut first = String::new();
                if reader.read_line(&mut first).unwrap_or(0) == 0 {
                    continue;
                }
                let mut parts = first.split_whitespace();
                let mut request = Seen {
                    method: parts.next().unwrap_or("").to_string(),
                    target: parts.next().unwrap_or("").to_string(),
                    ..Seen::default()
                };
                let mut length = 0usize;
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                        break;
                    }
                    if let Some((name, value)) = line.trim_end().split_once(':') {
                        let name = name.trim().to_ascii_lowercase();
                        let value = value.trim().to_string();
                        if name == "content-length" {
                            length = value.parse().unwrap_or(0);
                        }
                        request.headers.push((name, value));
                    }
                }
                let mut body = vec![0u8; length];
                let _ = reader.read_exact(&mut body);
                request.body = String::from_utf8_lossy(&body).into_owned();
                let (status, headers, answer) = respond(&request);
                log.lock().expect("the log").push(request);
                let mut stream = stream;
                let mut head = format!(
                    "HTTP/1.1 {status} {}\r\nContent-Length: {}\r\nConnection: close\r\n",
                    if status < 400 { "OK" } else { "Error" },
                    answer.len()
                );
                for (name, value) in headers {
                    head.push_str(&format!("{name}: {value}\r\n"));
                }
                head.push_str("\r\n");
                let _ = stream.write_all(head.as_bytes());
                let _ = stream.write_all(answer.as_bytes());
            }
        });
        Self { base, seen }
    }

    /// How many requests it read.
    #[must_use]
    pub fn count(&self) -> usize {
        self.seen.lock().map_or(0, |s| s.len())
    }

    /// The requests it read.
    #[must_use]
    pub fn requests(&self) -> Vec<Seen> {
        self.seen.lock().map(|s| s.clone()).unwrap_or_default()
    }
}

/// The TCP port [`closed_port`] names: 1 (`tcpmux`).
pub const CLOSED_PORT: u16 = 1;

/// A URL nothing listens on and nothing ever can in a test run.
///
/// X-TESTFIX3-2: this was a freed listener - the port a `bind("127.0.0.1:0")` was given and gave back -
/// and any program another test starts (every test server of a run binds port 0 for its own) can be
/// given the very port before this test connects: the "connection refused" of an offline test became a
/// call to a stranger's server. The port is now [`CLOSED_PORT`]: below every range the kernel gives a
/// port-0 bind (the ephemeral ports start at 32768, or 49152 on macOS), a port no ordinary user may
/// bind, and a port nothing on a machine serves. The kernel answers a connect to it with a reset, as it
/// did for a freed port: the same "connection refused" every caller read before.
///
/// A listener kept bound and never accepted was the other answer, and it refuses nothing: the kernel
/// completes the handshake into the listener's backlog, so the call connects and then waits for an
/// answer that never comes - an offline test would wait for its timeout instead of failing at once.
#[must_use]
pub fn closed_port() -> String {
    format!("http://127.0.0.1:{CLOSED_PORT}")
}

#[cfg(test)]
mod tests {
    use std::{
        io::ErrorKind,
        net::{TcpListener, TcpStream},
    };

    use super::*;

    /// X-TESTFIX3-2. Ten calls through the helper, while other threads run servers on the ports a port-0
    /// bind gives (as every test of a run does): each connect is refused, and no server is reached. RED
    /// on the freed listener only by luck (the port was given to another server in a window of
    /// milliseconds); the next test is the deterministic one.
    #[test]
    fn x_testfix3_2_ten_calls_through_closed_port_are_all_refused_and_reach_no_server() {
        let servers: Vec<Server> = (0..64)
            .map(|_| Server::start(|_| (200, Vec::new(), String::from("a stranger"))))
            .collect();
        for call in 0..10 {
            let url = closed_port();
            let address = url.strip_prefix("http://").expect("an http URL");
            let refused = TcpStream::connect(address)
                .expect_err("nothing listens on the closed port: the connect is refused");
            assert_eq!(
                refused.kind(),
                ErrorKind::ConnectionRefused,
                "call {call}: {refused}"
            );
        }
        // Give a server that was reached time to log it: a stranger's server answers at once.
        std::thread::sleep(std::time::Duration::from_millis(200));
        let reached: usize = servers.iter().map(Server::count).sum();
        assert_eq!(reached, 0, "a call through closed_port reached a server");
    }

    /// X-TESTFIX3-2, the property that makes the port safe and that the freed listener did not have: it
    /// is below every port a bind of port 0 is ever given, so no program a test starts can take it
    /// (RED on the freed listener: its port was one a port-0 bind had just been given).
    #[test]
    fn x_testfix3_2_the_closed_port_is_below_every_port_a_bind_of_port_zero_is_given() {
        let url = closed_port();
        let port: u16 = url
            .rsplit(':')
            .next()
            .and_then(|p| p.parse().ok())
            .expect("a port");
        let given: Vec<u16> = (0..32)
            .map(|_| {
                TcpListener::bind("127.0.0.1:0")
                    .and_then(|l| l.local_addr())
                    .map(|a| a.port())
                    .expect("a port-0 bind is given one")
            })
            .collect();
        assert!(
            given.iter().all(|g| *g > 1024) && port < 1024,
            "the closed port {port} is in the range a port-0 bind is given: {given:?}"
        );
    }
}
