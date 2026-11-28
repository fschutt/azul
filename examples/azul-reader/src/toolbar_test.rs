//! The toolbar's tools: what each runs, that each is an icon with a name (a control shows a
//! word or an icon, never both), and that a phone's bar holds only what fits it.

use azul::widgets::ToolbarItemKind;

use super::*;
use crate::settings::ReadingSettings;

fn state(phone: bool) -> AppState {
    let mut st = AppState::new(
        RefAny::new(0u8),
        std::path::PathBuf::from("."),
        ReadingSettings::default(),
        false,
    );
    st.phone = phone;
    st
}

fn tools(items: &[ToolbarItem]) -> Vec<&ToolbarItem> {
    items
        .iter()
        .filter(|i| matches!(i.kind, ToolbarItemKind::Button | ToolbarItemKind::Toggle))
        .collect()
}

#[test]
fn the_strips_button_opens_the_pages_or_closes_whatever_is_open() {
    let id = ids::TB_STRIP.as_str().to_string();
    assert_eq!(command_of(&id, Pane::None, Sort::Recent), Some(Command::Pane(Pane::Pages)));
    assert_eq!(command_of(&id, Pane::Pages, Sort::Recent), Some(Command::Pane(Pane::Pages)));
    assert_eq!(command_of(&id, Pane::Contents, Sort::Recent), Some(Command::Pane(Pane::Contents)));
}

#[test]
fn every_button_of_the_desktop_bar_runs_a_command() {
    let st = state(false);
    let app = RefAny::new(0u8);
    let items = reader_items_desktop(&app, &st);
    let tools = tools(&items);
    assert!(tools.len() >= 11, "{}", tools.len());
    for tool in tools {
        assert!(
            command_of(tool.id.as_str(), st.pane, st.sort).is_some(),
            "{} runs nothing",
            tool.id.as_str()
        );
    }
}

#[test]
fn a_tool_is_an_icon_with_a_name() {
    let app = RefAny::new(0u8);
    for st in [state(false), state(true)] {
        let items = if st.phone { reader_items_phone(&st) } else { reader_items_desktop(&app, &st) };
        for tool in tools(&items) {
            assert!(!tool.icon.as_str().is_empty(), "{} has no icon", tool.id.as_str());
            assert!(!tool.label.as_str().is_empty(), "{} has no name", tool.id.as_str());
            assert!(!tool.show_label, "{} shows a word beside its icon", tool.id.as_str());
        }
    }
}

#[test]
fn a_phones_bar_holds_what_fits_and_moves_none_into_a_menu() {
    let st = state(true);
    let items = reader_items_phone(&st);
    let tools = tools(&items);
    assert!(tools.len() <= 6, "{}", tools.len());
    for tool in tools {
        assert!(tool.never_overflow, "{} could end in the menu a phone cannot open", tool.id.as_str());
    }
    let ids_of_tools: Vec<&str> = items.iter().map(|i| i.id.as_str()).collect();
    for wanted in [&ids::TB_LIBRARY, &ids::TB_ZOOM_IN, &ids::TB_ZOOM_OUT, &ids::TB_BOOKMARK, &ids::TB_SETTINGS] {
        assert!(ids_of_tools.contains(&wanted.as_str()), "{}", wanted.as_str());
    }
}

#[test]
fn the_sort_button_goes_round_and_each_sort_has_its_icon() {
    let mut sort = Sort::Recent;
    let mut seen = Vec::new();
    for _ in 0..3 {
        seen.push(sort);
        sort = next_sort(sort);
    }
    assert_eq!(sort, Sort::Recent);
    assert_eq!(seen.len(), 3);
    let icons: std::collections::BTreeSet<&str> = seen.iter().map(|s| sort_icon(*s)).collect();
    assert_eq!(icons.len(), 3);
    let id = ids::TB_SORT.as_str().to_string();
    assert_eq!(command_of(&id, Pane::None, Sort::Title), Some(Command::Sort(Sort::Author)));
}

#[test]
fn an_unknown_tool_runs_nothing() {
    assert_eq!(command_of("something-else", Pane::None, Sort::Recent), None);
    assert_eq!(command_of(ids::TB_TITLE.as_str(), Pane::None, Sort::Recent), None, "the title is a label");
    assert_eq!(command_of(ids::TB_PAGE.as_str(), Pane::None, Sort::Recent), None, "the page field has its own key handler");
}
