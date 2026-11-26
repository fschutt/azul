//! The simulator's choices: which phone, which way round, which session, and how big the window must be for them.

use super::*;
use crate::sim::phone::SimPhone;
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

// ---- the window on a screen of a real size

/// The usable part of the screen of a 13 inch laptop at its default scaling: 1280 x 832 less the menu bar and the Dock.
const LAPTOP: (f32, f32) = (1280.0, 748.0);

fn on_screen(work: (f32, f32)) -> Extras {
    let mut e = extras();
    e.work_area = Some(work);
    e
}

fn steps_of_five_percent(scale: f32) -> bool {
    ((scale * 20.0).round() - scale * 20.0).abs() < 1e-3
}

#[test]
fn a_screen_too_small_for_the_phone_gets_a_smaller_phone_in_steps_of_five_percent_and_a_window_that_fits_it() {
    let e = on_screen(LAPTOP);
    let scale = e.scale();
    assert!((scale - 0.75).abs() < 1e-6, "an iPhone 15 on a laptop screen: {scale}");
    assert!(steps_of_five_percent(scale));
    let (w, h) = e.window_size();
    assert!(h <= LAPTOP.1 - WINDOW_FRAME, "the window is {h} high on a screen that has {} for it", LAPTOP.1 - WINDOW_FRAME);
    assert!(w <= LAPTOP.0, "{w}");
    let (full_w, full_h) = (e.env().width + 2.0 * BEZEL + 2.0 * STAGE_PAD, e.env().height + 2.0 * BEZEL + 2.0 * STAGE_PAD);
    assert!(h >= full_h * scale - 0.01, "the whole phone is inside the window");
    assert!((w - (full_w * scale + PANEL_W)).abs() < 0.01, "the panel keeps its own size");
}

#[test]
fn a_screen_with_room_leaves_the_phone_at_its_own_size() {
    let e = on_screen((2560.0, 1400.0));
    assert_eq!(e.scale(), 1.0);
    let flat = extras();
    assert_eq!(flat.scale(), 1.0, "no screen known: the phone's own size");
    assert_eq!(e.window_size(), flat.window_size());
}

#[test]
fn every_phone_both_ways_round_fits_the_screen_of_a_small_laptop() {
    for device in 0..DEVICES.len() {
        for landscape in [false, true] {
            let mut e = on_screen(LAPTOP);
            e.device = device;
            e.landscape = landscape;
            let (w, h) = e.window_size();
            let scale = e.scale();
            assert!(w <= LAPTOP.0 && h <= LAPTOP.1 - WINDOW_FRAME, "{} landscape {landscape}: {w} x {h}", DEVICES[device].name);
            assert!((0.3..=1.0).contains(&scale) && steps_of_five_percent(scale), "{scale}");
            let phone = (e.env().width * scale, e.env().height * scale);
            assert!(phone.0 + 2.0 * BEZEL * scale <= w - PANEL_W && phone.1 + 2.0 * BEZEL * scale <= h, "the phone is inside the window");
        }
    }
}

#[test]
fn the_zoom_the_person_chose_is_the_zoom_and_the_window_follows_it() {
    let mut e = on_screen(LAPTOP);
    e.zoom = Zoom::Percent(50);
    assert_eq!(e.scale(), 0.5);
    let (w, h) = e.window_size();
    let full_w = e.env().width + 2.0 * BEZEL + 2.0 * STAGE_PAD;
    assert!((w - (full_w * 0.5 + PANEL_W)).abs() < 0.01, "{w}");
    assert!(h >= PANEL_MIN_H.min(LAPTOP.1 - WINDOW_FRAME), "the panel needs its room, as much as the screen has: {h}");
    e.zoom = Zoom::Percent(100);
    assert_eq!(e.scale(), 1.0, "a person who asks for the phone's own size on a small screen gets it");
    assert!(e.window_size().1 <= LAPTOP.1 - WINDOW_FRAME, "but the window is never taller than the screen has room for");
    e.zoom = Zoom::Percent(5);
    assert_eq!(e.scale(), f32::from(MIN_ZOOM_PERCENT) / 100.0, "never smaller than a third, whatever was asked");
    e.zoom = Zoom::Percent(250);
    assert_eq!(e.scale(), 1.0, "never bigger than the phone");
}

#[test]
fn a_screen_that_is_no_number_is_not_believed_and_said() {
    let mut e = extras();
    for (w, h) in [(f32::NAN, 800.0), (1200.0, f32::INFINITY), (-5.0, 800.0), (0.0, 0.0), (1200.0, f32::NEG_INFINITY)] {
        let said = e.set_work_area(w, h);
        assert!(said.is_some_and(|s| s.contains("screen")), "{w} x {h}");
        assert_eq!(e.work_area, None);
        assert_eq!(e.scale(), 1.0);
    }
    assert_eq!(e.set_work_area(1280.0, 748.0), None);
    assert_eq!(e.work_area, Some((1280.0, 748.0)));
    // a screen that is tiny but a number: believed, and the phone is as small as it gets, the window as small as it gets
    let tiny = on_screen((200.0, 150.0));
    assert_eq!(tiny.scale(), f32::from(MIN_ZOOM_PERCENT) / 100.0);
    assert!(tiny.window_size().1 >= 300.0, "never a window with no room at all");
}

#[test]
fn choosing_a_zoom_in_the_panel_asks_for_the_new_window_and_chooses_it_for_good() {
    let mut e = on_screen(LAPTOP);
    let mut root = Root::new(Box::new(SimPhone::new(e.mac.clone(), e.commands.clone())), EnvSource::Fixed(e.env()));
    assert!(e.perform(&mut root, 0, SimAction::Zoom(Zoom::Percent(50))));
    assert_eq!(e.zoom, Zoom::Percent(50));
    assert_eq!(e.resize_wanted, Some(e.window_size()));
    assert_eq!(e.window_size().0, (e.env().width + 2.0 * BEZEL + 2.0 * STAGE_PAD) * 0.5 + PANEL_W);
    assert!(e.perform(&mut root, 0, SimAction::Zoom(Zoom::Fit)));
    assert!((e.scale() - 0.75).abs() < 1e-6);
}

#[test]
fn the_bezel_is_scaled_only_when_the_phone_is_smaller_than_its_own_size_and_keeps_its_box() {
    let own = bezel_style(1.0, 393.0, 852.0);
    assert!(!own.contains("transform"), "the phone at its own size is drawn as a screenshot draws it: {own}");
    assert!(own.contains("width: 419.0px;") && own.contains("height: 878.0px;"), "the bezel has the phone's size and its own padding and border, not what its content makes of it: {own}");
    let small = bezel_style(0.75, 393.0, 852.0);
    assert!(small.contains("transform: scale(0.75);") && small.contains("transform-origin: 0px 0px;"), "{small}");
    assert!(small.contains("width: 419.0px;") && small.contains("height: 878.0px;"), "a transform changes no layout: {small}");
    assert!(bezel_style(0.3, 360.0, 800.0).contains("scale(0.3)"));
}

#[test]
fn the_zoom_of_the_command_line_is_fit_or_a_percent_in_range_and_anything_else_is_said() {
    assert_eq!(Zoom::parse("fit"), Ok(Zoom::Fit));
    assert_eq!(Zoom::parse("FIT"), Ok(Zoom::Fit));
    assert_eq!(Zoom::parse("75"), Ok(Zoom::Percent(75)));
    assert_eq!(Zoom::parse("75%"), Ok(Zoom::Percent(75)));
    assert_eq!(Zoom::parse(" 100 "), Ok(Zoom::Percent(100)));
    assert_eq!(Zoom::parse("30"), Ok(Zoom::Percent(30)));
    for bad in ["", "abc", "29", "101", "-5", "7.5", "300", "fit!", "%"] {
        assert!(Zoom::parse(bad).is_err(), "{bad:?}");
    }
    let said = Zoom::parse("200").unwrap_err();
    assert!(said.contains("30") && said.contains("100") && said.contains("fit"), "the message names what is allowed: {said}");
}

#[test]
fn the_trace_of_the_window_says_its_size_its_scale_and_the_screen_it_was_fitted_to() {
    assert_eq!(window_line((688.75, 708.0), 0.75, Some((1280.0, 748.0))), "[trace] window 689x708 scale 0.75 screen 1280x748");
    assert_eq!(window_line((805.0, 924.0), 1.0, None), "[trace] window 805x924 scale 1.00 screen unknown");
}
