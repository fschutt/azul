//! The simulator's choices: which phone, which way round, which session, and how big the window must be for them.

use super::*;
use azul_review_link::view::Session;

fn extras() -> Extras {
    Extras::new(Arc::new(Mutex::new(SimMac::new())), Arc::new(Mutex::new(Vec::new())))
}

#[test]
fn the_window_holds_the_phone_in_its_bezel_and_the_panel() {
    let e = extras();
    let env = e.env();
    let (w, h) = e.window_size();
    assert_eq!(w, env.width + 2.0 * BEZEL + 2.0 * STAGE_PAD + PANEL_W);
    assert!(h >= env.height + 2.0 * BEZEL + 2.0 * STAGE_PAD);
    let mut flat = extras();
    flat.chrome = false;
    assert_eq!(flat.window_size(), (env.width, env.height), "a screenshot of the phone alone is the phone's size");
}

#[test]
fn another_phone_or_the_other_way_round_is_another_environment_and_another_window() {
    let mut e = extras();
    let portrait = e.window_size();
    e.landscape = true;
    let landscape = e.window_size();
    assert!(landscape.0 > portrait.0, "wider");
    e.device = 1;
    assert_eq!((e.env().width, e.env().height), (800.0, 360.0));
    e.landscape = false;
    assert_eq!((e.env().width, e.env().height), (360.0, 800.0));
    e.safe = false;
    assert_eq!(e.env().insets, crate::env::Insets::NONE);
    e.lang = Lang::De;
    assert_eq!(e.env().lang, Lang::De);
}

#[test]
fn a_session_scene_is_the_session_it_names_for_this_mac() {
    assert_eq!(SessionScene::Found.session("Studio Mac"), Session::Pairing { mac_name: "Studio Mac".into(), code: SIM_CODE });
    assert_eq!(SessionScene::Connected.session("Studio Mac"), Session::Connected { mac_name: "Studio Mac".into() });
    assert_eq!(SessionScene::Unpaired.session("x"), Session::Unpaired);
    assert!(matches!(SessionScene::AddressChanged.session("x"), Session::Failed { kind: FailureKind::AddressChanged }));
    assert!(matches!(SessionScene::Lost.session("x"), Session::Lost { attempt: 3, .. }));
}
