//! Categorization + adversarial strategy attachment for the `autotest` harness.
//!
//! Each extracted function is classified by name + signature heuristics into one of a
//! handful of [`Category`] kinds, and a tailored list of adversarial test [`Strategy`]
//! entries is attached. Round-trip detection happens at the file level (a type with
//! BOTH a parser and a serializer gets a property test), so it is applied in a second
//! pass over the per-file function list.

use serde::Serialize;

use super::extract::{ExtractedFn, SelfKind};

/// The category a function falls into. Drives which adversarial strategies are emitted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    /// `parse` / `from_str` / `from_bytes` / `try_from`, or takes `&str`/`&[u8]` and
    /// returns `Result`/`Option`.
    Parser,
    /// `to_string` / `Display::fmt` / `serialize` / `format*` returning `String`.
    Serializer,
    /// A type that has BOTH a parser and a serializer — gets a property round-trip test.
    RoundTrip,
    /// `new` / `with_*` / `default` returning `Self`.
    Constructor,
    /// `is_*` / `has_*` returning `bool`.
    Predicate,
    /// `&self` accessor returning a field-like value.
    Getter,
    /// Numeric / geometry function (floats, ints, `Point`/`Rect`/`Size`-style args).
    Numeric,
    /// Anything else — gets a generic "does not panic" smoke strategy.
    Other,
}

impl Category {
    pub fn as_str(self) -> &'static str {
        match self {
            Category::Parser => "parser",
            Category::Serializer => "serializer",
            Category::RoundTrip => "round_trip",
            Category::Constructor => "constructor",
            Category::Predicate => "predicate",
            Category::Getter => "getter",
            Category::Numeric => "numeric",
            Category::Other => "other",
        }
    }
}

/// A single adversarial test strategy: a short label + a concrete suggested case the
/// agent should turn into an assertion.
#[derive(Debug, Clone, Serialize)]
pub struct Strategy {
    /// Short label, e.g. `empty_input`, `overflow`, `round_trip`.
    pub label: String,
    /// Concrete, human-readable suggested case for the test author.
    pub case: String,
}

impl Strategy {
    fn new(label: &str, case: &str) -> Self {
        Strategy {
            label: label.to_string(),
            case: case.to_string(),
        }
    }
}

/// A function plus its computed category and adversarial strategies — the unit emitted
/// into the manifest and task files.
#[derive(Debug, Clone)]
pub struct CategorizedFn {
    pub func: ExtractedFn,
    pub category: Category,
    pub strategies: Vec<Strategy>,
}

/// Categorize all functions in a file and attach strategies, then run the file-level
/// round-trip detection pass.
pub fn categorize_file(funcs: Vec<ExtractedFn>) -> Vec<CategorizedFn> {
    // First pass: per-function categorization.
    let mut categorized: Vec<CategorizedFn> = funcs
        .into_iter()
        .map(|f| {
            let category = classify(&f);
            let strategies = strategies_for(&f, category);
            CategorizedFn {
                func: f,
                category,
                strategies,
            }
        })
        .collect();

    // Second pass: round-trip detection. A `self_type` (or, for free functions, a
    // type appearing in both a parser's return and a serializer's input) that has both
    // a parser AND a serializer is upgraded to RoundTrip.
    apply_round_trip(&mut categorized);

    categorized
}

/// Classify a single function by name + signature heuristics.
fn classify(f: &ExtractedFn) -> Category {
    let name = f.name.to_lowercase();
    let ret = f.return_type.as_deref().unwrap_or("");
    let ret_lower = ret.to_lowercase();

    let returns_result_or_option =
        ret_lower.starts_with("result<") || ret_lower.starts_with("option<");
    let returns_string = ret == "String" || ret_lower == "string";
    let returns_bool = ret == "bool";
    let returns_self = ret == "Self"
        || f.self_type.as_deref() == Some(ret)
        || (f.self_type.is_some()
            && (ret_lower.starts_with("result<self") || ret_lower.starts_with("option<self")));

    let takes_str = f.args.iter().any(|a| is_str_arg(&a.ty));
    let takes_bytes = f.args.iter().any(|a| is_bytes_arg(&a.ty));

    // // parser
    // Name-based: parse* / from_str / from_bytes / try_from.
    let is_parser_name = name.starts_with("parse")
        || name == "from_str"
        || name == "from_bytes"
        || name == "try_from"
        || name.starts_with("from_str")
        || name.starts_with("parse_");
    // Signature-based: takes &str or &[u8] and returns Result/Option.
    let is_parser_sig = (takes_str || takes_bytes) && returns_result_or_option;
    if is_parser_name || is_parser_sig {
        return Category::Parser;
    }

    // // serializer
    // Display::fmt is special: takes &Formatter, returns fmt::Result.
    let is_fmt = name == "fmt" && f.self_type.is_some();
    let is_serializer_name = name == "to_string"
        || name == "serialize"
        || name.starts_with("format")
        || name.starts_with("to_str")
        || name == "as_str_owned";
    if is_fmt || (is_serializer_name && (returns_string || is_fmt)) {
        return Category::Serializer;
    }

    // // predicate
    if (name.starts_with("is_") || name.starts_with("has_") || name.starts_with("can_"))
        && returns_bool
    {
        return Category::Predicate;
    }

    // // constructor
    let is_ctor_name = name == "new"
        || name == "default"
        || name == "empty"
        || name == "zero"
        || name.starts_with("with_")
        || name.starts_with("new_")
        || name.starts_with("from_");
    if is_ctor_name && (returns_self || f.self_kind.is_none()) && returns_self {
        return Category::Constructor;
    }

    // // numeric / geometry
    if is_numeric_signature(f) {
        return Category::Numeric;
    }

    // // getter (immutable accessor on &self with a return)
    if matches!(f.self_kind, Some(SelfKind::Ref))
        && f.args.is_empty()
        && f.return_type.is_some()
    {
        return Category::Getter;
    }

    Category::Other
}

/// Attach adversarial strategies tailored to the category.
fn strategies_for(f: &ExtractedFn, category: Category) -> Vec<Strategy> {
    let mut out = Vec::new();
    let takes_bytes = f.args.iter().any(|a| is_bytes_arg(&a.ty));

    match category {
        Category::Parser => {
            out.push(Strategy::new(
                "empty_input",
                "empty input (\"\" or b\"\") returns Err/None without panicking",
            ));
            out.push(Strategy::new(
                "whitespace_only",
                "whitespace-only input (\"   \", \"\\t\\n\") is handled (Err/None or trimmed)",
            ));
            out.push(Strategy::new(
                "garbage",
                "garbage / malformed input (random non-grammar bytes) returns Err/None, never panics",
            ));
            out.push(Strategy::new(
                "extremely_long",
                "extremely long input (e.g. 1_000_000 chars / repeated token) does not panic or hang",
            ));
            if takes_bytes {
                out.push(Strategy::new(
                    "invalid_utf8",
                    "invalid UTF-8 bytes (e.g. &[0xFF, 0xFE, 0x00]) return Err/None, never panic",
                ));
            }
            out.push(Strategy::new(
                "boundary_numbers",
                "boundary numeric strings (\"0\", \"-0\", i64::MAX, f64 huge/tiny, \"NaN\", \"inf\")",
            ));
            out.push(Strategy::new(
                "leading_trailing_junk",
                "leading/trailing junk (\"  valid  \", \"valid;garbage\") is rejected or trimmed deterministically",
            ));
            out.push(Strategy::new(
                "unicode",
                "non-ASCII / multibyte unicode input (e.g. \"\\u{1F600}\", combining marks) does not panic",
            ));
            out.push(Strategy::new(
                "nested_recursion",
                "deeply nested / recursive input (e.g. 10_000 nested brackets) does not stack-overflow",
            ));
            out.push(Strategy::new(
                "valid_minimal",
                "one known-good minimal input parses to the expected value (positive control)",
            ));
        }
        Category::Serializer => {
            out.push(Strategy::new(
                "non_empty_or_valid",
                "output is non-empty / well-formed for a representative value",
            ));
            out.push(Strategy::new(
                "no_panic_default",
                "no panic when serializing Default::default() / a zero value",
            ));
            out.push(Strategy::new(
                "edge_values",
                "no panic on edge values (empty collections, MIN/MAX numbers, NaN/inf for floats)",
            ));
        }
        Category::RoundTrip => {
            out.push(Strategy::new(
                "round_trip_representative",
                "parse(serialize(x)) == x for a representative value x",
            ));
            out.push(Strategy::new(
                "round_trip_edge",
                "round-trips for edge x: default, min, max, empty, and a unicode-bearing value",
            ));
            out.push(Strategy::new(
                "serialize_then_parse_stable",
                "serialize(parse(serialize(x))) == serialize(x) (idempotent normalization)",
            ));
        }
        Category::Constructor => {
            out.push(Strategy::new(
                "no_panic",
                "constructor does not panic for representative + extreme arguments",
            ));
            out.push(Strategy::new(
                "invariants_hold",
                "post-construction invariants hold (len/capacity consistent, fields match args)",
            ));
            if f.name == "default" || f.name == "empty" || f.name == "zero" {
                out.push(Strategy::new(
                    "default_is_neutral",
                    "default/empty/zero value behaves as a neutral element (is_empty()/len()==0 where applicable)",
                ));
            }
        }
        Category::Predicate => {
            out.push(Strategy::new(
                "basic_true_false",
                "returns the expected bool for one known-true and one known-false input",
            ));
            out.push(Strategy::new(
                "edge_inputs",
                "edge inputs (empty, default, boundary) return a deterministic bool without panicking",
            ));
        }
        Category::Getter => {
            out.push(Strategy::new(
                "basic_access",
                "returns the expected value after a known construction",
            ));
            out.push(Strategy::new(
                "edge_access",
                "does not panic on a default / empty / extreme instance",
            ));
        }
        Category::Numeric => {
            out.push(Strategy::new(
                "zero",
                "behaves correctly at 0",
            ));
            out.push(Strategy::new(
                "min_max",
                "no unexpected panic at MIN / MAX of the integer types involved",
            ));
            out.push(Strategy::new(
                "negative",
                "handles negative inputs (where signed) deterministically",
            ));
            out.push(Strategy::new(
                "overflow",
                "saturating/wrapping behavior at overflow is as documented (no debug-panic surprises)",
            ));
            if has_float_arg(f) {
                out.push(Strategy::new(
                    "nan_inf",
                    "NaN / +inf / -inf inputs do not panic and produce a defined result",
                ));
            }
        }
        Category::Other => {
            out.push(Strategy::new(
                "no_panic_smoke",
                "does not panic for representative + a couple of extreme arguments",
            ));
        }
    }

    out
}

/// File-level round-trip detection. For each `self_type` (method receiver type) that
/// owns BOTH a parser and a serializer, re-tag those functions as `RoundTrip` and give
/// them round-trip strategies (keeping their original adversarial cases as well).
fn apply_round_trip(funcs: &mut [CategorizedFn]) {
    use std::collections::BTreeMap;

    // Count parser/serializer presence per self_type.
    #[derive(Default)]
    struct Flags {
        has_parser: bool,
        has_serializer: bool,
    }
    let mut by_type: BTreeMap<String, Flags> = BTreeMap::new();

    for cf in funcs.iter() {
        let key = match &cf.func.self_type {
            Some(t) => t.clone(),
            None => continue,
        };
        let entry = by_type.entry(key).or_default();
        match cf.category {
            Category::Parser => entry.has_parser = true,
            Category::Serializer => entry.has_serializer = true,
            _ => {}
        }
    }

    let round_trip_types: std::collections::BTreeSet<String> = by_type
        .into_iter()
        .filter(|(_, f)| f.has_parser && f.has_serializer)
        .map(|(t, _)| t)
        .collect();

    if round_trip_types.is_empty() {
        return;
    }

    for cf in funcs.iter_mut() {
        let Some(ty) = cf.func.self_type.clone() else {
            continue;
        };
        if !round_trip_types.contains(&ty) {
            continue;
        }
        if matches!(cf.category, Category::Parser | Category::Serializer) {
            // Upgrade to RoundTrip but preserve the original adversarial cases too,
            // since a round-trip test still benefits from edge inputs.
            let mut combined = strategies_for(&cf.func, Category::RoundTrip);
            combined.append(&mut cf.strategies);
            cf.category = Category::RoundTrip;
            cf.strategies = combined;
        }
    }
}

// // helpers

/// True if an argument type is a string slice / owned string we can fuzz with text.
fn is_str_arg(ty: &str) -> bool {
    let t = ty.replace(' ', "");
    t == "&str"
        || t.starts_with("&'")
            && t.contains("str")
            && !t.contains("[")
        || t == "&String"
        || t == "String"
        || t.ends_with("str")
}

/// True if an argument type is a byte slice / byte vec we can fuzz with raw bytes.
fn is_bytes_arg(ty: &str) -> bool {
    let t = ty.replace(' ', "");
    t == "&[u8]"
        || t == "&[u8;N]"
        || t == "Vec<u8>"
        || t == "&Vec<u8>"
        || t.starts_with("&'") && t.contains("[u8]")
        || t.contains("U8Vec")
}

/// True if the function looks numeric / geometric based on arg / return types.
fn is_numeric_signature(f: &ExtractedFn) -> bool {
    const NUMERIC: &[&str] = &[
        "f32", "f64", "i8", "i16", "i32", "i64", "i128", "isize", "u8", "u16", "u32", "u64",
        "u128", "usize",
    ];
    const GEOM: &[&str] = &[
        "Point", "Rect", "Size", "Vec2", "Vector", "PixelValue", "FloatValue", "PercentageValue",
        "LayoutPoint", "LayoutRect", "LayoutSize", "LogicalPosition", "LogicalSize",
        "PhysicalPosition", "PhysicalSize",
    ];

    let mut numeric_args = 0;
    for a in &f.args {
        let base = strip_refs(&a.ty);
        if NUMERIC.contains(&base.as_str()) {
            numeric_args += 1;
        }
        if GEOM.iter().any(|g| base.contains(g)) {
            numeric_args += 1;
        }
    }

    let ret_numeric = f
        .return_type
        .as_deref()
        .map(|r| {
            let base = strip_refs(r);
            NUMERIC.contains(&base.as_str()) || GEOM.iter().any(|g| base.contains(g))
        })
        .unwrap_or(false);

    // Require at least one numeric/geom arg (so we have something to push to extremes),
    // OR a numeric return with no args at all is not interesting (that's a getter).
    numeric_args >= 1 && (ret_numeric || numeric_args >= 1)
}

/// True if any argument is a float.
fn has_float_arg(f: &ExtractedFn) -> bool {
    f.args.iter().any(|a| {
        let base = strip_refs(&a.ty);
        base == "f32" || base == "f64"
    })
}

/// Strip leading `&`, `&mut `, `*const `, `*mut ` and surrounding whitespace to get a
/// base type name for matching.
fn strip_refs(ty: &str) -> String {
    let mut t = ty.trim();
    for prefix in ["&mut ", "&", "*const ", "*mut "] {
        if let Some(rest) = t.strip_prefix(prefix) {
            t = rest.trim();
        }
    }
    // Drop lifetimes like `'a `.
    let t = t.trim_start_matches('\'');
    t.to_string()
}
