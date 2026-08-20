---
slug: animations/zombie-animations
title: Zombie Animations
language: en
canonical_slug: animations/zombie-animations
audience: external
maturity: wip
guide_order: 102
topic_only: false
short_desc: Exit animations, the retained subtree they run on, and native per-frame animation functions
prerequisites: [animations, dom/reconciliation]
tracked_files:
  - core/src/resources.rs
  - core/src/animation.rs
  - layout/src/window.rs
  - layout/src/callbacks.rs
  - css/src/props/basic/animation.rs
last_generated_rev: e33ef81cc56d8a7dac357d6c0610ff5e26d02e26
generated_at: 2026-08-20T00:00:00Z
default-search-keys:
  - ZombieAnimCallback
  - ZombieAnimInfo
  - ZombieFrame
  - AnimationFunction
  - StyleAnimation
  - AnimationTiming
  - AnimationIterationCount
  - TimerCallbackInfo
  - RefAny
---

# Zombie Animations

An exit animation animates a node that no longer exists. By the time the
layout callback has returned the DOM without it, reconciliation has already
unmounted it and layout has given its space to its neighbours — there is
nothing left to move. `-azul-animation-out` is what stops that: the node's
declaration makes the window **retain** the previous frame's subtree, and the
exit animates against that retained copy. The retained copy is the zombie.

```css
.sidebar {
    -azul-animation-in:  slideInLeft 220ms spring;
    -azul-animation-out: slideOutLeft 180ms ease-in;
}
@keyframes slideOutLeft {
    from { transform: translateX(0); }
    to   { transform: translateX(-100%); opacity: 0; }
}
```

## Retention is opt-in

A node without `-azul-animation-out` disappears the frame it unmounts — no
zombie, no retained tree, no engine-default slide. Departure animations are
something you or a widget library declare; nothing happens by default, and
the common unmount keeps its incremental layout entry untouched.

Retention also needs a *resolvable* name and a rect: a name that matches
neither a `@keyframes` block nor an attached animation function contributes
nothing, and a node that never laid out has nothing to animate from. Both
cases unmount instantly.

## The zombie owns the frame it animates

When at least one exit resolves, the previous frame's layout result is
**moved** out of the window's live results into the zombie — not cloned, not
shared. Three consequences you can rely on:

- **Hit-testing skips it by construction.** Input walks the live layout
  results, and the zombie is no longer among them. Clicks land on the new
  tree; your callbacks never see the departing node.
- **Scroll offsets freeze.** The retained frame keeps the scroll positions it
  was showing, so a list that scrolls under a departing panel does not yank
  the panel's content with it.
- **It is freed exactly once**, when the track finishes.

An `infinite` exit is clamped to a single run — an exit that never ends would
never reap its zombie. `infinite` on `-azul-animation-in` is not clamped, and
is how a spinner is expressed.

## Clipping

By default an exit is clipped to the rect it was retained at, so a
translating zombie cannot paint over the neighbours that have already taken
its space. The `no-clip` keyword turns that off for a declaration:

```css
-azul-animation-out: flyAway 400ms ease-out no-clip;
```

A native animation function can override it per frame through
`ZombieFrame::clip_to_frozen_rect`.

## Remounting mid-exit

Toggle a panel off and on again before its exit finishes and the exit is
*caught* rather than restarted. The retained node is keyed by its
reconciliation identity, so the remounted node is recognised as the same
node: the zombie is dropped (no double image), and the live node travels home
from wherever the exit had carried it, with velocity preserved.

The direction it travels home along is `-azul-animation-in` if the new
cascade declares one. If it does not, the engine reverses the out-track over
the time the exit had already spent, so the return runs at the pace the
departure did.

The mirror case works too: a node that unmounts mid-*enter* starts its
out-track from wherever the in-track had carried it, instead of snapping to
its laid-out position first.

## Native animation functions

`@keyframes` covers declarative motion. When a frame has to be *computed* —
physics, a value read off the live DOM, motion that depends on how far the
last gesture threw the panel — attach a function to the node and name it from
CSS:

```rust,ignore
use azul::prelude::*;

extern "C" fn shrink_out(
    _data: &mut RefAny,
    _info: &mut TimerCallbackInfo,
    z: &ZombieAnimInfo,
) -> ZombieFrame {
    // `z.t` is RAW LINEAR progress; the engine does not pre-apply easing to
    // native functions. `evaluate` applies the curve the CSS asked for.
    let eased = z.timing.evaluate(z.t);
    ZombieFrame {
        translate_x: -z.rect.size.width * eased,
        translate_y: 0.0,
        opacity: 1.0 - eased,
        width: OptionF32::Some(z.rect.size.width * (1.0 - eased)),
        clip_to_frozen_rect: true,
    }
}

fn sidebar() -> Dom {
    Dom::create_div()
        .with_class("sidebar".into())
        .with_animation_callback(
            "shrinkOut".into(),
            ZombieAnimCallback { cb: shrink_out as usize },
            RefAny::new(()),
        )
}
```

```css
.sidebar { -azul-animation-out: shrinkOut 300ms ease-in-out; }
```

The function lives on the node's own `NodeData`, not in app-global state: a
widget ships its fly-out next to its own DOM, and a library can hand you a
component whose motion travels with it.

**Name resolution order:** stylesheet `@keyframes` first, attached functions
second. The web mechanism stays the only default name source — a `@keyframes
shrinkOut` block anywhere in the stylesheet shadows the attached
`"shrinkOut"`. There are no engine builtins; an unresolvable name animates
nothing.

### What the function receives

`ZombieAnimInfo` is the zombie-specific half:

| Field | What it is |
| --- | --- |
| `styled_dom`, `node_id` | The tree the animated node lives in — retained for an exit, live for an enter — and its index in that tree. Borrowed for the call: walk it, don't store it. |
| `rect`, `viewport`, `dpi_factor` | The retained rect for exits, the solved rect for enters; the viewport it was laid out in. |
| `t` | Raw linear progress, `0..=1`. Not eased. |
| `timing` | The timing the CSS declared (`ease`, `spring`, `cubic-bezier(…)`). Apply it with `timing.evaluate(t)` or substitute your own curve. |
| `velocity_x`, `velocity_y` | Speed entering this frame, logical px/s. Reversing with continuity means emitting frames that start at this speed. |

The other half is a full `TimerCallbackInfo` — the same thing a timer
callback gets. The live DOM, the change queue, node measurement and the
momentum API are all reachable from inside an animation frame.

### What it returns

`ZombieFrame` holds absolute values, not deltas:

- `translate_x` / `translate_y` — logical px about the node's origin.
- `opacity` — `0.0` skips the node entirely for that frame.
- `width` — absolute painted width; left-anchored narrowing for exits. `None`
  keeps the full width. Ignored for enters, where the live layout owns width.
- `clip_to_frozen_rect` — per-frame override of the CSS `no-clip` keyword.
  Ignored for enters.

## Cost

A zombie is a whole retained frame — its own layout result, its own text and
layout caches. Tracks that change `width` re-solve that retained tree per
frame; `LayoutWindow::zombie_relayouts` counts how often that happened, which
is the number to watch if an exit animation costs more than it should. Tracks
that only translate or fade never re-solve anything.

## Cross-references

- [Animations](../animations.md): the `animation` and `-azul-animation-*`
  properties, timing keywords, and `@keyframes`.
- [Reconciliation](../dom/reconciliation.md): the identity that lets a
  remount recognise its own zombie.
- [Timers](timers.md): `TimerCallbackInfo`, which every animation function
  receives in full.
