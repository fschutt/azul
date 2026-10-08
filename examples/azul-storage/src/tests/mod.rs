//! Unit tests of the storage crate. No network and no libazul: the S3 tests
//! send through a fake transport, the local tests work in a temporary folder.

mod catalog;
mod config;
mod key;
mod local;
mod manifest;
mod ops;
mod s3;
mod scoped;
mod sigv4;
mod time;
mod transfer;
mod xml;

/// A fresh folder under the system's temporary folder, removed when dropped.
pub(crate) use crate::testing::TempDir;
