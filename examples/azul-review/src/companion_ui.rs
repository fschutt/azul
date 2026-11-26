//! What the phone companion shows besides its switch: the phones that are paired (and a key to forget each), the pairing code that is
//! open (as text to copy and, when it is short enough, as a picture to scan), and the bar over the window that asks the person to say
//! that the six digits are the same on both screens.
//!
//! The state is `companion_plug::Glue`'s; this builds the DOM from it and answers its keys. The code holds a one-time secret: it goes to
//! the clipboard as a secret (flagged, taken off again after a minute) and is never logged.

use azul::{prelude::*, str::String as AzString};
use azul_appkit::{
    l10n::{label, t_args, Arg},
    secret_clipboard,
    ui as kit,
};
use azul_review_link::{host::PhoneInfo, limits::MAX_PAIRED_PHONES, pairing::format_code};

use crate::{
    companion_plug::{save, with_companion, Status},
    ids, AppState,
};

/// What is said when the clipboard's copy of the code is taken off again.
const CLEARED: &str = "AZREVIEW_COMPANION the pairing code was taken off the clipboard";
/// The side of a module of the code's picture, px.
const QR_MODULE_PX: usize = 4;

const BUTTON: &str = "display: flex; flex-direction: row; align-items: center; justify-content: center; gap: 6px; height: 30px; \
                      padding: 0px 14px; font-size: 12px; border-radius: 6px; border: 1px solid system:separator; \
                      background: transparent; color: system:text; cursor: pointer; flex-shrink: 0;";
const BUTTON_PRIMARY: &str = "display: flex; flex-direction: row; align-items: center; justify-content: center; gap: 6px; height: 30px; \
                              padding: 0px 14px; font-size: 12px; border-radius: 6px; border: 0px solid transparent; \
                              background: system:accent; color: system:accent-text; cursor: pointer; flex-shrink: 0;";

/// Which phone a key is for.
struct PhoneTag {
    peer: String,
}

fn button(id: AzString, words: &AzString, primary: bool, app: &RefAny, on_click: extern "C" fn(RefAny, CallbackInfo) -> Update) -> Dom {
    Dom::create_node(NodeType::Button)
        .with_css(if primary { BUTTON_PRIMARY } else { BUTTON })
        .with_accessibility_name(words.clone())
        .with_callback(EventFilter::Hover(HoverEventFilter::Click), app.clone(), on_click)
        .with_id(id)
        .with_child(Dom::create_span_with_text(words.as_str()))
}

/// The companion's block of the settings page, under the switch: the key that makes a pairing code (or the code that is open, or why
/// there is none) and the list of the phones that are paired. `None` while there is nothing to say: off, and no phone kept.
#[must_use]
pub fn phones_block(s: &AppState, app: &RefAny) -> Option<Dom> {
    let g = s.companion.try_borrow().ok()?;
    let listening = matches!(g.status(), Status::Listening { .. });
    let phones = g.phones();
    if !listening && phones.is_empty() {
        return None;
    }
    let mut block = Dom::create_div().with_css("display: flex; flex-direction: column; gap: 10px; padding: 10px 0px 0px 0px;");
    if listening {
        if let Some(offer) = g.offer() {
            block.add_child(offer_box(offer, app));
        } else if phones.len() >= MAX_PAIRED_PHONES {
            block.add_child(kit::note("azreview-companion-full"));
        } else if !g.has_address() {
            block.add_child(kit::note("azreview-companion-no-address"));
        } else {
            block.add_child(button(ids::COMPANION_PAIR, &label("azreview-companion-pair"), true, app, on_pair));
        }
    }
    if phones.is_empty() {
        if listening {
            block.add_child(kit::note("azreview-companion-no-phones"));
        }
    } else {
        let mut list = Dom::create_div()
            .with_id(ids::COMPANION_PHONES)
            .with_css("display: flex; flex-direction: column; gap: 6px;")
            .with_child(
                Dom::create_span_with_text(label("azreview-companion-phones"))
                    .with_css("font-size: 13px; font-weight: 600; padding: 2px 0px;"),
            );
        for (index, phone) in phones.iter().enumerate() {
            list.add_child(phone_row(index, phone, app));
        }
        block.add_child(list);
    }
    Some(block)
}

/// One paired phone: a dot (green while it is here), its name, whether it is here, and the key that forgets it.
fn phone_row(index: usize, phone: &PhoneInfo, app: &RefAny) -> Dom {
    let (dot, state) = if phone.connected { ("#2fa84f", "azreview-companion-here") } else { ("#9a9a9a", "azreview-companion-away") };
    let forget = t_args("azreview-companion-forget-name", &[("name", Arg::from(phone.name.clone()))]);
    Dom::create_div()
        .with_id(AzString::from(format!("__azreview_companion-phone-{index}")))
        .with_css("display: flex; flex-direction: row; align-items: center; gap: 10px; font-size: 12px;")
        .with_child(Dom::create_div().with_accessibility_info(decorative()).with_css(format!("width: 8px; height: 8px; border-radius: 4px; flex-shrink: 0; background: {dot};").as_str()))
        .with_child(Dom::create_span_with_text(phone.name.as_str()).with_css("font-weight: 600;"))
        .with_child(Dom::create_span_with_text(label(state)).with_css("color: system:secondary-text; flex-grow: 1;"))
        .with_child(
            Dom::create_node(NodeType::Button)
                .with_css(BUTTON)
                .with_accessibility_name(AzString::from(forget))
                .with_dataset(OptionRefAny::Some(RefAny::new(PhoneTag { peer: phone.id.clone() })))
                .with_callback(EventFilter::Hover(HoverEventFilter::Click), app.clone(), on_forget)
                .with_id(AzString::from(format!("__azreview_companion-forget-{index}")))
                .with_child(Dom::create_span_with_text(label("azreview-companion-forget"))),
        )
}

/// The code that is open: what to do with it on the phone, the code as text, its picture (when there is one), and the keys that copy
/// it and close the window.
fn offer_box(offer: &crate::companion_plug::Offer, app: &RefAny) -> Dom {
    let mut row = Dom::create_div().with_css("display: flex; flex-direction: row; gap: 14px; align-items: flex-start; flex-wrap: wrap;");
    if let Some(symbol) = &offer.qr {
        row.add_child(Dom::create_div().with_id(ids::COMPANION_QR).with_accessibility_info(decorative()).with_child(kit::qr_dom(symbol, QR_MODULE_PX)));
    }
    let mut side = Dom::create_div()
        .with_css("display: flex; flex-direction: column; gap: 8px; flex-grow: 1; min-width: 220px;")
        .with_child(Dom::create_span_with_text(label("azreview-companion-offer-title")).with_css("font-size: 13px; font-weight: 600;"))
        .with_child(Dom::create_p_with_text(label("azreview-companion-offer-help")).with_css("margin: 0px; font-size: 12px; color: system:secondary-text;"))
        .with_child(
            Dom::create_p_with_text(AzString::from(offer.text.as_str()))
                .with_id(ids::COMPANION_OFFER_TEXT)
                .with_accessibility_name(label("azreview-companion-offer-text-name"))
                .with_css(
                    "margin: 0px; font-family: monospace; font-size: 11px; word-break: break-all; user-select: text; \
                     padding: 8px; border-radius: 6px; border: 1px solid system:separator;",
                ),
        );
    let keys = Dom::create_div()
        .with_css("display: flex; flex-direction: row; gap: 8px; flex-wrap: wrap;")
        .with_child(button(ids::COMPANION_COPY, &label("azreview-companion-copy"), true, app, on_copy))
        .with_child(button(ids::COMPANION_CLOSE, &label("azreview-companion-close"), false, app, on_close_offer));
    side.add_child(keys);
    if offer.copied {
        side.add_child(Dom::create_span_with_text(label("azreview-companion-copied")).with_css("font-size: 11px; color: system:secondary-text;"));
    }
    row.add_child(side);
    Dom::create_div()
        .with_id(ids::COMPANION_OFFER)
        .with_css("display: flex; flex-direction: column; gap: 8px; padding: 12px; border-radius: 8px; border: 1px solid system:separator;")
        .with_child(row)
}

/// The bar over the window while a phone waits for the person: its name, the six digits both screens show, and the keys that say yes
/// and no. `None` when no phone asks.
#[must_use]
pub fn pairing_bar(s: &AppState, app: &RefAny) -> Option<Dom> {
    let g = s.companion.try_borrow().ok()?;
    let asked = g.asked()?;
    let words = t_args("azreview-companion-asks", &[("name", Arg::from(asked.name.clone()))]);
    let said = t_args("azreview-companion-code-name", &[("name", Arg::from(asked.name.clone())), ("code", Arg::from(format_code(asked.code)))]);
    Some(
        Dom::create_div()
            .with_id(ids::COMPANION_ASK)
            .with_accessibility_name(AzString::from(said))
            .with_css(
                "display: flex; flex-direction: row; align-items: center; gap: 14px; flex-wrap: wrap; padding: 8px 14px; flex-shrink: 0; \
                 border-left: 4px solid system:accent; border-bottom: 1px solid system:separator; background: system:control-background; \
                 font-size: 13px;",
            )
            .with_child(Dom::create_span_with_text(words.as_str()).with_css("font-weight: 600;"))
            .with_child(
                Dom::create_span_with_text(format_code(asked.code).as_str())
                    .with_id(ids::COMPANION_CODE)
                    .with_css("font-family: monospace; font-size: 20px; font-weight: 700; letter-spacing: 2px;"),
            )
            .with_child(Dom::create_span_with_text(label("azreview-companion-same")).with_css("color: system:secondary-text; flex-grow: 1;"))
            .with_child(button(ids::COMPANION_ACCEPT, &label("azreview-companion-accept"), true, app, on_accept))
            .with_child(button(ids::COMPANION_DECLINE, &label("azreview-companion-decline"), false, app, on_decline)),
    )
}

fn decorative() -> AccessibilityInfo {
    AccessibilityInfo { role: AccessibilityRole::Nothing, ..AccessibilityInfo::create_default() }
}

// ==== the keys ====

/// "Pair a phone": the window opens and the code is made.
extern "C" fn on_pair(data: RefAny, _info: CallbackInfo) -> Update {
    let made = with_companion(&data, |g, _| {
        let now = g.now();
        g.begin_pairing(now)
    });
    println!("AZREVIEW_COMPANION pair {}", match made {
        Some(Ok(())) => "code made",
        Some(Err(_)) | None => "refused",
    });
    Update::RefreshDom
}

/// "Copy pairing code": the code goes to the clipboard as a secret and leaves it after a minute.
extern "C" fn on_copy(data: RefAny, mut info: CallbackInfo) -> Update {
    let text = with_companion(&data, |g, _| g.mark_copied()).flatten();
    if let Some(text) = text {
        secret_clipboard::clipboard_secret(&mut info, &text, CLEARED);
    }
    Update::RefreshDom
}

/// "Close": the window closes, the code no longer works (and is taken off the clipboard if it is still there).
extern "C" fn on_close_offer(data: RefAny, mut info: CallbackInfo) -> Update {
    let text = with_companion(&data, |g, _| {
        let text = g.offer().map(|o| o.text.clone());
        g.cancel_pairing();
        text
    })
    .flatten();
    if let Some(text) = text {
        secret_clipboard::clipboard_forget(&mut info, &text);
    }
    Update::RefreshDom
}

/// "Pair": the six digits are the same on both screens.
extern "C" fn on_accept(data: RefAny, mut info: CallbackInfo) -> Update {
    let paired = with_companion(&data, |g, _| {
        let now = g.now();
        g.accept(now)
    });
    if matches!(paired, Some(Ok(()))) {
        save(&data, &mut info);
        println!("AZREVIEW_COMPANION paired");
    }
    Update::RefreshDom
}

/// "Decline": they are not.
extern "C" fn on_decline(data: RefAny, _info: CallbackInfo) -> Update {
    with_companion(&data, |g, _| {
        let now = g.now();
        g.decline(now);
    });
    Update::RefreshDom
}

/// "Forget this phone": it is disconnected, refused from now on, and not kept.
extern "C" fn on_forget(data: RefAny, mut info: CallbackInfo) -> Update {
    let peer = info.get_dataset(info.get_hit_node()).into_option().and_then(|mut d| d.downcast_ref::<PhoneTag>().map(|t| t.peer.clone()));
    let Some(peer) = peer else {
        return Update::DoNothing;
    };
    let forgot = with_companion(&data, |g, _| {
        let now = g.now();
        g.forget(now, &peer)
    })
    .unwrap_or(false);
    if forgot {
        save(&data, &mut info);
    }
    Update::RefreshDom
}

#[cfg(test)]
#[path = "companion_ui_test.rs"]
mod companion_ui_test;
