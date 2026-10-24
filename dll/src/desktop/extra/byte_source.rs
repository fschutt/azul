//! The bytes of a media file, read WHERE THEY ARE - a local file, an HTTP(S) URL, or bytes in
//! memory - through one interface, [`ByteSource`], so a player never needs the whole file before
//! its first frame: the `<video>` worker's demuxer and the audio player's decoder read through it
//! block by block.
//!
//! An HTTP source DOWNLOADS on a thread of its own ([`HttpSource`]), in blocks of [`BLOCK`] by
//! range requests (`Range: bytes=a-b`), a window of [`AHEAD_BLOCKS`] ahead of each of its readers
//! (and [`BEHIND_BLOCKS`] behind them; the rest of the file is not kept), so memory stays a few
//! megabytes for a movie of gigabytes. The container's index at the END of a file (an MP4 whose
//! `moov` follows its `mdat`) is one range request for the tail. A read of bytes that have not
//! arrived asks for them first (they are fetched next, out of order) and then either WAITS for
//! them ([`Wait::Yes`]: the audio decoder, a container header) or answers `WouldBlock` at once
//! ([`Wait::No`]: the video worker, which reports buffering and keeps serving its controls
//! meanwhile). Readers of the same URL in one process - a video's picture and its sound - share
//! ONE download. A server that ignores ranges (`200` with the whole body) is read whole when the
//! file is at most [`WHOLE_LIMIT`]; a failing download is tried again, then says why.
//!
//! The network is a seam ([`RangeFetch`]), so the download is tested without one; the real fetch
//! is azul's HTTP client (`azul_layout::http`, the `http` feature - without it a URL says so).

use std::{
    collections::BTreeMap,
    io,
    sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError, Weak},
    time::{Duration, Instant},
};

use azul_css::AzString;
use azul_layout::http::OptionHttpClient;

/// Bytes a block holds: the unit of the download and of what is kept.
pub(crate) const BLOCK: u64 = 256 * 1024;
/// Blocks a download keeps ahead of each reader (16 MiB).
pub(crate) const AHEAD_BLOCKS: u64 = 64;
/// Blocks kept behind each reader (a short seek back reads them again without a fetch).
pub(crate) const BEHIND_BLOCKS: u64 = 8;
/// Blocks one range request asks for at most (1 MiB).
pub(crate) const RUN_BLOCKS: u64 = 4;
/// How long a waiting read waits for its block before it fails.
pub(crate) const WAIT_LIMIT: Duration = Duration::from_secs(30);
/// The largest file read whole from a server that ignores range requests.
pub(crate) const WHOLE_LIMIT: u64 = 256 * 1024 * 1024;
/// The seconds one range request may take.
const REQUEST_TIMEOUT_S: u64 = 60;

/// The pauses before a failed fetch is tried again; after the last, the download gives up.
#[cfg(not(test))]
const RETRIES: [Duration; 3] = [
    Duration::from_millis(250),
    Duration::from_secs(1),
    Duration::from_secs(3),
];
#[cfg(test)]
const RETRIES: [Duration; 3] = [Duration::from_millis(1); 3];

/// Whether a read waits for bytes that have not arrived yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Wait {
    /// Wait for them (at most [`WAIT_LIMIT`]).
    Yes,
    /// Answer `ErrorKind::WouldBlock` at once.
    No,
}

/// The bytes of a media file, wherever they are. Shared between threads (`&self` reads).
pub(crate) trait ByteSource: Send + Sync {
    /// The length in bytes; `None` while not known (a download before its first answer).
    fn byte_len(&self) -> Option<u64>;

    /// Reads up to `buf.len()` bytes at `offset` (0 at the end of the file). Bytes that have not
    /// arrived are asked for first, then waited for (`Wait::Yes`, at most [`WAIT_LIMIT`]) or
    /// answered with `ErrorKind::WouldBlock` (`Wait::No`).
    fn read_at(&self, offset: u64, buf: &mut [u8], wait: Wait) -> io::Result<usize>;

    /// Whether the bytes `offset..offset + len` are here (a read of them does not wait).
    fn has(&self, offset: u64, len: u64) -> bool;

    /// Where the bytes come from, for messages.
    fn name(&self) -> String;

    /// The length, waiting for it if it is not known yet (the first answer of a download).
    fn wait_len(&self) -> io::Result<u64> {
        if let Some(len) = self.byte_len() {
            return Ok(len);
        }
        let mut probe = [0u8; 1];
        self.read_at(0, &mut probe, Wait::Yes)?;
        self.byte_len().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("{}: the server does not say how long the file is", self.name()),
            )
        })
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Copies what `bytes` holds at `offset` into `buf`; how many bytes.
fn copy_from(bytes: &[u8], offset: u64, buf: &mut [u8]) -> usize {
    let Ok(start) = usize::try_from(offset) else {
        return 0;
    };
    if start >= bytes.len() {
        return 0;
    }
    let n = buf.len().min(bytes.len() - start);
    buf[..n].copy_from_slice(&bytes[start..start + n]);
    n
}

// ---- in memory, on disk ----

/// A file held in memory (`VideoSource::Bytes`, a test).
pub(crate) struct MemorySource {
    bytes: Vec<u8>,
}

impl MemorySource {
    pub(crate) fn new(bytes: Vec<u8>) -> MemorySource {
        MemorySource { bytes }
    }
}

impl ByteSource for MemorySource {
    fn byte_len(&self) -> Option<u64> {
        Some(self.bytes.len() as u64)
    }
    fn read_at(&self, offset: u64, buf: &mut [u8], _wait: Wait) -> io::Result<usize> {
        Ok(copy_from(&self.bytes, offset, buf))
    }
    fn has(&self, _offset: u64, _len: u64) -> bool {
        true
    }
    fn name(&self) -> String {
        String::from("the bytes in memory")
    }
}

/// A local file, read where it is (never whole into memory).
pub(crate) struct FileBytes {
    file: Mutex<std::fs::File>,
    len: u64,
    path: String,
}

impl FileBytes {
    /// Opens the file at `path`, or why not.
    pub(crate) fn open(path: &str) -> Result<FileBytes, String> {
        let file = std::fs::File::open(path).map_err(|e| format!("{path}: {e}"))?;
        let len = file.metadata().map_err(|e| format!("{path}: {e}"))?.len();
        Ok(FileBytes {
            file: Mutex::new(file),
            len,
            path: path.to_string(),
        })
    }
}

impl ByteSource for FileBytes {
    fn byte_len(&self) -> Option<u64> {
        Some(self.len)
    }
    fn read_at(&self, offset: u64, buf: &mut [u8], _wait: Wait) -> io::Result<usize> {
        use std::io::{Read, Seek, SeekFrom};
        let mut file = lock(&self.file);
        file.seek(SeekFrom::Start(offset))?;
        file.read(buf)
    }
    fn has(&self, _offset: u64, _len: u64) -> bool {
        true
    }
    fn name(&self) -> String {
        self.path.clone()
    }
}

// ---- a std reader over a source ----

/// `Read + Seek` over a [`ByteSource`], for the parsers that want one (the `mp4` crate, Symphonia):
/// waiting for bytes that have not arrived, or answering `WouldBlock`, as `wait` says.
pub(crate) struct SourceReader {
    source: Arc<dyn ByteSource>,
    pos: u64,
    wait: Wait,
}

impl SourceReader {
    pub(crate) fn new(source: Arc<dyn ByteSource>, wait: Wait) -> SourceReader {
        SourceReader {
            source,
            pos: 0,
            wait,
        }
    }

    /// The source read.
    pub(crate) fn source(&self) -> &Arc<dyn ByteSource> {
        &self.source
    }
}

impl io::Read for SourceReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.source.read_at(self.pos, buf, self.wait)?;
        self.pos += n as u64;
        Ok(n)
    }
}

impl io::Seek for SourceReader {
    fn seek(&mut self, to: io::SeekFrom) -> io::Result<u64> {
        let target = match to {
            io::SeekFrom::Start(n) => Some(n),
            io::SeekFrom::Current(delta) => self.pos.checked_add_signed(delta),
            io::SeekFrom::End(delta) => self
                .source
                .byte_len()
                .and_then(|len| len.checked_add_signed(delta)),
        };
        let Some(target) = target else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "a seek before the start of the file, or from an end not known yet",
            ));
        };
        self.pos = target;
        Ok(target)
    }
}

// ---- over HTTP ----

/// One answer to a range request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Fetched {
    pub body: Vec<u8>,
    /// The whole file's length, when the answer says it (`Content-Range: bytes a-b/total`).
    pub total: Option<u64>,
    /// The server answered with the WHOLE file (it ignores ranges).
    pub whole: bool,
}

/// Fetches byte ranges of one file: the network seam of [`HttpSource`].
pub(crate) trait RangeFetch: Send + Sync {
    /// The bytes `start..=end` of the file (fewer at its end), or why not.
    fn fetch(&self, start: u64, end: u64) -> Result<Fetched, String>;
}

/// The total of a `Content-Range` value (`bytes 0-1023/2070701`); `None` for `*`.
pub(crate) fn range_total(value: &str) -> Option<u64> {
    value.rsplit('/').next()?.trim().parse().ok()
}

/// What an answer to a range request says: `206` is the part asked for (`Content-Range` tells
/// the file's length), `200` the whole file (the server ignores ranges), `416` nothing past the
/// end; anything else failed.
pub(crate) fn answer(
    status: u16,
    content_range: Option<&str>,
    body: Vec<u8>,
) -> Result<Fetched, String> {
    match status {
        206 => Ok(Fetched {
            total: content_range.and_then(range_total),
            body,
            whole: false,
        }),
        200..=299 => Ok(Fetched {
            total: Some(body.len() as u64),
            body,
            whole: true,
        }),
        416 => Ok(Fetched {
            total: content_range.and_then(range_total),
            body: Vec::new(),
            whole: false,
        }),
        _ => Err(format!("the server answered {status}")),
    }
}

/// [`RangeFetch`] over azul's HTTP client: `GET` with `Range: bytes=a-b`, through one
/// connection pool for the whole download (every block reuses the connection, and the TLS
/// session, of the last).
pub(crate) struct HttpFetch {
    url: String,
    config: azul_layout::http::HttpRequestConfig,
}

impl HttpFetch {
    pub(crate) fn new(url: &str, client: &OptionHttpClient) -> HttpFetch {
        use azul_layout::http::{HttpClient, HttpClientConfig, HttpRequestConfig};
        let client = match client {
            OptionHttpClient::Some(c) => c.clone(),
            OptionHttpClient::None => HttpClient::create(HttpClientConfig::new()),
        };
        HttpFetch {
            url: url.to_string(),
            config: HttpRequestConfig::new()
                .with_timeout(REQUEST_TIMEOUT_S)
                .with_max_size(WHOLE_LIMIT)
                .with_client(client),
        }
    }
}

impl RangeFetch for HttpFetch {
    fn fetch(&self, start: u64, end: u64) -> Result<Fetched, String> {
        use azul_layout::http::ResultHttpResponseHttpError;
        let config = self
            .config
            .clone()
            .with_header("Range", format!("bytes={start}-{end}"));
        match config.http_get_blocking(AzString::from(self.url.clone())) {
            ResultHttpResponseHttpError::Ok(response) => {
                let content_range = response
                    .headers
                    .as_slice()
                    .iter()
                    .find(|h| h.name.as_str().eq_ignore_ascii_case("content-range"))
                    .map(|h| h.value.as_str().to_string());
                answer(
                    response.status_code,
                    content_range.as_deref(),
                    response.body.as_slice().to_vec(),
                )
            }
            ResultHttpResponseHttpError::Err(e) => Err(e.to_string()),
        }
    }
}

/// A file at a URL, downloaded on a thread of its own a window ahead of its readers. Each handle
/// is one reader; the handles of one URL share the download (see the module docs).
pub(crate) struct HttpSource {
    shared: Arc<Shared>,
    /// This handle's reader: where it reads, the download runs ahead of.
    reader: u64,
}

/// What a download and its readers share.
struct Shared {
    store: Mutex<Store>,
    wake: Condvar,
    name: String,
}

/// The blocks that arrived, and what the download does next.
#[derive(Default)]
struct Store {
    len: Option<u64>,
    blocks: BTreeMap<u64, Arc<Vec<u8>>>,
    /// The whole file, from a server that ignores ranges.
    whole: Option<Arc<Vec<u8>>>,
    /// The block each reader is at.
    readers: BTreeMap<u64, u64>,
    next_reader: u64,
    /// Blocks readers wait for: fetched before anything else.
    wanted: Vec<u64>,
    error: Option<String>,
    /// No reader is left: the download stops.
    quit: bool,
}

/// The downloads running in this process, by URL: a second reader of a URL joins its download.
static DOWNLOADS: Mutex<Vec<(String, Weak<Shared>)>> = Mutex::new(Vec::new());

impl HttpSource {
    /// A reader of `url` from its start, joining the URL's download when one runs in this
    /// process, else starting one (through `client`'s connection pool, or one of its own).
    pub(crate) fn open(url: &str, client: &OptionHttpClient) -> HttpSource {
        let client = client.clone();
        Self::shared(url, move || {
            Arc::new(HttpFetch::new(url, &client)) as Arc<dyn RangeFetch>
        })
    }

    /// A reader of the download named `name`, joining it when it runs, else starting it through
    /// the fetcher `make` makes.
    pub(crate) fn shared(name: &str, make: impl FnOnce() -> Arc<dyn RangeFetch>) -> HttpSource {
        let mut downloads = lock(&DOWNLOADS);
        downloads.retain(|(_, d)| d.strong_count() > 0);
        let running = downloads
            .iter()
            .find(|(n, _)| n == name)
            .and_then(|(_, d)| d.upgrade());
        if let Some(shared) = running {
            // A download that failed or stopped is not joined: this reader starts its own.
            let joined = {
                let mut store = lock(&shared.store);
                (!store.quit && store.error.is_none()).then(|| store.add_reader())
            };
            if let Some(reader) = joined {
                return HttpSource { shared, reader };
            }
        }
        let source = HttpSource::with_fetch(name, make());
        downloads.retain(|(n, _)| n != name);
        downloads.push((name.to_string(), Arc::downgrade(&source.shared)));
        source
    }

    /// A reader of a download of its own through `fetch` (not shared).
    pub(crate) fn with_fetch(name: &str, fetch: Arc<dyn RangeFetch>) -> HttpSource {
        let shared = Arc::new(Shared {
            store: Mutex::new(Store::default()),
            wake: Condvar::new(),
            name: name.to_string(),
        });
        let reader = lock(&shared.store).add_reader();
        let for_thread = Arc::clone(&shared);
        let spawned = std::thread::Builder::new()
            .name(String::from("azul-media-download"))
            .spawn(move || download(&for_thread, fetch.as_ref()));
        if let Err(e) = spawned {
            lock(&shared.store).error = Some(format!("{name}: the download did not start ({e})"));
        }
        HttpSource { shared, reader }
    }
}

impl Drop for HttpSource {
    fn drop(&mut self) {
        let mut store = lock(&self.shared.store);
        store.readers.remove(&self.reader);
        if store.readers.is_empty() {
            store.quit = true;
        }
        drop(store);
        self.shared.wake.notify_all();
    }
}

impl Store {
    /// A new reader, at the start of the file.
    fn add_reader(&mut self) -> u64 {
        let id = self.next_reader;
        self.next_reader += 1;
        self.readers.insert(id, 0);
        id
    }

    /// The blocks of the file (`None` while its length is not known).
    fn block_count(&self) -> Option<u64> {
        self.len.map(|len| len.div_ceil(BLOCK))
    }

    /// Whether block `b` is a block of the file that has not arrived.
    fn missing(&self, b: u64) -> bool {
        self.block_count().is_some_and(|n| b < n) && !self.blocks.contains_key(&b)
    }

    /// The next run of blocks to fetch: from the start while the length is not known; then a
    /// block a reader waits for; then the nearest block missing in a reader's window.
    fn next_run(&self) -> Option<(u64, u64)> {
        if self.len.is_none() {
            return Some((0, RUN_BLOCKS));
        }
        let blocks = self.block_count()?;
        let first = self
            .wanted
            .iter()
            .copied()
            .find(|b| self.missing(*b))
            .or_else(|| {
                self.readers
                    .values()
                    .filter_map(|&r| (r..(r + AHEAD_BLOCKS).min(blocks)).find(|b| self.missing(*b)))
                    .min()
            })?;
        let mut count = 1;
        while count < RUN_BLOCKS && self.missing(first + count) {
            count += 1;
        }
        Some((first, count))
    }

    /// An answer for the run `first..first + count` arrived.
    fn take(&mut self, first: u64, count: u64, fetched: Fetched) {
        if fetched.whole {
            self.len = Some(fetched.body.len() as u64);
            self.whole = Some(Arc::new(fetched.body));
            self.blocks.clear();
            self.wanted.clear();
            return;
        }
        if let Some(total) = fetched.total {
            self.len = Some(total);
        }
        let got = fetched.body.len() as u64;
        if fetched.total.is_none() && got < count * BLOCK {
            // No length in the answer (`bytes a-b/*`): a short answer is the end of the file.
            self.len = Some(first * BLOCK + got);
        }
        let len = self.len.unwrap_or(u64::MAX);
        let mut at = 0u64;
        let mut block = first;
        while at < got {
            let n = BLOCK.min(got - at);
            // Only whole blocks, or the file's last: a short block in the middle is fetched again.
            if n == BLOCK || block * BLOCK + n >= len {
                let start = usize::try_from(at).unwrap_or(usize::MAX);
                let end = usize::try_from(at + n).unwrap_or(usize::MAX);
                self.blocks
                    .insert(block, Arc::new(fetched.body[start..end].to_vec()));
            }
            at += n;
            block += 1;
        }
        let blocks = &self.blocks;
        self.wanted.retain(|b| !blocks.contains_key(b));
        self.evict();
    }

    /// Drops the blocks no reader is near (a window ahead and a little behind each).
    fn evict(&mut self) {
        let readers: Vec<u64> = self.readers.values().copied().collect();
        let near = |b: u64| {
            readers
                .iter()
                .any(|&r| b + BEHIND_BLOCKS >= r && b <= r + AHEAD_BLOCKS + RUN_BLOCKS)
        };
        let far: Vec<u64> = self
            .blocks
            .keys()
            .copied()
            .filter(|b| !near(*b) && !self.wanted.contains(b))
            .collect();
        for b in far {
            self.blocks.remove(&b);
        }
    }
}

/// The download thread: fetches the runs [`Store::next_run`] picks until no reader is left,
/// the whole file arrived, or the download failed for good.
fn download(shared: &Shared, fetch: &dyn RangeFetch) {
    loop {
        let (first, count) = {
            let mut store = lock(&shared.store);
            loop {
                if store.quit || store.whole.is_some() || store.error.is_some() {
                    return;
                }
                if let Some(run) = store.next_run() {
                    break run;
                }
                store = match shared.wake.wait_timeout(store, Duration::from_millis(500)) {
                    Ok((guard, _)) => guard,
                    Err(poisoned) => poisoned.into_inner().0,
                };
            }
        };
        let start = first * BLOCK;
        let end = start + count * BLOCK - 1;
        let fetched = fetch_again_and_again(shared, fetch, start, end);
        let mut store = lock(&shared.store);
        match fetched {
            Ok(fetched) => store.take(first, count, fetched),
            Err(why) => store.error = Some(format!("{}: {why}", shared.name)),
        }
        drop(store);
        shared.wake.notify_all();
    }
}

/// One fetch, tried again after each pause of [`RETRIES`] while a reader is left.
fn fetch_again_and_again(
    shared: &Shared,
    fetch: &dyn RangeFetch,
    start: u64,
    end: u64,
) -> Result<Fetched, String> {
    let mut last = fetch.fetch(start, end);
    for pause in RETRIES {
        if last.is_ok() || lock(&shared.store).quit {
            break;
        }
        std::thread::sleep(pause);
        last = fetch.fetch(start, end);
    }
    last
}

impl ByteSource for HttpSource {
    fn byte_len(&self) -> Option<u64> {
        let store = lock(&self.shared.store);
        store
            .whole
            .as_ref()
            .map(|w| w.len() as u64)
            .or(store.len)
    }

    fn read_at(&self, offset: u64, buf: &mut [u8], wait: Wait) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        let block = offset / BLOCK;
        let deadline = Instant::now() + WAIT_LIMIT;
        let mut store = lock(&self.shared.store);
        loop {
            if let Some(whole) = &store.whole {
                return Ok(copy_from(whole, offset, buf));
            }
            if store.len.is_some_and(|len| offset >= len) {
                return Ok(0);
            }
            // The reader is here now: the download's window follows it.
            let moved = store.readers.insert(self.reader, block) != Some(block);
            if let Some(data) = store.blocks.get(&block) {
                let in_block = usize::try_from(offset - block * BLOCK).unwrap_or(usize::MAX);
                let n = if in_block < data.len() {
                    let n = buf.len().min(data.len() - in_block);
                    buf[..n].copy_from_slice(&data[in_block..in_block + n]);
                    n
                } else {
                    0
                };
                drop(store);
                if moved {
                    self.shared.wake.notify_all();
                }
                return Ok(n);
            }
            if let Some(why) = &store.error {
                return Err(io::Error::other(why.clone()));
            }
            if store.quit {
                return Err(io::Error::other(format!(
                    "{}: the download stopped",
                    self.shared.name
                )));
            }
            if !store.wanted.contains(&block) {
                store.wanted.push(block);
            }
            self.shared.wake.notify_all();
            if wait == Wait::No {
                return Err(io::Error::from(io::ErrorKind::WouldBlock));
            }
            let now = Instant::now();
            if now >= deadline {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    format!("{}: the download stalled", self.shared.name),
                ));
            }
            store = match self.shared.wake.wait_timeout(store, deadline - now) {
                Ok((guard, _)) => guard,
                Err(poisoned) => poisoned.into_inner().0,
            };
        }
    }

    fn has(&self, offset: u64, len: u64) -> bool {
        let store = lock(&self.shared.store);
        if store.whole.is_some() {
            return true;
        }
        if len == 0 {
            return true;
        }
        let end = offset.saturating_add(len);
        let end = store.len.map_or(end, |l| end.min(l));
        if end <= offset {
            return store.len.is_some();
        }
        (offset / BLOCK..=(end - 1) / BLOCK).all(|b| store.blocks.contains_key(&b))
    }

    fn name(&self) -> String {
        self.shared.name.clone()
    }
}

/// A source for a media file named by a URL (`http://`, `https://`) or a path (anything else).
pub(crate) fn open_source(
    location: &str,
    client: &OptionHttpClient,
) -> Result<Arc<dyn ByteSource>, String> {
    let lower = location.to_ascii_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") {
        Ok(Arc::new(HttpSource::open(location, client)))
    } else {
        Ok(Arc::new(FileBytes::open(location)?))
    }
}

/// A one-file HTTP/1.1 server on 127.0.0.1 for the tests: keep-alive, `Range` answered with
/// `206`, or every request with `200` and the whole body when `ranges` is off.
#[cfg(test)]
pub(crate) mod test_server {
    use std::{
        io::{BufRead, BufReader, Write},
        sync::{
            atomic::{AtomicUsize, Ordering},
            Arc, Mutex,
        },
    };

    /// Serves `bytes` on a port of its own; the port and the count of requests answered.
    pub(crate) fn serve(bytes: Vec<u8>, ranges: bool) -> (u16, Arc<AtomicUsize>) {
        let (port, hits, _) = serve_recording(bytes, ranges);
        (port, hits)
    }

    /// [`serve`], also recording the value of every `Range` header asked for
    /// (`"0-1048575"`, `"0-"`).
    pub(crate) fn serve_recording(
        bytes: Vec<u8>,
        ranges: bool,
    ) -> (u16, Arc<AtomicUsize>, Arc<Mutex<Vec<String>>>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a port");
        let port = listener.local_addr().expect("an address").port();
        let bytes = Arc::new(bytes);
        let hits = Arc::new(AtomicUsize::new(0));
        let asked = Arc::new(Mutex::new(Vec::new()));
        let counted = Arc::clone(&hits);
        let recorded = Arc::clone(&asked);
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else {
                    return;
                };
                let bytes = Arc::clone(&bytes);
                let hits = Arc::clone(&counted);
                let asked = Arc::clone(&recorded);
                std::thread::spawn(move || answer_requests(stream, &bytes, ranges, &hits, &asked));
            }
        });
        (port, hits, asked)
    }

    /// Answers the requests of one connection until it closes.
    fn answer_requests(
        stream: std::net::TcpStream,
        bytes: &[u8],
        ranges: bool,
        hits: &AtomicUsize,
        asked: &Mutex<Vec<String>>,
    ) {
        let Ok(read_half) = stream.try_clone() else {
            return;
        };
        let mut reader = BufReader::new(read_half);
        let mut stream = stream;
        loop {
            let mut request_line = String::new();
            if reader.read_line(&mut request_line).unwrap_or(0) == 0 {
                return;
            }
            let mut range: Option<(u64, Option<u64>)> = None;
            loop {
                let mut header = String::new();
                if reader.read_line(&mut header).unwrap_or(0) == 0 {
                    return;
                }
                let header = header.trim_end().to_ascii_lowercase();
                if header.is_empty() {
                    break;
                }
                if let Some(spec) = header.strip_prefix("range: bytes=") {
                    asked
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .push(spec.trim().to_string());
                    let (a, b) = spec.split_once('-').unwrap_or((spec, ""));
                    range = a.trim().parse().ok().map(|a| (a, b.trim().parse().ok()));
                }
            }
            hits.fetch_add(1, Ordering::SeqCst);
            let total = bytes.len() as u64;
            let head;
            let body: &[u8];
            match range.filter(|_| ranges) {
                Some((start, end)) if start < total => {
                    let end = end.unwrap_or(total - 1).min(total - 1);
                    body = &bytes[start as usize..=end as usize];
                    head = format!(
                        "HTTP/1.1 206 Partial Content\r\nContent-Type: video/mp4\r\n\
                         Content-Length: {}\r\nContent-Range: bytes {start}-{end}/{total}\r\n\
                         Accept-Ranges: bytes\r\n\r\n",
                        body.len()
                    );
                }
                Some(_) => {
                    body = &[];
                    head = format!(
                        "HTTP/1.1 416 Range Not Satisfiable\r\nContent-Length: 0\r\n\
                         Content-Range: bytes */{total}\r\n\r\n"
                    );
                }
                None => {
                    body = bytes;
                    head = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: video/mp4\r\nContent-Length: {}\r\n\r\n",
                        body.len()
                    );
                }
            }
            if stream.write_all(head.as_bytes()).is_err() || stream.write_all(body).is_err() {
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        io::{Read, Seek, SeekFrom},
        sync::atomic::{AtomicUsize, Ordering},
    };

    use super::*;

    /// A file whose byte `i` is `(i * 7 + i / 251) as u8`: any misplaced read shows.
    fn pattern(len: usize) -> Vec<u8> {
        (0..len).map(|i| (i * 7 + i / 251) as u8).collect()
    }

    /// A server in memory: answers ranges (or the whole file), records what was asked, takes
    /// `delay` per answer.
    struct FakeServer {
        bytes: Vec<u8>,
        ranges: bool,
        delay: Duration,
        asked: Mutex<Vec<(u64, u64)>>,
    }

    impl FakeServer {
        fn new(bytes: Vec<u8>, ranges: bool, delay: Duration) -> Arc<FakeServer> {
            Arc::new(FakeServer {
                bytes,
                ranges,
                delay,
                asked: Mutex::new(Vec::new()),
            })
        }
        fn asked(&self) -> Vec<(u64, u64)> {
            lock(&self.asked).clone()
        }
    }

    impl RangeFetch for FakeServer {
        fn fetch(&self, start: u64, end: u64) -> Result<Fetched, String> {
            std::thread::sleep(self.delay);
            lock(&self.asked).push((start, end));
            let total = self.bytes.len() as u64;
            if !self.ranges {
                return Ok(Fetched {
                    body: self.bytes.clone(),
                    total: Some(total),
                    whole: true,
                });
            }
            if start >= total {
                return Ok(Fetched {
                    body: Vec::new(),
                    total: Some(total),
                    whole: false,
                });
            }
            let end = end.min(total - 1);
            Ok(Fetched {
                body: self.bytes[start as usize..=end as usize].to_vec(),
                total: Some(total),
                whole: false,
            })
        }
    }

    /// A fetch that always fails.
    struct Broken;

    impl RangeFetch for Broken {
        fn fetch(&self, _start: u64, _end: u64) -> Result<Fetched, String> {
            Err(String::from("connection refused"))
        }
    }

    /// Reads `len` bytes at `offset`, waiting for them.
    fn read(source: &dyn ByteSource, offset: u64, len: usize) -> Vec<u8> {
        let mut out = vec![0u8; len];
        let mut done = 0;
        while done < len {
            let n = source
                .read_at(offset + done as u64, &mut out[done..], Wait::Yes)
                .expect("the bytes arrive");
            assert!(n > 0, "the file ended early at {}", offset + done as u64);
            done += n;
        }
        out
    }

    /// Waits (at most two seconds) until `check` holds.
    fn eventually(what: &str, check: impl Fn() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(2);
        while !check() {
            assert!(Instant::now() < deadline, "never: {what}");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn memory_and_file_sources_read_at_any_offset_and_say_their_length() {
        let bytes = pattern(10_000);
        let memory = MemorySource::new(bytes.clone());
        assert_eq!(memory.byte_len(), Some(10_000));
        assert_eq!(read(&memory, 4_321, 100), bytes[4_321..4_421].to_vec());
        let mut past = [0u8; 4];
        assert_eq!(memory.read_at(10_000, &mut past, Wait::No).unwrap(), 0, "the end");

        let path = std::env::temp_dir().join(format!("azul-bytes-{}.bin", std::process::id()));
        std::fs::write(&path, &bytes).expect("written");
        let file = FileBytes::open(&path.to_string_lossy()).expect("opens");
        assert_eq!(file.byte_len(), Some(10_000));
        assert!(file.has(0, 10_000));
        assert_eq!(read(&file, 9_000, 1_000), bytes[9_000..].to_vec());
        let _ = std::fs::remove_file(&path);
        assert!(FileBytes::open("/nonexistent/clip.mp4").is_err());
    }

    #[test]
    fn a_reader_seeks_from_the_start_the_current_place_and_the_end() {
        let bytes = pattern(1_000);
        let mut r = SourceReader::new(Arc::new(MemorySource::new(bytes.clone())), Wait::Yes);
        let mut buf = [0u8; 10];
        r.seek(SeekFrom::End(-10)).unwrap();
        r.read_exact(&mut buf).unwrap();
        assert_eq!(&buf[..], &bytes[990..]);
        r.seek(SeekFrom::Start(100)).unwrap();
        r.seek(SeekFrom::Current(-50)).unwrap();
        r.read_exact(&mut buf).unwrap();
        assert_eq!(&buf[..], &bytes[50..60]);
        assert!(r.seek(SeekFrom::Current(-1_000)).is_err(), "before the start");
    }

    #[test]
    fn answers_to_range_requests_are_read_by_their_status() {
        assert_eq!(range_total("bytes 0-1023/2070701"), Some(2_070_701));
        assert_eq!(range_total("bytes */500"), Some(500));
        assert_eq!(range_total("bytes 0-1/*"), None);
        let part = answer(206, Some("bytes 0-3/10"), vec![1, 2, 3, 4]).unwrap();
        assert_eq!(part.total, Some(10));
        assert!(!part.whole);
        let whole = answer(200, None, vec![9; 10]).unwrap();
        assert!(whole.whole, "a 200 is the whole file");
        assert_eq!(whole.total, Some(10));
        let past = answer(416, Some("bytes */10"), Vec::new()).unwrap();
        assert!(past.body.is_empty() && past.total == Some(10));
        let missing = answer(404, None, Vec::new()).unwrap_err();
        assert!(missing.contains("404"), "{missing}");
    }

    #[test]
    fn a_download_learns_the_length_first_and_runs_a_window_ahead_of_its_reader() {
        // 40 MiB: more than the window.
        let bytes = pattern((40 * 1024 * 1024) as usize);
        let server = FakeServer::new(bytes.clone(), true, Duration::ZERO);
        let source = HttpSource::with_fetch("fake", server.clone());
        assert_eq!(source.wait_len().expect("a length"), bytes.len() as u64);
        assert_eq!(read(&source, 1_000, 100), bytes[1_000..1_100].to_vec());
        // The download runs ahead of the reader ...
        eventually("the window ahead arrives", || {
            source.has((AHEAD_BLOCKS - 1) * BLOCK, BLOCK)
        });
        // ... in range requests of at most RUN_BLOCKS blocks, and stops a window ahead.
        std::thread::sleep(Duration::from_millis(100));
        assert!(
            !source.has(120 * BLOCK, 1),
            "nothing far past the window is fetched"
        );
        for (start, end) in server.asked() {
            assert!(end >= start && end - start < RUN_BLOCKS * BLOCK, "{start}-{end}");
            assert_eq!(start % BLOCK, 0, "requests start at a block");
        }
        // The reader moves on: so does the download (and what is far behind it goes).
        assert_eq!(
            read(&source, 100 * BLOCK + 5, 300),
            bytes[(100 * BLOCK + 5) as usize..(100 * BLOCK + 305) as usize].to_vec()
        );
        eventually("the window moves with the reader", || {
            source.has(150 * BLOCK, BLOCK)
        });
        assert!(!source.has(0, 1), "far behind the reader, the start is not kept");
    }

    #[test]
    fn a_read_of_bytes_not_here_yet_says_would_block_or_waits_for_them() {
        let bytes = pattern((4 * 1024 * 1024) as usize);
        let server = FakeServer::new(bytes.clone(), true, Duration::from_millis(60));
        let source = HttpSource::with_fetch("slow", server.clone());
        let offset = 3 * 1024 * 1024 + 17;
        let mut buf = [0u8; 64];
        let err = source
            .read_at(offset, &mut buf, Wait::No)
            .expect_err("nothing has arrived");
        assert_eq!(err.kind(), io::ErrorKind::WouldBlock);
        assert_eq!(read(&source, offset, 64), bytes[offset as usize..offset as usize + 64].to_vec());
        assert!(
            server.asked().iter().any(|(start, _)| *start == (offset / BLOCK) * BLOCK),
            "the block waited for is fetched out of order: {:?}",
            server.asked()
        );
    }

    #[test]
    fn a_server_without_ranges_is_read_whole_and_a_failing_one_says_why() {
        let bytes = pattern(1_000_000);
        let source = HttpSource::with_fetch("no ranges", FakeServer::new(bytes.clone(), false, Duration::ZERO));
        assert_eq!(read(&source, 500_000, 10), bytes[500_000..500_010].to_vec());
        assert_eq!(source.byte_len(), Some(1_000_000));
        assert!(source.has(0, 1_000_000), "the whole file is here");

        let broken = HttpSource::with_fetch("broken", Arc::new(Broken));
        let mut buf = [0u8; 8];
        let err = broken.read_at(0, &mut buf, Wait::Yes).expect_err("no file");
        assert!(err.to_string().contains("connection refused"), "{err}");
    }

    #[test]
    fn the_readers_of_one_url_share_one_download() {
        let bytes = pattern((2 * 1024 * 1024) as usize);
        let server = FakeServer::new(bytes.clone(), true, Duration::ZERO);
        let made = Arc::new(AtomicUsize::new(0));
        let make = |made: &Arc<AtomicUsize>, server: &Arc<FakeServer>| {
            let made = Arc::clone(made);
            let server = Arc::clone(server);
            move || {
                made.fetch_add(1, Ordering::SeqCst);
                server as Arc<dyn RangeFetch>
            }
        };
        let name = format!("shared-{}", std::process::id());
        let picture = HttpSource::shared(&name, make(&made, &server));
        let sound = HttpSource::shared(&name, make(&made, &server));
        assert_eq!(made.load(Ordering::SeqCst), 1, "one download for both readers");
        assert_eq!(read(&picture, 10, 10), bytes[10..20].to_vec());
        assert_eq!(read(&sound, 1_500_000, 10), bytes[1_500_000..1_500_010].to_vec());
        let asked = server.asked();
        let mut starts: Vec<u64> = asked.iter().map(|(s, _)| *s).collect();
        starts.sort_unstable();
        starts.dedup();
        assert_eq!(starts.len(), asked.len(), "no block fetched twice: {asked:?}");
    }

    #[cfg(feature = "http")]
    #[test]
    fn a_url_is_read_by_range_requests_over_http() {
        let bytes = pattern(3 * 1024 * 1024 + 123);
        let (port, hits) = test_server::serve(bytes.clone(), true);
        let url = format!("http://127.0.0.1:{port}/clip.mp4");
        let source = open_source(&url, &OptionHttpClient::None).expect("a source");
        assert_eq!(source.wait_len().expect("a length"), bytes.len() as u64);
        let mut r = SourceReader::new(source, Wait::Yes);
        r.seek(SeekFrom::End(-123)).unwrap();
        let mut tail = vec![0u8; 123];
        r.read_exact(&mut tail).unwrap();
        assert_eq!(tail, bytes[bytes.len() - 123..].to_vec(), "the tail, by a range request");
        r.seek(SeekFrom::Start(1_000_000)).unwrap();
        let mut middle = vec![0u8; 70_000];
        r.read_exact(&mut middle).unwrap();
        assert_eq!(middle, bytes[1_000_000..1_070_000].to_vec());
        assert!(hits.load(Ordering::SeqCst) >= 2, "parts, not one whole download");

        // A server that ignores ranges: the whole (small) file.
        let (port, _) = test_server::serve(bytes.clone(), false);
        let whole = open_source(&format!("http://127.0.0.1:{port}/clip.mp4"), &OptionHttpClient::None)
            .expect("a source");
        let mut r = SourceReader::new(whole, Wait::Yes);
        r.seek(SeekFrom::Start(2_000_000)).unwrap();
        let mut some = vec![0u8; 100];
        r.read_exact(&mut some).unwrap();
        assert_eq!(some, bytes[2_000_000..2_000_100].to_vec());
    }
}
