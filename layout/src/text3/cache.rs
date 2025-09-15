use std::{
    any::{Any, TypeId},
    collections::hash_map::{DefaultHasher, Entry, HashMap},
    hash::{Hash, Hasher},
    mem::discriminant,
    sync::Arc,
};

use azul_css::Shape;
use hyphenation::{Hyphenator, Language, Load, Standard};
use unicode_bidi::{BidiInfo, Level, TextSource};
use unicode_segmentation::UnicodeSegmentation;

// Re-use core types from the main module.
// Assuming `mod.rs` makes these types public (`pub`).
use crate::text3::{
    perform_bidi_analysis, shape_visual_runs, BidiLevel, CharacterClass, Color, Direction,
    FontLoaderTrait, FontManager, FontRef, FontVariantCaps, FontVariantEastAsian,
    FontVariantLigatures, FontVariantNumeric, FourCc, InlineBreak, JustifyContent, LayoutError,
    ParsedFontTrait, Point, Rect, Script, SegmentAlignment, StyleProperties, StyledRun, TextAlign,
    TextDecoration, TextOrientation, TextTransform, UnifiedConstraints, VisualRun, WritingMode,
};
use crate::text3::{
    script::script_to_language, FontProviderTrait, Glyph, InlineContent, LineConstraints,
    LineSegment, OverflowBehavior, PathSegment, ShapeBoundary, ShapeDefinition, Size, Spacing,
    TextCombineUpright, VerticalAlign,
};

// --- Core Data Structures for the New Architecture ---

// Add this new struct for style overrides
#[derive(Debug, Clone)]
pub struct StyleOverride {
    /// The specific character this override applies to.
    pub target: ContentIndex,
    /// The style properties to apply.
    /// Any `None` value means "inherit from the base style".
    pub style: PartialStyleProperties,
}

#[derive(Debug, Clone, Default)]
pub struct PartialStyleProperties {
    pub font_ref: Option<FontRef>,
    pub font_size_px: Option<f32>,
    pub color: Option<Color>,
    pub letter_spacing: Option<Spacing>,
    pub word_spacing: Option<Spacing>,
    pub line_height: Option<f32>,
    pub text_decoration: Option<TextDecoration>,
    pub font_features: Option<Vec<String>>,
    pub font_variations: Option<Vec<(FourCc, f32)>>,
    pub tab_size: Option<f32>,
    pub text_transform: Option<TextTransform>,
    pub writing_mode: Option<WritingMode>,
    pub text_orientation: Option<TextOrientation>,
    pub text_combine_upright: Option<Option<TextCombineUpright>>,
    pub font_variant_caps: Option<FontVariantCaps>,
    pub font_variant_numeric: Option<FontVariantNumeric>,
    pub font_variant_ligatures: Option<FontVariantLigatures>,
    pub font_variant_east_asian: Option<FontVariantEastAsian>,
}

impl Hash for PartialStyleProperties {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.font_ref.hash(state);
        self.font_size_px.map(|f| f.to_bits()).hash(state);
        self.color.hash(state);
        self.letter_spacing.hash(state);
        self.word_spacing.hash(state);
        self.line_height.map(|f| f.to_bits()).hash(state);
        self.text_decoration.hash(state);
        self.font_features.hash(state);

        // Manual hashing for Vec<(FourCc, f32)>
        self.font_variations.as_ref().map(|v| {
            for (tag, val) in v {
                tag.hash(state);
                val.to_bits().hash(state);
            }
        });

        self.tab_size.map(|f| f.to_bits()).hash(state);
        self.text_transform.hash(state);
        self.writing_mode.hash(state);
        self.text_orientation.hash(state);
        self.text_combine_upright.hash(state);
        self.font_variant_caps.hash(state);
        self.font_variant_numeric.hash(state);
        self.font_variant_ligatures.hash(state);
        self.font_variant_east_asian.hash(state);
    }
}

impl PartialEq for PartialStyleProperties {
    fn eq(&self, other: &Self) -> bool {
        self.font_ref == other.font_ref &&
        self.font_size_px.map(|f| f.to_bits()) == other.font_size_px.map(|f| f.to_bits()) &&
        self.color == other.color &&
        self.letter_spacing == other.letter_spacing &&
        self.word_spacing == other.word_spacing &&
        self.line_height.map(|f| f.to_bits()) == other.line_height.map(|f| f.to_bits()) &&
        self.text_decoration == other.text_decoration &&
        self.font_features == other.font_features &&
        self.font_variations == other.font_variations && // Vec<(FourCc, f32)> is PartialEq
        self.tab_size.map(|f| f.to_bits()) == other.tab_size.map(|f| f.to_bits()) &&
        self.text_transform == other.text_transform &&
        self.writing_mode == other.writing_mode &&
        self.text_orientation == other.text_orientation &&
        self.text_combine_upright == other.text_combine_upright &&
        self.font_variant_caps == other.font_variant_caps &&
        self.font_variant_numeric == other.font_variant_numeric &&
        self.font_variant_ligatures == other.font_variant_ligatures &&
        self.font_variant_east_asian == other.font_variant_east_asian
    }
}

impl Eq for PartialStyleProperties {}

impl StyleProperties {
    fn apply_override(&self, partial: &PartialStyleProperties) -> Self {
        let mut new_style = self.clone();
        if let Some(val) = &partial.font_ref {
            new_style.font_ref = val.clone();
        }
        if let Some(val) = partial.font_size_px {
            new_style.font_size_px = val;
        }
        if let Some(val) = &partial.color {
            new_style.color = val.clone();
        }
        if let Some(val) = partial.letter_spacing {
            new_style.letter_spacing = val;
        }
        if let Some(val) = partial.word_spacing {
            new_style.word_spacing = val;
        }
        if let Some(val) = partial.line_height {
            new_style.line_height = val;
        }
        if let Some(val) = &partial.text_decoration {
            new_style.text_decoration = val.clone();
        }
        if let Some(val) = &partial.font_features {
            new_style.font_features = val.clone();
        }
        if let Some(val) = &partial.font_variations {
            new_style.font_variations = val.clone();
        }
        if let Some(val) = partial.tab_size {
            new_style.tab_size = val;
        }
        if let Some(val) = partial.text_transform {
            new_style.text_transform = val;
        }
        if let Some(val) = partial.writing_mode {
            new_style.writing_mode = val;
        }
        if let Some(val) = partial.text_orientation {
            new_style.text_orientation = val;
        }
        if let Some(val) = &partial.text_combine_upright {
            new_style.text_combine_upright = val.clone();
        }
        if let Some(val) = partial.font_variant_caps {
            new_style.font_variant_caps = val;
        }
        if let Some(val) = partial.font_variant_numeric {
            new_style.font_variant_numeric = val;
        }
        if let Some(val) = partial.font_variant_ligatures {
            new_style.font_variant_ligatures = val;
        }
        if let Some(val) = partial.font_variant_east_asian {
            new_style.font_variant_east_asian = val;
        }
        new_style
    }
}

/// The kind of a glyph, used to distinguish characters from layout-inserted items.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GlyphKind {
    /// A standard glyph representing one or more characters from the source text.
    Character,
    /// A hyphen glyph inserted by the line breaking algorithm.
    Hyphen,
    /// A `.notdef` glyph, indicating a character that could not be found in any font.
    NotDef,
    /// A Kashida justification glyph, inserted to stretch Arabic text.
    Kashida {
        /// The target width of the kashida.
        width: f32,
    },
}

/// A stable, logical pointer to an item within the original `InlineContent` array.
///
/// This eliminates the need for string concatenation and byte-offset math.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ContentIndex {
    /// The index of the `InlineContent` run in the original input array.
    pub run_index: u32,
    /// The index of the character or item *within* that run.
    pub item_index: u32,
}

/// A stable, logical identifier for a grapheme cluster.
///
/// This survives Bidi reordering and line breaking, making it ideal for tracking
/// text positions for selection and cursor logic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct GraphemeClusterId {
    /// The `run_index` from the source `ContentIndex`.
    pub source_run: u32,
    /// The byte index of the start of the cluster in its original `StyledRun`.
    pub start_byte_in_run: u32,
}

// --- Stage 1: Logical Representation ---

#[derive(Debug, Clone)]
pub enum LogicalItem {
    Text {
        /// A stable ID pointing back to the original source character.
        source: ContentIndex,
        /// The text of this specific logical item (often a single grapheme cluster).
        text: String,
        style: Arc<StyleProperties>,
    },
    /// Tate-chu-yoko: Run of text to be laid out horizontally within a vertical context.
    CombinedText {
        source: ContentIndex,
        text: String,
        style: Arc<StyleProperties>,
    },
    Ruby {
        source: ContentIndex,
        // For the stub, we simplify to strings. A full implementation
        // would need to handle Vec<LogicalItem> for both.
        base_text: String,
        ruby_text: String,
        style: Arc<StyleProperties>,
    },
    Object {
        /// A stable ID pointing back to the original source object.
        source: ContentIndex,
        /// The original non-text object.
        content: InlineContent,
    },
    Tab {
        source: ContentIndex,
        style: Arc<StyleProperties>,
    },
    Break {
        source: ContentIndex,
        break_info: InlineBreak,
    },
}

impl Hash for LogicalItem {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        discriminant(self).hash(state);
        match self {
            LogicalItem::Text {
                source,
                text,
                style,
            } => {
                source.hash(state);
                text.hash(state);
                style.as_ref().hash(state); // Hash the content, not the Arc pointer
            }
            LogicalItem::CombinedText {
                source,
                text,
                style,
            } => {
                source.hash(state);
                text.hash(state);
                style.as_ref().hash(state);
            }
            LogicalItem::Ruby {
                source,
                base_text,
                ruby_text,
                style,
            } => {
                source.hash(state);
                base_text.hash(state);
                ruby_text.hash(state);
                style.as_ref().hash(state);
            }
            LogicalItem::Object { source, content } => {
                source.hash(state);
                content.hash(state);
            }
            LogicalItem::Tab { source, style } => {
                source.hash(state);
                style.as_ref().hash(state);
            }
            LogicalItem::Break { source, break_info } => {
                source.hash(state);
                break_info.hash(state);
            }
        }
    }
}

// --- Stage 2: Visual Representation ---

#[derive(Debug, Clone)]
pub struct VisualItem {
    /// A reference to the logical item this visual item originated from.
    /// A single LogicalItem can be split into multiple VisualItems.
    pub logical_source: LogicalItem,
    /// The Bidi embedding level for this item.
    pub bidi_level: BidiLevel,
    /// The script detected for this run, crucial for shaping.
    pub script: Script,
    /// The text content for this specific visual run.
    pub text: String,
}

// --- Stage 3: Shaped Representation ---

#[derive(Debug, Clone)]
pub enum ShapedItem<T: ParsedFontTrait> {
    Cluster(ShapedCluster<T>),
    /// A block of combined text (tate-chu-yoko) that is laid out as a single unbreakable object.
    CombinedBlock {
        source: ContentIndex,
        /// The glyphs to be rendered horizontally within the vertical line.
        glyphs: Vec<ShapedGlyph<T>>,
        bounds: Rect,
        baseline_offset: f32,
    },
    Object {
        source: ContentIndex,
        bounds: Rect,
        baseline_offset: f32,
        // Store original object for rendering
        content: InlineContent,
    },
    Tab {
        source: ContentIndex,
        bounds: Rect,
    },
    Break {
        source: ContentIndex,
        break_info: InlineBreak,
    },
}

impl<T: ParsedFontTrait> ShapedItem<T> {
    fn as_cluster(&self) -> Option<&ShapedCluster<T>> {
        match self {
            ShapedItem::Cluster(c) => Some(c),
            _ => None,
        }
    }
}

/// A group of glyphs that corresponds to one or more source characters (a cluster).
#[derive(Debug, Clone)]
pub struct ShapedCluster<T: ParsedFontTrait> {
    /// The original text that this cluster was shaped from.
    /// This is crucial for correct hyphenation.
    pub text: String,
    /// The ID of the grapheme cluster this glyph cluster represents.
    pub source_cluster_id: GraphemeClusterId,
    /// The source `ContentIndex` for mapping back to logical items.
    pub source_content_index: ContentIndex,
    /// The glyphs that make up this cluster.
    pub glyphs: Vec<ShapedGlyph<T>>,
    /// The total advance width (horizontal) or height (vertical) of the cluster.
    pub advance: f32,
    /// The direction of this cluster, inherited from its `VisualItem`.
    pub direction: Direction,
    /// Font style of this cluster
    pub style: Arc<StyleProperties>,
}

/// A single, shaped glyph with its essential metrics.
#[derive(Debug, Clone)]
pub struct ShapedGlyph<T: ParsedFontTrait> {
    /// The kind of glyph this is (character, hyphen, etc.).
    pub kind: GlyphKind,
    /// Glyph ID inside of the font
    pub glyph_id: u16,
    /// The byte offset of this glyph's source character(s) within its cluster text.
    pub cluster_offset: u32,
    /// The horizontal advance for this glyph (for horizontal text)
    pub advance: f32,
    /// The horizontal offset/bearing for this glyph
    pub offset: Point,
    /// The vertical advance for this glyph (for vertical text).
    pub vertical_advance: f32,
    /// The vertical offset/bearing for this glyph.
    pub vertical_offset: Point,
    pub script: Script,
    pub style: Arc<StyleProperties>,
    pub font: Arc<T>,
}

// --- Stage 4: Positioned Representation (Final Layout) ---

#[derive(Debug, Clone)]
pub struct PositionedItem<T: ParsedFontTrait> {
    pub item: ShapedItem<T>,
    pub position: Point,
    pub line_index: usize,
}

#[derive(Debug, Clone)]
pub struct UnifiedLayout<T: ParsedFontTrait> {
    pub items: Vec<PositionedItem<T>>,
    pub bounds: Rect,
    /// Information about content that did not fit.
    pub overflow: OverflowInfo<T>,
}

/// Stores information about content that exceeded the available layout space.
#[derive(Debug, Clone)]
pub struct OverflowInfo<T: ParsedFontTrait> {
    /// The items that did not fit within the constraints.
    pub overflow_items: Vec<ShapedItem<T>>,
    /// The total bounds of all content, including overflowing items.
    /// This is useful for `OverflowBehavior::Visible` or `Scroll`.
    pub unclipped_bounds: Rect,
}

impl<T: ParsedFontTrait> OverflowInfo<T> {
    pub fn has_overflow(&self) -> bool {
        !self.overflow_items.is_empty()
    }
}

impl<T: ParsedFontTrait> Default for OverflowInfo<T> {
    fn default() -> Self {
        Self {
            overflow_items: Vec::new(),
            unclipped_bounds: Rect::default(),
        }
    }
}

/// Intermediate structure carrying information from the line breaker to the positioner.
#[derive(Debug, Clone)]
pub struct UnifiedLine<T: ParsedFontTrait> {
    pub items: Vec<ShapedItem<T>>,
    /// The y-position (for horizontal) or x-position (for vertical) of the line's baseline.
    pub cross_axis_position: f32,
    /// The geometric segments this line must fit into.
    pub constraints: LineConstraints,
    pub is_last: bool,
}

// --- Caching Infrastructure ---

pub type CacheId = u64;

/// Defines a single area for layout, with its own shape and properties.
#[derive(Debug, Clone)]
pub struct LayoutFragment {
    /// A unique identifier for this fragment (e.g., "main-content", "sidebar").
    pub id: String,
    /// The geometric and style constraints for this specific fragment.
    pub constraints: UnifiedConstraints,
}

/// Represents the final layout distributed across multiple fragments.
#[derive(Debug, Clone)]
pub struct FlowLayout<T: ParsedFontTrait> {
    /// A map from a fragment's unique ID to the layout it contains.
    pub fragment_layouts: HashMap<String, Arc<UnifiedLayout<T>>>,
    /// Any items that did not fit into the last fragment in the flow chain.
    /// This is useful for pagination or determining if more layout space is needed.
    pub remaining_items: Vec<ShapedItem<T>>,
}

pub struct LayoutCache<T: ParsedFontTrait> {
    // Stage 1 Cache: InlineContent -> LogicalItems
    logical_items: HashMap<CacheId, Arc<Vec<LogicalItem>>>,
    // Stage 2 Cache: LogicalItems -> VisualItems
    visual_items: HashMap<CacheId, Arc<Vec<VisualItem>>>,
    // Stage 3 Cache: VisualItems -> ShapedItems (now strongly typed)
    shaped_items: HashMap<CacheId, Arc<Vec<ShapedItem<T>>>>,
    // Stage 4 Cache: ShapedItems + Constraints -> Final Layout (now strongly typed)
    layouts: HashMap<CacheId, Arc<UnifiedLayout<T>>>,
}

impl<T: ParsedFontTrait> LayoutCache<T> {
    pub fn new() -> Self {
        Self {
            logical_items: HashMap::new(),
            visual_items: HashMap::new(),
            shaped_items: HashMap::new(),
            layouts: HashMap::new(),
        }
    }
}

impl<T: ParsedFontTrait> Default for LayoutCache<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Key for caching the conversion from `InlineContent` to `LogicalItem`s.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct LogicalItemsKey<'a> {
    pub inline_content_hash: u64, // Pre-hash the content for efficiency
    pub default_font_size: u32,   // Affects space widths
    // Add other relevant properties from constraints if they affect this stage
    pub _marker: std::marker::PhantomData<&'a ()>,
}

/// Key for caching the Bidi reordering stage.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct VisualItemsKey {
    pub logical_items_id: CacheId,
    pub base_direction: Direction,
}

/// Key for caching the shaping stage.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct ShapedItemsKey {
    pub visual_items_id: CacheId,
    pub style_hash: u64, // Represents a hash of all font/style properties
}

impl ShapedItemsKey {
    pub fn new(visual_items_id: CacheId, visual_items: &[VisualItem]) -> Self {
        let style_hash = {
            let mut hasher = DefaultHasher::new();
            for item in visual_items.iter() {
                // Hash the style from the logical source, as this is what determines the font.
                match &item.logical_source {
                    LogicalItem::Text { style, .. } | LogicalItem::CombinedText { style, .. } => {
                        style.as_ref().hash(&mut hasher);
                    }
                    _ => {}
                }
            }
            hasher.finish()
        };

        Self {
            visual_items_id,
            style_hash,
        }
    }
}

/// Key for the final layout stage.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct LayoutKey {
    pub shaped_items_id: CacheId,
    pub constraints: UnifiedConstraints,
}

/// Helper to create a `CacheId` from any `Hash`able type.
fn calculate_id<T: Hash>(item: &T) -> CacheId {
    let mut hasher = DefaultHasher::new();
    item.hash(&mut hasher);
    hasher.finish()
}

// --- Main Layout Pipeline Implementation ---

impl<T: ParsedFontTrait> LayoutCache<T> {

    /// New top-level entry point for flowing layout across multiple regions.
    ///
    /// This function orchestrates the entire layout pipeline, but instead of fitting
    /// content into a single set of constraints, it flows the content through an
    /// ordered sequence of `LayoutFragment`s.
    ///
    /// # Arguments
    /// * `content` - The raw `InlineContent` to be laid out.
    /// * `style_overrides` - Character-level style changes.
    /// * `flow_chain` - An ordered slice of `LayoutFragment` defining the regions
    ///   (e.g., columns, pages) that the content should flow through.
    /// * `font_manager` - The font provider.
    ///
    /// # Returns
    /// A `FlowLayout` struct containing the positioned items for each fragment that
    /// was filled, and any content that did not fit in the final fragment.
    pub fn layout_flow<Q: FontLoaderTrait<T>>(
        &mut self,
        content: &[InlineContent],
        style_overrides: &[StyleOverride],
        flow_chain: &[LayoutFragment],
        font_manager: &FontManager<T, Q>,
    ) -> Result<FlowLayout<T>, LayoutError> {
        
        // --- Stages 1-3: Preparation ---
        // These stages are independent of the final geometry. We perform them once
        // on the entire content block before flowing. Caching is used at each stage.

        // Stage 1: Logical Analysis (InlineContent -> LogicalItem)
        let logical_items_id = calculate_id(&content);
        let logical_items = self
            .logical_items
            .entry(logical_items_id)
            .or_insert_with(|| Arc::new(create_logical_items(content, style_overrides)))
            .clone();

        // Stage 2: Bidi Reordering (LogicalItem -> VisualItem)
        let base_direction = get_base_direction_from_logical(&logical_items);
        let visual_key = VisualItemsKey {
            logical_items_id,
            base_direction,
        };
        let visual_items_id = calculate_id(&visual_key);
        let visual_items = self
            .visual_items
            .entry(visual_items_id)
            .or_insert_with(|| {
                Arc::new(reorder_logical_items(&logical_items, base_direction).unwrap())
            })
            .clone();

        // Stage 3: Shaping (VisualItem -> ShapedItem)
        let shaped_key = ShapedItemsKey::new(visual_items_id, &visual_items);
        let shaped_items_id = calculate_id(&shaped_key);
        let shaped_items = match self.shaped_items.get(&shaped_items_id) {
            Some(cached) => cached.clone(),
            None => {
                let items = Arc::new(shape_visual_items(&visual_items, font_manager)?);
                self.shaped_items.insert(shaped_items_id, items.clone());
                items
            }
        };

        // --- Stage 4: Apply Vertical Text Transformations ---
        
        // TODO: This orients all text based on the constraints of the *first* fragment.
        // A more advanced system could defer orientation until inside the loop if
        // fragments can have different writing modes.
        let default_constraints = UnifiedConstraints::default();
        let first_constraints = flow_chain.first().map(|f| &f.constraints).unwrap_or(&default_constraints);
        let oriented_items = apply_text_orientation(shaped_items, first_constraints)?;
        
        // --- Stage 5: The Flow Loop ---

        let mut fragment_layouts = HashMap::new();
        // The cursor now manages the stream of items for the entire flow.
        let mut cursor = BreakCursor::new(&oriented_items);

        for fragment in flow_chain {
            if cursor.is_done() {
                break; // All content has been laid out.
            }

            // Perform layout for this single fragment, consuming items from the cursor.
            let fragment_layout = perform_fragment_layout(
                &mut cursor,
                &logical_items,
                &fragment.constraints,
            )?;
            
            fragment_layouts.insert(fragment.id.clone(), Arc::new(fragment_layout));
        }

        Ok(FlowLayout {
            fragment_layouts,
            remaining_items: cursor.drain_remaining(),
        })
    }
}

// --- Stage 1 Implementation ---
fn create_logical_items(
    content: &[InlineContent],
    style_overrides: &[StyleOverride],
) -> Vec<LogicalItem> {
    let mut items = Vec::new();

    // 1. PRE-COMPUTATION: Organize overrides by run_index for fast lookups.
    let mut run_overrides: HashMap<u32, Vec<(u32, &PartialStyleProperties)>> = HashMap::new();
    for override_item in style_overrides {
        run_overrides
            .entry(override_item.target.run_index)
            .or_default()
            .push((override_item.target.item_index, &override_item.style));
    }

    // Sort overrides by their item_index (byte offset) to enable linear scanning.
    for overrides in run_overrides.values_mut() {
        overrides.sort_by_key(|(item_index, _)| *item_index);
    }

    let mut style_cache: HashMap<u64, Arc<StyleProperties>> = HashMap::new();

    for (run_idx, inline_item) in content.iter().enumerate() {
        match inline_item {
            InlineContent::Text(run) => {
                let text = &run.text;
                if text.is_empty() {
                    continue;
                }

                let mut byte_cursor = 0;
                // Get the sorted overrides for this specific run.
                let current_run_overrides = run_overrides.get(&(run_idx as u32));
                let mut override_cursor = 0;

                while byte_cursor < text.len() {
                    // --- A. Determine the style for the item starting at `byte_cursor` ---
                    let current_index = ContentIndex {
                        run_index: run_idx as u32,
                        item_index: byte_cursor as u32,
                    };
                    let style_override = current_run_overrides.and_then(|overrides| {
                        // Find the override that applies to the current cursor position.
                        // Since they are sorted, we can advance our override_cursor.
                        while let Some((offset, _)) = overrides.get(override_cursor) {
                            if (*offset as usize) < byte_cursor {
                                override_cursor += 1;
                            } else {
                                break;
                            }
                        }
                        if let Some((offset, style)) = overrides.get(override_cursor) {
                            if (*offset as usize) == byte_cursor {
                                Some(*style)
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    });

                    let style_to_use = match style_override {
                        None => run.style.clone(),
                        Some(partial_style) => {
                            let mut hasher = DefaultHasher::new();
                            Arc::as_ptr(&run.style).hash(&mut hasher);
                            partial_style.hash(&mut hasher);
                            style_cache
                                .entry(hasher.finish())
                                .or_insert_with(|| {
                                    Arc::new(run.style.apply_override(partial_style))
                                })
                                .clone()
                        }
                    };

                    // --- B. Prioritize and handle special items at the cursor ---
                    let first_grapheme = text[byte_cursor..].graphemes(true).next().unwrap();

                    // Case 1: Tab character.
                    if first_grapheme == "\t" {
                        items.push(LogicalItem::Tab {
                            source: current_index,
                            style: style_to_use, // Tabs can now have styles applied.
                        });
                        byte_cursor += first_grapheme.len();
                        continue;
                    }

                    // Case 2: Text-Combine-Upright sequence.
                    let combine_digits = if let Some(TextCombineUpright::Digits(n)) =
                        style_to_use.text_combine_upright
                    {
                        n
                    } else {
                        0
                    };
                    if combine_digits > 0 && first_grapheme.chars().all(|c| c.is_ascii_digit()) {
                        let mut combined_text = String::new();
                        let mut grapheme_iter =
                            text[byte_cursor..].grapheme_indices(true).peekable();

                        while combined_text.len() < combine_digits as usize {
                            if let Some((_, grapheme)) = grapheme_iter.peek() {
                                if grapheme.chars().all(|c| c.is_ascii_digit()) {
                                    combined_text.push_str(grapheme);
                                    grapheme_iter.next(); // Consume
                                } else {
                                    break;
                                }
                            } else {
                                break;
                            }
                        }

                        items.push(LogicalItem::CombinedText {
                            source: current_index,
                            text: combined_text.clone(),
                            style: style_to_use,
                        });
                        byte_cursor += combined_text.len(); // Advance past the whole sequence.
                        continue;
                    }

                    // --- C. Handle as a plain text chunk ---
                    // Scan forward to find the next break point.
                    // A break point is the *earliest* of: a tab, or the next style override.

                    let next_tab_offset = text[byte_cursor + 1..]
                        .find('\t')
                        .map(|i| byte_cursor + 1 + i);
                    let next_override_offset = current_run_overrides
                        .and_then(|overrides| overrides.get(override_cursor))
                        .map(|(offset, _)| *offset as usize);

                    let chunk_end = match (next_tab_offset, next_override_offset) {
                        (Some(t), Some(o)) => t.min(o),
                        (Some(t), None) => t,
                        (None, Some(o)) => o,
                        (None, None) => text.len(),
                    };

                    let text_slice = &text[byte_cursor..chunk_end];
                    items.push(LogicalItem::Text {
                        source: current_index,
                        text: text_slice.to_string(),
                        style: style_to_use,
                    });

                    byte_cursor = chunk_end;
                }
            }
            // --- Handle other non-text InlineContent types as before ---
            InlineContent::Ruby { base, text, style } => {
                let base_text = base
                    .iter()
                    .filter_map(|c| {
                        if let InlineContent::Text(t) = c {
                            Some(t.text.as_str())
                        } else {
                            None
                        }
                    })
                    .collect();
                let ruby_text = text
                    .iter()
                    .filter_map(|c| {
                        if let InlineContent::Text(t) = c {
                            Some(t.text.as_str())
                        } else {
                            None
                        }
                    })
                    .collect();
                items.push(LogicalItem::Ruby {
                    source: ContentIndex {
                        run_index: run_idx as u32,
                        item_index: 0,
                    },
                    base_text,
                    ruby_text,
                    style: style.clone(),
                });
            }
            InlineContent::LineBreak(br) => {
                items.push(LogicalItem::Break {
                    source: ContentIndex {
                        run_index: run_idx as u32,
                        item_index: 0,
                    },
                    break_info: br.clone(),
                });
            }
            _ => {
                items.push(LogicalItem::Object {
                    source: ContentIndex {
                        run_index: run_idx as u32,
                        item_index: 0,
                    },
                    content: inline_item.clone(),
                });
            }
        }
    }
    items
}

// --- Stage 2 Implementation ---

pub fn get_base_direction_from_logical(logical_items: &[LogicalItem]) -> Direction {
    let first_strong = logical_items.iter().find_map(|item| {
        if let LogicalItem::Text { text, .. } = item {
            Some(unicode_bidi::get_base_direction(text.as_str()))
        } else {
            None
        }
    });

    match first_strong {
        Some(unicode_bidi::Direction::Rtl) => Direction::Rtl,
        _ => Direction::Ltr,
    }
}

fn reorder_logical_items(
    logical_items: &[LogicalItem],
    base_direction: Direction,
) -> Result<Vec<VisualItem>, LayoutError> {
    // 1. Create a temporary string for the Bidi algorithm.
    //
    // PERFORMANCE NOTE: This is a single allocation for the entire paragraph.
    //
    // While it would be ideal to avoid this, the `unicode-bidi` crate's API
    // requires a contiguous `&str`. Building a non-allocating structure
    // that satisfies this is non-trivial and often not a major bottleneck
    // compared to shaping and geometry calculations.
    let mut bidi_str = String::new();
    let mut item_map = Vec::new(); // Maps byte index in bidi_str to logical_item index
    for (idx, item) in logical_items.iter().enumerate() {
        let text = match item {
            LogicalItem::Text { text, .. } => text.as_str(),
            LogicalItem::CombinedText { text, .. } => text.as_str(),
            // Use the standard Object Replacement Character for non-text items.
            _ => "\u{FFFC}",
        };
        let start_byte = bidi_str.len();
        bidi_str.push_str(text);
        for _ in start_byte..bidi_str.len() {
            item_map.push(idx);
        }
    }

    if bidi_str.is_empty() {
        return Ok(Vec::new());
    }

    // 2. Run the Bidi algorithm.
    let bidi_level = if base_direction == Direction::Rtl {
        Some(Level::rtl())
    } else {
        None
    };
    let bidi_info = BidiInfo::new(&bidi_str, bidi_level);
    let para = &bidi_info.paragraphs[0];
    let (levels, visual_runs) = bidi_info.visual_runs(para, para.range.clone());

    // 3. Create VisualItems from the reordered runs, splitting by style.
    let mut visual_items = Vec::new();
    for run_range in visual_runs {
        let bidi_level = BidiLevel::new(levels[run_range.start].number());
        let mut sub_run_start = run_range.start;

        // Iterate through the bytes of the visual run to detect style changes.
        for i in (run_range.start + 1)..run_range.end {
            if item_map[i] != item_map[sub_run_start] {
                // Style boundary found. Finalize the previous sub-run.
                let logical_idx = item_map[sub_run_start];
                let logical_item = &logical_items[logical_idx];
                let text_slice = &bidi_str[sub_run_start..i];

                visual_items.push(VisualItem {
                    logical_source: logical_item.clone(),
                    bidi_level,
                    script: crate::text3::script::detect_script(text_slice)
                        .unwrap_or(Script::Latin),
                    text: text_slice.to_string(),
                });
                // Start a new sub-run.
                sub_run_start = i;
            }
        }

        // Add the last sub-run (or the only one if no style change occurred).
        let logical_idx = item_map[sub_run_start];
        let logical_item = &logical_items[logical_idx];
        let text_slice = &bidi_str[sub_run_start..run_range.end];

        visual_items.push(VisualItem {
            logical_source: logical_item.clone(),
            bidi_level,
            script: crate::text3::script::detect_script(text_slice).unwrap_or(Script::Latin),
            text: text_slice.to_string(),
        });
    }

    Ok(visual_items)
}

// --- Stage 3 Implementation ---

fn shape_visual_items<T: ParsedFontTrait, Q: FontLoaderTrait<T>>(
    visual_items: &[VisualItem],
    font_manager: &FontManager<T, Q>,
) -> Result<Vec<ShapedItem<T>>, LayoutError> {
    let mut shaped = Vec::new();

    for item in visual_items {
        match &item.logical_source {
            LogicalItem::Text { style, source, .. } => {
                let direction = if item.bidi_level.is_rtl() {
                    Direction::Rtl
                } else {
                    Direction::Ltr
                };
                let font = font_manager.load_font(&style.font_ref)?;
                let language = script_to_language(item.script, &item.text);

                let shaped_clusters = shape_text_correctly(
                    &item.text,
                    item.script,
                    language,
                    direction,
                    &font,
                    style,
                    *source,
                )?;
                shaped.extend(shaped_clusters.into_iter().map(ShapedItem::Cluster));
            }
            LogicalItem::Tab { source, style } => {
                // TODO: To get the space width accurately, we would need to shape
                // a space character with the current font.
                // For now, we approximate it as a fraction of the font size.
                let space_advance = style.font_size_px * 0.33;
                let tab_width = style.tab_size * space_advance;
                shaped.push(ShapedItem::Tab {
                    source: *source,
                    bounds: Rect {
                        x: 0.0,
                        y: 0.0,
                        width: tab_width,
                        height: 0.0,
                    },
                });
            }
            LogicalItem::Ruby {
                source,
                base_text,
                ruby_text,
                style,
            } => {
                // TODO: Implement Ruby layout. This is a major feature.
                // 1. Recursively call layout for the `base_text` to get its size.
                // 2. Recursively call layout for the `ruby_text` (with a smaller font from
                //    `style`).
                // 3. Position the ruby text bounds above/beside the base text bounds.
                // 4. Create a single `ShapedItem::Object` or `ShapedItem::CombinedBlock` that
                //    represents the combined metric bounds of the group, which will be used for
                //    line breaking and positioning on the main line.
                // For now, create a placeholder object.
                let placeholder_width = base_text.chars().count() as f32 * style.font_size_px * 0.6;
                shaped.push(ShapedItem::Object {
                    source: *source,
                    bounds: Rect {
                        x: 0.0,
                        y: 0.0,
                        width: placeholder_width,
                        height: style.line_height * 1.5,
                    },
                    baseline_offset: 0.0,
                    content: InlineContent::Text(StyledRun {
                        text: base_text.clone(),
                        style: style.clone(),
                        logical_start_byte: 0,
                    }),
                });
            }
            LogicalItem::CombinedText {
                style,
                source,
                text,
            } => {
                let font = font_manager.load_font(&style.font_ref)?;
                let language = script_to_language(item.script, &item.text);

                // Force LTR horizontal shaping for the combined block.
                let glyphs =
                    font.shape_text(text, item.script, language, Direction::Ltr, style.as_ref())?;

                let shaped_glyphs = glyphs
                    .into_iter()
                    .map(|g| ShapedGlyph {
                        kind: GlyphKind::Character,
                        glyph_id: g.glyph_id,
                        script: g.script,
                        font: g.font,
                        style: g.style,
                        cluster_offset: 0,
                        advance: g.advance,
                        offset: g.offset,
                        vertical_advance: g.vertical_advance,
                        vertical_offset: g.vertical_bearing,
                    })
                    .collect::<Vec<_>>();

                let total_width: f32 = shaped_glyphs.iter().map(|g| g.advance).sum();
                let bounds = Rect {
                    x: 0.0,
                    y: 0.0,
                    width: total_width,
                    height: style.line_height,
                };

                shaped.push(ShapedItem::CombinedBlock {
                    source: *source,
                    glyphs: shaped_glyphs,
                    bounds,
                    baseline_offset: 0.0,
                });
            }
            LogicalItem::Object {
                content, source, ..
            } => {
                let (bounds, baseline) = measure_inline_object(content)?;
                shaped.push(ShapedItem::Object {
                    source: *source,
                    bounds,
                    baseline_offset: baseline,
                    content: content.clone(),
                });
            }
            LogicalItem::Break { source, break_info } => {
                shaped.push(ShapedItem::Break {
                    source: *source,
                    break_info: break_info.clone(),
                });
            }
        }
    }
    Ok(shaped)
}

/// Helper to check if a cluster contains only hanging punctuation.
fn is_hanging_punctuation<T: ParsedFontTrait>(item: &ShapedItem<T>) -> bool {
    if let ShapedItem::Cluster(c) = item {
        if c.glyphs.len() == 1 {
            match c.text.as_str() {
                "." | "," | ":" | ";" => true,
                _ => false,
            }
        } else {
            false
        }
    } else {
        false
    }
}

/// A corrected shaping function that avoids the bugs identified in the critique.
fn shape_text_correctly<T: ParsedFontTrait>(
    text: &str,
    script: Script,
    language: hyphenation::Language,
    direction: Direction,
    font: &Arc<T>,
    style: &Arc<StyleProperties>,
    source_index: ContentIndex,
) -> Result<Vec<ShapedCluster<T>>, LayoutError> {
    let glyphs = font.shape_text(text, script, language, direction, style.as_ref())?;

    if glyphs.is_empty() {
        return Ok(Vec::new());
    }

    let mut clusters = Vec::new();

    // Group glyphs by cluster ID from the shaper.
    let mut current_cluster_glyphs = Vec::new();
    let mut cluster_id = glyphs[0].cluster;
    let mut cluster_start_byte_in_text = glyphs[0].logical_byte_index;

    for glyph in glyphs {
        if glyph.cluster != cluster_id {
            // Finalize previous cluster
            let advance = current_cluster_glyphs
                .iter()
                .map(|g: &Glyph<T>| g.advance)
                .sum();
            let cluster_text = &text[cluster_start_byte_in_text..glyph.logical_byte_index];

            clusters.push(ShapedCluster {
                text: cluster_text.to_string(), // Store original text for hyphenation
                source_cluster_id: GraphemeClusterId {
                    source_run: source_index.run_index,
                    start_byte_in_run: cluster_id,
                },
                source_content_index: source_index,
                glyphs: current_cluster_glyphs
                    .iter()
                    .map(|g| {
                        let source_char = text[g.logical_byte_index..]
                            .chars()
                            .next()
                            .unwrap_or('\u{FFFD}');
                        ShapedGlyph {
                            kind: if g.glyph_id == 0 {
                                GlyphKind::NotDef
                            } else {
                                GlyphKind::Character
                            },
                            glyph_id: g.glyph_id,
                            script: g.script,
                            font: g.font.clone(),
                            style: g.style.clone(),
                            cluster_offset: (g.logical_byte_index - cluster_start_byte_in_text)
                                as u32,
                            advance: g.advance,
                            vertical_advance: g.vertical_advance,
                            vertical_offset: g.vertical_bearing,
                            offset: g.offset,
                        }
                    })
                    .collect(),
                advance,
                direction,
                style: style.clone(),
            });
            current_cluster_glyphs.clear();
            cluster_id = glyph.cluster;
            cluster_start_byte_in_text = glyph.logical_byte_index;
        }
        current_cluster_glyphs.push(glyph);
    }

    // Finalize the last cluster
    if !current_cluster_glyphs.is_empty() {
        let advance = current_cluster_glyphs
            .iter()
            .map(|g: &Glyph<T>| g.advance)
            .sum();
        let cluster_text = &text[cluster_start_byte_in_text..];
        clusters.push(ShapedCluster {
            text: cluster_text.to_string(), // Store original text
            source_cluster_id: GraphemeClusterId {
                source_run: source_index.run_index,
                start_byte_in_run: cluster_id,
            },
            source_content_index: source_index,
            glyphs: current_cluster_glyphs
                .iter()
                .map(|g| {
                    let source_char = text[g.logical_byte_index..]
                        .chars()
                        .next()
                        .unwrap_or('\u{FFFD}');
                    ShapedGlyph {
                        kind: if g.glyph_id == 0 {
                            GlyphKind::NotDef
                        } else {
                            GlyphKind::Character
                        },
                        glyph_id: g.glyph_id,
                        font: g.font.clone(),
                        style: g.style.clone(),
                        script: g.script,
                        vertical_advance: g.vertical_advance,
                        vertical_offset: g.vertical_bearing,
                        cluster_offset: (g.logical_byte_index - cluster_start_byte_in_text) as u32,
                        advance: g.advance,
                        offset: g.offset,
                    }
                })
                .collect(),
            advance,
            direction,
            style: style.clone(),
        });
    }

    Ok(clusters)
}

/// Measures a non-text object, returning its bounds and baseline offset.
fn measure_inline_object(item: &InlineContent) -> Result<(Rect, f32), LayoutError> {
    match item {
        InlineContent::Image(img) => {
            let size = img.display_size.unwrap_or(img.intrinsic_size);
            Ok((
                Rect {
                    x: 0.0,
                    y: 0.0,
                    width: size.width,
                    height: size.height,
                },
                img.baseline_offset,
            ))
        }
        InlineContent::Shape(shape) => Ok((
            Rect {
                x: 0.0,
                y: 0.0,
                width: shape.size.width,
                height: shape.size.height,
            },
            shape.baseline_offset,
        )),
        InlineContent::Space(space) => Ok((
            Rect {
                x: 0.0,
                y: 0.0,
                width: space.width,
                height: 0.0,
            },
            0.0,
        )),
        _ => Err(LayoutError::InvalidText("Not a measurable object".into())),
    }
}

// --- Stage 4 Implementation: Vertical Text ---

/// Applies orientation and vertical metrics to glyphs if the writing mode is vertical.
fn apply_text_orientation<T: ParsedFontTrait>(
    items: Arc<Vec<ShapedItem<T>>>,
    constraints: &UnifiedConstraints,
) -> Result<Arc<Vec<ShapedItem<T>>>, LayoutError> {
    if !constraints.is_vertical() {
        return Ok(items);
    }

    let mut oriented_items = Vec::with_capacity(items.len());
    let writing_mode = constraints.writing_mode.unwrap_or_default();

    for item in items.iter() {
        match item {
            ShapedItem::Cluster(cluster) => {
                let mut new_cluster = cluster.clone();
                let mut total_vertical_advance = 0.0;

                for glyph in &mut new_cluster.glyphs {
                    if let Some(v_metrics) = glyph.font.get_vertical_metrics(glyph.glyph_id) {
                        glyph.vertical_advance = v_metrics.advance;
                        glyph.vertical_offset = Point {
                            x: v_metrics.bearing_x,
                            y: v_metrics.bearing_y,
                        };
                        total_vertical_advance += v_metrics.advance;
                    } else {
                        // Fallback: use line height for vertical advance.
                        let fallback_advance = cluster.style.line_height;
                        glyph.vertical_advance = fallback_advance;
                        // Center the glyph horizontally as a fallback
                        glyph.vertical_offset = Point {
                            x: -glyph.advance / 2.0,
                            y: 0.0,
                        };
                        total_vertical_advance += fallback_advance;
                    }
                }
                // The cluster's `advance` now represents vertical advance.
                new_cluster.advance = total_vertical_advance;
                oriented_items.push(ShapedItem::Cluster(new_cluster));
            }
            // Non-text objects also need their advance axis swapped.
            ShapedItem::Object {
                source,
                bounds,
                baseline_offset,
                content,
            } => {
                let mut new_bounds = *bounds;
                std::mem::swap(&mut new_bounds.width, &mut new_bounds.height);
                oriented_items.push(ShapedItem::Object {
                    source: *source,
                    bounds: new_bounds,
                    baseline_offset: *baseline_offset,
                    content: content.clone(),
                });
            }
            _ => oriented_items.push(item.clone()),
        }
    }

    Ok(Arc::new(oriented_items))
}

// --- Stage 5 & 6 Implementation: Combined Layout Pass ---
// This section replaces the previous simple line breaking and positioning logic.

/// Gets the ascent (distance from baseline to top) and descent (distance from baseline to bottom)
/// for a single item.
fn get_item_vertical_metrics<T: ParsedFontTrait>(item: &ShapedItem<T>) -> (f32, f32) {
    // (ascent, descent)
    match item {
        ShapedItem::Cluster(c) => {
            // For text, ascent/descent come from font metrics.
            // This is a simplification; a real implementation would find the max ascent/descent
            // among all glyphs.
            if let Some(glyph) = c.glyphs.first() {
                let metrics = glyph.font.get_font_metrics();
                let scale = glyph.style.font_size_px / metrics.units_per_em as f32;
                (metrics.ascent * scale, (-metrics.descent * scale).max(0.0))
            } else {
                (c.style.line_height, 0.0)
            }
        }
        ShapedItem::Object {
            bounds,
            baseline_offset,
            ..
        } => {
            // For an object, the "baseline" is an arbitrary point.
            // Ascent is the part above the baseline, descent is the part below.
            let ascent = bounds.height - *baseline_offset;
            let descent = *baseline_offset;
            (ascent, descent)
        }
        _ => (0.0, 0.0), // Breaks and other non-visible items don't affect line height.
    }
}

/// Calculates the maximum ascent and descent for an entire line of items.
/// This determines the "line box" used for vertical alignment.
fn calculate_line_metrics<T: ParsedFontTrait>(items: &[ShapedItem<T>]) -> (f32, f32) {
    // (max_ascent, max_descent)
    items
        .iter()
        .fold((0.0f32, 0.0f32), |(max_asc, max_desc), item| {
            let (item_asc, item_desc) = get_item_vertical_metrics(item);
            (max_asc.max(item_asc), max_desc.max(item_desc))
        })
}

/// Performs layout for a single fragment, consuming items from a `BreakCursor`.
///
/// This function contains the core line-breaking and positioning logic, but is
/// designed to operate on a portion of a larger content stream and within the
/// constraints of a single geometric area (a fragment).
///
/// The loop terminates when either the fragment is filled (e.g., runs out of
/// vertical space) or the content stream managed by the `cursor` is exhausted.
fn perform_fragment_layout<T: ParsedFontTrait>(
    cursor: &mut BreakCursor<T>, // Takes a mutable cursor to consume items
    logical_items: &[LogicalItem],
    constraints: &UnifiedConstraints,
) -> Result<UnifiedLayout<T>, LayoutError> {
    
    let hyphenator = if constraints.hyphenation {
        constraints
            .hyphenation_language
            .and_then(|lang| get_hyphenator(lang).ok())
    } else {
        None
    };

    // NOTE: Knuth-Plass is not used here as it's designed for simple rectangular
    // layout and would require significant adaptation to consume from a cursor
    // and handle complex shapes. The complex line breaker is used by default.

    let mut positioned_items = Vec::new();
    let mut layout_bounds = Rect::default();
    
    // STUB: Handle Initial Letter / Drop Cap.
    // This logic should only run if this fragment is at the very beginning of the content stream.
    if cursor.is_at_start() {
        if let Some(_initial_letter_config) = &constraints.initial_letter {
            // TODO:
            // 1. Identify the first `initial_letter_config.count` items from the cursor.
            // 2. Shape them with a larger font size.
            // 3. Get their total bounding box.
            // 4. Create a temporary copy of constraints with this box added to `shape_exclusions`.
            // 5. Advance the `cursor` past the consumed items.
            // The rest of the layout loop will then naturally flow around the drop cap.
        }
    }
    
    // --- Column layout state ---
    let num_columns = constraints.columns.max(1);
    let total_column_gap = constraints.column_gap * (num_columns - 1) as f32;
    let column_width = (constraints.available_width - total_column_gap) / num_columns as f32;
    let mut current_column = 0;

    // Perf: Memoize line constraint calculations.
    let mut line_constraints_cache = HashMap::<u32, LineConstraints>::new();
    let base_direction = get_base_direction_from_logical(logical_items);
    let physical_align = resolve_logical_align(constraints.text_align, base_direction);
    let is_vertical = constraints.is_vertical();

    'column_loop: while current_column < num_columns {
        let column_start_x = (column_width + constraints.column_gap) * current_column as f32;
        let mut line_index = 0;
        let mut cross_axis_pen = 0.0;

        // Loop until the fragment is full OR the content stream is empty.
        while !cursor.is_done() {
            
            if let Some(clamp) = constraints.line_clamp {
                if line_index >= clamp.get() {
                    break 'column_loop;
                }
            }

            // Check if fragment is full and break the entire fragment layout if so.
            if let Some(max_height) = constraints.available_height {
                if cross_axis_pen >= max_height && constraints.overflow != OverflowBehavior::Visible {
                    break 'column_loop;
                }
            }

            let line_y_key = if is_vertical { 0.0 } else { cross_axis_pen };
            let line_constraints = line_constraints_cache
                .entry(line_y_key.to_bits())
                .or_insert_with(|| {
                    let mut col_constraints = constraints.clone();
                    col_constraints.available_width = column_width;
                    get_line_constraints(line_y_key, constraints.line_height, &col_constraints)
                });

            if line_constraints.segments.is_empty() {
                if constraints.available_height.is_none() {
                    break 'column_loop; // Avoid infinite loop if no height constraint
                }
                cross_axis_pen += constraints.line_height;
                continue;
            }

            let (line_items, was_hyphenated) = break_one_line(
                cursor, // Use the passed-in mutable cursor
                &line_constraints,
                is_vertical,
                hyphenator.as_ref(),
            );

            if line_items.is_empty() {
                break; // Can't fit anything more in this column.
            }

            // Justification depends on whether this is the last line of the entire paragraph.
            let is_last_line_in_flow = cursor.is_done() && !was_hyphenated;
            let (mut line_pos_items, line_height) = position_one_line(
                line_items,
                &line_constraints,
                cross_axis_pen,
                line_index,
                physical_align,
                is_last_line_in_flow,
                constraints,
            );

            for item in &mut line_pos_items {
                if !is_vertical {
                    item.position.x += column_start_x;
                } else {
                    item.position.y += column_start_x;
                }
            }

            line_index += 1;
            cross_axis_pen += line_height.max(constraints.line_height);

            for p_item in &line_pos_items {
                let item_bounds = get_item_bounds(p_item);
                layout_bounds.width = layout_bounds.width.max(item_bounds.x + item_bounds.width);
                layout_bounds.height = layout_bounds.height.max(item_bounds.y + item_bounds.height);
            }
            positioned_items.extend(line_pos_items);
        }
        current_column += 1;
    }

    // The items remaining in the cursor are the overflow for this fragment,
    // which will be handled by the parent `layout_flow` function.
    let unclipped_bounds = layout_bounds;
    
    Ok(UnifiedLayout {
        items: positioned_items,
        bounds: layout_bounds,
        // The overflow field of a single fragment's layout is always default.
        overflow: OverflowInfo {
            overflow_items: Vec::new(),
            unclipped_bounds,
        },
    })
}

/// Breaks a single line of items to fit within the given geometric constraints,
/// handling multi-segment lines and hyphenation.
fn break_one_line<T: ParsedFontTrait>(
    cursor: &mut BreakCursor<T>,
    line_constraints: &LineConstraints,
    is_vertical: bool,
    hyphenator: Option<&Standard>,
) -> (Vec<ShapedItem<T>>, bool) {
    // (line_items, was_hyphenated)
    let mut line_items = Vec::new();
    let mut current_item_iterator = 0;

    // Store the state at the last known safe break point (e.g., after a space).
    // Tuple: (index in `line_items` to break at, num items consumed from main slice).
    let mut last_safe_break: Option<(usize, usize)> = None;

    // --- Item Source Logic ---
    // Create a temporary, chained iterator for the line breaking logic.
    // It starts with any remainder from the last line, then continues with the main item slice.
    let mut potential_items = Vec::new();
    if let Some(rem) = cursor.partial_remainder.take() {
        potential_items.push(rem);
    }
    potential_items.extend_from_slice(&cursor.items[cursor.next_item_index..]);

    if potential_items.is_empty() {
        return (Vec::new(), false);
    }

    // --- Stage 1: Greedily fill all available segments ---

    'segment_loop: for segment in &line_constraints.segments {
        let mut current_segment_width = 0.0;

        loop {
            let item = match potential_items.get(current_item_iterator) {
                Some(item) => item,
                None => {
                    // All available items have been placed.
                    cursor.next_item_index = cursor.items.len();
                    return (line_items, false);
                }
            };

            // A hard break forces the line to end immediately.
            if let ShapedItem::Break { .. } = item {
                line_items.push(item.clone());
                // Consume the break item.
                if current_item_iterator == 0 && potential_items.len() > cursor.items.len() {
                    // It was a partial remainder, which is now consumed.
                } else {
                    cursor.next_item_index += 1;
                }
                return (line_items, false);
            }

            let item_measure = get_item_measure(item, is_vertical);

            // Check if the item overflows the *current* segment.
            if current_segment_width + item_measure > segment.width {
                // This segment is full. Break the inner loop to move to the next segment.
                break;
            }

            // --- Item fits in the current segment ---
            line_items.push(item.clone());
            current_segment_width += item_measure;
            current_item_iterator += 1;

            // If this item represents a natural break point, record our state.
            if is_break_opportunity(item) {
                let items_from_main_list = if potential_items.len() > cursor.items.len() {
                    current_item_iterator.saturating_sub(1)
                } else {
                    current_item_iterator
                };
                last_safe_break = Some((line_items.len(), items_from_main_list));
            }
        }
    }

    // --- Stage 2: Determine the final break point based on what was fitted ---

    // A word overflowed. Backtrack to the last safe break point.
    if let Some((break_at_line_idx, consumed_from_main)) = last_safe_break {
        line_items.truncate(break_at_line_idx);
        cursor.next_item_index += consumed_from_main;
        return (line_items, false);
    }

    // --- Stage 3: No safe break point found, attempt hyphenation ---

    // This case means the entire line content so far is one long, unbreakable word.
    // The word is the content of `line_items`, and it overflows.
    if let Some(hyphenator) = hyphenator {
        // The available width for hyphenation is the total width of all segments.
        let available_width = line_constraints.total_available;

        if let Some(hyphenation_result) =
            try_hyphenate_word_cluster(&line_items, available_width, is_vertical, hyphenator)
        {
            // Hyphenation was successful.
            // The cursor must be updated to consume all original items that formed the word.
            let items_in_word = line_items.len();
            let items_from_main_list = if potential_items.len() > cursor.items.len() {
                items_in_word.saturating_sub(1)
            } else {
                items_in_word
            };
            cursor.next_item_index += items_from_main_list;
            cursor.partial_remainder = Some(hyphenation_result.remainder_part);
            return (hyphenation_result.line_part, true);
        }
    }

    // --- Stage 4: Hyphenation failed or disabled, force a break ---

    // The line is empty, which means the very first item is too large for any segment.
    // We must place it on the line by itself to make progress and avoid an infinite loop.
    let first_item = potential_items[0].clone();

    // Update cursor to consume this one item.
    if potential_items.len() > cursor.items.len() {
        // The item was the partial_remainder, which is now consumed.
    } else {
        cursor.next_item_index += 1;
    }

    return (vec![first_item], false);
}

/// Represents a single valid hyphenation point within a word.
#[derive(Clone)]
pub struct HyphenationBreak<T: ParsedFontTrait> {
    /// The number of characters from the original word string included on the line.
    pub char_len_on_line: usize,
    /// The total advance width of the line part + the hyphen.
    pub width_on_line: f32,
    /// The cluster(s) that will remain on the current line.
    pub line_part: Vec<ShapedItem<T>>,
    /// The cluster that represents the hyphen character itself.
    pub hyphen_item: ShapedItem<T>,
    /// The cluster(s) that will be carried over to the next line.
    pub remainder_part: ShapedItem<T>,
}

/// A "word" is defined as a sequence of one or more adjacent ShapedClusters.
pub fn find_all_hyphenation_breaks<T: ParsedFontTrait>(
    word_clusters: &[ShapedCluster<T>],
    hyphenator: &Standard,
    is_vertical: bool, // Pass this in to use correct metrics
) -> Option<Vec<HyphenationBreak<T>>> {
    if word_clusters.is_empty() {
        return None;
    }

    // --- 1. Concatenate the TRUE text and build a robust map ---
    // This is the most critical part for correctness.
    let mut word_string = String::new();
    // Map from a character index in `word_string` to its source (cluster_idx, glyph_idx,
    // width_so_far)
    let mut char_map = Vec::new();
    let mut current_width = 0.0;

    for (cluster_idx, cluster) in word_clusters.iter().enumerate() {
        // We iterate by CHARACTERS, not glyphs. This is more stable.
        for (char_byte_offset, ch) in cluster.text.char_indices() {
            // Find which glyph this character belongs to.
            // This assumes glyphs are ordered by their cluster_offset.
            let glyph_idx = cluster
                .glyphs
                .iter()
                .rposition(|g| g.cluster_offset as usize <= char_byte_offset)
                .unwrap_or(0);
            let glyph = &cluster.glyphs[glyph_idx];

            // Apportion the glyph's width across its characters. This is an approximation,
            // but it's far more robust than the previous methods.
            let num_chars_in_glyph = cluster.text[glyph.cluster_offset as usize..]
                .chars()
                .count();
            let advance_per_char = if is_vertical {
                glyph.vertical_advance
            } else {
                glyph.advance
            } / (num_chars_in_glyph as f32);

            current_width += advance_per_char;
            char_map.push((cluster_idx, glyph_idx, current_width));
        }
        word_string.push_str(&cluster.text);
    }

    // --- 2. Get hyphenation opportunities ---
    let opportunities = hyphenator.hyphenate(&word_string);
    if opportunities.breaks.is_empty() {
        return None;
    }

    let last_cluster = word_clusters.last().unwrap();
    let last_glyph = last_cluster.glyphs.last().unwrap();
    let (font, style) = (last_glyph.font.clone(), last_cluster.style.clone());
    let (hyphen_glyph_id, hyphen_advance) =
        font.get_hyphen_glyph_and_advance(style.font_size_px)?;

    let mut possible_breaks = Vec::new();

    // --- 3. Generate a HyphenationBreak for each valid opportunity ---
    for &break_char_idx in &opportunities.breaks {
        if break_char_idx >= char_map.len() {
            continue;
        }

        let (break_cluster_idx, break_glyph_idx, width_at_break) = char_map[break_char_idx];

        // --- 4. Perform the split logic ---
        let cluster_to_split = &word_clusters[break_cluster_idx];
        let first_part_glyphs = cluster_to_split.glyphs[..=break_glyph_idx].to_vec();
        let second_part_glyphs = cluster_to_split.glyphs[break_glyph_idx + 1..].to_vec();

        if first_part_glyphs.is_empty() || second_part_glyphs.is_empty() {
            continue;
        }

        // Correctly slice the text using character indices.
        let split_byte_offset = word_string
            .char_indices()
            .nth(break_char_idx + 1)
            .map_or(word_string.len(), |(idx, _)| idx);
        let first_part_text = &word_string[..split_byte_offset];
        let second_part_text = &word_string[split_byte_offset..];

        let first_part_advance: f32 = first_part_glyphs.iter().map(|g| g.advance).sum();
        let second_part_advance: f32 = second_part_glyphs.iter().map(|g| g.advance).sum();

        // --- 5. Assemble the pieces ---
        let mut line_part: Vec<ShapedItem<T>> = word_clusters[..break_cluster_idx]
            .iter()
            .map(|c| ShapedItem::Cluster(c.clone()))
            .collect();
        line_part.push(ShapedItem::Cluster(ShapedCluster {
            text: first_part_text.to_string(),
            glyphs: first_part_glyphs,
            advance: first_part_advance,
            ..cluster_to_split.clone()
        }));

        let hyphen_item = ShapedItem::Cluster(ShapedCluster {
            text: "-".to_string(),
            source_cluster_id: GraphemeClusterId {
                source_run: u32::MAX,
                start_byte_in_run: u32::MAX,
            },
            source_content_index: ContentIndex {
                run_index: u32::MAX,
                item_index: u32::MAX,
            },
            glyphs: vec![ShapedGlyph {
                kind: GlyphKind::Hyphen,
                glyph_id: hyphen_glyph_id,
                font: font.clone(),
                cluster_offset: 0,
                script: Script::Latin,
                advance: hyphen_advance,
                offset: Point::default(),
                style: style.clone(),
                vertical_advance: hyphen_advance,
                vertical_offset: Point::default(),
            }],
            advance: hyphen_advance,
            direction: Direction::Ltr,
            style: style.clone(),
        });

        let remainder_part = ShapedItem::Cluster(ShapedCluster {
            text: second_part_text.to_string(),
            glyphs: second_part_glyphs,
            advance: second_part_advance,
            ..cluster_to_split.clone()
        });

        possible_breaks.push(HyphenationBreak {
            char_len_on_line: break_char_idx + 1,
            width_on_line: width_at_break + hyphen_advance,
            line_part,
            hyphen_item,
            remainder_part,
        });
    }

    Some(possible_breaks)
}

/// Tries to find a hyphenation point within a word, returning the line part and remainder.
fn try_hyphenate_word_cluster<T: ParsedFontTrait>(
    word_items: &[ShapedItem<T>],
    remaining_width: f32,
    is_vertical: bool,
    hyphenator: &Standard,
) -> Option<HyphenationResult<T>> {
    // Extract the ShapedCluster sequence from the word items.
    let word_clusters: Vec<ShapedCluster<T>> = word_items
        .iter()
        .filter_map(|item| item.as_cluster().cloned())
        .collect();

    if word_clusters.is_empty() {
        return None;
    }

    // Call the unified function to get all possible breaks.
    let all_breaks = find_all_hyphenation_breaks(&word_clusters, hyphenator, is_vertical)?;

    // Find the last break that fits within the available width.
    if let Some(best_break) = all_breaks
        .into_iter()
        .rfind(|b| b.width_on_line <= remaining_width)
    {
        let mut line_part = best_break.line_part;
        line_part.push(best_break.hyphen_item);

        return Some(HyphenationResult {
            line_part,
            remainder_part: best_break.remainder_part,
        });
    }

    None
}

/// Positions a single line of items, handling alignment within segments.
///
/// Returns positioned items and its line box height.
fn position_one_line<T: ParsedFontTrait>(
    line_items: Vec<ShapedItem<T>>, // Must own for justification
    line_constraints: &LineConstraints,
    cross_axis_pos: f32,
    line_index: usize,
    physical_align: TextAlign,
    is_last_line: bool,
    constraints: &UnifiedConstraints,
) -> (Vec<PositionedItem<T>>, f32) {
    if line_items.is_empty() {
        return (Vec::new(), 0.0);
    }
    let mut positioned = Vec::new();
    let is_vertical = constraints.is_vertical();

    let justified_items = if constraints.justify_content != JustifyContent::None
        && (!is_last_line || constraints.text_align == TextAlign::JustifyAll)
    {
        justify_line_items(
            line_items,
            line_constraints,
            constraints.justify_content,
            is_vertical,
        )
    } else {
        line_items
    };

    let (line_ascent, line_descent) = calculate_line_metrics(&justified_items);
    let line_box_height = line_ascent + line_descent;

    let mut main_axis_pen = calculate_alignment_offset(
        &justified_items,
        line_constraints,
        physical_align,
        is_vertical,
        constraints,
    );

    // TODO: Handle text-indent for the very first line of the paragraph.
    if line_index == 0 {
        main_axis_pen += constraints.text_indent;
    }

    // Handle hanging punctuation
    if constraints.hanging_punctuation && !is_vertical {
        // If left-aligned, check the last item.
        if (physical_align == TextAlign::Left || physical_align == TextAlign::Justify)
            && !justified_items.is_empty()
        {
            let last_item = &justified_items[justified_items.len() - 1];
            if is_hanging_punctuation(last_item) {
                let overhang = get_item_measure(last_item, is_vertical) / 2.0;
                if let Some(segment) = line_constraints.segments.first() {
                    let total_width: f32 = justified_items
                        .iter()
                        .map(|item| get_item_measure(item, is_vertical))
                        .sum();
                    if total_width - overhang < segment.width {
                        // Pretend the line is shorter to pull the punctuation out.
                        main_axis_pen -= overhang;
                    }
                }
            }
        }
        // If right-aligned, check the first item.
        if physical_align == TextAlign::Right && !justified_items.is_empty() {
            let first_item = &justified_items[0];
            if is_hanging_punctuation(first_item) {
                // Start the pen further left so the punctuation hangs to the right.
                main_axis_pen += get_item_measure(first_item, is_vertical) / 2.0;
            }
        }
    }

    if let Some(first_segment) = line_constraints.segments.first() {
        main_axis_pen += first_segment.start_x;

        for item in justified_items {
            let (item_ascent, item_descent) = get_item_vertical_metrics(&item);
            let item_cross_axis_pos = match constraints.vertical_align {
                VerticalAlign::Top => cross_axis_pos - line_ascent,
                VerticalAlign::Middle => {
                    (cross_axis_pos - line_ascent + (line_box_height / 2.0))
                        - ((item_ascent + item_descent) / 2.0)
                }
                VerticalAlign::Bottom => {
                    cross_axis_pos + line_descent - (item_ascent + item_descent)
                }
                _ => cross_axis_pos - item_ascent, // Baseline
            };

            let position = if is_vertical {
                Point {
                    x: item_cross_axis_pos,
                    y: main_axis_pen,
                }
            } else {
                Point {
                    x: main_axis_pen,
                    y: item_cross_axis_pos,
                }
            };

            let item_measure = get_item_measure(&item, is_vertical);
            positioned.push(PositionedItem {
                item,
                position,
                line_index,
            });
            main_axis_pen += item_measure;

            if let ShapedItem::Cluster(c) = &positioned.last().unwrap().item {
                // Resolve spacing units to pixels
                let letter_spacing_px = match c.style.letter_spacing {
                    Spacing::Px(px) => px as f32,
                    Spacing::Em(em) => em * c.style.font_size_px,
                };
                main_axis_pen += letter_spacing_px;

                if is_word_separator(&positioned.last().unwrap().item) {
                    let word_spacing_px = match c.style.word_spacing {
                        Spacing::Px(px) => px as f32,
                        Spacing::Em(em) => em * c.style.font_size_px,
                    };
                    main_axis_pen += word_spacing_px;
                }
            }
        }
    }

    (positioned, line_box_height)
}

/// Resolves logical alignment (start/end) to physical alignment (left/right).
fn resolve_logical_align(align: TextAlign, direction: Direction) -> TextAlign {
    match (align, direction) {
        (TextAlign::Start, Direction::Ltr) => TextAlign::Left,
        (TextAlign::Start, Direction::Rtl) => TextAlign::Right,
        (TextAlign::End, Direction::Ltr) => TextAlign::Right,
        (TextAlign::End, Direction::Rtl) => TextAlign::Left,
        (other, _) => other,
    }
}

/// Calculates the starting pen offset to achieve the desired text alignment.
fn calculate_alignment_offset<T: ParsedFontTrait>(
    items: &[ShapedItem<T>],
    line_constraints: &LineConstraints,
    align: TextAlign,
    is_vertical: bool,
    constraints: &UnifiedConstraints,
) -> f32 {
    // Simplified to use the first segment for alignment.
    if let Some(segment) = line_constraints.segments.first() {
        let total_width: f32 = items
            .iter()
            .map(|item| get_item_measure(item, is_vertical))
            .sum();

        let available_width = if constraints.segment_alignment == SegmentAlignment::Total {
            line_constraints.total_available
        } else {
            segment.width
        };

        if total_width >= available_width {
            return 0.0; // No alignment needed if line is full or overflows
        }

        let remaining_space = available_width - total_width;

        match align {
            TextAlign::Center => remaining_space / 2.0,
            TextAlign::Right => remaining_space,
            _ => 0.0, // Left, Justify, Start, End
        }
    } else {
        0.0
    }
}

/// Distributes extra space on a line according to the justification mode.
fn justify_line_items<T: ParsedFontTrait>(
    mut items: Vec<ShapedItem<T>>,
    line_constraints: &LineConstraints,
    justify_content: JustifyContent,
    is_vertical: bool,
) -> Vec<ShapedItem<T>> {
    let total_width: f32 = items
        .iter()
        .map(|item| get_item_measure(item, is_vertical))
        .sum();
    let available_width = line_constraints.total_available;

    if total_width >= available_width || available_width <= 0.0 {
        return items;
    }

    let extra_space = available_width - total_width;

    match justify_content {
        JustifyContent::InterWord => {
            let mut new_items = items;
            let space_indices: Vec<usize> = new_items
                .iter()
                .enumerate()
                .filter_map(|(i, item)| {
                    if is_word_separator(item) {
                        Some(i)
                    } else {
                        None
                    }
                })
                .collect();

            if !space_indices.is_empty() {
                let space_per_gap = extra_space / space_indices.len() as f32;
                for idx in space_indices {
                    match &mut new_items[idx] {
                        ShapedItem::Cluster(c) => c.advance += space_per_gap,
                        ShapedItem::Object { bounds, .. } => {
                            if is_vertical {
                                bounds.height += space_per_gap;
                            } else {
                                bounds.width += space_per_gap;
                            }
                        }
                        _ => {}
                    }
                }
            }
            new_items
        }
        JustifyContent::InterCharacter | JustifyContent::Distribute => {
            let mut new_items = items;
            // `Distribute` is often the same as `InterCharacter` in practice, but could
            // also add space at the start/end of the line, which is handled by alignment.
            let justifiable_gaps: Vec<usize> = new_items
                .iter()
                .enumerate()
                .filter_map(|(i, item)| {
                    if i < new_items.len() - 1 && can_justify_after(item) {
                        Some(i)
                    } else {
                        None
                    }
                })
                .collect();

            if !justifiable_gaps.is_empty() {
                let space_per_gap = extra_space / justifiable_gaps.len() as f32;
                for idx in justifiable_gaps {
                    match &mut new_items[idx] {
                        ShapedItem::Cluster(c) => c.advance += space_per_gap,
                        ShapedItem::Object { bounds, .. } => {
                            if is_vertical {
                                bounds.height += space_per_gap;
                            } else {
                                bounds.width += space_per_gap;
                            }
                        }
                        _ => {}
                    }
                }
            }
            new_items
        }
        JustifyContent::Kashida => {
            // 1. Find a font on the line that can provide kashida glyphs.
            let font_info = items.iter().find_map(|item| {
                if let ShapedItem::Cluster(c) = item {
                    if let Some(glyph) = c.glyphs.first() {
                        if glyph.script == Script::Arabic {
                            return Some((glyph.font.clone(), glyph.style.clone()));
                        }
                    }
                }
                None
            });

            let (font, style) = match font_info {
                Some(info) => info,
                None => return items, // No Arabic font on line, do nothing.
            };

            let (kashida_glyph_id, kashida_advance) =
                match font.get_kashida_glyph_and_advance(style.font_size_px) {
                    Some((id, adv)) if adv > 0.0 => (id, adv),
                    _ => return items, // Font does not support kashida justification.
                };

            // 2. Identify all valid insertion points (opportunities) for kashida.
            // A simple rule: between two Arabic clusters, where the second is not whitespace.
            let opportunity_indices: Vec<usize> = items
                .windows(2)
                .enumerate()
                .filter_map(|(i, window)| {
                    if let (ShapedItem::Cluster(cur), ShapedItem::Cluster(next)) =
                        (&window[0], &window[1])
                    {
                        if is_arabic_cluster(cur)
                            && is_arabic_cluster(next)
                            && !is_word_separator(&window[1])
                        {
                            // Store the index *after* the current item.
                            return Some(i + 1);
                        }
                    }
                    None
                })
                .collect();

            if opportunity_indices.is_empty() {
                return items;
            }

            // 3. Calculate how many kashidas to insert.
            let num_kashidas_to_insert = (extra_space / kashida_advance).floor() as usize;
            if num_kashidas_to_insert == 0 {
                return items;
            }

            let kashidas_per_point = num_kashidas_to_insert / opportunity_indices.len();
            let mut remainder = num_kashidas_to_insert % opportunity_indices.len();

            // 4. Create a template kashida item to clone.
            let kashida_item = {
                let kashida_glyph = ShapedGlyph {
                    kind: GlyphKind::Kashida {
                        width: kashida_advance,
                    },
                    glyph_id: kashida_glyph_id,
                    font,
                    style: style.clone(),
                    script: Script::Arabic,
                    advance: kashida_advance,
                    cluster_offset: 0,
                    offset: Point::default(),
                    vertical_advance: 0.0,
                    vertical_offset: Point::default(),
                };
                // Use placeholder source indices for generated items.
                ShapedItem::Cluster(ShapedCluster {
                    text: "\u{0640}".to_string(),
                    source_cluster_id: GraphemeClusterId {
                        source_run: u32::MAX,
                        start_byte_in_run: u32::MAX,
                    },
                    source_content_index: ContentIndex {
                        run_index: u32::MAX,
                        item_index: u32::MAX,
                    },
                    glyphs: vec![kashida_glyph],
                    advance: kashida_advance,
                    direction: Direction::Ltr, // Kashida itself has neutral direction.
                    style,
                })
            };

            // 5. Rebuild the items vector with kashidas inserted.
            let mut new_items = Vec::with_capacity(items.len() + num_kashidas_to_insert);
            let mut last_copy_idx = 0;
            for &point in &opportunity_indices {
                // Add the items from the original vec up to the insertion point.
                new_items.extend_from_slice(&items[last_copy_idx..point]);

                // Distribute the kashidas.
                let mut num_to_insert = kashidas_per_point;
                if remainder > 0 {
                    num_to_insert += 1;
                    remainder -= 1;
                }
                for _ in 0..num_to_insert {
                    new_items.push(kashida_item.clone());
                }

                // Update our cursor in the original `items` slice.
                last_copy_idx = point;
            }
            // Add any remaining items after the last insertion point.
            new_items.extend_from_slice(&items[last_copy_idx..]);

            return new_items;
        }
        JustifyContent::None => items,
    }
}

/// Helper to determine if a cluster belongs to the Arabic script.
fn is_arabic_cluster<T: ParsedFontTrait>(cluster: &ShapedCluster<T>) -> bool {
    // A cluster is considered Arabic if its first non-NotDef glyph is from the Arabic script.
    // This is a robust heuristic for mixed-script lines.
    cluster.glyphs.iter().any(|g| g.script == Script::Arabic)
}

/// Helper to identify if an item is a word separator (like a space).
pub fn is_word_separator<T: ParsedFontTrait>(item: &ShapedItem<T>) -> bool {
    if let ShapedItem::Cluster(c) = item {
        // A cluster is a word separator if its text is whitespace.
        // This is a simplification; a single glyph might be whitespace.
        c.text.chars().any(|g| g.is_whitespace())
    } else {
        false
    }
}

/// Helper to identify if space can be added after an item.
fn can_justify_after<T: ParsedFontTrait>(item: &ShapedItem<T>) -> bool {
    if let ShapedItem::Cluster(c) = item {
        c.text.chars().last().map_or(false, |g| {
            !g.is_whitespace() && classify_character(g as u32) != CharacterClass::Combining
        })
    } else {
        // Can generally justify after inline objects unless they are followed by a break.
        !matches!(item, ShapedItem::Break { .. })
    }
}

/// Classifies a character for layout purposes (e.g., justification behavior).
/// Copied from `mod.rs`.
fn classify_character(codepoint: u32) -> CharacterClass {
    match codepoint {
        0x0020 | 0x00A0 | 0x3000 => CharacterClass::Space,
        0x0021..=0x002F | 0x003A..=0x0040 | 0x005B..=0x0060 | 0x007B..=0x007E => {
            CharacterClass::Punctuation
        }
        0x4E00..=0x9FFF | 0x3400..=0x4DBF => CharacterClass::Ideograph,
        0x0300..=0x036F | 0x1AB0..=0x1AFF => CharacterClass::Combining,
        // Mongolian script range
        0x1800..=0x18AF => CharacterClass::Letter,
        _ => CharacterClass::Letter,
    }
}

/// Helper to get the primary measure (width or height) of a shaped item.
pub fn get_item_measure<T: ParsedFontTrait>(item: &ShapedItem<T>, is_vertical: bool) -> f32 {
    match item {
        ShapedItem::Cluster(c) => c.advance,
        ShapedItem::Object { bounds, .. }
        | ShapedItem::CombinedBlock { bounds, .. }
        | ShapedItem::Tab { bounds, .. } => {
            if is_vertical {
                bounds.height
            } else {
                bounds.width
            }
        }
        ShapedItem::Break { .. } => 0.0,
    }
}

/// Helper to get the final positioned bounds of an item.
fn get_item_bounds<T: ParsedFontTrait>(item: &PositionedItem<T>) -> Rect {
    let measure = get_item_measure(&item.item, false); // for simplicity, use horizontal
    let cross_measure = match &item.item {
        ShapedItem::Object { bounds, .. } => bounds.height,
        _ => 20.0, // placeholder line height
    };
    Rect {
        x: item.position.x,
        y: item.position.y,
        width: measure,
        height: cross_measure,
    }
}

/// Calculates the available horizontal segments for a line at a given vertical position,
/// considering both shape boundaries and exclusions.
fn get_line_constraints(
    line_y: f32,
    line_height: f32, // The height of the line is needed for accurate intersection tests
    constraints: &UnifiedConstraints,
) -> LineConstraints {
    // 1. Determine the initial available segments from the boundaries.
    let mut available_segments = Vec::new();

    if constraints.shape_boundaries.is_empty() {
        // Fallback to simple rectangular available_width if no complex shapes are defined.
        available_segments.push(LineSegment {
            start_x: 0.0,
            width: constraints.available_width,
            priority: 0,
        });
    } else {
        // Get segments from all defined boundaries.
        for boundary in &constraints.shape_boundaries {
            let boundary = boundary.inflate(constraints.exclusion_margin);
            let boundary_spans =
                get_shape_horizontal_spans(&boundary, line_y, line_height).unwrap_or_default();
            for (start, end) in boundary_spans {
                available_segments.push(LineSegment {
                    start_x: start,
                    width: end - start,
                    priority: 0,
                });
            }
        }
        // Merge potentially overlapping segments from different boundary shapes.
        available_segments = merge_segments(available_segments);
    }

    // 2. Iteratively subtract each exclusion from the current set of available segments.
    for exclusion in &constraints.shape_exclusions {
        let exclusion_spans =
            get_shape_horizontal_spans(exclusion, line_y, line_height).unwrap_or_default();
        if exclusion_spans.is_empty() {
            continue; // This exclusion is not on the current line.
        }

        let mut next_segments = Vec::new();
        for (excl_start, excl_end) in exclusion_spans {
            // Apply this exclusion span to all current segments.
            for segment in &available_segments {
                let seg_start = segment.start_x;
                let seg_end = segment.start_x + segment.width;

                // Case 1: The segment is entirely to the left of the exclusion.
                if seg_end <= excl_start {
                    next_segments.push(segment.clone());
                    continue;
                }
                // Case 2: The segment is entirely to the right of the exclusion.
                if seg_start >= excl_end {
                    next_segments.push(segment.clone());
                    continue;
                }

                // Case 3: The segment is split by the exclusion.
                if seg_start < excl_start && seg_end > excl_end {
                    // Left part
                    next_segments.push(LineSegment {
                        start_x: seg_start,
                        width: excl_start - seg_start,
                        priority: segment.priority,
                    });
                    // Right part
                    next_segments.push(LineSegment {
                        start_x: excl_end,
                        width: seg_end - excl_end,
                        priority: segment.priority,
                    });
                    continue;
                }

                // Case 4: The exclusion truncates the right side of the segment.
                if seg_start < excl_start {
                    next_segments.push(LineSegment {
                        start_x: seg_start,
                        width: excl_start - seg_start,
                        priority: segment.priority,
                    });
                }

                // Case 5: The exclusion truncates the left side of the segment.
                if seg_end > excl_end {
                    next_segments.push(LineSegment {
                        start_x: excl_end,
                        width: seg_end - excl_end,
                        priority: segment.priority,
                    });
                }

                // Case 6 (Implicit): The segment is completely contained within the exclusion.
                // In this case, nothing is added to next_segments.
            }
            // The result of this exclusion becomes the input for the next one.
            available_segments = merge_segments(next_segments);
            next_segments = Vec::new();
        }
    }

    let total_width = available_segments.iter().map(|s| s.width).sum();

    LineConstraints {
        segments: available_segments,
        total_available: total_width,
    }
}

/// Helper function to get the horizontal spans of any shape at a given y-coordinate.
/// Returns a list of (start_x, end_x) tuples.
fn get_shape_horizontal_spans(
    shape: &ShapeBoundary,
    y: f32,
    line_height: f32,
) -> Result<Vec<(f32, f32)>, LayoutError> {
    // For simplicity in intersection, we can test against the center of the line.
    let line_center_y = y + line_height / 2.0;

    match shape {
        ShapeBoundary::Rectangle(rect) => {
            if line_center_y >= rect.y && line_center_y < rect.y + rect.height {
                Ok(vec![(rect.x, rect.x + rect.width)])
            } else {
                Ok(vec![])
            }
        }
        ShapeBoundary::Circle { center, radius } => {
            let dy = (line_center_y - center.y).abs();
            if dy <= *radius {
                let dx = (radius.powi(2) - dy.powi(2)).sqrt();
                Ok(vec![(center.x - dx, center.x + dx)])
            } else {
                Ok(vec![])
            }
        }
        ShapeBoundary::Ellipse { center, radii } => {
            let dy = line_center_y - center.y;
            if dy.abs() <= radii.height {
                // Formula: (x-h)^2/a^2 + (y-k)^2/b^2 = 1
                let y_term = dy / radii.height;
                let x_term_squared = 1.0 - y_term.powi(2);
                if x_term_squared >= 0.0 {
                    let dx = radii.width * x_term_squared.sqrt();
                    Ok(vec![(center.x - dx, center.x + dx)])
                } else {
                    Ok(vec![])
                }
            } else {
                Ok(vec![])
            }
        }
        ShapeBoundary::Polygon { points } => {
            let segments = polygon_line_intersection(points, y, line_height)?;
            Ok(segments
                .iter()
                .map(|s| (s.start_x, s.start_x + s.width))
                .collect())
        }
        ShapeBoundary::Path { .. } => Ok(vec![]), // TODO!
    }
}

/// Merges overlapping or adjacent line segments into larger ones.
fn merge_segments(mut segments: Vec<LineSegment>) -> Vec<LineSegment> {
    if segments.len() <= 1 {
        return segments;
    }
    segments.sort_by(|a, b| a.start_x.partial_cmp(&b.start_x).unwrap());
    let mut merged = vec![segments[0].clone()];
    for next_seg in segments.iter().skip(1) {
        let last = merged.last_mut().unwrap();
        if next_seg.start_x <= last.start_x + last.width {
            let new_width = (next_seg.start_x + next_seg.width) - last.start_x;
            last.width = last.width.max(new_width);
        } else {
            merged.push(next_seg.clone());
        }
    }
    merged
}

// TODO: Dummy polygon function to make it compile
fn polygon_line_intersection(
    points: &[Point],
    y: f32,
    line_height: f32,
) -> Result<Vec<LineSegment>, LayoutError> {
    if points.len() < 3 {
        return Ok(vec![]);
    }

    let line_center_y = y + line_height / 2.0;
    let mut intersections = Vec::new();

    // Use winding number algorithm for robustness with complex polygons.
    for i in 0..points.len() {
        let p1 = points[i];
        let p2 = points[(i + 1) % points.len()];

        // Skip horizontal edges as they don't intersect a horizontal scanline in a meaningful way.
        if (p2.y - p1.y).abs() < f32::EPSILON {
            continue;
        }

        // Check if our horizontal scanline at `line_center_y` crosses this polygon edge.
        let crosses = (p1.y <= line_center_y && p2.y > line_center_y)
            || (p1.y > line_center_y && p2.y <= line_center_y);

        if crosses {
            // Calculate intersection x-coordinate using linear interpolation.
            let t = (line_center_y - p1.y) / (p2.y - p1.y);
            let x = p1.x + t * (p2.x - p1.x);
            intersections.push(x);
        }
    }

    // Sort intersections by x-coordinate to form spans.
    intersections.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    // Build segments from paired intersection points.
    let mut segments = Vec::new();
    for chunk in intersections.chunks_exact(2) {
        let start_x = chunk[0];
        let end_x = chunk[1];
        if end_x > start_x {
            segments.push(LineSegment {
                start_x,
                width: end_x - start_x,
                priority: 0,
            });
        }
    }

    Ok(segments)
}

// ADDITION: A helper function to get a hyphenator.
/// Helper to get a hyphenator for a given language.
/// TODO: In a real app, this would be cached.
fn get_hyphenator(language: Language) -> Result<Standard, LayoutError> {
    Standard::from_embedded(language).map_err(|e| LayoutError::HyphenationError(e.to_string()))
}

fn is_break_opportunity<T: ParsedFontTrait>(item: &ShapedItem<T>) -> bool {
    // Break after spaces or explicit break items.
    if is_word_separator(item) {
        return true;
    }
    if let ShapedItem::Break { .. } = item {
        return true;
    }
    // Also consider soft hyphens as opportunities.
    if let ShapedItem::Cluster(c) = item {
        if c.text.starts_with('\u{00AD}') {
            return true;
        }
    }
    false
}

// A cursor to manage the state of the line breaking process.
// This allows us to handle items that are partially consumed by hyphenation.
struct BreakCursor<'a, T: ParsedFontTrait> {
    /// A reference to the complete list of shaped items.
    items: &'a [ShapedItem<T>],
    /// The index of the next *full* item to be processed from the `items` slice.
    next_item_index: usize,
    /// The remainder of an item that was split by hyphenation on the previous line.
    /// This will be the very first piece of content considered for the next line.
    partial_remainder: Option<ShapedItem<T>>,
}

impl<'a, T: ParsedFontTrait> BreakCursor<'a, T> {
    fn new(items: &'a [ShapedItem<T>]) -> Self {
        Self {
            items,
            next_item_index: 0,
            partial_remainder: None,
        }
    }

    /// Checks if the cursor is at the very beginning of the content stream.
    pub fn is_at_start(&self) -> bool {
        self.next_item_index == 0 && self.partial_remainder.is_none()
    }

    /// Consumes the cursor and returns all remaining items as a `Vec`.
    fn drain_remaining(&mut self) -> Vec<ShapedItem<T>> {
        let mut remaining = Vec::new();
        if let Some(rem) = self.partial_remainder.take() {
            remaining.push(rem);
        }
        if self.next_item_index < self.items.len() {
            remaining.extend_from_slice(&self.items[self.next_item_index..]);
        }
        self.next_item_index = self.items.len();
        remaining
    }

    /// Checks if all content, including any partial remainders, has been processed.
    fn is_done(&self) -> bool {
        self.next_item_index >= self.items.len() && self.partial_remainder.is_none()
    }
}

// A structured result from a hyphenation attempt.
struct HyphenationResult<T: ParsedFontTrait> {
    /// The items that fit on the current line, including the new hyphen.
    line_part: Vec<ShapedItem<T>>,
    /// The remainder of the split item to be carried over to the next line.
    remainder_part: ShapedItem<T>,
}
