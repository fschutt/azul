//! [`Transport`] over azul's HTTP client (`HttpRequestConfig::http_request_blocking`), for
//! the apps. Feature `azul`.
//!
//! [`AzulTransport::send`] runs the request on the calling thread and returns its answer:
//! nothing waits for the UI thread's pump, so a modal, a long layout, App Nap or a window that
//! closed at quit no longer loses an answer the server already sent (a drive token's rotation
//! among them). Call it from an azul `Thread` (or any worker thread), never from a UI callback:
//! azul refuses a blocking request on the thread that runs its event loop (a debug build
//! panics there), and this crate's mark of the UI thread ([`crate::ui_thread`]) stops it first.
//!
//! The call's limits ([`crate::CallLimits`]) become the request's: the overall timeout, the
//! times its body may take to send and its answer to arrive, and the most bytes the answer may
//! have (a bigger one is refused, never cut short).
//!
//! Every `AzulTransport` of a class sends through ONE `HttpClient` of that class: its connections
//! are pooled (a multipart upload's parts and a download's ranges reuse them), its DNS answers
//! are cached for [`DNS_CACHE_SECS`] and served stale while the name does not resolve, and the
//! fallback addresses a drive's failover hands it ([`Transport::fallback_addresses`]: a
//! node reached at its IP when its name does not resolve, TLS still verified for the name)
//! hold for every request. A name that does not resolve fails with [`DNS_FAILED`] in front.
//!
//! There are two classes ([`TrustClass`], RF39 2c, ENT27-M16): [`AzulTransport::azlin`] carries
//! the service's own traffic - the token server and the blocks and nodes of its drives, with a
//! drive token or a signature for one - on a pool that trusts the pinned authorities
//! ([`crate::trust::AZLIN_TLS_ROOTS_PEM`]) once the operator names them and follows no setting of a
//! file; [`AzulTransport::foreign`] carries everything else on a pool that follows the app's
//! roots. Each refuses the other's hosts ([`crate::trust::admits`]).

use std::{sync::OnceLock, time::Duration};

use azul::{
    error::{HttpError, ResultHttpResponseHttpError},
    http::{HttpClient, HttpClientConfig, HttpMethod, HttpRequestConfig, TrustRoots},
    str::String as AzString,
    vec::{StringVec, U8Vec},
};

use crate::{
    transport::DNS_FAILED,
    trust::{self, TrustClass},
    HttpCall, HttpReply, Method, Transport,
};

/// Seconds the pool reuses a host's DNS answer; once the name stops resolving, the engine serves
/// the last answer for seven days (serve-stale) - a drive keeps working while DNS is down.
pub const DNS_CACHE_SECS: u32 = 300;

/// Sends through azul's `HttpRequestConfig`. Call only from a worker thread.
#[derive(Debug, Clone)]
pub struct AzulTransport {
    user_agent: String,
    /// Whose traffic it carries. There is no constructor that leaves it unsaid: a call site
    /// chooses [`AzulTransport::azlin`] or [`AzulTransport::foreign`].
    class: TrustClass,
    /// This is a TEST build ([`trust::test_build`]): a stack on this computer, under any name,
    /// may stand in for the service. Read once, when the transport is made.
    test_build: bool,
}

impl AzulTransport {
    /// The Azlin service's transport: the token server and the blocks and nodes of its drives,
    /// on the pool that trusts the pinned authorities (the app's roots until the operator names
    /// them, [`trust::AZLIN_TLS_ROOTS_PEM`]). It sends to Azlin hosts only ([`trust::admits`]; a
    /// test build may reach a stack on this computer).
    #[must_use]
    pub fn azlin(user_agent: &str) -> Self {
        AzulTransport {
            user_agent: user_agent.to_string(),
            class: TrustClass::Azlin,
            test_build: trust::test_build(),
        }
    }

    /// Everybody else's transport: a data source, an OAuth provider, a feed, an address the user
    /// typed - on the pool that follows the app's roots. It never sends to an Azlin host.
    #[must_use]
    pub fn foreign(user_agent: &str) -> Self {
        AzulTransport {
            user_agent: user_agent.to_string(),
            class: TrustClass::Foreign,
            test_build: trust::test_build(),
        }
    }

    /// This transport with the build's choice as an argument (the tests of the host rule set
    /// both).
    #[cfg(test)]
    fn with_test_build(mut self, test_build: bool) -> Self {
        self.test_build = test_build;
        self
    }
}

/// The connection pool every request of the process goes through.
struct Pool(HttpClient);

// SAFETY: the handle points to the library's `Arc<HttpClientInner>`: a ureq agent and a
// mutex-guarded map of fallback addresses, both Send + Sync. Every call through the handle
// takes `&self`, and a clone is the library's clone of the `Arc`.
unsafe impl Send for Pool {}
unsafe impl Sync for Pool {}

/// The pool of `class`: the Azlin class has its own, whose TLS trusts the pinned authorities; every
/// other transport shares the foreign one, which follows the app's roots.
fn pool(class: TrustClass) -> &'static Pool {
    static FOREIGN: OnceLock<Pool> = OnceLock::new();
    static AZLIN: OnceLock<Pool> = OnceLock::new();
    match class {
        TrustClass::Azlin => AZLIN.get_or_init(|| {
            let config = HttpClientConfig::create().with_dns_cache_secs(DNS_CACHE_SECS);
            Pool(HttpClient::create(
                match pinned_roots(trust::AZLIN_TLS_ROOTS_PEM) {
                    Some(roots) => config.with_trust_roots(roots),
                    None => config,
                },
            ))
        }),
        TrustClass::Foreign => FOREIGN.get_or_init(|| {
            Pool(HttpClient::create(
                HttpClientConfig::create().with_dns_cache_secs(DNS_CACHE_SECS),
            ))
        }),
    }
}

/// Only `pems` (PEM text) as the roots of a pool; `None` for none named: the pool follows the
/// app's roots then, as a pool that pins nothing does.
fn pinned_roots(pems: &[&str]) -> Option<TrustRoots> {
    if pems.is_empty() {
        return None;
    }
    Some(TrustRoots::pinned(StringVec::from_vec(
        pems.iter().map(|pem| AzString::from(*pem)).collect(),
    )))
}

/// Why a request got no answer, as a sentence. azul's client names the URL it tried ("Connection
/// failed: {url}", "{url}: the name did not resolve"), and pCloud and WebHDFS put the password and
/// the delegation token in the query of every URL, a user may have typed `user:password@` into an
/// address: the text is scrubbed here, where it is made, before it can reach a form's red line, a
/// toast, a log or stdout (AZ-CN3).
fn http_error_text(e: &HttpError) -> String {
    let text = match e {
        HttpError::Timeout => String::from("timed out"),
        // By type, not by its words: the failover reaches the host at its addresses.
        HttpError::DnsFailed(s) => format!("{DNS_FAILED}: {}", s.as_str()),
        HttpError::InvalidUrl(s)
        | HttpError::ConnectionFailed(s)
        | HttpError::TlsError(s)
        | HttpError::IoError(s)
        | HttpError::Other(s) => s.as_str().to_string(),
        other => format!("{other:?}"),
    };
    crate::scrub::scrub_url(&text)
}

/// The answer of a blocking request.
fn answer_of(result: ResultHttpResponseHttpError) -> Result<HttpReply, String> {
    match result {
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
        ResultHttpResponseHttpError::Err(e) => Err(http_error_text(&e)),
    }
}

/// Whole seconds, at least one (azul's timeouts count seconds; 0 would mean none).
fn secs(time: Duration) -> u64 {
    time.as_secs()
        .saturating_add(u64::from(time.subsec_nanos() > 0))
        .max(1)
}

/// azul's verb for `method`: every verb the drives send, WebDAV's included (azul's `HttpMethod`
/// has OPTIONS, PROPFIND, PROPPATCH, MKCOL, COPY and MOVE since its client sends any verb, and
/// bodies for the ones that take one). The match has no wildcard: a verb added to [`Method`] does
/// not compile until it is mapped (AZ-CN5: this said "azul's HTTP client cannot send PROPFIND
/// requests yet", so Nextcloud, ownCloud and Synology could not be connected at all).
fn method_of(method: Method) -> HttpMethod {
    match method {
        Method::Get => HttpMethod::Get,
        Method::Head => HttpMethod::Head,
        Method::Put => HttpMethod::Put,
        Method::Post => HttpMethod::Post,
        Method::Delete => HttpMethod::Delete,
        Method::Patch => HttpMethod::Patch,
        Method::Options => HttpMethod::Options,
        Method::Propfind => HttpMethod::Propfind,
        Method::Proppatch => HttpMethod::Proppatch,
        Method::Mkcol => HttpMethod::Mkcol,
        Method::Copy => HttpMethod::Copy,
        Method::Move => HttpMethod::Move,
    }
}

impl Transport for AzulTransport {
    /// The pool trusts the app's set (`AppConfig::trust_roots`); the Azlin pool the pinned
    /// authorities, once they are named.
    fn trusts_public_roots(&self) -> bool {
        match (self.class, pinned_roots(trust::AZLIN_TLS_ROOTS_PEM)) {
            (TrustClass::Azlin, Some(_)) => false,
            _ => TrustRoots::app_default().is_public_roots(),
        }
    }

    /// Whose traffic it carries.
    fn trust_class(&self) -> TrustClass {
        self.class
    }

    fn send(&self, call: &HttpCall) -> Result<HttpReply, String> {
        crate::ui_thread::assert_may_wait("AzulTransport::send");
        if crate::ui_thread::is_ui_thread() {
            return Err(String::from(
                "a request was sent from the UI thread, which never waits for an answer",
            ));
        }
        // The class decides which hosts it may send to: an Azlin transport carries drive tokens and
        // signed requests to the service's own hosts only, a foreign one never to an Azlin host
        // (RF39 2c; a drive's endpoint and its nodes come from files and answers).
        trust::admits(self.class, call.url.as_str(), self.test_build)?;
        let method = method_of(call.method);
        let budget = call.budget();
        let mut config = HttpRequestConfig::create()
            .with_timeout(secs(budget.overall))
            .with_send_timeout(budget.send_body.map_or(0, secs))
            .with_recv_timeout(budget.recv_body.map_or(0, secs))
            // 0 is "no cap" to azul: an object read whole has none.
            .with_max_size(if budget.max_answer == u64::MAX { 0 } else { budget.max_answer })
            .with_user_agent(self.user_agent.as_str())
            .with_client(pool(self.class).0.clone());
        for (name, value) in &call.headers {
            config = config.with_header(name.as_str(), value.as_str());
        }
        // The Azlin service answers itself: a redirect from one of its hosts (a node, a proxy, a
        // stranger in the path) is an answer to read, never a place to follow - a signed request
        // and a drive token go where they were sent and nowhere else, as the bridge's transport
        // has it. Everybody else's servers move files and folders, and their redirects are
        // followed.
        config.follow_redirects = self.class != TrustClass::Azlin;
        answer_of(config.http_request_blocking(
            method,
            call.url.as_str(),
            U8Vec::from(call.body.clone()),
            call.content_type.as_str(),
        ))
    }

    /// The process's pool connects to `host` at `addresses` when its name does not resolve
    /// (the request and TLS still name `host`).
    fn fallback_addresses(&self, host: &str, addresses: &[String]) -> bool {
        let mut took = false;
        for address in addresses {
            took |= pool(self.class)
                .0
                .add_fallback_address(host, address.as_str());
        }
        took
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use azul::{error::HttpError, str::String as AzString};

    use azul::http::HttpMethod;

    use super::{http_error_text, method_of, pinned_roots, AzulTransport};
    use crate::{
        transport::{is_dns_failure, DNS_FAILED},
        trust::TrustClass,
        CallLimits, HttpCall, Method, Transport,
    };

    /// A GET of `url`: nothing is sent when the transport refuses it first.
    fn get(url: &str) -> HttpCall {
        HttpCall {
            method: Method::Get,
            url: url.to_string(),
            headers: Vec::new(),
            body: Vec::new(),
            content_type: String::new(),
            limits: CallLimits::json(),
        }
    }

    fn text(words: &str) -> AzString {
        AzString::from(words)
    }

    /// AZ-CN3: azul's client says "Connection failed: {url}" and "{url}: the name did not
    /// resolve", and pCloud's URL holds the password: the text this transport makes names the host
    /// and the path and nothing of the query.
    #[test]
    fn az_cn3_a_failed_request_names_no_password_of_its_url() {
        let url = "https://api.pcloud.com/listfolder?path=/&username=ann&password=hunter2";
        for error in [
            HttpError::ConnectionFailed(text(&format!("Connection failed: {url}"))),
            HttpError::IoError(text(&format!("{url}: connection reset"))),
            HttpError::TlsError(text(&format!("{url}: bad certificate"))),
            HttpError::InvalidUrl(text(url)),
            HttpError::Other(text(&format!("{url}: unexpected"))),
        ] {
            let said = http_error_text(&error);
            assert!(!said.contains("hunter2"), "{said}");
            assert!(
                said.contains("https://api.pcloud.com/listfolder?<hidden>"),
                "{said}"
            );
        }
    }

    /// The failover reads a lookup that failed by its sentence: scrubbing keeps it.
    #[test]
    fn az_cn3_a_name_that_did_not_resolve_is_still_said_so() {
        let said = http_error_text(&HttpError::DnsFailed(text(
            "https://api.pcloud.com/listfolder?password=hunter2: the name did not resolve",
        )));
        assert!(said.starts_with(DNS_FAILED), "{said}");
        assert!(is_dns_failure(&said), "{said}");
        assert!(!said.contains("hunter2"), "{said}");
        assert_eq!(http_error_text(&HttpError::Timeout), "timed out");
    }

    /// AZ-CN5 / X-AR5b-21: every verb a drive sends reaches azul's client as the same verb. WebDAV's
    /// six were refused with "cannot send PROPFIND requests yet"; the first request of every WebDAV
    /// source is a PROPFIND.
    #[test]
    fn az_cn5_every_verb_of_a_drive_is_the_verb_of_azuls_client() {
        macro_rules! maps {
            ($($ours:ident),+ $(,)?) => {{
                let mut mapped = 0;
                $(
                    assert!(
                        matches!(method_of(Method::$ours), HttpMethod::$ours),
                        "{} is not sent as itself",
                        Method::$ours.as_str()
                    );
                    mapped += 1;
                )+
                mapped
            }};
        }
        let mapped = maps!(
            Get, Head, Put, Post, Delete, Patch, Options, Propfind, Proppatch, Mkcol, Copy, Move
        );
        assert_eq!(
            mapped,
            Method::ALL.len(),
            "a verb of Method is missing from this list"
        );
    }
    /// ENT27-M16 / RF39 2c: the class a transport reports is the one it was made with - there is no
    /// constructor that leaves it unsaid.
    #[test]
    fn a_transport_reports_the_class_it_was_made_with() {
        assert_eq!(AzulTransport::azlin("ua").trust_class(), TrustClass::Azlin);
        assert_eq!(
            AzulTransport::foreign("ua").trust_class(),
            TrustClass::Foreign
        );
    }

    /// A foreign transport sends nothing to an Azlin host, whatever the build: Azlin's traffic
    /// belongs on the pool that trusts the pinned authorities. The refusal comes before any
    /// connection, so no network is needed.
    #[test]
    fn a_foreign_transport_sends_nothing_to_an_azlin_host() {
        let foreign = AzulTransport::foreign("ua");
        for url in [
            "https://token.azlin.io/v1/drives",
            "https://b1.s3.azlin.io/d-1/key",
            "https://ann@token.azlin.io/",
        ] {
            let why = foreign.send(&get(url)).unwrap_err();
            assert!(why.contains("Azlin host"), "{url}: {why}");
        }
    }

    /// The authorities the operator names are the roots of the Azlin pool and nothing else; none
    /// named (the state until the operator says which issue the token server's and the nodes'
    /// certificates): the pool follows the app's roots, exactly as a pool that pins nothing does.
    #[test]
    fn a_pool_pins_the_authorities_that_are_named_and_follows_the_app_when_none_is() {
        assert!(
            pinned_roots(&[]).is_none(),
            "nothing named: the app's roots"
        );
        let pem = include_str!("../roots/ISRGRootX1.pem");
        let roots = pinned_roots(&[pem]).expect("a named authority pins the pool");
        assert!(roots.is_pinned());
        assert!(
            !roots.is_public_roots(),
            "a pinned pool is not the public one"
        );
        assert!(pinned_roots(&[pem, pem]).is_some());
    }

    /// An Azlin transport sends nothing to a host the service does not run - not the endpoint a
    /// file names, not a node a stranger listed: the drive's signed requests and its tokens stay
    /// with the service. The refusal comes before any connection. (This crate's tests may be a
    /// test build, which reaches a stack on this computer; the shipped rule is the same
    /// transport with that flag off.)
    #[test]
    fn an_azlin_transport_sends_nothing_to_a_host_the_service_does_not_run() {
        let azlin = AzulTransport::azlin("ua").with_test_build(false);
        for url in [
            "https://evil.example/d-1/key",
            "https://azlin.io.evil.example/",
            "http://127.0.0.1:9000/d-1/key",
        ] {
            let why = azlin.send(&get(url)).unwrap_err();
            assert!(why.contains("not an Azlin host"), "{url}: {why}");
        }
        // An address that parsers read differently names no host (trust::host_of_url).
        let why = azlin
            .send(&get("https://evil.example\\@b1.s3.azlin.io/d-1/key"))
            .unwrap_err();
        assert!(why.contains("names no host"), "{why}");
    }

    /// A TEST build reaches a stack on this computer through an Azlin transport, whatever its
    /// name: the refusal above is not the answer (nothing listens at port 1, so the answer is a
    /// connection that failed).
    #[test]
    fn a_test_build_reaches_a_stack_on_this_computer_through_the_azlin_transport() {
        let why = AzulTransport::azlin("ua")
            .with_test_build(true)
            .send(&get("http://127.0.0.1:1/d-1/key"))
            .unwrap_err();
        assert!(!why.contains("not an Azlin host"), "{why}");
    }

    /// A one-connection HTTP server on this computer: it answers `reply` to the first request and
    /// hands back that request's first line.
    fn serve_once(reply: String) -> (String, std::sync::mpsc::Receiver<String>) {
        use std::io::{BufRead, BufReader, Write};

        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let Ok((mut stream, _)) = listener.accept() else {
                return;
            };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut first = String::new();
            let _ = reader.read_line(&mut first);
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                    break;
                }
            }
            let _ = stream.write_all(reply.as_bytes());
            let _ = tx.send(first);
        });
        (url, rx)
    }

    /// The reply of a server that moved the page to `to`.
    fn moved_to(to: &str) -> String {
        format!("HTTP/1.1 302 Found\r\nLocation: {to}/moved\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
    }

    /// RF39 2c: a signed request and a drive token go where they were sent. A redirect from an
    /// Azlin host is the answer (the 302 itself) and is not followed - the second server hears
    /// nothing - while everybody else's redirect is followed, as a WebDAV or a feed server that
    /// moved a page expects.
    #[test]
    fn a_redirect_is_the_answer_of_an_azlin_request_and_a_foreign_request_follows_it() {
        let ok =
            String::from("HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok");
        let (elsewhere, elsewhere_heard) = serve_once(ok);
        let (moved, _) = serve_once(moved_to(&elsewhere));
        let reply = AzulTransport::azlin("ua")
            .with_test_build(true)
            .send(&get(&format!("{moved}/x")))
            .unwrap();
        assert_eq!(reply.status, 302, "the redirect is the answer");
        assert!(
            elsewhere_heard
                .recv_timeout(Duration::from_millis(500))
                .is_err(),
            "the redirect of an Azlin host was followed to {elsewhere}"
        );

        let (moved, _) = serve_once(moved_to(&elsewhere));
        let reply = AzulTransport::foreign("ua")
            .with_test_build(true)
            .send(&get(&format!("{moved}/x")))
            .unwrap();
        assert_eq!(reply.status, 200, "a foreign request follows the redirect");
        assert_eq!(reply.body, b"ok");
        let heard = elsewhere_heard
            .recv_timeout(Duration::from_secs(5))
            .unwrap();
        assert!(heard.starts_with("GET /moved "), "{heard}");
    }
}
