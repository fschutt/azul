//! Resource management types for the application.
//!
//! This module contains the core types for managing application resources:
//! - `AppConfig`: application-level configuration (logging, fonts, routes, components)
//! - `ImageRef` / `ImageRefHash`: reference-counted decoded image handles
//! - `FontKey` / `FontInstanceKey` / `ImageKey`: renderer-scoped resource keys
//! - `RendererResources`: per-window font/image registry with frame-based GC
//! - `RawImage`: CPU-side pixel data with format conversion to BGRA8
//! - `build_add_font_resource_updates` / `build_add_image_resource_updates`:
//!   diff current frame against registered resources and produce WebRender updates

#[cfg(not(feature = "std"))]
use alloc::string::ToString;
use alloc::{boxed::Box, collections::btree_map::BTreeMap, string::String, vec::Vec};
use core::{
    fmt,
    hash::{Hash, Hasher},
    sync::atomic::{AtomicU64, AtomicUsize, Ordering as AtomicOrdering},
};

use azul_css::{
    codegen::format::GetHash,
    props::basic::{
        pixel::DEFAULT_FONT_SIZE, ColorU, FloatValue, FontRef, LayoutRect, LayoutSize,
        StyleFontFamily, StyleFontFamilyVec, StyleFontSize,
    },
    system::SystemStyle,
    AzString, F32Vec, LayoutDebugMessage, OptionI32, StringVec, U16Vec, U32Vec, U8Vec,
};
use rust_fontconfig::FcFontCache;

// Re-export Core* callback types for public use
pub use crate::callbacks::{
    CoreImageCallback, CoreRenderImageCallback, CoreRenderImageCallbackType,
};
use crate::{
    callbacks::{LayoutCallback, VirtualViewCallback},
    dom::{DomId, NodeData, NodeType},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    gl::{OptionGlContextPtr, Texture},
    hit_test::DocumentId,
    id::NodeId,
    prop_cache::CssPropertyCache,
    refany::RefAny,
    styled_dom::{
        NodeHierarchyItemId, StyleFontFamiliesHash, StyleFontFamilyHash, StyledDom, StyledNodeState,
    },
    ui_solver::GlyphInstance,
    window::{AzStringPair, OptionChar, StringPairVec},
    xml::{
        ComponentDef, ComponentDefVec, ComponentId, ComponentLibrary, ComponentLibraryVec,
        ComponentSource, RegisterComponentFn, RegisterComponentLibraryFn,
    },
    FastBTreeSet, OrderedMap,
};

/// Selects which image layer of an element a node-image update applies to.
///
/// Used by `CallbackInfo::change_node_image` to distinguish between replacing an
/// element's CSS `background` image and replacing its main content image (e.g. an
/// animated GL texture re-rendered on resize).
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd, Eq, Ord, Hash)]
#[repr(C)]
pub enum UpdateImageType {
    /// The update targets the element's background.
    Background,
    /// The update targets the element's main content.
    Content,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct DpiScaleFactor {
    pub inner: FloatValue,
}

impl DpiScaleFactor {
    #[must_use] pub fn new(f: f32) -> Self {
        Self {
            inner: FloatValue::new(f),
        }
    }
}

/// Determines what happens when all application windows are closed
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
#[derive(Default)]
pub enum AppTerminationBehavior {
    /// Return control to `main()` when all windows are closed (if platform supports it).
    /// On macOS, this exits the `NSApplication` run loop and returns to `main()`.
    /// This is useful if you want to clean up resources or restart the event loop.
    ReturnToMain,
    /// Keep the application running even when all windows are closed.
    /// This is the standard macOS behavior (app stays in dock until explicitly quit).
    RunForever,
    /// Immediately terminate the process when all windows are closed.
    /// Calls `std::process::exit(0)`.
    #[default]
    EndProcess,
}


/// A named font bundled with the application (name + raw bytes).
/// The name is used to reference the font in CSS (e.g. `font-family: "MyFont"`).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct NamedFont {
    /// The font family name to use in CSS (e.g. "Roboto", "`MyCustomFont`")
    pub name: AzString,
    /// Raw font file bytes (TTF, OTF, etc.)
    pub bytes: U8Vec,
}

impl_option!(
    NamedFont,
    OptionNamedFont,
    copy = false,
    [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

impl NamedFont {
    #[must_use] pub const fn new(name: AzString, bytes: U8Vec) -> Self {
        Self { name, bytes }
    }
}

impl_vec!(NamedFont, NamedFontVec, NamedFontVecDestructor, NamedFontVecDestructorType, NamedFontVecSlice, OptionNamedFont);
impl_vec_mut!(NamedFont, NamedFontVec);
impl_vec_debug!(NamedFont, NamedFontVec);
impl_vec_partialeq!(NamedFont, NamedFontVec);
impl_vec_eq!(NamedFont, NamedFontVec);
impl_vec_partialord!(NamedFont, NamedFontVec);
impl_vec_ord!(NamedFont, NamedFontVec);
impl_vec_hash!(NamedFont, NamedFontVec);
impl_vec_clone!(NamedFont, NamedFontVec, NamedFontVecDestructor);

/// Descriptor for a font that the layout engine currently has loaded in its
/// font cache.
///
/// Returned by `CallbackInfo::get_loaded_fonts()`. The `font_hash` field is
/// the same `u64` carried by `DisplayListItem::Text` glyph runs, so a callback
/// can correlate a loaded font with the text runs that use it and then fetch
/// the raw bytes via `CallbackInfo::get_loaded_font_bytes(font_hash)` (e.g. to
/// embed every font the layout actually used into a generated PDF).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct LoadedFont {
    /// Stable hash of the parsed font, identical to the `font_hash` stored on
    /// `DisplayListItem::Text` glyph runs. Use this to look up the bytes with
    /// `CallbackInfo::get_loaded_font_bytes`.
    pub font_hash: u64,
    /// PostScript / family name from the font's `name` table, or an empty
    /// string if the font did not provide one.
    pub family_name: AzString,
    /// Total number of glyphs in the font (from the `maxp` table).
    pub num_glyphs: u32,
    /// `true` if the source font bytes are retained and can be retrieved with
    /// `CallbackInfo::get_loaded_font_bytes(font_hash)`. Fonts loaded on the
    /// production (lazy mmap) path retain their bytes; some test-only fonts do
    /// not.
    pub has_bytes: bool,
}

impl_option!(
    LoadedFont,
    OptionLoadedFont,
    copy = false,
    [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

impl LoadedFont {
    #[must_use] pub const fn new(font_hash: u64, family_name: AzString, num_glyphs: u32, has_bytes: bool) -> Self {
        Self {
            font_hash,
            family_name,
            num_glyphs,
            has_bytes,
        }
    }
}

impl_vec!(LoadedFont, LoadedFontVec, LoadedFontVecDestructor, LoadedFontVecDestructorType, LoadedFontVecSlice, OptionLoadedFont);
impl_vec_mut!(LoadedFont, LoadedFontVec);
impl_vec_debug!(LoadedFont, LoadedFontVec);
impl_vec_partialeq!(LoadedFont, LoadedFontVec);
impl_vec_eq!(LoadedFont, LoadedFontVec);
impl_vec_partialord!(LoadedFont, LoadedFontVec);
impl_vec_ord!(LoadedFont, LoadedFontVec);
impl_vec_hash!(LoadedFont, LoadedFontVec);
impl_vec_clone!(LoadedFont, LoadedFontVec, LoadedFontVecDestructor);
#[allow(variant_size_differences)] // repr(C,u8) FFI enum: boxing the large variant would change the C ABI (api.json bindings); size disparity accepted
/// Configuration for how fonts should be loaded at app startup.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C, u8)]
#[derive(Default)]
pub enum FontLoadingConfig {
    /// Load all system fonts (default behavior, can be slow on systems with many fonts)
    #[default]
    LoadAllSystemFonts,
    /// Only load fonts for specific families (faster startup).
    /// Generic families like "sans-serif" are automatically expanded to OS-specific fonts.
    LoadOnlyFamilies(StringVec),
    /// Don't load any system fonts, only use bundled fonts
    BundledFontsOnly,
}


/// Mock environment for CSS evaluation.
/// 
/// Allows overriding auto-detected system properties for testing and development.
/// Any field set to `None` will use the auto-detected value.
/// Any field set to `Some(...)` will override the auto-detected value.
/// 
/// # Example
/// ```rust
/// # use azul_core::resources::CssMockEnvironment;
/// use azul_css::dynamic_selector::{
///     OsCondition, ThemeCondition, OsVersion,
///     OptionOsCondition, OptionThemeCondition, OptionOsVersion,
/// };
/// 
/// // Mock a Linux dark theme environment on any platform
/// let mock = CssMockEnvironment {
///     os: OptionOsCondition::Some(OsCondition::Linux),
///     theme: OptionThemeCondition::Some(ThemeCondition::Dark),
///     ..Default::default()
/// };
/// 
/// // Mock Windows XP for retro testing
/// let mock = CssMockEnvironment {
///     os: OptionOsCondition::Some(OsCondition::Windows),
///     os_version: OptionOsVersion::Some(OsVersion::WIN_XP),
///     ..Default::default()
/// };
/// ```
#[derive(Debug, Clone, Default)]
#[repr(C)]
pub struct CssMockEnvironment {
    /// Override the current theme (light/dark)
    pub theme: azul_css::dynamic_selector::OptionThemeCondition,
    /// Override the current language (BCP 47 tag, e.g., "de-DE", "en-US")
    pub language: azul_css::OptionString,
    /// Override the detected OS version
    pub os_version: azul_css::dynamic_selector::OptionOsVersion,
    /// Override the detected operating system
    pub os: azul_css::dynamic_selector::OptionOsCondition,
    /// Override the Linux desktop environment (only applies when os = Linux)
    pub desktop_env: azul_css::dynamic_selector::OptionLinuxDesktopEnv,
    /// Override viewport dimensions (for @media queries)
    /// Only use for testing - normally set by window size
    pub viewport_width: azul_css::OptionF32,
    pub viewport_height: azul_css::OptionF32,
    /// Override the reduced motion preference
    pub prefers_reduced_motion: azul_css::OptionBool,
    /// Override the high contrast preference
    pub prefers_high_contrast: azul_css::OptionBool,
}

impl CssMockEnvironment {
    /// Create a mock for Linux environment
    #[must_use] pub fn linux() -> Self {
        Self {
            os: azul_css::dynamic_selector::OptionOsCondition::Some(azul_css::dynamic_selector::OsCondition::Linux),
            ..Default::default()
        }
    }
    
    /// Create a mock for Windows environment
    #[must_use] pub fn windows() -> Self {
        Self {
            os: azul_css::dynamic_selector::OptionOsCondition::Some(azul_css::dynamic_selector::OsCondition::Windows),
            ..Default::default()
        }
    }
    
    /// Create a mock for macOS environment
    #[must_use] pub fn macos() -> Self {
        Self {
            os: azul_css::dynamic_selector::OptionOsCondition::Some(azul_css::dynamic_selector::OsCondition::MacOS),
            ..Default::default()
        }
    }
    
    /// Create a mock for dark theme
    #[must_use] pub fn dark_theme() -> Self {
        Self {
            theme: azul_css::dynamic_selector::OptionThemeCondition::Some(azul_css::dynamic_selector::ThemeCondition::Dark),
            ..Default::default()
        }
    }
    
    /// Create a mock for light theme
    #[must_use] pub fn light_theme() -> Self {
        Self {
            theme: azul_css::dynamic_selector::OptionThemeCondition::Some(azul_css::dynamic_selector::ThemeCondition::Light),
            ..Default::default()
        }
    }
    
    /// Apply this mock to a `DynamicSelectorContext`
    pub fn apply_to(&self, ctx: &mut azul_css::dynamic_selector::DynamicSelectorContext) {
        if let azul_css::dynamic_selector::OptionOsCondition::Some(os) = self.os {
            ctx.os = os;
        }
        if let azul_css::dynamic_selector::OptionOsVersion::Some(os_version) = self.os_version {
            ctx.os_version = os_version;
        }
        if let azul_css::dynamic_selector::OptionLinuxDesktopEnv::Some(de) = self.desktop_env {
            ctx.desktop_env = azul_css::dynamic_selector::OptionLinuxDesktopEnv::Some(de);
        }
        if let azul_css::dynamic_selector::OptionThemeCondition::Some(ref theme) = self.theme {
            ctx.theme = theme.clone();
        }
        if let azul_css::OptionString::Some(ref lang) = self.language {
            ctx.language = lang.clone();
        }
        if let azul_css::OptionBool::Some(reduced) = self.prefers_reduced_motion {
            ctx.prefers_reduced_motion = if reduced {
                azul_css::dynamic_selector::BoolCondition::True
            } else {
                azul_css::dynamic_selector::BoolCondition::False
            };
        }
        if let azul_css::OptionBool::Some(high_contrast) = self.prefers_high_contrast {
            ctx.prefers_high_contrast = if high_contrast {
                azul_css::dynamic_selector::BoolCondition::True
            } else {
                azul_css::dynamic_selector::BoolCondition::False
            };
        }
        if let azul_css::OptionF32::Some(w) = self.viewport_width {
            ctx.viewport_width = w;
        }
        if let azul_css::OptionF32::Some(h) = self.viewport_height {
            ctx.viewport_height = h;
        }
    }
}

impl_option!(
    CssMockEnvironment,
    OptionCssMockEnvironment,
    copy = false,
    [Debug, Clone]
);

/// A route mapping a URL pattern to a layout callback.
///
/// Routes are cross-platform: on desktop, switching routes swaps the
/// active layout callback and triggers `RefreshDom`. On web, it also
/// calls `history.pushState()` for browser navigation.
///
/// # Pattern syntax
///
/// - `"/"` — exact root
/// - `"/about"` — exact path
/// - `"/user/:id"` — parameterized segment, `/user/42` yields `id = "42"`
///
/// # C API
/// ```c
/// AzAppConfig_addRoute(&config, AzString_fromConstStr("/user/:id"), layout_user);
/// ```
#[repr(C)]
pub struct Route {
    /// URL pattern (e.g. `"/"`, `"/about"`, `"/user/:id"`)
    pub pattern: AzString,
    /// Layout callback invoked when this route is active
    pub layout_callback: LayoutCallback,
}

impl Clone for Route {
    fn clone(&self) -> Self {
        Self { pattern: self.pattern.clone(), layout_callback: self.layout_callback.clone() }
    }
}
impl fmt::Debug for Route {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Route")
            .field("pattern", &self.pattern)
            .field("layout_callback", &self.layout_callback)
            .finish()
    }
}
impl PartialEq for Route { fn eq(&self, o: &Self) -> bool { self.pattern == o.pattern && self.layout_callback == o.layout_callback } }
impl Eq for Route {}
impl PartialOrd for Route { fn partial_cmp(&self, o: &Self) -> Option<core::cmp::Ordering> { Some(self.cmp(o)) } }
impl Ord for Route { fn cmp(&self, o: &Self) -> core::cmp::Ordering { self.pattern.cmp(&o.pattern).then_with(|| self.layout_callback.cmp(&o.layout_callback)) } }
impl Hash for Route { fn hash<H: Hasher>(&self, state: &mut H) { self.pattern.hash(state); self.layout_callback.hash(state); } }

impl_option!(Route, OptionRoute, copy = false, [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]);
impl_vec!(Route, RouteVec, RouteVecDestructor, RouteVecDestructorType, RouteVecSlice, OptionRoute);
impl_vec_mut!(Route, RouteVec);
impl_vec_debug!(Route, RouteVec);
impl_vec_clone!(Route, RouteVec, RouteVecDestructor);
impl_vec_partialeq!(Route, RouteVec);
impl_vec_eq!(Route, RouteVec);
impl_vec_partialord!(Route, RouteVec);
impl_vec_ord!(Route, RouteVec);
impl_vec_hash!(Route, RouteVec);

/// Result of matching a URL against a route pattern.
///
/// Stores the matched pattern and any extracted parameters.
/// Available to layout callbacks via `LayoutCallbackInfo::get_route_param()`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct RouteMatch {
    /// The matched route pattern (e.g. `"/user/:id"`)
    pub pattern: AzString,
    /// Extracted parameters (e.g. `[("id", "42")]`)
    pub params: StringPairVec,
}

impl RouteMatch {
    /// Get a route parameter by key.
    #[must_use] pub fn get_param(&self, key: &str) -> Option<&AzString> {
        self.params.get_key(key)
    }
}

impl_option!(RouteMatch, OptionRouteMatch, copy = false, [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]);

/// Match a URL path against a route pattern, extracting parameters.
///
/// Returns `Some(RouteMatch)` with extracted params on match, `None` otherwise.
///
/// # Examples
/// - pattern `"/user/:id"`, path `"/user/42"` → `Some(RouteMatch { params: [("id","42")] })`
/// - pattern `"/"`, path `"/"` → `Some(RouteMatch { params: [] })`
/// - pattern `"/about"`, path `"/settings"` → `None`
#[allow(clippy::similar_names)] // domain-standard coordinate/control-point names
#[must_use] pub fn match_route(pattern: &str, path: &str) -> Option<RouteMatch> {
    let pat_segs: Vec<&str> = pattern.split('/').filter(|s| !s.is_empty()).collect();
    let path_segs: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();

    if pat_segs.len() != path_segs.len() {
        return None;
    }

    let mut params = Vec::new();
    for (pat, val) in pat_segs.iter().zip(path_segs.iter()) {
        if let Some(param_name) = pat.strip_prefix(':') {
            params.push(AzStringPair {
                key: AzString::from(param_name.to_string()),
                value: AzString::from((*val).to_string()),
            });
        } else if pat != val {
            return None;
        }
    }

    Some(RouteMatch {
        pattern: AzString::from(pattern.to_string()),
        params: StringPairVec::from_vec(params),
    })
}

/// Configuration for optional features, such as whether to enable logging or panic hooks
#[derive(Debug, Clone)]
#[repr(C)]
pub struct AppConfig {
    /// If enabled, logs error and info messages.
    ///
    /// Default is `LevelFilter::Error` to log all errors by default
    pub log_level: AppLogLevel,
    /// If the app crashes / panics, a window with a message box pops up.
    /// Setting this to `false` disables the popup box.
    pub enable_visual_panic_hook: bool,
    /// If this is set to `true` (the default), a backtrace + error information
    /// gets logged to stdout and the logging file (only if logging is enabled).
    pub enable_logging_on_panic: bool,
    /// Determines what happens when all windows are closed.
    /// Default: `EndProcess` (terminate when last window closes).
    pub termination_behavior: AppTerminationBehavior,
    /// Icon provider for the application.
    /// Register icons here before calling `App::run()`.
    /// Each window will clone this provider (cheap, Arc-based).
    pub icon_provider: crate::icon::IconProviderHandle,
    /// Fonts bundled with the application.
    /// These fonts are loaded into memory and take priority over system fonts.
    pub bundled_fonts: NamedFontVec,
    /// Configuration for how system fonts should be loaded.
    /// Default: `LoadAllSystemFonts` (scan all system fonts at startup)
    pub font_loading: FontLoadingConfig,
    /// Optional mock environment for CSS evaluation.
    /// 
    /// When set, this overrides the auto-detected system properties (OS, theme, etc.)
    /// for CSS @-rules and dynamic selectors. This is useful for:
    /// - Testing OS-specific styles on a different platform
    /// - Screenshot testing with consistent environment
    /// - Previewing how the app looks on different systems
    /// 
    /// Default: None (use auto-detected system properties)
    pub mock_css_environment: OptionCssMockEnvironment,
    /// System style detected at startup (theme, colors, fonts, etc.)
    /// 
    /// This is detected once at `AppConfig::create()` and passed to all windows.
    /// You can override this after creation to use a custom system style,
    /// for example to test how your app looks on a different platform.
    pub system_style: SystemStyle,
    /// Component libraries registered at startup.
    ///
    /// Use `add_component()` to register individual components, or
    /// `add_component_library()` to register entire libraries.
    /// User-registered (and built-in) component libraries.
    ///
    /// The 52 built-in HTML elements are automatically registered by
    /// `AppConfig::create()` via `register_builtin_components`.
    /// Additional libraries can be added with `add_component_library`.
    pub component_libraries: ComponentLibraryVec,
    /// Registered routes mapping URL patterns to layout callbacks.
    ///
    /// Cross-platform: on desktop, the active route determines which layout
    /// callback runs. On web, routes map to HTTP endpoints and browser URLs.
    ///
    /// The first route (or `"/"`) is the default. Use `add_route()` to register.
    pub routes: RouteVec,
}

impl AppConfig {
    #[must_use] pub fn create() -> Self {
        let log_level = AppLogLevel::Error;
        let icon_provider = crate::icon::IconProviderHandle::new();
        let bundled_fonts = NamedFontVec::from_const_slice(&[]);
        let font_loading = FontLoadingConfig::default();
        let system_style = SystemStyle::detect();
        let mut s = Self {
            log_level,
            enable_visual_panic_hook: false,
            enable_logging_on_panic: true,
            termination_behavior: AppTerminationBehavior::default(),
            icon_provider,
            bundled_fonts,
            font_loading,
            mock_css_environment: OptionCssMockEnvironment::None,
            system_style,
            component_libraries: ComponentLibraryVec::from_const_slice(&[]),
            routes: RouteVec::from_const_slice(&[]),
        };
        // Dogfood: register the 52 built-in HTML elements via the
        // same `add_component_library` API that users call.
        // Annotated binding coerces the fn item to the fn-pointer type that
        // `Into<RegisterComponentLibraryFn>` is implemented for (no `as` cast).
        let register_builtin: crate::xml::RegisterComponentLibraryFnType =
            crate::xml::register_builtin_components;
        s.add_component_library(
            AzString::from_const_str("builtin"),
            register_builtin,
        );
        s
    }
    
    /// Create config with a mock CSS environment for testing
    /// 
    /// This allows you to simulate how your app would look on a different OS,
    /// with a different theme, language, or accessibility settings.
    /// 
    /// # Example
    /// ```rust
    /// # use azul_core::resources::{AppConfig, CssMockEnvironment};
    /// # use azul_css::dynamic_selector::{OsCondition, OptionOsCondition, ThemeCondition, OptionThemeCondition};
    /// let config = AppConfig::create()
    ///     .with_mock_environment(CssMockEnvironment {
    ///         os: OptionOsCondition::Some(OsCondition::Linux),
    ///         theme: OptionThemeCondition::Some(ThemeCondition::Dark),
    ///         ..Default::default()
    ///     });
    /// ```
    #[must_use] pub fn with_mock_environment(mut self, env: CssMockEnvironment) -> Self {
        self.mock_css_environment = OptionCssMockEnvironment::Some(env);
        self
    }

    /// Register a single component into a named library.
    ///
    /// Calls `register_fn` immediately and adds the returned `ComponentDef`
    /// to the library named `library`. If no library with that name exists,
    /// a new one is created. If a component with the same `id.name` already
    /// exists in the library, it is replaced.
    ///
    /// # C API
    /// ```c
    /// AzAppConfig_addComponent(&config, AzString_fromConstStr("mylib"), my_register_fn);
    /// ```
    pub fn add_component<R: Into<RegisterComponentFn>>(&mut self, library: AzString, register_fn: R) {
        let register_fn = register_fn.into();
        let component = (register_fn.cb)();
        let empty_libs = ComponentLibraryVec::from_const_slice(&[]);
        let mut libs = core::mem::replace(&mut self.component_libraries, empty_libs).into_library_owned_vec();

        if let Some(existing_lib) = libs.iter_mut().find(|l| l.name.as_str() == library.as_str()) {
            let empty_comps = ComponentDefVec::from_const_slice(&[]);
            let mut comps = core::mem::replace(&mut existing_lib.components, empty_comps).into_library_owned_vec();
            if let Some(ec) = comps.iter_mut().find(|c| c.id.name.as_str() == component.id.name.as_str()) {
                *ec = component;
            } else {
                comps.push(component);
            }
            existing_lib.components = ComponentDefVec::from_vec(comps);
        } else {
            libs.push(ComponentLibrary {
                name: library,
                version: AzString::from_const_str("1.0.0"),
                description: AzString::from_const_str(""),
                components: ComponentDefVec::from_vec(alloc::vec![component]),
                exportable: true,
                modifiable: true,
                data_models: crate::xml::ComponentDataModelVec::from_const_slice(&[]),
                enum_models: crate::xml::ComponentEnumModelVec::from_const_slice(&[]),
            });
        }

        self.component_libraries = ComponentLibraryVec::from_vec(libs);
    }

    /// Register an entire component library.
    ///
    /// Calls `register_fn` immediately and adds the returned
    /// `ComponentLibrary` to the config. Uses `name` as the library name
    /// (overriding whatever the function sets). If a library with the same
    /// name already exists, it is replaced wholesale.
    ///
    /// # C API
    /// ```c
    /// AzAppConfig_addComponentLibrary(&config, AzString_fromConstStr("vendor"), my_lib_fn);
    /// ```
    pub fn add_component_library<R: Into<RegisterComponentLibraryFn>>(&mut self, name: AzString, register_fn: R) {
        let register_fn = register_fn.into();
        let mut library = (register_fn.cb)();
        library.name = name;

        let empty_libs = ComponentLibraryVec::from_const_slice(&[]);
        let mut libs = core::mem::replace(&mut self.component_libraries, empty_libs).into_library_owned_vec();
        if let Some(existing) = libs.iter_mut().find(|l| l.name.as_str() == library.name.as_str()) {
            *existing = library;
        } else {
            libs.push(library);
        }

        self.component_libraries = ComponentLibraryVec::from_vec(libs);
    }

    /// Register a route mapping a URL pattern to a layout callback.
    ///
    /// On web: each route becomes an HTTP endpoint. On desktop: the first
    /// route (or `"/"`) is the initial layout, and `CallbackInfo::switch_route()`
    /// swaps the active callback.
    ///
    /// # C API
    /// ```c
    /// AzAppConfig_addRoute(&config, AzString_fromConstStr("/user/:id"), layout_user);
    /// ```
    pub fn add_route<P: Into<AzString>, L: Into<LayoutCallback>>(&mut self, pattern: P, layout_fn: L) {
        let route = Route {
            pattern: pattern.into(),
            layout_callback: layout_fn.into(),
        };
        let empty = RouteVec::from_const_slice(&[]);
        let mut routes = core::mem::replace(&mut self.routes, empty).into_library_owned_vec();
        // Replace existing route with the same pattern
        if let Some(existing) = routes.iter_mut().find(|r| r.pattern.as_str() == route.pattern.as_str()) {
            *existing = route;
        } else {
            routes.push(route);
        }
        self.routes = RouteVec::from_vec(routes);
    }

    /// Find the route matching a given URL path.
    ///
    /// Returns the matched `Route` and a `RouteMatch` with extracted parameters.
    #[must_use] pub fn match_route_for_path(&self, path: &str) -> Option<(&Route, RouteMatch)> {
        for route in self.routes.as_ref() {
            if let Some(m) = match_route(route.pattern.as_str(), path) {
                return Some((route, m));
            }
        }
        None
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self::create()
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum AppLogLevel {
    Off,
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

/// Metadata (but not storage) describing an image In `WebRender`.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct ImageDescriptor {
    /// Format of the image data.
    pub format: RawImageFormat,
    /// Width and height of the image data, in pixels.
    pub width: usize,
    pub height: usize,
    /// The number of bytes from the start of one row to the next. If non-None,
    /// `compute_stride` will return this value, otherwise it returns
    /// `width * bpp`. Different source of images have different alignment
    /// constraints for rows, so the stride isn't always equal to width * bpp.
    pub stride: OptionI32,
    /// Offset in bytes of the first pixel of this image in its backing buffer.
    /// This is used for tiling, wherein `WebRender` extracts chunks of input images
    /// in order to cache, manipulate, and render them individually. This offset
    /// tells the texture upload machinery where to find the bytes to upload for
    /// this tile. Non-tiled images generally set this to zero.
    pub offset: i32,
    /// Various bool flags related to this descriptor.
    pub flags: ImageDescriptorFlags,
}

/// Various flags that are part of an image descriptor.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct ImageDescriptorFlags {
    /// Whether this image is opaque, or has an alpha channel. Avoiding blending
    /// for opaque surfaces is an important optimization.
    pub is_opaque: bool,
    /// Whether to allow the driver to automatically generate mipmaps. If images
    /// are already downscaled appropriately, mipmap generation can be wasted
    /// work, and cause performance problems on some cards/drivers.
    ///
    /// See <https://github.com/servo/webrender/pull/2555>/
    pub allow_mipmaps: bool,
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct IdNamespace(pub u32);

impl ::core::fmt::Display for IdNamespace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "IdNamespace({})", self.0)
    }
}

impl ::core::fmt::Debug for IdNamespace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self}")
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum RawImageFormat {
    R8,
    RG8,
    RGB8,
    RGBA8,
    R16,
    RG16,
    RGB16,
    RGBA16,
    BGR8,
    BGRA8,
    RGBF32,
    RGBAF32,
}

// NOTE: starts at 1 (0 = DUMMY)
static IMAGE_KEY: AtomicU64 = AtomicU64::new(1);
static FONT_KEY: AtomicU64 = AtomicU64::new(0);
static FONT_INSTANCE_KEY: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ImageKey {
    pub namespace: IdNamespace,
    pub key: u64,
}

impl ImageKey {
    pub const DUMMY: Self = Self {
        namespace: IdNamespace(0),
        key: 0,
    };

    pub fn unique(render_api_namespace: IdNamespace) -> Self {
        Self {
            namespace: render_api_namespace,
            key: IMAGE_KEY.fetch_add(1, AtomicOrdering::SeqCst),
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FontKey {
    pub namespace: IdNamespace,
    pub key: u64,
}

impl FontKey {
    pub fn unique(render_api_namespace: IdNamespace) -> Self {
        Self {
            namespace: render_api_namespace,
            key: FONT_KEY.fetch_add(1, AtomicOrdering::SeqCst),
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FontInstanceKey {
    pub namespace: IdNamespace,
    pub key: u64,
}

impl FontInstanceKey {
    pub fn unique(render_api_namespace: IdNamespace) -> Self {
        Self {
            namespace: render_api_namespace,
            key: FONT_INSTANCE_KEY.fetch_add(1, AtomicOrdering::SeqCst),
        }
    }
}

// NOTE: This type should NOT be exposed in the API!
// The only public functions are the constructors
#[derive(Debug)]
pub enum DecodedImage {
    /// Image that has a reserved key, but no data, i.e it is not yet rendered
    /// or there was an error during rendering
    NullImage {
        width: usize,
        height: usize,
        format: RawImageFormat,
        /// Sometimes images need to be tagged with extra data
        tag: Vec<u8>,
    },
    // OpenGl texture
    Gl(Texture),
    // Image backed by CPU-rendered pixels
    Raw((ImageDescriptor, ImageData)),
    // Same as `Texture`, but rendered AFTER the layout has been done
    Callback(CoreImageCallback),
    // YUVImage(...)
    // VulkanSurface(...)
    // MetalSurface(...),
    // DirectXSurface(...)
}

#[derive(Debug)]
#[repr(C)]
pub struct ImageRef {
    /// Shared pointer to an opaque implementation of the decoded image
    pub data: *const DecodedImage,
    /// How many copies does this image have (if 0, the font data will be deleted on drop)
    pub copies: *const AtomicUsize,
    /// Process-unique, monotonically-assigned identity of the *decoded image*
    /// (see [`ImageRefHash`]). Shared by shallow clones (they are the same
    /// image), fresh for [`ImageRef::deep_copy`] and every `new_*` (a
    /// different image). Unlike the old `data`-pointer identity this is drawn
    /// from a never-reused counter, so freeing an image and reusing its heap
    /// address can never make a *new* image collide with a stale key — the
    /// prerequisite for image GC (see resources.rs `image_ref_get_hash`).
    pub id: u64,
    pub run_destructor: bool,
}

/// Never-reused source of [`ImageRef::id`]. Starts at 1 so `id == 0` can flag
/// an un-initialised / raw-reconstructed handle.
static IMAGE_REF_ID_COUNTER: AtomicU64 = AtomicU64::new(1);

#[must_use]
fn next_image_ref_id() -> u64 {
    IMAGE_REF_ID_COUNTER.fetch_add(1, AtomicOrdering::SeqCst)
}

impl ImageRef {
    #[must_use] pub const fn get_hash(&self) -> ImageRefHash {
        image_ref_get_hash(self)
    }
}

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd, Hash, Ord, Eq)]
#[repr(C)]
pub struct ImageRefHash {
    pub inner: u64,
}

impl_option!(
    ImageRef,
    OptionImageRef,
    copy = false,
    [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

impl ImageRef {
    /// If *copies = 1, returns the internal image data
    #[must_use] pub fn into_inner(self) -> Option<DecodedImage> {
        // SAFETY: `data`/`copies` are non-null heap allocations from `Box::into_raw`
        // in `new()` (never mutated afterwards). When `copies == 1` we are the sole
        // owner, so reclaiming both Boxes and `forget`-ing `self` transfers ownership
        // without a double free / running the destructor twice.
        unsafe {
            if self.copies.as_ref().map(|m| m.load(AtomicOrdering::SeqCst)) == Some(1) {
                let data = Box::from_raw(self.data.cast_mut());
                drop(Box::from_raw(self.copies.cast_mut()));
                core::mem::forget(self); // do not run the destructor
                Some(*data)
            } else {
                None
            }
        }
    }

    #[must_use] pub const fn get_data(&self) -> &DecodedImage {
        // SAFETY: `data` is a non-null, live `Box` allocation owned by this handle
        // (and its shallow clones) until the last copy drops; the returned borrow is
        // tied to `&self`, so it cannot outlive the allocation.
        unsafe { &*self.data }
    }

    #[must_use] pub fn get_image_callback(&self) -> Option<&CoreImageCallback> {
        // SAFETY: `copies` is a non-null, live allocation for the lifetime of `&self`.
        if unsafe { self.copies.as_ref().map(|m| m.load(AtomicOrdering::SeqCst)) != Some(1) } {
            return None; // not safe: shared, so no exclusive borrow of the data
        }

        // SAFETY: `data` is a non-null, live `Box` allocation; borrow tied to `&self`.
        match unsafe { &*self.data } {
            DecodedImage::Callback(gl_texture_callback) => Some(gl_texture_callback),
            _ => None,
        }
    }

    pub fn get_image_callback_mut(&mut self) -> Option<&mut CoreImageCallback> {
        // SAFETY: `copies` is a non-null, live allocation for the lifetime of `&self`.
        if unsafe { self.copies.as_ref().map(|m| m.load(AtomicOrdering::SeqCst)) != Some(1) } {
            return None; // not safe: shared, so a &mut would alias other clones' data
        }

        // SAFETY: `copies == 1` proven above, so `&mut self` is the unique owner of
        // the `data` allocation; the exclusive borrow is tied to `&mut self`.
        match unsafe { &mut *self.data.cast_mut() } {
            DecodedImage::Callback(gl_texture_callback) => Some(gl_texture_callback),
            _ => None,
        }
    }

    /// In difference to the default shallow copy, creates a new image ref
    #[must_use] pub fn deep_copy(&self) -> Self {
        let new_data = match self.get_data() {
            DecodedImage::NullImage {
                width,
                height,
                format,
                tag,
            } => DecodedImage::NullImage {
                width: *width,
                height: *height,
                format: *format,
                tag: tag.clone(),
            },
            // NOTE: textures cannot be deep-copied yet (since the OpenGL calls for that
            // are missing from the trait), so calling clone() on a GL texture will result in an
            // empty image
            DecodedImage::Gl(tex) => DecodedImage::NullImage {
                width: tex.size.width as usize,
                height: tex.size.height as usize,
                format: tex.format,
                tag: Vec::new(),
            },
            // WARNING: the data may still be a U8Vec<'static> - the data may still not be
            // actually cloned. The data only gets cloned on a write operation
            DecodedImage::Raw((descriptor, data)) => {
                DecodedImage::Raw((*descriptor, data.clone()))
            }
            DecodedImage::Callback(cb) => DecodedImage::Callback(cb.clone()),
        };

        Self::new(new_data)
    }

    #[must_use] pub const fn is_null_image(&self) -> bool {
        matches!(self.get_data(), DecodedImage::NullImage { .. })
    }

    #[must_use] pub const fn is_gl_texture(&self) -> bool {
        matches!(self.get_data(), DecodedImage::Gl(_))
    }

    #[must_use] pub const fn is_raw_image(&self) -> bool {
        matches!(self.get_data(), DecodedImage::Raw((_, _)))
    }

    #[must_use] pub const fn is_callback(&self) -> bool {
        matches!(self.get_data(), DecodedImage::Callback(_))
    }

    // OptionRawImage
    #[must_use] pub fn get_rawimage(&self) -> Option<RawImage> {
        match self.get_data() {
            DecodedImage::Raw((image_descriptor, image_data)) => Some(RawImage {
                pixels: match image_data {
                    ImageData::Raw(shared_data) => {
                        // Clone the SharedRawImageData (increments ref count),
                        // then try to extract or convert to U8Vec
                        let data_clone = shared_data.clone();
                        data_clone.into_inner().map_or_else(|| RawImageData::U8(shared_data.as_ref().to_vec().into()), RawImageData::U8)
                    }
                    ImageData::External(_) => return None,
                },
                width: image_descriptor.width,
                height: image_descriptor.height,
                premultiplied_alpha: true,
                data_format: image_descriptor.format,
                tag: Vec::new().into(),
            }),
            _ => None,
        }
    }

    /// Get raw bytes from the image as a slice
    /// Returns None if this is not a Raw image or if it's an External image
    #[must_use] pub fn get_bytes(&self) -> Option<&[u8]> {
        match self.get_data() {
            DecodedImage::Raw((_, image_data)) => match image_data {
                ImageData::Raw(shared_data) => Some(shared_data.as_ref()),
                ImageData::External(_) => None,
            },
            _ => None,
        }
    }

    /// Get a pointer to the raw bytes for debugging/profiling purposes
    /// Returns a unique pointer for this `ImageRef`'s data
    #[must_use] pub fn get_bytes_ptr(&self) -> *const u8 {
        match self.get_data() {
            DecodedImage::Raw((_, image_data)) => match image_data {
                ImageData::Raw(shared_data) => shared_data.as_ptr(),
                ImageData::External(_) => core::ptr::null(),
            },
            _ => core::ptr::null(),
        }
    }

    /// NOTE: returns (0, 0) for a Callback
    #[allow(clippy::cast_precision_loss)] // image/graphics: bounded pixel/colour/dimension/unit casts
    #[must_use] pub const fn get_size(&self) -> LogicalSize {
        match self.get_data() {
            DecodedImage::NullImage { width, height, .. } => {
                LogicalSize::new(*width as f32, *height as f32)
            }
            DecodedImage::Gl(tex) => {
                LogicalSize::new(tex.size.width as f32, tex.size.height as f32)
            }
            DecodedImage::Raw((image_descriptor, _)) => LogicalSize::new(
                image_descriptor.width as f32,
                image_descriptor.height as f32,
            ),
            DecodedImage::Callback(_) => LogicalSize::new(0.0, 0.0),
        }
    }

    #[must_use] pub fn null_image(width: usize, height: usize, format: RawImageFormat, tag: Vec<u8>) -> Self {
        Self::new(DecodedImage::NullImage {
            width,
            height,
            format,
            tag,
        })
    }

    pub fn callback<C: Into<CoreRenderImageCallback>>(callback: C, data: RefAny) -> Self {
        Self::new(DecodedImage::Callback(CoreImageCallback {
            callback: callback.into(),
            refany: data,
        }))
    }

    #[must_use] pub fn new_rawimage(image_data: RawImage) -> Option<Self> {
        let (image_data, image_descriptor) = image_data.into_loaded_image_source()?;
        Some(Self::new(DecodedImage::Raw((image_descriptor, image_data))))
    }

    #[must_use] pub fn new_gltexture(texture: Texture) -> Self {
        Self::new(DecodedImage::Gl(texture))
    }

    fn new(data: DecodedImage) -> Self {
        Self {
            data: Box::into_raw(Box::new(data)),
            copies: Box::into_raw(Box::new(AtomicUsize::new(1))),
            id: next_image_ref_id(),
            run_destructor: true,
        }
    }

    // pub fn new_vulkan(...) -> Self
}

// SAFETY: the raw pointers only ever address heap `Box`es whose contents are
// themselves `Send`/`Sync`, and all cross-thread refcount mutation goes through the
// `AtomicUsize` in `copies`, so sharing/moving a handle across threads is sound.
unsafe impl Send for ImageRef {}
unsafe impl Sync for ImageRef {}

// Identity is the never-reused `id`, NOT the `data` pointer: two shallow
// clones of one image share an `id` (equal); distinct images (incl. a
// `deep_copy`) get distinct ids; a freed image's id is never handed to a
// later image, so a reused heap address can't forge equality.
impl PartialEq for ImageRef {
    fn eq(&self, rhs: &Self) -> bool {
        self.id == rhs.id
    }
}

impl PartialOrd for ImageRef {
    fn partial_cmp(&self, other: &Self) -> Option<::core::cmp::Ordering> {
        Some(self.id.cmp(&other.id))
    }
}

impl Ord for ImageRef {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.id.cmp(&other.id)
    }
}

impl Eq for ImageRef {}

impl Hash for ImageRef {
    fn hash<H>(&self, state: &mut H)
    where
        H: Hasher,
    {
        self.id.hash(state);
    }
}

impl Clone for ImageRef {
    fn clone(&self) -> Self {
        // SAFETY: `copies` is a non-null, live `AtomicUsize` allocation shared by all
        // clones; the atomic increment balances the `fetch_sub` in `Drop`.
        unsafe {
            self.copies
                .as_ref()
                .map(|m| m.fetch_add(1, AtomicOrdering::SeqCst));
        }
        Self {
            data: self.data,     // copy the pointer
            copies: self.copies, // copy the pointer
            id: self.id,         // same image → same identity
            run_destructor: true,
        }
    }
}

impl Drop for ImageRef {
    fn drop(&mut self) {
        self.run_destructor = false;
        // SAFETY: `data`/`copies` are non-null, live `Box` allocations shared by all
        // clones. `fetch_sub` returns the pre-decrement count, so `== 1` means this is
        // the last owner; only then do we reclaim both Boxes exactly once.
        unsafe {
            let copies = (*self.copies).fetch_sub(1, AtomicOrdering::SeqCst);
            if copies == 1 {
                drop(Box::from_raw(self.data.cast_mut()));
                drop(Box::from_raw(self.copies.cast_mut()));
            }
        }
    }
}

#[must_use] pub const fn image_ref_get_hash(ir: &ImageRef) -> ImageRefHash {
    // The identity is the never-reused `id`, not the freeable `data` pointer
    // (see the `id` field docs). This is what makes an ImageKey safe to
    // DeleteImage: once an image is dropped its id is retired forever, so a
    // future image that reuses the same heap address gets a *different* key
    // and is registered/uploaded correctly instead of aliasing the stale one.
    ImageRefHash {
        inner: ir.id,
    }
}

/// Convert a stable `ImageRefHash` directly to an `ImageKey`.
///
/// `ImageKey.key` is a `u64` and `ImageRefHash.inner` is the `ImageRef` `id`
/// (a `u64` counter) stored in a `usize`; on a 32-bit host that truncates the
/// top 32 bits, which is fine — a run would need 4 billion live images for the
/// low 32 bits to collide.
#[must_use] pub const fn image_ref_hash_to_image_key(hash: ImageRefHash, namespace: IdNamespace) -> ImageKey {
    ImageKey {
        namespace,
        key: hash.inner,
    }
}

#[must_use] pub fn font_ref_get_hash(fr: &FontRef) -> u64 {
    fr.get_hash()
}

/// Stores the resources for the application, such as fonts, images and cached
/// texts, also clipboard strings
///
/// Images and fonts can be references across window contexts (not yet tested,
/// but should work).
#[derive(Debug)]
#[derive(Default)]
pub struct ImageCache {
    /// The `AzString` is the string used in the CSS, i.e. `url("my_image`") = "`my_image`" -> ImageId(4)
    ///
    /// NOTE: This is the only map that is modifiable by the user and that has to be manually
    /// managed all other maps are library-internal only and automatically delete their
    /// resources once they aren't needed anymore
    pub image_id_map: OrderedMap<AzString, ImageRef>,
}


impl ImageCache {
    #[must_use] pub fn new() -> Self {
        Self::default()
    }

    // -- ImageId cache

    pub fn add_css_image_id(&mut self, css_id: AzString, image: ImageRef) {
        self.image_id_map.insert(css_id, image);
    }

    #[must_use] pub fn get_css_image_id(&self, css_id: &AzString) -> Option<&ImageRef> {
        self.image_id_map.get(css_id)
    }

    pub fn delete_css_image_id(&mut self, css_id: &AzString) {
        self.image_id_map.remove(css_id);
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct ResolvedImage {
    pub key: ImageKey,
    pub descriptor: ImageDescriptor,
}

/// Trait for accessing font resources
pub trait RendererResourcesTrait: fmt::Debug {
    /// Get a font family hash from a font families hash
    fn get_font_family(
        &self,
        style_font_families_hash: &StyleFontFamiliesHash,
    ) -> Option<&StyleFontFamilyHash>;

    /// Get a font key from a font family hash
    fn get_font_key(&self, style_font_family_hash: &StyleFontFamilyHash) -> Option<&FontKey>;

    /// Get a registered font and its instances from a font key
    fn get_registered_font(
        &self,
        font_key: &FontKey,
    ) -> Option<&(FontRef, OrderedMap<(Au, DpiScaleFactor), FontInstanceKey>)>;

    /// Get image information from an image hash
    fn get_image(&self, hash: &ImageRefHash) -> Option<&ResolvedImage>;

    /// Update an image descriptor for an existing image hash
    fn update_image(
        &mut self,
        image_ref_hash: &ImageRefHash,
        descriptor: ImageDescriptor,
    );
}

// Implementation for the original RendererResources struct
impl RendererResourcesTrait for RendererResources {
    fn get_font_family(
        &self,
        style_font_families_hash: &StyleFontFamiliesHash,
    ) -> Option<&StyleFontFamilyHash> {
        self.font_families_map.get(style_font_families_hash)
    }

    fn get_font_key(&self, style_font_family_hash: &StyleFontFamilyHash) -> Option<&FontKey> {
        self.font_id_map.get(style_font_family_hash)
    }

    fn get_registered_font(
        &self,
        font_key: &FontKey,
    ) -> Option<&(FontRef, OrderedMap<(Au, DpiScaleFactor), FontInstanceKey>)> {
        self.currently_registered_fonts.get(font_key)
    }

    fn get_image(&self, hash: &ImageRefHash) -> Option<&ResolvedImage> {
        self.currently_registered_images.get(hash)
    }

    fn update_image(
        &mut self,
        image_ref_hash: &ImageRefHash,
        descriptor: ImageDescriptor,
    ) {
        if let Some(s) = self.currently_registered_images.get_mut(image_ref_hash) {
            s.descriptor = descriptor;
        }
    }
}

/// Renderer resources that manage font, image and font instance keys.
/// `RendererResources` are local to each renderer / window, since the
/// keys are not shared across renderers
///
/// The resources are automatically managed, meaning that they each new frame
/// (signified by `start_frame_gc` and `end_frame_gc`)
#[derive(Default)]
pub struct RendererResources {
    /// All image keys currently active in the `RenderApi`
    pub currently_registered_images: OrderedMap<ImageRefHash, ResolvedImage>,
    /// Reverse lookup: `ImageKey` -> `ImageRefHash` for display list translation
    pub image_key_map: OrderedMap<ImageKey, ImageRefHash>,
    /// Image GC bookkeeping: last epoch (as `u32`) each registered image was
    /// seen referenced by a display list. An image absent for more than
    /// `IMAGE_GC_KEEP_EPOCHS` frames is `DeleteImage`d and evicted — this is
    /// what stops the unbounded texture growth of a window that swaps images
    /// every frame (video / capture / animated charts). Safe because
    /// `ImageRefHash` is now a never-reused id, not a freeable pointer.
    pub image_last_seen_epoch: OrderedMap<ImageRefHash, u32>,
    /// All font keys currently active in the `RenderApi`
    pub currently_registered_fonts:
        OrderedMap<FontKey, (FontRef, OrderedMap<(Au, DpiScaleFactor), FontInstanceKey>)>,
    /// Fonts registered on the last frame
    ///
    /// Fonts differ from images in that regard that we can't immediately
    /// delete them on a new frame, instead we have to delete them on "current frame + 1"
    /// This is because when the frame is being built, we do not know
    /// whether the font will actually be successfully loaded
    pub last_frame_registered_fonts:
        OrderedMap<FontKey, OrderedMap<(Au, DpiScaleFactor), FontInstanceKey>>,
    /// Map from the calculated families vec (`["Arial", "Helvetica"]`)
    /// to the final loaded font that could be loaded
    /// (in this case "Arial" on Windows and "Helvetica" on Mac,
    /// because the fonts are loaded in fallback-order)
    pub font_families_map: OrderedMap<StyleFontFamiliesHash, StyleFontFamilyHash>,
    /// Same as `AzString` -> `ImageId`, but for fonts, i.e. "Roboto" -> FontId(9)
    pub font_id_map: OrderedMap<StyleFontFamilyHash, FontKey>,
    /// Direct mapping from font hash (from `FontRef`) to `FontKey`
    /// TODO: This should become part of `SharedFontRegistry`
    pub font_hash_map: OrderedMap<u64, FontKey>,
}

impl fmt::Debug for RendererResources {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "RendererResources {{
                currently_registered_images: {:#?},
                currently_registered_fonts: {:#?},
                font_families_map: {:#?},
                font_id_map: {:#?},
            }}",
            self.currently_registered_images.keys().collect::<Vec<_>>(),
            self.currently_registered_fonts.keys().collect::<Vec<_>>(),
            self.font_families_map.keys().collect::<Vec<_>>(),
            self.font_id_map.keys().collect::<Vec<_>>(),
        )
    }
}


impl RendererResources {
    #[must_use] pub fn get_renderable_font_data(
        &self,
        font_instance_key: &FontInstanceKey,
    ) -> Option<(&FontRef, Au, DpiScaleFactor)> {
        self.currently_registered_fonts
            .iter()
            .find_map(|(font_key, (font_ref, instances))| {
                instances.iter().find_map(|((au, dpi), instance_key)| {
                    if *instance_key == *font_instance_key {
                        Some((font_ref, *au, *dpi))
                    } else {
                        None
                    }
                })
            })
    }

    #[allow(clippy::cast_possible_truncation)] // image/graphics: bounded pixel/colour/dimension/unit casts
    pub fn get_font_instance_key_for_text(
        &self,
        font_size_px: f32,
        css_property_cache: &CssPropertyCache,
        node_data: &NodeData,
        node_id: &NodeId,
        styled_node_state: &StyledNodeState,
        dpi_scale: f32,
    ) -> Option<FontInstanceKey> {
        // Convert font size to StyleFontSize.
        //
        // `font_size_px as isize` saturates +inf / f32::MAX to isize::MAX (and
        // -inf / -f32::MAX to isize::MIN). `const_px` then multiplies by 1000
        // (FP_PRECISION_MULTIPLIER) inside `FloatValue::const_new`, which would
        // overflow. Clamp to the range that survives that multiply so an absurd
        // size misses cleanly instead of panicking.
        let font_size_isize =
            (font_size_px as isize).clamp(isize::MIN / 1000, isize::MAX / 1000);
        let font_size = StyleFontSize {
            inner: azul_css::props::basic::PixelValue::const_px(font_size_isize),
        };

        // Convert to application units
        let font_size_au = font_size_to_au(font_size);

        // Create DPI scale factor
        let dpi_scale_factor = DpiScaleFactor {
            inner: FloatValue::new(dpi_scale),
        };

        // Get font family
        let font_family =
            css_property_cache.get_font_id_or_default(node_data, node_id, styled_node_state);

        // Calculate hash and lookup font instance key
        let font_families_hash = StyleFontFamiliesHash::new(font_family.as_ref());

        self.get_font_instance_key(&font_families_hash, font_size_au, dpi_scale_factor)
    }

    #[must_use] pub fn get_font_instance_key(
        &self,
        font_families_hash: &StyleFontFamiliesHash,
        font_size_au: Au,
        dpi_scale: DpiScaleFactor,
    ) -> Option<FontInstanceKey> {
        let font_family_hash = self.get_font_family(font_families_hash)?;
        let font_key = self.get_font_key(font_family_hash)?;
        let (_, instances) = self.get_registered_font(font_key)?;
        instances.get(&(font_size_au, dpi_scale)).copied()
    }

    // Delete all font family hashes that do not have a font key anymore
    //
    // AUDIT-TODO (font GC, resources.rs font leak — 2026-07-08):
    // Fonts and font instances are currently NEVER garbage-collected. This helper
    // only prunes `font_id_map` / `font_families_map` entries whose `FontKey` has
    // *already* vanished from `currently_registered_fonts` — but nothing ever
    // removes fonts from `currently_registered_fonts` in the first place, and this
    // helper itself has no callers. No `DeleteFont` / `DeleteFontInstance`
    // `ResourceUpdate` is ever emitted, so WebRender font memory grows unbounded
    // when an app cycles fonts (font pickers, editors, live CSS).
    //
    // To wire a real font GC mirroring the image GC (see `dll/.../wr_translate2.rs`
    // `garbage_collect_images` + `image_last_seen_epoch`), the following are needed
    // and MUST be done together (do not half-implement):
    //   1. Add `font_last_seen_epoch: OrderedMap<FontKey, u32>` (and, if instance-
    //      level GC is wanted, per-`FontInstanceKey` epochs) to `RendererResources`.
    //   2. In the display-list build (dll crate), after resolving each glyph run's
    //      `FontInstanceKey`, mark the owning `FontKey` (and instance) seen at the
    //      current epoch — exactly as images are marked in the image GC.
    //   3. Add a `garbage_collect_fonts(&mut self, now, keep_epochs, updates)` that,
    //      for every `FontKey` unseen for > keep_epochs frames, emits
    //      `DeleteFontInstance` for each of its instances then `DeleteFont`, and
    //      evicts the key from `currently_registered_fonts`, `font_hash_map`,
    //      `last_frame_registered_fonts`, and `font_id_map`/`font_families_map`
    //      (via this helper). Respect the "delete on current frame + 1" rule already
    //      documented on `last_frame_registered_fonts`.
    //   4. Call it once per frame from the same site as the image GC.
    // Left as a TODO because steps 2 and 4 are cross-crate (dll) and cannot be
    // implemented from `azul-core` alone; adding a GC method here without a caller
    // would just be more dead code.
    #[allow(dead_code)]
    fn remove_font_families_with_zero_references(&mut self) {
        let font_family_to_delete = self
            .font_id_map
            .iter()
            .filter_map(|(font_family, font_key)| {
                if self.currently_registered_fonts.contains_key(font_key) {
                    None
                } else {
                    Some(*font_family)
                }
            })
            .collect::<Vec<_>>();

        for f in font_family_to_delete {
            self.font_id_map.remove(&f); // font key does not exist anymore
        }

        let font_families_to_delete = self
            .font_families_map
            .iter()
            .filter_map(|(font_families, font_family)| {
                if self.font_id_map.contains_key(font_family) {
                    None
                } else {
                    Some(*font_families)
                }
            })
            .collect::<Vec<_>>();

        for f in font_families_to_delete {
            self.font_families_map.remove(&f); // font family does not exist anymore
        }
    }
}

// Result returned from rerender_image_callback() - should be used as:
//
// ```rust
// txn.update_image(
//     wr_translate_image_key(key),
//     wr_translate_image_descriptor(descriptor),
//     wr_translate_image_data(data),
//     &WrImageDirtyRect::All,
// );
// ```
#[derive(Debug, Clone)]
pub struct UpdateImageResult {
    pub key_to_update: ImageKey,
    pub new_descriptor: ImageDescriptor,
    pub new_image_data: ImageData,
}

#[derive(Debug, Default)]
pub struct GlTextureCache {
    pub solved_textures:
        BTreeMap<DomId, BTreeMap<NodeId, (ImageKey, ImageDescriptor, ExternalImageId)>>,
    pub hashes: BTreeMap<(DomId, NodeId, ImageRefHash), ImageRefHash>,
}

// necessary so the display list can be built in parallel
// SAFETY: only the raw pointers inside the contained `ImageRefHash`/key maps are
// non-`Send`-inferring; every stored value is a plain POD id/descriptor with no
// interior aliasing, so moving the cache to another thread is sound.
unsafe impl Send for GlTextureCache {}

impl GlTextureCache {
    /// Initializes an empty cache
    #[must_use] pub const fn empty() -> Self {
        Self {
            solved_textures: BTreeMap::new(),
            hashes: BTreeMap::new(),
        }
    }

    /// Updates a given texture
    ///
    /// This is called when a texture needs to be re-rendered (e.g., on resize or animation frame).
    /// It updates the texture in the `WebRender` external image cache and updates the internal
    /// descriptor to reflect the new size.
    ///
    /// # Arguments
    ///
    /// * `dom_id` - The DOM ID containing the texture
    /// * `node_id` - The node ID of the image element
    /// * `document_id` - The `WebRender` document ID
    /// * `epoch` - The current frame epoch
    /// * `new_texture` - The new texture to use
    /// * `insert_into_active_gl_textures_fn` - Function to insert the texture into the cache
    ///
    /// # Returns
    ///
    /// The `ExternalImageId` if successful, None if the texture wasn't found in the cache
    pub fn update_texture(
        &mut self,
        dom_id: DomId,
        node_id: NodeId,
        document_id: DocumentId,
        epoch: Epoch,
        new_texture: Texture,
        insert_into_active_gl_textures_fn: &GlStoreImageFn,
    ) -> Option<ExternalImageId> {
        let new_descriptor = new_texture.get_descriptor();
        let di_map = self.solved_textures.get_mut(&dom_id)?;
        let entry = di_map.get_mut(&node_id)?;

        // Update the descriptor
        entry.1 = new_descriptor;

        // The ExternalImageId is deterministic from (dom_id, node_id), so the cache
        // entry can keep referencing the same id across re-renders.
        let external_image_id = texture_external_image_id(dom_id, node_id);
        (insert_into_active_gl_textures_fn)(document_id, epoch, new_texture, external_image_id);
        entry.2 = external_image_id;

        Some(external_image_id)
    }
}

macro_rules! unique_id {
    ($struct_name:ident, $counter_name:ident) => {
        #[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Ord, PartialOrd)]
        #[repr(C)]
        pub struct $struct_name {
            pub id: usize,
        }

        impl $struct_name {
            pub fn unique() -> Self {
                Self {
                    id: $counter_name.fetch_add(1, AtomicOrdering::SeqCst),
                }
            }
        }
    };
}

// NOTE: the property key is unique across transform, color and opacity properties
static PROPERTY_KEY_COUNTER: AtomicUsize = AtomicUsize::new(0);
unique_id!(TransformKey, PROPERTY_KEY_COUNTER);
unique_id!(ColorKey, PROPERTY_KEY_COUNTER);
unique_id!(OpacityKey, PROPERTY_KEY_COUNTER);

static IMAGE_ID_COUNTER: AtomicUsize = AtomicUsize::new(0);
unique_id!(ImageId, IMAGE_ID_COUNTER);
static FONT_ID_COUNTER: AtomicUsize = AtomicUsize::new(0);
unique_id!(FontId, FONT_ID_COUNTER);

#[derive(Debug, Clone, PartialEq, PartialOrd, Eq, Ord, Hash)]
#[repr(C)]
pub struct ImageMask {
    pub image: ImageRef,
    pub rect: LogicalRect,
    pub repeat: bool,
}

impl_option!(
    ImageMask,
    OptionImageMask,
    copy = false,
    [Debug, Clone, PartialEq, PartialOrd, Eq, Ord, Hash]
);

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ImmediateFontId {
    Resolved((StyleFontFamilyHash, FontKey)),
    Unresolved(StyleFontFamilyVec),
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
#[repr(C, u8)]
pub enum RawImageData {
    // 8-bit image data
    U8(U8Vec),
    // 16-bit image data
    U16(U16Vec),
    // HDR image data
    F32(F32Vec),
}

impl RawImageData {
    #[must_use] pub const fn get_u8_vec_ref(&self) -> Option<&U8Vec> {
        match self {
            Self::U8(v) => Some(v),
            _ => None,
        }
    }

    #[must_use] pub const fn get_u16_vec_ref(&self) -> Option<&U16Vec> {
        match self {
            Self::U16(v) => Some(v),
            _ => None,
        }
    }

    #[must_use] pub const fn get_f32_vec_ref(&self) -> Option<&F32Vec> {
        match self {
            Self::F32(v) => Some(v),
            _ => None,
        }
    }

    fn get_u8_vec(self) -> Option<U8Vec> {
        match self {
            Self::U8(v) => Some(v),
            _ => None,
        }
    }

    fn get_u16_vec(self) -> Option<U16Vec> {
        match self {
            Self::U16(v) => Some(v),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct RawImage {
    pub pixels: RawImageData,
    pub width: usize,
    pub height: usize,
    pub premultiplied_alpha: bool,
    pub data_format: RawImageFormat,
    pub tag: U8Vec,
}

/// A soft round brush for the painting API.
///
/// The same parameters drive the CPU
/// rasterizer ([`RawImage::paint_dot`]) and the GPU brush shader, so a stroke
/// looks identical whether it lands on a `RawImage` or a `Texture`.
#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct Brush {
    /// Brush color (its alpha scales the dab opacity together with `flow`).
    pub color: ColorU,
    /// Brush radius in pixels.
    pub radius: f32,
    /// Edge hardness, `0.0` (fully feathered) .. `1.0` (hard edge). Opaque out
    /// to `hardness * radius`, then a smooth falloff to zero at the edge.
    pub hardness: f32,
    /// Per-dab opacity multiplier, `0.0`..`1.0`. Values < 1 let overlapping dabs
    /// build up smoothly (the "metaball"-like blend).
    pub flow: f32,
    /// Spacing between stamped dabs along a stroke, as a fraction of `radius`
    /// (e.g. `0.25` = a dab every quarter-radius). Smaller = smoother + slower.
    pub spacing: f32,
}

impl Brush {
    /// A sensible default brush: medium-soft, full flow, dense spacing.
    #[must_use] pub const fn new(color: ColorU, radius: f32) -> Self {
        Self {
            color,
            radius,
            hardness: 0.5,
            flow: 1.0,
            spacing: 0.25,
        }
    }
}

/// Brush dab coverage: `1.0` at the dab center, smoothly `0.0` at its edge.
///
/// `t` is `distance / radius` in `[0, 1]`; `hardness` in `[0, 1]`. Single source
/// of truth for the dab profile -- the GPU brush shader computes the identical
/// `1 - smoothstep(hardness, 1, t)` so CPU and GPU strokes match.
#[allow(clippy::suboptimal_flops)] // mul_add not guaranteed faster/available without target +fma; keep explicit a*b+c
#[inline]
#[must_use] pub fn brush_dab_coverage(t: f32, hardness: f32) -> f32 {
    let edge0 = hardness.clamp(0.0, 1.0);
    let denom = (1.0 - edge0).max(1.0e-4);
    let x = ((t - edge0) / denom).clamp(0.0, 1.0);
    1.0 - (x * x * (3.0 - 2.0 * x))
}

impl RawImage {
    /// CPU painting: stamp one brush dab centered at (`cx`, `cy`) in pixel
    /// coordinates, alpha-over compositing a radial-falloff disc. Only 8-bit
    /// `RGBA8`/`BGRA8` images are painted (other formats are left untouched).
    /// This is the CPU mirror of the GPU brush shader.
    #[allow(clippy::suboptimal_flops)] // mul_add not guaranteed faster/available without target +fma; keep explicit a*b+c
    #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss, clippy::cast_sign_loss)] // image/graphics: bounded pixel/colour/dimension/unit casts
    #[allow(clippy::cast_possible_wrap)] // image/graphics: bounded pixel/colour casts
    pub fn paint_dot(&mut self, cx: f32, cy: f32, brush: Brush) {
        let r = brush.radius;
        // `!(r > 0.0)` intentionally also rejects NaN (`r <= 0.0` would not).
        #[allow(clippy::neg_cmp_op_on_partial_ord)]
        if !(r > 0.0) || self.width == 0 || self.height == 0 {
            return;
        }
        let bgr = match self.data_format {
            RawImageFormat::RGBA8 => false,
            RawImageFormat::BGRA8 => true,
            _ => return,
        };
        let (w, h) = (self.width as i32, self.height as i32);
        let buf: &mut [u8] = match self.pixels {
            RawImageData::U8(ref mut v) => v.as_mut(),
            _ => return,
        };
        let flow = brush.flow.clamp(0.0, 1.0) * (f32::from(brush.color.a) / 255.0);
        let (cr, cg, cb) = (
            f32::from(brush.color.r),
            f32::from(brush.color.g),
            f32::from(brush.color.b),
        );
        let x0 = (cx - r).floor().max(0.0) as i32;
        let y0 = (cy - r).floor().max(0.0) as i32;
        let x1 = ((cx + r).ceil() as i32).min(w);
        let y1 = ((cy + r).ceil() as i32).min(h);
        for y in y0..y1 {
            for x in x0..x1 {
                let dx = x as f32 + 0.5 - cx;
                let dy = y as f32 + 0.5 - cy;
                let dist = dx.hypot(dy);
                if dist > r {
                    continue;
                }
                let a = brush_dab_coverage(dist / r, brush.hardness) * flow;
                if a <= 0.0 {
                    continue;
                }
                let idx = ((y * w + x) as usize) * 4;
                // `width`/`height` are public and may exceed the actual buffer;
                // trust the buffer, not the claimed dimensions, so a mismatch
                // skips the pixel instead of indexing out of bounds.
                if idx + 4 > buf.len() {
                    continue;
                }
                let (ri, gi, bi, ai) = if bgr {
                    (idx + 2, idx + 1, idx, idx + 3)
                } else {
                    (idx, idx + 1, idx + 2, idx + 3)
                };
                let inv = 1.0 - a;
                buf[ri] = (cr * a + f32::from(buf[ri]) * inv).round().clamp(0.0, 255.0) as u8;
                buf[gi] = (cg * a + f32::from(buf[gi]) * inv).round().clamp(0.0, 255.0) as u8;
                buf[bi] = (cb * a + f32::from(buf[bi]) * inv).round().clamp(0.0, 255.0) as u8;
                buf[ai] =
                    ((a + (f32::from(buf[ai]) / 255.0) * inv) * 255.0).round().clamp(0.0, 255.0) as u8;
            }
        }
    }

    /// CPU painting: stamp a stroke by spacing dabs along the segment
    /// (`x0`,`y0`)->(`x1`,`y1`). Call once per pointer move with the previous and
    /// current positions for a continuous line.
    #[allow(clippy::suboptimal_flops)] // mul_add not guaranteed faster/available without target +fma; keep explicit a*b+c
    #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)] // image/graphics: bounded pixel/colour/dimension/unit casts
    pub fn paint_stroke(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, brush: Brush) {
        let dx = x1 - x0;
        let dy = y1 - y0;
        let len = dx.hypot(dy);
        // A non-finite length (infinite / NaN endpoint) would make `n` saturate
        // to i32::MAX and the `for i in 0..=n` loop run ~2.1 billion times. Bail
        // rather than spin: an infinite segment has no finite dabs to stamp.
        if !len.is_finite() {
            return;
        }
        let step = (brush.radius * brush.spacing.max(0.01)).max(0.5);
        let n = (len / step).floor() as i32;
        if n <= 0 {
            self.paint_dot(x1, y1, brush);
            return;
        }
        for i in 0..=n {
            let t = i as f32 / n as f32;
            self.paint_dot(x0 + dx * t, y0 + dy * t, brush);
        }
    }
}

/// Multiplies the RGB channels of a single 4-byte BGRA/RGBA pixel by its alpha.
///
/// From webrender/wrench. These are slow. Gecko's gfx/2d/Swizzle.cpp has better
/// versions.
#[inline]
#[allow(clippy::cast_possible_truncation)] // image/graphics: bounded pixel/colour/dimension/unit casts
fn premultiply_alpha(array: &mut [u8]) {
    if array.len() != 4 {
        return;
    }
    let a = u32::from(array[3]);
    array[0] = (((u32::from(array[0]) * a) + 128) / 255) as u8;
    array[1] = (((u32::from(array[1]) * a) + 128) / 255) as u8;
    array[2] = (((u32::from(array[2]) * a) + 128) / 255) as u8;
}

#[inline]
#[allow(clippy::cast_possible_truncation)] // image/graphics: bounded pixel/colour/dimension/unit casts
#[allow(clippy::cast_sign_loss)] // image/graphics: bounded pixel/colour casts
fn normalize_u16(i: u16) -> u8 {
    ((f32::from(i) / f32::from(core::u16::MAX)) * f32::from(core::u8::MAX)) as u8
}

const FOUR_BPP: usize = 4;
const TWO_CHANNELS: usize = 2;
const THREE_CHANNELS: usize = 3;
const FOUR_CHANNELS: usize = 4;

impl RawImage {
    /// Returns a null / empty image
    #[must_use] pub fn null_image() -> Self {
        Self {
            pixels: RawImageData::U8(Vec::new().into()),
            width: 0,
            height: 0,
            premultiplied_alpha: true,
            data_format: RawImageFormat::BGRA8,
            tag: Vec::new().into(),
        }
    }

    /// Allocates a width * height, single-channel mask, used for drawing CPU image masks
    #[allow(clippy::cast_sign_loss)] // image/graphics: bounded pixel/colour/dimension/unit casts
    #[must_use] pub fn allocate_mask(size: LayoutSize) -> Self {
        Self {
            pixels: RawImageData::U8(
                vec![0; size.width.max(0) as usize * size.height.max(0) as usize].into(),
            ),
            width: size.width as usize,
            height: size.height as usize,
            premultiplied_alpha: true,
            data_format: RawImageFormat::R8,
            tag: Vec::new().into(),
        }
    }

    /// Encodes a `RawImage` as BGRA8 bytes and premultiplies it if the alpha is not premultiplied
    ///
    /// Returns None if the width * height * BPP does not match
    ///
    /// TODO: autovectorization fails spectacularly, need to manually optimize!
    #[must_use] pub fn into_loaded_image_source(self) -> Option<(ImageData, ImageDescriptor)> {
        let Self {
            width,
            height,
            pixels,
            data_format,
            premultiplied_alpha,
            tag,
        } = self;

        // Checked: a width*height that overflows usize is not a real image; return
        // None rather than panicking (debug) / wrapping to a bogus length (release).
        let expected_len = width.checked_mul(height)?;

        let (bytes, data_format, is_opaque): (U8Vec, RawImageFormat, bool) = match data_format {
            RawImageFormat::R8 => {
                let (bytes, is_opaque) = Self::load_r8(pixels, expected_len)?;
                (bytes, RawImageFormat::R8, is_opaque)
            }
            RawImageFormat::RG8 => {
                let (bytes, is_opaque) = Self::load_rg8(pixels, expected_len, premultiplied_alpha)?;
                (bytes, RawImageFormat::BGRA8, is_opaque)
            }
            RawImageFormat::RGB8 => {
                let (bytes, is_opaque) = Self::load_rgb8(pixels, expected_len)?;
                (bytes, RawImageFormat::BGRA8, is_opaque)
            }
            RawImageFormat::RGBA8 => {
                let (bytes, is_opaque) = Self::load_rgba8(pixels, expected_len, premultiplied_alpha)?;
                (bytes, RawImageFormat::BGRA8, is_opaque)
            }
            RawImageFormat::R16 => {
                let (bytes, is_opaque) = Self::load_r16(pixels, expected_len)?;
                (bytes, RawImageFormat::BGRA8, is_opaque)
            }
            RawImageFormat::RG16 => {
                let (bytes, is_opaque) = Self::load_rg16(pixels, expected_len)?;
                (bytes, RawImageFormat::BGRA8, is_opaque)
            }
            RawImageFormat::RGB16 => {
                let (bytes, is_opaque) = Self::load_rgb16(pixels, expected_len)?;
                (bytes, RawImageFormat::BGRA8, is_opaque)
            }
            RawImageFormat::RGBA16 => {
                let (bytes, is_opaque) =
                    Self::load_rgba16(pixels, expected_len, premultiplied_alpha)?;
                (bytes, RawImageFormat::BGRA8, is_opaque)
            }
            RawImageFormat::BGR8 => {
                let (bytes, is_opaque) = Self::load_bgr8(pixels, expected_len)?;
                (bytes, RawImageFormat::BGRA8, is_opaque)
            }
            RawImageFormat::BGRA8 => {
                let (bytes, is_opaque) = Self::load_bgra8(pixels, expected_len, premultiplied_alpha)?;
                (bytes, RawImageFormat::BGRA8, is_opaque)
            }
            RawImageFormat::RGBF32 => {
                let (bytes, is_opaque) = Self::load_rgbf32(pixels, expected_len)?;
                (bytes, RawImageFormat::BGRA8, is_opaque)
            }
            RawImageFormat::RGBAF32 => {
                let (bytes, is_opaque) =
                    Self::load_rgbaf32(pixels, expected_len, premultiplied_alpha)?;
                (bytes, RawImageFormat::BGRA8, is_opaque)
            }
        };

        let image_data = ImageData::Raw(SharedRawImageData::new(bytes));
        let image_descriptor = ImageDescriptor {
            format: data_format,
            width,
            height,
            offset: 0,
            stride: None.into(),
            flags: ImageDescriptorFlags {
                is_opaque,
                allow_mipmaps: true,
            },
        };

        Some((image_data, image_descriptor))
    }

    /// Keep R8 data as-is — `WebRender` supports R8 natively. This is important for
    /// image mask clips which need the single-channel data (white=visible,
    /// black=clipped). Stays in `R8` format; never opaque.
    fn load_r8(pixels: RawImageData, expected_len: usize) -> Option<(U8Vec, bool)> {
        let pixels = pixels.get_u8_vec()?;

        if pixels.len() != expected_len {
            return None;
        }

        Some((pixels, false))
    }

    fn load_rg8(
        pixels: RawImageData,
        expected_len: usize,
        premultiplied_alpha: bool,
    ) -> Option<(U8Vec, bool)> {
        let pixels = pixels.get_u8_vec()?;

        if pixels.len() != expected_len * TWO_CHANNELS {
            return None;
        }

        let mut is_opaque = true;
        let mut px = vec![0; expected_len * FOUR_BPP];

        // TODO: check that this function is SIMD optimized
        for (pixel_index, greyalpha) in pixels.as_ref().chunks_exact(TWO_CHANNELS).enumerate() {
            let grey = greyalpha[0];
            let alpha = greyalpha[1];

            if alpha != 255 {
                is_opaque = false;
            }

            px[pixel_index * FOUR_BPP] = grey;
            px[(pixel_index * FOUR_BPP) + 1] = grey;
            px[(pixel_index * FOUR_BPP) + 2] = grey;
            px[(pixel_index * FOUR_BPP) + 3] = alpha;

            if !premultiplied_alpha {
                premultiply_alpha(
                    &mut px[(pixel_index * FOUR_BPP)..((pixel_index * FOUR_BPP) + FOUR_BPP)],
                );
            }
        }

        Some((px.into(), is_opaque))
    }

    fn load_rgb8(pixels: RawImageData, expected_len: usize) -> Option<(U8Vec, bool)> {
        let pixels = pixels.get_u8_vec()?;

        if pixels.len() != expected_len * THREE_CHANNELS {
            return None;
        }

        let mut px = vec![0; expected_len * FOUR_BPP];

        // TODO: check that this function is SIMD optimized
        for (pixel_index, rgb) in pixels.as_ref().chunks_exact(THREE_CHANNELS).enumerate() {
            let red = rgb[0];
            let green = rgb[1];
            let blue = rgb[2];

            px[pixel_index * FOUR_BPP] = blue;
            px[(pixel_index * FOUR_BPP) + 1] = green;
            px[(pixel_index * FOUR_BPP) + 2] = red;
            px[(pixel_index * FOUR_BPP) + 3] = 0xff;
        }

        Some((px.into(), true))
    }

    fn load_rgba8(
        pixels: RawImageData,
        expected_len: usize,
        premultiplied_alpha: bool,
    ) -> Option<(U8Vec, bool)> {
        let mut pixels: Vec<u8> = pixels.get_u8_vec()?.into_library_owned_vec();

        if pixels.len() != expected_len * FOUR_CHANNELS {
            return None;
        }

        let mut is_opaque = true;

        // TODO: check that this function is SIMD optimized
        // no extra allocation necessary, but swizzling
        if premultiplied_alpha {
            for rgba in pixels.chunks_exact_mut(4) {
                let (r, gba) = rgba.split_first_mut()?;
                core::mem::swap(r, gba.get_mut(1)?);
                let a = rgba.get_mut(3)?;
                if *a != 255 {
                    is_opaque = false;
                }
            }
        } else {
            for rgba in pixels.chunks_exact_mut(4) {
                // RGBA => BGRA
                let (r, gba) = rgba.split_first_mut()?;
                core::mem::swap(r, gba.get_mut(1)?);
                let a = rgba.get_mut(3)?;
                if *a != 255 {
                    is_opaque = false;
                }
                premultiply_alpha(rgba); // <-
            }
        }

        Some((pixels.into(), is_opaque))
    }

    fn load_r16(pixels: RawImageData, expected_len: usize) -> Option<(U8Vec, bool)> {
        let pixels = pixels.get_u16_vec()?;

        if pixels.len() != expected_len {
            return None;
        }

        let mut px = vec![0; expected_len * FOUR_BPP];

        // TODO: check that this function is SIMD optimized
        for (pixel_index, grey_u16) in pixels.as_ref().iter().enumerate() {
            let grey_u8 = normalize_u16(*grey_u16);
            px[pixel_index * FOUR_BPP] = grey_u8;
            px[(pixel_index * FOUR_BPP) + 1] = grey_u8;
            px[(pixel_index * FOUR_BPP) + 2] = grey_u8;
            px[(pixel_index * FOUR_BPP) + 3] = 0xff;
        }

        Some((px.into(), true))
    }

    fn load_rg16(pixels: RawImageData, expected_len: usize) -> Option<(U8Vec, bool)> {
        let pixels = pixels.get_u16_vec()?;

        if pixels.len() != expected_len * TWO_CHANNELS {
            return None;
        }

        let mut is_opaque = true;
        let mut px = vec![0; expected_len * FOUR_BPP];

        // TODO: check that this function is SIMD optimized
        for (pixel_index, greyalpha) in pixels.as_ref().chunks_exact(TWO_CHANNELS).enumerate() {
            let grey_u8 = normalize_u16(greyalpha[0]);
            let alpha_u8 = normalize_u16(greyalpha[1]);

            if alpha_u8 != 255 {
                is_opaque = false;
            }

            px[pixel_index * FOUR_BPP] = grey_u8;
            px[(pixel_index * FOUR_BPP) + 1] = grey_u8;
            px[(pixel_index * FOUR_BPP) + 2] = grey_u8;
            px[(pixel_index * FOUR_BPP) + 3] = alpha_u8;
        }

        Some((px.into(), is_opaque))
    }

    fn load_rgb16(pixels: RawImageData, expected_len: usize) -> Option<(U8Vec, bool)> {
        let pixels = pixels.get_u16_vec()?;

        if pixels.len() != expected_len * THREE_CHANNELS {
            return None;
        }

        let mut px = vec![0; expected_len * FOUR_BPP];

        // TODO: check that this function is SIMD optimized
        for (pixel_index, rgb) in pixels.as_ref().chunks_exact(THREE_CHANNELS).enumerate() {
            let red_u8 = normalize_u16(rgb[0]);
            let green_u8 = normalize_u16(rgb[1]);
            let blue_u8 = normalize_u16(rgb[2]);

            px[pixel_index * FOUR_BPP] = blue_u8;
            px[(pixel_index * FOUR_BPP) + 1] = green_u8;
            px[(pixel_index * FOUR_BPP) + 2] = red_u8;
            px[(pixel_index * FOUR_BPP) + 3] = 0xff;
        }

        Some((px.into(), true))
    }

    fn load_rgba16(
        pixels: RawImageData,
        expected_len: usize,
        premultiplied_alpha: bool,
    ) -> Option<(U8Vec, bool)> {
        let pixels = pixels.get_u16_vec()?;

        if pixels.len() != expected_len * FOUR_CHANNELS {
            return None;
        }

        let mut is_opaque = true;
        let mut px = vec![0; expected_len * FOUR_BPP];

        // TODO: check that this function is SIMD optimized
        if premultiplied_alpha {
            for (pixel_index, rgba) in pixels.as_ref().chunks_exact(FOUR_CHANNELS).enumerate() {
                let red_u8 = normalize_u16(rgba[0]);
                let green_u8 = normalize_u16(rgba[1]);
                let blue_u8 = normalize_u16(rgba[2]);
                let alpha_u8 = normalize_u16(rgba[3]);

                if alpha_u8 != 255 {
                    is_opaque = false;
                }

                px[pixel_index * FOUR_BPP] = blue_u8;
                px[(pixel_index * FOUR_BPP) + 1] = green_u8;
                px[(pixel_index * FOUR_BPP) + 2] = red_u8;
                px[(pixel_index * FOUR_BPP) + 3] = alpha_u8;
            }
        } else {
            for (pixel_index, rgba) in pixels.as_ref().chunks_exact(FOUR_CHANNELS).enumerate() {
                let red_u8 = normalize_u16(rgba[0]);
                let green_u8 = normalize_u16(rgba[1]);
                let blue_u8 = normalize_u16(rgba[2]);
                let alpha_u8 = normalize_u16(rgba[3]);

                if alpha_u8 != 255 {
                    is_opaque = false;
                }

                px[pixel_index * FOUR_BPP] = blue_u8;
                px[(pixel_index * FOUR_BPP) + 1] = green_u8;
                px[(pixel_index * FOUR_BPP) + 2] = red_u8;
                px[(pixel_index * FOUR_BPP) + 3] = alpha_u8;
                premultiply_alpha(
                    &mut px[(pixel_index * FOUR_BPP)..((pixel_index * FOUR_BPP) + FOUR_BPP)],
                );
            }
        }

        Some((px.into(), is_opaque))
    }

    fn load_bgr8(pixels: RawImageData, expected_len: usize) -> Option<(U8Vec, bool)> {
        let pixels = pixels.get_u8_vec()?;

        if pixels.len() != expected_len * THREE_CHANNELS {
            return None;
        }

        let mut px = vec![0; expected_len * FOUR_BPP];

        // TODO: check that this function is SIMD optimized
        for (pixel_index, bgr) in pixels.as_ref().chunks_exact(THREE_CHANNELS).enumerate() {
            let blue = bgr[0];
            let green = bgr[1];
            let red = bgr[2];

            px[pixel_index * FOUR_BPP] = blue;
            px[(pixel_index * FOUR_BPP) + 1] = green;
            px[(pixel_index * FOUR_BPP) + 2] = red;
            px[(pixel_index * FOUR_BPP) + 3] = 0xff;
        }

        Some((px.into(), true))
    }

    fn load_bgra8(
        pixels: RawImageData,
        expected_len: usize,
        premultiplied_alpha: bool,
    ) -> Option<(U8Vec, bool)> {
        let mut is_opaque = true;

        let bytes: U8Vec = if premultiplied_alpha {
            // DO NOT CLONE THE IMAGE HERE!
            let pixels = pixels.get_u8_vec()?;

            if pixels.len() != expected_len * FOUR_BPP {
                return None;
            }

            is_opaque = pixels
                .as_ref()
                .chunks_exact(FOUR_CHANNELS)
                .all(|bgra| bgra[3] == 255);

            pixels
        } else {
            let mut pixels: Vec<u8> = pixels.get_u8_vec()?.into_library_owned_vec();

            if pixels.len() != expected_len * FOUR_BPP {
                return None;
            }

            for bgra in pixels.chunks_exact_mut(FOUR_CHANNELS) {
                if bgra[3] != 255 {
                    is_opaque = false;
                }
                premultiply_alpha(bgra);
            }
            pixels.into()
        };

        Some((bytes, is_opaque))
    }

    #[allow(clippy::cast_possible_truncation)] // image/graphics: bounded pixel/colour/dimension/unit casts
    #[allow(clippy::cast_sign_loss)] // image/graphics: bounded pixel/colour casts
    #[allow(clippy::needless_pass_by_value)] // owned azul value taken by value (public API / ownership-transfer convention)
    fn load_rgbf32(pixels: RawImageData, expected_len: usize) -> Option<(U8Vec, bool)> {
        let pixels = pixels.get_f32_vec_ref()?;

        if pixels.len() != expected_len * THREE_CHANNELS {
            return None;
        }

        let mut px = vec![0; expected_len * FOUR_BPP];

        // TODO: check that this function is SIMD optimized
        for (pixel_index, rgb) in pixels.as_ref().chunks_exact(THREE_CHANNELS).enumerate() {
            let red_u8 = (rgb[0] * 255.0) as u8;
            let green_u8 = (rgb[1] * 255.0) as u8;
            let blue_u8 = (rgb[2] * 255.0) as u8;

            px[pixel_index * FOUR_BPP] = blue_u8;
            px[(pixel_index * FOUR_BPP) + 1] = green_u8;
            px[(pixel_index * FOUR_BPP) + 2] = red_u8;
            px[(pixel_index * FOUR_BPP) + 3] = 0xff;
        }

        Some((px.into(), true))
    }

    #[allow(clippy::cast_possible_truncation)] // image/graphics: bounded pixel/colour/dimension/unit casts
    #[allow(clippy::cast_sign_loss)] // image/graphics: bounded pixel/colour casts
    #[allow(clippy::needless_pass_by_value)] // owned RawImageData taken by value (image decode entry point)
    fn load_rgbaf32(
        pixels: RawImageData,
        expected_len: usize,
        premultiplied_alpha: bool,
    ) -> Option<(U8Vec, bool)> {
        let pixels = pixels.get_f32_vec_ref()?;

        if pixels.len() != expected_len * FOUR_CHANNELS {
            return None;
        }

        let mut is_opaque = true;
        let mut px = vec![0; expected_len * FOUR_BPP];

        // TODO: check that this function is SIMD optimized
        if premultiplied_alpha {
            for (pixel_index, rgba) in pixels.as_ref().chunks_exact(FOUR_CHANNELS).enumerate() {
                let red_u8 = (rgba[0] * 255.0) as u8;
                let green_u8 = (rgba[1] * 255.0) as u8;
                let blue_u8 = (rgba[2] * 255.0) as u8;
                let alpha_u8 = (rgba[3] * 255.0) as u8;

                if alpha_u8 != 255 {
                    is_opaque = false;
                }

                px[pixel_index * FOUR_BPP] = blue_u8;
                px[(pixel_index * FOUR_BPP) + 1] = green_u8;
                px[(pixel_index * FOUR_BPP) + 2] = red_u8;
                px[(pixel_index * FOUR_BPP) + 3] = alpha_u8;
            }
        } else {
            for (pixel_index, rgba) in pixels.as_ref().chunks_exact(FOUR_CHANNELS).enumerate() {
                let red_u8 = (rgba[0] * 255.0) as u8;
                let green_u8 = (rgba[1] * 255.0) as u8;
                let blue_u8 = (rgba[2] * 255.0) as u8;
                let alpha_u8 = (rgba[3] * 255.0) as u8;

                if alpha_u8 != 255 {
                    is_opaque = false;
                }

                px[pixel_index * FOUR_BPP] = blue_u8;
                px[(pixel_index * FOUR_BPP) + 1] = green_u8;
                px[(pixel_index * FOUR_BPP) + 2] = red_u8;
                px[(pixel_index * FOUR_BPP) + 3] = alpha_u8;
                premultiply_alpha(
                    &mut px[(pixel_index * FOUR_BPP)..((pixel_index * FOUR_BPP) + FOUR_BPP)],
                );
            }
        }

        Some((px.into(), is_opaque))
    }
}

impl_option!(
    RawImage,
    OptionRawImage,
    copy = false,
    [Debug, Clone, PartialEq, PartialOrd]
);

#[must_use] pub fn font_size_to_au(font_size: StyleFontSize) -> Au {
    Au::from_px(font_size.inner.to_pixels_internal(0.0, DEFAULT_FONT_SIZE, DEFAULT_FONT_SIZE))
}

pub type FontInstanceFlags = u32;

// Common flags
pub const FONT_INSTANCE_FLAG_SYNTHETIC_BOLD: u32 = 1 << 1;
pub const FONT_INSTANCE_FLAG_EMBEDDED_BITMAPS: u32 = 1 << 2;
pub const FONT_INSTANCE_FLAG_SUBPIXEL_BGR: u32 = 1 << 3;
pub const FONT_INSTANCE_FLAG_TRANSPOSE: u32 = 1 << 4;
pub const FONT_INSTANCE_FLAG_FLIP_X: u32 = 1 << 5;
pub const FONT_INSTANCE_FLAG_FLIP_Y: u32 = 1 << 6;
pub const FONT_INSTANCE_FLAG_SUBPIXEL_POSITION: u32 = 1 << 7;

// Windows flags
pub const FONT_INSTANCE_FLAG_FORCE_GDI: u32 = 1 << 16;

// Mac flags
pub const FONT_INSTANCE_FLAG_FONT_SMOOTHING: u32 = 1 << 16;

// FreeType flags
pub const FONT_INSTANCE_FLAG_FORCE_AUTOHINT: u32 = 1 << 16;
pub const FONT_INSTANCE_FLAG_NO_AUTOHINT: u32 = 1 << 17;
pub const FONT_INSTANCE_FLAG_VERTICAL_LAYOUT: u32 = 1 << 18;
pub const FONT_INSTANCE_FLAG_LCD_VERTICAL: u32 = 1 << 19;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Ord, PartialOrd, Hash)]
pub struct GlyphOptions {
    pub render_mode: FontRenderMode,
    pub flags: FontInstanceFlags,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Ord, PartialOrd, Hash)]
pub enum FontRenderMode {
    Mono,
    Alpha,
    Subpixel,
}

#[cfg(target_arch = "wasm32")]
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, Ord, PartialOrd, Hash)]
pub struct FontInstancePlatformOptions {
    // empty for now
}

#[cfg(target_os = "windows")]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Ord, PartialOrd, Hash)]
pub struct FontInstancePlatformOptions {
    pub gamma: u16,
    pub contrast: u8,
    pub cleartype_level: u8,
}

#[cfg(target_os = "macos")]
#[derive(Default, Debug, Copy, Clone, PartialEq, Eq, Ord, PartialOrd, Hash)]
pub struct FontInstancePlatformOptions {
    pub unused: u32,
}

#[cfg(target_os = "linux")]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Ord, PartialOrd, Hash)]
pub struct FontInstancePlatformOptions {
    pub lcd_filter: FontLCDFilter,
    pub hinting: FontHinting,
}

// Mobile targets — empty platform-options struct keeps the
// `FontInstanceOptions { platform_options: Option<...>, .. }` field
// well-typed without inheriting Linux's freetype-specific tunables.
#[cfg(any(target_os = "android", target_os = "ios"))]
#[derive(Default, Debug, Copy, Clone, PartialEq, Eq, Ord, PartialOrd, Hash)]
pub struct FontInstancePlatformOptions {
    pub unused: u32,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Ord, PartialOrd, Hash)]
pub enum FontHinting {
    None,
    Mono,
    Light,
    Normal,
    LCD,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Ord, PartialOrd, Hash)]
#[derive(Default)]
pub enum FontLCDFilter {
    None,
    #[default]
    Default,
    Light,
    Legacy,
}


#[derive(Debug, Copy, Clone, PartialEq, Eq, Ord, PartialOrd, Hash)]
pub struct FontInstanceOptions {
    pub render_mode: FontRenderMode,
    pub flags: FontInstanceFlags,
    pub bg_color: ColorU,
    /// When `bg_color.a` is != 0 and `render_mode` is `FontRenderMode::Subpixel`,
    /// the text will be rendered with `bg_color.r/g/b` as an opaque estimated
    /// background color.
    pub synthetic_italics: SyntheticItalics,
}

impl Default for FontInstanceOptions {
    fn default() -> Self {
        Self {
            render_mode: FontRenderMode::Subpixel,
            flags: 0,
            bg_color: ColorU::TRANSPARENT,
            synthetic_italics: SyntheticItalics::default(),
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Ord, PartialOrd, Hash)]
#[derive(Default)]
pub struct SyntheticItalics {
    pub angle: i16,
}


/// Reference-counted wrapper around raw image bytes (`U8Vec`).
/// This allows sharing image data between azul-core and webrender without cloning.
///
/// Similar to `ImageRef` but specifically for raw byte data, avoiding the overhead
/// of the full `DecodedImage` enum when we just need the bytes.
#[derive(Debug)]
#[repr(C)]
pub struct SharedRawImageData {
    /// Shared pointer to the raw image bytes
    pub data: *const U8Vec,
    /// Reference counter - when it reaches 0, the data is deallocated
    pub copies: *const AtomicUsize,
    /// Whether to run the destructor (for FFI safety)
    pub run_destructor: bool,
}

impl SharedRawImageData {
    /// Create a new `SharedRawImageData` from a `U8Vec`
    #[must_use] pub fn new(data: U8Vec) -> Self {
        Self {
            data: Box::into_raw(Box::new(data)),
            copies: Box::into_raw(Box::new(AtomicUsize::new(1))),
            run_destructor: true,
        }
    }

    /// Get a reference to the underlying bytes
    #[must_use] pub fn as_ref(&self) -> &[u8] {
        // SAFETY: `data` is a non-null, live `Box<U8Vec>` owned by this handle (and its
        // clones) until the last copy drops; the borrow is tied to `&self`.
        unsafe { (*self.data).as_ref() }
    }

    /// Alias for `as_ref()` - get the raw bytes as a slice
    #[must_use] pub fn get_bytes(&self) -> &[u8] {
        self.as_ref()
    }

    /// Get a pointer to the raw bytes for hashing/identification
    #[must_use] pub fn as_ptr(&self) -> *const u8 {
        // SAFETY: `data` is a non-null, live `Box<U8Vec>` (see `as_ref`).
        unsafe { (*self.data).as_ref().as_ptr() }
    }

    /// Get the length of the data
    #[must_use] pub const fn len(&self) -> usize {
        // SAFETY: `data` is a non-null, live `Box<U8Vec>` (see `as_ref`).
        unsafe { (*self.data).len() }
    }

    /// Check if the data is empty
    #[must_use] pub const fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Try to extract the `U8Vec` if this is the only reference
    /// Returns None if there are other references
    #[must_use] pub fn into_inner(self) -> Option<U8Vec> {
        // SAFETY: `data`/`copies` are non-null heap allocations from `Box::into_raw` in
        // `new()`. When `copies == 1` we are the sole owner, so reclaiming both Boxes
        // and `forget`-ing `self` transfers ownership without a double free.
        unsafe {
            if self.copies.as_ref().map(|m| m.load(AtomicOrdering::SeqCst)) == Some(1) {
                let data = Box::from_raw(self.data.cast_mut());
                drop(Box::from_raw(self.copies.cast_mut()));
                core::mem::forget(self); // don't run the destructor
                Some(*data)
            } else {
                None
            }
        }
    }
}

// SAFETY: the raw pointers only address heap `Box`es of `Send`/`Sync` data, and all
// cross-thread refcount mutation goes through the `AtomicUsize` in `copies`.
unsafe impl Send for SharedRawImageData {}
unsafe impl Sync for SharedRawImageData {}

impl Clone for SharedRawImageData {
    fn clone(&self) -> Self {
        // SAFETY: `copies` is a non-null, live `AtomicUsize` shared by all clones; the
        // atomic increment balances the `fetch_sub` in `Drop`.
        unsafe {
            self.copies
                .as_ref()
                .map(|m| m.fetch_add(1, AtomicOrdering::SeqCst));
        }
        Self {
            data: self.data,
            copies: self.copies,
            run_destructor: true,
        }
    }
}

impl Drop for SharedRawImageData {
    fn drop(&mut self) {
        self.run_destructor = false;
        // SAFETY: `data`/`copies` are non-null, live `Box`es shared by all clones.
        // `fetch_sub` returns the pre-decrement count, so `== 1` means we are the last
        // owner; only then do we reclaim both Boxes exactly once.
        unsafe {
            let copies = (*self.copies).fetch_sub(1, AtomicOrdering::SeqCst);
            if copies == 1 {
                drop(Box::from_raw(self.data.cast_mut()));
                drop(Box::from_raw(self.copies.cast_mut()));
            }
        }
    }
}

impl PartialEq for SharedRawImageData {
    fn eq(&self, rhs: &Self) -> bool {
        core::ptr::eq(self.data, rhs.data)
    }
}

impl Eq for SharedRawImageData {}

impl PartialOrd for SharedRawImageData {
    fn partial_cmp(&self, other: &Self) -> Option<::core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for SharedRawImageData {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        (self.data as usize).cmp(&(other.data as usize))
    }
}

impl Hash for SharedRawImageData {
    fn hash<H>(&self, state: &mut H)
    where
        H: Hasher,
    {
        (self.data as usize).hash(state);
    }
}

/// Represents the backing store of an arbitrary series of pixels for display by
/// `WebRender`. This storage can take several forms.
#[derive(Debug, Clone, PartialEq, Eq, Ord, PartialOrd, Hash)]
#[repr(C, u8)]
pub enum ImageData {
    /// A simple series of bytes, provided by the embedding and owned by `WebRender`.
    /// The format is stored out-of-band, currently in `ImageDescriptor`.
    Raw(SharedRawImageData),
    /// An image owned by the embedding, and referenced by `WebRender`. This may
    /// take the form of a texture or a heap-allocated buffer.
    External(ExternalImageData),
}

/// Storage format identifier for externally-managed images.
#[derive(Debug, Copy, Clone, Eq, Hash, PartialEq, PartialOrd, Ord)]
#[repr(C, u8)]
pub enum ExternalImageType {
    /// The image is texture-backed.
    TextureHandle(ImageBufferKind),
    /// The image is heap-allocated by the embedding.
    Buffer,
}

/// An arbitrary identifier for an external image provided by the
/// application. It must be a unique identifier for each external
/// image.
#[repr(C)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, PartialOrd, Ord)]
pub struct ExternalImageId {
    pub inner: u64,
}

static LAST_EXTERNAL_IMAGE_ID: AtomicUsize = AtomicUsize::new(0);

impl Default for ExternalImageId {
    fn default() -> Self {
        Self::new()
    }
}

impl ExternalImageId {
    /// Creates a new, unique `ExternalImageId`
    pub fn new() -> Self {
        Self {
            inner: LAST_EXTERNAL_IMAGE_ID.fetch_add(1, AtomicOrdering::SeqCst) as u64,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd)]
#[repr(C, u8)]
pub enum GlyphOutlineOperation {
    MoveTo(OutlineMoveTo),
    LineTo(OutlineLineTo),
    QuadraticCurveTo(OutlineQuadTo),
    CubicCurveTo(OutlineCubicTo),
    ClosePath,
}

impl_option!(
    GlyphOutlineOperation,
    OptionGlyphOutlineOperation,
    copy = false,
    [Debug, Clone, PartialEq, Eq, PartialOrd]
);

// MoveTo in em units
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd)]
#[repr(C)]
pub struct OutlineMoveTo {
    pub x: i16,
    pub y: i16,
}

// LineTo in em units
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd)]
#[repr(C)]
pub struct OutlineLineTo {
    pub x: i16,
    pub y: i16,
}

// QuadTo in em units
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd)]
#[repr(C)]
pub struct OutlineQuadTo {
    pub ctrl_1_x: i16,
    pub ctrl_1_y: i16,
    pub end_x: i16,
    pub end_y: i16,
}

// CubicTo in em units
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd)]
#[repr(C)]
pub struct OutlineCubicTo {
    pub ctrl_1_x: i16,
    pub ctrl_1_y: i16,
    pub ctrl_2_x: i16,
    pub ctrl_2_y: i16,
    pub end_x: i16,
    pub end_y: i16,
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct GlyphOutline {
    pub operations: GlyphOutlineOperationVec,
}

azul_css::impl_vec!(GlyphOutlineOperation, GlyphOutlineOperationVec, GlyphOutlineOperationVecDestructor, GlyphOutlineOperationVecDestructorType, GlyphOutlineOperationVecSlice, OptionGlyphOutlineOperation);
azul_css::impl_vec_clone!(
    GlyphOutlineOperation,
    GlyphOutlineOperationVec,
    GlyphOutlineOperationVecDestructor
);
azul_css::impl_vec_debug!(GlyphOutlineOperation, GlyphOutlineOperationVec);
azul_css::impl_vec_partialord!(GlyphOutlineOperation, GlyphOutlineOperationVec);
azul_css::impl_vec_partialeq!(GlyphOutlineOperation, GlyphOutlineOperationVec);

#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct OwnedGlyphBoundingBox {
    pub max_x: i16,
    pub max_y: i16,
    pub min_x: i16,
    pub min_y: i16,
}

/// Specifies the type of texture target in driver terms.
#[derive(Copy, Clone, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
#[repr(C)]
pub enum ImageBufferKind {
    /// Standard texture. This maps to `GL_TEXTURE_2D` in OpenGL.
    Texture2D = 0,
    /// Rectangle texture. This maps to `GL_TEXTURE_RECTANGLE` in OpenGL. This
    /// is similar to a standard texture, with a few subtle differences
    /// (no mipmaps, non-power-of-two dimensions, different coordinate space)
    /// that make it useful for representing the kinds of textures we use
    /// in `WebRender`. See <https://www.khronos.org/opengl/wiki/Rectangle_Texture>
    /// for background on Rectangle textures.
    TextureRect = 1,
    /// External texture. This maps to `GL_TEXTURE_EXTERNAL_OES` in OpenGL, which
    /// is an extension. This is used for image formats that OpenGL doesn't
    /// understand, particularly YUV. See
    /// <https://www.khronos.org/registry/OpenGL/extensions/OES/OES_EGL_image_external.txt>
    TextureExternal = 2,
}

/// Descriptor for external image resources. See `ImageData`.
#[repr(C)]
#[derive(Debug, Clone, Copy, Eq, Hash, PartialEq, PartialOrd, Ord)]
pub struct ExternalImageData {
    /// The identifier of this external image, provided by the embedding.
    pub id: ExternalImageId,
    /// For multi-plane images (i.e. YUV), indicates the plane of the
    /// original image that this struct represents. 0 for single-plane images.
    pub channel_index: u8,
    /// Storage format identifier.
    pub image_type: ExternalImageType,
}

pub type TileSize = u16;

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd)]
pub enum ImageDirtyRect {
    All,
    Partial(LayoutRect),
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub enum ResourceUpdate {
    AddFont(AddFont),
    DeleteFont(FontKey),
    AddFontInstance(AddFontInstance),
    DeleteFontInstance(FontInstanceKey),
    AddImage(AddImage),
    UpdateImage(UpdateImage),
    DeleteImage(ImageKey),
}

#[derive(Debug, Clone, PartialEq, Eq, Ord, PartialOrd, Hash)]
pub struct AddImage {
    pub key: ImageKey,
    pub descriptor: ImageDescriptor,
    pub data: ImageData,
    pub tiling: Option<TileSize>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd)]
pub struct UpdateImage {
    pub key: ImageKey,
    pub descriptor: ImageDescriptor,
    pub data: ImageData,
    pub dirty_rect: ImageDirtyRect,
}

/// Message to add a font to `WebRender`.
/// Contains a reference to the parsed font data.
#[derive(Clone, PartialEq, Eq, Ord, PartialOrd, Hash)]
pub struct AddFont {
    pub key: FontKey,
    pub font: FontRef,
}

impl fmt::Debug for AddFont {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "AddFont {{ key: {:?}, font: {:?} }}",
            self.key, self.font
        )
    }
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub struct AddFontInstance {
    pub key: FontInstanceKey,
    pub font_key: FontKey,
    pub glyph_size: (Au, DpiScaleFactor),
    pub options: Option<FontInstanceOptions>,
    pub platform_options: Option<FontInstancePlatformOptions>,
    pub variations: Vec<FontVariation>,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialOrd, PartialEq)]
pub struct FontVariation {
    pub tag: u32,
    pub value: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Epoch {
    inner: u32,
}

impl fmt::Display for Epoch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.inner)
    }
}

impl Default for Epoch {
    fn default() -> Self {
        Self::new()
    }
}

impl Epoch {
    // prevent raw access to the .inner field so that
    // you can grep the codebase for .increment() to see
    // exactly where the epoch is being incremented
    #[must_use] pub const fn new() -> Self {
        Self { inner: 0 }
    }
    #[must_use] pub const fn from(i: u32) -> Self {
        Self { inner: i }
    }
    #[must_use] pub const fn into_u32(&self) -> u32 {
        self.inner
    }

    // We don't want the epoch to increase to u32::MAX, since
    // u32::MAX represents an invalid epoch, which could confuse webrender
    pub const fn increment(&mut self) {
        use core::u32;
        const MAX_ID: u32 = u32::MAX - 1;
        *self = match self.inner {
            MAX_ID => Self { inner: 0 },
            other => Self {
                inner: other.saturating_add(1),
            },
        };
    }
}

// App units that this font instance was registered for
#[derive(Debug, Clone, Copy, Hash, PartialEq, PartialOrd, Eq, Ord)]
pub struct Au(pub i32);

pub const AU_PER_PX: i32 = 60;
pub const MAX_AU: i32 = (1 << 30) - 1;
pub const MIN_AU: i32 = -(1 << 30) - 1;

impl Au {
    #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)] // image/graphics: bounded pixel/colour/dimension/unit casts
    #[must_use] pub fn from_px(px: f32) -> Self {
        let target_app_units = (px * AU_PER_PX as f32) as i32;
        Self(target_app_units.clamp(MIN_AU, MAX_AU))
    }
    #[allow(clippy::cast_precision_loss)] // image/graphics: bounded pixel/colour/dimension/unit casts
    #[must_use] pub fn into_px(&self) -> f32 {
        self.0 as f32 / AU_PER_PX as f32
    }
}

// Debug, PartialEq, Eq, PartialOrd, Ord
#[derive(Debug)]
pub enum AddFontMsg {
    // add font: font key, font bytes + font index
    Font(FontKey, StyleFontFamilyHash, FontRef),
    Instance(AddFontInstance, (Au, DpiScaleFactor)),
}

impl AddFontMsg {
    #[must_use] pub fn into_resource_update(&self) -> ResourceUpdate {
        use self::AddFontMsg::{Font, Instance};
        match self {
            Font(font_key, _, font_ref) => ResourceUpdate::AddFont(AddFont {
                key: *font_key,
                font: font_ref.clone(),
            }),
            Instance(fi, _) => ResourceUpdate::AddFontInstance(fi.clone()),
        }
    }
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub enum DeleteFontMsg {
    Font(FontKey),
    Instance(FontInstanceKey, (Au, DpiScaleFactor)),
}

impl DeleteFontMsg {
    #[must_use] pub const fn into_resource_update(&self) -> ResourceUpdate {
        use self::DeleteFontMsg::{Font, Instance};
        match self {
            Font(f) => ResourceUpdate::DeleteFont(*f),
            Instance(fi, _) => ResourceUpdate::DeleteFontInstance(*fi),
        }
    }
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub struct AddImageMsg(pub AddImage);

impl AddImageMsg {
    #[must_use] pub fn into_resource_update(&self) -> ResourceUpdate {
        ResourceUpdate::AddImage(self.0.clone())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[repr(C)]
pub struct LoadedFontSource {
    pub data: U8Vec,
    pub index: u32,
    pub load_outlines: bool,
}

// function to load the font source from a file
pub type LoadFontFn = fn(&StyleFontFamily, &FcFontCache) -> Option<LoadedFontSource>;

// function to parse the font given the loaded font source
pub type ParseFontFn = fn(LoadedFontSource) -> Option<FontRef>; // = Option<Box<azul_text_layout::Font>>

pub type GlStoreImageFn = fn(DocumentId, Epoch, Texture, ExternalImageId);

/// Compute the deterministic `ExternalImageId` that the OpenGL texture cache uses
/// for a texture bound to a specific DOM node.
///
/// The same `(DomId, NodeId)` always
/// maps to the same `ExternalImageId`, so cached display lists keep working across
/// frames.
#[must_use] pub fn texture_external_image_id(dom_id: DomId, node_id: NodeId) -> ExternalImageId {
    let dom = dom_id.inner as u64;
    let node = node_id.index() as u64;
    debug_assert!(u32::try_from(dom).is_ok(), "DomId exceeds 32-bit range");
    debug_assert!(u32::try_from(node).is_ok(), "NodeId exceeds 32-bit range");
    ExternalImageId {
        inner: (dom << 32) | (node & 0xFFFF_FFFF),
    }
}

/// Compute the `ExternalImageId` for a static GL texture identified by its
/// `ImageRefHash`. Mirrors `image_ref_hash_to_image_key` so a given image hash
/// produces the same identifiers everywhere.
#[must_use] pub const fn image_ref_hash_to_external_image_id(hash: ImageRefHash) -> ExternalImageId {
    ExternalImageId {
        inner: hash.inner,
    }
}

/// Given the fonts of the current frame, returns `AddFont` and `AddFontInstance`s of
/// which fonts / instances are currently not in the `current_registered_fonts` and
/// need to be added.
///
/// Deleting fonts can only be done after the entire frame has finished drawing,
/// otherwise (if removing fonts would happen after every DOM) we'd constantly
/// add-and-remove fonts after every `VirtualViewCallback`, which would cause a lot of
/// I/O waiting.
#[allow(clippy::too_many_lines)] // large but cohesive: single-purpose parser/builder/dispatch (one branch per input variant)
pub fn build_add_font_resource_updates(
    renderer_resources: &mut RendererResources,
    dpi: DpiScaleFactor,
    fc_cache: &FcFontCache,
    id_namespace: IdNamespace,
    fonts_in_dom: &OrderedMap<ImmediateFontId, FastBTreeSet<Au>>,
    font_source_load_fn: LoadFontFn,
    parse_font_fn: ParseFontFn,
) -> Vec<(StyleFontFamilyHash, AddFontMsg)> {
    let mut resource_updates = Vec::new();
    let mut font_instances_added_this_frame = FastBTreeSet::new();

    'outer: for (im_font_id, font_sizes) in fonts_in_dom {
        macro_rules! insert_font_instances {
            ($font_family_hash:expr, $font_key:expr, $font_size:expr) => {{
                let font_instance_key_exists = renderer_resources
                    .currently_registered_fonts
                    .get(&$font_key)
                    .and_then(|(_, font_instances)| font_instances.get(&($font_size, dpi)))
                    .is_some()
                    || font_instances_added_this_frame.contains(&($font_key, ($font_size, dpi)));

                if !font_instance_key_exists {
                    let font_instance_key = FontInstanceKey::unique(id_namespace);

                    // For some reason the gamma is way to low on Windows
                    #[cfg(target_os = "windows")]
                    let platform_options = FontInstancePlatformOptions {
                        gamma: 300,
                        contrast: 100,
                        cleartype_level: 100,
                    };

                    #[cfg(target_os = "linux")]
                    let platform_options = FontInstancePlatformOptions {
                        lcd_filter: FontLCDFilter::Default,
                        hinting: FontHinting::Normal,
                    };

                    #[cfg(target_os = "macos")]
                    let platform_options = FontInstancePlatformOptions::default();

                    #[cfg(target_arch = "wasm32")]
                    let platform_options = FontInstancePlatformOptions::default();

                    #[cfg(any(target_os = "android", target_os = "ios"))]
                    let platform_options = FontInstancePlatformOptions::default();

                    let options = FontInstanceOptions {
                        render_mode: FontRenderMode::Subpixel,
                        flags: FONT_INSTANCE_FLAG_NO_AUTOHINT,
                        ..Default::default()
                    };

                    font_instances_added_this_frame.insert(($font_key, ($font_size, dpi)));
                    resource_updates.push((
                        $font_family_hash,
                        AddFontMsg::Instance(
                            AddFontInstance {
                                key: font_instance_key,
                                font_key: $font_key,
                                glyph_size: ($font_size, dpi),
                                options: Some(options),
                                platform_options: Some(platform_options),
                                variations: alloc::vec::Vec::new(),
                            },
                            ($font_size, dpi),
                        ),
                    ));
                }
            }};
        }

        match im_font_id {
            ImmediateFontId::Resolved((font_family_hash, font_id)) => {
                // nothing to do, font is already added,
                // just insert the missing font instances
                for font_size in font_sizes {
                    insert_font_instances!(*font_family_hash, *font_id, *font_size);
                }
            }
            ImmediateFontId::Unresolved(style_font_families) => {
                // If the font is already loaded during the current frame,
                // do not attempt to load it again
                //
                // This prevents duplicated loading for fonts in different orders, i.e.
                // - vec!["Times New Roman", "serif"] and
                // - vec!["sans", "Times New Roman"]
                // ... will resolve to the same font instead of creating two fonts

                // If there is no font key, that means there's also no font instances
                let mut font_family_hash = None;
                let font_families_hash = StyleFontFamiliesHash::new(style_font_families.as_ref());

                // Find the first font that can be loaded and parsed
                'inner: for family in style_font_families.as_ref() {
                    let current_family_hash = StyleFontFamilyHash::new(family);

                    if let Some(font_id) = renderer_resources.font_id_map.get(&current_family_hash)
                    {
                        // font key already exists
                        for font_size in font_sizes {
                            insert_font_instances!(current_family_hash, *font_id, *font_size);
                        }
                        continue 'outer;
                    }

                    let font_ref = match family {
                        StyleFontFamily::Ref(r) => r.clone(), // Clone the FontRef
                        other => {
                            // Load and parse the font
                            let Some(font_data) = (font_source_load_fn)(other, fc_cache) else {
                                continue 'inner;
                            };

                            

                            match (parse_font_fn)(font_data) {
                                Some(s) => s,
                                None => continue 'inner,
                            }
                        }
                    };

                    // font loaded properly
                    font_family_hash = Some((current_family_hash, font_ref));
                    break 'inner;
                }

                let (font_family_hash, font_ref) = match font_family_hash {
                    None => continue 'outer, // No font could be loaded, try again next frame
                    Some(s) => s,
                };

                // Generate a new font key, store the mapping between hash and font key
                let font_key = FontKey::unique(id_namespace);
                let add_font_msg = AddFontMsg::Font(font_key, font_family_hash, font_ref);

                renderer_resources
                    .font_id_map
                    .insert(font_family_hash, font_key);
                renderer_resources
                    .font_families_map
                    .insert(font_families_hash, font_family_hash);
                resource_updates.push((font_family_hash, add_font_msg));

                // Insert font sizes for the newly generated font key
                for font_size in font_sizes {
                    insert_font_instances!(font_family_hash, font_key, *font_size);
                }
            }
        }
    }

    resource_updates
}

/// Given the images of the current frame, returns `AddImage`s of
/// which image keys are currently not in the `current_registered_images` and
/// need to be added.
///
/// Returns Vec<(`ImageRefHash`, `AddImageMsg`)> where:
/// - `ImageRefHash`: Stable hash of the `ImageRef` pointer
/// - `AddImageMsg`: Message to add the image to `WebRender`
///
/// The `ImageKey` in `AddImageMsg` is generated directly from the `ImageRefHash` using
/// `image_ref_hash_to_image_key()`, so no separate mapping table is needed.
///
/// Deleting images can only be done after the entire frame has finished drawing,
/// otherwise (if removing images would happen after every DOM) we'd constantly
/// add-and-remove images after every `VirtualViewCallback`, which would cause a lot of
/// I/O waiting.
#[allow(unused_variables)]
pub fn build_add_image_resource_updates(
    renderer_resources: &RendererResources,
    id_namespace: IdNamespace,
    epoch: Epoch,
    document_id: &DocumentId,
    images_in_dom: &FastBTreeSet<ImageRef>,
    insert_into_active_gl_textures: GlStoreImageFn,
) -> Vec<(ImageRefHash, AddImageMsg)> {
    images_in_dom
        .iter()
        .filter_map(|image_ref| {
            let image_ref_hash = image_ref_get_hash(image_ref);

            if renderer_resources
                .currently_registered_images
                .contains_key(&image_ref_hash)
            {
                return None;
            }

            // NOTE: The image_ref.clone() is a shallow clone,
            // does not actually clone the data
            match image_ref.get_data() {
                DecodedImage::Gl(texture) => {
                    let descriptor = texture.get_descriptor();
                    let key = image_ref_hash_to_image_key(image_ref_hash, id_namespace);
                    // The ExternalImageId is derived from the same stable hash that
                    // produces the ImageKey, so the GL texture cache and WebRender
                    // agree on a single identifier for this texture.
                    let external_image_id = image_ref_hash_to_external_image_id(image_ref_hash);
                    // NOTE: The texture is not really cloned here,
                    (insert_into_active_gl_textures)(
                        *document_id,
                        epoch,
                        texture.clone(),
                        external_image_id,
                    );
                    Some((
                        image_ref_hash,
                        AddImageMsg(AddImage {
                            key,
                            data: ImageData::External(ExternalImageData {
                                id: external_image_id,
                                channel_index: 0,
                                image_type: ExternalImageType::TextureHandle(
                                    ImageBufferKind::Texture2D,
                                ),
                            }),
                            descriptor,
                            tiling: None,
                        }),
                    ))
                }
                DecodedImage::Raw((descriptor, data)) => {
                    let key = image_ref_hash_to_image_key(image_ref_hash, id_namespace);
                    Some((
                        image_ref_hash,
                        AddImageMsg(AddImage {
                            key,
                            data: data.clone(), // deep-copy except in the &'static case
                            descriptor: *descriptor, /* deep-copy, but struct is not very
                                                 * large */
                            tiling: None,
                        }),
                    ))
                }
                // NullImage has nothing to upload; texture callbacks are handled after
                // layout is done.
                DecodedImage::NullImage { .. } | DecodedImage::Callback(_) => None,
            }
        })
        .collect()
}

/// Submits the `AddFont`, `AddFontInstance` and `AddImage` resources to the `RenderApi`.
///
/// Extends `currently_registered_images` and `currently_registered_fonts` by the
/// `last_frame_image_keys` and `last_frame_font_keys`, so that we don't lose track of
/// what font and image keys are currently in the API.
#[allow(clippy::needless_pass_by_value)] // owned azul value taken by value (public API / ownership-transfer convention)
pub fn add_resources(
    renderer_resources: &mut RendererResources,
    all_resource_updates: &mut Vec<ResourceUpdate>,
    add_font_resources: Vec<(StyleFontFamilyHash, AddFontMsg)>,
    add_image_resources: Vec<(ImageRefHash, AddImageMsg)>,
) {
    all_resource_updates.extend(
        add_font_resources
            .iter()
            .map(|(_, f)| f.into_resource_update()),
    );
    all_resource_updates.extend(
        add_image_resources
            .iter()
            .map(|(_, i)| i.into_resource_update()),
    );

    for (image_ref_hash, add_image_msg) in &add_image_resources {
        renderer_resources.currently_registered_images.insert(
            *image_ref_hash,
            ResolvedImage {
                key: add_image_msg.0.key,
                descriptor: add_image_msg.0.descriptor,
            },
        );
        // Keep the reverse lookup (`ImageKey` -> `ImageRefHash`) in sync with the
        // forward map so display-list translation can resolve keys back to hashes.
        renderer_resources
            .image_key_map
            .insert(add_image_msg.0.key, *image_ref_hash);
    }

    for (_, add_font_msg) in add_font_resources {
        use self::AddFontMsg::{Font, Instance};
        match add_font_msg {
            Font(fk, font_family_hash, font_ref) => {
                renderer_resources
                    .currently_registered_fonts
                    .entry(fk)
                    .or_insert_with(|| (font_ref.clone(), OrderedMap::default()));

                // CRITICAL: Map font_hash to FontKey so we can look it up during rendering
                renderer_resources
                    .font_hash_map
                    .insert(font_ref.get_hash(), fk);
            }
            Instance(fi, size) => {
                if let Some((_, instances)) = renderer_resources
                    .currently_registered_fonts
                    .get_mut(&fi.font_key)
                {
                    instances.insert(size, fi.key);
                }
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::items_after_statements, clippy::redundant_clone, clippy::cast_possible_truncation, clippy::cast_sign_loss, trivial_casts, clippy::borrow_as_ptr, clippy::cast_ptr_alignment, clippy::unused_self, unused_qualifications, unreachable_pub, private_interfaces)] // pedantic lints are noise in unsafe-exercising test code
mod tests {
    use super::*;

    #[test]
    fn normalize_u16_maps_full_range() {
        // 0 -> 0, u16::MAX -> u8::MAX, midpoint -> ~127/128, no div-by-zero.
        assert_eq!(normalize_u16(0), 0);
        assert_eq!(normalize_u16(u16::MAX), 255);
        // Half of u16::MAX should land at ~half of u8::MAX.
        let mid = normalize_u16(u16::MAX / 2);
        assert!((126..=128).contains(&mid), "midpoint normalized to {mid}");
        // Previously `(65535/i)*255` produced near-white garbage for small i;
        // a small input must now map to a small output.
        assert_eq!(normalize_u16(256), 0);
        assert_eq!(normalize_u16(257), 1);
    }

    #[test]
    fn load_bgra8_rejects_wrong_length() {
        // premultiplied branch: buffer shorter than expected must be rejected,
        // not silently accepted (previously missing length guard).
        let short = RawImageData::U8(vec![0u8; 4 * 3].into()); // 3 px worth
        assert!(RawImage::load_bgra8(short, 4, true).is_none());

        // correct length is accepted.
        let ok = RawImageData::U8(vec![255u8; 4 * 4].into()); // 4 px
        assert!(RawImage::load_bgra8(ok, 4, true).is_some());

        // non-premultiplied branch still rejects wrong length.
        let short2 = RawImageData::U8(vec![0u8; 4 * 2].into());
        assert!(RawImage::load_bgra8(short2, 4, false).is_none());
    }

    // --- unsafe-hardening tests (Miri-compatible: pure in-memory, no FFI/GL) ---

    #[test]
    fn imageref_get_data_reads_backing_box() {
        // Exercises the `&*self.data` raw-pointer deref in `get_data`.
        let img = ImageRef::null_image(2, 3, RawImageFormat::RGBA8, vec![7, 8]);
        match img.get_data() {
            DecodedImage::NullImage { width, height, tag, .. } => {
                assert_eq!((*width, *height), (2, 3));
                assert_eq!(tag.as_slice(), &[7, 8]);
            }
            _ => panic!("expected NullImage"),
        }
    }

    #[test]
    fn imageref_clone_shares_refcount_and_identity() {
        // Clone must bump the shared AtomicUsize (so `into_inner` refuses while a
        // second copy is alive) and preserve the never-reused identity `id`.
        let img = ImageRef::null_image(1, 1, RawImageFormat::R8, Vec::new());
        let c = img.clone();
        assert_eq!(img, c); // same id -> shallow clone
        // Two live copies: sole-owner extraction must fail (and `c` drops cleanly).
        assert!(c.into_inner().is_none());
        // Back to one owner: extraction now succeeds, forgetting `self` without leak.
        assert!(img.into_inner().is_some());
    }

    #[test]
    fn imageref_deep_copy_has_distinct_identity() {
        // deep_copy allocates a fresh backing Box + fresh id -> not equal, independent
        // drop (Miri would flag any shared/double-freed allocation here).
        let img = ImageRef::null_image(4, 4, RawImageFormat::RGBA8, vec![1]);
        let d = img.deep_copy();
        assert_ne!(img, d);
        drop(img);
        // `d` still valid and independently readable after `img` freed.
        assert_eq!(d.get_size().width as usize, 4);
    }

    #[test]
    fn imageref_last_drop_frees_once() {
        // Clone then drop both: the refcount path must free the two Boxes exactly once
        // on the final drop. Under Miri a double free / leak fails the test.
        let img = ImageRef::null_image(1, 1, RawImageFormat::R8, Vec::new());
        let c = img.clone();
        drop(img);
        drop(c);
    }

    #[test]
    fn imageref_get_callback_none_for_non_callback_and_when_shared() {
        // `get_image_callback` derefs both `copies` and `data`; a NullImage yields None,
        // and a shared (copies != 1) handle also yields None.
        let img = ImageRef::null_image(1, 1, RawImageFormat::R8, Vec::new());
        assert!(img.get_image_callback().is_none());
        let c = img.clone();
        assert!(img.get_image_callback().is_none()); // shared -> not safe
        drop(c);
    }

    #[test]
    fn shared_raw_image_data_read_paths() {
        // Exercises as_ref / len / is_empty / as_ptr raw-pointer derefs.
        let s = SharedRawImageData::new(vec![10u8, 20, 30].into());
        assert_eq!(s.as_ref(), &[10, 20, 30]);
        assert_eq!(s.len(), 3);
        assert!(!s.is_empty());
        assert_eq!(unsafe { *s.as_ptr() }, 10);
        assert!(SharedRawImageData::new(Vec::<u8>::new().into()).is_empty());
    }

    #[test]
    fn shared_raw_image_data_clone_shares_alloc() {
        // Clone shares the backing Box (ptr-equal) and refcount; `into_inner` refuses
        // while a second copy lives, and succeeds once sole owner.
        let s = SharedRawImageData::new(vec![1u8, 2, 3, 4].into());
        let c = s.clone();
        assert_eq!(s, c); // ptr::eq on the shared `data`
        assert_eq!(s.as_ptr(), c.as_ptr());
        assert!(c.into_inner().is_none()); // two owners -> None, `c` drops to refcount 1
        let inner = s.into_inner().expect("sole owner extraction");
        assert_eq!(inner.as_ref(), &[1, 2, 3, 4]);
    }

    #[test]
    fn shared_raw_image_data_last_drop_frees_once() {
        // Refcounted drop must free both Boxes exactly once; Miri flags UB otherwise.
        let s = SharedRawImageData::new(vec![0u8; 8].into());
        let c = s.clone();
        drop(s);
        drop(c);
    }
}

#[cfg(test)]
#[allow(
    clippy::float_cmp,
    clippy::items_after_statements,
    clippy::redundant_clone,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::cast_lossless,
    clippy::unreadable_literal,
    clippy::too_many_lines,
    clippy::many_single_char_names,
    clippy::similar_names,
    unused_qualifications,
    unreachable_pub,
    private_interfaces
)] // pedantic lints are noise in adversarial test code
mod autotest_generated {
    use alloc::string::String;

    use super::*;

    // ---------------------------------------------------------------------
    // helpers
    // ---------------------------------------------------------------------

    /// A `FontRef` whose `parsed` pointer addresses a `'static` byte and whose
    /// destructor is a no-op, so nothing is freed on drop. Sound because
    /// `FontRef`'s `Hash`/`get_hash` only read the never-reused `id` and never
    /// dereference `parsed`.
    fn dummy_font_ref() -> FontRef {
        static DUMMY_FONT_DATA: u8 = 0;
        extern "C" fn dummy_destructor(_: *mut core::ffi::c_void) {}
        FontRef::new(
            core::ptr::addr_of!(DUMMY_FONT_DATA).cast::<core::ffi::c_void>(),
            dummy_destructor,
        )
    }

    /// `LoadFontFn` that never resolves a font (simulates a missing font file).
    fn load_font_none(_: &StyleFontFamily, _: &FcFontCache) -> Option<LoadedFontSource> {
        None
    }

    /// `ParseFontFn` that never parses (simulates a corrupt font file).
    fn parse_font_none(_: LoadedFontSource) -> Option<FontRef> {
        None
    }

    /// `GlStoreImageFn` no-op: never invoked for raw / null / callback images.
    fn store_gl_texture_noop(_: DocumentId, _: Epoch, _: Texture, _: ExternalImageId) {}

    fn test_document_id() -> DocumentId {
        DocumentId {
            namespace_id: IdNamespace(7),
            id: 0,
        }
    }

    /// An `RGBA8` image of `w * h` transparent-black pixels.
    fn rgba8_image(w: usize, h: usize) -> RawImage {
        RawImage {
            pixels: RawImageData::U8(vec![0u8; w * h * 4].into()),
            width: w,
            height: h,
            premultiplied_alpha: true,
            data_format: RawImageFormat::RGBA8,
            tag: Vec::new().into(),
        }
    }

    fn opaque_red() -> ColorU {
        ColorU {
            r: 255,
            g: 0,
            b: 0,
            a: 255,
        }
    }

    // =====================================================================
    // PARSERS: match_route / RouteMatch::get_param / AppConfig::match_route_for_path
    // =====================================================================

    #[test]
    fn match_route_valid_minimal_positive_control() {
        // Documented examples must hold (positive control).
        let m = match_route("/user/:id", "/user/42").expect("documented example must match");
        assert_eq!(m.pattern.as_str(), "/user/:id");
        assert_eq!(m.get_param("id").map(AzString::as_str), Some("42"));

        let root = match_route("/", "/").expect("root must match root");
        assert!(root.params.as_ref().is_empty());

        assert!(match_route("/about", "/settings").is_none());
    }

    #[test]
    fn match_route_empty_input_does_not_panic() {
        // Empty pattern/path degrade to zero segments; the segment-count check
        // makes "" and "/" equivalent (both filter to no segments).
        let m = match_route("", "").expect("empty vs empty is a zero-segment match");
        assert!(m.params.as_ref().is_empty());
        assert!(match_route("", "/").is_some()); // "/" also has zero segments
        assert!(match_route("/", "").is_some());
        assert!(match_route("", "/a").is_none()); // 0 segments != 1 segment
        assert!(match_route("/a", "").is_none());
    }

    #[test]
    fn match_route_whitespace_only_is_not_trimmed() {
        // Whitespace is NOT trimmed: it is an ordinary (opaque) path segment,
        // so it only matches itself. Deterministic, no panic.
        assert!(match_route("   ", "   ").is_some());
        assert!(match_route("   ", "\t\n").is_none());
        assert!(match_route("/ ", "/").is_none()); // " " is a real segment
        assert!(match_route("/\t\n", "/\t\n").is_some());
    }

    #[test]
    fn match_route_garbage_never_panics() {
        for pat in [
            "\0\0\0",
            "///////",
            "::::",
            ":",
            "%%%$#@!",
            "\u{feff}",
            "/a/../../etc/passwd",
        ] {
            for path in ["", "/", "\0", "/a/b/c", "%%%$#@!", "\u{feff}"] {
                // Only requirement: a total function that never panics.
                let _ = match_route(pat, path);
            }
        }
        // "///////" collapses to zero segments, so it matches the root.
        assert!(match_route("///////", "/").is_some());
        // A bare ":" is a param with an EMPTY name; the value is still captured.
        let m = match_route("/:", "/hello").expect("empty param name still matches");
        assert_eq!(m.get_param("").map(AzString::as_str), Some("hello"));
    }

    #[test]
    fn match_route_leading_trailing_junk_is_rejected_or_ignored() {
        // Trailing slashes produce empty segments that are filtered out, so a
        // trailing slash is ignored (deterministic).
        assert!(match_route("/user/:id/", "/user/42").is_some());
        assert!(match_route("/user/:id", "/user/42/").is_some());
        // Surrounding spaces are part of the segment -> rejected.
        assert!(match_route(" /about ", "/about").is_none());
        assert!(match_route("/about", "/about;garbage").is_none());
    }

    #[test]
    fn match_route_boundary_number_strings_are_opaque_segments() {
        // Numeric-looking params are never parsed; they round-trip verbatim.
        for v in [
            "0",
            "-0",
            "9223372036854775807",
            "-9223372036854775808",
            "1e400",
            "NaN",
            "inf",
            "-inf",
            "0.0000000000000000001",
        ] {
            let path = String::from("/user/") + v;
            let m = match_route("/user/:id", &path).expect("any segment matches a :param");
            assert_eq!(m.get_param("id").map(AzString::as_str), Some(v));
        }
    }

    #[test]
    fn match_route_unicode_multibyte_does_not_panic() {
        // Splitting on '/' is byte-safe for UTF-8; multibyte segments survive.
        let m = match_route("/user/:id", "/user/\u{1F600}").expect("emoji segment matches");
        assert_eq!(m.get_param("id").map(AzString::as_str), Some("\u{1F600}"));

        // Combining marks + RTL + a unicode param NAME.
        let m = match_route("/:é\u{0301}", "/e\u{0301}\u{202E}x").expect("unicode param name");
        assert_eq!(
            m.get_param("é\u{0301}").map(AzString::as_str),
            Some("e\u{0301}\u{202E}x")
        );
        // A unicode segment does not equal its NFC/NFD-different twin.
        assert!(match_route("/é", "/e\u{0301}").is_none());
    }

    #[test]
    fn match_route_extremely_long_input_does_not_hang() {
        // 1M-char single segment: linear split, no quadratic blowup / panic.
        let huge = String::from("/") + &"a".repeat(1_000_000);
        assert!(match_route("/x", &huge).is_none());
        let m = match_route("/:id", &huge).expect("one long segment is still one segment");
        assert_eq!(m.get_param("id").map(|s| s.as_str().len()), Some(1_000_000));
    }

    #[test]
    fn match_route_deeply_nested_input_does_not_stack_overflow() {
        // match_route is iterative: 10k segments and 10k nested brackets are fine.
        let deep = "/a".repeat(10_000);
        let m = match_route(&deep, &deep).expect("identical deep paths match");
        assert!(m.params.as_ref().is_empty());

        let all_params = "/:p".repeat(10_000);
        let m = match_route(&all_params, &deep).expect("10k params extract");
        assert_eq!(m.params.as_ref().len(), 10_000);
        // Duplicate keys: get_param returns the FIRST binding.
        assert_eq!(m.get_param("p").map(AzString::as_str), Some("a"));

        let brackets = String::from("/") + &"[".repeat(10_000);
        assert!(match_route("/:x", &brackets).is_some());
    }

    #[test]
    fn match_route_segment_count_mismatch_is_none() {
        assert!(match_route("/a/:b", "/a").is_none());
        assert!(match_route("/a", "/a/b").is_none());
        assert!(match_route("/:a/:b/:c", "/1/2").is_none());
    }

    #[test]
    fn route_match_get_param_missing_keys_return_none() {
        let empty = RouteMatch {
            pattern: AzString::from_const_str("/"),
            params: StringPairVec::from_vec(Vec::new()),
        };
        // Empty / whitespace / garbage / unicode / huge keys: None, never a panic.
        assert!(empty.get_param("").is_none());
        assert!(empty.get_param("   ").is_none());
        assert!(empty.get_param("\t\n").is_none());
        assert!(empty.get_param("\u{1F600}").is_none());
        assert!(empty.get_param("\0").is_none());
        assert!(empty.get_param(&"k".repeat(100_000)).is_none());

        // Positive control + near-miss keys on a populated match.
        let m = match_route("/u/:id", "/u/7").expect("valid");
        assert_eq!(m.get_param("id").map(AzString::as_str), Some("7"));
        assert!(m.get_param("ID").is_none()); // case-sensitive
        assert!(m.get_param("i").is_none()); // no prefix matching
        assert!(m.get_param(":id").is_none()); // the ':' is stripped from the key
    }

    #[test]
    fn app_config_match_route_for_path_adversarial_inputs() {
        let mut config = AppConfig::create();
        let cb: crate::callbacks::LayoutCallbackType = autotest_layout;
        extern "C" fn autotest_layout(
            _: RefAny,
            _: crate::callbacks::LayoutCallbackInfo,
        ) -> crate::dom::Dom {
            crate::dom::Dom::create_body()
        }
        config.add_route(AzString::from_const_str("/"), cb);
        config.add_route(AzString::from_const_str("/user/:id"), cb);

        // valid_minimal (positive control)
        let (route, m) = config
            .match_route_for_path("/user/42")
            .expect("registered route must match");
        assert_eq!(route.pattern.as_str(), "/user/:id");
        assert_eq!(m.get_param("id").map(AzString::as_str), Some("42"));

        // "" and "/" both have zero segments -> they hit the "/" route.
        assert!(config.match_route_for_path("").is_some());
        assert!(config.match_route_for_path("/").is_some());

        // garbage / unicode / long / whitespace: deterministic, never panics.
        assert!(config.match_route_for_path("/nope/nope/nope").is_none());
        assert!(config.match_route_for_path("\0\0").is_none());
        assert!(config.match_route_for_path("   ").is_none());
        let long = String::from("/user/") + &"9".repeat(500_000);
        assert!(config.match_route_for_path(&long).is_some());
        let m = config
            .match_route_for_path("/user/\u{1F600}")
            .expect("unicode param");
        assert_eq!(m.1.get_param("id").map(AzString::as_str), Some("\u{1F600}"));
    }

    #[test]
    fn app_config_add_route_replaces_same_pattern_and_orders_by_insertion() {
        extern "C" fn layout_a(_: RefAny, _: crate::callbacks::LayoutCallbackInfo) -> crate::dom::Dom {
            crate::dom::Dom::create_body()
        }
        let cb: crate::callbacks::LayoutCallbackType = layout_a;

        let mut config = AppConfig::create();
        assert!(config.routes.as_ref().is_empty());
        config.add_route(AzString::from_const_str("/dup"), cb);
        config.add_route(AzString::from_const_str("/dup"), cb);
        assert_eq!(config.routes.as_ref().len(), 1, "same pattern must replace");

        // First matching route wins: a catch-all registered first shadows later routes.
        let mut config = AppConfig::create();
        config.add_route(AzString::from_const_str("/:anything"), cb);
        config.add_route(AzString::from_const_str("/about"), cb);
        let (route, _) = config.match_route_for_path("/about").expect("matches");
        assert_eq!(route.pattern.as_str(), "/:anything");
    }

    // =====================================================================
    // CONSTRUCTORS / INVARIANTS
    // =====================================================================

    #[test]
    fn dpi_scale_factor_new_handles_nan_and_infinities() {
        // FloatValue::new does a saturating f32 -> isize cast (NaN -> 0).
        assert_eq!(DpiScaleFactor::new(0.0).inner.get(), 0.0);
        assert_eq!(DpiScaleFactor::new(1.0).inner.get(), 1.0);
        assert_eq!(DpiScaleFactor::new(f32::NAN).inner.get(), 0.0);
        assert!(DpiScaleFactor::new(f32::INFINITY).inner.get().is_finite());
        assert!(DpiScaleFactor::new(f32::NEG_INFINITY).inner.get().is_finite());
        assert!(DpiScaleFactor::new(f32::MAX).inner.get().is_finite());
        assert!(DpiScaleFactor::new(f32::MIN).inner.get().is_finite());
        // Sub-precision values collapse to 0 (1/1000 fixed point), not to NaN.
        assert_eq!(DpiScaleFactor::new(f32::MIN_POSITIVE).inner.get(), 0.0);
        // Eq/Hash invariant: equal inputs produce equal (hashable) keys.
        assert_eq!(DpiScaleFactor::new(1.5), DpiScaleFactor::new(1.5));
        assert_ne!(DpiScaleFactor::new(1.5), DpiScaleFactor::new(2.0));
    }

    #[test]
    fn named_font_new_keeps_fields_verbatim() {
        let f = NamedFont::new(
            AzString::from_const_str(""),
            U8Vec::from_vec(Vec::new()),
        );
        assert_eq!(f.name.as_str(), "");
        assert!(f.bytes.as_ref().is_empty());

        let bytes = vec![0u8, 255, 128];
        let f = NamedFont::new(AzString::from(String::from("\u{1F600}")), bytes.clone().into());
        assert_eq!(f.name.as_str(), "\u{1F600}");
        assert_eq!(f.bytes.as_ref(), bytes.as_slice());
    }

    #[test]
    fn loaded_font_new_keeps_fields_verbatim_at_limits() {
        let f = LoadedFont::new(0, AzString::from_const_str(""), 0, false);
        assert_eq!(f.font_hash, 0);
        assert_eq!(f.num_glyphs, 0);
        assert!(!f.has_bytes);

        let f = LoadedFont::new(u64::MAX, AzString::from_const_str("x"), u32::MAX, true);
        assert_eq!(f.font_hash, u64::MAX);
        assert_eq!(f.num_glyphs, u32::MAX);
        assert!(f.has_bytes);
    }

    #[test]
    fn brush_new_defaults_and_extreme_radius() {
        let b = Brush::new(opaque_red(), 4.0);
        assert_eq!(b.radius, 4.0);
        assert_eq!(b.hardness, 0.5);
        assert_eq!(b.flow, 1.0);
        assert_eq!(b.spacing, 0.25);
        assert_eq!(b.color, opaque_red());

        // Extreme radii are stored verbatim (validation happens in paint_dot).
        assert!(Brush::new(opaque_red(), f32::NAN).radius.is_nan());
        assert_eq!(Brush::new(opaque_red(), -0.0).radius, -0.0);
        assert_eq!(Brush::new(opaque_red(), f32::INFINITY).radius, f32::INFINITY);
    }

    #[test]
    fn image_cache_new_is_empty_and_default_is_neutral() {
        let cache = ImageCache::new();
        assert!(cache.image_id_map.is_empty());
        assert!(ImageCache::default().image_id_map.is_empty());
    }

    #[test]
    fn gl_texture_cache_empty_is_neutral() {
        let cache = GlTextureCache::empty();
        assert!(cache.solved_textures.is_empty());
        assert!(cache.hashes.is_empty());
        let d = GlTextureCache::default();
        assert!(d.solved_textures.is_empty());
        assert!(d.hashes.is_empty());
    }

    #[test]
    fn external_image_id_new_is_monotonic() {
        let a = ExternalImageId::new();
        let b = ExternalImageId::new();
        assert!(b.inner > a.inner, "the counter must strictly increase");
        assert!(ExternalImageId::default().inner > b.inner);
    }

    #[test]
    fn shared_raw_image_data_new_invariants() {
        let empty = SharedRawImageData::new(U8Vec::from_vec(Vec::new()));
        assert_eq!(empty.len(), 0);
        assert!(empty.is_empty());
        assert!(empty.as_ref().is_empty());
        assert!(empty.get_bytes().is_empty());
        // An empty Vec still yields a non-null (dangling but aligned) pointer.
        assert!(!empty.as_ptr().is_null());

        let big = SharedRawImageData::new(vec![7u8; 100_000].into());
        assert_eq!(big.len(), 100_000);
        assert!(!big.is_empty());
        assert_eq!(big.as_ref().len(), big.len());
        assert_eq!(big.get_bytes(), big.as_ref());
        // len() must agree with the slice view (no stale-length bug).
        assert_eq!(big.into_inner().expect("sole owner").as_ref().len(), 100_000);
    }

    #[test]
    fn app_config_create_registers_builtins_and_defaults() {
        let config = AppConfig::create();
        assert_eq!(config.log_level, AppLogLevel::Error);
        assert!(!config.enable_visual_panic_hook);
        assert!(config.enable_logging_on_panic);
        assert_eq!(config.termination_behavior, AppTerminationBehavior::EndProcess);
        assert!(config.routes.as_ref().is_empty());
        assert!(matches!(
            config.mock_css_environment,
            OptionCssMockEnvironment::None
        ));
        // create() dogfoods add_component_library -> exactly one "builtin" library.
        let libs = config.component_libraries.as_ref();
        assert_eq!(libs.len(), 1);
        assert_eq!(libs[0].name.as_str(), "builtin");
        assert!(!libs[0].components.as_ref().is_empty());
    }

    #[test]
    fn app_config_add_component_library_replaces_same_name() {
        let register: crate::xml::RegisterComponentLibraryFnType =
            crate::xml::register_builtin_components;
        let mut config = AppConfig::create();
        let n_builtin = config.component_libraries.as_ref()[0].components.as_ref().len();

        // Same name -> wholesale replacement, NOT a duplicate library.
        config.add_component_library(AzString::from_const_str("builtin"), register);
        assert_eq!(config.component_libraries.as_ref().len(), 1);
        assert_eq!(
            config.component_libraries.as_ref()[0].components.as_ref().len(),
            n_builtin
        );

        // A different name (incl. empty / unicode) appends a new library.
        config.add_component_library(AzString::from_const_str(""), register);
        config.add_component_library(AzString::from_const_str("\u{1F600}"), register);
        assert_eq!(config.component_libraries.as_ref().len(), 3);
        assert_eq!(config.component_libraries.as_ref()[2].name.as_str(), "\u{1F600}");
    }

    #[test]
    fn app_config_with_mock_environment_sets_the_option() {
        let config = AppConfig::create().with_mock_environment(CssMockEnvironment::dark_theme());
        match config.mock_css_environment {
            OptionCssMockEnvironment::Some(env) => {
                assert!(matches!(
                    env.theme,
                    azul_css::dynamic_selector::OptionThemeCondition::Some(
                        azul_css::dynamic_selector::ThemeCondition::Dark
                    )
                ));
            }
            OptionCssMockEnvironment::None => panic!("mock env must be Some"),
        }
        // Last call wins (the field is overwritten, not merged).
        let config = AppConfig::create()
            .with_mock_environment(CssMockEnvironment::linux())
            .with_mock_environment(CssMockEnvironment::windows());
        match config.mock_css_environment {
            OptionCssMockEnvironment::Some(env) => assert!(matches!(
                env.os,
                azul_css::dynamic_selector::OptionOsCondition::Some(
                    azul_css::dynamic_selector::OsCondition::Windows
                )
            )),
            OptionCssMockEnvironment::None => panic!("mock env must be Some"),
        }
    }

    // =====================================================================
    // CssMockEnvironment
    // =====================================================================

    #[test]
    fn css_mock_environment_presets_only_set_their_own_field() {
        use azul_css::dynamic_selector::{
            OptionOsCondition, OptionThemeCondition, OsCondition, ThemeCondition,
        };

        for (mock, os) in [
            (CssMockEnvironment::linux(), OsCondition::Linux),
            (CssMockEnvironment::windows(), OsCondition::Windows),
            (CssMockEnvironment::macos(), OsCondition::MacOS),
        ] {
            assert!(matches!(mock.os, OptionOsCondition::Some(o) if o == os));
            // The other overrides stay unset (auto-detect).
            assert!(matches!(mock.theme, OptionThemeCondition::None));
            assert!(matches!(mock.viewport_width, azul_css::OptionF32::None));
        }

        assert!(matches!(
            CssMockEnvironment::dark_theme().theme,
            OptionThemeCondition::Some(ThemeCondition::Dark)
        ));
        assert!(matches!(
            CssMockEnvironment::light_theme().theme,
            OptionThemeCondition::Some(ThemeCondition::Light)
        ));
        assert!(matches!(
            CssMockEnvironment::dark_theme().os,
            OptionOsCondition::None
        ));
    }

    #[test]
    fn css_mock_environment_apply_to_overrides_only_set_fields() {
        use azul_css::dynamic_selector::{
            BoolCondition, DynamicSelectorContext, OptionOsCondition, OptionThemeCondition,
            OsCondition, ThemeCondition,
        };

        // An all-None mock must leave the context byte-for-byte alone.
        let mut ctx = DynamicSelectorContext::default();
        let before_os = ctx.os;
        let before_lang = ctx.language.clone();
        let before_w = ctx.viewport_width;
        CssMockEnvironment::default().apply_to(&mut ctx);
        assert_eq!(ctx.os, before_os);
        assert_eq!(ctx.language.as_str(), before_lang.as_str());
        assert_eq!(ctx.viewport_width, before_w);

        // A fully-populated mock overrides every field it sets - including
        // adversarial floats (NaN viewport) which must not panic.
        let mock = CssMockEnvironment {
            os: OptionOsCondition::Some(OsCondition::Windows),
            theme: OptionThemeCondition::Some(ThemeCondition::Dark),
            language: azul_css::OptionString::Some(AzString::from_const_str("de-DE")),
            viewport_width: azul_css::OptionF32::Some(f32::NAN),
            viewport_height: azul_css::OptionF32::Some(f32::INFINITY),
            prefers_reduced_motion: azul_css::OptionBool::Some(true),
            prefers_high_contrast: azul_css::OptionBool::Some(false),
            ..Default::default()
        };
        let mut ctx = DynamicSelectorContext::default();
        mock.apply_to(&mut ctx);
        assert_eq!(ctx.os, OsCondition::Windows);
        assert_eq!(ctx.theme, ThemeCondition::Dark);
        assert_eq!(ctx.language.as_str(), "de-DE");
        assert!(ctx.viewport_width.is_nan());
        assert_eq!(ctx.viewport_height, f32::INFINITY);
        assert_eq!(ctx.prefers_reduced_motion, BoolCondition::True);
        assert_eq!(ctx.prefers_high_contrast, BoolCondition::False);

        // apply_to is idempotent.
        let mut ctx2 = ctx.clone();
        mock.apply_to(&mut ctx2);
        assert_eq!(ctx2.os, ctx.os);
        assert_eq!(ctx2.theme, ctx.theme);
    }

    // =====================================================================
    // NUMERIC: brush_dab_coverage / normalize_u16 / premultiply_alpha / Au
    // =====================================================================

    #[test]
    fn brush_dab_coverage_boundaries_and_monotonicity() {
        // Documented profile: 1.0 at the center, 0.0 at (and past) the edge.
        assert_eq!(brush_dab_coverage(0.0, 0.5), 1.0);
        assert_eq!(brush_dab_coverage(1.0, 0.5), 0.0);
        // Out-of-range t is clamped, not extrapolated.
        assert_eq!(brush_dab_coverage(-5.0, 0.5), 1.0);
        assert_eq!(brush_dab_coverage(2.0, 0.5), 0.0);
        assert_eq!(brush_dab_coverage(f32::INFINITY, 0.5), 0.0);
        assert_eq!(brush_dab_coverage(f32::NEG_INFINITY, 0.5), 1.0);

        // Monotonically non-increasing in t, and always inside [0, 1].
        let mut prev = f32::INFINITY;
        for i in 0..=100 {
            let t = i as f32 / 100.0;
            let c = brush_dab_coverage(t, 0.5);
            assert!((0.0..=1.0).contains(&c), "coverage {c} out of range at t={t}");
            assert!(c <= prev + 1.0e-6, "not monotonic at t={t}");
            prev = c;
        }
    }

    #[test]
    fn brush_dab_coverage_hardness_limits_never_divide_by_zero() {
        // hardness == 1.0 would make (1 - edge0) == 0; the 1e-4 floor prevents
        // a division by zero -> a hard (but finite) edge instead of inf/NaN.
        assert_eq!(brush_dab_coverage(0.5, 1.0), 1.0);
        assert!(brush_dab_coverage(1.0, 1.0).is_finite());
        assert_eq!(brush_dab_coverage(1.0, 1.0), 1.0); // exactly at edge0 -> x == 0
        assert_eq!(brush_dab_coverage(2.0, 1.0), 0.0);

        // hardness is clamped, so out-of-range hardness behaves like 0.0 / 1.0.
        assert_eq!(brush_dab_coverage(0.5, -10.0), brush_dab_coverage(0.5, 0.0));
        assert_eq!(brush_dab_coverage(0.5, 10.0), brush_dab_coverage(0.5, 1.0));
        assert_eq!(
            brush_dab_coverage(0.5, f32::NEG_INFINITY),
            brush_dab_coverage(0.5, 0.0)
        );
        assert!(brush_dab_coverage(0.5, f32::INFINITY).is_finite());
    }

    #[test]
    fn brush_dab_coverage_nan_propagates_without_panicking() {
        // NaN in -> NaN out (documented-by-behavior); crucially, no panic and no
        // hang. paint_dot's `a <= 0.0` check then skips NaN coverage entirely.
        assert!(brush_dab_coverage(f32::NAN, 0.5).is_nan());
        assert!(brush_dab_coverage(0.5, f32::NAN).is_nan());
        assert!(brush_dab_coverage(f32::NAN, f32::NAN).is_nan());
    }

    #[test]
    fn normalize_u16_is_monotonic_and_saturating() {
        assert_eq!(normalize_u16(u16::MIN), 0);
        assert_eq!(normalize_u16(u16::MAX), u8::MAX);
        let mut prev = 0u8;
        for i in (0..=u16::MAX).step_by(97) {
            let v = normalize_u16(i);
            assert!(v >= prev, "normalize_u16 must be monotonic ({i} -> {v})");
            prev = v;
        }
    }

    #[test]
    fn premultiply_alpha_ignores_non_4_byte_slices() {
        // Documented: only a single 4-byte pixel is touched.
        for len in [0usize, 1, 2, 3, 5, 8] {
            let mut buf = vec![200u8; len];
            let before = buf.clone();
            premultiply_alpha(&mut buf);
            assert_eq!(buf, before, "len {len} must be left untouched");
        }
    }

    #[test]
    fn premultiply_alpha_boundary_values_never_overflow() {
        // a == 255 -> unchanged (rounding must not drift).
        let mut opaque = [255u8, 128, 0, 255];
        premultiply_alpha(&mut opaque);
        assert_eq!(opaque, [255, 128, 0, 255]);

        // a == 0 -> fully transparent -> RGB zeroed, alpha untouched.
        let mut transparent = [255u8, 255, 255, 0];
        premultiply_alpha(&mut transparent);
        assert_eq!(transparent, [0, 0, 0, 0]);

        // a == 128 -> ~half, computed with the +128/255 rounding, never > 255.
        let mut half = [255u8, 128, 0, 128];
        premultiply_alpha(&mut half);
        assert_eq!(half, [128, 64, 0, 128]);

        // The u32 intermediate must not truncate at the maximum product.
        let mut max = [255u8, 255, 255, 255];
        premultiply_alpha(&mut max);
        assert_eq!(max, [255, 255, 255, 255]);
    }

    #[test]
    fn au_from_px_saturates_at_limits_and_nan() {
        assert_eq!(Au::from_px(0.0).0, 0);
        assert_eq!(Au::from_px(-0.0).0, 0);
        assert_eq!(Au::from_px(1.0).0, AU_PER_PX);
        assert_eq!(Au::from_px(-1.0).0, -AU_PER_PX);
        // NaN -> 0 (saturating `as` cast), NOT a panic and NOT garbage.
        assert_eq!(Au::from_px(f32::NAN).0, 0);
        // Infinities / f32 extremes clamp into [MIN_AU, MAX_AU].
        assert_eq!(Au::from_px(f32::INFINITY).0, MAX_AU);
        assert_eq!(Au::from_px(f32::NEG_INFINITY).0, MIN_AU);
        assert_eq!(Au::from_px(f32::MAX).0, MAX_AU);
        assert_eq!(Au::from_px(f32::MIN).0, MIN_AU);
        // Anything in range stays in range.
        for px in [-1.0e9_f32, -1.0, 0.5, 16.0, 1.0e9] {
            let au = Au::from_px(px).0;
            assert!((MIN_AU..=MAX_AU).contains(&au), "{px} -> {au} escaped the clamp");
        }
    }

    #[test]
    fn au_px_round_trip_is_stable() {
        // px -> Au -> px must round-trip within one app-unit (1/60 px).
        for px in [0.0_f32, 0.5, 1.0, 12.0, 16.0, 72.5, -3.25, 1000.0] {
            let back = Au::from_px(px).into_px();
            assert!(
                (back - px).abs() <= 1.0 / AU_PER_PX as f32,
                "{px} round-tripped to {back}"
            );
        }
        // Exact for whole pixels.
        assert_eq!(Au::from_px(16.0).into_px(), 16.0);
        // Extremes stay finite.
        assert!(Au(MAX_AU).into_px().is_finite());
        assert!(Au(MIN_AU).into_px().is_finite());
        assert!(Au(i32::MIN).into_px().is_finite());
        assert!(Au(i32::MAX).into_px().is_finite());
    }

    #[test]
    fn font_size_to_au_zero_negative_and_typical() {
        use azul_css::props::basic::PixelValue;
        let au = |px: isize| {
            font_size_to_au(StyleFontSize {
                inner: PixelValue::const_px(px),
            })
            .0
        };
        assert_eq!(au(0), 0);
        assert_eq!(au(16), 16 * AU_PER_PX);
        assert_eq!(au(-10), -10 * AU_PER_PX);
        // Large-but-representable sizes stay inside the clamp.
        assert!((MIN_AU..=MAX_AU).contains(&au(1_000_000)));
        assert!((MIN_AU..=MAX_AU).contains(&au(-1_000_000)));
    }

    // =====================================================================
    // NUMERIC: Epoch
    // =====================================================================

    #[test]
    fn epoch_new_from_and_into_u32() {
        assert_eq!(Epoch::new().into_u32(), 0);
        assert_eq!(Epoch::default().into_u32(), 0);
        assert_eq!(Epoch::from(0).into_u32(), 0);
        assert_eq!(Epoch::from(1).into_u32(), 1);
        assert_eq!(Epoch::from(u32::MAX).into_u32(), u32::MAX);
        assert_eq!(Epoch::from(u32::MAX - 1).into_u32(), u32::MAX - 1);
    }

    #[test]
    fn epoch_increment_wraps_at_max_minus_one_and_never_reaches_max() {
        let mut e = Epoch::new();
        e.increment();
        assert_eq!(e.into_u32(), 1);

        // u32::MAX is reserved as "invalid", so MAX-1 wraps back to 0.
        let mut e = Epoch::from(u32::MAX - 1);
        e.increment();
        assert_eq!(e.into_u32(), 0, "MAX-1 must wrap to 0, never to u32::MAX");

        // An epoch that somehow starts AT u32::MAX saturates (fixpoint) instead
        // of wrapping or overflow-panicking - deterministic, no UB.
        let mut e = Epoch::from(u32::MAX);
        e.increment();
        assert_eq!(e.into_u32(), u32::MAX);

        // A long run of increments never yields the invalid u32::MAX.
        let mut e = Epoch::from(u32::MAX - 3);
        for _ in 0..8 {
            e.increment();
            assert_ne!(e.into_u32(), u32::MAX);
        }
    }

    #[test]
    fn epoch_display_is_non_empty_for_edge_values() {
        assert_eq!(alloc::format!("{}", Epoch::new()), "0");
        assert_eq!(alloc::format!("{}", Epoch::from(42)), "42");
        assert_eq!(
            alloc::format!("{}", Epoch::from(u32::MAX)),
            alloc::format!("{}", u32::MAX)
        );
        assert!(!alloc::format!("{:?}", Epoch::default()).is_empty());
    }

    #[test]
    fn id_namespace_display_and_debug_are_well_formed() {
        assert_eq!(alloc::format!("{}", IdNamespace(0)), "IdNamespace(0)");
        assert_eq!(
            alloc::format!("{}", IdNamespace(u32::MAX)),
            alloc::format!("IdNamespace({})", u32::MAX)
        );
        // Debug delegates to Display (must not recurse / be empty).
        assert_eq!(
            alloc::format!("{:?}", IdNamespace(7)),
            alloc::format!("{}", IdNamespace(7))
        );
    }

    // =====================================================================
    // KEYS: uniqueness / namespace preservation / hash->key derivation
    // =====================================================================

    #[test]
    fn unique_keys_are_strictly_increasing_and_keep_their_namespace() {
        let ns = IdNamespace(u32::MAX);

        let a = ImageKey::unique(ns);
        let b = ImageKey::unique(ns);
        assert_eq!(a.namespace, ns);
        assert!(b.key > a.key, "ImageKey counter must strictly increase");
        // The counter starts at 1 so a live key can never collide with DUMMY.
        assert_eq!(ImageKey::DUMMY.key, 0);
        assert_ne!(a, ImageKey::DUMMY);

        let a = FontKey::unique(ns);
        let b = FontKey::unique(ns);
        assert_eq!(a.namespace, ns);
        assert!(b.key > a.key);

        let a = FontInstanceKey::unique(IdNamespace(0));
        let b = FontInstanceKey::unique(IdNamespace(0));
        assert_eq!(a.namespace, IdNamespace(0));
        assert!(b.key > a.key);
    }

    #[test]
    fn image_ref_id_counter_is_monotonic_and_never_zero() {
        // id == 0 is reserved to flag an un-initialised handle.
        let a = next_image_ref_id();
        let b = next_image_ref_id();
        assert!(a > 0 && b > a);
    }

    #[test]
    fn image_ref_hash_conversions_are_lossless_and_agree() {
        let img = ImageRef::null_image(1, 1, RawImageFormat::RGBA8, Vec::new());
        let hash = img.get_hash();
        assert_eq!(hash, image_ref_get_hash(&img));

        let key = image_ref_hash_to_image_key(hash, IdNamespace(9));
        assert_eq!(key.namespace, IdNamespace(9));
        assert_eq!(key.key, hash.inner, "the u64 id must survive verbatim");

        let ext = image_ref_hash_to_external_image_id(hash);
        assert_eq!(ext.inner, hash.inner);

        // Both derivations agree for boundary hashes too.
        for inner in [0u64, 1, u64::MAX, u64::MAX - 1] {
            let h = ImageRefHash { inner };
            assert_eq!(image_ref_hash_to_image_key(h, IdNamespace(0)).key, inner);
            assert_eq!(image_ref_hash_to_external_image_id(h).inner, inner);
        }
    }

    #[test]
    fn texture_external_image_id_is_deterministic_and_collision_free() {
        let id = |d: usize, n: usize| texture_external_image_id(DomId { inner: d }, NodeId::new(n));

        // Same input -> same id (cached display lists depend on this).
        assert_eq!(id(3, 7), id(3, 7));
        assert_eq!(id(0, 0).inner, 0);
        // The dom goes in the high 32 bits, the node in the low 32.
        assert_eq!(id(1, 2).inner, (1u64 << 32) | 2);
        // (0,1) and (1,0) must not collide.
        assert_ne!(id(0, 1), id(1, 0));
        // Boundary node index inside the 32-bit range.
        assert_eq!(id(0, u32::MAX as usize).inner, u64::from(u32::MAX));
        assert_eq!(
            id(u32::MAX as usize, 0).inner,
            u64::from(u32::MAX) << 32
        );
    }

    // =====================================================================
    // GETTERS / PREDICATES: RawImageData
    // =====================================================================

    #[test]
    fn raw_image_data_typed_getters_only_match_their_own_variant() {
        let u8v = RawImageData::U8(vec![1u8, 2].into());
        let u16v = RawImageData::U16(vec![1u16, 2].into());
        let f32v = RawImageData::F32(vec![1.0f32, 2.0].into());

        assert_eq!(u8v.get_u8_vec_ref().map(|v| v.len()), Some(2));
        assert!(u8v.get_u16_vec_ref().is_none());
        assert!(u8v.get_f32_vec_ref().is_none());

        assert!(u16v.get_u8_vec_ref().is_none());
        assert_eq!(u16v.get_u16_vec_ref().map(|v| v.len()), Some(2));
        assert!(u16v.get_f32_vec_ref().is_none());

        assert!(f32v.get_u8_vec_ref().is_none());
        assert!(f32v.get_u16_vec_ref().is_none());
        assert_eq!(f32v.get_f32_vec_ref().map(|v| v.len()), Some(2));

        // Empty payloads are Some(empty), not None.
        let empty = RawImageData::U8(U8Vec::from_vec(Vec::new()));
        assert_eq!(empty.get_u8_vec_ref().map(|v| v.len()), Some(0));

        // by-value variants agree with the by-ref ones
        assert!(RawImageData::U8(vec![9u8].into()).get_u8_vec().is_some());
        assert!(RawImageData::U16(vec![9u16].into()).get_u8_vec().is_none());
        assert!(RawImageData::U16(vec![9u16].into()).get_u16_vec().is_some());
        assert!(RawImageData::F32(vec![9.0f32].into()).get_u16_vec().is_none());
    }

    // =====================================================================
    // NUMERIC / ROUND-TRIP: RawImage::load_* format conversions
    // =====================================================================

    #[test]
    fn load_fns_reject_wrong_payload_type() {
        // Every loader demands a specific RawImageData variant; a mismatch is
        // None (never a panic / never garbage pixels).
        let u16_1px = || RawImageData::U16(vec![0u16; 4].into());
        let f32_1px = || RawImageData::F32(vec![0.0f32; 4].into());
        let u8_1px = || RawImageData::U8(vec![0u8; 4].into());

        assert!(RawImage::load_r8(u16_1px(), 4).is_none());
        assert!(RawImage::load_rg8(f32_1px(), 2, true).is_none());
        assert!(RawImage::load_rgb8(u16_1px(), 1).is_none());
        assert!(RawImage::load_rgba8(f32_1px(), 1, true).is_none());
        assert!(RawImage::load_r16(u8_1px(), 4).is_none());
        assert!(RawImage::load_rg16(f32_1px(), 2).is_none());
        assert!(RawImage::load_rgb16(u8_1px(), 1).is_none());
        assert!(RawImage::load_rgba16(u8_1px(), 1, true).is_none());
        assert!(RawImage::load_bgr8(u16_1px(), 1).is_none());
        assert!(RawImage::load_bgra8(u16_1px(), 1, true).is_none());
        assert!(RawImage::load_rgbf32(u8_1px(), 1).is_none());
        assert!(RawImage::load_rgbaf32(u16_1px(), 1, true).is_none());
    }

    #[test]
    fn load_fns_reject_every_wrong_length() {
        // One byte too few and one too many must BOTH be rejected for each format.
        assert!(RawImage::load_r8(RawImageData::U8(vec![0u8; 3].into()), 4).is_none());
        assert!(RawImage::load_r8(RawImageData::U8(vec![0u8; 5].into()), 4).is_none());
        assert!(RawImage::load_rg8(RawImageData::U8(vec![0u8; 3].into()), 2, true).is_none());
        assert!(RawImage::load_rg8(RawImageData::U8(vec![0u8; 5].into()), 2, true).is_none());
        assert!(RawImage::load_rgb8(RawImageData::U8(vec![0u8; 5].into()), 2).is_none());
        assert!(RawImage::load_rgb8(RawImageData::U8(vec![0u8; 7].into()), 2).is_none());
        assert!(RawImage::load_rgba8(RawImageData::U8(vec![0u8; 7].into()), 2, true).is_none());
        assert!(RawImage::load_rgba8(RawImageData::U8(vec![0u8; 9].into()), 2, false).is_none());
        assert!(RawImage::load_r16(RawImageData::U16(vec![0u16; 3].into()), 4).is_none());
        assert!(RawImage::load_rg16(RawImageData::U16(vec![0u16; 3].into()), 2).is_none());
        assert!(RawImage::load_rgb16(RawImageData::U16(vec![0u16; 5].into()), 2).is_none());
        assert!(RawImage::load_rgba16(RawImageData::U16(vec![0u16; 7].into()), 2, true).is_none());
        assert!(RawImage::load_bgr8(RawImageData::U8(vec![0u8; 5].into()), 2).is_none());
        assert!(RawImage::load_bgra8(RawImageData::U8(vec![0u8; 7].into()), 2, false).is_none());
        assert!(RawImage::load_rgbf32(RawImageData::F32(vec![0.0f32; 5].into()), 2).is_none());
        assert!(
            RawImage::load_rgbaf32(RawImageData::F32(vec![0.0f32; 7].into()), 2, true).is_none()
        );
    }

    #[test]
    fn load_fns_accept_zero_pixels() {
        // expected_len == 0 with an empty buffer: Some(empty), no div-by-zero.
        let empty_u8 = || RawImageData::U8(U8Vec::from_vec(Vec::new()));
        let empty_u16 = || RawImageData::U16(U16Vec::from_vec(Vec::new()));
        let empty_f32 = || RawImageData::F32(F32Vec::from_vec(Vec::new()));

        assert_eq!(RawImage::load_r8(empty_u8(), 0).map(|(b, o)| (b.len(), o)), Some((0, false)));
        assert_eq!(RawImage::load_rg8(empty_u8(), 0, true).map(|(b, _)| b.len()), Some(0));
        assert_eq!(RawImage::load_rgb8(empty_u8(), 0).map(|(b, _)| b.len()), Some(0));
        assert_eq!(RawImage::load_rgba8(empty_u8(), 0, true).map(|(b, _)| b.len()), Some(0));
        assert_eq!(RawImage::load_r16(empty_u16(), 0).map(|(b, _)| b.len()), Some(0));
        assert_eq!(RawImage::load_rg16(empty_u16(), 0).map(|(b, _)| b.len()), Some(0));
        assert_eq!(RawImage::load_rgb16(empty_u16(), 0).map(|(b, _)| b.len()), Some(0));
        assert_eq!(RawImage::load_rgba16(empty_u16(), 0, true).map(|(b, _)| b.len()), Some(0));
        assert_eq!(RawImage::load_bgr8(empty_u8(), 0).map(|(b, _)| b.len()), Some(0));
        assert_eq!(RawImage::load_bgra8(empty_u8(), 0, true).map(|(b, _)| b.len()), Some(0));
        assert_eq!(RawImage::load_rgbf32(empty_f32(), 0).map(|(b, _)| b.len()), Some(0));
        assert_eq!(RawImage::load_rgbaf32(empty_f32(), 0, true).map(|(b, _)| b.len()), Some(0));
    }

    #[test]
    fn load_r8_passes_data_through_and_is_never_opaque() {
        // R8 must stay R8 (image masks depend on the single channel surviving).
        let (bytes, is_opaque) =
            RawImage::load_r8(RawImageData::U8(vec![0u8, 128, 255, 1].into()), 4)
                .expect("exact length");
        assert_eq!(bytes.as_ref(), &[0, 128, 255, 1]);
        assert!(!is_opaque, "R8 is documented as never opaque");
    }

    #[test]
    fn load_rgb8_and_bgr8_swizzle_to_bgra_opaque() {
        // RGB8 -> BGRA8: channel order flips, alpha forced to 0xFF, always opaque.
        let (bytes, is_opaque) =
            RawImage::load_rgb8(RawImageData::U8(vec![1u8, 2, 3].into()), 1).expect("1 px");
        assert_eq!(bytes.as_ref(), &[3, 2, 1, 255]);
        assert!(is_opaque);

        // BGR8 -> BGRA8: order preserved, alpha appended.
        let (bytes, is_opaque) =
            RawImage::load_bgr8(RawImageData::U8(vec![1u8, 2, 3].into()), 1).expect("1 px");
        assert_eq!(bytes.as_ref(), &[1, 2, 3, 255]);
        assert!(is_opaque);
    }

    #[test]
    fn load_rgba8_swizzles_and_detects_transparency() {
        // Premultiplied: RGBA -> BGRA swizzle only.
        let (bytes, is_opaque) =
            RawImage::load_rgba8(RawImageData::U8(vec![10u8, 20, 30, 255].into()), 1, true)
                .expect("1 px");
        assert_eq!(bytes.as_ref(), &[30, 20, 10, 255]);
        assert!(is_opaque);

        // A single non-255 alpha flips is_opaque to false.
        let (_, is_opaque) =
            RawImage::load_rgba8(RawImageData::U8(vec![0u8, 0, 0, 254].into()), 1, true)
                .expect("1 px");
        assert!(!is_opaque);

        // Non-premultiplied: swizzle THEN premultiply by alpha.
        let (bytes, is_opaque) =
            RawImage::load_rgba8(RawImageData::U8(vec![10u8, 20, 30, 128].into()), 1, false)
                .expect("1 px");
        assert_eq!(bytes.as_ref(), &[15, 10, 5, 128]);
        assert!(!is_opaque);

        // alpha == 0 must zero the colour (no leftover colour fringe).
        let (bytes, _) =
            RawImage::load_rgba8(RawImageData::U8(vec![255u8, 255, 255, 0].into()), 1, false)
                .expect("1 px");
        assert_eq!(bytes.as_ref(), &[0, 0, 0, 0]);
    }

    #[test]
    fn load_rg8_expands_grey_to_bgra() {
        // Greyscale + alpha -> BGRA with the grey replicated across B/G/R.
        let (bytes, is_opaque) =
            RawImage::load_rg8(RawImageData::U8(vec![100u8, 255].into()), 1, true).expect("1 px");
        assert_eq!(bytes.as_ref(), &[100, 100, 100, 255]);
        assert!(is_opaque);

        let (bytes, is_opaque) =
            RawImage::load_rg8(RawImageData::U8(vec![100u8, 128].into()), 1, false).expect("1 px");
        assert_eq!(bytes.as_ref(), &[50, 50, 50, 128]);
        assert!(!is_opaque);
    }

    #[test]
    fn load_16_bit_formats_normalize_to_8_bit() {
        // u16::MAX -> 255, 0 -> 0 (no wrap-around / no div-by-zero).
        let (bytes, is_opaque) =
            RawImage::load_r16(RawImageData::U16(vec![u16::MAX].into()), 1).expect("1 px");
        assert_eq!(bytes.as_ref(), &[255, 255, 255, 255]);
        assert!(is_opaque);

        let (bytes, is_opaque) =
            RawImage::load_rg16(RawImageData::U16(vec![0u16, u16::MAX].into()), 1).expect("1 px");
        assert_eq!(bytes.as_ref(), &[0, 0, 0, 255]);
        assert!(is_opaque);

        // RGB16 -> BGRA8 swizzle.
        let (bytes, _) = RawImage::load_rgb16(
            RawImageData::U16(vec![u16::MAX, 0, 0].into()),
            1,
        )
        .expect("1 px");
        assert_eq!(bytes.as_ref(), &[0, 0, 255, 255]);

        // RGBA16 with a zero alpha -> not opaque; premultiply zeroes the colour.
        let (bytes, is_opaque) = RawImage::load_rgba16(
            RawImageData::U16(vec![u16::MAX, u16::MAX, u16::MAX, 0].into()),
            1,
            false,
        )
        .expect("1 px");
        assert_eq!(bytes.as_ref(), &[0, 0, 0, 0]);
        assert!(!is_opaque);
    }

    #[test]
    fn load_f32_formats_saturate_on_out_of_range_nan_and_inf() {
        // The f32 -> u8 cast is saturating: >1.0 -> 255, <0.0 -> 0, NaN -> 0.
        // (This is the whole "HDR pixel with a garbage float" attack surface.)
        let (bytes, is_opaque) = RawImage::load_rgbf32(
            RawImageData::F32(vec![2.0f32, -1.0, f32::NAN].into()),
            1,
        )
        .expect("1 px");
        assert_eq!(bytes.as_ref(), &[0, 0, 255, 255], "b=NaN->0, g=-1->0, r=2.0->255");
        assert!(is_opaque);

        let (bytes, is_opaque) = RawImage::load_rgbaf32(
            RawImageData::F32(vec![f32::INFINITY, f32::NEG_INFINITY, 0.5, 1.0].into()),
            1,
            true,
        )
        .expect("1 px");
        assert_eq!(bytes.as_ref(), &[127, 0, 255, 255]);
        assert!(is_opaque);

        // NaN alpha -> 0 -> not opaque (fails safe, does not claim opacity).
        let (_, is_opaque) = RawImage::load_rgbaf32(
            RawImageData::F32(vec![1.0f32, 1.0, 1.0, f32::NAN].into()),
            1,
            true,
        )
        .expect("1 px");
        assert!(!is_opaque);
    }

    // =====================================================================
    // ROUND-TRIP: RawImage <-> ImageRef
    // =====================================================================

    #[test]
    fn raw_image_null_image_encodes_to_an_empty_bgra8_descriptor() {
        let null = RawImage::null_image();
        assert_eq!(null.width, 0);
        assert_eq!(null.height, 0);
        assert_eq!(null.data_format, RawImageFormat::BGRA8);
        assert!(null.premultiplied_alpha);

        let (data, descriptor) = null
            .into_loaded_image_source()
            .expect("a 0x0 image is still a valid (empty) source");
        assert_eq!(descriptor.width, 0);
        assert_eq!(descriptor.height, 0);
        assert_eq!(descriptor.format, RawImageFormat::BGRA8);
        assert_eq!(descriptor.offset, 0);
        match data {
            ImageData::Raw(bytes) => assert!(bytes.is_empty()),
            ImageData::External(_) => panic!("a RawImage must never encode to External"),
        }
    }

    #[test]
    fn raw_image_allocate_mask_zero_and_negative_sizes() {
        let mask = RawImage::allocate_mask(LayoutSize::zero());
        assert_eq!(mask.data_format, RawImageFormat::R8);
        assert_eq!(mask.width, 0);
        assert_eq!(mask.height, 0);
        assert_eq!(mask.pixels.get_u8_vec_ref().map(|v| v.len()), Some(0));

        let mask = RawImage::allocate_mask(LayoutSize::new(4, 4));
        assert_eq!(mask.pixels.get_u8_vec_ref().map(|v| v.len()), Some(16));
        assert!(mask
            .pixels
            .get_u8_vec_ref()
            .expect("u8")
            .as_ref()
            .iter()
            .all(|b| *b == 0));

        // Negative sizes: the BUFFER is clamped to 0 (no huge alloc, no panic),
        // but the width/height FIELDS keep the wrapped `as usize` value, so the
        // returned RawImage is internally inconsistent. Callers must not feed a
        // negative LayoutSize in. (Buffer-side safety is what matters here.)
        let mask = RawImage::allocate_mask(LayoutSize::new(-4, 4));
        assert_eq!(
            mask.pixels.get_u8_vec_ref().map(|v| v.len()),
            Some(0),
            "a negative extent must never allocate"
        );
        assert!(mask.width > 1_000_000, "negative width wraps via `as usize`");
    }

    #[test]
    fn raw_image_mask_round_trips_as_r8() {
        // A mask must stay single-channel R8 through the encoder (clip masks
        // break if it silently becomes BGRA8).
        let mask = RawImage::allocate_mask(LayoutSize::new(2, 2));
        let (data, descriptor) = mask.into_loaded_image_source().expect("consistent mask");
        assert_eq!(descriptor.format, RawImageFormat::R8);
        assert_eq!((descriptor.width, descriptor.height), (2, 2));
        assert!(!descriptor.flags.is_opaque, "R8 is never opaque");
        match data {
            ImageData::Raw(bytes) => assert_eq!(bytes.len(), 4),
            ImageData::External(_) => panic!("expected raw bytes"),
        }
    }

    #[test]
    fn raw_image_rgba8_encode_decode_round_trip() {
        // encode: RGBA8 -> BGRA8 bytes; decode: ImageRef::get_rawimage gives the
        // encoded (BGRA8) pixels back verbatim.
        let raw = RawImage {
            pixels: RawImageData::U8(vec![10u8, 20, 30, 255].into()),
            width: 1,
            height: 1,
            premultiplied_alpha: true,
            data_format: RawImageFormat::RGBA8,
            tag: Vec::new().into(),
        };
        let img = ImageRef::new_rawimage(raw).expect("1x1 RGBA8 with 4 bytes is valid");

        assert!(img.is_raw_image());
        assert!(!img.is_null_image());
        assert!(!img.is_gl_texture());
        assert!(!img.is_callback());
        assert_eq!(img.get_size(), LogicalSize::new(1.0, 1.0));
        assert_eq!(img.get_bytes(), Some(&[30u8, 20, 10, 255][..]));
        assert!(!img.get_bytes_ptr().is_null());

        let decoded = img.get_rawimage().expect("raw image round-trips");
        assert_eq!(decoded.width, 1);
        assert_eq!(decoded.height, 1);
        assert_eq!(decoded.data_format, RawImageFormat::BGRA8);
        assert!(decoded.premultiplied_alpha);
        assert_eq!(
            decoded.pixels.get_u8_vec_ref().map(|v| v.as_ref().to_vec()),
            Some(vec![30, 20, 10, 255])
        );
    }

    #[test]
    fn image_ref_new_rawimage_rejects_dimension_mismatch() {
        // 2x2 RGBA8 needs 16 bytes; 4 bytes must be rejected (None, not a crash).
        let too_small = RawImage {
            pixels: RawImageData::U8(vec![0u8; 4].into()),
            width: 2,
            height: 2,
            premultiplied_alpha: true,
            data_format: RawImageFormat::RGBA8,
            tag: Vec::new().into(),
        };
        assert!(ImageRef::new_rawimage(too_small).is_none());

        // Too MANY bytes is equally invalid.
        let too_big = RawImage {
            pixels: RawImageData::U8(vec![0u8; 64].into()),
            width: 2,
            height: 2,
            premultiplied_alpha: true,
            data_format: RawImageFormat::RGBA8,
            tag: Vec::new().into(),
        };
        assert!(ImageRef::new_rawimage(too_big).is_none());

        // Right byte count, wrong payload type -> None.
        let wrong_type = RawImage {
            pixels: RawImageData::U16(vec![0u16; 16].into()),
            width: 2,
            height: 2,
            premultiplied_alpha: true,
            data_format: RawImageFormat::RGBA8,
            tag: Vec::new().into(),
        };
        assert!(ImageRef::new_rawimage(wrong_type).is_none());
    }

    // =====================================================================
    // GETTERS / PREDICATES: ImageRef
    // =====================================================================

    #[test]
    fn image_ref_null_image_predicates_and_accessors() {
        let img = ImageRef::null_image(0, 0, RawImageFormat::BGRA8, Vec::new());
        assert!(img.is_null_image());
        assert!(!img.is_raw_image());
        assert!(!img.is_gl_texture());
        assert!(!img.is_callback());
        assert_eq!(img.get_size(), LogicalSize::new(0.0, 0.0));
        assert!(img.get_bytes().is_none());
        assert!(img.get_rawimage().is_none());
        assert!(img.get_bytes_ptr().is_null());
        assert!(img.get_image_callback().is_none());
        assert!(matches!(img.get_data(), DecodedImage::NullImage { .. }));
    }

    #[test]
    fn image_ref_null_image_at_usize_max_reports_a_finite_size() {
        // usize::MAX as f32 must not produce NaN/inf (it saturates to ~1.8e19).
        let img = ImageRef::null_image(usize::MAX, usize::MAX, RawImageFormat::R8, Vec::new());
        let size = img.get_size();
        assert!(size.width.is_finite() && size.height.is_finite());
        assert!(size.width > 0.0 && size.height > 0.0);
        assert!(img.is_null_image());

        // A large tag survives verbatim.
        let img = ImageRef::null_image(1, 1, RawImageFormat::R8, vec![9u8; 10_000]);
        match img.get_data() {
            DecodedImage::NullImage { tag, .. } => assert_eq!(tag.len(), 10_000),
            _ => panic!("expected NullImage"),
        }
    }

    #[test]
    fn image_ref_hash_identity_rules() {
        let a = ImageRef::null_image(1, 1, RawImageFormat::R8, Vec::new());
        let b = ImageRef::null_image(1, 1, RawImageFormat::R8, Vec::new());
        // Two structurally identical images are still DIFFERENT images.
        assert_ne!(a.get_hash(), b.get_hash());
        assert_ne!(a, b);

        // A shallow clone is the SAME image.
        let a2 = a.clone();
        assert_eq!(a.get_hash(), a2.get_hash());
        assert_eq!(a, a2);

        // A deep copy is a NEW image with a fresh identity.
        let deep = a.deep_copy();
        assert_ne!(a.get_hash(), deep.get_hash());
        assert!(deep.is_null_image());
        assert_eq!(deep.get_size(), a.get_size());
    }

    #[test]
    fn image_ref_callback_accessors() {
        // CoreRenderImageCallbackType is a usize placeholder, so 0 is a valid
        // (if inert) callback token.
        let mut img = ImageRef::callback(0usize, RefAny::new(123u32));
        assert!(img.is_callback());
        assert!(!img.is_null_image());
        assert!(!img.is_raw_image());
        // Documented: a Callback reports a (0, 0) size.
        assert_eq!(img.get_size(), LogicalSize::new(0.0, 0.0));
        assert!(img.get_bytes().is_none());
        assert!(img.get_bytes_ptr().is_null());
        assert!(img.get_rawimage().is_none());

        // Sole owner -> the callback is reachable (shared / mutable).
        assert!(img.get_image_callback().is_some());
        assert!(img.get_image_callback_mut().is_some());

        // While a second handle is alive, aliasing &mut would be unsound, so
        // BOTH accessors must refuse.
        let clone = img.clone();
        assert!(img.get_image_callback().is_none());
        assert!(img.get_image_callback_mut().is_none());
        drop(clone);
        assert!(img.get_image_callback().is_some());
    }

    #[test]
    fn image_ref_deep_copy_of_a_callback_keeps_it_a_callback() {
        let img = ImageRef::callback(0usize, RefAny::new(1u8));
        let deep = img.deep_copy();
        assert!(deep.is_callback());
        assert_ne!(img.get_hash(), deep.get_hash());
    }

    #[test]
    fn image_ref_into_inner_only_when_sole_owner() {
        let img = ImageRef::null_image(2, 2, RawImageFormat::RGBA8, vec![1, 2, 3]);
        let clone = img.clone();
        assert!(clone.into_inner().is_none(), "shared -> must refuse");

        let inner = img.into_inner().expect("sole owner -> takes ownership");
        match inner {
            DecodedImage::NullImage {
                width,
                height,
                format,
                tag,
            } => {
                assert_eq!((width, height), (2, 2));
                assert_eq!(format, RawImageFormat::RGBA8);
                assert_eq!(tag, vec![1, 2, 3]);
            }
            _ => panic!("expected NullImage"),
        }
    }

    // =====================================================================
    // ImageCache
    // =====================================================================

    #[test]
    fn image_cache_add_get_delete_round_trip() {
        let mut cache = ImageCache::new();
        let key = AzString::from_const_str("my_image");
        let img = ImageRef::null_image(1, 1, RawImageFormat::R8, Vec::new());
        let hash = img.get_hash();

        assert!(cache.get_css_image_id(&key).is_none());
        cache.add_css_image_id(key.clone(), img);
        assert_eq!(cache.get_css_image_id(&key).map(ImageRef::get_hash), Some(hash));

        // Re-inserting the same id replaces (does not duplicate).
        let img2 = ImageRef::null_image(2, 2, RawImageFormat::R8, Vec::new());
        let hash2 = img2.get_hash();
        cache.add_css_image_id(key.clone(), img2);
        assert_eq!(cache.image_id_map.len(), 1);
        assert_eq!(cache.get_css_image_id(&key).map(ImageRef::get_hash), Some(hash2));

        cache.delete_css_image_id(&key);
        assert!(cache.get_css_image_id(&key).is_none());
        assert!(cache.image_id_map.is_empty());
        // Deleting a missing id is a no-op, not a panic.
        cache.delete_css_image_id(&key);
        cache.delete_css_image_id(&AzString::from_const_str("never-existed"));
    }

    #[test]
    fn image_cache_handles_empty_and_unicode_keys() {
        let mut cache = ImageCache::new();
        let empty = AzString::from_const_str("");
        let unicode = AzString::from(String::from("\u{1F600}\u{0301}"));
        let long = AzString::from("k".repeat(100_000));

        cache.add_css_image_id(
            empty.clone(),
            ImageRef::null_image(1, 1, RawImageFormat::R8, Vec::new()),
        );
        cache.add_css_image_id(
            unicode.clone(),
            ImageRef::null_image(1, 1, RawImageFormat::R8, Vec::new()),
        );
        cache.add_css_image_id(
            long.clone(),
            ImageRef::null_image(1, 1, RawImageFormat::R8, Vec::new()),
        );

        assert_eq!(cache.image_id_map.len(), 3);
        assert!(cache.get_css_image_id(&empty).is_some());
        assert!(cache.get_css_image_id(&unicode).is_some());
        assert!(cache.get_css_image_id(&long).is_some());
        // Distinct keys must not alias each other.
        assert!(cache
            .get_css_image_id(&AzString::from_const_str("\u{1F600}"))
            .is_none());
    }

    // =====================================================================
    // RendererResources
    // =====================================================================

    #[test]
    fn renderer_resources_lookups_on_an_empty_registry_are_none() {
        let rr = RendererResources::default();
        let ns = IdNamespace(1);
        assert!(rr
            .get_renderable_font_data(&FontInstanceKey::unique(ns))
            .is_none());
        let families = StyleFontFamiliesHash::new(&[]);
        assert!(rr
            .get_font_instance_key(&families, Au(0), DpiScaleFactor::new(1.0))
            .is_none());
        assert!(rr
            .get_font_instance_key(&families, Au(MAX_AU), DpiScaleFactor::new(f32::NAN))
            .is_none());
        assert!(rr.get_image(&ImageRefHash { inner: 0 }).is_none());
        assert!(rr.get_font_key(&StyleFontFamilyHash::new(&StyleFontFamily::System(
            AzString::from_const_str("Arial")
        ))).is_none());
    }

    #[test]
    fn renderer_resources_gc_helper_is_a_noop_on_empty_maps() {
        // The private helper must not panic (or wrongly prune) on empty maps.
        let mut rr = RendererResources::default();
        rr.remove_font_families_with_zero_references();
        assert!(rr.font_id_map.is_empty());
        assert!(rr.font_families_map.is_empty());

        // A family whose FontKey is NOT registered gets pruned; the families
        // map entry pointing at it is pruned too.
        let family = StyleFontFamily::System(AzString::from_const_str("Arial"));
        let family_hash = StyleFontFamilyHash::new(&family);
        let families_hash = StyleFontFamiliesHash::new(core::slice::from_ref(&family));
        rr.font_id_map.insert(family_hash, FontKey::unique(IdNamespace(1)));
        rr.font_families_map.insert(families_hash, family_hash);
        rr.remove_font_families_with_zero_references();
        assert!(rr.font_id_map.is_empty(), "dangling font key must be pruned");
        assert!(rr.font_families_map.is_empty());
    }

    #[test]
    fn get_font_instance_key_for_text_is_none_on_empty_resources_for_all_sane_sizes() {
        // No fonts registered -> every lookup misses, whatever the size / DPI.
        // (0, negative and NaN sizes must not panic on the way to the miss.)
        let rr = RendererResources::default();
        let cache = CssPropertyCache::default();
        let node = NodeData::default();
        let node_id = NodeId::new(0);
        let state = StyledNodeState::default();

        for size in [0.0_f32, -0.0, 1.0, -12.0, f32::NAN, 1.0e6, -1.0e6] {
            for dpi in [1.0_f32, 0.0, -1.0, f32::NAN, f32::INFINITY] {
                assert!(
                    rr.get_font_instance_key_for_text(size, &cache, &node, &node_id, &state, dpi)
                        .is_none(),
                    "size={size} dpi={dpi} must miss cleanly"
                );
            }
        }
    }

    #[test]
    fn bug_get_font_instance_key_for_text_overflow_panics_on_infinite_font_size() {
        // `font_size_px as isize` saturates to isize::MAX for +inf / f32::MAX,
        // and FloatValue::const_new then computes `isize::MAX * 1000`, which
        // panics with "attempt to multiply with overflow" under the (default)
        // dev-profile overflow checks. A miss (None) is the correct behaviour.
        let rr = RendererResources::default();
        let cache = CssPropertyCache::default();
        let node = NodeData::default();
        let node_id = NodeId::new(0);
        let state = StyledNodeState::default();
        assert!(rr
            .get_font_instance_key_for_text(
                f32::INFINITY,
                &cache,
                &node,
                &node_id,
                &state,
                1.0
            )
            .is_none());
    }

    // =====================================================================
    // Resource-update builders (end-to-end)
    // =====================================================================

    #[test]
    fn font_ref_get_hash_is_stable_per_font_and_distinct_across_fonts() {
        let a = dummy_font_ref();
        let b = dummy_font_ref();
        assert_eq!(font_ref_get_hash(&a), font_ref_get_hash(&a));
        assert_eq!(font_ref_get_hash(&a), font_ref_get_hash(&a.clone()));
        assert_ne!(
            font_ref_get_hash(&a),
            font_ref_get_hash(&b),
            "two distinct FontRefs must not share a hash"
        );
    }

    #[test]
    fn build_add_font_resource_updates_on_empty_input_is_empty() {
        let mut rr = RendererResources::default();
        let fonts = OrderedMap::new();
        let updates = build_add_font_resource_updates(
            &mut rr,
            DpiScaleFactor::new(1.0),
            &FcFontCache::default(),
            IdNamespace(1),
            &fonts,
            load_font_none,
            parse_font_none,
        );
        assert!(updates.is_empty());
        assert!(rr.font_id_map.is_empty());
    }

    #[test]
    fn build_add_font_resource_updates_skips_unloadable_fonts() {
        // A family that cannot be loaded (missing file) must not register
        // anything - it is retried next frame, not half-registered.
        let mut rr = RendererResources::default();
        let mut fonts = OrderedMap::new();
        let mut sizes = FastBTreeSet::new();
        sizes.insert(Au::from_px(16.0));
        fonts.insert(
            ImmediateFontId::Unresolved(StyleFontFamilyVec::from_vec(vec![
                StyleFontFamily::System(AzString::from_const_str("DoesNotExist")),
            ])),
            sizes,
        );

        let updates = build_add_font_resource_updates(
            &mut rr,
            DpiScaleFactor::new(1.0),
            &FcFontCache::default(),
            IdNamespace(1),
            &fonts,
            load_font_none,
            parse_font_none,
        );
        assert!(updates.is_empty(), "an unloadable font must add no resources");
        assert!(rr.font_id_map.is_empty());
        assert!(rr.font_families_map.is_empty());
    }

    #[test]
    fn build_add_font_resource_updates_registers_a_font_and_deduplicates_sizes() {
        // A StyleFontFamily::Ref resolves without touching the loader, so this
        // exercises the whole add-font path deterministically.
        let mut rr = RendererResources::default();
        let font = dummy_font_ref();
        let family = StyleFontFamily::Ref(font.clone());
        let dpi = DpiScaleFactor::new(1.0);

        let mut sizes = FastBTreeSet::new();
        sizes.insert(Au::from_px(16.0));
        sizes.insert(Au::from_px(24.0));
        sizes.insert(Au::from_px(16.0)); // duplicate -> set dedups it
        assert_eq!(sizes.len(), 2);

        let mut fonts = OrderedMap::new();
        fonts.insert(
            ImmediateFontId::Unresolved(StyleFontFamilyVec::from_vec(vec![family.clone()])),
            sizes,
        );

        let updates = build_add_font_resource_updates(
            &mut rr,
            dpi,
            &FcFontCache::default(),
            IdNamespace(1),
            &fonts,
            load_font_none,
            parse_font_none,
        );
        // 1 AddFont + 2 AddFontInstance
        assert_eq!(updates.len(), 3);
        assert_eq!(
            updates
                .iter()
                .filter(|(_, m)| matches!(m, AddFontMsg::Font(..)))
                .count(),
            1
        );
        assert_eq!(
            updates
                .iter()
                .filter(|(_, m)| matches!(m, AddFontMsg::Instance(..)))
                .count(),
            2
        );
        assert_eq!(rr.font_id_map.len(), 1);
        assert_eq!(rr.font_families_map.len(), 1);

        // add_resources then makes the instances findable by (families, size, dpi).
        let mut all_updates = Vec::new();
        add_resources(&mut rr, &mut all_updates, updates, Vec::new());
        assert_eq!(all_updates.len(), 3);

        let families_hash = StyleFontFamiliesHash::new(core::slice::from_ref(&family));
        assert!(rr
            .get_font_instance_key(&families_hash, Au::from_px(16.0), dpi)
            .is_some());
        assert!(rr
            .get_font_instance_key(&families_hash, Au::from_px(24.0), dpi)
            .is_some());
        // A size that was never registered misses; so does a different DPI.
        assert!(rr
            .get_font_instance_key(&families_hash, Au::from_px(99.0), dpi)
            .is_none());
        assert!(rr
            .get_font_instance_key(&families_hash, Au::from_px(16.0), DpiScaleFactor::new(2.0))
            .is_none());

        // The instance key resolves back to the font (reverse lookup invariant).
        let key = rr
            .get_font_instance_key(&families_hash, Au::from_px(16.0), dpi)
            .expect("registered");
        let (font_ref, au, got_dpi) = rr
            .get_renderable_font_data(&key)
            .expect("registered instance must be renderable");
        assert_eq!(font_ref.get_hash(), font.get_hash());
        assert_eq!(au, Au::from_px(16.0));
        assert_eq!(got_dpi, dpi);

        // Rebuilding with the same font must not add anything a second time.
        let again = build_add_font_resource_updates(
            &mut rr,
            dpi,
            &FcFontCache::default(),
            IdNamespace(1),
            &fonts,
            load_font_none,
            parse_font_none,
        );
        assert!(again.is_empty(), "already-registered fonts must not be re-added");
    }

    #[test]
    fn add_font_msg_into_resource_update_preserves_keys() {
        let font = dummy_font_ref();
        let key = FontKey::unique(IdNamespace(3));
        let family_hash = StyleFontFamilyHash::new(&StyleFontFamily::Ref(font.clone()));
        let msg = AddFontMsg::Font(key, family_hash, font.clone());
        match msg.into_resource_update() {
            ResourceUpdate::AddFont(add) => {
                assert_eq!(add.key, key);
                assert_eq!(add.font.get_hash(), font.get_hash());
            }
            other => panic!("expected AddFont, got {other:?}"),
        }
    }

    #[test]
    fn delete_font_msg_into_resource_update_preserves_keys() {
        let fk = FontKey::unique(IdNamespace(1));
        match DeleteFontMsg::Font(fk).into_resource_update() {
            ResourceUpdate::DeleteFont(k) => assert_eq!(k, fk),
            other => panic!("expected DeleteFont, got {other:?}"),
        }
        let fik = FontInstanceKey::unique(IdNamespace(1));
        let size = (Au::from_px(16.0), DpiScaleFactor::new(1.0));
        match DeleteFontMsg::Instance(fik, size).into_resource_update() {
            ResourceUpdate::DeleteFontInstance(k) => assert_eq!(k, fik),
            other => panic!("expected DeleteFontInstance, got {other:?}"),
        }
    }

    #[test]
    fn add_image_msg_into_resource_update_preserves_the_key_and_descriptor() {
        let key = ImageKey::unique(IdNamespace(2));
        let descriptor = ImageDescriptor {
            format: RawImageFormat::BGRA8,
            width: 3,
            height: 5,
            stride: None.into(),
            offset: 0,
            flags: ImageDescriptorFlags {
                is_opaque: false,
                allow_mipmaps: true,
            },
        };
        let msg = AddImageMsg(AddImage {
            key,
            descriptor,
            data: ImageData::Raw(SharedRawImageData::new(vec![0u8; 60].into())),
            tiling: None,
        });
        match msg.into_resource_update() {
            ResourceUpdate::AddImage(add) => {
                assert_eq!(add.key, key);
                assert_eq!(add.descriptor, descriptor);
                assert!(add.tiling.is_none());
            }
            other => panic!("expected AddImage, got {other:?}"),
        }
    }

    #[test]
    fn build_add_image_resource_updates_skips_null_and_callback_images() {
        // NullImage has nothing to upload, Callback runs after layout.
        let rr = RendererResources::default();
        let mut images = FastBTreeSet::new();
        images.insert(ImageRef::null_image(4, 4, RawImageFormat::RGBA8, Vec::new()));
        images.insert(ImageRef::callback(0usize, RefAny::new(0u8)));

        let updates = build_add_image_resource_updates(
            &rr,
            IdNamespace(1),
            Epoch::new(),
            &test_document_id(),
            &images,
            store_gl_texture_noop,
        );
        assert!(updates.is_empty());

        // ... and an empty DOM produces no updates at all.
        let empty = FastBTreeSet::new();
        assert!(build_add_image_resource_updates(
            &rr,
            IdNamespace(1),
            Epoch::new(),
            &test_document_id(),
            &empty,
            store_gl_texture_noop,
        )
        .is_empty());
    }

    #[test]
    fn build_add_image_resource_updates_then_add_resources_round_trip() {
        let mut rr = RendererResources::default();
        let img = ImageRef::new_rawimage(rgba8_image(2, 2)).expect("valid 2x2");
        let hash = img.get_hash();
        let ns = IdNamespace(11);

        let mut images = FastBTreeSet::new();
        images.insert(img.clone());

        let updates = build_add_image_resource_updates(
            &rr,
            ns,
            Epoch::new(),
            &test_document_id(),
            &images,
            store_gl_texture_noop,
        );
        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].0, hash);
        // The ImageKey is derived from the hash (no separate mapping table).
        assert_eq!(updates[0].1 .0.key, image_ref_hash_to_image_key(hash, ns));
        assert_eq!(updates[0].1 .0.descriptor.width, 2);
        assert_eq!(updates[0].1 .0.descriptor.height, 2);

        let key = updates[0].1 .0.key;
        let mut all_updates = Vec::new();
        add_resources(&mut rr, &mut all_updates, Vec::new(), updates);
        assert_eq!(all_updates.len(), 1);
        assert!(matches!(all_updates[0], ResourceUpdate::AddImage(_)));

        // Forward and reverse maps must agree after registration.
        assert_eq!(rr.get_image(&hash).map(|r| r.key), Some(key));
        assert_eq!(rr.image_key_map.get(&key), Some(&hash));

        // An already-registered image is never re-uploaded.
        let again = build_add_image_resource_updates(
            &rr,
            ns,
            Epoch::new(),
            &test_document_id(),
            &images,
            store_gl_texture_noop,
        );
        assert!(again.is_empty());

        // update_image mutates the descriptor in place, keeping the key.
        let new_descriptor = ImageDescriptor {
            format: RawImageFormat::BGRA8,
            width: 8,
            height: 8,
            stride: None.into(),
            offset: 0,
            flags: ImageDescriptorFlags {
                is_opaque: true,
                allow_mipmaps: true,
            },
        };
        rr.update_image(&hash, new_descriptor);
        assert_eq!(rr.get_image(&hash).map(|r| r.descriptor.width), Some(8));
        assert_eq!(rr.get_image(&hash).map(|r| r.key), Some(key));
        // Updating an unknown hash is a silent no-op, not a panic.
        rr.update_image(&ImageRefHash { inner: u64::MAX }, new_descriptor);
    }

    #[test]
    fn add_resources_with_empty_input_changes_nothing() {
        let mut rr = RendererResources::default();
        let mut updates = Vec::new();
        add_resources(&mut rr, &mut updates, Vec::new(), Vec::new());
        assert!(updates.is_empty());
        assert!(rr.currently_registered_images.is_empty());
        assert!(rr.currently_registered_fonts.is_empty());
        assert!(rr.image_key_map.is_empty());
    }

    // =====================================================================
    // NUMERIC: CPU painting (paint_dot / paint_stroke)
    // =====================================================================

    #[test]
    fn paint_dot_composites_at_the_center_and_leaves_far_pixels_alone() {
        let mut img = rgba8_image(4, 4);
        img.paint_dot(2.0, 2.0, Brush::new(opaque_red(), 2.0));
        let px = img.pixels.get_u8_vec_ref().expect("u8").as_ref().to_vec();

        // Pixel (1,1) is 0.71 px from the center -> inside the hard core.
        let idx = (4 + 1) * 4;
        assert_eq!(&px[idx..idx + 4], &[255, 0, 0, 255], "center pixel must be opaque red");
        // Pixel (0,0) is 2.12 px away -> outside the radius -> untouched.
        assert_eq!(&px[0..4], &[0, 0, 0, 0], "pixels beyond the radius stay untouched");
    }

    #[test]
    fn paint_dot_honours_bgra_channel_order() {
        let mut img = rgba8_image(4, 4);
        img.data_format = RawImageFormat::BGRA8;
        img.paint_dot(2.0, 2.0, Brush::new(opaque_red(), 2.0));
        let px = img.pixels.get_u8_vec_ref().expect("u8").as_ref().to_vec();
        let idx = (4 + 1) * 4;
        // BGRA: red lands in byte 2, blue in byte 0.
        assert_eq!(&px[idx..idx + 4], &[0, 0, 255, 255]);
    }

    #[test]
    fn paint_dot_rejects_degenerate_radii_and_sizes() {
        let untouched = |img: &RawImage| {
            img.pixels
                .get_u8_vec_ref()
                .expect("u8")
                .as_ref()
                .iter()
                .all(|b| *b == 0)
        };

        // radius <= 0 and NaN radius are no-ops (the `!(r > 0.0)` guard).
        for r in [0.0_f32, -1.0, -0.0, f32::NAN, f32::NEG_INFINITY] {
            let mut img = rgba8_image(4, 4);
            img.paint_dot(2.0, 2.0, Brush::new(opaque_red(), r));
            assert!(untouched(&img), "radius {r} must not paint");
        }

        // Zero-sized images are no-ops (and must not index out of bounds).
        let mut img = rgba8_image(0, 0);
        img.paint_dot(0.0, 0.0, Brush::new(opaque_red(), 4.0));
        assert_eq!(img.pixels.get_u8_vec_ref().map(|v| v.len()), Some(0));

        // Non-8-bit-RGBA formats are documented as left untouched.
        for format in [
            RawImageFormat::R8,
            RawImageFormat::RGB8,
            RawImageFormat::RGBA16,
            RawImageFormat::RGBAF32,
        ] {
            let mut img = rgba8_image(4, 4);
            img.data_format = format;
            img.paint_dot(2.0, 2.0, Brush::new(opaque_red(), 2.0));
            assert!(untouched(&img), "format {format:?} must not be painted");
        }
    }

    #[test]
    fn paint_dot_with_nan_and_infinite_coordinates_is_a_safe_noop() {
        // NaN / +-inf centers collapse the scan range to nothing instead of
        // producing an out-of-bounds index.
        for (cx, cy) in [
            (f32::NAN, 2.0_f32),
            (2.0, f32::NAN),
            (f32::NAN, f32::NAN),
            (f32::INFINITY, 2.0),
            (f32::NEG_INFINITY, 2.0),
            (2.0, f32::INFINITY),
            (1.0e30, 1.0e30),
            (-1.0e30, -1.0e30),
        ] {
            let mut img = rgba8_image(4, 4);
            img.paint_dot(cx, cy, Brush::new(opaque_red(), 2.0));
            let px = img.pixels.get_u8_vec_ref().expect("u8").as_ref().to_vec();
            assert!(
                px.iter().all(|b| *b == 0),
                "({cx}, {cy}) must not paint anything"
            );
        }
    }

    #[test]
    fn paint_dot_alpha_saturates_and_never_overflows() {
        // Repeatedly stamping an opaque dab must clamp at 255, never wrap.
        let mut img = rgba8_image(4, 4);
        let brush = Brush::new(opaque_red(), 2.0);
        for _ in 0..50 {
            img.paint_dot(2.0, 2.0, brush);
        }
        let px = img.pixels.get_u8_vec_ref().expect("u8").as_ref().to_vec();
        let idx = (4 + 1) * 4;
        assert_eq!(&px[idx..idx + 4], &[255, 0, 0, 255]);

        // A NaN hardness makes the coverage NaN; the `a <= 0.0` check is false
        // for NaN, so the blend runs with a NaN alpha -- the `.clamp(0, 255)`
        // on the result keeps every channel a valid u8 (NaN clamps to 0 here),
        // i.e. garbage-in stays in-range instead of wrapping.
        let mut img = rgba8_image(4, 4);
        let mut nan_brush = Brush::new(opaque_red(), 2.0);
        nan_brush.hardness = f32::NAN;
        img.paint_dot(2.0, 2.0, nan_brush);
        // No assertion on the exact value: the point is that it did not panic
        // and every byte is (trivially) a valid u8.
        assert_eq!(img.pixels.get_u8_vec_ref().map(|v| v.len()), Some(64));
    }

    #[test]
    fn paint_dot_zero_flow_and_transparent_color_do_not_paint() {
        let mut img = rgba8_image(4, 4);
        let mut brush = Brush::new(opaque_red(), 2.0);
        brush.flow = 0.0;
        img.paint_dot(2.0, 2.0, brush);
        assert!(img
            .pixels
            .get_u8_vec_ref()
            .expect("u8")
            .as_ref()
            .iter()
            .all(|b| *b == 0));

        let mut img = rgba8_image(4, 4);
        let transparent = ColorU {
            r: 255,
            g: 0,
            b: 0,
            a: 0,
        };
        img.paint_dot(2.0, 2.0, Brush::new(transparent, 2.0));
        assert!(img
            .pixels
            .get_u8_vec_ref()
            .expect("u8")
            .as_ref()
            .iter()
            .all(|b| *b == 0));

        // A negative / >1 flow is clamped, not extrapolated.
        let mut img = rgba8_image(4, 4);
        let mut brush = Brush::new(opaque_red(), 2.0);
        brush.flow = -5.0;
        img.paint_dot(2.0, 2.0, brush);
        assert!(img
            .pixels
            .get_u8_vec_ref()
            .expect("u8")
            .as_ref()
            .iter()
            .all(|b| *b == 0));
    }

    #[test]
    fn paint_stroke_paints_both_endpoints() {
        let mut img = rgba8_image(8, 8);
        img.paint_stroke(1.5, 1.5, 6.5, 6.5, Brush::new(opaque_red(), 1.5));
        let px = img.pixels.get_u8_vec_ref().expect("u8").as_ref().to_vec();
        let alpha_at = |x: usize, y: usize| px[(y * 8 + x) * 4 + 3];
        assert!(alpha_at(1, 1) > 0, "start of the stroke must be painted");
        assert!(alpha_at(6, 6) > 0, "end of the stroke must be painted");
        assert_eq!(alpha_at(7, 0), 0, "off-line pixels stay untouched");
    }

    #[test]
    fn paint_stroke_zero_length_stamps_a_single_dab() {
        // len == 0 -> n == 0 -> the `n <= 0` branch stamps one dab at (x1, y1).
        let mut img = rgba8_image(4, 4);
        img.paint_stroke(2.0, 2.0, 2.0, 2.0, Brush::new(opaque_red(), 2.0));
        let px = img.pixels.get_u8_vec_ref().expect("u8").as_ref().to_vec();
        let idx = (4 + 1) * 4;
        assert_eq!(&px[idx..idx + 4], &[255, 0, 0, 255]);
    }

    #[test]
    fn paint_stroke_degenerate_brush_params_do_not_divide_by_zero_or_hang() {
        // spacing == 0 / negative: the `.max(0.01)` and `.max(0.5)` floors keep
        // the step positive, so the dab count stays finite.
        for spacing in [0.0_f32, -1.0, f32::NAN] {
            let mut img = rgba8_image(8, 8);
            let mut brush = Brush::new(opaque_red(), 2.0);
            brush.spacing = spacing;
            img.paint_stroke(0.0, 0.0, 7.0, 7.0, brush);
            assert_eq!(img.pixels.get_u8_vec_ref().map(|v| v.len()), Some(8 * 8 * 4));
        }

        // A NaN endpoint yields a NaN length -> n == 0 -> one (no-op) dab.
        let mut img = rgba8_image(4, 4);
        img.paint_stroke(f32::NAN, 0.0, 1.0, 1.0, Brush::new(opaque_red(), 1.0));
        assert_eq!(img.pixels.get_u8_vec_ref().map(|v| v.len()), Some(64));

        // radius == 0 -> every dab is a no-op, and the loop still terminates.
        let mut img = rgba8_image(4, 4);
        img.paint_stroke(0.0, 0.0, 3.0, 3.0, Brush::new(opaque_red(), 0.0));
        assert!(img
            .pixels
            .get_u8_vec_ref()
            .expect("u8")
            .as_ref()
            .iter()
            .all(|b| *b == 0));
    }

    #[test]
    fn bug_paint_stroke_with_infinite_endpoint_loops_2_billion_times() {
        // `len` is +inf, `step` is finite, so `n = (inf / step).floor() as i32`
        // saturates to i32::MAX and the `for i in 0..=n` loop runs 2^31 times.
        // paint_stroke should clamp `n` (or bail on a non-finite length).
        let mut img = rgba8_image(4, 4);
        img.paint_stroke(0.0, 0.0, f32::INFINITY, 0.0, Brush::new(opaque_red(), 2.0));
    }

    #[test]
    fn bug_paint_dot_indexes_out_of_bounds_when_dims_exceed_the_buffer() {
        // RawImage's fields are all public, so a caller can hand paint_dot a
        // 100x100 image backed by 4 bytes. paint_dot computes the index from
        // width/height and indexes `buf` unchecked -> panic. It should clamp the
        // scan rect to the buffer length (or bail).
        let mut img = RawImage {
            pixels: RawImageData::U8(vec![0u8; 4].into()),
            width: 100,
            height: 100,
            premultiplied_alpha: true,
            data_format: RawImageFormat::RGBA8,
            tag: Vec::new().into(),
        };
        img.paint_dot(50.0, 50.0, Brush::new(opaque_red(), 4.0));
    }

    #[test]
    fn bug_into_loaded_image_source_overflows_on_huge_dimensions() {
        // Documented contract: "Returns None if the width * height * BPP does not
        // match". With width == usize::MAX the multiplication overflows and
        // panics under the dev-profile overflow checks instead.
        let img = RawImage {
            pixels: RawImageData::U8(vec![0u8; 4].into()),
            width: usize::MAX,
            height: 2,
            premultiplied_alpha: true,
            data_format: RawImageFormat::RGBA8,
            tag: Vec::new().into(),
        };
        assert!(img.into_loaded_image_source().is_none());
    }
}
