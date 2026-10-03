//! The view-JSON interpreter, first slice (ERP README s3 "Where the
//! interpreter lives": serde models of `ui.*.json` / `menu.json`, view ->
//! azul `Dom`, a condition evaluator, column formatters).
//!
//! The ERP describes every screen as JSON and its web frontend interprets
//! it at run time; this does the same natively. The dialect is the ERP's
//! own (`erp.ui.assets.json` here is the ERP repo's file, unchanged, and
//! parses); `ui.assets.json` is the asset section AzERP runs - the ERP's
//! four views corrected (the required columns its form left out, both
//! depreciation methods, location and serial number) plus the views no ERP
//! JSON defines yet (categories, locations, maintenance, check-outs,
//! disposal, reports, import), and a `menu` of its sections.
//!
//! - this module: the serde models ([`ViewFile`], [`View`], ...), routing a
//!   path to its view ([`ViewFile::route`], `:id` parameters), the labels
//!   ([`Labels`]: `en.json`, else the key made readable - the ERP's own
//!   translations are empty), and which record kind an `api` path reads
//!   ([`api_kind`]).
//! - [`spec`]: a view as the screen needs it - table columns, typed form
//!   fields, actions, detail tabs, wizard steps - plus the condition
//!   evaluator and the form validation.
//! - [`rows`]: the records as the views read them: a field by its ERP name
//!   ([`rows::ViewRecord`]), a table's cells, a form's texts and back.
//!
//! The `Dom` half is in `crate::ui`: a `table` view becomes azul's
//! DataTable, a `form` / `form_modal` view a form, a `detail` view a header
//! with tabs whose named panels (`DepreciationSchedulePanel`, ...) are built
//! by hand, as the README says.

pub mod rows;
pub mod spec;

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::store::Kind;

/// The asset section's views (the corrected ERP views and the new ones).
pub const ASSET_VIEWS: &str = include_str!("ui.assets.json");

/// The ERP repo's own `json/ui/ui.assets.json`, unchanged: the dialect test.
pub const ERP_ASSET_VIEWS: &str = include_str!("erp.ui.assets.json");

/// The English labels of the keys the views use.
pub const LABELS_EN: &str = include_str!("en.json");

/// A path's `:name` parameters.
pub type Params = BTreeMap<String, String>;

/// A view file: the views by id, and the menu of the section.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct ViewFile {
    #[serde(default)]
    pub menu: Vec<MenuEntry>,
    #[serde(default)]
    pub views: BTreeMap<String, View>,
}

/// One entry of the section's menu.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct MenuEntry {
    pub key: String,
    pub path: String,
}

/// A view's path, or its paths (a form for "new" and "edit").
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum Paths {
    One(String),
    Many(Vec<String>),
}

impl Paths {
    /// Every path.
    #[must_use]
    pub fn all(&self) -> Vec<&str> {
        match self {
            Paths::One(p) => vec![p.as_str()],
            Paths::Many(ps) => ps.iter().map(String::as_str).collect(),
        }
    }
}

/// A view's title key, or one per mode of a form.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum TitleKey {
    Plain(String),
    Modes { new: String, edit: String },
}

/// The view types of the ERP's JSON.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ViewKind {
    Table,
    TableEmbedded,
    Form,
    FormModal,
    Detail,
    Wizard,
    Report,
    Dashboard,
    Kanban,
    Calendar,
    Inbox,
    SearchResults,
    Custom,
    /// A type this interpreter does not know yet.
    #[serde(other)]
    Unknown,
}

/// The REST paths a view reads and writes (here: which records).
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
pub struct Api {
    pub get: Option<String>,
    pub post: Option<String>,
    pub put: Option<String>,
    pub delete: Option<String>,
}

/// A button: a link to another view's path, a named action, submit, cancel.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Action {
    #[serde(rename = "type")]
    pub kind: String,
    pub label_key: Option<String>,
    pub path: Option<String>,
    pub variant: Option<String>,
    pub icon: Option<String>,
    /// A named action's name (`export_csv`).
    pub id: Option<String>,
    /// When it shows (`status == 'CHECKED_OUT'`).
    pub condition: Option<String>,
}

/// A table column.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Column {
    pub data_field: String,
    pub header_key: Option<String>,
    /// `date`, `currency`, `status_pill`, `integer` (text without).
    #[serde(rename = "type")]
    pub kind: Option<String>,
    pub sortable: Option<bool>,
    pub width: Option<f32>,
}

/// A form field.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct FieldDef {
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub label_key: Option<String>,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub options: Vec<OptionDef>,
    /// Where a `select_async` field's choices come from (an `api` path).
    pub options_source: Option<String>,
    pub min: Option<i64>,
    pub max: Option<i64>,
    /// `today`, `this_year` or a value.
    pub default: Option<String>,
    /// When it shows.
    pub condition: Option<String>,
}

/// A choice of a `select` field.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct OptionDef {
    pub value: String,
    pub label_key: Option<String>,
    pub label: Option<String>,
}

/// A detail view's header.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Header {
    pub title_field: String,
    pub subtitle_field: Option<String>,
    pub status_field: Option<String>,
    #[serde(default)]
    pub actions: Vec<Action>,
}

/// A detail view's tab: a named panel, or an embedded table view.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Tab {
    pub key: String,
    pub component: String,
    /// `EmbeddedTable`: the `table_embedded` view it shows.
    pub view: Option<String>,
    pub api: Option<Api>,
}

/// A wizard's step: fields, or a named component.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Step {
    pub key: String,
    pub title_key: Option<String>,
    #[serde(default)]
    pub fields: Vec<FieldDef>,
    pub component: Option<String>,
}

/// One view.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct View {
    pub id: String,
    pub path: Paths,
    #[serde(rename = "type")]
    pub kind: ViewKind,
    pub title_key: Option<TitleKey>,
    #[serde(default)]
    pub api: Api,
    #[serde(default)]
    pub actions: Vec<Action>,
    #[serde(default)]
    pub columns: Vec<Column>,
    #[serde(default)]
    pub row_actions: Vec<Action>,
    #[serde(default)]
    pub fields: Vec<FieldDef>,
    pub header: Option<Header>,
    #[serde(default)]
    pub tabs: Vec<Tab>,
    #[serde(default)]
    pub steps: Vec<Step>,
    /// A report's parameters.
    #[serde(default)]
    pub parameters: Vec<FieldDef>,
    /// A `report` / `custom` view's named component.
    pub component: Option<String>,
}

impl View {
    /// The title: the plain key, or the "new" / "edit" key of a form.
    #[must_use]
    pub fn title(&self, labels: &Labels, editing: bool) -> String {
        match &self.title_key {
            Some(TitleKey::Plain(key)) => labels.get(key),
            Some(TitleKey::Modes { new, edit }) => labels.get(if editing { edit } else { new }),
            None => labels.get(&self.id),
        }
    }

    /// The record kind the view reads (its `get`, else `post` / `put`).
    #[must_use]
    pub fn kind_of_records(&self) -> Option<Kind> {
        let _ = self;
        todo!("GREEN")
    }
}

/// A path routed to its view.
#[derive(Clone, Debug)]
pub struct Route<'a> {
    pub view: &'a View,
    /// The view's path pattern that matched (`/accounting/assets/:id/edit`).
    pub pattern: &'a str,
    pub params: Params,
}

impl Route<'_> {
    /// Whether the route edits a record (its pattern names one).
    #[must_use]
    pub fn editing(&self) -> bool {
        self.pattern.contains(':')
    }
}

impl ViewFile {
    /// Reads a view file.
    pub fn parse(json: &str) -> Result<ViewFile, String> {
        let _ = json;
        todo!("GREEN")
    }

    /// The asset section ([`ASSET_VIEWS`]).
    #[must_use]
    pub fn assets() -> ViewFile {
        ViewFile::parse(ASSET_VIEWS).unwrap_or_else(|e| {
            eprintln!("[AzERP] the asset views do not parse: {e}");
            ViewFile::default()
        })
    }

    /// The view with id `id`.
    #[must_use]
    pub fn view(&self, id: &str) -> Option<&View> {
        self.views.get(id)
    }

    /// The view `path` shows and its parameters. A literal segment beats a
    /// parameter: `/accounting/assets/new` is the form, not the detail of an
    /// asset called "new".
    #[must_use]
    pub fn route(&self, path: &str) -> Option<Route<'_>> {
        let _ = path;
        todo!("GREEN")
    }
}

/// `path` against `pattern` (`/accounting/assets/:id`): the parameters, or
/// `None` when it does not match.
#[must_use]
pub fn match_path(pattern: &str, path: &str) -> Option<Params> {
    let _ = (pattern, path);
    todo!("GREEN")
}

/// `pattern` with its parameters filled in (`:id` -> `params["id"]`).
#[must_use]
pub fn fill_path(pattern: &str, params: &Params) -> String {
    let _ = (pattern, params);
    todo!("GREEN")
}

/// The record kind an `api` path reads (`/api/assets/fixed-assets/:id` ->
/// assets).
#[must_use]
pub fn api_kind(api: &str) -> Option<Kind> {
    let _ = api;
    todo!("GREEN")
}

/// The parent filter of an `api` path's query (`?asset_id=:id` with
/// `id = x` -> `("asset_id", "x")`).
#[must_use]
pub fn api_filter(api: &str, params: &Params) -> Option<(String, String)> {
    let _ = (api, params);
    todo!("GREEN")
}

/// The labels of the keys.
#[derive(Clone, Debug, Default)]
pub struct Labels {
    map: BTreeMap<String, String>,
}

impl Labels {
    /// Labels from a `{ "key": "label" }` file.
    pub fn parse(json: &str) -> Result<Labels, String> {
        serde_json::from_str(json)
            .map(|map| Labels { map })
            .map_err(|e| e.to_string())
    }

    /// The English labels ([`LABELS_EN`]).
    #[must_use]
    pub fn en() -> Labels {
        Labels::parse(LABELS_EN).unwrap_or_default()
    }

    /// The label of `key`, else the key made readable ([`humanize`]).
    #[must_use]
    pub fn get(&self, key: &str) -> String {
        self.map.get(key).cloned().unwrap_or_else(|| humanize(key))
    }
}

/// A key made readable: `fields.purchase_order_no.label` -> `Purchase
/// order no` (its kind prefix and `label` / `title` suffix dropped).
#[must_use]
pub fn humanize(key: &str) -> String {
    let _ = key;
    todo!("GREEN")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_asset_views_parse_with_a_menu_of_six_sections_that_all_route() {
        let file = ViewFile::assets();
        assert_eq!(file.views.len(), 17);
        assert_eq!(file.menu.len(), 6);
        for entry in &file.menu {
            let route = file
                .route(&entry.path)
                .unwrap_or_else(|| panic!("{} routes nowhere", entry.path));
            assert!(
                matches!(route.view.kind, ViewKind::Table | ViewKind::Report),
                "{} is a {:?}",
                entry.path,
                route.view.kind
            );
        }
        for (id, view) in &file.views {
            assert_eq!(&view.id, id);
            assert_ne!(view.kind, ViewKind::Unknown, "{id}");
        }
    }

    #[test]
    fn the_erps_own_asset_views_parse_as_they_are() {
        let file = ViewFile::parse(ERP_ASSET_VIEWS).unwrap();
        assert_eq!(file.views.len(), 4);
        let kinds: Vec<ViewKind> = file.views.values().map(|v| v.kind).collect();
        for kind in [
            ViewKind::Table,
            ViewKind::Form,
            ViewKind::Detail,
            ViewKind::Wizard,
        ] {
            assert!(kinds.contains(&kind), "{kind:?}");
        }
        assert_eq!(
            file.route("/accounting/assets/new")
                .map(|r| r.view.id.as_str()),
            Some("assets_fixed_asset_form")
        );
        let form = file.view("assets_fixed_asset_form").unwrap();
        assert!(
            !form.fields.iter().any(|f| f.name == "residual_value"),
            "the ERP's form leaves the residual value out (fixed in ui.assets.json)"
        );
    }

    #[test]
    fn a_literal_path_wins_over_a_parameter_and_parameters_are_filled() {
        let file = ViewFile::assets();
        let new = file.route("/accounting/assets/new").unwrap();
        assert_eq!(new.view.id, "assets_fixed_asset_form");
        assert!(!new.editing());
        let detail = file.route("/accounting/assets/6f1c").unwrap();
        assert_eq!(detail.view.id, "assets_fixed_asset_detail");
        assert_eq!(detail.params.get("id").map(String::as_str), Some("6f1c"));
        let edit = file.route("/accounting/assets/6f1c/edit").unwrap();
        assert_eq!(edit.view.id, "assets_fixed_asset_form");
        assert!(edit.editing());
        assert_eq!(edit.view.title(&Labels::en(), edit.editing()), "Edit asset");
        assert_eq!(new.view.title(&Labels::en(), new.editing()), "New asset");
        assert!(file.route("/accounting/nothing").is_none());
        assert!(file.route("/accounting/assets/a/b/c/d").is_none());
        assert_eq!(
            match_path("/a/:id/b", "/a/x/b")
                .unwrap()
                .get("id")
                .map(String::as_str),
            Some("x")
        );
        assert!(
            match_path("/a/:id", "/a/").is_none(),
            "an empty parameter does not match"
        );
        assert!(
            match_path("/a/:id", "/a/x/").is_some(),
            "a trailing slash is the same path"
        );
        let mut params = Params::new();
        params.insert("id".into(), "6f1c".into());
        assert_eq!(
            fill_path("/accounting/assets/:id/edit", &params),
            "/accounting/assets/6f1c/edit"
        );
        assert_eq!(
            fill_path("/accounting/assets/:missing", &params),
            "/accounting/assets/"
        );
    }

    #[test]
    fn labels_come_from_the_table_else_from_the_key() {
        let labels = Labels::en();
        assert_eq!(labels.get("fields.book_value.label"), "Book value");
        assert_eq!(labels.get("menu.fixed_assets"), "Register");
        assert_eq!(
            labels.get("fields.purchase_order_no.label"),
            "Purchase order no"
        );
        assert_eq!(labels.get("view.asset_transfer.title"), "Asset transfer");
        assert_eq!(labels.get("tab.something_new"), "Something new");
        assert_eq!(humanize("plain"), "Plain");
        assert_eq!(humanize(""), "");
    }

    #[test]
    fn api_paths_name_the_record_kind_and_the_parent_filter() {
        assert_eq!(api_kind("/api/assets/fixed-assets"), Some(Kind::Asset));
        assert_eq!(api_kind("/api/assets/fixed-assets/:id"), Some(Kind::Asset));
        assert_eq!(api_kind("/api/assets/categories"), Some(Kind::Category));
        assert_eq!(api_kind("/api/assets/locations/:id"), Some(Kind::Location));
        assert_eq!(
            api_kind("/api/assets/maintenance?asset_id=:id"),
            Some(Kind::Maintenance)
        );
        assert_eq!(api_kind("/api/assets/checkouts"), Some(Kind::Checkout));
        assert_eq!(api_kind("/api/sales/invoices"), None);
        let mut params = Params::new();
        params.insert("id".into(), "a1".into());
        assert_eq!(
            api_filter("/api/assets/maintenance?asset_id=:id", &params),
            Some(("asset_id".to_string(), "a1".to_string()))
        );
        assert_eq!(api_filter("/api/assets/maintenance", &params), None);
        let file = ViewFile::assets();
        let kinds: Vec<Option<Kind>> = [
            "assets_fixed_asset_list",
            "assets_fixed_asset_form",
            "assets_category_list",
            "assets_maintenance_embedded",
            "assets_checkout_form",
            "assets_import",
        ]
        .iter()
        .map(|id| file.view(id).unwrap().kind_of_records())
        .collect();
        assert_eq!(
            kinds,
            [
                Some(Kind::Asset),
                Some(Kind::Asset),
                Some(Kind::Category),
                Some(Kind::Maintenance),
                Some(Kind::Checkout),
                None
            ]
        );
    }
}
