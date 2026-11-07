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
    limits::named, same_etag, BatchDelete, BatchMove, BatchStream, Drive, DriveError, ListPage,
    ListRequest, ObjectInfo, DEFAULT_PAGE_SIZE, PARALLEL_FILES,
};

/// Every object under `prefix`, at any depth, across all pages, in key order.
/// Folder markers (keys ending in `/`) are objects too.
pub fn list_all<D: Drive + ?Sized>(drive: &D, prefix: &str) -> Result<Vec<ObjectInfo>, DriveError> {
    list_all_paged(drive, prefix, DEFAULT_PAGE_SIZE)
}

/// [`list_all`] as the bucket has it now: a drive that answers from a copy it polls (an
/// encrypted drive's index) asks the bucket first ([`ListRequest::refresh`]). What a sync pass
/// works from: a copy up to a poll time old would miss what another device just committed.
pub fn list_all_fresh<D: Drive + ?Sized>(
    drive: &D,
    prefix: &str,
) -> Result<Vec<ObjectInfo>, DriveError> {
    list_from(drive, ListRequest::recursive(prefix).with_max_keys(DEFAULT_PAGE_SIZE).refreshed())
}

/// [`list_all`] with `page_size` keys per listing call (a test of the paging asks for small
/// pages).
pub fn list_all_paged<D: Drive + ?Sized>(
    drive: &D,
    prefix: &str,
    page_size: u32,
) -> Result<Vec<ObjectInfo>, DriveError> {
    list_from(drive, ListRequest::recursive(prefix).with_max_keys(page_size))
}

/// Every page from `request` on.
fn list_from<D: Drive + ?Sized>(
    drive: &D,
    mut request: ListRequest,
) -> Result<Vec<ObjectInfo>, DriveError> {
    let mut out = Vec::new();
    let mut pages = 0;
    loop {
        let page = drive.list(&request)?;
        pages += 1;
        out.extend(page.objects);
        match page.next {
            // The pages after the first come from the copy the first one polled.
            Some(token) => {
                request = continue_listing(request, token, pages)?;
                request.refresh = false;
            }
            None => break,
        }
    }
    Ok(out)
}

/// The request for the page after `pages` pages that named `token`, or why the listing cannot go
/// on: a token the request already carried is a listing that does not advance, and one that is
/// not over after [`named::LISTING_PAGES`] pages does not end. A bucket that answers either way
/// (a hostile or a broken one) would keep the caller reading and growing for ever
/// (X-DELTA-AZDRIVE-KEYS-22).
pub(crate) fn continue_listing(
    request: ListRequest,
    token: String,
    pages: usize,
) -> Result<ListRequest, DriveError> {
    if request.continuation.as_deref() == Some(token.as_str()) {
        return Err(DriveError::Protocol(format!(
            "the listing of \"{}\" names the page it was just asked for as its next one",
            request.prefix
        )));
    }
    if pages >= named::LISTING_PAGES {
        return Err(DriveError::Protocol(format!(
            "the listing of \"{}\" does not end after {pages} pages",
            request.prefix
        )));
    }
    Ok(request.with_continuation(token))
}

/// One folder level - the folders and the objects directly in `prefix` - across all
/// pages, in key order, as one page (`next` is `None`). The folder listings of the apps
/// (a browser's folder, a mail account's folders, the workbooks of `sheets/`).
pub fn list_folder_all<D: Drive + ?Sized>(drive: &D, prefix: &str) -> Result<ListPage, DriveError> {
    let mut out = ListPage::default();
    let mut request = ListRequest::folder(prefix);
    let mut pages = 0;
    loop {
        let page = drive.list(&request)?;
        pages += 1;
        out.folders.extend(page.folders);
        out.objects.extend(page.objects);
        match page.next {
            Some(token) => request = continue_listing(request, token, pages)?,
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
        // The version of each file that is copied: the listing's tag, else (a folder on disk lists
        // no tag) a look BEFORE its copy - never `.ok()`: a look that fails stops the rename before
        // anything is copied. The copy then checks it still holds, and the delete names it
        // (X-DELTA-AZDRIVE-FILES-24). A folder marker has none.
        let mut versions: Vec<Option<String>> = Vec::with_capacity(objects.len());
        for object in &objects {
            versions.push(match &object.etag {
                Some(tag) => Some(tag.clone()),
                None if object.key.ends_with('/') => None,
                None => drive.head(&object.key)?.etag,
            });
        }
        let moves: Vec<BatchMove<'_>> = objects
            .iter()
            .zip(&targets)
            .zip(&versions)
            .map(|((object, target), version)| {
                let one = BatchMove::new(&object.key, target);
                match version {
                    Some(tag) => one.of_version(tag),
                    None => one,
                }
            })
            .collect();
        // The target folder is empty (checked above): no target needs a look of its own. A
        // folder moves whole or not at all: the sources go only when every copy landed.
        if let Some(e) = copy_each(drive, &moves, false).into_iter().find_map(Result::err) {
            return Err(e);
        }
        // Only the versions that were copied go: a file written to since stays where it is, and
        // the move reports it (a duplicate, never a loss).
        // Deepest first: a folder marker goes after what is in it (FTP's RMD refuses a folder that
        // is not empty).
        let mut by_depth: Vec<(&ObjectInfo, Option<&str>)> = objects
            .iter()
            .zip(&versions)
            .map(|(object, version)| (object, version.as_deref()))
            .collect();
        by_depth.sort_by_key(|(object, _)| std::cmp::Reverse(object.key.len()));
        let deletes: Vec<BatchDelete<'_>> = by_depth
            .iter()
            .map(|(object, version)| delete_of(&object.key, *version))
            .collect();
        return drive
            .delete_many(&deletes)
            .into_iter()
            .find_map(Result::err)
            .map_or(Ok(()), Err);
    }
    if exists(drive, to)? {
        return Err(taken(to));
    }
    // The version that is copied is the version that goes (AZ-M12).
    let version = drive.head(from)?.etag;
    drive.copy(from, to)?;
    drive
        .delete_many(&[delete_of(from, version.as_deref())])
        .into_iter()
        .next()
        .unwrap_or(Ok(()))
}

/// The delete of `key`, of the version `etag` when the drive tells one.
fn delete_of<'a>(key: &'a str, etag: Option<&'a str>) -> BatchDelete<'a> {
    match etag {
        Some(etag) => BatchDelete::new(key).of_version(etag),
        None => BatchDelete::new(key),
    }
}

/// [`Drive::remove_empty_folder`] for a drive without one of its own: the folder is removed when
/// nothing is in it but folder markers (a bucket has no folders; a marker is an empty object that
/// stands for one) - they go, deepest first, and nothing else is deleted: a file written since the
/// listing stays, and the folder with it. `true` when the folder is gone (it was not there, or
/// only its markers were), `false` when a file is in it.
pub fn remove_empty_folder<D: Drive + ?Sized>(drive: &D, prefix: &str) -> Result<bool, DriveError> {
    check_folder(prefix)?;
    let objects = list_all(drive, prefix)?;
    if objects.iter().any(|object| !object.key.ends_with('/')) {
        return Ok(false);
    }
    let mut keys: Vec<&str> = objects.iter().map(|object| object.key.as_str()).collect();
    // A marker a listing did not show (some services list it, some do not).
    if !keys.contains(&prefix) {
        keys.push(prefix);
    }
    keys.sort_by_key(|key| std::cmp::Reverse(key.len()));
    let items: Vec<BatchDelete<'_>> = keys.iter().map(|key| BatchDelete::new(key)).collect();
    match drive.delete_many(&items).into_iter().find_map(Result::err) {
        Some(e) => Err(e),
        None => Ok(true),
    }
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
    // Deepest first: a folder marker goes after what is in it.
    keys.sort_by_key(|key| std::cmp::Reverse(key.len()));
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
    let copied = copy_each(drive, items, check_targets);
    let landed: Vec<usize> = (0..items.len()).filter(|&i| copied[i].is_ok()).collect();
    // The version that was copied is the version that goes (AZ-M12): the one the item names, else
    // the one the copy looked at.
    let versions: Vec<Option<String>> = landed
        .iter()
        .map(|&i| {
            items[i]
                .version
                .map(str::to_string)
                .or_else(|| copied[i].as_ref().ok().cloned().flatten())
        })
        .collect();
    let deletes: Vec<BatchDelete<'_>> = landed
        .iter()
        .zip(&versions)
        .map(|(&i, version)| delete_of(items[i].from, version.as_deref()))
        .collect();
    let mut results: Vec<Result<(), DriveError>> = copied
        .into_iter()
        .map(|copied| copied.map(|_| ()))
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
) -> Vec<Result<Option<String>, DriveError>> {
    let copied: Mutex<Vec<Option<Result<Option<String>, DriveError>>>> =
        Mutex::new(vec![None; items.len()]);
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
/// one) still that version, then copied - a folder marker is an empty object of its own. The
/// version of the source it looked at, when the drive tells one.
fn copy_for_move<D: Drive + ?Sized>(
    drive: &D,
    item: &BatchMove<'_>,
    check_target: bool,
) -> Result<Option<String>, DriveError> {
    if check_target && exists(drive, item.to)? {
        return Err(taken(item.to));
    }
    let mut seen = None;
    if check_target || item.version.is_some() {
        let info = drive.head(item.from)?;
        if let Some(version) = item.version {
            if !same_etag(info.etag.as_deref(), version) {
                return Err(DriveError::Conflict {
                    key: item.from.to_string(),
                });
            }
        }
        seen = info.etag;
    }
    if item.from.ends_with('/') {
        drive.put(item.to, &[])?;
    } else {
        drive.copy(item.from, item.to)?;
    }
    Ok(seen)
}
