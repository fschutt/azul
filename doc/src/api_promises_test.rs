//! What api.json's own doc strings tell a user to call must be declared in api.json.
//!
//! `IcuDate::now()`, `IcuTime::now()` and `IcuDateTime::now()` were the answer its doc strings of
//! `format_date`, `format_time` and `format_datetime` gave to "where does the date come from", and no
//! function of that name was declared: azul-layout compiled them only with `icu_chrono`, which no
//! shipped feature set turned on.

use std::collections::BTreeSet;

fn api() -> serde_json::Value {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("api.json");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&text).expect("api.json parses")
}

/// The constructors and functions api.json declares for `class`, in whatever module it sits.
fn declared(api: &serde_json::Value, class: &str) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    let modules = api["0.2.0"]["api"].as_object().expect("the api of 0.2.0");
    for module in modules.values() {
        let Some(found) = module["classes"].get(class) else {
            continue;
        };
        for kind in ["constructors", "functions"] {
            if let Some(map) = found[kind].as_object() {
                names.extend(map.keys().cloned());
            }
        }
    }
    names
}

#[test]
fn the_now_functions_the_docs_promise_are_declared() {
    let api = api();
    for class in ["IcuDate", "IcuTime", "IcuDateTime"] {
        let functions = declared(&api, class);
        assert!(!functions.is_empty(), "{class} is not in api.json");
        for name in ["now", "now_utc"] {
            assert!(
                functions.contains(name),
                "{class}::{name}() is what the docs of format_date, format_time and format_datetime \
                 tell the user to call, and api.json declares only {functions:?}"
            );
        }
    }
    let datetime = declared(&api, "IcuDateTime");
    for name in ["timestamp_now", "timestamp_now_seconds", "from_timestamp", "from_timestamp_millis"] {
        assert!(datetime.contains(name), "IcuDateTime::{name}() is not declared: {datetime:?}");
    }
}
