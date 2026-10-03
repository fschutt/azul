//! The app's state, without a window: where the user is (a path the views
//! route), the open form, the records, and what changing them writes.
//!
//! Every change goes into the [`Book`] at once and queues the files it
//! changed in azul-pim's write-behind [`WriteQueue`] (one write per key, one
//! batch in flight); the window drains the queue as azul-appkit file jobs on
//! a Thread (`crate::ui`). Nothing here blocks or touches a disk, so the
//! flows are tested as plain Rust.
//!
//! Navigation is the views' own: [`State::open`] routes a path; a `form` or
//! `form_modal` view opens a [`FormDraft`] OVER the page (the RecordsShell's
//! form pane, or a modal), every other view becomes the page.

use std::collections::BTreeMap;

use azul_pim::write_queue::WriteQueue;
use chrono::NaiveDate;

use crate::{
    csv_io, depreciation,
    model::{
        self, Asset, Category, Checkout, Location, MaintenanceEntry, MaintenanceKind, Record,
        Status,
    },
    money, sample,
    store::{self, Book, Kind, Skipped, Stored},
    views::{
        rows::{self, Ctx, ViewRecord},
        spec::{self, FieldSpec},
        Labels, Params, Route, View, ViewFile, ViewKind,
    },
};

/// The register: the start page.
pub const HOME: &str = "/accounting/assets";

/// An open form: its view, the path's parameters, the texts typed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FormDraft {
    /// The view's id.
    pub view: String,
    pub params: Params,
    /// It edits a record (`params["id"]`); else it creates one (whose
    /// parent, for a check-out or a maintenance entry, is `params["id"]`).
    pub editing: bool,
    /// The texts, by field name.
    pub values: BTreeMap<String, String>,
    /// What keeps it from being saved, by field name.
    pub problems: Vec<(String, String)>,
    /// Something was typed since it opened.
    pub dirty: bool,
}

/// A CSV file being imported: its rows, the mapping, what it would do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportDraft {
    /// The file's name.
    pub name: String,
    pub table: csv_io::Table,
    pub mapping: Vec<csv_io::Field>,
    /// What the import would do with the mapping (ids not minted yet).
    pub preview: csv_io::Import,
}

/// The app's state.
pub struct State {
    pub views: ViewFile,
    pub labels: Labels,
    pub book: Book,
    /// The records were read (until then the page says so).
    pub loaded: bool,
    /// The day book values are for, new records start on, exports are named by.
    pub today: NaiveDate,
    /// The path the main pane shows.
    pub page: String,
    /// The pages before it (Back).
    pub back: Vec<String>,
    /// The detail page's tab.
    pub tab: usize,
    pub form: Option<FormDraft>,
    pub import: Option<ImportDraft>,
    /// The depreciation run's year and step (0 parameters, 1 preview).
    pub run_year: i32,
    pub run_step: usize,
    /// The last thing that happened, for the status bar ("" = nothing).
    pub notice: String,
    /// The files the load could not read.
    pub skipped: Vec<Skipped>,
    /// The files to write.
    pub queue: WriteQueue,
}

impl State {
    /// A state on the register, nothing loaded yet.
    #[must_use]
    pub fn new(today: NaiveDate) -> State {
        let _ = today;
        todo!("GREEN")
    }

    /// The page's route.
    #[must_use]
    pub fn route(&self) -> Option<Route<'_>> {
        self.views.route(&self.page)
    }

    /// What the computed fields are computed against.
    #[must_use]
    pub fn ctx(&self) -> Ctx<'_> {
        Ctx {
            book: &self.book,
            today: self.today,
        }
    }

    /// The form fields of a view.
    #[must_use]
    pub fn fields_of(&self, view: &View) -> Vec<FieldSpec> {
        spec::fields(&view.fields, &self.labels)
    }

    /// Goes to `path`: a form opens over the page, anything else is the page.
    pub fn open(&mut self, path: &str) {
        let _ = path;
        todo!("GREEN")
    }

    /// Back to the page before.
    pub fn go_back(&mut self) {
        todo!("GREEN")
    }

    /// The open form's field `name` is now `text`.
    pub fn set_value(&mut self, name: &str, text: &str) {
        if let Some(draft) = self.form.as_mut() {
            draft.values.insert(name.to_string(), text.to_string());
            draft.dirty = true;
        }
    }

    /// Closes the open form without saving.
    pub fn cancel_form(&mut self) {
        self.form = None;
    }

    /// Saves the open form: the record changes, its file (and an asset's,
    /// for a check-out) is queued, the form closes; a new asset opens. On a
    /// problem the form stays open and names it. `true` = saved.
    pub fn save_form(&mut self, new_id: &mut dyn FnMut() -> String) -> bool {
        let _ = new_id;
        todo!("GREEN")
    }

    /// Checks the asset in: its open check-out ends today.
    pub fn check_in(&mut self, asset: &str) {
        let _ = asset;
        todo!("GREEN")
    }

    /// Deletes the asset with its logs (their files too), back to the register.
    pub fn delete_asset(&mut self, asset: &str) {
        let _ = asset;
        todo!("GREEN")
    }

    /// The register as CSV into `erp/exports/assets-<today>.csv`; its key.
    pub fn export_register(&mut self) -> String {
        todo!("GREEN")
    }

    /// The asset's schedule as CSV into `erp/exports/schedule-<number>-<today>.csv`.
    pub fn export_schedule(&mut self, asset: &str) -> Option<String> {
        let _ = asset;
        todo!("GREEN")
    }

    /// The depreciation run of `year`: every asset's depreciation that year
    /// (`(asset id, number, name, amount)`, the assets with any).
    #[must_use]
    pub fn run_preview(&self, year: i32) -> Vec<(String, String, String, i64)> {
        let _ = (year, depreciation::depreciation_in_year);
        todo!("GREEN")
    }

    /// Runs and "posts" the run of `year`: its journal (one line per asset
    /// and the total) as CSV into `erp/exports/depreciation-run-<year>.csv`
    /// (there is no ledger yet); its key.
    pub fn post_run(&mut self, year: i32) -> String {
        let _ = (year, money::file_amount);
        todo!("GREEN")
    }

    /// The records the load read (`(key, bytes)` of every file under `erp/`).
    pub fn load(&mut self, files: &[(String, Vec<u8>)]) {
        let _ = files;
        todo!("GREEN")
    }

    /// Writes the sample register into an EMPTY register; how many records.
    pub fn seed_sample(&mut self, new_id: &mut dyn FnMut() -> String) -> usize {
        let _ = (new_id, sample::book);
        todo!("GREEN")
    }

    /// Reads a CSV file to import: the import page shows its preview.
    pub fn start_import(&mut self, name: &str, text: &str) {
        let _ = (name, text);
        todo!("GREEN")
    }

    /// The import's column `column` is now `field`: the preview follows.
    pub fn set_mapping(&mut self, column: usize, field: csv_io::Field) {
        let _ = (column, field);
        todo!("GREEN")
    }

    /// Imports the rows the preview shows; how many assets were written.
    pub fn commit_import(&mut self, new_id: &mut dyn FnMut() -> String) -> usize {
        let _ = new_id;
        todo!("GREEN")
    }

    /// Queues the record's file.
    pub fn put_write<R: Record>(&mut self, record: &R) {
        let (key, bytes) = store::write_of(record);
        self.queue.put(key, bytes);
    }
}

#[cfg(test)]
mod tests {
    use azul_pim::write_queue::Write;

    use super::*;

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, 3).unwrap()
    }

    fn ids() -> impl FnMut() -> String {
        let mut n = 0;
        move || {
            n += 1;
            format!("00000000-0000-4000-8000-{n:012}")
        }
    }

    /// The keys of the writes waiting (the batch is taken and finished).
    fn written(s: &mut State) -> Vec<String> {
        let batch = s.queue.take().unwrap_or_default();
        s.queue.finish(Vec::new());
        batch
            .iter()
            .map(|w| match w {
                Write::Put { key, .. } => format!("put {key}"),
                Write::Delete { key } => format!("delete {key}"),
            })
            .collect()
    }

    /// A loaded state with the sample register, its writes taken.
    fn sampled() -> State {
        let mut s = State::new(today());
        s.load(&[]);
        let mut mint = ids();
        s.seed_sample(&mut mint);
        let _ = written(&mut s);
        s
    }

    fn fill(s: &mut State, pairs: &[(&str, &str)]) {
        for (k, v) in pairs {
            s.set_value(k, v);
        }
    }

    #[test]
    fn the_new_asset_path_opens_a_form_over_the_register_with_its_defaults() {
        let mut s = State::new(today());
        s.load(&[]);
        assert_eq!(s.page, HOME);
        s.open("/accounting/assets/new");
        assert_eq!(s.page, HOME, "the form opens over the page");
        let draft = s.form.as_ref().unwrap();
        assert_eq!(draft.view, "assets_fixed_asset_form");
        assert!(!draft.editing);
        assert_eq!(draft.values["asset_number"], "A-0001");
        assert_eq!(draft.values["acquisition_date"], "2026-10-03");
        assert_eq!(draft.values["depreciation_method"], "STRAIGHT_LINE");
        assert_eq!(draft.values["residual_value"], "0.00");
        assert_eq!(draft.values["name"], "");
    }

    #[test]
    fn saving_a_new_asset_queues_its_file_and_opens_it() {
        let mut s = State::new(today());
        s.load(&[]);
        s.open("/accounting/assets/new");
        fill(
            &mut s,
            &[
                ("name", "ThinkPad"),
                ("acquisition_cost", "1.596,64"),
                ("useful_life_years", "3"),
            ],
        );
        let mut mint = ids();
        assert!(s.save_form(&mut mint));
        assert!(s.form.is_none());
        let a = &s.book.assets[0];
        assert_eq!(
            (a.number.as_str(), a.cost, a.life_years),
            ("A-0001", 159_664, 3)
        );
        assert_eq!(s.page, format!("/accounting/assets/{}", a.id));
        assert_eq!(s.back, [HOME]);
        let id = a.id.clone();
        assert_eq!(written(&mut s), [format!("put erp/assets/{id}.json")]);
    }

    #[test]
    fn a_wrong_form_stays_open_with_its_problems_and_writes_nothing() {
        let mut s = sampled();
        s.open("/accounting/assets/new");
        fill(
            &mut s,
            &[("acquisition_cost", "lots"), ("useful_life_years", "3")],
        );
        let mut mint = ids();
        assert!(!s.save_form(&mut mint));
        let problems: Vec<&str> = s
            .form
            .as_ref()
            .unwrap()
            .problems
            .iter()
            .map(|(f, _)| f.as_str())
            .collect();
        assert_eq!(problems, ["name", "acquisition_cost"]);
        s.open("/accounting/assets/new");
        fill(
            &mut s,
            &[
                ("name", "Second laptop"),
                ("asset_number", "a-0001"),
                ("acquisition_cost", "10"),
                ("useful_life_years", "3"),
            ],
        );
        assert!(!s.save_form(&mut mint));
        let problems = &s.form.as_ref().unwrap().problems;
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert_eq!(problems[0].0, "asset_number", "the number is taken");
        assert!(written(&mut s).is_empty());
    }

    #[test]
    fn editing_an_asset_starts_from_its_values_and_rewrites_its_file() {
        let mut s = sampled();
        let id = s.book.asset_by_number("A-0005").unwrap().id.clone();
        s.open(&format!("/accounting/assets/{id}"));
        s.open(&format!("/accounting/assets/{id}/edit"));
        let draft = s.form.as_ref().unwrap();
        assert!(draft.editing);
        assert_eq!(draft.values["name"], "Standing desk, oak");
        assert_eq!(draft.values["acquisition_cost"], "1290.00");
        fill(&mut s, &[("name", "Standing desk, walnut")]);
        let mut mint = ids();
        assert!(s.save_form(&mut mint));
        assert_eq!(
            s.book.get::<Asset>(&id).unwrap().name,
            "Standing desk, walnut"
        );
        assert_eq!(
            s.page,
            format!("/accounting/assets/{id}"),
            "the detail stays"
        );
        assert_eq!(written(&mut s), [format!("put erp/assets/{id}.json")]);
    }

    #[test]
    fn checking_an_asset_out_and_in_writes_the_checkout_and_the_asset() {
        let mut s = sampled();
        let id = s.book.asset_by_number("A-0004").unwrap().id.clone();
        s.open(&format!("/accounting/assets/{id}/checkout"));
        assert_eq!(s.form.as_ref().unwrap().view, "assets_checkout_form");
        assert!(!s.form.as_ref().unwrap().editing);
        fill(
            &mut s,
            &[
                ("custodian", "Katherine Johnson"),
                ("due_date", "2026-10-31"),
            ],
        );
        let mut mint = ids();
        assert!(s.save_form(&mut mint));
        let a = s.book.get::<Asset>(&id).unwrap();
        assert_eq!(a.status, Status::CheckedOut);
        assert_eq!(a.custodian, "Katherine Johnson");
        let k = s.book.open_checkout(&id).unwrap().clone();
        assert_eq!(
            (k.out, k.due),
            (today(), NaiveDate::from_ymd_opt(2026, 10, 31))
        );
        let mut keys = written(&mut s);
        keys.sort();
        let mut expected = vec![
            format!("put erp/assets/{id}.json"),
            format!("put erp/checkouts/{}.json", k.id),
        ];
        expected.sort();
        assert_eq!(keys, expected);

        s.open(&format!("/accounting/assets/{id}/checkout"));
        fill(&mut s, &[("custodian", "Someone else")]);
        assert!(!s.save_form(&mut mint), "it is out already");

        s.check_in(&id);
        let a = s.book.get::<Asset>(&id).unwrap();
        assert_eq!(a.status, Status::InUse);
        assert!(a.custodian.is_empty());
        assert!(s.book.open_checkout(&id).is_none());
        assert_eq!(
            s.book.get::<Checkout>(&k.id).unwrap().returned,
            Some(today())
        );
        assert_eq!(written(&mut s).len(), 2);
    }

    #[test]
    fn logging_maintenance_and_disposing_change_the_asset_they_were_opened_on() {
        let mut s = sampled();
        let id = s.book.asset_by_number("A-0008").unwrap().id.clone();
        let before = s.book.maintenance_of(&id).len();
        s.open(&format!("/accounting/assets/{id}/maintenance/new"));
        fill(
            &mut s,
            &[
                ("kind", "REPAIR"),
                ("description", "New tyres"),
                ("cost", "780"),
            ],
        );
        let mut mint = ids();
        assert!(s.save_form(&mut mint));
        let log = s.book.maintenance_of(&id);
        assert_eq!(log.len(), before + 1);
        let entry = log.iter().find(|m| m.description == "New tyres").unwrap();
        assert_eq!(
            (entry.kind, entry.cost, entry.date),
            (MaintenanceKind::Repair, 78_000, today())
        );
        assert_eq!(written(&mut s).len(), 1);

        s.open(&format!("/accounting/assets/{id}/dispose"));
        assert_eq!(
            s.form.as_ref().unwrap().values["disposal_date"],
            "2026-10-03"
        );
        fill(&mut s, &[("disposal_amount", "9.500")]);
        assert!(s.save_form(&mut mint));
        let a = s.book.get::<Asset>(&id).unwrap();
        assert_eq!(a.status, Status::Disposed);
        assert_eq!((a.disposed, a.disposal_amount), (Some(today()), 950_000));
        assert_eq!(written(&mut s), [format!("put erp/assets/{id}.json")]);
    }

    #[test]
    fn categories_and_locations_are_made_and_edited_through_their_forms() {
        let mut s = sampled();
        s.open("/assets/categories");
        s.open("/assets/categories/new");
        assert_eq!(s.form.as_ref().unwrap().values["useful_life_years"], "5");
        fill(&mut s, &[("name", "Tools")]);
        let mut mint = ids();
        assert!(s.save_form(&mut mint));
        let tools = s.book.category_by_name("Tools").unwrap().clone();
        assert_eq!(
            (tools.life_years, s.page.as_str()),
            (5, "/assets/categories")
        );
        s.open(&format!("/assets/categories/{}/edit", tools.id));
        fill(&mut s, &[("useful_life_years", "8")]);
        assert!(s.save_form(&mut mint));
        assert_eq!(s.book.get::<Category>(&tools.id).unwrap().life_years, 8);
        s.open("/assets/locations/new");
        fill(&mut s, &[("name", "Home office")]);
        assert!(s.save_form(&mut mint));
        assert!(s.book.location_by_name("home office").is_some());
        assert_eq!(
            written(&mut s).len(),
            2,
            "one write per key: the category's newest"
        );
        let _ = Location::FOLDER;
    }

    #[test]
    fn deleting_an_asset_deletes_its_files_and_returns_to_the_register() {
        let mut s = sampled();
        let id = s.book.asset_by_number("A-0001").unwrap().id.clone();
        s.open(&format!("/accounting/assets/{id}"));
        s.delete_asset(&id);
        assert_eq!(s.page, HOME);
        assert!(s.book.get::<Asset>(&id).is_none());
        let keys = written(&mut s);
        assert!(
            keys.contains(&format!("delete erp/assets/{id}.json")),
            "{keys:?}"
        );
        assert!(keys.len() >= 2, "its check-out too: {keys:?}");
        assert!(keys.iter().all(|k| k.starts_with("delete ")));
    }

    #[test]
    fn exports_and_the_depreciation_run_go_into_the_data_tree() {
        let mut s = sampled();
        assert_eq!(s.export_register(), "erp/exports/assets-2026-10-03.csv");
        let id = s.book.asset_by_number("A-0001").unwrap().id.clone();
        assert_eq!(
            s.export_schedule(&id).as_deref(),
            Some("erp/exports/schedule-A-0001-2026-10-03.csv")
        );
        assert_eq!(s.export_schedule("nobody"), None);
        let preview = s.run_preview(2026);
        let total: i64 = preview.iter().map(|(_, _, _, amount)| amount).sum();
        let expected: i64 = s
            .book
            .assets
            .iter()
            .map(|a| depreciation::depreciation_in_year(a, 2026))
            .sum();
        assert_eq!(total, expected);
        assert!(preview.iter().all(|(_, _, _, amount)| *amount > 0));
        assert_eq!(s.post_run(2026), "erp/exports/depreciation-run-2026.csv");
        let batch = s.queue.take().unwrap();
        let journal = batch
            .iter()
            .find_map(|w| match w {
                Write::Put { key, bytes } if key.ends_with("depreciation-run-2026.csv") => {
                    Some(bytes.clone())
                }
                _ => None,
            })
            .unwrap();
        let text = String::from_utf8(journal).unwrap();
        assert!(
            text.starts_with("asset_number,name,category,year,depreciation"),
            "{text}"
        );
        assert!(
            text.contains(&format!("TOTAL,,,2026,{}", money::file_amount(expected))),
            "{text}"
        );
        assert_eq!(batch.len(), 3);
    }

    #[test]
    fn the_sample_goes_into_an_empty_register_only() {
        let mut s = State::new(today());
        s.load(&[]);
        let mut mint = ids();
        let n = s.seed_sample(&mut mint);
        assert!(n > 20);
        assert_eq!(s.queue.pending(), n);
        assert_eq!(s.seed_sample(&mut mint), 0, "not twice");
    }

    #[test]
    fn loading_reads_the_files_and_names_the_ones_it_skipped() {
        let mut s = State::new(today());
        assert!(!s.loaded);
        let a = Asset::new("a1", "A-0001", "Desk", today(), 100, 1);
        s.load(&[
            store::write_of(&a),
            ("erp/assets/bad.json".to_string(), b"{".to_vec()),
        ]);
        assert!(s.loaded);
        assert_eq!(s.book.assets, [a]);
        assert_eq!(s.skipped.len(), 1);
        assert!(s.notice.contains("1 file"), "{}", s.notice);
    }

    #[test]
    fn an_import_previews_with_the_guessed_mapping_then_writes() {
        let mut s = sampled();
        let before = s.book.assets.len();
        s.start_import(
            "new.csv",
            "Bezeichnung;Kategorie;Anschaffungsdatum;Anschaffungskosten;Nutzungsdauer\n\
             Drill press;Tools;01.03.2026;2.400,00;8\n\
             Ladder;Ladders;02.03.2026;189,00;\n",
        );
        assert_eq!(s.page, "/assets/import");
        let draft = s.import.as_ref().unwrap();
        assert_eq!(draft.preview.created, 1);
        assert_eq!(
            draft.preview.problems.len(),
            1,
            "the ladder has no life, and Ladders is new"
        );
        s.set_mapping(4, csv_io::Field::Skip);
        assert_eq!(s.import.as_ref().unwrap().preview.created, 0);
        s.set_mapping(4, csv_io::Field::Life);
        let mut mint = ids();
        assert_eq!(s.commit_import(&mut mint), 1);
        assert_eq!(s.book.assets.len(), before + 1);
        assert!(s.book.category_by_name("Tools").is_some());
        assert_eq!(written(&mut s).len(), 2, "the asset and its new category");
        assert!(s.import.is_none());
    }

    #[test]
    fn back_returns_to_the_page_before() {
        let mut s = sampled();
        s.open("/assets/categories");
        s.open("/assets/locations");
        s.go_back();
        assert_eq!(s.page, "/assets/categories");
        s.go_back();
        assert_eq!(s.page, HOME);
        s.go_back();
        assert_eq!(s.page, HOME, "the register is the first page");
        s.open("/nowhere");
        assert_eq!(s.page, HOME);
        assert!(!s.notice.is_empty());
        let _ = (ViewKind::Table, Kind::Asset, model::VERSION);
    }
}
