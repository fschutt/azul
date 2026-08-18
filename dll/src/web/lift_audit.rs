//! Startup lift-audit: refuse to serve a build that will crash at runtime.
//!
//! Previously the "will this lift misbehave?" signal was scattered advisory
//! logs plus runtime recorders that only fire AFTER the crash; this module is
//! the startup gate. Policy: aggressive by default — fire on every suspect
//! finding, then debug each one and either fix it or add it to the reviewed
//! allowlist. Catching a mis-lift at startup is always cheaper than hitting
//! the broken branch at runtime.
//!
//! Checks, fatal-first (F = refuses startup under strict mode, W = warn):
//!
//!   F1 stub mini            — transpiler available but the mini has no
//!                             exports (e.g. the dlsym-on-static-exe bug:
//!                             every page load fails at bootstrap)
//!   F2 remill lift failures — functions Leaf-stubbed because remill/llc
//!                             CRASHED. A crash stub returns 0 and never
//!                             writes its sret: the unwritten-sret silent-
//!                             corruption class (capacity_overflow bug).
//!   F3 unknown env imports  — the final wasm imports a symbol the loader
//!                             provably does not implement; the loader's
//!                             Proxy zero-stubs it and the first call
//!                             returns garbage silently.
//!   F4 untranslated natptrs — 8-aligned values inside the native image
//!                             range survive in mirrored data segments
//!                             (the hashbrown wild-deref class; healthy
//!                             builds scan 0).
//!   F5 __remill_error fns   — undecoded instructions. FATAL by default
//!                             (aggressive policy); COMISS/UCOMISS sNaN
//!                             semantics emit __remill_error on paths that
//!                             are benign in practice, so debugged-and-
//!                             cleared functions go into the reviewed
//!                             allowlist (`lift_audit_allowlist.txt`) with
//!                             a reason, which downgrades them to a note.
//!   W2 missing_block fns    — unrecovered control flow; usually benign
//!                             record-then-complete tails, the runtime
//!                             recorder at 0x400FC tracks live hits.
//!
//! `AZ_LIFT_STRICT=0` downgrades every fatal to a warning (serve anyway);
//! unset or any other value = strict.

/// Result of auditing one wasm artifact.
pub struct WasmAudit {
    pub export_count: usize,
    pub import_count: usize,
    pub unknown_imports: Vec<String>,
    /// (count of native-range qwords, total data bytes scanned)
    pub natptr_hits: usize,
    pub data_bytes: usize,
    /// Parse failed — counted as its own loud warning, never silently ok.
    pub parse_ok: bool,
}

fn leb_u32(b: &[u8], p: &mut usize) -> Option<u32> {
    let mut r: u32 = 0;
    let mut s = 0u32;
    loop {
        let byte = *b.get(*p)?;
        *p += 1;
        r |= ((byte & 0x7f) as u32) << s;
        if byte & 0x80 == 0 {
            return Some(r);
        }
        s += 7;
        if s >= 35 {
            return None;
        }
    }
}

fn leb_i32(b: &[u8], p: &mut usize) -> Option<i32> {
    let mut r: i64 = 0;
    let mut s = 0u32;
    loop {
        let byte = *b.get(*p)?;
        *p += 1;
        r |= ((byte & 0x7f) as i64) << s;
        s += 7;
        if byte & 0x80 == 0 {
            if s < 64 && byte & 0x40 != 0 {
                r |= -1i64 << s;
            }
            return Some(r as i32);
        }
        if s >= 35 {
            return None;
        }
    }
}

fn skip_limits(b: &[u8], p: &mut usize) -> Option<()> {
    let flags = leb_u32(b, p)?;
    leb_u32(b, p)?;
    if flags & 1 != 0 {
        leb_u32(b, p)?;
    }
    Some(())
}

/// Env imports the loader really implements (loader_js.rs realEnv + AZ_MATH
/// + boundary wiring). Anything NOT matched here resolves to the loader's
/// Proxy zero-stub — a silent-garbage source, which is the point of F3.
/// Keep in sync with loader_js.rs when adding real impls.
fn import_is_provided(module: &str, name: &str) -> bool {
    if module != "env" {
        return false;
    }
    if name.starts_with("sub_")            // boundary imports, wired per-manifest
        || name.starts_with("__remill_")   // intrinsics (read/write/atomic/cas/undef)
        || name.starts_with("__az")        // resolver/dispatch/probe hooks
    {
        return true;
    }
    matches!(
        name,
        "memory"
            | "__indirect_function_table"
            | "memset"
            | "memcpy"
            | "memmove"
            | "__multi3"
            | "__udivti3"
            | "__divti3"
            | "__umodti3"
            | "__modti3"
            | "sqrtf" | "sqrt"
            | "fmaxf" | "fminf" | "fmax" | "fmin"
            | "roundf" | "round"
            | "fabsf" | "fabs"
            | "floorf" | "floor"
            | "ceilf" | "ceil"
            | "truncf" | "trunc"
            | "powf" | "pow"
            | "fmodf" | "fmod"
            | "expf" | "exp" | "logf" | "log"
            | "sinf" | "sin" | "cosf" | "cos" | "tanf" | "tan"
            | "atan2f" | "atan2" | "atanf" | "atan"
            | "asinf" | "asin" | "acosf" | "acos"
    )
}

/// Parse one wasm binary: imports (section 2), export count (section 7),
/// active data segments (section 11) scanned for 8-aligned qwords inside
/// `img` = the native image's [lo, hi) VA range.
pub fn audit_wasm(bytes: &[u8], img: Option<(u64, u64)>) -> WasmAudit {
    let mut out = WasmAudit {
        export_count: 0,
        import_count: 0,
        unknown_imports: Vec::new(),
        natptr_hits: 0,
        data_bytes: 0,
        parse_ok: false,
    };
    if bytes.len() < 8 || &bytes[0..4] != b"\0asm" {
        return out;
    }
    let b = bytes;
    let mut p = 8usize;
    let parse = (|| -> Option<()> {
        while p < b.len() {
            let id = *b.get(p)?;
            p += 1;
            let size = leb_u32(b, &mut p)? as usize;
            let end = p.checked_add(size)?;
            if end > b.len() {
                return None;
            }
            match id {
                2 => {
                    let mut q = p;
                    let count = leb_u32(b, &mut q)?;
                    for _ in 0..count {
                        let ml = leb_u32(b, &mut q)? as usize;
                        let module = std::str::from_utf8(b.get(q..q + ml)?).ok()?;
                        q += ml;
                        let nl = leb_u32(b, &mut q)? as usize;
                        let name = std::str::from_utf8(b.get(q..q + nl)?).ok()?;
                        q += nl;
                        out.import_count += 1;
                        if !import_is_provided(module, name) {
                            out.unknown_imports.push(format!("{module}.{name}"));
                        }
                        match *b.get(q)? {
                            0x00 => {
                                q += 1;
                                leb_u32(b, &mut q)?;
                            }
                            0x01 => {
                                q += 1;
                                q += 1; // reftype
                                skip_limits(b, &mut q)?;
                            }
                            0x02 => {
                                q += 1;
                                skip_limits(b, &mut q)?;
                            }
                            0x03 => {
                                q += 1;
                                q += 2; // valtype + mutability
                            }
                            _ => return None,
                        }
                    }
                }
                7 => {
                    let mut q = p;
                    out.export_count = leb_u32(b, &mut q)? as usize;
                }
                11 => {
                    let mut q = p;
                    let count = leb_u32(b, &mut q)?;
                    for _ in 0..count {
                        let flags = leb_u32(b, &mut q)?;
                        let mut seg_addr: Option<u64> = None;
                        if flags == 2 {
                            leb_u32(b, &mut q)?; // memidx
                        }
                        if flags == 0 || flags == 2 {
                            // offset expr: expect i32.const N end
                            if *b.get(q)? == 0x41 {
                                q += 1;
                                seg_addr = Some(leb_i32(b, &mut q)? as u32 as u64);
                            } else {
                                // non-const offset (globals) — skip expr bytes
                                while *b.get(q)? != 0x0B {
                                    q += 1;
                                }
                            }
                            if *b.get(q)? != 0x0B {
                                return None;
                            }
                            q += 1;
                        }
                        let len = leb_u32(b, &mut q)? as usize;
                        let data = b.get(q..q + len)?;
                        out.data_bytes += len;
                        if let (Some(addr), Some((lo, hi))) = (seg_addr, img) {
                            // 8-aligned absolute addresses only — same rule as
                            // scripts/m9_e2e/natptr-scan.mjs.
                            let mis = (addr % 8) as usize;
                            let start = if mis == 0 { 0 } else { 8 - mis };
                            let mut i = start;
                            while i + 8 <= data.len() {
                                let v = u64::from_le_bytes(
                                    data[i..i + 8].try_into().unwrap(),
                                );
                                if v >= lo && v < hi {
                                    out.natptr_hits += 1;
                                }
                                i += 8;
                            }
                        }
                        q += len;
                    }
                }
                _ => {}
            }
            p = end;
        }
        Some(())
    })();
    out.parse_ok = parse.is_some();
    out
}

/// Locate the running native image containing `probe_addr`: walk 64 KiB
/// granules down to the PE 'MZ' header, read `SizeOfImage`. The walk stays
/// inside the image's own mapped range until it lands exactly on the base,
/// so every read is backed.
pub fn native_image_range(probe_addr: usize) -> Option<(u64, u64)> {
    let mut base = (probe_addr as u64) & !0xFFFF;
    for _ in 0..0x8000 {
        unsafe {
            let mz = *(base as *const u16);
            if mz == 0x5A4D {
                let e_lfanew = *((base + 0x3C) as *const u32) as u64;
                if e_lfanew < 0x1000 {
                    let nt = base + e_lfanew;
                    if *(nt as *const u32) == 0x0000_4550 {
                        // Signature(4) + IMAGE_FILE_HEADER(20) +
                        // OptionalHeader64.SizeOfImage @ +0x38 = nt + 0x50.
                        let size = *((nt + 0x50) as *const u32) as u64;
                        return Some((base, base + size));
                    }
                }
            }
        }
        base = base.checked_sub(0x10000)?;
    }
    None
}

/// Reviewed allowlist for F5: one fn-name per line, `#` comments. A name
/// listed here means "this function's __remill_error sites were debugged
/// and are benign" (e.g. COMISS sNaN paths). Grows ONLY through debugging;
/// entries carry their reason as a trailing comment in the file.
const ALLOWLIST: &str = include_str!("lift_audit_allowlist.txt");

fn allowlisted(fn_name: &str) -> bool {
    ALLOWLIST.lines().any(|l| {
        let entry = l.split('#').next().unwrap_or("").trim();
        !entry.is_empty() && entry == fn_name
    })
}

/// Print the audit block and return whether FATAL findings exist.
///
/// `artifacts`: (label, bytes) for every wasm the server is about to serve.
/// `lift_failures`: count of remill/llc-crash Leaf stubs (transpiler total).
/// `preflight`: per-fn `(name, __remill_error sites, missing_block sites)`.
pub fn run(
    artifacts: &[(&str, &[u8])],
    transpiler_available: bool,
    lift_failures: u32,
    preflight: &[(String, u32, u32)],
) -> bool {
    let img = native_image_range(crate::web::eventloop::AzStartup_alloc as usize);
    if img.is_none() {
        eprintln!("[azul-web][lift-audit] ⚠ could not locate the native image range — natptr check skipped");
    }
    let mut fatal = false;

    for (label, bytes) in artifacts {
        if bytes.len() <= 8 {
            // The 8-byte placeholder: only fatal for the mini when a real
            // transpiler ran (stub-by-config is a legitimate mode).
            if *label == "mini" && transpiler_available {
                eprintln!("[azul-web][lift-audit] ✗ F1 {label}: STUB ({} bytes) — bootstrap WILL fail in every client", bytes.len());
                fatal = true;
            }
            continue;
        }
        let a = audit_wasm(bytes, img);
        if !a.parse_ok {
            eprintln!("[azul-web][lift-audit] ⚠ {label}: wasm parse failed — audit incomplete (treating as suspect)");
            continue;
        }
        if *label == "mini" && transpiler_available && a.export_count == 0 {
            eprintln!("[azul-web][lift-audit] ✗ F1 {label}: 0 exports — bootstrap WILL fail in every client");
            fatal = true;
        }
        if !a.unknown_imports.is_empty() {
            let mut names = a.unknown_imports.clone();
            names.truncate(12);
            eprintln!(
                "[azul-web][lift-audit] ✗ F3 {label}: {} env import(s) the loader does not implement (zero-stubbed at runtime): {}{}",
                a.unknown_imports.len(),
                names.join(", "),
                if a.unknown_imports.len() > 12 { ", …" } else { "" },
            );
            fatal = true;
        }
        if a.natptr_hits > 0 {
            eprintln!(
                "[azul-web][lift-audit] ✗ F4 {label}: {} untranslated native pointer(s) in {} data bytes (wild-deref class; healthy = 0)",
                a.natptr_hits, a.data_bytes,
            );
            fatal = true;
        }
    }

    if lift_failures > 0 {
        eprintln!(
            "[azul-web][lift-audit] ✗ F2 {lift_failures} function(s) Leaf-stubbed by remill/llc CRASHES — unwritten-sret corruption class; see the per-fn warnings above",
        );
        fatal = true;
    }

    // F5: __remill_error, aggressive — every non-allowlisted fn is fatal.
    let mut err_new: Vec<&(String, u32, u32)> =
        preflight.iter().filter(|(n, e, _)| *e > 0 && !allowlisted(n)).collect();
    let err_allowed = preflight.iter().filter(|(n, e, _)| *e > 0 && allowlisted(n)).count();
    let mb_fns = preflight.iter().filter(|(_, e, mb)| *e == 0 && *mb > 0).count();
    if !err_new.is_empty() {
        err_new.sort_by(|a, b| b.1.cmp(&a.1));
        eprintln!(
            "[azul-web][lift-audit] ✗ F5 {} fn(s) with UNEXPLAINED __remill_error (sNaN-guard and ud2 classes already excluded) — debug each; a verified-benign residual goes into lift_audit_allowlist.txt WITH its reason, anything else is a real mis-lift:",
            err_new.len(),
        );
        for (name, e, mb) in err_new.iter().take(20) {
            eprintln!("[azul-web][lift-audit]     {e:>3} error {mb:>3} missing  {name}");
        }
        if err_new.len() > 20 {
            eprintln!("[azul-web][lift-audit]     … {} more (AZ_PREFLIGHT=1 for the full list)", err_new.len() - 20);
        }
        fatal = true;
    }
    if err_allowed > 0 {
        eprintln!("[azul-web][lift-audit] ✓ F5 {err_allowed} fn(s) with __remill_error covered by the reviewed allowlist");
    }
    if mb_fns > 0 {
        eprintln!(
            "[azul-web][lift-audit] ⚠ W2 {mb_fns} fn(s) contain __remill_missing_block — usually benign tails; runtime recorder @0x400FC tracks live hits",
        );
    }
    if !fatal && lift_failures == 0 && err_new.is_empty() {
        eprintln!("[azul-web][lift-audit] ✓ CLEAN — no fatal findings, no unreviewed __remill_error, no crash stubs");
    }
    fatal
}
