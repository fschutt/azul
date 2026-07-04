#![deny(missing_docs)]

//! `avar` Axis Variations Table
//!
//! The axis variations table (`avar`) is an optional table used in variable
//! fonts. It can be used to modify aspects of how a design varies for different
//! instances along a particular design-variation axis. Specifically, it allows
//! modification of the coordinate normalization that is used when processing
//! variation data for a particular variation instance.
//!
//! <https://learn.microsoft.com/en-us/typography/opentype/spec/avar>

use crate::binary::read::{ReadArray, ReadBinary, ReadCtxt, ReadFrom, ReadScope, ReadUnchecked};
use crate::error::ParseError;
use crate::tables::{F2Dot14, Fixed};

/// `avar` Axis Variations Table.
pub struct AvarTable<'a> {
    /// Major version number of the axis variations table.
    pub major_version: u16,
    /// Minor version number of the axis variations table.
    pub minor_version: u16,
    /// The number of variation axes for this font.
    pub axis_count: u16,
    segments_map_scope: ReadScope<'a>,
}

/// Segment map record.
///
/// Contains an array of mappings from a normalised coordinate value to a
/// modified value.
pub struct SegmentMap<'a> {
    /// The array of axis value map records for this axis.
    axis_value_maps: ReadArray<'a, AxisValueMap>,
}

/// A mapping from a normalised coordinate value to a modified value.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct AxisValueMap {
    /// A normalized coordinate value obtained using default normalization.
    pub from_coordinate: F2Dot14,
    /// The modified, normalized coordinate value.
    pub to_coordinate: F2Dot14,
}

impl AvarTable<'_> {
    /// Iterate over the segment maps.
    ///
    /// To retrieve the segment map for a specific index use [Iterator::nth].
    pub fn segment_maps(&self) -> impl Iterator<Item = SegmentMap<'_>> {
        (0..self.axis_count).scan(self.segments_map_scope.ctxt(), |ctxt, _i| {
            ctxt.read::<SegmentMap<'_>>().ok()
        })
    }
}

impl ReadBinary for AvarTable<'_> {
    type HostType<'a> = AvarTable<'a>;

    fn read<'a>(ctxt: &mut ReadCtxt<'a>) -> Result<Self::HostType<'a>, ParseError> {
        let major_version = ctxt.read_u16be()?;
        ctxt.check_version(major_version == 1)?;
        let minor_version = ctxt.read_u16be()?;
        let _reserved = ctxt.read_u16be()?;
        let axis_count = ctxt.read_u16be()?;

        let segment_map_scope = ctxt.scope();
        let mut segment_maps_len = 0;

        for _ in 0..axis_count {
            let segment_map = ctxt.read::<SegmentMap<'_>>()?;
            // + 2 for the 16-bit position map count
            segment_maps_len += segment_map.axis_value_maps.len() * AxisValueMap::SIZE + 2
        }

        let segments_map_scope = segment_map_scope.offset_length(0, segment_maps_len)?;

        Ok(AvarTable {
            major_version,
            minor_version,
            axis_count,
            segments_map_scope,
        })
    }
}

impl SegmentMap<'_> {
    /// Iterate over the axis value mappings.
    pub fn axis_value_mappings(&self) -> impl Iterator<Item = AxisValueMap> + '_ {
        self.axis_value_maps.iter()
    }

    /// Performs `avar` normalization to a value that has already been default
    /// normalised.
    ///
    /// `normalised_value` should be in the range [-1, +1].
    pub fn normalize(&self, mut normalized_value: Fixed) -> Fixed {
        // Scan the axis value maps for the first record that has a
        // from_coordinate >= default_normalised_value
        let mut start_seg: Option<AxisValueMap> = None;
        for end_seg in self.axis_value_mappings() {
            // From the spec: Note that endSeg cannot be the first map record, which is for
            // -1.
            if let Some(start_seg) = start_seg {
                let end_seg_from_coordinate = Fixed::from(end_seg.from_coordinate);
                if end_seg_from_coordinate == normalized_value {
                    normalized_value = end_seg.to_coordinate.into();
                    break;
                } else if end_seg_from_coordinate > normalized_value {
                    // if start_seg is None then this is the first axis value map record, which
                    // can't be the end seg
                    let ratio = (normalized_value - Fixed::from(start_seg.from_coordinate))
                        / (Fixed::from(end_seg.from_coordinate)
                            - Fixed::from(start_seg.from_coordinate));
                    normalized_value = Fixed::from(start_seg.to_coordinate)
                        + ratio
                            * (Fixed::from(end_seg.to_coordinate)
                                - Fixed::from(start_seg.to_coordinate));
                    break;
                }
            }
            start_seg = Some(end_seg);
        }
        normalized_value
    }
}

impl ReadBinary for SegmentMap<'_> {
    type HostType<'a> = SegmentMap<'a>;

    fn read<'a>(ctxt: &mut ReadCtxt<'a>) -> Result<Self::HostType<'a>, ParseError> {
        let position_map_count = ctxt.read_u16be()?;
        let axis_value_maps = ctxt.read_array::<AxisValueMap>(usize::from(position_map_count))?;

        Ok(SegmentMap { axis_value_maps })
    }
}

impl ReadFrom for AxisValueMap {
    type ReadType = (F2Dot14, F2Dot14);

    fn read_from((from_coordinate, to_coordinate): (F2Dot14, F2Dot14)) -> Self {
        AxisValueMap {
            from_coordinate,
            to_coordinate,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{AvarTable, AxisValueMap, F2Dot14, ReadScope};
    use crate::binary::write::{WriteBinary, WriteBuffer};
    use crate::binary::U16Be;
    use crate::error::ReadWriteError;
    use crate::font_data::FontData;
    use crate::tables::variable_fonts::avar::SegmentMap;
    use crate::tables::{Fixed, FontTableProvider};
    use crate::tag;
    use crate::tests::{assert_fixed_close, read_fixture};

    #[test]
    fn avar() {
        let buffer = read_fixture("tests/fonts/opentype/NotoSans-VF.abc.ttf");
        let scope = ReadScope::new(&buffer);
        let font_file = scope
            .read::<FontData<'_>>()
            .expect("unable to parse font file");
        let table_provider = font_file
            .table_provider(0)
            .expect("unable to create font provider");
        let avar_data = table_provider
            .read_table_data(tag::AVAR)
            .expect("unable to read avar table data");
        let avar = ReadScope::new(&avar_data).read::<AvarTable<'_>>().unwrap();

        let segment_maps = avar
            .segment_maps()
            .map(|segment_map| segment_map.axis_value_mappings().collect::<Vec<_>>())
            .collect::<Vec<_>>();
        let expected = vec![
            vec![
                AxisValueMap {
                    from_coordinate: F2Dot14::from(-1.0),
                    to_coordinate: F2Dot14::from(-1.0),
                },
                AxisValueMap {
                    from_coordinate: F2Dot14::from(-0.6667),
                    to_coordinate: F2Dot14::from(-0.7969),
                },
                AxisValueMap {
                    from_coordinate: F2Dot14::from(-0.3333),
                    to_coordinate: F2Dot14::from(-0.5),
                },
                AxisValueMap {
                    from_coordinate: F2Dot14::from(0.0),
                    to_coordinate: F2Dot14::from(0.0),
                },
                AxisValueMap {
                    from_coordinate: F2Dot14::from(0.2),
                    to_coordinate: F2Dot14::from(0.18),
                },
                AxisValueMap {
                    from_coordinate: F2Dot14::from(0.4),
                    to_coordinate: F2Dot14::from(0.38),
                },
                AxisValueMap {
                    from_coordinate: F2Dot14::from(0.6),
                    to_coordinate: F2Dot14::from(0.61),
                },
                AxisValueMap {
                    from_coordinate: F2Dot14::from(0.8),
                    to_coordinate: F2Dot14::from(0.79),
                },
                AxisValueMap {
                    from_coordinate: F2Dot14::from(1.0),
                    to_coordinate: F2Dot14::from(1.0),
                },
            ],
            vec![
                AxisValueMap {
                    from_coordinate: F2Dot14::from(-1.0),
                    to_coordinate: F2Dot14::from(-1.0),
                },
                AxisValueMap {
                    from_coordinate: F2Dot14::from(-0.6667),
                    to_coordinate: F2Dot14::from(-0.7),
                },
                AxisValueMap {
                    from_coordinate: F2Dot14::from(-0.3333),
                    to_coordinate: F2Dot14::from(-0.36664),
                },
                AxisValueMap {
                    from_coordinate: F2Dot14::from(0.0),
                    to_coordinate: F2Dot14::from(0.0),
                },
                AxisValueMap {
                    from_coordinate: F2Dot14::from(1.0),
                    to_coordinate: F2Dot14::from(1.0),
                },
            ],
            vec![
                AxisValueMap {
                    from_coordinate: F2Dot14::from(-1.0),
                    to_coordinate: F2Dot14::from(-1.0),
                },
                AxisValueMap {
                    from_coordinate: F2Dot14::from(0.0),
                    to_coordinate: F2Dot14::from(0.0),
                },
                AxisValueMap {
                    from_coordinate: F2Dot14::from(1.0),
                    to_coordinate: F2Dot14::from(1.0),
                },
            ],
        ];
        assert_eq!(segment_maps, expected);
    }

    #[test]
    fn test_avar_normalize() -> Result<(), ReadWriteError> {
        // Build test segment. Example data from:
        // https://learn.microsoft.com/en-us/typography/opentype/spec/otvaroverview#avar-normalization-example
        let mut buf = WriteBuffer::new();
        U16Be::write(&mut buf, 6u16)?; // position_map_count
        [
            (-1.0, -1.0),
            (-0.75, -0.5),
            (0., 0.),
            (0.4, 0.4),
            (0.6, 0.9),
            (1.0, 1.0),
        ]
        .iter()
        .copied()
        .try_for_each(|(from_coord, to_coord)| {
            F2Dot14::write(&mut buf, F2Dot14::from(from_coord))?;
            F2Dot14::write(&mut buf, F2Dot14::from(to_coord))
        })?;

        let data = buf.into_inner();
        let mut ctxt = ReadScope::new(&data).ctxt();
        let segment_map = ctxt.read::<SegmentMap<'_>>()?;
        [
            (-1.0, -1.0),
            (-0.75, -0.5),
            (-0.5, -0.3333),
            (-0.25, -0.1667),
            (0., 0.),
            (0.25, 0.25),
            (0.5, 0.65),
            (0.75, 0.9375),
            (1.0, 1.0),
        ]
        .iter()
        .copied()
        .for_each(|(input, expected)| {
            assert_fixed_close(segment_map.normalize(Fixed::from(input)), expected);
        });

        Ok(())
    }
}
