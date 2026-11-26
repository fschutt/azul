//! The ranged download: what it reports while it goes, how a stop leaves the file, what the next call
//! does with it, and what it says when a file is too large. The server is a real one on localhost; the
//! updater's tests (`updater.rs`) put the same loop against the servers that lie.

use std::{
    io::{BufRead, BufReader, Write},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
};

use super::{download, parse_content_range, Error, Request, WINDOW};

/// The body of a test file: bytes that tell where they are.
fn body_of(len: usize) -> Vec<u8> {
    (0..len).map(|i| (i.wrapping_mul(2_654_435_761) >> 7) as u8).collect()
}

/// What the test server saw.
#[derive(Default)]
struct Seen {
    /// The first byte of each `Range` it was asked for.
    range_starts: Vec<usize>,
    /// The value of the header `x-token` of the last request, if any.
    token: Option<String>,
}

/// A localhost server that answers a `Range` request honestly (`206` with its `Content-Range`, `416`
/// past the end); the URL of the file and what the server saw.
fn serve(body: Vec<u8>) -> (String, Arc<Mutex<Seen>>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    let url = format!("http://{}/file.bin", listener.local_addr().expect("addr"));
    let seen = Arc::new(Mutex::new(Seen::default()));
    let record = Arc::clone(&seen);
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { return };
            let mut reader = BufReader::new(stream.try_clone().expect("clone"));
            'requests: loop {
                let mut range = None;
                let mut line = String::new();
                loop {
                    line.clear();
                    match reader.read_line(&mut line) {
                        Ok(0) | Err(_) => break 'requests,
                        Ok(_) if line == "\r\n" => break,
                        Ok(_) => {
                            let lower = line.trim().to_ascii_lowercase();
                            if let Some(value) = lower.strip_prefix("range: bytes=") {
                                let (from, to) = value.split_once('-').unwrap_or(("0", "0"));
                                range = Some((from.parse::<usize>().unwrap_or(0), to.parse::<usize>().unwrap_or(0)));
                            } else if let Some(value) = lower.strip_prefix("x-token:") {
                                record.lock().unwrap().token = Some(value.trim().to_string());
                            }
                        }
                    }
                }
                let total = body.len();
                let answer = match range {
                    Some((from, _)) if from >= total => {
                        format!("HTTP/1.1 416 Range Not Satisfiable\r\nContent-Length: 0\r\nContent-Range: bytes */{total}\r\n\r\n")
                            .into_bytes()
                    }
                    Some((from, to)) => {
                        record.lock().unwrap().range_starts.push(from);
                        let part = &body[from..=to.min(total - 1)];
                        let mut out = format!(
                            "HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes {from}-{}/{total}\r\n\r\n",
                            part.len(),
                            from + part.len() - 1
                        )
                        .into_bytes();
                        out.extend_from_slice(part);
                        out
                    }
                    None => {
                        let mut out = format!("HTTP/1.1 200 OK\r\nContent-Length: {total}\r\n\r\n").into_bytes();
                        out.extend_from_slice(&body);
                        out
                    }
                };
                if stream.write_all(&answer).is_err() {
                    break;
                }
            }
        }
    });
    (url, seen)
}

fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("azul-ranged-download-{}-{name}", std::process::id()));
    drop(std::fs::remove_dir_all(&dir));
    std::fs::create_dir_all(&dir).expect("a scratch folder");
    dir
}

fn quiet() -> (&'static dyn Fn(u64, Option<u64>), &'static dyn Fn() -> bool) {
    (&|_, _| {}, &|| false)
}

#[test]
fn a_content_range_is_read_strictly() {
    assert_eq!(parse_content_range("bytes 0-1048575/3145728"), Some((0, 1_048_575, Some(3_145_728))));
    assert_eq!(parse_content_range("bytes 10-19/*"), Some((10, 19, None)));
    assert_eq!(parse_content_range(" bytes  5-5/9 "), Some((5, 5, Some(9))));
    for bad in ["", "bytes */100", "bytes 5-4/100", "bytes a-b/100", "items 0-9/10", "bytes 0-9", "bytes 0-9/x"] {
        assert_eq!(parse_content_range(bad), None, "{bad:?}");
    }
}

#[test]
fn a_download_reports_each_window_and_ends_with_the_whole_file() {
    let body = body_of(WINDOW as usize * 5 / 2);
    let (url, seen) = serve(body.clone());
    let dir = scratch("windows");
    let partial = dir.join("file.partial");
    let reports = Mutex::new(Vec::new());
    let (_, never) = quiet();
    let outcome = download(&Request {
        url: &url,
        headers: &[("x-token", "secret")],
        partial_path: &partial,
        max_bytes: 10 * WINDOW,
        what: "a test file",
        progress: &|done, total| reports.lock().unwrap().push((done, total)),
        cancel: never,
    })
    .expect("downloads");
    let total = body.len() as u64;
    assert_eq!(std::fs::read(&partial).expect("the file"), body);
    assert_eq!(
        *reports.lock().unwrap(),
        [(0, None), (WINDOW, Some(total)), (2 * WINDOW, Some(total)), (total, Some(total))],
        "at the start and after each window"
    );
    assert_eq!((outcome.resumed_from_bytes, outcome.bytes_written), (0, total));
    assert!(outcome.server_supports_resume);
    assert_eq!(outcome.total_bytes, Some(total));
    let seen = seen.lock().unwrap();
    assert_eq!(seen.range_starts, [0, WINDOW as usize, 2 * WINDOW as usize]);
    assert_eq!(seen.token.as_deref(), Some("secret"), "the headers of the request are sent");
    drop(std::fs::remove_dir_all(&dir));
}

#[test]
fn a_stopped_download_keeps_what_came_and_the_next_call_resumes_from_it() {
    let body = body_of(WINDOW as usize * 5 / 2);
    let (url, seen) = serve(body.clone());
    let dir = scratch("resume");
    let partial = dir.join("file.partial");
    let done = AtomicU64::new(0);
    let stopped = download(&Request {
        url: &url,
        headers: &[],
        partial_path: &partial,
        max_bytes: 10 * WINDOW,
        what: "a test file",
        progress: &|bytes, _| done.store(bytes, Ordering::SeqCst),
        // Stop once the first window is on disk.
        cancel: &|| done.load(Ordering::SeqCst) >= WINDOW,
    });
    assert_eq!(stopped, Err(Error::Cancelled));
    assert_eq!(std::fs::metadata(&partial).expect("the partial file stays").len(), WINDOW);

    let (progress, never) = quiet();
    let outcome = download(&Request {
        url: &url,
        headers: &[],
        partial_path: &partial,
        max_bytes: 10 * WINDOW,
        what: "a test file",
        progress,
        cancel: never,
    })
    .expect("resumes");
    assert_eq!(std::fs::read(&partial).expect("the file"), body);
    assert_eq!(outcome.resumed_from_bytes, WINDOW);
    assert_eq!(outcome.bytes_written, body.len() as u64 - WINDOW);
    assert!(outcome.server_supports_resume);
    assert_eq!(seen.lock().unwrap().range_starts, [0, WINDOW as usize, 2 * WINDOW as usize], "the second call began where the first stopped");
    drop(std::fs::remove_dir_all(&dir));
}

#[test]
fn a_file_larger_than_the_limit_is_refused_by_name_and_leaves_no_partial_file() {
    let (url, _) = serve(body_of(WINDOW as usize * 3));
    let dir = scratch("limit");
    let partial = dir.join("file.partial");
    let (progress, never) = quiet();
    let refused = download(&Request {
        url: &url,
        headers: &[],
        partial_path: &partial,
        max_bytes: WINDOW,
        what: "a speech model",
        progress,
        cancel: never,
    })
    .expect_err("three windows against a limit of one");
    let Error::Refused(why) = refused else { panic!("expected a refusal, got {refused:?}") };
    assert!(why.contains("larger than the 1 MiB a speech model may be"), "{why}");
    assert!(!partial.exists(), "a refused download leaves nothing behind");
    drop(std::fs::remove_dir_all(&dir));
}

#[test]
fn a_partial_file_longer_than_the_limit_is_refused_at_once() {
    let (url, seen) = serve(body_of(16));
    let dir = scratch("overlong");
    let partial = dir.join("file.partial");
    std::fs::write(&partial, vec![0u8; 64]).expect("a partial file");
    let (progress, never) = quiet();
    let refused = download(&Request {
        url: &url,
        headers: &[],
        partial_path: &partial,
        max_bytes: 32,
        what: "a test file",
        progress,
        cancel: never,
    });
    assert!(matches!(refused, Err(Error::Refused(_))), "{refused:?}");
    assert!(!partial.exists());
    assert!(seen.lock().unwrap().range_starts.is_empty(), "nothing was asked of the server");
    drop(std::fs::remove_dir_all(&dir));
}
