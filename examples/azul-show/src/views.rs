//! The editor's views: the normal view (the slide rail, the slide canvas
//! under the selection adorner, the notes, the format pane), the slide
//! sorter, the outline and the notes page, the status bar - and the
//! callbacks of their widgets.

use azul::{
    css::{EventFilter, FocusEventFilter},
    dom::{DomId, VirtualKeyCode},
    svg::{CssPath, CssPathSelector},
    callbacks::{
        ButtonOnClickCallbackType, CallbackInfo, NumberInputOnValueChangeCallbackType, RefAny,
        SelectionAdornerOnEventCallbackType, StatusBarOnViewSelectCallbackType, TextAreaOnFocusLostCallbackType, TextAreaOnTextInputCallbackType,
        TextInputOnFocusLostCallbackType, ThumbnailStripOnEventCallbackType, Update,
    },
    dom::Dom,
    str::String as AzString,
    widgets::{
        AdornerFrame, AdornerItem, Button, NumberInput, NumberInputState, OnTextInputReturn,
        RichTextEditorState, SelectionAdorner, SelectionAdornerEvent, SelectionAdornerEventKind, SliderState,
        StatusBar, StatusBarSegment, StatusBarView, StatusBarViewSwitcher, StatusBarZoom, TextArea, TextAreaState,
        TextInput, TextInputState, TextInputValid, ThumbnailItem, ThumbnailStrip, ThumbnailStripEvent,
        ThumbnailStripEventKind, ThumbnailStripLayout,
    },
};

use crate::{
    app::{command, AppState, Command, Play, TransitionFrom, View},
    commands::{self, on_command},
    editor::Editor,
    model::{Background, Deck, ElementKind, Frame, PlaceholderRole, Slide, TextBody},
    render::{self, css_color, RenderOptions},
};


fn s(text: &str) -> AzString {
    AzString::from(text)
}

// ==== The slide rail and the sorter ====

/// A slide's preview, `width` px wide.
fn preview(deck: &Deck, slide: &Slide, width: f32, st: &AppState) -> Dom {
    let scale = width / deck.size.width();
    render::slide_dom(deck, slide, &RenderOptions::still(scale, &st.media))
}

/// The rail (a column) or the sorter (a grid) of the deck's slides.
#[must_use]
pub fn strip(app: &RefAny, st: &AppState, ed: &Editor, layout: ThumbnailStripLayout) -> Dom {
    let width = if layout == ThumbnailStripLayout::Grid { 220.0 } else { 150.0 };
    let height = width * ed.deck.size.height() / ed.deck.size.width();
    let mut items = Vec::with_capacity(ed.deck.slides.len());
    for (i, slide) in ed.deck.slides.iter().enumerate() {
        let title = slide.title();
        let name = if title.is_empty() {
            format!("Slide {}", i + 1)
        } else {
            format!("Slide {}: {title}", i + 1)
        };
        let badge = if !slide.build_steps().is_empty() {
            "auto_awesome"
        } else if slide.transition.kind != crate::model::TransitionKind::None {
            "animation"
        } else {
            ""
        };
        let mut item = ThumbnailItem::create(preview(&ed.deck, slide, width, st), s(&format!("{}", i + 1)), s(&name))
            .with_selected(ed.rail.contains(i as u64))
            .with_hidden(slide.hidden)
            .with_badge(s(badge));
        if let Some(section) = &slide.section {
            item = item
                .with_section(s(section))
                .with_section_collapsed(ed.folded.contains(&slide.id));
        }
        items.push(item);
    }
    ThumbnailStrip::create(items)
        .with_layout(layout)
        .with_active(ed.current)
        .with_thumb_size(width, height)
        .with_accessibility_name(s("Slides"))
        .with_on_event(app.clone(), on_strip_event as ThumbnailStripOnEventCallbackType)
        .dom()
}

/// The rail's and the sorter's events.
pub extern "C" fn on_strip_event(mut data: RefAny, mut info: CallbackInfo, event: ThumbnailStripEvent) -> Update {
    let Some(mut guard) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    let st = &mut *guard;
    commands::sync_editing(st, &mut info);
    let sorter = st.view == View::Sorter;
    let Some(ed) = st.editor.as_mut() else {
        return Update::DoNothing;
    };
    match event.kind {
        ThumbnailStripEventKind::Select => {
            ed.rail_select(event.index, event.shift, event.ctrl);
            println!("AZSHOW_SLIDE {}", ed.current + 1);
        }
        ThumbnailStripEventKind::Activate => {
            ed.go_to(event.index);
            if sorter {
                st.view = View::Normal;
                println!("AZSHOW_VIEW {}", View::Normal.label());
            }
            if let Some(ed) = st.editor.as_ref() {
                println!("AZSHOW_SLIDE {}", ed.current + 1);
            }
        }
        ThumbnailStripEventKind::Move => {
            ed.move_slides(event.index, event.target);
            let order: Vec<String> = ed.deck.slides.iter().map(|sl| sl.id.to_string()).collect();
            println!("AZSHOW_ORDER {}", order.join(","));
        }
        ThumbnailStripEventKind::Delete => ed.delete_slides(),
        ThumbnailStripEventKind::SectionToggled => ed.toggle_fold(event.index),
    }
    Update::RefreshDom
}

// ==== The canvas ====

/// The slide's shadow on the canvas: flat's black one on its grey ground; under flora a leaf's
/// (flora.css `--fl-shadow-2`'s first layer, deepened to carry a slide), warm by day, black at
/// night, on the desk.
const SLIDE_SHADOW: &str = "box-shadow: 0px 2px 8px rgba(0, 0, 0, 0.45); flex-shrink: 0; \
                            @theme(flora) { box-shadow: 0px 2px 8px rgba(48, 45, 38, 0.3); \
                            @media (prefers-color-scheme: dark) { box-shadow: 0px 2px 8px \
                            rgba(0, 0, 0, 0.55); } }";

/// The slide under the selection adorner at `scale`, centred in a scroll box.
#[must_use]
pub fn canvas(app: &RefAny, st: &AppState, ed: &Editor, scale: f32) -> Dom {
    let deck = &ed.deck;
    let slide = ed.slide();
    let opts = RenderOptions {
        scale,
        editing: ed.editing,
        text: ed.text.as_ref(),
        prompts: true,
        step: None,
        playing: None,
        media: &st.media,
        hooks: Some(app),
        element_ids: false,
    };
    let content = match st.preview.as_ref().filter(|p| p.slide == ed.current) {
        Some(play) => preview_dom(deck, slide, play, &opts),
        None => render::slide_dom(deck, slide, &opts),
    }
    .with_id(crate::ids::SLIDE);
    let items: Vec<AdornerItem> = slide
        .elements
        .iter()
        .map(|e| AdornerItem {
            frame: AdornerFrame {
                x: e.frame.x,
                y: e.frame.y,
                width: e.frame.w,
                height: e.frame.h,
                rotation: e.frame.rotation,
            },
            selected: ed.selection.contains(e.id),
        })
        .collect();
    let mut adorner = SelectionAdorner::create(content, deck.size.width(), deck.size.height())
        .with_scale(scale)
        .with_items(items)
        .with_guides(st.guides.clone())
        .with_nudge(1.0)
        .with_accessibility_name(s(&format!("Slide {} of {}", ed.current + 1, deck.slides.len())))
        .with_on_event(app.clone(), on_adorner_event as SelectionAdornerOnEventCallbackType);
    if let Some(m) = st.marquee {
        adorner = adorner.with_marquee(m);
    }
    if let Some(i) = ed.editing.and_then(|id| slide.index_of(id)) {
        adorner = adorner.with_editing(i);
    }
    Dom::create_div()
        .with_id(crate::ids::CANVAS)
        .with_css(
            "display: flex; flex-direction: column; align-items: center; justify-content: center; \
             flex-grow: 1; min-height: 0px; overflow: auto; padding: 24px; background: #8f8f8f; \
             @theme(flora) { background: system:under-page-background; }",
        )
        .with_child(
            Dom::create_div()
                .with_css(SLIDE_SHADOW)
                .with_child(adorner.dom()),
        )
}

/// The canvas while a preview plays (`AppState::preview`): the slide's
/// transition from the slide before it, or one click of its builds, at the
/// preview's progress - drawn as the show draws it, with nothing in editing.
fn preview_dom(deck: &Deck, slide: &Slide, play: &Play, opts: &RenderOptions<'_>) -> Dom {
    play.count_frame();
    let p = play.progress();
    let shown = RenderOptions {
        editing: None,
        text: None,
        prompts: false,
        hooks: None,
        element_ids: true,
        ..*opts
    };
    match play.transition {
        Some(from) => {
            // The slide before with all its builds played.
            let from = match from {
                TransitionFrom::Slide(i) => {
                    deck.slides.get(i).map(|s| (s, s.build_steps().len()))
                }
                TransitionFrom::Black => None,
            };
            let to = RenderOptions {
                step: Some(0),
                ..shown
            };
            let (w, h) = (deck.size.width() * opts.scale, deck.size.height() * opts.scale);
            let layers = render::transition_layers(deck, from, slide, slide.transition.kind, p, &to);
            render::stage(w, h, layers)
        }
        None => {
            // The slide as the show has it after this click, the click playing.
            let step = play
                .ids
                .first()
                .and_then(|id| slide.build_step_of(*id))
                .map_or(0, |s| s + 1);
            let build = RenderOptions {
                step: Some(step),
                playing: Some((play.ids.as_slice(), p)),
                ..shown
            };
            render::slide_dom(deck, slide, &build)
        }
    }
}

fn frame_of(f: &AdornerFrame) -> Frame {
    Frame {
        x: f.x,
        y: f.y,
        w: f.width,
        h: f.height,
        rotation: f.rotation,
    }
}

/// The canvas's events: select, move / resize / rotate, marquee, edit, nudge.
pub extern "C" fn on_adorner_event(mut data: RefAny, mut info: CallbackInfo, event: SelectionAdornerEvent) -> Update {
    let handle = data.clone();
    let Some(mut guard) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    let st = &mut *guard;
    let indices: Vec<usize> = event.indices.as_ref().iter().map(|&i| i as usize).collect();
    let frames: Vec<Frame> = event.frames.as_ref().iter().map(frame_of).collect();
    let leaves_text = matches!(
        event.kind,
        SelectionAdornerEventKind::Select | SelectionAdornerEventKind::Clear | SelectionAdornerEventKind::Escape
    );
    if leaves_text {
        commands::sync_editing(st, &mut info);
    }
    let Some(ed) = st.editor.as_mut() else {
        return Update::DoNothing;
    };
    match event.kind {
        SelectionAdornerEventKind::Select => {
            if let Some(&i) = indices.first() {
                ed.select_element(i, event.shift, event.ctrl);
            }
        }
        SelectionAdornerEventKind::Clear => ed.clear_selection(),
        SelectionAdornerEventKind::Transform => {
            ed.transform(&indices, &frames, false);
            st.guides = event.guides.as_ref().to_vec();
        }
        SelectionAdornerEventKind::Commit | SelectionAdornerEventKind::Nudge => {
            ed.transform(&indices, &frames, true);
            st.guides.clear();
            if let (Some(&i), Some(f)) = (indices.first(), frames.first()) {
                if let Some(e) = ed.slide().elements.get(i) {
                    println!(
                        "AZSHOW_FRAME {} {:.0} {:.0} {:.0} {:.0} {:.0}",
                        e.id, f.x, f.y, f.w, f.h, f.rotation
                    );
                }
            }
        }
        SelectionAdornerEventKind::Marquee => st.marquee = event.frames.as_ref().first().copied(),
        SelectionAdornerEventKind::MarqueeEnd => {
            st.marquee = None;
            ed.select_indices(&indices);
        }
        SelectionAdornerEventKind::Activate => {
            if let Some(&i) = indices.first() {
                if ed.activate(i) {
                    // The text's editing host, or a table's first cell.
                    st.focus_text = ed.editing.map(|id| match ed.slide().element(id).map(|e| &e.kind) {
                        Some(ElementKind::Table { .. }) => crate::text::cell_id(id, 0, 0),
                        _ => crate::text::host_id(id),
                    });
                    crate::focus_text_soon(&mut info, &handle);
                }
            }
        }
        SelectionAdornerEventKind::Delete => ed.delete_selection(),
        SelectionAdornerEventKind::Escape => {
            if ed.editing.is_some() {
                ed.stop_editing();
            } else {
                ed.clear_selection();
            }
        }
    }
    Update::RefreshDom
}

/// The shared editor reports an edit of the text being edited (typing,
/// Enter / Backspace across paragraphs, a format, its undo): the body takes
/// the editor's document, the editor's state is kept for the next frame (no
/// rebuild: the editor shows it).
pub extern "C" fn on_text_change(mut data: RefAny, _info: CallbackInfo, state: RichTextEditorState) -> Update {
    let Some(mut guard) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    let Some(ed) = guard.editor.as_mut() else {
        return Update::DoNothing;
    };
    if let Some(body) = ed.edited_body_mut() {
        if crate::text::set_from_rich(body, &state.doc) {
            ed.dirty = true;
        }
    }
    ed.text = Some(state);
    Update::DoNothing
}

// ==== A table edited in place ====

/// A cell's payload: the app and which cell it is.
struct CellRef {
    app: RefAny,
    element: u64,
    row: usize,
    col: usize,
}

/// `td`, cell `row`, `col` of table `element`, as an editing host of its
/// own: typing is folded into the cell, Enter stays in it, Tab / Shift+Tab
/// go to the next / previous cell (Tab in the last cell adds a row).
#[must_use]
pub fn editable_cell(td: Dom, app: &RefAny, element: u64, row: usize, col: usize) -> Dom {
    let data = RefAny::new(CellRef {
        app: app.clone(),
        element,
        row,
        col,
    });
    td.with_id(AzString::from(crate::text::cell_id(element, row, col)))
        .with_contenteditable(true)
        .with_css("cursor: text; min-width: 24px;")
        .with_callback(EventFilter::Focus(FocusEventFilter::TextChanged), data.clone(), on_cell_text)
        .with_callback(EventFilter::Focus(FocusEventFilter::VirtualKeyDown), data, on_cell_key)
}

fn cell_of(data: &mut RefAny) -> Option<(RefAny, u64, usize, usize)> {
    let c = data.downcast_ref::<CellRef>()?;
    Some((c.app.clone(), c.element, c.row, c.col))
}

/// Typing in a cell: its text (the engine's edit of the cell's text node)
/// becomes the cell's.
extern "C" fn on_cell_text(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, element, row, col)) = cell_of(&mut data) else {
        return Update::DoNothing;
    };
    let cell = info.get_hit_node();
    let edits = info.get_unsynced_text_edits();
    let mut text = None;
    let mut max_revision = 0u64;
    for edit in edits.as_ref() {
        max_revision = max_revision.max(edit.revision);
        if info.get_node_child_index_path(cell, edit.node).into_option().is_some() {
            text = Some(edit.text.as_str().to_string());
        }
    }
    if max_revision > 0 {
        info.mark_text_revision_synced(max_revision);
    }
    let Some(text) = text else {
        return Update::DoNothing;
    };
    if let Some(mut st) = app.downcast_mut::<AppState>() {
        if let Some(ed) = st.editor.as_mut() {
            if ed.set_cell(element, row, col, &text) {
                println!("AZSHOW_CELL {element} {row} {col}");
            }
        }
    }
    Update::DoNothing
}

/// Enter stays in the cell; Tab / Shift+Tab move between the cells.
extern "C" fn on_cell_key(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, element, row, col)) = cell_of(&mut data) else {
        return Update::DoNothing;
    };
    let keyboard = info.get_current_keyboard_state();
    let Some(key) = keyboard.current_virtual_keycode.into_option() else {
        return Update::DoNothing;
    };
    let shift = info.get_key_modifiers().shift;
    match key {
        VirtualKeyCode::Return | VirtualKeyCode::NumpadEnter => {
            info.prevent_default();
            Update::DoNothing
        }
        VirtualKeyCode::Tab => {
            info.prevent_default();
            let back = shift;
            let mut added = false;
            let target = {
                let Some(mut st) = app.downcast_mut::<AppState>() else {
                    return Update::DoNothing;
                };
                let Some(ed) = st.editor.as_mut() else {
                    return Update::DoNothing;
                };
                let shape = match ed.slide().element(element).map(|e| &e.kind) {
                    Some(ElementKind::Table { rows, .. }) => {
                        rows.iter().map(Vec::len).collect::<Vec<usize>>()
                    }
                    _ => return Update::DoNothing,
                };
                match next_cell(&shape, row, col, back) {
                    Some(at) => Some(at),
                    None if !back => {
                        // Tab in the last cell: a new row, its first cell.
                        added = true;
                        ed.add_table_row(element).map(|r| (r, 0))
                    }
                    None => None,
                }
            };
            let Some((r, c)) = target else {
                return Update::DoNothing;
            };
            let id = crate::text::cell_id(element, r, c);
            if added {
                // The new row is there after the rebuild: focus it then.
                if let Some(mut st) = app.downcast_mut::<AppState>() {
                    st.focus_text = Some(id);
                }
                crate::focus_text_soon(&mut info, &app);
                return Update::RefreshDom;
            }
            info.set_focus_to_path(
                DomId { inner: 0 },
                CssPath {
                    selectors: vec![CssPathSelector::Id(AzString::from(id))].into(),
                },
            );
            Update::DoNothing
        }
        _ => Update::DoNothing,
    }
}

/// The cell after (`back`: before) `row`, `col` in a table whose rows have
/// `shape[r]` cells, row by row; `None` past either end.
#[must_use]
pub fn next_cell(shape: &[usize], row: usize, col: usize, back: bool) -> Option<(usize, usize)> {
    if back {
        if col > 0 {
            return Some((row, col - 1));
        }
        (0..row).rev().find(|r| shape[*r] > 0).map(|r| (r, shape[r] - 1))
    } else {
        if col + 1 < shape.get(row).copied().unwrap_or(0) {
            return Some((row, col + 1));
        }
        (row + 1..shape.len()).find(|r| shape[*r] > 0).map(|r| (r, 0))
    }
}

// ==== Notes ====

fn notes_field(app: &RefAny, notes: &str) -> Dom {
    TextArea::create()
        .with_text(s(notes))
        .with_placeholder(s("Click to add notes"))
        .with_accessibility_name(s("Notes"))
        .with_on_text_input(app.clone(), on_notes_input as TextAreaOnTextInputCallbackType)
        .with_on_focus_lost(app.clone(), on_notes_done as TextAreaOnFocusLostCallbackType)
        .dom()
        .with_id(crate::ids::NOTES)
}

extern "C" fn on_notes_input(mut data: RefAny, _info: CallbackInfo, state: TextAreaState) -> OnTextInputReturn {
    if let Some(mut st) = data.downcast_mut::<AppState>() {
        if let Some(ed) = st.editor.as_mut() {
            ed.set_notes(state.get_text().as_str());
        }
    }
    OnTextInputReturn {
        update: Update::DoNothing,
        valid: TextInputValid::Yes,
    }
}

extern "C" fn on_notes_done(mut data: RefAny, _info: CallbackInfo, state: TextAreaState) -> Update {
    if let Some(mut st) = data.downcast_mut::<AppState>() {
        if let Some(ed) = st.editor.as_mut() {
            ed.set_notes(state.get_text().as_str());
        }
    }
    Update::DoNothing
}

/// The normal view's document: the canvas over the notes.
#[must_use]
pub fn normal_document(app: &RefAny, st: &AppState, ed: &Editor, scale: f32) -> Dom {
    let mut column = Dom::create_div()
        .with_css("display: flex; flex-direction: column; flex-grow: 1; min-height: 0px;")
        .with_child(canvas(app, st, ed, scale));
    if st.show_notes {
        column.add_child(
            Dom::create_div()
                .with_css("display: flex; flex-direction: column; height: 110px; flex-shrink: 0; padding: 4px 8px;")
                .with_child(notes_field(app, &ed.slide().notes)),
        );
    }
    column
}

// ==== The outline ====

/// What an outline field edits.
struct OutlineField {
    app: RefAny,
    slide: usize,
    role: PlaceholderRole,
}

extern "C" fn on_outline_title(mut data: RefAny, _info: CallbackInfo, state: TextInputState) -> Update {
    let (mut app, slide, role) = match data.downcast_ref::<OutlineField>() {
        Some(f) => (f.app.clone(), f.slide, f.role),
        None => return Update::DoNothing,
    };
    let text = state.get_text();
    if let Some(mut st) = app.downcast_mut::<AppState>() {
        if let Some(ed) = st.editor.as_mut() {
            ed.set_placeholder_text(slide, role, text.as_str());
        }
    }
    Update::RefreshDom
}

extern "C" fn on_outline_body(mut data: RefAny, _info: CallbackInfo, state: TextAreaState) -> Update {
    let (mut app, slide, role) = match data.downcast_ref::<OutlineField>() {
        Some(f) => (f.app.clone(), f.slide, f.role),
        None => return Update::DoNothing,
    };
    let text = state.get_text().to_string();
    if let Some(mut st) = app.downcast_mut::<AppState>() {
        if let Some(ed) = st.editor.as_mut() {
            ed.set_placeholder_text(slide, role, &text);
        }
    }
    Update::RefreshDom
}

fn text_of(slide: &Slide, role: PlaceholderRole) -> Option<String> {
    slide.placeholder(role).and_then(|e| e.body()).map(TextBody::text)
}

/// The outline: every slide's number and title, its body's lines under it.
#[must_use]
pub fn outline(app: &RefAny, ed: &Editor) -> Dom {
    let mut list = Dom::create_div()
        .with_css("display: flex; flex-direction: column; flex-grow: 1; overflow-y: auto; padding: 8px;");
    for (i, slide) in ed.deck.slides.iter().enumerate() {
        let field = |role| {
            RefAny::new(OutlineField {
                app: app.clone(),
                slide: i,
                role,
            })
        };
        let mut row = Dom::create_div()
            .with_css("display: flex; flex-direction: column; margin: 0px 0px 10px 0px;")
            .with_child(
                Dom::create_div()
                    .with_css("display: flex; flex-direction: row; align-items: center;")
                    .with_child(
                        Button::create(s(&format!("{}", i + 1)))
                            .with_on_click(command(app, Command::GoToSlide(i)), on_command as ButtonOnClickCallbackType)
                            .dom(),
                    )
                    .with_child(
                        TextInput::create()
                            .with_text(s(&slide.title()))
                            .with_placeholder(s("Title"))
                            .with_accessibility_name(s(&format!("Slide {} title", i + 1)))
                            .with_on_focus_lost(field(PlaceholderRole::Title), on_outline_title as TextInputOnFocusLostCallbackType)
                            .dom(),
                    ),
            );
        let body_role = [PlaceholderRole::Body, PlaceholderRole::Subtitle]
            .into_iter()
            .find(|r| slide.placeholder(*r).is_some());
        if let Some(role) = body_role {
            row.add_child(
                TextArea::create()
                    .with_text(s(&text_of(slide, role).unwrap_or_default()))
                    .with_placeholder(s("Bullets, one per line"))
                    .with_accessibility_name(s(&format!("Slide {} text", i + 1)))
                    .with_on_focus_lost(field(role), on_outline_body as TextAreaOnFocusLostCallbackType)
                    .dom(),
            );
        }
        list.add_child(row);
    }
    list
}

// ==== The notes page ====

/// The notes page: every slide over its notes, the current one first.
#[must_use]
pub fn notes_page(app: &RefAny, st: &AppState, ed: &Editor) -> Dom {
    let mut pages = Dom::create_div().with_css(
        "display: flex; flex-direction: column; align-items: center; flex-grow: 1; overflow-y: auto; \
         padding: 24px; background: #8f8f8f; @theme(flora) { background: \
         system:under-page-background; }",
    );
    let width = 640.0;
    for (i, slide) in ed.deck.slides.iter().enumerate() {
        let notes = if i == ed.current {
            notes_field(app, &slide.notes)
        } else {
            Dom::create_p_with_text(if slide.notes.is_empty() { "(no notes)" } else { slide.notes.as_str() })
                .with_css("margin: 0px; font-size: 14px; color: #262626;")
        };
        pages.add_child(
            Dom::create_div()
                .with_css(format!(
                    "display: flex; flex-direction: column; width: {width:.0}px; background: #ffffff; \
                     padding: 24px; margin: 0px 0px 24px 0px; flex-shrink: 0;"
                ))
                .with_child(preview(&ed.deck, slide, width - 48.0, st))
                .with_child(Dom::create_div().with_css("height: 16px;"))
                .with_child(notes),
        );
    }
    pages
}

// ==== The format pane ====

/// What a size field sets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FrameField {
    X,
    Y,
    W,
    H,
    Rotation,
}

struct FrameFieldData {
    app: RefAny,
    field: FrameField,
}

extern "C" fn on_frame_field(mut data: RefAny, _info: CallbackInfo, state: NumberInputState) -> Update {
    let (mut app, field) = match data.downcast_ref::<FrameFieldData>() {
        Some(d) => (d.app.clone(), d.field),
        None => return Update::DoNothing,
    };
    let Some(mut st) = app.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    let Some(ed) = st.editor.as_mut() else {
        return Update::DoNothing;
    };
    let Some(&i) = ed.selected_indices().first() else {
        return Update::DoNothing;
    };
    let mut f = ed.slide().elements[i].frame;
    let v = state.number;
    match field {
        FrameField::X => f.x = v,
        FrameField::Y => f.y = v,
        FrameField::W => f.w = v.max(1.0),
        FrameField::H => f.h = v.max(1.0),
        FrameField::Rotation => f.rotation = v.rem_euclid(360.0),
    }
    ed.transform(&[i], &[f], true);
    Update::RefreshDom
}

fn frame_field(app: &RefAny, label: &str, field: FrameField, value: f32) -> Dom {
    Dom::create_div()
        .with_css("display: flex; flex-direction: row; align-items: center; margin: 0px 0px 4px 0px;")
        .with_child(Dom::create_p_with_text(label).with_css("margin: 0px 8px 0px 0px; width: 70px; font-size: 12px;"))
        .with_child(
            NumberInput::create(value.round())
                .with_accessibility_name(s(label))
                .with_on_value_change(
                    RefAny::new(FrameFieldData {
                        app: app.clone(),
                        field,
                    }),
                    on_frame_field as NumberInputOnValueChangeCallbackType,
                )
                .dom(),
        )
}

/// A pane's section title: bold in flat; flora's label under flora (capitals in the label ink).
fn section_title(text: &str) -> Dom {
    Dom::create_p_with_text(text).with_css(
        "margin: 12px 0px 6px 0px; font-size: 13px; font-weight: bold; @theme(flora) { \
         font-size: 11px; text-transform: uppercase; letter-spacing: 0.12em; color: \
         system:secondary-text; }",
    )
}

fn swatch(app: &RefAny, label: &str, cmd: Command) -> Dom {
    Button::create(s(label))
        .with_on_click(command(app, cmd), on_command as ButtonOnClickCallbackType)
        .dom()
        .with_css("margin: 0px 4px 4px 0px;")
}

/// The format pane: the selection's size and position, fill and builds,
/// or the slide's ground and transition.
#[must_use]
pub fn format_pane(app: &RefAny, ed: &Editor) -> Dom {
    let c = &ed.deck.theme.colors;
    let mut pane = Dom::create_div()
        .with_css("display: flex; flex-direction: column; flex-grow: 1; padding: 8px 12px; overflow-y: auto; font-size: 12px;");
    let selected = ed.selected_indices();
    if let Some(&i) = selected.first() {
        let e = &ed.slide().elements[i];
        pane.add_child(section_title(&format!("Format: {}", e.name())));
        pane.add_child(section_title("Size & Position"));
        pane.add_child(frame_field(app, "X", FrameField::X, e.frame.x));
        pane.add_child(frame_field(app, "Y", FrameField::Y, e.frame.y));
        pane.add_child(frame_field(app, "Width", FrameField::W, e.frame.w));
        pane.add_child(frame_field(app, "Height", FrameField::H, e.frame.h));
        pane.add_child(frame_field(app, "Rotation", FrameField::Rotation, e.frame.rotation));
        if matches!(e.kind, ElementKind::Shape { .. }) {
            pane.add_child(section_title("Fill"));
            pane.add_child(
                Dom::create_div()
                    .with_css("display: flex; flex-direction: row; flex-wrap: wrap;")
                    .with_child(swatch(app, "Accent", Command::Fill(Some(c.accent))))
                    .with_child(swatch(app, "Accent 2", Command::Fill(Some(c.accent2))))
                    .with_child(swatch(app, "Accent 3", Command::Fill(Some(c.accent3))))
                    .with_child(swatch(app, "None", Command::Fill(None))),
            );
        }
        if let Some(body) = e.body() {
            pane.add_child(section_title("Text"));
            pane.add_child(
                Dom::create_div()
                    .with_css("display: flex; flex-direction: row; align-items: center;")
                    .with_child(swatch(app, "A-", Command::Grow(-1)))
                    .with_child(Dom::create_p_with_text(format!("{:.0}", body.size)).with_css("margin: 0px 8px;"))
                    .with_child(swatch(app, "A+", Command::Grow(1))),
            );
        }
        if selected.len() > 1 {
            pane.add_child(Dom::create_p_with_text(format!("{} objects selected", selected.len())).with_css("margin: 8px 0px;"));
        }
    } else {
        let slide = ed.slide();
        pane.add_child(section_title("Format Background"));
        pane.add_child(
            Dom::create_div()
                .with_css("display: flex; flex-direction: row; flex-wrap: wrap;")
                .with_child(swatch(app, "Theme", Command::Background(None, false)))
                .with_child(swatch(app, "Soft", Command::Background(Some(Background::Solid { color: c.accent3 }), false)))
                .with_child(swatch(
                    app,
                    "Gradient",
                    Command::Background(Some(Background::Gradient { from: c.accent, to: c.title }), false),
                )),
        );
        pane.add_child(section_title("Transition"));
        pane.add_child(
            Dom::create_p_with_text(format!(
                "{}, {:.2} s",
                slide.transition.kind.label(),
                slide.transition.duration_ms as f32 / 1000.0
            ))
            .with_css("margin: 0px;"),
        );
    }
    // The animation pane: the slide's builds in click order.
    let slide = ed.slide();
    let steps = slide.build_steps();
    pane.add_child(section_title("Animation Pane"));
    if steps.is_empty() {
        pane.add_child(Dom::create_p_with_text("No animations on this slide.").with_css("margin: 0px;"));
    }
    for (n, step) in steps.iter().enumerate() {
        for id in step {
            if let Some(e) = slide.element(*id) {
                let effect = e.animation.map_or("", |a| a.effect.label());
                let selected = ed.selection.contains(*id);
                pane.add_child(Dom::create_p_with_text(format!("{}  {effect}  {}", n + 1, e.name())).with_css(format!(
                    "margin: 0px 0px 2px 0px; padding: 2px 4px; {}",
                    // The pane is chrome: under flora a selected step lies on the theme's
                    // selection, not in the deck's colours.
                    if selected {
                        format!(
                            "background: {}; color: {}; @theme(flora) {{ background: \
                             system:selection-background; color: system:selection-text; }}",
                            css_color(c.accent),
                            css_color(c.background)
                        )
                    } else {
                        String::new()
                    }
                )));
            }
        }
    }
    pane
}

// ==== The status bar ====

extern "C" fn on_view_select(mut data: RefAny, mut info: CallbackInfo, index: usize) -> Update {
    let view = View::ALL.get(index).copied().unwrap_or(View::Normal);
    commands::run(&mut data, Command::View(view), &mut info)
}

extern "C" fn on_zoom_slider(mut data: RefAny, _info: CallbackInfo, slider: SliderState) -> Update {
    let Some(mut st) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    st.zoom = Some(slider.value.round().clamp(crate::app::ZOOM_MIN, crate::app::ZOOM_MAX));
    Update::RefreshDom
}

/// The status bar: slide n of m, the language, the notes toggle, the
/// message, the view buttons and the zoom.
#[must_use]
pub fn status_bar(app: &RefAny, st: &AppState, zoom_percent: f32) -> Dom {
    let (slide_label, count) = st
        .editor
        .as_ref()
        .map_or((String::from("NO PRESENTATION"), 0), |e| {
            (format!("SLIDE {} OF {}", e.current + 1, e.deck.slides.len()), e.deck.slides.len())
        });
    let _ = count;
    let mut segments = vec![
        StatusBarSegment::create(s(&slide_label)),
        StatusBarSegment::create(s("ENGLISH (UNITED STATES)")),
        StatusBarSegment::create(s(if st.show_notes { "NOTES" } else { "NOTES (HIDDEN)" }))
            .with_icon(s("notes"))
            .with_on_click(command(app, Command::ToggleNotes), on_command as ButtonOnClickCallbackType),
    ];
    if !st.message.is_empty() {
        segments.push(StatusBarSegment::create(s(&st.message)));
    }
    let views: Vec<StatusBarView> = View::ALL.iter().map(|v| StatusBarView { icon: s(v.icon()) }).collect();
    // The slider spans the buttons' whole range: a fixed 10..190 window
    // snapped a 400 % zoom back on the first drag (DEDUP_OFFICE D28).
    let zoom = StatusBarZoom::create(zoom_percent, crate::app::ZOOM_MIN, crate::app::ZOOM_MAX)
        .with_on_zoom_out(command(app, Command::Zoom(-10)), on_command as ButtonOnClickCallbackType)
        .with_on_zoom_in(command(app, Command::Zoom(10)), on_command as ButtonOnClickCallbackType)
        .with_on_slider_change(app.clone(), on_zoom_slider);
    StatusBar::create(segments)
        .with_views(
            StatusBarViewSwitcher::create(views)
                .with_active_view(st.view.index())
                .with_on_select(app.clone(), on_view_select as StatusBarOnViewSelectCallbackType),
        )
        .with_zoom(zoom)
        .dom()
}

#[cfg(test)]
mod tests {
    use super::next_cell;

    #[test]
    fn the_next_cell_goes_row_by_row() {
        let shape = [2, 2, 2];
        assert_eq!(next_cell(&shape, 0, 0, false), Some((0, 1)));
        assert_eq!(next_cell(&shape, 0, 1, false), Some((1, 0)), "the end of a row: the next row");
        assert_eq!(next_cell(&shape, 2, 1, false), None, "past the last cell: a new row");
        assert_eq!(next_cell(&shape, 1, 0, true), Some((0, 1)), "Shift+Tab: the previous row's last cell");
        assert_eq!(next_cell(&shape, 0, 0, true), None);
        assert_eq!(next_cell(&[2, 0, 1], 0, 1, false), Some((2, 0)), "an empty row is skipped");
    }
}
