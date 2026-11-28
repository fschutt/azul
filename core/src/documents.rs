//! Files and folders the USER owns, wherever the platform keeps them: the data model of `Documents`.
//!
//! A path is not enough on a phone. Android's scoped storage gives an app the folders it is GRANTED (a tree of documents named by a
//! `content://` URI, with a permission the person can take back), and iOS gives it security-scoped folders; neither is a path
//! `std::fs` can open. The desktops give paths. [`DocumentEntry::uri`] is the one word for all three: a LOCATOR, an opaque string
//! the platform made (a path, a `content://` URI), which the app stores, shows (`name`) and hands back to `Documents`.
//!
//! The behaviour is `azul_layout::documents` (the trait of a backend, the desktop's backend over `std::fs`, the handle an app takes
//! from `CallbackInfo::documents`); the phones' backends are `azul-dll` (`desktop/extra/documents`).
//!
//! # Why the API has this shape
//!
//! * **Blocking calls on a handle.** Every call answers when the platform has: a cloud provider may take seconds. The app calls from a
//!   worker thread (an azul `Thread`) with a [`Documents`](crate) handle it took from the callback and moved in: no process-wide
//!   state, and a Drive over a folder (azul-storage's `SafDrive`) holds the handle like it holds a path.
//! * **Failures say what to do.** [`DocumentError::Denied`] is "the person took the permission back or never gave it: ask again",
//!   [`DocumentError::NotFound`] is "it vanished: forget it", [`DocumentError::TimedOut`] is "the provider is slow: retry or tell
//!   the person", never one `Err(String)` that has to be read.

use alloc::string::String;

use azul_css::{AzString, EmptyStruct, OptionU64, U8Vec};

/// What the platform says of one file or folder.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
#[allow(clippy::struct_excessive_bools)] // a folder and the three things the platform lets the app do with an entry: one flag each
pub struct DocumentEntry {
    /// The locator: what `Documents` takes to read, write, list or delete this entry. Opaque; a path on a desktop, a `content://`
    /// URI on Android.
    pub uri: AzString,
    /// The name the person sees ("Report.pdf", "Downloads").
    pub name: AzString,
    /// The MIME type (`application/pdf`, `inode/directory` for a folder); `application/octet-stream` when the platform does not know.
    pub mime: AzString,
    /// A folder.
    pub is_dir: bool,
    /// Bytes of a file where the platform says; `None` for a folder and for a file whose size it does not know.
    pub size: OptionU64,
    /// When it was last changed, in milliseconds since the Unix epoch, where the platform says.
    pub modified_ms: OptionU64,
    /// The app may write the file (replace its content) or, for a folder, create entries in it.
    pub can_write: bool,
    /// The app may delete it.
    pub can_delete: bool,
    /// The app may rename it.
    pub can_rename: bool,
}

impl_option!(
    DocumentEntry,
    OptionDocumentEntry,
    copy = false,
    [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

impl_vec!(
    DocumentEntry,
    DocumentEntryVec,
    DocumentEntryVecDestructor,
    DocumentEntryVecDestructorType,
    DocumentEntryVecSlice,
    OptionDocumentEntry
);
impl_vec_debug!(DocumentEntry, DocumentEntryVec);
impl_vec_clone!(DocumentEntry, DocumentEntryVec, DocumentEntryVecDestructor);
impl_vec_partialeq!(DocumentEntry, DocumentEntryVec);

/// Why a call on a document did not do what was asked. Each says what the app does next.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C, u8)]
// The C API's repr(C) enum: boxing the reason would change its ABI.
#[allow(variant_size_differences)]
pub enum DocumentError {
    /// It is not there (any more): moved, deleted, a folder the person removed. Forget the locator.
    NotFound(AzString),
    /// The permission is not there: the person took the grant back, or never gave one for this place. Ask for the folder again.
    Denied(AzString),
    /// The provider did not answer in time (a cloud drive, a network share). Retry, or tell the person.
    TimedOut(AzString),
    /// This platform or provider cannot do this (rename inside a read-only provider, thumbnails of a folder).
    Unsupported(AzString),
    /// The name is taken: a file or folder of that name is there already.
    Exists(AzString),
    /// A file where a folder was needed.
    NotADirectory(AzString),
    /// The call was stopped by the caller; nothing was changed.
    Cancelled,
    /// Anything else the platform said, in its words.
    Failed(AzString),
}

impl DocumentError {
    /// The words for the person.
    #[must_use]
    pub fn describe(&self) -> String {
        match self {
            Self::NotFound(what) => alloc::format!("{} is not there any more", what.as_str()),
            Self::Denied(what) => alloc::format!("access to {} was removed or never given", what.as_str()),
            Self::TimedOut(what) => alloc::format!("{} did not answer in time", what.as_str()),
            Self::Unsupported(what) => alloc::format!("{} is not supported here", what.as_str()),
            Self::Exists(what) => alloc::format!("{} exists already", what.as_str()),
            Self::NotADirectory(what) => alloc::format!("{} is not a folder", what.as_str()),
            Self::Cancelled => String::from("stopped; nothing was changed"),
            Self::Failed(what) => what.as_str().into(),
        }
    }
}

impl_result!(
    DocumentEntry,
    DocumentError,
    ResultDocumentEntryDocumentError,
    copy = false,
    [Debug, Clone, PartialEq, Eq]
);

impl_result!(
    DocumentEntryVec,
    DocumentError,
    ResultDocumentEntryVecDocumentError,
    copy = false,
    [Debug, Clone, PartialEq]
);

impl_result!(
    U8Vec,
    DocumentError,
    ResultU8VecDocumentError,
    copy = false,
    [Debug, Clone, PartialEq, Eq]
);

impl_result!(
    EmptyStruct,
    DocumentError,
    ResultEmptyStructDocumentError,
    copy = false,
    [Debug, Clone, PartialEq, Eq]
);

/// A folder (or place) the app was GRANTED and may use again without asking: what AzDrive lists as "On this phone".
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct Place {
    /// The locator of the folder: pass it to `Documents::children`.
    pub uri: AzString,
    /// What the person sees ("Downloads", "Documents", "Camera").
    pub name: AzString,
    /// The app may write into it (the grant includes writing).
    pub writable: bool,
    /// When the grant was given, in milliseconds since the Unix epoch; `None` when the platform does not say.
    pub granted_ms: OptionU64,
    /// What kind of place it is.
    pub kind: PlaceKind,
}

/// Where a [`Place`] comes from.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum PlaceKind {
    /// The app's own storage (always there, needs no grant).
    AppStorage,
    /// A folder the person granted with the system's folder picker (Android: a persisted tree grant; iOS: a bookmarked folder).
    Granted,
    /// The device's shared storage as a plain folder, after the person gave the app "All files access" (Android, an app built with
    /// `all_files_access`) or on a desktop.
    SharedStorage,
}

impl_option!(
    Place,
    OptionPlace,
    copy = false,
    [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

impl_vec!(
    Place,
    PlaceVec,
    PlaceVecDestructor,
    PlaceVecDestructorType,
    PlaceVecSlice,
    OptionPlace
);
impl_vec_debug!(Place, PlaceVec);
impl_vec_clone!(Place, PlaceVec, PlaceVecDestructor);
impl_vec_partialeq!(Place, PlaceVec);

/// What the app is allowed to do with the device's shared storage as a plain folder ("All files access").
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum AllFilesAccess {
    /// The person gave it.
    Granted,
    /// The app was built to use it and the person has not given it: `request_all_files_access` opens the system's page for it.
    NotGranted,
    /// This build or platform has no such thing (it is not a build with `all_files_access`, or not Android).
    Unavailable,
}
