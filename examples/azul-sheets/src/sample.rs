//! The `--sample` workbook, "Budget 2027" (excel.md §6), built through the
//! [`SheetEngine`] trait only, so it works on any engine: "Summary" (a title,
//! a frozen header row, five categories over three months, Q1 sums, a Q2
//! projection through the `GrowthRate` name and a Total row), "Monthly"
//! (twelve months x six categories with a SUM row) and "Assumptions" (the
//! growth rate the name points at).

use crate::engine::{BorderPreset, CellAddr, CellArea, EngineError, SheetEngine, StylePatch};

/// The sheets of the sample, by index.
pub const SUMMARY: u32 = 0;
pub const MONTHLY: u32 = 1;
pub const ASSUMPTIONS: u32 = 2;

/// The defined name of the growth rate.
pub const GROWTH_RATE: &str = "GrowthRate";

const HEADER_FILL: &str = "#DDEBF7";
const RULE: &str = "#5B7DB1";
const MONEY: &str = "#,##0.00";

const CATEGORIES: &[(&str, [f64; 3])] = &[
    ("Rent", [1250.0, 1250.0, 1250.0]),
    ("Food", [412.30, 389.10, 401.75]),
    ("Transport", [120.0, 95.50, 130.0]),
    ("Utilities", [210.40, 198.20, 205.0]),
    ("Leisure", [150.0, 220.0, 180.0]),
];

const MONTHS: &[&str] = &[
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

const MONTHLY_CATEGORIES: &[&str] = &[
    "Rent",
    "Food",
    "Transport",
    "Utilities",
    "Leisure",
    "Savings",
];

/// The column letters of the sample (columns 1..=13).
fn letter(column: i32) -> char {
    char::from(b'A' + (column - 1) as u8)
}

fn row(cells: &[&str]) -> Vec<String> {
    cells.iter().map(|s| s.to_string()).collect()
}

/// Bold + fill on a header area.
fn header(engine: &mut dyn SheetEngine, area: CellArea) -> Result<(), EngineError> {
    engine.update_style(area, &StylePatch::Bold(true))?;
    engine.update_style(area, &StylePatch::Fill(Some(HEADER_FILL.to_string())))
}

/// Writes the sample into `engine`'s current workbook (call it on an empty
/// one: the worker's `Sample` command starts a new workbook first).
pub fn budget(engine: &mut dyn SheetEngine) -> Result<(), EngineError> {
    while engine.sheets().len() < 3 {
        engine.add_sheet()?;
    }
    engine.rename_sheet(SUMMARY, "Summary")?;
    engine.rename_sheet(MONTHLY, "Monthly")?;
    engine.rename_sheet(ASSUMPTIONS, "Assumptions")?;

    // ---- Assumptions: the growth rate first, the name the Summary reads ----
    engine.set_inputs(
        CellAddr::new(ASSUMPTIONS, 1, 1),
        &[
            row(&["Assumption", "", "Value"]),
            row(&["Growth rate", "", "0.03"]),
            row(&["Currency", "", "EUR"]),
        ],
    )?;
    header(engine, CellArea::spanning(ASSUMPTIONS, 1, 1, 1, 3))?;
    engine.update_style(
        CellArea::cell(CellAddr::new(ASSUMPTIONS, 2, 3)),
        &StylePatch::NumberFormat(String::from("0.0%")),
    )?;
    engine.set_column_width(ASSUMPTIONS, 1, 1, 140.0)?;
    engine.add_defined_name(GROWTH_RATE, None, "Assumptions!$C$2")?;

    // ---- Summary ----
    let mut rows = vec![
        row(&["Household budget 2027"]),
        row(&["Category", "Jan", "Feb", "Mar", "Q1", "Q2 (projected)"]),
    ];
    for (i, (name, months)) in CATEGORIES.iter().enumerate() {
        let r = i + 3;
        rows.push(vec![
            name.to_string(),
            months[0].to_string(),
            months[1].to_string(),
            months[2].to_string(),
            format!("=SUM(B{r}:D{r})"),
            format!("=E{r}*(1+{GROWTH_RATE})"),
        ]);
    }
    let last = CATEGORIES.len() + 2;
    let total_row = last + 1;
    let mut total = vec![String::from("Total")];
    for column in 2..=6 {
        let c = letter(column);
        total.push(format!("=SUM({c}3:{c}{last})"));
    }
    rows.push(total);
    engine.set_inputs(CellAddr::new(SUMMARY, 1, 1), &rows)?;

    let title = CellArea::cell(CellAddr::new(SUMMARY, 1, 1));
    engine.update_style(title, &StylePatch::Bold(true))?;
    engine.update_style(title, &StylePatch::FontSize(16))?;
    header(engine, CellArea::spanning(SUMMARY, 2, 1, 2, 6))?;
    let total_area = CellArea::spanning(SUMMARY, total_row as i32, 1, total_row as i32, 6);
    engine.update_style(total_area, &StylePatch::Bold(true))?;
    engine.update_style(
        total_area,
        &StylePatch::Borders {
            preset: BorderPreset::Top,
            color: RULE.to_string(),
        },
    )?;
    engine.update_style(
        CellArea::spanning(SUMMARY, 3, 2, total_row as i32, 6),
        &StylePatch::NumberFormat(MONEY.to_string()),
    )?;
    engine.set_column_width(SUMMARY, 1, 1, 140.0)?;
    engine.set_column_width(SUMMARY, 2, 6, 100.0)?;
    engine.set_frozen(SUMMARY, 2, 0)?;

    // ---- Monthly: twelve months x six categories ----
    let mut rows = vec![{
        let mut head = vec![String::from("Category")];
        head.extend(MONTHS.iter().map(|m| m.to_string()));
        head
    }];
    for (i, name) in MONTHLY_CATEGORIES.iter().enumerate() {
        let mut line = vec![name.to_string()];
        for m in 0..MONTHS.len() {
            // A steady, readable pattern: a base per category plus a seasonal step.
            let base = [1250.0, 400.0, 115.0, 205.0, 180.0, 300.0][i];
            let season = ((m as f64) - 5.5).abs() * (i as f64 + 1.0) * 3.0;
            line.push(format!("{:.2}", base + season));
        }
        rows.push(line);
    }
    let last = MONTHLY_CATEGORIES.len() + 1;
    let mut total = vec![String::from("Total")];
    for column in 2..=(MONTHS.len() as i32 + 1) {
        let c = letter(column);
        total.push(format!("=SUM({c}2:{c}{last})"));
    }
    rows.push(total);
    engine.set_inputs(CellAddr::new(MONTHLY, 1, 1), &rows)?;
    header(
        engine,
        CellArea::spanning(MONTHLY, 1, 1, 1, MONTHS.len() as i32 + 1),
    )?;
    let total_row = last as i32 + 1;
    engine.update_style(
        CellArea::spanning(MONTHLY, total_row, 1, total_row, MONTHS.len() as i32 + 1),
        &StylePatch::Bold(true),
    )?;
    engine.update_style(
        CellArea::spanning(MONTHLY, 2, 2, total_row, MONTHS.len() as i32 + 1),
        &StylePatch::NumberFormat(MONEY.to_string()),
    )?;
    engine.set_column_width(MONTHLY, 1, 1, 120.0)?;
    engine.set_frozen(MONTHLY, 1, 1)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{engine::CellValue, fake_engine::FakeEngine};

    fn built() -> FakeEngine {
        let mut e = FakeEngine::new();
        budget(&mut e).unwrap();
        e
    }

    #[test]
    fn the_budget_has_a_summary_a_monthly_and_an_assumptions_sheet() {
        let names: Vec<String> = built().sheets().into_iter().map(|s| s.name).collect();
        assert_eq!(names, vec!["Summary", "Monthly", "Assumptions"]);
    }

    #[test]
    fn the_summary_holds_the_categories_the_q1_sums_and_the_total_row() {
        let e = built();
        let at = |r, c| CellAddr::new(SUMMARY, r, c);
        assert_eq!(e.cell_input(at(1, 1)), "Household budget 2027");
        assert_eq!(e.cell_input(at(3, 1)), "Rent");
        assert_eq!(e.cell_input(at(3, 5)), "=SUM(B3:D3)");
        assert_eq!(e.cell_input(at(3, 6)), "=E3*(1+GrowthRate)");
        assert_eq!(e.cell_input(at(8, 1)), "Total");
        assert_eq!(e.cell_input(at(8, 2)), "=SUM(B3:B7)");
        assert_eq!(
            e.cell_value(at(3, 5)),
            CellValue::Number(3750.0),
            "Rent's Q1"
        );
        assert_eq!(e.cell_formatted(at(3, 5)), "3,750.00");
        assert!(e.cell_style(at(2, 1)).bold);
        assert_eq!(e.cell_style(at(1, 1)).font_size, 16);
    }

    #[test]
    fn the_summary_freezes_its_two_header_rows_and_widens_the_category_column() {
        let e = built();
        assert_eq!(e.frozen(SUMMARY), (2, 0));
        assert_eq!(e.column_width(SUMMARY, 1), 140.0);
        assert_eq!(e.frozen(MONTHLY), (1, 1));
    }

    #[test]
    fn the_growth_rate_is_a_workbook_name_pointing_at_the_assumptions() {
        let e = built();
        let names = e.defined_names();
        let rate = names
            .iter()
            .find(|n| n.name == GROWTH_RATE)
            .expect("GrowthRate");
        assert_eq!(rate.scope, None);
        assert_eq!(rate.formula, "Assumptions!$C$2");
        assert_eq!(e.cell_input(CellAddr::new(ASSUMPTIONS, 2, 3)), "0.03");
    }

    #[test]
    fn the_monthly_sheet_has_twelve_months_and_a_sum_row() {
        let e = built();
        let at = |r, c| CellAddr::new(MONTHLY, r, c);
        assert_eq!(e.cell_input(at(1, 2)), "Jan");
        assert_eq!(e.cell_input(at(1, 13)), "Dec");
        assert_eq!(e.cell_input(at(8, 1)), "Total");
        assert_eq!(e.cell_input(at(8, 13)), "=SUM(M2:M7)");
    }
}
