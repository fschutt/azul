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
    /// Window is not focused (equivalent to GTK :backdrop)
    pub backdrop: bool,
    /// Element is currently being dragged (:dragging)
    pub dragging: bool,
    /// A dragged element is over this drop target (:drag-over)
    pub drag_over: bool,
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
    
    /// Create a range with only a minimum value (>= min)
    pub const fn with_min(min_val: f32) -> Self {
        Self {
            min: min_val,
            max: f32::NAN,
        }
    }
    
    /// Create a range with only a maximum value (<= max)
    pub const fn with_max(max_val: f32) -> Self {
        Self {
            min: f32::NAN,
            max: max_val,
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
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

impl_option!(
    OsCondition,
    OptionOsCondition,
    [Debug, Clone, Copy, PartialEq, Eq, Hash]
);

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
    /// Minimum version: >= specified version
    /// Format: OsVersion { os, version_id }
    Min(OsVersion),
    /// Maximum version: <= specified version
    Max(OsVersion),
    /// Exact version match
    Exact(OsVersion),
    /// Desktop environment (Linux only)
    DesktopEnvironment(LinuxDesktopEnv),
}

/// OS version with ordering - only comparable within the same OS family
/// 
/// Each OS has its own version numbering system with named versions.
/// Comparisons between different OS families always return false.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
    pub const fn new(os: OsFamily, version_id: u32) -> Self {
        Self { os, version_id }
    }
    
    /// Compare two versions - only meaningful within the same OS family
    /// Returns None if OS families don't match (comparison not meaningful)
    pub fn compare(&self, other: &Self) -> Option<core::cmp::Ordering> {
        if self.os != other.os {
            None // Cross-OS comparison not meaningful
        } else {
            Some(self.version_id.cmp(&other.version_id))
        }
    }
    
    /// Check if self >= other (for Min conditions)
    pub fn is_at_least(&self, other: &Self) -> bool {
        self.compare(other).map_or(false, |o| o != core::cmp::Ordering::Less)
    }
    
    /// Check if self <= other (for Max conditions)
    pub fn is_at_most(&self, other: &Self) -> bool {
        self.compare(other).map_or(false, |o| o != core::cmp::Ordering::Greater)
    }
}

impl_option!(
    OsVersion,
    OptionOsVersion,
    [Debug, Clone, Copy, PartialEq, Eq, Hash]
);

impl OsVersion {
    
    /// Check if self == other
    pub fn is_exactly(&self, other: &Self) -> bool {
        self.compare(other) == Some(core::cmp::Ordering::Equal)
    }
}

/// OS family for version comparisons
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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

/// Windows version constants - use these in CSS like `@os-version(>= win-xp)`
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
    pub const fn unknown() -> Self {
        Self {
            os: OsFamily::Linux, // Fallback, but version_id 0 means "unknown"
            version_id: 0,
        }
    }
}

/// Parse a named or numeric OS version string
/// Returns None if the version string is not recognized
pub fn parse_os_version(os: OsFamily, version_str: &str) -> Option<OsVersion> {
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
    // Named versions
    match s {
        "2000" | "win2000" | "win-2000" => Some(OsVersion::WIN_2000),
        "xp" | "winxp" | "win-xp" => Some(OsVersion::WIN_XP),
        "vista" | "winvista" | "win-vista" => Some(OsVersion::WIN_VISTA),
        "7" | "win7" | "win-7" => Some(OsVersion::WIN_7),
        "8" | "win8" | "win-8" => Some(OsVersion::WIN_8),
        "8.1" | "win8.1" | "win-8.1" | "win-8-1" => Some(OsVersion::WIN_8_1),
        "10" | "win10" | "win-10" => Some(OsVersion::WIN_10),
        "11" | "win11" | "win-11" => Some(OsVersion::WIN_11),
        // Numeric NT versions
        "5.0" | "nt5.0" => Some(OsVersion::WIN_2000),
        "5.1" | "nt5.1" => Some(OsVersion::WIN_XP),
        "6.0" | "nt6.0" => Some(OsVersion::WIN_VISTA),
        "6.1" | "nt6.1" => Some(OsVersion::WIN_7),
        "6.2" | "nt6.2" => Some(OsVersion::WIN_8),
        "6.3" | "nt6.3" => Some(OsVersion::WIN_8_1),
        "10.0" | "nt10.0" => Some(OsVersion::WIN_10),
        _ => None,
    }
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
    // Parse kernel version like "5.4", "6.0"
    let parts: Vec<&str> = s.split('.').collect();
    if parts.len() >= 2 {
        if let (Ok(major), Ok(minor)) = (parts[0].parse::<u32>(), parts[1].parse::<u32>()) {
            let patch = parts.get(2).and_then(|p| p.parse::<u32>().ok()).unwrap_or(0);
            return Some(OsVersion::new(OsFamily::Linux, major * 1000 + minor * 10 + patch));
        }
    }
    None
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

impl_option!(
    ThemeCondition,
    OptionThemeCondition,
    copy = false,
    [Debug, Clone, PartialEq, Eq, Hash]
);

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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
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

/// Context for evaluating dynamic selectors
#[repr(C)]
#[derive(Debug, Clone)]
pub struct DynamicSelectorContext {
    /// Operating system info
    pub os: OsCondition,
    pub os_version: OsVersion,
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
            window_focused: true,
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
            os_version: system_style.os_version, // Use version from SystemStyle
            desktop_env,
            theme,
            media_type: MediaType::Screen,
            viewport_width: 800.0, // Will be updated with window size
            viewport_height: 600.0,
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
        actual: &OsVersion,
        desktop_env: &OptionLinuxDesktopEnv,
    ) -> bool {
        match condition {
            OsVersionCondition::Exact(ver) => actual.is_exactly(ver),
            OsVersionCondition::Min(ver) => actual.is_at_least(ver),
            OsVersionCondition::Max(ver) => actual.is_at_most(ver),
            OsVersionCondition::DesktopEnvironment(env) => {
                desktop_env.as_ref().map_or(false, |de| de == env)
            }
        }
    }

    fn match_theme(condition: &ThemeCondition, actual: &ThemeCondition) -> bool {
        match (condition, actual) {
            (ThemeCondition::SystemPreferred, _) => true,
            _ => condition == actual,
        }
    }

    fn match_pseudo_state(state: PseudoStateType, ctx: &DynamicSelectorContext) -> bool {
        let node_state = &ctx.pseudo_state;
        match state {
            PseudoStateType::Normal => true, // Normal is always active (base state)
            PseudoStateType::Hover => node_state.hover,
            PseudoStateType::Active => node_state.active,
            PseudoStateType::Focus => node_state.focused,
            PseudoStateType::Disabled => node_state.disabled,
            PseudoStateType::Checked => node_state.checked,
            PseudoStateType::FocusWithin => node_state.focus_within,
            PseudoStateType::Visited => node_state.visited,
            // :backdrop is true when window is NOT focused (opposite of window_focused)
            PseudoStateType::Backdrop => !ctx.window_focused,
            PseudoStateType::Dragging => node_state.dragging,
            PseudoStateType::DragOver => node_state.drag_over,
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
    /// 
    /// Returns `true` for layout-affecting properties like width, height, margin, padding,
    /// font-size, etc. Returns `false` for paint-only properties like color, background,
    /// box-shadow, opacity, transform, etc.
    pub fn is_layout_affecting(&self) -> bool {
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
    pub fn parse(style: &str) -> Self {
        Self::parse_with_conditions(style, Vec::new())
    }
    
    /// Internal recursive parser with inherited conditions
    #[cfg(feature = "parser")]
    fn parse_with_conditions(style: &str, inherited_conditions: Vec<DynamicSelector>) -> Self {
        use crate::props::property::{
            parse_combined_css_property, parse_css_property, CombinedCssPropertyType, CssKeyMap,
            CssPropertyType,
        };

        let mut props = Vec::new();
        let key_map = CssKeyMap::get();
        let style = style.trim();
        
        if style.is_empty() {
            return CssPropertyWithConditionsVec::from_vec(props);
        }

        // Tokenize into segments: properties, pseudo-selectors, and @-rules
        let mut chars = style.chars().peekable();
        let mut current_segment = String::new();
        let mut brace_depth = 0;
        
        while let Some(c) = chars.next() {
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
                        
                        if let Some(parsed) = Self::parse_block_segment(&segment, &inherited_conditions, &key_map) {
                            props.extend(parsed);
                        }
                    }
                }
                ';' if brace_depth == 0 => {
                    // End of a simple property
                    let segment = current_segment.trim().to_string();
                    current_segment.clear();
                    
                    if !segment.is_empty() {
                        if let Some(parsed) = Self::parse_property_segment(&segment, &inherited_conditions, &key_map) {
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
            if let Some(parsed) = Self::parse_property_segment(remaining, &inherited_conditions, &key_map) {
                props.extend(parsed);
            }
        }

        CssPropertyWithConditionsVec::from_vec(props)
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
        let parsed = Self::parse_with_conditions(content, conditions);
        Some(parsed.into_library_owned_vec())
    }
    
    /// Parse a selector string into DynamicSelector conditions
    #[cfg(feature = "parser")]
    fn parse_selector_to_conditions(selector: &str) -> Option<Vec<DynamicSelector>> {
        let selector = selector.trim();
        
        // Handle pseudo-selectors
        if selector.starts_with(':') {
            let pseudo = &selector[1..];
            match pseudo {
                "hover" => return Some(vec![DynamicSelector::PseudoState(PseudoStateType::Hover)]),
                "active" => return Some(vec![DynamicSelector::PseudoState(PseudoStateType::Active)]),
                "focus" => return Some(vec![DynamicSelector::PseudoState(PseudoStateType::Focus)]),
                "focus-within" => return Some(vec![DynamicSelector::PseudoState(PseudoStateType::FocusWithin)]),
                "disabled" => return Some(vec![DynamicSelector::PseudoState(PseudoStateType::Disabled)]),
                "checked" => return Some(vec![DynamicSelector::PseudoState(PseudoStateType::Checked)]),
                "visited" => return Some(vec![DynamicSelector::PseudoState(PseudoStateType::Visited)]),
                "backdrop" => return Some(vec![DynamicSelector::PseudoState(PseudoStateType::Backdrop)]),
                "dragging" => return Some(vec![DynamicSelector::PseudoState(PseudoStateType::Dragging)]),
                "drag-over" => return Some(vec![DynamicSelector::PseudoState(PseudoStateType::DragOver)]),
                _ => return None,
            }
        }
        
        // Handle @-rules
        if selector.starts_with('@') {
            let rule_content = &selector[1..];
            
            // @os-version windows >= win-11
            // @os-version macos >= monterey
            // @os-version macos = sonoma
            // @os-version linux de gnome
            if rule_content.starts_with("os-version ") {
                let version_query = rule_content[11..].trim();
                if let Some(cond) = Self::parse_os_version_condition(version_query) {
                    return Some(vec![DynamicSelector::OsVersion(cond)]);
                }
            }

            // @os linux, @os windows, etc.
            if rule_content.starts_with("os ") {
                let os_name = rule_content[3..].trim();
                if let Some(os_cond) = Self::parse_os_name(os_name) {
                    return Some(vec![DynamicSelector::Os(os_cond)]);
                }
            }
            
            // @media (min-width: 800px), etc.
            if rule_content.starts_with("media ") {
                let media_query = rule_content[6..].trim();
                if let Some(media_conds) = Self::parse_media_query(media_query) {
                    return Some(media_conds);
                }
            }
            
            // @theme dark, @theme light
            if rule_content.starts_with("theme ") {
                let theme = rule_content[6..].trim();
                match theme {
                    "dark" => return Some(vec![DynamicSelector::Theme(ThemeCondition::Dark)]),
                    "light" => return Some(vec![DynamicSelector::Theme(ThemeCondition::Light)]),
                    _ => return None,
                }
            }
            
            return None;
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
    
    /// Parse OS name to OsCondition
    #[cfg(feature = "parser")]
    fn parse_os_name(name: &str) -> Option<OsCondition> {
        match name.to_lowercase().as_str() {
            "linux" => Some(OsCondition::Linux),
            "windows" | "win" => Some(OsCondition::Windows),
            "macos" | "mac" | "osx" => Some(OsCondition::MacOS),
            "ios" => Some(OsCondition::IOS),
            "android" => Some(OsCondition::Android),
            "apple" => Some(OsCondition::Apple),
            "web" | "wasm" => Some(OsCondition::Web),
            "any" | "*" => Some(OsCondition::Any),
            _ => None,
        }
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
                    _ => {}
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

    /// Parse an `@os-version` condition query string.
    ///
    /// Supported formats:
    /// - `windows >= win-11`
    /// - `macos >= monterey`
    /// - `macos = sonoma`
    /// - `ios <= 16`
    /// - `linux de gnome`  (desktop environment)
    #[cfg(feature = "parser")]
    fn parse_os_version_condition(query: &str) -> Option<OsVersionCondition> {
        let query = query.trim();

        // Handle "linux de gnome" for desktop environments
        if query.starts_with("linux") || query.starts_with("Linux") {
            let rest = query[5..].trim();
            if rest.starts_with("de ") || rest.starts_with("DE ") {
                let de_name = rest[3..].trim();
                let de = match de_name.to_lowercase().as_str() {
                    "gnome" => LinuxDesktopEnv::Gnome,
                    "kde" => LinuxDesktopEnv::KDE,
                    "xfce" => LinuxDesktopEnv::XFCE,
                    "unity" => LinuxDesktopEnv::Unity,
                    "cinnamon" => LinuxDesktopEnv::Cinnamon,
                    "mate" => LinuxDesktopEnv::MATE,
                    _ => LinuxDesktopEnv::Other,
                };
                return Some(OsVersionCondition::DesktopEnvironment(de));
            }
        }

        // Parse "os_family operator version" e.g. "windows >= win-11"
        // First extract the OS family name
        let mut parts = query.splitn(2, |c: char| c == '>' || c == '<' || c == '=');
        let os_str = parts.next()?.trim();

        let os_family = match os_str.to_lowercase().as_str() {
            "windows" | "win" => OsFamily::Windows,
            "macos" | "mac" | "osx" => OsFamily::MacOS,
            "ios" => OsFamily::IOS,
            "android" => OsFamily::Android,
            "linux" => OsFamily::Linux,
            _ => return None,
        };

        // Now find the operator and version in the remaining string
        let after_os = &query[os_str.len()..].trim_start();

        // Extract operator
        let (operator, rest) = if after_os.starts_with(">=") {
            (">=", &after_os[2..])
        } else if after_os.starts_with("<=") {
            ("<=", &after_os[2..])
        } else if after_os.starts_with('>') {
            // Treat > same as >= for simplicity (versions are discrete)
            (">=", &after_os[1..])
        } else if after_os.starts_with('<') {
            ("<=", &after_os[1..])
        } else if after_os.starts_with('=') {
            ("=", &after_os[1..])
        } else {
            // No operator: treat as exact match
            ("=", *after_os)
        };

        let version_str = rest.trim();
        let version = parse_os_version(os_family, version_str)?;

        match operator {
            ">=" => Some(OsVersionCondition::Min(version)),
            "<=" => Some(OsVersionCondition::Max(version)),
            "=" => Some(OsVersionCondition::Exact(version)),
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
                    apply_if: conditions.clone(),
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

    /// Parse CSS properties from a string, all with "normal" (unconditional) state
    /// 
    /// Deprecated: Use `parse()` instead which supports selectors and nesting
    #[cfg(feature = "parser")]
    pub fn parse_normal(style: &str) -> Self {
        Self::parse(style)
    }

    /// Parse CSS properties from a string, all with hover condition
    /// 
    /// Deprecated: Use `parse(":hover { ... }")` instead
    #[cfg(feature = "parser")]
    pub fn parse_hover(style: &str) -> Self {
        // Wrap in :hover { } and parse
        let wrapped = format!(":hover {{ {} }}", style);
        Self::parse(&wrapped)
    }

    /// Parse CSS properties from a string, all with active condition
    /// 
    /// Deprecated: Use `parse(":active { ... }")` instead
    #[cfg(feature = "parser")]
    pub fn parse_active(style: &str) -> Self {
        let wrapped = format!(":active {{ {} }}", style);
        Self::parse(&wrapped)
    }

    /// Parse CSS properties from a string, all with focus condition
    /// 
    /// Deprecated: Use `parse(":focus { ... }")` instead
    #[cfg(feature = "parser")]
    pub fn parse_focus(style: &str) -> Self {
        let wrapped = format!(":focus {{ {} }}", style);
        Self::parse(&wrapped)
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
        eprintln!("Parsed {} properties from '{}'", props.len(), style);
        for prop in &props {
            eprintln!("  {:?}", prop.property);
        }
        assert!(props.len() > 0, "Expected overflow to parse into at least 1 property");
    }

    #[test]
    fn test_inline_overflow_y_parse() {
        let style = "overflow-y: scroll;";
        let parsed = CssPropertyWithConditionsVec::parse(style);
        let props = parsed.into_library_owned_vec();
        eprintln!("Parsed {} properties from '{}'", props.len(), style);
        for prop in &props {
            eprintln!("  {:?}", prop.property);
        }
        assert!(props.len() > 0, "Expected overflow-y to parse into at least 1 property");
    }

    #[test]
    fn test_inline_combined_style_with_overflow() {
        let style = "padding: 20px; background-color: #f0f0f0; font-size: 14px; color: #222;overflow: scroll;";
        let parsed = CssPropertyWithConditionsVec::parse(style);
        let props = parsed.into_library_owned_vec();
        eprintln!("Parsed {} properties from combined style", props.len());
        for prop in &props {
            eprintln!("  {:?}", prop.property);
        }
        // padding:20px expands to 4, background:1, font-size:1, color:1, overflow:2 = 10
        assert!(props.len() >= 9, "Expected at least 9 properties, got {}", props.len());
    }

    #[test]
    fn test_inline_grid_template_columns_parse() {
        use crate::props::layout::grid::GridTrackSizing;
        let style = "display: grid; grid-template-columns: repeat(4, 160px); gap: 16px; padding: 10px;";
        let parsed = CssPropertyWithConditionsVec::parse(style);
        let props = parsed.into_library_owned_vec();
        eprintln!("Parsed {} properties from grid style", props.len());
        for prop in &props {
            eprintln!("  {:?}", prop.property);
        }
        // Find grid-template-columns property
        let grid_cols = props.iter().find(|p| {
            matches!(p.property, CssProperty::GridTemplateColumns(_))
        }).expect("Expected GridTemplateColumns property");
        
        if let CssProperty::GridTemplateColumns(ref value) = grid_cols.property {
            let template = value.get_property().expect("Expected Exact value");
            let tracks = template.tracks.as_ref();
            assert_eq!(tracks.len(), 4, "Expected 4 tracks");
            for (i, track) in tracks.iter().enumerate() {
                eprintln!("  Track {}: {:?} (is_fixed={})", i, track, matches!(track, GridTrackSizing::Fixed(_)));
                assert!(matches!(track, GridTrackSizing::Fixed(_)), 
                    "Track {} should be Fixed(160px), got {:?}", i, track);
            }
        } else {
            panic!("Expected CssProperty::GridTemplateColumns");
        }
    }
}
