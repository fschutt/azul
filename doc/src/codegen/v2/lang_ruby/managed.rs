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
//! ## RefAny ownership model
//!
//! `Azul::RefAny.wrap(value)` returns an `Azul::RefAny` *wrapper* whose
//! finalizer owns exactly one refcount (`AzRefAny_delete`). Every C call
//! that takes a `RefAny` by value receives a *clone* (`AzRefAny_clone`,
//! see `wrappers.rs`), so Ruby's reference and libazul's reference are
//! independent: the same wrapped value can be handed to any number of
//! callbacks, and the host-handle releaser fires exactly once, when the
//! last clone anywhere is dropped.
//!
//! ## Why FFI::Function works where Lua needed a libffi pointer-arg type
//!
//! ruby-ffi always allocates libffi closures with all-pointer-args at
//! the C boundary; the same restriction we worked around in LuaJIT FFI
//! exists in ruby-ffi too. The host invoker we register on Ruby's side
//! has *only* pointer args (and an out-pointer for the return value),
//! which is what the static thunk in libazul calls.
//!
//! ## Hand-declared core exports
//!
//! `AzApp_setHostHandleReleaser`, `AzRefAny_newHostHandle`,
//! `AzRefAny_getHostHandle`, `AzApp_set<Kind>Invoker` and
//! `Az<Kind>_createFromHostHandle` are `#[no_mangle]` exports of
//! `core/src/host_invoker.rs` (and its `impl_managed_callback!` macro),
//! not api.json functions, so they are not in `azul.h`. The signatures
//! below mirror that file; only the type prefix and the kind list are
//! IR-derived.

use super::{
    super::{
        config::CodegenConfig,
        generator::CodeBuilder,
        ir::{CallbackTypedefDef, CodegenIR, FunctionArg, TypeCategory},
        managed_host_invoker::{has_return, host_invoker_kinds, wrapper_name},
    },
    functions::ruby_attach_name,
};

/// True when `arg`'s (unprefixed) IR type is the struct the IR classifies
/// as [`TypeCategory::RefAny`]. Used for both function args and callback
/// typedef args (they share [`FunctionArg`]). No name-string matching.
pub(crate) fn is_refany_arg(arg: &FunctionArg, ir: &CodegenIR) -> bool {
    ir.find_struct(arg.type_name.trim())
        .map(|s| s.category == TypeCategory::RefAny)
        .unwrap_or(false)
}

/// The unprefixed name of the IR's `TypeCategory::RefAny` struct
/// (`"RefAny"` in today's api.json — looked up, not assumed).
pub(crate) fn refany_struct_name(ir: &CodegenIR) -> Option<&str> {
    ir.structs
        .iter()
        .find(|s| s.category == TypeCategory::RefAny)
        .map(|s| s.name.as_str())
}

/// Emit Ruby code that registers the host-invoker plumbing and
/// `Azul._register_callback`. Inserted at the bottom of `module Azul`,
/// after the Native sub-module is fully wired up but before user-facing
/// wrapper classes use the helpers.
pub fn emit_managed_module(builder: &mut CodeBuilder, ir: &CodegenIR, config: &CodegenConfig) {
    let Some(refany) = refany_struct_name(ir) else {
        builder.line("# (no TypeCategory::RefAny struct in the IR — host-invoker helpers skipped)");
        return;
    };
    let refany_prefixed = config.apply_prefix(refany);
    // C-export owner of the invoker setters / releaser (core/src/host_invoker.rs).
    let app_prefixed = config.apply_prefix("App");

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

    // Native module: attach_function for the host-invoker C-ABI exports.
    // These are core exports (not api.json), so their Ruby names are
    // derived from the C symbol through the same converter the
    // api.json attaches use.
    let releaser_sym = format!("{}_setHostHandleReleaser", app_prefixed);
    let new_handle_sym = format!("{}_newHostHandle", refany_prefixed);
    let get_handle_sym = format!("{}_getHostHandle", refany_prefixed);
    let releaser_rb = ruby_attach_name(&releaser_sym);
    let new_handle_rb = ruby_attach_name(&new_handle_sym);
    let get_handle_rb = ruby_attach_name(&get_handle_sym);

    builder.line("module Native");
    builder.indent();
    builder.line("# --- Host-invoker C-ABI exports (core/src/host_invoker.rs) ---");
    // `blocking: true` everywhere, same rule as functions.rs (no per-symbol
    // exemptions to reason about).
    builder.line(&format!(
        "attach_function :{}, :{}, [:pointer], :void, blocking: true",
        releaser_rb, releaser_sym
    ));
    builder.line(&format!(
        "attach_function :{}, :{}, [:uint64], {}.by_value, blocking: true",
        new_handle_rb, new_handle_sym, refany_prefixed
    ));
    builder.line(&format!(
        "attach_function :{}, :{}, [:pointer], :uint64, blocking: true",
        get_handle_rb, get_handle_sym
    ));
    for cb in host_invoker_kinds(ir) {
        emit_native_attach_for_kind(builder, cb, &app_prefixed, config);
    }
    builder.dedent();
    builder.line("end # module Native (host-invoker exports)");
    builder.blank();

    // Module-level state: id→callable + pin storage. Only ever touched
    // while holding the GVL: wrapper methods run on Ruby threads and the
    // releaser / invokers are ruby-ffi callbacks, which ruby-ffi
    // re-enters with the GVL held (marshalling foreign-thread calls to a
    // Ruby thread), so a plain Hash needs no lock.
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

    // Mark a wrapper instance as consumed: undefine its finalizer and
    // null `@ptr`. Used by consuming builder methods (`with_*`) and any
    // static factory that takes a wrapper by value (`App.create(data,
    // app_config)` moves app_config into the C call). Without this,
    // the wrapper's `ObjectSpace`-defined finalizer fires later and
    // calls `<Type>_delete` on memory the C side has already moved
    // out — a double free. Calling this on a non-wrapper value (raw
    // FFI::Struct, primitive, nil) is a no-op. The wrapper emitter never
    // emits it for RefAny args (passed as clones) or Ruby Strings
    // (copied by `_az_string`).
    builder.line("def self._consume(val)");
    builder.indent();
    builder.line("return unless val.respond_to?(:ptr) && val.respond_to?(:instance_variable_set)");
    builder.line("begin");
    builder.indent();
    builder.line("ObjectSpace.undefine_finalizer(val)");
    builder.dedent();
    builder.line("rescue StandardError");
    builder.line("end");
    builder.line("val.instance_variable_set(:@ptr, nil)");
    builder.dedent();
    builder.line("end");
    builder.blank();

    // Auto-AzString conversion: codegen emits Azul._az_string(x) for
    // any wrapper-method arg whose IR type is `String` and ref_kind is
    // Owned. Accepts a plain Ruby string and returns an AzString::ByValue
    // FFI struct. Also passes through values that are already AzString
    // structs / raw pointers / wrapper instances, so the helper is
    // idempotent across the wrapper layer's call paths.
    let string_from_utf8 = ir
        .functions
        .iter()
        .find(|f| {
            ir.find_struct(&f.class_name)
                .map(|s| s.category == TypeCategory::String)
                .unwrap_or(false)
                && f.method_name == "from_utf8"
        })
        .map(|f| ruby_attach_name(&f.c_name))
        .unwrap_or_else(|| ruby_attach_name(&format!("{}_fromUtf8", config.apply_prefix("String"))));
    builder.line("# Auto-AzString-conversion helper.");
    builder.line("# Wrapper methods route every Owned `String` arg through this so");
    builder
        .line("# user code can pass plain Ruby strings directly (Dom.create_p_with_text(\"hi\")).");
    builder.line("# The bytes are COPIED into an owned AzString; the Ruby String is untouched.");
    builder.line("def self._az_string(val)");
    builder.indent();
    builder.line("return val if val.is_a?(FFI::Struct) || val.is_a?(FFI::Pointer)");
    builder.line("return val.ptr if val.respond_to?(:ptr)");
    builder.line("bytes = val.to_s.encode(Encoding::UTF_8).bytes");
    builder.line("buf = FFI::MemoryPointer.new(:uint8, bytes.size)");
    builder.line("buf.write_array_of_uint8(bytes) if bytes.size > 0");
    builder.line(&format!("Native.{}(buf, bytes.size)", string_from_utf8));
    builder.dedent();
    builder.line("end");
    builder.blank();

    // CC-4: recursive opts-hash applier. Each wrapper class's
    // `with(opts)` instance method routes through this helper to
    // assign nested fields of the underlying FFI::Struct. Hash
    // values that are themselves Hashes recurse into the nested
    // struct; Ruby Strings auto-convert via `_az_string`. Other
    // values are forwarded by-value.
    //
    // Drops user-visible drilling like
    //   `window.ptr[:window_state][:title] = Azul._az_string('...')`
    // in favor of
    //   `window.with(window_state: { title: 'Hello World' })`.
    builder.line("def self._apply_opts(struct, opts)");
    builder.indent();
    builder.line("opts.each do |key, value|");
    builder.indent();
    builder.line("if value.is_a?(::Hash)");
    builder.indent();
    builder.line("_apply_opts(struct[key], value)");
    builder.dedent();
    // Fully-qualified ::String references Ruby's built-in String —
    // inside `module Azul` the bare `String` would resolve to the
    // codegen-emitted `Azul::String` wrapper class instead and the
    // is_a? check would silently return false for Ruby string
    // literals.
    builder.line("elsif value.is_a?(::String)");
    builder.indent();
    builder.line("struct[key] = _az_string(value)");
    builder.dedent();
    builder.line("elsif value.respond_to?(:ptr) && value.ptr.is_a?(FFI::Struct)");
    builder.indent();
    builder.line("struct[key] = value.ptr");
    builder.dedent();
    builder.line("else");
    builder.indent();
    builder.line("struct[key] = value");
    builder.dedent();
    builder.line("end");
    builder.dedent();
    builder.line("end");
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
    builder.line(&format!("Native.{}(releaser)", releaser_rb));
    builder.blank();

    // Per-kind invoker registration.
    builder.line("# --- Per-kind invoker registrations ---");
    for cb in host_invoker_kinds(ir) {
        emit_invoker_registration(builder, cb, ir, &app_prefixed, refany);
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
    builder.line(
        "raise ArgumentError, \"Azul._register_callback: expected callable, got \
         #{callable.class}\"",
    );
    builder.dedent();
    builder.line("end");
    builder.line("id = _alloc_handle(callable)");
    builder.line("case kind");
    for cb in host_invoker_kinds(ir) {
        let wrapper = wrapper_name(cb);
        builder.line(&format!("when '{}'", wrapper));
        builder.indent();
        builder.line(&format!(
            "Native.{}(id)",
            ruby_attach_name(&create_from_host_handle_sym(wrapper, config))
        ));
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

    // RefAny user-data helpers. The class is reopened (with `initialize`
    // + the finalizer) by the wrapper emitter later in the same file;
    // nothing here runs before the whole file has loaded.
    builder.line("# --- RefAny user-data helpers ---");
    builder.line(&format!("class {}", refany));
    builder.indent();
    builder.line("# Wrap an arbitrary Ruby value in a RefAny handle. Idempotent:");
    builder.line(&format!("#   * an Azul::{} wrapper is returned as-is;", refany));
    builder.line(&format!(
        "#   * a raw Native::{} struct is adopted (the wrapper takes over its refcount);",
        refany_prefixed
    ));
    builder.line("#   * anything else is stored in `@_ruby_handles` and referenced by a fresh");
    builder.line("#     host-handle RefAny. The releaser clears the entry when the LAST clone");
    builder.line("#     (Ruby's or libazul's) is dropped.");
    builder.line("# The wrapper's finalizer owns Ruby's refcount; every C call that takes a");
    builder.line("# RefAny by value receives a clone, so one wrapped value may be handed to");
    builder.line("# any number of callbacks.");
    builder.line("def self.wrap(value)");
    builder.indent();
    builder.line(&format!("return value if value.is_a?(Azul::{})", refany));
    builder.line(&format!(
        "return Azul::{}.new(value) if value.is_a?(Azul::Native::{})",
        refany, refany_prefixed
    ));
    builder.line("id = Azul._alloc_handle(value)");
    builder.line(&format!(
        "Azul::{}.new(Azul::Native.{}(id))",
        refany, new_handle_rb
    ));
    builder.dedent();
    builder.line("end");
    builder.blank();
    builder.line("# Recover the Ruby value previously wrapped via `wrap`. Accepts the");
    builder.line(&format!(
        "# Azul::{} wrapper, a Native::{} struct or a raw pointer (the host",
        refany, refany_prefixed
    ));
    builder.line("# invokers pass `*const RefAny`). Returns nil when the RefAny is not a");
    builder.line("# host handle (created from Rust/C) or the wrapper was consumed.");
    builder.line("def self.unwrap(refany)");
    builder.indent();
    builder.line(&format!("refany = refany.ptr if refany.is_a?(Azul::{})", refany));
    builder.line("return nil if refany.nil?");
    builder.line(&format!("id = Azul::Native.{}(refany)", get_handle_rb));
    builder.line("return nil if id == 0");
    builder.line("Azul.instance_variable_get(:@_ruby_handles)[id]");
    builder.dedent();
    builder.line("end");
    builder.dedent();
    builder.line(&format!("end # class {} (user-data helpers)", refany));
    builder.blank();
}

/// `Az<Kind>_createFromHostHandle` — the per-kind constructor exported
/// by `impl_managed_callback!`.
fn create_from_host_handle_sym(wrapper: &str, config: &CodegenConfig) -> String {
    format!("{}_createFromHostHandle", config.apply_prefix(wrapper))
}

/// `AzApp_set<Kind>Invoker` — the per-kind invoker setter exported by
/// `impl_managed_callback!`.
fn set_invoker_sym(wrapper: &str, app_prefixed: &str) -> String {
    format!("{}_set{}Invoker", app_prefixed, wrapper)
}

fn emit_native_attach_for_kind(
    builder: &mut CodeBuilder,
    cb: &CallbackTypedefDef,
    app_prefixed: &str,
    config: &CodegenConfig,
) {
    let wrapper = wrapper_name(cb);
    let setter = set_invoker_sym(wrapper, app_prefixed);
    let ctor = create_from_host_handle_sym(wrapper, config);
    builder.line(&format!("attach_function :{},", ruby_attach_name(&setter)));
    builder.indent();
    builder.line(&format!(":{}, [:pointer], :void, blocking: true", setter));
    builder.dedent();
    builder.line(&format!("attach_function :{},", ruby_attach_name(&ctor)));
    builder.indent();
    builder.line(&format!(
        ":{}, [:uint64], {}.by_value, blocking: true",
        ctor,
        config.apply_prefix(wrapper)
    ));
    builder.dedent();
}

fn emit_invoker_registration(
    builder: &mut CodeBuilder,
    cb: &CallbackTypedefDef,
    ir: &CodegenIR,
    app_prefixed: &str,
    refany: &str,
) {
    let wrapper = wrapper_name(cb);
    // Ruby local-variable name for the pinned FFI::Function.
    let invoker_var = format!("{}_invoker", ruby_attach_name(wrapper));
    let setter_rb = ruby_attach_name(&set_invoker_sym(wrapper, app_prefixed));

    // ruby-ffi FFI::Function: takes ret_type, [arg_types...], block.
    // The arg list mirrors the core macro's `$invoker_ty`: handle id
    // (u64) + one pointer per argument + ALWAYS one trailing out-pointer
    // (`out: *mut $ret`, passed even when `$ret = ()` so the macro stays
    // homogeneous — core/src/host_invoker.rs). Declaring it for void
    // kinds keeps the closure's arity identical to what the thunk calls.
    let mut arg_types: Vec<&str> = vec![":uint64"];
    for _ in &cb.args {
        arg_types.push(":pointer");
    }
    arg_types.push(":pointer");
    let cb_has_return = has_return(cb);

    builder.line(&format!("# {} invoker", wrapper));
    builder.line(&format!(
        "{} = FFI::Function.new(:void, [{}]) do |*args|",
        invoker_var,
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
    if cb_has_return {
        builder.line("out_ptr  = args.last");
    }
    builder.line("begin");
    builder.indent();
    builder.line("unwrapped_args = []");
    for (i, arg) in cb.args.iter().enumerate() {
        if is_refany_arg(arg, ir) {
            builder.line(&format!(
                "unwrapped_args << Azul::{}.unwrap(ptr_args[{}])",
                refany, i
            ));
        } else {
            builder.line(&format!("unwrapped_args << ptr_args[{}]", i));
        }
    }
    if cb_has_return {
        builder.line("ret = fn.call(*unwrapped_args)");
        builder.line("# Numeric returns (Update enum) → write32. Wrapper class");
        builder.line("# instances (e.g. `Dom` from a layout cb) → unwrap to the");
        builder.line("# underlying FFI::Struct, memcopy through out_ptr, then");
        builder.line("# mark the wrapper consumed (libazul now owns its memory).");
        builder.line("# Raw FFI::Struct returns → memcopy directly.");
        builder.line("if ret.is_a?(Integer)");
        builder.indent();
        builder.line("out_ptr.write_int32(ret)");
        builder.dedent();
        builder.line("elsif ret.respond_to?(:ptr) && ret.ptr.respond_to?(:to_ptr)");
        builder.indent();
        builder.line("_raw = ret.ptr");
        builder.line("size = _raw.class.respond_to?(:size) ? _raw.class.size : _raw.size");
        builder.line("out_ptr.write_bytes(_raw.to_ptr.read_bytes(size))");
        builder.line("Azul._consume(ret)");
        builder.dedent();
        builder.line("elsif ret.respond_to?(:to_ptr)");
        builder.indent();
        builder.line("size = ret.class.respond_to?(:size) ? ret.class.size : ret.size");
        builder.line("out_ptr.write_bytes(ret.to_ptr.read_bytes(size))");
        builder.dedent();
        builder.line("end");
    } else {
        builder.line("fn.call(*unwrapped_args)");
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
    builder.line(&format!("@_live_pins << {}", invoker_var));
    builder.line(&format!("Native.{}({})", setter_rb, invoker_var));
    builder.blank();
}
