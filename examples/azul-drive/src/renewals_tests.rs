//! A drive's next months as data: the Renew order and a change of tier, the reminders, what a
//! full drive offers, the price notices and the waitlist. No window.

use azcloud_kit::{
    azlin_contract::{
        Currency, DeadlineKind, Months, Prepay, Rfc3339, ScheduledChange, TierId, TierPriceWire,
    },
    Reminder, Tier, Tiers,
};
use azul_appkit::l10n::Arg;
use azul_storage::testing::TempDir;

use crate::{
    add_drive::{AddDialog, TiersState},
    renewals::{price_notices, reminder_text, space_offer, RenewFor, RenewOrder, Waitlist},
};

fn tier(id: &str, gb: u64, month: u64, year: u64) -> Tier {
    Tier {
        id: id.to_string(),
        quota_bytes: gb * 1_000_000_000,
        price_cents_month: Some(month),
        price_cents_year: Some(year),
        currency: "EUR".to_string(),
        first_month_free: true,
        yearly_only: false,
        prepay: Prepay::new(Months::ALL.to_vec()).expect("every period"),
        methods: Vec::new(),
    }
}

fn tiers() -> Tiers {
    Tiers {
        tiers: vec![tier("100GB", 100, 99, 990), tier("1TB", 1000, 499, 4990)],
        methods: vec!["sepa".to_string()],
        withdrawal_consent: None,
    }
}

/// R1: a single prepaid month is no "a month" subscription: the button says what is paid.
#[test]
fn the_buy_button_says_the_months_paid_at_once() {
    crate::l10n::in_english();
    let mut dialog = AddDialog::new(1);
    dialog.country = String::from("DE");
    dialog.choose_buy();
    dialog.tiers = TiersState::Loaded(tiers());
    assert_eq!(dialog.buy_label(), "Buy 100 GB - EUR 0.99 for 1 month");
    dialog.yearly = true;
    dialog.tier = 1;
    assert_eq!(dialog.buy_label(), "Buy 1 TB - EUR 49.90 for 12 months");
}

/// AD-02 and decision 5: Renew opens Buy storage for the drive, on its tier; the same tier is
/// its next months (under its price lock), another one a change of tier - an upgrade when it is
/// bigger.
#[test]
fn renew_orders_the_drives_next_months_and_another_tier_is_a_change() {
    crate::l10n::in_english();
    let mut dialog = AddDialog::renewing(
        2,
        RenewFor {
            drive_id: String::from("d_1"),
            name: String::from("Photos"),
            tier: String::from("1TB"),
            quota_bytes: 1_000_000_000_000,
        },
    );
    dialog.country = String::from("DE");
    dialog.tiers_loaded(tiers());
    assert_eq!(dialog.chosen_tier().map(|t| t.id.as_str()), Some("1TB"), "its own tier");
    assert_eq!(
        dialog.renew_order(),
        Some(RenewOrder {
            drive_id: String::from("d_1"),
            tier: String::from("1TB"),
            change_tier: false,
            upgrade: false,
        })
    );
    assert_eq!(dialog.buy_label(), "Renew 1 TB - EUR 4.99 for 1 month");
    dialog.tier = 0;
    let smaller = dialog.renew_order().unwrap();
    assert!(smaller.change_tier && !smaller.upgrade, "{smaller:?}");
    assert_eq!(dialog.buy_label(), "Move to 100 GB - EUR 0.99 for 1 month");
    // A new drive's dialog orders no renewal.
    let mut new = AddDialog::new(3);
    new.tiers = TiersState::Loaded(tiers());
    assert_eq!(new.renew_order(), None);
}

/// AD-02: the reminder says what ends, when, and what to do.
#[test]
fn the_reminders_say_what_ends_and_when() {
    crate::l10n::in_english();
    let at = Rfc3339::parse("2026-11-21T00:00:00Z").unwrap().unix() as u64;
    let said = |kind, days| reminder_text("Photos", &Reminder { kind, at, days_before: days });
    let end = said(DeadlineKind::PeriodEnd, 30);
    assert_eq!(end.key, "azdrive-renew-period-end");
    assert_eq!(end.get("drive"), Some(&Arg::from("Photos")));
    assert_eq!(end.get("at"), Some(&Arg::from("2026-11-21")));
    assert_eq!(said(DeadlineKind::ReadOnlyStart, 7).key, "azdrive-renew-read-only");
    assert_eq!(said(DeadlineKind::ReadOnlyEnd, 1).key, "azdrive-renew-closing");
    let english = crate::l10n::EN;
    assert!(english.contains("azdrive-renew-period-end = "));
    assert!(english.contains("turns read-only"));
    // A lapsed drive is closed and reviewed, still readable, not deleted by the deadline.
    assert!(english.contains("is closed on"));
    assert!(english.contains("stays readable after that"));
}

/// Q2: a full drive offers what exists: a bigger tier only when one is sold.
#[test]
fn a_full_drive_offers_a_bigger_tier_only_when_one_is_sold() {
    let sold = tiers();
    assert_eq!(space_offer(100_000_000_000, Some(&sold)), Some(String::from("1TB")));
    assert_eq!(space_offer(1_000_000_000_000, Some(&sold)), None, "the biggest");
    assert_eq!(space_offer(100_000_000_000, None), None, "the tiers are not known");
}

fn change(effective: &str, month: Option<u64>) -> ScheduledChange {
    let mut prices = std::collections::BTreeMap::new();
    let price = month.map(|m| {
        let mut by_currency = std::collections::BTreeMap::new();
        by_currency.insert(
            Currency::parse("EUR").unwrap(),
            TierPriceWire {
                month_cents: m,
                year_cents: m * 10,
            },
        );
        by_currency
    });
    prices.insert(TierId::parse("100GB").unwrap(), price);
    ScheduledChange {
        id: 4,
        from_version: 7,
        to_version: 8,
        announced_at: Rfc3339::parse("2026-10-01T00:00:00Z").unwrap(),
        effective_at: Rfc3339::parse(effective).unwrap(),
        prices,
        note: String::new(),
    }
}

/// PRICES.md section 7: a price change of the drive's tier is a notice, saved as a document in
/// the drive (the durable medium); another tier's is none.
#[test]
fn a_price_change_of_the_drives_tier_is_a_notice_with_its_document() {
    crate::l10n::in_english();
    let changes = vec![change("2026-12-01T00:00:00Z", Some(129))];
    let notices = price_notices(&changes, "100GB", Some(7));
    assert_eq!(notices.len(), 1);
    let notice = &notices[0];
    assert!(notice.key.starts_with("Azlin notices/"), "{}", notice.key);
    assert!(notice.text.contains("2026-12-01") && notice.text.contains("1.29"), "{}", notice.text);
    assert!(price_notices(&changes, "1TB", Some(7)).is_empty(), "another tier");
    assert!(price_notices(&changes, "100GB", Some(8)).is_empty(), "a lock of the new version");
    let ended = price_notices(&[change("2026-12-01T00:00:00Z", None)], "100GB", Some(7));
    assert!(ended[0].text.contains("ends"), "{}", ended[0].text);
}

/// PRICES.md section 7: a closed tier's "tell me when it is back" is kept on this computer and
/// said once when the tier is sold again.
#[test]
fn a_waitlisted_tier_is_said_once_when_it_is_sold_again() {
    let dir = TempDir::new("azdrive-waitlist");
    let list = Waitlist::new(dir.path());
    list.add("10TB").unwrap();
    assert!(list.back_on_sale(&tiers()).unwrap().is_empty(), "still closed");
    let mut open = tiers();
    open.tiers.push(tier("10TB", 10_000, 2999, 29990));
    assert_eq!(list.back_on_sale(&open).unwrap(), vec![String::from("10TB")]);
    assert!(list.back_on_sale(&open).unwrap().is_empty(), "said once");
}

/// Q2: the full drive's warning names the bigger tier only when one is sold.
#[test]
fn the_full_drive_warning_offers_only_what_exists() {
    let usage = azcloud_kit::usage::Usage {
        used: 100_000_000_000,
        quota: 100_000_000_000,
        original: None,
        estimate: false,
    };
    let offered = crate::usage_view::usage_warning("Photos", &usage, Some("1TB")).unwrap();
    assert_eq!(offered.get("bigger"), Some(&Arg::from("1TB")));
    let none = crate::usage_view::usage_warning("Photos", &usage, None).unwrap();
    assert_eq!(none.get("bigger"), Some(&Arg::from("none")));
    assert!(crate::l10n::EN.contains("[none] ."), "no bigger tier: free space only");
}
