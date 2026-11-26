//! Proxies the client reaches: a SOCKS5 proxy carries a request (ureq's `socks-proxy`), and on Windows
//! the proxy that Internet Options names is looked up (`win-system-proxy`).

use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::mpsc::{channel, Receiver},
    thread,
    time::Duration,
};

use super::*;

/// A SOCKS5 proxy on localhost for ONE connection: no authentication; it keeps the CONNECT target
/// and then speaks as the server behind it, answering the client's request with `reply`.
fn socks5_proxy(reply: &'static [u8]) -> (u16, Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let (sender, receiver) = channel();
    thread::spawn(move || {
        let Ok((mut stream, _)) = listener.accept() else {
            return;
        };
        // greeting: version 5, one method (no authentication)
        let mut greeting = [0u8; 3];
        if stream.read_exact(&mut greeting).is_err() || greeting[0] != 5 {
            return;
        }
        let _ = stream.write_all(&[5, 0]);
        // request: version, command (connect), reserved, address type, address, port
        let mut head = [0u8; 4];
        if stream.read_exact(&mut head).is_err() || head[1] != 1 {
            return;
        }
        let host = match head[3] {
            1 => {
                let mut ip = [0u8; 4];
                if stream.read_exact(&mut ip).is_err() {
                    return;
                }
                format!("{}.{}.{}.{}", ip[0], ip[1], ip[2], ip[3])
            }
            3 => {
                let mut length = [0u8; 1];
                if stream.read_exact(&mut length).is_err() {
                    return;
                }
                let mut name = vec![0u8; usize::from(length[0])];
                if stream.read_exact(&mut name).is_err() {
                    return;
                }
                String::from_utf8_lossy(&name).into_owned()
            }
            _ => return,
        };
        let mut port = [0u8; 2];
        if stream.read_exact(&mut port).is_err() {
            return;
        }
        let _ = sender.send(format!("{host}:{}", u16::from_be_bytes(port)));
        // connected, bound to 0.0.0.0:0
        let _ = stream.write_all(&[5, 0, 0, 1, 0, 0, 0, 0, 0, 0]);
        // the client's request, then the answer of the server behind the proxy
        let mut request = [0u8; 4096];
        let _ = stream.read(&mut request);
        let _ = stream.write_all(reply);
    });
    (port, receiver)
}

#[test]
fn a_request_goes_through_a_socks5_proxy() {
    let (port, target) = socks5_proxy(
        b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok",
    );
    let proxy = ureq::Proxy::new(&format!("socks5://127.0.0.1:{port}")).expect("a proxy url");
    // the engine's own agent configuration, with the proxy a user's environment would name
    let config = agent_config_trusting(false, no_root_certs())
        .proxy(Some(proxy))
        .timeout_global(Some(Duration::from_secs(20)))
        .build();
    let agent = ureq::Agent::with_parts(
        config,
        crate::http_retry::connector(),
        ureq::unversioned::resolver::DefaultResolver::default(),
    );
    // an address of TEST-NET-3: nothing there, the proxy stands in for it
    let mut response = agent
        .get("http://203.0.113.7:8080/proxied")
        .call()
        .expect("the request goes through the SOCKS5 proxy");
    assert_eq!(response.status().as_u16(), 200);
    assert_eq!(response.body_mut().read_to_string().expect("a body"), "ok");
    assert_eq!(
        target.recv_timeout(Duration::from_secs(5)).expect("the proxy was asked"),
        "203.0.113.7:8080"
    );
}

#[cfg(windows)]
#[test]
fn the_windows_system_proxy_is_looked_up_without_failing() {
    // reads HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings (ProxyEnable,
    // ProxyServer) when ureq has `win-system-proxy`; whatever it finds, it does not panic
    let _ = ureq::Proxy::try_from_env();
}
