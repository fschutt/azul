//! What the commands do, and the answers of the worker threads.
//!
//! Every file is read and written on a `Thread` ([`crate::jobs`], azul-appkit's file jobs for
//! `state.json`); every chapter is laid out on one ([`crate::paginate`]). A callback only
//! changes the state and asks.
//!
//! On stdout, for scripts (`scripts/azreader_e2e.py`): `AZREADER_LISTED <n>`,
//! `AZREADER_IMPORTED <id>`, `AZREADER_OPENED <id> <chapters>`, `AZREADER_PAGES <chapter>
//! <pages>`, `AZREADER_PAGE <chapter> <page>`, `AZREADER_BOOKMARKS <n>`, `AZREADER_SAVED`.

use std::path::PathBuf;

use azul::{
    callbacks::{CallbackInfo, RefAny, Update},
    dialog::{FileDialog, FileOpenResult},
    file::FileTypeList,
    option::{OptionFileTypeList, OptionString},
    str::String as AzString,
};
use azul_appkit::{ui as kit, FileJob};

use crate::{
    app::{AppState, Command, CommandRef, OpenBook, Pane, Screen, Target},
    jobs::{self, Done, Job},
    library::{self, Bookmark},
    paginate::{self, ChapterReady, ChapterRequest},
    position::{self, Turn},
    settings::{FONT_MAX, FONT_MIN, LINE_MAX, LINE_MIN, MARGIN_MAX, MARGIN_MIN},
};

/// The write-back tag of a `state.json` save.
const TAG_STATE: u64 = 1;

/// A button's click: the command it carries.
pub extern "C" fn on_command(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, cmd)) = data
        .downcast_ref::<CommandRef>()
        .map(|c| (c.app.clone(), c.cmd.clone()))
    else {
        return Update::DoNothing;
    };
    run(&mut app, cmd, &mut info)
}

/// Runs `cmd` on the app.
pub fn run(app: &mut RefAny, cmd: Command, info: &mut CallbackInfo) -> Update {
    let handle = app.clone();
    let Some(mut guard) = app.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    let st = &mut *guard;
    match cmd {
        Command::RibbonTab(index) => st.ribbon_tab = index,
        Command::ShowLibrary => {
            st.screen = Screen::Library;
            st.ribbon_tab = 0;
            scan_library(st, info, &handle);
        }
        Command::ShowReader => {
            if st.open.is_some() {
                st.screen = Screen::Reader;
                st.ribbon_tab = 0;
            }
        }
        Command::Shelf(shelf) => st.shelf = shelf,
        Command::Sort(sort) => st.sort = sort,
        Command::Select(id) => st.selected = Some(id),
        Command::OpenBook(id) => open_book(st, info, &handle, &id),
        Command::RemoveBook(id) => {
            if st.open.as_ref().is_some_and(|o| o.info.id == id) {
                close_book(st, info);
            }
            st.entries.retain(|e| e.info.id != id);
            jobs::spawn(
                info,
                &handle,
                &st.data_root,
                Job::Delete { id },
                on_job_done,
            );
        }
        Command::AddBooks => {
            let _request = FileDialog::open_file(
                "Add a book",
                OptionString::None,
                book_filter(),
                handle.clone(),
                on_book_picked,
            );
            return Update::DoNothing;
        }
        Command::NextPage => return turn_page(st, info, &handle, true),
        Command::PrevPage => return turn_page(st, info, &handle, false),
        Command::BookStart => go_to(st, info, &handle, 0, Target::Fraction(0.0)),
        Command::GoToEntry(index) => {
            let entry = st
                .open
                .as_ref()
                .and_then(|o| o.book.toc.get(index).cloned());
            if let Some((chapter, entry)) = entry.and_then(|e| Some((e.chapter?, e))) {
                let target = if entry.fragment.is_empty() {
                    Target::Fraction(0.0)
                } else {
                    Target::Anchor(entry.fragment.clone())
                };
                go_to(st, info, &handle, chapter, target);
            }
        }
        Command::GoToBookmark(index) => {
            let mark = st
                .open
                .as_ref()
                .and_then(|o| o.state.bookmarks.get(index).cloned());
            if let Some(mark) = mark {
                go_to(
                    st,
                    info,
                    &handle,
                    mark.position.chapter,
                    Target::Fraction(mark.position.fraction),
                );
            }
        }
        Command::ToggleBookmark => toggle_bookmark(st, info, &handle),
        Command::RemoveBookmark(index) => {
            let removed = match st.open.as_mut() {
                Some(open) => {
                    let id = open.state.bookmarks.get(index).map(|b| b.id.clone());
                    let removed = id.is_some_and(|id| open.state.remove_bookmark(&id));
                    println!("AZREADER_BOOKMARKS {}", open.state.bookmarks.len());
                    removed
                }
                None => false,
            };
            if removed {
                save_state(st, info, &handle);
            }
        }
        Command::Pane(pane) => {
            st.pane = if st.pane == pane { Pane::None } else { pane };
            // The reading area changes: measured again after the next frame.
            st.area = None;
        }
        Command::FontSize(step) => {
            st.settings.font_px = step_in(st.settings.font_px, step, FONT_MIN, FONT_MAX);
            settings_changed(st, info, &handle);
        }
        Command::LineSpacing(step) => {
            st.settings.line_tenths = step_in(st.settings.line_tenths, step, LINE_MIN, LINE_MAX);
            settings_changed(st, info, &handle);
        }
        Command::Margins(step) => {
            st.settings.margin_px = step_in(st.settings.margin_px, step, MARGIN_MIN, MARGIN_MAX);
            settings_changed(st, info, &handle);
        }
        Command::Paper(paper) => {
            st.settings.paper = paper;
            settings_changed(st, info, &handle);
        }
        Command::Font(font) => {
            st.settings.font = font;
            settings_changed(st, info, &handle);
        }
        Command::Layout(layout) => {
            st.settings.layout = layout;
            settings_changed(st, info, &handle);
        }
        Command::Justify(on) => {
            st.settings.justify = on;
            settings_changed(st, info, &handle);
        }
        Command::Settings => kit::open_settings(&st.kit, Some("Reading")),
        Command::About => st.about_open = true,
    }
    Update::RefreshDom
}

/// `value` moved by `step`, kept in `min..=max`.
fn step_in(value: u32, step: i32, min: u32, max: u32) -> u32 {
    let moved = i64::from(value) + i64::from(step);
    u32::try_from(moved.clamp(i64::from(min), i64::from(max))).unwrap_or(min)
}

/// The books the file dialog offers.
fn book_filter() -> OptionFileTypeList {
    OptionFileTypeList::Some(FileTypeList {
        document_types: vec![
            AzString::from("*.epub"),
            AzString::from("*.txt"),
            AzString::from("*.html"),
            AzString::from("*.htm"),
        ]
        .into(),
        document_descriptor: AzString::from("Books (EPUB, text, HTML)"),
    })
}

extern "C" fn on_book_picked(mut data: RefAny, mut info: CallbackInfo, result: RefAny) -> Update {
    let Some(picked) = FileOpenResult::downcast(result).into_option() else {
        return Update::DoNothing;
    };
    let Some(path) = picked.path.into_option() else {
        return Update::DoNothing;
    };
    let path = PathBuf::from(path.as_string().as_str());
    let handle = data.clone();
    let Some(mut guard) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    let st = &mut *guard;
    import_files(st, &mut info, &handle, vec![path]);
    Update::RefreshDom
}

/// Reads the library (its books, their states and covers).
pub fn scan_library(st: &mut AppState, info: &mut CallbackInfo, app: &RefAny) {
    jobs::spawn(info, app, &st.data_root, Job::Scan, on_job_done);
}

/// Adds `paths` to the library (each its own job).
pub fn import_files(st: &mut AppState, info: &mut CallbackInfo, app: &RefAny, paths: Vec<PathBuf>) {
    for path in paths {
        st.notice = format!("Adding {}...", path.display());
        jobs::spawn(info, app, &st.data_root, Job::Import { path }, on_job_done);
    }
}

/// Opens book `id` of the library.
pub fn open_book(st: &mut AppState, info: &mut CallbackInfo, app: &RefAny, id: &str) {
    if st.open.as_ref().is_some_and(|o| o.info.id == id) {
        st.screen = Screen::Reader;
        return;
    }
    let Some(entry) = st.entries.iter().find(|e| e.info.id == id) else {
        return;
    };
    let job = Job::Open {
        id: id.to_string(),
        format: entry.info.format,
        title: entry.info.title.clone(),
    };
    st.opening = Some(id.to_string());
    st.notice = format!("Opening {}...", entry.info.title);
    jobs::spawn(info, app, &st.data_root, job, on_job_done);
}

/// Closes the open book (its state is saved on every move already).
fn close_book(st: &mut AppState, info: &mut CallbackInfo) {
    unregister_images(st, info, &[]);
    st.open = None;
    st.chapter = None;
    st.pending = None;
    st.screen = Screen::Library;
}

/// The page of `chapter` a target lands on.
fn page_for(chapter: &ChapterReady, target: &Target) -> usize {
    match target {
        Target::Fraction(f) => chapter.pages.page_of_fraction(*f),
        Target::Anchor(id) => chapter
            .content
            .anchor_fraction(id)
            .map_or(0, |f| chapter.pages.page_of_fraction(f)),
        Target::LastPage => chapter.pages.page_count() - 1,
    }
}

/// Goes to `target` in `chapter`: at once when the chapter is laid out for the current
/// settings, else after its layout.
pub fn go_to(
    st: &mut AppState,
    info: &mut CallbackInfo,
    app: &RefAny,
    chapter: usize,
    target: Target,
) {
    let Some(chapters) = st.open.as_ref().map(|o| o.book.spine.len()) else {
        return;
    };
    let chapter = chapter.min(chapters.saturating_sub(1));
    let key = st.layout_key();
    let per_view = st.geometry().per_view;
    let book_id = st
        .open
        .as_ref()
        .map(|o| o.info.id.clone())
        .unwrap_or_default();
    let laid_out = st
        .chapter
        .as_ref()
        .filter(|c| c.book_id == book_id && c.chapter == chapter && c.layout_key == key)
        .map(|c| page_for(c, &target));
    match laid_out {
        Some(page) => {
            st.page = position::view_start(page, per_view);
            moved(st, info, app);
        }
        None => request_chapter(st, info, app, chapter, target),
    }
}

/// Lays chapter `chapter` of the open book out (on a Thread) to open at `target`.
fn request_chapter(
    st: &mut AppState,
    info: &mut CallbackInfo,
    app: &RefAny,
    chapter: usize,
    target: Target,
) {
    let Some(fonts) = st.fonts.clone() else {
        // Before the first frame: laid out when the fonts are known (`ensure_layout`).
        st.pending = Some((chapter, 0, target));
        return;
    };
    let geometry = st.geometry();
    let Some((book_id, container, item)) = st.open.as_ref().and_then(|o| {
        Some((
            o.info.id.clone(),
            o.container.clone(),
            o.book.spine.get(chapter)?.clone(),
        ))
    }) else {
        return;
    };
    st.generation += 1;
    let request = ChapterRequest {
        book_id,
        chapter,
        generation: st.generation,
        container,
        html: item.is_html(),
        path: item.path,
        settings: st.settings,
        text_width: geometry.text_width,
        text_height: geometry.text_height,
        fonts,
    };
    st.pending = Some((chapter, st.generation, target));
    paginate::spawn_chapter(info, app, request, on_chapter_ready);
}

/// Called after every frame's measuring and every settings change: lays the chapter on
/// screen out again when the settings or the page size changed, or lays out the one a
/// command asked for before the fonts were known.
pub fn ensure_layout(st: &mut AppState, info: &mut CallbackInfo, app: &RefAny) {
    if st.open.is_none() || st.fonts.is_none() {
        return;
    }
    if let Some((chapter, generation, target)) = st.pending.clone() {
        if generation == 0 {
            request_chapter(st, info, app, chapter, target);
        }
        return;
    }
    let key = st.layout_key();
    let stale = match st.chapter.as_ref() {
        Some(c) => (c.layout_key != key).then(|| (c.chapter, c.pages.fraction_of_page(st.page))),
        None => st
            .open
            .as_ref()
            .map(|o| (o.state.position.chapter, o.state.position.fraction)),
    };
    if let Some((chapter, fraction)) = stale {
        request_chapter(st, info, app, chapter, Target::Fraction(fraction));
    }
}

/// A page turn.
fn turn_page(st: &mut AppState, info: &mut CallbackInfo, app: &RefAny, forward: bool) -> Update {
    let Some(chapters) = st.open.as_ref().map(|o| o.book.spine.len()) else {
        return Update::DoNothing;
    };
    let Some((chapter, page_count)) = st
        .chapter
        .as_ref()
        .map(|c| (c.chapter, c.pages.page_count()))
    else {
        return Update::DoNothing;
    };
    let per_view = st.geometry().per_view;
    match position::turn(st.page, per_view, page_count, chapter, chapters, forward) {
        Turn::Page(page) => {
            st.page = page;
            moved(st, info, app);
        }
        Turn::NextChapter => go_to(st, info, app, chapter + 1, Target::Fraction(0.0)),
        Turn::PreviousChapterEnd => {
            go_to(st, info, app, chapter.saturating_sub(1), Target::LastPage)
        }
        Turn::AtEnd => {
            let newly = st.open.as_mut().is_some_and(|open| {
                let newly = !open.state.finished;
                open.state.finished = true;
                newly
            });
            if newly {
                save_state(st, info, app);
            }
            return Update::DoNothing;
        }
        Turn::AtStart => return Update::DoNothing,
    }
    Update::RefreshDom
}

/// The reader moved: the position, the progress and the time into the book's state, saved.
fn moved(st: &mut AppState, info: &mut CallbackInfo, app: &RefAny) {
    let (Some(position), progress) = (st.position(), st.progress()) else {
        return;
    };
    if let Some(chapter) = st.chapter.as_ref() {
        println!("AZREADER_PAGE {} {}", chapter.chapter, st.page);
    }
    if let Some(open) = st.open.as_mut() {
        open.state.position = position;
        open.state.progress = progress;
        open.state.last_read = library::now_secs();
    }
    save_state(st, info, app);
}

/// Writes the open book's `state.json` (on a Thread) and its library entry.
pub fn save_state(st: &mut AppState, info: &mut CallbackInfo, app: &RefAny) {
    let Some(open) = st.open.as_ref() else {
        return;
    };
    let id = open.info.id.clone();
    let state = open.state.clone();
    if let Some(entry) = st.entries.iter_mut().find(|e| e.info.id == id) {
        entry.state = state.clone();
    }
    let job = FileJob::Put {
        key: library::state_key(&id),
        bytes: jobs::state_json(&state).into_bytes(),
    };
    kit::spawn_file_jobs(
        info,
        &st.data_root,
        vec![job],
        app.clone(),
        TAG_STATE,
        on_state_saved,
    );
}

extern "C" fn on_state_saved(mut app: RefAny, mut msg: RefAny, _info: CallbackInfo) -> Update {
    let Some(reply) = kit::take_reply(&mut msg) else {
        return Update::DoNothing;
    };
    let Some(mut st) = app.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    match reply
        .outcomes
        .iter()
        .find_map(azul_appkit::FileOutcome::error)
    {
        Some(e) => {
            st.notice = format!("The reading position could not be saved: {e}");
            Update::RefreshDom
        }
        None => {
            println!("AZREADER_SAVED");
            Update::DoNothing
        }
    }
}

/// The bookmark button: a bookmark at the page on screen, or - when there is one - none.
fn toggle_bookmark(st: &mut AppState, info: &mut CallbackInfo, app: &RefAny) {
    let Some(position) = st.position() else {
        return;
    };
    let label = st
        .chapter
        .as_ref()
        .map(|c| c.content.excerpt(position.fraction, 80))
        .unwrap_or_default();
    {
        let Some(open) = st.open.as_mut() else {
            return;
        };
        let mark = Bookmark {
            id: azul_storage::ids::new_uuid(),
            position,
            label,
            chapter_title: open.book.chapter_title(position.chapter),
            created: library::now_secs(),
        };
        open.state.toggle_bookmark(mark);
        println!("AZREADER_BOOKMARKS {}", open.state.bookmarks.len());
    }
    save_state(st, info, app);
}

/// A reading setting changed: kept in the settings file, the pages laid out again.
fn settings_changed(st: &mut AppState, info: &mut CallbackInfo, app: &RefAny) {
    {
        let mut kit_ref = st.kit.clone();
        let kit = kit_ref.downcast_mut::<kit::Kit>();
        if let Some(mut k) = kit {
            st.settings.write_to(&mut k.settings);
        }
    }
    kit::save_settings(&st.kit, info);
    ensure_layout(st, info, app);
}

/// Removes the open chapter's pictures from the image cache, but those in `keep`.
fn unregister_images(st: &mut AppState, info: &mut CallbackInfo, keep: &[String]) {
    let old = std::mem::take(&mut st.registered_images);
    for src in old {
        if keep.contains(&src) {
            st.registered_images.push(src);
        } else {
            info.remove_image_from_cache(src.as_str());
        }
    }
}

/// A chapter is laid out: on screen when it is the one asked for last.
pub extern "C" fn on_chapter_ready(
    mut app: RefAny,
    mut msg: RefAny,
    mut info: CallbackInfo,
) -> Update {
    let Some(mut ready) = paginate::take_ready(&mut msg) else {
        return Update::DoNothing;
    };
    let handle = app.clone();
    let Some(mut guard) = app.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    let st = &mut *guard;
    let Some((_, generation, target)) = st.pending.clone() else {
        return Update::DoNothing;
    };
    let same_book = st.open.as_ref().is_some_and(|o| o.info.id == ready.book_id);
    if generation != ready.generation || !same_book {
        return Update::DoNothing;
    }
    st.pending = None;
    // The pictures: the new chapter's in, the old chapter's out.
    let images = std::mem::take(&mut ready.images);
    let names: Vec<String> = images.iter().map(|(src, _)| src.clone()).collect();
    unregister_images(st, &mut info, &names);
    for (src, image) in images {
        if !st.registered_images.contains(&src) {
            info.add_image_to_cache(src.as_str(), image);
            st.registered_images.push(src);
        }
    }
    println!(
        "AZREADER_PAGES {} {}",
        ready.chapter,
        ready.pages.page_count()
    );
    let page = page_for(&ready, &target);
    let per_view = st.geometry().per_view;
    st.page = position::view_start(page, per_view);
    st.chapter = Some(ready);
    moved(st, &mut info, &handle);
    // The settings or the area may have changed while it was laid out.
    ensure_layout(st, &mut info, &handle);
    Update::RefreshDom
}

/// The answers of the library jobs.
pub extern "C" fn on_job_done(mut app: RefAny, mut msg: RefAny, mut info: CallbackInfo) -> Update {
    let Some(done) = jobs::take_done(&mut msg) else {
        return Update::DoNothing;
    };
    let handle = app.clone();
    let Some(mut guard) = app.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    let st = &mut *guard;
    match done {
        Done::Library {
            entries,
            covers,
            errors,
        } => {
            st.entries = entries;
            for (id, cover) in covers {
                st.covers.insert(id, cover);
            }
            st.listed = true;
            if let Some(e) = errors.first() {
                st.notice = format!("Some books could not be read: {e}");
            }
            println!("AZREADER_LISTED {}", st.entries.len());
            if st.sample && !st.sample_done && st.entries.is_empty() {
                st.sample_done = true;
                let job = Job::ImportBytes {
                    name: crate::sample::FILE_NAME.to_string(),
                    bytes: crate::sample::sample_epub(),
                };
                jobs::spawn(&mut info, &handle, &st.data_root, job, on_job_done);
            }
        }
        Done::Imported { result, cover } => match result {
            Ok(entry) => {
                let id = entry.info.id.clone();
                println!("AZREADER_IMPORTED {id}");
                st.notice = format!("Added {}", entry.info.title);
                if let Some(cover) = cover {
                    st.covers.insert(id.clone(), cover);
                }
                st.entries.retain(|e| e.info.id != id);
                st.entries.push(entry);
                st.selected = Some(id.clone());
                if st.open.is_none() && (st.sample_done || !st.import_on_start.is_empty()) {
                    st.import_on_start.clear();
                    open_book(st, &mut info, &handle, &id);
                }
            }
            Err(e) => st.notice = format!("The book could not be added: {e}"),
        },
        Done::Opened { id, result } => {
            if st.opening.as_deref() != Some(id.as_str()) {
                return Update::DoNothing;
            }
            st.opening = None;
            match result {
                Ok((container, book)) => {
                    let Some(entry) = st.entries.iter().find(|e| e.info.id == id).cloned() else {
                        return Update::DoNothing;
                    };
                    unregister_images(st, &mut info, &[]);
                    let weights = book.chapter_weights();
                    let position = entry.state.position.clamped(book.spine.len());
                    println!("AZREADER_OPENED {id} {}", book.spine.len());
                    st.open = Some(OpenBook {
                        info: entry.info,
                        state: entry.state,
                        container,
                        book,
                        weights,
                    });
                    st.chapter = None;
                    st.pending = None;
                    st.page = 0;
                    st.screen = Screen::Reader;
                    st.ribbon_tab = 0;
                    st.notice.clear();
                    request_chapter(
                        st,
                        &mut info,
                        &handle,
                        position.chapter,
                        Target::Fraction(position.fraction),
                    );
                }
                Err(e) => st.notice = format!("The book could not be opened: {e}"),
            }
        }
        Done::Deleted { id, result } => match result {
            Ok(()) => {
                st.covers.remove(&id);
                st.notice = "The book was removed.".to_string();
            }
            Err(e) => {
                st.notice = format!("The book could not be removed: {e}");
                scan_library(st, &mut info, &handle);
            }
        },
    }
    Update::RefreshDom
}
