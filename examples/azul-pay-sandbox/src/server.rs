//! The sandbox on a loopback socket: a small HTTP/1.1 server (std only, one short connection per
//! request, bounded in size, time and number) in front of [`Core`], and a thread that posts the
//! due webhooks to the backend on this computer and reports what it answered.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, Shutdown, SocketAddr, TcpListener, TcpStream, ToSocketAddrs};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::Duration;

use crate::engine::{Core, Delivery, Request, Response};
use crate::{guard, util, Config, Error};

/// The longest request head and body the sandbox reads.
const MAX_HEAD: usize = 16 * 1024;
const MAX_BODY: usize = 256 * 1024;
/// The most connections served at once; more wait in the kernel's queue.
const MAX_CONNECTIONS: usize = 32;
/// How long a connection may take to send its request or read its answer.
const IO_TIMEOUT: Duration = Duration::from_secs(10);
/// How often the webhook thread looks for due webhooks, and how long it waits for the backend.
const TICK: Duration = Duration::from_millis(250);
const POST_TIMEOUT: Duration = Duration::from_secs(10);

/// A running sandbox. Dropping it leaves the threads running; [`Running::stop`] ends them.
pub struct Running {
    base_url: String,
    stop: Arc<AtomicBool>,
    threads: Vec<JoinHandle<()>>,
}

impl Running {
    /// `http://127.0.0.1:<port>`.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// What replaces the provider's API base URL in the backend (`<base>/stripe`).
    pub fn api_url(&self, kind: crate::Kind) -> String {
        format!("{}{}", self.base_url, kind.prefix())
    }

    /// Stops serving and posting, and waits for both threads.
    pub fn stop(self) {
        self.stop.store(true, Ordering::SeqCst);
        for t in self.threads {
            // a thread that panicked has nothing left to stop
            let _ = t.join();
        }
    }

    /// Blocks until the sandbox is stopped (the binary's main).
    pub fn wait(self) {
        for t in self.threads {
            let _ = t.join();
        }
    }
}

fn lock(core: &Mutex<Core>) -> MutexGuard<'_, Core> {
    // a panic in one request leaves the state as it was between requests: go on with it
    core.lock().unwrap_or_else(|p| p.into_inner())
}

/// Starts the sandbox on 127.0.0.1 (`cfg.port`; 0: any free port). Refused in a release build
/// without the local test stack's opt-in, and for a webhook URL off this computer.
pub fn start(cfg: Config) -> Result<Running, Error> {
    start_as(cfg, cfg!(debug_assertions))
}

/// [`start`] in a build with (`debug`) or without debug assertions.
pub(crate) fn start_as(cfg: Config, debug: bool) -> Result<Running, Error> {
    guard::refuse_in_release(debug, cfg.local_test_stack)?;
    guard::loopback_webhook_url(&cfg.webhook_url)?;
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, cfg.port)).map_err(|e| Error::Bind(e.to_string()))?;
    let port = listener.local_addr().map_err(|e| Error::Bind(e.to_string()))?.port();
    // polled, so a stop is noticed without a connection
    listener.set_nonblocking(true).map_err(|e| Error::Bind(e.to_string()))?;
    let base_url = format!("http://127.0.0.1:{port}");
    let core = Arc::new(Mutex::new(Core::new(&cfg, &base_url)?));
    let stop = Arc::new(AtomicBool::new(false));
    let serve = {
        let (core, stop) = (core.clone(), stop.clone());
        std::thread::spawn(move || accept_loop(&listener, &core, &stop))
    };
    let post = {
        let (core, stop) = (core, stop.clone());
        std::thread::spawn(move || webhook_loop(&core, &stop))
    };
    Ok(Running {
        base_url,
        stop,
        threads: vec![serve, post],
    })
}

fn accept_loop(listener: &TcpListener, core: &Arc<Mutex<Core>>, stop: &AtomicBool) {
    let active = Arc::new(AtomicUsize::new(0));
    while !stop.load(Ordering::SeqCst) {
        match listener.accept() {
            Ok((stream, peer)) => {
                // the socket is bound to 127.0.0.1, so a peer is always this computer; checked anyway
                if !peer.ip().is_loopback() || active.load(Ordering::SeqCst) >= MAX_CONNECTIONS {
                    let _ = stream.shutdown(Shutdown::Both);
                    continue;
                }
                active.fetch_add(1, Ordering::SeqCst);
                let (core, active) = (core.clone(), active.clone());
                std::thread::spawn(move || {
                    serve(stream, &core);
                    active.fetch_sub(1, Ordering::SeqCst);
                });
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => std::thread::sleep(Duration::from_millis(20)),
            Err(e) => {
                eprintln!("azul-pay-sandbox: accept: {e}");
                std::thread::sleep(Duration::from_millis(200));
            }
        }
    }
}

/// One request on one connection, then the connection closes.
fn serve(mut stream: TcpStream, core: &Mutex<Core>) {
    if stream.set_nonblocking(false).is_err()
        || stream.set_read_timeout(Some(IO_TIMEOUT)).is_err()
        || stream.set_write_timeout(Some(IO_TIMEOUT)).is_err()
    {
        return;
    }
    let response = match read_request(&mut stream) {
        Ok((method, target, headers, body)) => {
            let (path, query) = match target.split_once('?') {
                Some((p, q)) => (p.to_string(), Some(q.to_string())),
                None => (target, None),
            };
            let req = Request {
                method: &method,
                path: &path,
                query: query.as_deref(),
                headers: &headers,
                body: &body,
            };
            lock(core).handle(&req, util::now())
        }
        Err(status) => {
            let mut r = Response::json(status, serde_json::json!({"error": "bad_request", "message": "the request is not one this sandbox reads"}));
            r.headers.extend(crate::kit::security_headers());
            r
        }
    };
    if response.hang_secs > 0 {
        // the connection stays open and silent, outside the core's lock
        std::thread::sleep(Duration::from_secs(response.hang_secs));
        let _ = stream.shutdown(Shutdown::Both);
        return;
    }
    let _ = write_response(&mut stream, &response);
    let _ = stream.shutdown(Shutdown::Both);
}

type Head = (String, String, Vec<(String, String)>, Vec<u8>);

/// The method, target, headers and body; an error status for what it does not read (too big,
/// chunked, malformed).
fn read_request(stream: &mut TcpStream) -> Result<Head, u16> {
    let mut reader = BufReader::new(stream.take((MAX_HEAD + MAX_BODY) as u64));
    let mut line = String::new();
    let mut head_len = 0;
    let mut read_line = |reader: &mut BufReader<_>, line: &mut String| -> Result<(), u16> {
        line.clear();
        let n = reader.read_line(line).map_err(|_| 400u16)?;
        head_len += n;
        if n == 0 || head_len > MAX_HEAD {
            return Err(if n == 0 { 400 } else { 431 });
        }
        Ok(())
    };
    read_line(&mut reader, &mut line)?;
    let mut parts = line.split_whitespace();
    let (Some(method), Some(target), Some(version)) = (parts.next(), parts.next(), parts.next()) else {
        return Err(400);
    };
    if !version.starts_with("HTTP/1.") || !target.starts_with('/') {
        return Err(400);
    }
    let (method, target) = (method.to_string(), target.to_string());
    let mut headers = Vec::new();
    loop {
        read_line(&mut reader, &mut line)?;
        let l = line.trim_end_matches(['\r', '\n']);
        if l.is_empty() {
            break;
        }
        let Some((k, v)) = l.split_once(':') else {
            return Err(400);
        };
        if headers.len() >= 100 {
            return Err(431);
        }
        headers.push((k.trim().to_string(), v.trim().to_string()));
    }
    let header = |name: &str| headers.iter().find(|(k, _): &&(String, String)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.clone());
    if header("transfer-encoding").is_some() {
        return Err(411);
    }
    let len = match header("content-length") {
        Some(n) => n.parse::<usize>().map_err(|_| 400u16)?,
        None => 0,
    };
    if len > MAX_BODY {
        return Err(413);
    }
    let mut body = vec![0u8; len];
    reader.read_exact(&mut body).map_err(|_| 400u16)?;
    Ok((method, target, headers, body))
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        201 => "Created",
        204 => "No Content",
        303 => "See Other",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        409 => "Conflict",
        411 => "Length Required",
        413 => "Content Too Large",
        422 => "Unprocessable Content",
        431 => "Request Header Fields Too Large",
        _ => "Status",
    }
}

fn write_response(stream: &mut TcpStream, r: &Response) -> std::io::Result<()> {
    let mut head = format!("HTTP/1.1 {} {}\r\n", r.status, reason(r.status));
    for (k, v) in &r.headers {
        // a header value never carries a line break (they are ours, but a URL may be echoed)
        if !v.contains(['\r', '\n']) {
            head.push_str(&format!("{k}: {v}\r\n"));
        }
    }
    head.push_str(&format!("Content-Length: {}\r\nConnection: close\r\n\r\n", r.body.len()));
    stream.write_all(head.as_bytes())?;
    stream.write_all(&r.body)?;
    stream.flush()
}

fn webhook_loop(core: &Mutex<Core>, stop: &AtomicBool) {
    while !stop.load(Ordering::SeqCst) {
        std::thread::sleep(TICK);
        let due = lock(core).tick(util::now());
        for d in due {
            let status = post(&d);
            lock(core).delivered(d, status, util::now());
        }
    }
}

/// Posts one webhook to the backend on this computer: the status it answered, none without one.
fn post(d: &Delivery) -> Option<u16> {
    let url = guard::loopback_webhook_url(&d.url).ok()?;
    let host = url.host.trim_start_matches('[').trim_end_matches(']');
    let addr: SocketAddr = (host, url.port).to_socket_addrs().ok()?.find(|a| a.ip().is_loopback())?;
    let mut stream = TcpStream::connect_timeout(&addr, POST_TIMEOUT).ok()?;
    stream.set_read_timeout(Some(POST_TIMEOUT)).ok()?;
    stream.set_write_timeout(Some(POST_TIMEOUT)).ok()?;
    let mut head = format!("POST {} HTTP/1.1\r\nHost: {}:{}\r\nUser-Agent: azul-pay-sandbox\r\n", url.path, url.host, url.port);
    for (k, v) in &d.headers {
        head.push_str(&format!("{k}: {v}\r\n"));
    }
    head.push_str(&format!("Content-Length: {}\r\nConnection: close\r\n\r\n", d.body.len()));
    stream.write_all(head.as_bytes()).ok()?;
    stream.write_all(&d.body).ok()?;
    let mut status_line = String::new();
    BufReader::new(stream.take(1024)).read_line(&mut status_line).ok()?;
    status_line.split_whitespace().nth(1)?.parse::<u16>().ok()
}
