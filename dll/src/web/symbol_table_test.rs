use std::collections::HashMap;

use super::*;

fn api(shards: bool) -> ApiClasses {
    let mut by_name = HashMap::new();
    by_name.insert("AzApp_create".to_string(), ApiFnClass::Framework);
    by_name.insert("AzApp_run".to_string(), ApiFnClass::ServerEntryPoint);
    by_name.insert("AzDom_patch".to_string(), ApiFnClass::ReplaceWithDomPatcher);
    ApiClasses { by_name, shards }
}

#[test]
fn the_framework_class_is_a_boundary_import_in_sharded_mode_and_recursable_bundled() {
    assert_eq!(
        classify_for_name("AzApp_create", &api(true)),
        FnClass::BoundaryImport
    );
    assert_eq!(
        classify_for_name("AzApp_create", &api(false)),
        FnClass::Recursable
    );
}

#[test]
fn the_other_api_classes_do_not_depend_on_the_sharding() {
    for shards in [false, true] {
        let api = api(shards);
        assert_eq!(classify_for_name("AzApp_run", &api), FnClass::NeverLift);
        assert_eq!(classify_for_name("AzDom_patch", &api), FnClass::Leaf);
    }
}

#[test]
fn an_empty_api_is_the_bundled_default() {
    // what `ApiClasses::default()` is: no symbol named, the legacy bundled mode
    let none = ApiClasses::default();
    assert!(none.by_name.is_empty());
    assert!(!none.shards);
}
