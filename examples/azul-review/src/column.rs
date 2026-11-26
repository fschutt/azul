//! The side column: the notes dictated at the pen's rests, one card each (where it is anchored, its
//! words - live while it listens, editable afterwards - and a delete button), the "listening" state
//! and the stop button over them; the markers the notes put on the pages; the settings page's
//! dictation section.

use azul::{
    callbacks::{
        ButtonOnClickCallbackType, SegmentedOnChangeCallbackType, TextAreaOnFocusLostCallbackType,
        TextAreaOnTextInputCallbackType,
    },
    prelude::*,
    widgets::{Button, Segmented, SegmentedState, TextArea},
};
use azul_appkit::{
    l10n::{label, labels, t, t_args, Arg},
    settings::AppSettings,
    ui as kit,
};

use crate::{
    code::SourceFile,
    dictation::Mic,
    dwell::{self, Allow},
    ids,
    model::Note,
    speech_gate::Session as SpeechState,
    standby::{self, Standby},
    switches::{self, KEY_INPUT, KEY_REST, KEY_SILENCE, KEY_STANDBY, KEY_STANDBY_IDLE},
    ui, voice, AppState,
};

/// The column's width, px.
pub const COLUMN_W: f32 = 300.0;
/// The marker's diameter on the page, px.
pub const MARKER_D: f32 = 22.0;

/// The rest times the settings page offers, ms.
pub static REST_PRESETS: [u64; 4] = [800, 1_200, 2_000, 3_000];
/// The silence times the settings page offers, ms.
pub static SILENCE_PRESETS: [u64; 4] = [5_000, 10_000, 20_000, 40_000];
/// The idle times (how long the pen is away before the microphone is closed) the settings page offers, ms.
pub static IDLE_PRESETS: [u64; 4] = [15_000, 30_000, 120_000, 600_000];

/// What a note's controls carry: the app and the note's id.
pub struct NoteTag {
    pub app: RefAny,
    pub id: u64,
}

/// The place a note names: its page and the line of the file under its anchor.
fn place(file: Option<&SourceFile>, note: &Note) -> String {
    let first = file.map_or(1, |f| f.page(note.page).0);
    t_args(
        "azreview-note-place",
        &[
            ("page", Arg::from(note.page + 1)),
            ("line", Arg::from(first + ui::line_of(note.y))),
        ],
    )
}

/// The notes of the open file, in the order they were made, with the number each one shows.
fn numbered(s: &AppState) -> impl Iterator<Item = (usize, &Note)> {
    s.notes.iter().enumerate().map(|(i, n)| (i + 1, n))
}

/// A round badge: the note's number, or the microphone while it listens. The same on the page
/// (the marker) and in its card.
fn badge(number: usize, listening: bool, size: f32) -> Dom {
    let (fill, ring) = if listening {
        ("#d62d20", "rgba(214,45,32,0.30)")
    } else {
        ("#0d7a73", "rgba(13,122,115,0.28)")
    };
    let face = if listening {
        Dom::create_icon("mic").with_css("font-size: 13px;")
    } else {
        Dom::create_span_with_text(format!("{number}").as_str())
    };
    Dom::create_div()
        .with_css(
            format!(
                "display: flex; flex-direction: row; align-items: center; justify-content: \
                 center; flex-shrink: 0; width: {size}px; height: {size}px; border-radius: \
                 {}px; background: {fill}; box-shadow: 0px 0px 0px 3px {ring}; color: #ffffff; \
                 font-size: 11px; font-weight: 600;",
                size / 2.0
            )
            .as_str(),
        )
        .with_child(face)
}

/// The marker of `note` on its page: its badge, centred on the anchor. Under the pen layer, so it
/// never takes a stroke's pointer events.
#[must_use]
pub fn marker(note: &Note, number: usize, listening: bool) -> Dom {
    Dom::create_div()
        .with_id(format!("__azreview_marker-{}", note.id).as_str())
        .with_css(
            format!(
                "position: absolute; left: {}px; top: {}px;",
                note.x - MARKER_D / 2.0,
                note.y - MARKER_D / 2.0
            )
            .as_str(),
        )
        .with_child(badge(number, listening, MARKER_D))
}

/// The markers of the notes on `page` (`listening`: the id of the note that is listening now).
#[must_use]
pub fn markers(notes: &[Note], page: usize, listening: Option<u64>) -> Vec<Dom> {
    notes
        .iter()
        .enumerate()
        .filter(|(_, n)| n.page == page)
        .map(|(i, n)| marker(n, i + 1, listening == Some(n.id)))
        .collect()
}

/// The column's header: its title, and while a dictation runs the "listening" pill and the stop button.
fn header(s: &AppState, data: &RefAny) -> Dom {
    let mut row = Dom::create_div().with_css(
        "display: flex; flex-direction: row; align-items: center; gap: 6px; padding: 7px 10px; \
         flex-shrink: 0; border-bottom: 1px solid system:separator; user-select: none;",
    );
    row.add_child(Dom::create_icon("speaker_notes").with_css("font-size: 15px; color: system:secondary-text;"));
    row.add_child(
        Dom::create_span_with_text(label("azreview-notes"))
            .with_css("font-size: 11px; color: system:secondary-text; flex-grow: 1;"),
    );
    // The column's own way out: the top bar no longer has a key for it.
    let close = label("azreview-notes-switch");
    row.add_child(
        ui::with_tip(ui::key(ui::QUIET, close.clone(), data.clone(), voice::on_toggle_notes), &close)
            .with_id(ids::NOTES_TOGGLE)
            .with_child(Dom::create_icon("close").with_css("font-size: 14px;")),
    );
    if s.voice.running.is_some() {
        // Going (not asked to stop yet) is what the stop button is for; "listening" is the microphone taking words.
        let going = s.voice.is_listening();
        let ready = s.voice.is_ready();
        row.add_child(
            Dom::create_div()
                .with_id(ids::LISTENING)
                .with_css(
                    "display: flex; flex-direction: row; align-items: center; gap: 4px; font-size: \
                     11px; color: #d62d20; flex-shrink: 0;",
                )
                .with_child(Dom::create_icon(if ready { "mic" } else { "hourglass_top" }).with_css("font-size: 14px;"))
                .with_child(Dom::create_span_with_text(label(if ready {
                    "azreview-listening"
                } else if going {
                    "azreview-dictation-opening"
                } else {
                    "azreview-finishing"
                }))),
        );
        if going {
            row.add_child(
                Button::create(label("azreview-dictation-stop"))
                    .with_icon("stop")
                    .with_on_click(data.clone(), voice::on_stop as ButtonOnClickCallbackType)
                    .dom()
                    .with_id(ids::DICTATION_STOP)
                    .with_accessibility_name(label("azreview-dictation-stop")),
            );
        }
    }
    row
}

/// What the empty column says: how to start, or that the pen's rest is off.
fn hint(s: &AppState) -> Dom {
    let cfg = s.voice.dwell.config();
    let words = if cfg.allow == Allow::Off {
        t("azreview-notes-off")
    } else {
        let seconds = format!("{:.1}", cfg.rest_ms as f64 / 1_000.0);
        t_args(
            if cfg.allow == Allow::Pen {
                "azreview-notes-hint-pen"
            } else {
                "azreview-notes-hint-any"
            },
            &[("seconds", Arg::from(seconds))],
        )
    };
    Dom::create_p_with_text(words.as_str())
        .with_id(ids::NOTES_HINT)
        .with_css("margin: 0px; padding: 10px 4px; font-size: 12px; color: system:secondary-text;")
}

/// One note's card.
fn card(s: &AppState, data: &RefAny, number: usize, note: &Note) -> Dom {
    let running = s.voice.running_note() == Some(note.id);
    let listening = s.voice.ready_note() == Some(note.id);
    let tag = RefAny::new(NoteTag {
        app: data.clone(),
        id: note.id,
    });
    let mut head = Dom::create_div().with_css("display: flex; flex-direction: row; align-items: center; gap: 8px;");
    head.add_child(badge(number, listening, 18.0));
    head.add_child(
        Dom::create_span_with_text(place(s.file(), note).as_str())
            .with_css("font-size: 11px; color: system:secondary-text; flex-grow: 1;"),
    );
    head.add_child(
        Button::create("")
            .with_icon("delete")
            .with_on_click(tag.clone(), voice::on_note_delete as ButtonOnClickCallbackType)
            .dom()
            .with_id(format!("__azreview_note-delete-{}", note.id).as_str())
            .with_accessibility_name(t_args("azreview-note-delete", &[("n", Arg::from(number))])),
    );
    let body = if running {
        // While the words come in they are shown, not edited: the dictation owns the text.
        let words = if note.text.is_empty() {
            t("azreview-note-waiting")
        } else {
            note.text.clone()
        };
        let mut body = Dom::create_div()
            .with_css("display: flex; flex-direction: column; gap: 2px;")
            .with_child(Dom::create_p_with_text(words.as_str()).with_css(
                "margin: 0px; font-size: 13px; line-height: 1.35; white-space: pre-wrap; \
                 overflow-wrap: anywhere;",
            ));
        // The words of the utterance being spoken, before the recogniser has made them the note's.
        if !s.voice.partial().is_empty() {
            body.add_child(
                Dom::create_p_with_text(s.voice.partial())
                    .with_id(format!("__azreview_note-partial-{}", note.id).as_str())
                    .with_css(
                        "margin: 0px; font-size: 13px; line-height: 1.35; white-space: pre-wrap; \
                         overflow-wrap: anywhere; color: system:secondary-text; font-style: italic;",
                    ),
            );
        }
        body
    } else {
        TextArea::create()
            .with_text(note.text.as_str())
            .with_placeholder(label("azreview-note-placeholder"))
            .with_accessibility_name(t_args("azreview-note-name", &[("n", Arg::from(number))]))
            .with_on_text_input(tag.clone(), voice::on_note_text as TextAreaOnTextInputCallbackType)
            .with_on_focus_lost(tag, voice::on_note_blur as TextAreaOnFocusLostCallbackType)
            .dom()
            .with_id(format!("__azreview_note-text-{}", note.id).as_str())
            .with_css("min-height: 84px;")
    };
    Dom::create_div()
        .with_id(format!("__azreview_note-{}", note.id).as_str())
        .with_css(
            "display: flex; flex-direction: column; gap: 6px; padding: 8px; flex-shrink: 0; \
             border: 1px solid system:separator; border-radius: 6px; background: \
             system:control-background;",
        )
        .with_child(head)
        .with_child(body)
}

/// The side column of the window.
#[must_use]
pub fn column(s: &AppState, data: &RefAny) -> Dom {
    let mut list = Dom::create_div().with_css(
        "display: flex; flex-direction: column; flex-grow: 1; min-height: 0px; overflow-y: auto; \
         overflow-x: hidden; padding: 8px; gap: 8px;",
    );
    if s.notes.is_empty() {
        list.add_child(hint(s));
    } else {
        for (number, note) in numbered(s) {
            list.add_child(card(s, data, number, note));
        }
    }
    Dom::create_div()
        .with_id(ids::NOTES)
        .with_css(
            format!(
                "display: flex; flex-direction: column; width: {}px; flex-shrink: 0; min-height: \
                 0px; box-sizing: border-box; border-left: 1px solid system:separator; \
                 background: system:window-background;",
                COLUMN_W as isize
            )
            .as_str(),
        )
        .with_accessibility_name(label("azreview-notes"))
        .with_child(header(s, data))
        .with_child(list)
}

// ==== The settings page's dictation section ====

/// The preset nearest to `value`, as an index of `presets`.
#[must_use]
pub fn nearest(presets: &[u64], value: u64) -> usize {
    presets
        .iter()
        .enumerate()
        .min_by_key(|(_, p)| p.abs_diff(value))
        .map_or(0, |(i, _)| i)
}

/// The labels of the rest times.
static REST_LABELS: [&str; 4] = [
    "azreview-rest-0-8",
    "azreview-rest-1-2",
    "azreview-rest-2-0",
    "azreview-rest-3-0",
];
/// The labels of the silence times.
static SILENCE_LABELS: [&str; 4] = [
    "azreview-quiet-5",
    "azreview-quiet-10",
    "azreview-quiet-20",
    "azreview-quiet-40",
];
/// The labels of the idle times.
static IDLE_LABELS: [&str; 4] = [
    "azreview-idle-15",
    "azreview-idle-30",
    "azreview-idle-120",
    "azreview-idle-600",
];
/// The labels of the standby choices, in the order of [`Standby::ALL`].
static STANDBY_LABELS: [&str; 3] = [
    "azreview-standby-off",
    "azreview-standby-near",
    "azreview-standby-always",
];
/// The labels of the pointers that may rest into a dictation, in the order of [`Allow::ALL`].
static INPUT_LABELS: [&str; 3] = [
    "azreview-dictation-off",
    "azreview-dictation-pen",
    "azreview-dictation-any",
];

/// The labels of the microphone's states, in the order of [`Mic::ALL`].
static MIC_LABELS: [&str; 4] = [
    "azreview-mic-cold",
    "azreview-mic-warm",
    "azreview-mic-armed",
    "azreview-mic-listening",
];

/// The settings page's Dictation section: which pointers start a dictation, how long the pen rests,
/// how long the quiet lasts.
#[must_use]
pub fn settings_section(s: &AppState, app: &RefAny) -> kit::AppSection {
    let cfg = s.voice.dwell.config();
    let mut content = Dom::create_div()
        .with_css("display: flex; flex-direction: column;")
        .with_child(kit::row(
            "azreview-dictation-when",
            Segmented::create(labels(&INPUT_LABELS))
                .with_selected_index(Allow::ALL.iter().position(|a| *a == cfg.allow).unwrap_or(1))
                .with_on_change(app.clone(), on_input as SegmentedOnChangeCallbackType)
                .dom()
                .with_id(ids::SETTING_INPUT),
        ))
        .with_child(kit::row(
            "azreview-dictation-rest",
            Segmented::create(labels(&REST_LABELS))
                .with_selected_index(nearest(&REST_PRESETS, cfg.rest_ms))
                .with_on_change(app.clone(), on_rest as SegmentedOnChangeCallbackType)
                .dom()
                .with_id(ids::SETTING_REST),
        ))
        .with_child(kit::row(
            "azreview-dictation-quiet",
            Segmented::create(labels(&SILENCE_LABELS))
                .with_selected_index(nearest(&SILENCE_PRESETS, s.voice.silence_ms))
                .with_on_change(app.clone(), on_quiet as SegmentedOnChangeCallbackType)
                .dom()
                .with_id(ids::SETTING_QUIET),
        ))
        .with_child(kit::note("azreview-dictation-note"));
    // Offered where the session can open the microphone ahead of a dictation: a choice that does nothing is not one.
    if s.voice.can_arm {
        let standing = s.voice.arming.config();
        content.add_child(kit::row(
            "azreview-standby-when",
            Segmented::create(labels(&STANDBY_LABELS))
                .with_selected_index(Standby::ALL.iter().position(|b| *b == standing.standby).unwrap_or(1))
                .with_on_change(app.clone(), on_standby as SegmentedOnChangeCallbackType)
                .dom()
                .with_id(ids::SETTING_STANDBY),
        ));
        content.add_child(kit::row(
            "azreview-standby-idle",
            Segmented::create(labels(&IDLE_LABELS))
                .with_selected_index(nearest(&IDLE_PRESETS, standing.idle_ms))
                .with_on_change(app.clone(), on_standby_idle as SegmentedOnChangeCallbackType)
                .dom()
                .with_id(ids::SETTING_STANDBY_IDLE),
        ));
        content.add_child(kit::note("azreview-standby-note"));
    }
    content.add_child(mic_line(s));
    content.add_child(speech_status(s));
    kit::AppSection {
        category: 0,
        title: t("azreview-category-dictation"),
        content,
    }
}

/// What is known of the microphone, in words.
fn mic_line(s: &AppState) -> Dom {
    let at = Mic::ALL.iter().position(|m| *m == s.voice.mic).unwrap_or(0);
    let state = t(MIC_LABELS[at]);
    Dom::create_span_with_text(t_args("azreview-mic-line", &[("state", Arg::from(state))]).as_str())
        .with_id(ids::MIC_STATUS)
        .with_css("padding: 8px 0px 0px 0px; font-size: 11px; color: system:secondary-text;")
}

/// The recognisers the dictation asks, in the order it asks them, each with its state in the speech session's own
/// words (which one answers, which is not set up, which was refused and why).
fn speech_status(s: &AppState) -> Dom {
    let mut col = Dom::create_div()
        .with_id(ids::SPEECH_STATUS)
        .with_css("display: flex; flex-direction: column; gap: 2px; padding: 10px 0px; font-size: 11px; color: system:secondary-text;");
    col.add_child(Dom::create_span_with_text(label("azreview-speech-chain")).with_css("font-weight: 600;"));
    if let Some(speech) = &s.voice.speech {
        let lines = speech.recognizers();
        for i in 0..lines.len() {
            if let azul::option::OptionString::Some(line) = lines.c_get(i) {
                col.add_child(Dom::create_span_with_text(line.as_str()));
            }
        }
    } else if s.voice.gate.state() == SpeechState::Making {
        col.add_child(Dom::create_span_with_text(t("azreview-speech-making").as_str()));
    }
    if let Some(why) = s.voice.gate.failure() {
        col.add_child(Dom::create_span_with_text(
            t_args("azreview-speech-failed", &[("why", Arg::from(why.to_string()))]).as_str(),
        ));
    }
    col
}

/// Sets one value of the dictation: the window's copy, then the settings file.
fn change(
    data: &mut RefAny,
    info: &mut CallbackInfo,
    key: &str,
    value: String,
    set: impl FnOnce(&mut AppState),
) -> Update {
    let kit_ref = {
        let Some(mut s) = data.downcast_mut::<AppState>() else {
            return Update::DoNothing;
        };
        set(&mut s);
        s.kit.clone()
    };
    kit::set_value(&kit_ref, info, key, &value);
    Update::RefreshDom
}

extern "C" fn on_input(mut data: RefAny, mut info: CallbackInfo, state: SegmentedState) -> Update {
    let Some(&allow) = Allow::ALL.get(state.selected_index) else {
        return Update::DoNothing;
    };
    change(&mut data, &mut info, KEY_INPUT, allow.name().to_string(), |s| {
        let cfg = s.voice.dwell.config();
        s.voice.dwell.set_config(dwell::Config { allow, ..cfg });
    })
}

extern "C" fn on_rest(mut data: RefAny, mut info: CallbackInfo, state: SegmentedState) -> Update {
    let Some(&ms) = REST_PRESETS.get(state.selected_index) else {
        return Update::DoNothing;
    };
    change(&mut data, &mut info, KEY_REST, ms.to_string(), |s| {
        let cfg = s.voice.dwell.config();
        s.voice.dwell.set_config(dwell::Config { rest_ms: ms, ..cfg });
    })
}

extern "C" fn on_quiet(mut data: RefAny, mut info: CallbackInfo, state: SegmentedState) -> Update {
    let Some(&ms) = SILENCE_PRESETS.get(state.selected_index) else {
        return Update::DoNothing;
    };
    change(&mut data, &mut info, KEY_SILENCE, ms.to_string(), |s| {
        s.voice.silence_ms = ms;
    })
}

extern "C" fn on_standby(mut data: RefAny, mut info: CallbackInfo, state: SegmentedState) -> Update {
    let Some(&choice) = Standby::ALL.get(state.selected_index) else {
        return Update::DoNothing;
    };
    let update = change(&mut data, &mut info, KEY_STANDBY, choice.name().to_string(), |s| {
        let cfg = s.voice.arming.config();
        s.voice.set_standby(standby::Config { standby: choice, ..cfg });
    });
    voice::ensure_standby_timer(&mut data, &mut info);
    update
}

extern "C" fn on_standby_idle(mut data: RefAny, mut info: CallbackInfo, state: SegmentedState) -> Update {
    let Some(&ms) = IDLE_PRESETS.get(state.selected_index) else {
        return Update::DoNothing;
    };
    change(&mut data, &mut info, KEY_STANDBY_IDLE, ms.to_string(), |s| {
        let cfg = s.voice.arming.config();
        s.voice.set_standby(standby::Config { idle_ms: ms, ..cfg });
    })
}

/// After Cancel put the settings back: the window's copies read them again (the switches of this
/// run still win, as at the start).
pub fn reload_settings(app: &mut RefAny, _info: &mut CallbackInfo, settings: &AppSettings) {
    let Some(mut s) = app.downcast_mut::<AppState>() else {
        return;
    };
    let resolved = switches::resolve(&settings.values, &s.switches);
    let cfg = s.voice.dwell.config();
    s.voice.dwell.set_config(dwell::Config {
        allow: resolved.allow,
        rest_ms: resolved.rest_ms,
        ..cfg
    });
    s.voice.silence_ms = resolved.silence_ms;
    s.voice.set_standby(standby::Config {
        standby: resolved.standby,
        idle_ms: resolved.standby_idle_ms,
    });
}

#[cfg(test)]
#[path = "column_test.rs"]
mod column_test;
