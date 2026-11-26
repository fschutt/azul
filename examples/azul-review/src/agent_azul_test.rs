//! The engine's side of the agents, against a server of this machine: azul's HTTP client sends a
//! POST with headers and a body, an error status comes back as an answer, a server that never
//! answers is a timeout, a closed port is a failure in words, a too big answer is refused; and the
//! two API backends run through it end to end. The keychain read cannot fail the process.

use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{mpsc, Arc},
    thread,
    time::{Duration, Instant},
};

use super::*;
use crate::{
    agent_api::{AnthropicApi, AnthropicConfig, GeminiApi, GeminiApiConfig},
    agent_backend::{
        never_cancelled, run_task, AgentError, BackendKind, Body, Control, ManualClock, Request,
        Task,
    },
    agent_limits::Limits,
    agent_review::{Context, ReviewRequest, Utterance},
    agent_settings::Secret,
};

const ANTHROPIC_OK: &str = include_str!("agent_fixtures/anthropic_ok_shaped.json");
const ANTHROPIC_401: &str = include_str!("agent_fixtures/anthropic_401.json");
const GEMINI_OK: &str = include_str!("agent_fixtures/gemini_ok_shaped.json");

/// What the server was sent.
struct Received {
    request_line: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl Received {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

/// Reads one request: the head, then as many bytes of body as its length says.
fn read_request(stream: &mut TcpStream) -> Received {
    let mut data = Vec::new();
    let mut buffer = [0_u8; 4096];
    let head_end = loop {
        if let Some(at) = find(&data, b"\r\n\r\n") {
            break at;
        }
        let n = stream.read(&mut buffer).unwrap_or(0);
        if n == 0 {
            break data.len();
        }
        data.extend_from_slice(&buffer[..n]);
    };
    let head = String::from_utf8_lossy(&data[..head_end.min(data.len())]).to_string();
    let mut lines = head.lines();
    let request_line = lines.next().unwrap_or("").to_string();
    let headers: Vec<(String, String)> = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(n, v)| (n.trim().to_string(), v.trim().to_string()))
        .collect();
    let length: usize = headers
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case("content-length"))
        .and_then(|(_, v)| v.parse().ok())
        .unwrap_or(0);
    let body_start = (head_end + 4).min(data.len());
    while data.len() < body_start + length {
        let n = stream.read(&mut buffer).unwrap_or(0);
        if n == 0 {
            break;
        }
        data.extend_from_slice(&buffer[..n]);
    }
    let body = data[body_start..(body_start + length).min(data.len())].to_vec();
    Received {
        request_line,
        headers,
        body,
    }
}

/// A server of this machine that answers one request.
struct Server {
    port: u16,
    got: mpsc::Receiver<Received>,
}

fn serve(status: u16, body: &str, delay: Option<Duration>) -> Server {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a port");
    let port = listener.local_addr().expect("an address").port();
    let (send, got) = mpsc::channel();
    let reply = format!(
        "HTTP/1.1 {status} Status\r\ncontent-type: application/json\r\ncontent-length: {}\r\nretry-after: 7\r\nconnection: close\r\n\r\n{body}",
        body.len()
    );
    thread::spawn(move || {
        let Ok((mut stream, _)) = listener.accept() else {
            return;
        };
        let received = read_request(&mut stream);
        let _ = send.send(received);
        if let Some(delay) = delay {
            thread::sleep(delay);
        }
        let _ = stream.write_all(reply.as_bytes());
    });
    Server { port, got }
}

fn post(port: u16, path: &str, timeout_secs: u32, max_response: usize) -> HttpRequest {
    HttpRequest {
        url: format!("http://127.0.0.1:{port}{path}"),
        headers: vec![
            ("x-api-key".to_string(), "k-123456".to_string()),
            ("anthropic-version".to_string(), "2026-10-21".to_string()),
        ],
        body: br#"{"hello": "world"}"#.to_vec(),
        content_type: "application/json".to_string(),
        timeout_secs,
        max_response,
    }
}

// ==== the transport ====

#[test]
fn a_post_reaches_the_server_with_its_headers_and_body_and_the_answer_comes_back() {
    let server = serve(200, r#"{"ok": true}"#, None);
    let reply = AzulTransport::new("AzReview-test")
        .send(&post(server.port, "/v1/messages", 20, 1 << 20))
        .expect("answered");
    assert_eq!(reply.status, 200);
    assert_eq!(reply.body, br#"{"ok": true}"#);
    let got = server
        .got
        .recv_timeout(Duration::from_secs(5))
        .expect("the server saw it");
    assert_eq!(got.request_line, "POST /v1/messages HTTP/1.1");
    assert_eq!(got.header("x-api-key"), Some("k-123456"));
    assert_eq!(got.header("anthropic-version"), Some("2026-10-21"));
    assert_eq!(got.header("content-type"), Some("application/json"));
    assert_eq!(got.header("user-agent"), Some("AzReview-test"));
    assert_eq!(got.body, br#"{"hello": "world"}"#);
}

#[test]
fn an_error_status_is_an_answer_with_its_body_and_its_headers() {
    let server = serve(401, ANTHROPIC_401.trim(), None);
    let reply = AzulTransport::new("AzReview-test")
        .send(&post(server.port, "/v1/messages", 20, 1 << 20))
        .expect("an answer, not a failure");
    assert_eq!(reply.status, 401);
    assert!(String::from_utf8_lossy(&reply.body).contains("authentication_error"));
    assert!(
        reply
            .headers
            .iter()
            .any(|(n, v)| n.eq_ignore_ascii_case("retry-after") && v == "7"),
        "{:?}",
        reply.headers
    );
}

#[test]
fn a_server_that_does_not_answer_in_time_is_a_timeout() {
    let server = serve(200, "{}", Some(Duration::from_secs(8)));
    let began = Instant::now();
    let error = AzulTransport::new("AzReview-test")
        .send(&post(server.port, "/x", 1, 1 << 20))
        .expect_err("too slow");
    assert_eq!(error, TransportError::Timeout);
    assert!(
        began.elapsed() < Duration::from_secs(6),
        "{:?}",
        began.elapsed()
    );
}

#[test]
fn a_closed_port_is_a_failure_in_words() {
    let port = {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a port");
        listener.local_addr().expect("an address").port()
    };
    match AzulTransport::new("AzReview-test").send(&post(port, "/x", 10, 1 << 20)) {
        Err(TransportError::Failed(why)) => assert!(!why.trim().is_empty()),
        other => panic!("{other:?}"),
    }
}

#[test]
fn an_answer_bigger_than_the_limit_is_refused_and_not_cut_short() {
    let big = "x".repeat(10_000);
    let server = serve(200, &big, None);
    let result = AzulTransport::new("AzReview-test").send(&post(server.port, "/x", 10, 1_000));
    assert!(
        matches!(result, Err(TransportError::Failed(_))),
        "{result:?}"
    );
}

// ==== the API backends through it ====

fn request() -> Request {
    Request::Review(ReviewRequest {
        task: Task::Editing,
        conversation: 1,
        language: "en".to_string(),
        utterances: vec![Utterance {
            text: "say ok".to_string(),
            start_ms: 0,
            end_ms: 500,
            words: Vec::new(),
        }],
        strokes: Vec::new(),
        context: Context {
            file: "a.rs".to_string(),
            page: 0,
            first_line: 1,
            lines: vec!["fn a() {}".to_string()],
            total_lines: 1,
            focus: None,
            words_under: Vec::new(),
            grid: None,
        },
        turns: Vec::new(),
        workspace: None,
    })
}

#[test]
fn the_anthropic_backend_runs_through_the_engines_client_against_a_local_server() {
    let server = serve(200, ANTHROPIC_OK.trim(), None);
    let mut backend = AnthropicApi {
        config: AnthropicConfig {
            model: "claude-haiku-5-5".to_string(),
            base_url: format!("http://127.0.0.1:{}", server.port),
            timeout_secs: 20,
            max_tokens: 1024,
            effort: "low".to_string(),
            key: Secret::new("k-123456"),
            limits: Limits::default(),
        },
        transport: Arc::new(AzulTransport::new("AzReview-test")),
    };
    let mut clock = ManualClock::new();
    let mut ctl = Control::new(&mut clock, &never_cancelled);
    let response =
        run_task(&mut backend, &request(), &Limits::default(), &mut ctl).expect("answers");
    let Body::Review(reply) = &response.body else {
        panic!("a review answer")
    };
    assert_eq!(reply.answer, "ok");
    assert_eq!(response.identity.kind, BackendKind::AnthropicApi);
    let got = server
        .got
        .recv_timeout(Duration::from_secs(5))
        .expect("seen");
    assert_eq!(got.request_line, "POST /v1/messages HTTP/1.1");
    assert_eq!(got.header("x-api-key"), Some("k-123456"));
    let sent = String::from_utf8_lossy(&got.body).to_string();
    assert!(
        sent.contains("\"claude-haiku-5-5\"") && sent.contains("\"effort\":\"low\""),
        "{sent}"
    );
}

#[test]
fn the_gemini_backend_runs_through_the_engines_client_against_a_local_server() {
    let server = serve(200, GEMINI_OK.trim(), None);
    let mut backend = GeminiApi {
        config: GeminiApiConfig {
            model: "gemini-2.5-flash".to_string(),
            base_url: format!("http://127.0.0.1:{}", server.port),
            timeout_secs: 20,
            max_tokens: 1024,
            key: Secret::new("g-123456"),
            limits: Limits::default(),
        },
        transport: Arc::new(AzulTransport::new("AzReview-test")),
    };
    let mut clock = ManualClock::new();
    let mut ctl = Control::new(&mut clock, &never_cancelled);
    let response =
        run_task(&mut backend, &request(), &Limits::default(), &mut ctl).expect("answers");
    let Body::Review(reply) = &response.body else {
        panic!("a review answer")
    };
    assert_eq!(reply.answer, "ok");
    let got = server
        .got
        .recv_timeout(Duration::from_secs(5))
        .expect("seen");
    assert_eq!(
        got.request_line,
        "POST /v1beta/models/gemini-2.5-flash:generateContent HTTP/1.1"
    );
    assert_eq!(got.header("x-goog-api-key"), Some("g-123456"));
}

#[test]
fn a_wrong_key_comes_back_as_the_services_own_words_not_as_a_crash() {
    let server = serve(401, ANTHROPIC_401.trim(), None);
    let mut backend = AnthropicApi {
        config: AnthropicConfig {
            model: "claude-haiku-5-5".to_string(),
            base_url: format!("http://127.0.0.1:{}", server.port),
            timeout_secs: 20,
            max_tokens: 1024,
            effort: String::new(),
            key: Secret::new("wrong"),
            limits: Limits::default(),
        },
        transport: Arc::new(AzulTransport::new("AzReview-test")),
    };
    let mut clock = ManualClock::new();
    let mut ctl = Control::new(&mut clock, &never_cancelled);
    match run_task(&mut backend, &request(), &Limits::default(), &mut ctl).expect_err("401") {
        AgentError::Http {
            status,
            kind,
            detail,
        } => assert_eq!(
            (status, kind.as_str(), detail.as_str()),
            (401, "authentication_error", "invalid x-api-key")
        ),
        other => panic!("{other:?}"),
    }
}

// ==== the keychain ====

// A test never reads the keychain of the machine it runs on (a locked login keychain asks its owner
// for a password; azul-storage's tests keep their own keyring in memory for the same reason): this
// one is for a person to run on purpose, `cargo test -- --ignored`.
#[test]
#[ignore = "X-EDGE-C-2: reads the real keychain of this machine, run by hand: cargo test -p AzReview --lib -- --ignored reading_an_item"]
fn reading_an_item_that_is_not_there_never_gives_a_secret_and_never_panics() {
    // joined at run time: a literal that starts with the app's prefix and a hyphen would be taken for
    // a localization key by the check of the sources
    let item = [
        "azreview", "agent", "test", "item", "that", "does", "not", "exist",
    ]
    .join("-");
    let answer = AzulKeychain.read(&item);
    assert!(!matches!(answer, Ok(Some(_))), "{answer:?}");
}
