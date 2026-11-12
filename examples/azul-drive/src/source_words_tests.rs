//! The Add drive dialog's sources in the window's language: azul-storage's catalog says its
//! groups, sources, fields, form problems and drive kinds in English; AzDrive says them in
//! German too, and in English as the catalog does.

use azul_appkit::l10n::{keep, named_key, set_locale, t};
use azul_storage::{
    catalog::{self, DriveKind, FormProblem, ServiceGroup},
    config::DatabaseEngine,
};

use crate::{l10n::sources, source_words};

fn has(key: &str) -> bool {
    t(key) != key
}

#[test]
fn every_word_of_the_sources_has_its_german() {
    keep(&sources());
    set_locale("de-DE");
    for group in ServiceGroup::ALL {
        let key = named_key("AzDrive", "group", group.title());
        assert!(has(&key), "{key}");
    }
    for spec in catalog::services() {
        let key = named_key("AzDrive", "summary", spec.summary);
        assert!(has(&key), "{key}");
        for f in spec.fields {
            let key = named_key("AzDrive", "field", f.label);
            assert!(has(&key), "{key}");
            if !f.help.is_empty() {
                let key = named_key("AzDrive", "help", f.help);
                assert!(has(&key), "{key}");
            }
            // A placeholder of words (not an example address).
            if f.placeholder.contains(' ') {
                let key = named_key("AzDrive", "placeholder", f.placeholder);
                assert!(has(&key), "{key}");
            }
        }
    }
    set_locale("en-US");
}

#[test]
fn a_source_its_form_and_its_problems_are_said_in_the_windows_language() {
    keep(&sources());
    let webdav = catalog::service("webdav").unwrap();
    let endpoint = webdav.field("endpoint").unwrap();
    set_locale("de-DE");
    assert_eq!(source_words::field_label(endpoint), "Serveradresse");
    assert_eq!(
        source_words::problem(FormProblem::Required(endpoint)),
        "„Serveradresse“ ist erforderlich."
    );
    assert_eq!(
        source_words::problem(FormProblem::NoName),
        "Gib dem Laufwerk einen Namen."
    );
    assert_eq!(
        source_words::group(ServiceGroup::NetworkNas),
        "Netzwerk & NAS"
    );
    assert_eq!(
        source_words::name(catalog::service("local").unwrap()),
        "Ordner auf diesem Computer"
    );
    assert_eq!(
        source_words::name(catalog::service("dropbox").unwrap()),
        "Dropbox",
        "a name as it is"
    );
    assert_eq!(
        source_words::kind(&DriveKind::LocalDisk),
        "Lokaler Datenträger"
    );
    assert_eq!(
        source_words::kind(&DriveKind::Database(DatabaseEngine::Sqlite)),
        "SQLite-Datenbank"
    );
    set_locale("en-US");
    // English: the catalog's own words.
    for problem in [
        FormProblem::NoName,
        FormProblem::Required(endpoint),
        FormProblem::NotAnAddress(endpoint),
    ] {
        assert_eq!(source_words::problem(problem), problem.to_string());
    }
    for kind in [
        DriveKind::LocalDisk,
        DriveKind::AzlinCloud,
        DriveKind::S3Bucket,
        DriveKind::Source(webdav),
        DriveKind::Database(DatabaseEngine::Sqlite),
    ] {
        assert_eq!(source_words::kind(&kind), kind.to_string());
    }
}

/// Storage providers' names, which AzDrive's words never list: a provider is configuration, and
/// a name appears only as the label of a source type of its own (`Backblaze B2`'s own API).
const PROVIDERS: &[&str] = &[
    "AWS", "Amazon", "amazonaws", "MinIO", "Cloudflare", "R2", "Backblaze", "Wasabi", "Garage",
];

/// The provider names among `text`'s words.
fn providers_in(text: &str) -> Vec<&'static str> {
    let words: Vec<&str> = text.split(|c: char| !c.is_ascii_alphanumeric()).collect();
    PROVIDERS
        .iter()
        .copied()
        .filter(|p| words.iter().any(|w| w.eq_ignore_ascii_case(p)))
        .collect()
}

#[test]
fn s3_compatible_storage_lists_no_provider_and_says_any() {
    let s3 = catalog::service("s3").unwrap();
    assert_eq!(s3.summary, "Any S3-compatible storage");
    let mut words = vec![s3.name, s3.summary];
    for f in s3.fields {
        words.extend([f.label, f.help, f.placeholder]);
    }
    for text in words {
        assert!(providers_in(text).is_empty(), "{text:?} names {:?}", providers_in(text));
    }
    let empty = crate::browse::DriveForm {
        name: String::from("Photos"),
        ..crate::browse::DriveForm::default()
    };
    let said = empty.check().unwrap_err();
    assert!(providers_in(&said).is_empty(), "{said}");
}

#[test]
fn no_message_of_azdrive_names_a_storage_provider_but_a_source_types_own_label() {
    for (file, ftl) in [
        ("en.ftl", include_str!("../resources/en.ftl")),
        ("de.ftl", include_str!("../resources/de.ftl")),
    ] {
        for line in ftl.lines() {
            let Some((key, value)) = line.split_once(" = ") else {
                continue;
            };
            // A source type's own name and line (Backblaze B2's own API), a way to pay.
            let label = key.starts_with("azdrive-service-")
                || key.starts_with("azdrive-summary-b2-")
                || key.starts_with("azdrive-pay-method-");
            if !label {
                assert!(providers_in(value).is_empty(), "{file}: {key} = {value}");
            }
        }
    }
}

/// AZ-CN8 / AZ-CN9: the new things a form can say are said in German too, and a form's problem
/// names its field.
#[test]
fn the_new_form_problems_are_said_in_english_and_german() {
    keep(&sources());
    let endpoint = catalog::service("webdav")
        .unwrap()
        .field("endpoint")
        .unwrap();
    let credential = catalog::service("gcs")
        .unwrap()
        .field("credential")
        .unwrap();
    let problems = [
        FormProblem::WrongScheme(endpoint),
        FormProblem::AddressHasLogin(endpoint),
        FormProblem::SendsInClear(endpoint),
        FormProblem::NoCredential(credential),
        FormProblem::NotAKey(credential),
    ];
    set_locale("en-US");
    // A problem that names no list is the library's own English, word for word: the two cannot
    // drift apart.
    for problem in problems {
        if matches!(problem, FormProblem::WrongScheme(_)) {
            continue;
        }
        assert_eq!(
            source_words::problem(problem),
            problem.to_string(),
            "{problem:?}"
        );
    }
    // `WrongScheme` names the schemes of its field, and there the two Englishes differ in one
    // thing: the library joins them with the word "or", AzDrive's sentence with a sign (" / "),
    // which the German sentence has no English word to translate for (`source_words::problem`,
    // `azdrive-form-wrong-scheme`). AzDrive's is the designed text (the resources and the one
    // join), so it is pinned whole, and the library's is the same sentence but for that joiner.
    let wrong = FormProblem::WrongScheme(endpoint);
    assert_eq!(
        source_words::problem(wrong),
        "\"Server address\" must start with https:// / http://."
    );
    assert_eq!(
        source_words::problem(wrong).replace(" / ", " or "),
        wrong.to_string(),
        "the library's sentence is AzDrive's but for the joiner of the schemes"
    );
    set_locale("de-DE");
    for problem in problems {
        let german = source_words::problem(problem);
        assert_ne!(german, problem.to_string(), "{problem:?}");
        assert!(
            !german.contains("azdrive-form-"),
            "a missing key shows itself: {german}"
        );
    }
    assert_eq!(
        source_words::problem(wrong),
        "„Serveradresse“ muss mit https:// / http:// beginnen."
    );
    set_locale("en-US");
}

/// AZ-CN8: a stored address with a password in it is shown without; AZ-CN6: a database says how
/// many rows of a table it shows.
#[test]
fn az_cn8_az_cn6_the_rows_of_a_source_hide_a_password_and_name_the_row_limit() {
    use azul_storage::config::{DatabaseEngine, DriveEntry, DriveLocation};
    keep(&sources());
    set_locale("en-US");
    let mut options = std::collections::BTreeMap::new();
    options.insert(
        "endpoint".to_string(),
        "https://ann:hunter2@cloud.example/dav".to_string(),
    );
    options.insert("username".to_string(), "ann".to_string());
    let webdav = DriveEntry {
        id: "nas-1".to_string(),
        name: "NAS".to_string(),
        location: DriveLocation::Opendal {
            scheme: "webdav".to_string(),
            options,
            keyring: true,
        },
    };
    let DriveLocation::Opendal { options, .. } = &webdav.location else {
        unreachable!()
    };
    let rows = crate::ui_dialogs::source_rows(&webdav, options);
    assert_eq!(rows[0].1, "https://cloud.example/dav");
    assert!(
        rows.iter().all(|(_, value)| !value.contains("hunter2")),
        "{rows:?}"
    );
    assert_eq!(rows.len(), 2, "a web source has no row limit: {rows:?}");

    let mut options = std::collections::BTreeMap::new();
    options.insert("host".to_string(), "db.example".to_string());
    let database = DriveEntry {
        id: "shop-1".to_string(),
        name: "Shop".to_string(),
        location: DriveLocation::Database {
            engine: DatabaseEngine::Postgres,
            options: options.clone(),
            keyring: false,
        },
    };
    let rows = crate::ui_dialogs::source_rows(&database, &options);
    assert_eq!(
        rows.last()
            .map(|(name, value)| (name.as_str(), value.as_str())),
        Some(("Rows shown per table (at most)", "10,000"))
    );
}
