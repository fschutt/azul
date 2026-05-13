//! Phase C functional blueprint for azul's web backend.
//!
//! Pipeline:
//!     [1] extern "C" fn  in this binary
//!     [2] fn pointer     → raw machine bytes (read from .text)
//!     [3] disassembly    → yaxpeax-arm (aarch64) / yaxpeax-x86 (amd64)
//!     [4] lift           → LLVM IR text (StubLifter or RemillLifter)
//!     [5] llc            → wasm32 object file
//!     [6] wasm-ld        → final .wasm module
//!     [7] hex dump       → stdout, also written to target/transpile_blueprint.wasm
//!
//! Run:
//!     cargo run --release                          # stub lifter
//!     REMILL_INSTALL_DIR=... \
//!         cargo run --release --features remill    # remill (when built)
//!
//! What's deliberately faked vs. real:
//! - **Real**: fn address lookup, byte read from running .text, disassembly,
//!   llc invocation (LLVM 21 from brew), wasm-ld invocation (lld@21 from brew),
//!   final .wasm bytes.
//! - **Faked (stub mode)**: the lift step. StubLifter emits hand-written
//!   IR for an `(i32,i32)->i32` add. RemillLifter swaps in the real
//!   semantics-driven IR via FFI once the cxx-common + remill build
//!   completes in the background.

mod lifter;
#[cfg(feature = "remill")]
mod ffi;

use lifter::{Arch, LlvmLifter, RemillCliLifter, StubLifter};
use std::path::{Path, PathBuf};
use std::process::Command;

/// The target function. Marked `#[inline(never)]` + `#[no_mangle]` so
/// the symbol is preserved and the function isn't elided into callers.
/// Compiles to ~3 aarch64 instructions on release: `add w0, w0, w1; ret`
/// (with maybe a frame setup at the top depending on opt level).
#[inline(never)]
#[no_mangle]
pub extern "C" fn add_blueprint(a: i32, b: i32) -> i32 {
    a + b
}

const HOST_ARCH: Arch = if cfg!(target_arch = "aarch64") {
    Arch::AArch64
} else if cfg!(target_arch = "x86_64") {
    Arch::Amd64
} else {
    panic!("unsupported host arch — only aarch64 and x86_64 wired up")
};

const READ_WINDOW: usize = 64; // bytes to peek into .text
const EXPORT_SYMBOL: &str = "add_blueprint";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("┌─ azul Phase-C functional blueprint ──────────────────────┐");
    println!("│  host arch: {:<46}│", format!("{:?}", HOST_ARCH));
    println!("└──────────────────────────────────────────────────────────┘\n");

    // [1] sanity-check the function still works natively
    let native = add_blueprint(7, 35);
    println!("[1] native call    add_blueprint(7, 35) = {}", native);

    // [2] read the function's machine code from .text
    let fn_addr = add_blueprint as *const () as usize;
    let bytes: &[u8] = unsafe {
        std::slice::from_raw_parts(fn_addr as *const u8, READ_WINDOW)
    };
    println!(
        "[2] fn pointer:    0x{:x}  ({} bytes peeked)",
        fn_addr, READ_WINDOW
    );
    println!("    raw .text:");
    for (i, chunk) in bytes.chunks(16).enumerate() {
        print!("      {:#06x}:  ", i * 16);
        for b in chunk {
            print!("{:02x} ", b);
        }
        println!();
    }
    println!();

    // [3] disassemble with yaxpeax
    println!("[3] disassembly (host arch):");
    disassemble(fn_addr as u64, bytes);
    println!();

    // [4] lift to LLVM IR
    //   --lifter=stub       (default)           → StubLifter
    //   --lifter=remill     (or no flag + found) → RemillCliLifter (subprocess)
    //   --lifter=remill-ffi (Cargo feature)      → RemillFfiLifter (cxx, WIP)
    let args: Vec<String> = std::env::args().collect();
    let lifter_choice = args.iter()
        .find_map(|a| a.strip_prefix("--lifter=").map(str::to_string))
        .unwrap_or_else(|| "auto".to_string());

    let lifter: Box<dyn LlvmLifter> = match lifter_choice.as_str() {
        "stub" => Box::new(StubLifter),
        "remill" => match RemillCliLifter::discover() {
            Some(l) => Box::new(l),
            None => return Err(
                "--lifter=remill but remill-lift-17 not found. \
                 Either set $REMILL_LIFT_BIN, or build remill via \
                 `bash scripts/build_remill.sh`."
                .into(),
            ),
        },
        "remill-ffi" => {
            #[cfg(feature = "remill")]
            { Box::new(lifter::RemillFfiLifter) }
            #[cfg(not(feature = "remill"))]
            { return Err("--lifter=remill-ffi requires --features remill".into()); }
        }
        "auto" => match RemillCliLifter::discover() {
            Some(l) => Box::new(l),
            None => Box::new(StubLifter),
        },
        other => return Err(format!("unknown --lifter={other}").into()),
    };

    println!("[4] lifter:        {} ({})",
        lifter.name(),
        if lifter.is_real() { "REAL" } else { "stub" });
    let lifted = lifter.lift(bytes, fn_addr as u64, HOST_ARCH, EXPORT_SYMBOL)?;
    println!("    emitted {} bytes of LLVM IR\n", lifted.ir.len());

    // Write the IR to a file for llc to consume
    let out_dir = blueprint_out_dir();
    std::fs::create_dir_all(&out_dir)?;
    let ir_path = out_dir.join("blueprint.ll");
    std::fs::write(&ir_path, &lifted.ir)?;
    println!("    wrote IR  →   {}", ir_path.display());
    println!();
    println!("    --- LLVM IR (first 20 lines) ---");
    for line in lifted.ir.lines().take(20) {
        println!("    │ {}", line);
    }
    let total_lines = lifted.ir.lines().count();
    if total_lines > 20 {
        println!("    │ ... ({} more lines)", total_lines - 20);
    }
    println!();

    // [5] llc -mtriple=wasm32 -filetype=obj
    let obj_path = out_dir.join("blueprint.o");
    println!("[5] llc:           {} → {}", ir_path.display(), obj_path.display());
    run_tool(
        &llc_path(),
        &[
            "-mtriple=wasm32-unknown-unknown",
            "-filetype=obj",
            "-O2",
            "-o",
            obj_path.to_str().unwrap(),
            ir_path.to_str().unwrap(),
        ],
    )?;
    println!("    object size:   {} bytes\n", std::fs::metadata(&obj_path)?.len());

    // [6] wasm-ld --no-entry --export=<sym> -o blueprint.wasm blueprint.o
    let wasm_path = out_dir.join("blueprint.wasm");
    println!(
        "[6] wasm-ld:       {} → {}",
        obj_path.display(),
        wasm_path.display()
    );
    run_tool(
        &wasm_ld_path(),
        &[
            "--no-entry",
            &format!("--export={}", lifted.export_symbol),
            "--allow-undefined",
            "-o",
            wasm_path.to_str().unwrap(),
            obj_path.to_str().unwrap(),
        ],
    )?;

    // [7] hex dump
    let wasm_bytes = std::fs::read(&wasm_path)?;
    println!(
        "[7] final WASM:    {} bytes  (magic = {})",
        wasm_bytes.len(),
        if wasm_bytes.starts_with(b"\0asm") {
            "\\0asm ✓"
        } else {
            "INVALID ✗"
        }
    );
    println!("    hex dump:");
    for (i, chunk) in wasm_bytes.chunks(16).enumerate() {
        print!("      {:#06x}:  ", i * 16);
        for b in chunk {
            print!("{:02x} ", b);
        }
        print!("  ");
        for b in chunk {
            let c = *b;
            print!("{}", if (0x20..0x7f).contains(&c) { c as char } else { '.' });
        }
        println!();
        if i >= 7 {
            // cap output at 8 lines = 128 bytes
            let remaining = wasm_bytes.len().saturating_sub((i + 1) * 16);
            if remaining > 0 {
                println!("      ... ({} more bytes)", remaining);
            }
            break;
        }
    }
    println!();
    println!("done.  Verify with:");
    println!("    {}/wasm-objdump -d {}", llvm_bin_dir(), wasm_path.display());

    Ok(())
}

// ── disassembly ─────────────────────────────────────────────────────────

#[cfg(target_arch = "aarch64")]
fn disassemble(base: u64, bytes: &[u8]) {
    use yaxpeax_arch::{Decoder, U8Reader};
    use yaxpeax_arm::armv8::a64::InstDecoder;

    let decoder = InstDecoder::default();
    let mut reader = U8Reader::new(bytes);
    let mut pc = base;
    for _ in 0..(bytes.len() / 4) {
        match decoder.decode(&mut reader) {
            Ok(inst) => {
                let raw = u32::from_le_bytes(
                    bytes[(pc - base) as usize..(pc - base) as usize + 4]
                        .try_into()
                        .unwrap(),
                );
                let mnemonic = format!("{}", inst);
                println!("      0x{:x}:  {:08x}  {}", pc, raw, mnemonic);
                pc += 4;
                if mnemonic.starts_with("ret") {
                    break;
                }
            }
            Err(e) => {
                println!("      0x{:x}:  (decode error: {:?})", pc, e);
                break;
            }
        }
    }
}

#[cfg(target_arch = "x86_64")]
fn disassemble(_base: u64, bytes: &[u8]) {
    println!(
        "      (x86_64 disassembly not wired in this blueprint — would use \
         yaxpeax-x86. {} bytes available.)",
        bytes.len()
    );
}

// ── tool paths ──────────────────────────────────────────────────────────

fn llvm_bin_dir() -> String {
    // Default to homebrew's llvm@21 install; override via $LLVM_BIN.
    std::env::var("LLVM_BIN")
        .unwrap_or_else(|_| "/opt/homebrew/opt/llvm@21/bin".to_string())
}

fn llc_path() -> PathBuf {
    PathBuf::from(format!("{}/llc", llvm_bin_dir()))
}

fn wasm_ld_path() -> PathBuf {
    // wasm-ld ships with lld, not llvm; brew puts it at lld@21.
    std::env::var("WASM_LD")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/opt/homebrew/opt/lld@21/bin/wasm-ld"))
}

fn blueprint_out_dir() -> PathBuf {
    // Sit alongside the crate so re-running doesn't pollute the
    // project-wide target/ — and so artifacts survive `cargo clean`
    // in the main workspace.
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("out")
}

fn run_tool(prog: &Path, args: &[&str]) -> Result<(), String> {
    let out = Command::new(prog)
        .args(args)
        .output()
        .map_err(|e| format!("failed to spawn {}: {}", prog.display(), e))?;
    if !out.status.success() {
        return Err(format!(
            "{} failed: {}\n--- stderr ---\n{}",
            prog.display(),
            out.status,
            String::from_utf8_lossy(&out.stderr),
        ));
    }
    Ok(())
}
