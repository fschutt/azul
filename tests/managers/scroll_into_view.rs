use azul_layout::managers::scroll_into_view::*;

#[test]
fn test_calculate_axis_delta_nearest_visible() {
    let delta = calculate_axis_delta(
        100.0, 50.0, 50.0, 200.0,
        ScrollLogicalPosition::Nearest,
    );
    assert_eq!(delta, 0.0);
}

#[test]
fn test_calculate_axis_delta_nearest_above() {
    let delta = calculate_axis_delta(
        20.0, 50.0, 100.0, 200.0,
        ScrollLogicalPosition::Nearest,
    );
    assert_eq!(delta, -80.0);
}

#[test]
fn test_calculate_axis_delta_nearest_below() {
    let delta = calculate_axis_delta(
        280.0, 50.0, 100.0, 200.0,
        ScrollLogicalPosition::Nearest,
    );
    assert_eq!(delta, 30.0);
}

#[test]
fn test_calculate_axis_delta_center() {
    let delta = calculate_axis_delta(
        50.0, 20.0, 100.0, 200.0,
        ScrollLogicalPosition::Center,
    );
    assert_eq!(delta, -140.0);
}

#[test]
fn test_calculate_axis_delta_start() {
    let delta = calculate_axis_delta(
        150.0, 50.0, 100.0, 200.0,
        ScrollLogicalPosition::Start,
    );
    assert_eq!(delta, 50.0);
}

#[test]
fn test_calculate_axis_delta_end() {
    let delta = calculate_axis_delta(
        150.0, 50.0, 100.0, 200.0,
        ScrollLogicalPosition::End,
    );
    assert_eq!(delta, -100.0);
}
