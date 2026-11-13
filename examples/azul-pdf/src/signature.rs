//! A signature: drawn on a pad (strokes of points), kept as an SVG of paths (in the keyring,
//! for the next document), placed on a page by a click, and drawn into the saved copy as paths
//! (azul's `PdfStamp`: the engine reads the SVG's `<path>`s and flattens them onto the page).

/// How wide a placed signature is (points); its height follows its drawing's shape.
pub const SIGNATURE_WIDTH_PT: f32 = 150.0;

/// A point closer than this to the last one (pad px) adds nothing to a stroke.
const MIN_STEP_PX: f32 = 1.0;

/// The pad: its size (px) and what was drawn on it.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Pad {
    width: f32,
    height: f32,
    strokes: Vec<Vec<(f32, f32)>>,
    /// A stroke is being drawn (the button is down).
    drawing: bool,
}

impl Pad {
    #[must_use]
    pub fn new(width: f32, height: f32) -> Self {
        Self {
            width: width.max(1.0),
            height: height.max(1.0),
            strokes: Vec::new(),
            drawing: false,
        }
    }

    /// The pad's size (px).
    #[must_use]
    pub const fn size(&self) -> (f32, f32) {
        (self.width, self.height)
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.strokes.is_empty()
    }

    #[must_use]
    pub fn strokes(&self) -> &[Vec<(f32, f32)>] {
        &self.strokes
    }

    /// Whether a stroke is being drawn.
    #[must_use]
    pub const fn is_drawing(&self) -> bool {
        self.drawing
    }

    fn clamp(&self, x: f32, y: f32) -> (f32, f32) {
        (x.clamp(0.0, self.width), y.clamp(0.0, self.height))
    }

    /// A stroke starts at (`x`, `y`) (the button went down).
    pub fn begin(&mut self, x: f32, y: f32) {
        let p = self.clamp(x, y);
        self.strokes.push(vec![p]);
        self.drawing = true;
    }

    /// The stroke goes on to (`x`, `y`) (the pointer moved with the button down); `true` when
    /// the point was kept.
    pub fn extend(&mut self, x: f32, y: f32) -> bool {
        if !self.drawing {
            return false;
        }
        let p = self.clamp(x, y);
        let Some(stroke) = self.strokes.last_mut() else {
            return false;
        };
        if let Some(&(lx, ly)) = stroke.last() {
            if (p.0 - lx).hypot(p.1 - ly) < MIN_STEP_PX {
                return false;
            }
        }
        stroke.push(p);
        true
    }

    /// The stroke ends (the button came up).
    pub fn end(&mut self) {
        self.drawing = false;
    }

    /// Wipes the pad.
    pub fn clear(&mut self) {
        self.strokes.clear();
        self.drawing = false;
    }

    /// The drawing as an SVG of unfilled black paths over the pad's box; `None` when nothing
    /// is drawn.
    #[must_use]
    pub fn svg(&self) -> Option<String> {
        if self.strokes.is_empty() {
            return None;
        }
        let mut out = format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {} {}\" width=\"{}\" height=\"{}\">",
            number(self.width),
            number(self.height),
            number(self.width),
            number(self.height)
        );
        for stroke in &self.strokes {
            let mut d = String::new();
            for (i, (x, y)) in stroke.iter().enumerate() {
                d.push_str(if i == 0 { "M" } else { " L" });
                d.push_str(&format!("{} {}", number(*x), number(*y)));
            }
            if stroke.len() == 1 {
                // A tap: a dot, a line half a pixel long.
                let (x, y) = stroke[0];
                d.push_str(&format!(" L{} {}", number(x + 0.5), number(y)));
            }
            out.push_str(&format!(
                "<path d=\"{d}\" stroke=\"#000000\" stroke-width=\"2\" fill=\"none\"/>"
            ));
        }
        out.push_str("</svg>");
        Some(out)
    }
}

/// `v` as an SVG number: whole numbers without a point, else up to two decimals.
fn number(v: f32) -> String {
    let rounded = (v * 100.0).round() / 100.0;
    if rounded.fract() == 0.0 {
        format!("{rounded:.0}")
    } else {
        let text = format!("{rounded:.2}");
        text.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

/// A signature's box on a page: points from the page's top-left corner.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SigRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// A signature placed on a page (0-based).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlacedSignature {
    pub page: usize,
    pub rect: SigRect,
}

/// Where a signature drawn `drawing` (w, h px) lands for a click at `click` (CSS px from the
/// page frame's top-left) on a page shown `page_px` (w, h) large that is `page_pt` (w, h)
/// points: [`SIGNATURE_WIDTH_PT`] wide (at most the page's width), its height by the drawing's
/// shape, centred on the click and kept on the page.
#[must_use]
pub fn place(click: (f32, f32), page_px: (f32, f32), page_pt: (f32, f32), drawing: (f32, f32)) -> SigRect {
    let pt_per_px_x = page_pt.0 / page_px.0.max(1.0);
    let pt_per_px_y = page_pt.1 / page_px.1.max(1.0);
    let width = SIGNATURE_WIDTH_PT.min(page_pt.0);
    let height = (width * drawing.1 / drawing.0.max(1.0)).min(page_pt.1);
    let cx = click.0 * pt_per_px_x;
    let cy = click.1 * pt_per_px_y;
    let x = (cx - width / 2.0).clamp(0.0, (page_pt.0 - width).max(0.0));
    let y = (cy - height / 2.0).clamp(0.0, (page_pt.1 - height).max(0.0));
    SigRect {
        x,
        y,
        width,
        height,
    }
}
