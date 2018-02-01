use webrender::api::*;
use traits::LayoutScreen;
use constraints::{DisplayRect, CssConstraint};
use ui_description::{UiDescription, StyledNode};
use cassowary::{Constraint, Solver};
use id_tree::{Arena, NodeId};
use css_parser::*;
use dom::NodeData;
use std::collections::BTreeMap;

pub(crate) struct DisplayList<'a, T: LayoutScreen + 'a> {
    pub(crate) ui_descr: &'a UiDescription<T>,
    pub(crate) rectangles: BTreeMap<NodeId, DisplayRectangle<'a>>
}

pub(crate) struct DisplayRectangle<'a> {
    /// `Some(id)` if this rectangle has a callback attached to it 
    /// Note: this is not the same as the `NodeId`! 
    /// These two are completely seperate numbers!
    pub tag: Option<u64>,
    /// The actual rectangle
    pub(crate) rect: DisplayRect,
    /// Background color of this rectangle
    pub(crate) background_color: Option<ColorU>,
    /// Shadow color
    pub(crate) box_shadow: Option<BoxShadowPreDisplayItem>,
    /// Gradient (location) + stops
    pub(crate) background: Option<ParsedGradient>,
    /// Border
    pub(crate) border: Option<(BorderWidths, BorderDetails)>,
    /// border radius
    pub(crate) border_radius: Option<BorderRadius>,
    /// The original styled node
    pub(crate) styled_node: &'a StyledNode,
}

impl<'a> DisplayRectangle<'a> {
    #[inline]
    pub fn new(tag: Option<u64>, styled_node: &'a StyledNode) -> Self {
        Self {
            tag: tag,
            rect: DisplayRect::default(),
            background_color: None,
            box_shadow: None,
            background: None,
            border: None,
            border_radius: None,
            styled_node: styled_node,
        }
    }
}
impl<'a, T: LayoutScreen> DisplayList<'a, T> {

    /// NOTE: This function assumes that the UiDescription has an initialized arena
    /// 
    /// This only looks at the user-facing styles of the `UiDescription`, not the actual
    /// layout. The layout is done only in the `into_display_list_builder` step.
    pub fn new_from_ui_description(ui_description: &'a UiDescription<T>) -> Self {

        let arena = &ui_description.arena;

        let mut rect_btree = BTreeMap::new();

        for node in &ui_description.styled_nodes {
            let mut rect = DisplayRectangle::new(arena.borrow()[node.id].data.tag, &node);
            parse_css(&mut rect);
            rect_btree.insert(node.id, rect);
        }

        Self {
            ui_descr: ui_description,
            rectangles: rect_btree,
        }
    }

    pub fn into_display_list_builder(&self, pipeline_id: PipelineId, layout_size: LayoutSize, hidpi_factor: f32, layout_solver: &mut Solver)
    -> DisplayListBuilder
    {
        let mut builder = DisplayListBuilder::new(pipeline_id, layout_size);
        
        // println!("---------- start creating builder ------------");

        for rect in self.rectangles.values() {

            // TODO: split up layout and constraints better
            let mut layout_contraints = Vec::<CssConstraint>::new();
            {
                let arena = &*self.ui_descr.arena.borrow();
                create_layout_constraints(&rect.rect, arena, rect.styled_node, layout_size, &mut layout_contraints);
            }
            let cassowary_constraints = css_constraints_to_cassowary_constraints(&rect.rect, &layout_contraints);
            layout_solver.add_constraints(&cassowary_constraints).unwrap();
/*
            for change in solver.fetch_changes() {
                println!("change: - {:?}", change);
            }
*/
            let bounds1 = LayoutRect::new(
                LayoutPoint::new(0.0, 0.0),
                LayoutSize::new(200.0, 200.0),
            );
            let bounds2 = LayoutRect::new(
                LayoutPoint::new(0.0, 200.0),
                LayoutSize::new(200.0, 200.0),
            );

            // debugging - there are currently two rectangles on the screen
            // if the rectangle doesn't have a background color, choose the first bound
            //
            // this means, since the DOM in the debug example has two rectangles, we should
            // have two touching rectangles
            let mut bounds = if rect.background_color.is_some() { 
                bounds1 
            } else { 
                bounds2 
            };
/*
            // bug - for some reason, the origin gets scaled by 2.0, 
            // even if the HiDpi factor is set to 1.0
            println!("pushing rectangle ... ");
            println!("bounds: {:?}", bounds);
            println!("hidpi_factor: {:?}", hidpi_factor);
            println!("window size: {:?}", layout_size);
*/
            // this is a workaround, this seems to be a bug in webrender
            // bounds.origin.x /= 2.0;
            // bounds.origin.y /= 2.0;

            let clip = if let Some(border_radius) = rect.border_radius {
                LocalClip::RoundedRect(bounds, ComplexClipRegion {
                    rect: bounds,
                    radii: border_radius,
                    mode: ClipMode::Clip,
                })
            } else {
                LocalClip::Rect(bounds)
            };

            let info = LayoutPrimitiveInfo {
                rect: bounds,
                is_backface_visible: false,
                tag: rect.tag.and_then(|tag| Some((tag, 0))),
                local_clip: clip,
            };

            builder.push_stacking_context(
                &info,
                ScrollPolicy::Scrollable,
                None,
                TransformStyle::Flat,
                None,
                MixBlendMode::Normal,
                Vec::new(),
            );

            // red rectangle if we don't have a background color
            builder.push_rect(&info, rect.background_color.unwrap_or(ColorU { r: 255, g: 0, b: 0, a: 255 }).into());

            if let Some(ref pre_shadow) = rect.box_shadow {
                // The pre_shadow is missing the BorderRadius & LayoutRect
                let border_radius = rect.border_radius.unwrap_or(BorderRadius::zero());
                builder.push_box_shadow(&info, bounds, pre_shadow.offset, pre_shadow.color,
                                         pre_shadow.blur_radius, pre_shadow.spread_radius,
                                         border_radius, pre_shadow.clip_mode);
            }

            if let Some(ref background) = rect.background {
                match *background {
                    ParsedGradient::RadialGradient(ref _gradient) => {

                    },
                    ParsedGradient::LinearGradient(ref gradient) => {
                        let mut stops: Vec<GradientStop> = gradient.stops.iter().map(|gradient_pre|
                            GradientStop {
                                offset: gradient_pre.offset.unwrap(),
                                color: gradient_pre.color,
                            }).collect();
                        let (begin_pt, end_pt) = gradient.direction.to_points(&bounds);
                        let gradient = builder.create_gradient(begin_pt, end_pt, stops, gradient.extend_mode);
                        builder.push_gradient(&info, gradient, bounds.size, LayoutSize::zero());
                    }
                }
            }

            if let Some((border_widths, mut border_details)) = rect.border {
                if let Some(border_radius) = rect.border_radius {
                    if let BorderDetails::Normal(ref mut n) = border_details {
                        n.radius = border_radius;
                    }
                }
                builder.push_border(&info, border_widths, border_details);
            }

            builder.pop_stacking_context();
        }

        // println!("---------- finished creating builder ------------");
        builder
    }
}

/// Populate the constraint list
fn parse_css(rect: &mut DisplayRectangle)
{
    let constraint_list = &rect.styled_node.css_constraints.list;

    macro_rules! parse {
        ($id:ident, $key:expr, $replace:expr, $func:tt, $constraint_list:ident) => (
            if let Some($id) = $constraint_list.get($key) {
                match $func($id) {
                    Ok(r) => { $replace = Some(r); },
                    Err(e) => { println!("ERROR - invalid {:?}: {:?}", e, $key); }
                }
            }
        )
    }

    parse!(radius, "border-radius", rect.border_radius, parse_css_border_radius, constraint_list);
    parse!(background_color, "background-color", rect.background_color, parse_css_color, constraint_list);
    parse!(border, "border", rect.border, parse_css_border, constraint_list);
    parse!(background, "background", rect.background, parse_css_background, constraint_list);

    if let Some(box_shadow) = constraint_list.get("box-shadow") {
        match parse_css_box_shadow(box_shadow) {
            Ok(r) => { rect.box_shadow = r; },
            Err(e) => { println!("ERROR - invalid {:?}: {:?}", e, "box-shadow"); }
        }
    }
}

fn create_layout_constraints<T>(rect: &DisplayRect, 
                                arena: &Arena<NodeData<T>>, 
                                styled_node: &StyledNode, 
                                window_dimensions: LayoutSize, 
                                target_constraints: &mut Vec<CssConstraint>)
where T: LayoutScreen
{
    use constraints::{SizeConstraint};
    let constraint_list = &styled_node.css_constraints.list;
    
    macro_rules! parse_css_size {
        ($id:ident, $key:expr, $func:tt, $css_constraints:ident, $constraint_list:ident, $wrapper:path) => (
            if let Some($id) = $constraint_list.get($key) {
                match $func($id) {
                    Ok(w) => { $css_constraints.push(CssConstraint::Size($wrapper(w.to_pixels()))); },
                    Err(e) => { println!("ERROR - invalid {:?}: {:?}", e, $key); }
                }
            }
        )
    }

    // simple parsing rules
    parse_css_size!(width, "width", parse_pixel_value, target_constraints, constraint_list, SizeConstraint::Width);
    parse_css_size!(height, "height", parse_pixel_value, target_constraints, constraint_list, SizeConstraint::Height);
    parse_css_size!(min_height, "min-height", parse_pixel_value, target_constraints, constraint_list, SizeConstraint::MinHeight);
    parse_css_size!(min_width, "min-width", parse_pixel_value, target_constraints, constraint_list, SizeConstraint::MinWidth);

    // TODO: complex parsing rules
}

fn css_constraints_to_cassowary_constraints(rect: &DisplayRect, css: &Vec<CssConstraint>)
-> Vec<Constraint>
{
    use self::CssConstraint::*;

    css.iter().flat_map(|constraint|
        match *constraint {
            Size(ref c) => { c.build(&rect, 100.0) }
            Padding(ref p) => { p.build(&rect, 50.0, 10.0) }
        }
    ).collect()
}