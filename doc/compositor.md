To support this "load function pointers" approach without external dependencies in your core API, while enabling the "intrinsic size" behavior you described, we need to treat these windows exactly like `Image` nodes but with a dynamic source.

Here is the design to adjust your API **now** so you can implement the raw C function pointer logic **later**.

### 1. The DOM Structures (`dom.rs`)

We will add two new structs. We won't put a `tiling` boolean on them. Instead, we rely on the layout engine treating them as content with an **intrinsic aspect ratio and size**.

*   **If CSS is `width: auto; height: auto`:** The node asks the backing window "How big do you want to be?" (Floating / Client-controlled).
*   **If CSS is `flex: 1` or `width: 500px`:** The node forces the size, and the backend sends a resize event to the client (Tiling / Compositor-controlled).

Add this to your `dom.rs`:

```rust
/// A handle to a Wayland Surface (client window).
///
/// The sizing behavior depends on the CSS applied to this node:
/// - `width: auto` / `height: auto`: The node adopts the client's requested size (Floating).
/// - Defined constraints (e.g., `flex: 1`, `width: 500px`): Azul dictates the size (Tiling).
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct WaylandSurfaceNode {
    /// The internal ID assigned by the compositor when the surface was created.
    /// This ID is usually provided via a `WaylandSurfaceCreated` event.
    pub surface_id: u64,
}

/// A handle to an X11 Window.
///
/// The sizing behavior depends on the CSS applied to this node:
/// - `width: auto` / `height: auto`: The node adopts the X11 window's current size.
/// - Defined constraints: Azul forces the X11 window to resize to fit the node.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct X11WindowNode {
    /// The raw X11 Window ID (XID).
    pub xid: u64,
}
```

### 2. Update `NodeType`

Update the enum to include these new types.

```rust
#[derive(Debug, Clone, PartialEq, Hash, Eq, PartialOrd, Ord)]
#[repr(C, u8)]
pub enum NodeType {
    // ... existing variants ...

    // Special content types
    Text(AzString),
    Image(ImageRef),
    IFrame(IFrameNode),
    
    /// A Wayland Surface rendered as a texture
    WaylandSurface(WaylandSurfaceNode),
    /// An X11 Window rendered as a texture
    X11Window(X11WindowNode),
}
```

### 3. Update `NodeType` Implementation

We need to update `into_library_owned_nodetype` and `format` to handle the new variants.

```rust
impl NodeType {
    fn into_library_owned_nodetype(&self) -> Self {
        use self::NodeType::*;
        match self {
            // ... existing ...
            WaylandSurface(w) => WaylandSurface(*w),
            X11Window(w) => X11Window(*w),
        }
    }

    pub fn format(&self) -> Option<String> {
        use self::NodeType::*;
        match self {
            Text(s) => Some(format!("{}", s)),
            Image(id) => Some(format!("image({:?})", id)),
            IFrame(i) => Some(format!("iframe({:?})", i)),
            WaylandSurface(w) => Some(format!("wayland-surface({})", w.surface_id)),
            X11Window(w) => Some(format!("x11-window({:#x})", w.xid)),
            _ => None,
        }
    }
    
    // For CSS selector matching, we can treat them generally as "embedded content" 
    // or specific tags.
    pub fn get_path(&self) -> NodeTypeTag {
        match self {
            // ...
            Self::WaylandSurface(_) => NodeTypeTag::Canvas, // Acts like a canvas
            Self::X11Window(_) => NodeTypeTag::Canvas,
            // ...
        }
    }
}
```

### 4. Update `Dom` Constructors

Add the builder methods to the `Dom` struct so users can easily create these nodes.

```rust
impl Dom {
    // ... existing ...

    /// Creates a node that renders a Wayland surface.
    ///
    /// # Sizing
    /// Treat this node like an `<img>`. If you do not set a CSS width/height,
    /// it will expand to the size requested by the Wayland client.
    /// If you set a CSS size (or use Flexbox stretching), the Wayland client 
    /// will be resized to fit.
    #[inline]
    pub fn wayland_surface(surface_id: u64) -> Self {
        Self::new(NodeType::WaylandSurface(WaylandSurfaceNode { surface_id }))
    }

    /// Creates a node that renders an external X11 window.
    ///
    /// # Sizing
    /// Treat this node like an `<img>`. If you do not set a CSS width/height,
    /// it will expand to the size of the X11 window.
    /// If you set a CSS size (or use Flexbox stretching), the X11 window 
    /// will be resized to fit.
    #[inline]
    pub fn x11_window(xid: u64) -> Self {
        Self::new(NodeType::X11Window(X11WindowNode { xid }))
    }
}
```

---

### 5. Implementation Roadmap (The "How-To" for later)

Since you want to use the "load function pointers" approach later, here is how you will map the backend logic to this API.

#### Wayland (via `libwayland-server.so`)
You will dynamically load `libwayland-server.so`. This C library provides the primitives to create a display server.

1.  **Intrinsic Size (The Layout Phase):**
    *   When Azul calculates layout, it encounters `NodeType::WaylandSurface`.
    *   It looks up the `xdg_surface` associated with that ID in your compositor state.
    *   If the DOM style is `Auto`, Azul reads `current_geometry.width/height` from the surface state and uses that as the node size.
    *   If the DOM style is `Fixed`, Azul uses the CSS size.

2.  **Resize Logic (The Render Phase):**
    *   After layout, Azul compares the *calculated* node size vs. the *surface* size.
    *   If they differ, you call the raw function pointer `xdg_toplevel_send_configure(resource, width, height, states)`.
    *   This forces the client to redraw at the new size (Tiling).

#### X11 (via `libX11.so` and `libXcomposite.so`)
You will dynamically load X11 libraries.

1.  **Intrinsic Size:**
    *   Azul calls `XGetGeometry(display, xid, ...)` via function pointer.
    *   Returns the width/height to the layout engine if CSS is `Auto`.

2.  **Resize Logic:**
    *   If CSS dictates a specific size, Azul calls `XMoveResizeWindow(display, xid, ...)` during the render phase.
    *   For X11, you must handle the `ConfigureRequest` event manually in your event loop. If the specific window ID corresponds to a node that has "Fixed/Flex" CSS, you **deny** the request and enforce your own size. If it has "Auto" CSS, you **allow** the request and trigger a DOM relayout.

### Summary

With these changes, your API is completely agnostic to *how* the window is managed or where the pixels come from. The "Tiling vs Floating" behavior is elegantly offloaded to the existing CSS engine:

*   **Floating Window:** `Dom::wayland_surface(id).with_id("my-floaty-window")` + CSS `#my-floaty-window { position: absolute; left: 50px; top: 50px; /* width/height auto implied */ }`
*   **Tiling Window:** `Dom::div().with_class("row").with_child(Dom::wayland_surface(id))` + CSS `.row { flex-direction: row; }` (The flex layout naturally constrains the width).
