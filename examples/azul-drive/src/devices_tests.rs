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
