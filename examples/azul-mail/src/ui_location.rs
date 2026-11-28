//! The location block at the head of the ribbon's File tab (azul's `LocationBar`): the mailbox
//! (the account's address, "Local Folders") as the parent line, the shown folder's name big and
//! bold under it. The old File menu is gone: its pages (Info, Print, Help, Options, Exit) are the
//! backstage, which the `...` beside the name opens. There is no pen: a folder is renamed in the
//! server (AzMail receives read-only), so the name is not a field.

use azul::{
    callbacks::LocationOnEventCallbackType,
    prelude::*,
    widgets::{LocationBar, LocationCrumb, LocationEvent, LocationEventKind},
};
use azul_appkit::l10n::t;

use crate::{
    listing,
    ui_main::{self, Action},
    with_app, MailApp,
};

/// What the location says: the mailbox it names (its index, its name) when a mailbox is shown,
/// and the name big under it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Place {
    /// The mailbox: its index in the app's mailboxes and its name.
    pub mailbox: Option<(usize, String)>,
    /// The name: the folder's, the module's, or AzMail's own with nothing to show.
    pub name: String,
}

/// The place the window shows: the folder of the shown mailbox in the mail view, the module's name
/// under the mailbox elsewhere, "AzMail" with no mailbox at all.
#[must_use]
pub(crate) fn place_of(
    module: usize,
    module_name: &str,
    mailbox: Option<(usize, String)>,
    folder: Option<String>,
) -> Place {
    let name = if module != 0 {
        module_name.to_string()
    } else {
        folder
            .or_else(|| mailbox.as_ref().map(|(_, name)| name.clone()))
            .unwrap_or_else(|| String::from("AzMail"))
    };
    // the mailbox is the parent line only while the name is something inside it
    let mailbox = mailbox.filter(|(_, mailbox_name)| *mailbox_name != name);
    Place { mailbox, name }
}

/// The window's place now.
fn place(s: &MailApp) -> Place {
    let mailbox = match (s.current, s.current_account()) {
        (Some(i), Some(account)) => Some((i, account.email.clone())),
        (Some(i), None) if s.shows_local() => Some((i, listing::local_folders())),
        _ => None,
    };
    let modules = ["azmail-module-mail", "azmail-module-calendar", "azmail-module-contacts", "azmail-module-tasks"];
    place_of(
        s.module,
        &t(modules[s.module.min(modules.len() - 1)]),
        mailbox,
        ui_main::current_folder_label(s),
    )
}

/// The location block of the File tab.
pub(crate) fn location(s: &MailApp, app: &RefAny) -> Dom {
    let place = place(s);
    let mut bar = LocationBar::create(place.name.as_str())
        .with_editable(false)
        .with_more(true)
        .with_form(s.form)
        .with_on_event(app.clone(), on_location as LocationOnEventCallbackType);
    if let Some((index, name)) = place.mailbox {
        bar = bar.with_parent(LocationCrumb::create(name.as_str(), index));
    }
    bar.dom()
}

/// The block's reports: the mailbox's crumb shows that mailbox's Inbox, `...` opens File's pages.
extern "C" fn on_location(mut data: RefAny, mut info: CallbackInfo, event: LocationEvent) -> Update {
    match event.kind {
        LocationEventKind::CrumbClicked => with_app(&mut data, |s, _| {
            let mailbox = event.tag;
            let shown = mailbox < s.accounts.len() || (mailbox == s.local_index() && s.local_visible());
            if !shown {
                return Update::DoNothing;
            }
            s.show_account(mailbox);
            s.module = 0;
            Update::RefreshDom
        })
        .unwrap_or(Update::DoNothing),
        // File's old pages, as the File tab opened them: the backstage
        LocationEventKind::MoreClicked => ui_main::run_action(&mut data, &mut info, Action::OpenFile),
        // no pen: nothing is edited here
        _ => Update::DoNothing,
    }
}

#[cfg(test)]
#[path = "ui_location_test.rs"]
mod ui_location_test;
