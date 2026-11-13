//! How each travel mode uses the graph: access, the time an edge takes,
//! the time and the cost of a turn, and the costs that are not time.

use crate::{
    format::{flag, valhalla, Edge},
    mode::{Mode, Turn},
};

fn road(access: u16) -> Edge {
    Edge {
        from: 0,
        to: 1,
        length_m: 1000,
        speed_kmh: 50,
        access,
        class: 5,
        use_: 0,
        density: 0,
        flags: 0,
        local_index: 0,
        opp_local_index: 0,
        restrictions: 0,
        shape: 0,
        reversed: false,
        stop_impact: 0,
        same_name: 0,
    }
}

#[test]
fn a_one_way_edge_is_closed_to_cars_but_open_to_walkers() {
    let against = road(valhalla::PEDESTRIAN | valhalla::BICYCLE);
    assert!(!Mode::Car.may_use(&against));
    assert!(Mode::Walk.may_use(&against));
    assert!(Mode::Bike.may_use(&against));
}

#[test]
fn a_kilometre_at_fifty_takes_seventy_two_seconds_by_car() {
    assert_eq!(Mode::Car.time_ms(&road(valhalla::ALL), 1000.0), 72_000);
}

#[test]
fn a_mountain_path_is_left_out_for_walkers() {
    let mut path = road(valhalla::ALL);
    path.flags = 3 << flag::SAC_SHIFT;
    assert!(!Mode::Walk.may_use(&path));
}

#[test]
fn turning_back_takes_long_and_going_straight_almost_nothing() {
    let e = road(valhalla::ALL);
    let at = |degree| Turn { degree, roads: 4, crossing: false, node_info: 0 };
    let (straight, _) = Mode::Car.turn(&e, &e, &at(0));
    let (right, _) = Mode::Car.turn(&e, &e, &at(90));
    let (left, _) = Mode::Car.turn(&e, &e, &at(270));
    let (back, _) = Mode::Car.turn(&e, &e, &at(180));
    assert!(straight < 100, "{straight}");
    assert!(right < left, "a right turn is easier where traffic keeps right: {right} {left}");
    assert!(back > 20_000);
    // Where two roads only continue each other there is no intersection.
    let (bend, _) = Mode::Car.turn(&e, &e, &Turn { degree: 90, roads: 2, crossing: false, node_info: 0 });
    assert_eq!(bend, 0);
    assert_eq!(Mode::Walk.turn(&e, &e, &at(90)), (0, 0));
}

#[test]
fn a_turn_where_traffic_must_stop_costs_more_in_a_dense_area() {
    let prev = road(valhalla::ALL);
    let mut next = road(valhalla::ALL);
    next.stop_impact = 3; // from the edge whose way back has local index 0
    let quiet = Turn { degree: 270, roads: 4, crossing: false, node_info: 0 };
    let dense = Turn { node_info: 15, ..quiet };
    let (_, quiet_cost) = Mode::Car.turn(&prev, &next, &quiet);
    let (_, dense_cost) = Mode::Car.turn(&prev, &next, &dense);
    assert_eq!(quiet_cost, 7_500, "a left turn, 2.5 s, times stop impact 3");
    assert_eq!(dense_cost, 26_250);
}

#[test]
fn a_car_pays_more_per_second_in_a_dense_area_and_on_a_track() {
    let mut e = road(valhalla::ALL);
    assert_eq!(Mode::Car.edge_cost(&e, 1000), 850);
    e.density = 15;
    assert_eq!(Mode::Car.edge_cost(&e, 1000), 1225);
    e.use_ = valhalla::USE_TRACK;
    assert_eq!(Mode::Car.edge_cost(&e, 1000), 4900);
    assert_eq!(Mode::Walk.edge_cost(&e, 1000), 1000);
}

#[test]
fn boarding_a_ferry_costs_five_minutes_once() {
    let mut ferry = road(valhalla::ALL);
    ferry.use_ = valhalla::USE_FERRY;
    let land = road(valhalla::ALL);
    assert_eq!(Mode::Car.transition_ms(Some(&land), &ferry, 0), 300_000);
    assert_eq!(Mode::Car.transition_ms(Some(&ferry), &ferry, 0), 0);
}
