//! Fortran (modern, F2003+) binding generator.
//!
//! Generates a set of Fortran modules behind one facade, `azul`:
//!
//! 1. `azul_types_<unit>.f90` — one module per plan chunk (see
//!    [`super::module_plan::ModulePlan`]; `azul_types_css`,
//!    `azul_types_dom`, `azul_types_css_2`, ...) declaring the C-ABI types
//!    of that chunk as Fortran `type, bind(C)` derived types (the Fortran
//!    spelling of a C struct), unit-only enums as `enum, bind(C)` blocks
//!    (F2008), tagged unions as ABI-opaque blob types with the exact C
//!    size/alignment (Fortran has no native `union`; the blob keeps every
//!    embedding struct layout-identical to `azul.h` — see [`layout`]), and
//!    callback typedefs as `abstract interface`s. A chunk `use`s the chunks
//!    it references; `azul_types` re-exports them all.
//! 2. `azul_ffi_<module>.f90` — one module per api.json module declaring
//!    that module's C-API functions inside an `interface ... end interface`
//!    block, each with the verbatim C symbol carried via
//!    `bind(C, name="AzFoo_create")`. Fortran is case-insensitive for
//!    its own identifiers, but the `name="..."` argument is case-sensitive
//!    so the linker matches the same exported symbols as the C/C++/Pascal
//!    bindings. `azul_ffi` re-exports them all.
//! 3. `azul_api.f90` — the idiomatic layer: every class as a `<snake>_t`
//!    derived type (`dom_t`, `button_t`, `app_t`) whose methods are
//!    type-bound procedures (`call app%run(window)`), whose `String`
//!    arguments are `character(len=*)`, whose unit enums are plain
//!    `integer`, and whose callbacks are ordinary Fortran procedures
//!    matching a typed abstract interface; plus the host-invoker runtime
//!    (see [`managed`]): one handle table that owns both `RefAny` payloads
//!    and registered user procedures, installed lazily on the first handle
//!    so user code never calls an `init` function. There is deliberately
//!    NO `final ::` subroutine: gfortran finalizes a function result after
//!    the assignment that consumed it, so a finalizer would `_delete`
//!    everything a factory ever returned. Cleanup is the explicit `delete`
//!    type-bound procedure, guarded by an `owned` flag. This layer is one
//!    module because type-bound procedures, the callback interfaces that
//!    name the wrapper types and the wrappers that take callbacks form one
//!    cycle Fortran modules cannot express; a parent/submodule split of its
//!    bodies is the next step if it ever needs one.
//! 4. `azul_<module>.f90` — one facade per api.json module that
//!    re-exports exactly that module's names (`use azul_dom, only: dom_t,
//!    dom_create_body`), whichever unit above declares them.
//! 5. `azul.f90` — the facade that `use`s everything, so `use azul` is the
//!    only import a program needs (including the `iso_c_binding` entities
//!    the raw layer traffics in).
//! 6. `Makefile` + `sources.txt` — the compile order (a module must be
//!    compiled before the modules that `use` it).
//!
//! # Build
//!
//! ```bash
//! make            # or, by hand, in the order of sources.txt:
//! gfortran -ffree-line-length-none -c azul_types_css.f90 ... azul.f90
//! gfortran -ffree-line-length-none main.f90 azul*.o -L. -lazul -o main
//! ```
//!
//! No standardized package manifest exists in the Fortran ecosystem, so
//! the generator emits a plain Makefile rather than something like an
//! `fpm.toml`.
//!
//! # Output protocol
//!
//! `generate(ir, config)` returns a single `String` with multiple files
//! separated by [`FILE_MARKER`] / [`END_MARKER`] header lines (each a
//! valid Fortran comment). The orchestrator splits them into one directory.

use std::collections::BTreeMap;

use anyhow::Result;

use super::config::CodegenConfig;
use super::generator::CodeBuilder;
use super::ir::CodegenIR;
use super::module_plan::ModulePlan;

pub mod functions;
pub(crate) mod layout;
pub mod makefile;
pub mod managed;
pub mod types;
pub mod wrappers;

/// Library name used in `-lazul` link flags.
///
/// Matches the prebuilt artifact's name without extension; gfortran
/// resolves `-lazul` to `azul.dll` on Windows, `libazul.so` on Linux,
/// and `libazul.dylib` on macOS.
pub const LIB_NAME: &str = "azul";

/// Maximum identifier length permitted by F2003. F2008 raised this to
/// 63 chars; we use 63 throughout because all current toolchains accept
/// it. Names longer than this are truncated by [`truncate_identifier`].
pub const MAX_IDENT_LEN: usize = 63;

/// File-marker header introducing each per-file section of the output.
pub const FILE_MARKER: &str = "!==FILE: ";

/// Trailing marker closing the file-marker header line.
pub const END_MARKER: &str = " ==";

/// Comment the emitters write in front of every declaration group, naming
/// the api.json module it belongs to. The per-module facades are built by
/// reading these back together with the `public ::` lines that follow.
pub const API_MODULE_MARKER: &str = "! [api.json module: ";

/// The api.json module a class the plan does not know is filed under.
const FALLBACK_MODULE: &str = "misc";

/// Names per `use ..., only:` statement in a facade (F2008 allows 255
/// continuation lines; gfortran is generous, but keep every statement
/// short).
const ONLY_NAMES_PER_STATEMENT: usize = 40;

/// The split, shared by the per-unit emitters.
pub struct Split {
    pub plan: ModulePlan,
}

impl Split {
    pub fn new(ir: &CodegenIR) -> Self {
        Split {
            plan: ModulePlan::build(ir),
        }
    }

    /// The api.json module a class / type / callback typedef belongs to.
    pub fn module_of(&self, type_or_class: &str) -> String {
        self.plan
            .api_module_of(type_or_class)
            .unwrap_or(FALLBACK_MODULE)
            .to_string()
    }

    /// Every api.json module the per-class surface is split over.
    pub fn api_modules(&self, ir: &CodegenIR) -> Vec<String> {
        let mut mods: std::collections::BTreeSet<String> =
            self.plan.api_modules().into_iter().collect();
        for f in &ir.functions {
            mods.insert(self.module_of(&f.class_name));
        }
        mods.into_iter().collect()
    }

    /// Module name of a types chunk: `azul_types_css_2`.
    pub fn types_unit(&self, chunk_idx: usize) -> String {
        format!("azul_types_{}", self.plan.chunks[chunk_idx].name)
    }

    /// The marker line for a declaration group of `type_or_class`.
    pub fn marker(&self, type_or_class: &str) -> String {
        format!("{}{}]", API_MODULE_MARKER, self.module_of(type_or_class))
    }
}

/// Public entry point. Produces every Fortran source of the binding plus
/// the Makefile, concatenated with file markers.
pub fn generate(ir: &CodegenIR, config: &CodegenConfig) -> Result<String> {
    let split = Split::new(ir);
    let mut files: Vec<(String, String)> = Vec::new();
    // api.json module -> unit -> public names declared for it there.
    let mut registry: BTreeMap<String, BTreeMap<String, Vec<String>>> = BTreeMap::new();
    let mut order: Vec<String> = Vec::new();

    // 1. Types, one module per plan chunk.
    let mut type_units = Vec::new();
    for idx in 0..split.plan.chunks.len() {
        let unit = split.types_unit(idx);
        let src = generate_types_unit(ir, config, &split, idx)?;
        collect_publics(&src, &unit, FALLBACK_MODULE, &mut registry);
        files.push((format!("{}.f90", unit), src));
        order.push(unit.clone());
        type_units.push(unit);
    }
    files.push((
        "azul_types.f90".to_string(),
        generate_reexport_module(
            "azul_types",
            "Every C-ABI type of the binding: the union of the per-module chunks.",
            &type_units,
        ),
    ));
    order.push("azul_types".to_string());

    // 2. C-ABI interface blocks, one module per api.json module.
    let api_modules = split.api_modules(ir);
    let mut ffi_units = Vec::new();
    for m in &api_modules {
        let unit = format!("azul_ffi_{}", m);
        let src = generate_ffi_unit(ir, config, &split, m)?;
        collect_publics(&src, &unit, m, &mut registry);
        files.push((format!("{}.f90", unit), src));
        order.push(unit.clone());
        ffi_units.push(unit);
    }
    files.push((
        "azul_ffi.f90".to_string(),
        generate_reexport_module(
            "azul_ffi",
            "Every C-API function of the binding: the union of the per-module interface blocks.",
            &ffi_units,
        ),
    ));
    order.push("azul_ffi".to_string());

    // 3. The idiomatic layer.
    let api_src = generate_api_unit(ir, config, &split)?;
    collect_publics(&api_src, "azul_api", FALLBACK_MODULE, &mut registry);
    files.push(("azul_api.f90".to_string(), api_src));
    order.push("azul_api".to_string());

    // 4. Per-module facades.
    for m in &api_modules {
        let unit = format!("azul_{}", m);
        let names = registry.get(m).cloned().unwrap_or_default();
        files.push((format!("{}.f90", unit), generate_module_facade(m, &names)));
        order.push(unit);
    }

    // 5. The umbrella.
    files.push(("azul.f90".to_string(), generate_umbrella()));
    order.push("azul".to_string());

    // 6. Build files.
    let chunk_deps: Vec<(String, Vec<String>)> = split
        .plan
        .chunks
        .iter()
        .enumerate()
        .map(|(i, c)| {
            (
                split.types_unit(i),
                c.deps.iter().map(|&d| split.types_unit(d)).collect(),
            )
        })
        .collect();
    files.push((
        "Makefile".to_string(),
        makefile::generate_makefile(&chunk_deps, &ffi_units, &api_modules),
    ));
    let mut sources = String::new();
    for u in &order {
        sources.push_str(u);
        sources.push_str(".f90\n");
    }
    files.push(("sources.txt".to_string(), sources));

    let total: usize = files.iter().map(|(_, s)| s.len()).sum();
    let mut out = String::with_capacity(total + 64 * files.len());
    for (path, src) in &files {
        out.push_str(FILE_MARKER);
        out.push_str(path);
        out.push_str(END_MARKER);
        out.push('\n');
        out.push_str(src);
        if !src.ends_with('\n') {
            out.push('\n');
        }
    }
    Ok(out)
}

/// Read the `public :: <name>` lines of a generated unit back, attributed
/// to the api.json module the nearest preceding [`API_MODULE_MARKER`]
/// names (`default` before the first marker).
fn collect_publics(
    src: &str,
    unit: &str,
    default: &str,
    registry: &mut BTreeMap<String, BTreeMap<String, Vec<String>>>,
) {
    let mut module = default.to_string();
    for line in src.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix(API_MODULE_MARKER) {
            module = rest.trim_end_matches(']').trim().to_string();
        } else if let Some(name) = t.strip_prefix("public :: ") {
            let name = name.trim();
            let names = registry
                .entry(module.clone())
                .or_default()
                .entry(unit.to_string())
                .or_default();
            if !names.iter().any(|n| n == name) {
                names.push(name.to_string());
            }
        }
    }
}

// ============================================================================
// Per-unit builders
// ============================================================================

fn emit_header(builder: &mut CodeBuilder, what: &str) {
    builder.line("! ============================================================================");
    builder.line(&format!("! {}", what));
    builder.line("! Auto-generated Fortran (F2003+) bindings for the Azul GUI framework.");
    builder.line("! Generated by azul-doc codegen v2 (lang_fortran). DO NOT EDIT MANUALLY.");
    builder.line("!");
    builder.line("! Requires Fortran 2003 or newer (uses iso_c_binding); F2008 `enum, bind(C)`");
    builder.line("! blocks are used for unit enums (gfortran >= 4.6, ifort/ifx >= 14).");
    builder.line("!");
    builder.line("! Build with the generated Makefile, or by hand in the order of sources.txt:");
    builder.line("!   gfortran -ffree-line-length-none -c <each azul*.f90 in that order>");
    builder.line("!   gfortran -ffree-line-length-none main.f90 azul*.o -L. -lazul -o main");
    builder.line("!");
    builder.line("! `-ffree-line-length-none` is not optional. Free-form Fortran capped");
    builder.line("! lines at 132 columns until F2023 lifted it, and the C ABI names here");
    builder.line("! are long enough that ~1500 declarations run past that. gfortran >= 14");
    builder.line("! defaults to no limit and accepts the files bare; every older gfortran");
    builder.line("! truncates the line and then errors on the half-statement that is left.");
    builder.line("! The generated Makefile already carries the flag in FFLAGS.");
    builder.line("! ============================================================================");
    builder.blank();
}

/// A module that re-exports other modules wholesale.
fn generate_reexport_module(name: &str, what: &str, units: &[String]) -> String {
    let mut b = CodeBuilder::new("  ");
    emit_header(&mut b, what);
    b.line(&format!("module {}", name));
    b.indent();
    for u in units {
        b.line(&format!("use {}", u));
    }
    b.line("implicit none");
    b.line("public");
    b.dedent();
    b.line(&format!("end module {}", name));
    b.finish()
}

/// `azul_types_<unit>`: one plan chunk's types.
fn generate_types_unit(
    ir: &CodegenIR,
    config: &CodegenConfig,
    split: &Split,
    idx: usize,
) -> Result<String> {
    let chunk = &split.plan.chunks[idx];
    let mut b = CodeBuilder::new("  ");
    emit_header(
        &mut b,
        &format!(
            "C-ABI types of the api.json module `{}` (unit {} of the split).",
            chunk.api_module, chunk.ordinal
        ),
    );
    let unit = split.types_unit(idx);
    b.line(&format!("module {}", unit));
    b.indent();
    b.line("use, intrinsic :: iso_c_binding");
    for dep in &chunk.deps {
        b.line(&format!("use {}", split.types_unit(*dep)));
    }
    b.line("implicit none");
    b.line("private");
    b.blank();
    let members: std::collections::BTreeSet<&str> =
        chunk.types.iter().map(|s| s.as_str()).collect();
    let belongs = |t: &str| members.contains(t);
    types::generate_types_for(&mut b, ir, config, &belongs, split)?;
    b.dedent();
    b.line(&format!("end module {}", unit));
    Ok(b.finish())
}

/// `azul_ffi_<module>`: the C-API interface block of one api.json module.
fn generate_ffi_unit(
    ir: &CodegenIR,
    config: &CodegenConfig,
    split: &Split,
    api_module: &str,
) -> Result<String> {
    let mut b = CodeBuilder::new("  ");
    emit_header(
        &mut b,
        &format!(
            "C-API interface block of the api.json module `{}`.",
            api_module
        ),
    );
    let unit = format!("azul_ffi_{}", api_module);
    b.line(&format!("module {}", unit));
    b.indent();
    b.line("use, intrinsic :: iso_c_binding");
    b.line("use azul_types");
    b.line("implicit none");
    b.line("private");
    b.blank();
    let belongs = |c: &str| split.module_of(c) == api_module;
    functions::generate_externals_for(&mut b, ir, config, &belongs)?;
    b.dedent();
    b.line(&format!("end module {}", unit));
    Ok(b.finish())
}

/// `azul_api`: the idiomatic wrappers and the managed runtime.
fn generate_api_unit(ir: &CodegenIR, config: &CodegenConfig, split: &Split) -> Result<String> {
    let mut b = CodeBuilder::new("  ");
    emit_header(
        &mut b,
        "Idiomatic wrappers (`<snake>_t` types with type-bound methods) and the host-invoker runtime.",
    );
    b.line("module azul_api");
    b.indent();
    b.line("use, intrinsic :: iso_c_binding");
    b.line("use azul_types");
    b.line("use azul_ffi");
    b.line("implicit none");
    b.line("private");
    b.blank();

    // Wrapper type declarations (inside the module decl section — Fortran
    // modules separate type declarations from procedure bodies via the
    // `contains` keyword below). The plan is built once and shared with
    // the managed layer: both claim module-wide (case-folded) identifiers
    // from the same table.
    let ctx = wrappers::Ctx::new(ir, config);
    wrappers::generate_wrapper_decls(&mut b, &ctx, split)?;

    // Managed-FFI host-invoker plumbing — typed callback interfaces, the
    // handle table and the host-handle FFI block. Must come before
    // `contains`, and after the wrapper types the abstract interfaces
    // `import`.
    managed::emit_managed_decls(&mut b, &ctx, split);

    b.dedent();
    b.line("contains");
    b.indent();
    b.blank();

    wrappers::generate_wrapper_bodies(&mut b, &ctx)?;
    managed::emit_managed_bodies(&mut b, &ctx);

    b.dedent();
    b.line("end module azul_api");
    Ok(b.finish())
}

/// `azul_<module>`: re-exports exactly one api.json module's names.
fn generate_module_facade(api_module: &str, names: &BTreeMap<String, Vec<String>>) -> String {
    let mut b = CodeBuilder::new("  ");
    emit_header(
        &mut b,
        &format!(
            "Facade for the api.json module `{}`: `use azul_{}, only: ...` imports just this module's names.",
            api_module, api_module
        ),
    );
    let unit = format!("azul_{}", api_module);
    b.line(&format!("module {}", unit));
    b.indent();
    for (from, list) in names {
        // A whole per-module interface block is this module's by
        // construction; everything else is picked by name.
        if from == &format!("azul_ffi_{}", api_module) {
            b.line(&format!("use {}", from));
            continue;
        }
        for group in list.chunks(ONLY_NAMES_PER_STATEMENT) {
            b.line(&format!("use {}, only: {}", from, group.join(", ")));
        }
    }
    b.line("implicit none");
    b.line("public");
    b.dedent();
    b.line(&format!("end module {}", unit));
    b.finish()
}

/// `azul`: everything.
fn generate_umbrella() -> String {
    let mut b = CodeBuilder::new("  ");
    emit_header(
        &mut b,
        "The `azul` module: every unit of the binding. `use azul` (or `use azul, only: ...`) is the only import a program needs.",
    );
    b.line("module azul");
    b.indent();
    b.line("! iso_c_binding re-exports: `use azul` is the only import a program");
    b.line("! needs, even when it reaches for the raw `az_*` layer.");
    b.line(&format!(
        "use, intrinsic :: iso_c_binding, only: {}",
        ISO_C_REEXPORTS.join(", ")
    ));
    b.line("use azul_types");
    b.line("use azul_ffi");
    b.line("use azul_api");
    b.line("implicit none");
    b.line("public");
    b.dedent();
    b.line("end module azul");
    b.finish()
}

// ============================================================================
// Shared name helpers (used by submodules)
// ============================================================================

/// The `Az`-prefixed FFI derived-type name for an IR type
/// (e.g. `Dom` -> `AzDom`). Matches the Rust C-ABI struct name and the
/// linker symbol prefix.
pub fn ffi_type_name(name: &str) -> String {
    format!("Az{}", name)
}

/// Idiomatic wrapper type name (e.g. `Dom` -> `dom_t`,
/// `WindowCreateOptions` -> `window_create_options_t`).
///
/// The `_t` suffix is not decoration: Fortran folds case, so a wrapper
/// type spelled `Dom` makes `type(Dom) :: dom` — the natural variable
/// name — a redeclaration of the type itself. Every other Fortran
/// binding in the wild solves this the same way.
pub fn wrapper_type_name(name: &str) -> String {
    truncate_identifier(&format!("{}_t", pascal_to_snake_case(name)))
}

/// `iso_c_binding` entities re-exported from the `azul` module so user
/// code needs a single `use azul`. A `private` module may re-export
/// entities it obtained by use-association, including the intrinsic
/// procedures.
pub const ISO_C_REEXPORTS: &[&str] = &[
    "c_int",
    "c_int8_t",
    "c_int16_t",
    "c_int32_t",
    "c_int64_t",
    "c_size_t",
    "c_intptr_t",
    "c_float",
    "c_double",
    "c_bool",
    "c_char",
    "c_ptr",
    "c_funptr",
    "c_null_ptr",
    "c_null_funptr",
    "c_loc",
    "c_f_pointer",
    "c_f_procpointer",
    "c_associated",
    "c_funloc",
];

/// The "instance method" prefix for wrapper procedures. We use a
/// snake_case lowering of the wrapper type as the prefix so type-bound
/// procedure resolution works without conflicts: `App_run` -> `app_run`.
pub fn instance_method_prefix(wrapper_type: &str) -> String {
    pascal_to_snake_case(wrapper_type)
}

/// Map a Rust/IR type name to its Fortran `iso_c_binding` equivalent.
///
/// References / pointers always resolve to `type(c_ptr)` because Fortran
/// has no typed-pointer aliases the way Pascal/C do; users rely on the
/// derived-type definition for layout and on `c_loc`/`c_f_pointer` for
/// raw pointer interop.
///
/// IR derived types (structs, enums, callback typedefs) resolve to
/// `type(AzFoo)` so they may be used inline as fields or arguments.
pub fn map_type_to_fortran(rust_type: &str, ir: &CodegenIR) -> String {
    let trimmed = rust_type.trim();

    // Pointers and references — always `type(c_ptr)` (Fortran has no
    // typed-pointer alias). The wrapper layer reinterprets via
    // `c_f_pointer` when needed.
    if trimmed.starts_with("*const ")
        || trimmed.starts_with("*mut ")
        || trimmed.starts_with("&mut ")
        || trimmed.starts_with('&')
    {
        return "type(c_ptr)".to_string();
    }

    // Arrays: `[T; N]` -> `<elem> dimension(N)` would be the natural
    // mapping, but `bind(C)` only allows fixed-size arrays of basic
    // intrinsic types here, and we don't currently see user-facing arrays
    // in api.json. Fall back to opaque `type(c_ptr)` for safety.
    if trimmed.starts_with('[') && trimmed.ends_with(']') {
        return "type(c_ptr)".to_string();
    }

    match trimmed {
        // Void / unit: there is no `void` in Fortran; for arguments this
        // path is unreachable (we emit `subroutine` instead of `function`).
        // For pointer-to-void use `type(c_ptr)`.
        "void" | "c_void" | "()" => "type(c_ptr)".to_string(),

        // Booleans
        "bool" | "GLboolean" => "logical(c_bool)".to_string(),

        // Signed / unsigned integers via iso_c_binding kind selectors.
        // Fortran has no unsigned-integer kind, so `u8`/`u16`/... map to
        // the same kind as their signed counterparts; this is the same
        // approach the Pascal/Ada bindings take.
        "i8" | "u8" | "c_char" | "char" | "c_uchar" => "integer(c_int8_t)".to_string(),
        "i16" | "u16" => "integer(c_int16_t)".to_string(),
        "i32" | "u32" | "c_int" | "c_uint" | "GLint" | "GLuint" | "GLenum" | "GLbitfield"
        | "GLsizei" => "integer(c_int32_t)".to_string(),
        "i64" | "u64" | "GLint64" | "GLuint64" => "integer(c_int64_t)".to_string(),
        "f32" | "GLfloat" | "GLclampf" => "real(c_float)".to_string(),
        "f64" | "GLdouble" | "GLclampd" => "real(c_double)".to_string(),
        "usize" | "size_t" | "uintptr_t" => "integer(c_size_t)".to_string(),
        "isize" | "ssize_t" | "intptr_t" | "GLsizeiptr" | "GLintptr" => {
            "integer(c_intptr_t)".to_string()
        }

        // Anything else: assume it's a known IR type and emit a
        // `type(AzFoo)`. Unit enums (no payload variants) are emitted
        // as `enumerator :: AzFoo_X = N` constants without a backing
        // derived type, so they must map to `integer(c_int)` here.
        // Tagged-union enums DO get a `type, bind(C) :: AzFoo` block.
        // Unknown types fall back to `type(c_ptr)` so the generated
        // module still compiles.
        _ => {
            if let Some(e) = ir.find_enum(trimmed) {
                // Generic templates have no concrete layout — opaque ptr.
                if !e.generic_params.is_empty() {
                    "type(c_ptr)".to_string()
                } else if e.is_union {
                    // ALL non-generic tagged unions get a `type, bind(C)`
                    // block: included ones as an ABI-opaque blob (see
                    // types.rs::emit_tagged_union), skipped-category ones
                    // (DestructorOrClone etc.) as a blob stand-in emitted
                    // by generate_types. Mapping them to `type(c_ptr)`
                    // (8 bytes) here is what shrank every embedding
                    // struct and corrupted all by-value calls.
                    if layout::type_layout(trimmed, ir).is_some() {
                        format!("type({})", ffi_type_name(trimmed))
                    } else {
                        "type(c_ptr)".to_string()
                    }
                } else {
                    // Unit enums are C `enum`s (int-sized) — even the
                    // skipped-category ones, whose enumerator constants
                    // are simply not emitted.
                    "integer(c_int)".to_string()
                }
            } else if ir.callback_typedefs.iter().any(|c| c.name == trimmed) {
                // Callback typedefs are emitted as `abstract interface`
                // signatures + a `procedure pointer` value; the type itself
                // is `c_funptr` from `iso_c_binding`. Using `type(AzFoo)`
                // here would dangle — there's no derived type by that
                // name.
                "type(c_funptr)".to_string()
            } else if let Some(ta) = ir.find_type_alias(trimmed) {
                // Simple type alias: resolve to the target. The codegen
                // only emits a `type, bind(C) :: AzFoo` block for
                // monomorphized aliases; simple aliases (ScanCode = u32,
                // GLuint = u32) have no derived type and must lower to
                // the target's representation. Recurse so chains resolve.
                if ta.monomorphized_def.is_some() {
                    format!("type({})", ffi_type_name(trimmed))
                } else {
                    map_type_to_fortran(&ta.target, ir)
                }
            } else if let Some(s) = ir.find_struct(trimmed) {
                // Skipped struct categories (Recursive, VecRef,
                // DestructorOrClone) get an ABI-opaque blob stand-in
                // emitted by generate_types whenever their layout is
                // computable, so `type(AzFoo)` is safe AND layout-exact
                // (a bare `type(c_ptr)` is 8 bytes and would corrupt any
                // struct embedding them by value, e.g. AzXmlNodeChild).
                // Generic templates / layout-incomputable ones stay
                // opaque pointers.
                if !s.generic_params.is_empty() {
                    "type(c_ptr)".to_string()
                } else if matches!(
                    s.category,
                    crate::codegen::v2::ir::TypeCategory::Recursive
                        | crate::codegen::v2::ir::TypeCategory::VecRef
                        | crate::codegen::v2::ir::TypeCategory::DestructorOrClone
                        | crate::codegen::v2::ir::TypeCategory::GenericTemplate
                ) {
                    if layout::type_layout(trimmed, ir).is_some() {
                        format!("type({})", ffi_type_name(trimmed))
                    } else {
                        "type(c_ptr)".to_string()
                    }
                } else {
                    format!("type({})", ffi_type_name(trimmed))
                }
            } else {
                "type(c_ptr)".to_string()
            }
        }
    }
}

/// Sanitize a name for use as a Fortran identifier.
///
/// - Reserved keywords get a trailing underscore.
/// - Names longer than [`MAX_IDENT_LEN`] are truncated.
/// - Leading underscores (illegal in Fortran identifiers) get prefixed
///   with `f_`.
pub fn sanitize_identifier(name: &str) -> String {
    let mut out = if is_fortran_reserved(name) {
        format!("{}_", name)
    } else if name.starts_with('_') {
        format!("f{}", name)
    } else {
        name.to_string()
    };
    if out.len() > MAX_IDENT_LEN {
        out = truncate_identifier(&out);
    }
    out
}

/// Fortran reserved words that are likely to collide with field /
/// argument names in api.json. The full list is much larger (>100); we
/// include only those plausibly emitted from user-facing field names.
pub fn is_fortran_reserved(name: &str) -> bool {
    matches!(
        name.to_lowercase().as_str(),
        "if" | "then"
            | "else"
            | "elseif"
            | "endif"
            | "do"
            | "enddo"
            | "end"
            | "function"
            | "subroutine"
            | "module"
            | "program"
            | "type"
            | "interface"
            | "use"
            | "implicit"
            | "real"
            | "integer"
            | "logical"
            | "character"
            | "complex"
            | "double"
            | "precision"
            | "kind"
            | "len"
            | "where"
            | "elsewhere"
            | "endwhere"
            | "select"
            | "case"
            | "default"
            | "endselect"
            | "go"
            | "goto"
            | "continue"
            | "stop"
            | "return"
            | "call"
            | "pure"
            | "elemental"
            | "recursive"
            | "result"
            | "contains"
            | "private"
            | "public"
            | "protected"
            | "save"
            | "data"
            | "block"
            | "common"
            | "equivalence"
            | "namelist"
            | "external"
            | "intrinsic"
            | "optional"
            | "parameter"
            | "pointer"
            | "target"
            | "allocatable"
            | "dimension"
            | "intent"
            | "in"
            | "out"
            | "inout"
            | "value"
            | "volatile"
            | "asynchronous"
            | "bind"
            | "import"
            | "abstract"
            | "class"
            | "deferred"
            | "extends"
            | "final"
            | "generic"
            | "non_overridable"
            | "nopass"
            | "pass"
            | "procedure"
            | "sequence"
            | "abs"
            | "min"
            | "max"
            | "size"
            | "data_"
            // Single-letter names that collide with the synthetic
            // `r` result variable on every generated function.
            // Without these the Fortran compiler raises
            // "DUMMY attribute conflicts with RESULT attribute".
            | "r"
    )
}

/// Truncate an identifier to [`MAX_IDENT_LEN`] characters by hashing the
/// tail and appending a short suffix; this keeps the prefix readable
/// while guaranteeing uniqueness.
pub fn truncate_identifier(name: &str) -> String {
    if name.len() <= MAX_IDENT_LEN {
        return name.to_string();
    }
    // Reserve 9 chars for "_" + 8-char hex hash. Take the first
    // (MAX_IDENT_LEN - 9) chars verbatim, then append `_<hash>`.
    let head_len = MAX_IDENT_LEN.saturating_sub(9);
    let head: String = name.chars().take(head_len).collect();

    let mut hash: u32 = 0x811c9dc5;
    for b in name.as_bytes() {
        hash ^= *b as u32;
        hash = hash.wrapping_mul(0x01000193);
    }
    format!("{}_{:08x}", head, hash)
}

/// Convert PascalCase / CamelCase to snake_case (lowercase). Used to
/// derive instance-method prefixes from wrapper type names
/// (`AppConfig` -> `app_config`).
pub fn pascal_to_snake_case(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    for (i, c) in s.chars().enumerate() {
        if c.is_ascii_uppercase() {
            if i > 0 {
                out.push('_');
            }
            out.extend(c.to_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// Sanitize a doc comment for inclusion as a Fortran `! ...` line.
/// Newlines split the comment over multiple `!` lines; the caller is
/// responsible for emitting one `!` prefix per resulting line.
pub fn sanitize_comment_line(s: &str) -> String {
    s.replace('\r', " ").replace('\n', " ").trim().to_string()
}

#[cfg(test)]
mod split_tests {
    use super::super::config::CodegenConfig;
    use super::super::module_plan::test_fixture_ir;
    use super::*;
    use std::collections::BTreeMap;

    fn generated() -> BTreeMap<String, String> {
        let ir = test_fixture_ir();
        let out = generate(&ir, &CodegenConfig::c_header()).expect("fortran codegen");
        let mut files = BTreeMap::new();
        let mut cur: Option<String> = None;
        for line in out.lines() {
            if let Some(rest) = line.strip_prefix(FILE_MARKER) {
                cur = Some(rest.trim_end_matches(END_MARKER).trim().to_string());
                files.insert(cur.clone().unwrap(), String::new());
            } else if let Some(p) = &cur {
                let f = files.get_mut(p).unwrap();
                f.push_str(line);
                f.push('\n');
            }
        }
        files
    }

    #[test]
    fn types_split_per_module_with_uses() {
        let files = generated();
        let css = &files["azul_types_css.f90"];
        let dom = &files["azul_types_dom.f90"];
        assert!(css.contains("type, bind(C) :: AzColor0"), "{}", css);
        assert!(dom.contains("type, bind(C) :: AzDom\n"), "{}", dom);
        assert!(dom.contains("type, bind(C) :: AzDomVec"), "DomVec follows Dom");
        assert!(dom.contains("use azul_types_css"), "dom embeds Color0:\n{}", dom);
        assert!(!css.contains("use azul_types_dom"));
        assert!(dom.contains("enumerator :: AzUpdate_RefreshDom = 1"), "{}", dom);
    }

    #[test]
    fn facades_reexport_by_module_and_sources_are_ordered() {
        let files = generated();
        let dom = &files["azul_dom.f90"];
        assert!(dom.contains("use azul_types_dom, only:") && dom.contains("AzDom"), "{}", dom);
        assert!(dom.contains("use azul_ffi_dom\n"), "{}", dom);
        assert!(dom.contains("dom_t") && dom.contains("dom_create_body") && dom.contains("Update_RefreshDom"), "{}", dom);
        assert!(!dom.contains("button_t"), "widgets names stay out of azul_dom:\n{}", dom);
        let widgets = &files["azul_widgets.f90"];
        assert!(widgets.contains("button_t") && widgets.contains("button_dom"), "{}", widgets);
        let azul = &files["azul.f90"];
        for u in ["azul_types", "azul_ffi", "azul_api"] {
            assert!(azul.contains(&format!("use {}\n", u)), "azul lacks {}", u);
        }
        let sources: Vec<&str> = files["sources.txt"].lines().collect();
        assert_eq!(sources.first(), Some(&"azul_types_css.f90"));
        assert_eq!(sources.last(), Some(&"azul.f90"));
        let pos = |n: &str| sources.iter().position(|s| *s == n).unwrap_or_else(|| panic!("{} missing", n));
        assert!(pos("azul_types_dom.f90") < pos("azul_types.f90"));
        assert!(pos("azul_types.f90") < pos("azul_ffi_dom.f90"));
        assert!(pos("azul_ffi.f90") < pos("azul_api.f90"));
        assert!(pos("azul_api.f90") < pos("azul_dom.f90"));
        for s in &sources {
            assert!(files.contains_key(*s), "{} listed but not written", s);
        }
        let mk = &files["Makefile"];
        assert!(mk.contains("azul_types_dom.o: azul_types_dom.f90 azul_types_css.o"), "{}", mk);
        assert!(mk.contains("azul_api.o: azul_api.f90 azul_types.o azul_ffi.o"));
    }

    #[test]
    fn per_class_surface_is_grouped_by_module() {
        let files = generated();
        assert!(files["azul_ffi_widgets.f90"].contains("bind(C, name=\"AzButton_dom\")"));
        assert!(!files["azul_ffi_dom.f90"].contains("AzButton_"));
        let api = &files["azul_api.f90"];
        assert!(api.contains("type :: dom_t") && api.contains("type :: button_t"));
        assert!(api.contains(&format!("{}widgets]", API_MODULE_MARKER)));
        assert!(api.contains("use azul_ffi\n") && api.contains("use azul_types\n"));
    }
}
