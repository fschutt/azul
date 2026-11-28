//! The app frame: where the sidebar, the ribbon, the body, the Inspector and the footer stand, and
//! how a phone's open drawer pushes the main view.

use super::*;
use crate::widgets::{
    inspector::{INSPECTOR_BOTTOM_CLASS, INSPECTOR_RIGHT_CLASS},
    sidebar::{SidebarRow, SidebarSection, SIDEBAR_PHONE_CLASS},
};

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

fn first<'a>(dom: &'a Dom, class: &str) -> Option<&'a Dom> {
    all(dom, class).into_iter().next()
}

fn has_id(dom: &Dom, id: &str) -> bool {
    dom.root.get_ids_and_classes().as_ref().iter().any(|c| c.as_id() == Some(id))
        || dom.children.as_ref().iter().any(|c| has_id(c, id))
}

fn id_div(id: &str) -> Dom {
    Dom::create_div().with_id(AzString::from(id))
}

fn sidebar(state: SidebarState) -> Sidebar {
    Sidebar::create()
        .with_section(
            SidebarSection::create(AzString::from("S"))
                .with_row(SidebarRow::create(AzString::from("Inbox")).with_icon(AzString::from("inbox"))),
        )
        .with_state(state)
}

fn frame() -> AppFrame {
    AppFrame::create(id_div("ribbon"), id_div("body")).with_form(FormFactor::Desktop)
}

#[test]
fn a_frame_without_a_sidebar_is_the_main_column_alone() {
    let dom = frame().dom();
    assert!(has_class(&dom, FRAME_CLASS));
    assert_eq!(dom.children.as_ref().len(), 1);
    assert!(has_class(&dom.children.as_ref()[0], FRAME_MAIN_CLASS));
}

#[test]
fn the_ribbon_runs_over_the_body_and_the_footer_is_under_it() {
    let dom = frame().with_footer(id_div("footer")).dom();
    let main = first(&dom, FRAME_MAIN_CLASS).expect("the main column");
    let kids = main.children.as_ref();
    assert!(has_class(&kids[0], FRAME_HEADER_CLASS) && has_id(&kids[0], "ribbon"));
    assert!(has_class(&kids[1], FRAME_BODY_CLASS) && has_id(&kids[1], "body"));
    assert!(has_class(&kids[2], FRAME_FOOTER_CLASS) && has_id(&kids[2], "footer"));
}

#[test]
fn the_sidebar_is_the_first_column_beside_the_ribbon_not_under_it() {
    let dom = frame().with_sidebar(sidebar(SidebarState::Rail)).dom();
    assert_eq!(dom.children.as_ref().len(), 2);
    assert!(has_class(&dom.children.as_ref()[0], crate::widgets::sidebar::SIDEBAR_CLASS));
    assert!(has_class(&dom.children.as_ref()[1], FRAME_MAIN_CLASS));
    // the ribbon is inside the main column, so the sidebar runs up to the window's top edge
    assert!(!has_id(&dom.children.as_ref()[0], "ribbon"));
    assert!(has_id(&dom.children.as_ref()[1], "ribbon"));
}

#[test]
fn a_desktop_docks_the_inspector_at_the_right_below_the_ribbon() {
    let insp = Inspector::create(id_div("ignored"), id_div("pane"));
    let dom = frame().with_inspector(insp).dom();
    let body = first(&dom, FRAME_BODY_CLASS).expect("the body slot");
    assert!(first(body, INSPECTOR_RIGHT_CLASS).is_some());
    // the ribbon is outside the Inspector: it runs over the pane's full width
    let header = first(&dom, FRAME_HEADER_CLASS).expect("the header slot");
    assert!(first(header, INSPECTOR_RIGHT_CLASS).is_none());
}

#[test]
fn the_frames_body_replaces_the_inspectors_own() {
    let insp = Inspector::create(id_div("ignored"), id_div("pane"));
    let dom = frame().with_inspector(insp).dom();
    assert!(has_id(&dom, "body"));
    assert!(has_id(&dom, "pane"));
    assert!(!has_id(&dom, "ignored"));
}

#[test]
fn a_phone_docks_the_inspector_at_the_bottom() {
    let insp = Inspector::create(id_div("ignored"), id_div("pane"));
    let dom = frame().with_form(FormFactor::Phone).with_inspector(insp).dom();
    assert!(first(&dom, INSPECTOR_BOTTOM_CLASS).is_some());
    assert!(first(&dom, INSPECTOR_RIGHT_CLASS).is_none());
}

#[test]
fn the_form_factor_goes_to_the_sidebar_too() {
    let dom = frame().with_form(FormFactor::Phone).with_sidebar(sidebar(SidebarState::Rail)).dom();
    assert!(has_class(&dom, FRAME_PHONE_CLASS));
    assert!(first(&dom, SIDEBAR_PHONE_CLASS).is_some());
    let desktop = frame().with_sidebar(sidebar(SidebarState::Rail)).dom();
    assert!(!has_class(&desktop, FRAME_PHONE_CLASS));
    assert!(first(&desktop, SIDEBAR_PHONE_CLASS).is_none());
}

#[test]
fn a_phones_open_drawer_pushes_the_main_view_and_a_tap_on_what_is_left_closes_it() {
    let dom = frame()
        .with_form(FormFactor::Phone)
        .with_sidebar(sidebar(SidebarState::Expanded))
        .dom();
    let catcher = first(&dom, FRAME_CATCHER_CLASS).expect("a catcher over the pushed view");
    // it sits in the main column, so it covers exactly the view that was pushed
    let main = first(&dom, FRAME_MAIN_CLASS).expect("the main column");
    assert!(first(main, FRAME_CATCHER_CLASS).is_some());
    assert_eq!(catcher.root.get_callbacks().as_ref().len(), 0, "no callback without a sidebar callback to tell");
}

#[test]
fn the_catcher_asks_the_app_for_the_rail() {
    extern "C" fn nothing(_: RefAny, _: CallbackInfo, _: SidebarEvent) -> Update {
        Update::DoNothing
    }
    let dom = frame()
        .with_form(FormFactor::Phone)
        .with_sidebar(sidebar(SidebarState::Expanded).with_on_event(RefAny::new(()), nothing as crate::widgets::sidebar::SidebarOnEventCallbackType))
        .dom();
    let catcher = first(&dom, FRAME_CATCHER_CLASS).expect("a catcher");
    let cbs = catcher.root.get_callbacks().as_ref();
    assert_eq!(cbs.len(), 1);
    let mut data = cbs[0].refany.clone();
    assert!(data.downcast_ref::<CatcherRef>().is_some());
}

#[test]
fn a_rail_on_a_phone_pushes_nothing_and_a_desktop_never_does() {
    let rail = frame().with_form(FormFactor::Phone).with_sidebar(sidebar(SidebarState::Rail)).dom();
    assert!(first(&rail, FRAME_CATCHER_CLASS).is_none());
    let desktop = frame().with_sidebar(sidebar(SidebarState::Expanded)).dom();
    assert!(first(&desktop, FRAME_CATCHER_CLASS).is_none());
}

#[test]
fn the_sheet_is_the_last_child_of_the_frame() {
    let dom = frame()
        .with_form(FormFactor::Phone)
        .with_sidebar(sidebar(SidebarState::Rail))
        .with_sheet(id_div("sheet"))
        .dom();
    let last = dom.children.as_ref().last().expect("children");
    assert!(has_id(last, "sheet"));
    assert_eq!(dom.children.as_ref().len(), 3);
}

#[test]
fn a_frame_is_one_node_tall_and_wide() {
    let dom = frame().dom();
    let said = format!("{:?}", dom.root.style);
    assert!(said.contains("100"), "{said}");
}
