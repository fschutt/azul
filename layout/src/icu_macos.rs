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
use core::ops::Deref;

use azul_css::AzString;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_foundation::{
    NSArray, NSCalendar, NSCalendarIdentifierGregorian, NSDate,
    NSDateComponents, NSDateFormatter, NSDateFormatterStyle, NSListFormatter, NSLocale, NSNumber,
    NSNumberFormatter, NSNumberFormatterStyle, NSRange, NSString, NSStringCompareOptions,
};

use super::{FormatLength, IcuDate, IcuDateTime, IcuResult, IcuTime, ListType, PluralCategory, decimal_string, plural_for};

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
        self.locale_string.as_str().split(['-', '_']).nth(1).map(AzString::from)
    }

    pub fn set_locale(&mut self, locale_str: &str) -> bool {
        self.locale_string = AzString::from(locale_str);
        true
    }

    /// No-op on macOS: Foundation always uses system-provided locale data,
    /// so externally loaded ICU data blobs are not needed and are silently ignored.
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
        let value_str = decimal_string(integer_part, decimal_places);
        let dp = decimal_places.max(0) as usize;
        // NSNumber only accepts f64, so large i64 values (>2^53) may still
        // lose precision in the formatted output. The fallback path is exact.
        let value: f64 = value_str.parse().unwrap_or(0.0);
        unsafe {
            let fmt = NSNumberFormatter::new();
            fmt.setNumberStyle(NSNumberFormatterStyle::DecimalStyle);
            fmt.setLocale(Some(&self.make_ns_locale()));
            fmt.setMinimumFractionDigits(dp);
            fmt.setMaximumFractionDigits(dp);
            let n = NSNumber::new_f64(value);
            fmt.stringFromNumber(&n)
                .map(|s| AzString::from(s.to_string()))
                .unwrap_or_else(|| AzString::from(value_str))
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
            let any_refs: Vec<&AnyObject> =
                ns_strings.iter().map(|s| s.deref().deref().deref()).collect();
            let array = NSArray::<AnyObject>::from_slice(&any_refs);
            let formatter = NSListFormatter::new();
            formatter.setLocale(Some(&self.make_ns_locale()));
            match formatter.stringFromItems(&array) {
                Some(result) => AzString::from(result.to_string()),
                None => {
                    let str_refs: Vec<&NSString> = ns_strings.iter().map(|s| s.as_ref()).collect();
                    let str_array = NSArray::from_slice(&str_refs);
                    let result = NSListFormatter::localizedStringByJoiningStrings(&str_array);
                    AzString::from(result.to_string())
                }
            }
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
            let range = NSRange::new(0, a_ns.len());
            let locale = self.make_ns_locale();
            let result = a_ns.compare_options_range_locale(
                &b_ns,
                NSStringCompareOptions(0),
                range,
                Some(locale.deref().deref()),
            );
            Ordering::from(result)
        }
    }

    pub fn sort_strings(&mut self, strings: &mut [AzString]) {
        strings.sort_by(|a, b| self.compare(a.as_str(), b.as_str()));
    }

    pub fn sorted_strings(&mut self, strings: &[AzString]) -> Vec<AzString> {
        let mut v = strings.to_vec();
        self.sort_strings(&mut v);
        v
    }

    pub fn strings_equal(&mut self, a: &str, b: &str) -> bool {
        self.compare(a, b) == Ordering::Equal
    }

    /// Returns raw UTF-8 bytes of the string as an identity key.
    ///
    /// **Note:** Foundation does not expose raw collation sort keys, so this
    /// does *not* produce locale-aware ordering.  The result is suitable for
    /// identity / cache-key use cases only — it will not sort the same way
    /// as [`compare`](Self::compare).
    pub fn get_sort_key(&mut self, s: &str) -> Vec<u8> {
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
    // Set a known-good date so dateFromComponents doesn't fail
    // when only time components are provided.
    c.setYear(2000);
    c.setMonth(1);
    c.setDay(1);
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

// ─── Generated adversarial tests ──────────────────────────────────────────────

#[cfg(test)]
mod autotest_generated {
    #![allow(clippy::all, clippy::pedantic, clippy::nursery)]

    use super::*;

    // ── helpers ─────────────────────────────────────────────────────────────

    /// Locale whose Foundation output uses ASCII digits and `,` grouping / `.`
    /// decimal separator, so digit-extraction assertions below are stable.
    fn en() -> IcuLocalizer {
        IcuLocalizer::new("en-US")
    }

    /// Strip every non-ASCII-digit char, leaving just the digits. Robust against
    /// whichever grouping / minus / separator glyphs Foundation chooses.
    fn digits_only(s: &str) -> String {
        s.chars().filter(char::is_ascii_digit).collect()
    }

    /// `|value|` as decimal digits, computed via `i128` so that `i64::MIN` does
    /// not blow up the *test* itself (`i64::MIN.abs()` overflows).
    fn abs_digits(value: i64) -> String {
        (value as i128).unsigned_abs().to_string()
    }

    /// Foundation may emit ASCII hyphen-minus or U+2212 MINUS SIGN.
    fn has_minus(s: &str) -> bool {
        s.contains('-') || s.contains('\u{2212}')
    }

    fn is_ok(r: &IcuResult) -> bool {
        matches!(*r, IcuResult::Ok(_))
    }

    fn unwrap_ok(r: IcuResult) -> String {
        match r {
            IcuResult::Ok(s) => s.as_str().to_string(),
            IcuResult::Err(e) => panic!("expected Ok, got Err({})", e.message.as_str()),
        }
    }

    /// Whichever variant comes back, the payload must never be an empty string:
    /// an `Ok("")` is a silently-broken format, an `Err("")` is a useless error.
    fn assert_payload_non_empty(r: &IcuResult) {
        match *r {
            IcuResult::Ok(ref s) => assert!(!s.as_str().is_empty(), "Ok variant carried an empty string"),
            IcuResult::Err(ref e) => {
                assert!(!e.message.as_str().is_empty(), "Err variant carried an empty message");
            }
        }
    }

    fn az(items: &[&str]) -> Vec<AzString> {
        items.iter().map(|s| AzString::from(*s)).collect()
    }

    fn as_strs(items: &[AzString]) -> Vec<&str> {
        items.iter().map(AzString::as_str).collect()
    }

    /// Nasty-but-legal inputs: empty, whitespace, interior NUL, control chars,
    /// combining marks, astral-plane emoji, RTL, and a long string.
    fn hostile_strings() -> Vec<String> {
        vec![
            String::new(),
            " ".to_string(),
            "\t\r\n".to_string(),
            "a\0b".to_string(),
            "\u{7f}\u{1}".to_string(),
            "e\u{301}".to_string(),      // e + combining acute
            "é".to_string(),             // precomposed — canonically equivalent to the above
            "😀👨‍👩‍👧‍👦".to_string(), // astral plane + ZWJ sequence
            "مرحبا".to_string(),         // RTL
            "日本語".to_string(),
            "\u{FEFF}bom".to_string(),
            "ß".to_string(),
            "a".repeat(100_000),
        ]
    }

    // ── constructors: IcuLocalizer::new / from_system_language ───────────────

    #[test]
    fn new_roundtrips_the_locale_string_verbatim() {
        // `new` performs no validation or normalisation, so `get_locale` must
        // hand back exactly what went in — including nonsense.
        let cases = [
            "en-US",
            "",
            " ",
            "not a locale at all",
            "en_US",
            "EN-us",
            "zh-Hans-CN",
            "x",
            "-",
            "--",
            "_",
            "日本語",
            "a\0b",
            "\u{FEFF}",
        ];
        for c in cases {
            let loc = IcuLocalizer::new(c);
            assert_eq!(loc.get_locale().as_str(), c, "locale not preserved for {c:?}");
        }

        let huge = "x".repeat(10_000);
        assert_eq!(IcuLocalizer::new(&huge).get_locale().as_str(), huge.as_str());
    }

    #[test]
    fn from_system_language_is_equivalent_to_new() {
        for c in ["en-US", "", "de_DE", "garbage", "日本語"] {
            let a = IcuLocalizer::from_system_language(&AzString::from(c));
            let b = IcuLocalizer::new(c);
            assert_eq!(a.get_locale().as_str(), b.get_locale().as_str());
            assert_eq!(a.get_locale().as_str(), c);
        }
    }

    #[test]
    fn default_is_en_us_and_clone_is_independent() {
        let d = IcuLocalizer::default();
        assert_eq!(d.get_locale().as_str(), "en-US");
        assert_eq!(d.get_language().as_str(), "en");
        assert_eq!(d.get_region().map(|r| r.as_str().to_string()), Some("US".to_string()));

        let mut c = d.clone();
        assert_eq!(c.get_locale().as_str(), "en-US");
        // Mutating the clone must not disturb the original.
        assert!(c.set_locale("de-DE"));
        assert_eq!(c.get_locale().as_str(), "de-DE");
        assert_eq!(d.get_locale().as_str(), "en-US");
    }

    // ── getters: get_locale / get_language / get_region ──────────────────────

    #[test]
    fn get_language_takes_the_first_subtag() {
        let cases = [
            ("en-US", "en"),
            ("en_US", "en"),
            ("en", "en"),
            ("zh-Hans-CN", "zh"),
            ("", ""),
            ("-US", ""),   // leading separator ⇒ empty language, not "US"
            ("_US", ""),
            ("-", ""),
            ("日本-語", "日本"),
        ];
        for (input, expected) in cases {
            assert_eq!(
                IcuLocalizer::new(input).get_language().as_str(),
                expected,
                "get_language({input:?})"
            );
        }
    }

    #[test]
    fn get_language_never_contains_a_subtag_separator() {
        // Invariant: whatever the input, the language is a *single* subtag.
        let mut inputs: Vec<String> =
            hostile_strings().into_iter().collect();
        inputs.extend(
            ["en-US", "a-b-c-d-e", "---", "___", "-_-", "x_y-z"]
                .iter()
                .map(|s| (*s).to_string()),
        );
        for i in inputs {
            let lang = IcuLocalizer::new(&i).get_language();
            let lang = lang.as_str();
            assert!(
                !lang.contains('-') && !lang.contains('_'),
                "get_language({i:?}) = {lang:?} still contains a separator"
            );
            // It must also be a prefix of the original locale string.
            assert!(i.starts_with(lang), "get_language({i:?}) = {lang:?} is not a prefix");
        }
    }

    #[test]
    fn get_region_returns_the_second_subtag() {
        let cases: [(&str, Option<&str>); 8] = [
            ("en-US", Some("US")),
            ("en_US", Some("US")),
            ("en", None),
            ("", None),
            ("en-", Some("")), // trailing separator ⇒ an *empty* region, not None
            ("-US", Some("US")),
            ("a-b-c", Some("b")),
            ("x", None),
        ];
        for (input, expected) in cases {
            let got = IcuLocalizer::new(input).get_region();
            let got = got.as_ref().map(AzString::as_str);
            assert_eq!(got, expected, "get_region({input:?})");
        }
    }

    #[test]
    fn get_region_on_a_script_tag_returns_the_script_not_the_region() {
        // CHARACTERISATION (see report): `get_region` is a positional "2nd
        // subtag" lookup, so for a BCP-47 tag carrying a script subtag it
        // returns the *script* ("Hans"), not the region ("CN"). Locked in here
        // so that fixing it is a deliberate, visible change.
        let loc = IcuLocalizer::new("zh-Hans-CN");
        assert_eq!(loc.get_region().as_ref().map(AzString::as_str), Some("Hans"));
    }

    #[test]
    fn getters_do_not_panic_on_hostile_locales() {
        for s in hostile_strings() {
            let loc = IcuLocalizer::new(&s);
            assert_eq!(loc.get_locale().as_str(), s.as_str());
            let _ = loc.get_language();
            let _ = loc.get_region();
        }
    }

    // ── set_locale / load_data_blob ─────────────────────────────────────────

    #[test]
    fn set_locale_always_reports_success_and_roundtrips() {
        let mut loc = en();
        // `set_locale` performs no validation, so it returns `true` even for
        // input that is definitely not a locale.
        let cases: Vec<String> = vec![
            "de-DE".to_string(),
            String::new(),
            "!!!".to_string(),
            "\u{0}".to_string(),
            "y".repeat(5_000),
        ];
        for s in &cases {
            assert!(loc.set_locale(s), "set_locale({s:?}) reported failure");
            assert_eq!(loc.get_locale().as_str(), s.as_str());
        }
    }

    #[test]
    fn set_locale_changes_derived_language_and_region() {
        let mut loc = en();
        assert!(loc.set_locale("pt-BR"));
        assert_eq!(loc.get_language().as_str(), "pt");
        assert_eq!(loc.get_region().as_ref().map(AzString::as_str), Some("BR"));
        assert!(loc.set_locale("ja"));
        assert_eq!(loc.get_language().as_str(), "ja");
        assert_eq!(loc.get_region(), None);
    }

    #[test]
    fn load_data_blob_is_a_noop_and_never_panics() {
        let mut loc = IcuLocalizer::new("de-DE");
        let before = loc.format_integer(1_234_567);

        loc.load_data_blob(Vec::new());
        loc.load_data_blob(vec![0u8; 1]);
        loc.load_data_blob(vec![0xFFu8; 1_000_000]); // 1 MB of garbage
        loc.load_data_blob(b"not an ICU data blob".to_vec());

        // The locale — and therefore the formatting behaviour — is untouched.
        assert_eq!(loc.get_locale().as_str(), "de-DE");
        assert_eq!(loc.format_integer(1_234_567).as_str(), before.as_str());
    }

    // ── private: make_ns_locale ─────────────────────────────────────────────

    #[test]
    fn make_ns_locale_does_not_panic_for_any_identifier() {
        // `NSLocale::localeWithLocaleIdentifier` accepts arbitrary identifiers
        // (it never returns nil), so the contract here is simply: no panic, no
        // nil-deref, for every hostile identifier we can reach it with.
        for s in hostile_strings() {
            let loc = IcuLocalizer::new(&s);
            let ns: Retained<NSLocale> = loc.make_ns_locale();
            drop(ns);
        }
        // And it is callable repeatedly on one instance.
        let loc = en();
        for _ in 0..64 {
            drop(loc.make_ns_locale());
        }
    }

    // ── numeric: format_integer ─────────────────────────────────────────────

    #[test]
    fn format_integer_zero_is_a_bare_zero() {
        assert_eq!(en().format_integer(0).as_str(), "0");
    }

    #[test]
    fn format_integer_roundtrips_through_digit_extraction() {
        // Every value here is < 2^53, so no `f64`/`NSNumber` rounding can bite:
        // stripping the grouping separators must reproduce the exact digits.
        let mut loc = en();
        for v in [
            0i64,
            1,
            7,
            42,
            999,
            1_000,
            1_234_567,
            987_654_321,
            -1,
            -42,
            -1_234_567,
            i64::from(i32::MAX),
            i64::from(i32::MIN),
        ] {
            let out = loc.format_integer(v);
            let out = out.as_str();
            assert_eq!(digits_only(out), abs_digits(v), "format_integer({v}) = {out:?}");
            assert_eq!(has_minus(out), v < 0, "sign mismatch for format_integer({v}) = {out:?}");
        }
    }

    #[test]
    fn format_integer_groups_thousands_in_en_us() {
        assert_eq!(en().format_integer(1_234_567).as_str(), "1,234,567");
    }

    #[test]
    fn format_integer_at_i64_min_and_max_does_not_panic() {
        // The one case where a naive `abs()` implementation would overflow.
        let mut loc = en();
        for v in [i64::MIN, i64::MIN + 1, i64::MAX, i64::MAX - 1] {
            let out = loc.format_integer(v);
            let out = out.as_str();
            assert!(!out.is_empty(), "format_integer({v}) returned an empty string");
            // NSNumberFormatter may round beyond 2^53, so assert only the shape:
            // 19 significant digits and the correct sign.
            assert!(
                digits_only(out).len() >= 19,
                "format_integer({v}) = {out:?} lost magnitude"
            );
            assert_eq!(has_minus(out), v < 0, "sign mismatch for format_integer({v})");
        }
    }

    #[test]
    fn format_integer_is_deterministic_across_calls() {
        let mut loc = en();
        let a = loc.format_integer(-98_765);
        let b = loc.format_integer(-98_765);
        assert_eq!(a.as_str(), b.as_str());
    }

    // ── numeric: format_decimal ─────────────────────────────────────────────

    #[test]
    fn format_decimal_basic_cases() {
        let mut loc = en();
        let strip = |s: &AzString| s.as_str().replace(',', "");

        assert_eq!(strip(&loc.format_decimal(0, 0)), "0");
        assert_eq!(strip(&loc.format_decimal(12_345, 2)), "123.45");
        assert_eq!(strip(&loc.format_decimal(-12_345, 2)), "-123.45");
        assert_eq!(strip(&loc.format_decimal(5, 3)), "0.005");
        assert_eq!(strip(&loc.format_decimal(-5, 3)), "-0.005");
        // Zero decimal places ⇒ no fractional part at all.
        assert_eq!(strip(&loc.format_decimal(42, 0)), "42");
    }

    #[test]
    fn format_decimal_negative_places_scale_up() {
        // `decimal_places < 0` appends zeros (123 × 10^2 = 12300) and clamps the
        // displayed fraction digits to 0.
        let mut loc = en();
        let out = loc.format_decimal(123, -2);
        assert_eq!(digits_only(out.as_str()), "12300", "got {:?}", out.as_str());
        assert!(!out.as_str().contains('.'), "unexpected fraction part in {:?}", out.as_str());
    }

    #[test]
    fn format_decimal_zero_with_places_keeps_the_fraction_digits() {
        let mut loc = en();
        assert_eq!(loc.format_decimal(0, 2).as_str().replace(',', ""), "0.00");
    }

    #[test]
    fn format_decimal_at_i64_extremes_does_not_panic() {
        let mut loc = en();
        for v in [i64::MIN, i64::MAX] {
            for dp in [0i16, 2, -2] {
                let out = loc.format_decimal(v, dp);
                assert!(!out.as_str().is_empty(), "format_decimal({v}, {dp}) was empty");
                assert_eq!(
                    has_minus(out.as_str()),
                    v < 0,
                    "sign mismatch for format_decimal({v}, {dp})"
                );
            }
        }
    }

    #[test]
    fn format_decimal_at_i16_max_places_does_not_panic() {
        // 32767 fraction digits: the value underflows `f64` to 0.0, and the
        // formatter must still produce *something* rather than panicking.
        let mut loc = en();
        let out = loc.format_decimal(1, i16::MAX);
        assert!(!out.as_str().is_empty());
        assert!(out.as_str().starts_with('0'), "got {:?}", out.as_str());
    }

    #[test]
    fn format_decimal_large_positive_places_are_well_formed() {
        let mut loc = en();
        let out = loc.format_decimal(1, 18);
        let out = out.as_str();
        assert!(out.starts_with("0.0"), "format_decimal(1, 18) = {out:?}");
        assert!(!has_minus(out));
    }

    #[test]
    #[ignore = "KNOWN BUG: decimal_places = i16::MIN overflows in decimal_string(); \
                debug-panics on `-decimal_places`, and in release wraps to a ~1.8e19 \
                push('0') loop that exhausts memory. Un-ignore once icu.rs guards it."]
    fn format_decimal_at_i16_min_places_must_not_overflow() {
        // Reachable straight from the public API. `decimal_string` does
        // `for _ in 0..(-decimal_places as usize)`, and `-i16::MIN` is not
        // representable in i16.
        let mut loc = en();
        let out = loc.format_decimal(1, i16::MIN);
        assert!(!out.as_str().is_empty());
    }

    // ── numeric: get_plural_category ────────────────────────────────────────

    #[test]
    fn plural_category_english_defaults() {
        let mut loc = en();
        assert_eq!(loc.get_plural_category(1), PluralCategory::One);
        assert_eq!(loc.get_plural_category(0), PluralCategory::Other);
        assert_eq!(loc.get_plural_category(2), PluralCategory::Other);
        assert_eq!(loc.get_plural_category(100), PluralCategory::Other);
        assert_eq!(loc.get_plural_category(i64::MAX), PluralCategory::Other);
    }

    #[test]
    fn plural_category_unknown_or_empty_locale_falls_back_to_english() {
        for tag in ["", "qqq", "!!!", "-", "zz-ZZ"] {
            let mut loc = IcuLocalizer::new(tag);
            assert_eq!(loc.get_plural_category(1), PluralCategory::One, "tag {tag:?}");
            assert_eq!(loc.get_plural_category(5), PluralCategory::Other, "tag {tag:?}");
        }
    }

    #[test]
    fn plural_category_russian_rules() {
        let mut loc = IcuLocalizer::new("ru-RU");
        assert_eq!(loc.get_plural_category(1), PluralCategory::One);
        assert_eq!(loc.get_plural_category(21), PluralCategory::One);
        assert_eq!(loc.get_plural_category(2), PluralCategory::Few);
        assert_eq!(loc.get_plural_category(4), PluralCategory::Few);
        assert_eq!(loc.get_plural_category(5), PluralCategory::Many);
        assert_eq!(loc.get_plural_category(11), PluralCategory::Many);
        assert_eq!(loc.get_plural_category(14), PluralCategory::Many);
    }

    #[test]
    fn plural_category_arabic_rules() {
        let mut loc = IcuLocalizer::new("ar");
        assert_eq!(loc.get_plural_category(0), PluralCategory::Zero);
        assert_eq!(loc.get_plural_category(1), PluralCategory::One);
        assert_eq!(loc.get_plural_category(2), PluralCategory::Two);
        assert_eq!(loc.get_plural_category(3), PluralCategory::Few);
        assert_eq!(loc.get_plural_category(11), PluralCategory::Many);
        assert_eq!(loc.get_plural_category(100), PluralCategory::Other);
    }

    #[test]
    fn plural_category_welsh_and_french_rules() {
        let mut cy = IcuLocalizer::new("cy");
        assert_eq!(cy.get_plural_category(0), PluralCategory::Zero);
        assert_eq!(cy.get_plural_category(3), PluralCategory::Few);
        assert_eq!(cy.get_plural_category(6), PluralCategory::Many);
        assert_eq!(cy.get_plural_category(7), PluralCategory::Other);

        let mut fr = IcuLocalizer::new("fr-FR");
        assert_eq!(fr.get_plural_category(0), PluralCategory::One);
        assert_eq!(fr.get_plural_category(1), PluralCategory::One);
        assert_eq!(fr.get_plural_category(2), PluralCategory::Other);
    }

    #[test]
    fn plural_category_negative_one_is_other_in_english() {
        // CHARACTERISATION (see report): CLDR classifies English by |n|, so -1
        // should be `One`. The table matches on the signed value, so it is
        // `Other`. Locked in so the divergence is visible, not silent.
        let mut loc = en();
        assert_eq!(loc.get_plural_category(-1), PluralCategory::Other);
        assert_eq!(loc.get_plural_category(1), PluralCategory::One);
    }

    #[test]
    fn plural_category_negative_values_do_not_panic() {
        // Every branch of the CLDR table, exercised with negatives — but *not*
        // i64::MIN, which is covered separately below.
        for tag in ["en", "ar", "ru", "pl", "cs", "sl", "lt", "lv", "ro", "mt", "he", "ga", "cy", "fr"] {
            let mut loc = IcuLocalizer::new(tag);
            for v in [-1i64, -2, -11, -100, i64::MIN + 1] {
                let _ = loc.get_plural_category(v);
            }
        }
    }

    #[test]
    fn plural_category_at_i64_min_must_not_overflow() {
        // RED — genuine bug (see report). The `ar`/`ru`/`pl`/`sl`/`lt`/`lv`/
        // `ro`/`mt` branches of `plural_for` compute `n.abs() % 100`, and
        // `i64::MIN.abs()` is not representable: debug builds panic with
        // "attempt to negate with overflow", release builds silently yield a
        // *negative* modulus and therefore the wrong category.
        //
        // Reachable from safe public API: `IcuLocalizer::new("ru").get_plural_category(i64::MIN)`.
        // The assertion is deliberately weak — any category will do; the point
        // is that the call must return at all.
        for tag in ["ru", "ar", "pl", "sl", "lt", "lv", "ro", "mt"] {
            let mut loc = IcuLocalizer::new(tag);
            let cat = loc.get_plural_category(i64::MIN);
            assert!(
                matches!(
                    cat,
                    PluralCategory::Zero
                        | PluralCategory::One
                        | PluralCategory::Two
                        | PluralCategory::Few
                        | PluralCategory::Many
                        | PluralCategory::Other
                ),
                "{tag}: get_plural_category(i64::MIN) returned a bogus category"
            );
        }
    }

    // ── numeric: pluralize ──────────────────────────────────────────────────

    #[test]
    fn pluralize_selects_the_english_template_and_substitutes() {
        let mut loc = en();
        let p = |loc: &mut IcuLocalizer, v: i64| {
            loc.pluralize(v, "zero", "{} item", "two", "few", "many", "{} items")
                .as_str()
                .to_string()
        };
        assert_eq!(p(&mut loc, 1), "1 item");
        assert_eq!(p(&mut loc, 0), "0 items");
        assert_eq!(p(&mut loc, 2), "2 items");
        assert_eq!(p(&mut loc, -5), "-5 items");
    }

    #[test]
    fn pluralize_replaces_every_placeholder_occurrence() {
        let mut loc = en();
        let out = loc.pluralize(7, "z", "o", "t", "f", "m", "{} of {} ({})");
        assert_eq!(out.as_str(), "7 of 7 (7)");
    }

    #[test]
    fn pluralize_without_a_placeholder_returns_the_template_verbatim() {
        let mut loc = en();
        assert_eq!(loc.pluralize(3, "z", "o", "t", "f", "m", "no digits here").as_str(), "no digits here");
        // Empty templates stay empty rather than panicking.
        assert_eq!(loc.pluralize(3, "", "", "", "", "", "").as_str(), "");
    }

    #[test]
    fn pluralize_at_i64_extremes_does_not_panic_in_english() {
        let mut loc = en();
        let out = loc.pluralize(i64::MIN, "z", "o", "t", "f", "m", "{} items");
        assert_eq!(out.as_str(), "-9223372036854775808 items");
        let out = loc.pluralize(i64::MAX, "z", "o", "t", "f", "m", "{} items");
        assert_eq!(out.as_str(), "9223372036854775807 items");
    }

    #[test]
    fn pluralize_uses_the_russian_table() {
        let mut loc = IcuLocalizer::new("ru");
        assert_eq!(loc.pluralize(1, "z", "one", "t", "few", "many", "o").as_str(), "one");
        assert_eq!(loc.pluralize(3, "z", "one", "t", "few", "many", "o").as_str(), "few");
        assert_eq!(loc.pluralize(5, "z", "one", "t", "few", "many", "o").as_str(), "many");
    }

    #[test]
    fn pluralize_tolerates_hostile_templates() {
        let mut loc = en();
        for t in hostile_strings() {
            let _ = loc.pluralize(1, &t, &t, &t, &t, &t, &t);
            let _ = loc.pluralize(0, &t, &t, &t, &t, &t, &t);
        }
    }

    // ── format_list ─────────────────────────────────────────────────────────

    #[test]
    fn format_list_unit_is_a_plain_comma_join() {
        // The `Unit` path never touches Foundation, so it is exactly specified.
        let mut loc = en();
        assert_eq!(loc.format_list(&az(&["a", "b", "c"]), ListType::Unit).as_str(), "a, b, c");
        assert_eq!(loc.format_list(&az(&["solo"]), ListType::Unit).as_str(), "solo");
        assert_eq!(loc.format_list(&[], ListType::Unit).as_str(), "");
        assert_eq!(loc.format_list(&az(&["", ""]), ListType::Unit).as_str(), ", ");
    }

    #[test]
    fn format_list_and_or_mention_every_item() {
        // The exact conjunction wording is Foundation's business; what must hold
        // is that no item is dropped.
        let mut loc = en();
        let items = az(&["alpha", "beta", "gamma"]);
        for lt in [ListType::And, ListType::Or] {
            let out = loc.format_list(&items, lt);
            let out = out.as_str();
            for want in ["alpha", "beta", "gamma"] {
                assert!(out.contains(want), "{lt:?} output {out:?} dropped {want:?}");
            }
        }
    }

    #[test]
    fn format_list_single_item_is_just_that_item() {
        let mut loc = en();
        for lt in [ListType::And, ListType::Or] {
            let out = loc.format_list(&az(&["only"]), lt);
            assert!(out.as_str().contains("only"), "{lt:?} -> {:?}", out.as_str());
        }
    }

    #[test]
    fn format_list_empty_input_does_not_panic() {
        let mut loc = en();
        for lt in [ListType::And, ListType::Or, ListType::Unit] {
            let out = loc.format_list(&[], lt);
            assert!(out.as_str().is_empty(), "{lt:?} on [] -> {:?}", out.as_str());
        }
    }

    #[test]
    fn format_list_with_many_items_does_not_panic() {
        let mut loc = en();
        let items: Vec<AzString> = (0..1_000).map(|i| AzString::from(i.to_string())).collect();
        for lt in [ListType::And, ListType::Or, ListType::Unit] {
            let out = loc.format_list(&items, lt);
            assert!(!out.as_str().is_empty());
            assert!(out.as_str().contains("999"), "{lt:?} dropped the last item");
        }
    }

    #[test]
    fn format_list_with_hostile_items_does_not_panic() {
        // Interior NULs, ZWJ emoji, RTL, BOM, 100k-char items — NSString::from_str
        // must survive all of them.
        let mut loc = en();
        let items: Vec<AzString> = hostile_strings().iter().map(|s| AzString::from(s.as_str())).collect();
        for lt in [ListType::And, ListType::Or, ListType::Unit] {
            let _ = loc.format_list(&items, lt);
        }
    }

    // ── collation: compare / strings_equal / sort ───────────────────────────

    #[test]
    fn compare_is_reflexive_and_deterministic() {
        let mut loc = en();
        for s in hostile_strings() {
            assert_eq!(loc.compare(&s, &s), Ordering::Equal, "compare({s:?}, itself) != Equal");
            let a = loc.compare(&s, "m");
            let b = loc.compare(&s, "m");
            assert_eq!(a, b, "compare is not deterministic for {s:?}");
        }
    }

    #[test]
    fn compare_is_antisymmetric() {
        let mut loc = en();
        let pairs = [
            ("a", "b"),
            ("b", "a"),
            ("", "a"),
            ("a", ""),
            ("", ""),
            ("apple", "apples"),
            ("😀", "😁"),
            ("日", "本"),
            ("a\0b", "a\0c"),
        ];
        for (a, b) in pairs {
            let ab = loc.compare(a, b);
            let ba = loc.compare(b, a);
            assert_eq!(ab, ba.reverse(), "compare({a:?}, {b:?}) = {ab:?} but the reverse was {ba:?}");
        }
    }

    #[test]
    fn compare_orders_the_empty_string_first() {
        let mut loc = en();
        assert_eq!(loc.compare("", ""), Ordering::Equal);
        assert_eq!(loc.compare("", "a"), Ordering::Less);
        assert_eq!(loc.compare("a", ""), Ordering::Greater);
    }

    #[test]
    fn compare_handles_unicode_and_huge_strings_without_panicking() {
        // The `NSRange` is built from `a`'s UTF-16 length, so astral-plane input
        // (surrogate pairs) is the interesting case for an out-of-range raise.
        let mut loc = en();
        let big_a = "a".repeat(100_000);
        let big_b = "a".repeat(99_999);
        assert_eq!(loc.compare(&big_a, &big_a), Ordering::Equal);
        assert_eq!(loc.compare(&big_a, &big_b), Ordering::Greater);
        for s in hostile_strings() {
            let _ = loc.compare(&s, "😀");
            let _ = loc.compare("😀", &s);
        }
    }

    #[test]
    fn strings_equal_agrees_with_compare() {
        let mut loc = en();
        for s in hostile_strings() {
            assert!(loc.strings_equal(&s, &s), "{s:?} is not equal to itself");
            let eq = loc.strings_equal(&s, "zzz");
            assert_eq!(eq, loc.compare(&s, "zzz") == Ordering::Equal);
        }
        assert!(!en().strings_equal("a", "b"));
    }

    #[test]
    fn sort_strings_preserves_length_and_multiset() {
        let mut loc = en();
        let mut v = az(&["pear", "apple", "pear", "", "banana", "😀", "Apple"]);
        let before_len = v.len();
        let mut before: Vec<&str> = vec!["pear", "apple", "pear", "", "banana", "😀", "Apple"];
        before.sort_unstable();

        loc.sort_strings(&mut v);

        assert_eq!(v.len(), before_len, "sort_strings changed the length");
        let mut after = as_strs(&v);
        after.sort_unstable();
        assert_eq!(after, before, "sort_strings lost or duplicated an element");
    }

    #[test]
    fn sort_strings_is_idempotent_and_ordered_under_compare() {
        let mut loc = en();
        let mut v = az(&["delta", "alpha", "charlie", "bravo"]);
        loc.sort_strings(&mut v);
        let once = as_strs(&v).iter().map(|s| (*s).to_string()).collect::<Vec<_>>();
        loc.sort_strings(&mut v);
        assert_eq!(as_strs(&v), once, "sorting an already-sorted slice changed it");

        // Adjacent pairs must be non-decreasing under the very comparator used.
        for w in 0..v.len().saturating_sub(1) {
            let (a, b) = (v[w].as_str().to_string(), v[w + 1].as_str().to_string());
            assert_ne!(loc.compare(&a, &b), Ordering::Greater, "{a:?} sorted before {b:?}");
        }
    }

    #[test]
    fn sort_strings_on_empty_and_single_slices_does_not_panic() {
        let mut loc = en();
        let mut empty: Vec<AzString> = Vec::new();
        loc.sort_strings(&mut empty);
        assert!(empty.is_empty());

        let mut one = az(&["x"]);
        loc.sort_strings(&mut one);
        assert_eq!(as_strs(&one), vec!["x"]);

        // All-equal input is the classic comparator-consistency trap.
        let mut same = az(&["q", "q", "q", "q"]);
        loc.sort_strings(&mut same);
        assert_eq!(as_strs(&same), vec!["q", "q", "q", "q"]);
    }

    #[test]
    fn sorted_strings_leaves_the_input_untouched() {
        let mut loc = en();
        let input = az(&["c", "a", "b"]);
        let out = loc.sorted_strings(&input);

        assert_eq!(as_strs(&input), vec!["c", "a", "b"], "sorted_strings mutated its input");
        assert_eq!(out.len(), input.len());
        let mut got = as_strs(&out);
        got.sort_unstable();
        assert_eq!(got, vec!["a", "b", "c"]);
    }

    #[test]
    fn sorted_strings_on_empty_input_returns_empty() {
        let mut loc = en();
        assert!(loc.sorted_strings(&[]).is_empty());
    }

    // ── get_sort_key ────────────────────────────────────────────────────────

    #[test]
    fn get_sort_key_roundtrips_to_the_original_string() {
        // Documented as an *identity* key: raw UTF-8 bytes, so encode == decode.
        let mut loc = en();
        for s in hostile_strings() {
            let key = loc.get_sort_key(&s);
            assert_eq!(key.as_slice(), s.as_bytes(), "sort key is not the UTF-8 bytes of {s:?}");
            assert_eq!(String::from_utf8(key).expect("sort key must stay valid UTF-8"), s);
        }
    }

    #[test]
    fn get_sort_key_is_empty_only_for_the_empty_string() {
        let mut loc = en();
        assert!(loc.get_sort_key("").is_empty());
        assert!(!loc.get_sort_key("a").is_empty());
        assert!(!loc.get_sort_key("\0").is_empty());
    }

    #[test]
    fn get_sort_key_ignores_the_locale() {
        // It is a byte-identity key, so it must not vary with the locale — this
        // is exactly why the doc comment warns it will not agree with `compare`.
        let key_en = en().get_sort_key("straße");
        let key_de = IcuLocalizer::new("de-DE").get_sort_key("straße");
        let key_ar = IcuLocalizer::new("ar-EG").get_sort_key("straße");
        assert_eq!(key_en, key_de);
        assert_eq!(key_en, key_ar);
    }

    // ── private helpers: ns_date_style / gregorian ──────────────────────────

    #[test]
    fn ns_date_style_maps_every_length_distinctly() {
        assert_eq!(ns_date_style(FormatLength::Short), NSDateFormatterStyle::ShortStyle);
        assert_eq!(ns_date_style(FormatLength::Medium), NSDateFormatterStyle::MediumStyle);
        assert_eq!(ns_date_style(FormatLength::Long), NSDateFormatterStyle::LongStyle);

        // No two lengths may collapse onto the same style, and none may be NoStyle
        // (that would silently blank the date component out).
        let all = [
            ns_date_style(FormatLength::Short),
            ns_date_style(FormatLength::Medium),
            ns_date_style(FormatLength::Long),
        ];
        for (i, a) in all.iter().enumerate() {
            assert_ne!(*a, NSDateFormatterStyle::NoStyle);
            for b in all.iter().skip(i + 1) {
                assert_ne!(a, b, "two FormatLengths map to the same NSDateFormatterStyle");
            }
        }
    }

    #[test]
    fn gregorian_calendar_is_always_available() {
        // Every other date helper `?`s on this, so if it ever returns None the
        // whole date path silently degrades to "Invalid date".
        unsafe {
            assert!(gregorian().is_some(), "the Gregorian calendar is missing");
        }
    }

    // ── private helpers: make_ns_date / make_ns_time / make_ns_datetime ─────

    #[test]
    fn make_ns_date_accepts_ordinary_dates() {
        unsafe {
            assert!(make_ns_date(2025, 1, 15).is_some());
            assert!(make_ns_date(2000, 2, 29).is_some()); // leap day
            assert!(make_ns_date(1, 1, 1).is_some());
        }
    }

    #[test]
    fn make_ns_date_over_the_reachable_u8_range_does_not_panic() {
        // `format_date` feeds `IcuDate.month`/`.day` (both `u8`) straight in, so
        // 0..=255 is the whole reachable domain. Foundation is lenient and may
        // roll values over; the contract here is just Some/None, never a panic.
        unsafe {
            for month in [0isize, 1, 2, 12, 13, 200, 255] {
                for day in [0isize, 1, 28, 31, 32, 200, 255] {
                    let _ = make_ns_date(2025, month, day);
                }
            }
        }
    }

    #[test]
    fn make_ns_date_at_i32_year_extremes_does_not_panic() {
        unsafe {
            for year in [0i32, -1, 1, i32::MIN, i32::MAX, i32::MIN + 1, i32::MAX - 1] {
                let _ = make_ns_date(year, 1, 1);
                let _ = make_ns_date(year, 0, 0);
                let _ = make_ns_date(year, 255, 255);
            }
        }
    }

    #[test]
    fn make_ns_time_accepts_the_full_clock_range() {
        unsafe {
            assert!(make_ns_time(0, 0, 0).is_some());
            assert!(make_ns_time(12, 34, 56).is_some());
            assert!(make_ns_time(23, 59, 59).is_some());
        }
    }

    #[test]
    fn make_ns_time_out_of_range_components_do_not_panic() {
        unsafe {
            for h in [-1isize, 0, 23, 24, 25, 255] {
                for m in [-1isize, 0, 59, 60, 255] {
                    for s in [-1isize, 0, 59, 60, 61, 255] {
                        let _ = make_ns_time(h, m, s);
                    }
                }
            }
        }
    }

    #[test]
    fn make_ns_datetime_accepts_ordinary_values_and_survives_extremes() {
        unsafe {
            assert!(make_ns_datetime(2025, 1, 15, 12, 34, 56).is_some());
            for year in [0i32, -1, i32::MIN, i32::MAX] {
                let _ = make_ns_datetime(year, 0, 0, 0, 0, 0);
                let _ = make_ns_datetime(year, 255, 255, 255, 255, 255);
            }
        }
    }

    #[test]
    #[ignore = "Torture case: isize::MIN/MAX components are NOT reachable through the \
                public API (IcuDate/IcuTime use u8 + i32), and an out-of-range \
                NSDateComponents value can raise an ObjC exception, which aborts the \
                whole test binary rather than failing one test. Run deliberately."]
    fn make_ns_date_at_isize_extremes_does_not_panic() {
        unsafe {
            let _ = make_ns_date(i32::MAX, isize::MAX, isize::MAX);
            let _ = make_ns_date(i32::MIN, isize::MIN, isize::MIN);
            let _ = make_ns_time(isize::MAX, isize::MAX, isize::MAX);
            let _ = make_ns_time(isize::MIN, isize::MIN, isize::MIN);
            let _ = make_ns_datetime(i32::MIN, isize::MIN, isize::MIN, isize::MIN, isize::MIN, isize::MIN);
        }
    }

    // ── format_date / format_time / format_datetime ─────────────────────────

    #[test]
    fn format_date_produces_a_non_empty_ok_for_a_valid_date() {
        let mut loc = en();
        for length in [FormatLength::Short, FormatLength::Medium, FormatLength::Long] {
            let r = loc.format_date(IcuDate::new(2025, 1, 15), length);
            assert!(is_ok(&r), "format_date(2025-01-15, {length:?}) was an Err");
            assert!(!unwrap_ok(r).is_empty());
        }
    }

    #[test]
    fn format_date_styles_are_actually_distinct() {
        // If `ns_date_style` were mis-wired, every length would render the same.
        let mut loc = en();
        let d = IcuDate::new(2025, 1, 15);
        let short = unwrap_ok(loc.format_date(d, FormatLength::Short));
        let long = unwrap_ok(loc.format_date(d, FormatLength::Long));
        assert_ne!(short, long, "Short and Long rendered identically: {short:?}");
    }

    #[test]
    fn format_date_with_zero_or_overflowing_components_does_not_panic() {
        // month/day are `u8`, so 0 and 255 are reachable from safe code.
        let mut loc = en();
        for (m, d) in [(0u8, 0u8), (0, 1), (1, 0), (13, 32), (255, 255), (2, 30)] {
            let r = loc.format_date(IcuDate::new(2025, m, d), FormatLength::Short);
            assert_payload_non_empty(&r);
        }
    }

    #[test]
    fn format_date_at_extreme_years_does_not_panic() {
        let mut loc = en();
        for year in [0i32, -1, -44, i32::MIN, i32::MAX] {
            let r = loc.format_date(IcuDate::new(year, 1, 1), FormatLength::Medium);
            assert_payload_non_empty(&r);
        }
    }

    #[test]
    fn format_time_is_ok_and_seconds_change_the_output() {
        let mut loc = en();
        let t = IcuTime::new(12, 34, 56);
        let without = unwrap_ok(loc.format_time(t, false));
        let with = unwrap_ok(loc.format_time(t, true));
        assert!(!without.is_empty() && !with.is_empty());
        assert_ne!(
            without, with,
            "include_seconds made no difference: {without:?}"
        );
        // The seconds-bearing rendering must be the longer one.
        assert!(with.len() > without.len(), "{with:?} is not longer than {without:?}");
    }

    #[test]
    fn format_time_at_clock_boundaries_and_beyond_does_not_panic() {
        let mut loc = en();
        for (h, m, s) in [
            (0u8, 0u8, 0u8),
            (23, 59, 59),
            (24, 0, 0),
            (24, 60, 60),
            (255, 255, 255),
        ] {
            for secs in [false, true] {
                let r = loc.format_time(IcuTime::new(h, m, s), secs);
                assert_payload_non_empty(&r);
            }
        }
    }

    #[test]
    fn format_datetime_is_ok_and_non_empty() {
        let mut loc = en();
        let dt = IcuDateTime::new(IcuDate::new(2025, 1, 15), IcuTime::new(12, 34, 56));
        for length in [FormatLength::Short, FormatLength::Medium, FormatLength::Long] {
            let r = loc.format_datetime(dt, length);
            assert!(is_ok(&r), "format_datetime(.., {length:?}) was an Err");
            let s = unwrap_ok(r);
            assert!(!s.is_empty());
            // A datetime must be strictly richer than the bare date.
            let date_only = unwrap_ok(loc.format_date(dt.date, length));
            assert!(
                s.len() > date_only.len(),
                "{length:?}: datetime {s:?} is not longer than date {date_only:?}"
            );
        }
    }

    #[test]
    fn format_datetime_with_extreme_components_does_not_panic() {
        let mut loc = en();
        let cases = [
            IcuDateTime::new(IcuDate::new(0, 0, 0), IcuTime::new(0, 0, 0)),
            IcuDateTime::new(IcuDate::new(i32::MIN, 255, 255), IcuTime::new(255, 255, 255)),
            IcuDateTime::new(IcuDate::new(i32::MAX, 255, 255), IcuTime::new(255, 255, 255)),
        ];
        for dt in cases {
            for length in [FormatLength::Short, FormatLength::Medium, FormatLength::Long] {
                let r = loc.format_datetime(dt, length);
                assert_payload_non_empty(&r);
            }
        }
    }

    #[test]
    fn date_formatting_works_under_a_hostile_locale() {
        // A garbage locale identifier must degrade gracefully, not panic.
        for tag in ["", "!!!", "\u{FEFF}", "not-a-locale-at-all"] {
            let mut loc = IcuLocalizer::new(tag);
            let dt = IcuDateTime::new(IcuDate::new(2025, 1, 15), IcuTime::new(12, 34, 56));
            assert_payload_non_empty(&loc.format_date(dt.date, FormatLength::Long));
            assert_payload_non_empty(&loc.format_time(dt.time, true));
            assert_payload_non_empty(&loc.format_datetime(dt, FormatLength::Medium));
            assert!(!loc.format_integer(1_234).as_str().is_empty());
        }
    }
}
