//! Check 11 (rule 9 of the brief): the wire form of a request against the golden fixture of
//! `azlin-contract`. The wire form (CBOR) is `azlin-cbor`'s and S5's, and the glue unit maps a
//! request to it: this crate has no codec (its dependencies are `azlin-seams`, `azlin-policy`,
//! `blake3` and `thiserror` only, by the brief), and `azlin-contract` has no `enrollment.json`
//! fixture yet (`fixtures/c4/` lists none; drift register of lane W1-S17e). So the test is written
//! against the fixture once S5 lands it, and is `#[ignore]`d until then; what it can state today,
//! it states: the fixture file must exist and must name every path and every kind of evidence of
//! the table.
//!
//! Implements planning/architecture.md section 7.5 (the table of paths) and COMMON-W1R R6 (the wire
//! form is the codec's, the golden vectors are copied into `azlin-contract` at INT-WIRE).

mod common;

use std::fs;

use azlin_enroll::PATHS;
use common::root;

#[test]
#[ignore = "S5 fixture not landed"]
fn enrollment_request_matches_azlin_contract_fixture() {
    // The fixture of S5: `examples/azlin-contract/fixtures/c4/enrollment.json`.
    let path = root().join("../azlin-contract/fixtures/c4/enrollment.json");
    let text = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("the fixture {} is not there: {error}", path.display()));
    for (path, rule) in PATHS {
        assert!(
            text.contains(path.as_str()),
            "the fixture names the path {path}"
        );
        for kind in rule.evidence {
            let word = format!("{kind:?}");
            assert!(
                text.contains(&word),
                "the fixture names the evidence {word}"
            );
        }
    }
    // The CBOR bytes of a request opened on each path are the fixture's golden bytes: written when
    // the codec and the schema (the glue unit) exist.
    assert_eq!(PATHS.len(), 7);
}
