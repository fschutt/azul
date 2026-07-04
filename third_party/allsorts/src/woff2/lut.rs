//! WOFF2 lookup tables.

use crate::tag::*;

// When table tags are encoded into a WOFF2 TableDirectoryEntry this table is used to provide a
// 5-bit encoding for common tables. The tables are in the order that they are encoded such that
// the value read from the file can be looked up in this array to get the corresponding tag. If the
// value is 0b11111 (63), which is not present in this table, then this is an indication that a
// 4-byte tag follows the tag in the data stream.
// https://www.w3.org/TR/WOFF2/#table_dir_format
pub static KNOWN_TABLE_TAGS: [u32; 63] = [
    CMAP, HEAD, HHEA, HMTX, MAXP, NAME, OS_2, POST, CVT, FPGM, GLYF, LOCA, PREP, CFF, VORG, EBDT,
    EBLC, GASP, HDMX, KERN, LTSH, PCLT, VDMX, VHEA, VMTX, BASE, GDEF, GPOS, GSUB, EBSC, JSTF, MATH,
    CBDT, CBLC, COLR, CPAL, SVG, SBIX, ACNT, AVAR, BDAT, BLOC, BSLN, CVAR, FDSC, FEAT, FMTX, FVAR,
    GVAR, HSTY, JUST, LCAR, MORT, MORX, OPBD, PROP, TRAK, ZAPF, SILF, GLAT, GLOC, FEAT2, SILL,
];

pub struct XYTriplet {
    pub x_is_negative: bool,
    pub y_is_negative: bool,
    pub byte_count: u8,
    pub x_bits: u8,
    pub y_bits: u8,
    pub delta_x: u16,
    pub delta_y: u16,
}

impl XYTriplet {
    pub fn dx(&self, data: u32) -> i16 {
        let mask = (1u32 << self.x_bits) - 1;
        let shift = (self.byte_count * 8) - self.x_bits;
        let dx = ((data >> shift) & mask) + u32::from(self.delta_x);

        if self.x_is_negative {
            -(dx as i16)
        } else {
            dx as i16
        }
    }

    pub fn dy(&self, data: u32) -> i16 {
        let mask = (1u32 << self.y_bits) - 1;
        let shift = (self.byte_count * 8) - self.x_bits - self.y_bits;
        let dy = ((data >> shift) & mask) + u32::from(self.delta_y);

        if self.y_is_negative {
            -(dy as i16)
        } else {
            dy as i16
        }
    }
}

// Lookup table for decoding transformed glyf table point coordinates
// https://www.w3.org/TR/WOFF2/#glyf_table_format
#[rustfmt::skip]
pub static COORD_LUT: [XYTriplet; 128] = [
    XYTriplet { byte_count: 1, x_bits: 0,  y_bits: 8,  delta_x: 0,    delta_y: 0,    x_is_negative: false, y_is_negative: true  },
    XYTriplet { byte_count: 1, x_bits: 0,  y_bits: 8,  delta_x: 0,    delta_y: 0,    x_is_negative: false, y_is_negative: false },
    XYTriplet { byte_count: 1, x_bits: 0,  y_bits: 8,  delta_x: 0,    delta_y: 256,  x_is_negative: false, y_is_negative: true  },
    XYTriplet { byte_count: 1, x_bits: 0,  y_bits: 8,  delta_x: 0,    delta_y: 256,  x_is_negative: false, y_is_negative: false },
    XYTriplet { byte_count: 1, x_bits: 0,  y_bits: 8,  delta_x: 0,    delta_y: 512,  x_is_negative: false, y_is_negative: true  },
    XYTriplet { byte_count: 1, x_bits: 0,  y_bits: 8,  delta_x: 0,    delta_y: 512,  x_is_negative: false, y_is_negative: false },
    XYTriplet { byte_count: 1, x_bits: 0,  y_bits: 8,  delta_x: 0,    delta_y: 768,  x_is_negative: false, y_is_negative: true  },
    XYTriplet { byte_count: 1, x_bits: 0,  y_bits: 8,  delta_x: 0,    delta_y: 768,  x_is_negative: false, y_is_negative: false },
    XYTriplet { byte_count: 1, x_bits: 0,  y_bits: 8,  delta_x: 0,    delta_y: 1024, x_is_negative: false, y_is_negative: true  },
    XYTriplet { byte_count: 1, x_bits: 0,  y_bits: 8,  delta_x: 0,    delta_y: 1024, x_is_negative: false, y_is_negative: false },
    XYTriplet { byte_count: 1, x_bits: 8,  y_bits: 0,  delta_x: 0,    delta_y: 0,    x_is_negative: true,  y_is_negative: false },
    XYTriplet { byte_count: 1, x_bits: 8,  y_bits: 0,  delta_x: 0,    delta_y: 0,    x_is_negative: false, y_is_negative: false },
    XYTriplet { byte_count: 1, x_bits: 8,  y_bits: 0,  delta_x: 256,  delta_y: 0,    x_is_negative: true,  y_is_negative: false },
    XYTriplet { byte_count: 1, x_bits: 8,  y_bits: 0,  delta_x: 256,  delta_y: 0,    x_is_negative: false, y_is_negative: false },
    XYTriplet { byte_count: 1, x_bits: 8,  y_bits: 0,  delta_x: 512,  delta_y: 0,    x_is_negative: true,  y_is_negative: false },
    XYTriplet { byte_count: 1, x_bits: 8,  y_bits: 0,  delta_x: 512,  delta_y: 0,    x_is_negative: false, y_is_negative: false },
    XYTriplet { byte_count: 1, x_bits: 8,  y_bits: 0,  delta_x: 768,  delta_y: 0,    x_is_negative: true,  y_is_negative: false },
    XYTriplet { byte_count: 1, x_bits: 8,  y_bits: 0,  delta_x: 768,  delta_y: 0,    x_is_negative: false, y_is_negative: false },
    XYTriplet { byte_count: 1, x_bits: 8,  y_bits: 0,  delta_x: 1024, delta_y: 0,    x_is_negative: true,  y_is_negative: false },
    XYTriplet { byte_count: 1, x_bits: 8,  y_bits: 0,  delta_x: 1024, delta_y: 0,    x_is_negative: false, y_is_negative: false },
    XYTriplet { byte_count: 1, x_bits: 4,  y_bits: 4,  delta_x: 1,    delta_y: 1,    x_is_negative: true,  y_is_negative: true  },
    XYTriplet { byte_count: 1, x_bits: 4,  y_bits: 4,  delta_x: 1,    delta_y: 1,    x_is_negative: false, y_is_negative: true  },
    XYTriplet { byte_count: 1, x_bits: 4,  y_bits: 4,  delta_x: 1,    delta_y: 1,    x_is_negative: true,  y_is_negative: false },
    XYTriplet { byte_count: 1, x_bits: 4,  y_bits: 4,  delta_x: 1,    delta_y: 1,    x_is_negative: false, y_is_negative: false },
    XYTriplet { byte_count: 1, x_bits: 4,  y_bits: 4,  delta_x: 1,    delta_y: 17,   x_is_negative: true,  y_is_negative: true  },
    XYTriplet { byte_count: 1, x_bits: 4,  y_bits: 4,  delta_x: 1,    delta_y: 17,   x_is_negative: false, y_is_negative: true  },
    XYTriplet { byte_count: 1, x_bits: 4,  y_bits: 4,  delta_x: 1,    delta_y: 17,   x_is_negative: true,  y_is_negative: false },
    XYTriplet { byte_count: 1, x_bits: 4,  y_bits: 4,  delta_x: 1,    delta_y: 17,   x_is_negative: false, y_is_negative: false },
    XYTriplet { byte_count: 1, x_bits: 4,  y_bits: 4,  delta_x: 1,    delta_y: 33,   x_is_negative: true,  y_is_negative: true  },
    XYTriplet { byte_count: 1, x_bits: 4,  y_bits: 4,  delta_x: 1,    delta_y: 33,   x_is_negative: false, y_is_negative: true  },
    XYTriplet { byte_count: 1, x_bits: 4,  y_bits: 4,  delta_x: 1,    delta_y: 33,   x_is_negative: true,  y_is_negative: false },
    XYTriplet { byte_count: 1, x_bits: 4,  y_bits: 4,  delta_x: 1,    delta_y: 33,   x_is_negative: false, y_is_negative: false },
    XYTriplet { byte_count: 1, x_bits: 4,  y_bits: 4,  delta_x: 1,    delta_y: 49,   x_is_negative: true,  y_is_negative: true  },
    XYTriplet { byte_count: 1, x_bits: 4,  y_bits: 4,  delta_x: 1,    delta_y: 49,   x_is_negative: false, y_is_negative: true  },
    XYTriplet { byte_count: 1, x_bits: 4,  y_bits: 4,  delta_x: 1,    delta_y: 49,   x_is_negative: true,  y_is_negative: false },
    XYTriplet { byte_count: 1, x_bits: 4,  y_bits: 4,  delta_x: 1,    delta_y: 49,   x_is_negative: false, y_is_negative: false },
    XYTriplet { byte_count: 1, x_bits: 4,  y_bits: 4,  delta_x: 17,   delta_y: 1,    x_is_negative: true,  y_is_negative: true  },
    XYTriplet { byte_count: 1, x_bits: 4,  y_bits: 4,  delta_x: 17,   delta_y: 1,    x_is_negative: false, y_is_negative: true  },
    XYTriplet { byte_count: 1, x_bits: 4,  y_bits: 4,  delta_x: 17,   delta_y: 1,    x_is_negative: true,  y_is_negative: false },
    XYTriplet { byte_count: 1, x_bits: 4,  y_bits: 4,  delta_x: 17,   delta_y: 1,    x_is_negative: false, y_is_negative: false },
    XYTriplet { byte_count: 1, x_bits: 4,  y_bits: 4,  delta_x: 17,   delta_y: 17,   x_is_negative: true,  y_is_negative: true  },
    XYTriplet { byte_count: 1, x_bits: 4,  y_bits: 4,  delta_x: 17,   delta_y: 17,   x_is_negative: false, y_is_negative: true  },
    XYTriplet { byte_count: 1, x_bits: 4,  y_bits: 4,  delta_x: 17,   delta_y: 17,   x_is_negative: true,  y_is_negative: false },
    XYTriplet { byte_count: 1, x_bits: 4,  y_bits: 4,  delta_x: 17,   delta_y: 17,   x_is_negative: false, y_is_negative: false },
    XYTriplet { byte_count: 1, x_bits: 4,  y_bits: 4,  delta_x: 17,   delta_y: 33,   x_is_negative: true,  y_is_negative: true  },
    XYTriplet { byte_count: 1, x_bits: 4,  y_bits: 4,  delta_x: 17,   delta_y: 33,   x_is_negative: false, y_is_negative: true  },
    XYTriplet { byte_count: 1, x_bits: 4,  y_bits: 4,  delta_x: 17,   delta_y: 33,   x_is_negative: true,  y_is_negative: false },
    XYTriplet { byte_count: 1, x_bits: 4,  y_bits: 4,  delta_x: 17,   delta_y: 33,   x_is_negative: false, y_is_negative: false },
    XYTriplet { byte_count: 1, x_bits: 4,  y_bits: 4,  delta_x: 17,   delta_y: 49,   x_is_negative: true,  y_is_negative: true  },
    XYTriplet { byte_count: 1, x_bits: 4,  y_bits: 4,  delta_x: 17,   delta_y: 49,   x_is_negative: false, y_is_negative: true  },
    XYTriplet { byte_count: 1, x_bits: 4,  y_bits: 4,  delta_x: 17,   delta_y: 49,   x_is_negative: true,  y_is_negative: false },
    XYTriplet { byte_count: 1, x_bits: 4,  y_bits: 4,  delta_x: 17,   delta_y: 49,   x_is_negative: false, y_is_negative: false },
    XYTriplet { byte_count: 1, x_bits: 4,  y_bits: 4,  delta_x: 33,   delta_y: 1,    x_is_negative: true,  y_is_negative: true  },
    XYTriplet { byte_count: 1, x_bits: 4,  y_bits: 4,  delta_x: 33,   delta_y: 1,    x_is_negative: false, y_is_negative: true  },
    XYTriplet { byte_count: 1, x_bits: 4,  y_bits: 4,  delta_x: 33,   delta_y: 1,    x_is_negative: true,  y_is_negative: false },
    XYTriplet { byte_count: 1, x_bits: 4,  y_bits: 4,  delta_x: 33,   delta_y: 1,    x_is_negative: false, y_is_negative: false },
    XYTriplet { byte_count: 1, x_bits: 4,  y_bits: 4,  delta_x: 33,   delta_y: 17,   x_is_negative: true,  y_is_negative: true  },
    XYTriplet { byte_count: 1, x_bits: 4,  y_bits: 4,  delta_x: 33,   delta_y: 17,   x_is_negative: false, y_is_negative: true  },
    XYTriplet { byte_count: 1, x_bits: 4,  y_bits: 4,  delta_x: 33,   delta_y: 17,   x_is_negative: true,  y_is_negative: false },
    XYTriplet { byte_count: 1, x_bits: 4,  y_bits: 4,  delta_x: 33,   delta_y: 17,   x_is_negative: false, y_is_negative: false },
    XYTriplet { byte_count: 1, x_bits: 4,  y_bits: 4,  delta_x: 33,   delta_y: 33,   x_is_negative: true,  y_is_negative: true  },
    XYTriplet { byte_count: 1, x_bits: 4,  y_bits: 4,  delta_x: 33,   delta_y: 33,   x_is_negative: false, y_is_negative: true  },
    XYTriplet { byte_count: 1, x_bits: 4,  y_bits: 4,  delta_x: 33,   delta_y: 33,   x_is_negative: true,  y_is_negative: false },
    XYTriplet { byte_count: 1, x_bits: 4,  y_bits: 4,  delta_x: 33,   delta_y: 33,   x_is_negative: false, y_is_negative: false },
    XYTriplet { byte_count: 1, x_bits: 4,  y_bits: 4,  delta_x: 33,   delta_y: 49,   x_is_negative: true,  y_is_negative: true  },
    XYTriplet { byte_count: 1, x_bits: 4,  y_bits: 4,  delta_x: 33,   delta_y: 49,   x_is_negative: false, y_is_negative: true  },
    XYTriplet { byte_count: 1, x_bits: 4,  y_bits: 4,  delta_x: 33,   delta_y: 49,   x_is_negative: true,  y_is_negative: false },
    XYTriplet { byte_count: 1, x_bits: 4,  y_bits: 4,  delta_x: 33,   delta_y: 49,   x_is_negative: false, y_is_negative: false },
    XYTriplet { byte_count: 1, x_bits: 4,  y_bits: 4,  delta_x: 49,   delta_y: 1,    x_is_negative: true,  y_is_negative: true  },
    XYTriplet { byte_count: 1, x_bits: 4,  y_bits: 4,  delta_x: 49,   delta_y: 1,    x_is_negative: false, y_is_negative: true  },
    XYTriplet { byte_count: 1, x_bits: 4,  y_bits: 4,  delta_x: 49,   delta_y: 1,    x_is_negative: true,  y_is_negative: false },
    XYTriplet { byte_count: 1, x_bits: 4,  y_bits: 4,  delta_x: 49,   delta_y: 1,    x_is_negative: false, y_is_negative: false },
    XYTriplet { byte_count: 1, x_bits: 4,  y_bits: 4,  delta_x: 49,   delta_y: 17,   x_is_negative: true,  y_is_negative: true  },
    XYTriplet { byte_count: 1, x_bits: 4,  y_bits: 4,  delta_x: 49,   delta_y: 17,   x_is_negative: false, y_is_negative: true  },
    XYTriplet { byte_count: 1, x_bits: 4,  y_bits: 4,  delta_x: 49,   delta_y: 17,   x_is_negative: true,  y_is_negative: false },
    XYTriplet { byte_count: 1, x_bits: 4,  y_bits: 4,  delta_x: 49,   delta_y: 17,   x_is_negative: false, y_is_negative: false },
    XYTriplet { byte_count: 1, x_bits: 4,  y_bits: 4,  delta_x: 49,   delta_y: 33,   x_is_negative: true,  y_is_negative: true  },
    XYTriplet { byte_count: 1, x_bits: 4,  y_bits: 4,  delta_x: 49,   delta_y: 33,   x_is_negative: false, y_is_negative: true  },
    XYTriplet { byte_count: 1, x_bits: 4,  y_bits: 4,  delta_x: 49,   delta_y: 33,   x_is_negative: true,  y_is_negative: false },
    XYTriplet { byte_count: 1, x_bits: 4,  y_bits: 4,  delta_x: 49,   delta_y: 33,   x_is_negative: false, y_is_negative: false },
    XYTriplet { byte_count: 1, x_bits: 4,  y_bits: 4,  delta_x: 49,   delta_y: 49,   x_is_negative: true,  y_is_negative: true  },
    XYTriplet { byte_count: 1, x_bits: 4,  y_bits: 4,  delta_x: 49,   delta_y: 49,   x_is_negative: false, y_is_negative: true  },
    XYTriplet { byte_count: 1, x_bits: 4,  y_bits: 4,  delta_x: 49,   delta_y: 49,   x_is_negative: true,  y_is_negative: false },
    XYTriplet { byte_count: 1, x_bits: 4,  y_bits: 4,  delta_x: 49,   delta_y: 49,   x_is_negative: false, y_is_negative: false },
    XYTriplet { byte_count: 2, x_bits: 8,  y_bits: 8,  delta_x: 1,    delta_y: 1,    x_is_negative: true,  y_is_negative: true  },
    XYTriplet { byte_count: 2, x_bits: 8,  y_bits: 8,  delta_x: 1,    delta_y: 1,    x_is_negative: false, y_is_negative: true  },
    XYTriplet { byte_count: 2, x_bits: 8,  y_bits: 8,  delta_x: 1,    delta_y: 1,    x_is_negative: true,  y_is_negative: false },
    XYTriplet { byte_count: 2, x_bits: 8,  y_bits: 8,  delta_x: 1,    delta_y: 1,    x_is_negative: false, y_is_negative: false },
    XYTriplet { byte_count: 2, x_bits: 8,  y_bits: 8,  delta_x: 1,    delta_y: 257,  x_is_negative: true,  y_is_negative: true  },
    XYTriplet { byte_count: 2, x_bits: 8,  y_bits: 8,  delta_x: 1,    delta_y: 257,  x_is_negative: false, y_is_negative: true  },
    XYTriplet { byte_count: 2, x_bits: 8,  y_bits: 8,  delta_x: 1,    delta_y: 257,  x_is_negative: true,  y_is_negative: false },
    XYTriplet { byte_count: 2, x_bits: 8,  y_bits: 8,  delta_x: 1,    delta_y: 257,  x_is_negative: false, y_is_negative: false },
    XYTriplet { byte_count: 2, x_bits: 8,  y_bits: 8,  delta_x: 1,    delta_y: 513,  x_is_negative: true,  y_is_negative: true  },
    XYTriplet { byte_count: 2, x_bits: 8,  y_bits: 8,  delta_x: 1,    delta_y: 513,  x_is_negative: false, y_is_negative: true  },
    XYTriplet { byte_count: 2, x_bits: 8,  y_bits: 8,  delta_x: 1,    delta_y: 513,  x_is_negative: true,  y_is_negative: false },
    XYTriplet { byte_count: 2, x_bits: 8,  y_bits: 8,  delta_x: 1,    delta_y: 513,  x_is_negative: false, y_is_negative: false },
    XYTriplet { byte_count: 2, x_bits: 8,  y_bits: 8,  delta_x: 257,  delta_y: 1,    x_is_negative: true,  y_is_negative: true  },
    XYTriplet { byte_count: 2, x_bits: 8,  y_bits: 8,  delta_x: 257,  delta_y: 1,    x_is_negative: false, y_is_negative: true  },
    XYTriplet { byte_count: 2, x_bits: 8,  y_bits: 8,  delta_x: 257,  delta_y: 1,    x_is_negative: true,  y_is_negative: false },
    XYTriplet { byte_count: 2, x_bits: 8,  y_bits: 8,  delta_x: 257,  delta_y: 1,    x_is_negative: false, y_is_negative: false },
    XYTriplet { byte_count: 2, x_bits: 8,  y_bits: 8,  delta_x: 257,  delta_y: 257,  x_is_negative: true,  y_is_negative: true  },
    XYTriplet { byte_count: 2, x_bits: 8,  y_bits: 8,  delta_x: 257,  delta_y: 257,  x_is_negative: false, y_is_negative: true  },
    XYTriplet { byte_count: 2, x_bits: 8,  y_bits: 8,  delta_x: 257,  delta_y: 257,  x_is_negative: true,  y_is_negative: false },
    XYTriplet { byte_count: 2, x_bits: 8,  y_bits: 8,  delta_x: 257,  delta_y: 257,  x_is_negative: false, y_is_negative: false },
    XYTriplet { byte_count: 2, x_bits: 8,  y_bits: 8,  delta_x: 257,  delta_y: 513,  x_is_negative: true,  y_is_negative: true  },
    XYTriplet { byte_count: 2, x_bits: 8,  y_bits: 8,  delta_x: 257,  delta_y: 513,  x_is_negative: false, y_is_negative: true  },
    XYTriplet { byte_count: 2, x_bits: 8,  y_bits: 8,  delta_x: 257,  delta_y: 513,  x_is_negative: true,  y_is_negative: false },
    XYTriplet { byte_count: 2, x_bits: 8,  y_bits: 8,  delta_x: 257,  delta_y: 513,  x_is_negative: false, y_is_negative: false },
    XYTriplet { byte_count: 2, x_bits: 8,  y_bits: 8,  delta_x: 513,  delta_y: 1,    x_is_negative: true,  y_is_negative: true  },
    XYTriplet { byte_count: 2, x_bits: 8,  y_bits: 8,  delta_x: 513,  delta_y: 1,    x_is_negative: false, y_is_negative: true  },
    XYTriplet { byte_count: 2, x_bits: 8,  y_bits: 8,  delta_x: 513,  delta_y: 1,    x_is_negative: true,  y_is_negative: false },
    XYTriplet { byte_count: 2, x_bits: 8,  y_bits: 8,  delta_x: 513,  delta_y: 1,    x_is_negative: false, y_is_negative: false },
    XYTriplet { byte_count: 2, x_bits: 8,  y_bits: 8,  delta_x: 513,  delta_y: 257,  x_is_negative: true,  y_is_negative: true  },
    XYTriplet { byte_count: 2, x_bits: 8,  y_bits: 8,  delta_x: 513,  delta_y: 257,  x_is_negative: false, y_is_negative: true  },
    XYTriplet { byte_count: 2, x_bits: 8,  y_bits: 8,  delta_x: 513,  delta_y: 257,  x_is_negative: true,  y_is_negative: false },
    XYTriplet { byte_count: 2, x_bits: 8,  y_bits: 8,  delta_x: 513,  delta_y: 257,  x_is_negative: false, y_is_negative: false },
    XYTriplet { byte_count: 2, x_bits: 8,  y_bits: 8,  delta_x: 513,  delta_y: 513,  x_is_negative: true,  y_is_negative: true  },
    XYTriplet { byte_count: 2, x_bits: 8,  y_bits: 8,  delta_x: 513,  delta_y: 513,  x_is_negative: false, y_is_negative: true  },
    XYTriplet { byte_count: 2, x_bits: 8,  y_bits: 8,  delta_x: 513,  delta_y: 513,  x_is_negative: true,  y_is_negative: false },
    XYTriplet { byte_count: 2, x_bits: 8,  y_bits: 8,  delta_x: 513,  delta_y: 513,  x_is_negative: false, y_is_negative: false },
    XYTriplet { byte_count: 3, x_bits: 12, y_bits: 12, delta_x: 0,    delta_y: 0,    x_is_negative: true,  y_is_negative: true  },
    XYTriplet { byte_count: 3, x_bits: 12, y_bits: 12, delta_x: 0,    delta_y: 0,    x_is_negative: false, y_is_negative: true  },
    XYTriplet { byte_count: 3, x_bits: 12, y_bits: 12, delta_x: 0,    delta_y: 0,    x_is_negative: true,  y_is_negative: false },
    XYTriplet { byte_count: 3, x_bits: 12, y_bits: 12, delta_x: 0,    delta_y: 0,    x_is_negative: false, y_is_negative: false },
    XYTriplet { byte_count: 4, x_bits: 16, y_bits: 16, delta_x: 0,    delta_y: 0,    x_is_negative: true,  y_is_negative: true  },
    XYTriplet { byte_count: 4, x_bits: 16, y_bits: 16, delta_x: 0,    delta_y: 0,    x_is_negative: false, y_is_negative: true  },
    XYTriplet { byte_count: 4, x_bits: 16, y_bits: 16, delta_x: 0,    delta_y: 0,    x_is_negative: true,  y_is_negative: false },
    XYTriplet { byte_count: 4, x_bits: 16, y_bits: 16, delta_x: 0,    delta_y: 0,    x_is_negative: false, y_is_negative: false },
];
