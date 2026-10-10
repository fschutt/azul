//! An Azlin drive's space in AzDrive. The quota counts STORED bytes - what arrives at the storage
//! nodes, compressed and encrypted on this computer first - so the status line's "38 GB
//! available" and the details pane's "62 GB used of 100 GB, your files are 99 GB before
//! compression" take "used" from what the drive's node counts: its HeadBucket answer
//! (`x-azlin-used-bytes`, `x-azlin-quota-bytes`, SRV17; asked at most every
//! [`SPACE_EVERY_SECS`]), else the token server's drive status (the periods' look). The drive
//! index's totals (an encrypted drive's, asked for at most every [`TOTALS_EVERY_SECS`]) give the
//! files' size before compression - extra information, never the space used - and, only while
//! no server said its count, their objects' stored bytes as an ESTIMATE (the lines say so). A
//! drive nearly full or full says so once a run.

use std::sync::Arc;

use azcloud_kit::{
    usage::{size_text, Level, Usage},
    DriveStatus,
};
use azul::prelude::*;
use azul_storage::BucketSpace;

use crate::{jobs::Job, spawn, DriveState};

/// How long a drive's totals (its files' size before compression) are good for.
pub(crate) const TOTALS_EVERY_SECS: u64 = 1800;

/// How long the node's count of a drive's stored bytes is good for.
pub(crate) const SPACE_EVERY_SECS: u64 = 60;

/// What AzDrive knows of a drive's space, by where it came from; [`DriveUsage::usage`] makes
/// the lines' figures of it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct DriveUsage {
    /// The drive's node's count (HeadBucket): what the quota counts.
    pub node: BucketSpace,
    /// The token server's quota and count of stored bytes (its drive status).
    pub server: BucketSpace,
    /// The drive index's sum of the files' sizes: their size before compression.
    pub original: Option<u64>,
    /// The drive index's sum of its objects' stored sizes: this computer's estimate, for while
    /// no server counted.
    pub estimated: Option<u64>,
    /// When the drive index's totals were asked for, in seconds since 1970.
    pub totals_at: Option<u64>,
    /// When the node was asked for its count.
    pub space_at: Option<u64>,
    /// The nearly-full or full warning was said this run.
    pub warned: bool,
}

impl DriveUsage {
    /// The space as the lines show it: the node's count, else the token server's, else the
    /// estimate; the quota the node's, else the token server's.
    #[must_use]
    pub(crate) fn usage(&self) -> Usage {
        let counted = self.node.used_bytes.or(self.server.used_bytes);
        Usage {
            used: counted.or(self.estimated).unwrap_or(0),
            quota: self
                .node
                .quota_bytes
                .or(self.server.quota_bytes)
                .unwrap_or(0),
            original: self.original,
            estimate: counted.is_none() && self.estimated.is_some(),
        }
    }
}

/// What came in replaces what was known; what it does not say stays.
fn merged(known: BucketSpace, new: BucketSpace) -> BucketSpace {
    BucketSpace {
        used_bytes: new.used_bytes.or(known.used_bytes),
        quota_bytes: new.quota_bytes.or(known.quota_bytes),
    }
}

/// `previous` with what the token server said of the drive: its quota and its stored bytes.
#[must_use]
pub(crate) fn merge_status(
    previous: Option<DriveUsage>,
    status: &DriveStatus,
) -> Option<DriveUsage> {
    let said = BucketSpace {
        used_bytes: status.used_bytes,
        quota_bytes: status.quota_bytes,
    };
    if said == BucketSpace::default() {
        return previous;
    }
    let mut seen = previous.unwrap_or_default();
    seen.server = merged(seen.server, said);
    Some(seen)
}

/// `previous` with what the drive's storage node counts (its HeadBucket answer): the stored
/// bytes and the quota, which every other count gives way to.
#[must_use]
pub(crate) fn merge_space(previous: Option<DriveUsage>, space: &BucketSpace) -> Option<DriveUsage> {
    if *space == BucketSpace::default() {
        return previous;
    }
    let mut seen = previous.unwrap_or_default();
    seen.node = merged(seen.node, *space);
    Some(seen)
}

/// `previous` with the drive index's totals at `now`: the files' size before compression, and
/// their objects' stored bytes (the estimate while no server counted).
#[must_use]
pub(crate) fn merge_totals(
    previous: Option<DriveUsage>,
    original_bytes: u64,
    stored_bytes: u64,
    now: u64,
) -> Option<DriveUsage> {
    let mut seen = previous.unwrap_or_default();
    seen.original = Some(original_bytes);
    seen.estimated = Some(stored_bytes);
    seen.totals_at = Some(now);
    Some(seen)
}

/// Whether the drive index's totals are due again.
#[must_use]
pub(crate) fn totals_due(usage: Option<&DriveUsage>, now: u64) -> bool {
    usage
        .and_then(|u| u.totals_at)
        .map_or(true, |at| now.saturating_sub(at) >= TOTALS_EVERY_SECS)
}

/// Whether the node's count is due again.
#[must_use]
pub(crate) fn space_due(usage: Option<&DriveUsage>, now: u64) -> bool {
    usage
        .and_then(|u| u.space_at)
        .map_or(true, |at| now.saturating_sub(at) >= SPACE_EVERY_SECS)
}

/// The status line's part: "38 GB available" (of the quota, in stored bytes; an estimate:
/// "about 38 GB available"), in the window's language.
#[must_use]
pub(crate) fn available_part(usage: &DriveUsage) -> Option<String> {
    let usage = usage.usage();
    (usage.quota > 0).then(|| {
        azul_appkit::l10n::t_args(
            "azdrive-usage-available",
            &[
                ("size", azul_appkit::l10n::Arg::from(size_text(usage.available()))),
                ("about", azul_appkit::l10n::Arg::from(yes(usage.estimate))),
            ],
        )
    })
}

/// A flag as a message's argument (`yes`, `no`).
fn yes(on: bool) -> &'static str {
    if on {
        "yes"
    } else {
        "no"
    }
}

/// The details pane's line: "62 GB used of 100 GB, your files are 99 GB before compression"
/// (the second part when the original size is known and bigger than what is stored).
#[must_use]
pub(crate) fn usage_text(usage: &Usage) -> String {
    use azul_appkit::l10n::{t_args, Arg};
    let used = size_text(usage.used);
    let mut text = t_args(
        "azdrive-usage-used",
        &[
            ("used", Arg::from(used.as_str())),
            ("quota", Arg::from(size_text(usage.quota))),
            ("about", Arg::from(yes(usage.estimate))),
        ],
    );
    if let Some(original) = usage.original.filter(|o| *o > usage.used) {
        let original = size_text(original);
        if original != used {
            text.push_str(", ");
            text.push_str(&t_args("azdrive-usage-original", &[("size", Arg::from(original))]));
        }
    }
    text
}

/// A warning for a drive that is nearly full or full, in the window's language.
fn usage_warning(name: &str, usage: &Usage) -> Option<azul_appkit::l10n::Phrase> {
    let said = match usage.level() {
        Level::Fine => return None,
        Level::NearlyFull => "azdrive-usage-nearly-full",
        Level::Full => "azdrive-usage-full",
    };
    Some(
        azul_appkit::l10n::Phrase::new(said)
            .arg("name", name)
            .arg("used", size_text(usage.used))
            .arg("quota", size_text(usage.quota))
            .arg("about", yes(usage.estimate)),
    )
}

/// `seen` is the space of the Azlin drive `azlin_id` now: a drive nearly full or full says so
/// once a run.
fn store(s: &mut DriveState, azlin_id: &str, mut seen: DriveUsage) {
    let usage = seen.usage();
    if usage.level() != Level::Fine && !seen.warned {
        let name = s.drive_name(&crate::browse::Place::folder(
            &slot_id_of(s, azlin_id).unwrap_or_else(|| azlin_id.to_string()),
            "",
        ));
        if let Some(warning) = usage_warning(&name, &usage) {
            println!("AZDRIVE_DRIVE_NEARLY_FULL {azlin_id}");
            s.warn(warning);
        }
        seen.warned = true;
    }
    s.usage.insert(azlin_id.to_string(), seen);
}

/// The token server's word on the Azlin drive `azlin_id` (the periods' look): its quota and its
/// stored bytes.
pub(crate) fn status_seen(s: &mut DriveState, azlin_id: &str, status: &DriveStatus) {
    let previous = s.usage.get(azlin_id).copied();
    if let Some(seen) = merge_status(previous, status) {
        store(s, azlin_id, seen);
    }
}

/// The drive index's totals of the Azlin drive `azlin_id` arrived.
pub(crate) fn totals_seen(s: &mut DriveState, azlin_id: &str, original: u64, stored: u64, now: u64) {
    let previous = s.usage.get(azlin_id).copied();
    if let Some(seen) = merge_totals(previous, original, stored, now) {
        store(s, azlin_id, seen);
    }
}

/// The node's count of the Azlin drive `azlin_id` arrived (`AZDRIVE_SPACE <drive> <used>
/// <quota>`, `-` for what it did not say).
pub(crate) fn space_seen(s: &mut DriveState, azlin_id: &str, space: &BucketSpace) {
    let said = |n: Option<u64>| n.map_or_else(|| String::from("-"), |n| n.to_string());
    println!(
        "AZDRIVE_SPACE {azlin_id} {} {}",
        said(space.used_bytes),
        said(space.quota_bytes)
    );
    let previous = s.usage.get(azlin_id).copied();
    if let Some(seen) = merge_space(previous, space) {
        store(s, azlin_id, seen);
    }
}

/// Asks the node of every opened Azlin drive whose count is due what it stores (one HeadBucket
/// each, in the background): after a listing and at the periods' look.
pub(crate) fn request_space(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState) {
    let now = azul_storage::time::now_unix();
    let due: Vec<(String, Arc<azcloud_kit::AzlinDrive>)> = s
        .slots
        .iter()
        .filter_map(|slot| {
            let (azlin_id, _) = slot.entry.azlin()?;
            let azlin = slot.azlin.clone()?;
            space_due(s.usage.get(azlin_id), now).then(|| (azlin_id.to_string(), azlin))
        })
        .collect();
    for (azlin_id, azlin) in due {
        // Asked once: a second listing before the answer does not ask again.
        let previous = s.usage.get(&azlin_id).copied().unwrap_or_default();
        s.usage.insert(
            azlin_id.clone(),
            DriveUsage {
                space_at: Some(now),
                ..previous
            },
        );
        spawn(info, app, s, Job::BucketSpace { azlin_id, azlin });
    }
}

/// The slot of the Azlin drive `azlin_id`.
fn slot_id_of(s: &DriveState, azlin_id: &str) -> Option<String> {
    s.slots
        .iter()
        .find(|slot| slot.entry.azlin().is_some_and(|(id, _)| id == azlin_id))
        .map(|slot| slot.entry.id.clone())
}

/// What AzDrive knows of the space of the drive in slot `slot_id` (an Azlin drive).
pub(crate) fn usage_of_slot<'a>(s: &'a DriveState, slot_id: &str) -> Option<&'a DriveUsage> {
    let slot = s.slots.iter().find(|slot| slot.entry.id == slot_id)?;
    let (azlin_id, _) = slot.entry.azlin()?;
    s.usage.get(azlin_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    const GB: u64 = 1_000_000_000;

    fn status(quota: Option<u64>, used: Option<u64>) -> DriveStatus {
        DriveStatus {
            quota_bytes: quota,
            used_bytes: used,
            ..DriveStatus::default()
        }
    }

    #[test]
    fn the_space_left_is_the_quota_less_the_stored_bytes_the_server_counted() {
        crate::l10n::in_english();
        let seen = merge_status(None, &status(Some(100 * GB), Some(62 * GB))).unwrap();
        assert_eq!((seen.usage().quota, seen.usage().used), (100 * GB, 62 * GB));
        assert!(!seen.usage().estimate);
        assert_eq!(available_part(&seen).as_deref(), Some("38 GB available"));
        // The index's totals add the original size; they never replace the server's count.
        let seen = merge_totals(Some(seen), 99 * GB, 61 * GB, 1_000).unwrap();
        assert_eq!(seen.usage().used, 62 * GB);
        assert_eq!(seen.usage().original, Some(99 * GB));
        assert_eq!(
            seen.usage().text(),
            "62 GB used of 100 GB, your files are 99 GB before compression"
        );
        assert!(!totals_due(Some(&seen), 1_000 + TOTALS_EVERY_SECS - 1));
        assert!(totals_due(Some(&seen), 1_000 + TOTALS_EVERY_SECS));
    }

    /// SRV17: the node's HeadBucket count is the space used; the drive index's totals only add
    /// the files' size before compression, and the token server's word does not replace it.
    #[test]
    fn the_nodes_count_is_the_space_used_and_the_index_only_adds_the_original_size() {
        crate::l10n::in_english();
        let node = BucketSpace {
            used_bytes: Some(62 * GB),
            quota_bytes: Some(100 * GB),
        };
        let seen = merge_space(None, &node).unwrap();
        assert_eq!((seen.usage().used, seen.usage().quota), (62 * GB, 100 * GB));
        assert!(!seen.usage().estimate);
        assert_eq!(available_part(&seen).as_deref(), Some("38 GB available"));
        let seen = merge_totals(Some(seen), 99 * GB, 61 * GB, 1_000).unwrap();
        let seen = merge_status(Some(seen), &status(Some(100 * GB), Some(50 * GB))).unwrap();
        assert_eq!(seen.usage().used, 62 * GB, "the node's count stays");
        assert_eq!(
            seen.usage().text(),
            "62 GB used of 100 GB, your files are 99 GB before compression"
        );
        // A later count of the node replaces it.
        let seen = merge_space(
            Some(seen),
            &BucketSpace {
                used_bytes: Some(70 * GB),
                quota_bytes: None,
            },
        )
        .unwrap();
        assert_eq!((seen.usage().used, seen.usage().quota), (70 * GB, 100 * GB));
    }

    /// Without the node's headers (an old node, another S3) the drive index's sum of its
    /// objects stands in - an estimate, and the lines say so.
    #[test]
    fn without_the_nodes_count_the_index_stored_bytes_are_an_estimate() {
        crate::l10n::in_english();
        let seen = merge_status(None, &status(Some(100 * GB), None)).unwrap();
        let seen = merge_space(Some(seen), &BucketSpace::default()).unwrap();
        let seen = merge_totals(Some(seen), 30 * GB, 20 * GB, 5).unwrap();
        assert_eq!(seen.usage().used, 20 * GB);
        assert!(seen.usage().estimate);
        assert_eq!(available_part(&seen).as_deref(), Some("about 80 GB available"));
        // Then the node counts: no estimate any more.
        let seen = merge_space(
            Some(seen),
            &BucketSpace {
                used_bytes: Some(21 * GB),
                quota_bytes: Some(100 * GB),
            },
        )
        .unwrap();
        assert!(!seen.usage().estimate);
        assert_eq!(available_part(&seen).as_deref(), Some("79 GB available"));
        // A server that never said its quota: nothing to show yet.
        assert_eq!(merge_status(None, &status(None, None)), None);
        assert_eq!(merge_space(None, &BucketSpace::default()), None);
        assert!(totals_due(None, 0));
        assert!(space_due(None, 0));
        let asked = DriveUsage {
            space_at: Some(10),
            ..DriveUsage::default()
        };
        assert!(!space_due(Some(&asked), 10 + SPACE_EVERY_SECS - 1));
        assert!(space_due(Some(&asked), 10 + SPACE_EVERY_SECS));
    }
}
