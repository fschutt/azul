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
                _ => {
                    // PE / unknown — fall through. The web backend is
                    // macOS / Linux only today; Windows host support
                    // would add an `ingest_pe` here.
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
                    | FnClass::NeverLift
                    // LibcMemcpy is *always* out-of-image (it IS a
                    // libsystem symbol) — but it has a synthetic
                    // `@llvm.memmove` body, so don't downgrade it to a
                    // no-op Leaf here.
                    | FnClass::LibcMemcpy
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

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
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
        let classification = classify_for_name(&canonical_name, api);
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
                        {
                            return FnClass::NeverLift;
                        }
                        if crate_name == "alloc" && name.contains("raw_vec") {
                            return FnClass::Recursable;
                        }
                        return FnClass::Leaf;
                    }
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
    fn classify_runtime_crates_as_leaf() {
        let api: HashMap<String, ApiFnClass> = HashMap::new();
        // _ZN4core9panicking5panicE
        assert_eq!(
            classify_for_name("_ZN4core9panicking5panicE", &api),
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
