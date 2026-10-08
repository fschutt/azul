//! azcloud-api: the client side of Azlin Storage that AzDrive, AzMeet and the
//! `azcloud` CLI share (azul-apps `iso/docs/AZDRIVE-INTEGRATION.md` §3). It
//! links `azlin-client` (the crate the server tests use: SigV4, failover, CAS,
//! parallel transfers, S3 over iroh) and adds what an app needs on top:
//!
//! - [`settings`]: everything a run decides before it talks to anyone - the
//!   endpoints (azul-appkit's shared config, then the environment, then the
//!   flags; [`azul_appkit::azlin_config::resolve_endpoints`]), the transport,
//!   the folders - each with where its value came from. `azcloud config`
//!   prints it, so an address nobody configured shows up as "profile local".
//! - [`state`] and [`secrets`]: the state folder of one device (never inside a
//!   synced folder): `drives.json` in azul-storage's format (what AzDrive
//!   reads), `azlin.json` (the drive's Azlin side: node list, expiry), the
//!   secrets file (0600) under the OS keyring's entry names, the device id.
//! - [`token_api`] and [`account`]: signup, joining a drive from another
//!   device (each device its own token family: a drive token rotates on every
//!   refresh and a reused one revokes the family), credentials refreshed six
//!   hours before they expire, the node list re-read on every refresh.
//! - [`drive`]: one bucket with a transport preference - iroh first when a
//!   node's iroh id is known, HTTPS as the fallback, the choice remembered
//!   for a while and iroh tried again later.
//! - [`sync`]: a folder against a drive prefix (PLAN §12.3 in its first
//!   form): one index object guarded by compare-and-swap, content-addressed
//!   BLAKE3 blobs, a local index per folder, a three-way merge per file, and
//!   the Azlin tree (the data root and `~/.azlin`) with what never leaves the
//!   computer.
//! - [`share`]: presigned links, lockdown and restore.
//!
//! Every network call is async (tokio); file I/O is plain blocking `std::fs`,
//! as the CLI runs one sync at a time.

pub mod account;
pub mod drive;
pub mod secrets;
pub mod settings;
pub mod share;
pub mod state;
pub mod sync;
pub mod token_api;

pub use account::{Account, JoinCode};
pub use drive::{Drive, Lane, TransportPref};
pub use settings::{Flags, Settings};
pub use state::StateDir;

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
