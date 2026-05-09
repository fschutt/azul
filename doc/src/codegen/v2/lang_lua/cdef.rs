//! C-header → LuaJIT `ffi.cdef` payload transformation
//!
//! LuaJIT's FFI parser accepts most C declarations but cannot consume
//! preprocessor directives (`#include`, `#ifdef`, `#define`, `#endif`,
//! macro expansions, etc.) or C++-isms like `extern "C"`.
//!
//! This module takes the output of the production C header generator
//! (`super::lang_c::CGenerator`) and strips/rewrites the unsupported
//! constructs so the remaining text can be embedded directly inside
//! `ffi.cdef[[ ... ]]`.
//!
//! # Strategy
//!
//! 1. Skip lines that begin with `#` (after optional whitespace).
//! 2. Skip lines containing `extern "C"` openers/closers.
//! 3. Replace the platform-specific `DLLIMPORT` macro with empty text.
//! 4. Drop the `restrict` qualifier (LuaJIT's parser doesn't recognize it).
//! 5. Drop the `AZ_REFLECT*` macro definitions and helpers (they expand to
//!    code, not types, and aren't needed by the FFI binding).
//!
//! Other preprocessor lines (`#define X Y`) for **constants** are turned
//! into `enum { X = Y };` enum members so callers can still see the value
//! through the cdef. We only do this for simple integer literal defines.

/// Strip C preprocessor and C++isms from a C-header source string so the
/// result is acceptable inside a LuaJIT `ffi.cdef[[ ... ]]` block.
///
/// Two passes:
///
/// 1. `strip_preprocessor` — line-oriented filter that drops/rewrites
///    constructs LuaJIT FFI can't parse (#includes, extern "C", etc.).
/// 2. `reorder_callback_typedefs` — moves every
///    `typedef X (*Az<Name>Type)(args)` line below the struct definitions
///    so LuaJIT can see the full struct layouts when computing the size
///    of by-value struct args. C compilers don't need this because they
///    only resolve sizes lazily; LuaJIT FFI does it at typedef-parse
///    time and silently drops unparseable args, leaving the typedef
///    looking like `Result (*)()`.
pub fn strip_for_cdef(c_header: &str) -> String {
    let stripped = strip_preprocessor(c_header);
    reorder_callback_typedefs(&stripped)
}

/// Move callback function-pointer typedefs to the bottom of the cdef.
///
/// LuaJIT FFI needs the full struct definitions of any by-value args
/// before it can parse a function-pointer typedef. The C header generator
/// emits the typedefs in an order that's correct for cc (forward decls
/// suffice) but wrong for LuaJIT.
///
/// Callback typedefs are detected by the shape `typedef X (*Az<...>Type)(...)`
/// (matches `AzCallbackType`, `AzLayoutCallbackType`, `AzRefAnyDestructorType`,
/// `AzU8VecDestructorType`, etc. — every function-pointer typedef the
/// C header generator emits).
///
/// Struct fields that originally referenced a callback typedef by name
/// would fail to parse if we just moved the typedef without further
/// changes (the name isn't in scope yet at struct-definition time). We
/// rewrite those fields to `void *` — function pointers are pointer-sized
/// so the struct layout is unaffected, and user code goes through wrapper
/// methods that bypass direct field access.
fn reorder_callback_typedefs(input: &str) -> String {
    // Pass 1: collect the names of all callback typedefs so we know which
    // struct fields to rewrite.
    let mut cb_names = std::collections::HashSet::new();
    for line in input.lines() {
        if let Some(n) = parse_callback_typedef_name(line.trim()) {
            cb_names.insert(n);
        }
    }

    // Pass 2: split into [rewritten body, callback typedefs, extern fn decls].
    let mut callbacks: Vec<&str> = Vec::new();
    let mut externs: Vec<&str> = Vec::new();
    let mut body = String::with_capacity(input.len());
    for line in input.lines() {
        if is_callback_typedef_line(line) {
            callbacks.push(line);
            continue;
        }
        if line.trim_start().starts_with("extern ") {
            externs.push(line);
            continue;
        }
        let rewritten = rewrite_callback_field(line, &cb_names);
        body.push_str(&rewritten);
        body.push('\n');
    }

    // Emit: body, callback typedefs, then function declarations.
    //
    // The C header generator orders things in a way that's correct for cc
    // (forward decls suffice for typedefs) but wrong for LuaJIT FFI
    // (which needs full struct definitions before parsing typedef args).
    // Pulling the function declarations down too means they see the
    // typedefs already defined.
    let mut out = body;
    if !callbacks.is_empty() {
        out.push('\n');
        out.push_str(
            "/* Callback function-pointer typedefs — emitted after struct\n   \
             definitions so LuaJIT FFI can compute by-value arg sizes.\n   \
             Struct fields that referenced these typedefs by name have been\n   \
             rewritten to `void *` (same size — fn pointers are pointer-sized)\n   \
             so the structs parse without the typedef being in scope. */\n",
        );
        for l in &callbacks {
            out.push_str(l);
            out.push('\n');
        }
    }
    if !externs.is_empty() {
        out.push('\n');
        out.push_str(
            "/* Function declarations — moved here so they can resolve the\n   \
             callback-typedef names defined just above. */\n",
        );
        for l in &externs {
            out.push_str(l);
            out.push('\n');
        }
    }
    out
}

/// Detect `typedef X (*AzYType)(args);` and extract the typedef's name.
fn parse_callback_typedef_name(t: &str) -> Option<String> {
    if !t.starts_with("typedef ") {
        return None;
    }
    let open = t.find("(*")?;
    let after_star = &t[open + 2..];
    let close = after_star.find(')')?;
    // After the name's closing `)` we expect `(` (the args list).
    let after_name_close = &after_star[close..];
    if !after_name_close.starts_with(")(") {
        return None;
    }
    Some(after_star[..close].to_string())
}

fn is_callback_typedef_line(line: &str) -> bool {
    parse_callback_typedef_name(line.trim()).is_some()
}

/// Rewrite struct/union fields whose type is a callback typedef.
///
/// `    AzXxxCallbackType field_name;` → `    void* field_name;` (with a
/// trailing comment for traceability). Indentation is preserved.
fn rewrite_callback_field(line: &str, cb_names: &std::collections::HashSet<String>) -> String {
    // Quick reject: only rewrite indented `<Type> <name>;` lines that look
    // like struct/union members. Top-level typedefs and free declarations
    // start at column 0.
    if !line.starts_with(' ') && !line.starts_with('\t') {
        return line.to_string();
    }
    let trimmed = line.trim_end();
    if !trimmed.ends_with(';') {
        return line.to_string();
    }
    let inner = trimmed.trim_end_matches(';').trim_end();
    // Split on whitespace from the right: "<TypeTokens> <fieldName>".
    let (ty, field) = match inner.rsplit_once(char::is_whitespace) {
        Some(p) => p,
        None => return line.to_string(),
    };
    let ty = ty.trim();
    if !cb_names.contains(ty) {
        return line.to_string();
    }
    // Preserve the leading whitespace of the original line so indentation
    // matches the surrounding struct body.
    let leading_ws_end = line.find(|c: char| !c.is_whitespace()).unwrap_or(0);
    let leading = &line[..leading_ws_end];
    format!("{leading}void* {field}; /* was {ty} */")
}

fn strip_preprocessor(c_header: &str) -> String {
    let mut out = String::with_capacity(c_header.len());
    let mut iter = c_header.lines().peekable();
    let mut skip_depth: usize = 0;
    let mut in_extern_c_block = false;
    let mut saw_outer_guard = false;
    // We also drop the entire `AZ_REFLECT*` macro definitions because they
    // expand into multiple statements (no point in feeding them to the FFI
    // parser since LuaJIT users cannot reuse them anyway).
    let mut in_macro_continuation = false;
    // The C header inlines a couple of thousand `bool AzXxx_match...` helper
    // function *definitions* (note: definitions, not declarations) intended
    // for C consumers. LuaJIT FFI's `cdef` parser only accepts declarations,
    // so we elide function bodies. We detect the start by seeing a line that
    // ends with `{` and isn't a struct/union/enum/typedef/extern, then skip
    // until brace depth returns to zero.
    let mut in_fn_body = false;
    let mut fn_brace_depth: i32 = 0;

    while let Some(line) = iter.next() {
        let trimmed = line.trim_start();

        // Currently skipping a function body — track braces and bail when
        // we close the outermost.
        if in_fn_body {
            for ch in line.chars() {
                if ch == '{' {
                    fn_brace_depth += 1;
                } else if ch == '}' {
                    fn_brace_depth -= 1;
                }
            }
            if fn_brace_depth <= 0 {
                in_fn_body = false;
                fn_brace_depth = 0;
            }
            continue;
        }

        // Continuation of a previous backslash-terminated macro definition?
        if in_macro_continuation {
            if line.trim_end().ends_with('\\') {
                continue;
            } else {
                in_macro_continuation = false;
                continue;
            }
        }

        // Detect AZ_REFLECT macro definition start (multiline `#define ... \`)
        if trimmed.starts_with("#define AZ_REFLECT") {
            if line.trim_end().ends_with('\\') {
                in_macro_continuation = true;
            }
            continue;
        }

        // Detect AzString_fromConstStr macro - skip its multi-line definition
        if trimmed.starts_with("#define AzString_fromConstStr") {
            if line.trim_end().ends_with('\\') {
                in_macro_continuation = true;
            }
            continue;
        }

        // Detect any Vec_empty macro - they're multi-line backslash continuations
        if trimmed.starts_with("#define ") && line.trim_end().ends_with('\\') {
            in_macro_continuation = true;
            continue;
        }

        // Track preprocessor block depth so we strip everything inside.
        //
        // Special case: the outermost `#ifndef AZUL_H` ... `#endif /* AZUL_H */`
        // header guard's body is the *entire* file, so we MUST pass it
        // through unchanged rather than skipping. We detect it as the
        // first `#ifndef AZUL_H` we see and remember to balance it on the
        // closing `#endif`. Nested `#if`/`#ifdef` blocks (cross-platform
        // typedef branches, `__cplusplus`, etc.) keep the strip-by-depth
        // behavior — those parallel arms aren't useful to LuaJIT.
        if trimmed.starts_with("#if")
            || trimmed.starts_with("#ifdef")
            || trimmed.starts_with("#ifndef")
        {
            // First-ever #ifndef AZUL_H: pass through (drop only the line).
            if !saw_outer_guard
                && (trimmed == "#ifndef AZUL_H" || trimmed.starts_with("#ifndef AZUL_H "))
            {
                saw_outer_guard = true;
                continue;
            }
            skip_depth += 1;
            continue;
        }
        if trimmed.starts_with("#endif") {
            // The matching `#endif /* AZUL_H */` for the outer guard:
            // balance it by also dropping just the line.
            if skip_depth == 0 && saw_outer_guard {
                continue;
            }
            if skip_depth > 0 {
                skip_depth -= 1;
            }
            continue;
        }
        if skip_depth > 0 {
            continue;
        }

        // Drop other preprocessor lines unconditionally
        if trimmed.starts_with('#') {
            continue;
        }

        // Drop extern "C" wrappers
        if trimmed.starts_with("extern \"C\"") {
            // Either `extern "C" {` (open) or just `extern "C" ...`
            if trimmed.contains('{') {
                in_extern_c_block = true;
            }
            continue;
        }
        if in_extern_c_block && trimmed == "}" {
            in_extern_c_block = false;
            continue;
        }

        // Skip pure-comment lines that contain only `/* End ... */` markers
        // generated by the C header generator (cosmetic only).
        if trimmed.starts_with("/* End ")
            || trimmed.starts_with("/* C++ ")
            || trimmed.starts_with("/* C99 ")
            || trimmed.starts_with("/* Empty Vec ")
            || trimmed.starts_with("/* Macro ")
            || trimmed.starts_with("/* C-only reflection")
            || trimmed.starts_with("/* Internal macro")
            || trimmed.starts_with("/* Full reflection")
            || trimmed.starts_with("/* Macro to ")
        {
            continue;
        }

        // Detect a function-definition opening line: ends with `{` and is
        // not a struct/union/enum/typedef block. The C header inlines a few
        // thousand `bool AzXxx_match...` helpers as full definitions; LuaJIT
        // FFI cdef can only consume declarations, not bodies.
        let trimmed_end = line.trim_end();
        if trimmed_end.ends_with('{')
            && !trimmed.starts_with("struct ")
            && !trimmed.starts_with("union ")
            && !trimmed.starts_with("enum ")
            && !trimmed.starts_with("typedef ")
            && !trimmed.starts_with("extern ")
            && !trimmed.starts_with("//")
            && !trimmed.starts_with("/*")
            && trimmed.contains('(')
            && trimmed.contains(')')
        {
            // It's a function definition. Skip it and its body.
            in_fn_body = true;
            for ch in line.chars() {
                if ch == '{' {
                    fn_brace_depth += 1;
                } else if ch == '}' {
                    fn_brace_depth -= 1;
                }
            }
            if fn_brace_depth <= 0 {
                in_fn_body = false;
                fn_brace_depth = 0;
            }
            continue;
        }

        // Strip `DLLIMPORT` macro and `restrict` qualifier from the line
        let mut cleaned = line.replace("DLLIMPORT", "");
        cleaned = cleaned.replace(" restrict", "");
        cleaned = cleaned.replace("\t* restrict", "*");
        cleaned = cleaned.replace("* restrict", "*");

        // Skip lines that became empty after cleanup
        if cleaned.trim().is_empty() {
            // Preserve blank lines in moderation for readability
            if out.ends_with("\n\n") {
                continue;
            }
            out.push('\n');
            continue;
        }

        out.push_str(&cleaned);
        out.push('\n');
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_includes_and_ifdef_blocks() {
        let input = r#"
#ifndef AZUL_H
#define AZUL_H
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

struct AzApp {
    void* ptr;
};

#ifdef __cplusplus
}
#endif

#endif /* AZUL_H */
"#;
        let out = strip_for_cdef(input);
        assert!(!out.contains("#include"));
        assert!(!out.contains("#ifdef"));
        assert!(!out.contains("extern \"C\""));
        assert!(out.contains("struct AzApp"));
    }

    #[test]
    fn drops_dllimport_and_restrict() {
        let input = "extern DLLIMPORT void AzApp_run(AzApp* restrict self);\n";
        let out = strip_for_cdef(input);
        assert!(!out.contains("DLLIMPORT"));
        assert!(!out.contains("restrict"));
        assert!(out.contains("AzApp_run"));
    }
}
