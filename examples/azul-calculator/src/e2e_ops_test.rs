use super::*;

#[test]
fn an_ops_description_is_json_that_names_it() {
    let v: serde_json::Value = serde_json::from_str(&op_def(OP_TYPE, "types", serde_json::json!([{ "name": "text" }])))
        .expect("valid JSON");
    assert_eq!(v["name"], OP_TYPE);
    assert_eq!(v["args"][0]["name"], "text");
}

#[test]
fn the_ops_are_the_apps_own_and_differ() {
    assert!(OP_TYPE.starts_with("azcalculator_"));
    assert!(OP_LOAD_SAMPLE.starts_with("azcalculator_"));
    assert_ne!(OP_TYPE, OP_LOAD_SAMPLE);
}

#[test]
fn the_text_of_a_step_is_read_from_its_arguments() {
    assert_eq!(typed_text("{\"text\":\"12*7=\"}"), "12*7=");
    assert_eq!(typed_text("{}"), "");
    assert_eq!(typed_text("not json"), "");
    assert_eq!(typed_text("{\"text\":5}"), "", "a number is no text");
}

#[test]
fn a_typed_calculation_is_the_keyboards() {
    let mut calc = crate::calc::Calculator::new();
    assert_eq!(calc.type_text("12*7=", 1), 5);
    assert_eq!(calc.result_line(), "84");
}
