#![cfg(feature = "text_layout")]
//! A ribbon tab wider than its window keeps every control inside it.
//!
//! AzDrive's E2E: in a 1280 px window Explorer's View tab (Panes, the Layout
//! gallery, Current view, Show/hide, Options) put the Options button at
//! x = 1378, outside the window, and the click meant for it hit nothing;
//! Show/hide was cut mid-group. Office scales a tab that does not fit: its
//! groups shrink right to left - large buttons to small ones, galleries to
//! fewer cells, buttons to their icons - and a group collapses into one
//! button that opens it in a popup. Laid out in windows from 1280 px down to
//! 600 px, in both looks, every group, gallery and button the ribbon shows
//! lies inside the window.
//!
//! The ribbon is built the way `layout()` builds it, for its window (a
//! `WindowSizeScope`): the width source every app gets without passing one.
//!
//! Not compiled by the author (house rule).

use azul_core::{
    callbacks::WindowSizeScope,
    dom::{Dom, DomId, DomNodeId, NodeId, NodeType},
    geom::{LogicalRect, LogicalSize},
    resources::RendererResources,
    styled_dom::{NodeHierarchyItemId, StyledDom},
};
use azul_css::AzString;
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    widgets::{
        check_box::CheckBox,
        ribbon::{
            Ribbon, RibbonArrow, RibbonButton, RibbonColumn, RibbonGallery, RibbonGalleryCell,
            RibbonGroup, RibbonItem, RibbonRow, RibbonTab, RIBBON_GROUP_COLLAPSED_CLASS,
        },
        themes::UiTheme,
    },
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const HEIGHT: f32 = 700.0;

fn s(text: &str) -> AzString {
    AzString::from(text)
}

fn button(icon: &str, label: &str) -> RibbonButton {
    RibbonButton::new(s(icon), s(label))
}

fn large(icon: &str, label: &str) -> RibbonItem {
    RibbonItem::LargeButton(button(icon, label))
}

fn small(icon: &str, label: &str) -> RibbonItem {
    RibbonItem::SmallButton(button(icon, label))
}

fn small_menu(icon: &str, label: &str) -> RibbonItem {
    RibbonItem::SmallButton(button(icon, label).with_arrow(RibbonArrow::Menu))
}

fn row(items: Vec<RibbonItem>) -> RibbonItem {
    RibbonItem::Row(RibbonRow::new().with_items(items.into()))
}

/// A check box and its label, as AzDrive's Show/hide builds them.
fn check(label: &str) -> RibbonItem {
    row(vec![
        RibbonItem::Check(CheckBox::create(false)),
        RibbonItem::Custom(
            Dom::create_span_with_text(s(label)).with_css("font-size: 12px; margin-left: 4px;"),
        ),
    ])
}

/// A gallery of `names`, `visible` of them in the ribbon.
fn gallery(names: &[&str], visible: usize) -> RibbonItem {
    let cells: Vec<RibbonGalleryCell> = names
        .iter()
        .map(|name| {
            RibbonGalleryCell::new(
                Dom::create_icon(s("view_module")).with_css("font-size: 20px;"),
                s(name),
            )
        })
        .collect();
    RibbonItem::Gallery(RibbonGallery::new(cells.into()).with_visible(visible))
}

/// Explorer's View tab, as AzDrive builds it (`examples/azul-drive/src/ui_ribbon.rs`).
fn explorer_view() -> RibbonTab {
    RibbonTab::new(s("View"))
        .with_group(
            RibbonGroup::new(s("Panes"))
                .with_item(large("vertical_split", "Navigation pane"))
                .with_item(small("preview", "Preview pane"))
                .with_item(small("view_sidebar", "Details pane")),
        )
        .with_group(RibbonGroup::new(s("Layout")).with_item(gallery(
            &[
                "Extra large icons",
                "Large icons",
                "Medium icons",
                "Small icons",
                "List",
                "Details",
                "Tiles",
                "Content",
            ],
            4,
        )))
        .with_group(
            RibbonGroup::new(s("Current view"))
                .with_item(RibbonItem::LargeButton(
                    button("sort", "Sort by").with_arrow(RibbonArrow::Menu),
                ))
                .with_item(small_menu("category", "Group by"))
                .with_item(small_menu("view_column", "Add columns"))
                .with_item(small("fit_screen", "Size all columns to fit")),
        )
        .with_group(
            RibbonGroup::new(s("Show/hide"))
                .with_item(check("Item check boxes"))
                .with_item(check("File name extensions"))
                .with_item(check("Hidden items"))
                .with_item(large("visibility_off", "Hide selected items")),
        )
        .with_group(RibbonGroup::new(s("Options")).with_item(large("tune", "Options")))
}

/// AzWriter's Home tab (`examples/azul-writer/src/ribbon.rs`).
fn writer_home() -> RibbonTab {
    let styles: Vec<RibbonGalleryCell> = [
        "Normal",
        "Heading 1",
        "Heading 2",
        "Heading 3",
        "Quote",
        "Code",
    ]
    .iter()
    .map(|name| {
        RibbonGalleryCell::new(
            Dom::create_span_with_text(s("AaBbCc")).with_css("font-size: 13px;"),
            s(name),
        )
    })
    .collect();
    RibbonTab::new(s("HOME"))
        .with_group(RibbonGroup::new(s("Font")).with_item(row(vec![
            small("format_bold", "Bold"),
            small("format_italic", "Italic"),
            small("format_underlined", "Underline"),
            small("strikethrough_s", "Strikethrough"),
            small("code", "Code"),
        ])))
        .with_group(
            RibbonGroup::new(s("Paragraph")).with_item(RibbonItem::Column(
                RibbonColumn::new()
                    .with_item(row(vec![
                        small("format_list_bulleted", "Bullets"),
                        small("format_list_numbered", "Numbering"),
                        small("checklist", "Checklist"),
                        small("format_indent_decrease", "Outdent"),
                        small("format_indent_increase", "Indent"),
                    ]))
                    .with_item(row(vec![
                        small("format_align_left", "Left"),
                        small("format_align_center", "Center"),
                        small("format_align_right", "Right"),
                        small("format_align_justify", "Justify"),
                        small("format_quote", "Quote"),
                    ])),
            )),
        )
        .with_group(RibbonGroup::new(s("Styles")).with_item(RibbonItem::Gallery(
            RibbonGallery::new(styles.into()).with_visible(3),
        )))
        .with_group(
            RibbonGroup::new(s("Editing")).with_item(RibbonItem::Column(
                RibbonColumn::new()
                    .with_item(small("undo", "Undo"))
                    .with_item(small("redo", "Redo")),
            )),
        )
}

/// `tab` in `theme`, built as `layout()` builds it for a window `width` px
/// wide, and laid out in that window.
fn laid_out(tab: RibbonTab, theme: UiTheme, width: f32) -> LayoutWindow {
    let ribbon = {
        let _window = WindowSizeScope::enter(LogicalSize::new(width, HEIGHT));
        Ribbon::new(vec![tab].into())
            .with_theme(theme)
            .dom_desktop()
    };
    let mut dom = Dom::create_body()
        .with_css("display: flex; flex-direction: column; margin: 0px;")
        .with_child(ribbon);
    let styled = StyledDom::create(&mut dom, azul_css::css::Css::empty());
    let mut lw = LayoutWindow::new(FcFontCache::build()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(width, HEIGHT);
    lw.current_window_state = ws.clone();
    let mut debug = None;
    lw.layout_and_generate_display_list(
        styled,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut debug,
    )
    .expect("the ribbon lays out");
    lw
}

/// Every group, gallery and button the ribbon shows, with its laid-out box.
/// A box with no width is not shown: a closed popup's content, the
/// gallery's closed More panel.
fn controls(lw: &LayoutWindow) -> Vec<(String, LogicalRect)> {
    let result = lw
        .get_layout_result(&DomId::ROOT_ID)
        .expect("the root DOM is laid out");
    let mut out = Vec::new();
    for (index, node) in result
        .styled_dom
        .node_data
        .as_container()
        .internal
        .iter()
        .enumerate()
    {
        let what = if matches!(node.get_node_type(), NodeType::Button) {
            "button"
        } else if node.has_class("__azul-native-ribbon-group") {
            "group"
        } else if node.has_class("__azul-native-ribbon-gallery") {
            "gallery"
        } else {
            continue;
        };
        let Some(rect) = lw.get_node_layout_rect(DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(index))),
        }) else {
            continue;
        };
        if rect.size.width > 0.0 {
            out.push((format!("{what} #{index}"), rect));
        }
    }
    out
}

/// How many groups of the laid-out ribbon are collapsed into one button.
fn collapsed(lw: &LayoutWindow) -> usize {
    lw.get_layout_result(&DomId::ROOT_ID)
        .expect("the root DOM is laid out")
        .styled_dom
        .node_data
        .as_container()
        .internal
        .iter()
        .filter(|node| node.has_class(RIBBON_GROUP_COLLAPSED_CLASS))
        .count()
}

/// Every control of `tab` lies inside a window `width` px wide, in `theme`.
fn check_inside(tab: fn() -> RibbonTab, theme: UiTheme, width: f32) {
    let lw = laid_out(tab(), theme, width);
    let found = controls(&lw);
    assert!(
        found.iter().any(|(what, _)| what.starts_with("group")),
        "{theme:?} at {width} px: the walk found the groups"
    );
    for (what, rect) in &found {
        let right = rect.origin.x + rect.size.width;
        assert!(
            rect.origin.x >= -0.5 && right <= width + 0.5,
            "{theme:?} at {width} px: {what} spans x {}..{right}, outside the window",
            rect.origin.x
        );
    }
}

#[test]
fn every_control_of_explorers_view_tab_stays_inside_its_window() {
    for theme in [UiTheme::Flat, UiTheme::Flora] {
        for width in [1280.0, 1000.0, 800.0, 600.0] {
            check_inside(explorer_view, theme, width);
        }
    }
}

#[test]
fn every_control_of_azwriters_home_tab_stays_inside_its_window() {
    for theme in [UiTheme::Flat, UiTheme::Flora] {
        for width in [1280.0, 900.0, 700.0] {
            check_inside(writer_home, theme, width);
        }
    }
}

/// Explorer's View tab fits a 1280 px window as AzDrive built it (its
/// Show/hide check boxes over each other); at 600 px a group is one button.
#[test]
fn a_wide_window_shows_the_tab_as_built_and_a_narrow_one_collapses_groups() {
    assert_eq!(
        collapsed(&laid_out(explorer_view(), UiTheme::Flat, 1280.0)),
        0,
        "at 1280 px nothing collapses"
    );
    assert!(
        collapsed(&laid_out(explorer_view(), UiTheme::Flat, 600.0)) > 0,
        "at 600 px a group is one button that opens it"
    );
}
