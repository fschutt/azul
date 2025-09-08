use crate::{css_properties::*, parser::*};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct LinearGradient {
    pub direction: Direction,
    pub extend_mode: ExtendMode,
    pub stops: NormalizedLinearColorStopVec,
}

impl Default for LinearGradient {
    fn default() -> Self {
        Self {
            direction: Direction::default(),
            extend_mode: ExtendMode::default(),
            stops: Vec::new().into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct ConicGradient {
    pub extend_mode: ExtendMode,             // default = clamp (no-repeat)
    pub center: StyleBackgroundPosition,     // default = center center
    pub angle: AngleValue,                   // default = 0deg
    pub stops: NormalizedRadialColorStopVec, // default = []
}

impl Default for ConicGradient {
    fn default() -> Self {
        Self {
            extend_mode: ExtendMode::default(),
            center: StyleBackgroundPosition {
                horizontal: BackgroundPositionHorizontal::Center,
                vertical: BackgroundPositionVertical::Center,
            },
            angle: AngleValue::default(),
            stops: Vec::new().into(),
        }
    }
}

// normalized linear color stop
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct NormalizedLinearColorStop {
    pub offset: PercentageValue, // 0 to 100% // -- todo: theoretically this should be PixelValue
    pub color: ColorU,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct NormalizedRadialColorStop {
    pub angle: AngleValue, // 0 to 360 degrees
    pub color: ColorU,
}

impl LinearColorStop {
    pub fn get_normalized_linear_stops(
        stops: &[LinearColorStop],
    ) -> Vec<NormalizedLinearColorStop> {
        const MIN_STOP_DEGREE: f32 = 0.0;
        const MAX_STOP_DEGREE: f32 = 100.0;

        if stops.is_empty() {
            return Vec::new();
        }

        let self_stops = stops;

        let mut stops = self_stops
            .iter()
            .map(|s| NormalizedLinearColorStop {
                offset: s
                    .offset
                    .as_ref()
                    .copied()
                    .unwrap_or(PercentageValue::new(MIN_STOP_DEGREE)),
                color: s.color,
            })
            .collect::<Vec<_>>();

        let mut stops_to_distribute = 0;
        let mut last_stop = None;
        let stops_len = stops.len();

        for (stop_id, stop) in self_stops.iter().enumerate() {
            if let Some(s) = stop.offset.into_option() {
                let current_stop_val = s.normalized() * 100.0;
                if stops_to_distribute != 0 {
                    let last_stop_val =
                        stops[(stop_id - stops_to_distribute)].offset.normalized() * 100.0;
                    let value_to_add_per_stop = (current_stop_val.max(last_stop_val)
                        - last_stop_val)
                        / (stops_to_distribute - 1) as f32;
                    for (s_id, s) in stops[(stop_id - stops_to_distribute)..stop_id]
                        .iter_mut()
                        .enumerate()
                    {
                        s.offset = PercentageValue::new(
                            last_stop_val + (s_id as f32 * value_to_add_per_stop),
                        );
                    }
                }
                stops_to_distribute = 0;
                last_stop = Some(s);
            } else {
                stops_to_distribute += 1;
            }
        }

        if stops_to_distribute != 0 {
            let last_stop_val = last_stop
                .unwrap_or(PercentageValue::new(MIN_STOP_DEGREE))
                .normalized()
                * 100.0;
            let value_to_add_per_stop = (MAX_STOP_DEGREE.max(last_stop_val) - last_stop_val)
                / (stops_to_distribute - 1) as f32;
            for (s_id, s) in stops[(stops_len - stops_to_distribute)..]
                .iter_mut()
                .enumerate()
            {
                s.offset =
                    PercentageValue::new(last_stop_val + (s_id as f32 * value_to_add_per_stop));
            }
        }

        stops
    }
}

impl RadialColorStop {
    pub fn get_normalized_radial_stops(
        stops: &[RadialColorStop],
    ) -> Vec<NormalizedRadialColorStop> {
        const MIN_STOP_DEGREE: f32 = 0.0;
        const MAX_STOP_DEGREE: f32 = 360.0;

        if stops.is_empty() {
            return Vec::new();
        }

        let self_stops = stops;

        let mut stops = self_stops
            .iter()
            .map(|s| NormalizedRadialColorStop {
                angle: s
                    .offset
                    .as_ref()
                    .copied()
                    .unwrap_or(AngleValue::deg(MIN_STOP_DEGREE)),
                color: s.color,
            })
            .collect::<Vec<_>>();

        let mut stops_to_distribute = 0;
        let mut last_stop = None;
        let stops_len = stops.len();

        for (stop_id, stop) in self_stops.iter().enumerate() {
            if let Some(s) = stop.offset.into_option() {
                let current_stop_val = s.to_degrees();
                if stops_to_distribute != 0 {
                    let last_stop_val = stops[(stop_id - stops_to_distribute)].angle.to_degrees();
                    let value_to_add_per_stop = (current_stop_val.max(last_stop_val)
                        - last_stop_val)
                        / (stops_to_distribute - 1) as f32;
                    for (s_id, s) in stops[(stop_id - stops_to_distribute)..stop_id]
                        .iter_mut()
                        .enumerate()
                    {
                        s.angle =
                            AngleValue::deg(last_stop_val + (s_id as f32 * value_to_add_per_stop));
                    }
                }
                stops_to_distribute = 0;
                last_stop = Some(s);
            } else {
                stops_to_distribute += 1;
            }
        }

        if stops_to_distribute != 0 {
            let last_stop_val = last_stop
                .unwrap_or(AngleValue::deg(MIN_STOP_DEGREE))
                .to_degrees();
            let value_to_add_per_stop = (MAX_STOP_DEGREE.max(last_stop_val) - last_stop_val)
                / (stops_to_distribute - 1) as f32;
            for (s_id, s) in stops[(stops_len - stops_to_distribute)..]
                .iter_mut()
                .enumerate()
            {
                s.angle = AngleValue::deg(last_stop_val + (s_id as f32 * value_to_add_per_stop));
            }
        }

        stops
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct RadialGradient {
    pub shape: Shape,
    pub size: RadialGradientSize,
    pub position: StyleBackgroundPosition,
    pub extend_mode: ExtendMode,
    pub stops: NormalizedLinearColorStopVec,
}

impl Default for RadialGradient {
    fn default() -> Self {
        Self {
            shape: Shape::default(),
            size: RadialGradientSize::default(),
            position: StyleBackgroundPosition::default(),
            extend_mode: ExtendMode::default(),
            stops: Vec::new().into(),
        }
    }
}
