//! A drive in memory with a bucket's rules: keys are names, a "folder" is a common prefix of a
//! listing, an empty folder is a marker object ending in `/`, listings come in pages. The
//! bridge's tests run the mail store and WebDAV on it; `azul-bridge serve --memory` serves one
//! (a demo that keeps nothing).

use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex,
    },
};

use azul_storage::{
    sigv4::sha256_hex, time::now_unix, ByteRange, Drive, DriveError, ListPage, ListRequest,
    ObjectInfo,
};

#[derive(Debug, Clone)]
struct Object {
    bytes: Vec<u8>,
    modified: u64,
}

/// A bucket in memory. Its clock is the system's unless a test sets one ([`MemoryDrive::set_now`]).
#[derive(Debug, Default)]
pub struct MemoryDrive {
    objects: Mutex<BTreeMap<String, Object>>,
    /// Seconds since 1970 of every write; 0: the system clock.
    now: AtomicU64,
}

impl MemoryDrive {
    #[must_use]
    pub fn new() -> MemoryDrive {
        MemoryDrive::default()
    }

    /// Writes from now on are dated `secs`.
    pub fn set_now(&self, secs: u64) {
        self.now.store(secs, Ordering::SeqCst);
    }

    fn now(&self) -> u64 {
        match self.now.load(Ordering::SeqCst) {
            0 => now_unix(),
            secs => secs,
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, BTreeMap<String, Object>> {
        self.objects
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Every key, in order (the tests look at the bucket).
    #[must_use]
    pub fn keys(&self) -> Vec<String> {
        self.lock().keys().cloned().collect()
    }

    fn info(key: &str, object: &Object) -> ObjectInfo {
        ObjectInfo {
            key: key.to_string(),
            size: object.bytes.len() as u64,
            modified: Some(object.modified),
            etag: Some(sha256_hex(&object.bytes)[..32].to_string()),
        }
    }

    fn check(key: &str) -> Result<(), DriveError> {
        if key.is_empty() {
            return Err(DriveError::InvalidKey {
                key: key.to_string(),
                reason: "it is empty",
            });
        }
        Ok(())
    }
}

impl Drive for MemoryDrive {
    fn list(&self, request: &ListRequest) -> Result<ListPage, DriveError> {
        let objects = self.lock();
        let page_size = request.page_size() as usize;
        let mut page = ListPage::default();
        let mut count = 0usize;
        let mut last_key: Option<String> = None;
        let start = request
            .continuation
            .clone()
            .unwrap_or_else(|| request.prefix.clone());
        for (key, object) in objects.range(start..) {
            if !key.starts_with(&request.prefix) {
                break;
            }
            if request.continuation.as_deref() == Some(key.as_str()) {
                continue;
            }
            let rest = &key[request.prefix.len()..];
            let folder = request
                .delimiter
                .as_deref()
                .filter(|d| !d.is_empty())
                .and_then(|d| {
                    rest.find(d)
                        .map(|at| format!("{}{}", request.prefix, &rest[..at + d.len()]))
                });
            if let Some(folder) = &folder {
                if page.folders.last() == Some(folder) {
                    // The same folder again: it collapses into the entry already made.
                    last_key = Some(key.clone());
                    continue;
                }
            }
            if count == page_size {
                page.next = last_key;
                return Ok(page);
            }
            match folder {
                Some(folder) => page.folders.push(folder),
                None => page.objects.push(MemoryDrive::info(key, object)),
            }
            count += 1;
            last_key = Some(key.clone());
        }
        Ok(page)
    }

    fn get(&self, key: &str) -> Result<Vec<u8>, DriveError> {
        self.lock()
            .get(key)
            .map(|o| o.bytes.clone())
            .ok_or_else(|| DriveError::NotFound {
                key: key.to_string(),
            })
    }

    fn get_range(&self, key: &str, range: ByteRange) -> Result<Vec<u8>, DriveError> {
        let bytes = self.get(key)?;
        let len = bytes.len() as u64;
        let out_of_range = || DriveError::InvalidRange {
            key: key.to_string(),
        };
        if range.start >= len {
            return Err(out_of_range());
        }
        let end = range.end.map_or(len - 1, |end| end.min(len - 1));
        if end < range.start {
            return Err(out_of_range());
        }
        Ok(bytes[range.start as usize..=end as usize].to_vec())
    }

    fn put(&self, key: &str, bytes: &[u8]) -> Result<(), DriveError> {
        MemoryDrive::check(key)?;
        let modified = self.now();
        self.lock().insert(
            key.to_string(),
            Object {
                bytes: bytes.to_vec(),
                modified,
            },
        );
        Ok(())
    }

    fn delete(&self, key: &str) -> Result<(), DriveError> {
        self.lock().remove(key);
        Ok(())
    }

    fn head(&self, key: &str) -> Result<ObjectInfo, DriveError> {
        self.lock()
            .get(key)
            .map(|o| MemoryDrive::info(key, o))
            .ok_or_else(|| DriveError::NotFound {
                key: key.to_string(),
            })
    }
}

#[cfg(test)]
mod tests {
    use azul_storage::ops;

    use super::*;

    #[test]
    fn a_listing_collapses_folders_and_pages_without_repeating_one() {
        let drive = MemoryDrive::new();
        for key in [
            "mail/Inbox/a.eml",
            "mail/Inbox/b.eml",
            "mail/Sent/c.eml",
            "mail/Work/Projects/d.eml",
            "mail/Work/e.eml",
            "mail/top.txt",
            "other/x",
        ] {
            drive.put(key, key.as_bytes()).unwrap();
        }
        let level = ops::list_folder_all(&drive, "mail/").unwrap();
        assert_eq!(level.folders, vec!["mail/Inbox/", "mail/Sent/", "mail/Work/"]);
        let names: Vec<&str> = level.objects.iter().map(|o| o.key.as_str()).collect();
        assert_eq!(names, vec!["mail/top.txt"]);
        // One entry per page: every folder once, in order.
        let mut request = ListRequest::folder("mail/").with_max_keys(1);
        let mut seen = Vec::new();
        loop {
            let page = drive.list(&request).unwrap();
            seen.extend(page.folders);
            seen.extend(page.objects.into_iter().map(|o| o.key));
            match page.next {
                Some(token) => request = request.with_continuation(token),
                None => break,
            }
        }
        assert_eq!(
            seen,
            vec!["mail/Inbox/", "mail/Sent/", "mail/Work/", "mail/top.txt"]
        );
        assert_eq!(ops::list_all(&drive, "mail/Work/").unwrap().len(), 2);
    }

    #[test]
    fn objects_are_read_whole_or_in_ranges_and_missing_ones_are_not_found() {
        let drive = MemoryDrive::new();
        drive.set_now(1_790_843_400);
        drive.put("a", b"0123456789").unwrap();
        assert_eq!(drive.get_range("a", ByteRange::new(2, Some(4))).unwrap(), b"234");
        assert_eq!(drive.get_range("a", ByteRange::new(8, None)).unwrap(), b"89");
        assert_eq!(drive.get_range("a", ByteRange::new(8, Some(99))).unwrap(), b"89");
        assert!(matches!(
            drive.get_range("a", ByteRange::new(10, None)),
            Err(DriveError::InvalidRange { .. })
        ));
        let head = drive.head("a").unwrap();
        assert_eq!((head.size, head.modified), (10, Some(1_790_843_400)));
        assert!(matches!(drive.get("b"), Err(DriveError::NotFound { .. })));
        drive.copy("a", "b").unwrap();
        assert_eq!(drive.get("b").unwrap(), b"0123456789");
        drive.delete("a").unwrap();
        drive.delete("a").unwrap();
        assert_eq!(drive.keys(), vec!["b"]);
    }
}
