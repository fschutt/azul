use azul::desktop::zip::{
    zip_create_from_files, zip_extract_all, zip_list_contents, ZipReadConfig, ZipWriteConfig,
};
use azul::{
    fluent::FluentLocalizerHandle,
    fmt::{FmtArg, FmtValue},
};

fn main() {
    println!("=== HTTP & ZIP Language Pack Demo ===\n");

    let localizer = FluentLocalizerHandle::create("en-US");

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

    println!(
        "   Loaded: {:?}",
        localizer
            .get_loaded_locales()
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
    );
    println!(
        "   en-US 'hello': {}",
        localizer.translate("en-US", "hello", Vec::<FmtArg>::new())
    );
    println!(
        "   de-DE 'hello': {}",
        localizer.translate("de-DE", "hello", Vec::<FmtArg>::new())
    );
    println!();

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

    let files = vec![
        ("fr-FR.fluent".to_string(), fr_fr_ftl.as_bytes().to_vec()),
        ("es-ES.fluent".to_string(), es_es_ftl.as_bytes().to_vec()),
    ];

    let zip_data =
        zip_create_from_files(files, &ZipWriteConfig::default()).expect("Failed to create ZIP");
    println!("   Created ZIP: {} bytes", zip_data.len());

    let contents =
        zip_list_contents(&zip_data, &ZipReadConfig::default()).expect("Failed to list ZIP");
    println!("   Contents: {:?}", contents);

    let load_result = localizer.load_from_zip(zip_data.as_slice().into());
    println!("   Loaded {} files from ZIP", load_result.files_loaded);
    println!(
        "   All locales now: {:?}",
        localizer
            .get_loaded_locales()
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
    );
    println!(
        "   fr-FR 'hello': {}",
        localizer.translate("fr-FR", "hello", Vec::<FmtArg>::new())
    );
    println!(
        "   es-ES 'hello': {}",
        localizer.translate("es-ES", "hello", Vec::<FmtArg>::new())
    );
    println!();

    println!("3. Network language pack download (simulated)...");

    println!("   Would use: download_bytes(url) -> HttpResult<Vec<u8>>");
    println!("   Or: download_cached(url, cache_dir, filename) -> HttpResult<PathBuf>");
    println!();

    println!("4. Extracting specific files from ZIP...");

    let entries = zip_extract_all(&zip_data, &ZipReadConfig::default()).expect("Failed to extract");
    for entry in &entries {
        println!(
            "   {} - {} bytes ({})",
            entry.path,
            entry.data.len(),
            if entry.is_directory { "dir" } else { "file" }
        );
    }
    println!();

    println!("5. Full translation workflow...");

    let user_name = "Alice";
    let email_count = 5.0;

    let locales = ["en-US", "de-DE", "fr-FR", "es-ES"];

    for locale in &locales {
        let welcome_args = vec![FmtArg {
            key: "name".into(),
            value: FmtValue::Str(user_name.into()),
        }];
        let email_args = vec![FmtArg {
            key: "count".into(),
            value: FmtValue::Double(email_count),
        }];

        let welcome = localizer.translate(*locale, "welcome", welcome_args);
        let emails = localizer.translate(*locale, "emails", email_args);

        println!("   [{}]", locale);
        println!("      {}", welcome);
        println!("      {}", emails);
    }
    println!();

    println!("6. Language pack update pattern...");

    let demo_cache_dir = std::env::temp_dir().join("azul_langpacks");
    println!("   Cache directory: {:?}", demo_cache_dir);
    println!("   Pattern: download_cached(url, cache_dir, filename)");
    println!("   This returns cached file path if already downloaded.");
    println!();

    println!("=== Demo Complete ===");
}
