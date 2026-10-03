//! Folder operations every drive can do with the six basic calls: list
//! everything under a prefix, ask whether a key or a folder is there, rename
//! a file or a folder by copying, delete a folder by listing it. The [`Drive`]
//! trait's default methods are these; a folder on disk does them natively.

use crate::{Drive, DriveError, ListRequest, ObjectInfo, DEFAULT_PAGE_SIZE};

/// Every object under `prefix`, at any depth, across all pages, in key order.
/// Folder markers (keys ending in `/`) are objects too.
pub fn list_all<D: Drive + ?Sized>(drive: &D, prefix: &str) -> Result<Vec<ObjectInfo>, DriveError> {
    let mut out = Vec::new();
    let mut request = ListRequest::recursive(prefix).with_max_keys(DEFAULT_PAGE_SIZE);
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
        for object in &objects {
            let rest = &object.key[from.len()..];
            let target = format!("{to}{rest}");
            if object.key.ends_with('/') {
                // A folder marker: an empty object of its own.
                drive.put(&target, &[])?;
            } else {
                drive.copy(&object.key, &target)?;
            }
        }
        for object in &objects {
            drive.delete(&object.key)?;
        }
        return Ok(());
    }
    if exists(drive, to)? {
        return Err(taken(to));
    }
    drive.copy(from, to)?;
    drive.delete(from)
}

/// [`Drive::delete_folder`] for a drive without one of its own: every object
/// under `prefix` (its marker too) deleted.
pub fn delete_by_listing<D: Drive + ?Sized>(drive: &D, prefix: &str) -> Result<(), DriveError> {
    check_folder(prefix)?;
    for object in list_all(drive, prefix)? {
        drive.delete(&object.key)?;
    }
    // A marker a listing did not show (some services list it, some do not).
    drive.delete(prefix)
}
