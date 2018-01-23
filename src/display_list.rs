use webrender::api::*;
use constraints::{DisplayRect, CssConstraint};
use ui_description::UiDescription;
use cassowary::{Constraint, Solver};

use css_parser::*;

pub(crate) struct DisplayList {
	pub(crate) rectangles: Vec<DisplayRectangle>
}

pub(crate) struct DisplayRectangle {
	/// The actual rectangle
	pub(crate) rect: DisplayRect,
	/// The constraints to be solved
	pub(crate) constraints: Vec<CssConstraint>,
	/// Shadow color
	pub(crate) shadow: Option<Shadow>,
	/// Gradient (location) + stops
	pub(crate) gradient: Option<(Gradient, Vec<GradientStop>)>,
	/// Opacity of this rectangle
	pub(crate) opacity: Option<f32>,
	/// Thickness + line style
	pub(crate) outline: Option<(f32, LineStyle)>,
	/// border radius
	pub(crate) border_radius: Option<BorderRadius>,
}

impl DisplayRectangle {
	/// Returns an uninitialized rectangle
	#[inline]
	pub(crate) fn new() -> Self {
		Self {
			rect: DisplayRect::new(),
			constraints: Vec::new(),
			shadow: None,
			gradient: None,
			opacity: None,
			outline: None,
			border_radius: None,
		}
	}
}

impl DisplayList {

	pub fn new_from_ui_description(ui_description: &UiDescription) -> Self {

		use constraints::{SizeConstraint, PaddingConstraint};

		let rects = ui_description.styled_nodes.iter().filter_map(|node| {

			// currently only style divs
			if node.node.as_element().is_none() {
				return None;
			}

			let mut rect = DisplayRectangle::new();

			let constraint_list = &node.css_constraints.list;
			let mut css_constraints = Vec::<CssConstraint>::new();

			if let Some(radius) = constraint_list.get("border-radius") {
				match parse_border_radius(radius) {
					Ok(r) => { rect.border_radius = Some(r); },
					Err(e) => { println!("ERROR - invalid border-radius {:?}", e); }
				}
			}

			if let Some(with) = constraint_list.get("width") {
				css_constraints.push(CssConstraint::Size(SizeConstraint::Width(100.0)));
			}

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
			    LayoutPoint::new(0.0, 0.0),
			    LayoutSize::new(200.0, 200.0),
			);

			println!("border radius: {:?}", rect.border_radius);

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
				tag: None, // todo: for hit testing !!!
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

			builder.push_rect(&info, ColorF::new(0.0, 1.0, 0.0, 1.0));
			builder.pop_stacking_context();
		}


		builder
	}
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