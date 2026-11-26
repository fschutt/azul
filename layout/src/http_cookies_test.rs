//! A cookie jar in the HTTP client is a feature that a build asks for (`http-cookies`, ureq's `cookies`):
//! with it, the cookie that a server sets comes back with the next request to that server; without it,
//! no request carries a cookie of the engine's own.

use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::mpsc::{channel, Receiver},
    thread,
    time::Duration,
};

use super::*;

/// A server on localhost for TWO connections: the answer to the first sets a cookie, and the request
/// of the second is kept (as text) for the test.
fn cookie_server() -> (u16, Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let (sender, receiver) = channel();
    thread::spawn(move || {
        for round in 0..2 {
            let Ok((mut stream, _)) = listener.accept() else {
                return;
            };
            let mut buffer = [0u8; 4096];
            let read = stream.read(&mut buffer).unwrap_or(0);
            let request = String::from_utf8_lossy(&buffer[..read]).into_owned();
            let reply = if round == 0 {
                "HTTP/1.1 200 OK\r\nSet-Cookie: sid=abc123; Path=/\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok"
            } else {
                "HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok"
            };
            let _ = stream.write_all(reply.as_bytes());
            if round == 1 {
                let _ = sender.send(request);
            }
        }
    });
    (port, receiver)
}

#[test]
fn a_cookie_that_a_server_sets_comes_back_with_the_next_request_in_a_build_that_asks_for_it() {
    let (port, second) = cookie_server();
    // the engine's own agent configuration, and one agent for both requests: the jar belongs to it
    let config = agent_config_trusting(false, no_root_certs())
        .timeout_global(Some(Duration::from_secs(20)))
        .build();
    let agent = ureq::Agent::with_parts(
        config,
        crate::http_retry::connector(),
        ureq::unversioned::resolver::DefaultResolver::default(),
    );
    let url = format!("http://127.0.0.1:{port}/session");
    for _ in 0..2 {
        let mut response = agent.get(&url).call().expect("the request is answered");
        assert_eq!(response.body_mut().read_to_string().expect("a body"), "ok");
    }
    let request = second
        .recv_timeout(Duration::from_secs(5))
        .expect("the server saw the second request")
        .to_ascii_lowercase();
    #[cfg(feature = "http-cookies")]
    assert!(request.contains("cookie: sid=abc123"), "{request}");
    #[cfg(not(feature = "http-cookies"))]
    assert!(
        !request.contains("cookie:"),
        "no http-cookies feature, no jar: {request}"
    );
}
