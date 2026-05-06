---
slug: animations
title: Animations
language: en
canonical_slug: animations
audience: external
maturity: stub
guide_order: 110
topic_only: false
short_desc: CSS transitions and @keyframes
prerequisites: [hello-world, events, timers]
tracked_files:
  - core/src/animation.rs
  - layout/src/timer.rs
  - core/src/task.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T06:00:00Z
---

# Animations

> **Not yet functional.** Azul has no CSS-animation runtime today. `core/src/animation.rs` contains a single 2-variant enum (`UpdateImageType`) and nothing else. CSS `animation:` / `transition:` properties parse but do not interpolate. This page documents the shape the animation runtime is expected to take and the interim pattern that works now: drive interpolation by hand from a [timer](timers.md).

A frame-driven animation is a function of `t ∈ [0.0, 1.0]` that produces a CSS property value. Until the runtime is wired up, you build it by hand: install a timer, compute `t` from the elapsed time, push the new value into the DOM via `info.modify_window_state` or by mutating your model and returning `Update::RefreshDom`.

```rust,ignore
# use azul::prelude::*;
extern "C" fn animate(data: RefAny, info: TimerCallbackInfo) -> TimerCallbackReturn {
    let mut state = data.downcast_mut::<MyState>().unwrap();
    let t = info.frame_start.linear_interpolate(state.start, state.end);
    state.opacity = lerp(0.0, 1.0, t);
    if t >= 1.0 {
        TimerCallbackReturn::terminate_and_refresh_dom()
    } else {
        TimerCallbackReturn::continue_and_refresh_dom()
    }
}
```

## What exists today

`core/src/animation.rs` in full:

```rust,ignore
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd, Eq, Ord, Hash)]
#[repr(C)]
pub enum UpdateImageType {
    Background,
    Content,
}
```

That's the entire module. `UpdateImageType` is a sentinel for image-replacement animations — which image layer of an element a frame update should target — but the surrounding plumbing isn't in place. There is no `Animation` struct, no `Easing` enum, no per-frame interpolation pass, and no integration with the CSS cascade. CSS `animation-*` and `transition-*` properties parse via the CSS parser but the layout solver never reads them.

The `Instant` type does carry the one piece of math the future runtime will need: `Instant::linear_interpolate(start, end) -> f32` returns a clamped 0..=1 fraction (`core/src/task.rs:191`). A real easing library can build on this.

## The interim pattern: animate from a timer

The pattern below is what the animation runtime will eventually replace. It works today and won't disappear when the runtime lands — animations driven by application logic (game state, simulation, custom physics) will always need a timer-based path.

### 1. Pick the property to animate

Anything you can express as a CSS property in your DOM. The cheapest properties to animate are GPU-uploaded ones — opacity and transform — because the layout pass doesn't need to re-run. Width, height, padding, and font-size all force a relayout per frame.

### 2. Stash the animation start time and the target

Put the animation parameters in your model so the timer callback can read them:

```rust,ignore
struct State {
    /// When the current animation started; None when idle
    anim_start: Option<Instant>,
    anim_duration: Duration,
    anim_from_opacity: f32,
    anim_to_opacity: f32,
    /// The current interpolated value the layout callback reads
    current_opacity: f32,
}
```

### 3. Install a timer when the animation should kick off

```rust,ignore
extern "C" fn on_click(data: RefAny, mut info: CallbackInfo) -> Update {
    {
        let mut state = data.downcast_mut::<State>().unwrap();
        state.anim_start = Some(info.get_current_time());
        state.anim_from_opacity = state.current_opacity;
        state.anim_to_opacity = 1.0;
    }
    let timer = Timer::create(data.clone(), animate, info.get_system_time_fn())
        .with_interval(Duration::System(SystemTimeDiff::from_millis(16)));
    info.add_timer(TimerId::unique(), timer);
    Update::DoNothing
}
```

### 4. The timer interpolates and terminates itself

```rust,ignore
extern "C" fn animate(data: RefAny, info: TimerCallbackInfo) -> TimerCallbackReturn {
    let mut state = data.downcast_mut::<State>().unwrap();
    let start = match state.anim_start {
        Some(s) => s,
        None => return TimerCallbackReturn::terminate_unchanged(),
    };
    let end = start.clone().add_optional_duration(Some(&state.anim_duration));
    let t = info.frame_start.linear_interpolate(start, end);
    let eased = ease_out_cubic(t);
    state.current_opacity =
        state.anim_from_opacity + (state.anim_to_opacity - state.anim_from_opacity) * eased;
    if t >= 1.0 {
        state.anim_start = None;
        TimerCallbackReturn::terminate_and_refresh_dom()
    } else {
        TimerCallbackReturn::continue_and_refresh_dom()
    }
}

fn ease_out_cubic(t: f32) -> f32 {
    let inv = 1.0 - t;
    1.0 - inv * inv * inv
}
```

### 5. The layout callback reads the current value

```rust,ignore
extern "C" fn layout(data: RefAny, _: LayoutCallbackInfo) -> StyledDom {
    let state = data.downcast_ref::<State>().unwrap();
    let style = format!("opacity: {};", state.current_opacity);
    Dom::div().with_inline_style(style.into()).style(Css::empty())
}
```

This pattern is the floor — what the framework will provide once the animation runtime is wired is a more declarative version with the same end behaviour.

## Animating images, not the DOM

For animations whose only effect is a pixel change — sprite sheet, video frame, GL texture — `info.update_all_image_callbacks()` re-invokes every `ImageCallback` without touching layout. The `UpdateImageType` enum in `core/src/animation.rs` is the planned per-element variant of this trigger: `Background` will repaint the element's background-image layer, `Content` will repaint the foreground content. Neither variant is wired in yet — `update_all_image_callbacks` is the only available trigger today.

## What the future runtime will provide

Tracked in the manifest; not yet implemented. Once landed, expect:

| Today | Planned |
|---|---|
| Hand-written timer, hand-written easing | `Animation::new(prop, from, to, duration, easing)` value type |
| `f32` time interpolation | `Easing::EaseInOutCubic`, `Bezier(p1, p2)`, etc. |
| Mutate model, `RefreshDom` | GPU-only path for `opacity` / `transform` — no layout pass |
| Manual cleanup in callback | Auto-removal when `t == 1.0` |
| No CSS `transition:` / `animation:` | CSS `transition: opacity 200ms ease-out` triggers an animation when the property changes |

Until then, the timer pattern above is the supported approach. None of the API choices on this page will be invalidated by the runtime — the timer-driven path will remain available for animations driven by application state rather than by CSS.

## Cross-references

- [`timers`](timers.md) — the timer mechanics this page builds on.
- `Instant::linear_interpolate` — `core/src/task.rs:191`.
- The animation runtime issue / planning lives in the autodoc manifest under group `animation` (currently `maturity = stub`).
