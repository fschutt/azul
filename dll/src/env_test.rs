use super::*;

/// What `getprop` prints: one `[name]: [value]` per line, the `debug.az.*` ones among others.
const LISTING: &str = "\
[dalvik.vm.heapsize]: [512m]
[debug.az.e2e]: [/data/local/tmp/x.json]
[debug.az.present]: [1]
[debug.az.empty]: []
[debug.az.]: [nothing]
[debug.az.env]: [AZ_LOG=debug; AZ_BACKEND = cpu ;broken;=novalue;AZ_X=a=b]
[debug.other]: [AZ_E2E]
not a property line
";

#[test]
fn the_properties_of_a_listing_become_variables() {
    let properties = AndroidProperties::from_listing(LISTING);
    assert_eq!(properties.named("AZ_E2E"), Some("/data/local/tmp/x.json"));
    assert_eq!(
        properties.named("AZ_PRESENT"),
        Some("1"),
        "debug.az.present is the variable AZ_PRESENT"
    );
    assert_eq!(
        properties.named("AZ_EMPTY"),
        None,
        "an empty property is no value"
    );
    assert_eq!(properties.named("AZ_"), None, "the bare prefix names nothing");
    assert_eq!(properties.named("HEAPSIZE"), None, "other properties are not ours");
    assert_eq!(properties.named("AZ_OTHER"), None, "only debug.az.* counts");
}

#[test]
fn the_env_property_carries_pairs_and_skips_what_is_no_pair() {
    let properties = AndroidProperties::from_listing(LISTING);
    assert_eq!(properties.pair("AZ_LOG"), Some("debug"));
    assert_eq!(
        properties.pair("AZ_BACKEND"),
        Some("cpu"),
        "blanks around key and value are dropped"
    );
    assert_eq!(
        properties.pair("AZ_X"),
        Some("a=b"),
        "the value is everything after the first ="
    );
    assert_eq!(properties.pair("broken"), None, "a word with no = is skipped");
    assert_eq!(properties.pair(""), None, "an empty key is skipped");
    assert_eq!(properties.pairs.len(), 3);
}

#[test]
fn a_listing_without_our_properties_is_empty() {
    assert_eq!(
        AndroidProperties::from_listing("[ro.build.id]: [x]\n[debug.other]: [y]\n"),
        AndroidProperties::default()
    );
    assert_eq!(AndroidProperties::from_listing(""), AndroidProperties::default());
}

#[test]
fn the_reader_of_a_desktop_has_no_properties() {
    // `read` asks the system's property area on Android only
    if !cfg!(target_os = "android") {
        assert_eq!(AndroidProperties::read(), AndroidProperties::default());
    }
}

/// The entries the property area gives (a name and a value each) become the same variables the text of
/// `getprop` did: the reader that asks bionic and the parser of a listing agree.
///
/// Mutation: a prefix other than `debug.az.`; the env property parsed as a named one; an empty value kept.
#[test]
fn the_entries_of_the_property_area_become_variables_like_a_listing_does() {
    let entries = [
        ("dalvik.vm.heapsize", "512m"),
        ("debug.az.e2e", "/data/local/tmp/x.json"),
        ("debug.az.empty", ""),
        ("debug.az.", "nothing"),
        ("debug.az.env", "AZ_LOG=debug; AZ_BACKEND = cpu ;broken"),
    ];
    let properties = AndroidProperties::from_entries(entries);
    assert_eq!(properties.named("AZ_E2E"), Some("/data/local/tmp/x.json"));
    assert_eq!(properties.named("AZ_EMPTY"), None);
    assert_eq!(properties.named("AZ_"), None);
    assert_eq!(properties.pair("AZ_LOG"), Some("debug"));
    assert_eq!(properties.pair("AZ_BACKEND"), Some("cpu"));
    assert_eq!(
        AndroidProperties::from_listing(LISTING),
        AndroidProperties::from_entries(LISTING.lines().filter_map(split_property)),
        "a listing is its entries"
    );
}

/// The start spawns no process to read its environment: `getprop` forked and executed a binary that
/// listed every property of the phone, once per reader, and a start makes several. The reader of Android
/// asks bionic. (A source gate in the manner of the others of this tree: the file is read as text.)
///
/// Mutation: the old `Command::new("getprop")` in the reader.
#[test]
fn the_reader_of_the_environment_spawns_no_process() {
    let source = include_str!("env.rs");
    let spawn = ["Command", "::new"].concat();
    assert!(
        !source.contains(&spawn),
        "dll/src/env.rs spawns a process: every reader of the environment would pay a fork and an exec on a phone"
    );
}

#[test]
fn a_name_nobody_sets_is_unset_and_the_pairs_override_what_the_process_has() {
    let env = ProcessEnv {
        properties: AndroidProperties::from_listing(LISTING),
    };
    // the process has no variable of this name
    let never = "AZ_THE_ENV_TEST_NEVER_SETS_THIS";
    assert_eq!(env.var(never), None);
    assert!(!env.is_set(never));
    // `debug.az.e2e` answers where the process has nothing (the test environment has no AZ_E2E
    // unless a harness set one: then the process wins, which is the rule)
    match std::env::var("AZ_E2E") {
        Ok(real) => assert_eq!(env.var("AZ_E2E"), Some(real)),
        Err(_) => assert_eq!(env.var("AZ_E2E").as_deref(), Some("/data/local/tmp/x.json")),
    }
    // a pair overrides the process, as the `set_var` it replaces did
    let overriding = ProcessEnv {
        properties: AndroidProperties::from_listing("[debug.az.env]: [PATH=/pair]\n"),
    };
    assert_eq!(overriding.var("PATH").as_deref(), Some("/pair"));
    assert_eq!(
        overriding.var_os("PATH"),
        Some(std::ffi::OsString::from("/pair"))
    );
    // the process overrides a named property
    let named = ProcessEnv {
        properties: AndroidProperties {
            pairs: BTreeMap::new(),
            named: BTreeMap::from([(String::from("PATH"), String::from("/named"))]),
        },
    };
    match std::env::var("PATH") {
        Ok(real) => assert_eq!(named.var("PATH"), Some(real)),
        Err(_) => assert_eq!(named.var("PATH").as_deref(), Some("/named")),
    }
}

#[test]
fn the_reader_is_the_process_environment_on_a_desktop() {
    let env = ProcessEnv::new();
    assert_eq!(env.var("PATH"), std::env::var("PATH").ok());
    assert_eq!(env.var_os("PATH"), std::env::var_os("PATH"));
    assert!(
        !env.is_set("AZ_THE_ENV_TEST_NEVER_SETS_THIS"),
        "an unset variable is not set"
    );
}
