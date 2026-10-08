//! A data source Apache OpenDAL reaches, as a [`Drive`]: WebDAV, FTP, Google Drive, Dropbox,
//! OneDrive, GitHub, Google Cloud Storage, Azure, ... ([`crate::catalog`] lists the ones this
//! build offers).
//!
//! - **One HTTP stack.** OpenDAL's HTTP services send through the app's [`Transport`]
//!   ([`TransportHttp`], OpenDAL's `HttpTransport` seam): azul's HTTP client and its TLS in the
//!   apps, a fake in the tests - the same seam as the S3 client's. OpenDAL is built without
//!   reqwest. (azul's client sends GET, HEAD, POST, PUT, PATCH and DELETE; WebDAV's PROPFIND /
//!   MKCOL / COPY / MOVE fail with a sentence until the engine sends any verb.)
//! - **Blocking, like every drive.** OpenDAL is async: a call runs its future to the end on the
//!   calling thread ([`crate::runtime::block_on`], an azul `Thread`); a request's blocking send
//!   runs on tokio's blocking pool, so no runtime worker ever waits on the network.
//! - **S3 semantics.** A folder listing returns the folders (common prefixes, `docs/`) and the
//!   files directly in it, sorted; a recursive one every file under the prefix and no folder.
//!   Pages continue after the last key of the page before (OpenDAL streams a folder in one go;
//!   the page is cut here).

use std::{collections::BTreeMap, fmt, sync::Arc};

use ::opendal::{
    Buffer, Capability, ErrorKind, HttpBody, HttpTransport, HttpTransporter, Metadata,
    OperationContext, Operator,
};

use crate::{
    config::SecretOptions, key, ops, runtime, ByteRange, Drive, DriveError, HttpCall, HttpReply,
    ListPage, ListRequest, Method, ObjectInfo, Transport,
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
    Ok(HttpCall {
        method,
        url: request.uri().to_string(),
        headers,
        body: request.body().to_vec(),
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
            ::opendal::Error::new(ErrorKind::Unexpected, format!("{NO_ANSWER}: {why}"))
                .set_temporary()
        })?;
        response_of(reply, uri, head)
    }
}

// ==== The drive ====

/// A data source through OpenDAL. `Debug` names the service, never a setting.
pub struct OpendalDrive {
    op: Operator,
    scheme: String,
    capability: Capability,
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
fn drive_error(e: &::opendal::Error, key: &str) -> DriveError {
    if let Some(why) = e
        .message()
        .strip_prefix(NO_ANSWER)
        .and_then(|rest| rest.strip_prefix(": "))
    {
        return DriveError::Transport(why.to_string());
    }
    match e.kind() {
        ErrorKind::NotFound => DriveError::NotFound {
            key: key.to_string(),
        },
        ErrorKind::PermissionDenied => DriveError::Denied {
            message: e.to_string(),
        },
        ErrorKind::ConfigInvalid => DriveError::InvalidConfig(e.to_string()),
        ErrorKind::Unsupported => DriveError::Unsupported(e.to_string()),
        ErrorKind::RangeNotSatisfied => DriveError::InvalidRange {
            key: key.to_string(),
        },
        ErrorKind::RateLimited => DriveError::Transport(e.to_string()),
        _ => DriveError::Protocol(e.to_string()),
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

/// The page of `request` out of a whole listing: the entries in key order after the
/// continuation key, `request.page_size()` of them, `next` the last one's key when more follow.
fn page_of(mut listed: Vec<Listed>, request: &ListRequest) -> ListPage {
    listed.sort_by(|a, b| a.key().cmp(b.key()));
    listed.dedup_by(|a, b| a.key() == b.key());
    let after = request.continuation.as_deref();
    let size = request.page_size() as usize;
    let mut rest = listed
        .into_iter()
        .filter(|entry| after.is_none_or(|after| entry.key() > after));
    let mut page = ListPage::default();
    let mut last = None;
    for entry in rest.by_ref().take(size) {
        last = Some(entry.key().to_string());
        match entry {
            Listed::Folder(prefix) => page.folders.push(prefix),
            Listed::Object(object) => page.objects.push(object),
        }
    }
    if rest.next().is_some() {
        page.next = last;
    }
    page
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
        // The services of this build, registered once (no constructor runs before `main`).
        ::opendal::init_default_registry();
        let settings: Vec<(String, String)> = options
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .chain(secrets.iter().map(|(k, v)| (k.to_string(), v.to_string())))
            .filter(|(_, v)| !v.trim().is_empty())
            .collect();
        let op = Operator::via_iter(scheme, settings).map_err(|e| {
            DriveError::InvalidConfig(format!("the {scheme} source cannot be opened: {e}"))
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
        let entries = runtime::block_on(async {
            self.op.list_with(&path).recursive(recursive).await
        })?
        .map_err(|e| drive_error(&e, &folder))?;
        let mut listed = Vec::with_capacity(entries.len());
        for entry in entries {
            let key = entry.path().trim_start_matches('/');
            // The folder itself (OpenDAL lists the prefix's own entry), and what the request's
            // partial name excludes.
            if key.is_empty() || key == folder || !key.starts_with(request.prefix.as_str()) {
                continue;
            }
            if key.ends_with('/') {
                if !recursive {
                    listed.push(Listed::Folder(key.to_string()));
                }
            } else {
                listed.push(Listed::Object(object_info(key, entry.metadata())));
            }
        }
        Ok(page_of(listed, request))
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
        let bytes = self.get(from)?;
        self.put(to, &bytes)
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
            return runtime::block_on(async { self.op.remove_all(prefix).await })?
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
