//! Unit tests of the storage crate. No network and no libazul: the S3 tests
//! send through a fake transport, the local tests work in a temporary folder.

mod catalog;
mod config;
#[cfg(feature = "sql")]
mod database;
mod key;
mod local;
mod manifest;
mod ops;
mod s3;
mod scoped;
mod sigv4;
mod tables;
mod time;
mod transfer;
mod xml;

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

/// A fresh folder under the system's temporary folder, removed when dropped.
pub(crate) use crate::testing::TempDir;
