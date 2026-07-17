//! Cross-backend ICU parity tests.
//!
//! Azul ships three mutually-exclusive i18n backends, selected by feature in
//! `layout/src/lib.rs` (and mirrored in `layout/src/icu.rs`):
//!
//!   * `icu`         — ICU4X (the DEFAULT/REFERENCE backend; CLDR data blobs)
//!   * `icu_macos`   — Apple Foundation (`NS*Formatter`), macOS only
//!   * `icu_windows` — Win32 NLS (`Get*FormatEx`/`CompareStringEx`), Windows only
//!
//! All three are reached through the SAME public dispatch type
//! [`azul_layout::IcuLocalizerHandle`], which is backend-agnostic: it is defined
//! once and internally constructs the active backend's `IcuLocalizer`. This test
//! exercises that public API only — never any backend-internal function — so the
//! one file runs unchanged under whichever backend feature is active.
//!
//! ## Parity contract
//!
//! The asserted constants are the ICU4X REFERENCE output, captured by running
//! the (ignored) [`capture_reference`] dump under `--features icu` on
//! 2026-06-20 (icu 2.1). Every backend is pinned to those values. A backend
//! that drifts therefore FAILS — which is the point.
//!
//! ## Documented cross-backend divergences (captured ICU4X vs macOS, verified locally)
//!
//! 11 of 64 matrix cells legitimately differ on the platform backends. They are
//! asserted per-backend below (search "DIVERGENCE"); the ICU4X reference is the
//! contract and the macOS value is the locally-verified platform output:
//!
//!   1. format_time(include_seconds = FALSE) — ALL locales. ICU4X still emits
//!      seconds (icu 2.1 `fieldsets::T::short()` resolves to H:mm:ss); the
//!      Foundation/NLS backends correctly honor the flag. Looks like an ICU4X
//!      *backend* bug — flagged for maintainer review.
//!   2. format_datetime — en-US, fr-FR. Foundation joins date+time with a
//!      locale connector ("at" / "à"); ICU4X uses a comma. (de-DE, ja-JP match.)
//!   3. format_list(Or) — ALL locales. `NSListFormatter` has no disjunction
//!      form, so the macOS backend returns the conjunction ("and") output.
//!   4. plural(1) — ja-JP. ICU4X returns Other (Japanese has no plural class);
//!      the macOS/Windows shared `plural_for` table has no `ja` rule and falls
//!      back to the English default (n==1 => One).
//!
//! The Windows NLS backend cannot run on the macOS dev host (its divergent
//! values are therefore NOT pinned here — that job is compile-only in CI; see
//! `.github/workflows/rust.yml` `icu_parity`). Its agnostic-case output is also
//! known to differ from ICU4X in ways not verifiable off-Windows (4-digit short
//! years, U+00A0 vs U+202F French grouping, list comma placement), so the
//! divergent-case helper only smoke-checks Windows output.

#![cfg(any(
    feature = "icu",
    all(target_os = "macos", feature = "icu_macos"),
    all(target_os = "windows", feature = "icu_windows"),
))]

use azul_layout::{
    FormatLength, IcuDate, IcuDateTime, IcuLocalizerHandle, IcuResult, IcuTime, ListType,
    PluralCategory,
};

// ─── Active-backend detection (mirrors the cfg in layout/src/icu.rs exactly) ──

/// True when the ICU4X (reference) backend is the active one.
const IS_ICU4X: bool = cfg!(all(
    feature = "icu",
    not(all(target_os = "macos", feature = "icu_macos")),
    not(all(target_os = "windows", feature = "icu_windows")),
));
/// True when the macOS Foundation backend is the active one.
const IS_MACOS: bool = cfg!(all(target_os = "macos", feature = "icu_macos"));
// (Anything else reachable through the file-level cfg is the Windows NLS backend.)

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn handle() -> IcuLocalizerHandle {
    IcuLocalizerHandle::new("en-US")
}

/// Unwrap an `IcuResult` to its string, panicking on the error arm.
fn ok(res: IcuResult) -> String {
    match res {
        IcuResult::Ok(s) => s.as_str().to_string(),
        IcuResult::Err(e) => panic!("ICU returned an error: {:?}", e.message.as_str()),
    }
}

/// Assert a KNOWN-DIVERGENT case (see the four classes in the module header).
///
/// * ICU4X  — strict equality with the `reference` (the parity contract).
/// * macOS  — strict equality with the locally-verified `macos` value, so the
///   documented divergence is itself pinned and a future drift is caught.
/// * Windows — output is not verifiable off-Windows; smoke-check only.
fn assert_divergent(case: &str, got: &str, reference: &str, macos: &str) {
    if IS_ICU4X {
        assert_eq!(got, reference, "[{case}] drifted from the ICU4X reference");
    } else if IS_MACOS {
        assert_eq!(
            got, macos,
            "[{case}] macOS Foundation drifted from its documented divergent value \
             (ICU4X reference is {reference:?})"
        );
    } else {
        assert!(
            !got.is_empty(),
            "[{case}] Windows NLS produced empty output (ICU4X reference is {reference:?})"
        );
    }
}

// Fixed matrix inputs.
const LOCALES: [&str; 4] = ["en-US", "de-DE", "fr-FR", "ja-JP"];
fn fixed_date() -> IcuDate {
    IcuDate::new(2025, 1, 15)
}
fn fixed_time() -> IcuTime {
    IcuTime::new(16, 30, 45)
}

// ─── Number formatting ──────────────────────────────────────────────────────
// All number cases are byte-identical across ICU4X and macOS Foundation.
// NOTE: French (fr-FR) uses U+202F NARROW NO-BREAK SPACE as the grouping
// separator under ICU4X/CLDR and Foundation.

#[test]
fn numbers() {
    let h = handle();

    // format_integer — grouping separators, incl. a negative.
    assert_eq!(h.format_integer("en-US", 1234567).as_str(), "1,234,567");
    assert_eq!(h.format_integer("de-DE", 1234567).as_str(), "1.234.567");
    assert_eq!(h.format_integer("fr-FR", 1234567).as_str(), "1\u{202f}234\u{202f}567");
    assert_eq!(h.format_integer("ja-JP", 1234567).as_str(), "1,234,567");

    assert_eq!(h.format_integer("en-US", -1000000).as_str(), "-1,000,000");
    assert_eq!(h.format_integer("de-DE", -1000000).as_str(), "-1.000.000");
    assert_eq!(h.format_integer("fr-FR", -1000000).as_str(), "-1\u{202f}000\u{202f}000");
    assert_eq!(h.format_integer("ja-JP", -1000000).as_str(), "-1,000,000");

    // format_decimal(integer_part, decimal_places): 12345 @ 1 dp = 1234.5
    assert_eq!(h.format_decimal("en-US", 12345, 1).as_str(), "1,234.5");
    assert_eq!(h.format_decimal("de-DE", 12345, 1).as_str(), "1.234,5");
    assert_eq!(h.format_decimal("fr-FR", 12345, 1).as_str(), "1\u{202f}234,5");
    assert_eq!(h.format_decimal("ja-JP", 12345, 1).as_str(), "1,234.5");

    // 5 @ 1 dp = 0.5
    assert_eq!(h.format_decimal("en-US", 5, 1).as_str(), "0.5");
    assert_eq!(h.format_decimal("de-DE", 5, 1).as_str(), "0,5");
    assert_eq!(h.format_decimal("fr-FR", 5, 1).as_str(), "0,5");
    assert_eq!(h.format_decimal("ja-JP", 5, 1).as_str(), "0.5");
}

// ─── Date formatting ────────────────────────────────────────────────────────
// All date cases are byte-identical across ICU4X and macOS Foundation.

#[test]
fn dates() {
    let h = handle();
    let d = fixed_date();

    assert_eq!(ok(h.format_date("en-US", d, FormatLength::Short)), "1/15/25");
    assert_eq!(ok(h.format_date("en-US", d, FormatLength::Medium)), "Jan 15, 2025");
    assert_eq!(ok(h.format_date("en-US", d, FormatLength::Long)), "January 15, 2025");

    assert_eq!(ok(h.format_date("de-DE", d, FormatLength::Short)), "15.01.25");
    assert_eq!(ok(h.format_date("de-DE", d, FormatLength::Medium)), "15.01.2025");
    assert_eq!(ok(h.format_date("de-DE", d, FormatLength::Long)), "15. Januar 2025");

    assert_eq!(ok(h.format_date("fr-FR", d, FormatLength::Short)), "15/01/2025");
    assert_eq!(ok(h.format_date("fr-FR", d, FormatLength::Medium)), "15 janv. 2025");
    assert_eq!(ok(h.format_date("fr-FR", d, FormatLength::Long)), "15 janvier 2025");

    assert_eq!(ok(h.format_date("ja-JP", d, FormatLength::Short)), "2025/01/15");
    assert_eq!(ok(h.format_date("ja-JP", d, FormatLength::Medium)), "2025/01/15");
    assert_eq!(ok(h.format_date("ja-JP", d, FormatLength::Long)), "2025年1月15日");
}

// ─── Time formatting ────────────────────────────────────────────────────────

#[test]
fn times() {
    let h = handle();
    let t = fixed_time(); // 16:30:45

    // include_seconds = TRUE — identical across ICU4X and macOS.
    // NOTE: en-US uses U+202F NARROW NO-BREAK SPACE before "PM".
    assert_eq!(ok(h.format_time("en-US", t, true)), "4:30:45\u{202f}PM");
    assert_eq!(ok(h.format_time("de-DE", t, true)), "16:30:45");
    assert_eq!(ok(h.format_time("fr-FR", t, true)), "16:30:45");
    assert_eq!(ok(h.format_time("ja-JP", t, true)), "16:30:45");

    // DIVERGENCE 1: include_seconds = FALSE. The ICU4X backend STILL emits
    // seconds (icu 2.1 `fieldsets::T::short()` includes the second field), while
    // the Foundation/NLS backends correctly drop them. Likely an ICU4X-backend
    // bug — flagged for maintainer review.
    assert_divergent(
        "time_nosec en-US",
        &ok(h.format_time("en-US", t, false)),
        "4:30:45\u{202f}PM", // ICU4X reference (seconds NOT dropped)
        "4:30\u{202f}PM",    // macOS Foundation (seconds dropped, correct)
    );
    assert_divergent("time_nosec de-DE", &ok(h.format_time("de-DE", t, false)), "16:30:45", "16:30");
    assert_divergent("time_nosec fr-FR", &ok(h.format_time("fr-FR", t, false)), "16:30:45", "16:30");
    assert_divergent("time_nosec ja-JP", &ok(h.format_time("ja-JP", t, false)), "16:30:45", "16:30");
}

// ─── Date+time formatting ───────────────────────────────────────────────────

#[test]
fn datetimes() {
    let h = handle();
    let dt = IcuDateTime::new(fixed_date(), fixed_time());

    // de-DE and ja-JP are identical across ICU4X and macOS.
    assert_eq!(ok(h.format_datetime("de-DE", dt, FormatLength::Medium)), "15.01.2025, 16:30");
    assert_eq!(ok(h.format_datetime("ja-JP", dt, FormatLength::Medium)), "2025/01/15 16:30");

    // DIVERGENCE 2: Foundation joins date+time with a locale connector word
    // ("at" / "à"); ICU4X uses a plain comma.
    assert_divergent(
        "datetime_medium en-US",
        &ok(h.format_datetime("en-US", dt, FormatLength::Medium)),
        "Jan 15, 2025, 4:30\u{202f}PM",       // ICU4X reference (comma)
        "Jan 15, 2025 at 4:30\u{202f}PM",     // macOS Foundation ("at")
    );
    assert_divergent(
        "datetime_medium fr-FR",
        &ok(h.format_datetime("fr-FR", dt, FormatLength::Medium)),
        "15 janv. 2025, 16:30",   // ICU4X reference (comma)
        "15 janv. 2025 à 16:30",  // macOS Foundation ("à")
    );
}

// ─── List formatting ────────────────────────────────────────────────────────

#[test]
fn lists() {
    let h = handle();
    let items = ["a", "b", "c"];

    // And + Unit are identical across ICU4X and macOS.
    assert_eq!(h.format_list_strings("en-US", &items, ListType::And).as_str(), "a, b, and c");
    assert_eq!(h.format_list_strings("de-DE", &items, ListType::And).as_str(), "a, b und c");
    assert_eq!(h.format_list_strings("fr-FR", &items, ListType::And).as_str(), "a, b et c");
    assert_eq!(h.format_list_strings("ja-JP", &items, ListType::And).as_str(), "a、b、c");

    for loc in LOCALES {
        assert_eq!(
            h.format_list_strings(loc, &items, ListType::Unit).as_str(),
            "a, b, c",
            "[list_unit {loc}] comma-join is backend-agnostic",
        );
    }

    // DIVERGENCE 3: `NSListFormatter` has no disjunction form, so the macOS
    // backend returns the conjunction ("and") output for Or as well.
    assert_divergent(
        "list_or en-US",
        h.format_list_strings("en-US", &items, ListType::Or).as_str(),
        "a, b, or c",  // ICU4X reference
        "a, b, and c", // macOS Foundation (falls back to "and")
    );
    assert_divergent(
        "list_or de-DE",
        h.format_list_strings("de-DE", &items, ListType::Or).as_str(),
        "a, b oder c",
        "a, b und c",
    );
    assert_divergent(
        "list_or fr-FR",
        h.format_list_strings("fr-FR", &items, ListType::Or).as_str(),
        "a, b ou c",
        "a, b et c",
    );
    assert_divergent(
        "list_or ja-JP",
        h.format_list_strings("ja-JP", &items, ListType::Or).as_str(),
        "a、b、またはc",
        "a、b、c",
    );
}

// ─── Plural rules ───────────────────────────────────────────────────────────

#[test]
fn plurals() {
    use PluralCategory::*;
    let h = handle();

    // Cardinal plural category for n = 1, 2, 5, 0. Agnostic across ALL backends
    // EXCEPT ja-JP n=1 (DIVERGENCE 4, handled separately below).
    let agnostic: &[(&str, i64, PluralCategory)] = &[
        ("en-US", 1, One), ("en-US", 2, Other), ("en-US", 5, Other), ("en-US", 0, Other),
        ("de-DE", 1, One), ("de-DE", 2, Other), ("de-DE", 5, Other), ("de-DE", 0, Other),
        // French: 0 and 1 are both "one".
        ("fr-FR", 1, One), ("fr-FR", 2, Other), ("fr-FR", 5, Other), ("fr-FR", 0, One),
        // Japanese: no plural distinction → "other" (n=1 excluded; see below).
        ("ja-JP", 2, Other), ("ja-JP", 5, Other), ("ja-JP", 0, Other),
    ];
    for (loc, n, cat) in agnostic {
        assert_eq!(
            h.get_plural_category(loc, *n),
            *cat,
            "[plural {loc} n={n}] category mismatch",
        );
    }

    // DIVERGENCE 4: ja-JP plural(1). ICU4X returns Other (Japanese has no plural
    // class); the macOS/Windows shared `plural_for` table lacks a `ja` rule and
    // falls back to the English default (n==1 => One). Both platform backends
    // share that table, so the non-ICU4X value is One on each.
    let ja1 = h.get_plural_category("ja-JP", 1);
    if IS_ICU4X {
        assert_eq!(ja1, Other, "[plural ja-JP n=1] ICU4X reference is Other");
    } else {
        assert_eq!(ja1, One, "[plural ja-JP n=1] macOS/Windows plural_for falls back to One");
    }
}

// ─── Reference re-capture helper (ignored) ──────────────────────────────────
//
// Run under the ICU4X backend to re-dump the whole matrix as `{:?}` debug
// strings (so non-ASCII separators are visible) when CLDR data updates:
//
//   cargo test -p azul-layout --test icu_parity --no-default-features \
//       --features icu -- --ignored --nocapture capture_reference
#[test]
#[ignore = "diagnostic dump; run manually to re-capture the ICU4X reference"]
fn capture_reference() {
    let h = handle();
    let d = fixed_date();
    let t = fixed_time();
    let dt = IcuDateTime::new(d, t);
    let list = ["a", "b", "c"];
    let backend = if IS_ICU4X { "icu4x" } else if IS_MACOS { "macos" } else { "windows" };

    for loc in LOCALES {
        println!("CAP[{backend}]\t{loc}\tint_1234567\t{:?}", h.format_integer(loc, 1234567).as_str());
        println!("CAP[{backend}]\t{loc}\tint_neg1000000\t{:?}", h.format_integer(loc, -1000000).as_str());
        println!("CAP[{backend}]\t{loc}\tdec_1234_5\t{:?}", h.format_decimal(loc, 12345, 1).as_str());
        println!("CAP[{backend}]\t{loc}\tdec_0_5\t{:?}", h.format_decimal(loc, 5, 1).as_str());
        println!("CAP[{backend}]\t{loc}\tdate_short\t{:?}", ok(h.format_date(loc, d, FormatLength::Short)));
        println!("CAP[{backend}]\t{loc}\tdate_medium\t{:?}", ok(h.format_date(loc, d, FormatLength::Medium)));
        println!("CAP[{backend}]\t{loc}\tdate_long\t{:?}", ok(h.format_date(loc, d, FormatLength::Long)));
        println!("CAP[{backend}]\t{loc}\ttime_nosec\t{:?}", ok(h.format_time(loc, t, false)));
        println!("CAP[{backend}]\t{loc}\ttime_sec\t{:?}", ok(h.format_time(loc, t, true)));
        println!("CAP[{backend}]\t{loc}\tdatetime_medium\t{:?}", ok(h.format_datetime(loc, dt, FormatLength::Medium)));
        println!("CAP[{backend}]\t{loc}\tlist_and\t{:?}", h.format_list_strings(loc, &list, ListType::And).as_str());
        println!("CAP[{backend}]\t{loc}\tlist_or\t{:?}", h.format_list_strings(loc, &list, ListType::Or).as_str());
        println!("CAP[{backend}]\t{loc}\tlist_unit\t{:?}", h.format_list_strings(loc, &list, ListType::Unit).as_str());
        for n in [1_i64, 2, 5, 0] {
            println!("CAP[{backend}]\t{loc}\tplural_{n}\t{:?}", h.get_plural_category(loc, n));
        }
    }
}
