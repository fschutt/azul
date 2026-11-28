//! The location block at the head of the ribbon's File tab (azul's `LocationBar`): "Documents" as
//! the parent line, the shown document's title big and bold under it. The old File menu is gone:
//! its pages (Info, New, Open, Save, Export, Close) are the backstage, which the `...` beside the
//! title opens, and the crumb "Documents" opens its Open page. There is no pen: a document's title
//! is its first heading, so the name is not a field.

use azul::{
    callbacks::{CallbackInfo, LocationOnEventCallbackType, RefAny, Update},
    dom::Dom,
    widgets::{LocationBar, LocationCrumb, LocationEvent, LocationEventKind},
};
use azul_appkit::l10n::t;

use crate::app::{AppState, BackstagePage, Command};

/// The tag of the "Documents" crumb.
const DOCUMENTS: usize = 1;

/// What the location says: the title big, "Documents" as the parent. With nothing open it is the
/// app's own name.
#[must_use]
pub(crate) fn place(st: &AppState) -> String {
    st.doc().map_or_else(|| "AzWriter".to_string(), crate::model::DocumentModel::title)
}

/// The location block of the File tab.
#[must_use]
pub(crate) fn location(app: &RefAny, st: &AppState) -> Dom {
    LocationBar::create(place(st))
        .with_parent(LocationCrumb::create(t("azwriter-location-documents"), DOCUMENTS))
        .with_editable(false)
        .with_more(true)
        .with_on_event(app.clone(), on_location as LocationOnEventCallbackType)
        .dom()
}

/// The block's reports: the crumb opens the list of documents, `...` File's other pages.
extern "C" fn on_location(mut data: RefAny, mut info: CallbackInfo, event: LocationEvent) -> Update {
    let cmd = match event.kind {
        LocationEventKind::CrumbClicked if event.tag == DOCUMENTS => Command::OpenBackstage(BackstagePage::Open),
        LocationEventKind::MoreClicked => Command::OpenBackstage(BackstagePage::Info),
        // no pen: nothing is edited here
        _ => return Update::DoNothing,
    };
    crate::commands::run(&mut data, cmd, &mut info)
}

#[cfg(test)]
#[path = "ui_location_test.rs"]
mod ui_location_test;
