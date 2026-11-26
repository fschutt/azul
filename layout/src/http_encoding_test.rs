//! HTTP bodies that arrive compressed: the request says it takes gzip and brotli, the body is decoded
//! before the caller sees it, and a body that expands past the size cap is refused with a message
//! (a decompression bomb costs the server a few kilobytes, the client what it expands to).

use std::{
    io::{BufRead, BufReader, Write},
    net::TcpListener,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use super::*;

/// What the server was asked: the request head's header lines, lower-cased.
type Heads = Arc<Mutex<Vec<String>>>;

/// A server on localhost that answers every request with `status line + headers + body` as given
/// (one connection per request, `Connection: close`), and keeps the heads it was sent.
fn serve(headers: &'static str, body: Vec<u8>) -> (String, Heads) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let url = format!("http://{}/", listener.local_addr().expect("addr"));
    let heads: Heads = Arc::default();
    let kept = Arc::clone(&heads);
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { return };
            let mut reader = BufReader::new(stream.try_clone().expect("clone"));
            let mut head = String::new();
            loop {
                let mut line = String::new();
                match reader.read_line(&mut line) {
                    Ok(0) | Err(_) => break,
                    Ok(_) if line == "\r\n" => break,
                    Ok(_) => head.push_str(&line.to_ascii_lowercase()),
                }
            }
            kept.lock().unwrap().push(head);
            let reply = format!(
                "HTTP/1.1 200 OK\r\n{headers}Content-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            // a client that gives up on a refused body closes the socket: not an error here
            let _ = stream.write_all(reply.as_bytes());
            let _ = stream.write_all(&body);
            let _ = stream.flush();
        }
    });
    (url, heads)
}

fn gzip_of(plain: &[u8]) -> Vec<u8> {
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
    encoder.write_all(plain).expect("gzip");
    encoder.finish().expect("gzip")
}

fn brotli_of(plain: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut encoder = brotli::CompressorWriter::new(&mut out, 4096, 5, 22);
        encoder.write_all(plain).expect("brotli");
    }
    out
}

/// `size` zero bytes, gzipped in chunks (a few KB of compressed data per hundred MB).
fn gzip_zeros(size: usize) -> Vec<u8> {
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
    let chunk = vec![0u8; 1 << 20];
    let mut left = size;
    while left > 0 {
        let n = left.min(chunk.len());
        encoder.write_all(&chunk[..n]).expect("gzip");
        left -= n;
    }
    encoder.finish().expect("gzip")
}

fn config() -> HttpRequestConfig {
    HttpRequestConfig::default().with_timeout(20)
}

#[test]
fn a_request_says_it_takes_gzip_and_brotli() {
    let (url, heads) = serve("Content-Type: text/plain\r\n", b"plain".to_vec());
    let response = http_get_with_config(&url, &config()).expect("GET");
    assert_eq!(response.body.as_ref(), b"plain");
    let head = heads.lock().unwrap()[0].clone();
    let accepted = head
        .lines()
        .find_map(|l| l.strip_prefix("accept-encoding:"))
        .unwrap_or_default()
        .to_string();
    assert!(accepted.contains("gzip"), "Accept-Encoding: {accepted:?}");
    assert!(accepted.contains("br"), "Accept-Encoding: {accepted:?}");
}

#[test]
fn a_header_of_the_caller_decides_what_is_accepted() {
    let (url, heads) = serve("Content-Type: text/plain\r\n", b"plain".to_vec());
    let response = http_get_with_config(&url, &config().with_header("Accept-Encoding", "identity"))
        .expect("GET");
    assert_eq!(response.body.as_ref(), b"plain");
    let head = heads.lock().unwrap()[0].clone();
    assert!(head.contains("accept-encoding: identity"), "{head}");
    assert!(!head.contains("gzip"), "{head}");
}

#[test]
fn a_gzip_body_is_decoded() {
    let text = "a body that is long enough to be worth compressing ".repeat(200);
    let (url, _) = serve(
        "Content-Type: text/plain\r\nContent-Encoding: gzip\r\n",
        gzip_of(text.as_bytes()),
    );
    let response = http_get_with_config(&url, &config()).expect("GET");
    assert_eq!(response.status_code, 200);
    assert_eq!(response.body.as_ref(), text.as_bytes());
    // ureq removes Content-Encoding and Content-Length of a body it decoded: 0 = unknown
    assert_eq!(response.content_length, 0);
}

#[test]
fn a_brotli_body_is_decoded() {
    let text = "brotli packs repeated text very well, repeated text, repeated text ".repeat(200);
    let (url, _) = serve(
        "Content-Type: text/plain\r\nContent-Encoding: br\r\n",
        brotli_of(text.as_bytes()),
    );
    let response = http_get_with_config(&url, &config()).expect("GET");
    assert_eq!(response.body.as_ref(), text.as_bytes());
}

#[test]
fn a_body_that_says_gzip_and_is_not_is_an_error_with_a_message() {
    let (url, _) = serve(
        "Content-Type: text/plain\r\nContent-Encoding: gzip\r\n",
        b"this is not a gzip stream at all".to_vec(),
    );
    let error = http_get_with_config(&url, &config()).expect_err("an invalid gzip stream");
    let text = format!("{error:?}");
    assert!(
        text.to_ascii_lowercase().contains("gzip") || text.to_ascii_lowercase().contains("io"),
        "{text}"
    );
}

#[test]
fn a_gzip_bomb_is_refused_at_the_size_cap_in_bounded_time() {
    // 200 MB of zeros in a few hundred KB; the cap is 1 MiB
    let bomb = gzip_zeros(200 << 20);
    assert!(bomb.len() < 2 << 20, "the bomb is small on the wire: {}", bomb.len());
    let (url, _) = serve("Content-Type: text/plain\r\nContent-Encoding: gzip\r\n", bomb);
    let started = Instant::now();
    let error = http_get_with_config(&url, &config().with_max_size(1 << 20))
        .expect_err("the bomb expands past the cap");
    assert!(
        matches!(error, HttpError::ResponseTooLarge(_)),
        "{error:?}"
    );
    assert!(started.elapsed() < Duration::from_secs(20), "{:?}", started.elapsed());
}

#[test]
fn a_gzip_bomb_is_refused_at_the_default_cap_too() {
    // 150 MB of zeros against the default cap of 100 MB
    let bomb = gzip_zeros(150 << 20);
    let (url, _) = serve("Content-Type: text/plain\r\nContent-Encoding: gzip\r\n", bomb);
    let error = http_get_with_config(&url, &config()).expect_err("past the default cap");
    assert!(
        matches!(error, HttpError::ResponseTooLarge(_)),
        "{error:?}"
    );
}

#[test]
fn a_body_that_fits_the_cap_when_decoded_is_not_refused_for_its_encoded_size_or_its_decoded_one() {
    // 4 MiB of zeros: tiny on the wire, 4 MiB decoded, cap 8 MiB
    let (url, _) = serve(
        "Content-Type: text/plain\r\nContent-Encoding: gzip\r\n",
        gzip_zeros(4 << 20),
    );
    let response = http_get_with_config(&url, &config().with_max_size(8 << 20)).expect("GET");
    assert_eq!(response.body.as_ref().len(), 4 << 20);
}
