# azul-core bug audit — 2026-07-08

Read-only audit of the `azul-core` crate by 5 parallel scans (unsafe/FFI/refcount,
resources/caching, dom/diff/id, parsers, events/hit-test/geom). Findings are
severity-ranked with `file:line`, the trigger, and a suggested fix. Nothing was
modified. ~70 findings; the CRITICAL/HIGH set is the shortlist to fix.

Legend: 🔴 crash/UB/DoS · 🟠 memory leak / silent-wrong · 🟡 correctness · ⚪ latent.

---

## CRITICAL — trivially-triggered crash / hang / UB

- 🔴 **`path_parser.rs:354-489`** — **infinite loop (100% CPU) on `"M0 0Z5"`** (or `Z` + any digit/symbol): after `Z`, a non-command byte re-derives `cmd=Z`, the `Z` arm consumes 0 bytes, cursor never advances. Fix: reject `Z` reached via `last_command` fallthrough.
- 🔴 **`refany.rs:910-1017`** — `downcast_ref`/`downcast_mut` do a **non-atomic check-then-increment**; since `RefAny: Sync`, two threads both pass `can_be_shared_mut()` and hand out aliasing `&mut` to the same memory → UB. Fix: `compare_exchange` acquisition (as `replace_contents` already does).
- 🔴 **`gl.rs:3201-3211/3391/3443`** — `Texture`/VAO/VBO `Drop` write `run_destructor=false` but **never check it**, so the FFI double-drop is a **use-after-free** (`fetch_sub` on an already-freed refcount box) + double `delete_textures`. Fix: early-return when `!run_destructor` (mirror `GlContextPtr::drop`).
- 🔴 **`gl.rs:743, 753-848`** — `static mut ACTIVE_GL_TEXTURES` accessed via unsynchronized `as_mut()`/`as_ref()` → data race + `&mut`-aliasing UB (hard error on edition 2024). Fix: `Mutex`/`OnceLock` or raw-ptr accessors.

## HIGH — memory-unsafety, unbounded leaks, stack-overflow DoS

**Unsafe / refcount**
- 🔴 `refany.rs:591/603` — `unsafe impl Send/Sync for RefAny` is **unconditional in `T`**; a `!Send` payload moved cross-thread races its own internals.
- 🟠 `refany.rs:1036-1056` — `get_type_id_static` folds **only 8 of `TypeId`'s 16 bytes** → collisions permit a wrong-type `downcast` (UB). This u64 is the *only* FFI type guard. Fix: hash all 16 bytes.
- 🟠 `refany.rs:1264-1303` — `replace_contents` `mem::forget(new_value)` **leaks its `RefCountInner` box + data block** every call. Fix: manually reclaim both without running the T-destructor.

**Resources / caching**
- 🟠 `resources.rs:1414` — **fonts/font-instances are never GC'd** (`remove_font_families_with_zero_references` has zero callers; `DeleteFont*` never constructed) → unbounded WebRender font-memory leak on font cycling. Fix: mirror the image GC.
- 🔴 `resources.rs:1792` — `normalize_u16` computes `(65535/i)*255` — **inverted + divide-by-zero**; every 16-bit image decodes to near-white garbage. Fix: `(i/65535*255)` / `(i>>8) as u8`.
- 🟡 `styled_dom.rs:1404/1426` — `StyledDom::restyle(css)` **never rebuilds `compact_cache`** and assigns tag_ids from the stale cache → restyle silently no-ops for layout-hot props + wrong hit-test map. Fix: `compact_cache=None` + rebuild + regenerate tag_ids after inheritance.

**DOM / diff / parsers — unbounded recursion → stack overflow**
- 🔴 `diff.rs:377/895` — `calculate_reconciliation_key`/`_contenteditable_key` recurse the parent chain, **no depth cap, no visited-set** → deep-DOM overflow / cycle infinite-loop (once per node). Fix: iterative, bounded by node count.
- 🔴 `xml.rs:5783/5626/366/4886` — DOM-build + resource-scan recurse per nesting level → stack overflow on deeply-nested markup. Fix: depth cap or worklist.
- 🔴 `xml.rs:630-655` — `extract_css_urls` `@import` loop slices the **original** string with a **lowercased-temp byte offset** → panic on Unicode whose lowercase changes byte length (`İ`) + O(n²). Fix: index into the same lowercased string.
- 🔴 `id.rs:293-316` — `get_parents_sorted_by_depth` indexes `internal[0]` on an **empty hierarchy → panic**; also mislabels a childless root as a parent. Fix: `if is_empty { return }`.

## MEDIUM

**Events / hit-test / geom**
- 🟡 `events.rs:3124` — `get_first_hovered_node` **ignores z-order (`hit_depth`)**, picks lowest NodeId → clicks target the back-most node under overlaps. Fix: pick min `hit_depth`.
- 🟡 `geom.rs:87-108` — `contains()` (left/top-inclusive) and `hit_test()` (all-edges-exclusive) **disagree on edge pixels** → missed/duplicated edge hits.
- 🟡 `geom.rs:130/271` — raw `#[derive(PartialEq)]` vs **quantized `Ord`/`Hash`** → `a==b` false while `cmp==Equal`; breaks map lookups.
- 🔴 `events.rs:1497` — resize detection uses raw `==` on `LogicalSize` → **`NaN != NaN` emits a `Resize` every frame forever**. Fix: quantized/tolerance compare + NaN guard.
- 🔴 `events.rs:831` — `get_dom_path` walk has **no cycle guard** → infinite loop/OOM on corrupt hierarchy (runs every dispatch).
- 🟡 `events.rs:2460` — `Click` maps to `HoverEventFilter::LeftMouseDown` → synthesized clicks fire MouseDown + duplicate.
- 🟡 `drag.rs:720-740` — `ScrollbarThumbDrag` remap not scoped to a DOM; `NodeDrag.previous_drop_target` left stale after reconciliation → cross-DOM drag corruption.
- 🟡 `selection.rs:312/382` — `merge_overlapping` `sort_by(start_pos)` breaks the "primary = last-added" invariant → wrong node for scroll-into-view/IME.
- 🔴 `task.rs:237-312` — `Instant`/`Duration` mixed-kind (`System` vs `Tick`) ops `panic!`/`unreachable!` → hard-crash the event loop. Fix: saturate.
- 🟡 `hit_test.rs:293-306` — `ScrollState` clamps to full content size, not `content − viewport` → content scrolls fully out of view.
- 🟡 `events.rs:3168` — Ctrl+Click multi-cursor uses `ctrl_down()` not `primary_down()` → broken/misfires on macOS.

**DOM / diff / resources**
- 🟡 `diff.rs:524-540` — keyless Tier-2/3 matching has **no positional constraint** → focus/scroll/dataset state migrated to an unrelated identical node.
- 🟡 `compact.rs:574-591/805` + `id.rs:219` — inheritance/first-child silently **assume pre-order arena**, unvalidated → wrong inherited style / wrong traversal on non-pre-order trees; panics on forward/out-of-bounds parent refs.
- 🟡 `diff.rs:958-1011` — `reconcile_cursor_position` returns byte offsets that may **not be char boundaries** → later `str` slice panic.
- 🟡 `gpu.rs:154/263` — stale GPU transform/opacity keys leak when the DOM shrinks (loop bounded by new node count never emits `Removed`).
- 🟡 `styled_dom.rs:1516/1886/727` — `restyle_nodes_state`/`get_html_string`/`subtree_len` panic on stale NodeIds / single-node DOM / malformed FastDom (missing bounds/saturating guards).

**Unsafe / FFI (panic-across-boundary)**
- 🔴 `refany.rs:663-681/1006`, `host_invoker.rs:385-462`, `gl.rs:2874/3894` — a panic in `T::Drop` / default-ret expr / GL step **unwinds across `extern "C"`** = UB. Fix: `catch_unwind`/abort-guard.
- 🔴 `transform.rs:836-857` — `linear_combine_avx8` forms a `&__m128` (align 16) into an align-4 `[f32;4]` field → misaligned-reference UB. Fix: `loadu`.
- 🔴 `gl.rs:120/157/…` — `from_raw_parts`/`from_utf8_unchecked` on a possibly-**null** ptr with `len==0` from FFI = UB. Fix: empty-slice on null/0.
- 🟡 `xml.rs:1420`, `json.rs:648` — `ComponentFieldType::parse` (`Option<Option<…>>`) + `jq_all_recursive` unbounded recursion on attacker strings → stack overflow.

## LOW / latent (summary)

`gl.rs:3694` GlShader Drop no run_destructor guard · `transform.rs:21` SIMD flags no `is_x86_feature_detected!` gate (SIGILL risk) · `refany.rs:1107` set_*_fn racy writes to shared inner · `resources.rs:3227` forward/reverse image map divergence · `resources.rs:2182` load_bgra8 premultiplied path skips length guard · `prop_cache.rs:724` resolved_font_sizes not invalidated on restyle (dormant) · `styled_dom.rs:491/1227/1274` font-family hash no len-prefix / append_child inheritance not auto-recomputed / dead sibling branch · `id.rs:144/dom.rs:170` unchecked NodeId add + TagId wrap-to-1 (2^64, theoretical) · `xml.rs:6119/612` incomplete entity decode (`&amp;`,`&quot;`,`&#NN;`) + case-sensitive `url(` miss · `hit_test.rs:610` DomId>0xFFFF tag corruption (debug_assert only) · `window.rs:1478` hidpi divide-by-zero on dpi==0 · `task.rs:210/742` linear_interpolate NaN on zero interval + millis overflow · `geom.rs:203` quantize NaN→0 collision + isize overflow on wasm32.

## Verified OK (do not "fix")
`ImageRef` refcount unsafe is sound (identity via never-reused id) · image GC evicts all 3 paired maps + handles epoch wrap · `GlShader::new` error paths delete correctly · `GlContextPtr`/`host_handle_destructor` handle double-drop/null · serde `Json::parse` has its own recursion limit · `f64_as_i64` bounds-checked · xml `unsafe` boxes are null-checked/take-and-null.

---

**Suggested first fixes (highest value, easily triggered):** the CRITICAL four
(path_parser hang, RefAny downcast race, GL Texture double-drop UAF, static-mut GL
race), then the font leak + `normalize_u16` + `restyle` compact-cache, then the
z-order hit-test + NaN resize loop + `Instant` panic.
