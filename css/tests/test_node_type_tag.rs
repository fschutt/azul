//! Test für NodeTypeTag round-trip conversion (string -> NodeTypeTag -> string)

use azul_css::css::NodeTypeTag;

/// NodeTypeTag variants that represent real HTML elements (can be parsed from CSS selectors)
const HTML_ELEMENT_TAGS: &[NodeTypeTag] = &[
    // Document structure
    NodeTypeTag::Html,
    NodeTypeTag::Head,
    NodeTypeTag::Body,
    // Block-level elements
    NodeTypeTag::Div,
    NodeTypeTag::P,
    NodeTypeTag::Article,
    NodeTypeTag::Section,
    NodeTypeTag::Nav,
    NodeTypeTag::Aside,
    NodeTypeTag::Header,
    NodeTypeTag::Footer,
    NodeTypeTag::Main,
    NodeTypeTag::Figure,
    NodeTypeTag::FigCaption,
    // Headings
    NodeTypeTag::H1,
    NodeTypeTag::H2,
    NodeTypeTag::H3,
    NodeTypeTag::H4,
    NodeTypeTag::H5,
    NodeTypeTag::H6,
    // Inline text
    NodeTypeTag::Br,
    NodeTypeTag::Hr,
    NodeTypeTag::Pre,
    NodeTypeTag::BlockQuote,
    NodeTypeTag::Address,
    NodeTypeTag::Details,
    NodeTypeTag::Summary,
    NodeTypeTag::Dialog,
    // Lists
    NodeTypeTag::Ul,
    NodeTypeTag::Ol,
    NodeTypeTag::Li,
    NodeTypeTag::Dl,
    NodeTypeTag::Dt,
    NodeTypeTag::Dd,
    NodeTypeTag::Menu,
    NodeTypeTag::MenuItem,
    NodeTypeTag::Dir,
    // Tables
    NodeTypeTag::Table,
    NodeTypeTag::Caption,
    NodeTypeTag::THead,
    NodeTypeTag::TBody,
    NodeTypeTag::TFoot,
    NodeTypeTag::Tr,
    NodeTypeTag::Th,
    NodeTypeTag::Td,
    NodeTypeTag::ColGroup,
    NodeTypeTag::Col,
    // Forms
    NodeTypeTag::Form,
    NodeTypeTag::FieldSet,
    NodeTypeTag::Legend,
    NodeTypeTag::Label,
    NodeTypeTag::Input,
    NodeTypeTag::Button,
    NodeTypeTag::Select,
    NodeTypeTag::OptGroup,
    NodeTypeTag::SelectOption,
    NodeTypeTag::TextArea,
    NodeTypeTag::Output,
    NodeTypeTag::Progress,
    NodeTypeTag::Meter,
    NodeTypeTag::DataList,
    // Inline elements
    NodeTypeTag::Span,
    NodeTypeTag::A,
    NodeTypeTag::Em,
    NodeTypeTag::Strong,
    NodeTypeTag::B,
    NodeTypeTag::I,
    NodeTypeTag::U,
    NodeTypeTag::S,
    NodeTypeTag::Mark,
    NodeTypeTag::Del,
    NodeTypeTag::Ins,
    NodeTypeTag::Code,
    NodeTypeTag::Samp,
    NodeTypeTag::Kbd,
    NodeTypeTag::Var,
    NodeTypeTag::Cite,
    NodeTypeTag::Dfn,
    NodeTypeTag::Abbr,
    NodeTypeTag::Acronym,
    NodeTypeTag::Q,
    NodeTypeTag::Time,
    NodeTypeTag::Sub,
    NodeTypeTag::Sup,
    NodeTypeTag::Small,
    NodeTypeTag::Big,
    NodeTypeTag::Bdo,
    NodeTypeTag::Bdi,
    NodeTypeTag::Wbr,
    NodeTypeTag::Ruby,
    NodeTypeTag::Rt,
    NodeTypeTag::Rtc,
    NodeTypeTag::Rp,
    NodeTypeTag::Data,
    // Embedded content
    NodeTypeTag::Canvas,
    NodeTypeTag::Object,
    NodeTypeTag::Param,
    NodeTypeTag::Embed,
    NodeTypeTag::Audio,
    NodeTypeTag::Video,
    NodeTypeTag::Source,
    NodeTypeTag::Track,
    NodeTypeTag::Map,
    NodeTypeTag::Area,
    NodeTypeTag::Svg,
    // Metadata
    NodeTypeTag::Title,
    NodeTypeTag::Meta,
    NodeTypeTag::Link,
    NodeTypeTag::Script,
    NodeTypeTag::Style,
    NodeTypeTag::Base,
    // Special HTML elements
    NodeTypeTag::Img,
    NodeTypeTag::VirtualizedView,
];

/// NodeTypeTag variants that are NOT real HTML elements
/// These are internal types or pseudo-elements that cannot be parsed from simple CSS type selectors
const NON_HTML_ELEMENT_TAGS: &[NodeTypeTag] = &[
    NodeTypeTag::Text,        // Internal text node, not an HTML element
    NodeTypeTag::Before,      // ::before pseudo-element
    NodeTypeTag::After,       // ::after pseudo-element
    NodeTypeTag::Marker,      // ::marker pseudo-element
    NodeTypeTag::Placeholder, // ::placeholder pseudo-element
];

/// Test that NodeTypeTag::from_str correctly parses all HTML element tag names
#[test]
fn test_node_type_tag_from_str_roundtrip() {
    for &tag in HTML_ELEMENT_TAGS {
        // Convert tag to string
        let tag_str = format!("{}", tag);

        // Parse the string back to tag
        let parsed = NodeTypeTag::from_str(&tag_str);

        // Verify round-trip works
        assert!(
            parsed.is_ok(),
            "NodeTypeTag::from_str failed for '{}' (from {:?})",
            tag_str,
            tag
        );

        let parsed_tag = parsed.unwrap();
        assert_eq!(
            tag, parsed_tag,
            "Round-trip failed: {:?} -> '{}' -> {:?}",
            tag, tag_str, parsed_tag
        );
    }
}

/// Test that non-HTML element tags (pseudo-elements, text nodes) are handled correctly
/// These display with :: prefix but may or may not be parseable
#[test]
fn test_node_type_tag_non_html_elements() {
    // Text is an internal node type - doesn't need to be parseable from CSS
    // Pseudo-elements display with :: prefix

    // Just verify they can be formatted without panicking
    for &tag in NON_HTML_ELEMENT_TAGS {
        let tag_str = format!("{}", tag);
        assert!(
            !tag_str.is_empty(),
            "Tag {:?} formatted to empty string",
            tag
        );
    }
}

/// Test that common HTML tag names parse correctly
#[test]
fn test_node_type_tag_common_tags() {
    // Test commonly used tags
    let test_cases = vec![
        ("div", NodeTypeTag::Div),
        ("p", NodeTypeTag::P),
        ("span", NodeTypeTag::Span),
        ("a", NodeTypeTag::A),
        ("table", NodeTypeTag::Table),
        ("tr", NodeTypeTag::Tr),
        ("td", NodeTypeTag::Td),
        ("th", NodeTypeTag::Th),
        ("thead", NodeTypeTag::THead),
        ("tbody", NodeTypeTag::TBody),
        ("ul", NodeTypeTag::Ul),
        ("ol", NodeTypeTag::Ol),
        ("li", NodeTypeTag::Li),
        ("h1", NodeTypeTag::H1),
        ("h2", NodeTypeTag::H2),
        ("h3", NodeTypeTag::H3),
        ("img", NodeTypeTag::Img),
        ("input", NodeTypeTag::Input),
        ("button", NodeTypeTag::Button),
        ("form", NodeTypeTag::Form),
    ];

    for (tag_str, expected) in test_cases {
        let result = NodeTypeTag::from_str(tag_str);
        assert!(result.is_ok(), "Failed to parse common tag: '{}'", tag_str);
        assert_eq!(
            result.unwrap(),
            expected,
            "Wrong result for tag: '{}'",
            tag_str
        );
    }
}

/// Test that invalid tag names return an error
#[test]
fn test_node_type_tag_invalid() {
    let invalid_tags = vec![
        "invalid", "foo", "bar", "DIV", // Case sensitive - should fail
        "Div", "TABLE",
    ];

    for tag_str in invalid_tags {
        let result = NodeTypeTag::from_str(tag_str);
        assert!(
            result.is_err(),
            "Expected error for invalid tag '{}', but got {:?}",
            tag_str,
            result
        );
    }
}

/// Test pseudo-element variants
#[test]
fn test_node_type_tag_pseudo_elements() {
    // Test both with and without :: prefix
    let test_cases = vec![
        ("before", NodeTypeTag::Before),
        ("::before", NodeTypeTag::Before),
        ("after", NodeTypeTag::After),
        ("::after", NodeTypeTag::After),
        ("marker", NodeTypeTag::Marker),
        ("::marker", NodeTypeTag::Marker),
        ("placeholder", NodeTypeTag::Placeholder),
        ("::placeholder", NodeTypeTag::Placeholder),
    ];

    for (tag_str, expected) in test_cases {
        let result = NodeTypeTag::from_str(tag_str);
        assert!(
            result.is_ok(),
            "Failed to parse pseudo-element: '{}'",
            tag_str
        );
        assert_eq!(
            result.unwrap(),
            expected,
            "Wrong result for pseudo-element: '{}'",
            tag_str
        );
    }
}
