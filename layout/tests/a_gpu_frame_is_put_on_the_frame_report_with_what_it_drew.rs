//! The damage a WebRender frame leaves on the window's frame report: what the X11, Wayland and Windows
//! shells record for a frame they drew on the GPU (ledger E45).
//!
//! The e2e counters read the report (`assert_idle_stable`, `assert_work_bounded` and
//! `assert_state_machines_idle` fail with "NO FRAME was rendered since the last reset" when no frame was
//! recorded). A frame that drew nothing leaves no damage - an idle window that was asked for a frame must
//! read as idle - a frame that drew and names the rects it redrew keeps them, and one that drew and names
//! none is the whole window.

use azul_core::geom::{LogicalPosition, LogicalRect, LogicalSize};
use azul_layout::window::{FrameDamage, FrameReport};

fn rect(x: f32, y: f32, w: f32, h: f32) -> LogicalRect {
    LogicalRect::new(LogicalPosition::new(x, y), LogicalSize::new(w, h))
}

#[test]
fn a_frame_that_drew_nothing_leaves_no_damage_whatever_rects_it_names() {
    assert_eq!(FrameDamage::of_gpu_frame(false, &[]), FrameDamage::None);
    assert_eq!(
        FrameDamage::of_gpu_frame(false, &[rect(0.0, 0.0, 10.0, 10.0)]),
        FrameDamage::None
    );
}

#[test]
fn a_frame_that_drew_and_names_no_rect_is_the_whole_window() {
    assert_eq!(FrameDamage::of_gpu_frame(true, &[]), FrameDamage::Full);
}

#[test]
fn a_frame_that_drew_keeps_the_rects_it_names() {
    let rects = [rect(1.0, 2.0, 3.0, 4.0), rect(10.0, 20.0, 30.0, 40.0)];
    assert_eq!(
        FrameDamage::of_gpu_frame(true, &rects),
        FrameDamage::Rects(rects.to_vec())
    );
}

/// A shell that records the frames it draws: the report counts them since the last reset, and an idle frame
/// (nothing drawn) leaves the last frame's damage empty, which is what `assert_idle_stable` asks.
#[test]
fn a_recorded_gpu_frame_counts_and_an_idle_one_leaves_no_damage() {
    let mut report = FrameReport::default();
    let drawn = FrameDamage::of_gpu_frame(true, &[rect(0.0, 0.0, 50.0, 20.0)]);
    report.record_frame_at_generation(0, drawn.clone(), drawn);
    assert_eq!(report.frames_since_reset, 1);
    assert!(!report.paint_damage.is_none());

    let idle = FrameDamage::of_gpu_frame(false, &[]);
    report.record_frame_at_generation(0, idle.clone(), idle);
    assert_eq!(report.frames_since_reset, 2);
    assert!(report.paint_damage.is_none(), "the idle frame is the last one: no damage");
}
