//! What the system hands the app to open: a file another app shared, "Open with" from a file manager, a link of a scheme the
//! app declared (`mailto:`, `geo:`), a text shared to it.
//!
//! The data model only. The queue and the app's handler live in `azul-layout` (`managers::open_files`), the shells raise the
//! requests (`azul-dll`, `desktop/extra/open_files`: Android's `onCreate` / `onNewIntent` intent, iOS's
//! `application:openURL:options:`), and the app learns of them in the handler it installs with
//! `CallbackInfo::set_open_handler`, which reads the request with `CallbackInfo::get_open_request`.
//!
//! # Why the API has this shape
//!
//! * **A request, not a path.** The system gives the app a `content://` URI (Android) or a security-scoped URL (iOS), neither of
//!   which `std::fs` can open. The engine therefore makes a WORKING COPY in the app's own folder (a worker thread, with limits)
//!   and hands the app its path ([`OpenedItem::path`]) next to what the system said about the file ([`OpenedItem::name`],
//!   [`OpenedItem::mime`], [`OpenedItem::size`]) and where it came from ([`OpenedItem::uri`]).
//! * **Nothing is dropped.** An item that could not be copied (too large, the provider failed, the name was refused) is still an
//!   item: it has no `path` and says why in [`OpenedItem::error`], so the app can tell the user.
//! * **Held until the app listens.** The intent that cold-starts an app arrives before the app's first callback has installed its
//!   handler: the request is kept (bounded) and delivered when the handler is set.

use azul_css::{AzString, OptionString, OptionU64};

/// The most items one request carries: a share of more is cut, and the app is told (`OpenRequest::skipped`).
pub const MAX_ITEMS: usize = 64;

/// What the sender asked the app to do with what it handed over.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum OpenAction {
    /// Show it (Android `ACTION_VIEW`, iOS "Open in" of a viewer).
    View,
    /// Edit it, and write it back where it came from if the app can (Android `ACTION_EDIT`, a file opened in place).
    Edit,
    /// It was SHARED to the app (Android `ACTION_SEND` / `ACTION_SEND_MULTIPLE`): the app imports it (a mail attachment, a picture
    /// from the gallery, a text from a browser).
    Send,
}

/// What an [`OpenedItem`] is.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum OpenedKind {
    /// A file: [`OpenedItem::path`] is its working copy.
    File,
    /// A link (`mailto:someone@example.org`, `geo:48.5,8.6`, an `https` link of a declared host): [`OpenedItem::text`] is the URL.
    Url,
    /// A text shared to the app: [`OpenedItem::text`] is the text.
    Text,
}

/// One file, link or text the app is asked to open.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct OpenedItem {
    // The fields run by decreasing alignment (every 8-byte field first, then the 4-byte kind, then the flag), so that the struct has
    // no padding between them: the C ABI lays them out in this order.
    /// A file: the name the sender's file manager shows ("Report.pdf"), safe as a file name (no separator, no control character);
    /// empty for a link and a text.
    pub name: AzString,
    /// A file: the MIME type the system or the file's name says ("application/pdf"), `application/octet-stream` when neither
    /// knows; empty for a link and a text.
    pub mime: AzString,
    /// A file: where it came from, opaque (a `content://` URI on Android, a file URL on iOS), what a write-back or a "reveal in" needs;
    /// empty when the copy is all there is (a file the app was handed inside its own folder).
    pub uri: AzString,
    /// A link: the URL. A text: the text. A file: empty.
    pub text: AzString,
    /// A file: the WORKING COPY in the app's own folder, which `std::fs` and every decoder open; `None` when there is none
    /// ([`OpenedItem::error`] says why). The copy is the app's: it may change or delete it.
    pub path: OptionString,
    /// Why this item has no `path` (too large for a copy, the provider could not be read, the name was refused); `None` when it has
    /// one, or for a link and a text.
    pub error: OptionString,
    /// A file: its size in bytes where the system said it; `None` when it did not.
    pub size: OptionU64,
    /// File, link or text.
    pub kind: OpenedKind,
    /// The sender allows the app to write the file back where it came from (`ACTION_EDIT` with a write grant, a file opened in place).
    pub writable: bool,
}

impl OpenedItem {
    /// A file with a working copy at `path`.
    #[must_use]
    pub const fn file(name: AzString, mime: AzString, size: OptionU64, path: AzString, uri: AzString, writable: bool) -> Self {
        Self {
            kind: OpenedKind::File,
            name,
            mime,
            size,
            path: OptionString::Some(path),
            uri,
            text: AzString::from_const_str(""),
            writable,
            error: OptionString::None,
        }
    }

    /// A file that has no working copy, and why.
    #[must_use]
    pub const fn file_failed(name: AzString, mime: AzString, size: OptionU64, uri: AzString, reason: AzString) -> Self {
        Self {
            kind: OpenedKind::File,
            name,
            mime,
            size,
            path: OptionString::None,
            uri,
            text: AzString::from_const_str(""),
            writable: false,
            error: OptionString::Some(reason),
        }
    }

    /// A link.
    #[must_use]
    pub const fn url(url: AzString) -> Self {
        Self {
            kind: OpenedKind::Url,
            name: AzString::from_const_str(""),
            mime: AzString::from_const_str(""),
            size: OptionU64::None,
            path: OptionString::None,
            uri: AzString::from_const_str(""),
            text: url,
            writable: false,
            error: OptionString::None,
        }
    }

    /// A text.
    #[must_use]
    pub const fn text(text: AzString) -> Self {
        Self {
            kind: OpenedKind::Text,
            name: AzString::from_const_str(""),
            mime: AzString::from_const_str(""),
            size: OptionU64::None,
            path: OptionString::None,
            uri: AzString::from_const_str(""),
            text,
            writable: false,
            error: OptionString::None,
        }
    }
}

impl_option!(
    OpenedItem,
    OptionOpenedItem,
    copy = false,
    [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

impl_vec!(
    OpenedItem,
    OpenedItemVec,
    OpenedItemVecDestructor,
    OpenedItemVecDestructorType,
    OpenedItemVecSlice,
    OptionOpenedItem
);
impl_vec_debug!(OpenedItem, OpenedItemVec);
impl_vec_clone!(OpenedItem, OpenedItemVec, OpenedItemVecDestructor);
impl_vec_partialeq!(OpenedItem, OpenedItemVec);

/// What the system asked the app to open, as `CallbackInfo::get_open_request` reports it inside the app's open handler.
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct OpenRequest {
    /// What the sender wants done with the items.
    pub action: OpenAction,
    /// The items, in the order the system gave them (at most [`MAX_ITEMS`]; `skipped` counts the ones left out).
    pub items: OpenedItemVec,
    /// This request is what STARTED the app (the app was not running): its first window is not up yet or just came up.
    pub launched_app: bool,
    /// How many items the system sent beyond the limit of [`MAX_ITEMS`] and the engine left out; `0` for nearly every request.
    pub skipped: u32,
}

impl_option!(
    OpenRequest,
    OptionOpenRequest,
    copy = false,
    [Debug, Clone, PartialEq]
);
