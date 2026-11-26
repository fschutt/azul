//! The simulated connection: it answers like a Mac behind a link with a latency, and the panel's commands change what the
//! phone is told.

use super::*;
use azul_review_link::frame::ReplyStatus;

fn phone() -> (SimPhone, Arc<Mutex<SimMac>>, Commands) {
    let mac = Arc::new(Mutex::new(SimMac::new()));
    let commands: Commands = Arc::new(Mutex::new(Vec::new()));
    (SimPhone::new(mac.clone(), commands.clone()), mac, commands)
}

fn run(phone: &mut SimPhone, from: Millis, to: Millis) {
    for ms in from..=to {
        phone.poll(ms);
    }
}

#[test]
fn a_new_phone_is_connected_to_the_mac_and_hears_its_pen_state_and_picture_after_the_latency() {
    let (mut phone, _, _) = phone();
    assert!(phone.view().is_connected());
    assert!(phone.view().pen.is_none());
    phone.poll(0);
    assert!(phone.view().pen.is_none(), "the link takes its time");
    run(&mut phone, 1, 40);
    assert!(phone.view().pen.is_some() && phone.view().thumb.is_some());
    assert!(phone.view().thumb_is_current());
}

#[test]
fn a_press_goes_to_the_mac_and_its_conversation_comes_back_as_replies() {
    let (mut phone, mac, _) = phone();
    run(&mut phone, 0, 50);
    phone.press_button(100, 0);
    assert!(mac.lock().unwrap().log.iter().any(|l| l.text.contains("button 1")));
    assert_eq!(phone.view().button_status[0], None, "sent, not answered yet");
    run(&mut phone, 100, 200);
    assert_eq!(phone.view().button_status[0], Some(ReplyStatus::Received), "the Mac has it at once");
    run(&mut phone, 200, 500);
    assert_eq!(phone.view().button_status[0], Some(ReplyStatus::Working));
    run(&mut phone, 500, 4_300);
    assert_eq!(phone.view().button_status[0], Some(ReplyStatus::Suggested));
    run(&mut phone, 4_300, 9_300);
    assert_eq!(phone.view().button_status[0], Some(ReplyStatus::Done));
}

#[test]
fn a_folder_asked_for_arrives_after_the_round_trip() {
    let (mut phone, _, _) = phone();
    run(&mut phone, 0, 50);
    phone.list_dir(100, "layout", 0);
    assert!(phone.view().files.loading());
    run(&mut phone, 100, 110);
    assert!(phone.view().files.loading(), "12 ms there and 12 back");
    run(&mut phone, 110, 140);
    assert!(!phone.view().files.loading());
    assert_eq!(phone.view().files.dir, "layout");
    assert!(phone.view().files.entries.iter().any(|e| e.name == "src"));
}

#[test]
fn opening_a_file_changes_what_the_pen_state_says() {
    let (mut phone, _, _) = phone();
    run(&mut phone, 0, 50);
    phone.open_file(100, "core/src/dom.rs", Some(9));
    run(&mut phone, 100, 200);
    let pen = phone.view().pen.as_ref().unwrap();
    assert_eq!(pen.file, "core/src/dom.rs");
    assert_eq!(pen.line, 9);
}

#[test]
fn a_gesture_moves_the_view_the_mac_reports() {
    let (mut phone, _, _) = phone();
    run(&mut phone, 0, 50);
    let before = phone.view().pen.as_ref().unwrap().view;
    phone.gesture(100, 1, Phase::Begin, (0.0, 0.0), 1.0, Focus::Pen);
    phone.gesture(110, 1, Phase::Update, (0.1, 0.0), 1.0, Focus::Pen);
    phone.gesture(120, 1, Phase::End, (0.1, 0.0), 1.0, Focus::Pen);
    run(&mut phone, 120, 200);
    let after = phone.view().pen.as_ref().unwrap().view;
    assert!(after.x > before.x, "{before:?} {after:?}");
}

#[test]
fn the_panel_sets_the_session_and_a_link_that_is_down_drops_what_the_phone_asks() {
    let (mut phone, mac, commands) = phone();
    run(&mut phone, 0, 50);
    commands.lock().unwrap().push(SimCommand::Session(Session::Lost { mac_name: "Studio Mac".into(), attempt: 2 }));
    phone.poll(60);
    assert!(matches!(phone.view().session, Session::Lost { attempt: 2, .. }));
    let log_len = mac.lock().unwrap().log.len();
    phone.press_button(70, 1);
    phone.fit(71);
    assert_eq!(mac.lock().unwrap().log.len(), log_len, "the Mac heard nothing");
    commands.lock().unwrap().push(SimCommand::Session(Session::Connected { mac_name: "Studio Mac".into() }));
    run(&mut phone, 80, 140);
    assert!(phone.view().pen.is_some(), "a link that comes back says everything again");
}

#[test]
fn pairing_goes_searching_then_a_code_then_the_mac_confirms() {
    let (mut phone, _, commands) = phone();
    phone.forget_mac(0);
    assert_eq!(phone.view().session, Session::Unpaired);
    phone.retry(100);
    assert!(matches!(phone.view().session, Session::Searching { attempt: 1 }));
    phone.poll(1_000);
    assert!(matches!(phone.view().session, Session::Searching { .. }));
    phone.poll(2_000);
    assert_eq!(phone.view().session, Session::Pairing { mac_name: "Studio Mac".into(), code: SIM_CODE });
    commands.lock().unwrap().push(SimCommand::MacConfirms);
    phone.poll(2_100);
    assert!(phone.view().is_connected());
}

#[test]
fn a_mac_that_is_not_running_is_not_found_and_a_declined_pairing_says_so() {
    let (mut phone, mac, commands) = phone();
    mac.lock().unwrap().online = false;
    phone.forget_mac(0);
    phone.retry(0);
    phone.poll(3_000);
    assert!(matches!(phone.view().session, Session::Searching { .. }));
    phone.poll(3_300);
    assert_eq!(phone.view().session, Session::Failed { kind: FailureKind::NotFound });
    mac.lock().unwrap().online = true;
    phone.retry(4_000);
    phone.poll(6_000);
    commands.lock().unwrap().push(SimCommand::MacDeclines);
    phone.poll(6_100);
    assert_eq!(phone.view().session, Session::Failed { kind: FailureKind::Declined });
}

#[test]
fn a_pairing_text_that_is_no_offer_is_refused_and_a_good_one_starts_the_search() {
    let (mut phone, _, _) = phone();
    assert!(phone.begin_pairing(0, "hello").is_err());
    let offer = PairingOffer { ticket: "endpointabc".into(), secret: [1; 16] }.to_text();
    assert!(phone.begin_pairing(0, &offer).is_ok());
    assert!(matches!(phone.view().session, Session::Searching { .. }));
}

#[test]
fn saved_buttons_come_back_from_the_mac_cut_to_their_caps() {
    let (mut phone, _, _) = phone();
    let mut set = ButtonSet::defaults();
    set.buttons[0].label = "A far too long label".into();
    let caps = phone.set_buttons(0, &set);
    assert_eq!(caps.len(), 1);
    run(&mut phone, 0, 60);
    assert_eq!(phone.view().buttons.buttons[0].label.chars().count(), 12);
}
