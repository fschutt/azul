//! azcloud-kit: the Azlin cloud client the desktop apps share (AzDrive's "Buy storage" first;
//! AzMail and the `azcloud` command line next).
//!
//! - [`token`]: the token server's HTTP API - the storage tiers and their prices
//!   (`GET /v1/tiers`), a drive without payment on a development server (`POST /v1/drives`), a
//!   checkout and its status (`POST /v1/checkout`, `GET /v1/checkout/{id}`: the browser pays,
//!   the app polls until the drive is there), the refresh of a drive's credentials with its
//!   drive token (`POST /v1/drives/{id}/credentials`).
//! - [`bundle`]: what the token server answers for a drive - its drives-file entry, its S3
//!   credentials (temporary, 12 h, or long-lived), its drive token.
//! - [`session`]: what an app keeps in the OS keyring for an Azlin drive (the drive token and
//!   the current credentials, one JSON text that azul-storage also reads as plain
//!   credentials).
//! - [`endpoints`]: the token server of this run, from azul-appkit's shared config
//!   (`--token-url`, the environment, `~/.azlin/config.json`'s `endpoints`, the profile).
//! - [`drive`]: an Azlin drive as an azul-storage `Drive` that refreshes its credentials before
//!   they run out (and once after the service refuses them) and hands the rotated drive token
//!   back to the app for its keyring.
//!
//! Every call blocks and goes through azul-storage's `Transport` (azul's HTTP client in the
//! apps, a fake in the tests): call it from an azul `Thread`, never from a UI callback.
//! Nothing here prints, logs or `Debug`s a secret (a drive token, a secret key, a session
//! token). No azul types: tested without a window (`cargo test -p azcloud-kit`).

pub mod bundle;
pub mod drive;
pub mod endpoints;
pub mod error;
pub mod secrets;
pub mod session;
pub mod state;
pub mod token;

#[cfg(test)]
mod tests;

pub use bundle::DriveBundle;
pub use drive::AzlinDrive;
pub use endpoints::TokenEndpoint;
pub use error::{CloudError, CloudResult};
pub use session::AzlinSession;
pub use token::{Checkout, CheckoutStatus, Tier, Tiers, TokenError, TokenServer};

/// Now, in seconds since 1970-01-01 UTC.
#[must_use]
pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Now, in nanoseconds since 1970-01-01 UTC.
#[must_use]
pub fn now_ns() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0)
}

/// `2026-10-08T09:15:00Z`: seconds since 1970 as RFC 3339 (UTC); a time before 1970 as 1970.
#[must_use]
pub fn rfc3339(unix: i64) -> String {
    azul_storage::time::iso8601(u64::try_from(unix).unwrap_or(0))
}

/// RFC 3339 (`2026-10-08T09:15:00Z`, with or without milliseconds) to seconds since 1970.
#[must_use]
pub fn parse_rfc3339(text: &str) -> Option<i64> {
    azul_storage::time::parse_iso8601(text).and_then(|t| i64::try_from(t).ok())
}
