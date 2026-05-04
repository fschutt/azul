---
slug: dom/datasets-and-merge-callbacks
title: Datasets and Merge Callbacks
language: en
canonical_slug: dom/datasets-and-merge-callbacks
audience: external
maturity: mature
guide_order: 33
topic_only: false
short_desc: Attaching arbitrary state to a node — and surviving the next layout() rebuild without losing video decoders, GL textures, or focus-buffers.
prerequisites: [dom]
tracked_files:
  - core/src/dom.rs
  - core/src/diff.rs
  - core/src/callbacks.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T05:53:30Z
---

# Datasets and Merge Callbacks

The `Dom` is rebuilt from scratch every time a callback returns
`Update::RefreshDom`. The application data lives in a `RefAny` you own;
the tree is a fresh value. That works fine until you have **state that
exists only because the widget exists** — a video decoder mid-frame, a
GL texture cache, the typing buffer of a focused `<input>`, the scroll
offset of a scrollable region. None of that belongs in the application
data model, and you don't want to lose it just because the parent
re-rendered.

Datasets are the slot. A merge callback is the protocol that survives a
tree rebuild.

## What a dataset is

`Dom::with_dataset(OptionRefAny)` (`core/src/dom.rs:5145`) stamps a
`RefAny` onto a single node. The `RefAny` is yours — same type-erased,
atomically reference-counted handle as
[anywhere else in the framework](../architecture/understanding-refany.md).
What makes the dataset slot special is *where* it lives: on the node, in
the tree, traveling with the widget instead of in your application data.

```rust,no_run
# use azul::prelude::*;
struct EditorState {
    text: String,
    cursor: usize,
    undo: Vec<String>,
}

fn editor_node(initial: &str) -> Dom {
    let state = RefAny::new(EditorState {
        text: initial.to_string(),
        cursor: 0,
        undo: Vec::new(),
    });
    Dom::create_input_no_a11y("text".into(), "editor".into(), initial.into())
        .with_dataset(OptionRefAny::Some(state))
}
```

The dataset is **read-write at callback time** via
`CallbackInfo::get_dataset(node_id)` — typically `info.get_hit_node()`
during a click handler:

```rust,no_run
# use azul::prelude::*;
# struct EditorState { text: String, cursor: usize, undo: Vec<String> }
extern "C" fn on_keydown(_unused: RefAny, mut info: CallbackInfo) -> Update {
    let hit = info.get_hit_node();
    let mut ds = match info.get_dataset(hit) {
        Some(d) => d,
        None => return Update::DoNothing,
    };
    let mut state = match ds.downcast_mut::<EditorState>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };
    state.text.push_str("…");
    Update::RefreshDom
}
```

The dataset survives the *current* frame because the wrapper class holds
the only handle. What survives the *next* frame is the question this page
exists to answer.

## When does a dataset disappear?

Every `layout()` call returns a fresh `Dom` tree. The framework reconciles
the new tree against the previous one (see `core/src/diff.rs`), figures
out what moved, what was created, what was destroyed. A node that the
diff considers "freshly created" gets whatever `with_dataset(...)` you
called in the new layout function — which means **the dataset on the new
node is whatever you put there in `layout()`**, not what was there a
frame ago.

For most widgets this is fine. The application data is the source of
truth; the dataset is computed each frame from it; resources held by the
dataset (an iterator state, a parsed glyph cache) are cheap to recompute.

For widgets where the dataset *holds something expensive* — a 4K video
decoder mid-frame, a thread-bound GL context, a websocket — recomputing
isn't an option. The merge callback is how the framework hands you the
old dataset so you can transfer the resources before the old node is
freed.

## How reconciliation finds the old state

The diff pass classifies every node into one of:

- **Stable**: same key (or same structural hash) as last frame at the
  same path. Old node id ↔ new node id.
- **Moved**: same key but different parent or sibling order.
- **Created**: no match in the previous tree.
- **Destroyed**: the previous tree had a node here, the new tree does not.

The merge callback fires on **moves only** (`core/src/diff.rs:793`).
Stable nodes already share their dataset across frames implicitly because
the framework keeps the previous node alive. Created / destroyed nodes
get whatever the new layout function puts on them, with no merge.

A reliable [`with_key(...)`](../dom.md#callbacks-keys-datasets-virtualview)
on the node is what makes the difference between "moved" and "destroyed +
created". For a list that can reorder, **use a key** — the structural-hash
fallback works only when the order is fixed.

## The merge callback

```rust,ignore
pub type DatasetMergeCallbackType =
    extern "C" fn(new_data: RefAny, old_data: RefAny) -> RefAny;
```

The framework, during reconciliation, sees:

1. The new node has a `with_merge_callback(merge_video)`.
2. Both the old and the new node have datasets.

…and calls `merge_video(new_data_clone, old_data_clone) -> merged`. The
returned `RefAny` becomes the new node's dataset. The old node and its
dataset get dropped after the call, **so any resources you want to keep
have to be moved across** — `Option::take()` is your friend.

## Worked example: a video player

```rust,no_run
# use azul::prelude::*;
# struct VideoDecoder;
# struct GlTexture;
struct VideoState {
    src: String,
    decoder: Option<VideoDecoder>,
    gl_texture: Option<GlTexture>,
    last_frame_idx: u64,
    paused: bool,
}

extern "C" fn merge_video(new_data: RefAny, old_data: RefAny) -> RefAny {
    // Both clones are cheap (refcount bumps). The actual VideoState lives
    // behind the RefAny, accessible only after a downcast_mut/ref.
    let mut new = new_data.clone();
    let mut new_state = match new.downcast_mut::<VideoState>() {
        Some(s) => s,
        None => return new_data, // type mismatch -> let the new one win
    };
    let mut old = old_data.clone();
    let mut old_state = match old.downcast_mut::<VideoState>() {
        Some(s) => s,
        None => return new_data,
    };

    if new_state.src == old_state.src {
        // Same video. Move heavy resources from old to new.
        new_state.decoder       = old_state.decoder.take();
        new_state.gl_texture    = old_state.gl_texture.take();
        new_state.last_frame_idx = old_state.last_frame_idx;
        new_state.paused        = old_state.paused;
    }
    // else: different src -> let the new VideoState start fresh; the old
    // decoder / GL texture get dropped when `old_data` falls out of scope.

    drop(new_state); drop(old_state);
    new
}

pub fn video_player(src: &str) -> Dom {
    let state = RefAny::new(VideoState {
        src: src.to_string(),
        decoder: None,
        gl_texture: None,
        last_frame_idx: 0,
        paused: false,
    });
    Dom::create_div()
        .with_class("video-player".into())
        .with_dataset(OptionRefAny::Some(state))
        .with_merge_callback(merge_video)
        .with_key(hash_str(src))   // key on src so reorder == same video
}

# fn hash_str(s: &str) -> u64 { 0 }
```

What this gets you across a `RefreshDom`:

- The *parent* re-rendered for whatever reason (state change far up the
  tree, route switch, debug-server-driven re-layout).
- The new tree contains a `<div class="video-player">` for `"movie.mp4"`
  with a fresh `VideoState { decoder: None, gl_texture: None, ... }` in
  its dataset. That's because `layout()` just builds the value naively.
- The diff pass matches old ↔ new (same key on the same path),
  classifies it as a move (or a stable node — they take the same path
  through `node_moves`), and fires `merge_video`.
- `merge_video` moves the decoder + texture out of the old `VideoState`
  and into the new one. The old `VideoState` drops empty: nothing to free.
- The video keeps playing. The user never noticed.

The same pattern fits anything *the widget owns and `layout()` cannot
recreate cheaply*:

- **Text input**: typing buffer + cursor + undo stack, so the user
  doesn't lose edits when something else triggers a re-render.
- **Scroll regions**: the current scroll offset for a node that shows
  data from a `RefAny` — the framework already handles raw scroll, but
  if you want sticky-scroll-to-bottom semantics you keep the flag here.
- **WebGL / Skia surfaces**: the GPU-side resource handle.
- **Subscriptions**: a websocket that's mid-receive, a timer with a
  pending tick.

If the widget reorders or its parent re-keys, the framework follows the
key and your `merge_*` keeps the resources on the right node.

## Where this differs from "use a RefAny on App state"

A reasonable question: why not just put the `VideoState` on the
application's data model and have `layout()` look it up by key?

You can. For widgets that are always-singletons (the main video viewer
on the home screen) it's even cleaner. The dataset pattern wins when:

1. **The widget instance is the lifetime boundary.** A video player
   nested inside a list of search results that the user can re-query —
   the application data model has *search results*, not "the video that
   was playing in the row that is now row 4". The widget itself is the
   thing whose disappearance should free the resources.
2. **You have many of them and they are dynamic.** Putting per-instance
   state on app data turns into a `HashMap<WidgetId, VideoState>`
   that's parallel to the DOM. The dataset slot is that hashmap, but
   handed to you by the framework, indexed by the diff key, and
   garbage-collected when the node is destroyed.
3. **The state is genuinely "UI-layer".** The application doesn't care
   about cursor position inside a text input, scroll offset, an undo
   stack inside an editor widget. Putting that on the model pollutes the
   data layer with view-only state.

A useful split:
- **Application data → `RefAny`** at app construction, accessed in
  `layout()` to decide what to render.
- **Widget-instance state → dataset on the widget's root node**, with a
  merge callback if it owns expensive resources.

## Backreferences from a dataset

Datasets can hold a back-pointer to the higher-level component (the
[backreference pattern](components.md#the-backreference-pattern)). When
the inner widget fires its private callback, it follows the
backreference up to the application-level callback to forward whatever
event the widget computed.

```rust,ignore
pub struct NumberInput {
    value: i64,
    on_change: Option<(RefAny, OnNumberChange)>,
    parent: RefAny,   // the back-pointer
}
```

The merge callback for a wrapper like this typically transfers the
`on_change` slot if it was set, while letting the new `value` win — the
parent doesn't change between frames, but the displayed value might.

## Reading datasets in callbacks

`CallbackInfo` exposes:

- `info.get_dataset(node_id)` — borrow the node's dataset.
  `node_id` is usually `info.get_hit_node()` for click handlers, or
  whatever `info.get_first_child(...)` / `get_node_by_class(...)` /
  `get_dom_node_id_at(x, y)` returns for less-direct access.
- `info.get_hit_node()` — node id of the node the event landed on
  (after event-filter propagation).

The borrow follows the `RefAny` rules: a `downcast_ref` blocks
`downcast_mut` and vice versa. Drop the guard before triggering anything
that might re-enter the same dataset.

## Where to read the source

- `core/src/dom.rs:1781` — `NodeDataExt.dataset: Option<RefAny>` slot
- `core/src/dom.rs:1794` — `dataset_merge_callback` slot
- `core/src/dom.rs:1828` — `DatasetMergeCallback` struct
- `core/src/dom.rs:1872` — `DatasetMergeCallbackType` signature
- `core/src/dom.rs:5145` — `Dom::with_dataset`
- `core/src/dom.rs:5056` — `Dom::with_merge_callback`
- `core/src/diff.rs:793` — where the merge callback fires during reconciliation
