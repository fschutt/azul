#![allow(unused_variables)]
#![allow(unused_macros)]

use std::{
    collections::BTreeMap,
    sync::atomic::{Ordering, AtomicUsize},
    fmt::Debug,
};
use webrender::api::*;
use app_units::{AU_PER_PX, MIN_AU, MAX_AU, Au};
use euclid::{TypedRect, TypedSize2D};
use cassowary::{Constraint, Solver, Variable};

use {
    FastHashMap,
    resources::AppResources,
    traits::LayoutScreen,
    constraints::{DisplayRect, CssConstraint},
    ui_description::{UiDescription, StyledNode},
    window::{WindowDimensions, UiSolver},
    id_tree::{Arena, NodeId},
    css_parser::{self, *},
    dom::NodeData,
    css::Css,
    cache::DomChangeSet,
    ui_description::CssConstraintList,
};

const DEFAULT_FONT_COLOR: TextColor = TextColor(ColorU { r: 0, b: 0, g: 0, a: 255 });
const DEFAULT_BUILTIN_FONT_SANS_SERIF: css_parser::Font = Font::BuiltinFont("sans-serif");

pub(crate) struct DisplayList<'a, T: LayoutScreen + 'a> {
    pub(crate) ui_descr: &'a UiDescription<'a, T>,
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
    pub(crate) styled_node: &'a StyledNode<'a>,
    /// The style properties of the node, parsed
    pub(crate) style: RectStyle<'a>,
    /// The layout properties of the node, parsed
    pub(crate) layout: RectLayout,
}

/// It is not very efficient to re-create constraints on every call, the difference
/// in performance can be huge. Without re-creating constraints, solving can take 0.3 ms,
/// with re-creation it can take up to 9 ms. So the goal is to not re-create constraints
/// if their contents haven't changed.
#[derive(Default)]
pub(crate) struct SolvedLayout<T: LayoutScreen> {
    // List of previously solved constraints
    pub(crate) solved_constraints: FastHashMap<NodeId, NodeData<T>>,
}

impl<T: LayoutScreen> SolvedLayout<T> {
    pub fn empty() -> Self {
        Self {
            solved_constraints: FastHashMap::default(),
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

impl<'a, T: LayoutScreen + 'a> DisplayList<'a, T> {

    /// NOTE: This function assumes that the UiDescription has an initialized arena
    ///
    /// This only looks at the user-facing styles of the `UiDescription`, not the actual
    /// layout. The layout is done only in the `into_display_list_builder` step.
    pub fn new_from_ui_description(ui_description: &'a UiDescription<T>) -> Self {

        let arena = ui_description.ui_descr_arena.borrow();
        let display_rect_arena = arena.transform(|node, node_id| {
            let style = ui_description.styled_nodes.get(&node_id).unwrap_or(&ui_description.default_style_of_node);
            let mut rect = DisplayRectangle::new(node.tag, style);
            populate_css_properties(&mut rect);
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
        resource_updates: &mut ResourceUpdates)
    {
        Self::update_image_resources(api, app_resources, resource_updates);
        Self::update_font_resources(api, app_resources, resource_updates);
    }

    fn update_image_resources(
        api: &RenderApi,
        app_resources: &mut AppResources,
        resource_updates: &mut ResourceUpdates)
    {
        use images::{ImageState, ImageInfo};

        let mut updated_images = Vec::<(String, (ImageData, ImageDescriptor))>::new();
        let mut to_delete_images = Vec::<(String, Option<ImageKey>)>::new();

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
                resource_updates.delete_image(image_key);
            }
            app_resources.images.remove(&resource_key);
        }

        // Upload all remaining images to the GPU only if the haven't been
        // uploaded yet
        for (resource_key, (data, descriptor)) in updated_images.into_iter() {
            let key = api.generate_image_key();
            resource_updates.add_image(key, descriptor, data, None);
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
        resource_updates: &mut ResourceUpdates)
    {
        use font::FontState;

        let mut updated_fonts = Vec::<(::css_parser::Font, Vec<u8>)>::new();
        let mut to_delete_fonts = Vec::<(::css_parser::Font, Option<(FontKey, Vec<FontInstanceKey>)>)>::new();

        for (key, value) in app_resources.font_data.iter() {
            match value.1 {
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
                    resource_updates.delete_font_instance(instance);
                }
                resource_updates.delete_font(font_key);
                app_resources.fonts.remove(&font_key);
            }
            app_resources.font_data.remove(&resource_key);
        }

        // Upload all remaining fonts to the GPU only if the haven't been uploaded yet
        for (resource_key, data) in updated_fonts.into_iter() {
            let key = api.generate_font_key();
            resource_updates.add_raw_font(key, data, 0); // TODO: use the index better?
            app_resources.font_data.get_mut(&resource_key).unwrap().1 = FontState::Uploaded(key);
        }
    }

    pub fn into_display_list_builder(
        &self,
        pipeline_id: PipelineId,
        ui_solver: &mut UiSolver<T>,
        css: &mut Css,
        app_resources: &mut AppResources,
        render_api: &RenderApi,
        mut has_window_size_changed: bool)
    -> Option<DisplayListBuilder>
    {
        let mut changeset = None;
        if let Some(root) = self.ui_descr.ui_descr_root {
            let local_changeset = ui_solver.dom_tree_cache.update(root, &*(self.ui_descr.ui_descr_arena.borrow()));
            ui_solver.edit_variable_cache.initialize_new_rectangles(&mut ui_solver.solver, &local_changeset);
            ui_solver.edit_variable_cache.remove_unused_variables(&mut ui_solver.solver);
            changeset = Some(local_changeset);
        }

        if css.needs_relayout {

            // constraints were added or removed during the last frame
            for rect_idx in self.rectangles.linear_iter() {
                let rect = &self.rectangles[rect_idx].data;
                let arena = &*self.ui_descr.ui_descr_arena.borrow();
                let dom_hash = &ui_solver.dom_tree_cache.previous_layout.arena[rect_idx];
                let display_rect = ui_solver.edit_variable_cache.map[&dom_hash.data];
                let layout_contraints = create_layout_constraints(rect, rect_idx, &self.rectangles, &ui_solver.window_dimensions);
                let cassowary_constraints = css_constraints_to_cassowary_constraints(&display_rect.1, &layout_contraints);
                ui_solver.solver.add_constraints(&cassowary_constraints).unwrap();
            }

            // if we push or pop constraints that means we also need to re-layout the window
            has_window_size_changed = true;
        }

        let changeset_is_useless = match changeset {
            None => true,
            Some(c) => c.is_empty()
        };
/*
        // early return if we have nothing
        if !css.needs_relayout && changeset_is_useless && !has_window_size_changed {
            return None;
        }
*/

        // recalculate the actual layout
        if css.needs_relayout || has_window_size_changed {
            /*
                for change in solver.fetch_changes() {
                    println!("change: - {:?}", change);
                }
            */
        }

        css.needs_relayout = false;

        let layout_size = ui_solver.window_dimensions.layout_size;
        let mut builder = DisplayListBuilder::with_capacity(pipeline_id, layout_size, self.rectangles.nodes_len());
        let mut resource_updates = ResourceUpdates::new();
        let full_screen_rect = LayoutRect::new(LayoutPoint::zero(), builder.content_size());;

        // Upload image and font resources
        Self::update_resources(render_api, app_resources, &mut resource_updates);

        for rect_idx in self.rectangles.linear_iter() {
            let rect = &self.rectangles[rect_idx].data;

            // ask the solver what the bounds of the current rectangle is
            // let bounds = ui_solver.query_bounds_of_rect(*rect_idx);

            // debug rectangle
            let bounds = LayoutRect::new(LayoutPoint::new(0.0, 0.0), LayoutSize::new(200.0, 200.0));

            let info = LayoutPrimitiveInfo {
                rect: bounds,
                clip_rect: bounds,
                is_backface_visible: true,
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

            // TODO: expose 3D-transform in CSS
            // TODO: expose blend-modes in CSS
            // TODO: expose filters (blur, hue, etc.) in CSS
            builder.push_stacking_context(
                &info,
                clip_region_id,
                None,
                TransformStyle::Flat,
                None,
                MixBlendMode::Normal,
                Vec::new(),
                GlyphRasterSpace::Screen,
            );

            // Push the "outset" box shadow, before the clip is active
            push_box_shadow(
                &mut builder,
                &rect.style,
                &bounds,
                &full_screen_rect,
                BoxShadowClipMode::Outset);

            // Push clip
            if let Some(id) = clip_region_id {
                builder.push_clip_id(id);
            }

            push_rect(
                &info,
                &mut builder,
                &rect.style);

            push_background(
                &info,
                &bounds,
                &mut builder,
                &rect.style,
                &app_resources);

            // push the inset shadow (if any)
            push_box_shadow(&mut builder,
                            &rect.style,
                            &bounds,
                            &full_screen_rect,
                            BoxShadowClipMode::Inset);

            push_border(
                &info,
                &mut builder,
                &rect.style);

            push_text(
                &info,
                &self,
                rect_idx,
                &mut builder,
                &rect.style,
                app_resources,
                &render_api,
                &bounds,
                &mut resource_updates);

            // Pop clip
            if clip_region_id.is_some() {
                builder.pop_clip_id();
            }

            builder.pop_stacking_context();
        }

        render_api.update_resources(resource_updates);

        Some(builder)
    }
}

#[inline]
fn push_rect(info: &PrimitiveInfo<LayoutPixel>, builder: &mut DisplayListBuilder, style: &RectStyle) {
    match style.background_color {
        Some(bg) => builder.push_rect(&info, bg.0.into()),
        None => builder.push_clear_rect(&info),
    }
}

#[inline]
fn push_text<T: LayoutScreen>(
    info: &PrimitiveInfo<LayoutPixel>,
    display_list: &DisplayList<T>,
    rect_idx: NodeId,
    builder: &mut DisplayListBuilder,
    style: &RectStyle,
    app_resources: &mut AppResources,
    render_api: &RenderApi,
    bounds: &TypedRect<f32, LayoutPixel>,
    resource_updates: &mut ResourceUpdates)
{
    use dom::NodeType::*;
    use euclid::{TypedPoint2D, Length};
    use text_layout;
    use css_parser::{TextAlignment, TextOverflowBehaviour};

    // NOTE: If the text is outside the current bounds, webrender will not display the text, i.e. clip it
    let arena = display_list.ui_descr.ui_descr_arena.borrow();

    let text = match arena[rect_idx].data.node_type {
        Div => return,
        Label { ref text } => {
            text
        },
        _ => {
            /// The display list should only ever handle divs and labels.
            /// Everything more complex should be handled by a pre-processing step
            eprintln!("got a NodeType in a DisplayList that wasn't a div or a label, this is a bug");
            // unreachable!();
            return;
        }
    };

    if text.is_empty() {
        return;
    }

    let font_family = match style.font_family {
        Some(ref ff) => ff,
        None => return,
    };

    let font_size = style.font_size.unwrap_or(DEFAULT_FONT_SIZE);
    let font_size = Length::new(font_size.0.to_pixels());
    let font_size_app_units = (font_size.0 as i32) * AU_PER_PX;
    let font_id = font_family.fonts.get(0).unwrap_or(&DEFAULT_BUILTIN_FONT_SANS_SERIF);
    let font_size_app_units = Au(font_size_app_units as i32); // * text_layout::WEBRENDER_DPI_HACK) as i32
    let font_result = push_font(font_id, font_size_app_units,
        resource_updates, app_resources, render_api);

    let font_instance_key = match font_result {
        Some(f) => f,
        None => return,
    };

    let font = &app_resources.font_data[font_id].0;
    let alignment = style.text_align.unwrap_or(TextAlignment::default());
    let overflow_behaviour = style.text_overflow.unwrap_or(TextOverflowBehaviour::default());
    let positioned_glyphs = text_layout::put_text_in_bounds(
        text, font, font_size, alignment, overflow_behaviour, bounds);

    let font_color = style.font_color.unwrap_or(DEFAULT_FONT_COLOR).0.into();
    let flags = FontInstanceFlags::SUBPIXEL_BGR;
    let options = GlyphOptions {
        render_mode: FontRenderMode::Subpixel,
        flags: flags,
    };
    builder.push_text(&info, &positioned_glyphs, font_instance_key, font_color, Some(options));
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

        let origin_displace = pre_shadow.spread_radius - pre_shadow.blur_radius;
        clip_rect.origin.x = clip_rect.origin.x + pre_shadow.offset.x - origin_displace;
        clip_rect.origin.y = clip_rect.origin.y + pre_shadow.offset.y - origin_displace;

        let spread = (pre_shadow.spread_radius * 2.0) + (pre_shadow.blur_radius * 2.0);
        clip_rect.size.height = clip_rect.size.height + spread;
        clip_rect.size.width = clip_rect.size.width + spread;

        // prevent shadows that are larger than the full screen
        clip_rect.intersection(full_screen_rect).unwrap_or(clip_rect)
    };

    let info = LayoutPrimitiveInfo::with_clip_rect(LayoutRect::zero(), clip_rect);
    builder.push_box_shadow(&info, *bounds, pre_shadow.offset, pre_shadow.color,
                             pre_shadow.blur_radius, pre_shadow.spread_radius,
                             border_radius, pre_shadow.clip_mode);
}

#[inline]
fn push_background(
    info: &PrimitiveInfo<LayoutPixel>,
    bounds: &TypedRect<f32, LayoutPixel>,
    builder: &mut DisplayListBuilder,
    style: &RectStyle,
    app_resources: &AppResources)
{
    let background = match style.background {
        Some(ref bg) => bg,
        None => return,
    };

    match *background {
        Background::RadialGradient(ref gradient) => {
            let mut stops: Vec<GradientStop> = gradient.stops.iter().map(|gradient_pre|
                GradientStop {
                    offset: gradient_pre.offset.unwrap(),
                    color: gradient_pre.color,
                }).collect();
            let center = bounds.bottom_left(); // TODO - expose in CSS
            let radius = TypedSize2D::new(40.0, 40.0); // TODO - expose in CSS
            let gradient = builder.create_radial_gradient(center, radius, stops, gradient.extend_mode);
            builder.push_radial_gradient(&info, gradient, bounds.size, LayoutSize::zero());
        },
        Background::LinearGradient(ref gradient) => {
            let mut stops: Vec<GradientStop> = gradient.stops.iter().map(|gradient_pre|
                GradientStop {
                    offset: gradient_pre.offset.unwrap(),
                    color: gradient_pre.color,
                }).collect();
            let (begin_pt, end_pt) = gradient.direction.to_points(&bounds);
            let gradient = builder.create_gradient(begin_pt, end_pt, stops, gradient.extend_mode);
            builder.push_gradient(&info, gradient, bounds.size, LayoutSize::zero());
        },
        Background::Image(image_id) => {
            if let Some(image_info) = app_resources.images.get(image_id.0) {
                use images::ImageState::*;
                match *image_info {
                    Uploaded(ref image_info) => {
                        builder.push_image(
                                &info,
                                bounds.size,
                                LayoutSize::zero(),
                                ImageRendering::Auto,
                                AlphaType::Alpha,
                                image_info.key);
                    },
                    _ => { },
                }
            }
        }
    }
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
    font_id: &css_parser::Font,
    font_size_app_units: Au,
    resource_updates: &mut ResourceUpdates,
    app_resources: &mut AppResources,
    render_api: &RenderApi)
-> Option<FontInstanceKey>
{
    use font::FontState;

    if font_size_app_units < MIN_AU || font_size_app_units > MAX_AU {
        eprintln!("warning: too big or too small font size");
        return None;
    }

    let &(ref font, ref font_state) = match app_resources.font_data.get(font_id) {
        Some(f) => f,
        None => return None,
    };

    match *font_state {
        FontState::Uploaded(font_key) => {
            let font_sizes_hashmap = app_resources.fonts.entry(font_key)
                                     .or_insert(FastHashMap::default());
            let font_instance_key = font_sizes_hashmap.entry(font_size_app_units)
                .or_insert_with(|| {
                    let f_instance_key = render_api.generate_font_instance_key();
                    resource_updates.add_font_instance(
                        f_instance_key,
                        font_key,
                        font_size_app_units,
                        None,
                        None,
                        Vec::new(),
                    );
                    f_instance_key
                }
            );

            Some(*font_instance_key)
        },
        _ => {
            eprintln!("warning: trying to use font {:?} that isn't available", font_id);
            None
        },
    }
}

/// Populate and parse the CSS style properties
fn populate_css_properties(rect: &mut DisplayRectangle)
{
    for constraint in &rect.styled_node.css_constraints.list {
        use css::CssDeclaration::*;
        match constraint {
            Static(static_property) => {
                use css_parser::ParsedCssProperty::*;
                match static_property {
                    BorderRadius(b)             => { rect.style.border_radius = Some(*b);                   },
                    BackgroundColor(c)          => { rect.style.background_color = Some(*c);                },
                    TextColor(t)                => { rect.style.font_color = Some(*t);                      },
                    Border(widths, details)     => { rect.style.border = Some((*widths, *details));         },
                    Background(b)               => { rect.style.background = Some(b.clone());               },
                    FontSize(f)                 => { rect.style.font_size = Some(*f);                       },
                    FontFamily(f)               => { rect.style.font_family = Some(f.clone());              },
                    TextOverflow(to)            => { rect.style.text_overflow = Some(*to);                  },
                    TextAlign(ta)               => { rect.style.text_align = Some(*ta);                     },
                    BoxShadow(opt_box_shadow)   => { rect.style.box_shadow = *opt_box_shadow;               },

                    Width(w)                    => { rect.layout.width = Some(*w);                          },
                    Height(h)                   => { rect.layout.height = Some(*h);                         },
                    MinWidth(mw)                => { rect.layout.min_width = Some(*mw);                     },
                    MinHeight(mh)               => { rect.layout.min_height = Some(*mh);                    },
                    MaxWidth(mw)                => { rect.layout.max_width = Some(*mw);                     },
                    MaxHeight(mh)               => { rect.layout.max_height = Some(*mh);                    },

                    FlexWrap(w)                 => { rect.layout.wrap = Some(*w);                           },
                    FlexDirection(d)            => { rect.layout.direction = Some(*d);                      },
                    JustifyContent(j)           => { rect.layout.justify_content = Some(*j);                },
                    AlignItems(a)               => { rect.layout.align_items = Some(*a);                    },
                    AlignContent(a)             => { rect.layout.align_content = Some(*a);                  },
                }
            },
            Dynamic(_) => {
                // TODO
            }
        }
    }
}

// Returns the constraints for one rectangle
fn create_layout_constraints<'a>(
    rect: &DisplayRectangle,
    rect_id: NodeId,
    arena: &Arena<DisplayRectangle<'a>>,
    window_dimensions: &WindowDimensions)
-> Vec<CssConstraint>
{
    use css_parser;
    use cassowary::strength::*;
    use constraints::{SizeConstraint, Strength};

    let mut layout_constraints = Vec::<CssConstraint>::new();
    let max_width = arena.get_wh_for_rectangle(rect_id, WidthOrHeight::Width)
                         .unwrap_or(window_dimensions.layout_size.width);

    println!("max width for rectangle with the ID {} is: {}", rect_id, max_width);

    layout_constraints.push(CssConstraint::Size((SizeConstraint::Width(200.0), Strength(STRONG))));
    layout_constraints.push(CssConstraint::Size((SizeConstraint::Height(200.0), Strength(STRONG))));

    layout_constraints
}

fn css_constraints_to_cassowary_constraints(rect: &DisplayRect, css: &Vec<CssConstraint>)
-> Vec<Constraint>
{
    use self::CssConstraint::*;

    css.iter().flat_map(|constraint|
        match *constraint {
            Size((constraint, strength)) => {
                constraint.build(&rect, strength.0)
            }
            Padding((constraint, strength, padding)) => {
                constraint.build(&rect, strength.0, padding.0)
            }
        }
    ).collect()
}

// Layout / tracing-related functions

// What constraint (width or height) to search for when looking for a fitting width / height constraint
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
enum WidthOrHeight {
    Width,
    Height,
}

impl<'a> Arena<DisplayRectangle<'a>> {

    /// Recursive algorithm for getting the dimensions of a rectangle
    ///
    /// This function can be used on any rectangle to get the maximum allowed width
    /// (for inserting the width / height constraint into the layout solver).
    /// It simply traverses upwards through the nodes, until it finds a matching min-width / width
    /// constraint, returns None, if the root node is reached (with no constraints)
    ///
    /// Usually, you'd use it like:
    ///
    /// ```no_run,ignore
    /// let max_width = arena.get_wh_for_rectangle(id, WidthOrHeight::Width)
    ///                      .unwrap_or(window_dimensions.width);
    /// ```
    fn get_wh_for_rectangle(&self, id: NodeId, field: WidthOrHeight) -> Option<f32> {

        use self::WidthOrHeight::*;

        let node = &self[id];

        macro_rules! get_wh {
            ($field_name:ident, $min_field:ident) => ({
                let mut $field_name: Option<f32> = None;

                match node.data.layout.$min_field {
                    Some(m_w) => {
                        let m_w_px = m_w.0.to_pixels();
                        match node.data.layout.$field_name {
                            Some(w) => {
                                // width + min_width
                                let w_px = w.0.to_pixels();
                                $field_name = Some(m_w_px.max(w_px));
                            },
                            None => {
                                // min_width
                                $field_name = Some(m_w_px);
                            }
                        }
                    },
                    None => {
                        match node.data.layout.$field_name {
                            Some(w) => {
                                // width
                                let w_px = w.0.to_pixels();
                                $field_name = Some(w_px);
                            },
                            None => {
                                // neither width nor min_width
                            }
                        }
                    }
                };

                if $field_name.is_none() {
                    match node.parent() {
                        Some(p) => $field_name = self.get_wh_for_rectangle(p, field),
                        None => { },
                    }
                }

                $field_name
            })
        }

        match field {
            Width => {
                let w = get_wh!(width, min_width);
                w
            },
            Height => {
                let h = get_wh!(height, min_height);
                h
            }
        }
    }
}