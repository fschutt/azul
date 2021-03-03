![Azul SVG demo](https://i.imgur.com/JQvtmxA.png)

For drawing custom graphics, azul has a high-performance 2D vector API. It also
allows you to load and draw SVG files (with the exceptions of gradients: gradients
in SVG files are not yet supported). But azul itself does not know about SVG shapes
at all - so how the SVG widget implemented?

The solution is to draw the SVG to an OpenGL texture and hand that to azul. This
way, the SVG drawing component could even be implemented in an external crate, if
you really wanted to. This mechanism also allows for completely custom drawing
(let's say: a game, a 3D viewer, etc.) to be drawn.

## Necessary features

SVG rendering has a number of dependencies that are not enabled by default, to save
on compile time in simple cases (i.e. by default the SVG module is disabled, so
that a Hello-World doesn't need to compile those unnecessary dependencies):

```toml
[dependencies.azul]
git = "https://github.com/maps4print/azul"
rev = "..."
features = ["svg", "svg_parsing"]
```

You only need to enable the `svg_parsing` feature if you want to parse SVG files.
For building the documentation, you'll likely want to run `cargo doc --all-features`.
You can import the SVG module using `use azul::widgets::svg::*;`.

## Getting started

The SVG component currently uses  the `resvg` parser, `usvg` simplification and the
`lyon` triangulation libraries). Of course you can also add custom shapes
(bezier curves, circles, lines, whatever) programmatically, without going through
the SVG parser:

```rust
use azul::prelude::*;
use azul::widgets::svg::*;

const TEST_SVG: &str = include_str!("tiger.svg");

impl Layout for Model {
    fn layout(&self, _info: LayoutInfo<Self>) -> Dom<Self> {
        if let Some((svg_cache, svg_layers)) = self.svg {
            Svg::with_layers(svg_layers).dom(&info.window, &svg_cache)
        } else {
             Button::labeled("Load SVG file").dom()
                .with_callback(load_svg)
        }
    }
}

fn load_svg(app_state: &mut AppState<MyAppData>, _: &mut CallbackInfo<MyAppData>) -> UpdateScreen {
    let mut svg_cache = SvgCache::empty();
    let svg_layers = svg_cache.add_svg(TEST_SVG).unwrap();
    app_state.data.modify(|data| data.svg = Some((svg_cache, svg_layers)));
    Redraw
}
```

This is one of the few exceptions where azul allows persistent data across frames
since it wouldn't be performant enough otherwise. Ideally you'd have to load, triangulate
and draw the SVG file on every frame, but this isn't performant. You might have
noticed that the `.dom()` function takes in an extra parameter: The `svg_cache`
and the `info.window`. This way, the `svg_cache` handles everything necessary to
cache vertex buffers / the triangulated layers and shapes, only the drawing itself
is done on every frame.

Additionally, you can also register callbacks on any item **inside** the SVG using the
`SvgCallbacks`, i.e. when someone clicks on or hovers over a certain shape. In order
to draw your own vector data (for example in order to make a vector graphics editor),
you can build the "SVG layers" yourself (ex. from the SVG data). Each layer is
batch-rendered, so you can draw many lines or polygons in one draw call, as long as
they share the same `SvgStyle`.