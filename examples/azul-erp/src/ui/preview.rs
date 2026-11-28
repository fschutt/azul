//! The record in azul's `Preview`: the selected row of the page's table ITSELF - its name, its
//! number and status, the facts the table's columns name - and nothing else. What the old record
//! page piled around it is behind small (?) buttons in the Preview's header: the properties the
//! table has no column for, an asset's depreciation schedule, its maintenance log, its check-outs.
//! What needs saying at once is the banner at the very top: a check-out that is overdue, a service
//! that is due, with its own (?) holding the details.
//!
//! The facts are the JSON views': the title, the subtitle and the status come from the record
//! page's header (`title_field`, `subtitle_field`, `status_field`), the facts from the table's
//! columns, the properties from the edit form's fields, the (?) buttons from the record page's tabs.
//!
//! On a desktop the Preview is docked at the right of the table; on a phone it is the bottom pane
//! (four detents, a tap on its grip toggles the peek and the half) and the SELECTION and the
//! Preview follow the row at the TOP EDGE of the table while it scrolls (`ui/table.rs`,
//! `follow.rs`); a tap holds them until the table is scrolled on.
//!
//! The sizes of the text written here are in `rem`, never in px: they follow the root font size, so
//! the system's text size, which the engine applies at the root, reaches them.

use azul::{
    callbacks::{ButtonOnClickCallbackType, PreviewOnEventCallbackType},
    prelude::*,
    widgets::{
        Button, ButtonType, Preview, PreviewBanner, PreviewBannerTone, PreviewDetail, PreviewEvent,
        PreviewEventKind,
    },
};
use azul_appkit::l10n::{label, t, t_args, Arg};

use super::{states, text, with_erp, Erp};
use crate::{
    app::State,
    depreciation,
    model::{self, Asset},
    money, reports,
    store::Kind,
    views::{
        api_filter,
        rows::{self, Value},
        spec::{self, ActionKind, ColumnSpec},
        Params, ViewKind,
    },
};

/// The widest or tallest a Preview is taken to be from what a drag reported, px.
pub(crate) const SIZE_MAX: f32 = 4_000.0;

/// The most lines one (?) lists; a longer list says how many it leaves out.
pub(crate) const MAX_LINES: usize = 40;

/// A pane size a drag reported, held to what the app keeps: a number that is no size is 0 (the
/// widget's default), a size beyond [`SIZE_MAX`] is the limit.
#[must_use]
pub(crate) fn clamped_size(size: f32) -> f32 {
    if size.is_finite() && size > 0.0 {
        size.min(SIZE_MAX)
    } else {
        0.0
    }
}

/// How urgent a banner is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Tone {
    Info,
    Warning,
}

/// What a banner says and what is behind its (?).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Banner {
    pub tone: Tone,
    pub line: String,
    pub details: String,
}

/// One (?) of the header: what it is called, its icon and its lines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Detail {
    pub words: String,
    pub icon: &'static str,
    pub lines: Vec<String>,
}

/// The record the Preview shows, as plain data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Card {
    pub kind: Kind,
    pub id: String,
    pub title: String,
    pub subtitle: String,
    /// The status' words, when the record has one.
    pub status: Option<String>,
    /// The facts the table's columns name: `(column title, value)`.
    pub facts: Vec<(String, String)>,
    pub banner: Option<Banner>,
    /// What stands behind the (?) buttons: the properties first, then the record page's tabs.
    pub details: Vec<Detail>,
}

/// Whether the window has a Preview: it is on (View > Preview), the records are in, no form is
/// open, and the page is a table (a record's page, the reports, the run and the import are the
/// body themselves).
#[must_use]
pub(crate) fn shown(s: &Erp) -> bool {
    s.preview_open
        && s.state.loaded
        && s.state.form.is_none()
        && s.state
            .route()
            .is_some_and(|r| r.view.kind == ViewKind::Table)
}

/// The icon of a record page's tab behind a (?).
#[must_use]
fn tab_icon(key: &str) -> &'static str {
    static ICONS: [(&str, &str); 3] = [
        ("tab.depreciation_schedule", "show_chart"),
        ("tab.maintenance", "build"),
        ("tab.checkouts", "swap_horiz"),
    ];
    ICONS
        .iter()
        .find(|(k, _)| *k == key)
        .map_or("list", |(_, icon)| icon)
}

/// The names of the fields the title, subtitle and status of a record are read from: the record
/// page's header, else the table's first column.
fn header_fields(
    columns: &[ColumnSpec],
    target: Option<&crate::views::View>,
) -> (String, String, String) {
    match target.and_then(|v| v.header.as_ref()) {
        Some(h) => (
            h.title_field.clone(),
            h.subtitle_field.clone().unwrap_or_default(),
            h.status_field.clone().unwrap_or_default(),
        ),
        None => (
            columns.first().map(|c| c.field.clone()).unwrap_or_default(),
            String::new(),
            String::new(),
        ),
    }
}

/// The depreciation schedule of an asset as lines: the year, what is written off, what is left.
fn schedule_lines(asset: &Asset) -> Vec<String> {
    let rows = depreciation::schedule(asset);
    let mut lines: Vec<String> = rows
        .iter()
        .take(MAX_LINES)
        .map(|r| {
            t_args(
                "azerp-schedule-line",
                &[
                    ("year", Arg::from(r.year)),
                    (
                        "depreciation",
                        Arg::from(money::format_amount(r.depreciation)),
                    ),
                    ("closing", Arg::from(money::format_amount(r.closing))),
                ],
            )
        })
        .collect();
    if rows.len() > MAX_LINES {
        lines.push(t_args(
            "azerp-more-lines",
            &[("count", Arg::from(rows.len() - MAX_LINES))],
        ));
    }
    lines
}

/// The rows of an embedded table view for the record the path's parameters name, one line each:
/// the cells joined.
fn embedded_lines(state: &State, view_id: &str, params: &Params) -> Vec<String> {
    let Some(view) = state.views.view(view_id) else {
        return Vec::new();
    };
    let Some(kind) = view.kind_of_records() else {
        return Vec::new();
    };
    let columns = spec::columns(view, &state.labels);
    let filter = view
        .api
        .get
        .as_deref()
        .and_then(|api| api_filter(api, params));
    let grid = rows::grid(
        kind,
        &columns,
        filter.as_ref().map(|(k, v)| (k.as_str(), v.as_str())),
        &state.ctx(),
    );
    let mut lines: Vec<String> = grid
        .text
        .iter()
        .take(MAX_LINES)
        .map(|cells| {
            cells
                .iter()
                .filter(|c| !c.is_empty())
                .cloned()
                .collect::<Vec<_>>()
                .join("  ·  ")
        })
        .collect();
    if grid.len() > MAX_LINES {
        lines.push(t_args(
            "azerp-more-lines",
            &[("count", Arg::from(grid.len() - MAX_LINES))],
        ));
    }
    lines
}

/// What needs saying about an asset at once: an open check-out past its day, else a service past
/// its day or due within a month.
fn banner_of(state: &State, asset: &Asset) -> Option<Banner> {
    let today = state.today;
    if let Some(late) = reports::overdue_checkouts(&state.book, today)
        .into_iter()
        .find(|o| o.asset == asset.id)
    {
        return Some(Banner {
            tone: Tone::Warning,
            line: t_args(
                "azerp-banner-checkout-overdue",
                &[
                    ("custodian", Arg::from(&late.custodian)),
                    ("days", Arg::from(late.days_late)),
                ],
            ),
            details: t_args(
                "azerp-banner-checkout-overdue-detail",
                &[
                    ("custodian", Arg::from(&late.custodian)),
                    ("due", Arg::from(model::format_date(late.due))),
                    ("days", Arg::from(late.days_late)),
                ],
            ),
        });
    }
    let next = reports::next_service(&state.book, asset)?;
    let days = (next - today).num_days();
    if days > 30 {
        return None;
    }
    let when = model::format_date(next);
    if days < 0 {
        Some(Banner {
            tone: Tone::Warning,
            line: t_args(
                "azerp-banner-service-overdue",
                &[("when", Arg::from(&when))],
            ),
            details: t_args(
                "azerp-banner-service-detail",
                &[
                    ("every", Arg::from(asset.maintenance_months)),
                    ("when", Arg::from(&when)),
                ],
            ),
        })
    } else {
        Some(Banner {
            tone: Tone::Info,
            line: t_args("azerp-banner-service-due", &[("when", Arg::from(&when))]),
            details: t_args(
                "azerp-banner-service-detail",
                &[
                    ("every", Arg::from(asset.maintenance_months)),
                    ("when", Arg::from(&when)),
                ],
            ),
        })
    }
}

/// The card of the record the selection is on; `None` with no selection or a record that is gone.
#[must_use]
pub(crate) fn card(state: &State) -> Option<Card> {
    let sel = state.selected.as_ref()?;
    let route = state.route()?;
    let view = route.view;
    let columns = spec::columns(view, &state.labels);
    let path = state.row_path(view, sel.kind, &sel.id)?;
    let target = state.views.route(&path);
    let detail = target
        .as_ref()
        .map(|t| t.view)
        .filter(|v| v.kind == ViewKind::Detail);
    let (title_f, subtitle_f, status_f) = header_fields(&columns, detail);

    // every field the card reads, read once
    let mut fields: Vec<&str> = vec![title_f.as_str(), subtitle_f.as_str(), status_f.as_str()];
    fields.extend(columns.iter().map(|c| c.field.as_str()));
    let ctx = state.ctx();
    let values = rows::values_of(sel.kind, &sel.id, &fields, &ctx)?;
    let shown = |i: usize| values.get(i).map(Value::display).unwrap_or_default();
    let title = shown(0);
    let subtitle = if subtitle_f.is_empty() {
        String::new()
    } else {
        shown(1)
    };
    let status = (!status_f.is_empty())
        .then(|| shown(2))
        .filter(|s| !s.is_empty());
    let named = |name: &str| {
        !name.is_empty()
            && [&title_f, &subtitle_f, &status_f]
                .iter()
                .any(|f| f.as_str() == name)
    };
    let facts: Vec<(String, String)> = columns
        .iter()
        .enumerate()
        .filter(|(_, c)| !named(&c.field))
        .filter_map(|(i, c)| {
            let text = shown(3 + i);
            (!text.is_empty()).then(|| (c.title.clone(), text))
        })
        .collect();

    // behind the (?) buttons: the properties the table has no column for ...
    let mut details: Vec<Detail> = Vec::new();
    let edit = state.views.views.values().find(|v| {
        v.kind == ViewKind::Form
            && v.kind_of_records() == Some(sel.kind)
            && v.path.all().iter().any(|p| p.ends_with("/edit"))
    });
    if let Some(form) = edit {
        let mut properties: Vec<String> = Vec::new();
        for f in spec::fields(&form.fields, &state.labels) {
            let name = match f.name.as_str() {
                "category_id" => "category",
                "location_id" => "location",
                other => other,
            };
            if columns.iter().any(|c| c.field == name) || named(name) {
                continue;
            }
            if let Some(v) =
                rows::values_of(sel.kind, &sel.id, &[name], &ctx).and_then(|v| v.into_iter().next())
            {
                let shown = v.display();
                if !shown.is_empty() {
                    properties.push(format!("{}: {shown}", f.label));
                }
            }
        }
        if !properties.is_empty() {
            details.push(Detail {
                words: t("azerp-preview-properties"),
                icon: "info",
                lines: properties,
            });
        }
    }
    // ... and the record page's tabs
    let asset = (sel.kind == Kind::Asset)
        .then(|| state.book.get::<Asset>(&sel.id))
        .flatten();
    if let (Some(detail_view), Some(target)) = (detail, target.as_ref()) {
        for tab in &detail_view.tabs {
            let lines = match (tab.component.as_str(), asset) {
                ("DepreciationSchedulePanel", Some(a)) => schedule_lines(a),
                ("EmbeddedTable", _) => {
                    embedded_lines(state, tab.view.as_deref().unwrap_or(""), &target.params)
                }
                _ => continue,
            };
            if !lines.is_empty() {
                details.push(Detail {
                    words: state.labels.get(&tab.key),
                    icon: tab_icon(&tab.key),
                    lines,
                });
            }
        }
    }
    Some(Card {
        kind: sel.kind,
        id: sel.id.clone(),
        title,
        subtitle,
        status,
        facts,
        banner: asset.and_then(|a| banner_of(state, a)),
        details,
    })
}

// ==== The widget ====

/// A row of facts: the name and what it says.
fn fact(name: &str, value: &str) -> Dom {
    Dom::create_div()
        .with_css("display: flex; flex-direction: row; gap: 10px; font-size: 0.8125rem; padding: 1px 0px;")
        .with_child(
            text(name).with_css(format!(
                "flex-shrink: 0; width: 6.5rem; opacity: 0.75; {}",
                super::QUIET_FLORA
            )),
        )
        .with_child(text(value).with_css("flex-grow: 1; min-width: 0px; overflow-wrap: anywhere;"))
}

/// The lines behind a (?): one to a line.
fn lines_dom(lines: &[String]) -> Dom {
    let mut column = Dom::create_div()
        .with_css("display: flex; flex-direction: column; gap: 3px; max-width: 24rem;");
    for line in lines {
        column.add_child(text(line).with_css("font-size: 0.75rem; overflow-wrap: anywhere;"));
    }
    column
}

/// The pane: the record's name, its number and status, its facts. With none selected, the empty
/// state.
fn pane(card: Option<&Card>) -> Dom {
    let Some(card) = card else {
        return states::quiet(
            "inventory_2",
            &t("azerp-preview-nothing"),
            &t("azerp-preview-nothing-detail"),
        );
    };
    let mut head = Dom::create_div()
        .with_css("display: flex; flex-direction: row; align-items: baseline; gap: 10px; min-width: 0px;")
        .with_child(
            text(&card.title).with_css("font-size: 1rem; font-weight: 700; flex-grow: 1; min-width: 0px; overflow-wrap: anywhere;"),
        );
    if let Some(status) = &card.status {
        head.add_child(
            text(status)
                .with_id(crate::ids::PREVIEW_STATUS)
                .with_class(crate::ids::PILL)
                .with_css(super::PILL_CSS),
        );
    }
    let mut column = Dom::create_div()
        .with_id(crate::ids::PREVIEW_CARD)
        .with_css(
            "display: flex; flex-direction: column; gap: 2px; padding: 8px 14px; min-width: 0px;",
        )
        .with_child(head);
    if !card.subtitle.is_empty() {
        column.add_child(text(&card.subtitle).with_css(format!(
            "font-size: 0.8125rem; padding-bottom: 6px; opacity: 0.75; {}",
            super::QUIET_FLORA
        )));
    }
    for (name, value) in &card.facts {
        column.add_child(fact(name, value));
    }
    column
}

/// The Preview of the window now.
pub(crate) fn preview(s: &Erp, app: &RefAny) -> Preview {
    let card = card(&s.state);
    let title = card
        .as_ref()
        .map_or_else(|| t("azerp-preview"), |c| c.title.clone());
    let mut preview = Preview::create(Dom::create_div(), pane(card.as_ref()))
        .with_title(title.as_str())
        .with_size(clamped_size(s.preview_size))
        .with_form(s.form)
        .with_dom_id(crate::ids::PREVIEW)
        .with_on_event(app.clone(), on_preview as PreviewOnEventCallbackType);
    if let Some(card) = &card {
        preview = preview.with_header_end(
            Button::with_type(label("azerp-open"), ButtonType::Secondary)
                .with_on_click(app.clone(), on_open as ButtonOnClickCallbackType)
                .dom()
                .with_id(crate::ids::PREVIEW_OPEN),
        );
        for detail in &card.details {
            preview = preview.with_detail(
                PreviewDetail::create(detail.words.as_str(), lines_dom(&detail.lines))
                    .with_icon(detail.icon),
            );
        }
        if let Some(banner) = &card.banner {
            preview = preview.with_banner(
                PreviewBanner::create(
                    banner.line.as_str(),
                    match banner.tone {
                        Tone::Info => PreviewBannerTone::Info,
                        Tone::Warning => PreviewBannerTone::Warning,
                    },
                )
                .with_details(lines_dom(std::slice::from_ref(&banner.details))),
            );
        }
    }
    preview
}

/// The pane's size as a drag reports it: kept for the next build.
extern "C" fn on_preview(mut data: RefAny, _info: CallbackInfo, event: PreviewEvent) -> Update {
    let kept = match data.downcast_mut::<Erp>() {
        Some(mut s) => {
            s.preview_size = clamped_size(event.size);
            true
        }
        None => false,
    };
    // while the pane is dragged the widget moves it; a rebuild is for where it settled
    if kept && event.kind == PreviewEventKind::Settled {
        Update::RefreshDom
    } else {
        Update::DoNothing
    }
}

/// Open: the record's page.
extern "C" fn on_open(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_erp(&mut data, &mut info, |s, _| {
        let Some(sel) = s.state.selected.clone() else {
            return;
        };
        let Some(view) = s.state.route().map(|r| r.view.clone()) else {
            return;
        };
        if let Some(path) = s.state.row_path(&view, sel.kind, &sel.id) {
            super::run_action(s, &ActionKind::Link(path), &Params::new());
        }
    })
}

#[cfg(test)]
#[path = "preview_test.rs"]
mod preview_test;
