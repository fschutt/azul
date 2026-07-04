//! Shared test code.

include!("../tests/common.rs");

use crate::tables::{F2Dot14, Fixed};

pub(crate) mod writer {
    //! Testing utilities.
    #![allow(dead_code)]

    // The writer module is derived from ttf-parser, licenced under Apache-2.0.
    // https://github.com/RazrFalcon/ttf-parser/blob/439aaaebd50eb8aed66302e3c1b51fae047f85b2/src/writer.rs

    #[allow(missing_debug_implementations)]
    #[derive(Clone, Copy)]
    pub enum TtfType {
        Raw(&'static [u8]),
        TrueTypeMagic,
        OpenTypeMagic,
        FontCollectionMagic,
        Int8(i8),
        UInt8(u8),
        Int16(i16),
        UInt16(u16),
        Int32(i32),
        UInt32(u32),
        CFFInt(i32),
    }

    pub fn convert(values: &[TtfType]) -> Vec<u8> {
        let mut data = Vec::with_capacity(256);
        for v in values {
            convert_type(*v, &mut data);
        }

        data
    }

    pub fn convert_type(value: TtfType, data: &mut Vec<u8>) {
        match value {
            TtfType::Raw(bytes) => {
                data.extend_from_slice(bytes);
            }
            TtfType::TrueTypeMagic => {
                data.extend_from_slice(&[0x00, 0x01, 0x00, 0x00]);
            }
            TtfType::OpenTypeMagic => {
                data.extend_from_slice(&[0x4F, 0x54, 0x54, 0x4F]);
            }
            TtfType::FontCollectionMagic => {
                data.extend_from_slice(&[0x74, 0x74, 0x63, 0x66]);
            }
            TtfType::Int8(n) => {
                data.extend_from_slice(&i8::to_be_bytes(n));
            }
            TtfType::UInt8(n) => {
                data.extend_from_slice(&u8::to_be_bytes(n));
            }
            TtfType::Int16(n) => {
                data.extend_from_slice(&i16::to_be_bytes(n));
            }
            TtfType::UInt16(n) => {
                data.extend_from_slice(&u16::to_be_bytes(n));
            }
            TtfType::Int32(n) => {
                data.extend_from_slice(&i32::to_be_bytes(n));
            }
            TtfType::UInt32(n) => {
                data.extend_from_slice(&u32::to_be_bytes(n));
            }
            TtfType::CFFInt(n) => match n {
                -107..=107 => {
                    data.push((n as i16 + 139) as u8);
                }
                108..=1131 => {
                    let n = n - 108;
                    data.push(((n >> 8) + 247) as u8);
                    data.push((n & 0xFF) as u8);
                }
                -1131..=-108 => {
                    let n = -n - 108;
                    data.push(((n >> 8) + 251) as u8);
                    data.push((n & 0xFF) as u8);
                }
                -32768..=32767 => {
                    data.push(28);
                    data.extend_from_slice(&i16::to_be_bytes(n as i16));
                }
                _ => {
                    data.push(29);
                    data.extend_from_slice(&i32::to_be_bytes(n));
                }
            },
        }
    }

    #[derive(Debug)]
    pub struct Writer {
        pub data: Vec<u8>,
    }

    impl Writer {
        pub fn new() -> Self {
            Writer {
                data: Vec::with_capacity(256),
            }
        }

        pub fn offset(&self) -> usize {
            self.data.len()
        }

        pub fn write(&mut self, value: TtfType) {
            convert_type(value, &mut self.data);
        }
    }
}

#[macro_export]
macro_rules! assert_close {
    ($actual:expr, $expected:expr) => {
        assert_close!($actual, $expected, f32::EPSILON);
    };

    ($actual:expr, $expected:expr, $eps:expr) => {
        assert!(
            ($actual - $expected).abs() < $eps,
            "{:?} != {:?} ± {}",
            $actual,
            $expected,
            $eps
        );
    };
}

// Compatibility function
pub fn assert_close(actual: f32, expected: f32) {
    assert_close!(actual, expected);
}

pub fn assert_fixed_close(actual: Fixed, expected: f32) {
    let expected = Fixed::from(expected);
    assert!(
        (actual.raw_value().wrapping_sub(expected.raw_value())).abs() <= 3,
        "{} ({:?}) != {} ({:?}) ± {}",
        f32::from(actual),
        actual,
        f32::from(expected),
        expected,
        3. / 65535.
    );
}

pub fn assert_f2dot14_close(actual: F2Dot14, expected: f32) {
    let expected = F2Dot14::from(expected);
    assert!(
        (actual.raw_value().wrapping_sub(expected.raw_value())).abs() <= 3,
        "{} ({:?}) != {} ({:?}) ± {}",
        f32::from(actual),
        actual,
        f32::from(expected),
        expected,
        3. / 16384.
    );
}
