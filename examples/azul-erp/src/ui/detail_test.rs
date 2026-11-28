//! The record's page: the record's name is the location's big line.

use super::*;
use crate::fixtures_test::erp;

#[test]
fn the_title_is_the_headers_title_field_of_the_record() {
    let s = erp();
    let asset = &s.state.book.assets[0];
    let route = s
        .state
        .views
        .route(&format!("/accounting/assets/{}", asset.id))
        .unwrap();
    assert_eq!(title_of(&s, route.view, &route.params), asset.name);
}

#[test]
fn a_record_that_is_gone_says_so() {
    let s = erp();
    let route = s
        .state
        .views
        .route("/accounting/assets/no-such-record")
        .unwrap();
    assert_eq!(
        title_of(&s, route.view, &route.params),
        "The asset is not there any more."
    );
}
