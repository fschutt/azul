//! Check 9 (rule 9 of the brief): the policy's wire form against the golden fixture of
//! `azlin-contract`. The wire form of the policy (CBOR) is `azlin-cbor`'s and S5's, and the glue unit
//! maps `PolicyPatch` to it: this crate has no codec (its dependencies are `azlin-seams` and
//! `thiserror` only, by the brief), and `azlin-contract` has no `account_policy.json` fixture yet
//! (`fixtures/c4/` lists none; drift register of lane W1-S17b). So the test is written against the
//! fixture once S5 lands it, and is `#[ignore]`d until then; what it can state today, it states: the
//! fixture file must exist and must name every kind and every field of the table.
//!
//! Implements planning/architecture.md section 5.10 (the account policy) and COMMON-W1R R6 (the
//! wire form is the codec's, the golden vectors are copied into `azlin-contract` at INT-WIRE).

mod common;

use std::fs;

use azlin_policy::{AccountKind, Field};
use common::{root, KINDS};

#[test]
#[ignore = "S5 fixture not landed"]
fn policy_cbor_matches_azlin_contract_fixture() {
    // The fixture of S5: `examples/azlin-contract/fixtures/c4/account_policy.json`.
    let path = root().join("../azlin-contract/fixtures/c4/account_policy.json");
    let text = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("the fixture {} is not there: {error}", path.display()));
    for kind in KINDS {
        assert!(
            text.contains(kind.as_str()),
            "the fixture names the kind {kind}"
        );
    }
    for field in Field::ALL {
        let leaf = field.name().rsplit('.').next().expect("a name");
        assert!(text.contains(leaf), "the fixture names the field {field}");
    }
    // The CBOR bytes of `Policy::defaults(kind)` are the fixture's golden bytes: written when the
    // codec and the schema (the glue unit) exist.
    assert_eq!(AccountKind::ALL.len(), 4);
}
