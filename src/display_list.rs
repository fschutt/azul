#![allow(unused_variables)]
#![allow(unused_macros)]

use std::sync::{Arc, Mutex};
use webrender::api::*;
use app_units::{AU_PER_PX, MIN_AU, MAX_AU, Au};
use euclid::{TypedRect, TypedSize2D};
use {
    FastHashMap,
    app_resources::AppResources,
    traits::Layout,
    ui_description::{UiDescription, StyledNode},
    ui_solver::UiSolver,
    window_state::WindowSize,
    id_tree::{Arena, NodeId},
    css_parser::*,
    dom::{NodeData, NodeType::{self, *}},
    css::Css,
    text_layout::{TextOverflowPass2, ScrollbarInfo},
    images::ImageId,
    text_cache::TextId,
    compositor::new_opengl_texture_id,
    window::{WindowId, ReadOnlyWindow},
};

const DEFAULT_FONT_COLOR: TextColor = TextColor(ColorU { r: 0, b: 0, g: 0, a: 255 });

pub(crate) struct DisplayList<'a, T: Layout + 'a> {
    pub(crate) ui_descr: &'a UiDescription<T>,
    pub(crate) rectangles: Arena<DisplayRectangle<'a>>
}

/// DisplayRectangle is the main type which the layout parsing step gets operated on.
#[derive(Debug)]
pub(crate) struct DisplayRectangle<'a> {
    /// `Some(id)` if this rectangle has a callback attached to it
    /// Note: this is not the same as the `NodeId`!
    /// These two are completely seperate numbers!
    pub tag: Option<u64>,
    /// The original styled node
    pub(crate) styled_node: &'a StyledNode,
    /// The style properties of the node, parsed
    pub(crate) style: RectStyle,
    /// The layout properties of the node, parsed
    pub(crate) layout: RectLayout,
}

/// This is used for caching large strings (in the `push_text` function)
/// In the cached version, you can lookup the text as well as the dimensions of
/// the words in the `AppResources`. For the `Uncached` version, you'll have to re-
/// calculate it on every frame.
///
/// TODO: It should be possible to switch this over to a `&'a str`, but currently
/// this leads to unsolvable borrowing issues.
#[derive(Debug)]
pub(crate) enum TextInfo {
    Cached(TextId),
    Uncached(String),
}

impl TextInfo {
    /// Returns if the inner text is empty.
    ///
    /// Returns true if the TextInfo::Cached TextId does not exist
    /// (since in that case, it is "empty", so to speak)
    fn is_empty_text(&self, app_resources: &AppResources)
    -> bool
    {
        use self::TextInfo::*;

        match self {
            Cached(text_id) => {
                match app_resources.text_cache.string_cache.get(text_id) {
                    Some(s) => s.is_empty(),
                    None => true,
                }
            }
            Uncached(s) => s.is_empty(),
        }
    }
}

impl<'a> DisplayRectangle<'a> {
    #[inline]
    pub fn new(tag: Option<u64>, styled_node: &'a StyledNode) -> Self {
        Self {
            tag: tag,
            styled_node: styled_node,
            style: RectStyle::default(),
            layout: RectLayout::default(),
        }
    }
}

impl<'a, T: Layout + 'a> DisplayList<'a, T> {

    /// NOTE: This function assumes that the UiDescription has an initialized arena
    ///
    /// This only looks at the user-facing styles of the `UiDescription`, not the actual
    /// layout. The layout is done only in the `into_display_list_builder` step.
    pub fn new_from_ui_description(ui_description: &'a UiDescription<T>) -> Self {

        let arena = ui_description.ui_descr_arena.borrow();
        let display_rect_arena = arena.transform(|node, node_id| {
            let style = ui_description.styled_nodes.get(&node_id).unwrap_or(&ui_description.default_style_of_node);
            let mut rect = DisplayRectangle::new(node.tag, style);
            populate_css_properties(&mut rect, &ui_description.dynamic_css_overrides);
            rect
        });

        Self {
            ui_descr: ui_description,
            rectangles: display_rect_arena,
        }
    }

    /// Looks if any new images need to be uploaded and stores the in the image resources
    fn update_resources(
        api: &RenderApi,
        app_resources: &mut AppResources,
        resource_updates: &mut Vec<ResourceUpdate>)
    {
        Self::update_image_resources(api, app_resources, resource_updates);
        Self::update_font_resources(api, app_resources, resource_updates);
    }

    fn update_image_resources(
        api: &RenderApi,
        app_resources: &mut AppResources,
        resource_updates: &mut Vec<ResourceUpdate>)
    {
        use images::{ImageState, ImageInfo};

        let mut updated_images = Vec::<(ImageId, (ImageData, ImageDescriptor))>::new();
        let mut to_delete_images = Vec::<(ImageId, Option<ImageKey>)>::new();

        // possible performance bottleneck (duplicated cloning) !!
        for (key, value) in app_resources.images.iter() {
            match *value {
                ImageState::ReadyForUpload(ref d) => {
                    updated_images.push((key.clone(), d.clone()));
                },
                ImageState::Uploaded(_) => { },
                ImageState::AboutToBeDeleted(ref k) => {
                    to_delete_images.push((key.clone(), k.clone()));
                }
            }
        }

        // Remove any images that should be deleted
        for (resource_key, image_key) in to_delete_images.into_iter() {
            if let Some(image_key) = image_key {
                resource_updates.push(ResourceUpdate::DeleteImage(image_key));
            }
            app_resources.images.remove(&resource_key);
        }

        // Upload all remaining images to the GPU only if the haven't been
        // uploaded yet
        for (resource_key, (data, descriptor)) in updated_images.into_iter() {

            let key = api.generate_image_key();
            resource_updates.push(ResourceUpdate::AddImage(
                AddImage { key, descriptor, data, tiling: None }
            ));

            *app_resources.images.get_mut(&resource_key).unwrap() =
                ImageState::Uploaded(ImageInfo {
                    key: key,
                    descriptor: descriptor
            });
        }
    }

    // almost the same as update_image_resources, but fonts
    // have two HashMaps that need to be updated
    fn update_font_resources(
        api: &RenderApi,
        app_resources: &mut AppResources,
        resource_updates: &mut Vec<ResourceUpdate>)
    {
        use font::FontState;
        use css_parser::FontId;

        let mut updated_fonts = Vec::<(FontId, Vec<u8>)>::new();
        let mut to_delete_fonts = Vec::<(FontId, Option<(FontKey, Vec<FontInstanceKey>)>)>::new();

        for (key, value) in app_resources.font_data.borrow().iter() {
            match &*(*value.2).borrow() {
                FontState::ReadyForUpload(ref bytes) => {
                    updated_fonts.push((key.clone(), bytes.clone()));
                },
                FontState::Uploaded(_) => { },
                FontState::AboutToBeDeleted(ref font_key) => {
                    let to_delete_font_instances = font_key.and_then(|f_key| {
                        let to_delete_font_instances = app_resources.fonts[&f_key].values().cloned().collect();
                        Some((f_key.clone(), to_delete_font_instances))
                    });
                    to_delete_fonts.push((key.clone(), to_delete_font_instances));
                }
            }
        }

        // Delete the complete font. Maybe a more granular option to
        // keep the font data in memory should be added later
        for (resource_key, to_delete_instances) in to_delete_fonts.into_iter() {
            if let Some((font_key, font_instance_keys)) = to_delete_instances {
                for instance in font_instance_keys {
                    resource_updates.push(ResourceUpdate::DeleteFontInstance(instance));
                }
                resource_updates.push(ResourceUpdate::DeleteFont(font_key));
                app_resources.fonts.remove(&font_key);
            }
            app_resources.font_data.borrow_mut().remove(&resource_key);
        }

        // Upload all remaining fonts to the GPU only if the haven't been uploaded yet
        for (resource_key, data) in updated_fonts.into_iter() {
            let key = api.generate_font_key();
            resource_updates.push(ResourceUpdate::AddFont(AddFont::Raw(key, data, 0))); // TODO: use the index better?
            let mut borrow_mut = app_resources.font_data.borrow_mut();
            *borrow_mut.get_mut(&resource_key).unwrap().2.borrow_mut() = FontState::Uploaded(key);
        }
    }

    pub fn into_display_list_builder(
        &self,
        app_data: Arc<Mutex<T>>,
        pipeline_id: PipelineId,
        current_epoch: Epoch,
        ui_solver: &mut UiSolver,
        css: &mut Css,
        app_resources: &mut AppResources,
        render_api: &RenderApi,
        mut has_window_size_changed: bool,
        window_size: &WindowSize,
        window_id: WindowId,
        read_only_window: ReadOnlyWindow)
    -> DisplayListBuilder
    {
        use glium::glutin::dpi::LogicalSize;
        use std::collections::BTreeMap;

        let changeset = self.ui_descr.ui_descr_root.as_ref().and_then(|root| {
            let changeset = ui_solver.update_dom(root, &*(self.ui_descr.ui_descr_arena.borrow()));
            if changeset.is_empty() { None } else { Some(changeset) }
        });

        let root = match self.ui_descr.ui_descr_root {
            Some(r) => r,
            None => panic!("Dom has no root element!"),
        };

        if css.needs_relayout || changeset.is_some() {
            // inefficient for now, but prevents memory leak
            ui_solver.clear_all_constraints();
            for rect_idx in self.rectangles.linear_iter() {
                let constraints = create_layout_constraints(
                    rect_idx,
                    &self.rectangles,
                    &*self.ui_descr.ui_descr_arena.borrow(),
                    &ui_solver,
                );
                ui_solver.insert_css_constraints_for_rect(&constraints);
                ui_solver.push_added_constraints(rect_idx, constraints);
            }

            // If we push or pop constraints that means we also need to re-layout the window
            has_window_size_changed = true;
        }

        // TODO: early return based on changeset?

        // Recalculate the actual layout
        if has_window_size_changed {
            ui_solver.update_window_size(&window_size.dimensions);
        }
        ui_solver.update_layout_cache();

        css.needs_relayout = false;

        let LogicalSize { width, height } = window_size.dimensions;
        let mut builder = DisplayListBuilder::with_capacity(pipeline_id, TypedSize2D::new(width as f32, height as f32), self.rectangles.nodes_len());
        let mut resource_updates = Vec::<ResourceUpdate>::new();
        let full_screen_rect = LayoutRect::new(LayoutPoint::zero(), builder.content_size());;

        // Upload image and font resources
        Self::update_resources(render_api, app_resources, &mut resource_updates);

        let arena = self.ui_descr.ui_descr_arena.borrow();

        // Determine the correct implicit z-index rendering order of every rectangle
        let mut rects_in_rendering_order = BTreeMap::<usize, Vec<NodeId>>::new();

        for rect_id in self.rectangles.linear_iter() {

            // how many z-levels does this rectangle have until we get to the root?
            let z_index = {
                let mut index = 0;
                let mut cur_rect_idx = rect_id;
                while let Some(parent) = self.rectangles[cur_rect_idx].parent() {
                    index += 1;
                    cur_rect_idx = parent;
                }
                index
            };

            rects_in_rendering_order
                .entry(z_index)
                .or_insert_with(|| Vec::new())
                .push(rect_id);
        }

        for (z_index, rects) in rects_in_rendering_order.into_iter() {
            for rect_idx in rects {
                let bounds = ui_solver.query_bounds_of_rect(rect_idx);
                displaylist_handle_rect(
                    &mut builder,
                    current_epoch,
                    rect_idx,
                    &self.rectangles,
                    &arena[rect_idx].data.node_type,
                    bounds,
                    full_screen_rect,
                    app_resources,
                    render_api,
                    &mut resource_updates,
                    &app_data,
                    window_id,
                    read_only_window.clone());
            }
        }

        render_api.update_resources(resource_updates);

        builder
    }
}

fn displaylist_handle_rect<'a, T: Layout>(
    builder: &mut DisplayListBuilder,
    current_epoch: Epoch,
    rect_idx: NodeId,
    arena: &Arena<DisplayRectangle<'a>>,
    html_node: &NodeType<T>,
    bounds: TypedRect<f32, LayoutPixel>,
    full_screen_rect: TypedRect<f32, LayoutPixel>,
    app_resources: &mut AppResources,
    render_api: &RenderApi,
    resource_updates: &mut Vec<ResourceUpdate>,
    app_data: &Arc<Mutex<T>>,
    window_id: WindowId,
    read_only_window: ReadOnlyWindow)
{
    let rect = &arena[rect_idx].data;

    let info = LayoutPrimitiveInfo {
        rect: bounds,
        clip_rect: bounds,
        is_backface_visible: false,
        tag: rect.tag.and_then(|tag| Some((tag, 0))),
    };

    let clip_region_id = rect.style.border_radius.and_then(|border_radius| {
        let region = ComplexClipRegion {
            rect: bounds,
            radii: border_radius,
            mode: ClipMode::Clip,
        };
        Some(builder.define_clip(bounds, vec![region], None))
    });

    // Push the "outset" box shadow, before the clip is active
    push_box_shadow(
        builder,
        &rect.style,
        &bounds,
        &full_screen_rect,
        BoxShadowClipMode::Outset);

    if let Some(id) = clip_region_id {
        builder.push_clip_id(id);
    }

    if let Some(ref bg_col) = rect.style.background_color {
        push_rect(&info, builder, bg_col);
    }

    if let Some(ref bg) = rect.style.background {
        push_background(
            &info,
            &bounds,
            builder,
            bg,
            &app_resources);
    };

    // Push the inset shadow (if any)
    push_box_shadow(builder,
                    &rect.style,
                    &bounds,
                    &full_screen_rect,
                    BoxShadowClipMode::Inset);

    push_border(
        &info,
        builder,
        &rect.style);

    let (horz_alignment, vert_alignment) = determine_text_alignment(rect);

    let scrollbar_style = ScrollbarInfo {
        width: 17,
        padding: 2,
        background_color: BackgroundColor(ColorU { r: 241, g: 241, b: 241, a: 255 }),
        triangle_color: BackgroundColor(ColorU { r: 163, g: 163, b: 163, a: 255 }),
        bar_color: BackgroundColor(ColorU { r: 193, g: 193, b: 193, a: 255 }),
    };

    // The only thing changed between TextId and String is
    //`TextInfo::Cached` vs `TextInfo::Uncached` - reduce code duplication
    let push_text_wrapper = |
        text_info: &TextInfo,
        builder: &mut DisplayListBuilder,
        app_resources: &mut AppResources,
        resource_updates: &mut Vec<ResourceUpdate>|
    {
        // Adjust the bounds by the padding
        let mut text_bounds = rect.layout.padding.as_ref().and_then(|padding| {
            Some(subtract_padding(&bounds, padding))
        }).unwrap_or(bounds);

        text_bounds.size.width = text_bounds.size.width.max(0.0);
        text_bounds.size.height = text_bounds.size.height.max(0.0);
/*
        let text_clip_region_id = rect.layout.padding.and_then(|_|
            Some(builder.define_clip(text_bounds, vec![ComplexClipRegion {
                rect: text_bounds,
                radii: BorderRadius::zero(),
                mode: ClipMode::Clip,
            }], None))
        );

        if let Some(text_clip_id) = text_clip_region_id {
            builder.push_clip_id(text_clip_id);
        }
*/
        let overflow = push_text(
            &info,
            text_info,
            builder,
            &rect.style,
            app_resources,
            &render_api,
            &text_bounds,
            resource_updates,
            horz_alignment,
            vert_alignment,
            &scrollbar_style);
/*
        if text_clip_region_id.is_some() {
            builder.pop_clip_id();
        }
*/
        overflow
    };

    // handle the special content of the node
    let overflow_result = match html_node {
        Div => { None },
        Label(text) => {
            push_text_wrapper(&TextInfo::Uncached(text.clone()), builder, app_resources, resource_updates)
        },
        Text(text_id) => {
            push_text_wrapper(&TextInfo::Cached(*text_id), builder, app_resources, resource_updates)
        },
        Image(image_id) => {
            push_image(&info, builder, &bounds, app_resources, image_id)
        },
        GlTexture(texture_callback) => {

            use window::WindowInfo;

            let hidpi_factor = read_only_window.get_hidpi_factor();
            let width = (bounds.size.width * hidpi_factor as f32) as usize;
            let height = (bounds.size.height * hidpi_factor as f32) as usize;

            let t_locked = app_data.lock().unwrap();
            let window_info = WindowInfo {
                window_id: window_id,
                window: read_only_window,
                resources: &app_resources,
            };

            if let Some(texture) = (texture_callback.0)(&t_locked, window_info, width, height) {

                use compositor::{ActiveTexture, ACTIVE_GL_TEXTURES};

                let opaque = false;
                let allow_mipmaps = true;
                let descriptor = ImageDescriptor::new(texture.inner.width(), texture.inner.height(), ImageFormat::BGRA8, opaque, allow_mipmaps);
                let key = render_api.generate_image_key();
                let external_image_id = ExternalImageId(new_opengl_texture_id() as u64);

                let data = ImageData::External(ExternalImageData {
                    id: external_image_id,
                    channel_index: 0,
                    image_type: ExternalImageType::TextureHandle(TextureTarget::Default),
                });

                ACTIVE_GL_TEXTURES.lock().unwrap()
                    .entry(current_epoch).or_insert_with(|| FastHashMap::default())
                    .insert(external_image_id, ActiveTexture { texture: texture.clone() });

                resource_updates.push(ResourceUpdate::AddImage(
                    AddImage { key, descriptor, data, tiling: None }
                ));

                builder.push_image(
                    &info,
                    bounds.size,
                    LayoutSize::zero(),
                    ImageRendering::Auto,
                    AlphaType::Alpha,
                    key,
                    ColorF::WHITE);
            }

            None
        }
    };

    if let Some(overflow) = &overflow_result {
        // push scrollbars if necessary
        use text_layout::TextOverflow;

        // If the rectangle should have a scrollbar, push a scrollbar onto the display list
        if let TextOverflow::IsOverflowing(amount_vert) = overflow.text_overflow.vertical {
            push_scrollbar(builder, &overflow.text_overflow, &scrollbar_style, &bounds, &rect.style.border)
        }
        if let TextOverflow::IsOverflowing(amount_horz) = overflow.text_overflow.horizontal {
            push_scrollbar(builder, &overflow.text_overflow, &scrollbar_style, &bounds, &rect.style.border)
        }
    }

    if clip_region_id.is_some() {
        builder.pop_clip_id();
    }

}

#[inline]
fn push_rect(
    info: &PrimitiveInfo<LayoutPixel>,
    builder: &mut DisplayListBuilder,
    color: &BackgroundColor)
{
    builder.push_rect(&info, color.0.into());
}

struct OverflowInfo {
    pub text_overflow: TextOverflowPass2,
}

/// Note: automatically pushes the scrollbars on the parent,
/// this should be refined later
#[inline]
fn push_text(
    info: &PrimitiveInfo<LayoutPixel>,
    text: &TextInfo,
    builder: &mut DisplayListBuilder,
    style: &RectStyle,
    app_resources: &mut AppResources,
    render_api: &RenderApi,
    bounds: &TypedRect<f32, LayoutPixel>,
    resource_updates: &mut Vec<ResourceUpdate>,
    horz_alignment: TextAlignmentHorz,
    vert_alignment: TextAlignmentVert,
    scrollbar_info: &ScrollbarInfo)
-> Option<OverflowInfo>
{
    use text_layout;

    if text.is_empty_text(&*app_resources) {
        return None;
    }

    let font_family = match style.font_family {
        Some(ref ff) => ff,
        None => return None,
    };

    let font_size = style.font_size.unwrap_or(DEFAULT_FONT_SIZE);
    let font_size_app_units = Au((font_size.0.to_pixels() as i32) * AU_PER_PX as i32);
    let font_id = match font_family.fonts.get(0) { Some(s) => s, None => { error!("div @ {:?} has no font assigned!", bounds); return None; }};
    let font_result = push_font(font_id, font_size_app_units, resource_updates, app_resources, render_api);

    let font_instance_key = match font_result {
        Some(f) => f,
        None => return None,
    };

    let line_height = style.line_height;

    let overflow_behaviour = style.overflow.unwrap_or(LayoutOverflow::default());

    let (positioned_glyphs, text_overflow) = text_layout::get_glyphs(
        app_resources,
        bounds,
        horz_alignment,
        vert_alignment,
        &font_id,
        &font_size,
        line_height,
        text,
        &overflow_behaviour,
        scrollbar_info
    );

    let font_color = style.font_color.unwrap_or(DEFAULT_FONT_COLOR).0.into();
    let mut flags = FontInstanceFlags::empty();
    flags.set(FontInstanceFlags::SUBPIXEL_BGR, true);
    flags.set(FontInstanceFlags::FONT_SMOOTHING, true);
    flags.set(FontInstanceFlags::FORCE_AUTOHINT, true);
    flags.set(FontInstanceFlags::LCD_VERTICAL, true);

    let options = GlyphOptions {
        render_mode: FontRenderMode::Subpixel,
        flags: flags,
    };

    builder.push_text(&info, &positioned_glyphs, font_instance_key, font_color, Some(options));

    Some(OverflowInfo { text_overflow })
}

/// Adds a scrollbar to the left or bottom side of a rectangle.
/// TODO: make styling configurable (like the width / style of the scrollbar)
fn push_scrollbar(
    builder: &mut DisplayListBuilder,
    scrollbar_info: &TextOverflowPass2,
    scrollbar_style: &ScrollbarInfo,
    bounds: &TypedRect<f32, LayoutPixel>,
    border: &Option<(BorderWidths, BorderDetails)>)
{
    use euclid::TypedPoint2D;

    // The border is inside the rectangle - subtract the border width on the left and bottom side,
    // so that the scrollbar is laid out correctly
    let mut bounds = *bounds;
    if let Some((border_widths, _)) = border {
        bounds.size.width -= border_widths.left;
        bounds.size.height -= border_widths.bottom;
    }

    // Background of scrollbar (vertical)
    let scrollbar_vertical_background = TypedRect::<f32, LayoutPixel> {
        origin: TypedPoint2D::new(bounds.origin.x + bounds.size.width - scrollbar_style.width as f32, bounds.origin.y),
        size: TypedSize2D::new(scrollbar_style.width as f32, bounds.size.height),
    };

    let scrollbar_vertical_background_info = PrimitiveInfo {
        rect: scrollbar_vertical_background,
        clip_rect: bounds,
        is_backface_visible: false,
        tag: None, // TODO: for hit testing
    };

    push_rect(&scrollbar_vertical_background_info, builder, &scrollbar_style.background_color);

    // Actual scroll bar
    let scrollbar_vertical_bar = TypedRect::<f32, LayoutPixel> {
        origin: TypedPoint2D::new(
            bounds.origin.x + bounds.size.width - scrollbar_style.width as f32 + scrollbar_style.padding as f32,
            bounds.origin.y + scrollbar_style.width as f32),
        size: TypedSize2D::new(
            (scrollbar_style.width - (scrollbar_style.padding * 2)) as f32,
             bounds.size.height - (scrollbar_style.width * 2) as f32),
    };

    let scrollbar_vertical_bar_info = PrimitiveInfo {
        rect: scrollbar_vertical_bar,
        clip_rect: bounds,
        is_backface_visible: false,
        tag: None, // TODO: for hit testing
    };

    push_rect(&scrollbar_vertical_bar_info, builder, &scrollbar_style.bar_color);

    // Triangle top
    let mut scrollbar_triangle_rect = TypedRect::<f32, LayoutPixel> {
        origin: TypedPoint2D::new(
            bounds.origin.x + bounds.size.width - scrollbar_style.width as f32 + scrollbar_style.padding as f32,
            bounds.origin.y + scrollbar_style.padding as f32),
        size: TypedSize2D::new(
            (scrollbar_style.width - (scrollbar_style.padding * 2)) as f32,
            (scrollbar_style.width - (scrollbar_style.padding * 2)) as f32),
    };

    scrollbar_triangle_rect.origin.x += scrollbar_triangle_rect.size.width / 4.0;
    scrollbar_triangle_rect.origin.y += scrollbar_triangle_rect.size.height / 4.0;
    scrollbar_triangle_rect.size.width /= 2.0;
    scrollbar_triangle_rect.size.height /= 2.0;

    push_triangle(&scrollbar_triangle_rect, builder, &scrollbar_style.triangle_color, TriangleDirection::PointUp);

    // Triangle bottom
    scrollbar_triangle_rect.origin.y += bounds.size.height - scrollbar_style.width as f32 + scrollbar_style.padding as f32;
    push_triangle(&scrollbar_triangle_rect, builder, &scrollbar_style.triangle_color, TriangleDirection::PointDown);
}

enum TriangleDirection {
    PointUp,
    PointDown,
    PointRight,
    PointLeft,
}

fn push_triangle(
    bounds: &TypedRect<f32, LayoutPixel>,
    builder: &mut DisplayListBuilder,
    background_color: &BackgroundColor,
    direction: TriangleDirection)
{
    use self::TriangleDirection::*;

    // see: https://css-tricks.com/snippets/css/css-triangle/
    // uses the "3d effect" for making a triangle

    let triangle_rect_info = PrimitiveInfo {
        rect: *bounds,
        clip_rect: *bounds,
        is_backface_visible: false,
        tag: None,
    };

    const TRANSPARENT: ColorU = ColorU { r: 0, b: 0, g: 0, a: 0 };

    // make all borders but one transparent
    let [b_left, b_right, b_top, b_bottom] = match direction {
        PointUp         => [
            (TRANSPARENT, BorderStyle::Hidden),
            (TRANSPARENT, BorderStyle::Hidden),
            (TRANSPARENT, BorderStyle::Hidden),
            (background_color.0, BorderStyle::Solid)
        ],
        PointDown       => [
            (TRANSPARENT, BorderStyle::Hidden),
            (TRANSPARENT, BorderStyle::Hidden),
            (background_color.0, BorderStyle::Solid),
            (TRANSPARENT, BorderStyle::Hidden)
        ],
        PointLeft       => [
            (TRANSPARENT, BorderStyle::Hidden),
            (background_color.0, BorderStyle::Solid),
            (TRANSPARENT, BorderStyle::Hidden),
            (TRANSPARENT, BorderStyle::Hidden)
        ],
        PointRight      => [
            (background_color.0, BorderStyle::Solid),
            (TRANSPARENT, BorderStyle::Hidden),
            (TRANSPARENT, BorderStyle::Hidden),
            (TRANSPARENT, BorderStyle::Hidden)
        ],
    };

    let border_details = BorderDetails::Normal(NormalBorder {
        left:   BorderSide { color: b_left.0.into(),         style: b_left.1   },
        right:  BorderSide { color: b_right.0.into(),        style: b_right.1  },
        top:    BorderSide { color: b_top.0.into(),          style: b_top.1    },
        bottom: BorderSide { color: b_bottom.0.into(),       style: b_bottom.1 },
        radius: BorderRadius::zero(),
    });

    // make the borders half the width / height of the rectangle,
    // so that the border looks like a triangle
    let border_widths = BorderWidths {
        left: bounds.size.width / 2.0,
        top: bounds.size.height / 2.0,
        right: bounds.size.width / 2.0,
        bottom: bounds.size.height / 2.0,
    };

    builder.push_border(&triangle_rect_info, border_widths, border_details);
}

/// WARNING: For "inset" shadows, you must push a clip ID first, otherwise the
/// shadow will not show up.
///
/// To prevent a shadow from being pushed twice, you have to annotate the clip
/// mode for this - outset or inset.
#[inline]
fn push_box_shadow(
    builder: &mut DisplayListBuilder,
    style: &RectStyle,
    bounds: &TypedRect<f32, LayoutPixel>,
    full_screen_rect: &TypedRect<f32, LayoutPixel>,
    shadow_type: BoxShadowClipMode)
{
    let pre_shadow = match style.box_shadow {
        Some(ref ps) => ps,
        None => return,
    };

    // The pre_shadow is missing the BorderRadius & LayoutRect
    let border_radius = style.border_radius.unwrap_or(BorderRadius::zero());
    if pre_shadow.clip_mode != shadow_type {
        return;
    }

    let clip_rect = if pre_shadow.clip_mode == BoxShadowClipMode::Inset {
        // inset shadows do not work like outset shadows
        // for inset shadows, you have to push a clip ID first, so that they are
        // clipped to the bounds -we trust that the calling function knows to do this
        *bounds
    } else {
        // calculate the maximum extent of the outset shadow
        let mut clip_rect = *bounds;

        let origin_displace = (pre_shadow.spread_radius + pre_shadow.blur_radius) * 2.0;
        clip_rect.origin.x = clip_rect.origin.x - pre_shadow.offset.x - origin_displace;
        clip_rect.origin.y = clip_rect.origin.y - pre_shadow.offset.y - origin_displace;

        clip_rect.size.height = clip_rect.size.height + (origin_displace * 2.0);
        clip_rect.size.width = clip_rect.size.width + (origin_displace * 2.0);

        // prevent shadows that are larger than the full screen
        clip_rect.intersection(full_screen_rect).unwrap_or(clip_rect)
    };

    // Apply a gamma of 2.2 to the original value
    //
    // NOTE: strangely box-shadow is the only thing that needs to be gamma-corrected...
    fn apply_gamma(color: ColorF) -> ColorF {

        const GAMMA: f32 = 2.2;
        const GAMMA_F: f32 = 1.0 / GAMMA;

        ColorF {
            r: color.r.powf(GAMMA_F),
            g: color.g.powf(GAMMA_F),
            b: color.b.powf(GAMMA_F),
            a: color.a,
        }
    }

    let info = LayoutPrimitiveInfo::with_clip_rect(LayoutRect::zero(), clip_rect);
    builder.push_box_shadow(&info, *bounds, pre_shadow.offset, apply_gamma(pre_shadow.color),
                             pre_shadow.blur_radius, pre_shadow.spread_radius,
                             border_radius, pre_shadow.clip_mode);
}

#[inline]
fn push_background(
    info: &PrimitiveInfo<LayoutPixel>,
    bounds: &TypedRect<f32, LayoutPixel>,
    builder: &mut DisplayListBuilder,
    background: &Background,
    app_resources: &AppResources)
{
    match background {
        Background::RadialGradient(gradient) => {
            use css_parser::Shape;

            let mut stops: Vec<GradientStop> = gradient.stops.iter().map(|gradient_pre|
                GradientStop {
                    offset: gradient_pre.offset.unwrap(),
                    color: gradient_pre.color,
                }).collect();

            let center = bounds.center();

            // Note: division by 2.0 because it's the radius, not the diameter
            let radius = match gradient.shape {
                Shape::Ellipse => TypedSize2D::new(bounds.size.width / 2.0, bounds.size.height / 2.0),
                Shape::Circle => {
                    let largest_bound_size = bounds.size.width.max(bounds.size.height);
                    TypedSize2D::new(largest_bound_size / 2.0, largest_bound_size / 2.0)
                },
            };
            let gradient = builder.create_radial_gradient(center, radius, stops, gradient.extend_mode);
            builder.push_radial_gradient(&info, gradient, bounds.size, LayoutSize::zero());
        },
        Background::LinearGradient(gradient) => {

            let mut stops: Vec<GradientStop> = gradient.stops.iter().map(|gradient_pre|
                GradientStop {
                    offset: gradient_pre.offset.unwrap() / 100.0,
                    color: gradient_pre.color,
                }).collect();

            let (mut begin_pt, mut end_pt) = gradient.direction.to_points(&bounds);
            let gradient = builder.create_gradient(begin_pt, end_pt, stops, gradient.extend_mode);
            builder.push_gradient(&info, gradient, bounds.size, LayoutSize::zero());
        },
        Background::Image(css_image_id) => {
            if let Some(image_id) = app_resources.css_ids_to_image_ids.get(&css_image_id.0) {
                push_image(info, builder, bounds, app_resources, image_id);
            }
        },
        Background::NoBackground => { },
    }
}

#[inline]
fn push_image(
    info: &PrimitiveInfo<LayoutPixel>,
    builder: &mut DisplayListBuilder,
    bounds: &TypedRect<f32, LayoutPixel>,
    app_resources: &AppResources,
    image_id: &ImageId)
-> Option<OverflowInfo>
{
    use images::ImageState::*;

    let image_info = app_resources.images.get(image_id)?;

    match image_info {
        Uploaded(image_info) => {

            let mut image_bounds = *bounds;

            let image_key = image_info.key;
            let image_size = image_info.descriptor.size;

            // For now, adjust the width and height based on the
            if image_size.width < bounds.size.width as u32 && image_size.height < bounds.size.height as u32 {
                image_bounds.size.width = image_size.width as f32;
                image_bounds.size.height = image_size.height as f32;
            } else {
                let scale_factor_w = image_size.width as f32 / bounds.size.width;
                let scale_factor_h = image_size.height as f32 / bounds.size.height;

                if image_size.width < bounds.size.width as u32 {
                    // if the image fits horizontally
                    image_bounds.size.width = image_size.width as f32;
                    image_bounds.size.height = image_size.height as f32 * scale_factor_w;
                } else if image_size.height < bounds.size.height as u32 {
                    // if the image fits vertically
                    image_bounds.size.width = image_size.width as f32 * scale_factor_h;
                    image_bounds.size.height = image_size.height as f32;
                } else {
                    // image fits neither horizontally nor vertically
                    let scale_factor_smaller = scale_factor_w.max(scale_factor_w);
                    let new_width = image_size.width as f32 * scale_factor_smaller;
                    let new_height = image_size.height as f32 * scale_factor_smaller;
                    image_bounds.size.width = new_width;
                    image_bounds.size.height = new_height;
                }
            }

            // Just for testing
            image_bounds.size.width /= 2.0;
            image_bounds.size.height /= 2.0;

            builder.push_image(
                    &info,
                    image_bounds.size,
                    LayoutSize::zero(),
                    ImageRendering::Auto,
                    AlphaType::PremultipliedAlpha,
                    image_key,
                    ColorF::WHITE);
        },
        _ => { },
    }

    // TODO: determine if image has overflown its container
    None
}

#[inline]
fn push_border(
    info: &PrimitiveInfo<LayoutPixel>,
    builder: &mut DisplayListBuilder,
    style: &RectStyle)
{
    if let Some((border_widths, mut border_details)) = style.border {
        if let Some(border_radius) = style.border_radius {
            if let BorderDetails::Normal(ref mut n) = border_details {
                n.radius = border_radius;
            }
        }
        builder.push_border(info, border_widths, border_details);
    }
}

#[inline]
fn push_font(
    font_id: &FontId,
    font_size_app_units: Au,
    resource_updates: &mut Vec<ResourceUpdate>,
    app_resources: &mut AppResources,
    render_api: &RenderApi)
-> Option<FontInstanceKey>
{
    use font::FontState;

    if font_size_app_units < MIN_AU || font_size_app_units > MAX_AU {
        error!("warning: too big or too small font size");
        return None;
    }

    let font_state = app_resources.get_font_state(font_id)?;

    let borrow = font_state.borrow();

    match &*borrow {
        FontState::Uploaded(font_key) => {
            let font_sizes_hashmap = app_resources.fonts.entry(*font_key)
                                     .or_insert(FastHashMap::default());
            let font_instance_key = font_sizes_hashmap.entry(font_size_app_units)
                .or_insert_with(|| {
                    let f_instance_key = render_api.generate_font_instance_key();
                    resource_updates.push(ResourceUpdate::AddFontInstance(
                        AddFontInstance {
                            key: f_instance_key,
                            font_key: *font_key,
                            glyph_size: font_size_app_units,
                            options: None,
                            platform_options: None,
                            variations: Vec::new(),
                        }
                    ));
                    f_instance_key
                }
            );

            Some(*font_instance_key)
        },
        _ => {
            // This can happen when the font is loaded for the first time in `.get_font_state`
            // TODO: Make a pre-pass that queries and uploads all non-available fonts
            // error!("warning: trying to use font {:?} that isn't yet available", font_id);
            None
        },
    }
}

/// For a given rectangle, determines what text alignment should be used
fn determine_text_alignment<'a>(rect: &DisplayRectangle<'a>)
-> (TextAlignmentHorz, TextAlignmentVert)
{
    let mut horz_alignment = TextAlignmentHorz::default();
    let mut vert_alignment = TextAlignmentVert::default();

    if let Some(align_items) = rect.layout.align_items {
        // Vertical text alignment
        use css_parser::LayoutAlignItems;
        match align_items {
            LayoutAlignItems::Start => vert_alignment = TextAlignmentVert::Top,
            LayoutAlignItems::End => vert_alignment = TextAlignmentVert::Bottom,
            // technically stretch = blocktext, but we don't have that yet
            _ => vert_alignment = TextAlignmentVert::Center,
        }
    }

    if let Some(justify_content) = rect.layout.justify_content {
        use css_parser::LayoutJustifyContent;
        // Horizontal text alignment
        match justify_content {
            LayoutJustifyContent::Start => horz_alignment = TextAlignmentHorz::Left,
            LayoutJustifyContent::End => horz_alignment = TextAlignmentHorz::Right,
            _ => horz_alignment = TextAlignmentHorz::Center,
        }
    }

    if let Some(text_align) = rect.style.text_align {
        // Horizontal text alignment with higher priority
        horz_alignment = text_align;
    }

    (horz_alignment, vert_alignment)
}

/// Populate the CSS style properties of the `DisplayRectangle`
fn populate_css_properties(rect: &mut DisplayRectangle, css_overrides: &FastHashMap<String, ParsedCssProperty>)
{
    use css_parser::ParsedCssProperty::{self, *};

    fn apply_parsed_css_property(rect: &mut DisplayRectangle, property: &ParsedCssProperty) {
        match property {
            BorderRadius(b)             => { rect.style.border_radius = Some(*b);                   },
            BackgroundColor(c)          => { rect.style.background_color = Some(*c);                },
            TextColor(t)                => { rect.style.font_color = Some(*t);                      },
            Border(widths, details)     => { rect.style.border = Some((*widths, *details));         },
            Background(b)               => { rect.style.background = Some(b.clone());               },
            FontSize(f)                 => { rect.style.font_size = Some(*f);                       },
            FontFamily(f)               => { rect.style.font_family = Some(f.clone());              },
            Overflow(o)                 => {
                if let Some(ref mut existing_overflow) = rect.style.overflow {
                    existing_overflow.merge(o);
                } else {
                    rect.style.overflow = Some(*o)
                }
            },
            TextAlign(ta)               => { rect.style.text_align = Some(*ta);                     },
            BoxShadow(opt_box_shadow)   => { rect.style.box_shadow = *opt_box_shadow;               },
            LineHeight(lh)              => { rect.style.line_height = Some(*lh);                     },

            Width(w)                    => { rect.layout.width = Some(*w);                          },
            Height(h)                   => { rect.layout.height = Some(*h);                         },
            MinWidth(mw)                => { rect.layout.min_width = Some(*mw);                     },
            MinHeight(mh)               => { rect.layout.min_height = Some(*mh);                    },
            MaxWidth(mw)                => { rect.layout.max_width = Some(*mw);                     },
            MaxHeight(mh)               => { rect.layout.max_height = Some(*mh);                    },

            Position(p)                 => { rect.layout.position = Some(*p);                       },
            Top(t)                      => { rect.layout.top = Some(*t);                            },
            Bottom(b)                   => { rect.layout.bottom = Some(*b);                         },
            Right(r)                    => { rect.layout.right = Some(*r);                          },
            Left(l)                     => { rect.layout.left = Some(*l);                           },

            // TODO: merge new padding with existing padding
            Padding(p)                  => { rect.layout.padding = Some(*p);                        },

            FlexWrap(w)                 => { rect.layout.wrap = Some(*w);                           },
            FlexDirection(d)            => { rect.layout.direction = Some(*d);                      },
            JustifyContent(j)           => { rect.layout.justify_content = Some(*j);                },
            AlignItems(a)               => { rect.layout.align_items = Some(*a);                    },
            AlignContent(a)             => { rect.layout.align_content = Some(*a);                  },
        }
    }

    // Assert that the types of two properties matches
    fn property_type_matches(a: &ParsedCssProperty, b: &ParsedCssProperty) -> bool {
        use std::mem::discriminant;
        discriminant(a) == discriminant(b)
    }

    for constraint in &rect.styled_node.css_constraints.list {
        use css::CssDeclaration::*;
        match constraint {
            Static(static_property) => apply_parsed_css_property(rect, static_property),
            Dynamic(dynamic_property) => {
                let calculated_property = css_overrides.get(&dynamic_property.dynamic_id);
                if let Some(overridden_property) = calculated_property {
                    if property_type_matches(overridden_property, &dynamic_property.default) {
                        apply_parsed_css_property(rect, overridden_property);
                    } else {
                        error!(
                            "Dynamic CSS property on rect {:?} don't have the same discriminant type,\r\n
                            cannot override {:?} with {:?} - enum discriminant mismatch",
                            rect, dynamic_property.default, overridden_property
                        )
                    }
                } else {
                    apply_parsed_css_property(rect, &dynamic_property.default);
                }
            }
        }
    }
}

use cassowary::Constraint;

// Returns the constraints for one rectangle
fn create_layout_constraints<'a, T: Layout>(
    node_id: NodeId,
    display_rectangles: &Arena<DisplayRectangle<'a>>,
    dom: &Arena<NodeData<T>>,
    ui_solver: &UiSolver)
-> Vec<Constraint>
{
    use cassowary::{
        WeightedRelation::{EQ, GE, LE},
    };
    use ui_solver::RectConstraintVariables;
    use std::f64;

    const WEAK: f64 = 3.0;
    const MEDIUM: f64 = 30.0;
    const STRONG: f64 = 300.0;
    const REQUIRED: f64 = f64::MAX;

    let rect = &display_rectangles[node_id].data;
    let self_rect = ui_solver.get_rect_constraints(node_id).unwrap();

    let dom_node = &dom[node_id];

    let mut layout_constraints = Vec::new();

    let window_constraints = ui_solver.get_window_constraints();

    // Insert the max height and width constraints
    //
    // min-width and max-width are stronger than width because
    // the width has to be between min and max width

    // min-width, width, max-width
    if let Some(min_width) = rect.layout.min_width {
        layout_constraints.push(self_rect.width | GE(REQUIRED) | min_width.0.to_pixels());
    }
    if let Some(width) = rect.layout.width {
        layout_constraints.push(self_rect.width | EQ(STRONG) | width.0.to_pixels());
    } else {
        if let Some(parent) = dom_node.parent {
            let parent_rect = ui_solver.get_rect_constraints(parent).unwrap();
            layout_constraints.push(self_rect.width | EQ(STRONG) | parent_rect.width);
        } else {
            layout_constraints.push(self_rect.width | EQ(REQUIRED) | window_constraints.width_var);
        }
    }
    if let Some(max_width) = rect.layout.max_width {
        layout_constraints.push(self_rect.width | LE(REQUIRED) | max_width.0.to_pixels());
    }

    // min-height, height, max-height
    if let Some(min_height) = rect.layout.min_height {
        layout_constraints.push(self_rect.height | GE(REQUIRED) | min_height.0.to_pixels());
    }
    if let Some(height) = rect.layout.height {
        layout_constraints.push(self_rect.height | EQ(STRONG) | height.0.to_pixels());
    } else {
        if let Some(parent) = dom_node.parent {
            let parent_rect = ui_solver.get_rect_constraints(parent).unwrap();
            layout_constraints.push(self_rect.height | EQ(STRONG) | parent_rect.height);
        } else {
            layout_constraints.push(self_rect.height | EQ(REQUIRED) | window_constraints.height_var);
        }
    }
    if let Some(max_height) = rect.layout.max_height {
        layout_constraints.push(self_rect.height | LE(REQUIRED) | max_height.0.to_pixels());
    }

    // root node: start at (0, 0)
    if dom_node.parent.is_none() {
        layout_constraints.push(self_rect.top | EQ(REQUIRED) | 0.0);
        layout_constraints.push(self_rect.left | EQ(REQUIRED) | 0.0);
    }

    // Node has children: Push the constraints for `flex-direction`
    if dom_node.first_child.is_some() {

        let direction = rect.layout.direction.unwrap_or_default();

        let mut next_child_id = dom_node.first_child;
        let mut previous_child: Option<RectConstraintVariables> = None;

        // Iterate through children
        while let Some(child_id) = next_child_id {

            let child = &display_rectangles[child_id].data;
            let child_rect = ui_solver.get_rect_constraints(child_id).unwrap();

            let should_respect_relative_positioning = child.layout.position == Some(LayoutPosition::Relative);

            let (relative_top, relative_left, relative_right, relative_bottom) = if should_respect_relative_positioning {(
                child.layout.top.and_then(|top| Some(top.0.to_pixels())).unwrap_or(0.0),
                child.layout.left.and_then(|left| Some(left.0.to_pixels())).unwrap_or(0.0),
                child.layout.right.and_then(|right| Some(right.0.to_pixels())).unwrap_or(0.0),
                child.layout.right.and_then(|bottom| Some(bottom.0.to_pixels())).unwrap_or(0.0),
            )} else {
                (0.0, 0.0, 0.0, 0.0)
            };

            match direction {
                LayoutDirection::Row => {
                    match previous_child {
                        None => layout_constraints.push(child_rect.left | EQ(MEDIUM) | self_rect.left + relative_left),
                        Some(prev) => layout_constraints.push(child_rect.left | EQ(MEDIUM) | (prev.left + prev.width) + relative_left),
                    }
                    layout_constraints.push(child_rect.top | EQ(MEDIUM) | self_rect.top);
                },
                LayoutDirection::RowReverse => {
                    match previous_child {
                        None => layout_constraints.push(child_rect.left | EQ(MEDIUM) | (self_rect.left  + relative_left + (self_rect.width - child_rect.width))),
                        Some(prev) => layout_constraints.push((child_rect.left + child_rect.width) | EQ(MEDIUM) | prev.left + relative_left),
                    }
                    layout_constraints.push(child_rect.top | EQ(MEDIUM) | self_rect.top);
                },
                LayoutDirection::Column => {
                    match previous_child {
                        None => layout_constraints.push(child_rect.top | EQ(MEDIUM) | self_rect.top),
                        Some(prev) => layout_constraints.push(child_rect.top | EQ(MEDIUM) | (prev.top + prev.height)),
                    }
                    layout_constraints.push(child_rect.left | EQ(MEDIUM) | self_rect.left + relative_left);
                },
                LayoutDirection::ColumnReverse => {
                    match previous_child {
                        None => layout_constraints.push(child_rect.top | EQ(MEDIUM) | (self_rect.top + (self_rect.height - child_rect.height))),
                        Some(prev) => layout_constraints.push((child_rect.top + child_rect.height) | EQ(MEDIUM) | prev.top),
                    }
                    layout_constraints.push(child_rect.left | EQ(MEDIUM) | self_rect.left + relative_left);
                },
            }

            previous_child = Some(child_rect);
            next_child_id = dom[child_id].next_sibling;
        }
    }

    // Handle position: absolute
    if let Some(LayoutPosition::Absolute) = rect.layout.position {

        let top = rect.layout.top.and_then(|top| Some(top.0.to_pixels())).unwrap_or(0.0);
        let left = rect.layout.left.and_then(|left| Some(left.0.to_pixels())).unwrap_or(0.0);
        let right = rect.layout.right.and_then(|right| Some(right.0.to_pixels())).unwrap_or(0.0);
        let bottom = rect.layout.right.and_then(|bottom| Some(bottom.0.to_pixels())).unwrap_or(0.0);

        match get_nearest_positioned_ancestor(node_id, display_rectangles) {
            None => {
                // window is the nearest positioned ancestor
                // TODO: hacky magic that relies on having one root element
                let window_id = ui_solver.get_rect_constraints(NodeId::new(0)).unwrap();
                layout_constraints.push(self_rect.top | EQ(REQUIRED) | window_id.top + top);
                layout_constraints.push(self_rect.left | EQ(REQUIRED) | window_id.left + left);
            },
            Some(nearest_positioned) => {
                let nearest_positioned = ui_solver.get_rect_constraints(nearest_positioned).unwrap();
                layout_constraints.push(self_rect.top | GE(STRONG) | nearest_positioned.top + top);
                layout_constraints.push(self_rect.left | GE(STRONG) | nearest_positioned.left + left);
            }
        }
    }

    layout_constraints
}

/// Subtracts the padding from the bounds, returning the new bounds
///
/// Warning: The resulting rectangle may have negative width or height
fn subtract_padding(bounds: &TypedRect<f32, LayoutPixel>, padding: &LayoutPadding)
-> TypedRect<f32, LayoutPixel>
{
    let top     = padding.top.and_then(|top| Some(top.to_pixels())).unwrap_or(0.0);
    let bottom  = padding.bottom.and_then(|bottom| Some(bottom.to_pixels())).unwrap_or(0.0);
    let left    = padding.left.and_then(|left| Some(left.to_pixels())).unwrap_or(0.0);
    let right   = padding.right.and_then(|right| Some(right.to_pixels())).unwrap_or(0.0);

    let mut new_bounds = *bounds;

    new_bounds.origin.x += left;
    new_bounds.size.width -= right + left;
    new_bounds.origin.y += top;
    new_bounds.size.height -= top + bottom;

    new_bounds
}

/// Returns the nearest common ancestor with a `position: relative` attribute
/// or `None` if there is no ancestor that has `position: relative`. Usually
/// used in conjunction with `position: absolute`
fn get_nearest_positioned_ancestor<'a>(start_node_id: NodeId, arena: &Arena<DisplayRectangle<'a>>)
-> Option<NodeId>
{
    let mut current_node = start_node_id;
    while let Some(parent) = arena[current_node].parent() {
        // An element with position: absolute; is positioned relative to the nearest
        // positioned ancestor (instead of positioned relative to the viewport, like fixed).
        //
        // A "positioned" element is one whose position is anything except static.
        if let Some(LayoutPosition::Static) = arena[parent].data.layout.position {
            current_node = parent;
        } else {
            return Some(parent);
        }
    }
    None
}

// Empty test, for some reason codecov doesn't detect any files (and therefore
// doesn't report codecov % correctly) except if they have at least one test in
// the file. This is an empty test, which should be updated later on
#[test]
fn __codecov_test_display_list_file() {

}