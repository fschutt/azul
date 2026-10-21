//! What this app tells the Azlin services it is: the contract's [`ClientHello`] in
//! `x-azlin-client`, on every request to the token server and the storage nodes, so a service
//! can tell an app too old to understand it from one that sent something wrong.
//!
//! The app says it once at start ([`announce`]), as it names its User-Agent; until then (and
//! in the kit's own tests) the hello names the kit.

use std::sync::OnceLock;

use azlin_contract::{AppName, ClientHello, SemVer, CLIENT_HEADER};
use azul_storage::HttpCall;

static ANNOUNCED: OnceLock<ClientHello> = OnceLock::new();
static KIT: OnceLock<Option<ClientHello>> = OnceLock::new();

/// Makes `hello` the one every request of this process carries; `false` when another one was
/// announced first (that one stays: one process is one app).
pub fn announce(hello: ClientHello) -> bool {
    ANNOUNCED.set(hello.clone()).is_ok() || ANNOUNCED.get() == Some(&hello)
}

/// The hello of `app` at `version` (its Cargo version: `env!("CARGO_PKG_VERSION")`), speaking
/// the kit's contract; `None` for a name or a version that cannot be one.
#[must_use]
pub fn of_app(app: &str, version: &str) -> Option<ClientHello> {
    Some(ClientHello::new(
        AppName::parse(app).ok()?,
        SemVer::parse(version).ok()?,
    ))
}

/// The hello requests carry: the announced one, else the kit's own.
#[must_use]
pub fn current() -> Option<&'static ClientHello> {
    ANNOUNCED.get().or_else(|| {
        KIT.get_or_init(|| of_app("azcloud-kit", env!("CARGO_PKG_VERSION")))
            .as_ref()
    })
}

/// `call` with the hello (any it had is replaced). The header is not signed: a node reads it
/// as a hint, never as a grant.
pub fn stamp(call: &mut HttpCall) {
    stamp_headers(&mut call.headers);
}

/// [`stamp`] on a header list before it becomes a call.
pub fn stamp_headers(headers: &mut Vec<(String, String)>) {
    headers.retain(|(name, _)| !name.eq_ignore_ascii_case(CLIENT_HEADER));
    if let Some(hello) = current() {
        headers.push((CLIENT_HEADER.to_string(), hello.header_value()));
    }
}
