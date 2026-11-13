//! Turn-by-turn guidance along a road route, from the position fixes of a GPS (or of a
//! simulated drive): where on the route the car is, the next maneuver and how far it is, the
//! distance and time left, when to say what, when the car left the route, and arrival.
//!
//! A fix is noisy, so the guidance never takes it at its word: the position is snapped to the
//! route while it is near it; heading and speed are smoothed; leaving the route counts only when
//! the car stays off it for a few seconds, further than the fix's own uncertainty; and the
//! [`Rerouter`] spaces the requests for a new route, so a car that keeps driving its own way
//! does not ask the routing server every second.
//!
//! No azul types: the app feeds fixes in and shows and says what comes out.

use crate::{distance_m, online::Step};

/// How far from the route the car may be before it counts as off it, metres (more when the fix
/// is less certain than that).
pub const OFF_ROUTE_M: f64 = 40.0;

/// How long the car must stay off the route before a new route is asked for, seconds.
pub const OFF_ROUTE_FOR_S: f64 = 4.0;

/// How close to the destination counts as there, metres.
pub const ARRIVAL_M: f64 = 25.0;

/// The fewest seconds between two requests for a new route.
pub const REROUTE_GAP_S: f64 = 10.0;

/// The longest wait between two requests for a new route after failures, seconds.
pub const REROUTE_MAX_GAP_S: f64 = 120.0;

/// Above this speed the announcements come earlier, metres per second (80 km/h).
pub const FAST_MPS: f64 = 22.0;

/// One position fix.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Fix {
    pub at: (f64, f64),
    /// When it was taken, seconds on the caller's clock.
    pub time_s: f64,
    /// The fix's uncertainty, metres.
    pub accuracy_m: Option<f64>,
    /// The direction of travel the receiver reports, degrees clockwise from north.
    pub heading_deg: Option<f64>,
    /// The speed the receiver reports, metres per second.
    pub speed_mps: Option<f64>,
}

/// A route's line, measured: the cumulative distance at every point.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RouteLine {
    pub points: Vec<(f64, f64)>,
    /// The distance from the first point to each point, metres.
    pub along_m: Vec<f64>,
}

/// Where a place lies relative to a route line.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Snap {
    /// The nearest place on the line.
    pub point: (f64, f64),
    /// The segment it is on (from point `segment` to `segment + 1`).
    pub segment: usize,
    /// How far along the line it is, metres.
    pub along_m: f64,
    /// How far the place is from the line, metres.
    pub off_m: f64,
    /// The direction of the segment, degrees clockwise from north.
    pub bearing_deg: f64,
}

impl RouteLine {
    /// The line through `points`.
    #[must_use]
    pub fn new(points: Vec<(f64, f64)>) -> Self {
        let mut along_m = Vec::with_capacity(points.len());
        let mut total = 0.0;
        for (i, &p) in points.iter().enumerate() {
            if i > 0 {
                total += distance_m(points[i - 1], p);
            }
            along_m.push(total);
        }
        Self { points, along_m }
    }

    /// The line's length, metres.
    #[must_use]
    pub fn length_m(&self) -> f64 {
        self.along_m.last().copied().unwrap_or(0.0)
    }

    /// The nearest place on the line to `at`. Of places about as near, the one closest to
    /// `hint_m` along the line wins: a route that passes the same street twice is followed
    /// where the car is on it, not where it will be later.
    #[must_use]
    pub fn snap(&self, at: (f64, f64), hint_m: f64) -> Option<Snap> {
        /// Metres a candidate may lie from the hint before it pays for it.
        const HINT_SLACK_M: f64 = 200.0;
        /// What a metre further from the hint costs, in metres off the line.
        const HINT_WEIGHT: f64 = 0.05;
        let first = *self.points.first()?;
        if self.points.len() == 1 {
            return Some(Snap {
                point: first,
                segment: 0,
                along_m: 0.0,
                off_m: distance_m(at, first),
                bearing_deg: 0.0,
            });
        }
        // A flat frame in metres around `at`: exact enough for the length of a road segment.
        let metres_per_deg = 6_371_000.0_f64.to_radians();
        let cos_lat = at.0.to_radians().cos().max(1e-9);
        let local = |p: (f64, f64)| ((p.1 - at.1) * metres_per_deg * cos_lat, (p.0 - at.0) * metres_per_deg);
        let mut best: Option<(f64, Snap)> = None;
        for i in 0..self.points.len() - 1 {
            let (a, b) = (local(self.points[i]), local(self.points[i + 1]));
            let d = (b.0 - a.0, b.1 - a.1);
            let len2 = d.0 * d.0 + d.1 * d.1;
            let t = if len2 > 0.0 {
                (-(a.0 * d.0 + a.1 * d.1) / len2).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let q = (a.0 + t * d.0, a.1 + t * d.1);
            let off_m = (q.0 * q.0 + q.1 * q.1).sqrt();
            let along = self.along_m[i] + t * (self.along_m[i + 1] - self.along_m[i]);
            let score = off_m + HINT_WEIGHT * ((along - hint_m).abs() - HINT_SLACK_M).max(0.0);
            if best.as_ref().map_or(true, |(s, _)| score < *s) {
                let point = (at.0 + q.1 / metres_per_deg, at.1 + q.0 / (metres_per_deg * cos_lat));
                best = Some((
                    score,
                    Snap {
                        point,
                        segment: i,
                        along_m: along,
                        off_m,
                        bearing_deg: bearing_deg(self.points[i], self.points[i + 1]),
                    },
                ));
            }
        }
        best.map(|(_, snap)| snap)
    }

    /// The place `along_m` metres along the line and the direction there.
    #[must_use]
    pub fn point_at(&self, along_m: f64) -> ((f64, f64), f64) {
        match self.points.len() {
            0 => return ((0.0, 0.0), 0.0),
            1 => return (self.points[0], 0.0),
            _ => {}
        }
        let along = along_m.clamp(0.0, self.length_m());
        let n = self.points.len();
        let i = self
            .along_m
            .partition_point(|&a| a <= along)
            .saturating_sub(1)
            .min(n - 2);
        let (a, b) = (self.points[i], self.points[i + 1]);
        let len = self.along_m[i + 1] - self.along_m[i];
        let t = if len > 0.0 { (along - self.along_m[i]) / len } else { 0.0 };
        ((a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t), bearing_deg(a, b))
    }
}

/// When to say a maneuver.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Stage {
    /// Early: 500 m before in town, 2 km on a fast road.
    Prepare,
    /// Close: 200 m before, 500 m on a fast road.
    Approach,
    /// At the maneuver.
    Now,
}

/// Something to say: the maneuver of step `step`, how far it is.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Announcement {
    pub step: usize,
    pub stage: Stage,
    /// How far the maneuver is when it is said, metres.
    pub distance_m: f64,
}

/// What the guidance makes of a fix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavState {
    /// On the route.
    OnRoute,
    /// Off the route for this long, seconds: a new route may be due ([`Rerouter::due`]).
    OffRoute,
    /// At the destination.
    Arrived,
}

/// Where the car is on its way.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Progress {
    pub state: NavState,
    /// Where the car is drawn: on the route while it is on it, else where the fix is.
    pub shown_at: (f64, f64),
    /// The direction of travel, smoothed, degrees clockwise from north (the map turns by it in
    /// heading-up).
    pub heading_deg: f64,
    /// The speed, smoothed, metres per second.
    pub speed_mps: f64,
    /// How far along the route, metres.
    pub along_m: f64,
    /// How far is left to the destination, metres.
    pub remaining_m: f64,
    /// How long is left at the route's own pace, seconds.
    pub remaining_s: f64,
    /// The next maneuver (an index into the steps), if there is one.
    pub next_step: Option<usize>,
    /// How far it is, metres.
    pub to_next_m: f64,
    /// How far the fix is from the route, metres.
    pub off_route_m: f64,
    /// How long the car has been off the route, seconds (0 while on it).
    pub off_route_for_s: f64,
    /// What to say now, if anything.
    pub announce: Option<Announcement>,
}

/// An angle and a speed, smoothed over the last fixes.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
struct Smoothed {
    /// The heading as a vector: averaging vectors, not degrees, keeps 350 and 10 at 0.
    east: f64,
    north: f64,
    speed_mps: f64,
    known: bool,
}

/// The guidance along one route.
#[derive(Debug, Clone)]
pub struct Guidance {
    line: RouteLine,
    steps: Vec<Step>,
    /// How far along the route each step starts, metres.
    step_at_m: Vec<f64>,
    /// The route's travel time, seconds: the pace of the time left.
    duration_s: f64,
    /// How far along the car was at the last fix.
    along_m: f64,
    /// Since when the car is off the route (the fix's time).
    off_since_s: Option<f64>,
    smoothed: Smoothed,
    last_fix: Option<Fix>,
    /// The stages said per step.
    said: Vec<Option<Stage>>,
    arrived: bool,
}

impl Guidance {
    /// The guidance along `points` with its `steps` (possibly none: the route of the routing
    /// tiles has none), which takes `duration_s` at its own pace.
    #[must_use]
    pub fn new(points: Vec<(f64, f64)>, steps: Vec<Step>, duration_s: f64) -> Self {
        let line = RouteLine::new(points);
        let last = line.along_m.len().saturating_sub(1);
        let step_at_m = steps
            .iter()
            .map(|step| line.along_m.get(step.first_point.min(last)).copied().unwrap_or(0.0))
            .collect();
        Self {
            step_at_m,
            said: vec![None; steps.len()],
            line,
            steps,
            duration_s,
            along_m: 0.0,
            off_since_s: None,
            smoothed: Smoothed::default(),
            last_fix: None,
            arrived: false,
        }
    }

    /// The route's line.
    #[must_use]
    pub fn line(&self) -> &RouteLine {
        &self.line
    }

    /// The route's steps.
    #[must_use]
    pub fn steps(&self) -> &[Step] {
        &self.steps
    }

    /// The destination.
    #[must_use]
    pub fn destination(&self) -> Option<(f64, f64)> {
        self.line.points.last().copied()
    }

    /// Takes the next fix.
    pub fn update(&mut self, fix: &Fix) -> Progress {
        let total = self.line.length_m();
        let threshold = OFF_ROUTE_M.max(fix.accuracy_m.filter(|a| a.is_finite()).unwrap_or(0.0));
        let snap = self.line.snap(fix.at, self.along_m);
        let on = snap.is_some_and(|s| s.off_m <= threshold);
        let moved = self.last_fix.map(|last| (distance_m(last.at, fix.at), fix.time_s - last.time_s));

        // The speed: the receiver's, else from the way since the last fix.
        let speed = fix
            .speed_mps
            .filter(|v| v.is_finite() && *v >= 0.0)
            .or_else(|| moved.filter(|(_, dt)| *dt > 0.0).map(|(d, dt)| d / dt));
        if let Some(v) = speed {
            self.smoothed.speed_mps = if self.smoothed.known {
                0.6 * self.smoothed.speed_mps + 0.4 * v
            } else {
                v
            };
        }

        // The heading: the road's while on it, else the receiver's (when moving), else the way
        // since the last fix.
        let raw = fix
            .heading_deg
            .filter(|h| h.is_finite() && speed.map_or(true, |v| v > 1.0))
            .or_else(|| {
                let last = self.last_fix?;
                (moved?.0 > 3.0).then(|| bearing_deg(last.at, fix.at))
            });
        let sample = match snap {
            Some(s) if on => Some(s.bearing_deg),
            _ => raw,
        };
        if let Some(h) = sample {
            let (east, north) = (h.to_radians().sin(), h.to_radians().cos());
            if self.smoothed.known {
                self.smoothed.east = 0.6 * self.smoothed.east + 0.4 * east;
                self.smoothed.north = 0.6 * self.smoothed.north + 0.4 * north;
            } else {
                self.smoothed.east = east;
                self.smoothed.north = north;
            }
            self.smoothed.known = true;
        }
        let heading_deg = (self.smoothed.east.atan2(self.smoothed.north).to_degrees() + 360.0) % 360.0;

        if let (true, Some(s)) = (on, snap) {
            self.along_m = s.along_m;
            self.off_since_s = None;
        } else if self.off_since_s.is_none() {
            self.off_since_s = Some(fix.time_s);
        }
        let off_route_for_s = self.off_since_s.map_or(0.0, |since| (fix.time_s - since).max(0.0));
        let remaining_m = (total - self.along_m).max(0.0);
        let near_end = self.destination().is_some_and(|end| distance_m(fix.at, end) <= ARRIVAL_M);
        if (on && remaining_m <= ARRIVAL_M) || near_end {
            self.arrived = true;
        }
        let state = if self.arrived {
            NavState::Arrived
        } else if !on && off_route_for_s >= OFF_ROUTE_FOR_S {
            NavState::OffRoute
        } else {
            NavState::OnRoute
        };

        // The step the car is on is the last one that starts behind it; the next one is next.
        let current = self.step_at_m.iter().rposition(|&at| at <= self.along_m + 1e-6);
        let next_step = match current {
            Some(i) if i + 1 < self.steps.len() => Some(i + 1),
            Some(_) => None,
            None if self.steps.is_empty() => None,
            None => Some(0),
        };
        let to_next_m = next_step.map_or(remaining_m, |n| (self.step_at_m[n] - self.along_m).max(0.0));

        let announce = if state == NavState::OnRoute && on {
            self.announcement(next_step, to_next_m)
        } else {
            None
        };
        self.last_fix = Some(*fix);
        Progress {
            state,
            shown_at: match snap {
                Some(s) if on => s.point,
                _ => fix.at,
            },
            heading_deg,
            speed_mps: self.smoothed.speed_mps,
            along_m: self.along_m,
            remaining_m,
            remaining_s: if total > 0.0 { self.duration_s * remaining_m / total } else { 0.0 },
            next_step,
            to_next_m,
            off_route_m: snap.map_or(0.0, |s| s.off_m),
            off_route_for_s,
            announce,
        }
    }

    /// What to say now: the start once, then each maneuver's stages as the car nears it - only
    /// the latest stage that is due, so a stage already passed is not said late. The
    /// destination is not said ahead: arriving is.
    fn announcement(&mut self, next_step: Option<usize>, to_next_m: f64) -> Option<Announcement> {
        use crate::online::Maneuver;
        if let Some(Step { kind: Maneuver::Start, .. }) = self.steps.first() {
            if self.said[0].is_none() {
                self.said[0] = Some(Stage::Now);
                return Some(Announcement {
                    step: 0,
                    stage: Stage::Now,
                    distance_m: 0.0,
                });
            }
        }
        let n = next_step?;
        if self.steps[n].kind == Maneuver::Destination {
            return None;
        }
        let (prepare, approach, now) = announce_distances(self.smoothed.speed_mps);
        let due = if to_next_m <= now {
            Stage::Now
        } else if to_next_m <= approach {
            Stage::Approach
        } else if to_next_m <= prepare {
            Stage::Prepare
        } else {
            return None;
        };
        if self.said[n].is_some_and(|said| said >= due) {
            return None;
        }
        self.said[n] = Some(due);
        Some(Announcement {
            step: n,
            stage: due,
            distance_m: to_next_m,
        })
    }
}

/// The distances before a maneuver at which it is said, metres, at `speed_mps`: (prepare,
/// approach, now).
#[must_use]
pub fn announce_distances(speed_mps: f64) -> (f64, f64, f64) {
    // "Now" comes about three seconds ahead, never closer than 30 m.
    let now = (3.0 * speed_mps).max(30.0);
    if speed_mps > FAST_MPS {
        (2_000.0, 500.0, now)
    } else {
        (500.0, 200.0, now)
    }
}

/// The spacing of the requests for a new route while the car is off its route: one at once
/// when it has been off long enough, the next [`REROUTE_GAP_S`] later at the earliest, the wait
/// doubling after every failed request up to [`REROUTE_MAX_GAP_S`].
#[derive(Debug, Clone, Default)]
pub struct Rerouter {
    /// When the last request went out (the fixes' clock).
    last_asked_s: Option<f64>,
    /// Failed requests in a row.
    failures: u32,
}

impl Rerouter {
    /// Whether to ask for a new route now, the car having been off the route `off_for_s`.
    #[must_use]
    pub fn due(&self, now_s: f64, off_for_s: f64) -> bool {
        off_for_s >= OFF_ROUTE_FOR_S
            && self.last_asked_s.map_or(true, |at| now_s - at >= self.gap_s())
    }

    /// A request went out.
    pub fn asked(&mut self, now_s: f64) {
        self.last_asked_s = Some(now_s);
    }

    /// The request failed (no network, a busy server).
    pub fn failed(&mut self) {
        self.failures = self.failures.saturating_add(1);
    }

    /// A new route arrived.
    pub fn answered(&mut self) {
        self.failures = 0;
    }

    /// The wait before the next request, seconds.
    #[must_use]
    pub fn gap_s(&self) -> f64 {
        (REROUTE_GAP_S * 2f64.powi(i32::try_from(self.failures.min(16)).unwrap_or(16)))
            .min(REROUTE_MAX_GAP_S)
    }
}

/// `(lat, lon)` moved by `east_m` and `north_m`.
#[must_use]
pub fn offset((lat, lon): (f64, f64), east_m: f64, north_m: f64) -> (f64, f64) {
    const EARTH_RADIUS_M: f64 = 6_371_000.0;
    let dlat = north_m / EARTH_RADIUS_M;
    let dlon = east_m / (EARTH_RADIUS_M * lat.to_radians().cos().max(1e-9));
    (lat + dlat.to_degrees(), lon + dlon.to_degrees())
}

/// The direction from `a` to `b`, degrees clockwise from north.
#[must_use]
pub fn bearing_deg(a: (f64, f64), b: (f64, f64)) -> f64 {
    let (lat1, lat2) = (a.0.to_radians(), b.0.to_radians());
    let dlon = (b.1 - a.1).to_radians();
    let y = dlon.sin() * lat2.cos();
    let x = lat1.cos() * lat2.sin() - lat1.sin() * lat2.cos() * dlon.cos();
    (y.atan2(x).to_degrees() + 360.0) % 360.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::online::Maneuver;

    /// A point in central Berlin.
    const ORIGIN: (f64, f64) = (52.5163, 13.3777);

    /// A route 1 km east, a left turn, 1 km north.
    fn l_route() -> (Vec<(f64, f64)>, Vec<Step>) {
        let mut points = Vec::new();
        for i in 0..=10 {
            points.push(offset(ORIGIN, f64::from(i) * 100.0, 0.0));
        }
        for i in 1..=10 {
            points.push(offset(ORIGIN, 1_000.0, f64::from(i) * 100.0));
        }
        let step = |instruction: &str, kind, first_point, last_point, distance_m| Step {
            instruction: instruction.to_string(),
            distance_m,
            kind,
            first_point,
            last_point,
        };
        let steps = vec![
            step("Drive east on Unter den Linden.", Maneuver::Start, 0, 10, 1_000.0),
            step("Turn left onto Friedrichstraße.", Maneuver::Left, 10, 20, 1_000.0),
            step("You have arrived at your destination.", Maneuver::Destination, 20, 20, 0.0),
        ];
        (points, steps)
    }

    fn fix(at: (f64, f64), time_s: f64, speed_mps: f64) -> Fix {
        Fix {
            at,
            time_s,
            accuracy_m: Some(5.0),
            heading_deg: None,
            speed_mps: Some(speed_mps),
        }
    }

    #[test]
    fn a_route_line_is_measured_and_a_place_beside_it_snaps_onto_it() {
        let (points, _) = l_route();
        let line = RouteLine::new(points);
        assert!((line.length_m() - 2_000.0).abs() < 2.0, "{}", line.length_m());
        let beside = offset(ORIGIN, 250.0, 20.0);
        let snap = line.snap(beside, 0.0).expect("a snap");
        assert!((snap.along_m - 250.0).abs() < 1.0, "{snap:?}");
        assert!((snap.off_m - 20.0).abs() < 0.5, "{snap:?}");
        assert_eq!(snap.segment, 2);
        assert!((snap.bearing_deg - 90.0).abs() < 1.0, "east: {snap:?}");
        let (at, bearing) = line.point_at(1_500.0);
        assert!(distance_m(at, offset(ORIGIN, 1_000.0, 500.0)) < 1.0);
        assert!(bearing.abs() < 1.0 || (bearing - 360.0).abs() < 1.0, "north: {bearing}");
        assert!(RouteLine::new(Vec::new()).snap(ORIGIN, 0.0).is_none());
    }

    #[test]
    fn the_next_maneuver_counts_down_and_advances_after_the_turn() {
        let (points, steps) = l_route();
        let mut guidance = Guidance::new(points, steps, 240.0);
        let p = guidance.update(&fix(offset(ORIGIN, 300.0, 3.0), 0.0, 14.0));
        assert_eq!(p.state, NavState::OnRoute);
        assert_eq!(p.next_step, Some(1), "the left turn is next");
        assert!((p.to_next_m - 700.0).abs() < 2.0, "{p:?}");
        assert!((p.remaining_m - 1_700.0).abs() < 2.0, "{p:?}");
        assert!((p.remaining_s - 204.0).abs() < 2.0, "the route's own pace: {p:?}");
        assert!(distance_m(p.shown_at, offset(ORIGIN, 300.0, 0.0)) < 1.0, "drawn on the road");
        let p = guidance.update(&fix(offset(ORIGIN, 600.0, -2.0), 20.0, 14.0));
        assert!((p.to_next_m - 400.0).abs() < 2.0, "{p:?}");
        let p = guidance.update(&fix(offset(ORIGIN, 1_000.0, 200.0), 50.0, 14.0));
        assert_eq!(p.next_step, Some(2), "past the turn the destination is next");
        assert!((p.to_next_m - 800.0).abs() < 2.0, "{p:?}");
    }

    #[test]
    fn a_maneuver_is_said_early_close_and_at_it_once_each() {
        let (points, steps) = l_route();
        let mut guidance = Guidance::new(points, steps, 240.0);
        let mut said = Vec::new();
        for (i, east) in (0..=1_000).step_by(10).enumerate() {
            let p = guidance.update(&fix(offset(ORIGIN, f64::from(east), 0.0), i as f64, 10.0));
            if let Some(a) = p.announce {
                said.push((a.step, a.stage, a.distance_m.round()));
            }
        }
        let turn: Vec<_> = said.iter().filter(|s| s.0 == 1).map(|s| s.1).collect();
        assert_eq!(turn, [Stage::Prepare, Stage::Approach, Stage::Now], "{said:?}");
        let prepare = said.iter().find(|s| s.0 == 1 && s.1 == Stage::Prepare).expect("said");
        assert!((480.0..=500.0).contains(&prepare.2), "500 m before in town: {said:?}");
        let approach = said.iter().find(|s| s.0 == 1 && s.1 == Stage::Approach).expect("said");
        assert!((180.0..=200.0).contains(&approach.2), "200 m before: {said:?}");
        assert_eq!(said.first().map(|s| (s.0, s.1)), Some((0, Stage::Now)), "the start is said first");
    }

    #[test]
    fn on_a_fast_road_the_maneuver_is_said_earlier() {
        assert_eq!(announce_distances(10.0), (500.0, 200.0, 30.0));
        let (prepare, approach, now) = announce_distances(30.0);
        assert_eq!((prepare, approach), (2_000.0, 500.0));
        assert!(now >= 30.0 && now <= 120.0, "about three seconds ahead: {now}");
    }

    #[test]
    fn a_stage_already_passed_is_not_said_late() {
        let (points, steps) = l_route();
        let mut guidance = Guidance::new(points, steps, 240.0);
        // The first fix is 150 m before the turn: only the close stage is said.
        let p = guidance.update(&fix(offset(ORIGIN, 850.0, 0.0), 0.0, 10.0));
        let first = p.announce.expect("something is said");
        assert_eq!(first.step, 0, "the start first");
        let p = guidance.update(&fix(offset(ORIGIN, 860.0, 0.0), 1.0, 10.0));
        assert_eq!(p.announce.map(|a| (a.step, a.stage)), Some((1, Stage::Approach)));
        let p = guidance.update(&fix(offset(ORIGIN, 870.0, 0.0), 2.0, 10.0));
        assert_eq!(p.announce, None, "no Prepare after Approach");
    }

    #[test]
    fn a_car_off_the_route_for_a_few_seconds_counts_as_off_but_a_jump_does_not() {
        let (points, steps) = l_route();
        let mut guidance = Guidance::new(points, steps, 240.0);
        let _ = guidance.update(&fix(offset(ORIGIN, 100.0, 0.0), 0.0, 10.0));
        // One fix 80 m off, then back: a jump of the receiver.
        let p = guidance.update(&fix(offset(ORIGIN, 110.0, 80.0), 1.0, 10.0));
        assert_eq!(p.state, NavState::OnRoute, "{p:?}");
        assert!(p.off_route_m > 70.0);
        let p = guidance.update(&fix(offset(ORIGIN, 120.0, 0.0), 2.0, 10.0));
        assert_eq!(p.off_route_for_s, 0.0);
        // Then off for good: off once it lasted OFF_ROUTE_FOR_S.
        let mut state = NavState::OnRoute;
        for i in 0..8 {
            let p = guidance.update(&fix(offset(ORIGIN, 130.0, 60.0 + f64::from(i) * 10.0), 3.0 + f64::from(i), 10.0));
            state = p.state;
            if i < 3 {
                assert_eq!(p.state, NavState::OnRoute, "{i}: {p:?}");
            }
            assert!(distance_m(p.shown_at, offset(ORIGIN, 130.0, 60.0 + f64::from(i) * 10.0)) < 1.0 || i < 3);
        }
        assert_eq!(state, NavState::OffRoute);
    }

    #[test]
    fn an_uncertain_fix_needs_to_be_further_off_to_count() {
        let (points, steps) = l_route();
        let mut guidance = Guidance::new(points, steps, 240.0);
        for i in 0..10 {
            let mut f = fix(offset(ORIGIN, 100.0 + f64::from(i) * 10.0, 55.0), f64::from(i), 10.0);
            f.accuracy_m = Some(80.0);
            let p = guidance.update(&f);
            assert_eq!(p.state, NavState::OnRoute, "55 m off with 80 m uncertainty: {p:?}");
        }
    }

    #[test]
    fn the_destination_is_reached() {
        let (points, steps) = l_route();
        let mut guidance = Guidance::new(points, steps, 240.0);
        let _ = guidance.update(&fix(offset(ORIGIN, 1_000.0, 900.0), 0.0, 10.0));
        let p = guidance.update(&fix(offset(ORIGIN, 1_002.0, 985.0), 2.0, 5.0));
        assert_eq!(p.state, NavState::Arrived);
        assert!(p.remaining_m < ARRIVAL_M);
        assert_eq!(guidance.update(&fix(offset(ORIGIN, 1_030.0, 1_000.0), 3.0, 1.0)).state, NavState::Arrived, "arrived stays arrived");
    }

    #[test]
    fn the_heading_is_smoothed_and_follows_the_road() {
        let (points, steps) = l_route();
        let mut guidance = Guidance::new(points, steps, 240.0);
        let mut heading = 0.0;
        for (i, noise) in [8.0, -6.0, 5.0, -9.0, 3.0, -2.0].into_iter().enumerate() {
            let mut f = fix(offset(ORIGIN, 100.0 + i as f64 * 14.0, 0.0), i as f64, 14.0);
            f.heading_deg = Some(90.0 + noise);
            heading = guidance.update(&f).heading_deg;
        }
        assert!((heading - 90.0).abs() < 4.0, "{heading}");
        assert!((bearing_deg(ORIGIN, offset(ORIGIN, 0.0, 100.0))).abs() < 0.5);
        assert!((bearing_deg(ORIGIN, offset(ORIGIN, -100.0, 0.0)) - 270.0).abs() < 0.5);
    }

    #[test]
    fn headings_either_side_of_north_average_to_north() {
        let (points, _) = l_route();
        // Off the road (no road heading to follow): the receiver's heading is smoothed.
        let mut guidance = Guidance::new(points, Vec::new(), 240.0);
        let mut heading = 180.0;
        for (i, h) in [350.0, 10.0, 355.0, 5.0].into_iter().enumerate() {
            let mut f = fix(offset(ORIGIN, -500.0, -500.0 + i as f64 * 10.0), i as f64, 10.0);
            f.heading_deg = Some(h);
            heading = guidance.update(&f).heading_deg;
        }
        assert!(heading < 6.0 || heading > 354.0, "{heading}");
    }

    #[test]
    fn a_new_route_is_asked_for_at_once_then_spaced_and_backed_off() {
        let mut r = Rerouter::default();
        assert!(!r.due(0.0, 1.0), "not off long enough");
        assert!(r.due(0.0, OFF_ROUTE_FOR_S));
        r.asked(0.0);
        assert!(!r.due(5.0, 9.0), "not again within the gap");
        assert!(r.due(REROUTE_GAP_S, 14.0));
        r.asked(REROUTE_GAP_S);
        r.failed();
        assert!(r.gap_s() > REROUTE_GAP_S, "a failure doubles the wait");
        assert!(!r.due(REROUTE_GAP_S * 2.0, 25.0));
        for _ in 0..10 {
            r.failed();
        }
        assert!((r.gap_s() - REROUTE_MAX_GAP_S).abs() < 1e-9);
        r.answered();
        assert!((r.gap_s() - REROUTE_GAP_S).abs() < 1e-9);
    }
}
