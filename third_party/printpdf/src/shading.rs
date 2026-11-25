//! Gradients: axial and radial shadings.
//!
//! A *shading* is a PDF object that paints a smooth transition between colors. It is used for
//! gradients. An *axial* shading blends along a line between two points. A *radial* shading
//! blends between two circles. The blend is driven by a parameter `t` that runs from 0.0 at the
//! start to 1.0 at the end, and a PDF *function* that maps `t` to a color.
//!
//! This crate models a shading as a [`Shading`]: a geometry ([`ShadingGeometry`]), a list of color
//! stops ([`GradientStop`]) and the two `extend` flags. Colors are always DeviceRGB. Two stops
//! become one exponential function (function type 2). More stops become a stitching function
//! (type 3) over one exponential function per segment. The whole shading is written as one inline
//! dictionary in the page's `/Shading` resources, without extra indirect objects.
//!
//! To use a gradient, register it with [`PdfDocument::add_shading`](crate::PdfDocument::add_shading)
//! and paint it with [`Op::PaintShading`](crate::Op::PaintShading) (the `sh` operator). `sh`
//! fills the whole current clip region, so clip to the shape you want first, for example with a
//! polygon in [`PaintMode::Clip`](crate::PaintMode::Clip). The HTML renderer uses this for CSS
//! `linear-gradient` and `radial-gradient`.
//!
//! # Examples
//!
//! ```
//! use printpdf::*;
//!
//! let mut doc = PdfDocument::new("gradient");
//! // Red to blue, from the left to the right edge of a 100 pt wide area.
//! let shading = Shading {
//!     geometry: ShadingGeometry::Axial { coords: [0.0, 0.0, 100.0, 0.0] },
//!     stops: vec![
//!         GradientStop { offset: 0.0, color: [1.0, 0.0, 0.0] },
//!         GradientStop { offset: 1.0, color: [0.0, 0.0, 1.0] },
//!     ],
//!     extend: (true, true),
//! };
//! let id = doc.add_shading(shading);
//! let ops = vec![Op::PaintShading { id }];
//! # let _ = ops;
//! ```

use serde_derive::{Deserialize, Serialize};
use lopdf::Dictionary as LoDictionary;

/// Id of a shading in the document's resources, as returned by `PdfDocument::add_shading`.
#[derive(Debug, PartialEq, Clone, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ShadingId(pub String);

impl ShadingId {
    /// Creates a new random id.
    pub fn new() -> Self {
        Self(crate::utils::random_character_string_32())
    }
}

impl Default for ShadingId {
    fn default() -> Self {
        Self::new()
    }
}

/// One color stop of a gradient: a color at a position along the gradient.
#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct GradientStop {
    /// Position along the gradient, from 0.0 (start) to 1.0 (end). Values outside that range are
    /// clamped when the shading is written.
    pub offset: f32,
    /// Color as `[red, green, blue]`, each from 0.0 to 1.0 (not 0 to 255).
    pub color: [f32; 3],
}

/// Where a shading is painted: a line for an axial shading, two circles for a radial one.
///
/// All coordinates are in points in the user space that is active when the shading is painted
/// (the origin is the bottom left corner of the page unless the transformation matrix was
/// changed).
#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub enum ShadingGeometry {
    /// Linear gradient along the line from `(x0, y0)` to `(x1, y1)`. `coords` is
    /// `[x0, y0, x1, y1]`. The start color is at the first point, the end color at the second.
    Axial {
        /// `[x0, y0, x1, y1]` in points.
        coords: [f32; 4],
    },
    /// Gradient between two circles. `coords` is `[x0, y0, r0, x1, y1, r1]`: center and radius
    /// of the start circle, then of the end circle. The colors spread outward from the start
    /// circle to the end circle.
    Radial {
        /// `[x0, y0, r0, x1, y1, r1]` in points.
        coords: [f32; 6],
    },
}

/// A gradient: a geometry, color stops and extension flags. Written as a PDF shading dictionary.
#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct Shading {
    /// Axial line or radial circles, in points.
    pub geometry: ShadingGeometry,
    /// Color stops, sorted by `offset` ascending. A shading without stops is written as black and
    /// one stop as a solid color.
    pub stops: Vec<GradientStop>,
    /// Whether the end colors continue past the start and the end of the geometry. With `false`,
    /// nothing is painted beyond that side. `(true, true)` mimics CSS gradients.
    pub extend: (bool, bool),
}

impl Shading {
    /// Builds the shading dictionary (ISO 32000-1, 8.7.4.5) with an inline function that maps
    /// `t` to an RGB color.
    pub fn to_dict(&self) -> LoDictionary {
        use lopdf::Object::*;

        let (shading_type, coords): (i64, Vec<lopdf::Object>) = match &self.geometry {
            ShadingGeometry::Axial { coords } => (2, coords.iter().map(|v| Real(*v)).collect()),
            ShadingGeometry::Radial { coords } => (3, coords.iter().map(|v| Real(*v)).collect()),
        };

        let mut dict = LoDictionary::new();
        dict.set("ShadingType", Integer(shading_type));
        dict.set("ColorSpace", Name(b"DeviceRGB".to_vec()));
        dict.set("Coords", Array(coords));
        dict.set("Function", self.build_function());
        dict.set(
            "Extend",
            Array(vec![Boolean(self.extend.0), Boolean(self.extend.1)]),
        );
        dict
    }

    // Turns the stops into linear segments `(t_start, t_end, c_start, c_end)`. Constant segments
    // fill `[0, first_offset]` and `[last_offset, 1]` when the stops do not cover the whole
    // range, so `red 30%, blue` is solid red up to 30% as in CSS.
    fn build_function(&self) -> lopdf::Object {
        let (first, last) = match self.stops.as_slice() {
            [] => return lopdf::Object::Dictionary(exponential_fn([0.0; 3], [0.0; 3])),
            [only] => {
                let c = only.color;
                return lopdf::Object::Dictionary(exponential_fn(c, c));
            }
            [first, .., last] => (first, last),
        };

        let mut segs: Vec<(f32, f32, [f32; 3], [f32; 3])> = Vec::new();
        let o_first = first.offset.clamp(0.0, 1.0);
        let o_last = last.offset.clamp(0.0, 1.0);

        if o_first > 0.0 {
            segs.push((0.0, o_first, first.color, first.color));
        }
        for pair in self.stops.windows(2) {
            let [a, b] = pair else { continue };
            let ta = a.offset.clamp(0.0, 1.0);
            let tb = b.offset.clamp(0.0, 1.0);
            if tb > ta {
                segs.push((ta, tb, a.color, b.color));
            }
        }
        if o_last < 1.0 {
            segs.push((o_last, 1.0, last.color, last.color));
        }

        match segs.as_slice() {
            [] => {
                // All stops at the same offset: constant color.
                let c = first.color;
                return lopdf::Object::Dictionary(exponential_fn(c, c));
            }
            [s] => return lopdf::Object::Dictionary(exponential_fn(s.2, s.3)),
            _ => {}
        }

        use lopdf::Object::*;
        let functions: Vec<lopdf::Object> = segs
            .iter()
            .map(|s| Dictionary(exponential_fn(s.2, s.3)))
            .collect();
        // The end of every segment except the last is a breakpoint.
        let bounds: Vec<lopdf::Object> = segs
            .iter()
            .take(segs.len().saturating_sub(1))
            .map(|s| Real(s.1))
            .collect();
        // Each segment maps its sub-domain onto the [0, 1] domain of its function.
        let encode: Vec<lopdf::Object> = segs.iter().flat_map(|_| [Real(0.0), Real(1.0)]).collect();

        let mut dict = LoDictionary::new();
        dict.set("FunctionType", Integer(3));
        dict.set("Domain", Array(vec![Real(0.0), Real(1.0)]));
        dict.set("Functions", Array(functions));
        dict.set("Bounds", Array(bounds));
        dict.set("Encode", Array(encode));
        Dictionary(dict)
    }
}

// Function type 2 (exponential interpolation, exponent 1 = linear) from `c0` to `c1` over [0, 1].
fn exponential_fn(c0: [f32; 3], c1: [f32; 3]) -> LoDictionary {
    use lopdf::Object::*;
    let mut dict = LoDictionary::new();
    dict.set("FunctionType", Integer(2));
    dict.set("Domain", Array(vec![Real(0.0), Real(1.0)]));
    dict.set("C0", Array(c0.iter().map(|v| Real(*v)).collect()));
    dict.set("C1", Array(c1.iter().map(|v| Real(*v)).collect()));
    dict.set("N", Real(1.0));
    dict
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn axial_two_stop_builds_type2_function() {
        let sh = Shading {
            geometry: ShadingGeometry::Axial { coords: [0.0, 0.0, 100.0, 0.0] },
            stops: vec![
                GradientStop { offset: 0.0, color: [1.0, 0.0, 0.0] },
                GradientStop { offset: 1.0, color: [0.0, 0.0, 1.0] },
            ],
            extend: (true, true),
        };
        let d = sh.to_dict();
        assert_eq!(d.get(b"ShadingType").unwrap().as_i64().unwrap(), 2);
        let f = d.get(b"Function").unwrap().as_dict().unwrap();
        assert_eq!(f.get(b"FunctionType").unwrap().as_i64().unwrap(), 2);
    }

    #[test]
    fn multi_stop_builds_stitching_function() {
        let sh = Shading {
            geometry: ShadingGeometry::Axial { coords: [0.0, 0.0, 0.0, 100.0] },
            stops: vec![
                GradientStop { offset: 0.0, color: [1.0, 0.0, 0.0] },
                GradientStop { offset: 0.5, color: [0.0, 1.0, 0.0] },
                GradientStop { offset: 1.0, color: [0.0, 0.0, 1.0] },
            ],
            extend: (true, true),
        };
        let d = sh.to_dict();
        let f = d.get(b"Function").unwrap().as_dict().unwrap();
        assert_eq!(f.get(b"FunctionType").unwrap().as_i64().unwrap(), 3);
        // 2 segments → 2 functions, 1 interior bound.
        assert_eq!(f.get(b"Functions").unwrap().as_array().unwrap().len(), 2);
        assert_eq!(f.get(b"Bounds").unwrap().as_array().unwrap().len(), 1);
    }

    #[test]
    fn leading_constant_segment_for_offset_gradient() {
        // red 30% .. blue 100%  → a constant [0,0.3] segment + the real one.
        let sh = Shading {
            geometry: ShadingGeometry::Axial { coords: [0.0, 0.0, 100.0, 0.0] },
            stops: vec![
                GradientStop { offset: 0.3, color: [1.0, 0.0, 0.0] },
                GradientStop { offset: 1.0, color: [0.0, 0.0, 1.0] },
            ],
            extend: (true, true),
        };
        let d = sh.to_dict();
        let f = d.get(b"Function").unwrap().as_dict().unwrap();
        assert_eq!(f.get(b"FunctionType").unwrap().as_i64().unwrap(), 3);
        // constant[0,0.3] + interp[0.3,1.0] = 2 functions, bound at 0.3.
        assert_eq!(f.get(b"Functions").unwrap().as_array().unwrap().len(), 2);
        let bounds = f.get(b"Bounds").unwrap().as_array().unwrap();
        assert_eq!(bounds.len(), 1);
        assert!((bounds[0].as_float().unwrap() - 0.3).abs() < 1e-4);
    }
}
