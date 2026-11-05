//! The transfer engine: copy, move, replace and keep-both between two drives, with a plan made
//! first so the user decides each conflict before a byte moves (ARCH28 RF43 / AR5a-2).
//!
//! - [`site`]: where a drive's files are and whether two places overlap - by what the drives are
//!   and, on a disk, which file is which, never by drive ids or key text.
//! - [`targets`]: the names a transfer will have made at its target, folded the way the target's
//!   disk compares them; [`names`]: the ones Explorer makes ("a - Copy.txt", "a (2).txt").
//! - [`plan`]: everything a transfer will do, decided before a byte moves.
//!
//! The runner follows in a module of its own as it lands; AzDrive's `fileops` keeps the words
//! (the toasts, the dialog's wording) and the queue.

pub mod names;
pub mod plan;
pub mod site;
pub mod targets;

pub use plan::{parent_of, plan, Choice, Item, Plan, PlannedFile};
pub use site::{names_below, overlap, OtherId, Overlap, Site, Space};
pub use targets::{volume_of, Claim, TargetSet};
