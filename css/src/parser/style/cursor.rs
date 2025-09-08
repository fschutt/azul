use crate::{css_properties::*, parser::*};

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum StyleCursor {
    /// `alias`
    Alias,
    /// `all-scroll`
    AllScroll,
    /// `cell`
    Cell,
    /// `col-resize`
    ColResize,
    /// `context-menu`
    ContextMenu,
    /// `copy`
    Copy,
    /// `crosshair`
    Crosshair,
    /// `default` - note: called "arrow" in winit
    Default,
    /// `e-resize`
    EResize,
    /// `ew-resize`
    EwResize,
    /// `grab`
    Grab,
    /// `grabbing`
    Grabbing,
    /// `help`
    Help,
    /// `move`
    Move,
    /// `n-resize`
    NResize,
    /// `ns-resize`
    NsResize,
    /// `nesw-resize`
    NeswResize,
    /// `nwse-resize`
    NwseResize,
    /// `pointer` - note: called "hand" in winit
    Pointer,
    /// `progress`
    Progress,
    /// `row-resize`
    RowResize,
    /// `s-resize`
    SResize,
    /// `se-resize`
    SeResize,
    /// `text`
    Text,
    /// `unset`
    Unset,
    /// `vertical-text`
    VerticalText,
    /// `w-resize`
    WResize,
    /// `wait`
    Wait,
    /// `zoom-in`
    ZoomIn,
    /// `zoom-out`
    ZoomOut,
}

impl Default for StyleCursor {
    fn default() -> StyleCursor {
        StyleCursor::Default
    }
}

multi_type_parser!(
    parse_style_cursor,
    StyleCursor,
    ["alias", Alias],
    ["all-scroll", AllScroll],
    ["cell", Cell],
    ["col-resize", ColResize],
    ["context-menu", ContextMenu],
    ["copy", Copy],
    ["crosshair", Crosshair],
    ["default", Default],
    ["e-resize", EResize],
    ["ew-resize", EwResize],
    ["grab", Grab],
    ["grabbing", Grabbing],
    ["help", Help],
    ["move", Move],
    ["n-resize", NResize],
    ["ns-resize", NsResize],
    ["nesw-resize", NeswResize],
    ["nwse-resize", NwseResize],
    ["pointer", Pointer],
    ["progress", Progress],
    ["row-resize", RowResize],
    ["s-resize", SResize],
    ["se-resize", SeResize],
    ["text", Text],
    ["unset", Unset],
    ["vertical-text", VerticalText],
    ["w-resize", WResize],
    ["wait", Wait],
    ["zoom-in", ZoomIn],
    ["zoom-out", ZoomOut]
);
