//! Paged layout tests - currently disabled pending API export
//!
//! These tests require types that are not currently exported from
//! azul_layout::paged module.

// Disabled: FragmentationContext, Fragmentainer, LogicalSize not exported
#![cfg(feature = "DISABLED_paged_tests")]

use azul_layout::paged::*;

#[test]
#[ignore] // Requires paged module API
fn test_continuous_context_has_infinite_space() {
    let ctx = FragmentationContext::new_continuous(800.0);
    assert_eq!(ctx.fragmentainer_count(), 1);
    assert_eq!(ctx.current().remaining_space(), f32::MAX);
    assert!(!ctx.current().is_full());
}

#[test]
#[ignore] // Requires paged module API
fn test_paged_context_has_fixed_space() {
    let ctx = FragmentationContext::new_paged(LogicalSize::new(800.0, 1000.0));
    assert_eq!(ctx.fragmentainer_count(), 1);
    assert_eq!(ctx.current().remaining_space(), 1000.0);
    assert!(!ctx.current().is_full());
}

#[test]
#[ignore] // Requires paged module API
fn test_fragmentainer_tracks_used_space() {
    let mut fragmentainer = Fragmentainer::new(LogicalSize::new(800.0, 1000.0), true);
    assert_eq!(fragmentainer.remaining_space(), 1000.0);

    fragmentainer.use_space(300.0);
    assert_eq!(fragmentainer.remaining_space(), 700.0);

    fragmentainer.use_space(600.0);
    assert_eq!(fragmentainer.remaining_space(), 100.0);
}

#[test]
#[ignore] // Requires paged module API
fn test_fragmentainer_can_fit_checks_space() {
    let mut fragmentainer = Fragmentainer::new(LogicalSize::new(800.0, 1000.0), true);
    assert!(fragmentainer.can_fit(500.0));
    assert!(fragmentainer.can_fit(1000.0));
    assert!(!fragmentainer.can_fit(1001.0));

    fragmentainer.use_space(700.0);
    assert!(fragmentainer.can_fit(200.0));
    assert!(fragmentainer.can_fit(300.0));
    assert!(!fragmentainer.can_fit(301.0));
}

#[test]
#[ignore] // Requires paged module API
fn test_fragmentainer_is_full_when_space_exhausted() {
    let mut fragmentainer = Fragmentainer::new(LogicalSize::new(800.0, 1000.0), true);
    assert!(!fragmentainer.is_full());

    // After using 999px, we have 1px remaining - not full yet
    fragmentainer.use_space(999.0);
    assert!(!fragmentainer.is_full()); // Still has exactly 1px

    // After using 0.1px more, we have 0.9px remaining - now it's full (< 1px)
    fragmentainer.use_space(0.1);
    assert!(fragmentainer.is_full()); // Less than 1px remaining
}

#[test]
#[ignore] // Requires paged module API
fn test_paged_context_advances_creates_new_page() {
    let mut ctx = FragmentationContext::new_paged(LogicalSize::new(800.0, 1000.0));
    assert_eq!(ctx.fragmentainer_count(), 1);

    ctx.advance().unwrap();
    assert_eq!(ctx.fragmentainer_count(), 2);

    ctx.advance().unwrap();
    assert_eq!(ctx.fragmentainer_count(), 3);
}

#[test]
#[ignore] // Requires paged module API
fn test_continuous_context_advance_is_noop() {
    let mut ctx = FragmentationContext::new_continuous(800.0);
    assert_eq!(ctx.fragmentainer_count(), 1);

    ctx.advance().unwrap();
    assert_eq!(ctx.fragmentainer_count(), 1); // Still 1, doesn't create new containers
}

#[test]
#[ignore] // Requires paged module API
fn test_fragmentainer_never_full_for_continuous() {
    let mut fragmentainer = Fragmentainer::new(LogicalSize::new(800.0, f32::MAX), false);
    assert_eq!(fragmentainer.remaining_space(), f32::MAX);
    assert!(!fragmentainer.is_full());

    fragmentainer.use_space(10000.0);
    assert_eq!(fragmentainer.remaining_space(), f32::MAX);
    assert!(!fragmentainer.is_full());
}
