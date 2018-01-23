use webrender::api::*;
use constraints::DisplayRect;
use ui_description::UiDescription;

pub(crate) struct DisplayList {
	pub(crate) rectangles: Vec<DisplayRectangle>
}

pub(crate) struct DisplayRectangle {
	/// The actual rectangle
	pub(crate) rect: DisplayRect,
	/// Shadow color
	pub(crate) shadow: Option<Shadow>,
	/// Gradient (location) + stops
	pub(crate) gradient: Option<(Gradient, Vec<GradientStop>)>,
	/// Opacity of this rectangle
	pub(crate) opacity: Option<f32>,
	/// Thickness + line style
	pub(crate) outline: Option<(BorderRadius, LineStyle)>,
}

impl DisplayRectangle {
	/// Returns an uninitialized rectangle
	pub(crate) fn new() -> Self {
		Self {
			rect: DisplayRect::new(),
			shadow: None,
			gradient: None,
			opacity: None,
			outline: None,
		}
	}
}

impl DisplayList {
	pub fn new_from_ui_description(ui_description: &UiDescription) -> Self {
		Self {
			rectangles: Vec::new(),
		}
	}

	pub fn into_display_list_builder(&self, pipeline_id: PipelineId, layout_size: LayoutSize)
	-> DisplayListBuilder
	{
		let mut builder = DisplayListBuilder::new(pipeline_id, layout_size);

		// Create a 200x200 stacking context with an animated transform property.
		let bounds = LayoutRect::new(
		    LayoutPoint::new(0.0, 0.0),
		    LayoutSize::new(200.0, 200.0),
		);

		let complex_clip = ComplexClipRegion {
		    rect: bounds,
		    radii: BorderRadius::uniform(50.0),
		    mode: ClipMode::Clip,
		};

		let info = LayoutPrimitiveInfo {
		    local_clip: LocalClip::RoundedRect(bounds, complex_clip),
		    .. LayoutPrimitiveInfo::new(bounds)
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

		// Fill it with a green rect
		builder.push_rect(&info, ColorF::new(0.0, 1.0, 0.0, 1.0));
		builder.pop_stacking_context();

		builder
	}
}