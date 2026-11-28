//! Handing a file, a text or a link to ANOTHER app: the data model of `CallbackInfo::share`.
//!
//! The same three-layer split as the notifications ([`crate::notification`]):
//!
//! * this module is the DATA the backends agree on;
//! * `azul_layout::managers::share` holds what is pure and shared (the limits, the check of a request, the
//!   names, the area the shared copies are staged in) and the slot the platform's backend is installed in;
//! * `azul-dll` (`desktop/extra/share/`) is the OS plumbing: `Intent.ACTION_SEND` through a chooser on Android
//!   (the shared files are served by a `ContentProvider` of the app), `UIActivityViewController` /
//!   `UIDocumentInteractionController` on iOS. A desktop has no system share sheet: it answers
//!   [`ShareStatus::Unsupported`] with the reason, never a silent nothing.
//!
//! # Why the API has this shape
//!
//! * **A request, one call, one status.** The call is made from the callback of the user's tap, on the thread the
//!   callbacks run on (the loop thread of Android, the main thread of iOS), where starting an activity / presenting a
//!   view controller is allowed: so the answer is the real one ([`ShareStatus::Shown`] when the system's sheet is up,
//!   or why it is not), not "queued". What the user then does in the sheet is not reported: Android does not tell,
//!   and an app has nothing to do with the answer.
//! * **Files are handed over as COPIES.** The receiver gets a `content://` URI (Android) or a file in the app's
//!   temporary folder (iOS) of a copy the engine made under the display name the app gave, so a share never exposes
//!   the app's data folder or a neighbour of the file, and the name the receiver shows is the app's.
//! * **Nothing is silent.** An empty request, too many files, a file that cannot be read and a platform without a
//!   share sheet are each a status with the reason.

use alloc::vec::Vec;

use azul_css::{AzString, StringVec};

use crate::geom::OptionLogicalRect;

/// What one thing handed to another app is.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum ShareItemKind {
    /// A file: [`ShareItem::value`] is a path this process can read.
    File,
    /// A text: [`ShareItem::value`] is the text.
    Text,
    /// A link: [`ShareItem::value`] is the URL.
    Url,
}

/// One thing handed to another app.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct ShareItem {
    /// File, text or link.
    pub kind: ShareItemKind,
    /// A file: its path. A text: the text. A link: the URL.
    pub value: AzString,
    /// A file: its MIME type (`application/pdf`); empty = taken from the extension of the name. Ignored for a text
    /// (`text/plain`) and a link.
    pub mime: AzString,
    /// A file: the name the receiver shows ("Report.pdf"); empty = the name of the file at `value`. Ignored for a
    /// text and a link.
    pub name: AzString,
}

impl ShareItem {
    /// A file at `path`, shown to the receiver under the file's own name.
    #[must_use]
    pub fn file(path: AzString) -> Self {
        Self {
            kind: ShareItemKind::File,
            value: path,
            mime: AzString::default(),
            name: AzString::default(),
        }
    }

    /// A text.
    #[must_use]
    pub fn text(text: AzString) -> Self {
        Self {
            kind: ShareItemKind::Text,
            value: text,
            mime: AzString::default(),
            name: AzString::default(),
        }
    }

    /// A link.
    #[must_use]
    pub fn url(url: AzString) -> Self {
        Self {
            kind: ShareItemKind::Url,
            value: url,
            mime: AzString::default(),
            name: AzString::default(),
        }
    }

    /// This item with the MIME type `mime` (a file's `application/pdf`).
    #[must_use]
    pub fn with_mime(mut self, mime: AzString) -> Self {
        self.mime = mime;
        self
    }

    /// This item with the name `name` (the file name the receiver shows).
    #[must_use]
    pub fn with_name(mut self, name: AzString) -> Self {
        self.name = name;
        self
    }
}

impl_option!(
    ShareItem,
    OptionShareItem,
    copy = false,
    [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

impl_vec!(
    ShareItem,
    ShareItemVec,
    ShareItemVecDestructor,
    ShareItemVecDestructorType,
    ShareItemVecSlice,
    OptionShareItem
);
impl_vec_debug!(ShareItem, ShareItemVec);
impl_vec_clone!(ShareItem, ShareItemVec, ShareItemVecDestructor);
impl_vec_partialeq!(ShareItem, ShareItemVec);

/// What an app hands to the system's share sheet (`CallbackInfo::share`).
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct ShareRequest {
    /// What is shared: at least one item; at most [`ShareRequest::MAX_ITEMS`] (more is refused with the reason).
    pub items: ShareItemVec,
    /// The title of the sheet / the subject of a mail the receiver makes of it; empty = the system's.
    pub title: AzString,
    /// Where the sheet is anchored on a screen that needs one (a tablet's popover): the rect of the button the user
    /// pressed, in window coordinates; `None` = the middle of the window. A phone's sheet ignores it.
    pub anchor: OptionLogicalRect,
    /// "Open with...": the one file is offered only to the apps that can OPEN it (a viewer / editor), not to every app
    /// that can receive a share. Needs exactly one file; anything else is refused.
    pub open_with: bool,
}

impl ShareRequest {
    /// The most items one request may carry: a sheet of more is not a sheet a person reads, and every file is a copy.
    pub const MAX_ITEMS: usize = 64;

    /// A request with no items yet.
    #[must_use]
    pub fn new() -> Self {
        Self {
            items: ShareItemVec::from_vec(Vec::new()),
            title: AzString::default(),
            anchor: OptionLogicalRect::None,
            open_with: false,
        }
    }

    /// A request for one file.
    #[must_use]
    pub fn file(path: AzString) -> Self {
        Self::new().with_item(ShareItem::file(path))
    }

    /// A request for these files (paths).
    #[must_use]
    // an owned C-ABI vector, passed by value per the azul FFI / api.json convention
    #[allow(clippy::needless_pass_by_value)]
    pub fn files(paths: StringVec) -> Self {
        let items = paths
            .as_ref()
            .iter()
            .map(|p| ShareItem::file(p.clone()))
            .collect::<Vec<_>>();
        Self {
            items: ShareItemVec::from_vec(items),
            ..Self::new()
        }
    }

    /// A request for a text.
    #[must_use]
    pub fn text(text: AzString) -> Self {
        Self::new().with_item(ShareItem::text(text))
    }

    /// A request for a link.
    #[must_use]
    pub fn url(url: AzString) -> Self {
        Self::new().with_item(ShareItem::url(url))
    }

    /// This request with one more item.
    #[must_use]
    pub fn with_item(mut self, item: ShareItem) -> Self {
        let mut items = core::mem::replace(&mut self.items, ShareItemVec::from_vec(Vec::new()))
            .into_library_owned_vec();
        items.push(item);
        self.items = ShareItemVec::from_vec(items);
        self
    }

    /// This request with the sheet's title / the mail's subject.
    #[must_use]
    pub fn with_title(mut self, title: AzString) -> Self {
        self.title = title;
        self
    }

    /// This request anchored at `rect` (window coordinates): the button the user pressed.
    #[must_use]
    pub const fn with_anchor(mut self, rect: crate::geom::LogicalRect) -> Self {
        self.anchor = OptionLogicalRect::Some(rect);
        self
    }

    /// This request as "Open with..." (see [`ShareRequest::open_with`]).
    #[must_use]
    pub const fn with_open_with(mut self, open_with: bool) -> Self {
        self.open_with = open_with;
        self
    }
}

impl Default for ShareRequest {
    fn default() -> Self {
        Self::new()
    }
}

/// What `CallbackInfo::share` answers: the sheet is up, or why it is not. Never silent.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C, u8)]
// The C API's repr(C) enum: boxing the reason would change its ABI.
#[allow(variant_size_differences)]
pub enum ShareStatus {
    /// The system's sheet (Android's chooser, iOS's activity view) is showing.
    Shown,
    /// This platform has no share sheet (a desktop): the reason. The app offers what it can instead (reveal the file, copy
    /// the link) and asks `CallbackInfo::can_share` before it shows a Share button at all.
    Unsupported(AzString),
    /// The platform has one and could not show it: no app can handle it, the file could not be copied, the system
    /// refused to start it. The reason is for the user.
    Failed(AzString),
    /// The request itself is not one: no item, too many, a file name or size over the limit, "open with" with
    /// anything but one file. Nothing was started or copied. The reason says which.
    Refused(AzString),
}
