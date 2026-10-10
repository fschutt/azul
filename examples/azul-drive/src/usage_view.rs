//! An Azlin drive's space in AzDrive. The quota counts STORED bytes - what arrives at the storage
//! nodes, compressed and encrypted on this computer first - so the status line's "38 GB
//! available" and the details pane's "62 GB used of 100 GB, your files are 99 GB before
//! compression" come from the token server's count of stored bytes (azcloud-kit's
//! `DriveStatus`, seen by the periods' look) and, as extra information, the drive index's sum of
//! the files' sizes (an encrypted drive's totals, asked for at most every
//! [`TOTALS_EVERY_SECS`]). A drive nearly full or full says so once a run.

use azcloud_kit::{
    usage::{size_text, Level, Usage},
    DriveStatus,
};

use crate::DriveState;

/// How long a drive's totals (its files' size before compression) are good for.
pub(crate) const TOTALS_EVERY_SECS: u64 = 1800;

/// What AzDrive knows of a drive's space.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct DriveUsage {
    pub usage: Usage,
    /// `usage.used` is the token server's count (else the drive index's sum of its objects).
    pub server_counted: bool,
    /// When the drive index's totals came, in seconds since 1970.
    pub totals_at: Option<u64>,
    /// The nearly-full or full warning was said this run.
    pub warned: bool,
}

/// `previous` with what the token server said of the drive: its quota and its stored bytes.
#[must_use]
pub(crate) fn merge_status(
    previous: Option<DriveUsage>,
    status: &DriveStatus,
) -> Option<DriveUsage> {
    let Some(quota) = status.quota_bytes else {
        return previous;
    };
    let mut seen = previous.unwrap_or_default();
    seen.usage.quota = quota;
    if let Some(used) = status.used_bytes {
        seen.usage.used = used;
        seen.server_counted = true;
    }
    Some(seen)
}

/// `previous` with the drive index's totals at `now`: the files' size before compression, and
/// - while the token server has not counted - their objects' stored bytes.
#[must_use]
pub(crate) fn merge_totals(
    previous: Option<DriveUsage>,
    original_bytes: u64,
    stored_bytes: u64,
    now: u64,
) -> Option<DriveUsage> {
    let mut seen = previous.unwrap_or_default();
    seen.usage.original = Some(original_bytes);
    if !seen.server_counted {
        seen.usage.used = stored_bytes;
    }
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

/// The status line's part: "38 GB available" (of the quota, in stored bytes).
#[must_use]
pub(crate) fn available_part(usage: &DriveUsage) -> Option<String> {
    (usage.usage.quota > 0).then(|| format!("{} available", size_text(usage.usage.available())))
}

/// The token server's word on the Azlin drive `azlin_id` (the periods' look): its quota and its
/// stored bytes; a drive nearly full or full says so once a run.
pub(crate) fn status_seen(s: &mut DriveState, azlin_id: &str, status: &DriveStatus) {
    let previous = s.usage.get(azlin_id).copied();
    let Some(mut seen) = merge_status(previous, status) else {
        return;
    };
    if seen.usage.level() != Level::Fine && !seen.warned {
        if let Some(warning) = seen.usage.warning() {
            let name = s.drive_name(&crate::browse::Place::folder(
                &slot_id_of(s, azlin_id).unwrap_or_else(|| azlin_id.to_string()),
                "",
            ));
            println!("AZDRIVE_DRIVE_NEARLY_FULL {azlin_id}");
            s.warn(format!("\"{name}\": {warning}"));
        }
        seen.warned = true;
    }
    s.usage.insert(azlin_id.to_string(), seen);
}

/// The drive index's totals of the Azlin drive `azlin_id` arrived.
pub(crate) fn totals_seen(s: &mut DriveState, azlin_id: &str, original: u64, stored: u64, now: u64) {
    let previous = s.usage.get(azlin_id).copied();
    if let Some(seen) = merge_totals(previous, original, stored, now) {
        s.usage.insert(azlin_id.to_string(), seen);
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
        let seen = merge_status(None, &status(Some(100 * GB), Some(62 * GB))).unwrap();
        assert_eq!((seen.usage.quota, seen.usage.used), (100 * GB, 62 * GB));
        assert!(seen.server_counted);
        assert_eq!(available_part(&seen).as_deref(), Some("38 GB available"));
        // The index's totals add the original size; they never replace the server's count.
        let seen = merge_totals(Some(seen), 99 * GB, 61 * GB, 1_000).unwrap();
        assert_eq!(seen.usage.used, 62 * GB);
        assert_eq!(seen.usage.original, Some(99 * GB));
        assert_eq!(
            seen.usage.text(),
            "62 GB used of 100 GB, your files are 99 GB before compression"
        );
        assert!(!totals_due(Some(&seen), 1_000 + TOTALS_EVERY_SECS - 1));
        assert!(totals_due(Some(&seen), 1_000 + TOTALS_EVERY_SECS));
    }

    #[test]
    fn without_the_servers_count_the_index_stored_bytes_stand_in() {
        let seen = merge_status(None, &status(Some(100 * GB), None)).unwrap();
        assert!(!seen.server_counted);
        let seen = merge_totals(Some(seen), 30 * GB, 20 * GB, 5).unwrap();
        assert_eq!(seen.usage.used, 20 * GB);
        assert_eq!(available_part(&seen).as_deref(), Some("80 GB available"));
        // A server that never said its quota: nothing to show yet.
        assert_eq!(merge_status(None, &status(None, None)), None);
        assert!(totals_due(None, 0));
    }
}
