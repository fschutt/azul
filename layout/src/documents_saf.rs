//! What Android's storage framework says, in the engine's words: the PURE half of the phone's `Documents` backend.
//!
//! The backend itself (`azul-dll`, `desktop/extra/documents/android.rs`) talks to `ContentResolver` and `DocumentsContract`
//! through JNI. Everything that is a DECISION lives here, where a host test can reach it without a phone:
//!
//! * [`DocumentRow`] -> [`DocumentEntry`] ([`entry_from_row`]): which of a provider's flag bits mean "the app may write /
//!   delete / rename", a folder by its MIME type, a size a provider does not know, a name a provider does not give;
//! * the LOCATOR of a document: `content://<authority>/tree/<tree id>` (the root of a folder the person granted),
//!   `.../tree/<tree id>/document/<document id>` (a document inside it) and `content://<authority>/document/<document id>` (one
//!   document the person picked) are parsed and built here ([`Locator`]), with the framework's own percent-encoding
//!   ([`android_encode`]), so that the locator of a child is made by text and equals the one `DocumentsContract` would build;
//! * a provider's exception -> a [`DocumentError`] ([`classify_failure`]): a `SecurityException` is "the grant is gone"
//!   (`Denied`, ask for the folder again), a `FileNotFoundException` is "it vanished" (`NotFound`) unless its text is the
//!   `EACCES` of the descriptor (`Denied`), an `IllegalStateException` "already exists" is `Exists`, and so on.
//!
//! The values of the framework's constants (the column names, the flag bits, the folder MIME type) are READ from the framework
//! at run time by the backend (`DocumentsContract.Document.*`), never copied here: a number written down from memory is a
//! bug that only a phone finds.

use alloc::{string::String, vec::Vec};

use azul_core::documents::{DocumentEntry, DocumentError};
use azul_css::{AzString, OptionU64};

use crate::{managers::share_area::percent_decode, mime};

/// The flag bits of `DocumentsContract.Document.COLUMN_FLAGS` the engine reads, as the framework defines them
/// (`FLAG_SUPPORTS_WRITE`, `FLAG_SUPPORTS_DELETE`, `FLAG_DIR_SUPPORTS_CREATE`, `FLAG_SUPPORTS_RENAME`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FlagBits {
    /// `FLAG_SUPPORTS_WRITE`: the content of a file can be replaced.
    pub write: i32,
    /// `FLAG_SUPPORTS_DELETE`.
    pub delete: i32,
    /// `FLAG_DIR_SUPPORTS_CREATE`: entries can be created in a folder.
    pub create: i32,
    /// `FLAG_SUPPORTS_RENAME`.
    pub rename: i32,
}

/// One row of a provider's answer to a document query, in the types the cursor gives (a column that is `NULL` is `None`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DocumentRow {
    /// `COLUMN_DOCUMENT_ID`: the provider's own id of the document ("primary:Download/report.pdf").
    pub document_id: String,
    /// `COLUMN_DISPLAY_NAME`.
    pub display_name: Option<String>,
    /// `COLUMN_MIME_TYPE`.
    pub mime: Option<String>,
    /// `COLUMN_SIZE`: bytes; a provider that does not know says `NULL` or a negative number.
    pub size: Option<i64>,
    /// `COLUMN_LAST_MODIFIED`: milliseconds since the epoch; a provider that does not know says `NULL` or 0.
    pub modified_ms: Option<i64>,
    /// `COLUMN_FLAGS`.
    pub flags: i32,
}

/// The last readable piece of a document id, for a document the provider gave no name ("primary:Download/a.txt" -> "a.txt").
fn name_from_id(id: &str) -> &str {
    id.rsplit(['/', ':']).find(|piece| !piece.is_empty()).unwrap_or(id)
}

/// `text` percent-encoded the way `android.net.Uri.encode` does it: letters, digits and `_-!.~'()*` stay, every other byte of the
/// UTF-8 is `%XX` in upper case. The locators of the engine are the strings `DocumentsContract` builds, so this is that function.
#[must_use]
pub fn android_encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'!' | b'.' | b'~' | b'\'' | b'(' | b')' | b'*') {
            out.push(char::from(byte));
        } else {
            out.push_str(&alloc::format!("%{byte:02X}"));
        }
    }
    out
}

/// A content locator, taken apart (`None` from [`Locator::parse`] for anything that is not a document URI).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Locator {
    /// `content://<authority>/tree/<tree id>`: the root of a folder the person granted.
    Tree {
        /// The provider (`com.android.externalstorage.documents`).
        authority: String,
        /// The id of the folder's root document.
        tree_id: String,
    },
    /// `content://<authority>/tree/<tree id>/document/<document id>`: a file or folder inside a granted folder.
    TreeDocument {
        /// The provider.
        authority: String,
        /// The id of the granted folder's root document.
        tree_id: String,
        /// The id of the document.
        doc_id: String,
    },
    /// `content://<authority>/document/<document id>`: one document the person picked (no folder around it).
    Document {
        /// The provider.
        authority: String,
        /// The id of the document.
        doc_id: String,
    },
}

impl Locator {
    /// Take `locator` apart: ids are percent-decoded; anything with a query, a fragment, an empty piece or an encoding that is not
    /// UTF-8 is `None`.
    #[must_use]
    pub fn parse(locator: &str) -> Option<Self> {
        let rest = locator.strip_prefix("content://")?;
        if rest.contains(['?', '#']) {
            return None;
        }
        let mut pieces = rest.split('/');
        let authority = pieces.next().filter(|a| !a.is_empty())?.to_string();
        let path: Vec<&str> = pieces.collect();
        let decode = |piece: &str| percent_decode(piece).filter(|id| !id.is_empty());
        match path.as_slice() {
            ["tree", tree] => Some(Self::Tree { authority, tree_id: decode(tree)? }),
            ["tree", tree, "document", doc] => Some(Self::TreeDocument {
                authority,
                tree_id: decode(tree)?,
                doc_id: decode(doc)?,
            }),
            ["document", doc] => Some(Self::Document { authority, doc_id: decode(doc)? }),
            _ => None,
        }
    }

    /// The id of the document this locator addresses (the root's id for a [`Locator::Tree`]).
    #[must_use]
    pub fn document_id(&self) -> &str {
        match self {
            Self::Tree { tree_id, .. } => tree_id,
            Self::TreeDocument { doc_id, .. } | Self::Document { doc_id, .. } => doc_id,
        }
    }

    /// The tree URI the permission was granted on (`content://<authority>/tree/<tree id>`); `None` for a single document.
    #[must_use]
    pub fn tree_uri(&self) -> Option<String> {
        match self {
            Self::Tree { authority, tree_id } | Self::TreeDocument { authority, tree_id, .. } => {
                Some(alloc::format!("content://{authority}/tree/{}", android_encode(tree_id)))
            }
            Self::Document { .. } => None,
        }
    }

    /// The URI of the document itself, in the form the framework builds (a tree's root becomes its root document inside the tree).
    #[must_use]
    pub fn document_uri(&self) -> String {
        match self {
            Self::Tree { authority, tree_id } => alloc::format!(
                "content://{authority}/tree/{}/document/{}",
                android_encode(tree_id),
                android_encode(tree_id)
            ),
            Self::TreeDocument { authority, tree_id, doc_id } => alloc::format!(
                "content://{authority}/tree/{}/document/{}",
                android_encode(tree_id),
                android_encode(doc_id)
            ),
            Self::Document { authority, doc_id } => {
                alloc::format!("content://{authority}/document/{}", android_encode(doc_id))
            }
        }
    }

    /// The locator of the document `child_id` inside the same granted folder; `None` for a single document (it has no folder).
    #[must_use]
    pub fn child(&self, child_id: &str) -> Option<String> {
        let tree = self.tree_uri()?;
        Some(alloc::format!("{tree}/document/{}", android_encode(child_id)))
    }
}

/// What a person calls the document a locator addresses, for the failure texts ("Download/sub/a.txt" -> "a.txt"; a locator that is
/// not a document URI is shown as it is).
#[must_use]
pub fn readable_name(locator: &str) -> String {
    match Locator::parse(locator) {
        Some(parsed) => name_from_id(parsed.document_id()).to_string(),
        None => locator.to_string(),
    }
}

/// The entry the app sees for `row`, whose locator is `uri`; `dir_mime` is the framework's folder type
/// (`Document.MIME_TYPE_DIR`), `bits` its flag bits.
#[must_use]
pub fn entry_from_row(uri: String, row: &DocumentRow, dir_mime: &str, bits: FlagBits) -> DocumentEntry {
    let name = row
        .display_name
        .as_deref()
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| name_from_id(&row.document_id))
        .to_string();
    let is_dir = row.mime.as_deref() == Some(dir_mime);
    let mime = if is_dir {
        "inode/directory"
    } else {
        match row.mime.as_deref() {
            Some(known) if !known.is_empty() && known != mime::OCTET_STREAM => known,
            // a provider that says nothing, or only "some bytes": the name says more
            _ => mime::for_file_name(&name),
        }
    };
    let flags = row.flags;
    let writes = if is_dir { bits.create } else { bits.write };
    DocumentEntry {
        uri: AzString::from(uri),
        mime: AzString::from(mime.to_string()),
        name: AzString::from(name),
        is_dir,
        size: match row.size {
            Some(bytes) if !is_dir && bytes >= 0 => OptionU64::Some(bytes.unsigned_abs()),
            _ => OptionU64::None,
        },
        modified_ms: match row.modified_ms {
            Some(ms) if ms > 0 => OptionU64::Some(ms.unsigned_abs()),
            _ => OptionU64::None,
        },
        can_write: flags & writes != 0,
        can_delete: flags & bits.delete != 0,
        can_rename: flags & bits.rename != 0,
    }
}

/// The class and the message of what `Throwable.toString()` printed (`java.lang.SecurityException: Permission Denial: ...`).
fn split_throwable(text: &str) -> (&str, &str) {
    match text.split_once(": ") {
        Some((class, message)) => (class.trim(), message.trim()),
        None => (text.trim(), ""),
    }
}

/// The error of a provider's failure, from `Throwable.toString()` of the exception; `what` names the document or folder the call
/// was about (what the person sees).
#[must_use]
pub fn classify_failure(throwable: &str, what: &str) -> DocumentError {
    let (class, message) = split_throwable(throwable);
    let lower = message.to_ascii_lowercase();
    let name = AzString::from(what.to_string());
    let simple = class.rsplit(['.', '$']).next().unwrap_or(class);
    match simple {
        // the grant is gone, or never covered this document
        "SecurityException" => DocumentError::Denied(name),
        // a descriptor that cannot be opened says why in the text of the errno
        "FileNotFoundException" => {
            if lower.contains("eacces") || lower.contains("permission denied") || lower.contains("eperm") {
                DocumentError::Denied(name)
            } else if lower.contains("enospc") || lower.contains("no space") {
                DocumentError::Failed(AzString::from(alloc::format!("{what}: no space left on the device")))
            } else if lower.contains("erofs") || lower.contains("read-only") {
                DocumentError::Denied(name)
            } else {
                DocumentError::NotFound(name)
            }
        }
        // a provider that checks "is this a descendant of the tree I was granted" says it with an IllegalArgumentException
        "IllegalArgumentException" => {
            if lower.contains("descendant") || lower.contains("failed to determine if") || lower.contains("not a child") {
                DocumentError::Denied(name)
            } else if lower.contains("unknown uri") || lower.contains("invalid uri") || lower.contains("no such document") {
                DocumentError::NotFound(name)
            } else {
                DocumentError::Failed(AzString::from(alloc::format!("{what}: {simple}: {message}")))
            }
        }
        "IllegalStateException" => {
            if lower.contains("exists") {
                DocumentError::Exists(name)
            } else {
                DocumentError::Failed(AzString::from(alloc::format!("{what}: {simple}: {message}")))
            }
        }
        "UnsupportedOperationException" => DocumentError::Unsupported(name),
        "OperationCanceledException" => DocumentError::Cancelled,
        // the provider's process died or was never started
        "DeadObjectException" | "DeadSystemException" | "RemoteException" => {
            DocumentError::Failed(AzString::from(alloc::format!("{what}: the app that keeps it stopped answering")))
        }
        "IOException" | "ErrnoException" => {
            if lower.contains("enospc") || lower.contains("no space") {
                DocumentError::Failed(AzString::from(alloc::format!("{what}: no space left on the device")))
            } else if lower.contains("eacces") || lower.contains("permission denied") {
                DocumentError::Denied(name)
            } else if lower.contains("enoent") || lower.contains("no such file") {
                DocumentError::NotFound(name)
            } else {
                DocumentError::Failed(AzString::from(alloc::format!("{what}: {message}")))
            }
        }
        _ => DocumentError::Failed(AzString::from(alloc::format!("{what}: {}", throwable.trim()))),
    }
}

#[cfg(test)]
#[path = "documents_saf_test.rs"]
mod documents_saf_test;
