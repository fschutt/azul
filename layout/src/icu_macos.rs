//! macOS Foundation-based ICU backend for azul.
//!
//! Replaces ICU4X (which bundles ~3.3 MB of locale data blobs) with
//! `NSNumberFormatter`, `NSDateFormatter`, `NSListFormatter`, and
//! `NSString.localizedCompare:` from the system Foundation framework.
//!
//! Plural rules use a compact CLDR lookup table (~2 KB) instead of
//! the ICU segmenter dictionaries.

use alloc::{string::String, vec::Vec};
use core::cmp::Ordering;

use azul_css::AzString;
use objc2::rc::Retained;
use objc2_foundation::{
    NSArray, NSCalendar, NSCalendarIdentifierGregorian, NSComparisonResult, NSDate,
    NSDateComponents, NSDateFormatter, NSDateFormatterStyle, NSListFormatter, NSLocale, NSNumber,
    NSNumberFormatter, NSNumberFormatterStyle, NSString,
};

use super::{FormatLength, IcuDate, IcuDateTime, IcuResult, IcuTime, ListType, PluralCategory};

// ─── CLDR plural rules ───────────────────────────────────────────────────────
//
// Covers the major plural-rule groups defined in CLDR without bundling any
// data file.  Languages not explicitly listed fall back to English rules.

fn plural_for(n: i64, lang: &str) -> PluralCategory {
    let lang = lang.split(['-', '_']).next().unwrap_or(lang);
    match lang {
        // Arabic: six categories
        "ar" | "arz" | "ckb" => {
            let n100 = n.abs() % 100;
            if n == 0 {
                PluralCategory::Zero
            } else if n == 1 {
                PluralCategory::One
            } else if n == 2 {
                PluralCategory::Two
            } else if (3..=10).contains(&n100) {
                PluralCategory::Few
            } else if (11..=99).contains(&n100) {
                PluralCategory::Many
            } else {
                PluralCategory::Other
            }
        }
        // Welsh: six categories
        "cy" => match n {
            0 => PluralCategory::Zero,
            1 => PluralCategory::One,
            2 => PluralCategory::Two,
            3 => PluralCategory::Few,
            6 => PluralCategory::Many,
            _ => PluralCategory::Other,
        },
        // East Slavic (Russian, Ukrainian, Belarusian, Serbian, Croatian, Bosnian)
        "ru" | "uk" | "be" | "sr" | "hr" | "bs" | "sh" => {
            let n10 = n.abs() % 10;
            let n100 = n.abs() % 100;
            if n10 == 1 && n100 != 11 {
                PluralCategory::One
            } else if (2..=4).contains(&n10) && !(12..=14).contains(&n100) {
                PluralCategory::Few
            } else {
                PluralCategory::Many
            }
        }
        // Polish
        "pl" => {
            let n10 = n.abs() % 10;
            let n100 = n.abs() % 100;
            if n == 1 {
                PluralCategory::One
            } else if (2..=4).contains(&n10) && !(12..=14).contains(&n100) {
                PluralCategory::Few
            } else {
                PluralCategory::Many
            }
        }
        // Czech, Slovak
        "cs" | "sk" => {
            if n == 1 {
                PluralCategory::One
            } else if (2..=4).contains(&n) {
                PluralCategory::Few
            } else {
                PluralCategory::Other
            }
        }
        // Slovenian
        "sl" => {
            let n100 = n.abs() % 100;
            if n100 == 1 {
                PluralCategory::One
            } else if n100 == 2 {
                PluralCategory::Two
            } else if (3..=4).contains(&n100) {
                PluralCategory::Few
            } else {
                PluralCategory::Other
            }
        }
        // Lithuanian
        "lt" => {
            let n10 = n.abs() % 10;
            let n100 = n.abs() % 100;
            if n10 == 1 && !(11..=19).contains(&n100) {
                PluralCategory::One
            } else if (2..=9).contains(&n10) && !(11..=19).contains(&n100) {
                PluralCategory::Few
            } else {
                PluralCategory::Other
            }
        }
        // Latvian
        "lv" => {
            let n10 = n.abs() % 10;
            let n100 = n.abs() % 100;
            if n == 0 {
                PluralCategory::Zero
            } else if n10 == 1 && n100 != 11 {
                PluralCategory::One
            } else {
                PluralCategory::Other
            }
        }
        // Romanian
        "ro" | "mo" => {
            let n100 = n.abs() % 100;
            if n == 1 {
                PluralCategory::One
            } else if n == 0 || (1..=19).contains(&n100) {
                PluralCategory::Few
            } else {
                PluralCategory::Other
            }
        }
        // Maltese
        "mt" => {
            let n100 = n.abs() % 100;
            if n == 1 {
                PluralCategory::One
            } else if n == 0 || (2..=10).contains(&n100) {
                PluralCategory::Few
            } else if (11..=19).contains(&n100) {
                PluralCategory::Many
            } else {
                PluralCategory::Other
            }
        }
        // Hebrew / Yiddish
        "he" | "yi" | "iw" => {
            if n == 1 {
                PluralCategory::One
            } else if n == 2 {
                PluralCategory::Two
            } else if n != 0 && n % 10 == 0 {
                PluralCategory::Many
            } else {
                PluralCategory::Other
            }
        }
        // Irish (Gaelic)
        "ga" => match n {
            1 => PluralCategory::One,
            2 => PluralCategory::Two,
            3..=6 => PluralCategory::Few,
            7..=10 => PluralCategory::Many,
            _ => PluralCategory::Other,
        },
        // French, Kabyle: 0 and 1 are "one"
        "fr" | "ff" | "kab" => {
            if n <= 1 {
                PluralCategory::One
            } else {
                PluralCategory::Other
            }
        }
        // Default: English-style (exactly 1 → one, everything else → other)
        _ => {
            if n == 1 {
                PluralCategory::One
            } else {
                PluralCategory::Other
            }
        }
    }
}

// ─── IcuLocalizer ─────────────────────────────────────────────────────────────

/// macOS Foundation-based locale formatter.
///
/// Delegates number, date/time, and list formatting to `NSFormatter` classes
/// that ship with the OS.  Plural rules use a compact CLDR lookup table —
/// no ICU data blobs are linked.
#[derive(Debug)]
pub struct IcuLocalizer {
    locale_string: AzString,
}

impl IcuLocalizer {
    pub fn new(locale_str: &str) -> Self {
        Self { locale_string: AzString::from(locale_str) }
    }

    pub fn from_system_language(system_language: &AzString) -> Self {
        Self::new(system_language.as_str())
    }

    pub fn get_locale(&self) -> AzString {
        self.locale_string.clone()
    }

    pub fn get_language(&self) -> AzString {
        let lang = self.locale_string.as_str()
            .split(['-', '_'])
            .next()
            .unwrap_or(self.locale_string.as_str());
        AzString::from(lang)
    }

    pub fn get_region(&self) -> Option<AzString> {
        self.locale_string.as_str().split('-').nth(1).map(AzString::from)
    }

    pub fn set_locale(&mut self, locale_str: &str) -> bool {
        self.locale_string = AzString::from(locale_str);
        true
    }

    pub fn load_data_blob(&mut self, _data: Vec<u8>) {
        // no-op: Foundation always uses system locale data
    }

    fn make_ns_locale(&self) -> Retained<NSLocale> {
        unsafe {
            let ident = NSString::from_str(self.locale_string.as_str());
            NSLocale::localeWithLocaleIdentifier(&ident)
        }
    }

    // ── Number formatting ───────────────────────────────────────────────────

    pub fn format_integer(&mut self, value: i64) -> AzString {
        unsafe {
            let fmt = NSNumberFormatter::new();
            fmt.setNumberStyle(NSNumberFormatterStyle::DecimalStyle);
            fmt.setLocale(Some(&self.make_ns_locale()));
            let n = NSNumber::new_i64(value);
            fmt.stringFromNumber(&n)
                .map(|s| AzString::from(s.to_string()))
                .unwrap_or_else(|| AzString::from(value.to_string()))
        }
    }

    pub fn format_decimal(&mut self, integer_part: i64, decimal_places: i16) -> AzString {
        let dp = decimal_places.max(0) as usize;
        let value = integer_part as f64 * 10f64.powi(-(decimal_places as i32));
        unsafe {
            let fmt = NSNumberFormatter::new();
            fmt.setNumberStyle(NSNumberFormatterStyle::DecimalStyle);
            fmt.setLocale(Some(&self.make_ns_locale()));
            fmt.setMinimumFractionDigits(dp);
            fmt.setMaximumFractionDigits(dp);
            let n = NSNumber::new_f64(value);
            fmt.stringFromNumber(&n)
                .map(|s| AzString::from(s.to_string()))
                .unwrap_or_else(|| AzString::from(format!("{value:.dp$}")))
        }
    }

    // ── Plural rules ────────────────────────────────────────────────────────

    pub fn get_plural_category(&mut self, value: i64) -> PluralCategory {
        let lang = self.locale_string.as_str()
            .split(['-', '_'])
            .next()
            .unwrap_or("en");
        plural_for(value, lang)
    }

    pub fn pluralize(
        &mut self,
        value: i64,
        zero: &str,
        one: &str,
        two: &str,
        few: &str,
        many: &str,
        other: &str,
    ) -> AzString {
        let template = match self.get_plural_category(value) {
            PluralCategory::Zero => zero,
            PluralCategory::One => one,
            PluralCategory::Two => two,
            PluralCategory::Few => few,
            PluralCategory::Many => many,
            PluralCategory::Other => other,
        };
        AzString::from(template.replace("{}", &value.to_string()))
    }

    // ── List formatting ─────────────────────────────────────────────────────

    pub fn format_list(&mut self, items: &[AzString], list_type: ListType) -> AzString {
        if let ListType::Unit = list_type {
            let strs: Vec<&str> = items.iter().map(|s| s.as_str()).collect();
            return AzString::from(strs.join(", "));
        }
        unsafe {
            let ns_strings: Vec<Retained<NSString>> =
                items.iter().map(|s| NSString::from_str(s.as_str())).collect();
            let refs: Vec<&NSString> = ns_strings.iter().map(|s| s.as_ref()).collect();
            let array = NSArray::from_slice(&refs);
            // NSListFormatter::localizedStringByJoiningStrings is a class method
            // that uses the user's current locale — exactly what we want on macOS.
            let result = NSListFormatter::localizedStringByJoiningStrings(&array);
            AzString::from(result.to_string())
        }
    }

    // ── Date / time formatting ──────────────────────────────────────────────

    pub fn format_date(&mut self, date: IcuDate, length: FormatLength) -> IcuResult {
        unsafe {
            match make_ns_date(date.year, date.month as isize, date.day as isize) {
                None => IcuResult::err("Invalid date"),
                Some(ns_date) => {
                    let fmt = NSDateFormatter::new();
                    fmt.setDateStyle(ns_date_style(length));
                    fmt.setTimeStyle(NSDateFormatterStyle::NoStyle);
                    fmt.setLocale(Some(&self.make_ns_locale()));
                    IcuResult::ok(fmt.stringFromDate(&ns_date).to_string())
                }
            }
        }
    }

    pub fn format_time(&mut self, time: IcuTime, include_seconds: bool) -> IcuResult {
        let style = if include_seconds {
            NSDateFormatterStyle::MediumStyle // HH:MM:SS
        } else {
            NSDateFormatterStyle::ShortStyle // HH:MM
        };
        unsafe {
            match make_ns_time(time.hour as isize, time.minute as isize, time.second as isize) {
                None => IcuResult::err("Invalid time"),
                Some(ns_date) => {
                    let fmt = NSDateFormatter::new();
                    fmt.setDateStyle(NSDateFormatterStyle::NoStyle);
                    fmt.setTimeStyle(style);
                    fmt.setLocale(Some(&self.make_ns_locale()));
                    IcuResult::ok(fmt.stringFromDate(&ns_date).to_string())
                }
            }
        }
    }

    pub fn format_datetime(&mut self, datetime: IcuDateTime, length: FormatLength) -> IcuResult {
        unsafe {
            match make_ns_datetime(
                datetime.date.year,
                datetime.date.month as isize,
                datetime.date.day as isize,
                datetime.time.hour as isize,
                datetime.time.minute as isize,
                datetime.time.second as isize,
            ) {
                None => IcuResult::err("Invalid datetime"),
                Some(ns_date) => {
                    let fmt = NSDateFormatter::new();
                    fmt.setDateStyle(ns_date_style(length));
                    fmt.setTimeStyle(NSDateFormatterStyle::ShortStyle);
                    fmt.setLocale(Some(&self.make_ns_locale()));
                    IcuResult::ok(fmt.stringFromDate(&ns_date).to_string())
                }
            }
        }
    }

    // ── Collation ───────────────────────────────────────────────────────────

    pub fn compare(&mut self, a: &str, b: &str) -> Ordering {
        unsafe {
            let a_ns = NSString::from_str(a);
            let b_ns = NSString::from_str(b);
            Ordering::from(a_ns.localizedCompare(&b_ns))
        }
    }

    pub fn sort_strings(&mut self, strings: &mut [AzString]) {
        strings.sort_by(|a, b| unsafe {
            let a_ns = NSString::from_str(a.as_str());
            let b_ns = NSString::from_str(b.as_str());
            Ordering::from(a_ns.localizedCompare(&b_ns))
        });
    }

    pub fn sorted_strings(&mut self, strings: &[AzString]) -> Vec<AzString> {
        let mut v = strings.to_vec();
        self.sort_strings(&mut v);
        v
    }

    pub fn strings_equal(&mut self, a: &str, b: &str) -> bool {
        self.compare(a, b) == Ordering::Equal
    }

    pub fn get_sort_key(&mut self, s: &str) -> Vec<u8> {
        // Foundation doesn't expose raw collation sort keys; return UTF-8 bytes
        // as a proxy (sufficient for identity / cache-key use cases).
        s.as_bytes().to_vec()
    }
}

impl Default for IcuLocalizer {
    fn default() -> Self {
        Self::new("en-US")
    }
}

impl Clone for IcuLocalizer {
    fn clone(&self) -> Self {
        Self { locale_string: self.locale_string.clone() }
    }
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn ns_date_style(length: FormatLength) -> NSDateFormatterStyle {
    match length {
        FormatLength::Short => NSDateFormatterStyle::ShortStyle,
        FormatLength::Medium => NSDateFormatterStyle::MediumStyle,
        FormatLength::Long => NSDateFormatterStyle::LongStyle,
    }
}

unsafe fn gregorian() -> Option<Retained<NSCalendar>> {
    NSCalendar::calendarWithIdentifier(NSCalendarIdentifierGregorian)
}

unsafe fn make_ns_date(year: i32, month: isize, day: isize) -> Option<Retained<NSDate>> {
    let cal = gregorian()?;
    let c = NSDateComponents::new();
    c.setYear(year as isize);
    c.setMonth(month);
    c.setDay(day);
    cal.dateFromComponents(&c)
}

unsafe fn make_ns_time(hour: isize, minute: isize, second: isize) -> Option<Retained<NSDate>> {
    let cal = gregorian()?;
    let c = NSDateComponents::new();
    c.setHour(hour);
    c.setMinute(minute);
    c.setSecond(second);
    cal.dateFromComponents(&c)
}

unsafe fn make_ns_datetime(
    year: i32,
    month: isize,
    day: isize,
    hour: isize,
    minute: isize,
    second: isize,
) -> Option<Retained<NSDate>> {
    let cal = gregorian()?;
    let c = NSDateComponents::new();
    c.setYear(year as isize);
    c.setMonth(month);
    c.setDay(day);
    c.setHour(hour);
    c.setMinute(minute);
    c.setSecond(second);
    cal.dateFromComponents(&c)
}
