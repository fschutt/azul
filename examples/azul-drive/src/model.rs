//! What AzDrive's folder view is, as plain data: Explorer's eight layouts,
//! the groups ("Group by"), the columns of the Details layout and their
//! widths, the multi-selection with its anchor and focus (click, Ctrl+click,
//! Shift+click, the arrow keys), type-ahead, and the settings that persist.
//! No azul types here, so all of it is tested without a window.

#[cfg(test)]
mod tests {
    use chrono::{FixedOffset, TimeZone};

    use super::*;
    use crate::browse::{Column, Entry, Sort};

    fn file(name: &str, size: u64, modified: u64) -> Entry {
        Entry {
            key: format!("f/{name}"),
            name: name.to_string(),
            is_folder: false,
            size: Some(size),
            modified: Some(modified),
            etag: None,
        }
    }

    fn folder(name: &str) -> Entry {
        Entry {
            key: format!("f/{name}/"),
            name: name.to_string(),
            is_folder: true,
            size: None,
            modified: None,
            etag: None,
        }
    }

    fn keys(order: &[&str]) -> Vec<String> {
        order.iter().map(|s| s.to_string()).collect()
    }

    const ORDER: [&str; 5] = ["a", "b", "c", "d", "e"];

    #[test]
    fn explorer_has_eight_layouts_and_each_reads_back_from_its_name() {
        assert_eq!(ViewLayout::ALL.len(), 8);
        let labels: Vec<&str> = ViewLayout::ALL.iter().map(|l| l.label()).collect();
        assert_eq!(
            labels,
            vec![
                "Extra large icons",
                "Large icons",
                "Medium icons",
                "Small icons",
                "List",
                "Details",
                "Tiles",
                "Content"
            ]
        );
        for layout in ViewLayout::ALL {
            assert_eq!(ViewLayout::from_name(layout.name()), Some(layout));
        }
        assert_eq!(ViewLayout::from_name("nonsense"), None);
        assert!(ViewLayout::ExtraLargeIcons.icon_px() > ViewLayout::LargeIcons.icon_px());
        assert!(ViewLayout::LargeIcons.icon_px() > ViewLayout::MediumIcons.icon_px());
        assert!(ViewLayout::MediumIcons.icon_px() > ViewLayout::SmallIcons.icon_px());
        assert!(ViewLayout::LargeIcons.is_grid() && ViewLayout::Tiles.is_grid());
        assert!(!ViewLayout::Details.is_grid() && !ViewLayout::Content.is_grid());
    }

    #[test]
    fn a_plain_click_selects_one_item_and_moves_the_anchor() {
        let mut sel = Selection::default();
        sel.click("b");
        assert_eq!(sel.keys(), keys(&["b"]).as_slice());
        assert_eq!(sel.focus(), Some("b"));
        sel.click("d");
        assert_eq!(sel.keys(), keys(&["d"]).as_slice());
        assert_eq!(sel.single(), Some("d"));
    }

    #[test]
    fn ctrl_click_toggles_and_shift_click_selects_the_range_from_the_anchor() {
        let mut sel = Selection::default();
        sel.click("b");
        sel.toggle("d");
        assert_eq!(sel.keys(), keys(&["b", "d"]).as_slice());
        assert_eq!(sel.single(), None);
        sel.toggle("b");
        assert_eq!(sel.keys(), keys(&["d"]).as_slice());
        // The anchor is the last clicked item: d. Shift+click on a selects a..d.
        sel.extend("a", &ORDER);
        assert_eq!(sel.keys(), keys(&["a", "b", "c", "d"]).as_slice());
        // A second Shift+click re-ranges from the same anchor.
        sel.extend("e", &ORDER);
        assert_eq!(sel.keys(), keys(&["d", "e"]).as_slice());
        // Ctrl+Shift+click adds the range to what is selected.
        sel.click("a");
        sel.add_range("b", &ORDER);
        sel.toggle("e");
        sel.add_range("d", &ORDER);
        assert_eq!(sel.keys(), keys(&["a", "b", "c", "d", "e"]).as_slice());
    }

    #[test]
    fn select_all_none_and_invert_work_over_the_visible_order() {
        let mut sel = Selection::default();
        sel.select_all(&ORDER);
        assert_eq!(sel.len(), 5);
        sel.clear();
        assert!(sel.is_empty());
        sel.click("b");
        sel.toggle("c");
        sel.invert(&ORDER);
        assert_eq!(sel.keys(), keys(&["a", "d", "e"]).as_slice());
        // Items that left the listing leave the selection.
        sel.retain(&["a", "e"]);
        assert_eq!(sel.keys(), keys(&["a", "e"]).as_slice());
    }

    #[test]
    fn the_arrow_keys_move_the_focus_and_shift_extends_ctrl_only_moves() {
        let mut sel = Selection::default();
        // Nothing focused: the first step lands on the first item.
        assert_eq!(sel.step(&ORDER, 1, false, false), Some("a".to_string()));
        assert_eq!(sel.keys(), keys(&["a"]).as_slice());
        sel.step(&ORDER, 1, false, false);
        assert_eq!(sel.keys(), keys(&["b"]).as_slice());
        sel.step(&ORDER, 2, true, false);
        assert_eq!(sel.keys(), keys(&["b", "c", "d"]).as_slice());
        // Ctrl moves the focus without touching the selection.
        sel.step(&ORDER, 1, false, true);
        assert_eq!(sel.focus(), Some("e"));
        assert_eq!(sel.keys(), keys(&["b", "c", "d"]).as_slice());
        // Clamped at the ends (Home / End are big steps).
        sel.step(&ORDER, isize::MIN / 2, false, false);
        assert_eq!(sel.keys(), keys(&["a"]).as_slice());
        sel.step(&ORDER, isize::MAX / 2, false, false);
        assert_eq!(sel.keys(), keys(&["e"]).as_slice());
        assert_eq!(Selection::default().step(&[], 1, false, false), None);
    }

    #[test]
    fn type_ahead_finds_the_first_name_starting_with_what_was_typed() {
        let names = ["Apple", "apricot", "Banana", "blueberry", "cherry"];
        let mut ta = TypeAhead::default();
        assert_eq!(ta.push('a', 1_000), "a");
        assert_eq!(type_ahead_match(&names, "a", None), Some(0));
        // A quick second key extends the prefix.
        assert_eq!(ta.push('p', 1_300), "ap");
        assert_eq!(type_ahead_match(&names, "apr", Some(0)), Some(1));
        // After a pause the buffer starts again.
        assert_eq!(ta.push('b', 3_000), "b");
        // Repeating one letter cycles through the names starting with it.
        assert_eq!(type_ahead_match(&names, "b", Some(2)), Some(3));
        assert_eq!(type_ahead_match(&names, "bb", Some(3)), Some(2));
        assert_eq!(type_ahead_match(&names, "zz", Some(0)), None);
    }

    #[test]
    fn group_by_name_size_type_and_date_puts_items_into_explorers_groups() {
        assert_eq!(GroupBy::Name.group(&file("apple.txt", 1, 0), 0).1, "A - H");
        assert_eq!(GroupBy::Name.group(&file("Quince", 1, 0), 0).1, "Q - Z");
        assert_eq!(GroupBy::Name.group(&file("7up", 1, 0), 0).1, "0 - 9");
        assert_eq!(GroupBy::Name.group(&file("_x", 1, 0), 0).1, "Other");
        assert_eq!(GroupBy::Size.group(&file("e", 0, 0), 0).1, "Empty (0 KB)");
        assert_eq!(
            GroupBy::Size.group(&file("t", 10_000, 0), 0).1,
            "Tiny (0 - 16 KB)"
        );
        assert_eq!(
            GroupBy::Size.group(&file("s", 500_000, 0), 0).1,
            "Small (16 KB - 1 MB)"
        );
        assert_eq!(
            GroupBy::Size.group(&file("m", 50 * 1024 * 1024, 0), 0).1,
            "Medium (1 - 128 MB)"
        );
        assert_eq!(GroupBy::Size.group(&folder("x"), 0).1, "Folders");
        assert_eq!(GroupBy::Type.group(&folder("x"), 0).1, "File folder");
        assert_eq!(
            GroupBy::Type.group(&file("a.txt", 1, 0), 0).1,
            "Text Document"
        );
        // Folders come first in every grouping by size or type.
        assert!(
            GroupBy::Size.group(&folder("x"), 0).0 < GroupBy::Size.group(&file("e", 0, 0), 0).0
        );
    }

    #[test]
    fn dates_group_relative_to_today_in_the_local_zone() {
        let zone = FixedOffset::east_opt(0).unwrap();
        // Wednesday 2026-09-30 12:00 UTC.
        let now = zone.with_ymd_and_hms(2026, 9, 30, 12, 0, 0).unwrap();
        let at = |y, m, d| zone.with_ymd_and_hms(y, m, d, 9, 0, 0).unwrap().timestamp() as u64;
        let label = |unix: Option<u64>| date_group(unix, &now).1;
        assert_eq!(label(Some(at(2026, 9, 30))), "Today");
        assert_eq!(label(Some(at(2026, 9, 29))), "Yesterday");
        assert_eq!(label(Some(at(2026, 9, 28))), "Earlier this week");
        assert_eq!(label(Some(at(2026, 9, 22))), "Last week");
        assert_eq!(label(Some(at(2026, 9, 2))), "Earlier this month");
        assert_eq!(label(Some(at(2026, 8, 15))), "Last month");
        assert_eq!(label(Some(at(2026, 2, 1))), "Earlier this year");
        assert_eq!(label(Some(at(2019, 2, 1))), "A long time ago");
        assert_eq!(label(None), "Unknown date");
        assert!(
            date_group(Some(at(2026, 9, 30)), &now).0 < date_group(Some(at(2019, 2, 1)), &now).0
        );
    }

    #[test]
    fn grouping_keeps_the_sorted_order_inside_each_group() {
        let entries = vec![
            folder("zeta"),
            file("apple", 1, 0),
            file("Zoo", 1, 0),
            file("avocado", 1, 0),
        ];
        let refs: Vec<&Entry> = entries.iter().collect();
        let groups = group_entries(&refs, GroupBy::Name, 0);
        let shown: Vec<(String, Vec<&str>)> = groups
            .iter()
            .map(|g| {
                (
                    g.label.clone(),
                    g.entries.iter().map(|e| e.name.as_str()).collect(),
                )
            })
            .collect();
        assert_eq!(
            shown,
            vec![
                ("A - H".to_string(), vec!["apple", "avocado"]),
                ("Q - Z".to_string(), vec!["zeta", "Zoo"]),
            ]
        );
        assert_eq!(group_entries(&refs, GroupBy::None, 0).len(), 1);
        assert!(group_entries(&[], GroupBy::Name, 0).is_empty());
    }

    #[test]
    fn columns_can_be_added_resized_and_sized_to_fit() {
        let mut columns = ColumnLayout::default();
        assert_eq!(
            columns.visible(),
            vec![Column::Name, Column::Modified, Column::Type, Column::Size]
        );
        columns.toggle(Column::Path);
        assert!(columns.visible().contains(&Column::Path));
        columns.toggle(Column::Path);
        assert!(!columns.visible().contains(&Column::Path));
        columns.toggle(Column::Name);
        assert!(columns.visible().contains(&Column::Name), "Name never goes");
        columns.resize(Column::Size, 10.0);
        assert_eq!(columns.width(Column::Size), ColumnLayout::MIN_WIDTH);
        columns.resize(Column::Size, 150.0);
        assert_eq!(columns.width(Column::Size), 150.0);
        let long = file("a-very-long-file-name-that-needs-room.txt", 1, 0);
        columns.fit(&[&long], true);
        assert!(
            columns.width(Column::Name) > 250.0,
            "{}",
            columns.width(Column::Name)
        );
        assert!(columns.width(Column::Size) < 150.0, "fits the short sizes");
    }

    #[test]
    fn settings_round_trip_through_json_and_unknown_fields_fall_back_to_defaults() {
        let mut settings = Settings::default();
        assert_eq!(settings.layout, ViewLayout::Details);
        assert!(settings.navigation_pane && !settings.preview_pane);
        settings.layout = ViewLayout::LargeIcons;
        settings.group_by = GroupBy::Type;
        settings.sort = Sort {
            column: Column::Size,
            descending: true,
        };
        settings.show_hidden = true;
        settings.pinned.push(Pinned {
            drive: "home".to_string(),
            prefix: "docs/".to_string(),
            name: "docs".to_string(),
        });
        let text = settings.to_json();
        assert_eq!(Settings::from_json(&text), settings);
        let partial = Settings::from_json(r#"{"layout":"tiles","show_extensions":false}"#);
        assert_eq!(partial.layout, ViewLayout::Tiles);
        assert!(!partial.show_extensions);
        assert_eq!(partial.group_by, GroupBy::None);
        assert_eq!(Settings::from_json("not json"), Settings::default());
    }
}
