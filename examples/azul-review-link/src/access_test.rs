//! Who may command which screen: a pure function of the trust record of the peer and the screen, so that the phone today and
//! other AzReviews on the network later are held to the same rule.

use super::*;

const ME: &str = "node-me";

fn screen(id: u32, owner: &str, controller: &str, role: Role) -> ScreenInfo {
    ScreenInfo { id: ScreenId(id), owner: owner.to_string(), controller: controller.to_string(), role }
}

fn device(peer: &str) -> TrustRecord {
    TrustRecord::phone(peer, "My phone")
}

fn collaborator(peer: &str) -> TrustRecord {
    TrustRecord { peer: peer.to_string(), name: "Anna".to_string(), kind: PeerKind::Person, group: TrustGroup::Collaborator }
}

#[test]
fn the_main_screen_is_screen_zero_and_a_screen_id_shows_as_its_number() {
    assert_eq!(ScreenId::MAIN, ScreenId(0));
    assert_eq!(ScreenId::MAIN.to_string(), "screen 0");
    assert_eq!(ScreenId(7).to_string(), "screen 7");
}

#[test]
fn a_phone_that_was_paired_is_a_device_of_the_users_own_group() {
    let record = TrustRecord::phone("abc", "Anna's phone");
    assert_eq!((record.kind, record.group), (PeerKind::Device, TrustGroup::MyDevices));
    assert_eq!((record.peer.as_str(), record.name.as_str()), ("abc", "Anna's phone"));
}

#[test]
fn a_device_of_my_own_group_may_command_every_screen_of_the_node_at_once() {
    let screens = [screen(0, ME, ME, Role::Review), screen(1, ME, ME, Role::ReviewMirror), screen(2, ME, ME, Role::Whiteboard)];
    for s in &screens {
        assert_eq!(may_command(Some(&device("p")), Some(s), ME), Ok(()), "{s:?}");
    }
}

#[test]
fn a_device_may_command_a_screen_somebody_else_drives_at_the_moment_because_it_is_my_own_node() {
    let s = screen(0, ME, "someone-else", Role::Review);
    assert_eq!(may_command(Some(&device("p")), Some(&s), ME), Ok(()));
}

#[test]
fn a_collaborator_may_command_only_the_screen_it_was_given_control_of() {
    let mine = screen(0, ME, ME, Role::Review);
    let given = screen(1, ME, "anna", Role::Review);
    assert_eq!(may_command(Some(&collaborator("anna")), Some(&mine), ME), Err(Denied::NotController), "a screen it does not control");
    assert_eq!(may_command(Some(&collaborator("anna")), Some(&given), ME), Ok(()), "the screen it controls");
    assert_eq!(may_command(Some(&collaborator("ben")), Some(&given), ME), Err(Denied::NotController), "another collaborator is no controller");
}

#[test]
fn a_peer_that_is_not_trusted_may_command_nothing() {
    let s = screen(0, ME, ME, Role::Review);
    assert_eq!(may_command(None, Some(&s), ME), Err(Denied::NotTrusted));
}

#[test]
fn a_screen_that_does_not_exist_cannot_be_commanded_by_anyone() {
    assert_eq!(may_command(Some(&device("p")), None, ME), Err(Denied::UnknownScreen));
    assert_eq!(may_command(None, None, ME), Err(Denied::UnknownScreen), "the screen is looked at first: no probing which peers are trusted");
}

#[test]
fn a_screen_of_another_node_is_not_commanded_through_this_node() {
    let elsewhere = screen(3, "node-other", "node-other", Role::Review);
    assert_eq!(may_command(Some(&device("p")), Some(&elsewhere), ME), Err(Denied::NotOurScreen));
}

#[test]
fn the_owner_node_itself_may_always_command_its_screens() {
    let s = screen(0, ME, "anna", Role::Review);
    let owner = TrustRecord { peer: ME.to_string(), name: "me".to_string(), kind: PeerKind::Person, group: TrustGroup::Collaborator };
    assert_eq!(may_command(Some(&owner), Some(&s), ME), Ok(()));
}

#[test]
fn what_a_refusal_says_is_one_short_line_for_the_diagnostic() {
    for denied in [Denied::NotTrusted, Denied::NotController, Denied::UnknownScreen, Denied::NotOurScreen] {
        let said = denied.to_string();
        assert!(!said.is_empty() && !said.contains('\n') && said.len() < 100, "{denied:?}: {said}");
    }
}

#[test]
fn a_role_can_be_told_to_be_a_companion_or_a_window_of_azreview() {
    assert!(Role::Phone.is_companion());
    for role in [Role::Host, Role::Review, Role::ReviewMirror, Role::Whiteboard] {
        assert!(!role.is_companion(), "{role:?}");
    }
    assert_eq!(Role::from_tag(1), Some(Role::Phone));
    for role in [Role::Host, Role::Phone, Role::Review, Role::ReviewMirror, Role::Whiteboard] {
        assert_eq!(Role::from_tag(role.tag()), Some(role), "{role:?}");
    }
    assert_eq!(Role::from_tag(5), None);
    assert_eq!(Role::from_tag(255), None);
}
