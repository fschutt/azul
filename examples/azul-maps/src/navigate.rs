//! A navigation under way: the guidance along the road route (azul-routing's
//! [`Guidance`]), fed with the fixes of the position source - or of a simulated drive
//! (`--simulate-route`) - and turned into what the app does: what to say and when, when to ask
//! for a new route (spaced by the [`Rerouter`]), the camera that follows the car, arrival.
//!
//! No azul types and no window: the app's timer feeds it, the window shows what it says.

use std::time::Instant;

use azul_appkit::l10n::{t_args, Arg};
use azul_routing::{
    navigation::{Announcement, Fix, Guidance, NavState, Progress, Rerouter, Stage},
    online::Step,
    simulate::Drive,
};

use crate::model::{distance_text, TravelMode};

/// Where along the simulated drive the wrong turn is taken: the first maneuver after a third of
/// the route.
pub const WRONG_TURN_AT: f64 = 1.0 / 3.0;

/// What the app does after a fix.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// Say `text`; `interrupt` cuts off what is being said.
    Say { text: String, interrupt: bool },
    /// Ask for a new route from `from` to the destination.
    Reroute { from: (f64, f64) },
    /// The destination is reached.
    Arrived,
}

/// The camera that follows the car.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Camera {
    pub at: (f64, f64),
    pub zoom: f32,
    /// The map's bearing: the heading in heading-up, north (0) otherwise.
    pub bearing_deg: f32,
}

/// A navigation under way.
pub struct Navigation {
    pub guidance: Guidance,
    pub rerouter: Rerouter,
    pub destination: (f64, f64),
    pub mode: TravelMode,
    /// The map turns with the car (else north stays up).
    pub heading_up: bool,
    /// The big-type car layout.
    pub car_mode: bool,
    /// Whether the prompts are said aloud.
    pub voice: bool,
    /// The simulated drive, when one drives the navigation (`--simulate-route`).
    pub drive: Option<Drive>,
    /// The last fix's progress.
    pub progress: Option<Progress>,
    /// A new route is being asked for.
    pub rerouting: bool,
    /// How many new routes arrived.
    pub reroutes: u32,
    /// The route came from the cache with the server out of reach.
    pub offline: bool,
    /// When the navigation started: the clock of the fixes and of the drive.
    started: Instant,
    arrived: bool,
}

impl Navigation {
    /// A navigation along `points` with its `steps`, `duration_s` at the route's pace, to
    /// `destination`; with `simulate_mps`, a simulated car drives it.
    #[must_use]
    pub fn new(
        points: Vec<(f64, f64)>,
        steps: Vec<Step>,
        duration_s: f64,
        destination: (f64, f64),
        mode: TravelMode,
        simulate_mps: Option<f64>,
    ) -> Self {
        // The simulated car starts where the route does, at the navigation's start.
        let drive = simulate_mps.filter(|v| *v > 0.0).map(|speed| {
            Drive::new(points.clone(), speed, 0.0).with_wrong_turn(WRONG_TURN_AT)
        });
        Self {
            guidance: Guidance::new(points, steps, duration_s),
            rerouter: Rerouter::default(),
            destination,
            mode,
            heading_up: true,
            car_mode: mode == TravelMode::Car,
            voice: true,
            drive,
            progress: None,
            rerouting: false,
            reroutes: 0,
            offline: false,
            started: Instant::now(),
            arrived: false,
        }
    }

    /// Seconds since the navigation started.
    #[must_use]
    pub fn clock(&self) -> f64 {
        self.started.elapsed().as_secs_f64()
    }

    /// The simulated car's fix at `now_s`, when a simulated drive drives. Past the end of its
    /// route the car stands at the destination: a fast car whose last fix fell short of it
    /// still arrives.
    pub fn simulated_fix(&mut self, now_s: f64) -> Option<Fix> {
        let moving = self.drive.as_mut()?.fix(now_s);
        moving.or_else(|| {
            self.guidance.destination().map(|at| Fix {
                at,
                time_s: now_s,
                accuracy_m: Some(5.0),
                heading_deg: None,
                speed_mps: Some(0.0),
            })
        })
    }

    /// Takes the fix of `now_s` and says what to do.
    pub fn update(&mut self, fix: &Fix, now_s: f64) -> Vec<Event> {
        let mut events = Vec::new();
        if self.arrived {
            return events;
        }
        let progress = self.guidance.update(fix);
        self.progress = Some(progress);
        if progress.state == NavState::Arrived {
            self.arrived = true;
            events.push(Event::Say {
                text: t_args("azmaps-voice-arrived", &[]),
                interrupt: true,
            });
            events.push(Event::Arrived);
            return events;
        }
        if let Some(announcement) = progress.announce {
            if let Some(step) = self.guidance.steps().get(announcement.step) {
                events.push(Event::Say {
                    text: announcement_text(step, &announcement),
                    // The prompt at the maneuver must not wait behind an earlier one.
                    interrupt: announcement.stage == Stage::Now,
                });
            }
        }
        if progress.state == NavState::OffRoute
            && !self.rerouting
            && self.rerouter.due(now_s, progress.off_route_for_s)
        {
            self.rerouting = true;
            self.rerouter.asked(now_s);
            // Said once per stretch off the route: a repeat after a failed request stays quiet.
            if self.rerouter.gap_s() <= azul_routing::navigation::REROUTE_GAP_S {
                events.push(Event::Say {
                    text: t_args("azmaps-voice-rerouting", &[]),
                    interrupt: true,
                });
            }
            events.push(Event::Reroute { from: fix.at });
        }
        events
    }

    /// A new route arrived (from where the car was): the guidance follows it, the drive too.
    pub fn rerouted(&mut self, points: Vec<(f64, f64)>, steps: Vec<Step>, duration_s: f64, now_s: f64, offline: bool) {
        if let Some(drive) = self.drive.as_mut() {
            drive.follow(points.clone(), now_s);
        }
        self.guidance = Guidance::new(points, steps, duration_s);
        self.rerouter.answered();
        self.rerouting = false;
        self.reroutes += 1;
        self.offline = offline;
    }

    /// The request for a new route failed: the next waits longer.
    pub fn reroute_failed(&mut self) {
        self.rerouter.failed();
        self.rerouting = false;
    }

    /// Where the camera looks after the last fix.
    #[must_use]
    pub fn camera(&self) -> Option<Camera> {
        let progress = self.progress?;
        #[allow(clippy::cast_possible_truncation)] // a heading of 0..360
        let bearing_deg = if self.heading_up { progress.heading_deg as f32 } else { 0.0 };
        Some(Camera {
            at: progress.shown_at,
            zoom: follow_zoom(progress.speed_mps),
            bearing_deg,
        })
    }

    /// Whether the destination is reached.
    #[must_use]
    pub fn arrived(&self) -> bool {
        self.arrived
    }
}

/// What is said for a maneuver: its instruction, `In 500 metres: ` before it while it is ahead.
#[must_use]
pub fn announcement_text(step: &Step, announcement: &Announcement) -> String {
    let instruction = step.instruction.trim().to_string();
    if announcement.stage == Stage::Now {
        return instruction;
    }
    let (n, km) = spoken_distance(announcement.distance_m);
    let n = if km { azul_appkit::l10n::decimal(&n) } else { n };
    t_args(
        if km { "azmaps-voice-in-km" } else { "azmaps-voice-in-m" },
        &[("n", Arg::from(n)), ("instruction", Arg::from(instruction))],
    )
}

/// A distance as it is said: to 10 m below 100 m, to 50 m below 1 km, to half a kilometre
/// above; `(number, in kilometres)`.
#[must_use]
pub fn spoken_distance(metres: f64) -> (String, bool) {
    let metres = metres.max(0.0);
    let rounded = if metres < 100.0 {
        (metres / 10.0).round() * 10.0
    } else {
        (metres / 50.0).round() * 50.0
    };
    if rounded < 1_000.0 {
        return (format!("{rounded:.0}"), false);
    }
    let km = (metres / 500.0).round() / 2.0;
    if km.fract() == 0.0 {
        (format!("{km:.0}"), true)
    } else {
        (format!("{km:.1}"), true)
    }
}

/// A distance as the banner shows it: `350 m` (to 10 m), `1.2 km`.
#[must_use]
pub fn banner_distance(metres: f64) -> String {
    let rounded = (metres.max(0.0) / 10.0).round() * 10.0;
    distance_text(rounded / 1000.0)
}

/// The zoom the camera follows the car at, by its speed: closer in town, further out on a
/// motorway.
#[must_use]
pub fn follow_zoom(speed_mps: f64) -> f32 {
    if speed_mps <= 8.0 {
        17.0
    } else if speed_mps <= 20.0 {
        16.0
    } else if speed_mps <= 30.0 {
        15.0
    } else {
        14.0
    }
}

/// Whether the hour (0 to 23, local) is night for the car's colours.
#[must_use]
pub const fn is_night(hour: u32) -> bool {
    hour < 7 || hour >= 19
}

#[cfg(test)]
mod tests {
    use azul_routing::{navigation::offset, online::Maneuver};

    use super::*;

    const ORIGIN: (f64, f64) = (52.5163, 13.3777);

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

    fn said(events: &[Event]) -> Vec<String> {
        events
            .iter()
            .filter_map(|e| match e {
                Event::Say { text, .. } => Some(text.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn distances_are_said_and_shown_rounded() {
        assert_eq!(spoken_distance(487.0), ("500".to_string(), false));
        assert_eq!(spoken_distance(212.0), ("200".to_string(), false));
        assert_eq!(spoken_distance(84.0), ("80".to_string(), false));
        assert_eq!(spoken_distance(1_740.0), ("1.5".to_string(), true));
        assert_eq!(spoken_distance(2_000.0), ("2".to_string(), true));
        crate::l10n::in_english();
        assert_eq!(banner_distance(347.0), "350 m");
        assert_eq!(banner_distance(1_240.0), "1.2 km");
    }

    #[test]
    fn a_maneuver_is_announced_with_its_distance_then_at_it() {
        crate::l10n::in_english();
        let (_, steps) = l_route();
        let prepare = Announcement {
            step: 1,
            stage: Stage::Prepare,
            distance_m: 495.0,
        };
        assert_eq!(announcement_text(&steps[1], &prepare), "In 500 metres: Turn left onto Friedrichstraße.");
        let far = Announcement {
            step: 1,
            stage: Stage::Prepare,
            distance_m: 1_980.0,
        };
        assert_eq!(announcement_text(&steps[1], &far), "In 2 kilometres: Turn left onto Friedrichstraße.");
        let now = Announcement {
            step: 1,
            stage: Stage::Now,
            distance_m: 25.0,
        };
        assert_eq!(announcement_text(&steps[1], &now), "Turn left onto Friedrichstraße.");
    }

    #[test]
    fn a_simulated_drive_is_guided_rerouted_once_and_arrives() {
        crate::l10n::in_english();
        let (points, steps) = l_route();
        let destination = *points.last().expect("a point");
        let mut nav = Navigation::new(points, steps, 120.0, destination, TravelMode::Car, Some(20.0));
        let mut events = Vec::new();
        let mut t = 0.0;
        let mut rerouted = false;
        while t < 300.0 && !events.contains(&Event::Arrived) {
            if let Some(fix) = nav.simulated_fix(t) {
                let now = nav.update(&fix, t);
                for event in &now {
                    if let Event::Reroute { from } = event {
                        assert!(!rerouted, "one wrong turn, one new route");
                        rerouted = true;
                        // The routing server's answer: back to the corner, then north.
                        let back = vec![*from, offset(ORIGIN, 1_000.0, 0.0), destination];
                        nav.rerouted(back, Vec::new(), 60.0, t, false);
                    }
                }
                events.extend(now);
            }
            t += 1.0;
        }
        let words = said(&events);
        assert_eq!(words.first().map(String::as_str), Some("Drive east on Unter den Linden."), "{words:?}");
        assert!(words.iter().any(|w| w.starts_with("In 500 metres: Turn left")), "{words:?}");
        assert!(words.iter().any(|w| w == "Recalculating the route."), "{words:?}");
        assert!(rerouted);
        assert_eq!(nav.reroutes, 1);
        assert!(events.contains(&Event::Arrived), "{events:?}");
        assert_eq!(words.last().map(String::as_str), Some("You have arrived."));
        assert_eq!(events.iter().filter(|e| **e == Event::Arrived).count(), 1);
    }

    #[test]
    fn a_failed_new_route_is_asked_again_later_not_at_once() {
        let (points, steps) = l_route();
        let destination = *points.last().expect("a point");
        let mut nav = Navigation::new(points, steps, 120.0, destination, TravelMode::Car, None);
        let off = |t: f64| Fix {
            at: offset(ORIGIN, 300.0, 300.0 + t),
            time_s: t,
            accuracy_m: Some(5.0),
            heading_deg: Some(0.0),
            speed_mps: Some(10.0),
        };
        let mut asked = Vec::new();
        for t in 0..40 {
            let t = f64::from(t);
            for event in nav.update(&off(t), t) {
                if let Event::Reroute { .. } = event {
                    asked.push(t);
                    nav.reroute_failed();
                }
            }
        }
        assert!(asked.len() >= 2, "{asked:?}");
        assert!(asked[1] - asked[0] >= 19.0, "the second waits twice the gap: {asked:?}");
    }

    #[test]
    fn the_camera_follows_the_car_closer_in_town() {
        assert!(follow_zoom(8.0) > follow_zoom(30.0));
        assert_eq!(follow_zoom(5.0), 17.0);
        assert_eq!(follow_zoom(36.0), 14.0);
        let (points, steps) = l_route();
        let destination = *points.last().expect("a point");
        let mut nav = Navigation::new(points, steps, 120.0, destination, TravelMode::Car, None);
        assert_eq!(nav.camera(), None, "no fix yet");
        let fix = Fix {
            at: offset(ORIGIN, 1_000.0, 300.0),
            time_s: 0.0,
            accuracy_m: Some(5.0),
            heading_deg: Some(0.0),
            speed_mps: Some(10.0),
        };
        let _ = nav.update(&fix, 0.0);
        let camera = nav.camera().expect("a camera");
        assert!(azul_routing::distance_m(camera.at, offset(ORIGIN, 1_000.0, 300.0)) < 1.0);
        assert!(camera.bearing_deg.abs() < 1.0 || (camera.bearing_deg - 360.0).abs() < 1.0, "heading north: {camera:?}");
        nav.heading_up = false;
        assert_eq!(nav.camera().expect("a camera").bearing_deg, 0.0);
        assert!(is_night(22) && is_night(5) && !is_night(12) && !is_night(7));
    }
}
