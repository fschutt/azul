//! The shared stylesheets every code-generation test prints (one set, all
//! languages). Included by `codegen_goldens.rs` and `codegen_structure.rs`
//! via `mod codegen_cases;`.

#![allow(dead_code)]

/// The smallest case - the one the golden files were first written for by
/// hand: an api.json constructor (`width`), a struct literal (`color`), a
/// C-like enum (`display`), `FloatValue` sugar (`flex-grow`), a CSS-wide
/// keyword (`inherit`), a property WITHOUT an api.json constructor
/// (`white-space` -> `CssProperty::WhiteSpace(StyleWhiteSpaceValue::Exact(..))`)
/// and a `:hover` state merged into the same style.
pub const BASIC: &str = r"
.btn {
    width: 100px;
    color: #ff0000;
    display: flex;
    flex-grow: 1;
    min-width: inherit;
    white-space: nowrap;
}
.btn:hover {
    font-weight: bold;
}
";

/// Every property family at least once (checked by
/// `codegen_structure::every_css_property_is_in_the_families_case`).
pub const FAMILIES: &str = r#"
.text {
    color: #336699;
    font-size: 14px;
    font-family: Arial, sans-serif;
    font-weight: 600;
    font-style: italic;
    text-align: center;
    text-justify: inter-word;
    vertical-align: middle;
    letter-spacing: 0.5px;
    text-indent: 2em;
    initial-letter: 3;
    line-clamp: 3;
    hanging-punctuation: first;
    text-combine-upright: digits 2;
    unicode-bidi: isolate;
    text-box-trim: trim-both;
    text-box-edge: cap alphabetic;
    dominant-baseline: central;
    alignment-baseline: middle;
    baseline-source: last;
    line-fit-edge: leading;
    initial-letter-align: alphabetic;
    initial-letter-wrap: first;
    line-height: 1.5;
    word-spacing: 4px;
    tab-size: 4;
    white-space: pre-wrap;
    hyphens: auto;
    word-break: break-all;
    overflow-wrap: anywhere;
    line-break: strict;
    text-overflow: ellipsis;
    text-orientation: upright;
    text-align-last: justify;
    text-transform: uppercase;
    direction: rtl;
    user-select: none;
    text-decoration: underline;
    -azul-hyphenation-language: en-US;
    -azul-exclusion-margin: 10.5;
    caret-color: #ff0000;
    caret-animation-duration: 500ms;
    -azul-caret-width: 2px;
    -azul-selection-background-color: #3399ff;
    -azul-selection-color: #ffffff;
    -azul-selection-radius: 3px;
    font: Georgia, serif;
}
.box {
    display: block;
    float: left;
    box-sizing: border-box;
    width: 50%;
    height: 200px;
    min-width: 10em;
    min-height: 1rem;
    max-width: 800px;
    max-height: 90vh;
    position: absolute;
    top: 0px;
    right: 10px;
    bottom: 5pt;
    left: 1in;
    z-index: 10;
    padding-top: 1px;
    padding-right: 2px;
    padding-bottom: 3px;
    padding-left: 4px;
    padding-inline-start: 5px;
    padding-inline-end: 6px;
    margin-top: 7px;
    margin-right: 8px;
    margin-bottom: 9px;
    margin-left: auto;
    overflow-x: hidden;
    overflow-y: scroll;
    overflow-block: clip;
    overflow-inline: auto;
    scrollbar-gutter: stable both-edges;
    overflow-clip-margin: content-box;
    clip: rect(0px, 10px, 10px, 0px);
    writing-mode: vertical-rl;
    clear: both;
    visibility: hidden;
    opacity: 0.5;
    cursor: pointer;
    object-fit: cover;
    object-position: center top;
    aspect-ratio: 16 / 9;
}
.flex {
    flex-wrap: wrap;
    flex-direction: column;
    flex-grow: 2;
    flex-shrink: 0.5;
    flex-basis: 30%;
    justify-content: space-between;
    align-items: center;
    align-content: stretch;
    align-self: flex-end;
    column-gap: 8px;
    row-gap: 4px;
    gap: 6px;
}
.grid {
    grid-template-columns: 1fr 200px auto;
    grid-template-rows: 100px 1fr;
    grid-auto-columns: 50px;
    grid-auto-rows: minmax(10px, 1fr);
    grid-column: 1 / 3;
    grid-row: span 2;
    grid-template-areas: "header header" "sidebar main";
    grid-auto-flow: column;
    justify-self: center;
    justify-items: start;
    grid-gap: 10px;
}
.borders {
    border-top-left-radius: 4px;
    border-top-right-radius: 5px;
    border-bottom-left-radius: 6px;
    border-bottom-right-radius: 7px;
    border-top-color: #111111;
    border-right-color: #222222;
    border-bottom-color: #333333;
    border-left-color: #444444;
    border-top-style: solid;
    border-right-style: dashed;
    border-bottom-style: dotted;
    border-left-style: double;
    border-top-width: 1px;
    border-right-width: 2px;
    border-bottom-width: 3px;
    border-left-width: 4px;
    box-shadow: 0px 2px 4px 1px #00000040;
}
.scroll {
    -azul-scrollbar-track: #eeeeee;
    -azul-scrollbar-thumb: #888888;
    -azul-scrollbar-button: #cccccc;
    -azul-scrollbar-corner: #dddddd;
    -azul-scrollbar-resizer: #bbbbbb;
    scrollbar-width: thin;
    scrollbar-color: #888888 #eeeeee;
    overscroll-behavior-x: contain;
    overscroll-behavior-y: none;
    -azul-scrollbar-visibility: when-scrolling;
    -azul-scrollbar-fade-delay: 500ms;
    -azul-scrollbar-fade-duration: 200ms;
    -azul-app-region: drag;
    spatial-navigation-action: focus;
    spatial-navigation-contain: contain;
    spatial-navigation-function: grid;
}
.effects {
    background: #fafafa;
    background-position: center;
    background-size: cover;
    background-repeat: no-repeat;
    transform: rotate(45deg);
    transform-origin: 50% 50%;
    perspective-origin: 10px 20px;
    backface-visibility: hidden;
    filter: blur(2px);
    backdrop-filter: grayscale(50%);
    mix-blend-mode: multiply;
    text-shadow: 1px 1px 2px #000000;
}
.fragment {
    break-before: page;
    break-after: avoid;
    break-inside: avoid;
    orphans: 2;
    widows: 3;
    box-decoration-break: clone;
    column-count: 3;
    column-width: 200px;
    column-span: all;
    column-fill: balance;
    column-rule-width: 1px;
    column-rule-style: solid;
    column-rule-color: #cccccc;
    flow-into: article;
    flow-from: article;
}
.shape {
    shape-outside: circle(50px);
    shape-inside: circle(100px at 50px 50px);
    clip-path: circle(40px);
    shape-margin: 10px;
    shape-image-threshold: 0.5;
}
.table {
    table-layout: fixed;
    border-collapse: collapse;
    border-spacing: 2px 4px;
    caption-side: bottom;
    empty-cells: hide;
}
.content {
    content: "Hello";
    counter-reset: section 1;
    counter-increment: section;
    list-style-type: upper-roman;
    list-style-position: inside;
    string-set: title "Chapter";
}
.anim {
    animation: fadeIn 300ms ease-in-out;
    -azul-animation-in: flyInLeft 500ms spring;
    -azul-animation-out: fadeOut 200ms linear;
}
"#;

/// Pseudo-states and every kind of @-rule condition, merged per base
/// selector; structural pseudo-classes stay part of the selector.
pub const CONDITIONS: &str = r"
.card { padding-top: 8px; }
.card:hover { padding-top: 9px; }
.card:active { padding-top: 10px; }
.card:focus { border-top-color: #3399ff; }
@media (min-width: 600px) { .card { padding-left: 16px; } }
@media screen and (min-width: 1024px) { .card { padding-left: 24px; } }
@os macos { .card { font-size: 13px; } }
@os windows { .card { font-size: 12px; } }
@os linux { .card { font-size: 11px; } }
@os android { .card { font-size: 15px; } }
@theme(dark) { .card { color: #ffffff; } }
@theme(dark) { .card:hover { color: #cccccc; } }
@lang(de-DE) { .card { letter-spacing: 1px; } }
.list li:nth-child(2) { color: #999999; }
#main > .title { font-weight: bold; }
div .note:first { font-style: italic; }
";

/// `var()` (resolved by the parser to the custom property or its fallback)
/// and `env()` (kept as a runtime reference; a flat style holds its fallback).
pub const ENV: &str = r"
.bar {
    --accent: #ff6600;
    color: var(--accent, #000000);
    border-top-color: var(--missing, #cccccc);
    padding-top: env(safe-area-inset-top, 8px);
    padding-bottom: env(safe-area-inset-bottom);
}
";

/// Gradients, shadows, transforms, filters, images and string escaping.
pub const PAINT: &str = r#"
.hero {
    background: linear-gradient(135deg, #ff0000 0%, #00ff00 50%, #0000ff 100%);
    box-shadow: 0px 4px 12px 2px rgba(0, 0, 0, 0.3);
    transform: translate(10px, 20px) rotate(45deg) scale(1.5, 1.5) skew(10deg, 5deg);
    filter: blur(4px) grayscale(50%) drop-shadow(2px 2px 4px #000000);
    backdrop-filter: brightness(120%) contrast(80%);
}
.halo { background: radial-gradient(circle, #ffffff 0%, #000000 100%); }
.dial { background: conic-gradient(from 90deg, #ff0000, #0000ff); }
.photo {
    background: url("images/photo.png");
    background-size: cover;
    background-position: center center;
    background-repeat: no-repeat;
}
.caption {
    font-family: "Fira Code", monospace;
    content: "say \"hi\" \\ bye";
}
"#;

/// A real widget style, lifted from `layout/src/widgets/themes/flat.rs`
/// (`button_states` for a neutral button: hover / active fills, the hover
/// border, the focus ring - light and `@theme(dark)`).
pub const WIDGET: &str = r"
.__azul-native-button:hover {
    background: #f1f3f5;
    border-top-color: #adb5bd;
    border-bottom-color: #adb5bd;
    border-left-color: #adb5bd;
    border-right-color: #adb5bd;
}
.__azul-native-button:active { background: #dee2e6; }
.__azul-native-button:focus {
    border-top-color: #0d6efd;
    border-bottom-color: #0d6efd;
    border-left-color: #0d6efd;
    border-right-color: #0d6efd;
}
@theme(dark) {
    .__azul-native-button:hover { background: #495057; border-top-color: #495057; }
    .__azul-native-button:active { background: #2b3035; }
    .__azul-native-button:focus { border-top-color: #3b82f6; }
}
";

pub const CASES: &[(&str, &str)] = &[
    ("basic", BASIC),
    ("families", FAMILIES),
    ("conditions", CONDITIONS),
    ("env", ENV),
    ("paint", PAINT),
    ("widget", WIDGET),
];

/// `revert` / `unset` never come out of the parser, but a widget's property
/// list can hold them (they need the shadowed-variant path); `text-shadow`
/// holds a `BoxOrStatic` the bindings cannot build (dropped with a note).
pub fn keyword_list() -> Vec<azul_css::dynamic_selector::CssPropertyWithConditions> {
    use azul_css::{
        css::CssPropertyValue, dynamic_selector::CssPropertyWithConditions,
        props::property::CssProperty,
    };
    let shadow = {
        let css = azul_css::parser2::new_from_str(".x { text-shadow: 1px 1px 2px #000000; }").0;
        match &css.rules.as_slice()[0].declarations.as_slice()[0] {
            azul_css::css::CssDeclaration::Static(p) => p.clone(),
            azul_css::css::CssDeclaration::Dynamic(d) => d.default_value.clone(),
            azul_css::css::CssDeclaration::CustomProperty(_) => {
                unreachable!("text-shadow is not a custom property")
            }
        }
    };
    vec![
        CssPropertyWithConditions::simple(CssProperty::Width(CssPropertyValue::Revert)),
        CssPropertyWithConditions::simple(CssProperty::CaretWidth(CssPropertyValue::Unset)),
        CssPropertyWithConditions::simple(CssProperty::Height(CssPropertyValue::Auto)),
        CssPropertyWithConditions::simple(CssProperty::TextShadow(CssPropertyValue::None)),
        CssPropertyWithConditions::on_hover(shadow),
    ]
}

/// [`keyword_list`] lowered as one style named `keywords`.
pub fn keyword_module() -> azul_css::codegen::ir::Module {
    azul_css::codegen::lower::lower_property_list("keywords", &keyword_list())
}

// ── DOM export (AzBuilder "Subtree -> code" / "Component -> code") ──
//
// What `azul_core::xml::lower_xml_fragment` produces for a converted
// component's template
//
//     <div class="card" style="padding: 8px">
//       <h2>{title}</h2><p>{text}</p>
//       <a href="{href}">Read more</a><span>by {author}</span>
//     </div>
//
// (core's own tests pin that lowering; these pin the printers).

/// The card template as one item `render_card(title, text, href, author)`:
/// builder methods, parameters, a joined string, an accessible link.
pub fn dom_card_module() -> azul_css::codegen::ir::Module {
    use azul_css::codegen::ir::{Expr, Ident, Item, ItemParam, Module};
    let s = Expr::str;
    let p = Expr::param;
    let dom = |m: &str, args: Vec<Expr>| Expr::call("Dom", m, args);
    let with = |recv: Expr, m: &str, args: Vec<Expr>| Expr::method(recv, "Dom", m, args);
    let mut value = dom("create_div", vec![]);
    value = with(value, "with_css", vec![s("padding: 8px")]);
    value = with(value, "with_class", vec![s("card")]);
    value = with(value, "with_child", vec![dom("create_h2_with_text", vec![p("title")])]);
    value = with(value, "with_child", vec![dom("create_p_with_text", vec![p("text")])]);
    value = with(
        value,
        "with_child",
        vec![dom(
            "create_a",
            vec![
                p("href"),
                s("Read more"),
                Expr::call("SmallAriaInfo", "label", vec![s("Read more")]),
            ],
        )],
    );
    value = with(
        value,
        "with_child",
        vec![dom(
            "create_span_with_text",
            vec![Expr::concat(vec![s("by "), p("author")])],
        )],
    );
    Module {
        items: vec![Item {
            name: Ident::from_text("render_card"),
            doc: vec!["`user:card`: its texts and its link are parameters".to_string()],
            ty: "Dom".to_string(),
            params: vec![
                ItemParam::string("title", "Hello"),
                ItemParam::string("text", "Some text"),
                ItemParam::string("href", "https://azul.rs"),
                ItemParam::string("author", "me"),
            ],
            value,
        }],
        ..Module::default()
    }
}

/// [`dom_card_module`] as the component library `user` (the printers that
/// can spell it append the registration).
pub fn dom_library_module() -> azul_css::codegen::ir::Module {
    use azul_css::codegen::ir::{ComponentSpec, Ident, LibrarySpec};
    let mut m = dom_card_module();
    m.library = Some(LibrarySpec {
        name: "user".to_string(),
        version: "0.1.0".to_string(),
        components: vec![ComponentSpec {
            item: Ident::from_text("render_card"),
            name: "card".to_string(),
            display_name: "Card".to_string(),
            description: "Converted from a <div> subtree in AzBuilder".to_string(),
            data_model: "CardData".to_string(),
            data_model_description: "Converted from a <div> subtree in AzBuilder".to_string(),
            field_descriptions: vec![
                "Text of the <h2>".to_string(),
                "Text of the <p>".to_string(),
                "`href` of the <a>".to_string(),
                "Text of the <span>".to_string(),
            ],
        }],
    });
    m
}

/// A page body (`<body><h1>My App</h1><p>Hello</p></body>`) as an app.
pub fn dom_app_module() -> azul_css::codegen::ir::Module {
    use azul_css::codegen::ir::{AppSpec, Expr, Ident, Item, Module};
    let dom = |m: &str, args: Vec<Expr>| Expr::call("Dom", m, args);
    let with = |recv: Expr, m: &str, args: Vec<Expr>| Expr::method(recv, "Dom", m, args);
    let value = with(
        with(
            dom("create_body", vec![]),
            "with_child",
            vec![dom("create_h1_with_text", vec![Expr::str("My App")])],
        ),
        "with_child",
        vec![dom("create_p_with_text", vec![Expr::str("Hello")])],
    );
    Module {
        items: vec![Item {
            name: Ident::from_text("render_ui"),
            doc: Vec::new(),
            ty: "Dom".to_string(),
            params: Vec::new(),
            value,
        }],
        app: Some(AppSpec {
            title: "My App".to_string(),
            root: Ident::from_text("render_ui"),
            is_body: true,
        }),
        library: None,
    }
}

/// Component boundaries: what the component-aware lowering
/// (`azul_core::codegen::dom::lower_components_fragment`) produces for
///
///     <body><user:card title="Hi" tag="Beta"/><widgets:button label="OK"/></body>
///
/// where `user:card` is `<div class="card"><h2>{title}</h2><user:badge text="{tag}"/></div>`,
/// `user:badge` is `<span class="badge">{text}</span>` and `widgets:button`
/// is the widget `Button::create(label).dom()`: one function per component,
/// callees first, the page calls them (the card passes its own `tag` on),
/// nothing inlined.
pub fn dom_components_module() -> azul_css::codegen::ir::Module {
    use azul_css::codegen::ir::{Expr, Ident, Item, ItemParam, Module};
    let s = Expr::str;
    let p = Expr::param;
    let dom = |m: &str, args: Vec<Expr>| Expr::call("Dom", m, args);
    let with = |recv: Expr, m: &str, args: Vec<Expr>| Expr::method(recv, "Dom", m, args);
    let badge = with(
        dom("create_span_with_text", vec![p("text")]),
        "with_class",
        vec![s("badge")],
    );
    let card = with(
        with(
            with(dom("create_div", vec![]), "with_class", vec![s("card")]),
            "with_child",
            vec![dom("create_h2_with_text", vec![p("title")])],
        ),
        "with_child",
        vec![Expr::item_call("render_badge", vec![p("tag")])],
    );
    let ui = with(
        with(
            dom("create_body", vec![]),
            "with_child",
            vec![Expr::item_call("render_card", vec![s("Hi"), s("Beta")])],
        ),
        "with_child",
        vec![Expr::method(
            Expr::call("Button", "create", vec![s("OK")]),
            "Button",
            "dom",
            vec![],
        )],
    );
    Module {
        items: vec![
            Item {
                name: Ident::from_text("render_badge"),
                doc: vec!["`user:badge` (Badge)".to_string()],
                ty: "Dom".to_string(),
                params: vec![ItemParam::string("text", "New")],
                value: badge,
            },
            Item {
                name: Ident::from_text("render_card"),
                doc: vec!["`user:card` (Card)".to_string()],
                ty: "Dom".to_string(),
                params: vec![
                    ItemParam::string("title", "Hello"),
                    ItemParam::string("tag", "New"),
                ],
                value: card,
            },
            Item {
                name: Ident::from_text("render_ui"),
                doc: Vec::new(),
                ty: "Dom".to_string(),
                params: Vec::new(),
                value: ui,
            },
        ],
        ..Module::default()
    }
}
