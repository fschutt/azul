//! The graphing view's lower half: the list of functions (y₁, y₂, ...
//! typeset, in their curves' colours) and the plot - a paper box with its
//! grid and axes as one-pixel boxes, the tick numbers as text, and the
//! curves as SVG paths ([`crate::graph::svg`] read into DOM nodes), all
//! placed in the plot's own pixels.
//!
//! The plot measures itself (`AfterMount` / `NodeResized`: the rebuild that
//! follows draws at the new size), the wheel zooms about the pointer, a
//! drag moves the plane, a double-click (or Reset view) puts -10..10 back.
//! The entry being typed is plotted live when it has `x` in it, fainter
//! until `=` commits it.
//!
//! On stdout for scripts: `AZCALC_GRAPH x <lo>..<hi> y <lo>..<hi>` when the
//! viewport changes.

use azul::{
    callbacks::CallbackType,
    dom::{AccessibilityInfo, AccessibilityRole, DomId, DomNodeId, NodeId},
    prelude::*,
    str::String as AzString,
};

use crate::calc::{subscript, Calculator};
use crate::expr::{self, Tok};
use crate::graph::{self, Curve, Viewport};
use crate::ids;
use crate::look::{self, Look};
use crate::mathview;
use crate::ui::{with_app, CalcApp};

/// The plot's state: where it looks and how big it is.
#[derive(Clone, Debug)]
pub struct GraphState {
    pub vp: Viewport,
    /// The plot's inner size in px, as last measured.
    pub px: (f32, f32),
    /// The size was measured once: the first measure fits -10..10 to it.
    pub measured: bool,
    /// A drag in progress: where the pointer went down (window px) and the
    /// viewport then.
    pub drag: Option<(f32, f32, Viewport)>,
}

impl Default for GraphState {
    fn default() -> Self {
        GraphState {
            vp: Viewport::fresh(640.0),
            px: (640.0, 300.0),
            measured: false,
            drag: None,
        }
    }
}

impl GraphState {
    fn announce(&self) {
        println!("AZCALC_GRAPH {}", self.vp.describe(self.px.0, self.px.1));
    }
}

/// The functions the plot draws: the committed ones (`Some(i)`: y₁ is 0)
/// and the entry being typed, when it is a function (`None`).
fn functions(calc: &Calculator) -> Vec<(Option<usize>, Vec<Tok>)> {
    let mut out: Vec<(Option<usize>, Vec<Tok>)> = calc
        .plots
        .iter()
        .enumerate()
        .map(|(i, p)| (Some(i), p.tokens.clone()))
        .collect();
    if calc.entry_is_function() && !calc.tokens.is_empty() {
        out.push((None, calc.tokens.clone()));
    }
    out
}

/// The curve of `tokens` over the plot, or nothing if it does not parse yet.
fn curve_of(tokens: &[Tok], g: &GraphState) -> String {
    let Ok(e) = expr::parse(tokens) else {
        return String::new();
    };
    let Some(fx) = graph::compile(&e) else {
        return String::new();
    };
    graph::path_d(&graph::sample(&fx, &g.vp, g.px.0, g.px.1, 2.0))
}

/// The graphing view's lower half: the functions beside the plot.
#[must_use]
pub fn panel(s: &CalcApp, app: &RefAny, look: Look) -> Dom {
    Dom::create_div()
        .with_id(ids::GRAPH)
        .with_css(format!(
            "display: flex; flex-direction: row; flex-grow: 1; min-height: 220px; margin-top: 8px; {}",
            look::ENTER_GRAPH
        ))
        .with_child(function_list(s, app, look))
        .with_child(plot(s, app, look))
}

struct FunctionRef {
    app: RefAny,
    index: usize,
}

/// The list of functions: each one's colour, its name and the function
/// typeset; the entry being typed under them while it is a function.
fn function_list(s: &CalcApp, app: &RefAny, look: Look) -> Dom {
    let calc = &s.calc;
    let mut list = Dom::create_div()
        .with_id(ids::FUNCTIONS)
        .with_css(format!(
            "{} width: 270px; flex-shrink: 0; margin-right: 8px;",
            look::PANEL
        ))
        .with_accessibility_info(AccessibilityInfo::named("Functions", AccessibilityRole::List))
        .with_child(
            Dom::create_div()
                .with_css(look::PANEL_TITLE)
                .with_child(Dom::create_div_with_text("Functions").with_css("flex-grow: 1;"))
                .with_child(small_button(app, "Reset view", ids::RESET_VIEW, on_plot_reset))
                .with_child(small_button(app, "Clear", ids::CLEAR_PLOTS, on_clear_functions)),
        );
    let mut rows = Dom::create_div().with_css("display: flex; flex-direction: column; flex-grow: 1; overflow-y: auto;");
    for (i, plot) in calc.plots.iter().enumerate() {
        let typeset = expr::parse(&plot.tokens)
            .ok()
            .map(|e| mathview::to_dom(&mathview::layout(&e, calc.grouping)))
            .unwrap_or_else(|| Dom::create_div_with_text(AzString::from(plot.text.as_str())));
        rows.add_child(
            function_row(look.curve(i), &format!("y{} =", subscript(i + 1)), typeset)
                .with_child(
                    Dom::create_div_with_text("\u{2715}")
                        .with_css(look::FUNCTION_REMOVE)
                        .with_accessibility_info(AccessibilityInfo::named(
                            format!("Remove y{}", i + 1),
                            AccessibilityRole::PushButton,
                        ))
                        .with_callback(
                            EventFilter::Hover(HoverEventFilter::Click),
                            RefAny::new(FunctionRef { app: app.clone(), index: i }),
                            on_remove_function,
                        ),
                ),
        );
    }
    if calc.entry_is_function() {
        let typeset = mathview::entry(&calc.tokens, &calc.letters, calc.grouping)
            .map(|m| mathview::to_dom(&m))
            .unwrap_or_else(|| Dom::create_div_with_text("\u{2026}"));
        rows.add_child(function_row(look.draft(), "y =", typeset).with_css("opacity: 0.75;"));
    }
    if calc.plots.is_empty() && !calc.entry_is_function() {
        rows.add_child(
            Dom::create_div_with_text(
                "Type a function of x \u{2014} sin(x)*x^2, y = 2x + 1 \u{2014} and press = to plot it.",
            )
            .with_css(look::EMPTY_NOTE),
        );
    }
    list.add_child(rows);
    list.with_child(
        Dom::create_div_with_text(
            "Angles in radians \u{b7} the wheel zooms \u{b7} drag to move \u{b7} double-click resets",
        )
        .with_css(look::GRAPH_NOTE),
    )
}

/// A row of the list: the curve's colour, the name, the function.
fn function_row(color: &str, name: &str, typeset: Dom) -> Dom {
    Dom::create_div()
        .with_class(ids::FUNCTION)
        .with_css(look::FUNCTION_ROW)
        .with_child(Dom::create_div().with_css(format!(
            "width: 12px; height: 12px; border-radius: 6px; flex-shrink: 0; margin-right: 8px; \
             background: {color};"
        )))
        .with_child(Dom::create_div_with_text(AzString::from(name)).with_css(look::FUNCTION_NAME))
        .with_child(
            Dom::create_div()
                .with_css("flex-grow: 1; min-width: 0px; overflow: hidden; font-size: 17px;")
                .with_child(typeset),
        )
}

fn small_button(app: &RefAny, label: &str, id: AzString, cb: CallbackType) -> Dom {
    Dom::create_div_with_text(AzString::from(label))
        .with_id(id)
        .with_css(look::SMALL_BUTTON)
        .with_accessibility_info(AccessibilityInfo::named(label, AccessibilityRole::PushButton))
        .with_tab_index(azul::dom::TabIndex::Auto)
        .with_callback(EventFilter::Hover(HoverEventFilter::Click), app.clone(), cb)
}

/// The plot: grid, axes, tick numbers, curves.
fn plot(s: &CalcApp, app: &RefAny, look: Look) -> Dom {
    let g = &s.graph;
    let (w, h) = g.px;
    let vp = g.vp;
    let mut plot = Dom::create_div()
        .with_id(ids::PLOT)
        .with_css(look::PLOT)
        .with_accessibility_info(AccessibilityInfo::named(
            format!("Graph, {}", vp.describe(w, h)),
            AccessibilityRole::Chart,
        ))
        .with_callback(EventFilter::Component(ComponentEventFilter::AfterMount), app.clone(), on_plot_size)
        .with_callback(EventFilter::Component(ComponentEventFilter::NodeResized), app.clone(), on_plot_size)
        .with_callback(EventFilter::Hover(HoverEventFilter::LeftMouseDown), app.clone(), on_plot_down)
        .with_callback(EventFilter::Hover(HoverEventFilter::MouseMove), app.clone(), on_plot_move)
        .with_callback(EventFilter::Hover(HoverEventFilter::MouseUp), app.clone(), on_plot_up)
        .with_callback(EventFilter::Hover(HoverEventFilter::MouseLeave), app.clone(), on_plot_up)
        .with_callback(EventFilter::Hover(HoverEventFilter::Scroll), app.clone(), on_plot_wheel)
        .with_callback(EventFilter::Hover(HoverEventFilter::DoubleClick), app.clone(), on_plot_reset);

    let step = graph::nice_step(vp.scale, 70.0);
    let minor = step / if (step / 10f64.powf(step.log10().floor()) - 2.0).abs() < 1e-9 { 4.0 } else { 5.0 };
    let (x0, x1) = vp.x_range(w);
    let (y0, y1) = vp.y_range(h);
    let major = |v: f64| ((v / step) - (v / step).round()).abs() < 1e-6;
    let (ox, oy) = vp.to_px(0.0, 0.0, w, h);
    let line = |css: &str, vertical: bool, at: f64| {
        let place = if vertical {
            format!("left: {at:.1}px; top: 0px; width: 1px; height: {h:.0}px;")
        } else {
            format!("top: {at:.1}px; left: 0px; height: 1px; width: {w:.0}px;")
        };
        Dom::create_div().with_css(format!("{css} {place}"))
    };
    for x in graph::ticks(x0, x1, minor) {
        let (px, _) = vp.to_px(x, 0.0, w, h);
        let css = if major(x) { look::GRID_MAJOR } else { look::GRID_MINOR };
        plot.add_child(line(css, true, px.round()));
    }
    for y in graph::ticks(y0, y1, minor) {
        let (_, py) = vp.to_px(0.0, y, w, h);
        let css = if major(y) { look::GRID_MAJOR } else { look::GRID_MINOR };
        plot.add_child(line(css, false, py.round()));
    }
    let x_axis_visible = (0.0..=f64::from(h)).contains(&oy);
    let y_axis_visible = (0.0..=f64::from(w)).contains(&ox);
    if x_axis_visible {
        plot.add_child(line(look::AXIS, false, oy.round()));
    }
    if y_axis_visible {
        plot.add_child(line(look::AXIS, true, ox.round()));
    }
    // The numbers: under the x axis (or along the bottom when it is out of
    // view), left of the y axis (or along the left edge).
    let label_y = if x_axis_visible { (oy + 3.0).min(f64::from(h) - 16.0) } else { f64::from(h) - 16.0 };
    let label_x = if y_axis_visible { (ox + 4.0).min(f64::from(w) - 40.0) } else { 4.0 };
    for x in graph::ticks(x0, x1, step) {
        if x.abs() < step * 1e-6 {
            continue;
        }
        let (px, _) = vp.to_px(x, 0.0, w, h);
        if px < 12.0 || px > f64::from(w) - 24.0 {
            continue;
        }
        plot.add_child(tick(&graph::tick_label(x, step), px + 3.0, label_y));
    }
    for y in graph::ticks(y0, y1, step) {
        if y.abs() < step * 1e-6 {
            continue;
        }
        let (_, py) = vp.to_px(0.0, y, w, h);
        if py < 8.0 || py > f64::from(h) - 20.0 {
            continue;
        }
        plot.add_child(tick(&graph::tick_label(y, step), label_x, py - 15.0));
    }
    if x_axis_visible && y_axis_visible {
        plot.add_child(tick("0", ox + 4.0, oy + 3.0));
    }

    let curves: Vec<Curve> = functions(&s.calc)
        .into_iter()
        .map(|(i, tokens)| Curve {
            id: ids::curve(i),
            d: curve_of(&tokens, g),
            color: i.map_or(look.draft(), |i| look.curve(i)).to_string(),
            width: if i.is_some() { 2.2 } else { 1.6 },
            draft: i.is_none(),
        })
        .collect();
    if curves.iter().any(|c| !c.d.is_empty()) {
        match Xml::from_str(graph::svg(w, h, &curves)) {
            ResultXmlXmlError::Ok(xml) => plot.add_child(Dom::create_from_parsed_xml_fragment(xml)),
            ResultXmlXmlError::Err(_) => eprintln!("[azcalculator] the graph's SVG did not parse"),
        }
    }
    plot
}

fn tick(text: &str, x: f64, y: f64) -> Dom {
    Dom::create_div_with_text(AzString::from(text))
        .with_class(ids::TICK)
        .with_css(format!("{} left: {x:.0}px; top: {y:.0}px;", look::TICK))
}

// ==== The plot's callbacks ====

/// The plot's rectangle in the window.
fn plot_rect(info: &CallbackInfo) -> Option<LogicalRect> {
    let dom = DomId { inner: 0 };
    let node = info.get_node_id_by_id_attribute(dom, ids::PLOT);
    if node.into_raw() == 0 {
        return None;
    }
    info.get_node_rect(DomNodeId { dom, node }).into_option()
}

/// The pointer over the plot, in the plot's pixels (inside its border).
fn pointer_in_plot(info: &CallbackInfo) -> Option<(f64, f64)> {
    let rect = plot_rect(info)?;
    let at = info.get_cursor_position().into_option()?;
    Some((
        f64::from(at.x - rect.origin.x - 1.0),
        f64::from(at.y - rect.origin.y - 1.0),
    ))
}

/// The plot was laid out (or resized): draw at its size. The first size
/// fits -10..10 across it; later ones show more or less of the plane.
extern "C" fn on_plot_size(mut data: RefAny, info: CallbackInfo) -> Update {
    let Some(rect) = plot_rect(&info) else {
        return Update::DoNothing;
    };
    // Inside the 1 px border.
    let (w, h) = ((rect.size.width - 2.0).round(), (rect.size.height - 2.0).round());
    if w < 16.0 || h < 16.0 {
        return Update::DoNothing;
    }
    let Some(mut s) = data.downcast_mut::<CalcApp>() else {
        return Update::DoNothing;
    };
    let g = &mut s.graph;
    if g.measured && (g.px.0 - w).abs() < 1.0 && (g.px.1 - h).abs() < 1.0 {
        return Update::DoNothing;
    }
    if !g.measured {
        let (cx, cy) = (g.vp.cx, g.vp.cy);
        g.vp = Viewport::fresh(w);
        g.vp.cx = cx;
        g.vp.cy = cy;
        g.measured = true;
    }
    g.px = (w, h);
    g.announce();
    Update::RefreshDom
}

extern "C" fn on_plot_down(mut data: RefAny, info: CallbackInfo) -> Update {
    let Some(at) = info.get_cursor_position().into_option() else {
        return Update::DoNothing;
    };
    if let Some(mut s) = data.downcast_mut::<CalcApp>() {
        let vp = s.graph.vp;
        s.graph.drag = Some((at.x, at.y, vp));
    }
    Update::DoNothing
}

extern "C" fn on_plot_move(mut data: RefAny, info: CallbackInfo) -> Update {
    let Some(mut s) = data.downcast_mut::<CalcApp>() else {
        return Update::DoNothing;
    };
    let Some((x, y, start)) = s.graph.drag else {
        return Update::DoNothing;
    };
    if !info.get_current_mouse_state().left_down {
        s.graph.drag = None;
        s.graph.announce();
        return Update::DoNothing;
    }
    let Some(at) = info.get_cursor_position().into_option() else {
        return Update::DoNothing;
    };
    let (dx, dy) = (f64::from(at.x - x), f64::from(at.y - y));
    if dx.abs() < 0.5 && dy.abs() < 0.5 {
        return Update::DoNothing;
    }
    s.graph.vp = Viewport::dragged(start, dx, dy);
    Update::RefreshDom
}

extern "C" fn on_plot_up(mut data: RefAny, _info: CallbackInfo) -> Update {
    if let Some(mut s) = data.downcast_mut::<CalcApp>() {
        if s.graph.drag.take().is_some() {
            s.graph.announce();
        }
    }
    Update::DoNothing
}

/// The wheel zooms about the pointer (up: closer).
extern "C" fn on_plot_wheel(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let hit = info.get_hit_node();
    let Some(delta) = info.get_scroll_delta(hit.dom, NodeId { inner: 0 }).into_option() else {
        return Update::DoNothing;
    };
    if delta.y == 0.0 {
        return Update::DoNothing;
    }
    let pointer = pointer_in_plot(&info);
    info.prevent_default();
    info.stop_propagation();
    let Some(mut s) = data.downcast_mut::<CalcApp>() else {
        return Update::DoNothing;
    };
    let (w, h) = s.graph.px;
    let (px, py) = pointer.unwrap_or((f64::from(w) / 2.0, f64::from(h) / 2.0));
    let factor = (-f64::from(delta.y) * 0.002).exp();
    s.graph.vp.zoom_at(factor, px, py, w, h);
    s.graph.announce();
    Update::RefreshDom
}

/// Double-click, or Reset view: -10..10 around the origin again.
extern "C" fn on_plot_reset(mut data: RefAny, _info: CallbackInfo) -> Update {
    let Some(mut s) = data.downcast_mut::<CalcApp>() else {
        return Update::DoNothing;
    };
    s.graph.vp = Viewport::fresh(s.graph.px.0);
    s.graph.announce();
    Update::RefreshDom
}

extern "C" fn on_remove_function(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, index)) = data.downcast_ref::<FunctionRef>().map(|f| (f.app.clone(), f.index)) else {
        return Update::DoNothing;
    };
    with_app(&mut app, &mut info, |s, _info, _| {
        s.calc.remove_plot(index);
        s.announce_plots();
    })
}

extern "C" fn on_clear_functions(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_app(&mut data, &mut info, |s, _info, _| {
        s.calc.clear_plots();
        s.announce_plots();
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_entry_is_plotted_live_once_it_has_x() {
        let mut c = Calculator::new();
        c.type_text("sin(x", 1);
        let f = functions(&c);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].0, None, "the entry, not a committed function");
        c.type_text(")=", 1);
        let f = functions(&c);
        assert_eq!(f, vec![(Some(0), c.plots[0].tokens.clone())]);
        let g = GraphState::default();
        assert!(curve_of(&f[0].1, &g).starts_with('M'));
        assert_eq!(curve_of(&[Tok::LParen], &g), "", "nothing to draw yet");
    }
}
