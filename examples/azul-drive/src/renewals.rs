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

use std::path::{Path, PathBuf};

use azcloud_kit::{
    azlin_contract::{DeadlineKind, ScheduledChange, TierId},
    Reminder, Tiers,
};
use azul::{callbacks::ButtonOnClickCallbackType, prelude::*, str::String as AzString};
use azul_appkit::l10n::{self, label, t_args, Arg, Phrase};
use azul_storage::time::iso8601;
use serde::{Deserialize, Serialize};

use crate::{ids, jobs::Job, spawn, DriveState};

/// The drive a Renew dialog is for: its id, its name, its tier now and that tier's quota.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RenewFor {
    pub drive_id: String,
    pub name: String,
    pub tier: String,
    pub quota_bytes: u64,
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
                            &[("price", Arg::from(l10n::money(wire.month_cents, currency.as_str())))],
                        )
                    } else {
                        t_args(
                            "azdrive-pay-a-year",
                            &[("price", Arg::from(l10n::money(wire.year_cents, currency.as_str())))],
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
struct NoticeLog {
    path: PathBuf,
}

#[derive(Default, Serialize, Deserialize)]
struct NoticeFile {
    #[serde(default)]
    shown: Vec<u64>,
}

impl NoticeLog {
    fn new(dir: &Path, drive_id: &str) -> NoticeLog {
        let file: String = drive_id
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
            .collect();
        NoticeLog {
            path: dir.join(format!("{file}.notices.json")),
        }
    }

    /// Whether the change `id` is shown for the first time (it is marked shown then).
    fn first_time(&self, id: u64) -> Result<bool, String> {
        let mut file: NoticeFile = match std::fs::read_to_string(&self.path) {
            Ok(text) => serde_json::from_str(&text).map_err(|e| e.to_string())?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => NoticeFile::default(),
            Err(e) => return Err(e.to_string()),
        };
        if file.shown.contains(&id) {
            return Ok(false);
        }
        file.shown.push(id);
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let text = serde_json::to_string(&file).map_err(|e| e.to_string())?;
        std::fs::write(&self.path, text).map_err(|e| e.to_string())?;
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
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let text = serde_json::to_string(file).map_err(|e| e.to_string())?;
        std::fs::write(&self.path, text).map_err(|e| e.to_string())
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

/// Saves each notice not saved before as a document in the drive `drive_id` and shows it.
fn save_notices(
    info: &mut CallbackInfo,
    app: &RefAny,
    s: &mut DriveState,
    drive_id: &str,
    notices: Vec<PriceNotice>,
) {
    let Some(azlin) = crate::periods::azlin_drives(s).get(drive_id).cloned() else {
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
            azlin: azlin.clone(),
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
