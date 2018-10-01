use std::sync::{Arc, Mutex};
use {
    dom::{Dom},
    ui_description::{UiDescription},
    css::{Css, ParsedCss, match_dom_css_selectors},
    css_parser::{CssParsingError, ParsedCssProperty},
};
#[cfg(not(test))]
use window::WindowInfo;

/// The core trait that has to be implemented for the app model to provide a
/// Model -> View serialization.
pub trait Layout {
    /// Updates the DOM, must be provided by the final application.
    ///
    /// On each frame, a completely new DOM tree is generated. The final
    /// application can cache the DOM tree, but this isn't in the scope of `azul`.
    ///
    /// The `style_dom` looks through the given DOM rules, applies the style and
    /// recalculates the layout. This is done on each frame (except there are shortcuts
    /// when the DOM doesn't have to be recalculated).
    #[cfg(not(test))]
    fn layout(&self, window_id: WindowInfo<Self>) -> Dom<Self> where Self: Sized;
    #[cfg(test)]
    fn layout(&self) -> Dom<Self> where Self: Sized;
    /// Applies the CSS styles to the nodes calculated from the `layout_screen`
    /// function and calculates the final display list that is submitted to the
    /// renderer.
    fn style_dom(dom: &Dom<Self>, css: &Css) -> UiDescription<Self> where Self: Sized {
        match_dom_css_selectors(dom.root, &dom.arena, &ParsedCss::from_css(css), css, 0)
    }
}

/// Convenience trait for the `css.set_dynamic_property()` function.
///
/// This trait exists because `TryFrom` / `TryInto` are not yet stabilized.
/// This is the same as `Into<ParsedCssProperty>`, but with an additional error
/// case (since the parsing of the CSS value could potentially fail)
///
/// Using this trait you can write: `css.set_dynamic_property("var", ("width", "500px"))`
/// because `IntoParsedCssProperty` is implemented for `(&str, &str)`.
///
/// Note that the properties have to be re-parsed on every frame (which incurs a
/// small per-frame performance hit), however `("width", "500px")` is easier to
/// read than `ParsedCssProperty::Width(PixelValue::Pixels(500))`
pub trait IntoParsedCssProperty<'a> {
    fn into_parsed_css_property(self) -> Result<ParsedCssProperty, CssParsingError<'a>>;
}

/// Convenience trait that allows the `app_state.modify()` - only implemented for
/// `Arc<Mutex<T>` - shortly locks the app state mutex, modifies it and unlocks
/// it again.
///
/// Note: Usually when doing asynchronous programming you don't want to block the main
/// UI. While Rust executes the `app_state.modify()` closure, your `AppState` gets
/// locked, meaning that no layout can happen and no other thread or callback can write
/// to the apps data. In order to make your app performant, don't do heavy computations
/// inside the closure, only use it to write or copy data in and out of the application
/// state.
pub trait Modify<T> {
    /// Modifies the app state and then returns if the modification was successful
    /// Takes a FnMut that modifies the state
    fn modify<F>(&self, closure: F) -> bool where F: FnOnce(&mut T);
}

impl<T> Modify<T> for Arc<Mutex<T>> {
    fn modify<F>(&self, closure: F) -> bool where F: FnOnce(&mut T) {
        match self.lock().as_mut() {
            Ok(lock) => { closure(&mut *lock); true },
            Err(_) => false,
        }
    }
}

impl<'a> IntoParsedCssProperty<'a> for ParsedCssProperty {
    fn into_parsed_css_property(self) -> Result<ParsedCssProperty, CssParsingError<'a>> {
        Ok(self.clone())
    }
}

impl<'a> IntoParsedCssProperty<'a> for (&'a str, &'a str) {
    fn into_parsed_css_property(self) -> Result<ParsedCssProperty, CssParsingError<'a>> {
        ParsedCssProperty::from_kv(self.0, self.1)
    }
}

// Empty test, for some reason codecov doesn't detect any files (and therefore
// doesn't report codecov % correctly) except if they have at least one test in
// the file. This is an empty test, which should be updated later on
#[test]
fn __codecov_test_traits_file() {

}