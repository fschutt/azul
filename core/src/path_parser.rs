//! SVG `d=""` path data parser.
//!
//! Parses the `d` attribute of SVG `<path>` elements into `SvgMultiPolygon`
//! geometry, supporting all 14 SVG path commands (M/m, L/l, H/h, V/v,
//! C/c, S/s, Q/q, T/t, A/a, Z/z).

use alloc::{string::String, vec::Vec};
use azul_css::props::basic::{SvgCubicCurve, SvgPoint, SvgQuadraticCurve};

use crate::svg::{SvgLine, SvgMultiPolygon, SvgPath, SvgPathElement, SvgPathElementVec, SvgPathVec};

/// Bezier approximation constant for quarter-circle arcs.
const KAPPA: f32 = 0.552_284_8;

/// Tolerance for treating two points as coincident (used in closepath and arc degeneracy checks).
const POINT_EPSILON: f32 = 1e-6;

/// Tolerance for snapping a closepath line (slightly larger to avoid micro-segments).
const CLOSEPATH_EPSILON: f32 = 0.001;

/// Tolerance for treating a vector length as zero in angle computation.
const ZERO_LENGTH_EPSILON: f32 = 1e-10;

/// Small offset added to PI/2 when splitting arcs to avoid exact-boundary floating-point issues.
const ARC_SPLIT_FUDGE: f32 = 0.001;

/// Decode the UTF-8 character starting at `pos` in `input`.
///
/// `input` is always the byte view of a valid `&str` and `pos` is always at a
/// char boundary (only whole ASCII tokens are consumed), so the UTF-8 decode
/// succeeds; a corrupt offset falls back to the replacement character rather
/// than panicking. Used so error messages report the real Unicode char instead
/// of a Latin-1 reinterpretation of a single UTF-8 byte (`b as char`).
fn char_at(input: &[u8], pos: usize) -> char {
    input
        .get(pos..)
        .and_then(|rest| core::str::from_utf8(rest).ok())
        .and_then(|s| s.chars().next())
        .unwrap_or(char::REPLACEMENT_CHARACTER)
}

/// Errors that can occur during SVG path parsing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
                write!(f, "unexpected char '{ch}' at byte {pos}")
            }
            Self::ExpectedNumber { pos } => write!(f, "expected number at byte {pos}"),
            Self::InvalidArcFlag { pos } => write!(f, "invalid arc flag at byte {pos}"),
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
    const fn new(input: &'a [u8]) -> Self {
        Self {
            input,
            pos: 0,
            current: SvgPoint { x: 0.0, y: 0.0 },
            subpath_start: SvgPoint { x: 0.0, y: 0.0 },
            last_control: None,
            last_command: 0,
        }
    }

    const fn at_end(&self) -> bool {
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
            Some(b'+' | b'-' | b'.') => true,
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
        if self.input.get(self.pos) == Some(&b'.') {
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

    #[allow(clippy::similar_names)] // domain-standard coordinate/control-point names
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

    #[allow(clippy::suboptimal_flops)] // mul_add not guaranteed faster/available without target +fma; keep explicit a*b+c
    #[allow(clippy::similar_names)] // domain-standard coordinate/control-point names
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

    #[allow(clippy::suboptimal_flops)] // mul_add not guaranteed faster/available without target +fma; keep explicit a*b+c
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
///
/// # Panics
///
/// Panics if the path tokenizer signals a command but then yields no token
/// (an internal parser invariant that should not occur for any input).
#[allow(clippy::suboptimal_flops)] // mul_add not guaranteed faster/available without target +fma; keep explicit a*b+c
#[allow(clippy::too_many_lines)] // large but cohesive: single-purpose parser/builder/dispatch (one branch per input variant)
/// # Errors
///
/// Returns an error if `d` is not a valid SVG path-data string.
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
                // AUDIT 2026-07-08: a `Z`/`z` closepath takes no arguments, so it
                // cannot be implicitly repeated. Reaching here means a stray
                // non-command byte followed a closepath (e.g. the `5` in "M0 0Z5").
                // The old `other => other` fell through to the `Z` arm, which
                // consumes zero bytes, so `pos` never advanced -> 100% CPU infinite
                // loop. Reject it as an unexpected character instead.
                b'Z' | b'z' => {
                    return Err(SvgPathParseError::UnexpectedChar {
                        pos: parser.pos,
                        ch: char_at(parser.input, parser.pos),
                    });
                }
                other => other,
            }
        } else {
            return Err(SvgPathParseError::UnexpectedChar {
                pos: parser.pos,
                ch: char_at(parser.input, parser.pos),
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
            b'L' => {
                parser.handle_line_to(relative, &mut current_elements)?;
                parser.last_command = cmd;
            }
            b'H' => {
                parser.handle_horizontal_to(relative, &mut current_elements)?;
                parser.last_command = cmd;
            }
            b'V' => {
                parser.handle_vertical_to(relative, &mut current_elements)?;
                parser.last_command = cmd;
            }
            b'C' => {
                parser.handle_cubic_to(relative, &mut current_elements)?;
                parser.last_command = cmd;
            }
            b'S' => {
                parser.handle_smooth_cubic_to(relative, &mut current_elements)?;
                parser.last_command = cmd;
            }
            b'Q' => {
                parser.handle_quadratic_to(relative, &mut current_elements)?;
                parser.last_command = cmd;
            }
            b'T' => {
                parser.handle_smooth_quadratic_to(relative, &mut current_elements)?;
                parser.last_command = cmd;
            }
            b'A' => {
                parser.handle_arc_to(relative, &mut current_elements)?;
                parser.last_command = cmd;
            }
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

    // A `d` made up solely of comma/whitespace filler (e.g. ",") consumes to EOF
    // without ever reading a command and used to be accepted as an empty Ok. The SVG
    // path grammar requires a moveto to start; a bare separator is only valid BETWEEN
    // commands, never as the whole string.
    if parser.last_command == 0 && rings.is_empty() {
        return Err(SvgPathParseError::UnexpectedChar {
            pos: 0,
            ch: char_at(parser.input, 0),
        });
    }

    Ok(SvgMultiPolygon {
        rings: SvgPathVec::from_vec(rings),
    })
}

/// Convert an SVG arc to 1–4 cubic bezier curves.
///
/// Implements the SVG spec arc endpoint-to-center parameterization (Appendix F.6).
#[allow(clippy::suboptimal_flops)] // mul_add not guaranteed faster/available without target +fma; keep explicit a*b+c
// n_segs is a tiny arc-quadrant count (<= ~6) and its loop index; the float<->usize
// casts are exact for these bounded values.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]
#[allow(clippy::similar_names)] // domain-standard coordinate/control-point names
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
    let mx = f32::midpoint(start.x, end.x);
    let my = f32::midpoint(start.y, end.y);
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
#[allow(clippy::suboptimal_flops)] // mul_add not guaranteed faster/available without target +fma; keep explicit a*b+c
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
#[allow(clippy::suboptimal_flops)] // mul_add not guaranteed faster/available without target +fma; keep explicit a*b+c
#[allow(clippy::similar_names)] // domain-standard coordinate/control-point names
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
// builds the rounded-rect path segment-by-segment with a matching capacity hint;
// a `vec![..]` literal of the 8 multi-line elements would be less readable here.
#[allow(clippy::vec_init_then_push)]
#[allow(clippy::too_many_lines)] // large but cohesive: single-purpose parser/builder/dispatch (one branch per input variant)
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

#[cfg(test)]
mod tests {
    use super::*;

    /// AUDIT 2026-07-08 regression: `"M0 0Z5"` used to spin at 100% CPU forever
    /// because the trailing `5` re-derived `cmd = Z` (zero-length consume) and
    /// the cursor never advanced. It must now terminate with `UnexpectedChar`.
    #[test]
    fn m0_0z5_does_not_hang() {
        let err = parse_svg_path_d("M0 0Z5").unwrap_err();
        match err {
            SvgPathParseError::UnexpectedChar { ch, .. } => assert_eq!(ch, '5'),
            other => panic!("expected UnexpectedChar, got {other:?}"),
        }
    }

    /// Any digit or symbol directly after a closepath is rejected, not looped on.
    #[test]
    fn stray_byte_after_closepath_rejected() {
        for s in ["M0 0Z9", "m0 0z-", "M0 0Z."] {
            assert!(
                matches!(
                    parse_svg_path_d(s),
                    Err(SvgPathParseError::UnexpectedChar { .. })
                ),
                "expected UnexpectedChar for {s:?}"
            );
        }
    }

    /// A leading non-command byte reports the real Unicode char, not a Latin-1
    /// reinterpretation of a single UTF-8 byte (the old `b as char`).
    #[test]
    fn error_char_is_unicode_not_byte() {
        // 'ü' is two UTF-8 bytes; `b as char` would have yielded a mojibake char.
        let err = parse_svg_path_d("ü10 10").unwrap_err();
        match err {
            SvgPathParseError::UnexpectedChar { ch, pos } => {
                assert_eq!(ch, 'ü');
                assert_eq!(pos, 0);
            }
            other => panic!("expected UnexpectedChar, got {other:?}"),
        }
    }

    /// A well-formed closepath followed by a real command still parses.
    #[test]
    fn valid_closepath_then_command_ok() {
        let parsed = parse_svg_path_d("M0 0 L10 0 Z M20 20 L30 20 Z");
        assert!(parsed.is_ok(), "valid multi-subpath path should parse");
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)] // exact float equality is the point: the parser propagates values bit-for-bit
mod autotest_generated {
    use alloc::format;

    use super::*;

    // ---------------------------------------------------------------- helpers

    fn approx(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    /// Every point produced by a path, in element order.
    fn all_points(path: &SvgPath) -> Vec<SvgPoint> {
        let mut out = Vec::new();
        for e in path.items.as_ref() {
            match e {
                SvgPathElement::Line(l) => {
                    out.push(l.start);
                    out.push(l.end);
                }
                SvgPathElement::QuadraticCurve(q) => {
                    out.push(q.start);
                    out.push(q.ctrl);
                    out.push(q.end);
                }
                SvgPathElement::CubicCurve(c) => {
                    out.push(c.start);
                    out.push(c.ctrl_1);
                    out.push(c.ctrl_2);
                    out.push(c.end);
                }
            }
        }
        out
    }

    /// Each element's end must be the next element's start (the parser threads
    /// `self.current` through every handler, so this holds bit-for-bit).
    fn assert_contiguous(items: &[SvgPathElement], what: &str) {
        for w in items.windows(2) {
            assert_eq!(
                w[0].get_end(),
                w[1].get_start(),
                "{what}: element chain is not contiguous"
            );
        }
    }

    // ======================================================= char_at (numeric)

    #[test]
    fn char_at_zero_and_ascii() {
        // "M0 0" is ['M', '0', ' ', '0'] -- the space is at index 2, not 3.
        assert_eq!(char_at(b"M0 0", 0), 'M');
        assert_eq!(char_at(b"M0 0", 2), ' ');
        assert_eq!(char_at(b"M0 0", 3), '0');
    }

    #[test]
    fn char_at_empty_input_is_replacement() {
        assert_eq!(char_at(b"", 0), char::REPLACEMENT_CHARACTER);
    }

    #[test]
    fn char_at_past_end_is_replacement_not_panic() {
        assert_eq!(char_at(b"abc", 3), char::REPLACEMENT_CHARACTER);
        assert_eq!(char_at(b"abc", 4), char::REPLACEMENT_CHARACTER);
    }

    /// `pos = usize::MAX` must be a `get(pos..)` miss, not an arithmetic panic.
    #[test]
    fn char_at_usize_max_is_replacement() {
        assert_eq!(char_at(b"abc", usize::MAX), char::REPLACEMENT_CHARACTER);
        assert_eq!(char_at(b"", usize::MAX), char::REPLACEMENT_CHARACTER);
    }

    /// The whole point of `char_at`: report the real char, not one UTF-8 byte.
    #[test]
    fn char_at_decodes_multibyte_not_latin1() {
        assert_eq!(char_at("ü".as_bytes(), 0), 'ü');
        assert_eq!(char_at("€".as_bytes(), 0), '€');
        assert_eq!(char_at("\u{1F600}".as_bytes(), 0), '\u{1F600}');
    }

    /// A corrupt (mid-codepoint) offset falls back rather than panicking.
    #[test]
    fn char_at_mid_codepoint_offset_is_replacement() {
        let bytes = "😀".as_bytes(); // 4 bytes
        for pos in 1..bytes.len() {
            assert_eq!(
                char_at(bytes, pos),
                char::REPLACEMENT_CHARACTER,
                "mid-codepoint offset {pos} must not panic"
            );
        }
    }

    /// Trailing garbage after a valid char makes the *whole rest* invalid UTF-8,
    /// so even a valid leading char decodes to the replacement char. Pinned as
    /// deterministic (never a panic).
    #[test]
    fn char_at_invalid_utf8_tail_is_replacement() {
        assert_eq!(char_at(&[b'A', 0xFF], 0), char::REPLACEMENT_CHARACTER);
        assert_eq!(char_at(&[0xFF], 0), char::REPLACEMENT_CHARACTER);
        // ...but a clean tail after the char still decodes.
        assert_eq!(char_at(b"AB", 0), 'A');
    }

    // ============================================ SvgPathParseError (serializer)

    #[test]
    fn error_display_is_non_empty_for_every_variant() {
        let variants = [
            SvgPathParseError::EmptyPath,
            SvgPathParseError::UnexpectedChar { pos: 0, ch: 'x' },
            SvgPathParseError::ExpectedNumber { pos: 0 },
            SvgPathParseError::InvalidArcFlag { pos: 0 },
        ];
        for v in variants {
            let s = format!("{v}");
            assert!(!s.is_empty(), "Display for {v:?} must not be empty");
            assert!(!format!("{v:?}").is_empty(), "Debug must not be empty");
        }
    }

    #[test]
    fn error_display_edge_values_do_not_panic() {
        let s = format!(
            "{}",
            SvgPathParseError::UnexpectedChar {
                pos: usize::MAX,
                ch: char::REPLACEMENT_CHARACTER,
            }
        );
        assert!(s.contains(&format!("{}", usize::MAX)));
        assert!(s.contains(char::REPLACEMENT_CHARACTER));

        // NUL, an emoji and a combining mark all format without panicking.
        for ch in ['\0', '\u{1F600}', '\u{0301}'] {
            let s = format!("{}", SvgPathParseError::UnexpectedChar { pos: 0, ch });
            assert!(!s.is_empty());
        }
        assert!(!format!("{}", SvgPathParseError::ExpectedNumber { pos: usize::MAX }).is_empty());
        assert!(!format!("{}", SvgPathParseError::InvalidArcFlag { pos: usize::MAX }).is_empty());
    }

    // ================================================= PathParser::new / getters

    #[test]
    fn parser_new_invariants_hold() {
        let p = PathParser::new(b"M0 0");
        assert_eq!(p.pos, 0);
        assert_eq!(p.current, SvgPoint { x: 0.0, y: 0.0 });
        assert_eq!(p.subpath_start, SvgPoint { x: 0.0, y: 0.0 });
        assert!(p.last_control.is_none());
        assert_eq!(p.last_command, 0);
        assert_eq!(p.input.len(), 4);
    }

    #[test]
    fn parser_new_on_empty_input_does_not_panic() {
        let p = PathParser::new(b"");
        assert!(p.at_end(), "empty input is immediately at_end");
        assert_eq!(p.peek(), None);
        assert!(!p.has_number());
    }

    #[test]
    fn at_end_and_peek_agree_across_the_whole_input() {
        let mut p = PathParser::new(b"ab");
        assert!(!p.at_end());
        assert_eq!(p.peek(), Some(b'a'));
        p.pos = 1;
        assert!(!p.at_end());
        assert_eq!(p.peek(), Some(b'b'));
        p.pos = 2;
        assert!(p.at_end());
        assert_eq!(p.peek(), None);
    }

    /// A cursor pushed far past the end must report `at_end` / `None`, never panic.
    #[test]
    fn peek_and_at_end_at_extreme_positions() {
        let mut p = PathParser::new(b"abc");
        p.pos = usize::MAX;
        assert!(p.at_end());
        assert_eq!(p.peek(), None);
        assert!(!p.has_number());
    }

    // ============================================================ skip_* (other)

    #[test]
    fn skip_whitespace_and_commas_consumes_all_separators() {
        let mut p = PathParser::new(b" \t\r\n,,, \tX");
        p.skip_whitespace_and_commas();
        assert_eq!(p.peek(), Some(b'X'));
    }

    /// `skip_whitespace` must *not* eat commas (they are only argument separators).
    #[test]
    fn skip_whitespace_stops_at_comma() {
        let mut p = PathParser::new(b"  ,1");
        p.skip_whitespace();
        assert_eq!(p.peek(), Some(b','));
        assert_eq!(p.pos, 2);
    }

    #[test]
    fn skip_on_empty_and_all_separator_input_terminates() {
        let mut p = PathParser::new(b"");
        p.skip_whitespace();
        p.skip_whitespace_and_commas();
        assert!(p.at_end());

        let all_ws = " \t\r\n,".repeat(20_000);
        let mut p = PathParser::new(all_ws.as_bytes());
        p.skip_whitespace_and_commas();
        assert!(p.at_end(), "a huge run of separators must be fully consumed");
        assert_eq!(p.pos, all_ws.len());
    }

    /// Non-separator input leaves the cursor exactly where it was.
    #[test]
    fn skip_is_a_no_op_on_non_separator() {
        let mut p = PathParser::new("😀".as_bytes());
        p.skip_whitespace_and_commas();
        assert_eq!(p.pos, 0);
        // Form feed / vertical tab are NOT SVG wsp; the parser must leave them.
        let mut p = PathParser::new(b"\x0c\x0b1");
        p.skip_whitespace();
        assert_eq!(p.pos, 0);
    }

    // ======================================================= has_number (predicate)

    #[test]
    fn has_number_true_for_number_starters() {
        for s in ["0", "9", "+", "-", ".", "5.5", "-.5"] {
            assert!(
                PathParser::new(s.as_bytes()).has_number(),
                "{s:?} should look like a number start"
            );
        }
    }

    #[test]
    fn has_number_false_for_non_number_starters() {
        // Note 'e'/'E' only appear *inside* a number, never at its start.
        for s in ["", " ", ",", "M", "z", "e", "E", "😀", "\u{0301}"] {
            assert!(
                !PathParser::new(s.as_bytes()).has_number(),
                "{s:?} should not look like a number start"
            );
        }
    }

    // ========================================================= parse_number (parser)

    fn num(s: &str) -> Result<f32, SvgPathParseError> {
        PathParser::new(s.as_bytes()).parse_number()
    }

    #[test]
    fn parse_number_valid_minimal() {
        assert_eq!(num("0").unwrap(), 0.0);
        assert_eq!(num("5").unwrap(), 5.0);
        assert_eq!(num("+5").unwrap(), 5.0);
        assert_eq!(num("-5").unwrap(), -5.0);
        assert!(approx(num("12.34").unwrap(), 12.34));
        assert!(approx(num(".5").unwrap(), 0.5));
        assert_eq!(num("5.").unwrap(), 5.0);
        assert!(approx(num("1e2").unwrap(), 100.0));
        assert!(approx(num("1E-2").unwrap(), 0.01));
        assert!(approx(num("  ,, 7").unwrap(), 7.0), "leading separators skipped");
    }

    #[test]
    fn parse_number_empty_input_is_err() {
        assert_eq!(num(""), Err(SvgPathParseError::ExpectedNumber { pos: 0 }));
    }

    /// Whitespace-only input reports the position *after* the skipped separators.
    #[test]
    fn parse_number_whitespace_only_is_err() {
        assert_eq!(num("   "), Err(SvgPathParseError::ExpectedNumber { pos: 3 }));
        assert_eq!(num("\t\n"), Err(SvgPathParseError::ExpectedNumber { pos: 2 }));
        assert_eq!(num(" , "), Err(SvgPathParseError::ExpectedNumber { pos: 3 }));
    }

    #[test]
    fn parse_number_garbage_is_err_never_panics() {
        for s in ["abc", "@", "#$%", "-", "+", ".", "-.", "+.", "e5", "NaN", "inf", "-inf"] {
            assert!(
                matches!(num(s), Err(SvgPathParseError::ExpectedNumber { .. })),
                "{s:?} must be rejected, got {:?}",
                num(s)
            );
        }
    }

    /// A dangling exponent is consumed by the tokenizer but rejected by `f32::from_str`.
    #[test]
    fn parse_number_dangling_exponent_is_err() {
        for s in ["1e", "1E", "1e+", "1e-", "1.5e"] {
            assert!(
                matches!(num(s), Err(SvgPathParseError::ExpectedNumber { pos: 0 })),
                "{s:?} must be rejected"
            );
        }
    }

    #[test]
    fn parse_number_unicode_does_not_panic() {
        for s in ["😀", "\u{0301}", "ü", "€1", "１"] {
            assert!(num(s).is_err(), "{s:?} must be rejected");
        }
        // A number immediately followed by a multibyte char stops at the boundary.
        let mut p = PathParser::new("1😀".as_bytes());
        assert_eq!(p.parse_number().unwrap(), 1.0);
        assert_eq!(p.pos, 1);
    }

    /// Boundary numerics: overflow saturates to +/-inf, underflow flushes to zero,
    /// and `-0` keeps its sign. None of these panic.
    #[test]
    fn parse_number_boundary_values_saturate() {
        assert!(num("-0").unwrap().is_sign_negative(), "-0 keeps its sign bit");
        assert_eq!(num("-0").unwrap(), -0.0);

        assert!(num("1e999").unwrap().is_infinite());
        assert!(num("1e999").unwrap().is_sign_positive());
        assert!(num("-1e999").unwrap().is_infinite());
        assert!(num("-1e999").unwrap().is_sign_negative());

        assert_eq!(num("1e-999").unwrap(), 0.0, "underflow flushes to zero");

        // i64::MAX / f32 extremes round to a finite f32.
        assert!(num("9223372036854775807").unwrap().is_finite());
        assert!(num("340282350000000000000000000000000000000").unwrap().is_finite());
        // Just past f32::MAX -> +inf, not a panic.
        assert!(num("1e39").unwrap().is_infinite());
    }

    /// A 20k-digit literal must not hang or panic; it saturates to +inf.
    #[test]
    fn parse_number_extremely_long_input_terminates() {
        let huge = "9".repeat(20_000);
        assert!(num(&huge).unwrap().is_infinite());

        let long_frac = format!("0.{}", "0".repeat(20_000));
        assert_eq!(num(&long_frac).unwrap(), 0.0);

        let long_zeros = format!("{}1", "0".repeat(20_000));
        assert_eq!(num(&long_zeros).unwrap(), 1.0);
    }

    /// Trailing junk is left on the cursor rather than swallowed: `"1.2.3"` yields
    /// `1.2` and stops at the second dot (SVG's own "1.5.5" == two numbers rule).
    #[test]
    fn parse_number_stops_at_trailing_junk() {
        let mut p = PathParser::new(b"1.2.3");
        assert!(approx(p.parse_number().unwrap(), 1.2));
        assert_eq!(p.pos, 3, "second '.' must not be consumed");

        let mut p = PathParser::new(b"5;garbage");
        assert_eq!(p.parse_number().unwrap(), 5.0);
        assert_eq!(p.peek(), Some(b';'));
    }

    /// The cursor is never advanced past the end of the input, whatever happens.
    #[test]
    fn parse_number_never_overruns_the_buffer() {
        for s in ["", "-", ".", "1e", "1e+", "1.", "+.e", "1e-", "999"] {
            let mut p = PathParser::new(s.as_bytes());
            let _ = p.parse_number();
            assert!(p.pos <= s.len(), "{s:?}: pos {} > len {}", p.pos, s.len());
        }
    }

    // =========================================================== parse_flag (parser)

    fn flag(s: &str) -> Result<bool, SvgPathParseError> {
        PathParser::new(s.as_bytes()).parse_flag()
    }

    #[test]
    fn parse_flag_valid_minimal() {
        assert!(!flag("0").unwrap());
        assert!(flag("1").unwrap());
        assert!(flag("  , 1").unwrap(), "separators are skipped first");
    }

    /// A flag is exactly one byte: "11" is two flags, not the number eleven.
    #[test]
    fn parse_flag_consumes_exactly_one_byte() {
        let mut p = PathParser::new(b"10");
        assert!(p.parse_flag().unwrap());
        assert_eq!(p.pos, 1);
        assert!(!p.parse_flag().unwrap());
        assert_eq!(p.pos, 2);
    }

    #[test]
    fn parse_flag_empty_and_whitespace_only_are_err() {
        assert_eq!(flag(""), Err(SvgPathParseError::InvalidArcFlag { pos: 0 }));
        assert_eq!(flag("   "), Err(SvgPathParseError::InvalidArcFlag { pos: 3 }));
    }

    /// Any byte other than `0`/`1` is rejected and the cursor stays put.
    #[test]
    fn parse_flag_garbage_is_err_and_does_not_advance() {
        for s in ["2", "9", "-1", "+1", "x", ".", "😀", "0.5"] {
            let mut p = PathParser::new(s.as_bytes());
            let before = p.pos;
            match p.parse_flag() {
                Err(SvgPathParseError::InvalidArcFlag { pos }) => {
                    assert_eq!(pos, p.pos, "{s:?}: reported pos must be the cursor");
                    assert_eq!(p.pos, before, "{s:?}: rejected flag must not advance");
                }
                // "0.5" legitimately parses its leading '0' as the flag.
                Ok(v) => assert!(s == "0.5" && !v, "{s:?} unexpectedly parsed as {v}"),
                other => panic!("{s:?}: unexpected {other:?}"),
            }
        }
    }

    /// A 100k-byte separator run followed by no flag terminates with an error.
    #[test]
    fn parse_flag_extremely_long_separator_run_terminates() {
        let s = " ".repeat(100_000);
        assert_eq!(
            flag(&s),
            Err(SvgPathParseError::InvalidArcFlag { pos: 100_000 })
        );
    }

    // ================================================ parse_coordinate_pair (parser)

    fn pair(s: &str) -> Result<(f32, f32), SvgPathParseError> {
        PathParser::new(s.as_bytes()).parse_coordinate_pair()
    }

    #[test]
    fn parse_coordinate_pair_valid_minimal() {
        assert_eq!(pair("1 2").unwrap(), (1.0, 2.0));
        assert_eq!(pair("1,2").unwrap(), (1.0, 2.0));
        assert_eq!(pair(" 1 , 2 ").unwrap(), (1.0, 2.0));
        // SVG allows a sign to act as the separator.
        assert_eq!(pair("-1-2").unwrap(), (-1.0, -2.0));
    }

    /// SVG's notorious "1.5.5" == (1.5, 0.5) tokenization.
    #[test]
    fn parse_coordinate_pair_splits_on_second_dot() {
        let (x, y) = pair("1.5.5").unwrap();
        assert!(approx(x, 1.5) && approx(y, 0.5), "got ({x}, {y})");
    }

    #[test]
    fn parse_coordinate_pair_empty_and_partial_are_err() {
        assert!(pair("").is_err());
        assert!(pair("   ").is_err());
        assert!(pair("1").is_err(), "a lone x with no y must be rejected");
        assert!(pair("1 ").is_err());
        assert!(pair("1,").is_err());
    }

    #[test]
    fn parse_coordinate_pair_garbage_and_unicode_are_err() {
        for s in ["abc", "1 abc", "😀 1", "1 😀", ";;", "1;2"] {
            assert!(pair(s).is_err(), "{s:?} must be rejected");
        }
    }

    #[test]
    fn parse_coordinate_pair_boundary_values() {
        let (x, y) = pair("1e999 -1e999").unwrap();
        assert!(x.is_infinite() && x.is_sign_positive());
        assert!(y.is_infinite() && y.is_sign_negative());

        let (x, y) = pair("-0 0").unwrap();
        assert!(x.is_sign_negative() && y.is_sign_positive());
    }

    #[test]
    fn parse_coordinate_pair_extremely_long_input_terminates() {
        let s = format!("{} {}", "9".repeat(10_000), "9".repeat(10_000));
        let (x, y) = pair(&s).unwrap();
        assert!(x.is_infinite() && y.is_infinite());
    }

    // ======================================================= make_absolute (numeric)

    #[test]
    fn make_absolute_zero_and_absolute_mode_is_identity() {
        let mut p = PathParser::new(b"");
        p.current = SvgPoint { x: 7.0, y: -3.0 };
        // Absolute: current is ignored entirely.
        assert_eq!(p.make_absolute(0.0, 0.0, false), SvgPoint { x: 0.0, y: 0.0 });
        assert_eq!(p.make_absolute(1.0, 2.0, false), SvgPoint { x: 1.0, y: 2.0 });
        // Relative: offsets from current.
        assert_eq!(p.make_absolute(0.0, 0.0, true), SvgPoint { x: 7.0, y: -3.0 });
        assert_eq!(p.make_absolute(-7.0, 3.0, true), SvgPoint { x: 0.0, y: 0.0 });
    }

    #[test]
    fn make_absolute_negative_inputs() {
        let mut p = PathParser::new(b"");
        p.current = SvgPoint { x: -10.0, y: -10.0 };
        assert_eq!(p.make_absolute(-5.0, -5.0, true), SvgPoint { x: -15.0, y: -15.0 });
        assert_eq!(p.make_absolute(-5.0, -5.0, false), SvgPoint { x: -5.0, y: -5.0 });
    }

    /// f32 addition saturates to infinity; it never wraps or debug-panics.
    #[test]
    fn make_absolute_overflow_saturates_to_infinity() {
        let mut p = PathParser::new(b"");
        p.current = SvgPoint {
            x: f32::MAX,
            y: f32::MIN,
        };
        let r = p.make_absolute(f32::MAX, f32::MIN, true);
        assert!(r.x.is_infinite() && r.x.is_sign_positive());
        assert!(r.y.is_infinite() && r.y.is_sign_negative());
    }

    #[test]
    fn make_absolute_nan_and_inf_are_defined_not_panics() {
        let mut p = PathParser::new(b"");
        p.current = SvgPoint {
            x: f32::INFINITY,
            y: 0.0,
        };
        // inf + (-inf) is NaN by IEEE-754 -- defined, not a panic.
        let r = p.make_absolute(f32::NEG_INFINITY, f32::NAN, true);
        assert!(r.x.is_nan(), "inf + -inf must be NaN");
        assert!(r.y.is_nan());

        // Absolute mode passes NaN straight through.
        let r = p.make_absolute(f32::NAN, f32::INFINITY, false);
        assert!(r.x.is_nan());
        assert!(r.y.is_infinite());
    }

    // ============================================================ handle_* (other)

    /// Drive one handler over `input` starting from `current`, returning the
    /// pushed elements plus the parser's post-state.
    fn run_handler<F>(
        input: &str,
        current: SvgPoint,
        last_command: u8,
        last_control: Option<SvgPoint>,
        f: F,
    ) -> (Result<(), SvgPathParseError>, Vec<SvgPathElement>, SvgPoint)
    where
        F: FnOnce(&mut PathParser<'_>, &mut Vec<SvgPathElement>) -> Result<(), SvgPathParseError>,
    {
        let mut p = PathParser::new(input.as_bytes());
        p.current = current;
        p.last_command = last_command;
        p.last_control = last_control;
        let mut els = Vec::new();
        let r = f(&mut p, &mut els);
        (r, els, p.current)
    }

    const ORIGIN: SvgPoint = SvgPoint { x: 0.0, y: 0.0 };

    #[test]
    fn handle_line_to_absolute_and_relative() {
        let start = SvgPoint { x: 10.0, y: 10.0 };
        let (r, els, cur) = run_handler("5 5", start, b'L', None, |p, e| p.handle_line_to(false, e));
        assert!(r.is_ok());
        assert_eq!(els.len(), 1);
        assert_eq!(els[0].get_start(), start);
        assert_eq!(els[0].get_end(), SvgPoint { x: 5.0, y: 5.0 });
        assert_eq!(cur, SvgPoint { x: 5.0, y: 5.0 });

        let (r, els, cur) = run_handler("5 5", start, b'l', None, |p, e| p.handle_line_to(true, e));
        assert!(r.is_ok());
        assert_eq!(els[0].get_end(), SvgPoint { x: 15.0, y: 15.0 });
        assert_eq!(cur, SvgPoint { x: 15.0, y: 15.0 });
    }

    /// H keeps y, V keeps x -- including when the incoming coordinate is infinite.
    #[test]
    fn handle_horizontal_and_vertical_preserve_the_other_axis() {
        let start = SvgPoint { x: 3.0, y: 4.0 };
        let (_, els, _) = run_handler("9", start, b'H', None, |p, e| p.handle_horizontal_to(false, e));
        assert_eq!(els[0].get_end(), SvgPoint { x: 9.0, y: 4.0 });

        let (_, els, _) = run_handler("9", start, b'V', None, |p, e| p.handle_vertical_to(false, e));
        assert_eq!(els[0].get_end(), SvgPoint { x: 3.0, y: 9.0 });

        let (_, els, _) = run_handler("1e999", start, b'h', None, |p, e| p.handle_horizontal_to(true, e));
        let end = els[0].get_end();
        assert!(end.x.is_infinite(), "relative H by +inf saturates");
        assert_eq!(end.y, 4.0, "y must be untouched");
    }

    #[test]
    fn handlers_reject_empty_and_garbage_input_without_panicking() {
        for input in ["", "   ", "abc", "😀", ";", "1"] {
            // Each handler needs >= 1 number; "1" is enough only for H/V.
            let (r, _, _) = run_handler(input, ORIGIN, 0, None, |p, e| p.handle_line_to(false, e));
            assert!(r.is_err(), "line_to({input:?}) must be Err");

            let (r, _, _) = run_handler(input, ORIGIN, 0, None, |p, e| p.handle_cubic_to(false, e));
            assert!(r.is_err(), "cubic_to({input:?}) must be Err");

            let (r, _, _) = run_handler(input, ORIGIN, 0, None, |p, e| p.handle_quadratic_to(false, e));
            assert!(r.is_err(), "quadratic_to({input:?}) must be Err");

            let (r, _, _) = run_handler(input, ORIGIN, 0, None, |p, e| p.handle_arc_to(false, e));
            assert!(r.is_err(), "arc_to({input:?}) must be Err");

            if input != "1" {
                let (r, _, _) =
                    run_handler(input, ORIGIN, 0, None, |p, e| p.handle_horizontal_to(false, e));
                assert!(r.is_err(), "horizontal_to({input:?}) must be Err");
                let (r, _, _) =
                    run_handler(input, ORIGIN, 0, None, |p, e| p.handle_vertical_to(false, e));
                assert!(r.is_err(), "vertical_to({input:?}) must be Err");
            }
        }
    }

    #[test]
    fn handle_cubic_to_records_second_control_point() {
        let (r, els, _) = run_handler("1 1 2 2 3 3", ORIGIN, b'C', None, |p, e| {
            p.handle_cubic_to(false, e)
        });
        assert!(r.is_ok());
        match els[0] {
            SvgPathElement::CubicCurve(c) => {
                assert_eq!(c.start, ORIGIN);
                assert_eq!(c.ctrl_1, SvgPoint { x: 1.0, y: 1.0 });
                assert_eq!(c.ctrl_2, SvgPoint { x: 2.0, y: 2.0 });
                assert_eq!(c.end, SvgPoint { x: 3.0, y: 3.0 });
            }
            other => panic!("expected CubicCurve, got {other:?}"),
        }
    }

    /// S reflects the previous control point only when the previous command was
    /// C or S; otherwise ctrl_1 collapses onto the current point.
    #[test]
    fn handle_smooth_cubic_reflects_only_after_c_or_s() {
        let cur = SvgPoint { x: 10.0, y: 10.0 };
        let lc = Some(SvgPoint { x: 8.0, y: 6.0 });

        let (_, els, _) = run_handler("1 1 2 2", cur, b'C', lc, |p, e| p.handle_smooth_cubic_to(false, e));
        match els[0] {
            // reflection of (8,6) about (10,10) == (12,14)
            SvgPathElement::CubicCurve(c) => assert_eq!(c.ctrl_1, SvgPoint { x: 12.0, y: 14.0 }),
            other => panic!("expected CubicCurve, got {other:?}"),
        }

        // Previous command was L: no reflection, ctrl_1 == current.
        let (_, els, _) = run_handler("1 1 2 2", cur, b'L', lc, |p, e| p.handle_smooth_cubic_to(false, e));
        match els[0] {
            SvgPathElement::CubicCurve(c) => assert_eq!(c.ctrl_1, cur),
            other => panic!("expected CubicCurve, got {other:?}"),
        }

        // No stored control point at all: no reflection either.
        let (_, els, _) = run_handler("1 1 2 2", cur, b'S', None, |p, e| p.handle_smooth_cubic_to(false, e));
        match els[0] {
            SvgPathElement::CubicCurve(c) => assert_eq!(c.ctrl_1, cur),
            other => panic!("expected CubicCurve, got {other:?}"),
        }
    }

    #[test]
    fn handle_smooth_quadratic_reflects_only_after_q_or_t() {
        let cur = SvgPoint { x: 10.0, y: 10.0 };
        let lc = Some(SvgPoint { x: 8.0, y: 6.0 });

        let (_, els, _) = run_handler("2 2", cur, b'Q', lc, |p, e| p.handle_smooth_quadratic_to(false, e));
        match els[0] {
            SvgPathElement::QuadraticCurve(q) => assert_eq!(q.ctrl, SvgPoint { x: 12.0, y: 14.0 }),
            other => panic!("expected QuadraticCurve, got {other:?}"),
        }

        let (_, els, _) = run_handler("2 2", cur, b'M', lc, |p, e| p.handle_smooth_quadratic_to(false, e));
        match els[0] {
            SvgPathElement::QuadraticCurve(q) => assert_eq!(q.ctrl, cur),
            other => panic!("expected QuadraticCurve, got {other:?}"),
        }
    }

    /// Arc radii are absolute-valued, so a negative radius still draws an arc.
    #[test]
    fn handle_arc_to_takes_abs_of_radii() {
        let (r, els, cur) = run_handler("-5 -5 0 0 1 10 0", ORIGIN, b'A', None, |p, e| {
            p.handle_arc_to(false, e)
        });
        assert!(r.is_ok());
        assert!(!els.is_empty(), "negative radii must still produce an arc");
        assert!(
            els.iter().all(|e| matches!(e, SvgPathElement::CubicCurve(_))),
            "abs() of the radii keeps this a real arc, not a line fallback"
        );
        assert_eq!(cur, SvgPoint { x: 10.0, y: 0.0 });
    }

    /// A zero radius degenerates to a straight line (SVG spec F.6.6).
    #[test]
    fn handle_arc_to_zero_radius_degenerates_to_line() {
        let (r, els, _) = run_handler("0 0 0 0 1 10 0", ORIGIN, b'A', None, |p, e| {
            p.handle_arc_to(false, e)
        });
        assert!(r.is_ok());
        assert_eq!(els.len(), 1);
        assert!(matches!(els[0], SvgPathElement::Line(_)));
        assert_eq!(els[0].get_end(), SvgPoint { x: 10.0, y: 0.0 });
    }

    #[test]
    fn handle_arc_to_rejects_out_of_range_flags() {
        for input in ["5 5 0 2 1 10 0", "5 5 0 1 2 10 0", "5 5 0 x 1 10 0", "5 5 0"] {
            let (r, _, _) = run_handler(input, ORIGIN, b'A', None, |p, e| p.handle_arc_to(false, e));
            assert!(r.is_err(), "arc flags in {input:?} must be rejected");
        }
        let (r, _, _) = run_handler("5 5 0 2 1 10 0", ORIGIN, b'A', None, |p, e| {
            p.handle_arc_to(false, e)
        });
        assert!(matches!(r, Err(SvgPathParseError::InvalidArcFlag { .. })));
    }

    /// Infinite radii feed NaN through the endpoint parameterization. The
    /// segment count must still be bounded (no runaway loop) and no panic.
    #[test]
    fn handle_arc_to_infinite_radii_is_bounded() {
        let (r, els, _) = run_handler("1e999 1e999 0 0 1 10 10", ORIGIN, b'A', None, |p, e| {
            p.handle_arc_to(false, e)
        });
        assert!(r.is_ok());
        assert!(
            els.len() <= 4,
            "a single arc must never expand past 4 cubics, got {}",
            els.len()
        );
    }

    // ===================================================== parse_svg_path_d (parser)

    #[test]
    fn parse_path_empty_and_whitespace_only_is_empty_path_err() {
        for s in ["", "   ", "\t\n", "\r\n  \t"] {
            assert_eq!(
                parse_svg_path_d(s),
                Err(SvgPathParseError::EmptyPath),
                "{s:?} must be EmptyPath"
            );
        }
    }

    #[test]
    fn parse_path_valid_minimal() {
        let mp = parse_svg_path_d("M10 20 L30 40").unwrap();
        let rings = mp.rings.as_ref();
        assert_eq!(rings.len(), 1);
        let items = rings[0].items.as_ref();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].get_start(), SvgPoint { x: 10.0, y: 20.0 });
        assert_eq!(items[0].get_end(), SvgPoint { x: 30.0, y: 40.0 });
    }

    /// Relative commands accumulate from the current point.
    #[test]
    fn parse_path_relative_accumulates() {
        let mp = parse_svg_path_d("m10 20 l30 40").unwrap();
        let items_owner = &mp.rings.as_ref()[0];
        let items = items_owner.items.as_ref();
        assert_eq!(items[0].get_start(), SvgPoint { x: 10.0, y: 20.0 });
        assert_eq!(items[0].get_end(), SvgPoint { x: 40.0, y: 60.0 });
    }

    /// A moveto with no drawing commands produces no geometry (but is not an error).
    #[test]
    fn parse_path_moveto_only_yields_no_rings() {
        let mp = parse_svg_path_d("M10 10").unwrap();
        assert_eq!(mp.rings.as_ref().len(), 0);
    }

    /// Extra coordinate pairs after an M are implicit linetos (SVG spec).
    #[test]
    fn parse_path_implicit_lineto_after_moveto() {
        let mp = parse_svg_path_d("M0 0 10 0 20 0").unwrap();
        let items_owner = &mp.rings.as_ref()[0];
        let items = items_owner.items.as_ref();
        assert_eq!(items.len(), 2, "two implicit L commands");
        assert_eq!(items[0].get_end(), SvgPoint { x: 10.0, y: 0.0 });
        assert_eq!(items[1].get_end(), SvgPoint { x: 20.0, y: 0.0 });
    }

    #[test]
    fn parse_path_garbage_is_err_never_panics() {
        for s in ["@#$", "hello", "?", "-", ".", ",", "0 0", "5", ";;;", "\u{0}"] {
            assert!(parse_svg_path_d(s).is_err(), "{s:?} must be rejected");
        }
    }

    /// A leading non-command byte is reported as a real Unicode char at byte 0.
    #[test]
    fn parse_path_unicode_does_not_panic() {
        assert_eq!(
            parse_svg_path_d("\u{1F600}"),
            Err(SvgPathParseError::UnexpectedChar {
                pos: 0,
                ch: '\u{1F600}',
            })
        );
        assert_eq!(
            parse_svg_path_d("\u{0301}M0 0"),
            Err(SvgPathParseError::UnexpectedChar {
                pos: 0,
                ch: '\u{0301}',
            })
        );
        // A multibyte char *after* a valid command falls into the argument
        // parser and is rejected as a missing number, at the right byte offset.
        assert_eq!(
            parse_svg_path_d("M0 0L1 1ü"),
            Err(SvgPathParseError::ExpectedNumber { pos: 8 })
        );
    }

    /// Trailing junk after a valid prefix is rejected deterministically.
    #[test]
    fn parse_path_trailing_junk_is_rejected() {
        assert!(parse_svg_path_d("M0 0 L1 1;garbage").is_err());
        assert!(parse_svg_path_d("M0 0 L").is_err(), "command with no args");
        assert!(parse_svg_path_d("M0 0 L1").is_err(), "half a coordinate pair");
        assert!(parse_svg_path_d("M0 0 X10 10").is_err(), "unknown command letter");
        // Surrounding whitespace is trimmed, not rejected.
        assert!(parse_svg_path_d("  \n M0 0 L1 1 \t ").is_ok());
    }

    /// An unknown ASCII command letter reports its own offset.
    #[test]
    fn parse_path_unknown_command_reports_its_offset() {
        assert_eq!(
            parse_svg_path_d("M0 0 X10 10"),
            Err(SvgPathParseError::UnexpectedChar { pos: 5, ch: 'X' })
        );
    }

    /// Closepath only emits a joining line when the gap exceeds CLOSEPATH_EPSILON.
    #[test]
    fn parse_path_closepath_epsilon_boundary() {
        // Well above the epsilon: a closing line is added.
        let mp = parse_svg_path_d("M0 0 L1 0 Z").unwrap();
        assert_eq!(mp.rings.as_ref()[0].items.as_ref().len(), 2);

        // Exactly at the epsilon (dx*dx == eps*eps, and the test is strictly `>`):
        // no closing line.
        let mp = parse_svg_path_d("M0 0 L0.001 0 Z").unwrap();
        assert_eq!(mp.rings.as_ref()[0].items.as_ref().len(), 1);

        // Below the epsilon: no closing line.
        let mp = parse_svg_path_d("M0 0 L0.0005 0 Z").unwrap();
        assert_eq!(mp.rings.as_ref()[0].items.as_ref().len(), 1);

        // Degenerate closepath on an empty subpath yields no rings at all.
        assert_eq!(parse_svg_path_d("M0 0 Z").unwrap().rings.as_ref().len(), 0);
    }

    /// Every M and every Z flushes a ring.
    #[test]
    fn parse_path_multiple_subpaths_produce_multiple_rings() {
        let mp = parse_svg_path_d("M0 0 L10 0 Z M20 20 L30 20 Z M40 40 L50 40").unwrap();
        assert_eq!(mp.rings.as_ref().len(), 3);
    }

    /// Structural invariant: within a ring, each element's end is the next
    /// element's start -- for every command type, including arcs and closepath.
    #[test]
    fn parse_path_rings_are_contiguous_chains() {
        let d = "M0 0 L10 0 H20 V10 C25 15 30 20 35 20 S45 25 50 20 \
                 Q55 15 60 20 T70 20 A5 5 0 1 1 80 30 Z \
                 m100 100 l10 0 z";
        let mp = parse_svg_path_d(d).unwrap();
        assert!(mp.rings.as_ref().len() >= 2);
        for (i, ring) in mp.rings.as_ref().iter().enumerate() {
            assert_contiguous(ring.items.as_ref(), &format!("ring {i}"));
            assert!(!ring.items.as_ref().is_empty(), "ring {i} must not be empty");
        }
    }

    /// A closed ring ends where it started.
    #[test]
    fn parse_path_closed_ring_returns_to_subpath_start() {
        let mp = parse_svg_path_d("M0 0 L10 0 L10 10 Z").unwrap();
        let ring = &mp.rings.as_ref()[0];
        let items = ring.items.as_ref();
        assert_eq!(items.last().unwrap().get_end(), SvgPoint { x: 0.0, y: 0.0 });
        assert_eq!(items.first().unwrap().get_start(), SvgPoint { x: 0.0, y: 0.0 });
    }

    /// Boundary numerics survive the full parse: coordinates saturate to inf
    /// rather than panicking or wrapping.
    #[test]
    fn parse_path_boundary_numbers_saturate() {
        let mp = parse_svg_path_d("M1e999 -1e999 L1e-999 0").unwrap();
        let items_owner = &mp.rings.as_ref()[0];
        let start = items_owner.items.as_ref()[0].get_start();
        assert!(start.x.is_infinite() && start.x.is_sign_positive());
        assert!(start.y.is_infinite() && start.y.is_sign_negative());

        // f32::MAX-ish coordinates with a *relative* lineto overflow to +inf.
        let mp = parse_svg_path_d("M3.4e38 0 l3.4e38 0").unwrap();
        let items_owner = &mp.rings.as_ref()[0];
        assert!(items_owner.items.as_ref()[0].get_end().x.is_infinite());

        // "NaN" / "inf" are not valid SVG numbers -- they must be rejected,
        // so a NaN coordinate can never enter the geometry via the parser.
        assert!(parse_svg_path_d("M NaN 0").is_err());
        assert!(parse_svg_path_d("M inf 0").is_err());
    }

    /// 5000 implicit repeats: the parser is iterative, so this must neither
    /// hang nor blow the stack.
    #[test]
    fn parse_path_extremely_long_input_terminates() {
        let mut d = String::from("M0 0");
        for _ in 0..5_000 {
            d.push_str(" L1 1");
        }
        let mp = parse_svg_path_d(&d).unwrap();
        let items_owner = &mp.rings.as_ref()[0];
        assert_eq!(items_owner.items.as_ref().len(), 5_000);

        // 5000 subpaths -> 5000 rings, still iterative.
        let mut d = String::new();
        for _ in 0..5_000 {
            d.push_str("M0 0 L1 1 Z ");
        }
        assert_eq!(parse_svg_path_d(&d).unwrap().rings.as_ref().len(), 5_000);
    }

    /// A long run of adversarial fragments: every one must *return* (Ok or Err)
    /// and never spin. The `Z`-followed-by-a-digit case used to loop forever.
    #[test]
    fn parse_path_adversarial_fragments_all_terminate() {
        let fragments = [
            "Z", "z", "ZZZ", "M0 0ZZ", "M0 0Z0", "M0 0zZ5", "M0 0Z Z Z",
            "M", "M0", "M0 0 C", "M0 0 A", "M0 0 A1", "M0 0 A1 1 0 0 0 0",
            "M0 0 S", "M0 0 T", "M0 0 H", "M0 0 V", "M0 0 Q1",
            "M0 0 L1 1 1", "M0 0 L1 1 1 1 1", "M0 0 A0 0 0 0 0 0 0",
            "M0 0 l-.5-.5-.5-.5", "M0 0 t1 1 2 2", "M0 0 s1 1 2 2 3 3 4 4",
            "M0,0,1,1", "M0 0e", "M0 0 1e1e1", "M.5.5.5.5",
            "M0 0 A1 1 0 11 10 10", "M0 0 A1 1 0 1 1 10 10 A1 1 0 0 0 0 0",
            "M0 0 h1e999 v1e999 h-1e999", "M-0-0-0-0",
        ];
        for f in fragments {
            // The assertion is termination itself; the result is merely pinned
            // as "did not panic".
            let r = parse_svg_path_d(f);
            assert!(r.is_ok() || r.is_err(), "{f:?} must return, not panic");
        }
    }

    /// All 14 commands round-trip through the tokenizer into geometry of the
    /// expected kind.
    #[test]
    fn parse_path_every_command_produces_its_element_kind() {
        let cases: [(&str, usize); 10] = [
            ("M0 0 L1 1", 1),
            ("M0 0 l1 1", 1),
            ("M0 0 H1", 1),
            ("M0 0 V1", 1),
            ("M0 0 C1 1 2 2 3 3", 1),
            ("M0 0 S1 1 2 2", 1),
            ("M0 0 Q1 1 2 2", 1),
            ("M0 0 T1 1", 1),
            ("M0 0 L1 0 Z", 2), // the Z adds the closing line
            ("M0 0 A5 5 0 0 1 10 0", 2), // a half-turn arc splits into 2 cubics
        ];
        for (d, expected) in cases {
            let mp = parse_svg_path_d(d).unwrap_or_else(|e| panic!("{d:?} failed: {e:?}"));
            let rings = mp.rings.as_ref();
            assert_eq!(rings.len(), 1, "{d:?}");
            assert_eq!(rings[0].items.as_ref().len(), expected, "{d:?}");
        }
    }

    // ======================================================= arc_to_cubics (numeric)

    #[test]
    fn arc_to_cubics_coincident_endpoints_emit_nothing() {
        let mut out = Vec::new();
        arc_to_cubics(ORIGIN, ORIGIN, 5.0, 5.0, 0.0, true, true, &mut out);
        assert!(out.is_empty(), "a zero-length arc is dropped per SVG F.6.2");

        // Within POINT_EPSILON also counts as coincident.
        let near = SvgPoint { x: 1e-9, y: 1e-9 };
        arc_to_cubics(ORIGIN, near, 5.0, 5.0, 0.0, false, false, &mut out);
        assert!(out.is_empty());
    }

    #[test]
    fn arc_to_cubics_zero_radius_emits_a_line() {
        let end = SvgPoint { x: 10.0, y: 10.0 };
        for (rx, ry) in [(0.0, 5.0), (5.0, 0.0), (0.0, 0.0)] {
            let mut out = Vec::new();
            arc_to_cubics(ORIGIN, end, rx, ry, 0.0, false, true, &mut out);
            assert_eq!(out.len(), 1, "rx={rx} ry={ry}");
            assert!(matches!(out[0], SvgPathElement::Line(_)));
            assert_eq!(out[0].get_start(), ORIGIN);
            assert_eq!(out[0].get_end(), end);
        }
    }

    /// For every flag combination, the arc is 1..=4 cubics that start exactly at
    /// `start` and end exactly at `end` (the endpoint is snapped, not computed).
    #[test]
    fn arc_to_cubics_endpoints_are_exact_for_all_flag_combos() {
        let start = SvgPoint { x: 0.0, y: 0.0 };
        let end = SvgPoint { x: 10.0, y: 10.0 };
        for large_arc in [false, true] {
            for sweep in [false, true] {
                let mut out = Vec::new();
                arc_to_cubics(start, end, 8.0, 6.0, 30.0, large_arc, sweep, &mut out);
                assert!(
                    (1..=4).contains(&out.len()),
                    "large_arc={large_arc} sweep={sweep}: got {} cubics",
                    out.len()
                );
                assert_eq!(out[0].get_start(), start);
                assert_eq!(out.last().unwrap().get_end(), end);
                assert_contiguous(&out, "arc");
                for p in out.iter().flat_map(|e| [e.get_start(), e.get_end()]) {
                    assert!(p.x.is_finite() && p.y.is_finite(), "arc produced {p:?}");
                }
            }
        }
    }

    /// Radii that are too small to span the endpoints are scaled up (F.6.6 step 3),
    /// so the arc still lands exactly on the endpoint instead of NaN-ing out.
    #[test]
    fn arc_to_cubics_undersized_radii_are_scaled_up() {
        let start = ORIGIN;
        let end = SvgPoint { x: 100.0, y: 0.0 };
        let mut out = Vec::new();
        arc_to_cubics(start, end, 1.0, 1.0, 0.0, false, true, &mut out);
        assert!(!out.is_empty());
        assert_eq!(out.last().unwrap().get_end(), end);
        for p in all_points(&SvgPath {
            items: SvgPathElementVec::from_vec(out),
        }) {
            assert!(p.x.is_finite() && p.y.is_finite(), "scaled arc produced {p:?}");
        }
    }

    /// NaN / infinite radii must not spin the segment loop: `n_segs` comes from a
    /// NaN -> usize cast, which saturates to 0 and is then clamped to 1.
    #[test]
    fn arc_to_cubics_nan_and_inf_inputs_are_bounded() {
        let end = SvgPoint { x: 10.0, y: 10.0 };
        let bad = [
            (f32::NAN, 5.0, 0.0),
            (5.0, f32::NAN, 0.0),
            (f32::INFINITY, f32::INFINITY, 0.0),
            (5.0, 5.0, f32::NAN),
            (5.0, 5.0, f32::INFINITY),
            (f32::MAX, f32::MAX, 360.0),
        ];
        for (rx, ry, rot) in bad {
            let mut out = Vec::new();
            arc_to_cubics(ORIGIN, end, rx, ry, rot, true, false, &mut out);
            assert!(
                out.len() <= 4,
                "rx={rx} ry={ry} rot={rot}: {} elements (segment loop ran away)",
                out.len()
            );
        }
    }

    /// Extreme but finite endpoints do not panic and stay bounded.
    #[test]
    fn arc_to_cubics_extreme_endpoints_do_not_panic() {
        let mut out = Vec::new();
        arc_to_cubics(
            SvgPoint { x: f32::MIN, y: f32::MIN },
            SvgPoint { x: f32::MAX, y: f32::MAX },
            f32::MAX,
            f32::MAX,
            0.0,
            true,
            true,
            &mut out,
        );
        assert!(out.len() <= 4);
    }

    // ======================================================= angle_between (numeric)

    #[test]
    fn angle_between_known_angles() {
        assert!(approx(angle_between(1.0, 0.0, 1.0, 0.0), 0.0));
        assert!(approx(
            angle_between(1.0, 0.0, 0.0, 1.0),
            core::f32::consts::FRAC_PI_2
        ));
        assert!(approx(
            angle_between(1.0, 0.0, 0.0, -1.0),
            -core::f32::consts::FRAC_PI_2
        ));
        assert!(approx(
            angle_between(1.0, 0.0, -1.0, 0.0),
            core::f32::consts::PI
        ));
        // Magnitude is irrelevant -- only direction matters.
        assert!(approx(
            angle_between(100.0, 0.0, 0.0, 0.001),
            core::f32::consts::FRAC_PI_2
        ));
    }

    /// A zero-length (or underflowing) vector short-circuits to 0.0.
    #[test]
    fn angle_between_zero_length_vectors_return_zero() {
        assert_eq!(angle_between(0.0, 0.0, 1.0, 0.0), 0.0);
        assert_eq!(angle_between(1.0, 0.0, 0.0, 0.0), 0.0);
        assert_eq!(angle_between(0.0, 0.0, 0.0, 0.0), 0.0);
        // Denormal-scale vectors: the squared length underflows to 0.
        assert_eq!(angle_between(1e-30, 1e-30, 1e-30, 1e-30), 0.0);
    }

    /// Result is always within [-PI, PI] for finite inputs (the acos argument is
    /// clamped, so rounding can never push it out of the domain).
    #[test]
    fn angle_between_is_always_within_pi_for_finite_inputs() {
        let vals = [-1e30_f32, -3.0, -1.0, -0.0, 0.0, 1.0, 3.0, 1e30];
        for ux in vals {
            for uy in vals {
                for vx in vals {
                    for vy in vals {
                        let a = angle_between(ux, uy, vx, vy);
                        assert!(
                            a.is_nan() || a.abs() <= core::f32::consts::PI + 1e-5,
                            "angle_between({ux},{uy},{vx},{vy}) = {a} is out of range"
                        );
                    }
                }
            }
        }
    }

    /// Antiparallel/parallel unit vectors do not fall out of acos's domain even
    /// when the dot product rounds slightly past +/-1.
    #[test]
    fn angle_between_clamps_the_acos_domain() {
        let a = angle_between(0.1, 0.2, 0.1, 0.2);
        assert!(!a.is_nan(), "parallel vectors must not produce NaN, got {a}");
        assert!(approx(a, 0.0));
        let a = angle_between(0.1, 0.2, -0.1, -0.2);
        assert!(!a.is_nan());
        assert!(approx(a.abs(), core::f32::consts::PI));
    }

    /// NaN / inf inputs produce NaN, not a panic.
    #[test]
    fn angle_between_nan_and_inf_do_not_panic() {
        assert!(angle_between(f32::NAN, 0.0, 1.0, 0.0).is_nan());
        assert!(angle_between(1.0, 0.0, f32::NAN, f32::NAN).is_nan());
        assert!(angle_between(f32::INFINITY, 0.0, f32::INFINITY, 0.0).is_nan());
        assert!(angle_between(f32::INFINITY, 0.0, 1.0, 0.0).is_nan());
    }

    // ================================================ arc_segment_to_cubic (numeric)

    #[test]
    fn arc_segment_to_cubic_quarter_circle() {
        // Unit circle, no rotation, 0 -> PI/2: the endpoint must land on (0, 1).
        let (c1, c2, ep) = arc_segment_to_cubic(
            0.0,
            0.0,
            1.0,
            1.0,
            1.0,
            0.0,
            0.0,
            core::f32::consts::FRAC_PI_2,
        );
        assert!(approx(ep.x, 0.0) && approx(ep.y, 1.0), "ep = {ep:?}");
        // Control points bulge outward by kappa.
        assert!(approx(c1.x, 1.0) && approx(c1.y, KAPPA), "c1 = {c1:?}");
        assert!(approx(c2.x, KAPPA) && approx(c2.y, 1.0), "c2 = {c2:?}");
    }

    /// A zero-width segment collapses every point onto the start of the arc.
    #[test]
    fn arc_segment_to_cubic_zero_sweep_collapses() {
        let (c1, c2, ep) = arc_segment_to_cubic(5.0, 5.0, 2.0, 2.0, 1.0, 0.0, 0.7, 0.7);
        assert!(approx(c1.x, c2.x) && approx(c1.y, c2.y));
        assert!(approx(c2.x, ep.x) && approx(c2.y, ep.y));
        // ...and that point is on the circle around (5,5).
        assert!(approx((ep.x - 5.0).hypot(ep.y - 5.0), 2.0));
    }

    /// Zero radii put all three points at the center.
    #[test]
    fn arc_segment_to_cubic_zero_radius_is_the_center() {
        let (c1, c2, ep) = arc_segment_to_cubic(3.0, 4.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0);
        for p in [c1, c2, ep] {
            assert_eq!(p, SvgPoint { x: 3.0, y: 4.0 });
        }
    }

    /// Rotation is applied via the caller-supplied cos/sin pair.
    #[test]
    fn arc_segment_to_cubic_applies_rotation() {
        // 90-degree rotation (cos=0, sin=1) maps the theta=0 point (rx, 0) to (0, rx).
        let (_, _, ep) = arc_segment_to_cubic(0.0, 0.0, 2.0, 1.0, 0.0, 1.0, 0.0, 0.0);
        assert!(approx(ep.x, 0.0) && approx(ep.y, 2.0), "ep = {ep:?}");
    }

    #[test]
    fn arc_segment_to_cubic_nan_and_inf_do_not_panic() {
        let bad = [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, f32::MAX, f32::MIN];
        for v in bad {
            let (c1, c2, ep) = arc_segment_to_cubic(v, v, v, v, v, v, v, v);
            // The only contract is "returns a defined value without panicking".
            for p in [c1, c2, ep] {
                assert!(p.x.is_nan() || p.x.is_finite() || p.x.is_infinite());
                assert!(p.y.is_nan() || p.y.is_finite() || p.y.is_infinite());
            }
        }
        // A full-circle sweep drives tan(PI/2) to a huge value; still no panic.
        let (c1, _, _) = arc_segment_to_cubic(
            0.0,
            0.0,
            1.0,
            1.0,
            1.0,
            0.0,
            0.0,
            core::f32::consts::TAU,
        );
        assert!(!c1.x.is_nan() || c1.x.is_nan(), "must not panic");
    }

    // =================================================== svg_circle_to_paths (numeric)

    #[test]
    fn circle_has_four_cubics_and_closes_on_itself() {
        let p = svg_circle_to_paths(10.0, 20.0, 5.0);
        let items = p.items.as_ref();
        assert_eq!(items.len(), 4);
        assert!(items.iter().all(|e| matches!(e, SvgPathElement::CubicCurve(_))));
        assert_contiguous(items, "circle");
        assert_eq!(
            items.last().unwrap().get_end(),
            items.first().unwrap().get_start(),
            "the circle must close exactly"
        );
        // The four anchors are the cardinal points.
        assert_eq!(items[0].get_start(), SvgPoint { x: 10.0, y: 15.0 });
        assert_eq!(items[0].get_end(), SvgPoint { x: 15.0, y: 20.0 });
        assert_eq!(items[1].get_end(), SvgPoint { x: 10.0, y: 25.0 });
        assert_eq!(items[2].get_end(), SvgPoint { x: 5.0, y: 20.0 });
    }

    /// r = 0 degenerates to four zero-length curves at the center, not a panic.
    #[test]
    fn circle_zero_radius_collapses_to_the_center() {
        let p = svg_circle_to_paths(3.0, 4.0, 0.0);
        let items = p.items.as_ref();
        assert_eq!(items.len(), 4);
        for pt in all_points(&p) {
            assert_eq!(pt, SvgPoint { x: 3.0, y: 4.0 });
        }
    }

    /// A negative radius mirrors the circle (it is not rejected or abs()'d);
    /// it still yields a closed 4-curve path.
    #[test]
    fn circle_negative_radius_is_mirrored_not_rejected() {
        let p = svg_circle_to_paths(0.0, 0.0, -5.0);
        let items = p.items.as_ref();
        assert_eq!(items.len(), 4);
        assert_contiguous(items, "negative-r circle");
        assert_eq!(items[0].get_start(), SvgPoint { x: 0.0, y: 5.0 });
        for pt in all_points(&p) {
            assert!(pt.x.is_finite() && pt.y.is_finite());
        }
    }

    #[test]
    fn circle_nan_inf_and_max_do_not_panic() {
        for (cx, cy, r) in [
            (f32::NAN, 0.0, 1.0),
            (0.0, 0.0, f32::NAN),
            (0.0, 0.0, f32::INFINITY),
            (f32::MAX, f32::MAX, f32::MAX),
            (f32::MIN, f32::MIN, f32::MIN),
        ] {
            let p = svg_circle_to_paths(cx, cy, r);
            assert_eq!(p.items.as_ref().len(), 4, "cx={cx} cy={cy} r={r}");
        }
        // f32::MAX radius overflows the control points to infinity rather than
        // wrapping.
        let p = svg_circle_to_paths(f32::MAX, 0.0, f32::MAX);
        assert!(all_points(&p).iter().any(|pt| pt.x.is_infinite()));
    }

    // ==================================================== svg_rect_to_path (numeric)

    #[test]
    fn rect_sharp_corners_are_four_lines() {
        let p = svg_rect_to_path(1.0, 2.0, 10.0, 20.0, 0.0, 0.0);
        let items = p.items.as_ref();
        assert_eq!(items.len(), 4);
        assert!(items.iter().all(|e| matches!(e, SvgPathElement::Line(_))));
        assert_contiguous(items, "sharp rect");
        assert_eq!(items[0].get_start(), SvgPoint { x: 1.0, y: 2.0 });
        assert_eq!(items[1].get_start(), SvgPoint { x: 11.0, y: 2.0 });
        assert_eq!(items[2].get_start(), SvgPoint { x: 11.0, y: 22.0 });
        assert_eq!(items[3].get_start(), SvgPoint { x: 1.0, y: 22.0 });
        assert_eq!(
            items.last().unwrap().get_end(),
            items.first().unwrap().get_start(),
            "the rect must close exactly"
        );
    }

    #[test]
    fn rect_rounded_is_eight_alternating_segments_and_closes() {
        let p = svg_rect_to_path(0.0, 0.0, 100.0, 50.0, 10.0, 5.0);
        let items = p.items.as_ref();
        assert_eq!(items.len(), 8);
        for (i, e) in items.iter().enumerate() {
            if i % 2 == 0 {
                assert!(matches!(e, SvgPathElement::Line(_)), "item {i} should be an edge");
            } else {
                assert!(
                    matches!(e, SvgPathElement::CubicCurve(_)),
                    "item {i} should be a corner"
                );
            }
        }
        assert_contiguous(items, "rounded rect");
        assert_eq!(
            items.last().unwrap().get_end(),
            items.first().unwrap().get_start(),
            "the rounded rect must close exactly"
        );
    }

    /// Radii larger than half the rect are clamped to half (SVG spec), so an
    /// over-large rx cannot invert the geometry.
    #[test]
    fn rect_oversized_radii_are_clamped_to_half() {
        let p = svg_rect_to_path(0.0, 0.0, 10.0, 10.0, 1000.0, 1000.0);
        let items = p.items.as_ref();
        assert_eq!(items.len(), 8);
        // rx clamps to 5 => the top edge runs from x+5 to x+w-5, i.e. zero length.
        match items[0] {
            SvgPathElement::Line(l) => {
                assert_eq!(l.start, SvgPoint { x: 5.0, y: 0.0 });
                assert_eq!(l.end, SvgPoint { x: 5.0, y: 0.0 });
            }
            other => panic!("expected Line, got {other:?}"),
        }
        assert_contiguous(items, "clamped rect");
        for pt in all_points(&p) {
            assert!(pt.x.is_finite() && pt.y.is_finite());
            assert!((0.0..=10.0).contains(&pt.x), "x {} escaped the rect", pt.x);
            assert!((0.0..=10.0).contains(&pt.y), "y {} escaped the rect", pt.y);
        }
    }

    /// Only one of rx/ry being zero still takes the rounded path.
    #[test]
    fn rect_single_zero_radius_still_rounds() {
        let p = svg_rect_to_path(0.0, 0.0, 100.0, 100.0, 0.0, 10.0);
        assert_eq!(p.items.as_ref().len(), 8);
        let p = svg_rect_to_path(0.0, 0.0, 100.0, 100.0, 10.0, 0.0);
        assert_eq!(p.items.as_ref().len(), 8);
    }

    /// A negative width drives `rx.min(w / 2.0)` negative, which falls below the
    /// epsilon and takes the sharp-corner branch. Deterministic, no panic.
    #[test]
    fn rect_negative_extent_takes_the_sharp_branch() {
        let p = svg_rect_to_path(0.0, 0.0, -10.0, -10.0, 4.0, 4.0);
        let items = p.items.as_ref();
        assert_eq!(items.len(), 4);
        assert!(items.iter().all(|e| matches!(e, SvgPathElement::Line(_))));
        assert_contiguous(items, "negative rect");
        assert_eq!(items[1].get_start(), SvgPoint { x: -10.0, y: 0.0 });
    }

    /// `f32::min` returns the non-NaN operand, so a NaN radius silently becomes
    /// half the extent -- and every coordinate stays finite.
    #[test]
    fn rect_nan_radius_falls_back_to_half_extent() {
        let p = svg_rect_to_path(0.0, 0.0, 100.0, 100.0, f32::NAN, f32::NAN);
        let items = p.items.as_ref();
        assert_eq!(items.len(), 8, "NaN radii clamp to w/2, h/2 -> rounded path");
        for pt in all_points(&p) {
            assert!(pt.x.is_finite() && pt.y.is_finite(), "NaN leaked into {pt:?}");
        }
        // rx == 50 => the top edge collapses (x+50 .. x+100-50).
        match items[0] {
            SvgPathElement::Line(l) => assert_eq!(l.start, l.end),
            other => panic!("expected Line, got {other:?}"),
        }
    }

    /// A NaN extent cannot be repaired -- but it must still return a well-formed
    /// path rather than panicking.
    #[test]
    fn rect_nan_extent_is_deterministic() {
        let p = svg_rect_to_path(0.0, 0.0, f32::NAN, f32::NAN, 0.0, 0.0);
        // rx = 0.min(NaN) = 0 -> sharp branch.
        assert_eq!(p.items.as_ref().len(), 4);
        let p = svg_rect_to_path(f32::NAN, f32::NAN, 10.0, 10.0, 0.0, 0.0);
        assert_eq!(p.items.as_ref().len(), 4);
    }

    #[test]
    fn rect_inf_and_max_extents_do_not_panic() {
        for (x, y, w, h, rx, ry) in [
            (0.0, 0.0, f32::INFINITY, f32::INFINITY, 0.0, 0.0),
            (0.0, 0.0, f32::MAX, f32::MAX, 0.0, 0.0),
            (f32::MIN, f32::MIN, f32::MAX, f32::MAX, f32::MAX, f32::MAX),
            (0.0, 0.0, f32::INFINITY, f32::INFINITY, f32::INFINITY, f32::INFINITY),
        ] {
            let p = svg_rect_to_path(x, y, w, h, rx, ry);
            let n = p.items.as_ref().len();
            assert!(n == 4 || n == 8, "w={w} h={h} rx={rx} ry={ry}: got {n} items");
        }
    }
}
