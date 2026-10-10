//! An S3 service in memory for the tests that need one that remembers: objects with their
//! ETags, conditional writes (a PUT and a CompleteMultipartUpload with `If-None-Match: *` or
//! `If-Match`: 412 when the condition does not hold), ranges, and multipart uploads - start,
//! parts, ListParts, complete, abort. It answers path-style requests for the bucket `azdrive`
//! at any host (the failover tests ask several). Every request is logged (`POST start big.bin`,
//! `PUT part 3 big.bin`, `GET list-parts big.bin`, ...). Switches: after a number of parts no
//! part gets an answer (an app killed half way), and a part takes a while (to see how many are
//! in flight at once).

use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex, MutexGuard, PoisonError,
    },
    time::Duration,
};

use crate::{sigv4::uri_decode, HttpCall, HttpReply, Method, Transport};

/// The bucket the fake serves.
pub(super) const BUCKET: &str = "azdrive";

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

struct Upload {
    key: String,
    /// Part number -> its bytes and its ETag (quotes included).
    parts: BTreeMap<u32, (Vec<u8>, String)>,
}

#[derive(Default)]
struct State {
    /// Key -> bytes and ETag (quotes included).
    objects: BTreeMap<String, (Vec<u8>, String)>,
    uploads: BTreeMap<String, Upload>,
    next: u64,
}

impl State {
    fn next_id(&mut self) -> u64 {
        self.next += 1;
        self.next
    }
}

/// The service. Share it with `Arc`; [`FakeBucket::transport`] makes a transport to it.
#[derive(Default)]
pub(super) struct FakeBucket {
    state: Mutex<State>,
    log: Mutex<Vec<String>>,
    /// `Some(n)`: n more parts are taken, then no part gets an answer.
    parts_left: Mutex<Option<usize>>,
    part_delay: Mutex<Duration>,
    in_flight: AtomicUsize,
    most_in_flight: AtomicUsize,
}

fn reply(status: u16, headers: &[(&str, &str)], body: impl Into<Vec<u8>>) -> HttpReply {
    HttpReply {
        status,
        headers: headers
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
        body: body.into(),
    }
}

fn error(status: u16, code: &str) -> HttpReply {
    reply(
        status,
        &[("content-type", "application/xml")],
        format!("<Error><Code>{code}</Code><Message>{code}</Message></Error>"),
    )
}

/// The key (decoded) and the query of a path-style URL of the bucket.
fn split(url: &str) -> Option<(String, BTreeMap<String, String>)> {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    let path_and_query = rest.find('/').map_or("/", |i| &rest[i..]);
    let (path, query) = path_and_query
        .split_once('?')
        .unwrap_or((path_and_query, ""));
    let path = path.trim_start_matches('/');
    let (bucket, key) = path.split_once('/').unwrap_or((path, ""));
    if bucket != BUCKET {
        return None;
    }
    let query = query
        .split('&')
        .filter(|p| !p.is_empty())
        .map(|p| {
            let (name, value) = p.split_once('=').unwrap_or((p, ""));
            (
                uri_decode(name).unwrap_or_default(),
                uri_decode(value).unwrap_or_default(),
            )
        })
        .collect();
    Some((uri_decode(key).unwrap_or_default(), query))
}

fn header<'a>(call: &'a HttpCall, name: &str) -> Option<&'a str> {
    call.headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.as_str())
}

/// The `(PartNumber, ETag)` pairs of a CompleteMultipartUpload body.
fn listed_parts(xml: &str) -> Vec<(u32, String)> {
    let mut out = Vec::new();
    for part in xml.split("<Part>").skip(1) {
        let text = |tag: &str| {
            let open = format!("<{tag}>");
            let close = format!("</{tag}>");
            let start = part.find(&open)? + open.len();
            let end = part[start..].find(&close)? + start;
            Some(part[start..end].replace("&quot;", "\""))
        };
        if let (Some(number), Some(etag)) = (text("PartNumber"), text("ETag")) {
            if let Ok(number) = number.trim().parse() {
                out.push((number, etag));
            }
        }
    }
    out
}

/// Whether a write with these headers may replace `current` (its ETag).
fn condition_holds(call: &HttpCall, current: Option<&str>) -> bool {
    if header(call, "if-none-match").map(str::trim) == Some("*") && current.is_some() {
        return false;
    }
    match header(call, "if-match") {
        Some(want) => {
            current.is_some_and(|have| have.trim_matches('"') == want.trim().trim_matches('"'))
        }
        None => true,
    }
}

impl FakeBucket {
    pub(super) fn new() -> Arc<FakeBucket> {
        Arc::new(FakeBucket::default())
    }

    /// A transport to this service.
    pub(super) fn transport(self: &Arc<Self>) -> Box<dyn Transport> {
        Box::new(Handle(self.clone()))
    }

    /// `n` more parts are taken, then no part gets an answer; `None`: every part is taken.
    pub(super) fn take_parts(&self, n: Option<usize>) {
        *lock(&self.parts_left) = n;
    }

    /// Every part takes `delay` before it is answered.
    pub(super) fn slow_parts(&self, delay: Duration) {
        *lock(&self.part_delay) = delay;
    }

    /// The most parts that were in flight at once.
    pub(super) fn most_parts_at_once(&self) -> usize {
        self.most_in_flight.load(Ordering::SeqCst)
    }

    pub(super) fn log(&self) -> Vec<String> {
        lock(&self.log).clone()
    }

    pub(super) fn clear_log(&self) {
        lock(&self.log).clear();
    }

    /// How many log lines start with `what` (`PUT part`, `POST start`, `DELETE abort`, ...).
    pub(super) fn count(&self, what: &str) -> usize {
        lock(&self.log)
            .iter()
            .filter(|line| line.starts_with(what))
            .count()
    }

    pub(super) fn object(&self, key: &str) -> Option<Vec<u8>> {
        lock(&self.state).objects.get(key).map(|(b, _)| b.clone())
    }

    /// Writes `key` as another device would; its ETag.
    pub(super) fn write(&self, key: &str, bytes: &[u8]) -> String {
        let mut state = lock(&self.state);
        let etag = format!("\"v{}\"", state.next_id());
        state
            .objects
            .insert(key.to_string(), (bytes.to_vec(), etag.clone()));
        etag
    }

    /// The ids of the multipart uploads under way.
    pub(super) fn uploads(&self) -> Vec<String> {
        lock(&self.state).uploads.keys().cloned().collect()
    }

    /// Forgets every upload under way (a service that cleaned them up).
    pub(super) fn forget_uploads(&self) {
        lock(&self.state).uploads.clear();
    }

    fn note(&self, line: String) {
        lock(&self.log).push(line);
    }

    /// The answer to one request; `Err` for a request that gets no answer.
    pub(super) fn answer(&self, call: &HttpCall) -> Result<HttpReply, String> {
        let Some((key, query)) = split(&call.url) else {
            return Ok(error(404, "NoSuchBucket"));
        };
        let upload = query.get("uploadId").cloned();
        match (call.method, upload) {
            (Method::Put, Some(id)) => self.part(&key, &id, &query, call),
            (Method::Post, Some(id)) => Ok(self.complete(&key, &id, call)),
            (Method::Delete, Some(id)) => {
                self.note(format!("DELETE abort {key}"));
                let removed = lock(&self.state).uploads.remove(&id).is_some();
                Ok(if removed {
                    reply(204, &[], Vec::new())
                } else {
                    error(404, "NoSuchUpload")
                })
            }
            (Method::Get, Some(id)) => Ok(self.list_parts(&key, &id)),
            (Method::Post, None) if query.contains_key("uploads") => {
                self.note(format!("POST start {key}"));
                let mut state = lock(&self.state);
                let id = format!("up-{}", state.next_id());
                state.uploads.insert(
                    id.clone(),
                    Upload {
                        key: key.clone(),
                        parts: BTreeMap::new(),
                    },
                );
                Ok(reply(
                    200,
                    &[],
                    format!(
                        "<InitiateMultipartUploadResult><Bucket>{BUCKET}</Bucket><Key>{key}</Key>\
                         <UploadId>{id}</UploadId></InitiateMultipartUploadResult>"
                    ),
                ))
            }
            (Method::Put, None) => {
                self.note(format!("PUT object {key}"));
                let mut state = lock(&self.state);
                let current = state.objects.get(&key).map(|(_, e)| e.clone());
                if !condition_holds(call, current.as_deref()) {
                    return Ok(error(412, "PreconditionFailed"));
                }
                let etag = format!("\"v{}\"", state.next_id());
                state.objects.insert(key, (call.body.clone(), etag.clone()));
                Ok(reply(200, &[("ETag", etag.as_str())], Vec::new()))
            }
            (Method::Get | Method::Head, None) => {
                self.note(format!("{} object {key}", call.method.as_str()));
                let state = lock(&self.state);
                let Some((bytes, etag)) = state.objects.get(&key) else {
                    return Ok(error(404, "NoSuchKey"));
                };
                let len = bytes.len().to_string();
                if call.method == Method::Head {
                    return Ok(reply(
                        200,
                        &[("ETag", etag.as_str()), ("Content-Length", len.as_str())],
                        Vec::new(),
                    ));
                }
                match header(call, "range").and_then(|r| r.strip_prefix("bytes=")) {
                    Some(range) => {
                        let (start, end) = range.split_once('-').unwrap_or((range, ""));
                        let start: usize = start.parse().unwrap_or(0);
                        if start >= bytes.len() {
                            return Ok(error(416, "InvalidRange"));
                        }
                        let end: usize =
                            end.parse().unwrap_or(bytes.len() - 1).min(bytes.len() - 1);
                        Ok(reply(
                            206,
                            &[("ETag", etag.as_str())],
                            bytes[start..=end].to_vec(),
                        ))
                    }
                    None => Ok(reply(200, &[("ETag", etag.as_str())], bytes.clone())),
                }
            }
            (Method::Delete, None) => {
                self.note(format!("DELETE object {key}"));
                lock(&self.state).objects.remove(&key);
                Ok(reply(204, &[], Vec::new()))
            }
            _ => Ok(error(400, "BadRequest")),
        }
    }

    fn part(
        &self,
        key: &str,
        id: &str,
        query: &BTreeMap<String, String>,
        call: &HttpCall,
    ) -> Result<HttpReply, String> {
        let number: u32 = query
            .get("partNumber")
            .and_then(|n| n.parse().ok())
            .unwrap_or(0);
        {
            let mut left = lock(&self.parts_left);
            match left.as_mut() {
                Some(0) => {
                    self.note(format!("PUT lost-part {number} {key}"));
                    return Err(String::from("connection reset (the app was killed)"));
                }
                Some(n) => *n -= 1,
                None => {}
            }
        }
        let now = self.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
        self.most_in_flight.fetch_max(now, Ordering::SeqCst);
        let delay = *lock(&self.part_delay);
        if !delay.is_zero() {
            std::thread::sleep(delay);
        }
        self.in_flight.fetch_sub(1, Ordering::SeqCst);
        self.note(format!("PUT part {number} {key}"));
        let mut state = lock(&self.state);
        let etag = format!("\"p{}-{number}\"", state.next_id());
        let Some(upload) = state.uploads.get_mut(id) else {
            return Ok(error(404, "NoSuchUpload"));
        };
        upload
            .parts
            .insert(number, (call.body.clone(), etag.clone()));
        Ok(reply(200, &[("ETag", etag.as_str())], Vec::new()))
    }

    fn list_parts(&self, key: &str, id: &str) -> HttpReply {
        self.note(format!("GET list-parts {key}"));
        let state = lock(&self.state);
        let Some(upload) = state.uploads.get(id) else {
            return error(404, "NoSuchUpload");
        };
        let mut xml = format!(
            "<ListPartsResult><Bucket>{BUCKET}</Bucket><Key>{key}</Key><UploadId>{id}</UploadId>\
             <IsTruncated>false</IsTruncated>"
        );
        for (number, (bytes, etag)) in &upload.parts {
            xml.push_str(&format!(
                "<Part><PartNumber>{number}</PartNumber><ETag>{}</ETag><Size>{}</Size></Part>",
                etag.replace('"', "&quot;"),
                bytes.len()
            ));
        }
        xml.push_str("</ListPartsResult>");
        reply(200, &[], xml)
    }

    fn complete(&self, key: &str, id: &str, call: &HttpCall) -> HttpReply {
        self.note(format!("POST complete {key}"));
        let mut state = lock(&self.state);
        let current = state.objects.get(key).map(|(_, e)| e.clone());
        if !state.uploads.contains_key(id) {
            return error(404, "NoSuchUpload");
        }
        if !condition_holds(call, current.as_deref()) {
            return error(412, "PreconditionFailed");
        }
        let listed = listed_parts(&String::from_utf8_lossy(&call.body));
        let mut data = Vec::new();
        {
            let Some(upload) = state.uploads.get(id) else {
                return error(404, "NoSuchUpload");
            };
            if upload.key != key || listed.is_empty() {
                return error(400, "InvalidPart");
            }
            for (number, etag) in &listed {
                match upload.parts.get(number) {
                    Some((bytes, have)) if have == etag => data.extend_from_slice(bytes),
                    _ => return error(400, "InvalidPart"),
                }
            }
        }
        state.uploads.remove(id);
        let etag = format!("\"v{}-{}\"", state.next_id(), listed.len());
        state.objects.insert(key.to_string(), (data, etag.clone()));
        reply(
            200,
            &[],
            format!(
                "<CompleteMultipartUploadResult><Key>{key}</Key><ETag>{}</ETag>\
                 </CompleteMultipartUploadResult>",
                etag.replace('"', "&quot;")
            ),
        )
    }
}

struct Handle(Arc<FakeBucket>);

impl Transport for Handle {
    fn send(&self, call: &HttpCall) -> Result<HttpReply, String> {
        self.0.answer(call)
    }
}
