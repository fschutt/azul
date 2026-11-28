use super::*;

#[test]
fn every_command_has_a_label_a_category_and_a_name_of_its_own() {
    let mut names: Vec<&str> = ACTIONS.iter().map(|a| a.name()).collect();
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), ACTIONS.len(), "no two commands share a name");
    for &a in &ACTIONS {
        assert!(!a.label().is_empty() && !a.category().is_empty() && !a.icon().is_empty(), "{a:?}");
    }
    assert_eq!(Action::OpenFolder.keys(), "Mod+K Mod+O", "VSCode's chord");
    assert_eq!(Action::ToggleTerminal.keys(), "Ctrl+`");
}
