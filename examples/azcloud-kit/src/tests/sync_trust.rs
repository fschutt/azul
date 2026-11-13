//! Who may say "the user confirmed this delete" ([`crate::sync::trust`], SYNC-1): the judge's
//! truth table, cell by cell. The scenarios through a pass are in `sync_session.rs` and
//! `sync_named.rs` (`x_delta_azdrive_sync_1_*`).

use crate::sync::{
    trust::{trust_tombstone, verified_writer, Authenticated, MemberId, Trust},
    Tombstone,
};

fn tombstone(confirmed: bool, device: &str) -> Tombstone {
    Tombstone {
        hash: String::from("h"),
        gen: 3,
        at: 1_791_450_900,
        device: device.to_string(),
        confirmed,
    }
}

fn member(id: &str) -> MemberId {
    MemberId::new(id)
}

/// X-DELTA-AZDRIVE-SYNC-1, the truth table: a confirmation is the user's in ONE cell - the word is
/// set, its writer was verified, and the verified member is one of the account's own devices.
/// RED not run: the old judge was `SyncOptions::trusts(&tombstone.device)`, a string compare, and
/// had no cell for "the writer was not verified"; the cells with a name in the tombstone below
/// were all `true`.
#[test]
fn trust_tombstone_truth_table() {
    // (confirmed, device string in the tombstone, verified writer, own devices) -> trust
    struct Cell {
        confirmed: bool,
        device: &'static str,
        writer: Option<&'static str>,
        own: &'static [&'static str],
        trust: Trust,
    }
    let cells = [
        // The one cell: confirmed, verified, one of the account's own devices.
        Cell {
            confirmed: true,
            device: "anything",
            writer: Some("owner"),
            own: &["owner", "laptop"],
            trust: Trust::Trusted,
        },
        Cell {
            confirmed: true,
            device: "anything",
            writer: Some("laptop"),
            own: &["owner", "laptop"],
            trust: Trust::Trusted,
        },
        // A verified stranger.
        Cell {
            confirmed: true,
            device: "owner",
            writer: Some("anna"),
            own: &["owner", "laptop"],
            trust: Trust::Ask,
        },
        // Verified and one of ours, but the word is not set: nothing was confirmed.
        Cell {
            confirmed: false,
            device: "owner",
            writer: Some("owner"),
            own: &["owner", "laptop"],
            trust: Trust::Ask,
        },
        // Not verified: whatever name the tombstone carries - this device's, an own device's, a
        // member id - is a string its writer chose.
        Cell {
            confirmed: true,
            device: "owner",
            writer: None,
            own: &["owner", "laptop"],
            trust: Trust::Ask,
        },
        Cell {
            confirmed: true,
            device: "this device",
            writer: None,
            own: &["this device"],
            trust: Trust::Ask,
        },
        Cell {
            confirmed: true,
            device: "",
            writer: None,
            own: &[],
            trust: Trust::Ask,
        },
        Cell {
            confirmed: false,
            device: "owner",
            writer: None,
            own: &["owner"],
            trust: Trust::Ask,
        },
        // Verified, confirmed, but the account has no own devices to compare with.
        Cell {
            confirmed: true,
            device: "owner",
            writer: Some("owner"),
            own: &[],
            trust: Trust::Ask,
        },
    ];
    for (i, cell) in cells.iter().enumerate() {
        let own: Vec<MemberId> = cell.own.iter().map(|id| member(id)).collect();
        let writer = cell.writer.map(|id| Authenticated::verified(member(id)));
        let got = trust_tombstone(&tombstone(cell.confirmed, cell.device), writer, &own);
        assert_eq!(
            got, cell.trust,
            "cell {i}: confirmed={} device={:?} writer={:?} own={:?}",
            cell.confirmed, cell.device, cell.writer, cell.own
        );
    }
}

/// The index of this build carries no signature to check: no tombstone has a verified writer, so
/// the production judge is `Ask` for every tombstone, whatever its device string says. When a
/// reader learns to verify one, this test is the one that changes - on purpose.
#[test]
fn x_delta_azdrive_sync_1_no_tombstone_of_this_build_has_a_verified_writer() {
    for device in ["this device", "owner", "mac-home", ""] {
        let t = tombstone(true, device);
        assert!(verified_writer(&t).is_none(), "{device:?}");
        assert_eq!(
            trust_tombstone(&t, verified_writer(&t), &[member(device)]),
            Trust::Ask,
            "{device:?}"
        );
    }
}
