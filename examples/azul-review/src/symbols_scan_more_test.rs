//! The scanners of JavaScript and TypeScript, Go, Java, Kotlin, C#, Swift and shell on fixtures with
//! known lines. No window, no azul.

use super::*;
use crate::{symbols::Kind, symbols_source::Lang};

type Row = (String, Kind, u32, u32, Option<String>);

fn d(name: &str, kind: Kind, line: u32, lines: u32, parent: Option<&str>) -> Row {
    (name.to_string(), kind, line, lines, parent.map(str::to_string))
}

fn src(lines: &[&str]) -> String {
    let mut s = lines.join("\n");
    s.push('\n');
    s
}

fn scan(lang: Lang, lines: &[&str]) -> Scanned {
    scan_code(lang, "x.src", src(lines).as_bytes(), &ScanCaps::default())
}

fn rows(lang: Lang, lines: &[&str]) -> Vec<Row> {
    scan(lang, lines).defs.into_iter().map(|def| (def.name, def.kind, def.line, def.lines, def.parent)).collect()
}

#[test]
fn javascript_classes_functions_arrow_functions_constants_and_methods() {
    let got = rows(
        Lang::JavaScript,
        &[
            "import React from \"react\";",
            "import { a, b } from './util';",
            "const fs = require('fs');",
            "// class InComment {}",
            "const TEMPLATE = `class InTemplate { ${x} } function nope() {}`;",
            "export const MAX_ITEMS = 10;",
            "const lower = 3;",
            "export class Reader extends Base {",
            "  constructor(path) { this.path = path; }",
            "  async read() {",
            "    return 1;",
            "  }",
            "  static make() { return new Reader(); }",
            "}",
            "function plain(a, b) {",
            "  return a / b;",
            "}",
            "export default function* gen() {}",
            "const arrow = (a) => a + 1;",
            "const asyncArrow = async x => x;",
            "const fn = function () {};",
            "const re = /class NotAClass {/g;",
            "function after() {}",
        ],
    );
    let want = vec![
        d("MAX_ITEMS", Kind::Const, 6, 1, None),
        d("Reader", Kind::Class, 8, 7, None),
        d("constructor", Kind::Method, 9, 1, Some("Reader")),
        d("read", Kind::Method, 10, 3, Some("Reader")),
        d("make", Kind::Method, 13, 1, Some("Reader")),
        d("plain", Kind::Fn, 15, 3, None),
        d("gen", Kind::Fn, 18, 1, None),
        d("arrow", Kind::Fn, 19, 1, None),
        d("asyncArrow", Kind::Fn, 20, 1, None),
        d("fn", Kind::Fn, 21, 1, None),
        d("TEMPLATE", Kind::Const, 5, 1, None),
        d("after", Kind::Fn, 23, 1, None),
    ];
    let mut got = got;
    got.sort_by_key(|r| r.2);
    let mut want = want;
    want.sort_by_key(|r| r.2);
    assert_eq!(got, want);
    let s = scan(Lang::JavaScript, &["import React from \"react\";", "import { a } from './util';", "const fs = require('fs');", "import 'side-effect';"]);
    assert_eq!(s.imports, ["react", "./util", "fs", "side-effect"]);
}

#[test]
fn typescript_interfaces_types_enums_namespaces_and_typed_methods() {
    let got = rows(
        Lang::TypeScript,
        &[
            "export interface Options<T> {",
            "  name: string;",
            "}",
            "export type Id = string | number;",
            "enum Mode { A, B }",
            "declare namespace Api {",
            "  function call(): void;",
            "}",
            "export abstract class Store<T> implements Options<T> {",
            "  private items: T[] = [];",
            "  public get(id: Id): T | undefined {",
            "    return undefined;",
            "  }",
            "}",
        ],
    );
    let want = vec![
        d("Options", Kind::Interface, 1, 3, None),
        d("Id", Kind::Type, 4, 1, None),
        d("Mode", Kind::Enum, 5, 1, None),
        d("Api", Kind::Module, 6, 3, None),
        d("call", Kind::Fn, 7, 1, Some("Api")),
        d("Store", Kind::Class, 9, 6, None),
        d("get", Kind::Method, 11, 3, Some("Store")),
    ];
    assert_eq!(got, want);
}

#[test]
fn go_types_functions_methods_constants_and_imports() {
    let s = scan(
        Lang::Go,
        &[
            "package store",
            "",
            "import (",
            "\t\"fmt\"",
            "\tstr \"strings\"",
            ")",
            "import \"os\"",
            "",
            "// Reader reads.",
            "type Reader struct {",
            "\tpath string",
            "}",
            "type Closer interface { Close() error }",
            "type ID int",
            "const MaxItems = 10",
            "const (",
            "\tA = iota",
            "\tB",
            ")",
            "var global = 1",
            "func NewReader(path string) *Reader {",
            "\treturn &Reader{path: path}",
            "}",
            "func (r *Reader) Read() int {",
            "\treturn 1",
            "}",
            "func (r Reader) helper() {}",
            "var raw = `func InRaw() {}`",
            "func after() {}",
        ],
    );
    let got: Vec<Row> = s.defs.into_iter().map(|def| (def.name, def.kind, def.line, def.lines, def.parent)).collect();
    let want = vec![
        d("Reader", Kind::Struct, 10, 3, None),
        d("Closer", Kind::Interface, 13, 1, None),
        d("ID", Kind::Type, 14, 1, None),
        d("MaxItems", Kind::Const, 15, 1, None),
        d("A", Kind::Const, 17, 1, None),
        d("B", Kind::Const, 18, 1, None),
        d("global", Kind::Static, 20, 1, None),
        d("NewReader", Kind::Fn, 21, 3, None),
        d("Read", Kind::Method, 24, 3, Some("Reader")),
        d("helper", Kind::Method, 27, 1, Some("Reader")),
        d("raw", Kind::Static, 28, 1, None),
        d("after", Kind::Fn, 29, 1, None),
    ];
    assert_eq!(got, want);
    assert_eq!(s.imports, ["fmt", "strings", "os"]);
}

#[test]
fn java_types_methods_constructors_and_imports() {
    let s = scan(
        Lang::Java,
        &[
            "package a.b;",
            "import java.util.List;",
            "import static java.lang.Math.max;",
            "/** The reader. class InComment { } */",
            "public class Reader extends Base implements Closeable {",
            "    private final String path = \"class InString { }\";",
            "    public Reader(String path) {",
            "        this.path = path;",
            "    }",
            "    public int read(int n) throws IOException {",
            "        if (n > 0) { return n; }",
            "        return 0;",
            "    }",
            "    abstract void gone();",
            "    static class Inner {",
            "        void deep() {}",
            "    }",
            "}",
            "interface Closer { void close(); }",
            "enum Mode { A, B }",
            "record Pair(int a, int b) {}",
        ],
    );
    let got: Vec<Row> = s.defs.into_iter().map(|def| (def.name, def.kind, def.line, def.lines, def.parent)).collect();
    let want = vec![
        d("Reader", Kind::Class, 5, 14, None),
        d("Reader", Kind::Method, 7, 3, Some("Reader")),
        d("read", Kind::Method, 10, 4, Some("Reader")),
        d("gone", Kind::Method, 14, 1, Some("Reader")),
        d("Inner", Kind::Class, 15, 3, Some("Reader")),
        d("deep", Kind::Method, 16, 1, Some("Inner")),
        d("Closer", Kind::Interface, 19, 1, None),
        d("close", Kind::Method, 19, 1, Some("Closer")),
        d("Mode", Kind::Enum, 20, 1, None),
        d("Pair", Kind::Class, 21, 1, None),
    ];
    assert_eq!(got, want);
    assert_eq!(s.imports, ["java.util.List", "java.lang.Math.max"]);
}

#[test]
fn kotlin_swift_and_csharp_declarations() {
    let kt = rows(
        Lang::Kotlin,
        &[
            "package a",
            "import a.b.C",
            "data class Point(val x: Int, val y: Int)",
            "interface Shape { fun area(): Double }",
            "object Registry {",
            "    fun find(name: String): Shape? = null",
            "}",
            "fun top(a: Int): Int {",
            "    return a",
            "}",
            "fun String.shout() = this",
            "typealias Id = Long",
        ],
    );
    assert_eq!(
        kt,
        vec![
            d("Point", Kind::Class, 3, 1, None),
            d("Shape", Kind::Interface, 4, 1, None),
            d("area", Kind::Method, 4, 1, Some("Shape")),
            d("Registry", Kind::Class, 5, 3, None),
            d("find", Kind::Method, 6, 1, Some("Registry")),
            d("top", Kind::Fn, 8, 3, None),
            d("shout", Kind::Fn, 11, 1, None),
            d("Id", Kind::Type, 12, 1, None),
        ]
    );
    let swift = rows(
        Lang::Swift,
        &[
            "import Foundation",
            "struct Point {",
            "    var x: Int",
            "    func norm() -> Int { return x }",
            "}",
            "protocol Shape { func area() -> Double }",
            "enum Mode { case a, b }",
            "extension Point {",
            "    func double() -> Point { return self }",
            "}",
            "func free(_ a: Int) -> Int {",
            "    return a",
            "}",
        ],
    );
    assert_eq!(
        swift,
        vec![
            d("Point", Kind::Struct, 2, 4, None),
            d("norm", Kind::Method, 4, 1, Some("Point")),
            d("Shape", Kind::Interface, 6, 1, None),
            d("area", Kind::Method, 6, 1, Some("Shape")),
            d("Mode", Kind::Enum, 7, 1, None),
            d("double", Kind::Method, 9, 1, Some("Point")),
            d("free", Kind::Fn, 11, 3, None),
        ]
    );
    let cs = rows(
        Lang::CSharp,
        &[
            "using System;",
            "namespace Demo.App {",
            "    public class Reader {",
            "        public string Path { get; set; }",
            "        public int Read(int n) {",
            "            return n;",
            "        }",
            "        private static readonly string Raw = @\"class InVerbatim { }\";",
            "    }",
            "    public struct Pair { }",
            "    public interface ICloser { void Close(); }",
            "}",
        ],
    );
    assert_eq!(
        cs,
        vec![
            d("App", Kind::Module, 2, 11, None),
            d("Reader", Kind::Class, 3, 7, Some("App")),
            d("Read", Kind::Method, 5, 3, Some("Reader")),
            d("Pair", Kind::Struct, 10, 1, Some("App")),
            d("ICloser", Kind::Interface, 11, 1, Some("App")),
            d("Close", Kind::Method, 11, 1, Some("ICloser")),
        ]
    );
}

#[test]
fn shell_functions_in_both_spellings_and_what_is_sourced() {
    let s = scan(
        Lang::Shell,
        &[
            "#!/bin/bash",
            "# function fake() {",
            "source ./lib.sh",
            ". /etc/profile",
            "build() {",
            "  echo \"function nope() {\"",
            "}",
            "function deploy {",
            "  build",
            "}",
            "function other() { :; }",
            "x=1",
        ],
    );
    let got: Vec<Row> = s.defs.into_iter().map(|def| (def.name, def.kind, def.line, def.lines, def.parent)).collect();
    assert_eq!(got, vec![d("build", Kind::Fn, 5, 3, None), d("deploy", Kind::Fn, 8, 3, None), d("other", Kind::Fn, 11, 1, None)]);
    assert_eq!(s.imports, ["./lib.sh", "/etc/profile"]);
}

#[test]
fn hostile_input_of_every_one_of_these_languages_never_panics() {
    let nasty: Vec<String> = vec![
        "class".into(),
        "function".into(),
        "func (".into(),
        "func (r".into(),
        "import (".into(),
        "type".into(),
        "const (".into(),
        "interface {".into(),
        "`never closed".into(),
        "\"\"\"never closed".into(),
        "/regex/never closed".into(),
        "{".repeat(20_000),
        "(".repeat(20_000),
        "class A { ".repeat(5_000),
        "func f(".repeat(5_000),
        "a ".repeat(500_000),
        "@\"verbatim never closed".into(),
        "function () {".into(),
        "() => {".into(),
        "name() {".into(),
        "function".repeat(10_000),
    ];
    for lang in [Lang::JavaScript, Lang::TypeScript, Lang::Go, Lang::Java, Lang::Kotlin, Lang::CSharp, Lang::Swift, Lang::Shell] {
        for text in &nasty {
            let _ = scan_code(lang, "x", text.as_bytes(), &ScanCaps::default());
        }
        let bytes: Vec<u8> = (0..30_000u32).map(|i| (i.wrapping_mul(2_654_435_761) >> 11) as u8).collect();
        let _ = scan_code(lang, "x", &bytes, &ScanCaps::default());
    }
}
