//! [`Transport`] over azul's HTTP client (`HttpRequestConfig::http_request`),
//! for the apps. Feature `azul`.
//!
//! azul's public HTTP call is request / resume: `http_request` performs the
//! request where it is called (on desktop, synchronously) and queues its
//! answer, which the UI thread delivers to a `ResumeCallback` on its next pump
//! (the thread-poll tick while a `Thread` runs). A [`crate::Drive`] is blocking
//! and runs on an azul `Thread`, so [`AzulTransport::send`] issues the request
//! from that thread and waits for the resume callback to hand the answer back
//! through a channel. The cost is one UI pump per request (about 16 ms).
//!
//! Never call it from a UI callback: the answer is delivered BY the UI thread,
//! so waiting for it there would wait forever (the timeout ends the wait).

use std::{
    sync::{mpsc, Mutex},
    time::Duration,
};

use azul::{
    error::HttpError,
    http::{HttpGetResult, HttpMethod, HttpRequestConfig},
    prelude::*,
    vec::U8Vec,
};

use crate::{HttpCall, HttpReply, Method, Transport};

/// Seconds a request may take before azul's client gives up.
pub const DEFAULT_TIMEOUT_SECS: u64 = 60;
/// Extra seconds to wait for the UI thread to deliver an answer.
const DELIVERY_GRACE_SECS: u64 = 30;

/// Sends through azul's `HttpRequestConfig`. Call only from an azul `Thread`.
#[derive(Debug, Clone)]
pub struct AzulTransport {
    user_agent: String,
    timeout_secs: u64,
}

impl AzulTransport {
    #[must_use]
    pub fn new(user_agent: &str) -> Self {
        AzulTransport {
            user_agent: user_agent.to_string(),
            timeout_secs: DEFAULT_TIMEOUT_SECS,
        }
    }

    #[must_use]
    pub fn with_timeout(mut self, secs: u64) -> Self {
        self.timeout_secs = secs.max(1);
        self
    }
}

/// What the resume callback hands the waiting thread.
type Answer = Result<HttpReply, String>;

/// The resume callback's data: where the waiting thread listens.
struct Waiter {
    reply: Mutex<Option<mpsc::Sender<Answer>>>,
}

fn http_error_text(e: &HttpError) -> String {
    match e {
        HttpError::Timeout => String::from("timed out"),
        HttpError::InvalidUrl(s)
        | HttpError::ConnectionFailed(s)
        | HttpError::TlsError(s)
        | HttpError::IoError(s)
        | HttpError::Other(s) => s.as_str().to_string(),
        other => format!("{other:?}"),
    }
}

/// The answer in an `http_request` result.
fn answer_of(result: RefAny) -> Answer {
    let Some(answer) = HttpGetResult::downcast(result).into_option() else {
        return Err(String::from("the HTTP client gave no answer"));
    };
    match answer.result.into_result() {
        Ok(response) => Ok(HttpReply {
            status: response.status_code,
            headers: response
                .headers
                .as_slice()
                .iter()
                .map(|h| (h.name.as_str().to_string(), h.value.as_str().to_string()))
                .collect(),
            body: response.body.as_slice().to_vec(),
        }),
        Err(e) => Err(http_error_text(&e)),
    }
}

/// Runs on the UI thread: passes the answer to the thread waiting in `send`.
extern "C" fn on_answer(mut data: RefAny, _info: CallbackInfo, result: RefAny) -> Update {
    let answer = answer_of(result);
    if let Some(waiter) = data.downcast_ref::<Waiter>() {
        let sender = waiter.reply.lock().ok().and_then(|mut slot| slot.take());
        if let Some(sender) = sender {
            // The thread may have given up waiting; then nobody listens.
            let _ = sender.send(answer);
        }
    }
    Update::DoNothing
}

fn method_of(method: Method) -> HttpMethod {
    match method {
        Method::Get => HttpMethod::Get,
        Method::Head => HttpMethod::Head,
        Method::Put => HttpMethod::Put,
        Method::Post => HttpMethod::Post,
        Method::Delete => HttpMethod::Delete,
    }
}

impl Transport for AzulTransport {
    fn send(&self, call: &HttpCall) -> Result<HttpReply, String> {
        let (sender, receiver) = mpsc::channel::<Answer>();
        let mut config = HttpRequestConfig::create()
            .with_timeout(self.timeout_secs)
            // Downloads come in ranged chunks (transfer::CHUNK); no cap here.
            .with_max_size(0)
            .with_user_agent(self.user_agent.as_str());
        for (name, value) in &call.headers {
            config = config.with_header(name.as_str(), value.as_str());
        }
        let waiter = Waiter {
            reply: Mutex::new(Some(sender)),
        };
        let _request = config.http_request(
            method_of(call.method),
            call.url.as_str(),
            U8Vec::from(call.body.clone()),
            call.content_type.as_str(),
            RefAny::new(waiter),
            on_answer,
        );
        receiver
            .recv_timeout(Duration::from_secs(self.timeout_secs + DELIVERY_GRACE_SECS))
            .map_err(|_| {
                String::from(
                    "the answer was not delivered (is the app's window still open, and is this \
                     running on an azul Thread?)",
                )
            })?
    }
}
