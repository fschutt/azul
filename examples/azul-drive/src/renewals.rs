//! A drive's next months (AD-02): Renew orders them for the drive in view - under its price
//! lock, or as period tokens only, or another tier for it (decision 5: a bigger one at once, a
//! smaller one at the period's end when the data fits). The look of each Azlin drive brings
//! the reminders 30, 7 and 1 days before the period ends, before the drive turns read-only and
//! before it is closed (a notification once, the bar over the drive while it lasts), and the
//! tier list's news: a price change of the drive's tier is shown and saved as a document in the
//! drive (the durable medium, PRICES.md section 6), a closed tier someone asked about is said
//! when it is sold again. A monthly plan has its cancel button (section 312k BGB).
//!
//! On stdout: `AZDRIVE_RENEW_REMINDER <drive> <kind> <days>`, `AZDRIVE_TIER_CHANGED <drive>
//! <tier>`, `AZDRIVE_PRICE_NOTICE <drive> <key>`, `AZDRIVE_TIER_BACK <tier>`.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use azcloud_kit::{
    azlin_contract::{Cents, DeadlineKind, ScheduledChange, TierId},
    Reminder, Tiers,
};
use azul::{callbacks::ButtonOnClickCallbackType, prelude::*, str::String as AzString};
use azul_appkit::l10n::{label, t_args, Arg, Phrase};
use azul_storage::time::iso8601;
use serde::{Deserialize, Serialize};

use crate::{
    ids,
    jobs::{Job, NoticeDrive},
    pay_words, spawn, DriveState,
};

/// `price` as the window writes money.
fn show(price: Cents) -> String {
    pay_words::show(&price)
}

/// What this computer paid for a drive's price lock: the months and what they cost, from the
/// paid list ([`azcloud_kit::pending::PaidList`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LockPaid {
    pub months: u32,
    pub price: Cents,
}

/// A drive's price lock, as the dialog knows it. A renewal under a lock pays that ladder
/// version's price in the earlier checkout's currency, or today's when lower (PRICES.md): the
/// price is the token server's, and the app can say it only for the months this computer paid
/// for the lock. Any other number it could show - today's list price - is not what is charged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LockSeen {
    pub paid: Option<LockPaid>,
}

impl LockSeen {
    /// The lock's price for `months`, when this computer paid for the lock for as many months.
    #[must_use]
    pub(crate) fn price_for(&self, months: u32) -> Option<Cents> {
        self.paid
            .as_ref()
            .filter(|paid| paid.months == months)
            .map(|paid| paid.price.clone())
    }
}

/// The drive a Renew dialog is for: its id, its name, its tier now and that tier's quota, and
/// the price lock of that tier (`None`: the drive has none and pays today's price).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RenewFor {
    pub drive_id: String,
    pub name: String,
    pub tier: String,
    pub quota_bytes: u64,
    pub lock: Option<LockSeen>,
    /// Why the drive's vaults could not be read at its last look: whether it has a price lock is
    /// not known then, and the renewal waits (a lock that is not seen is not "no lock", and
    /// today's price is not what a lock charges).
    pub lock_unread: Option<String>,
}

/// What a Renew checkout buys: months for `drive_id`, whose tier is `tier` now - its next
/// months, or (`change_tier`) the chosen tier for it, a bigger one when `upgrade`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RenewOrder {
    pub drive_id: String,
    pub tier: String,
    pub change_tier: bool,
    pub upgrade: bool,
}

/// What a drive's change of tier came to at its last look.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ChangeSeen {
    /// A smaller tier is taken from `from` on (seconds since 1970).
    Waits { from: u64 },
    /// The stored bytes do not fit the smaller tier.
    DoesNotFit { stored: u64, quota: u64 },
    /// Done: the drive is on `tier`.
    Changed { tier: String, credited_days: Option<u32> },
}

/// What the look at one drive saw for its next months.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct RenewalSeen {
    /// The drive's tier and quota as the token server said.
    pub tier: Option<String>,
    pub quota_bytes: Option<u64>,
    /// The reminder due now; `notify`: said for the first time (a notification).
    pub reminder: Option<Reminder>,
    pub notify: bool,
    pub change: Option<ChangeSeen>,
    /// A running monthly plan's checkout: its cancel button.
    pub plan: Option<String>,
    /// The tier and the signed ladder version of the drive's price lock.
    pub lock: Option<(String, Option<u64>)>,
    /// What this computer paid for that lock, when it paid for it (the paid list).
    pub lock_paid: Option<LockPaid>,
    /// Why the drive's vaults were not all read, when they were not: `lock` and `plan` are then
    /// what could be read, not the whole.
    pub lock_unread: Option<String>,
}

/// A reminder's words: what ends at its time for the drive `drive`, and Renew.
#[must_use]
pub(crate) fn reminder_text(drive: &str, reminder: &Reminder) -> Phrase {
    let key = match reminder.kind {
        DeadlineKind::ReadOnlyStart => "azdrive-renew-read-only",
        DeadlineKind::ReadOnlyEnd => "azdrive-renew-closing",
        _ => "azdrive-renew-period-end",
    };
    Phrase::new(key)
        .arg("drive", drive)
        .arg("at", day_of(reminder.at))
        .arg("days", reminder.days_before)
}

/// When the months a renewal bought take effect - what the drive's vault will do with them -,
/// which is what the toast of the claim may say of them (X-DELTA-MONEY2-13).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Takes {
    /// The drive's paid period runs on past the week the vault redeems a month ahead
    /// ([`azcloud_kit::period::REDEEM_AHEAD_SECS`]): each month is redeemed a week before the
    /// period ends.
    AWeekBefore,
    /// The drive's paid period has ended, or ends within that week: the first look at the drive
    /// (ten minutes to six hours after the claim) redeems the first month at once - `due` is true
    /// for an end in the past or within the week -, and each next one a week before the end that
    /// made.
    AtTheNextLook,
    /// Bought to move the drive to its tier: the first look at the drive applies the change - a
    /// bigger tier at once, a smaller one from a week before the period's end, when the data
    /// fits ([`azcloud_kit::period::change_due`]) - and says which.
    MovesTier,
}

impl Takes {
    /// What a renewal bought as `change_tier` for a drive whose next month is `month_due` (its
    /// period has ended or ends within the week, as the last look saw it: [`month_due_at_once`])
    /// does.
    #[must_use]
    pub(crate) fn of(change_tier: bool, month_due: bool) -> Takes {
        if change_tier {
            Takes::MovesTier
        } else if month_due {
            Takes::AtTheNextLook
        } else {
            Takes::AWeekBefore
        }
    }
}

/// What the toast of a claimed renewal reads of the window: the two maps the looks fill and the
/// drive's id at the token server. Borrowed from the state by [`DriveState::period_facts`]; a test
/// builds it from the maps (X-DELTA-INTEGRITY3-20: the state itself is a window's).
pub(crate) struct PeriodFacts<'a> {
    /// The drives the looks found past their paid period (in grace, read-only or locked), by
    /// AzDrive's id or the drive's id at the token server.
    pub archives: &'a HashMap<String, crate::archive::Seen>,
    /// What each look saw of a drive's next months, by AzDrive's id.
    pub renewals: &'a HashMap<String, RenewalSeen>,
    /// The drive's id at the token server, when its slot has one.
    pub azlin_id: Option<String>,
}

impl PeriodFacts<'_> {
    /// Whether the paid period of the drive `drive_id` (AzDrive's id) had ended when it was last
    /// looked at - it is in its grace, read-only or locked: the months bought for it are then
    /// redeemed at its first look, not a week before an end that has passed.
    #[must_use]
    pub(crate) fn period_ended(&self, drive_id: &str) -> bool {
        self.archives.contains_key(drive_id)
            || self
                .azlin_id
                .as_deref()
                .is_some_and(|azlin_id| self.archives.contains_key(azlin_id))
    }
}

/// Whether the first look at the drive `drive_id` redeems a month at once, as the looks last saw
/// the drive at `now` (seconds since 1970): its paid period has ended ([`PeriodFacts::period_ended`]:
/// grace, read-only or locked), or its period's end is within the week the vault redeems a month
/// ahead - the reminder of its end, which a look keeps while the vault holds no month for the
/// drive. A drive no look saw ends no sooner than the sentence says, which is the common case.
#[must_use]
pub(crate) fn month_due_at_once(facts: &PeriodFacts<'_>, drive_id: &str, now: u64) -> bool {
    facts.period_ended(drive_id)
        || facts
            .renewals
            .get(drive_id)
            .and_then(|seen| seen.reminder.as_ref())
            .is_some_and(|reminder| match reminder.kind {
                DeadlineKind::PeriodEnd => {
                    reminder.at <= now.saturating_add(azcloud_kit::period::REDEEM_AHEAD_SECS)
                }
                // a later deadline is named only once the period has ended
                _ => true,
            })
}

/// The toast of a claimed renewal of `months` for the drive `drive`: what was bought and when the
/// drive's vault adds it - never "a week before the paid period ends" for months that are added
/// at the next look.
#[must_use]
pub(crate) fn bought_text(drive: &str, months: u32, takes: Takes) -> Phrase {
    let key = match takes {
        Takes::AWeekBefore => "azdrive-months-renewed",
        Takes::AtTheNextLook => "azdrive-months-renewed-next-look",
        Takes::MovesTier => "azdrive-tier-change-bought",
    };
    Phrase::new(key).arg("months", months).arg("drive", drive)
}

/// A change of tier's words for the drive `drive`.
#[must_use]
pub(crate) fn change_text(drive: &str, change: &ChangeSeen) -> Phrase {
    match change {
        ChangeSeen::Waits { from } => Phrase::new("azdrive-tier-change-waits")
            .arg("drive", drive)
            .arg("at", day_of(*from)),
        ChangeSeen::DoesNotFit { stored, quota } => Phrase::new("azdrive-tier-change-too-full")
            .arg("drive", drive)
            .arg("free", azcloud_kit::usage::size_text(stored.saturating_sub(*quota))),
        ChangeSeen::Changed {
            tier,
            credited_days,
        } => Phrase::new("azdrive-tier-changed")
            .arg("drive", drive)
            .arg("tier", tier.as_str())
            .arg("days", i64::from(credited_days.unwrap_or(0))),
    }
}

/// `YYYY-MM-DD` of `at` (seconds since 1970).
fn day_of(at: u64) -> String {
    iso8601(at).chars().take(10).collect()
}

/// What a full drive of `quota_bytes` can be offered besides freeing space (Q2): the smallest
/// tier on sale that is bigger, when the tier list is known and has one.
#[must_use]
pub(crate) fn space_offer(quota_bytes: u64, tiers: Option<&Tiers>) -> Option<String> {
    tiers?
        .tiers
        .iter()
        .filter(|t| t.quota_bytes > quota_bytes)
        .min_by_key(|t| t.quota_bytes)
        .map(|t| t.id.clone())
}

/// A notice of an announced price change, and the document it is saved as in the drive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PriceNotice {
    pub id: u64,
    /// Where it is saved in the drive (`Azlin notices/2026-12-01 price change 100GB.txt`).
    pub key: String,
    pub text: String,
}

/// The notices of `changes` for a drive of `tier` whose price lock is of the ladder version
/// `version`: the changes from that version that move or end the tier's price. A lock from
/// before the signed ladders has none: its renewals pay today's price, which the dialog shows.
#[must_use]
pub(crate) fn price_notices(
    changes: &[ScheduledChange],
    tier: &str,
    version: Option<u64>,
) -> Vec<PriceNotice> {
    let (Some(version), Ok(tier_id)) = (version, TierId::parse(tier)) else {
        return Vec::new();
    };
    changes
        .iter()
        .filter(|c| c.from_version == version)
        .filter_map(|c| {
            let price = c.prices.get(&tier_id)?;
            let at = c.effective_at.as_str().chars().take(10).collect::<String>();
            let text = match price.as_ref().and_then(|p| p.iter().next()) {
                Some((currency, wire)) => {
                    let monthly = if wire.month_cents > 0 {
                        t_args(
                            "azdrive-pay-a-month",
                            &[(
                                "price",
                                Arg::from(show(Cents {
                                    amount: wire.month_cents,
                                    currency: currency.clone(),
                                })),
                            )],
                        )
                    } else {
                        t_args(
                            "azdrive-pay-a-year",
                            &[(
                                "price",
                                Arg::from(show(Cents {
                                    amount: wire.year_cents,
                                    currency: currency.clone(),
                                })),
                            )],
                        )
                    };
                    t_args(
                        "azdrive-price-notice-new",
                        &[
                            ("tier", Arg::from(tier)),
                            ("at", Arg::from(at.clone())),
                            ("price", Arg::from(monthly)),
                            ("note", Arg::from(c.note.as_str())),
                        ],
                    )
                }
                None => t_args(
                    "azdrive-price-notice-ends",
                    &[
                        ("tier", Arg::from(tier)),
                        ("at", Arg::from(at.clone())),
                        ("note", Arg::from(c.note.as_str())),
                    ],
                ),
            };
            Some(PriceNotice {
                id: c.id,
                key: format!("Azlin notices/{at} price change {tier}.txt"),
                text,
            })
        })
        .collect()
}

/// The price notices a drive was shown (their change ids), so each is shown and saved once:
/// `<dir>/<drive id>.notices.json`.
pub(crate) struct NoticeLog {
    path: PathBuf,
}

#[derive(Default, Serialize, Deserialize)]
struct NoticeFile {
    #[serde(default)]
    shown: Vec<u64>,
}

/// The file the price notices of the drive `drive_id` are kept in, in the folder `dir` of the
/// period tokens: `<dir>/<drive id>.notices.json`, the id's letters, digits, `_` and `-`. The one
/// place the name is spelled; Remove drive asks it (`footprint.rs`).
pub(crate) fn notices_file(dir: &Path, drive_id: &str) -> PathBuf {
    let file: String = drive_id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
        .collect();
    dir.join(format!("{file}.notices.json"))
}

impl NoticeLog {
    pub(crate) fn new(dir: &Path, drive_id: &str) -> NoticeLog {
        NoticeLog {
            path: notices_file(dir, drive_id),
        }
    }

    /// Whether the change `id` is shown for the first time (it is marked shown then).
    pub(crate) fn first_time(&self, id: u64) -> Result<bool, String> {
        let mut file: NoticeFile = match std::fs::read_to_string(&self.path) {
            Ok(text) => serde_json::from_str(&text).map_err(|e| e.to_string())?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => NoticeFile::default(),
            Err(e) => return Err(e.to_string()),
        };
        if file.shown.contains(&id) {
            return Ok(false);
        }
        file.shown.push(id);
        let text = serde_json::to_string(&file).map_err(|e| e.to_string())?;
        // In the folder of the period tokens, so as private as they are; a power cut leaves the old
        // list or the new one, never an empty file that shows every notice again.
        azul_storage::durable::replace(
            &self.path,
            text.as_bytes(),
            azul_storage::durable::Mode::Private,
        )
        .map_err(|e| e.to_string())?;
        Ok(true)
    }
}

/// The closed tiers someone asked to be told about ("Tell me when it is back"), kept on this
/// computer only (D37: Azlin holds no address to tell).
#[derive(Debug, Clone)]
pub(crate) struct Waitlist {
    path: PathBuf,
}

#[derive(Default, Serialize, Deserialize)]
struct WaitlistFile {
    #[serde(default)]
    tiers: Vec<String>,
}

impl Waitlist {
    /// The list in `dir` (`waitlist.json`).
    #[must_use]
    pub(crate) fn new(dir: &Path) -> Waitlist {
        Waitlist {
            path: dir.join("waitlist.json"),
        }
    }

    fn read(&self) -> Result<WaitlistFile, String> {
        match std::fs::read_to_string(&self.path) {
            Ok(text) => serde_json::from_str(&text).map_err(|e| e.to_string()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(WaitlistFile::default()),
            Err(e) => Err(e.to_string()),
        }
    }

    fn write(&self, file: &WaitlistFile) -> Result<(), String> {
        let text = serde_json::to_string(file).map_err(|e| e.to_string())?;
        azul_storage::durable::replace(
            &self.path,
            text.as_bytes(),
            azul_storage::durable::Mode::Private,
        )
        .map_err(|e| e.to_string())
    }

    /// Keeps `tier` until it is sold again.
    pub(crate) fn add(&self, tier: &str) -> Result<(), String> {
        let mut file = self.read()?;
        if !file.tiers.iter().any(|t| t == tier) {
            file.tiers.push(tier.to_string());
            self.write(&file)?;
        }
        Ok(())
    }

    /// The tiers of the list `tiers` sells again: said once, they leave the list.
    pub(crate) fn back_on_sale(&self, tiers: &Tiers) -> Result<Vec<String>, String> {
        let mut file = self.read()?;
        let (back, closed): (Vec<String>, Vec<String>) = file
            .tiers
            .drain(..)
            .partition(|id| tiers.tiers.iter().any(|t| &t.id == id));
        if !back.is_empty() {
            file.tiers = closed;
            self.write(&file)?;
        }
        Ok(back)
    }
}

// ==== The window ====

/// The bar over the drive in view while a reminder or a change of tier is due for it: what
/// ends or waits, Renew, and for a monthly plan its cancel button.
#[must_use]
pub(crate) fn renewal_bar(s: &DriveState, app: &RefAny) -> Option<Dom> {
    let drive_id = s.current_drive_id()?;
    let seen = s.renewals.get(&drive_id)?;
    let name = s.drive_name(&crate::browse::Place::folder(&drive_id, ""));
    let phrase = match (&seen.change, &seen.reminder) {
        (Some(change), _) => change_text(&name, change),
        (None, Some(reminder)) => reminder_text(&name, reminder),
        (None, None) => return None,
    };
    let text = azul_appkit::l10n::t_phrase(&phrase);
    let mut bar = Dom::create_div()
        .with_id(ids::RENEWAL_BAR)
        .with_css(
            "display: flex; flex-direction: row; align-items: center; padding: 8px 12px; \
             background: #E8F1FB; color: #0B2A4A;",
        )
        .with_child(
            Dom::create_div()
                .with_css("flex-grow: 1; margin-right: 12px;")
                .with_child(Dom::create_span_with_text(AzString::from(text))),
        );
    let button = |key: &str, id: AzString, plan: bool| {
        Button::create(label(key))
            .with_on_click(
                RefAny::new(RenewRef {
                    app: app.clone(),
                    drive_id: drive_id.clone(),
                    plan,
                }),
                on_renew as ButtonOnClickCallbackType,
            )
            .dom()
            .with_id(id)
    };
    bar.add_child(button("azdrive-renew", ids::RENEWAL_RENEW, false));
    if seen.plan.is_some() {
        bar.add_child(button("azdrive-plan-cancel", ids::RENEWAL_CANCEL_PLAN, true));
    }
    Some(bar)
}

/// What Renew and Cancel plan carry.
struct RenewRef {
    app: RefAny,
    drive_id: String,
    plan: bool,
}

extern "C" fn on_renew(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, drive_id, plan)) = data
        .downcast_ref::<RenewRef>()
        .map(|r| (r.app.clone(), r.drive_id.clone(), r.plan))
    else {
        return Update::DoNothing;
    };
    let handle = app.clone();
    let Some(mut s) = app.downcast_mut::<DriveState>() else {
        return Update::DoNothing;
    };
    if plan {
        cancel_plan(&mut info, &handle, &mut *s, &drive_id);
    } else {
        open_renew(&mut info, &handle, &mut *s, &drive_id);
    }
    Update::RefreshDom
}

/// Opens Buy storage for the drive `drive_id`'s next months, on its tier.
pub(crate) fn open_renew(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState, drive_id: &str) {
    let seen = s.renewals.get(drive_id).cloned().unwrap_or_default();
    let usage_quota = s.usage.get(drive_id).map(|u| u.usage().quota);
    let renew = RenewFor {
        drive_id: drive_id.to_string(),
        name: s.drive_name(&crate::browse::Place::folder(drive_id, "")),
        tier: seen.tier.unwrap_or_else(|| azcloud_kit::token::DEFAULT_TIER.to_string()),
        quota_bytes: seen.quota_bytes.or(usage_quota).unwrap_or(0),
        lock: seen.lock.as_ref().map(|_| LockSeen {
            paid: seen.lock_paid.clone(),
        }),
        lock_unread: seen.lock_unread.clone(),
    };
    println!("AZDRIVE_RENEW_OPEN {drive_id}");
    crate::add_flow::open_renewing(info, app, s, renew);
}

/// Ends the drive's monthly plan: the token server stops the payments, the months paid stay.
fn cancel_plan(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState, drive_id: &str) {
    let Some(checkout_id) = s.renewals.get(drive_id).and_then(|r| r.plan.clone()) else {
        return;
    };
    let fallback = s.token.url.clone();
    let Some((_, token_url)) = s
        .slots
        .iter()
        .filter_map(|slot| crate::periods::azlin_drive(&slot.entry, fallback.as_deref()))
        .find(|(id, _)| id == drive_id)
    else {
        return;
    };
    let azlin = crate::periods::azlin_drives(s).get(drive_id).cloned();
    let job = Job::CancelPlan {
        token_url,
        store: s.period_tokens.clone(),
        azlin,
        drive_id: drive_id.to_string(),
        checkout_id,
    };
    spawn(info, app, s, job);
}

/// A monthly plan's cancel answered.
pub(crate) fn plan_cancelled(s: &mut DriveState, drive_id: &str, result: Result<(), String>) {
    match result {
        Ok(()) => {
            println!("AZDRIVE_PLAN_CANCELLED {drive_id}");
            if let Some(seen) = s.renewals.get_mut(drive_id) {
                seen.plan = None;
            }
            s.info(Phrase::new("azdrive-plan-cancelled"));
        }
        Err(why) => s.error(Phrase::new("azdrive-plan-not-cancelled").arg("why", why.as_str())),
    }
}

/// What the looks saw of each drive's next months: kept for the bar; a first reminder is a
/// notification; a change of tier done is said; a price change of the drive's tier is shown
/// and saved in the drive once ([`price_notices`]).
pub(crate) fn renewals_seen(
    info: &mut CallbackInfo,
    app: &RefAny,
    s: &mut DriveState,
    seen: Vec<(String, RenewalSeen)>,
) {
    for (drive_id, renewal) in seen {
        let name = s.drive_name(&crate::browse::Place::folder(&drive_id, ""));
        if let (Some(reminder), true) = (&renewal.reminder, renewal.notify) {
            println!(
                "AZDRIVE_RENEW_REMINDER {drive_id} {:?} {}",
                reminder.kind, reminder.days_before
            );
            let body = azul_appkit::l10n::translate_text(info, &reminder_text(&name, reminder).into());
            info.post_notification(
                azul::notification::Notification::create(
                    format!("azdrive-renew-{drive_id}-{}-{}", reminder.at, reminder.days_before),
                    "AzDrive",
                )
                .with_body(body),
            );
        }
        if let Some(change @ ChangeSeen::Changed { tier, .. }) = &renewal.change {
            println!("AZDRIVE_TIER_CHANGED {drive_id} {tier}");
            s.success(change_text(&name, change));
        }
        let notices = match (&s.tier_news, &renewal.lock) {
            (Some(news), Some((tier, version))) => price_notices(&news.changes, tier, *version),
            _ => Vec::new(),
        };
        save_notices(info, app, s, &drive_id, notices);
        s.renewals.insert(drive_id, renewal);
    }
}

/// The drive of the Azlin drive `drive_id` (its id at the token server) a notice is written to: THE
/// ENCRYPTION'S GATE of the slot that holds it, and no other drive - not the bytes below it, not the plain
/// bucket the keys live in (`AutoEncrypted::bucket`), not the Azlin drive's own handle. The type says so
/// (`Job::SaveNotice` takes a [`NoticeDrive`], which a plain drive is not), and a notice is never written in
/// the clear into an encrypted drive's bucket (X-DELTA-AZDRIVE-KEYS-4). `None` for a drive that is not open:
/// nothing is marked as saved then (X-DELTA-STRUCT3-13: the choice was a function of the window's state that
/// no test could call).
pub(crate) fn notice_drive_in(slots: &[crate::Slot], drive_id: &str) -> Option<NoticeDrive> {
    #[cfg(feature = "encryption")]
    {
        slots
            .iter()
            .find(|slot| slot.entry.azlin().is_some_and(|id| id == drive_id))
            .and_then(|slot| slot.auto.clone())
    }
    // No Azlin drive opens without the encryption (`Slot::open` refuses it), so there is none to write to.
    #[cfg(not(feature = "encryption"))]
    {
        let _ = (slots, drive_id);
        None
    }
}

/// Saves each notice not saved before as a document in the drive `drive_id` and shows it.
///
/// A build without the encryption opens no Azlin drive, so `notice_drive_in` has none for it and the rest
/// is never reached (the type of the drive has no value there: the compiler sees the code after it as
/// unreachable, which is the point).
#[cfg_attr(
    not(feature = "encryption"),
    allow(unreachable_code, unused_variables)
)]
fn save_notices(
    info: &mut CallbackInfo,
    app: &RefAny,
    s: &mut DriveState,
    drive_id: &str,
    notices: Vec<PriceNotice>,
) {
    let Some(drive) = notice_drive_in(&s.slots, drive_id) else {
        return;
    };
    let log = NoticeLog::new(s.period_tokens.dir(), drive_id);
    for notice in notices {
        // A notice that cannot be marked is shown at the next look again, never lost.
        if !log.first_time(notice.id).unwrap_or(false) {
            continue;
        }
        println!("AZDRIVE_PRICE_NOTICE {drive_id} {}", notice.key);
        s.warn(azul_appkit::l10n::Text::from(notice.text.clone()));
        let job = Job::SaveNotice {
            drive: drive.clone(),
            key: notice.key,
            text: notice.text,
        };
        spawn(info, app, s, job);
    }
}

/// The tier list the looks read: kept (a full drive's offer, the price notices); the closed
/// tiers someone asked about that are sold again are said.
pub(crate) fn tier_news_seen(info: &mut CallbackInfo, s: &mut DriveState, news: azcloud_kit::TierNews) {
    let back = Waitlist::new(s.period_tokens.dir())
        .back_on_sale(&news.tiers)
        .unwrap_or_default();
    for tier in back {
        println!("AZDRIVE_TIER_BACK {tier}");
        let said = Phrase::new("azdrive-tier-back").arg("tier", tier.as_str());
        let body = azul_appkit::l10n::translate_text(info, &said.clone().into());
        info.post_notification(
            azul::notification::Notification::create(format!("azdrive-tier-back-{tier}"), "AzDrive")
                .with_body(body),
        );
        s.info(said);
    }
    s.tier_news = Some(news);
}

/// "Tell me when it is back" for the closed tier `tier`.
pub(crate) fn wait_for_tier(s: &mut DriveState, tier: &str) {
    match Waitlist::new(s.period_tokens.dir()).add(tier) {
        Ok(()) => {
            println!("AZDRIVE_TIER_WAITLIST {tier}");
            s.info(Phrase::new("azdrive-tier-waitlisted").arg("tier", tier));
        }
        Err(why) => s.error(Phrase::new("azdrive-tier-not-waitlisted").arg("why", why.as_str())),
    }
}
