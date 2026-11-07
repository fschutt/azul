//! A data source Apache OpenDAL reaches, as a [`Drive`]: WebDAV, FTP, Google Drive, Dropbox,
//! OneDrive, GitHub, Google Cloud Storage, Azure, ... ([`crate::catalog`] lists the ones this
//! build offers).
//!
//! - **One HTTP stack.** OpenDAL's HTTP services send through the app's [`Transport`]
//!   ([`TransportHttp`], OpenDAL's `HttpTransport` seam): azul's HTTP client and its TLS in the
//!   apps, a fake in the tests - the same seam as the S3 client's. OpenDAL is built without
//!   reqwest. (azul's client sends every verb a drive uses, WebDAV's PROPFIND, PROPPATCH, MKCOL,
//!   COPY and MOVE and OPTIONS included: `AzulTransport` maps them one to one.)
//! - **Blocking, like every drive.** OpenDAL is async: a call runs its future to the end on the
//!   calling thread ([`crate::runtime::block_on`], an azul `Thread`); a request's blocking send
//!   runs on tokio's blocking pool, so no runtime worker ever waits on the network.
//! - **S3 semantics.** A folder listing returns the folders (common prefixes, `docs/`) and the
//!   files directly in it, sorted; a recursive one every file under the prefix and the folders
//!   as markers (a key ending in `/`, as a bucket's empty "folder/" object: a folder with only
//!   empty folders in it still has them, a move takes them along, a delete removes them).
//!   Pages continue after the last key of the page before. OpenDAL streams a folder in one go: the
//!   first page lists it, sorts it and keeps it for a minute, the pages after it are cut from that
//!   copy (they used to list the whole folder again each, 200 calls for 200,000 objects); a
//!   question whether anything is under a folder stops at its first entry.

use std::{
    collections::BTreeMap,
    fmt,
    sync::{Arc, Mutex, PoisonError},
    time::{Duration, Instant},
};

use ::opendal::{
    Buffer, Capability, ErrorKind, HttpBody, HttpTransport, HttpTransporter, Metadata,
    OperationContext, Operator,
};

use crate::{
    config::SecretOptions, key, ops, runtime, scrub::scrub_url, ByteRange, Drive, DriveError,
    HttpCall, HttpReply, ListPage, ListRequest, Method, ObjectInfo, Transport,
};

/// How the message of the error a request without an answer becomes starts (DNS, connection,
/// TLS, a timeout; the transport's sentence follows after ": "): such an error reads as
/// [`DriveError::Transport`].
const NO_ANSWER: &str = "the request got no answer";

/// Headers the HTTP client sets itself: it builds them from the URL and the body.
const CLIENT_HEADERS: &[&str] = &["host", "content-length", "transfer-encoding", "connection"];

// ==== OpenDAL's HTTP through the app's transport ====

/// OpenDAL's `HttpTransport` over a [`Transport`]: one request, one answer, the body in memory.
pub struct TransportHttp {
    transport: Arc<dyn Transport>,
}

impl TransportHttp {
    #[must_use]
    pub fn new(transport: Arc<dyn Transport>) -> Self {
        TransportHttp { transport }
    }
}

/// A request of OpenDAL's as one of the [`Transport`]'s: the verb, the URL, the headers (the
/// content type apart), the body.
fn call_of(request: &http::Request<Buffer>) -> ::opendal::Result<HttpCall> {
    let method = Method::parse(request.method().as_str()).ok_or_else(|| {
        ::opendal::Error::new(
            ErrorKind::Unsupported,
            format!("{} requests cannot be sent", request.method()),
        )
    })?;
    let mut headers = Vec::new();
    let mut content_type = String::new();
    for (name, value) in request.headers() {
        let name = name.as_str();
        if CLIENT_HEADERS.contains(&name) {
            continue;
        }
        let value = String::from_utf8_lossy(value.as_bytes()).into_owned();
        if name == "content-type" {
            content_type = value;
        } else {
            headers.push((name.to_string(), value));
        }
    }
    let body = request.body().to_vec();
    Ok(HttpCall {
        method,
        url: request.uri().to_string(),
        headers,
        limits: crate::CallLimits::for_request(method, body.len()),
        body,
        content_type,
    })
}

/// The [`Transport`]'s answer as OpenDAL's response: the status, the headers that are valid
/// HTTP, the body as one piece (none for HEAD).
fn response_of(
    reply: HttpReply,
    uri: http::Uri,
    head: bool,
) -> ::opendal::Result<http::Response<HttpBody>> {
    let mut builder = http::Response::builder().status(reply.status).extension(uri);
    if let Some(headers) = builder.headers_mut() {
        for (name, value) in &reply.headers {
            if let (Ok(name), Ok(value)) = (
                http::header::HeaderName::from_bytes(name.as_bytes()),
                http::header::HeaderValue::from_str(value),
            ) {
                headers.append(name, value);
            }
        }
    }
    let size = if head {
        None
    } else {
        Some(reply.body.len() as u64)
    };
    let body = if head { Vec::new() } else { reply.body };
    let pieces: Vec<::opendal::Result<Buffer>> = vec![Ok(Buffer::from(body))];
    builder
        .body(HttpBody::new(futures::stream::iter(pieces), size))
        .map_err(|e| {
            ::opendal::Error::new(ErrorKind::Unexpected, "the answer is no HTTP response")
                .set_source(e)
        })
}

impl HttpTransport for TransportHttp {
    async fn fetch(
        &self,
        request: http::Request<Buffer>,
    ) -> ::opendal::Result<http::Response<HttpBody>> {
        let call = call_of(&request)?;
        let head = call.method == Method::Head;
        let uri = request.uri().clone();
        let transport = self.transport.clone();
        // The send blocks (azul's client waits for its answer): on tokio's blocking pool, never
        // on a runtime worker.
        let sent = tokio::task::spawn_blocking(move || transport.send(&call))
            .await
            .map_err(|e| {
                ::opendal::Error::new(
                    ErrorKind::Unexpected,
                    format!("{NO_ANSWER}: the request's thread ended"),
                )
                .set_source(e)
            })?;
        let reply = sent.map_err(|why| {
            // Whatever transport it was: its text may name the URL, and a URL may hold a password
            // (pCloud) or a token (WebHDFS) in its query.
            let why = scrub_url(&why);
            ::opendal::Error::new(ErrorKind::Unexpected, format!("{NO_ANSWER}: {why}"))
                .set_temporary()
        })?;
        response_of(reply, uri, head)
    }
}

// ==== The drive ====

/// How long the pages after the first one are cut from the copy of the folder the first one
/// listed.
const LISTING_FRESH: Duration = Duration::from_secs(60);

/// How much of a file is in memory at once when an upload or a copy of this drive streams it: a
/// copy fallback and `put_from` held the whole file (AZ-CN10).
const STREAM_CHUNK: usize = 8 * 1024 * 1024;

/// Why a streamed transfer stopped: the service, or the reader that supplies the bytes.
enum Streamed {
    Service(::opendal::Error),
    Read(std::io::Error),
}

impl From<::opendal::Error> for Streamed {
    fn from(e: ::opendal::Error) -> Self {
        Streamed::Service(e)
    }
}

impl From<std::io::Error> for Streamed {
    fn from(e: std::io::Error) -> Self {
        Streamed::Read(e)
    }
}

/// A streamed transfer's failure as the drive's.
fn streamed_error(e: Streamed, key: &str) -> DriveError {
    match e {
        Streamed::Service(e) => drive_error(&e, key),
        Streamed::Read(e) => DriveError::from(e),
    }
}

/// A whole listing of one folder, kept for the pages after the first: sorted by key, one entry per
/// key.
struct Listing {
    path: String,
    recursive: bool,
    at: Instant,
    entries: Arc<Vec<Listed>>,
}

/// A data source through OpenDAL. `Debug` names the service, never a setting.
pub struct OpendalDrive {
    op: Operator,
    scheme: String,
    capability: Capability,
    /// The last folder listed ([`LISTING_FRESH`]).
    listing: Mutex<Option<Listing>>,
}

impl fmt::Debug for OpendalDrive {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OpendalDrive")
            .field("scheme", &self.scheme)
            .finish_non_exhaustive()
    }
}

/// An OpenDAL error as the drive's: a missing key, a refusal, a setting that cannot work, a
/// request without an answer, the rest as the service said it.
///
/// The text of an OpenDAL error carries its context, and the context names the URI of the request:
/// pCloud's has `password=...` in it, WebHDFS's the delegation token ([`scrub_url`], AZ-CN3).
fn drive_error(e: &::opendal::Error, key: &str) -> DriveError {
    if let Some(why) = e
        .message()
        .strip_prefix(NO_ANSWER)
        .and_then(|rest| rest.strip_prefix(": "))
    {
        return DriveError::Transport(scrub_url(why));
    }
    let said = || scrub_url(&e.to_string());
    match e.kind() {
        ErrorKind::NotFound => DriveError::NotFound {
            key: key.to_string(),
        },
        ErrorKind::PermissionDenied => DriveError::Denied { message: said() },
        ErrorKind::ConfigInvalid => DriveError::InvalidConfig(said()),
        ErrorKind::Unsupported => DriveError::Unsupported(said()),
        ErrorKind::RangeNotSatisfied => DriveError::InvalidRange {
            key: key.to_string(),
        },
        ErrorKind::RateLimited => DriveError::Transport(said()),
        _ => DriveError::Protocol(said()),
    }
}

/// The object a listing or a stat describes.
fn object_info(key: &str, meta: &Metadata) -> ObjectInfo {
    ObjectInfo {
        key: key.to_string(),
        size: meta.content_length(),
        modified: meta
            .last_modified()
            .and_then(|t| u64::try_from(t.into_inner().as_second()).ok()),
        etag: meta.etag().map(|e| e.trim_matches('"').to_string()),
    }
}

/// One entry of a listing, before it is cut into a page.
#[derive(Clone)]
enum Listed {
    Folder(String),
    Object(ObjectInfo),
}

impl Listed {
    fn key(&self) -> &str {
        match self {
            Listed::Folder(prefix) => prefix,
            Listed::Object(object) => &object.key,
        }
    }
}

/// The page of `request` out of a whole listing, which is sorted by key and holds one entry a
/// key ([`sorted`]): the entries under the request's prefix after the continuation key,
/// `request.page_size()` of them, `next` the last one's key when more follow.
fn page_of(listed: &[Listed], request: &ListRequest) -> ListPage {
    let after = request.continuation.as_deref();
    let size = request.page_size() as usize;
    let mut rest = listed.iter().filter(|entry| {
        entry.key().starts_with(request.prefix.as_str())
            && after.is_none_or(|after| entry.key() > after)
    });
    let mut page = ListPage::default();
    let mut last = None;
    for entry in rest.by_ref().take(size) {
        last = Some(entry.key().to_string());
        match entry {
            Listed::Folder(prefix) => page.folders.push(prefix.clone()),
            Listed::Object(object) => page.objects.push(object.clone()),
        }
    }
    if rest.next().is_some() {
        page.next = last;
    }
    page
}

/// `listed` in key order with one entry a key. Two entries of one name (a service that allows
/// them: Google Drive) are shown once: a key must name one thing to be read, renamed or deleted.
fn sorted(mut listed: Vec<Listed>) -> Vec<Listed> {
    listed.sort_by(|a, b| a.key().cmp(b.key()));
    listed.dedup_by(|a, b| a.key() == b.key());
    listed
}

/// `listed` (sorted) without the folder markers that have something under them: a bucket lists
/// the markers somebody made, and the services that have real folders list every folder they walk
/// into. What is left is an EMPTY folder - the one thing a listing of files cannot show, and the
/// reason a folder that holds only empty folders moved as "not found" and a move left them behind
/// (AZ-CN11). Keys sort so that everything under `docs/` follows `docs/` directly.
fn without_full_folders(listed: Vec<Listed>) -> Vec<Listed> {
    let mut kept = Vec::with_capacity(listed.len());
    for (i, entry) in listed.iter().enumerate() {
        let full = match entry {
            Listed::Object(object) if object.key.ends_with('/') => listed
                .get(i + 1)
                .is_some_and(|next| next.key().starts_with(object.key.as_str())),
            _ => false,
        };
        if !full {
            kept.push(entry.clone());
        }
    }
    kept
}

/// The marker of a folder in a recursive listing: an empty object whose key ends in `/`.
fn folder_marker(key: &str) -> ObjectInfo {
    ObjectInfo {
        key: key.to_string(),
        size: 0,
        modified: None,
        etag: None,
    }
}

/// The settings OpenDAL gets for `scheme`: the form's options and secrets, a switch of the form
/// that is no OpenDAL option made one, and - for a source that would otherwise look for
/// credentials on this computer (Google's login, the environment) - the lookup off when the form
/// chose a credential (AZ-CN9): a key that is mis-pasted must fail, not be replaced by the
/// developer's own login. A source with no explicit credential at all is one saved before the form
/// asked for one, and keeps its lookup.
pub(crate) fn settings_of(
    scheme: &str,
    options: &BTreeMap<String, String>,
    secrets: &SecretOptions,
) -> Vec<(String, String)> {
    let mut settings: Vec<(String, String)> = options
        .iter()
        .filter(|(key, _)| !matches!(key.as_str(), "anonymous" | "machine_identity"))
        .map(|(k, v)| (k.clone(), v.clone()))
        .chain(secrets.iter().map(|(k, v)| (k.to_string(), v.to_string())))
        // The binding of the secrets to their address is ours, not a setting of the service.
        .filter(|(k, v)| k.as_str() != crate::config::ORIGIN_KEY && !v.trim().is_empty())
        .collect();
    let on = |key: &str| options.get(key).is_some_and(|v| v.trim() == "true");
    match scheme {
        "gcs" => {
            let anonymous = on("anonymous");
            let chosen = anonymous || secrets.get("credential").is_some_and(|k| !k.is_empty());
            if anonymous {
                settings.push((String::from("skip_signature"), String::from("true")));
            }
            if chosen && !on("machine_identity") {
                settings.push((String::from("disable_config_load"), String::from("true")));
                settings.push((String::from("disable_vm_metadata"), String::from("true")));
            }
        }
        "cos" => {
            if secrets.get("secret_key").is_some_and(|k| !k.is_empty()) {
                settings.push((String::from("disable_config_load"), String::from("true")));
            }
        }
        _ => {}
    }
    settings
}

/// Whether `scheme` with `options` speaks TLS through a client of its own instead of the
/// transport: FTP over TLS (`ftps://`, or an address without a scheme, which OpenDAL takes
/// as FTPS) and Redis over TLS (`rediss://`) load the OS's roots themselves.
fn brings_own_tls_roots(scheme: &str, options: &BTreeMap<String, String>) -> bool {
    let address = |key: &str| {
        options
            .get(key)
            .map(|value| value.trim().to_ascii_lowercase())
            .unwrap_or_default()
    };
    match scheme {
        "ftp" => !address("endpoint").starts_with("ftp://"),
        "redis" => {
            address("endpoint").starts_with("rediss://")
                || address("cluster_endpoints").contains("rediss://")
        }
        _ => false,
    }
}

impl OpendalDrive {
    /// The OpenDAL service `scheme` with its plain `options` and its `secrets` (both are
    /// OpenDAL's settings of the service), sending HTTP through `transport`. Checks the
    /// settings, sends nothing.
    pub fn open(
        scheme: &str,
        options: &BTreeMap<String, String>,
        secrets: &SecretOptions,
        transport: Box<dyn Transport>,
    ) -> Result<OpendalDrive, DriveError> {
        if !transport.trusts_public_roots() && brings_own_tls_roots(scheme, options) {
            return Err(DriveError::InvalidConfig(format!(
                "the {scheme} source connects over TLS with its own fixed set of trusted roots, \
                 which cannot follow this app's trusted roots (pinned or distrusted ones); \
                 connect without TLS only inside a network you trust"
            )));
        }
        // A user name and a password in an address are kept in drives.json in plain text and shown
        // by every window: they have fields of their own (AZ-CN8).
        if let Some((key, _)) = options
            .iter()
            .find(|(_, value)| crate::catalog::address_has_login(value))
        {
            return Err(DriveError::InvalidConfig(format!(
                "the {scheme} source's \"{key}\" holds a user name or a password: take them out \
                 of the address and fill in the source's own fields"
            )));
        }
        // The services of this build, registered once (no constructor runs before `main`).
        ::opendal::init_default_registry();
        let settings = settings_of(scheme, options, secrets);
        let op = Operator::via_iter(scheme, settings).map_err(|e| {
            DriveError::InvalidConfig(scrub_url(&format!(
                "the {scheme} source cannot be opened: {e}"
            )))
        })?;
        let transport: Arc<dyn Transport> = Arc::from(transport);
        let op = op.with_context(
            OperationContext::new()
                .with_http_transport(HttpTransporter::new(TransportHttp::new(transport))),
        );
        let capability = op.info().capability();
        Ok(OpendalDrive {
            op,
            scheme: scheme.to_string(),
            capability,
            listing: Mutex::new(None),
        })
    }

    /// The OpenDAL service: `webdav`, `gdrive`, ...
    #[must_use]
    pub fn scheme(&self) -> &str {
        &self.scheme
    }

    /// What the service can do, as OpenDAL says.
    #[must_use]
    pub fn capability(&self) -> Capability {
        self.capability
    }
}

impl OpendalDrive {
    /// Every entry under `path`, sorted ([`sorted`]): OpenDAL lists the whole folder; the folder
    /// itself and what the request's partial name excludes are not in it. A recursive listing has
    /// the folders as markers.
    fn list_remote(
        &self,
        path: &str,
        folder: &str,
        recursive: bool,
    ) -> Result<Vec<Listed>, DriveError> {
        let entries =
            runtime::block_on(async { self.op.list_with(path).recursive(recursive).await })?
                .map_err(|e| drive_error(&e, folder))?;
        let mut listed = Vec::with_capacity(entries.len());
        for entry in entries {
            let key = entry.path().trim_start_matches('/');
            // The folder itself (OpenDAL lists the prefix's own entry): not an entry of its own
            // listing - but a recursive one keeps it as the folder's marker, which is all that is
            // left of a folder with nothing in it ([`without_full_folders`] drops it otherwise).
            if key.is_empty() || (key == folder && !recursive) {
                continue;
            }
            if key.ends_with('/') {
                listed.push(if recursive {
                    Listed::Object(folder_marker(key))
                } else {
                    Listed::Folder(key.to_string())
                });
            } else {
                listed.push(Listed::Object(object_info(key, entry.metadata())));
            }
        }
        let listed = sorted(listed);
        Ok(if recursive {
            without_full_folders(listed)
        } else {
            listed
        })
    }

    /// The listing of `path` the pages of one request are cut from: the copy the first page kept
    /// for the ones after it, else the folder listed now (and kept).
    fn listing(
        &self,
        path: &str,
        folder: &str,
        recursive: bool,
        request: &ListRequest,
    ) -> Result<Arc<Vec<Listed>>, DriveError> {
        if request.continuation.is_some() && !request.refresh {
            let kept = self.listing.lock().unwrap_or_else(PoisonError::into_inner);
            if let Some(kept) = kept.as_ref().filter(|kept| {
                kept.path == path
                    && kept.recursive == recursive
                    && kept.at.elapsed() < LISTING_FRESH
            }) {
                return Ok(kept.entries.clone());
            }
        }
        let entries = Arc::new(self.list_remote(path, folder, recursive)?);
        *self.listing.lock().unwrap_or_else(PoisonError::into_inner) = Some(Listing {
            path: path.to_string(),
            recursive,
            at: Instant::now(),
            entries: entries.clone(),
        });
        Ok(entries)
    }

    /// `from` copied to `to` a chunk at a time ([`STREAM_CHUNK`]): the service has no copy of its
    /// own, and reading the file whole into memory to put it again held a whole file (a gigabyte
    /// video) per copy.
    fn copy_streaming(&self, from: &str, to: &str) -> Result<(), DriveError> {
        runtime::block_on(async {
            let size = self.op.stat(from).await?.content_length();
            let reader = self.op.reader(from).await?;
            let mut writer = self.op.writer(to).await?;
            let mut offset = 0u64;
            while offset < size {
                let end = offset.saturating_add(STREAM_CHUNK as u64).min(size);
                let chunk = reader.read(offset..end).await?;
                writer.write(chunk).await?;
                offset = end;
            }
            writer.close().await?;
            Ok::<(), ::opendal::Error>(())
        })?
        .map_err(|e| drive_error(&e, from))
    }

    /// Whether anything is under the request's prefix, asked as `folder_exists` asks it (one key
    /// of a recursive listing): the first entry decides, the rest of the folder is not listed.
    fn first_under(
        &self,
        path: &str,
        folder: &str,
        request: &ListRequest,
    ) -> Result<ListPage, DriveError> {
        use futures::StreamExt;
        let found = runtime::block_on(async {
            let mut lister = self.op.lister_with(path).recursive(true).await?;
            while let Some(entry) = lister.next().await {
                let entry = entry?;
                let key = entry.path().trim_start_matches('/');
                // The folder's own entry counts: a folder with nothing in it is there.
                if key.is_empty() || !key.starts_with(request.prefix.as_str()) {
                    continue;
                }
                return Ok(Some(if key.ends_with('/') {
                    folder_marker(key)
                } else {
                    object_info(key, entry.metadata())
                }));
            }
            Ok::<_, ::opendal::Error>(None)
        })?;
        let found = match found {
            Ok(found) => found,
            // A service with real folders refuses to list one that is not there: nothing is there.
            Err(e) if e.kind() == ErrorKind::NotFound => None,
            Err(e) => return Err(drive_error(&e, folder)),
        };
        Ok(ListPage {
            objects: found.into_iter().collect(),
            ..ListPage::default()
        })
    }
}

impl Drive for OpendalDrive {
    fn list(&self, request: &ListRequest) -> Result<ListPage, DriveError> {
        key::check_path_prefix(&request.prefix)?;
        let folder = key::folder_of(&request.prefix).to_string();
        let recursive = request.delimiter.is_none();
        let path = if folder.is_empty() {
            String::from("/")
        } else {
            folder.clone()
        };
        // "Is anything there?" (`folder_exists`, an emptiness check): the first entry answers.
        if recursive && request.max_keys == 1 && request.continuation.is_none() {
            return self.first_under(&path, &folder, request);
        }
        let entries = self.listing(&path, &folder, recursive, request)?;
        Ok(page_of(&entries, request))
    }

    fn get(&self, key: &str) -> Result<Vec<u8>, DriveError> {
        runtime::block_on(async { self.op.read(key).await })?
            .map(|buffer| buffer.to_vec())
            .map_err(|e| drive_error(&e, key))
    }

    fn get_range(&self, key: &str, range: ByteRange) -> Result<Vec<u8>, DriveError> {
        let read = runtime::block_on(async {
            match range.end {
                Some(end) => self.op.read_with(key).range(range.start..=end).await,
                None => self.op.read_with(key).range(range.start..).await,
            }
        })?;
        read.map(|buffer| buffer.to_vec())
            .map_err(|e| drive_error(&e, key))
    }

    /// What `body` reads, written a chunk at a time ([`STREAM_CHUNK`]); a body that fails midway
    /// leaves the writer unclosed, and the service keeps nothing of it.
    fn put_from(&self, key: &str, body: &mut dyn std::io::Read) -> Result<u64, DriveError> {
        if key.ends_with('/') {
            // A folder marker is an empty put; it is not streamed.
            let mut bytes = Vec::new();
            body.read_to_end(&mut bytes)?;
            self.put(key, &bytes)?;
            return Ok(bytes.len() as u64);
        }
        runtime::block_on(async {
            let mut writer = self.op.writer(key).await?;
            let mut total = 0u64;
            loop {
                let mut chunk = Vec::new();
                let read = std::io::Read::read_to_end(
                    &mut std::io::Read::take(&mut *body, STREAM_CHUNK as u64),
                    &mut chunk,
                )?;
                if read == 0 {
                    break;
                }
                total += read as u64;
                writer.write(Buffer::from(chunk)).await?;
            }
            writer.close().await?;
            Ok::<u64, Streamed>(total)
        })?
        .map_err(|e| streamed_error(e, key))
    }

    fn put(&self, key: &str, bytes: &[u8]) -> Result<(), DriveError> {
        if key.ends_with('/') && bytes.is_empty() {
            // A folder marker (a bucket's empty "folder/" object): a folder here.
            return self.create_folder(key);
        }
        let data = bytes.to_vec();
        runtime::block_on(async { self.op.write(key, data).await })?
            .map(|_| ())
            .map_err(|e| drive_error(&e, key))
    }

    fn delete(&self, key: &str) -> Result<(), DriveError> {
        runtime::block_on(async { self.op.delete(key).await })?.map_err(|e| drive_error(&e, key))
    }

    fn head(&self, key: &str) -> Result<ObjectInfo, DriveError> {
        runtime::block_on(async { self.op.stat(key).await })?
            .map(|meta| object_info(key, &meta))
            .map_err(|e| drive_error(&e, key))
    }

    fn copy(&self, from: &str, to: &str) -> Result<(), DriveError> {
        // A folder's marker (an empty "folder/" a recursive listing shows) copies as a marker: a
        // move of a folder takes its empty folders along.
        if from.ends_with('/') && to.ends_with('/') {
            return self.create_folder(to);
        }
        if let Some(folder) = [from, to].into_iter().find(|k| k.ends_with('/')) {
            return Err(DriveError::InvalidKey {
                key: folder.to_string(),
                reason: "a folder is copied object by object",
            });
        }
        if self.capability.copy {
            return runtime::block_on(async { self.op.copy(from, to).await })?
                .map(|_| ())
                .map_err(|e| drive_error(&e, from));
        }
        self.copy_streaming(from, to)
    }

    fn create_folder(&self, prefix: &str) -> Result<(), DriveError> {
        ops::check_folder(prefix)?;
        if !self.capability.create_dir {
            return Err(DriveError::Unsupported(format!(
                "a {} source has no folders of its own",
                self.scheme
            )));
        }
        runtime::block_on(async { self.op.create_dir(prefix).await })?
            .map_err(|e| drive_error(&e, prefix))
    }

    fn rename(&self, from: &str, to: &str) -> Result<(), DriveError> {
        // The checks of every drive (nothing is replaced, no folder into itself), with the
        // service's own copy where it has one.
        ops::rename_by_copy(self, from, to)
    }

    fn delete_folder(&self, prefix: &str) -> Result<(), DriveError> {
        ops::check_folder(prefix)?;
        if self.capability.delete_with_recursive {
            return runtime::block_on(async { self.op.delete_with(prefix).recursive(true).await })?
                .map_err(|e| drive_error(&e, prefix));
        }
        ops::delete_by_listing(self, prefix)
    }

    fn metadata(&self, key: &str) -> Result<Vec<(String, String)>, DriveError> {
        let meta = runtime::block_on(async { self.op.stat(key).await })?
            .map_err(|e| drive_error(&e, key))?;
        let mut rows = vec![(String::from("Source"), self.scheme.clone())];
        let texts = [
            ("Content type", meta.content_type()),
            ("Content encoding", meta.content_encoding()),
            ("Cache control", meta.cache_control()),
            ("Version", meta.version()),
            ("ETag", meta.etag()),
        ];
        for (name, value) in texts {
            if let Some(value) = value.filter(|v| !v.is_empty()) {
                rows.push((name.to_string(), value.to_string()));
            }
        }
        Ok(rows)
    }
}
