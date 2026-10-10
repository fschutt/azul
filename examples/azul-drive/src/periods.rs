//! The Azlin drives' paid months (AZLINSEC17 F24): a period token kept for a drive buys it a
//! month when its period nears its end. AzDrive looks at every Azlin drive's period at its
//! start, once a day while it runs, an hour after a look that failed, at the moment a period
//! becomes due when that comes before the next daily look, and as soon as a paid checkout's
//! tokens are kept - each look a [`Job::RedeemPeriods`], under the drive's keyring lock with
//! its newest drive token ([`azcloud_kit::redeem_due`]). A drive without kept tokens asks the
//! token server nothing.
//!
//! A look also tells the owner (D42) of every device the drive was given since the last look
//! (`AZDRIVE_NEW_DEVICE <drive id> <member>`, a notification) and of a use of the recovery code:
//! a lockdown that takes the drive in 48 hours unless a device of the owner cancels it.
//!
//! On stdout: `AZDRIVE_PERIOD_REDEEMED <drive id> <count> <until>` for a month bought.
//! `AZDRIVE_PERIOD_CHECK_SECS` (a positive number of seconds) makes the daily look more often,
//! for a test run.

use std::collections::HashMap;

use azcloud_kit::{period::REDEEM_AHEAD_SECS, token::RECOVERY_MEMBER, Look, Redeemed};
use azul::{
    callbacks::{ButtonOnClickCallbackType, TimerCallbackInfo, TimerCallbackReturn},
    notification::Notification,
    prelude::*,
    str::String as AzString,
    task::{Timer, TimerId},
    time::{Duration, SystemTimeDiff},
};
use azul_storage::{
    config::{DriveAuth, DriveEntry, DriveLocation},
    time::iso8601,
};

use crate::{actions::now_secs, browse::Place, ids, jobs::Job, spawn, DriveState};

/// A day: how often a drive's period is looked at while AzDrive runs.
pub(crate) const DAY_SECS: u64 = 86_400;
/// After a look that failed (no answer, a refusal): an hour.
pub(crate) const RETRY_SECS: u64 = 3_600;
/// The longest the timer sleeps between two glances at the schedule.
const TICK_SECS: u64 = 600;

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
    /// A look runs (one job at a time from the timer).
    pub running: bool,
}

impl Default for Schedule {
    fn default() -> Schedule {
        Schedule::new(check_every(
            std::env::var("AZDRIVE_PERIOD_CHECK_SECS").ok().as_deref(),
        ))
    }
}

impl Schedule {
    /// Looks every `every` seconds (a day, or a test run's interval).
    #[must_use]
    pub(crate) fn new(every: u64) -> Schedule {
        Schedule {
            every: every.max(1),
            next: HashMap::new(),
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
    /// if that comes sooner.
    pub(crate) fn looked(&mut self, drive_id: &str, redeemed: &Redeemed, now: u64) {
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
            | Redeemed::NotDue { period_until: None }
            | Redeemed::Extended {
                period_until: None, ..
            } => daily,
        };
        self.next.insert(drive_id.to_string(), next);
    }

    /// `key` was looked at `now` (a cash order asked by the background claims): the next look
    /// is a day later (or the test run's interval).
    pub(crate) fn looked_at(&mut self, key: &str, now: u64) {
        self.next
            .insert(key.to_string(), now.saturating_add(self.every));
    }

    /// Seconds between two glances of the timer at the schedule.
    fn tick(&self) -> u64 {
        self.every.min(TICK_SECS)
    }
}

/// The Azlin drive of `entry`: its id at the token server and that server (the entry's
/// `account_url`, else `fallback`); `None` for every other drive.
pub(crate) fn azlin_drive(
    entry: &DriveEntry,
    fallback: Option<&str>,
) -> Option<(String, String)> {
    let DriveLocation::S3 {
        auth:
            DriveAuth::Azlin {
                drive_id,
                account_url,
            },
        ..
    } = &entry.location
    else {
        return None;
    };
    let url = if account_url.trim().is_empty() {
        fallback?.to_string()
    } else {
        account_url.clone()
    };
    Some((drive_id.clone(), url))
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
    }
    let job = Job::RedeemPeriods {
        keyring: s.keyring.clone(),
        store: s.period_tokens.clone(),
        drives,
    };
    spawn(info, app, s, job);
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
    for (drive_id, look) in results {
        s.redemptions.looked(&drive_id, &look.redeemed, now);
        if let Some(status) = &look.status {
            lockdown_seen(info, s, &drive_id, status.lockdown_pending_until);
            // A synced drive's status line says "Read-only (payment due)" by this word.
            crate::sync_jobs::drive_status_seen(s, &drive_id, status.read_only);
        }
        for member in new_devices(&look.new_members) {
            println!("AZDRIVE_NEW_DEVICE {drive_id} {member}");
            let body = new_device_text(&s.drive_name(&Place::folder(&drive_id, "")), member);
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
    let body = recovery_text(&name, until, now_secs());
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
pub(crate) fn new_device_text(drive: &str, member: &str) -> String {
    format!(
        "A new device was added to \"{drive}\" ({member}). Not you? Lock the drive down in \
         AzDrive: the drive's menu, \"I was hacked\"."
    )
}

/// The notice of a use of `drive`'s recovery code: the lockdown takes the drive at `until`
/// (the 48 hours the token server waits), and how long is left at `now` to cancel it.
#[must_use]
pub(crate) fn recovery_text(drive: &str, until: u64, now: u64) -> String {
    let left = until.saturating_sub(now);
    let hours = left.div_ceil(3_600);
    let left = match hours {
        _ if left < 3_600 => String::from("less than an hour"),
        1 => String::from("an hour"),
        _ => format!("{hours} hours"),
    };
    format!(
        "The recovery code of \"{drive}\" was used to lock it down. In {left} ({}) that device \
         takes the drive and every other device loses it. If that was not you, cancel it in \
         AzDrive now.",
        iso8601(until)
    )
}

/// The bar over the drive in view while a recovery-key lockdown of it is pending, with Cancel.
#[must_use]
pub(crate) fn lockdown_bar(s: &DriveState, app: &RefAny) -> Option<Dom> {
    let drive_id = s.current_drive_id()?;
    let until = *s.pending_lockdowns.get(&drive_id)?;
    let text = format!(
        "A lockdown with the recovery code is pending until {}: then every other device loses \
         this drive. If that was not you, cancel it now.",
        iso8601(until)
    );
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
                Button::create(AzString::from("Cancel lockdown"))
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

extern "C" fn on_cancel_lockdown(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, drive_id)) = data
        .downcast_ref::<LockdownRef>()
        .map(|r| (r.app.clone(), r.drive_id.clone()))
    else {
        return Update::DoNothing;
    };
    let handle = app.clone();
    let Some(mut s) = app.downcast_mut::<DriveState>() else {
        return Update::DoNothing;
    };
    let fallback = s.token.url.clone();
    let token_url = s
        .slot_index(&drive_id)
        .and_then(|index| azlin_drive(&s.slots[index].entry, fallback.as_deref()))
        .map(|(_, url)| url);
    let Some(token_url) = token_url else {
        s.error("The drive's token server is not known: the lockdown cannot be cancelled here.");
        return Update::RefreshDom;
    };
    let job = Job::CancelLockdown {
        keyring: s.keyring.clone(),
        drive_id,
        token_url,
    };
    spawn(&mut info, &handle, &mut *s, job);
    Update::RefreshDom
}

/// A pending lockdown called off (or why not).
pub(crate) fn lockdown_cancelled(s: &mut DriveState, drive_id: &str, result: Result<(), String>) {
    match result {
        Ok(()) => {
            s.pending_lockdowns.remove(drive_id);
            println!("AZDRIVE_LOCKDOWN_CANCELLED {drive_id}");
            s.info(
                "The lockdown with the recovery code was cancelled. If you did not start it, \
                 someone has your recovery code: make a new one.",
            );
        }
        Err(why) => s.error(format!("The lockdown could not be cancelled: {why}")),
    }
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
    TimerCallbackReturn::continue_unchanged()
}
