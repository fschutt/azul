//! Shared problem-report machinery: SCREENSHOT REDACTION and the report
//! bundle both the "Report a problem" dialog and the crash reporter build.
//!
//! The redaction is the privacy-critical part. A screenshot of a real
//! session shows real work — names, addresses, an open document. The user
//! must be able to black out anything before it leaves the machine, and the
//! blackout must be applied to the BYTES THAT ARE SENT, not merely drawn
//! over the preview: a rectangle painted in the dialog and forgotten at
//! send time would be a privacy hole disguised as a feature. Everything
//! here therefore operates on the PNG that is actually attached.

use alloc::{
    string::{String, ToString},
    vec::Vec,
};

use crate::cpurender::AzulPixmap;

/// A blackout rectangle in the coordinate space of the DISPLAYED preview
/// (logical pixels, origin at the preview's top-left).
#[derive(Debug, Copy, Clone, PartialEq, Default)]
#[repr(C)]
pub struct RedactRect {
    /// Left edge.
    pub x: f32,
    /// Top edge.
    pub y: f32,
    /// Width; negative widths are normalised by [`normalized`](Self::normalized).
    pub width: f32,
    /// Height; negative heights are normalised.
    pub height: f32,
}

impl RedactRect {
    /// A rectangle from two drag corners, in any order.
    #[must_use]
    pub fn from_corners(x0: f32, y0: f32, x1: f32, y1: f32) -> Self {
        Self {
            x: x0.min(x1),
            y: y0.min(y1),
            width: (x1 - x0).abs(),
            height: (y1 - y0).abs(),
        }
    }

    /// Positive-extent form (a drag up-and-left still covers what it drew).
    #[must_use]
    pub fn normalized(self) -> Self {
        Self::from_corners(self.x, self.y, self.x + self.width, self.y + self.height)
    }

    /// Whether the rectangle covers any area at all.
    #[must_use]
    pub fn is_empty(self) -> bool {
        self.width.abs() < 0.5 || self.height.abs() < 0.5
    }
}

/// Paints every rectangle solid black into the PNG and re-encodes it.
///
/// `scale` converts preview coordinates to image pixels (the preview is
/// usually shown smaller than the capture): `image_px = preview_px * scale`.
/// Rectangles are CLAMPED to the image — a drag that ran off the edge
/// blacks out to the edge instead of failing, because the user's intent
/// ("hide this") is unambiguous.
///
/// # Errors
///
/// Returns a description if the PNG cannot be decoded or re-encoded. The
/// caller must then treat the screenshot as UNREDACTED and refuse to send
/// it — silently attaching the original would defeat the whole feature.
pub fn redact_png(png: &[u8], rects: &[RedactRect], scale: f32) -> Result<Vec<u8>, String> {
    if rects.is_empty() {
        return Ok(png.to_vec());
    }
    let mut pixmap = AzulPixmap::decode_png(png)?;
    let (w, h) = (pixmap.width(), pixmap.height());
    if w == 0 || h == 0 {
        return Err("screenshot has no pixels".to_string());
    }
    let scale = if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    };

    for rect in rects {
        let r = rect.normalized();
        if r.is_empty() {
            continue;
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let x0 = ((r.x * scale).max(0.0) as u32).min(w);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let y0 = ((r.y * scale).max(0.0) as u32).min(h);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let x1 = (((r.x + r.width) * scale).max(0.0) as u32).min(w);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let y1 = (((r.y + r.height) * scale).max(0.0) as u32).min(h);
        if x1 <= x0 || y1 <= y0 {
            continue;
        }
        let data = pixmap.data_mut();
        for y in y0..y1 {
            let row = (y as usize) * (w as usize) * 4;
            for x in x0..x1 {
                let i = row + (x as usize) * 4;
                data[i] = 0;
                data[i + 1] = 0;
                data[i + 2] = 0;
                data[i + 3] = 255;
            }
        }
    }
    pixmap.encode_png()
}

/// Crops a PNG to a rectangle in IMAGE pixel coordinates, clamped to the
/// image. Used by `CallbackInfo::take_screenshot_of_node`, which renders the
/// whole window and then keeps one node's box.
///
/// # Errors
///
/// Returns a description if the PNG cannot be decoded, the rectangle is
/// empty after clamping, or the result cannot be encoded.
pub fn crop_png(png: &[u8], x: u32, y: u32, width: u32, height: u32) -> Result<Vec<u8>, String> {
    let src = AzulPixmap::decode_png(png)?;
    let (sw, sh) = (src.width(), src.height());
    let x0 = x.min(sw);
    let y0 = y.min(sh);
    let x1 = x.saturating_add(width).min(sw);
    let y1 = y.saturating_add(height).min(sh);
    if x1 <= x0 || y1 <= y0 {
        return Err(alloc::format!(
            "crop rectangle {x},{y} {width}x{height} lies outside the {sw}x{sh} screenshot"
        ));
    }
    let (cw, ch) = (x1 - x0, y1 - y0);
    let mut out = AzulPixmap::new(cw, ch).ok_or_else(|| "pixmap alloc failed".to_string())?;
    {
        let srcd = src.data();
        let dst = out.data_mut();
        for row in 0..ch {
            let s = ((y0 + row) as usize) * (sw as usize) * 4 + (x0 as usize) * 4;
            let d = (row as usize) * (cw as usize) * 4;
            let len = (cw as usize) * 4;
            dst[d..d + len].copy_from_slice(&srcd[s..s + len]);
        }
    }
    out.encode_png()
}

/// Everything a report can carry. Each section is opt-in in the dialog, and
/// a section the user did not tick is `None` HERE — not filtered later —
/// so there is exactly one place that decides what leaves the machine.
#[derive(Debug, Clone, Default)]
pub struct ReportBundle {
    /// What the user typed.
    pub message: String,
    /// OS / CPU / memory summary.
    pub sysinfo: Option<String>,
    /// The action journal as JSON ("include recent actions").
    pub recent_actions: Option<String>,
    /// The app's serialized state ("include app data", default OFF).
    pub app_data: Option<String>,
    /// The REDACTED screenshot, PNG.
    pub screenshot_png: Option<Vec<u8>>,
}

impl ReportBundle {
    /// The human-readable body. Sections the user declined are absent, not
    /// empty-headed: a report that lists "System information:" with nothing
    /// under it reads like data was lost.
    #[must_use]
    pub fn to_text(&self) -> String {
        let mut out = String::new();
        out.push_str(self.message.trim());
        out.push_str("\n\n");
        if let Some(sys) = &self.sysinfo {
            out.push_str("--- System information ---\n");
            out.push_str(sys);
            out.push_str("\n\n");
        }
        if let Some(actions) = &self.recent_actions {
            out.push_str("--- Recent actions ---\n");
            out.push_str(actions);
            out.push_str("\n\n");
        }
        if self.app_data.is_some() {
            out.push_str("--- Application data is attached as app-data.json ---\n\n");
        }
        if self.screenshot_png.is_some() {
            out.push_str("--- A screenshot is attached ---\n");
        }
        out
    }

    /// `(filename, bytes)` for every attachment the user consented to.
    #[must_use]
    pub fn attachments(&self) -> Vec<(String, Vec<u8>)> {
        let mut out = Vec::new();
        if let Some(png) = &self.screenshot_png {
            out.push(("screenshot.png".to_string(), png.clone()));
        }
        if let Some(actions) = &self.recent_actions {
            out.push((
                "recent-actions.json".to_string(),
                actions.clone().into_bytes(),
            ));
        }
        if let Some(data) = &self.app_data {
            out.push(("app-data.json".to_string(), data.clone().into_bytes()));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn white_png(w: u32, h: u32) -> Vec<u8> {
        let mut p = AzulPixmap::new(w, h).expect("pixmap");
        p.fill(255, 255, 255, 255);
        p.encode_png().expect("encode")
    }

    fn pixel(png: &[u8], x: u32, y: u32) -> (u8, u8, u8, u8) {
        let p = AzulPixmap::decode_png(png).expect("decode");
        let i = (y as usize) * (p.width() as usize) * 4 + (x as usize) * 4;
        let d = p.data();
        (d[i], d[i + 1], d[i + 2], d[i + 3])
    }

    /// LAW: redaction blacks out the SENT BYTES, and only inside the
    /// rectangle. A preview-only blackout would be a privacy hole.
    #[test]
    fn redaction_blacks_out_the_attached_pixels_and_nothing_else() {
        let png = white_png(40, 20);
        let out = redact_png(
            &png,
            &[RedactRect {
                x: 10.0,
                y: 5.0,
                width: 10.0,
                height: 5.0,
            }],
            1.0,
        )
        .expect("redaction must succeed");

        assert_eq!(pixel(&out, 10, 5), (0, 0, 0, 255), "top-left of the rect");
        assert_eq!(
            pixel(&out, 19, 9),
            (0, 0, 0, 255),
            "bottom-right of the rect"
        );
        assert_eq!(
            pixel(&out, 9, 5),
            (255, 255, 255, 255),
            "the pixel LEFT of the rect must be untouched"
        );
        assert_eq!(
            pixel(&out, 20, 10),
            (255, 255, 255, 255),
            "the pixel past the rect must be untouched"
        );
    }

    /// LAW: the preview is smaller than the capture, so a rectangle drawn
    /// on it must scale to the right IMAGE pixels — an unscaled blackout
    /// would cover the wrong area and leave the secret visible.
    #[test]
    fn redaction_scales_preview_coordinates_to_image_pixels() {
        let png = white_png(40, 40);
        // Preview is half size: a 0..10 preview rect covers 0..20 image px.
        let out = redact_png(
            &png,
            &[RedactRect {
                x: 0.0,
                y: 0.0,
                width: 10.0,
                height: 10.0,
            }],
            2.0,
        )
        .expect("redaction must succeed");
        assert_eq!(
            pixel(&out, 19, 19),
            (0, 0, 0, 255),
            "scaled rect must reach 19,19"
        );
        assert_eq!(
            pixel(&out, 20, 20),
            (255, 255, 255, 255),
            "and must stop at 20,20"
        );
    }

    /// A drag that ran off the edge, a backwards drag and a zero-area drag
    /// must all behave, not panic.
    #[test]
    fn degenerate_rectangles_are_clamped_not_fatal() {
        let png = white_png(10, 10);
        let out = redact_png(
            &png,
            &[
                // Starts off-canvas and reaches IN: rows 0..4 must go black.
                RedactRect {
                    x: -50.0,
                    y: -2.0,
                    width: 500.0,
                    height: 6.0,
                },
                RedactRect {
                    x: 8.0,
                    y: 8.0,
                    width: -6.0,
                    height: -6.0,
                },
                RedactRect {
                    x: 1.0,
                    y: 1.0,
                    width: 0.0,
                    height: 0.0,
                },
            ],
            1.0,
        )
        .expect("degenerate rectangles must not fail the redaction");
        assert_eq!(
            pixel(&out, 0, 0),
            (0, 0, 0, 255),
            "clamped rect covers the top row"
        );
        assert_eq!(pixel(&out, 9, 3), (0, 0, 0, 255), "…across the full width");
        assert_eq!(
            pixel(&out, 9, 5),
            (255, 255, 255, 255),
            "…and stops where the rect ends"
        );
        assert_eq!(
            pixel(&out, 3, 3),
            (0, 0, 0, 255),
            "backwards drag still covers its area"
        );
    }

    #[test]
    fn crop_keeps_the_requested_box_and_clamps_the_rest() {
        let png = white_png(40, 20);
        let cropped = crop_png(&png, 10, 5, 8, 6).expect("crop");
        let p = AzulPixmap::decode_png(&cropped).expect("decode");
        assert_eq!((p.width(), p.height()), (8, 6));
        // A box hanging off the edge clamps to what exists.
        let clamped = crop_png(&png, 35, 15, 100, 100).expect("clamped crop");
        let p = AzulPixmap::decode_png(&clamped).expect("decode");
        assert_eq!((p.width(), p.height()), (5, 5));
        // Fully outside is an error, not an empty image.
        assert!(crop_png(&png, 100, 100, 10, 10).is_err());
    }

    /// LAW: a section the user did not tick must not appear anywhere in the
    /// report — not as an empty heading, not as an attachment.
    #[test]
    fn declined_sections_are_absent_from_text_and_attachments() {
        let bundle = ReportBundle {
            message: "it broke".to_string(),
            sysinfo: None,
            recent_actions: None,
            app_data: None,
            screenshot_png: None,
        };
        let text = bundle.to_text();
        assert!(text.contains("it broke"));
        assert!(!text.contains("System information"));
        assert!(!text.contains("Recent actions"));
        assert!(!text.contains("Application data"));
        assert!(!text.contains("screenshot"));
        assert!(bundle.attachments().is_empty());

        let full = ReportBundle {
            message: "it broke".to_string(),
            sysinfo: Some("linux".to_string()),
            recent_actions: Some("[]".to_string()),
            app_data: Some("{}".to_string()),
            screenshot_png: Some(vec![1, 2, 3]),
        };
        let names: Vec<String> = full.attachments().into_iter().map(|(n, _)| n).collect();
        assert_eq!(
            names,
            vec![
                "screenshot.png".to_string(),
                "recent-actions.json".to_string(),
                "app-data.json".to_string()
            ]
        );
    }
}
