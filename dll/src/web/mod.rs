//! Web backend for Azul (`AZ_BACKEND=web://ip:port`).
//!
//! When enabled, Azul runs as an HTTP server instead of opening a native
//! window. The layout callback executes natively and the resulting DOM is
//! rendered to HTML with a CSS stylesheet (per-node-ID rules).
//!
//! # Architecture
//!
//! ```text
//! AZ_BACKEND=web://127.0.0.1:8080
//!   → AzBackend::Web(addr)
//!   → run_web(app_data, config, fc_cache, font_registry, root_window, addr)
//!     → Phase A: classify API functions (stubbed)
//!     → Phase B: generate azul-mini.wasm (stubbed)
//!     → Phase C: discover + transpile callbacks (discovery functional, transpile stubbed)
//!     → Phase D: pre-render ALL routes → HTML with stylesheet
//!     → Phase E: start HTTP server, serve pages + /az/img/ + /az/font/
//! ```

pub mod config;
pub mod server;
pub mod html_render;
pub mod loader_js;
pub mod classify;
pub mod transpiler;
#[cfg(feature = "web-transpiler")]
pub mod transpiler_remill;
#[cfg(feature = "web-transpiler")]
pub mod symbol_table;
#[cfg(feature = "web-transpiler-static")]
pub mod native_remill;
pub mod eventloop;
pub mod headless;
pub mod hydration;

/// Whether M10-D per-fn WASM sharding is active. Always `false` when the
/// transpiler (and thus `symbol_table`) isn't compiled in — keeps the non-
/// transpiler `web` build working without sprinkling `#[cfg]` at every call.
#[inline]
fn shards_enabled() -> bool {
    #[cfg(feature = "web-transpiler")]
    {
        symbol_table::shards_enabled()
    }
    #[cfg(not(feature = "web-transpiler"))]
    {
        false
    }
}

/// Framework-internal eventloop symbols lifted from libazul at server
/// startup, linked into `azul-mini.wasm`. Hand-written in
/// [`eventloop`]; not in `api.json`, so language bindings never see
/// them. `run_web` iterates this list, `dlsym`s each, lifts via
/// [`transpiler_remill::RemillTranspiler::lift_function`], then links
/// the resulting `.o` files into one WASM module via `wasm-ld`.
///
/// Keep in sync with the `#[no_mangle] pub extern "C"` exports in
/// [`eventloop`]. Wired into the lift loop in M8.2.
pub const EVENTLOOP_SYMBOLS: &[&str] = &[
    "AzStartup_alloc",
    "AzStartup_free",
    // WEB-FONT-VIA-JS: JS registers a fallback-font buffer it wrote into wasm memory.
    "AzStartup_setFallbackFont",
    "AzStartup_init",
    "AzStartup_hydrate",
    "AzStartup_dispatchEvent",
    "AzStartup_registerStateDeserializer",
    // M9-2: Layout-cb wasm-side LayoutCallbackInfo builder.
    "AzStartup_buildLayoutInfo",
    // M9-3: Layout-cb dispatch infrastructure.
    "AzStartup_setLayoutCbTableIdx",
    "AzStartup_setRefAny",
    "AzStartup_initLayoutCache",
    "AzStartup_getCurrentDomPtr",
    "AzStartup_getLastLayoutStatus",
    "AzStartup_getCascadeProbe",
    "AzStartup_pokeLastLayout",
    // M9-4: WASM-side hit-test (stub, returns last registered cb node).
    "AzStartup_registerCbNode",
    // 2026-06-10: per-EventFilter dispatch — registerCbNode + the event kind.
    "AzStartup_registerCbNodeKind",
    "AzStartup_hitTest",
    // M9-5: TLV patch emission.
    "AzStartup_buildCounterPatch",
    // M9-6: wasm-resident dispatch state setters.
    "AzStartup_setModelPtr",
    "AzStartup_setDisplayNode",
    // M11 Sprint 1: StyledDom hydrate — runs the cascade
    // (`StyledDom::create(&mut dom, Css::empty())`) wasm-side via
    // the S1.A transitive lift pipeline. `getStyledDomNodeCount`
    // returns the StyledDom's node count for cross-checking
    // against `getDomNodeCount` (the raw AzDom walker).
    "AzStartup_hydrateStyledDom",
    "AzStartup_isStyledDomHydrated",
    "AzStartup_getDomNodeCount",
    "AzStartup_getStyledDomNodeCount",
    "AzStartup_getStyledDomPtr",
    // M11 Sprint 1.C / Sprint 2: layout solver + positioned-rect
    // cache. AzStartup_hitTest now consumes the cache for real
    // bbox-walk dispatch.
    "AzStartup_solveLayout",
    // M12.7: real layout solver (LayoutWindow::layout_and_generate_display_list
    // → taffy block/flex/grid). Same signature as solveLayout.
    "AzStartup_solveLayoutReal",
    "AzStartup_isLayoutSolved",
    "AzStartup_getPositionedRectsLen",
    "AzStartup_getPositionedRectsPtr",
    // M12.7 debug: peek a u32 from wasm linear memory (reads the diag
    // markers the layout solver writes via write_volatile).
    "AzStartup_peekU32",
    // M11 Sprint 3: relayout + generalized patch builder for
    // SetText / SetAttr / SetInlineStyle / RemoveNode / InsertNode.
    // The JS decoder switches on kind.
    "AzStartup_relayout",
    "AzStartup_buildPatch",
    // M11 Sprint 5: VirtualView infrastructure (threshold + provider
    // table-idx). Full auto-virtualization pending Box::new init gap.
    "AzStartup_setAutoVirtualizeThreshold",
    "AzStartup_getAutoVirtualizeThreshold",
    "AzStartup_setVirtualViewProvider",
];

use std::collections::{BTreeMap, HashMap};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use azul_core::callbacks::{CoreCallback, LayoutCallback};
use azul_core::refany::RefAny;
use azul_core::resources::{AppConfig, RouteMatch};
use azul_layout::window_state::WindowCreateOptions;
use rust_fontconfig::FcFontCache;
use rust_fontconfig::registry::FcFontRegistry;

use crate::desktop::shell2::common::WindowError;

/// FNV-1a 64-bit offset basis. Shared with `html_render::content_hash`
/// so that all cache-busting URLs in the web backend use the same hash
/// family.
pub(crate) const FNV_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
pub(crate) const FNV_PRIME: u64 = 0x100000001b3;

/// FNV-1a 64-bit hash, formatted as a 16-char hex string.
pub(crate) fn fnv1a64_hex(data: &[u8]) -> String {
    let mut hash: u64 = FNV_OFFSET_BASIS;
    for byte in data {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    format!("{:016x}", hash)
}

/// A lifted layout-callback WASM module. One per unique
/// `LayoutCallback` fn-pointer across the configured routes.
///
/// **M8.3 wrapper status**: today the bytes are lifted under the
/// canonical Callback wrapper shape, which seeds X0/X1/X2 from the
/// JS-side `(refany_lo, refany_hi, info_ptr)` args but does NOT
/// seed X8 (AArch64 hidden return-buffer pointer for >16B
/// aggregate returns like `AzStyledDom`). Calling the lifted
/// module from JS would write `AzStyledDom` to address 0 → trap.
/// M8.5 introduces a `Pcs::HiddenPtrReturn` variant + a heap-allocated
/// return-buffer wrapper (per Q1 user direction: serialized-bytes
/// return path).
///
/// For M8.3 the bytes still exist + the route serves them, so the
/// `<link rel="preload">` infrastructure in `html_render` warms the
/// browser cache. loader.js doesn't call the layout module yet.
#[derive(Debug, Clone)]
pub struct LayoutWasm {
    /// Resolved symbol name (or `cb_{addr:x}` fallback).
    pub name: String,
    /// FNV-1a 64-bit hash of `name` — the `{hash}` in the served URL.
    pub content_hash: String,
    /// dladdr-resolved symbol address.
    pub fn_addr: usize,
    /// The lifted WASM bytes. Empty if the lift errored (transpiler
    /// unavailable, dlsym miss, etc.); the route handler then 404s.
    pub wasm_bytes: Vec<u8>,
    /// `true` when the bytes came from the real lift; `false` when the
    /// lift errored. Surfaces in `eprintln!` logging for debugging.
    pub is_client_side: bool,
    /// M10-D: canonical addresses of every BoundaryImport surfaced
    /// during this layout cb's transitive lift. See [`BoundaryWasm`].
    pub used_boundaries: Vec<usize>,
}

/// M10-D — one per-fn wasm shard. The sharded build factors every
/// `api.json::Framework` symbol out of the cb / layout / mini bundles
/// into its own wasm. Each cb / layout / mini wasm imports
/// `env.sub_<canonical_synth_hex>` for every boundary it touches; the
/// boundary shard's wasm exports the matching `sub_<canonical_synth_hex>`
/// body. loader.js wires the imports at instantiate-time via the
/// manifest.
#[derive(Debug, Clone)]
pub struct BoundaryWasm {
    /// Canonical (PLT-chased) address — the dedup key used to union
    /// `used_boundaries` sets across all lift outputs.
    pub canonical_addr: usize,
    /// Canonical C-API name (`AzRefAny_clone`, `AzDom_addChild`, …).
    /// Surfaces in the served URL `/az/fn/<name>.<hash>.wasm` for
    /// debugging.
    pub canonical_name: String,
    /// The wasm-export name JS resolves: always
    /// `sub_<canonical_synth_hex>`. Matches the env-import wasm-ld
    /// emits in dependent shards.
    pub body_export: String,
    /// FNV-1a 64-bit hash of the wasm bytes. Used in the served URL.
    pub content_hash: String,
    /// The shard's wasm bytes.
    pub wasm_bytes: Vec<u8>,
    /// Canonical addresses of every OTHER boundary the shard's BFS
    /// transitively references. Used by the orchestrator to ensure
    /// every reachable boundary gets its own shard.
    pub transitive_boundaries: Vec<usize>,
}

/// A discovered callback and its WASM module (if transpiled).
#[derive(Debug, Clone)]
pub struct CallbackWasm {
    /// Callback name (derived from symbol name via dladdr / SymFromAddr).
    pub name: String,
    /// Content hash for cache-busting (FNV-1a 64-bit of `name`).
    pub content_hash: String,
    /// Symbol address — what the lift pipeline reads bytes from. Equal
    /// to `dli_saddr` when dladdr resolved, otherwise the raw stored
    /// fn-ptr. Surfaced from [`DiscoveredCallback::fn_addr`] verbatim.
    pub fn_addr: usize,
    /// Byte window the lift pipeline reads starting at `fn_addr`.
    /// Fixed-size today (see [`LIFT_READ_WINDOW`]); a future revision
    /// could read DWARF / nm to bound it precisely per-symbol.
    pub fn_size: usize,
    /// WASM bytes. Empty until the remill-based transpiler is wired up.
    pub wasm_bytes: Vec<u8>,
    /// Whether this callback can run client-side (transpiled to WASM)
    /// or must fall back to server-side execution.
    pub is_client_side: bool,
    /// M10-D: canonical addresses of every BoundaryImport surfaced
    /// during this cb's transitive lift. Empty in legacy bundled
    /// mode. The orchestrator unions this across every cb / layout /
    /// mini lift, lifts each boundary into its own shard, and serves
    /// the result at `/az/fn/<name>.<hash>.wasm`.
    pub used_boundaries: Vec<usize>,
}

/// One callback found while walking a route's `StyledDom`, bound to a
/// concrete synthetic `az_N` node ID.
#[derive(Debug, Clone)]
pub struct DiscoveredCallback {
    /// `az_N` synthetic node ID within the host route's render.
    pub node_idx: u32,
    /// Resolved symbol name (or `cb_{addr:x}` fallback).
    pub name: String,
    /// FNV-1a 64-bit hash of `name`, used in `/az/cb/{name}.{hash}.wasm`.
    pub content_hash: String,
    /// The underlying callback (carries the fn-pointer usize plus the
    /// optional ctx for managed-FFI hosts).
    pub callback: CoreCallback,
    /// dladdr-resolved symbol address (start of the function in .text).
    /// Equals `callback.cb` when dladdr returned the same value, but
    /// `dli_saddr` may align downward to a symbol boundary when the
    /// stored pointer was already authenticated / offset.
    pub fn_addr: usize,
    /// Conservative byte window the lift pipeline reads from `fn_addr`.
    /// Fixed-size today (see [`LIFT_READ_WINDOW`]).
    pub fn_size: usize,
}

/// Symbol metadata returned by [`resolve_fn_ptr`]. `name` is the
/// dladdr-resolved symbol or a `cb_{addr:x}` fallback; `addr` is the
/// canonical start address of the function (from `dli_saddr` when
/// dladdr succeeded, otherwise the input pointer as-is); `size` is a
/// conservative read window the lift pipeline can pass to remill —
/// remill stops at the first `ret` it sees, so the window only needs
/// to be big enough to span the longest plausible function prologue +
/// body. The fixed `LIFT_READ_WINDOW` covers ~30 instructions on
/// arm64, which is comfortably more than any leaf callback in azul's
/// own surface.
#[derive(Debug, Clone)]
pub struct FnPtrSymbol {
    pub name: String,
    pub addr: usize,
    pub size: usize,
}

/// Fallback byte window used when the SymbolTable doesn't have an
/// exact size for an address. Survives the M8.8 refactor as a
/// safety-net constant; the M8.8 verification checks that lift logs
/// never see a `cb_<hex>` fallback, which is what would surface a
/// missed table entry.
///
/// arm64 instructions are 4 bytes each, so 4 KiB = 1024 instructions —
/// comfortable headroom for the longest libazul functions. remill
/// stops at the first `ret` so over-reading is harmless.
const LIFT_READ_WINDOW: usize = 4096;

// `resolve_macos_arm64_stub` deleted in M8.8 Stage 1. The PLT-stub
// chain is now precomputed in `symbol_table::SymbolTable::chain` at
// server startup by parsing LC_DYSYMTAB.indirectsymoff against
// __TEXT.__stubs.reserved1 — see `symbol_table::ingest_macho_stubs`.
// `resolve_fn_ptr` consumes the chain transparently via
// `SymbolTable::resolve`.

/// Resolve a function pointer to its `(name, addr, size)` triple.
///
/// **M8.8 Stage 1**: when the `SymbolTable` is installed (the normal
/// case after `run_web`'s startup phase), this delegates to it and
/// `resolve()` chases the precomputed PLT-stub chain. Sizes are
/// exact — derived from `(next_symbol_addr - this_addr)` at table
/// build — instead of the legacy flat-4 KiB window.
///
/// Fallback: when the table isn't installed (e.g. during the table
/// build itself, or when the SymbolTable build errored out), the
/// pre-M8.8 dladdr path is preserved. The fallback also runs when
/// the table doesn't know about an address — log loudly so the user
/// can add the missing image to the table's image set.
pub(crate) fn resolve_fn_ptr(fn_ptr: usize) -> FnPtrSymbol {
    #[cfg(feature = "web-transpiler")]
    if let Some(table) = symbol_table::get() {
        if let Some(entry) = table.resolve(fn_ptr) {
            return FnPtrSymbol {
                name: entry.canonical_name.clone(),
                addr: entry.canonical_addr,
                size: if entry.size > 0 { entry.size } else { LIFT_READ_WINDOW },
            };
        }
        // Windows incremental-link thunk (ILT) chase. MSVC's
        // /INCREMENTAL linker (default with /DEBUG) routes every
        // function reference through a 5-byte `E9 rel32` jump island in
        // an early `.text` band; these islands carry NO symbol, so the
        // captured callback fn-ptr lands on the island, not the real
        // function. Lifting the island fails silently: its `jmp <far>`
        // lifts to `__remill_missing_block` (a no-op) because the
        // target is outside the per-fn read window, so the callback
        // body NEVER runs. Chase the island to its target and resolve
        // THAT (which IS a PDB symbol with an exact size). This is the
        // PE analogue of the macOS `__TEXT.__stubs` PLT chase.
        if let Some(target) = chase_ilt_thunk(fn_ptr) {
            if let Some(entry) = table.resolve(target) {
                return FnPtrSymbol {
                    name: entry.canonical_name.clone(),
                    addr: entry.canonical_addr,
                    size: if entry.size > 0 { entry.size } else { LIFT_READ_WINDOW },
                };
            }
        }
    }
    // Pre-M8.8 fallback path.
    resolve_fn_ptr_dladdr(fn_ptr)
}

/// If `addr` points at an MSVC incremental-link jump island
/// (`E9 rel32` → near jump, or `FF 25 disp32` → indirect jump through
/// the IAT), return the island's target address; otherwise `None`.
/// Single-hop: one island never targets another. Only compiled on
/// Windows, where these islands exist.
#[cfg(all(feature = "web-transpiler", target_os = "windows"))]
fn chase_ilt_thunk(addr: usize) -> Option<usize> {
    if addr == 0 {
        return None;
    }
    // SAFETY: addr came from a live function pointer captured during
    // the route walk, so the 6 bytes at the island are mapped, readable
    // .text. We only read.
    let b = unsafe { core::slice::from_raw_parts(addr as *const u8, 6) };
    if b[0] == 0xE9 {
        let rel = i32::from_le_bytes([b[1], b[2], b[3], b[4]]);
        return Some((addr as isize).wrapping_add(5).wrapping_add(rel as isize) as usize);
    }
    if b[0] == 0xFF && b[1] == 0x25 {
        let disp = i32::from_le_bytes([b[2], b[3], b[4], b[5]]);
        let slot = (addr as isize).wrapping_add(6).wrapping_add(disp as isize) as usize;
        let target = unsafe { core::ptr::read_unaligned(slot as *const u64) } as usize;
        if target >= 0x1_0000 {
            return Some(target);
        }
    }
    None
}

#[cfg(all(feature = "web-transpiler", not(target_os = "windows")))]
fn chase_ilt_thunk(_addr: usize) -> Option<usize> {
    None
}

/// dladdr-only resolver used as the fallback when the SymbolTable
/// either isn't installed or doesn't have an entry for an address.
/// Returns `cb_<hex>` if dladdr can't name the address — the M8.8
/// verification checks that this fallback NEVER fires in the lift
/// logs (any cb_<hex> indicates a missed symbol classification).
fn resolve_fn_ptr_dladdr(fn_ptr: usize) -> FnPtrSymbol {
    #[cfg(any(target_os = "linux", target_os = "macos", target_os = "ios", target_os = "android"))]
    unsafe {
        #[repr(C)]
        struct DlInfo {
            dli_fname: *const core::ffi::c_char,
            dli_fbase: *mut core::ffi::c_void,
            dli_sname: *const core::ffi::c_char,
            dli_saddr: *mut core::ffi::c_void,
        }
        extern "C" {
            fn dladdr(addr: *const core::ffi::c_void, info: *mut DlInfo) -> core::ffi::c_int;
        }
        let mut info = DlInfo {
            dli_fname: core::ptr::null(),
            dli_fbase: core::ptr::null_mut(),
            dli_sname: core::ptr::null(),
            dli_saddr: core::ptr::null_mut(),
        };
        if dladdr(fn_ptr as *const _, &mut info) != 0 && !info.dli_sname.is_null() {
            if let Ok(s) = core::ffi::CStr::from_ptr(info.dli_sname).to_str() {
                if !s.is_empty() {
                    // Strip the macOS leading `_` so the name matches
                    // what the SymbolTable would have returned.
                    let canonical = s.strip_prefix('_').unwrap_or(s).to_string();
                    return FnPtrSymbol {
                        name: canonical,
                        addr: info.dli_saddr as usize,
                        size: LIFT_READ_WINDOW,
                    };
                }
            }
        }
    }
    FnPtrSymbol {
        name: format!("cb_{:016x}", fn_ptr),
        addr: fn_ptr,
        size: LIFT_READ_WINDOW,
    }
}

/// Back-compat shim — still used by html_render for `<link rel="preload">`
/// URL generation. New code should call `resolve_fn_ptr` for full metadata.
pub(crate) fn resolve_fn_ptr_name(fn_ptr: usize) -> String {
    resolve_fn_ptr(fn_ptr).name
}

/// Resolve a symbol name to its address in the current process.
///
/// Uses `dlsym(RTLD_DEFAULT, name)` on unix-like platforms. Returns
/// `None` if the symbol is undefined or `dlsym` returns null
/// (architecturally the same as "symbol not found" for our use case
/// — we never expose `dlerror()` because the only consumer
/// (`run_web`'s eventloop-lift loop) treats any failure as
/// "fall back to a stub eventloop").
///
/// Used by M8.2 to recover the address of every `AzStartup_*` symbol
/// listed in [`EVENTLOOP_SYMBOLS`] so the remill lift pipeline can
/// read function bytes from `.text`.
pub(crate) fn dlsym_self(name: &str) -> Option<usize> {
    #[cfg(any(target_os = "linux", target_os = "macos", target_os = "ios", target_os = "android"))]
    unsafe {
        // RTLD_DEFAULT: look up the symbol in the global scope of the
        // running process (matches the running dylib's exported
        // surface). macOS: -2 cast to handle; Linux: 0 cast to handle.
        // Using `core::ptr::null_mut()` works on both because the
        // dlopen handle is opaque pointer-sized and dlsym checks for
        // sentinel values internally on macOS.
        #[cfg(target_os = "macos")]
        const RTLD_DEFAULT: *mut core::ffi::c_void = (-2_isize) as *mut core::ffi::c_void;
        #[cfg(not(target_os = "macos"))]
        const RTLD_DEFAULT: *mut core::ffi::c_void = core::ptr::null_mut();

        extern "C" {
            fn dlsym(
                handle: *mut core::ffi::c_void,
                symbol: *const core::ffi::c_char,
            ) -> *mut core::ffi::c_void;
        }
        let c_name = std::ffi::CString::new(name).ok()?;
        let ptr = dlsym(RTLD_DEFAULT, c_name.as_ptr());
        if ptr.is_null() {
            None
        } else {
            Some(ptr as usize)
        }
    }
    #[cfg(target_os = "windows")]
    unsafe {
        // RTLD_DEFAULT equivalent: GetProcAddress over every loaded
        // non-system module. The AzStartup_* eventloop symbols are
        // plain C exports of azul.dll, and user callbacks may live in
        // the host exe, so both are probed. System DLLs are skipped
        // for parity with the dyld shared-cache filter (and to avoid
        // accidentally resolving a same-named ucrt export).
        type Hmodule = *mut core::ffi::c_void;
        #[link(name = "kernel32")]
        extern "system" {
            fn GetCurrentProcess() -> *mut core::ffi::c_void;
            fn K32EnumProcessModules(
                process: *mut core::ffi::c_void,
                modules: *mut Hmodule,
                cb: u32,
                needed: *mut u32,
            ) -> i32;
            fn GetModuleFileNameW(module: Hmodule, filename: *mut u16, size: u32) -> u32;
            fn GetProcAddress(
                module: Hmodule,
                name: *const core::ffi::c_char,
            ) -> *mut core::ffi::c_void;
        }
        let c_name = std::ffi::CString::new(name).ok()?;
        let mut modules: Vec<Hmodule> = vec![core::ptr::null_mut(); 1024];
        let mut needed: u32 = 0;
        if K32EnumProcessModules(
            GetCurrentProcess(),
            modules.as_mut_ptr(),
            (modules.len() * core::mem::size_of::<Hmodule>()) as u32,
            &mut needed,
        ) == 0
        {
            return None;
        }
        let count = (needed as usize / core::mem::size_of::<Hmodule>()).min(modules.len());
        for &module in &modules[..count] {
            if module.is_null() {
                continue;
            }
            let mut buf = [0u16; 1024];
            let len = GetModuleFileNameW(module, buf.as_mut_ptr(), buf.len() as u32);
            if len > 0 {
                let path = std::path::PathBuf::from({
                    use std::os::windows::ffi::OsStringExt;
                    std::ffi::OsString::from_wide(&buf[..len as usize])
                });
                let lower = path.to_string_lossy().to_ascii_lowercase();
                if lower.contains(":\\windows\\") || lower.contains(":/windows/") {
                    continue;
                }
            }
            let ptr = GetProcAddress(module, c_name.as_ptr());
            if !ptr.is_null() {
                return Some(ptr as usize);
            }
        }
        None
    }
    #[cfg(not(any(
        target_os = "linux",
        target_os = "macos",
        target_os = "ios",
        target_os = "android",
        target_os = "windows"
    )))]
    {
        let _ = name;
        None
    }
}

/// Aggregate `DiscoveredCallback`s from every rendered route into a
/// deduplicated `Vec<CallbackWasm>` keyed by function pointer.
///
/// Each unique fn-ptr becomes one `CallbackWasm` entry whose `wasm_bytes`
/// stays empty — the remill lift that fills them in is the final, untouched
/// step of Phase C. The output drives the server's `/az/cb/{name}.wasm`
/// route and (via the same `name` + `content_hash`) the per-page
/// `<link rel="preload">` hints emitted by `html_render`.
pub fn discover_and_transpile_callbacks(
    discovered_per_route: &BTreeMap<String, Vec<DiscoveredCallback>>,
) -> Vec<CallbackWasm> {
    // Dedup by the *resolved* symbol address — two callbacks that
    // ended up at the same function (e.g. the same `on_click` reused
    // across nodes) should produce one WASM module. `callback.cb` and
    // `fn_addr` may differ when dladdr aligned the stored pointer
    // downward to a symbol boundary; the symbol address is the right
    // key for cache + dispatch.
    let mut seen: BTreeMap<usize, ()> = BTreeMap::new();
    let mut out = Vec::new();

    // M5: get a transpiler instance. With `web-transpiler` feature
    // OFF: `StubTranspiler` whose `lift_function` always errors —
    // we fall back to the M3 no-op WASM. With it ON + remill-lift-17
    // discoverable: `RemillTranspiler` runs the real subprocess
    // pipeline (`remill-lift-17 → llc → wasm-ld`) and we ship the
    // lifted module.
    let transpiler = transpiler::default_transpiler();
    let transpiler_available = transpiler.is_available();
    eprintln!(
        "[azul-web] transpiler: {} (available={})",
        transpiler.name(),
        transpiler_available
    );

    for (_pattern, list) in discovered_per_route.iter() {
        for d in list {
            if seen.insert(d.fn_addr, ()).is_none() {
                let (wasm_bytes, is_client_side, used_boundaries) = lift_or_noop(
                    transpiler.as_ref(),
                    transpiler_available,
                    &d.name,
                    d.fn_addr,
                    d.fn_size,
                    // Widget callbacks (button on_click, etc.) all share
                    // the canonical `Callback` shape: `fn(AzRefAny,
                    // AzCallbackInfo) -> AzUpdate`. M9-1+ can extend
                    // `DiscoveredCallback` with a per-attachment-site
                    // typedef tag (CheckBoxOnToggleCallback, …) once
                    // the discovery side carries one through.
                    "Callback",
                );
                out.push(CallbackWasm {
                    name: d.name.clone(),
                    content_hash: d.content_hash.clone(),
                    fn_addr: d.fn_addr,
                    fn_size: d.fn_size,
                    wasm_bytes,
                    is_client_side,
                    used_boundaries,
                });
            }
        }
    }
    out
}

/// Try the real lift; on any failure return the M3 no-op WASM. The
/// `is_client_side` flag tracks whether the bytes are a real lift
/// (true) or a no-op fallback (false) so M5 debugging can distinguish
/// the two in logs / DevTools.
///
/// Lift failures we tolerate (and silently no-op):
///   - transpiler not available (web-transpiler feature off, or
///     remill-lift-17 / llc / wasm-ld binaries missing on the host).
///   - lift errored at one of the pipeline stages (remill, llc, or
///     wasm-ld — see `TranspileError.reason` for the per-stage cause).
/// Either way the no-op fallback keeps the browser-side dispatch
/// functional (the callback is a no-op but the JS path still works);
/// user direction is `complex callbacks broken-for-now is acceptable`.
fn lift_or_noop(
    transpiler: &dyn transpiler::Transpiler,
    transpiler_available: bool,
    name: &str,
    fn_addr: usize,
    fn_size: usize,
    kind: &str,
) -> (Vec<u8>, bool, Vec<usize>) {
    if !transpiler_available {
        return (emit_noop_callback_wasm(), false, Vec::new());
    }
    match transpiler.lift_function(name, fn_addr, fn_size, kind) {
        Ok(module) => {
            eprintln!(
                "[azul-web]   lifted: {} → {} bytes ({} exports, {} mini imports, \
                 {} boundary imports) [kind={}]",
                name,
                module.bytes.len(),
                module.exports.len(),
                module.imports_from_mini.len(),
                module.used_boundaries.len(),
                kind,
            );
            (module.bytes, true, module.used_boundaries)
        }
        Err(e) => {
            eprintln!(
                "[azul-web]   lift failed for {}: {} — falling back to no-op",
                name, e.reason
            );
            (emit_noop_callback_wasm(), false, Vec::new())
        }
    }
}

/// Export name for every per-callback WASM module. loader.js calls
/// `instance.exports[WASM_CALLBACK_EXPORT]` regardless of the underlying
/// C symbol name (which is in the URL path for cache addressability,
/// not for runtime lookup). Stays stable across M3 (no-op) → M5
/// (real lift) → M7 (intercept-pass real callback) so the JS side
/// doesn't have to track per-callback export names.
pub const WASM_CALLBACK_EXPORT: &str = "callback";

/// Emit a hand-rolled minimum-viable WASM module exporting a single
/// `(i32, i32) -> i32` function whose body is `i32.const 0`. ~43 bytes
/// for the canonical export name. Sufficient for M3-M4 to validate
/// that per-callback WASM URLs are served correctly and that
/// `WebAssembly.instantiateStreaming` succeeds on the browser side.
///
/// The signature `(i32, i32) -> i32` is a placeholder — loader.js
/// calls it with `(0, 0)` until the real arg-marshalling lands in
/// M7+. The return `0` encodes `Update::DoNothing` so the browser's
/// follow-up POST/render path treats the click as a no-op (per the
/// user direction `no server fallback by default` the POST isn't
/// fired anyway; this is just a defensive return value).
fn emit_noop_callback_wasm() -> Vec<u8> {
    // WASM binary format reference: https://webassembly.github.io/spec/core/binary
    // Sections we need:
    //   - Magic + version (8 bytes)
    //   - Type section   (1) — one signature  (i32, i32) -> i32
    //   - Function section (3) — one function using type 0
    //   - Export section (7) — one export of function 0 under WASM_CALLBACK_EXPORT
    //   - Code section   (10) — function body: i32.const 0 ; end
    //
    // Sizes use LEB128 unsigned; for all values we emit here the
    // single-byte form is sufficient (every length < 128).
    const I32: u8 = 0x7F;
    let export_name = WASM_CALLBACK_EXPORT.as_bytes();

    let mut out: Vec<u8> = Vec::with_capacity(64);

    // Magic + version
    out.extend_from_slice(&[0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00]);

    // Type section: id=1, body = num_types(1) + [functype]
    //   functype = 0x60 0x02 i32 i32 0x01 i32
    let type_body: [u8; 7] = [0x01, 0x60, 0x02, I32, I32, 0x01, I32];
    out.push(0x01);
    out.push(type_body.len() as u8);
    out.extend_from_slice(&type_body);

    // Function section: id=3, body = num_funcs(1) + type_idx(0)
    out.extend_from_slice(&[0x03, 0x02, 0x01, 0x00]);

    // Export section: id=7, body = num_exports(1) + [export]
    //   export = name_len + name_bytes + kind(0=fn) + fn_idx(0)
    let mut export_body: Vec<u8> = Vec::with_capacity(4 + export_name.len());
    export_body.push(0x01); // num_exports
    export_body.push(export_name.len() as u8);
    export_body.extend_from_slice(export_name);
    export_body.push(0x00); // export kind: function
    export_body.push(0x00); // function index: 0
    out.push(0x07);
    out.push(export_body.len() as u8);
    out.extend_from_slice(&export_body);

    // Code section: id=10, body = num_funcs(1) + [code]
    //   code = body_size + locals_count(0) + i32.const 0 (0x41 0x00) + end (0x0B)
    //   body bytes after body_size = 4
    let code_body: [u8; 6] = [0x01, 0x04, 0x00, 0x41, 0x00, 0x0B];
    out.push(0x0A);
    out.push(code_body.len() as u8);
    out.extend_from_slice(&code_body);

    out
}

/// Minimal valid WASM module: `\0asm` magic + version 1.
const WASM_HEADER: [u8; 8] = [
    0x00, 0x61, 0x73, 0x6D, // \0asm magic
    0x01, 0x00, 0x00, 0x00, // version 1
];

/// 8-byte minimum-viable stub. Used as the azul-mini.wasm fallback
/// when the lift pipeline isn't available (web-transpiler feature off,
/// remill not installed, dlsym misses, lift errors). Sufficient for
/// `WebAssembly.instantiate(bytes)` to succeed; exports nothing, so
/// loader.js's AzStartup_* calls will throw — desktop debug only.
fn generate_mini_wasm_stub() -> Vec<u8> {
    WASM_HEADER.to_vec()
}

/// M10-D — union every `used_boundaries` set from the cb / layout
/// lifts plus any provided initial set (typically from mini.wasm),
/// then run [`transpiler_remill::RemillTranspiler::lift_boundary_to_wasm`]
/// once per unique boundary canonical address. The BFS inside
/// `lift_boundary_to_wasm` may itself surface new boundary references
/// (boundaries that depend on other boundaries) — we follow the
/// chain via a work-queue until the set stabilizes.
///
/// Returns one [`BoundaryWasm`] per unique boundary, sorted by
/// canonical address for deterministic ordering. Empty if the
/// transpiler isn't the real RemillTranspiler (e.g. StubTranspiler
/// in tests) — the stub can't lift anything anyway.
#[cfg(feature = "web-transpiler")]
pub fn lift_boundary_shards(
    initial_boundaries: &[usize],
) -> Vec<BoundaryWasm> {
    use std::collections::{HashSet, VecDeque};
    use transpiler::Transpiler;

    if !symbol_table::shards_enabled() {
        // Legacy bundled mode: framework symbols ship inline in the
        // cb / layout / mini wasms. No boundary shards needed.
        return Vec::new();
    }

    let transpiler = transpiler_remill::RemillTranspiler::new();
    if !transpiler.is_available() {
        eprintln!(
            "[azul-web] boundary-lift: transpiler unavailable, skipping shards"
        );
        return Vec::new();
    }

    let mut pending: VecDeque<usize> = initial_boundaries.iter().copied().collect();
    let mut done: HashSet<usize> = HashSet::new();
    let mut shards: Vec<BoundaryWasm> = Vec::new();

    while let Some(addr) = pending.pop_front() {
        if !done.insert(addr) {
            continue;
        }
        match transpiler.lift_boundary_to_wasm(addr) {
            Ok(shard) => {
                eprintln!(
                    "[azul-web]   boundary[{}]: lifted {} addr=0x{:016x} → {} bytes \
                     ({} transitive boundaries)",
                    shards.len() + 1,
                    shard.canonical_name,
                    shard.canonical_addr,
                    shard.wasm_bytes.len(),
                    shard.transitive_boundaries.len(),
                );
                for trans in &shard.transitive_boundaries {
                    if !done.contains(trans) {
                        pending.push_back(*trans);
                    }
                }
                shards.push(BoundaryWasm {
                    canonical_addr: shard.canonical_addr,
                    canonical_name: shard.canonical_name,
                    body_export: shard.body_export,
                    content_hash: shard.content_hash,
                    wasm_bytes: shard.wasm_bytes,
                    transitive_boundaries: shard.transitive_boundaries,
                });
            }
            Err(e) => {
                eprintln!(
                    "[azul-web]   boundary: lift failed for canonical_addr=0x{:x}: {} \
                     — skipping",
                    addr, e.reason,
                );
            }
        }
    }
    shards.sort_by_key(|s| s.canonical_addr);
    eprintln!(
        "[azul-web] boundary-lift: {} shards total ({} initial seeds)",
        shards.len(),
        initial_boundaries.len(),
    );
    shards
}

/// Stub for non-web-transpiler builds — returns an empty Vec.
#[cfg(not(feature = "web-transpiler"))]
pub fn lift_boundary_shards(_initial_boundaries: &[usize]) -> Vec<BoundaryWasm> {
    Vec::new()
}

/// M8.3: lift every unique layout callback fn-ptr referenced by the
/// configured routes. Dedupes by fn-addr so two routes sharing the
/// same layout function produce one `LayoutWasm` entry. Each lift
/// runs through the same M5-M7 pipeline as widget callbacks; failure
/// falls back to the [`emit_noop_callback_wasm`] stub with
/// `is_client_side = false` (matching the per-callback fallback path).
pub fn lift_layout_callbacks(layout_callbacks: &[LayoutCallback]) -> Vec<LayoutWasm> {
    let transpiler = transpiler::default_transpiler();
    let transpiler_available = transpiler.is_available();
    let mut seen: BTreeMap<usize, ()> = BTreeMap::new();
    let mut out: Vec<LayoutWasm> = Vec::new();
    for cb in layout_callbacks {
        let cb_addr = cb.cb as usize;
        if seen.insert(cb_addr, ()).is_some() {
            continue;
        }
        let sym = resolve_fn_ptr(cb_addr);
        let (wasm_bytes, is_client_side, used_boundaries) = lift_or_noop(
            transpiler.as_ref(),
            transpiler_available,
            &sym.name,
            sym.addr,
            sym.size,
            // LayoutCallback returns AzDom (>16B aggregate), so the
            // wrapper takes an extra `out_ptr: i32` arg and seeds the
            // hidden X8 destination register before invoking the body.
            // See `signature_for_callback_kind("LayoutCallback")` +
            // `Pcs::HiddenPtrReturn` in transpiler_remill.rs.
            "LayoutCallback",
        );
        eprintln!(
            "[azul-web]   layout-cb: {:<40} addr=0x{:016x} wasm={} client_side={}",
            sym.name, sym.addr, wasm_bytes.len(), is_client_side,
        );
        let content_hash = fnv1a64_hex(sym.name.as_bytes());
        out.push(LayoutWasm {
            name: sym.name,
            content_hash,
            fn_addr: sym.addr,
            wasm_bytes,
            is_client_side,
            used_boundaries,
        });
    }
    out
}

/// M8.2: lift the EVENTLOOP_SYMBOLS into a real azul-mini.wasm.
///
/// dlsym each AzStartup_* name in the running libazul, feed
/// `(name, addr, size)` tuples to `transpiler.lift_and_link_eventloop`.
/// On any failure log + fall back to the 8-byte stub so the rest of
/// run_web can proceed (per the M0-M7 "fail soft" discipline).
fn lift_eventloop_mini_wasm() -> Vec<u8> {
    let transpiler = transpiler::default_transpiler();
    if !transpiler.is_available() {
        eprintln!(
            "[azul-web] azul-mini: transpiler unavailable ({}), using 8-byte stub",
            transpiler.name(),
        );
        return generate_mini_wasm_stub();
    }
    let mut targets: Vec<(String, usize, usize)> = Vec::with_capacity(EVENTLOOP_SYMBOLS.len());
    for sym_name in EVENTLOOP_SYMBOLS {
        let Some(addr) = dlsym_self(sym_name) else {
            eprintln!(
                "[azul-web] azul-mini: dlsym({}) returned null — falling back to stub",
                sym_name,
            );
            return generate_mini_wasm_stub();
        };
        let sym = resolve_fn_ptr(addr);
        targets.push((sym_name.to_string(), sym.addr, sym.size));
    }
    match transpiler.lift_and_link_eventloop(&targets) {
        Ok(module) => {
            eprintln!(
                "[azul-web] azul-mini: lifted + linked {} bytes ({} exports)",
                module.bytes.len(),
                module.exports.len(),
            );
            module.bytes
        }
        Err(e) => {
            eprintln!(
                "[azul-web] azul-mini: lift_and_link_eventloop failed for {}: {} — \
                 falling back to 8-byte stub",
                e.fn_name, e.reason,
            );
            generate_mini_wasm_stub()
        }
    }
}

/// Run the web backend — called from `run()` when `AzBackend::Web(cfg)`.
///
/// This function blocks (like `run_headless`) serving HTTP requests until
/// the process is terminated.
pub fn run_web(
    app_data: RefAny,
    config: AppConfig,
    fc_cache: Arc<FcFontCache>,
    font_registry: Option<Arc<FcFontRegistry>>,
    root_window: WindowCreateOptions,
    web_config: config::WebConfig,
) -> Result<(), WindowError> {

    eprintln!("[azul-web] Starting web backend...");

    // M8.7a: validate the App can be hydrated on the wasm client.
    // RefAny needs a registered JSON serializer (AZ_REFLECT_JSON);
    // layout cb should be dladdr-resolvable (warning only). FATAL
    // failures abort here, before any HTTP serving.
    //
    // Pre-validate via a temporary RefAny-only check — we don't
    // have the StyledDom yet (that's Phase D's output) so we can't
    // build a full HeadlessApp until later. But the RefAny JSON
    // check is the gating one for the web demo to work.
    {
        let pre_check = azul_layout::json::refany_serialize_to_json(&app_data);
        match pre_check {
            azul_core::json::OptionJson::None => {
                let msg = "[azul-web] FATAL: web backend requires the root RefAny \
                           to have a JSON serializer registered via AZ_REFLECT_JSON. \
                           Got AzRefAny with no toJson fn-ptr — cannot hydrate \
                           state on the wasm client. See dll/azul.h's AZ_REFLECT_JSON \
                           macro for how to register.";
                eprintln!("{}", msg);
                return Err(WindowError::PlatformError(msg.to_string()));
            }
            azul_core::json::OptionJson::Some(ref json) => {
                eprintln!(
                    "[azul-web] RefAny JSON-roundtrip check (serialized): {}",
                    json,
                );
            }
        }
    }

    // Phase A: Classify API functions (stubbed for now)
    let classification = classify::classify_api_functions();
    eprintln!(
        "[azul-web] Classified {} API functions ({} framework, {} excluded)",
        classification.total(),
        classification.framework_count(),
        classification.excluded_count(),
    );

    // M8.8 Stage 1: build the SymbolTable from the loaded image. This
    // is the canonical source of truth for "what address → what name →
    // what bytes" — every lift consumer reads from it instead of
    // computing the same metadata locally with different conventions.
    //
    // Failure mode: log and continue without a table. The lift
    // pipeline degrades to the pre-M8.8 dladdr+LIFT_READ_WINDOW path
    // (preserved as fallback) so the server still starts.
    #[cfg(feature = "web-transpiler")]
    {
        match symbol_table::SymbolTable::build_from_loaded_image(&classification) {
            Ok(table) => {
                eprintln!(
                    "[azul-web] SymbolTable: {} entries across loaded images",
                    table.len()
                );
                let _ = symbol_table::install(table);
            }
            Err(e) => {
                eprintln!(
                    "[azul-web] SymbolTable build failed: {} — falling back to dladdr",
                    e
                );
            }
        }
    }

    // Phase B (M8.2): lift the EVENTLOOP_SYMBOLS into azul-mini.wasm.
    // Falls back to an 8-byte WASM_HEADER stub when the transpiler
    // or dlsym path can't satisfy the request — keeps Phase D/E
    // unblocked even if the eventloop lift fails.
    let _ = &classification; // M8.9 will use this to wire framework-call routing.
    let mini_wasm = lift_eventloop_mini_wasm();
    eprintln!("[azul-web] azul-mini.wasm: {} bytes", mini_wasm.len());

    // Phase D: Pre-render all routes. The walk also collects every
    // callback fn-pointer it sees, which feeds Phase C below.
    let window_state = root_window.window_state.clone();
    // LayoutCallback is not Copy (holds a host-invoker handle). Clone
    // up front so the inline-route arm (line 232) and the
    // WebServerState builder (line 325) both have an owned value.
    let default_layout_callback = root_window.window_state.layout_callback.clone();

    let mut rendered_routes: HashMap<String, server::RenderedRoute> = HashMap::new();
    let mut all_images = Vec::new();
    let mut all_fonts = Vec::new();
    let mut discovered_per_route: BTreeMap<String, Vec<DiscoveredCallback>> = BTreeMap::new();

    let routes = config.routes.as_ref();

    if routes.is_empty() {
        // No routes configured → use the root window's layout callback as "/"
        eprintln!("[azul-web] No routes configured, using root layout as /");
        let output = html_render::render_initial_page(
            &app_data,
            &default_layout_callback,
            &window_state,
            &fc_cache,
            font_registry.as_deref(),
            &mini_wasm,
            None,
            config.bundled_fonts.as_ref(),
        );
        eprintln!("[azul-web] Route / : {} bytes HTML, {} images, {} fonts, {} callbacks",
            output.html.len(), output.images.len(), output.fonts.len(), output.callbacks.len());

        let callback_index = build_callback_index(&output.callbacks);
        all_images.extend(output.images);
        all_fonts.extend(output.fonts);
        discovered_per_route.insert("/".to_string(), output.callbacks);
        rendered_routes.insert("/".to_string(), server::RenderedRoute {
            pattern: "/".to_string(),
            html: output.html,
            layout_callback: default_layout_callback.clone(),
            callback_index,
        });
    } else {
        // Pre-render each registered route
        for route in routes.iter() {
            let pattern = route.pattern.as_str();
            eprintln!("[azul-web] Pre-rendering route: {}", pattern);

            let route_match = RouteMatch {
                pattern: route.pattern.clone(),
                params: azul_core::window::StringPairVec::from_const_slice(&[]),
            };

            let output = html_render::render_initial_page(
                &app_data,
                &route.layout_callback,
                &window_state,
                &fc_cache,
                font_registry.as_deref(),
                &mini_wasm,
                Some(&route_match),
                config.bundled_fonts.as_ref(),
            );

            eprintln!("[azul-web] Route {} : {} bytes HTML, {} images, {} fonts, {} callbacks",
                pattern, output.html.len(), output.images.len(), output.fonts.len(),
                output.callbacks.len());

            // Rebase image/font IDs to avoid collisions across routes
            let img_offset = all_images.len();
            let font_offset = all_fonts.len();
            let mut html = output.html;

            // Rewrite image IDs in HTML (simple string replace)
            for img in &output.images {
                let old = format!("/az/img/{}", img.id);
                let new = format!("/az/img/{}", img.id + img_offset);
                html = html.replace(&old, &new);
            }
            for font in &output.fonts {
                let old = format!("/az/font/{}", font.id);
                let new = format!("/az/font/{}", font.id + font_offset);
                html = html.replace(&old, &new);
            }

            for mut img in output.images {
                img.id += img_offset;
                all_images.push(img);
            }
            for mut font in output.fonts {
                font.id += font_offset;
                all_fonts.push(font);
            }

            let callback_index = build_callback_index(&output.callbacks);
            discovered_per_route.insert(pattern.to_string(), output.callbacks);
            rendered_routes.insert(pattern.to_string(), server::RenderedRoute {
                pattern: pattern.to_string(),
                html,
                layout_callback: route.layout_callback.clone(),
                callback_index,
            });
        }
    }

    // Phase C: feed every discovered callback into the (still-stubbed) lift
    // pipeline. Discovery is functional (DOM walk + dladdr); `wasm_bytes`
    // stays empty until the remill / LLVM-IR / wasm-link pass is wired up.
    let mut cb_wasms = discover_and_transpile_callbacks(&discovered_per_route);
    eprintln!(
        "[azul-web] Discovered {} unique callbacks across {} route(s); transpile lift is stubbed",
        cb_wasms.len(), discovered_per_route.len(),
    );
    for cb in &cb_wasms {
        eprintln!(
            "[azul-web]   cb: {:<40} addr=0x{:016x} size={} hash={}",
            cb.name, cb.fn_addr, cb.fn_size, cb.content_hash
        );
    }

    // Phase C-layout (M8.3): lift the unique layout callbacks referenced
    // by the rendered routes. Each lift goes through the same M5-M7
    // pipeline as widget callbacks but is currently wrapped under the
    // canonical Callback PCS — the X8 hidden-return for AzStyledDom is
    // M8.5 work. Bytes serve via `/az/layout/<name>.<hash>.wasm`.
    let unique_layout_callbacks: Vec<LayoutCallback> = {
        let mut seen: BTreeMap<usize, ()> = BTreeMap::new();
        let mut v: Vec<LayoutCallback> = Vec::new();
        for r in rendered_routes.values() {
            let addr = r.layout_callback.cb as usize;
            if seen.insert(addr, ()).is_none() {
                v.push(r.layout_callback.clone());
            }
        }
        v
    };
    // Dev knob: skip layout-cb lift for fast iteration while debugging
    // cb-side regressions. Set AZ_SKIP_LAYOUT_LIFT=1 to bypass.
    let mut layout_wasms = if std::env::var_os("AZ_SKIP_LAYOUT_LIFT").is_some() {
        eprintln!("[azul-web] AZ_SKIP_LAYOUT_LIFT=1 — skipping layout-cb lift");
        Vec::new()
    } else {
        lift_layout_callbacks(&unique_layout_callbacks)
    };

    eprintln!(
        "[azul-web] Pre-rendered {} routes, {} total images, {} total fonts, {} layout WASMs",
        rendered_routes.len(), all_images.len(), all_fonts.len(), layout_wasms.len(),
    );

    // Phase F (M10-D): union every cb / layout used_boundaries set
    // and run the boundary-lift pass. Each unique boundary produces
    // one wasm shard served at `/az/fn/<name>.<hash>.wasm`. Empty
    // in legacy bundled mode (when AZ_ENABLE_SHARDS isn't set or
    // AZ_BUNDLED_LEGACY=1) — the cb / layout wasms still embed
    // their framework deps inline.
    let mut initial_boundaries: std::collections::HashSet<usize> =
        std::collections::HashSet::new();
    for cb in &cb_wasms {
        for &addr in &cb.used_boundaries {
            initial_boundaries.insert(addr);
        }
    }
    for lw in &layout_wasms {
        for &addr in &lw.used_boundaries {
            initial_boundaries.insert(addr);
        }
    }
    let initial_seeds: Vec<usize> = {
        let mut v: Vec<usize> = initial_boundaries.into_iter().collect();
        v.sort_unstable();
        v
    };
    let boundary_wasms = if shards_enabled() {
        eprintln!(
            "[azul-web] M10-D: lifting boundary shards (seed_count={})",
            initial_seeds.len(),
        );
        lift_boundary_shards(&initial_seeds)
    } else {
        Vec::new()
    };
    eprintln!(
        "[azul-web] Boundary shards: {} (sharded mode={})",
        boundary_wasms.len(),
        shards_enabled(),
    );

    // Phase D-cache (2026-06-10): turn the cb/layout wasm URL hashes into REAL
    // content hashes. The HTML is pre-rendered BEFORE the lifts, so the URLs are
    // emitted with `fnv1a64(name)` placeholders — which are CONSTANT across
    // builds. Served with `Cache-Control: immutable, max-age=1yr`, a browser
    // would keep the FIRST layout.wasm it ever saw and pair it with each new
    // build's mini.wasm (mini IS content-hashed) → call_indirect signature
    // mismatch in AzStartup_initLayoutCache after any rebuild. Rewrite every
    // route's HTML (preload hints + data-az-wasm attrs) with the lifted bytes'
    // hash; the /az/cb/ and /az/layout/ route handlers match by NAME and treat
    // the hash purely as a cache key, so no handler change is needed.
    {
        let mut url_rewrites: Vec<(String, String)> = Vec::new();
        for cb in &mut cb_wasms {
            if cb.wasm_bytes.is_empty() {
                continue;
            }
            let real = fnv1a64_hex(&cb.wasm_bytes);
            if real != cb.content_hash {
                url_rewrites.push((
                    format!("/az/cb/{}.{}.wasm", cb.name, cb.content_hash),
                    format!("/az/cb/{}.{}.wasm", cb.name, real),
                ));
                cb.content_hash = real;
            }
        }
        for lw in &mut layout_wasms {
            if lw.wasm_bytes.is_empty() {
                continue;
            }
            let real = fnv1a64_hex(&lw.wasm_bytes);
            if real != lw.content_hash {
                url_rewrites.push((
                    format!("/az/layout/{}.{}.wasm", lw.name, lw.content_hash),
                    format!("/az/layout/{}.{}.wasm", lw.name, real),
                ));
                lw.content_hash = real;
            }
        }
        if !url_rewrites.is_empty() {
            for route in rendered_routes.values_mut() {
                for (old, new) in &url_rewrites {
                    if route.html.contains(old.as_str()) {
                        route.html = route.html.replace(old.as_str(), new.as_str());
                    }
                }
            }
            eprintln!(
                "[azul-web] content-hashed {} wasm URL(s) in the pre-rendered HTML",
                url_rewrites.len(),
            );
        }
    }

    // Phase 0.2 preflight: report any function that lifted with __remill_error
    // (undecoded instruction) / __remill_missing_block. No-op unless AZ_PREFLIGHT=1.
    #[cfg(feature = "web-transpiler")]
    transpiler_remill::preflight_report();

    // Phase E: Start HTTP server
    let bind_addr = web_config.bind;
    eprintln!("[azul-web] Listening on http://{}", bind_addr);

    // Pre-compress the (immutable) mini module once, up front, at a high
    // quality — it's served brotli on the wire to every client that accepts
    // it (WEB_WASM_DIET_PLAN §2.2). q is size-aware: q=11 is worth the extra
    // startup seconds for a normal (post-wasm-opt) module, but if the module
    // is still huge (opt fell back) drop to q=9 so startup doesn't stall.
    let mini_wasm_br = {
        let q = if mini_wasm.len() <= 8 * 1024 * 1024 { 11 } else { 9 };
        let br = server::brotli_compress(&mini_wasm, q);
        if let Some(ref b) = br {
            eprintln!(
                "[azul-web] mini.wasm: {} bytes raw -> {} bytes brotli (q{}, served .br to clients that accept it)",
                mini_wasm.len(), b.len(), q,
            );
        }
        br
    };

    let state = server::WebServerState {
        app_data: Arc::new(Mutex::new(app_data)),
        config,
        web_config,
        fc_cache,
        font_registry,
        window_state,
        mini_wasm,
        mini_wasm_br,
        cb_wasms,
        layout_wasms,
        boundary_wasms,
        layout_callback: default_layout_callback,
        rendered_routes,
        images: all_images,
        fonts: all_fonts,
    };

    server::run_server(bind_addr, state)
        .map_err(|e| WindowError::PlatformError(format!("Web server error: {}", e)))
}

/// Build the `az_N → CoreCallback` map used by the `/az/exec/{node_id}`
/// dispatch handler.
///
/// When the same node ID carries multiple callbacks (e.g. one for `MouseUp`
/// and one for `MouseDown`), the first one wins. Phase 0 dispatches a single
/// callback per node — the event filter is already captured in
/// `data-az-ev` on the emitted HTML, so the client only targets the right
/// kind of event.
fn build_callback_index(discovered: &[DiscoveredCallback]) -> HashMap<u32, CoreCallback> {
    let mut idx: HashMap<u32, CoreCallback> = HashMap::new();
    for d in discovered {
        idx.entry(d.node_idx).or_insert_with(|| d.callback.clone());
    }
    idx
}
