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
//! - The Ruby generator emits a free function `generate(ir, config)` instead of
//!   implementing the `LanguageGenerator` trait. The trait is shaped for
//!   Rust/C/C++/Python output formats; Ruby (like Lua, C#, etc.) doesn't fit
//!   that interface cleanly.
//! - Skipped types (Recursive, VecRef, GenericTemplate, DestructorOrClone,
//!   CallbackTypedef) get `# SKIPPED:` comments rather than `# TODO`.

use anyhow::Result;

use super::config::CodegenConfig;
use super::generator::CodeBuilder;
use super::ir::CodegenIR;

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
    // AZUL_LIB_DIR (override for explicit placement). The flat list is
    // tried in order; first hit wins, missing entries are ignored.
    builder.line("_azul_lib_candidates = ['azul', 'libazul.so', 'libazul.dylib', 'azul.dll']");
    builder.line("_here = File.expand_path(File.dirname(__FILE__))");
    builder.line("[ENV['AZUL_LIB_DIR'], _here].compact.each do |dir|");
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

    // Managed-FFI prelude: registers host-invoker closures + RefAny
    // helpers under `module Azul`. Must come before user-facing wrapper
    // classes because they reference `Azul._register_callback`.
    managed::emit_managed_module(&mut builder, ir);

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
