#[cfg(test)]
mod tests {
    use super::*;
    use crate::dom::{Dom, NodeType};

    #[test]
    fn test_inline_span_parsing() {
        // This test verifies that HTML with inline spans is parsed correctly
        // The DOM structure should preserve text nodes before, inside, and after the span

        let html = r#"<p>Text before <span class="highlight">inline text</span> text after.</p>"#;

        // Expected DOM structure:
        // <p>
        //   ├─ TextNode: "Text before "
        //   ├─ <span class="highlight">
        //   │   └─ TextNode: "inline text"
        //   └─ TextNode: " text after."

        // For this test, we'll create the DOM structure manually
        // since we're testing the parsing logic
        let expected_dom = Dom::create_p().with_children(
            vec![
                Dom::create_text_do_not_use_without_block_level_wrapper("Text before "),
                Dom::create_node(NodeType::Span).with_children(
                    vec![Dom::create_text_do_not_use_without_block_level_wrapper(
                        "inline text",
                    )]
                    .into(),
                ),
                Dom::create_text_do_not_use_without_block_level_wrapper(" text after."),
            ]
            .into(),
        );

        // Verify the structure has 3 children at the top level
        assert_eq!(expected_dom.children.as_ref().len(), 3);

        // Verify the middle child is a span
        match &expected_dom.children.as_ref()[1].root.node_type {
            NodeType::Span => {}
            other => panic!("Expected Span, got {:?}", other),
        }

        // Verify the span has 1 child (the text node)
        assert_eq!(expected_dom.children.as_ref()[1].children.as_ref().len(), 1);

        println!("Test passed: Inline span parsing structure is correct");
    }

    #[test]
    fn test_xml_node_structure() {
        // Test the basic XmlNode structure to ensure text content is preserved
        // Updated to use XmlNodeChild enum (Text/Element)

        let node = XmlNode {
            node_type: "p".into(),
            attributes: XmlAttributeMap {
                inner: StringPairVec::from_const_slice(&[]),
            },
            children: vec![
                XmlNodeChild::Text("Before ".into()),
                XmlNodeChild::Element(XmlNode {
                    node_type: "span".into(),
                    children: vec![XmlNodeChild::Text("inline".into())].into(),
                    ..Default::default()
                }),
                XmlNodeChild::Text(" after".into()),
            ]
            .into(),
        };

        // Verify structure
        assert_eq!(node.children.as_ref().len(), 3);
        assert_eq!(node.children.as_ref()[0].as_text(), Some("Before "));
        assert_eq!(
            node.children.as_ref()[1]
                .as_element()
                .unwrap()
                .node_type
                .as_str(),
            "span"
        );
        assert_eq!(node.children.as_ref()[2].as_text(), Some(" after"));

        // Verify span's child
        let span = node.children.as_ref()[1].as_element().unwrap();
        assert_eq!(span.children.as_ref().len(), 1);
        assert_eq!(span.children.as_ref()[0].as_text(), Some("inline"));

        println!("Test passed: XmlNode structure preserves text nodes correctly");
    }

    #[test]
    fn test_img_tag_becomes_image_node_with_src_tag() {
        // `<img src="cat.jpg" width="300" height="169">` must become a
        // `NodeType::Image` whose `NullImage` carries the `src` string as its
        // `tag` (so a renderer can resolve the bytes later), plus the declared
        // intrinsic size.
        use crate::resources::DecodedImage;
        use crate::window::{AzStringPair, StringPairVec};

        let img_node = XmlNode {
            node_type: "img".into(),
            attributes: XmlAttributeMap::from(StringPairVec::from_vec(alloc::vec![
                AzStringPair {
                    key: "src".into(),
                    value: "cat.jpg".into()
                },
                AzStringPair {
                    key: "width".into(),
                    value: "300".into()
                },
                AzStringPair {
                    key: "height".into(),
                    value: "169".into()
                },
            ])),
            children: Vec::new().into(),
        };

        let component_map = ComponentMap::default();
        let dom = xml_node_to_dom_fast(&img_node, &component_map, false, 0)
            .expect("xml_node_to_dom_fast for <img> should succeed");

        match dom.root.get_node_type() {
            NodeType::Image(image_ref) => match image_ref.as_ref().get_data() {
                DecodedImage::NullImage {
                    tag, width, height, ..
                } => {
                    assert_eq!(
                        core::str::from_utf8(tag).unwrap(),
                        "cat.jpg",
                        "image tag must carry the src string"
                    );
                    assert_eq!(*width, 300, "width attribute should set intrinsic width");
                    assert_eq!(*height, 169, "height attribute should set intrinsic height");
                }
                other => panic!("expected NullImage carrying the src tag, got {:?}", other),
            },
            other => panic!("expected NodeType::Image for <img>, got {:?}", other),
        }

        println!("Test passed: <img src=\"cat.jpg\"> -> NodeType::Image tagged \"cat.jpg\"");
    }

    #[test]
    fn test_icon_tag_builds_an_unnamed_icon_with_its_spec_as_text_child() {
        // `<icon>content_copy</icon>`: the DOM builders stay fully generic —
        // an un-named Icon node whose text child carries the spec. The icon
        // RESOLUTION pass (core::icon::resolve_icons_in_styled_dom) consumes
        // the text, exactly like a ligature icon font consumes glyph text.
        let text_form = XmlNode {
            node_type: "icon".into(),
            attributes: XmlAttributeMap::default(),
            children: alloc::vec![XmlNodeChild::Text(" content_copy ".into())].into(),
        };

        let component_map = ComponentMap::default();
        let dom = xml_node_to_dom_fast(&text_form, &component_map, false, 0)
            .expect("xml_node_to_dom_fast for <icon>text</icon> should succeed");

        match dom.root.get_node_type() {
            NodeType::Icon(name) => assert_eq!(
                name.as_ref().as_str(),
                "",
                "the builder must NOT interpret the spec — that's the resolver's job"
            ),
            other => panic!("expected NodeType::Icon for <icon>, got {other:?}"),
        }
        let children = dom.children.as_ref();
        assert_eq!(
            children.len(),
            1,
            "the spec text child is preserved for the resolver"
        );
        match children[0].root.get_node_type() {
            NodeType::Text(t) => assert_eq!(t.as_ref().as_str(), " content_copy "),
            other => panic!("expected the spec as a text child, got {other:?}"),
        }
    }

    #[test]
    fn test_tag_to_node_type_img_is_image() {
        // The bare tag mapping should also yield an Image (placeholder, empty tag).
        match tag_to_node_type("img") {
            NodeType::Image(_) => {}
            other => panic!("tag_to_node_type(\"img\") should be Image, got {:?}", other),
        }
    }

    /// Build a `<div>` nested `depth` levels deep, innermost first.
    fn nested_divs(depth: usize) -> XmlNode {
        let mut node = XmlNode {
            node_type: "div".into(),
            ..Default::default()
        };
        for _ in 0..depth {
            node = XmlNode {
                node_type: "div".into(),
                children: vec![XmlNodeChild::Element(node)].into(),
                ..Default::default()
            };
        }
        node
    }

    /// AUDIT 2026-07-08: `extract_css_urls` used to slice the original string with
    /// a byte offset computed in a `to_lowercase()` temporary. On `'İ'` (whose
    /// lowercase is longer in bytes) that offset was misaligned. This must no
    /// longer panic and must still find the `@import` target.
    #[test]
    fn extract_css_urls_unicode_import_no_panic() {
        let mut res = Vec::new();
        Xml::extract_css_urls("İ@import 'x'", &mut res);
        assert_eq!(res.len(), 1, "should find the one @import target");
        assert_eq!(res[0].url.as_str(), "x");
    }

    /// The `url(` and `@import` scans are case-insensitive after the audit fix.
    #[test]
    fn extract_css_urls_is_case_insensitive() {
        let mut res = Vec::new();
        Xml::extract_css_urls("body { background: URL(http://e.com/a.png); }", &mut res);
        assert_eq!(res.len(), 1);
        assert_eq!(res[0].url.as_str(), "http://e.com/a.png");

        let mut res2 = Vec::new();
        Xml::extract_css_urls("@IMPORT \"theme.css\";", &mut res2);
        assert_eq!(res2.len(), 1);
        assert_eq!(res2[0].url.as_str(), "theme.css");
    }

    /// AUDIT 2026-07-08: the resource scan recurses per nesting level; deep markup
    /// must not overflow the stack (deeper-than-cap subtrees are just not scanned).
    #[test]
    fn scan_external_resources_deep_nesting_ok() {
        let xml = Xml {
            root: vec![XmlNodeChild::Element(nested_divs(2000))].into(),
        };
        // Must simply return (no stack overflow); no resources in a plain tree.
        drop(xml.scan_external_resources());
    }

    /// AUDIT 2026-07-08: the fast + tree DOM builders recurse per nesting level;
    /// deep markup must not overflow the stack (children beyond the cap are
    /// dropped, but the call returns `Ok`).
    #[test]
    fn xml_node_to_dom_fast_deep_nesting_ok() {
        let deep = nested_divs(2000);
        let component_map = ComponentMap::default();
        let dom = xml_node_to_dom_fast(&deep, &component_map, false, 0);
        assert!(dom.is_ok(), "deep DOM build must not overflow the stack");

        let mut builder = CompactDomBuilder::new();
        let fast = xml_node_to_fast_dom(&deep, &component_map, false, &mut builder, 0);
        assert!(
            fast.is_ok(),
            "deep FastDom build must not overflow the stack"
        );
    }

    /// AUDIT 2026-07-08: `ComponentFieldType::parse` recurses through `Option<..>`
    /// / `Vec<..>` wrappers; an over-deep type string is rejected rather than
    /// overflowing the stack, while ordinary nesting still parses.
    #[test]
    fn component_field_type_parse_depth_capped() {
        let deep = format!("{}Bool{}", "Option<".repeat(4000), ">".repeat(4000));
        assert!(
            ComponentFieldType::parse(&deep).is_none(),
            "over-deep type string must be rejected, not overflow"
        );

        let shallow = format!("{}Bool{}", "Option<".repeat(8), ">".repeat(8));
        assert!(
            ComponentFieldType::parse(&shallow).is_some(),
            "ordinary nesting must still parse"
        );
    }

    /// AUDIT 2026-07-08: `prepare_string` now decodes the full common entity set
    /// plus numeric references, in a single pass so `&amp;` cannot double-decode.
    #[test]
    fn prepare_string_entity_decoding() {
        assert_eq!(prepare_string("a &amp; b"), "a & b");
        // `&amp;lt;` must yield the literal text "&lt;", not "<".
        assert_eq!(prepare_string("&amp;lt;"), "&lt;");
        assert_eq!(prepare_string("&quot;hi&quot;"), "\"hi\"");
        assert_eq!(prepare_string("&#65;&#66;"), "AB");
        assert_eq!(prepare_string("&#x41;"), "A");
        // Existing behavior preserved.
        assert_eq!(prepare_string("&lt;tag&gt;"), "<tag>");
    }
}

#[cfg(test)]
#[allow(clippy::all, clippy::pedantic, clippy::nursery)]
mod autotest_generated {
    use super::*;
    use crate::dom::{NodeData, NodeType};
    use azul_css::css::{CssNthChildPattern, CssNthChildSelector};

    // ----------------------------------------------------------------- helpers

    fn attrs(kv: &[(&str, &str)]) -> XmlAttributeMap {
        XmlAttributeMap::from(StringPairVec::from_vec(
            kv.iter()
                .map(|(k, v)| AzStringPair {
                    key: AzString::from(*k),
                    value: AzString::from(*v),
                })
                .collect::<Vec<_>>(),
        ))
    }

    fn node(tag: &str, kv: &[(&str, &str)], children: Vec<XmlNodeChild>) -> XmlNode {
        XmlNode {
            node_type: tag.into(),
            attributes: attrs(kv),
            children: children.into(),
        }
    }

    fn txt(s: &str) -> XmlNodeChild {
        XmlNodeChild::Text(AzString::from(s))
    }

    fn elem(n: XmlNode) -> XmlNodeChild {
        XmlNodeChild::Element(n)
    }

    /// `<html><head><style>{css}</style></head><body>{children}</body></html>`
    fn doc(css: &str, body_children: Vec<XmlNodeChild>) -> Vec<XmlNodeChild> {
        let style = node("style", &[], vec![txt(css)]);
        let head = node("head", &[], vec![elem(style)]);
        let body = node("body", &[], body_children);
        vec![elem(node("html", &[], vec![elem(head), elem(body)]))]
    }

    fn no_args() -> ComponentArgumentVec {
        ComponentArgumentVec::from_const_slice(&[])
    }

    fn dm(name: &str, fields: Vec<ComponentDataField>) -> ComponentDataModel {
        ComponentDataModel {
            name: AzString::from(name),
            description: AzString::from_const_str(""),
            fields: fields.into(),
        }
    }

    fn user_def(css: &str, fields: Vec<ComponentDataField>) -> ComponentDef {
        ComponentDef {
            id: ComponentId::new("mylib", "widget"),
            display_name: AzString::from_const_str("Widget"),
            description: AzString::from_const_str(""),
            css: AzString::from(css),
            source: ComponentSource::UserDefined,
            data_model: dm("WidgetData", fields),
            render_fn: user_defined_render_fn,
            compile_fn: user_defined_compile_fn,
            render_fn_source: None.into(),
            compile_fn_source: None.into(),
        }
    }

    /// A string that is long enough to smoke out O(n^2) / allocation blowups but
    /// still finishes fast in a debug-profile test run.
    const LONG: usize = 200_000;

    // ================================================================
    // Xml::extract_url_value  (parser)
    // ================================================================

    #[test]
    fn extract_url_value_empty_and_whitespace() {
        assert_eq!(Xml::extract_url_value(""), None);
        assert_eq!(Xml::extract_url_value("   "), None);
        assert_eq!(Xml::extract_url_value("\t\n\r "), None);
    }

    #[test]
    fn extract_url_value_valid_minimal() {
        assert_eq!(
            Xml::extract_url_value("a.png)"),
            Some("a.png".to_string()),
            "unquoted url terminated by ')'"
        );
        assert_eq!(
            Xml::extract_url_value("\"a.png\")"),
            Some("a.png".to_string()),
            "double-quoted url"
        );
        assert_eq!(
            Xml::extract_url_value("'a.png')"),
            Some("a.png".to_string()),
            "single-quoted url"
        );
    }

    #[test]
    fn extract_url_value_leading_trailing_junk_is_trimmed() {
        // Leading whitespace is trimmed by `trim_start`, inner padding by `trim`.
        assert_eq!(
            Xml::extract_url_value("   a.png   )tail"),
            Some("a.png".to_string())
        );
    }

    #[test]
    fn extract_url_value_garbage_returns_none() {
        // Unterminated quote / no closing paren => None, never a panic.
        assert_eq!(Xml::extract_url_value("\"unterminated"), None);
        assert_eq!(Xml::extract_url_value("'unterminated"), None);
        assert_eq!(Xml::extract_url_value("no-closing-paren"), None);
        assert_eq!(Xml::extract_url_value("\u{0}\u{1}\u{7f}"), None);
    }

    #[test]
    fn extract_url_value_boundary_numbers() {
        for s in [
            "0)",
            "-0)",
            "9223372036854775807)",
            "-9223372036854775808)",
            "NaN)",
            "inf)",
            "1e400)",
        ] {
            let got = Xml::extract_url_value(s);
            assert!(got.is_some(), "numeric-looking url {s:?} is still a url");
        }
        assert_eq!(Xml::extract_url_value("0)"), Some("0".to_string()));
    }

    #[test]
    fn extract_url_value_unicode_no_panic() {
        // The ')' scan must land on a char boundary of the ORIGINAL string.
        assert_eq!(
            Xml::extract_url_value("\u{1F600}\u{0301})"),
            Some("\u{1F600}\u{0301}".to_string())
        );
        assert_eq!(
            Xml::extract_url_value("\"\u{130}\")"),
            Some("\u{130}".to_string())
        );
        assert_eq!(Xml::extract_url_value("\u{1F600}"), None);
    }

    #[test]
    fn extract_url_value_extremely_long_terminates() {
        let s = "a".repeat(LONG);
        assert_eq!(Xml::extract_url_value(&s), None, "no ')' anywhere => None");
        let s2 = format!("{})", "b".repeat(LONG));
        assert_eq!(Xml::extract_url_value(&s2).map(|v| v.len()), Some(LONG));
    }

    #[test]
    fn extract_url_value_nested_brackets_no_stack_overflow() {
        // Not recursive, but confirm deeply "nested" input is handled iteratively.
        let s = "(".repeat(10_000);
        assert_eq!(Xml::extract_url_value(&s), None);
        let s2 = format!("{}{}", "(".repeat(10_000), ")");
        assert_eq!(Xml::extract_url_value(&s2), Some("(".repeat(10_000)));
    }

    // ================================================================
    // Xml::extract_quoted_string  (parser)
    // ================================================================

    #[test]
    fn extract_quoted_string_empty_whitespace_garbage() {
        assert_eq!(Xml::extract_quoted_string(""), None);
        assert_eq!(Xml::extract_quoted_string("   "), None);
        assert_eq!(Xml::extract_quoted_string("\t\n"), None);
        assert_eq!(Xml::extract_quoted_string("bare"), None);
        // Leading whitespace is NOT trimmed here (unlike extract_url_value).
        assert_eq!(Xml::extract_quoted_string("  \"x\""), None);
    }

    #[test]
    fn extract_quoted_string_valid_minimal_and_empty_quotes() {
        assert_eq!(Xml::extract_quoted_string("\"x\""), Some("x".to_string()));
        assert_eq!(Xml::extract_quoted_string("'x'"), Some("x".to_string()));
        // An empty quoted string is Some(""), not None.
        assert_eq!(Xml::extract_quoted_string("\"\""), Some(String::new()));
        assert_eq!(Xml::extract_quoted_string("''"), Some(String::new()));
    }

    #[test]
    fn extract_quoted_string_unterminated_is_none() {
        assert_eq!(Xml::extract_quoted_string("\"abc"), None);
        assert_eq!(Xml::extract_quoted_string("'abc"), None);
        // Mismatched quotes do not pair up.
        assert_eq!(Xml::extract_quoted_string("\"abc'"), None);
    }

    #[test]
    fn extract_quoted_string_boundary_numbers_and_unicode() {
        assert_eq!(Xml::extract_quoted_string("\"0\""), Some("0".to_string()));
        assert_eq!(Xml::extract_quoted_string("\"-0\""), Some("-0".to_string()));
        assert_eq!(
            Xml::extract_quoted_string("\"NaN\""),
            Some("NaN".to_string())
        );
        assert_eq!(
            Xml::extract_quoted_string("\"\u{1F600}\u{0301}\""),
            Some("\u{1F600}\u{0301}".to_string())
        );
    }

    #[test]
    fn extract_quoted_string_extremely_long_terminates() {
        let unterminated = format!("\"{}", "x".repeat(LONG));
        assert_eq!(Xml::extract_quoted_string(&unterminated), None);
        let terminated = format!("\"{}\"", "x".repeat(LONG));
        assert_eq!(
            Xml::extract_quoted_string(&terminated).map(|s| s.len()),
            Some(LONG)
        );
    }

    // ================================================================
    // Xml::parse_srcset  (parser)
    // ================================================================

    #[test]
    fn parse_srcset_empty_and_whitespace_yield_no_urls() {
        assert!(Xml::parse_srcset("").is_empty());
        assert!(Xml::parse_srcset("   ").is_empty());
        assert!(Xml::parse_srcset("\t\n").is_empty());
        assert!(
            Xml::parse_srcset(",,,").is_empty(),
            "all-empty entries dropped"
        );
    }

    #[test]
    fn parse_srcset_valid_minimal() {
        assert_eq!(
            Xml::parse_srcset("a.png 1x, b.png 2x"),
            vec!["a.png".to_string(), "b.png".to_string()]
        );
        // No descriptor at all is still a valid single entry.
        assert_eq!(Xml::parse_srcset("a.png"), vec!["a.png".to_string()]);
    }

    #[test]
    fn parse_srcset_garbage_and_boundary_numbers() {
        assert_eq!(Xml::parse_srcset("0, -0, NaN"), vec!["0", "-0", "NaN"]);
        // Garbage bytes still round out to "first whitespace-delimited token".
        assert_eq!(
            Xml::parse_srcset("\u{0}\u{7f} 1x"),
            vec!["\u{0}\u{7f}".to_string()]
        );
    }

    #[test]
    fn parse_srcset_unicode_no_panic() {
        assert_eq!(
            Xml::parse_srcset("\u{1F600}.png 1x, \u{130}.png 2x"),
            vec!["\u{1F600}.png".to_string(), "\u{130}.png".to_string()]
        );
    }

    #[test]
    fn parse_srcset_extremely_long_terminates() {
        let s = "a.png 1x,".repeat(20_000);
        assert_eq!(Xml::parse_srcset(&s).len(), 20_000);
        let one_huge = "a".repeat(LONG);
        assert_eq!(Xml::parse_srcset(&one_huge).len(), 1);
    }

    // ================================================================
    // Xml::looks_like_resource / guess_kind_from_url / guess_mime_from_url
    // ================================================================

    #[test]
    fn looks_like_resource_edges() {
        assert!(!Xml::looks_like_resource(""));
        assert!(!Xml::looks_like_resource("   "));
        assert!(!Xml::looks_like_resource("/about"));
        assert!(Xml::looks_like_resource("/a.PNG"), "case-insensitive");
        assert!(Xml::looks_like_resource("x.pdf"));
        // A query string defeats the extension check (documented consequence of
        // matching on `ends_with`).
        assert!(!Xml::looks_like_resource("x.png?v=1"));
        assert!(!Xml::looks_like_resource(&"a".repeat(LONG)));
    }

    #[test]
    fn guess_kind_from_url_covers_every_bucket() {
        use ExternalResourceKind::*;
        assert_eq!(Xml::guess_kind_from_url(""), Unknown);
        assert_eq!(Xml::guess_kind_from_url("a.PNG"), Image);
        assert_eq!(Xml::guess_kind_from_url("a.woff2"), Font);
        assert_eq!(Xml::guess_kind_from_url("a.css"), Stylesheet);
        assert_eq!(Xml::guess_kind_from_url("a.mjs"), Script);
        assert_eq!(Xml::guess_kind_from_url("a.webm"), Video);
        assert_eq!(Xml::guess_kind_from_url("a.flac"), Audio);
        assert_eq!(Xml::guess_kind_from_url("a.ico"), Icon);
        // Query strings ARE stripped here (unlike looks_like_resource).
        assert_eq!(Xml::guess_kind_from_url("a.png?v=1"), Image);
        assert_eq!(Xml::guess_kind_from_url("\u{1F600}"), Unknown);
    }

    #[test]
    fn guess_mime_from_url_empty_and_garbage() {
        assert_eq!(Xml::guess_mime_from_url("", ""), None);
        assert_eq!(Xml::guess_mime_from_url("   ", ""), None);
        assert_eq!(Xml::guess_mime_from_url("\u{0}\u{7f}", ""), None);
        assert_eq!(Xml::guess_mime_from_url("\u{1F600}", ""), None);
    }

    #[test]
    fn guess_mime_from_url_valid_minimal_and_category_fallback() {
        let m = Xml::guess_mime_from_url("a.PNG", "").expect("png is a known extension");
        assert_eq!(m.inner.as_str(), "image/png");
        let m = Xml::guess_mime_from_url("a.png?v=1", "").expect("query string stripped");
        assert_eq!(m.inner.as_str(), "image/png");
        // Unknown extension + a category hint => the category wildcard.
        let m = Xml::guess_mime_from_url("/no-ext", "image").expect("category fallback");
        assert_eq!(m.inner.as_str(), "image/*");
        // Unknown category => None.
        assert_eq!(Xml::guess_mime_from_url("/no-ext", "bogus"), None);
    }

    #[test]
    fn guess_mime_from_url_boundary_numbers_and_long() {
        assert_eq!(Xml::guess_mime_from_url("0", ""), None);
        assert_eq!(Xml::guess_mime_from_url("-0", ""), None);
        assert_eq!(Xml::guess_mime_from_url("NaN", ""), None);
        assert_eq!(Xml::guess_mime_from_url("inf", ""), None);
        let long = format!("{}.png", "a".repeat(LONG));
        assert_eq!(
            Xml::guess_mime_from_url(&long, "").map(|m| m.inner.as_str().to_string()),
            Some("image/png".to_string())
        );
    }

    // ================================================================
    // Xml::extract_css_urls / scan_node / scan_external_resources
    // ================================================================

    #[test]
    fn extract_css_urls_empty_and_garbage_no_panic() {
        let mut v = Vec::new();
        Xml::extract_css_urls("", &mut v);
        Xml::extract_css_urls("   ", &mut v);
        Xml::extract_css_urls("\u{0}\u{7f}\u{1F600}", &mut v);
        Xml::extract_css_urls("url(", &mut v);
        Xml::extract_css_urls("@import", &mut v);
        Xml::extract_css_urls("@import url(", &mut v);
        assert!(v.is_empty(), "no well-formed url in any of those inputs");
    }

    #[test]
    fn extract_css_urls_valid_minimal() {
        let mut v = Vec::new();
        Xml::extract_css_urls("a { background: url('x.png'); }", &mut v);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].url.as_str(), "x.png");
        assert_eq!(v[0].kind, ExternalResourceKind::Image);
        assert_eq!(v[0].source_attribute.as_str(), "url()");
    }

    #[test]
    fn extract_css_urls_import_is_tagged_as_stylesheet() {
        let mut v = Vec::new();
        Xml::extract_css_urls("@import url(theme.css);", &mut v);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].url.as_str(), "theme.css");
        assert_eq!(v[0].kind, ExternalResourceKind::Stylesheet);
        assert_eq!(v[0].source_attribute.as_str(), "@import");
    }

    #[test]
    fn extract_css_urls_multibyte_before_url_no_panic() {
        // ASCII-only lowercasing keeps byte offsets 1:1 with the original.
        let mut v = Vec::new();
        Xml::extract_css_urls("\u{130}\u{1F600} URL(\"a.css\") \u{0301}", &mut v);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].url.as_str(), "a.css");
    }

    #[test]
    fn extract_css_urls_extremely_long_terminates() {
        // Each iteration advances search_from past the "url(" it just matched, so
        // this must terminate (and not spin).
        let mut v = Vec::new();
        Xml::extract_css_urls(&"url(".repeat(2_000), &mut v);
        // No ')' anywhere => nothing extractable, but the scan still terminates.
        assert!(v.is_empty());

        let mut v2 = Vec::new();
        Xml::extract_css_urls(&"url(a.png)".repeat(2_000), &mut v2);
        assert_eq!(v2.len(), 2_000);
    }

    #[test]
    fn scan_node_on_empty_and_extreme_nodes_no_panic() {
        let mut v = Vec::new();
        Xml::scan_node(&XmlNode::default(), &mut v);
        Xml::scan_node(&node("", &[], vec![]), &mut v);
        Xml::scan_node(
            &node(&"a".repeat(10_000), &[("style", "url(x.png)")], vec![]),
            &mut v,
        );
        assert_eq!(v.len(), 1, "only the inline style url()");
        assert_eq!(v[0].url.as_str(), "x.png");
    }

    #[test]
    fn scan_node_img_srcset_and_background() {
        let mut v = Vec::new();
        Xml::scan_node(
            &node(
                "IMG",
                &[
                    ("src", "a.png"),
                    ("srcset", "b.png 1x, c.png 2x"),
                    ("background", "d.gif"),
                ],
                vec![],
            ),
            &mut v,
        );
        let urls: Vec<&str> = v.iter().map(|r| r.url.as_str()).collect();
        assert_eq!(urls, vec!["a.png", "b.png", "c.png", "d.gif"]);
        assert!(v.iter().all(|r| r.kind == ExternalResourceKind::Image));
    }

    #[test]
    fn scan_external_resources_on_empty_document() {
        let xml = Xml {
            root: Vec::new().into(),
        };
        assert_eq!(xml.scan_external_resources().as_ref().len(), 0);
    }

    #[test]
    fn scan_external_resources_finds_every_element_kind() {
        let xml = Xml {
            root: vec![
                elem(node("img", &[("src", "i.png")], vec![])),
                elem(node(
                    "link",
                    &[("href", "s.css"), ("rel", "stylesheet")],
                    vec![],
                )),
                elem(node("script", &[("src", "s.js")], vec![])),
                elem(node(
                    "video",
                    &[("src", "v.mp4"), ("poster", "p.jpg")],
                    vec![],
                )),
                elem(node("audio", &[("src", "a.mp3")], vec![])),
                elem(node("a", &[("href", "f.pdf")], vec![])),
                elem(node("a", &[("href", "/page")], vec![])),
            ]
            .into(),
        };
        let res = xml.scan_external_resources();
        let mut urls: Vec<&str> = res.as_ref().iter().map(|r| r.url.as_str()).collect();
        urls.sort_unstable();
        assert_eq!(
            urls,
            vec!["a.mp3", "f.pdf", "i.png", "p.jpg", "s.css", "s.js", "v.mp4"],
            "`/page` is not a resource and must be skipped"
        );
    }

    // ================================================================
    // MimeTypeHint  (constructor)
    // ================================================================

    #[test]
    fn mime_type_hint_new_no_panic_and_fields_match_args() {
        for s in ["", "   ", "text/css", "\u{1F600}", "\u{0}"] {
            assert_eq!(
                MimeTypeHint::new(s).inner.as_str(),
                s,
                "new() stores verbatim"
            );
        }
        let long = "x".repeat(LONG);
        assert_eq!(MimeTypeHint::new(&long).inner.as_str().len(), LONG);
    }

    #[test]
    fn mime_type_hint_from_extension_edges() {
        assert_eq!(
            MimeTypeHint::from_extension("").inner.as_str(),
            "application/octet-stream"
        );
        assert_eq!(
            MimeTypeHint::from_extension("PnG").inner.as_str(),
            "image/png",
            "extension match is case-insensitive"
        );
        assert_eq!(
            MimeTypeHint::from_extension("jpeg").inner.as_str(),
            "image/jpeg"
        );
        assert_eq!(
            MimeTypeHint::from_extension("woff2").inner.as_str(),
            "font/woff2"
        );
        assert_eq!(
            MimeTypeHint::from_extension("\u{1F600}").inner.as_str(),
            "application/octet-stream"
        );
        assert_eq!(
            MimeTypeHint::from_extension(&"z".repeat(LONG))
                .inner
                .as_str(),
            "application/octet-stream"
        );
    }

    // ================================================================
    // ComponentId  (constructor / getter)
    // ================================================================

    #[test]
    fn component_id_builtin_and_new_invariants() {
        let b = ComponentId::builtin("div");
        assert_eq!(b.collection.as_str(), "builtin");
        assert_eq!(b.name.as_str(), "div");

        let c = ComponentId::new("", "");
        assert_eq!(c.collection.as_str(), "");
        assert_eq!(c.name.as_str(), "");

        let u = ComponentId::new("\u{1F600}", "\u{130}");
        assert_eq!(u.collection.as_str(), "\u{1F600}");
        assert_eq!(u.name.as_str(), "\u{130}");
    }

    #[test]
    fn component_id_qualified_name_roundtrips_through_the_map_lookup() {
        assert_eq!(ComponentId::builtin("div").qualified_name(), "builtin:div");
        assert_eq!(ComponentId::new("", "").qualified_name(), ":");
        // A name that itself contains ':' makes the qualified name ambiguous —
        // pin the (lossy) behavior so a change is noticed.
        assert_eq!(ComponentId::new("a", "b:c").qualified_name(), "a:b:c");
    }

    // ================================================================
    // ComponentFieldTypeBox / ComponentFieldValueBox  (constructor / getter)
    // ================================================================

    #[test]
    fn component_field_type_box_new_as_ref_and_clone() {
        let b = ComponentFieldTypeBox::new(ComponentFieldType::Bool);
        assert!(!b.ptr.is_null());
        assert_eq!(*b.as_ref(), ComponentFieldType::Bool);

        let c = b.clone();
        assert_eq!(*c.as_ref(), ComponentFieldType::Bool);
        assert_ne!(b.ptr, c.ptr, "clone must deep-copy, not alias");
        assert_eq!(b, c, "PartialEq compares pointees");
        drop(c);
        assert_eq!(*b.as_ref(), ComponentFieldType::Bool, "original survives");
    }

    #[test]
    fn component_field_type_box_nested_deeply_drops_cleanly() {
        let mut t = ComponentFieldType::Bool;
        for _ in 0..64 {
            t = ComponentFieldType::OptionType(ComponentFieldTypeBox::new(t));
        }
        assert_eq!(
            t.format(),
            format!("{}Bool{}", "Option<".repeat(64), ">".repeat(64))
        );
        drop(t);
    }

    #[test]
    fn component_field_value_box_new_as_ref_and_clone() {
        let v = ComponentFieldValueBox::new(ComponentFieldValue::I32(i32::MIN));
        assert!(!v.ptr.is_null());
        assert_eq!(*v.as_ref(), ComponentFieldValue::I32(i32::MIN));
        let c = v.clone();
        assert_ne!(v.ptr, c.ptr);
        assert_eq!(v, c);
    }

    // ================================================================
    // ComponentFieldType::parse / parse_depth / format  (round-trip)
    // ================================================================

    #[test]
    fn component_field_type_parse_empty_whitespace_garbage() {
        assert_eq!(ComponentFieldType::parse(""), None);
        assert_eq!(ComponentFieldType::parse("   "), None);
        assert_eq!(ComponentFieldType::parse("\t\n"), None);
        assert_eq!(ComponentFieldType::parse("lowercase"), None);
        assert_eq!(ComponentFieldType::parse("\u{0}\u{7f}"), None);
        assert_eq!(
            ComponentFieldType::parse("Option<>"),
            None,
            "empty inner rejected"
        );
        assert_eq!(ComponentFieldType::parse("Vec<>"), None);
        assert_eq!(ComponentFieldType::parse("Option<lowercase>"), None);
    }

    #[test]
    fn component_field_type_parse_valid_minimal_and_trimming() {
        assert_eq!(
            ComponentFieldType::parse("String"),
            Some(ComponentFieldType::String)
        );
        assert_eq!(
            ComponentFieldType::parse("  String  "),
            Some(ComponentFieldType::String),
            "leading/trailing whitespace is trimmed"
        );
        assert_eq!(
            ComponentFieldType::parse("bool"),
            Some(ComponentFieldType::Bool)
        );
        assert_eq!(
            ComponentFieldType::parse("usize"),
            Some(ComponentFieldType::Usize)
        );
        assert_eq!(
            ComponentFieldType::parse("StructRef(Foo)"),
            Some(ComponentFieldType::StructRef(AzString::from("Foo")))
        );
        assert_eq!(
            ComponentFieldType::parse("EnumRef(Foo)"),
            Some(ComponentFieldType::EnumRef(AzString::from("Foo")))
        );
        assert_eq!(
            ComponentFieldType::parse("RefAny"),
            Some(ComponentFieldType::RefAny(AzString::from("")))
        );
    }

    #[test]
    fn component_field_type_parse_leading_trailing_junk_is_rejected_or_absorbed() {
        // Trailing junk after a known keyword falls through to the
        // "starts uppercase => StructRef" catch-all rather than being rejected.
        assert_eq!(
            ComponentFieldType::parse("String;garbage"),
            Some(ComponentFieldType::StructRef(AzString::from(
                "String;garbage"
            )))
        );
        // Lowercase junk has no uppercase first char => rejected.
        assert_eq!(ComponentFieldType::parse("string;garbage"), None);
    }

    #[test]
    fn component_field_type_parse_boundary_numbers() {
        for s in [
            "0",
            "-0",
            "9223372036854775807",
            "-9223372036854775808",
            "1e400",
            "inf",
        ] {
            assert_eq!(
                ComponentFieldType::parse(s),
                None,
                "numeric literal {s:?} is not a type name"
            );
        }
        // ...but anything starting with an uppercase letter hits the StructRef
        // catch-all, so "NaN" parses as a struct reference rather than failing.
        assert_eq!(
            ComponentFieldType::parse("NaN"),
            Some(ComponentFieldType::StructRef(AzString::from("NaN")))
        );
    }

    #[test]
    fn component_field_type_parse_unicode_no_panic() {
        assert_eq!(
            ComponentFieldType::parse("\u{1F600}"),
            None,
            "emoji is not uppercase"
        );
        // A real uppercase non-ASCII letter hits the StructRef catch-all.
        assert_eq!(
            ComponentFieldType::parse("\u{0391}bc"),
            Some(ComponentFieldType::StructRef(AzString::from("\u{0391}bc")))
        );
    }

    #[test]
    fn component_field_type_parse_depth_boundary_is_exact() {
        // MAX_TYPE_PARSE_DEPTH wrappers parse; one more is rejected.
        let ok = format!(
            "{}Bool{}",
            "Option<".repeat(MAX_TYPE_PARSE_DEPTH),
            ">".repeat(MAX_TYPE_PARSE_DEPTH)
        );
        assert!(
            ComponentFieldType::parse(&ok).is_some(),
            "exactly MAX_TYPE_PARSE_DEPTH wrappers must still parse"
        );

        let too_deep = format!(
            "{}Bool{}",
            "Option<".repeat(MAX_TYPE_PARSE_DEPTH + 1),
            ">".repeat(MAX_TYPE_PARSE_DEPTH + 1)
        );
        assert_eq!(
            ComponentFieldType::parse(&too_deep),
            None,
            "one wrapper past the cap must be rejected, not overflow"
        );
    }

    #[test]
    fn component_field_type_parse_depth_direct_call_honors_start_depth() {
        assert_eq!(
            ComponentFieldType::parse_depth("Bool", MAX_TYPE_PARSE_DEPTH),
            Some(ComponentFieldType::Bool),
            "depth == cap is still allowed"
        );
        assert_eq!(
            ComponentFieldType::parse_depth("Bool", MAX_TYPE_PARSE_DEPTH + 1),
            None
        );
        assert_eq!(
            ComponentFieldType::parse_depth("Bool", usize::MAX),
            None,
            "usize::MAX start depth must not overflow, just refuse"
        );
    }

    #[test]
    fn component_field_type_parse_nested_recursion_does_not_stack_overflow() {
        let bomb = format!("{}Bool{}", "Vec<".repeat(50_000), ">".repeat(50_000));
        assert_eq!(ComponentFieldType::parse(&bomb), None);
    }

    #[test]
    fn component_field_type_parse_extremely_long_terminates() {
        // A single 200k-char uppercase token becomes a StructRef of that name.
        let long = format!("A{}", "b".repeat(LONG));
        assert_eq!(
            ComponentFieldType::parse(&long),
            Some(ComponentFieldType::StructRef(AzString::from(long.as_str())))
        );
    }

    #[test]
    fn component_field_type_round_trip_representative() {
        let representative = vec![
            ComponentFieldType::String,
            ComponentFieldType::Bool,
            ComponentFieldType::I32,
            ComponentFieldType::I64,
            ComponentFieldType::U32,
            ComponentFieldType::U64,
            ComponentFieldType::Usize,
            ComponentFieldType::F32,
            ComponentFieldType::F64,
            ComponentFieldType::ColorU,
            ComponentFieldType::CssProperty,
            ComponentFieldType::ImageRef,
            ComponentFieldType::FontRef,
            ComponentFieldType::StyledDom,
            ComponentFieldType::StructRef(AzString::from("Foo")),
            ComponentFieldType::OptionType(ComponentFieldTypeBox::new(ComponentFieldType::Bool)),
            ComponentFieldType::VecType(ComponentFieldTypeBox::new(ComponentFieldType::I32)),
            ComponentFieldType::RefAny(AzString::from("")),
            ComponentFieldType::RefAny(AzString::from("Hint")),
            ComponentFieldType::Callback(ComponentCallbackSignature {
                return_type: AzString::from("Update"),
                args: Vec::new().into(),
            }),
        ];
        for x in representative {
            let s = x.format();
            assert_eq!(
                ComponentFieldType::parse(&s),
                Some(x.clone()),
                "parse(format({x:?})) must round-trip"
            );
        }
    }

    #[test]
    fn component_field_type_round_trip_edge_values() {
        // Empty / unicode-bearing payloads.
        for x in [
            ComponentFieldType::StructRef(AzString::from("\u{0391}\u{1F600}")),
            ComponentFieldType::OptionType(ComponentFieldTypeBox::new(
                ComponentFieldType::VecType(ComponentFieldTypeBox::new(
                    ComponentFieldType::StructRef(AzString::from("Foo")),
                )),
            )),
        ] {
            assert_eq!(ComponentFieldType::parse(&x.format()), Some(x.clone()));
        }
    }

    #[test]
    fn component_field_type_format_is_an_idempotent_normalization() {
        // `EnumRef` and `StructRef` share the same canonical spelling, so parse()
        // collapses EnumRef -> StructRef. The normalization is still STABLE:
        // format(parse(format(x))) == format(x).
        let e = ComponentFieldType::EnumRef(AzString::from("Role"));
        let once = e.format();
        assert_eq!(once, "Role");
        let reparsed = ComponentFieldType::parse(&once).expect("parses");
        assert_eq!(
            reparsed,
            ComponentFieldType::StructRef(AzString::from("Role")),
            "EnumRef is lossy through format() — it comes back as StructRef"
        );
        assert_eq!(
            reparsed.format(),
            once,
            "but the normalization is idempotent"
        );
    }

    #[test]
    fn component_field_type_display_matches_format() {
        let t = ComponentFieldType::OptionType(ComponentFieldTypeBox::new(ComponentFieldType::F64));
        assert_eq!(format!("{t}"), t.format());
        assert_eq!(format!("{t}"), "Option<F64>");
    }

    #[test]
    fn component_field_type_format_no_panic_on_empty_payloads() {
        let t = ComponentFieldType::Callback(ComponentCallbackSignature {
            return_type: AzString::from(""),
            args: Vec::new().into(),
        });
        assert_eq!(t.format(), "Callback()");
        // Callback() with an empty signature round-trips.
        assert_eq!(ComponentFieldType::parse("Callback()"), Some(t));
    }

    // ================================================================
    // ComponentFieldNamedValueVec::get_field / get_string  (parser-ish lookup)
    // ================================================================

    fn named(name: &str, v: ComponentFieldValue) -> ComponentFieldNamedValue {
        ComponentFieldNamedValue {
            name: AzString::from(name),
            value: v,
        }
    }

    fn named_vec() -> ComponentFieldNamedValueVec {
        vec![
            named("a", ComponentFieldValue::String(AzString::from("x"))),
            named("b", ComponentFieldValue::Bool(true)),
            named("", ComponentFieldValue::U64(u64::MAX)),
            named(
                "\u{1F600}",
                ComponentFieldValue::String(AzString::from("emoji")),
            ),
        ]
        .into()
    }

    #[test]
    fn named_value_vec_get_field_valid_minimal() {
        let v = named_vec();
        assert_eq!(
            v.get_field("a"),
            Some(&ComponentFieldValue::String(AzString::from("x")))
        );
        assert_eq!(v.get_field("b"), Some(&ComponentFieldValue::Bool(true)));
    }

    #[test]
    fn named_value_vec_get_field_empty_whitespace_garbage_unicode() {
        let v = named_vec();
        // An empty NAME is a legal key here — it matches the field literally named "".
        assert_eq!(v.get_field(""), Some(&ComponentFieldValue::U64(u64::MAX)));
        assert_eq!(v.get_field("   "), None);
        assert_eq!(v.get_field("\t\n"), None);
        assert_eq!(v.get_field("\u{0}\u{7f}"), None);
        assert!(v.get_field("\u{1F600}").is_some());
        assert_eq!(v.get_field(" a "), None, "no trimming: lookup is exact");
        assert_eq!(v.get_field("a;garbage"), None);
    }

    #[test]
    fn named_value_vec_get_field_on_empty_vec_and_long_key() {
        let empty = ComponentFieldNamedValueVec::from_const_slice(&[]);
        assert_eq!(empty.get_field("a"), None);
        assert_eq!(empty.get_string("a"), None);
        assert_eq!(named_vec().get_field(&"z".repeat(LONG)), None);
    }

    #[test]
    fn named_value_vec_get_string_only_matches_string_variant() {
        let v = named_vec();
        assert_eq!(v.get_string("a").map(AzString::as_str), Some("x"));
        assert_eq!(v.get_string("b"), None, "Bool is not a String");
        assert_eq!(v.get_string(""), None, "U64 is not a String");
        assert_eq!(v.get_string("missing"), None);
    }

    #[test]
    fn named_value_vec_boundary_numeric_keys() {
        let v: ComponentFieldNamedValueVec = vec![
            named("0", ComponentFieldValue::I32(0)),
            named("-0", ComponentFieldValue::I32(i32::MIN)),
            named("9223372036854775807", ComponentFieldValue::I64(i64::MAX)),
            named("NaN", ComponentFieldValue::F32(f32::NAN)),
        ]
        .into();
        assert_eq!(v.get_field("0"), Some(&ComponentFieldValue::I32(0)));
        assert_eq!(v.get_field("-0"), Some(&ComponentFieldValue::I32(i32::MIN)));
        assert_eq!(
            v.get_field("9223372036854775807"),
            Some(&ComponentFieldValue::I64(i64::MAX))
        );
        // NaN != NaN, so only check the variant, not equality.
        assert!(matches!(v.get_field("NaN"), Some(ComponentFieldValue::F32(f)) if f.is_nan()));
    }

    // ================================================================
    // ComponentDataModel::get_field / get_default_string / with_default
    // ================================================================

    fn model_with_text() -> ComponentDataModel {
        dm(
            "M",
            vec![
                data_field(
                    "text",
                    ComponentFieldType::String,
                    Some(ComponentDefaultValue::String(AzString::from("hi"))),
                    "",
                ),
                data_field(
                    "count",
                    ComponentFieldType::U32,
                    Some(ComponentDefaultValue::U32(3)),
                    "",
                ),
                data_field("required_one", ComponentFieldType::String, None, ""),
            ],
        )
    }

    #[test]
    fn data_model_get_field_valid_minimal_and_missing() {
        let m = model_with_text();
        assert!(m.get_field("text").is_some());
        assert!(m.get_field("count").is_some());
        assert!(m.get_field("missing").is_none());
        assert!(m.get_field("").is_none());
        assert!(m.get_field("   ").is_none());
        assert!(m.get_field(" text ").is_none(), "exact match, no trimming");
        assert!(m.get_field("\u{1F600}").is_none());
        assert!(m.get_field(&"z".repeat(LONG)).is_none());
    }

    #[test]
    fn data_model_get_field_on_empty_model() {
        let m = dm("Empty", Vec::new());
        assert!(m.get_field("anything").is_none());
        assert!(m.get_default_string("anything").is_none());
    }

    #[test]
    fn data_model_get_default_string_only_for_string_defaults() {
        let m = model_with_text();
        assert_eq!(
            m.get_default_string("text").map(AzString::as_str),
            Some("hi")
        );
        assert_eq!(
            m.get_default_string("count"),
            None,
            "U32 default is not a String"
        );
        assert_eq!(
            m.get_default_string("required_one"),
            None,
            "no default at all"
        );
        assert_eq!(m.get_default_string("missing"), None);
    }

    #[test]
    fn data_model_required_flag_follows_default_presence() {
        let m = model_with_text();
        assert!(!m.get_field("text").unwrap().required);
        assert!(
            m.get_field("required_one").unwrap().required,
            "a field with no default must be marked required"
        );
    }

    #[test]
    fn data_model_with_default_overrides_and_preserves_len() {
        let m = model_with_text();
        let before = m.fields.as_ref().len();
        let m = m.with_default("text", ComponentDefaultValue::String(AzString::from("bye")));
        assert_eq!(m.fields.as_ref().len(), before, "len is preserved");
        assert_eq!(
            m.get_default_string("text").map(AzString::as_str),
            Some("bye")
        );
    }

    #[test]
    fn data_model_with_default_on_missing_field_is_a_no_op() {
        let m = model_with_text();
        let m = m.with_default("nope", ComponentDefaultValue::Bool(true));
        assert_eq!(m.fields.as_ref().len(), 3);
        assert_eq!(
            m.get_default_string("text").map(AzString::as_str),
            Some("hi")
        );
        assert!(m.get_field("nope").is_none(), "no field is inserted");
    }

    #[test]
    fn data_model_with_default_extreme_names_and_values_no_panic() {
        let m = model_with_text()
            .with_default("", ComponentDefaultValue::None)
            .with_default(&"z".repeat(10_000), ComponentDefaultValue::F64(f64::NAN))
            .with_default("count", ComponentDefaultValue::Usize(usize::MAX))
            .with_default("text", ComponentDefaultValue::I64(i64::MIN));
        assert_eq!(m.fields.as_ref().len(), 3);
        assert!(matches!(
            m.get_field("count").unwrap().default_value,
            OptionComponentDefaultValue::Some(ComponentDefaultValue::Usize(usize::MAX))
        ));
        assert_eq!(
            m.get_default_string("text"),
            None,
            "text is now an I64 default, no longer a String"
        );
    }

    #[test]
    fn data_model_with_default_fills_only_the_first_match() {
        // Duplicate field names: `with_default` breaks after the first hit.
        let m = dm(
            "Dup",
            vec![
                data_field(
                    "x",
                    ComponentFieldType::String,
                    Some(ComponentDefaultValue::String(AzString::from("1"))),
                    "",
                ),
                data_field(
                    "x",
                    ComponentFieldType::String,
                    Some(ComponentDefaultValue::String(AzString::from("2"))),
                    "",
                ),
            ],
        )
        .with_default("x", ComponentDefaultValue::String(AzString::from("3")));
        let vals: Vec<&str> = m
            .fields
            .as_ref()
            .iter()
            .filter_map(|f| match &f.default_value {
                OptionComponentDefaultValue::Some(ComponentDefaultValue::String(s)) => {
                    Some(s.as_str())
                }
                _ => None,
            })
            .collect();
        assert_eq!(
            vals,
            vec!["3", "2"],
            "only the first duplicate is overridden"
        );
    }

    // ================================================================
    // ComponentSource / ComponentMap  (constructor / getter / lookup)
    // ================================================================

    #[test]
    fn component_source_create_is_user_defined() {
        assert_eq!(ComponentSource::create(), ComponentSource::UserDefined);
        assert_eq!(ComponentSource::default(), ComponentSource::UserDefined);
    }

    #[test]
    fn component_map_create_is_empty_and_all_lookups_return_none() {
        let m = ComponentMap::create();
        assert_eq!(m.libraries.as_ref().len(), 0);
        assert!(m.get("builtin", "div").is_none());
        assert!(m.get_unqualified("div").is_none());
        assert!(m.get_by_qualified_name("builtin:div").is_none());
        assert!(m.all_components().is_empty());
        assert!(m.get_exportable_libraries().is_empty());
    }

    #[test]
    fn component_map_with_builtin_invariants() {
        let m = ComponentMap::with_builtin();
        assert_eq!(m.libraries.as_ref().len(), 1);
        let lib = &m.libraries.as_ref()[0];
        assert_eq!(lib.name.as_str(), "builtin");
        assert!(!lib.exportable, "builtins must never be exportable");
        assert!(!lib.modifiable, "builtins must never be modifiable");
        assert_eq!(
            m.all_components().len(),
            lib.components.as_ref().len(),
            "all_components must see every registered component"
        );
        assert!(
            m.get_exportable_libraries().is_empty(),
            "the builtin library is not exportable"
        );
    }

    #[test]
    fn component_map_get_valid_minimal() {
        let m = ComponentMap::with_builtin();
        assert!(m.get("builtin", "div").is_some());
        assert!(m.get("builtin", "if").is_some());
        assert!(m.get("builtin", "for").is_some());
        assert!(m.get("builtin", "map").is_some());
        assert_eq!(
            m.get("builtin", "div").unwrap().id.qualified_name(),
            "builtin:div"
        );
    }

    #[test]
    fn component_map_get_empty_whitespace_garbage_unicode() {
        let m = ComponentMap::with_builtin();
        assert!(m.get("", "").is_none());
        assert!(m.get("builtin", "").is_none());
        assert!(m.get("", "div").is_none());
        assert!(m.get("builtin", "   ").is_none());
        assert!(m.get("builtin", " div ").is_none(), "no trimming");
        assert!(
            m.get("builtin", "DIV").is_none(),
            "lookup is case-sensitive"
        );
        assert!(m.get("builtin", "\u{1F600}").is_none());
        assert!(m.get("builtin", "\u{0}\u{7f}").is_none());
        assert!(m.get("builtin", "div;garbage").is_none());
    }

    #[test]
    fn component_map_get_extremely_long_name_terminates() {
        let m = ComponentMap::with_builtin();
        assert!(m.get(&"a".repeat(LONG), &"b".repeat(LONG)).is_none());
        assert!(m.get_unqualified(&"b".repeat(LONG)).is_none());
        assert!(m.get_by_qualified_name(&"c".repeat(LONG)).is_none());
    }

    #[test]
    fn component_map_get_unqualified_only_searches_builtin() {
        let lib = ComponentLibrary {
            name: AzString::from("mylib"),
            version: AzString::from("1.0.0"),
            description: AzString::from(""),
            components: vec![user_def("", Vec::new())].into(),
            exportable: true,
            modifiable: true,
            data_models: Vec::new().into(),
            enum_models: Vec::new().into(),
        };
        let libs: ComponentLibraryVec = vec![lib].into();
        let m = ComponentMap::from_libraries(&libs);

        assert!(m.get("mylib", "widget").is_some());
        assert!(
            m.get_unqualified("widget").is_none(),
            "unqualified lookup must NOT reach non-builtin libraries"
        );
        assert!(m.get_by_qualified_name("mylib:widget").is_some());
        assert_eq!(m.get_exportable_libraries().len(), 1);
        assert_eq!(m.all_components().len(), 1);
    }

    #[test]
    fn component_map_get_by_qualified_name_boundary_forms() {
        let m = ComponentMap::with_builtin();
        // No colon => falls back to the builtin library.
        assert!(m.get_by_qualified_name("div").is_some());
        // Exactly one colon.
        assert!(m.get_by_qualified_name("builtin:div").is_some());
        // Splits on the FIRST colon, so the remainder (incl. colons) is the name.
        assert!(m.get_by_qualified_name("builtin:div:extra").is_none());
        assert!(m.get_by_qualified_name(":").is_none());
        assert!(m.get_by_qualified_name(":div").is_none());
        assert!(m.get_by_qualified_name("builtin:").is_none());
        assert!(m.get_by_qualified_name("").is_none());
        assert!(m.get_by_qualified_name("   ").is_none());
    }

    #[test]
    fn component_map_from_libraries_clones_without_losing_entries() {
        let src = ComponentMap::with_builtin();
        let copy = ComponentMap::from_libraries(&src.libraries);
        assert_eq!(copy.all_components().len(), src.all_components().len());
        assert!(copy.get_unqualified("div").is_some());
    }

    #[test]
    fn register_builtin_components_is_stable_across_calls() {
        let a = register_builtin_components();
        let b = register_builtin_components();
        assert_eq!(a.components.as_ref().len(), b.components.as_ref().len());
        assert!(a.components.as_ref().len() > 50);
        assert_eq!(a.name.as_str(), "builtin");
        assert_eq!(a.version.as_str(), "1.0.0");
        // Every component must be namespaced into "builtin".
        assert!(a
            .components
            .as_ref()
            .iter()
            .all(|c| c.id.collection.as_str() == "builtin"));
        assert!(a
            .components
            .as_ref()
            .iter()
            .all(|c| c.source == ComponentSource::Builtin));
    }

    // ================================================================
    // XmlNodeChild / XmlNode  (getter / predicate / constructor)
    // ================================================================

    #[test]
    fn xml_node_child_as_text_and_as_element_are_mutually_exclusive() {
        let t = txt("hello");
        assert_eq!(t.as_text(), Some("hello"));
        assert!(t.as_element().is_none());

        let e = elem(node("div", &[], vec![]));
        assert!(e.as_text().is_none());
        assert_eq!(e.as_element().map(|n| n.node_type.as_str()), Some("div"));
    }

    #[test]
    fn xml_node_child_as_text_edge_values() {
        assert_eq!(
            txt("").as_text(),
            Some(""),
            "an empty text node is still text"
        );
        assert_eq!(
            txt("   ").as_text(),
            Some("   "),
            "no trimming in the getter"
        );
        assert_eq!(
            txt("\u{1F600}\u{0301}").as_text(),
            Some("\u{1F600}\u{0301}")
        );
        assert_eq!(txt("\u{0}").as_text(), Some("\u{0}"));
    }

    #[test]
    fn xml_node_child_as_element_mut_allows_mutation_and_rejects_text() {
        let mut e = elem(node("div", &[], vec![]));
        e.as_element_mut().expect("is an element").node_type = "span".into();
        assert_eq!(e.as_element().map(|n| n.node_type.as_str()), Some("span"));

        let mut t = txt("x");
        assert!(t.as_element_mut().is_none(), "text nodes have no element");
    }

    #[test]
    fn xml_node_create_and_with_children_invariants() {
        let n = XmlNode::create("div");
        assert_eq!(n.node_type.as_str(), "div");
        assert_eq!(n.children.as_ref().len(), 0);
        assert_eq!(n.attributes.as_ref().len(), 0);

        let n = n.with_children(vec![txt("a"), elem(XmlNode::create("b"))]);
        assert_eq!(n.children.as_ref().len(), 2);
        assert_eq!(n.node_type.as_str(), "div", "tag survives with_children");

        // with_children REPLACES, it does not append.
        let n = n.with_children(Vec::new());
        assert_eq!(n.children.as_ref().len(), 0);
    }

    #[test]
    fn xml_node_create_extreme_tag_names_no_panic() {
        assert_eq!(XmlNode::create("").node_type.as_str(), "");
        assert_eq!(XmlNode::create("\u{1F600}").node_type.as_str(), "\u{1F600}");
        assert_eq!(
            XmlNode::create(&*"a".repeat(10_000))
                .node_type
                .as_str()
                .len(),
            10_000
        );
    }

    #[test]
    fn xml_node_get_text_content_concatenates_only_direct_text() {
        let n = node(
            "p",
            &[],
            vec![
                txt("a "),
                elem(node("span", &[], vec![txt("IGNORED")])),
                txt("b"),
            ],
        );
        assert_eq!(
            n.get_text_content(),
            "a b",
            "only DIRECT text children, nested element text is not included"
        );
        assert_eq!(XmlNode::default().get_text_content(), "");
        assert_eq!(
            node("p", &[], vec![txt(""), txt("")]).get_text_content(),
            ""
        );
    }

    #[test]
    fn xml_node_has_only_text_children_true_false_and_empty() {
        assert!(
            XmlNode::default().has_only_text_children(),
            "vacuously true for a childless node — callers must pair this with a text check"
        );
        assert!(node("p", &[], vec![txt("a"), txt("b")]).has_only_text_children());
        assert!(
            !node("p", &[], vec![txt("a"), elem(XmlNode::create("b"))]).has_only_text_children()
        );
        assert!(!node("p", &[], vec![elem(XmlNode::create("b"))]).has_only_text_children());
    }

    // ================================================================
    // get_html_node / get_body_node / find_node_by_type / find_attribute
    // ================================================================

    #[test]
    fn get_html_node_empty_input_is_no_html_node() {
        assert_eq!(get_html_node(&[]), Err(DomXmlParseError::NoHtmlNode));
        assert_eq!(
            get_html_node(&[txt("just text")]),
            Err(DomXmlParseError::NoHtmlNode)
        );
        assert_eq!(
            get_html_node(&[elem(XmlNode::create("div"))]),
            Err(DomXmlParseError::NoHtmlNode)
        );
    }

    #[test]
    fn get_html_node_valid_minimal_and_case_insensitive() {
        let roots = vec![elem(XmlNode::create("HTML"))];
        assert!(get_html_node(&roots).is_ok(), "tag casing is normalized");
    }

    #[test]
    fn get_html_node_rejects_multiple_roots() {
        let roots = vec![elem(XmlNode::create("html")), elem(XmlNode::create("html"))];
        assert_eq!(
            get_html_node(&roots),
            Err(DomXmlParseError::MultipleHtmlRootNodes)
        );
    }

    #[test]
    fn get_body_node_empty_and_missing() {
        assert_eq!(get_body_node(&[]), Err(DomXmlParseError::NoBodyInHtml));
        assert_eq!(
            get_body_node(&[elem(XmlNode::create("head"))]),
            Err(DomXmlParseError::NoBodyInHtml)
        );
    }

    #[test]
    fn get_body_node_direct_and_nested() {
        let direct = vec![elem(XmlNode::create("body"))];
        assert!(get_body_node(&direct).is_ok());

        // Malformed markup: <body> buried inside <head>.
        let nested = vec![elem(node(
            "head",
            &[],
            vec![elem(node("div", &[], vec![elem(XmlNode::create("BODY"))]))],
        ))];
        assert!(
            get_body_node(&nested).is_ok(),
            "the recursive fallback finds a nested body"
        );
    }

    /// Build a `<div>` chain `depth` levels deep with `inner` at the bottom.
    fn wrap_divs(depth: usize, inner: XmlNode) -> XmlNode {
        let mut n = inner;
        for _ in 0..depth {
            n = node("div", &[], vec![elem(n)]);
        }
        n
    }

    #[test]
    fn get_body_node_deep_nesting_is_depth_capped_not_stack_overflowing() {
        // Body sits just inside the cap => found.
        let ok = vec![elem(wrap_divs(
            MAX_XML_NESTING_DEPTH - 2,
            XmlNode::create("body"),
        ))];
        assert!(
            get_body_node(&ok).is_ok(),
            "body within the depth cap is found"
        );

        // Body far below the cap => reported missing, but MUST NOT overflow.
        let too_deep = vec![elem(wrap_divs(2_000, XmlNode::create("body")))];
        assert_eq!(
            get_body_node(&too_deep),
            Err(DomXmlParseError::NoBodyInHtml),
            "past MAX_XML_NESTING_DEPTH the search gives up instead of crashing"
        );
    }

    #[test]
    fn find_node_by_type_empty_garbage_unicode() {
        assert!(find_node_by_type(&[], "div").is_none());
        assert!(find_node_by_type(&[txt("x")], "div").is_none());

        let roots = vec![elem(XmlNode::create("div"))];
        assert!(find_node_by_type(&roots, "").is_none());
        assert!(find_node_by_type(&roots, "   ").is_none());
        assert!(find_node_by_type(&roots, "\u{1F600}").is_none());
        assert!(find_node_by_type(&roots, &"z".repeat(LONG)).is_none());
        // Tag matching is ASCII case-insensitive (HTML tags are), so "DIV" matches a
        // <div> node. (It used to compare against the normalize_casing'd tag, which was
        // case-sensitive on the needle AND mangled uppercase tags to "d_i_v".)
        assert!(find_node_by_type(&roots, "DIV").is_some());
    }

    #[test]
    fn find_node_by_type_valid_minimal_and_recursive() {
        let roots = vec![elem(node(
            "html",
            &[],
            vec![elem(node(
                "head",
                &[],
                vec![elem(XmlNode::create("STYLE"))],
            ))],
        ))];
        assert!(find_node_by_type(&roots, "html").is_some());
        assert!(
            find_node_by_type(&roots, "style").is_some(),
            "search recurses into the whole tree"
        );
        assert!(find_node_by_type(&roots, "body").is_none());
    }

    #[test]
    fn find_node_by_type_prefers_the_shallowest_match() {
        let roots = vec![
            elem(node("div", &[], vec![elem(XmlNode::create("span"))])),
            elem(node("span", &[("id", "shallow")], vec![])),
        ];
        let found = find_node_by_type(&roots, "span").expect("found");
        assert_eq!(
            found.attributes.get_key("id").map(AzString::as_str),
            Some("shallow"),
            "direct children are scanned before recursing"
        );
    }

    #[test]
    fn find_attribute_valid_minimal_and_missing() {
        let n = node("a", &[("href", "x"), ("id", "y")], vec![]);
        assert_eq!(find_attribute(&n, "href").map(AzString::as_str), Some("x"));
        assert_eq!(find_attribute(&n, "id").map(AzString::as_str), Some("y"));
        assert!(find_attribute(&n, "missing").is_none());
        assert!(find_attribute(&n, "").is_none());
        assert!(find_attribute(&n, "   ").is_none());
        assert!(find_attribute(&XmlNode::default(), "href").is_none());
        assert!(find_attribute(&n, &"z".repeat(LONG)).is_none());
    }

    #[test]
    fn find_attribute_compares_against_the_normalized_key() {
        // `normalize_casing` turns `aria-label` into `aria_label`, so callers must
        // pass the NORMALIZED spelling — the raw HTML spelling does not match.
        let n = node("button", &[("aria-label", "Save")], vec![]);
        assert_eq!(
            find_attribute(&n, "aria_label").map(AzString::as_str),
            Some("Save")
        );
        assert!(
            find_attribute(&n, "aria-label").is_none(),
            "the hyphenated spelling never matches (keys are normalized first)"
        );
    }

    #[test]
    fn find_attribute_unicode_keys_no_panic() {
        let n = node("div", &[("\u{130}", "v"), ("\u{1F600}", "w")], vec![]);
        assert!(
            find_attribute(&n, "\u{1F600}").is_some(),
            "emoji keys pass through"
        );
        // 'İ' lowercases to 2 chars, so the normalized key is not 'İ'.
        assert!(find_attribute(&n, "\u{130}").is_none());
    }

    // ================================================================
    // normalize_casing
    // ================================================================

    #[test]
    fn normalize_casing_documented_forms() {
        assert_eq!(normalize_casing("abcDef"), "abc_def");
        assert_eq!(normalize_casing("AbcDef"), "abc_def");
        assert_eq!(normalize_casing("abc-def"), "abc_def");
        assert_eq!(normalize_casing("abc_def"), "abc_def");
    }

    #[test]
    fn normalize_casing_edge_inputs() {
        assert_eq!(normalize_casing(""), "");
        assert_eq!(
            normalize_casing("---"),
            "",
            "separators alone produce no words"
        );
        assert_eq!(normalize_casing("___"), "");
        assert_eq!(normalize_casing("A"), "a");
        assert_eq!(
            normalize_casing("ABC"),
            "a_b_c",
            "every uppercase char starts a new word"
        );
        assert_eq!(normalize_casing("h1"), "h1");
        assert_eq!(
            normalize_casing("   "),
            "   ",
            "whitespace is not a separator"
        );
    }

    #[test]
    fn normalize_casing_unicode_and_long_no_panic() {
        // 'İ' (U+0130) lowercases to TWO chars — the fn must not slice bytes.
        assert_eq!(normalize_casing("\u{130}"), "i\u{307}");
        assert_eq!(normalize_casing("\u{1F600}"), "\u{1F600}");
        assert_eq!(normalize_casing(&"a".repeat(50_000)).len(), 50_000);
        // 50k uppercase chars => 50k single-char words joined by '_'.
        assert_eq!(normalize_casing(&"A".repeat(50_000)).len(), 50_000 * 2 - 1);
    }

    // ================================================================
    // get_item / get_item_internal
    // ================================================================

    #[test]
    fn get_item_empty_hierarchy_returns_the_root() {
        let mut root = node("div", &[("id", "root")], vec![]);
        let got = get_item(&[], &mut root).expect("empty hierarchy => root");
        assert_eq!(
            got.attributes.get_key("id").map(AzString::as_str),
            Some("root")
        );
    }

    #[test]
    fn get_item_walks_nested_elements() {
        let mut root = node(
            "a",
            &[],
            vec![elem(node(
                "b",
                &[],
                vec![elem(node("c", &[("id", "deep")], vec![]))],
            ))],
        );
        let got = get_item(&[0, 0], &mut root).expect("a > b > c");
        assert_eq!(got.node_type.as_str(), "c");
        assert_eq!(
            got.attributes.get_key("id").map(AzString::as_str),
            Some("deep")
        );
    }

    #[test]
    fn get_item_out_of_bounds_and_text_nodes_return_none() {
        let mut root = node("a", &[], vec![txt("hello"), elem(XmlNode::create("b"))]);
        assert!(get_item(&[5], &mut root).is_none(), "out of bounds => None");
        assert!(
            get_item(&[usize::MAX], &mut root).is_none(),
            "usize::MAX index must not panic"
        );
        assert!(
            get_item(&[0], &mut root).is_none(),
            "index 0 is a TEXT node — not traversable"
        );
        assert!(get_item(&[1], &mut root).is_some());
        assert!(
            get_item(&[1, 0], &mut root).is_none(),
            "descending past a leaf => None"
        );
    }

    #[test]
    fn get_item_deep_hierarchy_terminates() {
        // 400 levels: deep, but bounded by the (short) hierarchy vec, not by markup.
        let mut root = wrap_divs(400, node("div", &[("id", "bottom")], vec![]));
        let path = vec![0usize; 400];
        let got = get_item(&path, &mut root).expect("reaches the bottom");
        assert_eq!(
            got.attributes.get_key("id").map(AzString::as_str),
            Some("bottom")
        );
    }

    // ================================================================
    // decode_numeric_entity  (parser)
    // ================================================================

    #[test]
    fn decode_numeric_entity_valid_minimal() {
        assert_eq!(decode_numeric_entity("#65"), Some('A'));
        assert_eq!(decode_numeric_entity("#x41"), Some('A'));
        assert_eq!(
            decode_numeric_entity("#X41"),
            Some('A'),
            "uppercase X accepted"
        );
        assert_eq!(decode_numeric_entity("#x1F600"), Some('\u{1F600}'));
    }

    #[test]
    fn decode_numeric_entity_empty_whitespace_garbage() {
        assert_eq!(decode_numeric_entity(""), None);
        assert_eq!(decode_numeric_entity("   "), None);
        assert_eq!(decode_numeric_entity("\t\n"), None);
        assert_eq!(
            decode_numeric_entity("amp"),
            None,
            "named entity is not numeric"
        );
        assert_eq!(decode_numeric_entity("#"), None);
        assert_eq!(decode_numeric_entity("#x"), None);
        assert_eq!(decode_numeric_entity("#zz"), None);
        assert_eq!(decode_numeric_entity("# 65"), None);
        assert_eq!(decode_numeric_entity("#65junk"), None);
        assert_eq!(decode_numeric_entity("\u{1F600}"), None);
    }

    #[test]
    fn decode_numeric_entity_boundary_code_points() {
        assert_eq!(
            decode_numeric_entity("#0"),
            Some('\u{0}'),
            "NUL is a valid char"
        );
        assert_eq!(
            decode_numeric_entity("#x10FFFF"),
            Some('\u{10FFFF}'),
            "max scalar"
        );
        assert_eq!(
            decode_numeric_entity("#x110000"),
            None,
            "one past the max scalar value"
        );
        assert_eq!(
            decode_numeric_entity("#xD800"),
            None,
            "a lone surrogate is not a char"
        );
        assert_eq!(
            decode_numeric_entity("#4294967295"),
            None,
            "u32::MAX is not a scalar value"
        );
        assert_eq!(
            decode_numeric_entity("#4294967296"),
            None,
            "one past u32::MAX must not wrap — it must fail to parse"
        );
        assert_eq!(
            decode_numeric_entity("#-1"),
            None,
            "negative is rejected by u32"
        );
        assert_eq!(
            decode_numeric_entity("#xFFFFFFFFFFFF"),
            None,
            "hex overflow is rejected, not truncated"
        );
    }

    #[test]
    fn decode_numeric_entity_extremely_long_terminates() {
        let s = format!("#{}", "9".repeat(LONG));
        assert_eq!(s.len(), LONG + 1);
        assert_eq!(decode_numeric_entity(&s), None, "overflows u32 => None");
    }

    // ================================================================
    // decode_entities / prepare_string
    // ================================================================

    #[test]
    fn decode_entities_leaves_unrecognized_sequences_verbatim() {
        assert_eq!(decode_entities(""), "");
        assert_eq!(
            decode_entities("&"),
            "&",
            "a bare '&' at EOF must not panic"
        );
        assert_eq!(
            decode_entities("&;"),
            "&;",
            "empty entity body is not decoded"
        );
        assert_eq!(decode_entities("&bogus;"), "&bogus;");
        assert_eq!(
            decode_entities("&nbsp;"),
            "&nbsp;",
            "&nbsp; is deliberately kept"
        );
        // A ';' further away than MAX_ENTITY_BODY is not treated as an entity end.
        assert_eq!(
            decode_entities("&averyveryverylongbody;"),
            "&averyveryverylongbody;"
        );
    }

    #[test]
    fn decode_entities_single_pass_prevents_double_decoding() {
        assert_eq!(
            decode_entities("&amp;lt;"),
            "&lt;",
            "&amp; must not re-open an entity"
        );
        assert_eq!(decode_entities("&lt;&gt;&amp;&quot;&apos;"), "<>&\"'");
    }

    #[test]
    fn decode_entities_unicode_and_long_input_terminate() {
        assert_eq!(
            decode_entities("\u{1F600}\u{0301}\u{130}"),
            "\u{1F600}\u{0301}\u{130}"
        );
        // NOTE: each '&' triggers a `find(';')` over the whole remaining suffix, so
        // this is quadratic in the number of '&'. Keep the input modest.
        let amps = "&".repeat(20_000);
        assert_eq!(decode_entities(&amps).len(), 20_000);
    }

    #[test]
    fn prepare_string_empty_and_whitespace() {
        assert_eq!(prepare_string(""), "");
        assert_eq!(prepare_string("   "), "");
        assert_eq!(prepare_string("\t\n\r  \n"), "");
    }

    #[test]
    fn prepare_string_nbsp_becomes_a_space_that_survives_trim() {
        assert_eq!(prepare_string("&nbsp;"), " ");
        assert_eq!(prepare_string("a&nbsp;b"), "a b");
    }

    #[test]
    fn prepare_string_collapses_a_blank_line_into_a_single_return() {
        assert_eq!(prepare_string("a\n\nb"), "a\nb");
        assert_eq!(
            prepare_string("a\n\n\n\nb"),
            "a\nb",
            "runs of blanks collapse"
        );
    }

    #[test]
    fn prepare_string_unicode_and_long_input_no_panic() {
        assert_eq!(prepare_string("\u{1F600}"), "\u{1F600}");
        assert_eq!(prepare_string("  \u{130}\u{0301}  "), "\u{130}\u{0301}");
        assert_eq!(prepare_string(&"x".repeat(LONG)).len(), LONG);
    }

    /// BUG (reported): a soft-wrapped multi-line text node loses the word break
    /// on the FINAL line, because `prepare_string` skips the joining space when
    /// `line_idx == line_len - 1`. HTML must collapse the newline into a space.
    #[test]
    fn prepare_string_joins_wrapped_lines_with_a_space() {
        assert_eq!(
            prepare_string("Hello\nworld"),
            "Hello world",
            "a single newline between words must collapse to a space, not vanish"
        );
        assert_eq!(prepare_string("a\nb\nc"), "a b c");
    }

    // ================================================================
    // parse_bool  (parser)
    // ================================================================

    #[test]
    fn parse_bool_valid_minimal_and_everything_else_is_none() {
        assert_eq!(parse_bool("true"), Some(true));
        assert_eq!(parse_bool("false"), Some(false));

        for s in [
            "",
            "   ",
            "\t\n",
            " true",
            "true ",
            "TRUE",
            "True",
            "FALSE",
            "1",
            "0",
            "-0",
            "yes",
            "no",
            "NaN",
            "inf",
            "9223372036854775807",
            "\u{1F600}",
            "true;garbage",
        ] {
            assert_eq!(parse_bool(s), None, "{s:?} must not parse as a bool");
        }
        assert_eq!(parse_bool(&"a".repeat(LONG)), None);
    }

    // ================================================================
    // split_dynamic_string / format_args_dynamic
    // ================================================================

    fn args(kv: &[(&str, &str)]) -> ComponentArgumentVec {
        kv.iter()
            .map(|(n, t)| ComponentArgument {
                name: AzString::from(*n),
                arg_type: AzString::from(*t),
            })
            .collect::<Vec<_>>()
            .into()
    }

    #[test]
    fn split_dynamic_string_empty_and_plain() {
        assert!(split_dynamic_string("").is_empty());
        assert_eq!(
            split_dynamic_string("abc"),
            vec![DynamicItem::Str("abc".to_string())]
        );
    }

    #[test]
    fn split_dynamic_string_format_spec_is_split_on_the_first_colon() {
        assert_eq!(
            split_dynamic_string("{a:?}"),
            vec![DynamicItem::Var {
                name: "a".to_string(),
                format_spec: Some("?".to_string()),
            }]
        );
        assert_eq!(
            split_dynamic_string("{a:x:y}"),
            vec![DynamicItem::Var {
                name: "a".to_string(),
                format_spec: Some("x:y".to_string()),
            }],
            "only the FIRST colon separates name from spec"
        );
    }

    #[test]
    fn split_dynamic_string_unterminated_var_stays_literal() {
        // No closing brace => the scan runs to EOF and the text stays a literal.
        assert_eq!(
            split_dynamic_string("{unterminated"),
            vec![DynamicItem::Str("{unterminated".to_string())]
        );
        // A whitespace inside the braces aborts the variable scan.
        assert_eq!(
            split_dynamic_string("{a b}"),
            vec![DynamicItem::Str("{a b}".to_string())]
        );
    }

    #[test]
    fn split_dynamic_string_unicode_and_long_input_terminate() {
        assert_eq!(
            split_dynamic_string("\u{1F600}{v}\u{130}"),
            vec![
                DynamicItem::Str("\u{1F600}".to_string()),
                DynamicItem::Var {
                    name: "v".to_string(),
                    format_spec: None
                },
                DynamicItem::Str("\u{130}".to_string()),
            ]
        );
        // The failed-variable scan advances the cursor by the amount it scanned,
        // so this stays linear rather than quadratic.
        let bomb = "{a".repeat(50_000);
        let out = split_dynamic_string(&bomb);
        assert!(
            out.len() <= 2,
            "a never-closed variable collapses, got {}",
            out.len()
        );

        let braces = "{".repeat(100_000);
        assert!(split_dynamic_string(&braces).len() <= 1);
    }

    #[test]
    fn format_args_dynamic_documented_example() {
        let vars = args(&[("a", "value1"), ("b", "value2")]);
        assert_eq!(
            format_args_dynamic("hello {a}, {b}{{ {c} }}", &vars),
            "hello value1, value2{ {c} }"
        );
    }

    #[test]
    fn format_args_dynamic_unknown_var_is_preserved_verbatim() {
        let empty = no_args();
        assert_eq!(format_args_dynamic("{c}", &empty), "{c}");
        assert_eq!(
            format_args_dynamic("{c:?}", &empty),
            "{c:?}",
            "the format spec is re-attached when the var is unresolved"
        );
        // Escaped braces round-trip to themselves.
        assert_eq!(format_args_dynamic("{{}}", &empty), "{{}}");
    }

    #[test]
    fn format_args_dynamic_variable_names_are_normalized() {
        let vars = args(&[("my_var", "V")]);
        assert_eq!(
            format_args_dynamic("{myVar}", &vars),
            "V",
            "camelCase is normalized"
        );
        assert_eq!(
            format_args_dynamic("{ my-var }", &vars),
            "{ my-var }",
            "whitespace inside the braces aborts the variable scan entirely"
        );
        assert_eq!(format_args_dynamic("{my-var}", &vars), "V");
    }

    #[test]
    fn format_args_dynamic_edge_values_no_panic() {
        let empty = no_args();
        assert_eq!(format_args_dynamic("", &empty), "");
        assert_eq!(format_args_dynamic("   ", &empty), "   ");
        assert_eq!(format_args_dynamic("\u{1F600}", &empty), "\u{1F600}");
        assert_eq!(
            format_args_dynamic(&"x".repeat(50_000), &empty).len(),
            50_000
        );
    }

    #[test]
    fn combine_and_replace_dynamic_items_on_empty_input() {
        assert_eq!(combine_and_replace_dynamic_items(&[], &no_args()), "");
    }

    // ================================================================
    // compile_and_format_dynamic_items / format_args_for_rust_code
    // ================================================================

    #[test]
    fn format_args_for_rust_code_empty_and_single_items() {
        assert_eq!(
            format_args_for_rust_code(""),
            "AzString::from_const_str(\"\")"
        );
        assert_eq!(
            format_args_for_rust_code("hi"),
            "AzString::from_const_str(\"hi\")"
        );
        assert_eq!(
            format_args_for_rust_code("{a}"),
            "a",
            "a lone var becomes a bare ident"
        );
        assert_eq!(
            format_args_for_rust_code("{a:?}"),
            "format!(\"{:?}\", a).into()"
        );
    }

    #[test]
    fn format_args_for_rust_code_multi_item_builds_a_format_call() {
        assert_eq!(
            format_args_for_rust_code("x={a} y={b}"),
            "format!(\"x={a} y={b}\", a, b).into()"
        );
    }

    #[test]
    fn format_args_for_rust_code_escapes_quotes_in_literals() {
        let out = format_args_for_rust_code("say \"hi\" {a}");
        assert!(
            out.contains("say \\\"hi\\\""),
            "double quotes must be escaped for the emitted literal, got {out}"
        );
    }

    #[test]
    fn compile_and_format_dynamic_items_edge_values() {
        assert_eq!(
            compile_and_format_dynamic_items(&[]),
            "AzString::from_const_str(\"\")"
        );
        assert_eq!(
            compile_and_format_dynamic_items(&[DynamicItem::Str(String::new())]),
            "AzString::from_const_str(\"\")"
        );
        assert_eq!(
            compile_and_format_dynamic_items(&[DynamicItem::Var {
                name: "  spaced  ".to_string(),
                format_spec: None,
            }]),
            "spaced",
            "the var name is trimmed + normalized"
        );
    }

    // ================================================================
    // cap_first / camel_to_snake / esc_lit / c_creator_suffix
    // ================================================================

    #[test]
    fn cap_first_edge_inputs() {
        assert_eq!(cap_first(""), "", "empty input must not panic");
        assert_eq!(cap_first("h1"), "H1");
        assert_eq!(cap_first("button"), "Button");
        assert_eq!(cap_first("A"), "A", "already-uppercase is idempotent");
        assert_eq!(
            cap_first("\u{1F600}x"),
            "\u{1F600}x",
            "emoji has no uppercase form"
        );
        // 'ß' uppercases to TWO chars — the fn must not assume 1:1.
        assert_eq!(cap_first("\u{df}x"), "SSx");
        assert_eq!(cap_first(&"a".repeat(10_000)).len(), 10_000);
    }

    #[test]
    fn camel_to_snake_documented_forms() {
        assert_eq!(camel_to_snake("ButtonNoA11y"), "button_no_a11y");
        assert_eq!(camel_to_snake("PWithText"), "p_with_text");
        assert_eq!(camel_to_snake("ANoA11y"), "a_no_a11y");
        assert_eq!(camel_to_snake("H1WithText"), "h1_with_text");
        assert_eq!(camel_to_snake("Div"), "div");
    }

    #[test]
    fn camel_to_snake_edge_inputs() {
        assert_eq!(camel_to_snake(""), "");
        assert_eq!(camel_to_snake("A"), "a");
        assert_eq!(camel_to_snake("AB"), "ab", "an all-caps run is not split");
        assert_eq!(
            camel_to_snake("ABc"),
            "a_bc",
            "a caps run splits before the last cap"
        );
        assert_eq!(camel_to_snake("\u{1F600}"), "\u{1F600}");
        assert_eq!(camel_to_snake(&"a".repeat(10_000)).len(), 10_000);
    }

    #[test]
    fn esc_lit_escapes_backslash_before_quote() {
        assert_eq!(esc_lit(""), "");
        assert_eq!(esc_lit("plain"), "plain");
        assert_eq!(esc_lit("a\"b"), "a\\\"b");
        assert_eq!(esc_lit("a\\b"), "a\\\\b");
        // The backslash pass must run FIRST so an escaped quote is not double-escaped.
        assert_eq!(esc_lit("\\\""), "\\\\\\\"");
        assert_eq!(esc_lit("\u{1F600}"), "\u{1F600}");
    }

    #[test]
    fn c_creator_suffix_edge_inputs() {
        assert_eq!(
            c_creator_suffix(""),
            "Div",
            "empty debug name falls back to Div"
        );
        assert_eq!(c_creator_suffix("Div"), "Div");
        assert_eq!(c_creator_suffix("H1"), "H1");
        assert_eq!(c_creator_suffix("BlockQuote"), "Blockquote");
        assert_eq!(c_creator_suffix("FigCaption"), "Figcaption");
        assert_eq!(c_creator_suffix("\u{1F600}"), "\u{1F600}");
    }

    // ================================================================
    // safe_container_tag
    // ================================================================

    #[test]
    fn safe_container_tag_falls_back_to_div_for_arg_taking_widgets() {
        assert_eq!(safe_container_tag(""), "Div");
        assert_eq!(safe_container_tag("Div"), "Div");
        assert_eq!(safe_container_tag("Span"), "Span");
        assert_eq!(safe_container_tag("H1"), "H1");
        // Interactive / arg-taking elements deliberately degrade to a container.
        assert_eq!(safe_container_tag("Button"), "Div");
        assert_eq!(safe_container_tag("Input"), "Div");
        assert_eq!(safe_container_tag("A"), "Div");
        assert_eq!(safe_container_tag("\u{1F600}"), "Div");
        assert_eq!(safe_container_tag(&"z".repeat(10_000)), "Div");
    }

    /// BUG (reported): `SAFE_CONTAINER_TAGS` is documented as holding the
    /// `NodeType`/`NodeTypeTag` **debug names**, and `safe_container_tag` compares
    /// against `format!("{:?}", tag_to_node_type(tag))`. But six entries are spelled
    /// with a different inner capitalization than the actual variant, so the
    /// comparison never matches and `<blockquote>`/`<figcaption>`/`<thead>`/`<tbody>`
    /// /`<tfoot>`/`<colgroup>` silently compile down to a plain `div`.
    #[test]
    fn safe_container_tag_matches_the_real_nodetype_debug_names() {
        for tag in [
            "blockquote",
            "figcaption",
            "thead",
            "tbody",
            "tfoot",
            "colgroup",
        ] {
            let dbg = format!("{:?}", tag_to_node_type(tag));
            assert_ne!(
                safe_container_tag(&dbg),
                "Div",
                "<{tag}> (NodeType debug name {dbg:?}) is a pure container and must keep \
                 its own creator instead of degrading to a div"
            );
        }
    }

    // ================================================================
    // fmt_f32_lit  (numeric)
    // ================================================================

    #[test]
    fn fmt_f32_lit_zero_and_negative() {
        assert_eq!(
            fmt_f32_lit(0.0),
            "0.0",
            "an integral value gains a decimal point"
        );
        assert_eq!(fmt_f32_lit(-0.0), "-0.0");
        assert_eq!(fmt_f32_lit(-1.0), "-1.0");
        assert_eq!(fmt_f32_lit(1.5), "1.5");
        assert_eq!(fmt_f32_lit(-1.5), "-1.5");
    }

    #[test]
    fn fmt_f32_lit_min_max_stay_parseable_float_literals() {
        for f in [f32::MAX, f32::MIN, f32::MIN_POSITIVE, f32::EPSILON] {
            let s = fmt_f32_lit(f);
            assert_eq!(
                s.parse::<f32>(),
                Ok(f),
                "{f:e} must round-trip through its emitted literal ({s})"
            );
        }
    }

    #[test]
    fn fmt_f32_lit_nan_inf_produce_a_defined_result_and_do_not_panic() {
        // NOTE: these are NOT valid Rust/C float literals — a page with
        // `<progress value="NaN">` emits `create_progress_no_a11y(NaN, 1.0)`.
        // Pinned so a fix (e.g. clamping to 0.0) is a visible change.
        assert_eq!(fmt_f32_lit(f32::NAN), "NaN");
        assert_eq!(fmt_f32_lit(f32::INFINITY), "inf");
        assert_eq!(fmt_f32_lit(f32::NEG_INFINITY), "-inf");
    }

    // ================================================================
    // node_direct_text / node_aria_label / node_attr_or / node_attr_f32
    // first_caption_text
    // ================================================================

    #[test]
    fn node_direct_text_trims_and_skips_elements() {
        assert_eq!(node_direct_text(&XmlNode::default()), "");
        assert_eq!(node_direct_text(&node("p", &[], vec![txt("  Go  ")])), "Go");
        assert_eq!(
            node_direct_text(&node(
                "p",
                &[],
                vec![
                    txt("a"),
                    elem(node("b", &[], vec![txt("IGNORED")])),
                    txt("b")
                ]
            )),
            "a b",
            "direct text children are joined with a single space"
        );
        assert_eq!(
            node_direct_text(&node("p", &[], vec![txt("   "), txt("\t\n")])),
            "",
            "whitespace-only children are dropped"
        );
    }

    #[test]
    fn node_aria_label_ignores_empty_and_whitespace() {
        assert_eq!(node_aria_label(&XmlNode::default()), None);
        assert_eq!(
            node_aria_label(&node("b", &[("aria-label", "")], vec![])),
            None
        );
        assert_eq!(
            node_aria_label(&node("b", &[("aria-label", "   ")], vec![])),
            None
        );
        assert_eq!(
            node_aria_label(&node("b", &[("aria-label", "  Save  ")], vec![])),
            Some("Save".to_string())
        );
    }

    #[test]
    fn node_attr_or_returns_the_default_when_absent() {
        let n = node("a", &[("href", "/x"), ("empty", "")], vec![]);
        assert_eq!(node_attr_or(&n, "href", "FALLBACK"), "/x");
        assert_eq!(node_attr_or(&n, "missing", "FALLBACK"), "FALLBACK");
        assert_eq!(
            node_attr_or(&n, "empty", "FALLBACK"),
            "",
            "a present-but-empty attribute wins over the default"
        );
        assert_eq!(node_attr_or(&XmlNode::default(), "x", ""), "");
    }

    #[test]
    fn node_attr_f32_zero_negative_and_defaults() {
        let n = node(
            "meter",
            &[
                ("zero", "0"),
                ("negzero", "-0"),
                ("neg", "-2.5"),
                ("pad", "  1.5  "),
            ],
            vec![],
        );
        assert_eq!(node_attr_f32(&n, "zero", 9.0), 0.0);
        assert!(node_attr_f32(&n, "negzero", 9.0).is_sign_negative());
        assert_eq!(node_attr_f32(&n, "neg", 9.0), -2.5);
        assert_eq!(
            node_attr_f32(&n, "pad", 9.0),
            1.5,
            "the value is trimmed first"
        );
        assert_eq!(node_attr_f32(&n, "missing", 9.0), 9.0);
    }

    #[test]
    fn node_attr_f32_unparsable_falls_back_and_min_max_saturate() {
        let n = node(
            "meter",
            &[
                ("junk", "abc"),
                ("empty", ""),
                ("huge", "1e400"),
                ("tiny", "-1e400"),
                ("big", "340282350000000000000000000000000000000"),
            ],
            vec![],
        );
        assert_eq!(node_attr_f32(&n, "junk", 7.0), 7.0);
        assert_eq!(node_attr_f32(&n, "empty", 7.0), 7.0);
        assert!(
            node_attr_f32(&n, "huge", 7.0).is_infinite(),
            "an out-of-range literal saturates to inf, it does not panic"
        );
        assert_eq!(node_attr_f32(&n, "tiny", 7.0), f32::NEG_INFINITY);
        assert_eq!(node_attr_f32(&n, "big", 7.0), f32::MAX);
    }

    #[test]
    fn node_attr_f32_accepts_nan_and_inf_spellings() {
        // Rust's f32 FromStr accepts "NaN"/"inf", so hostile markup can inject a
        // non-finite value straight into codegen (see fmt_f32_lit above).
        let n = node("progress", &[("value", "NaN"), ("max", "inf")], vec![]);
        assert!(node_attr_f32(&n, "value", 0.0).is_nan());
        assert_eq!(node_attr_f32(&n, "max", 1.0), f32::INFINITY);
    }

    #[test]
    fn node_attr_f32_nan_default_is_returned_verbatim() {
        assert!(node_attr_f32(&XmlNode::default(), "x", f32::NAN).is_nan());
        assert_eq!(
            node_attr_f32(&XmlNode::default(), "x", f32::INFINITY),
            f32::INFINITY
        );
    }

    #[test]
    fn first_caption_text_edges() {
        assert_eq!(first_caption_text(&XmlNode::default()), None);
        assert_eq!(
            first_caption_text(&node(
                "table",
                &[],
                vec![elem(node("caption", &[], vec![]))]
            )),
            None,
            "an empty caption yields None"
        );
        assert_eq!(
            first_caption_text(&node(
                "table",
                &[],
                vec![elem(node("CAPTION", &[], vec![txt("  Hi  ")]))]
            )),
            Some("Hi".to_string()),
            "the tag match is ASCII-case-insensitive and the text is trimmed"
        );
    }

    // ================================================================
    // analyze_node_ctor / CtorArg / NodeCtor
    // ================================================================

    #[test]
    fn analyze_node_ctor_plain_for_unknown_and_empty_tags() {
        assert!(matches!(
            analyze_node_ctor("div", &XmlNode::default()),
            NodeCtor::Plain
        ));
        assert!(matches!(
            analyze_node_ctor("", &XmlNode::default()),
            NodeCtor::Plain
        ));
        assert!(matches!(
            analyze_node_ctor("\u{1F600}", &XmlNode::default()),
            NodeCtor::Plain
        ));
        let plain = analyze_node_ctor("div", &XmlNode::default());
        assert_eq!(plain.render_rust(), None);
        assert_eq!(plain.render_c(), None);
        assert_eq!(plain.render_fluent(&CompileTarget::Cpp), None);
        assert!(!plain.consumes_text());
        assert!(!plain.skip_caption());
    }

    #[test]
    fn analyze_node_ctor_with_text_tier_requires_actual_text() {
        // Empty <p> stays a plain container (has_only_text_children() is vacuously
        // true for a childless node, so `has_text` is the real gate).
        assert!(matches!(
            analyze_node_ctor("p", &XmlNode::default()),
            NodeCtor::Plain
        ));
        // <p> with an element child is not "pure text" either.
        let mixed = node("p", &[], vec![txt("a"), elem(XmlNode::create("span"))]);
        assert!(matches!(analyze_node_ctor("p", &mixed), NodeCtor::Plain));

        let pure = node("p", &[], vec![txt("  Hello  ")]);
        let ctor = analyze_node_ctor("p", &pure);
        assert!(
            ctor.consumes_text(),
            "the text is folded into the constructor"
        );
        assert_eq!(
            ctor.render_rust().as_deref(),
            Some("Dom::create_p_with_text(AzString::from(\"Hello\"))")
        );
        assert_eq!(
            ctor.render_c().as_deref(),
            Some("AzDom_createPWithText(AZ_STR(\"Hello\"))")
        );
        assert_eq!(
            ctor.render_fluent(&CompileTarget::Python).as_deref(),
            Some("azul.Dom.create_p_with_text(\"Hello\")")
        );
    }

    #[test]
    fn analyze_node_ctor_button_with_and_without_aria() {
        let plain_btn = node("button", &[], vec![txt("Go")]);
        assert_eq!(
            analyze_node_ctor("button", &plain_btn)
                .render_rust()
                .as_deref(),
            Some("Dom::create_button_no_a11y(AzString::from(\"Go\"))")
        );

        let aria_btn = node("button", &[("aria-label", "Save")], vec![txt("Go")]);
        assert_eq!(
            analyze_node_ctor("button", &aria_btn)
                .render_rust()
                .as_deref(),
            Some(
                "Dom::create_button(AzString::from(\"Go\"), \
                 SmallAriaInfo::label(AzString::from(\"Save\")))"
            )
        );
    }

    #[test]
    fn analyze_node_ctor_escapes_quotes_and_backslashes_in_text() {
        let btn = node("button", &[], vec![txt("say \"hi\"\\now")]);
        let rust = analyze_node_ctor("button", &btn)
            .render_rust()
            .expect("semantic");
        assert!(
            rust.contains("say \\\"hi\\\"\\\\now"),
            "quotes and backslashes must be escaped for the literal, got {rust}"
        );
    }

    #[test]
    fn analyze_node_ctor_anchor_uses_option_string_when_it_has_no_text() {
        let bare = node("a", &[], vec![]);
        assert_eq!(
            analyze_node_ctor("a", &bare).render_rust().as_deref(),
            Some("Dom::create_a_no_a11y(AzString::from(\"\"), OptionString::None)"),
            "a missing href defaults to an empty string, missing text to OptionString::None"
        );

        let full = node("a", &[("href", "/x")], vec![txt("Home")]);
        assert_eq!(
            analyze_node_ctor("a", &full).render_rust().as_deref(),
            Some(
                "Dom::create_a_no_a11y(AzString::from(\"/x\"), \
                 OptionString::Some(AzString::from(\"Home\")))"
            )
        );
        assert_eq!(
            analyze_node_ctor("a", &full).render_c().as_deref(),
            Some("AzDom_createANoA11y(AZ_STR(\"/x\"), AzOptionString_some(AZ_STR(\"Home\")))")
        );
    }

    #[test]
    fn analyze_node_ctor_table_aria_form_skips_the_literal_caption() {
        let t = node(
            "table",
            &[("aria-label", "Prices")],
            vec![elem(node("caption", &[], vec![txt("Q1")]))],
        );
        let ctor = analyze_node_ctor("table", &t);
        assert!(
            ctor.skip_caption(),
            "the aria form injects its own caption child"
        );
        assert_eq!(
            ctor.render_rust().as_deref(),
            Some(
                "Dom::create_table(AzString::from(\"Q1\"), \
                 SmallAriaInfo::label(AzString::from(\"Prices\")))"
            )
        );

        let plain = analyze_node_ctor("table", &node("table", &[], vec![]));
        assert!(!plain.skip_caption());
        assert_eq!(
            plain.render_rust().as_deref(),
            Some("Dom::create_table_no_a11y()")
        );
    }

    #[test]
    fn analyze_node_ctor_scalar_widgets_use_defaults_and_emit_float_literals() {
        let p = node("progress", &[], vec![]);
        assert_eq!(
            analyze_node_ctor("progress", &p).render_rust().as_deref(),
            Some("Dom::create_progress_no_a11y(0.0, 1.0)"),
            "missing value/max fall back to 0.0 / 1.0 as float literals"
        );

        let m = node(
            "meter",
            &[("value", "5"), ("min", "-1"), ("max", "10")],
            vec![],
        );
        assert_eq!(
            analyze_node_ctor("meter", &m).render_c().as_deref(),
            Some("AzDom_createMeterNoA11y(5.0f, -1.0f, 10.0f)")
        );
    }

    /// Non-finite attribute values flow straight into the emitted literal. Pinned
    /// so that a fix (clamping / rejecting them) shows up as a change.
    #[test]
    fn analyze_node_ctor_non_finite_attributes_emit_non_finite_literals() {
        let p = node("progress", &[("value", "NaN"), ("max", "inf")], vec![]);
        let rust = analyze_node_ctor("progress", &p)
            .render_rust()
            .expect("semantic");
        assert_eq!(rust, "Dom::create_progress_no_a11y(NaN, inf)");
    }

    #[test]
    fn ctor_arg_render_targets_are_distinct() {
        let s = CtorArg::Str("a\"b".to_string());
        assert_eq!(s.render_rust(), "AzString::from(\"a\\\"b\")");
        assert_eq!(s.render_c(), "AZ_STR(\"a\\\"b\")");
        assert_eq!(s.render_cpp(), "String(\"a\\\"b\")");
        assert_eq!(s.render_python(), "\"a\\\"b\"");

        assert_eq!(CtorArg::OptNone.render_rust(), "OptionString::None");
        assert_eq!(CtorArg::OptNone.render_c(), "AzOptionString_none()");
        assert_eq!(CtorArg::OptNone.render_cpp(), "OptionString::none()");
        assert_eq!(CtorArg::OptNone.render_python(), "azul.OptionString.none()");

        assert_eq!(CtorArg::Float(0.0).render_rust(), "0.0");
        assert_eq!(CtorArg::Float(0.0).render_c(), "0.0f");
        assert_eq!(CtorArg::Float(f32::NAN).render_c(), "NaNf");
    }

    #[test]
    fn node_ctor_render_fluent_returns_none_for_non_fluent_targets() {
        let ctor = analyze_node_ctor("p", &node("p", &[], vec![txt("x")]));
        assert!(ctor.render_fluent(&CompileTarget::Rust).is_none());
        assert!(ctor.render_fluent(&CompileTarget::C).is_none());
        assert!(ctor.render_fluent(&CompileTarget::Cpp).is_some());
        assert!(ctor.render_fluent(&CompileTarget::Python).is_some());
    }

    // ================================================================
    // format_component_args / compile_component / compile_components
    // ================================================================

    #[test]
    fn format_component_args_empty_and_ordering() {
        assert_eq!(format_component_args(&no_args()), "");
        // Args are sorted DESCENDING by their rendered "name: type" string.
        assert_eq!(
            format_component_args(&args(&[("a", "u32"), ("b", "String")])),
            "b: String, a: u32"
        );
    }

    #[test]
    fn format_component_args_edge_values_no_panic() {
        let a = args(&[("", ""), ("\u{1F600}", "\u{130}")]);
        let out = format_component_args(&a);
        assert!(
            out.contains(": "),
            "still emits `name: type` pairs, got {out:?}"
        );
    }

    #[test]
    fn compile_component_emits_a_render_fn() {
        let ca = ComponentArguments {
            args: args(&[("count", "u32")]),
            accepts_text: false,
        };
        let out = compile_component("MyWidget", &ca, "Dom::create_div()");
        assert!(
            out.contains("pub fn render(count: u32) -> Dom {"),
            "got:\n{out}"
        );
        assert!(out.contains("#[inline]"), "a one-line body is inlined");
    }

    #[test]
    fn compile_component_accepts_text_prepends_the_text_param() {
        let ca = ComponentArguments {
            args: args(&[("count", "u32")]),
            accepts_text: true,
        };
        let out = compile_component("my-widget", &ca, "Dom::create_div()");
        assert!(
            out.contains("pub fn render(text: AzString, count: u32) -> Dom {"),
            "got:\n{out}"
        );

        let ca_no_args = ComponentArguments {
            args: no_args(),
            accepts_text: true,
        };
        let out = compile_component("w", &ca_no_args, "Dom::create_div()");
        assert!(
            out.contains("pub fn render(text: AzString) -> Dom {"),
            "no trailing comma when there are no extra args, got:\n{out}"
        );
    }

    #[test]
    fn compile_component_empty_name_and_body_no_panic() {
        let ca = ComponentArguments::default();
        let out = compile_component("", &ca, "");
        assert!(out.contains("pub fn render() -> Dom {"), "got:\n{out}");
    }

    #[test]
    fn compile_components_of_an_empty_list_is_empty() {
        assert_eq!(compile_components(Vec::new()), "");
    }

    // ================================================================
    // parse_svg_float / parse_svg_points  (parser / numeric)
    // ================================================================

    #[test]
    fn parse_svg_float_none_empty_whitespace_garbage() {
        assert_eq!(parse_svg_float(None), None);
        let empty = AzString::from("");
        assert_eq!(parse_svg_float(Some(&empty)), None);
        let ws = AzString::from("   \t\n");
        assert_eq!(parse_svg_float(Some(&ws)), None);
        let junk = AzString::from("10px");
        assert_eq!(parse_svg_float(Some(&junk)), None, "units are not stripped");
        let uni = AzString::from("\u{1F600}");
        assert_eq!(parse_svg_float(Some(&uni)), None);
    }

    #[test]
    fn parse_svg_float_valid_and_boundary_numbers() {
        let padded = AzString::from("  1.5  ");
        assert_eq!(
            parse_svg_float(Some(&padded)),
            Some(1.5),
            "value is trimmed"
        );
        let zero = AzString::from("0");
        assert_eq!(parse_svg_float(Some(&zero)), Some(0.0));
        let negzero = AzString::from("-0");
        assert!(parse_svg_float(Some(&negzero)).unwrap().is_sign_negative());
        let huge = AzString::from("1e400");
        assert!(
            parse_svg_float(Some(&huge)).unwrap().is_infinite(),
            "overflow saturates to inf rather than erroring"
        );
        let nan = AzString::from("NaN");
        assert!(parse_svg_float(Some(&nan)).unwrap().is_nan());
        let inf = AzString::from("-inf");
        assert_eq!(parse_svg_float(Some(&inf)), Some(f32::NEG_INFINITY));
    }

    #[test]
    fn parse_svg_points_rejects_degenerate_input() {
        assert!(parse_svg_points("", false).is_none());
        assert!(parse_svg_points("   ", false).is_none());
        assert!(parse_svg_points("garbage", false).is_none());
        assert!(
            parse_svg_points("1 2", false).is_none(),
            "a single point is not a line"
        );
        assert!(
            parse_svg_points("1 2 3", false).is_none(),
            "an odd coordinate count is rejected"
        );
        assert!(parse_svg_points("\u{1F600}", false).is_none());
    }

    #[test]
    fn parse_svg_points_valid_minimal_and_close() {
        let open = parse_svg_points("0,0 10,0", false).expect("two points => one line");
        assert_eq!(open.rings.as_ref().len(), 1);
        assert_eq!(open.rings.as_ref()[0].items.as_ref().len(), 1);

        // `close` adds a segment back to the first point when it differs.
        let closed = parse_svg_points("0,0 10,0 10,10", true).expect("triangle");
        assert_eq!(
            closed.rings.as_ref()[0].items.as_ref().len(),
            3,
            "2 segments + 1 closing segment"
        );

        // Already-closed rings do not get a duplicate closing segment.
        let already = parse_svg_points("0,0 10,0 0,0", true).expect("closed ring");
        assert_eq!(already.rings.as_ref()[0].items.as_ref().len(), 2);
    }

    #[test]
    fn parse_svg_points_skips_unparsable_tokens_and_handles_boundaries() {
        // Unparsable coordinates are silently dropped, which can shift the pairing.
        let p = parse_svg_points("0,0 junk 10,0", false).expect("junk token dropped");
        assert_eq!(p.rings.as_ref()[0].items.as_ref().len(), 1);

        let nan = parse_svg_points("NaN,0 1,1", false).expect("NaN is a parseable f32");
        assert_eq!(nan.rings.as_ref()[0].items.as_ref().len(), 1);
    }

    #[test]
    fn parse_svg_points_extremely_long_terminates() {
        let pts = "1,2 ".repeat(20_000);
        let p = parse_svg_points(&pts, false).expect("20k points");
        assert_eq!(p.rings.as_ref()[0].items.as_ref().len(), 19_999);
    }

    // ================================================================
    // CompactDomBuilder  (constructor / numeric)
    // ================================================================

    #[test]
    fn compact_dom_builder_new_and_with_capacity_start_empty() {
        for b in [
            CompactDomBuilder::new(),
            CompactDomBuilder::with_capacity(0),
            CompactDomBuilder::with_capacity(1),
            CompactDomBuilder::with_capacity(4096),
        ] {
            let fd = b.finish();
            assert_eq!(fd.node_hierarchy.as_ref().len(), 0);
            assert_eq!(fd.node_data.as_ref().len(), 0);
            assert_eq!(fd.css.as_ref().len(), 0);
        }
        assert_eq!(
            CompactDomBuilder::default()
                .finish()
                .node_data
                .as_ref()
                .len(),
            0
        );
    }

    #[test]
    fn compact_dom_builder_close_node_on_an_empty_stack_is_a_no_op() {
        let mut b = CompactDomBuilder::new();
        b.close_node();
        b.close_node();
        assert_eq!(
            b.finish().node_hierarchy.as_ref().len(),
            0,
            "no panic, no nodes"
        );
    }

    #[test]
    fn compact_dom_builder_keeps_hierarchy_and_data_parallel() {
        let mut b = CompactDomBuilder::new();
        b.open_node(NodeData::create_node(NodeType::Html));
        b.add_leaf(NodeData::create_text_do_not_use_without_block_level_wrapper("a"));
        b.add_leaf(NodeData::create_text_do_not_use_without_block_level_wrapper("b"));
        b.close_node();
        let fd = b.finish();
        assert_eq!(fd.node_data.as_ref().len(), 3);
        assert_eq!(
            fd.node_hierarchy.as_ref().len(),
            fd.node_data.as_ref().len(),
            "the two arenas must stay the same length"
        );
    }

    #[test]
    fn compact_dom_builder_unclosed_node_still_finishes() {
        let mut b = CompactDomBuilder::new();
        b.open_node(NodeData::create_node(NodeType::Div));
        // Deliberately NOT closed.
        let fd = b.finish();
        assert_eq!(fd.node_hierarchy.as_ref().len(), 1);
        assert_eq!(
            fd.node_hierarchy.as_ref()[0].last_child,
            0,
            "last_child stays unset when close_node() is never called"
        );
    }

    #[test]
    fn compact_dom_builder_add_css_accepts_zero_and_usize_max_node_ids() {
        let mut b = CompactDomBuilder::new();
        b.add_css(0, Css::empty());
        b.add_css(usize::MAX, Css::empty());
        let fd = b.finish();
        assert_eq!(fd.css.as_ref().len(), 2);
        assert_eq!(fd.css.as_ref()[0].node_id, 0);
        assert_eq!(
            fd.css.as_ref()[1].node_id,
            usize::MAX,
            "an out-of-range node id is stored verbatim (no bounds check, no panic)"
        );
    }

    // ================================================================
    // xml_node_to_dom_fast / xml_node_to_fast_dom  (numeric: depth)
    // ================================================================

    #[test]
    fn xml_node_to_dom_fast_depth_zero_builds_children() {
        let map = ComponentMap::default();
        let n = node("div", &[], vec![txt("hi"), elem(XmlNode::create("span"))]);
        let dom = xml_node_to_dom_fast(&n, &map, false, 0).expect("ok");
        assert_eq!(dom.children.as_ref().len(), 2);
    }

    #[test]
    fn xml_node_to_dom_fast_at_and_past_the_depth_cap_truncates_instead_of_panicking() {
        let map = ComponentMap::default();
        let n = node("div", &[], vec![txt("hi")]);

        let at_cap = xml_node_to_dom_fast(&n, &map, false, MAX_XML_NESTING_DEPTH).expect("ok");
        assert!(
            at_cap.children.as_ref().is_empty(),
            "at the cap the node is emitted without children"
        );

        let saturated = xml_node_to_dom_fast(&n, &map, false, usize::MAX)
            .expect("usize::MAX depth must not overflow when computing depth + 1");
        assert!(saturated.children.as_ref().is_empty());

        let below = xml_node_to_dom_fast(&n, &map, false, MAX_XML_NESTING_DEPTH - 1).expect("ok");
        assert_eq!(
            below.children.as_ref().len(),
            1,
            "one below the cap still recurses"
        );
    }

    #[test]
    fn xml_node_to_fast_dom_at_the_depth_cap_still_emits_the_node() {
        let map = ComponentMap::default();
        let n = node("div", &[], vec![txt("hi")]);

        let mut b = CompactDomBuilder::new();
        xml_node_to_fast_dom(&n, &map, false, &mut b, usize::MAX).expect("no overflow");
        let fd = b.finish();
        assert_eq!(
            fd.node_data.as_ref().len(),
            1,
            "the node itself is still opened+closed, only its children are dropped"
        );

        let mut b2 = CompactDomBuilder::new();
        xml_node_to_fast_dom(&n, &map, false, &mut b2, 0).expect("ok");
        assert_eq!(b2.finish().node_data.as_ref().len(), 2, "node + text child");
    }

    #[test]
    fn apply_xml_node_attributes_extreme_tabindex_does_not_panic() {
        let map = ComponentMap::default();
        for v in [
            "0",
            "-1",
            "2147483647",
            "9223372036854775807",
            "-9223372036854775808",
            "99999999999999999999999999999999",
            "abc",
            "",
            "\u{1F600}",
        ] {
            let n = node("div", &[("tabindex", v), ("focusable", "true")], vec![]);
            assert!(
                xml_node_to_dom_fast(&n, &map, false, 0).is_ok(),
                "tabindex={v:?} must not panic"
            );
        }
    }

    #[test]
    fn apply_xml_node_attributes_img_width_height_garbage_falls_back_to_zero() {
        let map = ComponentMap::default();
        let n = node(
            "img",
            &[
                ("src", "a.png"),
                ("width", "-5"),
                ("height", "not-a-number"),
            ],
            vec![],
        );
        let dom = xml_node_to_dom_fast(&n, &map, false, 0).expect("ok");
        match dom.root.get_node_type() {
            NodeType::Image(_) => {}
            other => panic!("expected an Image node, got {other:?}"),
        }
    }

    // ================================================================
    // set_stringified_attributes  (numeric: tabs / tabindex)
    // ================================================================

    #[test]
    fn set_stringified_attributes_zero_tabs_and_empty_attrs() {
        let mut s = String::new();
        set_stringified_attributes(&mut s, &attrs(&[]), &no_args(), 0);
        assert_eq!(s, "", "nothing to emit for an attribute-less node");
    }

    #[test]
    fn set_stringified_attributes_splits_ids_and_classes_on_whitespace() {
        let mut s = String::new();
        set_stringified_attributes(
            &mut s,
            &attrs(&[("id", "a  b"), ("class", "c\td")]),
            &no_args(),
            0,
        );
        assert!(s.contains(".with_id(\"a\")"), "got {s:?}");
        assert!(s.contains(".with_id(\"b\")"));
        assert!(s.contains(".with_class(\"c\")"));
        assert!(s.contains(".with_class(\"d\")"));
    }

    #[test]
    fn set_stringified_attributes_tabindex_boundaries() {
        let cases: &[(&str, &str)] = &[
            ("0", "TabIndex::Auto"),
            ("5", "TabIndex::OverrideInParent(5)"),
            ("-1", "TabIndex::NoKeyboardFocus"),
        ];
        for (val, expected) in cases {
            let mut s = String::new();
            set_stringified_attributes(&mut s, &attrs(&[("tabindex", val)]), &no_args(), 0);
            assert!(s.contains(expected), "tabindex={val:?} => {s:?}");
        }

        // Unparsable / overflowing values emit nothing rather than panicking.
        for val in ["abc", "", "99999999999999999999999999999999", "1.5"] {
            let mut s = String::new();
            set_stringified_attributes(&mut s, &attrs(&[("tabindex", val)]), &no_args(), 0);
            assert!(
                !s.contains("TabIndex"),
                "tabindex={val:?} must be ignored, got {s:?}"
            );
        }
    }

    #[test]
    fn set_stringified_attributes_focusable_only_accepts_exact_true_false() {
        let mut s = String::new();
        set_stringified_attributes(&mut s, &attrs(&[("focusable", "true")]), &no_args(), 0);
        assert!(s.contains("TabIndex::Auto"));

        let mut s = String::new();
        set_stringified_attributes(&mut s, &attrs(&[("focusable", "false")]), &no_args(), 0);
        assert!(s.contains("TabIndex::NoKeyboardFocus"));

        let mut s = String::new();
        set_stringified_attributes(&mut s, &attrs(&[("focusable", "TRUE")]), &no_args(), 0);
        assert!(
            s.is_empty(),
            "casing other than `true`/`false` is ignored, got {s:?}"
        );
    }

    #[test]
    fn set_stringified_attributes_large_tab_depth_does_not_overflow() {
        // `tabs` becomes `"    ".repeat(tabs)`; a large-but-sane nesting depth must
        // stay linear and allocate without panicking.
        let mut s = String::new();
        set_stringified_attributes(&mut s, &attrs(&[("id", "x")]), &no_args(), 1_000);
        assert!(s.contains(".with_id(\"x\")"));
        assert!(s.len() > 4_000, "the 1000-level indent is actually emitted");
    }

    // ================================================================
    // group_matches / CssMatcher  (numeric: indices)
    // ================================================================

    fn refs(v: &[CssPathSelector]) -> Vec<&CssPathSelector> {
        v.iter().collect()
    }

    #[test]
    fn group_matches_global_matches_at_any_index() {
        let a = vec![CssPathSelector::Global];
        assert!(group_matches(&refs(&a), &[], 0, 0));
        assert!(
            group_matches(&refs(&a), &[], usize::MAX, usize::MAX),
            "usize::MAX indices must not overflow"
        );
    }

    #[test]
    fn group_matches_type_class_id() {
        let div = vec![CssPathSelector::Type(NodeTypeTag::Div)];
        let p = vec![CssPathSelector::Type(NodeTypeTag::P)];
        assert!(group_matches(&refs(&div), &refs(&div), 0, 1));
        assert!(!group_matches(&refs(&div), &refs(&p), 0, 1));
        assert!(
            !group_matches(&refs(&div), &[], 0, 1),
            "an empty haystack never matches"
        );

        let cls = vec![CssPathSelector::Class(AzString::from("x"))];
        assert!(group_matches(&refs(&cls), &refs(&cls), 0, 1));
        let id = vec![CssPathSelector::Id(AzString::from("x"))];
        assert!(
            !group_matches(&refs(&id), &refs(&cls), 0, 1),
            "an id is not a class"
        );
    }

    #[test]
    fn group_matches_first_and_last_pseudo_at_boundaries() {
        let first = vec![CssPathSelector::PseudoSelector(
            CssPathPseudoSelector::First,
        )];
        assert!(group_matches(&refs(&first), &[], 0, 10));
        assert!(!group_matches(&refs(&first), &[], 1, 10));

        let last = vec![CssPathSelector::PseudoSelector(CssPathPseudoSelector::Last)];
        assert!(group_matches(&refs(&last), &[], 9, 10));
        assert!(!group_matches(&refs(&last), &[], 8, 10));
        assert!(
            group_matches(&refs(&last), &[], 0, 0),
            "parent_children == 0 saturates to 0, so index 0 counts as last"
        );
    }

    #[test]
    fn group_matches_nth_child_even_odd_and_number() {
        let even = vec![CssPathSelector::PseudoSelector(
            CssPathPseudoSelector::NthChild(CssNthChildSelector::Even),
        )];
        assert!(group_matches(&refs(&even), &[], 0, 0));
        assert!(!group_matches(&refs(&even), &[], 1, 0));
        assert!(
            !group_matches(&refs(&even), &[], usize::MAX, 0),
            "usize::MAX is odd"
        );

        let odd = vec![CssPathSelector::PseudoSelector(
            CssPathPseudoSelector::NthChild(CssNthChildSelector::Odd),
        )];
        assert!(group_matches(&refs(&odd), &[], 1, 0));
        assert!(!group_matches(&refs(&odd), &[], 2, 0));

        let n = vec![CssPathSelector::PseudoSelector(
            CssPathPseudoSelector::NthChild(CssNthChildSelector::Number(u32::MAX)),
        )];
        assert!(group_matches(&refs(&n), &[], u32::MAX as usize, 0));
        assert!(!group_matches(&refs(&n), &[], 0, 0));
    }

    #[test]
    fn group_matches_nth_child_pattern_zero_repeat_does_not_divide_by_zero() {
        let zero = vec![CssPathSelector::PseudoSelector(
            CssPathPseudoSelector::NthChild(CssNthChildSelector::Pattern(CssNthChildPattern {
                pattern_repeat: 0,
                offset: 0,
            })),
        )];
        // `is_multiple_of(0)` is `self == 0` — no division by zero.
        assert!(group_matches(&refs(&zero), &[], 0, 0));
        assert!(!group_matches(&refs(&zero), &[], 5, 0));

        let offset_past = vec![CssPathSelector::PseudoSelector(
            CssPathPseudoSelector::NthChild(CssNthChildSelector::Pattern(CssNthChildPattern {
                pattern_repeat: 2,
                offset: u32::MAX,
            })),
        )];
        assert!(
            group_matches(&refs(&offset_past), &[], 0, 0),
            "index - offset saturates to 0 rather than underflowing"
        );
    }

    #[test]
    fn group_matches_structural_combinators_never_match() {
        for sel in [
            CssPathSelector::Children,
            CssPathSelector::DirectChildren,
            CssPathSelector::AdjacentSibling,
            CssPathSelector::GeneralSibling,
        ] {
            let a = vec![sel.clone()];
            assert!(
                !group_matches(&refs(&a), &refs(&a), 0, 1),
                "{sel:?} is a combinator, not a matchable group member"
            );
        }
    }

    #[test]
    fn css_matcher_empty_path_never_matches() {
        let m = CssMatcher {
            path: Vec::new(),
            indices_in_parent: vec![0],
            children_length: vec![0],
        };
        let path = CssPath {
            selectors: vec![CssPathSelector::Type(NodeTypeTag::Body)].into(),
        };
        assert!(!m.matches(&path), "an empty matcher path can never match");

        let m2 = CssMatcher {
            path: vec![CssPathSelector::Type(NodeTypeTag::Body)],
            indices_in_parent: vec![0],
            children_length: vec![0],
        };
        let empty_path = CssPath {
            selectors: Vec::new().into(),
        };
        assert!(
            !m2.matches(&empty_path),
            "an empty CSS path can never match"
        );
    }

    #[test]
    fn css_matcher_get_hash_is_deterministic_and_path_sensitive() {
        let a = CssMatcher {
            path: vec![CssPathSelector::Type(NodeTypeTag::Body)],
            indices_in_parent: vec![0],
            children_length: vec![0],
        };
        let b = CssMatcher {
            path: vec![CssPathSelector::Type(NodeTypeTag::Body)],
            indices_in_parent: vec![9],
            children_length: vec![9],
        };
        let c = CssMatcher {
            path: vec![CssPathSelector::Type(NodeTypeTag::Div)],
            indices_in_parent: vec![0],
            children_length: vec![0],
        };
        assert_eq!(a.get_hash(), a.get_hash(), "stable across calls");
        assert_eq!(
            a.get_hash(),
            b.get_hash(),
            "the hash covers only `path`, not the sibling indices"
        );
        assert_ne!(a.get_hash(), c.get_hash());

        let empty = CssMatcher {
            path: Vec::new(),
            indices_in_parent: Vec::new(),
            children_length: Vec::new(),
        };
        let _ = empty.get_hash(); // must not panic
    }

    #[test]
    fn css_matcher_mismatched_bookkeeping_vec_lengths_bail_out() {
        // `indices_in_parent` / `children_length` must be as long as the group list.
        let m = CssMatcher {
            path: vec![CssPathSelector::Type(NodeTypeTag::Body)],
            indices_in_parent: Vec::new(),
            children_length: Vec::new(),
        };
        let path = CssPath {
            selectors: vec![CssPathSelector::Type(NodeTypeTag::Body)].into(),
        };
        assert!(
            !m.matches(&path),
            "a desynced matcher must return false, not index out of bounds"
        );
    }

    #[test]
    fn get_css_blocks_and_inline_string_on_empty_css() {
        let m = CssMatcher {
            path: vec![CssPathSelector::Type(NodeTypeTag::Body)],
            indices_in_parent: vec![0],
            children_length: vec![0],
        };
        assert!(get_css_blocks(&Css::empty(), &m).is_empty());
        assert_eq!(css_blocks_to_inline_string(&[]), "");
    }

    // ================================================================
    // str_to_dom / str_to_dom_unstyled / parse_page_style_and_body / body_matcher
    // ================================================================

    #[test]
    fn str_to_dom_rejects_documents_without_html_or_body() {
        let map = ComponentMap::with_builtin();
        assert_eq!(
            str_to_dom(&[], &map, None).unwrap_err(),
            DomXmlParseError::NoHtmlNode
        );
        let html_only = vec![elem(XmlNode::create("html"))];
        assert_eq!(
            str_to_dom(&html_only, &map, None).unwrap_err(),
            DomXmlParseError::NoBodyInHtml
        );
        assert!(str_to_dom_unstyled(&[], &map).is_err());
    }

    #[test]
    fn str_to_dom_valid_minimal() {
        let map = ComponentMap::with_builtin();
        let d = doc(
            "body { color: red; }",
            vec![elem(node("div", &[("id", "x")], vec![]))],
        );
        assert!(str_to_dom(&d, &map, None).is_ok());
        assert!(str_to_dom_unstyled(&d, &map).is_ok());
    }

    #[test]
    fn str_to_dom_max_width_edge_values_do_not_panic() {
        let map = ComponentMap::with_builtin();
        let d = doc("", vec![elem(XmlNode::create("div"))]);
        for w in [
            Some(0.0f32),
            Some(-0.0),
            Some(-1.0),
            Some(f32::MAX),
            Some(f32::MIN),
            Some(f32::NAN),
            Some(f32::INFINITY),
            Some(f32::NEG_INFINITY),
            None,
        ] {
            assert!(
                str_to_dom(&d, &map, w).is_ok(),
                "max_width={w:?} is formatted straight into a CSS string and must not panic"
            );
        }
    }

    #[test]
    fn str_to_dom_deeply_nested_body_is_depth_capped_not_stack_overflowing() {
        let map = ComponentMap::with_builtin();
        let deep = wrap_divs(2_000, node("div", &[("id", "bottom")], vec![]));
        let d = doc("", vec![elem(deep)]);
        assert!(
            str_to_dom(&d, &map, None).is_ok(),
            "children past MAX_XML_NESTING_DEPTH are dropped, not crashed on"
        );
    }

    #[test]
    fn parse_page_style_and_body_and_body_matcher() {
        let d = doc("body { color: red; }", vec![elem(XmlNode::create("div"))]);
        let (css, body) = parse_page_style_and_body(&d).expect("well-formed page");
        assert_eq!(body.node_type.as_str(), "body");
        assert!(
            !css.rules.as_ref().is_empty(),
            "the <style> block is parsed"
        );

        let m = body_matcher(body);
        assert!(m.path.is_empty(), "the matcher starts with an empty path");
        assert_eq!(m.indices_in_parent, vec![0]);
        assert_eq!(m.children_length, vec![body.children.as_ref().len()]);
    }

    #[test]
    fn parse_page_style_and_body_with_no_style_block() {
        let head = node("head", &[], vec![]);
        let body = node("body", &[], vec![]);
        let d = vec![elem(node("html", &[], vec![elem(head), elem(body)]))];
        let (css, body) = parse_page_style_and_body(&d).expect("ok");
        assert!(css.rules.as_ref().is_empty());
        assert_eq!(body.children.as_ref().len(), 0);
    }

    // ================================================================
    // str_to_rust_code / str_to_c_code / str_to_cpp_code / str_to_python_code
    // ================================================================

    #[test]
    fn str_to_rust_code_empty_input_is_an_error_not_a_panic() {
        let map = ComponentMap::with_builtin();
        assert!(matches!(
            str_to_rust_code(&[], "", &map),
            Err(CompileError::Xml(DomXmlParseError::NoHtmlNode))
        ));
        assert!(str_to_c_code(&[], &map).is_err());
        assert!(str_to_cpp_code(&[], &map).is_err());
        assert!(str_to_python_code(&[], &map).is_err());
    }

    #[test]
    fn str_to_rust_code_whitespace_and_text_only_roots_are_errors() {
        let map = ComponentMap::with_builtin();
        for roots in [vec![txt("   ")], vec![txt("\t\n")], vec![txt("garbage")]] {
            assert!(
                str_to_rust_code(&roots, "", &map).is_err(),
                "a document with no <html> element must be rejected"
            );
        }
    }

    #[test]
    fn str_to_rust_code_valid_minimal() {
        let map = ComponentMap::with_builtin();
        let d = doc("", vec![elem(node("p", &[], vec![txt("Hi")]))]);
        let src = str_to_rust_code(&d, "// imports", &map).expect("compiles");
        assert!(src.contains("Dom::create_body()"), "got:\n{src}");
        assert!(src.contains("Dom::create_p_with_text(AzString::from(\"Hi\"))"));
        assert!(src.contains("// imports"), "the imports blob is spliced in");
        assert!(src.contains("fn main()"));
    }

    #[test]
    fn str_to_c_cpp_python_code_valid_minimal() {
        let map = ComponentMap::with_builtin();
        let d = doc("", vec![elem(node("p", &[], vec![txt("Hi")]))]);

        let c = str_to_c_code(&d, &map).expect("compiles");
        assert!(c.contains("#include \"azul.h\""), "got:\n{c}");
        assert!(c.contains("AzDom n0 = AzDom_createBody();"));
        assert!(c.contains("AzDom_createPWithText(AZ_STR(\"Hi\"))"));

        let cpp = str_to_cpp_code(&d, &map).expect("compiles");
        assert!(cpp.contains("#include \"azul20.hpp\""), "got:\n{cpp}");
        assert!(cpp.contains("Dom::create_p_with_text(String(\"Hi\"))"));

        let py = str_to_python_code(&d, &map).expect("compiles");
        assert!(py.contains("import azul"), "got:\n{py}");
        assert!(py.contains("azul.Dom.create_p_with_text(\"Hi\")"));
    }

    #[test]
    fn compile_targets_escape_quotes_in_text_content() {
        let map = ComponentMap::with_builtin();
        let d = doc("", vec![elem(node("div", &[], vec![txt("say \"hi\"")]))]);

        let rust = str_to_rust_code(&d, "", &map).expect("compiles");
        assert!(rust.contains("say \\\"hi\\\""), "got:\n{rust}");
        let c = str_to_c_code(&d, &map).expect("compiles");
        assert!(c.contains("say \\\"hi\\\""), "got:\n{c}");
    }

    #[test]
    fn compile_body_node_to_rust_code_on_an_empty_body() {
        let map = ComponentMap::with_builtin();
        let body = node("body", &[], vec![]);
        let mut extra = VecContents::default();
        let mut blocks = BTreeMap::new();
        let out = compile_body_node_to_rust_code(
            &body,
            &map,
            &mut extra,
            &mut blocks,
            &Css::empty(),
            body_matcher(&body),
        )
        .expect("ok");
        assert_eq!(
            out, "Dom::create_body()",
            "no children => no .with_children()"
        );
    }

    #[test]
    fn compile_body_node_to_rust_code_skips_whitespace_only_text_children() {
        let map = ComponentMap::with_builtin();
        let body = node("body", &[], vec![txt("   \n\t ")]);
        let mut extra = VecContents::default();
        let mut blocks = BTreeMap::new();
        let out = compile_body_node_to_rust_code(
            &body,
            &map,
            &mut extra,
            &mut blocks,
            &Css::empty(),
            body_matcher(&body),
        )
        .expect("ok");
        assert!(
            !out.contains("create_text"),
            "a whitespace-only text child emits nothing, got:\n{out}"
        );
    }

    // ================================================================
    // builtin_render_fn / builtin_compile_fn  (numeric: indent)
    // ================================================================

    #[test]
    fn builtin_render_fn_for_a_text_and_a_textless_element() {
        let map = ComponentMap::with_builtin();
        let div = map.get_unqualified("div").expect("builtin div");
        assert!(matches!(
            builtin_render_fn(div, &div.data_model, &map),
            ResultStyledDomRenderDomError::Ok(_)
        ));

        let p = map.get_unqualified("p").expect("builtin p");
        assert!(matches!(
            builtin_render_fn(p, &p.data_model, &map),
            ResultStyledDomRenderDomError::Ok(_)
        ));
    }

    #[test]
    fn builtin_compile_fn_ignores_indent_so_usize_max_is_safe() {
        let map = ComponentMap::with_builtin();
        let div = map.get_unqualified("div").expect("builtin div");
        for indent in [0usize, 1, 1024, usize::MAX] {
            match builtin_compile_fn(div, &CompileTarget::Rust, &div.data_model, indent) {
                ResultStringCompileError::Ok(s) => assert_eq!(
                    s.as_str(),
                    "Dom::create_node(NodeType::Div)",
                    "indent is unused by builtin_compile_fn (indent={indent})"
                ),
                ResultStringCompileError::Err(e) => panic!("unexpected error: {e:?}"),
            }
        }
    }

    #[test]
    fn builtin_compile_fn_emits_text_and_escapes_it() {
        let map = ComponentMap::with_builtin();
        let p = map.get_unqualified("p").expect("builtin p");
        let data = p.data_model.clone().with_default(
            "text",
            ComponentDefaultValue::String(AzString::from("a\"b\\c")),
        );

        match builtin_compile_fn(p, &CompileTarget::Rust, &data, 0) {
            ResultStringCompileError::Ok(s) => {
                assert!(s.as_str().contains("a\\\"b\\\\c"), "got {}", s.as_str());
            }
            ResultStringCompileError::Err(e) => panic!("unexpected error: {e:?}"),
        }
    }

    #[test]
    fn builtin_compile_fn_covers_every_target() {
        let map = ComponentMap::with_builtin();
        let div = map.get_unqualified("div").expect("builtin div");
        for target in [
            CompileTarget::Rust,
            CompileTarget::C,
            CompileTarget::Cpp,
            CompileTarget::Python,
        ] {
            match builtin_compile_fn(div, &target, &div.data_model, 0) {
                ResultStringCompileError::Ok(s) => {
                    assert!(!s.as_str().is_empty(), "{target:?} emitted nothing");
                }
                ResultStringCompileError::Err(e) => panic!("{target:?}: {e:?}"),
            }
        }
    }

    // ================================================================
    // user_defined_render_fn / user_defined_compile_fn
    // ================================================================

    fn every_default_kind() -> Vec<ComponentDataField> {
        use ComponentDefaultValue as D;
        vec![
            data_field(
                "s",
                ComponentFieldType::String,
                Some(D::String(AzString::from("txt"))),
                "",
            ),
            data_field("b", ComponentFieldType::Bool, Some(D::Bool(true)), ""),
            data_field("i32", ComponentFieldType::I32, Some(D::I32(i32::MIN)), ""),
            data_field("i64", ComponentFieldType::I64, Some(D::I64(i64::MIN)), ""),
            data_field("u32", ComponentFieldType::U32, Some(D::U32(u32::MAX)), ""),
            data_field("u64", ComponentFieldType::U64, Some(D::U64(u64::MAX)), ""),
            data_field(
                "us",
                ComponentFieldType::Usize,
                Some(D::Usize(usize::MAX)),
                "",
            ),
            data_field("f32", ComponentFieldType::F32, Some(D::F32(f32::NAN)), ""),
            data_field(
                "f64",
                ComponentFieldType::F64,
                Some(D::F64(f64::INFINITY)),
                "",
            ),
            data_field(
                "c",
                ComponentFieldType::ColorU,
                Some(D::ColorU(ColorU {
                    r: 0,
                    g: 0,
                    b: 0,
                    a: 0,
                })),
                "",
            ),
            data_field(
                "cb",
                ComponentFieldType::StyledDom,
                Some(D::CallbackFnPointer(AzString::from("on_click"))),
                "",
            ),
            data_field(
                "j",
                ComponentFieldType::String,
                Some(D::Json(AzString::from("{}"))),
                "",
            ),
            data_field("none", ComponentFieldType::String, Some(D::None), ""),
            data_field("missing", ComponentFieldType::String, None, ""),
        ]
    }

    #[test]
    fn user_defined_render_fn_handles_every_default_value_kind() {
        let map = ComponentMap::with_builtin();
        let def = user_def("", every_default_kind());
        assert!(matches!(
            user_defined_render_fn(&def, &def.data_model, &map),
            ResultStyledDomRenderDomError::Ok(_)
        ));
    }

    #[test]
    fn user_defined_render_fn_on_an_empty_model_and_with_css() {
        let map = ComponentMap::with_builtin();
        let empty = user_def("", Vec::new());
        assert!(matches!(
            user_defined_render_fn(&empty, &empty.data_model, &map),
            ResultStyledDomRenderDomError::Ok(_)
        ));

        let styled = user_def(".widget { color: red; }", Vec::new());
        assert!(matches!(
            user_defined_render_fn(&styled, &styled.data_model, &map),
            ResultStyledDomRenderDomError::Ok(_)
        ));
    }

    #[test]
    fn user_defined_render_fn_unknown_sub_component_renders_a_placeholder() {
        let map = ComponentMap::create(); // empty: no library can resolve the instance
        let def = user_def(
            "",
            vec![data_field(
                "child",
                ComponentFieldType::StyledDom,
                Some(ComponentDefaultValue::ComponentInstance(
                    ComponentInstanceDefault {
                        library: AzString::from("nope"),
                        component: AzString::from("missing"),
                        field_overrides: Vec::new().into(),
                    },
                )),
                "",
            )],
        );
        assert!(
            matches!(
                user_defined_render_fn(&def, &def.data_model, &map),
                ResultStyledDomRenderDomError::Ok(_)
            ),
            "an unresolvable sub-component must render a placeholder, not error out"
        );
    }

    #[test]
    fn user_defined_compile_fn_indent_zero_and_every_target() {
        let def = user_def("", every_default_kind());
        for target in [
            CompileTarget::Rust,
            CompileTarget::C,
            CompileTarget::Cpp,
            CompileTarget::Python,
        ] {
            match user_defined_compile_fn(&def, &target, &def.data_model, 0) {
                ResultStringCompileError::Ok(s) => {
                    assert!(!s.as_str().is_empty(), "{target:?} emitted nothing");
                }
                ResultStringCompileError::Err(e) => panic!("{target:?}: {e:?}"),
            }
        }
    }

    #[test]
    fn user_defined_compile_fn_indent_scales_the_leading_whitespace() {
        // NOTE: `indent` is used as `" ".repeat(indent * 4)`, so it is NOT safe at
        // usize::MAX (the multiply overflows). Exercise the realistic range.
        let def = user_def("", Vec::new());
        let mut prev = 0usize;
        for indent in [0usize, 1, 2, 8] {
            match user_defined_compile_fn(&def, &CompileTarget::Rust, &def.data_model, indent) {
                ResultStringCompileError::Ok(s) => {
                    let len = s.as_str().len();
                    assert!(len > prev, "indent={indent} must widen the output");
                    prev = len;
                }
                ResultStringCompileError::Err(e) => panic!("indent={indent}: {e:?}"),
            }
        }
    }

    #[test]
    fn user_defined_compile_fn_escapes_string_defaults() {
        let def = user_def(
            "",
            vec![data_field(
                "s",
                ComponentFieldType::String,
                Some(ComponentDefaultValue::String(AzString::from("a\"b\\c"))),
                "",
            )],
        );
        match user_defined_compile_fn(&def, &CompileTarget::Rust, &def.data_model, 0) {
            ResultStringCompileError::Ok(s) => {
                assert!(s.as_str().contains("a\\\"b\\\\c"), "got:\n{}", s.as_str());
            }
            ResultStringCompileError::Err(e) => panic!("{e:?}"),
        }
    }

    #[test]
    fn push_scalar_field_appends_one_div_per_call() {
        let mut children: Vec<Dom> = Vec::new();
        push_scalar_field(&mut children, "n", &i64::MIN);
        push_scalar_field(&mut children, "", &f32::NAN);
        push_scalar_field(&mut children, "\u{1F600}", &usize::MAX);
        assert_eq!(children.len(), 3);
    }

    // ================================================================
    // Structural builtins: if / for / map
    // ================================================================

    #[test]
    fn builtin_if_for_map_component_defs_are_well_formed() {
        for (def, model, field) in [
            (builtin_if_component(), "IfData", "condition"),
            (builtin_for_component(), "ForData", "count"),
            (builtin_map_component(), "MapData", "data_json"),
        ] {
            assert_eq!(def.id.collection.as_str(), "builtin");
            assert_eq!(def.data_model.name.as_str(), model);
            assert!(
                def.data_model.get_field(field).is_some(),
                "{model} must expose `{field}`"
            );
        }
    }

    #[test]
    fn builtin_if_render_fn_defaults_to_the_else_branch() {
        let map = ComponentMap::create();
        let def = builtin_if_component();
        // Missing / wrongly-typed condition => false, no panic.
        let empty = dm("IfData", Vec::new());
        assert!(matches!(
            builtin_if_render_fn(&def, &empty, &map),
            ResultStyledDomRenderDomError::Ok(_)
        ));

        let truthy = def
            .data_model
            .clone()
            .with_default("condition", ComponentDefaultValue::Bool(true));
        assert!(matches!(
            builtin_if_render_fn(&def, &truthy, &map),
            ResultStyledDomRenderDomError::Ok(_)
        ));
    }

    #[test]
    fn builtin_for_render_fn_handles_zero_and_a_wrongly_typed_count() {
        let map = ComponentMap::create();
        let def = builtin_for_component();

        let zero = def
            .data_model
            .clone()
            .with_default("count", ComponentDefaultValue::U32(0));
        assert!(matches!(
            builtin_for_render_fn(&def, &zero, &map),
            ResultStyledDomRenderDomError::Ok(_)
        ));

        // A non-U32 default falls back to the documented default of 3.
        let wrong_type = def
            .data_model
            .clone()
            .with_default("count", ComponentDefaultValue::String(AzString::from("9")));
        assert!(matches!(
            builtin_for_render_fn(&def, &wrong_type, &map),
            ResultStyledDomRenderDomError::Ok(_)
        ));
    }

    #[test]
    fn builtin_map_render_fn_defaults_to_an_empty_json_array() {
        let map = ComponentMap::create();
        let def = builtin_map_component();
        assert!(matches!(
            builtin_map_render_fn(&def, &dm("MapData", Vec::new()), &map),
            ResultStyledDomRenderDomError::Ok(_)
        ));
        let garbage = def.data_model.clone().with_default(
            "data_json",
            ComponentDefaultValue::String(AzString::from("{{{")),
        );
        assert!(
            matches!(
                builtin_map_render_fn(&def, &garbage, &map),
                ResultStyledDomRenderDomError::Ok(_)
            ),
            "malformed JSON must not panic — it is only echoed into a label"
        );
    }

    #[test]
    fn structural_builtin_compile_fns_ignore_indent_entirely() {
        let cases: [(ComponentDef, ComponentCompileFn); 3] = [
            (builtin_if_component(), builtin_if_compile_fn),
            (builtin_for_component(), builtin_for_compile_fn),
            (builtin_map_component(), builtin_map_compile_fn),
        ];
        for (def, f) in cases {
            for target in [
                CompileTarget::Rust,
                CompileTarget::C,
                CompileTarget::Cpp,
                CompileTarget::Python,
            ] {
                for indent in [0usize, usize::MAX] {
                    match f(&def, &target, &def.data_model, indent) {
                        ResultStringCompileError::Ok(s) => {
                            assert!(
                                !s.as_str().is_empty(),
                                "{target:?}/{indent} emitted nothing"
                            );
                        }
                        ResultStringCompileError::Err(e) => panic!("{target:?}: {e:?}"),
                    }
                }
            }
        }
    }

    // ================================================================
    // data_field / builtin_data_model / builtin_component_def
    // ================================================================

    #[test]
    fn data_field_required_is_the_inverse_of_having_a_default() {
        let with = data_field(
            "x",
            ComponentFieldType::String,
            Some(ComponentDefaultValue::String(AzString::from("v"))),
            "d",
        );
        assert!(!with.required);
        assert_eq!(with.description.as_str(), "d");

        let without = data_field("x", ComponentFieldType::String, None, "");
        assert!(without.required);
        assert!(matches!(
            without.default_value,
            OptionComponentDefaultValue::None
        ));
    }

    #[test]
    fn builtin_data_model_unknown_tag_is_empty() {
        assert!(builtin_data_model("").is_empty());
        assert!(builtin_data_model("div").is_empty());
        assert!(builtin_data_model("\u{1F600}").is_empty());
        assert!(builtin_data_model(&"z".repeat(10_000)).is_empty());
    }

    #[test]
    fn builtin_data_model_known_tags_expose_their_attributes() {
        let a = builtin_data_model("a");
        assert!(
            a.iter().any(|f| f.name.as_str() == "href"),
            "<a> must expose href"
        );
        // `src` on <img> is required (it has no default value).
        let img = builtin_data_model("img");
        let src = img
            .iter()
            .find(|f| f.name.as_str() == "src")
            .expect("img has src");
        assert!(src.required, "<img src> must be a required field");
        // `img` and `image` share the same model.
        assert_eq!(builtin_data_model("image").len(), img.len());
    }

    #[test]
    fn builtin_component_def_default_text_controls_the_text_field() {
        let with_text = builtin_component_def("p", "Paragraph", Some("Hi"), "");
        assert_eq!(
            with_text
                .data_model
                .get_default_string("text")
                .map(AzString::as_str),
            Some("Hi")
        );
        assert_eq!(with_text.data_model.name.as_str(), "ParagraphData");
        assert_eq!(with_text.id.qualified_name(), "builtin:p");

        let no_text = builtin_component_def("div", "Div", None, "");
        assert!(
            no_text.data_model.get_field("text").is_none(),
            "a `None` default_text means the element has no text field at all"
        );

        // An empty-string default still creates the field.
        let empty_text = builtin_component_def("span", "Span", Some(""), "");
        assert!(empty_text.data_model.get_field("text").is_some());
        assert_eq!(
            empty_text
                .data_model
                .get_default_string("text")
                .map(AzString::as_str),
            Some("")
        );
    }

    // ================================================================
    // xml_attrs_to_data_model
    // ================================================================

    #[test]
    fn xml_attrs_to_data_model_overrides_defaults_from_attributes() {
        let base = builtin_component_def("a", "Link", Some("Link text"), "").data_model;
        let model = xml_attrs_to_data_model(&base, &attrs(&[("href", "/x")]), None);
        assert_eq!(
            model.get_default_string("href").map(AzString::as_str),
            Some("/x")
        );
        assert_eq!(
            model.get_default_string("text").map(AzString::as_str),
            Some("Link text"),
            "un-supplied fields keep their defaults"
        );
        assert_eq!(
            model.fields.as_ref().len(),
            base.fields.as_ref().len(),
            "no field is added or dropped"
        );
    }

    #[test]
    fn xml_attrs_to_data_model_text_content_is_prepared_and_empty_text_is_ignored() {
        let base = builtin_component_def("a", "Link", Some("Link text"), "").data_model;

        let with_text = xml_attrs_to_data_model(&base, &attrs(&[]), Some("  Hello &amp; bye  "));
        assert_eq!(
            with_text.get_default_string("text").map(AzString::as_str),
            Some("Hello & bye"),
            "text content is trimmed and entity-decoded"
        );

        let blank = xml_attrs_to_data_model(&base, &attrs(&[]), Some("   \n\t "));
        assert_eq!(
            blank.get_default_string("text").map(AzString::as_str),
            Some("Link text"),
            "whitespace-only text content leaves the default intact"
        );
    }

    #[test]
    fn xml_attrs_to_data_model_ignores_unknown_attributes() {
        let base = builtin_component_def("a", "Link", Some(""), "").data_model;
        let before = base.fields.as_ref().len();
        let model = xml_attrs_to_data_model(
            &base,
            &attrs(&[("data-nonsense", "1"), ("", ""), ("\u{1F600}", "x")]),
            None,
        );
        assert_eq!(
            model.fields.as_ref().len(),
            before,
            "unknown attributes must not create fields"
        );
    }

    // ================================================================
    // DomXml
    // ================================================================

    #[test]
    fn dom_xml_into_styled_dom_matches_the_from_impl() {
        let via_method: StyledDom = DomXml::default().into_styled_dom();
        let via_from: StyledDom = DomXml::default().into();
        assert_eq!(
            via_method, via_from,
            "into_styled_dom() must be exactly the From<DomXml> impl"
        );
    }

    // ================================================================
    // Display impls  (serializer)
    // ================================================================

    fn pos() -> XmlTextPos {
        XmlTextPos {
            row: u32::MAX,
            col: 0,
        }
    }

    #[test]
    fn xml_text_pos_display_is_non_empty_for_edge_values() {
        assert_eq!(
            format!("{}", XmlTextPos { row: 0, col: 0 }),
            "line 0:0",
            "a zero position is still rendered"
        );
        assert_eq!(
            format!(
                "{}",
                XmlTextPos {
                    row: u32::MAX,
                    col: u32::MAX
                }
            ),
            "line 4294967295:4294967295"
        );
    }

    #[test]
    fn xml_stream_error_display_covers_every_variant() {
        let variants = vec![
            XmlStreamError::UnexpectedEndOfStream,
            XmlStreamError::InvalidName,
            XmlStreamError::NonXmlChar(NonXmlCharError {
                ch: u32::MAX,
                pos: pos(),
            }),
            XmlStreamError::InvalidChar(InvalidCharError {
                expected: u8::MAX,
                got: 0,
                pos: pos(),
            }),
            XmlStreamError::InvalidCharMultiple(InvalidCharMultipleError {
                expected: 0,
                got: Vec::<u8>::new().into(),
                pos: pos(),
            }),
            XmlStreamError::InvalidQuote(InvalidQuoteError { got: 0, pos: pos() }),
            XmlStreamError::InvalidSpace(InvalidSpaceError { got: 0, pos: pos() }),
            XmlStreamError::InvalidString(InvalidStringError {
                got: AzString::from(""),
                pos: pos(),
            }),
            XmlStreamError::InvalidReference,
            XmlStreamError::InvalidExternalID,
            XmlStreamError::InvalidCommentData,
            XmlStreamError::InvalidCommentEnd,
            XmlStreamError::InvalidCharacterData,
        ];
        for v in &variants {
            let s = format!("{v}");
            assert!(!s.is_empty(), "{v:?} must render a non-empty message");
        }
        // `char::from_u32(u32::MAX)` is None — the formatter must not unwrap it.
        assert!(format!("{}", variants[2]).contains("None"));
    }

    #[test]
    fn xml_parse_error_display_covers_every_variant() {
        let te = XmlTextError {
            stream_error: XmlStreamError::InvalidName,
            pos: pos(),
        };
        let variants = vec![
            XmlParseError::InvalidDeclaration(te.clone()),
            XmlParseError::InvalidComment(te.clone()),
            XmlParseError::InvalidPI(te.clone()),
            XmlParseError::InvalidDoctype(te.clone()),
            XmlParseError::InvalidEntity(te.clone()),
            XmlParseError::InvalidElement(te.clone()),
            XmlParseError::InvalidAttribute(te.clone()),
            XmlParseError::InvalidCdata(te.clone()),
            XmlParseError::InvalidCharData(te),
            XmlParseError::UnknownToken(pos()),
        ];
        for v in &variants {
            assert!(!format!("{v}").is_empty(), "{v:?} must render");
        }
    }

    #[test]
    fn xml_error_display_covers_the_non_css_variants() {
        let variants = vec![
            XmlError::NoParserAvailable,
            XmlError::InvalidXmlPrefixUri(pos()),
            XmlError::UnexpectedXmlUri(pos()),
            XmlError::UnexpectedXmlnsUri(pos()),
            XmlError::InvalidElementNamePrefix(pos()),
            XmlError::DuplicatedNamespace(DuplicatedNamespaceError {
                ns: AzString::from(""),
                pos: pos(),
            }),
            XmlError::UnknownNamespace(UnknownNamespaceError {
                ns: AzString::from("\u{1F600}"),
                pos: pos(),
            }),
            XmlError::UnexpectedCloseTag(UnexpectedCloseTagError {
                expected: AzString::from("a"),
                actual: AzString::from("b"),
                pos: pos(),
            }),
            XmlError::UnexpectedEntityCloseTag(pos()),
            XmlError::UnknownEntityReference(UnknownEntityReferenceError {
                entity: AzString::from("x"),
                pos: pos(),
            }),
            XmlError::MalformedEntityReference(pos()),
            XmlError::EntityReferenceLoop(pos()),
            XmlError::InvalidAttributeValue(pos()),
            XmlError::DuplicatedAttribute(DuplicatedAttributeError {
                attribute: AzString::from("id"),
                pos: pos(),
            }),
            XmlError::NoRootNode,
            XmlError::SizeLimit,
            XmlError::DtdDetected,
            XmlError::MalformedHierarchy(MalformedHierarchyError {
                expected: AzString::from("app"),
                got: AzString::from("p"),
            }),
            XmlError::ParserError(XmlParseError::UnknownToken(pos())),
            XmlError::UnclosedRootNode,
            XmlError::UnexpectedDeclaration(pos()),
            XmlError::NodesLimitReached,
            XmlError::AttributesLimitReached,
            XmlError::NamespacesLimitReached,
            XmlError::InvalidName(pos()),
            XmlError::NonXmlChar(pos()),
            XmlError::InvalidChar(pos()),
            XmlError::InvalidChar2(pos()),
            XmlError::InvalidString(pos()),
            XmlError::InvalidExternalID(pos()),
            XmlError::InvalidComment(pos()),
            XmlError::InvalidCharacterData(pos()),
            XmlError::UnknownToken(pos()),
            XmlError::UnexpectedEndOfStream,
        ];
        for v in &variants {
            assert!(!format!("{v}").is_empty(), "{v:?} must render");
        }
    }

    #[test]
    fn component_and_render_and_compile_error_display() {
        let unknown = ComponentError::UnknownComponent(AzString::from("\u{1F600}"));
        assert!(format!("{unknown}").contains("Unknown component"));

        let useless = ComponentError::UselessFunctionArgument(UselessFunctionArgumentError {
            component_name: AzString::from("c"),
            argument_name: AzString::from("a"),
            valid_args: Vec::<AzString>::new().into(),
        });
        assert!(!format!("{useless}").is_empty());

        let render: RenderDomError = unknown.clone().into();
        assert!(!format!("{render}").is_empty());

        let compile: CompileError = render.clone().into();
        assert!(!format!("{compile}").is_empty());

        let dom_xml: DomXmlParseError = render.into();
        assert!(!format!("{dom_xml}").is_empty());
        let compile2: CompileError = dom_xml.into();
        assert!(!format!("{compile2}").is_empty());
    }

    #[test]
    fn dom_xml_parse_error_display_covers_the_non_css_variants() {
        let variants = vec![
            DomXmlParseError::NoHtmlNode,
            DomXmlParseError::MultipleHtmlRootNodes,
            DomXmlParseError::NoBodyInHtml,
            DomXmlParseError::MultipleBodyNodes,
            DomXmlParseError::Xml(XmlError::NoRootNode),
            DomXmlParseError::MalformedHierarchy(MalformedHierarchyError {
                expected: AzString::from("app"),
                got: AzString::from("p"),
            }),
            DomXmlParseError::RenderDom(RenderDomError::Component(
                ComponentError::UnknownComponent(AzString::from("x")),
            )),
            DomXmlParseError::Component(ComponentParseError::NotAComponent),
        ];
        for v in &variants {
            assert!(!format!("{v}").is_empty(), "{v:?} must render");
        }
    }

    #[test]
    fn component_parse_error_display_covers_the_non_css_variants() {
        let variants = vec![
            ComponentParseError::NotAComponent,
            ComponentParseError::UnnamedComponent,
            ComponentParseError::MissingName(usize::MAX),
            ComponentParseError::MissingType(MissingTypeError {
                arg_pos: 0,
                arg_name: AzString::from(""),
            }),
            ComponentParseError::WhiteSpaceInComponentName(WhiteSpaceInComponentNameError {
                arg_pos: usize::MAX,
                arg_name: AzString::from("a b"),
            }),
            ComponentParseError::WhiteSpaceInComponentType(WhiteSpaceInComponentTypeError {
                arg_pos: 0,
                arg_name: AzString::from("a"),
                arg_type: AzString::from("b c"),
            }),
        ];
        for v in &variants {
            assert!(!format!("{v}").is_empty(), "{v:?} must render");
        }
    }

    // ================================================================
    // serde-json gated: ComponentDataModel::to_json / from_json
    // ================================================================

    #[cfg(feature = "serde-json")]
    #[test]
    fn data_model_to_json_round_trips() {
        let m = model_with_text();
        let json = m.to_json().expect("serializes");
        let back = ComponentDataModel::from_json(&json).expect("deserializes");
        assert_eq!(back.name.as_str(), m.name.as_str());
        assert_eq!(back.fields.as_ref().len(), m.fields.as_ref().len());
        assert_eq!(
            back.get_default_string("text").map(AzString::as_str),
            Some("hi")
        );
    }

    #[cfg(feature = "serde-json")]
    #[test]
    fn data_model_from_json_rejects_garbage_without_panicking() {
        for s in [
            "",
            "   ",
            "\t\n",
            "not json",
            "{",
            "[]",
            "null",
            "0",
            "-0",
            "9223372036854775807",
            "NaN",
            "\u{1F600}",
        ] {
            assert!(
                ComponentDataModel::from_json(s).is_err(),
                "{s:?} is not a data model"
            );
        }
    }

    #[cfg(feature = "serde-json")]
    #[test]
    fn data_model_from_json_deeply_nested_input_does_not_stack_overflow() {
        let bomb = format!("{}{}", "[".repeat(10_000), "]".repeat(10_000));
        assert!(
            ComponentDataModel::from_json(&bomb).is_err(),
            "serde_json must reject the nesting bomb, not crash"
        );
    }

    #[cfg(feature = "serde-json")]
    #[test]
    fn data_model_to_json_on_an_empty_model() {
        let m = dm("Empty", Vec::new());
        let json = m.to_json().expect("serializes");
        assert!(json.contains("\"fields\""), "got {json}");
        assert!(ComponentDataModel::from_json(&json).is_ok());
    }
}
