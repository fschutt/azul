use std::{
    fmt,
    collections::BTreeMap,
};
use azul_css::{
    LayoutPoint, LayoutSize, LayoutRect,
    StyleBackgroundRepeat, StyleBackgroundPosition, ColorU, BoxShadowClipMode,
    LinearGradient, RadialGradient, BoxShadowPreDisplayItem, StyleBackgroundSize,
    CssPropertyValue, CssProperty, RectStyle, RectLayout,

    StyleBorderTopWidth, StyleBorderRightWidth, StyleBorderBottomWidth, StyleBorderLeftWidth,
    StyleBorderTopColor, StyleBorderRightColor, StyleBorderBottomColor, StyleBorderLeftColor,
    StyleBorderTopStyle, StyleBorderRightStyle, StyleBorderBottomStyle, StyleBorderLeftStyle,
    StyleBorderTopLeftRadius, StyleBorderTopRightRadius, StyleBorderBottomLeftRadius, StyleBorderBottomRightRadius,
};
use crate::{
    callbacks::PipelineId,
    ui_solver::{
        PositionedRectangle, ResolvedOffsets, ExternalScrollId,
        LayoutResult, ScrolledNodes, OverflowingScrollNode,
        PositionInfo,
    },
    window::{FullWindowState, LogicalRect, LogicalPosition, LogicalSize},
    app_resources::{
        AppResources, AddImageMsg, FontImageApi, ImageDescriptor, ImageDescriptorFlags,
        ImageKey, FontInstanceKey, ImageInfo, ImageId, LayoutedGlyphs, PrimitiveFlags,
        Epoch, ExternalImageId, GlyphOptions, LoadFontFn, LoadImageFn, ParseFontFn,
    },
    styled_dom::{DomId, StyledDom, ContentGroup},
    id_tree::{NodeDataContainer, NodeId},
    dom::{NodeData, TagId, ScrollTagId},
};
#[cfg(feature = "opengl")]
use crate::gl::{Texture, GlContextPtr};

pub type GlyphIndex = u32;

/// Parse a string in the format of "600x100" -> (600, 100)
pub fn parse_display_list_size(output_size: &str) -> Option<(f32, f32)> {
    let output_size = output_size.trim();
    let mut iter = output_size.split("x");
    let w = iter.next()?;
    let h = iter.next()?;
    let w = w.trim();
    let h = h.trim();
    let w = w.parse::<f32>().ok()?;
    let h = h.parse::<f32>().ok()?;
    Some((w, h))
}

#[derive(Debug, Default, Copy, Clone, PartialEq, PartialOrd)]
pub struct GlyphInstance {
    pub index: GlyphIndex,
    pub point: LogicalPosition,
    pub size: LogicalSize,
}

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
pub struct DisplayListImageMask {
    pub image: ImageKey,
    pub rect: LogicalRect,
    pub repeat: bool,
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub struct CachedDisplayList {
    pub root: DisplayListMsg,
}

impl CachedDisplayList {
    pub fn empty(size: LayoutSize, origin: LayoutPoint) -> Self {
        Self { root: DisplayListMsg::Frame(DisplayListFrame::root(size, origin)) }
    }

    pub fn new(
        epoch: Epoch,
        pipeline_id: PipelineId,
        full_window_state: &FullWindowState,
        solved_layout: &SolvedLayout,
        app_resources: &AppResources,
    ) -> Self {
        const DOM_ID: DomId = DomId::ROOT_ID;
        CachedDisplayList {
            root: push_rectangles_into_displaylist(
                &solved_layout.solved_layout_cache[DOM_ID.inner].styled_dom.rects_in_rendering_order,
                &DisplayListParametersRef {
                    dom_id: DOM_ID,
                    epoch,
                    pipeline_id,
                    full_window_state,
                    layout_results: &solved_layout.solved_layout_cache[..],
                    gl_texture_cache: &solved_layout.gl_texture_cache,
                    app_resources,
                },
            )
        }
    }
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub enum DisplayListMsg {
    Frame(DisplayListFrame),
    ScrollFrame(DisplayListScrollFrame),
}

impl DisplayListMsg {

    pub fn get_position(&self) -> PositionInfo {
        use self::DisplayListMsg::*;
        match self {
            Frame(f) => f.position,
            ScrollFrame(sf) => sf.frame.position,
        }
    }

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
            Frame(f) => f.size,
            ScrollFrame(sf) => sf.frame.size,
        }
    }
}

#[derive(Clone, PartialEq, PartialOrd)]
pub struct DisplayListScrollFrame {
    /// Bounding rect of the (overflowing) content of the scroll frame
    pub content_rect: LayoutRect,
    /// The scroll ID is the hash of the DOM node, so that scrolling
    /// positions can be tracked across multiple frames
    pub scroll_id: ExternalScrollId,
    /// The scroll tag is used for hit-testing
    pub scroll_tag: ScrollTagId,
    /// Content + children of the scroll clip
    pub frame: DisplayListFrame,
}

impl fmt::Debug for DisplayListScrollFrame {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "DisplayListScrollFrame {{\r\n")?;
        write!(f, "    content_rect: {}\r\n", self.content_rect)?;
        write!(f, "    scroll_tag: {}\r\n", self.scroll_tag)?;
        write!(f, "    frame: DisplayListFrame {{\r\n")?;
        let frame = format!("{:#?}", self.frame);
        let frame = frame.lines().map(|l| format!("        {}", l)).collect::<Vec<_>>().join("\r\n");
        write!(f, "{}\r\n", frame)?;
        write!(f, "    }}\r\n")?;
        write!(f, "}}")?;
        Ok(())
    }
}

#[derive(Clone, PartialEq, PartialOrd)]
pub struct DisplayListFrame {
    pub size: LayoutSize,
    pub position: PositionInfo,
    pub flags: PrimitiveFlags,
    pub clip_mask: Option<DisplayListImageMask>,
    /// Border radius, set to none only if overflow: visible is set!
    pub border_radius: StyleBorderRadius,
    pub tag: Option<TagId>,
    pub content: Vec<LayoutRectContent>,
    pub children: Vec<DisplayListMsg>,
}

impl fmt::Debug for DisplayListFrame {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let print_no_comma_rect =
            !self.border_radius.is_none() ||
            self.tag.is_some() ||
            !self.content.is_empty() ||
            !self.children.is_empty();

        write!(f, "rect: {:#?} @ {:?}{}", self.size, self.position, if !print_no_comma_rect { "" } else { "," })?;

        if !self.border_radius.is_none() {
            write!(f, "\r\nborder_radius: {:#?}", self.border_radius)?;
        }
        if let Some(tag) = &self.tag {
            write!(f, "\r\ntag: {}", tag.0)?;
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
    pub fn root(dimensions: LayoutSize, root_origin: LayoutPoint) -> Self {
        DisplayListFrame {
            tag: None,
            size: dimensions,
            position: PositionInfo::Static { x_offset: root_origin.x, y_offset: root_origin.y, static_x_offset: root_origin.x, static_y_offset: root_origin.y },
            flags: PrimitiveFlags {
                is_backface_visible: true,
                is_scrollbar_container: false,
                is_scrollbar_thumb: false,
            },
            border_radius: StyleBorderRadius::default(),
            content: vec![],
            children: vec![],
            clip_mask: None,
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

impl StyleBorderRadius {
    pub fn is_none(&self) -> bool {
        self.top_left.is_none() &&
        self.top_right.is_none() &&
        self.bottom_left.is_none() &&
        self.bottom_right.is_none()
    }
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

impl StyleBorderWidths {

    #[inline]
    pub fn left_width(&self) -> f32 {
        self.left.unwrap_or_default().get_property_owned().unwrap_or_default().inner.to_pixels(0.0)
    }

    #[inline]
    pub fn right_width(&self) -> f32 {
        self.right.unwrap_or_default().get_property_owned().unwrap_or_default().inner.to_pixels(0.0)
    }

    #[inline]
    pub fn top_width(&self) -> f32 {
        self.top.unwrap_or_default().get_property_owned().unwrap_or_default().inner.to_pixels(0.0)
    }

    #[inline]
    pub fn bottom_width(&self) -> f32 {
        self.bottom.unwrap_or_default().get_property_owned().unwrap_or_default().inner.to_pixels(0.0)
    }

    #[inline]
    pub fn total_horizontal(&self) -> f32 {
        self.left_width() + self.right_width()
    }

    #[inline]
    pub fn total_vertical(&self) -> f32 {
        self.top_width() + self.bottom_width()
    }
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

#[derive(Clone, PartialEq, PartialOrd)]
pub enum LayoutRectContent {
    Text {
        glyphs: Vec<GlyphInstance>,
        font_instance_key: FontInstanceKey,
        color: ColorU,
        glyph_options: Option<GlyphOptions>,
        overflow: (bool, bool),
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

impl fmt::Debug for LayoutRectContent {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::LayoutRectContent::*;
        match self {
            Text { glyphs, font_instance_key, color, glyph_options, overflow } => {
                write!(f,
                    "Text {{\r\n\
                        glyphs: {:?},\r\n\
                        font_instance_key: {:?},\r\n\
                        color: {:?},\r\n\
                        glyph_options: {:?},\r\n\
                        overflow: {:?},\r\n\
                    }}",
                    glyphs, font_instance_key, color, glyph_options, overflow
                )
            },
            Background { content, size, offset, repeat } => {
                write!(f, "Background {{\r\n")?;
                write!(f, "    content: {:?},\r\n", content)?;
                if let Some(size) = size {
                    write!(f, "    size: {:?},\r\n", size)?;
                }
                if let Some(offset) = offset {
                    write!(f, "    offset: {:?},\r\n", offset)?;
                }
                if let Some(repeat) = repeat {
                    write!(f, "    repeat: {:?},\r\n", repeat)?;
                }
                write!(f, "}}")
            },
            Image { size, offset, image_rendering, alpha_type, image_key, background_color } => {
                write!(f,
                    "Image {{\r\n\
                        size: {:?},\r\n\
                        offset: {:?},\r\n\
                        image_rendering: {:?},\r\n\
                        alpha_type: {:?},\r\n\
                        image_key: {:?},\r\n\
                        background_color: {:?}\r\n\
                    }}",
                    size, offset, image_rendering, alpha_type, image_key, background_color
                )
            },
            Border { widths, colors, styles, } => {
                write!(f,
                    "Border {{\r\n\
                        widths: {:?},\r\n\
                        colors: {:?},\r\n\
                        styles: {:?}\r\n\
                    }}",
                    widths, colors, styles,
                )
            },
            BoxShadow { shadow, clip_mode } => {
                write!(f,
                    "BoxShadow {{\r\n\
                        shadow: {:?},\r\n\
                        clip_mode: {:?}\r\n\
                    }}",
                    shadow, clip_mode,
                )
            },
        }
    }
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

// ------------------- NEW DISPLAY LIST CODE

/// Since the display list can take a lot of parameters, we don't want to
/// continually pass them as parameters of the function and rather use a
/// struct to pass them around. This is purely for ergonomic reasons.
///
/// `DisplayListParametersRef` has only members that are
///  **immutable references** to other things that need to be passed down the display list
#[derive(Clone)]
pub struct DisplayListParametersRef<'a> {
    /// ID of this Dom
    pub dom_id: DomId,
    /// Epoch of all the OpenGL textures
    pub epoch: Epoch,
    /// The CSS that should be applied to the DOM
    pub full_window_state: &'a FullWindowState,
    /// The current pipeline of the display list
    pub pipeline_id: PipelineId,
    /// Cached layouts (+ solved layouts for iframes)
    pub layout_results: &'a [LayoutResult],
    /// Cached rendered OpenGL textures
    pub gl_texture_cache: &'a GlTextureCache,
    /// Reference to the AppResources, necessary to query info about image and font keys
    pub app_resources: &'a AppResources,
}

#[derive(Default)]
pub struct GlTextureCache {
    pub solved_textures: BTreeMap<DomId, BTreeMap<NodeId, (ImageKey, ImageDescriptor)>>,
}

// todo: very unclean
pub type LayoutFn<U: FontImageApi> = fn(StyledDom, &mut AppResources, &mut U, PipelineId, RenderCallbacks<U>, &FullWindowState) -> Vec<LayoutResult>;
#[cfg(feature = "opengl")]
pub type GlStoreImageFn = fn(PipelineId, Epoch, Texture) -> ExternalImageId;

#[derive(Default)]
pub struct SolvedLayout {
    pub solved_layout_cache: Vec<LayoutResult>,
    pub gl_texture_cache: GlTextureCache,
}

pub struct RenderCallbacks<U: FontImageApi> {
    pub insert_into_active_gl_textures: GlStoreImageFn,
    pub layout_fn: LayoutFn<U>,
    pub load_font_fn: LoadFontFn,
    pub load_image_fn: LoadImageFn,
    pub parse_font_fn: ParseFontFn,
}

impl SolvedLayout {

    /// Does the layout, updates the image + font resources for the RenderAPI
    #[cfg(feature = "opengl")]
    pub fn new<U: FontImageApi>(
        styled_dom: StyledDom,
        epoch: Epoch,
        pipeline_id: PipelineId,
        full_window_state: &FullWindowState,
        gl_context: &GlContextPtr,
        render_api: &mut U,
        app_resources: &mut AppResources,
        callbacks: RenderCallbacks<U>,
    ) -> Self {

        use crate::{
            app_resources::{
                RawImageFormat, AddImage, ExternalImageData, TextureTarget, ExternalImageType,
                ImageData, add_resources, garbage_collect_fonts_and_images,
            },
            callbacks::{GlCallbackInfo, HidpiAdjustedBounds, GlCallbackInfoPtr},
            gl::insert_into_active_gl_textures,
        };
        use gleam::gl;
        use std::ffi::c_void;

        let layout_results = (callbacks.layout_fn)(
            styled_dom,
            app_resources,
            render_api,
            pipeline_id,
            callbacks,
            &full_window_state,
        );

        let mut solved_textures = BTreeMap::new();

        // Now that the layout is done, render the OpenGL textures and add them to the RenderAPI
        for layout_result in layout_results.iter() {
            for (node_id, gl_texture_node) in layout_result.styled_dom.scan_for_gltexture_callbacks() {

                let cb = gl_texture_node.callback;
                let ptr = &gl_texture_node.data;

                // Invoke OpenGL callback, render texture
                let rect_size = layout_result.rects.as_ref()[node_id].size;

                let texture = {

                    let callback_info = GlCallbackInfo {
                        state: ptr,
                        gl_context: &gl_context,
                        resources: app_resources,
                        bounds: HidpiAdjustedBounds::from_bounds(
                            rect_size,
                            full_window_state.size.hidpi_factor,
                        ),
                    };

                    let ptr = GlCallbackInfoPtr { ptr: Box::into_raw(Box::new(callback_info)) as *mut c_void };
                    let gl_callback_return = (cb.cb)(ptr);
                    let tex: Option<Texture> = gl_callback_return.texture.into();

                    // Reset the framebuffer and SRGB color target to 0
                    gl_context.bind_framebuffer(gl::FRAMEBUFFER, 0);
                    gl_context.disable(gl::FRAMEBUFFER_SRGB);
                    gl_context.disable(gl::MULTISAMPLE);

                    tex
                };

                if let Some(t) = texture {
                    solved_textures
                        .entry(layout_result.dom_id.clone())
                        .or_insert_with(|| BTreeMap::default())
                        .insert(node_id, t);
                }
            }
        }

        let mut image_resource_updates = Vec::new();
        let mut gl_texture_cache = GlTextureCache {
            solved_textures: BTreeMap::new(),
        };

        for (dom_id, textures) in solved_textures {
            for (node_id, texture) in textures {

                // Note: The ImageDescriptor has no effect on how large the image appears on-screen
                let descriptor = ImageDescriptor {
                    format: RawImageFormat::RGBA8,
                    dimensions: (texture.size.width as usize, texture.size.height as usize),
                    stride: None,
                    offset: 0,
                    flags: ImageDescriptorFlags {
                        is_opaque: texture.flags.is_opaque,
                        // The texture gets mapped 1:1 onto the display, so there is no need for mipmaps
                        allow_mipmaps: false,
                    },
                };

                let key = render_api.new_image_key();
                let external_image_id = (insert_into_active_gl_textures)(pipeline_id, epoch, texture);

                let add_img_msg = AddImageMsg(
                    AddImage {
                        key,
                        descriptor,
                        data: ImageData::External(ExternalImageData {
                            id: external_image_id,
                            channel_index: 0,
                            image_type: ExternalImageType::TextureHandle(TextureTarget::Default),
                        }),
                        tiling: None,
                    },
                    ImageInfo { key, descriptor }
                );

                image_resource_updates.push((ImageId::new(), add_img_msg));
                gl_texture_cache.solved_textures
                    .entry(dom_id.clone())
                    .or_insert_with(|| BTreeMap::new())
                    .insert(node_id, (key, descriptor));
            }
        }

        // Delete unused font and image keys (that were not used in this display list)
        garbage_collect_fonts_and_images(app_resources, render_api, &pipeline_id);
        // Add the new GL textures to the RenderApi
        add_resources(app_resources, render_api, &pipeline_id, Vec::new(), image_resource_updates);

        SolvedLayout {
            solved_layout_cache: layout_results,
            gl_texture_cache,
        }
    }
}

pub fn push_rectangles_into_displaylist<'a>(
    root_content_group: &ContentGroup,
    referenced_content: &DisplayListParametersRef<'a>,
) -> DisplayListMsg {

    let mut content = displaylist_handle_rect(
        root_content_group.root.into_crate_internal().unwrap(),
        referenced_content,
    );

    let children = root_content_group.children.iter().map(|child_content_group| {
        push_rectangles_into_displaylist(
            child_content_group,
            referenced_content,
        )
    }).collect();

    content.append_children(children);

    content
}

/// Push a single rectangle into the display list builder
pub fn displaylist_handle_rect<'a>(
    rect_idx: NodeId,
    referenced_content: &DisplayListParametersRef<'a>,
) -> DisplayListMsg {

    use crate::dom::NodeType::*;
    use crate::styled_dom::{AzNodeId, AzTagId};

    let DisplayListParametersRef {
        dom_id,
        pipeline_id,
        layout_results,
        gl_texture_cache,
        app_resources,
        ..
    } = referenced_content;

    let layout_result = &layout_results[dom_id.inner];
    let styled_node = &layout_result.styled_dom.styled_nodes.as_container()[rect_idx];
    let bounds = &layout_result.rects.as_ref()[rect_idx];
    let html_node = &layout_result.styled_dom.node_data.as_container()[rect_idx];

    let tag_id = styled_node.tag_id.into_option().or({
        layout_result.scrollable_nodes.overflowing_nodes
        .get(&AzNodeId::from_crate_internal(Some(rect_idx)))
        .map(|scrolled| AzTagId::from_crate_internal(scrolled.scroll_tag_id.0))
    });

    let (size, position) = bounds.get_background_bounds();

    let clip_mask = html_node.get_clip_mask().as_option().and_then(|m| {
        let image_info = app_resources.currently_registered_images.get(pipeline_id)?.get(&m.image)?;
        Some(DisplayListImageMask {
            image: image_info.key,
            rect: m.rect,
            repeat: m.repeat,
        })
    });

    let mut frame = DisplayListFrame {
        tag: tag_id.map(|t| t.into_crate_internal()),
        size,
        position,
        border_radius: StyleBorderRadius {
            top_left: styled_node.style.border_top_left_radius,
            top_right: styled_node.style.border_top_right_radius,
            bottom_left: styled_node.style.border_bottom_left_radius,
            bottom_right: styled_node.style.border_bottom_right_radius,
        },
        flags: PrimitiveFlags {
            is_backface_visible: true,
            is_scrollbar_container: false,
            is_scrollbar_thumb: false,
        },
        content: Vec::new(),
        children: Vec::new(),
        clip_mask,
    };

    if styled_node.style.has_box_shadow() {
        frame.content.push(LayoutRectContent::BoxShadow {
            shadow: StyleBoxShadow {
                left: styled_node.style.box_shadow_left,
                right: styled_node.style.box_shadow_right,
                top: styled_node.style.box_shadow_top,
                bottom: styled_node.style.box_shadow_bottom,
            },
            clip_mode: BoxShadowClipMode::Outset,
        });
    }

    // If the rect is hit-testing relevant, we need to push a rect anyway.
    // Otherwise the hit-testing gets confused
    if let Some(bg) = styled_node.style.background.as_ref().and_then(|br| br.get_property()) {

        use azul_css::{CssImageId, StyleBackgroundContent::*};

        fn get_image_info(app_resources: &AppResources, pipeline_id: &PipelineId, style_image_id: &CssImageId) -> Option<RectBackground> {
            let image_id = app_resources.get_css_image_id(style_image_id.inner.as_str())?;
            let image_info = app_resources.get_image_info(pipeline_id, image_id)?;
            Some(RectBackground::Image(*image_info))
        }

        let background_content = match bg {
            LinearGradient(lg) => Some(RectBackground::LinearGradient(lg.clone())),
            RadialGradient(rg) => Some(RectBackground::RadialGradient(rg.clone())),
            Image(style_image_id) => get_image_info(&app_resources, &pipeline_id, &style_image_id),
            Color(c) => Some(RectBackground::Color(*c)),
        };

        if let Some(background_content) = background_content {
            frame.content.push(LayoutRectContent::Background {
                content: background_content,
                size: styled_node.style.background_size.and_then(|bs| bs.get_property().cloned()),
                offset: styled_node.style.background_position.and_then(|bs| bs.get_property().cloned()),
                repeat: styled_node.style.background_repeat.and_then(|bs| bs.get_property().cloned()),
            });
        }
    }

    match html_node.get_node_type() {
        Div | Body => { },
        Text(_) | Label(_) => {
            if let Some(layouted_glyphs) = layout_result.layouted_glyphs_cache.get(&rect_idx).cloned() {

                use crate::ui_solver::DEFAULT_FONT_COLOR;

                let text_color = styled_node.style.text_color.and_then(|tc| tc.get_property().cloned()).unwrap_or(DEFAULT_FONT_COLOR).inner;
                let positioned_words = &layout_result.positioned_words_cache[&rect_idx];
                let font_instance_key = positioned_words.1;

                frame.content.push(get_text(
                    layouted_glyphs,
                    font_instance_key,
                    text_color,
                    &styled_node.layout,
                ));
            }
        },
        Image(image_id) => {
            if let Some(image_info) = app_resources.get_image_info(pipeline_id, image_id) {
                frame.content.push(LayoutRectContent::Image {
                    size: LayoutSize::new(bounds.size.width, bounds.size.height),
                    offset: LayoutPoint::zero(),
                    image_rendering: ImageRendering::Auto,
                    alpha_type: AlphaType::PremultipliedAlpha,
                    image_key: image_info.key,
                    background_color: ColorU::WHITE,
                });
            }
        },
        #[cfg(feature = "opengl")]
        GlTexture(_) => {
            if let Some((key, descriptor)) = gl_texture_cache.solved_textures.get(&dom_id).and_then(|textures| textures.get(&rect_idx)) {
                frame.content.push(LayoutRectContent::Image {
                    size: LayoutSize::new(descriptor.dimensions.0 as isize, descriptor.dimensions.1 as isize),
                    offset: LayoutPoint::zero(),
                    image_rendering: ImageRendering::Auto,
                    alpha_type: AlphaType::Alpha,
                    image_key: *key,
                    background_color: ColorU::WHITE,
                })
            }
        },
        IFrame(_) => {
            if let Some(iframe_dom_id) = layout_result.iframe_mapping.iter()
            .find_map(|(node_id, dom_id)| if *node_id == rect_idx { Some(*dom_id) } else { None }) {
                frame.children.push(push_rectangles_into_displaylist(
                    &layout_results[iframe_dom_id.inner].styled_dom.rects_in_rendering_order,
                    // layout_result.rects_in_rendering_order.root,
                    &DisplayListParametersRef {
                        // Important: Need to update the DOM ID,
                        // otherwise this function would be endlessly recurse
                        dom_id: iframe_dom_id.clone(),
                        .. *referenced_content
                    }
                ));
            }
        },
    };

    if styled_node.style.has_border() {
        frame.content.push(LayoutRectContent::Border {
            widths: StyleBorderWidths {
                top: styled_node.layout.border_top_width,
                left: styled_node.layout.border_left_width,
                bottom: styled_node.layout.border_bottom_width,
                right: styled_node.layout.border_right_width,
            },
            colors: StyleBorderColors {
                top: styled_node.style.border_top_color,
                left: styled_node.style.border_left_color,
                bottom: styled_node.style.border_bottom_color,
                right: styled_node.style.border_right_color,
            },
            styles: StyleBorderStyles {
                top: styled_node.style.border_top_style,
                left: styled_node.style.border_left_style,
                bottom: styled_node.style.border_bottom_style,
                right: styled_node.style.border_right_style,
            },
        });
    }

    if styled_node.style.has_box_shadow() {
        frame.content.push(LayoutRectContent::BoxShadow {
            shadow: StyleBoxShadow {
                left: styled_node.style.box_shadow_left,
                right: styled_node.style.box_shadow_right,
                top: styled_node.style.box_shadow_top,
                bottom: styled_node.style.box_shadow_bottom,
            },
            clip_mode: BoxShadowClipMode::Inset,
        });
    }

    match layout_result.scrollable_nodes.overflowing_nodes.get(&AzNodeId::from_crate_internal(Some(rect_idx))) {
        Some(scroll_node) => DisplayListMsg::ScrollFrame(DisplayListScrollFrame {
            content_rect: scroll_node.child_rect,
            scroll_id: scroll_node.parent_external_scroll_id,
            scroll_tag: scroll_node.scroll_tag_id,
            frame,
        }),
        None => DisplayListMsg::Frame(frame),
    }
}

pub fn get_text(
    layouted_glyphs: LayoutedGlyphs,
    font_instance_key: FontInstanceKey,
    font_color: ColorU,
    rect_layout: &RectLayout,
) -> LayoutRectContent {

    let overflow_horizontal_visible = rect_layout.is_horizontal_overflow_visible();
    let overflow_vertical_visible = rect_layout.is_horizontal_overflow_visible();

    LayoutRectContent::Text {
        glyphs: layouted_glyphs.glyphs,
        font_instance_key,
        color: font_color,
        glyph_options: None,
        overflow: (overflow_horizontal_visible, overflow_vertical_visible),
    }
}

/// Subtracts the padding from the size, returning the new size
///
/// Warning: The resulting rectangle may have negative width or height
#[inline]
pub fn subtract_padding(size: &LayoutSize, padding: &ResolvedOffsets) -> LayoutSize {
    LayoutSize {
        width: size.width - padding.right + padding.left,
        height: size.height - padding.top + padding.bottom,
    }
}

#[test]
fn test_overflow_parsing() {
    use azul_css::Overflow;

    let layout1 = RectLayout::default();

    // The default for overflowing is overflow: auto, which clips
    // children, so this should evaluate to true by default
    assert_eq!(node_needs_to_clip_children(&layout1), true);

    let layout2 = RectLayout {
        overflow_x: Some(CssPropertyValue::Exact(Overflow::Visible)),
        overflow_y: Some(CssPropertyValue::Exact(Overflow::Visible)),
        .. Default::default()
    };
    assert_eq!(node_needs_to_clip_children(&layout2), false);

    let layout3 = RectLayout {
        overflow_x: Some(CssPropertyValue::Exact(Overflow::Hidden)),
        overflow_y: Some(CssPropertyValue::Exact(Overflow::Hidden)),
        .. Default::default()
    };
    assert_eq!(node_needs_to_clip_children(&layout3), true);
}