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
//! # `pattern` (11b-i-b)
//!
//! Needs a regex engine, and the user chose `regex-lite` for it (minimal
//! configuration: the string API only) over a hand-rolled matcher that would
//! accept and reject the wrong strings. HTML's rules, applied here:
//!
//! * the WHOLE value must match - the pattern is compiled as `^(?:p)$`, so
//!   `pattern="[0-9]{3}"` rejects `1234` even though it contains three digits;
//! * an empty value is exempt (that is `required`'s job);
//! * a pattern that does not compile is IGNORED, not treated as a mismatch - a
//!   browser does the same, and failing every submit over a typo in an
//!   attribute would be the worse behaviour.
//!
//! Known deviation: HTML compiles patterns with the `v` (unicode sets) flag.
//! `regex-lite` is Unicode-aware for `.`, `\w` and classes but has no `\p{..}`
//! properties or set operations; such a pattern fails to compile and is
//! therefore ignored rather than mis-matched.

use alloc::{collections::BTreeMap, string::String, vec::Vec};

use azul_core::{
    dom::{AttributeType, DomId, DomNodeId, NodeId},
    form::{ValidityReason, ValidityState},
};

use crate::window::DomLayoutResult;

/// One control that failed, and EVERY constraint it failed.
///
/// `ValidityReason` and `ValidityState` moved to `azul_core::form` when
/// 11b-i-c exposed them: a type an app reads has to live where the FFI can
/// reach it. They are re-exported here so this module still reads as one
/// piece.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidControl {
    pub node: DomNodeId,
    /// Every constraint this control failed, not just the first.
    ///
    /// It used to be one `reason` per entry, which meant a field failing two
    /// constraints produced TWO entries and therefore two `Invalid` events on
    /// one node. HTML fires `invalid` once per control, and an app marking
    /// fields would have marked one twice and mis-counted its error list.
    pub state: ValidityState,
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

        // ONE entry per control, accumulating every failed constraint. See
        // `InvalidControl::state`.
        let mut state = ValidityState::valid();
        let mut push = |reason| {
            state.insert(reason);
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
                // `pattern` applies only to a NON-EMPTY value, like `minlength`:
                // an empty optional field is unfilled, not mismatched.
                AttributeType::Pattern(pattern) if !value.is_empty() => {
                    if pattern_matches(pattern.as_str(), &value) == Some(false) {
                        push(ValidityReason::PatternMismatch);
                    }
                }
                _ => {}
            }
        }
        drop(push);
        if !state.is_valid() {
            failures.push(InvalidControl { node, state });
        }
    }
    failures
}

/// Does `value` match the HTML `pattern` `pattern` as a whole?
///
/// `None` when the pattern does not compile, which the caller treats as "no
/// constraint" - HTML ignores an invalid `pattern` attribute rather than
/// failing the field. The anchoring is HTML's own recipe (`^(?:p)$`), which
/// also means an unbalanced `)` in the pattern cannot escape the group: it
/// makes the whole expression invalid, and so ignored.
#[must_use]
pub fn pattern_matches(pattern: &str, value: &str) -> Option<bool> {
    let anchored = alloc::format!("^(?:{pattern})$");
    regex_lite::Regex::new(&anchored)
        .ok()
        .map(|re| re.is_match(value))
}

/// What a control is FOR, so a soft keyboard can show the right layout.
///
/// This is HTML's `type` attribute, which `AttributeType::InputType` already
/// carries - the 10a-iii note said an input-purpose attribute was needed
/// "first, which is a design question", and it was already in the DOM.
///
/// The values are a deliberate SUBSET: only purposes that change what a
/// keyboard shows. `type="checkbox"` has no keyboard, so it is `Text` here
/// rather than a variant nobody would branch on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(i32)]
pub enum InputPurpose {
    /// Ordinary text - the platform default, and what an unset or unknown
    /// `type` resolves to. Per the user's ruling: default to the platform
    /// default rather than guessing.
    #[default]
    Text = 0,
    /// Digits only (`type="number"`).
    Number = 1,
    /// Digits plus a decimal separator.
    Decimal = 2,
    /// A phone pad (`type="tel"`).
    Phone = 3,
    /// Text with `@` and `.` promoted (`type="email"`).
    Email = 4,
    /// Text with `/` and `.com` promoted (`type="url"`).
    Url = 5,
    /// Obscured, and excluded from autocorrect and learning
    /// (`type="password"`).
    Password = 6,
    /// A Search key instead of Enter (`type="search"`).
    Search = 7,
}

/// The purpose of one control, from its `type` attribute.
///
/// Unknown and absent both give `Text`: a `type` this build does not know is
/// not a reason to show the wrong keyboard.
#[must_use]
pub fn input_purpose(
    node: DomNodeId,
    layout_results: &BTreeMap<DomId, DomLayoutResult>,
) -> InputPurpose {
    let Some(layout) = layout_results.get(&node.dom) else {
        return InputPurpose::Text;
    };
    let Some(id) = node.node.into_crate_internal() else {
        return InputPurpose::Text;
    };
    let node_data = layout.styled_dom.node_data.as_container();
    let Some(data) = node_data.get(id) else {
        return InputPurpose::Text;
    };
    for attr in data.attributes().as_ref() {
        if let AttributeType::InputType(t) = attr {
            let t = t.as_str();
            // ASCII-case-insensitive, as HTML attribute values are.
            return if t.eq_ignore_ascii_case("number") {
                InputPurpose::Number
            } else if t.eq_ignore_ascii_case("decimal") {
                InputPurpose::Decimal
            } else if t.eq_ignore_ascii_case("tel") {
                InputPurpose::Phone
            } else if t.eq_ignore_ascii_case("email") {
                InputPurpose::Email
            } else if t.eq_ignore_ascii_case("url") {
                InputPurpose::Url
            } else if t.eq_ignore_ascii_case("password") {
                InputPurpose::Password
            } else if t.eq_ignore_ascii_case("search") {
                InputPurpose::Search
            } else {
                InputPurpose::Text
            };
        }
    }
    InputPurpose::Text
}

/// Whether a control accepts newlines, which decides whether the Enter key is
/// a line break or an action key (Done / Go / Search).
///
/// `TextArea` is the multiline control; everything else is single-line. The
/// Android bridge hardcoded MULTI_LINE for every field, so a single-line input
/// showed a newline key and had no way to dismiss the keyboard.
#[must_use]
pub fn is_multiline(
    node: DomNodeId,
    layout_results: &BTreeMap<DomId, DomLayoutResult>,
) -> bool {
    let Some(layout) = layout_results.get(&node.dom) else {
        return false;
    };
    let Some(id) = node.node.into_crate_internal() else {
        return false;
    };
    layout
        .styled_dom
        .node_data
        .as_container()
        .get(id)
        .is_some_and(|d| {
            matches!(d.get_node_type(), azul_core::dom::NodeType::TextArea)
                || d.attributes()
                    .as_ref()
                    .iter()
                    .any(|a| matches!(a, AttributeType::ContentEditable(true)))
        })
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
        assert!(got[0].state.has(ValidityReason::ValueMissing));
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
        assert!(got[0].state.has(ValidityReason::TooShort));

        let got = validate_form(node(FORM), &layouts, &|_| Some("ééé".into()));
        assert!(got.is_empty(), "3 characters satisfies minlength 3");

        let got = validate_form(node(FORM), &layouts, &|_| Some("ééééé".into()));
        assert!(got[0].state.has(ValidityReason::TooLong));
    }

    /// ONE ENTRY PER CONTROL, however many constraints it breaks.
    ///
    /// This used to be one entry per failed CONSTRAINT, so a field that was
    /// both too short and out of range produced two entries - and therefore
    /// two `Invalid` events on one node. HTML fires `invalid` once per
    /// control, and an app marking bad fields would have marked this one
    /// twice and reported two errors for one field.
    #[test]
    fn a_control_failing_two_constraints_is_reported_once_with_both() {
        let layouts = form_with(vec![vec![
            AttributeType::MinLength(5),
            AttributeType::Max("10".into()),
        ]]);
        // "99" is 2 characters (too short) AND numerically above 10.
        let got = validate_form(node(FORM), &layouts, &|_| Some("99".into()));
        assert_eq!(got.len(), 1, "one control must produce one entry, got {got:?}");
        assert!(got[0].state.has(ValidityReason::TooShort));
        assert!(got[0].state.has(ValidityReason::RangeOverflow));
        assert!(!got[0].state.has(ValidityReason::ValueMissing));
    }

    #[test]
    fn min_and_max_compare_numerically() {
        let layouts = form_with(vec![vec![
            AttributeType::Min("10".into()),
            AttributeType::Max("20".into()),
        ]]);
        assert!(validate_form(node(FORM), &layouts, &|_| Some("9".into()))[0]
            .state
            .has(ValidityReason::RangeUnderflow));
        assert!(validate_form(node(FORM), &layouts, &|_| Some("21".into()))[0]
            .state
            .has(ValidityReason::RangeOverflow));
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

    #[test]
    fn every_html_type_maps_to_its_purpose() {
        for (html, expected) in [
            ("number", InputPurpose::Number),
            ("decimal", InputPurpose::Decimal),
            ("tel", InputPurpose::Phone),
            ("email", InputPurpose::Email),
            ("url", InputPurpose::Url),
            ("password", InputPurpose::Password),
            ("search", InputPurpose::Search),
            ("text", InputPurpose::Text),
        ] {
            let layouts = form_with(vec![vec![AttributeType::InputType(html.into())]]);
            assert_eq!(input_purpose(node(2), &layouts), expected, "type={html}");
        }
    }

    /// HTML attribute values are ASCII-case-insensitive, and real documents
    /// contain `type="EMAIL"`.
    #[test]
    fn the_type_attribute_is_case_insensitive() {
        let layouts = form_with(vec![vec![AttributeType::InputType("EMAIL".into())]]);
        assert_eq!(input_purpose(node(2), &layouts), InputPurpose::Email);
    }

    /// An absent or unrecognised `type` gives the PLATFORM DEFAULT, per the
    /// ruling - not a guess, and not a failure. A `type` a future HTML adds
    /// must not produce the wrong keyboard.
    #[test]
    fn an_unknown_or_absent_type_defaults_to_text() {
        let layouts = form_with(vec![vec![]]);
        assert_eq!(input_purpose(node(2), &layouts), InputPurpose::Text);

        let layouts = form_with(vec![vec![AttributeType::InputType("colour-picker".into())]]);
        assert_eq!(input_purpose(node(2), &layouts), InputPurpose::Text);
    }

    /// The discriminants are a WIRE FORMAT: `NativeTextBridge.java` switches on
    /// these exact integers. Renumbering the enum would silently give every
    /// field the wrong keyboard - the same hazard as the sensor codes, and it
    /// gets the same guard.
    #[test]
    fn the_purpose_discriminants_are_the_jni_wire_codes() {
        for (purpose, code) in [
            (InputPurpose::Text, 0),
            (InputPurpose::Number, 1),
            (InputPurpose::Decimal, 2),
            (InputPurpose::Phone, 3),
            (InputPurpose::Email, 4),
            (InputPurpose::Url, 5),
            (InputPurpose::Password, 6),
            (InputPurpose::Search, 7),
        ] {
            assert_eq!(
                purpose as i32, code,
                "{purpose:?} moved; NativeTextBridge.java still switches on {code}"
            );
        }
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

    /// THE WHOLE VALUE MUST MATCH (HTML anchors the pattern): three digits
    /// means exactly three, not "contains three".
    #[test]
    fn pattern_must_match_the_whole_value() {
        let layouts = form_with(vec![vec![AttributeType::Pattern("[0-9]{3}".into())]]);
        assert!(validate_form(node(FORM), &layouts, &|_| Some("123".into())).is_empty());
        for bad in ["1234", "12", "x123", "123x"] {
            let got = validate_form(node(FORM), &layouts, &|_| Some(bad.into()));
            assert_eq!(got.len(), 1, "{bad:?} must not match an anchored [0-9]{{3}}");
            assert!(got[0].state.has(ValidityReason::PatternMismatch));
        }
    }

    /// An empty value is `required`'s business, not `pattern`'s.
    #[test]
    fn pattern_ignores_an_empty_value() {
        let layouts = form_with(vec![vec![AttributeType::Pattern("[0-9]+".into())]]);
        assert!(validate_form(node(FORM), &layouts, &|_| Some(String::new())).is_empty());
    }

    /// A pattern that does not compile is IGNORED, as a browser ignores an
    /// invalid `pattern` attribute - not treated as "nothing matches".
    #[test]
    fn an_uncompilable_pattern_is_ignored_like_html() {
        for broken in ["(", "[0-9", "a)b"] {
            let layouts = form_with(vec![vec![AttributeType::Pattern(broken.into())]]);
            assert!(
                validate_form(node(FORM), &layouts, &|_| Some("anything".into())).is_empty(),
                "{broken:?} must be ignored, not fail the field"
            );
        }
        assert_eq!(pattern_matches("(", "x"), None);
    }

    /// `.` is a CHARACTER, not a byte - "\u{e9}a" is two characters.
    #[test]
    fn pattern_matching_is_unicode_aware() {
        assert_eq!(pattern_matches(".{2}", "\u{e9}a"), Some(true));
        assert_eq!(pattern_matches(".{3}", "\u{e9}a"), Some(false));
    }
}
