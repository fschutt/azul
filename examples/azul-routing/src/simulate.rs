//! A simulated drive along a route, for demos and headless tests of the navigation: the
//! position moves along the route at a steady speed and, once, takes a wrong turn - it keeps
//! going straight where the route turns - so the guidance sees the car leave its route and a new
//! route is asked for. Given the new route ([`Drive::follow`]) the drive goes on along it.

use crate::navigation::{bearing_deg, offset, Fix, RouteLine};

/// The drive's wrong turn: where along the first route it happens, metres.
#[derive(Debug, Clone, Copy, PartialEq)]
struct WrongTurn {
    at_m: f64,
}

/// Where the simulated car is.
#[derive(Debug, Clone, PartialEq)]
enum Leg {
    /// On a route: the line, how far along it the car was at `since_s`.
    Route {
        line: RouteLine,
        from_m: f64,
        since_s: f64,
    },
    /// Straight on from `from` in direction `bearing_deg` since `since_s`: the wrong turn.
    Astray {
        from: (f64, f64),
        bearing_deg: f64,
        since_s: f64,
    },
}

/// A simulated drive.
#[derive(Debug, Clone, PartialEq)]
pub struct Drive {
    speed_mps: f64,
    leg: Leg,
    wrong_turn: Option<WrongTurn>,
}

/// The longest the car drives astray before it stops and waits for its new route, metres.
const MAX_ASTRAY_M: f64 = 3_000.0;

impl Drive {
    /// A drive along `points` at `speed_mps`, starting at the time `start_s` (the caller's
    /// clock).
    #[must_use]
    pub fn new(points: Vec<(f64, f64)>, speed_mps: f64, start_s: f64) -> Self {
        Self {
            speed_mps: speed_mps.max(0.1),
            leg: Leg::Route {
                line: RouteLine::new(points),
                from_m: 0.0,
                since_s: start_s,
            },
            wrong_turn: None,
        }
    }

    /// The same drive with one wrong turn: at the first maneuver point after `fraction` of the
    /// route (a point where the route turns by more than 30 degrees), else at `fraction` itself.
    #[must_use]
    pub fn with_wrong_turn(mut self, fraction: f64) -> Self {
        /// How sharply the route must turn at a point for it to be a maneuver, degrees.
        const TURN_DEG: f64 = 30.0;
        let Leg::Route { line, .. } = &self.leg else {
            return self;
        };
        let target = fraction.clamp(0.0, 1.0) * line.length_m();
        let p = &line.points;
        let at_m = (1..p.len().saturating_sub(1))
            .filter(|&k| line.along_m[k] >= target)
            .find(|&k| {
                let turn = (bearing_deg(p[k], p[k + 1]) - bearing_deg(p[k - 1], p[k]) + 540.0) % 360.0 - 180.0;
                turn.abs() > TURN_DEG
            })
            .map_or(target, |k| line.along_m[k]);
        self.wrong_turn = Some(WrongTurn { at_m });
        self
    }

    /// The fix at time `now_s`; `None` once the car is at the end of its route.
    #[must_use]
    pub fn fix(&mut self, now_s: f64) -> Option<Fix> {
        let speed = self.speed_mps;
        let astray_from = match &self.leg {
            Leg::Route { line, from_m, since_s } => {
                let along = from_m + speed * (now_s - since_s).max(0.0);
                match self.wrong_turn {
                    Some(turn) if along >= turn.at_m => {
                        // Straight on in the direction the car came from.
                        let (at, _) = line.point_at(turn.at_m);
                        let (_, bearing) = line.point_at((turn.at_m - 1.0).max(0.0));
                        Some((at, bearing, since_s + (turn.at_m - from_m) / speed))
                    }
                    _ => {
                        if along > line.length_m() + 1e-6 {
                            return None;
                        }
                        let (at, bearing) = line.point_at(along);
                        return Some(fix_at(at, now_s, bearing, speed));
                    }
                }
            }
            Leg::Astray { from, bearing_deg, since_s } => {
                let gone = (speed * (now_s - since_s).max(0.0)).min(MAX_ASTRAY_M);
                let rad = bearing_deg.to_radians();
                let at = offset(*from, gone * rad.sin(), gone * rad.cos());
                let moving = if gone < MAX_ASTRAY_M { speed } else { 0.0 };
                return Some(fix_at(at, now_s, *bearing_deg, moving));
            }
        };
        if let Some((from, bearing, since_s)) = astray_from {
            self.wrong_turn = None;
            self.leg = Leg::Astray {
                from,
                bearing_deg: bearing,
                since_s,
            };
            return self.fix(now_s);
        }
        None
    }

    /// Whether the car drives astray now (it took its wrong turn and has no new route yet).
    #[must_use]
    pub fn astray(&self) -> bool {
        matches!(self.leg, Leg::Astray { .. })
    }

    /// Goes on along a new route from where the car is at `now_s` (the route starts there: it
    /// was asked for from there).
    pub fn follow(&mut self, points: Vec<(f64, f64)>, now_s: f64) {
        // The wrong turn belongs to the first route.
        self.wrong_turn = None;
        self.leg = Leg::Route {
            line: RouteLine::new(points),
            from_m: 0.0,
            since_s: now_s,
        };
    }
}

/// A simulated fix: a good receiver's, five metres uncertain.
fn fix_at(at: (f64, f64), time_s: f64, heading_deg: f64, speed_mps: f64) -> Fix {
    Fix {
        at,
        time_s,
        accuracy_m: Some(5.0),
        heading_deg: Some(heading_deg),
        speed_mps: Some(speed_mps),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{distance_m, navigation::Guidance, navigation::NavState};

    const ORIGIN: (f64, f64) = (52.5163, 13.3777);

    fn l_route() -> Vec<(f64, f64)> {
        let mut points = Vec::new();
        for i in 0..=10 {
            points.push(offset(ORIGIN, f64::from(i) * 100.0, 0.0));
        }
        for i in 1..=10 {
            points.push(offset(ORIGIN, 1_000.0, f64::from(i) * 100.0));
        }
        points
    }

    #[test]
    fn the_car_moves_along_the_route_at_its_speed_and_stops_at_the_end() {
        let mut drive = Drive::new(l_route(), 20.0, 100.0);
        let start = drive.fix(100.0).expect("a fix");
        assert!(distance_m(start.at, ORIGIN) < 1.0);
        let later = drive.fix(110.0).expect("a fix");
        assert!(distance_m(later.at, offset(ORIGIN, 200.0, 0.0)) < 1.0, "200 m in 10 s");
        assert!((later.heading_deg.expect("a heading") - 90.0).abs() < 1.0);
        assert_eq!(later.speed_mps, Some(20.0));
        let round = drive.fix(160.0).expect("a fix");
        assert!(distance_m(round.at, offset(ORIGIN, 1_000.0, 200.0)) < 1.0, "round the corner");
        assert!(drive.fix(200.1).is_none(), "2 km at 20 m/s ends after 100 s");
    }

    #[test]
    fn the_wrong_turn_goes_straight_on_where_the_route_turns() {
        let mut drive = Drive::new(l_route(), 20.0, 0.0).with_wrong_turn(0.3);
        let at_turn = drive.fix(50.0).expect("a fix");
        assert!(distance_m(at_turn.at, offset(ORIGIN, 1_000.0, 0.0)) < 1.0);
        let astray = drive.fix(55.0).expect("a fix");
        assert!(drive.astray());
        assert!(distance_m(astray.at, offset(ORIGIN, 1_100.0, 0.0)) < 1.0, "on east, not north");
    }

    #[test]
    fn the_guidance_sees_the_wrong_turn_and_the_drive_follows_the_new_route() {
        let points = l_route();
        let mut guidance = Guidance::new(points.clone(), Vec::new(), 100.0);
        let mut drive = Drive::new(points, 20.0, 0.0).with_wrong_turn(0.3);
        let mut off_at = None;
        let mut t = 0.0;
        while t < 80.0 {
            let fix = drive.fix(t).expect("a fix");
            if guidance.update(&fix).state == NavState::OffRoute {
                off_at = Some((t, fix.at));
                break;
            }
            t += 1.0;
        }
        let (t, here) = off_at.expect("the car left its route");
        assert!(t > 50.0 && t < 60.0, "a few seconds after the turn: {t}");
        // The new route from where the car is back to the destination.
        let back = vec![here, offset(ORIGIN, 1_000.0, 0.0), offset(ORIGIN, 1_000.0, 1_000.0)];
        drive.follow(back.clone(), t);
        assert!(!drive.astray());
        let next = drive.fix(t + 2.0).expect("a fix");
        assert!(distance_m(next.at, here) > 30.0 && distance_m(next.at, here) < 50.0);
        let mut guidance = Guidance::new(back, Vec::new(), 100.0);
        assert_eq!(guidance.update(&next).state, NavState::OnRoute);
    }

    #[test]
    fn a_drive_without_a_wrong_turn_never_leaves_its_route() {
        let points = l_route();
        let mut guidance = Guidance::new(points.clone(), Vec::new(), 100.0);
        let mut drive = Drive::new(points, 25.0, 0.0);
        let mut t = 0.0;
        while let Some(fix) = drive.fix(t) {
            assert_ne!(guidance.update(&fix).state, NavState::OffRoute, "{t}");
            t += 0.5;
        }
        assert!(t > 75.0);
    }
}
