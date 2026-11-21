//! What a link shares: the `kind` option of planning/architecture.md section 5.12 ("`file`,
//! `selection` (hand-picked files), `folder_snapshot`, `folder_live` (follows edits), `version` (one
//! pinned version)"; strength key: "the manifest holds keys only for what is shared, and no other
//! file's name").
//!
//! The words are the ones of the table and of `azlin-contract`'s `LinkKind`; `FromStr` accepts
//! exactly them. Which rules follow from the kind (subfolders only for the folder kinds, a live folder
//! follows the latest version, every other kind names at least one object) are asked of the kind
//! here, so that `validate` and the record read ONE definition.
//!
//! Implements planning/architecture.md section 5.12 (`kind`, `subfolders`, "Live folders").

/// `kind`: what the link shares.
///
/// Implements planning/architecture.md section 5.12 (`kind`: `file`, `selection`,
/// `folder_snapshot`, `folder_live`, `version`).
///
/// Caller: `LinkOptions::kind`, `validate`, `LinkRecord::follow_latest`, `repoint`;
/// Seam: S16d (the share dialog's choice), S9 (the `reg_link.kind` column).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LinkKind {
    /// One file.
    File,
    /// Hand-picked files.
    Selection,
    /// A folder as it is now.
    FolderSnapshot,
    /// A folder that follows edits: writers re-point the link at each new version.
    FolderLive,
    /// One pinned version of a file.
    Version,
}

words!(
    LinkKind,
    "link kind",
    [
        File => "file",
        Selection => "selection",
        FolderSnapshot => "folder_snapshot",
        FolderLive => "folder_live",
        Version => "version",
    ]
);

impl LinkKind {
    /// Whether the link shares a folder (`subfolders` is a choice of the folder kinds only).
    pub const fn is_folder(self) -> bool {
        matches!(self, LinkKind::FolderSnapshot | LinkKind::FolderLive)
    }

    /// Whether the link follows the latest version ("Live folders"): only `folder_live`. The record
    /// derives its `follow_latest` from this, so the two cannot disagree.
    pub const fn follows_latest(self) -> bool {
        matches!(self, LinkKind::FolderLive)
    }

    /// Whether a link of this kind names at least one object to serve when it is made: every kind
    /// but a live folder, which may be made over a folder that is still empty and is filled by the
    /// writers' `repoint`.
    pub const fn needs_objects(self) -> bool {
        !matches!(self, LinkKind::FolderLive)
    }
}
