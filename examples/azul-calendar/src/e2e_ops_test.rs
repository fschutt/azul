use super::*;

#[test]
fn every_op_has_the_apps_prefix_a_summary_and_a_name_of_its_own() {
    let mut seen = std::collections::BTreeSet::new();
    for (name, summary) in OPS {
        assert!(name.starts_with("azcal_"), "{name}");
        assert!(!summary.is_empty(), "{name}");
        assert!(seen.insert(name), "{name} is listed twice");
    }
    assert_eq!(seen.len(), OPS.len());
}

#[test]
fn an_ops_description_is_a_json_object_with_its_name() {
    let def: serde_json::Value = serde_json::from_str(&op_def(OP_VIEW, "show a view")).unwrap();
    assert_eq!(def["name"], OP_VIEW);
    assert_eq!(def["summary"], "show a view");
}

#[test]
fn a_page_and_a_date_are_named_the_way_the_command_line_names_them() {
    assert_eq!(page_of("print"), Some(BackstagePage::Print));
    assert_eq!(page_of("calendars"), Some(BackstagePage::Calendars));
    assert_eq!(page_of("nope"), None);
    assert_eq!(date_of("2026-11-28"), NaiveDate::from_ymd_opt(2026, 11, 28));
    assert_eq!(date_of(" 2026-1-5 "), NaiveDate::from_ymd_opt(2026, 1, 5));
    assert_eq!(date_of("2026-13-01"), None);
    assert_eq!(date_of("tomorrow"), None);
    assert_eq!(date_of(""), None);
}
