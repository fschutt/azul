use std::fmt;
use azul_css::{
    LayoutPoint, LayoutSize,
    StyleBackgroundRepeat, StyleBackgroundPosition, ColorU, BoxShadowClipMode,
    LinearGradient, RadialGradient, BoxShadowPreDisplayItem, StyleBackgroundSize,
    CssPropertyValue,

    StyleBorderTopWidth, StyleBorderRightWidth, StyleBorderBottomWidth, StyleBorderLeftWidth,
    StyleBorderTopColor, StyleBorderRightColor, StyleBorderBottomColor, StyleBorderLeftColor,
    StyleBorderTopStyle, StyleBorderRightStyle, StyleBorderBottomStyle, StyleBorderLeftStyle,
    StyleBorderTopLeftRadius, StyleBorderTopRightRadius, StyleBorderBottomLeftRadius, StyleBorderBottomRightRadius,
};
use {
    app_resources::{ImageKey, FontInstanceKey, ImageInfo},
    ui_solver::ExternalScrollId,
    dom::ScrollTagId,
};

/// A tag that can be used to identify items during hit testing. If the tag
/// is missing then the item doesn't take part in hit testing at all. This
/// is composed of two numbers. In Servo, the first is an identifier while the
/// second is used to select the cursor that should be used during mouse
/// movement. In Gecko, the first is a scrollframe identifier, while the second
/// is used to store various flags that APZ needs to properly process input
/// events.
pub type ItemTag = (u64, u16);
pub type GlyphIndex = u32;

pub type FontInstanceFlags = u32;

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
    pub flags: FontInstanceFlags,
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
    pub point: LayoutPoint,
}

#[derive(Copy, Clone, PartialEq, PartialOrd)]
pub struct DisplayListRect {
    pub origin: LayoutPoint,
    pub size: LayoutSize,
}

impl fmt::Debug for DisplayListRect {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f,
            "{{ origin: {{ x: {}, y: {} }}, size: {{ width: {}, height: {} }} }}",
            self.origin.x, self.origin.y, self.size.width, self.size.height,
        )
    }
}

impl DisplayListRect {
    pub const fn new(origin: LayoutPoint, size: LayoutSize) -> Self { Self { origin, size } }
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub struct CachedDisplayList {
    pub root: DisplayListMsg,
}

impl CachedDisplayList {
    pub fn empty(size: LayoutSize) -> Self {
        Self { root: DisplayListMsg::Frame(DisplayListFrame::root(size)) }
    }
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
            ScrollFrame(sf) => { sf.frame.children.push(child); },
        }
    }

    pub fn append_children(&mut self, mut children: Vec<Self>) {
        use self::DisplayListMsg::*;
        match self {
            Frame(f) => { f.children.append(&mut children); },
            ScrollFrame(sf) => { sf.frame.children.append(&mut children); },
        }
    }

    pub fn get_size(&self) -> LayoutSize {
        use self::DisplayListMsg::*;
        match self {
            Frame(f) => f.rect.size,
            ScrollFrame(sf) => sf.frame.rect.size,
        }
    }
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub struct DisplayListScrollFrame {
    /// Size of the (overflowing) content of the scroll frame
    pub content_size: LayoutSize,
    /// The scroll ID is the hash of the DOM node, so that scrolling
    /// positions can be tracked across multiple frames
    pub scroll_id: ExternalScrollId,
    /// The scroll tag is used for hit-testing
    pub scroll_tag: ScrollTagId,
    /// Content + children of the scroll clip
    pub frame: DisplayListFrame,
}

#[derive(Clone, PartialEq, PartialOrd)]
pub struct DisplayListFrame {
    pub rect: DisplayListRect,
    /// Border radius, set to none only if overflow: visible is set!
    pub border_radius: StyleBorderRadius,
    pub clip_rect: Option<DisplayListRect>,
    pub tag: Option<ItemTag>,
    pub content: Vec<DisplayListRectContent>,
    pub children: Vec<DisplayListMsg>,
}

impl fmt::Debug for DisplayListFrame {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "rect: {:#?},", self.rect)?;
        write!(f, "\r\nborder_radius: {:#?},", self.border_radius)?;
        if let Some(clip_rect) = &self.clip_rect {
            write!(f, "\r\nclip_rect: {:#?},", clip_rect)?;
        }
        if let Some(tag) = &self.tag {
            write!(f, "\r\ntag: ({}, {}),", tag.0, tag.1)?;
        }
        if !self.content.is_empty() {
            write!(f, "\r\ncontent: {:#?}", self.content)?;
        }
        if !self.children.is_empty() {
            write!(f, "\r\nchildren: {:#?}", self.children)?;
        }
        Ok(())
    }
}

impl DisplayListFrame {
    pub fn root(dimensions: LayoutSize) -> Self {
        DisplayListFrame {
            tag: None,
            clip_rect: None,
            rect: DisplayListRect {
                origin: LayoutPoint { x: 0.0, y: 0.0 },
                size: dimensions,
            },
            border_radius: StyleBorderRadius::default(),
            content: vec![],
            children: vec![],
        }
    }
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

#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StyleBorderRadius {
    pub top_left: Option<CssPropertyValue<StyleBorderTopLeftRadius>>,
    pub top_right: Option<CssPropertyValue<StyleBorderTopRightRadius>>,
    pub bottom_left: Option<CssPropertyValue<StyleBorderBottomLeftRadius>>,
    pub bottom_right: Option<CssPropertyValue<StyleBorderBottomRightRadius>>,
}

impl fmt::Debug for StyleBorderRadius {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "StyleBorderRadius {{")?;
        if let Some(tl) = &self.top_left {
            write!(f, "\r\n\ttop-left: {},", tl)?;
        }
        if let Some(tr) = &self.top_right {
            write!(f, "\r\n\ttop-right: {},", tr)?;
        }
        if let Some(bl) = &self.bottom_left {
            write!(f, "\r\n\tbottom-left: {},", bl)?;
        }
        if let Some(br) = &self.bottom_right {
            write!(f, "\r\n\tbottom-right: {},", br)?;
        }
        write!(f, "\r\n}}")
    }
}

macro_rules! tlbr_debug {($struct_name:ident) => (
    impl fmt::Debug for $struct_name {
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
            write!(f, "{} {{", stringify!($struct_name))?;
            if let Some(t) = &self.top {
                write!(f, "\r\n\ttop: {},", t)?;
            }
            if let Some(r) = &self.right {
                write!(f, "\r\n\tright: {},", r)?;
            }
            if let Some(b) = &self.bottom {
                write!(f, "\r\n\tbottom: {},", b)?;
            }
            if let Some(l) = &self.left {
                write!(f, "\r\n\tleft: {},", l)?;
            }
            write!(f, "\r\n}}")
        }
    }
)}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StyleBorderWidths {
    pub top: Option<CssPropertyValue<StyleBorderTopWidth>>,
    pub right: Option<CssPropertyValue<StyleBorderRightWidth>>,
    pub bottom: Option<CssPropertyValue<StyleBorderBottomWidth>>,
    pub left: Option<CssPropertyValue<StyleBorderLeftWidth>>,
}

tlbr_debug!(StyleBorderWidths);

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StyleBorderColors {
    pub top: Option<CssPropertyValue<StyleBorderTopColor>>,
    pub right: Option<CssPropertyValue<StyleBorderRightColor>>,
    pub bottom: Option<CssPropertyValue<StyleBorderBottomColor>>,
    pub left: Option<CssPropertyValue<StyleBorderLeftColor>>,
}

tlbr_debug!(StyleBorderColors);

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StyleBorderStyles {
    pub top: Option<CssPropertyValue<StyleBorderTopStyle>>,
    pub right: Option<CssPropertyValue<StyleBorderRightStyle>>,
    pub bottom: Option<CssPropertyValue<StyleBorderBottomStyle>>,
    pub left: Option<CssPropertyValue<StyleBorderLeftStyle>>,
}

tlbr_debug!(StyleBorderStyles);

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StyleBoxShadow {
    pub top: Option<CssPropertyValue<BoxShadowPreDisplayItem>>,
    pub right: Option<CssPropertyValue<BoxShadowPreDisplayItem>>,
    pub bottom: Option<CssPropertyValue<BoxShadowPreDisplayItem>>,
    pub left: Option<CssPropertyValue<BoxShadowPreDisplayItem>>,
}

tlbr_debug!(StyleBoxShadow);

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub enum DisplayListRectContent {
    Text {
        glyphs: Vec<GlyphInstance>,
        font_instance_key: FontInstanceKey,
        color: ColorU,
        glyph_options: Option<GlyphOptions>,
        clip: Option<DisplayListRect>,
    },
    Background {
        content: RectBackground,
        size: Option<StyleBackgroundSize>,
        offset: Option<StyleBackgroundPosition>,
        repeat: Option<StyleBackgroundRepeat>,
    },
    Image {
        size: LayoutSize,
        offset: LayoutPoint,
        image_rendering: ImageRendering,
        alpha_type: AlphaType,
        image_key: ImageKey,
        background_color: ColorU,
    },
    Border {
        widths: StyleBorderWidths,
        colors: StyleBorderColors,
        styles: StyleBorderStyles,
    },
    BoxShadow {
        shadow: StyleBoxShadow,
        clip_mode: BoxShadowClipMode,
    },
}

#[derive(Clone, PartialEq, PartialOrd)]
pub enum RectBackground {
    LinearGradient(LinearGradient),
    RadialGradient(RadialGradient),
    Image(ImageInfo),
    Color(ColorU),
}

impl fmt::Debug for RectBackground {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::RectBackground::*;
        match self {
            LinearGradient(l) => write!(f, "{}", l),
            RadialGradient(r) => write!(f, "{}", r),
            Image(id) => write!(f, "image({:#?})", id),
            Color(c) => write!(f, "{}", c),
        }
    }
}

impl RectBackground {
    pub fn get_content_size(&self) -> Option<(f32, f32)> {
        match self {
            RectBackground::Image(info) => { let dim = info.get_dimensions(); Some((dim.0 as f32, dim.1 as f32)) }
            _ => None,
        }
    }
}