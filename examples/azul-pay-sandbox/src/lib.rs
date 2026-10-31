//! A local test stand-in for payment providers, so an app's purchase flow can be tested end to
//! end without a provider account: Stripe- and GoCardless-shaped APIs (Checkout Sessions and
//! PaymentIntents, Billing Requests and their flows), dLocal- and EBANX-shaped hosted pages for
//! the local methods of Latin America, small test pages for the payer (the split screen with a
//! few inputs, a 3-D Secure step, a bank confirmation, a fake phone), and webhooks signed with
//! each provider's documented scheme, posted to the app's backend.
//!
//! The backend talks to it exactly as it would to the real provider, with only the API base URL
//! changed (`https://api.stripe.com` -> `http://127.0.0.1:8784/stripe`); the app opens the
//! session's page in its payment popover; the sandbox posts the webhook. Dev switches make the
//! next card decline or ask for 3-D Secure, the next debit come back, the next payment be
//! charged back, the next webhook be dropped or sent twice, and recurring plans renew.
//!
//! It is a test tool that cannot be mistaken for, or used as, a payment page: it listens on
//! loopback only and answers loopback names only, posts webhooks only to this computer, every
//! page says TEST in its title, ribbon and watermark in a hazard palette no provider uses, only
//! the providers' documented test cards and test IBANs are taken (nothing of them is kept), no
//! page may be framed from elsewhere, and a release build refuses to start it unless the local
//! test stack opts in. See README.md.

#![forbid(unsafe_code)]
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

mod dlocal;
mod ebanx;
mod engine;
mod gocardless;
pub mod guard;
mod hosted;
pub mod kit;
mod komoju;
mod local;
mod server;
mod stripe;
mod testdata;
mod util;

pub use engine::{Core, Cx, Delivery, FakeProvider, Request, Response, Switches};
pub use server::{start, Running};
pub use util::{dlocal_signature, form_decode, form_encode, gocardless_signature, komoju_signature, stripe_signature};

/// Where the sandbox listens unless told otherwise.
pub const DEFAULT_PORT: u16 = 8784;

/// A provider the sandbox stands in for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Stripe,
    GoCardless,
    DLocal,
    Ebanx,
    Komoju,
}

impl Kind {
    pub const ALL: [Kind; 5] = [Kind::Stripe, Kind::GoCardless, Kind::DLocal, Kind::Ebanx, Kind::Komoju];

    /// The id a backend names it by: its webhooks go to the URL template's `{provider}`.
    pub fn id(self) -> &'static str {
        match self {
            Kind::Stripe => "fake-stripe",
            Kind::GoCardless => "fake-gocardless",
            Kind::DLocal => "fake-dlocal",
            Kind::Ebanx => "fake-ebanx",
            Kind::Komoju => "fake-komoju",
        }
    }

    /// Its API and pages: `<base><prefix>` replaces the provider's API base URL.
    pub fn prefix(self) -> &'static str {
        match self {
            Kind::Stripe => "/stripe",
            Kind::GoCardless => "/gocardless",
            Kind::DLocal => "/dlocal",
            Kind::Ebanx => "/ebanx",
            Kind::Komoju => "/komoju",
        }
    }

    /// Its short name (`stripe`), as `--providers` and `/_dev/state` write it.
    pub fn short(self) -> &'static str {
        match self {
            Kind::Stripe => "stripe",
            Kind::GoCardless => "gocardless",
            Kind::DLocal => "dlocal",
            Kind::Ebanx => "ebanx",
            Kind::Komoju => "komoju",
        }
    }

    /// `stripe` or `fake-stripe`, ...
    pub fn parse(name: &str) -> Option<Kind> {
        let name = name.trim().to_ascii_lowercase();
        let name = name.strip_prefix("fake-").unwrap_or(&name);
        Kind::ALL.into_iter().find(|k| k.short() == name)
    }
}

/// One provider's keys, as the backend is configured with them. The defaults are fixed,
/// documented test values: the sandbox is on this computer only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Keys {
    /// The API key requests carry as their bearer token (Stripe `sk_test_...`, GoCardless
    /// `sandbox_...`), dLocal's secret key the requests are signed with, EBANX's integration key.
    pub secret_key: String,
    /// What the webhooks are signed with (Stripe `whsec_...`, dLocal its secret key again); the
    /// token EBANX's notification URL carries.
    pub webhook_secret: String,
    /// Stripe's publishable key for the fields page; empty for the others.
    pub publishable_key: String,
    /// dLocal's `X-Login`; empty for the others.
    pub login: String,
    /// dLocal's `X-Trans-Key`; empty for the others.
    pub trans_key: String,
    /// EBANX: the JWS key a backend signs its requests with (PKCS#8, base64 DER) and its id; the
    /// notifications' public key (SPKI, base64 DER) and its fingerprint. Empty for the others.
    pub jws_key: String,
    pub jws_kid: String,
    pub notify_key: String,
    pub notify_fingerprint: String,
}

impl Keys {
    pub fn stripe_default() -> Keys {
        Keys {
            secret_key: "sk_test_sandbox".into(),
            webhook_secret: "whsec_sandbox".into(),
            publishable_key: "pk_test_sandbox".into(),
            login: String::new(),
            trans_key: String::new(),
            jws_key: String::new(),
            jws_kid: String::new(),
            notify_key: String::new(),
            notify_fingerprint: String::new(),
        }
    }

    pub fn gocardless_default() -> Keys {
        Keys {
            secret_key: "sandbox_token".into(),
            webhook_secret: "sandbox_webhook_secret".into(),
            publishable_key: String::new(),
            login: String::new(),
            trans_key: String::new(),
            jws_key: String::new(),
            jws_kid: String::new(),
            notify_key: String::new(),
            notify_fingerprint: String::new(),
        }
    }

    pub fn dlocal_default() -> Keys {
        Keys {
            secret_key: "dlocal_sandbox_secret".into(),
            webhook_secret: "dlocal_sandbox_secret".into(),
            publishable_key: String::new(),
            login: "dlocal_sandbox_login".into(),
            trans_key: "dlocal_sandbox_trans_key".into(),
            jws_key: String::new(),
            jws_kid: String::new(),
            notify_key: String::new(),
            notify_fingerprint: String::new(),
        }
    }

    pub fn komoju_default() -> Keys {
        Keys {
            secret_key: "komoju_sandbox_secret".into(),
            webhook_secret: "komoju_sandbox_webhook_token".into(),
            publishable_key: String::new(),
            login: String::new(),
            trans_key: String::new(),
            jws_key: String::new(),
            jws_kid: String::new(),
            notify_key: String::new(),
            notify_fingerprint: String::new(),
        }
    }

    pub fn ebanx_default() -> Keys {
        Keys {
            secret_key: "ebanx_sandbox_key".into(),
            webhook_secret: "ebanx_sandbox_notification_token".into(),
            publishable_key: String::new(),
            login: String::new(),
            trans_key: String::new(),
            jws_key: util::b64(util::EBANX_JWS_KEY),
            jws_kid: util::EBANX_JWS_KID.into(),
            notify_key: util::b64(util::EBANX_NOTIFY_PUB),
            notify_fingerprint: util::EBANX_NOTIFY_FINGERPRINT.into(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Config {
    /// The loopback port (0: any free one). There is no host setting: it is always 127.0.0.1.
    pub port: u16,
    /// Where webhooks go, on this computer only; `{provider}` becomes the provider's id
    /// (`http://127.0.0.1:8081/v1/webhook/{provider}`).
    pub webhook_url: String,
    /// The providers it stands in for.
    pub providers: Vec<Kind>,
    pub stripe: Keys,
    pub gocardless: Keys,
    pub dlocal: Keys,
    pub ebanx: Keys,
    pub komoju: Keys,
    /// One step of a SEPA debit's compressed timeline in seconds (business days for real).
    pub step_secs: i64,
    /// The local test stack's explicit opt-in for a release build (`--local-test-stack`, or
    /// [`guard::ALLOW_RELEASE_ENV`] set to 1): without it a release build refuses to start. It
    /// lifts that check alone; loopback only and test data only stay.
    pub local_test_stack: bool,
}

impl Default for Config {
    fn default() -> Config {
        Config {
            port: DEFAULT_PORT,
            webhook_url: "http://127.0.0.1:8081/v1/webhook/{provider}".into(),
            providers: Kind::ALL.to_vec(),
            stripe: Keys::stripe_default(),
            gocardless: Keys::gocardless_default(),
            dlocal: Keys::dlocal_default(),
            ebanx: Keys::ebanx_default(),
            komoju: Keys::komoju_default(),
            step_secs: 2,
            local_test_stack: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// A webhook URL (or another address) off this computer.
    NotLoopback(String),
    /// A release build without the local test stack's opt-in: the sandbox does not run there.
    Release,
    /// The port could not be taken.
    Bind(String),
    /// The payment methods registry did not check out (a broken build of the data).
    Registry(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::NotLoopback(why) => write!(f, "{why}"),
            Error::Release => write!(
                f,
                "the payment sandbox is a test tool and does not run in a release build \
                 (the local test stack opts in with --local-test-stack or {}=1)",
                guard::ALLOW_RELEASE_ENV
            ),
            Error::Bind(why) => write!(f, "the sandbox's port: {why}"),
            Error::Registry(why) => write!(f, "the payment methods registry: {why}"),
        }
    }
}

impl std::error::Error for Error {}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod rsa_vector_tests;
