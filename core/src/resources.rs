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
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
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
        s.add_component_library(
            AzString::from_const_str("builtin"),
            crate::xml::register_builtin_components as extern "C" fn() -> ComponentLibrary,
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
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "IdNamespace({})", self.0)
    }
}

impl ::core::fmt::Debug for IdNamespace {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
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
    pub run_destructor: bool,
}

impl ImageRef {
    #[must_use] pub fn get_hash(&self) -> ImageRefHash {
        image_ref_get_hash(self)
    }
}

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd, Hash, Ord, Eq)]
#[repr(C)]
pub struct ImageRefHash {
    pub inner: usize,
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
        unsafe {
            if self.copies.as_ref().map(|m| m.load(AtomicOrdering::SeqCst)) == Some(1) {
                let data = Box::from_raw(self.data.cast_mut());
                let _ = Box::from_raw(self.copies.cast_mut());
                core::mem::forget(self); // do not run the destructor
                Some(*data)
            } else {
                None
            }
        }
    }

    #[must_use] pub const fn get_data(&self) -> &DecodedImage {
        unsafe { &*self.data }
    }

    #[must_use] pub fn get_image_callback(&self) -> Option<&CoreImageCallback> {
        if unsafe { self.copies.as_ref().map(|m| m.load(AtomicOrdering::SeqCst)) != Some(1) } {
            return None; // not safe
        }

        match unsafe { &*self.data } {
            DecodedImage::Callback(gl_texture_callback) => Some(gl_texture_callback),
            _ => None,
        }
    }

    pub fn get_image_callback_mut(&mut self) -> Option<&mut CoreImageCallback> {
        if unsafe { self.copies.as_ref().map(|m| m.load(AtomicOrdering::SeqCst)) != Some(1) } {
            return None; // not safe
        }

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
            run_destructor: true,
        }
    }

    // pub fn new_vulkan(...) -> Self
}

unsafe impl Send for ImageRef {}
unsafe impl Sync for ImageRef {}

impl PartialEq for ImageRef {
    fn eq(&self, rhs: &Self) -> bool {
        std::ptr::eq(self.data, rhs.data)
    }
}

impl PartialOrd for ImageRef {
    fn partial_cmp(&self, other: &Self) -> Option<::core::cmp::Ordering> {
        Some((self.data as usize).cmp(&(other.data as usize)))
    }
}

impl Ord for ImageRef {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        let self_data = self.data as usize;
        let other_data = other.data as usize;
        self_data.cmp(&other_data)
    }
}

impl Eq for ImageRef {}

impl Hash for ImageRef {
    fn hash<H>(&self, state: &mut H)
    where
        H: Hasher,
    {
        let self_data = self.data as usize;
        self_data.hash(state);
    }
}

impl Clone for ImageRef {
    fn clone(&self) -> Self {
        unsafe {
            self.copies
                .as_ref()
                .map(|m| m.fetch_add(1, AtomicOrdering::SeqCst));
        }
        Self {
            data: self.data,     // copy the pointer
            copies: self.copies, // copy the pointer
            run_destructor: true,
        }
    }
}

impl Drop for ImageRef {
    fn drop(&mut self) {
        self.run_destructor = false;
        unsafe {
            let copies = (*self.copies).fetch_sub(1, AtomicOrdering::SeqCst);
            if copies == 1 {
                let _ = Box::from_raw(self.data.cast_mut());
                let _ = Box::from_raw(self.copies.cast_mut());
            }
        }
    }
}

#[must_use] pub fn image_ref_get_hash(ir: &ImageRef) -> ImageRefHash {
    ImageRefHash {
        inner: ir.data as usize,
    }
}

/// Convert a stable `ImageRefHash` directly to an `ImageKey`.
///
/// `ImageKey.key` is a `u64`, so the pointer-derived `ImageRefHash.inner: usize`
/// round-trips losslessly on both 32- and 64-bit platforms without any folding.
#[must_use] pub const fn image_ref_hash_to_image_key(hash: ImageRefHash, namespace: IdNamespace) -> ImageKey {
    ImageKey {
        namespace,
        key: hash.inner as u64,
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
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
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
        // Convert font size to StyleFontSize
        let font_size = StyleFontSize {
            inner: azul_css::props::basic::PixelValue::const_px(font_size_px as isize),
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
    ((f32::from(core::u16::MAX) / f32::from(i)) * f32::from(core::u8::MAX)) as u8
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

        let expected_len = width * height;

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
        unsafe { (*self.data).as_ref() }
    }

    /// Alias for `as_ref()` - get the raw bytes as a slice
    #[must_use] pub fn get_bytes(&self) -> &[u8] {
        self.as_ref()
    }

    /// Get a pointer to the raw bytes for hashing/identification
    #[must_use] pub fn as_ptr(&self) -> *const u8 {
        unsafe { (*self.data).as_ref().as_ptr() }
    }

    /// Get the length of the data
    #[must_use] pub const fn len(&self) -> usize {
        unsafe { (*self.data).len() }
    }

    /// Check if the data is empty
    #[must_use] pub const fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Try to extract the `U8Vec` if this is the only reference
    /// Returns None if there are other references
    #[must_use] pub fn into_inner(self) -> Option<U8Vec> {
        unsafe {
            if self.copies.as_ref().map(|m| m.load(AtomicOrdering::SeqCst)) == Some(1) {
                let data = Box::from_raw(self.data.cast_mut());
                let _ = Box::from_raw(self.copies.cast_mut());
                core::mem::forget(self); // don't run the destructor
                Some(*data)
            } else {
                None
            }
        }
    }
}

unsafe impl Send for SharedRawImageData {}
unsafe impl Sync for SharedRawImageData {}

impl Clone for SharedRawImageData {
    fn clone(&self) -> Self {
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
        unsafe {
            let copies = (*self.copies).fetch_sub(1, AtomicOrdering::SeqCst);
            if copies == 1 {
                let _ = Box::from_raw(self.data.cast_mut());
                let _ = Box::from_raw(self.copies.cast_mut());
            }
        }
    }
}

impl PartialEq for SharedRawImageData {
    fn eq(&self, rhs: &Self) -> bool {
        std::ptr::eq(self.data, rhs.data)
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

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd)]
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
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd)]
#[repr(C)]
pub struct OutlineMoveTo {
    pub x: i16,
    pub y: i16,
}

// LineTo in em units
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd)]
#[repr(C)]
pub struct OutlineLineTo {
    pub x: i16,
    pub y: i16,
}

// QuadTo in em units
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd)]
#[repr(C)]
pub struct OutlineQuadTo {
    pub ctrl_1_x: i16,
    pub ctrl_1_y: i16,
    pub end_x: i16,
    pub end_y: i16,
}

// CubicTo in em units
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd)]
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

#[derive(Debug, Clone)]
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
#[derive(Debug, Clone, Eq, Hash, PartialEq, PartialOrd, Ord)]
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
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
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
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
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

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone)]
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
        inner: hash.inner as u64,
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
    let mut resource_updates = alloc::vec::Vec::new();
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
