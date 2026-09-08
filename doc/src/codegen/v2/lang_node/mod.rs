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

/// The Deno adapter: `Deno.dlopen` plus a by-value type layer that
/// encodes/decodes the wrapper layer's plain-object struct values to the
/// raw bytes Deno marshals. Emitted verbatim (no indentation pass).
const DENO_ADAPTER_JS: &str = r#"
function loadDeno() {
    // Deno.dlopen needs the whole symbol map at open time; bind() records
    // symbols and the first call opens the library. Deno does no library
    // name mangling either: DLL_NAME is already a resolved path or a full
    // platform filename from _resolveDllPath().
    const Deno = globalThis.Deno;
    const libPath = DLL_NAME;
    const pendingSymbols = {};
    let opened = null;
    function ensureOpen() {
        if (opened === null) {
            opened = Deno.dlopen(libPath, pendingSymbols).symbols;
        }
        return opened;
    }

    // ---- by-value type layer -----------------------------------------------
    // Deno marshals a struct parameter or return as raw bytes (Uint8Array)
    // laid out per the C ABI and described by a nested `{ struct: [...] }`
    // descriptor. The wrapper layer holds struct values as plain JS objects
    // keyed by field name (the shape koffi decodes to). This table bridges
    // the two: every registered struct/union/alias gets its C layout
    // (size, alignment, field offsets), a Deno descriptor, and
    // encode/decode routines. Little-endian on every supported target.
    const PRIM = {
        'void':     { kind: 'prim', size: 0, align: 1, native: 'void' },
        'bool':     { kind: 'prim', size: 1, align: 1, native: 'bool',
                      get: (dv, o) => dv.getUint8(o) !== 0, set: (dv, o, v) => dv.setUint8(o, v ? 1 : 0) },
        'int8_t':   { kind: 'prim', size: 1, align: 1, native: 'i8',
                      get: (dv, o) => dv.getInt8(o), set: (dv, o, v) => dv.setInt8(o, Number(v)) },
        'uint8_t':  { kind: 'prim', size: 1, align: 1, native: 'u8',
                      get: (dv, o) => dv.getUint8(o), set: (dv, o, v) => dv.setUint8(o, Number(v)) },
        'int16_t':  { kind: 'prim', size: 2, align: 2, native: 'i16',
                      get: (dv, o) => dv.getInt16(o, true), set: (dv, o, v) => dv.setInt16(o, Number(v), true) },
        'uint16_t': { kind: 'prim', size: 2, align: 2, native: 'u16',
                      get: (dv, o) => dv.getUint16(o, true), set: (dv, o, v) => dv.setUint16(o, Number(v), true) },
        'int32_t':  { kind: 'prim', size: 4, align: 4, native: 'i32',
                      get: (dv, o) => dv.getInt32(o, true), set: (dv, o, v) => dv.setInt32(o, Number(v), true) },
        'uint32_t': { kind: 'prim', size: 4, align: 4, native: 'u32',
                      get: (dv, o) => dv.getUint32(o, true), set: (dv, o, v) => dv.setUint32(o, Number(v), true) },
        'int64_t':  { kind: 'prim', size: 8, align: 8, native: 'i64',
                      get: (dv, o) => dv.getBigInt64(o, true), set: (dv, o, v) => dv.setBigInt64(o, BigInt(v), true) },
        'uint64_t': { kind: 'prim', size: 8, align: 8, native: 'u64',
                      get: (dv, o) => dv.getBigUint64(o, true), set: (dv, o, v) => dv.setBigUint64(o, BigInt(v), true) },
        'float':    { kind: 'prim', size: 4, align: 4, native: 'f32',
                      get: (dv, o) => dv.getFloat32(o, true), set: (dv, o, v) => dv.setFloat32(o, Number(v), true) },
        'double':   { kind: 'prim', size: 8, align: 8, native: 'f64',
                      get: (dv, o) => dv.getFloat64(o, true), set: (dv, o, v) => dv.setFloat64(o, Number(v), true) },
        'size_t':   { kind: 'prim', size: 8, align: 8, native: 'usize',
                      get: (dv, o) => Number(dv.getBigUint64(o, true)), set: (dv, o, v) => dv.setBigUint64(o, BigInt(v), true) },
        'intptr_t': { kind: 'prim', size: 8, align: 8, native: 'isize',
                      get: (dv, o) => Number(dv.getBigInt64(o, true)), set: (dv, o, v) => dv.setBigInt64(o, BigInt(v), true) },
    };
    const PTR = { kind: 'pointer', size: 8, align: 8, native: 'pointer' };
    const types = Object.create(null);

    function resolve(spec) {
        if (typeof spec !== 'string') throw new Error('azul.js: bad FFI type spec ' + spec);
        const s = spec.trim();
        if (s.endsWith('*')) return PTR;
        const p = PRIM[s];
        if (p) return p;
        const t = types[s];
        if (t) return t;
        throw new Error("azul.js: unregistered FFI type '" + s + "'");
    }
    function alignUp(n, a) { return (n + a - 1) & ~(a - 1); }
    function isAggregate(t) { return t.kind === 'struct' || t.kind === 'union'; }
    // Descriptor usable as a field: `bool` is a byte inside an aggregate.
    function fieldNative(t) { return t === PRIM.bool ? 'u8' : t.native; }
    // Descriptor usable as a parameter/return: an aggregate is always a
    // `{ struct }` so Deno marshals it as bytes even when a union's
    // representative member is a scalar.
    function paramNative(t) {
        if (isAggregate(t) && typeof t.native === 'string') return { struct: [t.native] };
        return t.native;
    }
    // libffi has no union type. A member whose size and alignment equal
    // the union's own reproduces the union's register classification on
    // every supported ABI whenever the other members are shorter (the
    // repr(C, u8) tagged enums: the largest variant carries the tag byte
    // every variant shares). Otherwise an integer fill of the union's
    // size/alignment keeps the layout of the enclosing struct exact.
    function unionNative(t) {
        for (const m of t.members) {
            if (m.type.size === t.size && m.type.align === t.align) return fieldNative(m.type);
        }
        const unit = t.align >= 8 ? 'u64' : t.align === 4 ? 'u32' : t.align === 2 ? 'u16' : 'u8';
        return { struct: new Array(t.size / t.align).fill(unit) };
    }
    function define(name, fields, isUnion) {
        const members = [];
        let size = 0, align = 1;
        for (const key of Object.keys(fields)) {
            const t = resolve(fields[key]);
            if (isUnion) {
                members.push({ name: key, type: t, offset: 0 });
                if (t.size > size) size = t.size;
            } else {
                const off = alignUp(size, t.align);
                members.push({ name: key, type: t, offset: off });
                size = off + t.size;
            }
            if (t.align > align) align = t.align;
        }
        size = alignUp(size, align);
        const t = { kind: isUnion ? 'union' : 'struct', name, members, size, align, native: null };
        t.native = isUnion ? unionNative(t) : { struct: members.map((m) => fieldNative(m.type)) };
        types[name] = t;
        return t;
    }

    function ptrValue(v) {
        if (v == null) return 0n;
        if (typeof v === 'bigint') return v;
        if (typeof v === 'number') return BigInt(v);
        if (v instanceof Deno.UnsafeCallback) v = v.pointer;
        else if (v instanceof Uint8Array || v instanceof ArrayBuffer) v = Deno.UnsafePointer.of(v);
        const pv = Deno.UnsafePointer.value(v);
        return typeof pv === 'bigint' ? pv : BigInt(pv);
    }
    function bytesAt(dv, off, len) { return new Uint8Array(dv.buffer, dv.byteOffset + off, len); }

    // JS value -> native bytes at `off`. Wrapper instances unwrap to their
    // `_ptr` value; a Uint8Array of the type's size is copied verbatim.
    function encode(t, v, dv, off) {
        if (v && typeof v === 'object' && v._ptr !== undefined) v = v._ptr;
        if (t.kind === 'pointer') { dv.setBigUint64(off, ptrValue(v), true); return; }
        if (t.set) { t.set(dv, off, v == null ? 0 : v); return; }
        if (v == null) return;
        if (v instanceof Uint8Array) { bytesAt(dv, off, t.size).set(v.subarray(0, t.size)); return; }
        if (t.kind === 'struct') {
            for (const m of t.members) {
                const fv = v[m.name];
                if (fv !== undefined) encode(m.type, fv, dv, off + m.offset);
            }
            return;
        }
        // Union. A decoded union remembers its source bytes and which member
        // the user assigned; a plain object encodes whichever members it has.
        const st = v.__azUnionState;
        if (st) {
            bytesAt(dv, off, t.size).set(v.__azBytes);
            if (st.active !== null) {
                for (const m of t.members) if (m.name === st.active) encode(m.type, st.cache[m.name], dv, off);
                return;
            }
            for (const m of t.members) if (m.name in st.cache) encode(m.type, st.cache[m.name], dv, off);
            return;
        }
        for (const m of t.members) if (v[m.name] !== undefined) encode(m.type, v[m.name], dv, off);
    }
    // Native bytes at `off` -> JS value (plain object for aggregates).
    function decode(t, dv, off) {
        if (t.kind === 'pointer') {
            const a = dv.getBigUint64(off, true);
            return a === 0n ? null : Deno.UnsafePointer.create(a);
        }
        if (t.get) return t.get(dv, off);
        if (t.kind === 'struct') {
            const obj = {};
            Object.defineProperty(obj, '__azType', { value: t.name });
            for (const m of t.members) obj[m.name] = decode(m.type, dv, off + m.offset);
            return obj;
        }
        return decodeUnion(t, dv, off);
    }
    // Every member of a union decodes from the same bytes, lazily; a
    // member assignment marks it as the one to encode back.
    function decodeUnion(t, dv, off) {
        const bytes = new Uint8Array(t.size);
        bytes.set(bytesAt(dv, off, t.size));
        const bdv = new DataView(bytes.buffer);
        const st = { cache: Object.create(null), active: null };
        const obj = {};
        Object.defineProperty(obj, '__azType', { value: t.name });
        Object.defineProperty(obj, '__azBytes', { value: bytes });
        Object.defineProperty(obj, '__azUnionState', { value: st });
        for (const m of t.members) {
            Object.defineProperty(obj, m.name, {
                enumerable: true,
                get() {
                    if (!(m.name in st.cache)) st.cache[m.name] = decode(m.type, bdv, 0);
                    return st.cache[m.name];
                },
                set(v) { st.cache[m.name] = v; st.active = m.name; },
            });
        }
        return obj;
    }
    // Copy a `T *` argument back into the caller's object after the call,
    // so a `&mut self` method's writes are visible to the wrapper.
    function decodeInto(t, dv, off, target) {
        if (!target || typeof target !== 'object') return;
        const st = target.__azUnionState;
        if (st) {
            target.__azBytes.set(bytesAt(dv, off, t.size));
            for (const k of Object.keys(st.cache)) delete st.cache[k];
            st.active = null;
        } else if (t.kind === 'struct') {
            for (const m of t.members) target[m.name] = decode(m.type, dv, off + m.offset);
        }
    }
    function toBytes(t, v) {
        const buf = new Uint8Array(t.size);
        encode(t, v, new DataView(buf.buffer), 0);
        return buf;
    }
    // Argument conversion for a `pointer` parameter: raw pointers pass
    // through, buffers/callbacks give their address, a decoded aggregate
    // is encoded into a scratch buffer (copied back after the call).
    function toPointerArg(v, copyBack) {
        if (v == null) return null;
        if (v instanceof Deno.UnsafeCallback) return v.pointer;
        if (typeof v === 'bigint' || typeof v === 'number') return Deno.UnsafePointer.create(BigInt(v));
        if (v instanceof Uint8Array || v instanceof ArrayBuffer) return Deno.UnsafePointer.of(v);
        if (typeof v === 'object' && v.__azType) {
            const vt = types[v.__azType];
            const buf = toBytes(vt, v);
            copyBack.push([vt, buf, v]);
            return Deno.UnsafePointer.of(buf);
        }
        return v;
    }
    function bind(name, paramSpecs, retSpec) {
        const ptypes = paramSpecs.map(resolve);
        const rtype = resolve(retSpec);
        pendingSymbols[name] = { parameters: ptypes.map(paramNative), result: paramNative(rtype) };
        opened = null;
        return (...args) => {
            const sym = ensureOpen()[name];
            const conv = new Array(ptypes.length);
            const copyBack = [];
            for (let i = 0; i < ptypes.length; i++) {
                const t = ptypes[i];
                let v = args[i];
                if (v && typeof v === 'object' && v._ptr !== undefined) v = v._ptr;
                if (isAggregate(t)) {
                    conv[i] = (v instanceof Uint8Array && v.length === t.size) ? v : toBytes(t, v);
                } else if (t.kind === 'pointer') {
                    conv[i] = toPointerArg(v, copyBack);
                } else {
                    conv[i] = v;
                }
            }
            const ret = sym(...conv);
            for (const [vt, buf, v] of copyBack) decodeInto(vt, new DataView(buf.buffer), 0, v);
            if (isAggregate(rtype)) return decode(rtype, new DataView(ret.buffer, ret.byteOffset, ret.byteLength), 0);
            return ret;
        };
    }
    // libazul keeps every callback pointer it is handed for the life of
    // the process; the trampolines must never be collected or closed.
    const liveCallbacks = [];

    return {
        runtime: 'deno',
        types,
        struct(name, fields) { return define(name, fields, false); },
        union(name, fields) { return define(name, fields, true); },
        array(_elemType, _length) { return null; },
        alias(name, target) { types[name] = resolve(target); return types[name]; },
        func(spec) { return bind(spec.name, spec.parameters, spec.returns); },
        proto(_name, retType, argTypes) {
            return { parameters: argTypes.map((s) => paramNative(resolve(s))), result: paramNative(resolve(retType)) };
        },
        callback(proto, jsFn) {
            const cb = new Deno.UnsafeCallback(proto, jsFn);
            liveCallbacks.push(cb);
            return cb;
        },
        addr(value) { return Deno.UnsafePointer.of(value); },
        // Write an int32 through an out-pointer (callback return writeback).
        // `UnsafePointerView.getArrayBuffer(len)` is a direct mutable view
        // over the native memory at the pointer.
        writeInt32(p, v) { new DataView(new Deno.UnsafePointerView(p).getArrayBuffer(4)).setInt32(0, v, true); },
        encodeInto(p, typeName, value) {
            const t = resolve(typeName);
            encode(t, value, new DataView(new Deno.UnsafePointerView(p).getArrayBuffer(t.size)), 0);
        },
        readBytes(p, len) { return new Uint8Array(new Deno.UnsafePointerView(p).getArrayBuffer(len)).slice(); },
        ptr: 'pointer',
    };
}

"#;

/// Public entry point. Returns a multi-file string with two sections:
/// `azul.js` and `package.json`.
pub fn generate(ir: &CodegenIR, config: &CodegenConfig) -> Result<String> {
    let azul_js = generate_azul_js(ir, config)?;
    let pkg_json = package_json::generate_package_json(&ir.api_version);

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
    b.line("// macOS, `azul.dll` on Windows) is resolved in this order:");
    b.line("//   1. $AZ_LIB      — explicit path to the shared-library file,");
    b.line("//   2. the directory containing this azul.js file,");
    b.line("//   3. $AZ_LIB_DIR  — directory containing the shared library,");
    b.line("//   4. the current working directory,");
    b.line("//   5. the system loader search path (LD_LIBRARY_PATH /");
    b.line("//      DYLD_LIBRARY_PATH / PATH / rpath).");
    b.line("// There is no native compile step.");
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
    // dlopen does NO lib-prefix/suffix mangling on any of the three runtimes
    // (verified empirically for koffi, `bun:ffi` and `Deno.dlopen`): a bare
    // "azul" is passed straight through and never matches `libazul.dylib` /
    // `libazul.so` / `azul.dll` on disk — no matter what `DYLD_LIBRARY_PATH`
    // / `LD_LIBRARY_PATH` / `PATH` say. So we derive the platform filename
    // ourselves and probe the well-known locations, in order:
    //
    //   1. `AZ_LIB`      — explicit path to the shared-library *file*
    //                       (the AZ_E2E harness hook); used verbatim.
    //   2. next to azul.js (npm-style: drop the dylib beside the binding).
    //   3. `AZ_LIB_DIR`  — *directory* containing the shared library.
    //   4. the current working directory (the documented download-and-run flow).
    //   5. the bare platform filename, so the system loader search path
    //      (`LD_LIBRARY_PATH` / `DYLD_LIBRARY_PATH` / `PATH` / rpath) still applies.
    b.line("// Derive the platform shared-library filename: azul -> azul.dll /");
    b.line("// libazul.so / libazul.dylib. dlopen does NOT do this mangling itself.");
    b.line("function _platformLibName(base) {");
    b.indent();
    b.line("const os = (typeof process !== 'undefined' && process.platform)");
    b.indent();
    b.line("? process.platform");
    b.line(": (typeof globalThis.Deno !== 'undefined' && globalThis.Deno.build.os === 'windows') ? 'win32'");
    b.line(": (typeof globalThis.Deno !== 'undefined' && globalThis.Deno.build.os === 'darwin') ? 'darwin'");
    b.line(": 'linux';");
    b.dedent();
    b.line("if (os === 'win32') return base + '.dll';");
    b.line("if (os === 'darwin') return 'lib' + base + '.dylib';");
    b.line("return 'lib' + base + '.so';");
    b.dedent();
    b.line("}");
    b.blank();
    b.line("function _libFileExists(p) {");
    b.indent();
    b.line("try {");
    b.indent();
    b.line("if (typeof require === 'function') return require('fs').existsSync(p);");
    b.line(
        "if (typeof globalThis.Deno !== 'undefined') { globalThis.Deno.statSync(p); return true; }",
    );
    b.dedent();
    b.line("} catch (_e) { /* unreadable or missing — keep probing */ }");
    b.line("return false;");
    b.dedent();
    b.line("}");
    b.blank();
    b.line("function _resolveDllPath() {");
    b.indent();
    b.line("const env = (typeof process !== 'undefined' && process.env) ? process.env : {};");
    b.line("// 1) AZ_LIB: explicit path to the shared-library file. Used verbatim.");
    b.line("if (env.AZ_LIB) return env.AZ_LIB;");
    b.line(&format!(
        "const fileName = _platformLibName('{}');",
        DLL_NAME
    ));
    b.line("const candidates = [];");
    b.line("// 2) Same directory as azul.js (npm-style: dylib next to the binding).");
    b.line("if (typeof __dirname !== 'undefined') candidates.push(__dirname + '/' + fileName);");
    b.line("// 3) AZ_LIB_DIR: directory that contains the shared library.");
    b.line("if (env.AZ_LIB_DIR) candidates.push(env.AZ_LIB_DIR + '/' + fileName);");
    b.line("// 4) Current working directory (the documented download-and-run flow).");
    b.line("if (typeof process !== 'undefined' && typeof process.cwd === 'function') {");
    b.indent();
    b.line("candidates.push(process.cwd() + '/' + fileName);");
    b.dedent();
    b.line("}");
    b.line("for (const c of candidates) { if (_libFileExists(c)) return c; }");
    b.line("// 5) Bare platform filename: lets the system loader search path");
    b.line("//    (LD_LIBRARY_PATH / DYLD_LIBRARY_PATH / PATH / rpath) resolve it.");
    b.line("return fileName;");
    b.dedent();
    b.line("}");
    b.blank();
    b.line("const DLL_NAME = _resolveDllPath();");
    b.blank();

    // ---- Node / koffi branch ------------------------------------------------
    b.line("function loadNodeKoffi() {");
    b.indent();
    b.line("// koffi is a pure-JS libffi binding. DLL_NAME was already resolved");
    b.line("// to a concrete platform filename or path by _resolveDllPath()");
    b.line("// above — koffi.load passes the string straight to dlopen and does");
    b.line("// no lib-prefix/suffix mangling of its own.");
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
    b.line("// Write an int32 through an out-pointer (callback return writeback;");
    b.line("// same primitive on all three runtimes so the invoker layer can");
    b.line("// write enum returns without a runtime gate).");
    b.line("writeInt32(p, v) { koffi.encode(p, 'int32_t', v); },");
    b.line("// Encode a by-value struct/union into native memory (callback");
    b.line("// struct-return writeback). Same shape on every runtime that can");
    b.line("// marshal aggregates, so the invoker layer needs no runtime gate.");
    b.line("encodeInto(p, typeName, value) { koffi.encode(p, typeName, value); },");
    b.line("// Copy `len` bytes out of native memory (AzString decode).");
    b.line("readBytes(p, len) { return koffi.decode(p, 'uint8_t', len); },");
    b.line("ptr: 'void *',");
    b.dedent();
    b.line("};");
    b.dedent();
    b.line("}");
    b.blank();

    // ---- Bun branch ---------------------------------------------------------
    b.line("function loadBun() {");
    b.indent();
    b.line("// `bun:ffi` ships in the Bun runtime; no install step. Bun's dlopen");
    b.line("// does no name mangling either — DLL_NAME is already a resolved");
    b.line("// path or a full platform filename (lib prefix + suffix included),");
    b.line("// so use it verbatim.");
    b.line("const { dlopen, FFIType, ptr, JSCallback, toArrayBuffer } = require('bun:ffi');");
    b.line("const path = DLL_NAME;");
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
    b.line("// Write an int32 through an out-pointer (callback return writeback).");
    b.line("// `toArrayBuffer(p, 0, 4)` maps the pointed-at native memory as a");
    b.line("// mutable ArrayBuffer view — DataView writes go straight through.");
    b.line("// Little-endian: all supported targets (x86_64/aarch64) are LE.");
    b.line("writeInt32(p, v) { new DataView(toArrayBuffer(p, 0, 4)).setInt32(0, v, true); },");
    b.line("ptr: FFIType.ptr,");
    b.dedent();
    b.line("};");
    b.dedent();
    b.line("}");
    b.blank();

    // ---- Deno branch --------------------------------------------------------
    b.raw(DENO_ADAPTER_JS);

    b.line("// Bun runs koffi through its Node-API layer (`bun add koffi`), which");
    b.line("// marshals every by-value struct the C API uses; the `bun:ffi` adapter");
    b.line("// stays as the fallback for a Bun install without koffi.");
    b.line("function _koffiResolves() {");
    b.indent();
    b.line("try { require.resolve('koffi'); return true; } catch (_e) { return false; }");
    b.dedent();
    b.line("}");
    b.blank();
    b.line("const azulFFI = isDeno ? loadDeno()");
    b.line("    : (isNode || (isBun && _koffiResolves())) ? loadNodeKoffi()");
    b.line("    : loadBun();");
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
    b.line("// Auto-AzString-conversion helper (referenced by hello-world).");
    b.line("_azString,");
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
