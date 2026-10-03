//! The records of asset management, and their files.
//!
//! Every record is ONE JSON file in the data tree (the S3 split: durable
//! data are files; [`crate::store`] names the keys). The files use the ERP
//! schema's field names (`asset_number`, `acquisition_cost`,
//! `useful_life_years`, `depreciation_method`, ...), so the view JSON's
//! `data_field`s name them as they are, and every file starts with a
//! `format` and a `version`:
//!
//! ```json
//! { "format": "azerp.asset", "version": 1,
//!   "id": "6f1c...", "asset_number": "A-0042", "name": "ThinkPad X1",
//!   "acquisition_date": "2026-01-15", "acquisition_cost": "1596.64",
//!   "residual_value": "0.00", "useful_life_years": 3,
//!   "depreciation_method": "STRAIGHT_LINE", "status": "IN_USE" }
//! ```
//!
//! Money is a decimal STRING with two decimals (exact, never a JSON float;
//! [`crate::money::file_amount`]), dates are `YYYY-MM-DD`. Empty optional
//! fields are left out. A file of another format is not this app's; a file
//! with a higher `version` was written by a newer AzERP and is left alone,
//! never guessed at; fields this version does not know are ignored.

use chrono::NaiveDate;
use serde::{de::DeserializeOwned, Deserialize, Serialize};

/// The file version this AzERP writes, and the newest it reads.
pub const VERSION: u64 = 1;

/// The day format of every file.
pub const DATE_FORMAT: &str = "%Y-%m-%d";

/// 100 % in basis points.
pub const FULL_RATE_BP: u32 = 10_000;

/// The longest useful life an asset may have.
pub const MAX_LIFE_YEARS: u32 = 100;

/// A record kind: its file `format`, its folder under `erp/`, its id.
pub trait Record: Serialize + DeserializeOwned + Clone {
    /// The `format` its files carry (`azerp.asset`).
    const FORMAT: &'static str;
    /// Its folder under the app's folder (`assets`).
    const FOLDER: &'static str;
    /// Its id: a version 4 UUID, the name of its file.
    fn id(&self) -> &str;
}

// ==== Enumerations (their codes are the ERP schema's) ====

/// How an asset loses value.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Method {
    /// The same share of the cost every year (monthly pro rata).
    #[default]
    StraightLine,
    /// A fixed rate of the book value every year, switching to straight-line
    /// over the remaining life when that is more.
    DecliningBalance,
}

impl Method {
    pub const ALL: [Method; 2] = [Method::StraightLine, Method::DecliningBalance];

    /// The schema's code: `STRAIGHT_LINE`.
    #[must_use]
    pub fn code(self) -> &'static str {
        match self {
            Method::StraightLine => "STRAIGHT_LINE",
            Method::DecliningBalance => "DECLINING_BALANCE",
        }
    }

    /// What the user reads: `Straight-line`.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Method::StraightLine => "Straight-line",
            Method::DecliningBalance => "Declining balance",
        }
    }

    /// A code, a label or a common name (`linear`, `degressive`, any case).
    #[must_use]
    pub fn parse(text: &str) -> Option<Method> {
        let t = text.trim().to_lowercase().replace(['-', '_'], " ");
        match t.as_str() {
            "straight line" | "straightline" | "linear" | "linear depreciation" | "sl" => {
                Some(Method::StraightLine)
            }
            "declining balance"
            | "declining"
            | "double declining"
            | "double declining balance"
            | "reducing balance"
            | "degressive"
            | "degressiv"
            | "db" => Some(Method::DecliningBalance),
            _ => None,
        }
    }
}

/// Where an asset is in its life.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Status {
    #[default]
    InUse,
    InStorage,
    /// Someone has it (the open check-out names who).
    CheckedOut,
    InRepair,
    /// Sold or scrapped (`disposal_date`).
    Disposed,
}

impl Status {
    pub const ALL: [Status; 5] = [
        Status::InUse,
        Status::InStorage,
        Status::CheckedOut,
        Status::InRepair,
        Status::Disposed,
    ];

    /// The schema's code: `IN_USE`.
    #[must_use]
    pub fn code(self) -> &'static str {
        match self {
            Status::InUse => "IN_USE",
            Status::InStorage => "IN_STORAGE",
            Status::CheckedOut => "CHECKED_OUT",
            Status::InRepair => "IN_REPAIR",
            Status::Disposed => "DISPOSED",
        }
    }

    /// What the user reads: `In use`.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Status::InUse => "In use",
            Status::InStorage => "In storage",
            Status::CheckedOut => "Checked out",
            Status::InRepair => "In repair",
            Status::Disposed => "Disposed",
        }
    }

    /// A code or a label, any case.
    #[must_use]
    pub fn parse(text: &str) -> Option<Status> {
        let t = text.trim();
        Status::ALL
            .into_iter()
            .find(|s| s.code().eq_ignore_ascii_case(t) || s.label().eq_ignore_ascii_case(t))
    }
}

/// What a maintenance entry was.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MaintenanceKind {
    #[default]
    Service,
    Inspection,
    Repair,
    Calibration,
    Other,
}

impl MaintenanceKind {
    pub const ALL: [MaintenanceKind; 5] = [
        MaintenanceKind::Service,
        MaintenanceKind::Inspection,
        MaintenanceKind::Repair,
        MaintenanceKind::Calibration,
        MaintenanceKind::Other,
    ];

    /// The code the files hold: `SERVICE`.
    #[must_use]
    pub fn code(self) -> &'static str {
        match self {
            MaintenanceKind::Service => "SERVICE",
            MaintenanceKind::Inspection => "INSPECTION",
            MaintenanceKind::Repair => "REPAIR",
            MaintenanceKind::Calibration => "CALIBRATION",
            MaintenanceKind::Other => "OTHER",
        }
    }

    /// A code or a label, any case.
    #[must_use]
    pub fn parse(text: &str) -> Option<MaintenanceKind> {
        let t = text.trim();
        MaintenanceKind::ALL
            .into_iter()
            .find(|k| k.code().eq_ignore_ascii_case(t) || k.label().eq_ignore_ascii_case(t))
    }

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            MaintenanceKind::Service => "Service",
            MaintenanceKind::Inspection => "Inspection",
            MaintenanceKind::Repair => "Repair",
            MaintenanceKind::Calibration => "Calibration",
            MaintenanceKind::Other => "Other",
        }
    }
}

// ==== The records ====

/// A fixed asset (`fixed_assets` in the ERP schema).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Asset {
    pub id: String,
    /// The inventory number on its tag: `A-0042`.
    #[serde(rename = "asset_number")]
    pub number: String,
    pub name: String,
    /// A category's id ("" = none).
    #[serde(
        rename = "category_id",
        default,
        skip_serializing_if = "String::is_empty"
    )]
    pub category: String,
    /// A location's id ("" = none).
    #[serde(
        rename = "location_id",
        default,
        skip_serializing_if = "String::is_empty"
    )]
    pub location: String,
    #[serde(
        rename = "serial_number",
        default,
        skip_serializing_if = "String::is_empty"
    )]
    pub serial: String,
    /// The day it was acquired: depreciation starts in this month.
    #[serde(rename = "acquisition_date", with = "date_text")]
    pub acquired: NaiveDate,
    /// What it cost, in minor units.
    #[serde(rename = "acquisition_cost", with = "amount_text")]
    pub cost: i64,
    /// What it is still worth at the end of its life, in minor units.
    #[serde(rename = "residual_value", with = "amount_text", default)]
    pub residual: i64,
    /// Its useful life; 0 = written off in the year it was acquired (a
    /// low-value asset).
    #[serde(rename = "useful_life_years")]
    pub life_years: u32,
    #[serde(rename = "depreciation_method", default)]
    pub method: Method,
    /// The declining-balance rate in basis points (2500 = 25 %); 0 = twice
    /// the straight-line rate ([`Asset::rate_bp`]).
    #[serde(rename = "declining_rate_bp", default, skip_serializing_if = "is_zero")]
    pub declining_rate_bp: u32,
    #[serde(default)]
    pub status: Status,
    /// Who has it while it is checked out.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub custodian: String,
    /// Months between two services (0 = no plan): the next one is due that
    /// long after the last entry of the maintenance log.
    #[serde(
        rename = "maintenance_interval_months",
        default,
        skip_serializing_if = "is_zero"
    )]
    pub maintenance_months: u32,
    /// The day it was sold or scrapped: depreciation stops in that month.
    #[serde(
        rename = "disposal_date",
        with = "opt_date_text",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub disposed: Option<NaiveDate>,
    /// What the sale brought, in minor units.
    #[serde(
        rename = "disposal_amount",
        with = "amount_text",
        default,
        skip_serializing_if = "is_zero_amount"
    )]
    pub disposal_amount: i64,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub notes: String,
}

impl Asset {
    /// A new asset in use: straight-line, no residual value, nothing else set.
    #[must_use]
    pub fn new(
        id: &str,
        number: &str,
        name: &str,
        acquired: NaiveDate,
        cost: i64,
        life_years: u32,
    ) -> Asset {
        Asset {
            id: id.to_string(),
            number: number.to_string(),
            name: name.to_string(),
            category: String::new(),
            location: String::new(),
            serial: String::new(),
            acquired,
            cost,
            residual: 0,
            life_years,
            method: Method::StraightLine,
            declining_rate_bp: 0,
            status: Status::InUse,
            custodian: String::new(),
            maintenance_months: 0,
            disposed: None,
            disposal_amount: 0,
            notes: String::new(),
        }
    }

    /// The declining-balance rate in basis points: the asset's own, else
    /// twice the straight-line rate (at most 100 %).
    #[must_use]
    pub fn rate_bp(&self) -> u32 {
        if self.declining_rate_bp > 0 {
            return self.declining_rate_bp;
        }
        if self.life_years == 0 {
            return FULL_RATE_BP;
        }
        (2 * FULL_RATE_BP / self.life_years).min(FULL_RATE_BP)
    }

    /// What is wrong with the asset, one sentence each (empty = it can be
    /// saved).
    #[must_use]
    pub fn problems(&self) -> Vec<String> {
        let mut p: Vec<String> = Vec::new();
        if self.name.trim().is_empty() {
            p.push("The asset needs a name.".into());
        }
        if self.number.trim().is_empty() {
            p.push("The asset needs an asset number.".into());
        }
        if self.cost < 0 {
            p.push("The cost cannot be negative.".into());
        }
        if self.residual < 0 || (self.cost >= 0 && self.residual > self.cost) {
            p.push("The residual value must be between 0 and the cost.".into());
        }
        if self.life_years > MAX_LIFE_YEARS {
            p.push(format!(
                "The useful life is at most {MAX_LIFE_YEARS} years."
            ));
        }
        if self.declining_rate_bp > FULL_RATE_BP {
            p.push("The declining-balance rate is at most 100 %.".into());
        }
        if self.disposal_amount < 0 {
            p.push("The disposal amount cannot be negative.".into());
        }
        if self.disposed.is_some_and(|d| d < self.acquired) {
            p.push("The disposal date is before the acquisition date.".into());
        }
        p
    }
}

impl Record for Asset {
    const FORMAT: &'static str = "azerp.asset";
    const FOLDER: &'static str = "assets";
    fn id(&self) -> &str {
        &self.id
    }
}

/// A kind of asset, with the life and method new assets of it start with.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Category {
    pub id: String,
    pub name: String,
    #[serde(rename = "useful_life_years", default)]
    pub life_years: u32,
    #[serde(rename = "depreciation_method", default)]
    pub method: Method,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub notes: String,
}

impl Record for Category {
    const FORMAT: &'static str = "azerp.category";
    const FOLDER: &'static str = "categories";
    fn id(&self) -> &str {
        &self.id
    }
}

/// Where assets are: a site, a building, a room.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Location {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub address: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub notes: String,
}

impl Record for Location {
    const FORMAT: &'static str = "azerp.location";
    const FOLDER: &'static str = "locations";
    fn id(&self) -> &str {
        &self.id
    }
}

/// One entry of an asset's maintenance log.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MaintenanceEntry {
    pub id: String,
    #[serde(rename = "asset_id")]
    pub asset: String,
    #[serde(with = "date_text")]
    pub date: NaiveDate,
    #[serde(default)]
    pub kind: MaintenanceKind,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// What it cost, in minor units.
    #[serde(with = "amount_text", default)]
    pub cost: i64,
    #[serde(
        rename = "performed_by",
        default,
        skip_serializing_if = "String::is_empty"
    )]
    pub by: String,
}

impl Record for MaintenanceEntry {
    const FORMAT: &'static str = "azerp.maintenance";
    const FOLDER: &'static str = "maintenance";
    fn id(&self) -> &str {
        &self.id
    }
}

/// One check-out of an asset to a person, open until it is checked in.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Checkout {
    pub id: String,
    #[serde(rename = "asset_id")]
    pub asset: String,
    pub custodian: String,
    #[serde(rename = "checked_out", with = "date_text")]
    pub out: NaiveDate,
    #[serde(
        rename = "due_date",
        with = "opt_date_text",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub due: Option<NaiveDate>,
    #[serde(
        rename = "checked_in",
        with = "opt_date_text",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub returned: Option<NaiveDate>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub note: String,
}

impl Checkout {
    /// Still out.
    #[must_use]
    pub fn is_open(&self) -> bool {
        self.returned.is_none()
    }
}

impl Record for Checkout {
    const FORMAT: &'static str = "azerp.checkout";
    const FOLDER: &'static str = "checkouts";
    fn id(&self) -> &str {
        &self.id
    }
}

// ==== Files ====

/// Why a file was not read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FileError {
    /// Not JSON at all.
    NotJson(String),
    /// JSON, but not this kind of AzERP file (`format` missing or different).
    WrongFormat,
    /// Written by a newer AzERP.
    Newer(u64),
    /// The right kind, but a field is wrong.
    Invalid(String),
}

impl std::fmt::Display for FileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FileError::NotJson(e) => write!(f, "not a JSON file ({e})"),
            FileError::WrongFormat => write!(f, "not an AzERP record of this kind"),
            FileError::Newer(v) => write!(f, "written by a newer AzERP (version {v}); left alone"),
            FileError::Invalid(e) => write!(f, "a field is wrong ({e})"),
        }
    }
}

/// The record's file: `format`, `version`, then its fields.
#[must_use]
pub fn to_json<R: Record>(record: &R) -> String {
    /// The header in front of the record's own fields.
    #[derive(Serialize)]
    struct Out<'a, T: Serialize> {
        format: &'a str,
        version: u64,
        #[serde(flatten)]
        record: &'a T,
    }
    let out = Out {
        format: R::FORMAT,
        version: VERSION,
        record,
    };
    let mut text = serde_json::to_string_pretty(&out).unwrap_or_default();
    text.push('\n');
    text
}

/// A record from its file.
pub fn from_json<R: Record>(text: &str) -> Result<R, FileError> {
    let value: serde_json::Value =
        serde_json::from_str(text).map_err(|e| FileError::NotJson(e.to_string()))?;
    if value.get("format").and_then(serde_json::Value::as_str) != Some(R::FORMAT) {
        return Err(FileError::WrongFormat);
    }
    let version = value
        .get("version")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    if version > VERSION {
        return Err(FileError::Newer(version));
    }
    serde_json::from_value::<R>(value).map_err(|e| FileError::Invalid(e.to_string()))
}

/// A day as the files write it: `2026-01-15`.
#[must_use]
pub fn format_date(date: NaiveDate) -> String {
    date.format(DATE_FORMAT).to_string()
}

/// A `YYYY-MM-DD` day (also `DD.MM.YYYY` and `MM/DD/YYYY`, as CSV files
/// from spreadsheets hold them).
#[must_use]
pub fn parse_date(text: &str) -> Option<NaiveDate> {
    let t = text.trim();
    if t.is_empty() {
        return None;
    }
    [DATE_FORMAT, "%d.%m.%Y", "%m/%d/%Y"]
        .into_iter()
        .find_map(|format| NaiveDate::parse_from_str(t, format).ok())
}

#[allow(clippy::trivially_copy_pass_by_ref)] // serde's skip_serializing_if hands a reference
fn is_zero(n: &u32) -> bool {
    *n == 0
}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_zero_amount(n: &i64) -> bool {
    *n == 0
}

/// Money fields as decimal strings (`"1596.64"`).
mod amount_text {
    use serde::{Deserialize, Deserializer, Serializer};

    #[allow(clippy::trivially_copy_pass_by_ref)]
    pub fn serialize<S: Serializer>(cents: &i64, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&crate::money::file_amount(*cents))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<i64, D::Error> {
        let text = String::deserialize(d)?;
        crate::money::parse_file_amount(&text).map_err(serde::de::Error::custom)
    }
}

/// Days as `YYYY-MM-DD`.
mod date_text {
    use chrono::NaiveDate;
    use serde::{Deserialize, Deserializer, Serializer};

    #[allow(clippy::trivially_copy_pass_by_ref)]
    pub fn serialize<S: Serializer>(date: &NaiveDate, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&super::format_date(*date))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<NaiveDate, D::Error> {
        let text = String::deserialize(d)?;
        day(&text).map_err(serde::de::Error::custom)
    }

    /// A file's `YYYY-MM-DD` day (strict: the files are written by AzERP).
    pub fn day(text: &str) -> Result<NaiveDate, String> {
        NaiveDate::parse_from_str(text.trim(), super::DATE_FORMAT)
            .map_err(|_| format!("\"{text}\" is not a day (YYYY-MM-DD)"))
    }
}

/// Optional days as `YYYY-MM-DD` (left out when `None`).
mod opt_date_text {
    use chrono::NaiveDate;
    use serde::{Deserialize, Deserializer, Serializer};

    #[allow(clippy::ref_option)] // serde's `with` hands a reference
    pub fn serialize<S: Serializer>(date: &Option<NaiveDate>, s: S) -> Result<S::Ok, S::Error> {
        match date {
            Some(d) => s.serialize_str(&super::format_date(*d)),
            None => s.serialize_none(),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<NaiveDate>, D::Error> {
        match Option::<String>::deserialize(d)? {
            Some(text) if !text.trim().is_empty() => super::date_text::day(&text)
                .map(Some)
                .map_err(serde::de::Error::custom),
            _ => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    /// An asset with every field set.
    fn full_asset() -> Asset {
        let mut a = Asset::new(
            "6f1c2a4e-0d1b-4c8e-9a51-2b7f3c9d0e11",
            "A-0042",
            "ThinkPad X1 \"Carbon\"",
            day(2026, 1, 15),
            159_664,
            3,
        );
        a.category = "c0000000-0000-4000-8000-000000000001".into();
        a.location = "l0000000-0000-4000-8000-000000000001".into();
        a.serial = "PF-2XK91".into();
        a.residual = 10_000;
        a.method = Method::DecliningBalance;
        a.declining_rate_bp = 2500;
        a.status = Status::CheckedOut;
        a.custodian = "Ada Lovelace".into();
        a.maintenance_months = 12;
        a.disposed = Some(day(2028, 6, 30));
        a.disposal_amount = 25_050;
        a.notes = "Line one\nline two".into();
        a
    }

    #[test]
    fn an_asset_file_round_trips_every_field_exactly() {
        let a = full_asset();
        let text = to_json(&a);
        assert_eq!(from_json::<Asset>(&text), Ok(a));
    }

    #[test]
    fn the_asset_file_uses_the_erp_field_names_and_exact_decimal_strings() {
        let text = to_json(&full_asset());
        let v: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(v["format"], "azerp.asset");
        assert_eq!(v["version"], 1);
        assert_eq!(v["asset_number"], "A-0042");
        assert_eq!(v["acquisition_date"], "2026-01-15");
        assert_eq!(
            v["acquisition_cost"], "1596.64",
            "money is a string, never a float"
        );
        assert_eq!(v["residual_value"], "100.00");
        assert_eq!(v["useful_life_years"], 3);
        assert_eq!(v["depreciation_method"], "DECLINING_BALANCE");
        assert_eq!(v["status"], "CHECKED_OUT");
        assert_eq!(v["disposal_date"], "2028-06-30");
        assert_eq!(v["disposal_amount"], "250.50");
        assert!(text.ends_with('\n'));
    }

    #[test]
    fn empty_optional_fields_are_left_out_and_read_back_as_defaults() {
        let a = Asset::new(
            "6f1c2a4e-0d1b-4c8e-9a51-2b7f3c9d0e11",
            "A-1",
            "Desk",
            day(2025, 3, 1),
            45_000,
            10,
        );
        let text = to_json(&a);
        for absent in [
            "category_id",
            "serial_number",
            "custodian",
            "disposal_date",
            "disposal_amount",
            "notes",
        ] {
            assert!(!text.contains(absent), "{absent} in {text}");
        }
        let minimal = r#"{"format":"azerp.asset","version":1,"id":"x","asset_number":"A-1","name":"Desk",
            "acquisition_date":"2025-03-01","acquisition_cost":"450","useful_life_years":10,"colour":"red"}"#;
        let read = from_json::<Asset>(minimal).unwrap();
        assert_eq!(read.cost, 45_000);
        assert_eq!(read.residual, 0);
        assert_eq!(read.method, Method::StraightLine);
        assert_eq!(read.status, Status::InUse);
        assert_eq!(read.disposed, None);
    }

    #[test]
    fn a_file_of_another_kind_or_a_newer_version_is_refused_not_guessed_at() {
        let text = to_json(&full_asset());
        assert_eq!(from_json::<Category>(&text), Err(FileError::WrongFormat));
        let newer = text.replace("\"version\": 1", "\"version\": 7");
        assert_eq!(from_json::<Asset>(&newer), Err(FileError::Newer(7)));
        assert!(matches!(
            from_json::<Asset>("not json"),
            Err(FileError::NotJson(_))
        ));
        let float = text.replace("\"1596.64\"", "1596.64");
        assert!(
            matches!(from_json::<Asset>(&float), Err(FileError::Invalid(_))),
            "a float is not money"
        );
        let bad_day = text.replace("2026-01-15", "2026-02-30");
        assert!(matches!(
            from_json::<Asset>(&bad_day),
            Err(FileError::Invalid(_))
        ));
    }

    #[test]
    fn categories_locations_maintenance_and_checkouts_round_trip() {
        let c = Category {
            id: "c1".into(),
            name: "IT equipment".into(),
            life_years: 3,
            method: Method::StraightLine,
            notes: String::new(),
        };
        assert_eq!(from_json::<Category>(&to_json(&c)), Ok(c));
        let l = Location {
            id: "l1".into(),
            name: "Head office, room 2.14".into(),
            address: "Musterstr. 1, 80331 Munich".into(),
            notes: "Badge needed".into(),
        };
        assert_eq!(from_json::<Location>(&to_json(&l)), Ok(l));
        let m = MaintenanceEntry {
            id: "m1".into(),
            asset: "a1".into(),
            date: day(2026, 9, 1),
            kind: MaintenanceKind::Inspection,
            description: "DGUV V3 electrical test".into(),
            cost: 4_900,
            by: "Elektro Huber".into(),
        };
        assert_eq!(from_json::<MaintenanceEntry>(&to_json(&m)), Ok(m));
        let open = Checkout {
            id: "k1".into(),
            asset: "a1".into(),
            custodian: "Grace Hopper".into(),
            out: day(2026, 9, 1),
            due: Some(day(2026, 9, 30)),
            returned: None,
            note: String::new(),
        };
        assert!(open.is_open());
        assert_eq!(from_json::<Checkout>(&to_json(&open)), Ok(open.clone()));
        let closed = Checkout {
            returned: Some(day(2026, 9, 12)),
            ..open
        };
        assert!(!closed.is_open());
        assert_eq!(from_json::<Checkout>(&to_json(&closed)), Ok(closed));
    }

    #[test]
    fn methods_and_statuses_read_their_codes_labels_and_common_names() {
        assert_eq!(Method::parse("STRAIGHT_LINE"), Some(Method::StraightLine));
        assert_eq!(Method::parse("straight-line"), Some(Method::StraightLine));
        assert_eq!(Method::parse("Linear"), Some(Method::StraightLine));
        assert_eq!(
            Method::parse("declining balance"),
            Some(Method::DecliningBalance)
        );
        assert_eq!(Method::parse("degressiv"), Some(Method::DecliningBalance));
        assert_eq!(Method::parse("sum of years"), None);
        for s in Status::ALL {
            assert_eq!(Status::parse(s.code()), Some(s));
            assert_eq!(Status::parse(&s.label().to_uppercase()), Some(s));
        }
        assert_eq!(Status::parse("lost"), None);
    }

    #[test]
    fn days_read_in_iso_german_and_us_notation() {
        assert_eq!(parse_date("2026-01-15"), Some(day(2026, 1, 15)));
        assert_eq!(parse_date(" 15.01.2026 "), Some(day(2026, 1, 15)));
        assert_eq!(parse_date("01/15/2026"), Some(day(2026, 1, 15)));
        assert_eq!(parse_date("2026-02-30"), None);
        assert_eq!(parse_date(""), None);
        assert_eq!(format_date(day(2026, 1, 5)), "2026-01-05");
    }

    #[test]
    fn the_declining_rate_defaults_to_twice_the_straight_line_rate() {
        let mut a = Asset::new("a", "A-1", "Press", day(2026, 1, 1), 1_000_000, 5);
        assert_eq!(a.rate_bp(), 4000, "2 x 20 %");
        a.life_years = 1;
        assert_eq!(a.rate_bp(), 10_000, "never more than 100 %");
        a.life_years = 0;
        assert_eq!(a.rate_bp(), 10_000);
        a.declining_rate_bp = 2500;
        assert_eq!(a.rate_bp(), 2500, "the asset's own rate wins");
    }

    #[test]
    fn an_asset_says_what_keeps_it_from_being_saved() {
        let ok = Asset::new("a", "A-1", "Press", day(2026, 1, 1), 1_000_000, 5);
        assert!(ok.problems().is_empty(), "{:?}", ok.problems());
        let mut a = ok.clone();
        a.name = "  ".into();
        a.number.clear();
        a.cost = -1;
        let p = a.problems();
        assert_eq!(p.len(), 3, "{p:?}");
        let mut b = ok.clone();
        b.residual = 2_000_000;
        assert_eq!(b.problems().len(), 1, "a residual value above the cost");
        let mut c = ok.clone();
        c.disposed = Some(day(2025, 12, 31));
        assert_eq!(c.problems().len(), 1, "disposed before it was acquired");
        let mut d = ok;
        d.life_years = 101;
        d.declining_rate_bp = 10_001;
        assert_eq!(d.problems().len(), 2);
    }
}
