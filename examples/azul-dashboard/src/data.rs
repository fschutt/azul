//! The dashboard's data set: orders of an office-supply business, 25
//! columns each, generated DETERMINISTICALLY (row `n` is the same in every
//! run and on every machine: a splitmix64 stream seeded by the row index),
//! so a script can assert on a cell's text.
//!
//! One order is 28 bytes ([`Order`]: indices into small name tables, cents,
//! a day number), so 500,000 orders are 14 MB; the texts the table shows are
//! formatted when a cell is asked for ([`DataSet::text`]), never stored. The
//! numbers a column sorts and range-filters by come from [`DataSet::value`]
//! (dates as days since 1970-01-01, what azul's DataTable reads a Date
//! column's value as).
//!
//! Edits ([`DataSet::edit`]) are validated here: a quantity is a whole
//! number from 1 to 999, a priority one of the four names, a date a real
//! `YYYY-MM-DD` ... - the table shows the reason when one is refused.
//!
//! The chart half reads the same [`DataSet`] (aggregate it over
//! [`DataSet::orders`]).

use chrono::{Datelike, NaiveDate};

/// The orders the dashboard shows.
pub const ROWS: u32 = 500_000;

/// What a column holds: how the table sorts and filters it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Text,
    Number,
    Date,
}

/// One column of the table.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Column {
    pub title: &'static str,
    /// The width in px.
    pub width: f32,
    pub kind: Kind,
    /// The user may edit it (the app validates every edit).
    pub editable: bool,
}

const fn col(title: &'static str, width: f32, kind: Kind, editable: bool) -> Column {
    Column {
        title,
        width,
        kind,
        editable,
    }
}

/// The column indices, by name.
pub mod c {
    pub const ORDER: usize = 0;
    pub const ORDER_DATE: usize = 1;
    pub const SHIP_DATE: usize = 2;
    pub const REGION: usize = 3;
    pub const COUNTRY: usize = 4;
    pub const CITY: usize = 5;
    pub const CUSTOMER: usize = 6;
    pub const SEGMENT: usize = 7;
    pub const CATEGORY: usize = 8;
    pub const SUBCATEGORY: usize = 9;
    pub const PRODUCT: usize = 10;
    pub const SHIP_MODE: usize = 11;
    pub const PRIORITY: usize = 12;
    pub const QUANTITY: usize = 13;
    pub const UNIT_PRICE: usize = 14;
    pub const DISCOUNT: usize = 15;
    pub const SALES: usize = 16;
    pub const COST: usize = 17;
    pub const SHIPPING: usize = 18;
    pub const PROFIT: usize = 19;
    pub const MARGIN: usize = 20;
    pub const DAYS_TO_SHIP: usize = 21;
    pub const RETURNED: usize = 22;
    pub const RATING: usize = 23;
    pub const SALES_REP: usize = 24;
}

/// The 25 columns, in table order.
pub const COLUMNS: [Column; 25] = [
    col("Order", 96.0, Kind::Text, false),
    col("Order date", 96.0, Kind::Date, true),
    col("Ship date", 96.0, Kind::Date, false),
    col("Region", 112.0, Kind::Text, false),
    col("Country", 120.0, Kind::Text, false),
    col("City", 110.0, Kind::Text, false),
    col("Customer", 140.0, Kind::Text, false),
    col("Segment", 110.0, Kind::Text, false),
    col("Category", 120.0, Kind::Text, false),
    col("Sub-category", 110.0, Kind::Text, false),
    col("Product", 90.0, Kind::Text, false),
    col("Ship mode", 104.0, Kind::Text, false),
    col("Priority", 80.0, Kind::Text, true),
    col("Quantity", 76.0, Kind::Number, true),
    col("Unit price", 92.0, Kind::Number, true),
    col("Discount %", 84.0, Kind::Number, true),
    col("Sales", 100.0, Kind::Number, false),
    col("Cost", 100.0, Kind::Number, false),
    col("Shipping", 84.0, Kind::Number, false),
    col("Profit", 100.0, Kind::Number, false),
    col("Margin %", 80.0, Kind::Number, false),
    col("Days to ship", 92.0, Kind::Number, false),
    col("Returned", 80.0, Kind::Text, true),
    col("Rating", 64.0, Kind::Number, true),
    col("Sales rep", 130.0, Kind::Text, false),
];

// ==== The name tables ====

/// Region, its countries, each with three cities.
type Geo = (&'static str, [(&'static str, [&'static str; 3]); 3]);

pub const GEO: [Geo; 6] = [
    (
        "Europe",
        [
            ("Germany", ["Berlin", "Munich", "Hamburg"]),
            ("France", ["Paris", "Lyon", "Marseille"]),
            ("United Kingdom", ["London", "Manchester", "Leeds"]),
        ],
    ),
    (
        "North America",
        [
            ("United States", ["New York", "Chicago", "Seattle"]),
            ("Canada", ["Toronto", "Montreal", "Vancouver"]),
            ("Mexico", ["Mexico City", "Monterrey", "Guadalajara"]),
        ],
    ),
    (
        "Asia Pacific",
        [
            ("Japan", ["Tokyo", "Osaka", "Nagoya"]),
            ("Australia", ["Sydney", "Melbourne", "Perth"]),
            ("India", ["Mumbai", "Delhi", "Bangalore"]),
        ],
    ),
    (
        "Latin America",
        [
            ("Brazil", ["Sao Paulo", "Rio de Janeiro", "Brasilia"]),
            ("Argentina", ["Buenos Aires", "Cordoba", "Rosario"]),
            ("Chile", ["Santiago", "Valparaiso", "Concepcion"]),
        ],
    ),
    (
        "Middle East",
        [
            ("United Arab Emirates", ["Dubai", "Abu Dhabi", "Sharjah"]),
            ("Turkey", ["Istanbul", "Ankara", "Izmir"]),
            ("Israel", ["Tel Aviv", "Jerusalem", "Haifa"]),
        ],
    ),
    (
        "Africa",
        [
            ("South Africa", ["Johannesburg", "Cape Town", "Durban"]),
            ("Nigeria", ["Lagos", "Abuja", "Kano"]),
            ("Kenya", ["Nairobi", "Mombasa", "Kisumu"]),
        ],
    ),
];

/// The number of cities (`Order::city` is below it).
pub const CITIES: u8 = 54;

pub const SEGMENTS: [&str; 4] = ["Consumer", "Corporate", "Home Office", "Small Business"];

pub const CATEGORIES: [&str; 3] = ["Furniture", "Office Supplies", "Technology"];

/// Four per category, in category order (`subcategory / 4` is the category).
pub const SUBCATEGORIES: [&str; 12] = [
    "Chairs",
    "Tables",
    "Bookcases",
    "Furnishings",
    "Paper",
    "Binders",
    "Storage",
    "Art",
    "Phones",
    "Laptops",
    "Monitors",
    "Accessories",
];

/// The product code's prefix per sub-category.
const PRODUCT_PREFIX: [&str; 12] = ["CH", "TB", "BC", "FU", "PA", "BI", "ST", "AR", "PH", "LT", "MO", "AC"];

pub const SHIP_MODES: [&str; 4] = ["Standard", "Second Class", "First Class", "Same Day"];

pub const PRIORITIES: [&str; 4] = ["Low", "Medium", "High", "Critical"];

const FIRST_NAMES: [&str; 24] = [
    "Anna", "Ben", "Carla", "David", "Elena", "Felix", "Grace", "Hugo", "Ines", "Jonas", "Kira", "Liam",
    "Mara", "Noah", "Olga", "Paul", "Quinn", "Rosa", "Sven", "Tara", "Umar", "Vera", "Wim", "Yara",
];

const LAST_NAMES: [&str; 24] = [
    "Abe", "Berger", "Costa", "Dubois", "Eriksen", "Fischer", "Garcia", "Haddad", "Ito", "Jansen",
    "Kowalski", "Lopez", "Meyer", "Nakamura", "Okafor", "Petrov", "Quist", "Rossi", "Silva", "Tanaka",
    "Ueda", "Varga", "Weber", "Young",
];

/// The number of customers (`Order::customer` is below it).
pub const CUSTOMERS: u16 = 24 * 24;

pub const SALES_REPS: [&str; 20] = [
    "Alex Morgan", "Bea Lindqvist", "Chen Wei", "Dana Novak", "Emil Strand", "Farah Aziz",
    "Gus Meyer", "Hana Sato", "Ivo Kranjc", "Jade Fontaine", "Kemal Arslan", "Lena Vogel",
    "Mateo Ruiz", "Nora Haugen", "Oscar Lind", "Priya Nair", "Rui Santos", "Sofia Greco",
    "Tom Becker", "Uma Patel",
];

/// "Yes" / "No".
const YES_NO: [&str; 2] = ["No", "Yes"];

// ==== One order ====

/// One order, compact: the table's texts are formatted from it on demand.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Order {
    /// The order date, days since 1970-01-01.
    pub date: i32,
    pub unit_price_cents: u32,
    /// An index below [`CUSTOMERS`]: first name `customer / 24`, last name `customer % 24`.
    pub customer: u16,
    /// The product's number (its code is the sub-category's prefix and this).
    pub product: u16,
    pub quantity: u16,
    pub shipping_cents: u16,
    /// The cost as a share of the sales, in thousandths.
    pub cost_permille: u16,
    /// An index below [`CITIES`]: region `city / 9`, country `city / 3`.
    pub city: u8,
    pub segment: u8,
    /// An index into [`SUBCATEGORIES`] (the category is `subcategory / 4`).
    pub subcategory: u8,
    pub ship_mode: u8,
    pub priority: u8,
    /// Percent.
    pub discount: u8,
    pub days_to_ship: u8,
    pub returned: bool,
    /// 1..=5.
    pub rating: u8,
    pub rep: u8,
}

impl Order {
    /// The sales in cents: quantity x unit price, less the discount.
    #[must_use]
    pub fn sales_cents(&self) -> i64 {
        let gross = i64::from(self.quantity) * i64::from(self.unit_price_cents);
        gross * (100 - i64::from(self.discount.min(100))) / 100
    }

    /// The cost in cents.
    #[must_use]
    pub fn cost_cents(&self) -> i64 {
        self.sales_cents() * i64::from(self.cost_permille) / 1000
    }

    /// The profit in cents: sales less cost and shipping.
    #[must_use]
    pub fn profit_cents(&self) -> i64 {
        self.sales_cents() - self.cost_cents() - i64::from(self.shipping_cents)
    }

    /// The margin in percent of the sales (0 on no sales).
    #[must_use]
    pub fn margin_percent(&self) -> f64 {
        let sales = self.sales_cents();
        if sales == 0 {
            0.0
        } else {
            self.profit_cents() as f64 * 100.0 / sales as f64
        }
    }

    /// The ship date, days since 1970-01-01.
    #[must_use]
    pub fn ship_date(&self) -> i32 {
        self.date + i32::from(self.days_to_ship)
    }

    #[must_use]
    pub fn region(&self) -> &'static str {
        GEO[usize::from(self.city / 9) % 6].0
    }

    #[must_use]
    pub fn country(&self) -> &'static str {
        let (_, countries) = GEO[usize::from(self.city / 9) % 6];
        countries[usize::from((self.city / 3) % 3)].0
    }

    #[must_use]
    pub fn city_name(&self) -> &'static str {
        let (_, countries) = GEO[usize::from(self.city / 9) % 6];
        countries[usize::from((self.city / 3) % 3)].1[usize::from(self.city % 3)]
    }

    #[must_use]
    pub fn category(&self) -> &'static str {
        CATEGORIES[usize::from(self.subcategory / 4) % 3]
    }
}

// ==== Generation ====

/// splitmix64: one step of the stream.
fn next(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// A number below `n` from the stream.
fn below(state: &mut u64, n: u64) -> u64 {
    next(state) % n.max(1)
}

/// The first order date: 2019-01-01, days since 1970-01-01.
pub const FIRST_DAY: i32 = 17_897;
/// Six years of orders (2019-01-01 .. 2024-12-31).
pub const DAYS: i32 = 2_192;

/// Order `row`, the same in every run.
#[must_use]
#[allow(clippy::cast_possible_truncation)] // every draw is below its table's size
pub fn order(row: u32) -> Order {
    let mut s = u64::from(row).wrapping_mul(0xD1B5_4A32_D192_ED03) ^ 0xA2_0D45_4B0A;
    let subcategory = below(&mut s, 12) as u8;
    let price_range: (u64, u64) = match subcategory / 4 {
        0 => (4_900, 149_900),
        1 => (99, 7_900),
        _ => (1_900, 249_900),
    };
    let unit_price_cents = (price_range.0 + below(&mut s, price_range.1 - price_range.0)) as u32;
    let ship_mode = match below(&mut s, 100) {
        0..=59 => 0,
        60..=79 => 1,
        80..=94 => 2,
        _ => 3,
    };
    let days_to_ship = match ship_mode {
        3 => 0,
        2 => 1 + below(&mut s, 2) as u8,
        1 => 2 + below(&mut s, 3) as u8,
        _ => 3 + below(&mut s, 5) as u8,
    };
    let quantity = 1 + below(&mut s, 20) as u16;
    let discount = [0u8, 0, 0, 5, 10, 15, 20, 25, 30][below(&mut s, 9) as usize];
    let rating = match below(&mut s, 100) {
        0..=4 => 1,
        5..=14 => 2,
        15..=39 => 3,
        40..=74 => 4,
        _ => 5,
    };
    Order {
        date: FIRST_DAY + below(&mut s, DAYS as u64) as i32,
        unit_price_cents,
        customer: below(&mut s, u64::from(CUSTOMERS)) as u16,
        product: below(&mut s, 10_000) as u16,
        quantity,
        shipping_cents: (200 + below(&mut s, 5_800) + u64::from(quantity) * 35) as u16,
        cost_permille: (550 + below(&mut s, 350)) as u16,
        city: below(&mut s, u64::from(CITIES)) as u8,
        segment: below(&mut s, 4) as u8,
        subcategory,
        ship_mode,
        priority: below(&mut s, 4) as u8,
        discount,
        days_to_ship,
        returned: below(&mut s, 100) < 6,
        rating,
        rep: below(&mut s, 20) as u8,
    }
}

/// Every order of the dashboard, plus what the user edited.
#[derive(Debug, Clone, Default)]
pub struct DataSet {
    pub orders: Vec<Order>,
    /// Edits so far (the status bar counts them).
    pub edits: u32,
}

impl DataSet {
    /// Orders `0..rows`.
    #[must_use]
    pub fn generate(rows: u32) -> Self {
        Self {
            orders: (0..rows).map(order).collect(),
            edits: 0,
        }
    }

    /// The number of orders.
    #[must_use]
    pub fn rows(&self) -> u32 {
        u32::try_from(self.orders.len()).unwrap_or(u32::MAX)
    }

    /// The text of column `column` of order `row` (empty past the end).
    #[must_use]
    pub fn text(&self, row: u32, column: usize) -> String {
        let Some(o) = self.orders.get(row as usize) else {
            return String::new();
        };
        match column {
            c::ORDER => format!("SO-{:07}", u64::from(row) + 1),
            c::ORDER_DATE => format_day(o.date),
            c::SHIP_DATE => format_day(o.ship_date()),
            c::REGION => o.region().to_string(),
            c::COUNTRY => o.country().to_string(),
            c::CITY => o.city_name().to_string(),
            c::CUSTOMER => format!(
                "{} {}",
                FIRST_NAMES[usize::from(o.customer / 24) % 24],
                LAST_NAMES[usize::from(o.customer % 24)]
            ),
            c::SEGMENT => SEGMENTS[usize::from(o.segment) % 4].to_string(),
            c::CATEGORY => o.category().to_string(),
            c::SUBCATEGORY => SUBCATEGORIES[usize::from(o.subcategory) % 12].to_string(),
            c::PRODUCT => format!("{}-{:04}", PRODUCT_PREFIX[usize::from(o.subcategory) % 12], o.product),
            c::SHIP_MODE => SHIP_MODES[usize::from(o.ship_mode) % 4].to_string(),
            c::PRIORITY => PRIORITIES[usize::from(o.priority) % 4].to_string(),
            c::QUANTITY => o.quantity.to_string(),
            c::UNIT_PRICE => money(i64::from(o.unit_price_cents)),
            c::DISCOUNT => o.discount.to_string(),
            c::SALES => money(o.sales_cents()),
            c::COST => money(o.cost_cents()),
            c::SHIPPING => money(i64::from(o.shipping_cents)),
            c::PROFIT => money(o.profit_cents()),
            c::MARGIN => format!("{:.1}", o.margin_percent()),
            c::DAYS_TO_SHIP => o.days_to_ship.to_string(),
            c::RETURNED => YES_NO[usize::from(o.returned)].to_string(),
            c::RATING => o.rating.to_string(),
            c::SALES_REP => SALES_REPS[usize::from(o.rep) % 20].to_string(),
            _ => String::new(),
        }
    }

    /// The text of a CATEGORY column of order `row`, borrowed (the charts group
    /// by it without building a String per row): region, country, city,
    /// segment, category, subcategory, ship mode, priority, returned, sales
    /// rep. Empty for any other column or past the end.
    #[must_use]
    pub fn category_text(&self, row: u32, column: usize) -> &'static str {
        let Some(o) = self.orders.get(row as usize) else {
            return "";
        };
        match column {
            c::REGION => o.region(),
            c::COUNTRY => o.country(),
            c::CITY => o.city_name(),
            c::SEGMENT => SEGMENTS[usize::from(o.segment) % 4],
            c::CATEGORY => o.category(),
            c::SUBCATEGORY => SUBCATEGORIES[usize::from(o.subcategory) % 12],
            c::SHIP_MODE => SHIP_MODES[usize::from(o.ship_mode) % 4],
            c::PRIORITY => PRIORITIES[usize::from(o.priority) % 4],
            c::RETURNED => YES_NO[usize::from(o.returned)],
            c::SALES_REP => SALES_REPS[usize::from(o.rep) % 20],
            _ => "",
        }
    }

    /// The number column `column` of order `row` sorts and range-filters
    /// by: money in units (not cents), dates as days since 1970-01-01. NaN
    /// for a text column (the table sorts those by their text).
    #[must_use]
    pub fn value(&self, row: u32, column: usize) -> f64 {
        let Some(o) = self.orders.get(row as usize) else {
            return f64::NAN;
        };
        match column {
            c::ORDER_DATE => f64::from(o.date),
            c::SHIP_DATE => f64::from(o.ship_date()),
            c::QUANTITY => f64::from(o.quantity),
            c::UNIT_PRICE => f64::from(o.unit_price_cents) / 100.0,
            c::DISCOUNT => f64::from(o.discount),
            c::SALES => o.sales_cents() as f64 / 100.0,
            c::COST => o.cost_cents() as f64 / 100.0,
            c::SHIPPING => f64::from(o.shipping_cents) / 100.0,
            c::PROFIT => o.profit_cents() as f64 / 100.0,
            c::MARGIN => o.margin_percent(),
            c::DAYS_TO_SHIP => f64::from(o.days_to_ship),
            c::RATING => f64::from(o.rating),
            _ => f64::NAN,
        }
    }

    /// Sets column `column` of order `row` from what the user typed, or
    /// says why not. The texts and numbers derived from it (sales, profit,
    /// the ship date) follow at once.
    pub fn edit(&mut self, row: u32, column: usize, typed: &str) -> Result<(), String> {
        let t = typed.trim();
        let Some(o) = self.orders.get_mut(row as usize) else {
            return Err("There is no such order.".to_string());
        };
        match column {
            c::ORDER_DATE => {
                o.date = parse_day(t).ok_or_else(|| format!("\"{t}\" is not a date (YYYY-MM-DD)."))?;
            }
            c::PRIORITY => {
                let i = PRIORITIES
                    .iter()
                    .position(|p| p.eq_ignore_ascii_case(t))
                    .ok_or_else(|| format!("The priority is one of {}.", PRIORITIES.join(", ")))?;
                o.priority = u8::try_from(i).unwrap_or(0);
            }
            c::QUANTITY => {
                o.quantity = t
                    .parse::<u16>()
                    .ok()
                    .filter(|q| (1..=999).contains(q))
                    .ok_or_else(|| "The quantity is a whole number from 1 to 999.".to_string())?;
            }
            c::UNIT_PRICE => {
                o.unit_price_cents = parse_cents(t)
                    .filter(|c| (1..=9_999_999).contains(c))
                    .ok_or_else(|| "The unit price is an amount from 0.01 to 99,999.99.".to_string())?;
            }
            c::DISCOUNT => {
                o.discount = t
                    .trim_end_matches('%')
                    .trim()
                    .parse::<u8>()
                    .ok()
                    .filter(|d| *d <= 90)
                    .ok_or_else(|| "The discount is a whole percentage from 0 to 90.".to_string())?;
            }
            c::RETURNED => {
                o.returned = match t.to_ascii_lowercase().as_str() {
                    "yes" | "y" => true,
                    "no" | "n" => false,
                    _ => return Err("Returned is Yes or No.".to_string()),
                };
            }
            c::RATING => {
                o.rating = t
                    .parse::<u8>()
                    .ok()
                    .filter(|r| (1..=5).contains(r))
                    .ok_or_else(|| "The rating is 1, 2, 3, 4 or 5.".to_string())?;
            }
            _ => return Err(format!("{} is not editable.", COLUMNS.get(column).map_or("This column", |c| c.title))),
        }
        self.edits += 1;
        Ok(())
    }
}

// ==== Formats ====

/// 1970-01-01, as chrono counts the days of the common era.
const EPOCH_FROM_CE: i32 = 719_163;

/// `YYYY-MM-DD` of a day number (days since 1970-01-01).
#[must_use]
pub fn format_day(day: i32) -> String {
    match NaiveDate::from_num_days_from_ce_opt(day + EPOCH_FROM_CE) {
        Some(d) => format!("{:04}-{:02}-{:02}", d.year(), d.month(), d.day()),
        None => String::new(),
    }
}

/// The day number of a `YYYY-MM-DD` text (a real date), else `None`.
#[must_use]
pub fn parse_day(text: &str) -> Option<i32> {
    let mut parts = text.trim().split('-');
    let y: i32 = parts.next()?.trim().parse().ok()?;
    let m: u32 = parts.next()?.trim().parse().ok()?;
    let d: u32 = parts.next()?.trim().parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    let date = NaiveDate::from_ymd_opt(y, m, d)?;
    Some(date.num_days_from_ce() - EPOCH_FROM_CE)
}

/// An amount in cents with thousands separators: `1,234.50`, `-12.00`.
#[must_use]
pub fn money(cents: i64) -> String {
    let sign = if cents < 0 { "-" } else { "" };
    let abs = cents.unsigned_abs();
    let units = abs / 100;
    let digits = units.to_string();
    let mut grouped = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(ch);
    }
    format!("{sign}{grouped}.{:02}", abs % 100)
}

/// The cents of a typed amount (`1234.5`, `1,234.50`, `12`), else `None`.
#[must_use]
pub fn parse_cents(text: &str) -> Option<u32> {
    let t: String = text.trim().chars().filter(|c| *c != ',').collect();
    let (whole, frac) = match t.split_once('.') {
        Some((w, f)) => (w, f),
        None => (t.as_str(), ""),
    };
    if whole.is_empty() && frac.is_empty() {
        return None;
    }
    if frac.len() > 2 || !whole.chars().all(|c| c.is_ascii_digit()) || !frac.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let units: u32 = if whole.is_empty() { 0 } else { whole.parse().ok()? };
    let mut cents: u32 = if frac.is_empty() { 0 } else { frac.parse().ok()? };
    if frac.len() == 1 {
        cents *= 10;
    }
    units.checked_mul(100)?.checked_add(cents)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_orders_are_the_same_in_every_run() {
        let a = DataSet::generate(1_000);
        let b = DataSet::generate(1_000);
        assert_eq!(a.orders, b.orders);
        assert_eq!(order(123_456), order(123_456));
        assert_ne!(order(1), order(2));
    }

    #[test]
    fn every_order_stays_inside_its_tables_and_its_six_years() {
        for row in 0..20_000 {
            let o = order(row);
            assert!(o.city < CITIES);
            assert!(o.customer < CUSTOMERS);
            assert!(o.subcategory < 12);
            assert!((1..=5).contains(&o.rating));
            assert!((1..=20).contains(&o.quantity));
            assert!(o.date >= FIRST_DAY && o.date < FIRST_DAY + DAYS);
        }
    }

    #[test]
    fn an_order_is_small() {
        assert!(std::mem::size_of::<Order>() <= 28, "{}", std::mem::size_of::<Order>());
    }

    #[test]
    fn the_first_day_is_new_years_day_2019() {
        assert_eq!(format_day(FIRST_DAY), "2019-01-01");
        assert_eq!(parse_day("2019-01-01"), Some(FIRST_DAY));
        assert_eq!(format_day(FIRST_DAY + DAYS - 1), "2024-12-31");
        assert_eq!(parse_day("2024-02-30"), None, "not a real date");
        assert_eq!(parse_day("2024-2"), None);
    }

    #[test]
    fn texts_and_values_of_one_order_agree() {
        let d = DataSet::generate(10);
        assert_eq!(d.text(0, c::ORDER), "SO-0000001");
        assert_eq!(d.text(9, c::ORDER), "SO-0000010");
        let o = d.orders[3];
        assert_eq!(d.text(3, c::ORDER_DATE), format_day(o.date));
        assert_eq!(d.value(3, c::ORDER_DATE), f64::from(o.date));
        assert_eq!(d.text(3, c::QUANTITY), o.quantity.to_string());
        assert!((d.value(3, c::SALES) * 100.0 - o.sales_cents() as f64).abs() < 0.5);
        assert!(d.value(3, c::REGION).is_nan(), "a text column has no number");
        assert_eq!(d.text(10, c::ORDER), "", "past the end");
    }

    #[test]
    fn money_has_thousands_separators_and_two_decimals() {
        assert_eq!(money(0), "0.00");
        assert_eq!(money(5), "0.05");
        assert_eq!(money(123_456), "1,234.56");
        assert_eq!(money(100_000_000), "1,000,000.00");
        assert_eq!(money(-1_250), "-12.50");
        assert_eq!(parse_cents("1,234.5"), Some(123_450));
        assert_eq!(parse_cents("12"), Some(1_200));
        assert_eq!(parse_cents(".5"), Some(50));
        assert_eq!(parse_cents("1.234"), None);
        assert_eq!(parse_cents("abc"), None);
    }

    #[test]
    fn an_edit_is_validated_and_the_derived_columns_follow() {
        let mut d = DataSet::generate(5);
        assert!(d.edit(0, c::QUANTITY, "0").is_err());
        assert!(d.edit(0, c::QUANTITY, "1000").is_err());
        assert!(d.edit(0, c::QUANTITY, "seven").is_err());
        d.edit(0, c::QUANTITY, "10").expect("10 is a quantity");
        d.edit(0, c::UNIT_PRICE, "2.50").expect("an amount");
        d.edit(0, c::DISCOUNT, "0").expect("no discount");
        assert_eq!(d.text(0, c::SALES), "25.00");
        assert!(d.edit(0, c::PRIORITY, "urgent").is_err());
        d.edit(0, c::PRIORITY, "high").expect("case does not matter");
        assert_eq!(d.text(0, c::PRIORITY), "High");
        d.edit(0, c::ORDER_DATE, "2020-02-29").expect("a leap day");
        assert_eq!(d.text(0, c::ORDER_DATE), "2020-02-29");
        assert!(d.edit(0, c::ORDER_DATE, "2021-02-29").is_err());
        assert!(d.edit(0, c::SALES, "1").is_err(), "a derived column is not editable");
        assert!(d.edit(0, c::RATING, "6").is_err());
        d.edit(0, c::RETURNED, "Yes").expect("yes");
        assert_eq!(d.text(0, c::RETURNED), "Yes");
        assert_eq!(d.edits, 6);
    }
}
