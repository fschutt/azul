//! Hover state management for tracking mouse and touch hover history
//!
//! The HoverManager records hit test results for multiple input points
//! (mouse, touch, pen) over multiple frames to enable gesture detection
//! (like DragStart) that requires analyzing hover patterns over time
//! rather than just the current frame.

use std::collections::{BTreeMap, VecDeque};

use crate::hit_test::FullHitTest;

/// Maximum number of frames to keep in hover history
const MAX_HOVER_HISTORY: usize = 5;

/// Identifier for an input point (mouse, touch, pen, etc.)
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum InputPointId {
    /// Mouse cursor
    Mouse,
    /// Touch point with unique ID (from TouchEvent.id)
    Touch(u64),
}

/// Manages hover state history for all input points
///
/// Records hit test results for mouse and touch inputs over multiple frames:
/// - DragStart detection (requires movement threshold over multiple frames)
/// - Hover-over event detection
/// - Multi-touch gesture detection
/// - Input path analysis
///
/// The manager maintains a separate history for each active input point.
#[derive(Debug, Clone, PartialEq)]
pub struct HoverManager {
    /// Hit test history for each input point
    /// Each point has its own ring buffer of the last N frames
    hover_histories: BTreeMap<InputPointId, VecDeque<FullHitTest>>,
}

impl HoverManager {
    /// Create a new empty HoverManager
    pub fn new() -> Self {
        Self {
            hover_histories: BTreeMap::new(),
        }
    }

    /// Push a new hit test result for a specific input point
    ///
    /// The most recent result is always at index 0 for that input point.
    /// If the history is full, the oldest frame is dropped.
    pub fn push_hit_test(&mut self, input_id: InputPointId, hit_test: FullHitTest) {
        let history = self
            .hover_histories
            .entry(input_id)
            .or_insert_with(|| VecDeque::with_capacity(MAX_HOVER_HISTORY));

        // Add to front (most recent)
        history.push_front(hit_test);

        // Remove oldest if we exceed the limit
        if history.len() > MAX_HOVER_HISTORY {
            history.pop_back();
        }
    }

    /// Remove an input point's history (e.g., when touch ends)
    pub fn remove_input_point(&mut self, input_id: &InputPointId) {
        self.hover_histories.remove(input_id);
    }

    /// Get the most recent hit test result for an input point
    ///
    /// Returns None if no hit tests have been recorded for this input point.
    pub fn get_current(&self, input_id: &InputPointId) -> Option<&FullHitTest> {
        self.hover_histories
            .get(input_id)
            .and_then(|history| history.front())
    }

    /// Get the most recent mouse cursor hit test (convenience method)
    pub fn get_current_mouse(&self) -> Option<&FullHitTest> {
        self.get_current(&InputPointId::Mouse)
    }

    /// Get the hit test result from N frames ago for an input point
    // (0 = current frame)
    ///
    /// Returns None if the requested frame is not in history.
    pub fn get_frame(&self, input_id: &InputPointId, frames_ago: usize) -> Option<&FullHitTest> {
        self.hover_histories
            .get(input_id)
            .and_then(|history| history.get(frames_ago))
    }

    /// Get the entire hover history for an input point (most recent first)
    pub fn get_history(&self, input_id: &InputPointId) -> Option<&VecDeque<FullHitTest>> {
        self.hover_histories.get(input_id)
    }

    /// Get all currently tracked input points
    pub fn get_active_input_points(&self) -> Vec<InputPointId> {
        self.hover_histories.keys().copied().collect()
    }

    /// Get the number of frames in history for an input point
    pub fn frame_count(&self, input_id: &InputPointId) -> usize {
        self.hover_histories
            .get(input_id)
            .map(|h| h.len())
            .unwrap_or(0)
    }

    /// Clear all hover history for all input points
    pub fn clear(&mut self) {
        self.hover_histories.clear();
    }

    /// Clear history for a specific input point
    pub fn clear_input_point(&mut self, input_id: &InputPointId) {
        if let Some(history) = self.hover_histories.get_mut(input_id) {
            history.clear();
        }
    }

    /// Check if we have enough frames for gesture detection on an input point
    ///
    /// DragStart detection requires analyzing movement over multiple frames.
    /// This returns true if we have at least 2 frames of history.
    pub fn has_sufficient_history_for_gestures(&self, input_id: &InputPointId) -> bool {
        self.frame_count(input_id) >= 2
    }

    /// Check if any input point has enough history for gesture detection
    pub fn any_has_sufficient_history_for_gestures(&self) -> bool {
        self.hover_histories
            .iter()
            .any(|(_, history)| history.len() >= 2)
    }
}

impl Default for HoverManager {
    fn default() -> Self {
        Self::new()
    }
}
