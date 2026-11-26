//! The HTTP APIs as backends: the Anthropic Messages API and the Gemini `generateContent` API.
//!
//! They talk through a [`Transport`] (a request in, a reply out), so everything here is plain data in
//! and out: the request is built ([`anthropic_request`], [`gemini_request`]), the reply is read
//! ([`read_anthropic`], [`read_gemini`]), and the transport is whatever sends bytes - azul's own HTTP
//! client in the app (`agent_azul.rs`), a recording stand-in in the tests, a local server in the end to
//! end test. Neither API's key is ever in a URL (an HTTP library's error texts name the URL): it goes
//! in the header `x-api-key` and `x-goog-api-key`, and every message that leaves here is scrubbed of
//! it.
//!
//! What the APIs cannot do for an agent: they have no tools of their own here, so an exploration
//! through an API answers from the code it was sent, and says so in its instructions.
//!
//! Anthropic: `anthropic-version: 2026-10-21` (the version the API's own raw-HTTP reference uses; none
//! of its changes touch what is sent here: no sampling parameters, no prefill, no documents in
//! assistant turns); the reply's `content` holds `thinking` blocks on the newer models next to the
//! `text` ones, only the text is the answer; `stop_reason` `refusal` and `max_tokens` are told.

use std::sync::Arc;

use crate::{
    agent_backend::{
        scrub, AgentError, Backend, BackendKind, Call, Completion, Control, Identity, Tools, Usage,
    },
    agent_json::{self, json, Json},
    agent_limits::{cap_chars, Limits},
    agent_settings::Secret,
};

/// The API version the requests to Anthropic name.
pub const ANTHROPIC_VERSION: &str = "2026-10-21";

/// Appended to the standing instructions of a call that wanted tools: the APIs give none.
const NO_TOOLS: &str = "\n\nIn this run you have no tools and cannot read files: answer from the code and the context given, and say so when you would need to read more.";

// ==== The transport ====

/// A request to send.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpRequest {
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    pub content_type: String,
    /// The whole call may take this long.
    pub timeout_secs: u32,
    /// A bigger answer is refused.
    pub max_response: usize,
}

/// What came back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpReply {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

/// Why nothing came back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransportError {
    Timeout,
    /// The text names no secret (the transport scrubs what it says).
    Failed(String),
}

/// What sends a request and waits for the reply, on the worker's thread.
pub trait Transport: Send + Sync {
    /// # Errors
    /// [`TransportError`]: a timeout, or why the request could not be sent or answered.
    fn send(&self, request: &HttpRequest) -> Result<HttpReply, TransportError>;
}

/// The reply's header `name`, whatever its case.
#[must_use]
pub fn header<'a>(reply: &'a HttpReply, name: &str) -> Option<&'a str> {
    reply
        .headers
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.as_str())
}

/// `base` and `path` joined by one slash (an empty base is `default`).
fn url(base: &str, default: &str, path: &str) -> String {
    let base = if base.trim().is_empty() {
        default
    } else {
        base.trim()
    };
    format!(
        "{}/{}",
        base.trim_end_matches('/'),
        path.trim_start_matches('/')
    )
}

/// What to say about a transport that failed.
fn transport_error(error: TransportError, timeout_secs: u32, secrets: &[&str]) -> AgentError {
    match error {
        TransportError::Timeout => AgentError::Timeout { secs: timeout_secs },
        TransportError::Failed(why) => AgentError::Network {
            why: cap_chars(&scrub(why.trim(), secrets), 300).0,
        },
    }
}

/// The instructions of a call, with the note that the API has no tools when the call wanted some.
fn system_of(call: &Call<'_>) -> String {
    if call.tools == Tools::ReadOnly {
        format!("{}{NO_TOOLS}", call.system)
    } else {
        call.system.clone()
    }
}

/// The words of a failure: the service's own, cut and scrubbed.
fn said(message: &str, secrets: &[&str]) -> String {
    cap_chars(&scrub(message.trim(), secrets), 300).0
}

// ==== Anthropic ====

/// What an Anthropic API backend is set up with.
#[derive(Debug, Clone)]
pub struct AnthropicConfig {
    pub model: String,
    /// Empty: `https://api.anthropic.com`.
    pub base_url: String,
    pub timeout_secs: u32,
    pub max_tokens: u32,
    /// `low` to `max`; empty: the model's own.
    pub effort: String,
    pub key: Secret,
    pub limits: Limits,
}

/// The Anthropic Messages API as a backend.
pub struct AnthropicApi {
    pub config: AnthropicConfig,
    pub transport: Arc<dyn Transport>,
}

/// The body of the request.
#[must_use]
pub fn anthropic_body(config: &AnthropicConfig, call: &Call<'_>) -> Json {
    let mut body = json!({
        "model": config.model.trim(),
        "max_tokens": config.max_tokens,
        "system": system_of(call),
        "messages": [{"role": "user", "content": call.prompt}],
    });
    if !config.effort.trim().is_empty() {
        body["output_config"] = json!({"effort": config.effort.trim()});
    }
    body
}

/// The request for `call`.
#[must_use]
pub fn anthropic_request(config: &AnthropicConfig, call: &Call<'_>) -> HttpRequest {
    HttpRequest {
        url: url(&config.base_url, "https://api.anthropic.com", "v1/messages"),
        headers: vec![
            ("x-api-key".to_string(), config.key.expose().to_string()),
            (
                "anthropic-version".to_string(),
                ANTHROPIC_VERSION.to_string(),
            ),
            ("accept".to_string(), "application/json".to_string()),
        ],
        body: agent_json::to_text(&anthropic_body(config, call)).into_bytes(),
        content_type: "application/json".to_string(),
        timeout_secs: config.timeout_secs,
        max_response: config.limits.max_http_response_bytes,
    }
}

/// The tokens a call read: what was new, what was written to the cache, what came from it.
fn anthropic_input_tokens(usage: &Json) -> Option<u64> {
    let found: Vec<u64> = [
        "input_tokens",
        "cache_creation_input_tokens",
        "cache_read_input_tokens",
    ]
    .iter()
    .filter_map(|k| agent_json::u64_of(usage, k))
    .collect();
    (!found.is_empty()).then(|| found.iter().sum())
}

/// The failure an error status says.
fn anthropic_error(reply: &HttpReply, config: &AnthropicConfig) -> AgentError {
    let secret = config.key.expose();
    let secrets = [secret];
    let text = String::from_utf8_lossy(&reply.body);
    let body = agent_json::parse(&text, config.limits.max_http_response_bytes).ok();
    let error = body.as_ref().and_then(|b| agent_json::get(b, "error"));
    let kind = error
        .and_then(|e| agent_json::str_of(e, "type"))
        .unwrap_or("")
        .to_string();
    let message = error
        .and_then(|e| agent_json::str_of(e, "message"))
        .map_or_else(|| text.trim().to_string(), str::to_string);
    let detail = said(&message, &secrets);
    match reply.status {
        404 if kind == "not_found_error" || message.to_lowercase().contains("model") => {
            AgentError::Settings {
                why: format!(
                    "the model {:?} is not available to this key: {detail}",
                    config.model.trim()
                ),
            }
        }
        429 => AgentError::RateLimited {
            retry_after_secs: header(reply, "retry-after").and_then(|v| v.trim().parse().ok()),
            detail,
        },
        status => AgentError::Http {
            status,
            kind: if kind.is_empty() {
                "error".to_string()
            } else {
                kind
            },
            detail,
        },
    }
}

/// Reads the reply of the Messages API.
///
/// # Errors
/// [`AgentError`]: what an error status says, a refusal, an answer that holds no text.
pub fn read_anthropic(
    reply: &HttpReply,
    config: &AnthropicConfig,
) -> Result<Completion, AgentError> {
    if !(200..300).contains(&reply.status) {
        return Err(anthropic_error(reply, config));
    }
    let text = String::from_utf8_lossy(&reply.body);
    let body = agent_json::parse(&text, config.limits.max_http_response_bytes).map_err(|e| {
        AgentError::BadOutput {
            why: format!("the service's answer is no JSON ({e:?})"),
            excerpt: cap_chars(text.trim(), 200).0,
        }
    })?;
    let stop = agent_json::str_of(&body, "stop_reason").unwrap_or("");
    if stop == "refusal" {
        let details = agent_json::get(&body, "stop_details");
        let category = details
            .and_then(|d| agent_json::str_of(d, "category"))
            .unwrap_or("");
        let explanation = details
            .and_then(|d| agent_json::str_of(d, "explanation"))
            .unwrap_or("");
        let detail = [category, explanation]
            .iter()
            .filter(|s| !s.is_empty())
            .copied()
            .collect::<Vec<_>>()
            .join(": ");
        return Err(AgentError::Refused {
            detail: if detail.is_empty() {
                "the model declined".to_string()
            } else {
                said(&detail, &[])
            },
        });
    }
    let answer: String = agent_json::array_of(&body, "content")
        .unwrap_or(&[])
        .iter()
        .filter(|block| agent_json::str_of(block, "type") == Some("text"))
        .filter_map(|block| agent_json::str_of(block, "text"))
        .collect::<Vec<_>>()
        .join("");
    let mut notes = Vec::new();
    if stop == "max_tokens" {
        if answer.trim().is_empty() {
            return Err(AgentError::BadOutput {
                why: format!(
                    "the answer was cut off at {} tokens before it said anything: raise the longest answer in the settings",
                    config.max_tokens
                ),
                excerpt: String::new(),
            });
        }
        notes.push(format!(
            "the service stopped the answer at {} tokens: raise the longest answer in the settings",
            config.max_tokens
        ));
    }
    if answer.trim().is_empty() {
        return Err(AgentError::BadOutput {
            why: "the answer holds no text".to_string(),
            excerpt: cap_chars(text.trim(), 200).0,
        });
    }
    let usage = agent_json::get(&body, "usage");
    Ok(Completion {
        text: answer,
        usage: Usage {
            input_tokens: usage.and_then(anthropic_input_tokens),
            output_tokens: usage.and_then(|u| agent_json::u64_of(u, "output_tokens")),
            cost_usd: None,
            elapsed_ms: 0,
            turns: Some(1),
        },
        model: agent_json::str_of(&body, "model").unwrap_or("").to_string(),
        notes,
    })
}

impl Backend for AnthropicApi {
    fn identity(&self) -> Identity {
        Identity {
            kind: BackendKind::AnthropicApi,
            model: self.config.model.trim().to_string(),
        }
    }

    fn complete(
        &mut self,
        call: &Call<'_>,
        ctl: &mut Control<'_>,
    ) -> Result<Completion, AgentError> {
        if ctl.cancelled() {
            return Err(AgentError::Cancelled);
        }
        let key = self.config.key.expose().to_string();
        let reply = self
            .transport
            .send(&anthropic_request(&self.config, call))
            .map_err(|e| transport_error(e, self.config.timeout_secs, &[&key]))?;
        // a blocking request cannot be interrupted: a call cancelled meanwhile drops its answer
        if ctl.cancelled() {
            return Err(AgentError::Cancelled);
        }
        read_anthropic(&reply, &self.config)
    }
}

// ==== Gemini ====

/// What a Gemini API backend is set up with.
#[derive(Debug, Clone)]
pub struct GeminiApiConfig {
    pub model: String,
    /// Empty: `https://generativelanguage.googleapis.com`.
    pub base_url: String,
    pub timeout_secs: u32,
    pub max_tokens: u32,
    pub key: Secret,
    pub limits: Limits,
}

/// The Gemini `generateContent` API as a backend.
pub struct GeminiApi {
    pub config: GeminiApiConfig,
    pub transport: Arc<dyn Transport>,
}

/// Whether `model` can be a part of a URL path as it is.
#[must_use]
pub fn is_model_path_safe(model: &str) -> bool {
    !model.is_empty()
        && model.len() <= 120
        && model
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
}

/// The body of the request: the instructions, the request, and for a call without tools an answer
/// that is JSON.
#[must_use]
pub fn gemini_body(config: &GeminiApiConfig, call: &Call<'_>) -> Json {
    let mut generation = json!({"maxOutputTokens": config.max_tokens});
    if call.tools == Tools::None {
        generation["responseMimeType"] = json!("application/json");
    }
    json!({
        "systemInstruction": {"parts": [{"text": system_of(call)}]},
        "contents": [{"role": "user", "parts": [{"text": call.prompt}]}],
        "generationConfig": generation,
    })
}

/// The request for `call`.
///
/// # Errors
/// [`AgentError::Settings`] when the model's name cannot be a part of a URL.
pub fn gemini_request(
    config: &GeminiApiConfig,
    call: &Call<'_>,
) -> Result<HttpRequest, AgentError> {
    let model = config.model.trim();
    if !is_model_path_safe(model) {
        return Err(AgentError::Settings {
            why: format!("{model:?} cannot be the name of a Gemini model (letters, digits, . - _)"),
        });
    }
    Ok(HttpRequest {
        url: url(
            &config.base_url,
            "https://generativelanguage.googleapis.com",
            &format!("v1beta/models/{model}:generateContent"),
        ),
        headers: vec![
            (
                "x-goog-api-key".to_string(),
                config.key.expose().to_string(),
            ),
            ("accept".to_string(), "application/json".to_string()),
        ],
        body: agent_json::to_text(&gemini_body(config, call)).into_bytes(),
        content_type: "application/json".to_string(),
        timeout_secs: config.timeout_secs,
        max_response: config.limits.max_http_response_bytes,
    })
}

/// The seconds a `RetryInfo` detail of an error asks to wait (`"31s"`).
fn gemini_retry_after(error: &Json) -> Option<u32> {
    agent_json::array_of(error, "details")?
        .iter()
        .find_map(|d| agent_json::str_of(d, "retryDelay"))
        .and_then(|delay| delay.trim_end_matches('s').split('.').next()?.parse().ok())
}

/// The failure an error status says.
fn gemini_error(reply: &HttpReply, config: &GeminiApiConfig) -> AgentError {
    let secret = config.key.expose();
    let secrets = [secret];
    let text = String::from_utf8_lossy(&reply.body);
    let body = agent_json::parse(&text, config.limits.max_http_response_bytes).ok();
    let error = body.as_ref().and_then(|b| agent_json::get(b, "error"));
    let status_word = error
        .and_then(|e| agent_json::str_of(e, "status"))
        .unwrap_or("")
        .to_string();
    let message = error
        .and_then(|e| agent_json::str_of(e, "message"))
        .map_or_else(|| text.trim().to_string(), str::to_string);
    let detail = said(&message, &secrets);
    match reply.status {
        404 => AgentError::Settings {
            why: format!(
                "the model {:?} is not available to this key: {detail}",
                config.model.trim()
            ),
        },
        429 => AgentError::RateLimited {
            retry_after_secs: error
                .and_then(gemini_retry_after)
                .or_else(|| header(reply, "retry-after").and_then(|v| v.trim().parse().ok())),
            detail,
        },
        status => AgentError::Http {
            status,
            kind: if status_word.is_empty() {
                "error".to_string()
            } else {
                status_word
            },
            detail,
        },
    }
}

/// Reads the reply of `generateContent`.
///
/// # Errors
/// [`AgentError`]: what an error status says, a prompt or an answer the service blocked, an answer
/// that holds no text.
pub fn read_gemini(reply: &HttpReply, config: &GeminiApiConfig) -> Result<Completion, AgentError> {
    if !(200..300).contains(&reply.status) {
        return Err(gemini_error(reply, config));
    }
    let text = String::from_utf8_lossy(&reply.body);
    let body = agent_json::parse(&text, config.limits.max_http_response_bytes).map_err(|e| {
        AgentError::BadOutput {
            why: format!("the service's answer is no JSON ({e:?})"),
            excerpt: cap_chars(text.trim(), 200).0,
        }
    })?;
    if let Some(reason) =
        agent_json::get(&body, "promptFeedback").and_then(|f| agent_json::str_of(f, "blockReason"))
    {
        return Err(AgentError::Refused {
            detail: format!("the request was blocked ({reason})"),
        });
    }
    let candidate = agent_json::array_of(&body, "candidates").and_then(<[Json]>::first);
    let finish = candidate
        .and_then(|c| agent_json::str_of(c, "finishReason"))
        .unwrap_or("");
    let answer: String = candidate
        .and_then(|c| agent_json::get(c, "content"))
        .and_then(|content| agent_json::array_of(content, "parts"))
        .unwrap_or(&[])
        .iter()
        .filter(|part| agent_json::bool_of(part, "thought") != Some(true))
        .filter_map(|part| agent_json::str_of(part, "text"))
        .collect::<Vec<_>>()
        .join("");
    let mut notes = Vec::new();
    if matches!(
        finish,
        "SAFETY" | "PROHIBITED_CONTENT" | "BLOCKLIST" | "SPII" | "RECITATION"
    ) && answer.trim().is_empty()
    {
        return Err(AgentError::Refused {
            detail: format!("the answer was blocked ({finish})"),
        });
    }
    if finish == "MAX_TOKENS" {
        if answer.trim().is_empty() {
            return Err(AgentError::BadOutput {
                why: format!(
                    "the answer was cut off at {} tokens before it said anything: raise the longest answer in the settings",
                    config.max_tokens
                ),
                excerpt: String::new(),
            });
        }
        notes.push(format!(
            "the service stopped the answer at {} tokens: raise the longest answer in the settings",
            config.max_tokens
        ));
    }
    if answer.trim().is_empty() {
        return Err(AgentError::BadOutput {
            why: "the answer holds no text".to_string(),
            excerpt: cap_chars(text.trim(), 200).0,
        });
    }
    let usage = agent_json::get(&body, "usageMetadata");
    Ok(Completion {
        text: answer,
        usage: Usage {
            input_tokens: usage.and_then(|u| agent_json::u64_of(u, "promptTokenCount")),
            output_tokens: usage.and_then(|u| agent_json::u64_of(u, "candidatesTokenCount")),
            cost_usd: None,
            elapsed_ms: 0,
            turns: Some(1),
        },
        model: agent_json::str_of(&body, "modelVersion")
            .unwrap_or("")
            .to_string(),
        notes,
    })
}

impl Backend for GeminiApi {
    fn identity(&self) -> Identity {
        Identity {
            kind: BackendKind::GeminiApi,
            model: self.config.model.trim().to_string(),
        }
    }

    fn complete(
        &mut self,
        call: &Call<'_>,
        ctl: &mut Control<'_>,
    ) -> Result<Completion, AgentError> {
        if ctl.cancelled() {
            return Err(AgentError::Cancelled);
        }
        let request = gemini_request(&self.config, call)?;
        let key = self.config.key.expose().to_string();
        let reply = self
            .transport
            .send(&request)
            .map_err(|e| transport_error(e, self.config.timeout_secs, &[&key]))?;
        if ctl.cancelled() {
            return Err(AgentError::Cancelled);
        }
        read_gemini(&reply, &self.config)
    }
}

#[cfg(test)]
#[path = "agent_api_test.rs"]
mod agent_api_test;
