# Layout-tree memory: allocation-strategy research and plan

Research date: 2026-08-06. Workload: 960-line markdown (~60k chars), 31,086 shaped
clusters. Measured: `LayoutTree` 16.2 MB, `warm.inline` 13.5 MB, process RSS ~121 MB
against a 20-30 MB target.

**Status: research only. No code changed. Nothing here has been implemented.**

Sourcing rule observed throughout: no other project's source code was read. All external
claims come from man pages, official documentation prose, papers, engineering blogs, and
issue-tracker prose. Claims are marked VERIFIED (with URL) or INFERRED. A ledger of what
could NOT be verified is in §9.

---

## 1. Framing: the layout tree is 13% of the problem

RSS is ~121 MB; `LayoutTree` is 16.2 MB; `warm.inline` is 13.5 MB. **Eliminating
`warm.inline` entirely takes 121 MB -> ~107 MB.** The 20-30 MB target is not reachable
from this workstream alone.

Two reasons the 16.2 MB figure is itself an undercount:

- `LayoutTree::memory_report()` (`layout/src/solver3/layout_tree.rs:893`) never counts
  `inline_content_cache`.
- It counts `c.text.capacity()` — the *requested* bytes. glibc's minimum chunk on x86-64
  is 32 bytes, so 31,086 one-byte strings cost ~1 MB of real RSS that the report scores
  as ~31 KB.

Summing all documented allocator rounding across the known object mix puts the **live heap
at roughly 8-12 MB against 121 MB RSS — a ~10x gap**. That is far outside anything
documented allocator overhead can produce: jemalloc bounds internal fragmentation at
"approximately 20%" ([jemalloc(3)](https://jemalloc.net/jemalloc.3.html)); mimalloc's
paper worst case across its whole suite is "up to 25% more"
([MSR-TR-2019-18](https://www.microsoft.com/en-us/research/wp-content/uploads/2019/06/mimalloc-tr-v1.pdf)).

**Conclusion: this is not an allocator-overhead problem.** Find the other ~90 MB before
optimizing structs. §7 gives the four cheap experiments that discriminate.

---

## 2. What the code actually shows (verified locally)

All four measured struct sizes reproduce exactly from the field declarations, which
confirms the model:

```
ShapedGlyph   =  96 B  kind 8 + glyph_id 2 + cluster_offset 4 + advance 4 + kerning 4
                     + offset 8 + vertical_advance 4 + vertical_offset 8 + script 1
                     + style Arc 8 + font_hash 8 + font_metrics 32   -> pad 96
ShapedCluster = 176 B  text String 24 + source_cluster_id 8 + source_content_index 8
                     + source_node_id Option<NodeId> 16 + glyphs SmallVec 104
                     + advance 4 + direction 1 + style Arc 8 + 3 bools 3  -> pad 176
ShapedItem    = 184 B  ShapedCluster + 8 B enum tag
PositionedItem= 200 B  ShapedItem 184 + position Point 8 + line_index usize 8
UnifiedLayout =  64 B  Vec<PositionedItem> 24 + OverflowInfo
```

Definitions: `layout/src/text3/cache.rs:4661` (ShapedCluster), `:4698` (ShapedGlyph),
`:4587` (ShapedItem), `:4796` (PositionedItem), `:2203` (LayoutFontMetrics).

### 2.1 The same text is retained in six places

`TextShapingCache` (`layout/src/text3/cache.rs:6108`) alone holds four:

| Field | Contents |
|---|---|
| `logical_items: HashMap<CacheId, Arc<Vec<LogicalItem>>>` | `LogicalItem::Text` owns a `String` copy via `to_string()` (`:7045`) |
| `visual_items: HashMap<CacheId, Arc<Vec<VisualItem>>>` | post-bidi representation |
| `shaped_items: HashMap<CacheId, Arc<Vec<ShapedItem>>>` | commented *"monolithic, for backward compat"* |
| `per_item_shaped: HashMap<u64, Arc<PerItemShapedEntry>>` | per-coalesce-group shaped results |

Plus two more on the layout tree:

| Field | Contents |
|---|---|
| `warm[].inline_layout_result` | `Vec<PositionedItem>` — a full **by-value** copy of every `ShapedCluster` |
| `warm[].inline_content_cache` | `Vec<InlineContent>` -> `StyledRun` text (uncounted by `memory_report`) |

Plus the `StyledDom`'s own node text. **This is the headline finding: the cost is
duplication, not allocator overhead.**

### 2.2 Per-glyph duplication

At the construction site (`layout/src/text3/cache.rs:8383`) every `ShapedGlyph` does
`style: g.style.clone()` — the same `Arc` its parent `ShapedCluster` already holds — and
copies 32 B of `font_metrics`. `LayoutFontMetrics` is in *font units*, so it is identical
for every glyph of a given font.

**48 of every 96-byte glyph (50%) is per-font/per-style data replicated per character**,
plus an atomic refcount increment per glyph on build and a decrement on drop
(~62,000 atomic ops per document).

### 2.3 Cheap structural waste

- `NodeId { inner: usize }` (`core/src/id.rs:71`) has **no niche**, despite its own doc
  comment describing a 1-based encoding where 0 means None. So `Option<NodeId>` costs
  16 B. A `NonMaxU32` newtype makes it 4 B.
- `LayoutFontMetrics` spends 16 of its 32 B on two `Option<f32>` (f32 has no niche, so
  8 B each for 4 B of payload). NaN sentinels make it 24 B.
- `InlineItemMetrics` (`layout/src/solver3/layout_tree.rs:180`, 40 B x 31,086 = 1.24 MB)
  duplicates `source_node_id`, `x_offset ~ position.x`, `line_index`, and
  `advance_width ~ cluster.advance`. Only `line_height_contribution` and `can_break`
  (5 B) are genuinely new.
- **The SmallVec inline slot is 104 of the 176 B cluster (59%).** Its inline arm is
  `max(sizeof(T), 16)`, so it is sized entirely by `ShapedGlyph`. Shrinking the glyph
  shrinks this too — see §8 P2.
- No `shrink_to_fit` is called on any retained layout/text vector (verified: the only
  matches in `text3/`+`solver3/` are the unrelated CSS shrink-to-fit sizing concept).

---

## 3. Q1/Q6 — why arena allocation is the wrong tool here

**The 31,086 `ShapedCluster`s are not individually heap-allocated.** They live inline
inside `Vec<PositionedItem>` — one contiguous allocation per IFC. **A `Vec` already is a
bump arena.** There is nothing for bumpalo to speed up or compact.

The only genuine many-small-allocations component is the 31,086 `String`s (~1 MB of glibc
chunk overhead) and rare SmallVec spills. So bumpalo's ceiling here is ~1 MB — and
deleting the `String` field recovers the same 1 MB *plus* 24 B of inline struct. Strictly
better.

The downsides are real and independently attested:

- **Zed built exactly this and backed it out.** GPUI 1 used bumpalo for elements, then
  abandoned it: *"every allocated value carries a lifetime that ties it to the arena in
  which it's allocated, and these lifetimes were adding a lot of complexity to the APIs."*
  GPUI 2 returned to arenas but wrote their own thread-local one enforcing safety
  *dynamically* rather than via lifetimes
  ([Zed Weekly #29](https://zed.dev/blog/zed-weekly-29)). A Rust GUI framework, same
  workload, tried it, reverted. **The most relevant precedent in this document.**
- **Drop is not run.** bumpalo: *"Objects that are bump-allocated will never have their
  `Drop` implementation called"*, and `Bump::reset` likewise
  ([docs](https://docs.rs/bumpalo/latest/bumpalo/struct.Bump.html)). Clusters hold
  `Arc<StyleProperties>` and `String`; leaking every Arc is unacceptable, and
  `bumpalo::boxed::Box` (non-default `boxed` feature) reintroduces per-object drop glue.
  typed-arena *does* run Drop but is single-type.
- **Incremental relayout breaks.** A bump arena reclaims only when everything dies. An
  arena per *document* would destroy the existing per-IFC reuse (`inline_content_cache` +
  fingerprints). If arenas are used at all, **per-IFC-root is the only correct
  granularity**, with `Bump::reset` (which recycles chunks) as the primitive for
  "rebuilt wholesale on change".

**Where an arena genuinely pays: teardown.** Dropping a document today runs 31,086 String
frees plus ~62,000 atomic Arc decrements. A representation with no per-cluster owned heap
makes `Vec<ClusterCompact>` trivially droppable — no Drop glue at all, one `free`. That
win comes from *removing owned heap from the struct*, not from adopting an arena crate.

---

## 4. Q3 — what the engines actually did (published prose only)

- **Gecko is the direct answer to this workload.** It stores **one 4-byte
  `CompressedGlyph` per character** for the simple case, with rare cases spilling to side
  `DetailedGlyph` records
  ([Bugzilla 671297](https://bugzilla.mozilla.org/show_bug.cgi?id=671297), prose
  discussion: the accounting is "character count multiplied by either 1 or 2 bytes, plus
  CompressedGlyph overhead"). Jonathan Kew in the same thread: *"I hit 75,000 [textruns]
  easily just by loading a couple of large, text-heavy tabs."*
  **Gecko spends 4 B/char where azul spends ~434.**
- **Chromium/Oilpan — compaction works.** Heap compaction cut Gmail's Blink heap
  **6.8 MB -> 2.3 MB** (~66%), shipped in Opera 39 beta
  ([Opera blog](https://blogs.opera.com/desktop/2016/07/memory-usage-opera-heap-compaction/)).
- **Chromium — handles beat pointers, measured.** Oilpan pointer compression (32-bit
  offsets into a heap cage instead of 64-bit pointers) gave **21% median / 33% p99
  (-59 MB) memory reduction on Windows**, 6%/8% on Android, plus **another 4% from field
  reorganization** in Chrome 108 ([v8.dev](https://v8.dev/blog/oilpan-pointer-compression)).
  Validates both halves of the plan below: small handles *and* struct packing.
- **Servo — flat arrays.** *"Boxes end up flattened into a flat list instead of a linked
  data structure, improving cache locality and memory usage, and the line breaking code
  need not traverse trees but only traverses a single array"* (Servo wiki, Layout Overview).
- **Firefox MemShrink** achieved ~30% overall reductions in its first releases
  ([Nethercote's blog](https://blog.mozilla.org/nnethercote/)), with `about:memory` as the
  enabling instrumentation — the methodological lesson being *measure attribution first*.

Could not verify: Flutter's approach, Blink `ShapeResult` layout, whether Servo ever
removed an arena.

---

## 5. Q2 — crates (verified against docs.rs / crates.io, Aug 2026)

**Key structural finding: bump allocators and handle arenas are different families, and
no bump allocator gives integer handles.** Every bumpalo/typed-arena/bump-scope API
returns `&mut T`.

| Crate | Ver / date | 90-day DL | Drop | Handle | no_std |
|---|---|---|---|---|---|
| bumpalo | 3.20.3 / 2026-05 | 130M | **No** (opt-in) | No | Yes |
| typed-arena | 2.0.2 / 2023-01 | 14.1M | Yes | No | Yes |
| la-arena | 0.3.1 / 2023-06 | 857K | — | **4 B** | **No** |
| slotmap | 1.1.1 / 2025-12 | 23.4M | Yes | 8 B | Yes |
| id-arena | 2.3.0 / 2026-01 | 54.2M | Yes | **16 B** | Yes |
| cranelift-entity | 0.134.3 / 2026-07 | 10.0M | Yes | **4 B** | Yes |

- **Generational indices are not needed here.** Clusters are never freed individually, so
  slotmap's 4-byte version field is pure overhead. `cranelift-entity` is the best
  structural match: `entity_impl!` (u32 newtype), `PackedOption` ("without additional
  space overhead"), and `EntityList` + `ListPool` ("much smaller footprint than `Vec`")
  for child lists.
- **`nonmax`** — documented guarantee that `Option<NonMaxU32>` is no larger than
  `NonMaxU32`, and unlike `NonZeroU32` index 0 stays valid (no +/-1 arithmetic).
- **`rkyv`: not recommended.** *"rkyv only supports non-cyclic data structures"* — fatal
  for a DOM with parent links. Archived form is read-only (`Seal` only overwrites
  fixed-size scalars). And you must materialize the real value first, so **peak RSS gets
  worse**. It earns its keep only if you persist or transmit the tree.
- **`blink-alloc`: claims don't hold.** Its 3.83x figure has no CPU spec, no date, no
  versions, and dates to a 2023 bumpalo. bumpalo's own suite measures blink-alloc
  1.19-1.21x *slower*; third-party `stumpalo` benchmarks agree. No functional development
  since 2023.
- **`compact_str` 0.10.0 — 24 B, same as `String`, 24 bytes inline, zero heap.** The
  drop-in for `ShapedCluster.text`. `ecow`/`lean_string` are 16 B with 15-16 inline and
  O(1) CoW clone.
- **SmallVec floor is 3 words = 24 B**, not 16 — the heap arm must carry ptr+cap.
  `union` is not default in any 1.x (azul already enables it; cargo unification makes it
  global). Inline capacity 1 is strictly dominated: the arm is `max(sizeof(T), 16)`.

Gotchas found: **smallvec 1.15.2 breaks under `--all-features`** (missing `.natvis` in the
published tarball; 1.15.1 is fine) — and azul pins both `1.15.1` and `1.13` in different
manifests. `slotmap::HopSlotMap` is deprecated for 2.0. `generational-arena` is archived.
`AzString` (`css/src/corety.rs:179`) is `repr(C)` and FFI-locked — it cannot become an SSO
type; only internal Rust types are swappable.

---

## 6. Q4 — Vec capacity and shrink_to_fit

- **`with_capacity`/`reserve_exact` are worth it and documented:** *"all of `vec![...]`,
  `vec![x; n]`, and `Vec::with_capacity(n)` produce a `Vec` that requests an allocation of
  the exact size needed for precisely `n` elements ... and no other size (such as, for
  example: a size rounded up to the nearest power of 2)"*
  ([Vec docs](https://doc.rust-lang.org/std/vec/struct.Vec.html)). But `reserve_exact`
  also warns *"capacity can not be relied upon to be precisely minimal."* Growth strategy
  is explicitly **unspecified** — only amortized O(1) push is guaranteed.
- Current strategy (VERIFIED from issue prose, not source): factor 2, with minimum
  non-zero capacity 8/4/1 by element size
  ([#111307](https://github.com/rust-lang/rust/issues/111307),
  [#72227](https://github.com/rust-lang/rust/pull/72227)). Final capacity lands in
  `[N, 2N)`; expected excess ~28-31% of the buffer (INFERRED arithmetic).
- **`shrink_to_fit` does not guarantee `capacity() == len()`:** *"may either shrink the
  vector in-place or reallocate ... might still have some excess capacity."*
- **Under glibc it is a no-op for small allocations.** Splitting a chunk requires the
  remainder to be >= MINSIZE (32 B). Shrinking a 112-byte chunk to 96 leaves 16 bytes —
  too small to split, so glibc does nothing (INFERRED from the verified MINSIZE).
  It *does* work for the multi-MB `Vec<PositionedItem>` buffers, which exceed
  `M_MMAP_THRESHOLD` (128 KiB) and are therefore mmap'd.
- `warm_inline_layout_bytes` uses `items.capacity()`, so doubling slack is already inside
  the 13.5 MB. 13.5 MB / 31,086 = **434 B/cluster against only ~240 B accounted at `len`**
  (200 PositionedItem + 40 InlineItemMetrics), pointing at ~1.8x capacity slack across
  many per-IFC vectors. **Measure `len` vs `capacity` before acting.**
- Better tool for the retained vectors: `into_boxed_slice()` shrinks *and* drops the
  header from 24 B to 16 B.

---

## 7. Q5 — allocator and OS knobs

### 7.1 glibc: the "top of main arena only" premise is outdated

That is true of *automatic* trimming on `free()`, but
[malloc_trim(3)](https://man7.org/linux/man-pages/man3/malloc_trim.3.html) states
verbatim: *"**Since glibc 2.8 this function frees memory in all arenas and in all chunks
with whole free pages**"*, via `sbrk` **or `madvise`**. Returns 1 if anything was released.

Precedent: [Algolia](https://www.algolia.com/blog/engineering/when-allocators-are-hoarding-your-precious-memory)
had one arena holding *"901 blocks, for more than 15GB"* unreclaimed; an explicit
`malloc_trim(0)` took the process **60 GB -> 20 GB**.

Also: **default arena count on 64-bit is 8 x cores** (32+ on a quad-core), each with its
own independently-trimmed top chunk.
[codearcana](https://codearcana.com/posts/2016/07/11/arena-leak-in-glibc.html) documents
*"64 malloc arenas that were using only ~1% of about ~200MB."* `MALLOC_ARENA_MAX=2` is
what Heroku defaults new apps to — an env var, zero code change.

Overhead table (VERIFIED via secondary prose; formula `chunk = max(32, (req + 8 + 15) & ~15)`):

| request | glibc chunk | waste |
|---|---|---|
| **1 B** | **32 B** | **31 B (96.9%)** |
| 24 B | 32 B | 8 B |
| 96 B | 112 B | 16 B |
| 200 B | 208 B | 8 B |

jemalloc has **no per-object header at all** — Jason Evans: *"less than two bits per
allocation even for e.g. 8-byte objects"*. Its smallest size class is 8 B (4x better than
glibc for the 1-byte strings), but its 224 B class loses to glibc's 208 B for
`PositionedItem`.

### 7.2 jemalloc idle trap

`tikv-jemallocator` ships `background_threads` **DISABLED** by default, with
`dirty_decay_ms:10000`, `muzzy_decay_ms:0`. **Without background threads, decay only
advances when the app calls back into the allocator** — so if the document is freed and
the GUI goes idle, nothing ticks the clock and RSS never drops. That matches the observed
symptom. Set `background_thread:true` if jemalloc is used anywhere.

### 7.3 MADV_DONTNEED vs MADV_FREE

[madvise(2)](https://man7.org/linux/man-pages/man2/madvise.2.html), verbatim:

- **`MADV_DONTNEED`**: *"The resident set size (RSS) of the calling process will be
  immediately reduced."*
- **`MADV_FREE`**: *"The kernel can thus free these pages, but the freeing could be
  delayed until memory pressure occurs."*

So **MADV_FREE will not reliably show up as an RSS win.** Go 1.12 adopted it and Go 1.16
reverted precisely for this reason ([Go 1.16 release notes](https://go.dev/doc/go1.16):
*"process-level memory statistics like RSS will more accurately reflect the amount of
physical memory being used"*).

### 7.4 Transparent Huge Pages

Kernel docs confirm the bloat mechanism verbatim: *"an application may mmap a large region
but only touch 1 byte of it, in that case a 2M page might be allocated instead of a 4k page
for no good"*
([transhuge.rst](https://docs.kernel.org/admin-guide/mm/transhuge.html)). THP also defeats
purging — THP's own author on LKML: *"`MADV_DONTNEED` doesn't necessarily free anything
when applied to a THP subpage"* and *"can be undone by khugepaged."*

Go's GC guide on this exact heap size: *"**Applications with small heaps tend not to
benefit from THP and may end up using a substantial amount of additional memory (as high
as 50%)**."* Reported cases: VoltDB 21 GB -> 50 GB; jemalloc#1127 "RSS is 50% higher than
what jemalloc itself reports"; golang/go#64332 4.4 GB RSS vs 300-400 MB profiled.

Check with `grep AnonHugePages /proc/<pid>/smaps_rollup`. Disable per-process with
**`prctl(PR_SET_THP_DISABLE)`** — no root needed, inherited across fork/exec.
**Note mimalloc allows THP by default on Linux.**

### 7.5 Do NOT swap allocators for this workload

The mimalloc tech report's **`cfrac`** benchmark is described by its own authors as *"many
small short-lived allocations - exactly the workload we are targeting."* On it, **glibc
ties or beats mimalloc (0.96-1.00 relative RSS) and jemalloc is 1.41x worse.**

Corroborating: [mimalloc#1111](https://github.com/microsoft/mimalloc/issues/1111) — Arrow
Parquet peak RSS, mimalloc 2.2.4 **regressed 22-27%** vs 2.0.6 and was worst of four;
glibc won the largest case. rustc rejected mimalloc over exactly this
([rust-lang/rust#147580](https://github.com/rust-lang/rust/issues/147580): *"max-rss was
very much what was blocking us from mimalloc"*).

Net effect on this object mix: **roughly +/-1 MB.**

### 7.6 `#[global_allocator]` does not cover C dependencies

Per [std::alloc docs](https://doc.rust-lang.org/std/alloc/index.html), `cdylib`s and
`staticlib`s *"are guaranteed to use the `System` [allocator] by default."*
**FreeType/HarfBuzz/image codecs keep using libc malloc regardless.** Since fonts are a
prime suspect for the missing ~90 MB, an allocator swap cannot even reach that memory
without `LD_PRELOAD`.

If mimalloc is ever adopted: enable `local_dynamic_tls`, because a statically-linked
mimalloc inside a `dlopen`ed shared library fails with
`initial-exec TLS resolves to dynamic definition` — and azul ships `libazul.so` and is
`LD_PRELOAD`ed on this machine.

---

## 8. Prioritized plan

### P0 — Measure attribution (hours, zero code)

Four experiments, cheapest first. Any one of them could account for the 10x gap, and all
are cheaper than any struct refactor.

1. `grep -E '^(Rss|Anonymous|Private_Dirty|AnonHugePages):' /proc/<pid>/smaps_rollup` at
   peak. **If it is not Anonymous/Private_Dirty, it is not the heap** (mapped fonts, GPU
   driver, `.so` text) and no allocator work will touch it.
2. `malloc_trim(0)` immediately after the bulk free; check the return value.
3. `MALLOC_ARENA_MAX=2`.
4. THP check per §7.4.

Then, if still unexplained: `tikv-jemalloc-ctl` logging `allocated/active/resident/mapped/
retained` at peak and after free (**advance `epoch` first or the numbers are stale**). If
`resident >> allocated` it is fragmentation/retention and no malloc-bytes profiler will
explain it. If `resident ~ allocated ~ 121 MB`, the struct work below is the whole answer.

Note `dhat` measures malloc-*requested* bytes, not RSS. `heaptrack -H` gives the
allocation-size histogram. jemalloc heap profiling defaults to `lg_prof_sample` = 512 KiB,
so the 31,086 tiny strings (~124 KB total) would be sampled **zero times** — set
`lg_prof_sample:0`.

Also measure `len` vs `capacity` on the retained per-IFC vectors (§6).

### P1 — Stop storing the same text six times (days, biggest win, no new deps)

Make `PositionedItem` reference the shaped cluster instead of embedding it by value, and
delete the `shaped_items` cache already labelled *"for backward compat"*. This is a
multiplier on everything below. Aligns with Oilpan's measured 66% compaction win and
Servo's flat-list rationale.

### P2 — Shrink `ShapedGlyph` 96 B -> ~16 B (days, ~4 MB)

Replace `font_metrics` (32) + `font_hash` (8) + `style: Arc` (8) with one `run_id: u16`
into a per-run table. **This pays twice:** the glyph shrinks *and*
`SmallVec<[ShapedGlyph; 1]>` collapses from 104 B to 24 B, because its inline arm is
`max(sizeof(T), 16)`. Roughly **-128 B/cluster ~ 4 MB**, and it removes ~62,000 atomic
refcount ops per document. Highest bytes-per-unit-complexity in this document.

### P3 — `text: String` -> `CompactString` (one line, ~1 MB, zero risk)

Same 24 B, 24 bytes inline, no malloc. Eliminates 31,086 malloc/free pairs. Deleting the
field outright saves 56 B/cluster (~1.7 MB) since `source_cluster_id` already carries the
start byte offset — but `CompactString` is the zero-argument version that can land today.

### P4 — Cheap struct fixes (~0.6 MB, hours)

`NodeId(NonMaxU32)` so `Option<NodeId>` goes 16 B -> 4 B; `LayoutFontMetrics`'s two
`Option<f32>` -> NaN sentinels (32 -> 24 B); drop the redundant `InlineItemMetrics` fields.

### P5 — Capacity tuning on the LARGE vectors only (hours, possibly ~3 MB)

`reserve_exact` up front (character count is known before shaping) and `shrink_to_fit` /
`into_boxed_slice` on the multi-MB retained buffers. **Not** on per-cluster allocations —
that is a no-op under glibc (§6). Gated on the P0 `len` vs `capacity` measurement.

### P6 — Only if P1-P5 fall short: the Gecko representation

A ~20 B/cluster packed record (glyph_id, advance, x_offset, start_byte, run_id, flags)
with a side table for the <1% complex cases, taking 13.5 MB -> ~0.65 MB. Gecko does this
at 4 B/char, so there is headroom. High complexity — the destination, not the next step.

### Explicit non-goals

- **Do not adopt a bump allocator for this.** Data is already in `Vec`s; bumpalo will not
  run `Drop` on the `Arc`s and `String`s; per-document arenas break per-IFC incremental
  reuse; Zed hit the lifetime wall doing precisely this and reverted (§3).
- **Do not swap allocators as a memory play.** Published evidence for this exact
  allocation profile says +/-1 MB (§7.5).
- **Do not use rkyv** as the in-memory representation — cycles are unsupported and peak
  RSS gets worse (§5).

---

## 9. Ledger: what could NOT be verified

| Claim | Status |
|---|---|
| glibc MINSIZE=32 / 16-byte align / request2size | VERIFIED via secondary prose only (how2heap, Mechpen, Azeria, openEuler) — source not read per the licensing rule. Consistent across all four. |
| Rust `max(cap*2, required)` and min caps 8/4/1 | VERIFIED via issue/PR discussion prose. std explicitly guarantees *nothing* here. |
| mimalloc's actual bin/size-class list | NOT VERIFIED. mimalloc#573 asks; no visible maintainer answer. |
| `mi_option_purge_delay` default (10 vs 1000) | CONFLICTING OFFICIAL DOCS. Reconciliation (10 for v1/v2, 1000 for v3) is INFERRED. |
| `MALLCTL_ARENAS_ALL` numeric value | NOT VERIFIED — symbolic only in the man page. Widely reported as 4096. |
| khugepaged `max_ptes_none` default | NOT VERIFIED from kernel docs. Third parties say 511; go#64332 says 512. |
| glibc version for the malloc_trim all-arenas change (2.8 vs 2.9) | NOT VERIFIED — man page says 2.8; a secondary source says a Dec-2007 commit shipping in 2.9. |
| Whether `MADV_NOHUGEPAGE` blocks khugepaged | NOT VERIFIED directly; strongly implied by two verified facts. |
| glibc `malloc_trim` being defeated by THP | INFERRED only — no glibc-authored statement. |
| Current snmalloc purge policy | NOT VERIFIED — ISMM'19 paper says it withholds memory until pressure; no doc says that changed. |
| `heaptrack` reporting RSS | NOT VERIFIED — no documentation found. Treat as a requested-bytes tool. |
| `bytehound`'s `show_rss` semantics | NOT VERIFIED. Crate is dormant (last release 2022-11). |
| Flutter / Blink ShapeResult / Servo arena history | NOT VERIFIED — no prose source found, and source reading was out of bounds. |
| Struct sizes of la-arena / id-arena / cranelift-entity handles | INFERRED from documented API; no crate publishes a numeric `size_of`. |
