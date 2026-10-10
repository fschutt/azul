//! Unit tests of the storage crate. No network and no libazul: the S3 tests
//! send through a fake transport, the local tests work in a temporary folder.

mod catalog;
mod config;
#[cfg(feature = "sql")]
mod database;
mod key;
mod keyring;
mod local;
mod manifest;
mod meta;
mod oauth;
mod ops;
mod s3;
mod scoped;
mod sigv4;
mod tables;
mod time;
mod transfer;
mod xml;

/// An S3 service in memory with multipart uploads (the multipart and transfer tests).
mod fake_bucket;
/// Parts of 16 MiB four at once, resumable uploads, conditional uploads, racing writers.
mod multipart;
/// An S3 drive's requests through a router (the endpoint failover's seam).
mod router;

/// The OpenDAL drive (feature `opendal`).
#[cfg(feature = "opendal")]
mod opendal_drive;

/// The keys of an encrypted drive (feature `encryption`).
#[cfg(feature = "encryption")]
mod crypto;
/// The AZL1 object format (feature `encryption`).
#[cfg(feature = "encryption")]
mod azl1;
/// A bucket in memory that counts and records its calls (the encryption tests).
#[cfg(feature = "encryption")]
mod mem_bucket;
/// The encrypted drive (feature `encryption`).
#[cfg(feature = "encryption")]
mod encrypted;
/// The keys on a device: setup, unlock, invites, recovery (feature `encryption`).
#[cfg(feature = "encryption")]
mod device;
/// A plaintext drive moved into its encrypted namespace (feature `encryption`).
#[cfg(feature = "encryption")]
mod migrate;
/// The recompression pass (feature `encryption`).
#[cfg(feature = "encryption")]
mod recompress;
/// The drops of incoming mail, AZD1 (feature `encryption`).
#[cfg(feature = "encryption")]
mod drops;
/// Share manifests and links (feature `encryption`).
#[cfg(feature = "encryption")]
mod sharing;
/// The key rotation and re-encryption (feature `encryption`).
#[cfg(feature = "encryption")]
mod rotation;
/// The key rotation through the drive index (feature `encryption`).
#[cfg(feature = "encryption")]
mod rotation_meta;
/// The key flows over the drive index's policy (feature `encryption`).
#[cfg(feature = "encryption")]
mod policy_keys;

/// A fresh folder under the system's temporary folder, removed when dropped.
pub(crate) use crate::testing::TempDir;
