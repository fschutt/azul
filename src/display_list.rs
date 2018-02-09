#![allow(unused_variables)]
#![allow(unused_macros)]

use webrender::api::*;
use traits::LayoutScreen;
use constraints::{DisplayRect, CssConstraint};
use ui_description::{UiDescription, StyledNode};
use cassowary::{Constraint, Solver, Variable};
use window::{WindowDimensions, UiSolver};
use id_tree::{Arena, NodeId};
use css_parser::*;
use dom::NodeData;
use css::Css;
use std::collections::BTreeMap;
use FastHashMap;

pub(crate) struct DisplayList<'a, T: LayoutScreen + 'a> {
    pub(crate) ui_descr: &'a UiDescription<T>,
    pub(crate) rectangles: BTreeMap<NodeId, DisplayRectangle<'a>>
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RectStyle {
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

#[derive(Debug)]
pub(crate) struct DisplayRectangle<'a> {
    /// `Some(id)` if this rectangle has a callback attached to it 
    /// Note: this is not the same as the `NodeId`! 
    /// These two are completely seperate numbers!
    pub tag: Option<u64>,
    /// The original styled node
    pub(crate) styled_node: &'a StyledNode,
    /// The style of the node
    pub(crate) style: RectStyle,
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
            style: RectStyle {
                background_color: None,
                box_shadow: None,
                background: None,
                border: None,
                border_radius: None,
            }
        }
    }
}

impl<'a, T: LayoutScreen> DisplayList<'a, T> {

    /// NOTE: This function assumes that the UiDescription has an initialized arena
    /// 
    /// This only looks at the user-facing styles of the `UiDescription`, not the actual
    /// layout. The layout is done only in the `into_display_list_builder` step.
    pub fn new_from_ui_description(ui_description: &'a UiDescription<T>) -> Self {

        let arena = &ui_description.ui_descr_arena;

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

    pub fn into_display_list_builder(&self, pipeline_id: PipelineId, ui_solver: &mut UiSolver<T>, css: &mut Css, mut has_window_size_changed: bool)
    -> DisplayListBuilder
    {
        let mut builder = DisplayListBuilder::new(pipeline_id, ui_solver.window_dimensions.layout_size);
        
        if let Some(root) = self.ui_descr.ui_descr_root {
            let changeset = ui_solver.dom_tree_cache.update(root, &*(self.ui_descr.ui_descr_arena.borrow()));
            println!("changeset: {:?}", changeset);
            ui_solver.edit_variable_cache.initialize_new_rectangles(&mut ui_solver.solver, &changeset);
            ui_solver.edit_variable_cache.remove_unused_variables(&mut ui_solver.solver);
        }

        println!("number of edit variables: {:?}", ui_solver.edit_variable_cache.map.len());

        if css.needs_relayout {
            
            // constraints were added or removed during the last frame
/*
            for rect in self.rectangles.values() {
                let mut layout_contraints = Vec::<CssConstraint>::new();
                let arena = &*self.ui_descr.arena.borrow();
                create_layout_constraints(&rect, arena, ui_solver);
                // let cassowary_constraints = css_constraints_to_cassowary_constraints(&rect.rect, &layout_contraints);
                // ui_solver.solver.add_constraints(&cassowary_constraints).unwrap();
            }
*/
            // if we push or pop constraints that means we also need to re-layout the window
            has_window_size_changed = true;
            css.needs_relayout = false;
        }

        // recalculate the actual layout
        if has_window_size_changed {
            /*
                for change in solver.fetch_changes() {
                    println!("change: - {:?}", change);
                }
            */
        }

        for (rect_idx, rect) in self.rectangles.iter() {

            let bounds1 = LayoutRect::new(
                LayoutPoint::new(0.0, 0.0),
                LayoutSize::new(200.0, 200.0),
            );
            let bounds2 = LayoutRect::new(
                LayoutPoint::new(0.0, 0.0),
                LayoutSize::new(3.0, 3.0),
            );

            // debugging - there are currently two rectangles on the screen
            // if the rectangle doesn't have a background color, choose the first bound
            //
            // this means, since the DOM in the debug example has two rectangles, we should
            // have two touching rectangles
            let mut bounds = if rect.style.background_color.is_some() { 
                bounds1 
            } else { 
                bounds2 
            };

            // bug - for some reason, the origin gets scaled by 2.0, 
            // even if the HiDpi factor is set to 1.0
            // println!("bounds: {:?}", bounds);
            // println!("hidpi_factor: {:?}", hidpi_factor);
            // println!("window size: {:?}", layout_size);

            // this is a workaround, this seems to be a bug in webrender
            bounds.origin.x /= 2.0;
            bounds.origin.y /= 2.0;

            let clip = if let Some(border_radius) = rect.style.border_radius {
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
            builder.push_rect(&info, rect.style.background_color.unwrap_or(ColorU { r: 255, g: 0, b: 0, a: 255 }).into());

            if let Some(ref pre_shadow) = rect.style.box_shadow {
                // The pre_shadow is missing the BorderRadius & LayoutRect
                let border_radius = rect.style.border_radius.unwrap_or(BorderRadius::zero());
                builder.push_box_shadow(&info, bounds, pre_shadow.offset, pre_shadow.color,
                                         pre_shadow.blur_radius, pre_shadow.spread_radius,
                                         border_radius, pre_shadow.clip_mode);
            }

            if let Some(ref background) = rect.style.background {
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

            if let Some((border_widths, mut border_details)) = rect.style.border {
                if let Some(border_radius) = rect.style.border_radius {
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

    parse!(radius, "border-radius", rect.style.border_radius, parse_css_border_radius, constraint_list);
    parse!(background_color, "background-color", rect.style.background_color, parse_css_color, constraint_list);
    parse!(border, "border", rect.style.border, parse_css_border, constraint_list);
    parse!(background, "background", rect.style.background, parse_css_background, constraint_list);

    if let Some(box_shadow) = constraint_list.get("box-shadow") {
        match parse_css_box_shadow(box_shadow) {
            Ok(r) => { rect.style.box_shadow = r; },
            Err(e) => { println!("ERROR - invalid {:?}: {:?}", e, "box-shadow"); }
        }
    }
}

fn create_layout_constraints<T>(rect: &DisplayRectangle, 
                                arena: &Arena<NodeData<T>>, 
                                ui_solver: &mut UiSolver<T>)
where T: LayoutScreen
{
    // todo: put these to use!
    let window_dimensions = &ui_solver.window_dimensions;
    let solver = &mut ui_solver.solver;
    let previous_layout = &mut ui_solver.solved_layout;

    use cassowary::strength::*;
    use constraints::{SizeConstraint, Strength};
    let constraint_list = &rect.styled_node.css_constraints.list;
    
    // get all the relevant keys we need to look at
    let kv_width = constraint_list.get("width");
    let kv_height = constraint_list.get("height");
    let kv_min_width = constraint_list.get("min-width");
    let kv_min_height = constraint_list.get("min-height");
    

    let kv_direction = constraint_list.get("direction");
    let kv_wrap = constraint_list.get("wrap");
    let kv_justify_content = constraint_list.get("justify-content");
    let kv_align_items = constraint_list.get("align-items");
    let kv_align_content = constraint_list.get("align-content");
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