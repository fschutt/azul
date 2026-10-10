//! One S3 request and its answer as HTTP/1.1 bytes: what S3 over iroh sends on a bidirectional
//! QUIC stream to a node (ALPN `azlin/s3/1`) - the same signed request that goes over TLS to the
//! block endpoint, its `Host` the URL's authority (SigV4 signed it), `Connection: close` so the
//! node finishes its side of the stream after the answer.

use crate::{
    http1::{decode_reply, encode_request, S3_OVER_IROH_ALPN},
    HttpCall, Method,
};

fn call(method: Method, url: &str, body: &[u8]) -> HttpCall {
    HttpCall {
        method,
        url: url.to_string(),
        headers: vec![
            (String::from("x-amz-date"), String::from("20261010T120000Z")),
            (
                String::from("authorization"),
                String::from("AWS4-HMAC-SHA256 sig"),
            ),
        ],
        body: body.to_vec(),
        content_type: if body.is_empty() {
            String::new()
        } else {
            String::from("application/octet-stream")
        },
    }
}

#[test]
fn a_request_is_its_method_path_host_signed_headers_and_body() {
    let bytes = encode_request(&call(
        Method::Put,
        "https://blk.azlin.test:8443/d-1/a%20b.txt?partNumber=1&uploadId=u",
        b"hello",
    ))
    .unwrap();
    let text = String::from_utf8(bytes).unwrap();
    let (head, body) = text.split_once("\r\n\r\n").unwrap();
    let mut lines = head.split("\r\n");
    assert_eq!(
        lines.next(),
        Some("PUT /d-1/a%20b.txt?partNumber=1&uploadId=u HTTP/1.1")
    );
    let headers: Vec<&str> = lines.collect();
    assert!(
        headers.contains(&"host: blk.azlin.test:8443"),
        "the authority SigV4 signed: {headers:?}"
    );
    assert!(headers.contains(&"x-amz-date: 20261010T120000Z"));
    assert!(headers.contains(&"authorization: AWS4-HMAC-SHA256 sig"));
    assert!(headers.contains(&"content-type: application/octet-stream"));
    assert!(headers.contains(&"content-length: 5"));
    assert!(
        headers.contains(&"connection: close"),
        "the node finishes its side after the answer"
    );
    assert_eq!(body, "hello");
    assert_eq!(S3_OVER_IROH_ALPN, "azlin/s3/1");
}

#[test]
fn a_bucket_request_without_a_path_goes_to_the_root_and_a_header_with_a_line_break_is_refused() {
    let text = String::from_utf8(
        encode_request(&call(Method::Get, "http://127.0.0.1:9000", b"")).unwrap(),
    )
    .unwrap();
    assert!(text.starts_with("GET / HTTP/1.1\r\n"), "{text}");
    assert!(
        !text.contains("content-length"),
        "no body, no length: {text}"
    );
    let mut bad = call(Method::Get, "http://127.0.0.1:9000/d-1", b"");
    bad.headers
        .push((String::from("x-evil"), String::from("a\r\nhost: elsewhere")));
    assert!(encode_request(&bad).is_err());
    assert!(encode_request(&call(Method::Get, "no url", b"")).is_err());
}

#[test]
fn an_answer_with_a_length_is_read_to_its_length() {
    let reply = decode_reply(
        b"HTTP/1.1 200 OK\r\nETag: \"abc\"\r\nContent-Length: 5\r\nConnection: close\r\n\r\nhello",
        false,
    )
    .unwrap();
    assert_eq!(reply.status, 200);
    assert_eq!(reply.header("etag"), Some("\"abc\""));
    assert_eq!(reply.body, b"hello");
    assert!(
        decode_reply(b"HTTP/1.1 200 OK\r\nContent-Length: 9\r\n\r\nhello", false).is_err(),
        "an answer that broke off"
    );
}

#[test]
fn a_chunked_answer_is_put_together_and_an_unframed_one_runs_to_the_end() {
    let reply = decode_reply(
        b"HTTP/1.1 404 Not Found\r\nTransfer-Encoding: chunked\r\n\r\n5\r\n<Erro\r\n3;x=y\r\nr/>\r\n0\r\n\r\n",
        false,
    )
    .unwrap();
    assert_eq!(reply.status, 404);
    assert_eq!(reply.body, b"<Error/>");
    let reply = decode_reply(b"HTTP/1.1 200 OK\r\n\r\nall of it", false).unwrap();
    assert_eq!(reply.body, b"all of it");
}

#[test]
fn the_answer_to_a_head_has_no_body_whatever_its_length_says() {
    let reply = decode_reply(b"HTTP/1.1 200 OK\r\nContent-Length: 1048576\r\n\r\n", true).unwrap();
    assert_eq!(reply.status, 200);
    assert_eq!(reply.header("content-length"), Some("1048576"));
    assert!(reply.body.is_empty());
    let interim = decode_reply(
        b"HTTP/1.1 100 Continue\r\n\r\nHTTP/1.1 204 No Content\r\n\r\n",
        false,
    )
    .unwrap();
    assert_eq!(interim.status, 204, "an interim answer is skipped");
    assert!(decode_reply(b"not http", false).is_err());
}
