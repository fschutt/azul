//! Windows NLS-based ICU backend for azul.
//!
//! Replaces ICU4X (which bundles ~3.3 MB of locale data blobs) with Win32 NLS
//! functions dynamically loaded from `kernel32.dll` at first use:
//! `GetNumberFormatEx`, `GetDateFormatEx`, `GetTimeFormatEx`, `CompareStringEx`.
//!
//! List formatting uses a compact language-keyed conjunction table.
//! Plural rules use the same compact CLDR lookup table as the macOS backend.
//!
//! Requires Windows Vista or later (all target functions are Vista+).

use alloc::{string::String, vec::Vec};
use core::cmp::Ordering;
use std::sync::OnceLock;

use azul_css::AzString;

use super::{FormatLength, IcuDate, IcuDateTime, IcuResult, IcuTime, ListType, PluralCategory, decimal_string, plural_for};

// ─── Win32 inline types (no winapi dep needed) ───────────────────────────────

type HMODULE = *mut core::ffi::c_void;

/// Matches the Win32 `SYSTEMTIME` layout exactly.
#[repr(C)]
struct SystemTime {
    year: u16,
    month: u16,
    day_of_week: u16,
    day: u16,
    hour: u16,
    minute: u16,
    second: u16,
    milliseconds: u16,
}

/// Matches the Win32 `NUMBERFMTW` layout exactly.
/// Only used when we need to override the number of decimal digits.
#[repr(C)]
struct NumberFmt {
    num_digits: u32,
    leading_zero: u32,
    grouping: u32,
    decimal_sep: *mut u16,
    thousand_sep: *mut u16,
    negative_order: u32,
}

// ─── Function pointer types ───────────────────────────────────────────────────

type GetNumberFormatExFn = unsafe extern "system" fn(
    lp_locale_name: *const u16,
    dw_flags: u32,
    lp_value: *const u16,
    lp_format: *const NumberFmt,
    lp_number_str: *mut u16,
    cch_number: i32,
) -> i32;

type GetDateFormatExFn = unsafe extern "system" fn(
    lp_locale_name: *const u16,
    dw_flags: u32,
    lp_date: *const SystemTime,
    lp_format: *const u16,
    lp_date_str: *mut u16,
    cch_date: i32,
    lp_calendar: *const u16,
) -> i32;

type GetTimeFormatExFn = unsafe extern "system" fn(
    lp_locale_name: *const u16,
    dw_flags: u32,
    lp_time: *const SystemTime,
    lp_format: *const u16,
    lp_time_str: *mut u16,
    cch_time: i32,
) -> i32;

type CompareStringExFn = unsafe extern "system" fn(
    lp_locale_name: *const u16,
    dw_cmp_flags: u32,
    lp_string1: *const u16,
    cch_count1: i32,
    lp_string2: *const u16,
    cch_count2: i32,
    lp_version_information: *mut core::ffi::c_void,
    lp_reserved: *mut core::ffi::c_void,
    l_param: isize,
) -> i32;

// ─── Kernel32 bootstrap (always available, no dynamic load needed) ────────────

extern "system" {
    fn LoadLibraryW(lp_lib_file_name: *const u16) -> HMODULE;
    fn GetProcAddress(
        h_module: HMODULE,
        lp_proc_name: *const u8,
    ) -> *mut core::ffi::c_void;
}

// ─── Lazy-loaded NLS function table ──────────────────────────────────────────

struct NlsFns {
    get_number_format_ex: GetNumberFormatExFn,
    get_date_format_ex:   GetDateFormatExFn,
    get_time_format_ex:   GetTimeFormatExFn,
    compare_string_ex:    CompareStringExFn,
}

// SAFETY: these are read-only function pointers after initialization.
unsafe impl Send for NlsFns {}
unsafe impl Sync for NlsFns {}

static NLS: OnceLock<Option<NlsFns>> = OnceLock::new();

fn nls() -> Option<&'static NlsFns> {
    NLS.get_or_init(|| {
        // kernel32.dll is always mapped; this just bumps its refcount.
        let name: Vec<u16> = "kernel32.dll\0".encode_utf16().collect();
        let hmod = unsafe { LoadLibraryW(name.as_ptr()) };
        if hmod.is_null() {
            return None;
        }
        macro_rules! sym {
            ($name:literal) => {{
                let ptr = unsafe {
                    GetProcAddress(hmod, concat!($name, "\0").as_ptr())
                };
                if ptr.is_null() {
                    return None;
                }
                unsafe { core::mem::transmute(ptr) }
            }};
        }
        Some(NlsFns {
            get_number_format_ex: sym!("GetNumberFormatEx"),
            get_date_format_ex:   sym!("GetDateFormatEx"),
            get_time_format_ex:   sym!("GetTimeFormatEx"),
            compare_string_ex:    sym!("CompareStringEx"),
        })
    })
    .as_ref()
}

// ─── UTF-16 helpers ───────────────────────────────────────────────────────────

fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(core::iter::once(0)).collect()
}

/// Read a null-terminated UTF-16 output buffer returned by NLS functions.
/// `n` is the return value (chars written including null terminator).
fn from_wide_n(buf: &[u16], n: i32) -> String {
    if n <= 0 {
        return String::new();
    }
    let len = (n as usize).saturating_sub(1); // exclude null
    String::from_utf16_lossy(&buf[..len]).to_string()
}

/// Call an NLS formatting function that fills a buffer.
/// `f(buf_ptr, buf_len) -> chars_written_including_null`
fn fmt_buf(f: impl Fn(*mut u16, i32) -> i32) -> Option<String> {
    let mut buf = vec![0u16; 256];
    let n = f(buf.as_mut_ptr(), buf.len() as i32);
    if n <= 0 { None } else { Some(from_wide_n(&buf, n)) }
}

// ─── Win32 flag constants ─────────────────────────────────────────────────────

const DATE_SHORTDATE:  u32 = 0x0000_0001;
const DATE_LONGDATE:   u32 = 0x0000_0002;
const TIME_NOSECONDS:  u32 = 0x0000_0002;

// CompareStringEx return values (0 = failure)
const CSTR_LESS_THAN:    i32 = 1;
const CSTR_EQUAL:        i32 = 2;
const CSTR_GREATER_THAN: i32 = 3;

// ─── List formatting helpers ──────────────────────────────────────────────────
//
// Windows has no single NLS API for list formatting.  We use a compact
// hardcoded conjunction table covering the most common languages.

fn conjunction_and(lang: &str) -> &'static str {
    match lang {
        "de" => "und",   "fr" => "et",    "es" => "y",    "it" => "e",
        "pt" => "e",     "nl" => "en",    "ru" => "и",    "uk" => "і",
        "be" => "і",     "pl" => "i",     "cs" => "a",    "sk" => "a",
        "sr" => "и",     "hr" => "i",     "bs" => "i",    "sl" => "in",
        "ro" => "și",    "hu" => "és",    "fi" => "ja",   "et" => "ja",
        "lv" => "un",    "lt" => "ir",    "sv" => "och",  "da" => "og",
        "no" | "nb" | "nn" => "og",
        "tr" => "ve",    "ar" => "و",     "he" => "ו",   "ja" => "と",
        "zh" => "和",    "ko" => "와",    "th" => "และ",
        _ => "and",
    }
}

fn conjunction_or(lang: &str) -> &'static str {
    match lang {
        "de" => "oder",  "fr" => "ou",    "es" => "o",    "it" => "o",
        "pt" => "ou",    "nl" => "of",    "ru" => "или",  "uk" => "або",
        "be" => "або",   "pl" => "lub",   "cs" => "nebo", "sk" => "alebo",
        "sr" => "или",   "hr" => "ili",   "bs" => "ili",  "sl" => "ali",
        "ro" => "sau",   "hu" => "vagy",  "fi" => "tai",  "et" => "või",
        "lv" => "vai",   "lt" => "arba",  "sv" => "eller","da" => "eller",
        "no" | "nb" | "nn" => "eller",
        "tr" => "veya",  "ar" => "أو",    "he" => "או",   "ja" => "か",
        "zh" => "或",    "ko" => "또는",  "th" => "หรือ",
        _ => "or",
    }
}

fn join_list(items: &[AzString], conjunction: &str) -> String {
    match items.len() {
        0 => String::new(),
        1 => items[0].as_str().to_string(),
        2 => alloc::format!("{} {} {}", items[0].as_str(), conjunction, items[1].as_str()),
        _ => {
            let init: String = items[..items.len() - 1]
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            alloc::format!("{}, {} {}", init, conjunction, items[items.len() - 1].as_str())
        }
    }
}

// ─── NLS collation helper ─────────────────────────────────────────────────────

/// Compare two strings using `CompareStringEx`.
/// Falls back to lexicographic comparison if the NLS call fails (returns 0).
fn compare_nls(f: &NlsFns, locale_wide: &[u16], a: &str, b: &str) -> Ordering {
    let a_w = to_wide(a);
    let b_w = to_wide(b);
    // Pass -1 to let NLS measure the null-terminated strings itself.
    let result = unsafe {
        (f.compare_string_ex)(
            locale_wide.as_ptr(), 0,
            a_w.as_ptr(), -1,
            b_w.as_ptr(), -1,
            core::ptr::null_mut(), core::ptr::null_mut(), 0,
        )
    };
    match result {
        CSTR_LESS_THAN    => Ordering::Less,
        CSTR_EQUAL        => Ordering::Equal,
        CSTR_GREATER_THAN => Ordering::Greater,
        // 0 means the API call failed; fall back to lexicographic order.
        _ => a.cmp(b),
    }
}

// ─── IcuLocalizer ─────────────────────────────────────────────────────────────

/// Windows NLS-based locale formatter.
///
/// Delegates number, date/time, and collation to Win32 NLS functions loaded
/// dynamically from `kernel32.dll`.  List formatting uses a compact hardcoded
/// conjunction table.  Plural rules use a compact CLDR lookup table.
/// No ICU data blobs are linked.
#[derive(Debug, Clone)]
pub struct IcuLocalizer {
    locale_string: AzString,
    /// Pre-encoded UTF-16 locale name for NLS calls (cached to avoid re-encoding).
    locale_wide: Vec<u16>,
}

impl IcuLocalizer {
    pub fn new(locale_str: &str) -> Self {
        Self {
            locale_string: AzString::from(locale_str),
            locale_wide: to_wide(locale_str),
        }
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
        self.locale_wide = to_wide(locale_str);
        true
    }

    pub fn load_data_blob(&mut self, _data: Vec<u8>) {
        // no-op: NLS always uses system locale data
    }

    fn lang(&self) -> &str {
        self.locale_string.as_str()
            .split(['-', '_'])
            .next()
            .unwrap_or("en")
    }

    // ── Number formatting ───────────────────────────────────────────────────

    pub fn format_integer(&mut self, value: i64) -> AzString {
        let Some(f) = nls() else {
            return AzString::from(value.to_string());
        };
        // Pass value as string without decimal point → NLS outputs 0 decimal digits.
        let value_w = to_wide(&value.to_string());
        let locale_ptr = self.locale_wide.as_ptr();
        let result = fmt_buf(|buf, len| unsafe {
            (f.get_number_format_ex)(locale_ptr, 0, value_w.as_ptr(), core::ptr::null(), buf, len)
        });
        AzString::from(result.unwrap_or_else(|| value.to_string()))
    }

    pub fn format_decimal(&mut self, integer_part: i64, decimal_places: i16) -> AzString {
        let value_str = decimal_string(integer_part, decimal_places);
        let Some(f) = nls() else {
            return AzString::from(value_str);
        };
        let value_w = to_wide(&value_str);
        let locale_ptr = self.locale_wide.as_ptr();
        let result = fmt_buf(|buf, len| unsafe {
            (f.get_number_format_ex)(locale_ptr, 0, value_w.as_ptr(), core::ptr::null(), buf, len)
        });
        AzString::from(result.unwrap_or(value_str))
    }

    // ── Plural rules ────────────────────────────────────────────────────────

    pub fn get_plural_category(&mut self, value: i64) -> PluralCategory {
        plural_for(value, self.lang())
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
            PluralCategory::Zero  => zero,
            PluralCategory::One   => one,
            PluralCategory::Two   => two,
            PluralCategory::Few   => few,
            PluralCategory::Many  => many,
            PluralCategory::Other => other,
        };
        AzString::from(template.replace("{}", &value.to_string()))
    }

    // ── List formatting ─────────────────────────────────────────────────────

    pub fn format_list(&mut self, items: &[AzString], list_type: ListType) -> AzString {
        let lang = self.lang();
        let s = match list_type {
            ListType::Unit => items.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", "),
            ListType::And  => join_list(items, conjunction_and(lang)),
            ListType::Or   => join_list(items, conjunction_or(lang)),
        };
        AzString::from(s)
    }

    // ── Date / time formatting ──────────────────────────────────────────────

    pub fn format_date(&mut self, date: IcuDate, length: FormatLength) -> IcuResult {
        let Some(f) = nls() else {
            return IcuResult::err("NLS unavailable");
        };
        let st = SystemTime {
            year: date.year.clamp(1601, 30827) as u16,
            month: date.month as u16,
            day_of_week: 0,
            day: date.day as u16,
            hour: 0, minute: 0, second: 0, milliseconds: 0,
        };
        let flags = match length {
            FormatLength::Short | FormatLength::Medium => DATE_SHORTDATE,
            FormatLength::Long => DATE_LONGDATE,
        };
        let locale_ptr = self.locale_wide.as_ptr();
        match fmt_buf(|buf, len| unsafe {
            (f.get_date_format_ex)(
                locale_ptr, flags, &st,
                core::ptr::null(), buf, len, core::ptr::null(),
            )
        }) {
            Some(s) => IcuResult::ok(s),
            None    => IcuResult::err("GetDateFormatEx failed"),
        }
    }

    pub fn format_time(&mut self, time: IcuTime, include_seconds: bool) -> IcuResult {
        let Some(f) = nls() else {
            return IcuResult::err("NLS unavailable");
        };
        let st = SystemTime {
            year: 2000, month: 1, day_of_week: 0, day: 1,
            hour: time.hour as u16,
            minute: time.minute as u16,
            second: time.second as u16,
            milliseconds: 0,
        };
        let flags = if include_seconds { 0 } else { TIME_NOSECONDS };
        let locale_ptr = self.locale_wide.as_ptr();
        match fmt_buf(|buf, len| unsafe {
            (f.get_time_format_ex)(locale_ptr, flags, &st, core::ptr::null(), buf, len)
        }) {
            Some(s) => IcuResult::ok(s),
            None    => IcuResult::err("GetTimeFormatEx failed"),
        }
    }

    pub fn format_datetime(&mut self, datetime: IcuDateTime, length: FormatLength) -> IcuResult {
        // Windows has no single "date+time" NLS function; format each part separately.
        let date_str = match self.format_date(datetime.date, length) {
            IcuResult::Ok(s) => s,
            e => return e,
        };
        let time_str = match self.format_time(datetime.time, true) {
            IcuResult::Ok(s) => s,
            e => return e,
        };
        IcuResult::ok(alloc::format!("{} {}", date_str.as_str(), time_str.as_str()))
    }

    // ── Collation ───────────────────────────────────────────────────────────

    pub fn compare(&mut self, a: &str, b: &str) -> Ordering {
        let Some(f) = nls() else {
            return a.cmp(b);
        };
        compare_nls(f, &self.locale_wide, a, b)
    }

    pub fn sort_strings(&mut self, strings: &mut [AzString]) {
        // Clone the locale_wide to avoid borrow issues inside the closure.
        let locale_wide = self.locale_wide.clone();
        if let Some(f) = nls() {
            strings.sort_by(|a, b| {
                compare_nls(f, &locale_wide, a.as_str(), b.as_str())
            });
        } else {
            strings.sort_by(|a, b| a.as_str().cmp(b.as_str()));
        }
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
        // NLS sort keys require LCMapStringEx (LCMAP_SORTKEY); not worth the extra
        // dynamic symbol for cache-key use cases.  Return UTF-8 bytes as a proxy.
        s.as_bytes().to_vec()
    }
}

impl Default for IcuLocalizer {
    fn default() -> Self {
        Self::new("en-US")
    }
}

#[cfg(test)]
mod autotest_generated {
    use core::cell::Cell;

    use super::*;

    // ─── helpers ──────────────────────────────────────────────────────────────

    fn azv(items: &[&str]) -> Vec<AzString> {
        items.iter().map(|s| AzString::from(*s)).collect()
    }

    fn strs(items: &[AzString]) -> Vec<String> {
        items.iter().map(|s| s.as_str().to_string()).collect()
    }

    /// ASCII digits of `s`, in order. Lets us assert that a formatter preserved
    /// the significant digits without pinning locale-specific separators.
    fn digits(s: &str) -> String {
        s.chars().filter(|c| c.is_ascii_digit()).collect()
    }

    fn ok_text(r: &IcuResult) -> Option<String> {
        match r {
            IcuResult::Ok(s) => Some(s.as_str().to_string()),
            IcuResult::Err(_) => None,
        }
    }

    fn err_text(r: &IcuResult) -> Option<String> {
        match r {
            IcuResult::Ok(_) => None,
            IcuResult::Err(e) => Some(e.message.as_str().to_string()),
        }
    }

    /// Fill an NLS-style output buffer the way the real Win32 APIs do:
    /// write the string plus a NUL, return the char count *including* the NUL,
    /// or 0 if the buffer is too small.
    ///
    /// # Safety
    /// `buf` must be valid for `len` `u16` writes.
    unsafe fn write_nls_buf(buf: *mut u16, len: i32, s: &str) -> i32 {
        let wide = to_wide(s);
        if len < 0 || wide.len() as i32 > len {
            return 0;
        }
        unsafe { core::ptr::copy_nonoverlapping(wide.as_ptr(), buf, wide.len()) };
        wide.len() as i32
    }

    /// The `IcuLocalizer` cache invariant: `locale_wide` is always the
    /// NUL-terminated UTF-16 encoding of `locale_string`.
    fn assert_locale_cache_coherent(l: &IcuLocalizer) {
        assert_eq!(l.locale_wide, to_wide(l.locale_string.as_str()));
        assert_eq!(
            l.locale_wide.last().copied(),
            Some(0),
            "locale_wide must stay NUL-terminated (it is handed to Win32 as *const u16)"
        );
    }

    // ─── nls() ────────────────────────────────────────────────────────────────

    #[test]
    fn nls_resolves_and_is_idempotent() {
        let a = nls().map(core::ptr::from_ref::<NlsFns>);
        let b = nls().map(core::ptr::from_ref::<NlsFns>);
        assert_eq!(a, b, "OnceLock must hand back the same table on every call");
        assert!(
            a.is_some(),
            "all four NLS symbols are Vista+; they must resolve from kernel32"
        );
    }

    // ─── to_wide ──────────────────────────────────────────────────────────────

    #[test]
    fn to_wide_empty_is_just_the_terminator() {
        assert_eq!(to_wide(""), vec![0u16]);
    }

    #[test]
    fn to_wide_always_nul_terminates_and_counts_utf16_units() {
        for s in ["", "a", "héllo", "日本語", "👍", "𝄞x"] {
            let w = to_wide(s);
            assert_eq!(w.last().copied(), Some(0), "missing terminator for {s:?}");
            assert_eq!(
                w.len(),
                s.encode_utf16().count() + 1,
                "wrong unit count for {s:?}"
            );
        }
    }

    #[test]
    fn to_wide_encodes_astral_chars_as_surrogate_pairs() {
        // U+1D11E needs two UTF-16 units; a naive `chars()` encoder would emit one.
        let w = to_wide("𝄞");
        assert_eq!(w.len(), 3, "surrogate pair + NUL");
        assert!((0xD800..0xDC00).contains(&w[0]), "high surrogate");
        assert!((0xDC00..0xE000).contains(&w[1]), "low surrogate");
        assert_eq!(w[2], 0);
    }

    #[test]
    fn to_wide_keeps_interior_nul_which_truncates_downstream_nls_calls() {
        // NLS is handed these buffers with cchCount == -1, i.e. "measure to the
        // first NUL". An interior NUL therefore silently truncates the value.
        // Pinning the encoding here so the hazard is visible at the boundary.
        assert_eq!(to_wide("a\u{0}b"), vec![97u16, 0, 98, 0]);
    }

    #[test]
    fn to_wide_handles_a_very_long_string() {
        let s = "a".repeat(100_000);
        assert_eq!(to_wide(&s).len(), 100_001);
    }

    // ─── from_wide_n ──────────────────────────────────────────────────────────

    #[test]
    fn from_wide_n_zero_and_negative_return_empty() {
        let buf = [104u16, 105, 0];
        // 0 and negatives are how the Win32 APIs report failure.
        for n in [0i32, -1, -7, i32::MIN] {
            assert_eq!(from_wide_n(&buf, n), "", "n = {n} must yield an empty string");
        }
    }

    #[test]
    fn from_wide_n_one_is_terminator_only() {
        let buf = [0u16, 65, 66];
        assert_eq!(from_wide_n(&buf, 1), "");
    }

    #[test]
    fn from_wide_n_strips_exactly_one_terminator() {
        let buf = [104u16, 105, 0, 88];
        assert_eq!(from_wide_n(&buf, 3), "hi");
    }

    #[test]
    fn from_wide_n_at_buffer_length_boundary_does_not_panic() {
        let buf = [65u16, 66, 67, 0];
        // n == buf.len() is the largest in-contract value: reads buf[..3].
        assert_eq!(from_wide_n(&buf, buf.len() as i32), "ABC");
        // n == buf.len() + 1 still lands on the `..len` upper edge (len == buf.len()).
        assert_eq!(from_wide_n(&buf, buf.len() as i32 + 1), "ABC\u{0}");
    }

    #[test]
    fn from_wide_n_empty_buffer_is_safe_for_n_up_to_one() {
        let buf: [u16; 0] = [];
        assert_eq!(from_wide_n(&buf, 0), "");
        assert_eq!(from_wide_n(&buf, 1), "");
    }

    #[test]
    fn from_wide_n_lossily_decodes_unpaired_surrogates() {
        // A truncated/garbled NLS buffer must not panic — from_utf16_lossy substitutes.
        let buf = [0xD800u16, 65, 0];
        let s = from_wide_n(&buf, 3);
        assert_eq!(s, "\u{FFFD}A");
        assert_eq!(s.chars().count(), 2);
    }

    #[test]
    fn from_wide_n_round_trips_to_wide() {
        // encode == decode: to_wide's length includes the NUL, which is exactly
        // the `n` convention from_wide_n expects.
        for s in ["", "a", "héllo", "日本語", "👍", "𝄞x", "a\u{0}b", "1,234.56"] {
            let w = to_wide(s);
            assert_eq!(from_wide_n(&w, w.len() as i32), s, "round-trip failed for {s:?}");
        }
    }

    #[test]
    #[should_panic(expected = "out of range")]
    fn from_wide_n_n_past_buffer_end_panics_out_of_contract() {
        // Out of contract (`n` is documented as an NLS return value, which never
        // exceeds the buffer), but `n` is trusted with no bounds check. Pinned so
        // that adding a clamp is a visible, deliberate change.
        let buf = [65u16, 0];
        let _s = from_wide_n(&buf, 4);
    }

    #[test]
    #[should_panic(expected = "out of range")]
    fn from_wide_n_i32_max_panics_out_of_contract() {
        let buf = [65u16, 0];
        let _s = from_wide_n(&buf, i32::MAX);
    }

    // ─── fmt_buf ──────────────────────────────────────────────────────────────

    #[test]
    fn fmt_buf_treats_zero_and_negative_returns_as_failure() {
        for n in [0i32, -1, i32::MIN] {
            assert_eq!(fmt_buf(|_, _| n), None, "n = {n} must map to None");
        }
    }

    #[test]
    fn fmt_buf_offers_a_256_unit_zeroed_buffer() {
        let seen_len = Cell::new(0i32);
        let first = Cell::new(1u16);
        let out = fmt_buf(|buf, len| {
            seen_len.set(len);
            first.set(unsafe { *buf });
            1
        });
        assert_eq!(seen_len.get(), 256);
        assert_eq!(first.get(), 0, "buffer must be zeroed before the callee writes");
        assert_eq!(out.as_deref(), Some(""));
    }

    #[test]
    fn fmt_buf_returns_what_the_callee_wrote() {
        let out = fmt_buf(|buf, len| unsafe { write_nls_buf(buf, len, "1,234.56") });
        assert_eq!(out.as_deref(), Some("1,234.56"));
    }

    #[test]
    fn fmt_buf_handles_unicode_and_empty_callee_output() {
        assert_eq!(
            fmt_buf(|buf, len| unsafe { write_nls_buf(buf, len, "١٢٣٤") }).as_deref(),
            Some("١٢٣٤")
        );
        assert_eq!(
            fmt_buf(|buf, len| unsafe { write_nls_buf(buf, len, "") }).as_deref(),
            Some("")
        );
    }

    #[test]
    fn fmt_buf_at_full_buffer_return_does_not_panic() {
        // n == buffer length is the largest legal NLS return for a 256-unit buffer.
        let out = fmt_buf(|_, len| len).expect("full-buffer return must not be treated as failure");
        assert_eq!(out.chars().count(), 255);
    }

    #[test]
    #[should_panic(expected = "out of range")]
    fn fmt_buf_absurd_callee_return_panics_out_of_contract() {
        // The FFI return value is trusted verbatim: a hostile/buggy callee that
        // over-reports gets an out-of-bounds slice rather than a rejected value.
        let _s = fmt_buf(|_, _| 9999);
    }

    // ─── conjunction tables ───────────────────────────────────────────────────

    const KNOWN_LANGS: &[&str] = &[
        "de", "fr", "es", "it", "pt", "nl", "ru", "uk", "be", "pl", "cs", "sk", "sr", "hr", "bs",
        "sl", "ro", "hu", "fi", "et", "lv", "lt", "sv", "da", "no", "nb", "nn", "tr", "ar", "he",
        "ja", "zh", "ko", "th",
    ];

    #[test]
    fn conjunctions_are_never_empty_and_and_never_equals_or() {
        for lang in KNOWN_LANGS.iter().chain(["", "en", "xx", "klingon"].iter()) {
            let and = conjunction_and(lang);
            let or = conjunction_or(lang);
            assert!(!and.is_empty(), "empty 'and' for {lang:?}");
            assert!(!or.is_empty(), "empty 'or' for {lang:?}");
            assert_ne!(and, or, "'and' and 'or' collide for {lang:?}");
        }
    }

    #[test]
    fn conjunctions_fall_back_to_english_for_unknown_input() {
        for lang in ["", "xx", "klingon", "  ", "🙂", "\u{0}"] {
            assert_eq!(conjunction_and(lang), "and", "and({lang:?})");
            assert_eq!(conjunction_or(lang), "or", "or({lang:?})");
        }
    }

    #[test]
    fn conjunctions_hit_the_table_for_known_languages() {
        assert_eq!(conjunction_and("de"), "und");
        assert_eq!(conjunction_or("de"), "oder");
        assert_eq!(conjunction_and("ja"), "と");
        assert_eq!(conjunction_or("zh"), "或");
        // All three Norwegian tags share one arm.
        for no in ["no", "nb", "nn"] {
            assert_eq!(conjunction_and(no), "og");
            assert_eq!(conjunction_or(no), "eller");
        }
    }

    #[test]
    fn conjunction_lookup_is_case_sensitive_and_wants_a_bare_subtag() {
        // Pinning current behaviour: the table is keyed by lowercase bare subtags,
        // so an uppercase tag or a full locale silently falls back to English.
        assert_eq!(conjunction_and("DE"), "and");
        assert_eq!(conjunction_and("De"), "and");
        assert_eq!(conjunction_and("de-DE"), "and");
        assert_eq!(conjunction_or("de_DE"), "or");
    }

    // ─── join_list ────────────────────────────────────────────────────────────

    #[test]
    fn join_list_empty_and_single_ignore_the_conjunction() {
        assert_eq!(join_list(&[], "and"), "");
        assert_eq!(join_list(&azv(&["solo"]), "and"), "solo");
        assert_eq!(join_list(&azv(&["solo"]), ""), "solo");
    }

    #[test]
    fn join_list_two_items_have_no_comma() {
        assert_eq!(join_list(&azv(&["a", "b"]), "and"), "a and b");
        assert_eq!(join_list(&azv(&["a", "b"]), "und"), "a und b");
    }

    #[test]
    fn join_list_three_or_more_use_an_oxford_comma() {
        assert_eq!(join_list(&azv(&["a", "b", "c"]), "and"), "a, b, and c");
        assert_eq!(join_list(&azv(&["a", "b", "c", "d"]), "or"), "a, b, c, or d");
    }

    #[test]
    fn join_list_does_not_escape_separators_inside_items() {
        // Items containing the separator round-trip ambiguously; pinned so the
        // behaviour is at least deterministic.
        assert_eq!(join_list(&azv(&["a, b", "c"]), "and"), "a, b and c");
        assert_eq!(join_list(&azv(&["", ""]), "and"), " and ");
        assert_eq!(join_list(&azv(&["", "", ""]), "and"), ", , and ");
    }

    #[test]
    fn join_list_empty_conjunction_leaves_a_double_space() {
        assert_eq!(join_list(&azv(&["a", "b"]), ""), "a  b");
    }

    #[test]
    fn join_list_handles_a_large_list_without_panicking() {
        let items: Vec<AzString> = (0..1000).map(|i| AzString::from(i.to_string())).collect();
        let joined = join_list(&items, "and");
        assert!(joined.starts_with("0, 1, 2,"));
        assert!(joined.ends_with(", and 999"));
    }

    #[test]
    fn join_list_preserves_unicode_items() {
        assert_eq!(join_list(&azv(&["日本", "中国"]), "と"), "日本 と 中国");
    }

    // ─── compare_nls ──────────────────────────────────────────────────────────

    #[test]
    fn compare_nls_is_reflexive_and_antisymmetric() {
        let Some(f) = nls() else { return };
        let loc = to_wide("en-US");
        let cases = [
            ("a", "a"),
            ("a", "b"),
            ("", ""),
            ("", "a"),
            ("ä", "a"),
            ("日", "本"),
            ("a\u{0}b", "a"),
            ("👍", "👎"),
        ];
        for (a, b) in cases {
            assert_eq!(
                compare_nls(f, &loc, a, a),
                Ordering::Equal,
                "not reflexive for {a:?}"
            );
            let ab = compare_nls(f, &loc, a, b);
            let ba = compare_nls(f, &loc, b, a);
            assert_eq!(ab, ba.reverse(), "not antisymmetric for {a:?} vs {b:?}");
        }
    }

    #[test]
    fn compare_nls_stays_a_total_order_on_an_invalid_locale() {
        // An unusable locale name makes CompareStringEx return 0; the lexicographic
        // fallback must still be reflexive and antisymmetric (a sort depends on it).
        let Some(f) = nls() else { return };
        for locale in ["not-a-locale-☃", "", "\u{0}"] {
            let loc = to_wide(locale);
            assert_eq!(compare_nls(f, &loc, "x", "x"), Ordering::Equal);
            let ab = compare_nls(f, &loc, "a", "b");
            let ba = compare_nls(f, &loc, "b", "a");
            assert_eq!(ab, ba.reverse(), "not antisymmetric under locale {locale:?}");
        }
    }

    // ─── IcuLocalizer: construction & getters ────────────────────────────────

    #[test]
    fn new_keeps_the_locale_verbatim_and_caches_a_coherent_wide_form() {
        for s in ["en-US", "", "de_DE", "zh-Hans-CN", "日本語", "x".repeat(10_000).as_str()] {
            let l = IcuLocalizer::new(s);
            assert_eq!(l.get_locale().as_str(), s);
            assert_locale_cache_coherent(&l);
        }
    }

    #[test]
    fn new_with_interior_nul_keeps_the_nul_in_the_wide_cache() {
        // Everything after the NUL is invisible to every NLS call this type makes.
        let l = IcuLocalizer::new("en\u{0}-US");
        assert_locale_cache_coherent(&l);
        assert_eq!(l.locale_wide[2], 0, "interior NUL truncates the locale for Win32");
    }

    #[test]
    fn from_system_language_matches_new() {
        for s in ["fr-FR", "", "🙂"] {
            let a = IcuLocalizer::from_system_language(&AzString::from(s));
            let b = IcuLocalizer::new(s);
            assert_eq!(a.get_locale().as_str(), b.get_locale().as_str());
            assert_eq!(a.locale_wide, b.locale_wide);
        }
    }

    #[test]
    fn default_is_en_us() {
        let l = IcuLocalizer::default();
        assert_eq!(l.get_locale().as_str(), "en-US");
        assert_eq!(l.get_language().as_str(), "en");
        assert_eq!(l.get_region().map(|r| r.as_str().to_string()), Some("US".to_string()));
        assert_locale_cache_coherent(&l);
    }

    #[test]
    fn get_language_splits_on_dash_and_underscore() {
        assert_eq!(IcuLocalizer::new("en-US").get_language().as_str(), "en");
        assert_eq!(IcuLocalizer::new("en_US").get_language().as_str(), "en");
        assert_eq!(IcuLocalizer::new("en").get_language().as_str(), "en");
        assert_eq!(IcuLocalizer::new("zh-Hans-CN").get_language().as_str(), "zh");
    }

    #[test]
    fn get_language_on_an_empty_locale_is_empty_not_en() {
        // `str::split` always yields at least one item, so `lang()`'s `unwrap_or("en")`
        // can never fire: an empty locale degrades to an empty language, not English.
        let l = IcuLocalizer::new("");
        assert_eq!(l.get_language().as_str(), "");
        assert_eq!(l.lang(), "");
    }

    #[test]
    fn get_language_and_lang_agree() {
        for s in ["en-US", "de_DE", "", "zh-Hans-CN", "-", "_x"] {
            let l = IcuLocalizer::new(s);
            assert_eq!(l.get_language().as_str(), l.lang(), "disagreement for {s:?}");
        }
    }

    #[test]
    fn get_region_only_splits_on_dash_and_returns_the_second_subtag() {
        assert_eq!(
            IcuLocalizer::new("en-US").get_region().map(|r| r.as_str().to_string()),
            Some("US".to_string())
        );
        assert_eq!(IcuLocalizer::new("en").get_region(), None);
        assert_eq!(IcuLocalizer::new("").get_region(), None);
        // Pinning two known deviations from BCP-47:
        // 1. an underscore-separated locale reports no region at all,
        assert_eq!(IcuLocalizer::new("en_US").get_region(), None);
        // 2. and for a script-bearing tag the *script* is returned as the region.
        assert_eq!(
            IcuLocalizer::new("zh-Hans-CN").get_region().map(|r| r.as_str().to_string()),
            Some("Hans".to_string())
        );
    }

    #[test]
    fn set_locale_reports_success_and_keeps_the_cache_coherent() {
        let mut l = IcuLocalizer::new("en-US");
        for s in ["de-DE", "", "ja", "x".repeat(5_000).as_str()] {
            assert!(l.set_locale(s), "set_locale must report success");
            assert_eq!(l.get_locale().as_str(), s);
            assert_locale_cache_coherent(&l);
        }
    }

    #[test]
    fn load_data_blob_is_a_no_op() {
        let mut l = IcuLocalizer::new("fr-FR");
        for blob in [Vec::new(), vec![0u8; 1], vec![0xFFu8; 100_000]] {
            l.load_data_blob(blob);
            assert_eq!(l.get_locale().as_str(), "fr-FR");
            assert_locale_cache_coherent(&l);
        }
    }

    // ─── number formatting ────────────────────────────────────────────────────

    #[test]
    fn format_integer_survives_the_i64_extremes_and_keeps_every_digit() {
        let mut l = IcuLocalizer::new("en-US");
        for v in [0i64, 1, -1, 7, -7, 1_234_567, i64::MAX, i64::MIN, i64::MIN + 1] {
            let out = l.format_integer(v);
            let out = out.as_str();
            assert!(!out.is_empty(), "empty output for {v}");
            // Grouping separators and a locale-default fraction may be added, but the
            // significant digits must survive in order (this also holds on the
            // non-NLS fallback path, where the output is just `v.to_string()`).
            assert!(
                digits(out).starts_with(&digits(&v.to_string())),
                "digits of {v} were lost or reordered: {out:?}"
            );
            if v < 0 {
                assert!(
                    out.contains('-') || out.contains('('),
                    "sign lost for {v}: {out:?}"
                );
            }
        }
    }

    #[test]
    fn format_integer_is_deterministic() {
        let mut l = IcuLocalizer::new("en-US");
        let a = l.format_integer(i64::MIN);
        let b = l.format_integer(i64::MIN);
        assert_eq!(a.as_str(), b.as_str());
    }

    #[test]
    fn format_integer_does_not_panic_on_a_garbage_locale() {
        let mut l = IcuLocalizer::new("☃\u{0}not-a-locale");
        let out = l.format_integer(42);
        assert!(digits(out.as_str()).starts_with('4'), "got {:?}", out.as_str());
    }

    #[test]
    fn format_decimal_with_zero_places_equals_format_integer() {
        // decimal_string(v, 0) == v.to_string(), so both paths hand NLS the same
        // input and must agree — true regardless of what the locale does to it.
        let mut l = IcuLocalizer::new("en-US");
        for v in [0i64, 1234, -1234, i64::MAX, i64::MIN] {
            let dec = l.format_decimal(v, 0);
            let int = l.format_integer(v);
            assert_eq!(dec.as_str(), int.as_str(), "disagreement for {v}");
        }
    }

    #[test]
    fn format_decimal_with_negative_places_scales_up() {
        // decimal_string(5, -3) == "5000", so this must equal format_integer(5000).
        let mut l = IcuLocalizer::new("en-US");
        let scaled = l.format_decimal(5, -3);
        let direct = l.format_integer(5000);
        assert_eq!(scaled.as_str(), direct.as_str());
    }

    #[test]
    fn format_decimal_keeps_digits_for_ordinary_inputs() {
        let mut l = IcuLocalizer::new("en-US");
        for (v, dp) in [(12345i64, 2i16), (0, 2), (-12345, 2), (1, 1), (i64::MIN, 2)] {
            let out = l.format_decimal(v, dp);
            let out = out.as_str();
            assert!(!out.is_empty(), "empty output for ({v}, {dp})");
            assert!(
                digits(out).starts_with(&digits(&decimal_string(v, dp))),
                "digits of ({v}, {dp}) lost: {out:?}"
            );
        }
    }

    #[test]
    fn format_decimal_at_min_plus_one_places_builds_a_huge_but_intact_number() {
        // -32767 is the most negative *safe* scale: decimal_string appends 32_767
        // zeros. The result is far too long for the 256-unit NLS buffer, so the raw
        // string is handed back — every digit must still be there.
        let mut l = IcuLocalizer::new("en-US");
        let out = l.format_decimal(5, i16::MIN + 1);
        let d = digits(out.as_str());
        assert!(d.starts_with('5'), "leading digit lost");
        assert_eq!(d.len(), 32_768, "expected 5 followed by 32_767 zeros");
    }

    #[test]
    fn format_decimal_at_max_places_does_not_panic() {
        let mut l = IcuLocalizer::new("en-US");
        let out = l.format_decimal(1, i16::MAX);
        assert!(!out.as_str().is_empty());
    }

    // `decimal_string` negates `decimal_places` (`0..(-decimal_places as usize)`),
    // which overflows for i16::MIN. Debug-only: with overflow checks off this
    // instead wraps to a ~1.8e19 iteration count that pushes '0' until the process
    // is OOM-killed, so the test must not exist in release/coverage builds.
    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "overflow")]
    fn format_decimal_at_i16_min_places_overflows() {
        let mut l = IcuLocalizer::new("en-US");
        let _s = l.format_decimal(5, i16::MIN);
    }

    // ─── plural rules ─────────────────────────────────────────────────────────

    #[test]
    fn plural_english_only_treats_exactly_one_as_one() {
        let mut l = IcuLocalizer::new("en-US");
        for v in [0i64, 2, -1, -2, 100, i64::MAX, i64::MIN] {
            assert_eq!(
                l.get_plural_category(v),
                PluralCategory::Other,
                "unexpected category for {v}"
            );
        }
        assert_eq!(l.get_plural_category(1), PluralCategory::One);
    }

    #[test]
    fn plural_uses_the_language_subtag_of_a_full_locale() {
        let mut l = IcuLocalizer::new("ru-RU");
        assert_eq!(l.get_plural_category(1), PluralCategory::One);
        assert_eq!(l.get_plural_category(2), PluralCategory::Few);
        assert_eq!(l.get_plural_category(5), PluralCategory::Many);
        assert_eq!(l.get_plural_category(11), PluralCategory::Many);
        assert_eq!(l.get_plural_category(12), PluralCategory::Many);
        assert_eq!(l.get_plural_category(21), PluralCategory::One);
    }

    #[test]
    fn plural_arabic_covers_all_six_categories() {
        let mut l = IcuLocalizer::new("ar");
        assert_eq!(l.get_plural_category(0), PluralCategory::Zero);
        assert_eq!(l.get_plural_category(1), PluralCategory::One);
        assert_eq!(l.get_plural_category(2), PluralCategory::Two);
        assert_eq!(l.get_plural_category(3), PluralCategory::Few);
        assert_eq!(l.get_plural_category(11), PluralCategory::Many);
        assert_eq!(l.get_plural_category(100), PluralCategory::Other);
    }

    #[test]
    fn plural_negative_values_are_deterministic() {
        let mut ru = IcuLocalizer::new("ru");
        // |−1| % 10 == 1 and |−1| % 100 != 11 → One, same as +1.
        assert_eq!(ru.get_plural_category(-1), PluralCategory::One);
        assert_eq!(ru.get_plural_category(-2), PluralCategory::Few);
        assert_eq!(ru.get_plural_category(-11), PluralCategory::Many);

        let mut fr = IcuLocalizer::new("fr");
        assert_eq!(fr.get_plural_category(0), PluralCategory::One);
        assert_eq!(fr.get_plural_category(-1), PluralCategory::Other);
    }

    #[test]
    fn plural_at_the_safe_end_of_the_i64_range() {
        // i64::MIN + 1 is the most negative value the `.abs()`-based rules can take.
        // |i64::MIN + 1| == i64::MAX, which ends in 7 → Many (ru) / Few (ar).
        let mut ru = IcuLocalizer::new("ru");
        assert_eq!(ru.get_plural_category(i64::MIN + 1), PluralCategory::Many);
        assert_eq!(ru.get_plural_category(i64::MAX), PluralCategory::Many);

        let mut ar = IcuLocalizer::new("ar");
        assert_eq!(ar.get_plural_category(i64::MIN + 1), PluralCategory::Few);
        assert_eq!(ar.get_plural_category(i64::MAX), PluralCategory::Few);
    }

    // The CLDR rules for ar/ru/pl/sl/lt/lv/ro/mt compute `n.abs()`, which panics on
    // i64::MIN. Debug-only: with overflow checks off `abs()` wraps to i64::MIN and
    // the category is merely wrong, so no panic is available to assert on.
    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "overflow")]
    fn plural_i64_min_overflows_for_abs_based_languages() {
        let mut l = IcuLocalizer::new("ru-RU");
        let _c = l.get_plural_category(i64::MIN);
    }

    #[test]
    fn pluralize_substitutes_every_placeholder() {
        let mut l = IcuLocalizer::new("en-US");
        let out = l.pluralize(3, "z", "o", "t", "f", "m", "{} of {} items");
        assert_eq!(out.as_str(), "3 of 3 items");
    }

    #[test]
    fn pluralize_english_ignores_the_zero_arm() {
        // English has no CLDR `zero` category, so the `zero` argument is dead.
        let mut l = IcuLocalizer::new("en-US");
        assert_eq!(
            l.pluralize(0, "ZERO", "ONE", "TWO", "FEW", "MANY", "OTHER").as_str(),
            "OTHER"
        );
        assert_eq!(
            l.pluralize(1, "ZERO", "ONE", "TWO", "FEW", "MANY", "OTHER").as_str(),
            "ONE"
        );
    }

    #[test]
    fn pluralize_arabic_reaches_the_zero_arm() {
        let mut l = IcuLocalizer::new("ar");
        assert_eq!(
            l.pluralize(0, "ZERO", "ONE", "TWO", "FEW", "MANY", "OTHER").as_str(),
            "ZERO"
        );
        assert_eq!(
            l.pluralize(2, "ZERO", "ONE", "TWO", "FEW", "MANY", "OTHER").as_str(),
            "TWO"
        );
    }

    #[test]
    fn pluralize_handles_extremes_and_empty_templates() {
        let mut l = IcuLocalizer::new("en-US");
        assert_eq!(l.pluralize(i64::MIN, "", "", "", "", "", "{}").as_str(), i64::MIN.to_string());
        assert_eq!(l.pluralize(i64::MAX, "", "", "", "", "", "{}").as_str(), i64::MAX.to_string());
        assert_eq!(l.pluralize(5, "", "", "", "", "", "").as_str(), "");
        // A template with no placeholder is passed through untouched.
        assert_eq!(l.pluralize(5, "", "", "", "", "", "no slot").as_str(), "no slot");
    }

    // ─── list formatting ──────────────────────────────────────────────────────

    #[test]
    fn format_list_empty_is_empty_for_every_type() {
        let mut l = IcuLocalizer::new("en-US");
        for t in [ListType::And, ListType::Or, ListType::Unit] {
            assert_eq!(l.format_list(&[], t).as_str(), "", "{t:?}");
        }
    }

    #[test]
    fn format_list_single_item_never_gains_a_conjunction() {
        let mut l = IcuLocalizer::new("de-DE");
        for t in [ListType::And, ListType::Or, ListType::Unit] {
            assert_eq!(l.format_list(&azv(&["solo"]), t).as_str(), "solo", "{t:?}");
        }
    }

    #[test]
    fn format_list_picks_the_conjunction_from_the_language_subtag() {
        let mut l = IcuLocalizer::new("de-DE");
        let items = azv(&["a", "b", "c"]);
        assert_eq!(l.format_list(&items, ListType::And).as_str(), "a, b, und c");
        assert_eq!(l.format_list(&items, ListType::Or).as_str(), "a, b, oder c");
        // Unit never uses a conjunction.
        assert_eq!(l.format_list(&items, ListType::Unit).as_str(), "a, b, c");
    }

    #[test]
    fn format_list_falls_back_to_english_for_an_unknown_language() {
        let mut l = IcuLocalizer::new("klingon-KL");
        assert_eq!(
            l.format_list(&azv(&["a", "b"]), ListType::And).as_str(),
            "a and b"
        );
    }

    #[test]
    fn format_list_handles_a_large_list() {
        let mut l = IcuLocalizer::new("en-US");
        let items: Vec<AzString> = (0..500).map(|i| AzString::from(i.to_string())).collect();
        let out = l.format_list(&items, ListType::Or);
        assert!(out.as_str().ends_with(", or 499"));
    }

    // ─── date / time formatting ───────────────────────────────────────────────

    #[test]
    fn format_date_rejects_out_of_range_fields_without_panicking() {
        let mut l = IcuLocalizer::new("en-US");
        let bad = [
            IcuDate::new(2025, 0, 15),   // month 0
            IcuDate::new(2025, 13, 15),  // month 13
            IcuDate::new(2025, 255, 15), // month 255
            IcuDate::new(2025, 1, 0),    // day 0
            IcuDate::new(2025, 1, 32),   // day 32
            IcuDate::new(2025, 1, 255),  // day 255
            IcuDate::new(2025, 2, 30),   // 30 February
        ];
        for d in bad {
            let r = l.format_date(d, FormatLength::Short);
            assert!(
                err_text(&r).is_some(),
                "invalid date {d:?} must return Err, got {:?}",
                ok_text(&r)
            );
        }
    }

    #[test]
    fn format_date_clamps_extreme_years_into_the_systemtime_range() {
        // year is clamped to 1601..=30827, so even i32::MIN/MAX stay formattable.
        if nls().is_none() {
            return;
        }
        let mut l = IcuLocalizer::new("en-US");
        for year in [i32::MIN, 0, 1, 1600, 30_828, i32::MAX] {
            let r = l.format_date(IcuDate::new(year, 1, 15), FormatLength::Short);
            assert!(
                ok_text(&r).is_some_and(|s| !s.is_empty()),
                "clamped year {year} should still format, got {:?}",
                err_text(&r)
            );
        }
    }

    #[test]
    fn format_date_medium_is_indistinguishable_from_short() {
        // Both map to DATE_SHORTDATE — Windows exposes no medium date style.
        if nls().is_none() {
            return;
        }
        let mut l = IcuLocalizer::new("en-US");
        let d = IcuDate::new(2025, 1, 15);
        let short = ok_text(&l.format_date(d, FormatLength::Short)).expect("short");
        let medium = ok_text(&l.format_date(d, FormatLength::Medium)).expect("medium");
        let long = ok_text(&l.format_date(d, FormatLength::Long)).expect("long");
        assert_eq!(short, medium);
        assert_ne!(short, long, "Long must use DATE_LONGDATE");
    }

    #[test]
    fn format_time_rejects_out_of_range_fields_without_panicking() {
        let mut l = IcuLocalizer::new("en-US");
        let bad = [
            IcuTime { hour: 24, minute: 0, second: 0 },
            IcuTime { hour: 255, minute: 0, second: 0 },
            IcuTime { hour: 0, minute: 99, second: 0 },
            IcuTime { hour: 0, minute: 255, second: 0 },
            IcuTime { hour: 0, minute: 0, second: 99 },
            IcuTime { hour: 0, minute: 0, second: 255 },
        ];
        for t in bad {
            let r = l.format_time(t, true);
            assert!(
                err_text(&r).is_some(),
                "invalid time {t:?} must return Err, got {:?}",
                ok_text(&r)
            );
        }
    }

    #[test]
    fn format_time_boundaries_are_accepted() {
        if nls().is_none() {
            return;
        }
        let mut l = IcuLocalizer::new("en-US");
        for t in [
            IcuTime { hour: 0, minute: 0, second: 0 },
            IcuTime { hour: 23, minute: 59, second: 59 },
        ] {
            assert!(
                ok_text(&l.format_time(t, true)).is_some_and(|s| !s.is_empty()),
                "{t:?} should format"
            );
        }
    }

    #[test]
    fn format_time_honours_the_include_seconds_flag() {
        if nls().is_none() {
            return;
        }
        let mut l = IcuLocalizer::new("en-US");
        let t = IcuTime { hour: 12, minute: 30, second: 45 };
        let with = ok_text(&l.format_time(t, true)).expect("with seconds");
        let without = ok_text(&l.format_time(t, false)).expect("without seconds");
        assert!(with.contains("45"), "seconds missing from {with:?}");
        assert!(!without.contains("45"), "seconds leaked into {without:?}");
        assert!(without.contains("30"), "minutes missing from {without:?}");
    }

    #[test]
    fn format_datetime_is_exactly_date_space_time() {
        if nls().is_none() {
            return;
        }
        let mut l = IcuLocalizer::new("en-US");
        let date = IcuDate::new(2025, 1, 15);
        let time = IcuTime { hour: 12, minute: 30, second: 45 };
        let dt = IcuDateTime { date, time };
        let combined = ok_text(&l.format_datetime(dt, FormatLength::Long)).expect("datetime");
        let d = ok_text(&l.format_date(date, FormatLength::Long)).expect("date");
        let t = ok_text(&l.format_time(time, true)).expect("time");
        assert_eq!(combined, alloc::format!("{d} {t}"));
    }

    #[test]
    fn format_datetime_propagates_the_failing_half() {
        if nls().is_none() {
            return;
        }
        let mut l = IcuLocalizer::new("en-US");
        let good_date = IcuDate::new(2025, 1, 15);
        let good_time = IcuTime { hour: 1, minute: 2, second: 3 };

        let bad_date = IcuDateTime {
            date: IcuDate::new(2025, 13, 1),
            time: good_time,
        };
        assert_eq!(
            err_text(&l.format_datetime(bad_date, FormatLength::Short)).as_deref(),
            Some("GetDateFormatEx failed")
        );

        let bad_time = IcuDateTime {
            date: good_date,
            time: IcuTime { hour: 99, minute: 0, second: 0 },
        };
        // Must not return a half-formatted date.
        assert_eq!(
            err_text(&l.format_datetime(bad_time, FormatLength::Short)).as_deref(),
            Some("GetTimeFormatEx failed")
        );
    }

    // ─── collation ────────────────────────────────────────────────────────────

    #[test]
    fn compare_is_reflexive_antisymmetric_and_deterministic() {
        let mut l = IcuLocalizer::new("en-US");
        let cases = [
            ("", ""),
            ("a", "a"),
            ("a", "b"),
            ("", "a"),
            ("ä", "z"),
            ("日本", "日本語"),
            ("👍", "👍"),
        ];
        for (a, b) in cases {
            assert_eq!(l.compare(a, a), Ordering::Equal, "not reflexive: {a:?}");
            let ab = l.compare(a, b);
            assert_eq!(ab, l.compare(a, b), "not deterministic: {a:?} vs {b:?}");
            assert_eq!(
                ab,
                l.compare(b, a).reverse(),
                "not antisymmetric: {a:?} vs {b:?}"
            );
        }
    }

    #[test]
    fn strings_equal_agrees_with_compare() {
        let mut l = IcuLocalizer::new("en-US");
        for (a, b) in [("a", "a"), ("a", "b"), ("", ""), ("x", ""), ("é", "é")] {
            assert_eq!(
                l.strings_equal(a, b),
                l.compare(a, b) == Ordering::Equal,
                "{a:?} vs {b:?}"
            );
        }
    }

    #[test]
    fn sort_strings_is_a_permutation_and_is_idempotent() {
        let mut l = IcuLocalizer::new("en-US");
        let input = azv(&["pear", "Apple", "banana", "", "apple", "éclair", "日本", "banana"]);

        let mut once = input.clone();
        l.sort_strings(&mut once);

        let mut before = strs(&input);
        let mut after = strs(&once);
        before.sort();
        after.sort();
        assert_eq!(before, after, "sorting must not lose or invent elements");

        let mut twice = once.clone();
        l.sort_strings(&mut twice);
        assert_eq!(strs(&once), strs(&twice), "sort must be idempotent");
    }

    #[test]
    fn sort_strings_handles_degenerate_slices() {
        let mut l = IcuLocalizer::new("en-US");
        l.sort_strings(&mut []);

        let mut one = azv(&["only"]);
        l.sort_strings(&mut one);
        assert_eq!(strs(&one), vec!["only".to_string()]);

        let mut dupes = azv(&["x", "x", "x"]);
        l.sort_strings(&mut dupes);
        assert_eq!(strs(&dupes), vec!["x".to_string(); 3]);
    }

    #[test]
    fn sort_strings_survives_a_garbage_locale() {
        // CompareStringEx rejects this locale name and returns 0, so every
        // comparison takes the lexicographic fallback. That fallback is a total
        // order, so the result must be fully sorted — not merely unpanicked.
        let mut l = IcuLocalizer::new("☃-not-a-locale");
        let mut v = azv(&["c", "a", "b", "a"]);
        l.sort_strings(&mut v);
        let out = strs(&v);
        assert!(
            out.windows(2).all(|w| w[0] <= w[1]),
            "fallback ordering is not sorted: {out:?}"
        );
        assert_eq!(out.len(), 4, "sorting must not lose elements");
    }

    #[test]
    fn sorted_strings_leaves_the_input_untouched() {
        let mut l = IcuLocalizer::new("en-US");
        let input = azv(&["c", "a", "b"]);
        let out = l.sorted_strings(&input);
        assert_eq!(strs(&input), vec!["c".to_string(), "a".to_string(), "b".to_string()]);

        let mut in_place = input.clone();
        l.sort_strings(&mut in_place);
        assert_eq!(strs(&out), strs(&in_place));
    }

    #[test]
    fn sorted_strings_of_empty_is_empty() {
        let mut l = IcuLocalizer::new("en-US");
        assert!(l.sorted_strings(&[]).is_empty());
    }

    // ─── sort keys ────────────────────────────────────────────────────────────

    #[test]
    fn get_sort_key_round_trips_through_utf8() {
        let mut l = IcuLocalizer::new("en-US");
        for s in ["", "a", "héllo", "日本語", "👍", "a\u{0}b"] {
            let key = l.get_sort_key(s);
            assert_eq!(key, s.as_bytes());
            assert_eq!(String::from_utf8(key).expect("valid UTF-8"), s);
        }
    }

    #[test]
    fn get_sort_key_orders_by_bytes_not_by_collation() {
        // Documented shortcut: the key is raw UTF-8, so its order is *not* the order
        // `compare` produces. Anything caching by sort key inherits ASCII ordering.
        let mut l = IcuLocalizer::new("en-US");
        assert!(
            l.get_sort_key("a") > l.get_sort_key("B"),
            "sort keys are byte-ordered ('a' = 0x61 > 'B' = 0x42)"
        );
    }
}
