//! The conversations of a page, where the reviewer reads: in the margin of the page a chip for each, at the height of the
//! line it was anchored to, and the one that is open as a popover with its words, what the agent answered and the
//! reviewer's reply. There is no column of notes beside the pages any more.
//!
//! A chip is the conversation closed: its mark (the macOS spinner while the agent works, yellow when a suggestion
//! waits, green when done, a microphone while it listens), its number and its first words. A click opens it, and a click
//! on its close key (or on the chip again) closes it. While a dictation runs for a note its popover is open and shows the
//! last three lines being said. The pure part - where the chips stand - is `margin.rs`.

use azul::{
    callbacks::{ButtonOnClickCallbackType, TextAreaOnTextInputCallbackType},
    dom::{AccessibilityInfo, AccessibilityRole},
    prelude::*,
    str::String as AzString,
    widgets::{Button, OnTextInputReturn, TextArea, TextAreaState, TextInputValid},
};
use azul_appkit::l10n::{label, t, t_args, Arg};

use crate::{
    agent_window,
    chips::{self, State},
    column::NoteTag,
    conversation::{self, last_lines, Phase, Who},
    ids,
    margin::{self, Slot},
    model::Note,
    ui, voice, AppState,
};

/// How tall a chip is, px.
pub const CHIP_H: f32 = 28.0;
/// The space between two chips, px.
pub const CHIP_GAP: f32 = 6.0;
/// Where the margin starts on the page (from its left edge), px.
pub const MARGIN_X: f32 = ui::PAGE_W - ui::MARGIN_GUTTER_W + 8.0;
/// How wide a chip is, px.
pub const MARGIN_W: f32 = ui::MARGIN_GUTTER_W - 8.0 - ui::PAGE_PAD / 2.0;
/// How wide the open conversation is, px: the margin and a little of the page.
pub const POP_W: f32 = 304.0;
/// How many of the newest turns the open conversation shows (the older ones are counted, not built).
pub const SHOW_TURNS: usize = 6;
/// How many letters of one turn it shows.
pub const SHOW_TURN_CHARS: usize = 1_000;
/// How many letters of the open conversation's text fit in a line (12 px text in 268 px, a little under: a guess).
pub const LINE_CHARS: usize = 38;
/// How many lines of what is being said are shown.
pub const LIVE_LINES: usize = 3;

/// Which conversation a chip is.
#[derive(Debug, Clone, Copy)]
pub struct ConvTag {
    pub id: u64,
}

fn tag_of(info: &mut CallbackInfo) -> Option<u64> {
    info.get_dataset(info.get_hit_node())
        .into_option()
        .and_then(|mut d| d.downcast_ref::<ConvTag>().map(|t| t.id))
}

/// How tall the margin is: the page, less its padding above and below.
fn margin_h() -> f32 {
    ui::page_h() - 2.0 * ui::PAGE_PAD
}

/// What a chip says of its conversation for the screen reader and the tooltip: its number, its words and its state.
fn chip_name(number: usize, note: &Note) -> AzString {
    let words = chips::short_title(&note.text, 40);
    let state = ui::state_word(note.talk.chip_state());
    AzString::from(
        t_args(
            "azreview-conv-chip-name",
            &[("n", Arg::from(number)), ("words", Arg::from(words)), ("state", Arg::from(state.as_str().to_string()))],
        )
        .as_str(),
    )
}

/// The conversations of `page` with their numbers and where their chips stand.
fn placed<'a>(s: &'a AppState, page: usize) -> (Vec<(usize, &'a Note)>, margin::Placement) {
    let here: Vec<(usize, &Note)> = s.notes.iter().enumerate().filter(|(_, n)| n.page == page).map(|(i, n)| (i + 1, n)).collect();
    let slots: Vec<Slot> = here
        .iter()
        .map(|(_, n)| Slot {
            id: n.id,
            want_y: n.y - ui::PAGE_PAD - CHIP_H / 2.0,
            keep: keep_of(s, n),
        })
        .collect();
    let placement = margin::place(&slots, margin_h(), CHIP_H, CHIP_GAP);
    (here, placement)
}

/// How much it matters that a conversation stays in sight when the margin is full.
fn keep_of(s: &AppState, note: &Note) -> u8 {
    if s.open_conv == Some(note.id) || s.voice.running_note() == Some(note.id) {
        return 3;
    }
    match note.talk.chip_state() {
        State::Working => 2,
        State::Suggested => 1,
        _ => 0,
    }
}

/// A callback that takes the pen's pointer events away from the page's ink beneath: a click on a chip or on the open
/// conversation is no stroke and no rest of the pen.
extern "C" fn block_ink(_data: RefAny, mut info: CallbackInfo) -> Update {
    info.stop_propagation();
    Update::DoNothing
}

fn blocking(dom: Dom) -> Dom {
    dom.with_callback(EventFilter::Hover(HoverEventFilter::MouseDown), RefAny::new(()), block_ink)
        .with_callback(EventFilter::Hover(HoverEventFilter::MouseMove), RefAny::new(()), block_ink)
}

/// The mark of a conversation: a microphone while it listens, else the mark of its state.
fn mark(listening: bool, state: State, size: isize) -> Dom {
    if listening {
        Dom::create_icon("mic").with_css(format!("font-size: {}px; color: #d62d20; flex-shrink: 0;", size).as_str())
    } else {
        ui::state_mark(state, size)
    }
}

/// One conversation's chip at `top`.
fn chip(s: &AppState, data: &RefAny, number: usize, note: &Note, top: f32) -> Dom {
    let open = s.open_conv == Some(note.id);
    let listening = s.voice.running_note() == Some(note.id);
    let name = chip_name(number, note);
    let face = format!(
        "position: absolute; left: 0px; top: {top:.0}px; width: {MARGIN_W:.0}px; height: {CHIP_H:.0}px; justify-content: \
         flex-start; gap: 6px; padding: 0px 8px; background: system:control-background; border: 1px solid {}; color: \
         system:text; box-shadow: 0px 1px 3px rgba(0,0,0,0.12);",
        if open { "system:accent" } else { "system:separator" }
    );
    let words = chips::short_title(&note.text, 26);
    let words = if words.is_empty() { t("azreview-conv-empty") } else { words };
    ui::with_tip(ui::key(&face, name.clone(), data.clone(), on_conv_toggle), &name)
        .with_id(format!("__azreview_conv-{}", note.id).as_str())
        .with_dataset(OptionRefAny::Some(RefAny::new(ConvTag { id: note.id })))
        .with_child(mark(listening, note.talk.chip_state(), 14))
        .with_child(Dom::create_span_with_text(format!("{number}").as_str()).with_css("font-weight: 600; flex-shrink: 0;"))
        .with_child(
            Dom::create_span_with_text(words.as_str())
                .with_css("overflow: hidden; white-space: nowrap; text-overflow: ellipsis; font-size: 11px;"),
        )
}

/// The margin of `page`: its conversations as chips, and how many more there are than the margin holds.
#[must_use]
pub fn margin(s: &AppState, data: &RefAny, page: usize) -> Option<Dom> {
    let (here, placement) = placed(s, page);
    if here.is_empty() {
        return None;
    }
    let mut column = Dom::create_div().with_id(format!("__azreview_margin-{}", page + 1).as_str()).with_css(
        format!(
            "position: absolute; left: {MARGIN_X:.0}px; top: {:.0}px; width: {MARGIN_W:.0}px; height: {:.0}px;",
            ui::PAGE_PAD,
            margin_h()
        )
        .as_str(),
    );
    for (id, top) in &placement.tops {
        if let Some((number, note)) = here.iter().find(|(_, n)| n.id == *id) {
            column.add_child(chip(s, data, *number, note, *top));
        }
    }
    if !placement.hidden.is_empty() {
        let words = t_args("azreview-conv-more", &[("n", Arg::from(placement.hidden.len()))]);
        column.add_child(
            Dom::create_span_with_text(words.as_str())
                .with_id(format!("__azreview_margin-more-{}", page + 1).as_str())
                .with_css(
                    format!(
                        "position: absolute; left: 0px; top: {:.0}px; font-size: 11px; color: system:secondary-text;",
                        margin_h() - 14.0
                    )
                    .as_str(),
                ),
        );
    }
    Some(blocking(column))
}

// ==== The open conversation ====

/// The words a turn is shown under.
fn who_word(who: Who) -> AzString {
    label(match who {
        Who::Reviewer => "azreview-conv-you",
        Who::Agent => "azreview-conv-agent",
    })
}

/// The listening pill and the stop key while a dictation runs for this note.
fn listening_row(s: &AppState, data: &RefAny) -> Dom {
    let going = s.voice.is_listening();
    let ready = s.voice.is_ready();
    let mut row = Dom::create_div()
        .with_id(ids::LISTENING)
        .with_css("display: flex; flex-direction: row; align-items: center; gap: 6px; font-size: 11px; color: #d62d20;")
        .with_child(Dom::create_icon(if ready { "mic" } else { "hourglass_top" }).with_css("font-size: 14px;"))
        .with_child(Dom::create_span_with_text(label(if ready {
            "azreview-listening"
        } else if going {
            "azreview-dictation-opening"
        } else {
            "azreview-finishing"
        })));
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
    row
}

/// The last lines of what is being said, the one being spoken in italics.
fn live_lines(s: &AppState, note: &Note) -> Dom {
    let partial = s.voice.partial().trim().to_string();
    let lines = last_lines(&note.talk.said, &partial, LIVE_LINES);
    let mut block = Dom::create_div()
        .with_id(format!("__azreview_conv-live-{}", note.id).as_str())
        .with_css("display: flex; flex-direction: column; gap: 2px;");
    if lines.is_empty() {
        block.add_child(Dom::create_p_with_text(t("azreview-note-waiting").as_str()).with_css(
            "margin: 0px; font-size: 13px; line-height: 1.35; color: system:secondary-text;",
        ));
    }
    let spoken = !partial.is_empty();
    let count = lines.len();
    for (k, line) in lines.iter().enumerate() {
        let now = spoken && k + 1 == count;
        block.add_child(
            Dom::create_p_with_text(line.as_str())
                .with_id(if now {
                    format!("__azreview_note-partial-{}", note.id)
                } else {
                    format!("__azreview_conv-live-{}-{k}", note.id)
                }
                .as_str())
                .with_css(
                    format!(
                        "margin: 0px; font-size: 13px; line-height: 1.35; white-space: pre-wrap; overflow-wrap: anywhere;{}",
                        if now { " color: system:secondary-text; font-style: italic;" } else { "" }
                    )
                    .as_str(),
                ),
        );
    }
    block
}

/// What the agent is doing, or did not manage.
fn status_row(note: &Note) -> Option<Dom> {
    match note.talk.phase {
        Phase::Working => Some(
            Dom::create_div()
                .with_id(format!("__azreview_conv-working-{}", note.id).as_str())
                .with_css("display: flex; flex-direction: row; align-items: center; gap: 8px; font-size: 12px; color: system:secondary-text;")
                .with_child(ui::state_mark(State::Working, 16))
                .with_child(Dom::create_span_with_text(t("azreview-conv-working").as_str())),
        ),
        Phase::Failed => {
            let why = note.talk.problem.clone().unwrap_or_default();
            Some(
                Dom::create_p_with_text(why.as_str())
                    .with_id(format!("__azreview_conv-problem-{}", note.id).as_str())
                    .with_css("margin: 0px; font-size: 12px; line-height: 1.35; color: #d62d20; overflow-wrap: anywhere;"),
            )
        }
        Phase::Suggested => Some(
            Dom::create_p_with_text(
                t_args("azreview-conv-suggestions", &[("n", Arg::from(note.talk.pending))]).as_str(),
            )
            .with_id(format!("__azreview_conv-suggestions-{}", note.id).as_str())
            .with_css("margin: 0px; font-size: 12px; color: #b07f00;"),
        ),
        Phase::Done => Some(
            Dom::create_div()
                .with_id(format!("__azreview_conv-done-{}", note.id).as_str())
                .with_css("display: flex; flex-direction: row; align-items: center; gap: 6px; font-size: 12px; color: #2fa84f;")
                .with_child(Dom::create_icon("check_circle").with_css("font-size: 14px;"))
                .with_child(Dom::create_span_with_text(t("azreview-conv-done").as_str())),
        ),
        Phase::Draft => None,
    }
}

/// An icon key of the open conversation: a button with the name for the screen reader.
fn icon_key(icon: &str, name: AzString, id: String, tag: &RefAny, on_click: ButtonOnClickCallbackType) -> Dom {
    Button::create("")
        .with_icon(icon)
        .with_on_click(tag.clone(), on_click)
        .dom()
        .with_id(id.as_str())
        .with_accessibility_name(name)
}

/// How many lines `chars` letters take.
#[must_use]
pub const fn lines_of(chars: usize) -> usize {
    let lines = chars.div_ceil(LINE_CHARS);
    if lines == 0 {
        1
    } else {
        lines
    }
}

/// `text` as at most `SHOW_TURN_CHARS` letters, with an ellipsis where it was cut (letters, not bytes).
#[must_use]
pub fn shown(text: &str) -> String {
    if text.chars().count() <= SHOW_TURN_CHARS {
        return text.to_string();
    }
    let mut cut: String = text.chars().take(SHOW_TURN_CHARS).collect();
    cut.push('…');
    cut
}

/// How tall the open conversation will be, px: a guess from what it holds (the header, the words or the lines being said,
/// what the agent understood, the newest turns, a status, the reply), good to a few lines. It places the popover so that
/// its foot is on the page; the layout makes the real height.
#[must_use]
pub fn popover_height(note: &Note, running: bool, partial_chars: usize) -> f32 {
    const PAD: f32 = 22.0;
    const HEAD: f32 = 30.0;
    const GAP: f32 = 8.0;
    const AREA: f32 = 70.0;
    const LINE: f32 = 17.0;
    let mut h = PAD + HEAD;
    if running {
        let said = note.talk.said.len().min(LIVE_LINES) + usize::from(partial_chars > 0);
        h += GAP + 24.0 + GAP + LINE * said.clamp(1, LIVE_LINES + 1) as f32;
    } else {
        h += GAP + AREA;
    }
    if note.talk.heard_as.as_deref().is_some_and(|x| !x.trim().is_empty() && x.trim() != note.text.trim()) {
        h += GAP + 15.0 * lines_of(note.talk.heard_as.as_deref().map_or(0, |x| x.chars().count() + 10)) as f32;
    }
    if let Some(understood) = note.talk.understood_text() {
        h += GAP + 15.0 * lines_of(understood.chars().count() + 14) as f32;
    }
    let from = note.talk.turns.len().saturating_sub(SHOW_TURNS);
    if from > 0 {
        h += GAP + 14.0;
    }
    for turn in &note.talk.turns[from..] {
        h += GAP + 14.0 + LINE * lines_of(turn.text.chars().count().min(SHOW_TURN_CHARS)) as f32 + 12.0;
    }
    if note.talk.phase != Phase::Draft {
        h += GAP + 20.0;
    }
    if !running {
        h += GAP + 44.0 + GAP + 28.0;
    }
    h
}

/// The open conversation, at the height of its chip; `visible_right` is how far across the page the window shows (px from
/// the page's left edge: the popover keeps inside it).
#[must_use]
pub fn popover(s: &AppState, data: &RefAny, page: usize, visible_right: f32) -> Option<Dom> {
    let id = s.open_conv?;
    let (here, placement) = placed(s, page);
    let (number, note) = here.iter().find(|(_, n)| n.id == id).copied()?;
    let chip_top = placement
        .tops
        .iter()
        .find(|(i, _)| *i == id)
        .map_or_else(|| (note.y - ui::PAGE_PAD - CHIP_H / 2.0).max(0.0), |(_, t)| *t);
    let running = s.voice.running_note() == Some(id);
    let top = ui::PAGE_PAD
        + margin::popover_top(chip_top, popover_height(note, running, s.voice.partial().chars().count()), margin_h());
    let tag = RefAny::new(NoteTag { app: data.clone(), id });

    let mut head = Dom::create_div().with_css("display: flex; flex-direction: row; align-items: center; gap: 8px;");
    head.add_child(mark(running, note.talk.chip_state(), 16));
    head.add_child(
        Dom::create_span_with_text(
            t_args("azreview-conv-title", &[("n", Arg::from(number)), ("where", Arg::from(crate::column::place(s.file(), note)))]).as_str(),
        )
        .with_css("font-size: 11px; color: system:secondary-text; flex-grow: 1;"),
    );
    head.add_child(icon_key(
        "delete",
        AzString::from(t_args("azreview-note-delete", &[("n", Arg::from(number))]).as_str()),
        format!("__azreview_note-delete-{id}"),
        &tag,
        voice::on_note_delete as ButtonOnClickCallbackType,
    ));
    head.add_child(icon_key(
        "close",
        label("azreview-conv-close"),
        format!("__azreview_conv-close-{id}"),
        &tag,
        on_conv_close as ButtonOnClickCallbackType,
    ));

    // The words and the thread, as tall as they are: a box that scrolls under a parent with no height of its own is laid out
    // with no height, so the thread is bounded by what is built (the newest turns, each cut) and not by a scroll.
    let mut body = Dom::create_div().with_css("display: flex; flex-direction: column; gap: 8px;");
    if running {
        body.add_child(listening_row(s, data));
        body.add_child(live_lines(s, note));
    } else {
        body.add_child(
            TextArea::create()
                .with_text(note.text.as_str())
                .with_placeholder(label("azreview-note-placeholder"))
                .with_accessibility_name(t_args("azreview-note-name", &[("n", Arg::from(number))]))
                .with_on_text_input(tag.clone(), voice::on_note_text as TextAreaOnTextInputCallbackType)
                .with_on_focus_lost(tag.clone(), voice::on_note_blur as azul::callbacks::TextAreaOnFocusLostCallbackType)
                .dom()
                .with_id(format!("__azreview_note-text-{id}").as_str())
                .with_css("min-height: 64px;"),
        );
    }
    if let Some(heard) = note.talk.heard_as.as_deref().filter(|h| !h.trim().is_empty() && h.trim() != note.text.trim()) {
        body.add_child(
            Dom::create_p_with_text(t_args("azreview-conv-heard-as", &[("words", Arg::from(heard.to_string()))]).as_str())
                .with_id(format!("__azreview_conv-heard-{id}").as_str())
                .with_css("margin: 0px; font-size: 11px; line-height: 1.35; color: system:secondary-text; overflow-wrap: anywhere;"),
        );
    }
    // What the names of the workspace make of the words: beside them, never in their place.
    if let Some(understood) = note.talk.understood_text() {
        body.add_child(
            Dom::create_p_with_text(t_args("azreview-conv-understood", &[("words", Arg::from(understood))]).as_str())
                .with_id(format!("__azreview_conv-understood-{id}").as_str())
                .with_css("margin: 0px; font-size: 11px; line-height: 1.35; color: system:secondary-text; overflow-wrap: anywhere;"),
        );
    }
    let from = note.talk.turns.len().saturating_sub(SHOW_TURNS);
    if from > 0 {
        body.add_child(
            Dom::create_span_with_text(t_args("azreview-conv-earlier", &[("n", Arg::from(from))]).as_str())
                .with_id(format!("__azreview_conv-earlier-{id}").as_str())
                .with_css("font-size: 10px; color: system:secondary-text;"),
        );
    }
    for (k, turn) in note.talk.turns[from..].iter().enumerate() {
        body.add_child(
            Dom::create_div()
                .with_id(format!("__azreview_conv-turn-{id}-{k}").as_str())
                .with_css(
                    format!(
                        "display: flex; flex-direction: column; gap: 2px; padding: 6px 8px; border-radius: 6px; background: {};",
                        if turn.who == Who::Agent { "system:window-background" } else { "transparent" }
                    )
                    .as_str(),
                )
                .with_child(
                    Dom::create_span_with_text(who_word(turn.who).as_str())
                        .with_css("font-size: 10px; font-weight: 600; color: system:secondary-text;"),
                )
                .with_child(
                    Dom::create_p_with_text(shown(&turn.text).as_str())
                        .with_css("margin: 0px; font-size: 12px; line-height: 1.4; white-space: pre-wrap; overflow-wrap: anywhere;"),
                ),
        );
    }
    if let Some(status) = status_row(note) {
        body.add_child(status);
    }

    // The reply: a text and the keys to send it, to close the conversation, or to open it again.
    let mut foot = Dom::create_div().with_css("display: flex; flex-direction: column; gap: 6px;");
    if !running {
        foot.add_child(
            TextArea::create()
                .with_text(note.talk.draft.as_str())
                .with_placeholder(label("azreview-conv-reply-placeholder"))
                .with_accessibility_name(label("azreview-conv-reply-name"))
                .with_on_text_input(tag.clone(), on_conv_reply_text as TextAreaOnTextInputCallbackType)
                .dom()
                .with_id(format!("__azreview_conv-reply-{id}").as_str())
                .with_css("min-height: 40px;"),
        );
        let mut keys = Dom::create_div().with_css("display: flex; flex-direction: row; align-items: center; gap: 6px; justify-content: flex-end;");
        if note.talk.phase == Phase::Done {
            keys.add_child(icon_key("undo", label("azreview-conv-reopen"), format!("__azreview_conv-reopen-{id}"), &tag, on_conv_reopen as ButtonOnClickCallbackType));
        } else {
            keys.add_child(icon_key("check", label("azreview-conv-resolve"), format!("__azreview_conv-resolve-{id}"), &tag, on_conv_resolve as ButtonOnClickCallbackType));
        }
        keys.add_child(icon_key(
            "send",
            label(if note.talk.phase == Phase::Failed { "azreview-conv-ask-again" } else { "azreview-conv-send" }),
            format!("__azreview_conv-send-{id}"),
            &tag,
            on_conv_send as ButtonOnClickCallbackType,
        ));
        foot.add_child(keys);
    }

    let name = chip_name(number, note);
    Some(blocking(
        Dom::create_div()
            .with_id(format!("__azreview_conv-pop-{id}").as_str())
            .with_css(
                format!(
                    "position: absolute; left: {:.0}px; top: {top:.0}px; width: {POP_W:.0}px; \
                     display: flex; flex-direction: column; gap: 8px; padding: 10px; box-sizing: border-box; \
                     background: system:control-background; border: 1px solid system:separator; border-radius: 8px; \
                     box-shadow: 0px 6px 22px rgba(0,0,0,0.28); z-index: 4;",
                    margin::popover_left(visible_right, POP_W, ui::PAGE_W)
                )
                .as_str(),
            )
            .with_accessibility_info(AccessibilityInfo::named(name, AccessibilityRole::Dialog))
            .with_child(head)
            .with_child(body)
            .with_child(foot),
    ))
}

// ==== What the keys do ====

/// A click on a conversation's chip: it opens, or closes when it was open. Only one is open at a time.
extern "C" fn on_conv_toggle(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(id) = tag_of(&mut info) else {
        return Update::DoNothing;
    };
    {
        let Some(mut s) = data.downcast_mut::<AppState>() else {
            return Update::DoNothing;
        };
        s.open_conv = if s.open_conv == Some(id) { None } else { Some(id) };
    }
    info.trigger_all_virtual_view_rerender();
    Update::RefreshDom
}

/// The open conversation's close key.
extern "C" fn on_conv_close(mut tag: RefAny, mut info: CallbackInfo) -> Update {
    let Some(mut app) = tag.downcast_ref::<NoteTag>().map(|t| t.app.clone()) else {
        return Update::DoNothing;
    };
    if let Some(mut s) = app.downcast_mut::<AppState>() {
        s.open_conv = None;
    }
    info.trigger_all_virtual_view_rerender();
    Update::RefreshDom
}

/// The reply being typed (each key): kept as the conversation's draft.
extern "C" fn on_conv_reply_text(mut tag: RefAny, _info: CallbackInfo, state: TextAreaState) -> OnTextInputReturn {
    let tag = tag.downcast_ref::<NoteTag>().map(|t| (t.app.clone(), t.id));
    if let Some((mut app, id)) = tag {
        let text = state.get_text().as_str().to_string();
        let mut guard = app.downcast_mut::<AppState>();
        if let Some(note) = guard.as_mut().and_then(|s| s.notes.iter_mut().find(|n| n.id == id)) {
            note.talk.draft = text;
        }
    }
    OnTextInputReturn { update: Update::DoNothing, valid: TextInputValid::Yes }
}

/// The send key: the reply typed (if any) is the reviewer's turn, and the agent is asked.
extern "C" fn on_conv_send(mut tag: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, id)) = tag.downcast_ref::<NoteTag>().map(|t| (t.app.clone(), t.id)) else {
        return Update::DoNothing;
    };
    let may = {
        let Some(mut s) = app.downcast_mut::<AppState>() else {
            return Update::DoNothing;
        };
        let now = agent_window::unix_now_ms();
        let Some(note) = s.notes.iter_mut().find(|n| n.id == id) else {
            return Update::DoNothing;
        };
        let draft = std::mem::take(&mut note.talk.draft);
        if !draft.trim().is_empty() {
            let _ = note.talk.say(&draft, now);
        }
        let may = note.talk.can_ask()
            && (note.talk.has_unanswered_follow_up() || matches!(note.talk.phase, Phase::Draft | Phase::Failed))
            && (!note.text.trim().is_empty() || note.talk.has_unanswered_follow_up());
        s.sync_chip(id);
        may
    };
    if may {
        return agent_window::ask(&mut app, &mut info, id);
    }
    info.trigger_all_virtual_view_rerender();
    Update::RefreshDom
}

/// The key that closes a conversation: what waits is no longer waited for.
extern "C" fn on_conv_resolve(mut tag: RefAny, mut info: CallbackInfo) -> Update {
    change(&mut tag, &mut info, conversation::Conversation::resolve)
}

/// The key that opens a finished conversation again.
extern "C" fn on_conv_reopen(mut tag: RefAny, mut info: CallbackInfo) -> Update {
    change(&mut tag, &mut info, conversation::Conversation::reopen)
}

/// Changes the conversation of the key's note, keeps the chip and the session.
fn change(tag: &mut RefAny, info: &mut CallbackInfo, apply: fn(&mut conversation::Conversation)) -> Update {
    let Some((mut app, id)) = tag.downcast_ref::<NoteTag>().map(|t| (t.app.clone(), t.id)) else {
        return Update::DoNothing;
    };
    {
        let Some(mut s) = app.downcast_mut::<AppState>() else {
            return Update::DoNothing;
        };
        if let Some(note) = s.notes.iter_mut().find(|n| n.id == id) {
            apply(&mut note.talk);
        }
        s.sync_chip(id);
    }
    crate::save_session(&app, info);
    info.trigger_all_virtual_view_rerender();
    Update::RefreshDom
}

#[cfg(test)]
#[path = "conv_ui_test.rs"]
mod conv_ui_test;
