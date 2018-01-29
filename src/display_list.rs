use webrender::api::*;
use constraints::{DisplayRect, CssConstraint};
use ui_description::{UiDescription, CssConstraintList};
use cassowary::{Constraint, Solver};

use css_parser::*;

pub(crate) struct DisplayList {
    pub(crate) rectangles: Vec<DisplayRectangle>
}

pub(crate) struct DisplayRectangle {
    /// Tag used for hit-testing (for webrender)
    pub tag: Option<(u64, u16)>,
    /// The actual rectangle
    pub(crate) rect: DisplayRect,
    /// The constraints to be solved
    pub(crate) constraints: Vec<CssConstraint>,
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
}

impl DisplayRectangle {

    /// Returns an uninitialized rectangle
    #[inline]
    pub(crate) fn new() -> Self {
        Self {
            tag: None,
            rect: DisplayRect::new(),
            constraints: Vec::new(),
            background_color: None,
            box_shadow: None,
            background: None,
            border: None,
            border_radius: None,
        }
    }
}

impl DisplayList {

    pub fn new_from_ui_description(ui_description: &UiDescription) -> Self {
        use dom::HTML_NODE_ID;

        let rects = ui_description.styled_nodes.iter().filter_map(|node| {

            // TODO: currently only styles divs
            let element_data = node.node.as_element();
            if element_data.is_none() {
                return None;
            }

            let element_data = element_data.unwrap();
            let mut rect = DisplayRectangle::new();
            let mut id = None;

            if let Some(id_str) = element_data.attributes.borrow().map.get(&HTML_NODE_ID) {
                use std::mem::transmute;
                let id_vec = id_str.as_bytes();
                assert!(id_vec.len() == 8);
                let id_arr: [u8; 8] = [id_vec[0], id_vec[1], id_vec[2], id_vec[3], id_vec[4], id_vec[5], id_vec[6], id_vec[7]];
                let node_id: u64 = unsafe { transmute(id_arr) };
                id = Some((node_id, 0));
            }

            rect.tag = id;

            let mut css_constraints = Vec::<CssConstraint>::new();
            parse_css(&node.css_constraints, &mut rect, &mut css_constraints);
            rect.constraints = css_constraints;
            Some(rect)

        }).collect();

        Self {
            rectangles: rects,
        }
    }

    pub fn into_display_list_builder(&self, pipeline_id: PipelineId, layout_size: LayoutSize, solver: &mut Solver)
    -> DisplayListBuilder
    {
        let mut builder = DisplayListBuilder::new(pipeline_id, layout_size);

        for rect in &self.rectangles {

            // TODO: get these constraints to work properly
            let cassowary_constraints = css_constraints_to_cassowary_constraints(&rect.rect, &rect.constraints);
            solver.add_constraints(&cassowary_constraints).unwrap();

            for change in solver.fetch_changes() {
                println!("change: - {:?}", change);
            }

            let bounds = LayoutRect::new(
                LayoutPoint::new(50.0, 50.0),
                LayoutSize::new(200.0, 200.0),
            );

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
                tag: rect.tag,
                local_clip: clip,
            };

            let opacity = 34.0;
            let opacity_key = PropertyBindingKey::new(43); // arbitrary magic number
            let property_key = PropertyBindingKey::new(42); // arbitrary magic number

            let filters = vec![
                FilterOp::Opacity(PropertyBinding::Binding(opacity_key), opacity),
            ];

            builder.push_stacking_context(
                &info,
                ScrollPolicy::Scrollable,
                Some(PropertyBinding::Binding(property_key)),
                TransformStyle::Flat,
                None,
                MixBlendMode::Normal,
                filters,
            );

            builder.push_rect(&info, rect.background_color.unwrap_or(ColorU { r: 0, g: 0, b: 0, a: 0 }).into());

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
                        builder.push_gradient(&info, gradient, LayoutSize::new(200.0, 200.0), LayoutSize::zero());
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


        builder
    }
}

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

/*
macro_rules! parse_css_padding {
    ($id:ident, $key:expr, $func:tt, $css_constraints:ident, $constraint_list:ident, $wrapper:path, $variable:ident) => (
        if let Some($id) = $constraint_list.get($key) {
            match $func($id) {
                Ok(w) => { $css_constraints.push(CssConstraint::Padding($wrapper($variable))); },
                Err(e) => { println!("ERROR - invalid {:?}: {:?}", e, $key); }
            }
        }
    )
}
*/

/// Populate the constraint list
fn parse_css(constraint_list: &CssConstraintList, rect: &mut DisplayRectangle, css_constraints: &mut Vec<CssConstraint>)
{
    use constraints::{SizeConstraint, PaddingConstraint};

    let constraint_list = &constraint_list.list;

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

    parse_css_size!(width, "width", parse_pixel_value, css_constraints, constraint_list, SizeConstraint::Width);
    parse_css_size!(height, "height", parse_pixel_value, css_constraints, constraint_list, SizeConstraint::Height);
    parse_css_size!(min_height, "min-height", parse_pixel_value, css_constraints, constraint_list, SizeConstraint::MinHeight);
    parse_css_size!(min_width, "min-width", parse_pixel_value, css_constraints, constraint_list, SizeConstraint::MinWidth);
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