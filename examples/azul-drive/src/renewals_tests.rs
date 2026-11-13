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
    renewals::{
        notices_file, price_notices, reminder_text, space_offer, NoticeLog, RenewFor, RenewOrder,
        Waitlist,
    },
};

fn tier(id: &str, gb: u64, month: u64, year: u64) -> Tier {
    Tier {
        id: id.to_string(),
        quota_bytes: gb * 1_000_000_000,
        price_month: Some(crate::pay_words_tests::cents(month, "EUR")),
        price_year: Some(crate::pay_words_tests::cents(year, "EUR")),
        currency: crate::pay_words_tests::currency("EUR"),
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
            lock: None,
            lock_unread: None,
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
    // A lapsed drive is locked and its files go to one offline copy; the text promises no
    // time of keeping them.
    assert!(english.contains("is locked on"));
    assert!(english.contains("one offline copy"));
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

/// X-AR5a-6: the notices a drive was shown and the waitlist live in the folder of the period
/// tokens and are as private as it: put in place whole, for this user alone (`fs::write` made them
/// with the default mode, in place - a power cut left an empty file that showed every notice again).
#[cfg(unix)]
#[test]
fn x_ar5a_6_the_notice_log_and_the_waitlist_are_private_and_whole() {
    use std::os::unix::fs::PermissionsExt;
    let dir = TempDir::new("azdrive-renewals-private");
    let tokens = dir.path().join("period-tokens");
    let log = NoticeLog::new(&tokens, "d_k3f9");
    assert!(log.first_time(7).unwrap(), "the first time");
    assert!(!log.first_time(7).unwrap(), "said once");
    assert!(log.first_time(8).unwrap(), "another change");
    let file = notices_file(&tokens, "d_k3f9");
    let list = Waitlist::new(&tokens);
    list.add("10TB").unwrap();
    for path in [file, tokens.join("waitlist.json")] {
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "{}: {mode:o}", path.display());
        assert!(
            serde_json::from_str::<serde_json::Value>(&std::fs::read_to_string(&path).unwrap())
                .is_ok(),
            "{}",
            path.display()
        );
    }
    let folder = std::fs::metadata(&tokens).unwrap().permissions().mode() & 0o777;
    assert_eq!(
        folder, 0o700,
        "the folder made for them is private too: {folder:o}"
    );
    let names: Vec<String> = std::fs::read_dir(&tokens)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names.len(), 2, "no temporary file stays: {names:?}");
}

/// X-DELTA-AZDRIVE-KEYS-4: a price notice was written with the Azlin drive's own handle - below
/// the encryption's gate - as a plaintext object in an encrypted drive's bucket, its name and text
/// the token server's words (and a folder in the clear that made a new drive look like an old
/// one). It is a document of the drive as the window opens it: in the encrypted index, and
/// nothing of it in the bucket.
#[cfg(feature = "encryption")]
#[test]
fn x_delta_azdrive_keys_4_a_price_notice_is_a_document_of_the_encrypted_drive_not_a_plain_object() {
    use std::sync::Arc;

    use azul_storage::{
        config::{DriveId, EncryptionRecord, SlotId},
        crypto::{device, keys::RecoveryKdf},
        keyring::MemoryKeyring,
        meta::MemoryBucket,
        Drive,
    };

    let key = "Azlin notices/2026-12-01 price change 100GB.txt";
    let text = "The price of 100GB changes on 2026-12-01.";
    let bucket = Arc::new(MemoryBucket::new());
    let keyring = Arc::new(MemoryKeyring::new());
    device::setup_new_drive(
        bucket.as_ref(),
        keyring.as_ref(),
        "d_az",
        RecoveryKdf::with_cost(64, 1, 1).unwrap(),
    )
    .unwrap();
    let records = crate::KeyRecords::default();
    let auto = crate::encryption::wrap_with(
        keyring,
        Some(Arc::new(crate::encryption::meta_provider(None))),
        DriveId::named("d_az"),
        bucket.clone(),
        EncryptionRecord::default(),
        SlotId::named("slot-1"),
        &records,
    );

    crate::jobs::save_notice(&auto, key, text).expect("the encrypted drive takes it");
    assert_eq!(
        auto.get(key).unwrap(),
        text.as_bytes(),
        "a document of the drive"
    );
    assert!(
        bucket.get(key).is_err(),
        "no object of that name in the bucket: the gate wrote it into the index"
    );
    for (name, bytes) in bucket.objects() {
        assert!(
            !String::from_utf8_lossy(&bytes).contains("price of 100GB"),
            "{name} shows the notice in the clear"
        );
    }
    assert!(
        !bucket
            .objects()
            .iter()
            .any(|(name, _)| name.starts_with("Azlin notices/")),
        "no folder of notices in the clear"
    );
}

/// X-DELTA-AZDRIVE-KEYS-4: a drive that is not (yet) encrypted refuses the notice at the gate and
/// its bucket is as it was - the write the old job made went to the bucket whatever the state.
#[cfg(feature = "encryption")]
#[test]
fn x_delta_azdrive_keys_4_a_drive_that_is_not_encrypted_takes_no_notice() {
    use std::sync::Arc;

    use azul_storage::{
        config::{DriveId, EncryptionRecord, SlotId},
        keyring::MemoryKeyring,
        meta::MemoryBucket,
    };

    let bucket = Arc::new(MemoryBucket::new());
    let records = crate::KeyRecords::default();
    let auto = crate::encryption::wrap_with(
        Arc::new(MemoryKeyring::new()),
        Some(Arc::new(crate::encryption::meta_provider(None))),
        DriveId::named("d_new"),
        bucket.clone(),
        EncryptionRecord::default(),
        SlotId::named("slot-1"),
        &records,
    );
    let before = bucket.objects();
    assert!(crate::jobs::save_notice(&auto, "Azlin notices/n.txt", "text").is_err());
    assert_eq!(bucket.objects(), before, "nothing was written");
}

/// X-DELTA-INTEGRITY3-20. The toast of a claimed renewal is made of what the vault will do with its
/// months, and what it does depends on two facts about the drive's period as the looks last saw it:
/// whether it has ended (the drive is in its grace, read-only or locked - by AzDrive's id or the
/// drive's id at the token server) and whether its end is within the week the vault redeems a month
/// ahead (the reminder of its end). Nothing asserted either: the helpers were tested, the facts they
/// are asked were not (and `renewed`, which reads them from the window, is gated by `money_gate`).
/// Both facts, the sentence each gives, in both languages.
#[test]
fn x_delta_integrity3_20_the_renewal_toast_follows_the_period_the_looks_saw() {
    use std::collections::HashMap;

    use azul_appkit::l10n::t_phrase;

    use crate::{
        archive::{Seen, Stage},
        renewals::{bought_text, month_due_at_once, PeriodFacts, RenewalSeen, Takes},
    };

    crate::l10n::in_english();
    let now = 1_791_450_000_u64;
    let week = azcloud_kit::period::REDEEM_AHEAD_SECS;
    let in_grace = Seen {
        stage: Stage::Grace {
            period_until: now - 86_400,
            read_only_at: now + 86_400,
            locks_at: now + 2 * 86_400,
        },
    };
    let reminder = |kind: DeadlineKind, at: u64| RenewalSeen {
        reminder: Some(Reminder {
            kind,
            at,
            days_before: 7,
        }),
        ..RenewalSeen::default()
    };
    let said = |archives: &HashMap<String, Seen>,
                renewals: &HashMap<String, RenewalSeen>,
                azlin_id: Option<&str>,
                change_tier: bool| {
        let facts = PeriodFacts {
            archives,
            renewals,
            azlin_id: azlin_id.map(str::to_string),
        };
        let due = month_due_at_once(&facts, "d_1", now);
        let takes = Takes::of(change_tier, due);
        (due, takes, t_phrase(&bought_text("Photos", 12, takes)))
    };
    let no_archives: HashMap<String, Seen> = HashMap::new();
    let no_renewals: HashMap<String, RenewalSeen> = HashMap::new();

    // a drive no look saw ends no sooner than the sentence says: a week before the end
    let (due, takes, text) = said(&no_archives, &no_renewals, Some("azl_1"), false);
    assert!(!due);
    assert_eq!(takes, Takes::AWeekBefore);
    assert!(
        text.contains("a week before the paid period ends"),
        "{text}"
    );

    // its period has ended: found in grace under AzDrive's id, or under its id at the token server
    for (archives, why) in [
        (
            HashMap::from([(String::from("d_1"), in_grace.clone())]),
            "AzDrive's id",
        ),
        (
            HashMap::from([(String::from("azl_1"), in_grace.clone())]),
            "the token server's id",
        ),
    ] {
        let (due, takes, text) = said(&archives, &no_renewals, Some("azl_1"), false);
        assert!(due, "period ended, by {why}");
        assert_eq!(takes, Takes::AtTheNextLook, "{why}");
        assert!(
            text.contains("at the next look at the drive"),
            "{why}: {text}"
        );
        assert!(
            !text.contains("Each month is added a week before"),
            "{why}: {text}"
        );
    }
    // another drive's period ended: this one's did not
    let other = HashMap::from([(String::from("d_2"), in_grace.clone())]);
    assert!(!said(&other, &no_renewals, Some("azl_1"), false).0);
    // the id at the token server does not count without the slot that has it
    let azlin_only = HashMap::from([(String::from("azl_1"), in_grace)]);
    assert!(!said(&azlin_only, &no_renewals, None, false).0);

    // its end is within the week - a day short of it, exactly at it - or further off
    for (at, due) in [
        (now + week - 1, true),
        (now + week, true),
        (now + week + 1, false),
    ] {
        let renewals =
            HashMap::from([(String::from("d_1"), reminder(DeadlineKind::PeriodEnd, at))]);
        let (is_due, takes, text) = said(&no_archives, &renewals, Some("azl_1"), false);
        assert_eq!(is_due, due, "end at now + {}", at - now);
        assert_eq!(
            takes,
            if due {
                Takes::AtTheNextLook
            } else {
                Takes::AWeekBefore
            }
        );
        assert_eq!(
            text.contains("at the next look at the drive"),
            due,
            "end at now + {}: {text}",
            at - now
        );
    }
    // a later deadline (read-only starts, the drive closes) is named only once the period ended
    for kind in [DeadlineKind::ReadOnlyStart, DeadlineKind::ReadOnlyEnd] {
        let renewals = HashMap::from([(String::from("d_1"), reminder(kind, now + 40 * 86_400))]);
        assert!(
            said(&no_archives, &renewals, Some("azl_1"), false).0,
            "{kind:?}"
        );
    }

    // bought as a change of tier: a move, whatever the period says
    for (archives, renewals) in [
        (no_archives.clone(), no_renewals.clone()),
        (
            HashMap::from([(
                String::from("d_1"),
                Seen {
                    stage: Stage::ReadOnly {
                        period_until: now - 86_400,
                        locks_at: now + 86_400,
                    },
                },
            )]),
            no_renewals.clone(),
        ),
    ] {
        let (_, takes, text) = said(&archives, &renewals, Some("azl_1"), true);
        assert_eq!(takes, Takes::MovesTier);
        assert!(text.contains("tier of \"Photos\" is bought"), "{text}");
    }

    // and in German
    azul_appkit::l10n::set_locale("de-DE");
    let archives = HashMap::from([(
        String::from("d_1"),
        Seen {
            stage: Stage::ReadOnly {
                period_until: now - 86_400,
                locks_at: now + 86_400,
            },
        },
    )]);
    let (_, _, german) = said(&archives, &no_renewals, Some("azl_1"), false);
    assert!(german.contains("nächsten Blick"), "{german}");
    let (_, _, german) = said(&no_archives, &no_renewals, Some("azl_1"), false);
    assert!(german.contains("eine Woche vor dem Ende"), "{german}");
    azul_appkit::l10n::set_locale("en-US");
}
