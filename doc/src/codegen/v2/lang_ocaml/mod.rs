//! OCaml binding generator.
//!
//! Produces a dune library of many small compilation units behind one
//! facade, `Azul`: user code keeps writing `Azul.Dom.create_body ()` and
//! `Azul.azul_refany_get`, but `ocamlopt` no longer sees one 8 MB /
//! 120k-line `azul.ml` (which overflowed an 8 MB stack and took minutes).
//!
//! The split follows [`super::module_plan::ModulePlan`] and is layered so
//! every unit only depends on units below it (OCaml compilation units
//! cannot be mutually recursive):
//!
//! 1. `azul_loader.ml` — the `Dl.dlopen` of libazul, forced before any `foreign` lookup by every
//!    FFI unit.
//! 2. `azul_types_<unit>.ml` — one unit per plan chunk (`azul_types_css`, `azul_types_dom`,
//!    `azul_types_css_2`, ...): the Ctypes `structure` stubs, fields + `seal`, unit-enum constants
//!    and tagged-union blobs of that chunk's types. A chunk `open`s the chunks it references.
//!    `azul_types.ml` `include`s them all. See `types.rs`.
//! 3. `azul_ffi_<module>.ml` — the `foreign "<C symbol>" (...)` bindings of one api.json module's
//!    classes; `azul_ffi.ml` includes them all. See `functions.rs`.
//! 4. `azul_enums_<module>.ml` — the enum modules of one api.json module: the ADT of every unit
//!    enum (`module Update = struct type t = | DoNothing | RefreshDom ... end`) and the derive
//!    capabilities of every enum, typed on `t`. Below the class modules so any signature can name
//!    `<Enum>.t`; `azul_enums.ml` includes them all. See `wrappers.rs`.
//! 5. `azul_records_<module>.ml` — the wrapper records with their `Gc.finalise` finalisers (`type
//!    app = { mutable raw; mutable disposed }`, `make_app`, `dispose_app`, `raw_app`) of one
//!    api.json module; `azul_records.ml` includes them all.
//! 6. `azul_managed.ml` — the host-invoker runtime (handle table, typed per-kind invokers,
//!    `azul_refany_create` / `azul_refany_get`, `azul_register_<kind>`,
//!    `azul_<class>_with_layout`). Its helpers return records, hence below them. See `managed.rs`.
//! 7. `azul_api_<module>.ml` / `.mli` — the idiomatic per-class submodules (`module Dom : sig ...
//!    end`) and the polymorphic-variant views of one api.json module. The `.mli` seals them. See
//!    `wrappers.rs`.
//! 8. `azul.ml` — the facade: `include`s every layer.
//!
//! Besides the units, `generate` emits the project files `azul.opam` and
//! `hello_world.ml` (see `dune.rs`); `dune` / `dune-project` are written by
//! the orchestrator from the same module.
//!
//! dune wraps the library, so only `Azul` is visible to consumers; the
//! internal units are reachable as `Azul.<name>` through the includes.
//!
//! ## Surface
//!
//! - FFI TYPE identifiers are `lower_snake_case` (OCaml's type-name convention) — `az_app`,
//!   `az_layout_callback_info`. FFI FUNCTION values keep the C symbol's own spelling with only
//!   the first letter lowered — `azApp_create` for `AzApp_create` — so the symbol in `azul.h`,
//!   in the `foreign` link string and at the call site is one greppable string, and the
//!   mixed-case name can never collide with an all-lowercase `typ` value.
//! - The `foreign "<C symbol>" (...)` link name uses the **exact** C symbol from the IR
//!   (`AzApp_create`), never the OCaml-snake form.
//! - Idiomatic surface lives inside nested modules: `Azul.App.create`, `Azul.App.run`, etc. The
//!   `Az_` / `Az` prefix is dropped.
//! - Unit enums are ADTs (`Azul.Update.RefreshDom`); tagged-union enums are surfaced as
//!   polymorphic-variant views (`[ \`None | \`Some of int ]`).
//!
//! ## Output protocol
//!
//! `generate(ir, config)` returns a single `String` with multiple files
//! (OCaml units, `azul.opam`, `hello_world.ml`) separated by [`FILE_MARKER`] /
//! [`END_MARKER`] header lines:
//!
//! ```text
//! (*==FILE: azul_types_css.ml ==*)
//! <contents>
//! (*==FILE: azul_api_dom.mli ==*)
//! <contents>
//! ```
//!
//! The marker is a valid OCaml block comment so the combined text still
//! parses if it is not split. The orchestrator splits on the marker and
//! writes each part next to the others (they must share a directory for
//! dune).

pub mod dune;
pub mod functions;
pub mod managed;
pub mod types;
pub mod wrappers;

use std::collections::BTreeSet;

use anyhow::Result;

use super::{
    config::CodegenConfig,
    generator::CodeBuilder,
    ir::{CodegenIR, FunctionKind, TypeCategory},
    managed_lang_helpers::is_refany_type,
    module_plan::ModulePlan,
};

/// Library link name passed to `Dl.dlopen` and used by `foreign` to
/// resolve the prebuilt artifact at runtime.
pub const LIB_NAME: &str = "azul";

/// File-marker header introducing each per-file section of the output.
pub const FILE_MARKER: &str = "(*==FILE: ";

/// Trailing marker closing the file-marker header line.
pub const END_MARKER: &str = " ==*)";

/// The api.json module a class the plan does not know (a function-only
/// class) is filed under.
const FALLBACK_MODULE: &str = "misc";

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
        let mut mods: BTreeSet<String> = self.plan.api_modules().into_iter().collect();
        for f in &ir.functions {
            mods.insert(self.module_of(&f.class_name));
        }
        mods.into_iter().collect()
    }

    /// Unit (file stem) of a types chunk: `azul_types_css_2`.
    pub fn types_unit(&self, chunk_idx: usize) -> String {
        format!("azul_types_{}", self.plan.chunks[chunk_idx].name)
    }
}

/// `azul_types_css` -> `Azul_types_css`: the OCaml module name of a unit.
pub fn unit_module(unit: &str) -> String {
    let mut c = unit.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

/// Public entry point. Generates every unit of the OCaml binding,
/// concatenated with file markers.
pub fn generate(ir: &CodegenIR, config: &CodegenConfig) -> Result<String> {
    let split = Split::new(ir);
    let mut files: Vec<(String, String)> = Vec::new();

    // 1. Loader.
    files.push(("azul_loader.ml".to_string(), generate_loader(config)));

    // 2. Types, one unit per plan chunk, then the facade.
    let mut type_units = Vec::new();
    for idx in 0..split.plan.chunks.len() {
        let unit = split.types_unit(idx);
        files.push((
            format!("{}.ml", unit),
            generate_types_unit(ir, config, &split, idx)?,
        ));
        type_units.push(unit);
    }
    files.push((
        "azul_types.ml".to_string(),
        generate_include_facade(
            config,
            "Every FFI type of the binding: the union of the per-module chunks.",
            &type_units,
        ),
    ));

    // 3. FFI bindings per api.json module, then the facade.
    let api_modules = split.api_modules(ir);
    let mut ffi_units = Vec::new();
    for m in &api_modules {
        let unit = format!("azul_ffi_{}", m);
        files.push((
            format!("{}.ml", unit),
            generate_ffi_unit(ir, config, &split, m)?,
        ));
        ffi_units.push(unit);
    }
    files.push((
        "azul_ffi.ml".to_string(),
        generate_include_facade(
            config,
            "Every raw `foreign` binding of the binding: the union of the per-module units.",
            &ffi_units,
        ),
    ));

    // 4. Enum modules per api.json module, then the facade.
    let mut enum_units = Vec::new();
    for m in &api_modules {
        let unit = format!("azul_enums_{}", m);
        files.push((
            format!("{}.ml", unit),
            generate_enums_unit(ir, config, &split, m)?,
        ));
        enum_units.push(unit);
    }
    files.push((
        "azul_enums.ml".to_string(),
        generate_include_facade(
            config,
            "Every enum module of the binding: the union of the per-module units.",
            &enum_units,
        ),
    ));

    // 5. Wrapper records per api.json module, then the facade.
    let mut record_units = Vec::new();
    for m in &api_modules {
        let unit = format!("azul_records_{}", m);
        files.push((
            format!("{}.ml", unit),
            generate_records_unit(ir, config, &split, m)?,
        ));
        record_units.push(unit);
    }
    files.push((
        "azul_records.ml".to_string(),
        generate_include_facade(
            config,
            "Every wrapper record of the binding: the union of the per-module units.",
            &record_units,
        ),
    ));

    // 6. Managed runtime (returns records, so after them).
    files.push((
        "azul_managed.ml".to_string(),
        generate_managed_unit(ir, config),
    ));

    // 7. Idiomatic per-class modules per api.json module (.ml + .mli).
    let mut api_units = Vec::new();
    for m in &api_modules {
        let unit = format!("azul_api_{}", m);
        let (ml, mli) = generate_api_unit(ir, config, &split, m)?;
        files.push((format!("{}.ml", unit), ml));
        files.push((format!("{}.mli", unit), mli));
        api_units.push(unit);
    }

    // 8. The facade.
    files.push((
        "azul.ml".to_string(),
        generate_facade(
            config,
            &type_units,
            &ffi_units,
            &enum_units,
            &record_units,
            &api_units,
        ),
    ));

    // 9. Project files: the opam manifest `opam install . --deps-only` reads
    //    and the guide's example, so the output directory is the complete
    //    project the release tarball ships.
    files.push(("azul.opam".to_string(), dune::generate_opam(&ir.api_version)));
    files.push(("hello_world.ml".to_string(), dune::hello_world().to_string()));

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

/// The unit names (file stems) `generate` emits, in dependency order —
/// what the dune `(modules ...)` field and the release bundle list.
pub fn unit_names(ir: &CodegenIR) -> Vec<String> {
    let split = Split::new(ir);
    let mut out = vec!["azul_loader".to_string()];
    for idx in 0..split.plan.chunks.len() {
        out.push(split.types_unit(idx));
    }
    out.push("azul_types".to_string());
    let api_modules = split.api_modules(ir);
    for m in &api_modules {
        out.push(format!("azul_ffi_{}", m));
    }
    out.push("azul_ffi".to_string());
    for m in &api_modules {
        out.push(format!("azul_enums_{}", m));
    }
    out.push("azul_enums".to_string());
    for m in &api_modules {
        out.push(format!("azul_records_{}", m));
    }
    out.push("azul_records".to_string());
    out.push("azul_managed".to_string());
    for m in &api_modules {
        out.push(format!("azul_api_{}", m));
    }
    out.push("azul".to_string());
    out
}

// ============================================================================
// Per-unit builders
// ============================================================================

fn unit_header(builder: &mut CodeBuilder, what: &str) {
    builder.line("(* ============================================================================");
    builder.line(&format!(" * {}", what));
    builder.line(" * Auto-generated OCaml bindings for the Azul GUI framework.");
    builder.line(" * Generated by azul-doc codegen v2 (lang_ocaml). DO NOT EDIT MANUALLY.");
    builder
        .line(" * ============================================================================ *)");
    builder.blank();
}

/// A unit that only `include`s other units.
fn generate_include_facade(config: &CodegenConfig, what: &str, units: &[String]) -> String {
    let mut b = CodeBuilder::new(&config.indent);
    unit_header(&mut b, what);
    for u in units {
        b.line(&format!("include {}", unit_module(u)));
    }
    b.finish()
}

/// `azul_loader.ml`: the dlopen every FFI unit forces first.
fn generate_loader(config: &CodegenConfig) -> String {
    let mut builder = CodeBuilder::new(&config.indent);
    unit_header(&mut builder, "Native library loader.");
    builder.line("(* Force the dynamic loader to bring in libazul up-front so all `foreign`");
    builder.line("   lookups resolve from the same handle. RTLD_GLOBAL lets the");
    builder.line("   library's transitive dependencies (e.g. system OpenGL) link too.");
    builder.line("   Try each platform-conventional filename in turn so the binding");
    builder.line("   loads on Linux, macOS, and Windows without manual configuration.");
    builder.line("   The first match wins; failures are silenced (logged via stderr).");
    builder.line("   Users can override the search by setting AZ_DYLIB.");
    builder.line("   Every FFI unit calls [ensure] at its top so the load happens before");
    builder.line("   its first `foreign` lookup, whatever the link order. *)");
    builder.line("let loaded = ref false");
    builder.blank();
    builder.line("let ensure () =");
    builder.indent();
    builder.line("if not !loaded then begin");
    builder.indent();
    builder.line("loaded := true;");
    builder.line("let candidates = match Sys.getenv_opt \"AZ_DYLIB\" with");
    builder.indent();
    builder.line("| Some p when String.length p > 0 -> [p]");
    builder.line(
        "| _ -> [\"libazul.dylib\"; \"libazul.so\"; \"azul.dll\"; \"./libazul.dylib\"; \
         \"./libazul.so\"]",
    );
    builder.dedent();
    builder.line("in");
    builder.line("let rec try_load = function");
    builder.indent();
    builder.line("| [] -> ()");
    builder.line("| candidate :: rest ->");
    builder.indent();
    builder
        .line("(try ignore (Dl.dlopen ~filename:candidate ~flags:[Dl.RTLD_LAZY; Dl.RTLD_GLOBAL])");
    builder.line(" with Dl.DL_error _ -> try_load rest)");
    builder.dedent();
    builder.dedent();
    builder.line("in try_load candidates");
    builder.dedent();
    builder.line("end");
    builder.dedent();
    builder.blank();
    builder.line("let () = ensure ()");
    builder.finish()
}

/// `azul_types_<unit>.ml`: one plan chunk's types.
fn generate_types_unit(
    ir: &CodegenIR,
    config: &CodegenConfig,
    split: &Split,
    idx: usize,
) -> Result<String> {
    let chunk = &split.plan.chunks[idx];
    let mut builder = CodeBuilder::new(&config.indent);
    unit_header(
        &mut builder,
        &format!(
            "FFI types of the api.json module `{}` (unit {} of the split).",
            chunk.api_module, chunk.ordinal
        ),
    );
    builder.line("open Ctypes");
    for dep in &chunk.deps {
        builder.line(&format!("open {}", unit_module(&split.types_unit(*dep))));
    }
    builder.blank();
    let members: BTreeSet<&str> = chunk.types.iter().map(|s| s.as_str()).collect();
    let belongs = |t: &str| members.contains(t);
    types::emit_types_chunk(&mut builder, ir, config, &belongs)?;
    Ok(builder.finish())
}

/// `azul_ffi_<module>.ml`: the `foreign` bindings of one api.json module.
fn generate_ffi_unit(
    ir: &CodegenIR,
    config: &CodegenConfig,
    split: &Split,
    api_module: &str,
) -> Result<String> {
    let mut builder = CodeBuilder::new(&config.indent);
    unit_header(
        &mut builder,
        &format!(
            "Raw `foreign` bindings of the api.json module `{}`.",
            api_module
        ),
    );
    builder.line("open Ctypes");
    builder.line("open Foreign");
    builder.line("open Azul_types");
    builder.blank();
    builder.line("let () = Azul_loader.ensure ()");
    builder.blank();
    let belongs = |c: &str| split.module_of(c) == api_module;
    functions::emit_foreign_bindings_for(&mut builder, ir, config, &belongs)?;
    Ok(builder.finish())
}

/// `azul_enums_<module>.ml`: the enum modules of one api.json module.
fn generate_enums_unit(
    ir: &CodegenIR,
    config: &CodegenConfig,
    split: &Split,
    api_module: &str,
) -> Result<String> {
    let mut builder = CodeBuilder::new(&config.indent);
    unit_header(
        &mut builder,
        &format!(
            "Enum modules and constants of the api.json module `{}`.",
            api_module
        ),
    );
    builder.line("open Ctypes");
    builder.line("open Azul_types");
    builder.line("open Azul_ffi");
    builder.blank();
    let belongs = |c: &str| split.module_of(c) == api_module;
    wrappers::emit_enum_modules_for(&mut builder, ir, config, &belongs)?;
    // The api.json constants of this module. They are plain values with no
    // dependency on anything else, and this is the lowest layer that is
    // split per api.json module, so they ride along here.
    wrappers::emit_constants_for(&mut builder, ir, api_module);
    Ok(builder.finish())
}

/// `azul_managed.ml`: the host-invoker runtime.
fn generate_managed_unit(ir: &CodegenIR, config: &CodegenConfig) -> String {
    let mut builder = CodeBuilder::new(&config.indent);
    unit_header(&mut builder, "Managed-FFI runtime (host-invoker pattern).");
    builder.line("open Ctypes");
    builder.line("open Foreign");
    builder.line("open Azul_types");
    builder.line("open Azul_ffi");
    builder.line("open Azul_enums");
    builder.line("open Azul_records");
    builder.blank();
    builder.line("let () = Azul_loader.ensure ()");
    let records = wrappers::record_types(ir, config);
    managed::emit_managed_prelude(&mut builder, ir, &records);
    builder.finish()
}

/// `azul_records_<module>.ml`: the wrapper records of one api.json module.
fn generate_records_unit(
    ir: &CodegenIR,
    config: &CodegenConfig,
    split: &Split,
    api_module: &str,
) -> Result<String> {
    let mut builder = CodeBuilder::new(&config.indent);
    unit_header(
        &mut builder,
        &format!(
            "Wrapper records (Gc.finalise-managed) of the api.json module `{}`.",
            api_module
        ),
    );
    builder.line("open Ctypes");
    builder.line("open Azul_types");
    builder.line("open Azul_ffi");
    builder.blank();
    let belongs = |c: &str| split.module_of(c) == api_module;
    wrappers::emit_wrapper_records_for(&mut builder, ir, config, &belongs)?;
    Ok(builder.finish())
}

/// `azul_api_<module>.ml` + `.mli`: the idiomatic per-class modules of one
/// api.json module.
fn generate_api_unit(
    ir: &CodegenIR,
    config: &CodegenConfig,
    split: &Split,
    api_module: &str,
) -> Result<(String, String)> {
    let belongs = |c: &str| split.module_of(c) == api_module;
    let opens = |b: &mut CodeBuilder| {
        b.line("open Ctypes");
        b.line("open Azul_types");
        b.line("open Azul_ffi");
        b.line("open Azul_enums");
        b.line("open Azul_records");
        b.line("open Azul_managed");
        b.blank();
    };

    let mut ml = CodeBuilder::new(&config.indent);
    unit_header(
        &mut ml,
        &format!(
            "Idiomatic per-class modules of the api.json module `{}` (implementation).",
            api_module
        ),
    );
    opens(&mut ml);
    wrappers::emit_idiomatic_module_implementation_for(&mut ml, ir, config, &belongs)?;

    let mut mli = CodeBuilder::new(&config.indent);
    unit_header(
        &mut mli,
        &format!(
            "Idiomatic per-class modules of the api.json module `{}` (interface).",
            api_module
        ),
    );
    opens(&mut mli);
    wrappers::emit_idiomatic_module_interface_for(&mut mli, ir, config, &belongs)?;

    Ok((ml.finish(), mli.finish()))
}

/// `azul.ml`: the facade every consumer opens.
fn generate_facade(
    config: &CodegenConfig,
    type_units: &[String],
    ffi_units: &[String],
    enum_units: &[String],
    record_units: &[String],
    api_units: &[String],
) -> String {
    let mut b = CodeBuilder::new(&config.indent);
    unit_header(
        &mut b,
        "The `Azul` module: every unit of the binding, included in dependency order.",
    );
    b.line("(* The binding is split into one unit per api.json module (and per");
    b.line("   dependency slice of it for the types). This facade includes them all,");
    b.line("   so `Azul.Dom.body`, `Azul.Update.RefreshDom`, `Azul.az_dom`,");
    b.line("   `Azul.azDom_delete` and `Azul.azul_refany_get` all resolve. *)");
    b.blank();
    b.line("include Azul_loader");
    for u in type_units {
        b.line(&format!("include {}", unit_module(u)));
    }
    for u in ffi_units {
        b.line(&format!("include {}", unit_module(u)));
    }
    for u in enum_units {
        b.line(&format!("include {}", unit_module(u)));
    }
    for u in record_units {
        b.line(&format!("include {}", unit_module(u)));
    }
    b.line("include Azul_managed");
    for u in api_units {
        b.line(&format!("include {}", unit_module(u)));
    }
    b.finish()
}

// ============================================================================
// ============================================================================
// Shared helpers (used by submodules)
// ============================================================================

/// Convert an IR type name (`PascalCase`) to the OCaml FFI struct
/// identifier (`lower_snake_case` with the `az_` prefix).
///
/// Example: `App` -> `az_app`, `LayoutCallbackInfo` ->
/// `az_layout_callback_info`.
pub fn ocaml_ffi_type_name(name: &str) -> String {
    format!("az_{}", to_snake_case(name))
}

/// The `foreign` value of the one function of `class` with `kind`, or
/// `None` when the class does not export that capability.
fn binding_of(ir: &CodegenIR, class: &str, kind: FunctionKind) -> Option<String> {
    ir.functions_for_class(class)
        .find(|f| f.kind == kind)
        .map(|f| functions::ocaml_binding_name(&f.c_name))
}

/// The IR's string type (`TypeCategory::String`): the one type a wrapper
/// method takes as a plain OCaml `string` and converts on the way in.
/// Category, not name, so an api.json rename travels with it.
pub fn is_string_type(ir: &CodegenIR, type_name: &str) -> bool {
    ir.find_struct(type_name.trim())
        .is_some_and(|s| s.category == TypeCategory::String)
}

/// The name of the IR's host-data handle type (`TypeCategory::RefAny`),
/// for the few places that must SPELL it (the `azul_refany_create` helper
/// names its return type).
pub fn refany_type_name(ir: &CodegenIR) -> Option<&str> {
    ir.structs
        .iter()
        .find(|s| is_refany_type(&s.name, ir))
        .map(|s| s.name.as_str())
}

/// The `foreign` value that deep-copies the host-data handle
/// (`TypeCategory::RefAny`). Every wrapper that hands one to C clones
/// it first, so the callee's `_delete` frees the copy and the caller
/// keeps its own. Looked up by CATEGORY and KIND so an api.json
/// rename of the type or the method travels with it.
pub fn refany_clone_binding(ir: &CodegenIR) -> Option<String> {
    let s = ir.structs.iter().find(|s| is_refany_type(&s.name, ir))?;
    binding_of(ir, &s.name, FunctionKind::DeepCopy)
}

/// A type alias whose C form is an AGGREGATE - `union AzLayoutClearValue`,
/// `struct AzPhysicalSizeU32` - rather than a scalar (`GLuint = u32`) or an
/// opaque word. Those cross the ABI by value, so their OCaml view is a
/// sealed `Ctypes.structure` like any other by-value type (see
/// `types::emit_type_alias`) and every signature naming one must say
/// `Ctypes.structure` too.
///
/// The rule is the IR's own `is_value_aggregate`, so this emitter and the C
/// header cannot drift apart on which aliases are unions.
pub fn alias_is_aggregate(ir: &CodegenIR, name: &str) -> bool {
    let name = name.trim();
    ir.find_type_alias(name).is_some() && ir.is_value_aggregate(name)
}

/// The `foreign` value that frees the IR's string type
/// (`TypeCategory::String`). Used wherever a C entry point returns a
/// string BY VALUE (`_toDbgString`): the bytes are copied into an
/// OCaml `string` and the C buffer is freed on the spot, because no
/// wrapper record and so no finaliser ever owns it.
pub fn string_delete_binding(ir: &CodegenIR) -> Option<String> {
    let s = ir
        .structs
        .iter()
        .find(|s| s.category == TypeCategory::String)?;
    binding_of(ir, &s.name, FunctionKind::Delete)
}

/// Convert an IR type name (`PascalCase`) to the user-facing wrapper
/// record name (`lower_snake_case`, no prefix). Shadow-prone names
/// get a `_wrapper` suffix instead of the `az_` prefix used at the
/// FFI level — `az_string` is already the FFI typ; the wrapper must
/// be a distinct identifier.
///
/// Example: `App` -> `app`; `String` -> `string_wrapper`.
pub fn ocaml_wrapper_type_name(name: &str) -> String {
    let snake = sanitize_identifier(&to_snake_case(name));
    if shadows_ocaml_primitive(&snake) {
        format!("{}_wrapper", snake)
    } else {
        snake
    }
}

fn shadows_ocaml_primitive(s: &str) -> bool {
    // That some of these spell an api.json method name (`array`) is a
    // coincidence of English; nothing here reads api.json.
    matches!(
        s,
        "string"
            | "bool"
            | "int"
            | "char"
            | "float"
            | "list"
            // allow-api-name: OCaml's `array` type, not api.json's method.
            | "array"
            | "option"
            | "result"
            | "unit"
            | "bytes"
            | "ref"
            | "exn"
    )
}

/// Convert an IR type name (`PascalCase`) to the user-facing module
/// name in the idiomatic surface (`Pascal_Case` with submodule chain
/// preserved). The `Az` prefix is dropped at this layer.
///
/// Example: `App` -> `App`, `LayoutCallbackInfo` -> `Layout_callback_info`.
///
/// OCaml module names must be capitalised; we keep the first letter
/// upper and treat subsequent CamelHumps as snake-cased segments to
/// keep names short and readable.
pub fn ocaml_module_name(name: &str) -> String {
    // Module names in OCaml are conventionally UpperCamelCase
    // ("RefAny", "WindowCreateOptions"). The previous implementation
    // produced `Ref_any` (snake-with-leading-cap) which is legal but
    // doesn't match the hand-written hello-world's `RefAny.wrap`
    // calls and isn't idiomatic. Strip the leading `Az`/`Iface`
    // prefix if present, then upper-camel-case the rest.
    let body = name.strip_prefix("Az").unwrap_or(name);
    // Re-split on underscores (in case the input was snake) and on
    // case boundaries (in case it was already camel) so we get a
    // consistent sequence of words.
    let snake = to_snake_case(body);
    let mut out = String::with_capacity(snake.len());
    let mut at_word_start = true;
    for c in snake.chars() {
        if c == '_' {
            at_word_start = true;
            continue;
        }
        if at_word_start {
            out.extend(c.to_uppercase());
            at_word_start = false;
        } else {
            out.push(c);
        }
    }
    if out.is_empty() {
        "M".to_string()
    } else {
        out
    }
}

/// Map a Rust/IR type name (with optional pointer/reference prefix) to
/// the matching OCaml `Ctypes` view expression.
///
/// Pointer / reference variants collapse to `(ptr <inner>)` if the
/// inner type is known to the IR, otherwise to `(ptr void)`.
/// `*const c_char` becomes `string` (Ctypes does the C-string
/// marshalling).
/// Map a Rust IR type name to its OCaml-position TYPE string for use
/// in `val foo : T -> U` signatures inside the .mli interface. Where
/// Ctypes' value-level typ name and the corresponding OCaml type
/// differ (e.g. `uint8_t : uint8_t typ` is a value, but the OCaml
/// type alias is `Unsigned.UInt8.t`), return the OCaml type.
pub fn map_type_to_ocaml_typ(rust_type: &str, ir: &CodegenIR) -> String {
    let trimmed = rust_type.trim();

    // Pointer/reference forms — use the postfix type-position form.
    if let Some(rest) = trimmed.strip_prefix("*const ") {
        let inner = rest.trim();
        if inner == "c_char" || inner == "u8" {
            return "string".to_string();
        }
        return inner_pointer_form_type(inner, ir);
    }
    if let Some(rest) = trimmed.strip_prefix("*mut ") {
        return inner_pointer_form_type(rest.trim(), ir);
    }
    if let Some(rest) = trimmed.strip_prefix("&mut ") {
        return inner_pointer_form_type(rest.trim(), ir);
    }
    if let Some(rest) = trimmed.strip_prefix('&') {
        return inner_pointer_form_type(rest.trim(), ir);
    }

    match trimmed {
        "bool" => "bool".to_string(),
        // Sized integers — OCaml type names in `Unsigned.*` and
        // `Signed.*`, NOT the bare ctypes value names. `int`
        // suffices for any small width where preserving the exact
        // representation doesn't matter at the Haskell level —
        // the value-position emit handles the precise C ABI width.
        "u8" | "c_uchar" => "Unsigned.UInt8.t".to_string(),
        "i8" | "c_char" => "int".to_string(), // Signed.SInt8.t exists but `int` is more ergonomic
        "char" => "char".to_string(),
        "u16" => "Unsigned.UInt16.t".to_string(),
        "i16" => "int".to_string(),
        "u32" | "c_uint" => "Unsigned.UInt32.t".to_string(),
        "i32" | "c_int" => "int32".to_string(),
        "u64" => "Unsigned.UInt64.t".to_string(),
        "i64" => "int64".to_string(),
        "f32" => "float".to_string(),
        "f64" => "float".to_string(),
        // `size_t` value-position is the Ctypes typ. The actual OCaml
        // type is `Unsigned.Size_t.t`. Same for ptrdiff_t / isize.
        // Use the precise Ctypes module paths so the mli matches what
        // `foreign ... size_t @-> ... @-> returning ...` actually
        // returns.
        "usize" => "Unsigned.Size_t.t".to_string(),
        "isize" => "Ctypes.Ptrdiff.t".to_string(),
        "c_void" | "()" | "void" => "unit".to_string(),

        _ => {
            // Struct types passed by value across the FFI are
            // represented at the value level as `T Ctypes.structure`;
            // the type-position emit must say the same so the mli
            // signature matches the impl's actual return type.
            // EXCEPT for filtered-out categories (Recursive, VecRef,
            // DestructorOrClone) which the codegen emits as opaque
            // `type T = unit ptr` placeholders — those use the bare
            // name in both positions.
            if let Some(s) = ir.find_struct(trimmed) {
                // `VecRef` is a sealed two-field structure like any other
                // (see `types::should_emit_struct`), so it says
                // `Ctypes.structure` here too.
                if matches!(
                    s.category,
                    super::ir::TypeCategory::Recursive | super::ir::TypeCategory::DestructorOrClone
                ) {
                    return ocaml_ffi_type_name(trimmed);
                }
                return format!("{} Ctypes.structure", ocaml_ffi_type_name(trimmed));
            }
            // A unit enum is its ADT `<Enum>.t` (from `Azul_enums`); a tagged
            // union is a `Ctypes.structure` (a payload byte-array inside a
            // struct); filtered categories are the opaque FFI name.
            if let Some(e) = ir.find_enum(trimmed) {
                if let Some(m) = unit_enum_module(trimmed, ir) {
                    return format!("{}.t", m);
                }
                if e.is_union
                    && !matches!(
                        e.category,
                        super::ir::TypeCategory::Recursive
                            | super::ir::TypeCategory::VecRef
                            | super::ir::TypeCategory::DestructorOrClone
                    )
                {
                    return format!("{} Ctypes.structure", ocaml_ffi_type_name(trimmed));
                }
                return ocaml_ffi_type_name(trimmed);
            }
            // An aggregate alias is a sealed structure like any other
            // by-value union; a scalar alias (`type az_gluint =
            // Unsigned.UInt32.t`) and a callback typedef are their own name.
            if alias_is_aggregate(ir, trimmed) {
                return format!("{} Ctypes.structure", ocaml_ffi_type_name(trimmed));
            }
            if ir.find_type_alias(trimmed).is_some()
                || ir.callback_typedefs.iter().any(|c| c.name == trimmed)
            {
                ocaml_ffi_type_name(trimmed)
            } else {
                "(unit ptr)".to_string()
            }
        }
    }
}

/// The ADT module of a unit enum (`Update` -> `Some("Update")`); `None` for
/// tagged unions, filtered categories, generics and non-enums. The module
/// lives in `Azul_enums_<module>`; every unit naming `<Enum>.t` opens
/// `Azul_enums`. `wrappers::emit_enum_modules_for` emits exactly the modules
/// this returns `Some` for.
pub fn unit_enum_module(type_name: &str, ir: &CodegenIR) -> Option<String> {
    let e = ir.find_enum(type_name.trim())?;
    // A variant-less unit enum has no `az_x` typ either (types.rs skips it).
    if e.is_union
        || e.variants.is_empty()
        || !e.generic_params.is_empty()
        || matches!(
            e.category,
            super::ir::TypeCategory::Recursive
                | super::ir::TypeCategory::VecRef
                | super::ir::TypeCategory::DestructorOrClone
                | super::ir::TypeCategory::GenericTemplate
        )
    {
        return None;
    }
    Some(ocaml_module_name(&e.name))
}

pub fn map_type_to_ocaml(rust_type: &str, ir: &CodegenIR) -> String {
    let trimmed = rust_type.trim();

    // Pointer/reference forms.
    if let Some(rest) = trimmed.strip_prefix("*const ") {
        let inner = rest.trim();
        if inner == "c_char" || inner == "u8" {
            return "string".to_string();
        }
        return inner_pointer_form(inner, ir);
    }
    if let Some(rest) = trimmed.strip_prefix("*mut ") {
        return inner_pointer_form(rest.trim(), ir);
    }
    if let Some(rest) = trimmed.strip_prefix("&mut ") {
        return inner_pointer_form(rest.trim(), ir);
    }
    if let Some(rest) = trimmed.strip_prefix('&') {
        return inner_pointer_form(rest.trim(), ir);
    }

    match trimmed {
        // Primitives. We use Ctypes' explicit-width integer views so the
        // memory layout matches the C ABI on every host (OCaml's native
        // `int` is 63-bit on 64-bit systems, which would mis-align a
        // C `int` field).
        "bool" => "bool".to_string(),
        "u8" | "c_uchar" => "uint8_t".to_string(),
        "i8" | "c_char" => "int8_t".to_string(),
        "char" => "char".to_string(),
        "u16" => "uint16_t".to_string(),
        "i16" => "int16_t".to_string(),
        "u32" | "c_uint" => "uint32_t".to_string(),
        "i32" | "c_int" => "int32_t".to_string(),
        "u64" => "uint64_t".to_string(),
        "i64" => "int64_t".to_string(),
        "f32" => "float".to_string(),
        "f64" => "double".to_string(),
        "usize" => "size_t".to_string(),
        "isize" => "ptrdiff_t".to_string(),
        "c_void" | "()" | "void" => "void".to_string(),

        _ => {
            if ir.find_struct(trimmed).is_some()
                || ir.find_enum(trimmed).is_some()
                || ir.find_type_alias(trimmed).is_some()
                || ir.callback_typedefs.iter().any(|c| c.name == trimmed)
            {
                ocaml_ffi_type_name(trimmed)
            } else {
                "(ptr void)".to_string()
            }
        }
    }
}

/// Pointer-form for VALUE-level emission (inside `Ctypes` schema
/// expressions like `ptr T @-> ... @-> returning Y`). Uses prefix
/// `ptr T` which Ctypes' `ptr` function expects.
pub fn inner_pointer_form(inner: &str, ir: &CodegenIR) -> String {
    if inner.is_empty() || inner == "c_void" || inner == "void" || inner == "()" {
        return "(ptr void)".to_string();
    }
    if ir.find_struct(inner).is_some()
        || ir.find_enum(inner).is_some()
        || ir.find_type_alias(inner).is_some()
        || ir.callback_typedefs.iter().any(|c| c.name == inner)
    {
        format!("(ptr {})", ocaml_ffi_type_name(inner))
    } else {
        "(ptr void)".to_string()
    }
}

/// Pointer-form for TYPE-level emission (val signatures in .mli).
/// OCaml type syntax applies constructors postfix — `T ptr`, not
/// `ptr T`. The latter is rejected with "expects 0 argument(s), but
/// is here applied to 1".
///
/// For struct/union types, the actual runtime type is
/// `<name> Ctypes.structure Ctypes_static.ptr`; the .mli signature
/// must say the same so it matches what `ptr <name_typ>` produces
/// in the impl.
pub fn inner_pointer_form_type(inner: &str, ir: &CodegenIR) -> String {
    if inner.is_empty() || inner == "c_void" || inner == "void" || inner == "()" {
        return "(unit Ctypes_static.ptr)".to_string();
    }
    if let Some(s) = ir.find_struct(inner) {
        if matches!(
            s.category,
            super::ir::TypeCategory::Recursive | super::ir::TypeCategory::DestructorOrClone
        ) {
            return format!("({} Ctypes_static.ptr)", ocaml_ffi_type_name(inner));
        }
        return format!(
            "({} Ctypes.structure Ctypes_static.ptr)",
            ocaml_ffi_type_name(inner)
        );
    }
    if let Some(e) = ir.find_enum(inner) {
        if matches!(
            e.category,
            super::ir::TypeCategory::Recursive
                | super::ir::TypeCategory::VecRef
                | super::ir::TypeCategory::DestructorOrClone
        ) {
            return format!("({} Ctypes_static.ptr)", ocaml_ffi_type_name(inner));
        }
        if e.is_union {
            return format!(
                "({} Ctypes.structure Ctypes_static.ptr)",
                ocaml_ffi_type_name(inner)
            );
        }
        return format!("({} Ctypes_static.ptr)", ocaml_ffi_type_name(inner));
    }
    if alias_is_aggregate(ir, inner) {
        return format!(
            "({} Ctypes.structure Ctypes_static.ptr)",
            ocaml_ffi_type_name(inner)
        );
    }
    if ir.find_type_alias(inner).is_some() || ir.callback_typedefs.iter().any(|c| c.name == inner) {
        format!("({} Ctypes_static.ptr)", ocaml_ffi_type_name(inner))
    } else {
        "(unit Ctypes_static.ptr)".to_string()
    }
}

/// Convert a `snake_case` (or already-Pascal) name to `PascalCase` — the
/// constructor names of the unit-enum ADTs (`refresh_dom` -> `RefreshDom`).
pub fn to_pascal_case(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut upper_next = true;
    for c in s.chars() {
        if c == '_' {
            upper_next = true;
        } else if upper_next {
            out.extend(c.to_uppercase());
            upper_next = false;
        } else {
            out.push(c);
        }
    }
    out
}

pub fn to_snake_case(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 4);
    let mut prev_lower_or_digit = false;
    for c in name.chars() {
        if c.is_ascii_uppercase() {
            if prev_lower_or_digit {
                out.push('_');
            }
            for low in c.to_lowercase() {
                out.push(low);
            }
            prev_lower_or_digit = false;
        } else if c == '_' {
            out.push('_');
            prev_lower_or_digit = false;
        } else {
            out.push(c);
            prev_lower_or_digit = c.is_ascii_lowercase() || c.is_ascii_digit();
        }
    }
    out
}

/// Sanitize an identifier that may collide with an OCaml reserved word.
/// We append a trailing underscore to mangle.
pub fn sanitize_identifier(name: &str) -> String {
    if is_ocaml_reserved(name) {
        format!("{}_", name)
    } else {
        // Also catch names starting with a digit (illegal in OCaml).
        if name
            .chars()
            .next()
            .map(|c| c.is_ascii_digit())
            .unwrap_or(false)
        {
            format!("_{}", name)
        } else {
            name.to_string()
        }
    }
}

fn is_ocaml_reserved(s: &str) -> bool {
    // `end` and `inherit` are keywords that happen to spell api.json method
    // names too; this list is about the parser, not about the API.
    matches!(
        s,
        "and"
            | "as"
            | "assert"
            | "asr"
            | "begin"
            | "class"
            | "constraint"
            | "do"
            | "done"
            | "downto"
            | "else"
            // allow-api-name: OCaml keyword, not api.json's method.
            | "end"
            | "exception"
            | "external"
            | "false"
            | "for"
            | "fun"
            | "function"
            | "functor"
            | "if"
            | "in"
            | "include"
            // allow-api-name: OCaml keyword, not api.json's method.
            | "inherit"
            | "initializer"
            | "land"
            | "lazy"
            | "let"
            | "lor"
            | "lsl"
            | "lsr"
            | "lxor"
            | "match"
            | "method"
            | "mod"
            | "module"
            | "mutable"
            | "new"
            | "nonrec"
            | "object"
            | "of"
            | "open"
            | "or"
            | "private"
            | "rec"
            | "sig"
            | "struct"
            | "then"
            | "to"
            | "true"
            | "try"
            | "type"
            | "val"
            | "virtual"
            | "when"
            | "while"
            | "with"
            // OCaml 5+ added effect handlers; `effect` is now a
            // reserved keyword that produces a syntax error when used
            // as an identifier.
            | "effect"
    )
}

/// Sanitize a doc-comment line so a stray `*)` mid-string doesn't
/// terminate the surrounding OCaml block comment.
pub fn sanitize_doc(s: &str) -> String {
    s.replace('\n', " ").replace("*)", "* )").trim().to_string()
}

#[cfg(test)]
mod split_tests {
    use std::collections::BTreeMap;

    use super::{
        super::{config::CodegenConfig, module_plan::test_fixture_ir},
        *,
    };

    fn generated() -> BTreeMap<String, String> {
        let ir = test_fixture_ir();
        let out = generate(&ir, &CodegenConfig::c_header()).expect("ocaml codegen");
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
    fn types_split_per_module_with_opens() {
        let files = generated();
        let css = &files["azul_types_css.ml"];
        let dom = &files["azul_types_dom.ml"];
        assert!(css.contains("structure \"AzColor0\""), "{}", css);
        assert!(dom.contains("structure \"AzDom\""), "{}", dom);
        assert!(dom.contains("structure \"AzDomVec\""), "DomVec follows Dom");
        assert!(
            dom.contains("open Azul_types_css"),
            "dom embeds Color0:\n{}",
            dom
        );
        assert!(!css.contains("open Azul_types_dom"));
        assert!(
            dom.contains("let az_dom_field_color = field az_dom \"color\" az_color0"),
            "{}",
            dom
        );
    }

    #[test]
    fn facade_includes_every_unit() {
        let files = generated();
        let ir = test_fixture_ir();
        let azul = &files["azul.ml"];
        for unit in unit_names(&ir) {
            assert!(
                files.contains_key(&format!("{}.ml", unit)),
                "unit {} not written",
                unit
            );
            if unit == "azul"
                || unit == "azul_types"
                || unit == "azul_ffi"
                || unit == "azul_enums"
                || unit == "azul_records"
            {
                // facades-of-facades are included as their parts
                continue;
            }
            let needle = format!("include {}", unit_module(&unit));
            assert!(azul.contains(&needle), "azul.ml lacks {}", needle);
        }
        assert_eq!(azul.matches("include Azul_types_dom").count(), 1);
        assert!(
            azul.find("include Azul_types_dom").unwrap()
                < azul.find("include Azul_ffi_dom").unwrap()
        );
        assert!(
            azul.find("include Azul_enums_dom").unwrap()
                < azul.find("include Azul_records_dom").unwrap()
        );
        assert!(
            azul.find("include Azul_records_dom").unwrap()
                < azul.find("include Azul_managed").unwrap()
        );
        assert!(
            azul.find("include Azul_managed").unwrap() < azul.find("include Azul_api_dom").unwrap()
        );
        assert!(files.contains_key("azul.opam") && files.contains_key("hello_world.ml"));
    }

    #[test]
    fn per_class_surface_is_grouped_by_module_and_sealed() {
        let files = generated();
        assert!(files["azul_ffi_widgets.ml"].contains("foreign \"AzButton_dom\""));
        assert!(!files["azul_ffi_dom.ml"].contains("AzButton_"));
        assert!(files["azul_records_dom.ml"].contains("type dom = { mutable raw"));
        assert!(files["azul_api_widgets.mli"].contains("module Button : sig"));
        // Every returned wrapped struct comes back as its record (finaliser
        // armed), own class or not: `Azul_records` (the facade of every
        // record unit) is open in every api unit.
        assert!(
            files["azul_api_widgets.mli"].contains("val dom : t -> dom"),
            "{}",
            files["azul_api_widgets.mli"]
        );
        // Enum modules live in their own layer, below the class modules.
        assert!(files["azul_enums_dom.ml"].contains("module Update = struct"));
        assert!(files["azul_enums_dom.ml"].contains("| RefreshDom"));
        assert!(!files["azul_api_dom.mli"].contains("module Update"));
        assert!(files["azul_api_dom.ml"].contains("open Azul_records"));
        assert!(files["azul_api_dom.ml"].contains("open Azul_enums"));
        for unit in ["azul_ffi_dom.ml", "azul_managed.ml"] {
            assert!(
                files[unit].contains("Azul_loader.ensure ()"),
                "{} must load libazul first",
                unit
            );
        }
    }

    #[test]
    fn smart_constructors_and_tag_helpers_derive_from_the_ir() {
        let files = generated();
        let widgets_mli = &files["azul_api_widgets.mli"];
        let widgets_ml = &files["azul_api_widgets.ml"];
        let dom_mli = &files["azul_api_dom.mli"];
        let dom_ml = &files["azul_api_dom.ml"];
        // The IR constructor stays reachable 1:1; the smart `create` of a
        // class with a `dom()` conversion returns the DOM record.
        assert!(widgets_mli.contains("val create_raw : unit -> t"), "{}", widgets_mli);
        assert!(widgets_mli.contains("val create : unit -> dom"), "{}", widgets_mli);
        assert!(widgets_ml.contains("dom __obj"), "{}", widgets_ml);
        // `create_<tag>()` + `with_child` -> `<tag> ~children`.
        assert!(dom_mli.contains("val body : children:t list -> t"), "{}", dom_mli);
        // An Owned wrapped arg is its record and is consumed after the call.
        assert!(dom_mli.contains("val with_child : t -> dom -> t"), "{}", dom_mli);
        assert!(dom_ml.contains("child.disposed <- true"), "{}", dom_ml);
        assert!(dom_ml.contains("self.disposed <- true"), "{}", dom_ml);
        assert!(!dom_ml.contains("azul_consume"), "generated code consumes typed records");
    }
}
