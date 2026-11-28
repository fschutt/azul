//! The designed empty states: what the window says while the records load, when a list has no
//! records yet (with the way out: a first record, a CSV file, or the sample register to look
//! around), and for a view type AzERP draws no screen for yet. A state is an icon, one line, a
//! line of detail and, where there is a way out, its buttons - never a blank page.

use azul::{
    callbacks::ButtonOnClickCallbackType,
    prelude::*,
    shells::ShellEmptyState,
    widgets::{Button, ButtonType},
};
use azul_appkit::l10n::{t, t_args, Arg};

use super::{mint, run_action, with_erp, Erp};
use crate::{
    ids,
    views::{
        spec::{ActionKind, ActionSpec},
        Params, View, ViewKind,
    },
};

/// A quiet state: an icon (a Material name), one line and a line of detail.
#[must_use]
pub(crate) fn quiet(icon: &str, title: &str, detail: &str) -> Dom {
    ShellEmptyState::create(title)
        .with_icon(icon)
        .with_detail(detail)
        .dom()
        .with_id(ids::EMPTY)
}

/// The window while the records are being read from the data folder (the first frame; the file
/// thread's answer replaces it a moment later).
#[must_use]
pub(crate) fn loading() -> Dom {
    quiet(
        "inventory_2",
        &t("azerp-reading-register"),
        &t("azerp-reading-register-detail"),
    )
}

/// A view type AzERP draws no screen for yet (the ERP's dashboards, boards, calendars, inbox and
/// search results): said as it is, with what the app does draw.
#[must_use]
pub(crate) fn no_screen(view: &View) -> Dom {
    quiet(
        "construction",
        &t("azerp-no-screen"),
        &t_args(
            "azerp-no-screen-detail",
            &[
                ("view", Arg::from(view.id.as_str())),
                ("kind", Arg::from(kind_words(view.kind))),
            ],
        ),
    )
}

/// The ERP's name of a view type, for the line that says it has no screen.
#[must_use]
fn kind_words(kind: ViewKind) -> &'static str {
    match kind {
        ViewKind::Dashboard => "dashboard",
        ViewKind::Kanban => "kanban",
        ViewKind::Calendar => "calendar",
        ViewKind::Inbox => "inbox",
        ViewKind::SearchResults => "search results",
        ViewKind::Unknown => "unknown",
        ViewKind::Table => "table",
        ViewKind::TableEmbedded => "embedded table",
        ViewKind::Form => "form",
        ViewKind::FormModal => "modal form",
        ViewKind::Detail => "detail",
        ViewKind::Wizard => "wizard",
        ViewKind::Report => "report",
        ViewKind::Custom => "custom",
    }
}

/// A list with no record: what is missing and the way out - the page's own New and Import
/// commands, and, while the whole register is empty, "Load sample data" (a register to look
/// around in: the same records `--sample` writes).
#[must_use]
pub(crate) fn empty_list(s: &Erp, app: &RefAny, title: &str, actions: &[ActionSpec]) -> Dom {
    let mut buttons = Dom::create_div().with_css(
        "display: flex; flex-direction: row; flex-wrap: wrap; justify-content: center; gap: 8px; padding-top: 14px;",
    );
    let new = actions
        .iter()
        .find(|a| a.primary && matches!(a.kind, ActionKind::Link(_)));
    if let Some(action) = new {
        buttons.add_child(
            Button::create(action.label.as_str())
                .with_button_type(ButtonType::Primary)
                .with_on_click(app.clone(), on_new as ButtonOnClickCallbackType)
                .dom()
                .with_id(ids::EMPTY_NEW),
        );
    }
    let import = actions
        .iter()
        .find(|a| matches!(&a.kind, ActionKind::Link(path) if path == crate::app::IMPORT));
    if let Some(action) = import {
        buttons.add_child(
            Button::create(action.label.as_str())
                .with_on_click(app.clone(), on_import as ButtonOnClickCallbackType)
                .dom()
                .with_id(ids::EMPTY_IMPORT),
        );
    }
    if s.state.book.assets.is_empty() {
        buttons.add_child(
            Button::create(t("azerp-load-sample"))
                .with_button_type(if new.is_some() {
                    ButtonType::Secondary
                } else {
                    ButtonType::Primary
                })
                .with_on_click(app.clone(), on_load_sample as ButtonOnClickCallbackType)
                .dom()
                .with_id(ids::LOAD_SAMPLE),
        );
    }
    let detail = t(if s.state.book.assets.is_empty() {
        "azerp-empty-first"
    } else {
        "azerp-add-first"
    });
    Dom::create_div()
        .with_id(ids::EMPTY)
        .with_css(
            "display: flex; flex-direction: column; flex-grow: 1; align-items: center; justify-content: center; \
             min-height: 0px; padding: 16px;",
        )
        .with_child(
            ShellEmptyState::create(t_args("azerp-none-yet", &[("title", Arg::from(title))]))
                .with_icon("inventory_2")
                .with_detail(detail)
                .dom(),
        )
        .with_child(buttons)
}

/// The page's primary action (a new record).
extern "C" fn on_new(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_erp(&mut data, &mut info, |s, _| {
        let actions = super::page_actions(s);
        if let Some(action) = actions
            .iter()
            .find(|a| a.primary && matches!(a.kind, ActionKind::Link(_)))
        {
            let kind = action.kind.clone();
            run_action(s, &kind, &Params::new());
        }
    })
}

/// The CSV import's page.
extern "C" fn on_import(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_erp(&mut data, &mut info, |s, _| {
        run_action(
            s,
            &ActionKind::Link(crate::app::IMPORT.to_string()),
            &Params::new(),
        );
    })
}

/// "Load sample data": the sample register into the empty one, as `--sample` does at the start.
extern "C" fn on_load_sample(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_erp(&mut data, &mut info, |s, _| load_sample(s))
}

/// Writes the sample register into an empty one; says how many records on stdout for scripts.
pub(crate) fn load_sample(s: &mut Erp) {
    let mut new_id = mint;
    let n = s.state.seed_sample(&mut new_id);
    if n > 0 {
        println!("AZERP_SAMPLE {n}");
    }
}

#[cfg(test)]
#[path = "states_test.rs"]
mod states_test;
