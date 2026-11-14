//! The counting engine of the automatic steps: which link a report pauses and which account loses its public
//! links. Pure: the caller feeds it events and a day, it returns the steps.
//!
//! Implements planning/architecture.md section 3.9 ("Automatic steps, so the operator rarely has to act first;
//! counting is per link and per account, from reports only, and no content is looked at: a link reported from
//! three different devices **est.** pauses until decided; an account whose links are reported from five
//! different devices within 30 days **est.** loses public (`anyone`) links (`links_disabled`, 5.3)"), section
//! 5.3 (`links_disabled`: "public links paused after reports from five devices in 30 days, until the operator
//! switches them back on") and section 5.12 ("a link reported from three different devices est. pauses until the
//! operator decides; an account reported from five devices within 30 days est. loses public links. No content is
//! looked at") and section 1.4 rule 4 (every table has a limit and refuses at it).
//!
//! WHO FEEDS IT is drift X-DELTA-W1S46A-1 (answered by decision X-D-27: the drop carries a small clear header
//! with the link, the reason and the device pseudonym, and the group's leader counts from it). This ledger does
//! not care who feeds it, and it holds no text, url, key or record: an event is five opaque fields.
//!
//! THE SEMANTICS (drift X-DELTA-W1S46A-16; what the naive model of check R10 recomputes from the whole history):
//! - The ledger has a PRESENT: the highest day any call has named. A window of `w` days ends at the present: an
//!   entry on day `d` counts while `present - d < w` (a report counts on `d..=d+w-1` and not on `d+w`).
//!   Events normally arrive in day order; an event dated earlier than the present is counted against the
//!   present, and one that already left the window counts for nothing.
//! - A key (a link, an account) counts DISTINCT reporters in its window. Reports of one device count once, on
//!   one link or on many.
//! - A step fires at the report that brings the count to the threshold, and then the key is TRIPPED: it holds
//!   no entries and records nothing until `decided` (a link) or `links_enabled` (an account) removes the key.
//!   So a step fires at most once per breach, and a decision restarts the count from zero: reports recorded
//!   before it no longer count, reports recorded after it (on the same day too) count for the next round.
//! - A tripped key stays until the operator acts; `expire` never frees it. With `link_window = None` (the plan's
//!   words: "until decided") a link's entries never leave by time either: a caller that wants them to leave
//!   sets a window (the default is 30 days, answer X-D-27).
//!
//! Limits: at most `max_links` links and `max_accounts` accounts, at most `max_reporters_per_key` reporters
//! per key. A report that would need a slot beyond a limit is refused with `LedgerError::Full` and changes
//! NOTHING; the ledger never evicts a count. Entries that left their window are freed by `expire`.

use core::fmt;
use std::collections::BTreeMap;

use crate::ids::{AccountRef, Day, Days, DeviceRef, LinkRef, ReportId};

/// The numbers of the automatic steps and the limits of the ledger.
///
/// Caller: S46-INT-2 (the leader task builds one from the node configuration; `Config::default()` is the plan's).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Config {
    /// Different devices that pause a link (planning/architecture.md section 3.9: three, **est.**).
    pub link_devices: u32,
    /// The window of the link count in days; `None` = until decided, the plan's words (drift
    /// X-DELTA-W1S46A-2: the plan gives the three no window; decision X-D-27 answers 30 days, which is the
    /// default).
    pub link_window: Option<Days>,
    /// Different devices that disable an account's public links (section 3.9: five, **est.**).
    pub account_devices: u32,
    /// The window of the account count (section 3.9: 30 days).
    pub account_window: Days,
    /// The most links the ledger keeps a count or a pause for.
    pub max_links: usize,
    /// The most accounts the ledger keeps a count or a switch for.
    pub max_accounts: usize,
    /// The most reporters one link or one account holds.
    pub max_reporters_per_key: usize,
}

impl Default for Config {
    /// Three devices for a link, five for an account, both counted over 30 days; 100,000 links and accounts,
    /// 1,024 reporters per key.
    fn default() -> Config {
        Config {
            link_devices: 3,
            link_window: Some(Days::new(30)),
            account_devices: 5,
            account_window: Days::new(30),
            max_links: 100_000,
            max_accounts: 100_000,
            max_reporters_per_key: 1_024,
        }
    }
}

/// One report as the counting sees it: opaque ids and a day, nothing of what the report says. `report` is the
/// caller's own key (for its journal): the count is by device, so a drop that arrives twice counts once.
///
/// Caller: S46-INT-2 (the leader task builds it from the drop's clear header).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReportEvent {
    /// The receipt of the report.
    pub report: ReportId,
    /// The reported link, when the report is about a link.
    pub link: Option<LinkRef>,
    /// The account the report is about.
    pub account: AccountRef,
    /// The device that reported (a pseudonym).
    pub reporter: DeviceRef,
    /// The day the report was counted.
    pub day: Day,
}

/// A step the counting engine takes by itself.
///
/// Caller: S46-INT-2 (the leader task sets the link's `paused` state or the account's `links_disabled` and
/// tells the owner), `statement_of_step` (the owner's statement of reasons).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AutoStep {
    /// Pause the link until the operator decides (section 3.9: "pauses until decided").
    PauseLink {
        /// The link.
        link: LinkRef,
        /// The different devices that reported it.
        devices: u32,
    },
    /// Pause the account's public links until the operator switches them back on (`links_disabled`, 5.3).
    DisableLinks {
        /// The account.
        account: AccountRef,
        /// The different devices that reported its links.
        devices: u32,
    },
}

/// Which limit of the ledger a refused report ran into.
///
/// Caller: S46-INT-2 (it alerts the operator and runs `expire`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Bound {
    /// `max_links` links are tracked.
    Links,
    /// `max_accounts` accounts are tracked.
    Accounts,
    /// The link already holds `max_reporters_per_key` reporters.
    ReportersOfLink,
    /// The account already holds `max_reporters_per_key` reporters.
    ReportersOfAccount,
}

/// Why the ledger refused a report.
///
/// Caller: S46-INT-2.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LedgerError {
    /// A limit is reached: the report was NOT counted and nothing changed (planning/architecture.md section 1.4
    /// rule 4: "At a limit the code refuses"). `expire` frees what left its window; the caller may retry.
    Full(Bound),
}

impl fmt::Display for LedgerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LedgerError::Full(Bound::Links) => {
                f.write_str("the ledger tracks as many links as it may")
            }
            LedgerError::Full(Bound::Accounts) => {
                f.write_str("the ledger tracks as many accounts as it may")
            }
            LedgerError::Full(Bound::ReportersOfLink) => {
                f.write_str("the link holds as many reporters as it may")
            }
            LedgerError::Full(Bound::ReportersOfAccount) => {
                f.write_str("the account holds as many reporters as it may")
            }
        }
    }
}

impl std::error::Error for LedgerError {}

/// What a ledger holds, as numbers.
///
/// Caller: S46-INT-2 (the leader's health line), the `Debug` of the ledger.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stats {
    /// Links with a count or a pause.
    pub links: usize,
    /// Accounts with a count or a switch.
    pub accounts: usize,
    /// Links paused until decided.
    pub paused_links: usize,
    /// Accounts whose public links are disabled until the operator switches them back on.
    pub disabled_accounts: usize,
}

/// The reporters of one key within its window, and whether the key has fired.
#[derive(Default)]
struct Tally {
    /// The key has fired: it holds nothing and records nothing until the operator acts.
    tripped: bool,
    /// For every reporter, the LATEST day it reported (the one that stays in a window longest).
    seen: BTreeMap<DeviceRef, Day>,
}

/// How a count is read: the present, the window, the threshold.
#[derive(Clone, Copy)]
struct View {
    now: Day,
    window: Option<Days>,
    threshold: u32,
}

/// What one table may hold, and which `Bound` names each limit.
#[derive(Clone, Copy)]
struct Caps {
    max_keys: usize,
    max_reporters: usize,
    keys: Bound,
    reporters: Bound,
}

/// Whether an entry of day `day` still counts at `now` in `window` (`None` = it never leaves).
fn counts(now: Day, day: Day, window: Option<Days>) -> bool {
    match window {
        None => true,
        Some(w) => now.within(day, w),
    }
}

/// Whether `reporter` can be written under `key` without passing a limit. Writes nothing.
fn room<K: Ord>(
    table: &BTreeMap<K, Tally>,
    key: &K,
    reporter: &DeviceRef,
    caps: Caps,
) -> Result<(), LedgerError> {
    match table.get(key) {
        None => {
            if table.len() >= caps.max_keys {
                return Err(LedgerError::Full(caps.keys));
            }
            if caps.max_reporters == 0 {
                return Err(LedgerError::Full(caps.reporters));
            }
            Ok(())
        }
        Some(t) if t.tripped || t.seen.contains_key(reporter) => Ok(()),
        Some(t) if t.seen.len() >= caps.max_reporters => Err(LedgerError::Full(caps.reporters)),
        Some(_) => Ok(()),
    }
}

/// Writes one live report under `key` and returns the device count when it brings the key to its threshold
/// (the key is then tripped and emptied). The room was checked by `room`.
fn tally<K: Ord>(
    table: &mut BTreeMap<K, Tally>,
    key: K,
    reporter: DeviceRef,
    day: Day,
    view: View,
) -> Option<u32> {
    let entry = table.entry(key).or_default();
    if entry.tripped {
        return None;
    }
    match entry.seen.get_mut(&reporter) {
        Some(latest) => {
            if day > *latest {
                *latest = day;
            }
        }
        None => {
            entry.seen.insert(reporter, day);
        }
    }
    let mut devices: u32 = 0;
    for d in entry.seen.values() {
        if counts(view.now, *d, view.window) {
            devices = devices.saturating_add(1);
        }
    }
    if devices >= view.threshold {
        entry.tripped = true;
        entry.seen.clear();
        Some(devices)
    } else {
        None
    }
}

/// Frees the entries that left their window and the keys that hold nothing and have not fired.
fn prune<K: Ord>(table: &mut BTreeMap<K, Tally>, now: Day, window: Option<Days>) {
    for entry in table.values_mut() {
        entry.seen.retain(|_, day| counts(now, *day, window));
    }
    table.retain(|_, entry| entry.tripped || !entry.seen.is_empty());
}

/// The counting engine (see the module documentation for its semantics).
///
/// Caller: S46-INT-2 (the leader task of the metadata group feeds it from the report drops' clear headers).
pub struct Ledger {
    cfg: Config,
    /// The highest day any call has named.
    present: Option<Day>,
    links: BTreeMap<LinkRef, Tally>,
    accounts: BTreeMap<AccountRef, Tally>,
}

impl fmt::Debug for Ledger {
    /// Only counts: no id, no day, nothing of any report.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = self.stats();
        f.debug_struct("Ledger")
            .field("links", &s.links)
            .field("accounts", &s.accounts)
            .field("paused_links", &s.paused_links)
            .field("disabled_accounts", &s.disabled_accounts)
            .finish()
    }
}

impl Ledger {
    /// An empty ledger.
    ///
    /// Caller: S46-INT-2.
    pub fn new(cfg: Config) -> Ledger {
        Ledger {
            cfg,
            present: None,
            links: BTreeMap::new(),
            accounts: BTreeMap::new(),
        }
    }

    /// The present once `day` is named: `day` itself unless the ledger has already seen a later one.
    fn present_with(&self, day: Day) -> Day {
        match self.present {
            Some(p) if p > day => p,
            _ => day,
        }
    }

    /// Counts one report and returns the steps it brings (a link's pause first, then the account's switch),
    /// each at most once per breach. `Err(Full)` leaves the ledger unchanged.
    ///
    /// Caller: S46-INT-2.
    pub fn record(&mut self, ev: ReportEvent) -> Result<Vec<AutoStep>, LedgerError> {
        let now = self.present_with(ev.day);
        let link_live = ev.link.is_some() && counts(now, ev.day, self.cfg.link_window);
        let account_live = counts(now, ev.day, Some(self.cfg.account_window));
        // 1. Room for every key that will be written: nothing changes unless all of it fits.
        if let (true, Some(link)) = (link_live, ev.link.as_ref()) {
            let caps = Caps {
                max_keys: self.cfg.max_links,
                max_reporters: self.cfg.max_reporters_per_key,
                keys: Bound::Links,
                reporters: Bound::ReportersOfLink,
            };
            room(&self.links, link, &ev.reporter, caps)?;
        }
        if account_live {
            let caps = Caps {
                max_keys: self.cfg.max_accounts,
                max_reporters: self.cfg.max_reporters_per_key,
                keys: Bound::Accounts,
                reporters: Bound::ReportersOfAccount,
            };
            room(&self.accounts, &ev.account, &ev.reporter, caps)?;
        }
        // 2. Write.
        self.present = Some(now);
        let mut steps = Vec::new();
        if let (true, Some(link)) = (link_live, ev.link) {
            let view = View {
                now,
                window: self.cfg.link_window,
                threshold: self.cfg.link_devices,
            };
            if let Some(devices) = tally(&mut self.links, link, ev.reporter, ev.day, view) {
                steps.push(AutoStep::PauseLink { link, devices });
            }
        }
        if account_live {
            let view = View {
                now,
                window: Some(self.cfg.account_window),
                threshold: self.cfg.account_devices,
            };
            if let Some(devices) = tally(&mut self.accounts, ev.account, ev.reporter, ev.day, view)
            {
                steps.push(AutoStep::DisableLinks {
                    account: ev.account,
                    devices,
                });
            }
        }
        Ok(steps)
    }

    /// The operator decided on `link` on `day`: the link's count restarts (its pause is lifted by the decision's
    /// own effect). Reports recorded before this call no longer count; reports recorded after it count for the
    /// next round, on the decision day too.
    ///
    /// Caller: S46-INT-3 (AzCtl, when the operator records a decision), through S46-INT-2.
    pub fn decided(&mut self, link: &LinkRef, day: Day) {
        self.present = Some(self.present_with(day));
        self.links.remove(link);
    }

    /// The operator switched `account`'s public links back on, on `day`: the account's count restarts.
    ///
    /// Caller: S46-INT-3 (AzCtl: `azctl account links-enable`), through S46-INT-2.
    pub fn links_enabled(&mut self, account: &AccountRef, day: Day) {
        self.present = Some(self.present_with(day));
        self.accounts.remove(account);
    }

    /// Moves the present to `today` and frees the entries that left their window and the keys that hold nothing
    /// (a tripped key stays until the operator acts). Counting never depends on it: it only frees room.
    ///
    /// Caller: S46-INT-2 (the leader task runs it once a day, and after `Full`).
    pub fn expire(&mut self, today: Day) {
        let now = self.present_with(today);
        self.present = Some(now);
        prune(&mut self.links, now, self.cfg.link_window);
        prune(&mut self.accounts, now, Some(self.cfg.account_window));
    }

    /// What the ledger holds, as numbers.
    ///
    /// Caller: S46-INT-2 (the leader's health line).
    pub fn stats(&self) -> Stats {
        Stats {
            links: self.links.len(),
            accounts: self.accounts.len(),
            paused_links: self.links.values().filter(|t| t.tripped).count(),
            disabled_accounts: self.accounts.values().filter(|t| t.tripped).count(),
        }
    }
}
