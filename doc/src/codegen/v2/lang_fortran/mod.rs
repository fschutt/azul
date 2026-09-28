//! Fortran (modern, F2003+) binding generator.
//!
//! Generates a set of Fortran modules behind one facade, `azul`:
//!
//! 1. `azul_types_<unit>.f90` — one module per plan chunk (see
//!    [`super::module_plan::ModulePlan`]; `azul_types_css`,
//!    `azul_types_dom`, `azul_types_css_2`, ...) declaring the C-ABI types
//!    of that chunk as Fortran `type, bind(C)` derived types (the Fortran
//!    spelling of a C struct), unit-only enums as `enum, bind(C)` blocks
//!    (F2003), tagged unions as ABI-opaque blob types with the exact C
//!    size/alignment (Fortran has no native `union`; the blob keeps every
//!    embedding struct layout-identical to `azul.h` — see [`layout`]), and
//!    callback typedefs as `abstract interface`s, plus the named constants
//!    of the classes it declares (`AzGlContextPtr_ACCUM_ALPHA_BITS` and
//!    the 1400-odd other OpenGL enum values) as `integer(kind),
//!    parameter`s. A chunk imports the types it names from the chunks
//!    declaring them (`use ..., only:`, which keeps every `.mod` small);
//!    `azul_types` re-exports them all.
//! 2. `azul_ffi_<module>.f90` — one module per api.json module declaring
//!    that module's C-API functions inside an `interface ... end interface`
//!    block, each with the verbatim C symbol carried via
//!    `bind(C, name="AzFoo_create")`. Fortran is case-insensitive for
//!    its own identifiers, but the `name="..."` argument is case-sensitive
//!    so the linker matches the same exported symbols as the C/C++/Pascal
//!    bindings. `azul_ffi` re-exports them all.
//! 3. `azul_api.f90` — the idiomatic layer (public by default, with its
//!    runtime internals listed `private`): every class as a `<snake>_t`
//!    derived type (`dom_t`, `button_t`, `app_t`) whose methods are
//!    type-bound procedures (`call app%run(window)`), whose constructors
//!    are also reachable through a generic interface named after the type
//!    when one can carry all of them (`btn = button_t('Increase
//!    counter')`, next to the flat `button_create` that keeps working),
//!    whose `String` arguments are
//!    `character(len=*)`, whose unit enums are plain `integer`, and whose
//!    callbacks are ordinary Fortran procedures matching a typed abstract
//!    interface; plus the host-invoker runtime
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
//! 5. `azul.f90` — the facade: `use azul_api`, which is public by default
//!    and so re-exports the raw layer and `iso_c_binding` too; `use azul`
//!    is the only import a program needs. (Merging `azul_types`, `azul_ffi`
//!    and `azul_api` here instead cost gfortran about a minute per build.)
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
//! The generator emits a plain Makefile: gfortran + make is available
//! everywhere, and the release bundle is a flat directory. An `fpm.toml`
//! (Fortran Package Manager) would need the binding shipped as a package
//! directory of its own that a user's project depends on.
//!
//! # Output protocol
//!
//! `generate(ir, config)` returns a single `String` with multiple files
//! separated by [`FILE_MARKER`] / [`END_MARKER`] header lines (each a
//! valid Fortran comment). The orchestrator splits them into one directory.

use std::collections::{BTreeMap, BTreeSet};

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

/// Maximum identifier length permitted since F2003 (Fortran 95 allowed
/// 31). Names longer than this are truncated by [`truncate_identifier`].
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
    // Public names of every chunk generated so far, by chunk index.
    let mut chunk_publics: Vec<BTreeSet<String>> = Vec::new();
    for idx in 0..split.plan.chunks.len() {
        let unit = split.types_unit(idx);
        let src = generate_types_unit(ir, config, &split, idx, &chunk_publics)?;
        chunk_publics.push(public_names(&src));
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

    // 2. C-ABI interface blocks, one module per api.json module. Each one
    // imports exactly the types its interfaces name, from the chunks that
    // declare them.
    let mut type_home: BTreeMap<String, usize> = BTreeMap::new();
    for (idx, names) in chunk_publics.iter().enumerate() {
        for n in names {
            type_home.entry(n.clone()).or_insert(idx);
        }
    }
    let api_modules = split.api_modules(ir);
    let mut ffi_units = Vec::new();
    let mut ffi_deps = Vec::new();
    for m in &api_modules {
        let unit = format!("azul_ffi_{}", m);
        let (src, deps) = generate_ffi_unit(ir, config, &split, m, &type_home)?;
        collect_publics(&src, &unit, m, &mut registry);
        files.push((format!("{}.f90", unit), src));
        order.push(unit.clone());
        ffi_deps.push((unit.clone(), deps));
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
        makefile::generate_makefile(&chunk_deps, &ffi_deps, &api_modules),
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

/// The names a generated unit declares `public :: <name>`.
fn public_names(src: &str) -> BTreeSet<String> {
    src.lines()
        .filter_map(|l| l.trim().strip_prefix("public :: "))
        .map(|n| n.trim().to_string())
        .collect()
}

/// Every `Az*` derived type a generated unit names as `type(AzFoo)`.
fn referenced_types(src: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut rest = src;
    while let Some(i) = rest.find("type(Az") {
        rest = &rest[i + "type(".len()..];
        let end = rest
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .unwrap_or(rest.len());
        if rest[end..].starts_with(')') {
            out.insert(rest[..end].to_string());
        }
        rest = &rest[end..];
    }
    out
}

/// `use <module>, only: <names>` in statements of at most
/// [`ONLY_NAMES_PER_STATEMENT`] names.
fn use_only<'a>(b: &mut CodeBuilder, module: &str, names: impl IntoIterator<Item = &'a String>) {
    let names: Vec<&str> = names.into_iter().map(|s| s.as_str()).collect();
    for group in names.chunks(ONLY_NAMES_PER_STATEMENT) {
        b.line(&format!("use {}, only: {}", module, group.join(", ")));
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
    builder.line("! Requires Fortran 2003 or newer (iso_c_binding, `enum, bind(C)`, `class(*)`);");
    builder.line("! every unit compiles with gfortran -std=f2003.");
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
///
/// Dependencies are imported with `only:` lists of the types this chunk
/// actually names. gfortran copies everything a module use-associates into
/// its `.mod`, so a wholesale `use` makes every chunk's `.mod` carry its
/// whole dependency tree and the modules that merge them (`azul_types`,
/// `azul`) slow to compile.
fn generate_types_unit(
    ir: &CodegenIR,
    config: &CodegenConfig,
    split: &Split,
    idx: usize,
    chunk_publics: &[BTreeSet<String>],
) -> Result<String> {
    let chunk = &split.plan.chunks[idx];
    let mut body = CodeBuilder::new("  ");
    body.indent();
    let members: std::collections::BTreeSet<&str> =
        chunk.types.iter().map(|s| s.as_str()).collect();
    let belongs = |t: &str| members.contains(t);
    types::generate_types_for(&mut body, ir, config, &belongs, split)?;
    let body = body.finish();
    let referenced = referenced_types(&body);

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
        let names: Vec<&String> = chunk_publics
            .get(*dep)
            .map(|p| referenced.intersection(p).collect())
            .unwrap_or_default();
        use_only(&mut b, &split.types_unit(*dep), names);
    }
    b.line("implicit none");
    b.line("private");
    b.blank();
    b.dedent();
    b.raw(&body);
    b.line(&format!("end module {}", unit));
    Ok(b.finish())
}

/// `azul_ffi_<module>`: the C-API interface block of one api.json module.
/// Returns the source and the type chunks it imports from (see
/// [`generate_types_unit`] for why the imports are `only:` lists).
fn generate_ffi_unit(
    ir: &CodegenIR,
    config: &CodegenConfig,
    split: &Split,
    api_module: &str,
    type_home: &BTreeMap<String, usize>,
) -> Result<(String, Vec<String>)> {
    let mut body = CodeBuilder::new("  ");
    body.indent();
    let belongs = |c: &str| split.module_of(c) == api_module;
    functions::generate_externals_for(&mut body, ir, config, &belongs)?;
    let body = body.finish();
    let mut by_chunk: BTreeMap<usize, Vec<String>> = BTreeMap::new();
    for t in referenced_types(&body) {
        if let Some(&idx) = type_home.get(&t) {
            by_chunk.entry(idx).or_default().push(t);
        }
    }

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
    let mut deps = Vec::new();
    for (idx, names) in &by_chunk {
        let dep = split.types_unit(*idx);
        use_only(&mut b, &dep, names);
        deps.push(dep);
    }
    b.line("implicit none");
    b.line("private");
    b.blank();
    b.dedent();
    b.raw(&body);
    b.line(&format!("end module {}", unit));
    Ok((b.finish(), deps))
}

/// `azul_api`: the idiomatic wrappers and the managed runtime.
fn generate_api_unit(ir: &CodegenIR, config: &CodegenConfig, split: &Split) -> Result<String> {
    let mut b = CodeBuilder::new("  ");
    emit_header(
        &mut b,
        "Idiomatic wrappers (`<snake>_t` types with type-bound methods) and the host-invoker runtime.",
    );
    // The plan is built once and shared with the managed layer: both claim
    // module-wide (case-folded) identifiers from the same table.
    let ctx = wrappers::Ctx::new(ir, config);

    b.line("module azul_api");
    b.indent();
    b.line("use, intrinsic :: iso_c_binding");
    b.line("use, intrinsic :: iso_fortran_env, only: error_unit");
    b.line("use azul_types");
    b.line("use azul_ffi");
    b.line("implicit none");
    // PUBLIC by default: this module already loads the whole binding, so it
    // re-exports the raw layer (`Az*` types, `az_*` interfaces, the
    // iso_c_binding entities) for free, and `azul` becomes a re-export of
    // this one module. Merging `azul_types` + `azul_ffi` + `azul_api` in
    // `azul` instead took gfortran about a minute on every first build.
    // The `public ::` lines below stay: the per-module facades are built
    // from them.
    b.line("public");
    b.line("private :: error_unit");
    let mut internals = managed::private_names(&ctx);
    internals.extend(ctx.classes.iter().filter_map(|c| c.take_name.clone()));
    for group in internals.chunks(ONLY_NAMES_PER_STATEMENT) {
        b.line(&format!("private :: {}", group.join(", ")));
    }
    b.blank();

    // Wrapper type declarations (inside the module decl section — Fortran
    // modules separate type declarations from procedure bodies via the
    // `contains` keyword below).
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
    b.line("! `azul_api` re-exports the raw layer (`azul_types`, `azul_ffi`) and");
    b.line("! iso_c_binding, so `use azul` is the only import a program needs, even");
    b.line("! when it reaches for the raw `az_*` layer.");
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

/// `iso_c_binding` entities a user program is expected to reach for next
/// to the raw layer. `azul_api` uses `iso_c_binding` wholesale and is
/// public by default, so `use azul` re-exports these (and the rest of
/// the intrinsic module); the wrapper layer keeps its names clear of them.
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

        // Booleans. The OpenGL scalar typedefs (`GLuint`, `GLsizeiptr`,
        // ...) are NOT listed in this table: api.json declares each of
        // them as a type alias of a Rust primitive, so the alias arm at
        // the bottom resolves them to the very same Fortran kinds, and a
        // second, hand-kept copy of that mapping here could only ever
        // drift from it. `GLboolean` is the one exception and it is a
        // deliberate one.
        //
        // It aliases `u8` but is a BOOLEAN by contract (GL_TRUE /
        // GL_FALSE), and `logical(c_bool)` is the same single byte, so
        // Fortran callers get `.true.`/`.false.` instead of 1/0; nothing
        // but the name tells it apart from any other `u8`.
        // allow-api-name: the one GL scalar whose Fortran type is not its alias target's.
        "bool" | "GLboolean" => "logical(c_bool)".to_string(),

        // Signed / unsigned integers via iso_c_binding kind selectors.
        // Fortran has no unsigned-integer kind, so `u8`/`u16`/... map to
        // the same kind as their signed counterparts; this is the same
        // approach the Pascal/Ada bindings take.
        "i8" | "u8" | "c_char" | "char" | "c_uchar" => "integer(c_int8_t)".to_string(),
        "i16" | "u16" => "integer(c_int16_t)".to_string(),
        "i32" | "u32" | "c_int" | "c_uint" => "integer(c_int32_t)".to_string(),
        "i64" | "u64" => "integer(c_int64_t)".to_string(),
        "f32" => "real(c_float)".to_string(),
        "f64" => "real(c_double)".to_string(),
        "usize" | "size_t" | "uintptr_t" => "integer(c_size_t)".to_string(),
        "isize" | "ssize_t" | "intptr_t" => "integer(c_intptr_t)".to_string(),

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
///
/// This is Fortran's grammar, not api.json's vocabulary: that `end`,
/// `contains`, `bind`, `min` and `max` are also api.json method names is
/// the reason they are listed, not a decision keyed on those methods.
pub fn is_fortran_reserved(name: &str) -> bool {
    matches!( // allow-api-name: a keyword table - Fortran's words, not the API's.
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
        assert!(dom.contains("use azul_types_css, only: AzColor0"), "dom embeds Color0:\n{}", dom);
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
        assert!(azul.contains("use azul_api\n"), "{}", azul);
        let api = &files["azul_api.f90"];
        for u in ["azul_types", "azul_ffi"] {
            assert!(api.contains(&format!("use {}\n", u)), "azul_api lacks {}", u);
        }
        assert!(api.contains("\n  public\n"), "azul_api re-exports the raw layer");
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
    fn ffi_units_import_only_the_types_they_name() {
        let files = generated();
        let ffi = &files["azul_ffi_dom.f90"];
        assert!(ffi.contains("use azul_types_dom, only: "), "{}", ffi);
        assert!(!ffi.contains("use azul_types\n"), "no wholesale import:\n{}", ffi);
        let mk = &files["Makefile"];
        assert!(mk.contains("azul_ffi_dom.o: azul_ffi_dom.f90 azul_types_"), "{}", mk);
        assert!(!mk.contains("azul_ffi_dom.o: azul_ffi_dom.f90 azul_types.o"), "{}", mk);
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
