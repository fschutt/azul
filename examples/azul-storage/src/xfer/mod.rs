//! The transfer engine: copy, move, replace and keep-both between two drives, with a plan made
//! first so the user decides each conflict before a byte moves (ARCH28 RF43 / AR5a-2).
//!
//! - [`site`]: where a drive's files are and whether two places overlap - by what the drives are
//!   and, on a disk, which file is which, never by drive ids or key text.
//!
//! The planner and the runner follow in the modules below as they land; AzDrive's `fileops`
//! keeps the words (the names Explorer makes, the toasts) and the queue.

pub mod site;

pub use site::{names_below, overlap, OtherId, Overlap, Site, Space};
