//! Fluent Localization Demo for Azul GUI Framework (Rust)
//!
//! This example demonstrates:
//! - Loading Fluent translations from strings
//! - Loading Fluent translations from ZIP archives
//! - Plural rules with select expressions
//! - Syntax checking for .fluent files
//! - Creating ZIP archives of translations
//!
//! Run with:
//!   cd examples/rust && cargo run --example fluent_demo --features fluent

use azul::desktop::fluent::{
    FluentLocalizerHandle, FluentFormatArg, 
    check_fluent_syntax, create_fluent_zip, FluentZipEntry,
    FluentSyntaxCheckResult
};

// We use azul_css::AzString directly since azul re-exports it
use azul_css::AzString;

fn main() {
    println!("=== Fluent Localization Demo ===\n");

    // Create a localizer
    let localizer = FluentLocalizerHandle::new("en-US");

    // Add English translations
    let en_ftl = r#"
# English translations
hello = Hello, world!
greeting = Hello, { $name }!
emails = You have { $count ->
    [one] one new email
   *[other] { $count } new emails
}.
welcome = Welcome to { $app }!
"#;

    // Add German translations
    let de_ftl = r#"
# German translations
hello = Hallo, Welt!
greeting = Hallo, { $name }!
emails = Du hast { $count ->
    [one] eine neue E-Mail
   *[other] { $count } neue E-Mails
}.
welcome = Willkommen bei { $app }!
"#;

    // Add French translations
    let fr_ftl = r#"
# French translations
hello = Bonjour le monde!
greeting = Bonjour, { $name }!
emails = Vous avez { $count ->
    [one] un nouveau message
   *[other] { $count } nouveaux messages
}.
welcome = Bienvenue dans { $app }!
"#;

    println!("--- Loading translations ---");
    assert!(localizer.add_resource("en-US", en_ftl), "Failed to add en-US");
    assert!(localizer.add_resource("de-DE", de_ftl), "Failed to add de-DE");
    assert!(localizer.add_resource("fr-FR", fr_ftl), "Failed to add fr-FR");
    
    let loaded_locales = localizer.get_loaded_locales();
    println!("Loaded {} locales:", loaded_locales.len());
    for locale in &loaded_locales {
        println!("  - {}", locale.as_str());
    }

    println!("\n--- Basic translation ---");
    for locale in &["en-US", "de-DE", "fr-FR"] {
        let hello = localizer.translate(locale, "hello", None);
        println!("{}: {}", locale, hello.as_str());
    }

    println!("\n--- Translation with arguments ---");
    let args = vec![FluentFormatArg::string("name", "Alice")];
    for locale in &["en-US", "de-DE", "fr-FR"] {
        let greeting = localizer.translate(locale, "greeting", Some(&args));
        println!("{}: {}", locale, greeting.as_str());
    }

    println!("\n--- Plural rules ---");
    for count in [0, 1, 2, 5, 21] {
        let args = vec![FluentFormatArg::number("count", count as f64)];
        for locale in &["en-US", "de-DE", "fr-FR"] {
            let msg = localizer.translate(locale, "emails", Some(&args));
            println!("{} (count={}): {}", locale, count, msg.as_str());
        }
        println!();
    }

    println!("--- Syntax checking ---");
    let valid_ftl = "hello = Hello!";
    let invalid_ftl = "hello = ";
    
    match check_fluent_syntax(valid_ftl) {
        FluentSyntaxCheckResult::Ok => println!("Valid FTL: OK ✓"),
        FluentSyntaxCheckResult::Errors(e) => println!("Valid FTL: Unexpected errors: {:?}", e),
    }
    
    match check_fluent_syntax(invalid_ftl) {
        FluentSyntaxCheckResult::Ok => println!("Invalid FTL: Unexpected OK"),
        FluentSyntaxCheckResult::Errors(e) => {
            println!("Invalid FTL: Found {} error(s):", e.len());
            for err in &e {
                println!("  - {}", err.as_str());
            }
        }
    }

    println!("\n--- ZIP creation ---");
    let entries = vec![
        FluentZipEntry {
            path: AzString::from("en-US.fluent".to_string()),
            content: AzString::from(en_ftl.to_string()),
        },
        FluentZipEntry {
            path: AzString::from("de-DE.fluent".to_string()),
            content: AzString::from(de_ftl.to_string()),
        },
        FluentZipEntry {
            path: AzString::from("fr-FR.fluent".to_string()),
            content: AzString::from(fr_ftl.to_string()),
        },
    ];

    match create_fluent_zip(&entries) {
        Ok(zip_data) => {
            println!("Created ZIP archive: {} bytes", zip_data.len());
            
            // Test loading from ZIP
            let localizer2 = FluentLocalizerHandle::new("en-US");
            let result = localizer2.load_from_zip(&zip_data);
            println!("Loaded from ZIP: {} files, {} failed", result.files_loaded, result.files_failed);
            
            if result.files_failed > 0 {
                for err in &result.errors {
                    println!("  Error: {}", err.as_str());
                }
            }
            
            // Debug: Check what locales were loaded
            let loaded = localizer2.get_loaded_locales();
            println!("Locales in ZIP: {:?}", loaded.iter().map(|s| s.as_str()).collect::<Vec<_>>());
            
            // Verify it works
            let hello = localizer2.translate("en-US", "hello", None);
            println!("Verification (en-US): '{}'", hello.as_str());
            
            let hello_de = localizer2.translate("de-DE", "hello", None);
            println!("Verification (de-DE): '{}'", hello_de.as_str());
            
            // Check if message exists
            let has_hello = localizer2.has_message("en-US", "hello");
            println!("has_message('en-US', 'hello'): {}", has_hello);
        }
        Err(e) => println!("Failed to create ZIP: {}", e),
    }

    println!("\n--- Fallback behavior ---");
    // Try to translate a message that doesn't exist
    let missing = localizer.translate("en-US", "nonexistent", None);
    println!("Missing message (returns message ID): '{}'", missing.as_str());

    // Try an unknown locale (should fall back to default)
    let unknown = localizer.translate("zh-CN", "hello", None);
    println!("Unknown locale 'zh-CN' (falls back to 'en-US'): '{}'", unknown.as_str());

    println!("\n=== Demo complete! ===");
}
