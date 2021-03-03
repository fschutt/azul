## Supported CSS attributes

This is a list of CSS attributes that are currently implemented. They work in
the same way as on a regular web page, except if noted otherwise:

- `border-radius`
- `background-color`
- `color` (also as an non-standard alias: `font-color`)
- `letter-spacing`
- `border`, `border-top`, `border-bottom`, `border-left`, `border-right`
- `padding`, `padding-top`, `padding-bottom`, `padding-left`, `padding-right`
- `margin`, `margin-top`, `margin-bottom`, `margin-left`, `margin-right`
- `background`                              [see #1]
- `font-size`
- `font-family`
- `box-shadow`, `box-shadow-top`, `box-shadow-bottom`, `box-shadow-left`, `box-shadow-right` [see #2]
- `line-height`
- `position`: [`static` | `relative` | `absolute`]
- `top`, `left`, `right`, `bottom` (only affects relative and absolute positioned items)
- `width`, `min-width`, `max-width`
- `height`, `min-height`, `max-height`
- `overflow`, `overflow-x`, `overflow-y`
- `text-align`                              [see #3]
- `flex-direction`
- `flex-grow`
- `flex-shrink` (parses, but currently has no effect)
- `flex-wrap` (parses, but currently has no effect)
- `justify-content` (also as an non-standard alias: `align-main-axis`)
- `align-items` (also as an non-standard alias: `align-cross-axis`)

## Supported pseudo selectors

- `:first`
- `:nth-child(x)`
- `:last`
- `:hover` [WIP]
- `:active` [WIP]
- `:focus` [WIP]

You can limit the inheritance of properties either to direct children only (using `>`) or to all children
(using ` `). I.e. `div#my_div.class` has a different effect than `div #my_div .class` or `div > #my_div > .class`.

Notes:

1. `image()` takes an ID instead of a URL. Images and fonts are external resources
    that have to be cached. Use `app.add_image("id", my_image_data)` or
    `app_state.add_image()`, then you can use the `"id"` in the CSS to select
    your image.
    If an image is not present on a displayed div (i.e. you added it to the CSS,
    but forgot to add the image), the following happens:
    - In debug mode, the app crashes with a descriptive error message
    - In release mode, the app doesn't display the image and logs the error
2.  `box-shadow-xxx` are non-standard extensions. They are used to clip a box-shadow to
    only one border of a rectangle. The rule is:
         - If `box-shadow` is specified, a regular box shadow is drawn
         - If `box-shadow-top` and `box-shadow-bottom` are specified, the shadow appears on the
           top and bottom of the rectangle. Same with a pair of `box-shadow-left` / `box-shadow-right`.
         - It isn't possible to display a box shadow on 3 sides, only on one or two
           opposite borders.
         - If all 4 borders are specified, the value for `box-shadow-top` is taken.
3.  Justified text is not (yet) supported
4.  The pseudo selectors marked as [WIP] currently parse, but have no effect.

## Creating CSS stylesheets

When you create a window, you have to give it a certain CSS stylesheet.
This stylesheet **cannot be modified during runtime**, which is an important
limitation. You can, however, create "dynamic properties", which can be dynamically
updated.

Azul has four modes on how the CSS is created:

TODO: The following APIs are outdated since #70 (refactoring CSS parsing into external crate)!

- `Css::new_from_str(&str)` - parses and creates a stylesheet directly
- `Css::native()` - creates the styles from the default, OS-native styles
- `Css::override_native(&str)` - same as `Css::new_from_str`, but applies the user-defined
styles on top of the OS-native styles, i.e. it "overrides" the OS-native styles with user-defined ones.

If you use a stylesheet that is saved to a separate file, Azul provides two extra
modes, which are, however, only available in **debug mode** (with `#[cfg(debug_assertions)]`).

- `Css::hot_reload(FilePath)` - hot-reloads a stylesheet from a file every 500ms, good for RAD.
- `Css::hot_reload_override_native(FilePath)` - same as `hot_reload`, but applies the user-defined
styles on top of the OS-native styles, i.e. it "overrides" the OS-native styles with user-defined ones.

If there are errors during parsing, the `hot_reload` function will print the error and use the
last stylesheet that could be parsed correctly.

## Protected class names

In general, you should avoid defining any CSS classes in your stylesheet that start 
with `__azul-native-` if you use `Css::native()`, `override_native` or `hot_reload_override_native`.
Otherwise, you will override built-in native styles, unless that's what you're going for,
it's better to not name your classes this way.

## Static and dynamic CSS Ids

Azul knows about two types of CSS key-value pairs: static and dynamic. Because
the CSS is only parsed once (at the start of the application), you cannot modify it
during runtime. However, you can specify IDs for certain properties, in order to 
change the style of the application during runtime (for example to change the color 
of a button on a `On::Hover` or `On::MouseDown` event). This API is also used for
animations (TODO) and `:hover`, `:focus` pseudo-selectors.

A static CSS property looks like this:

```css
.my_class {
    width: 500px;
}
```

... while a dynamic property looks like this:

```css
.my_class {
    width: [[ my_property_id | 500px ]];
}
```
The difference is that you can use the ID (in this case: `"my_property_id"` to
change the style dynamically from Rust):

```rust
use azul::prelude::{CssProperty, LayoutWidth};

impl Layout for DataModel {
    fn layout(&self, _info: LayoutInfo<Self>) -> Dom<Self> { 
        Dom::new(NodeType::Div)
        .with_class("my_class")
        .with_css_override("my_property_id", CssProperty::Width(LayoutWidth::px(700.0)))
    }
}
```

This will "override" the CSS property to 700 px on the specific div. If the `.with_css_override`
isn't present, the width would be 500px wide (every `DynamicCssProperty` need a "default" case).
If you want to specify the default width to be automatically inferred from how much space there
is left in the parent node, you can use `auto` like this:

```css
.my_class {
    width: [[ my_property_id | auto ]];
}
```

For example, this is useful if you want to have panels that expand an contract on clicking a button or
if you want to drag content (so you need to overwrite `margin-top / margin-left to actually move the div)
in the layout.

Notice that we don't have to un-set the width again. This is because dynamic
CSS properties only stay active for one frame, and need to be re-applied on each
frame - this is a concious design choice to not make the visual style depend on a
sequence of actions that have to be executed and thereby make the style more
unit-testing friendly.

Also notice: If the `set_dynamic_property` gets a different type for your ID (let's say you
typed `height` instead of `width`, it will not override the target value, but print an 
error (if logging is enabled)).

## Layout system

The layout model follows closely to the CSS flexbox model (although many properties aren't implemented yet):

- align the content across the main axis with `flex-direction`
- If the width and height is not constrained, the rectangle will take up as much width
  and height as possible, by default the width / height of the parent
- Nest the layout to get more intricate and complicated layouts
- When a rectangle is marked as `position: absolute`, it will search up the DOM tree for the nearest
  `position: relative` or `position: absolute` rectangle and use the `top, left, right, bottom` properties
  to position itself relative to that rectangle
- Z-indexing is currently only determined by the stacking order, there is no `z-index` attribute yet
- `inline` blocks are non-existent. The reason for this is that if you need inline blocks, you'll likely
  need a bit more complicated layout. For this purpose, you can use absolute and relative positioning if you
  want to lay rectangles behind the text and use the text metrics themselves to calculate the offsets.
  For regular inline-block content, just use `flex-direction` with a wrapper div.

## Remarks

1. All measurements respect HiDPI screens and azul is fully HiDPI aware. Em values
   are measured as `1em = 16px`
2. CSS key-value pairs are parsed from top to bottom, and get overwritten in 
   that order, for example:
   ```css
   #my_div {
       background: image("Cat01");
       background: linear-gradient("105deg, red, blue");
   }
   ```
   ... will draw a linear gradient, not the image, since the `linear-gradient` value
   overwrote the `image` rule.
3. Attributes in CSS paths are not (and aren't planned to be) supported,
   since this is a fundamental difference between how HTML and the `layout()` 
   function work.
5. `auto` as a value is only valid inside of dynamic CSS attributes, currently
   values such as `auto` and `inherit` do not work as general values.

Animations are planned to be implemented with `@keyframes`, but this is not yet supported yet.
You can create preliminary "animations" by using the dynamic CSS properties, but be aware that
every repaint with dynamic CSS Ids requires a full re-layout, which can be performance intensive.