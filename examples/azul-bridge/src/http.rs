//! HTTP/1.1 for WebDAV, strict and small: a request's head (line and headers, limited in
//! size and count), its body by Content-Length or chunked (limited; both at once, or a
//! transfer coding other than chunked, is refused), `Expect: 100-continue`, kept-alive
//! connections, and responses with a length.

use std::io::{Read, Write};

use crate::net::{Input, ReadError};

/// A request's line and headers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Head {
    pub method: String,
    /// The target as sent (origin form `/a/b?q`, or absolute form `http://host/a/b`).
    pub target: String,
    /// `HTTP/1.1` is `(1, 1)`.
    pub version: (u8, u8),
    pub headers: Vec<(String, String)>,
}

impl Head {
    /// The first header `name` (any case), trimmed.
    #[must_use]
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.trim())
    }

    /// Whether the connection stays open after the answer.
    #[must_use]
    pub fn keep_alive(&self) -> bool {
        let connection = self.header("Connection").unwrap_or_default().to_ascii_lowercase();
        if self.version >= (1, 1) {
            !connection.split(',').any(|t| t.trim() == "close")
        } else {
            connection.split(',').any(|t| t.trim() == "keep-alive")
        }
    }

    /// How the body comes.
    ///
    /// # Errors
    ///
    /// The status to refuse the request with.
    pub fn framing(&self) -> Result<Framing, Status> {
        let chunked = match self.header("Transfer-Encoding") {
            None => false,
            Some(coding) if coding.eq_ignore_ascii_case("chunked") => true,
            Some(_) => return Err(Status::NOT_IMPLEMENTED),
        };
        let lengths: Vec<&str> = self
            .headers
            .iter()
            .filter(|(n, _)| n.eq_ignore_ascii_case("Content-Length"))
            .map(|(_, v)| v.trim())
            .collect();
        if chunked {
            if !lengths.is_empty() {
                return Err(Status::BAD_REQUEST);
            }
            return Ok(Framing::Chunked);
        }
        match lengths.as_slice() {
            [] => Ok(Framing::Length(0)),
            [first, rest @ ..] => {
                if rest.iter().any(|other| other != first)
                    || first.is_empty()
                    || first.len() > 19
                    || !first.bytes().all(|b| b.is_ascii_digit())
                {
                    return Err(Status::BAD_REQUEST);
                }
                first
                    .parse()
                    .map(Framing::Length)
                    .map_err(|_| Status::BAD_REQUEST)
            }
        }
    }
}

/// How a body is sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Framing {
    Length(u64),
    Chunked,
}

/// A status code and its reason phrase.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Status(pub u16, pub &'static str);

impl Status {
    pub const OK: Status = Status(200, "OK");
    pub const CREATED: Status = Status(201, "Created");
    pub const NO_CONTENT: Status = Status(204, "No Content");
    pub const PARTIAL: Status = Status(206, "Partial Content");
    pub const MULTI_STATUS: Status = Status(207, "Multi-Status");
    pub const BAD_REQUEST: Status = Status(400, "Bad Request");
    pub const UNAUTHORIZED: Status = Status(401, "Unauthorized");
    pub const FORBIDDEN: Status = Status(403, "Forbidden");
    pub const NOT_FOUND: Status = Status(404, "Not Found");
    pub const METHOD_NOT_ALLOWED: Status = Status(405, "Method Not Allowed");
    pub const CONFLICT: Status = Status(409, "Conflict");
    pub const PRECONDITION_FAILED: Status = Status(412, "Precondition Failed");
    pub const TOO_LARGE: Status = Status(413, "Content Too Large");
    pub const URI_TOO_LONG: Status = Status(414, "URI Too Long");
    pub const UNSUPPORTED_MEDIA: Status = Status(415, "Unsupported Media Type");
    pub const RANGE_NOT_SATISFIABLE: Status = Status(416, "Range Not Satisfiable");
    pub const LOCKED: Status = Status(423, "Locked");
    pub const HEADERS_TOO_LARGE: Status = Status(431, "Request Header Fields Too Large");
    pub const INTERNAL: Status = Status(500, "Internal Server Error");
    pub const NOT_IMPLEMENTED: Status = Status(501, "Not Implemented");
    pub const BAD_GATEWAY: Status = Status(502, "Bad Gateway");
    pub const INSUFFICIENT_STORAGE: Status = Status(507, "Insufficient Storage");
}

/// Why no request could be read.
#[derive(Debug)]
pub enum HttpError {
    /// The client closed, or went quiet between requests: close without an answer.
    Gone,
    /// Answer with this status and close.
    Refuse(Status),
}

fn read_error(e: ReadError, limit: Status) -> HttpError {
    match e {
        ReadError::TooLong => HttpError::Refuse(limit),
        _ => HttpError::Gone,
    }
}

/// The next request's head.
///
/// # Errors
///
/// [`HttpError`]: gone, or a status to refuse with (a head over `max_bytes` or `max_count`
/// headers, a malformed line, an HTTP version other than 1.x).
pub fn read_head<R: Read + ?Sized>(
    input: &mut Input,
    reader: &mut R,
    max_bytes: usize,
    max_count: usize,
) -> Result<Head, HttpError> {
    // Empty lines before a request are allowed (RFC 9112 2.2).
    let mut line;
    let mut skipped = 0;
    loop {
        line = input
            .read_line(reader, max_bytes)
            .map_err(|e| read_error(e, Status::URI_TOO_LONG))?;
        if !line.is_empty() {
            break;
        }
        skipped += 1;
        if skipped > 4 {
            return Err(HttpError::Refuse(Status::BAD_REQUEST));
        }
    }
    let text = std::str::from_utf8(&line).map_err(|_| HttpError::Refuse(Status::BAD_REQUEST))?;
    let mut parts = text.split(' ');
    let (Some(method), Some(target), Some(version), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Err(HttpError::Refuse(Status::BAD_REQUEST));
    };
    let version = match version {
        "HTTP/1.1" => (1, 1),
        "HTTP/1.0" => (1, 0),
        _ => return Err(HttpError::Refuse(Status(505, "HTTP Version Not Supported"))),
    };
    if method.is_empty()
        || !method.bytes().all(|b| b.is_ascii_uppercase() || b == b'-')
        || target.is_empty()
    {
        return Err(HttpError::Refuse(Status::BAD_REQUEST));
    }
    let mut headers = Vec::new();
    let mut total = line.len();
    loop {
        let line = input
            .read_line(reader, max_bytes)
            .map_err(|e| read_error(e, Status::HEADERS_TOO_LARGE))?;
        if line.is_empty() {
            break;
        }
        total += line.len();
        if total > max_bytes || headers.len() >= max_count {
            return Err(HttpError::Refuse(Status::HEADERS_TOO_LARGE));
        }
        // A folded header line (obs-fold) is refused (RFC 9112 5.2).
        if matches!(line[0], b' ' | b'\t') {
            return Err(HttpError::Refuse(Status::BAD_REQUEST));
        }
        let text = String::from_utf8_lossy(&line);
        let Some((name, value)) = text.split_once(':') else {
            return Err(HttpError::Refuse(Status::BAD_REQUEST));
        };
        if name.is_empty() || name.bytes().any(|b| b.is_ascii_whitespace() || b.is_ascii_control()) {
            return Err(HttpError::Refuse(Status::BAD_REQUEST));
        }
        headers.push((name.to_string(), value.trim().to_string()));
    }
    Ok(Head {
        method: method.to_string(),
        target: target.to_string(),
        version,
        headers,
    })
}

/// The body as `framing` says, at most `max` bytes.
///
/// # Errors
///
/// [`HttpError`]: gone, too large (413), a malformed chunk (400).
pub fn read_body<R: Read + ?Sized>(
    input: &mut Input,
    reader: &mut R,
    framing: Framing,
    max: u64,
) -> Result<Vec<u8>, HttpError> {
    match framing {
        Framing::Length(n) => {
            if n > max {
                return Err(HttpError::Refuse(Status::TOO_LARGE));
            }
            let n = usize::try_from(n).map_err(|_| HttpError::Refuse(Status::TOO_LARGE))?;
            input.read_bytes(reader, n).map_err(|_| HttpError::Gone)
        }
        Framing::Chunked => {
            let mut body = Vec::new();
            loop {
                let line = input
                    .read_line(reader, 4096)
                    .map_err(|e| read_error(e, Status::BAD_REQUEST))?;
                let text = String::from_utf8_lossy(&line);
                let size_text = text.split(';').next().unwrap_or_default().trim();
                if size_text.is_empty()
                    || size_text.len() > 15
                    || !size_text.bytes().all(|b| b.is_ascii_hexdigit())
                {
                    return Err(HttpError::Refuse(Status::BAD_REQUEST));
                }
                let size = u64::from_str_radix(size_text, 16)
                    .map_err(|_| HttpError::Refuse(Status::BAD_REQUEST))?;
                if size == 0 {
                    // Trailers, up to the empty line.
                    loop {
                        let trailer = input
                            .read_line(reader, 4096)
                            .map_err(|e| read_error(e, Status::BAD_REQUEST))?;
                        if trailer.is_empty() {
                            return Ok(body);
                        }
                    }
                }
                if body.len() as u64 + size > max {
                    return Err(HttpError::Refuse(Status::TOO_LARGE));
                }
                let chunk = input
                    .read_bytes(reader, size as usize)
                    .map_err(|_| HttpError::Gone)?;
                body.extend_from_slice(&chunk);
                let end = input
                    .read_line(reader, 2)
                    .map_err(|e| read_error(e, Status::BAD_REQUEST))?;
                if !end.is_empty() {
                    return Err(HttpError::Refuse(Status::BAD_REQUEST));
                }
            }
        }
    }
}

/// A request body as a stream: its bytes as `framing` says, at most `max`, read from the
/// connection's buffer (`input`) and `reader` as they are asked for, so a big PUT is never held
/// whole. A failure - the client gone, too large, a malformed chunk - ends the stream with an
/// error and is kept for [`BodyReader::take_failure`]. A length is checked against `max` by the
/// caller before the body is read (the answer is 413 before any byte); a chunked body here.
pub struct BodyReader<'a, R: Read + ?Sized> {
    input: &'a mut Input,
    reader: &'a mut R,
    framing: Framing,
    max: u64,
    /// Bytes handed out.
    taken: u64,
    /// Left of the body (a length) or of the current chunk.
    left: u64,
    /// A chunk came before: its line end is read before the next size.
    chunk_started: bool,
    done: bool,
    failure: Option<HttpError>,
}

impl<'a, R: Read + ?Sized> BodyReader<'a, R> {
    #[must_use]
    pub fn new(input: &'a mut Input, reader: &'a mut R, framing: Framing, max: u64) -> Self {
        let left = match framing {
            Framing::Length(n) => n,
            Framing::Chunked => 0,
        };
        BodyReader {
            input,
            reader,
            framing,
            max,
            taken: 0,
            left,
            chunk_started: false,
            done: false,
            failure: None,
        }
    }

    /// Why the stream ended early, if it did.
    pub fn take_failure(&mut self) -> Option<HttpError> {
        self.failure.take()
    }

    fn fail(&mut self, failure: HttpError) -> std::io::Error {
        self.done = true;
        let text = match &failure {
            HttpError::Gone => "the client went away",
            HttpError::Refuse(status) => status.1,
        };
        self.failure = Some(failure);
        std::io::Error::other(text)
    }

    /// The next chunk's size; 0 is the last chunk (its trailers read).
    fn next_chunk(&mut self) -> Result<u64, HttpError> {
        if self.chunk_started {
            let end = self
                .input
                .read_line(self.reader, 2)
                .map_err(|e| read_error(e, Status::BAD_REQUEST))?;
            if !end.is_empty() {
                return Err(HttpError::Refuse(Status::BAD_REQUEST));
            }
        }
        self.chunk_started = true;
        let line = self
            .input
            .read_line(self.reader, 4096)
            .map_err(|e| read_error(e, Status::BAD_REQUEST))?;
        let text = String::from_utf8_lossy(&line);
        let size_text = text.split(';').next().unwrap_or_default().trim();
        if size_text.is_empty()
            || size_text.len() > 15
            || !size_text.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err(HttpError::Refuse(Status::BAD_REQUEST));
        }
        let size = u64::from_str_radix(size_text, 16).map_err(|_| HttpError::Refuse(Status::BAD_REQUEST))?;
        if size == 0 {
            // Trailers, up to the empty line.
            loop {
                let trailer = self
                    .input
                    .read_line(self.reader, 4096)
                    .map_err(|e| read_error(e, Status::BAD_REQUEST))?;
                if trailer.is_empty() {
                    return Ok(0);
                }
            }
        }
        if self.taken + size > self.max {
            return Err(HttpError::Refuse(Status::TOO_LARGE));
        }
        Ok(size)
    }
}

impl<R: Read + ?Sized> Read for BodyReader<'_, R> {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        if self.done || out.is_empty() {
            return Ok(0);
        }
        if self.left == 0 {
            match self.framing {
                Framing::Length(_) => {
                    self.done = true;
                    return Ok(0);
                }
                Framing::Chunked => match self.next_chunk() {
                    Ok(0) => {
                        self.done = true;
                        return Ok(0);
                    }
                    Ok(size) => self.left = size,
                    Err(failure) => return Err(self.fail(failure)),
                },
            }
        }
        let n = out
            .len()
            .min(usize::try_from(self.left).unwrap_or(usize::MAX))
            .min(64 * 1024);
        let bytes = match self.input.read_bytes(self.reader, n) {
            Ok(bytes) => bytes,
            Err(_) => return Err(self.fail(HttpError::Gone)),
        };
        out[..n].copy_from_slice(&bytes);
        self.left -= n as u64;
        self.taken += n as u64;
        Ok(n)
    }
}

/// A response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    pub status: Status,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Response {
    #[must_use]
    pub fn new(status: Status) -> Response {
        Response {
            status,
            headers: Vec::new(),
            body: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_header(mut self, name: &str, value: impl Into<String>) -> Response {
        self.headers.push((name.to_string(), value.into()));
        self
    }

    #[must_use]
    pub fn with_body(mut self, content_type: &str, body: Vec<u8>) -> Response {
        self.headers
            .push((String::from("Content-Type"), content_type.to_string()));
        self.body = body;
        self
    }

    /// A short text body saying what went wrong.
    #[must_use]
    pub fn text(status: Status, text: &str) -> Response {
        Response::new(status).with_body("text/plain; charset=utf-8", format!("{text}\n").into_bytes())
    }

    /// The first header `name` (any case).
    #[must_use]
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

/// Writes `response` (its body unless `head_only`; Content-Length always, from the body
/// unless the response names one, as a HEAD's does).
///
/// # Errors
///
/// The socket's.
pub fn write_response<W: Write + ?Sized>(
    writer: &mut W,
    response: &Response,
    head_only: bool,
    keep_alive: bool,
) -> std::io::Result<()> {
    let mut out = format!(
        "HTTP/1.1 {} {}\r\nServer: Azlin Bridge\r\n",
        response.status.0, response.status.1
    );
    for (name, value) in &response.headers {
        // Header values never carry line ends (a response-splitting guard).
        let value: String = value.chars().filter(|c| *c != '\r' && *c != '\n').collect();
        out.push_str(&format!("{name}: {value}\r\n"));
    }
    if response.header("Content-Length").is_none() {
        out.push_str(&format!("Content-Length: {}\r\n", response.body.len()));
    }
    out.push_str(if keep_alive {
        "Connection: keep-alive\r\n\r\n"
    } else {
        "Connection: close\r\n\r\n"
    });
    let mut bytes = out.into_bytes();
    if !head_only {
        bytes.extend_from_slice(&response.body);
    }
    writer.write_all(&bytes)?;
    writer.flush()
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    fn head(text: &str) -> Result<Head, HttpError> {
        let mut input = Input::new();
        read_head(&mut input, &mut Cursor::new(text.as_bytes().to_vec()), 8192, 20)
    }

    fn refused(result: Result<Head, HttpError>) -> u16 {
        match result {
            Err(HttpError::Refuse(status)) => status.0,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_head_gives_method_target_version_and_headers() {
        let h = head("\r\nPROPFIND /docs/a%20b.txt HTTP/1.1\r\nHost: 127.0.0.1:1180\r\nDepth: 1\r\n\r\n").unwrap();
        assert_eq!(h.method, "PROPFIND");
        assert_eq!(h.target, "/docs/a%20b.txt");
        assert_eq!(h.version, (1, 1));
        assert_eq!(h.header("depth"), Some("1"));
        assert!(h.keep_alive());
        let old = head("GET / HTTP/1.0\r\n\r\n").unwrap();
        assert!(!old.keep_alive());
        let closing = head("GET / HTTP/1.1\r\nConnection: close\r\n\r\n").unwrap();
        assert!(!closing.keep_alive());
    }

    #[test]
    fn malformed_or_oversized_heads_are_refused_with_their_status() {
        assert_eq!(refused(head("GET /\r\n\r\n")), 400);
        assert_eq!(refused(head("get / HTTP/1.1\r\n\r\n")), 400);
        assert_eq!(refused(head("GET / HTTP/2.0\r\n\r\n")), 505);
        assert_eq!(refused(head("GET / HTTP/1.1\r\nX: a\r\n folded\r\n\r\n")), 400);
        assert_eq!(refused(head("GET / HTTP/1.1\r\nno colon\r\n\r\n")), 400);
        let many: String = (0..30).map(|i| format!("X-{i}: v\r\n")).collect();
        assert_eq!(refused(head(&format!("GET / HTTP/1.1\r\n{many}\r\n"))), 431);
        let long = "a".repeat(10_000);
        assert_eq!(refused(head(&format!("GET /{long} HTTP/1.1\r\n\r\n"))), 414);
        assert!(matches!(head(""), Err(HttpError::Gone)));
    }

    #[test]
    fn bodies_come_by_length_or_in_chunks_within_the_limit() {
        let h = head("PUT /a HTTP/1.1\r\nContent-Length: 5\r\n\r\n").unwrap();
        assert_eq!(h.framing().unwrap(), Framing::Length(5));
        let chunked = head("PUT /a HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n").unwrap();
        assert_eq!(chunked.framing().unwrap(), Framing::Chunked);
        let both = head("PUT /a HTTP/1.1\r\nTransfer-Encoding: chunked\r\nContent-Length: 5\r\n\r\n").unwrap();
        assert_eq!(both.framing().unwrap_err().0, 400);
        let gzip = head("PUT /a HTTP/1.1\r\nTransfer-Encoding: gzip\r\n\r\n").unwrap();
        assert_eq!(gzip.framing().unwrap_err().0, 501);
        let two = head("PUT /a HTTP/1.1\r\nContent-Length: 5\r\nContent-Length: 6\r\n\r\n").unwrap();
        assert_eq!(two.framing().unwrap_err().0, 400);
        let minus = head("PUT /a HTTP/1.1\r\nContent-Length: -1\r\n\r\n").unwrap();
        assert_eq!(minus.framing().unwrap_err().0, 400);

        let mut input = Input::new();
        let mut wire = Cursor::new(b"4;ext=1\r\nWiki\r\n5\r\npedia\r\n0\r\nX-Trailer: 1\r\n\r\nNEXT".to_vec());
        let body = read_body(&mut input, &mut wire, Framing::Chunked, 100).unwrap();
        assert_eq!(body, b"Wikipedia");
        let mut input = Input::new();
        let mut wire = Cursor::new(b"ff\r\n".to_vec());
        assert!(matches!(
            read_body(&mut input, &mut wire, Framing::Chunked, 100),
            Err(HttpError::Refuse(Status(413, _)))
        ));
        let mut input = Input::new();
        let mut wire = Cursor::new(b"zz\r\n".to_vec());
        assert!(matches!(
            read_body(&mut input, &mut wire, Framing::Chunked, 100),
            Err(HttpError::Refuse(Status(400, _)))
        ));
        let mut input = Input::new();
        assert!(matches!(
            read_body(&mut input, &mut Cursor::new(Vec::new()), Framing::Length(101), 100),
            Err(HttpError::Refuse(Status(413, _)))
        ));
    }

    #[test]
    fn a_response_has_its_length_and_never_a_line_end_inside_a_header() {
        let mut out = Vec::new();
        let response = Response::text(Status::NOT_FOUND, "nothing")
            .with_header("X-Evil", "a\r\nSet-Cookie: x=y");
        write_response(&mut out, &response, false, true).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.starts_with("HTTP/1.1 404 Not Found\r\n"), "{text}");
        assert!(text.contains("Content-Length: 8\r\n"), "{text}");
        assert!(text.contains("X-Evil: aSet-Cookie: x=y\r\n"), "{text}");
        assert!(text.ends_with("\r\n\r\nnothing\n"), "{text}");
        let mut out = Vec::new();
        write_response(&mut out, &response, true, false).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.ends_with("Connection: close\r\n\r\n"), "{text}");
    }
}
