//! Idiomatic Ruby class wrappers under `module Azul`.
//!
//! Every API class the C ABI exports functions for — a struct, a tagged-union
//! enum, or a monomorphized generic alias (`CaretColorValue =
//! CssPropertyValue<CaretColor>`, a real `union AzCaretColorValue` in
//! `azul.h`) — gets a Ruby class that:
//!
//! 1. Holds `@ptr` (the underlying FFI struct value);
//! 2. When the class owns heap memory (it has a `<TypeName>_delete` export),
//!    registers an `ObjectSpace.define_finalizer` that calls
//!    `Native.az_<typename>_delete(ptr)` when the Ruby object is GC'd. The
//!    finalizer proc captures only `ptr` — never `self` — to avoid the
//!    well-known "finalizer keeps the instance alive forever" trap. A class
//!    with no `_delete` export owns nothing and gets no finalizer;
//! 3. Exposes idiomatic class methods (constructors, static helpers, enum
//!    variant constructors) and instance methods (anything else that takes
//!    `&self` / `&mut self`);
//! 4. Routes Ruby's value protocol through the C derives that implement it:
//!    `_toDbgString` → `to_s`/`inspect`, `_partialEq` → `==`/`eql?`, `_hash` →
//!    `hash`, `_cmp`/`_partialCmp` → `<=>` (+ `Comparable`), `_deepCopy` →
//!    `clone`/`dup`, `_createDefault` → `default`. None of that logic is
//!    reimplemented in Ruby — every one of them is one call into libazul.
//!
//! Method naming: drop the `Az` prefix and the `<TypeName>_` segment, then
//! convert `camelCase` to `snake_case`. So:
//!
//! - `AzApp_create`        → `App.create`        (static)
//! - `AzApp_run`           → `app.run`           (instance)
//! - `AzAppConfig_default` → `AppConfig.default` (static)
//! - `AzDom_addChild`      → `dom.add_child`     (instance)
//! - `AzOptionDom_some`    → `OptionDom.some`    (variant constructor)
//!
//! Unit enums are the one kind of API class with no wrapper here: they are
//! plain integer constants, surfaced as `Azul::<Enum>::<Variant>` by
//! `types::emit_user_facing_enum_aliases`, and a Ruby `Integer` already has
//! the language's own equality, ordering and hashing.

use std::collections::BTreeSet;

use super::{
    super::{
        config::CodegenConfig,
        generator::CodeBuilder,
        ir::{
            ArgRefKind, CodegenIR, ConstantDef, EnumDef, FunctionArg, FunctionDef, FunctionKind,
            StructDef, TypeAliasDef, TypeCategory,
        },
    },
    functions::ruby_attach_name,
    managed::is_refany_arg,
    types::{ruby_const_name, should_emit_enum, should_emit_struct, snake_case},
};

/// The IR function of `kind` declared on `class` (the `_delete` /
/// `_clone` / `_partialEq` / `_hash` / `_toDbgString` export), if any.
/// Every `Native.az_*` call the wrappers emit is spelled from the
/// returned `c_name` via [`ruby_attach_name`] — the same converter
/// `functions.rs` used to declare it — so a wrapper can never call an
/// attach that does not exist (the old `snake_case(class)` spelling
/// broke on `GLintVec` / `TessellatedGPUSvgNode` / `XWindowTypeVec`).
fn class_fn<'a>(ir: &'a CodegenIR, class: &str, kind: FunctionKind) -> Option<&'a FunctionDef> {
    ir.functions
        .iter()
        .find(|f| f.class_name == class && f.kind == kind)
}

/// Ruby attach name of the `kind` export on `class`, if the IR has one.
fn class_fn_rb(ir: &CodegenIR, class: &str, kind: FunctionKind) -> Option<String> {
    class_fn(ir, class, kind).map(|f| ruby_attach_name(&f.c_name))
}

// ============================================================================
// Public entry point
// ============================================================================

/// Emit an idiomatic Ruby class for every API class the C ABI exports
/// functions or constants for.
///
/// Three kinds of IR entry produce one:
///
/// * [`StructDef`] — the classic case. A struct with a `_delete` owns heap
///   memory and gets a finalizer; one without is a POD value and gets a
///   class anyway, because its constructors and methods are API too and a
///   comment pointing at `Native::AzFoo` reaches none of them.
/// * [`EnumDef`] with `is_union` — a real `union AzFoo` in `azul.h`. Its
///   variant constructors (`AzEventFilter_hover`) are the single largest
///   block of C exports, and without a class there is nowhere to hang them.
///   Unit enums are skipped: `emit_user_facing_enum_aliases` already surfaces
///   them as integer constants and their derives are the Integer's own.
/// * [`TypeAliasDef`] with a `monomorphized_def` — the 119 `<Prop>Value`
///   instantiations of `CssPropertyValue<T>` and friends. `find_struct` /
///   `find_enum` return `None` for these (they are aliases, not definitions),
///   which is exactly why they used to fall through every emitter, yet the
///   header declares `AzCaretColorValue_cmp` like any other type.
///
/// `DestructorOrClone` types are the one deliberate omission: they are the
/// `VecDestructor` tags libazul stores inside a Vec to remember how to free
/// it, never something Ruby code constructs or compares.
pub fn emit_wrappers(builder: &mut CodeBuilder, ir: &CodegenIR, config: &CodegenConfig) {
    builder.line("# ============================================================");
    builder.line("# Idiomatic wrappers (Az prefix dropped). Use these in user code.");
    builder.line("# ============================================================");

    let delete_set = collect_delete_targets(ir);
    // Constant owners that did get a class: the leftover pass below only has
    // to invent a module for the ones that did not.
    let mut constant_owners_emitted: BTreeSet<String> = BTreeSet::new();

    for s in &ir.structs {
        if !should_emit_struct(s, config) {
            continue;
        }
        let target = WrapperTarget::Struct(s);
        if !delete_set.contains(s.name.as_str()) && !has_idiomatic_surface(&target, ir) {
            // POD struct with no exports of its own: a class here would be an
            // empty shell. Point at the FFI::Struct and move on.
            builder.line(&format!(
                "# (no wrapper for {} — no _delete and no methods; use Native::{} directly)",
                s.name,
                config.apply_prefix(&s.name)
            ));
            continue;
        }
        emit_class_wrapper(builder, target, ir, config, &mut constant_owners_emitted);
        builder.blank();
    }

    for e in &ir.enums {
        if !should_emit_enum(e, config) {
            continue;
        }
        // A unit enum is an integer at the C ABI; `Azul::<Enum>::<Variant>`
        // already reaches every value of it.
        if !e.is_union {
            continue;
        }
        if matches!(e.category, TypeCategory::DestructorOrClone) {
            continue;
        }
        let target = WrapperTarget::Enum(e);
        if !has_idiomatic_surface(&target, ir) {
            continue;
        }
        emit_class_wrapper(builder, target, ir, config, &mut constant_owners_emitted);
        builder.blank();
    }

    for ta in &ir.type_aliases {
        if ta.monomorphized_def.is_none() || !config.should_include_type(&ta.name) {
            continue;
        }
        // An alias that resolves to a definition of its own name would
        // produce a second class for the same type; the struct/enum loops
        // above own those.
        if ir.find_struct(&ta.name).is_some() || ir.find_enum(&ta.name).is_some() {
            continue;
        }
        let target = WrapperTarget::Alias(ta);
        if !has_idiomatic_surface(&target, ir) {
            continue;
        }
        emit_class_wrapper(builder, target, ir, config, &mut constant_owners_emitted);
        builder.blank();
    }

    emit_orphan_constant_modules(builder, ir, &constant_owners_emitted);
}

// ============================================================================
// Wrapper targets
// ============================================================================

/// The IR entry a Ruby wrapper class is generated from. See
/// [`emit_wrappers`] for why all three kinds need one.
#[derive(Clone, Copy)]
enum WrapperTarget<'a> {
    Struct(&'a StructDef),
    Enum(&'a EnumDef),
    /// A monomorphized generic alias. It has no `StructDef`/`EnumDef`; the
    /// concrete layout lives in `TypeAliasDef::monomorphized_def` and is
    /// emitted into the `Native` module by `types::emit_monomorphized_alias`.
    Alias(&'a TypeAliasDef),
}

impl<'a> WrapperTarget<'a> {
    fn name(&self) -> &'a str {
        match *self {
            WrapperTarget::Struct(s) => s.name.as_str(),
            WrapperTarget::Enum(e) => e.name.as_str(),
            WrapperTarget::Alias(t) => t.name.as_str(),
        }
    }

    /// The `StructDef` behind this target, for the struct-shaped extras
    /// (`with`, the Vec iterator, the layout-callback factory).
    fn struct_def(&self) -> Option<&'a StructDef> {
        match *self {
            WrapperTarget::Struct(s) => Some(s),
            _ => None,
        }
    }

    /// Does the wrapped value carry UTF-8 text directly (the IR's single
    /// `TypeCategory::String`)? Such a class decodes its own bytes in `to_s`
    /// and must not have that overridden by the `_toDbgString` one.
    fn is_string_category(&self) -> bool {
        match *self {
            WrapperTarget::Struct(s) => matches!(s.category, TypeCategory::String),
            WrapperTarget::Enum(e) => matches!(e.category, TypeCategory::String),
            WrapperTarget::Alias(_) => false,
        }
    }

    /// One line of provenance in the generated source: which IR entry this
    /// class came from. Cheap, and it makes an unexpected class traceable.
    fn provenance(&self) -> String {
        match *self {
            WrapperTarget::Struct(s) => format!("struct {} ({})", s.name, s.category.description()),
            WrapperTarget::Enum(e) => format!("union {} ({})", e.name, e.category.description()),
            WrapperTarget::Alias(t) => {
                if t.generic_args.is_empty() {
                    format!("monomorphized alias {} = {}", t.name, t.target)
                } else {
                    format!(
                        "monomorphized alias {} = {}<{}>",
                        t.name,
                        t.target,
                        t.generic_args.join(", ")
                    )
                }
            }
        }
    }
}

/// Is there anything to put in this class? A class with no exported function
/// and no api.json constant is an empty shell; skipping it keeps `azul.rb`
/// to the API that exists.
fn has_idiomatic_surface(target: &WrapperTarget, ir: &CodegenIR) -> bool {
    let name = target.name();
    let has_fn = ir.functions.iter().any(|f| f.class_name == name);
    let has_const = ir.constants.iter().any(|c| constant_stem(c, name).is_some());
    has_fn || has_const
}

// ============================================================================
// Discovery
// ============================================================================

/// Build the set of class names that have a `<Name>_delete` C function.
fn collect_delete_targets(ir: &CodegenIR) -> BTreeSet<&str> {
    ir.functions
        .iter()
        .filter(|f| f.kind == FunctionKind::Delete)
        .map(|f| f.class_name.as_str())
        .collect()
}

// ============================================================================
// Per-class emission
// ============================================================================

fn emit_class_wrapper(
    builder: &mut CodeBuilder,
    target: WrapperTarget,
    ir: &CodegenIR,
    config: &CodegenConfig,
    constant_owners_emitted: &mut BTreeSet<String>,
) {
    let class_name = target.name();
    // Ruby-side snake form of the class name; only used to recognise the
    // api.json convention of naming the receiver arg after the class.
    let snake = snake_case(class_name); // e.g. "app"

    // The `_delete` export, if this class owns heap memory. A class without
    // one (a POD struct, a `Copy` union, a monomorphized alias) has nothing
    // to free — arming a finalizer for it would call a symbol that does not
    // exist.
    let delete_rb = class_fn_rb(ir, class_name, FunctionKind::Delete);

    builder.line(&format!("# From the IR's {}.", target.provenance()));
    builder.line(&format!("class {}", class_name));
    builder.indent();

    // attr reader for low-level access (escape hatch, also used to pass an
    // instance to other Native.* calls that take a pointer).
    builder.line("attr_reader :ptr");
    builder.blank();

    // api.json constants of this class (the OpenGL enum values on
    // GlContextPtr today). Emitted before the methods so the class body
    // reads constants-then-behaviour.
    if emit_class_constants(builder, class_name, ir) > 0 {
        constant_owners_emitted.insert(class_name.to_string());
    }

    // Constructor: stores the pointer and (when the value is owned) arms the
    // finalizer.
    builder.line("def initialize(ptr)");
    builder.indent();
    builder.line("@ptr = ptr");
    if delete_rb.is_some() {
        builder.line("ObjectSpace.define_finalizer(self, self.class.finalize(ptr))");
    }
    builder.dedent();
    builder.line("end");
    builder.blank();

    // CC-4: fluent `with(opts_hash)` builder. Recursively assigns
    // hash keys into the underlying FFI::Struct's nested fields,
    // auto-converting Ruby Strings to AzString via `Azul._az_string`.
    // Drops user-visible field-drilling like
    //   `window.ptr[:window_state][:title] = Azul._az_string('...')`
    // in favor of
    //   `window.with(window_state: { title: 'Hello World' })`
    // Returns self for chain composition with `.with_*` builder
    // methods. Pure FFI::Struct-driven; no per-field allow-list.
    //
    // Struct targets only: a tagged union's FFI fields are `:tag` and
    // `:payload`, so assigning named fields into one is never what the
    // caller meant — its variant constructors are the way in.
    if target.struct_def().is_some() {
        builder.line("# Fluent builder: recursively assigns `opts_hash` into the wrapper's");
        builder.line("# nested FFI::Struct fields. Ruby Strings auto-convert to AzString.");
        builder.line("# Returns self for chaining.");
        builder.line("def with(opts)");
        builder.indent();
        builder.line("Azul._apply_opts(@ptr, opts)");
        builder.line("self");
        builder.dedent();
        builder.line("end");
        builder.blank();
    }

    // Finalizer factory: a class-level proc that closes over `ptr` only.
    // This is critical — closing over `self` would keep the instance alive
    // and the finalizer would never fire.
    if let Some(delete_rb) = delete_rb.as_deref() {
        builder.line("def self.finalize(ptr)");
        builder.indent();
        builder.line(&format!("proc {{ Native.{}(ptr) }}", delete_rb));
        builder.dedent();
        builder.line("end");
        builder.blank();
    }

    // AzString gets a `to_s` override that decodes the wrapped UTF-8
    // bytes into a Ruby String. AzString's C-side layout is `{ vec:
    // AzU8Vec }`, AzU8Vec is `{ ptr, len, cap, destructor }`, so we
    // read offset 0 (vec.ptr) and offset 8 (vec.len) via FFI::Pointer.
    if target.is_string_category() {
        builder.line("# Decode the wrapped UTF-8 bytes into a Ruby String.");
        builder.line("def to_s");
        builder.indent();
        builder.line("vec_ptr = @ptr.get_pointer(0)");
        builder.line("vec_len = @ptr.get_uint64(8)");
        builder.line("return '' if vec_ptr.null? || vec_len.zero?");
        builder.line("vec_ptr.read_bytes(vec_len).force_encoding('UTF-8')");
        builder.dedent();
        builder.line("end");
        builder.blank();
    }

    // Method names this emitter owns. Seeding them means a method loop
    // below can never silently redefine one of them out of existence —
    // it gets a suffixed name instead. (No api.json export collides with
    // any of these today; this is the guard, not a fix.)
    let mut emitted_names: BTreeSet<String> = ["initialize", "finalize", "ptr"]
        .iter()
        .map(|n| n.to_string())
        .collect();
    if target.struct_def().is_some() {
        // `with` is emitted above; `each` below, for the Vec shape.
        emitted_names.insert("with".to_string());
        emitted_names.insert("each".to_string());
    }

    // Button#on_click(data, fn_or_block) — smart instance method:
    // accepts any Ruby object as the data payload and a Proc / lambda
    // / block as the click handler. Wraps both via the host invoker.
    // Phase J.1 (Ruby): same shared detector. Emit `def <event>(data, fn
    // = nil, &block)` for every method matching the with_on_*(self,
    // RefAny, <CallbackWrapperStruct>) shape.
    for func in ir.functions_for_class(class_name) {
        let Some((smart_snake, _wrapper_kind)) =
            super::super::managed_host_invoker::smart_callback_setter_info(func)
        else {
            continue;
        };
        // Ruby method names are snake_case; the smart name is already
        // snake_case from the detector.
        builder.line(&"# Smart builder: pass any Ruby object + a Proc/lambda/block.".to_string());
        builder.line(&format!(
            "# Delegates to {} which already auto-registers via _register_callback.",
            func.method_name
        ));
        emitted_names.insert(smart_snake.clone());
        builder.line(&format!("def {}(data, fn_arg = nil, &block)", smart_snake));
        builder.indent();
        builder.line("fn = fn_arg || block");
        builder.line("raise ArgumentError, 'callback fn required' unless fn");
        // The delegate wraps `data` (idempotently) and hands libazul a
        // clone, so any Ruby object or an existing Azul::RefAny works.
        builder.line(&format!("self.{}(data, fn)", ruby_method_name(&func.method_name)));
        builder.dedent();
        builder.line("end");
        builder.blank();
    }

    // <Class>.create_with_layout(fn) — smart factory that hides the
    // host-invoker plumbing. Ruby FFI's nested-struct field
    // assignment uses the same memory (no JNA reference-swap quirk),
    // so we register the user's callable, fetch a `_default()` value,
    // splice the registered-callback struct into the embedded nested
    // field, and return the wrapper. It sits next to the class's own
    // `create(...)` export, which the method loop emits as usual: that
    // one binds the `<fn>Struct` twin (see `managed_c_symbol`), so it
    // takes the whole callback wrapper — host-handle ctx included — by
    // value and reaches the user's Proc just as this factory does. This
    // one additionally accepts a block and needs no explicit default.
    //
    // Class name, default factory name, callback wrapper, dotted
    // field path, and the type at each intermediate level are all
    // IR-derived via [`layout_callback_factory_info`] — adding/
    // renaming the eligible class in api.json lights this up
    // without touching this emitter.
    //
    // An info with an empty field path names no slot to splice into, so
    // there would be nothing to assign: drop it rather than guess a field.
    let factory_info = target
        .struct_def()
        .and_then(|s| super::super::managed_host_invoker::layout_callback_factory_info(s, ir))
        .filter(|info| !info.field_path.is_empty());
    if let Some(info) = factory_info.as_ref() {
        let default_ruby = ruby_attach_name(&info.default_c_name);
        emitted_names.insert("create_with_layout".to_string());
        builder.line("# Smart factory: pass a layout-callback Proc/lambda/block;");
        builder.line("# the host-invoker registration and struct-field splice happen");
        builder.line("# internally. Replaces the manual register_callback +");
        builder.line("# `_default` + field-assign dance.");
        builder.line("def self.create_with_layout(layout_fn = nil, &block)");
        builder.indent();
        builder.line("fn = layout_fn || block");
        builder.line("raise ArgumentError, 'layout fn required' unless fn");
        builder.line(&format!(
            "cb_struct = Azul._register_callback('{}', fn)",
            info.callback_wrapper
        ));
        builder.line(&format!("wco = Native.{}()", default_ruby));
        builder.line("# Splice the registered callback into the embedded slot.");
        // `factory_info` above rejected an empty path, so `field_path[0]`
        // always exists.
        let depth = info.field_path.len();
        if depth <= 1 {
            // Direct leaf assignment.
            builder.line(&format!("wco[:{}] = cb_struct", info.field_path[0]));
        } else {
            // Materialise typed FFI views down to the parent of the
            // leaf field, then assign. Ruby FFI nested structs share
            // memory with the parent (unlike JNA's reference-swap),
            // so no write-back is required.
            let mut parent_var = "wco".to_string();
            for (i, seg) in info.field_path.iter().enumerate().take(depth - 1) {
                let parent_type = config.apply_prefix(&info.field_types[i]);
                let lvl_var = format!("__lvl{}", i);
                builder.line(&format!(
                    "{lvl} = Native::{ty}.new({parent}[:{seg}].to_ptr)",
                    lvl = lvl_var,
                    ty = parent_type,
                    parent = parent_var,
                    seg = seg
                ));
                parent_var = lvl_var;
            }
            let leaf = &info.field_path[depth - 1];
            builder.line(&format!("{}[:{}] = cb_struct", parent_var, leaf));
        }
        builder.line("new(wco)");
        builder.dedent();
        builder.line("end");
        builder.blank();
    }

    // Emit each function on this class as a Ruby method.
    //
    // `emitted_names` guards against two exports mapping onto one Ruby
    // name: api.json's `textColor` and a `text_color` next to it both
    // snake-case to `text_color`, and Ruby would silently keep only the
    // last `def`. `unique_method_name` suffixes the later one instead, so
    // no export is lost to a naming accident.
    let mut emitted_any_method = false;
    for func in &ir.functions {
        if func.class_name != *class_name {
            continue;
        }
        if !should_emit_method(func) {
            continue;
        }
        let rb_name = unique_method_name(&func.method_name, &mut emitted_names);
        emit_method(builder, func, &rb_name, class_name, &snake, ir, config);
        emitted_any_method = true;
    }

    // `_createDefault` is Ruby's `Foo.default`. The generic method loop
    // already emitted it under its api.json spelling (`create_default`);
    // this is the idiomatic name pointing at the same call, never a second
    // trip into libazul.
    emit_rb_default_alias(builder, class_name, ir, &mut emitted_names);

    if !emitted_any_method {
        builder.line("# (no public methods exposed)");
    }

    // Phase I.2.5: route ==/eql?/hash through the codegen-emitted
    // C-ABI helpers whenever the class exports them.
    emit_rb_eq_hash_if_supported(builder, class_name, ir);

    // `_cmp` / `_partialCmp` → `<=>` + Comparable, so `sort`, `min`,
    // `max`, `clamp` and the range operators all work on the C ordering.
    emit_rb_cmp_if_supported(builder, class_name, ir);

    // Phase I.3.3 (Ruby): override to_s + inspect through Az<X>_toDbgString.
    emit_rb_to_s_if_supported(builder, &target, ir);

    // Phase I.1.6 (Ruby): if this wrapper's underlying struct is a Vec
    // (ptr/len/cap/destructor shape), include Enumerable + emit `each`.
    if let Some(s) = target.struct_def() {
        emit_rb_each_if_vec(builder, s, ir, config);
    }

    builder.dedent();
    builder.line(&format!("end # class {}", class_name));
}

/// Phase I.1.6 (Ruby): if this wrapper's underlying struct matches the
/// codegen-emitted Vec shape (fields [ptr, len, cap, destructor]),
/// `include Enumerable` and emit a `def each` that iterates via FFI
/// pointer arithmetic. Pure type-driven (no name allowlist).
fn emit_rb_each_if_vec(
    builder: &mut CodeBuilder,
    s: &StructDef,
    ir: &CodegenIR,
    config: &CodegenConfig,
) {
    let Some(elem_rust) = detect_vec_elem_type(s) else {
        return;
    };
    builder.line("include Enumerable");
    builder.line("# Phase I.1: iterate the underlying Vec buffer.");
    builder.line("def each");
    builder.indent();
    builder.line("return enum_for(:each) unless block_given?");
    // @ptr is the FFI::Struct returned by-value (an AzXVec). Access
    // its fields directly via the [] indexer.
    builder.line("buf = @ptr[:ptr]");
    builder.line("n = @ptr[:len]");
    builder.line("return if buf.null? || n.zero?");
    // Primitive vs struct element handling.
    let primitive = matches!(
        elem_rust.as_str(),
        "u8" | "i8" | "u16" | "i16" | "u32" | "i32" | "u64" | "i64" | "f32" | "f64" | "bool"
    );
    if primitive {
        let read_method = match elem_rust.as_str() {
            "u8" => "read_uint8",
            "i8" => "read_int8",
            "u16" => "read_uint16",
            "i16" => "read_int16",
            "u32" => "read_uint32",
            "i32" => "read_int32",
            "u64" => "read_uint64",
            "i64" => "read_int64",
            "f32" => "read_float",
            "f64" => "read_double",
            "bool" => "read_uint8",
            _ => unreachable!(),
        };
        let elem_size = match elem_rust.as_str() {
            "u8" | "i8" | "bool" => 1,
            "u16" | "i16" => 2,
            "u32" | "i32" | "f32" => 4,
            _ => 8,
        };
        builder.line(&format!(
            "(0...n).each {{ |i| yield (buf + i * {}).{} }}",
            elem_size, read_method
        ));
    } else {
        // Struct element. The naive `Native::Az<T>.new(buf + i*size)`
        // overlay would dangle the moment this Vec is closed —
        // every yielded element would carry a pointer into the
        // Vec's owned heap. Clone each element via `Az<T>_clone`
        // (when available) so the yielded FFI::Struct owns its own
        // heap allocations and survives the Vec being closed.
        // Mirrors Java/Kotlin/C# Vec-iterator clone-via-_clone
        // pattern (commit 4edb65d7c).
        let elem_prefixed = config.apply_prefix(&elem_rust);
        let clone_rb = class_fn_rb(ir, &elem_rust, FunctionKind::DeepCopy);
        builder.line(&format!("elem_size = Native::{}.size", elem_prefixed));
        builder.line("(0...n).each do |i|");
        builder.indent();
        if let Some(clone_rb) = clone_rb {
            // Clone via the C export. `Native.az_<elem>_clone`
            // returns a fresh FFI::Struct whose internal heap
            // allocations are independent of the Vec's buffer.
            builder.line(&format!("yield Native.{}(buf + i * elem_size)", clone_rb));
        } else {
            // No _clone available — fall back to the borrowed
            // overlay shape and rely on the user not retaining
            // yielded elements past the Vec's lifetime. Comment
            // emitted for runtime clarity.
            builder.line(
                "# WARNING: element type has no _clone — yielded values borrow from the Vec's \
                 buffer.",
            );
            builder.line(&format!(
                "yield Native::{}.new(buf + i * elem_size)",
                elem_prefixed
            ));
        }
        builder.dedent();
        builder.line("end");
    }
    builder.dedent();
    builder.line("end");
    builder.blank();
}

/// Vec-shape detector mirroring lang_haskell::types::detect_vec_elem_type.
fn detect_vec_elem_type(s: &StructDef) -> Option<String> {
    if s.fields.len() != 4 {
        return None;
    }
    let f_ptr = &s.fields[0];
    let f_len = &s.fields[1];
    let f_cap = &s.fields[2];
    if f_ptr.name != "ptr" || f_len.name != "len" || f_cap.name != "cap" {
        return None;
    }
    if f_len.type_name.trim() != "usize" || f_cap.type_name.trim() != "usize" {
        return None;
    }
    let raw = f_ptr.type_name.trim();
    let elem = raw
        .strip_prefix("*mut ")
        .or_else(|| raw.strip_prefix("*const "))
        .map(str::trim)
        .unwrap_or(raw);
    if elem.is_empty() {
        return None;
    }
    Some(elem.to_string())
}

/// Phase I.3.3 (Ruby): override `to_s` + `inspect` routed through
/// `Az<X>_toDbgString` whenever the class exports it.
///
/// The `TypeCategory::String` class keeps the `to_s` that decodes its own
/// UTF-8 bytes — that is the string's *value*, not a debug rendering — and
/// gets the derive as `inspect` alone, which is exactly Ruby's split
/// between the two.
///
/// Gated on the export, not on `TypeTraits::is_debug`: the export is what
/// the C ABI actually has, and the alias types carry no struct/enum traits
/// at all.
fn emit_rb_to_s_if_supported(builder: &mut CodeBuilder, target: &WrapperTarget, ir: &CodegenIR) {
    let Some(dbg) = class_fn(ir, target.name(), FunctionKind::DebugToString) else {
        return;
    };
    // The returned AzString is owned by us: free it through the IR's
    // String-category `_delete` export once decoded.
    let Some(string_delete_rb) = string_delete_rb(ir) else {
        return;
    };
    // `to_s` is already the decoded text for the string class itself.
    let value_to_s_is_the_text = target.is_string_category();
    let method = if value_to_s_is_the_text {
        "inspect"
    } else {
        "to_s"
    };
    builder.line(&format!("# String repr routed through {}.", dbg.c_name));
    builder.line(&format!("def {}", method));
    builder.indent();
    builder.line("return '' if @ptr.nil?");
    builder.line(&format!("az_str = Native.{}(@ptr)", ruby_attach_name(&dbg.c_name)));
    // az_str is an AzString::ByValue FFI::Struct. Decode via vec.ptr/.len.
    builder.line("vec_ptr = az_str[:vec][:ptr]");
    builder.line("vec_len = az_str[:vec][:len]");
    builder.line("return '' if vec_ptr.null? || vec_len.zero?");
    builder.line("out = vec_ptr.read_bytes(vec_len).force_encoding('UTF-8')");
    // Free the AzString via the FFI struct's address.
    builder.line(&format!(
        "Native.{}(FFI::Pointer.new(az_str.to_ptr.address))",
        string_delete_rb
    ));
    builder.line("out");
    builder.dedent();
    builder.line("end");
    if !value_to_s_is_the_text {
        builder.line("alias_method :inspect, :to_s");
    }
    builder.blank();
}

/// Phase I.2.5 (Ruby): override `==` / `eql?` / `hash` routed through
/// the codegen-emitted `Az<X>_partialEq` / `Az<X>_hash` exports.
///
/// Gated on the exports themselves rather than on `TypeTraits`: the two
/// agree for structs and enums (the trait functions are generated *from*
/// the traits) and the monomorphized aliases have the exports without
/// carrying a `TypeTraits` of their own.
fn emit_rb_eq_hash_if_supported(builder: &mut CodeBuilder, class_name: &str, ir: &CodegenIR) {
    let eq_fn = class_fn(ir, class_name, FunctionKind::PartialEq);
    let hash_fn = class_fn(ir, class_name, FunctionKind::Hash);
    let has_eq = eq_fn.is_some();

    if let Some(eq) = eq_fn {
        builder.line(&format!("# Equality routed through {}.", eq.c_name));
        builder.line("def ==(other)");
        builder.indent();
        builder.line("return false unless other.is_a?(self.class)");
        builder.line("return @ptr == other.ptr if @ptr.nil? || other.ptr.nil?");
        builder.line(&format!(
            "Native.{}(@ptr, other.ptr)",
            ruby_attach_name(&eq.c_name)
        ));
        builder.dedent();
        builder.line("end");
        builder.line("alias_method :eql?, :==");
        builder.blank();
    }

    if let Some(h) = hash_fn {
        builder.line(&format!("# Hash routed through {}.", h.c_name));
        builder.line("def hash");
        builder.indent();
        builder.line("return 0 if @ptr.nil?");
        builder.line(&format!("Native.{}(@ptr)", ruby_attach_name(&h.c_name)));
        builder.dedent();
        builder.line("end");
        builder.blank();
    } else if has_eq {
        // == compares VALUES (the C `_partialEq`) and the type has no C
        // `_hash`: equal values must hash equal, so the only hash that keeps
        // the contract is a constant (a pointer address differs between two
        // equal values).
        builder.line("# Constant: equal values must hash equal, and the type has no value hash.");
        builder.line("def hash");
        builder.indent();
        builder.line("0");
        builder.dedent();
        builder.line("end");
        builder.blank();
    }
}

/// `<=>` + `Comparable`, routed through the C ordering derive.
///
/// The C ABI answers an ordering as `0 = Less, 1 = Equal, 2 = Greater`
/// (see the `Ord`/`PartialOrd` impls in `dll_api_external.rs`), which is
/// Ruby's `-1 / 0 / 1` shifted by one; `_partialCmp` additionally answers
/// `3` for "not comparable", which Ruby spells `nil`.
///
/// `_cmp` wins when both exist: a total order can never answer `nil`, and
/// `api.json`'s own reachability rule treats `_partialCmp` as redundant
/// next to a `_cmp`.
fn emit_rb_cmp_if_supported(builder: &mut CodeBuilder, class_name: &str, ir: &CodegenIR) {
    let (ord_fn, total) = match class_fn(ir, class_name, FunctionKind::Cmp) {
        Some(f) => (f, true),
        None => match class_fn(ir, class_name, FunctionKind::PartialCmp) {
            Some(f) => (f, false),
            None => return,
        },
    };
    // Comparable derives <, <=, >, >=, between? and clamp from `<=>`.
    // Our own `==` (emitted just above, routed through `_partialEq`)
    // still wins over Comparable#==: a method defined in the class
    // always beats one from an included module.
    builder.line("include Comparable");
    builder.line(&format!(
        "# Ordering routed through {} (C answers 0=Less, 1=Equal, 2=Greater).",
        ord_fn.c_name
    ));
    builder.line("def <=>(other)");
    builder.indent();
    builder.line("return nil unless other.is_a?(self.class)");
    builder.line("return nil if @ptr.nil? || other.ptr.nil?");
    builder.line(&format!(
        "_ord = Native.{}(@ptr, other.ptr)",
        ruby_attach_name(&ord_fn.c_name)
    ));
    if total {
        builder.line("_ord - 1");
    } else {
        // A partial order answers anything above Greater for "these two
        // are not comparable"; Ruby's protocol for that is nil.
        builder.line("_ord > 2 ? nil : _ord - 1");
    }
    builder.dedent();
    builder.line("end");
    builder.blank();
}

/// `Foo.default` next to the api.json spelling of `Az<Foo>_createDefault`.
///
/// Ruby's name for "the type's default value" is `default`; the generic
/// method loop emitted the export under its api.json name a few lines up,
/// and this forwards to it. Skipped when the class has no `_createDefault`
/// or already defines something called `default`.
fn emit_rb_default_alias(
    builder: &mut CodeBuilder,
    class_name: &str,
    ir: &CodegenIR,
    emitted_names: &mut BTreeSet<String>,
) {
    let Some(def) = class_fn(ir, class_name, FunctionKind::Default) else {
        return;
    };
    let target_name = ruby_method_name(&def.method_name);
    // The loop must have emitted it (Default is not a hidden kind), but if
    // a collision renamed it we would be forwarding to the wrong method.
    if !emitted_names.contains(&target_name) {
        return;
    }
    if !emitted_names.insert("default".to_string()) {
        return;
    }
    builder.line(&format!("# Ruby's name for {}.", target_name));
    builder.line("def self.default");
    builder.indent();
    builder.line(&target_name);
    builder.dedent();
    builder.line("end");
    builder.blank();
}

/// Emit the api.json constants declared on this class as Ruby constants.
///
/// api.json spells them `<Class>_<NAME>` (`GlContextPtr_ACCUM_ALPHA_BITS`)
/// and the values are C integer literals Ruby parses identically
/// (`0x0D5B`), so the class body is all the scoping they need:
/// `Azul::GlContextPtr::ACCUM_ALPHA_BITS`. Returns how many were emitted.
fn emit_class_constants(builder: &mut CodeBuilder, class_name: &str, ir: &CodegenIR) -> usize {
    let mut n = 0;
    for c in &ir.constants {
        let Some(stem) = constant_stem(c, class_name) else {
            continue;
        };
        if n == 0 {
            builder.line("# api.json constants of this class.");
        }
        builder.line(&format!(
            "{} = {} # {}",
            ruby_const_name(&stem),
            c.value,
            c.type_name
        ));
        n += 1;
    }
    if n > 0 {
        builder.blank();
    }
    n
}

/// Constants whose owning class produced no wrapper (nothing does today —
/// every constant in api.json belongs to a class with exports of its own —
/// but a new one must not silently vanish). A plain module is the smallest
/// home that keeps the `Azul::<Class>::<NAME>` spelling.
fn emit_orphan_constant_modules(
    builder: &mut CodeBuilder,
    ir: &CodegenIR,
    already_emitted: &BTreeSet<String>,
) {
    let mut owners: BTreeSet<&str> = BTreeSet::new();
    for c in &ir.constants {
        let Some((class, _)) = c.name.split_once('_') else {
            continue;
        };
        if already_emitted.contains(class) {
            continue;
        }
        owners.insert(class);
    }
    for owner in owners {
        builder.line(&format!(
            "# Constants of {}, which exports no function of its own.",
            owner
        ));
        builder.line(&format!("module {}", owner));
        builder.indent();
        for c in &ir.constants {
            let Some(stem) = constant_stem(c, owner) else {
                continue;
            };
            builder.line(&format!("{} = {}", ruby_const_name(&stem), c.value));
        }
        builder.dedent();
        builder.line("end");
        builder.blank();
    }
}

/// `constant.name` with the `<class>_` prefix stripped, when it names a
/// constant of `class_name`.
fn constant_stem(c: &ConstantDef, class_name: &str) -> Option<String> {
    let (class, _) = c.name.split_once('_')?;
    (class == class_name).then(|| c.member_name())
}

/// The Ruby name to define `method` under, reserving it in `taken`.
///
/// Two api.json exports can snake-case onto one Ruby name. Redefining a
/// `def` silently drops the first one, so the later export gets a numeric
/// suffix and stays callable.
fn unique_method_name(method: &str, taken: &mut BTreeSet<String>) -> String {
    let base = ruby_method_name(method);
    let mut candidate = base.clone();
    let mut n = 2;
    while !taken.insert(candidate.clone()) {
        candidate = format!("{}_{}", base, n);
        n += 1;
    }
    candidate
}

/// Should this function be exposed as a Ruby method on the wrapper?
///
/// We hide the trait functions Ruby reaches through its own protocol
/// instead: `_delete` fires from the finalizer, `_partialEq` / `_hash` /
/// `_cmp` / `_toDbgString` are `==` / `hash` / `<=>` / `to_s`. Everything
/// else — constructors, methods, static helpers, `_deepCopy`,
/// `_createDefault` and every enum variant constructor — is a method here.
fn should_emit_method(func: &FunctionDef) -> bool {
    !matches!(
        func.kind,
        FunctionKind::Delete
            | FunctionKind::PartialEq
            | FunctionKind::PartialCmp
            | FunctionKind::Cmp
            | FunctionKind::Hash
            | FunctionKind::DebugToString
    )
}

/// Phase I.5.3 (Ruby): classify the return type so we can auto-unwrap
/// `Option<T>` → nil/value and `Result<T,E>` → value/raise at the
/// wrapper boundary. Detection via the enum variant names `[None,Some]`
/// or `[Ok,Err]` — same predicate Haskell H.4/H.5 used.
#[derive(Clone, Copy, PartialEq)]
enum ReturnIdiom {
    Plain,
    Option,
    Result,
}

fn classify_return(func: &FunctionDef, ir: &CodegenIR) -> ReturnIdiom {
    let Some(rt) = func.return_type.as_deref() else {
        return ReturnIdiom::Plain;
    };
    let rt = rt.trim();
    if let Some(e) = ir.find_enum(rt) {
        if e.variants.len() == 2 {
            let has_none = e.variants.iter().any(|v| v.name == "None");
            let has_some = e.variants.iter().any(|v| v.name == "Some");
            if has_none && has_some {
                return ReturnIdiom::Option;
            }
            let has_ok = e.variants.iter().any(|v| v.name == "Ok");
            let has_err = e.variants.iter().any(|v| v.name == "Err");
            if has_ok && has_err {
                return ReturnIdiom::Result;
            }
        }
    }
    ReturnIdiom::Plain
}

fn emit_method(
    builder: &mut CodeBuilder,
    func: &FunctionDef,
    ruby_method: &str,
    class_name: &str,
    type_snake: &str,
    ir: &CodegenIR,
    config: &CodegenConfig,
) {
    let native_call = ruby_attach_name(&func.c_name);

    // Phase I.4.4 (Ruby): treat DeepCopy as an instance method too, so
    // `dom.clone` works (instead of the awkward `Dom.clone(dom)`
    // static form). The C signature `Az<X> clone(const Az<X>*)` matches
    // a takes-self method exactly.
    let takes_self = matches!(
        func.kind,
        FunctionKind::Method | FunctionKind::MethodMut | FunctionKind::DeepCopy
    );
    // `return_type` and `class_name` are both unprefixed IR names.
    let returns_self_type = func
        .return_type
        .as_deref()
        .map(|t| t.trim() == class_name)
        .unwrap_or(false);
    // A function that returns its OWN class is never an Option/Result
    // hand-back to unwrap — it IS the value. `OptionDom.some(dom)` must
    // return the option it just built, not immediately tear it open
    // again. (For struct classes the two can't collide: `find_enum` of a
    // struct name is None, so this only bites the Option/Result wrappers
    // themselves.)
    let idiom = if returns_self_type {
        ReturnIdiom::Plain
    } else {
        classify_return(func, ir)
    };

    // Does the C call MOVE self? Only when api.json declares
    // `self: "value"` (ir_builder maps that to ArgRefKind::Owned on the
    // arg named after the class). `&self` / `&mut self` receivers pass a
    // pointer — the wrapper stays valid and its finalizer must stay
    // armed. DeepCopy never consumes (clone leaves the original intact).
    let self_is_owned = takes_self
        && !matches!(func.kind, FunctionKind::DeepCopy)
        && func
            .args
            .iter()
            .find(|a| a.name == "self" || a.name == type_snake)
            .map(|a| matches!(a.ref_kind, ArgRefKind::Owned))
            .unwrap_or(false);

    // Plain returns of a DIFFERENT type that has an emitted wrapper class
    // (e.g. `Button#dom` → Dom) get auto-wrapped so users receive
    // `Azul::Dom` instead of a raw Native::AzDom FFI::Struct — consistent
    // with the self-typed path, and the wrapper's finalizer takes
    // ownership of the returned value.
    let wrap_class = if matches!(idiom, ReturnIdiom::Plain) && !returns_self_type {
        return_wrapper_class(func, ir, config)
    } else {
        None
    };

    // Strip an explicit `self` from the visible argument list (Ruby
    // supplies it via `@ptr`). For DeepCopy methods the first arg IS
    // the receiver regardless of how api.json names it (`instance`,
    // lowercased class, etc.) — drop args[0] unconditionally. Same fix
    // the JVM/.NET/Go wrappers landed in earlier phases.
    //
    // An enum variant constructor has no receiver at all: every one of its
    // args is payload, named `payload`/`payload0`/... or after the variant's
    // struct fields. Dropping one because a field happens to be spelled like
    // the class would build the variant from the wrong values.
    let visible_args: Vec<&_> = match func.kind {
        FunctionKind::DeepCopy if !func.args.is_empty() => func.args.iter().skip(1).collect(),
        FunctionKind::EnumVariantConstructor => func.args.iter().collect(),
        _ => func
            .args
            .iter()
            .filter(|a| a.name != "self" && a.name != type_snake)
            .collect(),
    };

    let arg_names: Vec<String> = visible_args
        .iter()
        .map(|a| ruby_arg_name(&a.name))
        .collect();

    // Names of args we should mark consumed after the C call: owned-by-
    // value wrapper-class instances. The C side moves them; the
    // wrapper's `ObjectSpace` finalizer must not fire on the now-
    // transferred memory. Skipped:
    //   * callback args — already replaced by FFI::Struct values via
    //     `_register_callback`, not wrappers;
    //   * Owned `String` args — `_az_string` COPIES the Ruby bytes, the
    //     Ruby String is never moved (and `_consume` on it is a no-op);
    //   * RefAny args — passed to C as a CLONE (see `arg_pass_expr`), so
    //     the caller's wrapper keeps its own refcount and stays usable.
    let consumed_names: Vec<String> = visible_args
        .iter()
        .zip(arg_names.iter())
        .filter(|(a, _)| {
            a.callback_info.is_none()
                && matches!(a.ref_kind, ArgRefKind::Owned)
                && !is_az_string_owned_arg(a, ir)
                && !is_refany_arg(a, ir)
        })
        .map(|(_, n)| n.clone())
        .collect();

    if takes_self {
        // Instance method: forward `@ptr` as the receiver.
        if arg_names.is_empty() {
            builder.line(&format!("def {}", ruby_method));
        } else {
            builder.line(&format!("def {}({})", ruby_method, arg_names.join(", ")));
        }
        builder.indent();
        emit_callback_register_lines(builder, &visible_args, &arg_names);
        emit_refany_wrap_lines(builder, &visible_args, &arg_names, ir);
        let mut call_args = vec!["@ptr".to_string()];
        for (name, a) in arg_names.iter().zip(visible_args.iter()) {
            call_args.push(arg_pass_expr(a, name, ir));
        }
        let call = format!("Native.{}({})", native_call, call_args.join(", "));
        // Consume self exactly when the C ABI takes it by value (see
        // self_is_owned above) — regardless of what the method returns.
        // Previously only the returns-self builder branch disarmed the
        // finalizer, so consuming methods returning a DIFFERENT type
        // (all 43 widget `.dom` methods) left it armed → double free.
        let consumes_self = self_is_owned;
        emit_method_body_instance(
            builder,
            &call,
            &func.return_type,
            returns_self_type,
            &consumed_names,
            consumes_self,
            idiom,
            wrap_class.as_deref(),
        );
        builder.dedent();
        builder.line("end");
        // Phase I.4.4 (Ruby): alias `dup` to `clone` for DeepCopy
        // methods so user code matches Ruby's value-type idiom.
        if matches!(func.kind, FunctionKind::DeepCopy) && ruby_method == "clone" {
            builder.line("alias_method :dup, :clone");
        }
        builder.blank();
        return;
    }

    // Static method (constructor / static helper).
    if arg_names.is_empty() {
        builder.line(&format!("def self.{}", ruby_method));
    } else {
        builder.line(&format!(
            "def self.{}({})",
            ruby_method,
            arg_names.join(", ")
        ));
    }
    builder.indent();
    emit_callback_register_lines(builder, &visible_args, &arg_names);
    emit_refany_wrap_lines(builder, &visible_args, &arg_names, ir);
    let call_args: Vec<String> = arg_names
        .iter()
        .zip(visible_args.iter())
        .map(|(n, a)| arg_pass_expr(a, n, ir))
        .collect();
    let call = format!("Native.{}({})", native_call, call_args.join(", "));
    emit_method_body_static(
        builder,
        &call,
        &func.return_type,
        returns_self_type,
        &consumed_names,
        idiom,
        wrap_class.as_deref(),
    );
    builder.dedent();
    builder.line("end");
    builder.blank();
}

/// Emit `name = Azul._register_callback('Wrapper', name)` for every
/// callback-typed arg whose wrapper is in the host-invoker list. Hosts
/// pass plain Ruby callables (Proc / lambda / method); the helper
/// stashes them in `@_ruby_handles` and returns the `Az<Wrapper>`
/// FFI::Struct the C-ABI takes.
fn emit_callback_register_lines(
    builder: &mut CodeBuilder,
    args: &[&FunctionArg],
    arg_names: &[String],
) {
    if !args.iter().any(|a| a.callback_info.is_some()) {
        return;
    }
    for (i, a) in args.iter().enumerate() {
        let Some(cb) = a.callback_info.as_ref() else {
            continue;
        };
        let wrapper = cb.callback_wrapper_name.as_str();
        // Every callback wrapper has an `impl_managed_callback!` thunk
        // (bug_classes::every_callback_wrapper_is_host_invokable).
        builder.line(&format!(
            "{n} = Azul._register_callback('{w}', {n})",
            n = arg_names[i],
            w = wrapper
        ));
    }
}

/// Emit `name = Azul::<RefAny>.wrap(name)` for every arg whose IR type is
/// the `TypeCategory::RefAny` struct. `wrap` is idempotent (an existing
/// wrapper passes through, a raw struct is adopted, any other Ruby value
/// gets a fresh host handle), so callers may pass plain objects or an
/// `Azul::RefAny` they hold on to.
fn emit_refany_wrap_lines(
    builder: &mut CodeBuilder,
    args: &[&FunctionArg],
    arg_names: &[String],
    ir: &CodegenIR,
) {
    for (a, n) in args.iter().zip(arg_names.iter()) {
        if is_refany_arg(a, ir) {
            builder.line(&format!(
                "{n} = Azul::{t}.wrap({n})",
                n = n,
                t = a.type_name.trim()
            ));
        }
    }
}

/// The expression that puts one wrapper-method argument on the C call.
///
/// * callback args — already an `Az<Kind>` FFI::Struct from
///   `_register_callback`, passed as-is;
/// * Owned `String` args — `Azul._az_string(x)` copies the Ruby bytes
///   into an owned AzString;
/// * RefAny args — `Native.<RefAny_clone>(x.ptr)`: libazul gets its own
///   refcount, the caller's `Azul::RefAny` keeps its own (finalizer →
///   `_delete`). Never a by-value move of a struct Ruby still aliases.
///   Falls back to a plain move only if the IR had no `_clone` export
///   for the RefAny struct (then `consumed_names` would have to include
///   it — it does not today because api.json always has one);
/// * everything else — `(x.respond_to?(:ptr) ? x.ptr : x)` so wrapper
///   instances and raw FFI values are both accepted.
fn arg_pass_expr(a: &FunctionArg, name: &str, ir: &CodegenIR) -> String {
    if a.callback_info.is_some() {
        return name.to_string();
    }
    if is_az_string_owned_arg(a, ir) {
        return format!("Azul._az_string({})", name);
    }
    if is_refany_arg(a, ir) && matches!(a.ref_kind, ArgRefKind::Owned) {
        if let Some(clone_rb) = class_fn_rb(ir, a.type_name.trim(), FunctionKind::DeepCopy) {
            return format!("Native.{}({}.ptr)", clone_rb, name);
        }
    }
    unwrap_expr(name)
}

/// Emit the body of an instance method (`def foo ... end`). The
/// `consumed_names` are owned-by-value wrapper-typed args that the C
/// side took ownership of; we tag them as consumed after the call so
/// the wrapper's finalizer won't fire on transferred memory.
fn emit_method_body_instance(
    builder: &mut CodeBuilder,
    call: &str,
    return_type: &Option<String>,
    returns_self_type: bool,
    consumed_names: &[String],
    consumes_self: bool,
    idiom: ReturnIdiom,
    wrap_class: Option<&str>,
) {
    // Phase I.5.3 (Ruby): Option<T>/Result<T,E> auto-unwrap at the
    // wrapper boundary. Detected via classify_return; the AzOption /
    // AzResult FFI structs already expose to_opt / unwrap methods
    // emitted by the accessor codegen.
    if matches!(idiom, ReturnIdiom::Option | ReturnIdiom::Result) {
        builder.line(&format!("_ret = {}", call));
        for n in consumed_names {
            builder.line(&format!("Azul._consume({})", n));
        }
        if consumes_self {
            emit_self_disarm(builder);
        }
        match idiom {
            ReturnIdiom::Option => builder.line("_ret.to_opt"),
            ReturnIdiom::Result => builder.line("_ret.unwrap"),
            _ => unreachable!(),
        };
        return;
    }
    match return_type {
        None => {
            builder.line(call);
            for n in consumed_names {
                builder.line(&format!("Azul._consume({})", n));
            }
            if consumes_self {
                emit_self_disarm(builder);
                builder.line("nil");
            }
        }
        Some(_) if returns_self_type && consumes_self => {
            // Consuming-builder: self is moved into the C call along
            // with any owned-by-value wrapper args. The returned value
            // is a fresh struct; wrap in a new instance, and mark all
            // moved-from wrappers (self + args) as consumed.
            builder.line(&format!("_next = {}", call));
            for n in consumed_names {
                builder.line(&format!("Azul._consume({})", n));
            }
            emit_self_disarm(builder);
            builder.line("self.class.new(_next)");
        }
        Some(_) if returns_self_type => {
            // Non-consuming clone-style method: self stays valid; just
            // wrap the returned struct in a fresh instance.
            builder.line(&format!("_next = {}", call));
            for n in consumed_names {
                builder.line(&format!("Azul._consume({})", n));
            }
            builder.line("self.class.new(_next)");
        }
        Some(_) => {
            // Plain return of a different type. If the C ABI moved self
            // (self: "value" in api.json — e.g. every widget's `.dom`),
            // disarm the finalizer so it can't double-free the moved-out
            // struct. If the return type has a wrapper class, hand back
            // the wrapper instead of the raw FFI::Struct.
            if consumed_names.is_empty() && !consumes_self && wrap_class.is_none() {
                builder.line(call);
            } else {
                builder.line(&format!("_ret = {}", call));
                for n in consumed_names {
                    builder.line(&format!("Azul._consume({})", n));
                }
                if consumes_self {
                    emit_self_disarm(builder);
                }
                match wrap_class {
                    Some(cls) => builder.line(&format!("{}.new(_ret)", cls)),
                    None => builder.line("_ret"),
                }
            }
        }
    }
}

/// Emit the body of a class method (`def self.foo ... end`). Same
/// rules as the instance variant, minus the self-consume step.
fn emit_method_body_static(
    builder: &mut CodeBuilder,
    call: &str,
    return_type: &Option<String>,
    returns_self_type: bool,
    consumed_names: &[String],
    idiom: ReturnIdiom,
    wrap_class: Option<&str>,
) {
    if matches!(idiom, ReturnIdiom::Option | ReturnIdiom::Result) {
        builder.line(&format!("_ret = {}", call));
        for n in consumed_names {
            builder.line(&format!("Azul._consume({})", n));
        }
        match idiom {
            ReturnIdiom::Option => builder.line("_ret.to_opt"),
            ReturnIdiom::Result => builder.line("_ret.unwrap"),
            _ => unreachable!(),
        };
        return;
    }
    match return_type {
        None => {
            builder.line(call);
            for n in consumed_names {
                builder.line(&format!("Azul._consume({})", n));
            }
        }
        Some(_) if returns_self_type => {
            builder.line(&format!("_next = {}", call));
            for n in consumed_names {
                builder.line(&format!("Azul._consume({})", n));
            }
            // `new(...)` resolves to the surrounding class in both
            // instance methods (`def foo`) and class methods (`def self.foo`).
            // `self.class.new(...)` is wrong in class methods because
            // there `self.class` is `Class`, not the wrapper class.
            builder.line("new(_next)");
        }
        Some(_) => {
            // Plain return of a different type: wrap in its wrapper
            // class when one exists (mirrors the instance path).
            if consumed_names.is_empty() && wrap_class.is_none() {
                builder.line(call);
            } else {
                builder.line(&format!("_ret = {}", call));
                for n in consumed_names {
                    builder.line(&format!("Azul._consume({})", n));
                }
                match wrap_class {
                    Some(cls) => builder.line(&format!("{}.new(_ret)", cls)),
                    None => builder.line("_ret"),
                }
            }
        }
    }
}

/// Emit the "self was moved into the C call" epilogue: undefine the
/// wrapper's finalizer (so it can't double-free the transferred struct)
/// and nil out `@ptr` (so later use fails visibly instead of passing a
/// dangling struct back into the FFI).
fn emit_self_disarm(builder: &mut CodeBuilder) {
    builder.line("begin");
    builder.indent();
    builder.line("ObjectSpace.undefine_finalizer(self)");
    builder.dedent();
    builder.line("rescue StandardError");
    builder.line("end");
    builder.line("@ptr = nil");
}

/// If `func`'s plain return type has an emitted Ruby wrapper class
/// (i.e. the struct passes `should_emit_struct` AND owns a `_delete`
/// C function — the exact condition `emit_wrappers` uses), return the
/// class name so call sites can auto-wrap the raw FFI::Struct.
fn return_wrapper_class(
    func: &FunctionDef,
    ir: &CodegenIR,
    config: &CodegenConfig,
) -> Option<String> {
    let rt = func.return_type.as_deref()?.trim();
    let s = ir.structs.iter().find(|st| st.name == rt)?;
    if !should_emit_struct(s, config) {
        return None;
    }
    let has_delete = ir
        .functions
        .iter()
        .any(|f| f.kind == FunctionKind::Delete && f.class_name == rt);
    if !has_delete {
        return None;
    }
    Some(rt.to_string())
}

/// Wrap a positional argument in a tiny `_unwrap` helper that accepts both
/// raw pointers/values and wrapper instances. We emit the inline form so
/// no module-level helper is required.
fn unwrap_expr(name: &str) -> String {
    format!("({n}.respond_to?(:ptr) ? {n}.ptr : {n})", n = name)
}

/// Auto-string-conversion rule (mirrors Java/Kotlin/C#): any Owned arg of
/// the IR's string type accepts a plain Ruby string at the wrapper level.
/// The call site routes the value through `Azul._az_string` (emitted from
/// `managed.rs`, which builds exactly that type). Pure type-driven: the
/// class is found by [`TypeCategory::String`], never by its name.
fn is_az_string_owned_arg(a: &FunctionArg, ir: &CodegenIR) -> bool {
    matches!(a.ref_kind, ArgRefKind::Owned)
        && ir
            .find_struct(a.type_name.trim())
            .is_some_and(|s| matches!(s.category, TypeCategory::String))
}

/// Ruby attach name of the `_delete` export of the IR's
/// `TypeCategory::String` struct (used to free AzStrings we own).
fn string_delete_rb(ir: &CodegenIR) -> Option<String> {
    let s = ir
        .structs
        .iter()
        .find(|s| matches!(s.category, TypeCategory::String))?;
    class_fn_rb(ir, &s.name, FunctionKind::Delete)
}

// ============================================================================
// Naming helpers
// ============================================================================

/// Ruby method names use snake_case. The IR's `method_name` is the
/// api.json key (already snake_case, e.g. `create_p_with_text`);
/// `snake_case` (= the shared `to_snake_case`) is a no-op on those and
/// normalises a camelCase name if one ever appears.
///
/// A name that lands on a Ruby keyword gets the same trailing `_` the
/// argument names get. Ruby's grammar does accept a keyword after `def`,
/// but `def self.class` would then shadow `Class#class` on the wrapper
/// class object — `MyType.class` would answer an enum variant instead of
/// `Class`. Enum variants spelled `Class`, `Next` and `In` exist today.
fn ruby_method_name(method: &str) -> String {
    let snake = snake_case(method);
    if RUBY_RESERVED.contains(&snake.as_str()) {
        format!("{}_", snake)
    } else {
        snake
    }
}

/// Argument names from the IR are usually already snake_case; if they
/// aren't, normalise. Also append a trailing `_` if the name collides
/// with a Ruby keyword.
fn ruby_arg_name(name: &str) -> String {
    let snake = snake_case(name);
    if RUBY_RESERVED.contains(&snake.as_str()) {
        format!("{}_", snake)
    } else {
        snake
    }
}

const RUBY_RESERVED: &[&str] = &[
    "alias", "and", "begin", "break", "case", "class", "def", "defined", "do", "else", "elsif",
    "end", "ensure", "false", "for", "if", "in", "module", "next", "nil", "not", "or", "redo",
    "rescue", "retry", "return", "self", "super", "then", "true", "undef", "unless", "until",
    "when", "while", "yield",
];
