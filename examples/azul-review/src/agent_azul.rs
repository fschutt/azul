//! The engine's side of the agents: azul's HTTP client as the APIs' [`Transport`] and azul's
//! keyring as the keychain a key reference can name. These two are the only places of the agents
//! that need libazul; everything else is plain Rust and is tested without it.
//!
//! [`AzulTransport::send`] runs on the calling thread and blocks until the answer arrived or its
//! timeouts ended it (`HttpRequestConfig::http_request_blocking`: the engine refuses a blocking
//! request on the thread that runs its event loop, so a request is only ever sent from a worker, as
//! the pool's jobs are). It cannot be interrupted: a call the window cancels meanwhile drops its
//! answer when it arrives, and its timeout bounds how long that thread lives. A status that is no
//! success is an ANSWER here (the status and the body), not a failure of the transport: the
//! services say why in the body.

use azul::{
    error::{HttpError, KeyringResult, ResultHttpResponseHttpError},
    http::{HttpMethod, HttpRequestConfig},
    misc::Keyring,
    str::String as AzString,
    vec::U8Vec,
};

use crate::{
    agent_api::{HttpReply, HttpRequest, Transport, TransportError},
    agent_settings::Keychain,
};

/// Sends through azul's `HttpRequestConfig`. Call only from a worker thread.
#[derive(Debug, Clone)]
pub struct AzulTransport {
    user_agent: String,
}

impl AzulTransport {
    #[must_use]
    pub fn new(user_agent: &str) -> Self {
        Self {
            user_agent: user_agent.to_string(),
        }
    }
}

/// What azul's client says about a request that got no answer.
fn http_error_text(error: &HttpError) -> String {
    match error {
        HttpError::Timeout => "timed out".to_string(),
        HttpError::DnsFailed(s)
        | HttpError::InvalidUrl(s)
        | HttpError::ConnectionFailed(s)
        | HttpError::TlsError(s)
        | HttpError::IoError(s)
        | HttpError::Other(s) => s.as_str().to_string(),
        other => format!("{other:?}"),
    }
}

impl Transport for AzulTransport {
    fn send(&self, request: &HttpRequest) -> Result<HttpReply, TransportError> {
        // 0 means "no limit" to azul: the limits are at least one
        let mut config = HttpRequestConfig::create()
            .with_timeout(u64::from(request.timeout_secs.max(1)))
            .with_max_size(
                u64::try_from(request.max_response)
                    .unwrap_or(u64::MAX)
                    .max(1),
            )
            .with_user_agent(self.user_agent.as_str());
        for (name, value) in &request.headers {
            config = config.with_header(name.as_str(), value.as_str());
        }
        let answer = config.http_request_blocking(
            HttpMethod::Post,
            AzString::from(request.url.as_str()),
            U8Vec::from(request.body.clone()),
            AzString::from(request.content_type.as_str()),
        );
        match answer {
            ResultHttpResponseHttpError::Ok(response) => Ok(HttpReply {
                status: response.status_code,
                headers: response
                    .headers
                    .as_slice()
                    .iter()
                    .map(|h| (h.name.as_str().to_string(), h.value.as_str().to_string()))
                    .collect(),
                body: response.body.as_slice().to_vec(),
            }),
            ResultHttpResponseHttpError::Err(HttpError::Timeout) => Err(TransportError::Timeout),
            ResultHttpResponseHttpError::Err(other) => {
                Err(TransportError::Failed(http_error_text(&other)))
            }
        }
    }
}

/// The engine's keyring in the app's own namespace, as the keychain a key reference may name.
/// Call only from a worker thread (a read may wait for the user's answer to the system's question).
#[derive(Debug, Clone, Copy, Default)]
pub struct AzulKeychain;

impl Keychain for AzulKeychain {
    fn read(&self, item: &str) -> Result<Option<String>, String> {
        match Keyring::get_blocking(AzString::from(item)) {
            KeyringResult::Retrieved(secret) => Ok(Some(secret.as_str().to_string())),
            KeyringResult::NotFound => Ok(None),
            KeyringResult::Denied => Err("the keychain refused access".to_string()),
            KeyringResult::Unavailable => Err("there is no keychain here".to_string()),
            other => Err(format!("the keychain answered {other:?}")),
        }
    }
}

#[cfg(test)]
#[path = "agent_azul_test.rs"]
mod agent_azul_test;
