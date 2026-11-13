//! The Azlin drives' paid months (AZLINSEC17 F24): a period token kept for a drive buys it a
//! month when its period nears its end. AzDrive looks at every Azlin drive's period at its
//! start, once a day while it runs, an hour after a look that failed, at the moment a period
//! becomes due when that comes before the next daily look, and as soon as a paid checkout's
//! tokens are kept - each look a [`Job::RedeemPeriods`], under the drive's keyring lock with
//! its newest drive token ([`azcloud_kit::redeem_due`]). The tokens and the grants they come
//! from are kept with the drive (its vault, encrypted with the drive key) when its key is on
//! this computer, so any device of the drive buys its months; a banned drive buys none.
//!
//! A look also tells the owner (D42) of every device the drive was given since the last look
//! (`AZDRIVE_NEW_DEVICE <drive id> <member>`, a notification) and of a use of the recovery code:
//! a lockdown that takes the drive in 48 hours unless the recovery code cancels it (F12: "the
//! recovery code always wins" - Cancel asks for it).
//!
//! On stdout: `AZDRIVE_PERIOD_REDEEMED <drive id> <count> <until>` for a month bought.
//! `AZDRIVE_PERIOD_CHECK_SECS` (a positive number of seconds) makes the daily look more often,
//! for a test run.

use std::{collections::HashMap, sync::Arc};

use azcloud_kit::{period::REDEEM_AHEAD_SECS, token::RECOVERY_MEMBER, Look, Redeemed};
use azul::{
    callbacks::{ButtonOnClickCallbackType, TimerCallbackInfo, TimerCallbackReturn},
    notification::Notification,
    prelude::*,
    str::String as AzString,
    task::{Timer, TimerId},
    time::{Duration, SystemTimeDiff},
};
use azul_storage::{config::DriveEntry, time::iso8601};

use azul_appkit::l10n::{label, t_args, translate_text, Arg, Phrase, Text};

use crate::{actions::now_secs, browse::Place, ids, jobs::Job, spawn, DriveState};

/// A day: how often a drive's period is looked at while AzDrive runs.
pub(crate) const DAY_SECS: u64 = 86_400;
/// After a look that failed (no answer, a refusal): an hour.
pub(crate) const RETRY_SECS: u64 = 3_600;
/// The longest the timer sleeps between two glances at the schedule.
const TICK_SECS: u64 = 600;
/// How many looks of one drive in a row that could not buy its month ([`Redeemed::Kept`]) are said to
/// the buyer: the look is tried again an hour after a failure, so six are six hours - more than a
/// commute without a connection, within the day a locked seal key or a full disk, which keep every try
/// from sending anything, can be put right (X-DELTA-MONEY7-7). Nothing but this tells the buyer.
pub(crate) const KEPT_SAYS_AFTER: u32 = 6;

/// The interval of the daily look: `AZDRIVE_PERIOD_CHECK_SECS` when it is a positive number of
/// seconds, else a day.
#[must_use]
pub(crate) fn check_every(value: Option<&str>) -> u64 {
    value
        .and_then(|text| text.trim().parse::<u64>().ok())
        .filter(|secs| *secs > 0)
        .unwrap_or(DAY_SECS)
}

/// When each Azlin drive's period is looked at next (seconds since 1970); a drive never looked
/// at is due at once.
#[derive(Debug, Clone)]
pub(crate) struct Schedule {
    every: u64,
    next: HashMap<String, u64>,
    /// How many looks of each drive in a row could not buy its month ([`Redeemed::Kept`]).
    kept: HashMap<String, u32>,
    /// A look runs (one job at a time from the timer).
    pub running: bool,
}

impl Default for Schedule {
    fn default() -> Schedule {
        Schedule::new(look_every(
            cfg!(feature = "test-hooks"),
            std::env::var("AZDRIVE_PERIOD_CHECK_SECS").ok().as_deref(),
        ))
    }
}

/// How often each drive's period is looked at: a day, or - in a test build (`test_build`) only -
/// the seconds a script sets in `$AZDRIVE_PERIOD_CHECK_SECS`; a shipped build does not read it
/// (X-AR5b-24).
#[must_use]
pub(crate) fn look_every(test_build: bool, var: Option<&str>) -> u64 {
    check_every(var.filter(|_| test_build))
}

impl Schedule {
    /// Looks every `every` seconds (a day, or a test run's interval).
    #[must_use]
    pub(crate) fn new(every: u64) -> Schedule {
        Schedule {
            every: every.max(1),
            next: HashMap::new(),
            kept: HashMap::new(),
            running: false,
        }
    }

    /// Of `drives`, those to look at `now`, in their order.
    #[must_use]
    pub(crate) fn due<'a>(
        &self,
        drives: impl IntoIterator<Item = &'a str>,
        now: u64,
    ) -> Vec<String> {
        drives
            .into_iter()
            .filter(|drive_id| self.next.get(*drive_id).is_none_or(|at| *at <= now))
            .map(str::to_string)
            .collect()
    }

    /// What the look at `drive_id` at `now` found: when to look again - a day later, an hour
    /// after a failure, or when the period becomes due ([`REDEEM_AHEAD_SECS`] before its end)
    /// if that comes sooner. Whether the buyer is told now that it keeps failing: the
    /// [`KEPT_SAYS_AFTER`]th look in a row that could not buy the month, once for as long as it lasts -
    /// a look that went well ends the streak.
    pub(crate) fn looked(&mut self, drive_id: &str, redeemed: &Redeemed, now: u64) -> bool {
        let daily = now.saturating_add(self.every);
        let next = match redeemed {
            Redeemed::Kept(_) => now.saturating_add(RETRY_SECS.min(self.every)),
            Redeemed::NotDue {
                period_until: Some(until),
            }
            | Redeemed::Extended {
                period_until: Some(until),
                ..
            } => daily.min(until.saturating_sub(REDEEM_AHEAD_SECS)).max(now + 1),
            Redeemed::Nothing
            | Redeemed::Banned
            | Redeemed::NotDue { period_until: None }
            | Redeemed::Extended {
                period_until: None, ..
            } => daily,
        };
        self.next.insert(drive_id.to_string(), next);
        match redeemed {
            Redeemed::Kept(_) => {
                let tries = self.kept.entry(drive_id.to_string()).or_insert(0);
                *tries = tries.saturating_add(1);
                *tries == KEPT_SAYS_AFTER
            }
            _ => {
                self.kept.remove(drive_id);
                false
            }
        }
    }

    /// The drive `drive_id` was just paid for (claimed): its first look waits `delay` seconds
    /// from `now` ([`azcloud_kit::period::first_look_delay`]), whatever the schedule says - a
    /// drive never looked at is due at once, and a look in the second of the claim would tie the
    /// payer to the drive by timing (ENT27 n50). A later look time already set stays.
    pub(crate) fn look_later(&mut self, drive_id: &str, now: u64, delay: u64) {
        let at = now.saturating_add(delay);
        let slot = self.next.entry(drive_id.to_string()).or_insert(at);
        *slot = (*slot).max(at);
    }

    /// `key` was looked at `now` (a cash order asked by the background claims): the next look
    /// is a day later (or the test run's interval).
    pub(crate) fn looked_at(&mut self, key: &str, now: u64) {
        self.next
            .insert(key.to_string(), now.saturating_add(self.every));
    }

    /// Seconds between two looks (a day, or the test run's interval).
    #[must_use]
    pub(crate) fn every(&self) -> u64 {
        self.every
    }

    /// Seconds between two glances of the timer at the schedule.
    fn tick(&self) -> u64 {
        self.every.min(TICK_SECS)
    }
}

/// `store` as a job reads it through `keyring`: the vault is sealed under a key of the keyring, and
/// a background job's keyring never prompts (the daily look finds the key or waits for the next
/// start; it never asks the user). A store that keeps plain JSON is as it was.
pub(crate) fn store_through(
    store: &azcloud_kit::PeriodTokenStore,
    keyring: &azcloud_kit::SharedKeyring,
) -> azcloud_kit::PeriodTokenStore {
    #[cfg(feature = "encryption")]
    {
        store.through(keyring.clone())
    }
    #[cfg(not(feature = "encryption"))]
    {
        let _ = keyring;
        store.clone()
    }
}

/// The Azlin drive of `entry`: its id at the token server and that server - the one a drive
/// token, a recovery key, a voucher, a period token or a lockdown is SENT to - as the kit's ONE
/// resolver chooses ([`azcloud_kit::azlin_target`], X-DELTA-AZDRIVE-KEYS-9: it was this function's own
/// reading of the entry's `account_url`, a file any process of the user can write). A pair, for the
/// callers that look at the drive's id and the address one after the other. `None` for every other
/// drive, and for one with no server to name.
pub(crate) fn azlin_drive(entry: &DriveEntry, fallback: Option<&str>) -> Option<(String, String)> {
    azcloud_kit::azlin_target(entry, fallback).map(|target| (target.drive_id, target.token_url))
}

/// The open Azlin drives by their id at the token server: their period tokens are kept in
/// their vaults, and a month bought refreshes their read-only credentials.
pub(crate) fn azlin_drives(s: &DriveState) -> HashMap<String, Arc<azcloud_kit::AzlinAccount>> {
    s.slots
        .iter()
        .filter_map(|slot| {
            let drive_id = slot.entry.azlin()?;
            Some((drive_id.to_string(), slot.azlin.clone()?))
        })
        .collect()
}

/// Looks at the periods of the Azlin drives of the source list - `only` that drive (at once,
/// whatever the schedule says: its paid checkout's tokens were just kept), else those the
/// schedule has due (all of them at the start). Nothing runs without one.
pub(crate) fn start_redemptions(
    info: &mut CallbackInfo,
    app: &RefAny,
    s: &mut DriveState,
    only: Option<&str>,
) {
    let fallback = s.token.url.clone();
    let azlin: Vec<(String, String)> = s
        .slots
        .iter()
        .filter_map(|slot| azlin_drive(&slot.entry, fallback.as_deref()))
        .collect();
    let drives: Vec<(String, String)> = match only {
        Some(only) => azlin.into_iter().filter(|(id, _)| id == only).collect(),
        None => {
            let due = s
                .redemptions
                .due(azlin.iter().map(|(id, _)| id.as_str()), now_secs());
            azlin.into_iter().filter(|(id, _)| due.contains(id)).collect()
        }
    };
    if drives.is_empty() {
        return;
    }
    if only.is_none() {
        s.redemptions.running = true;
        // the daily look sends a recovery code's registration the token server did not take
        #[cfg(feature = "encryption")]
        {
            let due: Vec<String> = drives.iter().map(|(id, _)| id.clone()).collect();
            crate::encryption::retry_registrations(info, app, s, Some(&due));
        }
    }
    let open = azlin_drives(s);
    let drives = drives
        .into_iter()
        .map(|(id, url)| {
            let azlin = open.get(&id).cloned();
            (id, url, azlin)
        })
        .collect();
    // The daily look runs by itself and reads quietly; the look after a paid checkout follows
    // the user's purchase.
    let keyring = if only.is_none() {
        s.quiet_keyring.clone()
    } else {
        s.keyring.clone()
    };
    let job = Job::RedeemPeriods {
        store: store_through(&s.period_tokens, &keyring),
        keyring,
        drives,
    };
    spawn(info, app, s, job);
}

/// The drive `drive_id` was just paid for: its first look waits a random ten minutes to six hours
/// (ENT27 n50; [`Schedule::look_later`]).
pub(crate) fn look_after_claim(s: &mut DriveState, drive_id: &str) {
    // The scripts of the E2E builds (`test-hooks`) buy a drive and expect its months at once;
    // a shipped build waits.
    let delay = if cfg!(feature = "test-hooks") {
        0
    } else {
        azcloud_kit::period::first_look_delay()
    };
    s.redemptions.look_later(drive_id, now_secs(), delay);
}

/// What the looks found: the schedule learns when to look again; a month bought is printed
/// (`AZDRIVE_PERIOD_REDEEMED <drive> <count> <until>`), a look that failed is tried again an
/// hour later, quietly.
pub(crate) fn periods_redeemed(
    info: &mut CallbackInfo,
    s: &mut DriveState,
    results: Vec<(String, Look)>,
) {
    s.redemptions.running = false;
    let now = now_secs();
    for (drive_id, mut look) in results {
        let says_kept = s.redemptions.looked(&drive_id, &look.redeemed, now);
        // a month bought: read-only credentials are refreshed before the next write
        if let Some(changed) = look.grant_changed.take() {
            if let Some(azlin) = azlin_drives(s).get(&drive_id) {
                azlin.apply(changed);
            }
        }
        if let Some(status) = &look.status {
            lockdown_seen(info, s, &drive_id, status.lockdown_pending_until);
            // A ban (ban contract v1): the banner and the refusals, or none any more.
            crate::ban::seen(s, &drive_id, status.ban.clone());
            // What the drive takes now, for the window's refusals and the sync's passes.
            crate::access::seen(s, &drive_id, status);
            // Unpaid or locked: the bar with what happens when, the locked page, or back.
            crate::archive::seen(info, s, &drive_id, status);
            // A synced drive's status line says "Read-only (payment due)" by this word: only
            // for a payment, not a recovery's freeze or an operator's.
            crate::sync_jobs::drive_status_seen(s, &drive_id, status.payment_due());
            // Its space: the quota and the stored bytes the token server counts.
            crate::usage_view::status_seen(s, &drive_id, status);
            // Its devices: Options > Drives offers to remove the others.
            crate::devices::seen(s, &drive_id, status);
        }
        for member in new_devices(&look.new_members) {
            println!("AZDRIVE_NEW_DEVICE {drive_id} {member}");
            let said = new_device_text(&s.drive_name(&Place::folder(&drive_id, "")), member);
            let body = translate_text(info, &said.into());
            info.post_notification(
                Notification::create(format!("azdrive-device-{drive_id}-{member}"), "AzDrive")
                    .with_body(body),
            );
        }
        match look.redeemed {
            Redeemed::Extended {
                count,
                period_until,
            } => {
                let until = period_until.map_or_else(|| String::from("-"), iso8601);
                println!("AZDRIVE_PERIOD_REDEEMED {drive_id} {count} {until}");
            }
            Redeemed::Kept(why) => {
                eprintln!("[azdrive] the period of {drive_id} waits for the next try: {why}");
                // the same look failed again and again: the buyer is told, once - a locked seal key or
                // a full disk keeps the month from being asked for, and stderr tells nobody
                // (X-DELTA-MONEY7-7)
                if says_kept {
                    let said = not_bought_text(&s.drive_name(&Place::folder(&drive_id, "")), &why);
                    let body = translate_text(info, &said.into());
                    info.post_notification(
                        Notification::create(format!("azdrive-period-kept-{drive_id}"), "AzDrive")
                            .with_body(body),
                    );
                }
            }
            Redeemed::Banned => {
                println!("AZDRIVE_PERIOD_BANNED {drive_id}");
            }
            Redeemed::Nothing | Redeemed::NotDue { .. } => {}
        }
    }
}

// ==== A pending recovery-key lockdown ====

/// What a look found of `drive_id`'s recovery-key lockdown: a new pending one is said (stdout
/// `AZDRIVE_LOCKDOWN_PENDING <drive> <until>`, a notification - it takes the drive from every
/// other device unless one of them cancels it), one no longer pending is forgotten.
fn lockdown_seen(
    info: &mut CallbackInfo,
    s: &mut DriveState,
    drive_id: &str,
    pending_until: Option<u64>,
) {
    let Some(until) = pending_until else {
        s.pending_lockdowns.remove(drive_id);
        return;
    };
    if s.pending_lockdowns.insert(drive_id.to_string(), until) == Some(until) {
        return;
    }
    println!("AZDRIVE_LOCKDOWN_PENDING {drive_id} {}", iso8601(until));
    let name = s.drive_name(&Place::folder(drive_id, ""));
    let body = translate_text(info, &recovery_text(&name, until, now_secs()).into());
    info.post_notification(
        Notification::create(format!("azdrive-lockdown-{drive_id}"), "AzDrive").with_body(body),
    );
}

/// Of the members a look saw first, the devices to announce: the recovery code's is announced
/// as its lockdown ([`lockdown_seen`]).
#[must_use]
pub(crate) fn new_devices(new_members: &[String]) -> Vec<&str> {
    new_members
        .iter()
        .map(String::as_str)
        .filter(|member| *member != RECOVERY_MEMBER)
        .collect()
}

/// The notice of a device the drive `drive` was given: what it is, and what to do when it was
/// not the owner.
#[must_use]
pub(crate) fn new_device_text(drive: &str, member: &str) -> Phrase {
    Phrase::new("azdrive-new-device")
        .arg("drive", drive)
        .arg("member", member)
}

/// The notice that the next month of `drive` could not be bought [`KEPT_SAYS_AFTER`] looks in a row,
/// and why (X-DELTA-MONEY7-7).
#[must_use]
pub(crate) fn not_bought_text(drive: &str, why: &str) -> Phrase {
    Phrase::new("azdrive-period-not-bought")
        .arg("drive", drive)
        .arg("tries", KEPT_SAYS_AFTER)
        .arg("why", why)
}

/// The notice of a use of `drive`'s recovery code: the lockdown takes the drive at `until`
/// (the 48 hours the token server waits), and how long is left at `now` to cancel it (`$hours`
/// 0 for less than an hour).
#[must_use]
pub(crate) fn recovery_text(drive: &str, until: u64, now: u64) -> Phrase {
    let left = until.saturating_sub(now);
    let hours = if left < 3_600 { 0 } else { left.div_ceil(3_600) };
    Phrase::new("azdrive-recovery-used")
        .arg("drive", drive)
        .arg("hours", hours)
        .arg("at", iso8601(until))
}

/// The bar over the drive in view while a recovery-key lockdown of it is pending, with Cancel.
#[must_use]
pub(crate) fn lockdown_bar(s: &DriveState, app: &RefAny) -> Option<Dom> {
    let drive_id = s.current_drive_id()?;
    let until = *s.pending_lockdowns.get(&drive_id)?;
    let text = t_args("azdrive-lockdown-pending", &[("until", Arg::from(iso8601(until)))]);
    Some(
        Dom::create_div()
            .with_id(ids::LOCKDOWN_BAR)
            .with_css(
                "display: flex; flex-direction: row; align-items: center; padding: 8px 12px; \
                 background: #FFF4CE; color: #3B2E00;",
            )
            .with_child(
                Dom::create_div()
                    .with_css("flex-grow: 1; margin-right: 12px;")
                    .with_child(Dom::create_span_with_text(AzString::from(text))),
            )
            .with_child(
                Button::create(label("azdrive-lockdown-cancel"))
                    .with_on_click(
                        RefAny::new(LockdownRef {
                            app: app.clone(),
                            drive_id,
                        }),
                        on_cancel_lockdown as ButtonOnClickCallbackType,
                    )
                    .dom()
                    .with_id(ids::LOCKDOWN_CANCEL),
            ),
    )
}

/// What Cancel lockdown carries.
struct LockdownRef {
    app: RefAny,
    drive_id: String,
}

/// F12 ("the recovery code always wins"): the cancel asks for the recovery code, whose key
/// signs it ([`crate::encryption::ask_cancel_lockdown`]); this computer's drive token alone
/// cancels nothing.
extern "C" fn on_cancel_lockdown(mut data: RefAny, _info: CallbackInfo) -> Update {
    let Some((mut app, drive_id)) = data
        .downcast_ref::<LockdownRef>()
        .map(|r| (r.app.clone(), r.drive_id.clone()))
    else {
        return Update::DoNothing;
    };
    let Some(mut s) = app.downcast_mut::<DriveState>() else {
        return Update::DoNothing;
    };
    #[cfg(feature = "encryption")]
    crate::encryption::ask_cancel_lockdown(&mut *s, &drive_id);
    #[cfg(not(feature = "encryption"))]
    {
        let _ = drive_id;
        s.error(Text::key("azdrive-lockdown-needs-encryption"));
    }
    Update::RefreshDom
}

/// A pending lockdown called off with the recovery code.
#[cfg(feature = "encryption")]
pub(crate) fn lockdown_cancelled(s: &mut DriveState, drive_id: &str) {
    s.pending_lockdowns.remove(drive_id);
    println!("AZDRIVE_LOCKDOWN_CANCELLED {drive_id}");
    s.info(Text::key("azdrive-lockdown-cancelled"));
}

/// Starts the timer of the looks (from the window's start, after the first look).
pub(crate) fn start_timer(info: &mut CallbackInfo, app: &RefAny, s: &DriveState) {
    let get_time = info.get_system_time_fn();
    let tick_ms = s.redemptions.tick().saturating_mul(1000);
    info.add_timer(
        TimerId::unique(),
        Timer::create(app.clone(), on_period_timer, get_time)
            .with_interval(Duration::System(SystemTimeDiff::from_millis(tick_ms))),
    );
}

extern "C" fn on_period_timer(mut data: RefAny, info: TimerCallbackInfo) -> TimerCallbackReturn {
    let mut callback_info = info.callback_info;
    let app = data.clone();
    let Some(mut s) = data.downcast_mut::<DriveState>() else {
        return TimerCallbackReturn::continue_unchanged();
    };
    if !s.redemptions.running {
        start_redemptions(&mut callback_info, &app, &mut *s, None);
    }
    // A transient storage problem nobody asked about again notifies once its half hour is up.
    crate::problems::notify_due(&mut callback_info, &mut *s);
    // A cash order's daily look (its letter may have arrived).
    crate::cash::look_if_due(&mut callback_info, &app, &mut *s);
    // The next run of the claims for what the last one left with a pace of its own: a receipt
    // awaited, a payment taken back.
    crate::add_flow::claims_if_due(&mut callback_info, &app, &mut s);
    // The trash of the local drives, once a day (the start looked at it once).
    crate::trash::look_if_due(&mut callback_info, &app, &mut s);
    // A banned drive's banner counts its hours down.
    if s.bans.is_empty() {
        TimerCallbackReturn::continue_unchanged()
    } else {
        TimerCallbackReturn::continue_and_refresh_dom()
    }
}
