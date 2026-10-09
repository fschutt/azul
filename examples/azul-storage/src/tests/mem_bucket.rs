//! A bucket in memory for the encryption tests: S3 semantics (a "folder" is a common prefix),
//! every call counted, the ranges of the ranged GETs recorded, conditional writes when it is
//! made with them. Only the basic calls and `put_if`: everything else is the trait's default.

use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicU32, Ordering},
        Mutex,
    },
};

use crate::{
    ByteRange, Drive, DriveError, ListPage, ListRequest, ObjectInfo, Precondition,
    DEFAULT_PAGE_SIZE,
};

#[derive(Default)]
pub(crate) struct MemBucket {
    objects: Mutex<BTreeMap<String, Vec<u8>>>,
    /// Whole GETs.
    gets: AtomicU32,
    /// Ranged GETs, in order.
    ranges: Mutex<Vec<(String, ByteRange)>>,
    puts: AtomicU32,
    /// Conditional PUTs, in order.
    conditional_puts: Mutex<Vec<(String, Precondition)>>,
    deletes: AtomicU32,
    /// Whether `put_if` works (a bucket with conditional writes) or is Unsupported.
    conditional: bool,
}

fn not_found(key: &str) -> DriveError {
    DriveError::NotFound {
        key: key.to_string(),
    }
}

/// The entity tag of `bytes`: their BLAKE3, shortened.
fn etag_of(bytes: &[u8]) -> String {
    crate::crypto::to_hex(&blake3::hash(bytes).as_bytes()[..8])
}

impl MemBucket {
    pub(crate) fn new() -> MemBucket {
        MemBucket::default()
    }

    /// A bucket that honours `If-None-Match: *` and `If-Match`.
    pub(crate) fn with_conditional_writes() -> MemBucket {
        MemBucket {
            conditional: true,
            ..MemBucket::default()
        }
    }

    /// Every key, sorted.
    pub(crate) fn keys(&self) -> Vec<String> {
        self.objects.lock().unwrap().keys().cloned().collect()
    }

    /// Every object, sorted by key.
    pub(crate) fn objects(&self) -> Vec<(String, Vec<u8>)> {
        self.objects
            .lock()
            .unwrap()
            .iter()
            .map(|(key, bytes)| (key.clone(), bytes.clone()))
            .collect()
    }

    pub(crate) fn object(&self, key: &str) -> Option<Vec<u8>> {
        self.objects.lock().unwrap().get(key).cloned()
    }

    /// Replaces an object behind the drive's back (a tampering server).
    pub(crate) fn set(&self, key: &str, bytes: Vec<u8>) {
        self.objects.lock().unwrap().insert(key.to_string(), bytes);
    }

    pub(crate) fn gets(&self) -> u32 {
        self.gets.load(Ordering::SeqCst)
    }

    pub(crate) fn ranges(&self) -> Vec<(String, ByteRange)> {
        self.ranges.lock().unwrap().clone()
    }

    pub(crate) fn puts(&self) -> u32 {
        self.puts.load(Ordering::SeqCst)
    }

    pub(crate) fn conditional_puts(&self) -> Vec<(String, Precondition)> {
        self.conditional_puts.lock().unwrap().clone()
    }

    pub(crate) fn deletes(&self) -> u32 {
        self.deletes.load(Ordering::SeqCst)
    }
}

impl Drive for MemBucket {
    fn list(&self, request: &ListRequest) -> Result<ListPage, DriveError> {
        let objects = self.objects.lock().unwrap();
        let mut entries: Vec<(String, Option<ObjectInfo>)> = Vec::new();
        for (key, bytes) in objects.iter() {
            if !key.starts_with(&request.prefix) {
                continue;
            }
            let rest = &key[request.prefix.len()..];
            match request
                .delimiter
                .as_deref()
                .and_then(|d| rest.find(d).map(|i| (i, d.len())))
            {
                Some((i, len)) => {
                    let folder = format!("{}{}", request.prefix, &rest[..i + len]);
                    if !entries.iter().any(|(k, _)| *k == folder) {
                        entries.push((folder, None));
                    }
                }
                None => entries.push((
                    key.clone(),
                    Some(ObjectInfo {
                        key: key.clone(),
                        size: bytes.len() as u64,
                        modified: Some(1),
                        etag: Some(etag_of(bytes)),
                    }),
                )),
            }
        }
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        if let Some(after) = request.continuation.as_deref() {
            entries.retain(|(k, _)| k.as_str() > after);
        }
        let page_size = if request.max_keys == 0 {
            DEFAULT_PAGE_SIZE as usize
        } else {
            request.max_keys as usize
        };
        let next = (entries.len() > page_size).then(|| entries[page_size - 1].0.clone());
        entries.truncate(page_size);
        let mut page = ListPage {
            next,
            ..ListPage::default()
        };
        for (key, info) in entries {
            match info {
                Some(info) => page.objects.push(info),
                None => page.folders.push(key),
            }
        }
        Ok(page)
    }

    fn get(&self, key: &str) -> Result<Vec<u8>, DriveError> {
        self.gets.fetch_add(1, Ordering::SeqCst);
        self.object(key).ok_or_else(|| not_found(key))
    }

    fn get_range(&self, key: &str, range: ByteRange) -> Result<Vec<u8>, DriveError> {
        self.ranges.lock().unwrap().push((key.to_string(), range));
        let bytes = self.object(key).ok_or_else(|| not_found(key))?;
        let start = range.start as usize;
        if start >= bytes.len() {
            return Err(DriveError::InvalidRange {
                key: key.to_string(),
            });
        }
        let end = range
            .end
            .map_or(bytes.len() - 1, |e| (e as usize).min(bytes.len() - 1));
        Ok(bytes[start..=end].to_vec())
    }

    fn put(&self, key: &str, bytes: &[u8]) -> Result<(), DriveError> {
        self.puts.fetch_add(1, Ordering::SeqCst);
        self.set(key, bytes.to_vec());
        Ok(())
    }

    fn delete(&self, key: &str) -> Result<(), DriveError> {
        self.deletes.fetch_add(1, Ordering::SeqCst);
        self.objects.lock().unwrap().remove(key);
        Ok(())
    }

    fn head(&self, key: &str) -> Result<ObjectInfo, DriveError> {
        let bytes = self.object(key).ok_or_else(|| not_found(key))?;
        Ok(ObjectInfo {
            key: key.to_string(),
            size: bytes.len() as u64,
            modified: Some(1),
            etag: Some(etag_of(&bytes)),
        })
    }

    fn put_if(
        &self,
        key: &str,
        bytes: &[u8],
        condition: &Precondition,
    ) -> Result<Option<String>, DriveError> {
        if !self.conditional {
            return Err(DriveError::Unsupported(String::from("conditional writes")));
        }
        self.conditional_puts
            .lock()
            .unwrap()
            .push((key.to_string(), condition.clone()));
        let mut objects = self.objects.lock().unwrap();
        let holds = match (condition, objects.get(key)) {
            (Precondition::Absent, current) => current.is_none(),
            (Precondition::Matches(etag), Some(current)) => etag_of(current) == *etag,
            (Precondition::Matches(_), None) => false,
        };
        if !holds {
            return Err(DriveError::Conflict {
                key: key.to_string(),
            });
        }
        objects.insert(key.to_string(), bytes.to_vec());
        Ok(Some(etag_of(bytes)))
    }
}
