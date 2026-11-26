//! A resumable download of one file, in windows of 1 MiB, with a size limit, progress and a way to stop.
//!
//! It is what the updater downloads a release artifact with and what any other big file a program
//! fetches can use (the speech model of the built-in recognizer). Behind the feature `updater`, whose
//! dependencies it shares (the HTTP client, SHA-256).
//!
//! * the file lands on disk window by window (the HTTP layer buffers a whole answer in memory: a single
//!   request for the whole file would leave nothing on disk when it is interrupted), so a download that
//!   was stopped, or that died with the process, is resumed from the length of the partial file with a
//!   `Range` request;
//! * a server that ignores `Range` answers `200` with the whole file: the partial file is superseded and
//!   the download is one buffered write (correct, only not incremental): [`Outcome::server_supports_resume`]
//!   says which happened;
//! * a `206` is believed only as far as it can be checked: its `Content-Range` has to start where the
//!   request did and describe the bytes that came, the size it names may not change from one window to
//!   the next, and nothing larger than `max_bytes` is taken;
//! * a download the server or the size limit refused leaves NO partial file ([`Error::Refused`]), one that
//!   was stopped or broke on the wire keeps it ([`Error::Cancelled`], [`Error::Failed`]) for the resume.
//!
//! Checking what came (a digest) is the caller's: the file is only complete, not trusted.

use std::{io::Write as _, path::Path};

/// The bytes of one window: what a `Range` header asks for at a time.
const WINDOW: u64 = 1024 * 1024;

/// What to download and how.
#[derive(Clone, Copy)]
pub struct Request<'a> {
    /// Where the file is.
    pub url: &'a str,
    /// Headers besides `Range` (a registry blob needs the bearer token of its manifest request).
    pub headers: &'a [(&'a str, &'a str)],
    /// The file the bytes are appended to. One that is there already is resumed from.
    pub partial_path: &'a Path,
    /// The most the file may be, bytes: a server that answers `206` for ever, or lies about the size,
    /// would fill the disk one window after another.
    pub max_bytes: u64,
    /// What the file is, for the sentence that says it is too large: "an update", "a speech model".
    pub what: &'a str,
    /// Called with the bytes that are on disk and the size of the file when the server has said it,
    /// at the start and after each window.
    pub progress: &'a dyn Fn(u64, Option<u64>),
    /// Asked before each window: true stops the download there, the partial file stays.
    pub cancel: &'a dyn Fn() -> bool,
}

impl core::fmt::Debug for Request<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Request")
            .field("url", &self.url)
            .field("partial_path", &self.partial_path)
            .field("max_bytes", &self.max_bytes)
            .field("what", &self.what)
            .finish_non_exhaustive()
    }
}

/// What a download did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Outcome {
    /// Bytes that were already on disk from an earlier attempt and are kept (0: a fresh download, or
    /// a server that restarted it).
    pub resumed_from_bytes: u64,
    /// Bytes transferred this call.
    pub bytes_written: u64,
    /// The server honoured the `Range` request with `206 Partial Content`; false on a resume means it
    /// ignored ranges and the file restarted.
    pub server_supports_resume: bool,
    /// The size of the file, when the server said it.
    pub total_bytes: Option<u64>,
}

/// Why a download did not end with the whole file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// [`Request::cancel`] said stop. The partial file stays.
    Cancelled,
    /// The server or the size limit refused it, and the partial file is deleted so the next attempt
    /// starts clean. The sentence says why.
    Refused(String),
    /// The transport or the file system failed. The partial file stays.
    Failed(String),
}

impl Error {
    /// A sentence for a status line or an error string.
    #[must_use]
    pub fn message(&self) -> String {
        match self {
            Self::Cancelled => "download: stopped".to_owned(),
            Self::Refused(why) | Self::Failed(why) => why.clone(),
        }
    }
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.message())
    }
}

impl std::error::Error for Error {}

/// `Content-Range: bytes <start>-<end>/<total>` (`total` may be `*`): the range and the total
/// size when the server knows it.
pub(crate) fn parse_content_range(value: &str) -> Option<(u64, u64, Option<u64>)> {
    let rest = value.trim().strip_prefix("bytes")?.trim_start();
    let (range, total) = rest.split_once('/')?;
    let (start, end) = range.split_once('-')?;
    let start = start.trim().parse::<u64>().ok()?;
    let end = end.trim().parse::<u64>().ok()?;
    if end < start {
        return None;
    }
    let total = match total.trim() {
        "*" => None,
        digits => Some(digits.parse::<u64>().ok()?),
    };
    Some((start, end, total))
}

/// Downloads the file at `request.url` into `request.partial_path`: resumed from what is there, window
/// by window, until the size the server names is reached or a window comes back short.
///
/// # Errors
///
/// [`Error::Cancelled`] when the request's `cancel` said stop, [`Error::Refused`] when the server broke
/// the rules of a ranged answer or the file would be larger than `max_bytes`, [`Error::Failed`] when the
/// transport or the file system did.
pub fn download(request: &Request<'_>) -> Result<Outcome, Error> {
    let partial_path = request.partial_path;
    let max_bytes = request.max_bytes;
    let resumed_from = std::fs::metadata(partial_path).map_or(0, |m| m.len());
    // A refused download leaves no partial file behind.
    let refuse = |why: String| -> Error {
        drop(std::fs::remove_file(partial_path));
        Error::Refused(why)
    };
    let too_large = || {
        format!(
            "download: the file is larger than the {} MiB {} may be",
            max_bytes / (1024 * 1024),
            request.what
        )
    };
    if resumed_from > max_bytes {
        return Err(refuse(too_large()));
    }

    // CHUNKED-RANGE loop: each window lands on disk as it completes, so killing the process mid-download
    // keeps every finished chunk and the next call resumes from the partial file's length. A server that
    // ignores `Range` answers `200` with the whole file on the first request and the loop degrades to one
    // buffered write.
    let mut offset = resumed_from;
    let mut written_this_call = 0u64;
    let mut honored = false;
    let mut known_total: Option<u64> = None;
    (request.progress)(offset, known_total);
    loop {
        if (request.cancel)() {
            return Err(Error::Cancelled);
        }
        let mut config = crate::http::HttpRequestConfig::new()
            .with_header("Range", format!("bytes={offset}-{}", offset + WINDOW - 1));
        for (name, value) in request.headers {
            config = config.with_header(*name, *value);
        }
        let response = crate::http::http_get_with_config(request.url, &config)
            .map_err(|e| Error::Failed(format!("download: {e:?}")))?;
        match response.status_code {
            206 => {
                let bytes = response.body.as_ref();
                let length = bytes.len() as u64;
                let range = response
                    .headers
                    .as_ref()
                    .iter()
                    .find(|h| h.name.as_str().eq_ignore_ascii_case("content-range"))
                    .and_then(|h| parse_content_range(h.value.as_str()));
                let Some((start, end, total)) = range else {
                    return Err(refuse(
                        "download: the server answered 206 without a usable Content-Range".to_owned(),
                    ));
                };
                // The bytes have to be the window that was asked for, where it was asked for.
                if start != offset || end - start + 1 != length || length > WINDOW {
                    return Err(refuse(format!(
                        "download: the server sent bytes {start}-{end} ({length} bytes) for a request at {offset}"
                    )));
                }
                if let Some(total) = total {
                    if known_total.is_some_and(|known| known != total) {
                        return Err(refuse("download: the size of the file changed during the download".to_owned()));
                    }
                    if total > max_bytes {
                        return Err(refuse(too_large()));
                    }
                    known_total = Some(total);
                }
                honored = true;
                let mut file = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(partial_path)
                    .map_err(|e| Error::Failed(e.to_string()))?;
                file.write_all(bytes).map_err(|e| Error::Failed(e.to_string()))?;
                offset += length;
                written_this_call += length;
                (request.progress)(offset, known_total);
                if offset > max_bytes {
                    return Err(refuse(too_large()));
                }
                // The end of the file: the size the server named is reached, or the window came back
                // short.
                if known_total.is_some_and(|total| offset >= total) || length < WINDOW {
                    break;
                }
            }
            // 200 = the server ignored the Range header: it sent the WHOLE file; a previous partial is
            // superseded.
            200 => {
                let bytes = response.body.as_ref();
                if bytes.len() as u64 > max_bytes {
                    return Err(refuse(too_large()));
                }
                std::fs::write(partial_path, bytes).map_err(|e| Error::Failed(e.to_string()))?;
                written_this_call = bytes.len() as u64;
                known_total = Some(written_this_call);
                honored = false;
                (request.progress)(written_this_call, known_total);
                break;
            }
            // 416 Range Not Satisfiable: offset is already at (or past) the end - the partial IS the
            // complete file.
            416 => break,
            code => return Err(Error::Failed(format!("download HTTP {code}"))),
        }
    }
    Ok(Outcome {
        resumed_from_bytes: if honored { resumed_from } else { 0 },
        bytes_written: written_this_call,
        server_supports_resume: honored,
        total_bytes: known_total,
    })
}

/// The SHA-256 of the file at `path` as 64 lowercase hex digits.
///
/// # Errors
///
/// The sentence of the I/O error when the file cannot be read.
pub fn sha256_hex_of_file(path: &Path) -> Result<String, String> {
    crate::updater::sha256_of_file(path).map(|hash| crate::updater::sha256_hex(&hash))
}

#[cfg(test)]
#[path = "ranged_download_test.rs"]
mod ranged_download_test;
