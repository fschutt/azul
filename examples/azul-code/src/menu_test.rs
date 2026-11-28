use super::*;

#[test]
fn every_command_has_a_menu_callback_of_its_own() {
    let mut seen: Vec<usize> = crate::actions::ACTIONS.iter().map(|a| callback_of(*a) as usize).collect();
    seen.sort_unstable();
    seen.dedup();
    assert_eq!(seen.len(), crate::actions::ACTIONS.len());
    assert_eq!(RECENT_CALLBACKS.len(), RECENT_MAX);
}
