#[allow(unused_imports)]
pub use super::*;
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
        use crate::{
            resources::DecodedImage,
            window::{AzStringPair, StringPairVec},
        };

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
    ///
    /// Runs on a thread with an 8 MiB stack ON PURPOSE. libtest gives each test
    /// the platform default (2 MiB on Linux), which is SMALLER than any context
    /// this code actually runs in - a real app's main thread gets 8 MiB. The cap
    /// is 512, and a debug-profile frame of `xml_node_to_dom_fast` is large
    /// enough that 512 of them clear 2 MiB, so on the dev-profile CI job this
    /// test used to abort with "has overflowed its stack" while the product was
    /// fine. Sizing the thread like the real caller tests the cap instead of
    /// testing libtest's default. (The sibling
    /// `scan_external_resources_deep_nesting_ok` recurses the same depth with a
    /// much smaller frame and needs no such help.)
    #[test]
    fn xml_node_to_dom_fast_deep_nesting_ok() {
        std::thread::Builder::new()
            .stack_size(8 * 1024 * 1024)
            .spawn(|| {
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
            })
            .expect("spawn deep-nesting probe")
            .join()
            .expect("deep DOM build must not overflow the stack");
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
            codegen: ComponentCodegen::RenderFunction,
            render_fn_source: None.into(),
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
    fn a_fragment_gets_a_synthesised_html_root() {
        // BROWSER-LIKE (user ruling): a document with no `<html>` root gets
        // one, instead of parsing into the TEXT "No <html> node found as the
        // root of the file" - which then laid out and painted like any other
        // text, so a caller measuring pixels saw an error message it never
        // asked for and no hint of what went wrong.
        let body_of = |roots: &[XmlNodeChild]| {
            let html = get_html_node(roots).expect("a root is synthesised");
            assert_eq!(html.node_type.as_str(), "html");
            html.children
                .as_ref()
                .iter()
                .find_map(|c| match c {
                    XmlNodeChild::Element(e) if e.node_type.as_str() == "body" => Some(e.clone()),
                    _ => None,
                })
                .expect("with a body")
        };

        assert_eq!(body_of(&[]).children.as_ref().len(), 0, "an empty document");
        assert_eq!(body_of(&[txt("just text")]).children.as_ref().len(), 1);
        assert_eq!(
            body_of(&[elem(XmlNode::create("div"))])
                .children
                .as_ref()
                .len(),
            1,
            "a bare fragment becomes the body's content"
        );
        // An `<svg>` root is a fragment like any other.
        assert_eq!(
            body_of(&[elem(XmlNode::create("svg"))])
                .children
                .as_ref()
                .len(),
            1
        );
    }

    /// Metadata is hoisted into a `<head>`, content into the `<body>` - the
    /// same split a browser makes. A `<style>` left in the body is not read by
    /// `str_to_dom_unstyled` (it looks for `<head><style>`), so a fragment that
    /// brought its own stylesheet would render unstyled with no hint why.
    #[test]
    fn a_synthesised_root_hoists_metadata_into_the_head() {
        let roots = vec![
            elem(XmlNode::create("style")),
            elem(XmlNode::create("div")),
            elem(XmlNode::create("title")),
        ];
        let html = get_html_node(&roots).expect("a root is synthesised");
        let child_tags: Vec<String> = html
            .children
            .as_ref()
            .iter()
            .filter_map(|c| match c {
                XmlNodeChild::Element(e) => Some(e.node_type.as_str().to_string()),
                XmlNodeChild::Text(_) => None,
            })
            .collect();
        assert_eq!(child_tags, vec!["head".to_string(), "body".to_string()]);

        let count = |tag: &str| {
            html.children
                .as_ref()
                .iter()
                .find_map(|c| match c {
                    XmlNodeChild::Element(e) if e.node_type.as_str() == tag => {
                        Some(e.children.as_ref().len())
                    }
                    _ => None,
                })
                .unwrap_or(0)
        };
        assert_eq!(count("head"), 2, "<style> and <title>");
        assert_eq!(count("body"), 1, "the <div>");
    }

    /// A root-level `<body>` is ADOPTED, not nested: wrapping it in a fresh
    /// `<body>` would give the document two, which is its own error.
    #[test]
    fn a_root_level_body_is_adopted_rather_than_nested() {
        let roots = vec![elem(XmlNode::create("body"))];
        let html = get_html_node(&roots).expect("a root is synthesised");
        let bodies = html
            .children
            .as_ref()
            .iter()
            .filter(|c| matches!(c, XmlNodeChild::Element(e) if e.node_type.as_str() == "body"))
            .count();
        assert_eq!(bodies, 1);
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
    // the one decoder (`html::decode_character_references`, XML rules)
    // - what `prepare_string` decodes with (DEDUP_WIDGETS_API F31)
    // ================================================================

    fn xml_decode(s: &str) -> String {
        html::decode_character_references(s, html::CharRefMode::Xml).into_owned()
    }

    #[test]
    fn a_numeric_reference_decodes_to_its_character() {
        assert_eq!(xml_decode("&#65;"), "A");
        assert_eq!(xml_decode("&#x41;"), "A");
        assert_eq!(xml_decode("&#X41;"), "A", "uppercase X accepted");
        assert_eq!(xml_decode("&#x1F600;"), "\u{1F600}");
    }

    #[test]
    fn a_malformed_numeric_reference_stays_as_written() {
        for s in ["&#;", "&#x;", "&#zz;", "&# 65;", "&#65junk;", "&#65"] {
            assert_eq!(xml_decode(s), s);
        }
    }

    #[test]
    fn a_numeric_reference_outside_the_scalar_values_stays_as_written() {
        assert_eq!(xml_decode("&#0;"), "\u{0}", "NUL is a valid char");
        assert_eq!(xml_decode("&#x10FFFF;"), "\u{10FFFF}", "max scalar");
        for s in [
            "&#x110000;",       // one past the max scalar value
            "&#xD800;",         // a lone surrogate is not a char
            "&#4294967295;",    // u32::MAX is not a scalar value
            "&#4294967296;",    // one past u32::MAX must not wrap
            "&#-1;",            // a sign is not a digit
            "&#xFFFFFFFFFFFF;", // hex overflow is rejected, not truncated
        ] {
            assert_eq!(xml_decode(s), s);
        }
    }

    #[test]
    fn an_extremely_long_numeric_reference_terminates() {
        let s = format!("&#{};", "9".repeat(LONG));
        assert_eq!(xml_decode(&s), s, "overflows u32: stays as written");
    }

    #[test]
    fn an_unknown_or_empty_reference_stays_as_written() {
        assert_eq!(xml_decode(""), "");
        assert_eq!(xml_decode("&"), "&", "a bare '&' at EOF must not panic");
        assert_eq!(xml_decode("&;"), "&;", "empty reference body");
        assert_eq!(xml_decode("&bogus;"), "&bogus;");
        assert_eq!(
            xml_decode("&averyveryverylongbody;"),
            "&averyveryverylongbody;"
        );
    }

    #[test]
    fn decoding_is_one_pass_so_an_ampersand_never_reopens_a_reference() {
        assert_eq!(
            xml_decode("&amp;lt;"),
            "&lt;",
            "&amp; must not re-open an entity"
        );
        assert_eq!(xml_decode("&lt;&gt;&amp;&quot;&apos;"), "<>&\"'");
    }

    #[test]
    fn decoding_keeps_unicode_and_terminates_on_long_input() {
        assert_eq!(
            xml_decode("\u{1F600}\u{0301}\u{130}"),
            "\u{1F600}\u{0301}\u{130}"
        );
        let amps = "&".repeat(20_000);
        assert_eq!(xml_decode(&amps).len(), 20_000);
    }

    /// `&nbsp;` is a space `prepare_string`'s trimming keeps: decoded to
    /// U+00A0 before the per-line trim, it would be trimmed away (U+00A0 is
    /// white space to `str::trim`).
    #[test]
    fn prepare_string_turns_nbsp_into_a_space_the_trim_keeps() {
        assert_eq!(prepare_string("a&nbsp;b"), "a b");
        assert_eq!(prepare_string("&nbsp;x&nbsp;"), " x ");
        assert_eq!(prepare_string("&nbsp;&amp;&nbsp;"), " & ");
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

    /// Every text node in `dom`, in document order.
    fn all_text(dom: &Dom, out: &mut Vec<String>) {
        if let NodeType::Text(t) = dom.root.get_node_type() {
            out.push(t.as_ref().as_str().to_string());
        }
        for child in dom.children.as_ref() {
            all_text(child, out);
        }
    }

    /// An icon theme is a directory of Inkscape-authored SVGs, and Inkscape
    /// writes an RDF block into every file it saves:
    ///
    /// ```text
    /// <metadata><rdf:RDF><cc:Work><dc:format>image/svg+xml</dc:format>…
    /// ```
    ///
    /// None of it is drawing. `<metadata>` is defined to render nothing, and
    /// an element in a foreign namespace is not rendered either - so the only
    /// thing a renderer takes out of that file is the `<path>`.
    ///
    /// Here every unrecognised tag became a `<div>` and every text child
    /// became a text node, so an icon drew the literal string
    /// `image/svg+xml`, clipped to the 16px icon box. That is what the window
    /// controls of a client-side titlebar came out as: the letters `im`, in
    /// place of a minimise bar.
    #[test]
    fn an_svg_metadata_block_is_not_drawn_as_text() {
        let map = ComponentMap::default();
        let svg = node(
            "svg",
            &[("width", "16"), ("height", "16")],
            vec![
                elem(node(
                    "metadata",
                    &[],
                    vec![elem(node(
                        "rdf:RDF",
                        &[],
                        vec![elem(node(
                            "cc:Work",
                            &[],
                            vec![elem(node("dc:format", &[], vec![txt("image/svg+xml")]))],
                        ))],
                    ))],
                )),
                elem(node("path", &[("d", "M4 10v1h8v-1z")], vec![])),
            ],
        );

        let dom = xml_node_to_dom_fast(&svg, &map, false, 0).expect("ok");

        let mut texts = Vec::new();
        all_text(&dom, &mut texts);
        assert!(
            texts.is_empty(),
            "an icon's metadata is ABOUT the drawing, not in it - but it drew {texts:?}"
        );
        assert_eq!(
            dom.children.as_ref().len(),
            1,
            "and the <path> is the one thing that survives"
        );
    }

    /// The same law, on the other half of what Inkscape leaves behind:
    /// `<sodipodi:namedview>` and `<inkscape:grid>` are editor state in a
    /// foreign namespace. They carry no text, so they were invisible - but
    /// they still became boxes in the middle of the artwork.
    #[test]
    fn a_foreign_namespaced_element_is_not_a_box() {
        let map = ComponentMap::default();
        let svg = node(
            "svg",
            &[],
            vec![
                elem(node(
                    "sodipodi:namedview",
                    &[],
                    vec![elem(node("inkscape:grid", &[], vec![]))],
                )),
                elem(node("path", &[], vec![])),
            ],
        );
        let dom = xml_node_to_dom_fast(&svg, &map, false, 0).expect("ok");
        assert_eq!(
            dom.children.as_ref().len(),
            1,
            "only the <path> is part of the drawing"
        );
    }

    /// A prefix is not by itself foreign: `<svg:path>` is the same element as
    /// `<path>`, written by a document that declares the SVG namespace.
    #[test]
    fn the_svg_prefix_still_draws() {
        let map = ComponentMap::default();
        let svg = node("svg", &[], vec![elem(node("svg:path", &[], vec![]))]);
        let dom = xml_node_to_dom_fast(&svg, &map, false, 0).expect("ok");
        assert_eq!(dom.children.as_ref().len(), 1, "<svg:path> IS a path");
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
    // str_to_dom / str_to_dom_unstyled
    // ================================================================

    #[test]
    fn a_document_without_a_root_gets_one_but_a_broken_html_is_still_an_error() {
        let map = ComponentMap::with_builtin();
        // A fragment (or nothing at all) is wrapped, not rejected.
        assert!(str_to_dom(&[], &map, None).is_ok());
        assert!(str_to_dom_unstyled(&[], &map).is_ok());
        assert!(str_to_dom_unstyled(&[elem(XmlNode::create("svg"))], &map).is_ok());

        // An EXPLICIT `<html>` still has to be well-formed: the author said
        // what the structure is, so a missing `<body>` is their mistake, not
        // something to paper over.
        let html_only = vec![elem(XmlNode::create("html"))];
        assert_eq!(
            str_to_dom(&html_only, &map, None).unwrap_err(),
            DomXmlParseError::NoBodyInHtml
        );
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

    // ================================================================
    // builtin_render_fn / how code builds a builtin (ComponentCodegen)
    // ================================================================

    #[test]
    fn builtin_elements_are_elements_and_the_structural_builtins_render_functions() {
        let map = ComponentMap::with_builtin();
        for def in map.all_components() {
            // `builtin:map` names two builtins: HTML's image map (an element)
            // and the structural map (a render function).
            let structural = match def.id.name.as_str() {
                "if" | "for" => true,
                "map" => def.display_name.as_str() != "Image Map",
                _ => false,
            };
            let want = if structural {
                ComponentCodegen::RenderFunction
            } else {
                ComponentCodegen::Element
            };
            assert_eq!(def.codegen, want, "{}", def.id.qualified_name());
        }
        // A zero-initialised C struct is a render-function component.
        assert_eq!(
            ComponentCodegen::render_function(),
            ComponentCodegen::RenderFunction
        );
    }

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
    fn a_builtins_preview_adds_its_example_but_what_it_renders_is_what_a_drop_inserts() {
        let map = ComponentMap::with_builtin();
        let ul = map.get_unqualified("ul").expect("builtin ul");
        assert_eq!(
            builtin_preview_dom("ul", &ul.data_model)
                .children
                .as_ref()
                .len(),
            2,
            "the preview of a <ul> holds two example items"
        );
        assert!(
            builtin_dom("ul", &ul.data_model, false)
                .children
                .as_ref()
                .is_empty(),
            "a dropped <ul> is empty: the example is the preview's only"
        );
        // A text element previews its text default, which a drop inserts too.
        let span = map.get_unqualified("span").expect("builtin span");
        assert_eq!(
            span.data_model
                .get_default_string("text")
                .map(AzString::as_str),
            Some("Span text")
        );
        // An example attribute goes through the XML attribute table.
        let input = map.get_unqualified("input").expect("builtin input");
        let preview = builtin_preview_dom("input", &input.data_model);
        assert!(preview
            .root
            .attributes()
            .iter()
            .any(|a| a.name().eq_ignore_ascii_case("placeholder")));
    }

    #[test]
    fn a_builtin_without_a_box_of_its_own_says_why_and_the_rest_do_not() {
        for tag in ["br", "option", "source", "head", "col"] {
            assert!(builtin_no_visual(tag).is_some(), "<{tag}> has no visual");
        }
        for tag in ["p", "div", "ul", "input", "hr", "svg"] {
            assert!(builtin_no_visual(tag).is_none(), "<{tag}> shows something");
        }
        assert!(builtin_no_visual("not-an-element").is_none());
        // Every element of the table is a registered builtin, once.
        let lib = register_builtin_components();
        for e in BUILTIN_ELEMENTS {
            let n = lib
                .components
                .as_ref()
                .iter()
                .filter(|c| c.id.name.as_str() == e.tag && c.codegen == ComponentCodegen::Element)
                .count();
            assert_eq!(n, 1, "<{}> is registered once", e.tag);
        }
    }

    // ================================================================
    // user_defined_render_fn
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
        // not "div": div takes its presentational `align` (39813667c)
        assert!(builtin_data_model("frobnicate").is_empty());
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
    // data_model_with_attributes
    // ================================================================

    /// The test attribute map as the loaders hand it in: name / value pairs.
    fn pairs(map: &XmlAttributeMap) -> Vec<(&str, &str)> {
        map.inner
            .as_ref()
            .iter()
            .map(|p| (p.key.as_str(), p.value.as_str()))
            .collect()
    }

    #[test]
    fn data_model_with_attributes_overrides_defaults_from_attributes() {
        let base = builtin_component_def("a", "Link", Some("Link text"), "").data_model;
        let model = data_model_with_attributes(&base, pairs(&attrs(&[("href", "/x")])));
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

    /// Text content is the loader's, not an attribute's: the `text` field is
    /// filled from the element's children and prepared at render
    /// (`prepare_string`), so the attribute pass leaves it alone.
    #[test]
    fn data_model_with_attributes_leaves_text_content_to_the_loader() {
        let base = builtin_component_def("a", "Link", Some("Link text"), "").data_model;
        let model = data_model_with_attributes(&base, pairs(&attrs(&[("text", "  Hello &amp; bye  ")])));
        assert_eq!(
            model.get_default_string("text").map(AzString::as_str),
            Some("  Hello &amp; bye  "),
            "an explicit text attribute is taken as written; content is prepared at render"
        );
    }

    #[test]
    fn data_model_with_attributes_ignores_unknown_attributes() {
        let base = builtin_component_def("a", "Link", Some(""), "").data_model;
        let before = base.fields.as_ref().len();
        let model = data_model_with_attributes(
            &base,
            pairs(&attrs(&[("data-nonsense", "1"), ("", ""), ("\u{1F600}", "x")])),
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

    // ---- Fluent l10n: data-l10n attribute parsing ----

    #[test]
    fn test_data_l10n_creates_localizable_text_node() {
        // `<p data-l10n="greeting">` stays a `<p>` (its UA style, its `p`
        // selectors, its a11y role) and gets ONE text child: the key
        // "greeting", marked localizable. The guide's own example reads
        // "Result: <p>Welcome back, Alice!</p>".
        use crate::dom::NodeType;

        let xml_node = XmlNode {
            node_type: "p".into(),
            attributes: {
                let mut v = crate::window::StringPairVec::from_const_slice(&[]);
                let mut pairs = v.into_library_owned_vec();
                pairs.push(crate::window::AzStringPair {
                    key: "data-l10n".into(),
                    value: "greeting".into(),
                });
                crate::window::StringPairVec::from_vec(pairs)
            }.into(),
            children: crate::xml::XmlNodeChildVec::from_const_slice(&[]),
        };

        let component_map = ComponentMap::with_builtin();
        let dom = xml_node_to_dom_fast(&xml_node, &component_map, false, 0)
            .expect("parse ok");

        assert_eq!(dom.root.node_type, NodeType::P, "the element keeps its tag");
        assert_eq!(dom.children.as_ref().len(), 1, "exactly one child: the key");
        match &dom.children.as_ref()[0].root.node_type {
            NodeType::Text(boxed) => {
                let s = boxed.as_ref();
                assert!(s.is_localizable(), "text node must be flagged localizable");
                assert_eq!(s.as_str(), "greeting", "text node must carry the l10n key");
            }
            other => panic!("expected Text child, got {:?}", other),
        }
        assert!(dom.root.fluent_args.is_none(), "no fluent args expected");
    }

    #[test]
    fn a_data_l10n_element_keeps_its_tag_in_the_arena_builder_too() {
        // `xml_node_to_fast_dom` shares `apply_xml_node_attributes` with the
        // tree builder above and must produce the same shape: p > text(key).
        use crate::dom::NodeType;

        let xml_node = XmlNode {
            node_type: "p".into(),
            attributes: {
                let mut pairs = Vec::new();
                pairs.push(crate::window::AzStringPair { key: "data-l10n".into(), value: "greeting".into() });
                crate::window::StringPairVec::from_vec(pairs)
            }.into(),
            children: crate::xml::XmlNodeChildVec::from_const_slice(&[]),
        };

        let component_map = ComponentMap::with_builtin();
        let mut builder = CompactDomBuilder::new();
        xml_node_to_fast_dom(&xml_node, &component_map, false, &mut builder, 0).expect("parse ok");
        let fast = builder.finish();
        let nodes = fast.node_data.as_ref();

        assert_eq!(nodes.len(), 2, "p + its key text, got {nodes:?}");
        assert_eq!(nodes[0].node_type, NodeType::P, "the element keeps its tag");
        match &nodes[1].node_type {
            NodeType::Text(boxed) => {
                assert!(boxed.as_ref().is_localizable());
                assert_eq!(boxed.as_ref().as_str(), "greeting");
            }
            other => panic!("expected Text child, got {:?}", other),
        }
    }

    #[test]
    fn test_data_l10n_with_fluent_args() {
        // `<p data-l10n="user-count" data-l10n-count="42">` should produce a
        // localizable Text node AND a FluentArgKV with key="count", value=I32(42).
        use crate::dom::{NodeType, FluentArg};

        let xml_node = XmlNode {
            node_type: "p".into(),
            attributes: {
                let mut pairs = Vec::new();
                pairs.push(crate::window::AzStringPair { key: "data-l10n".into(), value: "user-count".into() });
                pairs.push(crate::window::AzStringPair { key: "data-l10n-count".into(), value: "42".into() });
                crate::window::StringPairVec::from_vec(pairs)
            }.into(),
            children: crate::xml::XmlNodeChildVec::from_const_slice(&[]),
        };

        let component_map = ComponentMap::with_builtin();
        let dom = xml_node_to_dom_fast(&xml_node, &component_map, false, 0)
            .expect("parse ok");

        assert_eq!(dom.root.node_type, NodeType::P, "the element keeps its tag");
        match &dom.children.as_ref()[0].root.node_type {
            NodeType::Text(boxed) => {
                assert!(boxed.as_ref().is_localizable());
                assert_eq!(boxed.as_ref().as_str(), "user-count");
            }
            other => panic!("expected Text child, got {:?}", other),
        }

        // The arguments stay on the ELEMENT; its text child formats with
        // them (`translate_texts_in_dom` reads a text node's parent's args).
        let args = dom.root.fluent_args.as_ref().expect("fluent_args must be set");
        assert_eq!(args.as_slice().len(), 1);
        let kv = &args.as_slice()[0];
        assert_eq!(kv.key.as_str(), "count");
        assert!(matches!(kv.value, FluentArg::I32(42)));
    }

    #[test]
    fn test_azstring_tr_is_localizable() {
        // `AzString::tr("key")` should be flagged localizable; a regular string should not.
        let regular = azul_css::corety::AzString::from("hello");
        assert!(!regular.is_localizable(), "plain string must NOT be localizable");

        let tr = azul_css::corety::AzString::tr("greeting");
        assert!(tr.is_localizable(), "tr() string MUST be localizable");
        assert_eq!(tr.as_str(), "greeting", "key stored correctly");
    }

    /// A `style` attribute's value keeps every colon after the first: the
    /// widgets' `font-family: system:ui` (SYSUI8) and `url(https://...)`
    /// were cut at their second colon (`system`, `url(https`), so the
    /// declaration named a family no font has, or did not parse at all.
    #[test]
    fn a_style_attribute_value_keeps_its_colons() {
        use azul_css::props::{basic::font::StyleFontFamily, property::CssProperty};

        let map = azul_css::props::property::get_css_key_map();
        let decls = attributes::style_declarations(
            "font-family: system:ui; background-image: url(https://example.com/a.png)",
            &map,
        );
        let family = decls.iter().find_map(|d| match &d.property {
            CssProperty::FontFamily(v) => v.get_property().cloned(),
            _ => None,
        });
        let family = family.expect("the font-family declaration parses");
        assert!(
            matches!(
                family.as_ref().first(),
                Some(StyleFontFamily::SystemType(
                    azul_css::system::SystemFontType::Ui
                ))
            ),
            "font-family: system:ui is the system UI font role: {family:?}"
        );
        let background = decls
            .iter()
            .find(|d| matches!(d.property, CssProperty::BackgroundContent(_)))
            .map(|d| format!("{:?}", d.property));
        assert!(
            background
                .as_deref()
                .is_some_and(|b| b.contains("https://example.com/a.png")),
            "the image url keeps its scheme: {background:?}"
        );
    }
}
