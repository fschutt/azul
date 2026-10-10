//! Trusted contacts in AzDrive (feature `encryption`; D51): the recovery code of a drive split
//! 2-of-3 to three people (azul-storage's `crypto::contacts`), and both sides of it.
//!
//! - THE OWNER, Options > Drives > the drive's recovery: "Trusted contacts: Add..." - the code
//!   typed from the kit (checked against the drive's recovery key), three people, each with the
//!   contact key their AzDrive showed them or none (then their share is printed). The shares
//!   made: a sealed one to copy and send by any channel, a printed one to print, save as PDF
//!   or put on a USB stick (a page like the kit). A share handed over counts.
//! - A CONTACT, Options > Drives > "Shares you hold for others": "Be someone's trusted contact"
//!   makes a key for that person (kept in the keyring) and shows it to send; "Take a share"
//!   keeps a sealed share that came (in the settings, sealed; the key opens it); "Help with a
//!   recovery" takes a request, shows its SAFETY NUMBER to compare with the owner in person or
//!   by phone - against scams that fake a friend - and only then answers with the share sealed
//!   to the request.
//! - THE OWNER ON A COMPUTER THAT LOST THE DRIVE, the drive's menu: "Recover with trusted
//!   contacts..." - a request (its key kept in the keyring until the recovery is done) and its
//!   safety number to read to the contacts, then two shares (their replies, or printed shares
//!   typed in). They give back the recovery code, which signs the same lockdown as "Lock down
//!   with the recovery code": the token server holds it 48 hours for the owner's devices to
//!   cancel (D42). The code shows with the kit's buttons; after the 48 hours "Unlock with the
//!   recovery code" opens the drive with it.
//!
//! On stdout, for scripts (never a secret): `AZDRIVE_CONTACTS_SHARED <drive id> <n>` (shares
//! made), `AZDRIVE_CONTACT_KEY` (a key made), `AZDRIVE_SHARE_TAKEN <n>`, `AZDRIVE_SHARE_ANSWERED`,
//! `AZDRIVE_CONTACTS_REQUEST <drive id>`, `AZDRIVE_CONTACTS_RECOVERED <drive id>`.

use azul::{
    callbacks::{ButtonOnClickCallbackType, TextInputOnTextInputCallbackType},
    prelude::*,
    str::String as AzString,
    widgets::{ButtonType, OnTextInputReturn, TextInputState, TextInputValid},
};
use azul_appkit::l10n::{t, t_args, t_label, Arg, Phrase, Text};
use azul_storage::{
    azul_keyring::AzulKeyring,
    crypto::{
        contacts::{
            contact_from_text, contact_text, open_share, read_share, request_from_text,
            request_text, safety_number, seal_reply, seal_share, share_recipient, CodeShare,
            SHARES,
        },
        device::{forget_request_key, load_contact_key, new_contact_key, request_key},
        keys::{MemberPublic, RecoveryCode},
        Zeroizing,
    },
    time::now_unix,
};

use crate::{
    encryption::{Dialog, EncryptionJob},
    ids,
    jobs::Job,
    recovery::{drill_answer, kit_of, paper_buttons, DrillAnswer, Paper, Which},
    recovery_health::{state_mut, state_of, HeldShare, ShareKind, TrustedContact},
    save_settings, spawn,
    ui_dialogs::{button, buttons, label, line, typed_button},
    with_state, DriveState, Popup,
};

// ==== The pages ====

/// One of the three people of the owner's "Add".
#[derive(Default)]
pub(crate) struct Person {
    pub name: String,
    /// Their contact key's text; empty: their share is printed.
    pub key: String,
}

/// A share made, on the "shares made" page.
pub(crate) struct Made {
    pub name: String,
    pub index: u8,
    /// The sealed share's text, to send (a contact with AzDrive).
    pub sealed: Option<String>,
    /// The printed share's text (a contact without AzDrive). A secret.
    pub printed: Option<Zeroizing<String>>,
    pub handed: bool,
}

/// The trusted contacts' dialog pages.
pub(crate) enum Page {
    /// The owner: the code from the kit and three people.
    Add {
        drive_id: String,
        code: Zeroizing<String>,
        people: [Person; 3],
        error: String,
    },
    /// The owner: the shares made, each to send or to print.
    Made {
        drive_id: String,
        shares: Vec<Made>,
        note: String,
    },
    /// A contact: the key just made for an owner, to send to them.
    Key { text: String },
    /// A contact: a sealed share to keep (`answer` false) or a request to answer, pasted.
    Paste {
        answer: bool,
        typed: Zeroizing<String>,
        error: String,
    },
    /// A contact: a request's safety number, the held shares to answer it with, the reply.
    Answer {
        request: String,
        safety: String,
        reply: Option<String>,
        error: String,
    },
    /// The owner on a computer that lost the drive: the request, the shares that came back;
    /// `test`: Options > Drives' Test - the shares are checked here, nothing is locked down.
    Recover {
        test: bool,
        drive_id: String,
        request: String,
        safety: String,
        shares: [Zeroizing<String>; 2],
        error: String,
    },
    /// The code two shares gave back, and the lockdown it started.
    Rebuilt {
        drive_id: String,
        code: Zeroizing<String>,
        until: Option<u64>,
        note: String,
    },
}

/// What the owner's three people are checked into: each one's contact key (`None`: printed).
pub(crate) fn plan_shares(people: &[Person]) -> Result<Vec<Option<MemberPublic>>, String> {
    let mut keys: Vec<Option<MemberPublic>> = Vec::new();
    for (i, person) in people.iter().enumerate() {
        let name = person.name.trim();
        if name.is_empty() {
            return Err(t_args("azdrive-contacts-no-name", &[("n", Arg::from(i + 1))]));
        }
        let key = person.key.trim();
        let key = if key.is_empty() {
            None
        } else {
            Some(contact_from_text(key).ok_or_else(|| {
                t_args("azdrive-contacts-bad-key", &[("name", Arg::from(name))])
            })?)
        };
        if key.is_some() && keys.iter().any(|known| *known == key) {
            return Err(t_args("azdrive-contacts-key-twice", &[("name", Arg::from(name))]));
        }
        keys.push(key);
    }
    Ok(keys)
}

/// The recovery code two shares give back: replies opened with the request's key, printed
/// shares as typed. The code signs the same lockdown as the code typed would. (A worker
/// thread's: its own errors are keys of the resources.)
pub(crate) fn recovered_code(
    texts: &[&str],
    request: &azul_storage::crypto::keys::MemberSecret,
) -> Result<RecoveryCode, String> {
    let shares = texts
        .iter()
        .filter(|text| !text.trim().is_empty())
        .map(|text| read_share(text, request).map_err(|e| e.to_string()))
        .collect::<Result<Vec<CodeShare>, String>>()?;
    if shares.len() < 2 {
        return Err(String::from("azdrive-contacts-two-shares"));
    }
    CodeShare::combine(&shares).map_err(|e| match e {
        azul_storage::crypto::CryptoError::Damaged(_) => {
            String::from("azdrive-contacts-shares-no-code")
        }
        other => other.to_string(),
    })
}

/// What a sealed share says it is from: the owner's words.
fn label_of(s: &DriveState, drive_id: &str) -> String {
    let kit = kit_of(s, drive_id, "");
    if kit.named {
        t_args("azdrive-contacts-label-drive", &[("name", Arg::from(kit.drive_name.as_str()))])
    } else {
        t("azdrive-contacts-label-generic")
    }
}

/// A printed share's page.
fn share_paper(drive_name: &str, made: &Made, secret: &Zeroizing<String>) -> Paper {
    let day = chrono::Local::now().format("%Y-%m-%d").to_string();
    let mut safe: String = made
        .name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    safe = safe.trim_matches('-').chars().take(40).collect();
    let title = t("azdrive-share-title");
    Paper {
        subtitle: t_args(
            "azdrive-share-subtitle",
            &[
                ("index", Arg::from(u32::from(made.index))),
                ("of", Arg::from(u32::from(SHARES))),
                ("name", Arg::from(made.name.as_str())),
                ("day", Arg::from(day)),
            ],
        ),
        address: Vec::new(),
        text: vec![
            t_args("azdrive-share-one-of-three", &[("drive", Arg::from(drive_name))]),
            t("azdrive-share-keep-safe"),
            t("azdrive-share-how-to-use"),
        ],
        label: "azdrive-share-label",
        secret: Zeroizing::new(secret.as_str().to_string()),
        qr_label: "azdrive-share-qr-label",
        file_name: if safe.is_empty() {
            format!("{title} {}.pdf", made.index)
        } else {
            format!("{title} {} - {safe}.pdf", made.index)
        },
        title,
    }
}

/// The page a paper button of the contacts' dialog makes: the rebuilt code's kit, a printed
/// share.
pub(crate) fn paper_of(s: &DriveState, page: &Page, which: Which) -> Option<Paper> {
    match (page, which) {
        (Page::Rebuilt { drive_id, code, .. }, Which::Shown) => {
            Some(kit_of(s, drive_id, code).paper())
        }
        (
            Page::Made {
                drive_id, shares, ..
            },
            Which::Share(row),
        ) => {
            let made = shares.get(row)?;
            let secret = made.printed.as_ref()?;
            Some(share_paper(&label_of(s, drive_id), made, secret))
        }
        _ => None,
    }
}

/// The line under a page's paper buttons.
pub(crate) fn set_note(page: &mut Page, text: String) {
    match page {
        Page::Made { note, .. } | Page::Rebuilt { note, .. } => *note = text,
        _ => {}
    }
}

/// A printed share was printed or saved: it counts as handed over.
pub(crate) fn handed(s: &mut DriveState, which: Which) {
    let Which::Share(row) = which else {
        return;
    };
    let Some(Popup::Encryption(Dialog::Contacts(Page::Made {
        drive_id, shares, ..
    }))) = s.popup.as_mut()
    else {
        return;
    };
    let Some(made) = shares.get_mut(row) else {
        return;
    };
    made.handed = true;
    let (drive_id, index) = (drive_id.clone(), made.index);
    let state = state_mut(&mut s.settings.recovery.drives, &drive_id);
    if let Some(contact) = state.contacts.iter_mut().find(|c| c.index == index) {
        contact.handed = Some(now_unix());
    }
}

fn open(s: &mut DriveState, page: Page) {
    if s.popup.is_none() {
        s.popups_opened += 1;
    }
    s.popup = Some(Popup::Encryption(Dialog::Contacts(page)));
}

/// Options > Drives > "Trusted contacts: Add...".
pub(crate) fn ask_add(s: &mut DriveState, drive_id: &str) {
    open(
        s,
        Page::Add {
            drive_id: drive_id.to_string(),
            code: Zeroizing::new(String::new()),
            people: Default::default(),
            error: String::new(),
        },
    );
}

/// Options > Drives > "Remove" of the trusted contacts: this computer forgets them (their
/// shares open the code until a new code is made).
pub(crate) fn forget(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState, drive_id: &str) {
    let state = state_mut(&mut s.settings.recovery.drives, drive_id);
    state.contacts.clear();
    state.contacts_set = None;
    s.warn(Text::key("azdrive-contacts-forgotten"));
    save_settings(info, app, s);
}

/// "Be someone's trusted contact": a key made for them, on a worker thread.
pub(crate) fn ask_be_contact(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState) {
    spawn(
        info,
        app,
        s,
        Job::Encryption(EncryptionJob::Contacts(ContactsJob::NewKey)),
    );
}

/// "Take a share..." / "Help with a recovery...".
pub(crate) fn ask_paste(s: &mut DriveState, answer: bool) {
    open(
        s,
        Page::Paste {
            answer,
            typed: Zeroizing::new(String::new()),
            error: String::new(),
        },
    );
}

/// The drive's menu: "Recover with trusted contacts..." (`test`: Options > Drives' Test of the
/// contacts): the request first (its key from the keyring, or a new one), on a worker thread.
pub(crate) fn ask_recover(
    info: &mut CallbackInfo,
    app: &RefAny,
    s: &mut DriveState,
    drive_id: &str,
    test: bool,
) {
    let job = ContactsJob::Request {
        drive_id: drive_id.to_string(),
        test,
    };
    spawn(info, app, s, Job::Encryption(EncryptionJob::Contacts(job)));
}

/// The fields of the pages' text boxes.
#[derive(Clone, Copy)]
enum Field {
    Code,
    Name(usize),
    Key(usize),
    Paste,
    Share(usize),
}

struct FieldRef {
    app: RefAny,
    field: Field,
}

fn input(app: &RefAny, field: Field, text: &str, placeholder: &str, id: Option<AzString>) -> Dom {
    let dom = TextInput::create()
        .with_text(AzString::from(text))
        .with_placeholder(azul_appkit::l10n::label(placeholder))
        .with_on_text_input(
            RefAny::new(FieldRef {
                app: app.clone(),
                field,
            }),
            on_field as TextInputOnTextInputCallbackType,
        )
        .dom();
    match id {
        Some(id) => dom.with_id(id),
        None => dom,
    }
}

extern "C" fn on_field(
    mut data: RefAny,
    _info: CallbackInfo,
    state: TextInputState,
) -> OnTextInputReturn {
    let keep = OnTextInputReturn {
        update: Update::DoNothing,
        valid: TextInputValid::Yes,
    };
    let Some((mut app, field)) = data
        .downcast_ref::<FieldRef>()
        .map(|f| (f.app.clone(), f.field))
    else {
        return keep;
    };
    let Some(mut s) = app.downcast_mut::<DriveState>() else {
        return keep;
    };
    let text = state.get_text().as_str().to_string();
    let Some(Popup::Encryption(Dialog::Contacts(page))) = s.popup.as_mut() else {
        return keep;
    };
    match (page, field) {
        (Page::Add { code, error, .. }, Field::Code) => {
            *code = Zeroizing::new(text);
            error.clear();
        }
        (Page::Add { people, error, .. }, Field::Name(i)) => {
            if let Some(person) = people.get_mut(i) {
                person.name = text;
            }
            error.clear();
        }
        (Page::Add { people, error, .. }, Field::Key(i)) => {
            if let Some(person) = people.get_mut(i) {
                person.key = text;
            }
            error.clear();
        }
        (Page::Paste { typed, error, .. }, Field::Paste) => {
            *typed = Zeroizing::new(text);
            error.clear();
        }
        (Page::Recover { shares, error, .. }, Field::Share(i)) => {
            if let Some(share) = shares.get_mut(i) {
                *share = Zeroizing::new(text);
            }
            error.clear();
        }
        _ => {}
    }
    keep
}

fn column(children: Vec<Dom>) -> Dom {
    Dom::create_div()
        .with_css("display: flex; flex-direction: column; min-width: 460px; max-width: 560px;")
        .with_children(DomVec::from(children))
}

fn red(text: &str) -> Dom {
    line(text).with_css("color: #C42B1C;")
}

fn small(text: &str) -> Dom {
    line(text).with_css("font-size: 12px; opacity: 0.75;")
}

/// A text to copy, with its Copy button.
fn copy_row(app: &RefAny, text: &str, id: AzString) -> Dom {
    Dom::create_div()
        .with_css("display: flex; flex-direction: row; align-items: center; margin-top: 6px;")
        .with_child(
            Dom::create_span_with_text(AzString::from(text))
                .with_css(
                    "font-family: monospace; font-size: 11px; flex-grow: 1; overflow-wrap: \
                     anywhere;",
                )
                .with_id(id),
        )
        .with_child(
            Button::create(azul_appkit::l10n::label("azdrive-bridge-copy"))
                .with_on_click(
                    RefAny::new(CopyRef {
                        app: app.clone(),
                        text: text.to_string(),
                    }),
                    on_copy as ButtonOnClickCallbackType,
                )
                .dom()
                .with_css("margin-left: 6px;"),
        )
}

struct CopyRef {
    app: RefAny,
    text: String,
}

extern "C" fn on_copy(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, text)) = data
        .downcast_ref::<CopyRef>()
        .map(|c| (c.app.clone(), c.text.clone()))
    else {
        return Update::DoNothing;
    };
    with_state(&mut app, &mut info, |info, _app, s| {
        crate::ui_bridge::clipboard(info, &text);
        // A sealed share copied counts as sent.
        if let Some(Popup::Encryption(Dialog::Contacts(Page::Made {
            drive_id, shares, ..
        }))) = s.popup.as_mut()
        {
            if let Some(made) = shares
                .iter_mut()
                .find(|m| m.sealed.as_deref() == Some(text.as_str()))
            {
                made.handed = true;
                let (drive_id, index) = (drive_id.clone(), made.index);
                let state = state_mut(&mut s.settings.recovery.drives, &drive_id);
                if let Some(contact) = state.contacts.iter_mut().find(|c| c.index == index) {
                    contact.handed = Some(now_unix());
                }
            }
        }
        s.success(Text::key("azdrive-contacts-copied"));
    })
}

/// The dialog's title and content.
pub(crate) fn dialog_parts(page: &Page, s: &DriveState, app: &RefAny) -> (String, Dom) {
    let name_of = |drive_id: &str| s.drive_name(&crate::browse::Place::folder(drive_id, ""));
    match page {
        Page::Add {
            drive_id,
            code,
            people,
            error,
        } => {
            let mut body = column(vec![
                line("azdrive-contacts-add-what"),
                line("azdrive-contacts-add-keys"),
                label("azdrive-contacts-add-code"),
                input(
                    app,
                    Field::Code,
                    code,
                    "XXXXX-XXXXX-XXXXX-XXXXX-XXXXXX",
                    Some(ids::CONTACTS_CODE),
                ),
            ]);
            for (i, person) in people.iter().enumerate() {
                let person_label = t_args("azdrive-contacts-person", &[("n", Arg::from(i + 1))]);
                body.add_child(label(&person_label));
                body.add_child(
                    Dom::create_div()
                        .with_css("display: flex; flex-direction: row;")
                        .with_child(
                            input(
                                app,
                                Field::Name(i),
                                &person.name,
                                "azdrive-contacts-name",
                                Some(ids::contacts_name(i)),
                            )
                            .with_css("width: 140px; margin-right: 6px;"),
                        )
                        .with_child(
                            input(
                                app,
                                Field::Key(i),
                                &person.key,
                                "azdrive-contacts-key-placeholder",
                                Some(ids::contacts_key(i)),
                            )
                            .with_css("flex-grow: 1;"),
                        ),
                );
            }
            if !error.is_empty() {
                body.add_child(red(error));
            }
            body.add_child(buttons(vec![
                button("kit-button-cancel", app, crate::ui_dialogs::on_cancel_popup),
                typed_button("azdrive-contacts-make", ButtonType::Primary, app, on_make)
                    .with_id(ids::CONTACTS_MAKE),
            ]));
            (
                t_args("azdrive-contacts-add-title", &[("name", Arg::from(name_of(drive_id)))]),
                body,
            )
        }
        Page::Made {
            drive_id,
            shares,
            note,
        } => {
            let mut body = column(vec![line("azdrive-contacts-made-what")]);
            for (row, made) in shares.iter().enumerate() {
                body.add_child(
                    line(&t_args(
                        if made.handed {
                            "azdrive-contacts-made-row-handed"
                        } else {
                            "azdrive-contacts-made-row"
                        },
                        &[
                            ("name", Arg::from(made.name.as_str())),
                            ("index", Arg::from(u32::from(made.index))),
                            ("of", Arg::from(u32::from(SHARES))),
                        ],
                    ))
                    .with_css("font-weight: bold; margin-top: 12px;"),
                );
                if let Some(sealed) = &made.sealed {
                    body.add_child(copy_row(app, sealed, ids::contacts_share(row)));
                } else if let Some(printed) = &made.printed {
                    // The share itself too: to write down for them, or to check the paper.
                    body.add_child(
                        Dom::create_span_with_text(AzString::from(printed.as_str()))
                            .with_css("font-family: monospace; font-size: 13px; margin-top: 4px;")
                            .with_id(ids::contacts_share(row)),
                    );
                    body.add_child(paper_buttons(app, Which::Share(row)));
                }
            }
            if !note.is_empty() {
                body.add_child(small(note));
            }
            body.add_child(buttons(vec![typed_button(
                "azdrive-contacts-done",
                ButtonType::Primary,
                app,
                on_made_done,
            )
            .with_id(ids::CONTACTS_DONE)]));
            (
                t_args("azdrive-contacts-made-title", &[("name", Arg::from(name_of(drive_id)))]),
                body,
            )
        }
        Page::Key { text } => (
            t("azdrive-contacts-key-title"),
            column(vec![
                line("azdrive-contacts-key-what"),
                copy_row(app, text, ids::CONTACT_KEY_TEXT),
                buttons(vec![typed_button(
                    "azdrive-button-close",
                    ButtonType::Primary,
                    app,
                    crate::ui_dialogs::on_cancel_popup,
                )]),
            ]),
        ),
        Page::Paste {
            answer,
            typed,
            error,
        } => {
            let (title, text, placeholder) = if *answer {
                (
                    "azdrive-contacts-help-title",
                    "azdrive-contacts-help-what",
                    "azlin-recover:...",
                )
            } else {
                (
                    "azdrive-contacts-take-title",
                    "azdrive-contacts-take-what",
                    "azlin-share:...",
                )
            };
            let mut body = column(vec![
                line(text),
                input(
                    app,
                    Field::Paste,
                    typed,
                    placeholder,
                    Some(ids::CONTACT_PASTE),
                ),
            ]);
            if !error.is_empty() {
                body.add_child(red(error));
            }
            body.add_child(buttons(vec![
                button("kit-button-cancel", app, crate::ui_dialogs::on_cancel_popup),
                typed_button("azdrive-contacts-continue", ButtonType::Primary, app, on_paste)
                    .with_id(ids::CONTACT_PASTE_OK),
            ]));
            (t(title), body)
        }
        Page::Answer {
            safety,
            reply,
            error,
            ..
        } => {
            let mut body = column(vec![
                line("azdrive-contacts-answer-what"),
                Dom::create_span_with_text(AzString::from(safety.as_str()))
                    .with_css("font-family: monospace; font-size: 22px; margin-top: 10px;")
                    .with_id(ids::SAFETY_NUMBER),
                small("azdrive-contacts-answer-safe"),
            ]);
            match reply {
                Some(reply) => {
                    body.add_child(line("azdrive-contacts-send-answer"));
                    body.add_child(copy_row(app, reply, ids::CONTACT_REPLY));
                    body.add_child(buttons(vec![typed_button(
                        "azdrive-button-close",
                        ButtonType::Primary,
                        app,
                        crate::ui_dialogs::on_cancel_popup,
                    )]));
                }
                None => {
                    let mut row =
                        vec![button("kit-button-cancel", app, crate::ui_dialogs::on_cancel_popup)];
                    for (index, held) in s.settings.recovery.held.iter().enumerate() {
                        if held.sealed.is_empty() {
                            continue;
                        }
                        row.push(
                            Button::with_type(
                                AzString::from(t_args(
                                    "azdrive-contacts-numbers-match",
                                    &[("label", Arg::from(held.label.as_str()))],
                                )),
                                ButtonType::Primary,
                            )
                            .with_on_click(
                                RefAny::new(HeldRef {
                                    app: app.clone(),
                                    index,
                                }),
                                on_answer as ButtonOnClickCallbackType,
                            )
                            .dom()
                            .with_id(ids::contact_answer(index))
                            .with_css("margin-left: 6px;"),
                        );
                    }
                    body.add_child(buttons(row));
                }
            }
            if !error.is_empty() {
                body.add_child(red(error));
            }
            (t("azdrive-contacts-answer-title"), body)
        }
        Page::Recover {
            test,
            drive_id,
            request,
            safety,
            shares,
            error,
        } => {
            let mut body = column(vec![
                line(if *test {
                    "azdrive-contacts-recover-test-what"
                } else {
                    "azdrive-contacts-recover-what"
                }),
                copy_row(app, request, ids::CONTACTS_REQUEST),
                Dom::create_span_with_text(AzString::from(safety.as_str()))
                    .with_css("font-family: monospace; font-size: 22px; margin-top: 10px;")
                    .with_id(ids::SAFETY_NUMBER),
                label("azdrive-contacts-answers"),
            ]);
            for (i, share) in shares.iter().enumerate() {
                body.add_child(input(
                    app,
                    Field::Share(i),
                    share,
                    "azlin-share-reply:... or S1-XXXXXXXX-XXXXX-...",
                    Some(ids::contacts_answer_box(i)),
                ));
            }
            if !error.is_empty() {
                body.add_child(red(error));
            }
            body.add_child(buttons(vec![
                button("kit-button-cancel", app, crate::ui_dialogs::on_cancel_popup),
                typed_button(
                    if *test {
                        "azdrive-drill-check"
                    } else {
                        "azdrive-contacts-recover"
                    },
                    ButtonType::Primary,
                    app,
                    on_recover,
                )
                .with_id(ids::CONTACTS_RECOVER),
            ]));
            let title = t_args(
                if *test {
                    "azdrive-contacts-test-title"
                } else {
                    "azdrive-contacts-recover-title"
                },
                &[("name", Arg::from(name_of(drive_id)))],
            );
            (title, body)
        }
        Page::Rebuilt {
            drive_id,
            code,
            until,
            note,
        } => {
            let until = until.map_or_else(|| String::from("none"), azul_storage::time::iso8601);
            let mut body = column(vec![
                line(&t_args(
                    "azdrive-contacts-rebuilt-what",
                    &[
                        ("name", Arg::from(name_of(drive_id))),
                        ("until", Arg::from(until)),
                    ],
                )),
                Dom::create_span_with_text(AzString::from(code.as_str()))
                    .with_css(
                        "font-family: monospace; font-size: 20px; margin-top: 14px; \
                         margin-bottom: 8px; letter-spacing: 1px;",
                    )
                    .with_id(ids::REBUILT_CODE),
                small("azdrive-contacts-write-it-down"),
            ]);
            for piece in crate::recovery::kit_pieces(app, code, note) {
                body.add_child(piece);
            }
            body.add_child(buttons(vec![typed_button(
                "azdrive-button-close",
                ButtonType::Primary,
                app,
                crate::ui_dialogs::on_cancel_popup,
            )]));
            (t("azdrive-contacts-rebuilt-title"), body)
        }
    }
}

// ==== The owner: the shares ====

extern "C" fn on_make(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |info, app, s| {
        let Some(Popup::Encryption(Dialog::Contacts(Page::Add {
            drive_id,
            code,
            people,
            ..
        }))) = s.popup.as_ref()
        else {
            return;
        };
        let drive_id = drive_id.clone();
        let known = state_of(&s.settings.recovery.drives, &drive_id)
            .and_then(|state| state.recovery_key.clone());
        let checked = match drill_answer(known.as_deref(), &drive_id, code) {
            DrillAnswer::Passed => RecoveryCode::parse(code).ok_or_else(String::new),
            DrillAnswer::NotTheCode => Err(t("azdrive-drill-not-this-drives")),
            DrillAnswer::NotACode => Err(t("azdrive-drill-not-a-code")),
            DrillAnswer::AskTheBucket(_) => Err(t("azdrive-contacts-check-code-first")),
        };
        let plan = plan_shares(people);
        let names: Vec<String> = people.iter().map(|p| p.name.trim().to_string()).collect();
        let result = checked.and_then(|code| {
            let keys = plan?;
            let shares = CodeShare::split(&code).map_err(|e| e.to_string())?;
            let label = label_of(s, &drive_id);
            let mut made = Vec::new();
            for ((share, key), name) in shares.iter().zip(keys).zip(&names) {
                let (sealed, printed) = match key {
                    Some(key) => (
                        Some(seal_share(share, &label, &key).map_err(|e| e.to_string())?),
                        None,
                    ),
                    None => (None, Some(share.to_text())),
                };
                made.push(Made {
                    name: name.clone(),
                    index: share.index(),
                    sealed,
                    printed,
                    handed: false,
                });
            }
            Ok((made, shares[0].set_hex()))
        });
        match result {
            Ok((made, set)) => {
                let state = state_mut(&mut s.settings.recovery.drives, &drive_id);
                state.contacts_set = Some(set);
                state.contacts = made
                    .iter()
                    .map(|m| TrustedContact {
                        name: m.name.clone(),
                        kind: if m.sealed.is_some() {
                            ShareKind::App
                        } else {
                            ShareKind::Printed
                        },
                        index: m.index,
                        handed: None,
                    })
                    .collect();
                println!("AZDRIVE_CONTACTS_SHARED {drive_id} {}", made.len());
                s.popup = Some(Popup::Encryption(Dialog::Contacts(Page::Made {
                    drive_id,
                    shares: made,
                    note: String::new(),
                })));
                save_settings(info, app, s);
            }
            Err(why) => {
                if let Some(Popup::Encryption(Dialog::Contacts(Page::Add { error, .. }))) =
                    s.popup.as_mut()
                {
                    *error = why;
                }
            }
        }
    })
}

extern "C" fn on_made_done(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |info, app, s| {
        s.popup = None;
        save_settings(info, app, s);
    })
}

// ==== A contact: the key, the share, the answer ====

extern "C" fn on_paste(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |info, app, s| {
        let Some(Popup::Encryption(Dialog::Contacts(Page::Paste { answer, typed, .. }))) =
            s.popup.as_ref()
        else {
            return;
        };
        let (answer, text) = (*answer, typed.trim().to_string());
        let problem = if answer {
            match request_from_text(&text) {
                Some(request) => {
                    if s.settings
                        .recovery
                        .held
                        .iter()
                        .all(|held| held.sealed.is_empty())
                    {
                        Some(t("azdrive-contacts-no-share-held"))
                    } else {
                        s.popup = Some(Popup::Encryption(Dialog::Contacts(Page::Answer {
                            safety: safety_number(&request),
                            request: text,
                            reply: None,
                            error: String::new(),
                        })));
                        None
                    }
                }
                None => Some(t("azdrive-contacts-not-a-request")),
            }
        } else if share_recipient(&text).is_some() {
            let job = ContactsJob::Take { text };
            spawn(info, app, s, Job::Encryption(EncryptionJob::Contacts(job)));
            None
        } else {
            Some(t("azdrive-contacts-not-a-share"))
        };
        if let Some(why) = problem {
            if let Some(Popup::Encryption(Dialog::Contacts(Page::Paste { error, .. }))) =
                s.popup.as_mut()
            {
                *error = why;
            }
        }
    })
}

struct HeldRef {
    app: RefAny,
    index: usize,
}

extern "C" fn on_answer(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, index)) = data
        .downcast_ref::<HeldRef>()
        .map(|held| (held.app.clone(), held.index))
    else {
        return Update::DoNothing;
    };
    with_state(&mut app, &mut info, |info, app, s| {
        let Some(Popup::Encryption(Dialog::Contacts(Page::Answer { request, .. }))) =
            s.popup.as_ref()
        else {
            return;
        };
        let Some(sealed) = s
            .settings
            .recovery
            .held
            .get(index)
            .map(|h| h.sealed.clone())
        else {
            return;
        };
        let job = ContactsJob::Answer {
            request: request.clone(),
            sealed,
        };
        spawn(info, app, s, Job::Encryption(EncryptionJob::Contacts(job)));
    })
}

// ==== The owner on another computer: the recovery ====

extern "C" fn on_recover(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |info, app, s| {
        let Some(Popup::Encryption(Dialog::Contacts(Page::Recover {
            test,
            drive_id,
            shares,
            ..
        }))) = s.popup.as_ref()
        else {
            return;
        };
        let (test, drive_id) = (*test, drive_id.clone());
        let shares: Vec<Zeroizing<String>> = shares
            .iter()
            .map(|share| Zeroizing::new(share.as_str().to_string()))
            .collect();
        let Some(token_url) = crate::encryption::token_url_of(s, &drive_id) else {
            if let Some(Popup::Encryption(Dialog::Contacts(Page::Recover { error, .. }))) =
                s.popup.as_mut()
            {
                *error = t("azdrive-no-token-server");
            }
            return;
        };
        let job = ContactsJob::Recover {
            test,
            drive_id,
            shares,
            token_url,
            keyring: s.keyring.clone(),
        };
        spawn(info, app, s, Job::Encryption(EncryptionJob::Contacts(job)));
    })
}

// ==== The jobs ====

/// One blocking task of the trusted contacts (the keyring, the token server).
pub(crate) enum ContactsJob {
    /// A contact key made for an owner.
    NewKey,
    /// A sealed share opened once (with the key it names) and kept sealed.
    Take { text: String },
    /// A held share answered to a request.
    Answer { request: String, sealed: String },
    /// The drive's recovery request (its key from the keyring, else new).
    Request { drive_id: String, test: bool },
    /// Two shares into the code, and the code's lockdown (`test`: no lockdown).
    Recover {
        test: bool,
        drive_id: String,
        shares: Vec<Zeroizing<String>>,
        token_url: String,
        keyring: azcloud_kit::SharedKeyring,
    },
}

/// What a task of the trusted contacts found.
pub(crate) enum ContactsDone {
    NewKey(Result<(String, String), String>),
    Taken {
        text: String,
        result: Result<(String, String, u8), String>,
    },
    Answered(Result<String, String>),
    Request {
        drive_id: String,
        test: bool,
        result: Result<(String, String), String>,
    },
    Recovered {
        drive_id: String,
        result: Result<(Zeroizing<String>, Option<u64>), String>,
    },
    /// A test's two shares gave back a code: its public recovery key (the drive's when they
    /// work).
    Tested {
        drive_id: String,
        result: Result<String, String>,
    },
}

/// Runs on a worker thread.
pub(crate) fn run(job: ContactsJob) -> ContactsDone {
    let keyring = AzulKeyring::new();
    match job {
        ContactsJob::NewKey => ContactsDone::NewKey(
            new_contact_key(&keyring)
                .map(|key| (key.id(), contact_text(&key)))
                .map_err(|e| e.to_string()),
        ),
        ContactsJob::Take { text } => {
            let result = (|| -> Result<(String, String, u8), String> {
                let key = share_recipient(&text).ok_or("azdrive-contacts-err-not-a-share")?;
                let secret = load_contact_key(&keyring, &key)
                    .map_err(|e| e.to_string())?
                    .ok_or("azdrive-contacts-err-other-key")?;
                let (share, label) = open_share(&text, &secret).map_err(|e| e.to_string())?;
                Ok((key.id(), label, share.index()))
            })();
            ContactsDone::Taken { text, result }
        }
        ContactsJob::Answer { request, sealed } => {
            let result = (|| -> Result<String, String> {
                let to = request_from_text(&request).ok_or("azdrive-contacts-err-not-a-request")?;
                let key = share_recipient(&sealed).ok_or("azdrive-contacts-err-damaged")?;
                let secret = load_contact_key(&keyring, &key)
                    .map_err(|e| e.to_string())?
                    .ok_or("azdrive-contacts-err-key-gone")?;
                let (share, _) = open_share(&sealed, &secret).map_err(|e| e.to_string())?;
                seal_reply(&share, &to).map_err(|e| e.to_string())
            })();
            ContactsDone::Answered(result)
        }
        ContactsJob::Request { drive_id, test } => {
            let result = request_key(&keyring, &drive_id)
                .map(|secret| {
                    let public = secret.public();
                    (request_text(&public), safety_number(&public))
                })
                .map_err(|e| e.to_string());
            ContactsDone::Request {
                drive_id,
                test,
                result,
            }
        }
        ContactsJob::Recover {
            test: true,
            drive_id,
            shares,
            ..
        } => {
            let result = (|| -> Result<String, String> {
                let request = request_key(&keyring, &drive_id).map_err(|e| e.to_string())?;
                let texts: Vec<&str> = shares.iter().map(|s| s.as_str()).collect();
                let code = recovered_code(&texts, &request)?;
                let _ = forget_request_key(&keyring, &drive_id);
                Ok(crate::encryption::recovery_key_of(&code, &drive_id).public_base64())
            })();
            ContactsDone::Tested { drive_id, result }
        }
        ContactsJob::Recover {
            test: false,
            drive_id,
            shares,
            token_url,
            keyring: shared,
        } => {
            let result = (|| -> Result<(Zeroizing<String>, Option<u64>), String> {
                let request = request_key(&keyring, &drive_id).map_err(|e| e.to_string())?;
                let texts: Vec<&str> = shares.iter().map(|s| s.as_str()).collect();
                let code = recovered_code(&texts, &request)?;
                let until =
                    crate::encryption::recovery_lockdown(&drive_id, &code, &token_url, &shared)?;
                let _ = forget_request_key(&keyring, &drive_id);
                Ok((code.to_text(), until))
            })();
            ContactsDone::Recovered { drive_id, result }
        }
    }
}

/// The UI thread takes a task's answer.
pub(crate) fn on_done(
    info: &mut CallbackInfo,
    app: &RefAny,
    s: &mut DriveState,
    done: ContactsDone,
) {
    match done {
        ContactsDone::NewKey(Ok((key_id, text))) => {
            println!("AZDRIVE_CONTACT_KEY");
            s.settings.recovery.held.push(HeldShare {
                key_id,
                made: now_unix(),
                ..HeldShare::default()
            });
            open(s, Page::Key { text });
            save_settings(info, app, s);
        }
        ContactsDone::NewKey(Err(why)) => {
            s.error(Phrase::new("azdrive-contacts-no-key").arg("why", t_label(&why)));
        }
        ContactsDone::Taken { text, result } => match result {
            Ok((key_id, label, index)) => {
                println!("AZDRIVE_SHARE_TAKEN {index}");
                let held = match s
                    .settings
                    .recovery
                    .held
                    .iter()
                    .position(|h| h.key_id == key_id)
                {
                    Some(at) => &mut s.settings.recovery.held[at],
                    None => {
                        s.settings.recovery.held.push(HeldShare {
                            key_id,
                            made: now_unix(),
                            ..HeldShare::default()
                        });
                        let last = s.settings.recovery.held.len() - 1;
                        &mut s.settings.recovery.held[last]
                    }
                };
                held.label = label.clone();
                held.sealed = text;
                held.received = Some(now_unix());
                s.popup = None;
                s.success(
                    Phrase::new("azdrive-contacts-taken")
                        .arg("index", u32::from(index))
                        .arg("of", u32::from(SHARES))
                        .arg("label", label.as_str()),
                );
                save_settings(info, app, s);
            }
            Err(why) => {
                if let Some(Popup::Encryption(Dialog::Contacts(Page::Paste { error, .. }))) =
                    s.popup.as_mut()
                {
                    *error = t_args(
                        "azdrive-contacts-not-taken",
                        &[("why", Arg::from(t_label(&why)))],
                    );
                }
            }
        },
        ContactsDone::Answered(result) => {
            if let Some(Popup::Encryption(Dialog::Contacts(Page::Answer {
                reply, error, ..
            }))) = s.popup.as_mut()
            {
                match result {
                    Ok(text) => {
                        println!("AZDRIVE_SHARE_ANSWERED");
                        *reply = Some(text);
                    }
                    Err(why) => {
                        *error = t_args(
                            "azdrive-contacts-no-answer",
                            &[("why", Arg::from(t_label(&why)))],
                        );
                    }
                }
            }
        }
        ContactsDone::Request {
            drive_id,
            test,
            result,
        } => match result {
            Ok((request, safety)) => {
                println!("AZDRIVE_CONTACTS_REQUEST {drive_id}");
                open(
                    s,
                    Page::Recover {
                        test,
                        drive_id,
                        request,
                        safety,
                        shares: Default::default(),
                        error: String::new(),
                    },
                );
            }
            Err(why) => {
                s.error(Phrase::new("azdrive-contacts-no-request").arg("why", t_label(&why)));
            }
        },
        ContactsDone::Tested { drive_id, result } => {
            let known = state_of(&s.settings.recovery.drives, &drive_id)
                .and_then(|state| state.recovery_key.clone());
            let outcome = match result {
                Ok(key) if known.as_deref() == Some(key.as_str()) => Ok(()),
                Ok(_) => Err(t("azdrive-contacts-old-code")),
                Err(why) => Err(t_label(&why)),
            };
            match outcome {
                Ok(()) => {
                    println!("AZDRIVE_CONTACTS_TESTED {drive_id}");
                    s.popup = Some(Popup::Encryption(Dialog::Message {
                        title: t("azdrive-contacts-tested-title"),
                        text: t("azdrive-contacts-tested"),
                    }));
                }
                Err(why) => {
                    if let Some(Popup::Encryption(Dialog::Contacts(Page::Recover {
                        error, ..
                    }))) = s.popup.as_mut()
                    {
                        *error = why;
                    } else {
                        s.error(why);
                    }
                }
            }
        }
        ContactsDone::Recovered { drive_id, result } => match result {
            Ok((code, until)) => {
                println!("AZDRIVE_CONTACTS_RECOVERED {drive_id}");
                println!("AZDRIVE_RECOVERY_LOCKDOWN {drive_id}");
                open(
                    s,
                    Page::Rebuilt {
                        drive_id,
                        code,
                        until,
                        note: String::new(),
                    },
                );
            }
            Err(why) => {
                let why = t_label(&why);
                if let Some(Popup::Encryption(Dialog::Contacts(Page::Recover { error, .. }))) =
                    s.popup.as_mut()
                {
                    *error = why;
                } else {
                    s.error(why);
                }
            }
        },
    }
}

/// Options > Drives > "Shares you hold for others": each held share, and the contact's three
/// doors.
pub(crate) fn held_section(s: &DriveState, app: &RefAny) -> Dom {
    let mut rows: Vec<Dom> = Vec::new();
    for held in &s.settings.recovery.held {
        let text = if held.sealed.is_empty() {
            t("azdrive-contacts-held-key-only")
        } else {
            match held.received {
                Some(at) => {
                    let day = azul_storage::time::iso8601(at);
                    t_args(
                        "azdrive-contacts-held-taken",
                        &[
                            ("label", Arg::from(held.label.as_str())),
                            ("day", Arg::from(day.get(..10).unwrap_or(&day))),
                        ],
                    )
                }
                None => t_args(
                    "azdrive-contacts-held",
                    &[("label", Arg::from(held.label.as_str()))],
                ),
            }
        };
        rows.push(line(&text));
    }
    if rows.is_empty() {
        rows.push(small("azdrive-contacts-held-none"));
    }
    rows.push(
        Dom::create_div()
            .with_css("display: flex; flex-direction: row; flex-wrap: wrap; margin-top: 8px;")
            .with_child(
                button("azdrive-contacts-be-contact", app, on_be_contact)
                    .with_id(ids::CONTACT_BE),
            )
            .with_child(
                button("azdrive-contacts-take", app, on_take).with_id(ids::CONTACT_TAKE),
            )
            .with_child(
                button("azdrive-contacts-help", app, on_help).with_id(ids::CONTACT_HELP),
            ),
    );
    Dom::create_div()
        .with_css("display: flex; flex-direction: column;")
        .with_children(DomVec::from(rows))
}

extern "C" fn on_be_contact(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |info, app, s| {
        ask_be_contact(info, app, s)
    })
}

extern "C" fn on_take(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |_info, _app, s| ask_paste(s, false))
}

extern "C" fn on_help(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |_info, _app, s| ask_paste(s, true))
}

#[cfg(test)]
mod tests {
    use azul_storage::crypto::keys::MemberSecret;

    use super::*;

    fn person(name: &str, key: &str) -> Person {
        Person {
            name: name.to_string(),
            key: key.to_string(),
        }
    }

    #[test]
    fn the_three_people_are_named_and_each_key_is_a_contact_key_of_its_own() {
        crate::l10n::in_english();
        let ada = MemberSecret::generate().unwrap().public();
        let grace = MemberSecret::generate().unwrap().public();
        let people = [
            person("Ada", &contact_text(&ada)),
            person("Grace", &format!("  {}  ", contact_text(&grace))),
            person("Linus", ""),
        ];
        assert_eq!(
            plan_shares(&people).unwrap(),
            vec![Some(ada), Some(grace), None],
            "Linus's share is printed"
        );
        let unnamed = [person("Ada", ""), person(" ", ""), person("Linus", "")];
        assert!(plan_shares(&unnamed).unwrap_err().contains("Person 2"));
        let bad = [
            person("Ada", "azlin-contact:12"),
            person("B", ""),
            person("C", ""),
        ];
        assert!(plan_shares(&bad).unwrap_err().contains("Ada"));
        let twice = [
            person("Ada", &contact_text(&ada)),
            person("Grace", &contact_text(&ada)),
            person("Linus", ""),
        ];
        assert!(plan_shares(&twice).unwrap_err().contains("Grace"));
    }

    #[test]
    fn two_shares_give_back_the_code_whose_recovery_key_signs_the_same_lockdown() {
        let code = RecoveryCode::from_bytes([0x5A; 16]);
        let shares = CodeShare::split(&code).unwrap();
        let request = MemberSecret::generate().unwrap();
        let reply = seal_reply(&shares[0], &request.public()).unwrap();
        let printed = shares[2].to_text();
        let back = recovered_code(&[reply.as_str(), "", printed.as_str()], &request).unwrap();
        assert_eq!(back.as_bytes(), code.as_bytes());
        // The token server's check of the lockdown: the recovery key of the code it gave.
        assert_eq!(
            crate::encryption::recovery_key_of(&back, "d_1").public_base64(),
            crate::encryption::recovery_key_of(&code, "d_1").public_base64()
        );
        assert_eq!(
            recovered_code(&[reply.as_str()], &request).unwrap_err(),
            "azdrive-contacts-two-shares"
        );
        let other = CodeShare::split(&RecoveryCode::from_bytes([0x11; 16])).unwrap();
        assert_eq!(
            recovered_code(&[reply.as_str(), other[1].to_text().as_str()], &request).unwrap_err(),
            "azdrive-contacts-shares-no-code"
        );
    }
}
