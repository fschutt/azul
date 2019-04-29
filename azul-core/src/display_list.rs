use azul_css::{
    StyleBorder, StyleBoxShadow, StyleBorderRadius,
    StyleBackground, ColorU, BoxShadowClipMode,
};
use app_resources::{ImageKey, FontInstanceKey};
use window::{LogicalPosition, LogicalSize};
use callbacks::PipelineId;

/// A tag that can be used to identify items during hit testing. If the tag
/// is missing then the item doesn't take part in hit testing at all. This
/// is composed of two numbers. In Servo, the first is an identifier while the
/// second is used to select the cursor that should be used during mouse
/// movement. In Gecko, the first is a scrollframe identifier, while the second
/// is used to store various flags that APZ needs to properly process input
/// events.
pub type ItemTag = (u64, u16);

pub type GlyphIndex = u32;

// Common flags
pub const FONT_INSTANCE_FLAG_SYNTHETIC_BOLD: u32    = 1 << 1;
pub const FONT_INSTANCE_FLAG_EMBEDDED_BITMAPS: u32  = 1 << 2;
pub const FONT_INSTANCE_FLAG_SUBPIXEL_BGR: u32      = 1 << 3;
pub const FONT_INSTANCE_FLAG_TRANSPOSE: u32         = 1 << 4;
pub const FONT_INSTANCE_FLAG_FLIP_X: u32            = 1 << 5;
pub const FONT_INSTANCE_FLAG_FLIP_Y: u32            = 1 << 6;
pub const FONT_INSTANCE_FLAG_SUBPIXEL_POSITION: u32 = 1 << 7;

// Windows flags
pub const FONT_INSTANCE_FLAG_FORCE_GDI: u32         = 1 << 16;

// Mac flags
pub const FONT_INSTANCE_FLAG_FONT_SMOOTHING: u32    = 1 << 16;

// FreeType flags
pub const FONT_INSTANCE_FLAG_FORCE_AUTOHINT: u32    = 1 << 16;
pub const FONT_INSTANCE_FLAG_NO_AUTOHINT: u32       = 1 << 17;
pub const FONT_INSTANCE_FLAG_VERTICAL_LAYOUT: u32   = 1 << 18;
pub const FONT_INSTANCE_FLAG_LCD_VERTICAL: u32      = 1 << 19;

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
pub struct GlyphOptions {
    pub render_mode: FontRenderMode,
    pub flags: u32,
}

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
pub enum FontRenderMode {
    Mono,
    Alpha,
    Subpixel,
}

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
pub struct GlyphInstance {
    pub index: GlyphIndex,
    pub point: LogicalPosition,
}

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
pub struct DisplayListRect {
    pub origin: LogicalPosition,
    pub size: LogicalSize,
}

impl DisplayListRect {
    pub const fn new(origin: LogicalPosition, size: LogicalSize) -> Self { Self { origin, size } }
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub struct CachedDisplayList {
    pub root: DisplayListMsg,
    pub pipeline_id: PipelineId,
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub enum DisplayListMsg {
    Frame(DisplayListFrame),
    ScrollFrame(DisplayListScrollFrame),
}

impl DisplayListMsg {
    pub fn append_child(&mut self, child: Self) {
        use self::DisplayListMsg::*;
        match self {
            Frame(f) => { f.children.push(child); },
            ScrollFrame(sf) => { sf.children.push(child); },
        }
    }

    pub fn append_children(&mut self, mut children: Vec<Self>) {
        use self::DisplayListMsg::*;
        match self {
            Frame(f) => { f.children.append(&mut children); },
            ScrollFrame(sf) => { sf.children.append(&mut children); },
        }
    }

    pub fn get_size(&self) -> LogicalSize {
        use self::DisplayListMsg::*;
        match self {
            Frame(f) => f.rect.size,
            ScrollFrame(sf) => sf.rect.size,
        }
    }
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub struct DisplayListScrollFrame {
    pub rect: DisplayListRect,
    pub scroll_position: LogicalPosition,
    pub content_size: LogicalSize,
    pub overlay_scrollbars: bool,
    pub tag: Option<ItemTag>,
    pub content: Vec<DisplayListRectContent>,
    pub children: Vec<DisplayListMsg>,
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub struct DisplayListFrame {
    pub rect: DisplayListRect,
    pub clip_rect: Option<DisplayListRect>,
    pub tag: Option<ItemTag>,
    pub content: Vec<DisplayListRectContent>,
    pub children: Vec<DisplayListMsg>,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ImageRendering {
    Auto,
    CrispEdges,
    Pixelated,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AlphaType {
    Alpha,
    PremultipliedAlpha,
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub enum DisplayListRectContent {
    Text {
        glyphs: Vec<GlyphInstance>,
        font_instance_key: FontInstanceKey,
        color: ColorU,
        options: Option<GlyphOptions>,
        clip: Option<DisplayListRect>,
    },
    Background {
        background_type: StyleBackground
    },
    Image {
        size: LogicalSize,
        offset: LogicalPosition,
        image_rendering: ImageRendering,
        alpha_type: AlphaType,
        image_key: ImageKey,
        background: ColorU,
    },
    Border {
        border: StyleBorder,
        radius: StyleBorderRadius,
    },
    BoxShadow {
        pre_shadow: StyleBoxShadow,
        border_radius: StyleBorderRadius,
        bounds: DisplayListRect,
        clip_rect: DisplayListRect,
        shadow_type: BoxShadowClipMode,
    },
}