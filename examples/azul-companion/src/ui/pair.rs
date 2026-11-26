//! Connecting to AzReview (screen A): shown until the phone is paired and whenever it cannot reach its Mac. What it
//! says depends on where the pairing stands: a first screen that explains (and, on an iPhone, says before the system
//! does that it will ask for the local network), the search, the code both screens show, the connection, and the ways it
//! can fail, each in words and with the one thing to do about it.

use azul::{
    prelude::*,
    widgets::{Spinner, SpinnerStyle},
};
use azul_review_link::limits::PROTOCOL_VERSION;
use azul_review_link::pairing::format_code;
use azul_review_link::view::{FailureKind, Session};

use super::kit::{card, column, div, icon, px, row, tap, text, Cx};
use super::Action;
use crate::env::Platform;
use crate::theme as th;

/// The word that names a failure in the resources (`fail-<word>-title`).
#[must_use]
pub fn fail_word(kind: &FailureKind) -> &'static str {
    match kind {
        FailureKind::NoPermission => "nopermission",
        FailureKind::NotFound => "notfound",
        FailureKind::UpdateCompanion => "updatecompanion",
        FailureKind::UpdateAzReview => "updateazreview",
        FailureKind::Declined => "declined",
        FailureKind::WrongCode => "wrongcode",
        FailureKind::Expired => "expired",
        FailureKind::Forgotten => "forgotten",
        FailureKind::Busy => "busy",
        FailureKind::Timeout => "timeout",
        FailureKind::AddressChanged => "addresschanged",
    }
}

/// What a failure looks like: its icon and colour.
fn fail_look(kind: &FailureKind) -> (&'static str, &'static str) {
    match kind {
        FailureKind::NoPermission => ("lock", th::WARN),
        FailureKind::NotFound | FailureKind::AddressChanged => ("wifi_off", th::WARN),
        FailureKind::UpdateCompanion | FailureKind::UpdateAzReview => ("system_update", th::ACCENT),
        FailureKind::Declined => ("block", th::DANGER),
        FailureKind::WrongCode | FailureKind::Expired => ("qr_code_scanner", th::WARN),
        FailureKind::Forgotten => ("link_off", th::WARN),
        FailureKind::Busy | FailureKind::Timeout => ("hourglass_empty", th::WARN),
    }
}

/// The id of the key that opens the demo (a boot test finds it by this).
pub const DEMO_ID: &str = "az-demo";

/// A round mark over the title.
fn hero(glyph: &str, colour: &str, size: f32) -> Dom {
    div(&format!(
        "width: {}; height: {}; border-radius: {}; background: {}; display: flex; align-items: center; justify-content: center; flex-shrink: 0;",
        px(size),
        px(size),
        px(size / 2.0),
        th::tint(colour, 0.18)
    ))
    .with_child(icon(glyph, size * 0.5, colour))
}

fn spinner(size: isize, colour: ColorU) -> Dom {
    Spinner::with_size(size).with_color(colour).with_indicator(SpinnerStyle::Spokes).dom()
}

fn title(words: &str) -> Dom {
    text(words, "font-size: 26px; font-weight: 700; text-align: center; line-height: 1.15;")
}

fn body(words: &str) -> Dom {
    text(words, &format!("font-size: 16px; color: {}; text-align: center; line-height: 1.35;", th::TEXT_DIM))
}

fn primary(cx: &Cx, words: &str, action: Action) -> Dom {
    tap(words, &format!("width: 100%; height: 52px; border-radius: 14px; background: {};", th::ACCENT), cx, action)
        .with_child(text(words, &format!("font-size: 17px; font-weight: 600; color: {};", th::ON_ACCENT)))
}

fn secondary(cx: &Cx, words: &str, glyph: Option<&str>, action: Action) -> Dom {
    let mut key = tap(words, &format!("width: 100%; height: 52px; border-radius: 14px; background: {}; flex-direction: row; gap: 8px;", th::SURFACE), cx, action);
    if let Some(glyph) = glyph {
        key.add_child(icon(glyph, 22.0, th::TEXT));
    }
    key.with_child(text(words, &format!("font-size: 17px; font-weight: 600; color: {};", th::TEXT)))
}

/// The system will ask for the local network: said first, in words (an iPhone), or what an Android phone needs.
fn permission_card(cx: &Cx) -> Dom {
    let words = match cx.env.platform {
        Platform::Ios => cx.t("pair-permission-ios"),
        Platform::Android => cx.t("pair-permission-android"),
    };
    card("width: 100%; padding: 14px; flex-direction: row; display: flex; align-items: center; gap: 12px;")
        .with_child(icon("lan", 26.0, th::ACCENT))
        .with_child(text(&words, &format!("flex-grow: 1; min-width: 0px; font-size: 14px; color: {}; line-height: 1.3;", th::TEXT)))
}

fn code_card(cx: &Cx, code: u32) -> Dom {
    card("width: 100%; padding: 16px; align-items: center; gap: 6px;")
        .with_child(text(&cx.t("pair-code-label"), &format!("font-size: 13px; color: {}; text-align: center;", th::TEXT_DIM)))
        .with_child(text(&format_code(code), "font-size: 44px; font-weight: 700; text-align: center; letter-spacing: 4px;"))
}

/// What a page of this screen is: the content that explains, the buttons that act.
struct Page {
    top: Vec<Dom>,
    bottom: Vec<Dom>,
    note: Option<Dom>,
}

fn page(cx: &Cx) -> Page {
    let landscape = cx.env.landscape();
    // a short screen has no room for the big picture: the keys must stay on it, and not behind a scroll
    let hero_size = if landscape || cx.env.short() { 64.0 } else { 88.0 };
    let view = cx.view();
    let ui = &cx.root.ui;
    if ui.pair_again || matches!(view.session, Session::Unpaired) {
        return Page {
            top: vec![
                hero("devices", th::ACCENT, hero_size),
                title(&cx.t("pair-title")),
                body(&cx.t("pair-intro")),
                permission_card(cx),
            ],
            bottom: {
                // the phone's own app pairs with the text AzReview shows (the clipboard); the simulator's stand-in Mac can also be "found"
                let device = cx.root.sim.is_none() || ui.device_look;
                let mut keys = vec![primary(cx, &cx.t("pair-paste"), Action::PairPaste(String::new()))];
                if !device {
                    keys.push(secondary(cx, &cx.t("pair-find"), None, Action::PairFind));
                }
                keys.push(secondary(cx, &cx.t("pair-scan"), Some("qr_code_scanner"), Action::PairScan));
                if ui.pair_again {
                    keys.push(secondary(cx, &cx.t("pair-back"), Some("arrow_back"), Action::PairBack));
                } else if device {
                    // no Mac at hand: a pretend one inside the app, to try the touch pad, the pinch and the haptics
                    keys.push(
                        tap(&cx.t("pair-demo"), "width: 100%; height: 48px; border-radius: 14px; background: transparent; flex-direction: row; gap: 8px;", cx, Action::Demo)
                            .with_id(DEMO_ID)
                            .with_child(icon("play_circle", 22.0, th::ACCENT))
                            .with_child(text(&cx.t("pair-demo"), &format!("font-size: 17px; font-weight: 600; color: {};", th::ACCENT))),
                    );
                }
                keys
            },
            note: Some(text(&cx.t("pair-cable"), &format!("font-size: 13px; color: {}; text-align: center; line-height: 1.3;", th::TEXT_DIM))),
        };
    }
    match &view.session {
        Session::Searching { attempt } => Page {
            top: vec![
                hero("wifi_find", th::ACCENT, hero_size),
                title(&cx.t("pair-searching-title")),
                body(&cx.t("pair-searching-body")),
                row("gap: 10px; justify-content: center;")
                    .with_child(spinner(16, ColorU { r: 90, g: 175, b: 255, a: 255 }))
                    .with_child(text(&cx.tf("pair-attempt", &[("n", &attempt.to_string())]), &format!("font-size: 13px; color: {};", th::TEXT_DIM))),
            ],
            bottom: vec![secondary(cx, &cx.t("pair-cancel"), None, Action::PairCancel)],
            note: None,
        },
        Session::Pairing { mac_name, code } => Page {
            top: vec![
                hero("computer", th::ACCENT, hero_size),
                title(&cx.tf("pair-found-title", &[("mac", mac_name)])),
                code_card(cx, *code),
                body(&cx.t("pair-found-body")),
                row("gap: 10px; justify-content: center;")
                    .with_child(spinner(16, ColorU { r: 90, g: 175, b: 255, a: 255 }))
                    .with_child(text(&cx.t("pair-waiting"), &format!("font-size: 13px; color: {};", th::TEXT_DIM))),
            ],
            bottom: vec![secondary(cx, &cx.t("pair-cancel"), None, Action::PairCancel)],
            note: None,
        },
        Session::Connected { mac_name } => Page {
            top: vec![
                hero("check_circle", th::STATE_DONE, hero_size),
                title(&cx.tf("pair-connected-title", &[("mac", mac_name)])),
                body(&cx.t("pair-connected-body")),
            ],
            bottom: vec![primary(cx, &cx.t("pair-start"), Action::PairStart)],
            note: None,
        },
        Session::Failed { kind } => {
            let word = fail_word(kind);
            let (glyph, colour) = fail_look(kind);
            let keys = match kind {
                FailureKind::NoPermission => vec![primary(cx, &cx.t("fail-settings"), Action::OpenSystemSettings), secondary(cx, &cx.t("fail-retry"), None, Action::PairRetry)],
                FailureKind::AddressChanged | FailureKind::Forgotten => {
                    vec![primary(cx, &cx.t("fail-pairagain"), Action::PairAgain), secondary(cx, &cx.t("fail-retry"), None, Action::PairRetry)]
                }
                FailureKind::WrongCode | FailureKind::Expired => {
                    vec![primary(cx, &cx.t("fail-paste"), Action::PairPaste(String::new())), secondary(cx, &cx.t("fail-retry"), None, Action::PairRetry)]
                }
                FailureKind::UpdateCompanion | FailureKind::UpdateAzReview => vec![primary(cx, &cx.t("fail-retry"), Action::PairRetry)],
                _ => vec![primary(cx, &cx.t("fail-retry"), Action::PairRetry), secondary(cx, &cx.t("pair-paste"), None, Action::PairPaste(String::new()))],
            };
            Page {
                top: vec![
                    hero(glyph, colour, hero_size),
                    title(&cx.t(&format!("fail-{word}-title"))),
                    body(&cx.t(&format!("fail-{word}-body"))),
                ],
                bottom: keys,
                note: None,
            }
        }
        _ => Page { top: Vec::new(), bottom: Vec::new(), note: None },
    }
}

/// The pairing screen.
#[must_use]
pub fn dom(cx: &Cx) -> Dom {
    let z = &cx.z;
    let landscape = cx.env.landscape();
    let Page { top, bottom, note } = page(cx);
    let _ = PROTOCOL_VERSION;
    let pad = if cx.env.short() { 16.0 } else { 24.0 };
    let mut upper = column("align-items: center; gap: 14px; width: 100%;");
    for dom in top {
        upper.add_child(dom);
    }
    let mut lower = column("align-items: center; gap: 12px; width: 100%;");
    for dom in bottom {
        lower.add_child(dom);
    }
    if let Some(note) = note {
        lower.add_child(note);
    }
    let mut screen = if landscape {
        let half = ((z.safe.w - 3.0 * pad) / 2.0).max(200.0);
        row(&format!("width: {}; height: {}; padding: {} {}; gap: {}; align-items: stretch;", px(z.safe.w), px(z.safe.h), px(pad), px(pad), px(pad)))
            .with_child(column(&format!("width: {}; justify-content: center; overflow-y: auto;", px(half))).with_class("az-scroll").with_child(upper))
            .with_child(column(&format!("width: {}; justify-content: center; overflow-y: auto;", px(half))).with_class("az-scroll").with_child(lower))
    } else {
        // the keys stay clear of the system's bar (48 dp on a screen with room for it): a finger that aims at the lowest key lands on the
        // key, and not in the strip where the system's own gestures begin
        let bottom_pad = if cx.env.short() { pad + 8.0 } else { crate::layout::BAR_CLEARANCE };
        let mut page = column(&format!(
            "width: {}; height: {}; padding: {} {} {} {}; justify-content: space-between; overflow-y: auto; align-items: center;",
            px(z.safe.w),
            px(z.safe.h),
            px(pad + 8.0),
            px(pad),
            px(bottom_pad),
            px(pad)
        ))
        .with_class("az-scroll");
        if !cx.env.short() {
            // a screen with room for it: the page is made to fit its box, and the layout check finds anything of it that would scroll,
            // or a key too near the bar
            page = page.with_class("az-fit");
        }
        page.with_child(upper).with_child(lower)
    };
    if cx.root.ui.pair_again {
        // nothing more: Back is a key of the page
        screen = screen.with_class("az-pair-again");
    }
    screen
}
