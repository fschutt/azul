//! The records as the views read and write them: a field by its ERP name
//! ([`ViewRecord::value`] - `book_value`, `category`, `asset_name` are
//! computed from the book), a table's cells ([`grid`]), a form's texts
//! ([`form_values`]) and the form's texts back into the record ([`apply`]).

use std::collections::BTreeMap;

use chrono::NaiveDate;

use super::spec::{self, ColumnSpec, FieldSpec};
use crate::{
    depreciation,
    model::{
        self, Asset, Category, Checkout, Location, MaintenanceEntry, MaintenanceKind, Method,
        Record, Status,
    },
    money,
    store::{Book, Kind},
};

/// A field's value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    Empty,
    Text(String),
    Date(NaiveDate),
    /// Minor units.
    Money(i64),
    Int(i64),
    /// A code and what the user reads (a status, a method, a kind).
    Coded {
        code: String,
        label: String,
    },
}

impl Value {
    /// What a table cell shows.
    #[must_use]
    pub fn display(&self) -> String {
        match self {
            Value::Empty => String::new(),
            Value::Text(t) => t.clone(),
            Value::Date(d) => model::format_date(*d),
            Value::Money(cents) => money::format_amount(*cents),
            Value::Int(n) => n.to_string(),
            Value::Coded { label, .. } => label.clone(),
        }
    }

    /// What a form field holds (the files' notation: `1596.64`,
    /// `2026-01-15`, a code).
    #[must_use]
    pub fn form_text(&self) -> String {
        match self {
            Value::Money(cents) => money::file_amount(*cents),
            Value::Coded { code, .. } => code.clone(),
            other => other.display(),
        }
    }

    /// The number a table sorts and range-filters by (azul's DataTable reads
    /// a Number column's value, and a Date column's as days since
    /// 1970-01-01). An amount is its major units: a sort key, never money
    /// arithmetic. NaN for text.
    #[must_use]
    #[allow(clippy::cast_precision_loss)] // a sort key
    pub fn sort_value(&self) -> f64 {
        match self {
            Value::Date(d) => {
                let epoch = NaiveDate::from_ymd_opt(1970, 1, 1).unwrap_or(NaiveDate::MIN);
                (*d - epoch).num_days() as f64
            }
            Value::Money(cents) => *cents as f64 / money::MINOR_PER_MAJOR as f64,
            Value::Int(n) => *n as f64,
            _ => f64::NAN,
        }
    }
}

/// A text value (`Empty` for "").
fn text(s: &str) -> Value {
    if s.is_empty() {
        Value::Empty
    } else {
        Value::Text(s.to_string())
    }
}

/// A coded value.
fn coded(code: &str, label: &str) -> Value {
    Value::Coded {
        code: code.to_string(),
        label: label.to_string(),
    }
}

/// An amount, `Empty` for 0 (an optional amount).
fn optional_money(cents: i64) -> Value {
    if cents == 0 {
        Value::Empty
    } else {
        Value::Money(cents)
    }
}

/// A count, `Empty` for 0 (an optional count).
fn optional_int(n: u32) -> Value {
    if n == 0 {
        Value::Empty
    } else {
        Value::Int(i64::from(n))
    }
}

/// A day from a form's text.
fn day(t: &str) -> Result<NaiveDate, String> {
    model::parse_date(t).ok_or_else(|| format!("\"{t}\" is not a day"))
}

/// An optional day from a form's text ("" = none).
fn optional_day(t: &str) -> Result<Option<NaiveDate>, String> {
    if t.is_empty() {
        Ok(None)
    } else {
        day(t).map(Some)
    }
}

/// An optional amount from a form's text ("" = 0).
fn amount_or_zero(t: &str) -> Result<i64, String> {
    if t.is_empty() {
        Ok(0)
    } else {
        money::parse_amount(t)
    }
}

/// A whole number from a form's text ("" = 0).
fn count(t: &str) -> Result<u32, String> {
    if t.is_empty() {
        return Ok(0);
    }
    t.parse::<u32>()
        .map_err(|_| format!("\"{t}\" is not a whole number"))
}

/// The asset `id`'s number and name ("" for an unknown one).
fn asset_label<'b>(book: &'b Book, id: &str) -> (&'b str, &'b str) {
    book.get::<Asset>(id)
        .map_or(("", ""), |a| (a.number.as_str(), a.name.as_str()))
}

/// What the computed fields are computed against.
#[derive(Clone, Copy, Debug)]
pub struct Ctx<'a> {
    pub book: &'a Book,
    /// The day a book value is for.
    pub today: NaiveDate,
}

/// A record the views read and write by field name.
pub trait ViewRecord {
    /// The field `field` (an ERP name); [`Value::Empty`] for one it does
    /// not have.
    fn value(&self, field: &str, ctx: &Ctx) -> Value;

    /// Sets `field` from a form's text (validated by the view's field
    /// rules already); `Err` for a field it cannot set or a text it cannot
    /// read.
    fn set(&mut self, field: &str, text: &str) -> Result<(), String>;
}

impl ViewRecord for Asset {
    fn value(&self, field: &str, ctx: &Ctx) -> Value {
        match field {
            "id" => text(&self.id),
            "asset_number" => text(&self.number),
            "name" | "asset_name" => text(&self.name),
            "category" => text(ctx.book.category_name(&self.category)),
            "category_id" => text(&self.category),
            "location" => text(ctx.book.location_name(&self.location)),
            "location_id" => text(&self.location),
            "serial_number" => text(&self.serial),
            "acquisition_date" => Value::Date(self.acquired),
            "acquisition_cost" => Value::Money(self.cost),
            "residual_value" => Value::Money(self.residual),
            "useful_life_years" => Value::Int(i64::from(self.life_years)),
            "depreciation_method" => coded(self.method.code(), self.method.label()),
            // Basis points read as a percentage with two decimals: 2500 = "25.00".
            "declining_rate_percent" => optional_money(i64::from(self.declining_rate_bp)),
            "status" => coded(self.status.code(), self.status.label()),
            "custodian" => text(&self.custodian),
            "maintenance_interval_months" => optional_int(self.maintenance_months),
            "disposal_date" => self.disposed.map_or(Value::Empty, Value::Date),
            "disposal_amount" => optional_money(self.disposal_amount),
            "book_value" => Value::Money(depreciation::book_value_on(self, ctx.today)),
            "notes" => text(&self.notes),
            _ => Value::Empty,
        }
    }

    fn set(&mut self, field: &str, text: &str) -> Result<(), String> {
        let t = text.trim();
        match field {
            "asset_number" => self.number = t.to_string(),
            "name" => self.name = t.to_string(),
            "category_id" => self.category = t.to_string(),
            "location_id" => self.location = t.to_string(),
            "serial_number" => self.serial = t.to_string(),
            "acquisition_date" => self.acquired = day(t)?,
            "acquisition_cost" => self.cost = money::parse_amount(t)?,
            "residual_value" => self.residual = amount_or_zero(t)?,
            "useful_life_years" => self.life_years = count(t)?,
            "depreciation_method" => {
                self.method = Method::parse(t)
                    .ok_or_else(|| format!("\"{t}\" is not a depreciation method"))?;
            }
            "declining_rate_percent" => {
                self.declining_rate_bp = u32::try_from(amount_or_zero(t)?)
                    .map_err(|_| format!("\"{t}\" is not a rate"))?;
            }
            "status" => {
                self.status = Status::parse(t).ok_or_else(|| format!("\"{t}\" is not a status"))?;
            }
            "custodian" => self.custodian = t.to_string(),
            "maintenance_interval_months" => self.maintenance_months = count(t)?,
            "disposal_date" => self.disposed = optional_day(t)?,
            "disposal_amount" => self.disposal_amount = amount_or_zero(t)?,
            "notes" => self.notes = text.to_string(),
            _ => return Err(format!("an asset has no field \"{field}\"")),
        }
        Ok(())
    }
}

impl ViewRecord for Category {
    fn value(&self, field: &str, ctx: &Ctx) -> Value {
        match field {
            "id" => text(&self.id),
            "name" => text(&self.name),
            "useful_life_years" => Value::Int(i64::from(self.life_years)),
            "depreciation_method" => coded(self.method.code(), self.method.label()),
            "notes" => text(&self.notes),
            "asset_count" => {
                let n = ctx
                    .book
                    .assets
                    .iter()
                    .filter(|a| a.category == self.id)
                    .count();
                Value::Int(i64::try_from(n).unwrap_or(i64::MAX))
            }
            _ => Value::Empty,
        }
    }

    fn set(&mut self, field: &str, text: &str) -> Result<(), String> {
        let t = text.trim();
        match field {
            "name" => self.name = t.to_string(),
            "useful_life_years" => self.life_years = count(t)?,
            "depreciation_method" => {
                self.method = Method::parse(t)
                    .ok_or_else(|| format!("\"{t}\" is not a depreciation method"))?;
            }
            "notes" => self.notes = text.to_string(),
            _ => return Err(format!("a category has no field \"{field}\"")),
        }
        Ok(())
    }
}

impl ViewRecord for Location {
    fn value(&self, field: &str, ctx: &Ctx) -> Value {
        match field {
            "id" => text(&self.id),
            "name" => text(&self.name),
            "address" => text(&self.address),
            "notes" => text(&self.notes),
            "asset_count" => {
                let n = ctx
                    .book
                    .assets
                    .iter()
                    .filter(|a| a.location == self.id)
                    .count();
                Value::Int(i64::try_from(n).unwrap_or(i64::MAX))
            }
            _ => Value::Empty,
        }
    }

    fn set(&mut self, field: &str, text: &str) -> Result<(), String> {
        let t = text.trim();
        match field {
            "name" => self.name = t.to_string(),
            "address" => self.address = t.to_string(),
            "notes" => self.notes = text.to_string(),
            _ => return Err(format!("a location has no field \"{field}\"")),
        }
        Ok(())
    }
}

impl ViewRecord for MaintenanceEntry {
    fn value(&self, field: &str, ctx: &Ctx) -> Value {
        match field {
            "id" => text(&self.id),
            "asset_id" => text(&self.asset),
            "asset_number" => text(asset_label(ctx.book, &self.asset).0),
            "asset_name" => text(asset_label(ctx.book, &self.asset).1),
            "date" => Value::Date(self.date),
            "kind" => coded(self.kind.code(), self.kind.label()),
            "description" => text(&self.description),
            "cost" => Value::Money(self.cost),
            "performed_by" => text(&self.by),
            _ => Value::Empty,
        }
    }

    fn set(&mut self, field: &str, text: &str) -> Result<(), String> {
        let t = text.trim();
        match field {
            "asset_id" => self.asset = t.to_string(),
            "date" => self.date = day(t)?,
            "kind" => {
                self.kind = MaintenanceKind::parse(t)
                    .ok_or_else(|| format!("\"{t}\" is not a kind of maintenance"))?;
            }
            "description" => self.description = t.to_string(),
            "cost" => self.cost = amount_or_zero(t)?,
            "performed_by" => self.by = t.to_string(),
            _ => return Err(format!("a maintenance entry has no field \"{field}\"")),
        }
        Ok(())
    }
}

impl ViewRecord for Checkout {
    fn value(&self, field: &str, ctx: &Ctx) -> Value {
        match field {
            "id" => text(&self.id),
            "asset_id" => text(&self.asset),
            "asset_number" => text(asset_label(ctx.book, &self.asset).0),
            "asset_name" => text(asset_label(ctx.book, &self.asset).1),
            "custodian" => text(&self.custodian),
            "checked_out" => Value::Date(self.out),
            "due_date" => self.due.map_or(Value::Empty, Value::Date),
            "checked_in" => self.returned.map_or(Value::Empty, Value::Date),
            "note" => text(&self.note),
            _ => Value::Empty,
        }
    }

    fn set(&mut self, field: &str, text: &str) -> Result<(), String> {
        let t = text.trim();
        match field {
            "asset_id" => self.asset = t.to_string(),
            "custodian" => self.custodian = t.to_string(),
            "checked_out" => self.out = day(t)?,
            "due_date" => self.due = optional_day(t)?,
            "checked_in" => self.returned = optional_day(t)?,
            "note" => self.note = t.to_string(),
            _ => return Err(format!("a check-out has no field \"{field}\"")),
        }
        Ok(())
    }
}

/// The texts a form shows for `record`.
#[must_use]
pub fn form_values<R: ViewRecord>(
    record: &R,
    fields: &[FieldSpec],
    ctx: &Ctx,
) -> BTreeMap<String, String> {
    fields
        .iter()
        .map(|f| (f.name.clone(), record.value(&f.name, ctx).form_text()))
        .collect()
}

/// A form's `values` into `record`: validated by the fields' rules first
/// ([`spec::validate`]); nothing changes when anything is wrong. Hidden
/// fields keep the record's value.
pub fn apply<R: ViewRecord + Clone>(
    record: &mut R,
    fields: &[FieldSpec],
    values: &BTreeMap<String, String>,
) -> Result<(), Vec<(String, String)>> {
    let problems = spec::validate(fields, values);
    if !problems.is_empty() {
        return Err(problems);
    }
    let mut next = record.clone();
    let mut errors = Vec::new();
    for f in fields.iter().filter(|f| spec::visible(f, values)) {
        let Some(text) = values.get(&f.name) else {
            continue;
        };
        if let Err(e) = next.set(&f.name, text) {
            errors.push((f.name.clone(), e));
        }
    }
    if errors.is_empty() {
        *record = next;
        Ok(())
    } else {
        Err(errors)
    }
}

/// A table's cells: the record ids (in the book's order) and, per row and
/// column, the shown text and the sort value.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Grid {
    pub ids: Vec<String>,
    pub text: Vec<Vec<String>>,
    pub sort: Vec<Vec<f64>>,
}

impl Grid {
    /// Rows.
    #[must_use]
    pub fn len(&self) -> usize {
        self.ids.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }
}

/// The cells of a table of `kind` records with `columns`, only the rows
/// whose field `filter.0` holds `filter.1` (an embedded table's parent).
#[must_use]
pub fn grid(kind: Kind, columns: &[ColumnSpec], filter: Option<(&str, &str)>, ctx: &Ctx) -> Grid {
    match kind {
        Kind::Asset => grid_of(&ctx.book.assets, columns, filter, ctx),
        Kind::Category => grid_of(&ctx.book.categories, columns, filter, ctx),
        Kind::Location => grid_of(&ctx.book.locations, columns, filter, ctx),
        Kind::Maintenance => grid_of(&ctx.book.maintenance, columns, filter, ctx),
        Kind::Checkout => grid_of(&ctx.book.checkouts, columns, filter, ctx),
    }
}

/// [`grid`] over one kind's records.
fn grid_of<R: ViewRecord + Record>(
    records: &[R],
    columns: &[ColumnSpec],
    filter: Option<(&str, &str)>,
    ctx: &Ctx,
) -> Grid {
    let mut g = Grid::default();
    for r in records {
        if let Some((field, wanted)) = filter {
            if r.value(field, ctx).form_text() != wanted {
                continue;
            }
        }
        let values: Vec<Value> = columns.iter().map(|c| r.value(&c.field, ctx)).collect();
        g.ids.push(r.id().to_string());
        g.text.push(values.iter().map(Value::display).collect());
        g.sort.push(values.iter().map(Value::sort_value).collect());
    }
    g
}

/// The choices of a `Reference` field: `(id, name)` of the records its
/// `api` path names, by name; the first choice is "(none)" with id "".
#[must_use]
pub fn reference_choices(source: &str, book: &Book) -> Vec<(String, String)> {
    let mut choices: Vec<(String, String)> = match super::api_kind(source) {
        Some(Kind::Category) => book
            .categories
            .iter()
            .map(|c| (c.id.clone(), c.name.clone()))
            .collect(),
        Some(Kind::Location) => book
            .locations
            .iter()
            .map(|l| (l.id.clone(), l.name.clone()))
            .collect(),
        Some(Kind::Asset) => book
            .assets
            .iter()
            .map(|a| (a.id.clone(), format!("{} {}", a.number, a.name)))
            .collect(),
        _ => Vec::new(),
    };
    choices.sort_by_key(|(_, name)| name.to_lowercase());
    choices.insert(0, (String::new(), "(none)".to_string()));
    choices
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::views::{spec::fields, Labels, ViewFile};

    fn day(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn book() -> Book {
        let mut b = Book::default();
        b.put(Category {
            id: "c-it".into(),
            name: "IT equipment".into(),
            life_years: 3,
            method: Method::StraightLine,
            notes: String::new(),
        });
        b.put(Location {
            id: "l-hq".into(),
            name: "Head office".into(),
            address: "Musterstr. 1".into(),
            notes: String::new(),
        });
        let mut a = Asset::new("a1", "A-0042", "ThinkPad", day(2026, 1, 15), 159_664, 3);
        a.category = "c-it".into();
        a.location = "l-hq".into();
        a.status = Status::CheckedOut;
        b.put(a);
        b.put(Asset::new(
            "a2",
            "A-0043",
            "Desk",
            day(2026, 2, 1),
            45_000,
            13,
        ));
        b.put(MaintenanceEntry {
            id: "m1".into(),
            asset: "a1".into(),
            date: day(2026, 6, 1),
            kind: MaintenanceKind::Repair,
            description: "New keyboard".into(),
            cost: 8_950,
            by: "Service desk".into(),
        });
        b.put(MaintenanceEntry {
            id: "m2".into(),
            asset: "a2".into(),
            date: day(2026, 7, 1),
            kind: MaintenanceKind::Service,
            description: String::new(),
            cost: 0,
            by: String::new(),
        });
        b
    }

    #[test]
    fn an_asset_gives_its_fields_by_the_erp_names_with_the_computed_ones() {
        let b = book();
        let ctx = Ctx {
            book: &b,
            today: day(2026, 12, 31),
        };
        let a = b.get::<Asset>("a1").unwrap();
        assert_eq!(a.value("asset_number", &ctx), Value::Text("A-0042".into()));
        assert_eq!(a.value("acquisition_cost", &ctx), Value::Money(159_664));
        assert_eq!(a.value("book_value", &ctx), Value::Money(106_443));
        assert_eq!(
            a.value("category", &ctx),
            Value::Text("IT equipment".into())
        );
        assert_eq!(a.value("category_id", &ctx), Value::Text("c-it".into()));
        assert_eq!(a.value("location", &ctx).display(), "Head office");
        assert_eq!(
            a.value("acquisition_date", &ctx),
            Value::Date(day(2026, 1, 15))
        );
        assert_eq!(a.value("status", &ctx).display(), "Checked out");
        assert_eq!(a.value("status", &ctx).form_text(), "CHECKED_OUT");
        assert_eq!(
            a.value("depreciation_method", &ctx).display(),
            "Straight-line"
        );
        assert_eq!(a.value("disposal_date", &ctx), Value::Empty);
        assert_eq!(a.value("colour", &ctx), Value::Empty);
        assert_eq!(a.value("acquisition_cost", &ctx).display(), "1,596.64");
        assert_eq!(a.value("acquisition_cost", &ctx).form_text(), "1596.64");
        assert_eq!(a.value("acquisition_cost", &ctx).sort_value(), 1596.64);
        assert_eq!(a.value("acquisition_date", &ctx).display(), "2026-01-15");
        assert_eq!(
            a.value("acquisition_date", &ctx).sort_value(),
            20_468.0,
            "days since 1970"
        );
        assert!(a.value("name", &ctx).sort_value().is_nan());
        let c = b.get::<Category>("c-it").unwrap();
        assert_eq!(c.value("asset_count", &ctx), Value::Int(1));
        let m = b.get::<MaintenanceEntry>("m1").unwrap();
        assert_eq!(m.value("asset_name", &ctx), Value::Text("ThinkPad".into()));
        assert_eq!(m.value("kind", &ctx).display(), "Repair");
    }

    #[test]
    fn a_form_round_trips_a_record_through_its_texts() {
        let b = book();
        let ctx = Ctx {
            book: &b,
            today: day(2026, 10, 3),
        };
        let file = ViewFile::assets();
        let form = fields(
            &file.view("assets_fixed_asset_form").unwrap().fields,
            &Labels::en(),
        );
        let mut original = b.get::<Asset>("a1").unwrap().clone();
        original.method = Method::DecliningBalance;
        original.declining_rate_bp = 2500;
        original.residual = 1_000;
        original.serial = "PF-2XK91".into();
        original.maintenance_months = 12;
        original.notes = "Line 1\nLine 2".into();
        let texts = form_values(&original, &form, &ctx);
        assert_eq!(texts["acquisition_cost"], "1596.64");
        assert_eq!(texts["declining_rate_percent"], "25.00");
        assert_eq!(texts["category_id"], "c-it");
        let mut fresh = Asset::new("a1", "", "", day(2000, 1, 1), 0, 0);
        fresh.status = Status::CheckedOut;
        apply(&mut fresh, &form, &texts).unwrap();
        assert_eq!(fresh, original);
    }

    #[test]
    fn a_wrong_form_changes_nothing_and_says_why() {
        let b = book();
        let file = ViewFile::assets();
        let form = fields(
            &file.view("assets_fixed_asset_form").unwrap().fields,
            &Labels::en(),
        );
        let ctx = Ctx {
            book: &b,
            today: day(2026, 10, 3),
        };
        let original = b.get::<Asset>("a2").unwrap().clone();
        let mut texts = form_values(&original, &form, &ctx);
        texts.insert("name".into(), "Standing desk".into());
        texts.insert("acquisition_cost".into(), "lots".into());
        let mut changed = original.clone();
        let problems = apply(&mut changed, &form, &texts).unwrap_err();
        assert_eq!(problems.len(), 1);
        assert_eq!(problems[0].0, "acquisition_cost");
        assert_eq!(changed, original, "nothing changed");
    }

    #[test]
    fn a_grid_shows_formatted_cells_and_an_embedded_one_its_parents_rows() {
        let b = book();
        let ctx = Ctx {
            book: &b,
            today: day(2026, 12, 31),
        };
        let file = ViewFile::assets();
        let labels = Labels::en();
        let cols = spec::columns(file.view("assets_fixed_asset_list").unwrap(), &labels);
        let g = grid(Kind::Asset, &cols, None, &ctx);
        assert_eq!(g.ids, ["a1", "a2"]);
        assert_eq!(
            g.text[0],
            [
                "A-0042",
                "ThinkPad",
                "IT equipment",
                "Head office",
                "2026-01-15",
                "1,596.64",
                "1,064.43",
                "Checked out"
            ]
        );
        assert_eq!(g.sort[0][5], 1596.64);
        let log = spec::columns(file.view("assets_maintenance_embedded").unwrap(), &labels);
        let g = grid(Kind::Maintenance, &log, Some(("asset_id", "a1")), &ctx);
        assert_eq!(g.ids, ["m1"]);
        assert_eq!(
            g.text[0],
            [
                "2026-06-01",
                "Repair",
                "New keyboard",
                "89.50",
                "Service desk"
            ]
        );
        assert!(grid(Kind::Checkout, &log, None, &ctx).is_empty());
    }

    #[test]
    fn a_reference_field_offers_the_records_by_name_after_none() {
        let mut b = book();
        b.put(Category {
            id: "c-auto".into(),
            name: "Vehicles".into(),
            life_years: 6,
            method: Method::DecliningBalance,
            notes: String::new(),
        });
        b.put(Category {
            id: "c-a".into(),
            name: "buildings".into(),
            life_years: 33,
            method: Method::StraightLine,
            notes: String::new(),
        });
        assert_eq!(
            reference_choices("/api/assets/categories", &b),
            [
                (String::new(), "(none)".to_string()),
                ("c-a".into(), "buildings".into()),
                ("c-it".into(), "IT equipment".into()),
                ("c-auto".into(), "Vehicles".into()),
            ]
        );
        assert_eq!(reference_choices("/api/assets/locations", &b).len(), 2);
        assert_eq!(
            reference_choices("/api/assets/fixed-assets", &b)[1].1,
            "A-0042 ThinkPad"
        );
        assert_eq!(reference_choices("/api/sales/invoices", &b).len(), 1);
    }

    #[test]
    fn categories_locations_maintenance_and_checkouts_take_their_forms() {
        let file = ViewFile::assets();
        let labels = Labels::en();
        let b = Book::default();
        let ctx = Ctx {
            book: &b,
            today: day(2026, 10, 3),
        };
        let form = fields(&file.view("assets_category_form").unwrap().fields, &labels);
        let mut c = Category {
            id: "c".into(),
            name: String::new(),
            life_years: 0,
            method: Method::StraightLine,
            notes: String::new(),
        };
        let mut texts = form_values(&c, &form, &ctx);
        texts.insert("name".into(), "Vehicles".into());
        texts.insert("useful_life_years".into(), "6".into());
        texts.insert("depreciation_method".into(), "DECLINING_BALANCE".into());
        apply(&mut c, &form, &texts).unwrap();
        assert_eq!(
            (c.name.as_str(), c.life_years, c.method),
            ("Vehicles", 6, Method::DecliningBalance)
        );

        let form = fields(
            &file.view("assets_maintenance_form").unwrap().fields,
            &labels,
        );
        let mut m = MaintenanceEntry {
            id: "m".into(),
            asset: "a1".into(),
            date: day(2026, 1, 1),
            kind: MaintenanceKind::Service,
            description: String::new(),
            cost: 0,
            by: String::new(),
        };
        let mut texts = form_values(&m, &form, &ctx);
        texts.insert("date".into(), "2026-09-01".into());
        texts.insert("kind".into(), "INSPECTION".into());
        texts.insert("cost".into(), "49,00".into());
        apply(&mut m, &form, &texts).unwrap();
        assert_eq!(
            (m.date, m.kind, m.cost),
            (day(2026, 9, 1), MaintenanceKind::Inspection, 4_900)
        );

        let form = fields(&file.view("assets_checkout_form").unwrap().fields, &labels);
        let mut k = Checkout {
            id: "k".into(),
            asset: "a1".into(),
            custodian: String::new(),
            out: day(2026, 1, 1),
            due: None,
            returned: None,
            note: String::new(),
        };
        let mut texts = form_values(&k, &form, &ctx);
        texts.insert("custodian".into(), "Grace Hopper".into());
        texts.insert("due_date".into(), "2026-10-31".into());
        apply(&mut k, &form, &texts).unwrap();
        assert_eq!(k.custodian, "Grace Hopper");
        assert_eq!(k.due, Some(day(2026, 10, 31)));
        texts.insert("due_date".into(), String::new());
        apply(&mut k, &form, &texts).unwrap();
        assert_eq!(k.due, None, "an empty optional day clears it");

        let form = fields(&file.view("assets_location_form").unwrap().fields, &labels);
        let mut l = Location {
            id: "l".into(),
            name: String::new(),
            address: String::new(),
            notes: String::new(),
        };
        let mut texts = form_values(&l, &form, &ctx);
        texts.insert("name".into(), "Warehouse".into());
        apply(&mut l, &form, &texts).unwrap();
        assert_eq!(l.name, "Warehouse");
        let _ = model::VERSION;
    }
}
