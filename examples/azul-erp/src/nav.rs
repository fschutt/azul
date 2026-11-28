//! What the window's navigation is made of, as plain data (no widget): the sections and rows of the
//! sidebar, the quick-access buttons of the 18 px rail, the order of a record's action sheet on a
//! phone, and which columns a phone's table keeps. `ui/sidebar.rs`, `ui/sheet.rs` and
//! `ui/table.rs` turn it into the family's widgets and run what they report; nothing here knows a
//! widget, so every rule is tested without a window.
//!
//! The ERP's modules are the sections of the sidebar and the pages of a module (the views' menu
//! entries: Register, Categories, Locations, ...) its rows; a row carries the number of records the
//! page lists. A row's TAG is `module * TAG_STRIDE + page`: what the sidebar reports back.

/// How many pages one module's rows hold at most (more are dropped by the plan).
pub const MAX_PAGES: usize = 99;

/// How many modules the sidebar lists at most.
pub const MAX_MODULES: usize = 99;

/// A row's tag is `module * TAG_STRIDE + page`.
pub const TAG_STRIDE: usize = 100;

/// The tag of page `page` of module `module`.
#[must_use]
pub const fn tag(module: usize, page: usize) -> usize {
    module * TAG_STRIDE + page
}

/// The module and the page a tag stands for.
#[must_use]
pub const fn target(tag: usize) -> (usize, usize) {
    (tag / TAG_STRIDE, tag % TAG_STRIDE)
}

/// A page of a module, as the sidebar needs to know it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page {
    /// The path the row opens (the views' menu entry).
    pub path: String,
    /// The row's label.
    pub label: String,
    /// The row's icon, a Material name.
    pub icon: &'static str,
    /// How many records the page lists; `None` for a page that is no list (the reports).
    pub count: Option<usize>,
}

/// A module of the ERP: a section of the sidebar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Module {
    /// The section's title ("Assets").
    pub title: String,
    pub pages: Vec<Page>,
    /// The page the window shows (an index into `pages`).
    pub current: Option<usize>,
    /// Whether the section is open (its rows show).
    pub open: bool,
}

/// One row of the sidebar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NavRow {
    pub label: String,
    pub icon: &'static str,
    /// The count after the label; empty for none.
    pub badge: String,
    pub tag: usize,
    pub selected: bool,
}

/// One section of the sidebar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NavSection {
    pub title: String,
    pub open: bool,
    pub rows: Vec<NavRow>,
}

/// The count as a row's badge: nothing for none or zero, the number, and `999+` beyond.
#[must_use]
pub fn badge(count: Option<usize>) -> String {
    match count {
        None | Some(0) => String::new(),
        Some(n) if n > 999 => String::from("999+"),
        Some(n) => n.to_string(),
    }
}

/// The sections the sidebar shows, in order.
#[must_use]
pub fn plan(modules: &[Module]) -> Vec<NavSection> {
    modules
        .iter()
        .take(MAX_MODULES)
        .enumerate()
        .map(|(m, module)| NavSection {
            title: module.title.clone(),
            open: module.open,
            rows: module
                .pages
                .iter()
                .take(MAX_PAGES)
                .enumerate()
                .map(|(p, page)| NavRow {
                    label: page.label.clone(),
                    icon: page.icon,
                    badge: badge(page.count),
                    tag: tag(m, p),
                    selected: module.current == Some(p),
                })
                .collect(),
        })
        .collect()
}

/// The icon a menu entry (its key, `menu.fixed_assets`) is shown with; a plain page for a key
/// this table does not know.
#[must_use]
pub fn page_icon(key: &str) -> &'static str {
    static ICONS: [(&str, &str); 6] = [
        ("menu.fixed_assets", "inventory_2"),
        ("menu.asset_categories", "category"),
        ("menu.asset_locations", "place"),
        ("menu.maintenance", "build"),
        ("menu.checkouts", "swap_horiz"),
        ("menu.asset_reports", "bar_chart"),
    ];
    ICONS
        .iter()
        .find(|(k, _)| *k == key)
        .map_or("article", |(_, icon)| icon)
}

/// The page of a module the window is on: the entry whose path the window's page starts with, the
/// longest one when several do; a page no entry names (the CSV import and the depreciation run
/// start from the register) belongs to the first. `None` for a module without pages.
#[must_use]
pub fn current_page(paths: &[&str], page: &str) -> Option<usize> {
    if paths.is_empty() {
        return None;
    }
    let found = paths
        .iter()
        .enumerate()
        .filter(|(_, path)| is_under(page, path))
        .max_by_key(|(_, path)| path.len())
        .map(|(i, _)| i);
    Some(found.unwrap_or(0))
}

/// Whether `page` is `path` or a page below it (`/a/b` is under `/a`, `/ab` is not).
fn is_under(page: &str, path: &str) -> bool {
    let path = path.trim_end_matches('/');
    page == path
        || page
            .strip_prefix(path)
            .is_some_and(|rest| rest.starts_with('/'))
}

// ==== The rail's quick access ====

/// A quick-access button of the collapsed rail.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Quick {
    /// The CSV import page.
    ImportCsv,
    /// The page's CSV export.
    ExportCsv,
    /// A new record of the page's kind (the page's primary action).
    New,
}

/// What a quick-access button says and shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuickPlan {
    pub quick: Quick,
    /// The button's name, a key of AzERP's words.
    pub label_key: &'static str,
    pub icon: &'static str,
}

/// What the page has for the quick access.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct QuickInput {
    pub has_new: bool,
    pub has_import: bool,
    pub has_export: bool,
}

/// The rail's quick-access buttons for a page, in the order they are drawn: the one that is used
/// most comes LAST, nearest the thumb (the wireframe's rule): New. A button the page cannot run is
/// not there (a rail of greyed buttons says nothing).
#[must_use]
pub fn quick_plan(input: QuickInput) -> Vec<QuickPlan> {
    let mut plan = Vec::with_capacity(3);
    if input.has_import {
        plan.push(QuickPlan {
            quick: Quick::ImportCsv,
            label_key: "azerp-quick-import",
            icon: "upload_file",
        });
    }
    if input.has_export {
        plan.push(QuickPlan {
            quick: Quick::ExportCsv,
            label_key: "azerp-quick-export",
            icon: "download",
        });
    }
    if input.has_new {
        plan.push(QuickPlan {
            quick: Quick::New,
            label_key: "azerp-quick-new",
            icon: "add",
        });
    }
    plan
}

/// The tag a quick-access button reports.
#[must_use]
pub const fn quick_tag(quick: Quick) -> usize {
    match quick {
        Quick::ImportCsv => 1,
        Quick::ExportCsv => 2,
        Quick::New => 3,
    }
}

/// The button a reported tag stands for.
#[must_use]
pub const fn quick_of(tag: usize) -> Option<Quick> {
    match tag {
        1 => Some(Quick::ImportCsv),
        2 => Some(Quick::ExportCsv),
        3 => Some(Quick::New),
        _ => None,
    }
}

// ==== The record's action sheet ====

/// The places of a record's actions in the order the phone's sheet lists them: what is safe in the
/// order the record's view gives, what destroys (`danger`) last, in red. `danger[i]` says whether
/// action `i` destroys.
#[must_use]
pub fn sheet_order(danger: &[bool]) -> Vec<usize> {
    let safe = danger
        .iter()
        .enumerate()
        .filter(|(_, d)| !**d)
        .map(|(i, _)| i);
    let destroying = danger
        .iter()
        .enumerate()
        .filter(|(_, d)| **d)
        .map(|(i, _)| i);
    safe.chain(destroying).collect()
}

// ==== A phone's table ====

/// The most columns a phone's table keeps: the width of a phone holds three.
pub const PHONE_COLUMNS: usize = 3;

/// The narrowest a phone's column is squeezed to, px.
pub const MIN_COLUMN_PX: f32 = 72.0;

/// The columns a phone's table shows, as places in the view's columns: a table that has no more
/// than [`PHONE_COLUMNS`] shows all of them; a wider one its first two (what the row IS: its
/// number and its name) and its last (what it is IN: the status, the value) - the rest of the
/// record is in the Preview. A table of up to three columns keeps all.
#[must_use]
pub fn phone_columns(total: usize) -> Vec<usize> {
    if total <= PHONE_COLUMNS {
        (0..total).collect()
    } else {
        vec![0, 1, total - 1]
    }
}

/// `widths` scaled so that together they fill `available` px, none under `min`: a column that
/// would be narrower than `min` keeps `min` and the others share what is left in proportion. A
/// width that is no positive number counts as 1. `available` under `min` per column is raised to
/// it (the table then scrolls sideways).
#[must_use]
pub fn fit_widths(widths: &[f32], available: f32, min: f32) -> Vec<f32> {
    let n = widths.len();
    if n == 0 {
        return Vec::new();
    }
    let clean: Vec<f32> = widths
        .iter()
        .map(|w| if w.is_finite() && *w > 0.0 { *w } else { 1.0 })
        .collect();
    let min = if min.is_finite() && min > 0.0 {
        min
    } else {
        1.0
    };
    #[allow(clippy::cast_precision_loss)] // a handful of columns
    let floor = min * n as f32;
    let available = if available.is_finite() {
        available.max(floor)
    } else {
        floor
    };
    let mut pinned = vec![false; n];
    loop {
        let free: f32 = clean
            .iter()
            .zip(&pinned)
            .filter(|(_, p)| !**p)
            .map(|(w, _)| *w)
            .sum();
        #[allow(clippy::cast_precision_loss)]
        let pinned_px = pinned.iter().filter(|p| **p).count() as f32 * min;
        let scale = (available - pinned_px) / free.max(1.0);
        let mut changed = false;
        for (i, w) in clean.iter().enumerate() {
            if !pinned[i] && *w * scale < min {
                pinned[i] = true;
                changed = true;
            }
        }
        if !changed {
            return clean
                .iter()
                .zip(&pinned)
                .map(|(w, p)| if *p { min } else { *w * scale })
                .collect();
        }
    }
}

#[cfg(test)]
#[path = "nav_test.rs"]
mod nav_test;
