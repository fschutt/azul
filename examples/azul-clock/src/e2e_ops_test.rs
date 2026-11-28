use super::*;

#[test]
fn an_ops_description_is_json_that_names_it() {
    for name in [OP_LOAD_SAMPLE, OP_RING] {
        let v: serde_json::Value = serde_json::from_str(&op_def(name, "what it does")).expect("valid JSON");
        assert_eq!(v["name"], name);
        assert_eq!(v["summary"], "what it does");
        assert!(v["args"].is_array());
        assert!(v["examples"].is_array());
    }
}

#[test]
fn the_ops_are_the_apps_own_and_differ() {
    assert!(OP_LOAD_SAMPLE.starts_with("azclock_"));
    assert!(OP_RING.starts_with("azclock_"));
    assert_ne!(OP_LOAD_SAMPLE, OP_RING);
}
