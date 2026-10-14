//! How each travel mode uses the roads: which ways it may take, in which
//! direction, and how fast.

use crate::format::{Highway, Oneway, Way};

/// How the traveller goes. (Public transport is a timetable question, not a
/// road graph: the app asks timetable providers for it.)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Mode {
    Car,
    Bike,
    Walk,
}

impl Mode {
    /// Whether the mode may use `way` at all.
    #[must_use]
    pub fn allows(self, way: &Way) -> bool {
        use Highway::{
            Bridleway, Cycleway, Footway, Motorway, MotorwayLink, Path, Pedestrian, Steps, Track,
            Trunk, TrunkLink,
        };
        let h = way.highway;
        match self {
            Self::Car => {
                way.access.motor.unwrap_or(true)
                    && !matches!(
                        h,
                        Track | Path | Footway | Cycleway | Bridleway | Pedestrian | Steps
                    )
            }
            Self::Bike => match way.access.bicycle {
                Some(may) => may,
                None => !matches!(h, Motorway | MotorwayLink | Trunk | TrunkLink | Steps | Footway | Pedestrian | Bridleway),
            },
            Self::Walk => match way.access.foot {
                Some(may) => may,
                None => !matches!(h, Motorway | MotorwayLink | Trunk | TrunkLink),
            },
        }
    }

    /// Whether the mode may go along `way` in the order of its nodes
    /// (`forward`) or against it.
    #[must_use]
    pub fn may_go(self, way: &Way, forward: bool) -> bool {
        let oneway = match self {
            Self::Walk => Oneway::No,
            Self::Bike if way.bicycle_both_ways => Oneway::No,
            _ => way.oneway,
        };
        match oneway {
            Oneway::No => true,
            Oneway::Forward => forward,
            Oneway::Backward => !forward,
        }
    }

    /// The mode's speed on `way`, km/h: the speed limit (a little under it)
    /// where the way has one and a car takes it, else a usual speed for the
    /// road's kind. Never above [`Self::top_speed_kmh`].
    #[must_use]
    pub fn speed_kmh(self, way: &Way) -> f32 {
        use Highway::*;
        let speed = match self {
            Self::Car => {
                let usual = match way.highway {
                    Motorway => 115.0,
                    MotorwayLink => 60.0,
                    Trunk => 90.0,
                    TrunkLink => 50.0,
                    Primary => 70.0,
                    PrimaryLink => 45.0,
                    Secondary => 60.0,
                    SecondaryLink => 40.0,
                    Tertiary => 50.0,
                    TertiaryLink => 35.0,
                    Unclassified => 45.0,
                    Residential | Road => 30.0,
                    LivingStreet => 10.0,
                    Service => 15.0,
                    _ => 10.0,
                };
                way.maxspeed_kmh.map_or(usual, |limit| limit * 0.9)
            }
            Self::Bike => match way.highway {
                Track | Path | Bridleway => 12.0,
                Footway | Pedestrian => 8.0,
                Steps => 2.0,
                _ => 16.0,
            },
            Self::Walk => match way.highway {
                Steps => 2.5,
                _ => 5.0,
            },
        };
        speed.clamp(1.0, self.top_speed_kmh())
    }

    /// The fastest the mode ever goes, km/h: what keeps A*'s estimate of the
    /// time left from ever being too long.
    #[must_use]
    pub const fn top_speed_kmh(self) -> f32 {
        match self {
            Self::Car => 130.0,
            Self::Bike => 16.0,
            Self::Walk => 5.0,
        }
    }
}
