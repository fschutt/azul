//! AzMail's icons: Haiku's (MIT - `icons/haiku/LICENSE`), in HVIF, registered
//! under the Material names AzMail's widgets ask for (`reply`, `inbox`,
//! `settings` ...) in a pack searched first, so they replace those; a name
//! without a Haiku icon stays Material. azul draws each at the size it is
//! shown at (its level of detail and pixel hinting for that size).

use azul::{
    vec::U8VecRef,
    window::{IconMeta, IconProviderHandle},
};

/// The pack AzMail's Haiku icons are registered in.
pub const PACK: &str = "azmail-haiku";

/// (Material name, HVIF bytes).
const ICONS: &[(&str, &[u8])] = &[
    ("account_circle", include_bytes!("../icons/haiku/account_circle.hvif")),
    ("all_inbox", include_bytes!("../icons/haiku/all_inbox.hvif")),
    ("calendar_month", include_bytes!("../icons/haiku/calendar_month.hvif")),
    ("contacts", include_bytes!("../icons/haiku/contacts.hvif")),
    ("delete", include_bytes!("../icons/haiku/delete.hvif")),
    ("description", include_bytes!("../icons/haiku/description.hvif")),
    ("drafts", include_bytes!("../icons/haiku/drafts.hvif")),
    ("drive_file_move", include_bytes!("../icons/haiku/drive_file_move.hvif")),
    ("exports", include_bytes!("../icons/haiku/exports.hvif")),
    ("flag", include_bytes!("../icons/haiku/flag.hvif")),
    ("folder", include_bytes!("../icons/haiku/folder.hvif")),
    ("forward", include_bytes!("../icons/haiku/forward.hvif")),
    ("group", include_bytes!("../icons/haiku/group.hvif")),
    ("inbox", include_bytes!("../icons/haiku/inbox.hvif")),
    ("info", include_bytes!("../icons/haiku/info.hvif")),
    ("keyboard", include_bytes!("../icons/haiku/keyboard.hvif")),
    ("mail", include_bytes!("../icons/haiku/mail.hvif")),
    ("manage_accounts", include_bytes!("../icons/haiku/manage_accounts.hvif")),
    ("mark_email_read", include_bytes!("../icons/haiku/mark_email_read.hvif")),
    ("mark_email_unread", include_bytes!("../icons/haiku/mark_email_unread.hvif")),
    ("message", include_bytes!("../icons/haiku/message.hvif")),
    ("person_add", include_bytes!("../icons/haiku/person_add.hvif")),
    ("picture_as_pdf", include_bytes!("../icons/haiku/picture_as_pdf.hvif")),
    ("print", include_bytes!("../icons/haiku/print.hvif")),
    ("refresh", include_bytes!("../icons/haiku/refresh.hvif")),
    ("reply", include_bytes!("../icons/haiku/reply.hvif")),
    ("reply_all", include_bytes!("../icons/haiku/reply_all.hvif")),
    ("report", include_bytes!("../icons/haiku/report.hvif")),
    ("send", include_bytes!("../icons/haiku/send.hvif")),
    ("settings", include_bytes!("../icons/haiku/settings.hvif")),
    ("sync", include_bytes!("../icons/haiku/sync.hvif")),
    ("task_alt", include_bytes!("../icons/haiku/task_alt.hvif")),
    ("x", include_bytes!("../icons/haiku/x.hvif")),
];

/// Registers every icon in [`PACK`], searched before the Material icons.
pub fn register(provider: &mut IconProviderHandle) {
    for (name, bytes) in ICONS {
        if !provider.register_hvif_icon(PACK, *name, U8VecRef::from(*bytes), IconMeta::create_for_image()) {
            eprintln!("[azmail] the Haiku icon `{name}` did not register");
        }
    }
    provider.set_pack_rank(PACK, 0);
}

#[cfg(test)]
mod tests {
    use super::ICONS;

    #[test]
    fn every_icon_is_an_hvif_file_once() {
        let mut names: Vec<&str> = ICONS.iter().map(|(n, _)| *n).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), ICONS.len(), "a name twice");
        for (name, bytes) in ICONS {
            assert!(bytes.starts_with(b"ncif"), "{name} is no HVIF file");
        }
    }
}
