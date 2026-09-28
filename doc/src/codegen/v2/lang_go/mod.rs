//! Go binding generator (purego, no cgo).
//!
//! Emits a Go package (`package azul`) plus `go.mod`/`go.sum` that expose
//! `libazul`'s C-ABI to Go programs through
//! [`github.com/ebitengine/purego`](https://github.com/ebitengine/purego):
//! the shared library is `dlopen`ed / `LoadLibrary`ed at runtime, every
//! export is bound lazily on first use, and Go callbacks reach the engine
//! through purego trampolines. No C compiler is involved, so
//! `GOOS=windows go build` from macOS/Linux just works.
//!
//! # Files
//!
//! 1. `azul.go`          — package doc, `LoadLibrary`, the lazy-binding
//!                         helper `azRegister` (see [`generate_azul_go`]).
//! 2. `azul_unix.go` /
//!    `azul_windows.go`  — the per-OS `azOpenLibrary` (`purego.Dlopen` vs
//!                         `syscall.LoadLibrary`), selected by build tags.
//! 3. `types.go`         — Go-native definitions of every api.json type
//!                         with the exact `repr(C)` layout (see `types.rs`).
//! 4. `functions.go`     — the raw call layer, one Go function per C export
//!                         named like the C symbol; owned aggregates cross
//!                         by pointer through the exported `*Byref` twins
//!                         (see `functions.rs`).
//! 5. `wrappers.go`      — idiomatic wrappers (`type App struct { inner
//!                         *AzApp }`, constructors, methods, `Close()`).
//! 6. `callbacks.go` /
//!    `callbacks_trampolines.go` — host-invoker callback layer
//!                         (`Register<Kind>`, `Bind`, `Str`, smart setters).
//! 7. `go.mod` / `go.sum` — `module azul.rs/ui/go` + the purego pin.
//!
//! # Requirements
//!
//!   * Build: the Go toolchain (Go 1.18+, generics). `CGO_ENABLED` is
//!     irrelevant.
//!   * Run: `libazul.dylib` / `libazul.so` / `azul.dll` (or the release's
//!     platform-suffixed download name, e.g. `libazul.x86_64.dylib`) next to
//!     the executable, in the working directory, or on the loader path
//!     (`LoadLibrary("")`), or at an explicit path (`LoadLibrary(path)`).
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

/// Base name of the native library. The loader derives the platform file
/// name from it: `lib<name>.dylib` (macOS), `lib<name>.so` (Linux, BSDs),
/// `<name>.dll` (Windows). Must match the prebuilt release artifact.
pub const LIB_NAME: &str = "azul";

/// Public entry point. Generates the multi-file Go binding concatenated
/// into a single `String` with file markers between chunks.
pub fn generate(ir: &CodegenIR, config: &CodegenConfig) -> Result<String> {
    let sections: Vec<(&str, String)> = vec![
        ("azul.go", generate_azul_go(config)?),
        ("azul_unix.go", generate_azul_unix_go(config)?),
        ("azul_windows.go", generate_azul_windows_go(config)?),
        ("types.go", types::generate(ir, config)?),
        ("functions.go", functions::generate(ir, config)?),
        ("wrappers.go", wrappers::generate(ir, config)?),
        ("callbacks.go", managed::generate(ir, config)?),
        ("callbacks_trampolines.go", managed::generate_trampolines(ir, config)?),
        ("go.mod", gomod::generate_go_mod()),
        ("go.sum", gomod::generate_go_sum()),
    ];
    let mut out = String::with_capacity(
        sections.iter().map(|(_, c)| c.len() + 64).sum::<usize>(),
    );
    for (path, content) in &sections {
        push_section(&mut out, path, content);
    }
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
// azul.go (package doc + loader)
// ============================================================================

/// Generate the `azul.go` umbrella file: package documentation, the
/// library handle, `LoadLibrary` with its per-OS candidate search, and
/// `azRegister`, the lazy symbol binder every raw function and callback
/// factory goes through.
fn generate_azul_go(config: &CodegenConfig) -> Result<String> {
    let mut b = CodeBuilder::new(&config.indent);

    b.line("// ============================================================================");
    b.line("// azul.go - Go bindings for the Azul GUI framework (purego, no cgo).");
    b.line("// Auto-generated by azul-doc codegen v2 (lang_go). DO NOT EDIT MANUALLY.");
    b.line("// ============================================================================");
    b.line("//");
    b.line("// Layout of this package:");
    b.line("//   types.go                  Go-native mirrors of every C-ABI type.");
    b.line("//   functions.go              raw calls, one Go function per C export, bound");
    b.line("//                             lazily on first use; owned aggregates cross by");
    b.line("//                             pointer through the exported *Byref twins.");
    b.line("//   wrappers.go               idiomatic wrappers with Close()/finalizers.");
    b.line("//   callbacks*.go             Go functions as libazul callbacks (host-invoker");
    b.line("//                             pattern), Bind, Str, RefAny helpers.");
    b.line("//   azul_unix.go/_windows.go  the per-OS library loader.");
    b.line("//");
    b.line("// Build-time requirements: the Go toolchain only. No C compiler, no cgo;");
    b.line("// cross-compiling is `GOOS=windows GOARCH=amd64 go build`.");
    b.line("//");
    b.line("// Runtime requirements: the native library for the platform");
    b.line(&format!(
        "// (lib{n}.dylib / lib{n}.so / {n}.dll), located by LoadLibrary - see there.",
        n = LIB_NAME
    ));
    b.line("//");
    b.line("// ABI note: the Go types assume the 64-bit C ABI (8-byte pointers); the");
    b.line("// `const _ = ...` assertions in types.go fail the build anywhere else.");
    b.line("// ============================================================================");
    b.blank();
    b.line("package azul");
    b.blank();
    b.line("import (");
    b.line("    \"errors\"");
    b.line("    \"fmt\"");
    b.line("    \"os\"");
    b.line("    \"path/filepath\"");
    b.line("    \"runtime\"");
    b.line("    \"strings\"");
    b.line("    \"sync\"");
    b.blank();
    b.line("    \"github.com/ebitengine/purego\"");
    b.line(")");
    b.blank();
    b.line("// azLib is the handle of the loaded native library; 0 until LoadLibrary");
    b.line("// succeeded.");
    b.line("var azLib uintptr");
    b.blank();
    b.line("// azLoadMu serialises LoadLibrary so the callback trampolines are created");
    b.line("// exactly once per process (purego caps the number of NewCallback calls).");
    b.line("var azLoadMu sync.Mutex");
    b.blank();
    b.line("// LibraryFileName is the file name of the native library this package binds");
    b.line(&format!(
        "// on the running platform: lib{n}.dylib (macOS), {n}.dll (Windows), lib{n}.so",
        n = LIB_NAME
    ));
    b.line("// (everything else). LoadLibrary also accepts the release's platform-suffixed");
    b.line("// download names, see LibraryFileNames.");
    b.line("func LibraryFileName() string {");
    b.line("    switch runtime.GOOS {");
    b.line("    case \"darwin\", \"ios\":");
    b.line(&format!("        return \"lib{}.dylib\"", LIB_NAME));
    b.line("    case \"windows\":");
    b.line(&format!("        return \"{}.dll\"", LIB_NAME));
    b.line("    default:");
    b.line(&format!("        return \"lib{}.so\"", LIB_NAME));
    b.line("    }");
    b.line("}");
    b.blank();
    b.line("// LibraryFileNames lists every file name the native library may have on the");
    b.line("// running platform, most specific first: the platform-suffixed name the");
    b.line(&format!(
        "// release publishes (lib{n}.x86_64.dylib, lib{n}.linux-aarch64.so, {n}.i686.dll,",
        n = LIB_NAME
    ));
    b.line("// ...) and the canonical LibraryFileName(). LoadLibrary(\"\") accepts either, so");
    b.line("// a download does not have to be renamed.");
    b.line("func LibraryFileNames() []string {");
    b.line("    return libraryFileNamesFor(runtime.GOOS, runtime.GOARCH)");
    b.line("}");
    b.blank();
    b.line("// libraryFileNamesFor is LibraryFileNames for an explicit GOOS/GOARCH. The");
    b.line("// names follow the release file list (azul-doc dllgen/deploy.rs) and the");
    b.line("// Rust crate's build_link.rs.");
    b.line("func libraryFileNamesFor(goos, goarch string) []string {");
    b.line("    canonical := \"\"");
    b.line("    specific := \"\"");
    b.line("    switch goos {");
    b.line("    case \"darwin\", \"ios\":");
    b.line(&format!("        canonical = \"lib{}.dylib\"", LIB_NAME));
    b.line("        if goos == \"darwin\" && goarch == \"amd64\" {");
    b.line(&format!("            specific = \"lib{}.x86_64.dylib\"", LIB_NAME));
    b.line("        }");
    b.line("    case \"windows\":");
    b.line(&format!("        canonical = \"{}.dll\"", LIB_NAME));
    b.line("        if goarch == \"386\" {");
    b.line(&format!("            specific = \"{}.i686.dll\"", LIB_NAME));
    b.line("        }");
    b.line("    default:");
    b.line(&format!("        canonical = \"lib{}.so\"", LIB_NAME));
    b.line("        if goos == \"linux\" {");
    b.line("            suffix := map[string]string{");
    b.line("                \"386\": \"linux-i686\", \"arm64\": \"linux-aarch64\", \"arm\": \"linux-armv7\",");
    b.line("                \"ppc64\": \"linux-ppc64\", \"ppc64le\": \"linux-ppc64\", \"s390x\": \"linux-s390x\",");
    b.line("                \"riscv64\": \"linux-riscv64\",");
    b.line("            }[goarch]");
    b.line("            if suffix != \"\" {");
    b.line(&format!("                specific = \"lib{}.\" + suffix + \".so\"", LIB_NAME));
    b.line("            }");
    b.line("        }");
    b.line("    }");
    b.line("    if specific == \"\" {");
    b.line("        return []string{canonical}");
    b.line("    }");
    b.line("    return []string{specific, canonical}");
    b.line("}");
    b.blank();
    b.line("// LoadLibrary loads the native library and wires the callback trampolines.");
    b.line("// Call it once, before anything else in this package; later calls are");
    b.line("// no-ops that return nil.");
    b.line("//");
    b.line("// With an empty path, every name in LibraryFileNames() is searched in this");
    b.line("// order: the directory of the running executable, the current working");
    b.line("// directory, and the dynamic loader's own search path (DYLD_LIBRARY_PATH /");
    b.line("// LD_LIBRARY_PATH / PATH). A non-empty path is opened as given (absolute, or relative to the");
    b.line("// working directory) - the place to unpack a //go:embed'ed library to.");
    b.line("// On failure the error lists every candidate that was tried and why it");
    b.line("// failed.");
    b.line("func LoadLibrary(path string) error {");
    b.line("    azLoadMu.Lock()");
    b.line("    defer azLoadMu.Unlock()");
    b.line("    if azLib != 0 {");
    b.line("        return nil");
    b.line("    }");
    b.line("    var candidates []string");
    b.line("    if path != \"\" {");
    b.line("        candidates = []string{path}");
    b.line("    } else {");
    b.line("        names := LibraryFileNames()");
    b.line("        var dirs []string");
    b.line("        if exe, err := os.Executable(); err == nil {");
    b.line("            dirs = append(dirs, filepath.Dir(exe))");
    b.line("        }");
    b.line("        if cwd, err := os.Getwd(); err == nil && (len(dirs) == 0 || dirs[0] != cwd) {");
    b.line("            dirs = append(dirs, cwd)");
    b.line("        }");
    b.line("        for _, dir := range dirs {");
    b.line("            for _, name := range names {");
    b.line("                candidates = append(candidates, filepath.Join(dir, name))");
    b.line("            }");
    b.line("        }");
    b.line("        candidates = append(candidates, names...)");
    b.line("    }");
    b.line("    var tried []string");
    b.line("    for _, candidate := range candidates {");
    b.line("        handle, err := azOpenLibrary(candidate)");
    b.line("        if err == nil && handle != 0 {");
    b.line("            azLib = handle");
    b.line("            azInitCallbacks()");
    b.line("            return nil");
    b.line("        }");
    b.line("        if err == nil {");
    b.line("            err = errors.New(\"loader returned a null handle\")");
    b.line("        }");
    b.line("        tried = append(tried, fmt.Sprintf(\"%s: %v\", candidate, err))");
    b.line("    }");
    b.line("    return fmt.Errorf(\"azul.LoadLibrary: could not load %s; tried:\\n  %s\",");
    b.line("        strings.Join(LibraryFileNames(), \" or \"), strings.Join(tried, \"\\n  \"))");
    b.line("}");
    b.blank();
    b.line("// azRegister binds a purego function value (`fptr` is a pointer to it) to");
    b.line("// the named libazul export. functions.go and callbacks.go call it exactly");
    b.line("// once per symbol, on first use (sync.Once).");
    b.line("func azRegister(fptr any, name string) {");
    b.line("    if azLib == 0 {");
    b.line("        panic(\"azul: LoadLibrary must succeed before calling \" + name)");
    b.line("    }");
    b.line("    purego.RegisterLibFunc(fptr, azLib, name)");
    b.line("}");
    b.blank();

    Ok(b.finish())
}

/// `azul_unix.go`: `azOpenLibrary` via `purego.Dlopen` (every OS where
/// purego provides dlopen).
fn generate_azul_unix_go(config: &CodegenConfig) -> Result<String> {
    let mut b = CodeBuilder::new(&config.indent);
    b.line("//go:build !windows");
    b.blank();
    b.line("// azul_unix.go - library loader for dlopen platforms (purego.Dlopen).");
    b.line("// Auto-generated by azul-doc codegen v2 (lang_go). DO NOT EDIT MANUALLY.");
    b.blank();
    b.line("package azul");
    b.blank();
    b.line("import \"github.com/ebitengine/purego\"");
    b.blank();
    b.line("// azOpenLibrary opens the shared library at `path` (a bare file name is");
    b.line("// resolved by the dynamic loader's search path) and returns its handle.");
    b.line("func azOpenLibrary(path string) (uintptr, error) {");
    b.line("    return purego.Dlopen(path, purego.RTLD_NOW|purego.RTLD_GLOBAL)");
    b.line("}");
    Ok(b.finish())
}

/// `azul_windows.go`: `azOpenLibrary` via the standard library's
/// `syscall.LoadLibrary` (purego has no `Dlopen` on Windows; its
/// `RegisterLibFunc`/`NewCallback` work with any HMODULE).
fn generate_azul_windows_go(config: &CodegenConfig) -> Result<String> {
    let mut b = CodeBuilder::new(&config.indent);
    b.line("//go:build windows");
    b.blank();
    b.line("// azul_windows.go - library loader for Windows (LoadLibraryW).");
    b.line("// Auto-generated by azul-doc codegen v2 (lang_go). DO NOT EDIT MANUALLY.");
    b.blank();
    b.line("package azul");
    b.blank();
    b.line("import \"syscall\"");
    b.blank();
    b.line("// azOpenLibrary opens the DLL at `path` (a bare file name goes through the");
    b.line("// standard DLL search order: executable directory, system dirs, PATH) and");
    b.line("// returns its module handle.");
    b.line("func azOpenLibrary(path string) (uintptr, error) {");
    b.line("    handle, err := syscall.LoadLibrary(path)");
    b.line("    return uintptr(handle), err");
    b.line("}");
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

/// Map a Rust/IR primitive name to its Go equivalent. Returns `None`
/// for non-primitives (every other name is an api.json type, `Az<Name>`).
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
