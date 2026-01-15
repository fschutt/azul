//! Example demonstrating HTTP and ZIP modules for language pack downloading
//!
//! Shows how to:
//! 1. Download language packs from a network URL
//! 2. Load translations from ZIP files
//! 3. Cache downloaded language packs locally
//! 4. Use multiple translation sources (builtin, path, network)

use azul::desktop::fluent::{FluentLocalizerHandle, FluentFormatArg};
use azul::desktop::http::{download_bytes, download_cached, HttpRequestConfig, http_get_with_config, HttpError};
use azul::desktop::zip::{zip_list_contents, zip_extract_all, zip_create_from_files, ZipEntry};
use std::path::PathBuf;

fn main() {
    println!("=== HTTP & ZIP Language Pack Demo ===\n");
    
    // Create a localizer instance
    let localizer = FluentLocalizerHandle::new("en-US");
    
    // =========================================================================
    // Example 1: Builtin translations (embedded in binary)
    // =========================================================================
    println!("1. Loading builtin translations...");
    
    let en_us_ftl = r#"
# English translations (builtin)
hello = Hello!
welcome = Welcome, { $name }!
emails = { $count ->
    [one] You have one email.
   *[other] You have { $count } emails.
}
"#;
    
    let de_de_ftl = r#"
# German translations (builtin)
hello = Hallo!
welcome = Willkommen, { $name }!
emails = { $count ->
    [one] Du hast eine E-Mail.
   *[other] Du hast { $count } E-Mails.
}
"#;
    
    localizer.add_resource("en-US", en_us_ftl);
    localizer.add_resource("de-DE", de_de_ftl);
    
    println!("   Loaded: {:?}", localizer.get_loaded_locales());
    println!("   en-US 'hello': {}", localizer.translate("en-US", "hello", None));
    println!("   de-DE 'hello': {}", localizer.translate("de-DE", "hello", None));
    println!();
    
    // =========================================================================
    // Example 2: Creating a language pack ZIP for distribution
    // =========================================================================
    println!("2. Creating language pack ZIP...");
    
    let fr_fr_ftl = r#"
# French translations
hello = Bonjour!
welcome = Bienvenue, { $name }!
emails = { $count ->
    [one] Vous avez un message.
   *[other] Vous avez { $count } messages.
}
"#;
    
    let es_es_ftl = r#"
# Spanish translations
hello = ¡Hola!
welcome = ¡Bienvenido, { $name }!
emails = { $count ->
    [one] Tienes un correo.
   *[other] Tienes { $count } correos.
}
"#;
    
    // Create ZIP with language files
    let files = vec![
        ("fr-FR.fluent".to_string(), fr_fr_ftl.as_bytes().to_vec()),
        ("es-ES.fluent".to_string(), es_es_ftl.as_bytes().to_vec()),
    ];
    
    let zip_data = zip_create_from_files(&files).expect("Failed to create ZIP");
    println!("   Created ZIP: {} bytes", zip_data.len());
    
    // List contents
    let contents = zip_list_contents(&zip_data).expect("Failed to list ZIP");
    println!("   Contents: {:?}", contents);
    
    // Load from ZIP
    let load_result = localizer.load_from_zip(&zip_data);
    println!("   Loaded {} files from ZIP", load_result.files_loaded);
    println!("   All locales now: {:?}", localizer.get_loaded_locales());
    println!("   fr-FR 'hello': {}", localizer.translate("fr-FR", "hello", None));
    println!("   es-ES 'hello': {}", localizer.translate("es-ES", "hello", None));
    println!();
    
    // =========================================================================
    // Example 3: Simulated network download (would work with real URL)
    // =========================================================================
    println!("3. Network language pack download (simulated)...");
    
    // In a real app, you would download from a URL like:
    // let result = download_bytes("https://example.com/langpacks/ja-JP.zip");
    
    // For demo, we show the API usage:
    println!("   Would use: download_bytes(url) -> HttpResult<Vec<u8>>");
    println!("   Or: download_cached(url, cache_dir, filename) -> HttpResult<PathBuf>");
    println!();
    
    // =========================================================================
    // Example 4: Extracting specific files from ZIP
    // =========================================================================
    println!("4. Extracting specific files from ZIP...");
    
    let entries = zip_extract_all(&zip_data).expect("Failed to extract");
    for entry in &entries {
        println!("   {} - {} bytes ({})", 
            entry.path,
            entry.size,
            if entry.is_directory { "dir" } else { "file" }
        );
    }
    println!();
    
    // =========================================================================
    // Example 5: Full translation workflow
    // =========================================================================
    println!("5. Full translation workflow...");
    
    let user_name = "Alice";
    let email_count = 5.0;
    
    let locales = ["en-US", "de-DE", "fr-FR", "es-ES"];
    
    for locale in &locales {
        let welcome_args = vec![
            FluentFormatArg::string("name", user_name),
        ];
        let email_args = vec![
            FluentFormatArg::number("count", email_count),
        ];
        
        let welcome = localizer.translate(locale, "welcome", Some(&welcome_args));
        let emails = localizer.translate(locale, "emails", Some(&email_args));
        
        println!("   [{}]", locale);
        println!("      {}", welcome);
        println!("      {}", emails);
    }
    println!();
    
    // =========================================================================
    // Example 6: Language pack update pattern
    // =========================================================================
    println!("6. Language pack update pattern...");
    
    // In a real app:
    // 1. App starts with builtin translations
    // 2. Check for updates: is_url_reachable(update_url)
    // 3. Download if available: download_cached(url, cache_dir, None)
    // 4. Load from cache: localizer.load_from_path(&cached_path, None)
    
    let demo_cache_dir = std::env::temp_dir().join("azul_langpacks");
    println!("   Cache directory: {:?}", demo_cache_dir);
    println!("   Pattern: download_cached(url, cache_dir, filename)");
    println!("   This returns cached file path if already downloaded.");
    println!();
    
    println!("=== Demo Complete ===");
}
