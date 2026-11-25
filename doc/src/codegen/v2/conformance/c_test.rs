//! What the C program compares a variant's payload with: the value that went in, never a second
//! value made the same way.

use super::*;
use crate::codegen::v2::{
    build_ir_from_api,
    ir::{CodegenIR, FunctionKind},
};

/// The IR of api.json, its conformance plan, and the C program rendered from it.
fn rendered() -> (CodegenIR, ConformancePlan, String) {
    let api = crate::api::ApiData::from_str(
        &std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../api.json")).unwrap(),
    )
    .unwrap();
    let ir = build_ir_from_api(&api).unwrap();
    let plan = ConformancePlan::build(&ir);
    let program = render(&plan);
    (ir, plan, program)
}

/// The text of the function `check_variant_<n>` of the program.
fn case_body(program: &str, n: usize) -> &str {
    let start = program
        .find(&format!("static void check_variant_{n}(void)"))
        .expect("every variant case is rendered");
    let body = &program[start..];
    &body[..body.find("\n}\n").unwrap_or(body.len())]
}

/// `OptionTimer::Some(Timer)` read back through the union failed in the first CI run that reached
/// the conformance step (2026-11-25): the program compared the payload with a SECOND `Timer` made
/// by `AzTimer_createDefault()`. A `Timer` holds a `RefAny`, and a `RefAny` is equal to another
/// only when both are the same handle (`sharing_info`): two separately made `Timer`s are never
/// equal, whatever the offset the union reads the payload at. The check asked a question with the
/// answer "no" by construction. What went in is compared with a clone of what went in.
#[test]
fn the_payload_of_optiontimer_some_is_compared_with_a_clone_of_the_timer_that_went_in() {
    let (ir, plan, program) = rendered();
    let function = |class: &str, kind: FunctionKind| {
        ir.functions
            .iter()
            .find(|f| f.class_name == class && f.kind == kind)
            .map(|f| f.c_name.clone())
    };
    let clone = function("Timer", FunctionKind::DeepCopy).expect("a Timer can be cloned");
    let make = function("Timer", FunctionKind::Default).expect("a Timer has a Default");
    let n = plan
        .variants
        .iter()
        .position(|v| v.ty == "OptionTimer" && v.variant == "Some")
        .expect("OptionTimer::Some is in the plan");
    let body = case_body(&program, n);
    assert!(
        body.contains(&format!("{clone}(&arg)")),
        "the payload is not compared with a clone of the Timer that went in:\n{body}"
    );
    assert_eq!(
        body.matches(&format!("{make}()")).count(),
        1,
        "one Timer is made, and a clone of it is the expected value:\n{body}"
    );
}

/// The same for every variant that reads a payload back with `Az<Payload>_partialEq`: when the
/// payload type can be cloned (`Az<Payload>_clone`), the expected value is a clone of the one that
/// went into the constructor, not a fresh one.
#[test]
fn every_payload_that_can_be_cloned_is_compared_with_a_clone_of_what_went_in() {
    let (ir, plan, program) = rendered();
    let mut checked = 0usize;
    let mut fresh = Vec::new();
    for (n, v) in plan.variants.iter().enumerate() {
        let Some(PayloadCheck::PartialEq { .. }) = &v.payload_check else {
            continue;
        };
        let [payload] = v.args.as_slice() else {
            continue;
        };
        let Some(clone) = ir
            .functions
            .iter()
            .find(|f| f.class_name == payload.ty() && f.kind == FunctionKind::DeepCopy)
            .map(|f| f.c_name.clone())
        else {
            continue;
        };
        checked += 1;
        if !case_body(&program, n).contains(&format!("expected = {clone}(&arg);")) {
            fresh.push(format!("{}::{}", v.ty, v.variant));
        }
    }
    assert!(checked > 0, "no variant reads a clonable payload back");
    assert!(
        fresh.is_empty(),
        "{} of {checked} variant cases compare their payload with a fresh value, not with a clone \
         of the one that went in: {}",
        fresh.len(),
        fresh.iter().take(12).cloned().collect::<Vec<_>>().join(", ")
    );
}
