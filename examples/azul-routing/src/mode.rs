//! How each travel mode uses the graph: which edges and nodes it may take,
//! how long an edge takes, and what passing a node costs.
//!
//! Valhalla already decided who may use each edge in which direction (its
//! access masks hold one-way streets, `access=*`, country rules and
//! barriers), so this module mostly reads those bits. It is a small part of
//! Valhalla's auto, bicycle and pedestrian costing (`src/sif/*cost.cc`): the
//! same hard rules (access, U-turns, turn restrictions, impassable surfaces,
//! hiking difficulty) and the same default time penalties for gates, toll
//! booths, ferries and borders, and its "maneuver penalty" that makes a
//! route keep to one street rather than zig-zag.
//!
//! A car also loses time turning, and Valhalla 3.9 counts that twice over:
//! the time itself follows OSRM's car model (a few seconds for a sharp turn
//! at an intersection, more for one across traffic, two for a traffic
//! signal), and the route's cost adds Valhalla's own turn penalty (the turn's
//! kind times how much traffic has to stop there, more in dense areas). Both
//! are ported in [`Mode::turn`]. Bicycles and walkers turn for free here.

use crate::format::{flag, node_info, valhalla, Edge};

/// How the traveller goes. (Public transport is a timetable question, not a
/// road graph: the app asks timetable providers for it.)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Mode {
    /// Valhalla's `auto` costing.
    Car,
    /// Valhalla's `bicycle` access rules, with a simpler speed model.
    Bike,
    /// Valhalla's `pedestrian` costing.
    Walk,
}

/// A bicycle's speed on a smooth road, km/h: Valhalla's default for its
/// "hybrid" bicycle.
const BIKE_KMH: f64 = 18.0;
/// Walking speed, km/h: Valhalla's pedestrian default.
const WALK_KMH: f64 = 5.1;
/// A car's top speed, km/h: Valhalla's default `top_speed`.
const CAR_TOP_KMH: f64 = 140.0;
/// How much of a bicycle's speed each surface leaves, smooth paving (0) to
/// a path (6).
const BIKE_SURFACE: [f64; 7] = [1.0, 1.0, 0.9, 0.85, 0.75, 0.75, 0.6];

/// Seconds Valhalla adds for passing a gate.
const GATE_S: u64 = 30;
/// Seconds for a toll booth.
const TOLL_BOOTH_S: u64 = 15;
/// Seconds for getting onto a ferry.
const FERRY_S: u64 = 300;
/// Seconds for crossing a border.
const BORDER_S: u64 = 600;
/// The extra cost (not time) of a road meant only for getting to a place on
/// it: such roads are used near the ends of a trip, not as short cuts.
const DEST_ONLY_S: u64 = 600;
/// The extra cost (not time) of going on in a street of another name:
/// Valhalla's default `maneuver_penalty`.
const MANEUVER_S: u64 = 5;
/// The extra cost (not time) of turning into a service road: a car's
/// default `service_penalty`.
const SERVICE_S: u64 = 75;
/// How much a car's cost per second grows with unpaved surfaces: Valhalla's
/// surface factors (smooth paving .. path) times its `surface_factor` 0.5.
const CAR_SURFACE: [f64; 7] = [0.0, 0.0, 0.0, 0.05, 0.1, 0.25, 0.5];
/// The lowest a car's cost factor gets (an empty area): A*'s estimate of
/// the cost left is scaled by it so that it never guesses too high.
pub const CAR_MIN_FACTOR: f64 = 0.85;

/// One turn at a node, as the car's turn model sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Turn {
    /// The angle turned, degrees clockwise: 0 straight on, 90 right, 180
    /// back, 270 left.
    pub degree: u32,
    /// How many roads meet at the node.
    pub roads: usize,
    /// Roads branch off both to the left and to the right of the way
    /// through: going straight across them.
    pub crossing: bool,
    /// The node's [`node_info`] bits.
    pub node_info: u8,
}

/// Valhalla's turn kinds, by the angle turned (`baldr::Turn::GetType`):
/// 0 straight, 1 slight right, 2 right, 3 sharp right, 4 reverse, 5 sharp
/// left, 6 left, 7 slight left.
#[must_use]
pub const fn turn_kind(degree: u32) -> usize {
    match degree {
        0..=10 | 350.. => 0,
        11..=44 => 1,
        45..=135 => 2,
        136..=169 => 3,
        170..=190 => 4,
        191..=224 => 5,
        225..=315 => 6,
        _ => 7,
    }
}

/// Valhalla's car turn penalties by turn kind, seconds, where traffic keeps
/// right; where it keeps left, left and right swap.
const TURN_COSTS_RIGHT: [f64; 8] = [0.5, 0.75, 1.0, 1.5, 9.5, 3.5, 2.5, 0.75];
const TURN_COSTS_LEFT: [f64; 8] = [0.5, 0.75, 2.5, 3.5, 9.5, 1.5, 1.0, 0.75];
/// The penalty for going straight across a road.
const TURN_COST_CROSSING: f64 = 2.0;
/// Onto or off a ramp, and more for a roundabout.
const TURN_COST_RAMP: f64 = 1.5;
const TURN_COST_ROUNDABOUT: f64 = 0.5;
/// How much more turning costs where roads are dense (by node density).
const DENSITY_FACTOR: [f64; 16] = [1.0, 1.0, 1.0, 1.0, 1.0, 1.1, 1.2, 1.3, 1.4, 1.6, 1.9, 2.2, 2.5, 2.8, 3.1, 3.5];

/// OSRM's car turn time, seconds, for an angle (degrees clockwise): an
/// S-curve from about none going straight to 7.5 s turning back, a little
/// longer for turns across the traffic (`valhalla/sif/osrm_car_duration.h`).
fn osrm_turn_seconds(degree: u32, keep_right: bool) -> f64 {
    const PENALTY: f64 = 7.5;
    const BIAS: f64 = 1.075;
    let signed = if degree > 180 { f64::from(degree) - 360.0 } else { f64::from(degree) };
    // A right turn (positive) is the easy one where traffic keeps right.
    let easy = (signed >= 0.0) == keep_right;
    let (slope, shift) = if easy { (1.0 / BIAS, BIAS) } else { (BIAS, 1.0 / BIAS) };
    PENALTY / (1.0 + (-(13.0 * slope * signed.abs() / 180.0 - 6.5 * shift)).exp())
}

impl Mode {
    /// The time a turn takes and its extra cost, both ms: OSRM's car turn
    /// time and Valhalla's car turn penalty. Nothing for other modes.
    #[must_use]
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    pub fn turn(self, prev: &Edge, next: &Edge, turn: &Turn) -> (u64, u64) {
        if self != Self::Car {
            return (0, 0);
        }
        let keep_right = turn.node_info & node_info::DRIVE_LEFT == 0;
        let kind = turn_kind(turn.degree % 360);
        let mut seconds = if turn.node_info & node_info::TRAFFIC_SIGNAL != 0 { 2.0 } else { 0.0 };
        if turn.roads > 2 || kind == 4 {
            seconds += osrm_turn_seconds(turn.degree % 360, keep_right);
            if kind == 4 {
                seconds += 20.0;
            }
        }
        let mut penalty = 0.0;
        let stop = next.stop_from(prev.opp_local_index);
        if stop > 0 {
            let mut cost = if turn.crossing {
                TURN_COST_CROSSING
            } else if keep_right {
                TURN_COSTS_RIGHT[kind]
            } else {
                TURN_COSTS_LEFT[kind]
            };
            let ramp = |e: &Edge| e.use_ == valhalla::USE_RAMP;
            if ramp(prev) != ramp(next) {
                cost += TURN_COST_RAMP;
                if next.has(flag::ROUNDABOUT) {
                    cost += TURN_COST_ROUNDABOUT;
                }
            }
            let density = usize::from(turn.node_info & node_info::DENSITY_MASK);
            penalty = cost * f64::from(stop) * DENSITY_FACTOR[density];
        }
        ((seconds * 1000.0).round() as u64, (penalty * 1000.0).round() as u64)
    }

    /// The mode's bit in Valhalla's access masks.
    #[must_use]
    pub const fn access_bit(self) -> u16 {
        match self {
            Self::Car => valhalla::AUTO,
            Self::Bike => valhalla::BICYCLE,
            Self::Walk => valhalla::PEDESTRIAN,
        }
    }

    /// Whether the mode may travel along `edge` (in its direction).
    #[must_use]
    pub fn may_use(self, edge: &Edge) -> bool {
        if edge.access & self.access_bit() == 0
            || edge.use_ >= valhalla::USE_FIRST_TRANSIT
            || edge.use_ == valhalla::USE_CONSTRUCTION
            || edge.surface() >= valhalla::SURFACE_IMPASSABLE
        {
            return false;
        }
        match self {
            // Valhalla's pedestrian default `max_hiking_difficulty` is 1:
            // marked mountain paths and worse are left to hikers who ask.
            Self::Walk => edge.sac_scale() <= 1,
            Self::Car | Self::Bike => true,
        }
    }

    /// Whether the mode may pass a node whose access mask is `access`.
    #[must_use]
    pub const fn may_pass(self, access: u16) -> bool {
        access & self.access_bit() != 0
    }

    /// Whether the mode obeys turn restrictions. Pedestrians do not, as in
    /// Valhalla.
    #[must_use]
    pub const fn obeys_turn_restrictions(self) -> bool {
        matches!(self, Self::Car | Self::Bike)
    }

    /// The mode's speed along `edge`, km/h.
    #[must_use]
    pub fn speed_kmh(self, edge: &Edge) -> f64 {
        let ferry = matches!(edge.use_, valhalla::USE_FERRY | valhalla::USE_RAIL_FERRY);
        let road = f64::from(edge.speed_kmh.clamp(5, 250));
        match self {
            Self::Car => road.min(CAR_TOP_KMH),
            _ if ferry => road,
            Self::Bike if edge.has(flag::DISMOUNT) => WALK_KMH,
            Self::Bike if edge.use_ == valhalla::USE_STEPS => 2.0,
            Self::Bike => BIKE_KMH * BIKE_SURFACE[usize::from(edge.surface().min(6))],
            Self::Walk if edge.use_ == valhalla::USE_STEPS => WALK_KMH * 0.6,
            Self::Walk => WALK_KMH,
        }
    }

    /// The fastest the mode ever goes, km/h: A*'s estimate of the time left
    /// divides the straight distance by it, so it never guesses too long.
    #[must_use]
    pub const fn top_speed_kmh(self) -> f64 {
        match self {
            // A ferry may be faster than walking or cycling; a trip that
            // takes one is rare enough that the estimate stays on land.
            Self::Car => CAR_TOP_KMH,
            Self::Bike => BIKE_KMH,
            Self::Walk => WALK_KMH,
        }
    }

    /// The time `metres` along `edge` take, ms (at least 1).
    #[must_use]
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    pub fn time_ms(self, edge: &Edge, metres: f64) -> u64 {
        let speed = self.speed_kmh(edge) / 3.6;
        ((metres / speed * 1000.0).ceil() as u64).max(1)
    }

    /// The time it takes to go from `prev` onto `next` through a node of kind
    /// `node_kind`, ms: gates, toll booths, borders and boarding a ferry.
    #[must_use]
    pub fn transition_ms(self, prev: Option<&Edge>, next: &Edge, node_kind: u8) -> u64 {
        let mut seconds = match node_kind {
            valhalla::NODE_GATE if self == Self::Car => GATE_S,
            valhalla::NODE_TOLL_BOOTH if self == Self::Car => TOLL_BOOTH_S,
            valhalla::NODE_BORDER_CONTROL => BORDER_S,
            _ => 0,
        };
        let is_ferry = |e: &Edge| matches!(e.use_, valhalla::USE_FERRY | valhalla::USE_RAIL_FERRY);
        if is_ferry(next) && !prev.is_some_and(is_ferry) {
            seconds += FERRY_S;
        }
        if next.has(flag::COUNTRY_CROSSING) {
            seconds += BORDER_S;
        }
        seconds * 1000
    }

    /// What travelling along `edge` for `time_ms` costs, ms. A car's cost is
    /// its time weighted as Valhalla's car costing does by default: more in
    /// dense areas and on unpaved roads, four times on tracks, 2.6 times on
    /// living streets. Other modes' cost is their time.
    #[must_use]
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, clippy::cast_precision_loss)]
    pub fn edge_cost(self, edge: &Edge, time_ms: u64) -> u64 {
        if self != Self::Car {
            return time_ms;
        }
        let ferry = matches!(edge.use_, valhalla::USE_FERRY | valhalla::USE_RAIL_FERRY);
        let mut factor = if ferry { 1.0 } else { CAR_MIN_FACTOR + 0.025 * f64::from(edge.density.min(15)) };
        factor += CAR_SURFACE[usize::from(edge.surface().min(6))];
        factor *= match edge.use_ {
            valhalla::USE_TRACK => 4.0,
            valhalla::USE_LIVING_STREET => 2.6,
            _ => 1.0,
        };
        (time_ms as f64 * factor).round() as u64
    }

    /// The extra cost of taking `next` after `prev`, ms, that is not time:
    /// entering a destination-only road, and going on in a street of
    /// another name (`renamed`) other than onto a ramp.
    #[must_use]
    pub fn penalty_ms(self, prev: Option<&Edge>, next: &Edge, renamed: bool) -> u64 {
        let dest_only = |e: &Edge| e.has(flag::DEST_ONLY);
        let mut seconds = 0;
        if self != Self::Walk && dest_only(next) && !prev.is_some_and(dest_only) {
            seconds += DEST_ONLY_S;
        }
        if prev.is_some() && renamed && !next.has(flag::LINK) {
            seconds += MANEUVER_S;
        }
        let service = |e: &Edge| e.use_ == valhalla::USE_SERVICE_ROAD;
        if self == Self::Car && service(next) && !prev.is_some_and(service) {
            seconds += SERVICE_S;
        }
        seconds * 1000
    }
}
