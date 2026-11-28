#!/usr/bin/env python3
"""EXPERIMENT (the Android GPU root-cause hunt, never for master): makes third_party/gl-context-loader-trace, a copy of the crates.io crate
gl-context-loader 0.1.10 whose every GL call (221 functions) goes through a runtime-switchable trace: the call, its arguments, what it
returned, the values it wrote into `&mut [T]` out parameters, and the GL error it left, one line per call, numbered.

    python3 scripts/dev/gl_trace_crate.py <path of the crates.io source of gl-context-loader 0.1.10> third_party/gl-context-loader-trace

The generated lib.rs wraps the body of each function that has a `_gl_print_debug("glName")` line (all but `get_type` and `read_pixels`, which
call others that have one):

    let __g = crate::trace::enter(self, "glName", &|| format!("a={}, b={}", ...));
    let __r: RET = (|| -> RET { <the original body> })();
    __g.after(&__r, &[<out parameters>]);
    __r
"""
import os
import re
import shutil
import sys


def matching_brace(text, open_index):
    """Index of the `}` that closes the `{` at open_index, skipping string and char literals and comments."""
    depth = 0
    i = open_index
    n = len(text)
    while i < n:
        c = text[i]
        if c == '"':
            i += 1
            while i < n and text[i] != '"':
                if text[i] == "\\":
                    i += 1
                i += 1
        elif c == "'" and i + 2 < n and (text[i + 2] == "'" or (text[i + 1] == "\\" and text[i + 3] == "'")):
            i += 3 if text[i + 2] == "'" else 4
        elif text.startswith("//", i):
            while i < n and text[i] != "\n":
                i += 1
        elif c == "{":
            depth += 1
        elif c == "}":
            depth -= 1
            if depth == 0:
                return i
        i += 1
    raise ValueError("no matching brace")


def split_params(text):
    """Splits `a: A, b: &[B], c: (X, Y)` on the commas of depth 0."""
    parts, depth, cur = [], 0, ""
    for c in text:
        if c in "([<":
            depth += 1
        elif c in ")]>":
            # the `>` of `->` never appears in a parameter list
            depth -= 1
        if c == "," and depth == 0:
            parts.append(cur.strip())
            cur = ""
        else:
            cur += c
    if cur.strip():
        parts.append(cur.strip())
    return parts


def arg_expr(name, ty):
    ty = ty.strip()
    compact = ty.replace(" ", "")
    if compact in ("&[u8]", "&mut[u8]", "Option<&[u8]>", "Option<&mut[u8]>"):
        if compact.startswith("Option"):
            return f'{name}={{}}', f'match &{name} {{ Some(s) => format!("len {{}}", s.len()), None => "none".to_string() }}'
        return f'{name}={{}}', f'format!("len {{}}", {name}.len())'
    if compact == "&[&[u8]]":
        return f'{name}={{}}', f'format!("{{}} strings, lengths {{:?}}", {name}.len(), {name}.iter().map(|s| s.len()).collect::<Vec<_>>())'
    if compact.startswith("*const") or compact.startswith("*mut"):
        return f'{name}={{:p}}', name
    if compact.startswith("&mut["):
        return f'{name}={{}}', f'format!("len {{}}", {name}.len())'
    return f'{name}={{}}', f'crate::trace::short(&{name})'


def transform(src):
    start = src.index("macro_rules! impl_gl_context")
    head, body = src[:start], src[start:]
    out, pos = [], 0
    traced = 0
    total = 0
    for m in re.finditer(r"^        \$\( \$opt \)\? (unsafe )?fn (\w+)\s*\(", body, re.M):
        total += 1
        name = m.group(2)
        # the end of the parameter list: the matching `)`
        open_paren = m.end() - 1
        depth, i = 0, open_paren
        while True:
            if body[i] == "(":
                depth += 1
            elif body[i] == ")":
                depth -= 1
                if depth == 0:
                    break
            i += 1
        params_text = body[open_paren + 1:i]
        brace = body.index("{", i)
        ret_text = body[i + 1:brace].strip()
        ret = "()"
        if ret_text.startswith("->"):
            ret = ret_text[2:].strip()
        close = matching_brace(body, brace)
        fn_body = body[brace + 1:close]
        gl_name = re.search(r'_gl_print_debug\("(\w+)"\)', fn_body)
        if not gl_name:
            continue
        params = [p for p in split_params(params_text) if p and not p.startswith("&self") and p != "self"]
        fmt_parts, fmt_args, outs = [], [], []
        for p in params:
            pname, ptype = [x.strip() for x in p.split(":", 1)]
            f, a = arg_expr(pname, ptype)
            fmt_parts.append(f)
            fmt_args.append(a)
            if ptype.replace(" ", "").startswith("&mut["):
                outs.append(f'format!("{pname}={{}}", crate::trace::short(&{pname}[..{pname}.len().min(8)]))')
        fmt = ", ".join(fmt_parts)
        args_closure = f'&|| format!("{fmt}"{"".join(", " + a for a in fmt_args)})'
        new_body = (
            "\n            #[cfg(feature = \"trace\")]\n"
            f"            let __g = crate::trace::enter(self, \"{gl_name.group(1)}\", {args_closure});\n"
            f"            let __r: {ret} = (|| -> {ret} {{{fn_body}}})();\n"
            "            #[cfg(feature = \"trace\")]\n"
            f"            __g.after(&__r, &[{', '.join(outs)}]);\n"
            "            __r\n        "
        )
        out.append(body[pos:brace + 1])
        out.append(new_body)
        pos = close
        traced += 1
    out.append(body[pos:])
    print(f"{traced} of {total} functions traced", file=sys.stderr)
    return head + "".join(out)


TRACE_MODULE = r'''//! EXPERIMENT (never for master): the runtime-switchable trace of every GL call of the table. Generated by scripts/dev/gl_trace_crate.py's
//! companion; see `enter` and `Guard::after`.
use crate::c_void;
use std::{
    fmt::Debug,
    io::Write,
    sync::{
        atomic::{AtomicU64, AtomicU8, Ordering},
        Mutex,
    },
};

use crate::GenericGlContext;

/// 0: off. 1: only the calls that left a GL error are written. 2: every call.
static LEVEL: AtomicU8 = AtomicU8::new(0);
static SEQ: AtomicU64 = AtomicU64::new(0);
static ERRORS: AtomicU64 = AtomicU64::new(0);
static SINK: Mutex<Sink> = Mutex::new(Sink { file: None, buf: String::new(), errors_to_stdout: true, echo: false });

struct Sink {
    file: Option<std::fs::File>,
    buf: String,
    errors_to_stdout: bool,
    /// Every line goes to stdout (logcat on a phone) as it is made, not to a file.
    echo: bool,
}

/// Starts the trace at `level` (see `LEVEL`), writing to `path` when given. Errors are also printed to stdout (logcat on a phone).
pub fn start(level: u8, path: Option<&str>) {
    if let Ok(mut sink) = SINK.lock() {
        if path == Some("stdout") {
            sink.echo = true;
        } else if let Some(path) = path {
            sink.file = std::fs::OpenOptions::new().create(true).append(true).open(path).ok();
            if sink.file.is_none() {
                println!("GLTRACE: cannot open {path}");
            }
        }
    }
    LEVEL.store(level, Ordering::Relaxed);
    println!("GLTRACE: level {level}, file {path:?}");
}

/// The level now.
pub fn level() -> u8 {
    LEVEL.load(Ordering::Relaxed)
}

/// Changes the level (0 stops the trace).
pub fn set_level(level: u8) {
    LEVEL.store(level, Ordering::Relaxed);
}

/// How many GL calls were traced and how many left an error.
pub fn counts() -> (u64, u64) {
    (SEQ.load(Ordering::Relaxed), ERRORS.load(Ordering::Relaxed))
}

/// A line of the engine's own in the trace, and a flush.
pub fn mark(label: &str) {
    if LEVEL.load(Ordering::Relaxed) == 0 {
        return;
    }
    let seq = SEQ.load(Ordering::Relaxed);
    push(&format!("== {label} (after call #{seq})\n"));
    flush();
}

/// Writes what is buffered to the file.
pub fn flush() {
    if let Ok(mut sink) = SINK.lock() {
        let Sink { file, buf, .. } = &mut *sink;
        if let Some(file) = file.as_mut() {
            let _ = file.write_all(buf.as_bytes());
            let _ = file.flush();
        }
        buf.clear();
    }
}

fn push(line: &str) {
    if let Ok(mut sink) = SINK.lock() {
        if sink.echo {
            print!("GLT {line}");
            return;
        }
        sink.buf.push_str(line);
        if sink.buf.len() > (256 << 10) {
            let Sink { file, buf, .. } = &mut *sink;
            if let Some(file) = file.as_mut() {
                let _ = file.write_all(buf.as_bytes());
            }
            buf.clear();
        }
    }
}

/// A value for the trace: its Debug form, cut at 100 characters.
pub fn short<T: Debug + ?Sized>(value: &T) -> String {
    let mut text = format!("{value:?}");
    if text.len() > 100 {
        let mut cut = 100;
        while !text.is_char_boundary(cut) {
            cut -= 1;
        }
        text.truncate(cut);
        text.push_str("...");
    }
    text
}

/// What a wrapped GL function holds while it runs.
pub struct Guard {
    live: bool,
    seq: u64,
    name: &'static str,
    args: String,
    get_error: *mut c_void,
}

/// Called at the start of every GL function of the table.
pub fn enter(gl: &GenericGlContext, name: &'static str, args: &dyn Fn() -> String) -> Guard {
    if LEVEL.load(Ordering::Relaxed) == 0 {
        return Guard { live: false, seq: 0, name, args: String::new(), get_error: core::ptr::null_mut() };
    }
    Guard {
        live: true,
        seq: SEQ.fetch_add(1, Ordering::Relaxed),
        name,
        args: args(),
        get_error: gl.glGetError,
    }
}

impl Guard {
    /// Called after the call: the GL error it left (read and cleared here), what it returned and what it wrote into its out parameters.
    pub fn after<T: Debug>(self, ret: &T, outs: &[String]) {
        if !self.live {
            return;
        }
        let error = if self.name == "glGetError" || self.get_error.is_null() {
            0
        } else {
            // SAFETY: a non-null `glGetError` of the table is the function of that name.
            let f: extern "system" fn() -> u32 = unsafe { core::mem::transmute(self.get_error) };
            f()
        };
        let level = LEVEL.load(Ordering::Relaxed);
        if error == 0 && level < 2 {
            return;
        }
        let mut line = format!("#{} {}({})", self.seq, self.name, self.args);
        for out in outs {
            line.push_str(" out ");
            line.push_str(out);
        }
        let ret = short(ret);
        if ret != "()" {
            line.push_str(" = ");
            line.push_str(&ret);
        }
        if error != 0 {
            ERRORS.fetch_add(1, Ordering::Relaxed);
            line.push_str(&format!(" !! GL ERROR 0x{error:x}"));
            if SINK.lock().map(|s| s.errors_to_stdout && !s.echo).unwrap_or(true) {
                println!("GLTRACE {line}");
            }
        }
        line.push('\n');
        push(&line);
    }
}
'''


def main():
    registry, dest = sys.argv[1], sys.argv[2]
    os.makedirs(os.path.join(dest, "src"), exist_ok=True)
    for name in ("Cargo.toml", "README.md", "LICENSE-APACHE.txt", "LICENSE-MIT.txt"):
        shutil.copy(os.path.join(registry, name), os.path.join(dest, name))
    src = open(os.path.join(registry, "src", "lib.rs")).read()
    new = transform(src)
    # `get_debug_messages` returns a Vec of this: the trace prints what a call returned
    assert "pub struct DebugMessage {" in new
    new = new.replace("pub struct DebugMessage {", "#[derive(Debug)]\npub struct DebugMessage {", 1)
    # the trace module, under the new feature
    new = new.replace("macro_rules! impl_gl_context", '#[cfg(feature = "trace")]\npub mod trace;\n\nmacro_rules! impl_gl_context', 1)
    open(os.path.join(dest, "src", "lib.rs"), "w").write(new)
    open(os.path.join(dest, "src", "trace.rs"), "w").write(TRACE_MODULE)
    toml = open(os.path.join(dest, "Cargo.toml")).read()
    toml = toml.replace("[features]\ndebug = [\"std\"]", "[features]\ntrace = [\"std\"]\ndebug = [\"std\"]", 1)
    open(os.path.join(dest, "Cargo.toml"), "w").write(toml)


if __name__ == "__main__":
    main()
