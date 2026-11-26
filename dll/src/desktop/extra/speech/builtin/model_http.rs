//! The transport of the model store: azul's own HTTP client, in resumable windows
//! (`azul_layout::ranged_download`, the one the updater downloads with), and its SHA-256.

use std::path::Path;

use azul_layout::ranged_download::{download, sha256_hex_of_file, Error, Request};

use super::model::{FetchError, ModelFetch, ModelSource};

/// The model store's way to the network.
#[derive(Debug, Clone, Copy)]
pub struct LayoutFetch;

impl ModelFetch for LayoutFetch {
    fn download(
        &self,
        source: &ModelSource,
        partial: &Path,
        progress: &dyn Fn(u64, Option<u64>),
        cancel: &dyn Fn() -> bool,
    ) -> Result<(), FetchError> {
        download(&Request {
            url: source.url,
            headers: &[],
            partial_path: partial,
            // Not a byte more than the model: a server that sends more is refused.
            max_bytes: source.bytes,
            what: "the speech model",
            progress,
            cancel,
        })
        .map(|_| ())
        .map_err(|error| match error {
            Error::Cancelled => FetchError::Cancelled,
            Error::Refused(why) => FetchError::Refused(why),
            Error::Failed(why) => FetchError::Failed(why),
        })
    }

    fn sha256_hex(&self, path: &Path) -> Result<String, String> {
        sha256_hex_of_file(path)
    }
}
