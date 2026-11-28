//! The app frame: where the sidebar, the ribbon, the body, the Preview and the footer stand, and
//! how a phone's open drawer pushes the main view.

use super::*;
use crate::widgets::{
    preview::{PREVIEW_BOTTOM_CLASS, PREVIEW_RIGHT_CLASS},
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
fn a_desktop_docks_the_preview_at_the_right_below_the_ribbon() {
    let insp = Preview::create(id_div("ignored"), id_div("pane"));
    let dom = frame().with_preview(insp).dom();
    let body = first(&dom, FRAME_BODY_CLASS).expect("the body slot");
    assert!(first(body, PREVIEW_RIGHT_CLASS).is_some());
    // the ribbon is outside the Preview: it runs over the pane's full width
    let header = first(&dom, FRAME_HEADER_CLASS).expect("the header slot");
    assert!(first(header, PREVIEW_RIGHT_CLASS).is_none());
}

#[test]
fn the_frames_body_replaces_the_previews_own() {
    let insp = Preview::create(id_div("ignored"), id_div("pane"));
    let dom = frame().with_preview(insp).dom();
    assert!(has_id(&dom, "body"));
    assert!(has_id(&dom, "pane"));
    assert!(!has_id(&dom, "ignored"));
}

#[test]
fn a_phone_docks_the_preview_at_the_bottom() {
    let insp = Preview::create(id_div("ignored"), id_div("pane"));
    let dom = frame().with_form(FormFactor::Phone).with_preview(insp).dom();
    assert!(first(&dom, PREVIEW_BOTTOM_CLASS).is_some());
    assert!(first(&dom, PREVIEW_RIGHT_CLASS).is_none());
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

// ---- a phone's rail: below the tab row, beside everything else ----

use azul_css::props::property::CssProperty;

fn margin_left(dom: &Dom) -> Option<f32> {
    dom.root
        .style
        .iter_inline_properties()
        .filter_map(|(p, _)| match p {
            CssProperty::MarginLeft(v) => v.get_property().map(|m| m.inner.number.get()),
            _ => None,
        })
        .last()
}

fn top_of(dom: &Dom) -> Option<f32> {
    dom.root
        .style
        .iter_inline_properties()
        .filter_map(|(p, _)| match p {
            CssProperty::Top(v) => v.get_property().map(|t| t.inner.number.get()),
            _ => None,
        })
        .last()
}

fn phone() -> AppFrame {
    frame().with_form(FormFactor::Phone).with_footer(id_div("footer"))
}

#[test]
fn a_phones_rail_stands_below_the_tab_row_out_of_the_flow() {
    let dom = phone().with_sidebar(sidebar(SidebarState::Rail)).dom();
    let slot = first(&dom, FRAME_RAIL_CLASS).expect("the rail's slot");
    assert!(first(slot, crate::widgets::sidebar::SIDEBAR_CLASS).is_some());
    assert_eq!(top_of(slot), Some(48.0), "below the touch tab row");
    // the ribbon is the main column's first child, over the rail's whole width: the tab row stays full
    let main = first(&dom, FRAME_MAIN_CLASS).expect("the main column");
    assert!(has_class(&main.children.as_ref()[0], FRAME_HEADER_CLASS));
    assert_eq!(margin_left(&main.children.as_ref()[0]), None, "the tab row is not inset");
    let handle = phone().with_sidebar(sidebar(SidebarState::Handle)).dom();
    assert!(first(&handle, FRAME_RAIL_CLASS).is_some());
}

#[test]
fn an_app_with_a_taller_status_band_says_where_the_rail_starts() {
    let dom = phone().with_rail_top(72.0).with_sidebar(sidebar(SidebarState::Rail)).dom();
    assert_eq!(top_of(first(&dom, FRAME_RAIL_CLASS).expect("the slot")), Some(72.0));
    for bad in [f32::NAN, -5.0, f32::INFINITY, 0.0] {
        let dom = phone().with_rail_top(bad).with_sidebar(sidebar(SidebarState::Rail)).dom();
        assert_eq!(top_of(first(&dom, FRAME_RAIL_CLASS).expect("the slot")), Some(48.0), "{bad}");
    }
}

#[test]
fn the_body_and_the_footer_keep_the_rails_room_and_the_tab_content_is_told_the_same_inset() {
    let rail = phone().with_sidebar(sidebar(SidebarState::Rail));
    assert!((rail.rail_inset() - 18.0).abs() < 0.001);
    let dom = rail.dom();
    assert_eq!(margin_left(first(&dom, FRAME_BODY_CLASS).expect("the body slot")), Some(18.0));
    assert_eq!(margin_left(first(&dom, FRAME_FOOTER_CLASS).expect("the footer slot")), Some(18.0));
    let handle = phone().with_sidebar(sidebar(SidebarState::Handle));
    assert!((handle.rail_inset() - 16.0).abs() < 0.001);
}

#[test]
fn nothing_is_inset_with_the_drawer_open_on_a_desktop_or_without_a_sidebar() {
    let open = phone().with_sidebar(sidebar(SidebarState::Expanded));
    assert!(open.rail_inset().abs() < 0.001);
    let dom = open.dom();
    assert!(first(&dom, FRAME_RAIL_CLASS).is_none(), "the drawer is a column of the frame, full height");
    assert_eq!(margin_left(first(&dom, FRAME_BODY_CLASS).expect("the body slot")), None);
    let desktop = frame().with_sidebar(sidebar(SidebarState::Rail));
    assert!(desktop.rail_inset().abs() < 0.001);
    let dom = desktop.dom();
    assert!(first(&dom, FRAME_RAIL_CLASS).is_none());
    assert_eq!(margin_left(first(&dom, FRAME_BODY_CLASS).expect("the body slot")), None);
    assert!(phone().rail_inset().abs() < 0.001, "no sidebar: no inset");
}

#[test]
fn an_app_that_adds_no_preview_has_no_dock_no_grip_and_no_bottom_pane() {
    use crate::widgets::preview::{PREVIEW_CLASS, PREVIEW_GRIP_CLASS, PREVIEW_PANE_CLASS};
    for form in [FormFactor::Desktop, FormFactor::Phone] {
        let dom = frame().with_form(form).with_sidebar(sidebar(SidebarState::Rail)).dom();
        for class in [PREVIEW_CLASS, PREVIEW_PANE_CLASS, PREVIEW_GRIP_CLASS, PREVIEW_BOTTOM_CLASS, PREVIEW_RIGHT_CLASS] {
            assert!(first(&dom, class).is_none(), "{form:?}: {class}");
        }
    }
}

#[test]
fn the_frame_root_has_the_dom_id_the_app_gave_and_none_otherwise() {
    let has_root_id = |dom: &Dom, id: &str| dom.root.get_ids_and_classes().as_ref().iter().any(|c| c.as_id() == Some(id));
    let named = frame().with_dom_id(AzString::from("mail-frame")).dom();
    assert!(has_root_id(&named, "mail-frame") && has_class(&named, FRAME_CLASS));
    let plain = frame().dom();
    assert!(!plain.root.get_ids_and_classes().as_ref().iter().any(|c| c.as_id().is_some()));
}

#[test]
fn a_frame_is_one_node_tall_and_wide() {
    let dom = frame().dom();
    let said = format!("{:?}", dom.root.style);
    assert!(said.contains("100"), "{said}");
}

// ---- the label convention: no state on a bare text node (the family's own copy of
// `widgets::label_convention`, which lists the older widgets) ----

use azul_core::dom::NodeType;

/// What a text node carries that only a real box can honour.
fn state_on_text(node: &Dom) -> Vec<&'static str> {
    let mut found = Vec::new();
    if !node.root.style.rules.as_ref().is_empty() {
        found.push("css props");
    }
    if !node.css.as_ref().is_empty() {
        found.push("subtree css");
    }
    if !node.root.get_callbacks().as_ref().is_empty() {
        found.push("callbacks");
    }
    if node.root.get_tab_index().is_some() {
        found.push("tab_index");
    }
    if node.root.get_dataset().is_some() {
        found.push("dataset");
    }
    if !node.children.as_ref().is_empty() {
        found.push("children");
    }
    found
}

fn walk_text(node: &Dom, what: &str, bad: &mut Vec<String>) {
    if let NodeType::Text(text) = node.root.get_node_type() {
        let found = state_on_text(node);
        if !found.is_empty() {
            bad.push(format!("{what}: text {:?} carries {}", text.as_ref().as_str(), found.join(" + ")));
        }
    }
    for child in node.children.as_ref() {
        walk_text(child, what, bad);
    }
}

#[test]
fn no_widget_of_the_family_puts_state_on_a_bare_text_node() {
    use crate::widgets::{
        action_sheet::{ActionSheet, ActionSheetItem},
        location_bar::{LocationBar, LocationCrumb},
        preview::{Preview, PreviewBanner, PreviewBannerTone, PreviewDetail},
        pie_marker::PieTone,
        sidebar::{SidebarCollab, SidebarQuickAction, SidebarQuickState, SidebarStatus},
    };
    let rich = |state: SidebarState, form: FormFactor| {
        sidebar(state)
            .with_form(form)
            .with_help(AzString::from("Help"))
            .with_header(id_div("header"))
            .with_status(SidebarStatus::create(AzString::from("Sync"), 0.5, PieTone::Busy).with_text(AzString::from("50 %")))
            .with_status(SidebarStatus::badge(AzString::from("Unread"), AzString::from("12"), PieTone::Danger))
            .with_quick_action(
                SidebarQuickAction::create(AzString::from("Bold"), AzString::from("format_bold"))
                    .with_state(SidebarQuickState::On),
            )
            .with_section(
                SidebarSection::create(AzString::from("Docs"))
                    .with_collapsible(true)
                    .with_row(
                        SidebarRow::create(AzString::from("a.xlsx"))
                            .with_icon(AzString::from("table_chart"))
                            .with_badge(AzString::from("3"))
                            .with_pie(0.4, PieTone::Good)
                            .with_collab(SidebarCollab::On)
                            .with_disclosure(true),
                    ),
            )
    };
    let location = |editing: bool| {
        LocationBar::create(AzString::from("Fonts"))
            .with_parent(LocationCrumb::create(AzString::from("azul"), 1))
            .with_editing(editing, AzString::from("Fo"))
    };
    let sheet = ActionSheet::create()
        .with_title(AzString::from("a.pdf"), AzString::from("today"))
        .with_item(ActionSheetItem::create(AzString::from("Rename"), AzString::from("edit"), 1))
        .with_open(true)
        .with_form(FormFactor::Phone);
    let mut doms: Vec<(&str, Dom)> = Vec::new();
    for (name, state, form) in [
        ("sidebar expanded", SidebarState::Expanded, FormFactor::Desktop),
        ("sidebar rail", SidebarState::Rail, FormFactor::Desktop),
        ("sidebar handle", SidebarState::Handle, FormFactor::Desktop),
        ("sidebar drawer", SidebarState::Expanded, FormFactor::Phone),
    ] {
        doms.push((name, rich(state, form).dom()));
    }
    let rich_preview = |form: FormFactor| {
        Preview::create(id_div("b"), id_div("p"))
            .with_form(form)
            .with_detail(PreviewDetail::create(AzString::from("Properties"), id_div("props")))
            .with_banner(
                PreviewBanner::create(AzString::from("Unverified signer"), PreviewBannerTone::Warning)
                    .with_details(id_div("why")),
            )
    };
    doms.push(("preview right", rich_preview(FormFactor::Desktop).dom()));
    doms.push(("preview bottom", rich_preview(FormFactor::Phone).dom()));
    doms.push(("location", location(false).dom()));
    doms.push(("location editing", location(true).dom()));
    doms.push(("sheet", sheet.dom()));
    doms.push((
        "frame",
        frame().with_sidebar(rich(SidebarState::Expanded, FormFactor::Desktop)).with_footer(id_div("f")).dom(),
    ));
    let mut bad = Vec::new();
    for (name, dom) in &doms {
        walk_text(dom, name, &mut bad);
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}
