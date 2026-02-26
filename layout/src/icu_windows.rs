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

use super::{FormatLength, IcuDate, IcuDateTime, IcuResult, IcuTime, ListType, PluralCategory};

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
    String::from_utf16_lossy(&buf[..len]).into_owned()
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

// CompareStringEx return values
const CSTR_LESS_THAN:    i32 = 1;
const CSTR_EQUAL:        i32 = 2;
// CSTR_GREATER_THAN == 3

// ─── CLDR plural rules ────────────────────────────────────────────────────────
//
// Identical to the table in icu_macos.rs — covers major plural-rule groups
// without bundling any data file.

fn plural_for(n: i64, lang: &str) -> PluralCategory {
    let lang = lang.split(['-', '_']).next().unwrap_or(lang);
    match lang {
        "ar" | "arz" | "ckb" => {
            let n100 = n.abs() % 100;
            if n == 0 { PluralCategory::Zero }
            else if n == 1 { PluralCategory::One }
            else if n == 2 { PluralCategory::Two }
            else if (3..=10).contains(&n100) { PluralCategory::Few }
            else if (11..=99).contains(&n100) { PluralCategory::Many }
            else { PluralCategory::Other }
        }
        "cy" => match n {
            0 => PluralCategory::Zero,
            1 => PluralCategory::One,
            2 => PluralCategory::Two,
            3 => PluralCategory::Few,
            6 => PluralCategory::Many,
            _ => PluralCategory::Other,
        },
        "ru" | "uk" | "be" | "sr" | "hr" | "bs" | "sh" => {
            let n10  = n.abs() % 10;
            let n100 = n.abs() % 100;
            if n10 == 1 && n100 != 11 { PluralCategory::One }
            else if (2..=4).contains(&n10) && !(12..=14).contains(&n100) { PluralCategory::Few }
            else { PluralCategory::Many }
        }
        "pl" => {
            let n10  = n.abs() % 10;
            let n100 = n.abs() % 100;
            if n == 1 { PluralCategory::One }
            else if (2..=4).contains(&n10) && !(12..=14).contains(&n100) { PluralCategory::Few }
            else { PluralCategory::Many }
        }
        "cs" | "sk" => {
            if n == 1 { PluralCategory::One }
            else if (2..=4).contains(&n) { PluralCategory::Few }
            else { PluralCategory::Other }
        }
        "sl" => {
            let n100 = n.abs() % 100;
            if n100 == 1 { PluralCategory::One }
            else if n100 == 2 { PluralCategory::Two }
            else if (3..=4).contains(&n100) { PluralCategory::Few }
            else { PluralCategory::Other }
        }
        "lt" => {
            let n10  = n.abs() % 10;
            let n100 = n.abs() % 100;
            if n10 == 1 && !(11..=19).contains(&n100) { PluralCategory::One }
            else if (2..=9).contains(&n10) && !(11..=19).contains(&n100) { PluralCategory::Few }
            else { PluralCategory::Other }
        }
        "lv" => {
            let n10  = n.abs() % 10;
            let n100 = n.abs() % 100;
            if n == 0 { PluralCategory::Zero }
            else if n10 == 1 && n100 != 11 { PluralCategory::One }
            else { PluralCategory::Other }
        }
        "ro" | "mo" => {
            let n100 = n.abs() % 100;
            if n == 1 { PluralCategory::One }
            else if n == 0 || (1..=19).contains(&n100) { PluralCategory::Few }
            else { PluralCategory::Other }
        }
        "mt" => {
            let n100 = n.abs() % 100;
            if n == 1 { PluralCategory::One }
            else if n == 0 || (2..=10).contains(&n100) { PluralCategory::Few }
            else if (11..=19).contains(&n100) { PluralCategory::Many }
            else { PluralCategory::Other }
        }
        "he" | "yi" | "iw" => {
            if n == 1 { PluralCategory::One }
            else if n == 2 { PluralCategory::Two }
            else if n != 0 && n % 10 == 0 { PluralCategory::Many }
            else { PluralCategory::Other }
        }
        "ga" => match n {
            1 => PluralCategory::One,
            2 => PluralCategory::Two,
            3..=6 => PluralCategory::Few,
            7..=10 => PluralCategory::Many,
            _ => PluralCategory::Other,
        },
        "fr" | "ff" | "kab" => {
            if n <= 1 { PluralCategory::One } else { PluralCategory::Other }
        }
        _ => if n == 1 { PluralCategory::One } else { PluralCategory::Other },
    }
}

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
        let Some(f) = nls() else {
            let dp = decimal_places.max(0) as usize;
            let v = integer_part as f64 * 10f64.powi(-(decimal_places as i32));
            return AzString::from(alloc::format!("{v:.dp$}"));
        };
        let dp = decimal_places.max(0) as usize;
        let v = integer_part as f64 * 10f64.powi(-(decimal_places as i32));
        // Build the numeric string with a period as decimal separator (required by NLS).
        let value_str = alloc::format!("{v:.dp$}");
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
        let a_w = to_wide(a);
        let b_w = to_wide(b);
        let locale_ptr = self.locale_wide.as_ptr();
        // Pass -1 to let NLS measure the null-terminated strings itself.
        let result = unsafe {
            (f.compare_string_ex)(
                locale_ptr, 0,
                a_w.as_ptr(), -1,
                b_w.as_ptr(), -1,
                core::ptr::null_mut(), core::ptr::null_mut(), 0,
            )
        };
        match result {
            CSTR_LESS_THAN => Ordering::Less,
            CSTR_EQUAL     => Ordering::Equal,
            _              => Ordering::Greater,
        }
    }

    pub fn sort_strings(&mut self, strings: &mut [AzString]) {
        // Clone the locale_wide to avoid borrow issues inside the closure.
        let locale_wide = self.locale_wide.clone();
        if let Some(f) = nls() {
            strings.sort_by(|a, b| {
                let aw = to_wide(a.as_str());
                let bw = to_wide(b.as_str());
                let r = unsafe {
                    (f.compare_string_ex)(
                        locale_wide.as_ptr(), 0,
                        aw.as_ptr(), -1, bw.as_ptr(), -1,
                        core::ptr::null_mut(), core::ptr::null_mut(), 0,
                    )
                };
                match r {
                    CSTR_LESS_THAN => Ordering::Less,
                    CSTR_EQUAL     => Ordering::Equal,
                    _              => Ordering::Greater,
                }
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
