//! The two pies at the right of the navigation pane's foot, in place of a status bar: the sync
//! (how far it has come, coloured by its state: the accent while syncing, green up to date,
//! amber paused or held back, red for a conflict or an error) and the drive's space (how full
//! it is: the accent, amber past 85 %, red past 97 %). Their words are their accessible names;
//! the numbers are in the sync popover a click on either opens: the sync's state and progress,
//! the drive in view's storage problem (which also turns the sync pie red), the drive's usage
//! with its meter, a warning near full, this computer's health line, the transfers running, and
//! the actions (Sync now, Pause / Resume, Show transfers).

use azul::{
    callbacks::ButtonOnClickCallbackType,
    misc::TransientAnchor,
    prelude::*,
    str::String as AzString,
    widgets::{ButtonType, Dialog, DialogClosedBy, PieMarker, PieTone as MarkerTone},
};
use azul_appkit::l10n::{label, t, t_args, t_text, Arg};

use crate::{
    actions::{self, Action, ActionRef},
    ids, look,
    sync_view::{self, SyncAction},
    usage_view, with_state, DriveState, HOME_ID,
};

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
    let full = |tone| Pie {
        fraction: 1.0,
        tone,
        breathing: false,
    };
    let busy = |progress: Option<f32>| Pie {
        fraction: progress.unwrap_or(0.0).clamp(0.0, 1.0),
        tone: PieTone::Accent,
        breathing: true,
    };
    let Some(status) = status else {
        // Nothing in view syncs: the transfers while they run, else an empty disc.
        return match progress {
            Some(_) => busy(progress),
            None => Pie {
                fraction: 0.0,
                tone: PieTone::Accent,
                breathing: false,
            },
        };
    };
    match status {
        "azdrive-sync-status-syncing" | "azdrive-sync-status-syncing-files" => busy(progress),
        "azdrive-sync-status-up-to-date" => full(PieTone::Good),
        "azdrive-sync-status-never" => Pie {
            fraction: 0.0,
            tone: PieTone::Accent,
            breathing: false,
        },
        "azdrive-sync-status-paused"
        | "azdrive-sync-status-metered"
        | "azdrive-sync-status-payment-due"
        | "azdrive-sync-status-read-only"
        | "azdrive-sync-status-newer-format" => full(PieTone::Warn),
        // A conflict, an error, a burst or a mass delete waiting for an answer.
        _ => full(PieTone::Bad),
    }
}

/// The space pie of a drive using `used` of `total` bytes (`total` 0: unknown, an empty disc).
#[must_use]
pub(crate) fn space_pie(used: u64, total: u64) -> Pie {
    let fraction = if total == 0 {
        0.0
    } else {
        (used as f64 / total as f64).clamp(0.0, 1.0) as f32
    };
    let tone = if fraction > 0.97 {
        PieTone::Bad
    } else if fraction > 0.85 {
        PieTone::Warn
    } else {
        PieTone::Accent
    };
    Pie {
        fraction,
        tone,
        breathing: false,
    }
}

// ==== In the window ====

/// The drive the sync pie speaks of: the pairing of the open folder, else the drive the sync
/// commands are about when it syncs, else the first drive that syncs.
fn sync_drive(s: &DriveState) -> Option<String> {
    sync_view::place_in_pair(s)
        .map(|(drive_id, _)| drive_id)
        .or_else(|| sync_view::target_drive(s).filter(|id| sync_view::setup_of(s, id).is_some()))
        .or_else(|| s.settings.synced.first().map(|setup| setup.drive_id.clone()))
}

/// The drive the space pie speaks of: the open folder's drive, else Home.
fn space_drive(s: &DriveState) -> Option<String> {
    s.current_drive_id()
        .or_else(|| s.slot_index(HOME_ID).map(|_| HOME_ID.to_string()))
}

/// How far the work running has come (0 to 1): the sync's pass of `drive_id`, else the
/// transfers.
fn progress(s: &DriveState, drive_id: Option<&str>) -> Option<f32> {
    let pass = drive_id
        .and_then(|id| s.sync_view.drives.get(id))
        .and_then(|d| d.running.as_ref())
        .map(|running| {
            let p = &running.progress;
            if p.bytes_total > 0 {
                p.bytes_done as f32 / p.bytes_total as f32
            } else if p.files_total > 0 {
                p.files_done as f32 / p.files_total as f32
            } else {
                0.0
            }
        });
    pass.or_else(|| s.queue.percent().map(|percent| percent / 100.0))
}

/// The bytes drive `drive_id` uses of what it has: a folder on this computer's volume, an Azlin
/// drive's stored bytes of its quota; `None` where that is not known.
fn space_of(s: &DriveState, drive_id: &str) -> Option<(u64, u64)> {
    if let Some((total, free)) = s.disk.get(drive_id) {
        return Some((total.saturating_sub(*free), *total));
    }
    let usage = usage_view::usage_of_slot(s, drive_id)?.usage();
    (usage.quota > 0).then_some((usage.used, usage.quota))
}

/// What the sync pie's words are: its drive's status line ("Up to date", "Syncing 12 files
/// (340 MB)"), with how far it came while busy; the transfers' line when nothing syncs.
fn sync_words(s: &DriveState, drive_id: Option<&str>, pie: &Pie) -> String {
    let status = drive_id
        .and_then(|id| sync_view::drive_status(s, id))
        .map(|text| t_text(&text))
        .unwrap_or_else(|| {
            let transfers = t_text(&s.queue.status_text());
            if transfers.is_empty() {
                t("azdrive-pie-no-sync")
            } else {
                transfers
            }
        });
    if pie.breathing {
        format!("{status} \u{00B7} {:.0} %", pie.fraction * 100.0)
    } else {
        status
    }
}

/// The space pie's words: "Home: 38.2 GB used of 100 GB".
fn space_words(s: &DriveState, drive_id: &str) -> String {
    let name = s.drive_name(&crate::browse::Place::folder(drive_id, ""));
    let used = match usage_view::usage_of_slot(s, drive_id) {
        Some(usage) => usage_view::usage_text(&usage.usage()),
        None => match space_of(s, drive_id) {
            Some((used, total)) => t_args(
                "azdrive-usage-used",
                &[
                    ("used", Arg::from(crate::browse::format_size(Some(used)))),
                    ("quota", Arg::from(crate::browse::format_size(Some(total)))),
                    ("about", Arg::from("no")),
                ],
            ),
            None => t("azdrive-pie-space-unknown"),
        },
    };
    format!("{name}: {used}")
}

/// One pie in its 15 x 25 hit area, its words as its accessible name.
fn pie_dom(id: AzString, pie: Pie, words: &str) -> Dom {
    // A pie at work breathes: the widget's Busy tone is the accent, breathing.
    let tone = match pie.tone {
        PieTone::Accent if pie.breathing => MarkerTone::Busy,
        PieTone::Accent => MarkerTone::Accent,
        PieTone::Good => MarkerTone::Good,
        PieTone::Warn => MarkerTone::Warning,
        PieTone::Bad => MarkerTone::Danger,
    };
    // The widget is the 15 x 25 hit area, its hover face and its button: the words are its
    // label.
    PieMarker::create(pie.fraction, tone)
        .with_label(AzString::from(words))
        .dom()
        .with_id(id)
}

/// The two pies at the right of the navigation pane's foot; each opens the sync popover (above
/// it, left-aligned, kept in the window), which closes on a press outside it or Escape.
pub(crate) fn pies(s: &DriveState, app: &RefAny) -> Dom {
    let drive = sync_drive(s);
    let sync = sync_pie(
        drive
            .as_deref()
            .and_then(|id| sync_view::drive_status(s, id))
            .as_ref()
            .and_then(|text| text.keys().first().map(|key| key.to_string()))
            .as_deref(),
        progress(s, drive.as_deref()),
    );
    let mut sync_words = sync_words(s, drive.as_deref(), &sync);
    // A storage problem of the drive in view (in the error table's words, with its error ID)
    // turns the sync pie red; the popover says it in full.
    let problem = crate::problems::status_of(s).map(|text| t_text(&text));
    let sync = match &problem {
        Some(words) => {
            sync_words = words.clone();
            Pie {
                fraction: 1.0,
                tone: PieTone::Bad,
                breathing: false,
            }
        }
        None => sync,
    };
    let mut row = Dom::create_div().with_css(look::PIES);
    row.add_child(popover(s, app, pie_dom(ids::SYNC_PIE, sync, &sync_words)));
    if let Some(space) = space_drive(s) {
        let (used, total) = space_of(s, &space).unwrap_or((0, 0));
        let words = space_words(s, &space);
        row.add_child(popover(s, app, pie_dom(ids::SPACE_PIE, space_pie(used, total), &words)));
    }
    row
}

/// The sync popover on `invoker` (a pie): 330 px, opened above it.
fn popover(s: &DriveState, app: &RefAny, invoker: Dom) -> Dom {
    Dialog::create(popover_content(s, app))
        .with_invoker(invoker)
        .with_modal(false)
        .with_anchor(TransientAnchor::Top)
        .with_closed_by(DialogClosedBy::Any)
        .with_close_button(false)
        .dom()
}

/// A line of the popover.
fn line(text: &str, css: &str) -> Dom {
    Dom::create_div()
        .with_css(css)
        .with_child(Dom::create_span_with_text(AzString::from(text)))
}

/// A button of the popover: closes it, then runs `action`.
fn act(app: &RefAny, text: &str, action: Action, primary: bool) -> Dom {
    let kind = if primary {
        ButtonType::Primary
    } else {
        ButtonType::Default
    };
    Button::with_type(label(text), kind)
        .with_on_click(
            actions::action_ref(app, action),
            on_popover_action as ButtonOnClickCallbackType,
        )
        .dom()
        .with_css("margin-left: 6px;")
}

/// What the popover says: the sync's state (its words and, while it runs, its bar and numbers),
/// the drive's usage with a meter and a warning near full, this computer's health, the
/// transfers running, and what can be done now.
fn popover_content(s: &DriveState, app: &RefAny) -> Dom {
    let drive = sync_drive(s);
    let status = drive.as_deref().and_then(|id| sync_view::drive_status(s, id));
    let key = status
        .as_ref()
        .and_then(|text| text.keys().first().map(|key| key.to_string()));
    let pie = sync_pie(key.as_deref(), progress(s, drive.as_deref()));
    let mut panel = Dom::create_div()
        .with_id(ids::SYNC_POPOVER)
        .with_css(look::SYNC_POPOVER);
    panel.add_child(line(&sync_words(s, drive.as_deref(), &pie), look::POPOVER_HEAD));
    let detail = match key.as_deref() {
        Some("azdrive-sync-status-up-to-date") => Some(t("azdrive-sync-detail-up-to-date")),
        Some("azdrive-sync-status-syncing" | "azdrive-sync-status-syncing-files") => {
            Some(t("azdrive-sync-detail-syncing"))
        }
        Some(sync_view::METERED_STATUS) => Some(t_args(
            "azdrive-sync-metered-note",
            &[("mb", Arg::from(25u64))],
        )),
        _ => None,
    };
    if let Some(detail) = detail {
        panel.add_child(line(&detail, look::POPOVER_TEXT));
    }
    if pie.breathing {
        panel.add_child(
            Dom::create_div()
                .with_css("margin: 6px 0px 2px 0px;")
                .with_child(ProgressBar::create(pie.fraction * 100.0).dom()),
        );
    }
    // The transfers that run or wait (copies, moves, uploads, downloads).
    let transfers = t_text(&s.queue.status_text());
    if !transfers.is_empty() {
        panel.add_child(line(&transfers, look::POPOVER_SMALL));
    }
    let failed = s.queue.failed().len();
    if failed > 0 {
        panel.add_child(line(
            &t_args("azdrive-status-transfers-failed", &[("count", Arg::from(failed))]),
            look::POPOVER_WARNING,
        ));
    }
    if let Some(problem) = crate::problems::status_of(s) {
        panel.add_child(line(&t_text(&problem), look::POPOVER_WARNING));
    }
    if let Some(space) = space_drive(s) {
        panel.add_child(line(&space_words(s, &space), look::POPOVER_DRIVE));
        if let Some((used, total)) = space_of(s, &space) {
            let pie = space_pie(used, total);
            panel.add_child(
                Dom::create_div()
                    .with_css("margin: 4px 0px 2px 0px;")
                    .with_child(ProgressBar::create(pie.fraction * 100.0).dom()),
            );
            if pie.tone != PieTone::Accent {
                let said = if pie.tone == PieTone::Bad {
                    "azdrive-pie-space-full"
                } else {
                    "azdrive-pie-space-nearly-full"
                };
                panel.add_child(line(&t(said), look::POPOVER_WARNING));
            }
        }
    }
    if let Some(device) = s.sync_view.device.as_ref() {
        panel.add_child(line(&crate::health::line(device), look::POPOVER_SMALL));
    }
    let mut buttons = Dom::create_div().with_css(look::POPOVER_ACTIONS);
    if s.queue.running().is_some() || s.queue.waiting() > 0 || failed > 0 {
        buttons.add_child(act(app, "azdrive-pie-show-transfers", Action::ShowTransfers, false));
    }
    if drive.is_some() {
        let paused = drive
            .as_deref()
            .and_then(|id| sync_view::setup_of(s, id))
            .is_some_and(|setup| setup.paused);
        buttons.add_child(act(
            app,
            if paused {
                "azdrive-sync-resume"
            } else {
                "azdrive-sync-pause"
            },
            Action::Sync(SyncAction::Pause),
            paused,
        ));
        if !paused {
            buttons.add_child(act(app, "azdrive-sync-menu-now", Action::Sync(SyncAction::Now), true));
        }
    }
    panel.add_child(buttons);
    panel
}

/// A popover button: the popover closes (it holds the button), then the action runs.
extern "C" fn on_popover_action(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, action)) = data
        .downcast_ref::<ActionRef>()
        .map(|r| (r.app.clone(), r.action.clone()))
    else {
        return Update::DoNothing;
    };
    let hit = info.get_hit_node();
    let _ = Dialog::close_from(info, hit, "");
    println!("AZDRIVE_PIE_ACTION {action:?}");
    with_state(&mut app, &mut info, |info, app, s| actions::run_action(info, app, s, action))
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
