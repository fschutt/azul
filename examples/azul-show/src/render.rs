//! The slide renderer: one code path turns a slide into a DOM at any scale,
//! for the editor's canvas, the rail and the sorter's thumbnails, the show,
//! the presenter's current / next slide and the PDF export. A slide is a
//! fixed canvas (1920 x 1080 or 1440 x 1080 units) of absolutely positioned
//! boxes; the renderer multiplies every unit by the scale (no CSS scale, so
//! carets and hit tests stay in plain px) and turns a box with
//! `transform: rotate(..)`.

use std::collections::HashMap;

use azul::{
    callbacks::{RefAny, RichTextEditorOnChangeCallbackType},
    dom::Dom,
    image::ImageRef,
    widgets::RichTextEditorState,
};

use crate::{
    model::{
        AnimationClass, AnimationEffect, Background, ChartKind, Color, Deck, Element, ElementKind,
        Frame, ImageFit, PlaceholderRole, ShapeKind, Slide, TextBody, VAlign,
    },
    text,
};

/// How a slide is drawn.
pub struct RenderOptions<'a> {
    /// px per slide unit.
    pub scale: f32,
    /// The element whose text is being edited (contenteditable), if any.
    pub editing: Option<u64>,
    /// The shared editor's last state of that text (`Editor::text`).
    pub text: Option<&'a RichTextEditorState>,
    /// Empty placeholders show their prompt and a dashed outline (the editor).
    pub prompts: bool,
    /// The show's build step: elements whose build has not played are left
    /// out (`None`: everything is shown).
    pub step: Option<usize>,
    /// The elements whose build is playing and how far (0..1).
    pub playing: Option<(&'a [u64], f32)>,
    /// The deck's pictures, by media key.
    pub media: &'a HashMap<String, ImageRef>,
    /// The app, for the text being edited: the editor reports every edit to
    /// it (`views::on_text_change`).
    pub hooks: Option<&'a RefAny>,
}

impl<'a> RenderOptions<'a> {
    /// A still picture of the slide at `scale` (thumbnails, PDF).
    #[must_use]
    pub fn still(scale: f32, media: &'a HashMap<String, ImageRef>) -> Self {
        Self {
            scale,
            editing: None,
            text: None,
            prompts: false,
            step: None,
            playing: None,
            media,
            hooks: None,
        }
    }
}

/// A colour as CSS.
#[must_use]
pub fn css_color(c: Color) -> String {
    if c.a == 255 {
        format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b)
    } else {
        format!("rgba({}, {}, {}, {:.3})", c.r, c.g, c.b, f32::from(c.a) / 255.0)
    }
}

/// A font list as CSS: family names with spaces quoted.
#[must_use]
pub fn css_font_family(list: &str) -> String {
    list.split(',')
        .map(str::trim)
        .filter(|f| !f.is_empty())
        .map(|f| {
            if f.contains(' ') && !f.starts_with('"') && !f.starts_with('\'') {
                format!("\"{f}\"")
            } else {
                f.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// The slide's ground as CSS.
#[must_use]
pub fn background_css(deck: &Deck, slide: &Slide) -> String {
    match slide.background {
        Some(Background::Solid { color }) => format!("background: {};", css_color(color)),
        Some(Background::Gradient { from, to }) => format!(
            "background: linear-gradient(to bottom, {}, {});",
            css_color(from),
            css_color(to)
        ),
        None => format!("background: {};", css_color(deck.theme.colors.background)),
    }
}

/// The box of a frame at `scale`, as CSS (and the extra transform).
fn box_css(f: &Frame, scale: f32, extra_transform: &str) -> String {
    let mut css = format!(
        "position: absolute; left: {:.2}px; top: {:.2}px; width: {:.2}px; height: {:.2}px; \
         box-sizing: border-box;",
        f.x * scale,
        f.y * scale,
        f.w.max(0.0) * scale,
        f.h.max(0.0) * scale,
    );
    let mut transform = String::new();
    if f.rotation.abs() > f32::EPSILON {
        transform.push_str(&format!("rotate({:.2}deg)", f.rotation));
    }
    if !extra_transform.is_empty() {
        if !transform.is_empty() {
            transform.push(' ');
        }
        transform.push_str(extra_transform);
    }
    if !transform.is_empty() {
        css.push_str(&format!(" transform: {transform};"));
    }
    css
}

/// The ink, font and size of a text body for its role.
fn text_css(deck: &Deck, body: &TextBody, role: Option<PlaceholderRole>, scale: f32) -> String {
    let heading = role.is_some_and(PlaceholderRole::is_heading);
    let color = body.color.unwrap_or(if heading {
        deck.theme.colors.title
    } else {
        deck.theme.colors.text
    });
    let font = body.font.clone().unwrap_or_else(|| {
        if heading {
            deck.theme.fonts.heading.clone()
        } else {
            deck.theme.fonts.body.clone()
        }
    });
    let weight = if matches!(role, Some(PlaceholderRole::Heading | PlaceholderRole::Heading2)) {
        " font-weight: bold;"
    } else {
        ""
    };
    format!(
        "font-size: {:.2}px; font-family: {}; color: {}; line-height: 1.15;{weight}",
        (body.size * scale).max(1.0),
        css_font_family(&font),
        css_color(color),
    )
}

/// The text of an element, vertically placed in its box.
fn text_block(deck: &Deck, element: &Element, body: &TextBody, opts: &RenderOptions<'_>) -> Dom {
    let scale = opts.scale;
    let editing = opts.editing == Some(element.id);
    let justify = match body.valign {
        VAlign::Top => "flex-start",
        VAlign::Middle => "center",
        VAlign::Bottom => "flex-end",
    };
    let pad = 12.0 * scale;
    let holder = Dom::create_div().with_css(format!(
        "display: flex; flex-direction: column; justify-content: {justify}; width: 100%; \
         height: 100%; box-sizing: border-box; padding: {pad:.2}px; overflow: hidden; {}",
        text_css(deck, body, element.placeholder, scale)
    ));
    if body.is_empty() && !editing {
        if opts.prompts && element.placeholder.is_some() && !body.prompt.is_empty() {
            return holder.with_child(
                Dom::create_p_with_text(body.prompt.as_str())
                    .with_css(format!("margin: 0px; color: {};", css_color(deck.theme.colors.text.with_alpha(110)))),
            );
        }
        return holder;
    }
    let mut shown = body.clone();
    if shown.paragraphs.is_empty() {
        shown.paragraphs.push(crate::model::Paragraph::default());
    }
    // The shared rich-text editor draws the text: editable (its own typing,
    // structure, formats and history, every edit reported to the app) for
    // the text being edited, read-only everywhere else.
    let editor = match (editing, opts.hooks) {
        (true, Some(app)) => text::editor(&shown, text::state_for(&shown, element.id, opts.text), scale, true)
            .with_on_change(app.clone(), crate::views::on_text_change as RichTextEditorOnChangeCallbackType),
        _ => text::editor(&shown, text::view_state(&shown, element.id), scale, false),
    };
    holder.with_child(text::content(editor))
}

/// The polygon of a shape inside a `w` x `h` px box, for `clip-path`.
fn shape_polygon(shape: ShapeKind, w: f32, h: f32) -> Option<String> {
    let pt = |x: f32, y: f32| format!("{:.1}px {:.1}px", x, y);
    match shape {
        ShapeKind::Triangle => Some(format!("polygon({}, {}, {})", pt(w / 2.0, 0.0), pt(w, h), pt(0.0, h))),
        ShapeKind::Arrow => Some(format!(
            "polygon({}, {}, {}, {}, {}, {}, {})",
            pt(0.0, h * 0.3),
            pt(w * 0.65, h * 0.3),
            pt(w * 0.65, 0.0),
            pt(w, h / 2.0),
            pt(w * 0.65, h),
            pt(w * 0.65, h * 0.7),
            pt(0.0, h * 0.7),
        )),
        _ => None,
    }
}

/// The effect of a playing build at progress `p` (0..1): (opacity, extra
/// transform, vertical offset in px).
fn build_effect(effect: AnimationEffect, p: f32, slide_h: f32) -> (f32, String, f32) {
    let p = p.clamp(0.0, 1.0);
    match effect {
        AnimationEffect::Appear => (1.0, String::new(), 0.0),
        AnimationEffect::Fade => (p, String::new(), 0.0),
        AnimationEffect::FlyIn => (p, String::new(), (1.0 - p) * slide_h * 0.25),
        AnimationEffect::Zoom => (p, format!("scale({:.3})", 0.3 + 0.7 * p), 0.0),
        AnimationEffect::Pulse => (
            1.0,
            format!("scale({:.3})", 1.0 + 0.08 * (p * core::f32::consts::PI).sin()),
            0.0,
        ),
        AnimationEffect::Spin => (1.0, format!("rotate({:.1}deg)", 360.0 * p), 0.0),
        AnimationEffect::Disappear => (0.0, String::new(), 0.0),
        AnimationEffect::FadeOut => (1.0 - p, String::new(), 0.0),
        AnimationEffect::FlyOut => (1.0 - p, String::new(), p * slide_h * 0.25),
    }
}

/// One element's DOM at the options' scale.
#[must_use]
pub fn element_dom(deck: &Deck, slide: &Slide, element: &Element, opts: &RenderOptions<'_>) -> Dom {
    let scale = opts.scale;
    let playing = opts
        .playing
        .and_then(|(ids, p)| ids.contains(&element.id).then_some(p))
        .and_then(|p| element.animation.map(|a| (a.effect, p)));
    let (opacity, extra, dy) = match playing {
        Some((effect, p)) => build_effect(effect, p, deck.size.height() * scale),
        None => (1.0, String::new(), 0.0),
    };
    let mut css = box_css(&element.frame, scale, &extra);
    if dy.abs() > f32::EPSILON {
        css.push_str(&format!(" margin-top: {dy:.2}px;"));
    }
    if opacity < 0.999 {
        css.push_str(&format!(" opacity: {opacity:.3};"));
    }
    let (w, h) = (element.frame.w * scale, element.frame.h * scale);
    let empty_placeholder = opts.prompts
        && element.placeholder.is_some()
        && element.body().is_some_and(TextBody::is_empty)
        && opts.editing != Some(element.id);
    if empty_placeholder {
        css.push_str(&format!(
            " border: 1px dashed {};",
            css_color(deck.theme.colors.text.with_alpha(120))
        ));
    }
    let node = Dom::create_div();
    let node = match &element.kind {
        ElementKind::Text { body } => node.with_css(css).with_child(text_block(deck, element, body, opts)),
        ElementKind::Shape {
            shape,
            fill,
            stroke,
            stroke_width,
            body,
        } => {
            let fill_css = fill.map(|c| format!("background: {};", css_color(c))).unwrap_or_default();
            match shape {
                ShapeKind::Line => {
                    let thickness = (stroke_width.max(2.0) * scale).max(1.0);
                    let color = stroke.or(*fill).unwrap_or(deck.theme.colors.text);
                    node.with_css(css).with_child(Dom::create_div().with_css(format!(
                        "position: absolute; left: 0px; top: {:.2}px; width: 100%; height: {:.2}px; \
                         background: {};",
                        (h - thickness) / 2.0,
                        thickness,
                        css_color(color)
                    )))
                }
                _ => {
                    let mut shape_css = format!("{css} {fill_css}");
                    if let Some(c) = stroke {
                        shape_css.push_str(&format!(
                            " border: {:.2}px solid {};",
                            (stroke_width * scale).max(1.0),
                            css_color(*c)
                        ));
                    }
                    match shape {
                        ShapeKind::RoundRect => shape_css.push_str(&format!(
                            " border-radius: {:.2}px;",
                            w.min(h) * 0.15
                        )),
                        ShapeKind::Ellipse => shape_css.push_str(" border-radius: 50%;"),
                        _ => {}
                    }
                    if let Some(polygon) = shape_polygon(*shape, w, h) {
                        shape_css.push_str(&format!(" clip-path: {polygon};"));
                    }
                    let mut node = node.with_css(shape_css);
                    if !body.is_empty() || opts.editing == Some(element.id) {
                        node.add_child(text_block(deck, element, body, opts));
                    }
                    node
                }
            }
        }
        ElementKind::Image { media, fit } => {
            let inner = match opts.media.get(media.as_str()) {
                Some(image) => {
                    // Placed by its fit inside the frame (Cover's overflow
                    // is clipped by the frame).
                    let size = image.get_size();
                    let (x, y, iw, ih) = fit_rect(*fit, (w, h), (size.width, size.height));
                    Dom::create_div()
                        .with_css("position: relative; width: 100%; height: 100%; overflow: hidden;")
                        .with_child(Dom::create_image(image.clone()).with_css(format!(
                            "position: absolute; left: {x:.2}px; top: {y:.2}px; width: {iw:.2}px; height: {ih:.2}px;"
                        )))
                }
                None => Dom::create_div()
                    .with_css(format!(
                        "width: 100%; height: 100%; display: flex; align-items: center; \
                         justify-content: center; background: {}; color: {}; font-size: {:.1}px;",
                        css_color(deck.theme.colors.accent3.with_alpha(60)),
                        css_color(deck.theme.colors.text),
                        (48.0 * scale).max(6.0)
                    ))
                    .with_child(Dom::create_icon("image")),
            };
            node.with_css(css).with_child(inner)
        }
        ElementKind::Table { rows, header } => {
            // Edited in place: every cell its own editing host.
            let editing = match opts.hooks {
                Some(app) if opts.editing == Some(element.id) => Some(app),
                _ => None,
            };
            let size = (24.0 * scale).max(1.0);
            let mut table = Dom::create_table_no_a11y().with_css(format!(
                "border-collapse: collapse; width: 100%; font-size: {size:.2}px; font-family: {}; color: {};",
                css_font_family(&deck.theme.fonts.body),
                css_color(deck.theme.colors.text)
            ));
            for (r, row) in rows.iter().enumerate() {
                let is_head = *header && r == 0;
                let mut tr = Dom::create_tr();
                for (c, cell) in row.iter().enumerate() {
                    let cell_css = if is_head {
                        format!(
                            "padding: {p:.2}px; border: 1px solid {b}; background: {bg}; color: {ink}; font-weight: bold;",
                            p = 8.0 * scale,
                            b = css_color(deck.theme.colors.accent),
                            bg = css_color(deck.theme.colors.accent),
                            ink = css_color(deck.theme.colors.background),
                        )
                    } else {
                        format!(
                            "padding: {p:.2}px; border: 1px solid {b};",
                            p = 8.0 * scale,
                            b = css_color(deck.theme.colors.accent.with_alpha(140)),
                        )
                    };
                    let td = Dom::create_td_with_text(cell.as_str()).with_css(cell_css);
                    tr.add_child(match editing {
                        Some(app) => crate::views::editable_cell(td, app, element.id, r, c),
                        None => td,
                    });
                }
                table.add_child(tr);
            }
            node.with_css(css).with_child(table)
        }
        ElementKind::Chart { chart, title } => node.with_css(css).with_child(chart_placeholder(deck, *chart, title, w, h, scale)),
        ElementKind::Video { .. } => node
            .with_css(format!(
                "{css} background: #1b1b1b; display: flex; align-items: center; justify-content: center; \
                 color: #f0f0f0; font-size: {:.1}px;",
                (96.0 * scale).max(6.0)
            ))
            .with_child(Dom::create_icon("play_circle")),
        ElementKind::Group { children } => {
            // The members keep their own frames in slide units: inside the
            // group's box they are drawn relative to its corner.
            let mut group = node.with_css(css);
            for child in children {
                let mut local = child.clone();
                local.set_frame(child.frame.translated(-element.frame.x, -element.frame.y));
                group.add_child(element_dom(deck, slide, &local, opts));
            }
            group
        }
    };
    node
}

/// A chart placeholder: the title over a few bars (or a disc for a pie).
fn chart_placeholder(deck: &Deck, chart: ChartKind, title: &str, w: f32, h: f32, scale: f32) -> Dom {
    let colors = [deck.theme.colors.accent, deck.theme.colors.accent2, deck.theme.colors.accent3];
    let mut plot = Dom::create_div().with_css(
        "display: flex; flex-direction: row; align-items: flex-end; justify-content: space-around; \
         flex-grow: 1; width: 100%;",
    );
    match chart {
        ChartKind::Pie => {
            let d = w.min(h) * 0.6;
            plot.add_child(Dom::create_div().with_css(format!(
                "width: {d:.1}px; height: {d:.1}px; border-radius: 50%; background: {};",
                css_color(colors[0])
            )));
        }
        ChartKind::Bar | ChartKind::Line => {
            for (i, value) in [0.45f32, 0.8, 0.6, 0.95].iter().enumerate() {
                plot.add_child(Dom::create_div().with_css(format!(
                    "width: {:.1}px; height: {:.1}px; background: {};",
                    w * 0.12,
                    h * 0.7 * value,
                    css_color(colors[i % colors.len()])
                )));
            }
        }
    }
    Dom::create_div()
        .with_css(format!(
            "display: flex; flex-direction: column; width: 100%; height: 100%; box-sizing: border-box; \
             padding: {:.1}px; font-size: {:.1}px; color: {}; font-family: {};",
            10.0 * scale,
            (28.0 * scale).max(1.0),
            css_color(deck.theme.colors.text),
            css_font_family(&deck.theme.fonts.body),
        ))
        .with_child(Dom::create_p_with_text(title).with_css("margin: 0px; text-align: center;"))
        .with_child(plot)
}

/// The slide at the options' scale: its ground, then its elements in
/// z-order (the show leaves out the builds that have not played).
#[must_use]
pub fn slide_dom(deck: &Deck, slide: &Slide, opts: &RenderOptions<'_>) -> Dom {
    let (w, h) = (deck.size.width() * opts.scale, deck.size.height() * opts.scale);
    let mut root = Dom::create_div().with_css(format!(
        "position: relative; width: {w:.2}px; height: {h:.2}px; overflow: hidden; flex-shrink: 0; {}",
        background_css(deck, slide)
    ));
    for element in &slide.elements {
        if let Some(step) = opts.step {
            let playing_exit = opts.playing.is_some_and(|(ids, _)| ids.contains(&element.id))
                && element.animation.is_some_and(|a| a.effect.class() == AnimationClass::Exit);
            if !slide.visible_at(element, step) && !playing_exit {
                continue;
            }
        }
        root.add_child(element_dom(deck, slide, element, opts));
    }
    root
}

/// Where a picture of `image` size (px) lies in its `frame` box (px) for
/// `fit`: (left, top, width, height) inside the box. Contain shows all of
/// it centred (bands beside or above it), Cover fills the box centred (the
/// overflow is clipped), Stretch fills the box.
#[must_use]
pub fn fit_rect(fit: ImageFit, frame: (f32, f32), image: (f32, f32)) -> (f32, f32, f32, f32) {
    let (bw, bh) = frame;
    let (iw, ih) = image;
    if fit == ImageFit::Stretch || iw <= 0.0 || ih <= 0.0 || !iw.is_finite() || !ih.is_finite() {
        return (0.0, 0.0, bw, bh);
    }
    let scale = if fit == ImageFit::Contain {
        (bw / iw).min(bh / ih)
    } else {
        (bw / iw).max(bh / ih)
    };
    let (w, h) = (iw * scale, ih * scale);
    ((bw - w) / 2.0, (bh - h) / 2.0, w, h)
}

#[cfg(test)]
mod fit_tests {
    use super::*;

    #[test]
    fn contain_shows_all_of_the_picture_and_cover_fills_the_box() {
        // A 200 x 100 picture in a 100 x 100 box.
        assert_eq!(fit_rect(ImageFit::Contain, (100.0, 100.0), (200.0, 100.0)), (0.0, 25.0, 100.0, 50.0));
        assert_eq!(fit_rect(ImageFit::Cover, (100.0, 100.0), (200.0, 100.0)), (-50.0, 0.0, 200.0, 100.0));
        assert_eq!(fit_rect(ImageFit::Stretch, (100.0, 100.0), (200.0, 100.0)), (0.0, 0.0, 100.0, 100.0));
        // A tall picture: bands left and right.
        assert_eq!(fit_rect(ImageFit::Contain, (200.0, 100.0), (50.0, 100.0)), (75.0, 0.0, 50.0, 100.0));
        // No size known (0): stretch.
        assert_eq!(fit_rect(ImageFit::Contain, (80.0, 60.0), (0.0, 0.0)), (0.0, 0.0, 80.0, 60.0));
    }
}
