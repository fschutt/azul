//! The scanners of Rust, Python and C/C++ on fixtures with known lines: what is found, what is not
//! (names in comments, strings and macro bodies), the extents, the owners of methods, the imports,
//! and hostile input. No window, no azul.

use super::*;
use crate::symbols::{Kind, Vis};
use crate::symbols_source::Lang;

type Row = (String, Kind, u32, u32, Option<String>);

/// One expected definition: name, kind, line, lines, parent.
fn d(name: &str, kind: Kind, line: u32, lines: u32, parent: Option<&str>) -> Row {
    (name.to_string(), kind, line, lines, parent.map(str::to_string))
}

/// The text of the lines, each ended by a newline: line N of a fixture is the N-th element.
fn src(lines: &[&str]) -> String {
    let mut s = lines.join("\n");
    s.push('\n');
    s
}

fn scan(lang: Lang, text: &str) -> Scanned {
    scan_code(lang, "src/x.rs", text.as_bytes(), &ScanCaps::default())
}

fn rows(lang: Lang, text: &str) -> Vec<Row> {
    scan(lang, text)
        .defs
        .into_iter()
        .map(|def| (def.name, def.kind, def.line, def.lines, def.parent))
        .collect()
}

fn rust(lines: &[&str]) -> Vec<Row> {
    rows(Lang::Rust, &src(lines))
}

// ---- Rust ----

#[test]
fn rust_items_of_every_kind_with_their_lines_extents_and_owners() {
    let got = rust(&[
        "//! crate docs mention struct Fake { and fn ghost()",
        "use std::collections::HashMap;",
        "pub struct CssError { pub kind: u8 }",
        "pub(crate) enum Mode {",
        "    A,",
        "    B(u8),",
        "    C { x: i32 },",
        "}",
        "pub trait Draw {",
        "    fn draw(&self);",
        "    type Out;",
        "    const N: usize;",
        "}",
        "pub type Outcome<T> = std::result::Result<T, CssError>;",
        "pub const MAX: usize = 10;",
        "static mut COUNTER: u32 = 0;",
        "pub fn parse(input: &str) -> Outcome<()> {",
        "    Ok(())",
        "}",
        "mod inner {",
        "    pub fn deep() {}",
        "}",
        "macro_rules! my_macro {",
        "    ($x:expr) => { struct FromMacro; };",
        "}",
        "impl CssError {",
        "    pub fn new() -> Self { Self { kind: 0 } }",
        "    fn private(&self) {}",
        "}",
        "impl Draw for CssError {",
        "    fn draw(&self) {}",
        "    type Out = ();",
        "}",
    ]);
    let want = vec![
        d("CssError", Kind::Struct, 3, 1, None),
        d("Mode", Kind::Enum, 4, 5, None),
        d("Draw", Kind::Trait, 9, 5, None),
        d("draw", Kind::Method, 10, 1, Some("Draw")),
        d("Out", Kind::Type, 11, 1, Some("Draw")),
        d("N", Kind::Const, 12, 1, Some("Draw")),
        d("Outcome", Kind::Type, 14, 1, None),
        d("MAX", Kind::Const, 15, 1, None),
        d("COUNTER", Kind::Static, 16, 1, None),
        d("parse", Kind::Fn, 17, 3, None),
        d("inner", Kind::Module, 20, 3, None),
        d("deep", Kind::Fn, 21, 1, Some("inner")),
        d("my_macro", Kind::Macro, 23, 3, None),
        d("new", Kind::Method, 27, 1, Some("CssError")),
        d("private", Kind::Method, 28, 1, Some("CssError")),
        d("draw", Kind::Method, 31, 1, Some("CssError")),
        d("Out", Kind::Type, 32, 1, Some("CssError")),
    ];
    assert_eq!(got, want);
}

#[test]
fn rust_visibility_is_public_restricted_or_private() {
    let s = scan(
        Lang::Rust,
        "pub struct A; pub(crate) struct B; pub(super) struct C; pub(in crate::x) struct D; struct E;\n",
    );
    let vis: Vec<(&str, Vis)> = s.defs.iter().map(|d| (d.name.as_str(), d.vis)).collect();
    assert_eq!(
        vis,
        [("A", Vis::Public), ("B", Vis::Restricted), ("C", Vis::Restricted), ("D", Vis::Restricted), ("E", Vis::Private)]
    );
}

#[test]
fn rust_names_in_comments_strings_raw_strings_and_nested_comments_are_not_defs() {
    let got = rust(&[
        "// struct InComment {}",
        "/* struct InBlock {} /* nested struct InNested {} */ still comment struct StillComment {} */",
        "const S: &str = \"struct InString {}\";",
        "const R: &str = r#\"fn in_raw() {} \"quoted\" struct AlsoRaw\"#;",
        "fn braces() {",
        "    let c = '{';",
        "    let d = '}';",
        "    let s = \"}\";",
        "}",
        "fn after() {}",
    ]);
    let want = vec![
        d("S", Kind::Const, 3, 1, None),
        d("R", Kind::Const, 4, 1, None),
        d("braces", Kind::Fn, 5, 5, None),
        d("after", Kind::Fn, 10, 1, None),
    ];
    assert_eq!(got, want);
}

#[test]
fn rust_lifetimes_and_character_literals_do_not_confuse_the_braces() {
    let got = rust(&[
        "fn lt<'a>(x: &'a str) -> &'static str {",
        "    'outer: loop { break 'outer; }",
        "    let ch = '\\'';",
        "    let u = '\\u{1F600}';",
        "    let q = b'x';",
        "    let e = '}';",
        "}",
        "fn next() {}",
    ]);
    assert_eq!(got, vec![d("lt", Kind::Fn, 1, 7, None), d("next", Kind::Fn, 8, 1, None)]);
}

#[test]
fn rust_generics_where_clauses_and_impl_trait_returns_keep_the_item_whole() {
    let got = rust(&[
        "pub struct Wrapper<T: Clone, const N: usize> where T: Clone {",
        "    items: [T; N],",
        "}",
        "pub fn generic<T>(x: T) -> impl Iterator<Item = T> where T: Copy {",
        "    std::iter::once(x)",
        "}",
        "pub trait Maker<T>: Sized + Clone {",
        "    fn make() -> Self;",
        "}",
        "pub struct Unit;",
        "pub struct Tuple(u8, [u8; 4]);",
    ]);
    let want = vec![
        d("Wrapper", Kind::Struct, 1, 3, None),
        d("generic", Kind::Fn, 4, 3, None),
        d("Maker", Kind::Trait, 7, 3, None),
        d("make", Kind::Method, 8, 1, Some("Maker")),
        d("Unit", Kind::Struct, 10, 1, None),
        d("Tuple", Kind::Struct, 11, 1, None),
    ];
    assert_eq!(got, want);
}

#[test]
fn rust_a_method_belongs_to_the_type_after_for_or_the_type_of_a_plain_impl() {
    let got = rust(&[
        "impl<T: Clone> Trait<T> for Vec<T> {",
        "    fn m(&self) {}",
        "}",
        "impl crate::a::Foo {",
        "    fn n() {}",
        "}",
        "unsafe impl Send for Bar {}",
        "impl<'a> Baz<'a> {",
        "    const K: u8 = 1;",
        "    type Alias = u8;",
        "    pub fn k(&self) {}",
        "}",
        "impl Trait for (A, B) {",
        "    fn tuple(&self) {}",
        "}",
    ]);
    let want = vec![
        d("m", Kind::Method, 2, 1, Some("Vec")),
        d("n", Kind::Method, 5, 1, Some("Foo")),
        d("K", Kind::Const, 9, 1, Some("Baz")),
        d("Alias", Kind::Type, 10, 1, Some("Baz")),
        d("k", Kind::Method, 11, 1, Some("Baz")),
        d("tuple", Kind::Method, 14, 1, None),
    ];
    assert_eq!(got, want);
}

#[test]
fn rust_items_inside_a_function_body_are_not_recorded() {
    let got = rust(&["fn outer() {", "    fn inner() {}", "    struct Local;", "    const LOCAL: u8 = 1;", "}", "fn after() {}"]);
    assert_eq!(got, vec![d("outer", Kind::Fn, 1, 5, None), d("after", Kind::Fn, 6, 1, None)]);
}

#[test]
fn rust_test_modules_and_test_functions_are_marked_and_not_test_cfgs_are_not() {
    let s = scan(
        Lang::Rust,
        &src(&[
            "pub fn real() {}",
            "#[cfg(test)]",
            "mod tests {",
            "    #[test]",
            "    fn works() {}",
            "    fn helper() {}",
            "}",
            "#[test]",
            "fn standalone() {}",
            "#[cfg(not(test))]",
            "fn not_a_test() {}",
            "#[tokio::test]",
            "async fn async_test() {}",
        ]),
    );
    let flags: Vec<(&str, bool)> = s.defs.iter().map(|d| (d.name.as_str(), d.test)).collect();
    assert_eq!(
        flags,
        [
            ("real", false),
            ("tests", true),
            ("works", true),
            ("helper", true),
            ("standalone", true),
            ("not_a_test", false),
            ("async_test", true)
        ]
    );
    assert_eq!(s.defs[2].parent.as_deref(), Some("tests"));
}

#[test]
fn rust_raw_identifiers_are_names_and_not_keywords() {
    let got = rust(&["fn r#type() {}", "struct r#struct;", "fn after() {}"]);
    assert_eq!(
        got,
        vec![d("type", Kind::Fn, 1, 1, None), d("struct", Kind::Struct, 2, 1, None), d("after", Kind::Fn, 3, 1, None)]
    );
}

#[test]
fn rust_a_macro_invocation_is_read_as_the_code_it_holds_and_a_macro_definition_is_skipped() {
    let got = rust(&[
        "thread_local! {",
        "    static FOO: u8 = 1;",
        "}",
        "impl_vec!(Foo, FooVec);",
        "macro_rules! generate {",
        "    ($name:ident) => { pub struct $name; fn helper() {} };",
        "}",
        "fn after() {}",
    ]);
    let want = vec![
        d("FOO", Kind::Static, 2, 1, None),
        d("generate", Kind::Macro, 5, 3, None),
        d("after", Kind::Fn, 8, 1, None),
    ];
    assert_eq!(got, want);
}

#[test]
fn rust_function_pointer_types_are_no_functions() {
    let got = rust(&["type Cb = fn(i32) -> i32;", "fn takes(f: fn(u8) -> u8, g: impl Fn(u8)) -> Box<dyn Fn()> { f }"]);
    assert_eq!(got, vec![d("Cb", Kind::Type, 1, 1, None), d("takes", Kind::Fn, 2, 1, None)]);
}

#[test]
fn rust_use_trees_are_expanded_and_mod_declarations_are_imports_of_self() {
    let s = scan(
        Lang::Rust,
        &src(&[
            "use a::b::{c, d::e, f as g, self};",
            "use h::*;",
            "extern crate foo;",
            "mod x;",
            "pub mod y {}",
            "use std::io::{self, Read};",
        ]),
    );
    assert_eq!(s.imports, ["a::b::c", "a::b::d::e", "a::b::f", "a::b", "h::*", "foo", "self::x", "std::io", "std::io::Read"]);
    let mods: Vec<(&str, u32)> = s.defs.iter().map(|d| (d.name.as_str(), d.line)).collect();
    assert_eq!(mods, [("x", 4), ("y", 5)]);
}

#[test]
fn rust_extents_run_to_the_closing_brace_or_the_semicolon() {
    let got = rust(&["pub fn multi(", "    a: u8,", "    b: u8,", ") -> u8 {", "    a + b", "}", "pub enum E {", "    A,", "}", "struct S;"]);
    assert_eq!(
        got,
        vec![d("multi", Kind::Fn, 1, 6, None), d("E", Kind::Enum, 7, 3, None), d("S", Kind::Struct, 10, 1, None)]
    );
}

#[test]
fn rust_attributes_are_not_part_of_an_item_and_their_strings_are_not_items() {
    let got = rust(&[
        "#![allow(dead_code)]",
        "#[doc = \"struct Fake {\"]",
        "#[derive(Debug, Clone)]",
        "pub struct Real;",
        "#[cfg_attr(test, derive(Debug))]",
        "pub struct Other {}",
    ]);
    assert_eq!(got, vec![d("Real", Kind::Struct, 4, 1, None), d("Other", Kind::Struct, 6, 1, None)]);
}

#[test]
fn rust_a_union_is_a_def_only_when_it_looks_like_one() {
    let got = rust(&["union Bits { a: u32, b: f32 }", "fn f() { let union = 3; union + 1; }"]);
    assert_eq!(got, vec![d("Bits", Kind::Union, 1, 1, None), d("f", Kind::Fn, 2, 1, None)]);
}

#[test]
fn rust_a_file_counts_its_lines_with_or_without_a_last_newline_and_knows_a_test_file() {
    assert_eq!(scan(Lang::Rust, "a\nb\nc").lines, 3);
    assert_eq!(scan(Lang::Rust, "a\nb\n").lines, 2);
    assert_eq!(scan(Lang::Rust, "").lines, 0);
    let s = scan_code(Lang::Rust, "src/a_test.rs", b"fn a() {}\n", &ScanCaps::default());
    assert!(s.test_file);
    assert_eq!(s.lang, Some(Lang::Rust));
    assert_eq!(s.size, 10);
    assert!(!scan_code(Lang::Rust, "src/a.rs", b"fn a() {}\n", &ScanCaps::default()).test_file);
}

#[test]
fn rust_only_as_many_defs_as_the_cap_are_kept() {
    let text: String = (0..10).map(|i| format!("fn f{i}() {{}}\n")).collect();
    let caps = ScanCaps { max_defs: 3, ..ScanCaps::default() };
    let s = scan_code(Lang::Rust, "a.rs", text.as_bytes(), &caps);
    assert_eq!(s.defs.len(), 3);
    assert_eq!(s.defs[2].name, "f2");
}

#[test]
fn rust_hostile_input_finds_what_it_can_and_never_panics() {
    let nasty: Vec<String> = vec![
        "fn a() {}\n/* never closed".into(),
        "fn a() {}\nconst S: &str = \"never closed".into(),
        "fn a() {}\nlet x = r##\"never closed".into(),
        "fn a() {}\nlet c = '".into(),
        "fn a() {}\nlet c = '\\".into(),
        "{".repeat(20_000),
        "}".repeat(20_000),
        "(".repeat(20_000),
        "fn f(".repeat(5_000),
        "struct ".repeat(5_000),
        "a ".repeat(500_000),
        "fn".into(),
        "struct".into(),
        "impl".into(),
        "impl<".into(),
        "use".into(),
        "use a::{".into(),
        "macro_rules".into(),
        "macro_rules! m".into(),
        "macro_rules! m {".into(),
        "mod".into(),
        "#[".into(),
        "#[cfg(test".into(),
        "r#".into(),
        "b'".into(),
        "'a".into(),
        "\u{FEFF}fn bom() {}".into(),
    ];
    for text in &nasty {
        let _ = scan(Lang::Rust, text);
    }
    let got = scan(Lang::Rust, &nasty[0]);
    assert_eq!(got.defs.iter().map(|d| d.name.as_str()).collect::<Vec<_>>(), ["a"]);
    let bytes: Vec<u8> = (0..50_000u32).map(|i| (i.wrapping_mul(2_654_435_761) >> 13) as u8).collect();
    let _ = scan_code(Lang::Rust, "junk.rs", &bytes, &ScanCaps::default());
    let bom = scan(Lang::Rust, "\u{FEFF}fn bom() {}\n");
    assert_eq!(bom.defs.iter().map(|d| d.name.as_str()).collect::<Vec<_>>(), ["bom"]);
}

#[test]
fn rust_unicode_names_are_names() {
    let got = rust(&["pub struct Größe;", "fn größe_in_pixeln() {}"]);
    assert_eq!(got, vec![d("Größe", Kind::Struct, 1, 1, None), d("größe_in_pixeln", Kind::Fn, 2, 1, None)]);
}

// ---- Python ----

fn python(lines: &[&str]) -> Vec<Row> {
    rows(Lang::Python, &src(lines))
}

#[test]
fn python_classes_methods_functions_and_constants_with_their_extents() {
    let got = python(&[
        "\"\"\"Module docstring with def fake(): inside.\"\"\"",
        "import os, sys",
        "from a.b import c as d",
        "from . import x",
        "from .y import z",
        "MAX_RETRIES = 3",
        "lower_name = 1",
        "",
        "",
        "class Reader(Base):",
        "    \"\"\"Docs.",
        "    def not_a_method(self): pass",
        "    \"\"\"",
        "    LIMIT = 5",
        "",
        "    def __init__(self, path):",
        "        self.path = path",
        "",
        "    @property",
        "    def size(self):",
        "        return 1",
        "",
        "    async def fetch(self):",
        "        def inner():",
        "            pass",
        "        return inner",
        "",
        "    class Nested:",
        "        def deep(self): pass",
        "",
        "def top_level(a,",
        "              b):",
        "    # def commented(): pass",
        "    return a",
        "",
        "async def run():",
        "    pass",
    ]);
    let want = vec![
        d("MAX_RETRIES", Kind::Const, 6, 1, None),
        d("Reader", Kind::Class, 10, 20, None),
        d("__init__", Kind::Method, 16, 2, Some("Reader")),
        d("size", Kind::Method, 20, 2, Some("Reader")),
        d("fetch", Kind::Method, 23, 4, Some("Reader")),
        d("Nested", Kind::Class, 28, 2, Some("Reader")),
        d("deep", Kind::Method, 29, 1, Some("Nested")),
        d("top_level", Kind::Fn, 31, 4, None),
        d("run", Kind::Fn, 36, 2, None),
    ];
    assert_eq!(got, want);
}

#[test]
fn python_imports_are_dotted_paths_and_relative_ones_keep_their_dots() {
    let s = scan(
        Lang::Python,
        &src(&["import os, sys", "from a.b import c as d, e", "from . import x", "from .y import z", "from ..p.q import *", "import a.b.c"]),
    );
    assert_eq!(s.imports, ["os", "sys", "a.b.c", "a.b.e", ".x", ".y.z", "..p.q", "a.b.c"]);
}

#[test]
fn python_tabs_and_crlf_line_endings_are_read_like_spaces_and_lf() {
    let got = rows(Lang::Python, "class A:\r\n\tdef m(self):\r\n\t\tpass\r\n\r\ndef f():\r\n\tpass\r\n");
    assert_eq!(got, vec![d("A", Kind::Class, 1, 3, None), d("m", Kind::Method, 2, 2, Some("A")), d("f", Kind::Fn, 5, 2, None)]);
}

#[test]
fn python_a_def_inside_a_string_or_a_comment_is_none_and_an_open_string_never_panics() {
    let got = python(&["x = '''", "def fake(): pass", "'''", "# def commented(): pass", "s = \"def in_string(): pass\"", "def real(): pass"]);
    assert_eq!(got, vec![d("real", Kind::Fn, 6, 1, None)]);
    let s = scan(Lang::Python, "def a():\n    pass\n\"\"\"never closed\ndef b():\n");
    assert_eq!(s.defs.iter().map(|d| d.name.as_str()).collect::<Vec<_>>(), ["a"]);
    for hostile in ["def", "class", "def (", "class :", "\"\"\"", "'''", "def f(\n".repeat(10_000).as_str(), &"    ".repeat(100_000)] {
        let _ = scan(Lang::Python, hostile);
    }
}

// ---- C and C++ ----

fn c(lines: &[&str]) -> Vec<Row> {
    rows(Lang::Cpp, &src(lines))
}

#[test]
fn c_types_macros_functions_prototypes_namespaces_and_methods() {
    let got = c(&[
        "#include \"foo.h\"",
        "#include <stdio.h>",
        "#define MAX_LEN 10",
        "#define SQUARE(x) ((x) * (x))",
        "typedef struct Point { int x; int y; } Point;",
        "typedef enum { RED, GREEN } Color;",
        "typedef void (*Callback)(int);",
        "struct Node { struct Node *next; };",
        "enum Mode { A, B };",
        "int add(int a, int b) {",
        "    return a + b;",
        "}",
        "static void helper(void);",
        "extern \"C\" { int c_api(int x); }",
        "namespace outer { namespace inner {",
        "class Widget {",
        "public:",
        "    Widget();",
        "    void draw() const;",
        "    int value() const { return 1; }",
        "};",
        "} }",
        "void Widget::draw() const { }",
        "template <typename T> struct Box { T v; };",
        "template <typename T> T identity(T x) { return x; }",
    ]);
    let want = vec![
        d("MAX_LEN", Kind::Macro, 3, 1, None),
        d("SQUARE", Kind::Macro, 4, 1, None),
        d("Point", Kind::Struct, 5, 1, None),
        d("Color", Kind::Type, 6, 1, None),
        d("Callback", Kind::Type, 7, 1, None),
        d("Node", Kind::Struct, 8, 1, None),
        d("Mode", Kind::Enum, 9, 1, None),
        d("add", Kind::Fn, 10, 3, None),
        d("helper", Kind::Fn, 13, 1, None),
        d("c_api", Kind::Fn, 14, 1, None),
        d("outer", Kind::Module, 15, 8, None),
        d("inner", Kind::Module, 15, 8, Some("outer")),
        d("Widget", Kind::Class, 16, 6, Some("inner")),
        d("Widget", Kind::Method, 18, 1, Some("Widget")),
        d("draw", Kind::Method, 19, 1, Some("Widget")),
        d("value", Kind::Method, 20, 1, Some("Widget")),
        d("draw", Kind::Method, 23, 1, Some("Widget")),
        d("Box", Kind::Struct, 24, 1, None),
        d("identity", Kind::Fn, 25, 1, None),
    ];
    assert_eq!(got, want);
    let s = scan(Lang::Cpp, &src(&["#include \"foo.h\"", "#include <stdio.h>", "#include   \"sub/bar.hpp\""]));
    assert_eq!(s.imports, ["foo.h", "<stdio.h>", "sub/bar.hpp"]);
}

#[test]
fn c_comments_strings_continuations_raw_strings_and_char_literals_hide_nothing_real_and_show_nothing_fake() {
    let got = c(&[
        "// struct InComment { };",
        "/* int hidden(void) { } */",
        "const char *s = \"struct InString { int x; } fake(void) {}\";",
        "#define LONG(a, b) \\",
        "    do { a; b; } while (0)",
        "char c = '{';",
        "int real(void) { return c == '}'; }",
        "static const char raw[] = R\"x(struct InRaw {})x\";",
        "int after(void) { return 0; }",
    ]);
    let want = vec![d("LONG", Kind::Macro, 4, 2, None), d("real", Kind::Fn, 7, 1, None), d("after", Kind::Fn, 9, 1, None)];
    assert_eq!(got, want);
}

#[test]
fn c_calls_inside_a_function_body_and_statements_are_no_functions() {
    let got = c(&[
        "int main(int argc, char **argv) {",
        "    printf(\"hi\");",
        "    if (argc > 1) { exit(1); }",
        "    for (int i = 0; i < 3; i++) { work(i); }",
        "    return 0;",
        "}",
        "DECLARE_THING(x);",
        "int tail(void);",
    ]);
    assert_eq!(got, vec![d("main", Kind::Fn, 1, 6, None), d("tail", Kind::Fn, 8, 1, None)]);
}

#[test]
fn c_a_forward_declaration_of_a_struct_and_a_variable_of_one_are_no_defs() {
    let got = c(&["struct Fwd;", "struct Other *ptr;", "struct Real { int a; };", "struct Real value;"]);
    assert_eq!(got, vec![d("Real", Kind::Struct, 3, 1, None)]);
}

#[test]
fn c_hostile_input_never_panics() {
    for hostile in [
        "struct".to_string(),
        "typedef".to_string(),
        "#define".to_string(),
        "#include".to_string(),
        "#include <".to_string(),
        "namespace".to_string(),
        "R\"".to_string(),
        "R\"x(".to_string(),
        "int f(".repeat(5_000),
        "{".repeat(20_000),
        "(".repeat(20_000),
        "/*".to_string(),
        "'".to_string(),
        "\"".to_string(),
        "#define A \\".to_string(),
    ] {
        let _ = scan(Lang::C, &hostile);
        let _ = scan(Lang::Cpp, &hostile);
    }
}

#[test]
fn every_language_counts_its_lines_and_reports_itself() {
    for lang in [Lang::Go, Lang::Java, Lang::Kotlin, Lang::CSharp, Lang::Swift, Lang::Shell, Lang::JavaScript, Lang::TypeScript] {
        let s = scan_code(lang, "a.x", b"line one\nline two\n", &ScanCaps::default());
        assert_eq!(s.lines, 2, "{lang:?}");
        assert_eq!(s.lang, Some(lang));
    }
}

#[test]
fn rust_a_const_or_static_whose_value_has_braces_inside_brackets_keeps_its_whole_extent() {
    let got = rust(&[
        "pub const TABLE: [Pair; 2] = [",
        "    Pair { a: 1, b: 2 },",
        "    Pair { a: 3, b: 4 },",
        "];",
        "pub static DEFAULT: Light = Light::Sun(SunLight {",
        "    angle: 1,",
        "});",
        "const WRAPPED: Option<Pair> = Some(Pair {",
        "    a: 1,",
        "    b: 2,",
        "});",
        "const PLAIN: Pair = Pair {",
        "    a: 1,",
        "};",
        "fn after() {}",
    ]);
    let want = vec![
        d("TABLE", Kind::Const, 1, 4, None),
        d("DEFAULT", Kind::Static, 5, 3, None),
        d("WRAPPED", Kind::Const, 8, 4, None),
        d("PLAIN", Kind::Const, 12, 3, None),
        d("after", Kind::Fn, 15, 1, None),
    ];
    assert_eq!(got, want);
}

#[test]
fn rust_a_pointer_to_a_type_with_a_path_is_no_const() {
    let got = rust(&[
        "pub struct Holder {",
        "    pub a: *const crate::x::T,",
        "    pub b: *const core::ffi::c_void,",
        "    pub c: *mut azul_css::Y,",
        "    pub d: *const u8,",
        "}",
        "pub const REAL: u8 = 1;",
        "pub static OTHER: ::core::ffi::c_int = 2;",
    ]);
    let want = vec![
        d("Holder", Kind::Struct, 1, 6, None),
        d("REAL", Kind::Const, 7, 1, None),
        d("OTHER", Kind::Static, 8, 1, None),
    ];
    assert_eq!(got, want);
}

#[test]
fn rust_an_item_inside_the_value_of_a_const_or_static_is_local_to_it() {
    let got = rust(&[
        "const _: fn() = || {",
        "    fn assert_send_sync<T: Send + Sync>() {}",
        "    assert_send_sync::<u8>();",
        "};",
        "static LAZY: Lazy<u8> = Lazy::new(|| {",
        "    fn helper() -> u8 { 1 }",
        "    helper()",
        "});",
        "const BLOCK: u8 = {",
        "    struct Local;",
        "    1",
        "};",
        "fn after() {}",
    ]);
    let want = vec![
        d("LAZY", Kind::Static, 5, 4, None),
        d("BLOCK", Kind::Const, 9, 4, None),
        d("after", Kind::Fn, 13, 1, None),
    ];
    assert_eq!(got, want);
}

// ---- The line of what is found, after every kind of literal ----

/// Every definition's reported line must hold its name: the check the context builder makes before it
/// shows a line, run here on a text. Returns the definitions whose line does not.
fn wrong_lines(lang: Lang, text: &str) -> Vec<(String, u32)> {
    let lines: Vec<&str> = text.split('\n').collect();
    scan(lang, text)
        .defs
        .into_iter()
        .filter(|d| !lines.get(d.line as usize - 1).is_some_and(|l| l.contains(d.name.as_str())))
        .map(|d| (d.name, d.line))
        .collect()
}

#[test]
fn a_character_literal_of_a_backslash_or_a_quote_leaves_the_lines_after_it_where_they_are() {
    let got = rust(&[
        "fn first(c: char) -> bool {",
        r"    if c != '\\' {",
        "        return false;",
        "    }",
        r"    c == '\''",
        "}",
        "fn second() {}",
        r#"fn third() -> char { '"' }"#,
        "fn fourth() {}",
        r"fn fifth() -> u8 { b'\\' }",
        "fn sixth() {}",
    ]);
    let want = vec![
        d("first", Kind::Fn, 1, 6, None),
        d("second", Kind::Fn, 7, 1, None),
        d("third", Kind::Fn, 8, 1, None),
        d("fourth", Kind::Fn, 9, 1, None),
        d("fifth", Kind::Fn, 10, 1, None),
        d("sixth", Kind::Fn, 11, 1, None),
    ];
    assert_eq!(got, want);
}

#[test]
fn every_form_of_character_literal_is_one_token_and_costs_no_line() {
    let forms = [
        "'a'", r"'\n'", r"'\\'", r"'\''", r#"'\"'"#, r#"'"'"#, r"'\u{1F600}'", "'é'", r"'\x41'", r"'\0'", "b'x'", r"b'\\'", r"b'\''", r"b'\n'",
    ];
    for form in forms {
        let text = format!("fn first() {{ let c = {form}; }}\nfn second() {{}}\n");
        assert_eq!(wrong_lines(Lang::Rust, &text), [], "{form}");
        let got = rows(Lang::Rust, &text);
        assert_eq!(got.iter().map(|r| (r.0.as_str(), r.2)).collect::<Vec<_>>(), [("first", 1), ("second", 2)], "{form}");
        // Three on a line, the hard ones side by side.
        let text = format!("fn first() {{ let c = [{form}, '\\\\', '\\'', {form}]; }}\nfn second() {{}}\n");
        assert_eq!(wrong_lines(Lang::Rust, &text), [], "{form} in a row");
    }
}

#[test]
fn a_stray_quote_at_the_end_of_a_line_swallows_nothing_after_it() {
    // Not Rust that compiles, but a file being edited is not either: the line count must hold.
    for tail in ["'", r"'\", r"'\n", r"'\\", "'ab", "b'"] {
        let text = format!("fn first() {{ let c = {tail}\n}}\nfn second() {{}}\nfn third() {{}}\n");
        let got = rows(Lang::Rust, &text);
        let second = got.iter().find(|r| r.0 == "second").map(|r| r.2);
        let third = got.iter().find(|r| r.0 == "third").map(|r| r.2);
        assert_eq!((second, third), (Some(3), Some(4)), "{tail:?}");
    }
}

/// A small deterministic generator (no crate, no clock).
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        self.0 >> 33
    }
}

/// Pieces of a Rust file with literals and comments of every kind that carry quotes, backslashes and
/// newlines, each a few whole lines.
static RUST_PIECES: &[&[&str]] = &[
    &[r#"fn chars() { let a = ['\\', '\'', '\"', '\n', 'x']; }"#],
    &[r"fn backslash(c: char) -> bool { c != '\\' }"],
    &[r#"fn quotes() { let q = '\''; let d = '\"'; let e = '"'; }"#],
    &[r#"fn strings() { let s = "a \\"; let t = "b \" c"; }"#],
    &["fn multi() {", r#"    let s = "first \"#, r#"        second";"#, "}"],
    &["fn raw() { let r = r#\"a \"quoted\" # line", "second line\"#; }"],
    &["/* a block", "   comment with 'quote and \"string */"],
    &["/* outer /* inner */ still outer", "*/"],
    &[r#"// a line comment with an apostrophe it's and "quotes" and a backslash \"#],
    &[r"/// doc with `code` and 'a' and '\'"],
    &["fn life<'a>(x: &'a str) -> &'a str { x }"],
    &[r#"fn bytes() { let b = [b'\\', b'\'', b'"']; }"#],
    &["struct Held<'a> { x: &'a str }"],
    &["fn label() { 'outer: loop { break 'outer; } }"],
    &[r#"fn escapes() { let s = "\\\\"; let t = "\\"; let u = '\\'; }"#],
    &["#[derive(Debug)]", "struct Attr;"],
];

#[test]
fn the_line_of_every_definition_holds_its_name_in_files_of_random_literals_and_comments() {
    let mut rng = Lcg(2026);
    for round in 0..400 {
        let mut lines: Vec<String> = Vec::new();
        let mut want: Vec<(String, u32)> = Vec::new();
        for k in 0..(2 + rng.next() % 12) {
            let piece = RUST_PIECES[(rng.next() % RUST_PIECES.len() as u64) as usize];
            lines.extend(piece.iter().map(|l| (*l).to_string()));
            let name = format!("marker_{round}_{k}");
            lines.push(format!("fn {name}() {{}}"));
            want.push((name, lines.len() as u32));
        }
        let text = lines.join("\n") + "\n";
        let got: Vec<(String, u32)> = scan(Lang::Rust, &text)
            .defs
            .into_iter()
            .filter(|d| d.name.starts_with("marker_"))
            .map(|d| (d.name, d.line))
            .collect();
        assert_eq!(got, want, "round {round}:\n{text}");
        assert_eq!(wrong_lines(Lang::Rust, &text), [], "round {round}:\n{text}");
    }
}

/// The code files under `dir` (a folder of the repository), the product's own exclusions and size cap.
fn code_files_under(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let Ok(kind) = entry.file_type() else { continue };
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if kind.is_symlink() {
            continue;
        }
        if kind.is_dir() {
            if !matches!(name.as_str(), "target" | ".git" | "node_modules") {
                code_files_under(&path, out);
            }
        } else if matches!(crate::symbols_source::classify(&name), crate::symbols_source::FileClass::Code(_)) {
            out.push(path);
        }
    }
}

#[test]
fn the_line_of_every_definition_in_real_files_of_every_language_holds_its_name() {
    // A bug of this kind (a literal that ate a newline: every line after it was one too few) showed on
    // 2,700 of the 153,000 Rust definitions of the Azul tree and on none of the fixtures; the real files
    // are the ones that have every kind of literal.
    // The repository this crate is in (SYMBOLS_TREE names another checkout, as for the timed scan).
    let root = std::env::var_os("SYMBOLS_TREE")
        .map_or_else(|| std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."), std::path::PathBuf::from);
    let mut files = Vec::new();
    for dir in ["core/src", "css/src", "examples/c", "examples/cpp", "examples/python", "scripts", "tests/e2e"] {
        let path = root.join(dir);
        assert!(path.is_dir(), "{dir} is not a folder of the repository: the sample moved");
        code_files_under(&path, &mut files);
    }
    files.sort();
    let (mut checked, mut wrong): (usize, Vec<String>) = (0, Vec::new());
    // Rust: the last line of a definition's extent ends it with a brace, a semicolon or a bracket.
    let mut open_ends: Vec<String> = Vec::new();
    for path in &files {
        let rel = path.strip_prefix(&root).unwrap_or(path).to_string_lossy().replace('\\', "/");
        let Ok(bytes) = std::fs::read(path) else { continue };
        if bytes.len() > crate::symbols_source::MAX_FILE_BYTES || crate::symbols_source::inspect(&bytes).is_some() {
            continue;
        }
        let crate::symbols_source::FileClass::Code(lang) = crate::symbols_source::classify(&rel) else { continue };
        let scanned = scan_code(lang, &rel, &bytes, &ScanCaps::default());
        let text = String::from_utf8_lossy(&bytes);
        let lines: Vec<&str> = text.split('\n').collect();
        for def in &scanned.defs {
            checked += 1;
            let holds = lines.get((def.line as usize).wrapping_sub(1)).is_some_and(|l| l.contains(def.name.as_str()));
            if lang == Lang::Rust {
                let last = (def.line as usize + def.lines as usize).saturating_sub(2);
                let end = lines.get(last).copied().unwrap_or("");
                if !(end.contains('}') || end.contains(';') || end.contains(')')) && open_ends.len() < 10 {
                    open_ends.push(format!("{rel}:{} {} +{}", def.line, def.name, def.lines));
                }
            }
            if !holds && wrong.len() < 10 {
                wrong.push(format!("{rel}:{} {}", def.line, def.name));
            } else if !holds {
                wrong.push(String::new());
            }
        }
    }
    assert!(files.len() > 300 && checked > 10_000, "the sample is too small to mean anything: {} files, {checked} definitions", files.len());
    let count = wrong.len();
    wrong.retain(|w| !w.is_empty());
    assert_eq!(count, 0, "{count} of {checked} definitions have a line that does not hold their name, the first: {wrong:?}");
    assert_eq!(open_ends, Vec::<String>::new(), "definitions whose extent stops before its end (a brace, a semicolon, a bracket)");
}
