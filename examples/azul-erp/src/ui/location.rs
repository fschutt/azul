//! The location block at the head of the ribbon's File tab (azul's `LocationBar`): the module (and,
//! on a record's page or in a form, the list the record belongs to) as the parent line of links,
//! the page's name - or the record's - big and bold under it. The old File menu is gone: its
//! pages (the Options and the About, which the kit's settings page holds) are behind the `...`
//! beside the name. There is no pen: a record is renamed in its form, not in the title.

use azul::{
    callbacks::LocationOnEventCallbackType,
    prelude::*,
    widgets::{LocationBar, LocationCrumb, LocationEvent, LocationEventKind},
};
use azul_appkit::{l10n::t, ui as kit};

use super::{detail, sidebar, with_erp, Erp};
use crate::{nav, views::ViewKind};

/// What the location says: the parent line (a crumb each: its words and the tag of the sidebar row
/// of the page it goes to) and the name under it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Place {
    pub parents: Vec<(String, usize)>,
    pub name: String,
}

/// The place of a page named `name` in module `module` (its words and the tag of its first page),
/// below `list` (its words and the tag of its page) when the page is a record or a form of a list.
/// A list whose words are the page's own is not said twice.
#[must_use]
pub(crate) fn place_of(module: (&str, usize), list: Option<(&str, usize)>, name: &str) -> Place {
    let mut parents = vec![(module.0.to_string(), module.1)];
    if let Some((words, tag)) = list {
        if !words.is_empty() && words != name && words != module.0 {
            parents.push((words.to_string(), tag));
        }
    }
    Place {
        parents,
        name: name.to_string(),
    }
}

/// The place the window shows now.
fn place(s: &Erp) -> Place {
    let modules = sidebar::modules(s);
    let module = modules.first();
    let module_words = module.map_or_else(|| t("azerp-module-assets"), |m| m.title.clone());
    let current = module.and_then(|m| m.current).unwrap_or(0);
    let list_words = module
        .and_then(|m| m.pages.get(current))
        .map(|p| p.label.clone())
        .unwrap_or_default();
    let module_tag = nav::tag(0, 0);
    let list = Some((list_words.as_str(), nav::tag(0, current)));
    // a form open over the page: its own title under the list
    if let Some(draft) = &s.state.form {
        if let Some(view) = s.state.views.view(&draft.view) {
            let name = view.title(&s.state.labels, draft.editing);
            return place_of((module_words.as_str(), module_tag), list, &name);
        }
    }
    let Some(route) = s.state.route() else {
        return place_of((module_words.as_str(), module_tag), None, &module_words);
    };
    let name = match route.view.kind {
        ViewKind::Detail => detail::title_of(s, route.view, &route.params),
        _ => route.view.title(&s.state.labels, false),
    };
    match route.view.kind {
        // a record, the depreciation run and the import start from a list
        ViewKind::Detail | ViewKind::Wizard | ViewKind::Custom => {
            place_of((module_words.as_str(), module_tag), list, &name)
        }
        _ => place_of((module_words.as_str(), module_tag), None, &name),
    }
}

/// The location block of the File tab.
pub(crate) fn location(s: &Erp, app: &RefAny) -> Dom {
    let place = place(s);
    let mut bar = LocationBar::create(place.name.as_str())
        .with_editable(false)
        .with_more(true)
        .with_form(s.form)
        .with_on_event(app.clone(), on_location as LocationOnEventCallbackType);
    for (words, tag) in &place.parents {
        bar = bar.with_parent(LocationCrumb::create(words.as_str(), *tag));
    }
    bar.dom()
}

/// The block's reports: a crumb shows its page, `...` opens File's old pages (the settings).
extern "C" fn on_location(
    mut data: RefAny,
    mut info: CallbackInfo,
    event: LocationEvent,
) -> Update {
    match event.kind {
        LocationEventKind::CrumbClicked => with_erp(&mut data, &mut info, |s, _| {
            sidebar::open_page(s, event.tag);
        }),
        LocationEventKind::MoreClicked => with_erp(&mut data, &mut info, |s, _| {
            kit::open_settings(&s.kit, None);
        }),
        // no pen: nothing is edited here
        _ => Update::DoNothing,
    }
}

#[cfg(test)]
#[path = "location_test.rs"]
mod location_test;
