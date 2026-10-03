//! ABI guard: ONE hash of the C ABI that api.json describes, carried by
//! libazul (`AzAbi_getHash()`) and by every binding generated from the same
//! api.json (`AZ_ABI_HASH`). A binding compares the two before its first call
//! into libazul and aborts on a mismatch, naming both hashes.
//!
//! Why: on 2026-09-30 apps linked against a libazul built from an OLDER
//! api.json misread structs - one locked up, one AzWidgets grew to 17.7 GB and
//! took the Mac down. A changed struct layout changes no symbol name, so the
//! dynamic linker cannot notice it; only a hash of the layouts can.
//!
//! What the hash covers (everything an app and libazul exchange): every
//! struct (name, repr, generic parameters, its fields IN ORDER with their
//! types and reference kinds), every enum (name, repr, union-ness, its
//! variants in order with their payloads), every type alias (target, generic
//! arguments, the monomorphized layout), every callback typedef (argument
//! types and reference kinds, return type), every C function (symbol,
//! argument types and reference kinds in order, return type) and every
//! constant (type, value).
//!
//! What it does not cover, because none of it changes a byte that crosses
//! the boundary: documentation, argument names, function bodies (`fn_body`),
//! derives, modules, external Rust paths, and the order in which api.json
//! lists its classes and functions (the entries are sorted).
//!
//! The hash is FNV-1a 64 over that canonical text: deterministic across
//! machines, Rust versions and runs (no `DefaultHasher`, no `HashMap` order).
//!
//! Emitted into (one generator per output, all from [`abi_hash`]):
//! - `dll_api_internal.rs` (libazul): `AZ_ABI_HASH`, the export
//!   `AzAbi_getHash()`, and the check helpers;
//! - `dll_api_external.rs` / `azul.rs` (the Rust `azul` crate,
//!   `link-dynamic`, and the pre-rendered release crate): `AZ_ABI_HASH`, the
//!   `AzAbi_getHash` declaration and `az_abi_check()`, which the program's
//!   loader runs before `main` (`AZ_ABI_CHECK_AT_LOAD`, an initializer-section
//!   entry), and which every wrapper a program can call first (constructor,
//!   static method, `create_default` / `impl Default`, enum-variant
//!   constructor; [`is_first_call_kind`]) and the `AzString` `From` impls call
//!   again before entering libazul (one relaxed atomic load after the first);
//! - `azul.h` (C, and C++ through it): `AZ_ABI_HASH`, the declaration,
//!   `AzAbi_check()`, and a load-time call of it (a GCC/Clang constructor, or
//!   a static object in C++ on other compilers). `AZ_NO_ABI_CHECK` opts out.
//!
//! The Python extension is not a separate binding in this sense: it is
//! compiled INTO the library it calls (`python-extension` builds libazul
//! itself), so the two cannot disagree.

use super::{
    config::{CAbiFunctionMode, CodegenConfig},
    generator::CodeBuilder,
    ir::*,
};

/// The sentence every mismatch message ends with (Rust and C alike).
pub const REBUILD_HINT: &str = "rebuild the app against this libazul";

/// The ABI hash of `ir`: FNV-1a 64 over [`abi_signature`].
pub fn abi_hash(ir: &CodegenIR) -> u64 {
    fnv1a64(abi_signature(ir).as_bytes())
}

/// The canonical text the hash is taken over: one line per type, typedef,
/// function and constant, sorted. Docs, argument names and bodies are left
/// out (see the module docs).
pub fn abi_signature(ir: &CodegenIR) -> String {
    let mut entries: Vec<String> = Vec::with_capacity(
        ir.structs.len()
            + ir.enums.len()
            + ir.type_aliases.len()
            + ir.callback_typedefs.len()
            + ir.functions.len()
            + ir.constants.len(),
    );
    for s in &ir.structs {
        entries.push(format!(
            "struct {}{} repr({}) {{ {} }}",
            s.name,
            generics(&s.generic_params),
            s.repr.as_deref().unwrap_or(""),
            fields(&s.fields)
        ));
    }
    for e in &ir.enums {
        let variants: Vec<String> = e.variants.iter().map(variant).collect();
        entries.push(format!(
            "enum {}{} repr({}) union({}) {{ {} }}",
            e.name,
            generics(&e.generic_params),
            e.repr.as_deref().unwrap_or(""),
            e.is_union,
            variants.join(", ")
        ));
    }
    for a in &ir.type_aliases {
        entries.push(format!(
            "alias {} = {}<{}> {}",
            a.name,
            a.target.trim(),
            a.generic_args.join(", "),
            a.monomorphized_def
                .as_ref()
                .map(monomorphized)
                .unwrap_or_default()
        ));
    }
    for c in &ir.callback_typedefs {
        entries.push(format!(
            "callback {}({}){}",
            c.name,
            args(&c.args),
            ret(&c.return_type)
        ));
    }
    for f in &ir.functions {
        entries.push(format!(
            "fn {}({}){}",
            f.c_name,
            args(&f.args),
            ret(&f.return_type)
        ));
    }
    for k in &ir.constants {
        entries.push(format!(
            "const {}: {} = {}",
            k.name,
            k.type_name.trim(),
            k.value.trim()
        ));
    }
    entries.sort();
    entries.join("\n")
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

fn generics(params: &[String]) -> String {
    if params.is_empty() {
        String::new()
    } else {
        format!("<{}>", params.join(", "))
    }
}

fn field(f: &FieldDef) -> String {
    format!("{}: {:?} {}", f.name, f.ref_kind, f.type_name.trim())
}

fn fields(fs: &[FieldDef]) -> String {
    fs.iter().map(field).collect::<Vec<_>>().join("; ")
}

fn variant(v: &EnumVariantDef) -> String {
    match &v.kind {
        EnumVariantKind::Unit => v.name.clone(),
        EnumVariantKind::Tuple(items) => format!(
            "{}({})",
            v.name,
            items
                .iter()
                .map(|(t, k)| format!("{k:?} {}", t.trim()))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        EnumVariantKind::Struct(fs) => format!("{} {{ {} }}", v.name, fields(fs)),
    }
}

fn monomorphized(m: &MonomorphizedTypeDef) -> String {
    match &m.kind {
        MonomorphizedKind::TaggedUnion { repr, variants } => format!(
            "union repr({}) {{ {} }}",
            repr.as_deref().unwrap_or(""),
            variants
                .iter()
                .map(|v| format!(
                    "{}({:?} {})",
                    v.name,
                    v.payload_ref_kind,
                    v.payload_type.as_deref().unwrap_or("").trim()
                ))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        MonomorphizedKind::SimpleEnum { repr, variants } => format!(
            "enum repr({}) {{ {} }}",
            repr.as_deref().unwrap_or(""),
            variants.join(", ")
        ),
        MonomorphizedKind::Struct { fields: fs } => format!("struct {{ {} }}", fields(fs)),
    }
}

fn args(a: &[FunctionArg]) -> String {
    a.iter()
        .map(|a| format!("{:?} {}", a.ref_kind, a.type_name.trim()))
        .collect::<Vec<_>>()
        .join(", ")
}

fn ret(r: &Option<String>) -> String {
    r.as_deref()
        .map(|t| format!(" -> {}", t.trim()))
        .unwrap_or_default()
}

// ============================================================================
// Rust (dll_api_internal.rs / dll_api_external.rs / azul.rs)
// ============================================================================

/// Does a Rust wrapper of `kind` call `az_abi_check()` before entering
/// libazul? Only in a binding (`ExternalBindings`) and only for the functions
/// a program can call without already holding a libazul value
/// ([`is_first_call_kind`]). The first call of any program is one of them (or
/// an `AzString` `From` impl, which checks too), so checking them checks the
/// first call. The trait bodies that call such a function (`impl Default` ->
/// `AzX_createDefault`) ask the same question with the function's kind.
pub fn rust_wrapper_checks(config: &CodegenConfig, kind: FunctionKind) -> bool {
    matches!(
        config.cabi_functions,
        CAbiFunctionMode::ExternalBindings { .. }
    ) && is_first_call_kind(kind)
}

/// Can a function of `kind` be a program's FIRST call into libazul - is it
/// callable without a value libazul made (no `self`)? Constructors, static
/// methods, `createDefault` (`X::create_default()`, `impl Default`) and the
/// enum-variant constructors (`BorderStyle::none()`, `OptionX::some(..)`).
/// Methods and the other trait functions (`clone`, `eq`, `hash`, `drop`, ...)
/// need a value first.
pub fn is_first_call_kind(kind: FunctionKind) -> bool {
    matches!(
        kind,
        FunctionKind::Constructor
            | FunctionKind::StaticMethod
            | FunctionKind::Default
            | FunctionKind::EnumVariantConstructor
    )
}

/// The statement [`rust_wrapper_checks`] wrappers start with.
pub const RUST_CHECK_CALL: &str = "az_abi_check();";

/// The ABI-guard items of a Rust output: the library side for
/// `InternalBindings`, the binding side for `ExternalBindings`, nothing for a
/// types-only output.
pub fn rust_items(ir: &CodegenIR, config: &CodegenConfig) -> String {
    let hash = abi_hash(ir);
    let mut b = CodeBuilder::new(&config.indent);
    match &config.cabi_functions {
        CAbiFunctionMode::None => return String::new(),
        CAbiFunctionMode::InternalBindings { export_feature } => {
            rust_const(&mut b, hash);
            b.line("/// The ABI hash of this libazul ([`AZ_ABI_HASH`]). A binding calls it before");
            b.line("/// its first call into the library and aborts unless it equals the");
            b.line("/// `AZ_ABI_HASH` the binding was generated with.");
            b.line(&format!(
                "#[cfg_attr(feature = \"{export_feature}\", no_mangle)]"
            ));
            b.line("pub extern \"C\" fn AzAbi_getHash() -> u64 {");
            b.indent();
            b.line("AZ_ABI_HASH");
            b.dedent();
            b.line("}");
            b.blank();
        }
        CAbiFunctionMode::ExternalBindings { .. } => {
            rust_const(&mut b, hash);
            b.line("extern \"C\" {");
            b.indent();
            b.line("/// The ABI hash of the libazul this process loaded.");
            b.line("pub fn AzAbi_getHash() -> u64;");
            b.dedent();
            b.line("}");
            b.blank();
            b.line("static AZ_ABI_CHECKED: core::sync::atomic::AtomicBool =");
            b.line("    core::sync::atomic::AtomicBool::new(false);");
            b.blank();
            b.line("/// Aborts the process unless the loaded libazul has this binding's ABI");
            b.line("/// ([`az_abi_check_hash`]). Compares once per process; every later call is");
            b.line("/// one relaxed atomic load. The program's loader runs it before `main`");
            b.line("/// (`AZ_ABI_CHECK_AT_LOAD`); every wrapper a program can call first (a");
            b.line("/// constructor, static method, default or enum-variant constructor) calls it");
            b.line("/// again before entering libazul, for a platform without a load-time entry.");
            b.line("#[inline]");
            b.line("pub fn az_abi_check() {");
            b.indent();
            b.line("if !AZ_ABI_CHECKED.load(core::sync::atomic::Ordering::Relaxed) {");
            b.indent();
            b.line("az_abi_check_slow();");
            b.dedent();
            b.line("}");
            b.dedent();
            b.line("}");
            b.blank();
            b.line("#[cold]");
            b.line("#[inline(never)]");
            b.line("fn az_abi_check_slow() {");
            b.indent();
            b.line("// Names the load-time entry below: a linker that pulls this code in");
            b.line("// (every wrapper calls it) keeps the initializer entry too.");
            b.line("let _ = unsafe { core::ptr::read_volatile(&AZ_ABI_CHECK_AT_LOAD) };");
            b.line("az_abi_check_hash(unsafe { AzAbi_getHash() });");
            b.line("AZ_ABI_CHECKED.store(true, core::sync::atomic::Ordering::Relaxed);");
            b.dedent();
            b.line("}");
            b.blank();
            rust_load_time_check(&mut b);
        }
    }
    // Shared by both sides: the library's own tests drive them, and a host
    // that loads libazul by hand (dlopen) can call them with what it read.
    b.line("/// The message a binding prints when the libazul it loaded was generated from");
    b.line("/// another api.json, naming both hashes; `None` when the two agree.");
    b.line("pub fn az_abi_mismatch_message(");
    b.line("    app_hash: u64,");
    b.line("    lib_hash: u64,");
    b.line(") -> Option<::std::string::String> {");
    b.indent();
    b.line("if app_hash == lib_hash {");
    b.line("    return None;");
    b.line("}");
    b.line(&format!(
        "Some(::std::format!(\"{}\", app_hash, lib_hash))",
        rust_message_format()
    ));
    b.dedent();
    b.line("}");
    b.blank();
    b.line("/// Prints [`az_abi_mismatch_message`] to stderr and aborts the process when");
    b.line("/// `lib_hash` (what `AzAbi_getHash()` returned) is not [`AZ_ABI_HASH`]. Calling");
    b.line("/// into a library with other struct layouts would misread memory.");
    b.line("pub fn az_abi_check_hash(lib_hash: u64) {");
    b.indent();
    b.line("if let Some(message) = az_abi_mismatch_message(AZ_ABI_HASH, lib_hash) {");
    b.indent();
    b.line("::std::eprintln!(\"{}\", message);");
    b.line("::std::process::abort();");
    b.dedent();
    b.line("}");
    b.dedent();
    b.line("}");
    b.blank();
    b.finish()
}

/// The binding's load-time check: a `#[used]` entry in the platform loader's
/// initializer section, which the loader calls before `main` (after libazul's
/// own initializers - libazul is a dependency, so it is loaded first), as
/// azul.h's constructor does. Apple: `__DATA,__mod_init_func` (typed
/// `mod_init_funcs`, as the assembler types it); ELF systems:
/// `.init_array` (the loader passes argc / argv / envp, which a C function
/// without parameters may ignore); Windows (MSVC and MinGW CRTs): `.CRT$XCU`.
/// Elsewhere it is a plain static and the per-wrapper checks remain.
fn rust_load_time_check(b: &mut CodeBuilder) {
    b.line("/// Runs [`az_abi_check`] when the program loads, before `main`, as azul.h's");
    b.line("/// constructor does: a stale app aborts before its first line runs, whatever");
    b.line("/// that line is. The loader calls every entry of its initializer section.");
    b.line("#[used]");
    b.line("#[cfg_attr(target_vendor = \"apple\", link_section = \"__DATA,__mod_init_func,mod_init_funcs\")]");
    // One attribute per line (the test reads them back line by line).
    let elf = [
        "linux",
        "android",
        "freebsd",
        "netbsd",
        "openbsd",
        "dragonfly",
        "illumos",
        "solaris",
    ]
    .map(|os| format!("target_os = \"{os}\""))
    .join(", ");
    b.line(&format!(
        "#[cfg_attr(any({elf}), link_section = \".init_array\")]"
    ));
    b.line("#[cfg_attr(target_os = \"windows\", link_section = \".CRT$XCU\")]");
    b.line("static AZ_ABI_CHECK_AT_LOAD: extern \"C\" fn() = az_abi_check_at_load;");
    b.blank();
    b.line("extern \"C\" fn az_abi_check_at_load() {");
    b.indent();
    b.line(RUST_CHECK_CALL);
    b.dedent();
    b.line("}");
    b.blank();
}

fn rust_const(b: &mut CodeBuilder, hash: u64) {
    b.line("/// The ABI hash of the api.json this code was generated from: every struct's");
    b.line("/// fields (names, types, order) and repr, every enum's variants, every");
    b.line("/// function signature. Docs do not change it. libazul exports it as");
    b.line("/// `AzAbi_getHash()`; a binding compares the two before its first call.");
    b.line(&format!(
        "pub const AZ_ABI_HASH: u64 = {};",
        hex_literal(hash)
    ));
    b.blank();
}

/// `0x0123_4567_89ab_cdef` - the spelling every output uses for the hash, so
/// the dll and the bindings can be compared textually.
pub fn hex_literal(hash: u64) -> String {
    format!("0x{hash:016x}")
}

/// The mismatch message as a Rust format string (`{:016x}` app, then lib).
fn rust_message_format() -> String {
    format!(
        "azul: ABI mismatch - this app was built against the azul ABI {{:016x}}, but the libazul \
         it loaded has the ABI {{:016x}} (its struct layouts or function signatures differ; \
         calling into it would misread memory) - {REBUILD_HINT}."
    )
}

/// The mismatch message as a C `printf` format string (app, then lib).
fn c_message_format() -> String {
    format!(
        "azul: ABI mismatch - this app was built against the azul ABI %016llx, but the libazul \
         it loaded has the ABI %016llx (its struct layouts or function signatures differ; \
         calling into it would misread memory) - {REBUILD_HINT}.\\n"
    )
}

// ============================================================================
// C (azul.h; the C++ headers include it)
// ============================================================================

/// The includes the C check needs, emitted with the header's other includes
/// (BEFORE its `extern "C" {`: libstdc++'s `<stdlib.h>` declares C++
/// overloads, which must not get C linkage).
pub fn c_includes() -> String {
    let mut b = CodeBuilder::new("    ");
    b.line("/* fprintf + abort for AzAbi_check (the ABI guard, end of this header) */");
    b.line("#ifndef AZ_NO_ABI_CHECK");
    b.line("#include <stdio.h>");
    b.line("#include <stdlib.h>");
    b.line("#endif");
    b.blank();
    b.finish()
}

/// The ABI-guard block of azul.h: the hash, the export's declaration, the
/// check, and its load-time call.
pub fn c_items(ir: &CodegenIR) -> String {
    let hash = abi_hash(ir);
    let mut b = CodeBuilder::new("    ");
    b.line("/* ABI guard. AZ_ABI_HASH is the hash of the api.json this header was");
    b.line(" * generated from (struct layouts, enum variants, function signatures; not");
    b.line(" * docs). AzAbi_getHash() returns the loaded libazul's. AzAbi_check() aborts");
    b.line(" * with both hashes when they differ, and runs once when the program loads");
    b.line(" * (define AZ_NO_ABI_CHECK before including this header to opt out). */");
    b.line(&format!(
        "#define AZ_ABI_HASH ((uint64_t){}ULL)",
        hex_literal(hash)
    ));
    b.line("extern DLLIMPORT uint64_t AzAbi_getHash(void);");
    b.line("#ifndef AZ_NO_ABI_CHECK");
    b.line("static inline void AzAbi_check(void) {");
    b.indent();
    b.line("uint64_t lib_hash = AzAbi_getHash();");
    b.line("if (lib_hash != AZ_ABI_HASH) {");
    b.indent();
    b.line(&format!(
        "fprintf(stderr, \"{}\", (unsigned long long)AZ_ABI_HASH, (unsigned long long)lib_hash);",
        c_message_format()
    ));
    b.line("abort();");
    b.dedent();
    b.line("}");
    b.dedent();
    b.line("}");
    b.line("#if defined(__GNUC__) || defined(__clang__)");
    b.line("__attribute__((constructor)) static void AzAbi_checkAtLoad(void) { AzAbi_check(); }");
    b.line("#elif defined(__cplusplus)");
    b.line("namespace {");
    b.line("struct AzAbiCheckAtLoad { AzAbiCheckAtLoad() { AzAbi_check(); } };");
    b.line("static AzAbiCheckAtLoad az_abi_check_at_load;");
    b.line("}");
    b.line("#endif");
    b.line("#endif /* AZ_NO_ABI_CHECK */");
    b.blank();
    b.finish()
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen::v2::{CodeGenerator, CodegenConfig};

    fn ir() -> &'static CodegenIR {
        super::super::bug_classes::ir()
    }

    /// `pub const AZ_ABI_HASH: u64 = 0x...;` / `#define AZ_ABI_HASH ((uint64_t)0x...ULL)`.
    fn carried_hash(text: &str) -> Option<String> {
        text.lines().find_map(|l| {
            let t = l.trim();
            let rest = t
                .strip_prefix("pub const AZ_ABI_HASH: u64 = ")
                .or_else(|| t.strip_prefix("#define AZ_ABI_HASH ((uint64_t)"))?;
            Some(
                rest.chars()
                    .take_while(|c| c.is_ascii_hexdigit() || *c == 'x')
                    .collect(),
            )
        })
    }

    #[test]
    fn the_dll_and_every_binding_carry_the_same_abi_hash() {
        let ir = ir();
        let want = hex_literal(abi_hash(ir));
        assert_ne!(abi_hash(ir), 0);
        for (what, config) in [
            ("dll_api_internal.rs", CodegenConfig::dll_internal()),
            ("dll_api_external.rs", CodegenConfig::dll_dynamic()),
            ("azul.rs", CodegenConfig::rust_public_api(ir)),
            ("azul.h", CodegenConfig::c_header()),
        ] {
            let text = CodeGenerator::generate(ir, &config).expect(what);
            assert_eq!(
                carried_hash(&text).as_deref(),
                Some(want.as_str()),
                "{what} must carry AZ_ABI_HASH = {want}"
            );
        }
        let dll = CodeGenerator::generate(ir, &CodegenConfig::dll_internal()).unwrap();
        assert!(
            dll.contains("no_mangle)]\npub extern \"C\" fn AzAbi_getHash() -> u64 {"),
            "libazul must export AzAbi_getHash"
        );
    }

    /// Every call into libazul that a program can make BEFORE it holds any
    /// value libazul made - a wrapper without `self`: a constructor, a static
    /// method, `create_default()` and `impl Default`, an enum-variant
    /// constructor (`BorderStyle::none()`, `OptionX::some(..)`). Any of them
    /// can be a program's first call, so each one checks the ABI first: a
    /// stale app must abort before it reads a struct of another layout, not
    /// after (audit 2026-10-03: `X::create_default()`, `impl Default` and the
    /// enum-variant constructors entered libazul unchecked).
    #[test]
    fn every_wrapper_a_program_can_call_first_checks_the_abi_before_entering_libazul() {
        let ir = ir();
        let label = |k: FunctionKind| match k {
            FunctionKind::Constructor => Some("constructor"),
            FunctionKind::StaticMethod => Some("static method"),
            FunctionKind::Default => Some("default"),
            FunctionKind::EnumVariantConstructor => Some("enum variant constructor"),
            _ => None,
        };
        let entry: std::collections::BTreeMap<&str, &str> = ir
            .functions
            .iter()
            .filter_map(|f| Some((f.c_name.as_str(), label(f.kind)?)))
            .collect();
        let text = CodeGenerator::generate(ir, &CodegenConfig::dll_dynamic()).unwrap();
        let lines: Vec<&str> = text.lines().map(str::trim).collect();
        let mut checked: std::collections::BTreeMap<&str, usize> = Default::default();
        let mut trait_bodies_checked = 0usize;
        let mut offenders = Vec::new();
        for (i, line) in lines.iter().enumerate() {
            let Some(p) = line.find("unsafe { Az") else {
                continue;
            };
            let callee: String = line[p + "unsafe { ".len()..]
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect();
            let Some(&kind) = entry.get(callee.as_str()) else {
                continue;
            };
            // A one-line wrapper: `pub fn x(..) -> T { az_abi_check(); unsafe { AzX_y(..) } }`;
            // a trait body (`impl Default`): `az_abi_check();` on the line before.
            let same_line = line.contains(&format!("{RUST_CHECK_CALL} unsafe {{ {callee}("));
            let line_before = i > 0 && lines[i - 1] == RUST_CHECK_CALL;
            if same_line || line_before {
                *checked.entry(kind).or_default() += 1;
                if !line.starts_with("pub fn ") {
                    trait_bodies_checked += 1;
                }
            } else {
                offenders.push(format!("{kind}: {line}"));
            }
        }
        assert!(
            offenders.is_empty(),
            "{} wrappers enter libazul without the ABI check, e.g.:\n{}",
            offenders.len(),
            offenders
                .iter()
                .take(20)
                .cloned()
                .collect::<Vec<_>>()
                .join("\n")
        );
        for kind in [
            "constructor",
            "static method",
            "default",
            "enum variant constructor",
        ] {
            let n = checked.get(kind).copied().unwrap_or(0);
            assert!(n > 100, "only {n} checked {kind} wrappers found");
        }
        assert!(
            trait_bodies_checked > 100,
            "only {trait_bodies_checked} checked `impl Default` bodies found"
        );
        // The internal bindings (link-static / libazul itself) never check.
        let dll = CodeGenerator::generate(ir, &CodegenConfig::dll_internal()).unwrap();
        assert!(!dll.contains(RUST_CHECK_CALL));
    }

    #[test]
    fn a_doc_only_change_keeps_the_abi_hash() {
        let before = abi_hash(ir());
        let mut ir = ir().clone();
        let doc = |d: &mut Vec<String>| d.push("changed".to_string());
        for s in &mut ir.structs {
            doc(&mut s.doc);
            for f in &mut s.fields {
                f.doc = Some("changed".into());
            }
        }
        for e in &mut ir.enums {
            doc(&mut e.doc);
            for v in &mut e.variants {
                v.doc = Some("changed".into());
            }
        }
        for f in &mut ir.functions {
            doc(&mut f.doc);
            // Argument names and bodies do not cross the boundary either.
            for a in &mut f.args {
                a.doc = Some("changed".into());
                a.name.push_str("_renamed");
            }
            f.fn_body = Some("changed()".into());
        }
        for a in &mut ir.type_aliases {
            doc(&mut a.doc);
        }
        for c in &mut ir.callback_typedefs {
            doc(&mut c.doc);
        }
        for k in &mut ir.constants {
            doc(&mut k.doc);
        }
        for d in ir.module_docs.values_mut() {
            d.push_str(" changed");
        }
        assert_eq!(abi_hash(&ir), before);
    }

    #[test]
    fn the_abi_hash_does_not_depend_on_the_order_api_json_lists_classes_in() {
        let before = abi_hash(ir());
        let mut ir = ir().clone();
        ir.structs.reverse();
        ir.enums.reverse();
        ir.functions.reverse();
        ir.type_aliases.reverse();
        assert_eq!(abi_hash(&ir), before);
    }

    #[test]
    fn reordering_two_fields_of_a_struct_changes_the_abi_hash() {
        let before = abi_hash(ir());
        let mut ir = ir().clone();
        let s = ir
            .structs
            .iter_mut()
            .find(|s| s.fields.len() >= 2 && s.fields[0].type_name != s.fields[1].type_name)
            .expect("a struct with two differently typed fields");
        s.fields.swap(0, 1);
        assert_ne!(abi_hash(&ir), before);
    }

    #[test]
    fn a_changed_field_type_repr_variant_order_or_signature_changes_the_abi_hash() {
        let before = abi_hash(ir());
        let changed = |f: &dyn Fn(&mut CodegenIR)| {
            let mut ir = ir().clone();
            f(&mut ir);
            abi_hash(&ir)
        };
        let field_type = changed(&|ir| {
            let s = ir
                .structs
                .iter_mut()
                .find(|s| !s.fields.is_empty())
                .unwrap();
            s.fields[0].type_name.push_str("Changed");
        });
        let repr = changed(&|ir| {
            let s = ir.structs.iter_mut().next().unwrap();
            s.repr = Some("C, packed".into());
        });
        let variants = changed(&|ir| {
            let e = ir.enums.iter_mut().find(|e| e.variants.len() >= 2).unwrap();
            e.variants.swap(0, 1);
        });
        let new_field = changed(&|ir| {
            let s = ir
                .structs
                .iter_mut()
                .find(|s| !s.fields.is_empty())
                .unwrap();
            let mut f = s.fields[0].clone();
            f.name.push_str("_new");
            s.fields.push(f);
        });
        let arg = changed(&|ir| {
            let f = ir
                .functions
                .iter_mut()
                .find(|f| !f.args.is_empty())
                .unwrap();
            f.args[0].ref_kind = match f.args[0].ref_kind {
                ArgRefKind::Owned => ArgRefKind::Ref,
                _ => ArgRefKind::Owned,
            };
        });
        let ret = changed(&|ir| {
            let f = ir.functions.iter_mut().next().unwrap();
            f.return_type = Some(format!(
                "{}Changed",
                f.return_type.clone().unwrap_or_default()
            ));
        });
        for (what, h) in [
            ("a field type", field_type),
            ("a repr", repr),
            ("the variant order", variants),
            ("a new field", new_field),
            ("an argument's ref kind", arg),
            ("a return type", ret),
        ] {
            assert_ne!(h, before, "{what} changed but the ABI hash did not");
        }
    }

    #[test]
    fn the_abi_hash_is_deterministic() {
        assert_eq!(abi_hash(ir()), abi_hash(&ir().clone()));
        // FNV-1a 64 test vectors pin the algorithm itself.
        assert_eq!(fnv1a64(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a64(b"a"), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(fnv1a64(b"foobar"), 0x8594_4171_f739_67e8);
    }

    #[test]
    fn the_mismatch_message_names_both_hashes_and_the_rebuild_hint() {
        let rust = rust_items(ir(), &CodegenConfig::dll_dynamic());
        assert!(rust.contains(REBUILD_HINT));
        assert_eq!(rust_message_format().matches("{:016x}").count(), 2);
        let c = c_items(ir());
        assert!(c.contains(REBUILD_HINT));
        assert_eq!(c_message_format().matches("%016llx").count(), 2);
        assert!(c.contains("abort();"));
        assert!(rust.contains("::std::process::abort();"));
    }

    /// The Rust binding checks the ABI when the program LOADS, before `main`,
    /// as azul.h does with its constructor: the platform loader calls a
    /// function from its initializer section (`__mod_init_func` on Apple,
    /// `.init_array` on Linux / Android / the BSDs, `.CRT$XCU` on Windows).
    /// A stale app then aborts before its first line runs, whatever that line
    /// is - even a method on a value built from a struct literal, which no
    /// wrapper check sees. The per-wrapper checks stay as the fallback on a
    /// platform without such a section.
    #[test]
    fn the_rust_binding_checks_the_abi_when_the_program_loads() {
        let ir = ir();
        for (what, config) in [
            ("dll_api_external.rs", CodegenConfig::dll_dynamic()),
            ("azul.rs", CodegenConfig::rust_public_api(ir)),
        ] {
            let text = CodeGenerator::generate(ir, &config).expect(what);
            let lines: Vec<&str> = text.lines().map(str::trim).collect();
            let at = lines
                .iter()
                .position(|l| {
                    *l == "static AZ_ABI_CHECK_AT_LOAD: extern \"C\" fn() = az_abi_check_at_load;"
                })
                .unwrap_or_else(|| panic!("{what}: no load-time ABI check static"));
            // Its attributes: the lines above it, up to its doc comment.
            let attrs: Vec<&str> = lines[..at]
                .iter()
                .rev()
                .take_while(|l| l.starts_with("#["))
                .copied()
                .collect();
            assert!(attrs.contains(&"#[used]"), "{what}: {attrs:?}");
            for section in [
                "__DATA,__mod_init_func,mod_init_funcs",
                ".init_array",
                ".CRT$XCU",
            ] {
                assert!(
                    attrs
                        .iter()
                        .any(|a| a.contains(&format!("link_section = \"{section}\""))),
                    "{what}: the check is not in the {section} initializer section: {attrs:?}"
                );
            }
            // The function the loader calls runs the one check.
            let f = lines
                .iter()
                .position(|l| *l == "extern \"C\" fn az_abi_check_at_load() {")
                .unwrap_or_else(|| panic!("{what}: no az_abi_check_at_load"));
            assert_eq!(lines[f + 1], RUST_CHECK_CALL, "{what}");
            // The check every wrapper calls names the static, so a linker that
            // pulls the binding's code in pulls the initializer entry in too.
            let slow = lines
                .iter()
                .position(|l| *l == "fn az_abi_check_slow() {")
                .unwrap_or_else(|| panic!("{what}: no az_abi_check_slow"));
            assert!(
                lines[slow..]
                    .iter()
                    .take_while(|l| **l != "}")
                    .any(|l| l.contains("AZ_ABI_CHECK_AT_LOAD")),
                "{what}: az_abi_check_slow must keep AZ_ABI_CHECK_AT_LOAD linked"
            );
        }
        // libazul itself (and link-static) has nothing to check against.
        let dll = CodeGenerator::generate(ir, &CodegenConfig::dll_internal()).unwrap();
        assert!(!dll.contains("AZ_ABI_CHECK_AT_LOAD"));
    }

    /// azul.h checks the ABI when the program loads on every C compiler, not
    /// only GCC / Clang: MSVC C (the C compiler of Windows) has no
    /// constructor attribute, so the check is an entry in the CRT's
    /// initializer table `.CRT$XCU`, one per program (`selectany`: every
    /// translation unit that includes azul.h defines it), and kept by the
    /// linker (`/include:`, with the leading underscore 32-bit x86 C names
    /// carry). C++ keeps its static object, which MSVC C++ runs.
    #[test]
    fn azul_h_checks_the_abi_at_load_on_msvc_c_too() {
        let c = c_items(ir());
        let lines: Vec<&str> = c.lines().map(str::trim).collect();
        let at = |l: &str| lines.iter().position(|x| *x == l);
        let gnu = at("#if defined(__GNUC__) || defined(__clang__)").expect("the GCC / Clang arm");
        let cpp = at("#elif defined(__cplusplus)").expect("the C++ arm");
        let msvc = at("#elif defined(_MSC_VER)").expect("an MSVC C arm");
        assert!(
            gnu < cpp && cpp < msvc,
            "GCC / Clang, then C++, then MSVC C"
        );
        let arm = &lines[msvc..];
        let has = |s: &str| arm.iter().any(|l| l.contains(s));
        assert!(has("#pragma section(\".CRT$XCU\", read)"), "{arm:?}");
        assert!(
            has("__declspec(selectany) __declspec(allocate(\".CRT$XCU\"))"),
            "{arm:?}"
        );
        assert!(
            has("AzAbi_checkAtLoadEntry)(void) = AzAbi_checkAtLoad;"),
            "{arm:?}"
        );
        assert!(has(
            "static void __cdecl AzAbi_checkAtLoad(void) { AzAbi_check(); }"
        ));
        assert!(has(
            "#pragma comment(linker, \"/include:_AzAbi_checkAtLoadEntry\")"
        ));
        assert!(has(
            "#pragma comment(linker, \"/include:AzAbi_checkAtLoadEntry\")"
        ));
        assert!(has("#if defined(_M_IX86)"), "32-bit x86 decorates C names");
    }
}
