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

> **Not yet functional.** Azul has no CSS-animation runtime today. CSS `animation:` and `transition:` properties parse but don't interpolate. This page documents the shape of the user-facing API. Until the runtime is wired up, drive interpolation by hand from a [timer](timers.md).

The user-facing API is plain CSS. You write `transition: opacity 200ms ease-out` or a `@keyframes` block, and the framework interpolates between values. Until the runtime lands, the same effect is achieved with a [timer](timers.md) that mutates your model and returns `Update::RefreshDom` per frame.

## CSS transitions (planned)

```css
.button {
    opacity: 0.5;
    transition: opacity 200ms ease-out;
}
.button:hover {
    opacity: 1.0;
}
```

When `:hover` toggles, `opacity` interpolates from `0.5` to `1.0` over 200 milliseconds with an `ease-out` curve. Multiple properties separate with commas:

```css
transition: opacity 200ms ease-out, transform 300ms ease-in-out;
```

The cheapest properties to animate are GPU-uploaded ones (opacity, transform) because the layout pass doesn't need to re-run. Width, height, padding, and font-size force a relayout per frame.

## CSS keyframes (planned)

```css
@keyframes pulse {
    0%   { opacity: 1.0; }
    50%  { opacity: 0.4; }
    100% { opacity: 1.0; }
}
.notice {
    animation: pulse 1s infinite;
}
```

`@keyframes` blocks define named animations. Apply them with the `animation:` shorthand or its longhands.

## What works today: animate from a timer

This pattern is the floor. Once the animation runtime is wired, the framework will provide a more declarative version of the same thing. Animations driven by application logic (game state, simulation, custom physics) will always need a timer-based path.

### 1. Pick the property to animate

Anything you can express as a CSS property in your DOM. Prefer GPU-uploaded properties (opacity, transform) for the same reason as above.

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
    let end = start.clone().add_duration(&state.anim_duration);
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

`Instant::linear_interpolate(start, end)` returns a clamped 0..=1 fraction. Layer easing on top.

### 5. The layout callback reads the current value

```rust,ignore
extern "C" fn layout(data: RefAny, _: LayoutCallbackInfo) -> StyledDom {
    let state = data.downcast_ref::<State>().unwrap();
    let style = format!("opacity: {};", state.current_opacity);
    Dom::create_div().with_css(&style).style(Css::empty())
}
```

The timer-driven path stays available even after the CSS runtime lands. Use it for animations driven by application state rather than CSS rules.

## Animating images, not the DOM

For animations whose only effect is a pixel change (sprite sheet, video frame, GL texture), `info.update_all_image_callbacks()` re-invokes every image callback without touching layout.

## Cross-references

- [`timers`](timers.md): the timer mechanics this page builds on.

## Coming Up Next

- [Events](events.md) — Callbacks, event filters, and how state triggers relayout
- [Timers](timers.md) — Timers, threads, and scheduled work
- [Scrolling](scrolling-and-drag.md) — Scroll containers, drag-and-drop, hit testing
