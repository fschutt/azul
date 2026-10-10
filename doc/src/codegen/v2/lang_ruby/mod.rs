//! Ruby language binding generator (v2)
//!
//! Generates a single `azul.rb` source file using the standard `ffi` gem.
//! No native extension (mkmf) is required — Ruby loads the prebuilt
//! `azul.dll` / `libazul.so` / `libazul.dylib` at runtime via `ffi_lib`.
//!
//! # Output structure
//!
//! ```ruby
//! module Azul
//!   module Native
//!     extend FFI::Library
//!     ffi_lib ['azul', 'libazul.so', 'libazul.dylib', 'azul.dll']
//!
//!     # FFI::Struct subclasses for every C-API type (AzFoo)
//!     class AzApp < FFI::Struct; layout :ptr, :pointer; end
//!     # FFI::Union subclasses for tagged unions
//!     # callback :foo_callback, [...], :ret declarations
//!     # attach_function :az_app_create, [...], :pointer
//!   end
//!
//!   # Idiomatic wrappers (drop Az prefix)
//!   class App
//!     def initialize(ptr); @ptr = ptr; ObjectSpace.define_finalizer(self, self.class.finalize(@ptr)); end
//!     def self.finalize(ptr); proc { Native.az_app_delete(ptr) }; end
//!     def self.create(...); ... ; end
//!   end
//! end
//! ```
//!
//! # Notes
//!
//! - The Ruby generator emits a free function `generate(ir, config)` instead of implementing the
//!   `LanguageGenerator` trait. The trait is shaped for Rust/C/C++/Python output formats; Ruby
//!   (like Lua, C#, etc.) doesn't fit that interface cleanly.
//! - A generic template (`CssPropertyValue<T>`) gets a note naming the concrete instantiations
//!   emitted in its place — it has no C ABI of its own, so nothing is lost. A type that really is
//!   skipped keeps a `# SKIPPED:` comment.
//! - Every API class the C ABI exports functions for gets an idiomatic class: structs (with or
//!   without a `_delete`), tagged-union enums with their variant constructors, and the
//!   monomorphized generic aliases. Unit enums stay integer constants.

use anyhow::Result;

use super::{config::CodegenConfig, generator::CodeBuilder, ir::CodegenIR};

pub mod functions;
pub mod gemspec;
pub mod managed;
pub mod types;
pub mod wrappers;

/// Entry point: generate the full `azul.rb` source as a String.
pub fn generate(ir: &CodegenIR, config: &CodegenConfig) -> Result<String> {
    let mut builder = CodeBuilder::new("  ");

    // File header
    emit_header(&mut builder);

    // Top-level module + native FFI submodule
    builder.line("module Azul");
    builder.indent();

    // Native submodule: raw FFI bindings (FFI::Struct/Union, attach_function)
    builder.line("# ============================================================");
    builder.line("# Native FFI bindings (raw C-API surface; AZ-prefixed types).");
    builder.line("# Use the idiomatic wrappers below in user code instead.");
    builder.line("# ============================================================");
    builder.line("module Native");
    builder.indent();
    builder.line("extend FFI::Library");
    // ffi_lib accepts both bare names (resolved through the dynamic
    // loader's search path) and absolute paths. macOS's hardened runtime
    // refuses to load by bare name when launched from a non-system
    // path and the lib isn't on the default search path, so we also
    // try absolute paths next to this script and against
    // AZ_LIB_DIR (override for explicit placement). The flat list is
    // tried in order; first hit wins, missing entries are ignored.
    //
    // Search order = the two layouts that exist: (1) the guide's
    // "drop libazul next to azul.rb" and (2) the gem, which
    // `gemspec.rs` / CI pack FLAT as `lib/azul.rb` + `lib/libazul.*`
    // (so "next to azul.rb" covers it too). Keep `gemspec::generate_gemspec`
    // in sync if this ever changes.
    builder.line("_azul_lib_candidates = ['azul', 'libazul.so', 'libazul.dylib', 'azul.dll']");
    builder.line("_here = File.expand_path(File.dirname(__FILE__))");
    builder.line("[ENV['AZ_LIB_DIR'], _here].compact.each do |dir|");
    builder.indent();
    builder.line("%w[libazul.dylib libazul.so azul.dll].each do |name|");
    builder.indent();
    builder.line("p = File.join(dir, name)");
    builder.line("_azul_lib_candidates.unshift(p) if File.exist?(p)");
    builder.dedent();
    builder.line("end");
    builder.dedent();
    builder.line("end");
    builder.line("ffi_lib _azul_lib_candidates");
    builder.blank();

    // Forward declarations for FFI::Struct / FFI::Union classes (so layouts can
    // reference each other in any order via Foo.by_value / Foo.by_ref).
    types::emit_forward_declarations(&mut builder, ir, config);

    // Simple (unit) enums first as Ruby modules holding integer constants.
    types::emit_simple_enums(&mut builder, ir, config);

    // Callback typedefs as `callback :name, [args], :ret`
    types::emit_callback_typedefs(&mut builder, ir, config);

    // Topologically interleave tagged unions and struct layouts by
    // their `sort_order`. Either kind can reference the other via
    // `Foo.by_value`, and calling `.by_value` before the target type's
    // layout is set raises `wrong type in @layout ivar`. The IR builder
    // computes a unified sort order over both struct + enum that
    // satisfies all dependency edges; emit in that order.
    types::emit_typedefs_in_sort_order(&mut builder, ir, config);

    // attach_function for every C-ABI symbol.
    functions::emit_attach_functions(&mut builder, ir, config);

    builder.dedent();
    builder.line("end # module Native");
    builder.blank();

    // User-facing aliases for unit enums. The codegen emits
    // `Azul::Native::Az<Foo>::Variant` integer constants inside the
    // Native module; surface them at top-level `Azul::<Foo>::Variant`
    // so hello-worlds can write `Update.RefreshDom` rather than
    // `Native::AzUpdate::RefreshDom`.
    types::emit_user_facing_enum_aliases(&mut builder, ir, config);

    // Managed-FFI prelude: registers host-invoker closures + RefAny
    // helpers under `module Azul`. Must come before user-facing wrapper
    // classes because they reference `Azul._register_callback`.
    managed::emit_managed_module(&mut builder, ir, config);

    // Idiomatic wrappers (Azul::App, Azul::Dom, etc.)
    wrappers::emit_wrappers(&mut builder, ir, config);

    builder.dedent();
    builder.line("end # module Azul");

    Ok(builder.finish())
}

fn emit_header(builder: &mut CodeBuilder) {
    builder.line("# frozen_string_literal: true");
    builder.line("# WARNING: autogenerated Ruby bindings for the Azul GUI toolkit.");
    builder.line("# Generated by azul-doc codegen v2 — DO NOT EDIT MANUALLY.");
    builder.line("#");
    builder.line("# Loads the prebuilt native library (azul.dll / libazul.so / libazul.dylib)");
    builder.line("# via the standard `ffi` gem; no native extension build is required.");
    builder.blank();
    builder.line("require 'ffi'");
    builder.blank();
}

#[cfg(test)]
mod field_accessor_tests {
    use super::super::config::CodegenConfig;
    use super::*;

    /// `azul.rb` for the real api.json, generated once.
    fn azul_rb() -> &'static str {
        static OUT: std::sync::OnceLock<String> = std::sync::OnceLock::new();
        OUT.get_or_init(|| {
            generate(super::super::bug_classes::ir(), &CodegenConfig::c_header())
                .expect("ruby codegen")
        })
    }

    /// The body of the idiomatic `class <name>` (up to its `end # class`).
    fn class_body(name: &str) -> &'static str {
        let out = azul_rb();
        let head = format!("\n  class {}\n", name);
        let start = out
            .find(&head)
            .unwrap_or_else(|| panic!("class {} is not generated", name));
        let end_marker = format!("end # class {}", name);
        let end = out[start..].find(&end_marker).expect("class end") + start;
        &out[start..end]
    }

    /// The `def <name>` method of `body`, up to the next blank line.
    fn method<'a>(body: &'a str, def: &str) -> &'a str {
        let head = format!("def {}\n", def);
        let alt = format!("def {}(", def);
        let i = body
            .find(&head)
            .or_else(|| body.find(&alt))
            .unwrap_or_else(|| panic!("`def {}` is missing in:\n{}", def, body));
        let rest = &body[i..];
        &rest[..rest.find("\n\n").unwrap_or(rest.len())]
    }

    /// The module-level `def self.<name>` helper in `module Azul`.
    fn helper(name: &str) -> &'static str {
        let out = azul_rb();
        let head = format!("def self.{}(", name);
        let i = out
            .find(&head)
            .unwrap_or_else(|| panic!("helper {} is missing", name));
        let rest = &out[i..];
        &rest[..rest.find("\n\n").unwrap_or(rest.len())]
    }

    #[test]
    fn a_window_title_reads_as_a_ruby_string_and_its_setter_releases_the_old_title() {
        let body = class_body("FullWindowState");
        let get = method(body, "title");
        assert!(get.contains("Azul._read_string("), "decoded, never consumed:\n{}", get);
        let set = method(body, "title=");
        assert!(set.contains("Native.az_string_delete("), "the old title is released:\n{}", set);
        assert!(set.contains("Azul._az_string(value)"), "a Ruby String is copied in:\n{}", set);
        assert!(set.contains("Azul._own(value)"), "an Azul::String is moved in:\n{}", set);
        let rd = helper("_read_string");
        assert!(!rd.contains("_delete"), "reading must not free the field:\n{}", rd);
    }

    #[test]
    fn the_window_state_of_create_options_is_a_live_view_and_its_setter_moves_the_value_in() {
        let body = class_body("WindowCreateOptions");
        let get = method(body, "window_state");
        assert!(
            get.contains("Azul._view(FullWindowState, self, :window_state)"),
            "nested writes must reach the options:\n{}",
            get
        );
        let set = method(body, "window_state=");
        assert!(set.contains("Azul._own(value)"), "{}", set);
        assert!(set.contains("Native.az_full_window_state_delete("), "{}", set);
        assert!(set.contains("Azul._consume(value)"), "{}", set);
        // The size path: FullWindowState.size -> WindowSize.dimensions.
        let size = method(class_body("FullWindowState"), "size");
        assert!(size.contains("Azul._view(WindowSize, self, :size)"), "{}", size);
    }

    #[test]
    fn a_bool_field_of_a_wrapped_struct_reads_and_writes_in_place() {
        let body = class_body("CheckBoxState");
        assert!(method(body, "checked").contains("[:checked]"), "{}", body);
        let set = method(body, "checked=");
        assert!(set.contains("[:checked] = value"), "{}", set);
        assert!(!set.contains("_delete"), "a bool owns nothing:\n{}", set);
    }

    #[test]
    fn a_field_whose_getter_name_is_an_api_method_is_still_settable() {
        let body = class_body("TextInputState");
        assert!(body.contains("def get_text"), "{}", body);
        let set = method(body, "text=");
        assert!(set.contains("Native.az_u32_vec_delete("), "{}", set);
    }

    #[test]
    fn a_borrowed_view_has_no_finalizer_and_is_never_consumed() {
        let view = helper("_view");
        assert!(view.contains(".allocate"), "a view skips initialize (no finalizer):\n{}", view);
        assert!(!view.contains("define_finalizer"), "{}", view);
        let consume = helper("_consume");
        assert!(consume.contains("@az_owner"), "a view is not consumed:\n{}", consume);
        let own = helper("_own");
        assert!(own.contains("@az_owner"), "a view is deep-copied before a move:\n{}", own);
    }

    #[test]
    fn apply_opts_releases_the_old_value_and_consumes_the_wrapper_it_moves_in() {
        let apply = helper("_apply_opts");
        assert!(apply.contains("_own(value)"), "{}", apply);
        assert!(apply.contains("_consume(value)"), "{}", apply);
        assert!(apply.contains("FIELD_DELETE"), "the old value is released:\n{}", apply);
        assert!(azul_rb().contains("'AzString' => :az_string_delete,"));
    }

    #[test]
    fn a_wrapper_moved_into_a_call_goes_through_own() {
        let run = method(class_body("App"), "run");
        assert!(run.contains("Native.az_app_run(@ptr, Azul._own(root_window))"), "{}", run);
    }
}
