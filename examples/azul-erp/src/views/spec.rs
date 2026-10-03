//! A view as the screen needs it: table columns, typed form fields, buttons,
//! detail tabs, wizard steps - with their labels - and the two pieces of
//! logic the JSON carries: `condition` strings ([`eval_condition`]) and
//! field rules (`required`, `min` / `max`, the type: [`validate`]).

use std::collections::BTreeMap;

use chrono::{Datelike, NaiveDate};

use super::{Action, FieldDef, Labels, View};
use crate::{model, money};

/// How a column shows its cells.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColumnType {
    Text,
    Date,
    /// An amount: grouped, two decimals, right-aligned.
    Currency,
    /// A status pill: the status's label.
    Status,
    Integer,
}

impl ColumnType {
    /// The column type of a JSON column `type`.
    #[must_use]
    pub fn of(kind: Option<&str>) -> ColumnType {
        match kind.unwrap_or("") {
            "date" | "datetime" => ColumnType::Date,
            "currency" | "decimal" | "money" => ColumnType::Currency,
            "status" | "status_pill" | "badge" => ColumnType::Status,
            "integer" | "number" => ColumnType::Integer,
            _ => ColumnType::Text,
        }
    }

    /// The width a column of this type gets when the JSON names none.
    #[must_use]
    pub fn default_width(self) -> f32 {
        match self {
            ColumnType::Text => 160.0,
            ColumnType::Date | ColumnType::Status => 110.0,
            ColumnType::Currency => 120.0,
            ColumnType::Integer => 90.0,
        }
    }
}

/// A table column.
#[derive(Clone, Debug, PartialEq)]
pub struct ColumnSpec {
    /// The record field (`book_value`).
    pub field: String,
    pub title: String,
    pub kind: ColumnType,
    pub width: f32,
    pub sortable: bool,
}

/// The columns of a `table` / `table_embedded` view.
#[must_use]
pub fn columns(view: &View, labels: &Labels) -> Vec<ColumnSpec> {
    let _ = (view, labels);
    todo!("GREEN")
}

/// What kind of input a form field is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FieldKind {
    Text,
    TextArea,
    Password,
    /// `YYYY-MM-DD`.
    Date,
    /// An amount (two decimals).
    Decimal,
    Integer,
    Switch,
    /// A fixed list: `(value, label)`.
    Select(Vec<(String, String)>),
    /// A record of the `api` path's kind (TODO(WIDGETS9B): ReferencePicker).
    Reference(String),
}

/// A form field.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FieldSpec {
    /// The record field (`acquisition_cost`).
    pub name: String,
    pub label: String,
    pub kind: FieldKind,
    pub required: bool,
    pub min: Option<i64>,
    pub max: Option<i64>,
    /// `today`, `this_year` or a value ([`default_text`]).
    pub default: Option<String>,
    pub condition: Option<String>,
}

/// The fields of a form, a wizard step or a report's parameters.
#[must_use]
pub fn fields(defs: &[FieldDef], labels: &Labels) -> Vec<FieldSpec> {
    let _ = (defs, labels);
    todo!("GREEN")
}

/// A new record's text of `field`: its default (`today` is the day,
/// `this_year` the year), else empty.
#[must_use]
pub fn default_text(field: &FieldSpec, today: NaiveDate) -> String {
    let _ = (field, today.year(), model::format_date);
    todo!("GREEN")
}

/// What a button does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ActionKind {
    /// Go to this path (with `:id` filled in).
    Link(String),
    /// Run the named action (`export_csv`).
    Named(String),
    Submit,
    Cancel,
}

/// A button.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActionSpec {
    pub label: String,
    pub kind: ActionKind,
    /// A Material icon name ("" = none).
    pub icon: String,
    pub primary: bool,
    pub condition: Option<String>,
}

/// The buttons of a list of actions.
#[must_use]
pub fn actions(list: &[Action], labels: &Labels) -> Vec<ActionSpec> {
    let _ = (list, labels);
    todo!("GREEN")
}

/// A detail view's tab.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TabSpec {
    pub title: String,
    /// The named panel (`DepreciationSchedulePanel`, `EmbeddedTable`).
    pub component: String,
    /// `EmbeddedTable`: the view it shows.
    pub view: Option<String>,
}

/// The tabs of a `detail` view.
#[must_use]
pub fn tabs(view: &View, labels: &Labels) -> Vec<TabSpec> {
    let _ = (view, labels);
    todo!("GREEN")
}

/// A wizard's step.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StepSpec {
    pub title: String,
    pub fields: Vec<FieldSpec>,
    pub component: Option<String>,
}

/// The steps of a `wizard` view.
#[must_use]
pub fn steps(view: &View, labels: &Labels) -> Vec<StepSpec> {
    let _ = (view, labels);
    todo!("GREEN")
}

/// Whether a `condition` holds: comparisons `field == 'value'` /
/// `field != 'value'` joined by `&&` (all must hold) and `||` (one must),
/// `&&` binding tighter. A condition this grammar cannot read holds (the
/// field or button shows).
#[must_use]
pub fn eval_condition(condition: &str, value_of: &dyn Fn(&str) -> String) -> bool {
    let _ = (condition, value_of);
    todo!("GREEN")
}

/// Whether `field` shows with the form's `values`.
#[must_use]
pub fn visible(field: &FieldSpec, values: &BTreeMap<String, String>) -> bool {
    field.condition.as_deref().map_or(true, |c| {
        eval_condition(c, &|name| values.get(name).cloned().unwrap_or_default())
    })
}

/// What is wrong with a form's `values`, field by field: `(field name,
/// sentence)`. Hidden fields are not checked.
#[must_use]
pub fn validate(fields: &[FieldSpec], values: &BTreeMap<String, String>) -> Vec<(String, String)> {
    let _ = (fields, values, money::parse_amount);
    todo!("GREEN")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::views::ViewFile;

    fn day(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn values(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect()
    }

    #[test]
    fn a_table_view_becomes_columns_with_titles_types_and_widths() {
        let file = ViewFile::assets();
        let cols = columns(file.view("assets_fixed_asset_list").unwrap(), &Labels::en());
        let fields: Vec<&str> = cols.iter().map(|c| c.field.as_str()).collect();
        assert_eq!(
            fields,
            [
                "asset_number",
                "name",
                "category",
                "location",
                "acquisition_date",
                "acquisition_cost",
                "book_value",
                "status"
            ]
        );
        assert_eq!(cols[0].title, "Asset no.");
        assert_eq!(cols[0].width, 96.0);
        assert_eq!(cols[4].kind, ColumnType::Date);
        assert_eq!(cols[5].kind, ColumnType::Currency);
        assert_eq!(cols[7].kind, ColumnType::Status);
        assert!(cols.iter().all(|c| c.sortable));
        // The ERP's own columns name no width: the type's.
        let erp = ViewFile::parse(crate::views::ERP_ASSET_VIEWS).unwrap();
        let erp_cols = columns(erp.view("assets_fixed_asset_list").unwrap(), &Labels::en());
        assert_eq!(erp_cols[4].width, ColumnType::Currency.default_width());
    }

    #[test]
    fn a_form_view_becomes_typed_fields_with_the_columns_the_erp_left_out() {
        let file = ViewFile::assets();
        let form = fields(
            &file.view("assets_fixed_asset_form").unwrap().fields,
            &Labels::en(),
        );
        let find = |name: &str| {
            form.iter()
                .find(|f| f.name == name)
                .unwrap_or_else(|| panic!("{name}"))
        };
        assert_eq!(find("acquisition_date").kind, FieldKind::Date);
        assert!(find("acquisition_date").required);
        assert_eq!(find("acquisition_cost").kind, FieldKind::Decimal);
        assert_eq!(find("residual_value").kind, FieldKind::Decimal);
        assert_eq!(find("useful_life_years").kind, FieldKind::Integer);
        assert_eq!(
            (find("useful_life_years").min, find("useful_life_years").max),
            (Some(0), Some(100))
        );
        assert_eq!(
            find("depreciation_method").kind,
            FieldKind::Select(vec![
                ("STRAIGHT_LINE".into(), "Straight-line".into()),
                ("DECLINING_BALANCE".into(), "Declining balance".into()),
            ])
        );
        assert_eq!(
            find("category_id").kind,
            FieldKind::Reference("/api/assets/categories".into())
        );
        assert_eq!(find("notes").kind, FieldKind::TextArea);
        assert_eq!(find("useful_life_years").label, "Useful life (years)");
        assert_eq!(
            default_text(find("acquisition_date"), day(2026, 10, 3)),
            "2026-10-03"
        );
        assert_eq!(
            default_text(find("depreciation_method"), day(2026, 10, 3)),
            "STRAIGHT_LINE"
        );
        assert_eq!(default_text(find("name"), day(2026, 10, 3)), "");
        let run = steps(
            file.view("assets_depreciation_run_wizard").unwrap(),
            &Labels::en(),
        );
        assert_eq!(run.len(), 2);
        assert_eq!(default_text(&run[0].fields[0], day(2026, 10, 3)), "2026");
        assert_eq!(run[1].component.as_deref(), Some("DepreciationRunPreview"));
        assert_eq!(run[1].title, "Preview");
    }

    #[test]
    fn conditions_read_equals_not_equals_and_or() {
        let v = values(&[("status", "CHECKED_OUT"), ("method", "DECLINING_BALANCE")]);
        let of = |name: &str| v.get(name).cloned().unwrap_or_default();
        assert!(eval_condition("status == 'CHECKED_OUT'", &of));
        assert!(!eval_condition("status != 'CHECKED_OUT'", &of));
        assert!(
            eval_condition("status == \"CHECKED_OUT\"", &of),
            "double quotes too"
        );
        assert!(!eval_condition(
            "status != 'CHECKED_OUT' && status != 'DISPOSED'",
            &of
        ));
        assert!(eval_condition(
            "status == 'IN_USE' || method == 'DECLINING_BALANCE'",
            &of
        ));
        assert!(
            eval_condition("missing == ''", &of),
            "an unknown field is empty"
        );
        assert!(
            eval_condition("total > 5", &of),
            "what the grammar cannot read shows"
        );
        assert!(eval_condition("", &of));
    }

    #[test]
    fn hidden_fields_follow_their_condition() {
        let file = ViewFile::assets();
        let form = fields(
            &file.view("assets_fixed_asset_form").unwrap().fields,
            &Labels::en(),
        );
        let rate = form
            .iter()
            .find(|f| f.name == "declining_rate_percent")
            .unwrap();
        assert!(!visible(
            rate,
            &values(&[("depreciation_method", "STRAIGHT_LINE")])
        ));
        assert!(visible(
            rate,
            &values(&[("depreciation_method", "DECLINING_BALANCE")])
        ));
    }

    #[test]
    fn validation_names_each_wrong_field_in_words() {
        let file = ViewFile::assets();
        let form = fields(
            &file.view("assets_fixed_asset_form").unwrap().fields,
            &Labels::en(),
        );
        let good = values(&[
            ("name", "Laptop"),
            ("asset_number", "A-1"),
            ("acquisition_date", "2026-01-15"),
            ("acquisition_cost", "1,596.64"),
            ("useful_life_years", "3"),
            ("depreciation_method", "STRAIGHT_LINE"),
            ("declining_rate_percent", "nonsense: hidden, not checked"),
        ]);
        assert!(
            validate(&form, &good).is_empty(),
            "{:?}",
            validate(&form, &good)
        );
        let bad = values(&[
            ("name", "  "),
            ("asset_number", "A-1"),
            ("acquisition_date", "2026-02-30"),
            ("acquisition_cost", "-5"),
            ("residual_value", "abc"),
            ("useful_life_years", "3.5"),
            ("depreciation_method", "SUM_OF_YEARS"),
            ("maintenance_interval_months", "601"),
        ]);
        let problems = validate(&form, &bad);
        let named: Vec<&str> = problems.iter().map(|(f, _)| f.as_str()).collect();
        assert_eq!(
            named,
            [
                "name",
                "acquisition_date",
                "acquisition_cost",
                "residual_value",
                "useful_life_years",
                "depreciation_method",
                "maintenance_interval_months"
            ]
        );
        assert_eq!(problems[0].1, "Name is required.");
        assert_eq!(problems[2].1, "Cost must be at least 0.");
        assert_eq!(problems[4].1, "Useful life (years) must be a whole number.");
        assert_eq!(problems[6].1, "Service every (months) must be at most 600.");
    }

    #[test]
    fn the_detail_view_has_its_tabs_and_header_buttons() {
        let file = ViewFile::assets();
        let detail = file.view("assets_fixed_asset_detail").unwrap();
        let t = tabs(detail, &Labels::en());
        let titles: Vec<&str> = t.iter().map(|t| t.title.as_str()).collect();
        assert_eq!(
            titles,
            [
                "Overview",
                "Depreciation schedule",
                "Maintenance",
                "Check-outs"
            ]
        );
        assert_eq!(t[1].component, "DepreciationSchedulePanel");
        assert_eq!(t[2].view.as_deref(), Some("assets_maintenance_embedded"));
        let buttons = actions(&detail.header.as_ref().unwrap().actions, &Labels::en());
        assert_eq!(buttons[0].label, "Edit");
        assert_eq!(
            buttons[0].kind,
            ActionKind::Link("/accounting/assets/:id/edit".into())
        );
        assert_eq!(buttons[0].icon, "edit");
        assert_eq!(buttons[2].kind, ActionKind::Named("check_in".into()));
        assert_eq!(
            buttons[2].condition.as_deref(),
            Some("status == 'CHECKED_OUT'")
        );
        let list = actions(
            &file.view("assets_fixed_asset_list").unwrap().actions,
            &Labels::en(),
        );
        assert!(list[0].primary);
        assert!(!list[1].primary);
        let form = actions(
            &file.view("assets_fixed_asset_form").unwrap().actions,
            &Labels::en(),
        );
        assert_eq!(form[0].kind, ActionKind::Submit);
        assert_eq!(form[1].kind, ActionKind::Cancel);
    }
}
