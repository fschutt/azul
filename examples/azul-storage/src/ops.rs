//! Folder operations every drive can do with the six basic calls: list
//! everything under a prefix, ask whether a key or a folder is there, rename
//! a file or a folder by copying, delete a folder by listing it. The [`Drive`]
//! trait's default methods are these; a folder on disk does them natively.
//! Many files go in batches: copies several at once, deletes in one
//! [`Drive::delete_many`] (a bucket's DeleteObjects).

use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Mutex, PoisonError,
};

use crate::{
    same_etag, BatchDelete, BatchMove, BatchStream, Drive, DriveError, ListPage, ListRequest,
    ObjectInfo, DEFAULT_PAGE_SIZE, PARALLEL_FILES,
};

/// Every object under `prefix`, at any depth, across all pages, in key order.
/// Folder markers (keys ending in `/`) are objects too.
pub fn list_all<D: Drive + ?Sized>(drive: &D, prefix: &str) -> Result<Vec<ObjectInfo>, DriveError> {
    list_all_paged(drive, prefix, DEFAULT_PAGE_SIZE)
}

/// [`list_all`] with `page_size` keys per listing call (a test of the paging asks for small
/// pages).
pub fn list_all_paged<D: Drive + ?Sized>(
    drive: &D,
    prefix: &str,
    page_size: u32,
) -> Result<Vec<ObjectInfo>, DriveError> {
    let mut out = Vec::new();
    let mut request = ListRequest::recursive(prefix).with_max_keys(page_size);
    loop {
        let page = drive.list(&request)?;
        out.extend(page.objects);
        match page.next {
            Some(token) => request = request.with_continuation(token),
            None => break,
        }
    }
    Ok(out)
}

/// One folder level - the folders and the objects directly in `prefix` - across all
/// pages, in key order, as one page (`next` is `None`). The folder listings of the apps
/// (a browser's folder, a mail account's folders, the workbooks of `sheets/`).
pub fn list_folder_all<D: Drive + ?Sized>(drive: &D, prefix: &str) -> Result<ListPage, DriveError> {
    let mut out = ListPage::default();
    let mut request = ListRequest::folder(prefix);
    loop {
        let page = drive.list(&request)?;
        out.folders.extend(page.folders);
        out.objects.extend(page.objects);
        match page.next {
            Some(token) => request = request.with_continuation(token),
            None => break,
        }
    }
    Ok(out)
}

/// Whether the object `key` is there.
pub fn exists<D: Drive + ?Sized>(drive: &D, key: &str) -> Result<bool, DriveError> {
    match drive.head(key) {
        Ok(_) => Ok(true),
        Err(DriveError::NotFound { .. }) => Ok(false),
        Err(e) => Err(e),
    }
}

/// Whether the folder `prefix` (ending in `/`) is there: its marker, or any
/// object under it, or (on disk) the directory itself.
pub fn folder_exists<D: Drive + ?Sized>(drive: &D, prefix: &str) -> Result<bool, DriveError> {
    check_folder(prefix)?;
    if let Some(path) = drive.local_path(prefix) {
        return Ok(path.is_dir());
    }
    let page = drive.list(&ListRequest::recursive(prefix).with_max_keys(1))?;
    Ok(!page.objects.is_empty() || !page.folders.is_empty())
}

/// A folder name: not empty (the root is not a folder one can make, rename
/// or delete), ending in `/`.
pub fn check_folder(prefix: &str) -> Result<(), DriveError> {
    if prefix.is_empty() {
        return Err(DriveError::InvalidKey {
            key: String::new(),
            reason: "it is the drive's root",
        });
    }
    if !prefix.ends_with('/') {
        return Err(DriveError::InvalidKey {
            key: prefix.to_string(),
            reason: "a folder name ends with /",
        });
    }
    Ok(())
}

fn taken(key: &str) -> DriveError {
    DriveError::InvalidKey {
        key: key.to_string(),
        reason: "something has this name already",
    }
}

/// [`Drive::rename`] for a drive without a rename of its own: the object (or
/// every object under the folder `from`) copied to its new key, then deleted.
/// Refuses a target that is there, and a folder into itself.
pub fn rename_by_copy<D: Drive + ?Sized>(
    drive: &D,
    from: &str,
    to: &str,
) -> Result<(), DriveError> {
    if from.ends_with('/') || to.ends_with('/') {
        check_folder(from)?;
        check_folder(to)?;
        if to.starts_with(from) {
            return Err(DriveError::InvalidKey {
                key: to.to_string(),
                reason: "a folder cannot move into itself",
            });
        }
        let file_of_target = &to[..to.len() - 1];
        if folder_exists(drive, to)? || exists(drive, file_of_target)? {
            return Err(taken(to));
        }
        let objects = list_all(drive, from)?;
        if objects.is_empty() {
            return Err(DriveError::NotFound {
                key: from.to_string(),
            });
        }
        let targets: Vec<String> = objects
            .iter()
            .map(|object| format!("{to}{}", &object.key[from.len()..]))
            .collect();
        let moves: Vec<BatchMove<'_>> = objects
            .iter()
            .zip(&targets)
            .map(|(object, target)| BatchMove::new(&object.key, target))
            .collect();
        // The target folder is empty (checked above): no target needs a look of its own. A
        // folder moves whole or not at all: the sources go only when every copy landed.
        if let Some(e) = copy_each(drive, &moves, false).into_iter().find_map(Result::err) {
            return Err(e);
        }
        let deletes: Vec<BatchDelete<'_>> =
            objects.iter().map(|object| BatchDelete::new(&object.key)).collect();
        return drive
            .delete_many(&deletes)
            .into_iter()
            .find_map(Result::err)
            .map_or(Ok(()), Err);
    }
    if exists(drive, to)? {
        return Err(taken(to));
    }
    drive.copy(from, to)?;
    drive.delete(from)
}

/// [`Drive::delete_folder`] for a drive without one of its own: every object
/// under `prefix` (its marker too) deleted, in one [`Drive::delete_many`].
pub fn delete_by_listing<D: Drive + ?Sized>(drive: &D, prefix: &str) -> Result<(), DriveError> {
    check_folder(prefix)?;
    let mut keys: Vec<String> = list_all(drive, prefix)?
        .into_iter()
        .map(|object| object.key)
        .collect();
    // A marker a listing did not show (some services list it, some do not).
    if !keys.iter().any(|key| key == prefix) {
        keys.push(prefix.to_string());
    }
    let items: Vec<BatchDelete<'_>> = keys.iter().map(|key| BatchDelete::new(key)).collect();
    drive
        .delete_many(&items)
        .into_iter()
        .find_map(Result::err)
        .map_or(Ok(()), Err)
}

/// [`Drive::move_many`] by copying: every move checked and copied on its own, [`PARALLEL_FILES`]
/// at once (a bucket copies on the service, nothing passes through here), then every source
/// that landed deleted in one [`Drive::delete_many`]. `check_targets`: whether each target is
/// looked at first (a move never overwrites); a folder move checked its target folder already.
pub fn move_by_copy<D: Drive + ?Sized>(
    drive: &D,
    items: &[BatchMove<'_>],
    check_targets: bool,
) -> Vec<Result<(), DriveError>> {
    let mut results = copy_each(drive, items, check_targets);
    let landed: Vec<usize> = (0..items.len()).filter(|&i| results[i].is_ok()).collect();
    let deletes: Vec<BatchDelete<'_>> = landed
        .iter()
        .map(|&i| BatchDelete::new(items[i].from))
        .collect();
    for (&i, deleted) in landed.iter().zip(drive.delete_many(&deletes)) {
        if let Err(e) = deleted {
            results[i] = Err(e);
        }
    }
    results
}

/// [`Drive::put_from_many`] with [`PARALLEL_FILES`] files in flight at once, each through
/// [`Drive::put_from`] or [`Drive::put_from_if`]: one result per item, in order.
pub fn put_from_parallel<D: Drive + ?Sized>(
    drive: &D,
    items: &mut [BatchStream<'_>],
) -> Vec<Result<Option<String>, DriveError>> {
    let count = items.len();
    let queue: Mutex<Vec<(usize, &mut BatchStream<'_>)>> =
        Mutex::new(items.iter_mut().enumerate().rev().collect());
    let done: Mutex<Vec<Option<Result<Option<String>, DriveError>>>> =
        Mutex::new(vec![None; count]);
    std::thread::scope(|scope| {
        for _ in 0..PARALLEL_FILES.min(count) {
            scope.spawn(|| loop {
                let next = queue.lock().unwrap_or_else(PoisonError::into_inner).pop();
                let Some((i, item)) = next else {
                    break;
                };
                let result = match item.condition {
                    Some(condition) => drive.put_from_if(item.key, &mut *item.body, condition),
                    None => drive.put_from(item.key, &mut *item.body).map(|_| None),
                };
                done.lock().unwrap_or_else(PoisonError::into_inner)[i] = Some(result);
            });
        }
    });
    done.into_inner()
        .unwrap_or_else(PoisonError::into_inner)
        .into_iter()
        .map(|result| {
            result.unwrap_or_else(|| Err(DriveError::Protocol(String::from("not written"))))
        })
        .collect()
}

/// The copies of `items`, [`PARALLEL_FILES`] at once: one result per item, in order.
fn copy_each<D: Drive + ?Sized>(
    drive: &D,
    items: &[BatchMove<'_>],
    check_targets: bool,
) -> Vec<Result<(), DriveError>> {
    let copied: Mutex<Vec<Option<Result<(), DriveError>>>> = Mutex::new(vec![None; items.len()]);
    let next = AtomicUsize::new(0);
    std::thread::scope(|scope| {
        for _ in 0..PARALLEL_FILES.min(items.len()) {
            scope.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::SeqCst);
                let Some(item) = items.get(i) else {
                    break;
                };
                let result = copy_for_move(drive, item, check_targets);
                copied.lock().unwrap_or_else(PoisonError::into_inner)[i] = Some(result);
            });
        }
    });
    copied
        .into_inner()
        .unwrap_or_else(PoisonError::into_inner)
        .into_iter()
        .map(|result| {
            result.unwrap_or_else(|| Err(DriveError::Protocol(String::from("not copied"))))
        })
        .collect()
}

/// One move's copy: its target free (when asked), its source there and (when the move names
/// one) still that version, then copied - a folder marker is an empty object of its own.
fn copy_for_move<D: Drive + ?Sized>(
    drive: &D,
    item: &BatchMove<'_>,
    check_target: bool,
) -> Result<(), DriveError> {
    if check_target && exists(drive, item.to)? {
        return Err(taken(item.to));
    }
    if check_target || item.version.is_some() {
        let info = drive.head(item.from)?;
        if let Some(version) = item.version {
            if !same_etag(info.etag.as_deref(), version) {
                return Err(DriveError::Conflict {
                    key: item.from.to_string(),
                });
            }
        }
    }
    if item.from.ends_with('/') {
        drive.put(item.to, &[])
    } else {
        drive.copy(item.from, item.to)
    }
}
