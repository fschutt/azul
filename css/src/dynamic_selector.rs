//! Dynamic CSS selectors for runtime evaluation based on OS, media queries, container queries, etc.

use crate::corety::{AzString, OptionString};
use crate::props::property::CssProperty;

/// State flags for pseudo-classes (used in `DynamicSelectorContext`)
/// Note: This is a CSS-only version. See `azul_core::styled_dom::StyledNodeState` for the main type.
//
// TODO(superplan g8 item 3): unify with `azul_core::styled_dom::StyledNodeState`
// (core/src/styled_dom.rs:190). The two structs now carry the *identical* 10 fields
// (hover/active/focused/disabled/checked/focus_within/visited/backdrop/dragging/
// drag_over) and core already bridges them via `StyledNodeState::from_pseudo_state_flags`.
// `azul_css` cannot depend on `azul_core`, so the merge must land core-side (e.g. move
// the shared struct into `azul_css` and re-export from core, or delete one type). This is
// a cross-crate change touching core/, left as a TODO per group ownership.
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
    /// Window is not focused (equivalent to GTK :backdrop)
    pub backdrop: bool,
    /// Element is currently being dragged (:dragging)
    pub dragging: bool,
    /// A dragged element is over this drop target (:drag-over)
    pub drag_over: bool,
}

impl PseudoStateFlags {
    /// Check if a specific pseudo-state is active
    #[must_use] pub const fn has_state(&self, state: PseudoStateType) -> bool {
        match state {
            PseudoStateType::Normal => true,
            PseudoStateType::Hover => self.hover,
            PseudoStateType::Active => self.active,
            PseudoStateType::Focus => self.focused,
            PseudoStateType::Disabled => self.disabled,
            PseudoStateType::CheckedTrue => self.checked,
            PseudoStateType::CheckedFalse => !self.checked,
            PseudoStateType::FocusWithin => self.focus_within,
            PseudoStateType::Visited => self.visited,
            PseudoStateType::Backdrop => self.backdrop,
            PseudoStateType::Dragging => self.dragging,
            PseudoStateType::DragOver => self.drag_over,
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
    [Debug, Clone, PartialEq, Eq]
);

impl_vec!(DynamicSelector, DynamicSelectorVec, DynamicSelectorVecDestructor, DynamicSelectorVecDestructorType, DynamicSelectorVecSlice, OptionDynamicSelector);
impl_vec_clone!(
    DynamicSelector,
    DynamicSelectorVec,
    DynamicSelectorVecDestructor
);
impl_vec_debug!(DynamicSelector, DynamicSelectorVec);
impl_vec_partialeq!(DynamicSelector, DynamicSelectorVec);

impl DynamicSelector {
    /// Stable per-variant tag (mirrors the `#[repr(C, u8)]` discriminants), used as
    /// the primary key for both `Ord` and `Hash` so the two stay consistent.
    const fn variant_tag(&self) -> u8 {
        match self {
            Self::Os(_) => 0,
            Self::OsVersion(_) => 1,
            Self::Media(_) => 2,
            Self::ViewportWidth(_) => 3,
            Self::ViewportHeight(_) => 4,
            Self::ContainerWidth(_) => 5,
            Self::ContainerHeight(_) => 6,
            Self::ContainerName(_) => 7,
            Self::Theme(_) => 8,
            Self::AspectRatio(_) => 9,
            Self::Orientation(_) => 10,
            Self::PrefersReducedMotion(_) => 11,
            Self::PrefersHighContrast(_) => 12,
            Self::PseudoState(_) => 13,
            Self::Language(_) => 14,
        }
    }
}

// `DynamicSelector` carries `f32` ranges (`MinMaxRange`), so `Eq`/`Ord`/`Hash`
// cannot be derived. They are implemented by hand here: every non-float payload
// already provides them, and the float ranges are compared/hashed by their bit
// pattern so the resulting order is *total* and consistent with `Hash`. (Bit
// comparison means NaN sentinels sort deterministically instead of being
// incomparable.)
impl Eq for DynamicSelector {}

impl PartialOrd for DynamicSelector {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for DynamicSelector {
    // Order-dependent tie-break arms with identical bodies can't merge without
    // changing the ordering (clippy::match_same_arms false positive).
    #[allow(clippy::match_same_arms)]
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        use core::cmp::Ordering;
        match self.variant_tag().cmp(&other.variant_tag()) {
            Ordering::Equal => {}
            non_eq => return non_eq,
        }
        // Same variant on both sides (tags are equal): compare the payloads.
        match (self, other) {
            (Self::Os(a), Self::Os(b)) => a.cmp(b),
            (Self::OsVersion(a), Self::OsVersion(b)) => a.cmp(b),
            (Self::Media(a), Self::Media(b)) => a.cmp(b),
            (Self::ContainerName(a), Self::ContainerName(b)) => a.cmp(b),
            (Self::Theme(a), Self::Theme(b)) => a.cmp(b),
            (Self::Orientation(a), Self::Orientation(b)) => a.cmp(b),
            (Self::PrefersReducedMotion(a), Self::PrefersReducedMotion(b)) => {
                a.cmp(b)
            }
            (Self::PrefersHighContrast(a), Self::PrefersHighContrast(b)) => {
                a.cmp(b)
            }
            (Self::PseudoState(a), Self::PseudoState(b)) => a.cmp(b),
            (Self::Language(a), Self::Language(b)) => a.cmp(b),
            (Self::ViewportWidth(a), Self::ViewportWidth(b))
            | (Self::ViewportHeight(a), Self::ViewportHeight(b))
            | (Self::ContainerWidth(a), Self::ContainerWidth(b))
            | (Self::ContainerHeight(a), Self::ContainerHeight(b))
            | (Self::AspectRatio(a), Self::AspectRatio(b)) => {
                (a.min.to_bits(), a.max.to_bits()).cmp(&(b.min.to_bits(), b.max.to_bits()))
            }
            // Unreachable: tags are equal, so both sides are the same variant.
            _ => Ordering::Equal,
        }
    }
}

impl core::hash::Hash for DynamicSelector {
    // Per-variant dispatch: each `x` is a different type, so the identical
    // `x.hash(state)` bodies can't merge (clippy::match_same_arms false positive).
    #[allow(clippy::match_same_arms)]
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.variant_tag().hash(state);
        match self {
            Self::Os(x) => x.hash(state),
            Self::OsVersion(x) => x.hash(state),
            Self::Media(x) => x.hash(state),
            Self::ContainerName(x) => x.hash(state),
            Self::Theme(x) => x.hash(state),
            Self::Orientation(x) => x.hash(state),
            Self::PrefersReducedMotion(x) => x.hash(state),
            Self::PrefersHighContrast(x) => x.hash(state),
            Self::PseudoState(x) => x.hash(state),
            Self::Language(x) => x.hash(state),
            Self::ViewportWidth(r)
            | Self::ViewportHeight(r)
            | Self::ContainerWidth(r)
            | Self::ContainerHeight(r)
            | Self::AspectRatio(r) => {
                r.min.to_bits().hash(state);
                r.max.to_bits().hash(state);
            }
        }
    }
}

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
    #[must_use] pub const fn new(min: Option<f32>, max: Option<f32>) -> Self {
        Self {
            min: if let Some(m) = min { m } else { f32::NAN },
            max: if let Some(m) = max { m } else { f32::NAN },
        }
    }
    
    /// Create a range with only a minimum value (>= min)
    #[must_use] pub const fn with_min(min_val: f32) -> Self {
        Self {
            min: min_val,
            max: f32::NAN,
        }
    }
    
    /// Create a range with only a maximum value (<= max)
    #[must_use] pub const fn with_max(max_val: f32) -> Self {
        Self {
            min: f32::NAN,
            max: max_val,
        }
    }

    #[must_use] pub const fn min(&self) -> Option<f32> {
        if self.min.is_nan() {
            None
        } else {
            Some(self.min)
        }
    }

    #[must_use] pub const fn max(&self) -> Option<f32> {
        if self.max.is_nan() {
            None
        } else {
            Some(self.max)
        }
    }

    #[must_use] pub fn matches(&self, value: f32) -> bool {
        let min_ok = self.min.is_nan() || value >= self.min;
        let max_ok = self.max.is_nan() || value <= self.max;
        min_ok && max_ok
    }
}

/// Boolean condition (C-compatible)
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, PartialOrd, Ord)]
pub enum BoolCondition {
    #[default]
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

/// Operating system condition for `@os` CSS selectors
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
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

impl_option!(
    OsCondition,
    OptionOsCondition,
    [Debug, Clone, Copy, PartialEq, Eq, Hash]
);

impl OsCondition {
    /// Convert from `css::system::Platform`
    #[must_use] pub const fn from_system_platform(platform: &crate::system::Platform) -> Self {
        use crate::system::Platform;
        match platform {
            Platform::Windows => Self::Windows,
            Platform::MacOs => Self::MacOS,
            Platform::Linux(_) => Self::Linux,
            Platform::Android => Self::Android,
            Platform::Ios => Self::IOS,
            Platform::Unknown => Self::Any,
        }
    }
}

#[repr(C, u8)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum OsVersionCondition {
    /// Minimum version: >= specified version
    /// Format: `OsVersion` { os, `version_id` }
    Min(OsVersion),
    /// Maximum version: <= specified version
    Max(OsVersion),
    /// Exact version match
    Exact(OsVersion),
    /// Desktop environment (Linux only)
    DesktopEnvironment(LinuxDesktopEnv),
    /// Desktop environment with min version (e.g. `@os(linux:gnome > 40)`)
    DesktopEnvMin(DesktopEnvVersion),
    /// Desktop environment with max version
    DesktopEnvMax(DesktopEnvVersion),
    /// Desktop environment with exact version
    DesktopEnvExact(DesktopEnvVersion),
}

/// A desktop environment together with a numeric version (e.g. GNOME 40).
/// Used by `OsVersionCondition::DesktopEnv{Min,Max,Exact}` for `@os(linux:gnome > 40)` style selectors.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DesktopEnvVersion {
    pub env: LinuxDesktopEnv,
    pub version_id: u32,
}

/// OS version with ordering - only comparable within the same OS family
/// 
/// Each OS has its own version numbering system with named versions.
/// Comparisons between different OS families always return false.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct OsVersion {
    /// Which OS family this version belongs to
    pub os: OsFamily,
    /// Numeric version ID for ordering (higher = newer)
    /// Each OS has its own numbering scheme starting from 0
    pub version_id: u32,
}

impl Default for OsVersion {
    fn default() -> Self {
        Self::unknown()
    }
}

impl OsVersion {
    #[must_use] pub const fn new(os: OsFamily, version_id: u32) -> Self {
        Self { os, version_id }
    }
    
    /// Compare two versions - only meaningful within the same OS family
    /// Returns None if OS families don't match (comparison not meaningful)
    #[must_use] pub fn compare(&self, other: &Self) -> Option<core::cmp::Ordering> {
        if self.os == other.os {
            Some(self.version_id.cmp(&other.version_id))
        } else {
            None // Cross-OS comparison not meaningful
        }
    }
    
    /// Check if self >= other (for Min conditions)
    #[must_use] pub fn is_at_least(&self, other: &Self) -> bool {
        self.compare(other).is_some_and(|o| o != core::cmp::Ordering::Less)
    }
    
    /// Check if self <= other (for Max conditions)
    #[must_use] pub fn is_at_most(&self, other: &Self) -> bool {
        self.compare(other).is_some_and(|o| o != core::cmp::Ordering::Greater)
    }
}

impl_option!(
    OsVersion,
    OptionOsVersion,
    [Debug, Clone, Copy, PartialEq, Eq, Hash]
);

impl OsVersion {
    
    /// Check if self == other
    #[must_use] pub fn is_exactly(&self, other: &Self) -> bool {
        self.compare(other) == Some(core::cmp::Ordering::Equal)
    }
}

/// OS family for version comparisons
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum OsFamily {
    Windows,
    MacOS,
    IOS,
    Linux,
    Android,
}

// ============================================================================
// Windows Version IDs (chronological order)
// ============================================================================

/// Windows version constants - use these in CSS like `@os(windows >= win-xp)`
impl OsVersion {
    // Windows versions (version_id = NT version * 100 + minor)
    pub const WIN_2000: Self = Self::new(OsFamily::Windows, 500);       // NT 5.0
    pub const WIN_XP: Self = Self::new(OsFamily::Windows, 501);         // NT 5.1
    pub const WIN_XP_64: Self = Self::new(OsFamily::Windows, 502);      // NT 5.2
    pub const WIN_VISTA: Self = Self::new(OsFamily::Windows, 600);      // NT 6.0
    pub const WIN_7: Self = Self::new(OsFamily::Windows, 601);          // NT 6.1
    pub const WIN_8: Self = Self::new(OsFamily::Windows, 602);          // NT 6.2
    pub const WIN_8_1: Self = Self::new(OsFamily::Windows, 603);        // NT 6.3
    pub const WIN_10: Self = Self::new(OsFamily::Windows, 1000);        // NT 10.0
    pub const WIN_10_1507: Self = Self::new(OsFamily::Windows, 1000);   // Initial release
    pub const WIN_10_1511: Self = Self::new(OsFamily::Windows, 1001);   // November Update
    pub const WIN_10_1607: Self = Self::new(OsFamily::Windows, 1002);   // Anniversary Update
    pub const WIN_10_1703: Self = Self::new(OsFamily::Windows, 1003);   // Creators Update
    pub const WIN_10_1709: Self = Self::new(OsFamily::Windows, 1004);   // Fall Creators Update
    pub const WIN_10_1803: Self = Self::new(OsFamily::Windows, 1005);   // April 2018 Update
    pub const WIN_10_1809: Self = Self::new(OsFamily::Windows, 1006);   // October 2018 Update
    pub const WIN_10_1903: Self = Self::new(OsFamily::Windows, 1007);   // May 2019 Update
    pub const WIN_10_1909: Self = Self::new(OsFamily::Windows, 1008);   // November 2019 Update
    pub const WIN_10_2004: Self = Self::new(OsFamily::Windows, 1009);   // May 2020 Update
    pub const WIN_10_20H2: Self = Self::new(OsFamily::Windows, 1010);   // October 2020 Update
    pub const WIN_10_21H1: Self = Self::new(OsFamily::Windows, 1011);   // May 2021 Update
    pub const WIN_10_21H2: Self = Self::new(OsFamily::Windows, 1012);   // November 2021 Update
    pub const WIN_10_22H2: Self = Self::new(OsFamily::Windows, 1013);   // 2022 Update
    pub const WIN_11: Self = Self::new(OsFamily::Windows, 1100);        // Windows 11 base
    pub const WIN_11_21H2: Self = Self::new(OsFamily::Windows, 1100);   // Initial release
    pub const WIN_11_22H2: Self = Self::new(OsFamily::Windows, 1101);   // 2022 Update
    pub const WIN_11_23H2: Self = Self::new(OsFamily::Windows, 1102);   // 2023 Update
    pub const WIN_11_24H2: Self = Self::new(OsFamily::Windows, 1103);   // 2024 Update
    
    // macOS versions (version_id = major * 100 + minor)
    pub const MACOS_CHEETAH: Self = Self::new(OsFamily::MacOS, 1000);       // 10.0
    pub const MACOS_PUMA: Self = Self::new(OsFamily::MacOS, 1001);          // 10.1
    pub const MACOS_JAGUAR: Self = Self::new(OsFamily::MacOS, 1002);        // 10.2
    pub const MACOS_PANTHER: Self = Self::new(OsFamily::MacOS, 1003);       // 10.3
    pub const MACOS_TIGER: Self = Self::new(OsFamily::MacOS, 1004);         // 10.4
    pub const MACOS_LEOPARD: Self = Self::new(OsFamily::MacOS, 1005);       // 10.5
    pub const MACOS_SNOW_LEOPARD: Self = Self::new(OsFamily::MacOS, 1006);  // 10.6
    pub const MACOS_LION: Self = Self::new(OsFamily::MacOS, 1007);          // 10.7
    pub const MACOS_MOUNTAIN_LION: Self = Self::new(OsFamily::MacOS, 1008); // 10.8
    pub const MACOS_MAVERICKS: Self = Self::new(OsFamily::MacOS, 1009);     // 10.9
    pub const MACOS_YOSEMITE: Self = Self::new(OsFamily::MacOS, 1010);      // 10.10
    pub const MACOS_EL_CAPITAN: Self = Self::new(OsFamily::MacOS, 1011);    // 10.11
    pub const MACOS_SIERRA: Self = Self::new(OsFamily::MacOS, 1012);        // 10.12
    pub const MACOS_HIGH_SIERRA: Self = Self::new(OsFamily::MacOS, 1013);   // 10.13
    pub const MACOS_MOJAVE: Self = Self::new(OsFamily::MacOS, 1014);        // 10.14
    pub const MACOS_CATALINA: Self = Self::new(OsFamily::MacOS, 1015);      // 10.15
    pub const MACOS_BIG_SUR: Self = Self::new(OsFamily::MacOS, 1100);       // 11.0
    pub const MACOS_MONTEREY: Self = Self::new(OsFamily::MacOS, 1200);      // 12.0
    pub const MACOS_VENTURA: Self = Self::new(OsFamily::MacOS, 1300);       // 13.0
    pub const MACOS_SONOMA: Self = Self::new(OsFamily::MacOS, 1400);        // 14.0
    pub const MACOS_SEQUOIA: Self = Self::new(OsFamily::MacOS, 1500);       // 15.0
    pub const MACOS_TAHOE: Self = Self::new(OsFamily::MacOS, 2600);         // 26.0
    
    // iOS versions (version_id = major * 100 + minor)
    pub const IOS_1: Self = Self::new(OsFamily::IOS, 100);
    pub const IOS_2: Self = Self::new(OsFamily::IOS, 200);
    pub const IOS_3: Self = Self::new(OsFamily::IOS, 300);
    pub const IOS_4: Self = Self::new(OsFamily::IOS, 400);
    pub const IOS_5: Self = Self::new(OsFamily::IOS, 500);
    pub const IOS_6: Self = Self::new(OsFamily::IOS, 600);
    pub const IOS_7: Self = Self::new(OsFamily::IOS, 700);
    pub const IOS_8: Self = Self::new(OsFamily::IOS, 800);
    pub const IOS_9: Self = Self::new(OsFamily::IOS, 900);
    pub const IOS_10: Self = Self::new(OsFamily::IOS, 1000);
    pub const IOS_11: Self = Self::new(OsFamily::IOS, 1100);
    pub const IOS_12: Self = Self::new(OsFamily::IOS, 1200);
    pub const IOS_13: Self = Self::new(OsFamily::IOS, 1300);
    pub const IOS_14: Self = Self::new(OsFamily::IOS, 1400);
    pub const IOS_15: Self = Self::new(OsFamily::IOS, 1500);
    pub const IOS_16: Self = Self::new(OsFamily::IOS, 1600);
    pub const IOS_17: Self = Self::new(OsFamily::IOS, 1700);
    pub const IOS_18: Self = Self::new(OsFamily::IOS, 1800);
    
    // Android versions (API level as version_id)
    pub const ANDROID_CUPCAKE: Self = Self::new(OsFamily::Android, 3);      // 1.5
    pub const ANDROID_DONUT: Self = Self::new(OsFamily::Android, 4);        // 1.6
    pub const ANDROID_ECLAIR: Self = Self::new(OsFamily::Android, 7);       // 2.1
    pub const ANDROID_FROYO: Self = Self::new(OsFamily::Android, 8);        // 2.2
    pub const ANDROID_GINGERBREAD: Self = Self::new(OsFamily::Android, 10); // 2.3
    pub const ANDROID_HONEYCOMB: Self = Self::new(OsFamily::Android, 13);   // 3.2
    pub const ANDROID_ICE_CREAM_SANDWICH: Self = Self::new(OsFamily::Android, 15); // 4.0
    pub const ANDROID_JELLY_BEAN: Self = Self::new(OsFamily::Android, 18);  // 4.3
    pub const ANDROID_KITKAT: Self = Self::new(OsFamily::Android, 19);      // 4.4
    pub const ANDROID_LOLLIPOP: Self = Self::new(OsFamily::Android, 22);    // 5.1
    pub const ANDROID_MARSHMALLOW: Self = Self::new(OsFamily::Android, 23); // 6.0
    pub const ANDROID_NOUGAT: Self = Self::new(OsFamily::Android, 25);      // 7.1
    pub const ANDROID_OREO: Self = Self::new(OsFamily::Android, 27);        // 8.1
    pub const ANDROID_PIE: Self = Self::new(OsFamily::Android, 28);         // 9.0
    pub const ANDROID_10: Self = Self::new(OsFamily::Android, 29);          // 10
    pub const ANDROID_11: Self = Self::new(OsFamily::Android, 30);          // 11
    pub const ANDROID_12: Self = Self::new(OsFamily::Android, 31);          // 12
    pub const ANDROID_12L: Self = Self::new(OsFamily::Android, 32);         // 12L
    pub const ANDROID_13: Self = Self::new(OsFamily::Android, 33);          // 13
    pub const ANDROID_14: Self = Self::new(OsFamily::Android, 34);          // 14
    pub const ANDROID_15: Self = Self::new(OsFamily::Android, 35);          // 15
    
    // Linux kernel versions (major * 1000 + minor * 10 + patch)
    pub const LINUX_2_6: Self = Self::new(OsFamily::Linux, 2060);
    pub const LINUX_3_0: Self = Self::new(OsFamily::Linux, 3000);
    pub const LINUX_4_0: Self = Self::new(OsFamily::Linux, 4000);
    pub const LINUX_5_0: Self = Self::new(OsFamily::Linux, 5000);
    pub const LINUX_6_0: Self = Self::new(OsFamily::Linux, 6000);
    
    /// Unknown OS version (for when detection fails or OS is unknown)
    #[must_use] pub const fn unknown() -> Self {
        Self {
            os: OsFamily::Linux, // Fallback, but version_id 0 means "unknown"
            version_id: 0,
        }
    }
}

/// Parse a named or numeric OS version string
/// Returns None if the version string is not recognized
#[must_use] pub fn parse_os_version(os: OsFamily, version_str: &str) -> Option<OsVersion> {
    let version_str = version_str.trim().to_lowercase();
    let version_str = version_str.as_str();
    
    match os {
        OsFamily::Windows => parse_windows_version(version_str),
        OsFamily::MacOS => parse_macos_version(version_str),
        OsFamily::IOS => parse_ios_version(version_str),
        OsFamily::Android => parse_android_version(version_str),
        OsFamily::Linux => parse_linux_version(version_str),
    }
}

fn parse_windows_version(s: &str) -> Option<OsVersion> {
    // Strip optional "win"/"windows" prefix (allowing -, _ separators).
    // This collapses "11", "win11", "win-11", "windows11", "windows-11", "windows_11" to "11".
    let core = strip_os_prefix(s, &["windows", "win"]);
    match core {
        // Each version groups its named alias with the numeric NT version.
        "2000" | "5.0" | "nt5.0" => Some(OsVersion::WIN_2000),
        "xp" | "5.1" | "nt5.1" => Some(OsVersion::WIN_XP),
        "vista" | "6.0" | "nt6.0" => Some(OsVersion::WIN_VISTA),
        "7" | "6.1" | "nt6.1" => Some(OsVersion::WIN_7),
        "8" | "6.2" | "nt6.2" => Some(OsVersion::WIN_8),
        "8.1" | "8-1" | "6.3" | "nt6.3" => Some(OsVersion::WIN_8_1),
        "10" | "10.0" | "nt10.0" => Some(OsVersion::WIN_10),
        "11" => Some(OsVersion::WIN_11),
        _ => None,
    }
}

/// If `s` starts with any of the given prefixes, strip the prefix plus an optional
/// trailing `-` or `_` separator. Otherwise return `s` unchanged. Matching is
/// case-insensitive (callers already lowercase, this just makes the helper safe).
fn strip_os_prefix<'a>(s: &'a str, prefixes: &[&str]) -> &'a str {
    for p in prefixes {
        if let Some(rest) = s.strip_prefix(p) {
            return rest.strip_prefix(['-', '_']).unwrap_or(rest);
        }
    }
    s
}

fn parse_macos_version(s: &str) -> Option<OsVersion> {
    match s {
        "cheetah" | "10.0" => Some(OsVersion::MACOS_CHEETAH),
        "puma" | "10.1" => Some(OsVersion::MACOS_PUMA),
        "jaguar" | "10.2" => Some(OsVersion::MACOS_JAGUAR),
        "panther" | "10.3" => Some(OsVersion::MACOS_PANTHER),
        "tiger" | "10.4" => Some(OsVersion::MACOS_TIGER),
        "leopard" | "10.5" => Some(OsVersion::MACOS_LEOPARD),
        "snow-leopard" | "snowleopard" | "10.6" => Some(OsVersion::MACOS_SNOW_LEOPARD),
        "lion" | "10.7" => Some(OsVersion::MACOS_LION),
        "mountain-lion" | "mountainlion" | "10.8" => Some(OsVersion::MACOS_MOUNTAIN_LION),
        "mavericks" | "10.9" => Some(OsVersion::MACOS_MAVERICKS),
        "yosemite" | "10.10" => Some(OsVersion::MACOS_YOSEMITE),
        "el-capitan" | "elcapitan" | "10.11" => Some(OsVersion::MACOS_EL_CAPITAN),
        "sierra" | "10.12" => Some(OsVersion::MACOS_SIERRA),
        "high-sierra" | "highsierra" | "10.13" => Some(OsVersion::MACOS_HIGH_SIERRA),
        "mojave" | "10.14" => Some(OsVersion::MACOS_MOJAVE),
        "catalina" | "10.15" => Some(OsVersion::MACOS_CATALINA),
        "big-sur" | "bigsur" | "11" | "11.0" => Some(OsVersion::MACOS_BIG_SUR),
        "monterey" | "12" | "12.0" => Some(OsVersion::MACOS_MONTEREY),
        "ventura" | "13" | "13.0" => Some(OsVersion::MACOS_VENTURA),
        "sonoma" | "14" | "14.0" => Some(OsVersion::MACOS_SONOMA),
        "sequoia" | "15" | "15.0" => Some(OsVersion::MACOS_SEQUOIA),
        "tahoe" | "26" | "26.0" => Some(OsVersion::MACOS_TAHOE),
        _ => None,
    }
}

fn parse_ios_version(s: &str) -> Option<OsVersion> {
    match s {
        "1" | "1.0" => Some(OsVersion::IOS_1),
        "2" | "2.0" => Some(OsVersion::IOS_2),
        "3" | "3.0" => Some(OsVersion::IOS_3),
        "4" | "4.0" => Some(OsVersion::IOS_4),
        "5" | "5.0" => Some(OsVersion::IOS_5),
        "6" | "6.0" => Some(OsVersion::IOS_6),
        "7" | "7.0" => Some(OsVersion::IOS_7),
        "8" | "8.0" => Some(OsVersion::IOS_8),
        "9" | "9.0" => Some(OsVersion::IOS_9),
        "10" | "10.0" => Some(OsVersion::IOS_10),
        "11" | "11.0" => Some(OsVersion::IOS_11),
        "12" | "12.0" => Some(OsVersion::IOS_12),
        "13" | "13.0" => Some(OsVersion::IOS_13),
        "14" | "14.0" => Some(OsVersion::IOS_14),
        "15" | "15.0" => Some(OsVersion::IOS_15),
        "16" | "16.0" => Some(OsVersion::IOS_16),
        "17" | "17.0" => Some(OsVersion::IOS_17),
        "18" | "18.0" => Some(OsVersion::IOS_18),
        _ => None,
    }
}

fn parse_android_version(s: &str) -> Option<OsVersion> {
    match s {
        "cupcake" | "1.5" => Some(OsVersion::ANDROID_CUPCAKE),
        "donut" | "1.6" => Some(OsVersion::ANDROID_DONUT),
        "eclair" | "2.1" => Some(OsVersion::ANDROID_ECLAIR),
        "froyo" | "2.2" => Some(OsVersion::ANDROID_FROYO),
        "gingerbread" | "2.3" => Some(OsVersion::ANDROID_GINGERBREAD),
        "honeycomb" | "3.0" | "3.2" => Some(OsVersion::ANDROID_HONEYCOMB),
        "ice-cream-sandwich" | "ics" | "4.0" => Some(OsVersion::ANDROID_ICE_CREAM_SANDWICH),
        "jelly-bean" | "jellybean" | "4.3" => Some(OsVersion::ANDROID_JELLY_BEAN),
        "kitkat" | "4.4" => Some(OsVersion::ANDROID_KITKAT),
        "lollipop" | "5.0" | "5.1" => Some(OsVersion::ANDROID_LOLLIPOP),
        "marshmallow" | "6.0" => Some(OsVersion::ANDROID_MARSHMALLOW),
        "nougat" | "7.0" | "7.1" => Some(OsVersion::ANDROID_NOUGAT),
        "oreo" | "8.0" | "8.1" => Some(OsVersion::ANDROID_OREO),
        "pie" | "9" | "9.0" => Some(OsVersion::ANDROID_PIE),
        "10" | "q" => Some(OsVersion::ANDROID_10),
        "11" | "r" => Some(OsVersion::ANDROID_11),
        "12" | "s" => Some(OsVersion::ANDROID_12),
        "12l" | "12L" => Some(OsVersion::ANDROID_12L),
        "13" | "t" | "tiramisu" => Some(OsVersion::ANDROID_13),
        "14" | "u" | "upside-down-cake" => Some(OsVersion::ANDROID_14),
        "15" | "v" | "vanilla-ice-cream" => Some(OsVersion::ANDROID_15),
        _ => {
            // Try parsing as API level
            if let Some(api) = s.strip_prefix("api") {
                if let Ok(level) = api.trim().parse::<u32>() {
                    return Some(OsVersion::new(OsFamily::Android, level));
                }
            }
            None
        }
    }
}

fn parse_linux_version(s: &str) -> Option<OsVersion> {
    // Strip optional "linux" prefix so "linux6.1" / "linux-6.1" also work.
    let s = strip_os_prefix(s, &["linux"]);
    // Parse kernel version like "5.4", "6.0", or bare major like "5" (== "5.0").
    let mut parts = s.split('.');
    let major = parts.next()?.parse::<u32>().ok()?;
    let minor = parts.next().map_or(Some(0), |p| p.parse::<u32>().ok())?;
    let patch = parts.next().map_or(Some(0), |p| p.parse::<u32>().ok())?;
    Some(OsVersion::new(OsFamily::Linux, major * 1000 + minor * 10 + patch))
}

/// Linux desktop environment for `@os(linux:<de>)` CSS selectors.
///
/// Note: `from_system_desktop_env` currently only maps Gnome, KDE, and Other.
/// XFCE, Unity, Cinnamon, and MATE can be matched via CSS parsing (`@os(linux:xfce)`)
/// but will not be auto-detected from the system — they map to `Other` at runtime.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum LinuxDesktopEnv {
    Gnome,
    KDE,
    /// CSS-parse-only: not auto-detected from system (maps to `Other` at runtime)
    XFCE,
    /// CSS-parse-only: not auto-detected from system (maps to `Other` at runtime)
    Unity,
    /// CSS-parse-only: not auto-detected from system (maps to `Other` at runtime)
    Cinnamon,
    /// CSS-parse-only: not auto-detected from system (maps to `Other` at runtime)
    MATE,
    Other,
}

impl LinuxDesktopEnv {
    /// Convert from `css::system::DesktopEnvironment`
    #[must_use] pub const fn from_system_desktop_env(de: &crate::system::DesktopEnvironment) -> Self {
        use crate::system::DesktopEnvironment;
        match de {
            DesktopEnvironment::Gnome => Self::Gnome,
            DesktopEnvironment::Kde => Self::KDE,
            DesktopEnvironment::Other(_) => Self::Other,
        }
    }
}

/// Media type for `@media` CSS selectors (screen, print, all)
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum MediaType {
    Screen,
    Print,
    All,
}
#[allow(variant_size_differences)] // repr(C,u8) FFI enum: boxing the large variant would change the C ABI (api.json bindings); size disparity accepted
#[repr(C, u8)]
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ThemeCondition {
    Light,
    Dark,
    Custom(AzString),
    /// System preference
    SystemPreferred,
}

impl_option!(
    ThemeCondition,
    OptionThemeCondition,
    copy = false,
    [Debug, Clone, PartialEq, Eq, Hash]
);

impl ThemeCondition {
    /// Convert from `css::system::Theme`
    #[must_use] pub const fn from_system_theme(theme: crate::system::Theme) -> Self {
        use crate::system::Theme;
        match theme {
            Theme::Light => Self::Light,
            Theme::Dark => Self::Dark,
        }
    }
}

/// Orientation type for `@media (orientation: ...)` CSS selectors
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum OrientationType {
    Portrait,
    Landscape,
}

/// Language/Locale condition for @`lang()` CSS selector
/// Matches BCP 47 language tags with prefix matching
#[repr(C, u8)]
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum LanguageCondition {
    /// Exact match (e.g., "de-DE" matches only "de-DE")
    Exact(AzString),
    /// Prefix match (e.g., "de" matches "de", "de-DE", "de-AT", etc.)
    Prefix(AzString),
}

impl LanguageCondition {
    /// Check if this condition matches the given language tag
    #[must_use] pub fn matches(&self, language: &str) -> bool {
        match self {
            Self::Exact(lang) => language.eq_ignore_ascii_case(lang.as_str()),
            Self::Prefix(prefix) => {
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PseudoStateType {
    /// No special state (corresponds to "Normal" in `NodeDataInlineCssProperty`)
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
    CheckedTrue,
    /// Element is unchecked (:not(:checked))
    CheckedFalse,
    /// Element or child has focus (:focus-within)
    FocusWithin,
    /// Link has been visited (:visited)
    Visited,
    /// Window is not focused (:backdrop) - GTK compatibility
    Backdrop,
    /// Element is currently being dragged (:dragging)
    Dragging,
    /// A dragged element is over this drop target (:drag-over)
    DragOver,
}

impl_option!(
    LinuxDesktopEnv,
    OptionLinuxDesktopEnv,
    [Debug, Clone, Copy, PartialEq, Eq, Hash]
);

/// Default viewport width used when actual window size is not yet known.
pub const DEFAULT_VIEWPORT_WIDTH: f32 = 800.0;
/// Default viewport height used when actual window size is not yet known.
pub const DEFAULT_VIEWPORT_HEIGHT: f32 = 600.0;

/// Context for evaluating dynamic selectors
#[repr(C)]
#[derive(Debug, Clone)]
pub struct DynamicSelectorContext {
    /// Operating system info
    pub os: OsCondition,
    pub os_version: OsVersion,
    pub desktop_env: OptionLinuxDesktopEnv,
    /// Numeric version of the active desktop environment (0 = unknown).
    /// Used by `@os(linux:gnome > 40)` style selectors. A value of 0 never
    /// satisfies any DE-version constraint, so detection can be wired up
    /// later without breaking parsed rules.
    pub de_version: u32,

    /// Theme info
    pub theme: ThemeCondition,

    /// Media info (from `WindowState`)
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

    /// Node state (hover, active, focus, disabled, checked, `focus_within`, visited)
    pub pseudo_state: PseudoStateFlags,

    /// Language/Locale (BCP 47 tag, e.g., "en-US", "de-DE")
    pub language: AzString,

    /// Whether the window currently has focus (for :backdrop pseudo-class)
    /// When false, :backdrop styles should be applied
    pub window_focused: bool,
}

impl Default for DynamicSelectorContext {
    fn default() -> Self {
        Self {
            os: OsCondition::Any,
            os_version: OsVersion::unknown(),
            desktop_env: OptionLinuxDesktopEnv::None,
            de_version: 0,
            theme: ThemeCondition::Light,
            media_type: MediaType::Screen,
            viewport_width: DEFAULT_VIEWPORT_WIDTH,
            viewport_height: DEFAULT_VIEWPORT_HEIGHT,
            container_width: f32::NAN,
            container_height: f32::NAN,
            container_name: OptionString::None,
            prefers_reduced_motion: BoolCondition::False,
            prefers_high_contrast: BoolCondition::False,
            orientation: OrientationType::Landscape,
            pseudo_state: PseudoStateFlags::default(),
            language: AzString::from_const_str("en-US"),
            window_focused: true,
        }
    }
}

impl DynamicSelectorContext {
    /// Create a context from `SystemStyle`
    #[must_use] pub fn from_system_style(system_style: &crate::system::SystemStyle) -> Self {
        let os = OsCondition::from_system_platform(&system_style.platform);
        let desktop_env = if let crate::system::Platform::Linux(de) = &system_style.platform {
            OptionLinuxDesktopEnv::Some(LinuxDesktopEnv::from_system_desktop_env(de))
        } else {
            OptionLinuxDesktopEnv::None
        };
        let theme = ThemeCondition::from_system_theme(system_style.theme);

        Self {
            os,
            os_version: system_style.os_version, // Use version from SystemStyle
            desktop_env,
            de_version: 0, // TODO: wire up DE version detection in system::detect_*
            theme,
            media_type: MediaType::Screen,
            viewport_width: DEFAULT_VIEWPORT_WIDTH, // Will be updated with window size
            viewport_height: DEFAULT_VIEWPORT_HEIGHT,
            container_width: f32::NAN,
            container_height: f32::NAN,
            container_name: OptionString::None,
            prefers_reduced_motion: system_style.prefers_reduced_motion,
            prefers_high_contrast: system_style.prefers_high_contrast,
            orientation: OrientationType::Landscape,
            pseudo_state: PseudoStateFlags::default(),
            language: system_style.language.clone(),
            window_focused: true,
        }
    }

    /// Update viewport dimensions (e.g., on window resize)
    #[must_use] pub fn with_viewport(&self, width: f32, height: f32) -> Self {
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
    #[must_use] pub fn with_container(&self, width: f32, height: f32, name: Option<AzString>) -> Self {
        let mut ctx = self.clone();
        ctx.container_width = width;
        ctx.container_height = height;
        ctx.container_name = name.into();
        ctx
    }

    /// Update pseudo-state (hover, active, focus, etc.)
    #[must_use] pub fn with_pseudo_state(&self, state: PseudoStateFlags) -> Self {
        let mut ctx = self.clone();
        ctx.pseudo_state = state;
        ctx
    }

    /// Check if viewport changed significantly (for breakpoint detection)
    #[must_use] pub fn viewport_breakpoint_changed(&self, other: &Self, breakpoints: &[f32]) -> bool {
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
    #[must_use] pub fn matches(&self, ctx: &DynamicSelectorContext) -> bool {
        match self {
            Self::Os(os) => Self::match_os(*os, ctx.os),
            Self::OsVersion(ver) => Self::match_os_version(ver, ctx.os_version, ctx.desktop_env, ctx.de_version),
            Self::Media(media) => *media == ctx.media_type || *media == MediaType::All,
            Self::ViewportWidth(range) => range.matches(ctx.viewport_width),
            Self::ViewportHeight(range) => range.matches(ctx.viewport_height),
            Self::ContainerWidth(range) => {
                !ctx.container_width.is_nan() && range.matches(ctx.container_width)
            }
            Self::ContainerHeight(range) => {
                !ctx.container_height.is_nan() && range.matches(ctx.container_height)
            }
            Self::ContainerName(name) => ctx.container_name.as_ref() == Some(name),
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
            Self::PseudoState(state) => Self::match_pseudo_state(*state, ctx),
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
        actual: OsVersion,
        desktop_env: OptionLinuxDesktopEnv,
        de_version: u32,
    ) -> bool {
        // de_version == 0 means the runtime hasn't reported a version,
        // so any DE-version constraint fails until detection is wired up.
        let de_matches = |env: &LinuxDesktopEnv| desktop_env.as_ref() == Some(env);
        match condition {
            OsVersionCondition::Exact(ver) => actual.is_exactly(ver),
            OsVersionCondition::Min(ver) => actual.is_at_least(ver),
            OsVersionCondition::Max(ver) => actual.is_at_most(ver),
            OsVersionCondition::DesktopEnvironment(env) => de_matches(env),
            OsVersionCondition::DesktopEnvMin(d) =>
                de_matches(&d.env) && de_version != 0 && de_version >= d.version_id,
            OsVersionCondition::DesktopEnvMax(d) =>
                de_matches(&d.env) && de_version != 0 && de_version <= d.version_id,
            OsVersionCondition::DesktopEnvExact(d) =>
                de_matches(&d.env) && de_version == d.version_id,
        }
    }

    fn match_theme(condition: &ThemeCondition, actual: &ThemeCondition) -> bool {
        match (condition, actual) {
            (ThemeCondition::SystemPreferred, _) => true,
            _ => condition == actual,
        }
    }

    const fn match_pseudo_state(state: PseudoStateType, ctx: &DynamicSelectorContext) -> bool {
        let node_state = &ctx.pseudo_state;
        match state {
            PseudoStateType::Normal => true, // Normal is always active (base state)
            PseudoStateType::Hover => node_state.hover,
            PseudoStateType::Active => node_state.active,
            PseudoStateType::Focus => node_state.focused,
            PseudoStateType::Disabled => node_state.disabled,
            PseudoStateType::CheckedTrue => node_state.checked,
            PseudoStateType::CheckedFalse => !node_state.checked,
            PseudoStateType::FocusWithin => node_state.focus_within,
            PseudoStateType::Visited => node_state.visited,
            PseudoStateType::Backdrop => node_state.backdrop,
            PseudoStateType::Dragging => node_state.dragging,
            PseudoStateType::DragOver => node_state.drag_over,
        }
    }
}

/// Parse the content of an `@os(...)` at-rule into a list of dynamic-selector conditions.
///
/// Accepts both bare-identifier and parenthesized forms:
///
/// - `linux`                       → `[Os(Linux)]`
/// - `(linux)`                     → `[Os(Linux)]`
/// - `(linux:gnome)`               → `[Os(Linux), OsVersion(DesktopEnvironment(Gnome))]`
/// - `(windows >= win-11)`         → `[Os(Windows), OsVersion(Min(WIN_11))]`
/// - `(linux:gnome > 40)`          → `[Os(Linux), OsVersion(DesktopEnvMin{ env: Gnome, version_id: 40 })]`
/// - `(any)` / `(*)` / `(all)`     → `[]` (always-match, no conditions emitted)
///
/// Returns `None` only when the content is a parse error.
/// `Some(vec![])` means "always match" (the rule applies unconditionally).
#[cfg(feature = "parser")]
#[must_use] pub fn parse_os_at_rule_content(content: &str) -> Option<Vec<DynamicSelector>> {
    let trimmed = content.trim();
    let inner = trimmed
        .strip_prefix('(').and_then(|s| s.strip_suffix(')'))
        .unwrap_or(trimmed)
        .trim();
    let inner = inner
        .strip_prefix('"').and_then(|s| s.strip_suffix('"'))
        .or_else(|| inner.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')))
        .unwrap_or(inner)
        .trim();
    if inner.is_empty() {
        return None;
    }

    // Split off the operator + version, if any.
    let (subject, op_and_version) = split_op_and_version(inner);
    let subject = subject.trim();

    // subject is "family" or "family:de"
    let (family_str, de_str) = match subject.split_once(':') {
        Some((f, d)) => (f.trim(), Some(d.trim())),
        None => (subject, None),
    };

    let family = parse_os_family_token(family_str)?;
    let de = match de_str {
        Some(s) if !s.is_empty() => Some(parse_de_token(s)),
        _ => None,
    };

    let mut out = Vec::new();
    // Always emit the family selector, even for `Any` — `Os(Any)` is matched as
    // unconditionally true, but keeping it in the conditions list makes the rule
    // structure visible to introspection.
    out.push(DynamicSelector::Os(family));

    match (de, op_and_version) {
        // Bare DE with no version: just "is the DE this one"
        (Some(env), None) => {
            out.push(DynamicSelector::OsVersion(OsVersionCondition::DesktopEnvironment(env)));
        }
        // DE + version: emit a DesktopEnv* condition
        (Some(env), Some((op, ver_str))) => {
            let v: u32 = ver_str.parse().ok()?;
            let dev = DesktopEnvVersion { env, version_id: v };
            let cond = match op {
                VersionOp::Min => OsVersionCondition::DesktopEnvMin(dev),
                VersionOp::Max => OsVersionCondition::DesktopEnvMax(dev),
                VersionOp::Exact => OsVersionCondition::DesktopEnvExact(dev),
            };
            out.push(DynamicSelector::OsVersion(cond));
        }
        // OS family + version
        (None, Some((op, ver_str))) => {
            let os_family = match family {
                OsCondition::Linux => OsFamily::Linux,
                OsCondition::Windows => OsFamily::Windows,
                OsCondition::MacOS => OsFamily::MacOS,
                OsCondition::IOS => OsFamily::IOS,
                OsCondition::Android => OsFamily::Android,
                // Apple, Web, Any have no version line — reject.
                _ => return None,
            };
            let version = parse_os_version(os_family, ver_str)?;
            let cond = match op {
                VersionOp::Min => OsVersionCondition::Min(version),
                VersionOp::Max => OsVersionCondition::Max(version),
                VersionOp::Exact => OsVersionCondition::Exact(version),
            };
            out.push(DynamicSelector::OsVersion(cond));
        }
        // Family only — already pushed above (or empty for `any`).
        (None, None) => {}
    }

    Some(out)
}

#[cfg(feature = "parser")]
#[derive(Copy, Clone)]
enum VersionOp { Min, Max, Exact }

/// Find the first comparison operator (`>=`, `<=`, `=`, `>`, `<`) in `s` and split.
/// `>` and `<` are treated as `>=` / `<=` because version IDs are discrete integers.
#[cfg(feature = "parser")]
fn split_op_and_version(s: &str) -> (&str, Option<(VersionOp, &str)>) {
    // Earliest match wins; on a tie, the longer operator wins (so ">=" beats "=" at the same position).
    let candidates: &[(&str, VersionOp)] = &[
        (">=", VersionOp::Min),
        ("<=", VersionOp::Max),
        ("=",  VersionOp::Exact),
        (">",  VersionOp::Min),
        ("<",  VersionOp::Max),
    ];
    let mut best: Option<(usize, usize, VersionOp)> = None;
    for (op_str, op) in candidates {
        if let Some(pos) = s.find(op_str) {
            let len = op_str.len();
            best = Some(match best {
                None => (pos, len, *op),
                Some((bp, bl, _)) if pos < bp || (pos == bp && len > bl) => (pos, len, *op),
                Some(b) => b,
            });
        }
    }
    match best {
        Some((pos, len, op)) => (&s[..pos], Some((op, s[pos + len..].trim()))),
        None => (s, None),
    }
}

#[cfg(feature = "parser")]
fn parse_os_family_token(s: &str) -> Option<OsCondition> {
    match s.to_lowercase().as_str() {
        "linux" => Some(OsCondition::Linux),
        "windows" | "win" => Some(OsCondition::Windows),
        "macos" | "mac" | "osx" => Some(OsCondition::MacOS),
        "ios" => Some(OsCondition::IOS),
        "android" => Some(OsCondition::Android),
        "apple" => Some(OsCondition::Apple),
        "web" | "wasm" => Some(OsCondition::Web),
        "any" | "all" | "*" => Some(OsCondition::Any),
        _ => None,
    }
}

#[cfg(feature = "parser")]
fn parse_de_token(s: &str) -> LinuxDesktopEnv {
    match s.to_lowercase().as_str() {
        "gnome" => LinuxDesktopEnv::Gnome,
        "kde" => LinuxDesktopEnv::KDE,
        "xfce" => LinuxDesktopEnv::XFCE,
        "unity" => LinuxDesktopEnv::Unity,
        "cinnamon" => LinuxDesktopEnv::Cinnamon,
        "mate" => LinuxDesktopEnv::MATE,
        _ => LinuxDesktopEnv::Other,
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
    [Debug, Clone, PartialEq, Eq, PartialOrd]
);

impl Eq for CssPropertyWithConditions {}

impl PartialOrd for CssPropertyWithConditions {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for CssPropertyWithConditions {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        // Order by the property first, then lexicographically by the full list of
        // conditions. This is consistent with the derived `PartialEq` (which compares
        // both fields) and with `Hash` below, so the type is sound to use as a
        // `BTreeMap`/`BTreeSet` key or to dedup after sorting. (The previous impl
        // compared the condition *count* only, which violated the Eq/Ord agreement.)
        self.property
            .cmp(&other.property)
            .then_with(|| self.apply_if.as_slice().cmp(other.apply_if.as_slice()))
    }
}

impl CssPropertyWithConditions {
    /// Create an unconditional property (always applies) - const version
    #[must_use] pub const fn simple(property: CssProperty) -> Self {
        Self {
            property,
            apply_if: DynamicSelectorVec::from_const_slice(&[]),
        }
    }

    /// Create a property with a single condition (const version using slice reference)
    #[must_use] pub const fn with_single_condition(
        property: CssProperty,
        conditions: &'static [DynamicSelector],
    ) -> Self {
        Self {
            property,
            apply_if: DynamicSelectorVec::from_const_slice(conditions),
        }
    }

    /// Create a property with a single condition (non-const, allocates)
    #[must_use] pub fn with_condition(property: CssProperty, condition: DynamicSelector) -> Self {
        Self {
            property,
            apply_if: DynamicSelectorVec::from_vec(vec![condition]),
        }
    }

    /// Create a property with multiple conditions (all must match)
    #[must_use] pub const fn with_conditions(property: CssProperty, conditions: DynamicSelectorVec) -> Self {
        Self {
            property,
            apply_if: conditions,
        }
    }

    /// Create a property that applies only on hover (const version)
    #[must_use] pub const fn on_hover(property: CssProperty) -> Self {
        Self::with_single_condition(
            property,
            &[DynamicSelector::PseudoState(PseudoStateType::Hover)],
        )
    }

    /// Create a property that applies only when active (const version)
    #[must_use] pub const fn on_active(property: CssProperty) -> Self {
        Self::with_single_condition(
            property,
            &[DynamicSelector::PseudoState(PseudoStateType::Active)],
        )
    }

    /// Create a property that applies only when focused (const version)
    #[must_use] pub const fn on_focus(property: CssProperty) -> Self {
        Self::with_single_condition(
            property,
            &[DynamicSelector::PseudoState(PseudoStateType::Focus)],
        )
    }

    /// Create a property that applies only when disabled (const version)
    #[must_use] pub const fn when_disabled(property: CssProperty) -> Self {
        Self::with_single_condition(
            property,
            &[DynamicSelector::PseudoState(PseudoStateType::Disabled)],
        )
    }

    /// Create a property that applies only on a specific OS (non-const, needs runtime value)
    #[must_use] pub fn on_os(property: CssProperty, os: OsCondition) -> Self {
        Self::with_condition(property, DynamicSelector::Os(os))
    }

    /// Create a property that applies only in dark theme (const version)
    #[must_use] pub const fn dark_theme(property: CssProperty) -> Self {
        Self::with_single_condition(property, &[DynamicSelector::Theme(ThemeCondition::Dark)])
    }

    /// Create a property that applies only in light theme (const version)
    #[must_use] pub const fn light_theme(property: CssProperty) -> Self {
        Self::with_single_condition(property, &[DynamicSelector::Theme(ThemeCondition::Light)])
    }

    /// Create a property for Windows only (const version)
    #[must_use] pub const fn on_windows(property: CssProperty) -> Self {
        Self::with_single_condition(property, &[DynamicSelector::Os(OsCondition::Windows)])
    }

    /// Create a property for macOS only (const version)
    #[must_use] pub const fn on_macos(property: CssProperty) -> Self {
        Self::with_single_condition(property, &[DynamicSelector::Os(OsCondition::MacOS)])
    }

    /// Create a property for Linux only (const version)
    #[must_use] pub const fn on_linux(property: CssProperty) -> Self {
        Self::with_single_condition(property, &[DynamicSelector::Os(OsCondition::Linux)])
    }

    /// Check if this property matches in the given context
    #[must_use] pub fn matches(&self, ctx: &DynamicSelectorContext) -> bool {
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
    #[must_use] pub fn is_conditional(&self) -> bool {
        !self.apply_if.as_slice().is_empty()
    }

    /// Check if this property is a pseudo-state conditional only
    /// (hover, active, focus, etc.)
    #[must_use] pub fn is_pseudo_state_only(&self) -> bool {
        let conditions = self.apply_if.as_slice();
        !conditions.is_empty()
            && conditions
                .iter()
                .all(|c| matches!(c, DynamicSelector::PseudoState(_)))
    }

    /// Check if this property affects layout (width, height, margin, etc.)
    /// 
    /// Returns `true` for layout-affecting properties like width, height, margin, padding,
    /// font-size, etc. Returns `false` for paint-only properties like color, background,
    /// box-shadow, opacity, transform, etc.
    #[must_use] pub const fn is_layout_affecting(&self) -> bool {
        self.property.get_type().can_trigger_relayout()
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
        // Lexicographic, matching the `impl_vec_partialord!` PartialOrd above and the
        // element `Ord`; previously this compared length only (inconsistent with Eq).
        self.as_slice().cmp(other.as_slice())
    }
}

impl core::hash::Hash for CssPropertyWithConditions {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.property.hash(state);
        // Hash the full set of conditions (length + each selector, via the now-`Hash`
        // `DynamicSelector`) so the hash agrees with `Eq`/`Ord` instead of colliding
        // on condition count alone.
        self.apply_if.as_slice().hash(state);
    }
}

impl core::hash::Hash for CssPropertyWithConditionsVec {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        // Hashing the slice folds in the length as well as every element.
        self.as_slice().hash(state);
    }
}

impl CssPropertyWithConditionsVec {
    /// Parse CSS with support for selectors and nesting.
    /// 
    /// Supports:
    /// - Simple properties: `color: red;`
    /// - Pseudo-selectors: `:hover { background: blue; }`
    /// - @-rules: `@os linux { font-size: 14px; }`
    /// - Nesting: `@os linux { font-size: 14px; :hover { color: red; }}`
    /// 
    /// Examples:
    /// ```ignore
    /// // Simple inline styles
    /// CssPropertyWithConditionsVec::parse("color: red; font-size: 14px;")
    /// 
    /// // With hover state
    /// CssPropertyWithConditionsVec::parse(":hover { background: blue; }")
    /// 
    /// // OS-specific with nested hover
    /// CssPropertyWithConditionsVec::parse("@os linux { font-size: 14px; :hover { color: red; }}")
    /// ```
    #[cfg(feature = "parser")]
    #[must_use] pub fn parse(style: &str) -> Self {
        Self::parse_with_conditions(style, &[])
    }
    
    /// Internal recursive parser with inherited conditions
    #[cfg(feature = "parser")]
    fn parse_with_conditions(style: &str, inherited_conditions: &[DynamicSelector]) -> Self {
        use crate::props::property::{
            parse_combined_css_property, parse_css_property, CombinedCssPropertyType, CssKeyMap,
            CssPropertyType,
        };

        let mut props = Vec::new();
        let key_map = CssKeyMap::get();
        let style = style.trim();
        
        if style.is_empty() {
            return Self::from_vec(props);
        }

        // Tokenize into segments: properties, pseudo-selectors, and @-rules
        let chars = style.chars();
        let mut current_segment = String::new();
        let mut brace_depth = 0;
        
        for c in chars {
            match c {
                '{' => {
                    brace_depth += 1;
                    current_segment.push(c);
                }
                '}' => {
                    brace_depth -= 1;
                    current_segment.push(c);
                    
                    if brace_depth == 0 {
                        // End of a block - process it
                        let segment = current_segment.trim().to_string();
                        current_segment.clear();
                        
                        if let Some(parsed) = Self::parse_block_segment(&segment, inherited_conditions, &key_map) {
                            props.extend(parsed);
                        }
                    }
                }
                ';' if brace_depth == 0 => {
                    // End of a simple property
                    let segment = current_segment.trim().to_string();
                    current_segment.clear();
                    
                    if !segment.is_empty() {
                        if let Some(parsed) = Self::parse_property_segment(&segment, inherited_conditions, &key_map) {
                            props.extend(parsed);
                        }
                    }
                }
                _ => {
                    current_segment.push(c);
                }
            }
        }
        
        // Handle any remaining segment (property without trailing semicolon)
        let remaining = current_segment.trim();
        if !remaining.is_empty() && !remaining.contains('{') {
            if let Some(parsed) = Self::parse_property_segment(remaining, inherited_conditions, &key_map) {
                props.extend(parsed);
            }
        }

        Self::from_vec(props)
    }
    
    /// Parse a block segment like `:hover { ... }` or `@os linux { ... }`
    #[cfg(feature = "parser")]
    fn parse_block_segment(
        segment: &str,
        inherited_conditions: &[DynamicSelector],
        key_map: &crate::props::property::CssKeyMap,
    ) -> Option<Vec<CssPropertyWithConditions>> {
        // Find the opening brace
        let brace_pos = segment.find('{')?;
        let selector = segment[..brace_pos].trim();
        
        // Extract content between braces (excluding the braces themselves)
        let content_start = brace_pos + 1;
        let content_end = segment.rfind('}')?;
        if content_end <= content_start {
            return None;
        }
        let content = &segment[content_start..content_end];
        
        // Parse selector to get conditions
        let mut conditions = inherited_conditions.to_vec();
        
        if let Some(new_conditions) = Self::parse_selector_to_conditions(selector) {
            conditions.extend(new_conditions);
        } else {
            // Unknown selector, skip this block
            return None;
        }
        
        // Recursively parse the content with the new conditions
        let parsed = Self::parse_with_conditions(content, &conditions);
        Some(parsed.into_library_owned_vec())
    }
    
    /// Parse a selector string into `DynamicSelector` conditions
    #[cfg(feature = "parser")]
    fn parse_selector_to_conditions(selector: &str) -> Option<Vec<DynamicSelector>> {
        let selector = selector.trim();

        // Handle pseudo-selectors
        if let Some(pseudo) = selector.strip_prefix(':') {
            match pseudo {
                "hover" => return Some(vec![DynamicSelector::PseudoState(PseudoStateType::Hover)]),
                "active" => return Some(vec![DynamicSelector::PseudoState(PseudoStateType::Active)]),
                "focus" => return Some(vec![DynamicSelector::PseudoState(PseudoStateType::Focus)]),
                "focus-within" => return Some(vec![DynamicSelector::PseudoState(PseudoStateType::FocusWithin)]),
                "disabled" => return Some(vec![DynamicSelector::PseudoState(PseudoStateType::Disabled)]),
                "checked" => return Some(vec![DynamicSelector::PseudoState(PseudoStateType::CheckedTrue)]),
                "visited" => return Some(vec![DynamicSelector::PseudoState(PseudoStateType::Visited)]),
                "backdrop" => return Some(vec![DynamicSelector::PseudoState(PseudoStateType::Backdrop)]),
                "dragging" => return Some(vec![DynamicSelector::PseudoState(PseudoStateType::Dragging)]),
                "drag-over" => return Some(vec![DynamicSelector::PseudoState(PseudoStateType::DragOver)]),
                _ => return None,
            }
        }

        // Handle @-rules
        if let Some(rule_content) = selector.strip_prefix('@') {
            return Self::parse_at_rule(rule_content);
        }

        // Handle universal selector * (treat as unconditional)
        if selector == "*" {
            return Some(vec![]);
        }

        // Empty selector means unconditional
        if selector.is_empty() {
            return Some(vec![]);
        }
        
        None
    }

    /// Parse an @-rule (the content after '@') into `DynamicSelector` conditions.
    /// Handles @os, @media, @theme, @lang, @container,
    /// @prefers-reduced-motion, and @prefers-high-contrast.
    #[cfg(feature = "parser")]
    fn parse_at_rule(rule_content: &str) -> Option<Vec<DynamicSelector>> {
        // @os linux                    -- bare family
        // @os(linux)                   -- family in parens
        // @os(linux:gnome)             -- family + desktop env
        // @os(windows >= win-11)       -- family + version
        // @os(linux:gnome > 40)        -- family + DE + DE version
        if let Some(rest) = rule_content
            .strip_prefix("os ")
            .or_else(|| if rule_content.starts_with("os(") { Some(&rule_content[2..]) } else { None })
        {
            if let Some(conds) = parse_os_at_rule_content(rest) {
                return Some(conds);
            }
        }

        // @media (min-width: 800px), etc.
        if let Some(rest) = rule_content.strip_prefix("media ") {
            let media_query = rest.trim();
            if let Some(media_conds) = Self::parse_media_query(media_query) {
                return Some(media_conds);
            }
        }

        // @theme dark, @theme light
        if let Some(rest) = rule_content.strip_prefix("theme ") {
            let theme = rest.trim();
            match theme {
                "dark" => return Some(vec![DynamicSelector::Theme(ThemeCondition::Dark)]),
                "light" => return Some(vec![DynamicSelector::Theme(ThemeCondition::Light)]),
                _ => return None,
            }
        }

        // @lang("de-DE") or @lang de-DE
        let lang_body = rule_content
            .strip_prefix("lang(")
            .map(|r| r.trim_end_matches(')').trim())
            .or_else(|| rule_content.strip_prefix("lang ").map(str::trim));
        if let Some(lang_str) = lang_body {
            let lang_str = lang_str
                .strip_prefix('"').and_then(|s| s.strip_suffix('"'))
                .or_else(|| lang_str.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')))
                .unwrap_or(lang_str);
            if !lang_str.is_empty() {
                return Some(vec![DynamicSelector::Language(
                    LanguageCondition::Prefix(AzString::from(lang_str.to_string()))
                )]);
            }
        }

        // @container (min-width: 400px) or @container sidebar (min-width: 400px)
        if rule_content.starts_with("container ") || rule_content.starts_with("container(") {
            let container_str = if rule_content.starts_with("container(") {
                &rule_content[9..] // keep the '(' for parsing
            } else {
                rule_content[10..].trim()
            };
            let mut conds = Vec::new();
            // Check for named container: "sidebar (min-width: 400px)"
            let (name_part, query_part) = if container_str.starts_with('(') {
                (None, container_str)
            } else if let Some(paren_idx) = container_str.find('(') {
                let name = container_str[..paren_idx].trim();
                if name.is_empty() {
                    (None, container_str)
                } else {
                    (Some(name), &container_str[paren_idx..])
                }
            } else {
                if !container_str.is_empty() {
                    return Some(vec![DynamicSelector::ContainerName(
                        AzString::from(container_str.to_string())
                    )]);
                }
                return None;
            };
            if let Some(name) = name_part {
                conds.push(DynamicSelector::ContainerName(
                    AzString::from(name.to_string())
                ));
            }
            // Parse (min-width: 400px) style conditions
            if let Some(inner) = query_part.strip_prefix('(').and_then(|s| s.strip_suffix(')')) {
                if let Some((key, value)) = inner.split_once(':') {
                    let key = key.trim();
                    let value = value.trim();
                    let px_value = value.strip_suffix("px")
                        .and_then(|v| v.trim().parse::<f32>().ok());
                    match key {
                        "min-width" => { if let Some(px) = px_value { conds.push(DynamicSelector::ContainerWidth(MinMaxRange::with_min(px))); } }
                        "max-width" => { if let Some(px) = px_value { conds.push(DynamicSelector::ContainerWidth(MinMaxRange::with_max(px))); } }
                        "min-height" => { if let Some(px) = px_value { conds.push(DynamicSelector::ContainerHeight(MinMaxRange::with_min(px))); } }
                        "max-height" => { if let Some(px) = px_value { conds.push(DynamicSelector::ContainerHeight(MinMaxRange::with_max(px))); } }
                        _ => {}
                    }
                }
            }
            if !conds.is_empty() {
                return Some(conds);
            }
        }

        // @prefers-reduced-motion or @reduced-motion
        if rule_content == "prefers-reduced-motion" || rule_content == "reduced-motion" {
            return Some(vec![DynamicSelector::PrefersReducedMotion(BoolCondition::True)]);
        }

        // @prefers-high-contrast or @high-contrast
        if rule_content == "prefers-high-contrast" || rule_content == "high-contrast" {
            return Some(vec![DynamicSelector::PrefersHighContrast(BoolCondition::True)]);
        }

        None
    }

    /// Parse simple media query
    #[cfg(feature = "parser")]
    fn parse_media_query(query: &str) -> Option<Vec<DynamicSelector>> {
        let query = query.trim();
        
        // Handle (min-width: XXXpx)
        if query.starts_with('(') && query.ends_with(')') {
            let inner = &query[1..query.len()-1];
            if let Some((key, value)) = inner.split_once(':') {
                let key = key.trim();
                let value = value.trim();
                
                // Parse pixel value
                let px_value = value.strip_suffix("px")
                    .and_then(|v| v.trim().parse::<f32>().ok());
                
                match key {
                    "min-width" => {
                        if let Some(px) = px_value {
                            return Some(vec![DynamicSelector::ViewportWidth(
                                MinMaxRange::with_min(px)
                            )]);
                        }
                    }
                    "max-width" => {
                        if let Some(px) = px_value {
                            return Some(vec![DynamicSelector::ViewportWidth(
                                MinMaxRange::with_max(px)
                            )]);
                        }
                    }
                    "min-height" => {
                        if let Some(px) = px_value {
                            return Some(vec![DynamicSelector::ViewportHeight(
                                MinMaxRange::with_min(px)
                            )]);
                        }
                    }
                    "max-height" => {
                        if let Some(px) = px_value {
                            return Some(vec![DynamicSelector::ViewportHeight(
                                MinMaxRange::with_max(px)
                            )]);
                        }
                    }
                    other => {
                        // Try orientation, prefers-color-scheme, prefers-reduced-motion, etc.
                        if let Some(sel) = Self::parse_media_feature_inline(other, value) {
                            return Some(vec![sel]);
                        }
                    }
                }
            }
        }
        
        // Handle screen, print, all
        match query {
            "screen" => Some(vec![DynamicSelector::Media(MediaType::Screen)]),
            "print" => Some(vec![DynamicSelector::Media(MediaType::Print)]),
            "all" => Some(vec![DynamicSelector::Media(MediaType::All)]),
            _ => None,
        }
    }

    /// Parse a media query feature value into a `DynamicSelector`
    /// Handles features like orientation, prefers-color-scheme, prefers-reduced-motion, etc.
    #[cfg(feature = "parser")]
    fn parse_media_feature_inline(key: &str, value: &str) -> Option<DynamicSelector> {
        match key {
            "orientation" => {
                if value.eq_ignore_ascii_case("portrait") {
                    Some(DynamicSelector::Orientation(OrientationType::Portrait))
                } else if value.eq_ignore_ascii_case("landscape") {
                    Some(DynamicSelector::Orientation(OrientationType::Landscape))
                } else {
                    None
                }
            }
            "prefers-color-scheme" => {
                if value.eq_ignore_ascii_case("dark") {
                    Some(DynamicSelector::Theme(ThemeCondition::Dark))
                } else if value.eq_ignore_ascii_case("light") {
                    Some(DynamicSelector::Theme(ThemeCondition::Light))
                } else {
                    None
                }
            }
            "prefers-reduced-motion" => {
                if value.eq_ignore_ascii_case("reduce") {
                    Some(DynamicSelector::PrefersReducedMotion(BoolCondition::True))
                } else if value.eq_ignore_ascii_case("no-preference") {
                    Some(DynamicSelector::PrefersReducedMotion(BoolCondition::False))
                } else {
                    None
                }
            }
            "prefers-contrast" | "prefers-high-contrast" => {
                if value.eq_ignore_ascii_case("more") || value.eq_ignore_ascii_case("high") || value.eq_ignore_ascii_case("active") {
                    Some(DynamicSelector::PrefersHighContrast(BoolCondition::True))
                } else if value.eq_ignore_ascii_case("no-preference") || value.eq_ignore_ascii_case("none") {
                    Some(DynamicSelector::PrefersHighContrast(BoolCondition::False))
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// Parse a simple property like "color: red"
    #[cfg(feature = "parser")]
    fn parse_property_segment(
        segment: &str,
        inherited_conditions: &[DynamicSelector],
        key_map: &crate::props::property::CssKeyMap,
    ) -> Option<Vec<CssPropertyWithConditions>> {
        use crate::props::property::{
            parse_combined_css_property, parse_css_property, CombinedCssPropertyType,
            CssPropertyType,
        };

        let segment = segment.trim();
        if segment.is_empty() {
            return None;
        }
        
        let (key, value) = segment.split_once(':')?;
        let key = key.trim();
        let value = value.trim();
        
        let mut props = Vec::new();
        let conditions = if inherited_conditions.is_empty() {
            DynamicSelectorVec::from_const_slice(&[])
        } else {
            DynamicSelectorVec::from_vec(inherited_conditions.to_vec())
        };
        
        // First, try to parse as a regular (non-shorthand) property
        if let Some(prop_type) = CssPropertyType::from_str(key, key_map) {
            if let Ok(prop) = parse_css_property(prop_type, value) {
                props.push(CssPropertyWithConditions {
                    property: prop,
                    apply_if: conditions,
                });
                return Some(props);
            }
        }
        
        // If not found, try as a shorthand (combined) property
        if let Some(combined_type) = CombinedCssPropertyType::from_str(key, key_map) {
            if let Ok(expanded_props) = parse_combined_css_property(combined_type, value) {
                for prop in expanded_props {
                    props.push(CssPropertyWithConditions {
                        property: prop,
                        apply_if: conditions.clone(),
                    });
                }
                return Some(props);
            }
        }
        
        None
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_inline_overflow_parse() {
        let style = "overflow: scroll;";
        let parsed = CssPropertyWithConditionsVec::parse(style);
        let props = parsed.into_library_owned_vec();
        assert!(!props.is_empty(), "Expected overflow to parse into at least 1 property");
    }

    #[test]
    fn test_inline_overflow_y_parse() {
        let style = "overflow-y: scroll;";
        let parsed = CssPropertyWithConditionsVec::parse(style);
        let props = parsed.into_library_owned_vec();
        assert!(!props.is_empty(), "Expected overflow-y to parse into at least 1 property");
    }

    #[test]
    fn test_inline_combined_style_with_overflow() {
        let style = "padding: 20px; background-color: #f0f0f0; font-size: 14px; color: #222;overflow: scroll;";
        let parsed = CssPropertyWithConditionsVec::parse(style);
        let props = parsed.into_library_owned_vec();
        // padding:20px expands to 4, background:1, font-size:1, color:1, overflow:2 = 10
        assert!(props.len() >= 9, "Expected at least 9 properties, got {}", props.len());
    }

    #[test]
    fn test_inline_grid_template_columns_parse() {
        use crate::props::layout::grid::GridTrackSizing;
        let style = "display: grid; grid-template-columns: repeat(4, 160px); gap: 16px; padding: 10px;";
        let parsed = CssPropertyWithConditionsVec::parse(style);
        let props = parsed.into_library_owned_vec();
        // Find grid-template-columns property
        let grid_cols = props.iter().find(|p| {
            matches!(p.property, CssProperty::GridTemplateColumns(_))
        }).expect("Expected GridTemplateColumns property");

        if let CssProperty::GridTemplateColumns(ref value) = grid_cols.property {
            let template = value.get_property().expect("Expected Exact value");
            let tracks = template.tracks.as_ref();
            assert_eq!(tracks.len(), 4, "Expected 4 tracks");
            for (i, track) in tracks.iter().enumerate() {
                assert!(matches!(track, GridTrackSizing::Fixed(_)),
                    "Track {i} should be Fixed(160px), got {track:?}");
            }
        } else {
            panic!("Expected CssProperty::GridTemplateColumns");
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::float_cmp,
    clippy::too_many_lines,
    clippy::cast_precision_loss,
    clippy::field_reassign_with_default,
    clippy::unreadable_literal
)]
mod autotest_generated {
    use core::cmp::Ordering;
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    use super::*;
    use crate::props::property::CssPropertyType;

    // ---------------------------------------------------------------
    // helpers
    // ---------------------------------------------------------------

    fn hash_of<T: Hash>(t: &T) -> u64 {
        let mut h = DefaultHasher::new();
        t.hash(&mut h);
        h.finish()
    }

    /// A paint-only property (does not trigger relayout).
    fn paint_prop() -> CssProperty {
        CssProperty::const_none(CssPropertyType::TextColor)
    }

    /// A layout-affecting property.
    fn layout_prop() -> CssProperty {
        CssProperty::const_none(CssPropertyType::Width)
    }

    /// Every `DynamicSelector` variant, in discriminant order.
    fn all_selector_variants() -> Vec<DynamicSelector> {
        vec![
            DynamicSelector::Os(OsCondition::Linux),
            DynamicSelector::OsVersion(OsVersionCondition::Min(OsVersion::WIN_11)),
            DynamicSelector::Media(MediaType::Print),
            DynamicSelector::ViewportWidth(MinMaxRange::with_min(1.0)),
            DynamicSelector::ViewportHeight(MinMaxRange::with_max(2.0)),
            DynamicSelector::ContainerWidth(MinMaxRange::new(Some(1.0), Some(2.0))),
            DynamicSelector::ContainerHeight(MinMaxRange::new(None, None)),
            DynamicSelector::ContainerName(AzString::from_const_str("sidebar")),
            DynamicSelector::Theme(ThemeCondition::Dark),
            DynamicSelector::AspectRatio(MinMaxRange::with_min(0.5)),
            DynamicSelector::Orientation(OrientationType::Portrait),
            DynamicSelector::PrefersReducedMotion(BoolCondition::True),
            DynamicSelector::PrefersHighContrast(BoolCondition::False),
            DynamicSelector::PseudoState(PseudoStateType::Hover),
            DynamicSelector::Language(LanguageCondition::Prefix(AzString::from_const_str("de"))),
        ]
    }

    /// Adversarial input corpus reused across every string parser under test.
    fn nasty_strings() -> Vec<String> {
        vec![
            String::new(),
            " ".to_string(),
            "   \t\n\r  ".to_string(),
            "\0".to_string(),
            "0".to_string(),
            "-0".to_string(),
            "-1".to_string(),
            "NaN".to_string(),
            "nan".to_string(),
            "inf".to_string(),
            "-inf".to_string(),
            "infinity".to_string(),
            "1e400".to_string(),
            i64::MAX.to_string(),
            i64::MIN.to_string(),
            u32::MAX.to_string(),
            u64::MAX.to_string(),
            "9999999999999999999999999999".to_string(),
            "1.7976931348623157e308".to_string(),
            "\u{1F600}".to_string(),
            "e\u{301}\u{301}\u{301}".to_string(),
            "日本語".to_string(),
            "\u{202e}gnome".to_string(),
            "  linux  ".to_string(),
            "linux;garbage".to_string(),
            "linux)".to_string(),
            "((((".to_string(),
            "))))".to_string(),
            ">=".to_string(),
            "<=<=<=".to_string(),
            ":::::".to_string(),
            "-".to_string(),
            "_".to_string(),
            ".".to_string(),
            "..".to_string(),
            "...".to_string(),
            "1.2.3.4.5.6".to_string(),
            "%s%s%n".to_string(),
            "\\x00\\xff".to_string(),
            "a".repeat(100_000),
            "1".repeat(100_000),
            ".".repeat(10_000),
            "(".repeat(5_000),
            "🦀".repeat(10_000),
        ]
    }

    // ---------------------------------------------------------------
    // 1. PseudoStateFlags::has_state  (predicate)
    // ---------------------------------------------------------------

    #[test]
    fn has_state_default_flags_only_normal_and_checked_false() {
        let flags = PseudoStateFlags::default();
        // Normal is the base state and is always active.
        assert!(flags.has_state(PseudoStateType::Normal));
        // `checked: false` means :not(:checked) is active.
        assert!(flags.has_state(PseudoStateType::CheckedFalse));
        for state in [
            PseudoStateType::Hover,
            PseudoStateType::Active,
            PseudoStateType::Focus,
            PseudoStateType::Disabled,
            PseudoStateType::CheckedTrue,
            PseudoStateType::FocusWithin,
            PseudoStateType::Visited,
            PseudoStateType::Backdrop,
            PseudoStateType::Dragging,
            PseudoStateType::DragOver,
        ] {
            assert!(!flags.has_state(state), "{state:?} must be off by default");
        }
    }

    #[test]
    fn has_state_all_flags_set_reports_every_state_except_checked_false() {
        let flags = PseudoStateFlags {
            hover: true,
            active: true,
            focused: true,
            disabled: true,
            checked: true,
            focus_within: true,
            visited: true,
            backdrop: true,
            dragging: true,
            drag_over: true,
        };
        assert!(flags.has_state(PseudoStateType::Hover));
        assert!(flags.has_state(PseudoStateType::CheckedTrue));
        // CheckedTrue and CheckedFalse must always be mutually exclusive.
        assert!(!flags.has_state(PseudoStateType::CheckedFalse));
        assert!(flags.has_state(PseudoStateType::DragOver));
        assert!(flags.has_state(PseudoStateType::Normal));
    }

    #[test]
    fn has_state_checked_true_and_false_are_never_both_active() {
        for checked in [false, true] {
            let flags = PseudoStateFlags {
                checked,
                ..PseudoStateFlags::default()
            };
            assert_ne!(
                flags.has_state(PseudoStateType::CheckedTrue),
                flags.has_state(PseudoStateType::CheckedFalse),
                "checked={checked}: CheckedTrue/CheckedFalse must be complementary"
            );
        }
    }

    // ---------------------------------------------------------------
    // 2. DynamicSelector::variant_tag  (getter)
    // ---------------------------------------------------------------

    #[test]
    fn variant_tag_matches_declared_repr_discriminants() {
        for (expected, sel) in all_selector_variants().iter().enumerate() {
            let expected = u8::try_from(expected).expect("15 variants fit in u8");
            assert_eq!(
                sel.variant_tag(),
                expected,
                "variant_tag drifted from the #[repr(C, u8)] discriminant for {sel:?}"
            );
        }
    }

    #[test]
    fn variant_tag_is_unique_per_variant() {
        let variants = all_selector_variants();
        let mut tags: Vec<u8> = variants.iter().map(DynamicSelector::variant_tag).collect();
        tags.sort_unstable();
        tags.dedup();
        assert_eq!(tags.len(), variants.len(), "variant tags must be unique");
    }

    #[test]
    fn ord_is_keyed_on_variant_tag_first() {
        let variants = all_selector_variants();
        for w in variants.windows(2) {
            assert_eq!(
                w[0].cmp(&w[1]),
                Ordering::Less,
                "selectors must sort by variant tag: {:?} < {:?}",
                w[0],
                w[1]
            );
        }
    }

    #[test]
    fn hash_distinguishes_variants_carrying_the_same_payload() {
        // ViewportWidth / ContainerWidth carry identical payloads but must not collide,
        // because `variant_tag` is folded into the hash first.
        let range = MinMaxRange::with_min(800.0);
        let a = DynamicSelector::ViewportWidth(range);
        let b = DynamicSelector::ContainerWidth(range);
        assert_ne!(hash_of(&a), hash_of(&b));
        assert_ne!(a.cmp(&b), Ordering::Equal);
    }

    #[test]
    fn hash_and_ord_agree_for_nan_carrying_ranges() {
        // Both are implemented over the *bit pattern*, so two structurally identical
        // NaN-sentinel ranges must compare Equal and hash the same.
        let a = DynamicSelector::ViewportWidth(MinMaxRange::with_min(800.0));
        let b = DynamicSelector::ViewportWidth(MinMaxRange::with_min(800.0));
        assert_eq!(a.cmp(&b), Ordering::Equal);
        assert_eq!(hash_of(&a), hash_of(&b));
    }

    // RED (genuine bug): `MinMaxRange` derives `PartialEq` over raw `f32`s, but the type
    // uses NaN as the "no limit" sentinel. NaN != NaN, so a selector built by
    // `MinMaxRange::with_min`/`with_max` (i.e. every `@media (min-width: ...)` selector)
    // is not even equal to itself. `impl Eq for DynamicSelector` is therefore unsound,
    // and `Ord` (which compares bit patterns) reports `Equal` where `PartialEq` reports
    // `false` — breaking the Ord/Eq contract for BTreeMap/BTreeSet/dedup.
    #[test]
    fn nan_sentinel_range_selector_is_reflexive_under_partial_eq() {
        let a = DynamicSelector::ViewportWidth(MinMaxRange::with_min(800.0));
        let b = DynamicSelector::ViewportWidth(MinMaxRange::with_min(800.0));
        assert_eq!(a, b, "Eq requires reflexivity, but the NaN `max` sentinel breaks it");
    }

    // RED (same root cause, stated as the Ord/Eq contract it violates).
    #[test]
    fn ord_equal_implies_partial_eq_for_range_selectors() {
        let a = DynamicSelector::ViewportHeight(MinMaxRange::with_max(600.0));
        let b = a.clone();
        assert_eq!(a.cmp(&b), Ordering::Equal);
        assert!(
            a == b,
            "cmp() == Equal must imply == (Ord/Eq contract); NaN sentinel breaks it"
        );
    }

    // ---------------------------------------------------------------
    // 3-7. MinMaxRange constructors + getters
    // ---------------------------------------------------------------

    #[test]
    fn min_max_range_new_roundtrips_finite_values() {
        let r = MinMaxRange::new(Some(1.5), Some(9.5));
        assert_eq!(r.min(), Some(1.5));
        assert_eq!(r.max(), Some(9.5));
    }

    #[test]
    fn min_max_range_new_none_encodes_nan_sentinel() {
        let r = MinMaxRange::new(None, None);
        assert!(r.min.is_nan());
        assert!(r.max.is_nan());
        assert_eq!(r.min(), None);
        assert_eq!(r.max(), None);
    }

    #[test]
    fn min_max_range_new_nan_argument_is_indistinguishable_from_none() {
        // Documented sentinel behaviour: NaN *is* "no limit", so a caller passing
        // `Some(NAN)` gets `None` back. Assert it rather than letting it surprise.
        let r = MinMaxRange::new(Some(f32::NAN), Some(f32::NAN));
        assert_eq!(r.min(), None);
        assert_eq!(r.max(), None);
        assert!(r.matches(0.0));
        assert!(r.matches(f32::MAX));
    }

    #[test]
    fn min_max_range_with_min_and_with_max_leave_the_other_side_open() {
        let lo = MinMaxRange::with_min(-0.0);
        assert_eq!(lo.min(), Some(-0.0));
        assert_eq!(lo.max(), None);

        let hi = MinMaxRange::with_max(f32::MAX);
        assert_eq!(hi.min(), None);
        assert_eq!(hi.max(), Some(f32::MAX));
    }

    #[test]
    fn min_max_range_getters_survive_extreme_values() {
        for v in [
            0.0_f32,
            -0.0,
            f32::MIN,
            f32::MAX,
            f32::MIN_POSITIVE,
            f32::EPSILON,
            f32::INFINITY,
            f32::NEG_INFINITY,
        ] {
            let r = MinMaxRange::new(Some(v), Some(v));
            assert_eq!(r.min(), Some(v));
            assert_eq!(r.max(), Some(v));
        }
    }

    // ---------------------------------------------------------------
    // 8. MinMaxRange::matches  (numeric)
    // ---------------------------------------------------------------

    #[test]
    fn matches_zero_boundary_is_inclusive() {
        assert!(MinMaxRange::with_min(0.0).matches(0.0));
        assert!(MinMaxRange::with_max(0.0).matches(0.0));
        assert!(MinMaxRange::new(Some(0.0), Some(0.0)).matches(0.0));
        // IEEE-754: -0.0 == 0.0, so both bounds accept it.
        assert!(MinMaxRange::with_min(0.0).matches(-0.0));
        assert!(MinMaxRange::with_max(0.0).matches(-0.0));
    }

    #[test]
    fn matches_negative_values_are_ordered_correctly() {
        let r = MinMaxRange::new(Some(-10.0), Some(-1.0));
        assert!(r.matches(-10.0));
        assert!(r.matches(-5.0));
        assert!(r.matches(-1.0));
        assert!(!r.matches(-10.001));
        assert!(!r.matches(0.0));
    }

    #[test]
    fn matches_at_float_extremes_does_not_panic() {
        let open = MinMaxRange::new(None, None);
        let bounded = MinMaxRange::new(Some(f32::MIN), Some(f32::MAX));
        for v in [
            f32::MIN,
            f32::MAX,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::MIN_POSITIVE,
            -f32::MIN_POSITIVE,
        ] {
            // Open range accepts everything (both sentinels are NaN).
            assert!(open.matches(v), "open range must accept {v}");
        }
        assert!(bounded.matches(0.0));
        assert!(bounded.matches(f32::MIN));
        assert!(bounded.matches(f32::MAX));
        // Infinities fall outside a MIN..=MAX range.
        assert!(!bounded.matches(f32::INFINITY));
        assert!(!bounded.matches(f32::NEG_INFINITY));
    }

    #[test]
    fn matches_nan_value_is_rejected_by_any_real_bound() {
        // NaN compares false against everything, so any *actual* bound rejects it.
        assert!(!MinMaxRange::with_min(0.0).matches(f32::NAN));
        assert!(!MinMaxRange::with_max(0.0).matches(f32::NAN));
        assert!(!MinMaxRange::new(Some(1.0), Some(2.0)).matches(f32::NAN));
        // ...but a fully-open range has no bound to reject it.
        assert!(MinMaxRange::new(None, None).matches(f32::NAN));
    }

    #[test]
    fn matches_infinite_bounds_are_deterministic() {
        let min_inf = MinMaxRange::with_min(f32::INFINITY);
        assert!(!min_inf.matches(f32::MAX));
        assert!(min_inf.matches(f32::INFINITY));

        let max_neg_inf = MinMaxRange::with_max(f32::NEG_INFINITY);
        assert!(!max_neg_inf.matches(f32::MIN));
        assert!(max_neg_inf.matches(f32::NEG_INFINITY));
    }

    #[test]
    fn matches_inverted_range_matches_nothing() {
        let inverted = MinMaxRange::new(Some(10.0), Some(5.0));
        for v in [-1.0_f32, 0.0, 5.0, 7.5, 10.0, 1e30] {
            assert!(!inverted.matches(v), "inverted range must reject {v}");
        }
    }

    // ---------------------------------------------------------------
    // 9. OsCondition::from_system_platform  (constructor)
    // ---------------------------------------------------------------

    #[test]
    fn os_condition_from_system_platform_covers_every_platform() {
        use crate::system::{DesktopEnvironment, Platform};
        assert_eq!(
            OsCondition::from_system_platform(&Platform::Windows),
            OsCondition::Windows
        );
        assert_eq!(
            OsCondition::from_system_platform(&Platform::MacOs),
            OsCondition::MacOS
        );
        assert_eq!(
            OsCondition::from_system_platform(&Platform::Ios),
            OsCondition::IOS
        );
        assert_eq!(
            OsCondition::from_system_platform(&Platform::Android),
            OsCondition::Android
        );
        assert_eq!(
            OsCondition::from_system_platform(&Platform::Linux(DesktopEnvironment::Gnome)),
            OsCondition::Linux
        );
        assert_eq!(
            OsCondition::from_system_platform(&Platform::Linux(DesktopEnvironment::Other(
                AzString::from_const_str(""),
            ))),
            OsCondition::Linux
        );
        // Unknown degrades to `Any`, which `match_os` treats as always-true.
        assert_eq!(
            OsCondition::from_system_platform(&Platform::Unknown),
            OsCondition::Any
        );
    }

    // ---------------------------------------------------------------
    // 10-15. OsVersion constructor / compare / predicates / unknown
    // ---------------------------------------------------------------

    #[test]
    fn os_version_new_stores_fields_verbatim_at_boundaries() {
        for id in [0, 1, u32::MAX - 1, u32::MAX] {
            let v = OsVersion::new(OsFamily::Linux, id);
            assert_eq!(v.os, OsFamily::Linux);
            assert_eq!(v.version_id, id);
        }
    }

    #[test]
    fn os_version_unknown_is_the_default_and_has_id_zero() {
        let u = OsVersion::unknown();
        assert_eq!(u.version_id, 0);
        assert_eq!(u, OsVersion::default());
    }

    #[test]
    fn os_version_compare_is_none_across_families() {
        let win = OsVersion::WIN_11;
        let mac = OsVersion::MACOS_SONOMA;
        assert_eq!(win.compare(&mac), None);
        assert_eq!(mac.compare(&win), None);
        // A None comparison must make *all three* predicates false — a cross-OS
        // condition can never accidentally match.
        assert!(!win.is_at_least(&mac));
        assert!(!win.is_at_most(&mac));
        assert!(!win.is_exactly(&mac));
    }

    #[test]
    fn os_version_compare_within_family_orders_by_id() {
        assert_eq!(
            OsVersion::WIN_10.compare(&OsVersion::WIN_11),
            Some(Ordering::Less)
        );
        assert_eq!(
            OsVersion::WIN_11.compare(&OsVersion::WIN_10),
            Some(Ordering::Greater)
        );
        assert_eq!(
            OsVersion::WIN_11.compare(&OsVersion::WIN_11_21H2),
            Some(Ordering::Equal)
        );
    }

    #[test]
    fn os_version_compare_at_id_extremes() {
        let lo = OsVersion::new(OsFamily::Android, 0);
        let hi = OsVersion::new(OsFamily::Android, u32::MAX);
        assert_eq!(lo.compare(&hi), Some(Ordering::Less));
        assert_eq!(hi.compare(&lo), Some(Ordering::Greater));
        assert!(hi.is_at_least(&lo));
        assert!(!hi.is_at_most(&lo));
        assert!(lo.is_at_most(&hi));
    }

    #[test]
    fn os_version_predicates_are_reflexive_and_consistent() {
        for v in [
            OsVersion::unknown(),
            OsVersion::WIN_XP,
            OsVersion::MACOS_TAHOE,
            OsVersion::IOS_18,
            OsVersion::ANDROID_15,
            OsVersion::new(OsFamily::Linux, u32::MAX),
        ] {
            assert!(v.is_at_least(&v), "{v:?} >= itself");
            assert!(v.is_at_most(&v), "{v:?} <= itself");
            assert!(v.is_exactly(&v), "{v:?} == itself");
        }
    }

    #[test]
    fn os_version_at_least_is_the_strict_complement_of_less_than() {
        let a = OsVersion::WIN_10;
        let b = OsVersion::WIN_11;
        assert!(!a.is_at_least(&b));
        assert!(a.is_at_most(&b));
        assert!(!a.is_exactly(&b));
        assert!(b.is_at_least(&a));
        assert!(!b.is_at_most(&a));
    }

    #[test]
    fn os_version_unknown_never_satisfies_a_min_constraint_on_a_real_version() {
        // `unknown()` reports OsFamily::Linux/0 — it must not silently satisfy
        // "at least Windows 11" (different family) nor "at least Linux 6.0".
        assert!(!OsVersion::unknown().is_at_least(&OsVersion::WIN_11));
        assert!(!OsVersion::unknown().is_at_least(&OsVersion::LINUX_6_0));
    }

    // ---------------------------------------------------------------
    // 16-22. OS version parsers
    // ---------------------------------------------------------------

    #[test]
    fn parse_os_version_valid_minimal_positive_controls() {
        assert_eq!(
            parse_os_version(OsFamily::Windows, "11"),
            Some(OsVersion::WIN_11)
        );
        assert_eq!(
            parse_os_version(OsFamily::MacOS, "sonoma"),
            Some(OsVersion::MACOS_SONOMA)
        );
        assert_eq!(
            parse_os_version(OsFamily::IOS, "17.0"),
            Some(OsVersion::IOS_17)
        );
        assert_eq!(
            parse_os_version(OsFamily::Android, "tiramisu"),
            Some(OsVersion::ANDROID_13)
        );
        assert_eq!(
            parse_os_version(OsFamily::Linux, "6.0"),
            Some(OsVersion::LINUX_6_0)
        );
    }

    #[test]
    fn parse_os_version_trims_and_lowercases() {
        assert_eq!(
            parse_os_version(OsFamily::Windows, "  WIN-11  "),
            Some(OsVersion::WIN_11)
        );
        assert_eq!(
            parse_os_version(OsFamily::MacOS, "\tBIG-SUR\n"),
            Some(OsVersion::MACOS_BIG_SUR)
        );
        assert_eq!(
            parse_os_version(OsFamily::Android, " KitKat "),
            Some(OsVersion::ANDROID_KITKAT)
        );
    }

    #[test]
    fn parse_os_version_empty_and_whitespace_is_none_for_every_family() {
        for os in [
            OsFamily::Windows,
            OsFamily::MacOS,
            OsFamily::IOS,
            OsFamily::Android,
            OsFamily::Linux,
        ] {
            assert_eq!(parse_os_version(os, ""), None, "{os:?} empty");
            assert_eq!(parse_os_version(os, "   "), None, "{os:?} spaces");
            assert_eq!(parse_os_version(os, "\t\n\r"), None, "{os:?} ws");
        }
    }

    #[test]
    fn parse_os_version_garbage_and_unicode_never_panics() {
        for os in [
            OsFamily::Windows,
            OsFamily::MacOS,
            OsFamily::IOS,
            OsFamily::Android,
            OsFamily::Linux,
        ] {
            for s in nasty_strings() {
                // Only requirement: terminate, do not panic, be deterministic.
                let a = parse_os_version(os, &s);
                let b = parse_os_version(os, &s);
                assert_eq!(a, b, "{os:?} not deterministic for {s:?}");
            }
        }
    }

    #[test]
    fn parse_os_version_leading_trailing_junk_is_rejected() {
        assert_eq!(parse_os_version(OsFamily::Windows, "win-11;drop"), None);
        assert_eq!(parse_os_version(OsFamily::MacOS, "sonoma!"), None);
        assert_eq!(parse_os_version(OsFamily::IOS, "17.0.0.0"), None);
        assert_eq!(parse_os_version(OsFamily::Android, "api"), None);
        assert_eq!(parse_os_version(OsFamily::Android, "api abc"), None);
    }

    #[test]
    fn parse_windows_version_prefix_forms_all_collapse_to_the_same_version() {
        for s in [
            "11",
            "win11",
            "win-11",
            "win_11",
            "windows11",
            "windows-11",
            "windows_11",
        ] {
            assert_eq!(
                parse_windows_version(s),
                Some(OsVersion::WIN_11),
                "{s} should parse to WIN_11"
            );
        }
    }

    #[test]
    fn parse_windows_version_bare_prefix_and_separator_only_are_none() {
        assert_eq!(parse_windows_version("win"), None);
        assert_eq!(parse_windows_version("windows"), None);
        assert_eq!(parse_windows_version("win-"), None);
        assert_eq!(parse_windows_version("windows_"), None);
        assert_eq!(parse_windows_version(""), None);
    }

    #[test]
    fn parse_windows_version_nt_aliases_agree_with_names() {
        assert_eq!(parse_windows_version("xp"), parse_windows_version("nt5.1"));
        assert_eq!(parse_windows_version("vista"), parse_windows_version("6.0"));
        assert_eq!(parse_windows_version("8.1"), parse_windows_version("8-1"));
        assert_eq!(parse_windows_version("10"), Some(OsVersion::WIN_10));
    }

    #[test]
    fn parse_windows_version_boundary_numbers_are_none() {
        for s in ["0", "-0", "-1", "NaN", "inf", "99999999999999999999"] {
            assert_eq!(parse_windows_version(s), None, "{s} must not parse");
        }
    }

    #[test]
    fn parse_windows_version_huge_and_unicode_input_terminates() {
        assert_eq!(parse_windows_version(&"win".repeat(200_000)), None);
        assert_eq!(parse_windows_version(&"1".repeat(1_000_000)), None);
        assert_eq!(parse_windows_version("\u{1F600}"), None);
        assert_eq!(parse_windows_version("win-\u{1F600}"), None);
    }

    #[test]
    fn strip_os_prefix_strips_prefix_and_optional_separator() {
        assert_eq!(strip_os_prefix("win-11", &["win"]), "11");
        assert_eq!(strip_os_prefix("win_11", &["win"]), "11");
        assert_eq!(strip_os_prefix("win11", &["win"]), "11");
        // Longest prefix must be listed first; that ordering is the caller's job.
        assert_eq!(strip_os_prefix("windows-11", &["windows", "win"]), "11");
        assert_eq!(strip_os_prefix("windows-11", &["win", "windows"]), "dows-11");
    }

    #[test]
    fn strip_os_prefix_leaves_non_matching_input_untouched() {
        assert_eq!(strip_os_prefix("11", &["win"]), "11");
        assert_eq!(strip_os_prefix("", &["win"]), "");
        assert_eq!(strip_os_prefix("anything", &[]), "anything");
        assert_eq!(strip_os_prefix("\u{1F600}win", &["win"]), "\u{1F600}win");
    }

    #[test]
    fn strip_os_prefix_only_strips_one_separator() {
        assert_eq!(strip_os_prefix("win--11", &["win"]), "-11");
        assert_eq!(strip_os_prefix("win-", &["win"]), "");
        assert_eq!(strip_os_prefix("win", &["win"]), "");
    }

    #[test]
    fn strip_os_prefix_with_empty_prefix_is_identity() {
        // An empty prefix matches everything; it must still not eat a leading char
        // other than a separator, and must not panic on multibyte input.
        assert_eq!(strip_os_prefix("日本語", &[""]), "日本語");
        assert_eq!(strip_os_prefix("-日本語", &[""]), "日本語");
    }

    #[test]
    fn parse_macos_version_names_and_numbers_agree() {
        assert_eq!(parse_macos_version("cheetah"), parse_macos_version("10.0"));
        assert_eq!(parse_macos_version("big-sur"), parse_macos_version("bigsur"));
        assert_eq!(parse_macos_version("bigsur"), parse_macos_version("11.0"));
        assert_eq!(parse_macos_version("tahoe"), Some(OsVersion::MACOS_TAHOE));
        assert_eq!(
            parse_macos_version("snow-leopard"),
            parse_macos_version("snowleopard")
        );
    }

    #[test]
    fn parse_macos_version_rejects_junk_and_terminates_on_huge_input() {
        for s in ["", " ", "sonoma ", "SONOMA", "10.16", "27", "🍎"] {
            assert_eq!(parse_macos_version(s), None, "{s:?} must not parse");
        }
        assert_eq!(parse_macos_version(&"10.".repeat(100_000)), None);
    }

    #[test]
    fn parse_macos_version_is_ordered_monotonically() {
        let names = [
            "cheetah", "puma", "jaguar", "panther", "tiger", "leopard", "lion", "mojave",
            "catalina", "bigsur", "monterey", "ventura", "sonoma", "sequoia", "tahoe",
        ];
        let ids: Vec<u32> = names
            .iter()
            .map(|n| {
                parse_macos_version(n)
                    .unwrap_or_else(|| panic!("{n} must parse"))
                    .version_id
            })
            .collect();
        for w in ids.windows(2) {
            assert!(w[0] < w[1], "macOS version ids must increase: {w:?}");
        }
    }

    #[test]
    fn parse_ios_version_boundaries() {
        assert_eq!(parse_ios_version("1"), Some(OsVersion::IOS_1));
        assert_eq!(parse_ios_version("18.0"), Some(OsVersion::IOS_18));
        assert_eq!(parse_ios_version("0"), None);
        assert_eq!(parse_ios_version("19"), None);
        assert_eq!(parse_ios_version(""), None);
        assert_eq!(parse_ios_version("-1"), None);
        assert_eq!(parse_ios_version("NaN"), None);
        assert_eq!(parse_ios_version(&"9".repeat(500_000)), None);
    }

    #[test]
    fn parse_android_version_api_level_escape_hatch() {
        assert_eq!(
            parse_android_version("api34"),
            Some(OsVersion::new(OsFamily::Android, 34))
        );
        assert_eq!(
            parse_android_version("api 34"),
            Some(OsVersion::new(OsFamily::Android, 34))
        );
        assert_eq!(
            parse_android_version("api0"),
            Some(OsVersion::new(OsFamily::Android, 0))
        );
        assert_eq!(
            parse_android_version(&format!("api{}", u32::MAX)),
            Some(OsVersion::new(OsFamily::Android, u32::MAX))
        );
    }

    #[test]
    fn parse_android_version_api_level_out_of_range_is_none_not_a_panic() {
        // u32::MAX + 1 and beyond must be rejected by `parse::<u32>()`, not wrap.
        assert_eq!(parse_android_version("api4294967296"), None);
        assert_eq!(parse_android_version("api-1"), None);
        assert_eq!(parse_android_version("api+1"), Some(OsVersion::new(OsFamily::Android, 1)));
        assert_eq!(parse_android_version("apiNaN"), None);
        assert_eq!(parse_android_version(&format!("api{}", "9".repeat(100_000))), None);
    }

    #[test]
    fn parse_android_version_named_releases() {
        assert_eq!(parse_android_version("q"), Some(OsVersion::ANDROID_10));
        assert_eq!(parse_android_version("13"), parse_android_version("t"));
        assert_eq!(
            parse_android_version("13"),
            parse_android_version("tiramisu")
        );
        assert_eq!(parse_android_version("15"), Some(OsVersion::ANDROID_15));
        assert_eq!(parse_android_version(""), None);
        assert_eq!(parse_android_version("🤖"), None);
    }

    #[test]
    fn parse_linux_version_accepts_bare_major_and_prefixes() {
        assert_eq!(
            parse_linux_version("5"),
            Some(OsVersion::new(OsFamily::Linux, 5000))
        );
        assert_eq!(parse_linux_version("6.0"), Some(OsVersion::LINUX_6_0));
        assert_eq!(parse_linux_version("linux6.0"), Some(OsVersion::LINUX_6_0));
        assert_eq!(parse_linux_version("linux-6.0"), Some(OsVersion::LINUX_6_0));
        assert_eq!(parse_linux_version("linux_6.0"), Some(OsVersion::LINUX_6_0));
        assert_eq!(
            parse_linux_version("6.17.0"),
            Some(OsVersion::new(OsFamily::Linux, 6170))
        );
    }

    #[test]
    fn parse_linux_version_rejects_malformed_and_unicode() {
        for s in [
            "", " ", ".", "..", "-1", "6.-1", "6.x", "x.6", "NaN", "inf", "🐧", "linux", "linux-",
        ] {
            assert_eq!(parse_linux_version(s), None, "{s:?} must not parse");
        }
    }

    #[test]
    fn parse_linux_version_ignores_everything_past_the_patch_component() {
        // Only major/minor/patch are consumed by `split('.')`; anything after the third
        // component is silently dropped — including outright garbage. Pinning the
        // behaviour so a future tightening is a deliberate change, not a surprise.
        assert_eq!(
            parse_linux_version("6.1.2"),
            Some(OsVersion::new(OsFamily::Linux, 6012))
        );
        assert_eq!(
            parse_linux_version("6.1.2.3"),
            Some(OsVersion::new(OsFamily::Linux, 6012))
        );
        assert_eq!(
            parse_linux_version("6.0.0.0extra"),
            Some(OsVersion::LINUX_6_0),
            "a 4th component is never parsed, so trailing junk is accepted"
        );
    }

    // RED (genuine bug): `major * 1000 + minor * 10 + patch` is unchecked u32 arithmetic.
    // A major >= 4_294_968 overflows and panics in any debug/overflow-checks build, and
    // wraps silently in release. The input is attacker-reachable from CSS via
    // `@os(linux >= 5000000)`, so a stylesheet can crash the app.
    #[test]
    fn parse_linux_version_huge_major_does_not_overflow() {
        assert_eq!(
            parse_linux_version("5000000"),
            None,
            "out-of-range kernel major must be rejected, not overflow u32"
        );
    }

    // RED (same root cause, via the minor component: `minor * 10` overflows).
    #[test]
    fn parse_linux_version_huge_minor_does_not_overflow() {
        assert_eq!(
            parse_linux_version("1.999999999"),
            None,
            "out-of-range kernel minor must be rejected, not overflow u32"
        );
    }

    #[test]
    fn parse_linux_version_max_u32_component_is_rejected_by_parse_not_by_math() {
        // `u32::MAX + 1` fails `parse::<u32>()` before any multiplication happens.
        assert_eq!(parse_linux_version("4294967296"), None);
    }

    // ---------------------------------------------------------------
    // 23-24. LinuxDesktopEnv / ThemeCondition converters
    // ---------------------------------------------------------------

    #[test]
    fn linux_desktop_env_from_system_maps_unknown_des_to_other() {
        use crate::system::DesktopEnvironment;
        assert_eq!(
            LinuxDesktopEnv::from_system_desktop_env(&DesktopEnvironment::Gnome),
            LinuxDesktopEnv::Gnome
        );
        assert_eq!(
            LinuxDesktopEnv::from_system_desktop_env(&DesktopEnvironment::Kde),
            LinuxDesktopEnv::KDE
        );
        // XFCE/Unity/Cinnamon/MATE are parse-only: they collapse to `Other` at runtime.
        for name in ["xfce", "", "🖥", &"x".repeat(10_000)] {
            assert_eq!(
                LinuxDesktopEnv::from_system_desktop_env(&DesktopEnvironment::Other(
                    AzString::from(name.to_string())
                )),
                LinuxDesktopEnv::Other
            );
        }
    }

    #[test]
    fn theme_condition_from_system_theme_is_total() {
        use crate::system::Theme;
        assert_eq!(
            ThemeCondition::from_system_theme(Theme::Light),
            ThemeCondition::Light
        );
        assert_eq!(
            ThemeCondition::from_system_theme(Theme::Dark),
            ThemeCondition::Dark
        );
    }

    // ---------------------------------------------------------------
    // 25. LanguageCondition::matches
    // ---------------------------------------------------------------

    #[test]
    fn language_exact_is_case_insensitive_and_strict() {
        let cond = LanguageCondition::Exact(AzString::from_const_str("de-DE"));
        assert!(cond.matches("de-DE"));
        assert!(cond.matches("DE-de"));
        assert!(!cond.matches("de"));
        assert!(!cond.matches("de-AT"));
        assert!(!cond.matches("de-DE-x"));
        assert!(!cond.matches(""));
    }

    #[test]
    fn language_prefix_matches_subtags_only_at_a_dash_boundary() {
        let cond = LanguageCondition::Prefix(AzString::from_const_str("de"));
        assert!(cond.matches("de"));
        assert!(cond.matches("de-DE"));
        assert!(cond.matches("DE-at"));
        // "den" must NOT match prefix "de" — the boundary has to be '-'.
        assert!(!cond.matches("den"));
        assert!(!cond.matches("deu"));
        assert!(!cond.matches("d"));
        assert!(!cond.matches(""));
    }

    #[test]
    fn language_empty_prefix_matches_only_the_empty_tag() {
        // An empty prefix is NOT a wildcard. `matches` is a subtag/dash-boundary
        // prefix matcher, and CSS agrees: for `[att^=val]`, "if val is the empty
        // string then the selector does not represent anything"
        // (Selectors Level 3 §6.3.2). Both `@lang()` parsers refuse to build
        // `Prefix("")` anyway, so this value is unreachable in practice.
        let cond = LanguageCondition::Prefix(AzString::from_const_str(""));
        assert!(cond.matches(""));
        assert!(!cond.matches("en-US"));
    }

    #[test]
    fn language_prefix_longer_than_input_is_false() {
        let cond = LanguageCondition::Prefix(AzString::from_const_str("de-DE-1996"));
        assert!(!cond.matches("de"));
        assert!(!cond.matches(""));
    }

    #[test]
    fn language_matches_huge_input_terminates() {
        let cond = LanguageCondition::Prefix(AzString::from_const_str("en"));
        let huge = format!("en-{}", "a".repeat(1_000_000));
        assert!(cond.matches(&huge));
        let cond_exact = LanguageCondition::Exact(AzString::from_const_str("en"));
        assert!(!cond_exact.matches(&huge));
    }

    // RED (genuine bug, PANIC): `LanguageCondition::Prefix` slices the language tag with
    // `&language[..prefix_str.len()]` — a *byte* index. When the runtime language tag is
    // non-ASCII (or merely multibyte), that index can land inside a UTF-8 code point and
    // `str` indexing panics. `@lang("de")` + a system locale reported as e.g. "日本語"
    // aborts style resolution.
    #[test]
    fn language_prefix_does_not_panic_on_multibyte_language_tag() {
        let cond = LanguageCondition::Prefix(AzString::from_const_str("de"));
        // 2-byte prefix index falls inside the 3-byte '日'.
        assert!(!cond.matches("日本語"));
    }

    // RED (same root cause, 1-byte prefix into a 2-byte char).
    #[test]
    fn language_prefix_does_not_panic_on_two_byte_language_tag() {
        let cond = LanguageCondition::Prefix(AzString::from_const_str("d"));
        assert!(!cond.matches("é"));
    }

    // ---------------------------------------------------------------
    // 26-30. DynamicSelectorContext
    // ---------------------------------------------------------------

    #[test]
    fn context_from_system_style_default_is_coherent() {
        let style = crate::system::SystemStyle::default();
        let ctx = DynamicSelectorContext::from_system_style(&style);
        // Platform::Unknown -> OsCondition::Any, and no desktop env.
        assert_eq!(ctx.os, OsCondition::Any);
        assert_eq!(ctx.desktop_env, OptionLinuxDesktopEnv::None);
        assert_eq!(ctx.de_version, 0);
        assert_eq!(ctx.theme, ThemeCondition::Light);
        assert_eq!(ctx.media_type, MediaType::Screen);
        assert_eq!(ctx.viewport_width, DEFAULT_VIEWPORT_WIDTH);
        assert_eq!(ctx.viewport_height, DEFAULT_VIEWPORT_HEIGHT);
        // "no container" is encoded as NaN, and must therefore never match a
        // @container query.
        assert!(ctx.container_width.is_nan());
        assert!(ctx.container_height.is_nan());
        assert!(ctx.window_focused);
    }

    #[test]
    fn context_from_system_style_linux_carries_the_desktop_env() {
        use crate::system::{DesktopEnvironment, Platform};
        let mut style = crate::system::SystemStyle::default();
        style.platform = Platform::Linux(DesktopEnvironment::Kde);
        let ctx = DynamicSelectorContext::from_system_style(&style);
        assert_eq!(ctx.os, OsCondition::Linux);
        assert_eq!(
            ctx.desktop_env,
            OptionLinuxDesktopEnv::Some(LinuxDesktopEnv::KDE)
        );
    }

    #[test]
    fn with_viewport_updates_orientation_and_is_deterministic_at_extremes() {
        let base = DynamicSelectorContext::default();
        assert_eq!(
            base.with_viewport(1920.0, 1080.0).orientation,
            OrientationType::Landscape
        );
        assert_eq!(
            base.with_viewport(1080.0, 1920.0).orientation,
            OrientationType::Portrait
        );
        // Square is *not* landscape (strict `>`), by construction.
        assert_eq!(
            base.with_viewport(500.0, 500.0).orientation,
            OrientationType::Portrait
        );
        // NaN comparisons are all false -> Portrait. Deterministic, no panic.
        assert_eq!(
            base.with_viewport(f32::NAN, f32::NAN).orientation,
            OrientationType::Portrait
        );
        assert_eq!(
            base.with_viewport(f32::INFINITY, f32::NEG_INFINITY).orientation,
            OrientationType::Landscape
        );
        // Zero / negative sizes must not panic.
        let z = base.with_viewport(0.0, 0.0);
        assert_eq!(z.viewport_width, 0.0);
        assert_eq!(z.orientation, OrientationType::Portrait);
        let neg = base.with_viewport(-100.0, -200.0);
        assert_eq!(neg.orientation, OrientationType::Landscape);
    }

    #[test]
    fn with_viewport_does_not_disturb_unrelated_fields() {
        let base = DynamicSelectorContext::default();
        let updated = base.with_viewport(1.0, 2.0);
        assert_eq!(updated.os, base.os);
        assert_eq!(updated.theme, base.theme);
        assert_eq!(updated.language, base.language);
        assert_eq!(updated.pseudo_state, base.pseudo_state);
    }

    #[test]
    fn with_container_stores_name_and_extreme_dimensions() {
        let base = DynamicSelectorContext::default();
        let named = base.with_container(
            f32::MAX,
            f32::NEG_INFINITY,
            Some(AzString::from_const_str("sidebar")),
        );
        assert_eq!(named.container_width, f32::MAX);
        assert_eq!(named.container_height, f32::NEG_INFINITY);
        assert_eq!(
            named.container_name.as_ref(),
            Some(&AzString::from_const_str("sidebar"))
        );

        let unnamed = base.with_container(0.0, 0.0, None);
        assert_eq!(unnamed.container_name.as_ref(), None);
    }

    #[test]
    fn with_container_nan_dimensions_stay_unmatched() {
        let ctx = DynamicSelectorContext::default().with_container(f32::NAN, f32::NAN, None);
        // NaN container size means "no container": the guard in `matches` must reject
        // even a fully-open range.
        let open = DynamicSelector::ContainerWidth(MinMaxRange::new(None, None));
        assert!(!open.matches(&ctx));
        let open_h = DynamicSelector::ContainerHeight(MinMaxRange::new(None, None));
        assert!(!open_h.matches(&ctx));
    }

    #[test]
    fn with_pseudo_state_replaces_the_whole_flag_set() {
        let base = DynamicSelectorContext::default();
        let hovered = base.with_pseudo_state(PseudoStateFlags {
            hover: true,
            ..PseudoStateFlags::default()
        });
        assert!(hovered.pseudo_state.hover);
        assert!(!hovered.pseudo_state.active);
        // Replacing again must not OR the previous state in.
        let active = hovered.with_pseudo_state(PseudoStateFlags {
            active: true,
            ..PseudoStateFlags::default()
        });
        assert!(!active.pseudo_state.hover);
        assert!(active.pseudo_state.active);
    }

    #[test]
    fn viewport_breakpoint_changed_detects_crossings_only() {
        let bps = [480.0_f32, 768.0, 1024.0];
        let base = DynamicSelectorContext::default();
        let small = base.with_viewport(320.0, 480.0);
        let medium = base.with_viewport(800.0, 600.0);
        let also_medium = base.with_viewport(900.0, 600.0);

        assert!(small.viewport_breakpoint_changed(&medium, &bps));
        assert!(medium.viewport_breakpoint_changed(&small, &bps));
        assert!(!medium.viewport_breakpoint_changed(&also_medium, &bps));
        assert!(!small.viewport_breakpoint_changed(&small, &bps));
    }

    #[test]
    fn viewport_breakpoint_changed_is_exactly_on_the_boundary() {
        let bps = [800.0_f32];
        let base = DynamicSelectorContext::default();
        // `>=` bound: 800 is "above", 799.99 is not.
        let at = base.with_viewport(800.0, 600.0);
        let just_below = base.with_viewport(799.99, 600.0);
        assert!(at.viewport_breakpoint_changed(&just_below, &bps));
        assert!(!at.viewport_breakpoint_changed(&at, &bps));
    }

    #[test]
    fn viewport_breakpoint_changed_handles_empty_and_degenerate_breakpoints() {
        let base = DynamicSelectorContext::default();
        let a = base.with_viewport(100.0, 100.0);
        let b = base.with_viewport(5000.0, 100.0);
        assert!(!a.viewport_breakpoint_changed(&b, &[]));
        // NaN breakpoints: `>=` is false on both sides -> no crossing, no panic.
        assert!(!a.viewport_breakpoint_changed(&b, &[f32::NAN]));
        // Infinite breakpoints: nothing is >= +inf, everything is >= -inf.
        assert!(!a.viewport_breakpoint_changed(&b, &[f32::INFINITY]));
        assert!(!a.viewport_breakpoint_changed(&b, &[f32::NEG_INFINITY]));
        // A NaN viewport is never "above" any breakpoint.
        let nan_vp = base.with_viewport(f32::NAN, 100.0);
        assert!(nan_vp.viewport_breakpoint_changed(&b, &[800.0]));
    }

    #[test]
    fn viewport_breakpoint_changed_with_many_breakpoints_terminates() {
        let bps: Vec<f32> = (0..100_000).map(|i| i as f32).collect();
        let base = DynamicSelectorContext::default();
        let a = base.with_viewport(0.0, 100.0);
        let b = base.with_viewport(99_999.0, 100.0);
        assert!(a.viewport_breakpoint_changed(&b, &bps));
    }

    // ---------------------------------------------------------------
    // 31-35. DynamicSelector matching
    // ---------------------------------------------------------------

    #[test]
    fn match_os_any_matches_everything() {
        for actual in [
            OsCondition::Any,
            OsCondition::Apple,
            OsCondition::MacOS,
            OsCondition::IOS,
            OsCondition::Linux,
            OsCondition::Windows,
            OsCondition::Android,
            OsCondition::Web,
        ] {
            assert!(DynamicSelector::match_os(OsCondition::Any, actual));
        }
    }

    #[test]
    fn match_os_apple_is_the_macos_ios_union() {
        assert!(DynamicSelector::match_os(
            OsCondition::Apple,
            OsCondition::MacOS
        ));
        assert!(DynamicSelector::match_os(
            OsCondition::Apple,
            OsCondition::IOS
        ));
        assert!(!DynamicSelector::match_os(
            OsCondition::Apple,
            OsCondition::Linux
        ));
        // Note the asymmetry: `Apple` as the *actual* OS does not satisfy `MacOS`.
        assert!(!DynamicSelector::match_os(
            OsCondition::MacOS,
            OsCondition::Apple
        ));
    }

    #[test]
    fn match_os_concrete_conditions_require_equality() {
        assert!(DynamicSelector::match_os(
            OsCondition::Linux,
            OsCondition::Linux
        ));
        assert!(!DynamicSelector::match_os(
            OsCondition::Linux,
            OsCondition::Windows
        ));
        // `Any` as the *actual* OS (i.e. unknown platform) must not satisfy a concrete rule.
        assert!(!DynamicSelector::match_os(
            OsCondition::Windows,
            OsCondition::Any
        ));
    }

    #[test]
    fn match_os_version_min_max_exact_at_zero_and_u32_max() {
        let zero = OsVersion::new(OsFamily::Linux, 0);
        let max = OsVersion::new(OsFamily::Linux, u32::MAX);
        let none = OptionLinuxDesktopEnv::None;

        assert!(DynamicSelector::match_os_version(
            &OsVersionCondition::Min(zero),
            max,
            none,
            0
        ));
        assert!(!DynamicSelector::match_os_version(
            &OsVersionCondition::Min(max),
            zero,
            none,
            0
        ));
        assert!(DynamicSelector::match_os_version(
            &OsVersionCondition::Max(max),
            zero,
            none,
            0
        ));
        assert!(DynamicSelector::match_os_version(
            &OsVersionCondition::Exact(max),
            max,
            none,
            0
        ));
        assert!(!DynamicSelector::match_os_version(
            &OsVersionCondition::Exact(zero),
            max,
            none,
            0
        ));
    }

    #[test]
    fn match_os_version_cross_family_never_matches() {
        let none = OptionLinuxDesktopEnv::None;
        // A Windows rule evaluated against a macOS runtime must be false for all three ops.
        for cond in [
            OsVersionCondition::Min(OsVersion::WIN_10),
            OsVersionCondition::Max(OsVersion::WIN_10),
            OsVersionCondition::Exact(OsVersion::WIN_10),
        ] {
            assert!(
                !DynamicSelector::match_os_version(
                    &cond,
                    OsVersion::MACOS_SONOMA,
                    none,
                    0
                ),
                "{cond:?} must not match a macOS runtime"
            );
        }
    }

    #[test]
    fn match_os_version_desktop_environment_requires_the_env_to_be_present() {
        let cond = OsVersionCondition::DesktopEnvironment(LinuxDesktopEnv::Gnome);
        assert!(DynamicSelector::match_os_version(
            &cond,
            OsVersion::unknown(),
            OptionLinuxDesktopEnv::Some(LinuxDesktopEnv::Gnome),
            0
        ));
        assert!(!DynamicSelector::match_os_version(
            &cond,
            OsVersion::unknown(),
            OptionLinuxDesktopEnv::Some(LinuxDesktopEnv::KDE),
            0
        ));
        assert!(!DynamicSelector::match_os_version(
            &cond,
            OsVersion::unknown(),
            OptionLinuxDesktopEnv::None,
            0
        ));
    }

    #[test]
    fn match_os_version_desktop_env_min_max_respect_the_unknown_zero_sentinel() {
        let gnome = OptionLinuxDesktopEnv::Some(LinuxDesktopEnv::Gnome);
        let dev = DesktopEnvVersion {
            env: LinuxDesktopEnv::Gnome,
            version_id: 40,
        };
        // de_version == 0 means "not detected": Min/Max constraints must fail.
        assert!(!DynamicSelector::match_os_version(
            &OsVersionCondition::DesktopEnvMin(dev),
            OsVersion::unknown(),
            gnome,
            0
        ));
        assert!(!DynamicSelector::match_os_version(
            &OsVersionCondition::DesktopEnvMax(dev),
            OsVersion::unknown(),
            gnome,
            0
        ));
        // With a real version, the bounds are inclusive.
        assert!(DynamicSelector::match_os_version(
            &OsVersionCondition::DesktopEnvMin(dev),
            OsVersion::unknown(),
            gnome,
            40
        ));
        assert!(DynamicSelector::match_os_version(
            &OsVersionCondition::DesktopEnvMin(dev),
            OsVersion::unknown(),
            gnome,
            u32::MAX
        ));
        assert!(!DynamicSelector::match_os_version(
            &OsVersionCondition::DesktopEnvMin(dev),
            OsVersion::unknown(),
            gnome,
            39
        ));
        assert!(DynamicSelector::match_os_version(
            &OsVersionCondition::DesktopEnvMax(dev),
            OsVersion::unknown(),
            gnome,
            40
        ));
        assert!(!DynamicSelector::match_os_version(
            &OsVersionCondition::DesktopEnvMax(dev),
            OsVersion::unknown(),
            gnome,
            41
        ));
    }

    #[test]
    fn match_os_version_desktop_env_exact_needs_the_matching_env() {
        let dev = DesktopEnvVersion {
            env: LinuxDesktopEnv::Gnome,
            version_id: 45,
        };
        assert!(DynamicSelector::match_os_version(
            &OsVersionCondition::DesktopEnvExact(dev),
            OsVersion::unknown(),
            OptionLinuxDesktopEnv::Some(LinuxDesktopEnv::Gnome),
            45
        ));
        assert!(!DynamicSelector::match_os_version(
            &OsVersionCondition::DesktopEnvExact(dev),
            OsVersion::unknown(),
            OptionLinuxDesktopEnv::Some(LinuxDesktopEnv::KDE),
            45
        ));
        assert!(!DynamicSelector::match_os_version(
            &OsVersionCondition::DesktopEnvExact(dev),
            OsVersion::unknown(),
            OptionLinuxDesktopEnv::Some(LinuxDesktopEnv::Gnome),
            46
        ));
    }

    // RED (genuine bug, low severity): the invariant documented directly above
    // `match_os_version` is "de_version == 0 means the runtime hasn't reported a version,
    // so any DE-version constraint fails". `DesktopEnvMin`/`DesktopEnvMax` guard on
    // `de_version != 0`, but `DesktopEnvExact` does not — so `@os(linux:gnome = 0)`
    // matches every GNOME session while DE-version detection is still unwired.
    #[test]
    fn match_os_version_desktop_env_exact_zero_fails_when_de_version_is_unknown() {
        let dev = DesktopEnvVersion {
            env: LinuxDesktopEnv::Gnome,
            version_id: 0,
        };
        assert!(
            !DynamicSelector::match_os_version(
                &OsVersionCondition::DesktopEnvExact(dev),
                OsVersion::unknown(),
                OptionLinuxDesktopEnv::Some(LinuxDesktopEnv::Gnome),
                0
            ),
            "de_version == 0 is the 'unknown' sentinel; it must not satisfy an exact match"
        );
    }

    #[test]
    fn match_theme_system_preferred_is_a_wildcard() {
        for actual in [
            ThemeCondition::Light,
            ThemeCondition::Dark,
            ThemeCondition::SystemPreferred,
            ThemeCondition::Custom(AzString::from_const_str("solarized")),
        ] {
            assert!(DynamicSelector::match_theme(
                &ThemeCondition::SystemPreferred,
                &actual
            ));
        }
    }

    #[test]
    fn match_theme_custom_compares_the_name() {
        let a = ThemeCondition::Custom(AzString::from_const_str("nord"));
        let b = ThemeCondition::Custom(AzString::from_const_str("nord"));
        let c = ThemeCondition::Custom(AzString::from_const_str("Nord"));
        assert!(DynamicSelector::match_theme(&a, &b));
        // Theme names are compared case-sensitively.
        assert!(!DynamicSelector::match_theme(&a, &c));
        assert!(!DynamicSelector::match_theme(&a, &ThemeCondition::Dark));
        // `SystemPreferred` as the *actual* theme does not satisfy a concrete rule.
        assert!(!DynamicSelector::match_theme(
            &ThemeCondition::Dark,
            &ThemeCondition::SystemPreferred
        ));
    }

    #[test]
    fn match_pseudo_state_reads_through_to_the_context_flags() {
        let ctx = DynamicSelectorContext::default().with_pseudo_state(PseudoStateFlags {
            hover: true,
            checked: true,
            ..PseudoStateFlags::default()
        });
        assert!(DynamicSelector::match_pseudo_state(
            PseudoStateType::Hover,
            &ctx
        ));
        assert!(DynamicSelector::match_pseudo_state(
            PseudoStateType::CheckedTrue,
            &ctx
        ));
        assert!(!DynamicSelector::match_pseudo_state(
            PseudoStateType::CheckedFalse,
            &ctx
        ));
        assert!(!DynamicSelector::match_pseudo_state(
            PseudoStateType::Active,
            &ctx
        ));
        assert!(DynamicSelector::match_pseudo_state(
            PseudoStateType::Normal,
            &ctx
        ));
    }

    #[test]
    fn match_pseudo_state_agrees_with_has_state_for_every_state() {
        let flags = PseudoStateFlags {
            hover: true,
            focused: true,
            visited: true,
            drag_over: true,
            ..PseudoStateFlags::default()
        };
        let ctx = DynamicSelectorContext::default().with_pseudo_state(flags);
        for state in [
            PseudoStateType::Normal,
            PseudoStateType::Hover,
            PseudoStateType::Active,
            PseudoStateType::Focus,
            PseudoStateType::Disabled,
            PseudoStateType::CheckedTrue,
            PseudoStateType::CheckedFalse,
            PseudoStateType::FocusWithin,
            PseudoStateType::Visited,
            PseudoStateType::Backdrop,
            PseudoStateType::Dragging,
            PseudoStateType::DragOver,
        ] {
            assert_eq!(
                DynamicSelector::match_pseudo_state(state, &ctx),
                flags.has_state(state),
                "match_pseudo_state and has_state disagree on {state:?}"
            );
        }
    }

    #[test]
    fn selector_matches_every_variant_against_the_default_context() {
        // Smoke: no variant may panic on the default context.
        let ctx = DynamicSelectorContext::default();
        for sel in all_selector_variants() {
            let a = sel.matches(&ctx);
            let b = sel.matches(&ctx);
            assert_eq!(a, b, "{sel:?} is not deterministic");
        }
    }

    #[test]
    fn selector_matches_media_all_is_a_wildcard() {
        let ctx = DynamicSelectorContext::default();
        assert_eq!(ctx.media_type, MediaType::Screen);
        assert!(DynamicSelector::Media(MediaType::All).matches(&ctx));
        assert!(DynamicSelector::Media(MediaType::Screen).matches(&ctx));
        assert!(!DynamicSelector::Media(MediaType::Print).matches(&ctx));
    }

    #[test]
    fn selector_matches_aspect_ratio_never_divides_by_zero() {
        let base = DynamicSelectorContext::default();
        // height 0 is clamped to 1.0 by `.max(1.0)`, so the ratio stays finite.
        let flat = base.with_viewport(800.0, 0.0);
        assert!(DynamicSelector::AspectRatio(MinMaxRange::new(Some(799.0), Some(801.0)))
            .matches(&flat));
        // NaN height also clamps to 1.0 (f32::max ignores NaN).
        let nan_h = base.with_viewport(800.0, f32::NAN);
        assert!(DynamicSelector::AspectRatio(MinMaxRange::new(Some(799.0), Some(801.0)))
            .matches(&nan_h));
        // A NaN *width* yields a NaN ratio, which any real bound rejects.
        let nan_w = base.with_viewport(f32::NAN, 600.0);
        assert!(!DynamicSelector::AspectRatio(MinMaxRange::with_min(0.0)).matches(&nan_w));
    }

    #[test]
    fn selector_matches_container_name_requires_an_exact_name() {
        let ctx = DynamicSelectorContext::default().with_container(
            100.0,
            100.0,
            Some(AzString::from_const_str("sidebar")),
        );
        assert!(
            DynamicSelector::ContainerName(AzString::from_const_str("sidebar")).matches(&ctx)
        );
        assert!(
            !DynamicSelector::ContainerName(AzString::from_const_str("Sidebar")).matches(&ctx)
        );
        assert!(!DynamicSelector::ContainerName(AzString::from_const_str("main")).matches(&ctx));
        // No container at all.
        let no_ctr = DynamicSelectorContext::default();
        assert!(
            !DynamicSelector::ContainerName(AzString::from_const_str("sidebar")).matches(&no_ctr)
        );
    }

    #[test]
    fn selector_matches_bool_conditions_compare_both_polarities() {
        let ctx = DynamicSelectorContext::default();
        assert_eq!(ctx.prefers_reduced_motion, BoolCondition::False);
        // The impl compares equality, so `False` matches a "no preference" runtime.
        assert!(DynamicSelector::PrefersReducedMotion(BoolCondition::False).matches(&ctx));
        assert!(!DynamicSelector::PrefersReducedMotion(BoolCondition::True).matches(&ctx));
        assert!(DynamicSelector::PrefersHighContrast(BoolCondition::False).matches(&ctx));
        assert!(!DynamicSelector::PrefersHighContrast(BoolCondition::True).matches(&ctx));
    }

    #[test]
    fn bool_condition_roundtrips_through_bool() {
        for b in [false, true] {
            assert_eq!(bool::from(BoolCondition::from(b)), b);
        }
        assert_eq!(BoolCondition::default(), BoolCondition::False);
        assert!(!bool::from(BoolCondition::False));
        assert!(bool::from(BoolCondition::True));
    }

    // ---------------------------------------------------------------
    // 40-57. CssPropertyWithConditions
    // ---------------------------------------------------------------

    #[test]
    fn simple_property_is_unconditional_and_always_matches() {
        let p = CssPropertyWithConditions::simple(paint_prop());
        assert!(p.apply_if.as_slice().is_empty());
        assert!(!p.is_conditional());
        assert!(!p.is_pseudo_state_only());
        assert!(p.matches(&DynamicSelectorContext::default()));
    }

    #[test]
    fn with_condition_and_with_single_condition_agree() {
        let a = CssPropertyWithConditions::with_condition(
            paint_prop(),
            DynamicSelector::PseudoState(PseudoStateType::Hover),
        );
        let b = CssPropertyWithConditions::on_hover(paint_prop());
        assert_eq!(a.apply_if.as_slice(), b.apply_if.as_slice());
        assert_eq!(a.apply_if.as_slice().len(), 1);
    }

    #[test]
    fn with_conditions_preserves_order_and_length() {
        let conds = all_selector_variants();
        let p = CssPropertyWithConditions::with_conditions(
            paint_prop(),
            DynamicSelectorVec::from_vec(conds.clone()),
        );
        assert_eq!(p.apply_if.as_slice().len(), conds.len());
        for (i, c) in conds.iter().enumerate() {
            assert_eq!(p.apply_if.as_slice()[i].variant_tag(), c.variant_tag());
        }
        assert!(p.is_conditional());
    }

    #[test]
    fn with_conditions_empty_vec_is_unconditional() {
        let p = CssPropertyWithConditions::with_conditions(
            paint_prop(),
            DynamicSelectorVec::from_vec(vec![]),
        );
        assert!(!p.is_conditional());
        assert!(p.matches(&DynamicSelectorContext::default()));
    }

    #[test]
    fn pseudo_state_constructors_build_the_right_condition() {
        let cases = [
            (
                CssPropertyWithConditions::on_hover(paint_prop()),
                PseudoStateType::Hover,
            ),
            (
                CssPropertyWithConditions::on_active(paint_prop()),
                PseudoStateType::Active,
            ),
            (
                CssPropertyWithConditions::on_focus(paint_prop()),
                PseudoStateType::Focus,
            ),
            (
                CssPropertyWithConditions::when_disabled(paint_prop()),
                PseudoStateType::Disabled,
            ),
        ];
        for (prop, expected) in cases {
            assert_eq!(
                prop.apply_if.as_slice(),
                &[DynamicSelector::PseudoState(expected)]
            );
            assert!(prop.is_pseudo_state_only());
            assert!(prop.is_conditional());
        }
    }

    #[test]
    fn os_and_theme_constructors_build_the_right_condition() {
        assert_eq!(
            CssPropertyWithConditions::on_windows(paint_prop())
                .apply_if
                .as_slice(),
            &[DynamicSelector::Os(OsCondition::Windows)]
        );
        assert_eq!(
            CssPropertyWithConditions::on_macos(paint_prop())
                .apply_if
                .as_slice(),
            &[DynamicSelector::Os(OsCondition::MacOS)]
        );
        assert_eq!(
            CssPropertyWithConditions::on_linux(paint_prop())
                .apply_if
                .as_slice(),
            &[DynamicSelector::Os(OsCondition::Linux)]
        );
        assert_eq!(
            CssPropertyWithConditions::on_os(paint_prop(), OsCondition::Web)
                .apply_if
                .as_slice(),
            &[DynamicSelector::Os(OsCondition::Web)]
        );
        assert_eq!(
            CssPropertyWithConditions::dark_theme(paint_prop())
                .apply_if
                .as_slice(),
            &[DynamicSelector::Theme(ThemeCondition::Dark)]
        );
        assert_eq!(
            CssPropertyWithConditions::light_theme(paint_prop())
                .apply_if
                .as_slice(),
            &[DynamicSelector::Theme(ThemeCondition::Light)]
        );
        // OS / theme conditions are not pseudo-state conditions.
        assert!(!CssPropertyWithConditions::on_linux(paint_prop()).is_pseudo_state_only());
        assert!(!CssPropertyWithConditions::dark_theme(paint_prop()).is_pseudo_state_only());
    }

    #[test]
    fn matches_requires_all_conditions_to_hold() {
        let ctx = DynamicSelectorContext::default().with_pseudo_state(PseudoStateFlags {
            hover: true,
            ..PseudoStateFlags::default()
        });
        // hover (true) AND focus (false) -> false.
        let both = CssPropertyWithConditions::with_conditions(
            paint_prop(),
            DynamicSelectorVec::from_vec(vec![
                DynamicSelector::PseudoState(PseudoStateType::Hover),
                DynamicSelector::PseudoState(PseudoStateType::Focus),
            ]),
        );
        assert!(!both.matches(&ctx));

        // hover (true) AND normal (always true) -> true.
        let ok = CssPropertyWithConditions::with_conditions(
            paint_prop(),
            DynamicSelectorVec::from_vec(vec![
                DynamicSelector::PseudoState(PseudoStateType::Hover),
                DynamicSelector::PseudoState(PseudoStateType::Normal),
            ]),
        );
        assert!(ok.matches(&ctx));
    }

    #[test]
    fn matches_with_a_large_condition_list_terminates() {
        let conds: Vec<DynamicSelector> = (0..50_000)
            .map(|_| DynamicSelector::PseudoState(PseudoStateType::Normal))
            .collect();
        let p = CssPropertyWithConditions::with_conditions(
            paint_prop(),
            DynamicSelectorVec::from_vec(conds),
        );
        assert!(p.matches(&DynamicSelectorContext::default()));
    }

    #[test]
    fn is_pseudo_state_only_is_false_for_empty_and_for_mixed_lists() {
        // Empty -> false (there is no pseudo-state condition at all).
        assert!(!CssPropertyWithConditions::simple(paint_prop()).is_pseudo_state_only());
        // Mixed -> false.
        let mixed = CssPropertyWithConditions::with_conditions(
            paint_prop(),
            DynamicSelectorVec::from_vec(vec![
                DynamicSelector::PseudoState(PseudoStateType::Hover),
                DynamicSelector::Os(OsCondition::Linux),
            ]),
        );
        assert!(!mixed.is_pseudo_state_only());
        // All pseudo -> true.
        let all_pseudo = CssPropertyWithConditions::with_conditions(
            paint_prop(),
            DynamicSelectorVec::from_vec(vec![
                DynamicSelector::PseudoState(PseudoStateType::Hover),
                DynamicSelector::PseudoState(PseudoStateType::Focus),
            ]),
        );
        assert!(all_pseudo.is_pseudo_state_only());
    }

    #[test]
    fn is_layout_affecting_splits_layout_from_paint() {
        assert!(CssPropertyWithConditions::simple(layout_prop()).is_layout_affecting());
        assert!(!CssPropertyWithConditions::simple(paint_prop()).is_layout_affecting());
        // Conditions must not influence the answer — only the property does.
        assert!(CssPropertyWithConditions::on_hover(layout_prop()).is_layout_affecting());
        assert!(!CssPropertyWithConditions::on_hover(paint_prop()).is_layout_affecting());
    }

    #[test]
    fn css_property_with_conditions_hash_agrees_with_eq() {
        let a = CssPropertyWithConditions::on_hover(paint_prop());
        let b = CssPropertyWithConditions::on_hover(paint_prop());
        assert_eq!(a, b);
        assert_eq!(hash_of(&a), hash_of(&b));
        assert_eq!(a.cmp(&b), Ordering::Equal);

        // Same property, different condition sets must not collide on the hash and
        // must not compare Equal (the old impl keyed on condition *count* only).
        let c = CssPropertyWithConditions::on_focus(paint_prop());
        assert_ne!(a, c);
        assert_ne!(a.cmp(&c), Ordering::Equal);
        assert_ne!(hash_of(&a), hash_of(&c));
    }

    #[test]
    fn css_property_with_conditions_ord_is_lexicographic_over_conditions() {
        let short = CssPropertyWithConditions::on_hover(paint_prop());
        let long = CssPropertyWithConditions::with_conditions(
            paint_prop(),
            DynamicSelectorVec::from_vec(vec![
                DynamicSelector::PseudoState(PseudoStateType::Hover),
                DynamicSelector::PseudoState(PseudoStateType::Focus),
            ]),
        );
        // A prefix sorts before the longer list.
        assert_eq!(short.cmp(&long), Ordering::Less);
        assert_eq!(long.cmp(&short), Ordering::Greater);
    }

    // ---------------------------------------------------------------
    // 36-39. @os at-rule parsing (parser feature)
    // ---------------------------------------------------------------

    #[cfg(feature = "parser")]
    #[test]
    fn parse_os_at_rule_bare_and_parenthesized_forms_agree() {
        let bare = parse_os_at_rule_content("linux").expect("bare linux");
        let paren = parse_os_at_rule_content("(linux)").expect("(linux)");
        let quoted = parse_os_at_rule_content("(\"linux\")").expect("quoted");
        let spaced = parse_os_at_rule_content("   (  linux  )   ").expect("spaced");
        assert_eq!(bare, vec![DynamicSelector::Os(OsCondition::Linux)]);
        assert_eq!(bare, paren);
        assert_eq!(bare, quoted);
        assert_eq!(bare, spaced);
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_os_at_rule_emits_the_family_even_for_any() {
        // `(any)` still emits `Os(Any)` (documented: kept for introspection), and
        // `Os(Any)` matches unconditionally.
        for s in ["(any)", "(all)", "(*)"] {
            let conds = parse_os_at_rule_content(s).unwrap_or_else(|| panic!("{s} must parse"));
            assert_eq!(conds, vec![DynamicSelector::Os(OsCondition::Any)]);
            assert!(conds[0].matches(&DynamicSelectorContext::default()));
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_os_at_rule_desktop_env_forms() {
        assert_eq!(
            parse_os_at_rule_content("(linux:gnome)"),
            Some(vec![
                DynamicSelector::Os(OsCondition::Linux),
                DynamicSelector::OsVersion(OsVersionCondition::DesktopEnvironment(
                    LinuxDesktopEnv::Gnome
                )),
            ])
        );
        // Unknown DE names silently become `Other` (documented `parse_de_token` fallback).
        assert_eq!(
            parse_os_at_rule_content("(linux:notarealde)"),
            Some(vec![
                DynamicSelector::Os(OsCondition::Linux),
                DynamicSelector::OsVersion(OsVersionCondition::DesktopEnvironment(
                    LinuxDesktopEnv::Other
                )),
            ])
        );
        // A trailing ':' with no DE is treated as "no DE".
        assert_eq!(
            parse_os_at_rule_content("(linux:)"),
            Some(vec![DynamicSelector::Os(OsCondition::Linux)])
        );
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_os_at_rule_version_operators() {
        assert_eq!(
            parse_os_at_rule_content("(windows >= win-11)"),
            Some(vec![
                DynamicSelector::Os(OsCondition::Windows),
                DynamicSelector::OsVersion(OsVersionCondition::Min(OsVersion::WIN_11)),
            ])
        );
        // `>` is documented to behave as `>=` (version ids are discrete).
        assert_eq!(
            parse_os_at_rule_content("(windows > win-11)"),
            parse_os_at_rule_content("(windows >= win-11)")
        );
        assert_eq!(
            parse_os_at_rule_content("(macos <= sonoma)"),
            Some(vec![
                DynamicSelector::Os(OsCondition::MacOS),
                DynamicSelector::OsVersion(OsVersionCondition::Max(OsVersion::MACOS_SONOMA)),
            ])
        );
        assert_eq!(
            parse_os_at_rule_content("(ios = 17)"),
            Some(vec![
                DynamicSelector::Os(OsCondition::IOS),
                DynamicSelector::OsVersion(OsVersionCondition::Exact(OsVersion::IOS_17)),
            ])
        );
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_os_at_rule_desktop_env_version() {
        assert_eq!(
            parse_os_at_rule_content("(linux:gnome > 40)"),
            Some(vec![
                DynamicSelector::Os(OsCondition::Linux),
                DynamicSelector::OsVersion(OsVersionCondition::DesktopEnvMin(
                    DesktopEnvVersion {
                        env: LinuxDesktopEnv::Gnome,
                        version_id: 40,
                    }
                )),
            ])
        );
        // DE version must be a plain u32: overflow and junk are rejected, not wrapped.
        assert_eq!(parse_os_at_rule_content("(linux:gnome > 4294967296)"), None);
        assert_eq!(parse_os_at_rule_content("(linux:gnome > -1)"), None);
        assert_eq!(parse_os_at_rule_content("(linux:gnome > abc)"), None);
        assert_eq!(parse_os_at_rule_content("(linux:gnome > )"), None);
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_os_at_rule_rejects_versions_on_versionless_families() {
        // Apple / Web / Any have no version line -> reject rather than guess.
        assert_eq!(parse_os_at_rule_content("(apple >= 14)"), None);
        assert_eq!(parse_os_at_rule_content("(web >= 1)"), None);
        assert_eq!(parse_os_at_rule_content("(any >= 1)"), None);
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_os_at_rule_empty_and_garbage_is_none() {
        for s in [
            "",
            "   ",
            "\t\n",
            "()",
            "(   )",
            "(\"\")",
            "('')",
            "(:)",
            "(:gnome)",
            "(notanos)",
            "(linux;drop)",
            "\u{1F600}",
            "(\u{1F600})",
            "(linux linux)",
        ] {
            assert_eq!(parse_os_at_rule_content(s), None, "{s:?} must not parse");
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_os_at_rule_huge_and_deeply_parenthesized_input_terminates() {
        // Only one layer of parens is stripped; the rest is junk -> None, no hang.
        let nested = format!("{}linux{}", "(".repeat(10_000), ")".repeat(10_000));
        assert_eq!(parse_os_at_rule_content(&nested), None);
        assert_eq!(parse_os_at_rule_content(&"a".repeat(1_000_000)), None);
        assert_eq!(
            parse_os_at_rule_content(&format!("(linux >= {})", "9".repeat(100_000))),
            None
        );
    }

    #[cfg(feature = "parser")]
    #[test]
    fn split_op_and_version_picks_the_earliest_then_longest_operator() {
        let (subject, op) = split_op_and_version("linux >= 6.0");
        assert_eq!(subject, "linux ");
        let (op, ver) = op.expect("operator found");
        assert!(matches!(op, VersionOp::Min));
        assert_eq!(ver, "6.0");

        // ">=" must beat "=" at the same position.
        let (_, op) = split_op_and_version("a>=1");
        assert!(matches!(op.expect("op").0, VersionOp::Min));
        let (_, op) = split_op_and_version("a<=1");
        assert!(matches!(op.expect("op").0, VersionOp::Max));
        let (_, op) = split_op_and_version("a=1");
        assert!(matches!(op.expect("op").0, VersionOp::Exact));
    }

    #[cfg(feature = "parser")]
    #[test]
    fn split_op_and_version_with_no_operator_returns_the_whole_string() {
        let (subject, op) = split_op_and_version("linux");
        assert_eq!(subject, "linux");
        assert!(op.is_none());

        let (subject, op) = split_op_and_version("");
        assert_eq!(subject, "");
        assert!(op.is_none());
    }

    #[cfg(feature = "parser")]
    #[test]
    fn split_op_and_version_operator_only_yields_empty_sides() {
        let (subject, op) = split_op_and_version(">=");
        assert_eq!(subject, "");
        assert_eq!(op.expect("op").1, "");

        let (subject, op) = split_op_and_version("=");
        assert_eq!(subject, "");
        assert_eq!(op.expect("op").1, "");
    }

    #[cfg(feature = "parser")]
    #[test]
    fn split_op_and_version_does_not_split_inside_a_multibyte_char() {
        // Operators are ASCII, so the byte offsets returned by `find` are always char
        // boundaries — but assert it, because slicing here would panic otherwise.
        let (subject, op) = split_op_and_version("日本語 >= 6.0");
        assert_eq!(subject, "日本語 ");
        assert_eq!(op.expect("op").1, "6.0");
        // No operator at all in a multibyte string.
        let (subject, op) = split_op_and_version("🦀🦀🦀");
        assert_eq!(subject, "🦀🦀🦀");
        assert!(op.is_none());
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_os_family_token_accepts_aliases_case_insensitively() {
        assert_eq!(parse_os_family_token("LINUX"), Some(OsCondition::Linux));
        assert_eq!(parse_os_family_token("Win"), Some(OsCondition::Windows));
        assert_eq!(parse_os_family_token("windows"), Some(OsCondition::Windows));
        assert_eq!(parse_os_family_token("osx"), Some(OsCondition::MacOS));
        assert_eq!(parse_os_family_token("mac"), Some(OsCondition::MacOS));
        assert_eq!(parse_os_family_token("wasm"), Some(OsCondition::Web));
        assert_eq!(parse_os_family_token("*"), Some(OsCondition::Any));
        assert_eq!(parse_os_family_token("all"), Some(OsCondition::Any));
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_os_family_token_rejects_empty_padded_and_junk() {
        // The token is *not* trimmed here — the caller trims.
        for s in [
            "",
            " ",
            " linux",
            "linux ",
            "lin",
            "linux2",
            "0",
            "-1",
            "NaN",
            "\u{1F600}",
        ] {
            assert_eq!(parse_os_family_token(s), None, "{s:?} must not parse");
        }
        assert_eq!(parse_os_family_token(&"linux".repeat(100_000)), None);
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_de_token_is_total_and_falls_back_to_other() {
        assert_eq!(parse_de_token("GNOME"), LinuxDesktopEnv::Gnome);
        assert_eq!(parse_de_token("kde"), LinuxDesktopEnv::KDE);
        assert_eq!(parse_de_token("XFCE"), LinuxDesktopEnv::XFCE);
        assert_eq!(parse_de_token("unity"), LinuxDesktopEnv::Unity);
        assert_eq!(parse_de_token("Cinnamon"), LinuxDesktopEnv::Cinnamon);
        assert_eq!(parse_de_token("mate"), LinuxDesktopEnv::MATE);
        // `parse_de_token` returns a value, not an Option: everything else is `Other`.
        for s in ["", "  ", "gnome ", "\u{1F600}", "日本語", "\0"] {
            assert_eq!(parse_de_token(s), LinuxDesktopEnv::Other, "{s:?}");
        }
        assert_eq!(
            parse_de_token(&"gnome".repeat(200_000)),
            LinuxDesktopEnv::Other
        );
    }

    // ---------------------------------------------------------------
    // 58-65. CssPropertyWithConditionsVec parsing (parser feature)
    // ---------------------------------------------------------------

    #[cfg(feature = "parser")]
    fn parse_len(style: &str) -> usize {
        CssPropertyWithConditionsVec::parse(style)
            .into_library_owned_vec()
            .len()
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_valid_minimal_positive_control() {
        let props = CssPropertyWithConditionsVec::parse("color: red;").into_library_owned_vec();
        assert_eq!(props.len(), 1);
        assert!(!props[0].is_conditional());
        assert!(matches!(props[0].property, CssProperty::TextColor(_)));
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_empty_and_whitespace_yields_no_properties() {
        for s in ["", " ", "\t\n\r ", ";", ";;;;", "   ;   ;   "] {
            assert_eq!(parse_len(s), 0, "{s:?} must yield no properties");
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_garbage_never_panics_and_yields_nothing() {
        for s in [
            "not css at all",
            "color",
            "color:",
            ":",
            "::::",
            "{}",
            "}{",
            "}",
            "{",
            "{{{",
            "}}}",
            "}{color: red}",
            "color: ;",
            "\0: \0;",
            "%s%n%s",
            "\u{1F600}: \u{1F600};",
            "日本語: 赤;",
        ] {
            // Must terminate and not panic; the value itself is allowed to be empty.
            let _ = parse_len(s);
        }
        assert_eq!(parse_len("not css at all"), 0);
        assert_eq!(parse_len("color:"), 0);
        assert_eq!(parse_len("}{"), 0);
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_tolerates_a_missing_trailing_semicolon() {
        assert_eq!(parse_len("color: red"), 1);
        assert_eq!(parse_len("color: red;color: blue"), 2);
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_pseudo_selector_block_attaches_the_condition() {
        let props = CssPropertyWithConditionsVec::parse(":hover { color: red; }")
            .into_library_owned_vec();
        assert_eq!(props.len(), 1);
        assert!(props[0].is_pseudo_state_only());
        assert_eq!(
            props[0].apply_if.as_slice(),
            &[DynamicSelector::PseudoState(PseudoStateType::Hover)]
        );
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_unknown_selector_block_is_dropped_wholesale() {
        // An unknown pseudo-class must drop the whole block, not leak its properties
        // as unconditional.
        assert_eq!(parse_len(":nosuchstate { color: red; }"), 0);
        assert_eq!(parse_len("@nosuchrule { color: red; }"), 0);
        assert_eq!(parse_len("div { color: red; }"), 0);
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_nesting_accumulates_inherited_conditions() {
        let props =
            CssPropertyWithConditionsVec::parse("@os linux { font-size: 14px; :hover { color: red; }}")
                .into_library_owned_vec();
        assert_eq!(props.len(), 2);
        // Both properties carry the @os condition; the hover one carries both.
        let font = props
            .iter()
            .find(|p| matches!(p.property, CssProperty::FontSize(_)))
            .expect("font-size present");
        assert_eq!(
            font.apply_if.as_slice(),
            &[DynamicSelector::Os(OsCondition::Linux)]
        );
        let color = props
            .iter()
            .find(|p| matches!(p.property, CssProperty::TextColor(_)))
            .expect("color present");
        assert_eq!(
            color.apply_if.as_slice(),
            &[
                DynamicSelector::Os(OsCondition::Linux),
                DynamicSelector::PseudoState(PseudoStateType::Hover),
            ]
        );
        assert!(!color.is_pseudo_state_only());
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_moderately_deep_nesting_terminates() {
        // NOTE: `parse_block_segment` recurses once per nesting level with no depth cap,
        // so a pathologically nested stylesheet (~10k levels) would abort the process on
        // a stack overflow. Kept at a depth that is safe to run in-process; the missing
        // depth limit is reported separately.
        const DEPTH: usize = 50;
        let style = format!(
            "{}color: red;{}",
            ":hover {".repeat(DEPTH),
            "}".repeat(DEPTH)
        );
        let props = CssPropertyWithConditionsVec::parse(&style).into_library_owned_vec();
        assert_eq!(props.len(), 1);
        assert_eq!(props[0].apply_if.as_slice().len(), DEPTH);
        assert!(props[0].is_pseudo_state_only());
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_unbalanced_braces_do_not_panic() {
        // brace_depth goes negative / never returns to zero; both paths must be inert.
        for s in [
            "color: red; }",
            "{ color: red;",
            ":hover { color: red;",
            ":hover }",
            &"{".repeat(1_000),
            &"}".repeat(1_000),
        ] {
            let _ = parse_len(s);
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_very_long_input_terminates() {
        let long = "color: red;".repeat(5_000);
        assert_eq!(parse_len(&long), 5_000);
        // A single enormous junk token must not blow up either.
        assert_eq!(parse_len(&"a".repeat(500_000)), 0);
        assert_eq!(parse_len(&format!("color: {};", "z".repeat(500_000))), 0);
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_selector_to_conditions_covers_every_pseudo_class() {
        let cases = [
            ("hover", PseudoStateType::Hover),
            ("active", PseudoStateType::Active),
            ("focus", PseudoStateType::Focus),
            ("focus-within", PseudoStateType::FocusWithin),
            ("disabled", PseudoStateType::Disabled),
            ("checked", PseudoStateType::CheckedTrue),
            ("visited", PseudoStateType::Visited),
            ("backdrop", PseudoStateType::Backdrop),
            ("dragging", PseudoStateType::Dragging),
            ("drag-over", PseudoStateType::DragOver),
        ];
        for (name, expected) in cases {
            assert_eq!(
                CssPropertyWithConditionsVec::parse_selector_to_conditions(&format!(":{name}")),
                Some(vec![DynamicSelector::PseudoState(expected)]),
                ":{name} must parse"
            );
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_selector_to_conditions_wildcards_are_unconditional() {
        assert_eq!(
            CssPropertyWithConditionsVec::parse_selector_to_conditions("*"),
            Some(vec![])
        );
        assert_eq!(
            CssPropertyWithConditionsVec::parse_selector_to_conditions(""),
            Some(vec![])
        );
        assert_eq!(
            CssPropertyWithConditionsVec::parse_selector_to_conditions("   "),
            Some(vec![])
        );
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_selector_to_conditions_rejects_unknown_selectors() {
        for s in [
            ":", ":hoverr", ":HOVER", "div", "#id", ".class", "@", "\u{1F600}", ":\u{1F600}",
        ] {
            assert_eq!(
                CssPropertyWithConditionsVec::parse_selector_to_conditions(s),
                None,
                "{s:?} must not parse"
            );
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_at_rule_theme_lang_and_accessibility() {
        assert_eq!(
            CssPropertyWithConditionsVec::parse_at_rule("theme dark"),
            Some(vec![DynamicSelector::Theme(ThemeCondition::Dark)])
        );
        assert_eq!(
            CssPropertyWithConditionsVec::parse_at_rule("theme light"),
            Some(vec![DynamicSelector::Theme(ThemeCondition::Light)])
        );
        assert_eq!(CssPropertyWithConditionsVec::parse_at_rule("theme neon"), None);

        assert_eq!(
            CssPropertyWithConditionsVec::parse_at_rule("lang(\"de-DE\")"),
            Some(vec![DynamicSelector::Language(LanguageCondition::Prefix(
                AzString::from_const_str("de-DE")
            ))])
        );
        assert_eq!(
            CssPropertyWithConditionsVec::parse_at_rule("lang de"),
            Some(vec![DynamicSelector::Language(LanguageCondition::Prefix(
                AzString::from_const_str("de")
            ))])
        );
        assert_eq!(CssPropertyWithConditionsVec::parse_at_rule("lang()"), None);
        assert_eq!(CssPropertyWithConditionsVec::parse_at_rule("lang(\"\")"), None);

        assert_eq!(
            CssPropertyWithConditionsVec::parse_at_rule("prefers-reduced-motion"),
            Some(vec![DynamicSelector::PrefersReducedMotion(
                BoolCondition::True
            )])
        );
        assert_eq!(
            CssPropertyWithConditionsVec::parse_at_rule("high-contrast"),
            Some(vec![DynamicSelector::PrefersHighContrast(
                BoolCondition::True
            )])
        );
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_at_rule_container_named_and_sized() {
        assert_eq!(
            CssPropertyWithConditionsVec::parse_at_rule("container sidebar"),
            Some(vec![DynamicSelector::ContainerName(
                AzString::from_const_str("sidebar")
            )])
        );
        let conds = CssPropertyWithConditionsVec::parse_at_rule("container (min-width: 400px)")
            .expect("sized container must parse");
        assert_eq!(conds.len(), 1);
        // Destructured rather than compared with `==`: `MinMaxRange`'s derived PartialEq
        // is not reflexive while its `max` is the NaN sentinel (see
        // `nan_sentinel_range_selector_is_reflexive_under_partial_eq`).
        match conds[0] {
            DynamicSelector::ContainerWidth(r) => {
                assert_eq!(r.min(), Some(400.0));
                assert_eq!(r.max(), None);
            }
            ref other => panic!("expected ContainerWidth, got {other:?}"),
        }
        let named = CssPropertyWithConditionsVec::parse_at_rule(
            "container sidebar (max-height: 200px)",
        )
        .expect("named + sized container must parse");
        assert_eq!(named.len(), 2);
        assert!(matches!(named[0], DynamicSelector::ContainerName(_)));
        assert!(matches!(named[1], DynamicSelector::ContainerHeight(_)));
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_at_rule_empty_and_garbage_is_none() {
        for s in [
            "",
            " ",
            "os",
            "os ",
            "os()",
            "os(notanos)",
            "media",
            "media ",
            "media (min-width: abc)",
            "theme",
            "container",
            "container ()",
            "nosuchrule",
            "\u{1F600}",
        ] {
            assert_eq!(
                CssPropertyWithConditionsVec::parse_at_rule(s),
                None,
                "{s:?} must not parse"
            );
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_at_rule_huge_input_terminates() {
        assert_eq!(
            CssPropertyWithConditionsVec::parse_at_rule(&format!("os {}", "(".repeat(50_000))),
            None
        );
        assert_eq!(
            CssPropertyWithConditionsVec::parse_at_rule(&"z".repeat(500_000)),
            None
        );
        // A million-char language tag is accepted verbatim (no hang, no truncation).
        let long_lang = "e".repeat(100_000);
        assert_eq!(
            CssPropertyWithConditionsVec::parse_at_rule(&format!("lang {long_lang}")),
            Some(vec![DynamicSelector::Language(LanguageCondition::Prefix(
                AzString::from(long_lang)
            ))])
        );
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_media_query_media_types_and_dimensions() {
        assert_eq!(
            CssPropertyWithConditionsVec::parse_media_query("screen"),
            Some(vec![DynamicSelector::Media(MediaType::Screen)])
        );
        assert_eq!(
            CssPropertyWithConditionsVec::parse_media_query("print"),
            Some(vec![DynamicSelector::Media(MediaType::Print)])
        );
        assert_eq!(
            CssPropertyWithConditionsVec::parse_media_query("all"),
            Some(vec![DynamicSelector::Media(MediaType::All)])
        );
        let w = CssPropertyWithConditionsVec::parse_media_query("(min-width: 800px)")
            .expect("min-width must parse");
        assert_eq!(w.len(), 1);
        match w[0] {
            DynamicSelector::ViewportWidth(r) => {
                assert_eq!(r.min(), Some(800.0));
                assert_eq!(r.max(), None);
            }
            ref other => panic!("expected ViewportWidth, got {other:?}"),
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_media_query_boundary_pixel_values() {
        // Zero and negative pixel values are accepted verbatim by `f32::parse`.
        let zero = CssPropertyWithConditionsVec::parse_media_query("(min-width: 0px)")
            .expect("0px must parse");
        match zero[0] {
            DynamicSelector::ViewportWidth(r) => assert_eq!(r.min(), Some(0.0)),
            ref other => panic!("expected ViewportWidth, got {other:?}"),
        }
        let neg = CssPropertyWithConditionsVec::parse_media_query("(max-height: -1px)")
            .expect("-1px must parse");
        match neg[0] {
            DynamicSelector::ViewportHeight(r) => assert_eq!(r.max(), Some(-1.0)),
            ref other => panic!("expected ViewportHeight, got {other:?}"),
        }
        // Missing / wrong unit is rejected (falls through to the media-type match).
        assert_eq!(
            CssPropertyWithConditionsVec::parse_media_query("(min-width: 800)"),
            None
        );
        assert_eq!(
            CssPropertyWithConditionsVec::parse_media_query("(min-width: 800em)"),
            None
        );
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_media_query_empty_and_garbage_is_none() {
        for s in [
            "",
            " ",
            "(",
            ")",
            "()",
            "(:)",
            "(min-width)",
            "(min-width: )",
            "(nosuchfeature: 1px)",
            "SCREEN",
            "\u{1F600}",
            "(🦀: 1px)",
        ] {
            assert_eq!(
                CssPropertyWithConditionsVec::parse_media_query(s),
                None,
                "{s:?} must not parse"
            );
        }
        assert_eq!(
            CssPropertyWithConditionsVec::parse_media_query(&"(".repeat(100_000)),
            None
        );
    }

    // RED (genuine bug, low severity): `value.parse::<f32>()` accepts "NaN"/"nan", and
    // `MinMaxRange` uses NaN as the "no limit" sentinel. So `(min-width: NaNpx)` — an
    // invalid media feature — silently becomes an *unconditional* viewport-width match
    // instead of being rejected. Per CSS, an unparseable feature value makes the query
    // invalid (never matches); it must certainly not make it always match.
    #[cfg(feature = "parser")]
    #[test]
    fn parse_media_query_nan_pixel_value_is_rejected() {
        assert_eq!(
            CssPropertyWithConditionsVec::parse_media_query("(min-width: NaNpx)"),
            None,
            "a NaN px value collapses into the 'no limit' sentinel and matches everything"
        );
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_media_query_infinite_pixel_value_never_matches() {
        // "inf" also parses as f32 — unlike NaN it degrades safely (matches nothing),
        // so assert that rather than a rejection.
        let q = CssPropertyWithConditionsVec::parse_media_query("(min-width: infpx)")
            .expect("infpx currently parses");
        let ctx = DynamicSelectorContext::default().with_viewport(1e30, 1000.0);
        assert!(!q[0].matches(&ctx), "an infinite min-width can never be met");
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_media_feature_inline_known_features() {
        assert_eq!(
            CssPropertyWithConditionsVec::parse_media_feature_inline("orientation", "PORTRAIT"),
            Some(DynamicSelector::Orientation(OrientationType::Portrait))
        );
        assert_eq!(
            CssPropertyWithConditionsVec::parse_media_feature_inline(
                "prefers-color-scheme",
                "Dark"
            ),
            Some(DynamicSelector::Theme(ThemeCondition::Dark))
        );
        assert_eq!(
            CssPropertyWithConditionsVec::parse_media_feature_inline(
                "prefers-reduced-motion",
                "reduce"
            ),
            Some(DynamicSelector::PrefersReducedMotion(BoolCondition::True))
        );
        assert_eq!(
            CssPropertyWithConditionsVec::parse_media_feature_inline(
                "prefers-reduced-motion",
                "no-preference"
            ),
            Some(DynamicSelector::PrefersReducedMotion(BoolCondition::False))
        );
        assert_eq!(
            CssPropertyWithConditionsVec::parse_media_feature_inline("prefers-contrast", "more"),
            Some(DynamicSelector::PrefersHighContrast(BoolCondition::True))
        );
        assert_eq!(
            CssPropertyWithConditionsVec::parse_media_feature_inline(
                "prefers-high-contrast",
                "none"
            ),
            Some(DynamicSelector::PrefersHighContrast(BoolCondition::False))
        );
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_media_feature_inline_rejects_unknown_keys_and_values() {
        assert_eq!(
            CssPropertyWithConditionsVec::parse_media_feature_inline("orientation", ""),
            None
        );
        assert_eq!(
            CssPropertyWithConditionsVec::parse_media_feature_inline("orientation", "sideways"),
            None
        );
        assert_eq!(
            CssPropertyWithConditionsVec::parse_media_feature_inline("", ""),
            None
        );
        assert_eq!(
            CssPropertyWithConditionsVec::parse_media_feature_inline("nosuchkey", "dark"),
            None
        );
        assert_eq!(
            CssPropertyWithConditionsVec::parse_media_feature_inline(
                "prefers-color-scheme",
                "\u{1F600}"
            ),
            None
        );
        // The key is *not* trimmed by this helper.
        assert_eq!(
            CssPropertyWithConditionsVec::parse_media_feature_inline(" orientation", "portrait"),
            None
        );
        assert_eq!(
            CssPropertyWithConditionsVec::parse_media_feature_inline(
                &"k".repeat(200_000),
                &"v".repeat(200_000)
            ),
            None
        );
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_property_segment_valid_and_invalid() {
        let key_map = crate::props::property::CssKeyMap::get();
        let ok = CssPropertyWithConditionsVec::parse_property_segment("color: red", &[], &key_map)
            .expect("color: red must parse");
        assert_eq!(ok.len(), 1);
        assert!(!ok[0].is_conditional());

        // A shorthand expands into several properties, all sharing the conditions.
        let inherited = vec![DynamicSelector::PseudoState(PseudoStateType::Hover)];
        let shorthand = CssPropertyWithConditionsVec::parse_property_segment(
            "padding: 10px",
            &inherited,
            &key_map,
        )
        .expect("padding shorthand must parse");
        assert!(shorthand.len() > 1, "padding must expand to >1 property");
        for p in &shorthand {
            assert_eq!(p.apply_if.as_slice(), inherited.as_slice());
        }

        for s in ["", "   ", "color", "color:", ": red", "nosuchprop: red", "\u{1F600}"] {
            assert!(
                CssPropertyWithConditionsVec::parse_property_segment(s, &[], &key_map).is_none(),
                "{s:?} must not parse"
            );
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_block_segment_requires_balanced_braces() {
        let key_map = crate::props::property::CssKeyMap::get();
        // No brace at all.
        assert!(CssPropertyWithConditionsVec::parse_block_segment(
            "color: red",
            &[],
            &key_map
        )
        .is_none());
        // Opening brace, no closing brace.
        assert!(CssPropertyWithConditionsVec::parse_block_segment(
            ":hover { color: red",
            &[],
            &key_map
        )
        .is_none());
        // `}` before `{` -> content_end <= content_start -> None.
        assert!(
            CssPropertyWithConditionsVec::parse_block_segment("}{", &[], &key_map).is_none()
        );
        // Empty body is an empty (but valid) block.
        let empty = CssPropertyWithConditionsVec::parse_block_segment(
            ":hover {}",
            &[],
            &key_map,
        );
        // "{}" has content_end == content_start -> rejected by the guard.
        assert!(empty.is_none());
        // Valid block.
        let ok = CssPropertyWithConditionsVec::parse_block_segment(
            ":hover { color: red; }",
            &[],
            &key_map,
        )
        .expect("valid block");
        assert_eq!(ok.len(), 1);
        assert!(ok[0].is_pseudo_state_only());
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_with_conditions_prepends_inherited_conditions() {
        let inherited = vec![DynamicSelector::Os(OsCondition::Linux)];
        let props =
            CssPropertyWithConditionsVec::parse_with_conditions("color: red;", &inherited)
                .into_library_owned_vec();
        assert_eq!(props.len(), 1);
        assert_eq!(props[0].apply_if.as_slice(), inherited.as_slice());

        // Empty input with inherited conditions still yields nothing.
        let none = CssPropertyWithConditionsVec::parse_with_conditions("   ", &inherited)
            .into_library_owned_vec();
        assert!(none.is_empty());
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parsed_media_selector_evaluates_against_a_context() {
        // End-to-end: parse -> match. Guards against a parse that silently produces an
        // always-true or never-true condition.
        let props =
            CssPropertyWithConditionsVec::parse("@media (min-width: 800px) { color: red; }")
                .into_library_owned_vec();
        assert_eq!(props.len(), 1);
        let base = DynamicSelectorContext::default();
        assert!(props[0].matches(&base.with_viewport(1024.0, 768.0)));
        assert!(props[0].matches(&base.with_viewport(800.0, 600.0)));
        assert!(!props[0].matches(&base.with_viewport(799.0, 600.0)));
    }
}
