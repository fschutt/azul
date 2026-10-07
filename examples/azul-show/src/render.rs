//! The slide renderer: one code path turns a slide into a DOM at any scale,
//! for the editor's canvas, the rail and the sorter's thumbnails, the show,
//! the presenter's current / next slide and the PDF export. A slide is a
//! fixed canvas (1920 x 1080 or 1440 x 1080 units) of absolutely positioned
//! boxes; the renderer multiplies every unit by the scale (no CSS scale, so
//! carets and hit tests stay in plain px) and turns a box with
//! `transform: rotate(..)`.
//!
//! A shape CSS cannot draw (a triangle, an arrow, a diamond, a chevron, a
//! line with an arrowhead) is an SVG polygon read into the DOM: azul builds
//! an `<svg>`'s shapes as nodes and paints them along their geometry
//! (`clip-path: polygon(..)` only clips to the polygon's bounding box, so
//! an arrow drawn that way was a rectangle).
//!
//! Animation (the show's builds and transitions, the editor's preview) is
//! the slide drawn at a progress, frame after frame. A box that travels is
//! moved with `transform: translate(..)`, never by its `left` / `top` or
//! margin: the engine slides any box whose laid-out place changes between
//! two DOMs (its own move animation), which lagged behind a box the app
//! moves every frame.

use std::collections::HashMap;

use azul::{
    callbacks::{RefAny, RichTextEditorOnChangeCallbackType},
    dom::Dom,
    error::ResultXmlXmlError,
    image::ImageRef,
    str::String as AzString,
    widgets::RichTextEditorState,
    xml::Xml,
};

use crate::{
    model::{
        morph_pairs, AnimationClass, AnimationEffect, Background, ChartKind, Color, Deck, Element,
        ElementKind, Frame, ImageFit, PlaceholderRole, ShapeKind, Slide, TextBody, TransitionKind,
        VAlign,
    },
    text,
};

/// How a slide is drawn.
#[derive(Clone, Copy)]
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
    /// Every element's box carries its id (`ids::ELEMENT_PREFIX`): the show
    /// and the preview, where the DOM is rebuilt every frame. Not where one
    /// element is drawn twice (the rail and the canvas).
    pub element_ids: bool,
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
            element_ids: false,
        }
    }
}

/// A linear progress `p` (0..1) with a smooth start and end (smoothstep).
#[must_use]
pub fn ease(p: f32) -> f32 {
    let p = p.clamp(0.0, 1.0);
    p * p * (3.0 - 2.0 * p)
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

/// The box of a frame at `scale`, as CSS: placed by `left` / `top`, moved
/// by `shift` (px, an animation's way: see the module's notes), turned by
/// the frame's rotation about its centre, then `extra_transform`.
fn box_css(f: &Frame, scale: f32, shift: (f32, f32), extra_transform: &str) -> String {
    let mut css = format!(
        "position: absolute; left: {:.2}px; top: {:.2}px; width: {:.2}px; height: {:.2}px; \
         box-sizing: border-box;",
        f.x * scale,
        f.y * scale,
        f.w.max(0.0) * scale,
        f.h.max(0.0) * scale,
    );
    let mut transform: Vec<String> = Vec::new();
    if shift.0.abs() > 0.005 || shift.1.abs() > 0.005 {
        transform.push(format!("translate({:.2}px, {:.2}px)", shift.0, shift.1));
    }
    if f.rotation.abs() > f32::EPSILON {
        transform.push(format!("rotate({:.2}deg)", f.rotation));
    }
    if !extra_transform.is_empty() {
        transform.push(extra_transform.to_string());
    }
    if !transform.is_empty() {
        css.push_str(&format!(" transform: {};", transform.join(" ")));
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

/// The outline of a shape CSS cannot draw, as points inside a `w` x `h` px
/// box: `inset` px inside it (half an outline, so the outline stays in the
/// box), a line arrow's shaft `line` px thick. `None` for the shapes drawn
/// as boxes (rectangles, ovals, the plain line).
#[must_use]
pub fn shape_outline(shape: ShapeKind, w: f32, h: f32, inset: f32, line: f32) -> Option<Vec<(f32, f32)>> {
    let inset = inset.max(0.0).min(w / 2.0).min(h / 2.0);
    let (x0, y0, x1, y1) = (inset, inset, w - inset, h - inset);
    let (bw, bh) = (x1 - x0, y1 - y0);
    let (cx, cy) = (x0 + bw / 2.0, y0 + bh / 2.0);
    match shape {
        ShapeKind::Triangle => Some(vec![(cx, y0), (x1, y1), (x0, y1)]),
        ShapeKind::Arrow => {
            let neck = x0 + bw * 0.65;
            Some(vec![
                (x0, y0 + bh * 0.3),
                (neck, y0 + bh * 0.3),
                (neck, y0),
                (x1, cy),
                (neck, y1),
                (neck, y0 + bh * 0.7),
                (x0, y0 + bh * 0.7),
            ])
        }
        ShapeKind::Diamond => Some(vec![(cx, y0), (x1, cy), (cx, y1), (x0, cy)]),
        ShapeKind::Chevron => {
            let notch = (bh * 0.5).min(bw * 0.5);
            Some(vec![(x0, y0), (x1 - notch, y0), (x1, cy), (x1 - notch, y1), (x0, y1), (x0 + notch, cy)])
        }
        ShapeKind::LineArrow => {
            // The arrowhead is a path of its own (no <marker>): a shaft
            // `line` thick, a head as wide as the box allows.
            let mid = h / 2.0;
            let t = line.max(1.0).min(h.max(1.0));
            let half_head = (t * 2.5).min(mid).max(t / 2.0);
            let head = (half_head * 1.6).min(w * 0.5);
            Some(vec![
                (0.0, mid - t / 2.0),
                (w - head, mid - t / 2.0),
                (w - head, mid - half_head),
                (w, mid),
                (w - head, mid + half_head),
                (w - head, mid + t / 2.0),
                (0.0, mid + t / 2.0),
            ])
        }
        ShapeKind::Rect | ShapeKind::RoundRect | ShapeKind::Ellipse | ShapeKind::Line => None,
    }
}

/// `points` filled with `fill` and outlined with `stroke` (its colour and
/// width in px), as an `<svg>` `w` x `h` px whose units are px.
#[must_use]
pub fn shape_svg(points: &[(f32, f32)], w: f32, h: f32, fill: Option<Color>, stroke: Option<(Color, f32)>) -> String {
    let points = points
        .iter()
        .map(|(x, y)| format!("{x:.1},{y:.1}"))
        .collect::<Vec<_>>()
        .join(" ");
    let fill = fill.map_or_else(|| String::from("none"), css_color);
    let stroke = stroke.map_or_else(String::new, |(color, width)| {
        format!(" stroke=\"{}\" stroke-width=\"{width:.1}\"", css_color(color))
    });
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w:.1}\" height=\"{h:.1}\" \
         viewBox=\"0 0 {w:.1} {h:.1}\"><polygon points=\"{points}\" fill=\"{fill}\"{stroke}/></svg>"
    )
}

/// A shape's picture over its whole box: its SVG read into the DOM (azul
/// draws an `<svg>`'s shapes as nodes, along their geometry).
fn shape_picture(svg: &str) -> Dom {
    let picture = match Xml::from_str(AzString::from(svg)) {
        ResultXmlXmlError::Ok(xml) => Dom::create_from_parsed_xml_fragment(xml),
        ResultXmlXmlError::Err(_) => Dom::create_div(),
    };
    Dom::create_div()
        .with_class(AzString::from(crate::ids::SHAPE_SVG_CLASS))
        .with_css("position: absolute; left: 0px; top: 0px; width: 100%; height: 100%;")
        .with_child(picture)
}

/// The effect of a playing build at progress `p` (0..1): (opacity, extra
/// transform, vertical offset in px - a `translate`, see the module notes).
fn build_effect(effect: AnimationEffect, p: f32, slide_h: f32) -> (f32, String, f32) {
    let p = p.clamp(0.0, 1.0);
    let e = ease(p);
    match effect {
        AnimationEffect::Appear => (1.0, String::new(), 0.0),
        AnimationEffect::Fade => (p, String::new(), 0.0),
        AnimationEffect::FlyIn => (p, String::new(), (1.0 - e) * slide_h * 0.25),
        AnimationEffect::Zoom => (p, format!("scale({:.3})", 0.3 + 0.7 * e), 0.0),
        AnimationEffect::Pulse => (
            1.0,
            format!("scale({:.3})", 1.0 + 0.08 * (p * core::f32::consts::PI).sin()),
            0.0,
        ),
        AnimationEffect::Spin => (1.0, format!("rotate({:.1}deg)", 360.0 * e), 0.0),
        AnimationEffect::Disappear => (0.0, String::new(), 0.0),
        AnimationEffect::FadeOut => (1.0 - p, String::new(), 0.0),
        AnimationEffect::FlyOut => (1.0 - p, String::new(), e * slide_h * 0.25),
    }
}

/// The DOM id of element `id`'s box (`RenderOptions::element_ids`).
#[must_use]
pub fn element_box_id(id: u64) -> String {
    format!("{}{id}", crate::ids::ELEMENT_PREFIX)
}

/// One element's DOM at the options' scale.
#[must_use]
pub fn element_dom(deck: &Deck, slide: &Slide, element: &Element, opts: &RenderOptions<'_>) -> Dom {
    element_posed(deck, slide, element, opts, Pose::REST)
}

/// Where an animation shows a box away from its frame, without moving its
/// laid-out box (see the module notes): moved by `shift` px, scaled about
/// its centre by `stretch` (after its rotation), faded to `fade`.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Pose {
    shift: (f32, f32),
    stretch: (f32, f32),
    fade: f32,
}

impl Pose {
    const REST: Pose = Pose {
        shift: (0.0, 0.0),
        stretch: (1.0, 1.0),
        fade: 1.0,
    };

    /// Faded to `fade`, in place.
    fn faded(fade: f32) -> Pose {
        Pose { fade, ..Pose::REST }
    }
}

/// [`element_dom`] in a `pose`: how the Morph transition shows a box on its
/// way, or fades one in or out.
fn element_posed(deck: &Deck, slide: &Slide, element: &Element, opts: &RenderOptions<'_>, pose: Pose) -> Dom {
    let scale = opts.scale;
    let playing = opts
        .playing
        .and_then(|(ids, p)| ids.contains(&element.id).then_some(p))
        .and_then(|p| element.animation.map(|a| (a.effect, p)));
    let (opacity, effect, dy) = match playing {
        Some((effect, p)) => build_effect(effect, p, deck.size.height() * scale),
        None => (1.0, String::new(), 0.0),
    };
    let opacity = (opacity * pose.fade).clamp(0.0, 1.0);
    let (sx, sy) = pose.stretch;
    let mut extra = if (sx - 1.0).abs() > 0.0005 || (sy - 1.0).abs() > 0.0005 {
        format!("scale({sx:.4}, {sy:.4})")
    } else {
        String::new()
    };
    if !effect.is_empty() {
        if !extra.is_empty() {
            extra.push(' ');
        }
        extra.push_str(&effect);
    }
    let mut css = box_css(&element.frame, scale, (pose.shift.0, pose.shift.1 + dy), &extra);
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
    let node = if opts.element_ids {
        Dom::create_div().with_id(AzString::from(element_box_id(element.id).as_str()))
    } else {
        Dom::create_div()
    };
    let node = match &element.kind {
        ElementKind::Text { body } => node.with_css(css).with_child(text_block(deck, element, body, opts)),
        ElementKind::Shape {
            shape,
            fill,
            stroke,
            stroke_width,
            body,
        } => {
            // A line's thickness (the plain line's bar, a line arrow's shaft).
            let thickness = (stroke_width.max(2.0) * scale).max(1.0);
            match shape {
                ShapeKind::Line => {
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
                    // A line arrow is one colour, the line's; a polygon has
                    // its fill and its outline.
                    let (paint_fill, paint_stroke) = if shape.is_line() {
                        (Some(stroke.or(*fill).unwrap_or(deck.theme.colors.text)), None)
                    } else {
                        (*fill, stroke.map(|c| (c, (stroke_width * scale).max(1.0))))
                    };
                    let inset = paint_stroke.map_or(0.0, |(_, width)| width / 2.0);
                    let outline = shape_outline(*shape, w, h, inset, thickness);
                    let drawn = outline.is_some();
                    let mut shape_css = css;
                    if !drawn {
                        // A box: rectangle, rounded rectangle, oval.
                        if let Some(c) = fill {
                            shape_css.push_str(&format!(" background: {};", css_color(*c)));
                        }
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
                    }
                    let mut node = node.with_css(shape_css);
                    if let Some(points) = outline {
                        if w >= 1.0 && h >= 1.0 {
                            node.add_child(shape_picture(&shape_svg(&points, w, h, paint_fill, paint_stroke)));
                        }
                    }
                    if !body.is_empty() || opts.editing == Some(element.id) {
                        let text = text_block(deck, element, body, opts);
                        // Over the picture (positioned, later in the tree).
                        node.add_child(if drawn {
                            Dom::create_div()
                                .with_css("position: relative; width: 100%; height: 100%;")
                                .with_child(text)
                        } else {
                            text
                        });
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

/// `b` on its way from `a` (the same object on the slide before) at `t`
/// (0..1), at `scale` px per unit: `b` laid out in its own box (so nothing
/// in it reflows, frame after frame) and posed over the frame `t` of the way
/// from `a`'s to its own - moved, turned and stretched - its fill and outline
/// colours `t` of the way from `a`'s.
fn morphed(a: &Element, b: &Element, t: f32, scale: f32) -> (Element, Pose) {
    let mut out = b.clone();
    let way = a.frame.lerp(&b.frame, t);
    out.frame.rotation = way.rotation;
    if let ElementKind::Shape {
        fill: from_fill,
        stroke: from_stroke,
        ..
    } = &a.kind
    {
        if let ElementKind::Shape { fill, stroke, .. } = &mut out.kind {
            if let (Some(x), Some(y)) = (*from_fill, *fill) {
                *fill = Some(x.lerp(y, t));
            }
            if let (Some(x), Some(y)) = (*from_stroke, *stroke) {
                *stroke = Some(x.lerp(y, t));
            }
        }
    }
    let (wc, bc) = (way.center(), b.frame.center());
    let ratio = |on_way: f32, own: f32| if own > f32::EPSILON { on_way / own } else { 1.0 };
    let pose = Pose {
        shift: ((wc.0 - bc.0) * scale, (wc.1 - bc.1) * scale),
        stretch: (ratio(way.w, b.frame.w), ratio(way.h, b.frame.h)),
        fade: 1.0,
    };
    (out, pose)
}

/// The Morph transition from `from` (with `from_step` of its builds played)
/// to `to` at progress `p` (0..1), as one slide: the objects on both
/// ([`morph_pairs`]) go from their old place, size and colours to their new
/// ones, the old slide's others fade out, the new one's fade in, over the
/// old ground fading into the new. `opts` draws `to` (`opts.step`: its
/// builds played).
#[must_use]
pub fn morph_dom(deck: &Deck, from: &Slide, from_step: usize, to: &Slide, p: f32, opts: &RenderOptions<'_>) -> Dom {
    let scale = opts.scale;
    let e = ease(p);
    let (w, h) = (deck.size.width() * scale, deck.size.height() * scale);
    let mut root = Dom::create_div().with_css(format!(
        "position: relative; width: {w:.2}px; height: {h:.2}px; overflow: hidden; flex-shrink: 0; {}",
        background_css(deck, to)
    ));
    let old_ground = background_css(deck, from);
    if old_ground != background_css(deck, to) {
        root.add_child(Dom::create_div().with_css(format!(
            "position: absolute; left: 0px; top: 0px; width: 100%; height: 100%; opacity: {:.3}; {old_ground}",
            1.0 - e
        )));
    }
    let shown_to = |j: usize| opts.step.map_or(true, |step| to.visible_at(&to.elements[j], step));
    let pairs: Vec<(usize, usize)> = morph_pairs(from, to)
        .into_iter()
        .filter(|&(i, j)| from.visible_at(&from.elements[i], from_step) && shown_to(j))
        .collect();
    let from_opts = RenderOptions {
        step: Some(from_step),
        playing: None,
        ..*opts
    };
    // The old slide's objects that leave, fading out ...
    for (i, a) in from.elements.iter().enumerate() {
        if pairs.iter().any(|&(pi, _)| pi == i) || !from.visible_at(a, from_step) {
            continue;
        }
        root.add_child(element_posed(deck, from, a, &from_opts, Pose::faded(1.0 - e)));
    }
    // ... under the new slide's, in its z-order: the pairs on their way, the
    // others fading in.
    for (j, b) in to.elements.iter().enumerate() {
        if !shown_to(j) {
            continue;
        }
        match pairs.iter().find(|&&(_, pj)| pj == j) {
            Some(&(i, _)) => {
                let (on_its_way, pose) = morphed(&from.elements[i], b, e, scale);
                root.add_child(element_posed(deck, to, &on_its_way, opts, pose));
            }
            None => root.add_child(element_posed(deck, to, b, opts, Pose::faded(e))),
        }
    }
    root
}

/// The slide `to` coming in by `kind` at progress `p` (0..1) over `from`
/// (the slide before it and how many of its builds had played; `None`: a
/// black screen), as the layers of a stage ([`stage`]): the slide going out
/// (`ids::LAYER_FROM`) under the one coming in (`ids::LAYER_TO`); for Morph
/// one layer whose objects move. `opts` draws `to`.
#[must_use]
pub fn transition_layers(
    deck: &Deck,
    from: Option<(&Slide, usize)>,
    to: &Slide,
    kind: TransitionKind,
    p: f32,
    opts: &RenderOptions<'_>,
) -> Vec<Dom> {
    let (w, h) = (deck.size.width() * opts.scale, deck.size.height() * opts.scale);
    let e = ease(p);
    if let (TransitionKind::Morph, Some((old, old_step))) = (kind, from) {
        let morph = morph_dom(deck, old, old_step, to, p, opts);
        return vec![stage_layer(crate::ids::LAYER_TO, morph, w, h, 0.0, 1.0, w)];
    }
    let old = match from {
        Some((old, old_step)) => slide_dom(
            deck,
            old,
            &RenderOptions {
                step: Some(old_step),
                playing: None,
                ..*opts
            },
        ),
        None => Dom::create_div().with_css(format!("width: {w:.2}px; height: {h:.2}px; background: #000000;")),
    };
    let new = slide_dom(deck, to, opts);
    let (old_layer, new_layer) = match kind {
        TransitionKind::Push => (
            stage_layer(crate::ids::LAYER_FROM, old, w, h, -e * w, 1.0, w),
            stage_layer(crate::ids::LAYER_TO, new, w, h, (1.0 - e) * w, 1.0, w),
        ),
        TransitionKind::Wipe => (
            stage_layer(crate::ids::LAYER_FROM, old, w, h, 0.0, 1.0, w),
            stage_layer(crate::ids::LAYER_TO, new, w, h, 0.0, 1.0, e * w),
        ),
        // Morph from black (the first slide's preview) fades in.
        TransitionKind::Fade | TransitionKind::Morph | TransitionKind::None => (
            stage_layer(crate::ids::LAYER_FROM, old, w, h, 0.0, 1.0, w),
            stage_layer(crate::ids::LAYER_TO, new, w, h, 0.0, p, w),
        ),
    };
    vec![old_layer, new_layer]
}

/// One layer of a stage `w` x `h` px: `child` moved `shift` px to the right
/// (a `translate`: see the module notes), faded to `opacity`, cut to
/// `clip_w` px from the left (a wipe).
#[must_use]
pub fn stage_layer(id: AzString, child: Dom, w: f32, h: f32, shift: f32, opacity: f32, clip_w: f32) -> Dom {
    let mut css = format!(
        "position: absolute; left: 0px; top: 0px; width: {:.2}px; height: {h:.2}px; overflow: hidden;",
        clip_w.clamp(0.0, w)
    );
    if shift.abs() > 0.05 {
        css.push_str(&format!(" transform: translate({shift:.2}px, 0px);"));
    }
    if opacity < 0.999 {
        css.push_str(&format!(" opacity: {:.3};", opacity.clamp(0.0, 1.0)));
    }
    Dom::create_div().with_id(id).with_css(css).with_child(child)
}

/// The stage the layers are stacked on, `w` x `h` px.
#[must_use]
pub fn stage(w: f32, h: f32, layers: Vec<Dom>) -> Dom {
    let mut stage = Dom::create_div().with_css(format!(
        "position: relative; width: {w:.2}px; height: {h:.2}px; overflow: hidden; flex-shrink: 0;"
    ));
    for layer in layers {
        stage.add_child(layer);
    }
    stage
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

#[cfg(test)]
mod shape_tests {
    use super::*;

    #[test]
    fn the_shapes_css_cannot_draw_are_polygons_inside_their_box() {
        for shape in ShapeKind::ALL {
            let outline = shape_outline(shape, 200.0, 100.0, 0.0, 6.0);
            let polygon = matches!(
                shape,
                ShapeKind::Triangle | ShapeKind::Arrow | ShapeKind::Diamond | ShapeKind::Chevron | ShapeKind::LineArrow
            );
            assert_eq!(outline.is_some(), polygon, "{shape:?}");
            for (x, y) in outline.unwrap_or_default() {
                assert!((0.0..=200.0).contains(&x) && (0.0..=100.0).contains(&y), "{shape:?}: ({x}, {y})");
            }
        }
        // The arrow's tip is the box's right middle; an outline keeps half its width inside.
        let arrow = shape_outline(ShapeKind::Arrow, 200.0, 100.0, 0.0, 0.0).expect("an arrow");
        assert!(arrow.contains(&(200.0, 50.0)));
        let inset = shape_outline(ShapeKind::Triangle, 200.0, 100.0, 4.0, 0.0).expect("a triangle");
        assert_eq!(inset, vec![(100.0, 4.0), (196.0, 96.0), (4.0, 96.0)]);
        // A line arrow: a shaft as thick as the line, its head (a path of its own) wider.
        let line = shape_outline(ShapeKind::LineArrow, 300.0, 60.0, 0.0, 6.0).expect("a line arrow");
        assert_eq!(line[0], (0.0, 27.0));
        assert_eq!(line[3], (300.0, 30.0));
        assert!(line[2].1 < 27.0 && line[4].1 > 33.0);
    }

    #[test]
    fn a_shape_is_an_svg_polygon_in_px() {
        let red = Some(Color::rgb(255, 0, 0));
        let black = Some((Color::rgb(0, 0, 0), 2.0));
        let svg = shape_svg(&[(0.0, 0.0), (10.0, 0.0), (5.0, 8.0)], 10.0, 8.0, red, black);
        assert!(svg.starts_with("<svg "), "{svg}");
        assert!(svg.contains("viewBox=\"0 0 10.0 8.0\""), "{svg}");
        assert!(svg.contains("points=\"0.0,0.0 10.0,0.0 5.0,8.0\""), "{svg}");
        assert!(svg.contains("fill=\"#ff0000\""), "{svg}");
        assert!(svg.contains("stroke=\"#000000\" stroke-width=\"2.0\""), "{svg}");
        assert!(shape_svg(&[(0.0, 0.0), (1.0, 1.0)], 1.0, 1.0, None, None).contains("fill=\"none\""));
    }

    #[test]
    fn a_box_on_its_way_moves_by_translate_before_it_turns() {
        let f = Frame {
            rotation: 30.0,
            ..Frame::new(10.0, 20.0, 100.0, 50.0)
        };
        let css = box_css(&f, 2.0, (5.0, -3.0), "scale(0.500)");
        assert!(css.contains("left: 20.00px; top: 40.00px;"), "{css}");
        assert!(
            css.contains("transform: translate(5.00px, -3.00px) rotate(30.00deg) scale(0.500);"),
            "{css}"
        );
        assert!(!box_css(&Frame::new(0.0, 0.0, 1.0, 1.0), 1.0, (0.0, 0.0), "").contains("transform"));
        // A build moves a box by a translate, never by its margin (the engine
        // slides a box whose laid-out place changed on its own).
        assert!(build_effect(AnimationEffect::FlyIn, 0.0, 1080.0).2 > 0.0);
        assert_eq!(build_effect(AnimationEffect::FlyIn, 1.0, 1080.0).2, 0.0);
    }

    #[test]
    fn a_morphing_box_keeps_its_own_layout_and_is_posed_on_its_way() {
        let shape = ElementKind::Shape {
            shape: ShapeKind::Rect,
            fill: Some(Color::rgb(0, 0, 0)),
            stroke: None,
            stroke_width: 0.0,
            body: TextBody::default(),
        };
        let a = Element::new(1, Frame::new(0.0, 0.0, 100.0, 100.0), shape);
        let mut b = a.clone();
        b.id = 2;
        b.frame = Frame {
            rotation: 90.0,
            ..Frame::new(200.0, 100.0, 200.0, 50.0)
        };
        if let ElementKind::Shape { fill, .. } = &mut b.kind {
            *fill = Some(Color::rgb(200, 100, 0));
        }
        // Laid out in b's box (nothing reflows frame after frame), shown over a's.
        let (start, pose) = morphed(&a, &b, 0.0, 2.0);
        assert_eq!((start.frame.x, start.frame.y, start.frame.w, start.frame.h), (200.0, 100.0, 200.0, 50.0));
        assert_eq!(start.frame.rotation, 0.0);
        assert_eq!(pose.shift, ((50.0 - 300.0) * 2.0, (50.0 - 125.0) * 2.0));
        assert_eq!(pose.stretch, (0.5, 2.0));
        let (half, _) = morphed(&a, &b, 0.5, 2.0);
        assert_eq!(half.frame.rotation, 45.0);
        assert!(matches!(half.kind, ElementKind::Shape { fill: Some(c), .. } if c == Color::rgb(100, 50, 0)));
        let (end, pose) = morphed(&a, &b, 1.0, 2.0);
        assert_eq!(end, b);
        assert_eq!(pose, Pose::REST);
    }

    #[test]
    fn ease_starts_and_ends_slowly() {
        assert_eq!(ease(0.0), 0.0);
        assert_eq!(ease(1.0), 1.0);
        assert_eq!(ease(0.5), 0.5);
        assert!(ease(0.1) < 0.1 && ease(0.9) > 0.9);
        assert_eq!(ease(2.0), 1.0);
    }
}
