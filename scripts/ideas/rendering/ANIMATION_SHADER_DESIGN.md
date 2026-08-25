# Design report: DOM-morph animation + custom-shader CSS layers for azul

2026-07-08. Research + design (read-only; no code changed). Two features explored
against azul's real architecture (`scripts/ARCHITECTURE.md`, `doc/guide/en/architecture.md`):
(1) an animation system that fluently **morphs Dom A → Dom B**, and (2) **custom
shader layers** in CSS (glassmorphism), reusing **blinc**, patching the vendored
WebRender, with CPU + web fallbacks. Both must hook the DOM-diff mount/unmount
lifecycle. File refs are absolute-relative under the repo root.

> **The two headline de-risks:** azul's diff already produces an explicit old↔new
> element **correspondence map** (`DiffResult.node_moves`) — that *is* the morph's
> shared-element map. And azul's **vendored WebRender already implements the full
> `backdrop-filter` capture→resolve→composite pipeline** — "sample the layer behind"
> is already solved. Neither feature starts from scratch.

---

# PART 1 — DOM-morph animation system

## 1.1 How the web does it (research synthesis)

| System | Model | Lesson for azul |
|---|---|---|
| **View Transitions API** (browser) | Snapshot old + new tagged elements, cross-fade + tween a group box old→new geom. `view-transition-name` = correspondence key. | Closest prior art, but the old state is a **static bitmap** — no live reflow/interaction. This is the weakness to beat. |
| **FLIP** (First-Last-Invert-Play) | Record First rect, mutate, record Last rect, invert with a `transform`, animate to identity. Only `transform`/`opacity` animate → compositor-only. | Maps 1:1 onto azul's GPU transform/opacity keys. |
| **Framer Motion `layout`/`layoutId`** | Shared-layout FLIP on **live nodes**; `layoutId` links an unmounting element to a mounting one as one entity. "transform for highest performance… more performant than screenshots." | **The model to adopt.** `layoutId` ≡ azul's reconciliation key. |
| **Springs** (React Spring / wobble) | mass/stiffness/damping integrated per frame; inherently **interruptible + velocity-preserving**. Semi-implicit Euler (cheap/stable) or closed-form damped-harmonic. | Bezier easing breaks under interruption; springs are the right interpolation for drag-release/retarget. |
| **Motion One / WAAPI** | Compositor-thread transform/opacity; springs compiled to `linear()`/keyframes. WAAPI can't interrupt (new anim just jumps). | Treat **interruption as first-class**, not bolted-on. |
| **Rive** (`rive-rs`, Rust) | Timeline animations wired into a **state machine**; states blended by data-bound inputs. | "Dom A / Dom B" are two states; transitions carry blend curves — a powerful higher-level framing (defer). |
| **Lottie / rlottie** (AirBnB lineage) | Pre-authored AE vector keyframe JSON, any keyable property interpolated at runtime. | Asset playback, **orthogonal** to structural morphing. Validates "interpolate any keyable prop" (which azul already has). No AirBnB *Rust* runtime exists — the Rust one here is Rive. |

**Tree morphing** is universally reduced to **per-element correspondence + enter/exit/move
classification**, then FLIP the moves and fade/scale the enters/exits. Path/shape
morphing (resample to equal points + lerp) is separate.

**Target design = Framer Motion `layoutId` (live-node shared-layout FLIP) + spring
interruptibility + optional Rive-style state framing** — explicitly beating View
Transitions' static-bitmap limitation.

## 1.2 azul's existing substrate (~80% built)

- **The diff IS the correspondence map** — `core/src/diff.rs`:
  - `DiffResult.node_moves: Vec<NodeMove{old_node_id,new_node_id}>` (`diff.rs:401/415`) → `create_migration_map` `:731`. **This is the A→B shared-element map, free.**
  - Matching tiers `:446-541`: Tier1 reconciliation key (`.with_key()` → `#id` → structural) = **`layoutId`**; Tier2 content-hash; Tier3 structural. Enter = unmatched-new → `Mount` `:598`; Exit = unmatched-old → `Unmount` `:620`.
  - `RelayoutScope` per node (`:1358`): transform/opacity-only classify to paint/gpu-only.
- **CSS interpolation engine** — `CssProperty::interpolate(other,t,resolver)` (`css/src/props/property.rs:4584`), maps t through the easing curve, per-property lerp (opacity/transform/dims/color/font-size…). Easing `AnimationInterpolationFunction {Ease,Linear,EaseIn/Out/InOut,CubicBezier}` (`css/src/props/basic/animation.rs`). **Gap: no spring** (and this enum is `#[repr(C)]` → adding `Spring` is an ABI break).
- **Clock** — `Instant::linear_interpolate(start,end)->f32` (`core/src/task.rs:210`, NaN-guarded 0→1); `Timer` (`layout/src/timer.rs`) ticked by `invoke_expired_timers` (`event.rs:5424`). **A per-frame timer is the driver.**
- **No-relayout override channel** — `CallbackInfo::override_css_property(node, prop)` (`layout/src/callbacks.rs:1355`) → `user_overridden_properties` (top-priority cascade layer, `prop_cache.rs:687`); doc says *"typical for animation callbacks."*
- **GPU FLIP substrate** — `GpuValueCache` transform/opacity keys (`core/src/gpu.rs:43`) → `PushReferenceFrame{transform_key}` / `PushOpacity` (`display_list.rs:797`) → WebRender animates the key **on the GPU**. **Scrollbar thumbs already use this** — a working template.
- **Integration seam** — `dll/.../shell2/common/layout.rs:361-467` is the one place with old node_data + new node_data + `node_moves` **before the swap** (`:407`); old geometry readable from prior `LayoutCache`.

**Two missing pieces:** (1) an `AnimationManager` coordinating layer; (2) **exit-retention** — `BeforeUnmount` (`diff.rs:441-466`) is fire-and-forget; the old DOM is dropped synchronously at the swap, so there's no way to keep an exiting node alive to animate out. (Note: `ARCHITECTURE.md:184` references a non-existent `core/src/animation.rs`.)

## 1.3 Approaches

1. **Imperative FLIP kit** (thin helper, no engine change) — user stores prev rect in `RefAny`, starts a `Timer` that `override_css_property(transform)` from Δ→identity. *Effort: days. Zero ABI risk. No enter/exit/shared-element.* → **Phase-0 spike.**
2. **Declarative shared-layout (`layoutId`), engine-driven FLIP — RECOMMENDED.** Mark morphable elements with `.with_key(id)` + `.animate_layout(spec)`; the **engine** captures First (old layout) + Last (new layout) rects at the diff seam, and for every `node_moves` pair with differing rect/props auto-seeds a GPU transform/opacity morph (Framer Motion `layout`/`layoutId` on **live nodes**). Enter = default fade/scale-in; exit = fade/scale-out via **exit-retention**. *Effort: 2-4 weeks. Best power/effort; beats View Transitions (live, reflowable). Risk: exit-retention vs the double-drop-sensitive teardown.*
3. **View-Transitions-style snapshot** — snapshot old subtree, cross-fade to new tweening a group box. *Simplest exit story (bitmap = retained old state), but inherits the static-old-state/no-reflow weakness. Optional mode for whole-page route transitions, layered on #2.*
4. **Spring / state-machine layer** — add `Spring{stiffness,damping,mass}` (semi-implicit Euler / closed-form) as an interpolation option (natively interruptible), and optionally a Rive-style state machine (A/B = states). *Spring core: low-med (+ `#[repr(C)]` ABI bump). State machine: high — defer.*

## 1.4 Recommendation + build order

**Approach 2 (declarative `layoutId` shared-layout, engine FLIP) + Approach 4's spring as default interpolation + Approach 1 imperative escape hatch; Approach 3 as an optional route-transition mode.** Reuses nearly all existing machinery, matches the proven web model, animates **live GPU nodes** (structurally better than View Transitions).

1. **Phase 0 (days):** imperative helper — validate timer→`override_css_property(transform)`→GPU-key end-to-end; create a real `core/src/animation.rs` home (fix the stale doc ref).
2. **Phase 1 (weeks):** `AnimationManager` (sibling to `GpuStateManager`) + First/Last capture at `layout.rs:~415` + auto-FLIP for differing `node_moves` + default enter fade/scale. Populate the currently-empty layout maps (`layout.rs:382`) so geometry deltas are known.
3. **Phase 2:** exit-retention — "ghost" the old node from `old_node_data` on Unmount of opted-in nodes, animate out on GPU transforms, drop on completion. **Keep ghosts out of the `transfer_states` path** (double-drop hazard per project memory).
4. **Phase 3:** `Spring` interpolation + velocity-preserving retarget-on-interrupt in the manager. (Batch the `#[repr(C)]`/api.json ABI break with other planned breaks.)

## 1.5 Timer-driven presence + FLIP (concrete mechanism)

The whole system **compiles down to keyed timer callbacks** — no new scheduler, no new
event loop. A "system event" (mount / before-unmount / a detected move) does exactly one
thing: **start (or retarget) a `Timer` whose `RefAny` carries the animation state.** The
timer ticks `t: 0→1` via `Instant::linear_interpolate` (`task.rs:210`), writes interpolated
values, and requests a repaint. That reuses `Timer` + `override_css_property` + the GPU keys
verbatim.

**The load-bearing invariant** (state this as law):

> **Logic, callbacks, and hit-testing operate on Dom B only.** The display list rendered is
> **`B ∪ retained-A-zombies`**. Zombies are **non-interactive** (excluded from the hit-test
> tag tree). So `on_frame` "sees" B the instant the swap happens, while the screen still
> shows A-items flying out — exactly the desired decoupling. This is the retained-mode dual
> of a GPU back-buffer: B is the front buffer for *logic*; the zombie set is the residue of
> the old front buffer still being *composited*.

**Three classes, one driver** — driven off the diff, not just unmount:

| Class | Trigger (source of truth) | Timer seed (in `RefAny`) | What the tick writes |
|---|---|---|---|
| **Enter** | node in B, unmatched-new → `Mount` (`diff.rs:598`) | target rect (B layout) | opacity/scale `0→1` via GPU keys |
| **Exit** | node in A, unmatched-old → `Unmount` (`diff.rs:620`) → **`BeforeUnmount`** (`diff.rs:441`) | **frozen A rect + retained subtree** | opacity/scale `1→0`; drop subtree at `t=1` |
| **Move / morph** | `node_moves{old,new}` pair (`diff.rs:401`) with differing rect/props | First rect (A) + Last rect (B) | FLIP: `transform` Δ→identity on the GPU key |

Enter/Exit are the presence cases; **Move is FLIP and it is the "better than the web" part** —
it comes free from `node_moves` / `create_migration_map` (`diff.rs:731`), the correspondence
map that View Transitions fakes with `view-transition-name`. Don't derive morphs from unmount;
derive them from the correspondence at the swap seam (`shell2/common/layout.rs:~407`, the one
place with old+new node_data + `node_moves` before the swap).

**Retain subtrees, not the whole DOM.** "DOM back-buffer" is the v1 mental model, but a full
clone of Dom A per transition is wasteful (5000-node DOM, one exiting toast). What's actually
retained is a **keyed set of animating subtrees + their frozen computed layout**:

```
struct RetainedZombie {
    node_id:      NodeId,          // key = reconciliation key, stable across A/B
    subtree:      StyledDom,       // the opted-in exiting subtree only (not all of A)
    frozen_rect:  LogicalRect,     // last solved geometry from A's LayoutCache
    anim:         AnimState,       // t, easing/spring, from/to, velocity (for retarget)
}
struct AnimState { t: f32, curve: Interp, from: Interpolated, to: Interpolated, vel: f32 }
```

The `AnimationManager` (sibling to `GpuStateManager`) owns `Vec<RetainedZombie>` +
`Map<NodeId, AnimState>` for live enter/move nodes. The timer callback advances `anim.t`,
recomputes the interpolated value, and pushes it into `user_overridden_properties`
(`prop_cache.rs:687`) — the top cascade layer the docs already call out for animation.

**Display-list merge** = one extra pass, cheap because zombies are overlays outside B's layout:

```
build_display_list(B):
    dl = normal_build(B)                       # unchanged
    for z in anim_mgr.zombies:                 # inject as absolutely-positioned overlays
        item = build_subtree(z.subtree)
        item.bounds     = lerp(z.frozen_rect, z.target_or_frozen, z.anim.t)
        item.transform  = z.anim.gpu_transform_key   # GPU-animated, no per-frame relayout
        item.hit_test   = DISABLED                    # zombies are non-interactive
        dl.push_overlay(item, z.original_stacking)    # keep A z-order as an overlay layer
    return dl
```

Because zombies are injected as **composited overlays**, they do not perturb B's layout —
exiting siblings' space collapses immediately (the usual, wanted behavior). The rarer
"exiting item shrinks and pushes its siblings" needs the item to stay in flow and is a
separate, more expensive mode (per-tick partial re-solve).

**The perf fork — composited vs. layout animations.** `RelayoutScope` (`diff.rs:1358`) already
classifies each changed property:

- **transform / opacity only** → route through `GpuValueCache` keys (`gpu.rs:43`) →
  `PushReferenceFrame{transform_key}` / `PushOpacity` (`display_list.rs:797`). WebRender
  interpolates on the **GPU, zero relayout, 60 fps free** (scrollbar thumbs already do this).
  Enter fade, exit fade, and FLIP moves all live here.
- **layout-affecting props** (width/height/flex/margin) → each tick is a **partial re-solve**.
  Tag these explicitly; otherwise a naive "animate width" silently relayouts every frame.

**Interruption/retarget — design it in, don't bolt it on.** On A→B→C while A's exit is still
mid-flight, the animation must **not** restart from 0. Because state is **keyed by
reconciliation key**, the manager finds the in-flight `AnimState` and **retargets**: new
`to`, `from = current interpolated value`, `vel` carried over (this is *why* Approach 4's
spring beats bezier — springs retarget with velocity continuity; bezier snaps). WAAPI can't
do this; making it first-class is the differentiator.

**New surface — exactly four additions, all onto existing systems:**
1. **`on_before_unmount` system event** (new) — the only genuinely new callback; `Mount`
   already exists for enter. It fires at the diff seam and, for opted-in nodes, hands the
   exiting subtree + frozen rect to the `AnimationManager` instead of dropping it.
2. **`AnimationManager`** — keyed store of zombies + live anim states; sibling to
   `GpuStateManager`; lives in a real `core/src/animation.rs` (fixes the stale
   `ARCHITECTURE.md:184` ref).
3. **Display-list merge pass** — inject zombies as composited, non-interactive overlays.
4. **Timer glue** — one repaint-driving timer per active transition (or one shared ticker),
   `RefAny` = the anim state.

**The one real hazard:** exit-retention must keep zombies **out of `transfer_states`** at the
swap — the old DOM's teardown is the double-drop-sensitive path (project memory:
`azul-double-drop-systemic-fix`, `azul-clone-drop-double-free-audit`). A zombie is a *moved-out*
subtree with its own lifetime that drops exactly once at `t=1`, never via A's synchronous
teardown. Get that boundary right and the rest is additive.

---

# PART 2 — Custom shader CSS layers (glassmorphism)

## 2.1 blinc (research)

`github.com/project-blinc/Blinc` · `blinc.rs` · **Apache-2.0** · renderer crate `blinc_gpu` = a **standalone wgpu** renderer (Metal/DX12/Vulkan/WebGPU). azul-WR runs on **OpenGL (gleam)** → blinc's runtime **cannot** be dropped in. Reusable, license-compatible assets:
- **`@flow` shader-graph** (`FlowGraph` DAG of `FlowExpr`/`FlowFunc` incl. `SampleScene`, `Sdf*`, `Fbm`, `Phong`) + **`flow_to_wgsl()`** compiler (emits `FlowUniforms{viewport_size,time,frame_index,element_bounds,pointer,corner_radius}` + a corner-radius-SDF-clipped `fs_main`).
- **`SampleScene`** maps the element's screen rect into a **scene texture** = "sample what's behind." Backdrop produced by a **ping-pong `Backbuffer`** = the **prior frame's** composited scene (1-frame latency, cheap).
- **`GLASS_SHADER` / `SIMPLE_GLASS_SHADER`** WGSL (Apple-style frosted glass) + `GpuGlassPrimitive`/`GlassUniforms`.

**Reuse = depend on blinc for `@flow`/`flow_to_wgsl` codegen + port its glass WGSL, transpiled WGSL→GLSL via `naga`. Never its wgpu runtime** (that's the fork trap).

## 2.2 azul's vendored WebRender (research)

- Shaders are **compile-time typed fields** on `Shaders` (`webrender/core/src/renderer/shade.rs`) — **no runtime custom-GLSL hook**.
- **Backdrop already captured** for `backdrop-filter`: `SceneBuilder::add_backdrop_filter` (`webrender/core/src/scene_building.rs:3453`) → `BackdropCapture` placeholder → intermediate surface → `ResolveOp` blits backdrop-root pixels → filter chain, increasingly an **SVGFE DAG** (`PictureCompositeMode::SVGFEGraph`, executed by `res/cs_svg_filter_node.glsl`; `FilterGraphOp::SVGFESourceGraphic` = the backdrop). `push_backdrop_filter` is **already public** (`webrender/api/src/display_list.rs:1822`). `res/brush_mix_blend.glsl` is the canonical "sample what's behind me" template.
- Two native extension points: **(A) `FilterGraphOp::CustomShader`** node in the SVGFE graph (~150-300 LOC, inherits capture→resolve→composite; best glassmorphism fit) or **(B) `PatternKind::ShaderBackground`** quad shader (~400-600 LOC, first-class background paint but no backdrop by default).

## 2.3 azul's own pipeline (extension points)

- **CSS parse:** `StyleBackgroundContent` enum (`css/src/props/style/background.rs:62`), parsed by `parse_style_background_content` `:1097` via a function-name list `:1102`; `PrintAsCssValue`/`to_hash` in lockstep. (Closed codegen enum — no dynamic property mechanism; ride inside `StyleBackgroundContent` = least ABI churn.)
- **Lowering:** `push_backgrounds_and_border` (`layout/src/solver3/display_list.rs:1335`) → `DisplayListItem` → `translate_displaylist_to_wr` (`dll/.../compositor2.rs:164`). WR item set is **fixed** (no new `SpecificDisplayItem`).
- **Existing custom-GL path:** `GlShader::new(ctx,vtx,frag)` compiles raw GLSL, `GlShader::draw` renders a full-screen quad into a `Texture` via a transient FBO — **`core/src/gl_fxaa.rs` is the exact "sample a texture → run a fragment shader → texture" template.** `Texture` → `ImageRef::callback` → invoked **every frame** by `process_image_callback_updates` (`wr_translate2.rs:2990`) → registered with WR as an **external image** → pulled via `ExternalImageHandler::lock` (`wr_translate2.rs:350`) as a `NativeTexture`. **Gaps:** `UniformType` (`gl.rs:3711`) has no sampler variant; `RenderImageCallbackInfo` (`layout/src/callbacks.rs:4732`, reserved `_abi_mut` slot) doesn't expose the backdrop.

## 2.4 CSS syntax

Houdini-`paint()`-style, superset-compatible with blinc's `glass`:
```
background: shader(<name> [, <arg>]*);              /* general */
background: shader(glass, blur 20px, tint #ffffff30);
backdrop-filter: glass;                             /* blinc shorthand → shader(glass) over backdrop */
```
Add `"shader"` to `parse_style_background_content`'s name list → new `StyleBackgroundContent::Shader(StyleShader{name,args})` variant (+ `PrintAsCssValue`/`to_hash` + both lower sites).

## 2.5 Approaches + recommendation

- **Tier 1 (ship first, ZERO WR risk):** `shader(...)` lowers to `ImageRef::callback` running a `GlShader` (à la `gl_fxaa.rs`) into a `Texture`; backdrop = **previous-frame composited copy** (blinc's Backbuffer model, 1-frame latency) handed via a new backdrop slot on `RenderImageCallbackInfo`; blinc glass WGSL→GLSL via `naga`; add a sampler `UniformType`. *Medium effort, no WebRender patching.*
- **Tier 2 (native, latency-free):** new `FilterGraphOp::CustomShader` node + a branch in `cs_svg_filter_node.glsl` sampling `SVGFESourceGraphic` (captured backdrop), reachable via `push_backdrop_filter`. *Med-high, ~150-300 LOC additive WR patch; correct mid-frame backdrop, composites/clips correctly.*
- **CPU emulation:** presets (`glass`/`blur`/`tint`) as a hand-ported separable box/Gaussian-blur + tint over the CPU backbuffer region behind the rect (covers ~90% of usage); later an `@flow` **software interpreter** (the DAG is simple ops) for arbitrary shaders. Needs the CPU compositor to expose "pixels behind this rect" (software BackdropCapture).
- **Web/WebGL2:** target **GLSL ES 3.00** (`naga` emits it) → Tier-2 works in-browser unchanged (WR does backdrop capture on WebGL2). Optional Houdini `paint()` bridge **only** for backdrop-independent procedural `@flow` shaders (Houdini can't read the backdrop). blinc's own web target is WebGPU (not directly reusable for WebGL2 — the `naga` transpile is what carries it).

**Recommendation: Tier 1 first (reuses the proven callback/external-image path + `GlShader`), then Tier 2 for latency-free `backdrop-filter: glass`.** Reuse blinc as a **shader-codegen/source adapter** (Apache-2.0), never its runtime. Reference shader: `brush_mix_blend.glsl`.

**Key files:** `css/.../background.rs` (parse+enum), `layout/.../display_list.rs` (lower), `layout/src/callbacks.rs` (`RenderImageCallbackInfo` backdrop slot), `core/src/gl.rs` (sampler `UniformType`; `gl_fxaa.rs` template), `dll/.../compositor2.rs` + `wr_translate2.rs` (Tier-2 lower + backdrop copy); Tier-2 WR: `webrender/api` (`FilterGraphOp`), `webrender/core/src/render_task.rs`, `res/cs_svg_filter_node.glsl`.

---

# PART 3 — Lifecycle integration (both features share this)

Both features hook the same diff/lifecycle seam:
- The **diff** (`diff.rs`) already classifies enter (`Mount`)/exit (`Unmount`)/move (`node_moves`) and pre-resolves `BeforeUnmount` against the dying node (`layout.rs:441-466`).
- **Missing:** a **deferral/retention** mechanism so an element can *animate out before removal* (and a shader layer can tear down its GL texture). Today `BeforeUnmount` is a notification only. The build: let `reconcile_dom`/`ChangeAccumulator` (`diff.rs:1084`, unmount at `:1221`) honor a **retain** signal — a new `Update`/`Update::Retain` variant returned from `BeforeUnmount` that keeps the node as a transient ghost, ticked by the `AnimationManager`, dropped on completion.
- **Per-frame clock:** one `TimerId` in `process_timers_and_threads` (`event.rs:5133`) drives both the animation ticks and per-frame shader-texture refresh (textures are **epoch-scoped** — `gl_textures_remove_epochs_from_pipeline`, `gl.rs:779` — so an animated shader must re-register its texture per epoch).

**Hard constraints (both):** `CssProperty`/`DisplayListItem`/callback types are `#[repr(C)]` + codegen/api.json-frozen → new enum variants are ABI breaks to batch; `core` is `no_std`-capable (use `task::Instant`, not `std::time`); the `ExternalImageHandler` is single-threaded.

---

## Sources
Blinc (github.com/project-blinc/Blinc, blinc.rs, docs.rs/blinc_gpu) · WebRender `Shaders` + backdrop-filter (bugzilla 1178765) · WebKit backdrop filters · MDN View Transitions API + CSS Painting API · Chrome view-transitions guide · CSS-Tricks/Aerotwist FLIP · motion.dev (layout animations, WAAPI) · React Spring configs · Gaffer on Games (integration) · wobble · rive-rs + Rive state machines · Lottie (Wikipedia, airbnb.tech).
