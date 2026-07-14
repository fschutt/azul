//! CSS properties for CSS Grid layout.

use alloc::{
    boxed::Box,
    string::{String, ToString},
    vec::Vec,
};

use crate::{
    corety::AzString,
    codegen::format::FormatAsRustCode,
    impl_vec, impl_vec_clone, impl_vec_debug, impl_vec_eq, impl_vec_hash, impl_vec_mut,
    impl_vec_ord, impl_vec_partialeq, impl_vec_partialord,
    props::{basic::pixel::PixelValue, formatter::PrintAsCssValue},
};

// --- grid-template-columns / grid-template-rows ---

/// Wrapper for minmax(min, max) to satisfy repr(C) (enum variants can only have 1 field)
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct GridMinMax {
    pub min: Box<GridTrackSizing>,
    pub max: Box<GridTrackSizing>,
}

impl core::fmt::Debug for GridMinMax {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "minmax({}, {})",
            self.min.print_as_css_value(),
            self.max.print_as_css_value()
        )
    }
}

/// Represents a single track sizing function for grid
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C, u8)]
#[derive(Default)]
pub enum GridTrackSizing {
    /// Fixed pixel/percent size
    Fixed(PixelValue),
    /// fr units (value multiplied by `FR_SCALING_FACTOR` to allow fractional
    /// values while satisfying Eq/Ord/Hash — e.g. `1fr` = `Fr(100)`, `0.5fr` = `Fr(50)`)
    Fr(i32),
    /// min-content
    MinContent,
    /// max-content
    MaxContent,
    /// auto
    #[default]
    Auto,
    /// minmax(min, max) - uses `GridMinMax` which contains Box<GridTrackSizing> for each bound
    MinMax(GridMinMax),
    /// fit-content(size)
    FitContent(PixelValue),
}

impl_option!(
    GridTrackSizing,
    OptionGridTrackSizing,
    copy = false,
    [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

impl core::fmt::Debug for GridTrackSizing {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.print_as_css_value())
    }
}


impl PrintAsCssValue for GridTrackSizing {
    fn print_as_css_value(&self) -> String {
        match self {
            Self::Fixed(px) => px.print_as_css_value(),
            Self::Fr(f) => format!("{f}fr"),
            Self::MinContent => "min-content".to_string(),
            Self::MaxContent => "max-content".to_string(),
            Self::Auto => "auto".to_string(),
            Self::MinMax(minmax) => {
                format!(
                    "minmax({}, {})",
                    minmax.min.print_as_css_value(),
                    minmax.max.print_as_css_value()
                )
            }
            Self::FitContent(size) => {
                format!("fit-content({})", size.print_as_css_value())
            }
        }
    }
}

// C-compatible Vec for GridTrackSizing
impl_vec!(GridTrackSizing, GridTrackSizingVec, GridTrackSizingVecDestructor, GridTrackSizingVecDestructorType, GridTrackSizingVecSlice, OptionGridTrackSizing);
impl_vec_clone!(
    GridTrackSizing,
    GridTrackSizingVec,
    GridTrackSizingVecDestructor
);
impl_vec_debug!(GridTrackSizing, GridTrackSizingVec);
impl_vec_partialeq!(GridTrackSizing, GridTrackSizingVec);
impl_vec_eq!(GridTrackSizing, GridTrackSizingVec);
impl_vec_partialord!(GridTrackSizing, GridTrackSizingVec);
impl_vec_ord!(GridTrackSizing, GridTrackSizingVec);
impl_vec_hash!(GridTrackSizing, GridTrackSizingVec);
impl_vec_mut!(GridTrackSizing, GridTrackSizingVec);

/// Represents `grid-template-columns` or `grid-template-rows`
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct GridTemplate {
    pub tracks: GridTrackSizingVec,
}

impl core::fmt::Debug for GridTemplate {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.print_as_css_value())
    }
}

impl Default for GridTemplate {
    fn default() -> Self {
        Self {
            tracks: GridTrackSizingVec::from_vec(Vec::new()),
        }
    }
}

impl PrintAsCssValue for GridTemplate {
    fn print_as_css_value(&self) -> String {
        let tracks_slice = self.tracks.as_ref();
        if tracks_slice.is_empty() {
            "none".to_string()
        } else {
            tracks_slice
                .iter()
                .map(PrintAsCssValue::print_as_css_value)
                .collect::<Vec<_>>()
                .join(" ")
        }
    }
}

// --- grid-auto-columns / grid-auto-rows ---

/// Represents `grid-auto-columns` or `grid-auto-rows`
/// Structurally identical to `GridTemplate` but semantically different
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct GridAutoTracks {
    pub tracks: GridTrackSizingVec,
}

impl core::fmt::Debug for GridAutoTracks {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.print_as_css_value())
    }
}

impl Default for GridAutoTracks {
    fn default() -> Self {
        Self {
            tracks: GridTrackSizingVec::from_vec(Vec::new()),
        }
    }
}

impl PrintAsCssValue for GridAutoTracks {
    fn print_as_css_value(&self) -> String {
        let tracks_slice = self.tracks.as_ref();
        if tracks_slice.is_empty() {
            "auto".to_string()
        } else {
            tracks_slice
                .iter()
                .map(PrintAsCssValue::print_as_css_value)
                .collect::<Vec<_>>()
                .join(" ")
        }
    }
}

impl From<GridTemplate> for GridAutoTracks {
    fn from(template: GridTemplate) -> Self {
        Self {
            tracks: template.tracks,
        }
    }
}

// --- grid-row / grid-column (grid line placement) ---

/// Named grid line with optional span count (FFI-safe wrapper)
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct NamedGridLine {
    pub grid_line_name: AzString,
    /// Span count, 0 means no span specified
    pub span_count: i32,
}

impl NamedGridLine {
    #[must_use] pub fn create(name: AzString, span: Option<i32>) -> Self {
        Self {
            grid_line_name: name,
            span_count: span.unwrap_or(0),
        }
    }

    #[must_use] pub const fn span(&self) -> Option<i32> {
        if self.span_count == 0 {
            None
        } else {
            Some(self.span_count)
        }
    }
}
#[allow(variant_size_differences)] // repr(C,u8) FFI enum: boxing the large variant would change the C ABI (api.json bindings); size disparity accepted
/// Represents a grid line position (start or end)
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C, u8)]
#[derive(Default)]
pub enum GridLine {
    /// auto
    #[default]
    Auto,
    /// Line number (1-based, negative for counting from end)
    Line(i32),
    /// Named line with optional span count
    Named(NamedGridLine),
    /// span N
    Span(i32),
}

impl core::fmt::Debug for GridLine {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.print_as_css_value())
    }
}


impl PrintAsCssValue for GridLine {
    fn print_as_css_value(&self) -> String {
        match self {
            Self::Auto => "auto".to_string(),
            Self::Line(n) => n.to_string(),
            Self::Named(named) => {
                if named.span_count == 0 {
                    named.grid_line_name.as_str().to_string()
                } else {
                    format!("{} {}", named.grid_line_name.as_str(), named.span_count)
                }
            }
            Self::Span(n) => format!("span {n}"),
        }
    }
}

/// Represents `grid-row` or `grid-column` (start / end)
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct GridPlacement {
    pub grid_start: GridLine,
    pub grid_end: GridLine,
}

impl core::fmt::Debug for GridPlacement {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.print_as_css_value())
    }
}

impl Default for GridPlacement {
    fn default() -> Self {
        Self {
            grid_start: GridLine::Auto,
            grid_end: GridLine::Auto,
        }
    }
}

impl PrintAsCssValue for GridPlacement {
    fn print_as_css_value(&self) -> String {
        if self.grid_end == GridLine::Auto {
            self.grid_start.print_as_css_value()
        } else {
            format!(
                "{} / {}",
                self.grid_start.print_as_css_value(),
                self.grid_end.print_as_css_value()
            )
        }
    }
}

#[cfg(feature = "parser")]
#[derive(Clone, PartialEq, Eq)]
pub enum GridParseError<'a> {
    InvalidValue(&'a str),
}

#[cfg(feature = "parser")]
impl_debug_as_display!(GridParseError<'a>);
#[cfg(feature = "parser")]
impl_display! { GridParseError<'a>, {
    InvalidValue(e) => format!("Invalid grid value: \"{}\"", e),
}}

#[cfg(feature = "parser")]
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum GridParseErrorOwned {
    InvalidValue(AzString),
}

#[cfg(feature = "parser")]
impl GridParseError<'_> {
    #[must_use] pub fn to_contained(&self) -> GridParseErrorOwned {
        match self {
            GridParseError::InvalidValue(s) => GridParseErrorOwned::InvalidValue((*s).to_string().into()),
        }
    }
}

#[cfg(feature = "parser")]
impl GridParseErrorOwned {
    #[must_use] pub fn to_shared(&self) -> GridParseError<'_> {
        match self {
            Self::InvalidValue(s) => GridParseError::InvalidValue(s.as_str()),
        }
    }
}

#[cfg(feature = "parser")]
fn split_respecting_parens(input: &str) -> Result<Vec<String>, ()> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut paren_depth: i32 = 0;

    for ch in input.chars() {
        match ch {
            '(' => { paren_depth += 1; current.push(ch); }
            ')' => { paren_depth -= 1; if paren_depth < 0 { return Err(()); } current.push(ch); }
            ' ' if paren_depth == 0 => {
                if !current.trim().is_empty() {
                    parts.push(current.trim().to_string());
                    current.clear();
                }
            }
            _ => current.push(ch),
        }
    }
    if !current.trim().is_empty() {
        parts.push(current.trim().to_string());
    }
    Ok(parts)
}

#[cfg(feature = "parser")]
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `grid-template` value.
pub fn parse_grid_template(input: &str) -> Result<GridTemplate, GridParseError<'_>> {
    use crate::props::basic::pixel::parse_pixel_value;

    let input = input.trim();

    if input == "none" {
        return Ok(GridTemplate::default());
    }

    let parts = split_respecting_parens(input)
        .map_err(|()| GridParseError::InvalidValue(input))?;

    let mut tracks = Vec::new();
    for part in &parts {
        parse_grid_track_or_repeat(part, &mut tracks)
            .map_err(|()| GridParseError::InvalidValue(input))?;
    }

    Ok(GridTemplate {
        tracks: GridTrackSizingVec::from_vec(tracks),
    })
}

/// Parse a single grid track token, which may be `repeat(N, track)` or a plain track.
/// For `repeat(N, track_list)`, the tracks are expanded inline.
#[cfg(feature = "parser")]
fn parse_grid_track_or_repeat(input: &str, tracks: &mut Vec<GridTrackSizing>) -> Result<(), ()> {
    // Maximum repeat count accepted in `repeat(N, …)` to bound expansion.
    const MAX_GRID_REPEAT_COUNT: usize = 10_000;
    let input = input.trim();

    // Handle repeat(N, track_list)
    if input.starts_with("repeat(") && input.ends_with(')') {
        let content = &input[7..input.len() - 1];
        // Find the first comma that separates the count from the track list
        let comma_pos = content.find(',').ok_or(())?;
        let count_str = content[..comma_pos].trim();
        let track_list_str = content[comma_pos + 1..].trim();

        let count: usize = count_str.parse().map_err(|_| ())?;
        if count == 0 || count > MAX_GRID_REPEAT_COUNT {
            return Err(());
        }

        // Parse the track list (may contain multiple space-separated tracks)
        let parts = split_respecting_parens(track_list_str)?;
        let repeat_tracks: Vec<GridTrackSizing> = parts
            .iter()
            .map(|p| parse_grid_track_owned(p))
            .collect::<Result<Vec<_>, _>>()?;

        // Expand: repeat N times
        for _ in 0..count {
            tracks.extend(repeat_tracks.iter().cloned());
        }
        return Ok(());
    }

    // Plain single track
    tracks.push(parse_grid_track_owned(input)?);
    Ok(())
}

#[cfg(feature = "parser")]
fn parse_grid_track_owned(input: &str) -> Result<GridTrackSizing, ()> {
    use crate::props::basic::pixel::parse_pixel_value;

    let input = input.trim();

    if input == "auto" {
        return Ok(GridTrackSizing::Auto);
    }

    if input == "min-content" {
        return Ok(GridTrackSizing::MinContent);
    }

    if input == "max-content" {
        return Ok(GridTrackSizing::MaxContent);
    }

    if let Some(num_str) = input.strip_suffix("fr") {
        /// Fr values are stored as integers scaled by this factor (e.g. `1fr` = 100, `0.5fr` = 50).
        const FR_SCALING_FACTOR: f32 = 100.0;
        let num_str = num_str.trim();
        if let Ok(num) = num_str.parse::<f32>() {
            let scaled = num * FR_SCALING_FACTOR;
            if scaled.is_nan() || scaled < crate::cast::i32_to_f32(i32::MIN) || scaled > crate::cast::i32_to_f32(i32::MAX) {
                return Err(());
            }
            return Ok(GridTrackSizing::Fr(crate::cast::f32_to_i32(scaled)));
        }
        return Err(());
    }

    if input.starts_with("minmax(") && input.ends_with(')') {
        let content = &input[7..input.len() - 1];
        let parts: Vec<&str> = content.split(',').collect();
        if parts.len() == 2 {
            let min = parse_grid_track_owned(parts[0].trim())?;
            let max = parse_grid_track_owned(parts[1].trim())?;
            return Ok(GridTrackSizing::MinMax(GridMinMax {
                min: Box::new(min),
                max: Box::new(max),
            }));
        }
        return Err(());
    }

    if input.starts_with("fit-content(") && input.ends_with(')') {
        let size_str = &input[12..input.len() - 1].trim();
        if let Ok(size) = parse_pixel_value(size_str) {
            return Ok(GridTrackSizing::FitContent(size));
        }
        return Err(());
    }

    // Try to parse as pixel value
    if let Ok(px) = parse_pixel_value(input) {
        return Ok(GridTrackSizing::Fixed(px));
    }

    Err(())
}

#[cfg(feature = "parser")]
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `grid-placement` value.
pub fn parse_grid_placement(input: &str) -> Result<GridPlacement, GridParseError<'_>> {
    let input = input.trim();

    if input == "auto" {
        return Ok(GridPlacement::default());
    }

    // Split by "/"
    let parts: Vec<&str> = input.split('/').map(str::trim).collect();

    let grid_start =
        parse_grid_line_owned(parts[0]).map_err(|()| GridParseError::InvalidValue(input))?;
    let grid_end = if parts.len() > 1 {
        parse_grid_line_owned(parts[1]).map_err(|()| GridParseError::InvalidValue(input))?
    } else {
        GridLine::Auto
    };

    Ok(GridPlacement {
        grid_start,
        grid_end,
    })
}

// --- grid-auto-flow ---

/// Represents the `grid-auto-flow` property
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
#[derive(Default)]
pub enum LayoutGridAutoFlow {
    #[default]
    Row,
    Column,
    RowDense,
    ColumnDense,
}


impl PrintAsCssValue for LayoutGridAutoFlow {
    fn print_as_css_value(&self) -> alloc::string::String {
        match self {
            Self::Row => "row".to_string(),
            Self::Column => "column".to_string(),
            Self::RowDense => "row dense".to_string(),
            Self::ColumnDense => "column dense".to_string(),
        }
    }
}

#[cfg(feature = "parser")]
#[derive(Clone, PartialEq, Eq)]
pub enum GridAutoFlowParseError<'a> {
    InvalidValue(&'a str),
}

#[cfg(feature = "parser")]
impl_debug_as_display!(GridAutoFlowParseError<'a>);
#[cfg(feature = "parser")]
impl_display! { GridAutoFlowParseError<'a>, {
    InvalidValue(e) => format!("Invalid grid-auto-flow value: \"{}\"", e),
}}

#[cfg(feature = "parser")]
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum GridAutoFlowParseErrorOwned {
    InvalidValue(AzString),
}

#[cfg(feature = "parser")]
impl GridAutoFlowParseError<'_> {
    #[must_use] pub fn to_contained(&self) -> GridAutoFlowParseErrorOwned {
        match self {
            GridAutoFlowParseError::InvalidValue(s) => {
                GridAutoFlowParseErrorOwned::InvalidValue((*s).to_string().into())
            }
        }
    }
}

#[cfg(feature = "parser")]
impl GridAutoFlowParseErrorOwned {
    #[must_use] pub fn to_shared(&self) -> GridAutoFlowParseError<'_> {
        match self {
            Self::InvalidValue(s) => {
                GridAutoFlowParseError::InvalidValue(s.as_str())
            }
        }
    }
}

#[cfg(feature = "parser")]
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `grid-auto-flow` value.
pub fn parse_layout_grid_auto_flow(
    input: &str,
) -> Result<LayoutGridAutoFlow, GridAutoFlowParseError<'_>> {
    match input.trim() {
        "row" => Ok(LayoutGridAutoFlow::Row),
        "column" => Ok(LayoutGridAutoFlow::Column),
        "row dense" | "dense" => Ok(LayoutGridAutoFlow::RowDense),
        "column dense" => Ok(LayoutGridAutoFlow::ColumnDense),
        _ => Err(GridAutoFlowParseError::InvalidValue(input)),
    }
}

// --- justify-self / justify-items ---

/// Represents `justify-self` for grid items
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
#[derive(Default)]
pub enum LayoutJustifySelf {
    #[default]
    Auto,
    Start,
    End,
    Center,
    Stretch,
}


impl PrintAsCssValue for LayoutJustifySelf {
    fn print_as_css_value(&self) -> alloc::string::String {
        match self {
            Self::Auto => "auto".to_string(),
            Self::Start => "start".to_string(),
            Self::End => "end".to_string(),
            Self::Center => "center".to_string(),
            Self::Stretch => "stretch".to_string(),
        }
    }
}

#[cfg(feature = "parser")]
#[derive(Clone, PartialEq, Eq)]
pub enum JustifySelfParseError<'a> {
    InvalidValue(&'a str),
}

#[cfg(feature = "parser")]
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum JustifySelfParseErrorOwned {
    InvalidValue(AzString),
}

#[cfg(feature = "parser")]
impl JustifySelfParseError<'_> {
    #[must_use] pub fn to_contained(&self) -> JustifySelfParseErrorOwned {
        match self {
            JustifySelfParseError::InvalidValue(s) => {
                JustifySelfParseErrorOwned::InvalidValue((*s).to_string().into())
            }
        }
    }
}

#[cfg(feature = "parser")]
impl JustifySelfParseErrorOwned {
    #[must_use] pub fn to_shared(&self) -> JustifySelfParseError<'_> {
        match self {
            Self::InvalidValue(s) => {
                JustifySelfParseError::InvalidValue(s.as_str())
            }
        }
    }
}

#[cfg(feature = "parser")]
impl_debug_as_display!(JustifySelfParseError<'a>);
#[cfg(feature = "parser")]
impl_display! { JustifySelfParseError<'a>, {
    InvalidValue(e) => format!("Invalid justify-self value: \"{}\"", e),
}}

#[cfg(feature = "parser")]
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `justify-self` value.
pub fn parse_layout_justify_self(
    input: &str,
) -> Result<LayoutJustifySelf, JustifySelfParseError<'_>> {
    match input.trim() {
        "auto" => Ok(LayoutJustifySelf::Auto),
        "start" | "flex-start" => Ok(LayoutJustifySelf::Start),
        "end" | "flex-end" => Ok(LayoutJustifySelf::End),
        "center" => Ok(LayoutJustifySelf::Center),
        "stretch" => Ok(LayoutJustifySelf::Stretch),
        _ => Err(JustifySelfParseError::InvalidValue(input)),
    }
}

/// Represents `justify-items` for grid containers
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
#[derive(Default)]
pub enum LayoutJustifyItems {
    Start,
    End,
    Center,
    #[default]
    Stretch,
}


impl PrintAsCssValue for LayoutJustifyItems {
    fn print_as_css_value(&self) -> alloc::string::String {
        match self {
            Self::Start => "start".to_string(),
            Self::End => "end".to_string(),
            Self::Center => "center".to_string(),
            Self::Stretch => "stretch".to_string(),
        }
    }
}

#[cfg(feature = "parser")]
#[derive(Clone, PartialEq, Eq)]
pub enum JustifyItemsParseError<'a> {
    InvalidValue(&'a str),
}

#[cfg(feature = "parser")]
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum JustifyItemsParseErrorOwned {
    InvalidValue(AzString),
}

#[cfg(feature = "parser")]
impl JustifyItemsParseError<'_> {
    #[must_use] pub fn to_contained(&self) -> JustifyItemsParseErrorOwned {
        match self {
            JustifyItemsParseError::InvalidValue(s) => {
                JustifyItemsParseErrorOwned::InvalidValue((*s).to_string().into())
            }
        }
    }
}

#[cfg(feature = "parser")]
impl JustifyItemsParseErrorOwned {
    #[must_use] pub fn to_shared(&self) -> JustifyItemsParseError<'_> {
        match self {
            Self::InvalidValue(s) => {
                JustifyItemsParseError::InvalidValue(s.as_str())
            }
        }
    }
}

#[cfg(feature = "parser")]
impl_debug_as_display!(JustifyItemsParseError<'a>);
#[cfg(feature = "parser")]
impl_display! { JustifyItemsParseError<'a>, {
    InvalidValue(e) => format!("Invalid justify-items value: \"{}\"", e),
}}

#[cfg(feature = "parser")]
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `justify-items` value.
pub fn parse_layout_justify_items(
    input: &str,
) -> Result<LayoutJustifyItems, JustifyItemsParseError<'_>> {
    match input.trim() {
        "start" => Ok(LayoutJustifyItems::Start),
        "end" => Ok(LayoutJustifyItems::End),
        "center" => Ok(LayoutJustifyItems::Center),
        "stretch" => Ok(LayoutJustifyItems::Stretch),
        _ => Err(JustifyItemsParseError::InvalidValue(input)),
    }
}

// --- gap (single value type) ---

#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct LayoutGap {
    pub inner: PixelValue,
}

impl core::fmt::Debug for LayoutGap {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.inner)
    }
}

impl PrintAsCssValue for LayoutGap {
    fn print_as_css_value(&self) -> alloc::string::String {
        self.inner.print_as_css_value()
    }
}

// Implement FormatAsRustCode for the new types so they can be emitted by the
// code generator.
impl FormatAsRustCode for LayoutGridAutoFlow {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        format!(
            "LayoutGridAutoFlow::{}",
            match self {
                Self::Row => "Row",
                Self::Column => "Column",
                Self::RowDense => "RowDense",
                Self::ColumnDense => "ColumnDense",
            }
        )
    }
}

impl FormatAsRustCode for LayoutJustifySelf {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        format!(
            "LayoutJustifySelf::{}",
            match self {
                Self::Auto => "Auto",
                Self::Start => "Start",
                Self::End => "End",
                Self::Center => "Center",
                Self::Stretch => "Stretch",
            }
        )
    }
}

impl FormatAsRustCode for LayoutJustifyItems {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        format!(
            "LayoutJustifyItems::{}",
            match self {
                Self::Start => "Start",
                Self::End => "End",
                Self::Center => "Center",
                Self::Stretch => "Stretch",
            }
        )
    }
}

impl FormatAsRustCode for LayoutGap {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        use crate::codegen::format::format_pixel_value;
        format!("LayoutGap {{ inner: {} }}", format_pixel_value(&self.inner))
    }
}

impl FormatAsRustCode for GridTrackSizing {
    // `tabs` is required by the FormatAsRustCode trait signature; this variant only
    // threads it through to nested MinMax children, never reading it locally.
    #[allow(clippy::only_used_in_recursion)]
    fn format_as_rust_code(&self, tabs: usize) -> String {
        use crate::codegen::format::format_pixel_value;
        match self {
            Self::Fixed(pv) => {
                format!("GridTrackSizing::Fixed({})", format_pixel_value(pv))
            }
            Self::Fr(f) => format!("GridTrackSizing::Fr({f})"),
            Self::MinContent => "GridTrackSizing::MinContent".to_string(),
            Self::MaxContent => "GridTrackSizing::MaxContent".to_string(),
            Self::Auto => "GridTrackSizing::Auto".to_string(),
            Self::MinMax(minmax) => {
                format!(
                    "GridTrackSizing::MinMax(GridMinMax {{ min: Box::new({}), max: Box::new({}) }})",
                    minmax.min.format_as_rust_code(tabs),
                    minmax.max.format_as_rust_code(tabs)
                )
            }
            Self::FitContent(pv) => {
                format!("GridTrackSizing::FitContent({})", format_pixel_value(pv))
            }
        }
    }
}

impl FormatAsRustCode for GridAutoTracks {
    fn format_as_rust_code(&self, tabs: usize) -> String {
        let tracks: Vec<String> = self
            .tracks
            .as_ref()
            .iter()
            .map(|t| t.format_as_rust_code(tabs))
            .collect();
        format!(
            "GridAutoTracks {{ tracks: GridTrackSizingVec::from_vec(vec![{}]) }}",
            tracks.join(", ")
        )
    }
}

impl FormatAsRustCode for GridTemplateAreas {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        format!("GridTemplateAreas {{ areas: GridAreaDefinitionVec::from_vec(vec!{:?}) }}", self.areas.as_ref())
    }
}

#[cfg(feature = "parser")]
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `gap` value.
pub fn parse_layout_gap(
    input: &str,
) -> Result<LayoutGap, crate::props::basic::pixel::CssPixelValueParseError<'_>> {
    crate::props::basic::pixel::parse_pixel_value(input).map(|p| LayoutGap { inner: p })
}

#[cfg(feature = "parser")]
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `grid-line-owned` value.
pub fn parse_grid_line_owned(input: &str) -> Result<GridLine, ()> {
    let input = input.trim();

    if input == "auto" {
        return Ok(GridLine::Auto);
    }

    if let Some(num_str) = input.strip_prefix("span ") {
        let num_str = num_str.trim();
        if let Ok(num) = num_str.parse::<i32>() {
            return Ok(GridLine::Span(num));
        }
        return Err(());
    }

    // Try to parse as line number
    if let Ok(num) = input.parse::<i32>() {
        return Ok(GridLine::Line(num));
    }

    // Otherwise treat as named line
    Ok(GridLine::Named(NamedGridLine::create(
        input.to_string().into(),
        None,
    )))
}

#[cfg(all(test, feature = "parser"))]
mod tests {
    use super::*;

    // Grid template tests
    #[test]
    fn test_parse_grid_template_none() {
        let result = parse_grid_template("none").unwrap();
        assert_eq!(result.tracks.len(), 0);
    }

    #[test]
    fn test_parse_grid_template_single_px() {
        let result = parse_grid_template("100px").unwrap();
        assert_eq!(result.tracks.len(), 1);
        assert!(matches!(
            result.tracks.as_ref()[0],
            GridTrackSizing::Fixed(_)
        ));
    }

    #[test]
    fn test_parse_grid_template_multiple_tracks() {
        let result = parse_grid_template("100px 200px 1fr").unwrap();
        assert_eq!(result.tracks.len(), 3);
    }

    #[test]
    fn test_parse_grid_template_fr_units() {
        let result = parse_grid_template("1fr 2fr 1fr").unwrap();
        assert_eq!(result.tracks.len(), 3);
        assert!(matches!(
            result.tracks.as_ref()[0],
            GridTrackSizing::Fr(100)
        ));
        assert!(matches!(
            result.tracks.as_ref()[1],
            GridTrackSizing::Fr(200)
        ));
    }

    #[test]
    fn test_parse_grid_template_fractional_fr() {
        let result = parse_grid_template("0.5fr 1.5fr").unwrap();
        assert_eq!(result.tracks.len(), 2);
        assert!(matches!(result.tracks.as_ref()[0], GridTrackSizing::Fr(50)));
        assert!(matches!(
            result.tracks.as_ref()[1],
            GridTrackSizing::Fr(150)
        ));
    }

    #[test]
    fn test_parse_grid_template_auto() {
        let result = parse_grid_template("auto 100px auto").unwrap();
        assert_eq!(result.tracks.len(), 3);
        assert!(matches!(result.tracks.as_ref()[0], GridTrackSizing::Auto));
        assert!(matches!(result.tracks.as_ref()[2], GridTrackSizing::Auto));
    }

    #[test]
    fn test_parse_grid_template_min_max_content() {
        let result = parse_grid_template("min-content max-content auto").unwrap();
        assert_eq!(result.tracks.len(), 3);
        assert!(matches!(
            result.tracks.as_ref()[0],
            GridTrackSizing::MinContent
        ));
        assert!(matches!(
            result.tracks.as_ref()[1],
            GridTrackSizing::MaxContent
        ));
    }

    #[test]
    fn test_parse_grid_template_minmax() {
        let result = parse_grid_template("minmax(100px, 1fr)").unwrap();
        assert_eq!(result.tracks.len(), 1);
        assert!(matches!(
            result.tracks.as_ref()[0],
            GridTrackSizing::MinMax(_)
        ));
    }

    #[test]
    fn test_parse_grid_template_minmax_complex() {
        let result = parse_grid_template("minmax(min-content, max-content)").unwrap();
        assert_eq!(result.tracks.len(), 1);
    }

    #[test]
    fn test_parse_grid_template_fit_content() {
        let result = parse_grid_template("fit-content(200px)").unwrap();
        assert_eq!(result.tracks.len(), 1);
        assert!(matches!(
            result.tracks.as_ref()[0],
            GridTrackSizing::FitContent(_)
        ));
    }

    #[test]
    fn test_parse_grid_template_mixed() {
        let result = parse_grid_template("100px minmax(100px, 1fr) auto 2fr").unwrap();
        assert_eq!(result.tracks.len(), 4);
    }

    #[test]
    fn test_parse_grid_template_percent() {
        let result = parse_grid_template("25% 50% 25%").unwrap();
        assert_eq!(result.tracks.len(), 3);
    }

    #[test]
    fn test_parse_grid_template_em_units() {
        let result = parse_grid_template("10em 20em 1fr").unwrap();
        assert_eq!(result.tracks.len(), 3);
    }

    // Grid placement tests
    #[test]
    fn test_parse_grid_placement_auto() {
        let result = parse_grid_placement("auto").unwrap();
        assert!(matches!(result.grid_start, GridLine::Auto));
        assert!(matches!(result.grid_end, GridLine::Auto));
    }

    #[test]
    fn test_parse_grid_placement_line_number() {
        let result = parse_grid_placement("1").unwrap();
        assert!(matches!(result.grid_start, GridLine::Line(1)));
        assert!(matches!(result.grid_end, GridLine::Auto));
    }

    #[test]
    fn test_parse_grid_placement_negative_line() {
        let result = parse_grid_placement("-1").unwrap();
        assert!(matches!(result.grid_start, GridLine::Line(-1)));
    }

    #[test]
    fn test_parse_grid_placement_span() {
        let result = parse_grid_placement("span 2").unwrap();
        assert!(matches!(result.grid_start, GridLine::Span(2)));
    }

    #[test]
    fn test_parse_grid_placement_start_end() {
        let result = parse_grid_placement("1 / 3").unwrap();
        assert!(matches!(result.grid_start, GridLine::Line(1)));
        assert!(matches!(result.grid_end, GridLine::Line(3)));
    }

    #[test]
    fn test_parse_grid_placement_span_end() {
        let result = parse_grid_placement("1 / span 2").unwrap();
        assert!(matches!(result.grid_start, GridLine::Line(1)));
        assert!(matches!(result.grid_end, GridLine::Span(2)));
    }

    #[test]
    fn test_parse_grid_placement_named_line() {
        let result = parse_grid_placement("header-start").unwrap();
        assert!(matches!(result.grid_start, GridLine::Named(_)));
    }

    #[test]
    fn test_parse_grid_placement_named_start_end() {
        let result = parse_grid_placement("header-start / header-end").unwrap();
        assert!(matches!(result.grid_start, GridLine::Named(_)));
        assert!(matches!(result.grid_end, GridLine::Named(_)));
    }

    // Edge cases
    #[test]
    fn test_parse_grid_template_whitespace() {
        let result = parse_grid_template("  100px   200px  ").unwrap();
        assert_eq!(result.tracks.len(), 2);
    }

    #[test]
    fn test_parse_grid_placement_whitespace() {
        let result = parse_grid_placement("  1  /  3  ").unwrap();
        assert!(matches!(result.grid_start, GridLine::Line(1)));
        assert!(matches!(result.grid_end, GridLine::Line(3)));
    }

    #[test]
    fn test_parse_grid_template_zero_fr() {
        let result = parse_grid_template("0fr").unwrap();
        assert!(matches!(result.tracks.as_ref()[0], GridTrackSizing::Fr(0)));
    }

    #[test]
    fn test_parse_grid_placement_zero_line() {
        let result = parse_grid_placement("0").unwrap();
        assert!(matches!(result.grid_start, GridLine::Line(0)));
    }

    // repeat() tests
    #[test]
    fn test_parse_grid_template_repeat_fr() {
        let result = parse_grid_template("repeat(3, 1fr)").unwrap();
        assert_eq!(result.tracks.len(), 3);
        assert!(matches!(result.tracks.as_ref()[0], GridTrackSizing::Fr(100)));
        assert!(matches!(result.tracks.as_ref()[1], GridTrackSizing::Fr(100)));
        assert!(matches!(result.tracks.as_ref()[2], GridTrackSizing::Fr(100)));
    }

    #[test]
    fn test_parse_grid_template_repeat_px() {
        let result = parse_grid_template("repeat(2, 100px)").unwrap();
        assert_eq!(result.tracks.len(), 2);
        assert!(matches!(result.tracks.as_ref()[0], GridTrackSizing::Fixed(_)));
        assert!(matches!(result.tracks.as_ref()[1], GridTrackSizing::Fixed(_)));
    }

    #[test]
    fn test_parse_grid_template_repeat_multiple_tracks() {
        // repeat(2, 100px 1fr) should expand to [100px, 1fr, 100px, 1fr]
        let result = parse_grid_template("repeat(2, 100px 1fr)").unwrap();
        assert_eq!(result.tracks.len(), 4);
        assert!(matches!(result.tracks.as_ref()[0], GridTrackSizing::Fixed(_)));
        assert!(matches!(result.tracks.as_ref()[1], GridTrackSizing::Fr(100)));
        assert!(matches!(result.tracks.as_ref()[2], GridTrackSizing::Fixed(_)));
        assert!(matches!(result.tracks.as_ref()[3], GridTrackSizing::Fr(100)));
    }

    #[test]
    fn test_parse_grid_template_repeat_with_other_tracks() {
        // "100px repeat(2, 1fr) auto" should produce [100px, 1fr, 1fr, auto]
        let result = parse_grid_template("100px repeat(2, 1fr) auto").unwrap();
        assert_eq!(result.tracks.len(), 4);
        assert!(matches!(result.tracks.as_ref()[0], GridTrackSizing::Fixed(_)));
        assert!(matches!(result.tracks.as_ref()[1], GridTrackSizing::Fr(100)));
        assert!(matches!(result.tracks.as_ref()[2], GridTrackSizing::Fr(100)));
        assert!(matches!(result.tracks.as_ref()[3], GridTrackSizing::Auto));
    }

    #[test]
    fn test_parse_grid_template_repeat_minmax() {
        let result = parse_grid_template("repeat(3, minmax(100px, 1fr))").unwrap();
        assert_eq!(result.tracks.len(), 3);
        assert!(matches!(result.tracks.as_ref()[0], GridTrackSizing::MinMax(_)));
        assert!(matches!(result.tracks.as_ref()[1], GridTrackSizing::MinMax(_)));
        assert!(matches!(result.tracks.as_ref()[2], GridTrackSizing::MinMax(_)));
    }
}

// --- grid-template-areas ---

/// A single named grid area with its row/column bounds (1-based grid line numbers).
/// This matches taffy's `GridTemplateArea<String>`.
#[repr(C)]
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GridAreaDefinition {
    pub name: AzString,
    pub row_start: u16,
    pub row_end: u16,
    pub column_start: u16,
    pub column_end: u16,
}

impl_option!(
    GridAreaDefinition,
    OptionGridAreaDefinition,
    copy = false,
    [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

impl_vec!(GridAreaDefinition, GridAreaDefinitionVec, GridAreaDefinitionVecDestructor, GridAreaDefinitionVecDestructorType, GridAreaDefinitionVecSlice, OptionGridAreaDefinition);
impl_vec_clone!(
    GridAreaDefinition,
    GridAreaDefinitionVec,
    GridAreaDefinitionVecDestructor
);
impl_vec_debug!(GridAreaDefinition, GridAreaDefinitionVec);
impl_vec_partialeq!(GridAreaDefinition, GridAreaDefinitionVec);
impl_vec_eq!(GridAreaDefinition, GridAreaDefinitionVec);
impl_vec_partialord!(GridAreaDefinition, GridAreaDefinitionVec);
impl_vec_ord!(GridAreaDefinition, GridAreaDefinitionVec);
impl_vec_hash!(GridAreaDefinition, GridAreaDefinitionVec);
impl_vec_mut!(GridAreaDefinition, GridAreaDefinitionVec);

/// Represents the parsed value of `grid-template-areas`.
///
/// Example CSS:
/// ```css
/// grid-template-areas:
///     "header header header"
///     "sidebar main aside"
///     "footer footer footer";
/// ```
#[repr(C)]
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GridTemplateAreas {
    pub areas: GridAreaDefinitionVec,
}

impl Default for GridTemplateAreas {
    fn default() -> Self {
        Self { areas: GridAreaDefinitionVec::from_vec(Vec::new()) }
    }
}

impl PrintAsCssValue for GridTemplateAreas {
    fn print_as_css_value(&self) -> String {
        let areas_slice = self.areas.as_ref();
        if areas_slice.is_empty() {
            return "none".to_string();
        }
        // Reconstruct the row strings from the area definitions
        let max_row = areas_slice.iter().map(|a| a.row_end).max().unwrap_or(1);
        let max_col = areas_slice.iter().map(|a| a.column_end).max().unwrap_or(1);
        let num_rows = (max_row - 1) as usize;
        let num_cols = (max_col - 1) as usize;
        let mut grid: Vec<Vec<String>> = vec![vec![".".to_string(); num_cols]; num_rows];
        for area in areas_slice {
            let row_start = area.row_start as usize - 1;
            let row_end = area.row_end as usize - 1;
            let col_start = area.column_start as usize - 1;
            let col_end = area.column_end as usize - 1;
            for row in grid.iter_mut().take(row_end).skip(row_start) {
                for cell in row.iter_mut().take(col_end).skip(col_start) {
                    *cell = area.name.as_str().to_string();
                }
            }
        }
        grid.iter()
            .map(|row| format!("\"{}\"", row.join(" ")))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// Parse `grid-template-areas` CSS value.
///
/// Accepts quoted row strings like:
///   `"header header header" "sidebar main aside" "footer footer footer"`
///
/// Returns a `GridTemplateAreas` with deduplicated named areas and their
/// computed row/column line boundaries (1-based, as taffy expects).
#[cfg(feature = "parser")]
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `grid-template-areas` value.
pub fn parse_grid_template_areas(input: &str) -> Result<GridTemplateAreas, ()> {
    use alloc::collections::BTreeMap;
    let input = input.trim();
    if input == "none" {
        return Ok(GridTemplateAreas::default());
    }

    // Extract quoted strings: each one is a row
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut i = 0;
    let bytes = input.as_bytes();
    while i < bytes.len() {
        if bytes[i] == b'"' || bytes[i] == b'\'' {
            let quote = bytes[i];
            i += 1;
            let start = i;
            while i < bytes.len() && bytes[i] != quote {
                i += 1;
            }
            if i >= bytes.len() {
                return Err(());
            }
            let row_str = &input[start..i];
            let cells: Vec<String> = row_str.split_whitespace().map(std::string::ToString::to_string).collect();
            if cells.is_empty() {
                return Err(());
            }
            rows.push(cells);
        }
        // advance past the closing quote (quoted branch) or the current char (else)
        i += 1;
    }

    if rows.is_empty() {
        return Err(());
    }

    // Validate: all rows must have the same number of columns
    let num_cols = rows[0].len();
    for row in &rows {
        if row.len() != num_cols {
            return Err(());
        }
    }

    // Build area map: name -> (min_row, max_row, min_col, max_col) in 0-based indices
    let mut area_map: BTreeMap<String, (usize, usize, usize, usize)> = BTreeMap::new();

    for (row_idx, row) in rows.iter().enumerate() {
        for (col_idx, cell) in row.iter().enumerate() {
            if cell == "." {
                continue; // skip null cell tokens
            }
            let entry = area_map.entry(cell.clone()).or_insert((row_idx, row_idx, col_idx, col_idx));
            entry.0 = entry.0.min(row_idx);
            entry.1 = entry.1.max(row_idx);
            entry.2 = entry.2.min(col_idx);
            entry.3 = entry.3.max(col_idx);
        }
    }

    // Convert to 1-based grid line numbers (taffy convention)
    let mut areas = Vec::new();
    for (name, (min_row, max_row, min_col, max_col)) in area_map {
        areas.push(GridAreaDefinition {
            name: name.into(),
            row_start: u16::try_from(min_row + 1).unwrap_or(u16::MAX),
            row_end: u16::try_from(max_row + 2).unwrap_or(u16::MAX), // end line is one past the last cell
            column_start: u16::try_from(min_col + 1).unwrap_or(u16::MAX),
            column_end: u16::try_from(max_col + 2).unwrap_or(u16::MAX),
        });
    }

    Ok(GridTemplateAreas { areas: GridAreaDefinitionVec::from_vec(areas) })
}

#[cfg(all(test, feature = "parser"))]
mod autotest_generated {
    use super::*;

    // Every assertion below pins the *observed* behaviour of the code as it stands.
    // Where that behaviour deviates from the CSS Grid spec or loses information, the
    // test is named `..._is_lax` / `..._is_lossy` / `..._saturates` and carries a
    // BUG/DEVIATION comment. Those comments are the deliverable: they mark the places
    // where a fix would have to change the assertion, not the code under test.

    // ---------------------------------------------------------------------
    // NamedGridLine::create / NamedGridLine::span
    // ---------------------------------------------------------------------

    #[test]
    fn named_grid_line_span_roundtrips_every_nonzero_i32() {
        for span in [1_i32, -1, 7, -7, i32::MAX, i32::MIN, i32::MIN + 1] {
            let line = NamedGridLine::create("area".to_string().into(), Some(span));
            assert_eq!(line.span_count, span);
            assert_eq!(line.span(), Some(span), "span {span} must survive create()");
        }
    }

    #[test]
    fn named_grid_line_span_of_some_zero_is_lossy() {
        // BUG (encoding collision): `span_count == 0` is the sentinel for "no span",
        // so an explicitly requested `Some(0)` is indistinguishable from `None` on
        // the way out. `create(_, Some(0)).span()` should arguably be `Some(0)` or
        // `create` should reject 0; today it silently becomes `None`.
        let explicit_zero = NamedGridLine::create("a".to_string().into(), Some(0));
        let absent = NamedGridLine::create("a".to_string().into(), None);

        assert_eq!(explicit_zero.span(), None);
        assert_eq!(absent.span(), None);
        assert_eq!(explicit_zero, absent, "Some(0) and None collapse to the same value");
    }

    #[test]
    fn named_grid_line_create_accepts_empty_and_unicode_names() {
        let empty = NamedGridLine::create(String::new().into(), None);
        assert_eq!(empty.grid_line_name.as_str(), "");
        assert_eq!(empty.span(), None);

        let emoji = NamedGridLine::create("\u{1F600}\u{0301}".to_string().into(), Some(3));
        assert_eq!(emoji.grid_line_name.as_str(), "\u{1F600}\u{0301}");
        assert_eq!(emoji.span(), Some(3));
    }

    #[test]
    fn named_grid_line_span_on_an_extreme_instance_does_not_panic() {
        let huge_name = "x".repeat(100_000);
        let line = NamedGridLine::create(huge_name.clone().into(), Some(i32::MIN));
        assert_eq!(line.span(), Some(i32::MIN));
        assert_eq!(line.grid_line_name.as_str().len(), huge_name.len());
    }

    // ---------------------------------------------------------------------
    // split_respecting_parens (private)
    // ---------------------------------------------------------------------

    #[test]
    fn split_respecting_parens_empty_and_whitespace_yield_ok_empty_not_err() {
        // DEVIATION: the adversarial expectation is Err/None for empty input, but this
        // helper reports "no tokens" as `Ok(vec![])`. That is what makes
        // `parse_grid_template("")` succeed (see the parse_grid_template tests below).
        assert_eq!(split_respecting_parens(""), Ok(Vec::new()));
        assert_eq!(split_respecting_parens("   "), Ok(Vec::new()));
        assert_eq!(split_respecting_parens(" \t\n "), Ok(Vec::new()));
    }

    #[test]
    fn split_respecting_parens_valid_minimal_and_nested_calls() {
        assert_eq!(
            split_respecting_parens("100px 1fr"),
            Ok(vec!["100px".to_string(), "1fr".to_string()])
        );
        // The whole point of the helper: spaces inside parens are NOT separators.
        assert_eq!(
            split_respecting_parens("repeat(2, 100px 1fr) auto"),
            Ok(vec!["repeat(2, 100px 1fr)".to_string(), "auto".to_string()])
        );
        assert_eq!(
            split_respecting_parens("a(b c)d e"),
            Ok(vec!["a(b c)d".to_string(), "e".to_string()])
        );
    }

    #[test]
    fn split_respecting_parens_rejects_unbalanced_close_paren() {
        assert_eq!(split_respecting_parens(")"), Err(()));
        assert_eq!(split_respecting_parens("a)b"), Err(()));
        assert_eq!(split_respecting_parens("(a))"), Err(()));
        assert_eq!(split_respecting_parens(")("), Err(()));
    }

    #[test]
    fn split_respecting_parens_accepts_unbalanced_open_paren_is_lax() {
        // Asymmetry: a stray ')' is an error, a stray '(' is not — the depth counter is
        // never checked at end-of-input. The malformed token is handed downstream, where
        // the track parser happens to reject it, so nothing unsound escapes.
        assert_eq!(split_respecting_parens("((("), Ok(vec!["(((".to_string()]));
        assert_eq!(
            split_respecting_parens("repeat(2, 1fr"),
            Ok(vec!["repeat(2, 1fr".to_string()])
        );
        assert!(parse_grid_template("repeat(2, 1fr").is_err());
    }

    #[test]
    fn split_respecting_parens_does_not_treat_tab_or_newline_as_a_separator() {
        // BUG (CSS whitespace): only U+0020 splits tokens. CSS treats \t, \n, \r and \f
        // as whitespace too, so a multi-line `grid-template-columns` declaration is
        // mis-tokenised into one giant token.
        assert_eq!(
            split_respecting_parens("100px\t200px"),
            Ok(vec!["100px\t200px".to_string()])
        );
        assert_eq!(
            split_respecting_parens("100px\n200px"),
            Ok(vec!["100px\n200px".to_string()])
        );
        // ...and the consequence, one layer up:
        assert!(parse_grid_template("100px\t200px").is_err());
        assert!(parse_grid_template("100px\n200px").is_err());
        // Whereas the space-separated form is fine.
        assert_eq!(parse_grid_template("100px 200px").unwrap().tracks.len(), 2);
    }

    #[test]
    fn split_respecting_parens_handles_multibyte_unicode() {
        // char-based iteration, so no byte-boundary slicing hazard.
        assert_eq!(
            split_respecting_parens("\u{1F600} e\u{0301}"),
            Ok(vec!["\u{1F600}".to_string(), "e\u{0301}".to_string()])
        );
        assert_eq!(
            split_respecting_parens("\u{1F600}(\u{4E2D} \u{6587})"),
            Ok(vec!["\u{1F600}(\u{4E2D} \u{6587})".to_string()])
        );
    }

    #[test]
    fn split_respecting_parens_survives_a_million_chars_and_deep_nesting() {
        // 1M-char single token: linear scan, must not hang.
        let long = "a".repeat(1_000_000);
        let parts = split_respecting_parens(&long).unwrap();
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0].len(), 1_000_000);

        // 50k space-separated tokens.
        let many = "1fr ".repeat(50_000);
        assert_eq!(split_respecting_parens(&many).unwrap().len(), 50_000);

        // 10k nested parens: the scanner is iterative, so no stack overflow, and the
        // balanced nest is returned as a single (garbage) token that parsing rejects.
        let nested = format!("{}{}", "(".repeat(10_000), ")".repeat(10_000));
        assert_eq!(split_respecting_parens(&nested).unwrap().len(), 1);
        assert!(parse_grid_template(&nested).is_err());
    }

    // ---------------------------------------------------------------------
    // parse_grid_track_owned (private)
    // ---------------------------------------------------------------------

    #[test]
    fn parse_grid_track_owned_empty_and_whitespace_are_err() {
        assert_eq!(parse_grid_track_owned(""), Err(()));
        assert_eq!(parse_grid_track_owned("   "), Err(()));
        assert_eq!(parse_grid_track_owned("\t\n"), Err(()));
    }

    #[test]
    fn parse_grid_track_owned_valid_minimal_keywords() {
        assert_eq!(parse_grid_track_owned("auto"), Ok(GridTrackSizing::Auto));
        assert_eq!(parse_grid_track_owned("min-content"), Ok(GridTrackSizing::MinContent));
        assert_eq!(parse_grid_track_owned("max-content"), Ok(GridTrackSizing::MaxContent));
        assert_eq!(
            parse_grid_track_owned("  auto  "),
            Ok(GridTrackSizing::Auto),
            "leading/trailing whitespace is trimmed"
        );
        // DEVIATION: CSS keywords are ASCII case-insensitive; this parser is not.
        assert_eq!(parse_grid_track_owned("AUTO"), Err(()));
        assert_eq!(parse_grid_track_owned("Min-Content"), Err(()));
    }

    #[test]
    fn parse_grid_track_owned_fr_is_scaled_by_100_and_truncates() {
        assert_eq!(parse_grid_track_owned("1fr"), Ok(GridTrackSizing::Fr(100)));
        assert_eq!(parse_grid_track_owned("0fr"), Ok(GridTrackSizing::Fr(0)));
        assert_eq!(parse_grid_track_owned("-0fr"), Ok(GridTrackSizing::Fr(0)));
        assert_eq!(parse_grid_track_owned("+1fr"), Ok(GridTrackSizing::Fr(100)));
        assert_eq!(parse_grid_track_owned("0.5fr"), Ok(GridTrackSizing::Fr(50)));

        // Truncation, not rounding: anything below 0.01fr collapses to 0fr, which
        // taffy reads as "take no free space at all".
        assert_eq!(parse_grid_track_owned("0.005fr"), Ok(GridTrackSizing::Fr(0)));
        assert_eq!(parse_grid_track_owned("1.999fr"), Ok(GridTrackSizing::Fr(199)));

        // DEVIATION: CSS forbids negative <flex> values; this accepts them.
        assert_eq!(parse_grid_track_owned("-1fr"), Ok(GridTrackSizing::Fr(-100)));
    }

    #[test]
    fn parse_grid_track_owned_fr_rejects_nan_inf_and_out_of_range() {
        // The guard in parse_grid_track_owned checks is_nan() and the i32 bounds
        // *before* casting, so none of these can produce a garbage saturated Fr.
        for bad in [
            "NaNfr", "nanfr", "inffr", "-inffr", "infinityfr", "1e8fr", "-1e8fr",
            "1e30fr", "-1e30fr", "340282350000000000000000000000000000000fr",
        ] {
            assert_eq!(parse_grid_track_owned(bad), Err(()), "{bad:?} must be rejected");
        }

        // Largest values that still fit: 1e7fr * 100 == 1e9, exactly representable in f32.
        assert_eq!(parse_grid_track_owned("1e7fr"), Ok(GridTrackSizing::Fr(1_000_000_000)));
        assert_eq!(parse_grid_track_owned("-1e7fr"), Ok(GridTrackSizing::Fr(-1_000_000_000)));
    }

    #[test]
    fn parse_grid_track_owned_bare_fr_suffix_is_err() {
        assert_eq!(parse_grid_track_owned("fr"), Err(()));
        assert_eq!(parse_grid_track_owned("  fr"), Err(()));
        assert_eq!(parse_grid_track_owned("xfr"), Err(()));
    }

    #[test]
    fn parse_grid_track_owned_minmax_comma_split_is_paren_unaware() {
        assert_eq!(
            parse_grid_track_owned("minmax(100px, 1fr)"),
            Ok(GridTrackSizing::MinMax(GridMinMax {
                min: Box::new(GridTrackSizing::Fixed(PixelValue::px(100.0))),
                max: Box::new(GridTrackSizing::Fr(100)),
            }))
        );
        // fit-content nests fine (it has no top-level comma)...
        assert!(matches!(
            parse_grid_track_owned("minmax(fit-content(10px), 2px)"),
            Ok(GridTrackSizing::MinMax(_))
        ));
        // ...but anything with an inner comma blows the `split(',')` arity check.
        // DEVIATION: `minmax(minmax(1px,2px), 3px)` is legal-ish CSS shape-wise and is
        // rejected here purely because the comma split ignores parens. It fails closed
        // (Err, not a mis-parse), and it is also what bounds recursion depth to 2.
        assert_eq!(parse_grid_track_owned("minmax(minmax(1px,2px), 3px)"), Err(()));
        assert_eq!(parse_grid_track_owned("minmax(1px,2px,3px)"), Err(()));
        assert_eq!(parse_grid_track_owned("minmax(1px)"), Err(()));
        assert_eq!(parse_grid_track_owned("minmax()"), Err(()));
    }

    #[test]
    fn parse_grid_track_owned_deeply_nested_minmax_cannot_stack_overflow() {
        // 10k nested minmax: rejected at the arity check on the first level, so the
        // recursive descent never actually descends. Guards against a regression that
        // makes the comma split paren-aware without adding a depth limit.
        let deep = format!("{}1px{}", "minmax(".repeat(10_000), ")".repeat(10_000));
        assert_eq!(parse_grid_track_owned(&deep), Err(()));

        let deep_fit = format!("{}1px{}", "fit-content(".repeat(10_000), ")".repeat(10_000));
        assert_eq!(parse_grid_track_owned(&deep_fit), Err(()));
    }

    #[test]
    fn parse_grid_track_owned_unclosed_and_truncated_funcs_are_err() {
        // The `&input[7..len-1]` / `&input[12..len-1]` slices are only reached when both
        // the prefix and the ')' suffix match, which makes len >= 8 / >= 13. These inputs
        // probe every shape near that boundary for an out-of-bounds slice panic.
        for bad in [
            "minmax(", "minmax", "minmax)", "fit-content(", "fit-content",
            "fit-content)", "repeat(", ")", "(", "()", "fit-content()",
        ] {
            assert_eq!(parse_grid_track_owned(bad), Err(()), "{bad:?}");
        }
    }

    #[test]
    fn parse_grid_track_owned_fit_content_and_pixel_fallback() {
        assert_eq!(
            parse_grid_track_owned("fit-content(200px)"),
            Ok(GridTrackSizing::FitContent(PixelValue::px(200.0)))
        );
        assert_eq!(
            parse_grid_track_owned("100px"),
            Ok(GridTrackSizing::Fixed(PixelValue::px(100.0)))
        );
        assert_eq!(
            parse_grid_track_owned("25%"),
            Ok(GridTrackSizing::Fixed(PixelValue::percent(25.0)))
        );
        // DEVIATION (inherited from parse_pixel_value): a unitless number is accepted
        // and silently means px.
        assert_eq!(
            parse_grid_track_owned("100"),
            Ok(GridTrackSizing::Fixed(PixelValue::px(100.0)))
        );
    }

    #[test]
    fn parse_grid_track_owned_pixel_fallback_swallows_nan_and_inf() {
        // BUG (inherited from parse_pixel_value + FloatValue::new): the fr path guards
        // against NaN/inf, the *pixel* path does not. "NaN" is silently coerced to 0px
        // and "inf" saturates to isize::MAX rather than being rejected.
        assert_eq!(
            parse_grid_track_owned("NaNpx"),
            Ok(GridTrackSizing::Fixed(PixelValue::px(0.0))),
            "NaN silently becomes 0px"
        );
        assert_eq!(
            parse_grid_track_owned("inf"),
            Ok(GridTrackSizing::Fixed(PixelValue::px(f32::INFINITY))),
            "inf is accepted and saturates"
        );
        assert_eq!(
            parse_grid_track_owned("1e40px"),
            Ok(GridTrackSizing::Fixed(PixelValue::px(f32::INFINITY)))
        );
    }

    #[test]
    fn parse_grid_track_owned_garbage_and_long_input_never_panic() {
        for bad in [
            "!!!", ";;;", "\u{1F600}", "e\u{0301}\u{0301}", "\0", "100px;garbage",
            "auto;", "1fr 1fr", "--var(x)", "calc(1px + 1px)",
        ] {
            assert_eq!(parse_grid_track_owned(bad), Err(()), "{bad:?}");
        }
        // 1M chars of a non-numeric token: rejected fast, no hang.
        assert_eq!(parse_grid_track_owned(&"a".repeat(1_000_000)), Err(()));
    }

    // ---------------------------------------------------------------------
    // parse_grid_track_or_repeat (private)
    // ---------------------------------------------------------------------

    #[test]
    fn parse_grid_track_or_repeat_valid_minimal() {
        let mut tracks = Vec::new();
        assert_eq!(parse_grid_track_or_repeat("repeat(2, 1fr)", &mut tracks), Ok(()));
        assert_eq!(tracks, vec![GridTrackSizing::Fr(100), GridTrackSizing::Fr(100)]);

        // Plain (non-repeat) tracks are appended to whatever is already there.
        assert_eq!(parse_grid_track_or_repeat("auto", &mut tracks), Ok(()));
        assert_eq!(tracks.len(), 3);
        assert_eq!(tracks[2], GridTrackSizing::Auto);
    }

    #[test]
    fn parse_grid_track_or_repeat_count_is_bounded_at_10_000() {
        let mut tracks = Vec::new();
        assert_eq!(parse_grid_track_or_repeat("repeat(10000, 1fr)", &mut tracks), Ok(()));
        assert_eq!(tracks.len(), 10_000, "the documented maximum is accepted");

        // One past the cap, zero, negative, and a count that overflows usize.
        for bad in [
            "repeat(10001, 1fr)",
            "repeat(0, 1fr)",
            "repeat(-1, 1fr)",
            "repeat(18446744073709551616, 1fr)",
            "repeat(99999999999999999999999999, 1fr)",
            "repeat(1.5, 1fr)",
            "repeat(NaN, 1fr)",
            "repeat(inf, 1fr)",
            "repeat(auto-fill, 1fr)",
        ] {
            let mut t = Vec::new();
            assert_eq!(parse_grid_track_or_repeat(bad, &mut t), Err(()), "{bad:?}");
        }
    }

    #[test]
    fn parse_grid_track_or_repeat_expansion_is_amplified_by_the_track_list_length() {
        // The 10_000 cap bounds the *count*, not the expansion: `repeat(N, <k tracks>)`
        // yields N*k tracks. 10 tracks x 10_000 => 100_000 entries from a 60-byte input.
        // Not unsound, but worth pinning — the real bound on memory is N * k, and k is
        // limited only by the length of the declaration.
        let mut tracks = Vec::new();
        let input = format!("repeat(10000, {})", "1fr ".repeat(10).trim());
        assert_eq!(parse_grid_track_or_repeat(&input, &mut tracks), Ok(()));
        assert_eq!(tracks.len(), 100_000);
        assert!(tracks.iter().all(|t| *t == GridTrackSizing::Fr(100)));
    }

    #[test]
    fn parse_grid_track_or_repeat_with_an_empty_track_list_is_lax() {
        // DEVIATION: `repeat(2, )` has nothing to repeat. It is accepted and contributes
        // zero tracks instead of being rejected.
        let mut tracks = Vec::new();
        assert_eq!(parse_grid_track_or_repeat("repeat(2, )", &mut tracks), Ok(()));
        assert!(tracks.is_empty());
        assert_eq!(parse_grid_template("repeat(2, )").unwrap().tracks.len(), 0);
    }

    #[test]
    fn parse_grid_track_or_repeat_rejects_nested_repeat_and_malformed_shapes() {
        for bad in [
            "repeat(2, repeat(2, 1fr))", // repeat does not recurse
            "repeat(2)",                 // no comma
            "repeat()",
            "repeat(2, 1fr", // unterminated -> falls through to the plain-track path
            "",
            "   ",
            "\u{1F600}",
            "repeat(2, garbage)",
        ] {
            let mut t = Vec::new();
            assert_eq!(parse_grid_track_or_repeat(bad, &mut t), Err(()), "{bad:?}");
        }
    }

    #[test]
    fn parse_grid_track_or_repeat_appends_nothing_when_it_fails() {
        // Atomicity invariant: on Err the caller's Vec must be left exactly as it was.
        // The repeat path validates the entire track list (collect::<Result<Vec<_>,_>>)
        // *before* it expands, and the plain path propagates with `?` before pushing —
        // so a half-expanded repeat can never leak into the caller's tracks.
        let mut tracks = vec![GridTrackSizing::Auto];
        for bad in [
            "repeat(2, 1fr garbage)", // fails on the 2nd track of the list
            "repeat(3, 1fr) trailing", // not a single token: rejected as a plain track
            "repeat(0, 1fr)",
            "repeat(2, 1fr", // unterminated
            "garbage",
        ] {
            assert_eq!(parse_grid_track_or_repeat(bad, &mut tracks), Err(()), "{bad:?}");
            assert_eq!(
                tracks,
                vec![GridTrackSizing::Auto],
                "{bad:?} mutated the caller's Vec despite returning Err"
            );
        }
    }

    // ---------------------------------------------------------------------
    // parse_grid_template
    // ---------------------------------------------------------------------

    #[test]
    fn parse_grid_template_empty_and_whitespace_are_ok_empty_is_lax() {
        // DEVIATION: `grid-template-columns: ;` is not valid CSS, but empty / whitespace
        // input yields `Ok` with zero tracks — silently identical to `none`.
        for input in ["", "   ", "\t\n", "  \r\n  "] {
            let parsed = parse_grid_template(input)
                .unwrap_or_else(|e| panic!("{input:?} unexpectedly failed: {e:?}"));
            assert_eq!(parsed.tracks.len(), 0, "{input:?}");
            assert_eq!(parsed, GridTemplate::default(), "{input:?}");
        }
    }

    #[test]
    fn parse_grid_template_none_is_case_sensitive() {
        assert_eq!(parse_grid_template("none").unwrap(), GridTemplate::default());
        assert_eq!(parse_grid_template("  none  ").unwrap(), GridTemplate::default());
        // DEVIATION: CSS keywords are ASCII case-insensitive.
        assert!(parse_grid_template("NONE").is_err());
        assert!(parse_grid_template("None").is_err());
    }

    #[test]
    fn parse_grid_template_garbage_is_err_and_reports_the_trimmed_input() {
        let err = parse_grid_template("  !!! garbage  ").unwrap_err();
        assert_eq!(
            err,
            GridParseError::InvalidValue("!!! garbage"),
            "the error borrows the trimmed input, not the raw one"
        );
        assert_eq!(format!("{err}"), "Invalid grid value: \"!!! garbage\"");

        for bad in [
            ")", "a)b", "100px;200px", "1 fr", "100px, 200px", "\u{1F600}",
            "100px \u{1F600}", "\0", "calc(100px)",
        ] {
            assert!(parse_grid_template(bad).is_err(), "{bad:?} must be rejected");
        }
    }

    #[test]
    fn parse_grid_template_boundary_numbers_do_not_overflow() {
        // fr guards against NaN/inf/out-of-range...
        for bad in ["NaNfr", "inffr", "-inffr", "1e30fr"] {
            assert!(parse_grid_template(bad).is_err(), "{bad:?}");
        }
        // ...the pixel path saturates instead (see parse_grid_track_owned tests).
        assert_eq!(
            parse_grid_template("1e40px").unwrap().tracks.as_ref()[0],
            GridTrackSizing::Fixed(PixelValue::px(f32::INFINITY))
        );
        // i64::MAX as a bare number: parsed as an f32 px value, no integer overflow.
        assert!(parse_grid_template("9223372036854775807").is_ok());
        assert_eq!(
            parse_grid_template("0px -0px 0fr").unwrap().tracks.len(),
            3,
            "zero and negative-zero are accepted"
        );
    }

    #[test]
    fn parse_grid_template_extremely_long_input_terminates() {
        // 50k tokens.
        let many = "1fr ".repeat(50_000);
        assert_eq!(parse_grid_template(&many).unwrap().tracks.len(), 50_000);
        // 1M-char single garbage token: rejected without hanging.
        assert!(parse_grid_template(&"a".repeat(1_000_000)).is_err());
    }

    #[test]
    fn parse_grid_template_deeply_nested_parens_do_not_stack_overflow() {
        let nested = format!("{}1px{}", "(".repeat(10_000), ")".repeat(10_000));
        assert!(parse_grid_template(&nested).is_err());
        // Unbalanced in the *other* direction is caught by split_respecting_parens.
        assert!(parse_grid_template(&")".repeat(10_000)).is_err());
    }

    // ---------------------------------------------------------------------
    // parse_grid_line_owned
    // ---------------------------------------------------------------------

    #[test]
    fn parse_grid_line_owned_valid_minimal() {
        assert_eq!(parse_grid_line_owned("auto"), Ok(GridLine::Auto));
        assert_eq!(parse_grid_line_owned("1"), Ok(GridLine::Line(1)));
        assert_eq!(parse_grid_line_owned("-1"), Ok(GridLine::Line(-1)));
        assert_eq!(parse_grid_line_owned("+1"), Ok(GridLine::Line(1)));
        assert_eq!(parse_grid_line_owned("0"), Ok(GridLine::Line(0)));
        assert_eq!(parse_grid_line_owned("-0"), Ok(GridLine::Line(0)));
        assert_eq!(parse_grid_line_owned("span 2"), Ok(GridLine::Span(2)));
        assert_eq!(parse_grid_line_owned("span   2"), Ok(GridLine::Span(2)));
    }

    #[test]
    fn parse_grid_line_owned_i32_boundaries_saturate_into_a_named_line() {
        assert_eq!(parse_grid_line_owned("2147483647"), Ok(GridLine::Line(i32::MAX)));
        assert_eq!(parse_grid_line_owned("-2147483648"), Ok(GridLine::Line(i32::MIN)));

        // BUG (silent reinterpretation): one past the i32 range, the integer parse fails
        // and the input falls through to the "named line" catch-all — so `grid-row:
        // 2147483648` becomes a *named* line called "2147483648" instead of an error.
        // A typo'd or overflowing line number is silently accepted as a name.
        assert_eq!(
            parse_grid_line_owned("2147483648"),
            Ok(GridLine::Named(NamedGridLine::create("2147483648".to_string().into(), None)))
        );
        assert_eq!(
            parse_grid_line_owned("-2147483649"),
            Ok(GridLine::Named(NamedGridLine::create("-2147483649".to_string().into(), None)))
        );

        // The `span` path has no such fallback: it fails closed.
        assert_eq!(parse_grid_line_owned("span 2147483648"), Err(()));
        assert_eq!(parse_grid_line_owned("span -2147483649"), Err(()));
        assert_eq!(parse_grid_line_owned("span abc"), Err(()));

        // ...unless the input trims down to bare "span": the leading `trim()` eats the
        // trailing space, `strip_prefix("span ")` then misses, and `span` with no count
        // (invalid CSS) becomes a grid line *named* "span" instead of an error.
        assert_eq!(
            parse_grid_line_owned("span "),
            Ok(GridLine::Named(NamedGridLine::create("span".to_string().into(), None)))
        );
    }

    #[test]
    fn parse_grid_line_owned_span_accepts_zero_and_negative_is_lax() {
        // DEVIATION: CSS requires `span <integer [1,∞]>`. Both of these are invalid CSS
        // and both are accepted here; taffy gets a nonsensical span.
        assert_eq!(parse_grid_line_owned("span 0"), Ok(GridLine::Span(0)));
        assert_eq!(parse_grid_line_owned("span -5"), Ok(GridLine::Span(-5)));
        assert_eq!(parse_grid_line_owned("span -2147483648"), Ok(GridLine::Span(i32::MIN)));
    }

    #[test]
    fn parse_grid_line_owned_never_errs_on_garbage_it_names_it() {
        // BUG (unbounded catch-all): every input that is not `auto` / an i32 / a bad
        // `span` becomes a named grid line. Empty string, punctuation, emoji, an entire
        // stylesheet — all `Ok`. This is why parse_grid_placement effectively cannot
        // reject anything (see below).
        for garbage in ["", "   ", "!!!", ";;;", "\u{1F600}", "\0", "span", "100px", "1 2"] {
            let parsed = parse_grid_line_owned(garbage)
                .unwrap_or_else(|_| panic!("{garbage:?} unexpectedly errored"));
            assert!(
                matches!(parsed, GridLine::Named(_)),
                "{garbage:?} became {parsed:?}, expected a Named catch-all"
            );
        }
        assert_eq!(
            parse_grid_line_owned("   "),
            Ok(GridLine::Named(NamedGridLine::create(String::new().into(), None))),
            "whitespace-only trims to an EMPTY named line"
        );
        // No panic on a huge name.
        assert!(parse_grid_line_owned(&"x".repeat(100_000)).is_ok());
    }

    // ---------------------------------------------------------------------
    // parse_grid_placement
    // ---------------------------------------------------------------------

    #[test]
    fn parse_grid_placement_valid_minimal() {
        assert_eq!(parse_grid_placement("auto").unwrap(), GridPlacement::default());
        assert_eq!(
            parse_grid_placement("1 / 3").unwrap(),
            GridPlacement { grid_start: GridLine::Line(1), grid_end: GridLine::Line(3) }
        );
        assert_eq!(
            parse_grid_placement("1/3").unwrap(),
            GridPlacement { grid_start: GridLine::Line(1), grid_end: GridLine::Line(3) },
            "the slash does not need surrounding spaces"
        );
        assert_eq!(
            parse_grid_placement("  1  /  span 2  ").unwrap(),
            GridPlacement { grid_start: GridLine::Line(1), grid_end: GridLine::Span(2) }
        );
    }

    #[test]
    fn parse_grid_placement_extra_slash_segments_are_silently_dropped() {
        // DEVIATION: `1 / 2 / 3` is not valid CSS. Only parts[0] and parts[1] are read;
        // everything after the second slash is discarded without an error.
        assert_eq!(
            parse_grid_placement("1 / 2 / 3").unwrap(),
            GridPlacement { grid_start: GridLine::Line(1), grid_end: GridLine::Line(2) }
        );
        assert_eq!(
            parse_grid_placement("1 / 2 / 3 / 4 / garbage").unwrap(),
            GridPlacement { grid_start: GridLine::Line(1), grid_end: GridLine::Line(2) }
        );
    }

    #[test]
    fn parse_grid_placement_empty_and_garbage_are_ok_named_is_lax() {
        // BUG: consequence of the parse_grid_line_owned catch-all — `grid-row: ;` and
        // `grid-row: !!!;` both parse successfully into a named line. There is no
        // input-validation value in the Result at all except via the `span` path.
        let empty = parse_grid_placement("").unwrap();
        assert_eq!(
            empty,
            GridPlacement {
                grid_start: GridLine::Named(NamedGridLine::create(String::new().into(), None)),
                grid_end: GridLine::Auto,
            },
            "empty input should be Err, but yields a Named(\"\") start"
        );

        for garbage in ["   ", "!!!", "\u{1F600}", "/", "///", "\0"] {
            assert!(parse_grid_placement(garbage).is_ok(), "{garbage:?}");
        }

        // The ONLY rejection path: a malformed `span`.
        let err = parse_grid_placement("  span x  ").unwrap_err();
        assert_eq!(err, GridParseError::InvalidValue("span x"));
        assert_eq!(format!("{err}"), "Invalid grid value: \"span x\"");
        assert!(parse_grid_placement("1 / span x").is_err());
    }

    #[test]
    fn parse_grid_placement_long_and_nested_input_does_not_panic() {
        let long = "a".repeat(200_000);
        assert!(parse_grid_placement(&long).is_ok());
        assert!(parse_grid_placement(&format!("{long} / {long}")).is_ok());

        // No paren handling here at all, so nesting is inert (but must not panic).
        let nested = format!("{}x{}", "(".repeat(10_000), ")".repeat(10_000));
        assert!(parse_grid_placement(&nested).is_ok());
    }

    // ---------------------------------------------------------------------
    // parse_layout_grid_auto_flow
    // ---------------------------------------------------------------------

    #[test]
    fn parse_layout_grid_auto_flow_accepts_exactly_five_spellings() {
        assert_eq!(parse_layout_grid_auto_flow("row"), Ok(LayoutGridAutoFlow::Row));
        assert_eq!(parse_layout_grid_auto_flow("column"), Ok(LayoutGridAutoFlow::Column));
        assert_eq!(parse_layout_grid_auto_flow("row dense"), Ok(LayoutGridAutoFlow::RowDense));
        assert_eq!(parse_layout_grid_auto_flow("dense"), Ok(LayoutGridAutoFlow::RowDense));
        assert_eq!(
            parse_layout_grid_auto_flow("column dense"),
            Ok(LayoutGridAutoFlow::ColumnDense)
        );
        assert_eq!(parse_layout_grid_auto_flow("  row  "), Ok(LayoutGridAutoFlow::Row));
    }

    #[test]
    fn parse_layout_grid_auto_flow_rejects_everything_else() {
        for bad in [
            "",
            "   ",
            "\t\n",
            "ROW",           // DEVIATION: CSS keywords are case-insensitive
            "Row",
            "dense row",     // DEVIATION: CSS allows either order
            "dense column",
            "row  dense",    // DEVIATION: internal whitespace is not collapsed
            "row\tdense",
            "row dense extra",
            "row;",
            "\u{1F600}",
            "\0",
        ] {
            assert!(parse_layout_grid_auto_flow(bad).is_err(), "{bad:?} must be rejected");
        }
        // Extremely long input: a straight match, no hang.
        assert!(parse_layout_grid_auto_flow(&"row ".repeat(250_000)).is_err());
    }

    #[test]
    fn parse_layout_grid_auto_flow_error_borrows_the_untrimmed_input() {
        // The match trims, but the error variant is built from the *raw* `input`, so the
        // rendered message keeps the caller's padding. Pinned because Display output of
        // these errors ends up in user-facing CSS diagnostics.
        let err = parse_layout_grid_auto_flow("  bogus  ").unwrap_err();
        assert_eq!(err, GridAutoFlowParseError::InvalidValue("  bogus  "));
        assert_eq!(format!("{err}"), "Invalid grid-auto-flow value: \"  bogus  \"");
    }

    // ---------------------------------------------------------------------
    // parse_layout_justify_self / parse_layout_justify_items
    // ---------------------------------------------------------------------

    #[test]
    fn parse_layout_justify_self_accepts_the_flex_aliases() {
        assert_eq!(parse_layout_justify_self("auto"), Ok(LayoutJustifySelf::Auto));
        assert_eq!(parse_layout_justify_self("start"), Ok(LayoutJustifySelf::Start));
        assert_eq!(parse_layout_justify_self("flex-start"), Ok(LayoutJustifySelf::Start));
        assert_eq!(parse_layout_justify_self("end"), Ok(LayoutJustifySelf::End));
        assert_eq!(parse_layout_justify_self("flex-end"), Ok(LayoutJustifySelf::End));
        assert_eq!(parse_layout_justify_self("center"), Ok(LayoutJustifySelf::Center));
        assert_eq!(parse_layout_justify_self("stretch"), Ok(LayoutJustifySelf::Stretch));
        assert_eq!(parse_layout_justify_self("  center  "), Ok(LayoutJustifySelf::Center));
    }

    #[test]
    fn parse_layout_justify_self_rejects_everything_else() {
        for bad in [
            "", "   ", "Start", "START", "space-between", "normal", "left", "right",
            "\u{1F600}", "\0", "start end", "flex-start;",
        ] {
            assert!(parse_layout_justify_self(bad).is_err(), "{bad:?}");
        }
        let err = parse_layout_justify_self("  bogus  ").unwrap_err();
        assert_eq!(err, JustifySelfParseError::InvalidValue("  bogus  "));
        assert_eq!(format!("{err}"), "Invalid justify-self value: \"  bogus  \"");
        assert!(parse_layout_justify_self(&"x".repeat(1_000_000)).is_err());
    }

    #[test]
    fn parse_layout_justify_items_has_no_auto_and_no_flex_aliases() {
        assert_eq!(parse_layout_justify_items("start"), Ok(LayoutJustifyItems::Start));
        assert_eq!(parse_layout_justify_items("end"), Ok(LayoutJustifyItems::End));
        assert_eq!(parse_layout_justify_items("center"), Ok(LayoutJustifyItems::Center));
        assert_eq!(parse_layout_justify_items("stretch"), Ok(LayoutJustifyItems::Stretch));

        // Asymmetry with justify-self, pinned deliberately: `auto` and the `flex-*`
        // aliases are accepted by justify-self but rejected by justify-items. `auto` is
        // in fact valid CSS for justify-items (it means "legacy"/inherit-ish), so this
        // is a DEVIATION, and the two parsers disagree about the same input.
        assert!(parse_layout_justify_items("auto").is_err());
        assert!(parse_layout_justify_self("auto").is_ok());
        assert!(parse_layout_justify_items("flex-start").is_err());
        assert!(parse_layout_justify_self("flex-start").is_ok());

        for bad in ["", "   ", "Start", "\u{1F600}", "\0", "stretch stretch"] {
            assert!(parse_layout_justify_items(bad).is_err(), "{bad:?}");
        }
        let err = parse_layout_justify_items("  bogus  ").unwrap_err();
        assert_eq!(err, JustifyItemsParseError::InvalidValue("  bogus  "));
        assert_eq!(format!("{err}"), "Invalid justify-items value: \"  bogus  \"");
        assert!(parse_layout_justify_items(&"x".repeat(1_000_000)).is_err());
    }

    // ---------------------------------------------------------------------
    // parse_layout_gap
    // ---------------------------------------------------------------------

    #[test]
    fn parse_layout_gap_valid_minimal_and_units() {
        assert_eq!(parse_layout_gap("10px").unwrap().inner, PixelValue::px(10.0));
        assert_eq!(parse_layout_gap("  10px  ").unwrap().inner, PixelValue::px(10.0));
        assert_eq!(parse_layout_gap("1.5em").unwrap().inner, PixelValue::em(1.5));
        assert_eq!(parse_layout_gap("50%").unwrap().inner, PixelValue::percent(50.0));
        assert_eq!(parse_layout_gap("0").unwrap().inner, PixelValue::px(0.0));
        assert_eq!(parse_layout_gap("-0").unwrap().inner, PixelValue::px(0.0));
    }

    #[test]
    fn parse_layout_gap_empty_and_garbage_are_err() {
        assert!(matches!(
            parse_layout_gap("").unwrap_err(),
            crate::props::basic::pixel::CssPixelValueParseError::EmptyString
        ));
        assert!(matches!(
            parse_layout_gap("   ").unwrap_err(),
            crate::props::basic::pixel::CssPixelValueParseError::EmptyString
        ));
        for bad in ["px", "10pxpx", "!!!", "\u{1F600}", "\0", "auto", "normal"] {
            assert!(parse_layout_gap(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn parse_layout_gap_accepts_negative_unitless_and_split_units_is_lax() {
        // DEVIATION: `gap` is a <length-percentage [0,∞]> — negatives are invalid CSS,
        // and a nonzero unitless number is invalid too. Both are accepted here.
        assert_eq!(parse_layout_gap("-20px").unwrap().inner, PixelValue::px(-20.0));
        assert_eq!(parse_layout_gap("10").unwrap().inner, PixelValue::px(10.0));

        // DEVIATION (inherited from parse_pixel_value_inner): the unit suffix is stripped
        // *before* the remainder is trimmed, so whitespace between the number and its
        // unit is silently tolerated. `gap: 10 px` is not valid CSS but parses as 10px.
        assert_eq!(parse_layout_gap("10 px").unwrap().inner, PixelValue::px(10.0));
        assert_eq!(parse_layout_gap("10\tpx").unwrap().inner, PixelValue::px(10.0));
        // Same hole in the track parser's fr path, though the tokenizer usually hides it
        // by splitting "1 fr" into two tokens first.
        assert_eq!(parse_grid_track_owned("1 fr"), Ok(GridTrackSizing::Fr(100)));
        assert!(parse_grid_template("1 fr").is_err(), "...but not via the tokenizer");
    }

    #[test]
    fn parse_layout_gap_nan_and_inf_do_not_panic_but_are_swallowed() {
        // BUG (silent coercion, inherited from FloatValue::new): the f32 -> isize cast
        // saturates, so `gap: NaN` becomes 0px and `gap: inf` becomes isize::MAX/1000 px.
        // Neither is rejected, and neither panics.
        assert_eq!(
            parse_layout_gap("NaN").unwrap().inner,
            PixelValue::px(0.0),
            "NaN silently coerces to 0px"
        );
        assert_eq!(
            parse_layout_gap("inf").unwrap().inner,
            PixelValue::px(f32::INFINITY),
            "inf saturates instead of erroring"
        );
        assert_eq!(
            parse_layout_gap("-inf").unwrap().inner,
            PixelValue::px(f32::NEG_INFINITY)
        );
        assert_eq!(
            parse_layout_gap("1e40px").unwrap().inner,
            PixelValue::px(f32::INFINITY),
            "an f32 overflow saturates rather than wrapping"
        );
        // 10k-digit number: parses (to inf) without hanging.
        assert!(parse_layout_gap(&format!("{}px", "9".repeat(10_000))).is_ok());
    }

    // ---------------------------------------------------------------------
    // parse_grid_template_areas
    // ---------------------------------------------------------------------

    fn area_named<'a>(areas: &'a GridTemplateAreas, name: &str) -> &'a GridAreaDefinition {
        areas
            .areas
            .as_ref()
            .iter()
            .find(|a| a.name.as_str() == name)
            .unwrap_or_else(|| panic!("no area named {name:?}"))
    }

    #[test]
    fn parse_grid_template_areas_valid_minimal() {
        let parsed = parse_grid_template_areas("\"a\"").unwrap();
        assert_eq!(parsed.areas.len(), 1);
        assert_eq!(
            parsed.areas.as_ref()[0],
            GridAreaDefinition {
                name: "a".to_string().into(),
                row_start: 1,
                row_end: 2,
                column_start: 1,
                column_end: 2,
            },
            "1-based, end line is one past the last cell"
        );

        assert_eq!(
            parse_grid_template_areas("none").unwrap(),
            GridTemplateAreas::default()
        );
        assert_eq!(
            parse_grid_template_areas("  none  ").unwrap(),
            GridTemplateAreas::default()
        );
    }

    #[test]
    fn parse_grid_template_areas_computes_bounds_and_sorts_by_name() {
        let parsed =
            parse_grid_template_areas("\"header header\" \"sidebar main\" \"footer footer\"")
                .unwrap();
        assert_eq!(parsed.areas.len(), 4);

        // BTreeMap iteration => areas come out alphabetically, NOT in source order.
        let names: Vec<&str> = parsed.areas.as_ref().iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, vec!["footer", "header", "main", "sidebar"]);

        let header = area_named(&parsed, "header");
        assert_eq!((header.row_start, header.row_end), (1, 2));
        assert_eq!((header.column_start, header.column_end), (1, 3), "spans both columns");

        let sidebar = area_named(&parsed, "sidebar");
        assert_eq!((sidebar.row_start, sidebar.row_end), (2, 3));
        assert_eq!((sidebar.column_start, sidebar.column_end), (1, 2));

        let footer = area_named(&parsed, "footer");
        assert_eq!((footer.row_start, footer.row_end), (3, 4));
        assert_eq!((footer.column_start, footer.column_end), (1, 3));
    }

    #[test]
    fn parse_grid_template_areas_null_cells_are_skipped() {
        let parsed = parse_grid_template_areas("\". a\" \". a\"").unwrap();
        assert_eq!(parsed.areas.len(), 1, "'.' never becomes an area");
        let a = area_named(&parsed, "a");
        assert_eq!((a.row_start, a.row_end), (1, 3));
        assert_eq!((a.column_start, a.column_end), (2, 3));

        // An all-null grid is Ok-but-empty, indistinguishable from `none`.
        let all_null = parse_grid_template_areas("\". .\" \". .\"").unwrap();
        assert_eq!(all_null, GridTemplateAreas::default());
    }

    #[test]
    fn parse_grid_template_areas_rejects_empty_unterminated_and_ragged() {
        for bad in [
            "",              // no rows
            "   ",
            "\t\n",
            "\"\"",          // empty quoted row -> zero cells
            "\"  \"",        // whitespace-only row -> zero cells
            "\"abc",         // unterminated quote
            "'abc",
            "\"a\" \"b",     // second row unterminated
            "\"a b\" \"c\"", // ragged: 2 columns then 1
            "\"a\" \"b c\"",
            "abc",           // no quotes at all
            "\u{1F600}",
        ] {
            assert_eq!(parse_grid_template_areas(bad), Err(()), "{bad:?} must be rejected");
        }
    }

    #[test]
    fn parse_grid_template_areas_ignores_junk_outside_the_quotes_is_lax() {
        // DEVIATION: the scanner only looks at quoted runs; anything between them is
        // skipped without complaint. `grid-template-areas: garbage "a" ;;;` parses.
        let parsed = parse_grid_template_areas("garbage \"a\" ;;; \u{1F600}").unwrap();
        assert_eq!(parsed.areas.len(), 1);
        assert_eq!(area_named(&parsed, "a").row_start, 1);

        // Single and double quotes are interchangeable, and may be mixed.
        let mixed = parse_grid_template_areas("'a' \"b\"").unwrap();
        assert_eq!(mixed.areas.len(), 2);
        // A double quote inside a single-quoted row is just a name character.
        let weird = parse_grid_template_areas("'a\"b'").unwrap();
        assert_eq!(weird.areas.as_ref()[0].name.as_str(), "a\"b");
    }

    #[test]
    fn parse_grid_template_areas_uses_tabs_and_newlines_as_cell_separators() {
        // Contrast with split_respecting_parens: this path uses split_whitespace(), so
        // it *does* handle the whitespace CSS actually allows.
        let parsed = parse_grid_template_areas("\"a\tb\" \"c\nd\"").unwrap();
        assert_eq!(parsed.areas.len(), 4);
        for name in ["a", "b", "c", "d"] {
            assert_eq!(area_named(&parsed, name).name.as_str(), name);
        }
    }

    #[test]
    fn parse_grid_template_areas_non_rectangular_becomes_a_bounding_box() {
        // BUG (spec violation): CSS requires each named area to form a single rectangle;
        // a discontiguous name is a parse error. Here the min/max reduction just takes
        // the bounding box, so `"b a b"` yields a `b` that spans columns 1..4 and
        // *swallows* the `a` sitting between the two halves.
        let parsed = parse_grid_template_areas("\"b a b\"").unwrap();
        let b = area_named(&parsed, "b");
        let a = area_named(&parsed, "a");
        assert_eq!((b.column_start, b.column_end), (1, 4), "bounding box, not a rectangle");
        assert_eq!((a.column_start, a.column_end), (2, 3));
        assert!(
            a.column_start >= b.column_start && a.column_end <= b.column_end,
            "the two areas overlap, which taffy is never supposed to be handed"
        );
    }

    #[test]
    fn parse_grid_template_areas_row_bounds_saturate_past_u16_max() {
        // 65_599 rows of "a" then one row of "b". True line numbers exceed u16::MAX, and
        // `u16::try_from(..).unwrap_or(u16::MAX)` clamps rather than panicking.
        let mut input = String::with_capacity(65_600 * 4);
        for _ in 0..65_599 {
            input.push_str("\"a\" ");
        }
        input.push_str("\"b\"");

        let parsed = parse_grid_template_areas(&input).unwrap();
        assert_eq!(parsed.areas.len(), 2);

        let a = area_named(&parsed, "a");
        assert_eq!(a.row_start, 1);
        assert_eq!(
            a.row_end,
            u16::MAX,
            "true end line is 65_600; it saturates and the grid is silently truncated"
        );

        // BUG (degenerate output): "b" starts at row 65_600, so BOTH of its line numbers
        // clamp to u16::MAX — producing a zero-height area (row_start == row_end). Every
        // other area produced by this parser satisfies row_end > row_start.
        let b = area_named(&parsed, "b");
        assert_eq!(b.row_start, u16::MAX);
        assert_eq!(b.row_end, u16::MAX);
        assert_eq!(b.row_start, b.row_end, "zero-height area from double saturation");
    }

    #[test]
    fn parse_grid_template_areas_column_bounds_saturate_past_u16_max() {
        let mut row = String::with_capacity(70_000 * 2 + 2);
        row.push('"');
        for _ in 0..70_000 {
            row.push_str("a ");
        }
        row.push('"');

        let parsed = parse_grid_template_areas(&row).unwrap();
        assert_eq!(parsed.areas.len(), 1);
        let a = area_named(&parsed, "a");
        assert_eq!(a.column_start, 1);
        assert_eq!(a.column_end, u16::MAX, "true end line is 70_001; clamped, not wrapped");
        assert_eq!((a.row_start, a.row_end), (1, 2));
    }

    #[test]
    fn parse_grid_template_areas_invariant_end_line_exceeds_start_line() {
        // The invariant PrintAsCssValue relies on (it computes `max_row - 1` on u16 and
        // would underflow if any row_end were 0). Holds for every non-degenerate input;
        // the saturation test above is the one case that violates row_end > row_start.
        for input in [
            "\"a\"",
            "\"a b\" \"c d\"",
            "\"h h h\" \"s m a\" \"f f f\"",
            "\". a .\" \"b a c\"",
            "'x'",
        ] {
            let parsed = parse_grid_template_areas(input).unwrap();
            for area in parsed.areas.as_ref() {
                assert!(area.row_start >= 1, "{input:?}: row_start must be 1-based");
                assert!(area.column_start >= 1, "{input:?}: column_start must be 1-based");
                assert!(area.row_end > area.row_start, "{input:?}: {area:?}");
                assert!(area.column_end > area.column_start, "{input:?}: {area:?}");
            }
        }
    }

    // ---------------------------------------------------------------------
    // Round-trips: print_as_css_value -> parse
    // ---------------------------------------------------------------------

    #[test]
    fn roundtrip_grid_auto_flow_all_variants() {
        for value in [
            LayoutGridAutoFlow::Row,
            LayoutGridAutoFlow::Column,
            LayoutGridAutoFlow::RowDense,
            LayoutGridAutoFlow::ColumnDense,
        ] {
            let printed = value.print_as_css_value();
            assert_eq!(
                parse_layout_grid_auto_flow(&printed),
                Ok(value),
                "{value:?} printed as {printed:?} must re-parse to itself"
            );
        }
    }

    #[test]
    fn roundtrip_justify_self_and_justify_items_all_variants() {
        for value in [
            LayoutJustifySelf::Auto,
            LayoutJustifySelf::Start,
            LayoutJustifySelf::End,
            LayoutJustifySelf::Center,
            LayoutJustifySelf::Stretch,
        ] {
            let printed = value.print_as_css_value();
            assert_eq!(parse_layout_justify_self(&printed), Ok(value), "{value:?}");
        }
        for value in [
            LayoutJustifyItems::Start,
            LayoutJustifyItems::End,
            LayoutJustifyItems::Center,
            LayoutJustifyItems::Stretch,
        ] {
            let printed = value.print_as_css_value();
            assert_eq!(parse_layout_justify_items(&printed), Ok(value), "{value:?}");
        }
    }

    #[test]
    fn roundtrip_gap_and_fixed_tracks_are_stable() {
        for input in ["0px", "10px", "-20px", "1.5em", "2rem", "50%", "10mm", "1in"] {
            let gap = parse_layout_gap(input).unwrap();
            let printed = gap.print_as_css_value();
            assert_eq!(parse_layout_gap(&printed).unwrap(), gap, "gap {input:?} -> {printed:?}");
        }
        for input in ["auto", "min-content", "max-content", "100px", "25%", "fit-content(2px)"] {
            let track = parse_grid_track_owned(input).unwrap();
            let printed = track.print_as_css_value();
            assert_eq!(
                parse_grid_track_owned(&printed),
                Ok(track.clone()),
                "track {input:?} -> {printed:?}"
            );
        }
    }

    #[test]
    fn roundtrip_fr_track_inflates_by_100x_every_cycle() {
        // BUG (serialization): GridTrackSizing::Fr stores the value pre-multiplied by
        // FR_SCALING_FACTOR (1fr == Fr(100)), but PrintAsCssValue prints the RAW integer
        // — `format!("{f}fr")` — instead of dividing it back out. So `1fr` serializes as
        // `100fr`, and every print/parse cycle multiplies the track's flex factor by 100.
        // Any code path that round-trips a stylesheet (serialize a computed style, then
        // re-parse it) corrupts every fr track. Correct output would be "1fr".
        let once = parse_grid_track_owned("1fr").unwrap();
        assert_eq!(once, GridTrackSizing::Fr(100));
        assert_eq!(once.print_as_css_value(), "100fr", "should be \"1fr\"");

        let twice = parse_grid_track_owned(&once.print_as_css_value()).unwrap();
        assert_eq!(twice, GridTrackSizing::Fr(10_000), "100x inflation per cycle");

        let thrice = parse_grid_track_owned(&twice.print_as_css_value()).unwrap();
        assert_eq!(thrice, GridTrackSizing::Fr(1_000_000));

        // Four cycles overflow the i32 guard and the value is dropped entirely.
        let four = parse_grid_track_owned(&thrice.print_as_css_value()).unwrap();
        assert_eq!(four, GridTrackSizing::Fr(100_000_000));
        assert_eq!(
            parse_grid_track_owned(&four.print_as_css_value()),
            Err(()),
            "the fifth cycle exceeds i32 and the declaration is discarded"
        );

        // Same bug through the minmax and template printers.
        assert_eq!(
            parse_grid_track_owned("minmax(100px, 1fr)").unwrap().print_as_css_value(),
            "minmax(100px, 100fr)"
        );
        assert_eq!(
            parse_grid_template("1fr 2fr").unwrap().print_as_css_value(),
            "100fr 200fr"
        );
    }

    #[test]
    fn roundtrip_grid_template_none_and_repeat_expansion() {
        assert_eq!(GridTemplate::default().print_as_css_value(), "none");
        assert_eq!(
            parse_grid_template(&GridTemplate::default().print_as_css_value()).unwrap(),
            GridTemplate::default()
        );

        // repeat() is expanded at parse time and never re-emitted as repeat().
        let expanded = parse_grid_template("repeat(3, 100px)").unwrap();
        assert_eq!(expanded.print_as_css_value(), "100px 100px 100px");
        assert_eq!(parse_grid_template(&expanded.print_as_css_value()).unwrap(), expanded);
    }

    #[test]
    fn roundtrip_grid_auto_tracks_empty_prints_auto_but_reparses_to_one_track() {
        // GridAutoTracks::default() is *zero* tracks yet prints as "auto"; re-parsing
        // that text yields *one* Auto track. Structurally lossy, semantically equivalent
        // (taffy treats a missing grid-auto-* as `auto`), pinned so a future change to
        // either side is a deliberate one.
        let empty = GridAutoTracks::default();
        assert_eq!(empty.tracks.len(), 0);
        assert_eq!(empty.print_as_css_value(), "auto");

        let reparsed: GridAutoTracks = parse_grid_template(&empty.print_as_css_value())
            .unwrap()
            .into();
        assert_eq!(reparsed.tracks.len(), 1);
        assert_eq!(reparsed.tracks.as_ref()[0], GridTrackSizing::Auto);
        assert_ne!(reparsed, empty);
    }

    #[test]
    fn roundtrip_grid_placement_hides_a_trailing_auto_end() {
        // `GridPlacement { start, end: Auto }` prints only the start (no " / auto"), which
        // is what CSS does too. Both directions round-trip.
        for input in ["auto", "1", "-1", "span 2", "1 / 3", "1 / span 2", "auto / 3"] {
            let placement = parse_grid_placement(input).unwrap();
            let printed = placement.print_as_css_value();
            assert_eq!(
                parse_grid_placement(&printed).unwrap(),
                placement,
                "{input:?} printed as {printed:?}"
            );
        }
        assert_eq!(
            GridPlacement { grid_start: GridLine::Line(1), grid_end: GridLine::Auto }
                .print_as_css_value(),
            "1"
        );
        assert_eq!(
            GridPlacement { grid_start: GridLine::Auto, grid_end: GridLine::Line(3) }
                .print_as_css_value(),
            "auto / 3"
        );
    }

    #[test]
    fn roundtrip_named_grid_line_with_a_span_is_lossy() {
        // BUG (unparseable output): `GridLine::Named` with a span prints as "name N"
        // (e.g. "header 2"), but parse_grid_line_owned has no production for that shape —
        // it falls into the named catch-all and produces a line literally *named*
        // "header 2" with span 0. The span is silently lost. Nothing in this file can
        // construct a spanned Named line from CSS text, so the only way to hit this is to
        // build one via NamedGridLine::create and serialize it.
        let spanned = GridLine::Named(NamedGridLine::create("header".to_string().into(), Some(2)));
        assert_eq!(spanned.print_as_css_value(), "header 2");

        let reparsed = parse_grid_line_owned(&spanned.print_as_css_value()).unwrap();
        assert_eq!(
            reparsed,
            GridLine::Named(NamedGridLine::create("header 2".to_string().into(), None))
        );
        assert_ne!(reparsed, spanned);

        // Without a span it round-trips cleanly.
        let plain = GridLine::Named(NamedGridLine::create("header".to_string().into(), None));
        assert_eq!(plain.print_as_css_value(), "header");
        assert_eq!(parse_grid_line_owned(&plain.print_as_css_value()), Ok(plain));
    }

    #[test]
    fn roundtrip_grid_template_areas_rectangular_is_stable() {
        for input in [
            "\"a\"",
            "\"a a\"",
            "\"h h\" \"s m\"",
            "\"a a\" \"a a\"",
            "\". a\" \". a\"",
        ] {
            let parsed = parse_grid_template_areas(input).unwrap();
            let printed = parsed.print_as_css_value();
            assert_eq!(
                parse_grid_template_areas(&printed).unwrap(),
                parsed,
                "{input:?} printed as {printed:?}"
            );
        }
        assert_eq!(GridTemplateAreas::default().print_as_css_value(), "none");
        assert_eq!(
            parse_grid_template_areas(&GridTemplateAreas::default().print_as_css_value()).unwrap(),
            GridTemplateAreas::default()
        );
    }

    #[test]
    fn roundtrip_grid_template_areas_non_rectangular_loses_a_whole_area() {
        // BUG (data loss), the printing half of the bounding-box bug above: areas are
        // repainted in alphabetical order, so a later name overwrites the cells of an
        // earlier one that its bounding box happens to cover. `"b a b"` -> `"b b b"`:
        // the `a` area vanishes on serialization.
        let parsed = parse_grid_template_areas("\"b a b\"").unwrap();
        assert_eq!(parsed.areas.len(), 2);
        assert_eq!(parsed.print_as_css_value(), "\"b b b\"", "the `a` area is erased");

        let reparsed = parse_grid_template_areas(&parsed.print_as_css_value()).unwrap();
        assert_eq!(reparsed.areas.len(), 1);
        assert_ne!(reparsed, parsed);
    }

    // ---------------------------------------------------------------------
    // Error types: to_contained / to_shared
    // ---------------------------------------------------------------------

    #[test]
    fn grid_parse_error_to_contained_and_back_is_identity() {
        for payload in ["", "   ", "bogus", "\u{1F600}\u{0301}", "\0", "a\nb\tc"] {
            let shared = GridParseError::InvalidValue(payload);
            let owned = shared.to_contained();
            assert_eq!(owned, GridParseErrorOwned::InvalidValue(payload.to_string().into()));
            assert_eq!(owned.to_shared(), shared, "{payload:?} must survive the round-trip");
        }
        // A 100k-char payload: no truncation, no panic.
        let huge = "x".repeat(100_000);
        let owned = GridParseError::InvalidValue(&huge).to_contained();
        assert_eq!(owned.to_shared(), GridParseError::InvalidValue(huge.as_str()));
    }

    #[test]
    fn grid_auto_flow_parse_error_to_contained_and_back_is_identity() {
        for payload in ["", "bogus", "\u{1F600}", "\0"] {
            let shared = GridAutoFlowParseError::InvalidValue(payload);
            let owned = shared.to_contained();
            assert_eq!(
                owned,
                GridAutoFlowParseErrorOwned::InvalidValue(payload.to_string().into())
            );
            assert_eq!(owned.to_shared(), shared);
        }
        let huge = "x".repeat(100_000);
        let owned = GridAutoFlowParseError::InvalidValue(&huge).to_contained();
        assert_eq!(owned.to_shared(), GridAutoFlowParseError::InvalidValue(huge.as_str()));
    }

    #[test]
    fn justify_self_parse_error_to_contained_and_back_is_identity() {
        for payload in ["", "bogus", "\u{1F600}", "\0"] {
            let shared = JustifySelfParseError::InvalidValue(payload);
            let owned = shared.to_contained();
            assert_eq!(
                owned,
                JustifySelfParseErrorOwned::InvalidValue(payload.to_string().into())
            );
            assert_eq!(owned.to_shared(), shared);
        }
        let huge = "x".repeat(100_000);
        let owned = JustifySelfParseError::InvalidValue(&huge).to_contained();
        assert_eq!(owned.to_shared(), JustifySelfParseError::InvalidValue(huge.as_str()));
    }

    #[test]
    fn justify_items_parse_error_to_contained_and_back_is_identity() {
        for payload in ["", "bogus", "\u{1F600}", "\0"] {
            let shared = JustifyItemsParseError::InvalidValue(payload);
            let owned = shared.to_contained();
            assert_eq!(
                owned,
                JustifyItemsParseErrorOwned::InvalidValue(payload.to_string().into())
            );
            assert_eq!(owned.to_shared(), shared);
        }
        let huge = "x".repeat(100_000);
        let owned = JustifyItemsParseError::InvalidValue(&huge).to_contained();
        assert_eq!(owned.to_shared(), JustifyItemsParseError::InvalidValue(huge.as_str()));
    }

    #[test]
    fn error_types_round_trip_through_a_real_parse_failure() {
        // The realistic path: borrow an error out of a parser, own it, hand it back.
        let input = String::from("  span x  ");
        let owned = parse_grid_placement(&input).unwrap_err().to_contained();
        drop(input); // the owned form must not borrow the parsed input
        assert_eq!(owned.to_shared(), GridParseError::InvalidValue("span x"));

        let flow = String::from("bogus-flow");
        let owned_flow = parse_layout_grid_auto_flow(&flow).unwrap_err().to_contained();
        drop(flow);
        assert_eq!(
            owned_flow.to_shared(),
            GridAutoFlowParseError::InvalidValue("bogus-flow")
        );
    }

    // ---------------------------------------------------------------------
    // Debug / display invariants
    // ---------------------------------------------------------------------

    #[test]
    fn debug_impls_match_print_as_css_value() {
        // GridTrackSizing / GridTemplate / GridPlacement all forward Debug to the CSS
        // printer, so a Debug regression is a serialization regression.
        let track = parse_grid_track_owned("minmax(100px, max-content)").unwrap();
        assert_eq!(format!("{track:?}"), track.print_as_css_value());

        let template = parse_grid_template("100px auto").unwrap();
        assert_eq!(format!("{template:?}"), template.print_as_css_value());

        let placement = parse_grid_placement("1 / span 2").unwrap();
        assert_eq!(format!("{placement:?}"), placement.print_as_css_value());

        let minmax = GridMinMax {
            min: Box::new(GridTrackSizing::Fixed(PixelValue::px(1.0))),
            max: Box::new(GridTrackSizing::Auto),
        };
        assert_eq!(format!("{minmax:?}"), "minmax(1px, auto)");
    }
}
