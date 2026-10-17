//! What a checkout opens: the `surface` of the token server's answer to `POST /v1/checkout`
//! (and to `POST /v1/checkout/{id}/surface`), checked against the chosen provider before
//! anything shows it (CHECKOUT-PLAN §3.3, §3.11):
//!
//! ```json
//! {"checkout_id": "ck_...", "pay_url": "...", "provider": "stripe", "method": "card",
//!  "surface": {"kind": "fields", "page": "https://pay.azlin.io/fields/stripe/v1",
//!              "publishable_key": "pk_live_...", "client_secret": "..."}
//!           | {"kind": "page" | "browser", "url": "https://checkout.stripe.com/c/pay/cs_..."},
//!  "return": {"success": "https://pay.azlin.io/return/ok",
//!             "cancel": "https://pay.azlin.io/return/cancel"}}
//! ```
//!
//! - `fields`: the page must be the provider's fields page on its pages origin
//!   (`/fields/<provider id>/...`); its inputs - the publishable key, the client secret, the
//!   locale, the look - go into the URL FRAGMENT, which is never sent to a server.
//! - `page` / `browser`: the URL must be one of the provider's own origins (or its fields page);
//!   anything else is refused and said, even for the system browser.
//! - the kind must be one the choice may show (PayPal is never a popover, whatever the server
//!   sends); the answer's `provider` and `method`, where given, must be the choice's.
//! - the native surfaces (the Apple Pay sheet, a native IBAN field) are the app's to build: this
//!   crate opens none of them.
//!
//! [`SecretUrl`] carries every URL that may hold a secret: `Debug` shows its origin only.

use std::fmt;

use serde_json::Value;

use crate::{
    offer::{returns_over, OfferedProvider, Returns},
    pills::Choice,
    registry::{Method, SurfaceKind, FIELDS_PREFIX},
    url::{form_encode, shown_host, WebUrl},
};

/// The longest input of a fields page (a key, a client secret) taken from the server.
pub const MAX_INPUT: usize = 512;

/// A URL that may carry a secret (a client secret in its fragment, a session id in its path).
/// `Debug` shows its origin only; [`Self::reveal`] is for the web view and `Url::open` only.
#[derive(Clone, PartialEq, Eq)]
pub struct SecretUrl(WebUrl);

impl fmt::Debug for SecretUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SecretUrl({})", self.0.origin_text())
    }
}

impl SecretUrl {
    #[must_use]
    pub fn new(url: WebUrl) -> SecretUrl {
        SecretUrl(url)
    }

    /// The URL, read.
    #[must_use]
    pub fn url(&self) -> &WebUrl {
        &self.0
    }

    /// The whole URL: for the web view's `src` and `Url::open` only - never print it.
    #[must_use]
    pub fn reveal(&self) -> String {
        self.0.to_text()
    }

    /// Its host as the chrome and the notices show it (`checkout.stripe.com`,
    /// `127.0.0.1:8081`).
    #[must_use]
    pub fn host(&self) -> String {
        self.0.shown()
    }
}

/// How the fields page should look: the app's locale and its look (`flora-light`,
/// `flat-dark`), mapped by the page onto the provider's styling options.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Look {
    pub locale: String,
    pub look: String,
}

/// A checked surface: its kind and the URL to show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Surface {
    pub kind: SurfaceKind,
    pub url: SecretUrl,
}

/// Why the server's answer opens nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SurfaceError {
    /// A part the answer must have.
    Missing(&'static str),
    /// The answer is for another provider or method than the one chosen.
    Mismatch(String),
    /// A kind of surface this app does not know.
    Kind(String),
    /// A kind the choice may not show (or this crate does not open).
    NotOffered(SurfaceKind),
    /// A URL that is not the provider's: its host.
    Url(String),
    /// An input that is empty, too long or holds white space.
    Field(&'static str),
}

impl fmt::Display for SurfaceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SurfaceError::Missing(what) => write!(f, "the answer has no {what}"),
            SurfaceError::Mismatch(what) => f.write_str(what),
            SurfaceError::Kind(kind) => write!(f, "this app cannot show a \"{kind}\" surface"),
            SurfaceError::NotOffered(kind) => {
                write!(f, "a \"{}\" surface is not offered for this payment", kind.as_str())
            }
            SurfaceError::Url(host) => write!(
                f,
                "the payment page at {host} is not one of the provider's, so it was not opened"
            ),
            SurfaceError::Field(what) => write!(f, "the answer's {what} is not usable"),
        }
    }
}

impl std::error::Error for SurfaceError {}

/// A text of at most 64 printable ASCII characters (for an error).
fn said(text: &str) -> String {
    text.chars().filter(char::is_ascii_graphic).take(64).collect()
}

/// Whether `url` is `provider`'s fields page.
fn is_fields_page(provider: &OfferedProvider, url: &WebUrl) -> bool {
    provider.fields_page.is_some()
        && provider.spec.on_pages(url)
        && url
            .path()
            .starts_with(&format!("{FIELDS_PREFIX}{}/", provider.spec.id))
}

/// The input `key` of a fields surface: present, at most [`MAX_INPUT`] bytes, printable ASCII
/// without white space.
fn input<'v>(value: &'v Value, key: &'static str) -> Result<&'v str, SurfaceError> {
    match value[key].as_str() {
        Some(text)
            if !text.is_empty()
                && text.len() <= MAX_INPUT
                && text.bytes().all(|b| b.is_ascii_graphic()) =>
        {
            Ok(text)
        }
        _ => Err(SurfaceError::Field(key)),
    }
}

impl Surface {
    /// The `surface` object `value` for `choice`, how the fields page should `look`.
    ///
    /// # Errors
    ///
    /// [`SurfaceError`]: what keeps it from being shown.
    pub fn parse(value: &Value, choice: &Choice, look: &Look) -> Result<Surface, SurfaceError> {
        let kind_text = value["kind"]
            .as_str()
            .ok_or(SurfaceError::Missing("surface kind"))?;
        let kind =
            SurfaceKind::parse(kind_text).ok_or_else(|| SurfaceError::Kind(said(kind_text)))?;
        if !choice.method.surfaces.contains(&kind) {
            return Err(SurfaceError::NotOffered(kind));
        }
        let provider = &choice.provider;
        match kind {
            SurfaceKind::PopoverFields => {
                let text = value["page"].as_str().ok_or(SurfaceError::Missing("page"))?;
                let page =
                    WebUrl::parse(text).map_err(|_| SurfaceError::Url(shown_host(text)))?;
                if !is_fields_page(provider, &page) {
                    return Err(SurfaceError::Url(page.shown()));
                }
                let key = input(value, "publishable_key")?;
                let secret = input(value, "client_secret")?;
                let fragment = format!(
                    "pk={}&cs={}&locale={}&look={}",
                    form_encode(key),
                    form_encode(secret),
                    form_encode(&look.locale),
                    form_encode(&look.look)
                );
                Ok(Surface {
                    kind,
                    url: SecretUrl(page.with_fragment(&fragment)),
                })
            }
            SurfaceKind::WebviewPage | SurfaceKind::SystemBrowser => {
                let text = value["url"].as_str().ok_or(SurfaceError::Missing("url"))?;
                let url = WebUrl::parse(text).map_err(|_| SurfaceError::Url(shown_host(text)))?;
                if !provider.allows(&url) && !is_fields_page(provider, &url) {
                    return Err(SurfaceError::Url(url.shown()));
                }
                Ok(Surface {
                    kind,
                    url: SecretUrl(url),
                })
            }
            other => Err(SurfaceError::NotOffered(other)),
        }
    }
}

/// A checkout the token server made, checked: its id, what it opens, its return pages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Created {
    pub checkout_id: String,
    pub surface: Surface,
    /// The provider's return pages, the answer's where they are on the pages origin.
    pub returns: Returns,
}

impl Created {
    /// The answer of `POST /v1/checkout` for `choice`.
    ///
    /// # Errors
    ///
    /// [`SurfaceError`]: no checkout id, another provider or method, a surface that may not be
    /// shown.
    pub fn parse(answer: &Value, choice: &Choice, look: &Look) -> Result<Created, SurfaceError> {
        let id = answer["checkout_id"].as_str().unwrap_or_default().trim();
        if id.is_empty() {
            return Err(SurfaceError::Missing("checkout_id"));
        }
        if id.len() > 128
            || !id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
        {
            return Err(SurfaceError::Field("checkout_id"));
        }
        if let Some(provider) = answer["provider"].as_str() {
            if provider != choice.provider.spec.id {
                return Err(SurfaceError::Mismatch(format!(
                    "the checkout is for {}, not {}",
                    said(provider),
                    choice.provider.spec.id
                )));
            }
        }
        if let Some(method) = answer["method"].as_str() {
            if Method::parse(method) != Some(choice.method.method) {
                return Err(SurfaceError::Mismatch(format!(
                    "the checkout is paid by {}, not {}",
                    said(method),
                    choice.method.method.as_str()
                )));
            }
        }
        let surface = answer
            .get("surface")
            .filter(|v| v.is_object())
            .ok_or(SurfaceError::Missing("surface"))?;
        let surface = Surface::parse(surface, choice, look)?;
        let returns = returns_over(
            choice.provider.returns.clone(),
            choice.provider.spec,
            &answer["return"],
            &mut Vec::new(),
        );
        Ok(Created {
            checkout_id: id.to_string(),
            surface,
            returns,
        })
    }
}
