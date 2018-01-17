use kuchiki::NodeRef;
use ui_state::UiState;
use ui_description::UiDescription;
use css::Css;

pub trait LayoutScreen {
	/// Does only DOM up
	fn update_dom(&self, old_ui_state: Option<&UiState>) -> NodeRef;
	/// Applies the CSS styles to the nodes calculated from the `layout_screen`
	/// function and calculates the final display list that is submitted to the
	/// renderer.
	fn style_dom(nodes: &NodeRef, css: &Css) -> UiDescription {
		UiDescription {

		}
	}
}