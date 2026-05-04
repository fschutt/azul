---
slug: dom/merge-callbacks
title: Merge Callbacks
language: en
canonical_slug: dom/merge-callbacks
audience: external
maturity: mature
guide_order: 34
topic_only: false
short_desc: Surviving the next layout() rebuild — the reconcile-style protocol that hands a widget its previous dataset so heavy resources (video decoders, GL textures, websockets) don't get freed and reopened every frame.
prerequisites: [dom, dom/datasets]
tracked_files:
  - core/src/dom.rs
  - core/src/diff.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T05:53:30Z
---

# Merge Callbacks

A [dataset](datasets.md) attached to a node is rebuilt every time
`layout()` returns a new tree. That's fine for marker structs and
small per-instance state — they're cheap to recreate. It is **not** fine
for state that owns expensive resources: an FFmpeg encoder mid-frame, a
GL texture, a thread-bound decoder, an open websocket. You don't want
those to be freed and reopened every time something far up the tree
re-renders.

A **merge callback** is the protocol the framework uses to hand the
*new* dataset a chance to claim the *old* dataset's resources before the
old one gets dropped. It is a `Reconcile()` in the Kubernetes sense: the
new value is the desired state expressed by the latest `layout()`; the
merge function reconciles it against what was actually there a frame
ago.

```
                       layout()  →  fresh Dom (desired state)
                                          │
                                          ▼
                                 ┌─────────────────┐
                                 │   diff pass     │   per node
                                 └─────────────────┘
                                          │
                          stable / moved? │  no  → drop old, install new
                                          │ yes
                                          ▼
                              ┌────────────────────────┐
                              │ merge_fn(new, old) → m │   reconcile
                              └────────────────────────┘
                                          │
                                          ▼
                                 install m on new node
```

The function pointer signature is plain:

```rust,ignore
pub type DatasetMergeCallbackType =
    extern "C" fn(new_data: RefAny, old_data: RefAny) -> RefAny;
```

…and the framework invokes it from `core/src/diff.rs:793` during
reconciliation.

## When the framework calls it

The diff pass classifies every node into one of:

- **Stable** — same key (or same structural hash) as last frame at the
  same path. Old node id ↔ new node id.
- **Moved** — same key but different parent or sibling order.
- **Created** — no match in the previous tree.
- **Destroyed** — previous tree had a node here, the new tree does not.

The merge callback fires on **moves and stable nodes that ended up in
`node_moves`**. Created / destroyed nodes get whatever the new layout
function puts on them, with no merge.

A reliable [`with_key(...)`](../dom.md#callbacks-keys-datasets-virtualview)
on the node is what makes the difference between "moved" and
"destroyed + created". For a list that can reorder, **use a key** — the
structural-hash fallback works only when the order is fixed.

The framework runs the merge for any node where:

1. The new node has a `with_merge_callback(...)` attached, *and*
2. Both the old and the new node have datasets.

If either side is missing a dataset the framework leaves both alone:
the new node keeps whatever you put on it in `layout()`, the old one
drops normally.

## A worked example: a video encoder pipeline

Say you have a small video pane that holds an FFmpeg encoder
configured against a JSON-described pipeline. The application data
model stores the *desired* configuration (resolution, codec, bitrate,
overlay text); the widget owns the actual encoder instance plus a GL
texture that mirrors the current decode frame. The desired
configuration changes from time to time — bitrate adjusts, the user
toggles an overlay — and the widget needs to reconcile its live encoder
to match without tearing it down and rebuilding it.

```rust,ignore
use azul::prelude::*;

/// Application-side configuration. This is what `layout()` sees; this
/// is what describes the *desired* encoder state.
#[derive(Clone, PartialEq)]
pub struct VideoConfig {
    pub source_path: String,
    pub width: u32, pub height: u32,
    pub codec: String,         // "h264" / "hevc" / ...
    pub bitrate_kbps: u32,
    pub overlay_text: Option<String>,
}

/// Widget-side state. Holds the live encoder and its render target.
/// Built and torn down by the widget itself, never seen by app data.
pub struct VideoPaneState {
    pub config: VideoConfig,           // last applied config
    pub encoder: Option<FfmpegEncoder>,
    pub gl_texture: Option<GlTexture>,
    pub last_frame_idx: u64,
}
# pub struct FfmpegEncoder; pub struct GlTexture;
# impl FfmpegEncoder {
#     pub fn open(_: &VideoConfig) -> Self { Self }
#     pub fn reconfigure(&mut self, _: &VideoConfig) {}
# }

/// The reconcile function. Same signature as a Kubernetes reconciler:
/// "the world should look like X; bring it there."
extern "C" fn reconcile_video(new_data: RefAny, old_data: RefAny) -> RefAny {
    let new = new_data.clone();
    let mut new_state = match new.downcast_mut::<VideoPaneState>() {
        Some(s) => s, None => return new_data,    // unrecognised: let new win
    };
    let old = old_data.clone();
    let mut old_state = match old.downcast_mut::<VideoPaneState>() {
        Some(s) => s, None => return new_data,
    };

    // Take ownership of old encoder + texture, reuse them if compatible.
    if let Some(mut enc) = old_state.encoder.take() {
        if encoder_compatible(&new_state.config, &old_state.config) {
            enc.reconfigure(&new_state.config);   // bitrate / overlay tweak
            new_state.encoder = Some(enc);
        }
        // else: the source path or resolution changed. `enc` drops here,
        // releasing the FFmpeg context; the widget will lazy-open a fresh
        // one on the next render-image callback.
    }
    if let Some(tex) = old_state.gl_texture.take() {
        if texture_compatible(&new_state.config, &old_state.config) {
            new_state.gl_texture = Some(tex);
        }
    }
    new_state.last_frame_idx = old_state.last_frame_idx;

    drop(new_state); drop(old_state);
    new
}

fn encoder_compatible(a: &VideoConfig, b: &VideoConfig) -> bool {
    a.source_path == b.source_path && a.codec == b.codec
}
fn texture_compatible(a: &VideoConfig, b: &VideoConfig) -> bool {
    a.width == b.width && a.height == b.height
}

/// Building the widget. The dataset is a *fresh* VideoPaneState each
/// frame — empty encoder, empty texture, the new config baked in. The
/// merge callback is what fills in the heavy fields from the old state.
pub fn video_pane(config: VideoConfig) -> Dom {
    let key = stable_key_for(&config.source_path);
    let state = RefAny::new(VideoPaneState {
        config,
        encoder: None,
        gl_texture: None,
        last_frame_idx: 0,
    });
    Dom::create_div()
        .with_class("video-pane".into())
        .with_dataset(OptionRefAny::Some(state))
        .with_merge_callback(reconcile_video)
        .with_key(key)
}
# fn stable_key_for(_: &str) -> u64 { 0 }
```

What this gets you across a `RefreshDom`:

1. The parent re-renders for any reason (state change far up the tree,
   route switch, debug-server-driven re-layout).
2. The new tree contains a `<div class="video-pane">` with a fresh
   `VideoPaneState { config: <new>, encoder: None, gl_texture: None, ... }`
   in its dataset. That's because `layout()` just builds the value
   naively from the new config.
3. The diff pass matches old ↔ new (same key on the same path),
   classifies it as a stable-or-moved node, and fires `reconcile_video`.
4. `reconcile_video` looks at the two configs:
   - Same source + codec → take the live encoder, call `reconfigure`
     (bitrate / overlay toggle), keep going.
   - Different source or codec → drop the old encoder, let the widget
     re-open it on the next render frame.
   - Texture compatible (same resolution) → reuse it; otherwise drop it.
5. The new node's dataset is now populated with the merged state. The
   old node's `VideoPaneState` drops empty.

The shape mirrors the Kubernetes pattern exactly: `layout()` describes
the desired state declaratively, the reconciler runs against the actual
live state and brings the world into alignment with the smallest
possible delta. `take()` is the load-bearing primitive.

## Designing a merge function

Three rules of thumb:

- **Default to `new_data`.** The merge function returns whatever should
  *be* the new dataset. If you can't make sense of either side
  (downcast failed, type mismatch), return `new_data` and let the new
  state win — same outcome as if no merge callback were registered.
- **Use `take()` for resources.** Anything you want to move from old
  into new should be `Option<T>` and pulled out with `Option::take()`.
  When the function returns, the old `RefAny` drops; whatever you
  haven't taken gets freed.
- **Compare configs before reusing.** The new dataset carries the
  desired state; the old dataset has the actual state plus the resource
  it built last time. If the configs disagree (different codec,
  different source), the *resource* is no longer valid for the new
  config — drop it and let the widget rebuild.

The framework hands you cheap shallow clones of both `RefAny`s — the
encoder, texture, and other heavy fields don't move until you call
`take()` on them.

## What changes if the user navigates away

If the new layout function doesn't include a node that matches the old
one, the diff classifies the old node as **destroyed**. No merge fires;
the old `VideoPaneState` drops normally and `Drop` on the inner
`FfmpegEncoder` and `GlTexture` cleans up. This is the correct
behaviour: the user navigated away, the encoder is no longer needed,
the GPU memory comes back.

If the user navigates *back* later, a fresh `VideoPaneState` gets
created with no inherited encoder/texture, and the widget lazy-opens a
new pipeline on its next render-image callback.

## Where this differs from "use a RefAny on App state"

Putting the encoder on the application's data model *works* if the
encoder is a singleton — the main viewer on the home screen, say. The
merge-callback pattern wins when:

- **The widget instance is the lifetime boundary.** A video pane
  embedded inside a search-result row that the user can re-query. The
  application data has search results, not "the video that was playing
  in the row that is now row 4". The widget itself is the thing whose
  disappearance should free the resources.
- **You have many of them, dynamically.** Without merge-callbacks,
  per-instance state on app data turns into a `HashMap<WidgetId,
  VideoPaneState>` that you maintain in parallel with the DOM. The
  dataset slot is *that hashmap*, indexed by the diff key, freed when
  the node is destroyed.
- **The state is genuinely UI-layer.** The application doesn't care
  about the cursor position inside a text input, the scroll offset of
  a list, the GL texture that's currently mirroring a decode frame.
  Putting that on the model pollutes the data layer with view-only
  state.

A useful split:

- **Application data → `RefAny`** at app construction, accessed in
  `layout()` to decide what to render and which configs to pass to
  widgets.
- **Widget-instance state → dataset on the widget's root node**, with a
  merge callback if it owns expensive resources.

## Where to read the source

- `core/src/dom.rs:1794` — `NodeDataExt.dataset_merge_callback` slot
- `core/src/dom.rs:1828` — `DatasetMergeCallback` struct
- `core/src/dom.rs:1872` — `DatasetMergeCallbackType` signature
- `core/src/dom.rs:5056` — `Dom::with_merge_callback`
- `core/src/diff.rs:793` — where the merge callback fires during reconciliation
