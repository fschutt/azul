# Refactoring Plan: Dynamisches CSS-System mit `apply_if` und Container Queries

## 1. Architektur-Übersicht

### Aktueller Zustand
- `NodeDataInlineCssProperty` mit State-Varianten (Normal, Hover, Active, Focus)
- Inline-Styles haben höchste Priorität
- Keine dynamischen Selektoren oder Breakpoints

### Ziel-Zustand
- **`CssPropertyWithConditions` ERSETZT `CssProperty`** (BREAKING CHANGE)
- **Konfliktauflösung**: "Last Found Wins" - keine Specificity
- **Interne Umschreibung**: `with_hover_css_property()` bleibt, wird intern zu `CssPropertyWithConditions` konvertiert
- Dynamische Evaluation zur Runtime basierend auf:
  - **Betriebssystem** (OS, OS-Version)
  - **Media Queries** (@media print/screen, min-width, max-width)
  - **Container Queries** (@container, container-width, container-height)
  - **Theme** (dark/light/custom)
  - **Pseudo-States** (hover, active, focus - bisherige Logik)
- **Fokus**: Erkennen, ob ein Resize/Change ein Re-Layout erfordert

---

## 2. Neue Datenstrukturen (C-kompatibel)

### 2.1 `DynamicSelector` (Core-Ebene, C-kompatibel)

**WICHTIG**: Alle Enums müssen `#[repr(C, u8)]` sein und nur ein Feld haben für C-API-Kompatibilität.

```rust
// core/src/dynamic_selector.rs (neue Datei)

/// Dynamischer Selektor, der zur Runtime evaluiert wird
/// C-kompatibel: Tagged union mit einem Feld
#[repr(C, u8)]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum DynamicSelector {
    /// Betriebssystem-Bedingung
    Os(OsCondition) = 0,
    /// Betriebssystem-Version (z.B. macOS 14.0, Windows 11)
    OsVersion(OsVersionCondition) = 1,
    /// Media Query (print/screen)
    Media(MediaType) = 2,
    /// Viewport-Breite min/max (für @media)
    ViewportWidth(MinMaxRange) = 3,
    /// Viewport-Höhe min/max (für @media)
    ViewportHeight(MinMaxRange) = 4,
    /// Container-Breite min/max (für @container)
    ContainerWidth(MinMaxRange) = 5,
    /// Container-Höhe min/max (für @container)
    ContainerHeight(MinMaxRange) = 6,
    /// Container-Name (für benannte @container queries)
    ContainerName(AzString) = 7,
    /// Theme (dark/light/custom)
    Theme(ThemeCondition) = 8,
    /// Aspect Ratio (min/max für @media und @container)
    AspectRatio(MinMaxRange) = 9,
    /// Orientation (portrait/landscape)
    Orientation(OrientationType) = 10,
    /// Reduced Motion (accessibility)
    PrefersReducedMotion(BoolCondition) = 11,
    /// High Contrast (accessibility)
    PrefersHighContrast(BoolCondition) = 12,
    /// Pseudo-State (hover, active, focus, etc.)
    PseudoState(PseudoStateType) = 13,
}

impl_vec!(
    DynamicSelector,
    DynamicSelectorVec,
    DynamicSelectorVecDestructor
);
impl_vec_debug!(DynamicSelector, DynamicSelectorVec);
impl_vec_partialord!(DynamicSelector, DynamicSelectorVec);
impl_vec_ord!(DynamicSelector, DynamicSelectorVec);
impl_vec_partialeq!(DynamicSelector, DynamicSelectorVec);
impl_vec_eq!(DynamicSelector, DynamicSelectorVec);
impl_vec_hash!(DynamicSelector, DynamicSelectorVec);

/// Min/Max Range für numerische Bedingungen (C-kompatibel)
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct MinMaxRange {
    /// Minimum value (NaN = keine Mindestgrenze)
    pub min: f32,
    /// Maximum value (NaN = keine Höchstgrenze)
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
        if self.min.is_nan() { None } else { Some(self.min) }
    }
    
    pub fn max(&self) -> Option<f32> {
        if self.max.is_nan() { None } else { Some(self.max) }
    }
    
    pub fn matches(&self, value: f32) -> bool {
        let min_ok = self.min.is_nan() || value >= self.min;
        let max_ok = self.max.is_nan() || value <= self.max;
        min_ok && max_ok
    }
}

/// Boolean-Bedingung (C-kompatibel)
#[repr(C, u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BoolCondition {
    False = 0,
    True = 1,
}

impl From<bool> for BoolCondition {
    fn from(b: bool) -> Self {
        if b { Self::True } else { Self::False }
    }
}

impl From<BoolCondition> for bool {
    fn from(b: BoolCondition) -> Self {
        matches!(b, BoolCondition::True)
    }
}

#[repr(C, u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OsCondition {
    Any = 0,
    Apple = 1,      // macOS + iOS
    MacOS = 2,
    IOS = 3,
    Linux = 4,
    Windows = 5,
    Android = 6,
    Web = 7,        // WASM
}

#[repr(C, u8)]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum OsVersionCondition {
    /// Semantic version: "14.0", "11.0.22000"
    Exact(AzString) = 0,
    /// Minimum version: >= "14.0"
    Min(AzString) = 1,
    /// Maximum version: <= "14.0"
    Max(AzString) = 2,
    /// Desktop environment (Linux)
    DesktopEnvironment(LinuxDesktopEnv) = 3,
}

#[repr(C, u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LinuxDesktopEnv {
    Gnome = 0,
    KDE = 1,
    XFCE = 2,
    Unity = 3,
    Cinnamon = 4,
    MATE = 5,
    Other = 6,
}

#[repr(C, u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MediaType {
    Screen = 0,
    Print = 1,
    All = 2,
}

#[repr(C, u8)]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ThemeCondition {
    Light = 0,
    Dark = 1,
    Custom(AzString) = 2,
    /// System preference
    SystemPreferred = 3,
}

#[repr(C, u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OrientationType {
    Portrait = 0,
    Landscape = 1,
}

#[repr(C, u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PseudoStateType {
    /// Kein spezieller State (entspricht "Normal" in NodeDataInlineCssProperty)
    Normal = 0,
    /// Element wird mit der Maus überfahren (:hover)
    Hover = 1,
    /// Element ist aktiv/wird geklickt (:active)
    Active = 2,
    /// Element hat Fokus (:focus)
    Focus = 3,
    /// Element ist deaktiviert (:disabled)
    Disabled = 4,
    /// Element ist ausgewählt/aktiviert (:checked)
    Checked = 5,
    /// Element oder Kind hat Fokus (:focus-within)
    FocusWithin = 6,
    /// Link wurde besucht (:visited)
    Visited = 7,
}
```

### 2.2 `CssPropertyWithConditions` - ERSETZT `CssProperty` direkt

**WICHTIG**: `CssPropertyWithConditions` ist der NEUE Standard-Typ für alle CSS-Properties!

```rust
// core/src/style.rs

/// CSS-Property mit optionalen dynamischen Bedingungen (C-kompatibel)
/// ERSETZT den alten CssProperty-Typ vollständig
#[repr(C)]
#[derive(Debug, Clone, PartialEq)]
pub struct CssPropertyWithConditions {
    /// Die eigentliche CSS-Property
    pub property: CssProperty,
    /// Dynamische Selektoren, die ALLE erfüllt sein müssen (AND-Verknüpfung)
    /// Leerer Vec = Always Active (entspricht altem CssProperty ohne Conditions)
    pub apply_if: DynamicSelectorVec,
}

impl_vec!(
    CssPropertyWithConditions,
    CssPropertyWithConditionsVec,
    CssPropertyWithConditionsVecDestructor
);
impl_vec_debug!(CssPropertyWithConditions, CssPropertyWithConditionsVec);
impl_vec_partialord!(CssPropertyWithConditions, CssPropertyWithConditionsVec);
impl_vec_partialeq!(CssPropertyWithConditions, CssPropertyWithConditionsVec);

impl CssPropertyWithConditions {
    /// Erstelle eine einfache Property ohne Bedingungen (Always Active)
    pub const fn simple(property: CssProperty) -> Self {
        Self {
            property,
            apply_if: DynamicSelectorVec::from_const_slice(&[]),
        }
    }
    
    /// Erstelle Property mit Bedingungen
    pub fn conditional(property: CssProperty, conditions: DynamicSelectorVec) -> Self {
        Self {
            property,
            apply_if: conditions,
        }
    }
    
    /// Prüfe, ob alle Bedingungen erfüllt sind
    pub fn matches(&self, context: &DynamicSelectorContext) -> bool {
        // Leerer Vec = Always Active
        if self.apply_if.is_empty() {
            return true;
        }
        // Alle Conditions müssen matchen (AND)
        self.apply_if.as_ref().iter().all(|sel| sel.matches(context))
    }
    
    /// Gibt zurück, ob diese Property Layout-relevant ist
    pub fn is_layout_affecting(&self) -> bool {
        self.property.is_layout_affecting()
    }
    
    /// Gibt zurück, ob diese Property von Viewport-Größe abhängt
    pub fn depends_on_viewport(&self) -> bool {
        self.apply_if.as_ref().iter().any(|sel| matches!(
            sel,
            DynamicSelector::ViewportWidth(_) | 
            DynamicSelector::ViewportHeight(_) |
            DynamicSelector::AspectRatio(_) |
            DynamicSelector::Orientation(_)
        ))
    }
    
    /// Gibt zurück, ob diese Property von Container-Größe abhängt
    pub fn depends_on_container(&self) -> bool {
        self.apply_if.as_ref().iter().any(|sel| matches!(
            sel,
            DynamicSelector::ContainerWidth(_) | 
            DynamicSelector::ContainerHeight(_) |
            DynamicSelector::ContainerName(_)
        ))
    }
}

impl CssProperty {
    /// Prüfe, ob diese Property das Layout beeinflusst
    pub fn is_layout_affecting(&self) -> bool {
        use CssPropertyType::*;
        matches!(
            self.get_type(),
            Display | Width | Height | MinWidth | MinHeight | MaxWidth | MaxHeight |
            Padding | PaddingTop | PaddingRight | PaddingBottom | PaddingLeft |
            Margin | MarginTop | MarginRight | MarginBottom | MarginLeft |
            Border | BorderTop | BorderRight | BorderBottom | BorderLeft |
            FlexDirection | FlexWrap | FlexGrow | FlexShrink | FlexBasis |
            JustifyContent | AlignItems | AlignContent | AlignSelf |
            Position | Top | Right | Bottom | Left |
            FontSize | LineHeight | LetterSpacing | WordSpacing
        )
    }
}
```

### 2.3 `DynamicSelectorContext` (C-kompatibel)

```rust
// core/src/style.rs

/// Kontext für die Evaluierung dynamischer Selektoren (C-kompatibel)
#[repr(C)]
#[derive(Debug, Clone)]
pub struct DynamicSelectorContext {
    /// Betriebssystem-Info
    pub os: OsCondition,
    pub os_version: AzString,
    pub desktop_env: OptionLinuxDesktopEnv,
    
    /// Media-Info (von WindowState)
    pub media_type: MediaType,
    pub viewport_width: f32,
    pub viewport_height: f32,
    
    /// Container-Info (vom Parent-Node)
    /// NaN = kein Container
    pub container_width: f32,
    pub container_height: f32,
    pub container_name: OptionAzString,
    
    /// Theme-Info
    pub theme: ThemeCondition,
    
    /// Accessibility-Präferenzen
    pub prefers_reduced_motion: BoolCondition,
    pub prefers_high_contrast: BoolCondition,
    
    /// Orientation
    pub orientation: OrientationType,
    
    /// Node-State (hover, active, focus, disabled, checked, focus_within, visited)
    pub pseudo_state: StyledNodeState,
}

impl_option!(LinuxDesktopEnv, OptionLinuxDesktopEnv, Copy, [Debug, Clone, PartialEq, Eq, Hash]);

impl DynamicSelectorContext {
    /// Prüfe, ob sich dieser Context im Vergleich zum vorherigen geändert hat
    /// und ob die Änderung ein Re-Layout erfordert
    pub fn requires_relayout(&self, previous: &Self, properties: &[CssPropertyWithConditions]) -> bool {
        // Prüfe nur Layout-relevante Properties, die von geänderten Bedingungen abhängen
        for prop in properties {
            if !prop.is_layout_affecting() {
                continue;
            }
            
            // War die Property vorher aktiv?
            let was_active = prop.matches(previous);
            // Ist die Property jetzt aktiv?
            let is_active = prop.matches(self);
            
            // Wenn sich der Aktivierungsstatus geändert hat → Re-Layout
            if was_active != is_active {
                return true;
            }
        }
        
        false
    }
    
    /// Erstelle einen Hash für Caching-Zwecke
    pub fn cache_key(&self) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        
        let mut hasher = DefaultHasher::new();
        self.os.hash(&mut hasher);
        self.media_type.hash(&mut hasher);
        ((self.viewport_width * 100.0) as u32).hash(&mut hasher);
        ((self.viewport_height * 100.0) as u32).hash(&mut hasher);
        self.theme.hash(&mut hasher);
        self.orientation.hash(&mut hasher);
        hasher.finish()
    }
}

impl DynamicSelector {
    /// Prüfe, ob dieser Selektor im gegebenen Kontext matcht
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
            Self::ContainerName(name) => {
                ctx.container_name.is_some() && ctx.container_name.as_ref().unwrap() == name
            }
            Self::Theme(theme) => Self::match_theme(theme, &ctx.theme),
            Self::AspectRatio(range) => {
                let ratio = ctx.viewport_width / ctx.viewport_height.max(1.0);
                range.matches(ratio)
            }
            Self::Orientation(orient) => *orient == ctx.orientation,
            Self::PrefersReducedMotion(pref) => (*pref).into() == ctx.prefers_reduced_motion.into(),
            Self::PrefersHighContrast(pref) => (*pref).into() == ctx.prefers_high_contrast.into(),
            Self::PseudoState(state) => Self::match_pseudo_state(*state, &ctx.pseudo_state),
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
    
    fn match_pseudo_state(state: PseudoStateType, node_state: &StyledNodeState) -> bool {
        match state {
            PseudoStateType::Normal => true, // Normal ist immer aktiv (Basis-State)
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
```

---

## 3. Property-Evaluierung und State-Management

### 3.1 Erweitere StyledNodeState

**WICHTIG**: Der `StyledNodeState` muss ALLE 8 Pseudo-States tracken:

```rust
// core/src/styled_dom.rs

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct StyledNodeState {
    pub hover: bool,
    pub active: bool,
    pub focused: bool,
    pub disabled: bool,
    pub checked: bool,
    pub focus_within: bool,
    pub visited: bool,
}

impl StyledNodeState {
    /// Prüfe, ob ein bestimmter Pseudo-State aktiv ist
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
    
    /// Konvertiere von NodeDataInlineCssProperty-State
    pub fn from_inline_css_state(state: NodeDataInlineCssPropertyState) -> Self {
        let mut s = Self::default();
        match state {
            NodeDataInlineCssPropertyState::Normal => {},
            NodeDataInlineCssPropertyState::Hover => s.hover = true,
            NodeDataInlineCssPropertyState::Active => s.active = true,
            NodeDataInlineCssPropertyState::Focus => s.focused = true,
            NodeDataInlineCssPropertyState::Disabled => s.disabled = true,
            NodeDataInlineCssPropertyState::Checked => s.checked = true,
            NodeDataInlineCssPropertyState::FocusWithin => s.focus_within = true,
            NodeDataInlineCssPropertyState::Visited => s.visited = true,
        }
        s
    }
}
```

### 3.2 Bestehende `css::system::SystemStyle`-Integration

**WICHTIG**: Azul hat BEREITS ein System-Style-Detection-System in `css/src/system.rs`!

**Bestehendes System**:
```rust
// css/src/system.rs (EXISTIERT BEREITS)

pub struct SystemStyle {
    pub theme: Theme,               // Light/Dark
    pub platform: Platform,         // Windows, MacOs, Linux(Desktop), Android, iOS
    pub colors: SystemColors,       // text, background, accent, etc.
    pub fonts: SystemFonts,         // ui_font, monospace_font
    pub metrics: SystemMetrics,     // corner_radius, border_width
    pub scrollbar: Option<ComputedScrollbarStyle>,
    pub app_specific_stylesheet: Option<Box<Stylesheet>>,
}

impl SystemStyle {
    /// Erkannt beim App-Start EINMALIG
    pub fn new() -> Self {
        // Mit feature="io": Runtime-Detection
        //   - Linux: Detektiert GNOME/KDE via gsettings/kreadconfig
        //   - Windows: Liest Registry für Accent-Color
        //   - macOS: Queries NSAppearance
        // Ohne feature="io": Hardcoded defaults (defaults::windows_11_light(), etc.)
    }
}
```

**Aktuelles Vorgehen**:
1. **App-Start**: `SystemStyle::new()` wird EINMALIG aufgerufen
2. **Speicherung**: In `Arc<SystemStyle>` in `AppResources` (Linux/Windows) oder Window-struct (macOS)
3. **Nutzung**: Wird für CSD-Titlebar-Styling und Widget-Colors verwendet

**Für Dynamic CSS**:
- `OsCondition` und `OsVersionCondition` müssen aus `SystemStyle.platform` abgeleitet werden
- `ThemeCondition` kommt von `SystemStyle.theme`
- **Hard-Coded Overrides**: `WindowCreateOptions` kann `force_platform` haben → Überschreibt Auto-Detection

**Neue Felder für WindowCreateOptions**:
```rust
// layout/src/window_state.rs

#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct WindowCreateOptions {
    pub window_state: FullWindowState,
    pub size_to_content: bool,
    pub renderer: OptionRendererOptions,
    pub theme: OptionWindowTheme,
    pub create_callback: OptionCallback,
    pub hot_reload: bool,
    
    // NEU: Force specific platform styling (overrides SystemStyle.platform)
    pub force_platform: OptionPlatform,
    // NEU: Force specific desktop environment (Linux only)
    pub force_desktop_env: OptionDesktopEnvironment,
}
```

**Priority Logic**:
1. `WindowCreateOptions.force_platform` (höchste Priorität - Developer Override)
2. `SystemStyle.platform` (Auto-Detection vom OS)
3. Compile-Time Fallback (niedrigste Priorität)

### 3.3 Änderungen in `css/src/getters.rs`

**KRITISCH**: Die `get_property()` Funktion muss von `NodeDataInlineCssProperty` zu `CssPropertyWithConditions` wechseln.

**Vorher (NodeDataInlineCssProperty)**:
```rust
// css/src/getters.rs (ALT)

pub fn get_property(
    node_id: NodeId,
    property_type: CssPropertyType,
    node_state: NodeState, // hover, active, focus
    styled_dom: &StyledDom,
    css: &Css,
) -> Option<CssProperty> {
    // 1. Prüfe Inline-Styles nach Priorität:
    //    - Normal
    //    - Hover (wenn hover = true)
    //    - Active (wenn active = true)
    //    - Focus (wenn focused = true)
    let node_data = styled_dom.get_node_data(node_id)?;
    
    // Inline-Styles haben HÖCHSTE Priorität
    if let Some(prop) = get_inline_property(node_data, property_type, node_state) {
        return Some(prop);
    }
    
    // 2. Prüfe CSS-Stylesheet
    get_stylesheet_property(node_id, property_type, styled_dom, css)
}

fn get_inline_property(
    node_data: &NodeData,
    property_type: CssPropertyType,
    node_state: NodeState,
) -> Option<CssProperty> {
    // Priorität: Focus > Active > Hover > Normal
    if node_state.focused {
        if let Some(prop) = find_property(&node_data.inline_css_props, property_type, 
            NodeDataInlineCssPropertyState::Focus) {
            return Some(prop);
        }
    }
    
    if node_state.active {
        if let Some(prop) = find_property(&node_data.inline_css_props, property_type,
            NodeDataInlineCssPropertyState::Active) {
            return Some(prop);
        }
    }
    
    if node_state.hover {
        if let Some(prop) = find_property(&node_data.inline_css_props, property_type,
            NodeDataInlineCssPropertyState::Hover) {
            return Some(prop);
        }
    }
    
    // Normal ist Fallback
    find_property(&node_data.inline_css_props, property_type,
        NodeDataInlineCssPropertyState::Normal)
}

fn find_property(
    props: &[NodeDataInlineCssProperty],
    property_type: CssPropertyType,
    state: NodeDataInlineCssPropertyState,
) -> Option<CssProperty> {
    props.iter()
        .filter(|p| p.get_state() == state)
        .filter_map(|p| p.get_property())
        .find(|p| p.get_type() == property_type)
        .cloned()
}
```

**Nachher (CssPropertyWithConditions)**:
```rust
// css/src/getters.rs (NEU - VEREINFACHT)

pub fn get_property(
    node_id: NodeId,
    property_type: CssPropertyType,
    context: &DynamicSelectorContext,
    styled_dom: &StyledDom,
    css: &Css,
) -> Option<CssProperty> {
    let node_data = styled_dom.get_node_data(node_id)?;
    
    // 1. Prüfe Inline-Properties (HÖCHSTE Priorität)
    // LAST WINS: Iteriere rückwärts, erste matchende Property gewinnt
    for prop_with_conditions in node_data.inline_css_props.iter().rev() {
        if prop_with_conditions.property.get_type() == property_type 
            && prop_with_conditions.matches(context) {
            return Some(prop_with_conditions.property.clone());
        }
    }
    
    // 2. Prüfe CSS-Stylesheet (niedrigere Priorität)
    get_stylesheet_property(node_id, property_type, context, styled_dom, css)
}

// WICHTIG: get_all_properties() für Layout-Berechnungen
// Muss ALLE aktiven Properties zurückgeben, nicht nur eine
pub fn get_all_properties(
    node_id: NodeId,
    context: &DynamicSelectorContext,
    styled_dom: &StyledDom,
    css: &Css,
) -> Vec<CssProperty> {
    let node_data = styled_dom.get_node_data(node_id)?;
    let mut result = Vec::new();
    let mut seen_types = std::collections::HashSet::new();
    
    // Sammle Properties in Specificity-Reihenfolge
    for property_type in CssPropertyType::all() {
        if let Some(prop) = get_property(node_id, property_type, context, styled_dom, css) {
            if seen_types.insert(property_type) {
                result.push(prop);
            }
        }
    }
    
    result
}
```

**Wichtige Änderungen**:
1. **Vollständiger Context**: Statt nur `NodeState` übergeben wir den kompletten `DynamicSelectorContext`
2. **Condition-Matching**: ALLE Properties werden gefiltert nach `matches(context)`
3. **Specificity-Berechnung**: Base-Specificity (Inline vs. Stylesheet) + Condition-Specificity
4. **Alle States gleichzeitig**: Die Pseudo-State-Logik ist jetzt in `DynamicSelector::PseudoState` gekapselt
5. **Keine State-Priorisierung mehr**: Focus/Active/Hover werden durch Specificity sortiert, nicht hardcodiert

**Migration-Hinweis**: Während der Übergangsphase müssen beide Systeme parallel existieren:
```rust
// Während Migration
pub fn get_property_legacy(
    node_id: NodeId,
    property_type: CssPropertyType,
    node_state: NodeState,
    styled_dom: &StyledDom,
    css: &Css,
) -> Option<CssProperty> {
    // Konvertiere NodeState zu DynamicSelectorContext
    let context = DynamicSelectorContext::from_node_state(node_state, styled_dom.window_state);
    get_property(node_id, property_type, &context, styled_dom, css)
}
```

---

## 4. Window-Config Erweiterung & SystemStyle-Integration

### 4.1 OS-Detection aus bestehendem `SystemStyle`

**KEINE neue OS-Detection nötig** - nutze bestehende `css::system::SystemStyle`!

```rust
// core/src/dynamic_selector.rs (NEUE Konvertierungs-Helfer)

impl OsCondition {
    /// Konvertiere von css::system::Platform
    pub fn from_system_platform(platform: &azul_css::system::Platform) -> Self {
        use azul_css::system::Platform;
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

impl LinuxDesktopEnv {
    /// Konvertiere von css::system::DesktopEnvironment
    pub fn from_system_desktop_env(de: &azul_css::system::DesktopEnvironment) -> Self {
        use azul_css::system::DesktopEnvironment;
        match de {
            DesktopEnvironment::Gnome => LinuxDesktopEnv::Gnome,
            DesktopEnvironment::Kde => LinuxDesktopEnv::KDE,
            DesktopEnvironment::Other(_) => LinuxDesktopEnv::Other,
        }
    }
}

impl ThemeCondition {
    /// Konvertiere von css::system::Theme
    pub fn from_system_theme(theme: azul_css::system::Theme) -> Self {
        use azul_css::system::Theme;
        match theme {
            Theme::Light => ThemeCondition::Light,
            Theme::Dark => ThemeCondition::Dark,
        }
    }
}
```

### 4.2 WindowCreateOptions Erweiterung

```rust
// layout/src/window_state.rs

#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct WindowCreateOptions {
    pub window_state: FullWindowState,
    pub size_to_content: bool,
    pub renderer: OptionRendererOptions,
    pub theme: OptionWindowTheme,
    pub create_callback: OptionCallback,
    pub hot_reload: bool,
    
    // NEU: Force specific platform styling (overrides SystemStyle auto-detection)
    /// Wenn gesetzt: Überschreibt die automatische OS-Detection
    /// Use-Case: Testing, Previewing different OS styles, Cross-platform screenshots
    pub force_platform: OptionPlatform,
    
    // NEU: Force specific desktop environment (Linux only)
    pub force_desktop_env: OptionDesktopEnvironment,
}

impl_option!(Platform, OptionPlatform, [Debug, Clone, PartialEq]);
impl_option!(DesktopEnvironment, OptionDesktopEnvironment, [Debug, Clone, PartialEq]);
```

### 4.3 DynamicSelectorContext Erweiterung

```rust
// core/src/style.rs

/// Kontext für die Evaluierung dynamischer Selektoren (C-kompatibel)
#[repr(C)]
#[derive(Debug, Clone)]
pub struct DynamicSelectorContext {
    // === OS-Info (von SystemStyle oder force_platform Override) ===
    pub os: OsCondition,
    pub os_version: AzString,  // Aktuell nicht aus SystemStyle extrahierbar
    pub desktop_env: OptionLinuxDesktopEnv,
    
    // === Theme (von SystemStyle.theme oder WindowCreateOptions.theme) ===
    pub theme: ThemeCondition,
    
    /// Media-Info (von WindowState)
    pub media_type: MediaType,
    pub viewport_width: f32,
    pub viewport_height: f32,
    
    /// Container-Info (vom Parent-Node)
    /// NaN = kein Container
    pub container_width: f32,
    pub container_height: f32,
    pub container_name: OptionAzString,
    
    /// Accessibility-Präferenzen (TODO: Aus SystemStyle erweitern?)
    pub prefers_reduced_motion: BoolCondition,
    pub prefers_high_contrast: BoolCondition,
    
    /// Orientation
    pub orientation: OrientationType,
    
    /// Node-State (hover, active, focus, disabled, checked, focus_within, visited)
    pub pseudo_state: StyledNodeState,
}

impl DynamicSelectorContext {
    /// Erstelle Context aus SystemStyle + Window-Overrides
    pub fn from_window_state(
        window_state: &FullWindowState,
        system_style: &Arc<azul_css::system::SystemStyle>,
        force_platform: Option<azul_css::system::Platform>,
        node_state: StyledNodeState,
        container_info: Option<(f32, f32, Option<AzString>)>,
    ) -> Self {
        // Priority: force_platform > system_style.platform
        let platform = force_platform.as_ref().unwrap_or(&system_style.platform);
        
        let os = OsCondition::from_system_platform(platform);
        let desktop_env = if let azul_css::system::Platform::Linux(de) = platform {
            OptionLinuxDesktopEnv::Some(LinuxDesktopEnv::from_system_desktop_env(de))
        } else {
            OptionLinuxDesktopEnv::None
        };
        
        // Theme: Nutze window_state.theme falls gesetzt, sonst system_style.theme
        let theme = match window_state.theme {
            WindowTheme::System => ThemeCondition::from_system_theme(system_style.theme),
            WindowTheme::Light => ThemeCondition::Light,
            WindowTheme::Dark => ThemeCondition::Dark,
        };
        
        let (container_width, container_height, container_name) = container_info
            .map(|(w, h, n)| (w, h, n.into()))
            .unwrap_or((f32::NAN, f32::NAN, OptionAzString::None));
        
        Self {
            os,
            os_version: AzString::from("0.0"), // TODO: Version-Detection
            desktop_env,
            theme,
            media_type: MediaType::Screen, // TODO: Print-Mode
            viewport_width: window_state.size.dimensions.width,
            viewport_height: window_state.size.dimensions.height,
            container_width,
            container_height,
            container_name,
            prefers_reduced_motion: BoolCondition::False, // TODO: Accessibility
            prefers_high_contrast: BoolCondition::False,
            orientation: if window_state.size.dimensions.width > window_state.size.dimensions.height {
                OrientationType::Landscape
            } else {
                OrientationType::Portrait
            },
            pseudo_state: node_state,
        }
    }
}
```

---

## 4. Re-Layout Detection (KERN-FEATURE)

### 4.1 Breakpoint-Detection beim Resize

```rust
// dll/src/desktop/shell2/common/event_v2.rs

impl PlatformWindowV2Trait for MacosWindow {
    fn handle_resize(&mut self, new_size: PhysicalSize) {
        // Create contexts for before and after
        let old_context = self.create_base_dynamic_context(&self.current_window_state);
        
        // Update window state
        self.current_window_state.size = new_size;
        
        let new_context = self.create_base_dynamic_context(&self.current_window_state);
        
        // Check if any layout-affecting properties changed
        let needs_relayout = self.check_if_relayout_needed(&old_context, &new_context);
        
        if needs_relayout {
            // Force re-layout with new context
            self.mark_frame_needs_regeneration();
        } else {
            // Just re-render (no layout change)
            self.mark_needs_repaint();
        }
        
        // Normal resize handling...
    }
    
    /// Prüfe, ob irgendwelche Nodes ein Re-Layout benötigen
    fn check_if_relayout_needed(
        &self,
        old_context: &DynamicSelectorContext,
        new_context: &DynamicSelectorContext,
    ) -> bool {
        let layout_window = match self.get_layout_window() {
            Some(lw) => lw,
            None => return false,
        };
        
        // Prüfe alle DOMs
        for (_dom_id, layout_result) in &layout_window.layout_results {
            let node_data = layout_result.styled_dom.node_data.as_container();
            
            // Prüfe alle Nodes mit bedingten Styles
            for node_data in node_data.internal.iter() {
                if node_data.inline_css_props.requires_relayout(old_context, new_context) {
                    return true;
                }
            }
        }
        
        false
    }
    
    fn create_base_dynamic_context(&self, window_state: &FullWindowState) -> DynamicSelectorContext {
        DynamicSelectorContext {
            os: window_state.os_info.os_type,
            os_version: window_state.os_info.version.clone(),
            desktop_env: window_state.os_info.desktop_env,
            media_type: window_state.media_type,
            viewport_width: window_state.size.dimensions.width,
            viewport_height: window_state.size.dimensions.height,
            container_width: f32::NAN,
            container_height: f32::NAN,
            container_name: OptionAzString::None,
            theme: window_state.theme.clone(),
            prefers_reduced_motion: window_state.prefers_reduced_motion,
            prefers_high_contrast: window_state.prefers_high_contrast,
            orientation: if window_state.size.dimensions.width > window_state.size.dimensions.height {
                OrientationType::Landscape
            } else {
                OrientationType::Portrait
            },
            pseudo_state: StyledNodeState::default(),
        }
    }
}

impl CssPropertyWithConditionsVec {
    /// Prüfe, ob irgendeine Property ein Re-Layout erfordert
    pub fn requires_relayout(
        &self,
        old_context: &DynamicSelectorContext,
        new_context: &DynamicSelectorContext,
    ) -> bool {
        for prop in self.as_ref().iter() {
            if !prop.is_layout_affecting() {
                continue;
            }
            
            let was_active = prop.matches(old_context);
            let is_active = prop.matches(new_context);
            
            if was_active != is_active {
                return true;
            }
        }
        false
    }
}
```

---

## 5. Container Query Implementierung

### 5.1 Container-Annotation

```rust
// core/src/dom.rs

impl Dom {
    /// Markiere diesen Node als Container für @container queries
    pub fn with_container_name(mut self, name: AzString) -> Self {
        self.set_container_name(name);
        self
    }
    
    /// Markiere als anonymer Container (nutzt Position in Hierarchie)
    pub fn as_container(mut self) -> Self {
        self.set_container_name(AzString::from(""));
        self
    }
}

// NodeData-Erweiterung
#[repr(C)]
pub struct NodeData {
    // ...existing fields...
    
    /// Ist dieser Node ein Container? (für @container queries)
    pub is_container: BoolCondition,
    pub container_name: OptionAzString,
}
```

### 5.2 Container-Context beim Layout

```rust
// layout/src/window.rs

impl LayoutWindow {
    /// Erstelle DynamicSelectorContext für einen Node
    fn create_dynamic_context_for_node(
        &self,
        window_state: &FullWindowState,
        node_id: NodeId,
        node_state: &StyledNodeState,
    ) -> DynamicSelectorContext {
        // Finde Container-Parent
        let (container_width, container_height, container_name) = 
            self.find_parent_container(node_id);
        
        DynamicSelectorContext {
            os: window_state.os_info.os_type,
            os_version: window_state.os_info.version.clone(),
            desktop_env: window_state.os_info.desktop_env,
            media_type: window_state.media_type,
            viewport_width: window_state.size.dimensions.width,
            viewport_height: window_state.size.dimensions.height,
            container_width: container_width.unwrap_or(f32::NAN),
            container_height: container_height.unwrap_or(f32::NAN),
            container_name: container_name.into(),
            theme: window_state.theme.clone(),
            prefers_reduced_motion: window_state.prefers_reduced_motion,
            prefers_high_contrast: window_state.prefers_high_contrast,
            orientation: if window_state.size.dimensions.width > window_state.size.dimensions.height {
                OrientationType::Landscape
            } else {
                OrientationType::Portrait
            },
            pseudo_state: node_state.clone(),
        }
    }
    
    /// Finde Parent-Container für @container queries
    fn find_parent_container(&self, node_id: NodeId) -> (Option<f32>, Option<f32>, Option<AzString>) {
        // Traverse up the tree
        let mut current = node_id;
        loop {
            if let Some(parent) = self.get_parent_node(current) {
                if parent.is_container.into() {
                    // Hole Layout-Rect des Containers
                    if let Some(rect) = self.get_node_rect(parent) {
                        let name = if parent.container_name.is_some() {
                            parent.container_name.clone().into()
                        } else {
                            None
                        };
                        return (Some(rect.size.width), Some(rect.size.height), name);
                    }
                }
                current = parent.id;
            } else {
                break;
            }
        }
        (None, None, None)
    }
}
```

---

## 6. Button-Widget mit Multi-OS-Styles

### 6.1 Umstellung auf CssPropertyWithConditions

```rust
// layout/src/widgets/button.rs

impl Button {
    pub fn new(label: AzString) -> Self {
        let mut container_styles = Vec::new();
        
        // === Basis-Styles (alle OS) ===
        container_styles.push(CssPropertyWithConditions::simple(
            CssProperty::const_display(LayoutDisplay::InlineBlock)
        ));
        container_styles.push(CssPropertyWithConditions::simple(
            CssProperty::const_cursor(StyleCursor::Pointer)
        ));
        container_styles.push(CssPropertyWithConditions::simple(
            CssProperty::const_flex_direction(LayoutFlexDirection::Column)
        ));
        container_styles.push(CssPropertyWithConditions::simple(
            CssProperty::const_justify_content(LayoutJustifyContent::Center)
        ));
        
        // === macOS-spezifisch ===
        container_styles.push(CssPropertyWithConditions::conditional(
            CssProperty::const_background_color(RGB_239),
            DynamicSelectorVec::from_vec(vec![DynamicSelector::Os(OsCondition::MacOS)])
        ));
        container_styles.push(CssPropertyWithConditions::conditional(
            CssProperty::const_border_radius(LayoutBorderRadius::px(4.0)),
            DynamicSelectorVec::from_vec(vec![DynamicSelector::Os(OsCondition::MacOS)])
        ));
        container_styles.push(CssPropertyWithConditions::conditional(
            CssProperty::const_padding_top(LayoutPaddingTop::px(5.0)),
            DynamicSelectorVec::from_vec(vec![DynamicSelector::Os(OsCondition::MacOS)])
        ));
        
        // === Windows-spezifisch ===
        container_styles.push(CssPropertyWithConditions::conditional(
            CssProperty::const_background_gradient(WINDOWS_GRADIENT),
            DynamicSelectorVec::from_vec(vec![DynamicSelector::Os(OsCondition::Windows)])
        ));
        container_styles.push(CssPropertyWithConditions::conditional(
            CssProperty::const_border_radius(LayoutBorderRadius::px(2.0)),
            DynamicSelectorVec::from_vec(vec![DynamicSelector::Os(OsCondition::Windows)])
        ));
        
        // === Linux GNOME ===
        container_styles.push(CssPropertyWithConditions::conditional(
            CssProperty::const_background_color(RGB_229),
            DynamicSelectorVec::from_vec(vec![
                DynamicSelector::Os(OsCondition::Linux),
                DynamicSelector::OsVersion(
                    OsVersionCondition::DesktopEnvironment(LinuxDesktopEnv::Gnome)
                )
            ])
        ));
        
        // === Hover-State (alle OS) ===
        container_styles.push(CssPropertyWithConditions::conditional(
            CssProperty::const_background_color(HOVER_COLOR),
            DynamicSelectorVec::from_vec(vec![
                DynamicSelector::PseudoState(PseudoStateType::Hover)
            ])
        ));
        
        // === Responsive @media ===
        // Desktop: größeres Padding
        container_styles.push(CssPropertyWithConditions::conditional(
            CssProperty::const_padding(LayoutPadding::px(10.0)),
            DynamicSelectorVec::from_vec(vec![
                DynamicSelector::ViewportWidth(MinMaxRange::new(Some(768.0), None))
            ])
        ));
        // Mobile: kleineres Padding
        container_styles.push(CssPropertyWithConditions::conditional(
            CssProperty::const_padding(LayoutPadding::px(5.0)),
            DynamicSelectorVec::from_vec(vec![
                DynamicSelector::ViewportWidth(MinMaxRange::new(None, Some(768.0)))
            ])
        ));
        
        // === @container query ===
        // Kleinerer Font in engen Containern
        container_styles.push(CssPropertyWithConditions::conditional(
            CssProperty::const_font_size(StyleFontSize::px(12.0)),
            DynamicSelectorVec::from_vec(vec![
                DynamicSelector::ContainerWidth(MinMaxRange::new(None, Some(200.0)))
            ])
        ));
        
        Self {
            label,
            container_style: CssPropertyWithConditionsVec::from_vec(container_styles),
            ..Default::default()
        }
    }
}
```

---

## 7. Migrations-Strategie

### Phase 1: Infrastruktur (1-2 Tage)
1. ✅ **Button auf `<button>` umstellen** (bereits gemacht)
2. **Neue Typen definieren** (C-kompatibel):
   - `core/src/dynamic_selector.rs` erstellen
   - Alle Enums: `DynamicSelector`, `PseudoStateType` (8 States!), `OsCondition`, etc.
   - `MinMaxRange` struct
   - `CssPropertyWithConditions` in `core/src/style.rs`
3. **SystemStyle-Integration** (KEINE neue OS-Detection!):
   - Konvertierungs-Helfer: `OsCondition::from_system_platform()`
   - Konvertierungs-Helfer: `ThemeCondition::from_system_theme()`
   - Konvertierungs-Helfer: `LinuxDesktopEnv::from_system_desktop_env()`
4. **Window-Config erweitern**:
   - `WindowCreateOptions.force_platform: OptionPlatform` (Developer Override)
   - `WindowCreateOptions.force_desktop_env: OptionDesktopEnvironment`
   - `DynamicSelectorContext::from_window_state()` mit SystemStyle-Parameter

### Phase 2: Core-Integration (2-3 Tage)
1. **API-Änderungen (BREAKING)**:
   - `Dom::with_css_property()` nimmt jetzt `CssPropertyWithConditions` statt `CssProperty`
   - `Dom::with_hover_css_property()` wird INTERN umgeschrieben:
     ```rust
     // ALT: Speichert als NodeDataInlineCssProperty::Hover
     // NEU: Speichert als CssPropertyWithConditions mit PseudoState::Hover Condition
     pub fn with_hover_css_property(mut self, prop: CssProperty) -> Self {
         let conditional = CssPropertyWithConditions::conditional(
             prop,
             DynamicSelectorVec::from_vec(vec![
                 DynamicSelector::PseudoState(PseudoStateType::Hover)
             ])
         );
         self.root.inline_css_props.push(conditional);
         self
     }
     ```
   - **NodeDataInlineCssProperty BLEIBT** für alte Pseudo-State-Logik (deprecated)
2. **`css/src/getters.rs` vereinfachen**:
   - `get_property()`: Nimmt `DynamicSelectorContext` statt `NodeState`
   - **KEINE Specificity** - einfach "last found wins"
   - Rückwärts-Iteration über inline_css_props
   - Alle Aufrufe von `get_property()` aktualisieren (ca. 50+ Stellen)
2. **`StyledNodeState` erweitern**:
   - Fehlende Flags hinzufügen: `disabled`, `checked`, `focus_within`, `visited`
   - `from_inline_css_state()` Konvertierungs-Helfer
3. **`prop_cache.rs` für dynamische Evaluation anpassen**:
   - Cache-Invalidierung bei Context-Änderungen
   - Berücksichtige alle 8 Pseudo-States
4. **Re-Layout-Detection implementieren**:
   - `check_if_relayout_needed()` in Shell-Layer
   - `CssProperty::is_layout_affecting()` Helfer
   - Smart-Invalidierung nur bei echten Layout-Änderungen

### Phase 3: Container-Support (1-2 Tage)
1. **Container-Marker in NodeData**:
   - `NodeData.is_container: bool` Flag
   - `NodeData.container_name: Option<AzString>`
2. **Container-Size-Tracking im Layout**:
   - Nach Layout-Pass: Speichere Rect-Size in NodeData
   - Accessible für Context-Erstellung
3. **Context-Erstellung mit Container-Info**:
   - `find_parent_container()` Walk-Up-Funktion
   - Container-Size in `DynamicSelectorContext` einfügen

### Phase 4: Widget-Migration (1-2 Tage)
1. **Button-Widget mit Multi-OS-Styles**:
   - Alle OS-Varianten als `CssPropertyWithConditions`
   - Hover/Active/Focus/Disabled States mit `PseudoStateType`
   - Responsive Breakpoints (@media)
   - Container-Queries für Font-Size
2. **Andere Widgets migrieren**:
   - TextInput, ScrollBar, CheckBox, etc.
   - Jeweils mit OS-spezifischen Styles
   - Pseudo-States: `:focus`, `:disabled`, `:checked`, `:visited`

### Phase 5: Testing & Validation (1-2 Tage)
1. **Pseudo-State-Tests**:
   - Teste ALLE 8 States: Normal, Hover, Active, Focus, Disabled, Checked, FocusWithin, Visited
   - Verifiziere Priorität: Inline > Stylesheet, Specificity-Sortierung
2. **E2E-Tests mit verschiedenen Breakpoints**:
   - Mobile (320px), Tablet (768px), Desktop (1024px+)
   - Verifiziere Layout-Änderungen vs. reine Repaint
3. **Performance-Tests (Re-Layout-Detection)**:
   - Resize ohne Layout-Änderung → Kein Re-Layout
   - Resize mit Breakpoint-Crossing → Re-Layout
   - Messe Frame-Zeiten
4. **Cross-Platform-Tests**:
   - macOS 13+, 14+, 15+
   - Windows 10, 11
   - Linux (GNOME, KDE, XFCE)
   - Verifiziere OS-spezifische Styles werden korrekt angewendet

### Phase 6: API-Update (1 Tag)
1. **`api.json` aktualisieren**:
   - Neue Enums exportieren
   - `CssPropertyWithConditions` API
   - Builder-Methoden für C/C++/Python
2. **Dokumentation**:
   - Guide: "@media und @container Queries"
   - Guide: "Multi-OS Button Styling"
   - Migration-Guide für bestehenden Code

---

## 8. Performance-Optimierung

### 8.1 Smart Re-Layout Detection

**Schlüsselprinzip**: Nur re-layouten, wenn sich eine **layout-affecting** Property **tatsächlich ändert**.

```rust
// Beispiel-Flow beim Resize:

// 1. Window-Resize von 800px → 900px
// 2. Erstelle alte und neue Contexts
// 3. Iteriere über alle Nodes mit bedingten Styles
// 4. Für jeden Node:
//    - Prüfe: Hat sich eine layout-affecting Property geändert?
//    - Wenn ja: Mark needs relayout
//    - Wenn nein: Mark needs repaint (nur visuelle Änderung)
// 5. Nur bei needs_relayout: Triggere vollständiges Layout
// 6. Sonst: Nur Display-List neu generieren
```

### 8.2 Caching-Strategie

```rust
/// Cache für evaluierte Selektoren
pub struct DynamicSelectorCache {
    /// Map: (NodeId, ContextHash) → MatchedProperties
    cache: FastHashMap<(NodeId, u64), Vec<usize>>,
}

impl DynamicSelectorCache {
    /// Hole gematchte Property-Indices aus Cache
    pub fn get_matched_properties(
        &mut self,
        node_id: NodeId,
        context: &DynamicSelectorContext,
        properties: &CssPropertyWithConditionsVec,
    ) -> Vec<usize> {
        let context_hash = context.cache_key();
        let key = (node_id, context_hash);
        
        if let Some(cached) = self.cache.get(&key) {
            return cached.clone();
        }
        
        // Evaluiere und cache
        let matched: Vec<usize> = properties.as_ref().iter()
            .enumerate()
            .filter(|(_, p)| p.matches(context))
            .map(|(i, _)| i)
            .collect();
        
        self.cache.insert(key, matched.clone());
        matched
    }
    
    /// Invalidiere Cache für bestimmte Selector-Typen
    pub fn invalidate_for_viewport_change(&mut self) {
        // Bei Viewport-Änderung: Nur Entries mit viewport-abhängigen Selektoren entfernen
        self.cache.clear(); // TODO: Selektive Invalidierung
    }
}
```

---

## 9. Beispiel-Flow: Resize-Event

```
┌─────────────────────────────────────────────────────────────┐
│ 1. User resizes window: 800px → 1024px                     │
└─────────────────────────────────────────────────────────────┘
                          ↓
┌─────────────────────────────────────────────────────────────┐
│ 2. Create old context (viewport_width = 800)               │
│    Create new context (viewport_width = 1024)              │
└─────────────────────────────────────────────────────────────┘
                          ↓
┌─────────────────────────────────────────────────────────────┐
│ 3. Check all nodes with CssPropertyWithConditions          │
│    Example Node: Button with padding breakpoint at 768px   │
└─────────────────────────────────────────────────────────────┘
                          ↓
┌─────────────────────────────────────────────────────────────┐
│ 4. Evaluate properties:                                     │
│    - padding: 5px if width < 768px                         │
│    - padding: 10px if width >= 768px                       │
│                                                             │
│    Old context (800px): 10px ✓                             │
│    New context (1024px): 10px ✓                            │
│    → No change, no relayout needed!                        │
└─────────────────────────────────────────────────────────────┘
                          ↓
┌─────────────────────────────────────────────────────────────┐
│ 5. But if window was 700px → 800px:                        │
│    Old: 5px, New: 10px → CHANGED!                          │
│    → is_layout_affecting(padding) = true                   │
│    → TRIGGER RELAYOUT                                       │
└─────────────────────────────────────────────────────────────┘
```

---

## 10. Zusammenfassung & Kritische Änderungen

### Kern-Features:
1. ✅ **C-kompatible Structs** (`repr(C, u8)`, ein Feld pro Variant)
2. ✅ **Smart Re-Layout Detection** (nur bei tatsächlichen Layout-Änderungen)
3. ✅ **Multi-OS Support** (macOS, Windows, Linux mit Desktop-Env)
4. ✅ **@media Queries** (viewport-basiert)
5. ✅ **@container Queries** (parent-basiert)
6. ✅ **Theme Support** (dark/light/custom)
7. ✅ **Accessibility** (reduced-motion, high-contrast)
8. ✅ **Alle 8 Pseudo-States**: Normal, Hover, Active, Focus, Disabled, Checked, FocusWithin, Visited

### Hauptvorteil:
- **Buttons haben alle OS-Styles kompiliert** → Runtime-Switch statt Compile-Time-Switch
- **Keine unnötigen Re-Layouts** → Nur wenn sich layout-relevante Properties ändern
- **Container-aware Components** → Responsive basierend auf Parent, nicht nur Viewport
- **Einheitliche Pseudo-State-Logik** → Alle States über `DynamicSelector::PseudoState`
- **Nutzt bestehendes SystemStyle-System** → Keine Duplikation der OS-Detection
- **Developer-Overrides möglich** → `force_platform` für Testing/Screenshots

### Kritische Änderungen (BREAKING):

#### 1. **`css/src/getters.rs` - Vereinfachte Umschreibung**
- **ALT**: `get_property(node_id, property_type, node_state, ...)`
- **NEU**: `get_property(node_id, property_type, context, ...)`
- **Änderung**: Keine Specificity - einfach "last found wins" mit Rückwärts-Iteration
- **Keine Migration**: BREAKING CHANGE

#### 2. **`Dom::with_css_property()` - API-Änderung**
- **ALT**: `with_css_property(prop: CssProperty)`
- **NEU**: `with_css_property(prop: CssPropertyWithConditions)`
- **Einfacher Wrapper**: `CssPropertyWithConditions::simple(prop)` für Properties ohne Conditions

#### 3. **`Dom::with_hover_css_property()` - Interne Umschreibung**
- **API bleibt gleich**: `with_hover_css_property(prop: CssProperty)`
- **Intern**: Konvertiert zu `CssPropertyWithConditions` mit `PseudoState::Hover` Condition
- **Gleiches gilt für**: `with_active_css_property()`, `with_focus_css_property()`, etc.

#### 3. **`core/src/styled_dom.rs` - StyledNodeState erweitern**
- **ALT**: `struct StyledNodeState { hover, active, focused }`
- **NEU**: `struct StyledNodeState { hover, active, focused, disabled, checked, focus_within, visited }`
- **Alle 8 States**: Müssen in Hit-Test und Event-Handling gesetzt werden

#### 4. **Alle `get_property()` Aufrufe (50+ Stellen)**
- Überall wo `get_property()` aufgerufen wird: Context erstellen
- Context braucht: OS-Info, Viewport, Container, Theme, Node-State (alle 8 Flags!)

### Pseudo-State Priority (NEU):
- **ALT**: Hardcodiert Focus > Active > Hover > Normal
- **NEU**: Specificity-basiert
  - Inline Properties: Base 1000
  - Jede Condition: +10
  - Beispiel: `:hover` auf Inline = 1010, `:hover:focus` = 1020
  - → Automatisch korrekte Priorität durch Sortierung

### Konfliktauflösung - "Last Wins" Strategy:

**Regel**: Wenn mehrere Properties mit gleichem Type und gleichen Conditions existieren → letzte gefundene gewinnt.

```rust
// Beispiel:
Dom::div()
    .with_css_property(CssPropertyWithConditions::simple(
        CssProperty::BackgroundColor(ColorU::RED)
    ))
    .with_css_property(CssPropertyWithConditions::simple(
        CssProperty::BackgroundColor(ColorU::BLUE)  // <- Diese gewinnt!
    ))
```

**In `getters.rs`**:
```rust
// Rückwärts-Iteration = Letzte zuerst
for prop in node_data.inline_css_props.iter().rev() {
    if prop.property.get_type() == property_type && prop.matches(context) {
        return Some(prop.property.clone()); // Erste Match = Letzte im Array
    }
}
```

### Keine Backward-Kompatibilität:
- **BREAKING CHANGE**: Alle `with_css_property()` APIs nehmen jetzt `CssPropertyWithConditions`
- `NodeDataInlineCssProperty` bleibt UNVERÄNDERT (für alte State-Logik)
- User-Code muss angepasst werden:
  ```rust
  // ALT:
  .with_css_property(CssProperty::Width(px(100)))
  
  // NEU:
  .with_css_property(CssPropertyWithConditions::simple(
      CssProperty::Width(px(100))
  ))
  ```

### Geschätzte Zeitaufwand:
- **Phase 1** (Infrastruktur): 1-2 Tage
- **Phase 2** (getters.rs + Core): 2-3 Tage ← **KRITISCH**
- **Phase 3** (Container): 1-2 Tage
- **Phase 4** (Widgets): 1-2 Tage
- **Phase 5** (Tests): 1-2 Tage
- **Phase 6** (API/Docs): 1 Tag
- **TOTAL**: 8-12 Tage
