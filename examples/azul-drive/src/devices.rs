//! The devices of an Azlin drive (Options > Drives): the drive's members as its last look saw
//! them (the token server's `members` and `you`), each one but this computer with Remove. A
//! removal revokes that device's token families and credentials at once, from this computer
//! ([`azcloud_kit::SharedKeyring::remove_member`], named by a key the outbox keeps).
//!
//! On stdout: `AZDRIVE_DEVICE_REMOVED <drive id> <member>`.

use azul::{callbacks::ButtonOnClickCallbackType, prelude::*, str::String as AzString};
use azul_appkit::l10n::{self, Phrase, Text};

use crate::{jobs::Job, periods, spawn, with_state, DriveState};

/// The members of a drive as its last look saw them, and which one is this computer.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct DriveDevices {
    pub members: Vec<String>,
    /// This computer's member; `None` when the token server did not say (nothing is offered
    /// then: it could be this one).
    pub you: Option<String>,
}

/// Whether a member name is a device someone may remove.
pub(crate) enum Removable {
    Yes,
    /// One of the token server's own names (a recovery hand-over waiting).
    No,
}

impl Removable {
    pub(crate) fn of(member: &str) -> Removable {
        if member == azcloud_kit::token::RECOVERY_MEMBER {
            Removable::No
        } else {
            Removable::Yes
        }
    }
}

impl DriveDevices {
    /// The devices the row offers to remove: every other one, none while this computer's own
    /// member is not known.
    pub(crate) fn removable(&self) -> Vec<&str> {
        let Some(you) = self.you.as_deref() else {
            return Vec::new();
        };
        self.members
            .iter()
            .map(String::as_str)
            .filter(|m| *m != you && matches!(Removable::of(m), Removable::Yes))
            .collect()
    }

    /// `member` was removed: it leaves the row.
    pub(crate) fn removed(&mut self, member: &str) {
        self.members.retain(|m| m != member);
    }
}

/// What a look said of `drive_id`'s members.
pub(crate) fn seen(s: &mut DriveState, drive_id: &str, status: &azcloud_kit::DriveStatus) {
    if status.members.is_empty() {
        return;
    }
    s.devices.insert(
        drive_id.to_string(),
        DriveDevices {
            members: status.members.clone(),
            you: status.you.clone(),
        },
    );
}

/// The words of a removal: the device and the drive.
pub(crate) fn removed_text(drive_name: &str, member: &str) -> Phrase {
    Phrase::new("azdrive-device-removed")
        .arg("device", member)
        .arg("name", drive_name)
}

/// The lines under `drive_id`'s row in Options > Drives: each device it may remove, with Remove.
pub(crate) fn rows(s: &DriveState, app: &RefAny, drive_id: &str) -> Vec<Dom> {
    let Some(devices) = s.devices.get(drive_id) else {
        return Vec::new();
    };
    devices
        .removable()
        .into_iter()
        .map(|member| {
            Dom::create_div()
                .with_css(
                    "display: flex; flex-direction: row; align-items: center; \
                     padding: 2px 0px 2px 28px;",
                )
                .with_child(
                    Dom::create_span_with_text(AzString::from(l10n::t_args(
                        "azdrive-options-device",
                        &[("device", l10n::Arg::from(member))],
                    )))
                    .with_css("flex-grow: 1; font-size: 12px;"),
                )
                .with_child(
                    Button::create(l10n::label("azdrive-options-remove-device"))
                        .with_on_click(
                            RefAny::new(DeviceRef {
                                app: app.clone(),
                                drive_id: drive_id.to_string(),
                                member: member.to_string(),
                            }),
                            on_remove as ButtonOnClickCallbackType,
                        )
                        .dom(),
                )
        })
        .collect()
}

struct DeviceRef {
    app: RefAny,
    drive_id: String,
    member: String,
}

extern "C" fn on_remove(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, drive_id, member)) = data
        .downcast_ref::<DeviceRef>()
        .map(|d| (d.app.clone(), d.drive_id.clone(), d.member.clone()))
    else {
        return Update::DoNothing;
    };
    with_state(&mut app, &mut info, |info, app, s| {
        let fallback = s.token.url.clone();
        let token_url = s
            .slot_index(&drive_id)
            .and_then(|index| periods::azlin_drive(&s.slots[index].entry, fallback.as_deref()))
            .map(|(_, url)| url);
        let Some(token_url) = token_url else {
            s.error(Text::key("azdrive-no-token-server"));
            return;
        };
        let job = Job::RemoveDevice {
            token_url,
            drive_id,
            member,
            keyring: s.keyring.clone(),
        };
        spawn(info, app, s, job);
    })
}

/// What the removal of `member` from `drive_id` came to: said, and the device leaves the row.
pub(crate) fn removed(s: &mut DriveState, drive_id: &str, member: &str, result: Result<(), Text>) {
    let name = s
        .slot_index(drive_id)
        .map(|index| s.slots[index].entry.name.clone())
        .unwrap_or_else(|| drive_id.to_string());
    match result {
        Ok(()) => {
            println!("AZDRIVE_DEVICE_REMOVED {drive_id} {member}");
            if let Some(devices) = s.devices.get_mut(drive_id) {
                devices.removed(member);
            }
            s.success(removed_text(&name, member));
        }
        Err(why) => s.error(Text::key("azdrive-device-remove-failed").then(" ").then(why)),
    }
}
