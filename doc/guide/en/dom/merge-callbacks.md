---
slug: dom/merge-callbacks
title: Merge Callbacks
language: en
canonical_slug: dom/merge-callbacks
audience: external
maturity: mature
guide_order: 33
topic_only: false
short_desc: How widgets keep heavy resources across a layout rebuild
prerequisites: [dom, dom/datasets, dom/reconciliation]
tracked_files:
  - core/src/dom.rs
  - core/src/diff.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T05:53:30Z
---

# Merge Callbacks

## The problem

A [dataset](datasets.md) is rebuilt every time `layout()` runs. For a marker struct that's free. For an FFmpeg encoder it's a disaster.

Imagine a video pane whose dataset holds an open encoder, a GL texture, and a frame counter. The user toggles a checkbox somewhere far up the tree. `layout()` runs again. The pane appears in the new tree with a fresh dataset value. The old dataset drops. The encoder closes. The texture is freed. The next frame reopens everything.

That isn't acceptable for resources you can't recreate cheaply.

## The fix

A merge callback is a function attached to a node that gets to claim resources from the previous frame's dataset before that dataset is dropped.

```rust,ignore
pub type DatasetMergeCallbackType =
    extern "C" fn(new_data: RefAny, old_data: RefAny) -> RefAny;
```

You receive the new dataset (built fresh by `layout()` this frame) and the old dataset (the one from the previous frame). You move whatever you want to keep from `old` into `new`. You return the merged value. The framework installs it on the new node. Whatever you didn't take drops with the old `RefAny`.

This is a `Reconcile()` in the Kubernetes sense. The new value is the desired state. The old value is the actual state. The callback brings the world into alignment.

```
              layout()  →  fresh Dom (desired state)
                                  │
                                  ▼
                         ┌─────────────────┐
                         │   diff pass     │
                         └─────────────────┘
                                  │
                  matched node?   │  no  → drop old, install new
                                  │ yes
                                  ▼
                      ┌────────────────────────┐
                      │ merge_fn(new, old) → m │
                      └────────────────────────┘
                                  │
                                  ▼
                         install m on new node
```

## When it fires

The callback runs during reconciliation. The conditions are strict.

- The new node has a merge callback registered via `with_merge_callback(...)`.
- Both the old node and the new node have a dataset attached.
- The diff matched the two nodes (Stable or Moved). Created and Destroyed nodes don't qualify.

If any of those fails, nothing happens. The new dataset stays as `layout()` built it. The old one drops normally.

The matching rules belong to the diff. `with_key(...)` is what makes a node survive a reorder. Without a key, the diff falls back to structural hashing, which only works if the order is fixed. See [Reconciliation](reconciliation.md) for how that works.

## Worked example: a video encoder

Take a video pane backed by FFmpeg. The application data describes the desired pipeline. The widget owns the live encoder and a GL texture. The user can change the bitrate, toggle an overlay, or switch the source path.

Two structs:

```rust,ignore
use azul::prelude::*;

#[derive(Clone, PartialEq)]
pub struct VideoConfig {
    pub source_path: String,
    pub width: u32, pub height: u32,
    pub codec: String,
    pub bitrate_kbps: u32,
    pub overlay_text: Option<String>,
}

pub struct VideoPaneState {
    pub config: VideoConfig,
    pub encoder: Option<FfmpegEncoder>,
    pub gl_texture: Option<GlTexture>,
    pub last_frame_idx: u64,
}
# pub struct FfmpegEncoder; pub struct GlTexture;
# impl FfmpegEncoder {
#     pub fn open(_: &VideoConfig) -> Self { Self }
#     pub fn reconfigure(&mut self, _: &VideoConfig) {}
# }
```

`VideoConfig` is what the application stores. `VideoPaneState` is widget-owned. The application never sees the encoder.

The merge function does three things. It takes the encoder out of the old state. It checks whether the new config is compatible. If yes, it reconfigures and keeps the encoder; if no, it lets the old encoder drop.

```rust,ignore
extern "C" fn reconcile_video(new_data: RefAny, old_data: RefAny) -> RefAny {
    let new = new_data.clone();
    let mut new_state = match new.downcast_mut::<VideoPaneState>() {
        Some(s) => s,
        None => return new_data,
    };
    let old = old_data.clone();
    let mut old_state = match old.downcast_mut::<VideoPaneState>() {
        Some(s) => s,
        None => return new_data,
    };

    if let Some(mut enc) = old_state.encoder.take() {
        if encoder_compatible(&new_state.config, &old_state.config) {
            enc.reconfigure(&new_state.config);
            new_state.encoder = Some(enc);
        }
    }
    if let Some(tex) = old_state.gl_texture.take() {
        if texture_compatible(&new_state.config, &old_state.config) {
            new_state.gl_texture = Some(tex);
        }
    }
    new_state.last_frame_idx = old_state.last_frame_idx;

    drop(new_state);
    drop(old_state);
    new
}

fn encoder_compatible(a: &VideoConfig, b: &VideoConfig) -> bool {
    a.source_path == b.source_path && a.codec == b.codec
}
fn texture_compatible(a: &VideoConfig, b: &VideoConfig) -> bool {
    a.width == b.width && a.height == b.height
}
```

Building the widget is straightforward. Each frame `layout()` produces a `VideoPaneState` with the new config and empty resource slots. The merge callback fills the slots from the previous frame.

```rust,ignore
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

What happens on a `RefreshDom`:

1. The parent re-renders.
2. `layout()` builds a fresh `VideoPaneState` with `encoder: None` and `gl_texture: None`.
3. The diff matches old and new by key.
4. `reconcile_video` runs. The encoder moves over if the source and codec are unchanged. The texture moves over if the resolution is unchanged.
5. The new node now owns the live encoder. The old `VideoPaneState` drops empty.

If the user navigates away and the pane disappears from the new tree, the diff classifies the old node as destroyed. No callback fires. The old `VideoPaneState` drops normally. `Drop` on `FfmpegEncoder` and `GlTexture` releases the resources.

## Designing a merge function

A few rules.

Default to the new value. If you can't downcast, return `new_data` as-is. The new node ends up with whatever `layout()` built. That matches the behaviour of having no merge callback at all.

Use `Option<T>` for resources you might want to move. `Option::take()` is what transfers ownership. Whatever you don't take drops with the old `RefAny`.

Compare configs before reusing a resource. The new dataset has the desired config. The old dataset has the actual config plus the resource that was built for it. If the configs disagree, the resource is stale. Drop it and let the widget rebuild on the next render.

The clones the framework hands you are shallow. Heavy fields stay in place until you call `take()`.

## When to use this versus app-state RefAny

Putting expensive resources on the application's data model works if the resource is a singleton. One main viewer on the home screen is fine.

Merge callbacks are the right tool when the widget instance defines the lifetime. A video pane inside a search-result row is the canonical case. The application data has search results. It doesn't have "the encoder for the row that used to be at index 4". The widget's presence in the tree is what should keep the encoder alive.

Merge callbacks are also right when there are many instances created dynamically. Without them, you'd maintain a `HashMap<WidgetId, VideoPaneState>` on the application side that mirrors the DOM. The dataset slot is that hashmap, indexed by the diff key, freed when the node disappears.

A useful split:

- Application data lives in a `RefAny` at app construction. `layout()` reads it to decide what to render.
- Widget-instance state lives on the widget's root node as a dataset. Add a merge callback if the state owns expensive resources.

## Source

- `core/src/dom.rs` — `Dom::with_merge_callback`, `DatasetMergeCallback`, `DatasetMergeCallbackType`

## Coming Up Next

- [Virtual Views](virtual-views.md) — A node that materialises lazily, for infinite lists and embedded sub-DOMs
- [Components](components.md) — Reusable UI fragments - named functions of (args) -> Dom
- [Reconciliation](reconciliation.md) — Diffing, restyle scope, and damage-rect repaint
