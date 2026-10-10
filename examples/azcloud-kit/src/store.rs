//! What the sync and the shares need of a bucket: conditional GET and PUT (the index's
//! compare-and-swap), PUT / GET / HEAD of blobs, a listing (the garbage collection).
//!
//! [`RemoteStore`] is the seam between them and the network. The built-in one is
//! [`crate::bucket::Bucket`]: S3 over HTTPS through azul-storage (its SigV4, the app's
//! `Transport`), with the drive's endpoint failover - iroh to the nodes first when a build can
//! dial it ([`crate::transport::IrohLane`], what [`crate::transport::CloudDrive`] sets up); any
//! other store (a bucket in memory, a cache) implements the trait itself.

use std::{io::Read, path::Path};

use crate::error::CloudResult;

/// Blobs above this are fetched in ranges, several at once, and HEADed before an upload (a
/// resumed sync does not send them twice).
pub const BIG_BLOB: u64 = 8 * 1024 * 1024;

/// One object of a listing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoteObject {
    pub key: String,
    pub size: u64,
    /// Seconds since 1970, when the service says.
    pub modified: Option<i64>,
}

/// The answer of a conditional GET.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Conditional {
    NotFound,
    /// The object still has the ETag given.
    NotModified,
    Found {
        body: Vec<u8>,
        etag: Option<String>,
    },
}

/// A bucket as the sync uses it. Every call blocks: call it from an azul `Thread`, never from a
/// UI callback. `Sync`, as the sync moves several blobs at once on threads of its own.
pub trait RemoteStore: Send + Sync {
    /// GET, conditional on `etag` (`If-None-Match`) when given.
    fn get_unless(&self, key: &str, etag: Option<&str>) -> CloudResult<Conditional>;
    /// GET of an object of `size` bytes (in ranges when it is big); `None` when there is none.
    fn fetch(&self, key: &str, size: u64) -> CloudResult<Option<Vec<u8>>>;
    /// PUT; the ETag.
    fn put(&self, key: &str, data: &[u8]) -> CloudResult<String>;
    /// Conditional PUT (`If-Match: <etag>`, else `If-None-Match: *`); `None` when another
    /// writer won.
    fn put_if(&self, key: &str, data: &[u8], if_match: Option<&str>)
        -> CloudResult<Option<String>>;
    /// HEAD: the size; `None` when there is none.
    fn head(&self, key: &str) -> CloudResult<Option<u64>>;
    /// DELETE (a missing object is no error).
    fn delete(&self, key: &str) -> CloudResult<()>;
    /// Every object under `prefix`.
    fn list(&self, prefix: &str) -> CloudResult<Vec<RemoteObject>>;

    /// PUT of what `body` reads (`size` bytes), to its end; the ETag. A bucket streams it (a
    /// big body in parts, never whole in memory, read to its end before it is completed - a
    /// reader that fails there stops the upload); by default it is read into memory and put.
    fn put_from(&self, key: &str, body: &mut dyn Read, size: u64) -> CloudResult<String> {
        let mut data = Vec::with_capacity(usize::try_from(size).unwrap_or(0).min(BIG_BLOB as usize));
        body.read_to_end(&mut data)?;
        self.put(key, &data)
    }

    /// GET of an object of `size` bytes into the file `dest` (replaced); `false` when there is
    /// none. A bucket fetches a big one in ranges, several at once, straight into the file; by
    /// default it is fetched into memory and written.
    fn fetch_to(&self, key: &str, size: u64, dest: &Path) -> CloudResult<bool> {
        match self.fetch(key, size)? {
            Some(bytes) => {
                std::fs::write(dest, bytes)?;
                Ok(true)
            }
            None => Ok(false),
        }
    }
}
