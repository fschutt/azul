//! One bucket of an Azlin drive as the sync and the shares reach it: the built-in
//! [`RemoteStore`], S3 through azul-storage - its SigV4 ([`S3Drive::send_raw`]) and the app's
//! [`azul_storage::Transport`] (azul's HTTP client in the apps, a fake in the tests, whatever
//! the `azcloud` command line plugs in).
//!
//! - Every request goes through the drive's [`Failover`]: the block endpoint first, the nodes an
//!   answer's `x-azlin-alt-endpoints` names (in the same request), the drive's nodes
//!   ([`Bucket::with_nodes`]: the node list of the last refresh, reached at their addresses when
//!   their names do not resolve) and failover URLs ([`Bucket::with_alternatives`]), retried by
//!   the class of the answer (busy: again after a backoff; a pause the node asks for; a drive
//!   that moved; see [`crate::failover`]).
//! - Conditional writes (`If-Match`, `If-None-Match: *`; a 412 says another writer won) and
//!   conditional reads (`If-None-Match`; a 304 says nothing changed): the sync's
//!   compare-and-swap.
//! - An object above twice the part size goes up as a multipart upload (azul-storage's: parts of
//!   16 MiB, four at once, each failing over on its own); one above the part size comes down in
//!   ranges, several at once; a file goes up resumably ([`Bucket::put_file`]) and comes down
//!   into a file ([`Bucket::download_to`]) without the object in memory.
//!
//! Blocking: call it from an azul `Thread`, never from a UI callback.

use std::{fmt, io::Read, path::Path, sync::Arc};

use azul_storage::{
    multipart::in_parallel, s3::parse_listing, transfer, ByteRange, Credentials, DriveError,
    HttpReply, ListPage, Method, ObjectInfo, S3Config, S3Drive,
};

use crate::{
    drive::TransportFactory,
    error::{CloudError, CloudResult},
    failover::{Clock, Failover, Node, Retry, Sleep},
    store::{Conditional, RemoteObject, RemoteStore, BIG_BLOB},
};

/// The part size of a multipart upload and of a ranged download (azul-storage's).
pub const PART_SIZE: usize = azul_storage::s3::PART_SIZE;
/// Parts or ranges of one object in flight at once.
pub const PARALLEL: usize = azul_storage::s3::PARALLEL_PARTS;
/// The content type of every object the bucket writes (the sync's blobs and index, the
/// command line's uploads).
const OCTETS: &str = "application/octet-stream";

/// The pair of a query parameter or a header.
fn pair(name: &str, value: impl Into<String>) -> (String, String) {
    (name.to_string(), value.into())
}

/// An ETag as S3 sends it: in quotes (empty when the service said none).
fn quoted(etag: Option<String>) -> String {
    etag.map(|e| format!("\"{e}\"")).unwrap_or_default()
}

/// One bucket of a drive. `Debug` shows no secret.
pub struct Bucket {
    config: S3Config,
    credentials: Credentials,
    transports: TransportFactory,
    failover: Arc<Failover>,
    /// The bucket, every request through the failover.
    drive: S3Drive,
    /// The block endpoint, asked once (a probe: any answer means the pipe works).
    direct: S3Drive,
    part_size: usize,
    clock: Option<Clock>,
}

impl fmt::Debug for Bucket {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Bucket")
            .field("config", &self.config)
            .field("failover", &self.failover)
            .field("part_size", &self.part_size)
            .field("parallel", &self.parallel())
            .finish_non_exhaustive()
    }
}

/// The routed drive and the direct one of a bucket.
fn open(
    config: &S3Config,
    credentials: &Credentials,
    transports: &TransportFactory,
    failover: &Arc<Failover>,
    clock: Option<&Clock>,
    part_size: usize,
) -> Result<(S3Drive, S3Drive), DriveError> {
    let mut drive = S3Drive::new(config.clone(), credentials.clone(), transports())?
        .with_router(failover.clone())
        .with_part_size(part_size);
    let mut direct = S3Drive::new(config.clone(), credentials.clone(), transports())?;
    if let Some(clock) = clock {
        let signing = clock.clone();
        drive = drive.with_clock(move || signing());
        let signing = clock.clone();
        direct = direct.with_clock(move || signing());
    }
    Ok((drive, direct))
}

impl Bucket {
    /// The bucket `config` (its endpoint is the block endpoint) with `credentials`; its requests
    /// go through transports from `transports`. Refused when the endpoint or the bucket name
    /// cannot be one; sends nothing.
    pub fn new(
        config: S3Config,
        credentials: Credentials,
        transports: TransportFactory,
    ) -> CloudResult<Bucket> {
        let failover = Arc::new(Failover::new(&config.endpoint));
        let (drive, direct) = open(
            &config,
            &credentials,
            &transports,
            &failover,
            None,
            PART_SIZE,
        )?;
        Ok(Bucket {
            config,
            credentials,
            transports,
            failover,
            drive,
            direct,
            part_size: PART_SIZE,
            clock: None,
        })
    }

    /// Opens the drives again (a new clock or part size), keeping how many parts travel at once.
    fn reopen(&mut self) {
        let parallel = self.parallel();
        if let Ok((drive, direct)) = open(
            &self.config,
            &self.credentials,
            &self.transports,
            &self.failover,
            self.clock.as_ref(),
            self.part_size,
        ) {
            drive.set_parallel(parallel);
            self.drive = drive;
            self.direct = direct;
        }
    }

    /// Asks `urls` too, in their order, after the nodes, when the block endpoint does not
    /// answer (the drive's failover URLs).
    #[must_use]
    pub fn with_alternatives(self, urls: Vec<String>) -> Bucket {
        self.failover.add_alternatives(urls);
        self
    }

    /// Asks the drive's nodes (the node list of the last credential refresh) when the block
    /// endpoint does not answer; a node whose name does not resolve at its addresses.
    #[must_use]
    pub fn with_nodes(self, nodes: Vec<Node>) -> Bucket {
        self.failover.set_nodes(nodes);
        self
    }

    /// Tries every request this often, with these pauses.
    #[must_use]
    pub fn with_retry(self, retry: Retry) -> Bucket {
        self.failover.set_retry(retry);
        self
    }

    /// Takes the pauses between retries with `sleep` instead of sleeping (tests).
    #[must_use]
    pub fn with_sleep(self, sleep: Sleep) -> Bucket {
        self.failover.set_sleep(sleep);
        self
    }

    /// Signs with this clock (seconds since 1970) instead of the system's; the failover's
    /// sticky node keeps its time too.
    #[must_use]
    pub fn with_clock(mut self, clock: impl Fn() -> u64 + Send + Sync + 'static) -> Bucket {
        let clock: Clock = Arc::new(clock);
        self.failover.set_clock(clock.clone());
        self.clock = Some(clock);
        self.reopen();
        self
    }

    /// The failover every request of the bucket goes through.
    #[must_use]
    pub fn failover(&self) -> &Arc<Failover> {
        &self.failover
    }

    /// Sets how many parts or ranges of one big object are in flight (at least one).
    pub fn set_parallel(&self, parallel: usize) {
        self.drive.set_parallel(parallel);
    }

    /// How many parts or ranges of one big object are in flight.
    #[must_use]
    pub fn parallel(&self) -> usize {
        self.drive.parallel()
    }

    /// Sets the part size of multipart uploads and ranged downloads (at least one byte; S3
    /// takes parts of 5 MiB and more but the last).
    pub fn set_part_size(&mut self, part_size: usize) {
        self.part_size = part_size.max(1);
        self.reopen();
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

    /// The endpoints after the block endpoint and the nodes, as they are now: the failover
    /// URLs, then what answers named.
    #[must_use]
    pub fn alternatives(&self) -> Vec<String> {
        self.failover.alternatives()
    }

    /// The bucket as an azul-storage drive (its requests through the failover).
    #[must_use]
    pub fn drive(&self) -> &S3Drive {
        &self.drive
    }

    /// One request through the failover. A key S3 cannot take is refused before anything is
    /// sent.
    fn send(
        &self,
        method: Method,
        key: Option<&str>,
        query: Vec<(String, String)>,
        extra: Vec<(String, String)>,
        body: &[u8],
        content_type: &str,
    ) -> Result<HttpReply, DriveError> {
        self.drive
            .send_raw(method, key, query, extra, body.to_vec(), content_type)
    }

    /// The error of a failed answer about `key` (an Azlin node's own code, pause and request ID
    /// in its ServiceError: user_errors words it).
    fn failure(reply: &HttpReply, key: &str) -> CloudError {
        CloudError::Drive(S3Drive::failure_of(reply, Some(key)))
    }

    /// PUT (a multipart upload above twice the part size); the ETag the service answered.
    pub fn put(&self, key: &str, data: &[u8]) -> CloudResult<String> {
        if data.len() > self.part_size.saturating_mul(2) {
            let (_, etag) = self.drive.put_stream(key, &mut &data[..])?;
            return Ok(quoted(etag));
        }
        let reply = self.send(Method::Put, Some(key), Vec::new(), Vec::new(), data, OCTETS)?;
        if !reply.is_success() {
            return Err(Self::failure(&reply, key));
        }
        Ok(reply.header("etag").unwrap_or_default().to_string())
    }

    /// PUT of what `body` reads, to its end: one PUT for a body of one part or less, else a
    /// multipart upload, the parts read a few ahead (never the whole body in memory); the ETag.
    pub fn put_from(&self, key: &str, body: &mut dyn Read) -> CloudResult<String> {
        let (_, etag) = self.drive.put_stream(key, body)?;
        Ok(quoted(etag))
    }

    /// PUT of the local file `path`: its parts several at once, resumable after the app was
    /// killed (azul-storage's resume folder); the bytes sent. `progress` hears the bytes sent so
    /// far, from the parts' threads.
    pub fn put_file(
        &self,
        key: &str,
        path: &Path,
        progress: &(dyn Fn(u64) + Sync),
    ) -> CloudResult<u64> {
        Ok(azul_storage::Drive::put_file(
            &self.drive,
            key,
            path,
            progress,
        )?)
    }

    /// GET of `key` into the file `dest`: ranged GETs several at once into a hidden file next
    /// to it, resumed by the next download of the same version; the bytes written.
    pub fn download_to(
        &self,
        key: &str,
        dest: &Path,
        progress: &mut dyn FnMut(u64),
    ) -> CloudResult<u64> {
        Ok(transfer::download_with_progress(
            &self.drive,
            key,
            None,
            dest,
            self.part_size as u64,
            progress,
        )?)
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
        let parts = in_parallel(ranges.len(), self.parallel(), |i| {
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
        self.direct
            .send_raw(
                Method::Head,
                Some(key),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                "",
            )
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
