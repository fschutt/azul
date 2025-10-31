//! Hover state management for tracking mouse hover history
//!
//! The HoverManager records hit test results over multiple frames to enable
//! gesture detection (like DragStart) that requires analyzing hover patterns
//! over time rather than just the current frame.

use std::collections::VecDeque;

use crate::hit_test::FullHitTest;

/// Maximum number of frames to keep in hover history
const MAX_HOVER_HISTORY: usize = 5;

/// Manages hover state history for gesture detection
///
/// Records hit test results over multiple frames to enable:
/// - DragStart detection (requires movement threshold over multiple frames)
/// - Hover-over event detection
/// - Mouse path analysis
///
/// The manager maintains a fixed-size ring buffer of the last N frames.
#[derive(Debug, Clone, PartialEq)]
pub struct HoverManager {
    /// Ring buffer of recent hit test results (most recent first)
    /// Limited to MAX_HOVER_HISTORY entries
    hover_history: VecDeque<FullHitTest>,
}

impl HoverManager {
    /// Create a new empty HoverManager
    pub fn new() -> Self {
        Self {
            hover_history: VecDeque::with_capacity(MAX_HOVER_HISTORY),
        }
    }

    /// Push a new hit test result, maintaining the frame limit
    ///
    /// The most recent result is always at index 0.
    /// If the history is full, the oldest frame is dropped.
    pub fn push_hit_test(&mut self, hit_test: FullHitTest) {
        // Add to front (most recent)
        self.hover_history.push_front(hit_test);

        // Remove oldest if we exceed the limit
        if self.hover_history.len() > MAX_HOVER_HISTORY {
            self.hover_history.pop_back();
        }
    }

    /// Get the most recent hit test result
    ///
    /// Returns None if no hit tests have been recorded yet.
    pub fn get_current(&self) -> Option<&FullHitTest> {
        self.hover_history.front()
    }

    /// Get the hit test result from N frames ago (0 = current frame)
    ///
    /// Returns None if the requested frame is not in history.
    pub fn get_frame(&self, frames_ago: usize) -> Option<&FullHitTest> {
        self.hover_history.get(frames_ago)
    }

    /// Get the entire hover history (most recent first)
    pub fn get_history(&self) -> &VecDeque<FullHitTest> {
        &self.hover_history
    }

    /// Get the number of frames in history
    pub fn frame_count(&self) -> usize {
        self.hover_history.len()
    }

    /// Clear all hover history
    pub fn clear(&mut self) {
        self.hover_history.clear();
    }

    /// Check if we have enough frames for gesture detection
    ///
    /// DragStart detection requires analyzing movement over multiple frames.
    /// This returns true if we have at least 2 frames of history.
    pub fn has_sufficient_history_for_gestures(&self) -> bool {
        self.hover_history.len() >= 2
    }
}

impl Default for HoverManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hover_manager_push_and_get() {
        let mut manager = HoverManager::new();
        assert_eq!(manager.frame_count(), 0);

        let hit1 = FullHitTest::empty(None);
        manager.push_hit_test(hit1.clone());
        assert_eq!(manager.frame_count(), 1);
        assert_eq!(manager.get_current(), Some(&hit1));
    }

    #[test]
    fn test_hover_manager_frame_limit() {
        let mut manager = HoverManager::new();

        // Push 7 frames (more than MAX_HOVER_HISTORY = 5)
        for _ in 0..7 {
            let hit = FullHitTest::empty(None);
            manager.push_hit_test(hit);
        }

        // Should only keep the last 5
        assert_eq!(manager.frame_count(), MAX_HOVER_HISTORY);
    }

    #[test]
    fn test_gesture_history_check() {
        let mut manager = HoverManager::new();
        assert!(!manager.has_sufficient_history_for_gestures());

        manager.push_hit_test(FullHitTest::empty(None));
        assert!(!manager.has_sufficient_history_for_gestures());

        manager.push_hit_test(FullHitTest::empty(None));
        assert!(manager.has_sufficient_history_for_gestures());
    }
}
