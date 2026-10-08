//! Office's ribbon scaling (`ribbon.rs`, "Ribbon scaling"): a tab too wide
//! for its ribbon scales its groups down step by step, the rightmost group
//! first - large buttons to small ones, galleries to fewer cells, buttons to
//! their icons - and collapses a group into one button that opens the whole
//! group in a popup only when nothing else is left. A command in the popup
//! runs in the app's window once the popup closed; any other control runs
//! where it is, and the app's window rebuilds with it.
//!
//! Not compiled by the author (house rule).

use std::sync::{Arc, Mutex};

use azul_core::{
    callbacks::{
        take_recorded_size_queries, CoreCallback, SizeQueryAxis, SizeQueryOp, WindowSizeScope,
    },
    dom::{
        ComponentEventFilter, DomId, DomNodeId, EventFilter, HoverEventFilter, IdOrClass, NodeId,
        NodeType,
    },
    geom::LogicalSize,
    styled_dom::{NodeHierarchyItemId, StyledDom},
    transient::{TransientAnchor, TransientDismiss},
};
use azul_css::AzString;

use super::*;
use crate::{
    callbacks::{CallbackChange, CallbackInfo},
    widgets::{
        button::ButtonOnClickCallbackType,
        check_box::CheckBox,
        roving::test_support as rv,
        themes::{flora, theme_checks as tc, UiTheme},
    },
};

// ---- the app: commands that tell a log they ran ----

type Log = Arc<Mutex<Vec<&'static str>>>;

/// What a command tells the log when it runs.
struct Clicked {
    log: Log,
    name: &'static str,
}

extern "C" fn clicked(mut data: RefAny, _info: CallbackInfo) -> Update {
    if let Some(c) = data.downcast_ref::<Clicked>() {
        c.log.lock().expect("log").push(c.name);
    }
    Update::RefreshDom
}

fn log() -> Log {
    Arc::new(Mutex::new(Vec::new()))
}

fn heard(log: &Log) -> Vec<&'static str> {
    log.lock().expect("log").clone()
}

fn s(text: &str) -> AzString {
    AzString::from(text)
}

/// A button that tells `log` its `name` when it runs.
fn command(log: &Log, icon: &str, name: &'static str) -> RibbonButton {
    RibbonButton::new(s(icon), s(name)).with_on_click(
        RefAny::new(Clicked {
            log: log.clone(),
            name,
        }),
        clicked as ButtonOnClickCallbackType,
    )
}

// ---- Explorer's View tab, as AzDrive builds it ----

/// Panes: Navigation pane over its label, Preview pane and Details pane
/// beside their icons.
fn panes(log: &Log) -> RibbonGroup {
    RibbonGroup::new(s("Panes"))
        .with_item(RibbonItem::LargeButton(command(
            log,
            "vertical_split",
            "Navigation pane",
        )))
        .with_item(RibbonItem::SmallButton(command(
            log,
            "preview",
            "Preview pane",
        )))
        .with_item(RibbonItem::SmallButton(command(
            log,
            "view_sidebar",
            "Details pane",
        )))
}

/// Layout: `cells` layouts, `visible` of them in the ribbon.
fn layouts(cells: usize, visible: usize) -> RibbonGroup {
    let cells: Vec<RibbonGalleryCell> = (0..cells)
        .map(|i| {
            RibbonGalleryCell::new(
                Dom::create_icon(s("view_module")),
                AzString::from(format!("Layout {i}")),
            )
        })
        .collect();
    RibbonGroup::new(s("Layout")).with_item(RibbonItem::Gallery(
        RibbonGallery::new(RibbonGalleryCellVec::from_vec(cells)).with_visible(visible),
    ))
}

/// Current view: Sort by over its label, two menus and a command beside
/// their icons.
fn current_view(log: &Log) -> RibbonGroup {
    RibbonGroup::new(s("Current view"))
        .with_item(RibbonItem::LargeButton(
            command(log, "sort", "Sort by").with_arrow(RibbonArrow::Menu),
        ))
        .with_item(RibbonItem::SmallButton(
            command(log, "category", "Group by").with_arrow(RibbonArrow::Menu),
        ))
        .with_item(RibbonItem::SmallButton(
            command(log, "view_column", "Add columns").with_arrow(RibbonArrow::Menu),
        ))
        .with_item(RibbonItem::SmallButton(command(
            log,
            "fit_screen",
            "Size all columns to fit",
        )))
}

/// A check box and its label: one row.
fn check_row(label: &str) -> RibbonItem {
    RibbonItem::Row(
        RibbonRow::new()
            .with_item(RibbonItem::Check(CheckBox::create(false)))
            .with_item(RibbonItem::Custom(crate::widgets::widget_p_with_text(s(
                label,
            )))),
    )
}

/// Show/hide: three check boxes over each other beside Hide selected items.
fn show_hide(log: &Log) -> RibbonGroup {
    RibbonGroup::new(s("Show/hide"))
        .with_item(check_row("Item check boxes"))
        .with_item(check_row("File name extensions"))
        .with_item(check_row("Hidden items"))
        .with_item(RibbonItem::LargeButton(command(
            log,
            "visibility_off",
            "Hide selected items",
        )))
}

fn options(log: &Log) -> RibbonGroup {
    RibbonGroup::new(s("Options"))
        .with_item(RibbonItem::LargeButton(command(log, "tune", "Options")))
}

fn view_groups(log: &Log) -> Vec<RibbonGroup> {
    vec![
        panes(log),
        layouts(8, 4),
        current_view(log),
        show_hide(log),
        options(log),
    ]
}

/// The View tab without its check boxes: the parts the theme checks know
/// (the check box is a widget of its own, checked by its own tests).
fn view_groups_without_check_boxes(log: &Log) -> Vec<RibbonGroup> {
    vec![panes(log), layouts(8, 4), current_view(log), options(log)]
}

/// A one-tab ribbon of `groups`, in the flat look.
fn ribbon(groups: Vec<RibbonGroup>) -> Ribbon {
    Ribbon::new(RibbonTabVec::from_vec(vec![
        RibbonTab::new(s("View")).with_groups(RibbonGroupVec::from_vec(groups))
    ]))
    .with_theme(UiTheme::Flat)
}

/// The flat ribbon of `groups`, built for a ribbon `width` px wide.
fn built(groups: Vec<RibbonGroup>, width: f32) -> Dom {
    ribbon(groups).with_available_width(width).dom_desktop()
}

/// The flat ribbon of `groups` at its narrowest step.
fn narrowest(groups: Vec<RibbonGroup>) -> Dom {
    built(groups, 1.0)
}

/// Every step the tab takes, widest first: the groups' scales and the tab's
/// width.
fn steps(groups: &[RibbonGroup]) -> Vec<(Vec<GroupScale>, f32)> {
    let mut out = Vec::new();
    walk_scaling_steps(groups, |scales, total| {
        out.push((scales.to_vec(), total));
        false
    });
    out
}

const fn at(size: GroupSize) -> GroupScale {
    GroupScale {
        size,
        gallery_cells: None,
    }
}

// ---- reading a built ribbon ----

/// The groups of the content band, left to right.
fn band(dom: &Dom) -> Vec<&Dom> {
    tc::find(dom, "__azul-native-ribbon-content")
        .expect("the content band")
        .children
        .as_ref()
        .iter()
        .collect()
}

/// Every non-empty text under `node`, depth first.
fn texts(node: &Dom) -> Vec<String> {
    let mut out = Vec::new();
    if let NodeType::Text(t) = node.root.get_node_type() {
        if !t.as_str().is_empty() {
            out.push(t.as_str().to_string());
        }
    }
    for c in node.children.as_ref() {
        out.extend(texts(c));
    }
    out
}

/// Every icon under `node`, depth first.
fn icons(node: &Dom) -> Vec<String> {
    let mut out = Vec::new();
    if let NodeType::Icon(i) = node.root.get_node_type() {
        out.push(i.as_ref().as_str().to_string());
    }
    for c in node.children.as_ref() {
        out.extend(icons(c));
    }
    out
}

fn name_of(node: &Dom) -> Option<String> {
    node.root.get_accessibility_info().and_then(|i| {
        i.accessibility_name
            .as_ref()
            .map(|n| n.as_str().to_string())
    })
}

/// Every node with its path, its classes and its text: what two builds of
/// the same ribbon agree on.
fn shape(dom: &Dom) -> Vec<(String, Vec<String>, Option<String>)> {
    tc::nodes(dom)
        .into_iter()
        .map(|(path, node)| {
            let classes = node
                .root
                .get_ids_and_classes()
                .as_ref()
                .iter()
                .map(|c| match c {
                    IdOrClass::Class(c) => c.as_str().to_string(),
                    IdOrClass::Id(i) => format!("#{}", i.as_str()),
                })
                .collect();
            let text = match node.root.get_node_type() {
                NodeType::Text(t) => Some(t.as_str().to_string()),
                _ => None,
            };
            (path, classes, text)
        })
        .collect()
}

/// `dom` without its tab strip: what the flora look must lay out as the
/// flat one does (flora cuts its own tab row).
fn without_tab_strip(dom: &Dom) -> Dom {
    let mut d = dom.clone();
    d.children = DomVec::from_vec(
        dom.children
            .as_ref()
            .iter()
            .filter(|c| !tc::has_class(c, "__azul-native-ribbon-tabbar"))
            .cloned()
            .collect(),
    );
    d
}

// ---- clicking in a styled ribbon ----

fn nodes_with_class(styled: &StyledDom, class: &str) -> Vec<NodeId> {
    styled
        .node_data
        .as_ref()
        .iter()
        .enumerate()
        .filter(|(_, nd)| {
            nd.get_ids_and_classes()
                .as_ref()
                .iter()
                .any(|c| matches!(c, IdOrClass::Class(s) if s.as_str() == class))
        })
        .map(|(i, _)| NodeId::new(i))
        .collect()
}

fn id(n: NodeId) -> DomNodeId {
    DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(n)),
    }
}

fn click(styled: &StyledDom, node: NodeId) -> Option<(Update, Vec<CallbackChange>)> {
    rv::fire(
        styled,
        id(node),
        EventFilter::Hover(HoverEventFilter::Click),
    )
}

/// The popup `window` closed: what the app's window hears.
fn dismissed(styled: &StyledDom, window: NodeId) -> Option<(Update, Vec<CallbackChange>)> {
    rv::fire(
        styled,
        id(window),
        EventFilter::Component(ComponentEventFilter::Dismissed),
    )
}

/// `(node, open)` of every `SetTransientWindowOpen` in `changes`.
fn window_writes(changes: &[CallbackChange]) -> Vec<(NodeId, bool)> {
    changes
        .iter()
        .filter_map(|c| match c {
            CallbackChange::SetTransientWindowOpen { node, open } => {
                node.node.into_crate_internal().map(|n| (n, *open))
            }
            _ => None,
        })
        .collect()
}

/// Whether `node` lies in the subtree of `ancestor`.
fn inside(styled: &StyledDom, node: NodeId, ancestor: NodeId) -> bool {
    let hierarchy = styled.node_hierarchy.as_container();
    let mut current = Some(node);
    while let Some(n) = current {
        if n == ancestor {
            return true;
        }
        current = hierarchy[n].parent_id();
    }
    false
}

/// The button named `name` in the popup `window`.
fn popup_button(styled: &StyledDom, window: NodeId, name: &str) -> NodeId {
    let data = styled.node_data.as_container();
    (0..data.len())
        .map(NodeId::new)
        .find(|n| {
            inside(styled, *n, window)
                && matches!(data[*n].get_node_type(), NodeType::Button)
                && data[*n].get_accessibility_info().is_some_and(|a| {
                    a.accessibility_name
                        .as_ref()
                        .is_some_and(|t| t.as_str() == name)
                })
        })
        .unwrap_or_else(|| panic!("the popup holds a button named {name:?}"))
}

/// The node a click on the text `text` in the popup `window` reaches: the
/// text's nearest ancestor that hears clicks.
fn clickable(styled: &StyledDom, window: NodeId, text: &str) -> NodeId {
    let data = styled.node_data.as_container();
    let hierarchy = styled.node_hierarchy.as_container();
    let mut node = (0..data.len())
        .map(NodeId::new)
        .find(|n| {
            inside(styled, *n, window)
                && matches!(data[*n].get_node_type(), NodeType::Text(t) if t.as_str() == text)
        })
        .unwrap_or_else(|| panic!("{text:?} in the popup"));
    loop {
        let hears = data[node]
            .get_callbacks()
            .as_ref()
            .iter()
            .any(|c| c.event == EventFilter::Hover(HoverEventFilter::Click));
        if hears {
            return node;
        }
        node = hierarchy[node]
            .parent_id()
            .unwrap_or_else(|| panic!("{text:?} hears no click"));
    }
}

// ---- the steps ----

#[test]
fn a_group_is_narrower_at_each_size_it_takes() {
    let log = log();
    let group = panes(&log);
    let large = group_px(&group, at(GroupSize::Large));
    let medium = group_px(&group, at(GroupSize::Medium));
    let small = group_px(&group, at(GroupSize::Small));
    assert!(
        medium < large,
        "three to a column, each label beside its icon: {medium} < {large}"
    );
    assert!(small < medium, "the icons alone: {small} < {medium}");

    let gallery = layouts(8, 4);
    let cells = |n: usize| {
        group_px(
            &gallery,
            GroupScale {
                size: GroupSize::Large,
                gallery_cells: Some(n),
            },
        )
    };
    assert_eq!(
        cells(4),
        group_px(&gallery, GroupScale::FULL),
        "the gallery showed four"
    );
    assert!(
        (cells(4) - cells(3) - GALLERY_CELL_PX).abs() < 0.5,
        "a cell fewer is a cell narrower"
    );

    let hide = show_hide(&log);
    assert!(
        group_px(&hide, at(GroupSize::Collapsed)) < group_px(&hide, GroupScale::FULL),
        "one button is narrower than the group"
    );
}

#[test]
fn the_rightmost_group_scales_down_first() {
    let log = log();
    let groups = vec![panes(&log), panes(&log), panes(&log)];
    let s1 = steps(&groups);
    let (full, medium) = (GroupScale::FULL, at(GroupSize::Medium));
    assert_eq!(
        s1[0].0,
        vec![full, full, full],
        "the tab as the app built it"
    );
    assert_eq!(s1[1].0, vec![full, full, medium]);
    assert_eq!(s1[2].0, vec![full, medium, medium]);
    assert_eq!(s1[3].0, vec![medium, medium, medium]);
    assert!(
        s1.windows(2).all(|w| w[1].1 < w[0].1),
        "every step makes the tab narrower"
    );
}

#[test]
fn galleries_show_fewer_cells_before_a_button_loses_its_label_and_a_group_collapses_last() {
    let log = log();
    let groups = view_groups(&log);
    let s1 = steps(&groups);
    // What each step changed: which group, and how.
    let changes: Vec<(usize, &str)> = s1
        .windows(2)
        .map(|w| {
            let i = (0..groups.len())
                .find(|&i| w[0].0[i] != w[1].0[i])
                .expect("a step scales one group");
            let (was, is) = (w[0].0[i], w[1].0[i]);
            let how = if is.size == was.size {
                "cells"
            } else {
                match is.size {
                    GroupSize::Medium => "medium",
                    GroupSize::Small => "small",
                    GroupSize::Collapsed => "collapse",
                    GroupSize::Large => panic!("a step never scales a group up"),
                }
            };
            (i, how)
        })
        .collect();
    let rank = |how: &str| {
        ["medium", "cells", "small", "collapse"]
            .iter()
            .position(|h| *h == how)
            .expect("a kind of step")
    };
    assert!(
        changes.windows(2).all(|w| rank(w[0].1) <= rank(w[1].1)),
        "the large buttons small, then the galleries, then the icons alone, then collapsing: \
         {changes:?}"
    );
    for how in ["medium", "small", "collapse"] {
        let order: Vec<usize> = changes.iter().filter(|c| c.1 == how).map(|c| c.0).collect();
        assert!(
            order.windows(2).all(|w| w[0] > w[1]),
            "{how}: right to left: {order:?}"
        );
    }
    assert!(
        changes.contains(&(1, "cells")),
        "the Layout gallery shows fewer cells: {changes:?}"
    );
    assert!(
        changes.iter().any(|c| c.1 == "collapse"),
        "a narrow enough ribbon collapses groups: {changes:?}"
    );
    let last = &s1.last().expect("the steps").0;
    assert_eq!(
        last[1].gallery_cells,
        Some(GALLERY_MIN_CELLS),
        "a gallery keeps its fewest cells"
    );
}

#[test]
fn a_group_that_fills_the_space_counts_as_its_floor_and_keeps_its_cells() {
    let mut styles = layouts(6, 0);
    styles.fills_space = true;
    assert!(
        (group_px(&styles, GroupScale::FULL) - FILL_GROUP_MIN_PX).abs() < 0.5,
        "it gives way down to its floor: its gallery clips"
    );
    for (scales, _) in steps(&[styles]) {
        assert_eq!(
            scales[0].gallery_cells, None,
            "the ribbon does not cut the cells of a gallery that clips"
        );
    }
}

// ---- the built ribbon ----

#[test]
fn a_tab_that_fits_is_built_as_the_app_built_it() {
    let log = log();
    let full = steps(&view_groups(&log))[0].1;
    let fitted = built(view_groups(&log), full + FIT_SLACK_PX);
    let as_built = ribbon(view_groups(&log)).dom_desktop();
    assert_eq!(shape(&fitted), shape(&as_built));
    assert!(tc::find(&fitted, RIBBON_GROUP_COLLAPSED_CLASS).is_none());
}

#[test]
fn medium_stacks_a_groups_buttons_three_to_a_column_beside_their_labels() {
    let log = log();
    let s1 = steps(&[panes(&log)]);
    assert_eq!(s1[1].0, vec![at(GroupSize::Medium)], "a group's first step");
    let dom = built(vec![panes(&log)], s1[1].1 + FIT_SLACK_PX);
    let group = band(&dom)[0];
    let items = &group.children.as_ref()[0];
    let ch = items.children.as_ref();
    assert_eq!(ch.len(), 1, "one column");
    assert!(tc::has_class(&ch[0], "__azul-native-ribbon-column"));
    assert_eq!(
        texts(&ch[0]),
        vec!["Navigation pane", "Preview pane", "Details pane"]
    );
    assert!(
        tc::find(&ch[0], "__azul-native-ribbon-large-label").is_none(),
        "no label set under an icon: each beside its 16 px icon"
    );
}

#[test]
fn small_shows_every_button_as_its_icon_alone_named_by_its_label() {
    let log = log();
    let dom = narrowest(vec![panes(&log)]);
    let group = band(&dom)[0];
    assert!(
        !tc::has_class(group, RIBBON_GROUP_COLLAPSED_CLASS),
        "one button would be wider than three icons"
    );
    let buttons = tc::find_all(group, "__azul-native-button");
    let names: Vec<Option<String>> = buttons.iter().map(|b| name_of(b)).collect();
    assert_eq!(
        names,
        vec![
            Some("Navigation pane".to_string()),
            Some("Preview pane".to_string()),
            Some("Details pane".to_string()),
        ]
    );
    for b in &buttons {
        assert!(texts(b).is_empty(), "{:?}: its icon alone", name_of(b));
    }
    assert_eq!(texts(group), vec!["Panes"], "the caption stays");
}

#[test]
fn a_collapsed_group_is_one_button_that_opens_the_whole_group_in_a_popup() {
    let log = log();
    let s1 = steps(&view_groups(&log));
    let (scales, total) = s1
        .iter()
        .find(|(scales, _)| scales.iter().any(|g| g.size == GroupSize::Collapsed))
        .cloned()
        .expect("a narrow enough ribbon collapses a group");
    let collapsed: Vec<usize> = (0..scales.len())
        .filter(|&i| scales[i].size == GroupSize::Collapsed)
        .collect();
    assert_eq!(collapsed.len(), 1, "one group at a time");
    let i = collapsed[0];

    let dom = built(view_groups(&log), total + FIT_SLACK_PX);
    let groups = band(&dom);
    assert_eq!(groups.len(), 5);
    for (j, g) in groups.iter().enumerate() {
        assert_eq!(
            tc::has_class(g, RIBBON_GROUP_COLLAPSED_CLASS),
            j == i,
            "group {j}"
        );
        assert!(
            tc::has_class(g, "__azul-native-ribbon-group"),
            "group {j} is a group still"
        );
    }
    let parts = groups[i].children.as_ref();
    assert_eq!(parts.len(), 2, "[the group's button, its popup]");
    let button = &parts[0];
    assert!(matches!(button.root.get_node_type(), NodeType::Button));
    assert!(tc::has_class(button, RIBBON_GROUP_BUTTON_CLASS));
    let label = view_groups(&log)[i].label.as_str().to_string();
    assert_eq!(
        name_of(button).as_deref(),
        Some(label.as_str()),
        "named by the group's label"
    );
    assert_eq!(
        icons(button).last().map(String::as_str),
        Some("arrow_drop_down"),
        "its ▾ says it opens"
    );

    let NodeType::TransientWindow(cfg) = parts[1].root.get_node_type() else {
        panic!("the popup is a window");
    };
    assert!(!cfg.open, "it opens when the button is clicked");
    assert_eq!(cfg.anchor, TransientAnchor::Bottom, "under the group");
    assert_eq!(
        cfg.dismiss,
        TransientDismiss::Outside,
        "a press outside or Escape closes it"
    );
    assert!(tc::has_class(&parts[1], RIBBON_GROUP_POPUP_WINDOW_CLASS));

    // The popup holds the whole group, as a window wide enough draws it.
    let panel = &parts[1].children.as_ref()[0];
    assert!(tc::has_class(panel, RIBBON_GROUP_POPUP_CLASS));
    let whole = &panel.children.as_ref()[0];
    let as_built = ribbon(view_groups(&log)).dom_desktop();
    let wide = band(&as_built)[i];
    assert_eq!(texts(whole), texts(wide));
    assert_eq!(icons(whole), icons(wide));
    assert_eq!(
        tc::find_all(whole, "__azul-native-button").len(),
        tc::find_all(wide, "__azul-native-button").len()
    );
}

#[test]
fn a_collapsed_group_shows_its_own_icon_else_its_first_buttons() {
    let log = log();
    let icon = |dom: &Dom| {
        icons(tc::find(dom, RIBBON_GROUP_BUTTON_CLASS).expect("the group collapsed"))
            .first()
            .cloned()
    };
    let own = narrowest(vec![show_hide(&log).with_icon(s("visibility"))]);
    assert_eq!(icon(&own).as_deref(), Some("visibility"));
    let first = narrowest(vec![show_hide(&log)]);
    assert_eq!(
        icon(&first).as_deref(),
        Some("visibility_off"),
        "Hide selected items' icon"
    );
}

// ---- the popup ----

#[test]
fn a_click_on_a_collapsed_groups_button_opens_its_popup() {
    let log = log();
    let styled = StyledDom::create_from_dom(narrowest(vec![show_hide(&log)]));
    let buttons = nodes_with_class(&styled, RIBBON_GROUP_BUTTON_CLASS);
    let windows = nodes_with_class(&styled, RIBBON_GROUP_POPUP_WINDOW_CLASS);
    assert_eq!((buttons.len(), windows.len()), (1, 1));
    let (_, changes) = click(&styled, buttons[0]).expect("the button takes the click");
    assert_eq!(
        window_writes(&changes),
        vec![(windows[0], true)],
        "the click opens it"
    );
    assert!(heard(&log).is_empty(), "and runs no command");
}

#[test]
fn a_command_in_the_popup_closes_it_and_runs_in_the_apps_window_once() {
    let log = log();
    let styled = StyledDom::create_from_dom(narrowest(vec![show_hide(&log)]));
    let window = nodes_with_class(&styled, RIBBON_GROUP_POPUP_WINDOW_CLASS)[0];
    let hide = popup_button(&styled, window, "Hide selected items");
    let (update, changes) = click(&styled, hide).expect("the command takes the click");
    assert_eq!(
        window_writes(&changes),
        vec![(window, false)],
        "the popup closes"
    );
    assert!(heard(&log).is_empty(), "nothing runs in the popup's window");
    assert_eq!(update, Update::DoNothing);

    let (update, _) = dismissed(&styled, window).expect("the app's window hears the popup close");
    assert_eq!(heard(&log), vec!["Hide selected items"]);
    assert_eq!(
        update,
        Update::RefreshDom,
        "the command's own verdict, in the app's window"
    );
    let _ = dismissed(&styled, window);
    assert_eq!(heard(&log).len(), 1, "a command runs once");
}

#[test]
fn a_control_of_the_popup_that_is_no_command_runs_there_and_rebuilds_the_apps_window_too() {
    let log = log();
    let label = Dom::create_div()
        .with_child(crate::widgets::widget_p_with_text(s("Hidden items")))
        .with_callback(
            EventFilter::Hover(HoverEventFilter::Click),
            RefAny::new(Clicked {
                log: log.clone(),
                name: "Hidden items",
            }),
            CoreCallback {
                cb: clicked as usize,
                ctx: azul_core::refany::OptionRefAny::None,
            },
        );
    let group = RibbonGroup::new(s("Show/hide"))
        .with_item(RibbonItem::Row(
            RibbonRow::new()
                .with_item(RibbonItem::Check(CheckBox::create(false)))
                .with_item(RibbonItem::Custom(label)),
        ))
        .with_item(RibbonItem::LargeButton(command(
            &log,
            "visibility_off",
            "Hide selected items",
        )))
        .with_item(RibbonItem::LargeButton(
            command(&log, "sort", "Sort by").with_arrow(RibbonArrow::Menu),
        ));
    let styled = StyledDom::create_from_dom(narrowest(vec![group]));
    let window = nodes_with_class(&styled, RIBBON_GROUP_POPUP_WINDOW_CLASS)[0];

    let (update, changes) = click(&styled, clickable(&styled, window, "Hidden items"))
        .expect("the app's control takes the click");
    assert_eq!(
        heard(&log),
        vec!["Hidden items"],
        "it runs at once, in the popup"
    );
    assert!(window_writes(&changes).is_empty(), "the popup stays open");
    assert_eq!(
        update,
        Update::RefreshDomAllWindows,
        "the app's window, whose DOM the popup shows, rebuilds too"
    );

    let (update, changes) = click(&styled, popup_button(&styled, window, "Sort by"))
        .expect("the menu button takes the click");
    assert_eq!(
        heard(&log),
        vec!["Hidden items", "Sort by"],
        "a menu opens beside its button, in the popup"
    );
    assert!(window_writes(&changes).is_empty());
    assert_eq!(update, Update::RefreshDomAllWindows);
}

// ---- the width ----

#[test]
fn a_ribbon_built_for_a_window_asks_it_only_about_the_widths_of_its_steps() {
    let log = log();
    let s1 = steps(&view_groups(&log));
    assert!(s1.len() > 3, "the View tab takes steps: {}", s1.len());
    let window = s1[2].1 + FIT_SLACK_PX;
    let _ = take_recorded_size_queries();
    let in_window = {
        let _scope = WindowSizeScope::enter(LogicalSize::new(window, 600.0));
        ribbon(view_groups(&log)).dom_desktop()
    };
    let (queries, _) = take_recorded_size_queries();
    let asked: Vec<f32> = queries.iter().map(|q| q.threshold_px).collect();
    let tried: Vec<f32> = s1[..3]
        .iter()
        .map(|(_, total)| total + FIT_SLACK_PX)
        .collect();
    assert_eq!(asked, tried, "the widest step first, until one fits");
    assert!(queries
        .iter()
        .all(|q| q.axis == SizeQueryAxis::Width && q.op == SizeQueryOp::LessThan));
    assert_eq!(
        queries.iter().map(|q| q.answer).collect::<Vec<_>>(),
        vec![true, true, false]
    );
    assert_eq!(
        shape(&in_window),
        shape(&built(view_groups(&log), window)),
        "built at the step that fits"
    );

    let _ = ribbon(view_groups(&log)).dom_desktop();
    assert!(
        take_recorded_size_queries().0.is_empty(),
        "outside a window's build the ribbon asks nothing"
    );
}

#[test]
fn the_touch_chrome_is_never_scaled() {
    let log = log();
    let dom = ribbon(view_groups(&log))
        .with_available_width(1.0)
        .dom_mobile();
    assert!(tc::find(&dom, RIBBON_GROUP_COLLAPSED_CLASS).is_none());
}

// ---- both looks ----

#[test]
fn flat_and_flora_scale_a_tab_alike_and_flora_keeps_its_invariants() {
    let log = log();
    let narrow = steps(&view_groups_without_check_boxes(&log))
        .last()
        .expect("the steps")
        .1
        + FIT_SLACK_PX;
    let build = |theme: UiTheme| {
        ribbon(view_groups_without_check_boxes(&log))
            .with_theme(theme)
            .with_available_width(narrow)
            .dom_desktop()
    };
    let (flat_dom, flora_dom) = (build(UiTheme::Flat), build(UiTheme::Flora));
    assert!(
        tc::find(&flat_dom, RIBBON_GROUP_COLLAPSED_CLASS).is_some(),
        "the narrowest step collapses groups"
    );
    let moved = flora::chrome_metric_findings(
        &without_tab_strip(&flat_dom),
        &without_tab_strip(&flora_dom),
    );
    assert!(
        moved.is_empty(),
        "the scaled flora ribbon moves:\n  {}",
        moved.join("\n  ")
    );
    tc::assert_theme_invariants("flora ribbon, scaled", &flora_dom);
}

/// A ribbon that follows the app theme is built in both looks and merged
/// node by node: both must take the same steps, and the structure of what
/// the steps build - the collapsed group's button, its popup panel - is the
/// ribbon's base, declared once for every theme.
#[test]
fn a_scaled_ribbon_declares_its_structure_once_for_every_theme() {
    use azul_css::props::property::CssPropertyType;

    use crate::widgets::themes::{
        theme_blocks::checks::{under, BOTH},
        theme_checks::assert_structure_is_shared,
    };

    let log = log();
    let narrow = steps(&view_groups_without_check_boxes(&log))
        .last()
        .expect("the steps")
        .1
        + FIT_SLACK_PX;
    for t in BOTH {
        let dom = under(t, || {
            Ribbon::new(RibbonTabVec::from_vec(vec![RibbonTab::new(s("View"))
                .with_groups(RibbonGroupVec::from_vec(
                    view_groups_without_check_boxes(&log),
                ))]))
            .with_available_width(narrow)
            .dom_desktop()
        });
        assert!(
            tc::find(&dom, RIBBON_GROUP_COLLAPSED_CLASS).is_some(),
            "built for {}: the narrowest step collapses groups",
            t.name()
        );
        assert_structure_is_shared(
            &format!("a scaled ribbon built for {}", t.name()),
            &dom,
            &[(
                "__azul-native-ribbon-tab-active",
                CssPropertyType::Position,
                "flora hangs the Australis curves off its selected tab",
            )],
        );
    }
}
