//! Ruby-side runtime helpers emitted into `azul.rb`.
//!
//! Mirrors `lang_lua::managed`: at module load we register one
//! `FFI::Function` closure per supported callback kind plus a single
//! shared releaser. Per-callback registration goes through a Ruby
//! id→callable hash (`@_ruby_handles`); the framework's RefAny
//! destructor calls back through `AzApp_setHostHandleReleaser` to drop
//! the entry when the last clone goes away.
//!
//! ## What `_register_callback` produces
//!
//! `Azul._register_callback(kind, callable)` builds the matching
//! `Az<Kind>` wrapper struct via libazul's `_createFromHostHandle`
//! constructor and returns it as a Ruby `FFI::Struct`. The wrapper
//! emitter passes that struct directly to whatever `attach_function`
//! takes a `Callback` / `LayoutCallback` / `VirtualViewCallback` arg.
//!
//! ## Why FFI::Function works where Lua needed a libffi pointer-arg type
//!
//! ruby-ffi always allocates libffi closures with all-pointer-args at
//! the C boundary; the same restriction we worked around in LuaJIT FFI
//! exists in ruby-ffi too. The host invoker we register on Ruby's side
//! has *only* pointer args (and an out-pointer for the return value),
//! which is what the static thunk in libazul calls.

use super::super::ir::{CallbackTypedefDef, CodegenIR};

/// Same contract as `lang_lua::managed::HOST_INVOKER_KINDS`.
const HOST_INVOKER_KINDS: &[&str] = &[
    "Callback",
    "LayoutCallback",
    "VirtualViewCallback",
];

/// Emit Ruby code that registers the host-invoker plumbing and
/// `Azul._register_callback`. Inserted at the bottom of `module Azul`,
/// after the Native sub-module is fully wired up but before user-facing
/// wrapper classes use the helpers.
pub fn emit_managed_module(builder: &mut super::super::generator::CodeBuilder, ir: &CodegenIR) {
    builder.line("# ============================================================");
    builder.line("# Managed-FFI runtime helpers (host-invoker pattern)");
    builder.line("# ============================================================");
    builder.line("# libazul exports per supported callback kind:");
    builder.line("#   * a static thunk (the `cb` field of the callback wrapper),");
    builder.line("#   * Az<Kind>_createFromHostHandle(u64) -> Az<Kind> constructor,");
    builder.line("#   * AzApp_set<Kind>Invoker(fn) setter.");
    builder.line("#");
    builder.line("# We register one FFI::Function per kind at module load (these have");
    builder.line("# *pointer-arg* signatures which ruby-ffi handles fine — by-value");
    builder.line("# plumbing happens inside libazul's static thunk).");
    builder.blank();

    // Native module: attach_function for the new C-ABI exports.
    builder.line("module Native");
    builder.indent();
    builder.line("# --- Host-invoker C-ABI exports ---");
    builder.line("attach_function :az_app_set_host_handle_releaser,");
    builder.indent();
    builder.line(":AzApp_setHostHandleReleaser, [:pointer], :void");
    builder.dedent();
    builder.line("attach_function :az_ref_any_new_host_handle,");
    builder.indent();
    builder.line(":AzRefAny_newHostHandle, [:uint64], AzRefAny.by_value");
    builder.dedent();
    builder.line("attach_function :az_ref_any_get_host_handle,");
    builder.indent();
    builder.line(":AzRefAny_getHostHandle, [:pointer], :uint64");
    builder.dedent();
    for cb in managed_callbacks(ir) {
        emit_native_attach_for_kind(builder, cb);
    }
    builder.dedent();
    builder.line("end # module Native (host-invoker exports)");
    builder.blank();

    // Module-level state: id→callable + pin storage.
    builder.line("@_ruby_handles    = {}");
    builder.line("@_next_handle_id  = 0");
    builder.line("@_live_pins       = []");
    builder.blank();

    builder.line("def self._alloc_handle(callable)");
    builder.indent();
    builder.line("@_next_handle_id += 1");
    builder.line("id = @_next_handle_id");
    builder.line("@_ruby_handles[id] = callable");
    builder.line("id");
    builder.dedent();
    builder.line("end");
    builder.blank();

    // Releaser: clears the hash entry. Pinned for process lifetime.
    builder.line("releaser = FFI::Function.new(:void, [:uint64]) do |id|");
    builder.indent();
    builder.line("@_ruby_handles.delete(id)");
    builder.dedent();
    builder.line("end");
    builder.line("@_live_pins << releaser");
    builder.line("Native.az_app_set_host_handle_releaser(releaser)");
    builder.blank();

    // Per-kind invoker registration.
    builder.line("# --- Per-kind invoker registrations ---");
    for cb in managed_callbacks(ir) {
        emit_invoker_registration(builder, cb);
    }
    builder.blank();

    // _register_callback dispatch table.
    builder.line("# Wrapper-emitted methods call this to wrap a Ruby callable into");
    builder.line("# the matching callback wrapper struct. Returns an FFI::Struct.");
    builder.line("def self._register_callback(kind, callable)");
    builder.indent();
    builder.line("return nil if callable.nil?");
    builder.line("unless callable.respond_to?(:call)");
    builder.indent();
    builder.line("raise ArgumentError, \"Azul._register_callback: expected callable, got #{callable.class}\"");
    builder.dedent();
    builder.line("end");
    builder.line("id = _alloc_handle(callable)");
    builder.line("case kind");
    for cb in managed_callbacks(ir) {
        let wrapper = wrapper_name(cb);
        builder.line(&format!("when '{}'", wrapper));
        builder.indent();
        builder.line(&format!("Native.az_{}_create_from_host_handle(id)", to_snake(wrapper)));
        builder.dedent();
    }
    builder.line("else");
    builder.indent();
    builder.line("raise ArgumentError, \"Azul._register_callback: unknown kind #{kind.inspect}\"");
    builder.dedent();
    builder.line("end");
    builder.dedent();
    builder.line("end");
    builder.blank();

    // RefAny user-data helpers.
    builder.line("# --- RefAny user-data helpers ---");
    builder.line("class RefAny");
    builder.indent();
    builder.line("# Wrap an arbitrary Ruby value in an AzRefAny. The value is held");
    builder.line("# alive by `@_ruby_handles`; the destructor clears it on drop.");
    builder.line("def self.wrap(value)");
    builder.indent();
    builder.line("id = Azul._alloc_handle(value)");
    builder.line("Azul::Native.az_ref_any_new_host_handle(id)");
    builder.dedent();
    builder.line("end");
    builder.blank();
    builder.line("# Recover the Ruby value previously wrapped via `wrap`.");
    builder.line("def self.unwrap(refany)");
    builder.indent();
    builder.line("id = Azul::Native.az_ref_any_get_host_handle(refany)");
    builder.line("return nil if id == 0");
    builder.line("Azul.instance_variable_get(:@_ruby_handles)[id]");
    builder.dedent();
    builder.line("end");
    builder.dedent();
    builder.line("end # class RefAny (user-data helpers)");
    builder.blank();
}

fn managed_callbacks(ir: &CodegenIR) -> impl Iterator<Item = &CallbackTypedefDef> {
    ir.callback_typedefs.iter().filter(|cb| {
        let wrapper = wrapper_name(cb);
        HOST_INVOKER_KINDS.contains(&wrapper)
    })
}

fn wrapper_name(cb: &CallbackTypedefDef) -> &str {
    cb.name.strip_suffix("Type").unwrap_or(cb.name.as_str())
}

fn emit_native_attach_for_kind(
    builder: &mut super::super::generator::CodeBuilder,
    cb: &CallbackTypedefDef,
) {
    let wrapper = wrapper_name(cb);
    let snake = to_snake(wrapper);
    builder.line(&format!("attach_function :az_app_set_{}_invoker,", snake));
    builder.indent();
    builder.line(&format!(":AzApp_set{}Invoker, [:pointer], :void", wrapper));
    builder.dedent();
    builder.line(&format!("attach_function :az_{}_create_from_host_handle,", snake));
    builder.indent();
    builder.line(&format!(
        ":Az{}_createFromHostHandle, [:uint64], Az{}.by_value",
        wrapper, wrapper
    ));
    builder.dedent();
}

fn emit_invoker_registration(
    builder: &mut super::super::generator::CodeBuilder,
    cb: &CallbackTypedefDef,
) {
    let wrapper = wrapper_name(cb);
    let snake = to_snake(wrapper);

    // ruby-ffi FFI::Function: takes ret_type, [arg_types...], block.
    // The arg list mirrors the macro: handle id (u64) + one pointer per
    // argument + one out-pointer for the return.
    let mut arg_types: Vec<&str> = vec![":uint64"];
    for _ in &cb.args {
        arg_types.push(":pointer");
    }
    let has_return = cb.return_type.as_deref().map(|s| s != "void").unwrap_or(false);
    if has_return {
        arg_types.push(":pointer");
    }

    builder.line(&format!("# {} invoker", wrapper));
    builder.line(&format!(
        "{}_invoker = FFI::Function.new(:void, [{}]) do |*args|",
        snake,
        arg_types.join(", ")
    ));
    builder.indent();
    builder.line("id = args[0]");
    builder.line("fn = @_ruby_handles[id]");
    builder.line("next if fn.nil?");
    let user_arg_count = cb.args.len();
    builder.line(&format!(
        "ptr_args = args[1, {}] # pointer args, by reference",
        user_arg_count
    ));
    if has_return {
        builder.line("out_ptr  = args.last");
    }
    builder.line("begin");
    builder.indent();
    builder.line("ret = fn.call(*ptr_args)");
    if has_return {
        builder.line("# Best-effort write through the out-pointer. Numeric returns");
        builder.line("# (Update enums) write directly; FFI::Struct returns are");
        builder.line("# memcopied. Anything else is silently dropped.");
        builder.line("if ret.is_a?(Integer)");
        builder.indent();
        builder.line("out_ptr.write_int32(ret)");
        builder.dedent();
        builder.line("elsif ret.respond_to?(:to_ptr)");
        builder.indent();
        builder.line("size = ret.class.respond_to?(:size) ? ret.class.size : ret.size");
        builder.line("out_ptr.write_bytes(ret.to_ptr.read_bytes(size))");
        builder.dedent();
        builder.line("end");
    }
    builder.dedent();
    builder.line("rescue => e");
    builder.indent();
    builder.line(&format!(
        "$stderr.puts \"[azul] {} error: #{{e.message}}\"",
        wrapper
    ));
    builder.dedent();
    builder.line("end");
    builder.dedent();
    builder.line("end");
    builder.line(&format!("@_live_pins << {}_invoker", snake));
    builder.line(&format!(
        "Native.az_app_set_{}_invoker({}_invoker)",
        snake, snake
    ));
    builder.blank();
}

/// CamelCase → snake_case (e.g. "VirtualViewCallback" → "virtual_view_callback").
fn to_snake(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 4);
    for (i, c) in name.chars().enumerate() {
        if c.is_ascii_uppercase() {
            if i != 0 {
                out.push('_');
            }
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}
