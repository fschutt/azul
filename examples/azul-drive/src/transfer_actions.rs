//! One notice per user action that moves files. A drop, a paste, an upload or a download
//! queues one transfer per source folder; the action's one notice says what it does while
//! they run, and once the last one ended it says how the action went, in the same place of
//! the toast stack. A failure shows in the InfoBar instead, and the action's notice goes.

use std::collections::HashMap;

use azul_appkit::l10n::{Phrase, Text};
use azul_storage::key;

use crate::{
    fileops::{SourceItem, TransferKind},
    Message, MessageKind,
};

/// A user action's transfers and what they did so far.
#[derive(Debug, Clone)]
struct Action {
    /// What the action does (the first transfer's kind; one action never mixes them).
    kind: TransferKind,
    /// How many items its transfers take together.
    items: usize,
    /// The one item's name, when it takes exactly one (the label names it).
    name: String,
    /// Where they go: the folder's name (a download's is the Downloads folder's path).
    target: String,
    /// Its transfers still queued or running; the action ends when this is back at 0.
    open: usize,
    /// Items its ended transfers copied, moved or downloaded.
    done: usize,
    /// Items they left (a conflict answered with Skip).
    skipped: usize,
    /// One of them was cancelled.
    cancelled: bool,
    /// One of them failed (its error is in the InfoBar).
    failed: bool,
}

/// How a transfer of an action ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Ended {
    /// It ran: what it did.
    Ran {
        done: usize,
        skipped: usize,
        cancelled: bool,
        failed: bool,
    },
    /// Its plan was empty (the items are where they would go).
    NothingToDo,
    /// It was cancelled before it ran.
    Cancelled,
    /// It could not start (its error is in the InfoBar).
    Failed,
}

/// What an action's notice does now.
#[derive(Debug, Clone)]
pub(crate) enum NoticeChange {
    /// It shows `Message`, in the action's place (or as the newest when it left meanwhile).
    Show(Message),
    /// It stays as it is: other transfers of the action still run.
    Keep,
    /// It goes: the action failed and the InfoBar says why.
    Remove,
}

/// The user actions with transfers still open.
#[derive(Debug, Clone, Default)]
pub(crate) struct TransferActions {
    /// The id the last action took; ids never repeat.
    last_id: u64,
    /// The actions by id, while one of their transfers is open.
    open: HashMap<u64, Action>,
}

impl TransferActions {
    /// A new user action (a drop, a paste, an upload, a download): its id, for each transfer it
    /// queues and for its notice.
    pub fn begin(&mut self) -> u64 {
        self.last_id += 1;
        self.last_id
    }

    /// A transfer of `action` was queued: `items` of `kind` into `target`. What the action's
    /// notice says now ("Uploading 3 items to docs…").
    pub fn queued(
        &mut self,
        action: u64,
        kind: TransferKind,
        items: &[SourceItem],
        target: &str,
    ) -> Message {
        let entry = self.open.entry(action).or_insert_with(|| Action {
            kind,
            items: 0,
            name: String::new(),
            target: target.to_string(),
            open: 0,
            done: 0,
            skipped: 0,
            cancelled: false,
            failed: false,
        });
        entry.open += 1;
        if entry.items == 0 {
            if let [one] = items {
                entry.name = key::last_segment(&one.key).to_string();
            }
        }
        entry.items += items.len();
        Message {
            kind: MessageKind::Info,
            text: Phrase::new("azdrive-transfer-label")
                .arg("kind", entry.kind.name())
                .arg("count", entry.items)
                .arg("name", entry.name.as_str())
                .arg("target", entry.target.as_str())
                .then("…"),
        }
    }

    /// Whether `action` has a transfer queued or running.
    pub fn is_open(&self, action: u64) -> bool {
        self.open.contains_key(&action)
    }

    /// A transfer of `action` ended `how`. Once the action's last one ended, the notice says
    /// how the whole action went; before, it stays.
    pub fn ended(&mut self, action: u64, how: Ended) -> NoticeChange {
        let Some(entry) = self.open.get_mut(&action) else {
            return NoticeChange::Keep;
        };
        entry.open = entry.open.saturating_sub(1);
        match how {
            Ended::Ran {
                done,
                skipped,
                cancelled,
                failed,
            } => {
                entry.done += done;
                entry.skipped += skipped;
                entry.cancelled |= cancelled;
                entry.failed |= failed;
            }
            Ended::NothingToDo => {}
            Ended::Cancelled => entry.cancelled = true,
            Ended::Failed => entry.failed = true,
        }
        if entry.open > 0 {
            return NoticeChange::Keep;
        }
        let Some(action) = self.open.remove(&action) else {
            return NoticeChange::Keep;
        };
        outcome(&action)
    }
}

/// The notice of an action whose transfers all ended.
fn outcome(action: &Action) -> NoticeChange {
    let (kind, text): (MessageKind, Text) = if action.failed {
        return NoticeChange::Remove;
    } else if action.cancelled && action.done == 0 {
        (MessageKind::Info, Text::key("azdrive-transfer-cancelled"))
    } else if action.cancelled {
        (
            MessageKind::Info,
            Phrase::new("azdrive-transfer-cancelled-after")
                .arg("count", action.done)
                .into(),
        )
    } else if action.done == 0 && action.skipped == 0 {
        (MessageKind::Info, Text::key("azdrive-transfer-nothing-to-do"))
    } else if action.skipped > 0 {
        (
            MessageKind::Success,
            Phrase::new("azdrive-transfer-done-skipped")
                .arg("count", action.done)
                .arg("skipped", action.skipped)
                .into(),
        )
    } else if action.kind == TransferKind::Download {
        (
            MessageKind::Success,
            Phrase::new("azdrive-downloaded")
                .arg("count", action.done)
                .arg("folder", action.target.as_str())
                .into(),
        )
    } else {
        (
            MessageKind::Success,
            Phrase::new("azdrive-transfer-done")
                .arg("count", action.done)
                .into(),
        )
    };
    NoticeChange::Show(Message { kind, text })
}
