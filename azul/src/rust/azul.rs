//! Auto-generated public Rust API for the Azul GUI toolkit version 0.1.0
//!
//! # About
//! 
//! Azul is a free, functional, immediate-mode GUI framework for rapid development
//! of desktop applications written in Rust, supported by the Mozilla WebRender
//! rendering engine, using a flexbox-based CSS / DOM model for layout and styling.
//! 
//! # Concept
//! 
//! Azul is largely based on the principle of immediate-mode GUI frameworks, which
//! is that the entire UI (in Azuls case the DOM) is reconstructed and re-rendered
//! on every frame (instead of having functions that mutate the UI state like
//! `button.setText()`). This method of constructing UIs has a performance overhead
//! over methods that retain the UI, therefore Azul only calls the [`Layout::layout()`]
//! function when its absolutely necessary - inside of a callback, you can return
//! whether it is necessary to redraw the screen or not (by returning
//! [`Redraw`] or [`DontRedraw`], respectively).
//! 
//! In difference to other immediate-mode frameworks, Azul does not immediately
//! draw to the screen, but rather "draws" to a [`Dom`]. This has several advantages,
//! such as making it possible to layout code at runtime, [loading a `Dom` from
//! an XML file], recognizing state changes by [diffing two frames], as well as being
//! able to reparent DOMs into almost any configuration to make components reusable
//! independent of the context they are in.
//! 
//! # Hello World
//! 
//! ```no_run
//! extern crate azul;
//! 
//! use azul::prelude::*;
//! 
//! struct MyDataModel { }
//! 
//! impl Layout for MyDataModel {
//!     fn layout(&Ref<Self>, _: LayoutInfo) -> Dom {
//!         Dom::label("Hello World")
//!     }
//! }
//! 
//! fn main() {
//!     let app = App::new(MyDataModel { }, AppConfig::default()).unwrap();
//!     app.run(WindowCreateOptions::new(css::native()));
//! }
//! ```
//! 
//! # Development lifecycle
//! 
//! A huge problem when working with GUI applications in Rust is managing the
//! compile time. Having to recompile your entire code when you just want to
//! shift an element a pixel to the right is not a good developer experience.
//! Azul has three main methods of combating compile time:
//! 
//! - The [XML] system, which allows you to load DOMs at runtime [from a file]
//! - The [CSS] system, which allows you to [load and parse stylesheets]
//! 
//! You can create your own hot-reload handler (for example to automatically
//! compile SASS / LESS or other files when reloading) by implementing the [`HotReloadHandler`]
//! trait or use the [default HotReloader]
//! 
//! Due to Azuls stateless rendering architecture, hot-reloading also preserves
//! the current application state. Once you are done layouting your applications
//! UI, you can [transpile the XML code to valid Rust source code] using [azulc],
//! the Azul-XML-to-Rust compiler.
//! 
//! Please note that the compiler isn't perfect - the XML system is very limited,
//! and parsing XML has a certain performance overhead, since it's done on every frame.
//! That is fine for debug builds, but the XML system should not be used in release mode.
//! 
//! When you are done with designing the callbacks of your widget, you may want to
//! package the widget up to autmatically react to certain events without having the
//! user of your widget write any code to hook up the callbacks - for this purpose,
//! 
//! # Application state management
//! 
//! Azul features a [two way data binding] system using [`Ref<MyWidget>`] which you can
//! upcast to a [`RefAny`] (a type-erased `Rc<RefCell<Any>>`). This `RefAny` then [gets
//! stored in the `Dom`], and when invoking a [`DefaultCallback`] you can access the `RefAny`
//! through the [`DefaultCallbackInfo.state`] and [downcast it] to a `RefCell<MyWidget>`.
//! 
//! Events bubble from inner to outer elements. You can either override the
//! default behaviour of widgets - for example, you can set the `ontextinput` callback
//! function to a custom callback function. This is one way of implementing "extends / override"
//! in Rust (which doesn't feature such concepts). You can also simply [prevent the default
//! callback from being called] or [prevent an event from bubbling to its parents].
//! 
//! # Custom drawing and embedding external applications
//! 
//! Azul is mostly concerned with rendering text, images and rectangular boxes (divs).
//! There is no audio / video playback support, but there are other crates for this purpose,
//! for example [rodio]. Azul is not a complete application development framework like Qt,
//! it only focuses on the GUI aspect.
//! 
//! Other content can be drawn by drawing to an OpenGL texture (using a
//! [`GlCallback`]) and [hand it over to Azul]. This is how components like a
//! video player or other OpenGL-based visualizations can be injected into the UI,
//! without being part of the core library.
//! 
//! You can draw to an OpenGL texture and  in order to display it
//! in the UI - the texture doesn't have to come from Azul itself, you can inject
//! it from an external application.
//! 
//! Azul can also render to a headless target - for this purpose, the `text-layout`,
//! the `layout` and the `core` components are separate crates, separate from the `azul`
//! crate, which depends on webrender and is meant as the desktop deployment target. By default,
//! `azul-layout` depends on a static build of HarfBuzz and FreeType for layouting glyphs,
//! but you can use the system-native builds by using `features = ["native-freetype"]`.
//! 
//! The output of a headless rendering pass is a [`CachedDisplayList`], which contains all
//! information about how and where to render items - without actually rendering them anywhere.
//! This way you can render to a custom rendering target (such as a non-OpenGL renderer, a software renderer,
//! PDF / SVG or the web). However, you will have to handle input handling and hit-testing
//! yourself and provide a custom event loop.
//! 
//! # File picker, popup dialogs, multi-window handling
//! 
//! Azul implements the APIs to open native file picker dialogs, see the [dialogs] module.
//! You can create one or more windows in callbacks by calling [CallbackInfo::create_window()].
//! Each window carries a unique ID and a unique type, which can be user-defined. This type
//! gets then passed to the [`Layout::layout()`] function, and you can use said window type.
//! 
//! For small, frameless popup windows, such as context menus and tearoff-windows, azul also
//! features `display: window-*;` (ex. `display: window-context-menu`) - which behaves exactly
//! like `display: absolute`, except that it displays the content of the div it is applied to in
//! a separate, frameless window.
//! 
//! # Rendering of large / infinite data sets
//! 
//! For rendering very large or infinite data sources such as long lists, tables, etc., Azul
//! provides [`IFrameCallbacks`] so that you don't have to render what you don't see on the screen.
//! For example, imagine you want to render a long list with multiple thousands of items. It isn't
//! necessary to show all items on screen at the same time, instead you only want to display a "window"
//! and load / unload items as the user scrolls.
//! 
//! `IFrameCallbacks` do exactly that: The callbacks are stored in the `Dom` and during the layout step,
//! the callback is invoked with the size (of its container) and scroll offset. So, if you know that every
//! item is 20px high, and the `IFrameCallback` gets invoked with a height of 100px, then you only need
//! to render 5 divs into the Dom instead of thousands.
//! 
//! # Limitations
//! 
//! There are a few limitations that should be noted:
//! 
//! - There are no scrollbars yet.
//! - There is no support for CSS animations of any kind yet.
//! - There is no text selection yet.
//! - Changing dynamic variables will trigger an entire UI relayout and restyling
//! 
//! # Tutorials
//! 
//! Explaining all concepts and examples is too much to be included in
//! this API reference. Please refer to the [wiki](https://github.com/maps4print/azul/wiki)
//! or use the links below to learn about how to use Azul.
//! 
//! - [Getting Started](https://github.com/maps4print/azul/wiki/Getting-Started)
//! - [A simple counter](https://github.com/maps4print/azul/wiki/A-simple-counter)
//! - [Styling your app with CSS](https://github.com/maps4print/azul/wiki/Styling-your-application-with-CSS)
//! - [SVG drawing](https://github.com/maps4print/azul/wiki/SVG-drawing)
//! - [OpenGL drawing](https://github.com/maps4print/azul/wiki/OpenGL-drawing)
//! - [Timers, timers, tasks and async IO](https://github.com/maps4print/azul/wiki/Timers,-timers,-tasks-and-async-IO)
//! - [Two-way data binding](https://github.com/maps4print/azul/wiki/Two-way-data-binding)
//! - [Unit testing](https://github.com/maps4print/azul/wiki/Unit-testing)
//! 
//! [`Layout::layout()`]: ../azul/traits/trait.Layout.html
//! [`Dom`]: ../azul/dom/struct.Dom.html
//! [diffing two frames]: ../azul/diff/struct.DomDiff.html#method.new
//! [widgets]: ../azul/widgets/index.html
//! [loading a `Dom` from an XML file]: ../azul/xml/struct.DomXml.html#method.from_file
//! [XML]: ../azul/xml/index.html
//! [`Redraw`]: ../azul/callbacks/constant.Redraw.html
//! [`DontRedraw`]: ../azul/callbacks/constant.DontRedraw.html
//! [`GlCallback`]: ../azul/callbacks/struct.GlCallback.html
//! [creating an `IFrameCallback`]: ../azul/dom/struct.Dom.html#method.iframe
//! [from a file]: ../azul/dom/struct.Dom.html#method.from_file
//! [CSS]: ../azul/css/index.html
//! [load and parse stylesheets]: ../azul/css/fn.from_str.html
//! [transpile the XML code to valid Rust source code]: https://github.com/maps4print/azul/wiki/XML-to-Rust-compilation
//! [azulc]: https://crates.io/crates/azulc
//! [two way data binding]: https://github.com/maps4print/azul/wiki/Two-way-data-binding
//! [`Ref<MyWidget>`]: ../azul/callbacks/struct.RefAny.html#method.downcast
//! [`RefAny`]: ../azul/callbacks/struct.Ref.html#method.upcast
//! [`HotReloadHandler`]: ../azul/css/trait.HotReloadHandler.html
//! [`CachedDisplayList`]: ../azul_core/display_list/struct.CachedDisplayList.html
//! [hand it over to Azul]: ../azul/target/doc/azul/gl/struct.Texture.html

// Copyright 2017 Maps4Print Einzelunternehmung
// 
// Permission is hereby granted, free of charge, to any person obtaining
// a copy of this software and associated documentation files (the
// "Software"), to deal in the Software without restriction, including
// without limitation the rights to use, copy, modify, merge, publish,
// distribute, sublicense, and/or sell copies of the Software, and to
// permit persons to whom the Software is furnished to do so, subject to
// the following conditions:
// 
// The above copyright notice and this permission notice shall be
// included in all copies or substantial portions of the Software.
// 
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
// EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
// MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.
// IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
// CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT,
// TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE
// SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.


extern crate azul_dll;

pub mod callbacks {

    use azul_dll::*;
    use crate::dom::Dom;
    /// Callback fn that returns the layout
    pub type LayoutCallback = fn(RefAny, LayoutInfo) -> Dom;


    /// `LayoutInfo` struct
    pub struct LayoutInfo { pub(crate) ptr: AzLayoutInfoPtr }

    impl Drop for LayoutInfo { fn drop(&mut self) { az_layout_info_delete(self.ptr); } }


    /// `RefAny` struct
    pub struct RefAny { pub(crate) ptr: AzRefAnyPtr }

    impl Drop for RefAny { fn drop(&mut self) { az_ref_any_delete(self.ptr); } }
}

pub mod app {

    use azul_dll::*;
    use crate::callbacks::{RefAny, LayoutCallback};
    use crate::window::WindowCreateOptions;


    /// `AppConfig` struct
    pub struct AppConfig { pub(crate) ptr: AzAppConfigPtr }

    impl AppConfig {
        /// Creates a new `AppConfig` instance.
        pub fn new() -> Self { Self { ptr: az_app_config_new() } }
    }

    impl Drop for AppConfig { fn drop(&mut self) { az_app_config_delete(self.ptr); } }


    /// `App` struct
    pub struct App { pub(crate) ptr: AzAppPtr }

    impl App {
        /// Creates a new App instance.
        pub fn new(config: AppConfig, data: RefAny, callback: LayoutCallback) -> Self { Self { ptr: az_app_new(config.ptr, data.ptr, callback) } }
        /// Calls the `App::run` function.
        pub fn run(&self, window: WindowCreateOptions)  { az_app_run(self.ptr, window.ptr) }
    }

    impl Drop for App { fn drop(&mut self) { az_app_delete(self.ptr); } }
}

pub mod window {

    use azul_dll::*;
    use crate::css::Css;


    /// `WindowCreateOptions` struct
    pub struct WindowCreateOptions { pub(crate) ptr: AzWindowCreateOptionsPtr }

    impl WindowCreateOptions {
        /// Creates a new `WindowCreateOptions` instance.
        pub fn new(css: Css) -> Self { Self { ptr: az_window_create_options_new(css.ptr) } }
    }

    impl Drop for WindowCreateOptions { fn drop(&mut self) { az_window_create_options_delete(self.ptr); } }
}

pub mod css {

    use azul_dll::*;


    /// `Css` struct
    pub struct Css { pub(crate) ptr: AzCssPtr }

    impl Css {
        /// Creates a new `Css` instance.
        pub fn native() -> Self { Self { ptr: az_css_native() } }
    }

    impl Drop for Css { fn drop(&mut self) { az_css_delete(self.ptr); } }
}

pub mod dom {

    use azul_dll::*;


    /// `Dom` struct
    pub struct Dom { pub(crate) ptr: AzDomPtr }

    impl Dom {
        /// Creates a new `Dom` instance.
        pub fn div() -> Self { Self { ptr: az_dom_div() } }
    }

    impl Drop for Dom { fn drop(&mut self) { az_dom_delete(self.ptr); } }
}

