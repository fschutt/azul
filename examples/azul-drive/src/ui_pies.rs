//! The two pies at the right of the navigation pane's foot, in place of a status bar: the sync
//! (how far it has come, coloured by its state: the accent while syncing, green up to date,
//! amber paused or held back, red for a conflict or an error) and the drive's space (how full
//! it is: the accent, amber past 85 %, red past 97 %). Their words are their accessible names;
//! the numbers are in the sync popover a click on either opens: the sync's state and progress,
//! the drive's usage with its meter, a warning near full, this computer's health line, the
//! transfers running, and the actions (Sync now, Pause / Resume, Sync anyway, Show transfers).

/// The pie's colour: what its state means.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PieTone {
    /// The theme's accent: syncing, or a drive with room.
    Accent,
    /// Green: up to date.
    Good,
    /// Amber: paused, held back by the network, nearly full.
    Warn,
    /// Red: a conflict, an error, full.
    Bad,
}

/// What a pie shows.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Pie {
    /// How much of the disc the wedge covers, 0 to 1.
    pub fraction: f32,
    pub tone: PieTone,
    /// It breathes while the sync is busy.
    pub breathing: bool,
}

/// The sync pie: from the sync status's key (`None`: nothing in view syncs) and how far the
/// work running has come (a pass, or the transfers when nothing syncs).
#[must_use]
pub(crate) fn sync_pie(status: Option<&str>, progress: Option<f32>) -> Pie {
    let _ = (status, progress);
    todo!()
}

/// The space pie of a drive using `used` of `total` bytes (`total` 0: unknown, an empty disc).
#[must_use]
pub(crate) fn space_pie(used: u64, total: u64) -> Pie {
    let _ = (used, total);
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sync_pie_fills_while_syncing_and_is_full_in_its_states_colour_otherwise() {
        let busy = sync_pie(Some("azdrive-sync-status-syncing-files"), Some(0.42));
        assert_eq!(busy.tone, PieTone::Accent);
        assert!((busy.fraction - 0.42).abs() < 1e-6);
        assert!(busy.breathing, "it breathes while busy");
        let done = sync_pie(Some("azdrive-sync-status-up-to-date"), None);
        assert_eq!((done.fraction, done.tone, done.breathing), (1.0, PieTone::Good, false));
        for held in [
            "azdrive-sync-status-paused",
            "azdrive-sync-status-metered",
            "azdrive-sync-status-payment-due",
        ] {
            assert_eq!(sync_pie(Some(held), None).tone, PieTone::Warn, "{held}");
        }
        for bad in [
            "azdrive-sync-status-conflicts",
            "azdrive-sync-status-failed",
            "azdrive-sync-status-burst",
        ] {
            let pie = sync_pie(Some(bad), None);
            assert_eq!((pie.fraction, pie.tone), (1.0, PieTone::Bad), "{bad}");
        }
        let never = sync_pie(Some("azdrive-sync-status-never"), None);
        assert_eq!((never.fraction, never.tone), (0.0, PieTone::Accent));
    }

    /// Nothing in view syncs: the pie shows the transfers' progress while they run, else an
    /// empty disc.
    #[test]
    fn without_a_sync_the_pie_shows_the_transfers() {
        let copying = sync_pie(None, Some(0.25));
        assert_eq!((copying.tone, copying.breathing), (PieTone::Accent, true));
        assert!((copying.fraction - 0.25).abs() < 1e-6);
        let quiet = sync_pie(None, None);
        assert_eq!((quiet.fraction, quiet.breathing), (0.0, false));
    }

    #[test]
    fn the_space_pie_turns_amber_past_85_and_red_past_97_percent() {
        let half = space_pie(50, 100);
        assert_eq!((half.fraction, half.tone), (0.5, PieTone::Accent));
        assert_eq!(space_pie(85, 100).tone, PieTone::Accent, "85 % is not past 85 %");
        assert_eq!(space_pie(86, 100).tone, PieTone::Warn);
        assert_eq!(space_pie(98, 100).tone, PieTone::Bad);
        assert_eq!(space_pie(150, 100).fraction, 1.0, "never more than the disc");
        assert_eq!(space_pie(10, 0).fraction, 0.0, "an unknown size is an empty disc");
        assert!(!space_pie(98, 100).breathing);
    }
}
