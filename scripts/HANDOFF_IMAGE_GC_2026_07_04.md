# Handoff — image / GPU-texture garbage collection (the 1 GB video leak)

**Status: IMAGE GC IMPLEMENTED (commit `df222b9`).** The aliasing prerequisite
(§2d) and the image GC (Option C ids + Option B epoch delete, §4) are done and
unit-tested headlessly (`dll/tests/image_lifecycle.rs::
stale_image_is_deleted_after_retention_window`). This document is kept as the
design rationale + the remaining work:

- **Still open — FONT GC.** `FontRef` now has aliasing-safe ids too, but
  `DeleteFont`/`DeleteFontInstance` are still never emitted (fonts leak, just
  far more slowly — few, stable fonts vs. thousands of video frames). The
  `RendererResources.last_frame_registered_fonts` field is the intended
  two-frame-delayed delete hook; wire the same epoch-diff there.
- **Still recommended — windowed verification.** The unit test proves the GC
  *logic* (retention window, single DeleteImage, 3-map eviction). The §5
  windowed, GPU-instrumented, 4-backend pass is still the right final gate
  before trusting the numbers in production — a headless session can't watch
  RSS/GPU memory plateau or confirm no on-screen image ever flickers.

Original framing (kept for context): the single largest runtime-RSS problem
in the audit (§3.4), on the hot render path.

Written 2026-07-04. Line numbers are against commit `52f7af8`-era `master`
(the size-diet branch). Re-grep before trusting any `file:line`.

---

## 1. The symptom

A window that swaps images every frame — a video widget, a camera/capture
tile, an animated chart, a map that re-tiles on pan — grows unboundedly in
both **host RSS** and **GPU/driver memory** and never shrinks. A 1080p RGBA
frame is ~8 MB; at 30 fps that is ~240 MB/s of images that are uploaded to
WebRender and **never freed**. A few seconds of video = >1 GB resident.

This is a leak, not a cache: memory grows with *time*, not with the number of
*distinct* images on screen.

## 2. Root cause — three cooperating "add, never remove" registries

Adding an image walks: decode → `ImageRef` → `build_add_image_resource_updates`
→ `AddImage` → WebRender `add_image`, and registers it in two azul-side maps.
**Every step has an add path and no remove path.**

### 2a. `ResourceUpdate::DeleteImage` is translated but never constructed

- `core/src/resources.rs:2707` — the `DeleteImage(ImageKey)` variant exists.
- `dll/src/desktop/wr_translate2.rs:1485` — it is *translated* to
  `WrResourceUpdate::DeleteImage` … **if** an azul-side `DeleteImage` ever
  reaches the translator.
- **Nothing anywhere constructs `ResourceUpdate::DeleteImage`.** (grep:
  `ResourceUpdate::DeleteImage` / `DeleteImage {` across `core/ layout/ dll/`
  minus `webrender/` and the one translate arm → zero hits.)

So WebRender is told to `AddImage` forever and `DeleteImage` never. Every
uploaded texture stays in WR's texture cache for the life of the document.

### 2b. `currently_registered_images` is insert-only

- `core/src/resources.rs:1271` — `currently_registered_images:
  OrderedMap<ImageRefHash, ResolvedImage>`.
- Mutations: `.get` (1248), `.get_mut` (1256), `.insert`
  (`resources.rs:3194`, `wr_translate2.rs:3104`). **No `.remove`, `.retain`,
  `.clear`.**
- The reverse map `image_key_map` (`wr_translate2.rs:1825/3113`) is likewise
  insert-only.

Even if WR freed the texture, azul would keep the `ResolvedImage` (and its
CPU-side bytes for some image types) resident and would keep thinking the key
is live.

### 2c. The GC that was meant to do this is dead code

- `layout/src/window.rs:2141` — `scan_used_images(&self, _css_image_cache) ->
  BTreeSet<ImageRefHash>` computes the live set … and has **no real caller**
  (only a comment in `solver3/mod.rs:484` explaining what *would* go wrong).
  Note the `_css_image_cache` param is already ignored — it is a stub.

There is a *separate*, working epoch GC for GL textures in
`dll/src/desktop/gl_texture_cache.rs:122` (`remove_old_epochs`) — that one is
wired and fine. It governs the compositor's own render targets, **not** the
user-image resource cache. Don't confuse the two; the leak is in the resource
cache, not the compositor targets.

## 2d. CRITICAL: the leak currently *masks* a latent aliasing bug

Verified by reading `ImageRef` (`resources.rs:853`): `data: *const
DecodedImage` is refcounted via `copies: *const AtomicUsize` and **freed the
moment the last ref drops** (`into_inner` / the `Drop` impl). `ImageRefHash =
data as usize` (`:1135`) and the registered-image lookup in
`build_add_image_resource_updates` (`resources.rs:3096`) skips `AddImage` for
any hash already in `currently_registered_images`.

Chain the two facts together:

> today nothing is ever deleted, so the heap only grows and a freed image's
> address is (almost) never handed back out → pointer-derived hashes stay
> unique in practice → the skip-if-registered check is safe **only because of
> the leak**.

The instant a GC frees a texture + its registry entry, the backing `data`
box is freed and the allocator is free to hand that exact address to the
**next** decoded image. That new image now hashes to the **same**
`ImageRefHash` as the just-evicted one. Two ways it then goes wrong:

1. If the stale registry entry hasn't been evicted yet:
   `build_add_image_resource_updates` sees the hash as "already registered",
   emits **no** `AddImage`, and the new image renders with the **old
   image's texture** — silent visual corruption, not a crash.
2. If it was evicted: fine — until the *same* address is reused a second
   time within one frame, at which point two logically-distinct images share
   one key.

**Consequence for implementation:** you cannot land *any* of the GC options
in §4 (A or B) on top of pointer-derived keys — doing so trades a memory leak
for intermittent wrong-image rendering that only shows up under allocator
reuse (i.e. exactly the video/capture workload the GC targets, once the leak
that hid it is gone). **Option C is not optional; it is a prerequisite.** Do
C, prove rendering is unchanged, *then* add B.

## 3. Why `ImageRefHash` makes this subtle (read before designing)

- `core/src/resources.rs:1135` — `image_ref_get_hash(ir).inner = ir.data as
  usize`, i.e. **the raw heap pointer** of the `ImageRef`'s data.
- `image_ref_hash_to_image_key` (1145) turns that pointer straight into the
  WR `ImageKey`.

Consequences you must respect:

1. **Freeing an `ImageRef` frees the pointer** that *is* the hash/key. If the
   allocator reuses that address for a *new, different* image, the new image
   collides with the freed key → you'd `AddImage` onto a key WR thinks it
   already has, or `DeleteImage` a key that's been silently reassigned. A
   naive "delete when refcount hits zero" therefore has a use-after-free-style
   **hash aliasing** hazard. The GC must delete the WR key *and* evict both
   azul maps **before** the `ImageRef` backing that pointer can be reallocated,
   or move off pointer-derived keys entirely (see §4, option C).
2. Two live `ImageRef`s to the *same* decoded bytes share a hash today only if
   they share the `data` pointer. Refcounting must be per *pointer*, not per
   logical image.

## 4. Design options (pick with the person who has a display)

### Option A — mark-and-sweep per frame (matches the dead `scan_used_images`)
Each frame, after layout, compute the live set with a *real*
`scan_used_images` (walk every `StyledDom`'s display list + the CSS image
cache + widget datasets like `video::current_frame`), diff against
`currently_registered_images.keys()`, and for every key in registry−live:
emit `ResourceUpdate::DeleteImage`, then `currently_registered_images.remove`
+ `image_key_map.remove`.
- **Pro:** conceptually simple; reuses the intended architecture.
- **Con:** a full scan every frame; must run *after* the pointer is still
  valid but the image is off-screen — ordering vs §3.1 is the whole game.
- **Blocker to fix first:** `scan_used_images` currently ignores
  `_css_image_cache` and has no caller. It must become correct *and* complete
  (miss a source → you free an on-screen image → flicker/black tile).

### Option B — deferred delete queue (epoch-style, like the GL cache)
Tag each registered key with the last epoch it was seen live. Each frame,
delete keys not seen for N epochs (N≥2 for double-buffer safety, mirroring
`remove_old_epochs`'s "current + previous" rule). Keeps the `ImageRef` alive
in a side table for those N epochs so the pointer can't be reused underneath
the key (defuses §3.1).
- **Pro:** amortized, no per-frame full diff; naturally handles a frame that
  briefly drops then re-adds an image (video pause).
- **Con:** holds up to N epochs of images — bounds the leak, doesn't zero it.

### Option C — stop using raw pointers as keys (do this regardless)
Give `ImageRef` a process-unique monotonic id (an `AtomicU64` counter) and
derive `ImageRefHash`/`ImageKey` from *that*. Removes the aliasing hazard in
§3.1 entirely and makes A or B safe to implement. This is the enabling
refactor; A/B are unsafe without it.

**Recommendation:** C first (unblocks safety), then B (bounded, cheap, matches
the working GL-cache pattern). A only if a hard zero-growth guarantee is
needed and the full-scan cost is acceptable.

## 5. Verification plan (the reason this can't be done headless)

1. **Repro harness:** a windowed example that `WriteBack`s a fresh
   `RawImage`/`ImageRef` into a `video`-style widget every frame for ~600
   frames. (`layout/src/widgets/video.rs:63` `current_frame` +
   `video_writeback` is the exact pattern.)
2. **Instrument:** log `currently_registered_images.len()`,
   `image_key_map.len()`, process RSS, and — critically — GPU memory
   (`Metal`/`nvidia-smi`/`intel_gpu_top`) each frame.
3. **Pre-fix baseline:** all four grow linearly → confirms the leak.
4. **Post-fix pass condition:** registry sizes and RSS plateau within N frames
   and stay flat for the rest of the run; **on-screen image never flickers,
   blacks out, or corrupts** (the failure mode of freeing a live key, §4-A).
5. **Aliasing stress:** rapidly free + allocate images so the allocator reuses
   addresses; assert no wrong-image renders (guards §3.1 / validates option C).
6. Run on **all four backends** (macOS/Metal, X11, Wayland, Windows) — texture
   lifetime and the `DeleteImage` round-trip differ per compositor.

## 6. Blast radius / do-no-harm notes

- Freeing a key that is *still referenced* → black/blank tile or a WR panic on
  next use. This is worse than the leak. Hence the "verify no flicker" gate.
- The GL-texture epoch GC (`gl_texture_cache.rs`) is **correct and unrelated** —
  do not touch it while fixing the resource cache.
- Any change here must keep the `ImageRefHash` → `ImageKey` round-trip lossless
  on 32-bit (`resources.rs:1143` note) if you keep pointer/id in `usize`.

---

*Cross-refs:* audit §3.4 in
`scripts/RELEASE_SIZE_MEMORY_AUDIT_2026_07_04.md`; the working epoch-GC
precedent is `dll/src/desktop/gl_texture_cache.rs:122`.
