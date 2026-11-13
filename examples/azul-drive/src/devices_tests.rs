//! The devices of an Azlin drive in Options > Drives: each one but this computer with Remove,
//! known from the drive's last look, and gone from the row once the token server removed it.

use crate::devices::{DriveDevices, Removable};

fn devices(members: &[&str], you: Option<&str>) -> DriveDevices {
    DriveDevices {
        members: members.iter().map(|m| m.to_string()).collect(),
        you: you.map(String::from),
    }
}

#[test]
fn the_row_offers_to_remove_the_other_devices_never_this_one() {
    let seen = devices(&["owner", "pickup-1", "pickup-2"], Some("pickup-1"));
    assert_eq!(seen.removable(), vec!["owner", "pickup-2"]);
    // this computer's member unknown (an older token server): nothing is offered, as it could
    // be this one
    let unknown = devices(&["owner", "pickup-1"], None);
    assert!(unknown.removable().is_empty());
    // alone on the drive: nothing to remove
    assert!(devices(&["owner"], Some("owner")).removable().is_empty());
}

#[test]
fn a_removed_device_leaves_the_row() {
    let mut seen = devices(&["owner", "pickup-1", "pickup-2"], Some("owner"));
    seen.removed("pickup-2");
    assert_eq!(seen.removable(), vec!["pickup-1"]);
    seen.removed("not-there");
    assert_eq!(seen.removable(), vec!["pickup-1"]);
}

#[test]
fn a_removal_is_said_with_the_device_and_the_drive() {
    let said = crate::devices::removed_text("Work", "pickup-2");
    assert_eq!(said.key, "azdrive-device-removed");
    assert!(said.get("device").is_some() && said.get("name").is_some(), "{said:?}");
    let Removable::Yes = Removable::of("pickup-2") else {
        panic!("a pick-up can be removed");
    };
    // the server's own names are no device to remove
    assert!(matches!(Removable::of("recovery-pending"), Removable::No));
}

/// AZ-K4: a removal revokes the device's token families and credentials, nothing more: the device
/// keeps the drive's key and what it already copied. The answer used to say "it can no longer open
/// the drive", which is not so; now it says what the removal did not do and where the new keys that
/// do are (the encrypted drive's window asks the question in a dialog of its own, with that
/// button).
#[test]
fn az_k4_the_removal_does_not_say_the_device_can_no_longer_open_the_drive() {
    use azul_appkit::l10n::{keep, set_locale, t_text, Text};

    keep(&crate::l10n::sources());
    for (locale, keeps, new_keys, old_claim) in [
        (
            "en-US",
            "still has the drive's key",
            "new keys",
            "can no longer open the drive",
        ),
        (
            "de-DE",
            "noch den Schlüssel des Laufwerks",
            "neue Schlüssel",
            "nicht mehr öffnen",
        ),
    ] {
        set_locale(locale);
        let said = t_text(&Text::from(crate::devices::removed_text(
            "Work", "pickup-2",
        )));
        assert!(said.contains("pickup-2") && said.contains("Work"), "{said}");
        assert!(
            said.contains(keeps),
            "{locale}: it says the device keeps the key: {said}"
        );
        assert!(
            said.contains(new_keys),
            "{locale}: it names the new keys: {said}"
        );
        assert!(
            !said.contains(old_claim),
            "{locale}: it no longer claims the lock-out: {said}"
        );
    }
    set_locale("en-US");
}
