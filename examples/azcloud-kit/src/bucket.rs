//! One bucket of an Azlin drive as the sync and the shares reach it: the built-in
//! [`RemoteStore`], S3 through azul-storage - its SigV4 ([`S3Drive::send_raw`]) and the app's
//! [`azul_storage::Transport`] (azul's HTTP client in the apps, a fake in the tests, whatever
//! the `azcloud` command line plugs in).
//!
//! - Every request goes to the block endpoint first, then to the drive's direct node URLs and
//!   failover addresses ([`crate::DriveBundle::node_urls`]) and to what an answer's
//!   `x-azlin-alt-endpoints` named: the next one is asked when one gives no answer or answers
//!   503.
//! - Conditional writes (`If-Match`, `If-None-Match: *`; a 412 says another writer won) and
//!   conditional reads (`If-None-Match`; a 304 says nothing changed): the sync's
//!   compare-and-swap.
//! - An object above twice the part size goes up as a multipart upload; one above the part size
//!   comes down in ranges; either moves several parts at once, on threads of its own.
//!
//! Blocking: call it from an azul `Thread`, never from a UI callback.

use std::{
    fmt,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex, MutexGuard, PoisonError,
    },
};

use azul_storage::{
    s3::parse_listing, ByteRange, Credentials, DriveError, HttpReply, ListPage, Method,
    ObjectInfo, S3Config, S3Drive,
};

use crate::{
    drive::TransportFactory,
    error::{CloudError, CloudResult},
    store::{Conditional, RemoteObject, RemoteStore, BIG_BLOB},
};

/// The part size of a multipart upload and of a ranged download.
pub const PART_SIZE: usize = 8 * 1024 * 1024;
/// Parts or ranges of one object in flight at once.
pub const PARALLEL: usize = 4;
/// The content type of every object the bucket writes (the sync's blobs and index, the
/// command line's uploads).
const OCTETS: &str = "application/octet-stream";

/// The mutex's value, also after a thread panicked while holding it.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The text of the first `<tag>...</tag>` of an XML answer.
fn xml_text<'a>(xml: &'a str, tag: &str) -> Option<&'a str> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = xml.find(&open)? + open.len();
    let end = xml[start..].find(&close)? + start;
    Some(&xml[start..end])
}

fn xml_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// The pair of a query parameter or a header.
fn pair(name: &str, value: impl Into<String>) -> (String, String) {
    (name.to_string(), value.into())
}

/// Runs `job(0)`, `job(1)`, ... `job(count - 1)` on up to `parallel` threads; the answers in
/// index order, or the first error (no new job starts after one failed).
fn in_parallel<T: Send>(
    count: usize,
    parallel: usize,
    job: impl Fn(usize) -> CloudResult<T> + Sync,
) -> CloudResult<Vec<T>> {
    let next = AtomicUsize::new(0);
    let results: Mutex<Vec<Option<CloudResult<T>>>> =
        Mutex::new((0..count).map(|_| None).collect());
    let workers = parallel.max(1).min(count.max(1));
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::SeqCst);
                if i >= count {
                    break;
                }
                let result = job(i);
                let failed = result.is_err();
                lock(&results)[i] = Some(result);
                if failed {
                    next.store(count, Ordering::SeqCst);
                    break;
                }
            });
        }
    });
    let mut out = Vec::with_capacity(count);
    for slot in results.into_inner().unwrap_or_else(PoisonError::into_inner) {
        match slot {
            Some(Ok(value)) => out.push(value),
            Some(Err(e)) => return Err(e),
            None => {
                return Err(CloudError::failed(
                    "a part was left out after another part failed",
                ))
            }
        }
    }
    Ok(out)
}

/// One bucket of a drive. `Debug` shows no secret.
pub struct Bucket {
    config: S3Config,
    credentials: Credentials,
    transports: TransportFactory,
    /// The endpoints after the block endpoint: the node URLs, the failover, what answers named.
    alternatives: Mutex<Vec<String>>,
    /// The drive of every endpoint asked so far.
    drives: Mutex<Vec<(String, Arc<S3Drive>)>>,
    part_size: usize,
    parallel: usize,
    clock: Option<Arc<dyn Fn() -> u64 + Send + Sync>>,
}

impl fmt::Debug for Bucket {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Bucket")
            .field("config", &self.config)
            .field("alternatives", &*lock(&self.alternatives))
            .field("part_size", &self.part_size)
            .field("parallel", &self.parallel)
            .finish_non_exhaustive()
    }
}

impl Bucket {
    /// The bucket `config` (its endpoint is the block endpoint) with `credentials`; every
    /// endpoint it asks gets a transport from `transports`. Refused when the endpoint or the
    /// bucket name cannot be one; sends nothing.
    pub fn new(
        config: S3Config,
        credentials: Credentials,
        transports: TransportFactory,
    ) -> CloudResult<Bucket> {
        S3Drive::new(config.clone(), credentials.clone(), transports())?;
        Ok(Bucket {
            config,
            credentials,
            transports,
            alternatives: Mutex::new(Vec::new()),
            drives: Mutex::new(Vec::new()),
            part_size: PART_SIZE,
            parallel: PARALLEL,
            clock: None,
        })
    }

    /// Asks `urls` too, in their order, when the block endpoint does not answer (the drive's
    /// node URLs and failover).
    #[must_use]
    pub fn with_alternatives(self, urls: Vec<String>) -> Bucket {
        {
            let mut alternatives = lock(&self.alternatives);
            for url in urls {
                let url = url.trim().trim_end_matches('/').to_string();
                if !url.is_empty() && url != self.config.endpoint && !alternatives.contains(&url)
                {
                    alternatives.push(url);
                }
            }
        }
        self
    }

    /// Signs with this clock (seconds since 1970) instead of the system's.
    #[must_use]
    pub fn with_clock(mut self, clock: impl Fn() -> u64 + Send + Sync + 'static) -> Bucket {
        self.clock = Some(Arc::new(clock));
        lock(&self.drives).clear();
        self
    }

    /// Sets how many parts or ranges of one big object are in flight (at least one).
    pub fn set_parallel(&mut self, parallel: usize) {
        self.parallel = parallel.max(1);
    }

    /// Sets the part size of multipart uploads and ranged downloads (at least one byte; S3
    /// takes parts of 5 MiB and more but the last).
    pub fn set_part_size(&mut self, part_size: usize) {
        self.part_size = part_size.max(1);
    }

    /// Where the bucket is: its block endpoint, region, name, URL style.
    #[must_use]
    pub fn config(&self) -> &S3Config {
        &self.config
    }

    /// The bucket's name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.config.bucket
    }

    /// The endpoints after the block endpoint, as they are now.
    #[must_use]
    pub fn alternatives(&self) -> Vec<String> {
        lock(&self.alternatives).clone()
    }

    /// The drive of `endpoint`, opened on first use.
    fn drive_at(&self, endpoint: &str) -> Result<Arc<S3Drive>, DriveError> {
        let mut drives = lock(&self.drives);
        if let Some((_, drive)) = drives.iter().find(|(e, _)| e == endpoint) {
            return Ok(drive.clone());
        }
        let config = S3Config {
            endpoint: endpoint.to_string(),
            ..self.config.clone()
        };
        let mut drive = S3Drive::new(config, self.credentials.clone(), (self.transports)())?;
        if let Some(clock) = &self.clock {
            let clock = clock.clone();
            drive = drive.with_clock(move || clock());
        }
        let drive = Arc::new(drive);
        drives.push((endpoint.to_string(), drive.clone()));
        Ok(drive)
    }

    /// Remembers the endpoints an answer's `x-azlin-alt-endpoints` names.
    fn learn(&self, reply: &HttpReply) {
        let Some(named) = reply.header("x-azlin-alt-endpoints") else {
            return;
        };
        let mut alternatives = lock(&self.alternatives);
        for url in named.split(',').map(str::trim).filter(|u| !u.is_empty()) {
            let url = url.trim_end_matches('/');
            if url != self.config.endpoint && !alternatives.iter().any(|a| a == url) {
                alternatives.push(url.to_string());
            }
        }
    }

    /// One request: the block endpoint first, then every alternative while there is no answer
    /// or a 503. A key S3 cannot take is refused before anything is sent.
    fn send(
        &self,
        method: Method,
        key: Option<&str>,
        query: Vec<(String, String)>,
        extra: Vec<(String, String)>,
        body: &[u8],
        content_type: &str,
    ) -> Result<HttpReply, DriveError> {
        let mut endpoints = vec![self.config.endpoint.clone()];
        endpoints.extend(lock(&self.alternatives).iter().cloned());
        let mut last: Option<DriveError> = None;
        for (i, endpoint) in endpoints.iter().enumerate() {
            let drive = match self.drive_at(endpoint) {
                Ok(drive) => drive,
                Err(e) => {
                    last = Some(e);
                    continue;
                }
            };
            let sent = drive.send_raw(
                method,
                key,
                query.clone(),
                extra.clone(),
                body.to_vec(),
                content_type,
            );
            match sent {
                Ok(reply) => {
                    self.learn(&reply);
                    if reply.status == 503 && i + 1 < endpoints.len() {
                        last = Some(S3Drive::failure_of(&reply, key));
                        continue;
                    }
                    return Ok(reply);
                }
                Err(DriveError::Transport(why)) => {
                    last = Some(DriveError::Transport(format!("{endpoint}: {why}")));
                }
                Err(other) => return Err(other),
            }
        }
        Err(last.unwrap_or_else(|| DriveError::Transport(String::from("no endpoint to ask"))))
    }

    /// The error of a failed answer about `key`, with the node's own reason when it sent one.
    fn failure(reply: &HttpReply, key: &str) -> CloudError {
        let error = CloudError::Drive(S3Drive::failure_of(reply, Some(key)));
        match reply.header("x-azlin-error") {
            Some(why) => error.context(format!("{key} (x-azlin-error {why})")),
            None => error,
        }
    }

    /// PUT (a multipart upload above twice the part size); the ETag the service answered.
    pub fn put(&self, key: &str, data: &[u8]) -> CloudResult<String> {
        if data.len() > self.part_size.saturating_mul(2) {
            return self.put_multipart(key, data);
        }
        let reply = self.send(Method::Put, Some(key), Vec::new(), Vec::new(), data, OCTETS)?;
        if !reply.is_success() {
            return Err(Self::failure(&reply, key));
        }
        Ok(reply.header("etag").unwrap_or_default().to_string())
    }

    /// Conditional PUT: `If-Match: <etag>`, or `If-None-Match: *` without one (the key must be
    /// free); `None` when another writer won (412).
    pub fn put_if(
        &self,
        key: &str,
        data: &[u8],
        if_match: Option<&str>,
    ) -> CloudResult<Option<String>> {
        let extra = match if_match {
            Some(etag) => vec![pair("if-match", etag)],
            None => vec![pair("if-none-match", "*")],
        };
        let reply = self.send(Method::Put, Some(key), Vec::new(), extra, data, OCTETS)?;
        match reply.status {
            200..=299 => Ok(Some(reply.header("etag").unwrap_or_default().to_string())),
            412 => Ok(None),
            _ => Err(Self::failure(&reply, key)),
        }
    }

    /// GET; `None` when there is no such object.
    pub fn get(&self, key: &str) -> CloudResult<Option<Vec<u8>>> {
        let reply = self.send(Method::Get, Some(key), Vec::new(), Vec::new(), &[], "")?;
        match reply.status {
            200..=299 => Ok(Some(reply.body)),
            404 => Ok(None),
            _ => Err(Self::failure(&reply, key)),
        }
    }

    /// A conditional GET: `If-None-Match: <etag>` when one is given (the index's poll: a 304
    /// says nothing changed).
    pub fn get_unless(&self, key: &str, etag: Option<&str>) -> CloudResult<Conditional> {
        let extra = etag
            .map(|etag| vec![pair("if-none-match", etag)])
            .unwrap_or_default();
        let reply = self.send(Method::Get, Some(key), Vec::new(), extra, &[], "")?;
        match reply.status {
            200..=299 => Ok(Conditional::Found {
                etag: reply.header("etag").map(str::to_string),
                body: reply.body,
            }),
            304 => Ok(Conditional::NotModified),
            404 => Ok(Conditional::NotFound),
            _ => Err(Self::failure(&reply, key)),
        }
    }

    /// GET of an object that may be big: a HEAD first, then ranges of the part size, several
    /// at once; `None` when there is no such object.
    pub fn get_big(&self, key: &str) -> CloudResult<Option<Vec<u8>>> {
        let Some((len, _)) = self.head(key)? else {
            return Ok(None);
        };
        let part = self.part_size as u64;
        if len <= part {
            return self.get(key);
        }
        let ranges: Vec<(u64, u64)> = (0..len)
            .step_by(self.part_size)
            .map(|start| (start, (start + part - 1).min(len - 1)))
            .collect();
        let parts = in_parallel(ranges.len(), self.parallel, |i| {
            let (start, end) = ranges[i];
            let range = ByteRange::new(start, Some(end)).header_value();
            let reply = self.send(
                Method::Get,
                Some(key),
                Vec::new(),
                vec![pair("range", range)],
                &[],
                "",
            )?;
            match reply.status {
                206 => Ok(reply.body),
                // A server that ignores `Range` sends the whole object: cut the range out.
                200 if reply.body.len() as u64 > end => {
                    Ok(reply.body[start as usize..=end as usize].to_vec())
                }
                _ if reply.is_success() => Err(CloudError::Drive(DriveError::Protocol(format!(
                    "{key}: the range {start}-{end} came back cut short"
                )))),
                _ => Err(Self::failure(&reply, key)),
            }
        })?;
        let mut out = Vec::with_capacity(usize::try_from(len).unwrap_or(0));
        for ((start, end), bytes) in ranges.iter().zip(parts) {
            if bytes.len() as u64 != end - start + 1 {
                return Err(CloudError::Drive(DriveError::Protocol(format!(
                    "{key}: the range {start}-{end} came back with {} bytes",
                    bytes.len()
                ))));
            }
            out.extend_from_slice(&bytes);
        }
        Ok(Some(out))
    }

    /// HEAD: the size and the ETag; `None` when there is no such object.
    pub fn head(&self, key: &str) -> CloudResult<Option<(u64, String)>> {
        let reply = self.send(Method::Head, Some(key), Vec::new(), Vec::new(), &[], "")?;
        match reply.status {
            200..=299 => Ok(Some((
                reply
                    .header("content-length")
                    .and_then(|v| v.trim().parse().ok())
                    .unwrap_or(0),
                reply.header("etag").unwrap_or_default().to_string(),
            ))),
            404 => Ok(None),
            _ => Err(Self::failure(&reply, key)),
        }
    }

    /// One signed HEAD of `key`: `Ok` for any answer at all (the pipe works), the reason
    /// otherwise.
    pub fn probe(&self, key: &str) -> Result<(), String> {
        self.send(Method::Head, Some(key), Vec::new(), Vec::new(), &[], "")
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    /// DELETE (a missing object is no error).
    pub fn delete(&self, key: &str) -> CloudResult<()> {
        let reply = self.send(Method::Delete, Some(key), Vec::new(), Vec::new(), &[], "")?;
        if reply.is_success() || reply.status == 404 {
            return Ok(());
        }
        Err(Self::failure(&reply, key))
    }

    /// One page of the objects under `prefix` (at every depth), after `continuation`.
    pub fn list_page(&self, prefix: &str, continuation: Option<&str>) -> CloudResult<ListPage> {
        let mut query = vec![
            pair("list-type", "2"),
            pair("prefix", prefix),
            pair("max-keys", "1000"),
        ];
        if let Some(token) = continuation {
            query.push(pair("continuation-token", token));
        }
        let reply = self.send(Method::Get, None, query, Vec::new(), &[], "")?;
        if !reply.is_success() {
            return Err(CloudError::Drive(S3Drive::failure_of(&reply, None)));
        }
        let text = std::str::from_utf8(&reply.body)
            .map_err(|_| DriveError::Protocol(String::from("the listing is not UTF-8")))?;
        Ok(parse_listing(text)?)
    }

    /// Every object under `prefix`, page by page.
    pub fn list_all(&self, prefix: &str) -> CloudResult<Vec<ObjectInfo>> {
        let mut out = Vec::new();
        let mut token: Option<String> = None;
        loop {
            let page = self.list_page(prefix, token.as_deref())?;
            out.extend(page.objects);
            match page.next {
                Some(next) if !next.is_empty() => token = Some(next),
                _ => return Ok(out),
            }
        }
    }

    /// A multipart upload: started, its parts sent several at once, completed; the ETag.
    fn put_multipart(&self, key: &str, data: &[u8]) -> CloudResult<String> {
        let reply = self.send(
            Method::Post,
            Some(key),
            vec![pair("uploads", "")],
            vec![pair("content-type", OCTETS)],
            &[],
            "",
        )?;
        if !reply.is_success() {
            return Err(Self::failure(&reply, key));
        }
        let started = String::from_utf8_lossy(&reply.body).into_owned();
        let upload_id = xml_text(&started, "UploadId")
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .ok_or_else(|| {
                DriveError::Protocol(format!(
                    "{key}: the service started no multipart upload (no UploadId)"
                ))
            })?
            .to_string();
        let parts: Vec<&[u8]> = data.chunks(self.part_size).collect();
        let etags = in_parallel(parts.len(), self.parallel, |i| {
            let query = vec![
                pair("partNumber", (i + 1).to_string()),
                pair("uploadId", upload_id.as_str()),
            ];
            let reply = self.send(Method::Put, Some(key), query, Vec::new(), parts[i], OCTETS)?;
            if !reply.is_success() {
                return Err(Self::failure(&reply, key));
            }
            Ok(reply.header("etag").unwrap_or_default().to_string())
        })?;
        let mut xml = String::from("<CompleteMultipartUpload>");
        for (i, etag) in etags.iter().enumerate() {
            xml.push_str(&format!(
                "<Part><PartNumber>{}</PartNumber><ETag>{}</ETag></Part>",
                i + 1,
                xml_escape(etag)
            ));
        }
        xml.push_str("</CompleteMultipartUpload>");
        let reply = self.send(
            Method::Post,
            Some(key),
            vec![pair("uploadId", upload_id.as_str())],
            Vec::new(),
            xml.as_bytes(),
            "application/xml",
        )?;
        if !reply.is_success() {
            return Err(Self::failure(&reply, key));
        }
        // A 200 can still carry an error: the service answers early and finishes later.
        let done = String::from_utf8_lossy(&reply.body).into_owned();
        if done.contains("<Error>") {
            let failed = HttpReply {
                status: 500,
                headers: reply.headers.clone(),
                body: reply.body.clone(),
            };
            return Err(Self::failure(&failed, key));
        }
        Ok(reply
            .header("etag")
            .map(str::to_string)
            .or_else(|| xml_text(&done, "ETag").map(|e| e.trim().replace("&quot;", "\"")))
            .unwrap_or_default())
    }
}

impl RemoteStore for Bucket {
    fn get_unless(&self, key: &str, etag: Option<&str>) -> CloudResult<Conditional> {
        Bucket::get_unless(self, key, etag)
    }

    fn fetch(&self, key: &str, size: u64) -> CloudResult<Option<Vec<u8>>> {
        if size > BIG_BLOB {
            self.get_big(key)
        } else {
            self.get(key)
        }
    }

    fn put(&self, key: &str, data: &[u8]) -> CloudResult<String> {
        Bucket::put(self, key, data)
    }

    fn put_if(
        &self,
        key: &str,
        data: &[u8],
        if_match: Option<&str>,
    ) -> CloudResult<Option<String>> {
        Bucket::put_if(self, key, data, if_match)
    }

    fn head(&self, key: &str) -> CloudResult<Option<u64>> {
        Ok(Bucket::head(self, key)?.map(|(size, _)| size))
    }

    fn delete(&self, key: &str) -> CloudResult<()> {
        Bucket::delete(self, key)
    }

    fn list(&self, prefix: &str) -> CloudResult<Vec<RemoteObject>> {
        Ok(self
            .list_all(prefix)?
            .into_iter()
            .map(|o| RemoteObject {
                modified: o.modified.and_then(|at| i64::try_from(at).ok()),
                key: o.key,
                size: o.size,
            })
            .collect())
    }
}
