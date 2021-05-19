## WebRender and OpenGL

The general concept of interacting between OpenGL and webrender is fairly simple - you draw to a texture,
hand it to webrender and webrender draws it when necessary. What can be quite confusing however, is the
way that the process of drawing to an OpenGL texture is structured.

The problem, however, is that if we'd allow directly pushing a `Texture` into the DOM, there would be
no way of knowing how large that texture needs to be, since the size can depend on the size and number
of its sibling DOM nodes.

We need to know the size of the texture for aspect ratio correction and preventing stretched / blurry
textures. Since the size of the rectangle that the texture should cover isn't known until it is time to
layout the frame, we have to "delay" the rendering of the texture. In azuls case, the DOM-building step
simply pushes a `GlTextureCallback` instead of the texture itself, i.e. a function that will render
the texture in the future (after the layout step).

The definition of a `GlTextureCallback` is the following:

```rust
pub struct GlTextureCallback<T: Layout>(pub fn(&StackCheckedPointer<T>, LayoutInfo<T>, HidpiAdjustedBounds) -> Option<Texture>);
```

The `HidpiAdjustedBounds` contains the `width, height` of the desired texture as well as HiDPI information
that you might need to scale the content of your texture correctly. If the callback returns `None`, the
result is simply a white square. The `LayoutInfo` allows you to create an OpenGL texture like this:

```rust
let mut texture = window_info.window.create_texture(
    hi_dpi_bounds.physical_size.width as usize,
    hi_dpi_bounds.physical_size.height as usize);
```

This creates an empty, uninitialized GPU texture. Note that we use the `physical_size` instead of
the `logical_size` - the "logical size" is the HiDPI-adjusted version (which is usually what you want
to calculate UI metrics), but the physical size is necessary in this case to provide the actual size of
the texture, without a HiDPI scaling factor.

Next, we clear the texture with the color red (255, 0, 0) and return it:

```rust
texture.as_surface().clear_color(1.0, 0.0, 0.0, 1.0);
return Some(texture);
```

Here, the `.as_surface()` activates the texture as the current FBO and draws to it. In this case we
only clear the texture and return it, but of course, you can do much more here - upload and draw
vertices and textures, activate and bind shaders, etc. See the `Surface` trait for more details.

Azul requires at least OpenGL 3.1, which is checked at startup. You can rely on any function of
OpenGL 3.1 being available at the time the callback is called.

Here is a simple, full example:

```rust
impl Layout for OpenGlAppState {
    fn layout(&self, _info: LayoutInfo) -> Dom<Self> {
        // See below for the meaning of StackCheckedPointer::new(self)
        Dom::new(NodeType::GlTexture(GlTextureCallback(render_my_texture), StackCheckedPointer::new(self)))
    }
}

fn render_my_texture(
    state: &StackCheckedPointer<OpenGlAppState>,
    info: LayoutInfo<OpenGlAppState>,
    hi_dpi_bounds: HidpiAdjustedBounds)
-> Option<Texture>
{
    let mut texture = info.window.create_texture(
        hi_dpi_bounds.physical_size.width as usize,
        hi_dpi_bounds.physical_size.height as usize);

    texture.as_surface().clear_color(0.0, 1.0, 0.0, 1.0);
    Some(texture)
}
```

This should give you a window with a red texture spanning the entire window. Remember than if a
`Div` isn't limited in width / height, it will try to fill its parent, in this case the entire window.
Try adding another `Div` in the `layout()` function and laying them out horizontally via CSS. You can
decorate, skew, etc. your texture with CSS as you like. You can even use clipping and borders - OpenGL
textures get treated the same way as regular images.

## Using and updating the components state in OpenGL textures

Let's say you have a component that draws a cube to an OpenGL texture. You want to update the
cube's rotation, translation and scale from another UI component. How would you implement such a widget?

By now you have probably noticed the `StackCheckedPointer<T>` that gets passed into the callback.
This allows you to build a stack-allocated "component" that takes care of rendering itself, without
the user calling any rendering code. As always, be careful to cast the pointer back to the type you
created it with (see the [https://github.com/maps4print/azul/wiki/Two-way-data-binding] chapter on
why `StackCheckedPointer` is unsafe and how to migitate this problem to build a type-safe API).

For example, we want to draw a cube, and control its rotation and scaling from another UI element.
So we build a `CubeControl` that renders the cube and exposes an API to control the cubes rotation
from any other UI element:

```rust
// This is your "API" that other UI elements will mess with. For example, a user could hook up a button
// to increase the scale by 0.1 every time a button is clicked.
//
// The point is that the CubeControl is just a dumb struct, it doesn't know about any other component.
// The CubeControl contains all the "state" necessary for your renderer, the renderer itself doesn't know
// about the state itself
pub struct CubeControl {
    pub translation: Vector3,
    pub rotation: Quaternion,
    pub scaling: Vector3,
}

// This is your "rendering component", i.e. the thing that generates the DOM.
// The procedure is similar to how you'd use a regular StackCheckedPointer
#[derive(Default)]
pub struct CubeRenderer { /* no state! */ }

impl CubeRenderer {
    // The DOM stores the pointer to the state of the renderer. This state may be modified
    // by other UI controls before the GlTextureCallback is invoked.
    pub fn dom<T: Layout>(data: &CubeControl) -> Dom<T> {
         // Regular two-way data binding. Yes, you should use unwrap() here, since StackCheckedPointer
         // will only fail if the data is not on the stack.
         //
         // Think of this as a StackCheckedPointer<CubeControl> - internally the `<CubeControl>` type is erased
         // and you need to cast it back manually in the rendering callback
         let ptr = StackCheckedPointer::new(data);
        Dom::new(NodeType::GlTexture(GlTextureCallback(Self::render), ptr))
    }

    // Private rendering function. External code doesn't need to know or care how the
    // `CubeRenderer` renders itself.
    fn render<T: Layout>(
        state: &StackCheckedPointer<T>,
        info: LayoutInfo<T>,
        bounds: HidpiAdjustedBounds)
    {
        // Important: The type of the StackCheckedPointer has been erased
        // Casting the pointer back to anything else than a &mut CubeControl will invoke undefined behaviour.
        // HOWEVER: This function is (and should be) private. Only you, the **creator** of this component
        // can invoke UB, not the user.
        //
        // The way that the pointer is casted back is by giving it a function that has the same signature
        // as the render() function, but with a `&mut CubeControl` instead of a `&StackCheckedPointer<T>`.
        // You do not have to worry about aliasing the pointer or race conditions, that is what the
        // StackCheckedPointer takes care of.
        fn render_inner(component_state: &mut CubeControl, info: LayoutInfo<T>, bounds: HidpiAdjustedBounds) -> Texture {
             let texture = info.window.create_texture(width as u32, height as u32);
             // render_cube_to_surface (not included in this example for brevity) takes the texture
             // **and the current state of the component** and draws the cube on the surface according to the state
             render_cube_to_surface(texture.as_surface(), &component_state);
             // You could update your component_state here, if you'd like.
             texture
        }

        // Cast the StackCheckedPointer to a CubeControl, then invoke the render_inner function on it.
        Some(unsafe { state.invoke_mut_texture(render_inner, state, info, bounds) })
    }
}
```

Now, why is this so complicated? The answer is that now the API for the user of this component is very easy:

```rust
// The data model of the final program
struct DataModel {
    // Stores the state of the cubes rotation, scaling and translation.
    // The user doesn't need to know about any other details
    cube_control: CubeControl,
}

impl Layout for DataModel {
    fn layout(&self, _info: LayoutInfo<Self>) -> Dom<Self> {
        CubeRenderer::default().dom(&self.cube_control)
    }
}
```

That's it! Now the user can hook up other components or custom callbacks that modify `self.cube_control` -
but the application data is cleanly seperated from its view or other components.

The user does also not need to care about how the component (in this case our `CubeRenderer`) renders
itself, it is done "magically" by the framework - the framework determines when to call the
`GlTextureCallback` and does so behind the  back of the user. There is **no code** that the user
has to write in order to render a `CubeRenderer`. The only limitation is that the `CubeControl`
has to be stack-allocated, it can't be stored in a `Vec` or similar (because then, azul can't
reason about the lifetime of the component, to make sure it's not dereferencing a dangling pointer).

## What is `StackCheckedPointer::new(self)` ?

If you followed closely, you can probably already see what this is doing: `StackCheckedPointer` takes a reference to something on the stack that is contained in `T`. However, it can also take a reference to the **entire data model** (i.e. `T` itself). So `StackCheckedPointer::new(self)` essentially builds a `StackCheckedPointer<OpenGlAppState>` - the pointer can be safely casted back to a `OpenGlAppState`,
at which point the callback has full control over the entire data model. Usually this is only
something you'd want to do for prototyping, it's better for maintentance to build a custom
component as shown above, for type safety reasons.

## Raw OpenGL

For ease of use, azul exposes primitives of the `glium` library, which provide functions, such as
for example `.clear_color()` - usually it's easier to work with that than with raw OpenGL - glium
provides primitives for GLSL shader compilation and linking.

Right now OpenGL is the only supported backend and that will probably stay this way in the future -
since webrender is only portable to platforms that can target OpenGL, it wouldn't make sense to
support other rendering backends, since webrender, the main appeal of this entire library,
wouldn't run on them. There are experiments of porting webrender to Vulkan / DirectX or Metal,
however these are, as the name implies, experimental indeed.

## Notes

- It is not possible to render directly to the screen (for example, to use the built-in MSAA).

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