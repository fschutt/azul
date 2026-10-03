//! The "Building blocks" section: the pieces the 2026-10-02 wave added for
//! the apps - the rich text editor on its document model, list selection
//! (click / Ctrl / Shift), the close guard ("save changes?"), the status
//! bar's zoom with its own range, a toggled and a disabled button, the date
//! picker's first day of the week.
//!
//! Every value the cards show is the APP's (`BlocksDemo`): each widget is
//! built from it and reports back into it (`crate::keep`).

use azul::{prelude::*, str::String as AzString, widgets::*};

use crate::{captioned, section, strs, Showcase};

/// The status line under a card.
const NOTE_CSS: &str = "font-size: 12px; color: system:secondary-text; margin: 6px 0px 0px 0px;";
/// A row of controls.
const ROW_CSS: &str = "display: flex; flex-direction: row; align-items: center; gap: 8px;";
/// The list box of the selection demo.
const LIST_CSS: &str = "display: flex; flex-direction: column; width: 260px; border: 1px solid \
                        system:separator; background-color: system:control-background;";
const ROW_ITEM_CSS: &str = "padding: 5px 10px; font-size: 13px; color: system:text; cursor: pointer;";
const ROW_SELECTED_CSS: &str = "padding: 5px 10px; font-size: 13px; cursor: pointer; \
                                background-color: system:accent; color: system:accent-text;";

/// The mailboxes of the selection demo, in list order.
const MAILBOXES: [&str; 6] = ["Inbox", "Drafts", "Sent", "Archive", "Spam", "Trash"];

/// The status bar's zoom range and step.
const ZOOM_MIN: f32 = 25.0;
const ZOOM_MAX: f32 = 400.0;
const ZOOM_STEP: f32 = 10.0;

/// Every value the building-block cards show.
#[derive(Clone)]
pub(crate) struct BlocksDemo {
    editor: RichTextEditorState,
    selection: ListSelection,
    dirty: bool,
    asking: bool,
    guard_status: AzString,
    zoom: f32,
    bold: bool,
    week_starts_monday: bool,
}

impl BlocksDemo {
    pub(crate) fn create() -> Self {
        let doc = RichTextDoc::create_from_markdown(
            "# Meeting notes\n\nThe **rich text editor** keeps a document model with *one* undo \
             history.\n\n- Markdown, HTML and plain text in and out\n- [ ] a checklist item\n\n> \
             A quote, too.",
        );
        Self {
            editor: RichTextEditorState::create(doc),
            selection: ListSelection::create(),
            dirty: false,
            asking: false,
            guard_status: "Tick the box, then close the document.".into(),
            zoom: 100.0,
            bold: true,
            week_starts_monday: true,
        }
    }
}

fn keep(data: &mut RefAny, put: impl FnOnce(&mut BlocksDemo)) -> Update {
    crate::keep(data, |s| put(&mut s.blocks))
}

fn note(text: &str) -> Dom {
    Dom::create_p_with_text(text).with_css(NOTE_CSS)
}

/// The key of every mailbox, in list order (the order Shift extends in).
fn order() -> Vec<u64> {
    MAILBOXES.iter().map(|name| ListSelection::key_of(*name)).collect()
}

/// One mailbox row's report: which row, and the app.
struct RowRef {
    app: RefAny,
    key: u64,
}

/// The "Building blocks" section.
pub(crate) fn blocks_section(data: &RefAny, b: &BlocksDemo, theme: UiTheme) -> Dom {
    let editor = RichTextEditor::create(b.editor.clone())
        .with_accessibility_name("Meeting notes")
        .with_toolbar(RichTextToolbar::create_full())
        .with_on_change(data.clone(), on_editor)
        .with_theme(theme)
        .dom()
        .with_css("height: 260px;");

    let mut list = Dom::create_div()
        .with_css(LIST_CSS)
        .with_accessibility_name("Mailboxes");
    for name in MAILBOXES {
        let key = ListSelection::key_of(name);
        let css = if b.selection.contains(key) { ROW_SELECTED_CSS } else { ROW_ITEM_CSS };
        list.add_child(
            Dom::create_div()
                .with_css(css)
                .with_child(Dom::create_span_with_text(name))
                .with_callback(
                    EventFilter::Hover(HoverEventFilter::MouseUp),
                    RefAny::new(RowRef { app: data.clone(), key }),
                    on_row,
                ),
        );
    }
    let picked = MAILBOXES
        .iter()
        .filter(|name| b.selection.contains(ListSelection::key_of(**name)))
        .copied()
        .collect::<Vec<_>>()
        .join(", ");
    let selection = Dom::create_div()
        .with_child(list)
        .with_child(note(&format!(
            "{} selected{}{}",
            b.selection.len(),
            if picked.is_empty() { "" } else { ": " },
            picked
        )));

    let document = Dom::create_div()
        .with_css("display: flex; flex-direction: column; gap: 8px;")
        .with_child(
            CheckBox::create(b.dirty)
                .with_accessibility_name("The document has unsaved changes")
                .with_on_toggle(data.clone(), on_dirty)
                .with_theme(theme)
                .dom(),
        )
        .with_child(
            Button::create("Close the document\u{2026}")
                .with_on_click(data.clone(), on_close_document)
                .with_theme(theme)
                .dom(),
        )
        .with_child(note(b.guard_status.as_str()));
    let guard = CloseGuard::create(document, "Meeting notes")
        .with_dirty(b.dirty)
        .with_asking(b.asking)
        .with_on_event(data.clone(), on_guard)
        .with_theme(theme)
        .dom();

    let status = StatusBar::create(vec![
        StatusBarSegment::create("Page 1 of 3"),
        StatusBarSegment::create("412 words"),
    ])
    .with_zoom(
        StatusBarZoom::create(b.zoom, ZOOM_MIN, ZOOM_MAX)
            .with_on_slider_change(data.clone(), on_zoom_slider)
            .with_on_zoom_in(data.clone(), on_zoom_in)
            .with_on_zoom_out(data.clone(), on_zoom_out),
    )
    .dom();

    let buttons = Dom::create_div()
        .with_css(ROW_CSS)
        .with_child(
            Button::create("Bold")
                .with_icon("format_bold")
                .with_toggled(b.bold)
                .with_on_click(data.clone(), on_bold)
                .with_theme(theme)
                .dom(),
        )
        .with_child(
            Button::create("Delete")
                .with_icon("delete")
                .with_disabled("Select a message first")
                .with_theme(theme)
                .dom(),
        );

    let week = Dom::create_div()
        .with_css("display: flex; flex-direction: column; gap: 8px;")
        .with_child(
            Segmented::create(strs(&["Sunday", "Monday"]))
                .with_selected_index(usize::from(b.week_starts_monday))
                .with_on_change(data.clone(), on_week_start)
                .with_theme(theme)
                .dom()
                .with_accessibility_name("First day of the week"),
        )
        .with_child(
            DatePicker::create(2026, 10, 3)
                .with_inline(true)
                .with_week_start(if b.week_starts_monday {
                    DatePickerWeekStart::Monday
                } else {
                    DatePickerWeekStart::Sunday
                })
                .with_accessibility_name("Calendar")
                .with_theme(theme)
                .dom(),
        );

    section(
        "Building blocks",
        vec![
            captioned("RichTextEditor (the document model, Markdown in)", editor),
            captioned("ListSelection (click, Ctrl / Cmd + click, Shift + click)", selection),
            captioned("CloseGuard (\u{201C}save changes?\u{201D})", guard),
            captioned(
                &format!("StatusBarZoom ({ZOOM_MIN:.0} % to {ZOOM_MAX:.0} %)"),
                status,
            ),
            captioned("Button: toggled / disabled (with its reason)", buttons),
            captioned("DatePicker: the first day of the week", week),
        ],
    )
}

// ==== Callbacks ====

/// The editor reports its state on every edit: kept for the next rebuild,
/// no rebuild of its own (the engine shows the edit already).
extern "C" fn on_editor(mut data: RefAny, _: CallbackInfo, state: RichTextEditorState) -> Update {
    if let Some(mut s) = data.downcast_mut::<Showcase>() {
        s.blocks.editor = state;
    }
    Update::DoNothing
}

extern "C" fn on_row(mut data: RefAny, info: CallbackInfo) -> Update {
    let Some((mut app, key)) = data.downcast_ref::<RowRef>().map(|r| (r.app.clone(), r.key)) else {
        return Update::DoNothing;
    };
    let m = info.get_key_modifiers();
    let (shift, extend) = (m.shift, m.primary_down());
    keep(&mut app, |b| b.selection.select_in(order(), key, shift, extend))
}

extern "C" fn on_dirty(mut data: RefAny, _: CallbackInfo, state: CheckBoxState) -> Update {
    keep(&mut data, |b| b.dirty = state.checked)
}

extern "C" fn on_close_document(mut data: RefAny, _: CallbackInfo) -> Update {
    keep(&mut data, |b| {
        if b.dirty {
            b.asking = true;
        } else {
            b.guard_status = "Closed - nothing to save.".into();
        }
    })
}

extern "C" fn on_guard(mut data: RefAny, _: CallbackInfo, event: CloseGuardEvent) -> Update {
    keep(&mut data, |b| match event.kind {
        CloseGuardEventKind::Ask => b.asking = true,
        CloseGuardEventKind::Save => {
            b.asking = false;
            b.dirty = false;
            b.guard_status = "Saved, then closed.".into();
        }
        CloseGuardEventKind::Discard => {
            b.asking = false;
            b.dirty = false;
            b.guard_status = "Closed without saving.".into();
        }
        CloseGuardEventKind::Cancel => {
            b.asking = false;
            b.guard_status = "Kept open.".into();
        }
    })
}

extern "C" fn on_zoom_slider(mut data: RefAny, _: CallbackInfo, state: SliderState) -> Update {
    keep(&mut data, |b| b.zoom = state.value.clamp(ZOOM_MIN, ZOOM_MAX))
}

extern "C" fn on_zoom_in(mut data: RefAny, _: CallbackInfo) -> Update {
    keep(&mut data, |b| b.zoom = (b.zoom + ZOOM_STEP).min(ZOOM_MAX))
}

extern "C" fn on_zoom_out(mut data: RefAny, _: CallbackInfo) -> Update {
    keep(&mut data, |b| b.zoom = (b.zoom - ZOOM_STEP).max(ZOOM_MIN))
}

extern "C" fn on_bold(mut data: RefAny, _: CallbackInfo) -> Update {
    keep(&mut data, |b| b.bold = !b.bold)
}

extern "C" fn on_week_start(mut data: RefAny, _: CallbackInfo, state: SegmentedState) -> Update {
    keep(&mut data, |b| b.week_starts_monday = state.selected_index == 1)
}
