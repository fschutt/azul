//! Running a [`Command`]: the one place every button, menu item and
//! shortcut ends up. Storage work goes to the storage thread, the show and
//! its presenter window are opened and closed here, the text formats go to
//! the rich text model.

use azul::{
    callbacks::{CallbackInfo, RefAny, Update},
    css::DarkLightMode,
    dialog::{FileDialog, FileOpenResult},
    dom::{Callback, Dom, DomId, DomNodeId, TextFormat},
    option::{OptionDarkLightMode, OptionFileTypeList, OptionString},
    pdf::Pdf,
    str::String as AzString,
    task::{TimerId, Timer},
    time::{Duration, SystemTimeDiff},
    window::{WindowCreateOptions, WindowDecorations, WindowFrame},
};
use azwriter::ir::FormatAxis;

use crate::{
    app::{AppState, BackstagePage, Command, CommandData, Play, Screen, ShowRuntime},
    editor::Editor,
    model::{sample_deck, Deck, FontScheme, ShowMove, ShowState},
    render::{self, RenderOptions},
    storage::Job,
    text, themes,
};

/// A button / menu item: runs its [`CommandData`].
pub extern "C" fn on_command(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let (mut app, cmd) = match data.downcast_ref::<CommandData>() {
        Some(d) => (d.app.clone(), d.cmd.clone()),
        None => return Update::DoNothing,
    };
    run(&mut app, cmd, &mut info)
}

/// Runs `cmd` on the app.
pub fn run(app: &mut RefAny, cmd: Command, info: &mut CallbackInfo) -> Update {
    let handle = app.clone();
    let Some(mut guard) = app.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    apply(&handle, &mut guard, cmd, info)
}

/// A fresh deck id (the deck's folder under `show/`): random, so no launch
/// reuses another's (`Uuid::v4` is a process-local marker sequence).
#[must_use]
pub fn new_deck_id() -> String {
    azul::uuid::Uuid::from_seed(azul_storage::ids::random_seed()).as_str().to_string()
}

/// Opens `deck` for editing on its first slide (or `--slide`).
pub fn open_deck(s: &mut AppState, deck: Deck) {
    let id = deck.id.clone();
    let mut ed = Editor::new(deck);
    if let Some(n) = s.args.slide.take() {
        ed.go_to(n.saturating_sub(1));
    }
    s.editor = Some(ed);
    s.screen = Screen::Editor;
    s.guides.clear();
    s.marquee = None;
    println!("AZSHOW_DECK {id}");
}

/// Mirrors the engine's typing into the element being edited.
pub fn sync_editing(s: &mut AppState, info: &mut CallbackInfo) -> bool {
    let Some(ed) = s.editor.as_mut() else {
        return false;
    };
    let Some(id) = ed.editing else {
        return false;
    };
    let Some(body) = ed
        .slide_mut()
        .elements
        .iter_mut()
        .find(|e| e.id == id)
        .and_then(|e| e.body_mut())
    else {
        return false;
    };
    let changed = text::sync_typing(body, id, info);
    if changed {
        ed.dirty = true;
    }
    changed
}

/// Hands `job` to a storage thread.
pub fn spawn(info: &mut CallbackInfo, app: &RefAny, s: &mut AppState, job: Job) {
    s.busy += 1;
    crate::spawn_storage(info, app, s.data_root.clone(), job);
}

/// The DOM node with the id attribute `id` in the window's DOM.
fn node_by_id(info: &CallbackInfo, id: &str) -> Option<DomNodeId> {
    let dom = DomId { inner: 0 };
    let node = info.get_node_id_by_id_attribute(dom, id);
    (node.into_raw() != 0).then_some(DomNodeId { dom, node })
}

fn format_of(axis: FormatAxis) -> TextFormat {
    match axis {
        FormatAxis::Bold => TextFormat::Bold,
        FormatAxis::Italic => TextFormat::Italic,
        FormatAxis::Underline => TextFormat::Underline,
        FormatAxis::Strike => TextFormat::Strikethrough,
    }
}

/// B / I / U / S: over the text selection of the element being edited, at
/// its caret (the engine's typing style), or over the whole selected boxes.
pub fn format(s: &mut AppState, info: &mut CallbackInfo, axis: FormatAxis) -> Update {
    sync_editing(s, info);
    let Some(ed) = s.editor.as_mut() else {
        return Update::DoNothing;
    };
    let Some(id) = ed.editing else {
        ed.with_bodies(|b| text::toggle_all(b, axis));
        return Update::RefreshDom;
    };
    let spans = info.get_document_selection();
    let mut ranges = Vec::new();
    if let Some(body) = ed.slide().element(id).and_then(|e| e.body()) {
        for span in spans.as_ref() {
            for i in 0..body.paragraphs.len() {
                let block_node = info.get_node_id_by_id_attribute(span.node.dom, text::block_id(id, i).as_str());
                if block_node.into_raw() == 0 {
                    continue;
                }
                let block = DomNodeId {
                    dom: span.node.dom,
                    node: block_node,
                };
                let Some(rel) = info.get_node_child_index_path(block, span.node).into_option() else {
                    continue;
                };
                let run = rel.as_ref().first().copied().unwrap_or(0) as usize;
                let before: usize = body.paragraphs[i].runs.iter().take(run).map(|r| r.text.len()).sum();
                ranges.push((i, before + span.start_byte as usize, before + span.end_byte as usize));
                break;
            }
        }
    }
    if ranges.iter().all(|(_, a, b)| a >= b) {
        // A caret: the engine styles what is typed next.
        if let Some(host) = node_by_id(info, &text::host_id(id)) {
            info.toggle_text_format(host, format_of(axis));
        }
        return Update::DoNothing;
    }
    ed.checkpoint();
    let mut changed = false;
    if let Some(body) = ed
        .slide_mut()
        .elements
        .iter_mut()
        .find(|e| e.id == id)
        .and_then(|e| e.body_mut())
    {
        for (p, a, b) in ranges {
            changed |= text::toggle_range(body, p, a, b, axis);
        }
    }
    if changed {
        Update::RefreshDom
    } else {
        Update::DoNothing
    }
}

/// Starts the show from the first slide or the current one: the window goes
/// full screen and stays awake, the presenter view opens in a second window.
pub fn start_show(s: &mut AppState, info: &mut CallbackInfo, from_current: bool) -> Update {
    sync_editing(s, info);
    let Some(ed) = s.editor.as_mut() else {
        return Update::DoNothing;
    };
    ed.stop_editing();
    let from = if from_current { ed.current } else { 0 };
    let state = ShowState::start(&ed.deck, from);
    let presenter = !s.args.no_presenter;
    s.show = Some(ShowRuntime {
        state,
        started: std::time::Instant::now(),
        presenter,
        play: None,
        typed: String::new(),
        return_view: s.view,
    });
    s.screen = Screen::Show;
    let mut ws = info.get_current_window_state();
    ws.flags.frame = WindowFrame::Fullscreen;
    ws.flags.prevent_system_sleep = true;
    info.modify_window_state(ws);
    if presenter {
        let mut options = WindowCreateOptions::create(crate::presenter_layout);
        options.window_state.title = AzString::from("Presenter View - AzShow");
        options.window_state.size.dimensions.width = 1100.0;
        options.window_state.size.dimensions.height = 700.0;
        options.window_state.flags.decorations = WindowDecorations::NoTitle;
        options.create_callback = Some(Callback::create(crate::on_presenter_created)).into();
        info.create_window(options);
    }
    println!("AZSHOW_SHOW {} {}", state.slide + 1, state.step);
    Update::RefreshDomAllWindows
}

/// Ends the show: the window comes back from full screen on the slide the
/// show was on.
pub fn end_show(s: &mut AppState, info: &mut CallbackInfo) -> Update {
    let Some(rt) = s.show.take() else {
        return Update::DoNothing;
    };
    if let Some(ed) = s.editor.as_mut() {
        ed.go_to(rt.state.slide);
    }
    s.screen = Screen::Editor;
    s.view = rt.return_view;
    let mut ws = info.get_current_window_state();
    ws.flags.frame = WindowFrame::Normal;
    ws.flags.prevent_system_sleep = false;
    info.modify_window_state(ws);
    println!("AZSHOW_SHOW_CLOSED");
    Update::RefreshDomAllWindows
}

/// Starts the timer that plays builds and transitions (one at a time).
fn play(s: &mut AppState, app: &RefAny, info: &mut CallbackInfo, play: Play) {
    let Some(rt) = s.show.as_mut() else {
        return;
    };
    rt.play = Some(play);
    if !s.playing_timer {
        s.playing_timer = true;
        let timer = Timer::create(app.clone(), crate::on_play_tick, info.get_system_time_fn())
            .with_interval(Duration::System(SystemTimeDiff::from_millis(16)));
        info.add_timer(TimerId::unique(), timer);
    }
}

/// A show key: forward, back, a blank screen, a jump.
pub fn show_move(s: &mut AppState, app: &RefAny, info: &mut CallbackInfo, cmd: &Command) -> Update {
    let Some(deck) = s.editor.as_ref().map(|e| e.deck.clone()) else {
        return Update::DoNothing;
    };
    let Some(rt) = s.show.as_mut() else {
        return Update::DoNothing;
    };
    if rt.state.ended && matches!(cmd, Command::ShowNext) {
        return end_show(s, info);
    }
    let before = rt.state;
    let moved = match cmd {
        Command::ShowNext => rt.state.next(&deck),
        Command::ShowPrev => rt.state.prev(&deck),
        Command::ShowBlank(b) => {
            rt.state.toggle_blank(*b);
            ShowMove::Stay
        }
        Command::ShowGoto(i) => {
            rt.state.goto(&deck, *i);
            ShowMove::Slide
        }
        _ => ShowMove::Stay,
    };
    let state = rt.state;
    let forward = matches!(cmd, Command::ShowNext);
    match moved {
        ShowMove::Step if forward => {
            let slide = &deck.slides[state.slide];
            let ids = slide.build_steps().get(state.step - 1).cloned().unwrap_or_default();
            let duration_ms = ids
                .iter()
                .filter_map(|id| slide.element(*id).and_then(|e| e.animation))
                .map(|a| a.duration_ms)
                .max()
                .unwrap_or(500);
            play(
                s,
                app,
                info,
                Play {
                    ids,
                    transition_from: None,
                    started: std::time::Instant::now(),
                    duration_ms,
                },
            );
        }
        ShowMove::Slide if forward => {
            let t = deck.slides[state.slide].transition;
            if t.kind != crate::model::TransitionKind::None {
                play(
                    s,
                    app,
                    info,
                    Play {
                        ids: Vec::new(),
                        transition_from: Some(before.slide),
                        started: std::time::Instant::now(),
                        duration_ms: t.duration_ms,
                    },
                );
            }
        }
        _ => {
            if let Some(rt) = s.show.as_mut() {
                rt.play = None;
            }
        }
    }
    if moved == ShowMove::End {
        println!("AZSHOW_SHOW_ENDED");
    } else {
        println!("AZSHOW_SHOW {} {}", state.slide + 1, state.step);
    }
    Update::RefreshDomAllWindows
}

/// File > Export: every shown slide as a page of a PDF (960 x 540 px pages
/// for 16:9), through azul's PDF path, saved where the user says.
pub fn export_pdf(s: &mut AppState, info: &mut CallbackInfo) -> Update {
    sync_editing(s, info);
    let Some(ed) = s.editor.as_ref() else {
        return Update::DoNothing;
    };
    let deck = &ed.deck;
    let scale = 0.5;
    let (w, h) = (deck.size.width() * scale, deck.size.height() * scale);
    let mut body = Dom::create_body().with_css("margin: 0px; padding: 0px;");
    for slide in deck.slides.iter().filter(|sl| !sl.hidden) {
        let opts = RenderOptions::still(scale, &s.media);
        body.add_child(
            Dom::create_div()
                .with_css(format!("width: {w:.0}px; height: {h:.0}px; overflow: hidden;"))
                .with_child(render::slide_dom(deck, slide, &opts)),
        );
    }
    let bytes = Pdf::create().from_dom_in_callback(*info, body, w, h).as_ref().to_vec();
    if bytes.is_empty() {
        s.message = String::from("The PDF export produced no bytes");
        return Update::RefreshDom;
    }
    let name = format!("{}.pdf", deck.title);
    let len = bytes.len();
    if FileDialog::save_bytes(AzString::from(name.clone()), AzString::from("application/pdf"), bytes) {
        s.message = format!("Exported {name} ({len} bytes)");
        println!("AZSHOW_EXPORTED pdf {len}");
    }
    Update::RefreshDom
}

/// File > Export: the current slide as a PNG (a screenshot of the canvas).
pub fn export_image(s: &mut AppState, info: &mut CallbackInfo) -> Update {
    let Some(node) = node_by_id(info, crate::views::SLIDE_ID) else {
        s.message = String::from("Open the normal view to export the slide as a picture");
        return Update::RefreshDom;
    };
    let current = s.editor.as_ref().map_or(0, |e| e.current);
    match info.take_screenshot_of_node(node).into_result() {
        Ok(png) => {
            let len = png.as_ref().len();
            let name = format!("slide-{}.png", current + 1);
            if FileDialog::save_bytes(AzString::from(name.clone()), AzString::from("image/png"), png) {
                s.message = format!("Exported {name}");
                println!("AZSHOW_EXPORTED png {len}");
            }
        }
        Err(e) => s.message = format!("The slide picture failed: {}", e.as_str()),
    }
    Update::RefreshDom
}

/// Insert > Pictures: the picked file goes into the deck's media folder.
extern "C" fn on_picture_picked(mut data: RefAny, mut info: CallbackInfo, result: RefAny) -> Update {
    let Some(picked) = FileOpenResult::downcast(result).into_option() else {
        return Update::DoNothing;
    };
    let Some(path) = picked.path.into_option() else {
        return Update::DoNothing;
    };
    let path = std::path::PathBuf::from(path.as_string().as_str());
    let handle = data.clone();
    let Some(mut s) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    let Some(deck) = s.editor.as_ref().map(|e| e.deck.id.clone()) else {
        return Update::DoNothing;
    };
    spawn(&mut info, &handle, &mut *s, Job::ImportFile { deck, path });
    Update::DoNothing
}

fn picture_filter() -> OptionFileTypeList {
    use azul::file::FileTypeList;
    OptionFileTypeList::Some(FileTypeList {
        document_types: vec![
            AzString::from("*.png"),
            AzString::from("*.jpg"),
            AzString::from("*.jpeg"),
            AzString::from("*.gif"),
            AzString::from("*.bmp"),
        ]
        .into(),
        document_descriptor: AzString::from("Pictures"),
    })
}

/// The window's size, for the zoom steps.
fn window_size(info: &CallbackInfo) -> (f32, f32) {
    let ws = info.get_current_window_state();
    (ws.size.dimensions.width, ws.size.dimensions.height)
}

/// Runs `cmd` with the state at hand.
pub fn apply(app: &RefAny, s: &mut AppState, cmd: Command, info: &mut CallbackInfo) -> Update {
    use Command as C;

    // Commands that need no deck.
    match &cmd {
        C::OpenBackstage(page) => {
            sync_editing(s, info);
            if let Some(ed) = s.editor.as_mut() {
                ed.stop_editing();
            }
            s.screen = Screen::Backstage;
            s.page = *page;
            if *page == BackstagePage::Open {
                spawn(info, app, s, Job::List);
            }
            if *page == BackstagePage::Save {
                return apply(app, s, C::Save, info);
            }
            return Update::RefreshDom;
        }
        C::CloseBackstage => {
            if s.editor.is_some() {
                s.screen = Screen::Editor;
            }
            return Update::RefreshDom;
        }
        C::NewTheme(i) => {
            s.new_theme = *i;
            return Update::RefreshDom;
        }
        C::NewVariant(i) => {
            s.new_variant = *i;
            return Update::RefreshDom;
        }
        C::NewFonts(i) => {
            s.new_fonts = *i;
            return Update::RefreshDom;
        }
        C::NewSize(size) => {
            s.new_size = *size;
            return Update::RefreshDom;
        }
        C::NewDeck => {
            let theme = themes::theme(s.new_theme, s.new_variant, s.new_fonts);
            let deck = Deck::new(&new_deck_id(), "Presentation", theme, s.new_size);
            s.media.clear();
            open_deck(s, deck);
            s.message = String::from("New presentation");
            return Update::RefreshDom;
        }
        C::OpenSample => {
            let deck = sample_deck(&new_deck_id(), themes::theme(1, 0, 0));
            s.media.clear();
            open_deck(s, deck);
            s.message = String::from("Opened the sample deck");
            return Update::RefreshDom;
        }
        C::OpenDeck(id) => {
            s.message = format!("Opening {id}");
            spawn(info, app, s, Job::Load { id: id.clone() });
            return Update::RefreshDom;
        }
        C::RefreshDecks => {
            spawn(info, app, s, Job::List);
            return Update::RefreshDom;
        }
        C::RibbonTab(i) => {
            s.ribbon_tab = *i;
            return Update::RefreshDom;
        }
        C::AppTheme(name) => {
            info.set_theme(AzString::from(name.as_str()));
            return Update::RefreshDom;
        }
        C::Mode(dark) => {
            s.mode_choice = *dark;
            info.set_mode(match dark {
                Some(true) => OptionDarkLightMode::Some(DarkLightMode::Dark),
                Some(false) => OptionDarkLightMode::Some(DarkLightMode::Light),
                None => OptionDarkLightMode::None,
            });
            return Update::RefreshDom;
        }
        C::SettingsCategory(i) => {
            s.settings_category = *i;
            return Update::RefreshDom;
        }
        C::ShowNext | C::ShowPrev | C::ShowBlank(_) | C::ShowGoto(_) => {
            return show_move(s, app, info, &cmd);
        }
        C::ShowEnd => return end_show(s, info),
        _ => {}
    }

    if s.editor.is_none() {
        return Update::DoNothing;
    }
    // Typing first, so every command sees the text as it is on screen.
    sync_editing(s, info);

    match cmd {
        C::Save => {
            let Some(deck) = s.editor.as_ref().map(|e| e.deck.clone()) else {
                return Update::DoNothing;
            };
            s.message = String::from("Saving...");
            spawn(info, app, s, Job::Save { deck: Box::new(deck) });
            if s.screen == Screen::Backstage {
                s.screen = Screen::Editor;
            }
            return Update::RefreshDom;
        }
        C::ExportPdf => return export_pdf(s, info),
        C::ExportImages => return export_image(s, info),
        C::CloseDeck => {
            s.editor = None;
            s.media.clear();
            s.screen = Screen::Backstage;
            s.page = BackstagePage::New;
            return Update::RefreshDom;
        }
        C::StartShow { from_current } => return start_show(s, info, from_current),
        C::View(v) => {
            if let Some(ed) = s.editor.as_mut() {
                ed.stop_editing();
            }
            s.view = v;
            s.screen = Screen::Editor;
            println!("AZSHOW_VIEW {}", v.label());
            return Update::RefreshDom;
        }
        C::StopEditing => {
            if let Some(ed) = s.editor.as_mut() {
                ed.stop_editing();
            }
            return Update::RefreshDom;
        }
        C::GoToSlide(i) => {
            if let Some(ed) = s.editor.as_mut() {
                ed.go_to(i);
                println!("AZSHOW_SLIDE {}", ed.current + 1);
            }
            s.view = crate::app::View::Normal;
            s.screen = Screen::Editor;
            return Update::RefreshDom;
        }
        C::Zoom(delta) => {
            let (w, h) = window_size(info);
            let now = s.zoom.unwrap_or_else(|| s.zoom_percent(w, h));
            s.zoom = Some((now + delta as f32).clamp(10.0, 400.0));
            return Update::RefreshDom;
        }
        C::ZoomFit => {
            s.zoom = None;
            return Update::RefreshDom;
        }
        C::ToggleNotes => {
            s.show_notes = !s.show_notes;
            return Update::RefreshDom;
        }
        C::Picture => {
            let _request = FileDialog::open_file(
                AzString::from("Insert Picture"),
                OptionString::None,
                picture_filter(),
                app.clone(),
                on_picture_picked,
            );
            return Update::DoNothing;
        }
        C::Bold => return format(s, info, FormatAxis::Bold),
        C::Italic => return format(s, info, FormatAxis::Italic),
        C::Underline => return format(s, info, FormatAxis::Underline),
        C::Strike => return format(s, info, FormatAxis::Strike),
        _ => {}
    }

    let Some(ed) = s.editor.as_mut() else {
        return Update::DoNothing;
    };
    let editing = ed.editing.is_some();
    match cmd {
        C::Undo => {
            ed.stop_editing();
            ed.undo();
        }
        C::Redo => {
            ed.stop_editing();
            ed.redo();
        }
        // While a text is edited the engine owns the clipboard keys.
        C::Cut if !editing => ed.cut(),
        C::Copy if !editing => ed.copy(),
        C::Paste if !editing => ed.paste(),
        C::Duplicate if !editing => ed.duplicate(),
        C::Delete if !editing => ed.delete_selection(),
        C::SelectAll if !editing => {
            let all: Vec<usize> = (0..ed.slide().elements.len()).collect();
            ed.select_indices(&all);
        }
        C::Cut | C::Copy | C::Paste | C::Duplicate | C::Delete | C::SelectAll => return Update::DoNothing,
        C::NewSlide(layout) => {
            ed.new_slide(layout);
            println!("AZSHOW_SLIDES {} {}", ed.deck.slides.len(), ed.current + 1);
        }
        C::Layout(layout) => ed.apply_layout(layout),
        C::ResetSlide => ed.reset_slide(),
        C::AddSection => {
            let n = ed.deck.slides.iter().filter(|sl| sl.section.is_some()).count() + 1;
            ed.add_section(&format!("Section {n}"));
        }
        C::ToggleHidden => ed.toggle_hidden(),
        C::DuplicateSlides => ed.duplicate_slides(),
        C::DeleteSlides => ed.delete_slides(),
        C::Align(a) => ed.set_align(a),
        C::Bullets => ed.toggle_bullets(),
        C::Indent(d) => ed.indent(d),
        C::Grow(d) => ed.grow_text(d as f32 * 4.0),
        C::TextColor(c) => ed.set_text_color(c),
        C::Font(f) => ed.set_font(f),
        C::Shape(kind) => {
            ed.insert_shape(kind);
        }
        C::TextBox => {
            let id = ed.insert_text_box();
            s.focus_text = Some(id);
            crate::focus_text_soon(info, app);
        }
        C::Table => {
            ed.insert_table(3, 3);
        }
        C::Chart(kind) => {
            ed.insert_chart(kind);
        }
        C::Video => {
            ed.insert_video("");
        }
        C::Arrange(how) => ed.arrange(how),
        C::Group => ed.group(),
        C::Ungroup => ed.ungroup(),
        C::Fill(c) => ed.set_fill(c),
        C::Outline(c) => ed.set_outline(c, if c.is_some() { 4.0 } else { 0.0 }),
        C::Theme(i) => {
            let fonts = FontScheme::all()
                .iter()
                .position(|f| *f == ed.deck.theme.fonts)
                .unwrap_or(0);
            ed.set_theme(themes::theme(i, 0, fonts));
        }
        C::Variant(v) => {
            let fonts = FontScheme::all()
                .iter()
                .position(|f| *f == ed.deck.theme.fonts)
                .unwrap_or(0);
            let index = themes::index_of(&ed.deck.theme);
            ed.set_theme(themes::theme(index, v, fonts));
        }
        C::FontScheme(f) => {
            let mut theme = ed.deck.theme.clone();
            if let Some(scheme) = FontScheme::all().get(f) {
                theme.fonts = scheme.clone();
                ed.set_theme(theme);
            }
        }
        C::SlideSize(size) => ed.set_size(size),
        C::Background(bg, all) => ed.set_background(bg, all),
        C::Transition(kind) => ed.set_transition(kind),
        C::TransitionDuration(d) => ed.transition_duration(d),
        C::TransitionToAll => ed.transition_to_all(),
        C::Animation(effect) => ed.set_animation(effect),
        C::MoveAnimation(d) => ed.move_animation(d),
        _ => return Update::DoNothing,
    }
    Update::RefreshDom
}
