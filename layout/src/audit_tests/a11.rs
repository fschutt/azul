//! A11, guards and tooling.

/// The names of the fields of a `#[derive(Debug)]` struct of plain values, from its `{:?}`:
/// `Name { a: 0, b: 1 }` -> `["a", "b"]`.
#[cfg(feature = "e2e-server")]
fn debug_field_names(debug: &str) -> Vec<String> {
    let inner = match (debug.find('{'), debug.rfind('}')) {
        (Some(open), Some(close)) if open < close => &debug[open + 1..close],
        _ => return Vec::new(),
    };
    inner
        .split(',')
        .filter_map(|field| field.split(':').next())
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty())
        .collect()
}

/// A11-07: every counter of a CPU frame reaches the e2e frame report. `get_frame_report` answers
/// `cpu_last` / `cpu_since_reset` as `CpuCountersJson` (`e2e/full.rs`), a copy of
/// `CpuFrameCounters` (`cpu_counters.rs`) field by field: a counter added to the frame and not to
/// the JSON is counted and never seen by a scenario (`assert_response` on
/// `cpu_since_reset.<name>` fails with "the response has no ..."). Mutation: a field added to
/// `CpuFrameCounters` only -> the field counts differ.
#[cfg(feature = "e2e-server")]
#[test]
fn a11_every_cpu_frame_counter_is_in_the_frame_report_json() {
    let counters = crate::cpu_counters::CpuFrameCounters::default();
    let fields = debug_field_names(&format!("{counters:?}"));
    assert!(fields.len() >= 11, "the frame counters as Debug: {fields:?}");
    let json = serde_json::to_value(crate::e2e::CpuCountersJson::from(&counters))
        .expect("the counters serialise");
    let keys = json
        .as_object()
        .expect("the counters are a JSON object")
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(
        keys.len(),
        fields.len(),
        "CpuCountersJson has {keys:?}, CpuFrameCounters has {fields:?}: a counter is missing from \
         the frame report (add it to CpuCountersJson and its From in e2e/full.rs)"
    );
    for field in &fields {
        assert!(
            keys.contains(field),
            "the frame counter `{field}` is not in the frame report's JSON ({keys:?})"
        );
    }
}

/// The helper above, on the shapes it reads.
#[cfg(feature = "e2e-server")]
#[test]
fn a11_debug_field_names_reads_a_struct_of_values() {
    assert_eq!(debug_field_names("A { x: 0, yy: 12 }"), vec!["x".to_string(), "yy".to_string()]);
    assert_eq!(debug_field_names("A"), Vec::<String>::new());
    assert_eq!(debug_field_names("A {}"), Vec::<String>::new());
}
