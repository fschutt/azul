//! azcloud-kit: the Azlin cloud client the desktop apps share (AzDrive, AzMail) and the
//! `azcloud` command line is built on.
//!
//! The token server and what it answers:
//!
//! - [`token`]: the token server's HTTP API - the storage tiers and their prices
//!   (`GET /v1/tiers`), a drive without payment on a development server (`POST /v1/drives`), a
//!   checkout and its status (`POST /v1/checkout`, `GET /v1/checkout/{id}`: the browser pays,
//!   the app polls until the drive is there), the refresh of a drive's credentials with its
//!   drive token (`POST /v1/drives/{id}/credentials`), and a drive's account calls: a member
//!   family for another device, the drive's info, the lockdown, a restore.
//! - [`bundle`]: what the token server answers for a drive - its drives-file entry, its S3
//!   credentials (temporary, 12 h, or long-lived), its drive token, its nodes.
//! - [`claim`]: the claim of a paid drive - the X25519 key a checkout names, the sign-up the
//!   token server seals to it and the app opens however late it asks.
//! - [`pending`]: the checkouts this device has no drive of yet - one keyring entry that
//!   outlives the app - polled into their drives (after "Stop waiting", at the next start).
//! - [`period`]: a paid checkout's months as blind-signed period tokens (RFC 9474) - blinded
//!   here, issued against the sealed sign-up's issue key, finalized, kept per drive until each
//!   buys the drive a month.
//! - [`session`]: what an app keeps in the OS keyring for an Azlin drive (the drive token and
//!   the current credentials, one JSON text that azul-storage also reads as plain
//!   credentials).
//! - [`shared`] and [`lock`]: the keyring every process of this user shares and the locks its
//!   entries change under (an OS file lock per entry: two windows or two apps never spend one
//!   drive token twice).
//! - [`endpoints`]: the token server of this run, from azul-appkit's shared config
//!   (`--token-url`, the environment, `~/.azlin/config.json`'s `endpoints`, the profile).
//! - [`drive`]: an Azlin drive as an azul-storage `Drive` that refreshes its credentials before
//!   they run out (and once after the service refuses them) and hands the rotated drive token
//!   back to the app for its keyring.
//!
//! A device's account, kept in a state folder (the command line's; an app's once it keeps one):
//!
//! - [`settings`]: everything a run decides before it talks to anyone, each value with where
//!   it came from (what `azcloud config` prints).
//! - [`state`] and [`secrets`]: the state folder of one device - `drives.json` in
//!   azul-storage's format, the drives' Azlin side, the secrets file (0600) under the OS
//!   keyring's entry names, the device id.
//! - [`bridge`]: the Azlin Bridge as the apps show it - its settings (`bridge.json` of its state
//!   folder), the rows a mail, file or calendar program is to be told, where its password is.
//! - [`account`]: sign-up, joining a drive from another device (each device its own token
//!   family), credentials renewed six hours before they expire, the node list re-read on every
//!   refresh, the invite, the lockdown, the restore.
//!
//! The bucket and what moves through it:
//!
//! - [`store`]: [`RemoteStore`], what the sync and the shares need of a bucket - the seam
//!   between them and the network.
//! - [`bucket`]: [`Bucket`], the built-in one - S3 through azul-storage's SigV4 over the app's
//!   transport, with the drive's endpoint failover, conditional requests, multipart uploads and
//!   ranged downloads.
//! - [`failover`]: the four failover layers of a drive's requests (the block endpoint, a node's
//!   hint, the node list, the nodes' addresses) and their retries by the class of the answer -
//!   the router of the bucket and of [`AzlinDrive`].
//! - [`transport`]: [`CloudDrive`], one bucket over the transport a run chose: iroh first,
//!   when a build that links iroh plugs in an [`IrohDialer`], HTTPS as the fallback.
//! - [`sync`]: a folder against a drive prefix - one index object guarded by compare-and-swap,
//!   content-addressed BLAKE3 blobs, a local index per folder, a three-way merge per file, the
//!   Azlin tree (the data root and `~/.azlin`) with what never leaves the computer.
//! - [`share`]: presigned links, lockdown and restore.
//! - `encryption` (feature `encryption`): the drive encrypted on this device - its keys in the
//!   keyring, the join code's key seal for a second device, the recovery code, the files
//!   through azul-storage's `EncryptedDrive` with the index an `IndexProvider` opens.
//! - [`error`]: [`CloudError`], what went wrong, telling a drive token to sign in again for
//!   from everything else.
//! - [`user_errors`]: the errors users see - one table from the token server's and the
//!   storage's codes to a class, a behaviour and an English and German text with the error ID.
//!
//! Every call blocks and goes through azul-storage's `Transport` (azul's HTTP client in the
//! apps, a fake in the tests): call it from an azul `Thread`, never from a UI callback.
//! Nothing here prints, logs or `Debug`s a secret (a drive token, a secret key, a session
//! token). No azul types: tested without a window (`cargo test -p azcloud-kit`).

pub mod account;
pub mod bridge;
pub mod bucket;
pub mod bundle;
pub mod claim;
#[cfg(feature = "encryption")]
pub mod cloudflare;
pub mod drive;
#[cfg(feature = "encryption")]
pub mod encryption;
pub mod endpoints;
pub mod error;
pub mod failover;
pub mod lock;
pub mod pending;
pub mod period;
pub mod recovery;
pub mod secrets;
pub mod session;
pub mod settings;
pub mod share;
pub mod shared;
pub mod state;
pub mod store;
pub mod sync;
pub mod token;
pub mod transport;
pub mod user_errors;

#[cfg(test)]
mod tests;

pub use account::{Account, JoinCode};
pub use bucket::Bucket;
pub use bundle::{DriveBundle, PeriodTokens};
pub use claim::{ClaimError, ClaimKey};
pub use drive::AzlinDrive;
pub use endpoints::TokenEndpoint;
pub use error::{CloudError, CloudResult};
pub use lock::LockDir;
pub use pending::{Finished, PendingCheckout, PendingTokens, Polled};
pub use period::{
    issue_tokens, look_at_drive, redeem_due, IssueRequest, Issuer, IssuerKey, Look, PeriodToken,
    PeriodTokenStore, Redeemed,
};
pub use recovery::RecoveryKey;
pub use session::AzlinSession;
pub use settings::{Flags, OsDirs, Settings};
pub use shared::SharedKeyring;
pub use state::StateDir;
pub use store::RemoteStore;
pub use token::{
    BlindSignatures, Checkout, CheckoutStatus, CheckoutVia, DriveStatus, IssueAnswer,
    OptionsQuery, RecoveryLockdown, Tier, Tiers, TokenError, TokenServer, VoucherRedeemed,
};
pub use transport::{CloudDrive, IrohDialer, Lane, TransportPref};
pub use user_errors::{Lang, UserError};

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
