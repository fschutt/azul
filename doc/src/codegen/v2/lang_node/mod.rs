//! Node.js / Bun / Deno binding generator (pure JS FFI via koffi).
//!
//! Emits a single `azul.js` source file plus a `package.json` manifest.
//! The generated `azul.js` is **CommonJS by default** and supports three
//! JavaScript runtimes via a small detection prelude:
//!
//! 1. **Node.js (>= 16)** via [`koffi`](https://koffi.dev/) — a pure-JS
//!    libffi-based loader. This is the *primary* path: koffi works on
//!    every Node.js LTS, ships prebuilt binaries for every common
//!    platform, and accepts plain C-decl strings for type registration.
//! 2. **Bun** via the built-in `bun:ffi` module.
//! 3. **Deno** via `Deno.dlopen`.
//!
//! All three runtimes load the **same prebuilt** `libazul.{so,dylib,dll}`
//! that the C, C++, Lua, PHP, etc. bindings load. There is no native
//! compile step on the consumer's side — koffi is pure JS and ships its
//! own libffi prebuilds; Bun/Deno include FFI primitives in their
//! standard runtime.
//!
//! Why a single file (rather than three siblings):
//!
//! - The **wrapper layer** — ES6 classes with `FinalizationRegistry` for
//!   automatic disposal — is identical across all three runtimes. A
//!   sibling-file split would duplicate it three times.
//! - The runtime-specific code is small and constrained to the
//!   `loadLib()` adapter at the top of the file. Once the lib handle is
//!   normalised into a uniform `dispatch(name, args)` shape, the
//!   wrappers do not care which engine is hosting them.
//! - Single-file consumers can drop the binding into a project without
//!   any build-tool reconfiguration.
//!
//! ## Layout of the emitted `azul.js`
//!
//! ```text
//! 1. Header comment + version / runtime banner
//! 2. Runtime detection (`isNode` / `isBun` / `isDeno`)
//! 3. `loadLib()` adapter — returns a uniform `{ call(name, args) }` object
//! 4. Type registrations (koffi.struct/.union/.array; for Bun/Deno these
//!    are JSDoc-only because their FFI layers don't pre-register types)
//! 5. Function bindings (`koffi.func` / `dlopen` symbol map)
//! 6. ES6 wrapper classes with `FinalizationRegistry`
//! 7. `module.exports` / `globalThis.azul` exports
//! ```
//!
//! ## What is and is NOT skipped
//!
//! Skipped (matches the Lua / PHP filters):
//!
//! - `TypeCategory::Recursive`        (infinite-size types)
//! - `TypeCategory::VecRef`           (raw slice pointers)
//! - `TypeCategory::Boxed`            (internal heap wrappers)
//! - `TypeCategory::GenericTemplate`  (parameterised shells)
//! - `TypeCategory::DestructorOrClone`(internal callback typedefs)
//! - `TypeCategory::CallbackTypedef`  (function-pointer typedefs;
//!   user-facing CallbackDataPair wrappers ARE emitted, and consumers
//!   wrap their JS callbacks via `koffi.proto(...)`)
//!
//! Emitted with full wrapper treatment:
//!
//! - `TypeCategory::Regular` and `TypeCategory::CallbackDataPair`
//! - Unit-only enums become flat constant tables (`azul.LayoutAxis.Horizontal`)
//! - Tagged-union enums get tag constants + per-variant predicates
//!
//! ## koffi parser tolerance vs. LuaJIT
//!
//! koffi parses its own C-like type spec language (see
//! https://koffi.dev/usage). It accepts `const`, pointers, arrays, and
//! anonymous structs/unions, but NOT C preprocessor directives, NOT
//! `extern "C"`, and NOT macro expansions — the same restrictions as
//! LuaJIT. We therefore **reuse** [`super::lang_lua::cdef::strip_for_cdef`]
//! to produce a payload acceptable to koffi, with one extra step: koffi
//! prefers per-type registrations as JS calls (e.g.
//! `koffi.struct('AzApp', { ... })`) rather than parsing a giant C
//! header in one go. We therefore generate explicit per-type calls in
//! [`types::generate_type_registrations`] rather than feeding the
//! whole header to koffi.

pub mod functions;
pub mod managed;
pub mod package_json;
pub mod types;
pub mod wrappers;

use anyhow::Result;

use super::config::CodegenConfig;
use super::generator::CodeBuilder;
use super::ir::CodegenIR;

/// File-marker header for multi-file output. Orchestrator splits on
/// lines beginning with this prefix. The marker is a syntactically
/// valid JS line comment so unsplit output still parses.
pub const FILE_MARKER: &str = "// ==FILE: ";

/// Trailing marker that closes the file-marker header line.
pub const END_MARKER: &str = " ==";

/// Library base name used by all three runtimes. koffi, Bun, and Deno
/// all resolve `'azul'` to the platform-specific filename
/// (`azul.dll` / `libazul.so` / `libazul.dylib`) when the file lives
/// on the dynamic-loader search path.
pub const DLL_NAME: &str = "azul";

/// Public entry point. Returns a multi-file string with two sections:
/// `azul.js` and `package.json`.
pub fn generate(ir: &CodegenIR, config: &CodegenConfig) -> Result<String> {
    let azul_js = generate_azul_js(ir, config)?;
    let pkg_json = package_json::generate_package_json();

    let mut out = String::with_capacity(azul_js.len() + pkg_json.len() + 256);
    push_section(&mut out, "azul.js", &azul_js);
    push_section(&mut out, "package.json", &pkg_json);
    Ok(out)
}

fn push_section(out: &mut String, path: &str, content: &str) {
    out.push_str(FILE_MARKER);
    out.push_str(path);
    out.push_str(END_MARKER);
    out.push('\n');
    out.push_str(content);
    if !content.ends_with('\n') {
        out.push('\n');
    }
}

// ============================================================================
// azul.js builder
// ============================================================================

fn generate_azul_js(ir: &CodegenIR, _config: &CodegenConfig) -> Result<String> {
    let mut b = CodeBuilder::new("    ");

    emit_header(&mut b);
    emit_runtime_detection(&mut b);
    emit_load_lib(&mut b);
    types::generate_type_registrations(&mut b, ir);
    functions::generate_function_bindings(&mut b, ir);
    // Managed-FFI runtime helpers (host-invoker pattern) — must run after
    // `lib` is populated so the per-kind invoker registration can reach
    // `lib.AzApp_set<Kind>Invoker(...)`, and before wrappers reference
    // `registerCallback` / `refanyCreate`.
    managed::emit_managed(&mut b, ir);
    wrappers::generate_wrappers(&mut b, ir);
    emit_value_helpers(&mut b);
    emit_exports(&mut b, ir);

    Ok(b.finish())
}

/// AzOption / AzResult helpers exposed on the module so user code can do
/// `azul.optionToNullable(opt)` / `azul.resultUnwrap(res, 'Name')` without
/// having to know which variant koffi materialized. AzOption/AzResult
/// types don't get a per-type JS wrapper class (they're koffi unions), so
/// module-level helpers fill the ergonomic gap. Mirrors the per-type
/// `toNullable` / `Unwrap` methods that Java/Kotlin/C#/Ruby get.
fn emit_value_helpers(b: &mut CodeBuilder) {
    b.blank();
    b.line("// ----------------------------------------------------------------------------");
    b.line("// AzOption / AzResult helpers. Operate on koffi-decoded objects whose Ok /");
    b.line("// Some / Err / None members each carry a `tag` byte at offset 0 (shared via");
    b.line("// repr(C, u8)). Per-type methods aren't possible on koffi unions, so these");
    b.line("// expose the same affordance as Java's .toNullable() / .unwrap() but at");
    b.line("// module level.");
    b.line("// ----------------------------------------------------------------------------");
    b.line("function optionToNullable(opt) {");
    b.indent();
    b.line("if (!opt) return null;");
    b.line("// Tag byte lives in either variant (they overlap); prefer Some/None.");
    b.line("var tag = (opt.Some && opt.Some.tag) != null ? opt.Some.tag");
    b.line("        : (opt.None && opt.None.tag) != null ? opt.None.tag");
    b.line("        : null;");
    b.line("if (tag === 0 || tag == null) return null;");
    b.line("return opt.Some && opt.Some.payload;");
    b.dedent();
    b.line("}");
    b.blank();
    b.line("function resultUnwrap(res, label) {");
    b.indent();
    b.line("if (!res) throw new Error('unwrap on null');");
    b.line("var tag = (res.Ok && res.Ok.tag) != null ? res.Ok.tag");
    b.line("        : (res.Err && res.Err.tag) != null ? res.Err.tag");
    b.line("        : null;");
    b.line("if (tag === 0) return res.Ok.payload;");
    b.line("var name = label || 'Result';");
    b.line("var errPayload = res.Err && res.Err.payload;");
    b.line("throw new Error(name + ' unwrap on Err: ' + (errPayload && errPayload.toString ? errPayload.toString() : JSON.stringify(errPayload)));");
    b.dedent();
    b.line("}");
    b.blank();
    b.line("function resultIsOk(res) {");
    b.indent();
    b.line("if (!res) return false;");
    b.line("var tag = (res.Ok && res.Ok.tag) != null ? res.Ok.tag : (res.Err && res.Err.tag);");
    b.line("return tag === 0;");
    b.dedent();
    b.line("}");
    b.line("function resultIsErr(res) { return !resultIsOk(res); }");
}

fn emit_header(b: &mut CodeBuilder) {
    b.line("// ============================================================================");
    b.line("// azul.js -- JavaScript bindings for the Azul GUI framework.");
    b.line("// Generated by azul-doc codegen v2 (lang_node). DO NOT EDIT MANUALLY.");
    b.line("//");
    b.line("// Supported runtimes:");
    b.line("//   * Node.js >= 16 (uses the `koffi` package, https://koffi.dev/)");
    b.line("//   * Bun       >= 1.0 (uses built-in `bun:ffi`)");
    b.line("//   * Deno      >= 1.30 (uses built-in `Deno.dlopen`)");
    b.line("//");
    b.line("// The prebuilt native library (`libazul.so` on Linux, `libazul.dylib` on");
    b.line("// macOS, `azul.dll` on Windows) must be discoverable on the dynamic-loader");
    b.line("// search path (`LD_LIBRARY_PATH` / `DYLD_LIBRARY_PATH` / `PATH`) or placed");
    b.line("// in the same directory as this file. There is no native compile step.");
    b.line("//");
    b.line("// Modern JavaScript only: ES2020+, `class`, `FinalizationRegistry`,");
    b.line("// `const` / `let`, no `var`. Output module format is CommonJS for maximum");
    b.line("// compatibility; ESM consumers can `import` it via Node's CJS interop.");
    b.line("// ============================================================================");
    b.blank();
    b.line("'use strict';");
    b.blank();
}

fn emit_runtime_detection(b: &mut CodeBuilder) {
    b.line("// ----------------------------------------------------------------------------");
    b.line("// Runtime detection. Each branch sets up `loadLib` to return a uniform");
    b.line("// adapter: `{ call(symbol, argTypes, retType, args), proto(name, sig) }`.");
    b.line("// ----------------------------------------------------------------------------");
    b.blank();
    b.line("const isDeno = typeof globalThis.Deno !== 'undefined';");
    b.line("const isBun  = typeof globalThis.Bun  !== 'undefined';");
    b.line("const isNode = !isDeno && !isBun && typeof process !== 'undefined' && !!process.versions && !!process.versions.node;");
    b.blank();
    b.line("if (!isNode && !isBun && !isDeno) {");
    b.indent();
    b.line("throw new Error('azul.js requires Node.js >= 16, Bun >= 1.0, or Deno >= 1.30');");
    b.dedent();
    b.line("}");
    b.blank();
}

fn emit_load_lib(b: &mut CodeBuilder) {
    b.line("// ----------------------------------------------------------------------------");
    b.line("// loadLib(): runtime-specific shared-library loader. Returns an object");
    b.line("// shaped like:");
    b.line("//");
    b.line("//   {");
    b.line("//     // Register a struct/union/array type by name. Returns a koffi-style");
    b.line("//     // type handle on Node, or null on Bun/Deno (their FFI layers infer");
    b.line("//     // struct shape at the call site).");
    b.line("//     struct(name, fields), union(name, fields), array(elemType, length),");
    b.line("//     // Bind a C function symbol. Returns a callable JS function.");
    b.line("//     func(declOrSpec),");
    b.line("//     // Build a callback prototype (function-pointer type) for koffi");
    b.line("//     // (`koffi.proto`) / Bun (`JSCallback`) / Deno (`UnsafeCallback`).");
    b.line("//     proto(name, retType, argTypes),");
    b.line("//     // Wrap a JS function to a C-callable pointer of the given proto.");
    b.line("//     callback(proto, jsFn),");
    b.line("//     // Native pointer manipulation (deref / addressof) for wrappers.");
    b.line("//     ptr,  // 'pointer'-style sentinel for the runtime");
    b.line("//   }");
    b.line("// ----------------------------------------------------------------------------");
    b.blank();
    b.line(&format!("const DLL_NAME = '{}';", DLL_NAME));
    b.blank();

    // ---- Node / koffi branch ------------------------------------------------
    b.line("function loadNodeKoffi() {");
    b.indent();
    b.line("// koffi is a pure-JS libffi binding. It loads via require() and");
    b.line("// resolves the platform DLL name automatically (`azul` -> `azul.dll`");
    b.line("// / `libazul.so` / `libazul.dylib`).");
    b.line("const koffi = require('koffi');");
    b.line("const lib = koffi.load(DLL_NAME);");
    b.line("return {");
    b.indent();
    b.line("runtime: 'node-koffi',");
    b.line("koffi,");
    b.line("lib,");
    b.line("struct(name, fields) { return koffi.struct(name, fields); },");
    b.line("union(name, fields) { return koffi.union(name, fields); },");
    b.line("array(elemType, length) { return koffi.array(elemType, length); },");
    b.line("alias(name, target) { return koffi.alias(name, target); },");
    b.line("// Bind a function. The spec is `{ name, decl, parameters, returns }`.");
    b.line("// koffi accepts the full C declaration string (`decl`) directly.");
    b.line("func(spec) { return lib.func(spec.decl); },");
    b.line("// Function-pointer type. koffi.proto returns a type used for");
    b.line("// declaring callback parameters.");
    b.line("proto(name, retType, argTypes) {");
    b.indent();
    // koffi.proto's C-decl syntax is `'<retType> <name>(<argTypes,...>)'`,
    // matching the C function-pointer declaration form. The call returns
    // a callback-type handle; for use in `koffi.register(fn, type)` we
    // pass the registered name with a `*` suffix (koffi's pointer-to-callback form).
    b.line("koffi.proto(retType + ' ' + name + '(' + argTypes.join(',') + ')');");
    b.line("return name + ' *';");
    b.dedent();
    b.line("},");
    b.line("// Wrap a JS function as a C callback. koffi.register pins the");
    b.line("// trampoline for the JS function's lifetime (process-long here);");
    b.line("// callers must keep the returned handle alive themselves.");
    b.line("callback(proto, jsFn) { return koffi.register(jsFn, proto); },");
    b.line("// Address-of / pointer helpers for wrapper destructors.");
    b.line("addr(value) { return koffi.address(value); },");
    b.line("ptr: 'void *',");
    b.dedent();
    b.line("};");
    b.dedent();
    b.line("}");
    b.blank();

    // ---- Bun branch ---------------------------------------------------------
    b.line("function loadBun() {");
    b.indent();
    b.line("// `bun:ffi` ships in the Bun runtime; no install step. The dlopen");
    b.line("// call resolves library names the same way as koffi/Deno.");
    b.line("const { dlopen, FFIType, suffix, ptr, JSCallback } = require('bun:ffi');");
    b.line("const path = `${DLL_NAME}.${suffix}`;");
    b.line("// Bun requires the symbol map up-front. We populate it lazily by");
    b.line("// returning a builder that records bindings until the user is done,");
    b.line("// then reopens. This wastes a small amount of work but keeps the");
    b.line("// per-symbol binding API uniform across runtimes.");
    b.line("const pendingSymbols = {};");
    b.line("let opened = null;");
    b.line("function ensureOpen() {");
    b.indent();
    b.line("if (opened === null) {");
    b.indent();
    b.line("opened = dlopen(path, pendingSymbols).symbols;");
    b.dedent();
    b.line("}");
    b.line("return opened;");
    b.dedent();
    b.line("}");
    b.line("// Map a koffi-style type spec string to a Bun FFIType. Any non-primitive");
    b.line("// (registered struct, pointer, or unknown) collapses to FFIType.ptr.");
    b.line("function toBunType(spec) {");
    b.indent();
    b.line("if (typeof spec !== 'string') return FFIType.ptr;");
    b.line("switch (spec.trim()) {");
    b.indent();
    b.line("case 'void':     return FFIType.void;");
    b.line("case 'bool':     return FFIType.bool;");
    b.line("case 'int8_t':   return FFIType.i8;");
    b.line("case 'uint8_t':  return FFIType.u8;");
    b.line("case 'int16_t':  return FFIType.i16;");
    b.line("case 'uint16_t': return FFIType.u16;");
    b.line("case 'int32_t':  return FFIType.i32;");
    b.line("case 'uint32_t': return FFIType.u32;");
    b.line("case 'int64_t':  return FFIType.i64;");
    b.line("case 'uint64_t': return FFIType.u64;");
    b.line("case 'float':    return FFIType.f32;");
    b.line("case 'double':   return FFIType.f64;");
    b.line("case 'size_t':   return FFIType.u64;");
    b.line("case 'intptr_t': return FFIType.ptr;");
    b.line("default:         return FFIType.ptr;");
    b.dedent();
    b.line("}");
    b.dedent();
    b.line("}");
    b.line("return {");
    b.indent();
    b.line("runtime: 'bun',");
    b.line("FFIType,");
    b.line("// Bun's FFI infers struct shape at the call site via raw pointer");
    b.line("// passing; we record the shape for documentation purposes only.");
    b.line("struct(_name, _fields) { return null; },");
    b.line("union(_name, _fields) { return null; },");
    b.line("array(_elemType, _length) { return null; },");
    b.line("alias(_name, _target) { return null; },");
    b.line("func(spec) {");
    b.indent();
    b.line("// Spec shape: { name, decl, parameters, returns }.");
    b.line("// Bun's FFIType vocabulary differs from koffi's string-named types;");
    b.line("// any non-primitive (struct-by-value) collapses to FFIType.ptr at the");
    b.line("// FFI boundary. The wrapper layer carries the high-level shape.");
    b.line("const { name, parameters, returns } = spec;");
    b.line("pendingSymbols[name] = {");
    b.indent();
    b.line("args: parameters.map(toBunType),");
    b.line("returns: toBunType(returns),");
    b.dedent();
    b.line("};");
    b.line("opened = null;  // invalidate cached open");
    b.line("return (...args) => ensureOpen()[name](...args);");
    b.dedent();
    b.line("},");
    b.line("proto(_name, retType, argTypes) {");
    b.indent();
    b.line("return { _retType: toBunType(retType), _argTypes: argTypes.map(toBunType) };");
    b.dedent();
    b.line("},");
    b.line("callback(proto, jsFn) {");
    b.indent();
    b.line("return new JSCallback(jsFn, { args: proto._argTypes, returns: proto._retType });");
    b.dedent();
    b.line("},");
    b.line("addr(value) { return ptr(value); },");
    b.line("ptr: FFIType.ptr,");
    b.dedent();
    b.line("};");
    b.dedent();
    b.line("}");
    b.blank();

    // ---- Deno branch --------------------------------------------------------
    b.line("function loadDeno() {");
    b.indent();
    b.line("// Deno.dlopen requires a symbol map at open time. We use the same");
    b.line("// lazy-symbol-map pattern as Bun.");
    b.line("const Deno = globalThis.Deno;");
    b.line("// Resolve the platform-specific filename. Deno does NOT auto-resolve");
    b.line("// `'azul'` to `libazul.so` etc.: the caller must spell it out.");
    b.line("const platform = Deno.build.os;");
    b.line("const libPath = platform === 'windows'");
    b.indent();
    b.line("? `${DLL_NAME}.dll`");
    b.line(": platform === 'darwin' ? `lib${DLL_NAME}.dylib` : `lib${DLL_NAME}.so`;");
    b.dedent();
    b.line("const pendingSymbols = {};");
    b.line("let opened = null;");
    b.line("function ensureOpen() {");
    b.indent();
    b.line("if (opened === null) {");
    b.indent();
    b.line("opened = Deno.dlopen(libPath, pendingSymbols).symbols;");
    b.dedent();
    b.line("}");
    b.line("return opened;");
    b.dedent();
    b.line("}");
    b.line("// Map a koffi-style type spec string to Deno's NativeType. Any");
    b.line("// non-primitive (registered struct, pointer, unknown) collapses to");
    b.line("// 'pointer' — wrappers carry the high-level shape.");
    b.line("function toDenoType(spec) {");
    b.indent();
    b.line("if (typeof spec !== 'string') return 'pointer';");
    b.line("switch (spec.trim()) {");
    b.indent();
    b.line("case 'void':     return 'void';");
    b.line("case 'bool':     return 'bool';");
    b.line("case 'int8_t':   return 'i8';");
    b.line("case 'uint8_t':  return 'u8';");
    b.line("case 'int16_t':  return 'i16';");
    b.line("case 'uint16_t': return 'u16';");
    b.line("case 'int32_t':  return 'i32';");
    b.line("case 'uint32_t': return 'u32';");
    b.line("case 'int64_t':  return 'i64';");
    b.line("case 'uint64_t': return 'u64';");
    b.line("case 'float':    return 'f32';");
    b.line("case 'double':   return 'f64';");
    b.line("case 'size_t':   return 'usize';");
    b.line("case 'intptr_t': return 'pointer';");
    b.line("default:         return 'pointer';");
    b.dedent();
    b.line("}");
    b.dedent();
    b.line("}");
    b.line("return {");
    b.indent();
    b.line("runtime: 'deno',");
    b.line("struct(_name, _fields) { return null; },");
    b.line("union(_name, _fields) { return null; },");
    b.line("array(_elemType, _length) { return null; },");
    b.line("alias(_name, _target) { return null; },");
    b.line("func(spec) {");
    b.indent();
    b.line("const { name, parameters, returns } = spec;");
    b.line("pendingSymbols[name] = {");
    b.indent();
    b.line("parameters: parameters.map(toDenoType),");
    b.line("result: toDenoType(returns),");
    b.dedent();
    b.line("};");
    b.line("opened = null;");
    b.line("return (...args) => ensureOpen()[name](...args);");
    b.dedent();
    b.line("},");
    b.line("proto(_name, retType, argTypes) {");
    b.indent();
    b.line("return { parameters: argTypes.map(toDenoType), result: toDenoType(retType) };");
    b.dedent();
    b.line("},");
    b.line("callback(proto, jsFn) {");
    b.indent();
    b.line("return new Deno.UnsafeCallback(proto, jsFn);");
    b.dedent();
    b.line("},");
    b.line("addr(value) { return Deno.UnsafePointer.of(value); },");
    b.line("ptr: 'pointer',");
    b.dedent();
    b.line("};");
    b.dedent();
    b.line("}");
    b.blank();

    b.line("const azulFFI = isNode ? loadNodeKoffi() : isBun ? loadBun() : loadDeno();");
    b.blank();
}

fn emit_exports(b: &mut CodeBuilder, ir: &CodegenIR) {
    b.blank();
    b.line("// ----------------------------------------------------------------------------");
    b.line("// CommonJS exports. ESM consumers receive the same object via Node's");
    b.line("// CJS interop (`import azul from 'azul'`).");
    b.line("// ----------------------------------------------------------------------------");
    b.blank();
    b.line("module.exports = {");
    b.indent();
    b.line("// Runtime banner — useful for sanity-checking which FFI layer is in use.");
    b.line("__runtime: azulFFI.runtime,");
    b.line("// Raw FFI handle for power users who want to call unwrapped symbols.");
    b.line("__ffi: azulFFI,");
    b.line("// Raw `lib` object for direct access to C-ABI symbols (advanced).");
    b.line("__lib: lib,");
    b.line("// Managed-FFI runtime helpers (host-invoker pattern). User callbacks");
    b.line("// pass through `registerCallback(kind, fn)`; arbitrary user data goes");
    b.line("// through `refanyCreate(value)` + `refanyGet(refany)`.");
    b.line("registerCallback,");
    b.line("refanyCreate,");
    b.line("refanyGet,");
    b.line("// AzOption / AzResult ergonomic helpers.");
    b.line("optionToNullable,");
    b.line("resultUnwrap,");
    b.line("resultIsOk,");
    b.line("resultIsErr,");
    // List wrapper class names
    for s in &ir.structs {
        if !wrappers::should_emit_struct(s) {
            continue;
        }
        b.line(&format!("{},", sanitize_export_name(&s.name)));
    }
    for e in &ir.enums {
        if !wrappers::should_emit_enum(e) {
            continue;
        }
        b.line(&format!("{},", sanitize_export_name(&e.name)));
    }
    b.dedent();
    b.line("};");
}

// ============================================================================
// Shared naming helpers
// ============================================================================

/// FFI / koffi type name for an IR type. We keep the `Az` prefix on
/// the FFI side so the registered koffi type matches the C header,
/// the same way other bindings preserve `AzApp` / `AzDom` etc.
pub fn ffi_type_name(name: &str) -> String {
    format!("Az{}", name)
}

/// Public JS export name. We drop the `Az` prefix on the wrapper layer
/// so users write `new App(...)` rather than `new AzApp(...)`. Reserved
/// JS identifiers get an underscore suffix.
pub fn sanitize_export_name(name: &str) -> String {
    let s = name.to_string();
    if is_js_reserved(&s) {
        format!("{}_", s)
    } else {
        s
    }
}

/// Sanitize an identifier for use as a JS field / parameter name.
/// Reserved word collisions get a trailing underscore.
pub fn sanitize_js_identifier(name: &str) -> String {
    if is_js_reserved(name) {
        format!("{}_", name)
    } else {
        name.to_string()
    }
}

fn is_js_reserved(s: &str) -> bool {
    matches!(
        s,
        "break"
            | "case"
            | "catch"
            | "class"
            | "const"
            | "continue"
            | "debugger"
            | "default"
            | "delete"
            | "do"
            | "else"
            | "enum"
            | "export"
            | "extends"
            | "false"
            | "finally"
            | "for"
            | "function"
            | "if"
            | "import"
            | "in"
            | "instanceof"
            | "new"
            | "null"
            | "return"
            | "super"
            | "switch"
            | "this"
            | "throw"
            | "true"
            | "try"
            | "typeof"
            | "var"
            | "void"
            | "while"
            | "with"
            | "yield"
            | "let"
            | "static"
            | "implements"
            | "interface"
            | "package"
            | "private"
            | "protected"
            | "public"
            | "await"
            | "async"
    )
}

/// Map a Rust/IR type name to the koffi type-spec string used inside
/// struct field declarations and function-binding parameters. koffi
/// accepts a small library of primitive names (`int32_t`, `uint8_t`,
/// `void *`, etc.) plus any user-registered struct names.
pub fn map_type_to_koffi(rust_type: &str, ir: &CodegenIR) -> String {
    let trimmed = rust_type.trim();

    // Pointer types collapse to `void *` at the koffi spec level.
    // The wrapper layer carries the type information in JS-land.
    if trimmed.starts_with("*const ")
        || trimmed.starts_with("*mut ")
        || trimmed.starts_with("&mut ")
        || trimmed.starts_with('&')
    {
        return "void *".to_string();
    }

    match trimmed {
        "bool" => "bool".to_string(),
        "u8" | "c_uchar" => "uint8_t".to_string(),
        "i8" | "c_char" | "char" => "int8_t".to_string(),
        "u16" => "uint16_t".to_string(),
        "i16" => "int16_t".to_string(),
        "u32" | "c_uint" => "uint32_t".to_string(),
        "i32" | "c_int" => "int32_t".to_string(),
        "u64" => "uint64_t".to_string(),
        "i64" => "int64_t".to_string(),
        "f32" => "float".to_string(),
        "f64" => "double".to_string(),
        "usize" => "size_t".to_string(),
        "isize" => "intptr_t".to_string(),
        "c_void" | "()" | "void" => "void".to_string(),

        // Anything else: treat as registered struct/enum if known, else
        // opaque pointer.
        _ => {
            // Type aliases: monomorphized ones are koffi-registered as
            // concrete types. Simple aliases (e.g. `HwndHandle = *mut c_void`,
            // `GLuint = u32`) follow through to the target since they're
            // never registered as koffi types of their own.
            if let Some(ta) = ir.find_type_alias(trimmed) {
                if ta.monomorphized_def.is_none() {
                    let resolved = if ta.target.starts_with("*mut ")
                        || ta.target.starts_with("*const ")
                        || ta.target.starts_with('&')
                    {
                        "void *".to_string()
                    } else {
                        map_type_to_koffi(&ta.target, ir)
                    };
                    return resolved;
                }
                return ffi_type_name(trimmed);
            }
            // Recursive types collapse to `void *` in field positions —
            // koffi can't expand them inline.
            if let Some(s) = ir.find_struct(trimmed) {
                if matches!(s.category, super::super::ir::TypeCategory::Recursive) {
                    return "void *".to_string();
                }
                return ffi_type_name(trimmed);
            }
            if let Some(e) = ir.find_enum(trimmed) {
                if matches!(e.category, super::super::ir::TypeCategory::Recursive) {
                    return "void *".to_string();
                }
                return ffi_type_name(trimmed);
            }
            if ir.callback_typedefs.iter().any(|c| c.name == trimmed) {
                ffi_type_name(trimmed)
            } else {
                "void *".to_string()
            }
        }
    }
}
