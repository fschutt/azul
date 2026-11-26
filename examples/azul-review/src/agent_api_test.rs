//! The two HTTP APIs as backends: the requests that are built (the key in a header and never in a
//! URL or a body), the replies that are read (the real error bodies of both services are fixtures,
//! the successes are shaped as the services' references describe them), and the whole path through
//! a transport that records what it was given. No network.

use std::{
    collections::VecDeque,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, PoisonError,
    },
};

use super::*;
use crate::{
    agent_backend::{never_cancelled, run_task, Body, ManualClock, Request, Task},
    agent_review::{build_call, Context, ReviewRequest, Utterance},
};

const ANTHROPIC_401: &str = include_str!("agent_fixtures/anthropic_401.json");
const ANTHROPIC_OK: &str = include_str!("agent_fixtures/anthropic_ok_shaped.json");
const GEMINI_400: &str = include_str!("agent_fixtures/gemini_400.json");
const GEMINI_OK: &str = include_str!("agent_fixtures/gemini_ok_shaped.json");

fn key() -> Secret {
    Secret::new("test-key-0123456789")
}

fn anthropic_config() -> AnthropicConfig {
    AnthropicConfig {
        model: "claude-haiku-5-5".to_string(),
        base_url: String::new(),
        timeout_secs: 90,
        max_tokens: 2048,
        effort: String::new(),
        key: key(),
        limits: Limits::default(),
    }
}

fn gemini_config() -> GeminiApiConfig {
    GeminiApiConfig {
        model: "gemini-2.5-flash".to_string(),
        base_url: String::new(),
        timeout_secs: 90,
        max_tokens: 2048,
        key: key(),
        limits: Limits::default(),
    }
}

fn request(task: Task) -> Request {
    Request::Review(ReviewRequest {
        task,
        conversation: 1,
        language: "en".to_string(),
        utterances: vec![Utterance {
            text: "what does this do?".to_string(),
            start_ms: 0,
            end_ms: 900,
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
        workspace: Some(std::path::PathBuf::from("/work/space")),
    })
}

fn with_call<T>(task: Task, f: impl FnOnce(&Call<'_>) -> T) -> T {
    let req = request(task);
    let review = req.as_review().expect("a review request");
    let call = build_call(&req, review, &Limits::default()).expect("builds");
    f(&call)
}

fn reply(status: u16, body: &str) -> HttpReply {
    HttpReply {
        status,
        headers: Vec::new(),
        body: body.as_bytes().to_vec(),
    }
}

fn body_of(request: &HttpRequest) -> Json {
    agent_json::parse(std::str::from_utf8(&request.body).expect("utf-8"), 1 << 20).expect("json")
}

// ==== Anthropic: the request ====

#[test]
fn the_anthropic_request_names_the_version_carries_the_key_in_a_header_and_sends_no_sampling() {
    with_call(Task::Editing, |call| {
        let r = anthropic_request(&anthropic_config(), call);
        assert_eq!(r.url, "https://api.anthropic.com/v1/messages");
        assert_eq!(r.content_type, "application/json");
        assert!(r
            .headers
            .contains(&("x-api-key".to_string(), "test-key-0123456789".to_string())));
        assert!(r
            .headers
            .contains(&("anthropic-version".to_string(), "2026-10-21".to_string())));
        assert_eq!(
            (r.timeout_secs, r.max_response),
            (90, Limits::default().max_http_response_bytes)
        );
        let body = body_of(&r);
        assert_eq!(agent_json::str_of(&body, "model"), Some("claude-haiku-5-5"));
        assert_eq!(agent_json::u64_of(&body, "max_tokens"), Some(2048));
        assert_eq!(
            agent_json::str_of(&body, "system"),
            Some(call.system.as_str())
        );
        let messages = agent_json::array_of(&body, "messages").expect("messages");
        assert_eq!(messages.len(), 1);
        assert_eq!(agent_json::str_of(&messages[0], "role"), Some("user"));
        assert_eq!(
            agent_json::str_of(&messages[0], "content"),
            Some(call.prompt.as_str())
        );
        for removed in [
            "temperature",
            "top_p",
            "top_k",
            "thinking",
            "output_config",
            "stream",
        ] {
            assert!(
                agent_json::get(&body, removed).is_none(),
                "{removed} is not sent"
            );
        }
        let all = format!("{} {}", r.url, String::from_utf8_lossy(&r.body));
        assert!(
            !all.contains("test-key-0123456789"),
            "the key is in a header only"
        );
    });
}

#[test]
fn an_effort_goes_in_output_config_only_when_it_is_set() {
    with_call(Task::Editing, |call| {
        let mut c = anthropic_config();
        c.effort = " low ".to_string();
        let body = body_of(&anthropic_request(&c, call));
        assert_eq!(
            agent_json::str_of(
                agent_json::get(&body, "output_config").expect("set"),
                "effort"
            ),
            Some("low")
        );
    });
}

#[test]
fn a_call_that_wanted_tools_is_told_it_has_none_and_one_that_did_not_is_not() {
    with_call(Task::Exploration, |call| {
        let body = body_of(&anthropic_request(&anthropic_config(), call));
        let system = agent_json::str_of(&body, "system").expect("system");
        assert!(
            system.starts_with(&call.system) && system.contains("you have no tools"),
            "{system}"
        );
    });
    with_call(Task::Editing, |call| {
        let body = body_of(&anthropic_request(&anthropic_config(), call));
        assert_eq!(
            agent_json::str_of(&body, "system"),
            Some(call.system.as_str())
        );
    });
}

#[test]
fn another_address_is_used_as_it_is_given() {
    with_call(Task::Editing, |call| {
        let mut c = anthropic_config();
        c.base_url = "http://127.0.0.1:8080/".to_string();
        assert_eq!(
            anthropic_request(&c, call).url,
            "http://127.0.0.1:8080/v1/messages"
        );
    });
}

// ==== Anthropic: the reply ====

#[test]
fn an_answer_is_the_text_blocks_only_with_the_tokens_and_the_model() {
    let c = read_anthropic(&reply(200, ANTHROPIC_OK), &anthropic_config()).expect("an answer");
    assert_eq!(
        c.text, "{\"answer\": \"ok\", \"suggestions\": []}",
        "the thinking block is not the answer"
    );
    assert_eq!(c.usage.input_tokens, Some(1_265));
    assert_eq!(c.usage.output_tokens, Some(43));
    assert_eq!(c.model, "claude-haiku-5-5");
    assert!(c.notes.is_empty());
}

#[test]
fn an_answer_cut_at_the_token_limit_is_kept_and_says_so_or_is_an_error_when_nothing_was_said() {
    let cut = r#"{"model":"m","content":[{"type":"text","text":"{\"answer\": \"half"}],"stop_reason":"max_tokens","usage":{"input_tokens":1,"output_tokens":2048}}"#;
    let c = read_anthropic(&reply(200, cut), &anthropic_config()).expect("what there is");
    assert!(
        c.notes.iter().any(|n| n.contains("2048 tokens")),
        "{:?}",
        c.notes
    );
    let nothing = r#"{"model":"m","content":[{"type":"thinking","thinking":""}],"stop_reason":"max_tokens","usage":{}}"#;
    match read_anthropic(&reply(200, nothing), &anthropic_config()).expect_err("nothing said") {
        AgentError::BadOutput { why, .. } => {
            assert!(why.contains("raise the longest answer"), "{why}")
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_refusal_is_told_with_its_category() {
    let refusal = r#"{"model":"m","content":[],"stop_reason":"refusal","stop_details":{"type":"refusal","category":"cyber","explanation":"declined"},"usage":{}}"#;
    match read_anthropic(&reply(200, refusal), &anthropic_config()).expect_err("refused") {
        AgentError::Refused { detail } => assert_eq!(detail, "cyber: declined"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn an_answer_that_is_no_json_or_has_no_text_is_a_bad_output() {
    for body in [
        "not json at all",
        "{\"content\": []}",
        "{\"content\": [{\"type\": \"text\", \"text\": \"  \"}]}",
    ] {
        let e = read_anthropic(&reply(200, body), &anthropic_config()).expect_err(body);
        assert!(matches!(e, AgentError::BadOutput { .. }), "{body}: {e:?}");
    }
}

#[test]
fn the_real_error_of_a_wrong_key_is_an_http_failure_in_the_services_words() {
    match read_anthropic(&reply(401, ANTHROPIC_401), &anthropic_config()).expect_err("401") {
        AgentError::Http {
            status,
            kind,
            detail,
        } => {
            assert_eq!(
                (status, kind.as_str(), detail.as_str()),
                (401, "authentication_error", "invalid x-api-key")
            );
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn the_other_failures_are_told_apart_by_their_status() {
    let config = anthropic_config();
    let not_found =
        r#"{"type":"error","error":{"type":"not_found_error","message":"model: claude-nope"}}"#;
    match read_anthropic(&reply(404, not_found), &config).expect_err("404") {
        AgentError::Settings { why } => assert!(
            why.contains("claude-haiku-5-5") && why.contains("claude-nope"),
            "{why}"
        ),
        other => panic!("{other:?}"),
    }
    let limited = HttpReply {
        status: 429,
        headers: vec![("Retry-After".to_string(), " 20 ".to_string())],
        body: br#"{"type":"error","error":{"type":"rate_limit_error","message":"too many"}}"#
            .to_vec(),
    };
    assert_eq!(
        read_anthropic(&limited, &config).expect_err("429"),
        AgentError::RateLimited {
            retry_after_secs: Some(20),
            detail: "too many".to_string()
        }
    );
    let overloaded =
        r#"{"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}}"#;
    let e = read_anthropic(&reply(529, overloaded), &config).expect_err("529");
    assert!(
        e.is_retryable() && matches!(e, AgentError::Http { status: 529, .. }),
        "{e:?}"
    );
    match read_anthropic(&reply(502, "<html>bad gateway</html>"), &config).expect_err("502") {
        AgentError::Http { kind, detail, .. } => assert_eq!(
            (kind.as_str(), detail.as_str()),
            ("error", "<html>bad gateway</html>")
        ),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_key_the_service_echoes_is_scrubbed_from_what_is_shown() {
    let echo = r#"{"type":"error","error":{"type":"authentication_error","message":"invalid key test-key-0123456789"}}"#;
    match read_anthropic(&reply(401, echo), &anthropic_config()).expect_err("401") {
        AgentError::Http { detail, .. } => {
            assert!(!detail.contains("test-key-0123456789"), "{detail}")
        }
        other => panic!("{other:?}"),
    }
}

// ==== Gemini ====

#[test]
fn the_gemini_request_has_the_model_in_the_path_the_key_in_a_header_and_json_for_a_call_without_tools(
) {
    with_call(Task::Editing, |call| {
        let r = gemini_request(&gemini_config(), call).expect("builds");
        assert_eq!(r.url, "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-flash:generateContent");
        assert!(r.headers.contains(&(
            "x-goog-api-key".to_string(),
            "test-key-0123456789".to_string()
        )));
        let body = body_of(&r);
        let parts = agent_json::get(&body, "systemInstruction")
            .and_then(|s| agent_json::array_of(s, "parts"))
            .expect("parts");
        assert_eq!(
            agent_json::str_of(&parts[0], "text"),
            Some(call.system.as_str())
        );
        let contents = agent_json::array_of(&body, "contents").expect("contents");
        assert_eq!(agent_json::str_of(&contents[0], "role"), Some("user"));
        let generation = agent_json::get(&body, "generationConfig").expect("config");
        assert_eq!(
            agent_json::u64_of(generation, "maxOutputTokens"),
            Some(2048)
        );
        assert_eq!(
            agent_json::str_of(generation, "responseMimeType"),
            Some("application/json")
        );
        assert!(!format!("{} {}", r.url, String::from_utf8_lossy(&r.body))
            .contains("test-key-0123456789"));
    });
    with_call(Task::Exploration, |call| {
        let body = body_of(&gemini_request(&gemini_config(), call).expect("builds"));
        assert!(agent_json::get(
            agent_json::get(&body, "generationConfig").expect("config"),
            "responseMimeType"
        )
        .is_none());
        let parts = agent_json::get(&body, "systemInstruction")
            .and_then(|s| agent_json::array_of(s, "parts"))
            .expect("parts");
        assert!(agent_json::str_of(&parts[0], "text")
            .expect("text")
            .contains("you have no tools"));
    });
}

#[test]
fn a_model_name_that_cannot_be_a_path_is_refused_before_anything_is_sent() {
    with_call(Task::Editing, |call| {
        for bad in [
            "",
            "a/b",
            "m?key=1",
            "m:generateContent",
            "a b",
            &"m".repeat(200),
        ] {
            let mut c = gemini_config();
            c.model = bad.to_string();
            assert!(
                matches!(gemini_request(&c, call), Err(AgentError::Settings { .. })),
                "{bad:?}"
            );
        }
        assert!(is_model_path_safe("gemini-3-pro-preview"));
    });
}

#[test]
fn a_gemini_answer_skips_the_thinking_parts_and_counts_the_tokens() {
    let c = read_gemini(&reply(200, GEMINI_OK), &gemini_config()).expect("an answer");
    assert_eq!(c.text, "{\"answer\": \"ok\", \"suggestions\": []}");
    assert_eq!(
        (c.usage.input_tokens, c.usage.output_tokens),
        (Some(1_450), Some(14))
    );
    assert_eq!(c.model, "gemini-2.5-flash");
}

#[test]
fn a_blocked_prompt_or_answer_is_a_refusal_and_a_cut_answer_is_kept_with_a_note() {
    let blocked = r#"{"promptFeedback":{"blockReason":"SAFETY"}}"#;
    assert!(matches!(
        read_gemini(&reply(200, blocked), &gemini_config()),
        Err(AgentError::Refused { .. })
    ));
    let answer_blocked = r#"{"candidates":[{"finishReason":"SAFETY"}]}"#;
    assert!(matches!(
        read_gemini(&reply(200, answer_blocked), &gemini_config()),
        Err(AgentError::Refused { .. })
    ));
    let cut = r#"{"candidates":[{"content":{"parts":[{"text":"{\"answer\": \"half"}]},"finishReason":"MAX_TOKENS"}]}"#;
    let c = read_gemini(&reply(200, cut), &gemini_config()).expect("what there is");
    assert!(
        c.notes.iter().any(|n| n.contains("2048 tokens")),
        "{:?}",
        c.notes
    );
    let nothing = r#"{"candidates":[{"content":{"parts":[]},"finishReason":"MAX_TOKENS"}]}"#;
    assert!(matches!(
        read_gemini(&reply(200, nothing), &gemini_config()),
        Err(AgentError::BadOutput { .. })
    ));
    assert!(matches!(
        read_gemini(&reply(200, "{}"), &gemini_config()),
        Err(AgentError::BadOutput { .. })
    ));
}

#[test]
fn the_real_error_of_a_wrong_gemini_key_is_an_http_failure_in_the_services_words() {
    match read_gemini(&reply(400, GEMINI_400), &gemini_config()).expect_err("400") {
        AgentError::Http {
            status,
            kind,
            detail,
        } => {
            assert_eq!((status, kind.as_str()), (400, "INVALID_ARGUMENT"));
            assert_eq!(detail, "API key not valid. Please pass a valid API key.");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn the_other_gemini_failures_are_told_apart_by_their_status() {
    let config = gemini_config();
    let missing = r#"{"error":{"code":404,"message":"models/gemini-nope is not found","status":"NOT_FOUND"}}"#;
    assert!(matches!(
        read_gemini(&reply(404, missing), &config),
        Err(AgentError::Settings { .. })
    ));
    let limited = r#"{"error":{"code":429,"message":"Quota exceeded","status":"RESOURCE_EXHAUSTED","details":[{"@type":"type.googleapis.com/google.rpc.RetryInfo","retryDelay":"31s"}]}}"#;
    assert_eq!(
        read_gemini(&reply(429, limited), &config).expect_err("429"),
        AgentError::RateLimited {
            retry_after_secs: Some(31),
            detail: "Quota exceeded".to_string()
        }
    );
    let down =
        r#"{"error":{"code":503,"message":"The model is overloaded","status":"UNAVAILABLE"}}"#;
    let e = read_gemini(&reply(503, down), &config).expect_err("503");
    assert!(
        e.is_retryable() && matches!(e, AgentError::Http { status: 503, .. }),
        "{e:?}"
    );
}

// ==== through a transport ====

/// A transport that answers from a script and records what it was asked.
struct Recording {
    replies: Mutex<VecDeque<Result<HttpReply, TransportError>>>,
    seen: Mutex<Vec<HttpRequest>>,
    on_send: Option<Box<dyn Fn() + Send + Sync>>,
}

impl Recording {
    fn answering(replies: Vec<Result<HttpReply, TransportError>>) -> Arc<Self> {
        Arc::new(Self {
            replies: Mutex::new(replies.into()),
            seen: Mutex::new(Vec::new()),
            on_send: None,
        })
    }

    fn requests(&self) -> Vec<HttpRequest> {
        self.seen
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

impl Transport for Recording {
    fn send(&self, request: &HttpRequest) -> Result<HttpReply, TransportError> {
        self.seen
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(request.clone());
        if let Some(on_send) = &self.on_send {
            on_send();
        }
        self.replies
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .pop_front()
            .unwrap_or_else(|| Err(TransportError::Failed("nothing scripted".to_string())))
    }
}

fn run_anthropic(
    transport: Arc<dyn Transport>,
    cancel: &dyn Fn() -> bool,
) -> Result<crate::agent_backend::Response, AgentError> {
    let mut backend = AnthropicApi {
        config: anthropic_config(),
        transport,
    };
    let mut clock = ManualClock::new();
    let mut ctl = Control::new(&mut clock, cancel);
    run_task(
        &mut backend,
        &request(Task::Editing),
        &Limits::default(),
        &mut ctl,
    )
}

#[test]
fn the_whole_path_sends_one_request_and_reads_the_answer() {
    let transport = Recording::answering(vec![Ok(reply(200, ANTHROPIC_OK))]);
    let response = run_anthropic(transport.clone(), &never_cancelled).expect("answers");
    let Body::Review(reviewed) = &response.body else {
        panic!("a review answer")
    };
    assert_eq!(reviewed.answer, "ok");
    assert_eq!(
        response.identity,
        Identity {
            kind: BackendKind::AnthropicApi,
            model: "claude-haiku-5-5".to_string()
        }
    );
    assert_eq!(transport.requests().len(), 1);
    assert_eq!(response.usage.output_tokens, Some(43));
}

#[test]
fn a_call_cancelled_before_it_is_sent_is_never_sent() {
    let transport = Recording::answering(vec![Ok(reply(200, ANTHROPIC_OK))]);
    let e = run_anthropic(transport.clone(), &|| true).expect_err("cancelled");
    assert_eq!(e, AgentError::Cancelled);
    assert!(transport.requests().is_empty());
}

#[test]
fn a_call_cancelled_while_the_request_was_out_drops_the_answer() {
    let flag = Arc::new(AtomicBool::new(false));
    let setter = flag.clone();
    let transport = Arc::new(Recording {
        replies: Mutex::new(vec![Ok(reply(200, ANTHROPIC_OK))].into()),
        seen: Mutex::new(Vec::new()),
        on_send: Some(Box::new(move || setter.store(true, Ordering::Relaxed))),
    });
    let reader = flag.clone();
    let e =
        run_anthropic(transport, &move || reader.load(Ordering::Relaxed)).expect_err("cancelled");
    assert_eq!(e, AgentError::Cancelled);
}

#[test]
fn a_transport_that_timed_out_or_failed_is_said_in_words_and_without_the_key() {
    let e = run_anthropic(
        Recording::answering(vec![Err(TransportError::Timeout)]),
        &never_cancelled,
    )
    .expect_err("timeout");
    assert_eq!(e, AgentError::Timeout { secs: 90 });
    let failed = TransportError::Failed("connection reset sending test-key-0123456789".to_string());
    match run_anthropic(Recording::answering(vec![Err(failed)]), &never_cancelled)
        .expect_err("failed")
    {
        AgentError::Network { why } => assert!(
            !why.contains("test-key-0123456789") && why.contains("connection reset"),
            "{why}"
        ),
        other => panic!("{other:?}"),
    }
}

#[test]
fn the_gemini_path_goes_the_same_way() {
    let transport = Recording::answering(vec![Ok(reply(200, GEMINI_OK))]);
    let mut backend = GeminiApi {
        config: gemini_config(),
        transport: transport.clone(),
    };
    let mut clock = ManualClock::new();
    let mut ctl = Control::new(&mut clock, &never_cancelled);
    let response = run_task(
        &mut backend,
        &request(Task::Editing),
        &Limits::default(),
        &mut ctl,
    )
    .expect("answers");
    let Body::Review(reviewed) = &response.body else {
        panic!("a review answer")
    };
    assert_eq!(reviewed.answer, "ok");
    assert_eq!(response.identity.kind, BackendKind::GeminiApi);
    assert!(transport.requests()[0].url.ends_with(":generateContent"));
    // a model that cannot be a path never reaches the transport
    backend.config.model = "a/b".to_string();
    let mut ctl = Control::new(&mut clock, &never_cancelled);
    assert!(matches!(
        run_task(
            &mut backend,
            &request(Task::Editing),
            &Limits::default(),
            &mut ctl
        ),
        Err(AgentError::Settings { .. })
    ));
    assert_eq!(transport.requests().len(), 1);
}
