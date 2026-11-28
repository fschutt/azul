//! The sidebar's DOM: its three states, the rows and the rail, the foot (help, status pies, toggle),
//! the keyboard stop, and what every part reports.

use azul_core::dom::TabIndex;

use super::*;

// ---- helpers ----

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

fn texts(dom: &Dom, out: &mut Vec<String>) {
    if let azul_core::dom::NodeType::Text(t) = dom.root.get_node_type() {
        out.push(t.as_ref().as_str().to_string());
    }
    for child in dom.children.as_ref() {
        texts(child, out);
    }
}

fn text_of(dom: &Dom) -> String {
    let mut v = Vec::new();
    texts(dom, &mut v);
    v.join(" ")
}

/// The event a node's click reports (its callback's carrier, read without a window).
fn click_event(dom: &Dom) -> Option<SidebarEvent> {
    dom.root.get_callbacks().as_ref().iter().find_map(|cb| {
        let mut data = cb.refany.clone();
        data.downcast_ref::<PartRef>().map(|p| p.event)
    })
}

fn name_of(dom: &Dom) -> String {
    dom.root
        .get_accessibility_info()
        .and_then(|a| a.accessibility_name.as_ref().map(|n| n.as_str().to_string()))
        .unwrap_or_default()
}

fn mail() -> Sidebar {
    let favourites = SidebarSection::create(AzString::from("Favourites"))
        .with_row(SidebarRow::create(AzString::from("All")).with_icon(AzString::from("inbox")).with_tag(1))
        .with_row(
            SidebarRow::create(AzString::from("Flagged"))
                .with_icon(AzString::from("flag"))
                .with_badge(AzString::from("329"))
                .with_tag(2),
        );
    let account = SidebarSection::create(AzString::from("Outlook"))
        .with_row(
            SidebarRow::create(AzString::from("Inbox"))
                .with_icon(AzString::from("inbox"))
                .with_selected(true)
                .with_tag(3),
        )
        .with_row(
            SidebarRow::create(AzString::from("Archive"))
                .with_icon(AzString::from("archive"))
                .with_disclosure(false)
                .with_tag(4),
        )
        .with_row(SidebarRow::create(AzString::from("2026")).with_depth(1).with_tag(5));
    Sidebar::create()
        .with_section(favourites)
        .with_section(account)
        .with_form(FormFactor::Desktop)
}

fn foot_names(dom: &Dom) -> Vec<String> {
    first(dom, SIDEBAR_FOOT_CLASS)
        .map(|foot| {
            foot.children
                .as_ref()
                .iter()
                .map(|c| {
                    if has_class(c, SIDEBAR_HELP_CLASS) {
                        "help".to_string()
                    } else if has_class(c, SIDEBAR_TOGGLE_CLASS) {
                        "toggle".to_string()
                    } else {
                        "status".to_string()
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}

// ---- the states ----

#[test]
fn a_new_sidebar_is_the_icon_rail() {
    let sb = Sidebar::create();
    assert_eq!(sb.state, SidebarState::Rail);
    let dom = sb.dom();
    assert!(has_class(&dom, SIDEBAR_CLASS));
    assert!(has_class(&dom, SIDEBAR_RAIL_CLASS));
    assert!(!has_class(&dom, SIDEBAR_EXPANDED_CLASS));
}

#[test]
fn the_rail_has_a_button_for_each_top_level_row_with_an_icon() {
    let dom = mail().with_state(SidebarState::Rail).dom();
    // All, Flagged, Inbox, Archive have icons; "2026" is nested and has none
    assert_eq!(all(&dom, SIDEBAR_BUTTON_CLASS).len(), 4);
    assert!(all(&dom, SIDEBAR_ROW_CLASS).is_empty(), "no expanded rows in the rail");
    assert!(all(&dom, SIDEBAR_SECTION_CLASS).is_empty(), "no section titles in the rail");
}

#[test]
fn the_rail_separates_the_sections_with_a_hairline_and_has_one_over_the_foot() {
    let dom = mail().with_state(SidebarState::Rail).dom();
    // one between the two sections and one over the foot
    assert_eq!(all(&dom, SIDEBAR_RULE_CLASS).len(), 2);
}

#[test]
fn a_row_can_opt_out_of_the_rail() {
    let section = SidebarSection::create(AzString::from("S"))
        .with_row(SidebarRow::create(AzString::from("A")).with_icon(AzString::from("inbox")))
        .with_row(SidebarRow::create(AzString::from("B")).with_icon(AzString::from("flag")).with_in_rail(false));
    let dom = Sidebar::create().with_section(section).with_form(FormFactor::Desktop).dom();
    assert_eq!(all(&dom, SIDEBAR_BUTTON_CLASS).len(), 1);
}

#[test]
fn a_rail_button_is_named_by_its_tooltip_or_its_label_for_a_screen_reader() {
    let section = SidebarSection::create(AzString::from("S"))
        .with_row(SidebarRow::create(AzString::from("Inbox")).with_icon(AzString::from("inbox")))
        .with_row(
            SidebarRow::create(AzString::from("Flg"))
                .with_icon(AzString::from("flag"))
                .with_tooltip(AzString::from("Flagged messages")),
        );
    let dom = Sidebar::create().with_section(section).with_form(FormFactor::Desktop).dom();
    let names: Vec<String> = all(&dom, SIDEBAR_BUTTON_CLASS).into_iter().map(name_of).collect();
    assert_eq!(names, vec!["Inbox".to_string(), "Flagged messages".to_string()]);
}

#[test]
fn the_expanded_sidebar_shows_the_titles_and_every_row_of_an_open_section() {
    let dom = mail().with_state(SidebarState::Expanded).dom();
    assert!(has_class(&dom, SIDEBAR_EXPANDED_CLASS));
    assert_eq!(all(&dom, SIDEBAR_SECTION_CLASS).len(), 2);
    assert_eq!(all(&dom, SIDEBAR_ROW_CLASS).len(), 5);
    assert!(all(&dom, SIDEBAR_BUTTON_CLASS).is_empty(), "no rail buttons when expanded");
    let said = text_of(&dom);
    for word in ["Favourites", "Outlook", "Inbox", "Flagged", "329", "Archive", "2026"] {
        assert!(said.contains(word), "{word} is on the sidebar: {said}");
    }
}

#[test]
fn a_collapsed_section_hides_its_rows_but_the_rows_after_it_keep_their_places() {
    let hidden = SidebarSection::create(AzString::from("Hidden"))
        .with_collapsible(false)
        .with_row(SidebarRow::create(AzString::from("one")))
        .with_row(SidebarRow::create(AzString::from("two")));
    let shown = SidebarSection::create(AzString::from("Shown"))
        .with_row(SidebarRow::create(AzString::from("three")).with_tag(33));
    let dom = Sidebar::create()
        .with_section(hidden)
        .with_section(shown)
        .with_state(SidebarState::Expanded)
        .with_form(FormFactor::Desktop)
        .dom();
    let rows = all(&dom, SIDEBAR_ROW_CLASS);
    assert_eq!(rows.len(), 1, "the closed section's rows are not built");
    let ev = click_event(rows[0]).expect("the row reports");
    assert_eq!(ev.kind, SidebarEventKind::RowClicked);
    assert_eq!(ev.index, 2, "the two hidden rows still count, so the index is the app's own");
    assert_eq!(ev.tag, 33);
}

#[test]
fn a_collapsible_section_title_reports_the_opposite_open_state() {
    let section = SidebarSection::create(AzString::from("Mailboxes"))
        .with_collapsible(true)
        .with_row(SidebarRow::create(AzString::from("a")));
    let dom = Sidebar::create()
        .with_section(section)
        .with_state(SidebarState::Expanded)
        .with_form(FormFactor::Desktop)
        .dom();
    let title = first(&dom, SIDEBAR_SECTION_CLASS).expect("a title");
    let ev = click_event(title).expect("a collapsible title reports");
    assert_eq!(ev.kind, SidebarEventKind::SectionToggled);
    assert_eq!(ev.index, 0);
    assert!(!ev.expand, "it is open now, so the click asks to close it");
}

#[test]
fn a_title_that_does_not_collapse_reports_nothing() {
    let dom = mail().with_state(SidebarState::Expanded).dom();
    let title = first(&dom, SIDEBAR_SECTION_CLASS).expect("a title");
    assert!(click_event(title).is_none());
}

#[test]
fn a_section_without_a_title_has_no_title_row() {
    let section = SidebarSection::create(AzString::from("")).with_row(SidebarRow::create(AzString::from("a")));
    let dom = Sidebar::create()
        .with_section(section)
        .with_state(SidebarState::Expanded)
        .with_form(FormFactor::Desktop)
        .dom();
    assert!(all(&dom, SIDEBAR_SECTION_CLASS).is_empty());
    assert_eq!(all(&dom, SIDEBAR_ROW_CLASS).len(), 1);
}

// ---- rows ----

#[test]
fn the_selected_row_is_marked_for_the_eye_and_for_a_screen_reader() {
    let dom = mail().with_state(SidebarState::Expanded).dom();
    let selected = all(&dom, SIDEBAR_SELECTED_CLASS);
    assert_eq!(selected.len(), 1);
    assert!(text_of(selected[0]).contains("Inbox"));
    let states = &selected[0].root.get_accessibility_info().expect("an a11y info").states;
    assert!(states.as_ref().iter().any(|s| *s == AccessibilityState::Selected));
}

#[test]
fn the_rows_are_one_tab_stop_the_selected_row() {
    let dom = mail().with_state(SidebarState::Expanded).dom();
    let stops: Vec<usize> = all(&dom, SIDEBAR_ROW_CLASS)
        .iter()
        .enumerate()
        .filter(|(_, r)| r.root.get_tab_index() == Some(TabIndex::Auto))
        .map(|(i, _)| i)
        .collect();
    assert_eq!(stops, vec![2], "only the selected row (the third) is a Tab stop");
}

#[test]
fn with_nothing_selected_the_first_row_is_the_tab_stop() {
    let section = SidebarSection::create(AzString::from("S"))
        .with_row(SidebarRow::create(AzString::from("a")))
        .with_row(SidebarRow::create(AzString::from("b")));
    let dom = Sidebar::create()
        .with_section(section)
        .with_state(SidebarState::Expanded)
        .with_form(FormFactor::Desktop)
        .dom();
    let rows = all(&dom, SIDEBAR_ROW_CLASS);
    assert_eq!(rows[0].root.get_tab_index(), Some(TabIndex::Auto));
    assert_eq!(rows[1].root.get_tab_index(), Some(TabIndex::NoKeyboardFocus));
}

#[test]
fn a_tree_row_has_a_disclosure_triangle_that_asks_for_the_other_state() {
    let dom = mail().with_state(SidebarState::Expanded).dom();
    let triangles = all(&dom, SIDEBAR_DISCLOSURE_CLASS);
    assert_eq!(triangles.len(), 1, "only Archive has children");
    let ev = click_event(triangles[0]).expect("the triangle reports");
    assert_eq!(ev.kind, SidebarEventKind::RowToggled);
    assert_eq!((ev.index, ev.tag), (3, 4));
    assert!(ev.expand, "Archive is closed, so a click asks to open it");
}

#[test]
fn an_open_tree_row_asks_to_close() {
    let section = SidebarSection::create(AzString::from("S"))
        .with_row(SidebarRow::create(AzString::from("Folder")).with_disclosure(true));
    let dom = Sidebar::create()
        .with_section(section)
        .with_state(SidebarState::Expanded)
        .with_form(FormFactor::Desktop)
        .dom();
    let t = first(&dom, SIDEBAR_DISCLOSURE_CLASS).expect("a triangle");
    assert!(!click_event(t).expect("reports").expand);
}

#[test]
fn a_row_reports_its_place_and_its_tag() {
    let dom = mail().with_state(SidebarState::Expanded).dom();
    let rows = all(&dom, SIDEBAR_ROW_CLASS);
    let ev = click_event(rows[1]).expect("a row reports");
    assert_eq!(ev.kind, SidebarEventKind::RowClicked);
    assert_eq!((ev.index, ev.tag), (1, 2));
    assert_eq!(ev.state, SidebarState::Expanded);
}

#[test]
fn a_rail_button_reports_the_same_event_as_its_row() {
    let dom = mail().with_state(SidebarState::Rail).dom();
    let buttons = all(&dom, SIDEBAR_BUTTON_CLASS);
    let ev = click_event(buttons[1]).expect("a button reports");
    assert_eq!(ev.kind, SidebarEventKind::RowClicked);
    assert_eq!((ev.index, ev.tag), (1, 2));
}

#[test]
fn rows_past_the_limit_are_dropped_and_what_is_kept_is_the_first() {
    let rows: Vec<SidebarRow> = (0..MAX_ROWS + 10)
        .map(|i| SidebarRow::create(AzString::from(format!("row {i}"))).with_tag(i))
        .collect();
    let section = SidebarSection::create(AzString::from("big")).with_rows(SidebarRowVec::from_vec(rows));
    let (sections, _) = limited(alloc::vec![section], Vec::new());
    let kept = sections[0].rows.as_ref();
    assert_eq!(kept.len(), MAX_ROWS);
    assert_eq!(kept[0].tag, 0);
    assert_eq!(kept[MAX_ROWS - 1].tag, MAX_ROWS - 1);
}

#[test]
fn the_limit_counts_across_the_sections() {
    let make = |n: usize| {
        SidebarSection::create(AzString::from("s")).with_rows(SidebarRowVec::from_vec(
            (0..n).map(|_| SidebarRow::create(AzString::from("r"))).collect(),
        ))
    };
    let (sections, _) = limited(alloc::vec![make(MAX_ROWS - 1), make(10)], Vec::new());
    let total: usize = sections.iter().map(|s| s.rows.as_ref().len()).sum();
    assert_eq!(total, MAX_ROWS);
    assert_eq!(sections[1].rows.as_ref().len(), 1);
}

#[test]
fn more_status_pies_than_the_foot_has_room_for_are_dropped() {
    let statuses: Vec<SidebarStatus> = (0..MAX_STATUSES + 3)
        .map(|i| SidebarStatus::create(AzString::from(format!("s{i}")), 0.5, PieTone::Accent))
        .collect();
    let (_, kept) = limited(Vec::new(), statuses);
    assert_eq!(kept.len(), MAX_STATUSES);
}

#[test]
fn a_row_nested_deeper_than_the_limit_is_still_built() {
    let section = SidebarSection::create(AzString::from("S"))
        .with_row(SidebarRow::create(AzString::from("deep")).with_depth(MAX_DEPTH + 100));
    let dom = Sidebar::create()
        .with_section(section)
        .with_state(SidebarState::Expanded)
        .with_form(FormFactor::Desktop)
        .dom();
    assert_eq!(all(&dom, SIDEBAR_ROW_CLASS).len(), 1);
}

#[test]
fn a_checked_row_shows_a_check_box_before_its_icon() {
    let section = SidebarSection::create(AzString::from("Calendars"))
        .with_row(SidebarRow::create(AzString::from("Product")).with_mark(SidebarRowMark::Checked))
        .with_row(SidebarRow::create(AzString::from("Events")).with_mark(SidebarRowMark::Unchecked));
    let dom = Sidebar::create()
        .with_section(section)
        .with_state(SidebarState::Expanded)
        .with_form(FormFactor::Desktop)
        .dom();
    let rows = all(&dom, SIDEBAR_ROW_CLASS);
    let icon_of = |row: &Dom| -> String {
        row.children
            .as_ref()
            .iter()
            .find_map(|c| match c.root.get_node_type() {
                azul_core::dom::NodeType::Icon(name) => Some(name.as_ref().as_str().to_string()),
                _ => None,
            })
            .unwrap_or_default()
    };
    assert_eq!(icon_of(rows[0]), "check_box");
    assert_eq!(icon_of(rows[1]), "check_box_outline_blank");
}

// ---- the foot ----

fn with_foot() -> Sidebar {
    mail()
        .with_help(AzString::from("Help"))
        .with_status(SidebarStatus::create(AzString::from("Sync"), 0.82, PieTone::Busy).with_text(AzString::from("82 %")))
        .with_status(SidebarStatus::create(AzString::from("Drive"), 0.64, PieTone::Accent).with_text(AzString::from("64 % full")))
}

#[test]
fn the_foot_is_help_then_the_status_pies_then_the_toggle_in_the_rail() {
    let dom = with_foot().with_state(SidebarState::Rail).dom();
    assert_eq!(foot_names(&dom), vec!["help", "status", "status", "toggle"]);
}

#[test]
fn the_foot_is_the_same_order_expanded_and_the_toggle_is_still_last() {
    let dom = with_foot().with_state(SidebarState::Expanded).dom();
    assert_eq!(foot_names(&dom), vec!["help", "status", "status", "toggle"]);
}

#[test]
fn a_sidebar_without_help_or_statuses_has_only_the_toggle() {
    let dom = mail().with_state(SidebarState::Rail).dom();
    assert_eq!(foot_names(&dom), vec!["toggle"]);
}

#[test]
fn the_toggle_asks_for_the_other_state() {
    let rail = mail().with_state(SidebarState::Rail).dom();
    let ev = click_event(first(&rail, SIDEBAR_TOGGLE_CLASS).expect("a toggle")).expect("reports");
    assert_eq!((ev.kind, ev.state), (SidebarEventKind::StateChanged, SidebarState::Expanded));
    let open = mail().with_state(SidebarState::Expanded).dom();
    let ev = click_event(first(&open, SIDEBAR_TOGGLE_CLASS).expect("a toggle")).expect("reports");
    assert_eq!((ev.kind, ev.state), (SidebarEventKind::StateChanged, SidebarState::Rail));
}

#[test]
fn the_toggle_visits_the_handle_only_when_the_sidebar_asks_for_it() {
    let dom = mail().with_state(SidebarState::Rail).with_handle_in_cycle(true).dom();
    let ev = click_event(first(&dom, SIDEBAR_TOGGLE_CLASS).expect("a toggle")).expect("reports");
    assert_eq!(ev.state, SidebarState::Handle);
    assert_eq!(mail().toggled_state(), SidebarState::Expanded);
}

#[test]
fn the_help_row_reports_help() {
    let dom = with_foot().with_state(SidebarState::Expanded).dom();
    let ev = click_event(first(&dom, SIDEBAR_HELP_CLASS).expect("a help row")).expect("reports");
    assert_eq!(ev.kind, SidebarEventKind::HelpClicked);
}

#[test]
fn a_status_is_a_named_button_with_its_text_in_the_name() {
    let dom = with_foot().with_state(SidebarState::Rail).dom();
    let names: Vec<String> = all(&dom, SIDEBAR_STATUS_CLASS).into_iter().map(name_of).collect();
    assert_eq!(names, vec!["Sync, 82 %".to_string(), "Drive, 64 % full".to_string()]);
}

#[test]
fn the_expanded_status_row_shows_its_label_and_its_text() {
    let dom = with_foot().with_state(SidebarState::Expanded).dom();
    let row = first(&dom, SIDEBAR_STATUS_CLASS).expect("a status row");
    let said = text_of(row);
    assert!(said.contains("Sync") && said.contains("82 %"), "{said}");
    let rail = with_foot().with_state(SidebarState::Rail).dom();
    let button = first(&rail, SIDEBAR_STATUS_CLASS).expect("a status button");
    assert!(text_of(button).is_empty(), "the rail's pie has no text");
}

#[test]
fn a_status_has_a_popover_with_a_title_and_a_line_of_detail() {
    let status = SidebarStatus::create(AzString::from("Drive"), 0.64, PieTone::Accent)
        .with_text(AzString::from("64 %"))
        .with_detail(AzString::from("64 GB of 100 GB used"));
    let dom = Sidebar::create().with_status(status).with_form(FormFactor::Desktop).dom();
    let popover = first(&dom, SIDEBAR_POPOVER_CLASS).expect("the default popover is in the tree");
    let said = text_of(popover);
    assert!(said.contains("Drive, 64 %") && said.contains("64 GB of 100 GB used"), "{said}");
}

#[test]
fn a_status_can_bring_its_own_popover_content() {
    let own = Dom::create_div().with_id(AzString::from("own-popover"));
    let status = SidebarStatus::create(AzString::from("Sync"), 0.3, PieTone::Busy).with_popover(own);
    let dom = Sidebar::create().with_status(status).with_form(FormFactor::Desktop).dom();
    assert!(first(&dom, SIDEBAR_POPOVER_CLASS).is_none(), "the app's content replaces the default");
    fn has_id(dom: &Dom, id: &str) -> bool {
        dom.root.get_ids_and_classes().as_ref().iter().any(|c| c.as_id() == Some(id))
            || dom.children.as_ref().iter().any(|c| has_id(c, id))
    }
    assert!(has_id(&dom, "own-popover"));
}

// ---- the handle ----

#[test]
fn the_handle_is_a_grab_pill_and_a_click_on_it_asks_for_the_rail() {
    let dom = mail().with_state(SidebarState::Handle).dom();
    assert!(has_class(&dom, SIDEBAR_HANDLE_CLASS));
    assert!(first(&dom, SIDEBAR_GRAB_CLASS).is_some());
    assert!(all(&dom, SIDEBAR_BUTTON_CLASS).is_empty() && all(&dom, SIDEBAR_ROW_CLASS).is_empty());
    let target = dom.children.as_ref().first().expect("the strip");
    let ev = click_event(target).expect("the strip reports");
    assert_eq!((ev.kind, ev.state), (SidebarEventKind::StateChanged, SidebarState::Rail));
}

#[test]
fn the_handle_column_is_16_px() {
    let col = mail().with_state(SidebarState::Handle).column();
    assert!((col.px - 16.0).abs() < 0.001);
}

// ---- form factor, window controls, surface ----

#[test]
fn the_phone_drawer_is_a_share_of_the_frame_and_the_phone_rail_is_48() {
    let drawer = mail().with_form(FormFactor::Phone).with_state(SidebarState::Expanded).column();
    assert!((drawer.percent - 76.0).abs() < 0.001 && (drawer.max_px - 360.0).abs() < 0.001);
    let rail = mail().with_form(FormFactor::Phone).with_state(SidebarState::Rail).column();
    assert!((rail.px - 48.0).abs() < 0.001);
    let dom = mail().with_form(FormFactor::Phone).dom();
    assert!(has_class(&dom, SIDEBAR_PHONE_CLASS));
}

#[test]
fn a_phone_row_is_a_finger_tall_and_a_desktop_row_is_not() {
    let phone = Ctx {
        state: SidebarState::Expanded,
        phone: true,
        look: SidebarSurface::Theme.look(),
        on_event: OptionSidebarOnEvent::None,
        stop: 0,
    };
    assert!((phone.row_h() - 44.0).abs() < 0.001 && (phone.button() - 44.0).abs() < 0.001);
    let desktop = Ctx { phone: false, ..phone };
    assert!((desktop.row_h() - 28.0).abs() < 0.001 && (desktop.button() - 40.0).abs() < 0.001);
}

#[test]
fn the_band_for_the_window_controls_is_there_only_when_they_are_at_the_left() {
    let with = mail().with_window_controls(78.0, 28.0).dom();
    assert!(first(&with, SIDEBAR_CONTROLS_CLASS).is_some());
    let without = mail().with_window_controls(0.0, 28.0).dom();
    assert!(first(&without, SIDEBAR_CONTROLS_CLASS).is_none());
    let none = mail().dom();
    assert!(first(&none, SIDEBAR_CONTROLS_CLASS).is_none());
}

#[test]
fn the_rail_keeps_the_width_of_the_window_controls() {
    let col = mail().with_state(SidebarState::Rail).with_window_controls(78.0, 28.0).column();
    assert!((col.px - 78.0).abs() < 0.001);
    let plain = mail().with_state(SidebarState::Rail).column();
    assert!((plain.px - 54.0).abs() < 0.001);
}

#[test]
fn the_surface_paints_the_theme_ground_or_a_material_or_nothing() {
    assert!(SidebarSurface::Theme.root_css().contains("system:sidebar-background"));
    assert!(SidebarSurface::Leather.root_css().contains("builtin(leather)"));
    assert!(SidebarSurface::BlueJeans.root_css().contains("builtin(bluejeans)"));
    assert!(SidebarSurface::Aluminium.root_css().contains("builtin(aluminium)"));
    assert!(SidebarSurface::Vellum.root_css().contains("builtin(vellum-leaf)"));
    assert!(!SidebarSurface::Inherit.root_css().contains("background"));
}

#[test]
fn the_sidebar_is_a_named_landmark() {
    let dom = mail().with_label(AzString::from("Folders")).dom();
    assert_eq!(name_of(&dom), "Folders");
    assert_eq!(dom.root.get_node_type(), &azul_core::dom::NodeType::Nav);
}

#[test]
fn the_default_sidebar_is_empty_and_builds() {
    let dom = Sidebar::default().dom();
    assert!(has_class(&dom, SIDEBAR_CLASS));
    assert_eq!(foot_names(&dom), vec!["toggle"]);
}

#[test]
fn the_header_slot_is_in_the_expanded_sidebar_and_dropped_from_the_rail() {
    let header = Dom::create_div().with_id(AzString::from("account"));
    let expanded = mail().with_header(header.clone()).with_state(SidebarState::Expanded).dom();
    fn has_id(dom: &Dom, id: &str) -> bool {
        dom.root.get_ids_and_classes().as_ref().iter().any(|c| c.as_id() == Some(id))
            || dom.children.as_ref().iter().any(|c| has_id(c, id))
    }
    assert!(has_id(&expanded, "account"));
    let rail = mail().with_header(header).with_state(SidebarState::Rail).dom();
    assert!(!has_id(&rail, "account"));
}

#[test]
fn swap_with_default_leaves_an_empty_sidebar_and_returns_the_old_one() {
    let mut sb = mail();
    let old = sb.swap_with_default();
    assert_eq!(old.sections.as_ref().len(), 2);
    assert!(sb.sections.as_ref().is_empty());
}

// ---- an open document: its own pie and its collaboration switch ----

fn documents() -> Sidebar {
    let section = SidebarSection::create(AzString::from("Open"))
        .with_row(
            SidebarRow::create(AzString::from("book1.xlsx"))
                .with_icon(AzString::from("table_chart"))
                .with_pie(0.7, PieTone::Busy)
                .with_collab(SidebarCollab::Off)
                .with_selected(true)
                .with_tag(11),
        )
        .with_row(
            SidebarRow::create(AzString::from("book2.xlsx"))
                .with_icon(AzString::from("table_chart"))
                .with_pie(1.0, PieTone::Good)
                .with_collab(SidebarCollab::On)
                .with_tag(12),
        )
        .with_row(SidebarRow::create(AzString::from("notes.xlsx")).with_icon(AzString::from("table_chart")).with_tag(13));
    Sidebar::create().with_section(section).with_form(FormFactor::Desktop)
}

#[test]
fn a_document_row_has_its_own_pie_and_a_collaboration_icon() {
    let dom = documents().with_state(SidebarState::Expanded).dom();
    assert_eq!(all(&dom, SIDEBAR_ROW_PIE_CLASS).len(), 2, "the two documents that sync");
    assert_eq!(all(&dom, SIDEBAR_COLLAB_CLASS).len(), 2, "the two that can collaborate");
    let rows = all(&dom, SIDEBAR_ROW_CLASS);
    assert!(first(rows[2], SIDEBAR_ROW_PIE_CLASS).is_none() && first(rows[2], SIDEBAR_COLLAB_CLASS).is_none());
}

#[test]
fn the_pie_reports_the_document_and_does_not_choose_its_row() {
    let dom = documents().with_state(SidebarState::Expanded).dom();
    let pie = all(&dom, SIDEBAR_ROW_PIE_CLASS)[0];
    let ev = click_event(pie).expect("the pie reports");
    assert_eq!((ev.kind, ev.index, ev.tag), (SidebarEventKind::RowStatusClicked, 0, 11));
}

#[test]
fn the_collaboration_icon_asks_for_the_other_state() {
    let dom = documents().with_state(SidebarState::Expanded).dom();
    let icons = all(&dom, SIDEBAR_COLLAB_CLASS);
    let off = click_event(icons[0]).expect("reports");
    assert_eq!((off.kind, off.index, off.tag, off.expand), (SidebarEventKind::RowCollabToggled, 0, 11, true));
    let on = click_event(icons[1]).expect("reports");
    assert_eq!((on.kind, on.index, on.tag, on.expand), (SidebarEventKind::RowCollabToggled, 1, 12, false));
}

#[test]
fn the_collaboration_icon_says_what_it_is_for_a_screen_reader() {
    let dom = documents().with_state(SidebarState::Expanded).dom();
    let icons = all(&dom, SIDEBAR_COLLAB_CLASS);
    assert_eq!(name_of(icons[0]), "Activate collaboration");
    assert_eq!(name_of(icons[1]), "Collaboration is on, turn it off");
    let states = |d: &Dom| d.root.get_accessibility_info().expect("a11y").states.as_ref().to_vec();
    assert!(states(icons[0]).contains(&AccessibilityState::CheckedFalse));
    assert!(states(icons[1]).contains(&AccessibilityState::CheckedTrue));
}

#[test]
fn in_the_rail_a_document_keeps_its_icon_and_its_pie_and_loses_the_collaboration_icon() {
    let dom = documents().with_state(SidebarState::Rail).dom();
    let buttons = all(&dom, SIDEBAR_BUTTON_CLASS);
    assert_eq!(buttons.len(), 3);
    let disc = crate::widgets::pie_marker::PIE_MARKER_DISC_CLASS;
    assert!(first(buttons[0], disc).is_some() && first(buttons[1], disc).is_some());
    assert!(first(buttons[2], disc).is_none(), "a document that does not sync has no pie");
    assert!(all(&dom, SIDEBAR_COLLAB_CLASS).is_empty());
    assert!(all(&dom, SIDEBAR_ROW_PIE_CLASS).is_empty(), "the rail's pie is a corner mark, not a button");
}

#[test]
fn a_row_has_no_pie_unless_it_is_given_a_fraction() {
    assert!(!SidebarRow::create(AzString::from("a")).has_pie());
    assert!(SidebarRow::create(AzString::from("a")).with_pie(0.0, PieTone::Accent).has_pie());
    assert!(!SidebarRow::create(AzString::from("a")).with_pie(f32::NAN, PieTone::Accent).has_pie());
    assert!(!SidebarRow::create(AzString::from("a")).with_pie(-0.5, PieTone::Accent).has_pie());
}

#[test]
fn the_tab_row_keeps_clear_of_the_controls_only_where_the_sidebar_is_narrower_than_they_are() {
    let controls = |state: SidebarState| mail().with_state(state).with_window_controls(78.0, 28.0).header_left_pad();
    assert!((controls(SidebarState::Rail) - 0.0).abs() < 0.001, "the rail is as wide as the controls");
    assert!((controls(SidebarState::Expanded) - 0.0).abs() < 0.001);
    assert!((controls(SidebarState::Handle) - 62.0).abs() < 0.001, "the 16 px handle covers 16 of the 78");
    assert!((mail().with_state(SidebarState::Handle).header_left_pad() - 0.0).abs() < 0.001, "no controls at the left");
}
