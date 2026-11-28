//! iOS: `Documents` over paths: the app's own storage, and the folders the person granted, kept as BOOKMARKS.
//!
//! A folder chosen with the document picker (`FileDialog::open_folder`) is a security-scoped URL that is good until the process ends.
//! To use it again after a restart the app keeps a BOOKMARK of it (opaque bytes the system makes from the URL), in a small file of its
//! own ([`PlacesStore`], `Library/Application Support/azul/places.tsv`), and resolves the bookmarks when the places are listed: the
//! resolved URL is security-scoped again, its access is started once for the life of the process, and from then on its PATH is an
//! ordinary path (`std::fs` reads it), so every call is the desktop's backend over paths ([`LocalDocuments`]).
//!
//! A bookmark the system says is stale (the folder moved, the volume changed) is made again from the resolved URL and the store is
//! updated; a bookmark that does not resolve (the folder was deleted, its volume is gone) is left in the store and not listed: the app
//! decides ([`DocumentBackend::forget`] removes it).

use std::{
    path::PathBuf,
    ptr,
    sync::{Mutex, PoisonError},
};

use azul_core::documents::{AllFilesAccess, DocumentEntry, DocumentError, Place, PlaceKind};
use azul_css::{AzString, OptionU64};
use azul_layout::{
    documents::{DocumentBackend, LocalDocuments, ReadSeek},
    places_store::{PlacesStore, StoredPlace},
};
use objc::{
    class, msg_send,
    runtime::{Object, BOOL, NO},
    sel, sel_impl,
};

/// Where the app's own files live: the home of the account is the app's container on iOS (asked of the platform facade of
/// `azul_layout::file`: the process environment is read in one place only, `dll/src/env.rs`).
fn container() -> Option<PathBuf> {
    azul_layout::file::FilePath::get_home_dir().map(|dir| PathBuf::from(dir.inner.as_str()))
}

/// The file of the kept folders: `<container>/Library/Application Support/azul/places.tsv`.
fn store_path() -> Option<PathBuf> {
    Some(container()?.join("Library").join("Application Support").join("azul").join("places.tsv"))
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .and_then(|d| u64::try_from(d.as_millis()).ok())
        .unwrap_or(0)
}

/// `NSData` of `bytes`.
unsafe fn data_of(bytes: &[u8]) -> *mut Object {
    msg_send![class!(NSData), dataWithBytes: bytes.as_ptr().cast::<core::ffi::c_void>() length: bytes.len()]
}

/// The bytes of an `NSData`.
unsafe fn bytes_of(data: *mut Object) -> Vec<u8> {
    if data.is_null() {
        return Vec::new();
    }
    let length: usize = msg_send![data, length];
    let pointer: *const u8 = msg_send![data, bytes];
    if pointer.is_null() || length == 0 {
        return Vec::new();
    }
    core::slice::from_raw_parts(pointer, length).to_vec()
}

/// The path text of an `NSString`.
unsafe fn string_of(ns: *mut Object) -> Option<String> {
    if ns.is_null() {
        return None;
    }
    let text: *const std::os::raw::c_char = msg_send![ns, UTF8String];
    if text.is_null() {
        return None;
    }
    core::ffi::CStr::from_ptr(text).to_str().ok().map(str::to_owned)
}

/// The bookmark of a folder URL: `[url bookmarkDataWithOptions:0 includingResourceValuesForKeys:nil relativeToURL:nil error:]`.
unsafe fn bookmark_of(url: *mut Object) -> Option<Vec<u8>> {
    if url.is_null() {
        return None;
    }
    let mut error: *mut Object = ptr::null_mut();
    let data: *mut Object = msg_send![
        url,
        bookmarkDataWithOptions: 0_usize
        includingResourceValuesForKeys: ptr::null_mut::<Object>()
        relativeToURL: ptr::null_mut::<Object>()
        error: &mut error as *mut *mut Object
    ];
    let bytes = bytes_of(data);
    (!bytes.is_empty()).then_some(bytes)
}

/// A bookmark resolved: the folder's path (its access is started: it stays open for the life of the process) and whether the system
/// says the bookmark is stale (made again from the URL when it is).
struct Resolved {
    path: String,
    refreshed: Option<Vec<u8>>,
}

unsafe fn resolve(bookmark: &[u8]) -> Option<Resolved> {
    let data = data_of(bookmark);
    let mut stale: BOOL = NO;
    let mut error: *mut Object = ptr::null_mut();
    let url: *mut Object = msg_send![
        class!(NSURL),
        URLByResolvingBookmarkData: data
        options: 0_usize
        relativeToURL: ptr::null_mut::<Object>()
        bookmarkDataIsStale: &mut stale as *mut BOOL
        error: &mut error as *mut *mut Object
    ];
    if url.is_null() {
        return None;
    }
    // security-scoped again: the access is what makes the path readable
    let _started: bool = msg_send![url, startAccessingSecurityScopedResource];
    let path: *mut Object = msg_send![url, path];
    let path = string_of(path)?;
    let refreshed = if stale == NO { None } else { bookmark_of(url) };
    Some(Resolved { path, refreshed })
}

/// Keep the folder the person just granted (a security-scoped URL from the document picker, without a copy): its bookmark goes into the
/// store, so that the folder is in `places()` after a restart. A failure is logged: the folder still works until the process ends.
///
/// # Safety
///
/// `url` is a live `NSURL` (or null).
pub(crate) unsafe fn remember_folder(url: *mut Object) {
    let Some(file) = store_path() else {
        return;
    };
    let Some(bookmark) = bookmark_of(url) else {
        crate::plog_warn!("[documents] the system made no bookmark of the folder: it is usable until the app ends");
        return;
    };
    let path: *mut Object = msg_send![url, path];
    let name = string_of(path)
        .and_then(|p| std::path::Path::new(&p).file_name().map(|n| n.to_string_lossy().into_owned()))
        .unwrap_or_else(|| "Folder".to_string());
    match PlacesStore::new(file).add(StoredPlace {
        name,
        bookmark,
        granted_ms: now_ms(),
    }) {
        Ok(dropped) => {
            if !dropped.is_empty() {
                crate::plog_warn!("[documents] {} folder(s) beyond the 64 the app keeps were forgotten", dropped.len());
            }
        }
        Err(e) => crate::plog_warn!("[documents] the folder could not be kept for the next start: {e}"),
    }
}

/// One kept folder whose bookmark resolved in this run.
#[derive(Debug, Clone)]
struct Granted {
    bookmark: Vec<u8>,
    path: String,
    name: String,
    granted_ms: u64,
}

/// `Documents` of an iPhone: paths by the desktop's backend, the granted folders from the bookmarks.
#[derive(Debug)]
pub(super) struct IosDocuments {
    local: LocalDocuments,
    /// The folders resolved so far (a bookmark is resolved, and its access started, once per run).
    resolved: Mutex<Vec<Granted>>,
}

impl IosDocuments {
    pub(super) fn new() -> Self {
        Self {
            local: LocalDocuments::default(),
            resolved: Mutex::new(Vec::new()),
        }
    }

    /// The kept folders of the store that resolve, in the store's order; a new one is resolved on the way.
    fn granted(&self) -> Vec<Granted> {
        let Some(file) = store_path() else {
            return Vec::new();
        };
        let store = PlacesStore::new(file);
        let mut known = self.resolved.lock().unwrap_or_else(PoisonError::into_inner);
        let mut listed = Vec::new();
        for kept in store.load() {
            if let Some(found) = known.iter().find(|g| g.bookmark == kept.bookmark) {
                listed.push(found.clone());
                continue;
            }
            // SAFETY: `resolve` sends messages to objects it made itself.
            let Some(resolved) = (unsafe { resolve(&kept.bookmark) }) else {
                crate::plog_warn!("[documents] the folder {} no longer resolves (deleted, or its volume is gone)", kept.name);
                continue;
            };
            let mut granted = Granted {
                bookmark: kept.bookmark.clone(),
                path: resolved.path,
                name: kept.name.clone(),
                granted_ms: kept.granted_ms,
            };
            if let Some(fresh) = resolved.refreshed {
                // the system said the bookmark is stale: the store keeps the new one
                drop(store.remove(&kept.bookmark));
                drop(store.add(StoredPlace {
                    name: kept.name,
                    bookmark: fresh.clone(),
                    granted_ms: kept.granted_ms,
                }));
                granted.bookmark = fresh;
            }
            known.push(granted.clone());
            listed.push(granted);
        }
        listed
    }
}

impl DocumentBackend for IosDocuments {
    fn name(&self) -> &'static str {
        "ios-bookmarks"
    }

    fn places(&self) -> Vec<Place> {
        let mut places = Vec::new();
        if let Some(home) = container() {
            let documents = home.join("Documents");
            places.push(Place {
                uri: AzString::from(documents.to_string_lossy().into_owned()),
                name: AzString::from("On this iPhone"),
                writable: true,
                granted_ms: OptionU64::None,
                kind: PlaceKind::AppStorage,
            });
        }
        for granted in self.granted() {
            places.push(Place {
                uri: AzString::from(granted.path),
                name: AzString::from(granted.name),
                writable: true,
                granted_ms: if granted.granted_ms > 0 { OptionU64::Some(granted.granted_ms) } else { OptionU64::None },
                kind: PlaceKind::Granted,
            });
        }
        places
    }

    fn forget(&self, locator: &str) -> Result<(), DocumentError> {
        let mut known = self.resolved.lock().unwrap_or_else(PoisonError::into_inner);
        let Some(at) = known.iter().position(|g| g.path == locator) else {
            return Ok(());
        };
        let gone = known.remove(at);
        let Some(file) = store_path() else {
            return Ok(());
        };
        PlacesStore::new(file)
            .remove(&gone.bookmark)
            .map(|_| ())
            .map_err(|e| DocumentError::Failed(AzString::from(format!("{locator}: the folder could not be forgotten: {e}"))))
    }

    fn stat(&self, locator: &str) -> Result<DocumentEntry, DocumentError> {
        self.local.stat(locator)
    }

    fn children(&self, locator: &str) -> Result<Vec<DocumentEntry>, DocumentError> {
        self.local.children(locator)
    }

    fn read(&self, locator: &str, offset: u64, len: u64) -> Result<Vec<u8>, DocumentError> {
        self.local.read(locator, offset, len)
    }

    fn open_read(&self, locator: &str) -> Result<Box<dyn ReadSeek>, DocumentError> {
        self.local.open_read(locator)
    }

    fn write(&self, locator: &str, bytes: &[u8]) -> Result<DocumentEntry, DocumentError> {
        self.local.write(locator, bytes)
    }

    fn create_file(&self, parent: &str, name: &str, mime: &str) -> Result<DocumentEntry, DocumentError> {
        self.local.create_file(parent, name, mime)
    }

    fn create_dir(&self, parent: &str, name: &str) -> Result<DocumentEntry, DocumentError> {
        self.local.create_dir(parent, name)
    }

    fn rename(&self, locator: &str, new_name: &str) -> Result<DocumentEntry, DocumentError> {
        self.local.rename(locator, new_name)
    }

    fn delete(&self, locator: &str) -> Result<(), DocumentError> {
        self.local.delete(locator)
    }

    fn all_files_access(&self) -> AllFilesAccess {
        // iOS has no such grant: the person gives folders one by one
        AllFilesAccess::Unavailable
    }

    fn request_all_files_access(&self) -> bool {
        false
    }
}
