use kuchiki::NodeRef;
use ui_state::UiState;
use ui_description::UiDescription;
use css::Css;
use window::WindowId;

pub trait LayoutScreen {
	/// Updates the DOM, must be provided by the final application.
	///
	/// On each frame, a completely new DOM tree is generated. The final
	/// application can cache the DOM tree, but this isn't in the scope of `azul`.
	///
	/// The `style_dom` looks through the given DOM rules, applies the style and
	/// recalculates the layout. This is done on each frame (except there are shortcuts
	/// when the DOM doesn't have to be recalculated).
	fn update_dom(&self, old_ui_state: Option<&UiState>) -> NodeRef;
	/// Provide access to the Css style for the application
	fn get_css<'a>(&'a self, window_id: &WindowId) -> &'a Css;
	/// Applies the CSS styles to the nodes calculated from the `layout_screen`
	/// function and calculates the final display list that is submitted to the
	/// renderer.
	fn style_dom(nodes: &NodeRef, css: &Css) -> UiDescription {

		UiDescription {

		}
	}
}