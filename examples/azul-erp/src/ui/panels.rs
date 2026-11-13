//! The named panels the views refer to (the README: "the JSON only names
//! them"): the asset's overview and depreciation schedule, the reports, the
//! depreciation run wizard's steps, the CSV import.

use std::path::PathBuf;

use azul::{
    callbacks::{
        ButtonOnClickCallbackType, DataTableOnEventCallbackType,
        DropDownOnChoiceChangeCallbackType, TextInputOnTextInputCallbackType,
    },
    dialog::{FileDialog, FileOpenResult},
    option::OptionFileTypeList,
    prelude::*,
    str::String as AzString,
    vec::StringVec,
    widgets::{
        ButtonType, Chart, ChartKind, ChartPoint, ChartSeries, DropDown, OnTextInputReturn,
        TextInputState, TextInputValid,
    },
};
use azul_appkit::{
    l10n::{t, t_args, Arg},
    ui as kit,
};
use chrono::Datelike;

use super::{
    column, detail, empty, mint, page_header, table, text, with_erp, Erp, CHROME_HEIGHT, TAG_IMPORT,
};
use crate::{
    csv_io::Field,
    depreciation, ids,
    model::Asset,
    money, reports,
    views::{
        rows::{Grid, ViewRecord},
        spec::{self, ActionKind, ColumnSpec, ColumnType},
        Params, View,
    },
};

/// A label and its value, as a row.
fn fact(label: &str, value: &str) -> Dom {
    Dom::create_div()
        .with_class(ids::FIELD_ROW)
        .with_css("display: flex; flex-direction: row; padding: 3px 0px;")
        .with_child(
            text(label)
                .with_class(ids::FIELD_LABEL)
                .with_css(format!(
                    "width: 200px; flex-shrink: 0; font-size: 13px; opacity: 0.75; {}",
                    super::QUIET_FLORA
                )),
        )
        .with_child(text(value).with_css("font-size: 13px;"))
}

/// A heading inside a panel (flora's label under flora).
fn heading(title: &str) -> Dom {
    text(title).with_css(format!(
        "font-size: 13px; font-weight: 600; padding: 12px 0px 4px 0px; {}",
        super::LABEL_FLORA
    ))
}

// ==== FixedAssetOverviewPanel ====

/// The asset's fields (the asset form's, with names for ids), its book
/// value today, its custodian, its next service, a disposal's result.
pub fn overview(s: &Erp, asset: &Asset) -> Dom {
    let ctx = s.state.ctx();
    let labels = &s.state.labels;
    let fields = s
        .state
        .views
        .view("assets_fixed_asset_form")
        .map(|v| spec::fields(&v.fields, labels))
        .unwrap_or_default();
    let mut rows: Vec<Dom> = Vec::new();
    for f in &fields {
        let shown = match f.name.as_str() {
            "category_id" => "category",
            "location_id" => "location",
            name => name,
        };
        let value = asset.value(shown, &ctx).display();
        if !value.is_empty() {
            rows.push(fact(&f.label, &value));
        }
    }
    rows.push(fact(
        &t("azerp-book-value-today"),
        &asset.value("book_value", &ctx).display(),
    ));
    rows.push(fact(&t("azerp-status"), &asset.status.said()));
    if !asset.custodian.is_empty() {
        rows.push(fact(&t("azerp-custodian"), &asset.custodian));
    }
    if let Some(next) = reports::next_service(&s.state.book, asset) {
        rows.push(fact(
            &t("azerp-next-service"),
            &crate::model::format_date(next),
        ));
    }
    if let Some(result) = depreciation::disposal_result(asset) {
        let word = t(if result >= 0 {
            "azerp-gain-on-disposal"
        } else {
            "azerp-loss-on-disposal"
        });
        rows.push(fact(&word, &money::format_amount(result.abs())));
    }
    column(rows)
        .with_id(ids::OVERVIEW)
        .with_css("padding: 8px 16px; overflow-y: auto;")
}

// ==== DepreciationSchedulePanel ====

/// The schedule's columns: a field each, its title said in the window's language.
fn schedule_columns() -> Vec<ColumnSpec> {
    let col = |field: &str, title: &str, kind: ColumnType, width: f32| ColumnSpec {
        field: field.to_string(),
        title: t(title),
        kind,
        width,
        sortable: false,
    };
    vec![
        col("year", "azerp-col-year", ColumnType::Integer, 70.0),
        col("months", "azerp-col-months", ColumnType::Integer, 70.0),
        col(
            "opening value",
            "azerp-col-opening",
            ColumnType::Currency,
            130.0,
        ),
        col(
            "depreciation",
            "azerp-col-depreciation",
            ColumnType::Currency,
            130.0,
        ),
        col(
            "accumulated",
            "azerp-col-accumulated",
            ColumnType::Currency,
            130.0,
        ),
        col(
            "closing value",
            "azerp-col-closing",
            ColumnType::Currency,
            130.0,
        ),
    ]
}

/// The asset's schedule as a table, and its book value year by year as a line.
#[allow(clippy::cast_precision_loss)] // chart coordinates, not money arithmetic
pub fn schedule(s: &Erp, app: &RefAny, asset: &Asset) -> Dom {
    let rows = depreciation::schedule(asset);
    if rows.is_empty() {
        return empty(
            &t("azerp-nothing-to-depreciate"),
            &t("azerp-nothing-to-depreciate-detail"),
        );
    }
    let mut grid = Grid::default();
    for r in &rows {
        grid.ids.push(r.year.to_string());
        grid.text.push(vec![
            r.year.to_string(),
            r.months.to_string(),
            money::format_amount(r.opening),
            money::format_amount(r.depreciation),
            money::format_amount(r.accumulated),
            money::format_amount(r.closing),
        ]);
        grid.sort.push(vec![
            f64::from(r.year),
            f64::from(r.months),
            r.opening as f64 / 100.0,
            r.depreciation as f64 / 100.0,
            r.accumulated as f64 / 100.0,
            r.closing as f64 / 100.0,
        ]);
    }
    let chart_height = 180.0;
    let size = (
        s.window.0,
        s.window.1 - CHROME_HEIGHT - detail::HEADER_HEIGHT - chart_height,
    );
    let table = table::grid_table(
        ids::SCHEDULE_TABLE,
        &t("azerp-schedule"),
        &schedule_columns(),
        grid,
        s.inner_table.clone(),
        size,
        app,
        detail::on_inner_event as DataTableOnEventCallbackType,
    );
    let years: Vec<AzString> = rows
        .iter()
        .map(|r| AzString::from(r.year.to_string()))
        .collect();
    let points: Vec<ChartPoint> = rows
        .iter()
        .enumerate()
        .map(|(i, r)| ChartPoint::create(i as f64, r.closing as f64 / 100.0))
        .collect();
    let chart = Chart::create(
        ChartKind::Line,
        (s.window.0 - 48.0).max(240.0),
        chart_height,
    )
    .with_title(t("azerp-chart-year-end-value"))
    .with_axis_titles(t("azerp-col-year"), t("azerp-book-value"))
    .with_categories(years)
    .with_added_series(ChartSeries::create(t("azerp-book-value"), points))
    .dom()
    .with_id(ids::SCHEDULE_CHART);
    column(vec![table, chart]).with_id(ids::SCHEDULE)
}

// ==== AssetReportsPanel ====

/// A figure with its caption.
fn card(caption: &str, value: &str) -> Dom {
    Dom::create_div()
        .with_css(
            "display: flex; flex-direction: column; padding: 8px 16px 8px 0px; min-width: 140px;",
        )
        .with_child(text(caption).with_css(format!("font-size: 12px; opacity: 0.75; {}", super::QUIET_FLORA)))
        .with_child(text(value).with_css("font-size: 18px; font-weight: 600;"))
}

/// The reports: the register's figures, the book value by category, the
/// depreciation forecast, the services due, the overdue check-outs.
#[allow(clippy::cast_precision_loss)] // chart coordinates, not money arithmetic
pub fn reports(s: &Erp, app: &RefAny, view: &View) -> Dom {
    let title = view.title(&s.state.labels, false);
    let header = page_header(s, app, view, &title, &Params::new());
    let book = &s.state.book;
    let today = s.state.today;
    let totals = reports::totals(book, today);
    let figures = Dom::create_div()
        .with_id(ids::REPORT_TOTALS)
        .with_css("display: flex; flex-direction: row; flex-wrap: wrap;")
        .with_child(card(&t("azerp-assets-on-books"), &totals.count.to_string()))
        .with_child(card(&t("azerp-cost"), &money::format_amount(totals.cost)))
        .with_child(card(
            &t("azerp-book-value"),
            &money::format_amount(totals.book_value),
        ))
        .with_child(card(
            &t_args(
                "azerp-depreciation-year",
                &[("year", Arg::from(today.year()))],
            ),
            &money::format_amount(totals.depreciation_this_year),
        ))
        .with_child(card(
            &t("azerp-checked-out"),
            &totals.checked_out.to_string(),
        ))
        .with_child(card(&t("azerp-disposed"), &totals.disposed.to_string()));

    let width = ((s.window.0 - 64.0) / 2.0).max(240.0);
    let groups = reports::by_category(book, today);
    let names: Vec<AzString> = groups
        .iter()
        .map(|g| AzString::from(g.name.as_str()))
        .collect();
    let values: Vec<ChartPoint> = groups
        .iter()
        .enumerate()
        .map(|(i, g)| ChartPoint::create(i as f64, g.book_value as f64 / 100.0))
        .collect();
    let by_category = Chart::create(ChartKind::Bar, width, 220.0)
        .with_title(t("azerp-chart-by-category"))
        .with_categories(names)
        .with_added_series(ChartSeries::create(t("azerp-book-value"), values))
        .dom()
        .with_id(ids::REPORT_CATEGORIES);
    let years = reports::depreciation_by_year(book, today.year() - 1, today.year() + 4);
    let year_names: Vec<AzString> = years
        .iter()
        .map(|(y, _)| AzString::from(y.to_string()))
        .collect();
    let amounts: Vec<ChartPoint> = years
        .iter()
        .enumerate()
        .map(|(i, (_, cents))| ChartPoint::create(i as f64, *cents as f64 / 100.0))
        .collect();
    let forecast = Chart::create(ChartKind::Bar, width, 220.0)
        .with_title(t("azerp-chart-by-year"))
        .with_categories(year_names)
        .with_added_series(ChartSeries::create(t("azerp-col-depreciation"), amounts))
        .dom()
        .with_id(ids::REPORT_FORECAST);
    let charts = Dom::create_div()
        .with_css("display: flex; flex-direction: row; flex-wrap: wrap;")
        .with_child(by_category)
        .with_child(forecast);

    let mut due = vec![heading(&t_args(
        "azerp-services-due",
        &[("days", Arg::from(60_u32))],
    ))];
    let services = reports::maintenance_due(book, today, 60);
    if services.is_empty() {
        due.push(text(&t("azerp-none")));
    }
    for d in services {
        let service = [
            ("number", Arg::from(&d.number)),
            ("name", Arg::from(&d.name)),
            ("when", Arg::from(crate::model::format_date(d.due))),
        ];
        let key = if d.overdue {
            "azerp-service-overdue"
        } else {
            "azerp-service-due"
        };
        due.push(text(&t_args(key, &service)));
    }
    let mut late = vec![heading(&t("azerp-overdue-checkouts"))];
    let overdue = reports::overdue_checkouts(book, today);
    if overdue.is_empty() {
        late.push(text(&t("azerp-none")));
    }
    for o in overdue {
        late.push(text(&t_args(
            "azerp-checkout-overdue",
            &[
                ("number", Arg::from(&o.number)),
                ("name", Arg::from(&o.name)),
                ("custodian", Arg::from(&o.custodian)),
                ("due", Arg::from(crate::model::format_date(o.due))),
                ("days", Arg::from(o.days_late)),
            ],
        )));
    }
    let body = column(vec![
        figures,
        charts,
        column(due).with_id(ids::REPORT_DUE),
        column(late).with_id(ids::REPORT_OVERDUE),
    ])
    .with_id(ids::REPORTS)
    .with_css("padding: 0px 16px; overflow-y: auto;");
    column(vec![header, body])
}

// ==== The depreciation run (DepreciationRunPreview) ====

/// The wizard: the year, then the preview and "Run and post".
pub fn run(s: &Erp, app: &RefAny, view: &View) -> Dom {
    let labels = &s.state.labels;
    let title = view.title(labels, false);
    let steps = spec::steps(view, labels);
    let step = s.state.run_step.min(steps.len().saturating_sub(1));
    let step_title = steps
        .get(step)
        .map(|st| {
            t_args(
                "azerp-step",
                &[
                    ("step", Arg::from(step + 1)),
                    ("steps", Arg::from(steps.len())),
                    ("title", Arg::from(&st.title)),
                ],
            )
        })
        .unwrap_or_default();
    let mut children = vec![
        text(&title)
            .with_id(ids::PAGE_TITLE)
            .with_css("font-size: 16px; font-weight: 600;"),
        text(&step_title).with_css(format!(
            "font-size: 13px; opacity: 0.75; padding-bottom: 8px; {}",
            super::QUIET_FLORA
        )),
    ];
    if step == 0 {
        children.push(
            Dom::create_div()
                .with_css("display: flex; flex-direction: row; align-items: center;")
                .with_child(text(&t("azerp-col-year")).with_css("width: 80px;"))
                .with_child(
                    TextInput::create()
                        .with_text(s.state.run_year.to_string())
                        .with_accessibility_name(t("azerp-col-year"))
                        .with_on_text_input(
                            app.clone(),
                            on_run_year as TextInputOnTextInputCallbackType,
                        )
                        .dom()
                        .with_id(ids::RUN_YEAR),
                )
                .with_child(
                    Button::create(t("azerp-next"))
                        .with_button_type(ButtonType::Primary)
                        .with_on_click(app.clone(), on_run_next as ButtonOnClickCallbackType)
                        .dom()
                        .with_id(ids::RUN_NEXT)
                        .with_css("margin-left: 8px;"),
                ),
        );
    } else {
        let year = s.state.run_year;
        let preview = s.state.run_preview(year);
        let total: i64 = preview.iter().map(|(_, _, _, amount)| amount).sum();
        if preview.is_empty() {
            children.push(text(&t_args(
                "azerp-nothing-depreciated",
                &[("year", Arg::from(year))],
            )));
        }
        for (_, number, name, amount) in &preview {
            children.push(fact(
                &format!("{number} {name}"),
                &money::format_amount(*amount),
            ));
        }
        children.push(
            fact(
                &t_args("azerp-total-year", &[("year", Arg::from(year))]),
                &money::format_amount(total),
            )
            .with_id(ids::RUN_TOTAL),
        );
        let post_label = spec::actions(&view.actions, labels)
            .into_iter()
            .find(|a| a.kind == ActionKind::Submit)
            .map_or_else(|| t("azerp-run-and-post"), |a| a.label);
        children.push(
            Dom::create_div()
                .with_css("display: flex; flex-direction: row; padding-top: 10px;")
                .with_child(
                    Button::create(t("azerp-back"))
                        .with_on_click(app.clone(), on_run_back as ButtonOnClickCallbackType)
                        .dom(),
                )
                .with_child(
                    Button::create(post_label.as_str())
                        .with_button_type(ButtonType::Primary)
                        .with_on_click(app.clone(), on_run_post as ButtonOnClickCallbackType)
                        .dom()
                        .with_id(ids::RUN_POST)
                        .with_css("margin-left: 8px;"),
                ),
        );
    }
    column(children)
        .with_id(ids::RUN)
        .with_css("padding: 12px 16px; overflow-y: auto;")
}

extern "C" fn on_run_year(
    mut data: RefAny,
    _info: CallbackInfo,
    state: TextInputState,
) -> OnTextInputReturn {
    let typed = state.get_text().as_str().trim().to_string();
    let valid = match typed.parse::<i32>() {
        Ok(year) if (1900..=9999).contains(&year) => {
            if let Some(mut s) = data.downcast_mut::<Erp>() {
                s.state.run_year = year;
            }
            TextInputValid::Yes
        }
        _ => TextInputValid::No,
    };
    OnTextInputReturn {
        update: Update::DoNothing,
        valid,
    }
}

extern "C" fn on_run_next(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_erp(&mut data, &mut info, |s, _info| {
        s.state.run_step = 1;
        println!("AZERP_RUN_PREVIEW {}", s.state.run_year);
    })
}

extern "C" fn on_run_back(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_erp(&mut data, &mut info, |s, _info| s.state.run_step = 0)
}

extern "C" fn on_run_post(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_erp(&mut data, &mut info, |s, _info| {
        let year = s.state.run_year;
        println!("AZERP_EXPORTED {}", s.state.post_run(year));
    })
}

// ==== AssetImportPanel ====

/// What a mapping drop-down carries.
struct ColumnRef {
    app: RefAny,
    column: usize,
}

/// The import: choose a file, map its columns, see what it would do, import.
pub fn import(s: &Erp, app: &RefAny, view: &View) -> Dom {
    let title = view.title(&s.state.labels, false);
    let mut children = vec![Dom::create_div()
        .with_css("display: flex; flex-direction: row; align-items: center; padding-bottom: 8px;")
        .with_child(
            text(&title)
                .with_id(ids::PAGE_TITLE)
                .with_css("font-size: 16px; font-weight: 600; flex-grow: 1;"),
        )
        .with_child(
            Button::create(t("azerp-choose-csv"))
                .with_icon("upload_file")
                .with_on_click(app.clone(), on_import_choose as ButtonOnClickCallbackType)
                .dom()
                .with_id(ids::IMPORT_CHOOSE),
        )];
    let Some(draft) = &s.state.import else {
        children.push(text(&t("azerp-import-help")));
        return column(children)
            .with_id(ids::IMPORT)
            .with_css("padding: 12px 16px; overflow-y: auto;");
    };
    let p = &draft.preview;
    children.push(
        text(&t_args(
            "azerp-import-summary",
            &[
                ("file", Arg::from(&draft.name)),
                ("rows", Arg::from(draft.table.rows.len())),
                ("new", Arg::from(p.created)),
                ("changed", Arg::from(p.updated)),
                ("left", Arg::from(p.problems.len())),
            ],
        ))
        .with_id(ids::IMPORT_SUMMARY)
        .with_css("padding-bottom: 8px;"),
    );
    let choices: Vec<AzString> = Field::CHOICES
        .iter()
        .map(|f| AzString::from(f.said()))
        .collect();
    for (i, header) in draft.table.headers.iter().enumerate() {
        let selected = draft.mapping.get(i).map_or(0, |f| f.choice_index());
        children.push(
            Dom::create_div()
                .with_css(
                    "display: flex; flex-direction: row; align-items: center; padding: 2px 0px;",
                )
                .with_child(text(header).with_css("width: 220px; font-size: 13px;"))
                .with_child(
                    DropDown::create(StringVec::from_vec(choices.clone()))
                        .with_selected(selected)
                        .with_accessibility_name(header.as_str())
                        .with_on_choice_change(
                            RefAny::new(ColumnRef {
                                app: app.clone(),
                                column: i,
                            }),
                            on_mapping as DropDownOnChoiceChangeCallbackType,
                        )
                        .dom()
                        .with_id(ids::field(&format!("map-{i}"))),
                ),
        );
    }
    if !p.problems.is_empty() {
        children.push(heading(&t("azerp-rows-left-out")));
        for why in p.problems.iter().take(20) {
            children.push(text(why).with_css("font-size: 12px;"));
        }
    }
    children.push(
        Dom::create_div()
            .with_css("display: flex; flex-direction: row; padding-top: 10px;")
            .with_child(
                Button::create(t_args(
                    "azerp-import-assets",
                    &[("count", Arg::from(p.created + p.updated))],
                ))
                .with_button_type(ButtonType::Primary)
                .with_on_click(app.clone(), on_import_commit as ButtonOnClickCallbackType)
                .dom()
                .with_id(ids::IMPORT_COMMIT),
            ),
    );
    column(children)
        .with_id(ids::IMPORT)
        .with_css("padding: 12px 16px; overflow-y: auto;")
}

extern "C" fn on_import_choose(mut data: RefAny, _info: CallbackInfo) -> Update {
    let app = data.clone();
    if data.downcast_ref::<Erp>().is_none() {
        return Update::DoNothing;
    }
    let _request = FileDialog::open_file(
        t("azerp-import-dialog"),
        OptionString::None,
        OptionFileTypeList::None,
        app,
        on_import_picked,
    );
    Update::DoNothing
}

/// The file picked: read on the file thread (a drive at its folder, outside
/// the data tree); the preview comes with the answer.
extern "C" fn on_import_picked(mut data: RefAny, mut info: CallbackInfo, result: RefAny) -> Update {
    let Some(picked) = FileOpenResult::downcast(result).into_option() else {
        return Update::DoNothing;
    };
    let Some(path) = picked.path.into_option() else {
        return Update::DoNothing; // cancelled
    };
    let path = PathBuf::from(path.as_string().as_str());
    let app = data.clone();
    if !kit::spawn_outside_read(&mut info, &path, app, TAG_IMPORT, super::on_files) {
        if let Some(mut s) = data.downcast_mut::<Erp>() {
            s.state.notice = t_args(
                "azerp-cannot-read",
                &[("path", Arg::from(path.display().to_string()))],
            );
        }
    }
    Update::RefreshDom
}

extern "C" fn on_mapping(mut data: RefAny, mut info: CallbackInfo, choice: usize) -> Update {
    let Some((mut app, column)) = data
        .downcast_ref::<ColumnRef>()
        .map(|c| (c.app.clone(), c.column))
    else {
        return Update::DoNothing;
    };
    let field = Field::CHOICES[choice.min(Field::CHOICES.len() - 1)];
    with_erp(&mut app, &mut info, |s, _info| {
        s.state.set_mapping(column, field)
    })
}

extern "C" fn on_import_commit(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_erp(&mut data, &mut info, |s, _info| {
        let mut new_id = mint;
        let n = s.state.commit_import(&mut new_id);
        println!("AZERP_IMPORTED {n}");
    })
}
