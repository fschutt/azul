# Release 0.2.0 size + memory audit (2026-07-04)

**Status: REPORT ONLY — no fixes applied.** Companion doc:
`scripts/WEB_WASM_DIET_PLAN_2026_07_04.md` (azul-mini.wasm architecture
plan). Method: downloaded **all** artifacts linked from
`https://azul.rs/ui/release/0.2.0` (99 live files, 3.40 GiB; 38 linked
files 404 — see §1.2), dissected with bloaty/nm/otool/strings/custom COFF+
Mach-O parsers, cross-checked against a local `master` build; RSS profiled
with the in-tree tooling from `guide/memory-profiling.md`
(`AZ_PROFILE=memory`, `AZ_E2E_TEST` headless harness) plus macOS
`vmmap`/`footprint`/`heap`/`malloc_history`.

Constraint honored throughout: **no `-Oz` / opt-level / codegen changes**
on native artifacts — every size lever below is metadata, packaging,
feature-gating, or data-placement; `.text` bytes are untouched.

---

## 0. Executive summary

**Size.** The release weighs ~3.4 GB but ~2.4 GB of it is a handful of
packaging bugs, not code: embedded LLVM bitcode shipped inside every
static library (56–89 % of each `.a`/`.lib`), unstripped DWARF in the
Android archives, a 245 MB `examples.zip` that is 80 % duplicated demo
binaries, and a 158 MB unstripped `.so` inside every Android APK. The
dynamic libraries themselves (24–45 MB) are comparatively healthy; their
levers are worth ~2–7 MB each (hyphenation dictionaries, SQLite/pdf
feature bloat in default builds, panic-path strings) — see §2.3.

**Memory.** A hello-world window on macOS has a 94 MB physical footprint.
It decomposes almost exactly into: ~37 MB WebRender texture-cache atlases
(GPU driver memory, default `TextureCacheConfig`), **21.4 MB = one heap
copy of the system font's entire `glyf` table** made by
`allsorts::LocaGlyf::load`, ~10 MB window IOSurfaces, ~15 MB misc heap,
remainder shared lib pages. There is **no leak in steady state** (100
headless resize iterations: RSS flat). The video-app 1 GB blowup is a
confirmed unbounded leak with a precise root cause: **azul never emits
`DeleteImage`** — every video frame registers a brand-new permanently
retained image in WebRender's resource cache (§3.4).

Top 10 actions by impact (details in referenced sections):

| # | action | saves | where |
|---|---|---|---|
| 1 | strip `.llvmbc`/`__bitcode` from shipped `.a`/`.lib` | ~1.3–1.5 GB of release | §2.4 |
| 2 | `examples.zip`: sources+headers only | −240 MB | §2.5 |
| 3 | strip Android `.a` DWARF (NDK `llvm-strip`) + APK `.so` | −379 MB raw + APKs ×4 smaller | §2.4, §2.6 |
| 4 | wire image GC (`DeleteImage` sweep) | fixes 1 GB video leak | §3.4 |
| 5 | stop boxing the whole `glyf` table per font | −21 MB RSS in *every* app | §3.3 |
| 6 | externalize/trim baked ICU4X data + hyphenation dicts | −3–6 MB per Linux/Win artifact | §2.3, §2.10 |
| 7 | feature-split kitchen-sink APIs (pdf/db/http/video) into core+full artifacts | −6–10 MB per artifact | §2.2, §2.10 |
| 8 | ship pure-C `libazul.so` (not the +Python combined build) in deb/standalone | −6.3 MB ×3 artifacts (and the 43-vs-27 MB optics) | §2.7, §2.10 |
| 9 | `TextureCacheConfig` below WR defaults (esp. mobile) | −10–30 MB RSS | §3.3 |
| 10 | armv7: build Thumb2 (`thumbv7neon-*`) — currently pure ARM mode | −4–6 MB on armv7 | §2.10 |
| 11 | RELR packed relocs (needs glibc ≥ 2.36 floor), lld `--icf=all`, drop js_sys/wasm-bindgen from native, `--remap-path-prefix` | −2–6 MB per Linux artifact | §2.10 |
| 12 | ~~decide panic=unwind vs abort~~ CORRECTED: dist already IS abort (misdiagnosis — see §2.10d); CI now asserts it | 0 (was misread) | §2.10 |
| 13 | fix 38 dead links / missing artifacts on release page | correctness | §1.2 |

---

## 1. Artifact inventory

### 1.1 What ships (99 files, 3.40 GiB)

Biggest first (MB):

| artifact | MB | | artifact | MB |
|---|---|---|---|---|
| azul.lib | 294.9 | | libazul.linux-i686.so | 35.1 |
| azul.i686.lib | 281.8 | | azul.pyd | 33.1 |
| examples.zip | 244.5 | | 4× android .apk | 33–38 |
| libazul-android-{arm64,x64}.a.tar.gz | 163+162 | | libazul.linux-{riscv64,armv7,aarch64}.so | 30 |
| 7× libazul.linux-*.a.tar.gz | 120–131 | | azul.so (macOS py) | 29.6 |
| libazul-ios{,-sim}-arm64.a.tar.gz | 115 ×2 | | azuldbg.dll | 28.8 |
| libazul.macos{,-x86_64}.a.tar.gz | 108 ×2 | | libazul.x86_64.dylib | 27.3 |
| libazul.linux-ppc64.so | 45.2 | | guide.pdf | 27.2 |
| libazul.so / azul.cpython.so | 42.9 ×2 | | azul.dll | 27.0 |
| libazuldbg.so | 37.5 | | libazul.dylib | 24.0 |
| 5× .deb | 14–19 | | 30× demo-app tarballs | 5.8–9.7 |

### 1.2 Dead links on the release page (38)

`curl --fail` 404s for: **all 26 iOS mobile-app artifacts**
(`mobile-apps/azul-*-ios{,-sim}.{ipa,app.zip}`), `azul.rust9x.dll`,
`azul.rust9x.lib`, **all 5 `.rpm`s**, and
`statistics/dependency-justifications`. The page CSS has an "inert tile"
class for not-shipped artifacts, but these render as normal links. Either
publish them or mark them inert; CI should link-check the release page it
generates (`doc/src/dllgen/deploy.rs` emits the grid, so it knows the
real file set).

Also: `statistics/cargo-geiger.txt` is 287 bytes (essentially empty —
the tool evidently failed; either fix or drop the link).

---

## 2. Binary size analysis

### 2.1 The macOS release dylib as reference (24.0 MiB)

bloaty section view of `libazul.dylib` (arm64, prod-release: thin-LTO,
cgu=1, strip=symbols, panic=abort):

| section | size | share |
|---|---|---|
| `__TEXT,__text` (code) | 15.8 MiB | 72.8 % |
| `__TEXT,__const` (ro data) | 5.7 MiB | 26.3 % |
| `__DATA_CONST,__const` | 1.12 MiB | |
| `__LINKEDIT` (string tab 409 K + export trie 293 K + symtab 225 K + codesig 190 K) | 1.18 MiB | |
| eh_frame + unwind_info | 135 KiB | small only because Mach-O uses compact unwind — the artifacts are **panic=unwind** (see §2.10), Linux/i686 pay real eh_frame |

Exports: 14,072 total = 13,916 `Az*` API (intentional; the C API *is* the
product) + **156 stray turso/SQLite extension exports** (`_uuid`,
`_time_now`, `_time_parse`, `_register_*VTabModule`, …). Beyond a few KB,
these generic names in the global dynamic namespace are a **symbol-clash
risk** for host apps (e.g. anything else defining `_time_parse`).
→ Localize them (visibility hidden / exported-symbols list = `_Az*` only).

### 2.2 What the 15.8 MiB of code is (crate attribution, local build)

`cargo bloat` on the statically-linked `widgets` example (linker
dead-strips unused API → the "true app cost") vs symbol aggregation of the
full dylib:

- Full dylib (must carry everything): azul_layout ~2.0, core+alloc generics
  ~4.4, **turso_core+turso_parser 2.14**, azul_css ~0.96, webrender ~0.9,
  azul_core ~0.8, allsorts ~0.56, hashbrown 0.58, std 0.53, **printpdf+lopdf
  ~0.52**, image+zune_jpeg+image_webp+tiff+png **~1.1**, rustls+ureq ~0.42,
  regex ~0.46, taffy 0.33 MiB, …
- Static widgets app: **7.0 MiB .text total** — no turso, no printpdf, no
  rustls (dead-stripped). A full azul app is ~7 MiB of code; the dll pays
  +9 MiB for API breadth.

Implications (no codegen change needed):
- The cdylib cannot dead-strip because the subsystems are **API-anchored
  by exports**: AzSvg 208 exported fns, AzXml 146, AzVideo 102, AzHttp 66,
  AzGamepad 42, AzDb 41, AzPdf 7 — `--gc-sections` can never remove what
  the export table roots. The lever is **feature composition**: a
  `libazul-core` without pdf (printpdf+lopdf ~3–4 MB), db (turso ~2–3 MB),
  http/tls (rustls+ureq+webpki ~1.5–2 MB), video (mp4+gpu_video+ash
  ~1–1.5 MB) is **−8–12 MB per artifact** (machine-code figures from
  per-object .text+.rdata in azul.lib, cross-checked against dylib symbol
  aggregation). Product decision: mirror the existing python-artifact
  split with core/full artifacts.
- Language bindings that can consume the **staticlib** get ~7 MiB apps for
  free — worth documenting as the size-sensitive path.

### 2.3 Read-only data: 5.7 MiB `__TEXT,__const` dissected

- **~2.8 MiB: hyphenation dictionaries for ~70 languages.**
  `layout/Cargo.toml:48`: `hyphenation … features = ["embed_all"]`, on in
  default features via `text_layout_hyphenation`. Identified as the
  2.77 MiB `_anon.ebd3116…` blob (bincode-serialized patterns).
  → embed en-US only by default; load other locales from disk/asset at
  runtime (the crate supports it), or a `hyphenation_all` opt-in feature.
  Saves ~2.8 MB on **every** platform artifact including wasm lift input.
- **1.58 MiB printable strings** (45,521 runs ≥8 bytes), of which:
  - 239 KiB cargo-registry **source paths** (`/Users/runner/work/azul/...`,
    `index.crates.io-…/objc2-avf-audio-0.3.2/src/generated/…`) — panic
    `Location` strings. → build releases with
    `--remap-path-prefix $HOME/.cargo=~c --remap-path-prefix $PWD=azul`
    (shrinks + de-leaks CI paths). ~150–200 KB.
  - layout debug/trace format strings (`[layout_bfc] ENTERED…`,
    `[TAFFY INPUT]…`, `--- Entering perform_fragment_layout ---`, kashida
    justification traces, …). Individually small, and the `[MEM]`/`[PRUNE]`
    logs printed *unconditionally* in release runs (§3.2) show some of
    these paths are `eprintln!`, not compiled-out trace macros. → sweep
    layout/text3 for bare `eprintln!`/`format!` diagnostics → `plog_*!`
    (feature-gated), which also removes the strings.
  - 24.4 KiB single icon-name→codepoint string + ~900 KiB total
    `icon_to_char` region (`icons` default feature, Material icon names).
    → `phf` at build time or u32-hash keys instead of concatenated names;
    or make `icons` non-default.
  - 11 KiB GL function-name table (gl-context-loader), 57 KiB SQL strings
    (turso), 8 KiB unicode property names — fine.
- Remainder ≈ 1–1.5 MiB: legit lookup tables (unicode, encoding_rs BIG5
  ~37 K, crc32 51 K (×2 impls?), brotli dictionary 119 K in DATA_CONST).

### 2.4 Static libraries: **the »295 MB azul.lib« is 58–89 % LLVM bitcode**

Verified by parsing COFF/Mach-O/ELF section headers of every archive
member:

| archive | raw size | `.llvmbc`/`__bitcode` | .text | DWARF |
|---|---|---|---|---|
| azul.lib (win x64) | 295 MB | **173–181 MB (~59–64 %)** | ~32 MB | 5.5 MB |
| azul.i686.lib | 282 MB | **~187 MB (68 %)** | ~30 MB | — |
| libazul.macos.a | 298 MB | **199 MB (78 %)** | 28.6 MB | ~4 MB |
| libazul.linux.a | 356 MB | **212 MB (77 %)** | 39.6 MB | 0 |
| libazul-ios-arm64.a | 365 MB | **296 MB (89 %)** | 17.8 MB | 0 |
| libazul-android-arm64.a | **708 MB** | 274 MB (56 %) | 17.4 MB | **179 MB DWARF + ~200 MB debug relocs** |

Root cause: `[profile.prod-release] lto = "thin"` embeds bitcode in every
object; cargo's `strip = "symbols"` only strips *linked outputs*, and the
CI `strip --strip-debug` belt (Cargo.toml comment, lines ~122–128) removes
debug info only — nobody removes `.llvmbc`. Consumers of a prebuilt
staticlib never run Rust ThinLTO, so the bitcode is 100 % dead weight.

→ **Post-process every shipped archive**: `llvm-objcopy
--remove-section=.llvmbc --remove-section=.llvmcmd` per ELF/COFF member,
`bitcode_strip -r` (or objcopy `__LLVM,__bitcode`) for Mach-O. Do **not**
switch to `-C embed-bitcode=no`/`lto=false` for this — that would change
codegen/perf; stripping post-build keeps the ThinLTO-optimized `.text`
bit-identical. Add a CI assert: `no .llvmbc section in shipped archives`.

→ **Android additionally**: run NDK `llvm-strip --strip-debug` on the
`.a` (host binutils `strip` can't process cross-ELF and evidently
silently skipped; 179 MB DWARF + ~200 MB `.rela.debug_*`).

Expected: each `.a` lands at ~60–100 MB raw / 20–40 MB gz;
**release total shrinks by ~1.3–1.5 GB**. Windows `azul.lib` 295→~110 MB.

(There is precedent in-repo: the Cargo.toml prod-release comment already
fought the debug=1-in-staticlib version of this problem; bitcode is the
second head of the same hydra.)

### 2.5 examples.zip: 244.5 MB → should be ~5 MB

127 files, 578.9 MB uncompressed: **demos/ = 460 MB (79.5 %)** of
prebuilt per-OS demo binaries that *all* also ship as individual
`gh/azul-*-{linux,macos,windows}` tarballs, + 98.5 MB root copies of
libazul.so/azul.dll/libazul.dylib (also duplicates), + 20 MB headers.
The 29 language example *source* dirs — the actual point of the zip —
are **< 0.3 MB combined**.
→ zip = sources + headers only (−240 MB, −98 %).

### 2.6 Mobile packages

- APKs contain a single **unstripped 158.6 MB `libazul_widgets.so`**
  (arm64). The Linux release .so is 31–43 MB stripped. The APK compresses
  it to 33 MB, but on-device it's extracted/mapped at full size, and
  jetsam/low-RAM devices pay dirty-page + install-size cost.
  → `llvm-strip` in the APK packaging step (`scripts/build-android.sh` /
  packaging/): APK ~33 MB → ~10–12 MB, install size −120+ MB.
- iOS `.a` = 89 % bitcode (§2.4). The linked demo `.app`s were not
  shipped this release (dead links, §1.2).

### 2.7 Linux/deb/python packaging crossovers

- **`libazul.so` (x86_64, 43 MB) is byte-identical (md5 `4834c6f9…`) to
  `azul.cpython.so`** — the C-ABI artifact is the *combined* C+PyO3 build
  (14,044 C exports + `PyInit_azul`). Every other arch ships a pure-C
  ~30–31 MB .so. The deb packages the same 45 MB combined build.
  → intentional per `rust.yml` ("python build is C-safe — make it THE
  release dylib"), but it costs +6.3 MB of PyO3 glue on the flagship
  artifact ×3 places (libazul.so, cpython.so is fine, deb) and makes
  x86_64 look 50 % fatter than aarch64. Ship the lean C build + a
  separate cpython module like macOS/Windows do (§2.10a/b).
- deb metadata: `Maintainer: Unset Maintainer <unset@localhost>` — fix.
- `azul.so` being a Mach-O named `.so` is **correct** (CPython on macOS
  requires the `.so` suffix) — not a bug, noting to preempt confusion.
- 5 × identical 53 KB `LICENSE-*.txt` — cosmetic.
- `guide.pdf` 27 MB: 21 MB Flate streams + **78 embedded font files** —
  the doc generator embeds fonts per-section without subsetting/dedup;
  printpdf's subsetting exists. → est. −15–20 MB. Low priority.

### 2.8 Cargo.lock duplicate-version audit (minor, shipped code)

87 crates resolve at 2+ versions; actually *shipped* in the dll per the
release `statistics/dependency-tree-*`: hashbrown 0.16.1+0.17.1,
rand 0.8.6+0.9.4, nom 7+8, zip 2.4.2 (+6.0/8.6 elsewhere), rstar ×5 in
the workspace (not all in dll). Each dup costs ~50–200 KB. → `cargo tree
-d -e normal` gate in CI with an allowlist; align versions opportunistically.

### 2.9 Per-platform shared objects: section tables

All artifacts on all 3 OSes are already **fully stripped** (no `.symtab`,
COFF nsym=0, no `.gnu_debuglink`, `.comment` ≈ 90 B) — stripping is a
non-lever. rustc 1.88.0 everywhere. Sizes MiB:

Linux ELF:

| file (total) | .text | .rodata | .rela.dyn | .data.rel.ro | .eh_frame | dyn sym/str/hash |
|---|---|---|---|---|---|---|
| libazul.so x86_64 (45.0) **= python build** | 27.42 | 8.08 | 2.57 | 2.20 | 1.50 | 0.82 |
| aarch64 (31.5) | 18.05 | 7.45 | 1.47 | 1.47 | 0.67 | 0.82 |
| armv7 (31.6) — **pure ARM mode, 0 Thumb fns** | 20.66 | 7.25 | 0.53 | 0.83 | 0.16 | 0.71 |
| i686 (36.8) | 22.01 | 7.72 | 0.53 | 0.83 | **3.27** | 0.71 |
| ppc64 (47.4) — +`.opd` 0.99 (ELFv1 descriptors) | 25.65 | 7.77 | **4.13** | **4.95** | 0.79 | 0.82 |
| s390x (39.7) | 25.47 | 7.91 | 1.47 | 1.48 | 0.66 | 0.82 |
| riscv64 (31.6) | 17.50 | 7.78 | 1.59 | 1.51 | 0.93 | 0.82 |
| libazuldbg.so (39.3) — debug-server feature, no python | 24.00 | 8.00 | 1.79 | 1.57 | 1.17 | 0.82 |

Windows PE (external azul.pdb; debug dir = PDB path only):
azul.dll x64 (28.4): .text 18.69, .rdata 7.96 · i686 (25.6): 17.12/6.83 ·
azuldbg (30.2): 20.23/8.17 · azul.pyd (34.8): 23.42/8.91.
macOS: see §2.1; x86_64 dylib __text 19.48 (arm64: 15.76).

### 2.10 Cross-platform findings & levers (evidence-backed, no codegen-perf impact)

**(a) The 43-vs-27 MB mystery fully decomposes.** Linux x86_64
`libazul.so` *is* the PyO3 build — deliberate per `rust.yml` ("the python
build is C-safe (weak stubs) — make it THE release dylib"); python glue
costs +6.3 MB (pyd−dll = +6.4, mac azul.so−dylib = +5.9). x86-64 code is
~24 % larger than arm64 (19.48 vs 15.76 MiB __text, same features). ELF
pays ~3.5 MB more metadata than Mach-O (real `.eh_frame` vs compact
unwind; 2.57 MiB `.rela.dyn` vs chained fixups). ppc64/s390x = code
density (+41 % .text) + ppc64's ELFv1 descriptor tax (.opd + TOC +
180 k relocs). Nothing is *wrong* with these files.

**(b) On macOS/Windows the engine ships twice.** `azul.so` / `azul.pyd`
are separate full builds (53 k functions vs 35 k; full `Az*` export
overlap + PyInit) — not thin wrappers over libazul. Either accept
(hermetic wheels are a feature) or make the python module link the C dylib.

**(c) `.rodata` ≈ 7.3–8.1 MiB per ELF, and the "mystery tables" are ICU4X
+ hyphenation.** Per-object attribution (from azul.lib members):
icu_segmenter 3.99 + icu_datetime 3.68 + icu_collator 1.06 +
icu_properties 0.42 + hyphenation 2.79 MiB const data (≈ 12 MB pre-GC,
landing as ~6.5 MiB of final .rodata; measured strings are only
0.8–1.2 MiB of .rodata — the rest is these tables). macOS avoids most of
the ICU blob (`icu_macos` uses system ICU — its __const is 5.7 MiB).
→ same treatment as hyphenation: `icu_provider_blob` loaded from a file
at runtime, trim the locale set, and prefer the dictionary segmenter over
bundled ML models; −3–6 MB per Linux/Windows artifact.

**(d) Dist artifacts are `panic=unwind`.** ~~`_Unwind_Resume` imported
everywhere; the workspace `[profile.release] panic="abort"` is evidently
overridden for dll dist builds~~ — **CORRECTED 2026-07-04 (perf/release-size-diet):
this was a misdiagnosis.** No shipped `.so` imports `_Unwind_RaiseException`
(the throw primitive only the Rust `panic_unwind` runtime uses) — the dist
artifacts ARE `panic=abort`. The observed `_Unwind_Resume` import comes from
C++ deps (vk-mem) and is present in known-abort local builds too; the
eh_frame bytes come from rustc's default `force-unwind-tables` (kept for
crash backtraces + C++ exception safety), not from the panic strategy.
CI now asserts abort stays true ("Assert panic=abort" step in rust.yml).
The i686 3.27 MiB eh_frame could only be removed with
`-C force-unwind-tables=no`, which risks C++ exception termination paths —
rejected.

**(e) Cheap linker/toolchain wins:**
- RELR packed relocs (`-Wl,--pack-dyn-relocs=relr`): 99.7–99.9 % of
  `.rela.dyn` is `R_*_RELATIVE` — saves x86_64 −2.4, ppc64 −3.9,
  aarch64/s390x −1.35, riscv64 −1.5 MB. ⚠ needs glibc ≥ 2.36 at runtime;
  current floor is Ubuntu 22.04 (glibc 2.35) — gate on raising it.
- lld `--icf=all` on Linux: MSVC already folds 14,044 exports → 6,940
  unique RVAs; the Linux links stop at 8,574 → ~0.5–1.5 MB per artifact.
- armv7 as Thumb2 (`thumbv7neon-unknown-linux-gnueabihf`): .text
  currently 20.66 MiB in pure ARM mode — bigger than aarch64! → −4–6 MB.
- Drop `js_sys`/`wasm-bindgen` from native builds (6 `__wbindgen_*`
  exports + ~1.2 MiB js_sys object linked in; from the `js`
  feature-unification for web cross-checks) → −0.3–1 MB per artifact.
- `--remap-path-prefix`: 163–210 KiB of source-path strings per file
  (§2.3).

**(f) `azuldbg` naming.** The "debug library" is the **same prod-release
optimization + `debug-server` feature** (+1.3–1.9 MB), no debug info.
Fine as a product (inspector builds), but rename/describe it —
users will expect symbols/assertions from a "-dbg" artifact.

**(g) Non-levers, checked and closed:** stripping (done), `.comment`
(90 B), export-table trimming (~0.9 MB total but it *is* the API; only
the 156 turso strays are actionable, §2.1), embedded debug info (none),
section alignment slack (~0), string-merge (already deduped).

**(h) Illustrative stacking (Linux x86_64):** 45.0 MB → −6.3 (python
split) → −1 (ICF) → −4 (ICU/hyphenation data diet) → **~33.7 MB**, →
~31.3 MB if the RELR glibc-2.36 floor is acceptable; with a core/full
split the core artifact lands ~22–25 MB. armv7: 31.6 → ~26 (Thumb2) →
~22.5 (data diet). All .text bytes identical or better (Thumb2/ICF),
zero opt-level changes.

---

## 3. Runtime memory (RSS)

### 3.1 Tooling used

As prescribed by `guide/memory-profiling.md`: examples built with
`--features azul/e2e-test` (pulls `azul-layout/probe`), then
- `AZ_E2E_TEST` headless scenario: 10 warmup ticks + 100 × (resize 800×600
  → 600×400 → resize_full → tick), RSS probe every 10 with
  `memory_breakdown: true`;
- windowed runs with `AZ_PROFILE=memory`;
- macOS: `vmmap -summary`, `footprint`, `heap`, `malloc_history`
  (`MallocStackLogging=lite`).

### 3.2 Baseline numbers (hello-world, macOS arm64, release)

| measurement | value |
|---|---|
| headless (AZ_E2E_TEST) baseline RSS | 44.1 MiB |
| headless after 100 resize loops | 42.2 MiB (**flat — no growth**) |
| headless heap (mstats bytes_used) | 38 → 43.5 MiB (slow creep, RSS flat) |
| windowed physical footprint | **94.1 MB** (peak 101.9) |
| windowed `ps` RSS | 56.9 MB |
| widgets demo windowed footprint | 110.9 MB (peak 121.7) |
| azul's own accounting (`AZ_PROFILE=memory`) | layout caches = **39 KiB** (0.1 % of RSS) |

So the user-visible "70–150 MB" is real, all *floor*, no leak — and
azul's tracked caches are irrelevant to it (39 KiB!). The floor
decomposes (windowed hello-world, vmmap dirty+swapped semantics):

| component | size | evidence |
|---|---|---|
| WebRender texture-cache atlases (GPU driver) | **33.3 MB** (46.5 MB in widgets) | `IOAccelerator (graphics)` region; matches `TextureCacheConfig::DEFAULT` = 2048² color + 2048² glyph + 2048² alpha-glyph + 1024² alpha ≈ 37 MB; azul passes no custom config (`webrender/core/src/texture_cache.rs:515`) |
| one boxed `glyf` table (system font) | **21.4 MB** | §3.3 — single MALLOC_LARGE, cold ⇒ compressor takes it |
| IOSurfaces (window swapchain) | 9.6 MB | 3 surfaces (retina 2×) |
| rest of heap (41 MB total live) | ~19 MB | 54 k Rust allocations + 8 k CFString/ObjC (AppKit) |
| dylib/framework dirty (`__DATA*`, GLSL builtins, ObjC RW) | ~8–10 MB | vmmap |

Two incidental findings:
- `[MEM]`/`[PRUNE]`/`[CASCADE]` blocks are correctly gated (absent in a
  plain release run, present with `AZ_PROFILE=memory`) — **but** their
  format strings, plus deeper trace strings that never printed in either
  run (`[layout_bfc] ENTERED…`, `[TAFFY INPUT]…`, kashida traces), are
  compiled into the release binary as `&str` constants (§2.3). Runtime is
  fine; the bytes are the cost.
- headless heap creep ~55 KB/iteration with all tracked caches flat —
  retention outside the instrumented caches (suspect: probe event buffer
  or window-state history). Worth a 1000-iteration
  `heap,jsonl,detail` run; below the priority bar today.

### 3.3 The floor, item by item

**(a) 21.4 MB: `allsorts` boxes the entire `glyf` table.**
`malloc_history` backtrace (abridged): `shape_text_internal →
ParsedFont::get_hinted_advance_px → get_or_decode_glyph →
allsorts_azul::tables::glyf::LocaGlyf::load (glyf.rs:1053) →
read_and_box_table (tables.rs:1476)` → `Box::from(table.into_owned())`.
`LocaGlyf::load` copies the **whole glyf table** (22.4 MB for the macOS
system TTC) into heap and keeps it for the font's lifetime — while
rust-fontconfig already holds the same file via `mmapio`. Decoded glyphs
are *additionally* cached in `LocaGlyf.cache` (FxHashMap), so after
warmup the boxed table is nearly never read again (that's why the
compressor swaps it: written once, cold).
→ Fix direction (allsorts-azul is our fork): store `Arc<[u8]>`/mmap +
`(offset, len)` ranges instead of owned boxes for `glyf` (and audit
`read_and_box_table`'s other callers — CFF, GPOS/GSUB…); glyph decode
reads ranges out of the mapping (page-cache-friendly). Saves ~21 MB
footprint in every text-rendering app; more when multiple large fonts
load (each font pays its own table copy today: CJK fonts would be
30–40 MB *each*).

**(b) ~37 MB GPU: WebRender `TextureCacheConfig::DEFAULT`.**
Azul never overrides it (no `TextureCacheConfig` reference outside
vendored webrender). Desktop Firefox defaults are sized for a browser.
→ Make it explicit config on `AppConfig` with azul defaults ~
{color8_linear: 1024, color8_glyph: 1024, alpha8: 512, alpha8_glyph:
1024, …} ≈ 7–10 MB, letting WR grow on demand (it allocates additional
shared textures when full; steady-state cost = occasional atlas
alloc). Mobile profile smaller still. Needs a scroll/zoom perf sanity
pass to confirm no atlas-churn regressions on the widgets demo.

**(c) 9.6 MB IOSurfaces** — 800×600@2× × RGBA × ~3 buffers. Inherent to
compositing; only lever is not over-allocating swapchain depth. Fine.

**(d) ~19 MB residual heap.** 41 MB live total − 21.9 MB glyf. Composition
from `heap`: 35.8 MB "non-object" Rust allocations (54 k nodes — biggest
single classes 608 KB ×4, then long tail ≤544 KB), ~1 MB CFString + ObjC
metadata. Suspects worth a follow-up `malloc_history -callTree` session:
parsed-font auxiliary tables (cmap/GPOS also boxed, same pattern as (a)),
WR glyph rasterizer staging, shader binary cache. Not itemized further in
this pass.

### 3.4 The video 1 GB bug: images are registered forever (**confirmed root cause**)

Data flow of one video frame (`layout/src/widgets/video.rs:396-425`):
decode worker → `video_writeback` → `ImageRef::new_rawimage` (fresh
`Box<DecodedImage>` ⇒ **`ImageRefHash` = the raw pointer address**,
`core/src/resources.rs:1135-1139`) → widget state swaps `current_frame`
(old `ImageRef` drops, its pixels are freed core-side — that part is
correct) → `trigger_all_virtual_view_rerender` → display list rebuilt →
`collect_image_resource_updates` (`dll/src/desktop/wr_translate2.rs:1227`)
sees an unknown hash → `build_add_image_resource_updates`
(`core/src/resources.rs:3096-3168`) **deep-copies the RGBA bytes** into
`AddImage`, inserts into `currently_registered_images` + `image_key_map`,
uploads to WebRender's resource cache.

And then: **nothing ever deletes it.**
- `ResourceUpdate::DeleteImage` exists and is translated
  (`wr_translate2.rs:1456`) but is **never constructed anywhere** in
  azul (only inside vendored webrender). WR's own doc: *"Must be matched
  with a DeleteImage at some point to prevent memory leaks"*.
- `currently_registered_images` / `image_key_map`: insert-only — no
  `remove`/`retain`/`clear` call sites at all.
- The designed GC exists as **dead code**: `RendererResources` doc
  promises `start_frame_gc`/`end_frame_gc` (`resources.rs:1266`) — the
  functions don't exist; `LayoutWindow::scan_used_images`
  (`layout/src/window.rs:2141`) computes exactly the live-set a sweep
  needs and has **zero callers**.
- The GL-texture path *does* have an epoch GC
  (`gl_texture_cache.rs:122-152`) — but video deliberately uses
  `DecodedImage::Raw`, which that GC never touches.

Arithmetic: 640×360 RGBA (the azul-video BBB demo) = 0.9 MB/frame ×
~30 fps ≈ **26 MB/s**; 1 GB in ~40 s. A 720p/1080p capture tile
(azul-meet/screenshare): 3.5–8 MB/frame ⇒ 1 GB in 4–10 s. Matches the
reported "video apps go to 1 GB". The same add-only registry also leaks
(slower) for *any* app that swaps `RawImage`s (map tiles on pan!, paint
canvas snapshots, capture tiles).

**Corollary (already on the books):** the open "capture tile repaint —
NullImage after ChangeNodeImage" bug is plausibly the *mirror* symptom of
the same root cause: hash = pointer ⇒ if the allocator **reuses** a freed
Box address that is still registered, the "already registered" check
(`resources.rs:3109-3114`) skips the upload and the tile shows the stale
old frame / placeholder.

**Fix design (for the follow-up implementation PR):**
1. End-of-frame sweep: after display-list submit, diff
   `scan_used_images(all live DOMs)` (already written!) against
   `currently_registered_images`; emit `DeleteImage` for the difference,
   drop from both maps. Natural home: the submit block at
   `wr_translate2.rs:1777-1810`. Debounce by N frames or epoch tag to
   avoid delete/re-add churn for images that blink out for one frame.
2. Stop keying by pointer: `ImageRefHash` should be a monotonically
   assigned u64 id (or content hash for cacheable images) minted in
   `ImageRef::new` — kills the ABA/stale-frame corollary independently
   of (1).
3. For the video path specifically, the *right* long-term shape is
   `UpdateImage` on a stable key (WR supports dirty-rect updates —
   no per-frame add+delete, no key churn, less texture realloc), or the
   external-texture path the GL cache already GCs.
4. Regression harness: `AZ_E2E_TEST` scenario swapping a RawImage every
   tick with `assert_growth_mib_max` ~5 MB / 500 iterations (the guide's
   machinery, `memory_breakdown: true` gains a
   `mgr_renderer_images` counter that must stay ~2, not grow) — the
   probe field already exists and read 0/1 in the headless runs.

### 3.5 Ranked memory actions

1. §3.4 image GC (fixes 1 GB class + stale-tile corollary; unblocks all
   image-swapping widgets: video, camera, maps, paint).
2. §3.3(a) glyf zero-copy (−21 MB every app, more per extra font; also
   shrinks the wasm-lift's ParsedFont story, see companion plan §3.2).
3. §3.3(b) texture-cache config (−10–30 MB, needs perf check; expose on
   AppConfig, pick mobile defaults).
4. Gate the `[PRUNE]`/`[CASCADE]`/`[MEM]` release logging (§3.2).
5. Investigate the ~55 KB/iter headless heap creep (low prio, bounded).
6. Follow-up pass on the ~19 MB residual heap (cmap/GPOS boxing audit).

---

## 4. Verification ledger

Claims in this report were verified as follows: archive bitcode
percentages re-derived with an independent COFF parser (58.5 % on
azul.lib) in addition to the member-extraction sampling; `libazul.so` ==
`azul.cpython.so` re-checked by md5; the 21.4 MB glyf allocation
backtraced live twice (`malloc_history` under `MallocStackLogging=lite`)
and source-confirmed in `allsorts-azul-0.16.5` (`Box::from(table.
into_owned())`); DeleteImage-never-constructed grep-verified across
core/layout/dll; dead links re-fetched with `--fail` retries; headless
RSS numbers from two separate runs. Raw data (bloaty CSVs, vmmap dumps,
JSONL probes, download log) in the session scratchpad; regenerate with
the commands quoted inline.
