//! Go binding generator (cgo).
//!
//! Emits a Go package (`package azul`) plus a `go.mod` manifest that
//! exposes `libazul`'s C-ABI to Go programs through cgo — one statically
//! linked binary, no dlopen.
//!
//! # Strategy: Go-native types, cgo only for the calls
//!
//! cgo's first gcc probe writes five deliberately failing C functions for
//! every `C.` name a file references; gcc's error-recovery path grows
//! super-linearly with that count (2 s at 1.1k names, 6 s at 5.4k, 356 s
//! at 9.4k). The binding used to reference every C type (`C.AzFoo`) AND
//! every function — ~18k names. It now references no C type at all:
//!
//! 1. `types.go`  — Go-native definitions of every api.json type with the
//!                  exact `repr(C)` layout (see `types.rs`), no cgo.
//! 2. `functions*.go` — the raw call layer, one Go function per C export,
//!                  named like the C symbol. Owned aggregates cross through
//!                  the exported `*Byref` twins by pointer; everything is
//!                  declared with `void*`/primitive prototypes so no C type
//!                  is named (see `functions.rs`). Chunked into files of
//!                  at most `functions::CHUNK` names because the probe cost
//!                  is per file.
//! 3. `wrappers.go` — idiomatic wrappers (`type App struct { inner *AzApp }`,
//!                  constructors, methods, `Close() error`), no cgo.
//! 4. `callbacks.go` / `callbacks_export.go` — host-invoker callback layer
//!                  (`//export` trampolines); its preamble includes azul.h
//!                  for a handful of static shims, but the Go side names
//!                  only shim functions and `C.uint64_t`.
//! 5. `azul.go`   — package doc + the `#cgo LDFLAGS` directive.
//! 6. `go.mod`    — `module azul.rs/ui/go` + Go 1.21 directive.
//!
//! # Build-time requirements (cgo)
//!
//!   * a C compiler (`gcc` / `clang` / MinGW) on the host,
//!   * `azul.h` on the C include path (`CGO_CFLAGS=-I...`) — the shim and
//!     callback preambles include it,
//!   * `libazul.{so,dylib}` (or `azul.dll`) on the linker path
//!     (`CGO_LDFLAGS=-L...`) and reachable at runtime.
//!
//! # Output protocol
//!
//! `generate(ir, config)` returns a single concatenated `String` with
//! per-file sections separated by [`FILE_MARKER`]. The marker is a
//! syntactically valid Go line comment (`// ==FILE: <path> ==`). The
//! orchestrator splits on the marker and writes each chunk to its
//! relative path under `target/codegen/go/`.

pub mod functions;
pub mod gomod;
pub mod managed;
pub mod types;
pub mod wrappers;

use anyhow::Result;

use super::config::CodegenConfig;
use super::generator::CodeBuilder;
use super::ir::CodegenIR;

/// File-marker header that introduces each per-file section in the
/// concatenated output. The orchestrator splits on lines that start
/// with this prefix.
pub const FILE_MARKER: &str = "// ==FILE: ";

/// Trailing marker that closes the file-marker header line.
pub const END_MARKER: &str = " ==";

/// Library name passed to the linker via `// #cgo LDFLAGS: -lazul`.
/// Must match the prebuilt artifact (`libazul.so` / `libazul.dylib` /
/// `azul.dll`).
pub const LIB_NAME: &str = "azul";

/// Public entry point. Generates the multi-file Go binding concatenated
/// into a single `String` with file markers between chunks.
pub fn generate(ir: &CodegenIR, config: &CodegenConfig) -> Result<String> {
    let azul = generate_azul_go(config)?;
    let types_src = types::generate(ir, config)?;
    let raw_files = functions::generate_files(ir, config)?;
    let wrappers_src = wrappers::generate(ir, config)?;
    let callbacks_src = managed::generate(ir, config)?;
    let callbacks_export_src = managed::generate_export(ir, config)?;
    let gomod_src = gomod::generate_go_mod();

    let mut out = String::with_capacity(
        azul.len()
            + types_src.len()
            + raw_files.iter().map(|(_, c)| c.len()).sum::<usize>()
            + wrappers_src.len()
            + callbacks_src.len()
            + callbacks_export_src.len()
            + gomod_src.len()
            + 256,
    );
    push_section(&mut out, "azul.go", &azul);
    push_section(&mut out, "types.go", &types_src);
    for (path, content) in &raw_files {
        push_section(&mut out, path, content);
    }
    push_section(&mut out, "wrappers.go", &wrappers_src);
    push_section(&mut out, "callbacks.go", &callbacks_src);
    push_section(&mut out, "callbacks_export.go", &callbacks_export_src);
    push_section(&mut out, "go.mod", &gomod_src);
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
// azul.go (umbrella + cgo prelude)
// ============================================================================

/// Generate the `azul.go` umbrella file. This file owns the cgo prelude
/// (the `// #cgo` + `// #include` comment block immediately before
/// `import "C"`) and the package-level documentation. It contains no
/// Go declarations of its own — those live in the sibling files.
fn generate_azul_go(config: &CodegenConfig) -> Result<String> {
    let mut b = CodeBuilder::new(&config.indent);

    b.line("// ============================================================================");
    b.line("// azul.go - Go (cgo) bindings for the Azul GUI framework.");
    b.line("// Auto-generated by azul-doc codegen v2 (lang_go). DO NOT EDIT MANUALLY.");
    b.line("// ============================================================================");
    b.line("//");
    b.line("// Layout of this package:");
    b.line("//   types.go        Go-native mirrors of every C-ABI type (no cgo).");
    b.line("//   functions*.go   raw calls, one Go function per C export (the only cgo");
    b.line("//                   users; owned structs cross by pointer via the *Byref twins).");
    b.line("//   wrappers.go     idiomatic wrappers with Close()/finalizers (no cgo).");
    b.line("//   callbacks*.go   Go functions as libazul callbacks (host-invoker pattern).");
    b.line("//");
    b.line("// Build-time requirements (cgo):");
    b.line("//   * a working C compiler (gcc / clang / MinGW) on the host,");
    b.line("//   * `azul.h` reachable on the C include path (CGO_CFLAGS=-I...),");
    b.line("//   * `libazul.{so,dylib}` (or `azul.dll`) on the linker path (CGO_LDFLAGS=-L...).");
    b.line("//");
    b.line("// Runtime requirements:");
    b.line("//   * the same `libazul.{so,dylib}` (or `azul.dll`) reachable through");
    b.line("//     `LD_LIBRARY_PATH` (Linux), `DYLD_LIBRARY_PATH` (macOS), or `PATH`");
    b.line("//     (Windows). `-Wl,-rpath,$ORIGIN` also works on Linux when the");
    b.line("//     library sits next to the binary.");
    b.line("//");
    b.line("// ABI note: the Go types assume the 64-bit C ABI (8-byte pointers); the");
    b.line("// `const _ = ...` assertions in types.go fail the build anywhere else.");
    b.line("// ============================================================================");
    b.blank();
    b.line("package azul");
    b.blank();

    // The cgo prelude MUST be a single comment block (no blank lines)
    // immediately followed by `import "C"`. This file carries only the
    // package-wide linker directive; it names no C symbol.
    b.line("/*");
    b.line(&format!("#cgo LDFLAGS: -l{}", LIB_NAME));
    b.line("#include <stdint.h>");
    b.line("*/");
    b.line("import \"C\"");
    b.blank();

    Ok(b.finish())
}

// ============================================================================
// Shared name-mangling and type-mapping helpers (used by submodules)
// ============================================================================

/// The `Az`-prefixed FFI type name for an IR type
/// (e.g. `Dom` -> `AzDom`).
pub fn ffi_type_name(name: &str) -> String {
    format!("Az{}", name)
}

/// The Go-side wrapper type name (no `Az` prefix), with reserved-word
/// mangling. Matches the convention `azul.NewApp`, `azul.Dom`, etc.
pub fn go_type_name(name: &str) -> String {
    sanitize_identifier(name)
}

/// Sanitize a name for use as a Go identifier. Go reserved words are
/// mangled with a trailing underscore (the convention used by the
/// stdlib's `cgo` tooling for the same purpose).
pub fn sanitize_identifier(name: &str) -> String {
    if is_go_keyword(name) {
        format!("{}_", name)
    } else {
        name.to_string()
    }
}

/// The complete set of Go reserved keywords, plus the predeclared
/// identifiers that, while technically re-bindable, would create
/// confusing user-facing wrappers.
fn is_go_keyword(s: &str) -> bool {
    matches!(
        s,
        "break"
            | "case"
            | "chan"
            | "const"
            | "continue"
            | "default"
            | "defer"
            | "else"
            | "fallthrough"
            | "for"
            | "func"
            | "go"
            | "goto"
            | "if"
            | "import"
            | "interface"
            | "map"
            | "package"
            | "range"
            | "return"
            | "select"
            | "struct"
            | "switch"
            | "type"
            | "var"
    )
}

/// Convert a snake_case method name to PascalCase for idiomatic Go
/// (Go uses PascalCase for exported identifiers).
pub fn snake_to_pascal(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut upper = true;
    for c in s.chars() {
        if c == '_' {
            upper = true;
        } else if upper {
            out.extend(c.to_uppercase());
            upper = false;
        } else {
            out.push(c);
        }
    }
    out
}

/// Convert an api.json method name (typically already lowerCamelCase or
/// snake_case) into an idiomatic exported Go method name (PascalCase).
///
/// `new` becomes `New` (used as a constructor prefix), `default` becomes
/// `Default`. Other names are PascalCased.
pub fn idiomatic_method_name(method_name: &str) -> String {
    let pascal = if method_name.contains('_') {
        snake_to_pascal(method_name)
    } else {
        let mut chars = method_name.chars();
        match chars.next() {
            None => String::new(),
            Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
        }
    };
    // Every wrapper class gets a `Close()` method for io.Closer; a
    // user-API method named `close` would re-declare it (Go errors:
    // `method Close already declared`). Rename it. The SvgPath wrapper
    // hits this — `close` is the SVG path "close path" segment, not
    // a lifecycle operation.
    if pascal == "Close" {
        "CloseInner".to_string()
    } else {
        pascal
    }
}

/// PascalCase / camelCase -> snake_case. Mirrors the IR builder's
/// `to_snake_case` helper for class names (e.g. `StyleTextView` ->
/// `style_text_view`). Used to identify the implicit-self argument
/// the IR rewrites onto each instance method.
pub fn to_snake_case(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    let bytes = s.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        let c = b as char;
        if c.is_ascii_uppercase() {
            if i > 0 {
                let prev = bytes[i - 1] as char;
                let next = bytes.get(i + 1).map(|&n| n as char).unwrap_or(' ');
                let prev_lower_or_digit = prev.is_ascii_lowercase() || prev.is_ascii_digit();
                let next_lower = next.is_ascii_lowercase();
                if prev_lower_or_digit || (prev.is_ascii_uppercase() && next_lower) {
                    out.push('_');
                }
            }
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// Map a Rust/IR primitive name to its Go equivalent. Returns `None`
/// for non-primitives (caller routes those through `C.<TypeName>`).
pub fn primitive_to_go(name: &str) -> Option<&'static str> {
    Some(match name {
        "bool" => "bool",
        "u8" | "c_uchar" => "uint8",
        "i8" | "c_char" | "char" => "int8",
        "u16" => "uint16",
        "i16" => "int16",
        "u32" | "c_uint" => "uint32",
        "i32" | "c_int" => "int32",
        "u64" => "uint64",
        "i64" => "int64",
        "f32" => "float32",
        "f64" => "float64",
        "usize" => "uintptr",
        "isize" => "int",
        "c_void" | "void" | "()" => "",
        _ => return None,
    })
}

/// Map a Rust/IR primitive name directly to its `C.*` cgo equivalent
/// (used in cgo call sites where we need to cast Go values into C
/// argument types). Returns `None` for non-primitives.
pub fn primitive_to_cgo(name: &str) -> Option<&'static str> {
    Some(match name {
        "bool" => "C.bool",
        "u8" | "c_uchar" => "C.uint8_t",
        "i8" | "c_char" | "char" => "C.int8_t",
        "u16" => "C.uint16_t",
        "i16" => "C.int16_t",
        "u32" | "c_uint" => "C.uint32_t",
        "i32" | "c_int" => "C.int32_t",
        "u64" => "C.uint64_t",
        "i64" => "C.int64_t",
        "f32" => "C.float",
        "f64" => "C.double",
        "usize" => "C.size_t",
        "isize" => "C.intptr_t",
        _ => return None,
    })
}
