//! The pages of the old FILE menu as dialogs: Info, Open & Export (.ics), Print, Calendars,
//! Options, About. Each is azul's `Modal` - the engine shows it as an OS window with the system's
//! title bar on a desktop and as an inline sheet (a title row, a close box) over the frame on a
//! phone, the same code on both - holding the page's content (`chrome.rs`, `print_ui.rs`). The
//! location's `...` (a desktop's menu, a phone's action sheet), the ribbon's buttons, the sidebar's
//! "Add a calendar" and Ctrl / Cmd + P open them (`chrome::open_backstage`); the close box and
//! Escape close the dialog again.

use azul::{
    callbacks::ModalOnCloseCallbackType,
    prelude::*,
    widgets::{Modal, ModalState},
};
use azul_appkit::l10n::label;

use crate::{args::BackstagePage, chrome, ids, print_ui, CalState};

/// The dialog's body: as wide as the page needs and no wider than the window, as tall as the page
/// and no taller than a screen, scrolling beyond that.
const DIALOG_BODY: &str = "display: flex; flex-direction: column; width: 100%; max-width: 480px; \
                           min-width: 0px; max-height: 640px; overflow-y: auto;";

/// The dialog of the page that is open, when one is.
pub(crate) fn file_dialog(s: &CalState, app: &RefAny) -> Option<Dom> {
    let page = s.backstage?;
    let content = match page {
        BackstagePage::Info => chrome::info_page(s, app),
        BackstagePage::Open => chrome::open_page(s, app),
        BackstagePage::Print => print_ui::print_page(s, app),
        BackstagePage::Calendars => chrome::calendars_page(s, app),
        BackstagePage::Options => chrome::options_page(s, app),
        BackstagePage::About => chrome::about_page(),
    };
    let body = Dom::create_div()
        .with_id(ids::backstage_page(page.name()))
        .with_css(DIALOG_BODY)
        .with_child(content);
    Some(
        Modal::create(body)
            .with_title(label(page.label()).as_str())
            .with_open(true)
            .with_on_close(app.clone(), on_close as ModalOnCloseCallbackType)
            .dom(),
    )
}

/// The close box or Escape: the page is shut.
extern "C" fn on_close(mut data: RefAny, _info: CallbackInfo, _state: ModalState) -> Update {
    let Some(mut s) = data.downcast_mut::<CalState>() else {
        return Update::DoNothing;
    };
    s.backstage = None;
    Update::RefreshDom
}
