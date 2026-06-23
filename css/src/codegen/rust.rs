//! Rust source-code emitter for parsed CSS.
//!
//! Produces a `const CSS: Css = ...;` literal plus a minimal `Cargo.toml` and
//! `src/main.rs` skeleton suitable for `cargo build` against `azul`.

use alloc::{format, string::String, string::ToString, vec, vec::Vec};

use super::{CodegenBackend, GeneratedFile};
use crate::{
    css::{
        AttributeMatchOp, Css, CssAttributeSelector, CssDeclaration, CssNthChildPattern,
        CssNthChildSelector, CssPath, CssPathPseudoSelector, CssPathSelector, DynamicCssProperty,
        NodeTypeTag,
    },
    props::property::format_static_css_prop,
};

/// Emits Rust source code for a parsed CSS stylesheet.
pub struct RustBackend;

impl CodegenBackend for RustBackend {
    fn lang(&self) -> &'static str {
        "rust"
    }

    fn emit_css(&self, css: &Css) -> String {
        css_to_rust_code(css)
    }

    fn emit_project(&self, css: &Css) -> Vec<GeneratedFile> {
        let css_literal = css_to_rust_code(css);
        let main_rs = format!(
            "use azul::prelude::*;\r\n\r\n{css_literal}\r\n\r\nfn main() {{\r\n    \
             println!(\"Generated stylesheet contains {{}} rule(s)\", \
             CSS.rules.as_ref().len());\r\n}}\r\n",
        );
        let cargo_toml = "[package]\r\n\
            name = \"azul-generated-app\"\r\n\
            version = \"0.1.0\"\r\n\
            edition = \"2021\"\r\n\
            \r\n\
            [dependencies]\r\n\
            azul = \"0.0.7\"\r\n"
            .to_string();
        vec![
            GeneratedFile {
                path: "Cargo.toml".to_string(),
                contents: cargo_toml,
            },
            GeneratedFile {
                path: "src/main.rs".to_string(),
                contents: main_rs,
            },
        ]
    }
}

/// Render a parsed [`Css`] as Rust source code (a `const CSS: Css = ...;`).
#[must_use] pub fn css_to_rust_code(css: &Css) -> String {
    let mut output = String::new();

    output.push_str("const CSS: Css = Css {\r\n");
    output.push_str("\trules: [\r\n");

    for block in css.rules.iter() {
        output.push_str("\t\tCssRuleBlock {\r\n");
        output.push_str(&format!(
            "\t\t\tpath: {},\r\n",
            print_block_path(&block.path, 3)
        ));
        output.push_str(&format!(
            "\t\t\tpriority: {},\r\n",
            block.priority,
        ));

        output.push_str("\t\t\tdeclarations: [\r\n");
        for declaration in block.declarations.iter() {
            output.push_str(&format!(
                "\t\t\t\t{},\r\n",
                print_declaration(declaration, 4)
            ));
        }
        output.push_str("\t\t\t]\r\n");

        output.push_str("\t\t},\r\n");
    }

    output.push_str("\t]\r\n");
    output.push_str("};");

    output.replace('\t', "    ")
}

#[must_use] pub const fn format_node_type(n: &NodeTypeTag) -> &'static str {
    match n {
        // Document structure
        NodeTypeTag::Html => "NodeTypeTag::Html",
        NodeTypeTag::Head => "NodeTypeTag::Head",
        NodeTypeTag::Body => "NodeTypeTag::Body",

        // Block elements
        NodeTypeTag::Div => "NodeTypeTag::Div",
        NodeTypeTag::P => "NodeTypeTag::P",
        NodeTypeTag::Article => "NodeTypeTag::Article",
        NodeTypeTag::Section => "NodeTypeTag::Section",
        NodeTypeTag::Nav => "NodeTypeTag::Nav",
        NodeTypeTag::Aside => "NodeTypeTag::Aside",
        NodeTypeTag::Header => "NodeTypeTag::Header",
        NodeTypeTag::Footer => "NodeTypeTag::Footer",
        NodeTypeTag::Main => "NodeTypeTag::Main",
        NodeTypeTag::Figure => "NodeTypeTag::Figure",
        NodeTypeTag::FigCaption => "NodeTypeTag::FigCaption",

        // Headings
        NodeTypeTag::H1 => "NodeTypeTag::H1",
        NodeTypeTag::H2 => "NodeTypeTag::H2",
        NodeTypeTag::H3 => "NodeTypeTag::H3",
        NodeTypeTag::H4 => "NodeTypeTag::H4",
        NodeTypeTag::H5 => "NodeTypeTag::H5",
        NodeTypeTag::H6 => "NodeTypeTag::H6",

        // Text formatting
        NodeTypeTag::Br => "NodeTypeTag::Br",
        NodeTypeTag::Hr => "NodeTypeTag::Hr",
        NodeTypeTag::Pre => "NodeTypeTag::Pre",
        NodeTypeTag::BlockQuote => "NodeTypeTag::BlockQuote",
        NodeTypeTag::Address => "NodeTypeTag::Address",
        NodeTypeTag::Details => "NodeTypeTag::Details",
        NodeTypeTag::Summary => "NodeTypeTag::Summary",
        NodeTypeTag::Dialog => "NodeTypeTag::Dialog",

        // List elements
        NodeTypeTag::Ul => "NodeTypeTag::Ul",
        NodeTypeTag::Ol => "NodeTypeTag::Ol",
        NodeTypeTag::Li => "NodeTypeTag::Li",
        NodeTypeTag::Dl => "NodeTypeTag::Dl",
        NodeTypeTag::Dt => "NodeTypeTag::Dt",
        NodeTypeTag::Dd => "NodeTypeTag::Dd",
        NodeTypeTag::Menu => "NodeTypeTag::Menu",
        NodeTypeTag::MenuItem => "NodeTypeTag::MenuItem",
        NodeTypeTag::Dir => "NodeTypeTag::Dir",

        // Table elements
        NodeTypeTag::Table => "NodeTypeTag::Table",
        NodeTypeTag::Caption => "NodeTypeTag::Caption",
        NodeTypeTag::THead => "NodeTypeTag::THead",
        NodeTypeTag::TBody => "NodeTypeTag::TBody",
        NodeTypeTag::TFoot => "NodeTypeTag::TFoot",
        NodeTypeTag::Tr => "NodeTypeTag::Tr",
        NodeTypeTag::Th => "NodeTypeTag::Th",
        NodeTypeTag::Td => "NodeTypeTag::Td",
        NodeTypeTag::ColGroup => "NodeTypeTag::ColGroup",
        NodeTypeTag::Col => "NodeTypeTag::Col",

        // Form elements
        NodeTypeTag::Form => "NodeTypeTag::Form",
        NodeTypeTag::FieldSet => "NodeTypeTag::FieldSet",
        NodeTypeTag::Legend => "NodeTypeTag::Legend",
        NodeTypeTag::Label => "NodeTypeTag::Label",
        NodeTypeTag::Input => "NodeTypeTag::Input",
        NodeTypeTag::Button => "NodeTypeTag::Button",
        NodeTypeTag::Select => "NodeTypeTag::Select",
        NodeTypeTag::OptGroup => "NodeTypeTag::OptGroup",
        NodeTypeTag::SelectOption => "NodeTypeTag::SelectOption",
        NodeTypeTag::TextArea => "NodeTypeTag::TextArea",
        NodeTypeTag::Output => "NodeTypeTag::Output",
        NodeTypeTag::Progress => "NodeTypeTag::Progress",
        NodeTypeTag::Meter => "NodeTypeTag::Meter",
        NodeTypeTag::DataList => "NodeTypeTag::DataList",

        // Inline elements
        NodeTypeTag::Span => "NodeTypeTag::Span",
        NodeTypeTag::A => "NodeTypeTag::A",
        NodeTypeTag::Em => "NodeTypeTag::Em",
        NodeTypeTag::Strong => "NodeTypeTag::Strong",
        NodeTypeTag::B => "NodeTypeTag::B",
        NodeTypeTag::I => "NodeTypeTag::I",
        NodeTypeTag::U => "NodeTypeTag::U",
        NodeTypeTag::S => "NodeTypeTag::S",
        NodeTypeTag::Mark => "NodeTypeTag::Mark",
        NodeTypeTag::Del => "NodeTypeTag::Del",
        NodeTypeTag::Ins => "NodeTypeTag::Ins",
        NodeTypeTag::Code => "NodeTypeTag::Code",
        NodeTypeTag::Samp => "NodeTypeTag::Samp",
        NodeTypeTag::Kbd => "NodeTypeTag::Kbd",
        NodeTypeTag::Var => "NodeTypeTag::Var",
        NodeTypeTag::Cite => "NodeTypeTag::Cite",
        NodeTypeTag::Dfn => "NodeTypeTag::Dfn",
        NodeTypeTag::Abbr => "NodeTypeTag::Abbr",
        NodeTypeTag::Acronym => "NodeTypeTag::Acronym",
        NodeTypeTag::Q => "NodeTypeTag::Q",
        NodeTypeTag::Time => "NodeTypeTag::Time",
        NodeTypeTag::Sub => "NodeTypeTag::Sub",
        NodeTypeTag::Sup => "NodeTypeTag::Sup",
        NodeTypeTag::Small => "NodeTypeTag::Small",
        NodeTypeTag::Big => "NodeTypeTag::Big",
        NodeTypeTag::Bdo => "NodeTypeTag::Bdo",
        NodeTypeTag::Bdi => "NodeTypeTag::Bdi",
        NodeTypeTag::Wbr => "NodeTypeTag::Wbr",
        NodeTypeTag::Ruby => "NodeTypeTag::Ruby",
        NodeTypeTag::Rt => "NodeTypeTag::Rt",
        NodeTypeTag::Rtc => "NodeTypeTag::Rtc",
        NodeTypeTag::Rp => "NodeTypeTag::Rp",
        NodeTypeTag::Data => "NodeTypeTag::Data",

        // Embedded content
        NodeTypeTag::Canvas => "NodeTypeTag::Canvas",
        NodeTypeTag::Object => "NodeTypeTag::Object",
        NodeTypeTag::Param => "NodeTypeTag::Param",
        NodeTypeTag::Embed => "NodeTypeTag::Embed",
        NodeTypeTag::Audio => "NodeTypeTag::Audio",
        NodeTypeTag::Video => "NodeTypeTag::Video",
        NodeTypeTag::Source => "NodeTypeTag::Source",
        NodeTypeTag::Track => "NodeTypeTag::Track",
        NodeTypeTag::Map => "NodeTypeTag::Map",
        NodeTypeTag::Area => "NodeTypeTag::Area",
        NodeTypeTag::Svg => "NodeTypeTag::Svg",
        NodeTypeTag::SvgPath => "NodeTypeTag::SvgPath",
        NodeTypeTag::SvgCircle => "NodeTypeTag::SvgCircle",
        NodeTypeTag::SvgRect => "NodeTypeTag::SvgRect",
        NodeTypeTag::SvgEllipse => "NodeTypeTag::SvgEllipse",
        NodeTypeTag::SvgLine => "NodeTypeTag::SvgLine",
        NodeTypeTag::SvgPolygon => "NodeTypeTag::SvgPolygon",
        NodeTypeTag::SvgPolyline => "NodeTypeTag::SvgPolyline",
        NodeTypeTag::SvgG => "NodeTypeTag::SvgG",

        // SVG container elements
        NodeTypeTag::SvgDefs => "NodeTypeTag::SvgDefs",
        NodeTypeTag::SvgSymbol => "NodeTypeTag::SvgSymbol",
        NodeTypeTag::SvgUse => "NodeTypeTag::SvgUse",
        NodeTypeTag::SvgSwitch => "NodeTypeTag::SvgSwitch",

        // SVG text elements
        NodeTypeTag::SvgText => "NodeTypeTag::SvgText",
        NodeTypeTag::SvgTspan => "NodeTypeTag::SvgTspan",
        NodeTypeTag::SvgTextPath => "NodeTypeTag::SvgTextPath",

        // SVG paint server elements
        NodeTypeTag::SvgLinearGradient => "NodeTypeTag::SvgLinearGradient",
        NodeTypeTag::SvgRadialGradient => "NodeTypeTag::SvgRadialGradient",
        NodeTypeTag::SvgStop => "NodeTypeTag::SvgStop",
        NodeTypeTag::SvgPattern => "NodeTypeTag::SvgPattern",

        // SVG clipping/masking elements
        NodeTypeTag::SvgClipPathElement => "NodeTypeTag::SvgClipPathElement",
        NodeTypeTag::SvgMask => "NodeTypeTag::SvgMask",

        // SVG filter elements
        NodeTypeTag::SvgFilter => "NodeTypeTag::SvgFilter",
        NodeTypeTag::SvgFeBlend => "NodeTypeTag::SvgFeBlend",
        NodeTypeTag::SvgFeColorMatrix => "NodeTypeTag::SvgFeColorMatrix",
        NodeTypeTag::SvgFeComponentTransfer => "NodeTypeTag::SvgFeComponentTransfer",
        NodeTypeTag::SvgFeComposite => "NodeTypeTag::SvgFeComposite",
        NodeTypeTag::SvgFeConvolveMatrix => "NodeTypeTag::SvgFeConvolveMatrix",
        NodeTypeTag::SvgFeDiffuseLighting => "NodeTypeTag::SvgFeDiffuseLighting",
        NodeTypeTag::SvgFeDisplacementMap => "NodeTypeTag::SvgFeDisplacementMap",
        NodeTypeTag::SvgFeDistantLight => "NodeTypeTag::SvgFeDistantLight",
        NodeTypeTag::SvgFeDropShadow => "NodeTypeTag::SvgFeDropShadow",
        NodeTypeTag::SvgFeFlood => "NodeTypeTag::SvgFeFlood",
        NodeTypeTag::SvgFeFuncR => "NodeTypeTag::SvgFeFuncR",
        NodeTypeTag::SvgFeFuncG => "NodeTypeTag::SvgFeFuncG",
        NodeTypeTag::SvgFeFuncB => "NodeTypeTag::SvgFeFuncB",
        NodeTypeTag::SvgFeFuncA => "NodeTypeTag::SvgFeFuncA",
        NodeTypeTag::SvgFeGaussianBlur => "NodeTypeTag::SvgFeGaussianBlur",
        NodeTypeTag::SvgFeImage => "NodeTypeTag::SvgFeImage",
        NodeTypeTag::SvgFeMerge => "NodeTypeTag::SvgFeMerge",
        NodeTypeTag::SvgFeMergeNode => "NodeTypeTag::SvgFeMergeNode",
        NodeTypeTag::SvgFeMorphology => "NodeTypeTag::SvgFeMorphology",
        NodeTypeTag::SvgFeOffset => "NodeTypeTag::SvgFeOffset",
        NodeTypeTag::SvgFePointLight => "NodeTypeTag::SvgFePointLight",
        NodeTypeTag::SvgFeSpecularLighting => "NodeTypeTag::SvgFeSpecularLighting",
        NodeTypeTag::SvgFeSpotLight => "NodeTypeTag::SvgFeSpotLight",
        NodeTypeTag::SvgFeTile => "NodeTypeTag::SvgFeTile",
        NodeTypeTag::SvgFeTurbulence => "NodeTypeTag::SvgFeTurbulence",

        // SVG marker/image elements
        NodeTypeTag::SvgMarker => "NodeTypeTag::SvgMarker",
        NodeTypeTag::SvgImage => "NodeTypeTag::SvgImage",
        NodeTypeTag::SvgForeignObject => "NodeTypeTag::SvgForeignObject",

        // SVG descriptive elements
        NodeTypeTag::SvgTitle => "NodeTypeTag::SvgTitle",
        NodeTypeTag::SvgDesc => "NodeTypeTag::SvgDesc",
        NodeTypeTag::SvgMetadata => "NodeTypeTag::SvgMetadata",
        NodeTypeTag::SvgA => "NodeTypeTag::SvgA",
        NodeTypeTag::SvgView => "NodeTypeTag::SvgView",
        NodeTypeTag::SvgStyle => "NodeTypeTag::SvgStyle",
        NodeTypeTag::SvgScript => "NodeTypeTag::SvgScript",

        // SVG animation elements
        NodeTypeTag::SvgAnimate => "NodeTypeTag::SvgAnimate",
        NodeTypeTag::SvgAnimateMotion => "NodeTypeTag::SvgAnimateMotion",
        NodeTypeTag::SvgAnimateTransform => "NodeTypeTag::SvgAnimateTransform",
        NodeTypeTag::SvgSet => "NodeTypeTag::SvgSet",
        NodeTypeTag::SvgMpath => "NodeTypeTag::SvgMpath",

        // Metadata
        NodeTypeTag::Title => "NodeTypeTag::Title",
        NodeTypeTag::Meta => "NodeTypeTag::Meta",
        NodeTypeTag::Link => "NodeTypeTag::Link",
        NodeTypeTag::Script => "NodeTypeTag::Script",
        NodeTypeTag::Style => "NodeTypeTag::Style",
        NodeTypeTag::Base => "NodeTypeTag::Base",

        // Content elements
        NodeTypeTag::Text => "NodeTypeTag::Text",
        NodeTypeTag::Img => "NodeTypeTag::Img",
        NodeTypeTag::VirtualView => "NodeTypeTag::VirtualView",
        NodeTypeTag::Icon => "NodeTypeTag::Icon",
        NodeTypeTag::GeolocationProbe => "NodeTypeTag::GeolocationProbe",

        // Pseudo-elements
        NodeTypeTag::Before => "NodeTypeTag::Before",
        NodeTypeTag::After => "NodeTypeTag::After",
        NodeTypeTag::Marker => "NodeTypeTag::Marker",
        NodeTypeTag::Placeholder => "NodeTypeTag::Placeholder",
    }
}

#[must_use] pub fn print_block_path(path: &CssPath, tabs: usize) -> String {
    let t = String::from("    ").repeat(tabs);
    let t1 = String::from("    ").repeat(tabs + 1);

    format!(
        "CssPath {{\r\n{}selectors: {}\r\n{}}}",
        t1,
        format_selectors(path.selectors.as_ref(), tabs + 1),
        t
    )
}

#[must_use] pub fn format_selectors(selectors: &[CssPathSelector], tabs: usize) -> String {
    let t = String::from("    ").repeat(tabs);
    let t1 = String::from("    ").repeat(tabs + 1);

    let selectors_formatted = selectors
        .iter()
        .map(|s| format!("{}{},", t1, format_single_selector(s, tabs + 1)))
        .collect::<Vec<String>>()
        .join("\r\n");

    format!("vec![\r\n{selectors_formatted}\r\n{t}].into()")
}

#[must_use] pub fn format_single_selector(p: &CssPathSelector, _tabs: usize) -> String {
    match p {
        CssPathSelector::Global => "CssPathSelector::Global".to_string(),
        CssPathSelector::Root(r) => format!(
            "CssPathSelector::Root(CssScopeRange {{ start: {}, end: {} }})",
            r.start, r.end
        ),
        CssPathSelector::Type(ntp) => format!("CssPathSelector::Type({})", format_node_type(ntp)),
        CssPathSelector::Class(class) => {
            format!("CssPathSelector::Class(String::from({class:?}))")
        }
        CssPathSelector::Id(id) => format!("CssPathSelector::Id(String::from({id:?}))"),
        CssPathSelector::PseudoSelector(cps) => format!(
            "CssPathSelector::PseudoSelector({})",
            format_pseudo_selector_type(cps)
        ),
        CssPathSelector::Attribute(a) => format!(
            "CssPathSelector::Attribute({})",
            format_attribute_selector(a)
        ),
        CssPathSelector::DirectChildren => "CssPathSelector::DirectChildren".to_string(),
        CssPathSelector::Children => "CssPathSelector::Children".to_string(),
        CssPathSelector::AdjacentSibling => "CssPathSelector::AdjacentSibling".to_string(),
        CssPathSelector::GeneralSibling => "CssPathSelector::GeneralSibling".to_string(),
    }
}

#[must_use] pub fn format_pseudo_selector_type(p: &CssPathPseudoSelector) -> String {
    match p {
        CssPathPseudoSelector::First => "CssPathPseudoSelector::First".to_string(),
        CssPathPseudoSelector::Last => "CssPathPseudoSelector::Last".to_string(),
        CssPathPseudoSelector::NthChild(n) => format!(
            "CssPathPseudoSelector::NthChild({})",
            format_nth_child_selector(n)
        ),
        CssPathPseudoSelector::Hover => "CssPathPseudoSelector::Hover".to_string(),
        CssPathPseudoSelector::Active => "CssPathPseudoSelector::Active".to_string(),
        CssPathPseudoSelector::Focus => "CssPathPseudoSelector::Focus".to_string(),
        CssPathPseudoSelector::Backdrop => "CssPathPseudoSelector::Backdrop".to_string(),
        CssPathPseudoSelector::Lang(lang) => format!(
            "CssPathPseudoSelector::Lang(AzString::from_const_str(\"{}\"))",
            lang.as_str()
        ),
        CssPathPseudoSelector::Dragging => "CssPathPseudoSelector::Dragging".to_string(),
        CssPathPseudoSelector::DragOver => "CssPathPseudoSelector::DragOver".to_string(),
    }
}

#[must_use] pub fn format_attribute_selector(a: &CssAttributeSelector) -> String {
    let value = match a.value.as_ref() {
        Some(v) => format!(
            "OptionString::Some(AzString::from_const_str({:?}))",
            v.as_str()
        ),
        None => "OptionString::None".to_string(),
    };
    format!(
        "CssAttributeSelector {{ name: AzString::from_const_str({:?}), op: {}, value: {} }}",
        a.name.as_str(),
        format_attribute_match_op(&a.op),
        value
    )
}

#[must_use] pub fn format_attribute_match_op(op: &AttributeMatchOp) -> String {
    match op {
        AttributeMatchOp::Exists => "AttributeMatchOp::Exists".to_string(),
        AttributeMatchOp::Eq => "AttributeMatchOp::Eq".to_string(),
        AttributeMatchOp::Includes => "AttributeMatchOp::Includes".to_string(),
        AttributeMatchOp::DashMatch => "AttributeMatchOp::DashMatch".to_string(),
        AttributeMatchOp::Prefix => "AttributeMatchOp::Prefix".to_string(),
        AttributeMatchOp::Suffix => "AttributeMatchOp::Suffix".to_string(),
        AttributeMatchOp::Substring => "AttributeMatchOp::Substring".to_string(),
    }
}

#[must_use] pub fn format_nth_child_selector(n: &CssNthChildSelector) -> String {
    match n {
        CssNthChildSelector::Number(num) => format!("CssNthChildSelector::Number({num})"),
        CssNthChildSelector::Even => "CssNthChildSelector::Even".to_string(),
        CssNthChildSelector::Odd => "CssNthChildSelector::Odd".to_string(),
        CssNthChildSelector::Pattern(CssNthChildPattern {
            pattern_repeat,
            offset,
        }) => format!(
            "CssNthChildSelector::Pattern(CssNthChildPattern {{ pattern_repeat: {pattern_repeat}, offset: {offset} }})"
        ),
    }
}

#[must_use] pub fn print_declaration(decl: &CssDeclaration, tabs: usize) -> String {
    match decl {
        CssDeclaration::Static(s) => format!(
            "CssDeclaration::Static({})",
            format_static_css_prop(s, tabs)
        ),
        CssDeclaration::Dynamic(d) => format!(
            "CssDeclaration::Dynamic({})",
            format_dynamic_css_prop(d, tabs)
        ),
    }
}

#[must_use] pub fn format_dynamic_css_prop(decl: &DynamicCssProperty, tabs: usize) -> String {
    let t = String::from("    ").repeat(tabs);
    format!(
        "DynamicCssProperty {{\r\n{}    dynamic_id: {:?},\r\n{}    default_value: {},\r\n{}}}",
        t,
        decl.dynamic_id,
        t,
        format_static_css_prop(&decl.default_value, tabs + 1),
        t
    )
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use super::*;
    use crate::css::CssRuleBlock;

    fn sample_css() -> Css {
        let path = CssPath::new(vec![CssPathSelector::Type(NodeTypeTag::Div)]);
        let block = CssRuleBlock::new(path, vec![]);
        Css {
            rules: vec![block].into(),
        }
    }

    #[test]
    fn rust_backend_emits_const_literal() {
        let css = sample_css();
        let rust = RustBackend.emit_css(&css);
        assert!(rust.contains("const CSS: Css"));
        assert!(rust.contains("NodeTypeTag::Div"));
    }

    #[test]
    fn rust_backend_emits_project_files() {
        let css = sample_css();
        let files = RustBackend.emit_project(&css);
        let paths: Vec<_> = files.iter().map(|f| f.path.as_str()).collect();
        assert!(paths.contains(&"Cargo.toml"));
        assert!(paths.contains(&"src/main.rs"));
        let main_rs = files
            .iter()
            .find(|f| f.path == "src/main.rs")
            .expect("main.rs missing");
        assert!(main_rs.contains_const_literal());
    }

    impl GeneratedFile {
        fn contains_const_literal(&self) -> bool {
            self.contents.contains("const CSS: Css")
        }
    }
}
