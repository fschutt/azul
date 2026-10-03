//! An inline DatePicker fits the pane it sits in.
//!
//! Its cells are 32 px wide - 7 x 32 + 2 x 8 padding + 2 x 1 border = 242 px -
//! and nothing let them give way: AzCalendar's date navigator (a 230 px pane
//! with its own padding) and its To-Do bar cut the calendar's last column off
//! (PIM6, seen again on the wave-6 build). A pane narrower than the calendar
//! must get a calendar as wide as the pane, its seven columns narrower.

use azul_core::{
    dom::{Dom, DomId, NodeId},
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::StyledDom,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, widgets::date_picker::DatePicker, window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// The used widths of the nodes carrying `class`, in document order.
fn widths_of_class(lw: &LayoutWindow, class: &str) -> Vec<f32> {
    let lr = lw.get_layout_result(&DomId::ROOT_ID).expect("root layout");
    let container = lr.styled_dom.node_data.as_container();
    (0..container.len())
        .map(NodeId::new)
        .filter(|nid| {
            container[*nid].attributes().as_ref().iter().any(|a| {
                a.as_class().is_some_and(|c| {
                    let s: &str = c;
                    s == class
                })
            })
        })
        .filter_map(|nid| {
            let idx = *lr.layout_tree.dom_to_layout.get(&nid)?.first()?;
            Some(lr.layout_tree.nodes.get(idx.index())?.used_size?.width)
        })
        .collect()
}

/// An inline picker in a column pane `pane` px wide.
fn laid_out(pane: f32) -> LayoutWindow {
    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    let mut window_state = FullWindowState::default();
    window_state.size.dimensions = LogicalSize::new(640.0, 480.0);
    lw.current_window_state = window_state.clone();

    let mut dom = Dom::create_body().with_child(
        Dom::create_div()
            .with_css(format!(
                "display: flex; flex-direction: column; width: {pane}px;"
            ))
            .with_child(DatePicker::create(2026, 10, 3).with_inline(true).dom()),
    );
    let (css, _) = azul_css::parser2::new_from_str("body { margin: 0; }");
    let styled = StyledDom::create(&mut dom, css);
    let mut dbg = None;
    lw.layout_and_generate_display_list(
        styled,
        &window_state,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut dbg,
    )
    .unwrap();
    lw
}

#[test]
fn an_inline_date_picker_shrinks_to_a_pane_narrower_than_its_calendar() {
    let pane = 200.0;
    let lw = laid_out(pane);
    let panel = widths_of_class(&lw, "__azul-native-date-picker-panel");
    assert_eq!(panel.len(), 1, "one calendar");
    assert!(
        panel[0] <= pane + 0.5,
        "the calendar is no wider than its {pane}px pane, got {}",
        panel[0]
    );
    let weekdays = widths_of_class(&lw, "__azul-native-date-picker-weekday");
    assert_eq!(weekdays.len(), 7, "seven weekday columns");
    let row: f32 = weekdays.iter().sum();
    assert!(
        row <= panel[0] - 17.5,
        "the seven columns fit inside the calendar's padding and border: {row} of {} \
         ({weekdays:?})",
        panel[0]
    );
}

#[test]
fn an_inline_date_picker_keeps_its_size_in_a_wide_pane() {
    let lw = laid_out(400.0);
    let weekdays = widths_of_class(&lw, "__azul-native-date-picker-weekday");
    assert_eq!(weekdays.len(), 7, "seven weekday columns");
    for w in &weekdays {
        assert!(
            (*w - 32.0).abs() < 0.5,
            "a column keeps its 32px where there is room, got {w} ({weekdays:?})"
        );
    }
}
