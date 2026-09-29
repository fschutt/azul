//! Structural tests of the CSS code generators (`--features codegen,parser`):
//! the lowering is total and covers every property, every C-ABI name the IR
//! produces exists in the real generated `azul.h`, and every language's
//! output is balanced, ASCII and free of `new` constructors.

use std::{collections::BTreeSet, fs, path::PathBuf};

use azul_css::{
    codegen::{
        all_backends,
        ir::{snake_to_lower_camel, EnumShape, Expr, Module},
        lang::variant_ctor_method,
        lower::{lower_property_list, lower_styles, lower_stylesheet},
    },
    css::{Css, CssDeclaration},
    props::property::{get_css_key_map, CssPropertyType},
};

mod codegen_cases;

use codegen_cases::{CASES, FAMILIES};

fn parse(css: &str) -> Css {
    azul_css::parser2::new_from_str(css).0
}

fn property_types(css: &Css) -> BTreeSet<CssPropertyType> {
    let mut out = BTreeSet::new();
    for rule in css.rules.as_slice() {
        for decl in rule.declarations.as_slice() {
            match decl {
                CssDeclaration::Static(p) => out.insert(p.get_type()),
                CssDeclaration::Dynamic(d) => out.insert(d.default_value.get_type()),
            };
        }
    }
    out
}

#[test]
fn every_css_property_is_in_the_families_case() {
    // A longhand spelled like a shorthand (`gap`, `grid-gap`) cannot come
    // from CSS text - the shorthand parse wins and expands into its
    // longhands. The lowering still covers it (its match is exhaustive).
    let key_map = get_css_key_map();
    let all: BTreeSet<CssPropertyType> = key_map
        .non_shorthands
        .iter()
        .filter(|(name, _)| !key_map.shorthands.contains_key(*name))
        .map(|(_, ty)| *ty)
        .collect();
    let mut seen = BTreeSet::new();
    for (_, css) in CASES {
        seen.extend(property_types(&parse(css)));
    }
    let missing: Vec<&str> = all.difference(&seen).map(CssPropertyType::to_str).collect();
    assert!(
        missing.is_empty(),
        "the shared codegen stylesheets never produce these properties (a value in \
         tests/codegen_cases did not parse, or the case lacks it): {missing:?}"
    );
    // the families case alone must reach most of them
    assert!(property_types(&parse(FAMILIES)).len() > 150);
}

/// The only things the bindings cannot express.
const KNOWN_UNSUPPORTED: &[&str] = &["BoxOrStatic", "minmax()", "FontRef"];

fn unsupported_in(m: &Module) -> Vec<String> {
    let mut out = Vec::new();
    for item in &m.items {
        item.value.unsupported_reasons(&mut out);
    }
    out
}

#[test]
fn the_lowering_is_total_and_only_drops_what_the_bindings_cannot_express() {
    for (case, css) in CASES {
        let css = parse(css);
        for m in [lower_styles(&css), lower_stylesheet(&css)] {
            assert!(!m.items.is_empty(), "{case}: nothing lowered");
            for reason in unsupported_in(&m) {
                assert!(
                    KNOWN_UNSUPPORTED.iter().any(|k| reason.contains(k)),
                    "{case}: unexpected unsupported value: {reason}"
                );
            }
        }
    }
}

#[test]
fn styles_merge_pseudo_states_and_conditions_into_one_list_per_selector() {
    let m = lower_styles(&parse(codegen_cases::BASIC));
    assert_eq!(m.items.len(), 1);
    assert_eq!(m.items[0].name.snake(), "style_btn");
    assert_eq!(m.items[0].doc[0], "CSS: .btn, .btn:hover");
    let Expr::Vec { items, .. } = &m.items[0].value else {
        panic!("a style is a Vec")
    };
    assert_eq!(items.len(), 7);
    assert!(matches!(&items[6], Expr::Call { method, .. } if method == "on_hover"));
    // every conditional rule of the conditions case lands in `style_card`
    let m = lower_styles(&parse(codegen_cases::CONDITIONS));
    let card = m.items.iter().find(|i| i.name.snake() == "style_card").unwrap();
    let Expr::Vec { items, .. } = &card.value else {
        panic!()
    };
    assert_eq!(items.len(), 13, "{items:#?}");
}

use codegen_cases::keyword_list;

#[test]
fn revert_and_unset_use_the_variant_over_the_value_alias() {
    let m = lower_property_list("keywords", &keyword_list());
    // the text-shadow (a BoxOrStatic payload) is dropped with a note
    assert!(
        m.items[0].doc.iter().any(|d| d.contains("text-shadow")),
        "{:?}",
        m.items[0].doc
    );
    let Expr::Vec { items, .. } = &m.items[0].value else {
        panic!()
    };
    match &items[0] {
        Expr::Call { method, args, .. } => {
            assert_eq!(method, "simple");
            match &args[0] {
                Expr::Variant {
                    ty,
                    shape,
                    variant,
                    args,
                } => {
                    assert_eq!((ty.as_str(), variant.as_str()), ("CssProperty", "Width"));
                    assert_eq!(*shape, EnumShape::TaggedShadowed);
                    assert!(matches!(&args[0], Expr::Variant { ty, variant, shape: EnumShape::Generic { .. }, .. }
                        if ty == "LayoutWidthValue" && variant == "Revert"));
                }
                other => panic!("{other:?}"),
            }
        }
        other => panic!("{other:?}"),
    }
    let c = azul_css::codegen::backend_for("c").unwrap().emit_module(&m);
    assert!(
        c.contains(
            "(AzCssProperty){ .Width = { .tag = AzCssProperty_Tag_Width, .payload = \
             (AzLayoutWidthValue){ .Revert = { .tag = AzLayoutWidthValue_Tag_Revert } } } }"
        ),
        "{c}"
    );
}

#[test]
fn julia_builds_a_union_variant_by_field_name_so_the_c_padding_cannot_shift_its_payload() {
    // azul.h puts `uint8_t _pad0[N]` between a variant's tag and a payload
    // that is less aligned than the union, and azul.jl mirrors it as a
    // `_pad0` field of the variant struct. A positional
    // `AzXVariant_Y(tag, payload)` would hand the payload to the pad.
    let m = lower_property_list("keywords", &keyword_list());
    let src = azul_css::codegen::backend_for("julia").unwrap().emit_module(&m);
    assert!(
        src.contains("az_union(Azul.AzCssProperty, Azul.AzCssPropertyVariant_Width, UInt8("),
        "{src}"
    );
    assert!(!src.contains("Azul.AzCssPropertyVariant_Width(UInt8("), "{src}");
}

// ------------------------------------------------------ names vs. azul.h

fn azul_h() -> Option<String> {
    let dirs = [
        std::env::var("AZ_CODEGEN_DIR").ok().map(PathBuf::from),
        Some(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../target/codegen")),
    ];
    dirs.into_iter()
        .flatten()
        .find_map(|d| fs::read_to_string(d.join("azul.h")).ok())
}

/// Every C-ABI name an IR node stands for (the other languages derive their
/// spellings from the same api.json names).
fn c_names(e: &Expr, out: &mut BTreeSet<String>) {
    e.walk(&mut |n| match n {
        Expr::Call { class, method, .. } => {
            out.insert(format!("Az{class}_{}(", snake_to_lower_camel(method)));
        }
        Expr::Variant {
            ty, shape, variant, ..
        } => {
            out.insert(match shape {
                EnumShape::CLike => format!("Az{ty}_{variant},"),
                EnumShape::Tagged => format!("Az{ty}_{}(", variant_ctor_method(variant)),
                EnumShape::TaggedShadowed | EnumShape::Generic { .. } => {
                    format!("Az{ty}_Tag_{variant},")
                }
            });
        }
        Expr::Struct { ty, fields } => {
            out.insert(format!("struct Az{ty} {{"));
            for (f, _) in fields {
                out.insert(format!(" {f};"));
            }
        }
        Expr::Vec { ty, .. } => {
            out.insert(format!("Az{ty}_copyFromPtr("));
            out.insert(format!("Az{ty}_create("));
        }
        Expr::Str(_) => {
            out.insert("AzString_copyFromBytes(".to_string());
        }
        // a by-value `self` method: `AzDom_withChild(AzDom dom, ..`
        Expr::Method { class, method, .. } => {
            out.insert(format!("Az{class}_{}(Az{class} ", snake_to_lower_camel(method)));
        }
        _ => {}
    });
}

#[test]
fn every_c_abi_name_the_ir_produces_exists_in_azul_h() {
    let Some(header) = azul_h() else {
        eprintln!(
            "SKIPPED: no generated azul.h (set AZ_CODEGEN_DIR or run `azul-doc codegen all`)"
        );
        return;
    };
    let mut names = BTreeSet::new();
    for (_, css) in CASES {
        let css = parse(css);
        for m in [lower_styles(&css), lower_stylesheet(&css)] {
            for item in &m.items {
                c_names(&item.value, &mut names);
            }
        }
    }
    for item in &lower_property_list("keywords", &keyword_list()).items {
        c_names(&item.value, &mut names);
    }
    // the DOM export's constructors and builder methods
    for m in [
        codegen_cases::dom_card_module(),
        codegen_cases::dom_app_module(),
    ] {
        for item in &m.items {
            c_names(&item.value, &mut names);
        }
    }
    let missing: Vec<&String> = names.iter().filter(|n| !header.contains(n.as_str())).collect();
    assert!(missing.is_empty(), "not in azul.h: {missing:#?}");
}

// ------------------------------------------------- per-language structure

/// How to skip comments and string literals when counting brackets.
struct Lexical {
    line_comments: &'static [&'static str],
    block_comment: Option<(&'static str, &'static str)>,
    quote: char,
    /// `true`: `\"` escapes; `false`: `""` escapes (Pascal, Ada, VB, COBOL, ...)
    backslash_escapes: bool,
}

const C_LIKE: Lexical = Lexical {
    line_comments: &["//"],
    block_comment: Some(("/*", "*/")),
    quote: '"',
    backslash_escapes: true,
};

fn lexical(lang: &str) -> Lexical {
    match lang {
        "python" | "ruby" | "perl" | "crystal" | "julia" | "nim" => Lexical {
            line_comments: &["#"],
            block_comment: None,
            ..C_LIKE
        },
        "powershell" => Lexical {
            line_comments: &["#"],
            block_comment: Some(("<#", "#>")),
            quote: '\'',
            backslash_escapes: false,
        },
        "lua" | "haskell" => Lexical {
            line_comments: &["--"],
            block_comment: None,
            ..C_LIKE
        },
        "ada" => Lexical {
            line_comments: &["--"],
            block_comment: None,
            quote: '"',
            backslash_escapes: false,
        },
        "pascal" => Lexical {
            line_comments: &["//"],
            block_comment: Some(("{", "}")),
            quote: '\'',
            backslash_escapes: false,
        },
        "freebasic" | "vb6" => Lexical {
            line_comments: &["'"],
            block_comment: None,
            quote: '"',
            backslash_escapes: false,
        },
        "fortran" => Lexical {
            line_comments: &["!"],
            block_comment: None,
            quote: '\'',
            backslash_escapes: false,
        },
        "cobol" => Lexical {
            line_comments: &["*>"],
            block_comment: None,
            quote: '"',
            backslash_escapes: false,
        },
        "lisp" | "racket" | "red" => Lexical {
            line_comments: &[";"],
            block_comment: None,
            ..C_LIKE
        },
        "ocaml" => Lexical {
            line_comments: &[],
            block_comment: Some(("(*", "*)")),
            ..C_LIKE
        },
        "smalltalk" => Lexical {
            line_comments: &[],
            block_comment: Some(("\"", "\"")),
            quote: '\'',
            backslash_escapes: false,
        },
        "algol68" => Lexical {
            line_comments: &[],
            block_comment: Some(("#", "#")),
            quote: '"',
            backslash_escapes: false,
        },
        "v" => Lexical {
            quote: '\'',
            ..C_LIKE
        },
        _ => C_LIKE,
    }
}

/// The source with comments and string literals blanked out.
fn code_only(src: &str, lx: &Lexical) -> String {
    let mut out = String::new();
    let mut rest = src;
    'outer: while let Some(c) = rest.chars().next() {
        for lc in lx.line_comments {
            if rest.starts_with(lc) {
                let end = rest.find('\n').unwrap_or(rest.len());
                rest = &rest[end..];
                continue 'outer;
            }
        }
        if let Some((open, close)) = lx.block_comment {
            if rest.starts_with(open) {
                let after = &rest[open.len()..];
                let end = after.find(close).map_or(after.len(), |i| i + close.len());
                rest = &after[end..];
                continue 'outer;
            }
        }
        if c == lx.quote {
            let mut chars = rest.char_indices().skip(1);
            let mut end = rest.len();
            while let Some((i, ch)) = chars.next() {
                if lx.backslash_escapes && ch == '\\' {
                    chars.next();
                } else if ch == lx.quote {
                    if !lx.backslash_escapes && rest[i + 1..].starts_with(lx.quote) {
                        chars.next();
                        continue;
                    }
                    end = i + 1;
                    break;
                }
            }
            out.push_str("\"\"");
            rest = &rest[end..];
            continue;
        }
        out.push(c);
        rest = &rest[c.len_utf8()..];
    }
    out
}

fn check_balanced(src: &str, lang: &str, what: &str) {
    let code = code_only(src, &lexical(lang));
    let mut stack = Vec::new();
    for (line_no, line) in code.lines().enumerate() {
        for c in line.chars() {
            match c {
                '(' | '[' | '{' => stack.push((c, line_no + 1)),
                ')' | ']' | '}' => {
                    let want = match c {
                        ')' => '(',
                        ']' => '[',
                        _ => '{',
                    };
                    match stack.pop() {
                        Some((open, _)) if open == want => {}
                        other => panic!(
                            "{lang} {what}: unbalanced `{c}` on line {} (open: {other:?})\n{src}",
                            line_no + 1
                        ),
                    }
                }
                _ => {}
            }
        }
    }
    assert!(stack.is_empty(), "{lang} {what}: unclosed {stack:?}\n{src}");
}

fn all_outputs() -> Vec<(String, String, String)> {
    let mut out = Vec::new();
    let keywords = lower_property_list("keywords", &keyword_list());
    for backend in all_backends() {
        let lang = backend.lang().to_string();
        for (case, css) in CASES {
            let css = parse(css);
            out.push((lang.clone(), format!("{case} (styles)"), backend.emit_css(&css)));
            out.push((lang.clone(), format!("{case} (stylesheet)"), backend.emit_stylesheet(&css)));
        }
        out.push((lang.clone(), "keywords".into(), backend.emit_module(&keywords)));
        for f in backend.emit_project(&parse(codegen_cases::BASIC)) {
            out.push((lang.clone(), format!("project {}", f.path), f.contents));
        }
    }
    out
}

#[test]
fn every_language_output_has_balanced_brackets() {
    for (lang, what, src) in all_outputs() {
        check_balanced(&src, &lang, &what);
    }
}

#[test]
fn every_language_output_is_ascii_and_non_empty() {
    for (lang, what, src) in all_outputs() {
        assert!(!src.trim().is_empty(), "{lang} {what}: empty");
        assert!(src.is_ascii(), "{lang} {what}: non-ASCII output");
    }
}

#[test]
fn no_output_calls_a_constructor_named_new() {
    // api.json reserves `new`: constructors are `create*`. The IR never
    // names one, and no printer invents one.
    for (_, css) in CASES {
        let css = parse(css);
        for m in [lower_styles(&css), lower_stylesheet(&css)] {
            for item in &m.items {
                item.value.walk(&mut |e| {
                    if let Expr::Call { method, class, .. } = e {
                        assert_ne!(method, "new", "{class}::new");
                    }
                });
            }
        }
    }
    for (lang, what, src) in all_outputs() {
        for bad in ["::new(", "_new(", ".New(", "::New("] {
            assert!(!src.contains(bad), "{lang} {what}: contains `{bad}`");
        }
    }
}

#[test]
fn every_backend_is_reachable_by_its_id_and_aliases() {
    for backend in all_backends() {
        let found = azul_css::codegen::backend_for(backend.lang()).unwrap();
        assert_eq!(found.lang(), backend.lang());
        for alias in backend.aliases() {
            assert_eq!(azul_css::codegen::backend_for(alias).unwrap().lang(), backend.lang());
        }
    }
    assert!(azul_css::codegen::backend_for("brainfuck").is_none());
}

#[test]
fn exports_dom_is_true_exactly_for_the_printers_that_build_the_dom() {
    // AzBuilder's dialogs offer DOM export for the languages whose
    // `exports_dom()` is true: it must say what the printer does. The card's
    // link text is printed only when the printer builds the DOM (a printer
    // without DOM export prints why instead of the value).
    let card = codegen_cases::dom_card_module();
    let mut dom = Vec::new();
    for backend in all_backends() {
        let src = backend.emit_module(&card);
        assert_eq!(
            src.contains("Read more"),
            backend.exports_dom(),
            "{}: exports_dom() is {} but the printer {} the DOM:\n{src}",
            backend.lang(),
            backend.exports_dom(),
            if src.contains("Read more") { "builds" } else { "does not build" }
        );
        if backend.exports_dom() {
            dom.push(backend.lang());
        }
    }
    assert_eq!(dom, ["rust", "c", "cpp", "python"]);
}
