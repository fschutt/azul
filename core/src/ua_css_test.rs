#[allow(unused_imports)]
pub use super::*;
#[cfg(test)]
mod autotest_generated {
    use alloc::{string::String, vec, vec::Vec};

    use azul_css::{corety::AzString, css::BoxOrStatic, props::basic::length::SizeMetric};

    use super::*;
    use crate::resources::{ImageRef, RawImageFormat};

    // ------------------------------------------------------------------
    // Constructors / helpers
    // ------------------------------------------------------------------

    fn text_node(s: &str) -> NodeType {
        NodeType::Text(BoxOrStatic::heap(AzString::from(s)))
    }

    /// A VirtualView is a scroll container BY DEFAULT: `overflow: auto` on
    /// both axes from the UA sheet, so app CSS no longer has to opt in (the
    /// virtual-size-aware necessity rule keeps bars away until the published
    /// `virtual_scroll_size` actually overflows). Opt-outs stay explicit
    /// (`overflow: hidden` — map/video); invisible-but-scrollable composes
    /// via `scrollbar-width: none`.
    #[test]
    fn virtual_view_defaults_to_overflow_auto_on_both_axes() {
        use azul_css::props::layout::LayoutOverflow;
        for pt in [CssPropertyType::OverflowX, CssPropertyType::OverflowY] {
            let got = get_ua_property(&NodeType::VirtualView, pt)
                .unwrap_or_else(|| panic!("no UA default for VirtualView {pt:?}"));
            let ok = matches!(
                got,
                CssProperty::OverflowX(CssPropertyValue::Exact(LayoutOverflow::Auto))
                    | CssProperty::OverflowY(CssPropertyValue::Exact(LayoutOverflow::Auto))
            );
            assert!(
                ok,
                "VirtualView {pt:?} UA default is not overflow:auto: {got:?}"
            );
        }
        // And the axis-correct variant is returned for each request.
        assert!(matches!(
            get_ua_property(&NodeType::VirtualView, CssPropertyType::OverflowX),
            Some(CssProperty::OverflowX(_))
        ));
        assert!(matches!(
            get_ua_property(&NodeType::VirtualView, CssPropertyType::OverflowY),
            Some(CssProperty::OverflowY(_))
        ));
    }

    fn icon_node(s: &str) -> NodeType {
        NodeType::Icon(BoxOrStatic::heap(AzString::from(s)))
    }

    fn image_node() -> NodeType {
        NodeType::Image(BoxOrStatic::heap(ImageRef::null_image(
            1,
            1,
            RawImageFormat::RGBA8,
            Vec::new(),
        )))
    }

    /// Broad (not literally exhaustive) sample of `NodeType`, covering every
    /// variant that has an arm in `get_ua_property` plus a spread of variants
    /// that have none, so the catch-all arms get exercised too.
    fn sample_node_types() -> Vec<NodeType> {
        use crate::dom::NodeType as NT;
        vec![
            // matched arms
            NT::Html,
            NT::Head,
            NT::Body,
            NT::Div,
            NT::P,
            NT::Main,
            NT::Header,
            NT::Footer,
            NT::Section,
            NT::Article,
            NT::Aside,
            NT::Nav,
            NT::H1,
            NT::H2,
            NT::H3,
            NT::H4,
            NT::H5,
            NT::H6,
            NT::Ul,
            NT::Ol,
            NT::Li,
            NT::Dl,
            NT::Dt,
            NT::Dd,
            NT::Span,
            NT::A,
            NT::Strong,
            NT::Em,
            NT::B,
            NT::I,
            NT::U,
            NT::Small,
            NT::Code,
            NT::Kbd,
            NT::Samp,
            NT::Sub,
            NT::Sup,
            NT::Pre,
            NT::BlockQuote,
            NT::Hr,
            NT::Table,
            NT::THead,
            NT::TBody,
            NT::TFoot,
            NT::Tr,
            NT::Th,
            NT::Td,
            NT::Caption,
            NT::ColGroup,
            NT::Col,
            NT::Form,
            NT::Input,
            NT::Button,
            NT::Select,
            NT::TextArea,
            NT::Label,
            NT::Title,
            NT::Script,
            NT::Style,
            NT::Link,
            NT::Br,
            NT::Video,
            NT::Audio,
            NT::Canvas,
            NT::Svg,
            NT::VirtualView,
            NT::SelectOption,
            NT::OptGroup,
            NT::Abbr,
            NT::Cite,
            NT::Del,
            NT::Ins,
            NT::Mark,
            NT::Q,
            NT::Dfn,
            NT::Var,
            NT::Time,
            NT::Data,
            NT::Wbr,
            NT::Bdi,
            NT::Bdo,
            NT::Rp,
            NT::Rt,
            NT::Rtc,
            NT::Ruby,
            NT::FieldSet,
            NT::Figure,
            NT::FigCaption,
            NT::Details,
            NT::Summary,
            NT::Dialog,
            NT::Menu,
            NT::Dir,
            // unmatched arms (must fall through to the catch-alls)
            NT::Address,
            NT::Legend,
            NT::Output,
            NT::Progress,
            NT::Meter,
            NT::DataList,
            NT::MenuItem,
            NT::S,
            NT::Big,
            NT::Acronym,
            NT::Object,
            NT::Param,
            NT::Embed,
            NT::Source,
            NT::Track,
            NT::Map,
            NT::Area,
            NT::Meta,
            NT::Base,
            NT::Before,
            NT::After,
            NT::Marker,
            NT::Placeholder,
            NT::SvgG,
            NT::SvgPath,
            NT::SvgRect,
            NT::SvgText(AzString::from("svg-text")),
            // payload-carrying variants
            text_node(""),
            text_node("hello"),
            icon_node("home"),
            image_node(),
        ]
    }

    fn all_os() -> Vec<OsCondition> {
        vec![
            OsCondition::Any,
            OsCondition::Apple,
            OsCondition::MacOS,
            OsCondition::IOS,
            OsCondition::Linux,
            OsCondition::Windows,
            OsCondition::Android,
            OsCondition::Web,
        ]
    }

    fn all_themes() -> Vec<ThemeCondition> {
        vec![
            ThemeCondition::Light,
            ThemeCondition::Dark,
            ThemeCondition::Custom(AzString::from("neon")),
            ThemeCondition::SystemPreferred,
        ]
    }

    fn ctx(os: OsCondition, theme: ThemeCondition) -> DynamicSelectorContext {
        DynamicSelectorContext {
            os,
            theme,
            ..DynamicSelectorContext::default()
        }
    }

    const CLASSIC_LIGHT_THUMB: ColorU = ColorU {
        r: 193,
        g: 193,
        b: 193,
        a: 255,
    };
    const CLASSIC_LIGHT_TRACK: ColorU = ColorU {
        r: 241,
        g: 241,
        b: 241,
        a: 255,
    };

    fn custom_color(thumb: ColorU, track: ColorU) -> StyleScrollbarColor {
        StyleScrollbarColor::Custom(ScrollbarColorCustom { thumb, track })
    }

    /// Extract the `(thumb, track)` pair, panicking if the property is not a
    /// `Custom` scrollbar color.
    fn unwrap_custom(c: StyleScrollbarColor) -> (ColorU, ColorU) {
        match c {
            StyleScrollbarColor::Custom(c) => (c.thumb, c.track),
            StyleScrollbarColor::Auto => panic!("expected a Custom scrollbar color, got Auto"),
        }
    }

    fn display_of(nt: &NodeType) -> LayoutDisplay {
        match get_ua_property(nt, CssPropertyType::Display) {
            Some(CssProperty::Display(CssPropertyValue::Exact(d))) => *d,
            other => panic!("{nt:?}: expected an exact display value, got {other:?}"),
        }
    }

    fn font_size_em(nt: &NodeType) -> f32 {
        match get_ua_property(nt, CssPropertyType::FontSize) {
            Some(CssProperty::FontSize(CssPropertyValue::Exact(fs))) => {
                assert_eq!(
                    fs.inner.metric,
                    SizeMetric::Em,
                    "{nt:?}: font-size must be em-relative"
                );
                fs.inner.number.get()
            }
            other => panic!("{nt:?}: expected an exact em font-size, got {other:?}"),
        }
    }

    // ==================================================================
    // get_ua_property — table-wide invariants
    // ==================================================================

    /// The single most important invariant of the lookup table: the property
    /// that comes back must be *the property that was asked for*. A copy-paste
    /// slip in the ~200-arm table (e.g. `(H1, MarginBottom) => &MARGIN_TOP_...`)
    /// would silently mis-style elements; nothing else in the codebase checks it.
    #[test]
    fn returned_property_always_has_the_requested_type() {
        for nt in sample_node_types() {
            for pt in CssPropertyType::ALL {
                if let Some(prop) = get_ua_property(&nt, *pt) {
                    assert_eq!(
                        prop.get_type(),
                        *pt,
                        "get_ua_property({nt:?}, {pt:?}) returned a {:?} property",
                        prop.get_type()
                    );
                }
            }
        }
    }

    #[test]
    fn full_cross_product_never_panics_and_is_deterministic() {
        for nt in sample_node_types() {
            for pt in CssPropertyType::ALL {
                let a = get_ua_property(&nt, *pt);
                let b = get_ua_property(&nt, *pt);
                match (a, b) {
                    (Some(a), Some(b)) => assert!(
                        core::ptr::eq(a, b),
                        "{nt:?}/{pt:?}: repeated lookups must hand back the same static"
                    ),
                    (None, None) => {}
                    _ => panic!("{nt:?}/{pt:?}: lookup is not deterministic"),
                }
            }
        }
    }

    /// Documented contract: the `(_, Display)` catch-all means *every* node type
    /// resolves a display value, so layout never sees a node without one.
    #[test]
    fn display_resolves_for_every_node_type() {
        for nt in sample_node_types() {
            assert!(
                get_ua_property(&nt, CssPropertyType::Display).is_some(),
                "{nt:?} has no default display"
            );
        }
    }

    #[test]
    fn unknown_elements_default_to_inline_display() {
        // Per CSS spec, unknown/custom elements are inline.
        for nt in [NodeType::Address, NodeType::Legend, NodeType::Meter] {
            assert_eq!(display_of(&nt), LayoutDisplay::Inline, "{nt:?}");
        }
    }

    /// SVG shapes are NOT unknown elements: they are painted boxes.
    ///
    /// `SvgPath` used to be listed above as an example of the inline default,
    /// and that default is exactly what stopped an SVG in a DOM from painting
    /// - `paint_node_background_and_border` skips inline boxes, because their
    /// backgrounds belong to text layout, which knows nothing about SVG
    /// geometry. The definition containers stay `display: none`: they define,
    /// they do not draw.
    #[test]
    fn svg_shapes_are_boxes_and_svg_definitions_do_not_draw() {
        for nt in [
            NodeType::SvgPath,
            NodeType::SvgCircle,
            NodeType::SvgRect,
            NodeType::SvgEllipse,
            NodeType::SvgLine,
            NodeType::SvgPolygon,
            NodeType::SvgPolyline,
            NodeType::SvgG,
            NodeType::SvgUse,
        ] {
            assert_eq!(display_of(&nt), LayoutDisplay::Block, "{nt:?} must paint");
        }
        for nt in [
            NodeType::SvgDefs,
            NodeType::SvgSymbol,
            NodeType::SvgClipPathElement,
        ] {
            assert_eq!(display_of(&nt), LayoutDisplay::None, "{nt:?} must not draw");
        }
        // The `<svg>` element itself is a REPLACED element, like `<img>`:
        // inline-level, but with a box, so its intrinsic size applies.
        assert_eq!(display_of(&NodeType::Svg), LayoutDisplay::InlineBlock);
    }

    /// `cursor` is deliberately defined for exactly three node types; anything
    /// else must return `None` so the cursor-resolution walk can inherit.
    #[test]
    fn cursor_default_exists_only_for_button_textarea_and_text() {
        for nt in sample_node_types() {
            let has_cursor = get_ua_property(&nt, CssPropertyType::Cursor).is_some();
            let expected = matches!(
                nt,
                NodeType::Button | NodeType::TextArea | NodeType::Text(_)
            );
            assert_eq!(has_cursor, expected, "{nt:?}: unexpected cursor default");
        }
    }

    // ==================================================================
    // get_ua_property — payload-carrying node types (unicode / huge / empty)
    // ==================================================================

    #[test]
    fn text_node_defaults_are_independent_of_the_payload() {
        let huge = "🦀".repeat(100_000);
        let payloads: Vec<String> = vec![
            String::new(),
            "\0".into(),
            "\u{202E}\u{200B}\u{FEFF}".into(), // RTL override, ZWSP, BOM
            "مرحبا بالعالم".into(),
            "🇩🇪👨‍👩‍👧‍👦".into(),
            "\u{FFFD}".into(),
            huge,
        ];

        for p in payloads {
            let nt = text_node(&p);
            assert_eq!(
                display_of(&nt),
                LayoutDisplay::Inline,
                "text node display must not depend on its content"
            );
            assert_eq!(
                get_ua_property(&nt, CssPropertyType::Cursor),
                Some(&CURSOR_TEXT),
                "text node cursor must not depend on its content"
            );
            // Text nodes define no box properties of their own.
            assert!(get_ua_property(&nt, CssPropertyType::Width).is_none());
            assert!(get_ua_property(&nt, CssPropertyType::Height).is_none());
            assert!(get_ua_property(&nt, CssPropertyType::MarginTop).is_none());
        }
    }

    #[test]
    fn icon_and_image_nodes_are_inline_block_regardless_of_payload() {
        let huge_name = "x".repeat(50_000);
        let names: [&str; 4] = ["", "home", "🏠", huge_name.as_str()];
        for name in names {
            assert_eq!(
                display_of(&icon_node(name)),
                LayoutDisplay::InlineBlock,
                "icon {name:?}"
            );
        }
        assert_eq!(display_of(&image_node()), LayoutDisplay::InlineBlock);
    }

    // ==================================================================
    // get_ua_property — specific, load-bearing defaults
    // ==================================================================

    /// Regression guard for the 2026-06-02 DIAG revert documented in the table:
    /// `(Html, Height) => HEIGHT_100_PERCENT` is commented out on purpose. If it
    /// comes back without the jump-table dispatch fix, children wrongly inherit
    /// `height: 100%`.
    #[test]
    fn html_has_no_default_height() {
        assert_eq!(
            get_ua_property(&NodeType::Html, CssPropertyType::Display),
            Some(&DISPLAY_BLOCK)
        );
        assert!(
            get_ua_property(&NodeType::Html, CssPropertyType::Height).is_none(),
            "the (Html, Height) arm is intentionally disabled — see the DIAG note"
        );
    }

    /// `body { margin: 8px }` (Chrome UA), and crucially *no* width/height:
    /// giving body a size would break percentage sizing of its children.
    #[test]
    fn body_has_8px_margins_and_no_intrinsic_size() {
        assert_eq!(display_of(&NodeType::Body), LayoutDisplay::Block);
        assert_eq!(
            get_ua_property(&NodeType::Body, CssPropertyType::MarginTop),
            Some(&MARGIN_TOP_8PX)
        );
        assert_eq!(
            get_ua_property(&NodeType::Body, CssPropertyType::MarginBottom),
            Some(&MARGIN_BOTTOM_8PX)
        );
        assert_eq!(
            get_ua_property(&NodeType::Body, CssPropertyType::MarginLeft),
            Some(&MARGIN_LEFT_8PX)
        );
        assert_eq!(
            get_ua_property(&NodeType::Body, CssPropertyType::MarginRight),
            Some(&MARGIN_RIGHT_8PX)
        );
        assert!(get_ua_property(&NodeType::Body, CssPropertyType::Width).is_none());
        assert!(get_ua_property(&NodeType::Body, CssPropertyType::Height).is_none());
    }

    /// Block elements must have `width: auto`, not `width: 100%` — the comment in
    /// the table calls this out as critical for flexbox (100% defeats flex-grow).
    #[test]
    fn block_elements_have_no_default_width() {
        for nt in [
            NodeType::Div,
            NodeType::P,
            NodeType::Section,
            NodeType::Main,
            NodeType::VirtualView,
        ] {
            assert_eq!(display_of(&nt), LayoutDisplay::Block, "{nt:?}");
            assert!(
                get_ua_property(&nt, CssPropertyType::Width).is_none(),
                "{nt:?} must be width:auto so it can flex-grow"
            );
        }
    }

    #[test]
    fn div_defines_only_a_display_default() {
        for pt in CssPropertyType::ALL {
            let got = get_ua_property(&NodeType::Div, *pt);
            if *pt == CssPropertyType::Display {
                assert!(got.is_some());
            } else {
                assert!(
                    got.is_none(),
                    "Div should not define a UA default for {pt:?}"
                );
            }
        }
    }

    #[test]
    fn metadata_elements_are_display_none() {
        for nt in [
            NodeType::Head,
            NodeType::Title,
            NodeType::Script,
            NodeType::Style,
            NodeType::Link,
        ] {
            assert_eq!(
                display_of(&nt),
                LayoutDisplay::None,
                "{nt:?} must not render"
            );
        }
    }

    #[test]
    fn heading_font_sizes_are_strictly_decreasing() {
        let sizes: Vec<f32> = [
            NodeType::H1,
            NodeType::H2,
            NodeType::H3,
            NodeType::H4,
            NodeType::H5,
            NodeType::H6,
        ]
        .iter()
        .map(font_size_em)
        .collect();

        // Chrome UA values — also verifies `const_em_fractional(1, 5)` really
        // encodes 1.5 (and not 1.05), which the digit-count encoding makes subtle.
        let expected = [2.0_f32, 1.5, 1.17, 1.0, 0.83, 0.67];
        for (i, (got, want)) in sizes.iter().zip(expected.iter()).enumerate() {
            assert!(
                (got - want).abs() < 1e-4,
                "H{} font-size: got {got}em, want {want}em",
                i + 1
            );
        }
        for w in sizes.windows(2) {
            assert!(
                w[0] > w[1],
                "heading font sizes must strictly decrease, got {sizes:?}"
            );
        }
    }

    #[test]
    fn headings_are_bold_blocks_that_avoid_page_breaks() {
        for nt in [
            NodeType::H1,
            NodeType::H2,
            NodeType::H3,
            NodeType::H4,
            NodeType::H5,
            NodeType::H6,
        ] {
            assert_eq!(display_of(&nt), LayoutDisplay::Block, "{nt:?}");
            assert_eq!(
                get_ua_property(&nt, CssPropertyType::FontWeight),
                Some(&FONT_WEIGHT_BOLD),
                "{nt:?}"
            );
            assert_eq!(
                get_ua_property(&nt, CssPropertyType::BreakInside),
                Some(&BREAK_INSIDE_AVOID),
                "{nt:?}"
            );
            assert_eq!(
                get_ua_property(&nt, CssPropertyType::BreakAfter),
                Some(&BREAK_AFTER_AVOID),
                "{nt:?}"
            );
            // Both margins must exist and be em-relative (they scale with font-size).
            for pt in [CssPropertyType::MarginTop, CssPropertyType::MarginBottom] {
                assert!(
                    get_ua_property(&nt, pt).is_some(),
                    "{nt:?} is missing {pt:?}"
                );
            }
        }
    }

    /// Tables *can* break across pages; their rows/headers/footers cannot. The
    /// table comments say so explicitly, so lock the asymmetry in.
    #[test]
    fn tables_may_break_across_pages_but_rows_may_not() {
        assert!(get_ua_property(&NodeType::Table, CssPropertyType::BreakInside).is_none());
        assert!(get_ua_property(&NodeType::TBody, CssPropertyType::BreakInside).is_none());
        for nt in [NodeType::THead, NodeType::TFoot, NodeType::Tr] {
            assert_eq!(
                get_ua_property(&nt, CssPropertyType::BreakInside),
                Some(&BREAK_INSIDE_AVOID),
                "{nt:?}"
            );
        }
    }

    #[test]
    fn table_display_types_are_not_crossed() {
        assert_eq!(display_of(&NodeType::Table), LayoutDisplay::Table);
        assert_eq!(
            display_of(&NodeType::THead),
            LayoutDisplay::TableHeaderGroup
        );
        assert_eq!(display_of(&NodeType::TBody), LayoutDisplay::TableRowGroup);
        assert_eq!(
            display_of(&NodeType::TFoot),
            LayoutDisplay::TableFooterGroup
        );
        assert_eq!(display_of(&NodeType::Tr), LayoutDisplay::TableRow);
        assert_eq!(display_of(&NodeType::Th), LayoutDisplay::TableCell);
        assert_eq!(display_of(&NodeType::Td), LayoutDisplay::TableCell);
        assert_eq!(display_of(&NodeType::Caption), LayoutDisplay::TableCaption);
        assert_eq!(
            display_of(&NodeType::ColGroup),
            LayoutDisplay::TableColumnGroup
        );
        assert_eq!(display_of(&NodeType::Col), LayoutDisplay::TableColumn);
    }

    #[test]
    fn table_cells_have_1px_padding_on_all_four_sides() {
        for nt in [NodeType::Th, NodeType::Td] {
            assert_eq!(
                get_ua_property(&nt, CssPropertyType::PaddingTop),
                Some(&PADDING_TOP_1PX),
                "{nt:?}"
            );
            assert_eq!(
                get_ua_property(&nt, CssPropertyType::PaddingBottom),
                Some(&PADDING_BOTTOM_1PX),
                "{nt:?}"
            );
            assert_eq!(
                get_ua_property(&nt, CssPropertyType::PaddingLeft),
                Some(&PADDING_LEFT_1PX),
                "{nt:?}"
            );
            assert_eq!(
                get_ua_property(&nt, CssPropertyType::PaddingRight),
                Some(&PADDING_RIGHT_1PX),
                "{nt:?}"
            );
            assert_eq!(
                get_ua_property(&nt, CssPropertyType::VerticalAlign),
                Some(&VERTICAL_ALIGN_MIDDLE),
                "{nt:?}"
            );
        }
        // Only <th> is centered + bold.
        assert_eq!(
            get_ua_property(&NodeType::Th, CssPropertyType::TextAlign),
            Some(&TEXT_ALIGN_CENTER)
        );
        assert_eq!(
            get_ua_property(&NodeType::Th, CssPropertyType::FontWeight),
            Some(&FONT_WEIGHT_BOLD)
        );
        assert!(get_ua_property(&NodeType::Td, CssPropertyType::TextAlign).is_none());
        assert!(get_ua_property(&NodeType::Td, CssPropertyType::FontWeight).is_none());
    }

    /// A button's border is symmetric. Crossed sides (e.g. `BorderLeftWidth`
    /// answered with the *top* static) would render an asymmetric button, so
    /// check that each side carries the value the table promises.
    #[test]
    fn button_border_is_symmetric_on_all_four_sides() {
        let widths = [
            (CssPropertyType::BorderTopWidth, &BUTTON_BORDER_TOP_WIDTH),
            (
                CssPropertyType::BorderBottomWidth,
                &BUTTON_BORDER_BOTTOM_WIDTH,
            ),
            (CssPropertyType::BorderLeftWidth, &BUTTON_BORDER_LEFT_WIDTH),
            (
                CssPropertyType::BorderRightWidth,
                &BUTTON_BORDER_RIGHT_WIDTH,
            ),
        ];
        for (pt, want) in widths {
            assert_eq!(get_ua_property(&NodeType::Button, pt), Some(want), "{pt:?}");
        }

        let styles = [
            (CssPropertyType::BorderTopStyle, &BUTTON_BORDER_TOP_STYLE),
            (
                CssPropertyType::BorderBottomStyle,
                &BUTTON_BORDER_BOTTOM_STYLE,
            ),
            (CssPropertyType::BorderLeftStyle, &BUTTON_BORDER_LEFT_STYLE),
            (
                CssPropertyType::BorderRightStyle,
                &BUTTON_BORDER_RIGHT_STYLE,
            ),
        ];
        for (pt, want) in styles {
            assert_eq!(get_ua_property(&NodeType::Button, pt), Some(want), "{pt:?}");
        }

        let colors = [
            (CssPropertyType::BorderTopColor, &BUTTON_BORDER_TOP_COLOR),
            (
                CssPropertyType::BorderBottomColor,
                &BUTTON_BORDER_BOTTOM_COLOR,
            ),
            (CssPropertyType::BorderLeftColor, &BUTTON_BORDER_LEFT_COLOR),
            (
                CssPropertyType::BorderRightColor,
                &BUTTON_BORDER_RIGHT_COLOR,
            ),
        ];
        for (pt, want) in colors {
            assert_eq!(get_ua_property(&NodeType::Button, pt), Some(want), "{pt:?}");
        }

        assert_eq!(display_of(&NodeType::Button), LayoutDisplay::InlineBlock);
        assert_eq!(
            get_ua_property(&NodeType::Button, CssPropertyType::Cursor),
            Some(&CURSOR_POINTER)
        );
    }

    /// `<hr>` draws its line from the *border*, not from a height — height must
    /// be exactly 0px, and the width exactly 100%.
    #[test]
    fn hr_line_comes_from_the_border_not_from_height() {
        match get_ua_property(&NodeType::Hr, CssPropertyType::Height) {
            Some(CssProperty::Height(CssPropertyValue::Exact(LayoutHeight::Px(pv)))) => {
                assert_eq!(pv.metric, SizeMetric::Px);
                assert!(
                    (pv.number.get() - 0.0).abs() < 1e-6,
                    "hr height must be 0px"
                );
            }
            other => panic!("hr height: {other:?}"),
        }
        match get_ua_property(&NodeType::Hr, CssPropertyType::Width) {
            Some(CssProperty::Width(CssPropertyValue::Exact(LayoutWidth::Px(pv)))) => {
                assert_eq!(pv.metric, SizeMetric::Percent);
                assert!(
                    (pv.number.get() - 100.0).abs() < 1e-4,
                    "hr width must be 100%"
                );
            }
            other => panic!("hr width: {other:?}"),
        }
        assert_eq!(
            get_ua_property(&NodeType::Hr, CssPropertyType::BorderTopStyle),
            Some(&BORDER_TOP_STYLE_INSET)
        );
        assert_eq!(
            get_ua_property(&NodeType::Hr, CssPropertyType::BorderTopWidth),
            Some(&BORDER_TOP_WIDTH_1PX)
        );
        assert_eq!(
            get_ua_property(&NodeType::Hr, CssPropertyType::BorderTopColor),
            Some(&BORDER_TOP_COLOR_GRAY)
        );
    }

    #[test]
    fn list_containers_reset_the_counter_and_reserve_marker_space() {
        for (nt, marker) in [
            (NodeType::Ul, &LIST_STYLE_TYPE_DISC),
            (NodeType::Ol, &LIST_STYLE_TYPE_DECIMAL),
        ] {
            assert_eq!(display_of(&nt), LayoutDisplay::Block, "{nt:?}");
            assert_eq!(
                get_ua_property(&nt, CssPropertyType::ListStyleType),
                Some(marker),
                "{nt:?}"
            );
            assert_eq!(
                get_ua_property(&nt, CssPropertyType::CounterReset),
                Some(&COUNTER_RESET_LIST_ITEM),
                "{nt:?} must reset the list-item counter"
            );
            assert_eq!(
                get_ua_property(&nt, CssPropertyType::PaddingLeft),
                Some(&PADDING_INLINE_START_40PX),
                "{nt:?}"
            );
        }
        assert_eq!(display_of(&NodeType::Li), LayoutDisplay::ListItem);
    }

    #[test]
    fn inline_emphasis_and_link_defaults() {
        assert_eq!(
            get_ua_property(&NodeType::A, CssPropertyType::TextDecoration),
            Some(&TEXT_DECORATION_UNDERLINE)
        );
        assert_eq!(
            get_ua_property(&NodeType::U, CssPropertyType::TextDecoration),
            Some(&TEXT_DECORATION_UNDERLINE)
        );
        assert_eq!(
            get_ua_property(&NodeType::Strong, CssPropertyType::FontWeight),
            Some(&FONT_WEIGHT_BOLDER)
        );
        assert_eq!(
            get_ua_property(&NodeType::B, CssPropertyType::FontWeight),
            Some(&FONT_WEIGHT_BOLDER)
        );
        // <em>/<i> are italic via font-style, which the UA table does not define.
        assert!(get_ua_property(&NodeType::Em, CssPropertyType::FontWeight).is_none());
        assert!(get_ua_property(&NodeType::I, CssPropertyType::FontWeight).is_none());
    }

    // ==================================================================
    // const scrollbar helpers — numeric round-trips / boundaries
    // ==================================================================

    #[test]
    fn scrollbar_fade_delay_round_trips_every_boundary() {
        for ms in [
            0_u32,
            1,
            2,
            299,
            300,
            500,
            u32::from(u16::MAX),
            i32::MAX as u32,
            u32::MAX - 1,
            u32::MAX,
        ] {
            match scrollbar_fade_delay(ms) {
                CssProperty::ScrollbarFadeDelay(CssPropertyValue::Exact(d)) => {
                    assert_eq!(d.ms, ms, "fade-delay must round-trip losslessly");
                }
                other => panic!("scrollbar_fade_delay({ms}) built a {other:?}"),
            }
        }
    }

    #[test]
    fn scrollbar_fade_duration_round_trips_every_boundary() {
        for ms in [
            0_u32,
            1,
            150,
            200,
            u32::from(u16::MAX),
            i32::MAX as u32,
            u32::MAX - 1,
            u32::MAX,
        ] {
            match scrollbar_fade_duration(ms) {
                CssProperty::ScrollbarFadeDuration(CssPropertyValue::Exact(d)) => {
                    assert_eq!(d.ms, ms, "fade-duration must round-trip losslessly");
                }
                other => panic!("scrollbar_fade_duration({ms}) built a {other:?}"),
            }
        }
    }

    /// `u32::MAX` in a `const` item: if either helper ever grew an arithmetic
    /// conversion (ms → ns, ms → seconds), this fails to *compile* rather than
    /// silently wrapping in release and panicking in debug.
    #[test]
    fn scrollbar_fade_helpers_are_const_evaluable_at_u32_max() {
        const MAX_DELAY: CssProperty = scrollbar_fade_delay(u32::MAX);
        const MAX_DURATION: CssProperty = scrollbar_fade_duration(u32::MAX);
        const ZERO_DELAY: CssProperty = scrollbar_fade_delay(0);

        assert_eq!(MAX_DELAY, scrollbar_fade_delay(u32::MAX));
        assert_eq!(MAX_DURATION, scrollbar_fade_duration(u32::MAX));
        assert_eq!(ZERO_DELAY, scrollbar_fade_delay(0));
    }

    /// The two helpers take the same `u32` and differ only in the wrapper type —
    /// exactly the shape a copy-paste bug likes. Assert they stay distinct.
    #[test]
    fn fade_delay_and_fade_duration_produce_distinct_property_types() {
        assert_eq!(
            scrollbar_fade_delay(42).get_type(),
            CssPropertyType::ScrollbarFadeDelay
        );
        assert_eq!(
            scrollbar_fade_duration(42).get_type(),
            CssPropertyType::ScrollbarFadeDuration
        );
        assert_ne!(scrollbar_fade_delay(42), scrollbar_fade_duration(42));
    }

    /// A `0` delay means "never fades" (per the `ScrollbarFadeDelay` docs), so it
    /// must be stored as a literal zero, not as a sentinel.
    #[test]
    fn zero_fade_delay_and_duration_are_literal_zero() {
        assert_eq!(
            scrollbar_fade_delay(0),
            CssProperty::ScrollbarFadeDelay(CssPropertyValue::Exact(ScrollbarFadeDelay::ZERO))
        );
        assert_eq!(
            scrollbar_fade_duration(0),
            CssProperty::ScrollbarFadeDuration(CssPropertyValue::Exact(
                ScrollbarFadeDuration::ZERO
            ))
        );
    }

    #[test]
    fn scrollbar_color_never_swaps_thumb_and_track() {
        let cases = [
            (
                ColorU {
                    r: 1,
                    g: 2,
                    b: 3,
                    a: 4,
                },
                ColorU {
                    r: 5,
                    g: 6,
                    b: 7,
                    a: 8,
                },
            ),
            (
                ColorU {
                    r: 0,
                    g: 0,
                    b: 0,
                    a: 0,
                },
                ColorU {
                    r: 255,
                    g: 255,
                    b: 255,
                    a: 255,
                },
            ),
            (
                ColorU {
                    r: 255,
                    g: 255,
                    b: 255,
                    a: 255,
                },
                ColorU::TRANSPARENT,
            ),
            (ColorU::TRANSPARENT, ColorU::TRANSPARENT),
        ];
        for (thumb, track) in cases {
            match scrollbar_color(thumb, track) {
                CssProperty::ScrollbarColor(CssPropertyValue::Exact(
                    StyleScrollbarColor::Custom(c),
                )) => {
                    assert_eq!(c.thumb, thumb, "thumb was not preserved");
                    assert_eq!(
                        c.track, track,
                        "track was not preserved (arguments swapped?)"
                    );
                }
                other => panic!("scrollbar_color built a {other:?}"),
            }
        }
    }

    #[test]
    fn scrollbar_width_and_visibility_round_trip_every_variant() {
        for w in [
            LayoutScrollbarWidth::Auto,
            LayoutScrollbarWidth::Thin,
            LayoutScrollbarWidth::None,
        ] {
            match scrollbar_width(w) {
                CssProperty::ScrollbarWidth(CssPropertyValue::Exact(got)) => assert_eq!(got, w),
                other => panic!("scrollbar_width({w:?}) built a {other:?}"),
            }
        }
        for v in [
            ScrollbarVisibilityMode::Always,
            ScrollbarVisibilityMode::WhenScrolling,
            ScrollbarVisibilityMode::Auto,
        ] {
            match scrollbar_visibility(v) {
                CssProperty::ScrollbarVisibility(CssPropertyValue::Exact(got)) => {
                    assert_eq!(got, v)
                }
                other => panic!("scrollbar_visibility({v:?}) built a {other:?}"),
            }
        }
    }

    // ==================================================================
    // UA_SCROLLBAR_CSS — table shape invariants
    // ==================================================================

    /// `evaluate_ua_scrollbar_css` matches on exactly five property kinds and
    /// silently drops everything else via `_ => {}`. A sixth property added to
    /// the table would therefore never take effect — fail loudly here instead.
    #[test]
    fn table_contains_only_property_kinds_the_evaluator_understands() {
        let understood = [
            CssPropertyType::ScrollbarColor,
            CssPropertyType::ScrollbarWidth,
            CssPropertyType::ScrollbarVisibility,
            CssPropertyType::ScrollbarFadeDelay,
            CssPropertyType::ScrollbarFadeDuration,
        ];
        for (i, entry) in UA_SCROLLBAR_CSS.iter().enumerate() {
            let ty = entry.property.get_type();
            assert!(
                understood.contains(&ty),
                "UA_SCROLLBAR_CSS[{i}] is a {ty:?}, which evaluate_ua_scrollbar_css ignores"
            );
        }
    }

    /// The evaluator only reads `CssPropertyValue::Exact`; an `Auto`/`Inherit`
    /// entry would be skipped without a trace.
    #[test]
    fn every_table_entry_carries_an_exact_value() {
        for (i, entry) in UA_SCROLLBAR_CSS.iter().enumerate() {
            let is_exact = matches!(
                &entry.property,
                CssProperty::ScrollbarColor(CssPropertyValue::Exact(_))
                    | CssProperty::ScrollbarWidth(CssPropertyValue::Exact(_))
                    | CssProperty::ScrollbarVisibility(CssPropertyValue::Exact(_))
                    | CssProperty::ScrollbarFadeDelay(CssPropertyValue::Exact(_))
                    | CssProperty::ScrollbarFadeDuration(CssPropertyValue::Exact(_))
            );
            assert!(
                is_exact,
                "UA_SCROLLBAR_CSS[{i}] is not an Exact value: {:?}",
                entry.property
            );
        }
    }

    /// The documented guarantee ("unconditional fallback entries … guarantee that
    /// every field resolves") plus the ordering rule it depends on: under
    /// first-match-wins, an unconditional entry that is *not* last for its
    /// property type would make every rule after it dead code.
    #[test]
    fn each_property_type_has_exactly_one_unconditional_entry_and_it_is_last() {
        for ty in [
            CssPropertyType::ScrollbarColor,
            CssPropertyType::ScrollbarWidth,
            CssPropertyType::ScrollbarVisibility,
            CssPropertyType::ScrollbarFadeDelay,
            CssPropertyType::ScrollbarFadeDuration,
        ] {
            let of_type: Vec<&CssPropertyWithConditions> = UA_SCROLLBAR_CSS
                .iter()
                .filter(|e| e.property.get_type() == ty)
                .collect();
            assert!(!of_type.is_empty(), "{ty:?} has no entry at all");

            let unconditional: Vec<usize> = of_type
                .iter()
                .enumerate()
                .filter(|(_, e)| e.apply_if.as_slice().is_empty())
                .map(|(i, _)| i)
                .collect();

            assert_eq!(
                unconditional.len(),
                1,
                "{ty:?} must have exactly one unconditional fallback, found {}",
                unconditional.len()
            );
            assert_eq!(
                unconditional[0],
                of_type.len() - 1,
                "{ty:?}: the unconditional fallback must come last, otherwise the \
                 {} rule(s) after it are dead under first-match-wins",
                of_type.len() - 1 - unconditional[0]
            );
        }
    }

    // ==================================================================
    // evaluate_ua_scrollbar_css
    // ==================================================================

    #[test]
    fn default_context_resolves_to_the_classic_light_scrollbar() {
        let r = evaluate_ua_scrollbar_css(&DynamicSelectorContext::default());
        assert_eq!(r.width, LayoutScrollbarWidth::Auto);
        assert_eq!(r.visibility, ScrollbarVisibilityMode::Always);
        assert_eq!(r.fade_delay.ms, 0);
        assert_eq!(r.fade_duration.ms, 0);
        assert_eq!(
            unwrap_custom(r.color),
            (CLASSIC_LIGHT_THUMB, CLASSIC_LIGHT_TRACK)
        );
    }

    #[test]
    fn per_os_and_theme_defaults_are_what_the_table_promises() {
        let cases: Vec<(
            OsCondition,
            ThemeCondition,
            LayoutScrollbarWidth,
            ScrollbarVisibilityMode,
            u32,
            u32,
            StyleScrollbarColor,
        )> = vec![
            (
                OsCondition::MacOS,
                ThemeCondition::Dark,
                LayoutScrollbarWidth::Thin,
                ScrollbarVisibilityMode::WhenScrolling,
                500,
                200,
                custom_color(
                    ColorU {
                        r: 180,
                        g: 180,
                        b: 180,
                        a: 200,
                    },
                    ColorU {
                        r: 40,
                        g: 40,
                        b: 40,
                        a: 80,
                    },
                ),
            ),
            (
                OsCondition::MacOS,
                ThemeCondition::Light,
                LayoutScrollbarWidth::Thin,
                ScrollbarVisibilityMode::WhenScrolling,
                500,
                200,
                custom_color(
                    ColorU {
                        r: 80,
                        g: 80,
                        b: 80,
                        a: 200,
                    },
                    ColorU {
                        r: 200,
                        g: 200,
                        b: 200,
                        a: 80,
                    },
                ),
            ),
            (
                OsCondition::Windows,
                ThemeCondition::Dark,
                LayoutScrollbarWidth::Auto,
                ScrollbarVisibilityMode::Always,
                0,
                0,
                custom_color(
                    ColorU {
                        r: 110,
                        g: 110,
                        b: 110,
                        a: 255,
                    },
                    ColorU {
                        r: 32,
                        g: 32,
                        b: 32,
                        a: 255,
                    },
                ),
            ),
            (
                OsCondition::Windows,
                ThemeCondition::Light,
                LayoutScrollbarWidth::Auto,
                ScrollbarVisibilityMode::Always,
                0,
                0,
                custom_color(
                    ColorU {
                        r: 130,
                        g: 130,
                        b: 130,
                        a: 255,
                    },
                    ColorU {
                        r: 241,
                        g: 241,
                        b: 241,
                        a: 255,
                    },
                ),
            ),
            (
                OsCondition::IOS,
                ThemeCondition::Dark,
                LayoutScrollbarWidth::Thin,
                ScrollbarVisibilityMode::WhenScrolling,
                500,
                200,
                custom_color(
                    ColorU {
                        r: 255,
                        g: 255,
                        b: 255,
                        a: 100,
                    },
                    ColorU::TRANSPARENT,
                ),
            ),
            (
                OsCondition::IOS,
                ThemeCondition::Light,
                LayoutScrollbarWidth::Thin,
                ScrollbarVisibilityMode::WhenScrolling,
                500,
                200,
                custom_color(
                    ColorU {
                        r: 0,
                        g: 0,
                        b: 0,
                        a: 100,
                    },
                    ColorU::TRANSPARENT,
                ),
            ),
            (
                OsCondition::Android,
                ThemeCondition::Dark,
                LayoutScrollbarWidth::Thin,
                ScrollbarVisibilityMode::WhenScrolling,
                300,
                150,
                custom_color(
                    ColorU {
                        r: 255,
                        g: 255,
                        b: 255,
                        a: 77,
                    },
                    ColorU::TRANSPARENT,
                ),
            ),
            (
                OsCondition::Android,
                ThemeCondition::Light,
                LayoutScrollbarWidth::Thin,
                ScrollbarVisibilityMode::WhenScrolling,
                300,
                150,
                custom_color(
                    ColorU {
                        r: 0,
                        g: 0,
                        b: 0,
                        a: 77,
                    },
                    ColorU::TRANSPARENT,
                ),
            ),
            (
                // Linux has no OS-specific colour rule: dark falls through to the
                // generic dark entry.
                OsCondition::Linux,
                ThemeCondition::Dark,
                LayoutScrollbarWidth::Auto,
                ScrollbarVisibilityMode::Always,
                0,
                0,
                custom_color(
                    ColorU {
                        r: 100,
                        g: 100,
                        b: 100,
                        a: 255,
                    },
                    ColorU {
                        r: 45,
                        g: 45,
                        b: 45,
                        a: 255,
                    },
                ),
            ),
            (
                OsCondition::Linux,
                ThemeCondition::Light,
                LayoutScrollbarWidth::Auto,
                ScrollbarVisibilityMode::Always,
                0,
                0,
                custom_color(CLASSIC_LIGHT_THUMB, CLASSIC_LIGHT_TRACK),
            ),
            (
                OsCondition::Web,
                ThemeCondition::Dark,
                LayoutScrollbarWidth::Auto,
                ScrollbarVisibilityMode::Always,
                0,
                0,
                custom_color(
                    ColorU {
                        r: 100,
                        g: 100,
                        b: 100,
                        a: 255,
                    },
                    ColorU {
                        r: 45,
                        g: 45,
                        b: 45,
                        a: 255,
                    },
                ),
            ),
        ];

        for (os, theme, width, visibility, delay, duration, color) in cases {
            let r = evaluate_ua_scrollbar_css(&ctx(os, theme.clone()));
            assert_eq!(r.width, width, "{os:?}/{theme:?}: width");
            assert_eq!(r.visibility, visibility, "{os:?}/{theme:?}: visibility");
            assert_eq!(r.fade_delay.ms, delay, "{os:?}/{theme:?}: fade-delay");
            assert_eq!(
                r.fade_duration.ms, duration,
                "{os:?}/{theme:?}: fade-duration"
            );
            assert_eq!(r.color, color, "{os:?}/{theme:?}: color");
        }
    }

    /// `match_theme` compares by equality (except when the *condition* is
    /// `SystemPreferred`), so a context theme of `Custom(..)` / `SystemPreferred`
    /// matches no `@theme` rule at all — every such context must still resolve a
    /// colour, via the unconditional fallback.
    #[test]
    fn unrecognised_context_themes_fall_back_instead_of_failing() {
        for theme in [
            ThemeCondition::Custom(AzString::from("")),
            ThemeCondition::Custom(AzString::from("🎨")),
            ThemeCondition::SystemPreferred,
        ] {
            // OS-conditioned properties still apply — only the theme rules miss.
            let r = evaluate_ua_scrollbar_css(&ctx(OsCondition::MacOS, theme.clone()));
            assert_eq!(r.width, LayoutScrollbarWidth::Thin, "{theme:?}");
            assert_eq!(
                r.visibility,
                ScrollbarVisibilityMode::WhenScrolling,
                "{theme:?}"
            );
            assert_eq!(
                unwrap_custom(r.color),
                (CLASSIC_LIGHT_THUMB, CLASSIC_LIGHT_TRACK),
                "{theme:?}: must fall back to the unconditional colour"
            );
        }
    }

    /// `OsCondition::Apple` is condition-side sugar (it *matches* MacOS/IOS); as a
    /// *context* value it equals neither, so an `Apple` context gets the generic
    /// defaults. `DynamicSelectorContext::from_system_style` never produces it, so
    /// this pins down the (slightly surprising) behaviour rather than blessing it.
    #[test]
    fn apple_as_a_context_os_matches_no_macos_or_ios_rule() {
        let r = evaluate_ua_scrollbar_css(&ctx(OsCondition::Apple, ThemeCondition::Dark));
        assert_eq!(r.width, LayoutScrollbarWidth::Auto);
        assert_eq!(r.visibility, ScrollbarVisibilityMode::Always);
        assert_eq!(r.fade_delay.ms, 0);
        assert_eq!(r.fade_duration.ms, 0);
    }

    /// Overlay scrollbars are a package deal: `thin` ⇔ `when-scrolling` ⇔ a
    /// non-zero fade delay ⇔ a non-zero fade duration. A per-OS rule added to one
    /// group but forgotten in another would produce an overlay scrollbar that
    /// never fades (or a classic one that does).
    #[test]
    fn overlay_scrollbar_fields_stay_consistent_across_every_os_and_theme() {
        for os in all_os() {
            for theme in all_themes() {
                let r = evaluate_ua_scrollbar_css(&ctx(os, theme.clone()));
                let thin = r.width == LayoutScrollbarWidth::Thin;
                let overlay = r.visibility == ScrollbarVisibilityMode::WhenScrolling;

                assert_eq!(
                    thin, overlay,
                    "{os:?}/{theme:?}: thin/when-scrolling disagree"
                );
                assert_eq!(
                    overlay,
                    r.fade_delay.ms > 0,
                    "{os:?}/{theme:?}: an overlay scrollbar needs a fade delay"
                );
                assert_eq!(
                    overlay,
                    r.fade_duration.ms > 0,
                    "{os:?}/{theme:?}: an overlay scrollbar needs a fade duration"
                );
                // The table only ever supplies Custom colours.
                assert!(
                    matches!(r.color, StyleScrollbarColor::Custom(_)),
                    "{os:?}/{theme:?}: colour resolved to Auto"
                );
            }
        }
    }

    /// The evaluator `break`s early once all five fields are filled. Cross-check
    /// it against a straight first-match-wins scan with no early exit: the two
    /// must agree for every context, or the optimisation changed the semantics.
    #[test]
    fn early_break_does_not_change_the_first_match_result() {
        for os in all_os() {
            for theme in all_themes() {
                let c = ctx(os, theme.clone());
                let got = evaluate_ua_scrollbar_css(&c);

                let mut want_color = None;
                let mut want_width = None;
                let mut want_vis = None;
                let mut want_delay = None;
                let mut want_dur = None;
                for entry in UA_SCROLLBAR_CSS.iter().filter(|e| e.matches(&c)) {
                    match &entry.property {
                        CssProperty::ScrollbarColor(CssPropertyValue::Exact(v)) => {
                            if want_color.is_none() {
                                want_color = Some(*v);
                            }
                        }
                        CssProperty::ScrollbarWidth(CssPropertyValue::Exact(v)) => {
                            if want_width.is_none() {
                                want_width = Some(*v);
                            }
                        }
                        CssProperty::ScrollbarVisibility(CssPropertyValue::Exact(v)) => {
                            if want_vis.is_none() {
                                want_vis = Some(*v);
                            }
                        }
                        CssProperty::ScrollbarFadeDelay(CssPropertyValue::Exact(v)) => {
                            if want_delay.is_none() {
                                want_delay = Some(*v);
                            }
                        }
                        CssProperty::ScrollbarFadeDuration(CssPropertyValue::Exact(v)) => {
                            if want_dur.is_none() {
                                want_dur = Some(*v);
                            }
                        }
                        _ => {}
                    }
                }

                let label = alloc::format!("{os:?}/{theme:?}");
                assert_eq!(Some(got.color), want_color, "{label}: color");
                assert_eq!(Some(got.width), want_width, "{label}: width");
                assert_eq!(Some(got.visibility), want_vis, "{label}: visibility");
                assert_eq!(Some(got.fade_delay), want_delay, "{label}: fade-delay");
                assert_eq!(Some(got.fade_duration), want_dur, "{label}: fade-duration");
            }
        }
    }

    /// Degenerate / hostile context values (NaN, infinities, empty and huge
    /// strings) must not panic, and every field must still resolve.
    #[test]
    fn degenerate_context_values_do_not_panic() {
        let hostile = [
            (f32::NAN, f32::NAN),
            (0.0, 0.0),
            (-0.0, -1.0),
            (f32::INFINITY, f32::NEG_INFINITY),
            (f32::MAX, f32::MIN),
            (f32::MIN_POSITIVE, f32::EPSILON),
        ];

        for (w, h) in hostile {
            let c = DynamicSelectorContext {
                os: OsCondition::MacOS,
                theme: ThemeCondition::Dark,
                de_version: u32::MAX,
                viewport_width: w,
                viewport_height: h,
                container_width: h,
                container_height: w,
                language: AzString::from(""),
                ..DynamicSelectorContext::default()
            };
            let r = evaluate_ua_scrollbar_css(&c);
            // macOS/dark rules are OS+theme-only, so viewport garbage cannot
            // perturb them.
            assert_eq!(r.width, LayoutScrollbarWidth::Thin, "viewport {w}x{h}");
            assert_eq!(r.fade_delay.ms, 500, "viewport {w}x{h}");
            assert!(
                matches!(r.color, StyleScrollbarColor::Custom(_)),
                "viewport {w}x{h}"
            );
        }
    }

    #[test]
    fn evaluate_is_deterministic() {
        for os in all_os() {
            for theme in all_themes() {
                let c = ctx(os, theme.clone());
                let a = evaluate_ua_scrollbar_css(&c);
                let b = evaluate_ua_scrollbar_css(&c);
                assert_eq!(a.color, b.color, "{os:?}/{theme:?}");
                assert_eq!(a.width, b.width, "{os:?}/{theme:?}");
                assert_eq!(a.visibility, b.visibility, "{os:?}/{theme:?}");
                assert_eq!(a.fade_delay, b.fade_delay, "{os:?}/{theme:?}");
                assert_eq!(a.fade_duration, b.fade_duration, "{os:?}/{theme:?}");
            }
        }
    }
}
