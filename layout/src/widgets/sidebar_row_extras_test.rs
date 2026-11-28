//! A row's state glyph, the right click that asks for the row's menu, and the drops a row takes
//! (what a file manager's source list needs of the widget).

use azul_core::dom::TabIndex;

use super::*;

fn has_class(dom: &Dom, class: &str) -> bool {
    dom.root
        .get_ids_and_classes()
        .as_ref()
        .iter()
        .any(|c| c.as_class() == Some(class))
}

fn collect<'a>(dom: &'a Dom, class: &str, out: &mut Vec<&'a Dom>) {
    if has_class(dom, class) {
        out.push(dom);
    }
    for child in dom.children.as_ref() {
        collect(child, class, out);
    }
}

fn all<'a>(dom: &'a Dom, class: &str) -> Vec<&'a Dom> {
    let mut v = Vec::new();
    collect(dom, class, &mut v);
    v
}

/// The event a node reports for `filter` (its callback's carrier, read without a window).
fn event_on(dom: &Dom, filter: EventFilter) -> Option<SidebarEvent> {
    dom.root
        .get_callbacks()
        .as_ref()
        .iter()
        .filter(|cb| cb.event == filter)
        .find_map(|cb| {
            let mut data = cb.refany.clone();
            data.downcast_ref::<PartRef>().map(|p| p.event)
        })
}

fn has_callback(dom: &Dom, filter: EventFilter) -> bool {
    dom.root.get_callbacks().as_ref().iter().any(|cb| cb.event == filter)
}

fn drive() -> Sidebar {
    let section = SidebarSection::create(AzString::from("Cloud"))
        .with_row(
            SidebarRow::create(AzString::from("Backups"))
                .with_icon(AzString::from("cloud"))
                .with_status(AzString::from("cloud_done"), AzString::from("Connected"))
                .with_accepts_drops(true)
                .with_tag(7),
        )
        .with_row(SidebarRow::create(AzString::from("Add drive")).with_icon(AzString::from("add_link")).with_tag(8));
    Sidebar::create().with_section(section).with_form(FormFactor::Desktop)
}

#[test]
fn a_row_has_a_state_glyph_with_the_words_it_says() {
    let dom = drive().with_state(SidebarState::Expanded).dom();
    let glyphs = all(&dom, SIDEBAR_STATUS_ICON_CLASS);
    assert_eq!(glyphs.len(), 1, "only the row that has a state");
    let a11y = glyphs[0].root.get_accessibility_info().expect("a name");
    assert_eq!(a11y.accessibility_name.as_ref().map(|n| n.as_str()), Some("Connected"));
}

#[test]
fn a_row_without_a_state_has_no_glyph() {
    assert!(!SidebarRow::create(AzString::from("a")).accepts_drops);
    let dom = drive().with_state(SidebarState::Expanded).dom();
    let rows = all(&dom, SIDEBAR_ROW_CLASS);
    assert!(all(rows[1], SIDEBAR_STATUS_ICON_CLASS).is_empty());
}

#[test]
fn a_right_click_on_a_row_asks_the_app_for_its_menu() {
    let dom = drive().with_state(SidebarState::Expanded).dom();
    let rows = all(&dom, SIDEBAR_ROW_CLASS);
    let ev = event_on(rows[0], EventFilter::Hover(HoverEventFilter::RightMouseUp)).expect("it reports");
    assert_eq!((ev.kind, ev.index, ev.tag), (SidebarEventKind::RowMenuRequested, 0, 7));
    let ev = event_on(rows[1], EventFilter::Hover(HoverEventFilter::RightMouseUp)).expect("it reports");
    assert_eq!((ev.kind, ev.index, ev.tag), (SidebarEventKind::RowMenuRequested, 1, 8));
}

#[test]
fn the_rails_button_asks_for_the_menu_too() {
    let dom = drive().with_state(SidebarState::Rail).dom();
    let buttons = all(&dom, SIDEBAR_BUTTON_CLASS);
    let ev = event_on(buttons[0], EventFilter::Hover(HoverEventFilter::RightMouseUp)).expect("it reports");
    assert_eq!((ev.kind, ev.tag), (SidebarEventKind::RowMenuRequested, 7));
}

#[test]
fn only_a_row_that_takes_drops_listens_for_them() {
    let dom = drive().with_state(SidebarState::Expanded).dom();
    let rows = all(&dom, SIDEBAR_ROW_CLASS);
    assert!(has_callback(rows[0], EventFilter::Hover(HoverEventFilter::DragOver)));
    assert!(has_callback(rows[0], EventFilter::Hover(HoverEventFilter::Drop)));
    assert!(!has_callback(rows[1], EventFilter::Hover(HoverEventFilter::DragOver)));
    assert!(!has_callback(rows[1], EventFilter::Hover(HoverEventFilter::Drop)));
}

#[test]
fn a_drop_on_a_row_reports_the_row() {
    let dom = drive().with_state(SidebarState::Expanded).dom();
    let rows = all(&dom, SIDEBAR_ROW_CLASS);
    let ev = event_on(rows[0], EventFilter::Hover(HoverEventFilter::Drop)).expect("it reports");
    assert_eq!((ev.kind, ev.index, ev.tag), (SidebarEventKind::RowDropped, 0, 7));
}

#[test]
fn the_rail_takes_drops_on_the_button_of_such_a_row() {
    let dom = drive().with_state(SidebarState::Rail).dom();
    let buttons = all(&dom, SIDEBAR_BUTTON_CLASS);
    assert!(has_callback(buttons[0], EventFilter::Hover(HoverEventFilter::Drop)));
    assert!(!has_callback(buttons[1], EventFilter::Hover(HoverEventFilter::Drop)));
}

#[test]
fn the_status_glyph_is_no_tab_stop_of_its_own() {
    let dom = drive().with_state(SidebarState::Expanded).dom();
    let glyph = all(&dom, SIDEBAR_STATUS_ICON_CLASS)[0];
    assert_ne!(glyph.root.get_tab_index(), Some(TabIndex::Auto));
}
