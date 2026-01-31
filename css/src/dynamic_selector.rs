//! Dynamic CSS selectors for runtime evaluation based on OS, media queries, container queries, etc.

use crate::corety::{AzString, OptionString};
use crate::props::property::CssProperty;

/// State flags for pseudo-classes (used in DynamicSelectorContext)
/// Note: This is a CSS-only version. See azul_core::styled_dom::StyledNodeState for the main type.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct PseudoStateFlags {
    pub hover: bool,
    pub active: bool,
    pub focused: bool,
    pub disabled: bool,
    pub checked: bool,
    pub focus_within: bool,
    pub visited: bool,
}

impl PseudoStateFlags {
    /// Check if a specific pseudo-state is active
    pub fn has_state(&self, state: PseudoStateType) -> bool {
        match state {
            PseudoStateType::Normal => true,
            PseudoStateType::Hover => self.hover,
            PseudoStateType::Active => self.active,
            PseudoStateType::Focus => self.focused,
            PseudoStateType::Disabled => self.disabled,
            PseudoStateType::Checked => self.checked,
            PseudoStateType::FocusWithin => self.focus_within,
            PseudoStateType::Visited => self.visited,
        }
    }
}

/// Dynamic selector that is evaluated at runtime
/// C-compatible: Tagged union with single field
#[repr(C, u8)]
#[derive(Debug, Clone, PartialEq)]
pub enum DynamicSelector {
    /// Operating system condition
    Os(OsCondition) = 0,
    /// Operating system version (e.g. macOS 14.0, Windows 11)
    OsVersion(OsVersionCondition) = 1,
    /// Media query (print/screen)
    Media(MediaType) = 2,
    /// Viewport width min/max (for @media)
    ViewportWidth(MinMaxRange) = 3,
    /// Viewport height min/max (for @media)
    ViewportHeight(MinMaxRange) = 4,
    /// Container width min/max (for @container)
    ContainerWidth(MinMaxRange) = 5,
    /// Container height min/max (for @container)
    ContainerHeight(MinMaxRange) = 6,
    /// Container name (for named @container queries)
    ContainerName(AzString) = 7,
    /// Theme (dark/light/custom)
    Theme(ThemeCondition) = 8,
    /// Aspect Ratio (min/max for @media and @container)
    AspectRatio(MinMaxRange) = 9,
    /// Orientation (portrait/landscape)
    Orientation(OrientationType) = 10,
    /// Reduced Motion (accessibility)
    PrefersReducedMotion(BoolCondition) = 11,
    /// High Contrast (accessibility)
    PrefersHighContrast(BoolCondition) = 12,
    /// Pseudo-State (hover, active, focus, etc.)
    PseudoState(PseudoStateType) = 13,
    /// Language/Locale (for @lang("de-DE"))
    /// Matches BCP 47 language tags (e.g., "de", "de-DE", "en-US")
    Language(LanguageCondition) = 14,
}

impl_option!(
    DynamicSelector,
    OptionDynamicSelector,
    copy = false,
    [Debug, Clone, PartialEq]
);

impl_vec!(DynamicSelector, DynamicSelectorVec, DynamicSelectorVecDestructor, DynamicSelectorVecDestructorType, DynamicSelectorVecSlice, OptionDynamicSelector);
impl_vec_clone!(
    DynamicSelector,
    DynamicSelectorVec,
    DynamicSelectorVecDestructor
);
impl_vec_debug!(DynamicSelector, DynamicSelectorVec);
impl_vec_partialeq!(DynamicSelector, DynamicSelectorVec);

/// Min/Max Range for numeric conditions (C-compatible)
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct MinMaxRange {
    /// Minimum value (NaN = no minimum limit)
    pub min: f32,
    /// Maximum value (NaN = no maximum limit)
    pub max: f32,
}

impl MinMaxRange {
    pub const fn new(min: Option<f32>, max: Option<f32>) -> Self {
        Self {
            min: if let Some(m) = min { m } else { f32::NAN },
            max: if let Some(m) = max { m } else { f32::NAN },
        }
    }

    pub fn min(&self) -> Option<f32> {
        if self.min.is_nan() {
            None
        } else {
            Some(self.min)
        }
    }

    pub fn max(&self) -> Option<f32> {
        if self.max.is_nan() {
            None
        } else {
            Some(self.max)
        }
    }

    pub fn matches(&self, value: f32) -> bool {
        let min_ok = self.min.is_nan() || value >= self.min;
        let max_ok = self.max.is_nan() || value <= self.max;
        min_ok && max_ok
    }
}

/// Boolean condition (C-compatible)
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BoolCondition {
    False,
    True,
}

impl From<bool> for BoolCondition {
    fn from(b: bool) -> Self {
        if b {
            Self::True
        } else {
            Self::False
        }
    }
}

impl From<BoolCondition> for bool {
    fn from(b: BoolCondition) -> Self {
        matches!(b, BoolCondition::True)
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OsCondition {
    Any,
    Apple, // macOS + iOS
    MacOS,
    IOS,
    Linux,
    Windows,
    Android,
    Web, // WASM
}

impl OsCondition {
    /// Convert from css::system::Platform
    pub fn from_system_platform(platform: &crate::system::Platform) -> Self {
        use crate::system::Platform;
        match platform {
            Platform::Windows => OsCondition::Windows,
            Platform::MacOs => OsCondition::MacOS,
            Platform::Linux(_) => OsCondition::Linux,
            Platform::Android => OsCondition::Android,
            Platform::Ios => OsCondition::IOS,
            Platform::Unknown => OsCondition::Any,
        }
    }
}

#[repr(C, u8)]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum OsVersionCondition {
    /// Semantic version: "14.0", "11.0.22000"
    Exact(AzString),
    /// Minimum version: >= "14.0"
    Min(AzString),
    /// Maximum version: <= "14.0"
    Max(AzString),
    /// Desktop environment (Linux)
    DesktopEnvironment(LinuxDesktopEnv),
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LinuxDesktopEnv {
    Gnome,
    KDE,
    XFCE,
    Unity,
    Cinnamon,
    MATE,
    Other,
}

impl LinuxDesktopEnv {
    /// Convert from css::system::DesktopEnvironment
    pub fn from_system_desktop_env(de: &crate::system::DesktopEnvironment) -> Self {
        use crate::system::DesktopEnvironment;
        match de {
            DesktopEnvironment::Gnome => LinuxDesktopEnv::Gnome,
            DesktopEnvironment::Kde => LinuxDesktopEnv::KDE,
            DesktopEnvironment::Other(_) => LinuxDesktopEnv::Other,
        }
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MediaType {
    Screen,
    Print,
    All,
}

#[repr(C, u8)]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ThemeCondition {
    Light,
    Dark,
    Custom(AzString),
    /// System preference
    SystemPreferred,
}

impl ThemeCondition {
    /// Convert from css::system::Theme
    pub fn from_system_theme(theme: crate::system::Theme) -> Self {
        use crate::system::Theme;
        match theme {
            Theme::Light => ThemeCondition::Light,
            Theme::Dark => ThemeCondition::Dark,
        }
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OrientationType {
    Portrait,
    Landscape,
}

/// Language/Locale condition for @lang() CSS selector
/// Matches BCP 47 language tags with prefix matching
#[repr(C, u8)]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum LanguageCondition {
    /// Exact match (e.g., "de-DE" matches only "de-DE")
    Exact(AzString),
    /// Prefix match (e.g., "de" matches "de", "de-DE", "de-AT", etc.)
    Prefix(AzString),
}

impl LanguageCondition {
    /// Check if this condition matches the given language tag
    pub fn matches(&self, language: &str) -> bool {
        match self {
            LanguageCondition::Exact(lang) => language.eq_ignore_ascii_case(lang.as_str()),
            LanguageCondition::Prefix(prefix) => {
                let prefix_str = prefix.as_str();
                if language.len() < prefix_str.len() {
                    return false;
                }
                // Check if language starts with prefix (case-insensitive)
                let lang_prefix = &language[..prefix_str.len()];
                if !lang_prefix.eq_ignore_ascii_case(prefix_str) {
                    return false;
                }
                // Must be exact match or followed by '-'
                language.len() == prefix_str.len()
                    || language.as_bytes().get(prefix_str.len()) == Some(&b'-')
            }
        }
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PseudoStateType {
    /// No special state (corresponds to "Normal" in NodeDataInlineCssProperty)
    Normal,
    /// Element is being hovered (:hover)
    Hover,
    /// Element is active/being clicked (:active)
    Active,
    /// Element has focus (:focus)
    Focus,
    /// Element is disabled (:disabled)
    Disabled,
    /// Element is checked/selected (:checked)
    Checked,
    /// Element or child has focus (:focus-within)
    FocusWithin,
    /// Link has been visited (:visited)
    Visited,
}

impl_option!(
    LinuxDesktopEnv,
    OptionLinuxDesktopEnv,
    [Debug, Clone, Copy, PartialEq, Eq, Hash]
);

/// Context for evaluating dynamic selectors
#[repr(C)]
#[derive(Debug, Clone)]
pub struct DynamicSelectorContext {
    /// Operating system info
    pub os: OsCondition,
    pub os_version: AzString,
    pub desktop_env: OptionLinuxDesktopEnv,

    /// Theme info
    pub theme: ThemeCondition,

    /// Media info (from WindowState)
    pub media_type: MediaType,
    pub viewport_width: f32,
    pub viewport_height: f32,

    /// Container info (from parent node)
    /// NaN = no container
    pub container_width: f32,
    pub container_height: f32,
    pub container_name: OptionString,

    /// Accessibility preferences
    pub prefers_reduced_motion: BoolCondition,
    pub prefers_high_contrast: BoolCondition,

    /// Orientation
    pub orientation: OrientationType,

    /// Node state (hover, active, focus, disabled, checked, focus_within, visited)
    pub pseudo_state: PseudoStateFlags,

    /// Language/Locale (BCP 47 tag, e.g., "en-US", "de-DE")
    pub language: AzString,
}

impl Default for DynamicSelectorContext {
    fn default() -> Self {
        Self {
            os: OsCondition::Any,
            os_version: AzString::from_const_str("0.0"),
            desktop_env: OptionLinuxDesktopEnv::None,
            theme: ThemeCondition::Light,
            media_type: MediaType::Screen,
            viewport_width: 800.0,
            viewport_height: 600.0,
            container_width: f32::NAN,
            container_height: f32::NAN,
            container_name: OptionString::None,
            prefers_reduced_motion: BoolCondition::False,
            prefers_high_contrast: BoolCondition::False,
            orientation: OrientationType::Landscape,
            pseudo_state: PseudoStateFlags::default(),
            language: AzString::from_const_str("en-US"),
        }
    }
}

impl DynamicSelectorContext {
    /// Create a context from SystemStyle
    pub fn from_system_style(system_style: &crate::system::SystemStyle) -> Self {
        let os = OsCondition::from_system_platform(&system_style.platform);
        let desktop_env = if let crate::system::Platform::Linux(de) = &system_style.platform {
            OptionLinuxDesktopEnv::Some(LinuxDesktopEnv::from_system_desktop_env(de))
        } else {
            OptionLinuxDesktopEnv::None
        };
        let theme = ThemeCondition::from_system_theme(system_style.theme);

        Self {
            os,
            os_version: AzString::from_const_str("0.0"), // TODO: Version detection
            desktop_env,
            theme,
            media_type: MediaType::Screen,
            viewport_width: 800.0, // Will be updated with window size
            viewport_height: 600.0,
            container_width: f32::NAN,
            container_height: f32::NAN,
            container_name: OptionString::None,
            prefers_reduced_motion: BoolCondition::False, // TODO: Accessibility
            prefers_high_contrast: BoolCondition::False,
            orientation: OrientationType::Landscape,
            pseudo_state: PseudoStateFlags::default(),
            language: system_style.language.clone(),
        }
    }

    /// Update viewport dimensions (e.g., on window resize)
    pub fn with_viewport(&self, width: f32, height: f32) -> Self {
        let mut ctx = self.clone();
        ctx.viewport_width = width;
        ctx.viewport_height = height;
        ctx.orientation = if width > height {
            OrientationType::Landscape
        } else {
            OrientationType::Portrait
        };
        ctx
    }

    /// Update container dimensions (for @container queries)
    pub fn with_container(&self, width: f32, height: f32, name: Option<AzString>) -> Self {
        let mut ctx = self.clone();
        ctx.container_width = width;
        ctx.container_height = height;
        ctx.container_name = name.into();
        ctx
    }

    /// Update pseudo-state (hover, active, focus, etc.)
    pub fn with_pseudo_state(&self, state: PseudoStateFlags) -> Self {
        let mut ctx = self.clone();
        ctx.pseudo_state = state;
        ctx
    }

    /// Check if viewport changed significantly (for breakpoint detection)
    pub fn viewport_breakpoint_changed(&self, other: &Self, breakpoints: &[f32]) -> bool {
        for bp in breakpoints {
            let self_above = self.viewport_width >= *bp;
            let other_above = other.viewport_width >= *bp;
            if self_above != other_above {
                return true;
            }
        }
        false
    }
}

impl DynamicSelector {
    /// Check if this selector matches in the given context
    pub fn matches(&self, ctx: &DynamicSelectorContext) -> bool {
        match self {
            Self::Os(os) => Self::match_os(*os, ctx.os),
            Self::OsVersion(ver) => Self::match_os_version(ver, &ctx.os_version, &ctx.desktop_env),
            Self::Media(media) => *media == ctx.media_type || *media == MediaType::All,
            Self::ViewportWidth(range) => range.matches(ctx.viewport_width),
            Self::ViewportHeight(range) => range.matches(ctx.viewport_height),
            Self::ContainerWidth(range) => {
                !ctx.container_width.is_nan() && range.matches(ctx.container_width)
            }
            Self::ContainerHeight(range) => {
                !ctx.container_height.is_nan() && range.matches(ctx.container_height)
            }
            Self::ContainerName(name) => ctx.container_name.as_ref().map_or(false, |n| n == name),
            Self::Theme(theme) => Self::match_theme(theme, &ctx.theme),
            Self::AspectRatio(range) => {
                let ratio = ctx.viewport_width / ctx.viewport_height.max(1.0);
                range.matches(ratio)
            }
            Self::Orientation(orient) => *orient == ctx.orientation,
            Self::PrefersReducedMotion(pref) => {
                bool::from(*pref) == bool::from(ctx.prefers_reduced_motion)
            }
            Self::PrefersHighContrast(pref) => {
                bool::from(*pref) == bool::from(ctx.prefers_high_contrast)
            }
            Self::PseudoState(state) => Self::match_pseudo_state(*state, &ctx.pseudo_state),
            Self::Language(lang_cond) => lang_cond.matches(ctx.language.as_str()),
        }
    }

    fn match_os(condition: OsCondition, actual: OsCondition) -> bool {
        match condition {
            OsCondition::Any => true,
            OsCondition::Apple => matches!(actual, OsCondition::MacOS | OsCondition::IOS),
            _ => condition == actual,
        }
    }

    fn match_os_version(
        condition: &OsVersionCondition,
        actual: &AzString,
        desktop_env: &OptionLinuxDesktopEnv,
    ) -> bool {
        match condition {
            OsVersionCondition::Exact(ver) => ver == actual,
            OsVersionCondition::Min(ver) => Self::compare_version(actual, ver) >= 0,
            OsVersionCondition::Max(ver) => Self::compare_version(actual, ver) <= 0,
            OsVersionCondition::DesktopEnvironment(env) => {
                desktop_env.as_ref().map_or(false, |de| de == env)
            }
        }
    }

    fn compare_version(a: &AzString, b: &AzString) -> i32 {
        // Simple string comparison for now
        // TODO: Proper semantic version comparison
        a.as_str().cmp(b.as_str()) as i32
    }

    fn match_theme(condition: &ThemeCondition, actual: &ThemeCondition) -> bool {
        match (condition, actual) {
            (ThemeCondition::SystemPreferred, _) => true,
            _ => condition == actual,
        }
    }

    fn match_pseudo_state(state: PseudoStateType, node_state: &PseudoStateFlags) -> bool {
        match state {
            PseudoStateType::Normal => true, // Normal is always active (base state)
            PseudoStateType::Hover => node_state.hover,
            PseudoStateType::Active => node_state.active,
            PseudoStateType::Focus => node_state.focused,
            PseudoStateType::Disabled => node_state.disabled,
            PseudoStateType::Checked => node_state.checked,
            PseudoStateType::FocusWithin => node_state.focus_within,
            PseudoStateType::Visited => node_state.visited,
        }
    }
}

// ============================================================================
// CssPropertyWithConditions - Replacement for NodeDataInlineCssProperty
// ============================================================================

/// A CSS property with optional conditions for when it should be applied.
/// This replaces `NodeDataInlineCssProperty` with a more flexible system.
///
/// If `apply_if` is empty, the property always applies.
/// If `apply_if` contains conditions, ALL conditions must be satisfied for the property to apply.
#[repr(C)]
#[derive(Debug, Clone, PartialEq)]
pub struct CssPropertyWithConditions {
    /// The actual CSS property value
    pub property: CssProperty,
    /// Conditions that must all be satisfied for this property to apply.
    /// Empty means unconditional (always apply).
    pub apply_if: DynamicSelectorVec,
}

impl_option!(
    CssPropertyWithConditions,
    OptionCssPropertyWithConditions,
    copy = false,
    [Debug, Clone, PartialEq, PartialOrd]
);

impl Eq for CssPropertyWithConditions {}

impl PartialOrd for CssPropertyWithConditions {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for CssPropertyWithConditions {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        // Compare by condition count only (simple stable ordering)
        self.apply_if
            .as_slice()
            .len()
            .cmp(&other.apply_if.as_slice().len())
    }
}

impl CssPropertyWithConditions {
    /// Create an unconditional property (always applies) - const version
    pub const fn simple(property: CssProperty) -> Self {
        Self {
            property,
            apply_if: DynamicSelectorVec::from_const_slice(&[]),
        }
    }

    /// Create a property with a single condition (const version using slice reference)
    pub const fn with_single_condition(
        property: CssProperty,
        conditions: &'static [DynamicSelector],
    ) -> Self {
        Self {
            property,
            apply_if: DynamicSelectorVec::from_const_slice(conditions),
        }
    }

    /// Create a property with a single condition (non-const, allocates)
    pub fn with_condition(property: CssProperty, condition: DynamicSelector) -> Self {
        Self {
            property,
            apply_if: DynamicSelectorVec::from_vec(vec![condition]),
        }
    }

    /// Create a property with multiple conditions (all must match)
    pub const fn with_conditions(property: CssProperty, conditions: DynamicSelectorVec) -> Self {
        Self {
            property,
            apply_if: conditions,
        }
    }

    /// Create a property that applies only on hover (const version)
    pub const fn on_hover(property: CssProperty) -> Self {
        Self::with_single_condition(
            property,
            &[DynamicSelector::PseudoState(PseudoStateType::Hover)],
        )
    }

    /// Create a property that applies only when active (const version)
    pub const fn on_active(property: CssProperty) -> Self {
        Self::with_single_condition(
            property,
            &[DynamicSelector::PseudoState(PseudoStateType::Active)],
        )
    }

    /// Create a property that applies only when focused (const version)
    pub const fn on_focus(property: CssProperty) -> Self {
        Self::with_single_condition(
            property,
            &[DynamicSelector::PseudoState(PseudoStateType::Focus)],
        )
    }

    /// Create a property that applies only when disabled (const version)
    pub const fn when_disabled(property: CssProperty) -> Self {
        Self::with_single_condition(
            property,
            &[DynamicSelector::PseudoState(PseudoStateType::Disabled)],
        )
    }

    /// Create a property that applies only on a specific OS (non-const, needs runtime value)
    pub fn on_os(property: CssProperty, os: OsCondition) -> Self {
        Self::with_condition(property, DynamicSelector::Os(os))
    }

    /// Create a property that applies only in dark theme (const version)
    pub const fn dark_theme(property: CssProperty) -> Self {
        Self::with_single_condition(property, &[DynamicSelector::Theme(ThemeCondition::Dark)])
    }

    /// Create a property that applies only in light theme (const version)
    pub const fn light_theme(property: CssProperty) -> Self {
        Self::with_single_condition(property, &[DynamicSelector::Theme(ThemeCondition::Light)])
    }

    /// Create a property for Windows only (const version)
    pub const fn on_windows(property: CssProperty) -> Self {
        Self::with_single_condition(property, &[DynamicSelector::Os(OsCondition::Windows)])
    }

    /// Create a property for macOS only (const version)
    pub const fn on_macos(property: CssProperty) -> Self {
        Self::with_single_condition(property, &[DynamicSelector::Os(OsCondition::MacOS)])
    }

    /// Create a property for Linux only (const version)
    pub const fn on_linux(property: CssProperty) -> Self {
        Self::with_single_condition(property, &[DynamicSelector::Os(OsCondition::Linux)])
    }

    /// Check if this property matches in the given context
    pub fn matches(&self, ctx: &DynamicSelectorContext) -> bool {
        // Empty conditions = always matches
        if self.apply_if.as_slice().is_empty() {
            return true;
        }

        // All conditions must match
        self.apply_if
            .as_slice()
            .iter()
            .all(|selector| selector.matches(ctx))
    }

    /// Check if this property has any conditions
    pub fn is_conditional(&self) -> bool {
        !self.apply_if.as_slice().is_empty()
    }

    /// Check if this property is a pseudo-state conditional only
    /// (hover, active, focus, etc.)
    pub fn is_pseudo_state_only(&self) -> bool {
        let conditions = self.apply_if.as_slice();
        !conditions.is_empty()
            && conditions
                .iter()
                .all(|c| matches!(c, DynamicSelector::PseudoState(_)))
    }

    /// Check if this property affects layout (width, height, margin, etc.)
    /// TODO: Implement when CssProperty has this method
    pub fn is_layout_affecting(&self) -> bool {
        // For now, assume all properties might affect layout
        // This should be implemented properly when property categories are available
        true
    }
}

impl_vec!(CssPropertyWithConditions, CssPropertyWithConditionsVec, CssPropertyWithConditionsVecDestructor, CssPropertyWithConditionsVecDestructorType, CssPropertyWithConditionsVecSlice, OptionCssPropertyWithConditions);
impl_vec_debug!(CssPropertyWithConditions, CssPropertyWithConditionsVec);
impl_vec_partialeq!(CssPropertyWithConditions, CssPropertyWithConditionsVec);
impl_vec_partialord!(CssPropertyWithConditions, CssPropertyWithConditionsVec);
impl_vec_clone!(
    CssPropertyWithConditions,
    CssPropertyWithConditionsVec,
    CssPropertyWithConditionsVecDestructor
);

// Manual implementations for Eq and Ord (required for NodeData derives)
impl Eq for CssPropertyWithConditionsVec {}

impl Ord for CssPropertyWithConditionsVec {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.as_slice().len().cmp(&other.as_slice().len())
    }
}

impl core::hash::Hash for CssPropertyWithConditions {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.property.hash(state);
        // DynamicSelectorVec doesn't implement Hash, so we hash the length
        self.apply_if.as_slice().len().hash(state);
    }
}

impl core::hash::Hash for CssPropertyWithConditionsVec {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        for item in self.as_slice() {
            item.hash(state);
        }
    }
}

impl CssPropertyWithConditionsVec {
    /// Parse CSS properties from a string, all with "normal" (unconditional) state
    #[cfg(feature = "parser")]
    pub fn parse_normal(style: &str) -> Self {
        use crate::props::property::{
            parse_combined_css_property, parse_css_property, CombinedCssPropertyType, CssKeyMap,
            CssPropertyType,
        };

        let mut props = Vec::new();
        let key_map = CssKeyMap::get();

        // Simple CSS parsing: split by semicolons and parse key:value pairs
        for pair in style.split(';') {
            let pair = pair.trim();
            if pair.is_empty() {
                continue;
            }
            if let Some((key, value)) = pair.split_once(':') {
                let key = key.trim();
                let value = value.trim();
                // First, try to parse as a regular (non-shorthand) property
                if let Some(prop_type) = CssPropertyType::from_str(key, &key_map) {
                    if let Ok(prop) = parse_css_property(prop_type, value) {
                        props.push(CssPropertyWithConditions::simple(prop));
                        continue;
                    }
                }
                // If not found, try as a shorthand (combined) property (e.g., overflow, margin, padding)
                if let Some(combined_type) = CombinedCssPropertyType::from_str(key, &key_map) {
                    if let Ok(expanded_props) = parse_combined_css_property(combined_type, value) {
                        for prop in expanded_props {
                            props.push(CssPropertyWithConditions::simple(prop));
                        }
                    }
                }
            }
        }

        CssPropertyWithConditionsVec::from_vec(props)
    }

    /// Parse CSS properties from a string, all with hover condition
    #[cfg(feature = "parser")]
    pub fn parse_hover(style: &str) -> Self {
        use crate::props::property::{
            parse_combined_css_property, parse_css_property, CombinedCssPropertyType, CssKeyMap,
            CssPropertyType,
        };

        let mut props = Vec::new();
        let key_map = CssKeyMap::get();

        for pair in style.split(';') {
            let pair = pair.trim();
            if pair.is_empty() {
                continue;
            }
            if let Some((key, value)) = pair.split_once(':') {
                let key = key.trim();
                let value = value.trim();
                // First, try to parse as a regular (non-shorthand) property
                if let Some(prop_type) = CssPropertyType::from_str(key, &key_map) {
                    if let Ok(prop) = parse_css_property(prop_type, value) {
                        props.push(CssPropertyWithConditions::on_hover(prop));
                        continue;
                    }
                }
                // If not found, try as a shorthand (combined) property
                if let Some(combined_type) = CombinedCssPropertyType::from_str(key, &key_map) {
                    if let Ok(expanded_props) = parse_combined_css_property(combined_type, value) {
                        for prop in expanded_props {
                            props.push(CssPropertyWithConditions::on_hover(prop));
                        }
                    }
                }
            }
        }

        CssPropertyWithConditionsVec::from_vec(props)
    }

    /// Parse CSS properties from a string, all with active condition
    #[cfg(feature = "parser")]
    pub fn parse_active(style: &str) -> Self {
        use crate::props::property::{
            parse_combined_css_property, parse_css_property, CombinedCssPropertyType, CssKeyMap,
            CssPropertyType,
        };

        let mut props = Vec::new();
        let key_map = CssKeyMap::get();

        for pair in style.split(';') {
            let pair = pair.trim();
            if pair.is_empty() {
                continue;
            }
            if let Some((key, value)) = pair.split_once(':') {
                let key = key.trim();
                let value = value.trim();
                // First, try to parse as a regular (non-shorthand) property
                if let Some(prop_type) = CssPropertyType::from_str(key, &key_map) {
                    if let Ok(prop) = parse_css_property(prop_type, value) {
                        props.push(CssPropertyWithConditions::on_active(prop));
                        continue;
                    }
                }
                // If not found, try as a shorthand (combined) property
                if let Some(combined_type) = CombinedCssPropertyType::from_str(key, &key_map) {
                    if let Ok(expanded_props) = parse_combined_css_property(combined_type, value) {
                        for prop in expanded_props {
                            props.push(CssPropertyWithConditions::on_active(prop));
                        }
                    }
                }
            }
        }

        CssPropertyWithConditionsVec::from_vec(props)
    }

    /// Parse CSS properties from a string, all with focus condition
    #[cfg(feature = "parser")]
    pub fn parse_focus(style: &str) -> Self {
        use crate::props::property::{
            parse_combined_css_property, parse_css_property, CombinedCssPropertyType, CssKeyMap,
            CssPropertyType,
        };

        let mut props = Vec::new();
        let key_map = CssKeyMap::get();

        for pair in style.split(';') {
            let pair = pair.trim();
            if pair.is_empty() {
                continue;
            }
            if let Some((key, value)) = pair.split_once(':') {
                let key = key.trim();
                let value = value.trim();
                // First, try to parse as a regular (non-shorthand) property
                if let Some(prop_type) = CssPropertyType::from_str(key, &key_map) {
                    if let Ok(prop) = parse_css_property(prop_type, value) {
                        props.push(CssPropertyWithConditions::on_focus(prop));
                        continue;
                    }
                }
                // If not found, try as a shorthand (combined) property
                if let Some(combined_type) = CombinedCssPropertyType::from_str(key, &key_map) {
                    if let Ok(expanded_props) = parse_combined_css_property(combined_type, value) {
                        for prop in expanded_props {
                            props.push(CssPropertyWithConditions::on_focus(prop));
                        }
                    }
                }
            }
        }

        CssPropertyWithConditionsVec::from_vec(props)
    }
}
