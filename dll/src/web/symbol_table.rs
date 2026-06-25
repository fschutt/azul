//! SymbolTable — canonical source of truth for symbol identity in
//! the web-transpiler lift pipeline.
//!
//! # Why this exists
//!
//! Pre-M8.8, five subsystems each derived symbol metadata independently:
//!
//!   - `mod.rs::resolve_fn_ptr` (dladdr + macOS-arm64 PLT-stub chase)
//!   - `mod.rs::LIFT_READ_WINDOW` (flat 4 KiB read window per symbol)
//!   - `transpiler_remill.rs::parse_extern_sub_declares` + `.N` suffix
//!     handling (parses remill's `sub_<hex>` declares per call site)
//!   - `transpiler_remill.rs::branch_target_to_host_addr` (lift-space
//!     hex → host addr arithmetic, per-call-site)
//!   - `transpiler_remill.rs::is_recursable_dep` (regex on mangled
//!     names to decide whether to recurse)
//!
//! Each disagreement between these five became a downstream workaround:
//! `resolve_macos_arm64_stub`, `rewrite_tailcall_wrapper`, the `.N`
//! suffix carry-through, the hand-curated runtime-crate denylist, etc.
//!
//! The SymbolTable retires the gap by computing each per-symbol
//! quantity ONCE, at server startup, from the loaded image's own
//! Mach-O / ELF metadata. Every lift consumer reads from it rather
//! than rederiving.
//!
//! # Contract (per `scripts/M8.8_NEW_SESSION_PROMPT.md`)
//!
//! - `lookup(addr) -> Option<&SymbolEntry>`: returns the entry FOR
//!   THE GIVEN ADDRESS without chasing PLT stubs. The entry's `kind`
//!   field reveals whether the address points at a stub.
//! - `resolve(addr) -> Option<&SymbolEntry>`: chases the stub chain.
//!   Multi-hop (PLT → GOT → real) handled via repeated lookup.
//! - `canonical_name_for(addr) -> Option<&str>`: the canonical
//!   (post-chain) symbol name. Used by the post-lift IR rewrite to
//!   normalize remill's `sub_<lift_target_hex>` references into
//!   `sub_<canonical_addr_hex>` form so the linker dedupes naturally.
//!
//! # Why goblin
//!
//! Goblin's `Object::parse` accepts the loaded image's bytes and
//! returns a `Mach` / `Elf` / `PE` variant — one backend covers every
//! platform. macOS code paths use the LC_SYMTAB + LC_DYSYMTAB indirect
//! stub table; Linux paths use `.symtab` + `.got.plt`. The per-OS
//! enumerate_images() helper feeds goblin the right byte slice.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use std::sync::OnceLock;

use super::classify::{ApiClassification, FnClass as ApiFnClass};

/// Process-wide SymbolTable, set once at server startup. The lift
/// pipeline reads from it via `symbol_table()` instead of accepting
/// the table as an extra arg through the `Transpiler` trait (which is
/// stub-aware and shouldn't depend on goblin).
///
/// Initialized in `run_web` after classify_api_functions() finishes.
static SYMBOL_TABLE: OnceLock<SymbolTable> = OnceLock::new();

/// Install the process-wide SymbolTable. Returns `false` if a table
/// was already installed (the first call wins; subsequent calls are
/// no-ops so tests don't fight over the slot).
pub fn install(table: SymbolTable) -> bool {
    SYMBOL_TABLE.set(table).is_ok()
}

/// Fetch the process-wide SymbolTable, or `None` if it hasn't been
/// installed yet. The lift pipeline degrades gracefully (falls back
/// to the old dladdr / LIFT_READ_WINDOW path) when this returns
/// `None`, so partial wiring still builds.
pub fn get() -> Option<&'static SymbolTable> {
    SYMBOL_TABLE.get()
}

/// M10-D rollout knob. When the env var `AZ_ENABLE_SHARDS` is set,
/// `api.json::Framework` symbols classify as [`FnClass::BoundaryImport`]
/// instead of [`FnClass::Recursable`] — the lift BFS then stops at
/// every boundary and a separate pass produces per-fn wasm shards.
///
/// Once the sharded path is verified end-to-end the polarity will
/// flip: default = sharded; `AZ_BUNDLED_LEGACY=1` keeps the old
/// behavior.
pub fn shards_enabled() -> bool {
    std::env::var_os("AZ_ENABLE_SHARDS").is_some()
        && std::env::var_os("AZ_BUNDLED_LEGACY").is_none()
}

/// Kind of symbol entry. The lift pipeline switches on this to decide
/// whether to lift the bytes directly (`Function`), to chase through
/// to a sibling entry (`Stub`), or to skip lifting (`Data`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymKind {
    /// Defined code symbol — has bytes worth lifting.
    Function,
    /// Mach-O `__TEXT.__stubs` / ELF `.plt` trampoline. The target
    /// field is the canonical address of the real callee (computed
    /// at table-build time by walking the indirect-symtab + dlsym).
    Stub { target: usize },
    /// Data symbol (e.g. global). Not lifted; reserved for future
    /// const-pool hoisting (M8.9+).
    Data,
}

/// Per-symbol classification — drives the helper-IR body emission and
/// the transitive lifter's recurse-or-stop decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FnClass {
    /// `Az*` C API surface (per api.json) + Rust internals in our
    /// own crates (azul_core, azul_layout, …). Lift + walk its bl
    /// targets recursively.
    Recursable,
    /// **M10-D**: `Az*` framework symbol that ships as its own
    /// per-fn wasm shard. Lift treats it like `Leaf` inside cb /
    /// layout / mini lifts — emits a `declare` only, no body,
    /// no transitive recursion into its bl targets. A separate
    /// boundary-lift pass produces one `.wasm` per BoundaryImport
    /// that's referenced anywhere; the cb's wasm-ld run sees the
    /// declare as undefined and (with `--allow-undefined`) turns it
    /// into an env-import. At instantiate time, JS wires
    /// `env.sub_<canonical_hex>` to the boundary shard's exported
    /// body.
    BoundaryImport,
    /// `__rust_alloc` / `__rust_alloc_zeroed`. Helper IR emits a
    /// body that bumps `@__az_bump_ptr` by State.X0 and returns the
    /// old value. See `emit_helper_ir` BranchExternKind::RustAlloc.
    BumpAlloc,
    /// `__rust_realloc(old_ptr, old_size, align, new_size)`. Helper
    /// IR emits a body that bumps `@__az_bump_ptr` by new_size,
    /// memcpys `min(old_size, new_size)` bytes from old_ptr into the
    /// new region, and returns the new pointer. Required so Vec
    /// resizes (Vec::push past capacity etc.) work for layout-cb
    /// dispatch. Bump-only allocator means the old region is leaked
    /// but never reused — fine for short-lived per-request lifts.
    BumpRealloc,
    /// `__rust_dealloc(ptr, size, align)`. Helper IR emits a noop
    /// body (bump-only allocator never frees). Distinguished from
    /// Leaf so the classifier can flag it explicitly rather than
    /// silently fall through. Behaviorally identical to Leaf.
    BumpDealloc,
    /// `__az_call_indirect(tidx, refany_lo, refany_hi, info_ptr)`.
    /// Helper IR lowers to wasm `call_indirect` via the imported
    /// `__indirect_function_table`.
    CallIndirect,
    /// `__az_call_indirect_layout4(tidx, refany_lo, refany_hi,
    /// info_ptr, out_ptr)`. M9-3 4-arg call_indirect shape for the
    /// layout-cb wrapper which uses [`Pcs::HiddenPtrReturn`] (an
    /// extra `out_ptr` arg seeds State.X8 for the AAPCS64 indirect
    /// return). Wraps a fn whose signature is
    /// `(i64, i64, i32, i32) -> i32`. Kept separate from
    /// [`CallIndirect`] so the existing 3-arg widget-cb dispatch
    /// path is untouched.
    CallIndirectLayout4,
    /// `__az_resolve_callback(fn_addr)`. Helper IR emits a JS-import
    /// bridge that returns a table index.
    ResolveCallback,
    /// `std::sys::random::hashmap_random_keys() -> (u64, u64)` — the entropy
    /// source for `std::HashMap`'s `RandomState` SipHash keys. It's a syscall
    /// wrapper (getentropy) that can't be lifted; the default `Leaf` stub
    /// returns X0=0 and leaves the (u64,u64) keys unusable, so RandomState's
    /// hasher is degenerate and EVERY `std::HashMap` in the lifted env comes
    /// back empty (the M12.7 `dom_to_layout` symptom, but systemic). Helper IR
    /// gives it a body returning a FIXED non-zero seed in X0:X1, so all lifted
    /// HashMaps hash + probe consistently. The seed needn't be random — only
    /// consistent within the process (no HashDoS threat here).
    HashmapRandomKeys,
    /// Known leaf with no real body to lift — typed extern goes to
    /// the WASM as an env import. (System libraries, libc, libdyld,
    /// libpthread, mangled Rust runtime internals like
    /// `core::panicking`.)
    Leaf,
    /// libc `memcpy` / `memmove` (X0=dest, X1=src, X2=n; returns
    /// dest in X0). The real symbol is out-of-image (resolved via
    /// PLT-chase to a libsystem address no rebase covers), so it
    /// can't be lifted. The default `Leaf` stub RETURNS without
    /// copying — which silently drops every large struct move: Rust
    /// lowers `Box::new(big_struct)` and `<[T]>::to_vec` to an
    /// out-of-line `bl _memcpy`, so the destination keeps its
    /// (zero-init) bump-alloc bytes. (Verified: `Box::new(styled)`
    /// of a 352-byte StyledDom left `node_data.len == 0`.) Helper
    /// IR emits a real `@llvm.memmove` body instead — see
    /// `emit_helper_ir` BranchExternKind::LibcMemcpy.
    LibcMemcpy,
    /// libc `memset` (X0=dest, X1=byte, X2=n; returns dest in X0). Like
    /// `LibcMemcpy` the real symbol is out-of-image, so the default `Leaf`
    /// stub RETURNS without writing — which silently drops every bulk fill.
    /// CRITICAL: hashbrown initializes a freshly-allocated table's control
    /// bytes to EMPTY (0xFF) via `ptr::write_bytes(EMPTY, n)` = `memset`. A
    /// no-op stub leaves them at the bump allocator's zero bytes (0x00, which
    /// reads as a FULL slot with the high bit clear), so `HashMap::insert`'s
    /// SwissTable probe never finds an empty slot → INFINITE LOOP (the M12.7
    /// sizing hang in `calculate_intrinsic_sizes`). Helper IR emits a real
    /// `@llvm.memset` body — see `emit_helper_ir`.
    LibcMemset,
    /// libc `snprintf` / `vsnprintf` (and `_chk` spellings). AArch64 ABI for
    /// `__snprintf_chk(buf, maxlen, flag, slen, fmt, ...)`: X0=buf, X1=maxlen,
    /// X4=fmt, X5=first vararg; returns the written length in X0. Out-of-image
    /// like `LibcMemcpy`/`LibcMemset`, so the default `Leaf` stub returns WITHOUT
    /// writing the buffer → e.g. hello-world's `snprintf(buf,20,"%d",counter)`
    /// leaves the counter label EMPTY (a height-0 wrapper). Helper IR emits a
    /// minimal real body that handles ONLY the `"%d"` format (verified by reading
    /// 3 bytes at X4) and itoa's the non-negative i32 vararg; any other format
    /// falls through to the no-op so unrelated snprintf uses are unaffected.
    /// See `emit_helper_ir` BranchExternKind::LibcSnprintf.
    LibcSnprintf,
    /// Server-entry-point (e.g. `AzApp_run`). Should never appear in
    /// a lifted body; if it does, the helper IR should trap loudly.
    NeverLift,
}

impl FnClass {
    /// Whether the transitive lifter should walk this symbol's
    /// own bl targets.
    pub fn is_recursable(self) -> bool {
        matches!(self, FnClass::Recursable)
    }

    /// WEB-LIFT FACET 2: whether this is a bump-allocator hook
    /// (`__rust_alloc`/`dealloc`/`realloc`), lifted to an intercepted
    /// bump/noop body. The web force-enqueue lifts these so indirect
    /// calls to them (Drop glue) get a dispatcher `switch` case.
    pub fn is_bump_alloc(self) -> bool {
        matches!(self, FnClass::BumpAlloc)
    }

    /// M10-D: whether this symbol ships as its own per-fn wasm shard.
    /// Lift sites stop recursion at boundary imports and let wasm-ld
    /// emit an env-import for the canonical `sub_<hex>` body; the
    /// boundary-lift pass produces the body wasm separately.
    pub fn is_boundary_import(self) -> bool {
        matches!(self, FnClass::BoundaryImport)
    }
}

#[derive(Debug, Clone)]
pub struct SymbolEntry {
    /// Canonical name. For symbols from a known image's symbol table:
    /// the de-underscored name (`_Az…` → `Az…` on macOS so it matches
    /// what `dladdr` reports). For symbols only reachable via
    /// dlsym/dladdr (e.g. local-but-discoverable callbacks): whatever
    /// dladdr returned.
    pub canonical_name: String,
    /// Real entry-point address in this process, post-PLT-chase.
    pub canonical_addr: usize,
    /// Per-image-rebased address chosen so wasm `i32.const`
    /// truncations of lifted `adrp+ldr` targets land inside a small,
    /// predictable region of wasm linear memory.
    ///
    /// Computed as `image_synth_base + (canonical_addr - image_native_min)`.
    /// PC-relative distances within an image are preserved (so intra-
    /// image `bl`/`adrp` lifts at `--address=synthetic_addr` produce
    /// correct cross-call / cross-page targets without IR rewriting).
    ///
    /// **Why it exists (M9-review fix, 2026-05-18)**: passing the
    /// post-ASLR runtime `canonical_addr` to `remill-lift --address=…`
    /// bakes that high value as the PC for every lifted instruction.
    /// ARM64 `adrp x<n>, …` lifts to `(PC & ~0xFFF) | (imm << 12)`,
    /// which truncates to ~200 MiB on typical macOS dyld slides —
    /// past wasm's 16 MiB initial memory. Switching the lifter to
    /// `--address=synthetic_addr` lands every `adrp` in a small,
    /// pre-arranged band of wasm memory where the data mirror puts
    /// the right bytes.
    ///
    /// Defaults to `canonical_addr` if [`SymbolTable::assign_synthetic_addresses`]
    /// hasn't run; treat the two as equal in that case.
    pub synthetic_addr: usize,
    /// Exact size in bytes from `(next_symbol_addr - this_symbol_addr)`
    /// within the same section. Conservative overshoot when this is
    /// the last symbol in its section (size goes up to section end).
    /// Always ≥ 0.
    pub size: usize,
    /// Live code bytes for this symbol. `Some` for `Function` /
    /// `Stub` kinds (a slice into the loaded image's __TEXT segment,
    /// which stays mapped for the process lifetime). `None` for
    /// `Data` symbols.
    ///
    /// SAFETY note: the `'static` lifetime here reflects "the loaded
    /// image is never unmapped" — true for libazul under our use
    /// case (loaded once at server start, no dlclose). If you ever
    /// add dlclose-on-shutdown, change this lifetime.
    pub bytes: Option<&'static [u8]>,
    pub kind: SymKind,
    pub classification: FnClass,
}

#[derive(Debug)]
pub struct SymbolTable {
    by_addr: BTreeMap<usize, SymbolEntry>,
    by_name: HashMap<String, usize>,
    /// `stub_addr → canonical_addr`. Identity (`addr → addr`) for
    /// non-stub addresses, so `resolve(addr)` can chase
    /// unconditionally.
    chain: HashMap<usize, usize>,
    /// Per-image rebasing record. One entry per loaded image that
    /// contributed symbols. Drives [`assign_synthetic_addresses`]
    /// + the `inject_user_binary_data_segments` pass in
    /// `transpiler_remill.rs`.
    image_rebases: Vec<ImageRebase>,
    /// `stub_addr_synth → canonical_addr_synth`. Parallel to `chain`
    /// but keyed by synthetic addresses. Populated by
    /// [`assign_synthetic_addresses`].
    synth_chain: HashMap<usize, usize>,
    /// Image bytes kept alive so the `&'static [u8]` slices inside
    /// `bytes` fields don't dangle. Each entry is the file-bytes
    /// `Vec<u8>` of one loaded image. We don't actually point into
    /// these — we point into live __TEXT memory — but keeping the
    /// file bytes lets us re-derive sizes / stub targets later if
    /// the table grows extension points.
    #[allow(dead_code)]
    image_bytes: Vec<Vec<u8>>,
    /// macOS thread-local-variable geometry per image (live addresses).
    /// Drives the wasm TLS emulation: the data mirror rewrites every
    /// descriptor's thunk pointer to `AZ_TLV_MAGIC_PC` and the indirect
    /// dispatcher resolves that PC to `tls_base + descriptor.offset`
    /// (single-threaded wasm ⇒ TLS is just statics). See
    /// `transpiler_remill.rs` `AZ_TLV_MAGIC_PC`.
    tlv_regions: Vec<TlvRegion>,
}

/// One image's `__DATA.__thread_vars` + TLS-image geometry (live, slid
/// addresses). `tls_base` is the start of `__thread_data` — dyld assigns
/// each descriptor's `offset` field relative to it (`__thread_bss`
/// follows contiguously; its zero-init maps to wasm's zero default, so
/// only `__thread_data` bytes need mirroring).
#[derive(Debug, Clone, Copy)]
pub struct TlvRegion {
    /// Live start of `__thread_vars` (array of 24-byte descriptors
    /// `{thunk, key, offset}`).
    pub vars_start: usize,
    /// Byte size of `__thread_vars`.
    pub vars_size: usize,
    /// Live start of `__thread_data` (the TLS initial image; descriptor
    /// offsets are relative to this).
    pub tls_base: usize,
    /// Byte size of `__thread_data` (initialized TLS bytes to mirror).
    pub tls_data_size: usize,
}

/// Per-image rebasing record. Tracks the native↔synthetic mapping
/// so the lifter can rebase `--address` + the data-section mirror
/// can compute synthetic destination offsets.
#[derive(Debug, Clone)]
pub struct ImageRebase {
    /// Image's runtime base (lowest live address across all
    /// non-PAGEZERO segments — typically the __TEXT segment's
    /// `vmaddr + slide`).
    pub native_base: usize,
    /// One past the image's highest live address. Used to bound
    /// the rebased synthetic range.
    pub native_end: usize,
    /// Per-image synthetic base; `synthetic_addr = synth_base +
    /// (canonical_addr - native_base)`. Chosen by
    /// [`SymbolTable::assign_synthetic_addresses`] in monotonic
    /// order so images don't collide.
    pub synth_base: usize,
    /// Display path of the image (for diagnostics).
    pub path: String,
}

/// Error type for `SymbolTable::build_from_loaded_image`. The `stage`
/// field is a short identifier (`"dyld"`, `"parse"`, `"symtab"`, …)
/// useful for log filtering.
#[derive(Debug)]
pub struct BuildError {
    pub stage: &'static str,
    pub message: String,
}

impl std::fmt::Display for BuildError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SymbolTable [{}]: {}", self.stage, self.message)
    }
}
impl std::error::Error for BuildError {}

impl SymbolTable {
    /// Lookup the entry FOR THE GIVEN ADDRESS — without stub chasing.
    /// The returned entry's `kind` reveals whether the address is a
    /// PLT stub (in which case `kind = Stub { target }`).
    pub fn lookup(&self, addr: usize) -> Option<&SymbolEntry> {
        self.by_addr.get(&addr)
    }

    /// Follow the stub chain. For non-stub addresses, returns the
    /// entry at `addr`. For stubs, follows the chain (up to 4 hops to
    /// short-circuit any pathological cycle) and returns the entry at
    /// the final target.
    pub fn resolve(&self, addr: usize) -> Option<&SymbolEntry> {
        let mut cur = addr;
        for _ in 0..4 {
            let next = *self.chain.get(&cur).unwrap_or(&cur);
            if next == cur {
                break;
            }
            cur = next;
        }
        self.by_addr.get(&cur)
    }

    /// Canonical (post-chain) name for an address. Returns `None`
    /// when neither `addr` nor any chain target is registered.
    pub fn canonical_name_for(&self, addr: usize) -> Option<&str> {
        self.resolve(addr).map(|e| e.canonical_name.as_str())
    }

    /// Canonical address for an address — `addr` itself for non-stub
    /// addrs, the chased-through real address for stubs. `None` when
    /// nothing about the address is known.
    pub fn canonical_addr_for(&self, addr: usize) -> Option<usize> {
        self.resolve(addr).map(|e| e.canonical_addr)
    }

    pub fn by_name(&self, name: &str) -> Option<&SymbolEntry> {
        let addr = *self.by_name.get(name)?;
        self.by_addr.get(&addr)
    }

    /// Total number of entries (sum of Function + Stub + Data kinds).
    pub fn len(&self) -> usize {
        self.by_addr.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_addr.is_empty()
    }

    /// Iterate entries by ascending address. Useful for diagnostics.
    pub fn iter(&self) -> impl Iterator<Item = (&usize, &SymbolEntry)> {
        self.by_addr.iter()
    }

    /// M9-3b: enumerate every loaded image's `__TEXT.__cstring`,
    /// `__TEXT.__const`, `__DATA.__data`, and `__DATA.__const`
    /// segments and return the subset whose runtime address truncated
    /// to 32 bits fits inside a wasm linear memory region (currently
    /// `[0 .. 192 KiB]`, the band below the relocated wasm stacks set
    /// up by [`crate::web::transpiler_remill::relocate_stack_if_non_mini`]).
    ///
    /// The returned `(wasm_offset, bytes)` pairs are intended for
    /// injection as wasm Data segments in the mini wasm. Bytes from
    /// the user binary's `__cstring` / `__const` / `__data` are
    /// addressed by the lifted code via `adrp + ldr/add` to native
    /// addresses; on wasm32 those addresses get truncated to the low
    /// 32 bits when used as memory pointers. Pre-populating wasm
    /// memory at the truncated offsets lets a string-literal read
    /// like `snprintf(buf, 20, "%d", n)` find the `"%d"` format
    /// where the cb expects it.
    ///
    /// Each entry is returned at most once — duplicate ranges across
    /// images (very common since libdyld is shared) get deduped on
    /// `(wasm_offset, bytes_hash)`.
    #[allow(dead_code)]
    pub fn enumerate_low32_data_for_wasm(wasm_offset_limit: u32) -> Vec<(u32, Vec<u8>)> {
        let mut out: Vec<(u32, Vec<u8>)> = Vec::new();
        let images = match enumerate_loaded_images() {
            Ok(v) => v,
            Err(e) => {
                eprintln!("[symbol_table] enumerate_low32 failed: {}", e);
                return out;
            }
        };
        // Largest single segment we'll mirror. Keeps a malformed image
        // from blowing up the data-segment count.
        const PER_SECTION_LIMIT: usize = 64 * 1024;
        for img in &images {
            let parsed = match goblin::Object::parse(&img.bytes) {
                Ok(p) => p,
                Err(_) => continue,
            };
            let sections = match parsed {
                goblin::Object::Mach(goblin::mach::Mach::Binary(macho)) => {
                    collect_macho_low32_sections(&macho, &img.bytes, img.slide, wasm_offset_limit)
                }
                goblin::Object::Mach(goblin::mach::Mach::Fat(fat)) => {
                    match pick_fat_slice(&fat, &img.bytes) {
                        Ok(Some(macho)) => collect_macho_low32_sections(
                            &macho, &img.bytes, img.slide, wasm_offset_limit,
                        ),
                        _ => Vec::new(),
                    }
                }
                goblin::Object::Elf(elf) => {
                    collect_elf_low32_sections(&elf, &img.bytes, img.slide, wasm_offset_limit)
                }
                _ => Vec::new(),
            };
            for (off, sz) in sections {
                if sz == 0 || sz > PER_SECTION_LIMIT as u64 {
                    continue;
                }
                let live_addr = (off as usize).wrapping_add(img.slide);
                let truncated = (live_addr as u64) & 0xFFFF_FFFF;
                if truncated == 0 || truncated.saturating_add(sz) > wasm_offset_limit as u64 {
                    continue;
                }
                // Read the live bytes at runtime. SAFETY: the loader
                // mapped these segments and they stay mapped for the
                // process lifetime.
                let bytes = unsafe {
                    core::slice::from_raw_parts(live_addr as *const u8, sz as usize).to_vec()
                };
                out.push((truncated as u32, bytes));
            }
        }
        // Dedup by exact (offset, content) so identical sections from
        // multiple images don't bloat mini.wasm.
        out.sort_by_key(|(off, _)| *off);
        out.dedup_by(|a, b| a.0 == b.0 && a.1 == b.1);
        out
    }

    /// Build the SymbolTable by walking every loaded image of the
    /// current process.
    ///
    /// `api` is consumed to classify Az* symbols per their api.json
    /// declarations (Framework / ServerEntryPoint / ReplaceWithDomPatcher).
    /// Non-Az symbols are classified via name pattern matching
    /// (see `classify_by_name`).
    pub fn build_from_loaded_image(api: &ApiClassification) -> Result<Self, BuildError> {
        let images = enumerate_loaded_images()?;
        if images.is_empty() {
            return Err(BuildError {
                stage: "dyld",
                message: "no loaded images discovered".into(),
            });
        }

        let api_class_by_name: HashMap<String, ApiFnClass> = api
            .functions
            .iter()
            .map(|(n, c)| (n.clone(), *c))
            .collect();

        let mut by_addr: BTreeMap<usize, SymbolEntry> = BTreeMap::new();
        let mut by_name: HashMap<String, usize> = HashMap::new();
        let mut chain: HashMap<usize, usize> = HashMap::new();
        let mut image_bytes: Vec<Vec<u8>> = Vec::with_capacity(images.len());
        let mut tlv_regions: Vec<TlvRegion> = Vec::new();

        for LoadedImage { path, slide, bytes } in images {
            // Defensive: bytes empty (file unreadable) → skip silently.
            if bytes.is_empty() {
                image_bytes.push(bytes);
                continue;
            }
            let parsed = match goblin::Object::parse(&bytes) {
                Ok(obj) => obj,
                Err(e) => {
                    eprintln!(
                        "[symbol_table] skipping {}: parse error: {}",
                        path.display(),
                        e
                    );
                    image_bytes.push(bytes);
                    continue;
                }
            };
            match parsed {
                goblin::Object::Mach(goblin::mach::Mach::Binary(macho)) => {
                    ingest_macho(
                        &macho,
                        &bytes,
                        slide,
                        &api_class_by_name,
                        &mut by_addr,
                        &mut by_name,
                        &mut chain,
                    )?;
                    tlv_regions.extend(collect_macho_tlv_regions(&macho, &bytes, slide, &path));
                }
                goblin::Object::Mach(goblin::mach::Mach::Fat(fat)) => {
                    // Fat archive: pick the slice matching the host
                    // arch. This is uncommon for live-loaded images
                    // (dyld resolves the right slice before mapping)
                    // but show up if someone parses a universal
                    // binary file on disk. Pick the first viable slice.
                    if let Ok(Some(macho)) = pick_fat_slice(&fat, &bytes) {
                        ingest_macho(
                            &macho,
                            &bytes,
                            slide,
                            &api_class_by_name,
                            &mut by_addr,
                            &mut by_name,
                            &mut chain,
                        )?;
                        tlv_regions.extend(collect_macho_tlv_regions(&macho, &bytes, slide, &path));
                    }
                }
                goblin::Object::Elf(elf) => {
                    ingest_elf(
                        &elf,
                        &bytes,
                        slide,
                        &api_class_by_name,
                        &mut by_addr,
                        &mut by_name,
                        &mut chain,
                    )?;
                }
                #[cfg(target_os = "windows")]
                goblin::Object::PE(pe) => {
                    ingest_pe(
                        &pe,
                        &bytes,
                        slide,
                        &path,
                        &api_class_by_name,
                        &mut by_addr,
                        &mut by_name,
                        &mut chain,
                    )?;
                    // No TLV regions on Windows yet: Rust-std TLS uses
                    // TlsGetValue imports / _tls_index here, not the
                    // Mach-O __thread_vars descriptor walk. Revisit when
                    // a relift log shows TLS traffic (compendium A8).
                }
                _ => {
                    // Unknown / foreign-OS object — fall through.
                }
            }
            image_bytes.push(bytes);
        }

        // Ensure every recorded address is a self-chain at minimum,
        // so resolve()'s lookup-or-self behaviour is uniform.
        for addr in by_addr.keys().copied().collect::<Vec<_>>() {
            chain.entry(addr).or_insert(addr);
        }

        let mut table = SymbolTable {
            by_addr,
            by_name,
            chain,
            image_rebases: Vec::new(),
            synth_chain: HashMap::new(),
            image_bytes,
            tlv_regions,
        };

        // M9-review: assign per-image synthetic bases so lifted code
        // uses wasm-friendly addresses for `adrp+ldr` page targets.
        // See `M9_REVIEW_AND_OPTION_A.md` for the rationale.
        table.assign_synthetic_addresses();

        Ok(table)
    }

    /// M9-review (2026-05-18): walk every loaded image, group
    /// `SymbolEntry`s by their `canonical_addr`'s containing image,
    /// assign each image a unique `synth_base` in monotonically
    /// increasing order, then fill in `entry.synthetic_addr` =
    /// `synth_base + (canonical_addr - image_native_min)`.
    ///
    /// **Layout** (current scheme; subject to tuning):
    ///
    /// ```text
    /// synth offset │ what lives here
    /// ─────────────┼─────────────────────────────────────────
    /// 0x0    .. 0x10000  reserved (cb wrapper stacks land here via
    ///                    `relocate_stack_if_non_mini` post-link patch)
    /// 0x10000+          image 0 (typically user binary, ~64 KiB)
    /// 0x100000+         image 1 (libazul.dylib, ~80 MiB)
    /// (further bases assigned as 1 MiB above the previous image's end)
    /// ```
    ///
    /// PC-relative distances within an image are preserved
    /// (`synth_B - synth_A == canonical_B - canonical_A` for any
    /// two symbols in the same image), so intra-image `bl` and
    /// `adrp` lifts at `--address=synthetic_addr` produce correct
    /// cross-call / cross-page targets without IR rewriting.
    fn assign_synthetic_addresses(&mut self) {
        // Pass 1: derive per-image native min/max by re-parsing the
        // image bytes via goblin.
        let mut rebases: Vec<ImageRebase> = Vec::new();
        let images = enumerate_loaded_images().unwrap_or_default();
        for img in &images {
            if img.bytes.is_empty() {
                continue;
            }
            let parsed = match goblin::Object::parse(&img.bytes) {
                Ok(p) => p,
                Err(_) => continue,
            };
            let (native_min_off, native_max_off) = match parsed {
                goblin::Object::Mach(goblin::mach::Mach::Binary(macho)) => {
                    macho_image_text_data_range(&macho, &img.bytes)
                }
                goblin::Object::Mach(goblin::mach::Mach::Fat(fat)) => {
                    match pick_fat_slice(&fat, &img.bytes) {
                        Ok(Some(macho)) => macho_image_text_data_range(&macho, &img.bytes),
                        _ => (0, 0),
                    }
                }
                goblin::Object::Elf(elf) => {
                    elf_image_text_data_range(&elf, &img.bytes)
                }
                goblin::Object::PE(pe) => pe_image_text_data_range(&pe),
                _ => (0, 0),
            };
            if native_min_off == 0 && native_max_off == 0 {
                continue;
            }
            let native_base = native_min_off.wrapping_add(img.slide);
            let native_end = native_max_off.wrapping_add(img.slide);
            rebases.push(ImageRebase {
                native_base,
                native_end,
                synth_base: 0, // filled below
                path: img.path.display().to_string(),
            });
        }

        // Pass 2: sort by native_base + assign synthetic bases in
        // ascending order with 1 MiB rounding so different images
        // sit in separate megabyte-aligned bands.
        rebases.sort_by_key(|r| r.native_base);
        const FIRST_SYNTH_BASE: usize = 0x10000;   // 64 KiB
        const SYNTH_ALIGN: usize = 0x10_0000;       // 1 MiB
        let mut next_synth = FIRST_SYNTH_BASE;
        for r in &mut rebases {
            r.synth_base = next_synth;
            let span = r.native_end.saturating_sub(r.native_base);
            // Round up to next 1 MiB boundary so the NEXT image's
            // base aligns cleanly.
            let aligned_span = (span + (SYNTH_ALIGN - 1)) & !(SYNTH_ALIGN - 1);
            next_synth = next_synth.saturating_add(aligned_span.max(SYNTH_ALIGN));
        }

        // Pass 3: walk all entries and assign each its synthetic_addr
        // based on which image's native range contains the entry's
        // BY_ADDR KEY (= the entry's actual location in memory),
        // NOT its `canonical_addr`. PLT stubs live in the calling
        // image with `canonical_addr` pointing at the chase target
        // in a different image — using `canonical_addr` would assign
        // them the chase-target's synth, which mismatches what the
        // calling image's lifted `bl` produces.
        //
        // Symbols not falling in any tracked image (e.g. dynamically
        // resolved addresses) keep synthetic_addr = canonical_addr.
        for (native_loc, entry) in self.by_addr.iter_mut() {
            for r in &rebases {
                if *native_loc >= r.native_base && *native_loc < r.native_end {
                    entry.synthetic_addr =
                        r.synth_base.wrapping_add(*native_loc - r.native_base);
                    break;
                }
            }
        }

        // Pass 3.5: M10-A1 — address-based classifier override for
        // out-of-image symbols.
        //
        // The name-based `classify_for_name` falls through to
        // `Recursable` for any symbol that doesn't match a known
        // prefix/suffix pattern. That's correct for our own crates
        // (azul_*, webrender_*, …) but WRONG for libsystem stubs
        // that arrived via PLT-chase + dlsym: e.g. `_platform_memmove`
        // has its leading `_` stripped at ingest, falls through the
        // pattern table, and ends up Recursable. Lifting one of these
        // produces garbage because its `synthetic_addr` stays at the
        // native (6+ GiB) value — no image rebase covers it, so the
        // per-page data mirror has nothing at the address the lifted
        // body reads from.
        //
        // The fix is structural: if `canonical_addr` falls outside
        // every tracked `image_rebases` range, it CANNOT be lifted
        // correctly. Force `Leaf` so the helper IR emits a typed
        // extern stub instead. Keep the existing classifications for
        // BumpAlloc/Realloc/Dealloc/CallIndirect[Layout4]/ResolveCallback
        // (they're identified by name and don't need a body lift) and
        // for NeverLift.
        let in_tracked = |addr: usize| -> bool {
            rebases
                .iter()
                .any(|r| addr >= r.native_base && addr < r.native_end)
        };
        let mut overridden = 0usize;
        for entry in self.by_addr.values_mut() {
            if matches!(
                entry.classification,
                FnClass::Leaf
                    | FnClass::BumpAlloc
                    | FnClass::BumpRealloc
                    | FnClass::BumpDealloc
                    | FnClass::CallIndirect
                    | FnClass::CallIndirectLayout4
                    | FnClass::ResolveCallback
                    | FnClass::HashmapRandomKeys
                    | FnClass::NeverLift
                    // LibcMemcpy is *always* out-of-image (it IS a
                    // libsystem symbol) — but it has a synthetic
                    // `@llvm.memmove` body, so don't downgrade it to a
                    // no-op Leaf here.
                    | FnClass::LibcMemcpy
                    // LibcMemset: same — out-of-image but has a synthetic
                    // `@llvm.memset` body; don't downgrade to a no-op Leaf.
                    | FnClass::LibcMemset
                    // LibcSnprintf: same — out-of-image but has a synthetic
                    // "%d" formatter body; don't downgrade to a no-op Leaf.
                    | FnClass::LibcSnprintf
            ) {
                continue;
            }
            if !in_tracked(entry.canonical_addr) {
                entry.classification = FnClass::Leaf;
                overridden += 1;
            }
        }
        eprintln!(
            "[symbol_table] M10-A1: forced Leaf on {} symbols whose \
             canonical_addr falls outside tracked image ranges",
            overridden,
        );

        // Pass 4: build the synth-keyed chain mirroring `chain`.
        let synth_of = |addr: usize| -> usize {
            self.by_addr
                .get(&addr)
                .map(|e| e.synthetic_addr)
                .unwrap_or(addr)
        };
        let synth_chain: HashMap<usize, usize> = self
            .chain
            .iter()
            .map(|(stub_native, canon_native)| {
                (synth_of(*stub_native), synth_of(*canon_native))
            })
            .collect();

        eprintln!(
            "[symbol_table] M9-review: assigned synthetic addresses for {} images, \
             {} symbols rebased (total span {} MiB)",
            rebases.len(),
            self.by_addr.len(),
            next_synth / (1024 * 1024),
        );
        for r in &rebases {
            let span_mib = (r.native_end - r.native_base) / (1024 * 1024);
            eprintln!(
                "[symbol_table]   {} → synth_base=0x{:x}, native=[0x{:x}..0x{:x}] (~{} MiB)",
                r.path, r.synth_base, r.native_base, r.native_end, span_mib,
            );
        }

        self.image_rebases = rebases;
        self.synth_chain = synth_chain;
    }

    /// M9-review: read accessor for the per-image rebase records.
    /// Used by `transpiler_remill::inject_user_binary_data_segments`
    /// to compute synthetic offsets for mirrored data segments.
    pub fn image_rebases(&self) -> &[ImageRebase] {
        &self.image_rebases
    }

    /// M9-review: synthetic-space stub-chain follow. Mirrors
    /// [`resolve`] but operates on synthetic addresses. Used when
    /// rewriting `sub_<synth_hex>` symbols in lifted IR.
    pub fn resolve_synth(&self, synth_addr: usize) -> Option<usize> {
        let mut cur = synth_addr;
        for _ in 0..4 {
            let next = *self.synth_chain.get(&cur).unwrap_or(&cur);
            if next == cur {
                break;
            }
            cur = next;
        }
        Some(cur)
    }

    /// M9-review: look up an entry by its `synthetic_addr` (since
    /// `by_addr` is keyed by `canonical_addr`). Linear scan; O(n)
    /// but only called by helper-IR emission per branch extern, not
    /// on a hot path.
    pub fn lookup_by_synth(&self, synth_addr: usize) -> Option<&SymbolEntry> {
        self.by_addr
            .values()
            .find(|e| e.synthetic_addr == synth_addr)
    }

    /// M9-review: generic native-address → synthetic-offset
    /// translation for ANY address, not just symbol entries. Walks
    /// [`image_rebases`] and applies the per-image formula
    /// `synth = synth_base + (native - native_base)` when `native`
    /// falls inside a tracked image's `[native_base..native_end)`
    /// range.
    ///
    /// Returns `None` for addresses outside every tracked image;
    /// the caller decides whether to fall back to the input
    /// address or treat as an error.
    ///
    /// Used SERVER-SIDE to translate data-symbol values that get
    /// captured natively (e.g. `_MyDataModel_RttiTypeId` which
    /// contains the native pointer of `_MyDataModel_RttiTypePtrId`)
    /// into the synth space lifted callbacks operate in. The cb's
    /// lifted `adrp + add x1, x1, #offset` produces a synth
    /// address; the JS-supplied identifier (captured natively)
    /// must be the SAME synth address for `isType` to succeed.
    pub fn native_to_synth(&self, native_addr: usize) -> Option<usize> {
        for r in &self.image_rebases {
            if native_addr >= r.native_base && native_addr < r.native_end {
                return Some(r.synth_base.wrapping_add(native_addr - r.native_base));
            }
        }
        None
    }

    /// A1 keystone fix, x86 edition: synthesize a `Recursable` entry for an
    /// in-image synth address that has NO symbol. The Windows PDB leaves a
    /// coverage gap (~200 unsymboled functions in azul.dll's text-shaping /
    /// allsorts region — LTO-internalized / compiler-outlined code with no
    /// S_PUB32/S_GPROC32 record). An unsymboled `call` target is REAL code
    /// that must be lifted; the old `None` path stubbed it as an env-import,
    /// and the stub returns garbage that crashes text layout — the exact
    /// aarch64 A1 bug ("a `String`'s len holds a heap pointer → phantom huge
    /// &str → OOB"), x86 edition, surfaced once switches actually lift their
    /// arms and the real shaper runs.
    ///
    /// `native = native_base + (synth - synth_base)` for the containing image;
    /// `size` = gap to the next known symbol (capped at 64 KiB — generous, and
    /// remill stops at the function's `ret` regardless). Returns `None` only
    /// when `synth` is outside every tracked image (genuinely external — leave
    /// it a stub). Callers should only pass `call`-target synths (which are code
    /// by construction); a stray data address would just fail the remill lift
    /// and fall back to a Leaf stub, no worse than skipping.
    pub fn synthesize_text_entry(&self, synth_addr: usize) -> Option<SymbolEntry> {
        let r = self.image_rebases.iter().find(|r| {
            let synth_end = r.synth_base.wrapping_add(r.native_end - r.native_base);
            synth_addr >= r.synth_base && synth_addr < synth_end
        })?;
        let native = r.native_base.wrapping_add(synth_addr - r.synth_base);
        let next_addr = self.by_addr.range((native + 1)..).next().map(|(a, _)| *a);
        let size = match next_addr {
            Some(n) => n.saturating_sub(native).min(0x10000),
            None => r.native_end.saturating_sub(native).min(0x10000),
        };
        if size == 0 {
            return None;
        }
        // SAFETY: `native` is inside the loaded image's live mapping (mapped for
        // the process lifetime); `size` is bounded by the next symbol / image end.
        let bytes = unsafe { std::slice::from_raw_parts(native as *const u8, size) };
        Some(SymbolEntry {
            canonical_name: format!("sub_{:x}", synth_addr),
            canonical_addr: native,
            synthetic_addr: synth_addr,
            size,
            bytes: Some(bytes),
            kind: SymKind::Function,
            classification: FnClass::Recursable,
        })
    }

    /// Find Recursable function entries whose canonical name contains ALL of
    /// `must_contain`'s substrings. Used to seed the transitive lift with
    /// fn-POINTER-called targets (their pointers live in mirrored DATA — e.g.
    /// `azul_core::task::Instant::now` stored in GetSystemTimeCallback — so the
    /// BL/B byte-scan and the adrp+add scan never discover them).
    pub fn find_recursable_by_name(&self, must_contain: &[&str]) -> Vec<(String, usize, usize)> {
        self.by_addr
            .values()
            .filter(|e| {
                e.classification.is_recursable()
                    && must_contain.iter().all(|p| e.canonical_name.contains(p))
            })
            .map(|e| (e.canonical_name.clone(), e.canonical_addr, e.size))
            .collect()
    }

    /// True when `addr` falls inside any tracked image's SYNTH span
    /// `[synth_base, synth_base + image_len)`. Used by the indirect-call
    /// dispatcher to reject native-truncated alias case labels that would
    /// collide with a real synthetic address (mis-routing a dispatch is far
    /// worse than dropping it).
    pub fn is_synth_in_image_span(&self, addr: usize) -> bool {
        for r in &self.image_rebases {
            let len = r.native_end.saturating_sub(r.native_base);
            if addr >= r.synth_base && addr < r.synth_base.wrapping_add(len) {
                return true;
            }
        }
        false
    }

    /// [WEB-LIFT 2026-06-11] TLV geometry of every loaded image that has
    /// thread-locals (see [`TlvRegion`]). Live addresses.
    pub fn tlv_regions(&self) -> &[TlvRegion] {
        &self.tlv_regions
    }

    /// True when `live_addr` is the THUNK field (offset 0 of a 24-byte
    /// descriptor) inside some image's `__thread_vars`. The data mirror
    /// rewrites exactly these 8-byte slots to `AZ_TLV_MAGIC_PC`.
    pub fn is_tlv_thunk_slot(&self, live_addr: usize) -> bool {
        for r in &self.tlv_regions {
            if live_addr >= r.vars_start
                && live_addr < r.vars_start + r.vars_size
                && (live_addr - r.vars_start) % 24 == 0
            {
                return true;
            }
        }
        false
    }

    /// The wasm-side TLS base the dispatcher's TLV case adds each
    /// descriptor's `offset` to. This is the SYNTH address of
    /// `__thread_data` — the data mirror places section bytes at synth
    /// offsets and lifted `adrp` pages are synth-rebased, so the whole
    /// TLV access stays in synth space (NOT the truncated live address).
    /// One TLS area is supported (libazul's — the only image whose code
    /// gets lifted); `None` when the process has no thread-locals.
    pub fn tlv_tls_base_synth(&self) -> Option<u32> {
        let r = self.tlv_regions.first()?;
        self.native_to_synth(r.tls_base).map(|s| s as u32)
    }
}

// ── Image enumeration ───────────────────────────────────────────────

struct LoadedImage {
    path: PathBuf,
    /// ASLR slide for this image (live_addr = file_addr + slide).
    slide: usize,
    /// File contents (read once at table-build time). Kept on
    /// `SymbolTable` so we can re-parse for future extensions
    /// without re-reading disk.
    bytes: Vec<u8>,
}

#[cfg(target_os = "macos")]
fn enumerate_loaded_images() -> Result<Vec<LoadedImage>, BuildError> {
    extern "C" {
        fn _dyld_image_count() -> u32;
        fn _dyld_get_image_name(image_index: u32) -> *const core::ffi::c_char;
        fn _dyld_get_image_vmaddr_slide(image_index: u32) -> isize;
    }
    let mut out = Vec::new();
    unsafe {
        let n = _dyld_image_count();
        for i in 0..n {
            let name_ptr = _dyld_get_image_name(i);
            if name_ptr.is_null() {
                continue;
            }
            let Ok(name) = core::ffi::CStr::from_ptr(name_ptr).to_str() else {
                continue;
            };
            let path = PathBuf::from(name);
            // Skip clearly-irrelevant images to keep startup fast.
            // The macOS shared-cache holds hundreds of system dylibs
            // (CoreFoundation, libobjc, the entire frameworks bundle)
            // that we never lift into. Match by path prefix.
            if is_system_image(&path) {
                continue;
            }
            let slide = _dyld_get_image_vmaddr_slide(i) as usize;
            let bytes = match fs::read(&path) {
                Ok(b) => b,
                Err(e) => {
                    eprintln!(
                        "[symbol_table] skipping {}: read error: {}",
                        path.display(),
                        e
                    );
                    continue;
                }
            };
            out.push(LoadedImage { path, slide, bytes });
        }
    }
    Ok(out)
}

#[cfg(target_os = "linux")]
fn enumerate_loaded_images() -> Result<Vec<LoadedImage>, BuildError> {
    use std::os::raw::{c_char, c_int, c_void};

    #[repr(C)]
    struct DlPhdrInfo {
        dlpi_addr: usize,
        dlpi_name: *const c_char,
        dlpi_phdr: *const c_void,
        dlpi_phnum: u16,
        // Trailing fields exist on newer glibc but we don't need them.
    }
    extern "C" {
        fn dl_iterate_phdr(
            callback: extern "C" fn(*mut DlPhdrInfo, usize, *mut c_void) -> c_int,
            data: *mut c_void,
        ) -> c_int;
    }
    extern "C" fn cb(
        info: *mut DlPhdrInfo,
        _size: usize,
        data: *mut c_void,
    ) -> c_int {
        unsafe {
            let images = &mut *(data as *mut Vec<LoadedImage>);
            if info.is_null() {
                return 0;
            }
            let info = &*info;
            if info.dlpi_name.is_null() {
                return 0;
            }
            let Ok(name) = core::ffi::CStr::from_ptr(info.dlpi_name).to_str() else {
                return 0;
            };
            // Empty name = the main executable. dl_iterate_phdr passes
            // the executable as the first entry with an empty name.
            let path = if name.is_empty() {
                match std::env::current_exe() {
                    Ok(p) => p,
                    Err(_) => return 0,
                }
            } else {
                PathBuf::from(name)
            };
            if is_system_image(&path) {
                return 0;
            }
            let slide = info.dlpi_addr;
            let bytes = match fs::read(&path) {
                Ok(b) => b,
                Err(_) => return 0,
            };
            images.push(LoadedImage { path, slide, bytes });
        }
        0
    }
    let mut images: Vec<LoadedImage> = Vec::new();
    unsafe {
        let raw = &mut images as *mut _ as *mut std::os::raw::c_void;
        dl_iterate_phdr(cb, raw);
    }
    Ok(images)
}

/// Windows: walk the process module list via `K32EnumProcessModules`.
/// The PE analog of the Mach-O `vmaddr + slide` convention: every
/// ingest fn computes `live_addr = file_pref_va + slide` where
/// `file_pref_va = OptionalHeader.ImageBase + RVA` (the address the
/// file *asks* to be loaded at) and `slide = actual_module_base -
/// preferred ImageBase` (the relocation delta; 0 when the loader
/// honored the preference, which never happens under default ASLR).
#[cfg(target_os = "windows")]
fn enumerate_loaded_images() -> Result<Vec<LoadedImage>, BuildError> {
    use std::os::windows::ffi::OsStringExt;
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
    }

    let mut modules: Vec<Hmodule> = vec![core::ptr::null_mut(); 1024];
    let mut needed: u32 = 0;
    let ok = unsafe {
        K32EnumProcessModules(
            GetCurrentProcess(),
            modules.as_mut_ptr(),
            (modules.len() * core::mem::size_of::<Hmodule>()) as u32,
            &mut needed,
        )
    };
    if ok == 0 {
        return Err(BuildError {
            stage: "EnumProcessModules",
            message: "K32EnumProcessModules failed".into(),
        });
    }
    let count = (needed as usize / core::mem::size_of::<Hmodule>()).min(modules.len());
    let mut out = Vec::new();
    for &module in &modules[..count] {
        if module.is_null() {
            continue;
        }
        let mut buf = [0u16; 1024];
        let len = unsafe { GetModuleFileNameW(module, buf.as_mut_ptr(), buf.len() as u32) };
        if len == 0 {
            continue;
        }
        let path = PathBuf::from(std::ffi::OsString::from_wide(&buf[..len as usize]));
        if is_system_image(&path) {
            continue;
        }
        let bytes = match fs::read(&path) {
            Ok(b) => b,
            Err(e) => {
                eprintln!(
                    "[symbol_table] skipping {}: read error: {}",
                    path.display(),
                    e
                );
                continue;
            }
        };
        // slide = actual base − preferred ImageBase, so that
        // `pref_va + slide == live_va` matches the shared convention.
        let preferred = match goblin::pe::PE::parse(&bytes) {
            Ok(pe) => pe.image_base as usize,
            Err(_) => {
                continue;
            }
        };
        let slide = (module as usize).wrapping_sub(preferred);
        out.push(LoadedImage { path, slide, bytes });
    }
    Ok(out)
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
fn enumerate_loaded_images() -> Result<Vec<LoadedImage>, BuildError> {
    Err(BuildError {
        stage: "platform",
        message: "SymbolTable not implemented for this OS yet".into(),
    })
}

/// Filter out images we never want to walk — the macOS shared
/// dyld_shared_cache + system frameworks have hundreds of dylibs that
/// the lift pipeline will never lift into. Skipping them at this stage
/// drops table-build time from seconds to <100 ms.
fn is_system_image(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    // Windows: everything under %SystemRoot% (ntdll, kernel32, ucrtbase,
    // the api-ms-win-* api sets, …). Imports INTO these are still named
    // via the importing image's import directory (see `ingest_pe`), so
    // skipping the images themselves loses nothing the lift needs —
    // exactly like skipping libSystem on macOS.
    #[cfg(target_os = "windows")]
    {
        let lower = s.to_ascii_lowercase();
        if lower.contains(":\\windows\\") || lower.contains(":/windows/") {
            return true;
        }
    }
    s.starts_with("/usr/lib/")
        || s.starts_with("/System/Library/")
        || s.starts_with("/Library/Apple/")
        // macOS dyld shared cache pseudo-paths: dyld inserts placeholder
        // entries like "/usr/lib/dyld" + system frameworks even when no
        // such file exists on disk (the bytes live inside the shared
        // cache). They won't be readable; skip preemptively.
        || s.contains("/dyld")
        // Linux equivalents: glibc, ld-linux, vDSO.
        || s.starts_with("/lib/")
        || s.starts_with("/lib64/")
        || s.contains("linux-vdso")
        || s.contains("ld-linux")
}

// ── PE / PDB ingestion (Windows) ────────────────────────────────────
//
// PE linked images carry no symbol table — internal symbol names live
// in the .pdb next to the image (or at the absolute path embedded in
// the PE debug directory). Names are LOAD-BEARING for the classifier:
// an unnamed internal fn falls to the default classification and the
// whole A1 "Leaf-stub garbage" class comes back (see
// scripts/WEB_LIFT_BUG_COMPENDIUM.md). So a missing PDB degrades to
// exports-only ingestion with a LOUD warning.
//
// Import calls on Windows go through the IAT: either a direct
// `call qword ptr [rip+disp]` (no stub function at all — the lift's
// extern resolution sees the IAT slot address) or a `jmp [rip+disp]`
// thunk (the PE analog of the macOS `__TEXT.__stubs` trampoline,
// compendium A2). Both are wired here: every import gets a synthetic
// SymbolEntry at its RESOLVED live address (read out of our own IAT,
// which the loader bound at startup), named by the import name so
// `classify_for_name` matches `memcpy`/`memset`/… exactly like the
// macOS PLT-chase produced libSystem names.

/// Per-image record of one IAT slot: `slot_live` (the address the
/// `[rip+disp]` operand resolves to) → the import's name + the live
/// resolved target the loader wrote into the slot.
#[cfg(target_os = "windows")]
struct PeIatSlot {
    slot_live: usize,
    target_live: usize,
    name: String,
}

#[cfg(target_os = "windows")]
fn ingest_pe(
    pe: &goblin::pe::PE<'_>,
    file_bytes: &[u8],
    slide: usize,
    path: &std::path::Path,
    api: &HashMap<String, ApiFnClass>,
    by_addr: &mut BTreeMap<usize, SymbolEntry>,
    by_name: &mut HashMap<String, usize>,
    chain: &mut HashMap<usize, usize>,
) -> Result<(), BuildError> {
    const IMAGE_SCN_CNT_CODE: u32 = 0x0000_0020;
    let image_base = pe.image_base as usize;
    let live_base = image_base.wrapping_add(slide);

    // 1) .text range in file-preferred-VA space (so the next-addr size
    //    trick mirrors ingest_macho exactly).
    let mut text_section: Option<(usize, usize)> = None;
    for s in &pe.sections {
        let name = s.name().unwrap_or("");
        if name == ".text" || (text_section.is_none() && (s.characteristics & IMAGE_SCN_CNT_CODE) != 0) {
            let start = image_base + s.virtual_address as usize;
            let size = s.virtual_size as usize;
            if name == ".text" {
                text_section = Some((start, size));
                break;
            }
            text_section = Some((start, size));
        }
    }
    let Some((text_start, text_size)) = text_section else {
        return Ok(()); // resource-only image — skip
    };
    let text_end = text_start + text_size;

    // 2) Defined symbols: PDB publics (mangled `_ZN…` link names — the
    //    names classify_for_name keys on) + module-stream procs (cover
    //    LTO-internalized locals; display-style names). Fallback:
    //    export table only.
    let mut defined: Vec<(String, usize)> = Vec::new();
    let pdb_result = read_pdb_function_symbols(pe, path, image_base, &mut defined);
    if let Err(msg) = pdb_result {
        eprintln!(
            "[symbol_table] WARNING: no PDB symbols for {} ({}) — falling back to \
             EXPORTS ONLY. Internal fns will classify by default rules and the \
             lift WILL mis-stub them (compendium A1). Build with debuginfo to fix.",
            path.display(),
            msg
        );
    }
    for e in &pe.exports {
        if let (Some(name), rva) = (e.name, e.rva) {
            if rva != 0 {
                defined.push((name.to_string(), image_base + rva));
            }
        }
    }

    // Sort + dedup by addr, preferring public C names over mangled
    // internal aliases at the same address (same as ingest_macho).
    defined.sort_by(|(a_name, a_addr), (b_name, b_addr)| {
        a_addr
            .cmp(b_addr)
            .then_with(|| public_name_score(b_name).cmp(&public_name_score(a_name)))
    });
    let mut seen_addr: HashSet<usize> = HashSet::new();
    defined.retain(|(_, addr)| seen_addr.insert(*addr));

    // 3) Text symbols → SymbolEntry with live bytes + next-addr sizes.
    let text_syms: Vec<(String, usize)> = defined
        .iter()
        .filter(|(_, a)| (text_start..text_end).contains(a))
        .cloned()
        .collect();
    for (i, (raw_name, file_addr)) in text_syms.iter().enumerate() {
        let live_addr = file_addr.wrapping_add(slide);
        let next_addr = if i + 1 < text_syms.len() {
            text_syms[i + 1].1
        } else {
            text_end
        };
        let size = next_addr.saturating_sub(*file_addr);
        if size == 0 {
            continue;
        }
        // NOTE: no strip_leading_underscore here — x64 PE link names
        // have no `_` prefix, and stripping would corrupt `_ZN…`
        // mangled names into `ZN…` (breaking the crate-name parse in
        // classify_for_name).
        let canonical_name = raw_name.clone();
        let bytes: Option<&'static [u8]> = unsafe {
            if live_addr != 0 && size > 0 {
                Some(core::slice::from_raw_parts(live_addr as *const u8, size))
            } else {
                None
            }
        };
        let classification = classify_for_name(&canonical_name, api);
        let entry = SymbolEntry {
            canonical_name: canonical_name.clone(),
            canonical_addr: live_addr,
            synthetic_addr: live_addr, // assigned in pass 2
            size,
            bytes,
            kind: SymKind::Function,
            classification,
        };
        upsert_entry(by_addr, live_addr, entry);
        by_name.entry(canonical_name).or_insert(live_addr);
    }

    // 4) Imports: synthesize a named entry at every import's RESOLVED
    //    live address (loader-bound IAT slot content). This is what
    //    the macOS PLT-chase produced for libSystem callees — the name
    //    is enough for LibcMemcpy/LibcMemset/HashmapRandomKeys/… and
    //    the entry terminates resolve() chains from thunks.
    let mut iat_slots: Vec<PeIatSlot> = Vec::new();
    for imp in &pe.imports {
        let slot_live = live_base.wrapping_add(imp.rva);
        let target_live = unsafe { core::ptr::read_unaligned(slot_live as *const u64) } as usize;
        if target_live < 0x1_0000 {
            continue; // unbound/garbage slot — skip defensively
        }
        let name = imp.name.to_string();
        let entry = SymbolEntry {
            canonical_name: name.clone(),
            canonical_addr: target_live,
            synthetic_addr: target_live,
            size: 0,
            bytes: None,
            kind: SymKind::Function,
            classification: classify_for_name(&name, api),
        };
        // or-insert semantics: don't clobber a real in-image symbol.
        if !by_addr.contains_key(&target_live) {
            upsert_entry(by_addr, target_live, entry);
        }
        by_name.entry(name.clone()).or_insert(target_live);
        chain.entry(target_live).or_insert(target_live);
        iat_slots.push(PeIatSlot { slot_live, target_live, name });
    }

    // 5) Tail-call shims, x86 spelling (compendium B7/A2):
    //      E9 rel32              jmp rel32      (intra-image tail shim,
    //                                            e.g. __rust_alloc → __rdl_alloc)
    //      FF 25 disp32          jmp [rip+disp] (IAT import thunk)
    //    Both are FIRST-INSTRUCTION-anchored, so no length decode is
    //    needed here (the transpiler-side scanners use iced-x86).
    detect_pe_tail_shims(by_addr, chain, &iat_slots);

    Ok(())
}

/// Read function symbols out of the image's PDB into `defined` as
/// `(link_name, file_preferred_va)` pairs. Errors return a message
/// string (caller logs + degrades to exports-only).
#[cfg(target_os = "windows")]
fn read_pdb_function_symbols(
    pe: &goblin::pe::PE<'_>,
    image_path: &std::path::Path,
    image_base: usize,
    defined: &mut Vec<(String, usize)>,
) -> Result<(), String> {
    // SymbolIter / ModuleIter are fallible iterators, not std ones.
    use pdb::FallibleIterator;
    // PDB path candidates, in order: the embedded codeview path as-is
    // (absolute when the linker recorded one), the embedded FILENAME
    // resolved against the image's own directory (rustc/link often
    // record just "azul.pdb"), and `<image>.pdb` as a final fallback.
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(cv) = pe.debug_data.as_ref().and_then(|d| d.codeview_pdb70_debug_info.as_ref()) {
        let raw = cv.filename;
        let trimmed: &[u8] = raw.split(|b| *b == 0).next().unwrap_or(raw);
        if let Ok(s) = core::str::from_utf8(trimmed) {
            if !s.is_empty() {
                let embedded = PathBuf::from(s);
                if let (Some(fname), Some(dir)) = (embedded.file_name(), image_path.parent()) {
                    candidates.push(dir.join(fname));
                }
                candidates.push(embedded);
            }
        }
    }
    candidates.push(image_path.with_extension("pdb"));

    let pdb_path = candidates
        .iter()
        .find(|p| p.exists())
        .ok_or_else(|| format!("no .pdb found (tried {:?})", candidates))?;
    let file = fs::File::open(pdb_path).map_err(|e| format!("open {}: {}", pdb_path.display(), e))?;
    let mut pdb = pdb::PDB::open(file).map_err(|e| format!("parse {}: {}", pdb_path.display(), e))?;
    let address_map = pdb.address_map().map_err(|e| format!("address_map: {}", e))?;

    let mut n_publics = 0usize;
    let mut n_procs = 0usize;

    // Publics: linker-level symbols with the mangled LINK name.
    if let Ok(globals) = pdb.global_symbols() {
        let mut iter = globals.iter();
        while let Ok(Some(symbol)) = iter.next() {
            let Ok(data) = symbol.parse() else { continue };
            if let pdb::SymbolData::Public(p) = data {
                if !(p.code || p.function) {
                    continue;
                }
                let Some(rva) = p.offset.to_rva(&address_map) else { continue };
                if rva.0 == 0 {
                    continue;
                }
                defined.push((p.name.to_string().into_owned(), image_base + rva.0 as usize));
                n_publics += 1;
            }
        }
    }

    // Module procs: cover internal (LTO-internalized / non-public)
    // fns the publics stream misses. Names here are display-style
    // (`azul_core::refany::…`) — classify_for_name has a path-style
    // fallback for these.
    if let Ok(di) = pdb.debug_information() {
        if let Ok(mut modules) = di.modules() {
            while let Ok(Some(module)) = modules.next() {
                let Ok(Some(mi)) = pdb.module_info(&module) else { continue };
                let Ok(mut syms) = mi.symbols() else { continue };
                while let Ok(Some(symbol)) = syms.next() {
                    let Ok(data) = symbol.parse() else { continue };
                    if let pdb::SymbolData::Procedure(p) = data {
                        let Some(rva) = p.offset.to_rva(&address_map) else { continue };
                        if rva.0 == 0 {
                            continue;
                        }
                        defined.push((p.name.to_string().into_owned(), image_base + rva.0 as usize));
                        n_procs += 1;
                    }
                }
            }
        }
    }

    if n_publics == 0 && n_procs == 0 {
        return Err(format!("{}: parsed but contained 0 function symbols", pdb_path.display()));
    }
    eprintln!(
        "[symbol_table] {}: {} publics + {} procs from {}",
        image_path.display(),
        n_publics,
        n_procs,
        pdb_path.display()
    );
    Ok(())
}

/// Walk every Function entry whose first instruction is an x86 tail
/// jump and reclassify it as a Stub, mirroring `detect_arm64_tail_shims`:
///   `E9 rel32`     → intra-image tail shim, target = addr + 5 + rel32.
///   `FF 25 disp32` → IAT thunk; the [rip+disp] slot must match a known
///                    IAT slot, target = the loader-resolved address.
#[cfg(target_os = "windows")]
fn detect_pe_tail_shims(
    by_addr: &mut BTreeMap<usize, SymbolEntry>,
    chain: &mut HashMap<usize, usize>,
    iat_slots: &[PeIatSlot],
) {
    use std::collections::HashMap as Map;
    let slot_to_target: Map<usize, (usize, &str)> = iat_slots
        .iter()
        .map(|s| (s.slot_live, (s.target_live, s.name.as_str())))
        .collect();
    let candidates: Vec<(usize, usize)> = by_addr
        .iter()
        .filter_map(|(addr, e)| {
            if !matches!(e.kind, SymKind::Function) {
                return None;
            }
            let bytes = e.bytes?;
            if bytes.len() >= 5 && bytes[0] == 0xE9 {
                let rel = i32::from_le_bytes([bytes[1], bytes[2], bytes[3], bytes[4]]);
                let target = (*addr as isize).wrapping_add(5).wrapping_add(rel as isize) as usize;
                return Some((*addr, target));
            }
            if bytes.len() >= 6 && bytes[0] == 0xFF && bytes[1] == 0x25 {
                let disp = i32::from_le_bytes([bytes[2], bytes[3], bytes[4], bytes[5]]);
                let slot = (*addr as isize).wrapping_add(6).wrapping_add(disp as isize) as usize;
                if let Some((target, _name)) = slot_to_target.get(&slot) {
                    return Some((*addr, *target));
                }
                return None;
            }
            None
        })
        .collect();
    for (addr, target) in candidates {
        if let Some(e) = by_addr.get_mut(&addr) {
            e.kind = SymKind::Stub { target };
            e.canonical_addr = target;
        }
        chain.insert(addr, target);
    }
}

/// PE: min/max FILE-preferred-VA across all mapped sections (text +
/// data + rdata), mirroring `macho_image_text_data_range` semantics so
/// `assign_synthetic_addresses` covers the data sections the mirror
/// will copy.
fn pe_image_text_data_range(pe: &goblin::pe::PE<'_>) -> (usize, usize) {
    const IMAGE_SCN_MEM_DISCARDABLE: u32 = 0x0200_0000;
    let image_base = pe.image_base as usize;
    let mut min_va = usize::MAX;
    let mut max_va = 0usize;
    for s in &pe.sections {
        if s.virtual_size == 0 || (s.characteristics & IMAGE_SCN_MEM_DISCARDABLE) != 0 {
            continue;
        }
        let start = image_base + s.virtual_address as usize;
        let end = start + s.virtual_size as usize;
        min_va = min_va.min(start);
        max_va = max_va.max(end);
    }
    if min_va == usize::MAX {
        (0, 0)
    } else {
        (min_va, max_va)
    }
}

// ── Mach-O ingestion ────────────────────────────────────────────────

pub(crate) fn pick_fat_slice<'a>(
    fat: &goblin::mach::MultiArch<'a>,
    _all_bytes: &[u8],
) -> Result<Option<goblin::mach::MachO<'a>>, BuildError> {
    // Best-effort: walk fat arches, take the first MachO entry.
    // Real disambiguation would compare arch tags against host_arch_tag,
    // but live-loaded images aren't fat — dyld picks the right slice
    // before mapping. This is for tooling robustness when the on-disk
    // image happens to be a universal binary.
    for entry in fat.into_iter() {
        let Ok(entry) = entry else { continue };
        match entry {
            goblin::mach::SingleArch::MachO(m) => return Ok(Some(m)),
            goblin::mach::SingleArch::Archive(_) => continue,
        }
    }
    Ok(None)
}

fn ingest_macho(
    macho: &goblin::mach::MachO<'_>,
    file_bytes: &[u8],
    slide: usize,
    api: &HashMap<String, ApiFnClass>,
    by_addr: &mut BTreeMap<usize, SymbolEntry>,
    by_name: &mut HashMap<String, usize>,
    chain: &mut HashMap<usize, usize>,
) -> Result<(), BuildError> {
    use goblin::mach::load_command::{
        CommandVariant, SIZEOF_SECTION_64, SIZEOF_SEGMENT_COMMAND_64,
    };

    // Locate __TEXT.__text + __TEXT.__stubs. Goblin's generalized
    // `Section` strips `reserved1/reserved2`, which __stubs needs
    // (per-stub byte size + indirect-symtab base index). Walk
    // load_commands directly to read Section64 with those fields
    // intact via a hand-rolled byte parser (avoids dragging scroll
    // into the dep tree explicitly).
    let mut text_section: Option<(usize, usize)> = None; // (file_vmaddr, size)
    let mut stubs_section: Option<MachOStubsInfo> = None;

    for lc in &macho.load_commands {
        let CommandVariant::Segment64(seg64) = &lc.command else { continue };
        let segname = trim_macho_name(&seg64.segname);
        if segname != "__TEXT" {
            continue;
        }
        let sections_off = lc.offset + SIZEOF_SEGMENT_COMMAND_64;
        for i in 0..seg64.nsects as usize {
            let off = sections_off + i * SIZEOF_SECTION_64;
            if off + SIZEOF_SECTION_64 > file_bytes.len() {
                break;
            }
            let Some(s) = parse_section64(&file_bytes[off..off + SIZEOF_SECTION_64]) else {
                continue;
            };
            let sectname = trim_macho_name(&s.sectname);
            match sectname {
                "__text" => {
                    text_section = Some((s.addr as usize, s.size as usize));
                }
                "__stubs" => {
                    stubs_section = Some(MachOStubsInfo {
                        addr: s.addr as usize,
                        size: s.size as usize,
                        stub_size: s.reserved2 as usize,
                        indirect_start: s.reserved1 as usize,
                    });
                }
                _ => {}
            }
        }
    }

    let (text_start, text_size) = match text_section {
        Some(v) => v,
        None => return Ok(()), // image with no __text — skip
    };
    let text_end = text_start + text_size;

    // 1) Collect all defined symbols (name + file_addr).
    //    Mach-O nlist: n_type & N_TYPE (0x0E mask) == N_SECT (0x0E)
    //    means "defined in a section". n_value is the VM address.
    //    Skip N_STAB debug symbols (n_type & N_STAB == any nonzero).
    let mut defined: Vec<(String, usize)> = Vec::new();
    for sym in macho.symbols() {
        let Ok((name, nlist)) = sym else { continue };
        if name.is_empty() {
            continue;
        }
        // Skip stabs (debug symbols).
        if nlist.n_type & N_STAB != 0 {
            continue;
        }
        // Defined in a section?
        if nlist.n_type & N_TYPE != N_SECT {
            continue;
        }
        let addr = nlist.n_value as usize;
        if addr == 0 {
            continue;
        }
        defined.push((name.to_string(), addr));
    }
    // Sort + dedup by addr, PREFERRING the C-ABI public name over
    // mangled internal aliases when they share an address. Rust
    // codegen emits both `_AzRefCount_canBeSharedMut` (extern "C"
    // public symbol) and `__ZN4azul14__ffi_internal..._can_be_shared_mut`
    // (internal monomorphization) at the same address. The
    // SymbolTable's classification + downstream `by_name` lookup
    // both need the public name to match api.json + per-call
    // intercept logs.
    defined.sort_by(|(a_name, a_addr), (b_name, b_addr)| {
        a_addr
            .cmp(b_addr)
            .then_with(|| public_name_score(b_name).cmp(&public_name_score(a_name)))
    });
    let mut seen_addr: HashSet<usize> = HashSet::new();
    defined.retain(|(_, addr)| seen_addr.insert(*addr));

    // 2) Compute sizes from adjacent addrs. Restrict to symbols whose
    //    addr lies within [text_start, text_end) for the
    //    next-addr-as-size trick. Symbols outside __text are still
    //    recorded (so by_name resolution works) but their `size` is
    //    derived from the section they live in, or 0 for data.
    let text_syms: Vec<(String, usize)> = defined
        .iter()
        .filter(|(_, a)| (text_start..text_end).contains(a))
        .cloned()
        .collect();

    for (i, (raw_name, file_addr)) in text_syms.iter().enumerate() {
        let live_addr = file_addr + slide;
        let next_addr = if i + 1 < text_syms.len() {
            text_syms[i + 1].1
        } else {
            text_end
        };
        let size = next_addr.saturating_sub(*file_addr);
        if size == 0 {
            continue;
        }
        // De-underscore once at the symbol-table level so consumers
        // see the same name dladdr would have reported on this OS.
        // macOS prepends `_` to every C symbol in the symbol table.
        let canonical_name = strip_leading_underscore(raw_name);
        let bytes: Option<&'static [u8]> = unsafe {
            // SAFETY: live_addr/size are computed from the loaded
            // image's symbol table; __TEXT stays mapped for the
            // process lifetime; the slice is read-only.
            if live_addr != 0 && size > 0 {
                Some(core::slice::from_raw_parts(
                    live_addr as *const u8,
                    size,
                ))
            } else {
                None
            }
        };
        let mut classification = classify_for_name(&canonical_name, api);
        // WEB-LIFT FIX (2026-06-03): SP-restoring machine-outliner epilogues
        // (`add sp,sp,#N; ret` or `ldp ...,[sp],#N; ret`) are tail-jumped via INDIRECT `br Xn`
        // in the allsorts shaping path. classify_for_name marks the tiny ones Leaf → they lift as
        // a STUB that returns WITHOUT the `add sp` → SP never restored → downstream `unreachable`
        // (_OUTLINED_FUNCTION_2 = `add sp,sp,#0x30; ret` is the canonical case). Force them
        // Recursable so the REAL body (the SP restore) is lifted, and so the tail-call scan +
        // M12.7 dispatcher (both gated on is_recursable) pick them up.
        // CONTENT-BASED (not name-based): a fn whose FIRST instruction is an SP-restore
        // (`add sp,sp,#N` or `ldp ...,[sp],#N` post-index) AND that returns within a few instrs is
        // a machine-outliner EPILOGUE, tail-jumped via INDIRECT `br Xn` in the allsorts shaping
        // path. classify_for_name marks them Leaf → lifted as STUBS that return WITHOUT the
        // `add sp` → SP never restored → downstream `unreachable`. Detect by CONTENT (not the
        // "OUTLINED_FUNCTION" name) because upsert_entry's dedup can keep a non-outlined symbol at
        // the same address — the code is still the epilogue. Force Recursable so the REAL body
        // lifts. Normal fns start with a prologue (`sub sp`/`stp`), never `add sp`, so this is safe.
        // OUTLINED fn with an SP-restore anywhere = a tail-jumped epilogue → force Recursable so its
        // REAL body lifts. (A name-INDEPENDENT/pure-epilogue variant was tried but made MORE fns
        // Recursable → the tail-call scan lifted more → the address-sensitive lift shifted → the
        // JT_SEEDS stopped resolving (21 missing_blocks vs 7). Kept name-gated = the 21→7 state.)
        if canonical_name.contains("OUTLINED_FUNCTION") && live_addr != 0 {
            let lim = if size > 0 { size.min(256) } else { 64 };
            if lim >= 8 {
                let b = unsafe { core::slice::from_raw_parts(live_addr as *const u8, lim) };
                let mut o = 0usize;
                while o + 4 <= lim {
                    let ins = u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]);
                    if (ins & 0xFFC0_03FF) == 0x9100_03FF
                        || ((ins >> 22) == 0x2A3 && ((ins >> 5) & 0x1F) == 31)
                    {
                        classification = FnClass::Recursable;
                        break;
                    }
                    o += 4;
                }
            }
        }
        let entry = SymbolEntry {
            canonical_name: canonical_name.clone(),
            canonical_addr: live_addr,
            synthetic_addr: live_addr,  // assigned in pass 2
            size,
            bytes,
            kind: SymKind::Function,
            classification,
        };
        // Upsert: a previous image's stub_walk may have synthesized
        // a placeholder entry (size=0, bytes=None) at this address;
        // overwrite with our real symtab entry (size > 0, bytes
        // populated). For two entries with the same score, keep the
        // first (deterministic).
        upsert_entry(by_addr, live_addr, entry);
        by_name.entry(canonical_name).or_insert(live_addr);
    }

    // 3) Walk the __stubs section. Each stub is `stub_size` bytes
    //    (12 on arm64, 6 on x86_64). For stub index i:
    //      indirect_idx = indirect_symtab[indirect_start + i]
    //      target_name  = symtab[indirect_idx].name
    //      target_addr  = dlsym(target_name) -- in this process
    //
    //    The chain is stub_live_addr → target_live_addr, so the lift
    //    pipeline's resolve() unifies stub + real-callee references.
    if let Some(stubs) = stubs_section {
        ingest_macho_stubs(
            macho,
            file_bytes,
            slide,
            stubs,
            api,
            by_addr,
            by_name,
            chain,
        )?;
    }

    // Detect bare-`b imm26` tail-call shims (functions whose first
    // instruction is an unconditional branch to another symbol).
    // Pre-M8.8 these were patched at lift-time by
    // `rewrite_tailcall_wrapper`. Post-M8.8 they're chained at table
    // build, identically to PLT stubs: the chain map redirects the
    // shim's address to the real target, so both
    // `resolve_fn_ptr(shim_addr)` and the post-lift IR rewriter chase
    // through to the real body. The shim itself never lifts.
    //
    // arm64 only — non-arm64 platforms have different shim shapes and
    // aren't on the M8.8 critical path.
    #[cfg(target_arch = "aarch64")]
    detect_arm64_tail_shims(by_addr, chain);

    Ok(())
}

/// Walk every Function entry and reclassify as Stub when the first
/// instruction is `B imm26` (unconditional branch). The target
/// address is computed from imm26 sign-extended × 4. Chain map gains
/// `shim_addr → target_addr` so callers chase through transparently.
#[cfg(target_arch = "aarch64")]
fn detect_arm64_tail_shims(
    by_addr: &mut BTreeMap<usize, SymbolEntry>,
    chain: &mut HashMap<usize, usize>,
) {
    let candidates: Vec<(usize, usize)> = by_addr
        .iter()
        .filter_map(|(addr, e)| {
            if !matches!(e.kind, SymKind::Function) {
                return None;
            }
            let bytes = e.bytes?;
            if bytes.len() < 4 {
                return None;
            }
            let first = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
            // `B imm26`: bits 31..26 == 0b000101. (`BL` is 0b100101 —
            // a regular call, not a tail call; do not match.)
            if (first >> 26) & 0x3F != 0b000101 {
                return None;
            }
            // Decode imm26 as signed 26-bit offset, in 4-byte words.
            let imm26 = (first & 0x03FF_FFFF) as i32;
            // Sign-extend 26 bits to 32, then × 4 for byte offset.
            let imm_sext = (imm26 << 6) >> 6;
            let byte_offset = (imm_sext as isize) * 4;
            let target = (*addr as isize).wrapping_add(byte_offset) as usize;
            Some((*addr, target))
        })
        .collect();
    for (addr, target) in candidates {
        if let Some(e) = by_addr.get_mut(&addr) {
            e.kind = SymKind::Stub { target };
            e.canonical_addr = target;
        }
        chain.insert(addr, target);
    }
}

#[cfg(not(target_arch = "aarch64"))]
fn detect_arm64_tail_shims(
    _by_addr: &mut BTreeMap<usize, SymbolEntry>,
    _chain: &mut HashMap<usize, usize>,
) {
}

struct MachOStubsInfo {
    addr: usize,
    size: usize,
    /// Per-stub size in bytes. macOS arm64 = 12, x86_64 = 6.
    stub_size: usize,
    /// Starting index into LC_DYSYMTAB.indirectsymoff.
    indirect_start: usize,
}

fn ingest_macho_stubs(
    macho: &goblin::mach::MachO<'_>,
    file_bytes: &[u8],
    slide: usize,
    stubs: MachOStubsInfo,
    api: &HashMap<String, ApiFnClass>,
    by_addr: &mut BTreeMap<usize, SymbolEntry>,
    by_name: &mut HashMap<String, usize>,
    chain: &mut HashMap<usize, usize>,
) -> Result<(), BuildError> {
    let MachOStubsInfo {
        addr: stub_section_addr,
        size: stub_section_size,
        stub_size,
        indirect_start,
    } = stubs;
    if stub_size == 0 {
        return Ok(());
    }
    let n_stubs = stub_section_size / stub_size;

    // Locate the indirect symbol table offset + the regular symtab
    // entries from the load commands. Goblin parses these into
    // `macho.symbols()` (regular) and `macho.load_commands` (raw).
    // For the indirect symtab, walk load commands for Dysymtab.
    let mut indirect_off: Option<usize> = None;
    let mut n_indirect: Option<usize> = None;
    let mut symtab_off: Option<usize> = None;
    let mut strtab_off: Option<usize> = None;
    let mut strtab_size: Option<usize> = None;
    for lc in &macho.load_commands {
        match &lc.command {
            goblin::mach::load_command::CommandVariant::Symtab(sym) => {
                symtab_off = Some(sym.symoff as usize);
                strtab_off = Some(sym.stroff as usize);
                strtab_size = Some(sym.strsize as usize);
            }
            goblin::mach::load_command::CommandVariant::Dysymtab(dy) => {
                indirect_off = Some(dy.indirectsymoff as usize);
                n_indirect = Some(dy.nindirectsyms as usize);
            }
            _ => {}
        }
    }
    let (Some(indirect_off), Some(n_indirect), Some(symtab_off), Some(strtab_off), Some(strtab_size)) =
        (indirect_off, n_indirect, symtab_off, strtab_off, strtab_size)
    else {
        return Ok(());
    };

    // The regular symtab entry width is sizeof(nlist_64) = 16 on
    // 64-bit Mach-O (n_strx u32, n_type u8, n_sect u8, n_desc u16,
    // n_value u64).
    const NLIST64_SIZE: usize = 16;

    for i in 0..n_stubs {
        let table_idx_off = indirect_off + (indirect_start + i) * 4;
        if table_idx_off + 4 > file_bytes.len() {
            break;
        }
        if indirect_start + i >= n_indirect {
            break;
        }
        let mut buf = [0u8; 4];
        buf.copy_from_slice(&file_bytes[table_idx_off..table_idx_off + 4]);
        let entry = u32::from_le_bytes(buf);
        // Indirect symtab entries can be the sentinel INDIRECT_SYMBOL_LOCAL
        // (0x80000000) or INDIRECT_SYMBOL_ABS (0x40000000) for
        // non-indirected stubs (rare); skip those.
        const INDIRECT_LOCAL: u32 = 0x80000000;
        const INDIRECT_ABS: u32 = 0x40000000;
        if entry & (INDIRECT_LOCAL | INDIRECT_ABS) != 0 {
            continue;
        }
        let sym_idx = entry as usize;
        let nlist_off = symtab_off + sym_idx * NLIST64_SIZE;
        if nlist_off + NLIST64_SIZE > file_bytes.len() {
            continue;
        }
        // Read n_strx (u32) at offset 0.
        let n_strx = u32::from_le_bytes([
            file_bytes[nlist_off],
            file_bytes[nlist_off + 1],
            file_bytes[nlist_off + 2],
            file_bytes[nlist_off + 3],
        ]) as usize;
        if n_strx >= strtab_size {
            continue;
        }
        let str_start = strtab_off + n_strx;
        if str_start >= file_bytes.len() {
            continue;
        }
        let name_bytes = match file_bytes[str_start..].iter().position(|&b| b == 0) {
            Some(end) => &file_bytes[str_start..str_start + end],
            None => continue,
        };
        let Ok(name) = std::str::from_utf8(name_bytes) else {
            continue;
        };
        if name.is_empty() {
            continue;
        }

        let stub_live_addr = stub_section_addr + slide + i * stub_size;
        let canonical_name = strip_leading_underscore(name);
        // Resolve the target address via dlsym. This handles both
        // exported (most things) and lazily-resolved (rarely-used)
        // symbols correctly — dlsym does the binding for us.
        let target_addr = super::dlsym_self(&canonical_name).unwrap_or(0);
        let target_addr = if target_addr == 0 {
            // Try the underscored form as a fallback for symbols
            // dyld registers with the underscore intact.
            super::dlsym_self(name).unwrap_or(0)
        } else {
            target_addr
        };

        // Stub entry: record it under stub_live_addr with kind=Stub
        // pointing to target_addr. The chain map gets
        // stub_live_addr → target_addr so resolve() walks through.
        let stub_bytes: Option<&'static [u8]> = unsafe {
            if stub_live_addr != 0 && stub_size > 0 {
                Some(core::slice::from_raw_parts(
                    stub_live_addr as *const u8,
                    stub_size,
                ))
            } else {
                None
            }
        };
        // Stub itself: overwrite any prior entry (the symtab might
        // carry a private name for this stub, but the indirected
        // name is the canonical one for chain consumers).
        by_addr.insert(
            stub_live_addr,
            SymbolEntry {
                canonical_name: canonical_name.clone(),
                canonical_addr: if target_addr != 0 {
                    target_addr
                } else {
                    stub_live_addr
                },
                synthetic_addr: if target_addr != 0 {
                    target_addr
                } else {
                    stub_live_addr
                },  // assigned in pass 2
                size: stub_size,
                bytes: stub_bytes,
                kind: SymKind::Stub {
                    target: if target_addr != 0 { target_addr } else { stub_live_addr },
                },
                classification: classify_for_name(&canonical_name, api),
            },
        );
        // Wire the chain.
        if target_addr != 0 && target_addr != stub_live_addr {
            chain.insert(stub_live_addr, target_addr);
            // Synthesize a placeholder entry at the target if none
            // exists yet — but ONLY if no real entry is registered.
            // When the target's defining image (e.g. libazul.dylib)
            // is ingested later, its real entry (size > 0, bytes
            // populated) replaces this placeholder via `upsert_entry`.
            let placeholder = SymbolEntry {
                canonical_name: canonical_name.clone(),
                canonical_addr: target_addr,
                synthetic_addr: target_addr,  // assigned in pass 2
                size: 0,
                bytes: None,
                kind: SymKind::Function,
                classification: classify_for_name(&canonical_name, api),
            };
            upsert_entry(by_addr, target_addr, placeholder);
            by_name.entry(canonical_name).or_insert(target_addr);
        }
    }

    Ok(())
}

/// Insert or update an entry, preferring richer metadata. "Richer"
/// = bigger size + has bytes. When a stub_walk in one image
/// synthesizes a placeholder at an address that later gets a real
/// symtab entry from the defining image, the upsert promotes the
/// real entry. Equal-score collisions keep the existing entry
/// (deterministic for testing).
fn upsert_entry(
    by_addr: &mut BTreeMap<usize, SymbolEntry>,
    addr: usize,
    new_entry: SymbolEntry,
) {
    fn score(e: &SymbolEntry) -> u32 {
        let mut s = 0u32;
        if e.size > 0 {
            s += 2;
        }
        if e.bytes.is_some() {
            s += 1;
        }
        s
    }
    match by_addr.get_mut(&addr) {
        Some(existing) => {
            if score(&new_entry) > score(existing) {
                *existing = new_entry;
            }
        }
        None => {
            by_addr.insert(addr, new_entry);
        }
    }
}

// macOS Mach-O nlist N_TYPE bits.
const N_STAB: u8 = 0xe0;
const N_TYPE: u8 = 0x0e;
const N_SECT: u8 = 0x0e;

/// Trim trailing NULs off a 16-byte Mach-O name field, returning a
/// `&str` slice into the underlying buffer. Returns the empty string
/// if the bytes aren't UTF-8 (vanishingly unlikely for section /
/// segment names).
fn trim_macho_name(bytes: &[u8; 16]) -> &str {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    std::str::from_utf8(&bytes[..end]).unwrap_or("")
}

/// Minimal `Section64` carrying just the fields the SymbolTable's
/// ingester reads. The full Mach-O Section64 has more fields but
/// they're not consumed here.
struct LocalSection64 {
    sectname: [u8; 16],
    addr: u64,
    size: u64,
    reserved1: u32,
    reserved2: u32,
}

/// Hand-rolled Section64 reader. The 80-byte layout is fixed by
/// Mach-O 64-bit spec (loader.h `struct section_64`). Returns `None`
/// when the input is shorter than 80 bytes; the caller silently
/// skips malformed sections.
fn parse_section64(buf: &[u8]) -> Option<LocalSection64> {
    if buf.len() < 80 {
        return None;
    }
    let mut sectname = [0u8; 16];
    sectname.copy_from_slice(&buf[0..16]);
    // segname at 16..32 — not used.
    let addr = u64::from_le_bytes(buf[32..40].try_into().ok()?);
    let size = u64::from_le_bytes(buf[40..48].try_into().ok()?);
    // offset (48..52), align (52..56), reloff (56..60), nreloc (60..64),
    // flags (64..68) — not used here.
    let reserved1 = u32::from_le_bytes(buf[68..72].try_into().ok()?);
    let reserved2 = u32::from_le_bytes(buf[72..76].try_into().ok()?);
    // reserved3 (76..80) — not used here.
    Some(LocalSection64 {
        sectname,
        addr,
        size,
        reserved1,
        reserved2,
    })
}

// ── ELF ingestion (Linux) ───────────────────────────────────────────

fn ingest_elf(
    elf: &goblin::elf::Elf<'_>,
    file_bytes: &[u8],
    slide: usize,
    api: &HashMap<String, ApiFnClass>,
    by_addr: &mut BTreeMap<usize, SymbolEntry>,
    by_name: &mut HashMap<String, usize>,
    chain: &mut HashMap<usize, usize>,
) -> Result<(), BuildError> {
    // Defined function symbols: ST_TYPE == STT_FUNC, sym.st_value != 0,
    // sym.st_shndx != SHN_UNDEF. Iterate .symtab if present; fall back
    // to .dynsym (always present in shared libs).
    let collect_defined = |symtab: &goblin::elf::Symtab<'_>,
                            strtab: &goblin::strtab::Strtab<'_>|
     -> Vec<(String, usize, usize)> {
        let mut out: Vec<(String, usize, usize)> = Vec::new();
        for sym in symtab.iter() {
            let st_type = sym.st_type();
            if st_type != goblin::elf::sym::STT_FUNC
                && st_type != goblin::elf::sym::STT_OBJECT
            {
                continue;
            }
            if sym.st_value == 0 {
                continue;
            }
            if sym.st_shndx == goblin::elf::section_header::SHN_UNDEF as usize {
                continue;
            }
            let name = match strtab.get_at(sym.st_name) {
                Some(s) => s,
                None => continue,
            };
            if name.is_empty() {
                continue;
            }
            out.push((name.to_string(), sym.st_value as usize, sym.st_size as usize));
        }
        out
    };

    let mut defined = collect_defined(&elf.syms, &elf.strtab);
    if defined.is_empty() {
        defined = collect_defined(&elf.dynsyms, &elf.dynstrtab);
    }

    // Sort by address. Compute sizes either from sym.st_size (when
    // present) or from next - this.
    defined.sort_by_key(|(_, addr, _)| *addr);
    let mut prev_idx: Option<usize> = None;
    for i in 0..defined.len() {
        let (name, addr, size_hint) = &defined[i];
        let live_addr = addr + slide;
        let size = if *size_hint > 0 {
            *size_hint
        } else if i + 1 < defined.len() {
            defined[i + 1].1.saturating_sub(*addr)
        } else {
            // Last symbol — no upper bound. Use a small conservative
            // window so we don't read past the section end. Section
            // boundary computation would be cleaner; defer to M8.9.
            512
        };
        if size == 0 {
            continue;
        }
        let bytes: Option<&'static [u8]> = unsafe {
            if live_addr != 0 && size > 0 {
                Some(core::slice::from_raw_parts(
                    live_addr as *const u8,
                    size,
                ))
            } else {
                None
            }
        };
        let classification = classify_for_name(name, api);
        by_addr.entry(live_addr).or_insert(SymbolEntry {
            canonical_name: name.clone(),
            canonical_addr: live_addr,
            synthetic_addr: live_addr,  // assigned in pass 2
            size,
            bytes,
            kind: SymKind::Function,
            classification,
        });
        by_name.entry(name.clone()).or_insert(live_addr);
        prev_idx = Some(i);
    }
    let _ = prev_idx;
    let _ = file_bytes;
    // .plt + .got.plt walk for ELF stub chain: deferred to M8.9 (Linux
    // host support). The current lift pipeline runs on macOS arm64;
    // tests against Linux happen in CI separately and would surface
    // any need to populate `chain` here.
    let _ = chain;

    Ok(())
}

// ── Name normalization + classification ────────────────────────────

/// Rank a symbol name by how "public" it is. Higher = more preferred
/// when multiple aliases share an address. The C-ABI surface
/// (`_Az*` / `Az*`) wins over Rust-mangled monomorphizations
/// (`_ZN...`, `_R...`) which carry the same address as their wrapper
/// stubs.
fn public_name_score(name: &str) -> u32 {
    let stripped = name.trim_start_matches('_');
    if stripped.starts_with("Az") {
        return 100;
    }
    // Other public C names — anything not starting with the
    // double-underscore mangling prefixes.
    if !name.starts_with("__") && !name.starts_with("_Z") && !name.starts_with("_R") {
        return 80;
    }
    0
}

fn strip_leading_underscore(name: &str) -> String {
    // macOS C linkage prepends `_` to every external symbol in the
    // Mach-O symbol table (`_AzApp_create`), but dladdr returns the
    // un-underscored form for compatibility with C source identifiers.
    // Strip exactly one leading underscore — Rust-mangled symbols
    // (`__ZN…`, `__rust_alloc`) need their second underscore preserved.
    name.strip_prefix('_').unwrap_or(name).to_string()
}

/// Classify a canonical symbol name. Az* names look up against
/// api.json; bump / call_indirect / resolve_callback names hardcode
/// to the relevant FnClass; everything else gets Leaf or Recursable
/// based on coarse name patterns.
fn classify_for_name(name: &str, api: &HashMap<String, ApiFnClass>) -> FnClass {
    // 1) Special bridge symbols emitted by remill helper IR or the
    //    Rust runtime. The Rust allocator API has three layers of
    //    alias each with their own name:
    //      __rust_alloc — top-level public C ABI shim
    //      __rg_alloc   — global allocator wrapper (when
    //                     `#[global_allocator]` is set)
    //      __rdl_alloc  — default allocator (System) implementation
    //    The bare-`b` shim from __rust_alloc tail-calls __rdl_alloc
    //    (or __rg_alloc), so after `detect_arm64_tail_shims` chains
    //    them, the canonical address might land on any of the three
    //    names. Treat all three (plus their v0-mangled variants
    //    `*___rdl_alloc`, `*_alloc::ALLOC`, etc.) as BumpAlloc.
    //    Similarly for `*_alloc_zeroed`.
    //    Match suffix-after-stripping-underscores to cover
    //    macOS-style `___rust_alloc`, Linux-style `__rust_alloc`,
    //    and v0-mangled wrappers.
    let stripped = name.trim_start_matches('_');
    // Windows x64 stack probe (compendium A2): MSVC emits a `__chkstk`
    // call in the prologue of every fn with a frame > 4 KiB. On x64
    // the probe does NOT adjust RSP (RAX carries the size; only guard
    // pages get touched) — wasm linear memory needs no guard probing,
    // so a plain no-op Leaf IS "SP as if the probe ran". (The x86-32
    // `_chkstk` variant DOES move ESP and must never be Leaf'd; this
    // port targets x64 only.)
    if stripped == "chkstk" {
        return FnClass::Leaf;
    }
    // Order matters: `rust_alloc_zeroed` must match BEFORE `rust_alloc`
    // (which is a prefix). Similarly `rust_realloc` is a separate
    // family from `rust_alloc` — keep it ahead of the alloc check so
    // the alloc match doesn't accidentally trigger on `__rust_realloc`
    // (which doesn't, but we hardcode order to be defensive).
    for variant in ["rust_realloc", "rdl_realloc", "rg_realloc"] {
        if stripped == variant || stripped.ends_with(variant) {
            return FnClass::BumpRealloc;
        }
    }
    for variant in ["rust_dealloc", "rdl_dealloc", "rg_dealloc"] {
        if stripped == variant || stripped.ends_with(variant) {
            return FnClass::BumpDealloc;
        }
    }
    for variant in ["rust_alloc_zeroed", "rdl_alloc_zeroed", "rg_alloc_zeroed"] {
        if stripped == variant || stripped.ends_with(variant) {
            return FnClass::BumpAlloc;
        }
    }
    for variant in ["rust_alloc", "rdl_alloc", "rg_alloc"] {
        if stripped == variant || stripped.ends_with(variant) {
            return FnClass::BumpAlloc;
        }
    }
    // libc bulk-copy primitives: `memcpy` / `memmove` (and their
    // `_chk` and `_platform_` spellings, however the PLT-chase names
    // them — e.g. `memcpy`, `_platform_memmove`, `___memcpy_chk`).
    // The bare `_`-stripped name is matched so all spellings collapse.
    // These resolve to out-of-image libsystem addresses, so the
    // address-based M10-A1 pass would otherwise force `Leaf` (a no-op
    // stub) and drop every out-of-line struct copy. Classify as
    // `LibcMemcpy` so the helper IR emits a real `@llvm.memmove` body.
    {
        let core = stripped.strip_prefix("platform_").unwrap_or(stripped);
        if core.starts_with("memcpy") || core.starts_with("memmove") {
            return FnClass::LibcMemcpy;
        }
        // libc `memset` (+ `_platform_memset`, `___memset_chk`): same out-of-image
        // problem — a no-op stub drops hashbrown's control-byte EMPTY (0xFF) init,
        // hanging HashMap::insert. Emit a real `@llvm.memset` body.
        if core.starts_with("memset") {
            return FnClass::LibcMemset;
        }
        // libc `snprintf` / `vsnprintf` (+ `_chk` spellings, e.g. `__snprintf_chk`
        // → stripped `snprintf_chk`). Out-of-image; the no-op Leaf stub leaves the
        // caller's buffer empty (e.g. hello-world's counter label → height-0).
        // Emit a minimal "%d" formatter body.
        if core.starts_with("snprintf") || core.starts_with("vsnprintf") {
            return FnClass::LibcSnprintf;
        }
        // Windows ucrt spelling (compendium A2): MSVC's <stdio.h> makes
        // snprintf a static-inline wrapper over the IMPORTED
        // `__stdio_common_vsprintf(options, buf, len, fmt, locale, va_list)`.
        // The helper-IR body for x86_64 is emitted in that shape.
        if core.starts_with("stdio_common_vsprintf") {
            return FnClass::LibcSnprintf;
        }
    }
    // std HashMap entropy source. `std::sys::random::hashmap_random_keys`
    // seeds `RandomState`'s SipHash keys via a getentropy syscall that can't be
    // lifted. The default `core/std → Leaf` stub returns 0 and leaves the
    // (u64,u64) result unusable → every lifted `std::HashMap` degenerates to
    // empty (the M12.7 dom_to_layout symptom, systemically). Route it to a
    // dedicated helper that returns a FIXED non-zero seed so all lifted HashMaps
    // are internally consistent. (Matched on the std module path, not a one-off
    // mangled-name fragment — this is a known std primitive needing a real stub.)
    if stripped.contains("hashmap_random_keys") {
        return FnClass::HashmapRandomKeys;
    }
    // Web backend: the display-list painter surface
    // (`azul_layout::solver3::display_list` — `DisplayListGenerator` +
    // all `paint_*` helpers) is never executed in wasm. `layout_document`
    // gates its `generate_display_list` call behind `SKIP_DISPLAY_LIST`
    // (set by `AzStartup_solveLayoutReal`) because the web backend emits
    // TLV DOM patches, not a display list. Classify these symbols `Leaf`
    // so the transitive lifter stops at the `generate_display_list` entry
    // and never descends into the painters (~300+ fns: glyph emission,
    // gradients, borders, tables, images, …) — a large lift-surface +
    // lift-time reduction. The mangled Rust module path contains
    // lowercase `display_list`; the `Az*` C API uses camelCase
    // `DisplayList`, so framework symbols are unaffected.
    if name.contains("display_list") {
        return FnClass::Leaf;
    }
    // M12.7: azul_layout/azul_core `probe` is profiling instrumentation
    // (timing `Span`s). `Span::drop` reads a THREAD-LOCAL event buffer; the
    // lifted wasm has no real TLS, so that access mis-evaluates and calls
    // `std::thread::local::panic_access_error` (-> trap). Profiling is
    // non-essential for web layout, so stub the whole probe module to Leaf:
    // `Span::drop` becomes a no-op and the `let _p = Probe::span(..)` guard
    // (never read) is harmlessly null. Same idea as the display_list cut.
    if (name.contains("azul_layout") || name.contains("azul_core")) && name.contains("probe") {
        return FnClass::Leaf;
    }
    // M9-3: check the more specific layout4 variant FIRST — its name
    // is a superstring of `az_call_indirect`, so a naive ends_with
    // match would mis-classify it as the 3-arg variant.
    if stripped == "az_call_indirect_layout4"
        || stripped.ends_with("az_call_indirect_layout4")
    {
        return FnClass::CallIndirectLayout4;
    }
    if stripped == "az_call_indirect" || stripped.ends_with("az_call_indirect") {
        return FnClass::CallIndirect;
    }
    if stripped == "az_resolve_callback" || stripped.ends_with("az_resolve_callback") {
        return FnClass::ResolveCallback;
    }

    // 2) api.json — authoritative for Az* names.
    //
    //   • Framework → BoundaryImport (M10-D, sharded mode) OR
    //     Recursable (legacy bundled mode).
    //   • ServerEntryPoint → NeverLift.
    //   • ReplaceWithDomPatcher → Leaf (the lift pipeline never reaches
    //     them because they're stripped upstream, but classify
    //     defensively).
    //
    // M10-D rollout: while the boundary-lift pass + manifest + sharded
    // loader.js are being wired up, default to the legacy bundled mode
    // so the existing acceptance gates stay green. Set
    // `AZ_ENABLE_SHARDS=1` to opt into the new behavior. Once the
    // sharded path is end-to-end green, the polarity flips:
    // default = BoundaryImport; `AZ_BUNDLED_LEGACY=1` keeps the old
    // Recursable behavior.
    if let Some(api_class) = api.get(name) {
        return match api_class {
            ApiFnClass::Framework => {
                if shards_enabled() {
                    FnClass::BoundaryImport
                } else {
                    FnClass::Recursable
                }
            }
            ApiFnClass::ServerEntryPoint => FnClass::NeverLift,
            ApiFnClass::ReplaceWithDomPatcher => FnClass::Leaf,
        };
    }

    // 3) System / runtime prefixes → Leaf (typed extern; never lift).
    //    These mirror the original is_recursable_dep denylist.
    let system_prefixes = ["_dyld", "_dispatch", "_pthread", "_objc_"];
    for prefix in &system_prefixes {
        if name.starts_with(prefix) {
            return FnClass::Leaf;
        }
    }
    if name.contains("___rustc") || name.contains("__rust_") {
        return FnClass::Leaf;
    }
    // M12.7 (g134): taffy GRID track-sizing `resolve_intrinsic_track_sizes` is a ~67 KB monster fn
    // that intermittently HANGS remill-lift (stalls at 0% CPU with no progress — observed twice,
    // g132 + g134; g131/g132b happened to get past it). It is GRID-only — NEVER reached for text or
    // flex layouts (web-text-min, hello-world's flex button). NeverLift skips lifting it
    // (trap-if-called); since it is never called for these examples, the trap is dead. Unblocks +
    // speeds the lift. (If/when web GRID layout is needed, this must be revisited — split the fn or
    // raise the remill timeout.)
    if name.contains("resolve_intrinsic_track_sizes") {
        return FnClass::NeverLift;
    }
    // Windows/PDB: module-stream procs report DISPLAY-style paths
    // (`azul_core::refany::RefCount::can_be_shared_mut`) instead of
    // mangled link names. Publics (mangled) win dedup ties, so this
    // branch only decides LTO-internalized locals with no public.
    // Mirrors the headline _ZN rules: A7 panic family → NeverLift;
    // A1 keystone: `alloc`/`core` are RECURSABLE by default (no_std,
    // no syscalls — a Leaf stub here re-opens the silent-garbage
    // class); noisy runtime crates → Leaf; everything else (azul_*,
    // webrender_*, user crates) → Recursable.
    if !name.starts_with("_ZN") && !name.starts_with("_R") && name.contains("::") {
        let lead = name.trim_start_matches('_');
        let crate_name: String = lead
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        let is_panic_path = name.contains("panicking::")
            || name.contains("handle_alloc_error")
            || name.contains("capacity_overflow")
            || name.contains("begin_panic")
            || name.contains("rust_begin_unwind")
            || name.contains("panic_access_error")
            || name.contains("already_borrowed")
            || name.contains("unwrap_failed")
            || name.contains("expect_failed")
            || name.contains("slice_start_index_len_fail")
            || name.contains("slice_end_index_len_fail")
            || name.contains("slice_index_order_fail")
            || name.contains("panic_nounwind")
            || name.contains("panic_bounds_check")
            || name.contains("panic_fmt");
        if is_panic_path {
            return FnClass::NeverLift;
        }
        match crate_name.as_str() {
            "alloc" | "core" => {
                // Known-trap carve-out kept from the mangled branch:
                // core::ops::function::impls (FnOnce shims).
                if name.contains("ops::function::impls") {
                    return FnClass::Leaf;
                }
                return FnClass::Recursable;
            }
            "std" | "compiler_builtins" | "panic_abort" | "panic_unwind"
            | "rustc_demangle" | "backtrace" | "addr2line" | "gimli" | "object"
            | "miniz_oxide" => {
                if name.contains("hashmap_random_keys") {
                    return FnClass::HashmapRandomKeys;
                }
                return FnClass::Leaf;
            }
            _ => return FnClass::Recursable,
        }
    }
    // Itanium-mangled Rust internals: `_ZN<len><crate>...`.
    if let Some(rest) = name.strip_prefix("_ZN") {
        let mut digits = 0usize;
        for c in rest.chars() {
            if c.is_ascii_digit() {
                digits += 1;
            } else {
                break;
            }
        }
        if digits > 0 {
            if let Ok(len) = rest[..digits].parse::<usize>() {
                let name_start = digits;
                let name_end = name_start + len;
                if name_end <= rest.len() {
                    let crate_name = &rest[name_start..name_end];
                    let runtime_crates = [
                        "core",
                        "std",
                        "alloc",
                        "compiler_builtins",
                        "panic_abort",
                        "panic_unwind",
                        "rustc_demangle",
                        "backtrace",
                        "addr2line",
                        "gimli",
                        "object",
                        "miniz_oxide",
                    ];
                    if runtime_crates.iter().any(|c| *c == crate_name) {
                        // M12.5e ROOT-CAUSE FIX: `alloc::raw_vec` holds the
                        // Vec/RawVec growth logic (`grow_one`,
                        // `grow_amortized`, `finish_grow`). These are NOT
                        // leaf primitives — they compute the new capacity,
                        // call the allocator, memcpy existing elements, and
                        // write the new `ptr`/`cap` back through `&mut self`.
                        // Classifying them as `Leaf` emits a noop-with-X0=0
                        // stub, so Vecs NEVER grow: `ptr` stays dangling,
                        // `cap` stays 0, and every cascade-built StyledDom
                        // Vec (node_data, node_hierarchy, …) reads back
                        // empty. (Verified with make_test_vec_struct: cap=0,
                        // ptr=0x4 dangling, len=3.) The allocator shim
                        // itself is already `BumpAlloc` (matched above), so
                        // lifting raw_vec only pulls in its bounded callee
                        // set. Lift it.
                        // M12.5y: noreturn handlers (panic / alloc-error /
                        // capacity-overflow) are `-> !`. Native code that
                        // branches to them NEVER restores SP/callee-saved (it
                        // aborts). The default `Leaf` stub RETURNS (X0=0), so a
                        // lifted caller that reaches its error exit RETURNS with
                        // an unrestored frame, silently corrupting the caller's
                        // SP-relative locals (the apply_ua_css → create_from
                        // cache-base = NULL bug). Trap (NeverLift) so these abort
                        // loudly instead of returning a corrupt frame.
                        if name.contains("9panicking")
                            || name.contains("handle_alloc_error")
                            || name.contains("capacity_overflow")
                            || name.contains("begin_panic")
                            || name.contains("rust_begin_unwind")
                            // M12.7: more `-> !` panic helpers that were
                            // slipping through to `Leaf`. A Leaf stub RETURNS,
                            // so a lifted caller that branches to one of these
                            // (e.g. layout's RefCell borrow / thread-local /
                            // slice-index checks) falls through into the dead
                            // padding the compiler emits after a noreturn `bl`,
                            // which remill can't decode → `__remill_missing_block`
                            // → `unreachable` trap (the LayoutWindow::new trap).
                            // These all diverge, so NeverLift is correct.
                            || name.contains("panic_access_error")     // std::thread::local
                            || name.contains("already_borrowed")       // core::cell RefCell
                            || name.contains("unwrap_failed")          // Option/Result
                            || name.contains("expect_failed")
                            || name.contains("slice_index_fail")
                            || name.contains("slice_error_fail")
                            || name.contains("slice_start_index_len_fail")
                            || name.contains("slice_end_index_len_fail")
                            || name.contains("slice_index_order_fail")
                            || name.contains("slice_index_overflow_fail")
                            || name.contains("str_index_overflow_fail")
                            || name.contains("panic_cannot_unwind")
                            || name.contains("panic_nounwind")
                        {
                            return FnClass::NeverLift;
                        }
                        // MECH-B ROOT-CAUSE FIX (2026-06-12): `alloc` + `core` default to
                        // RECURSABLE, not Leaf. Both are no-syscall crates by construction
                        // (no_std): their only external edges are the allocator shims
                        // (matched to BumpAlloc/BumpRealloc/BumpDealloc by name above) and
                        // the diverging panic/error helpers (NeverLift above). Every other
                        // fn in them is pure compute that MUST run for correctness. The old
                        // Leaf default no-op'd whichever out-of-line monomorphization the
                        // compiler didn't inline, silently corrupting the caller — the
                        // exemption blocks below (raw_vec, btree, from_iter, spec_extend,
                        // resize, sort, binary_search, utf8, FnOnce, …) were each a
                        // separately-diagnosed incident of THIS one gap. Terminal case:
                        // `<[&str]>::join` → alloc::str::join_generic_copy was Leaf'd, so
                        // split_text_for_whitespace's whitespace-collapse "returned" stale
                        // stack garbage {ptr=1, len=<heap ptr>} → 170 MB phantom &str →
                        // finish_grow OOB ("mechanism B", which gated the whole web
                        // backend). A native-aarch64 harness run of the actually-lifted fn
                        // (scripts/mechb_harness/) proved the lift itself was always
                        // correct — the fn simply never ran. The blocks below are shadowed
                        // for alloc/core but kept as incident history.
                        if crate_name == "core" && name.contains("8function5impls") {
                            // Known landmine, tried + reverted before (see NOTE below):
                            // the fn-ptr blanket impls (`impl Fn for &F` etc.) lift to an
                            // `unreachable` trap that poisons the cascade. Keep the no-op
                            // stub until that trap is root-caused.
                            return FnClass::Leaf;
                        }
                        if crate_name == "alloc" || crate_name == "core" {
                            return FnClass::Recursable;
                        }
                        // COLLECT-CHAIN ROOT-CAUSE (2026-06-10): `Vec::from_iter`/`collect()` lowers
                        // to `alloc::vec::spec_from_iter*::from_iter`, `alloc::vec::in_place_collect::
                        // from_iter_in_place`, and `spec_extend`/`extend_trusted`/`extend_desugared` —
                        // ALL real-work Vec builders that the runtime-crates filter stubbed to Leaf.
                        // The no-op stub never writes the collected Vec through its sret dest, so every
                        // `.iter().map(..).collect::<Vec<_>>()` in lifted code returned stack garbage
                        // (len=0 on a clean frame; pointer-shaped values like 0x27370 on a dirty one).
                        // THIS was the real "class-B sret mis-lift": the historic shape_text garbage
                        // lens (g126/g127), the Ok→Err Result<Vec> reads (g76/g78), the corrupt
                        // FontChainKey from from_selectors (g121/g122, families.len 4-vs-3), and the
                        // css.rs From<CssPropertyWithConditionsVec> FIX-B rewrite were all this one
                        // classifier gap. Same fix class as raw_vec/btree/resize below: lift them.
                        if crate_name == "alloc"
                            && (name.contains("raw_vec")
                                || name.contains("btree")
                                || name.contains("from_iter")
                                || name.contains("in_place_collect")
                                || name.contains("spec_from_iter")
                                || name.contains("spec_extend")
                                || name.contains("extend_trusted")
                                || name.contains("extend_desugared"))
                        {
                            // raw_vec: see above. btree (M12.7): BTreeMap's drop
                            // drains the tree via `IntoIter::dying_next`, which
                            // advances the iterator + frees each node THROUGH
                            // `&mut self`. A Leaf stub is a no-op, so the drop
                            // loop `LOOP: bl dying_next; ldr x8,[iter.cur]; cbnz
                            // x8, LOOP` never advances `iter.cur` → infinite
                            // (the layout_dom_recursive solver hang: a populated
                            // BTreeMap local being dropped at the fn's return).
                            // Like raw_vec, these are NOT leaf primitives — lift
                            // the btree machinery so the recursive walk pulls in
                            // the node-free + advance logic.
                            return FnClass::Recursable;
                        }
                        // CSS-apply ROOT-CAUSE (2026-06-01): Vec::resize and the slice
                        // sorts do REAL WORK but defaulted to a no-op Leaf stub — same
                        // class as raw_vec/btree above. `Vec::resize` fills/grows per-node
                        // prop Vecs in the cascade (computed_values @prop_cache.rs:5135,
                        // styled_dom node Vecs @styled_dom.rs:1781); a Leaf no-op leaves
                        // them empty → inherited/inline CSS lost → compact height_raw stays
                        // SENTINEL → every laid-out rect collapses to 0. The slice sorts
                        // (core::slice::sort driftsort_main / insertion_sort_*) order CSS
                        // rules by specificity (sort_by_specificity); a no-op leaves them
                        // unsorted (wrong cascade order). Lift both.
                        if (crate_name == "alloc"
                            && name.contains("3vec")
                            && name.contains("6resize"))
                            || (crate_name == "core"
                                && name.contains("5slice")
                                && name.contains("4sort"))
                            // CSS-apply ROOT-CAUSE (2026-06-01, cont.): `core::slice::binary_search*`
                            // does REAL WORK (it compares + halves to find an index) but defaulted to a
                            // no-op Leaf stub returning X0=0 = `Ok(0)`. The layout font-size resolver
                            // `resolve_font_size_slow` (getters.rs:233) does
                            // `computed_values.binary_search_by_key(&FontSize, …)`: with the search a
                            // no-op the fast in-cache lookup ALWAYS "succeeds" at index 0 (wrong slot) or
                            // is bypassed, so it FALLS THROUGH to `cache.get_font_size → get_property →
                            // get_property_slow` (getters.rs:269) for EVERY box — and get_property_slow
                            // is the function remill mis-lifts (the solveLayoutReal MISSING_BLOCK trap).
                            // Lifting binary_search lets the fast path resolve FontSize from the compact
                            // cache directly and never reach get_property_slow. Same class as sort/resize.
                            || (crate_name == "core"
                                && name.contains("5slice")
                                && name.contains("binary_search"))
                        {
                            return FnClass::Recursable;
                        }
                        // WEB-LIFT TEXT ROOT-CAUSE (2026-06-03): UTF-8 conversion/validation
                        // (`String::from_utf8_lossy`, `str::from_utf8`, `run_utf8_validation`,
                        // `from_utf8_unchecked`) do REAL WORK but defaulted to a no-op Leaf stub
                        // returning garbage. `AzString::copy_from_bytes` (the C-API string ctor,
                        // css/corety.rs:260) does `String::from_utf8_lossy(raw).into_owned()`; a
                        // Leaf no-op makes the resulting String — and thus the AzString — garbage
                        // (ptr=0/len=node-addr/cap=0), so EVERY `NodeType::Text(AzString)` (the
                        // "Hello" text) loses its bytes → the intrinsic-sizing `extract_text_from_node`
                        // OOBs copying the corrupt AzString. Same class as raw_vec/resize/sort/
                        // binary_search above. Lift it (the UTF-8-validation NEON cmhi/CMHS ops are
                        // supported by the remill fork). THE last blocker for web text.
                        if (crate_name == "alloc" || crate_name == "core")
                            && name.contains("utf8")
                        {
                            return FnClass::Recursable;
                        }
                        // M12.7: closure-dispatch trampolines
                        // (FnOnce::call_once / Fn::call / FnMut::call_mut) are
                        // monomorphized to the ACTUAL closure body — they are
                        // NOT runtime-internal noops. Stubbing them Leaf
                        // (returns X0=0) means the closure NEVER RUNS, so
                        // layout's get_or_init / unwrap_or_else / iterator
                        // closures yield garbage that opt folds into the
                        // `unreachable` trap in LayoutWindow::new. Lift them so
                        // the recursive walk discovers + lifts the closure body
                        // (same reasoning as the raw_vec exemption above).
                        if crate_name == "core"
                            && (name.contains("6FnOnce9call_once")
                                || name.contains("5FnMut8call_mut")
                                || name.contains("8function2Fn4call"))
                        {
                            return FnClass::Recursable;
                        }
                        // M12.7: OnceLock/OnceCell lazy-init + thread-local lazy
                        // Storage. `get_or_init`'s slow path is `OnceLock::initialize`
                        // / `Storage::get_or_init_slow`, which CALL the init closure
                        // and store the value through `&self`. A Leaf stub no-ops, so
                        // the cell stays uninitialized → `get_or_init` returns garbage.
                        // THE M12.7 GEOMETRY BLOCKER: get_element_font_size does
                        // `resolved_font_sizes_px.get_or_init(|| compute_all_font_sizes_px())`;
                        // with initialize a no-op the cache is never populated → the
                        // getter diverges → create_node_from_dom builds nothing → empty
                        // LayoutTree → 0 positioned rects. Lift them (like the FnOnce
                        // trampolines above) so the closure runs + the value is stored.
                        if name.contains("OnceLock")
                            || name.contains("OnceCell")
                            || name.contains("once_lock")
                            || name.contains("once_cell")
                            || name.contains("OnceBox")     // OnceLock's storage: initialize() stores the value
                            || name.contains("once_box")
                            || name.contains("get_or_init")
                        {
                            return FnClass::Recursable;
                        }
                        // NOTE: classifying `core::ops::function::impls` (fn-ptr Fn::call)
                        // Recursable was tried + REVERTED — it pulled in a fn-ptr impl that
                        // lifts to an `unreachable` trap, breaking the cascade (hydrate).
                        return FnClass::Leaf;
                    }
                    // WEB FONT BOUNDARY (2026-06-01): the web backend NEVER
                    // parses or shapes fonts in-wasm. Fonts are fetched from
                    // the browser as resources (the PART 2 RequestResources
                    // design) and the `FcFontCache` is built natively,
                    // server-side, then handed to the lifted layout callback.
                    // `allsorts_azul` (glyph outlines, cmap, GSUB/GPOS shaping,
                    // woff2 / variable-font table parsing) is therefore DEAD in
                    // the lifted callback: for a box layout with no text,
                    // `font_hash_to_families` is empty, so
                    // `collect_and_resolve_font_chains_with_registration`
                    // resolves zero chains and never enters allsorts. Lifting it
                    // anyway dragged 627 transitive deps into the module (1680
                    // total → bloat + slow lift) AND its large table-parsing
                    // match/jump-table functions are a runtime-trap surface that
                    // poisons the whole wasm. Treat the entire font parser as a
                    // lift boundary (Leaf no-op stub). NOTE: CSS font-SIZE math
                    // (em/rem/% → px, `compute_all_font_sizes_px`) lives in
                    // `azul_core`, NOT allsorts, so layout geometry is unaffected.
                    // PART 2 will swap these stubs for a resource-request emitter.
                    // 2026-06-02: REMOVED the allsorts Leaf boundary so TEXT can be
                    // shaped/measured in-wasm (hello-world's label/counter measured to
                    // height 0 because allsorts was stubbed → no glyphs/metrics). The
                    // original concern was that allsorts' large table-parsing jump-table
                    // fns are a runtime-trap surface — but this session's static jump-table
                    // devirt (azul_remill.cpp exact-decode + extra_data) now resolves those
                    // jump tables, so lifting allsorts should no longer poison the wasm.
                    // Cost: ~+627 transitive deps (bigger/slower lift). Re-add the boundary
                    // (return Leaf) if allsorts traps return.
                    // if crate_name == "allsorts_azul" {
                    //     return FnClass::Leaf;
                    // }
                    // Our own crates + 3rd party crates we want to lift
                    // into (azul_*, webrender_*, serde_*) → Recursable.
                    return FnClass::Recursable;
                }
            }
        }
        return FnClass::Leaf;
    }
    if name.starts_with("_R") {
        // v0 mangling — conservatively Leaf (rare in our lift path
        // and the recursive walk into them tends to be runtime).
        return FnClass::Leaf;
    }
    // C-internal libSystem entry points (`_main`, `_malloc`, `_memcpy`).
    if let Some(rest) = name.strip_prefix('_') {
        if !rest.starts_with("Az") {
            return FnClass::Leaf;
        }
    }
    // Az-prefixed but not in api.json — bare extern. Treat as
    // Recursable so the transitive walker can pick up cb-on-cb chains.
    FnClass::Recursable
}

/// M9-review helper: derive a Mach-O image's contributing-segment
/// `(native_min, native_max)` file-vmaddr range. Used by
/// [`SymbolTable::assign_synthetic_addresses`] to bound each image's
/// rebasing band. Skips `__PAGEZERO` (which is the 4 GiB null-deref
/// hole at the start of executables).
fn macho_image_text_data_range(
    macho: &goblin::mach::MachO<'_>,
    _file_bytes: &[u8],
) -> (usize, usize) {
    use goblin::mach::load_command::CommandVariant;
    let mut min: u64 = u64::MAX;
    let mut max: u64 = 0;
    for lc in &macho.load_commands {
        if let CommandVariant::Segment64(seg64) = &lc.command {
            let segname = trim_macho_name(&seg64.segname);
            if segname == "__PAGEZERO" {
                continue;
            }
            let start = seg64.vmaddr;
            let end = start.saturating_add(seg64.vmsize);
            if end > start {
                if start < min { min = start; }
                if end > max { max = end; }
            }
        }
    }
    if min == u64::MAX {
        (0, 0)
    } else {
        (min as usize, max as usize)
    }
}

/// M9-review helper: ELF sibling of [`macho_image_text_data_range`].
/// Walks PT_LOAD program headers + reports the union of their virtual
/// address ranges.
fn elf_image_text_data_range(
    elf: &goblin::elf::Elf<'_>,
    _file_bytes: &[u8],
) -> (usize, usize) {
    let mut min: u64 = u64::MAX;
    let mut max: u64 = 0;
    for ph in &elf.program_headers {
        if ph.p_type != goblin::elf::program_header::PT_LOAD || ph.p_memsz == 0 {
            continue;
        }
        let start = ph.p_vaddr;
        let end = start.saturating_add(ph.p_memsz);
        if start < min { min = start; }
        if end > max { max = end; }
    }
    if min == u64::MAX {
        (0, 0)
    } else {
        (min as usize, max as usize)
    }
}

/// M9-3b helper: enumerate Mach-O sections whose file-vmaddr + slide
/// + size falls within `wasm_offset_limit` once truncated to 32 bits.
/// Returns `(file_vmaddr, size)` pairs — the caller adds `slide` and
/// the truncation.
///
/// Targets: `__TEXT.__cstring`, `__TEXT.__const`, `__DATA.__data`,
/// `__DATA.__const`. Each is data the user-binary's lifted code may
/// read via `adrp + ldr/add` (string literals, const tables, fixed
/// initializers).
pub(crate) fn collect_macho_low32_sections(
    macho: &goblin::mach::MachO<'_>,
    file_bytes: &[u8],
    slide: usize,
    wasm_offset_limit: u32,
) -> Vec<(u64, u64)> {
    use goblin::mach::load_command::{
        CommandVariant, SIZEOF_SECTION_64, SIZEOF_SEGMENT_COMMAND_64,
    };
    let mut out = Vec::new();
    let want = |segname: &str, sectname: &str| -> bool {
        matches!(
            (segname, sectname),
            ("__TEXT", "__cstring")
                | ("__TEXT", "__const")
                | ("__DATA", "__data")
                | ("__DATA", "__const")
                | ("__DATA_CONST", "__const")
        )
    };
    for lc in &macho.load_commands {
        let CommandVariant::Segment64(seg64) = &lc.command else { continue };
        let segname = trim_macho_name(&seg64.segname);
        let sections_off = lc.offset + SIZEOF_SEGMENT_COMMAND_64;
        for i in 0..seg64.nsects as usize {
            let off = sections_off + i * SIZEOF_SECTION_64;
            if off + SIZEOF_SECTION_64 > file_bytes.len() {
                break;
            }
            let Some(s) = parse_section64(&file_bytes[off..off + SIZEOF_SECTION_64]) else {
                continue;
            };
            let sectname = trim_macho_name(&s.sectname);
            if !want(segname, sectname) {
                continue;
            }
            let live = (s.addr as usize).wrapping_add(slide);
            let truncated = (live as u64) & 0xFFFF_FFFF;
            if truncated == 0 || truncated.saturating_add(s.size) > wasm_offset_limit as u64 {
                continue;
            }
            out.push((s.addr, s.size));
        }
    }
    out
}

/// [WEB-LIFT FIX 2026-06-11] macOS thread-local geometry of one image:
/// `__DATA.__thread_vars` (the 24-byte `{thunk, key, offset}` descriptor
/// array) + `__DATA.__thread_data` (the TLS initial image; descriptor
/// offsets are relative to its start, with `__thread_bss` contiguous
/// after it — bss needs no mirror bytes, wasm memory zero-default IS its
/// init). Lifted code reaches a thread-local via `adrp x0,<descriptor>;
/// ldr x8,[x0]; blr x8` — the thunk is `__tlv_bootstrap`/`tlv_get_addr`
/// (out-of-image libdyld), so the lifted `blr` used to fall into the
/// dispatcher's unknown-target drop, leaving X0 = the DESCRIPTOR address;
/// std then read the descriptor bytes as the TLS variable → garbage
/// `LocalKey` state byte → `panic_access_error` traps (first seen:
/// `RandomState::new`'s KEYS thread_local inside `HashSet::new()` in
/// `get_loaded_font_ids`, post-rebase). The fix consumes this geometry in
/// `transpiler_remill.rs`: mirror vars+data, rewrite each mirrored thunk
/// to `AZ_TLV_MAGIC_PC`, dispatcher-resolve that PC to
/// `tls_base + descriptor.offset` (single-threaded wasm ⇒ TLS = statics).
fn collect_macho_tlv_regions(
    macho: &goblin::mach::MachO<'_>,
    file_bytes: &[u8],
    slide: usize,
    path: &std::path::Path,
) -> Vec<TlvRegion> {
    use goblin::mach::load_command::{
        CommandVariant, SIZEOF_SECTION_64, SIZEOF_SEGMENT_COMMAND_64,
    };
    // Only the image whose code gets LIFTED matters — its descriptors are
    // what lifted `adrp+ldr+blr` reaches. Collecting every loaded image's
    // TLS would make `tlv_regions.first()` (the dispatcher's TLS base)
    // ambiguous — first seen as another dylib's region shadowing libazul's.
    if !path
        .file_name()
        .and_then(|f| f.to_str())
        .map(|f| f.contains("libazul"))
        .unwrap_or(false)
    {
        return Vec::new();
    }
    let mut vars: Option<(usize, usize)> = None;
    let mut data: Option<(usize, usize)> = None;
    for lc in &macho.load_commands {
        let CommandVariant::Segment64(seg64) = &lc.command else { continue };
        if trim_macho_name(&seg64.segname) != "__DATA" {
            continue;
        }
        let sections_off = lc.offset + SIZEOF_SEGMENT_COMMAND_64;
        for i in 0..seg64.nsects as usize {
            let off = sections_off + i * SIZEOF_SECTION_64;
            if off + SIZEOF_SECTION_64 > file_bytes.len() {
                break;
            }
            let Some(s) = parse_section64(&file_bytes[off..off + SIZEOF_SECTION_64]) else {
                continue;
            };
            let live = (s.addr as usize).wrapping_add(slide);
            match trim_macho_name(&s.sectname) {
                "__thread_vars" => vars = Some((live, s.size as usize)),
                "__thread_data" => data = Some((live, s.size as usize)),
                _ => {}
            }
        }
    }
    match (vars, data) {
        (Some((vs, vsz)), Some((ds, dsz))) if vsz > 0 => {
            eprintln!(
                "[azul-web] TLV: {} __thread_vars live=0x{:x}+0x{:x} __thread_data live=0x{:x}+0x{:x}",
                path.file_name().and_then(|f| f.to_str()).unwrap_or("?"),
                vs, vsz, ds, dsz,
            );
            vec![TlvRegion {
                vars_start: vs,
                vars_size: vsz,
                tls_base: ds,
                tls_data_size: dsz,
            }]
        }
        // Descriptors with no __thread_data (all-bss TLS): offsets are
        // still relative to where __thread_data WOULD start = the bss
        // start; without a data section there is nothing to anchor, so
        // skip (no such image in practice — libazul has both).
        _ => Vec::new(),
    }
}

/// [WEB-LIFT FIX 2026-06-06] Find hashbrown's `EMPTY_GROUP` static(s) in the
/// loaded `libazul` image, returned as NATIVE `(addr, len)` ranges to mirror.
///
/// hashbrown encodes an EMPTY control byte as `0xFF`; an empty `RawTable`'s
/// `ctrl` points at a `Group::WIDTH`-byte all-`0xFF` static (`EMPTY_GROUP`),
/// `Group::WIDTH == 8` for the SWAR group used on this aarch64 build (16 with
/// NEON). `Group::static_empty()` materializes its address via `adrp + add`.
/// When the empty table is built inside a function reached only INDIRECTLY
/// (not in a callback's static BL/B dep-walk), [`scan_arm64_adrp_pages`] never
/// sees that `adrp`, so the page is never mirrored → reads back as `0x00` → an
/// all-zero control group looks ALL-FULL → `RawIterRange::fold_impl` loops
/// forever → text shaping HANGS.
///
/// To mirror it regardless of how the table is reached, scan the WHOLE libazul
/// `__text` for `adrp Xd,#pg ; add Xd,Xd,#off` (within a few instructions)
/// whose materialized, 8-byte-aligned target begins an all-`0xFF` run of >= 8
/// bytes, and return each run as a `(native_addr, run_len)` range. The caller
/// mirrors only those bytes (a precise range, never the whole 4 KiB page):
/// whole-page mirroring of arbitrary const pages re-translates coincidental
/// pointer-shaped windows and traps OOB, whereas an all-`0xFF` run is never a
/// valid in-image native pointer and survives pointer-translation unchanged.
/// Signature-based, so it tracks `EMPTY_GROUP` wherever a rebuild moves it.
pub(crate) fn find_hashbrown_empty_group_ranges() -> &'static [(usize, usize)] {
    static RANGES: std::sync::OnceLock<Vec<(usize, usize)>> = std::sync::OnceLock::new();
    RANGES.get_or_init(compute_hashbrown_empty_group_ranges).as_slice()
}

/// [WEB-LIFT FIX 2026-06-25] Format-general signature scan of a mapped, read-only
/// const section for hashbrown's `EMPTY_GROUP`. On x86_64 hashbrown uses the
/// 16-byte SSE2 `Group`, so `EMPTY_GROUP == [0xFF; 16]` (16-aligned). It is reached
/// ONLY via the empty `RawTable`'s rebased `ctrl` data-pointer — never a direct
/// code reference — so the riprel/adrp page-mirror structurally misses it; un-
/// mirrored it reads back as `0x00`, an empty control group then looks "all-FULL"
/// (FULL has the high bit clear), `match_empty` never fires, and the probe loops
/// forever → the layout solve HANGS. Mirroring bytes that ARE `0xFF` in the native
/// image is always sound (a `0xFFFF…FF` window is never a translatable pointer), so
/// over-matching incidental `i128 -1` constants is harmless. Each run is capped at
/// 256 B and deduped via `seen`. Mirrors the Mach-O `__const` scan below.
fn scan_const_runs_for_empty_group(
    sec_lo: usize,
    sec_size: usize,
    img_lo: usize,
    img_hi: usize,
    seen: &mut std::collections::HashSet<usize>,
    out: &mut Vec<(usize, usize)>,
) {
    if sec_size == 0 || sec_lo < img_lo || sec_lo.wrapping_add(sec_size) > img_hi {
        return;
    }
    // SAFETY: `.rdata`/`.rodata` is a mapped, read-only image section for the
    // process lifetime; reading `sec_size` contiguous bytes is sound.
    let blob = unsafe { core::slice::from_raw_parts(sec_lo as *const u8, sec_size) };
    let mut i = 0usize;
    while i < sec_size {
        if blob[i] != 0xFF {
            i += 1;
            continue;
        }
        let start = i;
        while i < sec_size && blob[i] == 0xFF {
            i += 1;
        }
        let run_len = i - start;
        let target = sec_lo + start;
        // >= 16 contiguous 0xFF, 8-aligned (16-aligned EMPTY_GROUP ⊂ 8-aligned).
        if run_len >= 16 && (target & 0x7) == 0 && seen.insert(target) {
            out.push((target, core::cmp::min(run_len, 256)));
        }
    }
}

fn compute_hashbrown_empty_group_ranges() -> Vec<(usize, usize)> {
    use goblin::mach::load_command::{
        CommandVariant, SIZEOF_SECTION_64, SIZEOF_SEGMENT_COMMAND_64,
    };
    let images = match enumerate_loaded_images() {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    let mut out: Vec<(usize, usize)> = Vec::new();
    for img in &images {
        // Match the azul library image regardless of platform naming:
        // macOS `libazul.dylib`, Linux `libazul.so`, Windows `azul.dll`.
        let pl = img.path.to_string_lossy().to_lowercase();
        if img.bytes.is_empty() || !pl.contains("azul") {
            continue;
        }
        // [WEB-LIFT FIX 2026-06-25] x86_64 PE (Windows) / ELF (Linux) port. The
        // Mach-O/aarch64 path below no-ops on these formats, so EMPTY_GROUP was
        // never force-mirrored → the lifted empty-map probe spun forever (the
        // SwissTable/hashbrown hang). Signature-scan the read-only const section(s)
        // for >=16-byte all-0xFF runs (x86_64 SSE2 Group::WIDTH == 16) and return
        // them; the unconditional caller appends each as a precise 0xFF segment.
        match goblin::Object::parse(&img.bytes) {
            Ok(goblin::Object::PE(pe)) => {
                let image_base = pe.image_base as usize;
                let (lo, hi) = pe_image_text_data_range(&pe);
                let (img_lo, img_hi) = (lo.wrapping_add(img.slide), hi.wrapping_add(img.slide));
                let mut seen = std::collections::HashSet::new();
                for s in &pe.sections {
                    let name = s.name().unwrap_or("");
                    if name == ".rdata" || name == ".rodata" {
                        let sec_lo = image_base + s.virtual_address as usize + img.slide;
                        scan_const_runs_for_empty_group(
                            sec_lo, s.virtual_size as usize, img_lo, img_hi, &mut seen, &mut out,
                        );
                    }
                }
                continue;
            }
            Ok(goblin::Object::Elf(elf)) => {
                const SHF_ALLOC: u64 = 0x2;
                const PT_LOAD: u32 = 1;
                // Image bounds from PT_LOAD segments (slid).
                let (mut img_lo, mut img_hi) = (usize::MAX, 0usize);
                for ph in &elf.program_headers {
                    if ph.p_type == PT_LOAD {
                        let lo = (ph.p_vaddr as usize).wrapping_add(img.slide);
                        img_lo = img_lo.min(lo);
                        img_hi = img_hi.max(lo + ph.p_memsz as usize);
                    }
                }
                if img_lo == usize::MAX {
                    continue;
                }
                let mut seen = std::collections::HashSet::new();
                for sh in &elf.section_headers {
                    if (sh.sh_flags & SHF_ALLOC) == 0 || sh.sh_size == 0 {
                        continue;
                    }
                    let name = elf.shdr_strtab.get_at(sh.sh_name).unwrap_or("");
                    if name == ".rodata" || name == ".data.rel.ro" {
                        let sec_lo = (sh.sh_addr as usize).wrapping_add(img.slide);
                        scan_const_runs_for_empty_group(
                            sec_lo, sh.sh_size as usize, img_lo, img_hi, &mut seen, &mut out,
                        );
                    }
                }
                continue;
            }
            // Mach-O (or unknown) → fall through to the aarch64/Mach-O path below
            // (re-parses once; one-time init cost, keeps the working path untouched).
            _ => {}
        }
        let macho = match goblin::Object::parse(&img.bytes) {
            Ok(goblin::Object::Mach(goblin::mach::Mach::Binary(m))) => m,
            Ok(goblin::Object::Mach(goblin::mach::Mach::Fat(f))) => {
                match pick_fat_slice(&f, &img.bytes) {
                    Ok(Some(m)) => m,
                    _ => continue,
                }
            }
            _ => continue,
        };
        let slide = img.slide;
        let (minv, maxv) = macho_image_text_data_range(&macho, &img.bytes);
        if minv == 0 && maxv == 0 {
            continue;
        }
        let img_lo = minv.wrapping_add(slide);
        let img_hi = maxv.wrapping_add(slide);
        // Locate __TEXT.__text (native range, mapped r-x → readable).
        let mut text: Option<(usize, usize)> = None; // (native_lo, size)
        // [g211] Also locate const DATA sections (`__const` in any segment) so we can
        // signature-scan them for hashbrown's EMPTY_GROUP. EMPTY_GROUP is reached only
        // via the empty-table singleton's REBASED `ctrl` data-pointer — never a direct
        // `adrp`/`add` in code — so the instruction scan below structurally misses it,
        // and an empty map's ctrl-scan then reads 0x00 → "all-FULL" → RawIterRange loops
        // forever → text shaping hangs.
        let mut const_secs: Vec<(usize, usize)> = Vec::new(); // (native_lo, size)
        for lc in &macho.load_commands {
            let CommandVariant::Segment64(seg64) = &lc.command else { continue };
            let sections_off = lc.offset + SIZEOF_SEGMENT_COMMAND_64;
            for i in 0..seg64.nsects as usize {
                let so = sections_off + i * SIZEOF_SECTION_64;
                if so + SIZEOF_SECTION_64 > img.bytes.len() {
                    break;
                }
                let Some(s) = parse_section64(&img.bytes[so..so + SIZEOF_SECTION_64]) else {
                    continue;
                };
                let nm = trim_macho_name(&s.sectname);
                if nm == "__text" {
                    text = Some(((s.addr as usize).wrapping_add(slide), s.size as usize));
                } else if nm == "__const" && s.size > 0 {
                    const_secs.push(((s.addr as usize).wrapping_add(slide), s.size as usize));
                }
            }
        }
        let Some((text_lo, tsize)) = text else { continue };
        if tsize < 8 || text_lo < img_lo || text_lo + tsize > img_hi {
            continue;
        }
        // PIC `adrp`/`add` immediates are not rebased, so the mapped bytes match
        // the file; reading the contiguous, mapped __text section is safe.
        let code = unsafe { core::slice::from_raw_parts(text_lo as *const u8, tsize) };
        let rd32 = |i: usize| -> u32 {
            u32::from_le_bytes([code[i], code[i + 1], code[i + 2], code[i + 3]])
        };
        let mut seen: std::collections::HashSet<usize> = std::collections::HashSet::new();
        let mut o = 0usize;
        while o + 4 <= tsize {
            let instr = rd32(o);
            // ADRP: bit31==1, bits28..24 == 0b10000.
            if (instr >> 31) == 1 && ((instr >> 24) & 0x1F) == 0x10 {
                let rd = instr & 0x1F;
                let immlo = (instr >> 29) & 0x3;
                let immhi = (instr >> 5) & 0x7FFFF;
                let imm21 = ((immhi << 2) | immlo) as i64;
                let signed = if imm21 & (1 << 20) != 0 {
                    imm21 | !0x1F_FFFF
                } else {
                    imm21
                };
                let pc = text_lo + o;
                let pg = (((pc & !0xFFF) as i64).wrapping_add(signed << 12)) as usize;
                // Look ahead a few instructions for `add Xd, Xd, #imm12` (sh=0).
                let mut k = 1usize;
                while k <= 5 {
                    let o2 = o + k * 4;
                    if o2 + 4 > tsize {
                        break;
                    }
                    let i2 = rd32(o2);
                    // ADD (immediate) 64-bit, S=0, op=0: (i2 & 0xFF80_0000) == 0x9100_0000.
                    if (i2 & 0xFF80_0000) == 0x9100_0000
                        && ((i2 >> 5) & 0x1F) == rd
                        && (i2 & 0x1F) == rd
                        && ((i2 >> 22) & 1) == 0
                    {
                        let imm12 = ((i2 >> 10) & 0xFFF) as usize;
                        let target = pg.wrapping_add(imm12);
                        if (target & 0x7) == 0 && target >= img_lo && target + 8 <= img_hi {
                            let max_rl = core::cmp::min(64, img_hi - target);
                            let tb = unsafe {
                                core::slice::from_raw_parts(target as *const u8, max_rl)
                            };
                            let mut rl = 0usize;
                            while rl < max_rl && tb[rl] == 0xFF {
                                rl += 1;
                            }
                            if rl >= 8 && seen.insert(target) {
                                out.push((target, rl));
                            }
                        }
                        break;
                    }
                    k += 1;
                }
            }
            o += 4;
        }
        // [g211/g213] Signature-scan const DATA for hashbrown's EMPTY_GROUP (the empty-map
        // ctrl singleton). It is reached ONLY via the empty-table's REBASED `ctrl` data-
        // pointer — never a direct `adrp`/`add` in code — so the instruction scan above
        // structurally misses it; un-mirrored, the empty map's ctrl-scan then reads 0x00 →
        // looks "all-FULL" → the find probe loops forever → text shaping hangs. Mirroring
        // 0xFF bytes that ARE 0xFF in the native image is always correct (over-matching a
        // few incidental runs is harmless), each run capped at 256 B, deduped via `seen`
        // against the code scan above. See the run-length note below for WIDTH=8 vs 16.
        for &(sec_lo, sec_size) in &const_secs {
            if sec_lo < img_lo || sec_lo.wrapping_add(sec_size) > img_hi {
                continue;
            }
            // SAFETY: __const lives in a mapped, read-only image segment for the process
            // lifetime; reading `sec_size` contiguous bytes is sound.
            let blob = unsafe { core::slice::from_raw_parts(sec_lo as *const u8, sec_size) };
            let mut i = 0usize;
            while i < sec_size {
                if blob[i] != 0xFF {
                    i += 1;
                    continue;
                }
                let start = i;
                while i < sec_size && blob[i] == 0xFF {
                    i += 1;
                }
                let run_len = i - start;
                let target = sec_lo + start;
                // >= 8 contiguous 0xFF, 8-aligned. The PORTABLE hashbrown Group is
                // WIDTH = size_of::<usize>() = 8 on this build (NOT the 16-byte NEON Group),
                // so its `static_empty()` is `[0xFF; 8]` — an 8-byte, 8-aligned (NOT
                // 16-aligned) run. g212 AZ_READ_TRACE PROVED the empty-map ctrl-scan reads
                // exactly such a run (synth 0x41c0088 → libazul __TEXT.__const file 0x40b0088
                // = 0xFF×8); an earlier `>= 16` filter missed it and the probe read 0x00 →
                // looked all-FULL → spun. Mirroring 8-byte runs also matches incidental i64
                // `-1` constants, but that is harmless: every byte we write IS 0xFF in the
                // native image, so it can never flip a value the lift needs (a 0xFFFF_FFFF_
                // FFFF_FFFF value is never a translatable pointer). ~1.6k runs / ~16 KiB.
                if run_len >= 8 && (target & 0x7) == 0 && seen.insert(target) {
                    out.push((target, core::cmp::min(run_len, 256)));
                }
            }
        }
    }
    out
}

/// M9-3b helper: ELF sibling of [`collect_macho_low32_sections`].
/// Targets `.rodata`, `.data`, `.data.rel.ro`. Same filter rule:
/// only sections whose runtime address truncated to 32 bits fits in
/// `wasm_offset_limit`.
pub(crate) fn collect_elf_low32_sections(
    elf: &goblin::elf::Elf<'_>,
    file_bytes: &[u8],
    slide: usize,
    wasm_offset_limit: u32,
) -> Vec<(u64, u64)> {
    let _ = file_bytes;
    let mut out = Vec::new();
    for sh in &elf.section_headers {
        let Some(name) = elf.shdr_strtab.get_at(sh.sh_name) else { continue };
        if !matches!(name, ".rodata" | ".data" | ".data.rel.ro") {
            continue;
        }
        let live = (sh.sh_addr as usize).wrapping_add(slide);
        let truncated = (live as u64) & 0xFFFF_FFFF;
        if truncated == 0 || truncated.saturating_add(sh.sh_size) > wasm_offset_limit as u64 {
            continue;
        }
        out.push((sh.sh_addr, sh.sh_size));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_underscore_handles_mangled_names() {
        assert_eq!(strip_leading_underscore("_AzApp_create"), "AzApp_create");
        assert_eq!(strip_leading_underscore("__rust_alloc"), "_rust_alloc");
        assert_eq!(strip_leading_underscore("foo"), "foo");
    }

    #[test]
    fn classify_bump_alloc_variants() {
        let api: HashMap<String, ApiFnClass> = HashMap::new();
        for n in &["__rust_alloc", "___rust_alloc", "_rust_alloc", "rust_alloc"] {
            assert_eq!(
                classify_for_name(n, &api),
                FnClass::BumpAlloc,
                "expected BumpAlloc for {}",
                n
            );
        }
        for n in &["__rust_alloc_zeroed", "___rust_alloc_zeroed"] {
            assert_eq!(
                classify_for_name(n, &api),
                FnClass::BumpAlloc,
                "expected BumpAlloc for {}",
                n
            );
        }
    }

    #[test]
    fn classify_az_via_api() {
        let mut api: HashMap<String, ApiFnClass> = HashMap::new();
        api.insert("AzApp_create".to_string(), ApiFnClass::Framework);
        api.insert("AzApp_run".to_string(), ApiFnClass::ServerEntryPoint);
        assert_eq!(classify_for_name("AzApp_create", &api), FnClass::Recursable);
        assert_eq!(classify_for_name("AzApp_run", &api), FnClass::NeverLift);
        // Bare Az* not in api.json → Recursable (transitively reachable
        // user-bound cb names land here).
        assert_eq!(classify_for_name("AzMyHelper", &api), FnClass::Recursable);
    }

    #[test]
    fn classify_runtime_crates() {
        let api: HashMap<String, ApiFnClass> = HashMap::new();
        // Diverging panic helpers must TRAP, not return (M12.5y).
        assert_eq!(
            classify_for_name("_ZN4core9panicking5panicE", &api),
            FnClass::NeverLift
        );
        // MECH-B regression: out-of-line alloc/core monomorphizations are
        // real compute and must be lifted, never no-op stubbed (2026-06-12).
        assert_eq!(
            classify_for_name(
                "_ZN5alloc3str17join_generic_copy17h9c9d2f7abfe94f50E",
                &api
            ),
            FnClass::Recursable
        );
        // std stays Leaf-by-default (syscall surface).
        assert_eq!(
            classify_for_name("_ZN3std2io5stdio6_printE", &api),
            FnClass::Leaf
        );
        // Known landmine stays stubbed: core fn-ptr blanket impls.
        assert_eq!(
            classify_for_name(
                "_ZN4core3ops8function5impls5whatever17h00E",
                &api
            ),
            FnClass::Leaf
        );
    }

    #[test]
    fn classify_own_crate_as_recursable() {
        let api: HashMap<String, ApiFnClass> = HashMap::new();
        // _ZN9azul_core3dom3DomE
        assert_eq!(
            classify_for_name("_ZN9azul_core3dom3DomE", &api),
            FnClass::Recursable
        );
    }
}
