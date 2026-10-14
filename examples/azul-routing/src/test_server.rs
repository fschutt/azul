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

/// A URL nothing listens on.
#[must_use]
pub fn closed_port() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a port");
    let base = format!("http://{}", listener.local_addr().expect("an address"));
    drop(listener);
    base
}
