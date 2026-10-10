//! An S3 service in memory, behind the kit's [`Bucket`]: it answers path-style requests the
//! way an S3 service does - objects with their ETags, conditional reads (`If-None-Match`: 304)
//! and writes (`If-Match`, `If-None-Match: *`: 412), ranges (206), ListObjectsV2 pages,
//! multipart uploads. Every request is logged as `METHOD key` (`LIST prefix` for a listing),
//! and a test can make another device's write land right before the next conditional PUT
//! ([`FakeS3::before_next_cas`]).

use std::{
    collections::BTreeMap,
    ops::Deref,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex, MutexGuard, PoisonError,
    },
};

use azul_storage::{
    sigv4::uri_decode, time::iso8601, Credentials, HttpCall, HttpReply, Method, S3Config,
    Transport,
};

use super::S3;
use crate::{
    bucket::Bucket,
    drive::TransportFactory,
    error::CloudResult,
    store::{Conditional, RemoteObject, RemoteStore},
};

/// The bucket the fake serves.
pub(crate) const BUCKET: &str = "d-1";

/// Another device's write, run once right before the next conditional PUT.
type Race = Box<dyn FnOnce(&FakeS3) + Send>;

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// One object: its bytes, its ETag (quotes included, as S3 sends it), when it was written.
struct Object {
    data: Vec<u8>,
    etag: String,
    at: i64,
}

/// The service.
#[derive(Default)]
pub(crate) struct FakeS3 {
    objects: Mutex<BTreeMap<String, Object>>,
    /// Multipart uploads under way: their parts by number.
    uploads: Mutex<BTreeMap<String, BTreeMap<u32, Vec<u8>>>>,
    next: AtomicU64,
    race: Mutex<Option<Race>>,
    /// Every request, as `METHOD key`.
    pub log: Mutex<Vec<String>>,
}

fn reply(status: u16, headers: Vec<(String, String)>, body: Vec<u8>) -> HttpReply {
    HttpReply {
        status,
        headers,
        body,
    }
}

fn error(status: u16, code: &str, message: &str) -> HttpReply {
    reply(
        status,
        vec![(
            String::from("content-type"),
            String::from("application/xml"),
        )],
        format!("<Error><Code>{code}</Code><Message>{message}</Message></Error>").into_bytes(),
    )
}

fn xml_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// The path and the query parameters (decoded) of a URL.
fn split_url(url: &str) -> (String, BTreeMap<String, String>) {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    let path_and_query = rest.find('/').map_or("/", |i| &rest[i..]);
    let (path, query) = path_and_query
        .split_once('?')
        .unwrap_or((path_and_query, ""));
    let params = query
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
    (path.to_string(), params)
}

impl FakeS3 {
    pub fn new() -> Arc<FakeS3> {
        Arc::new(FakeS3::default())
    }

    /// A transport to this service for every endpoint a bucket asks.
    pub fn factory(self: &Arc<Self>) -> TransportFactory {
        let s3 = self.clone();
        Arc::new(move || Box::new(S3Handle(s3.clone())) as Box<dyn Transport>)
    }

    /// The kit's bucket on this service (at the tests' S3 endpoint, signed at a fixed time).
    pub fn bucket(self: &Arc<Self>) -> Bucket {
        Bucket::new(
            S3Config {
                endpoint: S3.to_string(),
                region: String::from("us-east-1"),
                bucket: BUCKET.to_string(),
                path_style: true,
            },
            Credentials::new("AKID1", "secret-of-AKID1"),
            self.factory(),
        )
        .unwrap()
        .with_clock(|| 1_791_450_900)
    }

    fn note(&self, method: &str, key: &str) {
        lock(&self.log).push(format!("{method} {key}"));
    }

    /// Writes `data` at `key` unconditionally (a test's other device); the new ETag.
    pub fn write(&self, key: &str, data: Vec<u8>) -> String {
        let etag = format!("\"v{}\"", self.next.fetch_add(1, Ordering::SeqCst) + 1);
        lock(&self.objects).insert(
            key.to_string(),
            Object {
                data,
                etag: etag.clone(),
                at: crate::now(),
            },
        );
        etag
    }

    /// The object at `key`.
    pub fn read(&self, key: &str) -> Option<Vec<u8>> {
        lock(&self.objects).get(key).map(|o| o.data.clone())
    }

    /// Every key.
    pub fn keys(&self) -> Vec<String> {
        lock(&self.objects).keys().cloned().collect()
    }

    /// Runs `race` once, right before the next conditional PUT.
    pub fn before_next_cas(&self, race: impl FnOnce(&FakeS3) + Send + 'static) {
        *lock(&self.race) = Some(Box::new(race));
    }

    /// How many requests of `method` (`PUT`, `GET`, `HEAD`, `LIST`, ...) were made.
    pub fn count(&self, method: &str) -> usize {
        let prefix = format!("{method} ");
        lock(&self.log)
            .iter()
            .filter(|line| line.starts_with(&prefix))
            .count()
    }

    /// Forgets the request log.
    pub fn clear_log(&self) {
        lock(&self.log).clear();
    }

    /// The answer to one request.
    pub fn answer(&self, call: &HttpCall) -> HttpReply {
        let (path, query) = split_url(&call.url);
        let rest = path.trim_start_matches('/');
        let (bucket, key) = match rest.split_once('/') {
            Some((bucket, key)) => (bucket, uri_decode(key).unwrap_or_default()),
            None => (rest, String::new()),
        };
        if bucket != BUCKET {
            return error(404, "NoSuchBucket", "The specified bucket does not exist");
        }
        if !call
            .headers
            .iter()
            .any(|(name, _)| name.eq_ignore_ascii_case("authorization"))
        {
            return error(403, "AccessDenied", "the request is not signed");
        }
        let header = |name: &str| {
            call.headers
                .iter()
                .find(|(n, _)| n.eq_ignore_ascii_case(name))
                .map(|(_, v)| v.clone())
        };
        match (call.method, key.is_empty()) {
            (Method::Get, true) if query.get("list-type").map(String::as_str) == Some("2") => {
                self.list_answer(&query)
            }
            (Method::Get, false) if query.contains_key("uploadId") => {
                self.parts_answer(&key, &query)
            }
            (Method::Get, false) => {
                self.get_answer(&key, header("if-none-match"), header("range"))
            }
            (Method::Head, false) => self.head_answer(&key),
            (Method::Put, false) if query.contains_key("partNumber") => {
                self.part_answer(&key, &query, &call.body)
            }
            (Method::Put, false) => {
                self.put_answer(&key, header("if-match"), header("if-none-match"), call)
            }
            (Method::Post, false) if query.contains_key("uploads") => self.start_answer(&key),
            (Method::Post, false) if query.contains_key("uploadId") => {
                self.complete_answer(&key, &query, header("if-none-match"), header("if-match"))
            }
            (Method::Delete, false) if query.contains_key("uploadId") => {
                self.note("ABORT", &key);
                let id = query.get("uploadId").cloned().unwrap_or_default();
                match lock(&self.uploads).remove(&id) {
                    Some(_) => reply(204, Vec::new(), Vec::new()),
                    None => error(404, "NoSuchUpload", "The specified upload does not exist"),
                }
            }
            (Method::Delete, false) => {
                self.note("DELETE", &key);
                lock(&self.objects).remove(&key);
                reply(204, Vec::new(), Vec::new())
            }
            _ => error(400, "BadRequest", "a request this fake does not know"),
        }
    }

    fn get_answer(&self, key: &str, if_none_match: Option<String>, range: Option<String>) -> HttpReply {
        self.note("GET", key);
        let objects = lock(&self.objects);
        let Some(object) = objects.get(key) else {
            return error(404, "NoSuchKey", "The specified key does not exist.");
        };
        let etag = vec![(String::from("etag"), object.etag.clone())];
        if if_none_match.as_deref() == Some(object.etag.as_str()) {
            return reply(304, etag, Vec::new());
        }
        if let Some(range) = range {
            let len = object.data.len() as u64;
            let (start, end) = range
                .trim_start_matches("bytes=")
                .split_once('-')
                .unwrap_or(("0", ""));
            let start: u64 = start.parse().unwrap_or(0);
            let end: u64 = end.parse().unwrap_or(len.saturating_sub(1)).min(len.saturating_sub(1));
            if start >= len {
                return error(416, "InvalidRange", "The requested range is not satisfiable");
            }
            let mut headers = etag;
            headers.push((
                String::from("content-range"),
                format!("bytes {start}-{end}/{len}"),
            ));
            return reply(
                206,
                headers,
                object.data[start as usize..=end as usize].to_vec(),
            );
        }
        reply(200, etag, object.data.clone())
    }

    fn head_answer(&self, key: &str) -> HttpReply {
        self.note("HEAD", key);
        match lock(&self.objects).get(key) {
            Some(object) => reply(
                200,
                vec![
                    (String::from("content-length"), object.data.len().to_string()),
                    (String::from("etag"), object.etag.clone()),
                ],
                Vec::new(),
            ),
            None => reply(404, Vec::new(), Vec::new()),
        }
    }

    fn put_answer(
        &self,
        key: &str,
        if_match: Option<String>,
        if_none_match: Option<String>,
        call: &HttpCall,
    ) -> HttpReply {
        if if_match.is_some() || if_none_match.is_some() {
            let race = lock(&self.race).take();
            if let Some(race) = race {
                race(self);
            }
        }
        self.note("PUT", key);
        let current = lock(&self.objects).get(key).map(|o| o.etag.clone());
        let wins = match (&if_match, &if_none_match, current.as_deref()) {
            (Some(want), _, Some(have)) => want == have,
            (Some(_), _, None) => false,
            (None, Some(star), have) if star == "*" => have.is_none(),
            _ => true,
        };
        if !wins {
            return error(
                412,
                "PreconditionFailed",
                "At least one of the pre-conditions you specified did not hold",
            );
        }
        let etag = self.write(key, call.body.clone());
        reply(200, vec![(String::from("etag"), etag)], Vec::new())
    }

    fn start_answer(&self, key: &str) -> HttpReply {
        self.note("POST", key);
        let id = format!("up-{}", self.next.fetch_add(1, Ordering::SeqCst) + 1);
        lock(&self.uploads).insert(id.clone(), BTreeMap::new());
        reply(
            200,
            Vec::new(),
            format!(
                "<InitiateMultipartUploadResult><Bucket>{BUCKET}</Bucket><Key>{}</Key>\
                 <UploadId>{id}</UploadId></InitiateMultipartUploadResult>",
                xml_escape(key)
            )
            .into_bytes(),
        )
    }

    fn part_answer(&self, key: &str, query: &BTreeMap<String, String>, body: &[u8]) -> HttpReply {
        self.note("PUT", key);
        let number: u32 = query
            .get("partNumber")
            .and_then(|n| n.parse().ok())
            .unwrap_or(0);
        let id = query.get("uploadId").cloned().unwrap_or_default();
        let mut uploads = lock(&self.uploads);
        let Some(parts) = uploads.get_mut(&id) else {
            return error(404, "NoSuchUpload", "The specified upload does not exist");
        };
        parts.insert(number, body.to_vec());
        reply(
            200,
            vec![(String::from("etag"), format!("\"part-{number}\""))],
            Vec::new(),
        )
    }

    fn complete_answer(
        &self,
        key: &str,
        query: &BTreeMap<String, String>,
        if_none_match: Option<String>,
        if_match: Option<String>,
    ) -> HttpReply {
        self.note("POST", key);
        let id = query.get("uploadId").cloned().unwrap_or_default();
        if !lock(&self.uploads).contains_key(&id) {
            return error(404, "NoSuchUpload", "The specified upload does not exist");
        }
        let current = lock(&self.objects).get(key).map(|o| o.etag.clone());
        let holds = match (&if_match, &if_none_match, current.as_deref()) {
            (Some(want), _, Some(have)) => want == have,
            (Some(_), _, None) => false,
            (None, Some(star), have) if star == "*" => have.is_none(),
            _ => true,
        };
        if !holds {
            return error(
                412,
                "PreconditionFailed",
                "At least one of the pre-conditions you specified did not hold",
            );
        }
        let Some(parts) = lock(&self.uploads).remove(&id) else {
            return error(404, "NoSuchUpload", "The specified upload does not exist");
        };
        let data: Vec<u8> = parts.into_values().flatten().collect();
        let etag = self.write(key, data);
        reply(
            200,
            vec![(String::from("etag"), etag.clone())],
            format!(
                "<CompleteMultipartUploadResult><Key>{}</Key><ETag>{}</ETag>\
                 </CompleteMultipartUploadResult>",
                xml_escape(key),
                xml_escape(&etag)
            )
            .into_bytes(),
        )
    }

    /// ListParts of an upload under way.
    fn parts_answer(&self, key: &str, query: &BTreeMap<String, String>) -> HttpReply {
        self.note("PARTS", key);
        let id = query.get("uploadId").cloned().unwrap_or_default();
        let uploads = lock(&self.uploads);
        let Some(parts) = uploads.get(&id) else {
            return error(404, "NoSuchUpload", "The specified upload does not exist");
        };
        let mut xml = format!(
            "<ListPartsResult><Bucket>{BUCKET}</Bucket><Key>{}</Key><UploadId>{id}</UploadId>\
             <IsTruncated>false</IsTruncated>",
            xml_escape(key)
        );
        for (number, bytes) in parts {
            xml.push_str(&format!(
                "<Part><PartNumber>{number}</PartNumber><ETag>&quot;part-{number}&quot;</ETag>\
                 <Size>{}</Size></Part>",
                bytes.len()
            ));
        }
        xml.push_str("</ListPartsResult>");
        reply(200, Vec::new(), xml.into_bytes())
    }

    fn list_answer(&self, query: &BTreeMap<String, String>) -> HttpReply {
        let prefix = query.get("prefix").cloned().unwrap_or_default();
        self.note("LIST", &prefix);
        let max: usize = query
            .get("max-keys")
            .and_then(|m| m.parse().ok())
            .unwrap_or(1000)
            .max(1);
        let after = query.get("continuation-token").cloned();
        let objects = lock(&self.objects);
        let matching: Vec<(&String, &Object)> = objects
            .iter()
            .filter(|(k, _)| k.starts_with(&prefix))
            .filter(|(k, _)| after.as_ref().is_none_or(|a| k.as_str() > a.as_str()))
            .collect();
        let page = &matching[..matching.len().min(max)];
        let truncated = matching.len() > page.len();
        let mut xml = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?><ListBucketResult \
             xmlns=\"http://s3.amazonaws.com/doc/2006-03-01/\"><Name>{BUCKET}</Name>\
             <Prefix>{}</Prefix><KeyCount>{}</KeyCount><MaxKeys>{max}</MaxKeys>\
             <IsTruncated>{truncated}</IsTruncated>",
            xml_escape(&prefix),
            page.len()
        );
        if truncated {
            if let Some((last, _)) = page.last() {
                xml.push_str(&format!(
                    "<NextContinuationToken>{}</NextContinuationToken>",
                    xml_escape(last)
                ));
            }
        }
        for (key, object) in page {
            xml.push_str(&format!(
                "<Contents><Key>{}</Key><LastModified>{}</LastModified><ETag>{}</ETag>\
                 <Size>{}</Size></Contents>",
                xml_escape(key),
                iso8601(u64::try_from(object.at).unwrap_or(0)),
                xml_escape(&object.etag),
                object.data.len()
            ));
        }
        xml.push_str("</ListBucketResult>");
        reply(
            200,
            vec![(
                String::from("content-type"),
                String::from("application/xml"),
            )],
            xml.into_bytes(),
        )
    }
}

/// The service as a transport of its own (one per endpoint a bucket asks).
pub(crate) struct S3Handle(pub Arc<FakeS3>);

impl Transport for S3Handle {
    fn send(&self, call: &HttpCall) -> Result<HttpReply, String> {
        Ok(self.0.answer(call))
    }
}

/// The service and the kit's bucket on it: what a sync test syncs with (the bucket) and looks
/// into (the service, through `Deref`).
pub(crate) struct S3Bucket {
    pub s3: Arc<FakeS3>,
    pub bucket: Bucket,
}

impl S3Bucket {
    pub fn new() -> S3Bucket {
        let s3 = FakeS3::new();
        let bucket = s3.bucket();
        S3Bucket { s3, bucket }
    }
}

impl Deref for S3Bucket {
    type Target = FakeS3;

    fn deref(&self) -> &FakeS3 {
        &self.s3
    }
}

impl RemoteStore for S3Bucket {
    fn get_unless(&self, key: &str, etag: Option<&str>) -> CloudResult<Conditional> {
        RemoteStore::get_unless(&self.bucket, key, etag)
    }

    fn fetch(&self, key: &str, size: u64) -> CloudResult<Option<Vec<u8>>> {
        RemoteStore::fetch(&self.bucket, key, size)
    }

    fn put(&self, key: &str, data: &[u8]) -> CloudResult<String> {
        RemoteStore::put(&self.bucket, key, data)
    }

    fn put_if(
        &self,
        key: &str,
        data: &[u8],
        if_match: Option<&str>,
    ) -> CloudResult<Option<String>> {
        RemoteStore::put_if(&self.bucket, key, data, if_match)
    }

    fn head(&self, key: &str) -> CloudResult<Option<u64>> {
        RemoteStore::head(&self.bucket, key)
    }

    fn delete(&self, key: &str) -> CloudResult<()> {
        RemoteStore::delete(&self.bucket, key)
    }

    fn list(&self, prefix: &str) -> CloudResult<Vec<RemoteObject>> {
        RemoteStore::list(&self.bucket, prefix)
    }

    fn put_from(&self, key: &str, body: &mut dyn std::io::Read, size: u64) -> CloudResult<String> {
        RemoteStore::put_from(&self.bucket, key, body, size)
    }

    fn fetch_to(&self, key: &str, size: u64, dest: &std::path::Path) -> CloudResult<bool> {
        RemoteStore::fetch_to(&self.bucket, key, size, dest)
    }
}
