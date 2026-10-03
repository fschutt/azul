//! CSV export and import of the asset register.
//!
//! The export ([`export_assets`]) writes one asset per row under a header
//! of the ERP schema's field names (`asset_number`, `acquisition_cost`,
//! ...): amounts as plain decimals (`1596.64`), days as `YYYY-MM-DD`, the
//! method and status as their codes, names (not ids) for the category and
//! location, and the book value on the day of the export. It goes INTO the
//! data tree (`erp/exports/<name>.csv`, user ruling). [`export_schedule`]
//! writes one asset's depreciation schedule.
//!
//! The import reads what spreadsheets and other asset registers write
//! ([`parse`]: RFC 4180 through the `csv` crate, a leading byte-order mark
//! dropped, the separator - comma, semicolon or tab - the one the header
//! line holds most of). Each column is MAPPED to a field: the mapping starts
//! from the header's name ([`guess`]: the ERP names, English labels, German
//! headers like `Inventarnummer` / `Anschaffungskosten` / `Nutzungsdauer`)
//! and the import screen lets the user change it. [`import_assets`] turns
//! the rows into assets: a row whose asset number is in the register
//! UPDATES that asset (only the mapped, non-empty cells), any other row
//! creates one; categories and locations are matched by name and created
//! when new; a row that cannot be read is left out and named with its line.

use chrono::NaiveDate;

use crate::{
    depreciation,
    model::{self, Asset, Category, Location, Method, Status},
    money,
    store::Book,
};

/// What a CSV column is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Field {
    /// Not imported.
    Skip,
    Number,
    Name,
    /// A category's name.
    Category,
    /// A location's name.
    Location,
    Serial,
    Acquired,
    Cost,
    Residual,
    Life,
    Method,
    /// The declining-balance rate in percent.
    Rate,
    Status,
    Custodian,
    MaintenanceMonths,
    Disposed,
    DisposalAmount,
    /// Computed: exported, never imported.
    BookValue,
    Notes,
}

impl Field {
    /// The export's columns, in order.
    pub const EXPORT: [Field; 18] = [
        Field::Number,
        Field::Name,
        Field::Category,
        Field::Location,
        Field::Serial,
        Field::Acquired,
        Field::Cost,
        Field::Residual,
        Field::Life,
        Field::Method,
        Field::Rate,
        Field::Status,
        Field::Custodian,
        Field::MaintenanceMonths,
        Field::Disposed,
        Field::DisposalAmount,
        Field::BookValue,
        Field::Notes,
    ];

    /// Every choice of the import's mapping, "Skip" first.
    pub const CHOICES: [Field; 18] = [
        Field::Skip,
        Field::Number,
        Field::Name,
        Field::Category,
        Field::Location,
        Field::Serial,
        Field::Acquired,
        Field::Cost,
        Field::Residual,
        Field::Life,
        Field::Method,
        Field::Rate,
        Field::Status,
        Field::Custodian,
        Field::MaintenanceMonths,
        Field::Disposed,
        Field::DisposalAmount,
        Field::Notes,
    ];

    /// The export's header: the ERP schema's name.
    #[must_use]
    pub fn header(self) -> &'static str {
        match self {
            Field::Skip => "",
            Field::Number => "asset_number",
            Field::Name => "name",
            Field::Category => "category",
            Field::Location => "location",
            Field::Serial => "serial_number",
            Field::Acquired => "acquisition_date",
            Field::Cost => "acquisition_cost",
            Field::Residual => "residual_value",
            Field::Life => "useful_life_years",
            Field::Method => "depreciation_method",
            Field::Rate => "declining_rate_percent",
            Field::Status => "status",
            Field::Custodian => "custodian",
            Field::MaintenanceMonths => "maintenance_interval_months",
            Field::Disposed => "disposal_date",
            Field::DisposalAmount => "disposal_amount",
            Field::BookValue => "book_value",
            Field::Notes => "notes",
        }
    }

    /// What the mapping control shows.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Field::Skip => "(skip)",
            Field::Number => "Asset number",
            Field::Name => "Name",
            Field::Category => "Category",
            Field::Location => "Location",
            Field::Serial => "Serial number",
            Field::Acquired => "Acquisition date",
            Field::Cost => "Acquisition cost",
            Field::Residual => "Residual value",
            Field::Life => "Useful life (years)",
            Field::Method => "Depreciation method",
            Field::Rate => "Declining rate (%)",
            Field::Status => "Status",
            Field::Custodian => "Custodian",
            Field::MaintenanceMonths => "Maintenance interval (months)",
            Field::Disposed => "Disposal date",
            Field::DisposalAmount => "Disposal amount",
            Field::BookValue => "Book value",
            Field::Notes => "Notes",
        }
    }

    /// Its index in [`Field::CHOICES`] (0 = skip; the book value is not a
    /// choice).
    #[must_use]
    pub fn choice_index(self) -> usize {
        Field::CHOICES.iter().position(|f| *f == self).unwrap_or(0)
    }
}

/// The field a column named `header` most likely holds: the ERP names,
/// English labels and German headers, any case and punctuation; `Skip` for
/// one nobody knows.
#[must_use]
pub fn guess(header: &str) -> Field {
    let _ = header;
    todo!("GREEN")
}

/// A CSV file: its header and its rows (each as long as the header).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Table {
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

impl Table {
    /// The mapping [`guess`] starts with: one field per header.
    #[must_use]
    pub fn guessed_mapping(&self) -> Vec<Field> {
        self.headers.iter().map(|h| guess(h)).collect()
    }
}

/// Reads a CSV text; `Err` says why it is no table.
pub fn parse(text: &str) -> Result<Table, String> {
    let _ = text;
    todo!("GREEN")
}

/// The register as CSV: the [`Field::EXPORT`] columns, one asset per row,
/// the book value on `today`.
#[must_use]
pub fn export_assets(book: &Book, today: NaiveDate) -> String {
    let _ = (book, today, depreciation::book_value_on);
    todo!("GREEN")
}

/// One asset's depreciation schedule as CSV.
#[must_use]
pub fn export_schedule(asset: &Asset) -> String {
    let _ = asset;
    todo!("GREEN")
}

/// What an import made of the rows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Import {
    /// The assets to write: new ones and changed ones.
    pub assets: Vec<Asset>,
    /// The categories the rows named that were not there.
    pub categories: Vec<Category>,
    /// The locations the rows named that were not there.
    pub locations: Vec<Location>,
    pub created: usize,
    pub updated: usize,
    /// One sentence per row left out: `Row 4: ...`.
    pub problems: Vec<String>,
}

/// The assets of `table` with each column mapped by `mapping` (by
/// position), against the register `book`; `new_id` mints the ids of new
/// records.
pub fn import_assets(
    table: &Table,
    mapping: &[Field],
    book: &Book,
    new_id: &mut dyn FnMut() -> String,
) -> Import {
    let _ = (
        table,
        mapping,
        book,
        new_id,
        money::parse_amount,
        model::parse_date,
    );
    let _ = (Method::StraightLine, Status::InUse);
    todo!("GREEN")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::MaintenanceKind;

    fn day(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    /// Ids `id-1`, `id-2`, ...
    fn ids() -> impl FnMut() -> String {
        let mut n = 0;
        move || {
            n += 1;
            format!("id-{n}")
        }
    }

    fn register() -> Book {
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
            name: "Head office, Munich".into(),
            address: String::new(),
            notes: String::new(),
        });
        let mut a = Asset::new(
            "a1",
            "A-0042",
            "ThinkPad X1, \"Carbon\"",
            day(2026, 1, 15),
            159_664,
            3,
        );
        a.category = "c-it".into();
        a.location = "l-hq".into();
        a.serial = "PF-2XK91".into();
        a.custodian = "Ada Lovelace".into();
        a.status = Status::CheckedOut;
        a.notes = "Two lines:\nthe second".into();
        b.put(a);
        let mut press = Asset::new("a2", "A-0007", "Press", day(2024, 4, 1), 1_000_000, 5);
        press.method = Method::DecliningBalance;
        press.declining_rate_bp = 2500;
        press.residual = 100_000;
        press.maintenance_months = 6;
        b.put(press);
        let mut sold = Asset::new("a3", "A-0001", "Van", day(2020, 7, 1), 3_000_000, 6);
        sold.disposed = Some(day(2025, 3, 31));
        sold.disposal_amount = 950_000;
        sold.status = Status::Disposed;
        b.put(sold);
        b.sort();
        // A log entry: not part of the register's CSV.
        b.put(model::MaintenanceEntry {
            id: "m1".into(),
            asset: "a2".into(),
            date: day(2026, 1, 1),
            kind: MaintenanceKind::Service,
            description: String::new(),
            cost: 1,
            by: String::new(),
        });
        b
    }

    #[test]
    fn a_csv_file_is_read_with_quotes_line_breaks_a_bom_and_its_separator() {
        let text = "\u{feff}Inventarnummer;Bezeichnung;Notiz\r\nA-1;\"Desk; oak\";\"Line 1\nLine \"\"2\"\"\"\r\nA-2;Chair\r\n";
        let t = parse(text).unwrap();
        assert_eq!(t.headers, ["Inventarnummer", "Bezeichnung", "Notiz"]);
        assert_eq!(t.rows.len(), 2);
        assert_eq!(t.rows[0], ["A-1", "Desk; oak", "Line 1\nLine \"2\""]);
        assert_eq!(
            t.rows[1],
            ["A-2", "Chair", ""],
            "a short row is padded to the header"
        );
        let tabs = parse("a\tb\n1\t2\n").unwrap();
        assert_eq!(tabs.rows[0], ["1", "2"]);
        assert!(parse("").is_err());
        assert!(parse("\u{feff}  \n").is_err());
    }

    #[test]
    fn columns_are_guessed_from_erp_names_english_labels_and_german_headers() {
        for (header, field) in [
            ("asset_number", Field::Number),
            ("Asset No.", Field::Number),
            ("Inventarnummer", Field::Number),
            ("Bezeichnung", Field::Name),
            ("NAME", Field::Name),
            ("Kategorie", Field::Category),
            ("Standort", Field::Location),
            ("Serial number", Field::Serial),
            ("Anschaffungsdatum", Field::Acquired),
            ("Purchase date", Field::Acquired),
            ("Anschaffungskosten", Field::Cost),
            ("acquisition_cost", Field::Cost),
            ("Restwert", Field::Residual),
            ("Nutzungsdauer (Jahre)", Field::Life),
            ("Useful life", Field::Life),
            ("AfA-Methode", Field::Method),
            ("declining_rate_percent", Field::Rate),
            ("Status", Field::Status),
            ("Assigned to", Field::Custodian),
            ("Wartungsintervall (Monate)", Field::MaintenanceMonths),
            ("disposal_date", Field::Disposed),
            ("Disposal amount", Field::DisposalAmount),
            ("Buchwert", Field::BookValue),
            ("Bemerkungen", Field::Notes),
            ("colour", Field::Skip),
            ("", Field::Skip),
        ] {
            assert_eq!(guess(header), field, "{header}");
        }
        for f in Field::EXPORT {
            assert_eq!(
                guess(f.header()),
                f,
                "the export's own header {}",
                f.header()
            );
            assert_eq!(guess(f.label()), f, "the mapping's label {}", f.label());
        }
    }

    #[test]
    fn the_export_writes_plain_amounts_iso_days_codes_and_names() {
        let csv = export_assets(&register(), day(2026, 12, 31));
        let t = parse(&csv).unwrap();
        let headers: Vec<&str> = Field::EXPORT.iter().map(|f| f.header()).collect();
        assert_eq!(t.headers, headers);
        assert_eq!(t.rows.len(), 3, "one row per asset, by number");
        let row = &t.rows[2];
        let col = |f: Field| Field::EXPORT.iter().position(|x| *x == f).unwrap();
        assert_eq!(row[col(Field::Number)], "A-0042");
        assert_eq!(row[col(Field::Name)], "ThinkPad X1, \"Carbon\"");
        assert_eq!(row[col(Field::Category)], "IT equipment");
        assert_eq!(row[col(Field::Location)], "Head office, Munich");
        assert_eq!(row[col(Field::Acquired)], "2026-01-15");
        assert_eq!(row[col(Field::Cost)], "1596.64");
        assert_eq!(row[col(Field::Method)], "STRAIGHT_LINE");
        assert_eq!(row[col(Field::Status)], "CHECKED_OUT");
        assert_eq!(
            row[col(Field::BookValue)],
            "1064.43",
            "the book value at the end of 2026"
        );
        assert_eq!(row[col(Field::Notes)], "Two lines:\nthe second");
        let press = &t.rows[1];
        assert_eq!(press[col(Field::Rate)], "25.00");
        assert_eq!(press[col(Field::Residual)], "1000.00");
        let van = &t.rows[0];
        assert_eq!(van[col(Field::Disposed)], "2025-03-31");
        assert_eq!(van[col(Field::DisposalAmount)], "9500.00");
        assert_eq!(van[col(Field::BookValue)], "0.00", "off the books");
        assert_eq!(row[col(Field::Disposed)], "", "no disposal: an empty cell");
    }

    #[test]
    fn re_importing_an_export_updates_every_asset_and_changes_nothing() {
        let book = register();
        let csv = export_assets(&book, day(2026, 10, 3));
        let t = parse(&csv).unwrap();
        let mapping = t.guessed_mapping();
        let mut mint = ids();
        let import = import_assets(&t, &mapping, &book, &mut mint);
        assert!(import.problems.is_empty(), "{:?}", import.problems);
        assert_eq!((import.created, import.updated), (0, 3));
        assert!(import.categories.is_empty() && import.locations.is_empty());
        assert_eq!(import.assets, book.assets);
    }

    #[test]
    fn an_export_imported_into_an_empty_register_makes_the_same_assets() {
        let book = register();
        let csv = export_assets(&book, day(2026, 10, 3));
        let t = parse(&csv).unwrap();
        let mut mint = ids();
        let import = import_assets(&t, &t.guessed_mapping(), &Book::default(), &mut mint);
        assert!(import.problems.is_empty(), "{:?}", import.problems);
        assert_eq!((import.created, import.updated), (3, 0));
        assert_eq!(import.categories.len(), 1);
        assert_eq!(import.categories[0].name, "IT equipment");
        assert_eq!(
            import.categories[0].life_years, 3,
            "a new category takes its first asset's life"
        );
        assert_eq!(import.locations.len(), 1);
        let mut fresh = Book::default();
        for c in &import.categories {
            fresh.put(c.clone());
        }
        for l in &import.locations {
            fresh.put(l.clone());
        }
        for (old, new) in book.assets.iter().zip(&import.assets) {
            assert_ne!(old.id, new.id);
            assert_eq!(
                book.category_name(&old.category),
                fresh.category_name(&new.category)
            );
            assert_eq!(
                book.location_name(&old.location),
                fresh.location_name(&new.location)
            );
            let same = Asset {
                id: old.id.clone(),
                category: old.category.clone(),
                location: old.location.clone(),
                ..new.clone()
            };
            assert_eq!(&same, old);
        }
    }

    #[test]
    fn an_import_maps_columns_by_choice_and_names_the_rows_it_left_out() {
        let text = "Nr;Bezeichnung;Gruppe;Kaufdatum;Preis;Jahre;Methode\n\
                    ;Laptop;IT;15.01.2026;1.596,64;3;linear\n\
                    ;Monitor;it;01.02.2026;349,00;;\n\
                    ;;IT;01.02.2026;10;3;linear\n\
                    ;Phone;IT;31.02.2026;10;3;linear\n\
                    ;Tablet;IT;01.03.2026;zehn;3;linear\n\
                    ;Printer;IT;01.03.2026;100;3;sum of years\n\
                    ;;;;;;\n";
        let t = parse(text).unwrap();
        let mapping = [
            Field::Number,
            Field::Name,
            Field::Category, // "Gruppe" is no header guess knows: the user chose it
            Field::Acquired,
            Field::Cost,
            Field::Life,
            Field::Method,
        ];
        let mut book = Book::default();
        book.put(Asset::new("old", "A-0009", "Old", day(2020, 1, 1), 1, 1));
        let mut mint = ids();
        let import = import_assets(&t, &mapping, &book, &mut mint);
        assert_eq!(import.created, 2);
        let names: Vec<&str> = import.assets.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, ["Laptop", "Monitor"]);
        let numbers: Vec<&str> = import.assets.iter().map(|a| a.number.as_str()).collect();
        assert_eq!(
            numbers,
            ["A-0010", "A-0011"],
            "numbered after the register's highest"
        );
        assert_eq!(import.assets[0].cost, 159_664);
        assert_eq!(import.assets[0].acquired, day(2026, 1, 15));
        assert_eq!(import.categories.len(), 1, "IT and it are one new category");
        assert_eq!(import.assets[1].category, import.categories[0].id);
        assert_eq!(import.assets[1].life_years, 3, "the life its category has");
        assert_eq!(import.problems.len(), 4, "{:?}", import.problems);
        assert!(
            import.problems[0].starts_with("Row 4:"),
            "{}",
            import.problems[0]
        );
        assert!(
            import.problems[1].starts_with("Row 5:") && import.problems[1].contains("31.02.2026")
        );
        assert!(import.problems[2].starts_with("Row 6:") && import.problems[2].contains("zehn"));
        assert!(
            import.problems[3].starts_with("Row 7:") && import.problems[3].contains("sum of years")
        );
    }

    #[test]
    fn a_row_with_a_known_number_changes_only_its_mapped_cells() {
        let book = register();
        let t = parse("asset_number,custodian,status\nA-0007,Grace Hopper,checked out\n").unwrap();
        let mut mint = ids();
        let import = import_assets(&t, &t.guessed_mapping(), &book, &mut mint);
        assert!(import.problems.is_empty(), "{:?}", import.problems);
        assert_eq!((import.created, import.updated), (0, 1));
        let mut expected = book.get::<Asset>("a2").unwrap().clone();
        expected.custodian = "Grace Hopper".into();
        expected.status = Status::CheckedOut;
        assert_eq!(import.assets, [expected]);
    }

    #[test]
    fn the_schedule_exports_year_by_year() {
        let a = Asset::new("a", "A-1", "ThinkPad", day(2026, 1, 15), 159_664, 3);
        let csv = export_schedule(&a);
        let t = parse(&csv).unwrap();
        assert_eq!(
            t.headers,
            [
                "year",
                "months",
                "opening_value",
                "depreciation",
                "accumulated",
                "closing_value"
            ]
        );
        assert_eq!(
            t.rows[0],
            ["2026", "12", "1596.64", "532.21", "532.21", "1064.43"]
        );
        assert_eq!(
            t.rows[2],
            ["2028", "12", "532.22", "532.22", "1596.64", "0.00"]
        );
        assert!(csv.contains("\r\n"), "RFC 4180 line ends");
    }
}
