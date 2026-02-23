//! CSS properties for CSS Grid layout.

use alloc::{
    boxed::Box,
    string::{String, ToString},
    vec::Vec,
};

use crate::{
    corety::AzString,
    format_rust_code::FormatAsRustCode,
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
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
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
pub enum GridTrackSizing {
    /// Fixed pixel/percent size
    Fixed(PixelValue),
    /// fr units (stored as integer to satisfy Eq/Ord/Hash)
    Fr(i32),
    /// min-content
    MinContent,
    /// max-content
    MaxContent,
    /// auto
    Auto,
    /// minmax(min, max) - uses GridMinMax which contains Box<GridTrackSizing> for each bound
    MinMax(GridMinMax),
    /// fit-content(size)
    FitContent(PixelValue),
}

impl_option!(
    GridTrackSizing,
    OptionGridTrackSizing,
    copy = false,
    [Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

impl core::fmt::Debug for GridTrackSizing {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(f, "{}", self.print_as_css_value())
    }
}

impl Default for GridTrackSizing {
    fn default() -> Self {
        GridTrackSizing::Auto
    }
}

impl PrintAsCssValue for GridTrackSizing {
    fn print_as_css_value(&self) -> String {
        match self {
            GridTrackSizing::Fixed(px) => px.print_as_css_value(),
            GridTrackSizing::Fr(f) => format!("{}fr", f),
            GridTrackSizing::MinContent => "min-content".to_string(),
            GridTrackSizing::MaxContent => "max-content".to_string(),
            GridTrackSizing::Auto => "auto".to_string(),
            GridTrackSizing::MinMax(minmax) => {
                format!(
                    "minmax({}, {})",
                    minmax.min.print_as_css_value(),
                    minmax.max.print_as_css_value()
                )
            }
            GridTrackSizing::FitContent(size) => {
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
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(f, "{}", self.print_as_css_value())
    }
}

impl Default for GridTemplate {
    fn default() -> Self {
        GridTemplate {
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
                .map(|t| t.print_as_css_value())
                .collect::<Vec<_>>()
                .join(" ")
        }
    }
}

// --- grid-auto-columns / grid-auto-rows ---

/// Represents `grid-auto-columns` or `grid-auto-rows`
/// Structurally identical to GridTemplate but semantically different
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct GridAutoTracks {
    pub tracks: GridTrackSizingVec,
}

impl core::fmt::Debug for GridAutoTracks {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(f, "{}", self.print_as_css_value())
    }
}

impl Default for GridAutoTracks {
    fn default() -> Self {
        GridAutoTracks {
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
                .map(|t| t.print_as_css_value())
                .collect::<Vec<_>>()
                .join(" ")
        }
    }
}

impl From<GridTemplate> for GridAutoTracks {
    fn from(template: GridTemplate) -> Self {
        GridAutoTracks {
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
    pub fn create(name: AzString, span: Option<i32>) -> Self {
        Self {
            grid_line_name: name,
            span_count: span.unwrap_or(0),
        }
    }

    pub fn span(&self) -> Option<i32> {
        if self.span_count == 0 {
            None
        } else {
            Some(self.span_count)
        }
    }
}

/// Represents a grid line position (start or end)
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C, u8)]
pub enum GridLine {
    /// auto
    Auto,
    /// Line number (1-based, negative for counting from end)
    Line(i32),
    /// Named line with optional span count
    Named(NamedGridLine),
    /// span N
    Span(i32),
}

impl core::fmt::Debug for GridLine {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(f, "{}", self.print_as_css_value())
    }
}

impl Default for GridLine {
    fn default() -> Self {
        GridLine::Auto
    }
}

impl PrintAsCssValue for GridLine {
    fn print_as_css_value(&self) -> String {
        match self {
            GridLine::Auto => "auto".to_string(),
            GridLine::Line(n) => n.to_string(),
            GridLine::Named(named) => {
                if named.span_count == 0 {
                    named.grid_line_name.as_str().to_string()
                } else {
                    format!("{} {}", named.grid_line_name.as_str(), named.span_count)
                }
            }
            GridLine::Span(n) => format!("span {}", n),
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
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(f, "{}", self.print_as_css_value())
    }
}

impl Default for GridPlacement {
    fn default() -> Self {
        GridPlacement {
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
#[derive(Clone, PartialEq)]
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
#[derive(Debug, Clone, PartialEq)]
#[repr(C, u8)]
pub enum GridParseErrorOwned {
    InvalidValue(AzString),
}

#[cfg(feature = "parser")]
impl<'a> GridParseError<'a> {
    pub fn to_contained(&self) -> GridParseErrorOwned {
        match self {
            GridParseError::InvalidValue(s) => GridParseErrorOwned::InvalidValue(s.to_string().into()),
        }
    }
}

#[cfg(feature = "parser")]
impl GridParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> GridParseError<'a> {
        match self {
            GridParseErrorOwned::InvalidValue(s) => GridParseError::InvalidValue(s.as_str()),
        }
    }
}

#[cfg(feature = "parser")]
pub fn parse_grid_template<'a>(input: &'a str) -> Result<GridTemplate, GridParseError<'a>> {
    use crate::props::basic::pixel::parse_pixel_value;

    let input = input.trim();

    if input == "none" {
        return Ok(GridTemplate::default());
    }

    let mut tracks = Vec::new();
    let mut current = String::new();
    let mut paren_depth = 0;

    for ch in input.chars() {
        match ch {
            '(' => {
                paren_depth += 1;
                current.push(ch);
            }
            ')' => {
                paren_depth -= 1;
                current.push(ch);
            }
            ' ' if paren_depth == 0 => {
                if !current.trim().is_empty() {
                    let track_str = current.trim().to_string();
                    parse_grid_track_or_repeat(&track_str, &mut tracks)
                        .map_err(|_| GridParseError::InvalidValue(input))?;
                    current.clear();
                }
            }
            _ => current.push(ch),
        }
    }

    if !current.trim().is_empty() {
        let track_str = current.trim().to_string();
        parse_grid_track_or_repeat(&track_str, &mut tracks)
            .map_err(|_| GridParseError::InvalidValue(input))?;
    }

    Ok(GridTemplate {
        tracks: GridTrackSizingVec::from_vec(tracks),
    })
}

/// Parse a single grid track token, which may be `repeat(N, track)` or a plain track.
/// For `repeat(N, track_list)`, the tracks are expanded inline.
#[cfg(feature = "parser")]
fn parse_grid_track_or_repeat(input: &str, tracks: &mut Vec<GridTrackSizing>) -> Result<(), ()> {
    let input = input.trim();

    // Handle repeat(N, track_list)
    if input.starts_with("repeat(") && input.ends_with(')') {
        let content = &input[7..input.len() - 1];
        // Find the first comma that separates the count from the track list
        let comma_pos = content.find(',').ok_or(())?;
        let count_str = content[..comma_pos].trim();
        let track_list_str = content[comma_pos + 1..].trim();

        let count: usize = count_str.parse().map_err(|_| ())?;
        if count == 0 || count > 10000 {
            return Err(());
        }

        // Parse the track list (may contain multiple space-separated tracks)
        let mut repeat_tracks = Vec::new();
        let mut current = String::new();
        let mut paren_depth = 0;
        for ch in track_list_str.chars() {
            match ch {
                '(' => { paren_depth += 1; current.push(ch); }
                ')' => { paren_depth -= 1; current.push(ch); }
                ' ' if paren_depth == 0 => {
                    if !current.trim().is_empty() {
                        repeat_tracks.push(parse_grid_track_owned(current.trim())?);
                        current.clear();
                    }
                }
                _ => current.push(ch),
            }
        }
        if !current.trim().is_empty() {
            repeat_tracks.push(parse_grid_track_owned(current.trim())?);
        }

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

    if input.ends_with("fr") {
        let num_str = &input[..input.len() - 2].trim();
        if let Ok(num) = num_str.parse::<f32>() {
            return Ok(GridTrackSizing::Fr((num * 100.0) as i32));
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
pub fn parse_grid_placement<'a>(input: &'a str) -> Result<GridPlacement, GridParseError<'a>> {
    let input = input.trim();

    if input == "auto" {
        return Ok(GridPlacement::default());
    }

    // Split by "/"
    let parts: Vec<&str> = input.split('/').map(|s| s.trim()).collect();

    let grid_start =
        parse_grid_line_owned(parts[0]).map_err(|_| GridParseError::InvalidValue(input))?;
    let grid_end = if parts.len() > 1 {
        parse_grid_line_owned(parts[1]).map_err(|_| GridParseError::InvalidValue(input))?
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
pub enum LayoutGridAutoFlow {
    Row,
    Column,
    RowDense,
    ColumnDense,
}

impl Default for LayoutGridAutoFlow {
    fn default() -> Self {
        LayoutGridAutoFlow::Row
    }
}

impl crate::props::formatter::PrintAsCssValue for LayoutGridAutoFlow {
    fn print_as_css_value(&self) -> alloc::string::String {
        match self {
            LayoutGridAutoFlow::Row => "row".to_string(),
            LayoutGridAutoFlow::Column => "column".to_string(),
            LayoutGridAutoFlow::RowDense => "row dense".to_string(),
            LayoutGridAutoFlow::ColumnDense => "column dense".to_string(),
        }
    }
}

#[cfg(feature = "parser")]
#[derive(Clone, PartialEq)]
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
#[derive(Debug, Clone, PartialEq)]
#[repr(C, u8)]
pub enum GridAutoFlowParseErrorOwned {
    InvalidValue(AzString),
}

#[cfg(feature = "parser")]
impl<'a> GridAutoFlowParseError<'a> {
    pub fn to_contained(&self) -> GridAutoFlowParseErrorOwned {
        match self {
            GridAutoFlowParseError::InvalidValue(s) => {
                GridAutoFlowParseErrorOwned::InvalidValue(s.to_string().into())
            }
        }
    }
}

#[cfg(feature = "parser")]
impl GridAutoFlowParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> GridAutoFlowParseError<'a> {
        match self {
            GridAutoFlowParseErrorOwned::InvalidValue(s) => {
                GridAutoFlowParseError::InvalidValue(s.as_str())
            }
        }
    }
}

#[cfg(feature = "parser")]
pub fn parse_layout_grid_auto_flow<'a>(
    input: &'a str,
) -> Result<LayoutGridAutoFlow, GridAutoFlowParseError<'a>> {
    match input.trim() {
        "row" => Ok(LayoutGridAutoFlow::Row),
        "column" => Ok(LayoutGridAutoFlow::Column),
        "row dense" => Ok(LayoutGridAutoFlow::RowDense),
        "column dense" => Ok(LayoutGridAutoFlow::ColumnDense),
        "dense" => Ok(LayoutGridAutoFlow::RowDense),
        _ => Err(GridAutoFlowParseError::InvalidValue(input)),
    }
}

// --- justify-self / justify-items ---

/// Represents `justify-self` for grid items
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum LayoutJustifySelf {
    Auto,
    Start,
    End,
    Center,
    Stretch,
}

impl Default for LayoutJustifySelf {
    fn default() -> Self {
        Self::Auto
    }
}

impl crate::props::formatter::PrintAsCssValue for LayoutJustifySelf {
    fn print_as_css_value(&self) -> alloc::string::String {
        match self {
            LayoutJustifySelf::Auto => "auto".to_string(),
            LayoutJustifySelf::Start => "start".to_string(),
            LayoutJustifySelf::End => "end".to_string(),
            LayoutJustifySelf::Center => "center".to_string(),
            LayoutJustifySelf::Stretch => "stretch".to_string(),
        }
    }
}

#[cfg(feature = "parser")]
#[derive(Clone, PartialEq)]
pub enum JustifySelfParseError<'a> {
    InvalidValue(&'a str),
}

#[cfg(feature = "parser")]
#[derive(Debug, Clone, PartialEq)]
#[repr(C, u8)]
pub enum JustifySelfParseErrorOwned {
    InvalidValue(AzString),
}

#[cfg(feature = "parser")]
impl<'a> JustifySelfParseError<'a> {
    pub fn to_contained(&self) -> JustifySelfParseErrorOwned {
        match self {
            JustifySelfParseError::InvalidValue(s) => {
                JustifySelfParseErrorOwned::InvalidValue(s.to_string().into())
            }
        }
    }
}

#[cfg(feature = "parser")]
impl JustifySelfParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> JustifySelfParseError<'a> {
        match self {
            JustifySelfParseErrorOwned::InvalidValue(s) => {
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
pub fn parse_layout_justify_self<'a>(
    input: &'a str,
) -> Result<LayoutJustifySelf, JustifySelfParseError<'a>> {
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
pub enum LayoutJustifyItems {
    Start,
    End,
    Center,
    Stretch,
}

impl Default for LayoutJustifyItems {
    fn default() -> Self {
        Self::Stretch
    }
}

impl crate::props::formatter::PrintAsCssValue for LayoutJustifyItems {
    fn print_as_css_value(&self) -> alloc::string::String {
        match self {
            LayoutJustifyItems::Start => "start".to_string(),
            LayoutJustifyItems::End => "end".to_string(),
            LayoutJustifyItems::Center => "center".to_string(),
            LayoutJustifyItems::Stretch => "stretch".to_string(),
        }
    }
}

#[cfg(feature = "parser")]
#[derive(Clone, PartialEq)]
pub enum JustifyItemsParseError<'a> {
    InvalidValue(&'a str),
}

#[cfg(feature = "parser")]
#[derive(Debug, Clone, PartialEq)]
#[repr(C, u8)]
pub enum JustifyItemsParseErrorOwned {
    InvalidValue(AzString),
}

#[cfg(feature = "parser")]
impl<'a> JustifyItemsParseError<'a> {
    pub fn to_contained(&self) -> JustifyItemsParseErrorOwned {
        match self {
            JustifyItemsParseError::InvalidValue(s) => {
                JustifyItemsParseErrorOwned::InvalidValue(s.to_string().into())
            }
        }
    }
}

#[cfg(feature = "parser")]
impl JustifyItemsParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> JustifyItemsParseError<'a> {
        match self {
            JustifyItemsParseErrorOwned::InvalidValue(s) => {
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
pub fn parse_layout_justify_items<'a>(
    input: &'a str,
) -> Result<LayoutJustifyItems, JustifyItemsParseError<'a>> {
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
    pub inner: crate::props::basic::pixel::PixelValue,
}

impl core::fmt::Debug for LayoutGap {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(f, "{}", self.inner)
    }
}

impl crate::props::formatter::PrintAsCssValue for LayoutGap {
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
                LayoutGridAutoFlow::Row => "Row",
                LayoutGridAutoFlow::Column => "Column",
                LayoutGridAutoFlow::RowDense => "RowDense",
                LayoutGridAutoFlow::ColumnDense => "ColumnDense",
            }
        )
    }
}

impl FormatAsRustCode for LayoutJustifySelf {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        format!(
            "LayoutJustifySelf::{}",
            match self {
                LayoutJustifySelf::Auto => "Auto",
                LayoutJustifySelf::Start => "Start",
                LayoutJustifySelf::End => "End",
                LayoutJustifySelf::Center => "Center",
                LayoutJustifySelf::Stretch => "Stretch",
            }
        )
    }
}

impl FormatAsRustCode for LayoutJustifyItems {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        format!(
            "LayoutJustifyItems::{}",
            match self {
                LayoutJustifyItems::Start => "Start",
                LayoutJustifyItems::End => "End",
                LayoutJustifyItems::Center => "Center",
                LayoutJustifyItems::Stretch => "Stretch",
            }
        )
    }
}

impl FormatAsRustCode for LayoutGap {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        // LayoutGap wraps a PixelValue which implements FormatAsRustCode via helpers;
        // print as LayoutGap::Exact(LAYERVALUE) is not required here — use the CSS string
        format!("LayoutGap::Exact({})", self.inner)
    }
}

impl FormatAsRustCode for GridTrackSizing {
    fn format_as_rust_code(&self, tabs: usize) -> String {
        use crate::format_rust_code::format_pixel_value;
        match self {
            GridTrackSizing::Fixed(pv) => {
                format!("GridTrackSizing::Fixed({})", format_pixel_value(pv))
            }
            GridTrackSizing::Fr(f) => format!("GridTrackSizing::Fr({})", f),
            GridTrackSizing::MinContent => "GridTrackSizing::MinContent".to_string(),
            GridTrackSizing::MaxContent => "GridTrackSizing::MaxContent".to_string(),
            GridTrackSizing::Auto => "GridTrackSizing::Auto".to_string(),
            GridTrackSizing::MinMax(minmax) => {
                format!(
                    "GridTrackSizing::MinMax(GridMinMax {{ min: Box::new({}), max: Box::new({}) }})",
                    minmax.min.format_as_rust_code(tabs),
                    minmax.max.format_as_rust_code(tabs)
                )
            }
            GridTrackSizing::FitContent(pv) => {
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
pub fn parse_layout_gap<'a>(
    input: &'a str,
) -> Result<LayoutGap, crate::props::basic::pixel::CssPixelValueParseError<'a>> {
    crate::props::basic::pixel::parse_pixel_value(input).map(|p| LayoutGap { inner: p })
}

#[cfg(feature = "parser")]
pub fn parse_grid_line_owned(input: &str) -> Result<GridLine, ()> {
    let input = input.trim();

    if input == "auto" {
        return Ok(GridLine::Auto);
    }

    if input.starts_with("span ") {
        let num_str = &input[5..].trim();
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
    [Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
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
        GridTemplateAreas { areas: GridAreaDefinitionVec::from_vec(Vec::new()) }
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
            for r in (area.row_start as usize - 1)..(area.row_end as usize - 1) {
                for c in (area.column_start as usize - 1)..(area.column_end as usize - 1) {
                    if r < num_rows && c < num_cols {
                        grid[r][c] = area.name.as_str().to_string();
                    }
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
pub fn parse_grid_template_areas(input: &str) -> Result<GridTemplateAreas, ()> {
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
            let cells: Vec<String> = row_str.split_whitespace().map(|s| s.to_string()).collect();
            if cells.is_empty() {
                return Err(());
            }
            rows.push(cells);
            i += 1; // skip closing quote
        } else {
            i += 1;
        }
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
    use alloc::collections::BTreeMap;
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
            row_start: (min_row + 1) as u16,
            row_end: (max_row + 2) as u16,   // end line is one past the last cell
            column_start: (min_col + 1) as u16,
            column_end: (max_col + 2) as u16,
        });
    }

    Ok(GridTemplateAreas { areas: GridAreaDefinitionVec::from_vec(areas) })
}
