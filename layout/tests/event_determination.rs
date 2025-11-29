//! Event determination tests - currently disabled pending API export
//!
//! These tests require types that are not currently exported.

// Disabled: functions and types not exported
#![cfg(feature = "DISABLED_event_tests")]

use azul_layout::event_determination::*;

#[test]
fn test_detect_window_resize() {
    let timestamp = Instant::Tick(SystemTick::new(0));
    let mut prev_state = FullWindowState::default();
    let mut curr_state = FullWindowState::default();

    prev_state.size = azul_core::window::WindowSize {
        dimensions: azul_core::geom::LogicalSize {
            width: 800.0,
            height: 600.0,
        },
        dpi: 96,
        min_dimensions: Default::default(),
        max_dimensions: Default::default(),
    };
    curr_state.size = azul_core::window::WindowSize {
        dimensions: azul_core::geom::LogicalSize {
            width: 1024.0,
            height: 768.0,
        },
        dpi: 96,
        min_dimensions: Default::default(),
        max_dimensions: Default::default(),
    };

    let events = detect_window_state_events(&curr_state, &prev_state, timestamp);

    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event_type, EventType::WindowResize);
}

#[test]
fn test_determine_events_deduplicates() {
    struct DummyManager;
    impl EventProvider for DummyManager {
        fn get_pending_events(&self, timestamp: Instant) -> Vec<SyntheticEvent> {
            // Return duplicate event
            vec![SyntheticEvent::new(
                EventType::Input,
                EventSource::User,
                DomNodeId {
                    dom: DomId { inner: 0 },
                    node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::ZERO)),
                },
                timestamp,
                EventData::None,
            )]
        }
    }

    let timestamp = Instant::Tick(SystemTick::new(0));
    let prev_state = FullWindowState::default();
    let curr_state = FullWindowState::default();

    let manager1 = DummyManager;
    let manager2 = DummyManager;
    let managers: Vec<&dyn EventProvider> = vec![&manager1, &manager2];

    let events = determine_events_from_managers(&curr_state, &prev_state, &managers, timestamp);

    // Should deduplicate the two identical Input events
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event_type, EventType::Input);
}
