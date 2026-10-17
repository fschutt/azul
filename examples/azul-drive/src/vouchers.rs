//! Vouchers on an Azlin drive (Options > Drives' "Redeem a voucher"): the code typed in a
//! dialog, redeemed at the drive's token server under the drive's keyring lock with its newest
//! drive token ([`Job::RedeemVoucher`]); the days it added and the period's new end are said.
//! A voucher that buys a new drive is Add drive > Buy storage's "I have a voucher".
//!
//! On stdout: `AZDRIVE_VOUCHER <drive id> <days added>`.

use azul::prelude::*;
use azul_appkit::l10n::{Phrase, Text};
use azul_storage::time::iso8601;

use crate::{jobs::Job, periods, spawn, with_state, DriveState, Popup};

/// Opens the voucher dialog of `drive_id`.
pub(crate) fn open(s: &mut DriveState, drive_id: &str) {
    s.popups_opened += 1;
    s.popup = Some(Popup::Voucher {
        drive_id: drive_id.to_string(),
        code: String::new(),
        error: Text::default(),
        busy: false,
    });
}

/// The dialog's Redeem.
pub(crate) extern "C" fn on_redeem(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |info, app, s| redeem(info, app, s))
}

fn redeem(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState) {
    let Some(Popup::Voucher {
        drive_id,
        code,
        busy: false,
        ..
    }) = s.popup.as_ref()
    else {
        return;
    };
    let (drive_id, code) = (drive_id.clone(), code.trim().to_string());
    let fallback = s.token.url.clone();
    let token_url = s
        .slot_index(&drive_id)
        .and_then(|index| periods::azlin_drive(&s.slots[index].entry, fallback.as_deref()))
        .map(|(_, url)| url);
    let problem = match (&token_url, code.is_empty()) {
        (None, _) => Some("azdrive-no-token-server"),
        (_, true) => Some("azdrive-voucher-type-code"),
        _ => None,
    };
    if let Some(Popup::Voucher { error, busy, .. }) = s.popup.as_mut() {
        match problem {
            Some(why) => {
                *error = Text::key(why);
                return;
            }
            None => {
                *error = Text::default();
                *busy = true;
            }
        }
    }
    let Some(token_url) = token_url else {
        return;
    };
    let job = Job::RedeemVoucher {
        serial: 0,
        token_url,
        code,
        tier: String::new(),
        drive: Some(drive_id),
        keyring: s.keyring.clone(),
    };
    spawn(info, app, s, job);
}

/// What a voucher on `drive_id` did: the dialog closes and the window says the days it added,
/// or the dialog says why not.
pub(crate) fn redeemed(
    s: &mut DriveState,
    drive_id: &str,
    result: Result<(u32, Option<u64>), Text>,
) {
    let open = matches!(&s.popup, Some(Popup::Voucher { drive_id: id, .. }) if id == drive_id);
    match result {
        Ok((days, until)) => {
            println!("AZDRIVE_VOUCHER {drive_id} {days}");
            if open {
                s.popup = None;
            }
            let name = s
                .slot_index(drive_id)
                .map(|index| s.slots[index].entry.name.clone())
                .unwrap_or_else(|| drive_id.to_string());
            let said = match until {
                Some(at) => Phrase::new("azdrive-voucher-added-until").arg("until", iso8601(at)),
                None => Phrase::new("azdrive-voucher-added"),
            };
            s.success(said.arg("days", days).arg("name", name));
        }
        Err(why) => {
            if open {
                if let Some(Popup::Voucher { error, busy, .. }) = s.popup.as_mut() {
                    *error = why;
                    *busy = false;
                }
            } else {
                s.error(Text::key("azdrive-voucher-failed").then(" ").then(why));
            }
        }
    }
}
