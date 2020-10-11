use std::{
    fmt,
    collections::BTreeMap,
    rc::Rc,
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
    FastHashMap,
    callbacks::PipelineId,
    ui_solver::{
        PositionedRectangle, ResolvedOffsets, ExternalScrollId,
        LayoutResult, ScrolledNodes, OverflowingScrollNode
    },
    gl::Texture,
    window::{FullWindowState, LogicalSize},
    app_resources::{
        AppResources, AddImageMsg, FontImageApi, ImageDescriptor,
        ImageKey, FontInstanceKey, ImageInfo, ImageId, LayoutedGlyphs,
        Epoch, ExternalImageId, GlyphOptions, LoadFontFn, LoadImageFn,
    },
    ui_state::UiState,
    ui_description::{UiDescription, StyledNode},
    id_tree::{NodeDataContainer, NodeId, NodeHierarchy},
    dom::{
        DomId, NodeData, TagId, ScrollTagId, DomString,
        NodeType::{Div, Text, Image, GlTexture, IFrame, Label},
    },
};
use gleam::gl::Gl;

pub type GlyphIndex = u32;

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
pub struct GlyphInstance {
    pub index: GlyphIndex,
    pub point: LayoutPoint,
    pub size: LayoutSize,
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub struct CachedDisplayList {
    pub root: DisplayListMsg,
}

impl CachedDisplayList {
    pub fn empty(size: LayoutSize) -> Self {
        Self { root: DisplayListMsg::Frame(DisplayListFrame::root(size)) }
    }

    pub fn new<T>(
            epoch: Epoch,
            pipeline_id: PipelineId,
            full_window_state: &FullWindowState,
            ui_state_cache: &BTreeMap<DomId, UiState<T>>,
            layout_result_cache: &SolvedLayoutCache,
            gl_texture_cache: &GlTextureCache,
            app_resources: &AppResources,
    ) -> Self {
        const DOM_ID: DomId = DomId::ROOT_ID;
        CachedDisplayList {
            root: push_rectangles_into_displaylist(
                &layout_result_cache.rects_in_rendering_order[&DOM_ID],
                &DisplayListParametersRef {
                    dom_id: DOM_ID,
                    epoch,
                    full_window_state,
                    pipeline_id,
                    layout_result: layout_result_cache,
                    gl_texture_cache,
                    ui_state_cache,
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

#[derive(Clone, PartialEq, PartialOrd)]
pub struct DisplayListFrame {
    pub rect: LayoutRect,
    /// Border radius, set to none only if overflow: visible is set!
    pub border_radius: StyleBorderRadius,
    pub clip_rect: Option<LayoutRect>,
    pub tag: Option<TagId>,
    pub content: Vec<LayoutRectContent>,
    pub children: Vec<DisplayListMsg>,
}

impl fmt::Debug for DisplayListFrame {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let print_no_comma_rect =
            !self.border_radius.is_none() ||
            self.clip_rect.is_some() ||
            self.tag.is_some() ||
            !self.content.is_empty() ||
            !self.children.is_empty();

        write!(f, "rect: {:#?}{}", self.rect, if !print_no_comma_rect { "" } else { "," })?;

        if !self.border_radius.is_none() {
            write!(f, "\r\nborder_radius: {:#?}", self.border_radius)?;
        }
        if let Some(clip_rect) = &self.clip_rect {
            write!(f, "\r\nclip_rect: {:#?}", clip_rect)?;
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
    pub fn root(dimensions: LayoutSize) -> Self {
        DisplayListFrame {
            tag: None,
            clip_rect: None,
            rect: LayoutRect {
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
        clip: Option<LayoutRect>,
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
            Text { glyphs, font_instance_key, color, glyph_options, clip } => {
                write!(f,
                    "Text {{\r\n\
                        glyphs: {:?},\r\n\
                        font_instance_key: {:?},\r\n\
                        color: {:?},\r\n\
                        glyph_options: {:?},\r\n\
                        clip: {:?}\r\n\
                    }}",
                    glyphs, font_instance_key, color, glyph_options, clip,
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

pub struct DisplayList {
    pub rectangles: NodeDataContainer<DisplayRectangle>
}

impl DisplayList {

    /// NOTE: This function assumes that the UiDescription has an initialized arena
    pub fn new<T>(ui_description: &UiDescription, ui_state: &UiState<T>) -> Self {

        let arena = &ui_state.dom.arena;

        let mut override_warnings = Vec::new();

        let display_rect_arena = arena.node_data.transform(|_, node_id| {
            let tag = ui_state.node_ids_to_tag_ids.get(&node_id).map(|tag| *tag);
            let style = &ui_description.styled_nodes[node_id];
            let mut rect = DisplayRectangle::new(tag);
            override_warnings.append(&mut populate_css_properties(&mut rect, node_id, &ui_description.dynamic_css_overrides, &style));
            rect
        });

        #[cfg(feature = "logging")] {
            for warning in override_warnings {
                error!(
                    "Cannot override {} with {:?}",
                    warning.default.get_type(), warning.overridden_property,
                )
            }
        }

        DisplayList {
            rectangles: display_rect_arena,
        }
    }
}

/// Since the display list can take a lot of parameters, we don't want to
/// continually pass them as parameters of the function and rather use a
/// struct to pass them around. This is purely for ergonomic reasons.
///
/// `DisplayListParametersRef` has only members that are
///  **immutable references** to other things that need to be passed down the display list
#[derive(Clone)]
pub struct DisplayListParametersRef<'a, T: 'a> {
    /// ID of this Dom
    pub dom_id: DomId,
    /// Epoch of all the OpenGL textures
    pub epoch: Epoch,
    /// The CSS that should be applied to the DOM
    pub full_window_state: &'a FullWindowState,
    /// The current pipeline of the display list
    pub pipeline_id: PipelineId,
    /// Cached layouts (+ solved layouts for iframes)
    pub layout_result: &'a SolvedLayoutCache,
    /// Cached rendered OpenGL textures
    pub gl_texture_cache: &'a GlTextureCache,
    /// Reference to the UIState, for access to `node_hierarchy` and `node_data`
    pub ui_state_cache: &'a BTreeMap<DomId, UiState<T>>,
    /// Reference to the AppResources, necessary to query info about image and font keys
    pub app_resources: &'a AppResources,
}

/// DisplayRectangle is the main type which the layout parsing step gets operated on.
#[derive(Default, Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DisplayRectangle {
    /// `Some(id)` if this rectangle has a callback attached to it
    /// Note: this is not the same as the `NodeId`!
    /// These two are completely separate numbers!
    pub tag: Option<TagId>,
    /// The style properties of the node, parsed
    pub style: RectStyle,
    /// The layout properties of the node, parsed
    pub layout: RectLayout,
}

impl DisplayRectangle {
    #[inline]
    pub fn new(tag: Option<TagId>) -> Self {
        Self {
            tag,
            style: RectStyle::default(),
            layout: RectLayout::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ContentGroup {
    /// The parent of the current node group, i.e. either the root node (0)
    /// or the last positioned node ()
    pub root: NodeId,
    /// Node ids in order of drawing
    pub children: Vec<ContentGroup>,
}

#[derive(Default)]
pub struct SolvedLayoutCache {
    pub solved_layouts: BTreeMap<DomId, LayoutResult>,
    pub display_lists: BTreeMap<DomId, DisplayList>,
    pub iframe_mappings: BTreeMap<(DomId, NodeId), DomId>,
    pub scrollable_nodes: BTreeMap<DomId, ScrolledNodes>,
    pub rects_in_rendering_order: BTreeMap<DomId, ContentGroup>,
}

#[derive(Default)]
pub struct GlTextureCache {
    pub solved_textures: BTreeMap<DomId, BTreeMap<NodeId, (ImageKey, ImageDescriptor)>>,
}

// todo: very unclean, just so that
pub type LayoutFn<T> = fn(&NodeHierarchy, &NodeDataContainer<NodeData<T>>, &NodeDataContainer<DisplayRectangle>, &AppResources, &PipelineId, LayoutRect) -> LayoutResult;

// struct ImageData
// enum ExternalImageType
// enum TextureTarget
// enum ResourceUpdate

pub type GlStoreImageFn = fn(PipelineId, Epoch, Texture) -> ExternalImageId;

#[derive(Default)]
pub struct SolvedLayout {
    pub solved_layout_cache: SolvedLayoutCache,
    pub gl_texture_cache: GlTextureCache,
}

impl SolvedLayout {

    /// Does the layout, updates the image + font resources for the RenderAPI
    pub fn new<T, U: FontImageApi>(
        epoch: Epoch,
        pipeline_id: PipelineId,
        full_window_state: &FullWindowState,
        gl_context: Rc<dyn Gl>,
        render_api: &mut U,
        app_resources: &mut AppResources,
        ui_states: &mut BTreeMap<DomId, UiState<T>>,
        ui_descriptions: &mut BTreeMap<DomId, UiDescription>,
        insert_into_active_gl_textures: GlStoreImageFn,
        layout_func: LayoutFn<T>,
        load_font_fn: LoadFontFn,
        load_image_fn: LoadImageFn,
    ) -> Self {

        use crate::{
            app_resources::{
                RawImageFormat, AddImage, ExternalImageData, TextureTarget, ExternalImageType,
                ImageData, add_resources, garbage_collect_fonts_and_images,
            },
        };

        fn recurse<T, U: FontImageApi>(
            layout_cache: &mut SolvedLayoutCache,
            solved_textures: &mut BTreeMap<DomId, BTreeMap<NodeId, Texture>>,
            iframe_ui_states: &mut BTreeMap<DomId, UiState<T>>,
            iframe_ui_descriptions: &mut BTreeMap<DomId, UiDescription>,
            app_resources: &mut AppResources,
            render_api: &mut U,
            full_window_state: &FullWindowState,
            ui_state: &UiState<T>,
            ui_description: &UiDescription,
            pipeline_id: &PipelineId,
            bounds: LayoutRect,
            gl_context: Rc<dyn Gl>,
            layout_func: LayoutFn<T>,
            load_font_fn: LoadFontFn,
            load_image_fn: LoadImageFn,
        ) {
            use gleam::gl;
            use crate::{
                callbacks::{
                    HidpiAdjustedBounds, LayoutInfo,
                    IFrameCallbackInfo, GlCallbackInfo
                },
                app_resources::add_fonts_and_images,
            };

            // Right now the IFrameCallbacks and GlTextureCallbacks need to know how large their
            // containers are in order to be solved properly
            let display_list = DisplayList::new(ui_description, ui_state);
            let dom_id = ui_state.dom_id.clone();

            let rects_in_rendering_order = determine_rendering_order(
                &ui_state.dom.arena.node_layout,
                &display_list.rectangles
            );

            // In order to calculate the layout, font + image metrics have to be calculated first
            add_fonts_and_images(
                app_resources,
                render_api,
                &pipeline_id,
                &display_list,
                &ui_state.dom.arena.node_data,
                load_font_fn,
                load_image_fn,
            );

            let layout_result = (layout_func)(
                &ui_state.dom.arena.node_layout,
                &ui_state.dom.arena.node_data,
                &display_list.rectangles,
                &app_resources,
                pipeline_id,
                bounds,
            );

            let scrollable_nodes = get_nodes_that_need_scroll_clip(
                &ui_state.dom.arena.node_layout,
                &display_list.rectangles,
                &ui_state.dom.arena.node_data,
                &layout_result.rects,
                &layout_result.node_depths,
                *pipeline_id,
            );

            // Now the size of rects are known, render all the OpenGL textures
            for (node_id, (cb, ptr)) in ui_state.scan_for_gltexture_callbacks() {

                // Invoke OpenGL callback, render texture
                let rect_bounds = layout_result.rects[node_id].bounds;

                // TODO: Unused!
                let mut window_size_width_stops = Vec::new();
                let mut window_size_height_stops = Vec::new();

                let texture = {

                    let tex = (cb.0)(GlCallbackInfo {
                        state: ptr,
                        layout_info: LayoutInfo {
                            window_size: &full_window_state.size,
                            window_size_width_stops: &mut window_size_width_stops,
                            window_size_height_stops: &mut window_size_height_stops,
                            gl_context: gl_context.clone(),
                            resources: &app_resources,
                        },
                        bounds: HidpiAdjustedBounds::from_bounds(
                            rect_bounds,
                            full_window_state.size.hidpi_factor,
                            full_window_state.size.winit_hidpi_factor,
                        ),
                    });

                    // Reset the framebuffer and SRGB color target to 0
                    gl_context.bind_framebuffer(gl::FRAMEBUFFER, 0);
                    gl_context.disable(gl::FRAMEBUFFER_SRGB);
                    gl_context.disable(gl::MULTISAMPLE);

                    tex
                };

                if let Some(t) = texture {
                    solved_textures
                        .entry(dom_id.clone())
                        .or_insert_with(|| BTreeMap::default())
                        .insert(node_id, t);
                }
            }

            // Call IFrames and recurse
            for (node_id, (cb, ptr)) in ui_state.scan_for_iframe_callbacks() {

                let bounds = layout_result.rects[node_id].bounds;
                let hidpi_bounds = HidpiAdjustedBounds::from_bounds(
                    bounds,
                    full_window_state.size.hidpi_factor,
                    full_window_state.size.winit_hidpi_factor
                );

                // TODO: Unused!
                let mut window_size_width_stops = Vec::new();
                let mut window_size_height_stops = Vec::new();

                let iframe_dom = {
                    (cb.0)(IFrameCallbackInfo {
                        state: ptr,
                        layout_info: LayoutInfo {
                            window_size: &full_window_state.size,
                            window_size_width_stops: &mut window_size_width_stops,
                            window_size_height_stops: &mut window_size_height_stops,
                            gl_context: gl_context.clone(),
                            resources: &app_resources,
                        },
                        bounds: hidpi_bounds,
                    })
                };

                if let Some(iframe_dom) = iframe_dom {
                    let is_mouse_down = full_window_state.mouse_state.mouse_down();
                    let mut iframe_ui_state = UiState::new(iframe_dom, Some((dom_id.clone(), node_id)));
                    let iframe_dom_id = iframe_ui_state.dom_id.clone();
                    let hovered_nodes = full_window_state.hovered_nodes.get(&iframe_dom_id).cloned().unwrap_or_default();
                    let iframe_ui_description = UiDescription::new(
                        &mut iframe_ui_state,
                        &full_window_state.css,
                        &full_window_state.focused_node,
                        &hovered_nodes,
                        is_mouse_down,
                    );
                    layout_cache.iframe_mappings.insert((dom_id.clone(), node_id), iframe_dom_id.clone());
                    recurse(
                        layout_cache,
                        solved_textures,
                        iframe_ui_states,
                        iframe_ui_descriptions,
                        app_resources,
                        render_api,
                        full_window_state,
                        &iframe_ui_state,
                        &iframe_ui_description,
                        pipeline_id,
                        bounds,
                        gl_context.clone(),
                        layout_func,
                        load_font_fn,
                        load_image_fn,
                    );
                    iframe_ui_states.insert(iframe_dom_id.clone(), iframe_ui_state);
                    iframe_ui_descriptions.insert(iframe_dom_id.clone(), iframe_ui_description);
                }
            }

            layout_cache.solved_layouts.insert(dom_id.clone(), layout_result);
            layout_cache.display_lists.insert(dom_id.clone(), display_list);
            layout_cache.rects_in_rendering_order.insert(dom_id.clone(), rects_in_rendering_order);
            layout_cache.scrollable_nodes.insert(dom_id.clone(), scrollable_nodes);
        }

        let mut solved_layout_cache = SolvedLayoutCache::default();
        let mut solved_textures = BTreeMap::new();
        let mut iframe_ui_states = BTreeMap::new();
        let mut iframe_ui_descriptions = BTreeMap::new();

        for (dom_id, ui_state) in ui_states.iter_mut() {

            let ui_description = &ui_descriptions[dom_id];

            recurse(
                &mut solved_layout_cache,
                &mut solved_textures,
                &mut iframe_ui_states,
                &mut iframe_ui_descriptions,
                app_resources,
                render_api,
                full_window_state,
                ui_state,
                ui_description,
                &pipeline_id,
                LayoutRect {
                    origin: LayoutPoint::new(0.0, 0.0),
                    size: LayoutSize::new(full_window_state.size.dimensions.width, full_window_state.size.dimensions.height),
                },
                gl_context.clone(),
                layout_func,
                load_font_fn,
                load_image_fn,
            );
        }

        ui_states.extend(iframe_ui_states.into_iter());
        ui_descriptions.extend(iframe_ui_descriptions.into_iter());

        let mut gl_texture_cache = GlTextureCache {
            solved_textures: BTreeMap::new(),
        };

        let mut image_resource_updates = Vec::new();

        for (dom_id, textures) in solved_textures {
            for (node_id, texture) in textures {

            const TEXTURE_IS_OPAQUE: bool = false;
            // The texture gets mapped 1:1 onto the display, so there is no need for mipmaps
            const TEXTURE_ALLOW_MIPMAPS: bool = false;

            // Note: The ImageDescriptor has no effect on how large the image appears on-screen
            let descriptor = ImageDescriptor {
                format: RawImageFormat::RGBA8,
                dimensions: (texture.size.width as usize, texture.size.height as usize),
                stride: None,
                offset: 0,
                is_opaque: TEXTURE_IS_OPAQUE,
                allow_mipmaps: TEXTURE_ALLOW_MIPMAPS,
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

        Self {
            solved_layout_cache,
            gl_texture_cache,
        }
    }
}

pub fn determine_rendering_order<'a>(
    node_hierarchy: &NodeHierarchy,
    rectangles: &NodeDataContainer<DisplayRectangle>,
) -> ContentGroup {

    let children_sorted: BTreeMap<NodeId, Vec<NodeId>> = node_hierarchy
        .get_parents_sorted_by_depth()
        .into_iter()
        .map(|(_, parent_id)| (parent_id, sort_children_by_position(parent_id, node_hierarchy, rectangles)))
        .collect();

    let mut root_content_group = ContentGroup { root: NodeId::ZERO, children: Vec::new() };
    fill_content_group_children(&mut root_content_group, &children_sorted);
    root_content_group
}

pub fn fill_content_group_children(group: &mut ContentGroup, children_sorted: &BTreeMap<NodeId, Vec<NodeId>>) {
    if let Some(c) = children_sorted.get(&group.root) { // returns None for leaf nodes
        group.children = c
            .iter()
            .map(|child| ContentGroup { root: *child, children: Vec::new() })
            .collect();

        for c in &mut group.children {
            fill_content_group_children(c, children_sorted);
        }
    }
}

pub fn sort_children_by_position(
    parent: NodeId,
    node_hierarchy: &NodeHierarchy,
    rectangles: &NodeDataContainer<DisplayRectangle>
) -> Vec<NodeId> {
    use azul_css::LayoutPosition::*;

    let mut not_absolute_children = parent
        .children(node_hierarchy)
        .filter(|id| rectangles[*id].layout.position.and_then(|p| p.get_property_or_default()).unwrap_or_default() != Absolute)
        .collect::<Vec<NodeId>>();

    let mut absolute_children = parent
        .children(node_hierarchy)
        .filter(|id| rectangles[*id].layout.position.and_then(|p| p.get_property_or_default()).unwrap_or_default() == Absolute)
        .collect::<Vec<NodeId>>();

    // Append the position:absolute children after the regular children
    not_absolute_children.append(&mut absolute_children);
    not_absolute_children
}

/// Returns all node IDs where the children overflow the parent, together with the
/// `(parent_rect, child_rect)` - the child rect is the sum of the children.
///
/// TODO: The performance of this function can be theoretically improved:
///
/// - Unioning the rectangles is heavier than just looping through the children and
/// summing up their width / height / padding + margin.
/// - Scroll nodes only need to be inserted if the parent doesn't have `overflow: hidden`
/// activated
/// - Overflow for X and Y needs to be tracked seperately (for overflow-x / overflow-y separation),
/// so there we'd need to track in which direction the inner_rect is overflowing.
pub fn get_nodes_that_need_scroll_clip<T>(
    node_hierarchy: &NodeHierarchy,
    display_list_rects: &NodeDataContainer<DisplayRectangle>,
    dom_rects: &NodeDataContainer<NodeData<T>>,
    layouted_rects: &NodeDataContainer<PositionedRectangle>,
    parents: &[(usize, NodeId)],
    pipeline_id: PipelineId,
) -> ScrolledNodes {

    use azul_css::Overflow;

    let mut nodes = BTreeMap::new();
    let mut tags_to_node_ids = BTreeMap::new();

    for (_, parent) in parents {

        let parent_rect = &layouted_rects[*parent];

        let children_scroll_rect = match parent_rect.bounds.get_scroll_rect(parent.children(&node_hierarchy).map(|child_id| layouted_rects[child_id].bounds)) {
            None => continue,
            Some(sum) => sum,
        };

        // Check if the scroll rect overflows the parent bounds
        if contains_rect_rounded(&parent_rect.bounds, children_scroll_rect) {
            continue;
        }

        // If the overflow isn't "scroll", then there doesn't need to be a scroll frame
        if parent_rect.overflow == Overflow::Visible || parent_rect.overflow == Overflow::Hidden {
            continue;
        }

        let parent_dom_hash = dom_rects[*parent].calculate_node_data_hash();

        // Create an external scroll id. This id is required to preserve its
        // scroll state accross multiple frames.
        let parent_external_scroll_id  = ExternalScrollId(parent_dom_hash.0, pipeline_id);

        // Create a unique scroll tag for hit-testing
        let scroll_tag_id = match display_list_rects.get(*parent).and_then(|node| node.tag) {
            Some(existing_tag) => ScrollTagId(existing_tag),
            None => ScrollTagId::new(),
        };

        tags_to_node_ids.insert(scroll_tag_id, *parent);
        nodes.insert(*parent, OverflowingScrollNode {
            child_rect: children_scroll_rect,
            parent_external_scroll_id,
            parent_dom_hash,
            scroll_tag_id,
        });
    }

    ScrolledNodes { overflowing_nodes: nodes, tags_to_node_ids }
}

// Since there can be a small floating point error, round the item to the nearest pixel,
// then compare the rects
pub fn contains_rect_rounded(a: &LayoutRect, b: LayoutRect) -> bool {
    let a_x = a.origin.x.round() as isize;
    let a_y = a.origin.x.round() as isize;
    let a_width = a.size.width.round() as isize;
    let a_height = a.size.height.round() as isize;

    let b_x = b.origin.x.round() as isize;
    let b_y = b.origin.x.round() as isize;
    let b_width = b.size.width.round() as isize;
    let b_height = b.size.height.round() as isize;

    b_x >= a_x &&
    b_y >= a_y &&
    b_x + b_width <= a_x + a_width &&
    b_y + b_height <= a_y + a_height
}

pub fn node_needs_to_clip_children(layout: &RectLayout) -> bool {
    !(layout.is_horizontal_overflow_visible() || layout.is_vertical_overflow_visible())
}

pub fn push_rectangles_into_displaylist<'a, T>(
    root_content_group: &ContentGroup,
    referenced_content: &DisplayListParametersRef<'a, T>,
) -> DisplayListMsg {

    let mut content = displaylist_handle_rect(
        root_content_group.root,
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
pub fn displaylist_handle_rect<'a, T>(
    rect_idx: NodeId,
    referenced_content: &DisplayListParametersRef<'a, T>,
) -> DisplayListMsg {

    let DisplayListParametersRef {
        dom_id,
        pipeline_id,
        ui_state_cache,
        layout_result,
        gl_texture_cache,
        app_resources,
        full_window_state,
        ..
    } = referenced_content;

    let rect = &layout_result.display_lists[dom_id].rectangles[rect_idx];
    let bounds = &layout_result.solved_layouts[dom_id].rects[rect_idx].bounds;
    let html_node = &ui_state_cache[&dom_id].dom.arena.node_data[rect_idx].get_node_type();

    let display_list_rect_bounds = LayoutRect::new(
         LayoutPoint::new(bounds.origin.x, bounds.origin.y),
         LayoutSize::new(bounds.size.width, bounds.size.height),
    );

    let tag_id = rect.tag.or({
        layout_result.scrollable_nodes[dom_id].overflowing_nodes
        .get(&rect_idx)
        .map(|scrolled| scrolled.scroll_tag_id.0)
    });

    let mut frame = DisplayListFrame {
        tag: tag_id,
        clip_rect: None,
        border_radius: StyleBorderRadius {
            top_left: rect.style.border_top_left_radius,
            top_right: rect.style.border_top_right_radius,
            bottom_left: rect.style.border_bottom_left_radius,
            bottom_right: rect.style.border_bottom_right_radius,
        },
        rect: display_list_rect_bounds,
        content: Vec::new(),
        children: Vec::new(),
    };

    if rect.style.has_box_shadow() {
        frame.content.push(LayoutRectContent::BoxShadow {
            shadow: StyleBoxShadow {
                left: rect.style.box_shadow_left,
                right: rect.style.box_shadow_right,
                top: rect.style.box_shadow_top,
                bottom: rect.style.box_shadow_bottom,
            },
            clip_mode: BoxShadowClipMode::Outset,
        });
    }

    // If the rect is hit-testing relevant, we need to push a rect anyway.
    // Otherwise the hit-testing gets confused
    if let Some(bg) = rect.style.background.as_ref().and_then(|br| br.get_property()) {

        use azul_css::{CssImageId, StyleBackgroundContent::*};

        fn get_image_info(app_resources: &AppResources, pipeline_id: &PipelineId, style_image_id: &CssImageId) -> Option<RectBackground> {
            let image_id = app_resources.get_css_image_id(&style_image_id.0)?;
            let image_info = app_resources.get_image_info(pipeline_id, image_id)?;
            Some(RectBackground::Image(*image_info))
        }

        let background_content = match bg {
            LinearGradient(lg) => Some(RectBackground::LinearGradient(lg.clone())),
            RadialGradient(rg) => Some(RectBackground::RadialGradient(rg.clone())),
            Image(style_image_id) => get_image_info(&app_resources, &pipeline_id, style_image_id),
            Color(c) => Some(RectBackground::Color(*c)),
        };

        if let Some(background_content) = background_content {
            frame.content.push(LayoutRectContent::Background {
                content: background_content,
                size: rect.style.background_size.and_then(|bs| bs.get_property().cloned()),
                offset: rect.style.background_position.and_then(|bs| bs.get_property().cloned()),
                repeat: rect.style.background_repeat.and_then(|bs| bs.get_property().cloned()),
            });
        }
    }

    match html_node {
        Div => { },
        Text(_) | Label(_) => {
            if let Some(layouted_glyphs) = layout_result.solved_layouts.get(dom_id).and_then(|lr| lr.layouted_glyph_cache.get(&rect_idx)).cloned() {

                use crate::ui_solver::DEFAULT_FONT_COLOR;

                let text_color = rect.style.text_color.and_then(|tc| tc.get_property().cloned()).unwrap_or(DEFAULT_FONT_COLOR).0;
                let positioned_words = &layout_result.solved_layouts[dom_id].positioned_word_cache[&rect_idx];
                let font_instance_key = positioned_words.1;

                frame.content.push(get_text(
                    display_list_rect_bounds,
                    &layout_result.solved_layouts[dom_id].rects[rect_idx].padding,
                    full_window_state.size.dimensions,
                    layouted_glyphs,
                    font_instance_key,
                    text_color,
                    &rect.layout,
                ));
            }
        },
        Image(image_id) => {
            if let Some(image_info) = app_resources.get_image_info(pipeline_id, image_id) {
                frame.content.push(LayoutRectContent::Image {
                    size: LayoutSize::new(bounds.size.width, bounds.size.height),
                    offset: LayoutPoint::new(0.0, 0.0),
                    image_rendering: ImageRendering::Auto,
                    alpha_type: AlphaType::PremultipliedAlpha,
                    image_key: image_info.key,
                    background_color: ColorU::WHITE,
                });
            }
        },
        GlTexture(_) => {
            if let Some((key, descriptor)) = gl_texture_cache.solved_textures.get(&dom_id).and_then(|textures| textures.get(&rect_idx)) {
                frame.content.push(LayoutRectContent::Image {
                    size: LayoutSize::new(descriptor.dimensions.0 as f32, descriptor.dimensions.1 as f32),
                    offset: LayoutPoint::new(0.0, 0.0),
                    image_rendering: ImageRendering::Auto,
                    alpha_type: AlphaType::Alpha,
                    image_key: *key,
                    background_color: ColorU::WHITE,
                })
            }
        },
        IFrame(_) => {
            if let Some(iframe_dom_id) = layout_result.iframe_mappings.get(&(dom_id.clone(), rect_idx)) {
                frame.children.push(push_rectangles_into_displaylist(
                    &layout_result.rects_in_rendering_order[&iframe_dom_id],
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

    if rect.style.has_border() {
        frame.content.push(LayoutRectContent::Border {
            widths: StyleBorderWidths {
                top: rect.layout.border_top_width,
                left: rect.layout.border_left_width,
                bottom: rect.layout.border_bottom_width,
                right: rect.layout.border_right_width,
            },
            colors: StyleBorderColors {
                top: rect.style.border_top_color,
                left: rect.style.border_left_color,
                bottom: rect.style.border_bottom_color,
                right: rect.style.border_right_color,
            },
            styles: StyleBorderStyles {
                top: rect.style.border_top_style,
                left: rect.style.border_left_style,
                bottom: rect.style.border_bottom_style,
                right: rect.style.border_right_style,
            },
        });
    }

    if rect.style.has_box_shadow() {
        frame.content.push(LayoutRectContent::BoxShadow {
            shadow: StyleBoxShadow {
                left: rect.style.box_shadow_left,
                right: rect.style.box_shadow_right,
                top: rect.style.box_shadow_top,
                bottom: rect.style.box_shadow_bottom,
            },
            clip_mode: BoxShadowClipMode::Inset,
        });
    }

    match layout_result.scrollable_nodes[dom_id].overflowing_nodes.get(&rect_idx) {
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
    bounds: LayoutRect,
    padding: &ResolvedOffsets,
    root_window_size: LogicalSize,
    layouted_glyphs: LayoutedGlyphs,
    font_instance_key: FontInstanceKey,
    font_color: ColorU,
    rect_layout: &RectLayout,
) -> LayoutRectContent {

    let overflow_horizontal_visible = rect_layout.is_horizontal_overflow_visible();
    let overflow_vertical_visible = rect_layout.is_horizontal_overflow_visible();

    let padding_clip_bounds = subtract_padding(&bounds, padding);

    // Adjust the bounds by the padding, depending on the overflow:visible parameter
    let text_clip_rect = match (overflow_horizontal_visible, overflow_vertical_visible) {
        (true, true) => None,
        (false, false) => Some(padding_clip_bounds),
        (true, false) => {
            // Horizontally visible, vertically cut
            Some(LayoutRect {
                origin: bounds.origin,
                size: LayoutSize::new(root_window_size.width, padding_clip_bounds.size.height),
            })
        },
        (false, true) => {
            // Vertically visible, horizontally cut
            Some(LayoutRect {
                origin: bounds.origin,
                size: LayoutSize::new(padding_clip_bounds.size.width, root_window_size.height),
            })
        },
    };

    LayoutRectContent::Text {
        glyphs: layouted_glyphs.glyphs,
        font_instance_key,
        color: font_color,
        glyph_options: None,
        clip: text_clip_rect,
    }
}

/// Subtracts the padding from the bounds, returning the new bounds
///
/// Warning: The resulting rectangle may have negative width or height
pub fn subtract_padding(bounds: &LayoutRect, padding: &ResolvedOffsets) -> LayoutRect {

    let mut new_bounds = *bounds;

    new_bounds.origin.x += padding.left;
    new_bounds.size.width -= padding.right + padding.left;
    new_bounds.origin.y += padding.top;
    new_bounds.size.height -= padding.top + padding.bottom;

    new_bounds
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct OverrideWarning {
    pub default: CssProperty,
    pub overridden_property: CssProperty,
}

/// Populate the style properties of the `DisplayRectangle`, apply static / dynamic properties
pub fn populate_css_properties(
    rect: &mut DisplayRectangle,
    node_id: NodeId,
    css_overrides: &BTreeMap<NodeId, FastHashMap<DomString, CssProperty>>,
    styled_node: &StyledNode,
) -> Vec<OverrideWarning> {

    use azul_css::CssDeclaration::*;
    use std::mem;

    let rect_style = &mut rect.style;
    let rect_layout = &mut rect.layout;
    let css_constraints = &styled_node.css_constraints;

   css_constraints
    .values()
    .filter_map(|constraint| match constraint {
        Static(static_property) => {
            apply_style_property(rect_style, rect_layout, &static_property);
            None
        },
        Dynamic(dynamic_property) => {
            let overridden_property = css_overrides.get(&node_id).and_then(|overrides| overrides.get(&dynamic_property.dynamic_id.clone().into()))?;

            // Apply the property default if the discriminant of the two types matches
            if mem::discriminant(overridden_property) == mem::discriminant(&dynamic_property.default_value) {
                apply_style_property(rect_style, rect_layout, overridden_property);
                None
            } else {
                Some(OverrideWarning {
                    default: dynamic_property.default_value.clone(),
                    overridden_property: overridden_property.clone(),
                })
            }
        },
    })
    .collect()
}

pub fn apply_style_property(style: &mut RectStyle, layout: &mut RectLayout, property: &CssProperty) {

    use azul_css::CssProperty::*;

    match property {

        Display(d)                      => layout.display = Some(*d),
        Float(f)                        => layout.float = Some(*f),
        BoxSizing(bs)                   => layout.box_sizing = Some(*bs),

        TextColor(c)                    => style.text_color = Some(*c),
        FontSize(fs)                    => style.font_size = Some(*fs),
        FontFamily(ff)                  => style.font_family = Some(ff.clone()),
        TextAlign(ta)                   => style.text_align = Some(*ta),

        LetterSpacing(ls)               => style.letter_spacing = Some(*ls),
        LineHeight(lh)                  => style.line_height = Some(*lh),
        WordSpacing(ws)                 => style.word_spacing = Some(*ws),
        TabWidth(tw)                    => style.tab_width = Some(*tw),
        Cursor(c)                       => style.cursor = Some(*c),

        Width(w)                        => layout.width = Some(*w),
        Height(h)                       => layout.height = Some(*h),
        MinWidth(mw)                    => layout.min_width = Some(*mw),
        MinHeight(mh)                   => layout.min_height = Some(*mh),
        MaxWidth(mw)                    => layout.max_width = Some(*mw),
        MaxHeight(mh)                   => layout.max_height = Some(*mh),

        Position(p)                     => layout.position = Some(*p),
        Top(t)                          => layout.top = Some(*t),
        Bottom(b)                       => layout.bottom = Some(*b),
        Right(r)                        => layout.right = Some(*r),
        Left(l)                         => layout.left = Some(*l),

        FlexWrap(fw)                    => layout.wrap = Some(*fw),
        FlexDirection(fd)               => layout.direction = Some(*fd),
        FlexGrow(fg)                    => layout.flex_grow = Some(*fg),
        FlexShrink(fs)                  => layout.flex_shrink = Some(*fs),
        JustifyContent(jc)              => layout.justify_content = Some(*jc),
        AlignItems(ai)                  => layout.align_items = Some(*ai),
        AlignContent(ac)                => layout.align_content = Some(*ac),

        BackgroundContent(bc)           => style.background = Some(bc.clone()),
        BackgroundPosition(bp)          => style.background_position = Some(*bp),
        BackgroundSize(bs)              => style.background_size = Some(*bs),
        BackgroundRepeat(br)            => style.background_repeat = Some(*br),

        OverflowX(ox)                   => layout.overflow_x = Some(*ox),
        OverflowY(oy)                   => layout.overflow_y = Some(*oy),

        PaddingTop(pt)                  => layout.padding_top = Some(*pt),
        PaddingLeft(pl)                 => layout.padding_left = Some(*pl),
        PaddingRight(pr)                => layout.padding_right = Some(*pr),
        PaddingBottom(pb)               => layout.padding_bottom = Some(*pb),

        MarginTop(mt)                   => layout.margin_top = Some(*mt),
        MarginLeft(ml)                  => layout.margin_left = Some(*ml),
        MarginRight(mr)                 => layout.margin_right = Some(*mr),
        MarginBottom(mb)                => layout.margin_bottom = Some(*mb),

        BorderTopLeftRadius(btl)        => style.border_top_left_radius = Some(*btl),
        BorderTopRightRadius(btr)       => style.border_top_right_radius = Some(*btr),
        BorderBottomLeftRadius(bbl)     => style.border_bottom_left_radius = Some(*bbl),
        BorderBottomRightRadius(bbr)    => style.border_bottom_right_radius = Some(*bbr),

        BorderTopColor(btc)             => style.border_top_color = Some(*btc),
        BorderRightColor(brc)           => style.border_right_color = Some(*brc),
        BorderLeftColor(blc)            => style.border_left_color = Some(*blc),
        BorderBottomColor(bbc)          => style.border_bottom_color = Some(*bbc),

        BorderTopStyle(bts)             => style.border_top_style = Some(*bts),
        BorderRightStyle(brs)           => style.border_right_style = Some(*brs),
        BorderLeftStyle(bls)            => style.border_left_style = Some(*bls),
        BorderBottomStyle(bbs)          => style.border_bottom_style = Some(*bbs),

        BorderTopWidth(btw)             => layout.border_top_width = Some(*btw),
        BorderRightWidth(brw)           => layout.border_right_width = Some(*brw),
        BorderLeftWidth(blw)            => layout.border_left_width = Some(*blw),
        BorderBottomWidth(bbw)          => layout.border_bottom_width = Some(*bbw),

        BoxShadowLeft(bsl)              => style.box_shadow_left = Some(*bsl),
        BoxShadowRight(bsr)             => style.box_shadow_right = Some(*bsr),
        BoxShadowTop(bst)               => style.box_shadow_top = Some(*bst),
        BoxShadowBottom(bsb)            => style.box_shadow_bottom = Some(*bsb),
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