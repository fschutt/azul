# Startup Latency Elimination Plan

## Problem Statement

Azul's `App::create()` blocks the main thread for **~700ms** scanning and parsing ~1000
system fonts via `FcFontCache::build()` before any window can be shown. Additionally,
WebRender compiles **all** shaders synchronously (`ShaderPrecacheFlags::FULL_COMPILE`)
on the first window creation, adding another ~100-200ms. Combined, the user waits nearly
a full second before seeing anything on screen.

**Goal:** Zero flash of unstyled/wrong-font content. The first frame the user sees must
use the **correct** fonts. Font discovery runs in background threads with a priority queue,
but the main thread **blocks at layout time** until the specific fonts needed for that
frame are loaded. Shader compilation is cached to disk; optionally pre-compiled via a
headless helper so the very first startup can use CPU rendering instead.

### Design Principles

1. **No FOUC.** The first rendered frame MUST use the correct fonts. We never render
   with a "temporary default" and swap later. Instead, we block briefly at layout time
   until the Builder has loaded the specific fonts the StyledDom needs.
2. **Stale cache is better than no cache.** If an on-disk font cache exists (even if
   potentially stale), load it immediately and use it. Verify staleness in background.
   Fonts don't change often — a stale cache is correct 99.9% of the time.
3. **Progressive background work.** Font scanning and parsing happen in background
   threads from the moment `App::create()` is called. By the time the main thread
   reaches layout (after window creation, DOM construction, etc.), most commonly-needed
   fonts are already loaded.
4. **Shaders are simpler.** No background threads for shaders — just cache compiled
   programs to disk. First-ever startup pays the compile cost once. Optionally,
   developers can pre-compile shaders via a headless helper function.

---

## Current Architecture (What's Slow)

### App Startup Sequence (today)

```
Main Thread:
  App::create()
    ├── FcFontCache::build()           ← 700ms BLOCKING (scans all system fonts)
    │     ├── enumerate font dirs       (~5ms)
    │     ├── for each .ttf/.otf:       (~0.7ms × 1000 fonts)
    │     │     ├── mmap file
    │     │     ├── parse HEAD, OS/2, POST, NAME tables
    │     │     ├── decode unicode ranges from OS/2 bitfield
    │     │     ├── verify_unicode_ranges_with_cmap()  ← EXPENSIVE per font
    │     │     │     (loads CMAP, tests ~50 codepoints per range)
    │     │     ├── fallback: analyze_cmap_coverage()  ← even more expensive
    │     │     └── build token index
    │     └── return FcFontCache
    │
    ├── app.run()
    │     └── create_window()
    │           ├── create_webrender_instance()
    │           │     └── Shaders::new()               ← ~100-200ms (compiles ALL shaders)
    │           ├── LayoutWindow::new()
    │           │     └── FontManager::new(fc_cache)
    │           └── first layout pass
    │                 ├── collect_and_resolve_font_chains()
    │                 ├── load_fonts_from_disk()         (loads ~2-5 fonts actually needed)
    │                 └── solve_layout()
    └── event loop
```

**Key bottlenecks:**

| Phase | Time | Location | Problem |
|-------|------|----------|---------|
| `FcFontCache::build()` | ~700ms | `dll/src/desktop/app.rs:150` | Scans + deep-parses ALL 1000 fonts |
| `verify_unicode_ranges_with_cmap()` | ~400ms of the 700ms | `rust-fontconfig/src/lib.rs:3636` | Per-font CMAP glyph verification |
| `Shaders::new()` | ~100-200ms | `dll/src/desktop/shell2/macos/mod.rs:2593` | Compiles ~60 WebRender shaders |
| First layout only uses ~2-5 fonts | 0ms (wasted) | `layout/src/solver3/paged_layout.rs:143` | 995+ fonts parsed for nothing |

### What `FcFontCache::build()` Actually Does

1. **Enumerate directories** — OS-specific (`~/Library/Fonts`, `/System/Library/Fonts`, etc.)
2. **Recursively scan** — Collect all `.ttf`, `.otf`, `.ttc`, `.woff`, `.woff2` paths
3. **For each file** — mmap, parse TTC header, then for each font index:
   - Parse `HEAD` table (units_per_em, flags)
   - Parse `POST` table (is_fixed_pitch)
   - Parse `OS/2` table (weight, width, fsSelection, ulUnicodeRange1-4)
   - **Decode 128-bit unicode range bitfield** → `Vec<UnicodeRange>`
   - **`verify_unicode_ranges_with_cmap()`** — For each claimed range, test representative
     codepoints against the actual CMAP table. Requires loading/parsing CMAP subtables and
     doing glyph ID lookups. This is ~60% of total scan time.
   - **`analyze_cmap_coverage()`** (fallback) — If OS/2 has no range data, iterate all
     ~50 unicode blocks testing codepoints. Even more expensive.
   - Parse `NAME` table (font name, family, metadata — 17 string fields)
   - Detect monospace (PANOSE fallback → HMTX advance width comparison)
4. **Build indices** — `patterns`, `metadata`, `token_index`, `font_tokens` maps

### What the GUI Actually Needs at Startup

For the **first frame**, the layout engine calls `collect_and_resolve_font_chains()` which
resolves the CSS `font-family` stacks from the DOM. A typical "Hello World" app needs:

- The system sans-serif font (1 font)
- Maybe 1-2 fallback fonts for the chain
- Material Icons (bundled, already in memory)

That's **3-5 fonts** out of 1000. The other 995 are parsed speculatively.

---

## Proposed Architecture: Block-at-Layout with Background Priority Queue

### Overview

Replace the monolithic `FcFontCache::build()` with a concurrent system where background
threads race to load fonts ahead of when the main thread needs them, but the main thread
**blocks at layout time** (not at `App::create()`) until the specific fonts it needs are
ready. This guarantees the first frame always uses the correct fonts — no FOUC.

```
                                    ┌─────────────────────────┐
                                    │   On-Disk Cache         │
                                    │   (~/.cache/azul/...)   │
                                    │                         │
                                    │  fonts/manifest.bin     │
                                    │  shaders/programs/      │
                                    └────────┬────────────────┘
                                             │ read/write
          ┌──────────────────────────────────┼──────────────────────────┐
          │                                  │                          │
    ┌─────▼──────┐   paths    ┌──────────────▼───────┐   done    ┌─────▼──────┐
    │   Scout    │──────────→│   Builder Pool        │─────────→│  Registry  │
    │  (Thread)  │            │  (N worker threads)   │          │  (Shared)  │
    │            │            │                       │          │            │
    │ enumerate  │  priority  │ parse font tables     │          │ RwLock     │
    │ dirs only  │  boost     │ check disk cache      │  BLOCK   │ BTreeMaps  │
    │ guess name │◄───────────│ skip CMAP verify      │◄────────│            │
    └────────────┘  from main │ write to registry     │  until   │            │
                              └───────────────────────┘  done    └─────▲──────┘
                                                                       │
                                                        request_fonts()│ blocks
                                                                 ┌─────┴──────┐
                                                                 │ Main Thread│
                                                                 │            │
                                                                 │ App:create │
                                                                 │   (spawn)  │
                                                                 │ window ops │
                                                                 │ DOM build  │
                                                                 │ layout ←── BLOCK here
                                                                 │ render     │
                                                                 └────────────┘
```

**Key insight:** There is a natural "gap" between `App::create()` and the first layout
pass. During this gap (window creation, DOM construction, style resolution — typically
20-100ms), the background threads are already racing to load fonts. By the time the
main thread reaches layout, the common fonts (serif, sans-serif, monospace) are very
likely already loaded. The block at layout only waits for stragglers.

### The Three Components

#### 1. The Registry (`FcFontRegistry`) — Shared State

A thread-safe, incrementally-populated replacement for `FcFontCache`.

```
FcFontRegistry {
    // === Populated by Scout (fast, Phase 1) ===
    known_paths: RwLock<BTreeMap<String, Vec<PathBuf>>>,
    //  key = guessed family name (lowercase), value = file paths
    //  e.g. "arial" → ["/System/Library/Fonts/Arial.ttf",
    //                   "/System/Library/Fonts/Arial Bold.ttf"]

    // === Populated by Builder (incremental, Phase 2+) ===
    patterns:    RwLock<BTreeMap<FcPattern, FontId>>,
    disk_fonts:  RwLock<BTreeMap<FontId, FcFontPath>>,
    metadata:    RwLock<BTreeMap<FontId, FcPattern>>,
    token_index: RwLock<BTreeMap<String, BTreeSet<FontId>>>,
    font_tokens: RwLock<BTreeMap<FontId, Vec<String>>>,

    // === In-memory fonts (bundled, embedded) ===
    memory_fonts: RwLock<BTreeMap<FontId, FcFont>>,

    // === Chain cache (computed lazily) ===
    chain_cache: Mutex<HashMap<FontChainCacheKey, FontFallbackChain>>,

    // === Priority queue for Builder ===
    build_queue: Mutex<PriorityQueue<FcBuildJob>>,
    queue_condvar: Condvar,                     // wake Builder threads

    // === Completion tracking ===
    pending_requests: Mutex<Vec<FontRequest>>,   // main thread waiting on these
    request_complete: Condvar,                   // signal main thread

    // === Status ===
    scan_complete: AtomicBool,
    build_complete: AtomicBool,
    fonts_loaded: AtomicUsize,
    fonts_total: AtomicUsize,
}
```

#### 2. The Scout (Background Thread — Phase 1)

Lightweight filesystem-only enumeration. **No file parsing.** Takes ~5-20ms.

**What it does:**
1. Enumerate all font directories (OS-specific, same logic as today)
2. For each file with a font extension (`.ttf`, `.otf`, `.ttc`, `.woff`, `.woff2`):
   - Record the path
   - **Guess the family name** from the filename:
     - `"ArialBold.ttf"` → `"arial"` (strip style suffixes, lowercase)
     - `"NotoSansJP-Regular.otf"` → `"notosansjp"` (strip weight/style suffixes)
   - Insert into `known_paths`
3. Feed all paths to the Builder's priority queue:
   - **High** priority for common families (hardcoded list: serif, sans-serif, monospace
     defaults per OS)
   - **Low** priority for everything else
4. Set `scan_complete = true`

**Why guess from filenames?** So that when the GUI requests `font-family: "Arial"` before
the Builder has parsed Arial, we can look up `known_paths["arial"]` and **promote** those
specific files to Critical priority. The guess doesn't need to be perfect — it's a
heuristic for prioritization, not for matching.

#### 3. The Builder Pool (N Worker Threads — Phase 2+)

Heavy work: parsing font tables, building indices. Uses `rayon` (already a dependency)
or a small thread pool. Pulls from the priority queue and writes results to the Registry.

**Priority levels:**

| Priority | Source | Example |
|----------|--------|---------|
| **Critical** | `request_fonts()` from main thread | Layout needs `font-family: "Comic Sans"` |
| **High** | Common fallback families (from Scout) | OS sans-serif, serif, monospace defaults |
| **Medium** | Disk cache hit (cheap deserialization) | Font metadata from previous run |
| **Low** | Everything else found by Scout | Random font in `/usr/share/fonts/` |

**Processing loop (per worker thread):**
```
loop {
    job = queue.pop();  // blocks if empty, wakes on new jobs or priority boost

    // 1. Already processed? (deduplication)
    if registry.has_font_for_path(job.path) { continue; }

    // 2. Check disk cache
    if let Some(cached) = disk_cache.lookup(job.path, job.mtime) {
        registry.insert(cached);  // ~0.01ms
    } else {
        // 3. Cache miss → full parse including CMAP verification
        let parsed = FcParseFont(job.path);  // ~0.5ms per font
        disk_cache.store(job.path, job.mtime, &parsed);
        registry.insert(parsed);
    }

    // 4. Check if any pending request from main thread is now satisfied
    check_and_signal_pending_requests();
}
```

---

## The Startup Sequence

### What happens on `App::create()`

```
App::create(initial_data, config):
    │
    ├── 1. Create empty FcFontRegistry
    │
    ├── 2. Register bundled fonts (Material Icons, etc.)       ← <1ms
    │
    ├── 3. Try to load on-disk font cache
    │      ├── If found: deserialize ALL font metadata         ← ~10-20ms
    │      │   into Registry immediately. Mark all entries
    │      │   as "from_cache" (needs background verification)
    │      └── If not found: Registry starts empty
    │
    ├── 4. Spawn Scout thread                                  ← returns immediately
    │      Scout enumerates dirs, feeds Builder queue
    │      Also detects stale/new/removed fonts vs cache
    │
    ├── 5. Spawn Builder pool (N threads)                      ← returns immediately
    │      Starts processing queue (High-priority first)
    │
    └── return App  (total: <25ms with cache, <5ms without)
```

### What happens between `App::create()` and first layout

This is the **free parallelism window** — the main thread is busy with:
- Creating the OS window (~5-10ms)
- Initializing OpenGL context (~5ms)
- Creating WebRender instance + compiling shaders (~100-200ms first time, ~5ms cached)
- User's DOM construction code

During all of this, the Builder pool is **racing ahead**, parsing fonts in the background.
On a machine with 8 cores and the Scout feeding High-priority common fonts first, by the
time the main thread reaches layout:
- **With cache:** All 1000 fonts are already in the Registry (loaded from cache in Step 3)
- **Without cache:** Common fonts (sans-serif, serif, monospace — ~20 files) are likely
  already parsed. Other fonts are in progress.

### What happens at first layout (`request_fonts`)

This is where the **blocking guarantee** kicks in. No FOUC, ever.

```
collect_and_resolve_font_chains(styled_dom, registry):
    │
    ├── 1. Extract all unique font-family stacks from StyledDom
    │      e.g. ["Roboto", "sans-serif"], ["Fira Code", "monospace"]
    │
    ├── 2. For each family, check if it's in the Registry
    │      ├── Found → great, use it
    │      └── Not found → add to "missing" list
    │
    ├── 3. If missing list is empty → return immediately (common case with cache)
    │
    ├── 4. If missing list is non-empty:
    │      │
    │      ├── a. For each missing family, look up known_paths (from Scout)
    │      │      If Scout found matching files → push to Builder queue
    │      │      at CRITICAL priority with a completion token
    │      │
    │      ├── b. If Scout hasn't finished yet, wait for Scout first,
    │      │      then do the lookup
    │      │
    │      └── c. BLOCK main thread on request_complete condvar
    │            Builder threads process Critical jobs first,
    │            signal when all requested families are loaded
    │            (or confirmed not to exist on the system)
    │
    └── 5. All fonts guaranteed available. Proceed with layout.
```

**Worst-case latency at layout time:**

| Scenario | Block duration | Why |
|----------|---------------|-----|
| Warm cache, common fonts | **0ms** | Everything loaded from cache in Step 3 |
| Warm cache, exotic font | **0ms** | Even exotic fonts are in cache |
| Stale cache, font renamed | **~1ms** | Re-parse one file at Critical priority |
| Cold boot, common fonts | **~5-20ms** | Builder already parsed them (High priority) |
| Cold boot, exotic font | **~20-50ms** | Scout must find it, Builder must parse it |
| Cold boot, font not on system | **~20ms** | Wait for Scout to finish, confirm not found |

In all cases, this is dramatically better than the current 700ms, and the user
**never** sees the wrong font.

### What happens after first layout

The Builder pool continues parsing remaining fonts at Low priority in the background.
This populates the Registry for future use (e.g., user opens a font picker, document
references a new font, etc.).

The disk cache is written/updated once the Builder has processed all fonts, or
periodically as batches complete.

---

## On-Disk Font Cache

### Directory Structure

```
~/.cache/azul/                          (Linux: $XDG_CACHE_HOME/azul/)
~/Library/Caches/azul/                  (macOS)
%LOCALAPPDATA%\azul\                    (Windows)
  │
  ├── fonts/
  │     └── manifest.bin                 Font cache: path → mtime + parsed metadata
  │
  ├── shaders/
  │     └── <gl_fingerprint>/            Keyed by GL vendor + version + source hash
  │           ├── ps_text_run.bin
  │           ├── ps_image.bin
  │           └── ...
  │
  └── version                            Cache format version (for breaking changes)
```

The ricing CSS stays in its existing location:
```
~/.config/azul/styles/<app_name>.css    (Linux)
~/Library/Application Support/azul/styles/<app_name>.css  (macOS)
%APPDATA%\azul\styles\<app_name>.css    (Windows)
```

All azul apps share the same cache directory. A font parsed by app A is immediately
available to app B. Similarly, shaders compiled by app A are reused by app B (since
they share the same WebRender version and GL context fingerprint).

### Cache Design

A minimal binary format. Not a general-purpose database — just bincode-serialized
structs written to disk.

```
AzulCache {
    base_dir: PathBuf,           // ~/.cache/azul/
}

impl AzulCache {
    fn base_dir() -> PathBuf;

    // Font cache
    fn load_font_manifest() -> Option<FontManifest>;
    fn save_font_manifest(manifest: &FontManifest);

    // Shader cache
    fn load_shader_binary(name: &str, fingerprint: &str) -> Option<ShaderBinary>;
    fn save_shader_binary(name: &str, fingerprint: &str, binary: &ShaderBinary);
    fn gl_fingerprint(vendor: &str, version: &str, source_hash: &str) -> String;
}
```

### Font Manifest Format

```
FontManifest {
    version: u32,                              // bump on format changes
    created: SystemTime,
    entries: BTreeMap<PathBuf, FontCacheEntry>,
}

FontCacheEntry {
    mtime: SystemTime,                         // file modification time at parse time
    file_size: u64,                            // additional staleness check
    font_indices: Vec<FontIndexEntry>,         // one per font in a .ttc collection
}

FontIndexEntry {
    pattern: FcPattern,                        // full parsed metadata
    // NOT the font bytes — those are loaded from disk on demand via mmap
}
```

### Cache Load Strategy: "Trust Then Verify"

On startup, if a disk cache exists:
1. **Deserialize the entire manifest immediately** (~10-20ms for 1000 entries).
   Insert all patterns into the Registry. Mark them as `CacheSource::Disk`.
2. **The Scout thread verifies in background:**
   - For each path on disk: check if it's in the manifest with matching `mtime`/`size`
   - **Stale entry** (mtime/size changed) → re-parse at Medium priority
   - **Missing entry** (file deleted) → remove from Registry
   - **New file** (not in manifest) → parse at Low priority
3. **The main thread doesn't wait for verification.** It uses the cached data immediately.
   The chance that a font changed between the last run and now is extremely low.
   If a font did change, the background verification will fix the Registry within seconds,
   and any subsequent layout pass will pick up the corrected data.

This means: **with a cache, the first frame is always instant and always correct**
(modulo the extremely rare case of a font being modified between runs).

---

## On-Disk Shader Cache

### Philosophy: Simple, Synchronous, No Background Threads

Shader compilation is fundamentally different from font loading:
- There are only ~60 shaders (not 1000)
- Compiling takes ~100-200ms total (not 700ms)
- Compiled shaders are GPU-binary blobs, not metadata
- The GL context must exist before loading shaders (can't pre-load)
- All shaders are needed before the first frame (can't be lazy)

Therefore: **no background threads for shaders.** Just cache to disk and load on startup.

### How It Works

On `create_webrender_instance()`:

1. Compute the GL fingerprint: `hash(gl_vendor + gl_version + shader_source_hash)`
2. Check `~/.cache/azul/shaders/<fingerprint>/`
3. **Cache hit:** Load all program binaries via `glProgramBinary()`. Takes ~5-10ms.
4. **Cache miss:** Compile all shaders as today. Then save via `glGetProgramBinary()`.
   This is the ~100-200ms cost, paid **only once per GPU driver + azul version combo**.

Since all azul apps share `~/.cache/azul/shaders/`, the very first azul app on a system
pays the compile cost, and every subsequent app gets instant shader loading.

### Shader Pre-Compilation Helper (Optional Developer API)

For developers who want the absolute best first-run experience, provide:

```rust
/// Pre-compile all WebRender shaders into the disk cache.
///
/// Creates a temporary hidden GL window, compiles all shaders, saves them
/// to ~/.cache/azul/shaders/, then destroys the window. This function
/// blocks until compilation is complete (~100-200ms).
///
/// Call this from your app's installer, first-run wizard, or post-install script.
/// After this, the actual app startup will load cached shaders (~5ms).
///
/// If shaders are already cached and the cache is valid, this is a no-op.
pub fn azul_precompile_shaders() -> Result<(), ShaderCacheError>;
```

**Use case:** An app installer runs `azul_precompile_shaders()` as a post-install step.
The very first user launch gets cached shaders for free.

**Alternative for truly zero-latency first startup:** The app can start with CPU rendering
(the `cpurender.rs` path already exists) for the first frame, then switch to GPU rendering
once shaders are compiled. But this adds complexity — the `azul_precompile_shaders()` API
is the cleaner solution for developers who need the best "onboarding" experience.

### Shader Sharing Across Windows

Independent of the disk cache: currently each window passes `None` for the `SharedShaders`
parameter, causing WebRender to recompile all shaders per window. Fix this by keeping a
process-level `Rc<RefCell<Shaders>>` and passing it to subsequent windows.

```
Current:  Window 1: compile (200ms), Window 2: compile (200ms), ...
Fixed:    Window 1: compile (200ms), Window 2: reuse (0ms), ...
Cached:   Window 1: load (5ms),     Window 2: reuse (0ms), ...
```

---

## Changes Required in `rust-fontconfig`

### 1. New `FcFontRegistry` struct (replaces monolithic `FcFontCache` for async use)

The existing `FcFontCache` stays as-is for backward compatibility. A new `FcFontRegistry`
wraps it with thread-safe incremental population:

- All `BTreeMap` fields wrapped in `RwLock`
- New `known_paths: RwLock<BTreeMap<String, Vec<PathBuf>>>` for Scout results
- Priority queue + condvar for Builder pool
- `request_fonts(families) -> blocks until ready` method for main thread
- `AtomicBool` flags for `scan_complete` / `build_complete`
- `AtomicUsize` for progress tracking

### 2. `FcScout` — Filesystem-only enumerator

Extract the directory-walking logic from `FcFontCache::build()` into a standalone function
that returns `Vec<(PathBuf, String)>` (path + guessed family). No file I/O beyond `readdir`.

The family-name guessing heuristic:
- Strip extension: `"ArialBold.ttf"` → `"ArialBold"`
- Strip common suffixes: `-Regular`, `-Bold`, `-Italic`, `-Light`, `_Regular`, etc.
- Split on CamelCase / hyphens / underscores: `"NotoSansJP"` → `["noto", "sans", "jp"]`
- Lowercase everything
- Primary guess = full stripped name; secondary = first token group

### 3. `FcParseFont` — Full parse always, but only for needed fonts

The existing `FcParseFont` (including `verify_unicode_ranges_with_cmap()`) is kept as-is.
We do **NOT** trust OS/2 ranges — CMAP verification always runs. This is affordable
because we are no longer parsing 1000 fonts upfront; we only parse the ~5-20 fonts
that are actually requested via the priority queue. Full CMAP verification on 5 fonts
takes ~2.5ms — negligible.

The disk cache stores **fully verified** metadata (post-CMAP-check). A cache hit
skips all parsing entirely.

### 4. Priority Queue

```
FcBuildJob {
    priority: Priority,         // Critical > High > Medium > Low
    path: PathBuf,
    font_index: Option<usize>,  // for .ttc collections
}

enum Priority {
    Critical,  // Main thread is blocked waiting for this
    High,      // Common OS default fonts (sans-serif, serif, monospace)
    Medium,    // Disk cache hit (just deserialization)
    Low,       // Everything else
}
```

The queue supports **reprioritization**: when the main thread calls `request_fonts()`
for a family that's already in the Low queue, a new Critical job is pushed for the same
path. The Builder deduplicates via a `HashSet<PathBuf>` of already-processed files.

### 5. The `request_fonts()` blocking API

The key new method on `FcFontRegistry`:

```rust
impl FcFontRegistry {
    /// Block the calling thread until all requested font families are loaded
    /// (or confirmed to not exist on the system).
    ///
    /// This is called by the layout engine before the first layout pass.
    /// It boosts the priority of any not-yet-loaded fonts to Critical and
    /// waits for the Builder to process them.
    ///
    /// If the Scout hasn't finished yet, this also waits for the Scout
    /// to complete (so we can look up file paths for the requested families).
    ///
    /// Returns the resolved font chains for all requested families.
    pub fn request_fonts(
        &self,
        families: &[Vec<String>],  // each inner Vec is a CSS font-family stack
    ) -> Vec<FontFallbackChain>;
}
```

**Implementation:**
1. For each family in each stack, check if already loaded → skip
2. For missing families, wait for Scout to complete (if not already)
3. Look up `known_paths` for each missing family
4. Push matching paths to queue at Critical priority
5. Wait on `request_complete` condvar
6. Builder threads process Critical jobs first, signal condvar when
   all requested families are satisfied (or confirmed not on system)
7. Resolve chains from the now-populated Registry and return

### 6. `FcFontCache::build()` remains as a convenience

For users who don't need async loading (CLI tools, batch processors), the existing
synchronous `build()` function stays. Internally it can use the same Scout + Builder
pipeline but just blocks until completion:

```rust
impl FcFontCache {
    pub fn build() -> Self {
        let registry = FcFontRegistry::new();
        registry.run_scout_sync();
        registry.run_builder_sync();  // blocks until all fonts parsed
        registry.into_fc_font_cache()  // snapshot into immutable FcFontCache
    }
}
```

---

## Changes Required in `azul`

### 1. `App::create()` — No longer blocks on font loading

**Current** (`dll/src/desktop/app.rs:150`):
```rust
let fc_cache = Arc::new(FcFontCache::build());  // 700ms blocking
```

**New:**
```rust
let registry = Arc::new(FcFontRegistry::new());

// Register bundled fonts (Material Icons, etc.) — <1ms
registry.register_memory_fonts(bundled_fonts);

// Load disk cache if available — ~10-20ms (or 0ms if no cache)
registry.load_from_disk_cache();

// Start background scanning + parsing — returns immediately
registry.spawn_scout();
registry.spawn_builder_pool(num_cpus);

// Total: <25ms with cache, <5ms without. Proceed to window creation.
```

### 2. Layout engine — Block on `request_fonts()`

**Current** (`layout/src/solver3/getters.rs:2391`):
```
collect_and_resolve_font_chains() queries the fully-built FcFontCache
```

**New:**
```
collect_and_resolve_font_chains(styled_dom, registry):
    1. Extract all font-family stacks from StyledDom
    2. Call registry.request_fonts(stacks)  // ← BLOCKS until ready
    3. Proceed with layout using the guaranteed-available fonts
```

No FOUC. No fallback rendering. No re-layout needed. The first frame is correct.

### 3. `FontManager` — Works with `FcFontRegistry`

`FontManager<T>` (in `layout/src/font_traits.rs`) currently holds `Arc<FcFontCache>`.
Change to `Arc<FcFontRegistry>`. All read methods (`get_font_bytes()`, `get_metadata()`,
etc.) acquire the appropriate `RwLock` read lock.

### 4. `FontLoadingConfig` — Wire it in

The `FontLoadingConfig` enum already exists (`dll/src/desktop/app.rs:116`) but is
**not connected** to any logic. Wire it into the new pipeline:

- `LoadAllSystemFonts` → Run Scout + Builder for all directories (default)
- `LoadOnlyFamilies(families)` → Run Scout for all directories but only queue
  matching families at High priority; others stay at Low
- `BundledFontsOnly` → Skip Scout entirely, only use memory fonts

### 5. WebRender Shader Caching

**Current** (`dll/src/desktop/wr_translate2.rs:123`):
```rust
pub const WR_SHADER_CACHE: Option<&Rc<RefCell<webrender::Shaders>>> = None;
```

**New:**
1. On first window creation:
   - Compute GL fingerprint
   - Check `~/.cache/azul/shaders/<fingerprint>/` for cached binaries
   - If valid → load via `glProgramBinary()` (~5ms)
   - If miss → compile as usual (~100-200ms), then save via `glGetProgramBinary()`
2. Store `Rc<RefCell<Shaders>>` at App level
3. Pass `Some(&shared_shaders)` to subsequent windows → 0ms per extra window

**Note:** `glGetProgramBinary` / `glProgramBinary` require OpenGL 4.1+ or
`GL_ARB_get_program_binary`. On macOS with Metal-backed GL, this works. On older
systems, fall back to recompilation (same as today).

### 6. `azul_precompile_shaders()` — Optional developer helper

New public API function:
```rust
/// Pre-compile all WebRender shaders into the disk cache.
///
/// Creates a temporary hidden GL window, compiles all shaders, saves them
/// to ~/.cache/azul/shaders/, then destroys the window. This function
/// blocks until compilation is complete (~100-200ms).
///
/// Call this from your app's installer, first-run wizard, or post-install script.
/// After this, the actual app startup will load cached shaders (~5ms).
///
/// If shaders are already cached and the cache is valid, this is a no-op.
pub fn azul_precompile_shaders() -> Result<(), ShaderCacheError>;
```

Intended for use in app installers or first-run wizards. After this function runs,
the actual app startup loads cached shaders in ~5ms.

All azul apps share the same shader cache. If app A has already compiled shaders for
the same GL fingerprint, app B gets them for free on first run.

For the absolute best "onboarding" UX: the first startup of app B can use CPU rendering
(via the existing `cpurender.rs` path) while shaders compile synchronously, then switch
to GPU rendering. But this is an advanced optional optimization — the disk cache alone
solves the problem for all runs after the first.

---

## Performance Expectations

### Cold Boot (no disk cache, no shader cache)

```
Timeline (ms):
0       5       10      15      20      25     ...    50     ...   200
│       │       │       │       │       │             │            │
├─ App::create ─┤                                     
│ spawn threads │                                     
│               ├── Window creation ──────────────────┤
│               │   (GL context, shader compile)      │
│               │                                     ├── Layout ─┤
│               │                                     │ BLOCK for │
│               │                                     │ fonts ~5ms│
│               │                                     └── Render  │
│                                                                 │
│  Scout ───────┤ (~10ms, enumerates all paths)                   │
│               │                                                 │
│  Builder ─────┼── High priority fonts ──────────────┤ (done)    │
│               │   (sans-serif, serif, monospace)    │           │
│               ├── Critical fonts (if any) ──────────┤           │
│               └── Low priority fonts ───────────────────────────┼──→ ...
│                                                                 │
First frame visible ─────────────────────────────────────────────→│ ~200ms*
                                                                    (shader compile
                                                                     dominates)
```

*Shader compilation (~100-200ms) dominates cold boot. Font blocking is ~5-20ms.

| Phase | Time | What happens |
|-------|------|-------------|
| App::create + spawn | ~5ms | Registry created, threads spawned |
| Scout completes | ~10ms | All paths known, Builder queue populated |
| Window + GL + WebRender | ~200ms* | Window, GL context, shader compile (first time) |
| **Layout block** | **~0-20ms** | Wait for requested fonts (likely already done) |
| First frame visible | **~210ms** | Correct fonts, no FOUC |
| Background complete | ~300ms | All 1000 fonts parsed, cache written |

### Warm Boot (valid disk cache + shader cache)

| Phase | Time | What happens |
|-------|------|-------------|
| App::create + cache load | ~20ms | All 1000 font patterns loaded from cache |
| Window + GL + shader load | ~15ms | Window, GL context, cached shader load |
| **Layout block** | **0ms** | All fonts already in Registry from cache |
| First frame visible | **~35ms** | Correct fonts, no FOUC |
| Background verification | ~50ms | Scout confirms no fonts changed (async) |

### Comparison

| Scenario | Current | New (cold) | New (warm) |
|----------|---------|------------|------------|
| App::create() | 700ms | 5ms | 20ms |
| First window | +200ms | +200ms* | +15ms |
| First frame | **~900ms** | **~210ms** | **~35ms** |
| Second run | ~900ms | ~35ms | **~35ms** |
| Nth run | ~900ms | ~35ms | **~35ms** |

*Cold boot without shader cache. With shader cache (from any previous azul app): ~50ms.

---

## Migration Path

### Step 1: Async Pipeline + `request_fonts()` (rust-fontconfig)

Implement `FcFontRegistry`, Scout, Builder pool, priority queue, and the blocking
`request_fonts()` API. Full CMAP verification runs on every parsed font — this is
affordable because only the requested fonts are parsed (not all 1000).
This is the largest change but eliminates startup latency and guarantees no FOUC.

**Dependencies:** `serde`, `bincode`, `crossbeam-channel` (or `std::sync`).

### Step 2: Disk Cache (rust-fontconfig + azul)

Add `serde` support to all font metadata types. Implement `AzulCache` with the font
manifest format. The cache stores fully-verified metadata (post-CMAP-check), so warm
boots are instant (~20ms) with no correctness trade-off.

**Dependencies:** `serde`, `bincode`.

### Step 3: Shader Disk Cache (azul)

Implement `glGetProgramBinary` / `glProgramBinary` caching. Share `Shaders` across
windows in the same process. Implement `azul_precompile_shaders()`.

### Step 4: Wire `FontLoadingConfig` (azul)

Connect the existing config enum to the new pipeline. Low effort, high value for apps
that know exactly which fonts they need.

---

## Cache Invalidation Summary

| Event | Detection | Action |
|-------|-----------|--------|
| Font file modified | `mtime` / `file_size` changed (Scout) | Re-parse in background, update manifest |
| Font file deleted | Scout doesn't find path | Remove from Registry + manifest |
| New font installed | Scout finds unknown path | Parse at Low priority, add to manifest |
| GPU driver update | GL fingerprint changed | Shader cache miss → recompile + cache |
| Azul version update | Cache `version` field changed | Invalidate all caches |
| WebRender update | Shader source hash changed | Shader cache miss → recompile + cache |
| Manual clear | User deletes `~/.cache/azul/` | Full rebuild on next start (graceful) |

**Stale font cache policy:** A stale cache (loaded at startup) is used immediately and
corrected in the background. The user never notices unless a font was completely replaced
with a different font at the same path — an extremely rare event that is silently
corrected by the background Scout/Builder verification.

---

## Resolved Design Decisions

1. **CMAP verification: Always run, even on first boot.** We do NOT trust OS/2 unicode
   range bitfields. Since the new architecture only parses the ~5-20 fonts actually
   needed (not all 1000), full CMAP verification is affordable (~0.5ms per font).
   The disk cache stores fully-verified metadata, so subsequent runs skip verification.

2. **Thread count for Builder pool:** Use all available threads:
   `std::thread::available_parallelism().saturating_sub(1).max(1)`. Font parsing is
   mixed I/O + CPU, and we want maximum throughput during the race window between
   `App::create()` and the first layout pass.

3. **Font hot-reloading:** No automatic re-scanning. The existing `CallbackInfo` API
   exposes a function to reload the font cache. Applications use that as the trigger
   for re-scanning (e.g., after the user installs a font).

4. **Cache contents:** The disk cache stores only the parsed+verified font metadata
   (post-CMAP-check `FcPattern`s). No raw font bytes, no unverified data. Cache size
   for 1000 fonts is ~200-500KB — negligible.

5. **`request_fonts()` timeout:** Hard timeout of 5 seconds. If exceeded, log an error
   and proceed with whatever fonts are available. This prevents hangs if the Builder
   pool panics or the filesystem is unresponsive.

6. **CPU rendering as first-startup fallback:** No. This is an explicit opt-in
   optimization that can be added later. It should not be the default behavior — it's
   easy to misuse and adds complexity. The `azul_precompile_shaders()` API is the
   recommended solution for developers who need zero-latency first startup.

---

## Implementation Status

### ✅ Phase 1: Font Registry (COMPLETE)

**rust-fontconfig changes:**
- `src/registry.rs` (~1400 lines): `FcFontRegistry`, Scout thread, Builder pool,
  `request_fonts()` with 5s timeout, disk cache (bincode), `into_fc_font_cache()` snapshot,
  `cache_loaded` fast path, builder deduplication for cached paths
- `src/lib.rs`: serde derives on 8 data structs, public API surface for registry access
- `Cargo.toml`: New features `cache` (serde+bincode) and `async-registry` (std+parsing)
- Compiles cleanly: 0 errors, 0 warnings

**azul integration:**
- `dll/Cargo.toml`, `layout/Cargo.toml`, `core/Cargo.toml`: Local path dependency with new features
- `dll/src/desktop/app.rs`: `AppInternal` stores `font_registry: Option<Arc<FcFontRegistry>>`,
  `App::create()` spawns registry (returns immediately), loads disk cache, spawns Scout+Builders
- `dll/src/desktop/shell2/run.rs`: All 4 platform `run()` functions accept `font_registry`
- `dll/src/desktop/shell2/common/layout_v2.rs`: `regenerate_layout()` calls `request_fonts()`
  before layout to block until needed fonts are ready, then snapshots to `FcFontCache`
- Platform backends (macOS, Windows, Linux/X11, Linux/Wayland): `font_registry` threaded through
  window structs and constructors
- `dll/src/desktop/wr_translate2.rs`: Changed `ShaderPrecacheFlags::FULL_COMPILE` →
  `ShaderPrecacheFlags::EMPTY` (lazy shader compilation) to avoid 671ms blocking
- Compiles cleanly: 0 errors, 0 warnings

### Benchmark Results (macOS, M-series, ~1000 system fonts)

**hello-world.c C example:**

| Phase | Before | After (warm cache) | Speedup |
|-------|--------|-------------------|---------|
| `App::create()` (font scan) | ~700ms | **73ms** | 9.6× |
| GL context + view setup | ~200ms | **206ms** | — |
| WebRender init (shaders) | ~200ms¹ | **20ms** | 10× |
| `request_fonts()` | N/A | **0.4ms** | ∞ |
| `into_fc_font_cache` snapshot | N/A | **2.5ms** | — |
| First layout callback | ~1400ms | **458ms** | 3.1× |
| Total window creation | ~1400ms | **570ms** | 2.5× |

¹ Previously included in `App::create()` total; separated now.

**Breakdown of remaining 458ms to first layout:**
- 73ms: `App::create()` (registry creation, disk cache load, thread spawn)
- 206ms: macOS NSWindow + NSOpenGLContext + view creation
- 20ms: WebRender initialization (lazy, no shader pre-compilation)
- 0.4ms: `request_fonts()` with disk cache (fast path)
- 2.5ms: Font registry → FcFontCache snapshot
- ~156ms: Layout solving + on-demand shader compilation for first frame

**Font disk cache:** `~/Library/Caches/azul/fonts/manifest.bin` (1.4MB for ~1000 fonts)

### ❌ Phase 2: Shader Disk Cache (NOT STARTED)

Shader caching (`glGetProgramBinary`/`glProgramBinary`) is a simpler follow-up task.
The GL context setup (206ms) and on-demand shader compilation (~156ms) are the remaining
optimization targets. See the "Shader Compilation Caching" section above for the plan.
