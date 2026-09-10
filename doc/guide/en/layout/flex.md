---
slug: layout/flex
title: Flexbox
language: en
canonical_slug: layout/flex
audience: external
maturity: mature
guide_order: 83
topic_only: false
short_desc: One-axis container layout with grow/shrink/basis
prerequisites: [layout]
tracked_files:
  - css/src/props/layout/flex.rs
  - css/src/props/layout/spacing.rs
last_generated_rev: 4a4eb85b8c8943cc13bff93b797f0b795c23069c
generated_at: 2026-05-02T05:49:28Z
default-search-keys:
  - Dom
  - Css
  - CssProperty
  - StyledDom
---

# Flexbox

## Overview

`display: flex` and `display: inline-flex` lay out children along one axis and align them on the other. The axis is set by `flex-direction`. The remaining properties decide how free space is distributed and where items align.

```html
<div style='display: flex; gap: 8px; padding: 8px;'>
   <div>one</div>
   <div>two</div>
   <div>three</div>
 </div>
```

## Container properties

These apply to the element with `display: flex` or `display: inline-flex`.

### flex-direction

* `row` (default). Main axis is horizontal. Items run first to last.
* `row-reverse`. Main axis is horizontal. Items run last to first.
* `column`. Main axis is vertical. Items run first to last.
* `column-reverse`. Main axis is vertical. Items run last to first.

`row` makes the inline axis the main axis. `column` makes the block axis the main axis.

### flex-wrap

* `nowrap` (default). Forces a single line for all children - items shrink to fit.
* `wrap`. Overflowing children wrap to a new line.
* `wrap-reverse`. Wraps in reverse cross-axis order.

```html
<div style='display: flex; flex-wrap: wrap; gap: 8px;'>
   <div style='width: 120px;'>one</div>
   <div style='width: 120px;'>two</div>
   <div style='width: 120px;'>three</div>
 </div>
```

### justify-content: main-axis alignment

`justify-content` distributes free space along the main axis.

* `start` (default). Items pack to the absolute start of the axis.
* `end`. Items pack to the absolute end of the axis.
* `flex-start`. Items pack to the flex-relative start. 
  * Under `row-reverse` or `column-reverse`, this equals `end`.
* `flex-end`. Items pack to the flex-relative end. 
  * Under `row-reverse` or `column-reverse`, this equals `start`.
* `center`. Free space splits equally on both ends.
* `space-between`. Space between items, none at ends.
* `space-around`. Half-space at ends, full between.
* `space-evenly`. Equal space everywhere.

`start` and `flex-start` are distinct variants and do not behave the same in reversed directions, i.e. in RTL contexts. The same applies to `end` and `flex-end`.

### align-items: cross-axis alignment for every line

* `stretch` (default). Fills the cross axis.
* `start` / `flex-start`. Aligns to cross-start.
* `end` / `flex-end`. Aligns to cross-end.
* `center`. Centres on cross axis.
* `baseline`. Aligns text baselines.

Unlike `justify-content`, the `align-items` property aliases `start` to `flex-start` and `end` to `flex-end`.

### align-content: cross-axis alignment between lines

Takes effect when `flex-wrap: wrap` produces multiple lines. `align-content` accepts:

* `stretch` (default).
* `center`.
* `start` / `flex-start`.
* `end` / `flex-end`.
* `space-between`.
* `space-around`.

## Item properties

These apply to children of a flex container.

### flex-grow and flex-shrink

`flex-grow` is a non-negative number, default `0`. `flex-shrink` is its mirror, default `1`.

When the container has free space on the main axis, items split it in proportion to `flex-grow`. When the container overflows, items shrink in proportion to `flex-shrink`.

```html
<div style='display: flex;'>
   <div style='flex-grow: 1;'>fills available</div>
   <div style='flex-grow: 2;'>fills twice as much</div>
   <div>auto-sized</div>
 </div>
```

### flex-basis

* `auto` (default). Uses `width` or `height`.
* `<length>`. Fixed size before grow/shrink applies.

`flex-basis` defines the initial size of the item before space distribution. It accepts `px`, percentages, `em`, `rem`, and `pt`. Setting a non-`auto` basis completely overrides the item's `width` property.

### flex shorthand

The `flex` property combines `flex-grow`, `flex-shrink`, and `flex-basis`.

* `flex: none` sets grow to `0`, shrink to `0`, and basis to `auto`.
* `flex: <number>` sets grow. Shrink defaults to `1` and basis becomes `0px`.
* `flex: <basis>` sets basis.
* `flex: <grow> <shrink>` sets grow and shrink. Basis becomes `0px`.
* `flex: <grow> <basis>` sets grow and basis. Shrink defaults to `1`.
* `flex: <grow> <shrink> <basis>` sets all three.

Setting `flex: 1` computes to `flex-basis: 0px`. This overrides the default `auto` basis and removes the element's natural width from the distribution calculation.

### align-self: override align-items for one item

`align-self` overrides the container's `align-items` for a specific item. It accepts:

* `auto` (default). Inherits the container's `align-items`.
* `stretch`, `center`, `start` / `flex-start`, `end` / `flex-end`, `baseline`.

```html
<div style='display: flex; align-items: stretch;'>
   <div>stretches</div>
   <div style='align-self: center;'>centred only</div>
 </div>
```

## Recipes

### Sidebar + content

```azul-render screenshot=flex-sidebar width=480 height=200 subtitle="Fixed sidebar with growing content area"
<body style="font-family: sans-serif;">
  <div style="display: flex; height: 180px; gap: 8px;">
    <div style="width: 120px; background: #e0e7ff; padding: 8px;">sidebar</div>
    <div style="flex-grow: 1; background: #f5f3ff; padding: 8px;">content fills the rest</div>
  </div>
</body>
```

### Equal columns

```azul-render screenshot=flex-equal width=480 height=160 subtitle="Three equal columns via flex-grow: 1"
<body style="font-family: sans-serif;">
  <div style="display: flex; gap: 8px; padding: 8px;">
    <div style="flex-grow: 1; background: #fce7f3; padding: 8px;">A</div>
    <div style="flex-grow: 1; background: #fbcfe8; padding: 8px;">B</div>
    <div style="flex-grow: 1; background: #f9a8d4; padding: 8px;">C</div>
  </div>
</body>
```

### Centred content

```azul-render screenshot=flex-center width=400 height=200 subtitle="Both axes centred"
<body style="font-family: sans-serif;">
  <div style="display: flex; justify-content: center; align-items: center; height: 180px; background: #ecfeff;">
    <div style="background: #a5f3fc; padding: 16px;">centred</div>
  </div>
</body>
```

### Wrapping cards

```azul-render screenshot=flex-wrap width=480 height=240 subtitle="Cards wrap onto multiple lines as the container narrows"
<body style="font-family: sans-serif;">
  <div style="display: flex; flex-wrap: wrap; gap: 12px; padding: 8px;">
    <div style="width: 140px; background: #fef3c7; padding: 12px;">card 1</div>
    <div style="width: 140px; background: #fde68a; padding: 12px;">card 2</div>
    <div style="width: 140px; background: #fcd34d; padding: 12px;">card 3</div>
    <div style="width: 140px; background: #fbbf24; padding: 12px;">card 4</div>
  </div>
</body>
```

## Default values at a glance

* `flex-direction` defaults to `row`.
* `flex-wrap` defaults to `nowrap`.
* `justify-content` defaults to `start`.
* `align-items` defaults to `stretch`.
* `align-content` defaults to `stretch`.
* `flex-grow` defaults to `0`.
* `flex-shrink` defaults to `1`.
* `flex-basis` defaults to `auto`.
* `align-self` defaults to `auto`.
* `gap` defaults to `0`.
