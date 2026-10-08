//! What the sync needs of a bucket: conditional GET and PUT (the index's
//! CAS), PUT / GET / HEAD of blobs, a listing (the garbage collection). The
//! [`Drive`] is the real one; [`MemStore`] is a bucket in memory with the
//! same conditional semantics, so the merge loop is tested without a
//! cluster - including another device's commit landing between this one's
//! read and its write ([`MemStore::before_next_cas`]).

use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex,
    },
};

use anyhow::Result;

use crate::drive::{Conditional, Drive};

/// Blobs above this are fetched in parallel ranges and HEADed before an
/// upload (a resumed sync does not send them twice).
pub const BIG_BLOB: u64 = 8 * 1024 * 1024;

/// One object of a listing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoteObject {
    pub key: String,
    pub size: u64,
    /// Seconds since 1970, when the service says.
    pub modified: Option<i64>,
}

/// A bucket as the sync uses it. The futures need not be `Send`: the sync
/// runs its transfers on one task (`buffer_unordered`), never spawned.
#[allow(async_fn_in_trait)]
pub trait RemoteStore {
    /// GET, conditional on `etag` (`If-None-Match`) when given.
    async fn get_unless(&self, key: &str, etag: Option<&str>) -> Result<Conditional>;
    /// GET of an object of `size` bytes (in ranges when it is big); `None`
    /// when there is none.
    async fn fetch(&self, key: &str, size: u64) -> Result<Option<Vec<u8>>>;
    /// PUT; the ETag.
    async fn put(&self, key: &str, data: Vec<u8>) -> Result<String>;
    /// Conditional PUT (`If-Match`, else `If-None-Match: *`); `None` when
    /// another writer won.
    async fn put_if(
        &self,
        key: &str,
        data: Vec<u8>,
        if_match: Option<&str>,
    ) -> Result<Option<String>>;
    /// HEAD: the size; `None` when there is none.
    async fn head(&self, key: &str) -> Result<Option<u64>>;
    /// DELETE.
    async fn delete(&self, key: &str) -> Result<()>;
    /// Every object under `prefix`.
    async fn list(&self, prefix: &str) -> Result<Vec<RemoteObject>>;
}

impl RemoteStore for Drive {
    async fn get_unless(&self, key: &str, etag: Option<&str>) -> Result<Conditional> {
        Drive::get_unless(self, key, etag).await
    }

    async fn fetch(&self, key: &str, size: u64) -> Result<Option<Vec<u8>>> {
        if size > BIG_BLOB {
            self.get_big(key).await
        } else {
            self.get(key).await
        }
    }

    async fn put(&self, key: &str, data: Vec<u8>) -> Result<String> {
        Drive::put(self, key, data).await
    }

    async fn put_if(
        &self,
        key: &str,
        data: Vec<u8>,
        if_match: Option<&str>,
    ) -> Result<Option<String>> {
        Drive::put_if(self, key, data, if_match).await
    }

    async fn head(&self, key: &str) -> Result<Option<u64>> {
        Ok(Drive::head(self, key).await?.map(|(size, _)| size))
    }

    async fn delete(&self, key: &str) -> Result<()> {
        Drive::delete(self, key).await
    }

    async fn list(&self, prefix: &str) -> Result<Vec<RemoteObject>> {
        Ok(self
            .list_all(prefix)
            .await?
            .into_iter()
            .map(|o| RemoteObject {
                modified: azlin_proto::time::parse_iso(&o.last_modified),
                key: o.key,
                size: o.size,
            })
            .collect())
    }
}

/// A hook run once, right before the next conditional PUT: another device's
/// write landing between this one's read and its CAS.
type Race = Box<dyn FnOnce(&MemStore) + Send>;

/// A bucket in memory with S3's conditional semantics: an object's ETag
/// changes with every write; `If-Match` must name the current one,
/// `If-None-Match: *` needs the key to be free.
#[derive(Default)]
pub struct MemStore {
    objects: Mutex<BTreeMap<String, (Vec<u8>, String, i64)>>,
    next: AtomicU64,
    race: Mutex<Option<Race>>,
    /// Every request, as `METHOD key` (for the tests' counts).
    pub log: Mutex<Vec<String>>,
}

impl MemStore {
    #[must_use]
    pub fn new() -> MemStore {
        MemStore::default()
    }

    fn note(&self, method: &str, key: &str) {
        self.log
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .push(format!("{method} {key}"));
    }

    /// Writes `data` at `key` unconditionally (a test's other device); the
    /// new ETag.
    pub fn write(&self, key: &str, data: Vec<u8>) -> String {
        let etag = format!("\"v{}\"", self.next.fetch_add(1, Ordering::SeqCst) + 1);
        self.objects
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .insert(key.to_string(), (data, etag.clone(), crate::now()));
        etag
    }

    /// The object at `key`.
    #[must_use]
    pub fn read(&self, key: &str) -> Option<Vec<u8>> {
        self.objects
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get(key)
            .map(|(data, _, _)| data.clone())
    }

    /// Every key.
    #[must_use]
    pub fn keys(&self) -> Vec<String> {
        self.objects
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .keys()
            .cloned()
            .collect()
    }

    /// Runs `race` once, right before the next conditional PUT.
    pub fn before_next_cas(&self, race: impl FnOnce(&MemStore) + Send + 'static) {
        *self.race.lock().unwrap_or_else(|p| p.into_inner()) = Some(Box::new(race));
    }

    /// How many requests of `method` (`PUT`, `GET`, ...) were made.
    #[must_use]
    pub fn count(&self, method: &str) -> usize {
        let prefix = format!("{method} ");
        self.log
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .iter()
            .filter(|l| l.starts_with(&prefix))
            .count()
    }

    /// Forgets the request log.
    pub fn clear_log(&self) {
        self.log.lock().unwrap_or_else(|p| p.into_inner()).clear();
    }
}

impl RemoteStore for MemStore {
    async fn get_unless(&self, key: &str, etag: Option<&str>) -> Result<Conditional> {
        self.note("GET", key);
        let objects = self.objects.lock().unwrap_or_else(|p| p.into_inner());
        Ok(match objects.get(key) {
            None => Conditional::NotFound,
            Some((_, current, _)) if Some(current.as_str()) == etag => Conditional::NotModified,
            Some((data, current, _)) => Conditional::Found {
                body: data.clone(),
                etag: Some(current.clone()),
            },
        })
    }

    async fn fetch(&self, key: &str, _size: u64) -> Result<Option<Vec<u8>>> {
        self.note("GET", key);
        Ok(self.read(key))
    }

    async fn put(&self, key: &str, data: Vec<u8>) -> Result<String> {
        self.note("PUT", key);
        Ok(self.write(key, data))
    }

    async fn put_if(
        &self,
        key: &str,
        data: Vec<u8>,
        if_match: Option<&str>,
    ) -> Result<Option<String>> {
        let race = self.race.lock().unwrap_or_else(|p| p.into_inner()).take();
        if let Some(race) = race {
            race(self);
        }
        self.note("PUT", key);
        let current = self
            .objects
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get(key)
            .map(|(_, etag, _)| etag.clone());
        let wins = match (if_match, current.as_deref()) {
            (None, None) => true,
            (Some(want), Some(have)) => want == have,
            _ => false,
        };
        Ok(wins.then(|| self.write(key, data)))
    }

    async fn head(&self, key: &str) -> Result<Option<u64>> {
        self.note("HEAD", key);
        Ok(self.read(key).map(|d| d.len() as u64))
    }

    async fn delete(&self, key: &str) -> Result<()> {
        self.note("DELETE", key);
        self.objects
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(key);
        Ok(())
    }

    async fn list(&self, prefix: &str) -> Result<Vec<RemoteObject>> {
        self.note("LIST", prefix);
        Ok(self
            .objects
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .iter()
            .filter(|(k, _)| k.starts_with(prefix))
            .map(|(k, (data, _, at))| RemoteObject {
                key: k.clone(),
                size: data.len() as u64,
                modified: Some(*at),
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn the_memory_bucket_has_one_winner_per_etag_like_the_real_one() {
        let s = MemStore::new();
        let first = s.put_if("i", b"1".to_vec(), None).await.unwrap();
        assert!(first.is_some(), "the first create wins");
        assert!(s.put_if("i", b"x".to_vec(), None).await.unwrap().is_none());
        let etag = first.unwrap();
        let second = s.put_if("i", b"2".to_vec(), Some(&etag)).await.unwrap();
        assert!(second.is_some());
        assert!(
            s.put_if("i", b"3".to_vec(), Some(&etag))
                .await
                .unwrap()
                .is_none(),
            "a stale ETag loses"
        );
        match s.get_unless("i", second.as_deref()).await.unwrap() {
            Conditional::NotModified => {}
            other => panic!("{other:?}"),
        }
        s.before_next_cas(|s| {
            s.write("i", b"from another device".to_vec());
        });
        assert!(
            s.put_if("i", b"4".to_vec(), second.as_deref())
                .await
                .unwrap()
                .is_none(),
            "the write that landed in between wins"
        );
        assert_eq!(s.read("i").unwrap(), b"from another device");
    }
}
