/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::{collections::HashMap, io::Write, marker::PhantomData, mem, ops::Range};

use euclid::SideOffsets2D;
#[cfg(feature = "deserialize")]
use serde::de::Deserializer;
#[cfg(feature = "serialize")]
use serde::ser::Serializer;
use serde::{Deserialize, Serialize};

// local imports
use crate::display_item as di;
use crate::{
    backport::precise_time_ns,
    color::ColorF,
    display_item_cache::*,
    font::{FontInstanceKey, GlyphInstance, GlyphOptions},
    gradient_builder::GradientBuilder,
    image::{ColorDepth, ImageKey},
    units::*,
    APZScrollGeneration, HasScrollLinkedEffect, PipelineId, PropertyBinding,
};

// We don't want to push a long text-run. If a text-run is too long, split it into several parts.
// This needs to be set to (renderer::MAX_VERTEX_TEXTURE_WIDTH - VECS_PER_TEXT_RUN) * 2
pub const MAX_TEXT_RUN_LENGTH: usize = 2040;

// See ROOT_REFERENCE_FRAME_SPATIAL_ID and ROOT_SCROLL_NODE_SPATIAL_ID
// TODO(mrobinson): It would be a good idea to eliminate the root scroll frame which is only
// used by Servo.
const FIRST_SPATIAL_NODE_INDEX: usize = 2;

// See ROOT_SCROLL_NODE_SPATIAL_ID
const FIRST_CLIP_NODE_INDEX: usize = 1;

#[derive(Debug, Copy, Clone, PartialEq)]
enum BuildState {
    Idle,
    Build,
}

#[repr(C)]
#[derive(Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct ItemRange<'a, T> {
    bytes: &'a [u8],
    _boo: PhantomData<T>,
}

impl<'a, T> Copy for ItemRange<'a, T> {}
impl<'a, T> Clone for ItemRange<'a, T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<'a, T> Default for ItemRange<'a, T> {
    fn default() -> Self {
        ItemRange {
            bytes: Default::default(),
            _boo: PhantomData,
        }
    }
}

impl<'a, T> ItemRange<'a, T> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Self {
            bytes,
            _boo: PhantomData,
        }
    }

    pub fn is_empty(&self) -> bool {
        // Nothing more than space for a length (0).
        self.bytes.len() <= mem::size_of::<usize>()
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

impl<'a, T: Default> ItemRange<'a, T> {
    pub fn iter(&self) -> AuxIter<'a, T> {
        AuxIter::new(T::default(), self.bytes)
    }
}

impl<'a, T> IntoIterator for ItemRange<'a, T>
where
    T: Copy + Default,
{
    type Item = T;
    type IntoIter = AuxIter<'a, T>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

#[derive(Copy, Clone)]
pub struct TempFilterData<'a> {
    pub func_types: ItemRange<'a, di::ComponentTransferFuncType>,
    pub r_values: ItemRange<'a, f32>,
    pub g_values: ItemRange<'a, f32>,
    pub b_values: ItemRange<'a, f32>,
    pub a_values: ItemRange<'a, f32>,
}

#[derive(Default, Clone)]
pub struct DisplayListPayload {
    /// Direct storage of display items (no serialization needed)
    pub items: Vec<di::DisplayItem>,

    /// Direct storage of spatial tree items (no serialization needed)
    pub spatial_items: Vec<di::SpatialTreeItem>,

    // === Auxiliary data for complex items ===
    /// Glyphs for text items - each Text item references a range in this vec
    pub glyphs: Vec<GlyphInstance>,

    /// Gradient stops - each gradient references a range in this vec
    pub stops: Vec<di::GradientStop>,

    /// Filter operations for stacking contexts
    pub filters: Vec<di::FilterOp>,

    /// Filter data (for component transfer)
    pub filter_data: Vec<di::FilterData>,

    /// Filter primitives (for SVG filters)
    pub filter_primitives: Vec<di::FilterPrimitive>,

    /// Clip IDs for clip chains
    pub clip_chain_items: Vec<di::ClipId>,

    /// Points for polygon clips
    pub points: Vec<LayoutPoint>,
}

impl DisplayListPayload {
    fn default() -> Self {
        DisplayListPayload {
            items: Vec::new(),
            spatial_items: Vec::new(),
            glyphs: Vec::new(),
            stops: Vec::new(),
            filters: Vec::new(),
            filter_data: Vec::new(),
            filter_primitives: Vec::new(),
            clip_chain_items: Vec::new(),
            points: Vec::new(),
        }
    }

    fn new(_capacity: DisplayListCapacity) -> Self {
        // Capacity hints are no longer used - just return default
        Self::default()
    }

    fn clear(&mut self) {
        self.items.clear();
        self.spatial_items.clear();
        self.glyphs.clear();
        self.stops.clear();
        self.filters.clear();
        self.filter_data.clear();
        self.filter_primitives.clear();
        self.clip_chain_items.clear();
        self.points.clear();
    }

    fn size_in_bytes(&self) -> usize {
        self.items.len() * std::mem::size_of::<di::DisplayItem>()
            + self.spatial_items.len() * std::mem::size_of::<di::SpatialTreeItem>()
            + self.glyphs.len() * std::mem::size_of::<GlyphInstance>()
            + self.stops.len() * std::mem::size_of::<di::GradientStop>()
            + self.filters.len() * std::mem::size_of::<di::FilterOp>()
            + self.filter_data.len() * std::mem::size_of::<di::FilterData>()
            + self.filter_primitives.len() * std::mem::size_of::<di::FilterPrimitive>()
            + self.clip_chain_items.len() * std::mem::size_of::<di::ClipId>()
            + self.points.len() * std::mem::size_of::<LayoutPoint>()
    }

    #[cfg(feature = "serialize")]
    fn create_debug_spatial_tree_items(&self) -> Vec<di::SpatialTreeItem> {
        // Spatial items are now stored directly in Vec
        self.spatial_items.clone()
    }
}

/// A display list.
#[derive(Default, Clone)]
pub struct BuiltDisplayList {
    payload: DisplayListPayload,
    descriptor: BuiltDisplayListDescriptor,
}

#[repr(C)]
#[derive(Copy, Clone, Deserialize, Serialize)]
pub enum GeckoDisplayListType {
    None,
    Partial(f64),
    Full(f64),
}

impl Default for GeckoDisplayListType {
    fn default() -> Self {
        GeckoDisplayListType::None
    }
}

/// Describes the memory layout of a display list.
///
/// A display list consists of some number of display list items, followed by a number of display
/// items.
#[repr(C)]
#[derive(Copy, Clone, Default, Deserialize, Serialize)]
pub struct BuiltDisplayListDescriptor {
    /// Gecko specific information about the display list.
    gecko_display_list_type: GeckoDisplayListType,
    /// The first IPC time stamp: before any work has been done
    builder_start_time: u64,
    /// The second IPC time stamp: after serialization
    builder_finish_time: u64,
    /// The third IPC time stamp: just before sending
    send_start_time: u64,
    /// The amount of clipping nodes created while building this display list.
    total_clip_nodes: usize,
    /// The amount of spatial nodes created while building this display list.
    total_spatial_nodes: usize,
    /// The size of the cache for this display list.
    cache_size: usize,
}

#[derive(Clone)]
pub struct DisplayListWithCache {
    pub display_list: BuiltDisplayList,
    cache: DisplayItemCache,
}

impl DisplayListWithCache {
    pub fn iter(&self) -> BuiltDisplayListIter {
        self.display_list.iter_with_cache(&self.cache)
    }

    pub fn new_from_list(display_list: BuiltDisplayList) -> Self {
        let mut cache = DisplayItemCache::new();
        cache.update(&display_list);

        DisplayListWithCache {
            display_list,
            cache,
        }
    }

    pub fn update(&mut self, display_list: BuiltDisplayList) {
        self.cache.update(&display_list);
        self.display_list = display_list;
    }

    pub fn descriptor(&self) -> &BuiltDisplayListDescriptor {
        self.display_list.descriptor()
    }

    pub fn times(&self) -> (u64, u64, u64) {
        self.display_list.times()
    }
}

/// A debug (human-readable) representation of a built display list that
/// can be used for capture and replay.
#[cfg(any(feature = "serialize", feature = "deserialize"))]
#[cfg_attr(feature = "serialize", derive(Serialize))]
#[cfg_attr(feature = "deserialize", derive(Deserialize))]
struct DisplayListCapture {
    display_items: Vec<di::DebugDisplayItem>,
    spatial_tree_items: Vec<di::SpatialTreeItem>,
    descriptor: BuiltDisplayListDescriptor,
}

#[cfg(feature = "serialize")]
impl Serialize for DisplayListWithCache {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let display_items = BuiltDisplayList::create_debug_display_items(self.iter());
        let spatial_tree_items = self.display_list.payload.create_debug_spatial_tree_items();

        let dl = DisplayListCapture {
            display_items,
            spatial_tree_items,
            descriptor: self.display_list.descriptor,
        };

        dl.serialize(serializer)
    }
}

#[cfg(feature = "deserialize")]
impl<'de> Deserialize<'de> for DisplayListWithCache {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        use crate::display_item::{DebugDisplayItem as Debug, DisplayItem as Real};

        let capture = DisplayListCapture::deserialize(deserializer)?;

        // Create the new payload with direct Vec storage
        let mut payload = DisplayListPayload::default();

        for complete in capture.display_items {
            let item = match complete {
                Debug::ClipChain(v, clip_chain_ids) => {
                    payload.clip_chain_items.extend(clip_chain_ids);
                    Real::ClipChain(v)
                }
                Debug::Text(v, glyphs) => {
                    payload.glyphs.extend(glyphs);
                    Real::Text(v)
                }
                Debug::Iframe(v) => Real::Iframe(v),
                Debug::PushReferenceFrame(v) => Real::PushReferenceFrame(v),
                Debug::SetFilterOps(filters) => {
                    payload.filters.extend(filters);
                    Real::SetFilterOps
                }
                Debug::SetFilterData(filter_data) => {
                    payload.filter_data.push(filter_data);
                    Real::SetFilterData
                }
                Debug::SetFilterPrimitives(filter_primitives) => {
                    payload.filter_primitives.extend(filter_primitives);
                    Real::SetFilterPrimitives
                }
                Debug::SetGradientStops(stops) => {
                    payload.stops.extend(stops);
                    Real::SetGradientStops
                }
                Debug::SetPoints(points) => {
                    payload.points.extend(points);
                    Real::SetPoints
                }
                Debug::RectClip(v) => Real::RectClip(v),
                Debug::RoundedRectClip(v) => Real::RoundedRectClip(v),
                Debug::ImageMaskClip(v) => Real::ImageMaskClip(v),
                Debug::Rectangle(v) => Real::Rectangle(v),
                Debug::ClearRectangle(v) => Real::ClearRectangle(v),
                Debug::HitTest(v) => Real::HitTest(v),
                Debug::Line(v) => Real::Line(v),
                Debug::Image(v) => Real::Image(v),
                Debug::RepeatingImage(v) => Real::RepeatingImage(v),
                Debug::YuvImage(v) => Real::YuvImage(v),
                Debug::Border(v) => Real::Border(v),
                Debug::BoxShadow(v) => Real::BoxShadow(v),
                Debug::Gradient(v) => Real::Gradient(v),
                Debug::RadialGradient(v) => Real::RadialGradient(v),
                Debug::ConicGradient(v) => Real::ConicGradient(v),
                Debug::PushStackingContext(v) => Real::PushStackingContext(v),
                Debug::PushShadow(v) => Real::PushShadow(v),
                Debug::BackdropFilter(v) => Real::BackdropFilter(v),

                Debug::PopStackingContext => Real::PopStackingContext,
                Debug::PopReferenceFrame => Real::PopReferenceFrame,
                Debug::PopAllShadows => Real::PopAllShadows,
            };
            payload.items.push(item);
        }

        // Add spatial tree items
        payload.spatial_items = capture.spatial_tree_items;

        Ok(DisplayListWithCache {
            display_list: BuiltDisplayList {
                descriptor: capture.descriptor,
                payload,
            },
            cache: DisplayItemCache::new(),
        })
    }
}

pub struct BuiltDisplayListIter<'a> {
    payload: &'a DisplayListPayload,
    item_index: usize,
    // Indices into auxiliary data arrays
    glyph_index: usize,
    stop_index: usize,
    filter_index: usize,
    filter_data_index: usize,
    filter_primitive_index: usize,
    clip_chain_item_index: usize,
    point_index: usize,
    // Cache support
    cache: Option<&'a DisplayItemCache>,
    pending_items: std::slice::Iter<'a, CachedDisplayItem>,
    cur_cached_item: Option<&'a CachedDisplayItem>,
    // Current item and associated data slices
    cur_item: di::DisplayItem,
    cur_stops: &'a [di::GradientStop],
    cur_glyphs: &'a [GlyphInstance],
    cur_filters: &'a [di::FilterOp],
    cur_filter_data: Vec<&'a di::FilterData>,
    cur_filter_primitives: &'a [di::FilterPrimitive],
    cur_clip_chain_items: &'a [di::ClipId],
    cur_points: &'a [LayoutPoint],
    peeking: Peek,
}

/// Internal info used for more detailed analysis of serialized display lists
#[allow(dead_code)]
struct DebugStats {
    /// Last address in the buffer we pointed to, for computing serialized sizes
    last_addr: usize,
    stats: HashMap<&'static str, ItemStats>,
}

impl DebugStats {
    #[cfg(feature = "display_list_stats")]
    fn _update_entry(&mut self, name: &'static str, item_count: usize, byte_count: usize) {
        let entry = self.stats.entry(name).or_default();
        entry.total_count += item_count;
        entry.num_bytes += byte_count;
    }

    /// Computes the number of bytes we've processed since we last called
    /// this method, so we can compute the serialized size of a display item.
    #[cfg(feature = "display_list_stats")]
    fn debug_num_bytes(&mut self, data: &[u8]) -> usize {
        let old_addr = self.last_addr;
        let new_addr = data.as_ptr() as usize;
        let delta = new_addr - old_addr;
        self.last_addr = new_addr;

        delta
    }

    /// Logs stats for the last deserialized display item
    #[cfg(feature = "display_list_stats")]
    fn log_item(&mut self, data: &[u8], item: &di::DisplayItem) {
        let num_bytes = self.debug_num_bytes(data);
        self._update_entry(item.debug_name(), 1, num_bytes);
    }

    /// Logs the stats for the given serialized slice
    #[cfg(feature = "display_list_stats")]
    fn log_slice<T: Copy + Default>(&mut self, slice_name: &'static str, range: &ItemRange<T>) {
        // Run this so log_item_stats is accurate, but ignore its result
        // because log_slice_stats may be called after multiple slices have been
        // processed, and the `range` has everything we need.
        self.last_addr = range.bytes.as_ptr() as usize + range.bytes.len();

        self._update_entry(slice_name, range.iter().len(), range.bytes.len());
    }

    #[cfg(not(feature = "display_list_stats"))]
    fn log_slice<T>(&mut self, _slice_name: &str, _range: &ItemRange<T>) {
        /* no-op */
    }
}

/// Stats for an individual item
#[derive(Copy, Clone, Debug, Default)]
pub struct ItemStats {
    /// How many instances of this kind of item we deserialized
    pub total_count: usize,
    /// How many bytes we processed for this kind of item
    pub num_bytes: usize,
}

pub struct DisplayItemRef<'a: 'b, 'b> {
    iter: &'b BuiltDisplayListIter<'a>,
}

// Some of these might just become ItemRanges
impl<'a, 'b> DisplayItemRef<'a, 'b> {
    // Creates a new iterator where this element's iterator is, to hack around borrowck.
    pub fn sub_iter(&self) -> BuiltDisplayListIter<'a> {
        self.iter.sub_iter()
    }

    pub fn item(&self) -> &di::DisplayItem {
        self.iter.current_item()
    }

    pub fn clip_chain_items(&self) -> &'a [di::ClipId] {
        self.iter.cur_clip_chain_items
    }

    pub fn points(&self) -> &'a [LayoutPoint] {
        self.iter.cur_points
    }

    pub fn glyphs(&self) -> &'a [GlyphInstance] {
        self.iter.glyphs()
    }

    pub fn gradient_stops(&self) -> &'a [di::GradientStop] {
        self.iter.gradient_stops()
    }

    pub fn filters(&self) -> &'a [di::FilterOp] {
        self.iter.cur_filters
    }

    pub fn filter_datas(&self) -> &Vec<&'a di::FilterData> {
        self.iter.filter_datas()
    }

    pub fn filter_primitives(&self) -> &'a [di::FilterPrimitive] {
        self.iter.cur_filter_primitives
    }
}

#[derive(PartialEq)]
enum Peek {
    StartPeeking,
    IsPeeking,
    NotPeeking,
}

#[derive(Clone)]
pub struct AuxIter<'a, T> {
    item: T,
    data: &'a [u8],
    size: usize,
    //    _boo: PhantomData<T>,
}

impl BuiltDisplayList {
    pub fn from_data(payload: DisplayListPayload, descriptor: BuiltDisplayListDescriptor) -> Self {
        BuiltDisplayList {
            payload,
            descriptor,
        }
    }

    pub fn into_data(self) -> (DisplayListPayload, BuiltDisplayListDescriptor) {
        (self.payload, self.descriptor)
    }

    pub fn items(&self) -> &[di::DisplayItem] {
        &self.payload.items
    }

    pub fn spatial_items(&self) -> &[di::SpatialTreeItem] {
        &self.payload.spatial_items
    }

    pub fn glyphs(&self) -> &[GlyphInstance] {
        &self.payload.glyphs
    }

    pub fn stops(&self) -> &[di::GradientStop] {
        &self.payload.stops
    }

    pub fn filters(&self) -> &[di::FilterOp] {
        &self.payload.filters
    }

    pub fn filter_data(&self) -> &[di::FilterData] {
        &self.payload.filter_data
    }

    pub fn filter_primitives(&self) -> &[di::FilterPrimitive] {
        &self.payload.filter_primitives
    }

    pub fn clip_chain_items(&self) -> &[di::ClipId] {
        &self.payload.clip_chain_items
    }

    pub fn points(&self) -> &[LayoutPoint] {
        &self.payload.points
    }

    pub fn descriptor(&self) -> &BuiltDisplayListDescriptor {
        &self.descriptor
    }

    pub fn set_send_time_ns(&mut self, time: u64) {
        self.descriptor.send_start_time = time;
    }

    pub fn times(&self) -> (u64, u64, u64) {
        (
            self.descriptor.builder_start_time,
            self.descriptor.builder_finish_time,
            self.descriptor.send_start_time,
        )
    }

    pub fn gecko_display_list_stats(&self) -> (f64, bool) {
        match self.descriptor.gecko_display_list_type {
            GeckoDisplayListType::Full(duration) => (duration, true),
            GeckoDisplayListType::Partial(duration) => (duration, false),
            _ => (0.0, false),
        }
    }

    pub fn total_clip_nodes(&self) -> usize {
        self.descriptor.total_clip_nodes
    }

    pub fn total_spatial_nodes(&self) -> usize {
        self.descriptor.total_spatial_nodes
    }

    pub fn iter(&self) -> BuiltDisplayListIter {
        BuiltDisplayListIter::new(&self.payload, None)
    }

    pub fn cache_data_iter(&self) -> BuiltDisplayListIter {
        // Cache iteration is no longer supported in direct storage mode
        BuiltDisplayListIter::new(&self.payload, None)
    }

    pub fn iter_with_cache<'a>(&'a self, cache: &'a DisplayItemCache) -> BuiltDisplayListIter<'a> {
        BuiltDisplayListIter::new(&self.payload, Some(cache))
    }

    pub fn cache_size(&self) -> usize {
        self.descriptor.cache_size
    }

    pub fn size_in_bytes(&self) -> usize {
        self.payload.size_in_bytes()
    }

    pub fn iter_spatial_tree<F>(&self, mut f: F)
    where
        F: FnMut(&di::SpatialTreeItem),
    {
        // Iterate over spatial items stored directly in the Vec
        for item in &self.payload.spatial_items {
            f(item);
        }
    }

    #[cfg(feature = "serialize")]
    pub fn create_debug_display_items(
        mut iterator: BuiltDisplayListIter,
    ) -> Vec<di::DebugDisplayItem> {
        use di::{DebugDisplayItem as Debug, DisplayItem as Real};
        let mut debug_items = Vec::new();

        while let Some(item) = iterator.next_raw() {
            let serial_di = match *item.item() {
                Real::ClipChain(v) => {
                    Debug::ClipChain(v, item.iter.cur_clip_chain_items.iter().copied().collect())
                }
                Real::Text(v) => Debug::Text(v, item.iter.cur_glyphs.iter().cloned().collect()),
                Real::SetFilterOps => {
                    Debug::SetFilterOps(item.iter.cur_filters.iter().cloned().collect())
                }
                Real::SetFilterData => {
                    debug_assert!(
                        !item.iter.cur_filter_data.is_empty(),
                        "next_raw should have populated cur_filter_data"
                    );
                    let filter_data =
                        &item.iter.cur_filter_data[item.iter.cur_filter_data.len() - 1];

                    // cur_filter_data contains &FilterData, so we clone it directly
                    Debug::SetFilterData(di::FilterData {
                        func_r_type: filter_data.func_r_type,
                        r_values: filter_data.r_values.clone(),
                        func_g_type: filter_data.func_g_type,
                        g_values: filter_data.g_values.clone(),
                        func_b_type: filter_data.func_b_type,
                        b_values: filter_data.b_values.clone(),
                        func_a_type: filter_data.func_a_type,
                        a_values: filter_data.a_values.clone(),
                    })
                }
                Real::SetFilterPrimitives => Debug::SetFilterPrimitives(
                    item.iter.cur_filter_primitives.iter().cloned().collect(),
                ),
                Real::SetGradientStops => {
                    Debug::SetGradientStops(item.iter.cur_stops.iter().cloned().collect())
                }
                Real::SetPoints => Debug::SetPoints(item.iter.cur_points.iter().copied().collect()),
                Real::RectClip(v) => Debug::RectClip(v),
                Real::RoundedRectClip(v) => Debug::RoundedRectClip(v),
                Real::ImageMaskClip(v) => Debug::ImageMaskClip(v),
                Real::Rectangle(v) => Debug::Rectangle(v),
                Real::ClearRectangle(v) => Debug::ClearRectangle(v),
                Real::HitTest(v) => Debug::HitTest(v),
                Real::Line(v) => Debug::Line(v),
                Real::Image(v) => Debug::Image(v),
                Real::RepeatingImage(v) => Debug::RepeatingImage(v),
                Real::YuvImage(v) => Debug::YuvImage(v),
                Real::Border(v) => Debug::Border(v),
                Real::BoxShadow(v) => Debug::BoxShadow(v),
                Real::Gradient(v) => Debug::Gradient(v),
                Real::RadialGradient(v) => Debug::RadialGradient(v),
                Real::ConicGradient(v) => Debug::ConicGradient(v),
                Real::Iframe(v) => Debug::Iframe(v),
                Real::PushReferenceFrame(v) => Debug::PushReferenceFrame(v),
                Real::PushStackingContext(v) => Debug::PushStackingContext(v),
                Real::PushShadow(v) => Debug::PushShadow(v),
                Real::BackdropFilter(v) => Debug::BackdropFilter(v),

                Real::PopReferenceFrame => Debug::PopReferenceFrame,
                Real::PopStackingContext => Debug::PopStackingContext,
                Real::PopAllShadows => Debug::PopAllShadows,
                Real::ReuseItems(_) | Real::RetainedItems(_) => unreachable!("Unexpected item"),
            };
            debug_items.push(serial_di);
        }

        debug_items
    }
}

/// Returns the byte-range the slice occupied.
/// Note: Serialization removed - this is a stub for compatibility
fn skip_slice<'a, T>(data: &mut &'a [u8]) -> ItemRange<'a, T> {
    ItemRange {
        bytes: &[],
        _boo: PhantomData,
    }
}

impl<'a> BuiltDisplayListIter<'a> {
    pub fn new(payload: &'a DisplayListPayload, cache: Option<&'a DisplayItemCache>) -> Self {
        Self {
            payload,
            item_index: 0,
            glyph_index: 0,
            stop_index: 0,
            filter_index: 0,
            filter_data_index: 0,
            filter_primitive_index: 0,
            clip_chain_item_index: 0,
            point_index: 0,
            cache,
            pending_items: [].iter(),
            cur_cached_item: None,
            cur_item: di::DisplayItem::PopStackingContext,
            cur_stops: &[],
            cur_glyphs: &[],
            cur_filters: &[],
            cur_filter_data: Vec::new(),
            cur_filter_primitives: &[],
            cur_clip_chain_items: &[],
            cur_points: &[],
            peeking: Peek::NotPeeking,
        }
    }

    pub fn sub_iter(&self) -> Self {
        let mut iter = BuiltDisplayListIter::new(self.payload, self.cache);
        iter.pending_items = self.pending_items.clone();
        iter.item_index = self.item_index;
        iter.glyph_index = self.glyph_index;
        iter.stop_index = self.stop_index;
        iter.filter_index = self.filter_index;
        iter.filter_data_index = self.filter_data_index;
        iter.filter_primitive_index = self.filter_primitive_index;
        iter.clip_chain_item_index = self.clip_chain_item_index;
        iter.point_index = self.point_index;
        iter
    }

    pub fn current_item(&self) -> &di::DisplayItem {
        match self.cur_cached_item {
            Some(cached_item) => cached_item.display_item(),
            None => &self.cur_item,
        }
    }

    pub fn glyphs(&self) -> &'a [GlyphInstance] {
        self.cur_glyphs
    }

    pub fn gradient_stops(&self) -> &'a [di::GradientStop] {
        self.cur_stops
    }

    pub fn filters(&self) -> &'a [di::FilterOp] {
        self.cur_filters
    }

    pub fn filter_datas(&self) -> &Vec<&'a di::FilterData> {
        &self.cur_filter_data
    }

    pub fn filter_primitives(&self) -> &'a [di::FilterPrimitive] {
        self.cur_filter_primitives
    }

    pub fn clip_chain_items(&self) -> &'a [di::ClipId] {
        self.cur_clip_chain_items
    }

    pub fn points(&self) -> &'a [LayoutPoint] {
        self.cur_points
    }

    fn advance_pending_items(&mut self) -> bool {
        self.cur_cached_item = self.pending_items.next();
        self.cur_cached_item.is_some()
    }

    pub fn next<'b>(&'b mut self) -> Option<DisplayItemRef<'a, 'b>> {
        use crate::DisplayItem::*;

        match self.peeking {
            Peek::IsPeeking => {
                self.peeking = Peek::NotPeeking;
                return Some(self.as_ref());
            }
            Peek::StartPeeking => {
                self.peeking = Peek::IsPeeking;
            }
            Peek::NotPeeking => { /* do nothing */ }
        }

        // Don't let these bleed into another item
        self.cur_stops = &[];
        self.cur_clip_chain_items = &[];
        self.cur_points = &[];
        self.cur_filters = &[];
        self.cur_filter_primitives = &[];
        self.cur_filter_data.clear();

        loop {
            self.next_raw()?;
            match self.cur_item {
                SetGradientStops { .. } | SetFilterOps { .. } | SetFilterData | SetFilterPrimitives { .. }
                | SetPoints { .. } => {
                    // These are marker items for populating other display items, don't yield them.
                    continue;
                }
                _ => {
                    break;
                }
            }
        }

        Some(self.as_ref())
    }

    /// Gets the next display item, even if it's a dummy. Also doesn't handle peeking
    /// and may leave irrelevant ranges live (so a Clip may have GradientStops if
    /// for some reason you ask).
    pub fn next_raw<'b>(&'b mut self) -> Option<DisplayItemRef<'a, 'b>> {
        use crate::DisplayItem::*;

        if self.advance_pending_items() {
            return Some(self.as_ref());
        }

        // Check if we have more items to iterate
        if self.item_index >= self.payload.items.len() {
            return None;
        }

        // Get the next item from the items Vec
        self.cur_item = self.payload.items[self.item_index];
        self.item_index += 1;

        match self.cur_item {
            SetGradientStops { stop_count } => {
                let end = (self.stop_index + stop_count).min(self.payload.stops.len());
                self.cur_stops = &self.payload.stops[self.stop_index..end];
                self.stop_index = end;
            }
            SetFilterOps { filter_count } => {
                let end = (self.filter_index + filter_count).min(self.payload.filters.len());
                self.cur_filters = &self.payload.filters[self.filter_index..end];
                self.filter_index = end;
            }
            SetFilterData => {
                if self.filter_data_index < self.payload.filter_data.len() {
                    self.cur_filter_data
                        .push(&self.payload.filter_data[self.filter_data_index]);
                    self.filter_data_index += 1;
                }
            }
            SetFilterPrimitives { primitive_count } => {
                let end = (self.filter_primitive_index + primitive_count).min(self.payload.filter_primitives.len());
                self.cur_filter_primitives =
                    &self.payload.filter_primitives[self.filter_primitive_index..end];
                self.filter_primitive_index = end;
            }
            SetPoints { point_count } => {
                let end = (self.point_index + point_count).min(self.payload.points.len());
                self.cur_points = &self.payload.points[self.point_index..end];
                self.point_index = end;
            }
            ClipChain(ref chain_item) => {
                // Use clip_count to know how many clip items belong to this chain
                let count = chain_item.clip_count;
                let end =
                    (self.clip_chain_item_index + count).min(self.payload.clip_chain_items.len());
                self.cur_clip_chain_items =
                    &self.payload.clip_chain_items[self.clip_chain_item_index..end];
                self.clip_chain_item_index = end;
            }
            Text(ref text_item) => {
                // Use glyph_count from the TextDisplayItem to know how many glyphs belong to this text
                let count = text_item.glyph_count;
                let end = (self.glyph_index + count).min(self.payload.glyphs.len());
                self.cur_glyphs = &self.payload.glyphs[self.glyph_index..end];
                self.glyph_index = end;
            }
            ReuseItems(key) => match self.cache {
                Some(cache) => {
                    self.pending_items = cache.get_items(key).iter();
                    self.advance_pending_items();
                }
                None => {
                    unreachable!("Cache marker without cache!");
                }
            },
            _ => { /* do nothing */ }
        }

        Some(self.as_ref())
    }

    pub fn as_ref<'b>(&'b self) -> DisplayItemRef<'a, 'b> {
        DisplayItemRef { iter: self }
    }

    pub fn skip_current_stacking_context(&mut self) {
        let mut depth = 0;
        while let Some(item) = self.next() {
            match *item.item() {
                di::DisplayItem::PushStackingContext(..) => depth += 1,
                di::DisplayItem::PopStackingContext if depth == 0 => return,
                di::DisplayItem::PopStackingContext => depth -= 1,
                _ => {}
            }
        }
    }

    pub fn current_stacking_context_empty(&mut self) -> bool {
        match self.peek() {
            Some(item) => *item.item() == di::DisplayItem::PopStackingContext,
            None => true,
        }
    }

    pub fn peek<'b>(&'b mut self) -> Option<DisplayItemRef<'a, 'b>> {
        if self.peeking == Peek::NotPeeking {
            self.peeking = Peek::StartPeeking;
            self.next()
        } else {
            Some(self.as_ref())
        }
    }

    /// Get the debug stats for what this iterator has deserialized.
    /// Should always be empty in release builds.
    pub fn debug_stats(&mut self) -> Vec<(&'static str, ItemStats)> {
        // Debug stats are no longer tracked in direct storage mode
        Vec::new()
    }

    /// Adds the debug stats from another to our own, assuming we are a sub-iter of the other
    /// (so we can ignore where they were in the traversal).
    pub fn merge_debug_stats_from(&mut self, _other: &mut Self) {
        // Debug stats are no longer tracked in direct storage mode
    }

    /// Logs stats for the last deserialized display item
    #[cfg(feature = "display_list_stats")]
    fn log_item_stats(&mut self) {
        // Debug stats are no longer tracked in direct storage mode
    }

    #[cfg(not(feature = "display_list_stats"))]
    fn log_item_stats(&mut self) { /* no-op */
    }
}

impl<'a, T> AuxIter<'a, T> {
    pub fn new(item: T, data: &'a [u8]) -> Self {
        // Serialization removed - no deserialization needed
        AuxIter {
            item,
            data,
            size: 0,
            //            _boo: PhantomData,
        }
    }
}

impl<'a, T: Copy> Iterator for AuxIter<'a, T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        // Serialization removed - this is a stub for compatibility
        None
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.size, Some(self.size))
    }
}

impl<'a, T: Copy> ::std::iter::ExactSizeIterator for AuxIter<'a, T> {}

#[derive(Clone, Debug)]
pub struct SaveState {
    dl_items_len: usize,
    next_clip_index: usize,
    next_spatial_index: usize,
    next_clip_chain_id: u64,
}

/// DisplayListSection determines the target buffer for the display items.
pub enum DisplayListSection {
    /// The main/default buffer: contains item data and item group markers.
    Data,
    /// Auxiliary buffer: contains the item data for item groups.
    CacheData,
    /// Temporary buffer: contains the data for pending item group. Flushed to
    /// one of the buffers above, after item grouping finishes.
    Chunk,
}

pub struct DisplayListBuilder {
    payload: DisplayListPayload,
    pub pipeline_id: PipelineId,

    pending_chunk: Vec<u8>,
    writing_to_chunk: bool,

    next_clip_index: usize,
    next_spatial_index: usize,
    next_clip_chain_id: u64,
    builder_start_time: u64,

    save_state: Option<SaveState>,

    cache_size: usize,
    serialized_content_buffer: Option<String>,
    state: BuildState,

    /// Helper struct to map stacking context coords <-> reference frame coords.
    rf_mapper: ReferenceFrameMapper,
}

#[repr(C)]
struct DisplayListCapacity {
    items_size: usize,
    cache_size: usize,
    spatial_tree_size: usize,
}

impl DisplayListCapacity {
    fn empty() -> Self {
        DisplayListCapacity {
            items_size: 0,
            cache_size: 0,
            spatial_tree_size: 0,
        }
    }
}

impl DisplayListBuilder {
    pub fn new(pipeline_id: PipelineId) -> Self {
        DisplayListBuilder {
            payload: DisplayListPayload::new(DisplayListCapacity::empty()),
            pipeline_id,

            pending_chunk: Vec::new(),
            writing_to_chunk: false,

            next_clip_index: FIRST_CLIP_NODE_INDEX,
            next_spatial_index: FIRST_SPATIAL_NODE_INDEX,
            next_clip_chain_id: 0,
            builder_start_time: 0,
            save_state: None,
            cache_size: 0,
            serialized_content_buffer: None,
            state: BuildState::Idle,

            rf_mapper: ReferenceFrameMapper::new(),
        }
    }

    fn reset(&mut self) {
        self.payload.clear();
        self.pending_chunk.clear();
        self.writing_to_chunk = false;

        self.next_clip_index = FIRST_CLIP_NODE_INDEX;
        self.next_spatial_index = FIRST_SPATIAL_NODE_INDEX;
        self.next_clip_chain_id = 0;

        self.save_state = None;
        self.cache_size = 0;
        self.serialized_content_buffer = None;

        self.rf_mapper = ReferenceFrameMapper::new();
    }

    /// Saves the current display list state, so it may be `restore()`'d.
    ///
    /// # Conditions:
    ///
    /// * Doesn't support popping clips that were pushed before the save.
    /// * Doesn't support nested saves.
    /// * Must call `clear_save()` if the restore becomes unnecessary.
    pub fn save(&mut self) {
        assert!(
            self.save_state.is_none(),
            "DisplayListBuilder doesn't support nested saves"
        );

        self.save_state = Some(SaveState {
            dl_items_len: self.payload.items.len(),
            next_clip_index: self.next_clip_index,
            next_spatial_index: self.next_spatial_index,
            next_clip_chain_id: self.next_clip_chain_id,
        });
    }

    /// Restores the state of the builder to when `save()` was last called.
    pub fn restore(&mut self) {
        let state = self
            .save_state
            .take()
            .expect("No save to restore DisplayListBuilder from");

        self.payload.items.truncate(state.dl_items_len);
        self.next_clip_index = state.next_clip_index;
        self.next_spatial_index = state.next_spatial_index;
        self.next_clip_chain_id = state.next_clip_chain_id;
    }

    /// Discards the builder's save (indicating the attempted operation was successful).
    pub fn clear_save(&mut self) {
        self.save_state
            .take()
            .expect("No save to clear in DisplayListBuilder");
    }

    /// Emits a debug representation of display items in the list, for debugging
    /// purposes. If the range's start parameter is specified, only display
    /// items starting at that index (inclusive) will be printed. If the range's
    /// end parameter is specified, only display items before that index
    /// (exclusive) will be printed. Calling this function with end <= start is
    /// allowed but is just a waste of CPU cycles. The function emits the
    /// debug representation of the selected display items, one per line, with
    /// the given indent, to the provided sink object. The return value is
    /// the total number of items in the display list, which allows the
    /// caller to subsequently invoke this function to only dump the newly-added
    /// items.
    pub fn emit_display_list<W>(
        &mut self,
        indent: usize,
        range: Range<Option<usize>>,
        mut sink: W,
    ) -> usize
    where
        W: Write,
    {
        let mut temp = BuiltDisplayList::default();
        // Serialization removed - no red zone needed
        mem::swap(&mut temp.payload, &mut self.payload);

        let mut index: usize = 0;
        {
            let mut cache = DisplayItemCache::new();
            cache.update(&temp);
            let mut iter = temp.iter_with_cache(&cache);
            while let Some(item) = iter.next_raw() {
                if index >= range.start.unwrap_or(0) && range.end.map_or(true, |e| index < e) {
                    writeln!(sink, "{}{:?}", "  ".repeat(indent), item.item()).unwrap();
                }
                index += 1;
            }
        }

        self.payload = temp.payload;
        // Serialization removed - no red zone to strip
        index
    }

    /// Print the display items in the list to stdout.
    pub fn dump_serialized_display_list(&mut self) {
        self.serialized_content_buffer = Some(String::new());
    }

    fn add_to_display_list_dump<T: std::fmt::Debug>(&mut self, item: T) {
        if let Some(ref mut content) = self.serialized_content_buffer {
            use std::fmt::Write;
            write!(content, "{:?}\n", item).expect("DL dump write failed.");
        }
    }

    /// Returns the default section that DisplayListBuilder will write to,
    /// if no section is specified explicitly.
    fn default_section(&self) -> DisplayListSection {
        if self.writing_to_chunk {
            DisplayListSection::Chunk
        } else {
            DisplayListSection::Data
        }
    }

    // Note: buffer_from_section removed - we no longer serialize to byte buffers

    #[inline]
    pub fn push_item_to_section(&mut self, item: &di::DisplayItem, _section: DisplayListSection) {
        debug_assert_eq!(self.state, BuildState::Build);
        // Store item directly in Vec instead of serializing
        self.payload.items.push(*item);
        self.add_to_display_list_dump(item);
    }

    /// Add an item to the display list.
    ///
    /// NOTE: It is usually preferable to use the specialized methods to push
    /// display items. Pushing unexpected or invalid items here may
    /// result in WebRender panicking or behaving in unexpected ways.
    #[inline]
    pub fn push_item(&mut self, item: &di::DisplayItem) {
        self.push_item_to_section(item, self.default_section());
    }

    #[inline]
    pub fn push_spatial_tree_item(&mut self, item: &di::SpatialTreeItem) {
        debug_assert_eq!(self.state, BuildState::Build);
        // Store spatial tree item directly in Vec instead of serializing
        self.payload.spatial_items.push(*item);
    }

    fn push_iter_impl<I>(data: &mut Vec<u8>, iter_source: I)
    where
        I: IntoIterator,
        I::IntoIter: ExactSizeIterator,
    {
        // Serialization removed - items not serialized for in-process rendering
    }

    /// Push items from an iterator to the display list.
    ///
    /// NOTE: Pushing unexpected or invalid items to the display list
    /// may result in panic and confusion.
    pub fn push_iter<I>(&mut self, iter: I)
    where
        I: IntoIterator,
        I::IntoIter: ExactSizeIterator,
    {
        assert_eq!(self.state, BuildState::Build);
        // Serialization removed - items not serialized for in-process rendering
    }

    // Remap a clip/bounds from stacking context coords to reference frame relative
    fn remap_common_coordinates_and_bounds(
        &self,
        common: &di::CommonItemProperties,
        bounds: LayoutRect,
    ) -> (di::CommonItemProperties, LayoutRect) {
        let offset = self.rf_mapper.current_offset();

        (
            di::CommonItemProperties {
                clip_rect: common.clip_rect.translate(offset),
                ..*common
            },
            bounds.translate(offset),
        )
    }

    // Remap a bounds from stacking context coords to reference frame relative
    fn remap_bounds(&self, bounds: LayoutRect) -> LayoutRect {
        let offset = self.rf_mapper.current_offset();

        bounds.translate(offset)
    }

    pub fn push_rect(
        &mut self,
        common: &di::CommonItemProperties,
        bounds: LayoutRect,
        color: ColorF,
    ) {
        let (common, bounds) = self.remap_common_coordinates_and_bounds(common, bounds);

        let item = di::DisplayItem::Rectangle(di::RectangleDisplayItem {
            common,
            color: PropertyBinding::Value(color),
            bounds,
        });
        self.push_item(&item);
    }

    pub fn push_rect_with_animation(
        &mut self,
        common: &di::CommonItemProperties,
        bounds: LayoutRect,
        color: PropertyBinding<ColorF>,
    ) {
        let (common, bounds) = self.remap_common_coordinates_and_bounds(common, bounds);

        let item = di::DisplayItem::Rectangle(di::RectangleDisplayItem {
            common,
            color,
            bounds,
        });
        self.push_item(&item);
    }

    pub fn push_clear_rect(&mut self, common: &di::CommonItemProperties, bounds: LayoutRect) {
        let (common, bounds) = self.remap_common_coordinates_and_bounds(common, bounds);

        let item =
            di::DisplayItem::ClearRectangle(di::ClearRectangleDisplayItem { common, bounds });
        self.push_item(&item);
    }

    pub fn push_hit_test(
        &mut self,
        rect: LayoutRect,
        clip_chain_id: di::ClipChainId,
        spatial_id: di::SpatialId,
        flags: di::PrimitiveFlags,
        tag: di::ItemTag,
    ) {
        let rect = self.remap_bounds(rect);

        let item = di::DisplayItem::HitTest(di::HitTestDisplayItem {
            rect,
            clip_chain_id,
            spatial_id,
            flags,
            tag,
        });
        self.push_item(&item);
    }

    pub fn push_line(
        &mut self,
        common: &di::CommonItemProperties,
        area: &LayoutRect,
        wavy_line_thickness: f32,
        orientation: di::LineOrientation,
        color: &ColorF,
        style: di::LineStyle,
    ) {
        let (common, area) = self.remap_common_coordinates_and_bounds(common, *area);

        let item = di::DisplayItem::Line(di::LineDisplayItem {
            common,
            area,
            wavy_line_thickness,
            orientation,
            color: *color,
            style,
        });

        self.push_item(&item);
    }

    pub fn push_image(
        &mut self,
        common: &di::CommonItemProperties,
        bounds: LayoutRect,
        image_rendering: di::ImageRendering,
        alpha_type: di::AlphaType,
        key: ImageKey,
        color: ColorF,
    ) {
        let (common, bounds) = self.remap_common_coordinates_and_bounds(common, bounds);

        let item = di::DisplayItem::Image(di::ImageDisplayItem {
            common,
            bounds,
            image_key: key,
            image_rendering,
            alpha_type,
            color,
        });

        self.push_item(&item);
    }

    pub fn push_repeating_image(
        &mut self,
        common: &di::CommonItemProperties,
        bounds: LayoutRect,
        stretch_size: LayoutSize,
        tile_spacing: LayoutSize,
        image_rendering: di::ImageRendering,
        alpha_type: di::AlphaType,
        key: ImageKey,
        color: ColorF,
    ) {
        let (common, bounds) = self.remap_common_coordinates_and_bounds(common, bounds);

        let item = di::DisplayItem::RepeatingImage(di::RepeatingImageDisplayItem {
            common,
            bounds,
            image_key: key,
            stretch_size,
            tile_spacing,
            image_rendering,
            alpha_type,
            color,
        });

        self.push_item(&item);
    }

    /// Push a yuv image. All planar data in yuv image should use the same buffer type.
    pub fn push_yuv_image(
        &mut self,
        common: &di::CommonItemProperties,
        bounds: LayoutRect,
        yuv_data: di::YuvData,
        color_depth: ColorDepth,
        color_space: di::YuvColorSpace,
        color_range: di::ColorRange,
        image_rendering: di::ImageRendering,
    ) {
        let (common, bounds) = self.remap_common_coordinates_and_bounds(common, bounds);

        let item = di::DisplayItem::YuvImage(di::YuvImageDisplayItem {
            common,
            bounds,
            yuv_data,
            color_depth,
            color_space,
            color_range,
            image_rendering,
        });
        self.push_item(&item);
    }

    pub fn push_text(
        &mut self,
        common: &di::CommonItemProperties,
        bounds: LayoutRect,
        glyphs: &[GlyphInstance],
        font_key: FontInstanceKey,
        color: ColorF,
        glyph_options: Option<GlyphOptions>,
    ) {
        let (common, bounds) = self.remap_common_coordinates_and_bounds(common, bounds);
        let ref_frame_offset = self.rf_mapper.current_offset();

        for split_glyphs in glyphs.chunks(MAX_TEXT_RUN_LENGTH) {
            let item = di::DisplayItem::Text(di::TextDisplayItem {
                common,
                bounds,
                color,
                font_key,
                glyph_options,
                ref_frame_offset,
                glyph_count: split_glyphs.len(),
            });

            self.push_item(&item);
            // Push glyphs directly to payload.glyphs
            // The display list iterator expects glyphs to be stored here
            self.payload.glyphs.extend_from_slice(split_glyphs);
        }
    }

    /// NOTE: gradients must be pushed in the order they're created
    /// because create_gradient stores the stops in anticipation.
    pub fn create_gradient(
        &mut self,
        start_point: LayoutPoint,
        end_point: LayoutPoint,
        stops: Vec<di::GradientStop>,
        extend_mode: di::ExtendMode,
    ) -> di::Gradient {
        let mut builder = GradientBuilder::with_stops(stops);
        let gradient = builder.gradient(start_point, end_point, extend_mode);
        self.push_stops(builder.stops());
        gradient
    }

    /// NOTE: gradients must be pushed in the order they're created
    /// because create_gradient stores the stops in anticipation.
    pub fn create_radial_gradient(
        &mut self,
        center: LayoutPoint,
        radius: LayoutSize,
        stops: Vec<di::GradientStop>,
        extend_mode: di::ExtendMode,
    ) -> di::RadialGradient {
        let mut builder = GradientBuilder::with_stops(stops);
        let gradient = builder.radial_gradient(center, radius, extend_mode);
        self.push_stops(builder.stops());
        gradient
    }

    /// NOTE: gradients must be pushed in the order they're created
    /// because create_gradient stores the stops in anticipation.
    pub fn create_conic_gradient(
        &mut self,
        center: LayoutPoint,
        angle: f32,
        stops: Vec<di::GradientStop>,
        extend_mode: di::ExtendMode,
    ) -> di::ConicGradient {
        let mut builder = GradientBuilder::with_stops(stops);
        let gradient = builder.conic_gradient(center, angle, extend_mode);
        self.push_stops(builder.stops());
        gradient
    }

    pub fn push_border(
        &mut self,
        common: &di::CommonItemProperties,
        bounds: LayoutRect,
        widths: LayoutSideOffsets,
        details: di::BorderDetails,
    ) {
        let (common, bounds) = self.remap_common_coordinates_and_bounds(common, bounds);

        let item = di::DisplayItem::Border(di::BorderDisplayItem {
            common,
            bounds,
            details,
            widths,
        });

        self.push_item(&item);
    }

    pub fn push_box_shadow(
        &mut self,
        common: &di::CommonItemProperties,
        box_bounds: LayoutRect,
        offset: LayoutVector2D,
        color: ColorF,
        blur_radius: f32,
        spread_radius: f32,
        border_radius: di::BorderRadius,
        clip_mode: di::BoxShadowClipMode,
    ) {
        let (common, box_bounds) = self.remap_common_coordinates_and_bounds(common, box_bounds);

        let item = di::DisplayItem::BoxShadow(di::BoxShadowDisplayItem {
            common,
            box_bounds,
            offset,
            color,
            blur_radius,
            spread_radius,
            border_radius,
            clip_mode,
        });

        self.push_item(&item);
    }

    /// Pushes a linear gradient to be displayed.
    ///
    /// The gradient itself is described in the
    /// `gradient` parameter. It is drawn on
    /// a "tile" with the dimensions from `tile_size`.
    /// These tiles are now repeated to the right and
    /// to the bottom infinitely. If `tile_spacing`
    /// is not zero spacers with the given dimensions
    /// are inserted between the tiles as seams.
    ///
    /// The origin of the tiles is given in `layout.rect.origin`.
    /// If the gradient should only be displayed once limit
    /// the `layout.rect.size` to a single tile.
    /// The gradient is only visible within the local clip.
    pub fn push_gradient(
        &mut self,
        common: &di::CommonItemProperties,
        bounds: LayoutRect,
        gradient: di::Gradient,
        tile_size: LayoutSize,
        tile_spacing: LayoutSize,
    ) {
        let (common, bounds) = self.remap_common_coordinates_and_bounds(common, bounds);

        let item = di::DisplayItem::Gradient(di::GradientDisplayItem {
            common,
            bounds,
            gradient,
            tile_size,
            tile_spacing,
        });

        self.push_item(&item);
    }

    /// Pushes a radial gradient to be displayed.
    ///
    /// See [`push_gradient`](#method.push_gradient) for explanation.
    pub fn push_radial_gradient(
        &mut self,
        common: &di::CommonItemProperties,
        bounds: LayoutRect,
        gradient: di::RadialGradient,
        tile_size: LayoutSize,
        tile_spacing: LayoutSize,
    ) {
        let (common, bounds) = self.remap_common_coordinates_and_bounds(common, bounds);

        let item = di::DisplayItem::RadialGradient(di::RadialGradientDisplayItem {
            common,
            bounds,
            gradient,
            tile_size,
            tile_spacing,
        });

        self.push_item(&item);
    }

    /// Pushes a conic gradient to be displayed.
    ///
    /// See [`push_gradient`](#method.push_gradient) for explanation.
    pub fn push_conic_gradient(
        &mut self,
        common: &di::CommonItemProperties,
        bounds: LayoutRect,
        gradient: di::ConicGradient,
        tile_size: LayoutSize,
        tile_spacing: LayoutSize,
    ) {
        let (common, bounds) = self.remap_common_coordinates_and_bounds(common, bounds);

        let item = di::DisplayItem::ConicGradient(di::ConicGradientDisplayItem {
            common,
            bounds,
            gradient,
            tile_size,
            tile_spacing,
        });

        self.push_item(&item);
    }

    pub fn push_reference_frame(
        &mut self,
        origin: LayoutPoint,
        parent_spatial_id: di::SpatialId,
        transform_style: di::TransformStyle,
        transform: PropertyBinding<LayoutTransform>,
        kind: di::ReferenceFrameKind,
        key: di::SpatialTreeItemKey,
    ) -> di::SpatialId {
        let id = self.generate_spatial_index();

        let current_offset = self.rf_mapper.current_offset();
        let origin = origin + current_offset;

        let descriptor = di::SpatialTreeItem::ReferenceFrame(di::ReferenceFrameDescriptor {
            parent_spatial_id,
            origin,
            reference_frame: di::ReferenceFrame {
                transform_style,
                transform: di::ReferenceTransformBinding::Static { binding: transform },
                kind,
                id,
                key,
            },
        });
        self.push_spatial_tree_item(&descriptor);

        self.rf_mapper.push_scope();

        let item = di::DisplayItem::PushReferenceFrame(di::ReferenceFrameDisplayListItem {});
        self.push_item(&item);

        id
    }

    pub fn push_computed_frame(
        &mut self,
        origin: LayoutPoint,
        parent_spatial_id: di::SpatialId,
        scale_from: Option<LayoutSize>,
        vertical_flip: bool,
        rotation: di::Rotation,
        key: di::SpatialTreeItemKey,
    ) -> di::SpatialId {
        let id = self.generate_spatial_index();

        let current_offset = self.rf_mapper.current_offset();
        let origin = origin + current_offset;

        let descriptor = di::SpatialTreeItem::ReferenceFrame(di::ReferenceFrameDescriptor {
            parent_spatial_id,
            origin,
            reference_frame: di::ReferenceFrame {
                transform_style: di::TransformStyle::Flat,
                transform: di::ReferenceTransformBinding::Computed {
                    scale_from,
                    vertical_flip,
                    rotation,
                },
                kind: di::ReferenceFrameKind::Transform {
                    is_2d_scale_translation: false,
                    should_snap: false,
                    paired_with_perspective: false,
                },
                id,
                key,
            },
        });
        self.push_spatial_tree_item(&descriptor);

        self.rf_mapper.push_scope();

        let item = di::DisplayItem::PushReferenceFrame(di::ReferenceFrameDisplayListItem {});
        self.push_item(&item);

        id
    }

    pub fn pop_reference_frame(&mut self) {
        self.rf_mapper.pop_scope();
        self.push_item(&di::DisplayItem::PopReferenceFrame);
    }

    pub fn push_stacking_context(
        &mut self,
        origin: LayoutPoint,
        spatial_id: di::SpatialId,
        prim_flags: di::PrimitiveFlags,
        clip_chain_id: Option<di::ClipChainId>,
        transform_style: di::TransformStyle,
        mix_blend_mode: di::MixBlendMode,
        filters: &[di::FilterOp],
        filter_datas: &[di::FilterData],
        filter_primitives: &[di::FilterPrimitive],
        raster_space: di::RasterSpace,
        flags: di::StackingContextFlags,
    ) {
        let ref_frame_offset = self.rf_mapper.current_offset();
        self.push_filters(filters, filter_datas, filter_primitives);

        let item = di::DisplayItem::PushStackingContext(di::PushStackingContextDisplayItem {
            origin,
            spatial_id,
            prim_flags,
            ref_frame_offset,
            stacking_context: di::StackingContext {
                transform_style,
                mix_blend_mode,
                clip_chain_id,
                raster_space,
                flags,
            },
        });

        self.rf_mapper.push_offset(origin.to_vector());
        self.push_item(&item);
    }

    /// Helper for examples/ code.
    pub fn push_simple_stacking_context(
        &mut self,
        origin: LayoutPoint,
        spatial_id: di::SpatialId,
        prim_flags: di::PrimitiveFlags,
    ) {
        self.push_simple_stacking_context_with_filters(
            origin,
            spatial_id,
            prim_flags,
            &[],
            &[],
            &[],
        );
    }

    /// Helper for examples/ code.
    pub fn push_simple_stacking_context_with_filters(
        &mut self,
        origin: LayoutPoint,
        spatial_id: di::SpatialId,
        prim_flags: di::PrimitiveFlags,
        filters: &[di::FilterOp],
        filter_datas: &[di::FilterData],
        filter_primitives: &[di::FilterPrimitive],
    ) {
        self.push_stacking_context(
            origin,
            spatial_id,
            prim_flags,
            None,
            di::TransformStyle::Flat,
            di::MixBlendMode::Normal,
            filters,
            filter_datas,
            filter_primitives,
            di::RasterSpace::Screen,
            di::StackingContextFlags::empty(),
        );
    }

    pub fn pop_stacking_context(&mut self) {
        self.rf_mapper.pop_offset();
        self.push_item(&di::DisplayItem::PopStackingContext);
    }

    pub fn push_stops(&mut self, stops: &[di::GradientStop]) {
        if stops.is_empty() {
            return;
        }
        self.push_item(&di::DisplayItem::SetGradientStops { stop_count: stops.len() });
        // Store stops directly in payload
        self.payload.stops.extend_from_slice(stops);
    }

    pub fn push_backdrop_filter(
        &mut self,
        common: &di::CommonItemProperties,
        filters: &[di::FilterOp],
        filter_datas: &[di::FilterData],
        filter_primitives: &[di::FilterPrimitive],
    ) {
        let common = di::CommonItemProperties {
            clip_rect: self.remap_bounds(common.clip_rect),
            ..*common
        };

        self.push_filters(filters, filter_datas, filter_primitives);

        let item = di::DisplayItem::BackdropFilter(di::BackdropFilterDisplayItem { common });
        self.push_item(&item);
    }

    pub fn push_filters(
        &mut self,
        filters: &[di::FilterOp],
        filter_datas: &[di::FilterData],
        filter_primitives: &[di::FilterPrimitive],
    ) {
        if !filters.is_empty() {
            self.push_item(&di::DisplayItem::SetFilterOps { filter_count: filters.len() });
            // Store filters directly in payload
            self.payload.filters.extend_from_slice(filters);
        }

        for filter_data in filter_datas {
            self.push_item(&di::DisplayItem::SetFilterData);
            // Store filter data directly in payload
            self.payload.filter_data.push(filter_data.clone());
        }

        if !filter_primitives.is_empty() {
            self.push_item(&di::DisplayItem::SetFilterPrimitives { primitive_count: filter_primitives.len() });
            // Store filter primitives directly in payload
            self.payload
                .filter_primitives
                .extend_from_slice(filter_primitives);
        }
    }

    fn generate_clip_index(&mut self) -> di::ClipId {
        self.next_clip_index += 1;
        di::ClipId(self.next_clip_index - 1, self.pipeline_id)
    }

    fn generate_spatial_index(&mut self) -> di::SpatialId {
        self.next_spatial_index += 1;
        di::SpatialId::new(self.next_spatial_index - 1, self.pipeline_id)
    }

    fn generate_clip_chain_id(&mut self) -> di::ClipChainId {
        self.next_clip_chain_id += 1;
        di::ClipChainId(self.next_clip_chain_id - 1, self.pipeline_id)
    }

    pub fn define_scroll_frame(
        &mut self,
        parent_space: di::SpatialId,
        external_id: di::ExternalScrollId,
        content_rect: LayoutRect,
        frame_rect: LayoutRect,
        external_scroll_offset: LayoutVector2D,
        scroll_offset_generation: APZScrollGeneration,
        has_scroll_linked_effect: HasScrollLinkedEffect,
        key: di::SpatialTreeItemKey,
    ) -> di::SpatialId {
        let scroll_frame_id = self.generate_spatial_index();
        let current_offset = self.rf_mapper.current_offset();

        let descriptor = di::SpatialTreeItem::ScrollFrame(di::ScrollFrameDescriptor {
            content_rect,
            frame_rect: frame_rect.translate(current_offset),
            parent_space,
            scroll_frame_id,
            external_id,
            external_scroll_offset,
            scroll_offset_generation,
            has_scroll_linked_effect,
            key,
        });

        self.push_spatial_tree_item(&descriptor);

        scroll_frame_id
    }

    pub fn define_clip_chain<I>(
        &mut self,
        parent: Option<di::ClipChainId>,
        clips: I,
    ) -> di::ClipChainId
    where
        I: IntoIterator<Item = di::ClipId>,
        I::IntoIter: ExactSizeIterator + Clone,
    {
        let id = self.generate_clip_chain_id();
        let clips_iter = clips.into_iter();
        let clip_count = clips_iter.len();
        self.push_item(&di::DisplayItem::ClipChain(di::ClipChainItem {
            id,
            parent,
            clip_count,
        }));
        // Store clip chain items directly in the payload
        self.payload.clip_chain_items.extend(clips_iter);
        id
    }

    pub fn define_clip_image_mask(
        &mut self,
        spatial_id: di::SpatialId,
        image_mask: di::ImageMask,
        points: &[LayoutPoint],
        fill_rule: di::FillRule,
    ) -> di::ClipId {
        let id = self.generate_clip_index();

        let current_offset = self.rf_mapper.current_offset();

        let image_mask = di::ImageMask {
            rect: image_mask.rect.translate(current_offset),
            ..image_mask
        };

        let item = di::DisplayItem::ImageMaskClip(di::ImageMaskClipDisplayItem {
            id,
            spatial_id,
            image_mask,
            fill_rule,
        });

        // We only need to supply points if there are at least 3, which is the
        // minimum to specify a polygon. BuiltDisplayListIter.next ensures that points
        // are cleared between processing other display items, so we'll correctly get
        // zero points when no SetPoints item has been pushed.
        if points.len() >= 3 {
            self.push_item(&di::DisplayItem::SetPoints { point_count: points.len() });
            // Store points directly in the payload
            self.payload.points.extend_from_slice(points);
        }
        self.push_item(&item);
        id
    }

    pub fn define_clip_rect(
        &mut self,
        spatial_id: di::SpatialId,
        clip_rect: LayoutRect,
    ) -> di::ClipId {
        let id = self.generate_clip_index();

        let current_offset = self.rf_mapper.current_offset();
        let clip_rect = clip_rect.translate(current_offset);

        let item = di::DisplayItem::RectClip(di::RectClipDisplayItem {
            id,
            spatial_id,
            clip_rect,
        });

        self.push_item(&item);
        id
    }

    pub fn define_clip_rounded_rect(
        &mut self,
        spatial_id: di::SpatialId,
        clip: di::ComplexClipRegion,
    ) -> di::ClipId {
        let id = self.generate_clip_index();

        let current_offset = self.rf_mapper.current_offset();

        let clip = di::ComplexClipRegion {
            rect: clip.rect.translate(current_offset),
            ..clip
        };

        let item = di::DisplayItem::RoundedRectClip(di::RoundedRectClipDisplayItem {
            id,
            spatial_id,
            clip,
        });

        self.push_item(&item);
        id
    }

    pub fn define_sticky_frame(
        &mut self,
        parent_spatial_id: di::SpatialId,
        frame_rect: LayoutRect,
        margins: SideOffsets2D<Option<f32>, LayoutPixel>,
        vertical_offset_bounds: di::StickyOffsetBounds,
        horizontal_offset_bounds: di::StickyOffsetBounds,
        previously_applied_offset: LayoutVector2D,
        key: di::SpatialTreeItemKey,
        // TODO: The caller only ever passes an identity transform.
        // Could we pass just an (optional) animation id instead?
        transform: Option<PropertyBinding<LayoutTransform>>,
    ) -> di::SpatialId {
        let id = self.generate_spatial_index();
        let current_offset = self.rf_mapper.current_offset();

        let descriptor = di::SpatialTreeItem::StickyFrame(di::StickyFrameDescriptor {
            parent_spatial_id,
            id,
            bounds: frame_rect.translate(current_offset),
            margins,
            vertical_offset_bounds,
            horizontal_offset_bounds,
            previously_applied_offset,
            key,
            transform,
        });

        self.push_spatial_tree_item(&descriptor);
        id
    }

    pub fn push_iframe(
        &mut self,
        bounds: LayoutRect,
        clip_rect: LayoutRect,
        space_and_clip: &di::SpaceAndClipInfo,
        pipeline_id: PipelineId,
        ignore_missing_pipeline: bool,
    ) {
        let current_offset = self.rf_mapper.current_offset();
        let bounds = bounds.translate(current_offset);
        let clip_rect = clip_rect.translate(current_offset);

        let item = di::DisplayItem::Iframe(di::IframeDisplayItem {
            bounds,
            clip_rect,
            space_and_clip: *space_and_clip,
            pipeline_id,
            ignore_missing_pipeline,
        });
        self.push_item(&item);
    }

    pub fn push_shadow(
        &mut self,
        space_and_clip: &di::SpaceAndClipInfo,
        shadow: di::Shadow,
        should_inflate: bool,
    ) {
        let item = di::DisplayItem::PushShadow(di::PushShadowDisplayItem {
            space_and_clip: *space_and_clip,
            shadow,
            should_inflate,
        });
        self.push_item(&item);
    }

    pub fn pop_all_shadows(&mut self) {
        self.push_item(&di::DisplayItem::PopAllShadows);
    }

    pub fn start_item_group(&mut self) {
        debug_assert!(!self.writing_to_chunk);
        debug_assert!(self.pending_chunk.is_empty());

        self.writing_to_chunk = true;
    }

    fn flush_pending_item_group(&mut self, key: di::ItemKey) {
        // Push RetainedItems-marker to items
        self.push_retained_items(key);

        // Note: pending_chunk no longer used in direct storage mode
        // self.payload.cache_data.append(&mut self.pending_chunk);

        // Push ReuseItems-marker to items
        self.push_reuse_items(key);
    }

    pub fn finish_item_group(&mut self, key: di::ItemKey) -> bool {
        debug_assert!(self.writing_to_chunk);
        self.writing_to_chunk = false;

        if self.pending_chunk.is_empty() {
            return false;
        }

        self.flush_pending_item_group(key);
        true
    }

    pub fn cancel_item_group(&mut self, discard: bool) {
        debug_assert!(self.writing_to_chunk);
        self.writing_to_chunk = false;

        if discard {
            self.pending_chunk.clear();
        } else {
            // Note: pending_chunk no longer used in direct storage mode
            // self.payload.items_data.append(&mut self.pending_chunk);
        }
    }

    pub fn push_reuse_items(&mut self, key: di::ItemKey) {
        self.push_item_to_section(&di::DisplayItem::ReuseItems(key), DisplayListSection::Data);
    }

    fn push_retained_items(&mut self, key: di::ItemKey) {
        self.push_item_to_section(
            &di::DisplayItem::RetainedItems(key),
            DisplayListSection::CacheData,
        );
    }

    pub fn set_cache_size(&mut self, cache_size: usize) {
        self.cache_size = cache_size;
    }

    pub fn begin(&mut self) {
        assert_eq!(self.state, BuildState::Idle);
        self.state = BuildState::Build;
        self.builder_start_time = precise_time_ns();
        self.reset();
    }

    pub fn end(&mut self) -> (PipelineId, BuiltDisplayList) {
        assert_eq!(self.state, BuildState::Build);
        assert!(
            self.save_state.is_none(),
            "Finalized DisplayListBuilder with a pending save"
        );

        // Debug serialization - disabled to avoid console output
        self.serialized_content_buffer = None;

        // While the first display list after tab-switch can be large, the
        // following ones are always smaller thanks to interning. We attempt
        // to reserve the same capacity again, although it may fail. Memory
        // pressure events will cause us to release our buffers if we ask for
        // too much. See bug 1531819 for related OOM issues.
        let next_capacity = DisplayListCapacity {
            cache_size: 0, // Not used anymore
            items_size: self.payload.items.len(),
            spatial_tree_size: self.payload.spatial_items.len(),
        };
        let payload = mem::replace(&mut self.payload, DisplayListPayload::new(next_capacity));
        let end_time = precise_time_ns();

        self.state = BuildState::Idle;

        (
            self.pipeline_id,
            BuiltDisplayList {
                descriptor: BuiltDisplayListDescriptor {
                    gecko_display_list_type: GeckoDisplayListType::None,
                    builder_start_time: self.builder_start_time,
                    builder_finish_time: end_time,
                    send_start_time: end_time,
                    total_clip_nodes: self.next_clip_index,
                    total_spatial_nodes: self.next_spatial_index,
                    cache_size: self.cache_size,
                },
                payload,
            },
        )
    }
}
// and iterated via BuiltDisplayList::iter_spatial_tree method

/// The offset stack for a given reference frame.
#[derive(Clone)]
struct ReferenceFrameState {
    /// A stack of current offsets from the current reference frame scope.
    offsets: Vec<LayoutVector2D>,
}

/// Maps from stacking context layout coordinates into reference frame
/// relative coordinates.
#[derive(Clone)]
pub struct ReferenceFrameMapper {
    /// A stack of reference frame scopes.
    frames: Vec<ReferenceFrameState>,
}

impl ReferenceFrameMapper {
    pub fn new() -> Self {
        ReferenceFrameMapper {
            frames: vec![ReferenceFrameState {
                offsets: vec![LayoutVector2D::zero()],
            }],
        }
    }

    /// Push a new scope. This resets the current offset to zero, and is
    /// used when a new reference frame or iframe is pushed.
    pub fn push_scope(&mut self) {
        self.frames.push(ReferenceFrameState {
            offsets: vec![LayoutVector2D::zero()],
        });
    }

    /// Pop a reference frame scope off the stack.
    pub fn pop_scope(&mut self) {
        self.frames.pop().unwrap();
    }

    /// Push a new offset for the current scope. This is used when
    /// a new stacking context is pushed.
    pub fn push_offset(&mut self, offset: LayoutVector2D) {
        let frame = self.frames.last_mut().unwrap();
        let current_offset = *frame.offsets.last().unwrap();
        frame.offsets.push(current_offset + offset);
    }

    /// Pop a local stacking context offset from the current scope.
    pub fn pop_offset(&mut self) {
        let frame = self.frames.last_mut().unwrap();
        frame.offsets.pop().unwrap();
    }

    /// Retrieve the current offset to allow converting a stacking context
    /// relative coordinate to be relative to the owing reference frame.
    /// TODO(gw): We could perhaps have separate coordinate spaces for this,
    ///           however that's going to either mean a lot of changes to
    ///           public API code, or a lot of changes to internal code.
    ///           Before doing that, we should revisit how Gecko would
    ///           prefer to provide coordinates.
    /// TODO(gw): For now, this includes only the reference frame relative
    ///           offset. Soon, we will expand this to include the initial
    ///           scroll offsets that are now available on scroll nodes. This
    ///           will allow normalizing the coordinates even between display
    ///           lists where APZ has scrolled the content.
    pub fn current_offset(&self) -> LayoutVector2D {
        *self.frames.last().unwrap().offsets.last().unwrap()
    }
}
