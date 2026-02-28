//! ICU4X Internationalization Demo for Azul GUI Framework (Rust)
//!
//! This example demonstrates locale-aware:
//! - Number formatting (thousands separators, decimal points)
//! - Date and time formatting
//! - Plural rules (1 item vs 2 items)
//! - List formatting ("A, B, and C")
//! - String collation/sorting
//!
//! Run with:
//!   cd examples/rust && cargo run --example icu_demo --features icu

use azul::desktop::icu::{
    FormatLength, IcuDate, IcuDateTime, IcuLocalizerHandle, IcuTime, ListType,
};

// We use azul_css::AzString directly since azul re-exports it
use azul_css::AzString;

fn demo_locale(locale_name: &str, locale_code: &str) {
    println!("\n{}", "=".repeat(60));
    println!("Locale: {} ({})", locale_name, locale_code);
    println!("{}", "=".repeat(60));
    
    // Create a shared cache for all ICU operations
    // The cache will lazily create formatters per-locale as needed
    let cache = IcuLocalizerHandle::new(locale_code);
    
    // === Number Formatting ===
    println!("\n--- Number Formatting ---");
    let number: i64 = 1234567;
    let formatted = cache.format_integer(locale_code, number);
    println!("Raw:       {}", number);
    println!("Formatted: {}", formatted.as_str());
    
    // === Plural Rules ===
    println!("\n--- Plural Rules ---");
    for count in [0i64, 1, 2, 5, 21] {
        let category = cache.get_plural_category(locale_code, count);
        let message = cache.pluralize(
            locale_code,
            count,
            "no items",    // zero
            "1 item",      // one
            "2 items",     // two
            "{} items",    // few
            "{} items",    // many
            "{} items",    // other
        );
        println!("count={:2}: '{}' (category: {:?})", count, message.as_str(), category);
    }
    
    // === List Formatting ===
    println!("\n--- List Formatting ---");
    let items = vec![
        AzString::from("Apple"),
        AzString::from("Banana"),
        AzString::from("Cherry"),
    ];
    let and_list = cache.format_list(locale_code, &items, ListType::And);
    let or_list = cache.format_list(locale_code, &items, ListType::Or);
    println!("And-list: {}", and_list.as_str());
    println!("Or-list:  {}", or_list.as_str());
    
    // === String Collation/Sorting ===
    println!("\n--- Locale-Aware Sorting (Collation) ---");
    let unsorted = vec![
        AzString::from("Österreich"),
        AzString::from("Andorra"),
        AzString::from("Ägypten"),
        AzString::from("Bahamas"),
        AzString::from("Öland"),
    ];
    let sorted = cache.sort_strings(locale_code, &unsorted);
    println!("Unsorted: {:?}", unsorted.iter().map(|s| s.as_str()).collect::<Vec<_>>());
    println!("Sorted:   {:?}", sorted.iter().map(|s| s.as_str()).collect::<Vec<_>>());
    
    // String comparison
    println!("\n--- String Comparison ---");
    let a = "Ägypten";
    let b = "Bahamas";
    let cmp = cache.compare_strings(locale_code, a, b);
    let cmp_str = match cmp {
        x if x < 0 => "<",
        x if x > 0 => ">",
        _ => "==",
    };
    println!("'{}' {} '{}' (result: {})", a, cmp_str, b, cmp);
    
    // === Date/Time Formatting ===
    println!("\n--- Date/Time Formatting ---");
    let date = IcuDate { year: 2025, month: 1, day: 15 };
    let time = IcuTime { hour: 16, minute: 30, second: 45 };
    let datetime = IcuDateTime { date, time };
    
    if let Some(fd) = cache.format_date(locale_code, date, FormatLength::Short).into_option() {
        println!("Date (Short):  {}", fd.as_str());
    }
    if let Some(fd) = cache.format_date(locale_code, date, FormatLength::Medium).into_option() {
        println!("Date (Medium): {}", fd.as_str());
    }
    if let Some(fd) = cache.format_date(locale_code, date, FormatLength::Long).into_option() {
        println!("Date (Long):   {}", fd.as_str());
    }
    
    if let Some(ft) = cache.format_time(locale_code, time, false).into_option() {
        println!("Time (12h):    {}", ft.as_str());
    }
    if let Some(ft) = cache.format_time(locale_code, time, true).into_option() {
        println!("Time (24h):    {}", ft.as_str());
    }
    
    if let Some(fdt) = cache.format_datetime(locale_code, datetime, FormatLength::Long).into_option() {
        println!("DateTime:      {}", fdt.as_str());
    }
}

fn demo_multi_locale() {
    println!("\n{}", "=".repeat(60));
    println!("Multi-Locale Demo (Single Cache)");
    println!("{}", "=".repeat(60));
    
    // Create a single cache that can handle multiple locales
    let cache = IcuLocalizerHandle::default();
    let number: i64 = 1234567;
    
    println!("\nFormatting {} in different locales:", number);
    
    // Format the same number in different locales using one cache
    let locales = [
        ("en-US", "English (US)"),
        ("de-DE", "German"),
        ("fr-FR", "French"),
        ("ja-JP", "Japanese"),
    ];
    
    for (locale, name) in locales {
        let formatted = cache.format_integer(locale, number);
        println!("  {}: {}", name, formatted.as_str());
    }
    
    // Demonstrate dynamic locale switching for pluralization
    println!("\nPlural rules for count=2 in different locales:");
    for (locale, name) in locales {
        let message = cache.pluralize(
            locale,
            2,
            "no items", "1 item", "2 items", "{} items", "{} items", "{} items",
        );
        let category = cache.get_plural_category(locale, 2);
        println!("  {}: '{}' ({:?})", name, message.as_str(), category);
    }
}

fn main() {
    println!("##############################################################");
    println!("#       ICU4X Internationalization Demo for Azul             #");
    println!("##############################################################");
    println!();
    println!("This demo shows how ICU4X provides locale-aware formatting");
    println!("for numbers, dates, plurals, lists, and string sorting.");
    println!();
    println!("NEW: All functions now take a locale parameter, allowing");
    println!("     dynamic language switching per-call!");
    
    // Demo different locales (each creates its own cache)
    demo_locale("English (US)", "en-US");
    demo_locale("German", "de-DE");
    demo_locale("French", "fr-FR");
    demo_locale("Spanish", "es-ES");
    demo_locale("Japanese", "ja-JP");
    
    // Demo multi-locale support with a single cache
    demo_multi_locale();
    
    println!("\n{}", "=".repeat(60));
    println!("Demo complete!");
    println!("{}", "=".repeat(60));
}
