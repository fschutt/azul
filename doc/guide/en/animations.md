---
slug: animations
title: Animations
language: en
canonical_slug: animations
audience: external
maturity: stub
guide_order: 110
topic_only: false
prerequisites: [timers]
tracked_files:
  - core/src/animation.rs
  - core/src/callbacks.rs
last_generated_rev: 2acdeae71299faed9a65b0dddeea8d53c350e9ac
generated_at: 2026-05-01T20:34:08Z
---

# Animations

> **Not yet functional.** The animation type definitions exist (`core/src/animation.rs`) but the runtime that ticks animations and drives layout updates is not yet wired up. This page describes the **planned** API. Until the runtime lands, hand-roll animations with [Timers](timers.md) — the gap is small.

A future `Animation` will be a declarative tween over a CSS property: a target value, a duration, an easing function, and an `UpdateImageType` flag controlling whether the property animates the element's background or main content. The framework will tick it on every frame, interpolate the property, and apply the result to the layout/render pipeline without re-running the user's `layout` callback.

## What exists today

```rust,ignore
// core/src/animation.rs
pub enum UpdateImageType {
    Background,  // animation targets the element's background image
    Content,     // animation targets the element's main content image
}
```

That is the entire current surface. The intent is for `UpdateImageType` to discriminate which image layer of an element a value-animation should drive when the framework hands the result to the GPU compositor.

There is no `Animation` struct yet. There is no `AnimationManager`. The CSS `transition:` and `animation:` properties parse but do not actually run.

## What you can do today

For visual effects that need to update on every frame, register a [Timer](timers.md) at ~16 ms intervals and have it return `Update::RefreshDom`. Read `Instant::now()` (or the timer's `frame_start`) inside your `layout` callback and compute the current value:

```rust,no_run
# use azul::prelude::*;
# struct State { started_at: Instant, animating: bool }
extern "C" fn tick(_: RefAny, _: TimerCallbackInfo) -> TimerCallbackReturn {
    TimerCallbackReturn::continue_and_refresh_dom()
}

extern "C" fn layout(mut data: RefAny, _: LayoutCallbackInfo) -> Dom {
    let state = match data.downcast_ref::<State>() {
        Some(s) => s,
        None => return Dom::create_body(),
    };

    let t = if state.animating {
        let now = Instant::now();
        let elapsed = now.duration_since(&state.started_at);
        // 0.0..=1.0
        elapsed.div(&Duration::System(SystemTimeDiff::from_millis(300))).clamp(0.0, 1.0)
    } else {
        1.0
    };

    let opacity = (t * 100.0) as i32;

    let mut body = Dom::create_body();
    body.set_inline_style(format!("opacity: {}%", opacity).as_str());
    body
}
```

The downside vs. a real animation runtime: each frame re-runs `layout()` and the framework re-styles everything. For most UIs at 60 Hz that is fine — Azul's layout cache is incremental — but transform and opacity changes that should stay GPU-only will go through the full pipeline.

## Easing functions exist for scrolling

The `EasingFunction` enum (`core/src/events.rs:26`) is wired up only for smooth-scroll right now:

```rust,ignore
pub enum EasingFunction {
    Linear,
    EaseInOut,
    EaseOut,
}
```

When the animation runtime lands it will reuse this enum. For now, if you implement easing manually, copy the curve formula you want — `EaseInOut` is `t < 0.5 ? 2t² : 1 - (−2t + 2)²/2`.

## Planned shape

The eventual API will look approximately like:

```rust,ignore
// PLANNED — DOES NOT EXIST YET
let anim = Animation::create(
    CssPropertyType::Opacity,
    CssProperty::Opacity(StyleOpacity::const_new(0.0)),  // from
    CssProperty::Opacity(StyleOpacity::const_new(1.0)),  // to
    Duration::System(SystemTimeDiff::from_millis(300)),
    AnimationInterpolationFunction::EaseOut,
);

info.add_animation(node_id, anim);
```

The runtime would tick on every frame against the `GpuStateManager` (`layout/src/managers/gpu_state.rs`), interpolate, and push only the changed property into the WebRender display list — no DOM rebuild, no layout pass.

The CSS `transition` and `animation` shorthands would compile down to the same `Animation` struct at parse time, so authors who write CSS animations would get the same runtime for free.

When the runtime lands, this page will be regenerated as a `mature` page with copy-pasteable examples.

## What's missing

- `Animation` / `AnimationId` / `AnimationManager` types
- `CallbackInfo::add_animation` / `remove_animation`
- A frame-driven animation tick separate from the layout tick (so opacity/transform animate without rebuilding the DOM)
- Animation events (`AnimationStart`, `AnimationEnd`) on the `EventFilter` enum
- CSS `transition:` and `animation:` runtime support

If you need animations now, use [Timers](timers.md) and `Update::RefreshDom`. The pattern works; it is just less efficient than a dedicated runtime.

## Next

- [Scrolling and Drag-and-Drop](scrolling-and-drag.md) — the one place easing actually runs (smooth scroll).
- [Timers](timers.md) — what to use until the runtime lands.
