//! A `detail` view: a record's own page - a slim header (its `subtitle_field`, its `status_field`
//! as a pill), the tabs, and the tab's panel: a named one (`FixedAssetOverviewPanel`,
//! `DepreciationSchedulePanel`) or an `EmbeddedTable` of another view, filtered to this record.
//!
//! The record's name is the location's big line (`ui/location.rs`) and the record's commands
//! (Edit, Check out, Check in, Log maintenance, Dispose, Export schedule, Delete) are the
//! ribbon's Home tab: the page itself holds only what is of the record.

use azul::{
    callbacks::{DataTableOnEventCallbackType, TabOnClickCallbackType},
    prelude::*,
    str::String as AzString,
    widgets::{DataTableEvent, TabHeader, TabHeaderState},
};
use azul_appkit::l10n::t;

use super::{
    column, panels, states,
    table::{self, TableSpec},
    text, with_erp, Erp,
};
use crate::{
    ids,
    metrics::{self, Room},
    model::Asset,
    views::{
        api_filter,
        rows::{self, ViewRecord},
        spec::{self},
        Params, View,
    },
};

/// The px a detail's slim header and tab row take over a tab's table.
pub const HEADER_HEIGHT: f32 = 84.0;

/// The record's name, the big line of the location: the header's `title_field`.
#[must_use]
pub fn title_of(s: &Erp, view: &View, params: &Params) -> String {
    let id = params.get("id").cloned().unwrap_or_default();
    let Some(asset) = s.state.book.get::<Asset>(&id) else {
        return t("azerp-asset-gone");
    };
    match &view.header {
        Some(h) => asset.value(&h.title_field, &s.state.ctx()).display(),
        None => view.title(&s.state.labels, false),
    }
}

/// The page of a `detail` view (assets: the one record kind with one).
pub fn page(s: &Erp, app: &RefAny, view: &View, params: &Params, room: &Room, phone: bool) -> Dom {
    let id = params.get("id").cloned().unwrap_or_default();
    let Some(asset) = s.state.book.get::<Asset>(&id).cloned() else {
        return states::quiet("inventory_2", &t("azerp-asset-gone"), &id);
    };
    let ctx = s.state.ctx();
    let labels = &s.state.labels;
    let mut header = Dom::create_div()
        .with_id(ids::DETAIL)
        .with_css(
            "display: flex; flex-direction: row; align-items: center; gap: 10px; padding: 8px 14px; min-height: 2.25rem;",
        );
    if let Some(h) = &view.header {
        if let Some(sub) = &h.subtitle_field {
            header.add_child(
                text(&asset.value(sub, &ctx).display())
                    .with_id(ids::DETAIL_TITLE)
                    .with_css(format!("font-size: 0.8125rem; {}", super::QUIET_FLORA)),
            );
        }
        if let Some(status) = &h.status_field {
            header.add_child(
                text(&asset.value(status, &ctx).display())
                    .with_id(ids::DETAIL_STATUS)
                    .with_class(ids::PILL)
                    .with_css(super::PILL_CSS),
            );
        }
    }
    let tabs = spec::tabs(view, labels);
    let active = s.state.tab.min(tabs.len().saturating_sub(1));
    let tab_titles: Vec<AzString> = tabs
        .iter()
        .map(|t| AzString::from(t.title.as_str()))
        .collect();
    let tab_row = Dom::create_div()
        .with_id(ids::DETAIL_TABS)
        .with_css("padding: 0px 14px; overflow-x: auto;")
        .with_child(
            TabHeader::create(tab_titles)
                .with_active_tab(active)
                .with_on_click(app.clone(), on_tab as TabOnClickCallbackType)
                .dom(),
        );
    let panel = match tabs.get(active) {
        Some(tab) => match tab.component.as_str() {
            "FixedAssetOverviewPanel" => panels::overview(s, &asset),
            "DepreciationSchedulePanel" => panels::schedule(s, app, &asset, room, phone),
            "EmbeddedTable" => embedded(
                s,
                app,
                tab.view.as_deref().unwrap_or(""),
                params,
                room,
                phone,
            ),
            other => states::quiet("construction", &t("azerp-panel-not-built"), other),
        },
        None => Dom::create_div(),
    };
    column(vec![header, tab_row, panel])
}

/// An `EmbeddedTable` tab: the view's table, only this record's rows.
fn embedded(
    s: &Erp,
    app: &RefAny,
    view_id: &str,
    params: &Params,
    room: &Room,
    phone: bool,
) -> Dom {
    let Some(view) = s.state.views.view(view_id) else {
        return states::quiet("table_chart", &t("azerp-tab-table-missing"), view_id);
    };
    let Some(kind) = view.kind_of_records() else {
        return states::quiet("table_chart", &t("azerp-table-no-records"), view_id);
    };
    let (width, height) = metrics::table(room, HEADER_HEIGHT);
    let columns = table::columns_for(view, &s.state.labels, phone, width);
    let filter = view
        .api
        .get
        .as_deref()
        .and_then(|api| api_filter(api, params));
    let grid = rows::grid(
        kind,
        &columns,
        filter.as_ref().map(|(k, v)| (k.as_str(), v.as_str())),
        &s.state.ctx(),
    );
    if grid.is_empty() {
        return states::quiet("inventory_2", &t("azerp-nothing-recorded-asset"), "");
    }
    table::grid_table(
        TableSpec {
            id: ids::EMBEDDED,
            name: &view.id,
            columns: &columns,
            view: s.inner_table.clone(),
            size: (width, height),
            phone,
        },
        grid,
        app,
        on_inner_event as DataTableOnEventCallbackType,
    )
}

/// A detail tab's table keeps its view.
pub extern "C" fn on_inner_event(
    mut data: RefAny,
    mut info: CallbackInfo,
    event: DataTableEvent,
) -> Update {
    with_erp(&mut data, &mut info, |s, _info| {
        s.inner_table = event.view.clone()
    })
}

extern "C" fn on_tab(mut data: RefAny, mut info: CallbackInfo, state: TabHeaderState) -> Update {
    with_erp(&mut data, &mut info, |s, _info| {
        s.state.tab = state.active_tab;
        s.inner_table = azul::widgets::DataTableView::create();
        println!("AZERP_TAB {}", state.active_tab);
    })
}

#[cfg(test)]
#[path = "detail_test.rs"]
mod detail_test;
