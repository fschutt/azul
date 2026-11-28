//! The location block at the head of the ribbon's File tab (azul's `LocationBar`): the year (and
//! the month) as the trail, the month, the week's days or the day big and bold under it. The old
//! FILE menu is gone: its pages (Info, Open & Export, Print, Calendars, Options, About) are the
//! dialogs the `...` beside the name opens (a desktop's menu, a phone's action sheet), and the
//! pen is "go to a date": type "24 Dec", "2026-12-24", "24.12.", "+1w" or "today" and the view
//! follows what is typed.

use azul::{
    callbacks::LocationOnEventCallbackType,
    menu::{Menu, MenuItem, MenuPopupPosition, StringMenuItem},
    prelude::*,
    widgets::{LocationBar, LocationCrumb, LocationEvent, LocationEventKind},
};
use azul_appkit::l10n::{label, t, DateStyle};
use chrono::Datelike;

use crate::{
    args::BackstagePage,
    chrome,
    nav::{self, Place, Say},
    ui_frame,
    ui_sheet::SheetFor,
    views::{self, ViewKind},
    CalState,
};

/// The English names of the months: the keys of appkit's words are `kit-month-<name>`.
static MONTH_NAMES: [&str; 12] = [
    "january",
    "february",
    "march",
    "april",
    "may",
    "june",
    "july",
    "august",
    "september",
    "october",
    "november",
    "december",
];

/// The place's words in the window's language.
struct Words;

impl Say for Words {
    fn year(&self, year: i32) -> String {
        year.to_string()
    }

    fn month(&self, month: u32) -> String {
        let name = MONTH_NAMES[(month.clamp(1, 12) - 1) as usize];
        t(&format!("kit-month-{name}"))
    }

    fn day(&self, day: chrono::NaiveDate) -> String {
        crate::day_text(DateStyle::ShortDate, day)
    }

    fn range(&self, first: chrono::NaiveDate, last: chrono::NaiveDate) -> String {
        let head = if first.month() == last.month() && first.year() == last.year() {
            DateStyle::DayOnly
        } else {
            DateStyle::DayShortMonth
        };
        format!(
            "{} - {}",
            crate::day_text(head, first),
            crate::day_text(DateStyle::DayShortMonth, last)
        )
    }

    fn list(&self) -> String {
        t("azcalendar-next-7-days")
    }
}

/// The place the window shows now.
#[must_use]
pub(crate) fn place(s: &CalState) -> Place {
    nav::place_of(s.view, s.anchor, views::visible_range(s.view, s.anchor), &Words)
}

/// The location block of the File tab.
pub(crate) fn location(s: &CalState, app: &RefAny) -> Dom {
    let place = place(s);
    let mut bar = LocationBar::create(place.name.as_str())
        .with_editable(true)
        .with_editing(s.goto_editing, s.goto_text.as_str())
        .with_placeholder(label("azcalendar-goto-placeholder"))
        .with_more(true)
        .with_form(s.form)
        .with_on_event(app.clone(), on_location as LocationOnEventCallbackType);
    for (name, tag) in &place.crumbs {
        bar = bar.with_parent(LocationCrumb::create(name.as_str(), *tag));
    }
    bar.dom()
}

/// The block's reports: a crumb zooms out to the month, the pen goes to a date, `...` opens File's
/// pages.
extern "C" fn on_location(mut data: RefAny, mut info: CallbackInfo, event: LocationEvent) -> Update {
    let app = data.clone();
    let Some(mut s) = data.downcast_mut::<CalState>() else {
        return Update::DoNothing;
    };
    match event.kind {
        LocationEventKind::CrumbClicked => {
            if s.view != ViewKind::Month {
                s.set_view(ViewKind::Month);
            }
        }
        LocationEventKind::EditStarted => {
            s.goto_editing = true;
            // empty: the field shows the name
            s.goto_text.clear();
        }
        LocationEventKind::EditChanged => {
            s.goto_text = event.text.as_str().to_string();
            if let Some(day) = nav::parse_goto(&s.goto_text, s.today, s.anchor) {
                if day != s.anchor {
                    s.set_anchor(day);
                }
            }
        }
        LocationEventKind::EditCommitted => {
            s.goto_editing = false;
            let typed = event.text.as_str().to_string();
            s.goto_text.clear();
            match nav::parse_goto(&typed, s.today, s.anchor) {
                Some(day) => s.set_anchor(day),
                None if typed.trim().is_empty() => {}
                None => s.notice = t("azcalendar-goto-not-a-date"),
            }
        }
        LocationEventKind::EditCancelled => {
            s.goto_editing = false;
            s.goto_text.clear();
        }
        LocationEventKind::MoreClicked => {
            if ui_frame::phone_now(&s) {
                s.sheet = Some(SheetFor::File);
            } else {
                drop(s);
                open_file_menu(&app, &mut info);
                return Update::DoNothing;
            }
        }
    }
    Update::RefreshDom
}

// ---- File's old items on a desktop: a menu ----

/// A page, for the callback of its menu entry.
struct PageRef {
    app: RefAny,
    page: BackstagePage,
}

/// File's pages as a drop-down menu under the `...`.
fn open_file_menu(app: &RefAny, info: &mut CallbackInfo) {
    let items: Vec<MenuItem> = nav::FILE_ITEMS
        .iter()
        .map(|(name, _, page)| {
            MenuItem::String(
                StringMenuItem::create(label(name)).with_callback(
                    RefAny::new(PageRef {
                        app: app.clone(),
                        page: *page,
                    }),
                    on_file_item,
                ),
            )
        })
        .collect();
    let menu = Menu::create(items).with_popup_position(MenuPopupPosition::BottomOfHitRect);
    if !info.open_menu_for_hit_node(menu.clone()) {
        info.open_menu(menu);
    }
}

/// A menu entry chosen: that page opens.
extern "C" fn on_file_item(mut data: RefAny, _info: CallbackInfo) -> Update {
    let Some((mut app, page)) = data.downcast_ref::<PageRef>().map(|r| (r.app.clone(), r.page)) else {
        return Update::DoNothing;
    };
    let Some(mut s) = app.downcast_mut::<CalState>() else {
        return Update::DoNothing;
    };
    chrome::open_backstage(&mut s, page);
    Update::RefreshDom
}

#[cfg(test)]
#[path = "ui_location_test.rs"]
mod ui_location_test;
