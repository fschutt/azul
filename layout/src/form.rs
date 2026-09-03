//! Form constraint validation - the `Invalid` half of 11b-i.
//!
//! HTML's rule, and the one implemented here: on submit, every control in the
//! form is checked against its constraint attributes; if any fails, `invalid`
//! fires on it and the submission is CANCELLED. An app that wants to submit
//! anyway calls `prevent_default` on the `Invalid`, exactly as it would in a
//! browser.
//!
//! Every constraint is read from attributes that ALREADY EXISTED
//! (`Required`, `MinLength`, `MaxLength`, `Min`, `Max`) - the item said
//! validation needed "a `required` / `pattern` attribute or a validator
//! callback", and the attributes turned out to be in the DOM already.
//!
//! # What is deliberately not validated
//!
//! `Pattern` needs a regex engine and this workspace has no regex dependency.
//! Adding one for form validation is a dependency decision, not a wiring one,
//! so `Pattern` is skipped rather than approximated - a hand-rolled matcher
//! would accept and reject the wrong strings, which is worse than not
//! checking, because an app would trust it. See 11b-i-b.

use alloc::{collections::BTreeMap, string::String, vec::Vec};

use azul_core::dom::{AttributeType, DomId, DomNodeId, NodeId};

use crate::window::DomLayoutResult;

/// Why a control failed validation. Mirrors the `ValidityState` flags HTML
/// exposes, minus the ones no attribute here can produce.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidityReason {
    /// `required` and the value is empty.
    ValueMissing,
    /// Shorter than `minlength`.
    TooShort,
    /// Longer than `maxlength`.
    TooLong,
    /// Below `min`.
    RangeUnderflow,
    /// Above `max`.
    RangeOverflow,
}

/// One control that failed, and why.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidControl {
    pub node: DomNodeId,
    pub reason: ValidityReason,
}

/// Validate every control inside `form`, in document order.
///
/// `value_of` reads a control's CURRENT value; it is a callback rather than a
/// lookup here because the value lives in the text-edit pipeline, which this
/// crate's pure layer cannot reach - and because it makes the whole function
/// testable without a live window.
///
/// Returns every failure, not just the first: an app showing all errors at
/// once needs them all, and the caller decides how many to report.
pub fn validate_form(
    form: DomNodeId,
    layout_results: &BTreeMap<DomId, DomLayoutResult>,
    value_of: &dyn Fn(DomNodeId) -> Option<String>,
) -> Vec<InvalidControl> {
    let mut failures = Vec::new();
    let Some(layout) = layout_results.get(&form.dom) else {
        return failures;
    };
    let Some(form_id) = form.node.into_crate_internal() else {
        return failures;
    };
    let node_data = layout.styled_dom.node_data.as_container();
    let hierarchy = layout.styled_dom.node_hierarchy.as_container();

    // Descendants are CONTIGUOUS after the parent, which is what `subtree_len`
    // measures - so the form's controls are one index range rather than a
    // recursive walk.
    let count = hierarchy.subtree_len(form_id);
    let start = form_id.index() + 1;
    for index in start..start + count {
        let node_id = NodeId::new(index);
        let Some(data) = node_data.get(node_id) else {
            continue;
        };

        // Disabled and readonly controls are BARRED from constraint
        // validation in HTML - a disabled required field must not block a
        // submit the user cannot fix.
        let attrs = data.attributes();
        let barred = attrs
            .as_ref()
            .iter()
            .any(|a| matches!(a, AttributeType::Disabled | AttributeType::Readonly));
        if barred {
            continue;
        }

        let node = DomNodeId {
            dom: form.dom,
            node: azul_core::styled_dom::NodeHierarchyItemId::from_crate_internal(Some(node_id)),
        };
        let value = value_of(node).unwrap_or_default();

        let mut push = |reason| {
            failures.push(InvalidControl { node, reason });
        };

        for attr in attrs.as_ref() {
            match attr {
                AttributeType::Required if value.trim().is_empty() => {
                    push(ValidityReason::ValueMissing);
                }
                // `minlength` applies only to a NON-EMPTY value: an empty
                // optional field is not "too short", it is unfilled, and
                // reporting both for one empty required field would be two
                // errors for one mistake.
                AttributeType::MinLength(n) if !value.is_empty() => {
                    if usize::try_from(*n).is_ok_and(|n| value.chars().count() < n) {
                        push(ValidityReason::TooShort);
                    }
                }
                AttributeType::MaxLength(n) => {
                    if usize::try_from(*n).is_ok_and(|n| value.chars().count() > n) {
                        push(ValidityReason::TooLong);
                    }
                }
                // Range checks need BOTH sides to parse as numbers. A `min`
                // on a text field, or a non-numeric value, is not a failure -
                // the constraint simply does not apply.
                AttributeType::Min(bound) => {
                    if let (Ok(v), Ok(b)) = (value.parse::<f64>(), bound.as_str().parse::<f64>()) {
                        if v < b {
                            push(ValidityReason::RangeUnderflow);
                        }
                    }
                }
                AttributeType::Max(bound) => {
                    if let (Ok(v), Ok(b)) = (value.parse::<f64>(), bound.as_str().parse::<f64>()) {
                        if v > b {
                            push(ValidityReason::RangeOverflow);
                        }
                    }
                }
                _ => {}
            }
        }
    }
    failures
}

/// Every control in `form` paired with the value a reset restores it to.
///
/// HTML restores each control to its `value` ATTRIBUTE - the DEFAULT value,
/// not whatever it currently holds - so the "initial values to restore" the
/// original item said were missing are already in the DOM. A control with no
/// `value` attribute resets to EMPTY, which is also what HTML does.
///
/// Disabled and readonly controls are NOT skipped here, unlike in validation:
/// a reset clears a readonly field in a browser, because the reset is the
/// app's action rather than the user's.
pub fn default_values(
    form: DomNodeId,
    layout_results: &BTreeMap<DomId, DomLayoutResult>,
) -> Vec<(DomNodeId, String)> {
    let mut out = Vec::new();
    let Some(layout) = layout_results.get(&form.dom) else {
        return out;
    };
    let Some(form_id) = form.node.into_crate_internal() else {
        return out;
    };
    let node_data = layout.styled_dom.node_data.as_container();
    let hierarchy = layout.styled_dom.node_hierarchy.as_container();

    let count = hierarchy.subtree_len(form_id);
    let start = form_id.index() + 1;
    for index in start..start + count {
        let node_id = NodeId::new(index);
        let Some(data) = node_data.get(node_id) else {
            continue;
        };
        let attrs = data.attributes();
        // Only controls that can HOLD a value are reset. A form full of divs
        // would otherwise have every one of them blanked.
        let is_control = matches!(
            data.get_node_type(),
            azul_core::dom::NodeType::Input
                | azul_core::dom::NodeType::TextArea
                | azul_core::dom::NodeType::Select
        );
        if !is_control {
            continue;
        }
        let default = attrs
            .as_ref()
            .iter()
            .find_map(|a| match a {
                AttributeType::Value(v) => Some(v.as_str().to_string()),
                _ => None,
            })
            .unwrap_or_default();
        out.push((
            DomNodeId {
                dom: form.dom,
                node: azul_core::styled_dom::NodeHierarchyItemId::from_crate_internal(Some(
                    node_id,
                )),
            },
            default,
        ));
    }
    out
}

#[cfg(all(test, feature = "std"))]
mod tests {
    use azul_core::{
        dom::{Dom, NodeData, NodeType},
        styled_dom::StyledDom,
    };

    use super::*;

    /// A form whose children carry the constraint under test.
    fn form_with(attrs: Vec<Vec<AttributeType>>) -> BTreeMap<DomId, DomLayoutResult> {
        let mut form = Dom::create_node(NodeType::Form);
        for a in attrs {
            let mut data = NodeData::create_node(NodeType::Input);
            data.set_attributes(a.into());
            form = form.with_child(Dom::create_from_data(data));
        }
        let styled_dom = StyledDom::create_from_dom(Dom::create_body().with_child(form));
        let mut map = BTreeMap::new();
        map.insert(
            DomId::ROOT_ID,
            DomLayoutResult {
                styled_dom,
                layout_tree: crate::solver3::layout_tree::LayoutTree {
                    nodes: Vec::new(),
                    warm: Vec::new(),
                    cold: Vec::new(),
                    root: 0,
                    dom_to_layout: BTreeMap::new(),
                    children_arena: Vec::new(),
                    children_offsets: Vec::new(),
                    subtree_needs_intrinsic: Vec::new(),
                },
                calculated_positions: Vec::new(),
                viewport: azul_core::geom::LogicalRect::zero(),
                display_list: std::sync::Arc::new(
                    crate::solver3::display_list::DisplayList::default(),
                ),
                scroll_ids: std::collections::HashMap::new(),
                scroll_id_to_node_id: std::collections::HashMap::new(),
            },
        );
        map
    }

    fn node(index: usize) -> DomNodeId {
        DomNodeId {
            dom: DomId::ROOT_ID,
            node: azul_core::styled_dom::NodeHierarchyItemId::from_crate_internal(Some(
                NodeId::new(index),
            )),
        }
    }

    /// Node 0 = body, 1 = form, 2.. = controls.
    const FORM: usize = 1;

    #[test]
    fn a_required_control_with_no_value_is_value_missing() {
        let layouts = form_with(vec![vec![AttributeType::Required]]);
        let got = validate_form(node(FORM), &layouts, &|_| None);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].reason, ValidityReason::ValueMissing);
        assert_eq!(got[0].node, node(2));
    }

    /// WHITESPACE IS EMPTY. A field containing only spaces has not been
    /// filled in, and accepting it is the classic required-field hole.
    #[test]
    fn whitespace_does_not_satisfy_required() {
        let layouts = form_with(vec![vec![AttributeType::Required]]);
        let got = validate_form(node(FORM), &layouts, &|_| Some("   ".into()));
        assert_eq!(got.len(), 1, "whitespace must not count as a value");
    }

    #[test]
    fn a_filled_required_control_passes() {
        let layouts = form_with(vec![vec![AttributeType::Required]]);
        assert!(validate_form(node(FORM), &layouts, &|_| Some("x".into())).is_empty());
    }

    /// A DISABLED required field must not block a submit the user cannot fix -
    /// HTML bars disabled and readonly controls from validation entirely.
    #[test]
    fn disabled_and_readonly_controls_are_barred_from_validation() {
        for barred in [AttributeType::Disabled, AttributeType::Readonly] {
            let layouts = form_with(vec![vec![AttributeType::Required, barred.clone()]]);
            assert!(
                validate_form(node(FORM), &layouts, &|_| None).is_empty(),
                "{barred:?} must bar the control from validation"
            );
        }
    }

    /// `minlength` must NOT fire on an empty value: an empty optional field is
    /// unfilled, not too short, and an empty REQUIRED field would otherwise
    /// report two errors for one mistake.
    #[test]
    fn minlength_ignores_an_empty_value() {
        let layouts = form_with(vec![vec![AttributeType::MinLength(5)]]);
        assert!(validate_form(node(FORM), &layouts, &|_| Some(String::new())).is_empty());
    }

    #[test]
    fn minlength_and_maxlength_are_measured_in_characters() {
        let layouts = form_with(vec![vec![
            AttributeType::MinLength(3),
            AttributeType::MaxLength(4),
        ]]);
        // "é" is two BYTES and one character; a byte-length check would call
        // this 4 long and wrongly pass minlength on a 2-char value.
        let got = validate_form(node(FORM), &layouts, &|_| Some("éé".into()));
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].reason, ValidityReason::TooShort);

        let got = validate_form(node(FORM), &layouts, &|_| Some("ééé".into()));
        assert!(got.is_empty(), "3 characters satisfies minlength 3");

        let got = validate_form(node(FORM), &layouts, &|_| Some("ééééé".into()));
        assert_eq!(got[0].reason, ValidityReason::TooLong);
    }

    #[test]
    fn min_and_max_compare_numerically() {
        let layouts = form_with(vec![vec![
            AttributeType::Min("10".into()),
            AttributeType::Max("20".into()),
        ]]);
        assert_eq!(
            validate_form(node(FORM), &layouts, &|_| Some("9".into()))[0].reason,
            ValidityReason::RangeUnderflow
        );
        assert_eq!(
            validate_form(node(FORM), &layouts, &|_| Some("21".into()))[0].reason,
            ValidityReason::RangeOverflow
        );
        assert!(validate_form(node(FORM), &layouts, &|_| Some("15".into())).is_empty());
        // STRING comparison would call "9" greater than "20"; the numeric one
        // is the whole point of parsing both sides.
        assert!(validate_form(node(FORM), &layouts, &|_| Some("10".into())).is_empty());
    }

    /// A non-numeric value against a numeric bound is NOT a failure - the
    /// constraint simply does not apply, and reporting it would make every
    /// text field with a stray `min` permanently invalid.
    #[test]
    fn a_non_numeric_value_is_not_a_range_failure() {
        let layouts = form_with(vec![vec![AttributeType::Min("10".into())]]);
        assert!(validate_form(node(FORM), &layouts, &|_| Some("abc".into())).is_empty());
    }

    /// Every failing control is reported, not just the first: an app showing
    /// all errors at once needs them all.
    #[test]
    fn every_failing_control_is_reported() {
        let layouts = form_with(vec![
            vec![AttributeType::Required],
            vec![AttributeType::Required],
        ]);
        let got = validate_form(node(FORM), &layouts, &|_| None);
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].node, node(2));
        assert_eq!(got[1].node, node(3), "in document order");
    }

    /// A control resets to its `value` ATTRIBUTE - the default - not to
    /// whatever it currently holds. That is the whole reason the "initial
    /// values" did not need storing anywhere.
    #[test]
    fn a_control_resets_to_its_value_attribute() {
        let layouts = form_with(vec![vec![AttributeType::Value("hello".into())]]);
        let got = default_values(node(FORM), &layouts);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].0, node(2));
        assert_eq!(got[0].1, "hello");
    }

    /// No `value` attribute means reset-to-EMPTY, which is what HTML does -
    /// not "leave it alone".
    #[test]
    fn a_control_with_no_value_attribute_resets_to_empty() {
        let layouts = form_with(vec![vec![AttributeType::Required]]);
        let got = default_values(node(FORM), &layouts);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].1, "");
    }

    /// Disabled and readonly controls ARE reset, unlike in validation where
    /// they are barred: a reset is the app's action, not the user's, and a
    /// browser clears a readonly field too.
    #[test]
    fn reset_does_not_skip_disabled_or_readonly_controls() {
        for barred in [AttributeType::Disabled, AttributeType::Readonly] {
            let layouts =
                form_with(vec![vec![AttributeType::Value("v".into()), barred.clone()]]);
            assert_eq!(
                default_values(node(FORM), &layouts).len(),
                1,
                "{barred:?} must still be reset"
            );
        }
    }

    #[test]
    fn a_form_with_no_constraints_is_valid() {
        let layouts = form_with(vec![vec![], vec![]]);
        assert!(validate_form(node(FORM), &layouts, &|_| None).is_empty());
    }
}
