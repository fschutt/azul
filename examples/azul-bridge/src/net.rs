//! The sockets: every server listens on 127.0.0.1 only, drops a peer that is not this computer
//! (a socket that somehow got exposed, a forwarded port), holds at most so many connections,
//! runs each connection on a thread of its own, and reads lines and bytes with hard limits and
//! timeouts ([`Input`]). [`RateLimiter`] slows a connection that sends commands faster than a
//! person's mail program ever does.

use std::{
    io::{self, Read, Write},
    net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener, TcpStream},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

/// A connection a server talks over: a socket, or a test's stand-in.
pub trait Conn: Read + Write + Send {
    /// Reads give up after `timeout` (`None`: never) with a `WouldBlock` / `TimedOut` error.
    ///
    /// # Errors
    ///
    /// The socket's.
    fn set_read_timeout(&self, timeout: Option<Duration>) -> io::Result<()>;
}

impl Conn for TcpStream {
    fn set_read_timeout(&self, timeout: Option<Duration>) -> io::Result<()> {
        TcpStream::set_read_timeout(self, timeout)
    }
}

/// Whether `addr` is this computer: 127.0.0.0/8, `::1`, or `::ffff:127.x.y.z`.
#[must_use]
pub fn is_loopback(addr: &SocketAddr) -> bool {
    match addr.ip() {
        IpAddr::V4(ip) => ip.is_loopback(),
        IpAddr::V6(ip) => {
            ip.is_loopback() || ip.to_ipv4_mapped().is_some_and(|v4| v4.is_loopback())
        }
    }
}

/// A listener on 127.0.0.1 at `port` (0: any free port, as the tests take).
///
/// # Errors
///
/// When the port is taken or not allowed.
pub fn bind_loopback(port: u16) -> io::Result<TcpListener> {
    TcpListener::bind((Ipv4Addr::LOCALHOST, port))
}

/// Takes one of `max` places while it lives.
struct Place(Arc<AtomicUsize>);

impl Drop for Place {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

/// What a server does with a connection.
pub type Handler = Arc<dyn Fn(TcpStream) + Send + Sync>;
/// What a server says to a connection it has no place for, before closing it.
pub type Busy = fn(&mut TcpStream);

/// Accepts connections until the listener fails, each on a thread of its own. A peer that is
/// not this computer is closed unanswered; past `max` connections `busy` answers and closes.
pub fn serve(listener: TcpListener, max: usize, handler: Handler, busy: Busy) {
    let active = Arc::new(AtomicUsize::new(0));
    for stream in listener.incoming() {
        let Ok(mut stream) = stream else {
            // A connection that failed before it was accepted (reset): the next one.
            continue;
        };
        let local = stream.peer_addr().map(|peer| is_loopback(&peer)).unwrap_or(false);
        if !local {
            let _ = stream.shutdown(std::net::Shutdown::Both);
            continue;
        }
        if active.fetch_add(1, Ordering::SeqCst) >= max {
            active.fetch_sub(1, Ordering::SeqCst);
            let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
            busy(&mut stream);
            let _ = stream.shutdown(std::net::Shutdown::Both);
            continue;
        }
        let place = Place(active.clone());
        let handler = handler.clone();
        let spawned = std::thread::Builder::new()
            .name(String::from("azul-bridge connection"))
            .spawn(move || {
                let _place = place;
                let _ = stream.set_nodelay(true);
                // A client that stops reading must not hold the thread forever.
                let _ = stream.set_write_timeout(Some(Duration::from_secs(120)));
                handler(stream);
            });
        if spawned.is_err() {
            // No thread: the place was dropped with the closure, the client sees the close.
            continue;
        }
    }
}

/// Why a read gave nothing.
#[derive(Debug)]
pub enum ReadError {
    /// The client closed the connection.
    Closed,
    /// A line longer than allowed; the connection cannot be read in step any more.
    TooLong,
    /// Nothing came within the read timeout (what was read so far is kept).
    Timeout,
    Io(io::Error),
}

impl std::fmt::Display for ReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReadError::Closed => write!(f, "the connection was closed"),
            ReadError::TooLong => write!(f, "a line was too long"),
            ReadError::Timeout => write!(f, "the client said nothing for too long"),
            ReadError::Io(e) => write!(f, "{e}"),
        }
    }
}

/// What has been read from a connection and not taken yet: lines (CRLF, or a bare LF, ends
/// one) and runs of bytes (a literal, a body), each with a limit. A timeout keeps what came.
#[derive(Debug, Default)]
pub struct Input {
    buf: Vec<u8>,
    pos: usize,
}

impl Input {
    #[must_use]
    pub fn new() -> Input {
        Input::default()
    }

    /// Whether bytes wait that the client sent ahead (pipelining).
    #[must_use]
    pub fn has_buffered(&self) -> bool {
        self.pos < self.buf.len()
    }

    fn compact(&mut self) {
        if self.pos == self.buf.len() {
            self.buf.clear();
            self.pos = 0;
        } else if self.pos > 64 * 1024 {
            self.buf.drain(..self.pos);
            self.pos = 0;
        }
    }

    /// Reads once more from `reader`.
    fn fill<R: Read + ?Sized>(&mut self, reader: &mut R) -> Result<(), ReadError> {
        self.compact();
        let mut chunk = [0u8; 16 * 1024];
        loop {
            match reader.read(&mut chunk) {
                Ok(0) => return Err(ReadError::Closed),
                Ok(n) => {
                    self.buf.extend_from_slice(&chunk[..n]);
                    return Ok(());
                }
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e)
                    if matches!(
                        e.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                    ) =>
                {
                    return Err(ReadError::Timeout)
                }
                Err(e) => return Err(ReadError::Io(e)),
            }
        }
    }

    /// The next line without its line end, at most `max` bytes.
    ///
    /// # Errors
    ///
    /// [`ReadError`]: closed, too long, a timeout (nothing is lost), an I/O error.
    pub fn read_line<R: Read + ?Sized>(
        &mut self,
        reader: &mut R,
        max: usize,
    ) -> Result<Vec<u8>, ReadError> {
        let mut searched = self.pos;
        loop {
            if let Some(at) = self.buf[searched..].iter().position(|b| *b == b'\n') {
                let end = searched + at;
                let mut line = self.buf[self.pos..end].to_vec();
                if line.last() == Some(&b'\r') {
                    line.pop();
                }
                self.pos = end + 1;
                if line.len() > max {
                    return Err(ReadError::TooLong);
                }
                return Ok(line);
            }
            if self.buf.len() - self.pos > max + 2 {
                return Err(ReadError::TooLong);
            }
            searched = self.buf.len();
            let before = self.pos;
            self.fill(reader)?;
            // `fill` may have moved what is buffered to the front.
            searched = searched - before + self.pos;
        }
    }

    /// The next `n` bytes exactly.
    ///
    /// # Errors
    ///
    /// [`ReadError`]: closed before `n` bytes came, a timeout, an I/O error.
    pub fn read_bytes<R: Read + ?Sized>(
        &mut self,
        reader: &mut R,
        n: usize,
    ) -> Result<Vec<u8>, ReadError> {
        while self.buf.len() - self.pos < n {
            self.fill(reader)?;
        }
        let out = self.buf[self.pos..self.pos + n].to_vec();
        self.pos += n;
        Ok(out)
    }
}

/// A token bucket: `per_second` on average, `burst` at once; [`RateLimiter::take`] waits for a
/// token instead of refusing, so a fast client is slowed, not cut off.
#[derive(Debug)]
pub struct RateLimiter {
    rate: f64,
    burst: f64,
    tokens: f64,
    last: Instant,
}

impl RateLimiter {
    #[must_use]
    pub fn new(per_second: u32, burst: u32) -> RateLimiter {
        let burst = f64::from(burst.max(1));
        RateLimiter {
            rate: f64::from(per_second.max(1)),
            burst,
            tokens: burst,
            last: Instant::now(),
        }
    }

    /// How long the next token is away (zero: there is one).
    #[must_use]
    pub fn wait_needed(&mut self) -> Duration {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last).as_secs_f64();
        self.last = now;
        self.tokens = (self.tokens + elapsed * self.rate).min(self.burst);
        if self.tokens >= 1.0 {
            Duration::ZERO
        } else {
            Duration::from_secs_f64((1.0 - self.tokens) / self.rate)
        }
    }

    /// Takes one token, waiting for it when the bucket is empty.
    pub fn take(&mut self) {
        let wait = self.wait_needed();
        if !wait.is_zero() {
            std::thread::sleep(wait);
            self.tokens = 1.0;
            self.last = Instant::now();
        }
        self.tokens -= 1.0;
    }
}

/// Whether a first line is an HTTP request: a browser page that points a request at the IMAP
/// or SMTP port (a cross-protocol attack) is closed before a second line is read.
#[must_use]
pub fn looks_like_http(line: &[u8]) -> bool {
    const METHODS: [&[u8]; 9] = [
        b"GET ", b"POST ", b"PUT ", b"HEAD ", b"OPTIONS ", b"DELETE ", b"PATCH ", b"CONNECT ",
        b"TRACE ",
    ];
    let upper: Vec<u8> = line.iter().map(u8::to_ascii_uppercase).collect();
    let is_request = METHODS.iter().any(|m| upper.starts_with(m))
        && upper.windows(6).any(|w| w == b" HTTP/");
    is_request || upper.starts_with(b"HOST:") || upper.starts_with(b"USER-AGENT:")
}

/// Writes `bytes` and flushes.
///
/// # Errors
///
/// The socket's.
pub fn send<W: Write + ?Sized>(writer: &mut W, bytes: &[u8]) -> io::Result<()> {
    writer.write_all(bytes)?;
    writer.flush()
}

#[cfg(test)]
mod tests {
    use std::net::{Ipv6Addr, SocketAddrV6};

    use super::*;

    /// A reader that hands out its chunks one per read, then times out, then is closed.
    struct Chunks(Vec<Vec<u8>>, bool);

    impl Read for Chunks {
        fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
            if self.0.is_empty() {
                if self.1 {
                    self.1 = false;
                    return Err(io::Error::new(io::ErrorKind::WouldBlock, "later"));
                }
                return Ok(0);
            }
            let chunk = self.0.remove(0);
            out[..chunk.len()].copy_from_slice(&chunk);
            Ok(chunk.len())
        }
    }

    fn chunks(parts: &[&[u8]]) -> Chunks {
        Chunks(parts.iter().map(|p| p.to_vec()).collect(), true)
    }

    #[test]
    fn only_this_computer_is_loopback() {
        let v4 = |a, b, c, d| SocketAddr::from((Ipv4Addr::new(a, b, c, d), 1));
        assert!(is_loopback(&v4(127, 0, 0, 1)));
        assert!(is_loopback(&v4(127, 1, 2, 3)));
        assert!(!is_loopback(&v4(192, 168, 1, 2)));
        assert!(!is_loopback(&v4(0, 0, 0, 0)));
        let v6 = |ip: Ipv6Addr| SocketAddr::V6(SocketAddrV6::new(ip, 1, 0, 0));
        assert!(is_loopback(&v6(Ipv6Addr::LOCALHOST)));
        assert!(is_loopback(&v6("::ffff:127.0.0.1".parse().unwrap())));
        assert!(!is_loopback(&v6("::ffff:10.0.0.1".parse().unwrap())));
        assert!(!is_loopback(&v6("fe80::1".parse().unwrap())));
    }

    #[test]
    fn lines_end_at_crlf_or_lf_across_reads_and_a_timeout_loses_nothing() {
        let mut reader = chunks(&[b"A1 NO", b"OP\r\nA2 LOG", b"OUT\nrest"]);
        let mut input = Input::new();
        assert_eq!(input.read_line(&mut reader, 100).unwrap(), b"A1 NOOP");
        assert_eq!(input.read_line(&mut reader, 100).unwrap(), b"A2 LOGOUT");
        assert!(matches!(
            input.read_line(&mut reader, 100),
            Err(ReadError::Timeout)
        ));
        assert!(input.has_buffered(), "\"rest\" waits for its line end");
        assert!(matches!(
            input.read_line(&mut reader, 100),
            Err(ReadError::Closed)
        ));
    }

    #[test]
    fn a_line_over_the_limit_is_refused_even_before_its_end_comes() {
        let mut input = Input::new();
        let long = vec![b'x'; 50];
        assert!(matches!(
            input.read_line(&mut chunks(&[&long, b"\r\n"]), 10),
            Err(ReadError::TooLong)
        ));
        let mut input = Input::new();
        let mut endless = Chunks(vec![vec![b'y'; 4096]; 64], false);
        assert!(matches!(
            input.read_line(&mut endless, 1000),
            Err(ReadError::TooLong)
        ));
    }

    #[test]
    fn bytes_are_taken_exactly_and_the_rest_stays_for_the_next_line() {
        let mut reader = chunks(&[b"{5}\r\nhel", b"lo)\r\n"]);
        let mut input = Input::new();
        assert_eq!(input.read_line(&mut reader, 100).unwrap(), b"{5}");
        assert_eq!(input.read_bytes(&mut reader, 5).unwrap(), b"hello");
        assert_eq!(input.read_line(&mut reader, 100).unwrap(), b")");
    }

    #[test]
    fn the_rate_limiter_lets_a_burst_through_then_paces() {
        // Ten a second: one token is 100 ms away, far more than this test takes between calls.
        let mut limiter = RateLimiter::new(10, 3);
        for _ in 0..3 {
            assert_eq!(limiter.wait_needed(), Duration::ZERO);
            limiter.take();
        }
        assert!(limiter.wait_needed() > Duration::ZERO);
        let started = Instant::now();
        limiter.take();
        let waited = started.elapsed();
        assert!(waited > Duration::from_millis(20) && waited < Duration::from_secs(2), "{waited:?}");
    }

    #[test]
    fn http_request_lines_are_recognised_on_the_mail_ports() {
        assert!(looks_like_http(b"POST /x HTTP/1.1"));
        assert!(looks_like_http(b"get / http/1.0"));
        assert!(looks_like_http(b"Host: 127.0.0.1:1143"));
        assert!(!looks_like_http(b"A1 LOGIN ada pw"));
        assert!(!looks_like_http(b"EHLO localhost"));
        assert!(!looks_like_http(b"POST mail please"));
    }

    #[test]
    fn a_server_answers_this_computer_and_turns_away_the_one_too_many() {
        let listener = bind_loopback(0).unwrap();
        let addr = listener.local_addr().unwrap();
        assert!(addr.ip().is_loopback());
        let handler: Handler = Arc::new(|mut stream: TcpStream| {
            let _ = send(&mut stream, b"hello\r\n");
            let mut sink = [0u8; 16];
            let _ = stream.read(&mut sink);
        });
        fn busy(stream: &mut TcpStream) {
            let _ = send(stream, b"busy\r\n");
        }
        std::thread::spawn(move || serve(listener, 1, handler, busy));
        let read_first = |stream: &mut TcpStream| {
            let mut input = Input::new();
            input.read_line(stream, 100).unwrap()
        };
        let mut first = TcpStream::connect(addr).unwrap();
        assert_eq!(read_first(&mut first), b"hello");
        let mut second = TcpStream::connect(addr).unwrap();
        assert_eq!(read_first(&mut second), b"busy");
    }
}
