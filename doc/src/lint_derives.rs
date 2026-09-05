//! Derive-parity lint: every capability a class DECLARES in api.json must be
//! reachable from every binding that ships that class.
//!
//! WHY THIS EXISTS
//! ---------------
//! `api.json` records a `derive` list per class — 1889 of them. That list is
//! not documentation: `IrBuilder::build_trait_functions` turns it into real
//! C-ABI exports (`Az{T}_partialEq`, `Az{T}_partialCmp`, `Az{T}_cmp`,
//! `Az{T}_hash`, `Az{T}_toDbgString`, `Az{T}_default`, `Az{T}_clone`), and the
//! shipped `libazul` exports every one of them. Whether a *binding* then hands
//! those exports to its users is a separate decision made independently in each
//! of the ~38 language emitters, and until this lint existed nothing compared
//! the two. The failure mode is silent and one-directional: the export is in
//! the library, the caller has no way to name it.
//!
//! That is not hypothetical. `dll_api_external.rs` (the `link-dynamic` Rust
//! binding) declared all 1456 `_partialEq` externs and implemented `PartialEq`
//! for none of them, so `MsgBox::yes_no(..) == YesNo::Yes` did not compile for
//! any dynamically-linked caller while the identical statically-linked build
//! was fine.
//!
//! WHAT IT CHECKS, AND HOW STUPID IT IS ON PURPOSE
//! ----------------------------------------------
//! Name-based presence, nothing more. For each (binding, class, derive) it asks
//! "does this binding's own generated text contain a name that gives the caller
//! this capability for this class". It does not parse, type-check, or run
//! anything, and it cannot: the artifacts are 38 different languages. A false
//! PASS therefore needs a name to exist while being unusable, which is a much
//! rarer bug than the one this catches, and the compile gates catch it.
//!
//! THE ONE JUDGEMENT CALL, STATED
//! ------------------------------
//! A derive counts as honoured when the binding's OWN emitted text names the
//! entry point (or a native equivalent). Being able to reach the C symbol
//! through the language's raw-C escape hatch does NOT count — `#include
//! "azul.h"` from a C++ header, `C.AzFoo_partialEq` through cgo, `@cImport` in
//! Zig. Under any other rule every binding that can call C would pass by
//! definition and the lint would measure nothing.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

use crate::api::ApiData;

/// The nine things an api.json `derive` list can say.
pub const DERIVES: &[&str] = &[
    "Debug",
    "Clone",
    "Copy",
    "PartialEq",
    "Eq",
    "PartialOrd",
    "Ord",
    "Hash",
    "Default",
];

/// What a binding must contain for one declared derive.
#[derive(Clone, Copy)]
pub enum Expect {
    /// Honoured iff any pattern occurs in the binding's own text, with `{T}`
    /// replaced by the class name as that binding spells it (prefix applied).
    Marker(&'static [&'static str]),
    /// Honoured iff any `markers` entry occurs inside the class's own block —
    /// the text from the first `start` match to the next `block_end`. Needed
    /// wherever the native construct does not carry the class name in it
    /// (`__eq__`, `uint64_t hash() const`), which a flat substring search
    /// cannot attribute to a class.
    Block {
        start: &'static [&'static str],
        markers: &'static [&'static str],
    },
    /// Honoured iff ANY alternative is. Needed where one binding gives the same
    /// capability two different shapes for two different kinds of class - C++
    /// puts `toDbgString()` inside a wrapper class for a struct, and offers
    /// `azul::toDbgString(const AzAlertKind&)` as a free function for an enum,
    /// which has no class to put it in.
    Either(&'static [Expect]),

    /// The language has no equivalent AND forwarding the ABI function would be
    /// meaningless. Reported as N/A, never as a gap.
    NotApplicable(&'static str),
    /// No mapping could be decided. Excluded from the pass/fail verdict and
    /// reported separately, so an undecided case can never masquerade as a pass.
    Unmapped(&'static str),
}

/// Markers shared by every binding that forwards the C ABI by symbol name.
const ABI_DEBUG: Expect = Expect::Marker(&["{T}_toDbgString"]);
const ABI_CLONE: Expect = Expect::Marker(&["{T}_clone"]);
const ABI_PARTIAL_EQ: Expect = Expect::Marker(&["{T}_partialEq"]);
const ABI_PARTIAL_ORD: Expect = Expect::Marker(&["{T}_partialCmp"]);
const ABI_ORD: Expect = Expect::Marker(&["{T}_cmp"]);
const ABI_HASH: Expect = Expect::Marker(&["{T}_hash"]);
const ABI_DEFAULT: Expect = Expect::Marker(&["{T}_default"]);

/// `Copy` is the one derive with no surface anywhere, by design.
///
/// It has no ABI entry point: `build_trait_functions` emits none, because a
/// `Copy` value is copied by assignment in every target language and needs no
/// call. In the Rust mirrors it is emitted as `#[derive(Copy)]` and is what
/// makes the memtest size/alignment gate meaningful. Counting it as a gap in 38
/// bindings would be noise, so it is N/A everywhere.
const COPY_NA: Expect =
    Expect::NotApplicable("no ABI entry point exists or is needed: Copy values are copied by assignment");

/// `Eq` outside Rust.
///
/// `Eq` is a Rust marker trait asserting that `PartialEq` is reflexive. It has
/// no runtime surface distinct from equality itself, and `build_trait_functions`
/// emits no `Az{T}_eq`. The equality entry point is already accounted for under
/// `PartialEq`, so counting `Eq` again would double-count one export.
const EQ_NA: Expect = Expect::NotApplicable(
    "Rust marker trait with no runtime surface of its own; the equality entry point is counted under PartialEq",
);

/// The profile used by every binding that forwards the C ABI by symbol name —
/// which is most of them, because the generated FFI declarations spell the C
/// name out.
const ABI_PROFILE: &[(&str, Expect)] = &[
    ("Debug", ABI_DEBUG),
    ("Clone", ABI_CLONE),
    ("Copy", COPY_NA),
    ("PartialEq", ABI_PARTIAL_EQ),
    ("Eq", EQ_NA),
    ("PartialOrd", ABI_PARTIAL_ORD),
    ("Ord", ABI_ORD),
    ("Hash", ABI_HASH),
    ("Default", ABI_DEFAULT),
];

/// The Rust mirrors (`dll_api_internal.rs`, `dll_api_external.rs`, `memtest.rs`).
/// These do not merely declare the ABI, they must implement the trait, so the
/// marker is the impl header — declaring `Az{T}_partialEq` and never writing
/// `impl PartialEq` is exactly the bug this lint was written for.
const RUST_MIRROR_PROFILE: &[(&str, Expect)] = &[
    ("Debug", Expect::Marker(&["impl core::fmt::Debug for {T} {"])),
    (
        "Clone",
        Expect::Marker(&["impl Clone for {T} {", "#[derive(Clone)]\n#[repr(C)]\npub struct {T} ", "#[derive(Clone)]\n#[repr(C)]\npub enum {T} "]),
    ),
    ("Copy", COPY_NA),
    ("PartialEq", Expect::Marker(&["impl PartialEq for {T} {"])),
    ("Eq", Expect::Marker(&["impl Eq for {T} {"])),
    ("PartialOrd", Expect::Marker(&["impl PartialOrd for {T} {"])),
    ("Ord", Expect::Marker(&["impl Ord for {T} {"])),
    ("Hash", Expect::Marker(&["impl core::hash::Hash for {T} {"])),
    ("Default", Expect::Marker(&["impl Default for {T} {"])),
];

/// The PyO3 extension. The Python-visible surface is the `#[pymethods]` block,
/// not the Rust trait impls on the embedded mirror types: `python_api.rs`
/// carries `impl PartialEq for AzStyleCursor` and `impl core::hash::Hash`, and
/// neither is callable from Python without a `__eq__` / `__hash__`.
const PYO3_PROFILE: &[(&str, Expect)] = &[
    (
        "Debug",
        Expect::Block { start: PY_BLOCK, markers: &["fn __repr__", "fn __str__"] },
    ),
    (
        "Clone",
        Expect::Block { start: PY_BLOCK, markers: &["fn __copy__", "fn __deepcopy__", "fn clone("] },
    ),
    ("Copy", COPY_NA),
    (
        "PartialEq",
        Expect::Block { start: PY_BLOCK, markers: &["fn __eq__", "fn __richcmp__"] },
    ),
    ("Eq", EQ_NA),
    (
        "PartialOrd",
        Expect::Block { start: PY_BLOCK, markers: &["fn __lt__", "fn __richcmp__"] },
    ),
    (
        "Ord",
        Expect::Block { start: PY_BLOCK, markers: &["fn __lt__", "fn __richcmp__"] },
    ),
    ("Hash", Expect::Block { start: PY_BLOCK, markers: &["fn __hash__"] }),
    ("Default", Expect::Block { start: PY_BLOCK, markers: &["fn default("] }),
];

const PY_BLOCK: &[&str] = &["#[pymethods]\nimpl {T} {"];

/// C++ (`azul03.hpp` … `azul23.hpp`, `azul.cppm`).
///
/// The headers open with `extern "C" { #include "azul.h" }`, which under this
/// lint's stated rule is the raw-C escape hatch and does not count. What counts
/// is the wrapper class's own method — `bool partialEq(const MonitorId& b)
/// const;`. Those methods carry the class name in their parameter for the
/// comparisons, and not at all for `toDbgString`/`hash`, so the latter two are
/// matched inside the wrapper class block.
const CPP_BLOCK: &[&str] = &["class {C} {"];

/// The two shapes a C++ capability can take.
///
/// A struct becomes `class Foo { .. String toDbgString() const; .. }`, so the
/// evidence is a method inside that class's own block. An ENUM or tagged union
/// becomes `using Foo = AzFoo;` or `namespace Foo { constants }` over the raw C
/// type - there is no class to put a method in, so the same capability is a
/// free function overloaded on the argument type,
/// `azul::toDbgString(const AzFoo&)`. Both spell the class out, so both are
/// attributable; neither is the raw-C escape hatch (`AzFoo_toDbgString` from
/// the `#include`d header still does not count).
const CPP_PROFILE: &[(&str, Expect)] = &[
    (
        "Debug",
        Expect::Either(&[
            Expect::Block { start: CPP_BLOCK, markers: &["toDbgString() const", "operator<<"] },
            Expect::Marker(&["toDbgString(const {T}&"]),
        ]),
    ),
    (
        "Clone",
        Expect::Either(&[
            Expect::Block { start: CPP_BLOCK, markers: &["clone() const"] },
            Expect::Marker(&["clone(const {T}&"]),
        ]),
    ),
    ("Copy", COPY_NA),
    (
        "PartialEq",
        Expect::Marker(&["partialEq(const {C}&", "operator==(const {C}&", "partialEq(const {T}&"]),
    ),
    ("Eq", EQ_NA),
    (
        "PartialOrd",
        Expect::Marker(&[
            "partialCmp(const {C}&",
            "operator<(const {C}&",
            "partialCmp(const {T}&",
        ]),
    ),
    (
        "Ord",
        Expect::Marker(&["cmp(const {C}&", "operator<=>(const {C}&", "cmp(const {T}&"]),
    ),
    (
        "Hash",
        Expect::Either(&[
            Expect::Block { start: CPP_BLOCK, markers: &["hash() const", "struct hash<"] },
            Expect::Marker(&["hash(const {T}&"]),
        ]),
    ),
    ("Default", Expect::Marker(&["static {C} default_()", "defaultOf<{T}>()"])),
];

/// Go. cgo makes `C.Az{T}_partialEq` reachable, which does not count under this
/// lint's rule; the binding has to emit a Go-level name.
const GO_PROFILE: &[(&str, Expect)] = &[
    ("Debug", Expect::Marker(&["{T}_toDbgString", "{T}) String()"])),
    ("Clone", Expect::Marker(&["{T}_clone", "{T}) Clone()"])),
    ("Copy", COPY_NA),
    ("PartialEq", Expect::Marker(&["{T}_partialEq", "{T}) Equals("])),
    ("Eq", EQ_NA),
    ("PartialOrd", Expect::Marker(&["{T}_partialCmp", "{T}) PartialCmp("])),
    ("Ord", Expect::Marker(&["{T}_cmp", "{T}) Cmp("])),
    ("Hash", Expect::Marker(&["{T}_hash", "{T}) Hash()"])),
    ("Default", Expect::Marker(&["{T}_default", "{T}Default()"])),
];

/// Zig. `@cImport` makes `C.Az{T}_partialEq` reachable, which does not count.
const ZIG_PROFILE: &[(&str, Expect)] = &[
    ("Debug", Expect::Marker(&["{T}_toDbgString"])),
    ("Clone", Expect::Marker(&["{T}_clone"])),
    ("Copy", COPY_NA),
    ("PartialEq", Expect::Marker(&["{T}_partialEq"])),
    ("Eq", EQ_NA),
    ("PartialOrd", Expect::Marker(&["{T}_partialCmp"])),
    ("Ord", Expect::Marker(&["{T}_cmp"])),
    ("Hash", Expect::Marker(&["{T}_hash"])),
    ("Default", Expect::Marker(&["{T}_default"])),
];

/// A binding: what it is called, which generated files are ITS text, how it
/// spells a class name, and what each derive must look like in it.
pub struct Binding {
    pub name: &'static str,
    /// Paths relative to `target/codegen`. A directory is walked recursively.
    pub files: &'static [&'static str],
    /// How this binding spells `StyleCursor` (`"Az"` for the prefixed ones).
    pub prefix: &'static str,
    /// Where a `Expect::Block` search stops. Empty means "scan 4 KiB forward".
    pub block_end: &'static str,
    /// `derive` -> what proves it. Every entry of [`DERIVES`] must be present.
    pub expects: &'static [(&'static str, Expect)],
    /// One line for the report: what shape this binding is.
    pub note: &'static str,
}

/// Every binding `azul-doc codegen all` writes. The list is deliberately the
/// full set of emitted artifacts, not the subset anyone remembers: a binding
/// missing from here is a blind spot, which is the same failure this lint
/// exists to prevent. The six C++ standards are separate entries because they
/// do NOT agree — the C++11 and C++14 emitters filter trait functions out and
/// the other four do not, and one merged "cpp" row would hide that.
pub const BINDINGS: &[Binding] = &[
    Binding { name: "rust-internal (static)", files: &["dll_api_internal.rs"], prefix: "Az", block_end: "", expects: RUST_MIRROR_PROFILE, note: "mirror types + impls delegating to the real crate" },
    Binding { name: "rust-dynamic (dynamic)", files: &["dll_api_external.rs"], prefix: "Az", block_end: "", expects: RUST_MIRROR_PROFILE, note: "mirror types + extern decls; impls must call the ABI" },
    Binding { name: "rust-public (azul.rs)", files: &["azul.rs"], prefix: "", block_end: "", expects: RUST_MIRROR_PROFILE, note: "standalone unprefixed Rust surface" },
    Binding { name: "memtest", files: &["memtest.rs"], prefix: "Az", block_end: "", expects: RUST_MIRROR_PROFILE, note: "layout/size tests over the mirror types" },
    Binding { name: "c", files: &["azul.h"], prefix: "Az", block_end: "", expects: ABI_PROFILE, note: "C header" },
    Binding { name: "cpp03", files: &["azul03.hpp"], prefix: "Az", block_end: "\n};", expects: CPP_PROFILE, note: "C++03 wrapper classes" },
    Binding { name: "cpp11", files: &["azul11.hpp"], prefix: "Az", block_end: "\n};", expects: CPP_PROFILE, note: "C++11 wrapper classes" },
    Binding { name: "cpp14", files: &["azul14.hpp"], prefix: "Az", block_end: "\n};", expects: CPP_PROFILE, note: "C++14 wrapper classes" },
    Binding { name: "cpp17", files: &["azul17.hpp"], prefix: "Az", block_end: "\n};", expects: CPP_PROFILE, note: "C++17 wrapper classes" },
    Binding { name: "cpp20", files: &["azul20.hpp"], prefix: "Az", block_end: "\n};", expects: CPP_PROFILE, note: "C++20 wrapper classes" },
    Binding { name: "cpp23", files: &["azul23.hpp"], prefix: "Az", block_end: "\n};", expects: CPP_PROFILE, note: "C++23 wrapper classes" },
    Binding { name: "csharp", files: &["Azul.cs"], prefix: "Az", block_end: "", expects: ABI_PROFILE, note: "P/Invoke externs + wrapper classes" },
    Binding { name: "python", files: &["python_api.rs"], prefix: "Az", block_end: "\n}\n", expects: PYO3_PROFILE, note: "PyO3 native extension" },
    Binding { name: "ruby", files: &["azul.rb"], prefix: "Az", block_end: "", expects: ABI_PROFILE, note: "Fiddle/FFI attach_function" },
    Binding { name: "lua", files: &["azul.lua"], prefix: "Az", block_end: "", expects: ABI_PROFILE, note: "LuaJIT ffi.cdef" },
    Binding { name: "pascal", files: &["azul.pas"], prefix: "Az", block_end: "", expects: ABI_PROFILE, note: "external declarations" },
    Binding { name: "ada", files: &["azul.ads", "azul.adb"], prefix: "Az", block_end: "", expects: ABI_PROFILE, note: "pragma Import (C, .., \"Az..\")" },
    Binding { name: "freebasic", files: &["azul.bi"], prefix: "Az", block_end: "", expects: ABI_PROFILE, note: "Declare Function .. Alias" },
    Binding { name: "zig", files: &["azul.zig"], prefix: "Az", block_end: "", expects: ZIG_PROFILE, note: "@cImport wrapper structs" },
    Binding { name: "powershell", files: &["Azul.psm1"], prefix: "Az", block_end: "", expects: ABI_PROFILE, note: "Add-Type with the C# source embedded" },
    Binding { name: "php", files: &["Azul.php"], prefix: "Az", block_end: "", expects: ABI_PROFILE, note: "FFI::cdef over the whole header" },
    Binding { name: "php-ext", files: &["php_api.rs"], prefix: "Az", block_end: "", expects: ABI_PROFILE, note: "Zend native extension (curated 5-class surface)" },
    Binding { name: "perl", files: &["Azul.pm"], prefix: "Az", block_end: "", expects: ABI_PROFILE, note: "FFI::Platypus attach" },
    Binding { name: "ocaml", files: &["azul.mli"], prefix: "Az", block_end: "", expects: ABI_PROFILE, note: "the .mli SEALS the module - only what it lists is reachable" },
    Binding { name: "haskell", files: &["haskell"], prefix: "Az", block_end: "", expects: ABI_PROFILE, note: "foreign import ccall + idiomatic instances" },
    Binding { name: "java", files: &["java"], prefix: "Az", block_end: "", expects: ABI_PROFILE, note: "JNA/Panama" },
    Binding { name: "kotlin", files: &["kotlin"], prefix: "Az", block_end: "", expects: ABI_PROFILE, note: "JNA" },
    Binding { name: "fortran", files: &["azul.f90"], prefix: "Az", block_end: "", expects: ABI_PROFILE, note: "iso_c_binding interfaces" },
    Binding { name: "go", files: &["go"], prefix: "Az", block_end: "", expects: GO_PROFILE, note: "cgo" },
    Binding { name: "lisp", files: &["azul.lisp"], prefix: "Az", block_end: "", expects: ABI_PROFILE, note: "CFFI defcfun" },
    Binding { name: "smalltalk", files: &["Azul.st", "BaselineOfAzul.st"], prefix: "Az", block_end: "", expects: ABI_PROFILE, note: "Pharo FFI" },
    Binding { name: "algol68", files: &["azul.a68"], prefix: "Az", block_end: "", expects: ABI_PROFILE, note: "Algol 68 Genie" },
    Binding { name: "cobol", files: &["azul.cpy"], prefix: "Az", block_end: "", expects: ABI_PROFILE, note: "GnuCOBOL copybook" },
    Binding { name: "vb6", files: &["vb6"], prefix: "Az", block_end: "", expects: ABI_PROFILE, note: "Declare Function" },
    Binding { name: "node", files: &["node"], prefix: "Az", block_end: "", expects: ABI_PROFILE, note: "koffi/ffi-napi" },
    Binding { name: "crystal", files: &["azul.cr"], prefix: "Az", block_end: "", expects: ABI_PROFILE, note: "lib binding" },
    Binding { name: "d", files: &["azul.d"], prefix: "Az", block_end: "", expects: ABI_PROFILE, note: "extern(C)" },
    Binding { name: "julia", files: &["azul.jl"], prefix: "Az", block_end: "", expects: ABI_PROFILE, note: "ccall" },
    Binding { name: "nim", files: &["azul.nim"], prefix: "Az", block_end: "", expects: ABI_PROFILE, note: "importc" },
    Binding { name: "odin", files: &["azul.odin"], prefix: "Az", block_end: "", expects: ABI_PROFILE, note: "foreign import" },
    Binding { name: "racket", files: &["azul.rkt"], prefix: "Az", block_end: "", expects: ABI_PROFILE, note: "ffi/unsafe" },
    Binding { name: "red", files: &["azul.reds"], prefix: "Az", block_end: "", expects: ABI_PROFILE, note: "Red/System" },
    Binding { name: "swift", files: &["azul.swift"], prefix: "Az", block_end: "", expects: ABI_PROFILE, note: "module map" },
    Binding { name: "v", files: &["azul.v"], prefix: "Az", block_end: "", expects: ABI_PROFILE, note: "V C interop" },
];

/// One (binding, class, derive) that should have been reachable and is not.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Gap {
    pub binding: String,
    pub class: String,
    pub derive: String,
}

/// Per-binding tally.
#[derive(Clone, Debug, Default)]
pub struct BindingReport {
    pub binding: String,
    pub note: String,
    /// declared derive honoured by a name in this binding
    pub honoured: usize,
    /// declared derive with no name in this binding
    pub missing: usize,
    /// derive with no equivalent in this language, by construction
    pub not_applicable: usize,
    /// derive with no decided mapping; excluded from the verdict
    pub unmapped: usize,
    /// classes that carry a derive list but never appear in this binding at all
    pub classes_absent: usize,
    /// classes checked
    pub classes_checked: usize,
    /// per-derive missing counts, for the report
    pub missing_by_derive: BTreeMap<String, usize>,
    /// a few concrete examples, so a failure is actionable
    pub examples: Vec<Gap>,
}

/// Read every file of a binding into one blob. A directory is walked.
fn read_binding_text(codegen_dir: &Path, files: &[&str]) -> std::io::Result<String> {
    fn push_file(out: &mut String, p: &Path) {
        if let Ok(s) = fs::read_to_string(p) {
            out.push_str(&s);
            out.push('\n');
        }
    }
    fn walk(out: &mut String, p: &Path) {
        if p.is_dir() {
            let mut entries: Vec<_> = match fs::read_dir(p) {
                Ok(rd) => rd.filter_map(|e| e.ok()).map(|e| e.path()).collect(),
                Err(_) => return,
            };
            entries.sort();
            for e in entries {
                walk(out, &e);
            }
        } else {
            push_file(out, p);
        }
    }
    let mut out = String::new();
    for f in files {
        walk(&mut out, &codegen_dir.join(f));
    }
    Ok(out)
}

/// Every `Az{Ident}_{suffix}` in the text, as a `(ident, suffix)` set.
///
/// One pass per suffix rather than 1889 x 9 substring searches over a 12 MB
/// file, which is the difference between a lint that runs in `check` and one
/// nobody runs.
fn index_abi_symbols(text: &str, prefix: &str) -> BTreeSet<(String, String)> {
    const SUFFIXES: &[&str] = &[
        "_toDbgString",
        "_clone",
        "_partialEq",
        "_partialCmp",
        "_cmp",
        "_hash",
        "_default",
    ];
    let bytes = text.as_bytes();
    let mut found = BTreeSet::new();
    for suffix in SUFFIXES {
        let mut from = 0usize;
        while let Some(rel) = text[from..].find(suffix) {
            let at = from + rel;
            from = at + 1;
            // The suffix must END the identifier.
            let after = at + suffix.len();
            if bytes
                .get(after)
                .is_some_and(|c| c.is_ascii_alphanumeric() || *c == b'_')
            {
                continue;
            }
            // Walk back over the identifier that carries it.
            let mut start = at;
            while start > 0 {
                let c = bytes[start - 1];
                if c.is_ascii_alphanumeric() || c == b'_' {
                    start -= 1;
                } else {
                    break;
                }
            }
            let ident = &text[start..at];
            if let Some(class) = ident.strip_prefix(prefix) {
                if !class.is_empty() {
                    found.insert((class.to_string(), (*suffix).to_string()));
                }
            }
        }
    }
    found
}

/// The text of one class's own block: from the first `start` match to the next
/// `block_end` after it (or 4 KiB, when a binding declares no terminator).
///
/// This is what lets the lint attribute a construct that does NOT carry the
/// class name — `fn __hash__`, `uint64_t hash() const` — to the right class,
/// without parsing the language.
fn class_block<'a>(
    text: &'a str,
    starts: &[&str],
    block_end: &str,
    spelled: &str,
    bare: &str,
) -> Option<&'a str> {
    for pat in starts {
        let needle = pat.replace("{T}", spelled).replace("{C}", bare);
        if let Some(at) = text.find(&needle) {
            let from = at + needle.len();
            let rest = &text[from..];
            let len = if block_end.is_empty() {
                rest.len().min(4096)
            } else {
                rest.find(block_end).map(|e| e + block_end.len()).unwrap_or(rest.len().min(4096))
            };
            return Some(&rest[..len]);
        }
    }
    None
}

/// Is this class named in this binding at all?
///
/// Looser than [`contains_token`] on the right-hand side only: a following `_`
/// is accepted, because most bindings never write the bare type name — Ada
/// writes `pragma Import (C, .., "AzStyleCursor_toDbgString")`, Pascal writes
/// `PAzStyleCursor`. A following alphanumeric is still rejected, so
/// `AzStyleCursorValue` never proves `StyleCursor` is here.
fn class_is_named(text: &str, spelled: &str) -> bool {
    let bytes = text.as_bytes();
    let mut from = 0usize;
    while let Some(rel) = text[from..].find(spelled) {
        let at = from + rel;
        from = at + 1;
        let before_ok = at == 0 || {
            let c = bytes[at - 1];
            !(c.is_ascii_alphanumeric() || c == b'_')
        };
        let after = at + spelled.len();
        let after_ok = bytes
            .get(after)
            .is_none_or(|c| !c.is_ascii_alphanumeric() || *c == b'_');
        if before_ok && after_ok {
            return true;
        }
    }
    false
}

/// Does `text` contain `needle` as a standalone name (not glued to a longer
/// identifier on either side)? Prevents `AzStyleCursor_hash` from being counted
/// as evidence for `AzStyleCursorValue`.
fn contains_token(text: &str, needle: &str) -> bool {
    let bytes = text.as_bytes();
    let nb = needle.as_bytes();
    let mut from = 0usize;
    while let Some(rel) = text[from..].find(needle) {
        let at = from + rel;
        from = at + 1;
        let before_ok = at == 0 || {
            let c = bytes[at - 1];
            !(c.is_ascii_alphanumeric() || c == b'_')
        };
        let after = at + nb.len();
        let last_is_ident = nb
            .last()
            .is_some_and(|c| c.is_ascii_alphanumeric() || *c == b'_');
        let after_ok = !last_is_ident
            || bytes
                .get(after)
                .is_none_or(|c| !(c.is_ascii_alphanumeric() || *c == b'_'));
        if before_ok && after_ok {
            return true;
        }
    }
    false
}

/// Class name -> declared derives, from api.json.
///
/// GENERIC classes are excluded, because codegen itself excludes them:
/// `IrBuilder::build_trait_functions` filters on `generic_params.is_empty()`,
/// so `CssPropertyValue<T>`, `PhysicalSize<T>` and `PhysicalPosition<T>` never
/// get a `Az..._partialEq` of their own — their monomorphised aliases
/// (`AzStyleCursorValue`, …) carry their own api.json entries and ARE checked.
/// Demanding a per-class entry point for a type that has no single ABI identity
/// would be demanding something impossible.
pub fn declared_derives(api: &ApiData) -> BTreeMap<String, BTreeSet<String>> {
    let mut out: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (_ver, vd) in &api.0 {
        for (_module, md) in &vd.api {
            for (class, cd) in &md.classes {
                if cd
                    .generic_params
                    .as_ref()
                    .is_some_and(|g| !g.is_empty())
                {
                    continue;
                }
                if let Some(d) = &cd.derive {
                    out.entry(class.clone())
                        .or_default()
                        .extend(d.iter().cloned());
                }
            }
        }
    }
    out
}

/// Does this binding's text carry the evidence one [`Expect`] asks for?
///
/// Recursive only through [`Expect::Either`]; `NotApplicable` and `Unmapped`
/// are verdicts, not searches, and are handled by the caller (they return
/// `false` here so a mis-nested one can never be read as a pass).
fn evidence_found(
    expect: &Expect,
    text: &str,
    abi_index: &BTreeSet<(String, String)>,
    block_end: &str,
    spelled: &str,
    class: &str,
) -> bool {
    match expect {
        Expect::NotApplicable(_) | Expect::Unmapped(_) => false,
        Expect::Either(alts) => alts
            .iter()
            .any(|e| evidence_found(e, text, abi_index, block_end, spelled, class)),
        Expect::Marker(pats) => pats.iter().any(|p| {
            let needle = p.replace("{T}", spelled).replace("{C}", class);
            // The ABI-symbol case goes through the prebuilt index; anything
            // else is a plain token search.
            if let Some(suffix) = needle.strip_prefix(spelled) {
                if suffix.starts_with('_')
                    && suffix.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_')
                {
                    return abi_index.contains(&(class.to_string(), suffix.to_string()));
                }
            }
            contains_token(text, &needle)
        }),
        Expect::Block { start, markers } => class_block(text, start, block_end, spelled, class)
            .is_some_and(|b| markers.iter().any(|m| b.contains(*m))),
    }
}

/// Run the lint over every binding.
pub fn check(codegen_dir: &Path, api: &ApiData) -> anyhow::Result<Vec<BindingReport>> {
    let declared = declared_derives(api);
    if declared.is_empty() {
        anyhow::bail!("api.json carries no `derive` lists at all - refusing to report a vacuous pass");
    }
    let mut reports = Vec::new();

    for binding in BINDINGS {
        let text = read_binding_text(codegen_dir, binding.files)?;
        if text.is_empty() {
            anyhow::bail!(
                "binding `{}` has no generated text at {} ({:?}) - run `azul-doc codegen all` first",
                binding.name,
                codegen_dir.display(),
                binding.files
            );
        }

        let abi_index = index_abi_symbols(&text, binding.prefix);

        let mut rep = BindingReport {
            binding: binding.name.to_string(),
            note: binding.note.to_string(),
            ..Default::default()
        };

        for (class, derives) in &declared {
            // OWNER'S RULING, 2026-09-05: a `*VecDestructor` cannot be called
            // from Python or any other high-level binding. It exists only so
            // destruction behaves correctly, and its payload is a function
            // pointer - so it is to be treated as `*const c_void` and excluded
            // from generation. Its derives are therefore NOT missing in a
            // wrapper surface; they are not applicable there.
            //
            // The exception is the surfaces that DO carry it as a first-class
            // type and really do implement its derives today: the three Rust
            // mirrors, the layout test, and the C header. Excluding it there
            // would delete a check that currently passes, which is the one
            // thing this file must never do.
            if class.ends_with("VecDestructor") && !DESTRUCTOR_BEARING.contains(&binding.name) {
                rep.not_applicable += declared_count(derives);
                continue;
            }
            let spelled = format!("{}{}", binding.prefix, class);
            // Is this class emitted in this binding at all? A class that never
            // ships here cannot have a derive gap here; it is reported in its
            // own column so the exclusion is never silent.
            let present = class_is_named(&text, &spelled);
            if !present {
                rep.classes_absent += 1;
                continue;
            }
            rep.classes_checked += 1;

            for derive in DERIVES {
                if !derives.contains(*derive) {
                    continue;
                }
                // `Clone` on a `Copy` class has no `_clone` export by design
                // (`needs_deep_copy` is `Clone && !Copy`); the value is copied.
                if *derive == "Clone" && derives.contains("Copy") {
                    rep.not_applicable += 1;
                    continue;
                }
                let expect = binding
                    .expects
                    .iter()
                    .find(|(d, _)| d == derive)
                    .map(|(_, e)| e)
                    .unwrap_or(&Expect::Unmapped("no entry in this binding's profile"));

                // `NotApplicable` and `Unmapped` are verdicts in themselves and
                // never reach the evidence search; everything else does.
                match expect {
                    Expect::NotApplicable(_) => {
                        rep.not_applicable += 1;
                        continue;
                    }
                    Expect::Unmapped(_) => {
                        rep.unmapped += 1;
                        continue;
                    }
                    _ => {}
                }
                let hit = evidence_found(
                    expect,
                    &text,
                    &abi_index,
                    binding.block_end,
                    &spelled,
                    class,
                );
                if hit {
                    rep.honoured += 1;
                } else {
                    rep.missing += 1;
                    *rep.missing_by_derive.entry((*derive).to_string()).or_insert(0) += 1;
                    if rep.examples.len() < 40 {
                        rep.examples.push(Gap {
                            binding: binding.name.to_string(),
                            class: class.clone(),
                            derive: (*derive).to_string(),
                        });
                    }
                }
            }
        }
        reports.push(rep);
    }
    Ok(reports)
}

/// The gaps this repo currently ships, per binding, as an EXACT number.
///
/// This is a ratchet, not an exemption list. It is compared for EQUALITY, so
/// fixing a binding fails the lint until the number here comes down with it —
/// a baseline that is only ever checked as an upper bound goes stale the day
/// after it is written and then proves nothing.
///
/// A binding absent from this table must have ZERO gaps. Twenty-five are
/// today: the three Rust mirrors, memtest, all six C++ dialects, C, C#, Java,
/// Node, Kotlin, Racket, Ruby, Lua, Pascal, Ada, PHP, Perl, Common Lisp,
/// Algol 68 and COBOL.
///
/// WHAT EACH REMAINING NUMBER IS, IN ONE LINE (measured 2026-09-05):
///   * `python` (36) - the `*VecDestructor`s are now excluded by the owner's
///     2026-09-05 ruling (opaque function pointer, treated as `*const c_void`,
///     never generated), which cleared 114 of the original 150. What is left
///     is NOT type aliases and NOT plumbing: `PhysicalSizeU32`, `RefAny`,
///     `GLintVec`, `GLuintVec` and `ResultXmlXmlError` are real types with no
///     `#[pyclass]` at all. Closing them means DECIDING which of them belong
///     in the Python surface and giving them one - a product call, not a
///     routing change - so it is logged rather than guessed.
///   * `zig` (603, was 5168) - two of three causes closed. The wrapper gate
///     excluded every trait kind, so a class whose only exports were
///     `_partialEq`/`_cmp`/`_hash`/`_toDbgString` got no wrapper at all - and
///     `azul.zig` redeclares nothing (`@cImport` parses `azul.h`), so a C
///     symbol no wrapper calls cannot be spelled from Zig. Enums then needed
///     their own emitter, because an enum wrapper holds a raw tagged union
///     rather than an `inner` field. What is LEFT is the third case: a
///     FIELDLESS C enum (`AccessibilityRole`) gets no wrapper of any kind - it
///     appears only as a `C.Az*` parameter type - so its derives have nowhere
///     to hang. Closing it means emitting a wrapper for plain enums.
///   * `go` (2246, was 5648) - structs and unit-only enums are closed. The
///     struct half was the same gate-plus-emitter pair zig needed; unit enums
///     are real named Go types (`type AzAccessibilityRole uint32`), so they
///     simply take methods. Go gets idiomatic spellings - `Equal`, `Order`,
///     `PartialOrder`, `Hash`, and a `String()` implementing fmt.Stringer that
///     frees the `AzString` (GoStr only copies, so it would otherwise leak one
///     per call).
///     What is LEFT is TAGGED UNIONS, and it is a redesign rather than a
///     missing call: Go models them as a sealed INTERFACE
///     (`type AzCssProperty interface { isAzCssProperty() }`) whose variant
///     structs hold no C value, so there is no `C.Az*` whose address a trait
///     entry point could take. Wiring it means having the variant types carry
///     the union. Recorded, not guessed at.
///   * `4-20` in the remaining tail — `Xml`, `XmlNodeChild` and
///     `ResultXmlXmlError`, three classes whose entry points these emitters
///     drop for a reason not yet established. Consistent across every binding
///     that has a residue at all, so it is one cause, not fourteen.
///   * `ocaml` — every capability IS generated in `azul.ml` and the `.mli`
///     exports only `clone` and `default`, so a consumer can reach nothing else.
///
/// WHAT CAME OFF THIS TABLE, AND HOW:
///   * `rust-public` (was 7122) — `azul.rs` was `UsingDerive` with no `extern`
///     block, a combination that could not compile in either direction. It is
///     now `UsingCAPI` + `ExternalBindings`, the same shape as
///     `dll_api_external.rs`; see `CodegenConfig::rust_public_api`.
///   * `cpp11` / `cpp14` (were 5682 / 5568) — those two emitters alone applied
///     `should_skip_method`, which drops every `is_trait_function()` kind. They
///     now filter on `is_constructor_or_default` like the other four.
///   * every `cpp*` (2133 each) — enums and tagged unions get no wrapper class
///     to hang a method on, so their capabilities are now free functions
///     overloaded on the argument type; see
///     `lang_cpp::common::generate_freefn_trait_helpers`.
///   * `python` (was 4671) — the PyO3 extension had no `__eq__`, `__lt__`,
///     `__hash__`, `__copy__` or `default` anywhere: the mirror's Rust trait
///     impls are not dunders, so `a == b` silently fell back to identity.
///     `generate_derive_dunders` emits them under exactly the condition that
///     makes the corresponding mirror impl exist.
///   * the `*VecDestructor` tail, in twenty bindings at once (114 each in
///     csharp/java/node/kotlin/racket, and the bulk of the 118-134 in
///     powershell, crystal, d, julia, nim, odin, v, fortran, red, smalltalk,
///     vb6, freebasic, haskell, plus 114 previously-ABSENT classes each in
///     pascal/ada/lisp/algol68/cobol) — every one of those
///     `should_emit_function` filters excluded the whole `DestructorOrClone`
///     category, including the one entry point a `derive` actually asks for.
///     They now share `FunctionKind::is_declared_capability`.
/// The surfaces that carry `*VecDestructor` as a first-class type and really
/// do implement its derives. Everywhere else it is opaque - see the ruling at
/// the exclusion site.
const DESTRUCTOR_BEARING: &[&str] =
    &["rust-internal", "rust-dynamic", "rust-public", "memtest", "c"];

/// How many of `DERIVES` a class actually declares - the number the exclusion
/// has to add to `not_applicable` so the columns still sum to the same total.
fn declared_count(derives: &BTreeSet<String>) -> usize {
    DERIVES.iter().filter(|d| derives.contains(**d)).count()
}

pub const BASELINE: &[(&str, usize)] = &[
    ("python", 36),
    ("freebasic", 11),
    ("zig", 603),
    ("powershell", 4),
    ("php-ext", 134),
    ("ocaml", 272),
    ("haskell", 11),
    ("fortran", 20),
    ("go", 2246),
    ("smalltalk", 10),
    ("vb6", 10),
    ("crystal", 20),
    ("d", 20),
    ("julia", 20),
    ("nim", 20),
    ("odin", 20),
    ("red", 12),
    ("swift", 4),
    ("v", 20),
];

/// Compare against [`BASELINE`] and render the verdict.
pub fn verdict(reports: &[BindingReport]) -> (bool, String) {
    use std::fmt::Write as _;
    let mut out = String::new();
    let mut ok = true;

    let _ = writeln!(
        out,
        "{:<28} {:>9} {:>8} {:>7} {:>8} {:>8}  {}",
        "binding", "honoured", "missing", "n/a", "unmapped", "absent", "shape"
    );
    let _ = writeln!(out, "{}", "-".repeat(110));
    for r in reports {
        let _ = writeln!(
            out,
            "{:<28} {:>9} {:>8} {:>7} {:>8} {:>8}  {}",
            r.binding, r.honoured, r.missing, r.not_applicable, r.unmapped, r.classes_absent, r.note
        );
    }
    let _ = writeln!(out, "{}", "-".repeat(110));
    let th: usize = reports.iter().map(|r| r.honoured).sum();
    let tm: usize = reports.iter().map(|r| r.missing).sum();
    let tn: usize = reports.iter().map(|r| r.not_applicable).sum();
    let tu: usize = reports.iter().map(|r| r.unmapped).sum();
    let _ = writeln!(
        out,
        "{:<28} {:>9} {:>8} {:>7} {:>8}",
        "TOTAL", th, tm, tn, tu
    );
    let _ = writeln!(out);

    for r in reports {
        let expected = BASELINE
            .iter()
            .find(|(n, _)| *n == r.binding)
            .map(|(_, c)| *c)
            .unwrap_or(0);
        if r.missing == expected {
            continue;
        }
        ok = false;
        if r.missing > expected {
            let _ = writeln!(
                out,
                "[FAIL] {}: {} unreachable derive(s), baseline allows {}",
                r.binding, r.missing, expected
            );
            for (d, n) in &r.missing_by_derive {
                let _ = writeln!(out, "         {d}: {n}");
            }
            for g in r.examples.iter().take(3) {
                let _ = writeln!(
                    out,
                    "         e.g. `{}` declares {} but nothing in this binding names it",
                    g.class, g.derive
                );
            }
        } else {
            let _ = writeln!(
                out,
                "[FAIL] {}: improved to {} gap(s) but the baseline still says {} - lower it in \
                 doc/src/lint_derives.rs::BASELINE",
                r.binding, r.missing, expected
            );
        }
    }
    if ok {
        let _ = writeln!(
            out,
            "[ok] every declared derive is reachable in every binding, or matches its recorded baseline"
        );
    }
    (ok, out)
}

/// The per-derive breakdown behind the table, for one binding or all of them.
///
/// The table says "cpp11: 5682 missing"; this says WHICH 5682 — how they split
/// across `Debug`/`PartialEq`/`Hash`/..., and a sample of the classes. It is a
/// reporting helper only: it computes nothing the verdict does not already
/// compute and cannot change a pass into a fail or back.
pub fn details(reports: &[BindingReport], only: Option<&str>) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    for r in reports {
        if let Some(want) = only {
            if r.binding != want {
                continue;
            }
        }
        if r.missing == 0 {
            continue;
        }
        let _ = writeln!(out, "\n{} - {} unreachable", r.binding, r.missing);
        for (d, n) in &r.missing_by_derive {
            let _ = writeln!(out, "    {d:<12} {n}");
        }
        for g in &r.examples {
            let _ = writeln!(out, "    e.g. {} :: {}", g.class, g.derive);
        }
    }
    out
}
