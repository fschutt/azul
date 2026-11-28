//! The location: the module, the list a record belongs to, and the name under them.

use super::*;

#[test]
fn a_page_of_a_module_says_the_module_and_its_own_name() {
    let place = place_of(("Assets", 0), None, "Fixed assets");
    assert_eq!(place.parents, [(String::from("Assets"), 0)]);
    assert_eq!(place.name, "Fixed assets");
}

#[test]
fn a_record_or_a_form_says_its_list_between_the_module_and_its_name() {
    let place = place_of(("Assets", 0), Some(("Register", 3)), "ThinkPad X1 Carbon");
    assert_eq!(
        place.parents,
        [(String::from("Assets"), 0), (String::from("Register"), 3)],
        "each crumb goes to its page"
    );
    assert_eq!(place.name, "ThinkPad X1 Carbon");
}

#[test]
fn a_list_that_is_the_page_or_the_module_is_not_said_twice() {
    let same = place_of(("Assets", 0), Some(("Locations", 2)), "Locations");
    assert_eq!(same.parents.len(), 1, "{same:?}");
    let module = place_of(("Assets", 0), Some(("Assets", 0)), "Register");
    assert_eq!(module.parents.len(), 1, "{module:?}");
    let empty = place_of(("Assets", 0), Some(("", 1)), "Register");
    assert_eq!(empty.parents.len(), 1, "{empty:?}");
}

#[test]
fn the_window_says_where_it_is_on_every_page_the_views_route_to() {
    use crate::fixtures_test::erp;
    let mut s = erp();
    s.state.open("/accounting/assets");
    let register = place(&s);
    assert_eq!(register.parents.len(), 1);
    assert_eq!(register.name, "Fixed assets");

    let id = s.state.book.assets[0].id.clone();
    let name = s.state.book.assets[0].name.clone();
    s.state.open(&format!("/accounting/assets/{id}"));
    let asset = place(&s);
    assert_eq!(asset.name, name, "the record's name is the big line");
    assert_eq!(asset.parents.len(), 2, "the module and the register");
    assert_eq!(
        asset.parents[1].1,
        nav::tag(0, 0),
        "the register crumb goes to the register"
    );

    s.state.open(&format!("/accounting/assets/{id}/edit"));
    assert_eq!(place(&s).name, "Edit asset", "a form says its title");
    s.state.cancel_form();

    s.state.open("/assets/categories");
    assert_eq!(place(&s).name, "Asset categories");
    s.state.open("/assets/reports");
    assert_eq!(place(&s).name, "Asset reports");
}
