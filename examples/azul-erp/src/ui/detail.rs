//! A `detail` view: the header (the record's `title_field`, its
//! `subtitle_field`, its `status_field` as a pill, the header actions whose
//! `condition` holds), the tabs, and the tab's panel - a named one
//! (`FixedAssetOverviewPanel`, `DepreciationSchedulePanel`) or an
//! `EmbeddedTable` of another view, filtered to this record.

use azul::{
    callbacks::{ButtonOnClickCallbackType, DataTableOnEventCallbackType, TabOnClickCallbackType},
    prelude::*,
    str::String as AzString,
    widgets::{DataTableEvent, TabHeader, TabHeaderState},
};

use super::{action_button, column, empty, panels, table, text, with_erp, Erp, CHROME_HEIGHT};
use crate::{
    ids,
    model::Asset,
    views::{
        api_filter,
        rows::{self, ViewRecord},
        spec::{self},
        Params, View,
    },
};

/// The px a detail's header and tab row take over a tab's table.
pub const HEADER_HEIGHT: f32 = 96.0;

/// The page of a `detail` view (assets: the one record kind with one).
pub fn page(s: &Erp, app: &RefAny, view: &View, params: &Params) -> Dom {
    let id = params.get("id").cloned().unwrap_or_default();
    let Some(asset) = s.state.book.get::<Asset>(&id).cloned() else {
        return empty("This asset is not there any more.", &id);
    };
    let ctx = s.state.ctx();
    let labels = &s.state.labels;
    let mut header = Dom::create_div()
        .with_id(ids::DETAIL)
        .with_css("display: flex; flex-direction: row; align-items: center; padding: 8px 12px;")
        .with_child(
            Button::create("Back")
                .with_icon("arrow_back")
                .with_on_click(app.clone(), on_back as ButtonOnClickCallbackType)
                .dom()
                .with_id(ids::BACK),
        );
    if let Some(h) = &view.header {
        let title = asset.value(&h.title_field, &ctx).display();
        let mut heading = text(&title)
            .with_id(ids::DETAIL_TITLE)
            .with_css("font-size: 16px; font-weight: 600; padding-left: 12px;");
        if let Some(sub) = &h.subtitle_field {
            heading = Dom::create_div()
                .with_css("display: flex; flex-direction: row; align-items: baseline;")
                .with_child(heading)
                .with_child(
                    text(&asset.value(sub, &ctx).display())
                        .with_css(format!(
                            "padding-left: 8px; font-size: 13px; opacity: 0.7; {}",
                            super::QUIET_FLORA
                        )),
                );
        }
        header.add_child(heading);
        if let Some(status) = &h.status_field {
            header.add_child(
                text(&asset.value(status, &ctx).display())
                    .with_id(ids::DETAIL_STATUS)
                    .with_class(ids::PILL)
                    // Under flora flora.css's `.pill`: a quiet badge at the house radius,
                    // its edge the theme's, its label in the label ink's capitals.
                    .with_css(
                        "margin-left: 10px; padding: 2px 8px; border-radius: 10px; font-size: 12px; \
                         border: 1px solid #8888; @theme(flora) { border-radius: 3px; border: 1px \
                         solid system:separator; color: system:secondary-text; font-size: 11px; \
                         font-weight: bold; text-transform: uppercase; letter-spacing: 0.07em; }",
                    ),
            );
        }
        header.add_child(Dom::create_div().with_css("flex-grow: 1;"));
        let value_of = |name: &str| asset.value(name, &ctx).form_text();
        for action in spec::actions(&h.actions, labels) {
            let shows = action
                .condition
                .as_deref()
                .map_or(true, |c| spec::eval_condition(c, &value_of));
            if shows {
                header.add_child(action_button(app, &action, params));
            }
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
        .with_css("padding: 0px 12px;")
        .with_child(
            TabHeader::create(tab_titles)
                .with_active_tab(active)
                .with_on_click(app.clone(), on_tab as TabOnClickCallbackType)
                .dom(),
        );
    let panel = match tabs.get(active) {
        Some(tab) => match tab.component.as_str() {
            "FixedAssetOverviewPanel" => panels::overview(s, &asset),
            "DepreciationSchedulePanel" => panels::schedule(s, app, &asset),
            "EmbeddedTable" => embedded(s, app, tab.view.as_deref().unwrap_or(""), params),
            other => empty("This panel is not built yet.", other),
        },
        None => Dom::create_div(),
    };
    column(vec![header, tab_row, panel])
}

/// An `EmbeddedTable` tab: the view's table, only this record's rows.
fn embedded(s: &Erp, app: &RefAny, view_id: &str, params: &Params) -> Dom {
    let Some(view) = s.state.views.view(view_id) else {
        return empty("The table of this tab is missing.", view_id);
    };
    let Some(kind) = view.kind_of_records() else {
        return empty("This table reads no records.", view_id);
    };
    let columns = spec::columns(view, &s.state.labels);
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
        return empty("Nothing recorded for this asset yet.", "");
    }
    let size = (s.window.0, s.window.1 - CHROME_HEIGHT - HEADER_HEIGHT);
    table::grid_table(
        ids::EMBEDDED,
        &view.id,
        &columns,
        grid,
        s.inner_table.clone(),
        size,
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

extern "C" fn on_back(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_erp(&mut data, &mut info, |s, _info| {
        s.state.go_back();
        println!("AZERP_PAGE {}", s.state.page);
    })
}
