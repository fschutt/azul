---
slug: animations
title: Animations
language: en
canonical_slug: animations
audience: external
maturity: wip
guide_order: 100
topic_only: false
short_desc: Transitions, @keyframes, and the enter/exit animation properties
prerequisites: [hello-world, events]
tracked_files:
  - css/src/props/basic/animation.rs
  - core/src/animation.rs
  - layout/src/window.rs
  - core/src/task.rs
last_generated_rev: e33ef81cc56d8a7dac357d6c0610ff5e26d02e26
generated_at: 2026-08-20T00:00:00Z
default-search-keys:
  - StyleAnimation
  - AnimationTiming
  - AnimationTimingBezier
  - AnimationIterationCount
  - Keyframes
  - CssProperty
  - Timer
  - TimerCallbackInfo
  - TimerCallbackReturn
  - Update
---

# Animations

Three properties drive everything the engine animates:

| Property | Runs when | Names |
| --- | --- | --- |
| `animation` | a property's computed value changes between DOM rebuilds | `all`, or the property to scope to |
| `-azul-animation-in` | the node mounts | a `@keyframes` block or an attached function |
| `-azul-animation-out` | the node unmounts | a `@keyframes` block or an attached function |

There is no `transition` property. `animation` is where that job lives.

## The shorthand

```
<name> <duration> [<delay>] [<timing>] [infinite | <count>] [no-clip]
```

```css
animation: all 200ms ease-out;               /* every change transitions */
animation: width 1s, background-color 2s;    /* per-property scopes      */
-azul-animation-in:  slideIn 220ms spring;
-azul-animation-out: slideOut 180ms 50ms ease-in no-clip;
```

The first time value is the duration, the second the delay — CSS order. A
list is read last-match-wins, so a later entry overrides an earlier one for
the properties it covers.

## Transitions

`animation` turns covered property changes into timed interpolations instead
of instant updates. The property list is read off the **old** cascade: adding
`animation` in the same rebuild that changes a value does not retro-animate
that change.

```css
.row          { background-color: #fff; animation: all 150ms ease-out; }
.row.selected { background-color: #dce8ff; }
```

Rebuild the DOM with `selected` on the row and the colour walks over 150 ms.
Change it again mid-flight and the animation **retargets** from its current
value rather than stacking a second run — rapid A→B→C stays smooth. A
transition on a paint-only property (colour, opacity, transform) patches the
display list; one on `width` or `font-size` re-runs layout per frame.

## Presence: enter and exit

`-azul-animation-in` and `-azul-animation-out` animate a node's arrival and
departure. Both name a `@keyframes` block or a function attached to the node,
resolved in that order.

```css
.toast {
    -azul-animation-in:  toastIn 200ms spring-snappy;
    -azul-animation-out: toastOut 150ms ease-in;
}
```

An exit needs something to draw after layout has removed the node, so a
declared `-azul-animation-out` makes the engine retain the previous frame's
subtree for the duration — the zombie. Retention, catching an exit that
remounts mid-flight, and per-frame native animation functions are
[Zombie Animations](animations/zombie-animations.md).

`infinite` on an enter track is how a spinner is expressed; on an exit it is
clamped to one run.

## `@keyframes`

```css
@keyframes toastIn {
    from { transform: translateY(24px); opacity: 0; }
    to   { transform: translateY(0);    opacity: 1; }
}
@keyframes pulse {
    0%   { opacity: 1.0; }
    50%  { opacity: 0.4; }
    100% { opacity: 1.0; }
}
```

Stops accept `from`, `to`, and percentages. The compiled track reads
`transform` (`translate`, `translateX`, `translateY`, `scale`, `rotate`),
`opacity`, `width` and `height`; percentage translations resolve against the
node's own rect. Other properties in a stop are ignored. When two blocks
share a name, the last definition wins.

## Timing

`ease` (the default), `linear`, `ease-in`, `ease-out`, `ease-in-out`,
`cubic-bezier(x1, y1, x2, y2)`, and three springs: `spring`,
`spring-gentle`, `spring-snappy`.

Springs settle on physics rather than on the clock — the declared duration is
their retarget time base, not a stop watch. Reach for one when the animation
can be interrupted (a panel the user re-opens mid-close); reach for a bezier
when the motion has to land on an exact beat.

## Driving an animation yourself

Motion that follows application state — a simulation, a game loop, a value
arriving over the network — belongs in a timer that mutates your model and
returns `Update::RefreshDom` per frame:

```rust,ignore
extern "C" fn animate(data: RefAny, info: TimerCallbackInfo) -> TimerCallbackReturn {
    let mut state = data.downcast_mut::<State>().unwrap();
    let Some(start) = state.anim_start else {
        return TimerCallbackReturn::terminate_unchanged();
    };
    let end = start.clone().add_duration(&state.anim_duration);
    let t = info.frame_start.linear_interpolate(start, end);
    state.current_opacity = state.anim_from + (state.anim_to - state.anim_from) * ease_out(t);
    if t >= 1.0 {
        state.anim_start = None;
        TimerCallbackReturn::terminate_and_refresh_dom()
    } else {
        TimerCallbackReturn::continue_and_refresh_dom()
    }
}
```

`Instant::linear_interpolate(start, end)` returns the clamped `0..=1`
fraction; layer easing on top. The layout callback then reads
`state.current_opacity` and writes it into the style it builds. See
[Timers](animations/timers.md) for scheduling, cancellation, and the 60 fps
pattern end to end.

## Animating images, not the DOM

When the only thing that changes is pixels — a sprite sheet, a video frame, a
GL texture — `info.update_all_image_callbacks()` re-invokes every image
callback without touching layout or the display list.

## Cross-references

- [Zombie Animations](animations/zombie-animations.md): exit retention and
  native per-frame animation functions.
- [Timers](animations/timers.md): the timer mechanics the manual path builds
  on.
- [Reconciliation](dom/reconciliation.md): the node identity that decides
  what counts as a mount, an unmount, and a move.
