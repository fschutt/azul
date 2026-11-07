//! When AzDrive looks at an Azlin drive's period: at the start, then once a day while it runs,
//! an hour after a look that failed, and at the moment a period becomes due when that comes
//! before the next daily look. What a look tells the owner: a new device, a recovery-key
//! lockdown and how long is left to cancel it. No window.

use azcloud_kit::{period::REDEEM_AHEAD_SECS, token::RECOVERY_MEMBER, Redeemed};
use azul_appkit::l10n::Arg;
use azul_storage::time::iso8601;

use crate::periods::{
    azlin_drive, azlin_drive_choice, check_every, new_device_text, new_devices, recovery_text,
    Schedule, DAY_SECS, RETRY_SECS,
};

const NOW: u64 = 1_791_450_000;

fn due(schedule: &Schedule, at: u64) -> Vec<String> {
    schedule.due(["d_1", "d_2"], at)
}

#[test]
fn a_drive_is_looked_at_at_the_start_then_once_a_day_and_an_hour_after_a_failed_look() {
    let mut schedule = Schedule::new(DAY_SECS);
    assert_eq!(due(&schedule, NOW), vec!["d_1", "d_2"], "never looked at: now");
    schedule.looked("d_1", &Redeemed::Nothing, NOW);
    schedule.looked("d_2", &Redeemed::Kept(String::from("no answer")), NOW);
    assert!(due(&schedule, NOW + 1).is_empty());
    assert_eq!(due(&schedule, NOW + RETRY_SECS - 1), Vec::<String>::new());
    assert_eq!(due(&schedule, NOW + RETRY_SECS), vec!["d_2"], "a failed look: an hour later");
    assert_eq!(due(&schedule, NOW + DAY_SECS - 1), vec!["d_2"]);
    assert_eq!(due(&schedule, NOW + DAY_SECS), vec!["d_1", "d_2"], "a day later");
}

#[test]
fn a_period_that_becomes_due_before_the_next_daily_look_is_looked_at_when_it_does() {
    let mut schedule = Schedule::new(DAY_SECS);
    // Due (within a week of its end) in two hours: looked at then, not a day later.
    let until = NOW + REDEEM_AHEAD_SECS + 2 * 3600;
    schedule.looked(
        "d_1",
        &Redeemed::NotDue {
            period_until: Some(until),
        },
        NOW,
    );
    assert!(!due(&schedule, NOW + 2 * 3600 - 1).contains(&String::from("d_1")));
    assert!(due(&schedule, NOW + 2 * 3600).contains(&String::from("d_1")));
    // A month bought: the next look a day later (its new end is weeks away).
    schedule.looked(
        "d_1",
        &Redeemed::Extended {
            count: 1,
            period_until: Some(NOW + 32 * DAY_SECS),
        },
        NOW,
    );
    assert!(!due(&schedule, NOW + DAY_SECS - 1).contains(&String::from("d_1")));
    assert!(due(&schedule, NOW + DAY_SECS).contains(&String::from("d_1")));
}

#[test]
fn a_drive_no_longer_in_the_source_list_is_not_looked_at() {
    let mut schedule = Schedule::new(DAY_SECS);
    schedule.looked("d_9", &Redeemed::Nothing, NOW);
    assert_eq!(schedule.due(["d_1"], NOW + 2 * DAY_SECS), vec!["d_1"]);
}

#[test]
fn the_daily_look_can_be_made_more_often_for_a_test_run() {
    assert_eq!(check_every(None), DAY_SECS);
    assert_eq!(check_every(Some("5")), 5);
    assert_eq!(check_every(Some(" 60 ")), 60);
    for wrong in ["", "0", "soon", "-5"] {
        assert_eq!(check_every(Some(wrong)), DAY_SECS, "{wrong:?}");
    }
    // A failed look is tried again within the (shorter) interval too.
    let mut schedule = Schedule::new(5);
    schedule.looked("d_1", &Redeemed::Kept(String::from("busy")), NOW);
    assert_eq!(schedule.due(["d_1"], NOW + 5), vec!["d_1"]);
}

#[test]
fn a_new_device_is_announced_with_the_drive_and_what_to_do_if_it_was_not_you() {
    let said = new_device_text("Work", "m_laptop");
    assert_eq!(said.key, "azdrive-new-device");
    assert_eq!(said.get("drive"), Some(&Arg::from("Work")));
    assert_eq!(said.get("member"), Some(&Arg::from("m_laptop")));
    let english = crate::l10n::EN;
    assert!(english.contains("azdrive-new-device = A new device was added to \"{ $drive }\""));
    assert!(english.contains("Not you?"));
}

#[test]
fn the_recovery_codes_device_is_announced_as_a_lockdown_not_as_a_new_device() {
    let new = [String::from("m_laptop"), String::from(RECOVERY_MEMBER)];
    assert_eq!(new_devices(&new), vec!["m_laptop"]);
}

#[test]
fn a_recovery_lockdown_says_how_long_the_owner_has_to_cancel_it() {
    let until = NOW + 48 * 3_600;
    let said = recovery_text("Work", until, NOW);
    assert_eq!(said.key, "azdrive-recovery-used");
    assert_eq!(said.get("drive"), Some(&Arg::from("Work")));
    assert_eq!(said.get("hours"), Some(&Arg::Int(48)));
    assert_eq!(said.get("at"), Some(&Arg::from(iso8601(until))));
    let soon = recovery_text("Work", NOW + 90 * 60, NOW);
    assert_eq!(soon.get("hours"), Some(&Arg::Int(2)));
    let now = recovery_text("Work", NOW + 600, NOW);
    assert_eq!(now.get("hours"), Some(&Arg::Int(0)), "less than an hour");
}

/// X-AR5b-24: the interval of the period look is a day; a script shortens it with
/// `$AZDRIVE_PERIOD_CHECK_SECS` in a test build only.
#[test]
fn x_ar5b_24_the_period_look_interval_is_read_from_the_environment_in_a_test_build_only() {
    use crate::periods::look_every;
    assert_eq!(look_every(true, Some("3")), 3);
    assert_eq!(
        look_every(false, Some("3")),
        DAY_SECS,
        "a shipped build does not read it"
    );
    assert_eq!(look_every(true, None), DAY_SECS);
    assert_eq!(
        look_every(true, Some("0")),
        DAY_SECS,
        "never a look at no interval"
    );
    assert_eq!(look_every(true, Some("soon")), DAY_SECS);
}

/// X-AR5b-24: the sync's two timing switches of the scripts are read in a test build only.
#[test]
fn x_ar5b_24_the_sync_timing_switches_are_read_in_a_test_build_only() {
    use crate::sync_jobs::switch_in;
    assert_eq!(
        switch_in(true, Some(String::from("2"))).as_deref(),
        Some("2")
    );
    assert_eq!(switch_in(false, Some(String::from("2"))), None);
    assert_eq!(switch_in(true, None), None);
}

/// ENT27 n50: the first look at a drive whose payment was just claimed waits a random ten
/// minutes to six hours, whatever the schedule says (a drive never looked at is due at once).
#[test]
fn ent27_n50_the_first_look_after_a_claim_waits_a_random_while() {
    use azcloud_kit::period::{
        first_look_delay, first_look_delay_from, FIRST_LOOK_MAX_SECS, FIRST_LOOK_MIN_SECS,
    };
    // The spread: the least and the most, and every draw in between.
    assert_eq!(first_look_delay_from(0), FIRST_LOOK_MIN_SECS);
    assert_eq!(
        first_look_delay_from(FIRST_LOOK_MAX_SECS - FIRST_LOOK_MIN_SECS),
        FIRST_LOOK_MAX_SECS
    );
    assert!(first_look_delay_from(u64::MAX) >= FIRST_LOOK_MIN_SECS);
    assert!(first_look_delay_from(u64::MAX) <= FIRST_LOOK_MAX_SECS);
    let draws: Vec<u64> = (0..20).map(|_| first_look_delay()).collect();
    assert!(
        draws
            .iter()
            .all(|d| (FIRST_LOOK_MIN_SECS..=FIRST_LOOK_MAX_SECS).contains(d)),
        "{draws:?}"
    );
    assert!(
        draws.iter().any(|d| *d != draws[0]),
        "not one fixed wait: {draws:?}"
    );

    // The schedule: a drive never looked at is due at once - until its claim.
    let mut schedule = Schedule::new(DAY_SECS);
    let now = 1_000_000;
    assert_eq!(due(&schedule, now), vec!["d_1", "d_2"]);
    schedule.look_later("d_1", now, 700);
    assert_eq!(
        due(&schedule, now),
        vec!["d_2"],
        "not in the second of the claim"
    );
    assert_eq!(due(&schedule, now + 699), vec!["d_2"]);
    assert_eq!(due(&schedule, now + 700), vec!["d_1", "d_2"]);
    // A later look time already set stays: it is never brought forward.
    schedule.look_later("d_1", now, 100);
    assert_eq!(due(&schedule, now + 699), vec!["d_2"]);
}

/// X-AR5b-24 (the wiring): a shipped build (without `test-hooks`) looks at each drive's period at
/// its own pace whatever `$AZDRIVE_PERIOD_CHECK_SECS` says; only a test build reads it. The
/// switch is set for the test: in a build that does not read it nothing else can see it.
#[cfg(not(feature = "test-hooks"))]
#[test]
fn x_ar5b_24_a_shipped_build_looks_at_its_own_pace_whatever_the_environment_says() {
    std::env::set_var("AZDRIVE_PERIOD_CHECK_SECS", "5");
    let every = Schedule::default().every();
    std::env::remove_var("AZDRIVE_PERIOD_CHECK_SECS");
    assert_eq!(every, DAY_SECS);
}

// ==== Where a drive token goes (X-DELTA-AZDRIVE-KEYS-9, AZ-K20) ====

/// The live token server and a hostile one that `drives.json` (any process of the user can write
/// it) or a lying token server's bundle names.
const LIVE: &str = "https://token.azlin.io";
const HOSTILE: &str = "https://evil.example";

fn azlin_entry(account_url: &str) -> azul_storage::config::DriveEntry {
    use azul_storage::config::{DriveAuth, DriveEntry, DriveLocation};
    DriveEntry {
        id: String::from("d_1"),
        name: String::from("Photos"),
        location: DriveLocation::S3 {
            endpoint: String::from("https://s3.example.test"),
            region: String::from("us-east-1"),
            bucket: String::from("d-1"),
            path_style: true,
            auth: DriveAuth::Azlin {
                drive_id: String::from("d_1"),
                account_url: account_url.to_string(),
                encrypted: false,
                key_id: None,
            },
        },
    }
}

/// A SHIPPED build sends the drive token, the period tokens, a recovery key, a voucher and a
/// lockdown to the server the run resolved (the live one), whatever the entry says: the old code
/// took `account_url` of `drives.json` and the next start's daily look sent the drive token there
/// (`Authorization: Bearer ...`), which the attacker spends at the real server.
#[test]
fn a_shipped_build_sends_the_drive_token_to_the_live_server_whatever_the_entry_names() {
    assert_eq!(
        azlin_drive_choice(false, &azlin_entry(HOSTILE), Some(LIVE)),
        Some((String::from("d_1"), String::from(LIVE)))
    );
    // The same for an entry that names the live server, and one that names nothing.
    assert_eq!(
        azlin_drive_choice(false, &azlin_entry(LIVE), Some(LIVE)),
        Some((String::from("d_1"), String::from(LIVE)))
    );
    assert_eq!(
        azlin_drive_choice(false, &azlin_entry(""), Some(LIVE)),
        Some((String::from("d_1"), String::from(LIVE)))
    );
    // A run with no server to name sends nothing - not to the entry's.
    assert_eq!(azlin_drive_choice(false, &azlin_entry(HOSTILE), None), None);
    assert_eq!(
        azlin_drive_choice(false, &azlin_entry(HOSTILE), Some("  ")),
        None
    );
}

/// A TEST build keeps what it had: the entry's server (a local stack's drive), else the run's.
#[test]
fn a_test_build_takes_the_entrys_server_else_the_runs() {
    assert_eq!(
        azlin_drive_choice(true, &azlin_entry("http://127.0.0.1:18081"), Some(LIVE)),
        Some((String::from("d_1"), String::from("http://127.0.0.1:18081")))
    );
    assert_eq!(
        azlin_drive_choice(true, &azlin_entry("  "), Some("http://127.0.0.1:8081")),
        Some((String::from("d_1"), String::from("http://127.0.0.1:8081")))
    );
    assert_eq!(azlin_drive_choice(true, &azlin_entry(""), None), None);
}

/// Only an Azlin drive has a token server.
#[test]
fn a_drive_that_is_not_an_azlin_drive_has_no_token_server() {
    use azul_storage::config::{DriveEntry, DriveLocation};
    let local = DriveEntry {
        id: String::from("home"),
        name: String::from("Home"),
        location: DriveLocation::Local {
            root: String::from("/tmp/home"),
        },
    };
    assert_eq!(azlin_drive_choice(true, &local, Some(LIVE)), None);
    assert_eq!(azlin_drive_choice(false, &local, Some(LIVE)), None);
}

/// The build under test is a test build (appkit's `test-hooks` is a dev-dependency): the choice
/// `azlin_drive` makes is the one the anchors make for it - the shipped branch is the one above.
#[test]
fn azlin_drive_follows_the_anchors_choice_for_its_build() {
    let honoured = azul_appkit::anchors::layers_honoured();
    assert_eq!(
        azlin_drive(&azlin_entry(HOSTILE), Some(LIVE)),
        azlin_drive_choice(honoured, &azlin_entry(HOSTILE), Some(LIVE))
    );
}

// ==== The period vault is sealed (AZ-K13) ====

/// AzDrive keeps its period tokens in a store sealed under a key of the keyring; a background job
/// (the daily look) reads that store through the quiet keyring over the same entries, and what
/// the interactive one wrote is what it reads.
#[cfg(feature = "encryption")]
#[test]
fn a_background_job_reads_the_sealed_period_vault_through_its_own_keyring() {
    use std::sync::Arc;

    use azcloud_kit::{period::PeriodToken, PeriodTokenStore, SharedKeyring};
    use azul_storage::{
        crypto::blob,
        keyring::{KeyringNamespace, KeyringStore, MemoryKeyring},
        testing::TempDir,
    };

    use crate::periods::store_through;

    let dir = TempDir::new("azdrive-periods-seal");
    let memory = Arc::new(MemoryKeyring::new());
    let shared = || {
        SharedKeyring::with_lock_root(memory.clone(), KeyringNamespace::azlin_shared(), dir.path())
    };
    let store = PeriodTokenStore::sealed(dir.path().join("period-tokens"), shared());
    let token = PeriodToken {
        tier: String::from("100GB"),
        year: 2026,
        nonce: "ab".repeat(32),
        signature: String::from("c2lnbmF0dXJl"),
        randomizer: String::from("cmFuZG9taXplcg=="),
    };
    store.add("d_1", &[token.clone()]).unwrap();
    assert!(
        blob::is_sealed(&std::fs::read(store.path_of("d_1").unwrap()).unwrap()),
        "the tokens are in the clear"
    );
    assert!(
        memory
            .get(&azul_storage::footprint::KeyEntry::VaultKey.name("d_1"))
            .unwrap()
            .is_some(),
        "the key is in the keyring"
    );

    let job = store_through(&store, &shared());
    assert!(job.is_sealed(), "a job reads a plain store");
    assert_eq!(job.tokens("d_1").unwrap(), vec![token]);
}
