//! SVG `d=""` path data parser.
//!
//! Parses the `d` attribute of SVG `<path>` elements into `SvgMultiPolygon`
//! geometry, supporting all 14 SVG path commands (M/m, L/l, H/h, V/v,
//! C/c, S/s, Q/q, T/t, A/a, Z/z).

use alloc::{string::String, vec::Vec};
use azul_css::props::basic::{SvgCubicCurve, SvgPoint, SvgQuadraticCurve};

use crate::svg::{SvgLine, SvgMultiPolygon, SvgPath, SvgPathElement, SvgPathElementVec, SvgPathVec};

/// Bezier approximation constant for quarter-circle arcs.
const KAPPA: f32 = 0.5522847498;

/// Tolerance for treating two points as coincident (used in closepath and arc degeneracy checks).
const POINT_EPSILON: f32 = 1e-6;

/// Tolerance for snapping a closepath line (slightly larger to avoid micro-segments).
const CLOSEPATH_EPSILON: f32 = 0.001;

/// Tolerance for treating a vector length as zero in angle computation.
const ZERO_LENGTH_EPSILON: f32 = 1e-10;

/// Small offset added to PI/2 when splitting arcs to avoid exact-boundary floating-point issues.
const ARC_SPLIT_FUDGE: f32 = 0.001;

/// Errors that can occur during SVG path parsing.
#[derive(Debug, Clone, PartialEq)]
pub enum SvgPathParseError {
    /// The path string is empty.
    EmptyPath,
    /// Unexpected character encountered at the given byte offset.
    UnexpectedChar { pos: usize, ch: char },
    /// Expected a number but found something else.
    ExpectedNumber { pos: usize },
    /// Invalid arc flag (must be 0 or 1).
    InvalidArcFlag { pos: usize },
}

/// Human-readable error messages for SVG path parse failures.
impl core::fmt::Display for SvgPathParseError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::EmptyPath => write!(f, "empty path"),
            Self::UnexpectedChar { pos, ch } => {
                write!(f, "unexpected char '{}' at byte {}", ch, pos)
            }
            Self::ExpectedNumber { pos } => write!(f, "expected number at byte {}", pos),
            Self::InvalidArcFlag { pos } => write!(f, "invalid arc flag at byte {}", pos),
        }
    }
}

/// Internal parser state.
struct PathParser<'a> {
    input: &'a [u8],
    pos: usize,
    current: SvgPoint,
    subpath_start: SvgPoint,
    last_control: Option<SvgPoint>,
    last_command: u8,
}

impl<'a> PathParser<'a> {
    fn new(input: &'a [u8]) -> Self {
        Self {
            input,
            pos: 0,
            current: SvgPoint { x: 0.0, y: 0.0 },
            subpath_start: SvgPoint { x: 0.0, y: 0.0 },
            last_control: None,
            last_command: 0,
        }
    }

    fn at_end(&self) -> bool {
        self.pos >= self.input.len()
    }

    fn peek(&self) -> Option<u8> {
        self.input.get(self.pos).copied()
    }

    fn skip_whitespace_and_commas(&mut self) {
        while let Some(&b) = self.input.get(self.pos) {
            if b == b' ' || b == b'\t' || b == b'\n' || b == b'\r' || b == b',' {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    fn skip_whitespace(&mut self) {
        while let Some(&b) = self.input.get(self.pos) {
            if b == b' ' || b == b'\t' || b == b'\n' || b == b'\r' {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    /// Returns true if the current position looks like the start of a number.
    fn has_number(&self) -> bool {
        match self.input.get(self.pos) {
            Some(b'+') | Some(b'-') | Some(b'.') => true,
            Some(b) if b.is_ascii_digit() => true,
            _ => false,
        }
    }

    fn parse_number(&mut self) -> Result<f32, SvgPathParseError> {
        self.skip_whitespace_and_commas();
        let start = self.pos;

        // Optional sign
        if let Some(&b) = self.input.get(self.pos) {
            if b == b'+' || b == b'-' {
                self.pos += 1;
            }
        }

        let mut has_digits = false;

        // Integer part
        while let Some(&b) = self.input.get(self.pos) {
            if b.is_ascii_digit() {
                self.pos += 1;
                has_digits = true;
            } else {
                break;
            }
        }

        // Decimal part
        if let Some(&b'.') = self.input.get(self.pos) {
            self.pos += 1;
            while let Some(&b) = self.input.get(self.pos) {
                if b.is_ascii_digit() {
                    self.pos += 1;
                    has_digits = true;
                } else {
                    break;
                }
            }
        }

        if !has_digits {
            return Err(SvgPathParseError::ExpectedNumber { pos: start });
        }

        // Exponent
        if let Some(&b) = self.input.get(self.pos) {
            if b == b'e' || b == b'E' {
                self.pos += 1;
                if let Some(&b) = self.input.get(self.pos) {
                    if b == b'+' || b == b'-' {
                        self.pos += 1;
                    }
                }
                while let Some(&b) = self.input.get(self.pos) {
                    if b.is_ascii_digit() {
                        self.pos += 1;
                    } else {
                        break;
                    }
                }
            }
        }

        let s = core::str::from_utf8(&self.input[start..self.pos])
            .map_err(|_| SvgPathParseError::ExpectedNumber { pos: start })?;
        s.parse::<f32>()
            .map_err(|_| SvgPathParseError::ExpectedNumber { pos: start })
    }

    fn parse_flag(&mut self) -> Result<bool, SvgPathParseError> {
        self.skip_whitespace_and_commas();
        match self.input.get(self.pos) {
            Some(b'0') => {
                self.pos += 1;
                Ok(false)
            }
            Some(b'1') => {
                self.pos += 1;
                Ok(true)
            }
            _ => Err(SvgPathParseError::InvalidArcFlag { pos: self.pos }),
        }
    }

    fn parse_coordinate_pair(&mut self) -> Result<(f32, f32), SvgPathParseError> {
        let x = self.parse_number()?;
        let y = self.parse_number()?;
        Ok((x, y))
    }

    fn make_absolute(&self, x: f32, y: f32, relative: bool) -> SvgPoint {
        if relative {
            SvgPoint {
                x: self.current.x + x,
                y: self.current.y + y,
            }
        } else {
            SvgPoint { x, y }
        }
    }

    fn handle_line_to(&mut self, relative: bool, elements: &mut Vec<SvgPathElement>) -> Result<(), SvgPathParseError> {
        let (x, y) = self.parse_coordinate_pair()?;
        let end = self.make_absolute(x, y, relative);
        elements.push(SvgPathElement::Line(SvgLine { start: self.current, end }));
        self.current = end;
        self.last_control = None;
        Ok(())
    }

    fn handle_horizontal_to(&mut self, relative: bool, elements: &mut Vec<SvgPathElement>) -> Result<(), SvgPathParseError> {
        let x = self.parse_number()?;
        let abs_x = if relative { self.current.x + x } else { x };
        let end = SvgPoint { x: abs_x, y: self.current.y };
        elements.push(SvgPathElement::Line(SvgLine { start: self.current, end }));
        self.current = end;
        self.last_control = None;
        Ok(())
    }

    fn handle_vertical_to(&mut self, relative: bool, elements: &mut Vec<SvgPathElement>) -> Result<(), SvgPathParseError> {
        let y = self.parse_number()?;
        let abs_y = if relative { self.current.y + y } else { y };
        let end = SvgPoint { x: self.current.x, y: abs_y };
        elements.push(SvgPathElement::Line(SvgLine { start: self.current, end }));
        self.current = end;
        self.last_control = None;
        Ok(())
    }

    fn handle_cubic_to(&mut self, relative: bool, elements: &mut Vec<SvgPathElement>) -> Result<(), SvgPathParseError> {
        let (c1x, c1y) = self.parse_coordinate_pair()?;
        let (c2x, c2y) = self.parse_coordinate_pair()?;
        let (ex, ey) = self.parse_coordinate_pair()?;
        let ctrl_1 = self.make_absolute(c1x, c1y, relative);
        let ctrl_2 = self.make_absolute(c2x, c2y, relative);
        let end = self.make_absolute(ex, ey, relative);
        elements.push(SvgPathElement::CubicCurve(SvgCubicCurve {
            start: self.current, ctrl_1, ctrl_2, end,
        }));
        self.last_control = Some(ctrl_2);
        self.current = end;
        Ok(())
    }

    fn handle_smooth_cubic_to(&mut self, relative: bool, elements: &mut Vec<SvgPathElement>) -> Result<(), SvgPathParseError> {
        let ctrl_1 = match self.last_control {
            Some(lc) if matches!(self.last_command.to_ascii_uppercase(), b'C' | b'S') => {
                SvgPoint {
                    x: 2.0 * self.current.x - lc.x,
                    y: 2.0 * self.current.y - lc.y,
                }
            }
            _ => self.current,
        };
        let (c2x, c2y) = self.parse_coordinate_pair()?;
        let (ex, ey) = self.parse_coordinate_pair()?;
        let ctrl_2 = self.make_absolute(c2x, c2y, relative);
        let end = self.make_absolute(ex, ey, relative);
        elements.push(SvgPathElement::CubicCurve(SvgCubicCurve {
            start: self.current, ctrl_1, ctrl_2, end,
        }));
        self.last_control = Some(ctrl_2);
        self.current = end;
        Ok(())
    }

    fn handle_quadratic_to(&mut self, relative: bool, elements: &mut Vec<SvgPathElement>) -> Result<(), SvgPathParseError> {
        let (cx, cy) = self.parse_coordinate_pair()?;
        let (ex, ey) = self.parse_coordinate_pair()?;
        let ctrl = self.make_absolute(cx, cy, relative);
        let end = self.make_absolute(ex, ey, relative);
        elements.push(SvgPathElement::QuadraticCurve(SvgQuadraticCurve {
            start: self.current, ctrl, end,
        }));
        self.last_control = Some(ctrl);
        self.current = end;
        Ok(())
    }

    fn handle_smooth_quadratic_to(&mut self, relative: bool, elements: &mut Vec<SvgPathElement>) -> Result<(), SvgPathParseError> {
        let ctrl = match self.last_control {
            Some(lc) if matches!(self.last_command.to_ascii_uppercase(), b'Q' | b'T') => {
                SvgPoint {
                    x: 2.0 * self.current.x - lc.x,
                    y: 2.0 * self.current.y - lc.y,
                }
            }
            _ => self.current,
        };
        let (ex, ey) = self.parse_coordinate_pair()?;
        let end = self.make_absolute(ex, ey, relative);
        elements.push(SvgPathElement::QuadraticCurve(SvgQuadraticCurve {
            start: self.current, ctrl, end,
        }));
        self.last_control = Some(ctrl);
        self.current = end;
        Ok(())
    }

    fn handle_arc_to(&mut self, relative: bool, elements: &mut Vec<SvgPathElement>) -> Result<(), SvgPathParseError> {
        let rx = self.parse_number()?.abs();
        let ry = self.parse_number()?.abs();
        let x_rotation = self.parse_number()?;
        let large_arc = self.parse_flag()?;
        let sweep = self.parse_flag()?;
        let (ex, ey) = self.parse_coordinate_pair()?;
        let end = self.make_absolute(ex, ey, relative);
        arc_to_cubics(self.current, end, rx, ry, x_rotation, large_arc, sweep, elements);
        self.current = end;
        self.last_control = None;
        Ok(())
    }
}

/// Parse an SVG path `d` attribute string into a `SvgMultiPolygon`.
///
/// Each M/m command starts a new subpath (ring). All 14 SVG path commands are
/// supported including arcs (converted to cubic beziers).
#[must_use]
pub fn parse_svg_path_d(d: &str) -> Result<SvgMultiPolygon, SvgPathParseError> {
    let d = d.trim();
    if d.is_empty() {
        return Err(SvgPathParseError::EmptyPath);
    }

    let mut parser = PathParser::new(d.as_bytes());
    let mut rings: Vec<SvgPath> = Vec::new();
    let mut current_elements: Vec<SvgPathElement> = Vec::new();

    parser.skip_whitespace();

    while !parser.at_end() {
        parser.skip_whitespace_and_commas();
        if parser.at_end() {
            break;
        }

        let b = parser.peek().unwrap();

        // Determine if this is a command letter or an implicit repeat
        let cmd = if b.is_ascii_alphabetic() {
            parser.pos += 1;
            b
        } else if parser.last_command != 0 {
            // Implicit repeat: after M/m, implicit commands become L/l
            match parser.last_command {
                b'M' => b'L',
                b'm' => b'l',
                other => other,
            }
        } else {
            return Err(SvgPathParseError::UnexpectedChar {
                pos: parser.pos,
                ch: b as char,
            });
        };

        let relative = cmd.is_ascii_lowercase();
        let cmd_upper = cmd.to_ascii_uppercase();

        match cmd_upper {
            b'M' => {
                // Flush current subpath
                if !current_elements.is_empty() {
                    rings.push(SvgPath {
                        items: SvgPathElementVec::from_vec(core::mem::take(&mut current_elements)),
                    });
                }
                let (x, y) = parser.parse_coordinate_pair()?;
                let pt = parser.make_absolute(x, y, relative);
                parser.current = pt;
                parser.subpath_start = pt;
                parser.last_control = None;
                parser.last_command = cmd;
            }
            b'L' => { parser.handle_line_to(relative, &mut current_elements)?; parser.last_command = cmd; }
            b'H' => { parser.handle_horizontal_to(relative, &mut current_elements)?; parser.last_command = cmd; }
            b'V' => { parser.handle_vertical_to(relative, &mut current_elements)?; parser.last_command = cmd; }
            b'C' => { parser.handle_cubic_to(relative, &mut current_elements)?; parser.last_command = cmd; }
            b'S' => { parser.handle_smooth_cubic_to(relative, &mut current_elements)?; parser.last_command = cmd; }
            b'Q' => { parser.handle_quadratic_to(relative, &mut current_elements)?; parser.last_command = cmd; }
            b'T' => { parser.handle_smooth_quadratic_to(relative, &mut current_elements)?; parser.last_command = cmd; }
            b'A' => { parser.handle_arc_to(relative, &mut current_elements)?; parser.last_command = cmd; }
            b'Z' => {
                // Close subpath
                let dx = parser.current.x - parser.subpath_start.x;
                let dy = parser.current.y - parser.subpath_start.y;
                if dx * dx + dy * dy > CLOSEPATH_EPSILON * CLOSEPATH_EPSILON {
                    current_elements.push(SvgPathElement::Line(SvgLine {
                        start: parser.current,
                        end: parser.subpath_start,
                    }));
                }
                parser.current = parser.subpath_start;
                parser.last_control = None;
                parser.last_command = cmd;

                // Flush current subpath
                if !current_elements.is_empty() {
                    rings.push(SvgPath {
                        items: SvgPathElementVec::from_vec(core::mem::take(&mut current_elements)),
                    });
                }
            }
            _ => {
                return Err(SvgPathParseError::UnexpectedChar {
                    pos: parser.pos - 1,
                    ch: cmd as char,
                });
            }
        }

        // After processing one argument group, try to consume more
        // argument groups for the same command (implicit repeats)
        if cmd_upper != b'M' && cmd_upper != b'Z' {
            loop {
                parser.skip_whitespace_and_commas();
                if parser.at_end() {
                    break;
                }
                let next = parser.peek().unwrap();
                if next.is_ascii_alphabetic() {
                    break; // Next command letter
                }
                if !parser.has_number() {
                    break;
                }

                // Implicit repeat of the same command
                match cmd_upper {
                    b'L' => parser.handle_line_to(relative, &mut current_elements)?,
                    b'H' => parser.handle_horizontal_to(relative, &mut current_elements)?,
                    b'V' => parser.handle_vertical_to(relative, &mut current_elements)?,
                    b'C' => parser.handle_cubic_to(relative, &mut current_elements)?,
                    b'S' => parser.handle_smooth_cubic_to(relative, &mut current_elements)?,
                    b'Q' => parser.handle_quadratic_to(relative, &mut current_elements)?,
                    b'T' => parser.handle_smooth_quadratic_to(relative, &mut current_elements)?,
                    b'A' => parser.handle_arc_to(relative, &mut current_elements)?,
                    _ => break,
                }
            }
        }
    }

    // Flush any remaining elements
    if !current_elements.is_empty() {
        rings.push(SvgPath {
            items: SvgPathElementVec::from_vec(current_elements),
        });
    }

    Ok(SvgMultiPolygon {
        rings: SvgPathVec::from_vec(rings),
    })
}

/// Convert an SVG arc to 1–4 cubic bezier curves.
///
/// Implements the SVG spec arc endpoint-to-center parameterization (Appendix F.6).
fn arc_to_cubics(
    start: SvgPoint,
    end: SvgPoint,
    mut rx: f32,
    mut ry: f32,
    x_rotation_deg: f32,
    large_arc: bool,
    sweep: bool,
    out: &mut Vec<SvgPathElement>,
) {
    // Degenerate cases
    if (start.x - end.x).abs() < POINT_EPSILON && (start.y - end.y).abs() < POINT_EPSILON {
        return;
    }
    if rx < POINT_EPSILON || ry < POINT_EPSILON {
        out.push(SvgPathElement::Line(SvgLine { start, end }));
        return;
    }

    let phi = x_rotation_deg.to_radians();
    let cos_phi = phi.cos();
    let sin_phi = phi.sin();

    // Step 1: Compute (x1', y1')
    let dx = (start.x - end.x) / 2.0;
    let dy = (start.y - end.y) / 2.0;
    let x1p = cos_phi * dx + sin_phi * dy;
    let y1p = -sin_phi * dx + cos_phi * dy;

    // Step 2: Compute (cx', cy') - correct radii if too small
    let x1p2 = x1p * x1p;
    let y1p2 = y1p * y1p;
    let mut rx2 = rx * rx;
    let mut ry2 = ry * ry;

    let lambda = x1p2 / rx2 + y1p2 / ry2;
    if lambda > 1.0 {
        let sqrt_lambda = lambda.sqrt();
        rx *= sqrt_lambda;
        ry *= sqrt_lambda;
        rx2 = rx * rx;
        ry2 = ry * ry;
    }

    let num = (rx2 * ry2 - rx2 * y1p2 - ry2 * x1p2).max(0.0);
    let den = rx2 * y1p2 + ry2 * x1p2;
    let sq = if den > 0.0 {
        (num / den).sqrt()
    } else {
        0.0
    };

    let sign = if large_arc == sweep { -1.0 } else { 1.0 };
    let cxp = sign * sq * (rx * y1p / ry);
    let cyp = sign * sq * -(ry * x1p / rx);

    // Step 3: Compute (cx, cy) from (cx', cy')
    let mx = (start.x + end.x) / 2.0;
    let my = (start.y + end.y) / 2.0;
    let cx = cos_phi * cxp - sin_phi * cyp + mx;
    let cy = sin_phi * cxp + cos_phi * cyp + my;

    // Step 4: Compute theta1 and dtheta
    let theta1 = angle_between(1.0, 0.0, (x1p - cxp) / rx, (y1p - cyp) / ry);
    let mut dtheta = angle_between(
        (x1p - cxp) / rx,
        (y1p - cyp) / ry,
        (-x1p - cxp) / rx,
        (-y1p - cyp) / ry,
    );

    if !sweep && dtheta > 0.0 {
        dtheta -= core::f32::consts::TAU;
    } else if sweep && dtheta < 0.0 {
        dtheta += core::f32::consts::TAU;
    }

    // Split into segments of at most PI/2
    let n_segs = (dtheta.abs() / (core::f32::consts::FRAC_PI_2 + ARC_SPLIT_FUDGE)).ceil() as usize;
    let n_segs = n_segs.max(1);
    let seg_angle = dtheta / n_segs as f32;

    let mut prev = start;
    for i in 0..n_segs {
        let t1 = theta1 + seg_angle * i as f32;
        let t2 = theta1 + seg_angle * (i + 1) as f32;

        let (c1, c2, ep) =
            arc_segment_to_cubic(cx, cy, rx, ry, cos_phi, sin_phi, t1, t2);

        let seg_end = if i + 1 == n_segs { end } else { ep };
        out.push(SvgPathElement::CubicCurve(SvgCubicCurve {
            start: prev,
            ctrl_1: c1,
            ctrl_2: c2,
            end: seg_end,
        }));
        prev = seg_end;
    }
}

/// Compute the angle between two vectors.
fn angle_between(ux: f32, uy: f32, vx: f32, vy: f32) -> f32 {
    let dot = ux * vx + uy * vy;
    let len = ((ux * ux + uy * uy) * (vx * vx + vy * vy)).sqrt();
    if len < ZERO_LENGTH_EPSILON {
        return 0.0;
    }
    let cos_val = (dot / len).clamp(-1.0, 1.0);
    let angle = cos_val.acos();
    if ux * vy - uy * vx < 0.0 {
        -angle
    } else {
        angle
    }
}

/// Convert a single arc segment (<=90 degrees) to a cubic bezier.
fn arc_segment_to_cubic(
    cx: f32,
    cy: f32,
    rx: f32,
    ry: f32,
    cos_phi: f32,
    sin_phi: f32,
    theta1: f32,
    theta2: f32,
) -> (SvgPoint, SvgPoint, SvgPoint) {
    let alpha = 4.0 / 3.0 * ((theta2 - theta1) / 4.0).tan();

    let cos1 = theta1.cos();
    let sin1 = theta1.sin();
    let cos2 = theta2.cos();
    let sin2 = theta2.sin();

    // Control point 1 (relative to unit circle)
    let dx1 = rx * (cos1 - alpha * sin1);
    let dy1 = ry * (sin1 + alpha * cos1);
    // Control point 2
    let dx2 = rx * (cos2 + alpha * sin2);
    let dy2 = ry * (sin2 - alpha * cos2);
    // End point
    let dx3 = rx * cos2;
    let dy3 = ry * sin2;

    let c1 = SvgPoint {
        x: cos_phi * dx1 - sin_phi * dy1 + cx,
        y: sin_phi * dx1 + cos_phi * dy1 + cy,
    };
    let c2 = SvgPoint {
        x: cos_phi * dx2 - sin_phi * dy2 + cx,
        y: sin_phi * dx2 + cos_phi * dy2 + cy,
    };
    let ep = SvgPoint {
        x: cos_phi * dx3 - sin_phi * dy3 + cx,
        y: sin_phi * dx3 + cos_phi * dy3 + cy,
    };

    (c1, c2, ep)
}

/// Approximate a circle with 4 cubic bezier curves.
///
/// Uses the standard kappa constant (0.5522847498) for quarter-arc approximation.
#[must_use]
pub fn svg_circle_to_paths(cx: f32, cy: f32, r: f32) -> SvgPath {
    let k = r * KAPPA;

    let elements = vec![
        // Top to right
        SvgPathElement::CubicCurve(SvgCubicCurve {
            start: SvgPoint { x: cx, y: cy - r },
            ctrl_1: SvgPoint {
                x: cx + k,
                y: cy - r,
            },
            ctrl_2: SvgPoint {
                x: cx + r,
                y: cy - k,
            },
            end: SvgPoint { x: cx + r, y: cy },
        }),
        // Right to bottom
        SvgPathElement::CubicCurve(SvgCubicCurve {
            start: SvgPoint { x: cx + r, y: cy },
            ctrl_1: SvgPoint {
                x: cx + r,
                y: cy + k,
            },
            ctrl_2: SvgPoint {
                x: cx + k,
                y: cy + r,
            },
            end: SvgPoint { x: cx, y: cy + r },
        }),
        // Bottom to left
        SvgPathElement::CubicCurve(SvgCubicCurve {
            start: SvgPoint { x: cx, y: cy + r },
            ctrl_1: SvgPoint {
                x: cx - k,
                y: cy + r,
            },
            ctrl_2: SvgPoint {
                x: cx - r,
                y: cy + k,
            },
            end: SvgPoint { x: cx - r, y: cy },
        }),
        // Left to top
        SvgPathElement::CubicCurve(SvgCubicCurve {
            start: SvgPoint { x: cx - r, y: cy },
            ctrl_1: SvgPoint {
                x: cx - r,
                y: cy - k,
            },
            ctrl_2: SvgPoint {
                x: cx - k,
                y: cy - r,
            },
            end: SvgPoint { x: cx, y: cy - r },
        }),
    ];

    SvgPath {
        items: SvgPathElementVec::from_vec(elements),
    }
}

/// Convert an SVG `<rect>` to a path with optional rounded corners.
///
/// If `rx` and `ry` are both 0, produces 4 line segments.
/// Otherwise, produces lines for straight edges and cubic curves for corners.
#[must_use]
pub fn svg_rect_to_path(x: f32, y: f32, w: f32, h: f32, rx: f32, ry: f32) -> SvgPath {
    let rx = rx.min(w / 2.0);
    let ry = ry.min(h / 2.0);

    if rx < CLOSEPATH_EPSILON && ry < CLOSEPATH_EPSILON {
        // Simple rectangle: 4 lines
        let tl = SvgPoint { x, y };
        let tr = SvgPoint { x: x + w, y };
        let br = SvgPoint { x: x + w, y: y + h };
        let bl = SvgPoint { x, y: y + h };

        let elements = vec![
            SvgPathElement::Line(SvgLine { start: tl, end: tr }),
            SvgPathElement::Line(SvgLine { start: tr, end: br }),
            SvgPathElement::Line(SvgLine {
                start: br,
                end: bl,
            }),
            SvgPathElement::Line(SvgLine { start: bl, end: tl }),
        ];

        return SvgPath {
            items: SvgPathElementVec::from_vec(elements),
        };
    }

    // Rounded rectangle
    let kx = rx * KAPPA;
    let ky = ry * KAPPA;

    let mut elements = Vec::with_capacity(8);

    // Top edge (left to right)
    elements.push(SvgPathElement::Line(SvgLine {
        start: SvgPoint { x: x + rx, y },
        end: SvgPoint { x: x + w - rx, y },
    }));
    // Top-right corner
    elements.push(SvgPathElement::CubicCurve(SvgCubicCurve {
        start: SvgPoint { x: x + w - rx, y },
        ctrl_1: SvgPoint {
            x: x + w - rx + kx,
            y,
        },
        ctrl_2: SvgPoint {
            x: x + w,
            y: y + ry - ky,
        },
        end: SvgPoint {
            x: x + w,
            y: y + ry,
        },
    }));
    // Right edge
    elements.push(SvgPathElement::Line(SvgLine {
        start: SvgPoint {
            x: x + w,
            y: y + ry,
        },
        end: SvgPoint {
            x: x + w,
            y: y + h - ry,
        },
    }));
    // Bottom-right corner
    elements.push(SvgPathElement::CubicCurve(SvgCubicCurve {
        start: SvgPoint {
            x: x + w,
            y: y + h - ry,
        },
        ctrl_1: SvgPoint {
            x: x + w,
            y: y + h - ry + ky,
        },
        ctrl_2: SvgPoint {
            x: x + w - rx + kx,
            y: y + h,
        },
        end: SvgPoint {
            x: x + w - rx,
            y: y + h,
        },
    }));
    // Bottom edge (right to left)
    elements.push(SvgPathElement::Line(SvgLine {
        start: SvgPoint {
            x: x + w - rx,
            y: y + h,
        },
        end: SvgPoint { x: x + rx, y: y + h },
    }));
    // Bottom-left corner
    elements.push(SvgPathElement::CubicCurve(SvgCubicCurve {
        start: SvgPoint { x: x + rx, y: y + h },
        ctrl_1: SvgPoint {
            x: x + rx - kx,
            y: y + h,
        },
        ctrl_2: SvgPoint {
            x,
            y: y + h - ry + ky,
        },
        end: SvgPoint { x, y: y + h - ry },
    }));
    // Left edge
    elements.push(SvgPathElement::Line(SvgLine {
        start: SvgPoint { x, y: y + h - ry },
        end: SvgPoint { x, y: y + ry },
    }));
    // Top-left corner
    elements.push(SvgPathElement::CubicCurve(SvgCubicCurve {
        start: SvgPoint { x, y: y + ry },
        ctrl_1: SvgPoint {
            x,
            y: y + ry - ky,
        },
        ctrl_2: SvgPoint {
            x: x + rx - kx,
            y,
        },
        end: SvgPoint { x: x + rx, y },
    }));

    SvgPath {
        items: SvgPathElementVec::from_vec(elements),
    }
}
