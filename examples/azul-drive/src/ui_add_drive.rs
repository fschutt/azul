//! The Add drive dialog's pages (its data is `add_drive`, what its buttons start `add_flow`):
//!
//! ```text
//! Choose     [Buy storage]          Azlin cloud storage, the first month free
//!            [Connect data source]  S3, WebDAV, FTP, Google Drive, GitHub, databases, ...
//! Buy        < Back   the tiers (size, price a month or a year)   [ ] Pay yearly
//!            with the token server's payment options: Country [Germany v], the pills (a
//!            method each, "via <provider>", a switch where two providers offer it), the price,
//!            [ ] the consent the order needs
//!            Name [Azlin Storage]   [Create test drive] (a development server)  [Buy ...]
//!            the payment popover under Buy (its own window): [lock] host - what, legal name;
//!            for a card the artwork (brand, number, name) and Name on card; the provider's
//!            page in a <webview>; the price; [Open in browser instead] [Pay ...]
//! Sources    < Back   the sources of this build in their groups (a tile each)
//! Form       < Back   the source, Name, [Sign in to <provider>] and what the sign-in does
//!            (Google Drive, Dropbox, OneDrive), one field per setting (text, password, a
//!            switch, a choice, a path with Choose...), the test's sentence  [Test connection]
//!            [Cancel] [Add drive]
//! ```
//!
//! The dialog is azul's modal `Dialog` - a `<transient-window>` over AzDrive's window (or the
//! sheet of `--dialogs inline`); its content is a subtree of the window's DOM, so every click
//! here asks EVERY window to rebuild (`RefreshDomAllWindows`): the window builds the new page,
//! the dialog's window shows it. The widgets are azul's (Tile, Button, TextInput, CheckBox,
//! DropDown): they follow the app theme.

use azul::{
    callbacks::{
        ButtonOnClickCallbackType, CheckBoxOnToggleCallbackType, DialogOnCloseCallbackType,
        DropDownOnChoiceChangeCallbackType, TextInputOnTextInputCallbackType,
        TileOnClickCallbackType,
    },
    dialog::{FileDialog, FileOpenResult},
    dom::WebViewEvent,
    misc::TransientAnchor,
    option::OptionFileTypeList,
    prelude::*,
    str::String as AzString,
    vec::StringVec,
    widgets::{
        ButtonType, CheckBoxState, Dialog, DialogClosedBy, DialogState, DropDown,
        OnTextInputReturn, TextInputState, TextInputValid, Tile,
    },
};
use azul_pay::{
    bridge::{clean_name, CardBrand},
    machine::{Chip, Page},
    offer::amount_text,
    pills, Event as PayEvent, Method, SecretUrl, State as PayState, SurfaceKind,
};
use azul_storage::{
    catalog::{FieldKind, FieldSpec, ServiceSpec},
    oauth::OAuthProvider,
};

use crate::{
    add_drive::{source_groups, AddDialog, AddPage, BuyStep, OfferState, TiersState, COUNTRIES},
    add_flow::{self, AddEvent},
    ids, look,
    sign_in::{self, SignInSettings, SignInStep},
    with_state, DriveState, Popup,
};

// ==== Pieces ====

/// Every rebuild the dialog asks for reaches the window that holds its content.
pub(crate) fn everywhere(update: Update) -> Update {
    match update {
        Update::RefreshDom => Update::RefreshDomAllWindows,
        other => other,
    }
}

/// A button or a tile's event.
struct EventRef {
    app: RefAny,
    event: AddEvent,
}

fn event_ref(app: &RefAny, event: AddEvent) -> RefAny {
    RefAny::new(EventRef {
        app: app.clone(),
        event,
    })
}

/// Where a text field writes.
#[derive(Clone, Copy)]
enum TextTarget {
    /// The drive's name (a form's).
    Name,
    /// Buy storage's name.
    BuyName,
    /// The payment popover's cardholder name (drawn on the card artwork).
    CardName,
    /// A form's field.
    Field(&'static str),
    /// "I have a voucher"'s code.
    VoucherCode,
}

/// The app, for a callback that builds its event from what it is given (a choice's index, the
/// web view's report).
struct AppRef {
    app: RefAny,
}

/// The providers of a pill's switch, in the switch's order (indices of the offer's providers).
struct ProviderRef {
    app: RefAny,
    providers: Vec<usize>,
}

struct TextRef {
    app: RefAny,
    target: TextTarget,
}

/// A form's switch, choice or path field.
struct KeyRef {
    app: RefAny,
    key: &'static str,
    /// A path field: a folder (else a file).
    folder: bool,
}

/// The dialog kit's text size (azul's wizard pages, settings rows and standard dialogs: text and
/// field values 13px, hints 12px): the dialog's text and its fields' values are set in it.
const TEXT_SIZE: &str = "font-size: 13px;";

fn label(text: &str) -> Dom {
    Dom::create_span_with_text(AzString::from(text))
        .with_css("font-size: 12px; opacity: 0.75; margin-top: 10px; margin-bottom: 4px;")
}

fn note(text: &str) -> Dom {
    Dom::create_span_with_text(AzString::from(text))
        .with_css("font-size: 12px; opacity: 0.75; margin-top: 4px;")
}

fn line(text: &str) -> Dom {
    Dom::create_span_with_text(AzString::from(text)).with_css("margin-top: 8px;")
}

fn error_line(text: &str) -> Dom {
    line(text)
        .with_id(ids::ADD_ERROR)
        .with_css("color: #C42B1C;")
}

fn heading(text: &str) -> Dom {
    Dom::create_span_with_text(AzString::from(text)).with_css(
        "font-size: 11px; font-weight: bold; text-transform: uppercase; letter-spacing: 1px; \
         opacity: 0.7; margin: 12px 0px 6px 0px;",
    )
}

fn column(children: Vec<Dom>) -> Dom {
    Dom::create_div()
        .with_css("display: flex; flex-direction: column;")
        .with_children(DomVec::from(children))
}

/// The buttons under a page, at its right.
fn buttons(children: Vec<Dom>) -> Dom {
    Dom::create_div()
        .with_css(
            "display: flex; flex-direction: row; justify-content: flex-end; flex-wrap: wrap; \
             margin-top: 16px;",
        )
        .with_children(DomVec::from(children))
}

/// A dialog button running `event`; disabled with `why_not`.
fn button(
    app: &RefAny,
    text: &str,
    kind: ButtonType,
    event: AddEvent,
    id: AzString,
    why_not: Option<&str>,
) -> Dom {
    let mut b = Button::with_type(AzString::from(text), kind)
        .with_on_click(event_ref(app, event), on_event as ButtonOnClickCallbackType);
    if let Some(why) = why_not {
        b = b.with_disabled(AzString::from(why));
    }
    b.dom().with_id(id).with_css("margin-left: 6px; margin-top: 4px;")
}

/// "< Back" at the top of a page.
fn back(app: &RefAny) -> Dom {
    Dom::create_div()
        .with_css("display: flex; flex-direction: row; margin-bottom: 6px;")
        .with_child(
            Button::create(AzString::from("Back"))
                .with_icon(AzString::from("arrow_back"))
                .with_on_click(event_ref(app, AddEvent::Back), on_event as ButtonOnClickCallbackType)
                .dom()
                .with_id(ids::ADD_BACK),
        )
}

/// A big choice or a source: azul's Tile (an icon beside a title over a detail line).
fn tile(
    app: &RefAny,
    icon: &str,
    title: &str,
    detail: &str,
    event: AddEvent,
    selected: bool,
    css: &str,
) -> Dom {
    Tile::create(AzString::from(title))
        .with_icon(AzString::from(icon))
        .with_detail(AzString::from(detail))
        .with_selected(selected)
        .with_on_click(event_ref(app, event), on_event as TileOnClickCallbackType)
        .dom()
        .with_css(format!(
            "{css} {}",
            if selected { look::TILE_SELECTED } else { "" }
        ))
}

/// A text field writing to `target`.
fn text_field(app: &RefAny, value: &str, placeholder: &str, secret: bool, target: TextTarget) -> Dom {
    let base = if secret {
        TextInput::create_password()
    } else {
        TextInput::create()
    };
    base.with_text(AzString::from(value))
        .with_placeholder(AzString::from(placeholder))
        .with_on_text_input(
            RefAny::new(TextRef {
                app: app.clone(),
                target,
            }),
            on_text as TextInputOnTextInputCallbackType,
        )
        .dom()
        // The value in the dialog's text size, not the field's own (smaller) default.
        .with_css(TEXT_SIZE)
}

// ==== The dialog ====

/// The dialog's title and content for `d`; `development`: the token server is a development one
/// (Buy storage offers a test drive).
pub(crate) fn dialog(
    d: &AddDialog,
    development: bool,
    sign_in: &SignInSettings,
    app: &RefAny,
) -> (String, Dom) {
    let (title, page) = match d.page {
        AddPage::Choose => (String::from("Add a drive"), choose(app)),
        AddPage::Buy => (String::from("Buy storage"), buy(d, development, app)),
        AddPage::Sources => (String::from("Connect a data source"), sources(app)),
        AddPage::Voucher => (String::from("Redeem a voucher"), voucher(d, app)),
        AddPage::Form => {
            let title = match (&d.editing, d.spec()) {
                (Some(_), _) => format!("Enter the keys of \"{}\" again", d.name),
                (None, Some(spec)) => format!("Connect {}", spec.name),
                (None, None) => String::from("Connect a data source"),
            };
            (title, form(d, sign_in, app))
        }
    };
    // Text that sets no size of its own (a check box's label, a status line) takes the dialog
    // kit's, not the dialog panel's larger one.
    let content = Dom::create_div()
        .with_id(ids::ADD_DRIVE)
        .with_css("display: flex; flex-direction: column; width: 460px; max-width: 100%;")
        .with_css(TEXT_SIZE)
        .with_child(page);
    (title, content)
}

/// The two choices.
fn choose(app: &RefAny) -> Dom {
    let css = "width: 100%; margin-bottom: 10px; padding: 10px;";
    column(vec![
        tile(
            app,
            "shopping_cart",
            "Buy storage",
            "Azlin cloud storage from 100 GB - the first month is free",
            AddEvent::ChooseBuy,
            false,
            css,
        )
        .with_id(ids::ADD_CHOICE_BUY),
        tile(
            app,
            "cable",
            "Connect data source",
            "S3, WebDAV, FTP, Google Drive, Dropbox, GitHub, databases ...",
            AddEvent::ChooseConnect,
            false,
            css,
        )
        .with_id(ids::ADD_CHOICE_CONNECT),
        buttons(vec![button(
            app,
            "Cancel",
            ButtonType::Default,
            AddEvent::Cancel,
            ids::ADD_CANCEL,
            None,
        )]),
    ])
    .with_id(ids::ADD_CHOOSE)
}

/// Buy storage: the tiers and their prices, monthly or yearly, the name, Create test drive
/// (a development token server), Buy.
fn buy(d: &AddDialog, development: bool, app: &RefAny) -> Dom {
    let mut children = vec![
        back(app),
        Dom::create_span_with_text(AzString::from("Azlin cloud storage"))
            .with_css("font-size: 15px; font-weight: bold;"),
        note(
            "Storage for AzDrive, AzMail and every Azlin app, paid monthly or yearly. The first \
             month is free.",
        ),
    ];
    match &d.tiers {
        TiersState::NotLoaded | TiersState::Loading => {
            children.push(line("Loading the storage tiers ...").with_id(ids::ADD_STATUS));
        }
        TiersState::Failed(why) => {
            children.push(error_line(&format!("The storage tiers could not be loaded: {why}")));
            children.push(buttons(vec![button(
                app,
                "Try again",
                ButtonType::Default,
                AddEvent::RetryTiers,
                ids::ADD_RETRY,
                None,
            )]));
        }
        TiersState::Loaded(tiers) => {
            let cells: Vec<Dom> = tiers
                .tiers
                .iter()
                .enumerate()
                .map(|(index, tier)| {
                    let price = tier
                        .price_text(d.yearly)
                        .unwrap_or_else(|| String::from("price on request"));
                    tile(
                        app,
                        "cloud",
                        &tier.quota_text(),
                        &price,
                        AddEvent::Tier(index),
                        index == d.tier,
                        // two to a line: the width holds the tile's padding
                        "width: 222px; box-sizing: border-box; margin: 0px 8px 8px 0px;",
                    )
                    .with_id(ids::add_tier(index))
                })
                .collect();
            children.push(
                Dom::create_div()
                    .with_css(
                        "display: flex; flex-direction: row; flex-wrap: wrap; margin-top: 10px; \
                         max-height: 260px; overflow-y: auto;",
                    )
                    .with_children(DomVec::from(cells)),
            );
            children.push(
                Dom::create_div()
                    .with_css("display: flex; flex-direction: row; align-items: center;")
                    .with_child(
                        CheckBox::create(d.yearly)
                            .with_accessibility_name(AzString::from("Pay yearly"))
                            .with_on_toggle(
                                event_ref(app, AddEvent::Yearly),
                                on_yearly as CheckBoxOnToggleCallbackType,
                            )
                            .dom()
                            .with_id(ids::ADD_YEARLY),
                    )
                    .with_child(
                        Dom::create_span_with_text(AzString::from(
                            "Pay yearly (two months for free)",
                        ))
                        .with_css("margin-left: 8px;")
                        .with_callback(
                            EventFilter::Hover(HoverEventFilter::Click),
                            event_ref(app, AddEvent::Yearly),
                            on_event,
                        ),
                    ),
            );
            if d.offer().is_some() {
                children.extend(payment(d, app));
            } else if matches!(d.offer, OfferState::Loading) {
                children.push(note("Loading the payment options ..."));
            }
            if !d.pays_with_pills() {
                if let Some(consent) = &tiers.withdrawal_consent {
                    children.push(note(consent));
                }
            }
            children.push(label("Name"));
            children.push(
                text_field(app, &d.buy_name, "Azlin Storage", false, TextTarget::BuyName)
                    .with_id(ids::ADD_NAME),
            );
        }
    }
    // While the popover shows a page, it says what there is to say.
    if !d.notice.is_empty() && d.pay.presenting().is_none() {
        children.push(line(&d.notice).with_id(ids::ADD_STATUS));
    }
    let loaded = matches!(d.tiers, TiersState::Loaded(_))
        && !matches!(d.offer, OfferState::Loading);
    let mut row = Vec::new();
    if d.paying() {
        if let PayState::Waiting {
            browser: Some(_), ..
        } = &d.pay
        {
            row.push(button(
                app,
                "Open the page again",
                ButtonType::Default,
                AddEvent::OpenInBrowser,
                ids::ADD_OPEN_AGAIN,
                None,
            ));
        }
        row.push(button(
            app,
            "Stop waiting",
            ButtonType::Default,
            AddEvent::StopWaiting,
            ids::ADD_STOP,
            None,
        ));
    } else {
        if matches!(d.pay, PayState::Stopped { .. }) {
            row.push(button(
                app,
                "Check again",
                ButtonType::Default,
                AddEvent::CheckAgain,
                ids::ADD_CHECK_AGAIN,
                None,
            ));
        }
        let busy = d.busy().then_some("Wait for the step that runs.");
        let not_loaded = (!loaded).then_some("The storage tiers are not loaded yet.");
        if development {
            row.push(button(
                app,
                "Create test drive",
                ButtonType::Default,
                AddEvent::CreateTestDrive,
                ids::ADD_CREATE_TEST,
                busy.or(not_loaded),
            ));
        }
        row.push(button(
            app,
            "Cancel",
            ButtonType::Default,
            AddEvent::Cancel,
            ids::ADD_CANCEL,
            None,
        ));
        let order = match d.offer().and_then(|o| o.legal.order_button.clone()) {
            Some(words) if d.pays_with_pills() => words,
            _ => d.buy_label(),
        };
        row.push(button(
            app,
            &order,
            ButtonType::Primary,
            AddEvent::Buy,
            ids::ADD_BUY_BUTTON,
            busy.or(not_loaded),
        ));
        // The payment popover hangs off the order button's row: its own window.
        if let Some(popover) = pay_popover(d, app) {
            row.push(popover);
        }
    }
    if matches!(d.step, BuyStep::Paying { .. }) && matches!(d.pay, PayState::Choosing) {
        children.push(note(
            "Paying happens on the payment page in your browser; this window only waits for the \
             drive.",
        ));
    }
    children.push(buttons(row));
    // A voucher is never a pill: a line of its own (azul-pay's pills).
    if !d.paying() {
        children.push(buttons(vec![button(
            app,
            "I have a voucher",
            ButtonType::Default,
            AddEvent::VoucherPage,
            ids::ADD_VOUCHER,
            d.busy().then_some("Wait for the step that runs."),
        )]));
    }
    column(children).with_id(ids::ADD_BUY)
}

/// "I have a voucher": its code, then the new drive it buys - the voucher's months (or its
/// value) of the tier it names, else of the tier chosen; named as Buy storage's name says.
fn voucher(d: &AddDialog, app: &RefAny) -> Dom {
    let mut children = vec![
        back(app),
        note(
            "A voucher buys a new drive: its months, or its value, of the tier it names (else of \
             the tier chosen in Buy storage).",
        ),
        label("The voucher's code"),
        text_field(
            app,
            &d.voucher_code,
            "AZ-XXXX-XXXX",
            false,
            TextTarget::VoucherCode,
        )
        .with_id(ids::ADD_VOUCHER_CODE),
    ];
    if !d.notice.is_empty() {
        children.push(line(&d.notice).with_id(ids::ADD_STATUS));
    }
    children.push(buttons(vec![
        button(
            app,
            "Cancel",
            ButtonType::Default,
            AddEvent::Cancel,
            ids::ADD_CANCEL,
            None,
        ),
        button(
            app,
            "Redeem",
            ButtonType::Primary,
            AddEvent::RedeemVoucher,
            ids::ADD_VOUCHER_REDEEM,
            d.busy().then_some("Wait for the step that runs."),
        ),
    ]));
    column(children)
}

// ==== Buy storage's payment ====

/// The country, the pills (each a method with its provider), a pill's provider switch, the
/// offer's price and the consent the order needs.
fn payment(d: &AddDialog, app: &RefAny) -> Vec<Dom> {
    let Some(offer) = d.offer() else {
        return Vec::new();
    };
    let mut parts = vec![label("Country")];
    let selected = COUNTRIES
        .iter()
        .position(|(code, _)| *code == d.country)
        .unwrap_or(0);
    parts.push(
        DropDown::create(StringVec::from(
            COUNTRIES
                .iter()
                .map(|(_, name)| AzString::from(*name))
                .collect::<Vec<AzString>>(),
        ))
        .with_selected(selected)
        .with_accessibility_name(AzString::from("Country"))
        .with_on_choice_change(
            RefAny::new(AppRef { app: app.clone() }),
            on_country as DropDownOnChoiceChangeCallbackType,
        )
        .dom()
        .with_id(ids::ADD_COUNTRY),
    );
    let shown = d.pills();
    if shown.is_empty() {
        parts.push(note(
            "No payment method of this app is offered for this country: the payment page opens \
             in your browser.",
        ));
        return parts;
    }
    parts.push(label("Pay with"));
    let chosen = d.chosen_pill();
    let cells: Vec<Dom> = shown
        .iter()
        .map(|pill| {
            let pill = match &chosen {
                Some(c) if c.method == pill.method => c,
                _ => pill,
            };
            let selected = chosen.as_ref().is_some_and(|c| c.method == pill.method);
            tile(
                app,
                pill.method.icon(),
                pill.method.label(),
                &pills::via(pill.provider(offer)),
                AddEvent::Pill(pill.method),
                selected,
                "width: 146px; box-sizing: border-box; margin: 0px 8px 8px 0px;",
            )
            .with_id(ids::add_pill(pill.method.as_str()))
        })
        .collect();
    parts.push(
        Dom::create_div()
            .with_css("display: flex; flex-direction: row; flex-wrap: wrap; margin-top: 4px;")
            .with_children(DomVec::from(cells)),
    );
    if let Some(pill) = chosen.as_ref().filter(|p| p.providers.len() > 1) {
        let names: Vec<AzString> = pill
            .providers
            .iter()
            .map(|&i| AzString::from(pills::via(&offer.providers[i])))
            .collect();
        let at = pill
            .providers
            .iter()
            .position(|&i| i == pill.chosen)
            .unwrap_or(0);
        parts.push(
            DropDown::create(StringVec::from(names))
                .with_selected(at)
                .with_accessibility_name(AzString::from("Payment provider"))
                .with_on_choice_change(
                    RefAny::new(ProviderRef {
                        app: app.clone(),
                        providers: pill.providers.clone(),
                    }),
                    on_pill_provider as DropDownOnChoiceChangeCallbackType,
                )
                .dom()
                .with_id(ids::ADD_PROVIDER),
        );
    }
    if let Some(price) = &offer.price {
        parts.push(note(&price.text()).with_id(ids::ADD_PRICE));
    }
    let consent = offer.legal.withdrawal_consent.clone().unwrap_or_else(|| {
        String::from(
            "I ask Azlin to start the service now. If I withdraw, I pay for the service provided \
             until then.",
        )
    });
    parts.push(
        Dom::create_div()
            .with_css("display: flex; flex-direction: row; align-items: center; margin-top: 8px;")
            .with_child(
                CheckBox::create(d.consent)
                    .with_accessibility_name(AzString::from("Consent to the order"))
                    .with_on_toggle(
                        event_ref(app, AddEvent::Consent),
                        on_consent as CheckBoxOnToggleCallbackType,
                    )
                    .dom()
                    .with_id(ids::ADD_CONSENT),
            )
            .with_child(
                Dom::create_span_with_text(AzString::from(consent))
                    .with_css("margin-left: 8px; font-size: 12px; flex-shrink: 1; min-width: 0px;")
                    .with_callback(
                        EventFilter::Hover(HoverEventFilter::Click),
                        event_ref(app, AddEvent::Consent),
                        on_event,
                    ),
            ),
    );
    parts
}

/// The amount the order costs: the offer's price, else the tier's.
fn amount(d: &AddDialog) -> String {
    match d.offer().and_then(|o| o.price.as_ref()) {
        Some(price) => format!("{} {}", price.currency, amount_text(price.amount_cents)),
        None => d
            .chosen_tier()
            .and_then(|t| t.price_text(d.yearly))
            .unwrap_or_default(),
    }
}

/// The popover of the machine's checkout while it shows a page: a `<transient-window>` of the
/// dialog's (azul's Dialog, not modal, anchored under the order button, no light dismiss - an
/// outside click never drops a half-typed payment), with the verified-origin chip, for a card
/// the artwork and the cardholder name, the provider's page in the web view, the price, Pay and
/// "Open in browser instead".
fn pay_popover(d: &AddDialog, app: &RefAny) -> Option<Dom> {
    let (checkout, page) = d.pay.presenting()?;
    let chip = d.pay.chip()?;
    let fields = checkout.surface.kind == SurfaceKind::PopoverFields;
    let card = fields && checkout.choice.method.method == Method::Card;
    let mut parts = vec![chip_dom(&chip)];
    if card {
        parts.push(card_art(d, page));
        parts.push(label("Name on card"));
        parts.push(
            text_field(app, &d.card_name, "Name on card", false, TextTarget::CardName)
                .with_id(ids::PAY_NAME),
        );
    }
    let height = page
        .height
        .unwrap_or(if fields { 190 } else { 440 })
        .clamp(120, 640);
    parts.push(
        Dom::create_div()
            .with_css(look::PAY_SLOT)
            .with_child(pay_webview(app, &checkout.surface.url, height)),
    );
    let what = match d.chosen_tier() {
        Some(tier) => format!(
            "{}, {}",
            tier.quota_text(),
            if d.yearly { "12 months" } else { "1 month" }
        ),
        None => String::new(),
    };
    parts.push(line(&format!("{what}   {}", amount(d))));
    if !d.notice.is_empty() {
        parts.push(line(&d.notice).with_id(ids::PAY_NOTICE));
    }
    let mut row = vec![button(
        app,
        "Open in browser instead",
        ButtonType::Default,
        AddEvent::OpenInBrowser,
        ids::PAY_BROWSER,
        None,
    )];
    if fields {
        let why_not = match &d.pay {
            PayState::Confirming { .. } => Some("The payment is being confirmed."),
            _ if !(page.ready && page.complete) => Some("Fill in the payment details first."),
            _ => None,
        };
        row.push(button(
            app,
            &format!("Pay {}", amount(d)),
            ButtonType::Primary,
            AddEvent::PayConfirm,
            ids::PAY_CONFIRM,
            why_not,
        ));
    }
    parts.push(buttons(row));
    let title = format!(
        "{} {}",
        checkout.choice.method.method.label(),
        pills::via(&checkout.choice.provider)
    );
    let content = Dom::create_div()
        .with_id(ids::PAY_POPOVER)
        .with_css("display: flex; flex-direction: column; width: 360px; max-width: 100%;")
        .with_children(DomVec::from(parts));
    Some(
        Dialog::create(content)
            .with_title(AzString::from(title))
            .with_open(true)
            .with_modal(false)
            .with_anchor(TransientAnchor::Bottom)
            .with_closed_by(DialogClosedBy::None)
            .with_close_button(true)
            .with_on_close(
                event_ref(app, AddEvent::ClosePopover),
                on_popover_closed as DialogOnCloseCallbackType,
            )
            .dom(),
    )
}

/// The verified-origin chip: a lock, the host of the page shown (the last navigation the
/// policy allowed), what it is, the provider's legal name - outside the web view, so no page
/// can draw over it.
fn chip_dom(chip: &Chip) -> Dom {
    let second = match chip.seller {
        Some(seller) => format!("{} - sold by {seller}", chip.legal_name),
        None => chip.legal_name.to_string(),
    };
    Dom::create_div()
        .with_id(ids::PAY_CHIP)
        .with_css(look::PAY_CHIP)
        .with_child(
            Dom::create_icon(AzString::from("lock"))
                .with_css("font-size: 16px; margin-right: 8px; flex-shrink: 0;"),
        )
        .with_child(column(vec![
            Dom::create_div()
                .with_css("display: flex; flex-direction: row; flex-wrap: wrap;")
                .with_child(
                    Dom::create_span_with_text(AzString::from(chip.host.as_str()))
                        .with_id(ids::PAY_HOST)
                        .with_css("font-weight: bold;"),
                )
                .with_child(Dom::create_span_with_text(AzString::from(format!(
                    " - {}",
                    chip.what
                )))),
            Dom::create_span_with_text(AzString::from(second))
                .with_css("font-size: 11px; opacity: 0.8;"),
        ]))
}

/// The card artwork: the brand the fields page reported, the masked number (the provider's
/// last four once it echoes them), the cardholder's name as typed.
fn card_art(d: &AddDialog, page: &Page) -> Dom {
    let brand = page.brand.map_or("", CardBrand::label);
    let number = match &page.last4 {
        Some(last4) => format!("**** **** **** {last4}"),
        None => String::from("**** **** **** ****"),
    };
    let typed = clean_name(&d.card_name).to_uppercase();
    let holder = if typed.is_empty() {
        String::from("CARDHOLDER")
    } else {
        typed
    };
    Dom::create_div()
        .with_id(ids::PAY_CARD)
        .with_css(look::PAY_CARD)
        .with_child(
            Dom::create_div()
                .with_css("display: flex; flex-direction: row; justify-content: space-between;")
                .with_child(
                    Dom::create_span_with_text(AzString::from("AZLIN"))
                        .with_css("font-weight: bold; letter-spacing: 3px;"),
                )
                .with_child(
                    Dom::create_span_with_text(AzString::from(brand))
                        .with_id(ids::PAY_BRAND)
                        .with_css("font-weight: bold; font-style: italic;"),
                ),
        )
        .with_child(
            Dom::create_span_with_text(AzString::from(number))
                .with_css("font-size: 17px; letter-spacing: 2px;"),
        )
        .with_child(
            Dom::create_span_with_text(AzString::from(holder))
                .with_css("font-size: 12px; opacity: 0.9;"),
        )
}

/// The provider's page (`url`) in the web view: every main-frame navigation, finished load and
/// failed load goes to the machine; the marker is how Pay finds it.
fn pay_webview(app: &RefAny, url: &SecretUrl, height: u32) -> Dom {
    let data = RefAny::new(AppRef { app: app.clone() });
    Dom::create_webview(AzString::from(url.reveal()))
        .with_marker(OptionString::Some(AzString::from(ids::PAY_WEBVIEW_MARKER)))
        .with_id(ids::PAY_WEBVIEW)
        .with_css(format!("width: 100%; height: {height}px;"))
        .with_callback(
            EventFilter::Component(ComponentEventFilter::WebViewNavigationRequested),
            data.clone(),
            on_pay_webview,
        )
        .with_callback(
            EventFilter::Component(ComponentEventFilter::WebViewLoadFinished),
            data.clone(),
            on_pay_webview,
        )
        .with_callback(
            EventFilter::Component(ComponentEventFilter::WebViewLoadFailed),
            data,
            on_pay_webview,
        )
}

/// The sources this build can open, in their groups.
fn sources(app: &RefAny) -> Dom {
    let (groups, unavailable) = source_groups();
    let mut list = Vec::new();
    for (group, specs) in groups {
        list.push(heading(group.title()));
        let cells: Vec<Dom> = specs
            .iter()
            .map(|spec| {
                tile(
                    app,
                    spec.icon,
                    spec.name,
                    spec.summary,
                    AddEvent::Service(spec.id),
                    false,
                    // one to a line, so a source's summary is not cut off
                    "width: 100%; box-sizing: border-box; margin-bottom: 4px;",
                )
                .with_id(ids::add_service(spec.id))
            })
            .collect();
        list.push(
            Dom::create_div()
                .with_css("display: flex; flex-direction: column;")
                .with_children(DomVec::from(cells)),
        );
    }
    let mut children = vec![
        back(app),
        note("Choose what to connect. Passwords, tokens and keys stay in the system keyring."),
        Dom::create_div()
            .with_css(
                "display: flex; flex-direction: column; max-height: 380px; overflow-y: auto; \
                 margin-top: 4px;",
            )
            .with_children(DomVec::from(list)),
    ];
    if unavailable > 0 {
        children.push(note(&format!(
            "{unavailable} more sources need a build of AzDrive with OpenDAL or the database \
             drivers."
        )));
    }
    children.push(buttons(vec![button(
        app,
        "Cancel",
        ButtonType::Default,
        AddEvent::Cancel,
        ids::ADD_CANCEL,
        None,
    )]));
    column(children).with_id(ids::ADD_SOURCES)
}

/// One field of a source's form.
fn field(d: &AddDialog, app: &RefAny, f: &'static FieldSpec) -> Dom {
    let title = if f.required {
        f.label.to_string()
    } else {
        format!("{} (optional)", f.label)
    };
    let value = d.value(f.key);
    let control = match f.kind {
        FieldKind::Bool => {
            return Dom::create_div()
                .with_css(
                    "display: flex; flex-direction: row; align-items: center; margin-top: 10px;",
                )
                .with_child(
                    CheckBox::create(d.bool_value(f.key))
                        .with_accessibility_name(AzString::from(f.label))
                        .with_on_toggle(
                            RefAny::new(KeyRef {
                                app: app.clone(),
                                key: f.key,
                                folder: false,
                            }),
                            on_check as CheckBoxOnToggleCallbackType,
                        )
                        .dom()
                        .with_id(ids::add_field(f.key)),
                )
                .with_child(
                    Dom::create_span_with_text(AzString::from(f.label))
                        .with_css("margin-left: 8px;"),
                );
        }
        FieldKind::Choice(words) => {
            let selected = words.iter().position(|w| *w == value).unwrap_or(0);
            DropDown::create(StringVec::from(
                words
                    .iter()
                    .map(|w| AzString::from(*w))
                    .collect::<Vec<AzString>>(),
            ))
            .with_selected(selected)
            .with_accessibility_name(AzString::from(f.label))
            .with_on_choice_change(
                RefAny::new(KeyRef {
                    app: app.clone(),
                    key: f.key,
                    folder: false,
                }),
                on_choice as DropDownOnChoiceChangeCallbackType,
            )
            .dom()
            .with_id(ids::add_field(f.key))
        }
        FieldKind::Path => Dom::create_div()
            .with_css("display: flex; flex-direction: row; align-items: center;")
            .with_child(
                text_field(app, value, f.placeholder, false, TextTarget::Field(f.key))
                    .with_id(ids::add_field(f.key))
                    .with_css("flex-grow: 1; min-width: 0px;"),
            )
            .with_child(
                Button::create(AzString::from("Choose ..."))
                    .with_on_click(
                        RefAny::new(KeyRef {
                            app: app.clone(),
                            key: f.key,
                            // A database is a file; every other path a folder.
                            folder: f.key != "path",
                        }),
                        on_choose_path as ButtonOnClickCallbackType,
                    )
                    .dom()
                    .with_id(ids::add_choose(f.key))
                    .with_css("margin-left: 6px;"),
            ),
        FieldKind::Secret => {
            text_field(app, value, f.placeholder, true, TextTarget::Field(f.key))
                .with_id(ids::add_field(f.key))
        }
        FieldKind::Text | FieldKind::Url | FieldKind::Number => {
            text_field(app, value, f.placeholder, false, TextTarget::Field(f.key))
                .with_id(ids::add_field(f.key))
        }
    };
    let mut parts = vec![label(&title), control];
    if !f.help.is_empty() {
        parts.push(note(f.help));
    }
    column(parts)
}

/// A consumer cloud's sign-in: "Sign in to <provider>" and what it does - or which setting
/// its OAuth client needs (the button is off then, and says it too).
fn sign_in_row(
    d: &AddDialog,
    settings: &SignInSettings,
    app: &RefAny,
    provider: &OAuthProvider,
) -> Dom {
    let missing = sign_in::plan(provider.scheme, settings).err();
    let (text, failed) = match (&d.sign_in, &missing) {
        (SignInStep::Idle, Some(why)) => (why.clone(), true),
        (SignInStep::Idle, None) => (
            String::from(
                "Sign in with your browser; AzDrive keeps the refresh token in this computer's \
                 keyring, never in its files.",
            ),
            false,
        ),
        (SignInStep::Waiting, _) => (
            String::from("Waiting for the sign-in in your browser ..."),
            false,
        ),
        (SignInStep::Exchanging, _) => (String::from("Signing in ..."), false),
        (SignInStep::SignedIn, _) => (
            format!(
                "Signed in to {}. Add drive keeps the refresh token in this computer's keyring.",
                provider.name
            ),
            false,
        ),
        (SignInStep::Failed(why), _) => (why.clone(), true),
    };
    let why_not = missing
        .as_deref()
        .or_else(|| d.signing_in().then_some("The sign-in runs."));
    let label = if matches!(d.sign_in, SignInStep::SignedIn) {
        format!("Sign in to {} again", provider.name)
    } else {
        format!("Sign in to {}", provider.name)
    };
    let mut status = note(&text).with_id(ids::ADD_SIGN_IN_STATUS);
    if failed {
        status = status.with_css("color: #C42B1C; opacity: 1;");
    }
    column(vec![
        Dom::create_div()
            .with_css("display: flex; flex-direction: row; margin-top: 8px;")
            .with_child(button(
                app,
                &label,
                ButtonType::Primary,
                AddEvent::SignIn,
                ids::ADD_SIGN_IN,
                why_not,
            )),
        status,
    ])
}

/// A source's form: its name, its fields, the test's sentence, Test connection / Add drive.
fn form(d: &AddDialog, sign_in: &SignInSettings, app: &RefAny) -> Dom {
    let Some(spec) = d.spec() else {
        return column(vec![back(app), error_line("Choose a source first.")]);
    };
    let mut children = Vec::new();
    if d.editing.is_none() {
        children.push(back(app));
    }
    children.push(header(spec));
    if let Some(provider) = d.sign_in_provider() {
        children.push(sign_in_row(d, sign_in, app, provider));
    }
    let mut fields = vec![
        label("Name"),
        text_field(app, &d.name, spec.name, false, TextTarget::Name).with_id(ids::ADD_NAME),
    ];
    fields.extend(spec.fields.iter().map(|f| field(d, app, f)));
    if spec.read_only {
        fields.push(note(
            "AzDrive browses this source; it does not write to it.",
        ));
    }
    children.push(
        Dom::create_div()
            .with_css(
                "display: flex; flex-direction: column; max-height: 400px; overflow-y: auto; \
                 padding-right: 4px;",
            )
            .with_children(DomVec::from(fields)),
    );
    let status = if d.testing {
        Some((String::from("Testing the connection ..."), false))
    } else {
        match &d.tested {
            Some(Ok(text)) => Some((text.clone(), false)),
            Some(Err(text)) => Some((format!("The connection failed: {text}"), true)),
            None => None,
        }
    };
    if let Some((text, failed)) = status {
        let mut status = line(&text).with_id(ids::ADD_STATUS);
        if failed {
            status = status.with_css("color: #C42B1C;");
        }
        children.push(status);
    }
    if !d.error.is_empty() {
        children.push(error_line(&d.error));
    }
    let busy = d.testing.then_some("The connection test runs.");
    children.push(buttons(vec![
        button(
            app,
            "Test connection",
            ButtonType::Default,
            AddEvent::Test,
            ids::ADD_TEST,
            busy,
        ),
        button(
            app,
            "Cancel",
            ButtonType::Default,
            AddEvent::Cancel,
            ids::ADD_CANCEL,
            None,
        ),
        button(
            app,
            if d.editing.is_some() {
                "Save keys"
            } else {
                "Add drive"
            },
            ButtonType::Primary,
            AddEvent::Save,
            ids::ADD_SAVE,
            None,
        ),
    ]));
    column(children).with_id(ids::ADD_FORM)
}

/// The form's head: the source's icon, name and line.
fn header(spec: &ServiceSpec) -> Dom {
    Dom::create_div()
        .with_css("display: flex; flex-direction: row; align-items: center; margin-bottom: 4px;")
        .with_child(
            Dom::create_icon(AzString::from(spec.icon))
                .with_css("font-size: 28px; margin-right: 10px;"),
        )
        .with_child(column(vec![
            Dom::create_span_with_text(AzString::from(spec.name))
                .with_css("font-size: 15px; font-weight: bold;"),
            note(spec.summary),
        ]))
}

// ==== The callbacks ====

extern "C" fn on_event(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, event)) = data
        .downcast_ref::<EventRef>()
        .map(|r| (r.app.clone(), r.event.clone()))
    else {
        return Update::DoNothing;
    };
    everywhere(with_state(&mut app, &mut info, |info, app, s| {
        add_flow::event(info, app, s, event)
    }))
}

extern "C" fn on_yearly(data: RefAny, info: CallbackInfo, _state: CheckBoxState) -> Update {
    on_event(data, info)
}

extern "C" fn on_consent(data: RefAny, info: CallbackInfo, _state: CheckBoxState) -> Update {
    on_event(data, info)
}

/// The payment popover's close button.
extern "C" fn on_popover_closed(data: RefAny, info: CallbackInfo, _state: DialogState) -> Update {
    on_event(data, info)
}

/// Buy storage's country: its index of `COUNTRIES`.
extern "C" fn on_country(mut data: RefAny, mut info: CallbackInfo, index: usize) -> Update {
    let Some(mut app) = data.downcast_ref::<AppRef>().map(|r| r.app.clone()) else {
        return Update::DoNothing;
    };
    everywhere(with_state(&mut app, &mut info, |info, app, s| {
        add_flow::event(info, app, s, AddEvent::Country(index))
    }))
}

/// A pill's provider switch: its index of the switch's providers.
extern "C" fn on_pill_provider(mut data: RefAny, mut info: CallbackInfo, index: usize) -> Update {
    let Some((mut app, provider)) = data
        .downcast_ref::<ProviderRef>()
        .and_then(|r| r.providers.get(index).map(|&p| (r.app.clone(), p)))
    else {
        return Update::DoNothing;
    };
    everywhere(with_state(&mut app, &mut info, |info, app, s| {
        add_flow::event(info, app, s, AddEvent::PillProvider(provider))
    }))
}

/// The payment page's web view reported: a main-frame navigation it asks about (the machine's
/// policy allows it, or it is cancelled here with `prevent_default` - a bridge message, a
/// return page, a login for the browser, a page off the provider's origins), a finished load
/// (the chip follows), a failed load (the next surface).
extern "C" fn on_pay_webview(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(mut app) = data.downcast_ref::<AppRef>().map(|r| r.app.clone()) else {
        return Update::DoNothing;
    };
    let event = match info.get_webview_event().into_option() {
        Some(WebViewEvent::NavigationRequested(navigation)) => PayEvent::Navigation {
            url: navigation.url.as_str().to_string(),
            redirect: navigation.is_redirect,
        },
        Some(WebViewEvent::LoadFinished(url)) => PayEvent::LoadFinished {
            url: url.as_str().to_string(),
        },
        Some(WebViewEvent::LoadFailed(error)) => PayEvent::LoadFailed {
            reason: error.reason.as_str().to_string(),
        },
        _ => return Update::DoNothing,
    };
    let navigation = matches!(event, PayEvent::Navigation { .. });
    // A navigation nobody decided on is cancelled.
    let mut cancel = navigation;
    let update = with_state(&mut app, &mut info, |info, app, s| {
        cancel = add_flow::pay(info, app, s, event);
    });
    if navigation && cancel {
        info.prevent_default();
    }
    everywhere(update)
}

/// A text field: the dialog keeps what it says (no rebuild: the field shows it already).
extern "C" fn on_text(
    mut data: RefAny,
    _info: CallbackInfo,
    state: TextInputState,
) -> OnTextInputReturn {
    let keep = OnTextInputReturn {
        update: Update::DoNothing,
        valid: TextInputValid::Yes,
    };
    let Some((mut app, target)) = data
        .downcast_ref::<TextRef>()
        .map(|r| (r.app.clone(), r.target))
    else {
        return keep;
    };
    let Some(mut s) = app.downcast_mut::<DriveState>() else {
        return keep;
    };
    let text = state.get_text().as_str().to_string();
    let mut redraw = false;
    if let Some(Popup::AddDrive(d)) = s.popup.as_mut() {
        match target {
            TextTarget::Name => d.set_name(&text),
            TextTarget::BuyName => d.buy_name = text,
            // The card artwork shows the name as it is typed: the window that holds the
            // popover's content builds it anew.
            TextTarget::CardName => {
                d.card_name = text;
                redraw = true;
            }
            TextTarget::Field(key) => d.set_value(key, &text),
            TextTarget::VoucherCode => {
                d.voucher_code = text;
                d.notice.clear();
            }
        }
    }
    OnTextInputReturn {
        update: if redraw {
            Update::RefreshDomAllWindows
        } else {
            Update::DoNothing
        },
        valid: TextInputValid::Yes,
    }
}

/// A switch of the form.
extern "C" fn on_check(mut data: RefAny, mut info: CallbackInfo, _state: CheckBoxState) -> Update {
    let Some((mut app, key)) = data
        .downcast_ref::<KeyRef>()
        .map(|r| (r.app.clone(), r.key))
    else {
        return Update::DoNothing;
    };
    everywhere(with_state(&mut app, &mut info, |_info, _app, s| {
        if let Some(Popup::AddDrive(d)) = s.popup.as_mut() {
            d.toggle(key);
        }
    }))
}

/// A choice of the form.
extern "C" fn on_choice(mut data: RefAny, mut info: CallbackInfo, index: usize) -> Update {
    let Some((mut app, key)) = data
        .downcast_ref::<KeyRef>()
        .map(|r| (r.app.clone(), r.key))
    else {
        return Update::DoNothing;
    };
    everywhere(with_state(&mut app, &mut info, |_info, _app, s| {
        if let Some(Popup::AddDrive(d)) = s.popup.as_mut() {
            d.choose(key, index);
        }
    }))
}

/// "Choose ...": the system's folder (or file) dialog fills a path field.
extern "C" fn on_choose_path(mut data: RefAny, _info: CallbackInfo) -> Update {
    let Some((app, key, folder)) = data
        .downcast_ref::<KeyRef>()
        .map(|r| (r.app.clone(), r.key, r.folder))
    else {
        return Update::DoNothing;
    };
    let picker = RefAny::new(KeyRef { app, key, folder });
    if folder {
        let _request = FileDialog::open_directory(
            AzString::from("Choose a folder"),
            OptionString::None,
            picker,
            on_path_picked,
        );
    } else {
        let _request = FileDialog::open_file(
            AzString::from("Choose a database file"),
            OptionString::None,
            OptionFileTypeList::None,
            picker,
            on_path_picked,
        );
    }
    Update::DoNothing
}

/// The system's dialog answered: the path goes into its field.
extern "C" fn on_path_picked(mut data: RefAny, mut info: CallbackInfo, result: RefAny) -> Update {
    let Some(picked) = FileOpenResult::downcast(result).into_option() else {
        return Update::DoNothing;
    };
    let Some(path) = picked.path.into_option() else {
        return Update::DoNothing; // cancelled
    };
    let text = path.inner.as_str().to_string();
    let Some((mut app, key)) = data
        .downcast_ref::<KeyRef>()
        .map(|r| (r.app.clone(), r.key))
    else {
        return Update::DoNothing;
    };
    everywhere(with_state(&mut app, &mut info, |_info, _app, s| {
        if let Some(Popup::AddDrive(d)) = s.popup.as_mut() {
            d.set_value(key, &text);
        }
    }))
}

#[cfg(test)]
mod tests {
    use azcloud_kit::{Tier, Tiers};
    use azul::css::{CssDeclaration, CssProperty, StyleFontSizeValue};

    use super::*;

    /// The font size (px) `dom`'s own sheets give it - the app's `with_css`, never a widget's
    /// UA default (rule priority 0).
    fn own_font_size(dom: &Dom) -> Option<f32> {
        dom.css
            .as_slice()
            .iter()
            .flat_map(|css| css.rules.as_slice().iter())
            .filter(|rule| rule.priority > 0)
            .flat_map(|rule| rule.declarations.as_slice().iter())
            .filter_map(|declaration| match declaration {
                CssDeclaration::Static(CssProperty::FontSize(StyleFontSizeValue::Exact(size))) => {
                    Some(size.inner.number.get())
                }
                _ => None,
            })
            .next_back()
    }

    /// The size the text `text` is set in inside `dom`: the nearest of its boxes that sets one
    /// (`None`: nothing inside `dom` does, the text takes what is around the dialog). `None`
    /// outside: no such text.
    fn size_of_text(dom: &Dom, text: &str, around: Option<f32>) -> Option<Option<f32>> {
        let here = own_font_size(dom).or(around);
        if dom.root.node_type.get_text().into_option().is_some_and(|t| t.as_str() == text) {
            return Some(here);
        }
        dom.children.as_slice().iter().find_map(|child| size_of_text(child, text, here))
    }

    /// The box with the id `id` in `dom`.
    fn with_id<'a>(dom: &'a Dom, id: &AzString) -> Option<&'a Dom> {
        if dom.root.has_id(id.clone()) {
            return Some(dom);
        }
        dom.children.as_slice().iter().find_map(|child| with_id(child, id))
    }

    /// Buy storage with two tiers loaded.
    fn buy_page() -> Dom {
        let tier = |id: &str, gb: u64| Tier {
            id: id.to_string(),
            quota_bytes: gb * 1_000_000_000,
            price_cents_month: Some(99),
            price_cents_year: Some(990),
            currency: "EUR".to_string(),
            first_month_free: true,
        };
        let mut d = AddDialog::new(1);
        d.choose_buy();
        d.tiers = TiersState::Loaded(Tiers {
            tiers: vec![tier("100GB", 100), tier("1TB", 1000)],
            methods: vec!["sepa".to_string()],
            withdrawal_consent: None,
        });
        dialog(&d, false, &RefAny::new(())).1
    }

    /// The dialog kit's sizes (azul's wizard pages, settings rows and standard dialogs): text and
    /// field values 13px, hints 12px. The yearly label took the dialog panel's 14px and the name
    /// field its own 11px default.
    #[test]
    fn the_buy_pages_yearly_label_and_name_field_are_set_in_the_dialog_kits_text_size() {
        let page = buy_page();
        assert_eq!(
            size_of_text(&page, "Pay yearly (two months for free)", None),
            Some(Some(13.0)),
            "the yearly label: the dialog's text size"
        );
        let name = with_id(&page, &ids::ADD_NAME).expect("the name field");
        assert_eq!(own_font_size(name), Some(13.0), "the name: the dialog's text size");
    }
}
