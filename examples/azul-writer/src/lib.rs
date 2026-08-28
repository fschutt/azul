//! azwriter — an Office-2013-era-styled document editor on the azul GUI framework.
//!
//! UI ONLY: the document pipeline is stubbed behind the seams in
//! [`document`] (`load_markdown` / `save_markdown` / `paginate`); a
//! parallel workstream hooks the real markdown -> StyledDom -> PageSequence
//! pipeline into exactly those three functions.
//!
//! Screens:
//! - Editor: title band (quick-access toolbar), the Office-2013-era look ribbon (HOME tab
//!   clone), print-layout canvas with the paginated white sheet, status bar
//!   (page / words / language, view switcher, zoom slider).
//! - Backstage ("FILE"): dark-blue nav column with Info / Open panes per
//!   the the Office-2013-era look screenshots; back arrow and Esc return to the editor.
//!
//! Screenshot harness (headless verification):
//! `AZWRITER_SHOT=/path/out.png [AZWRITER_SCREEN=editor|backstage-info|
//! backstage-open] AZ_BACKEND=headless ./azwriter` renders the requested
//! screen, writes the PNG and exits.

mod backstage_ui;
mod document;
mod editor_ui;
mod fonts;
mod perf;
mod ribbon_ui;

use std::path::{Path, PathBuf};

use azul_core::{
    callbacks::{LayoutCallbackInfo, TimerCallbackReturn, Update},
    dom::{Dom, DomId},
    refany::RefAny,
    resources::AppConfig,
    task::{Duration, SystemTimeDiff, TerminateTimer, TimerId},
};
use azul_css::{AzString, OptionString, StringVec};
// The azul-dll package names its lib target `azul`; `unified::app::App` is
// the raw-typed desktop App (no generated wrappers involved).
use azul::unified::app::App;
use azul_layout::{
    callbacks::{Callback, CallbackInfo},
    desktop::dialogs::{FileDialog, FileTypeList, OptionFileTypeList},
    timer::{Timer, TimerCallback, TimerCallbackInfo},
    window_state::WindowCreateOptions,
};

use azul_layout::managers::changeset::{
    DocumentChangeset, DocumentOperation, EditResumePoint, NodePosition,
};

use crate::document::DocumentModel;

/// Diagnostic: log any layout() call slower than the frame budget. A client
/// that spends too long here cannot answer the compositor's configure/ping
/// handshake, and the surface gets dropped (AZWRITER_FRAME_LOG=1).
///
/// Reports the per-phase breakdown recorded by [`perf::Phase`], because the
/// total alone does not say whether the cost is pagination, the state clone
/// or the ribbon. `AZWRITER_FRAME_LOG=all` prints every frame.
struct FrameTimer(Option<std::time::Instant>);
impl FrameTimer {
    fn start() -> Self {
        Self((perf::mode() != perf::Mode::Off).then(std::time::Instant::now))
    }
}
impl Drop for FrameTimer {
    fn drop(&mut self) {
        let Some(t) = self.0 else { return };
        let d = t.elapsed();
        let n = perf::next_frame_number();
        let phases = perf::take_phases();
        let over_budget = d > std::time::Duration::from_millis(8);
        if !over_budget && perf::mode() != perf::Mode::All {
            return;
        }
        eprintln!("[frame #{n}] layout() took {d:?}");
        for (name, dur) in phases {
            eprintln!("[frame #{n}]   {name:<22} {dur:?}");
        }
    }
}

// ---------------------------------------------------------------------------
// Application state
// ---------------------------------------------------------------------------

/// Which screen fills the window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Editor,
    Backstage,
}

/// The whole UI state, held in one `RefAny` shared by every callback.
#[derive(Clone)]
pub struct AppState {
    pub screen: Screen,
    /// Active backstage nav item (0 = Info, 2 = Open, …).
    pub backstage_pane: usize,
    /// Active ribbon tab (0 = HOME).
    pub ribbon_tab: usize,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    /// 0 = left, 1 = center, 2 = right, 3 = justify.
    pub align: usize,
    /// Selected cell of the ribbon styles gallery.
    pub selected_style: usize,
    /// Active status-bar view (0 read mode, 1 print layout, 2 web layout).
    pub view_mode: usize,
    /// Zoom percent (status-bar cluster; the page sheet scales with it).
    pub zoom_percent: f32,
    /// Page whose sheet currently holds the caret (the edit loop maps
    /// page-relative changeset paths through this page's block offset).
    pub editing_page: usize,
    /// Structural undo history: each entry is an operation that reverses an
    /// applied edit, paired with the resume point the ENGINE said to replay
    /// it with (index resolution differs between split and merge, so the app
    /// must not compute that itself).
    pub undo_stack: Vec<(DocumentOperation, Vec<u32>)>,
    /// Operations undone and available to redo (cleared by a new edit).
    pub redo_stack: Vec<(DocumentOperation, Vec<u32>)>,
    pub document: DocumentModel,
    /// #28(c): EXACT page count from the background pagination thread —
    /// `Some((generation, count))` once its writeback lands for the current
    /// document generation; until then the UI shows the monitor-bounded
    /// estimate. Cleared implicitly by the generation pairing (a stale
    /// writeback is ignored).
    pub exact_page_count: Option<(u64, usize)>,
    /// #28(c): the pages `VirtualView` node, stored at `AfterMount` so the
    /// background writeback can address `update_virtual_view` (scrollbar
    /// correction without re-invoking the callback).
    pub pages_vv_node: Option<azul_core::dom::DomNodeId>,
    /// #28(c): the in-flight background pagination — `(generation it was
    /// started for, its azul ThreadId)`. `None` = nothing running. The
    /// generation guards staleness (edits supersede the run); the ThreadId
    /// lets the VirtualView's unmount callback CANCEL the decode via
    /// `CallbackInfo::remove_thread` when the document/app closes before
    /// loading finishes (USER design 2026-08-12). Cancellation today drops
    /// the writeback registration (no stale result can land); true
    /// mid-compute abort needs chunked pagination — a recorded refinement.
    pub pagination_thread: Option<(u64, azul_core::task::ThreadId)>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            screen: Screen::Editor,
            backstage_pane: 0,
            ribbon_tab: 0,
            bold: false,
            italic: false,
            underline: false,
            align: 0,
            selected_style: 0,
            view_mode: 1,
            zoom_percent: 100.0,
            editing_page: 0,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            document: DocumentModel::untitled(),
            exact_page_count: None,
            pages_vv_node: None,
            pagination_thread: None,
        }
    }
}

// ---------------------------------------------------------------------------
// #28(c): streaming background pagination
// ---------------------------------------------------------------------------

/// Blocks per background-pagination chunk. Each chunk produces one
/// writeback → one scrollbar/page-count update; the rate is however fast
/// chunks compute (USER: "updates at 60 fps or however fast the doc pages
/// load in"). Tune toward ~16ms/chunk later if needed.
const PAGINATION_CHUNK_BLOCKS: u32 = 64;

/// Init data moved INTO the pagination worker (markdown, not Dom — Dom is
/// not Send; the worker rebuilds content thread-side).
struct PaginationThreadInit {
    markdown: String,
    generation: u64,
    fonts: Option<rust_fontconfig::FcFontCache>,
}

/// One streamed chunk result (worker → main-thread writeback).
struct PaginationChunk {
    generation: u64,
    /// Total pages discovered so far (monotone; the final chunk's value is
    /// the exact count — chunk seams force a page break, matching the
    /// seeded seam paths, so displayed pagination and count agree).
    pages_so_far: usize,
    /// ABSOLUTE break paths so far (chunk-relative first components offset
    /// by the chunk's block start; seams appear as `[next_chunk_start]`).
    paths_so_far: Vec<Vec<u32>>,
    done: bool,
}

/// #28(c): the chunk loop. Checks for `TerminateThread` BETWEEN chunks, so
/// the unmount callback's `remove_thread` actually aborts remaining work
/// (USER design).
extern "C" fn pagination_worker(
    mut init: RefAny,
    mut sender: azul_layout::thread::ThreadSender,
    mut recv: azul_core::task::ThreadReceiver,
) {
    use azul_core::task::{OptionThreadSendMsg, ThreadSendMsg};
    use azul_layout::thread::{ThreadReceiveMsg, ThreadWriteBackMsg};

    let (markdown, generation, fonts) = {
        let Some(init) = init.downcast_ref::<PaginationThreadInit>() else {
            return;
        };
        (init.markdown.clone(), init.generation, init.fonts.clone())
    };

    let total_blocks = document::markdown_block_count(&markdown) as u32;
    let content = document::markdown_to_content_dom(&markdown);

    let mut remaining = content;
    let mut block_offset: u32 = 0;
    let mut paths_acc: Vec<Vec<u32>> = Vec::new();
    let mut pages_acc: usize = 0;

    loop {
        // Cancellation point (unmount / app close mid-load).
        if matches!(
            recv.recv(),
            OptionThreadSendMsg::Some(ThreadSendMsg::TerminateThread)
        ) {
            return;
        }

        let (chunk, tail) =
            azul_layout::document_edit::split_dom_at_path(&remaining, &[PAGINATION_CHUNK_BLOCKS]);
        let rel = document::break_paths_for(&chunk, fonts.clone());
        for p in &rel {
            let mut abs = p.clone();
            if let Some(first) = abs.first_mut() {
                *first += block_offset;
            }
            paths_acc.push(abs);
        }
        pages_acc += rel.len() + 1;

        block_offset += PAGINATION_CHUNK_BLOCKS;
        let done = block_offset >= total_blocks;
        if !done {
            // Seam: the next chunk starts a fresh page — the displayed
            // pagination and the count agree by construction. (Exact-resume
            // via PageSequence first-page leftover height is the recorded
            // refinement.)
            paths_acc.push(vec![block_offset]);
        }

        let sent = sender.send(ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg {
            refany: RefAny::new(PaginationChunk {
                generation,
                pages_so_far: pages_acc,
                paths_so_far: paths_acc.clone(),
                done,
            }),
            callback: azul_layout::thread::WriteBackCallback {
                cb: pagination_writeback,
                ctx: azul_core::refany::OptionRefAny::None,
            },
        }));
        if !sent || done {
            return;
        }
        remaining = tail;
    }
}

/// #28(c): main-thread landing of one chunk — seeds the break-path memo,
/// updates the displayed count and corrects the VirtualView's scrollbar via
/// `update_virtual_view` (no callback re-invoke). Stale generations cancel
/// the run.
extern "C" fn pagination_writeback(
    mut app: RefAny,
    mut msg: RefAny,
    mut info: CallbackInfo,
) -> Update {
    let (generation, pages, paths, done) = {
        let Some(chunk) = msg.downcast_ref::<PaginationChunk>() else {
            return Update::DoNothing;
        };
        (
            chunk.generation,
            chunk.pages_so_far,
            chunk.paths_so_far.clone(),
            chunk.done,
        )
    };

    let (vv_node, zoom) = {
        let Some(mut state) = app.downcast_mut::<AppState>() else {
            return Update::DoNothing;
        };
        if generation != state.document.generation {
            // The document changed under the run — cancel it (a fresh mount
            // spawn covers the new generation).
            if let Some((g, tid)) = state.pagination_thread.take() {
                if g == generation {
                    info.remove_thread(tid);
                } else {
                    state.pagination_thread = Some((g, tid));
                }
            }
            return Update::DoNothing;
        }
        document::seed_break_paths(generation, paths, done);
        state.exact_page_count = Some((generation, pages));
        if done {
            state.pagination_thread = None;
        }
        (state.pages_vv_node, state.zoom_percent / 100.0)
    };

    // Scrollbar correction: virtual size grows → the bar shrinks live while
    // the scroll POSITION stays where it was (the op clamps, not resets).
    if let Some(vv) = vv_node {
        let stride = editor_ui::page_stride(zoom);
        let width = (editor_ui::page_sheet_w() * zoom).round() + 2.0;
        // `materialized: None` = keep the rendered window exactly as it is.
        // Only the document estimate changes, and placement never reads it —
        // so the bar re-scales and not one pixel of the page moves.
        info.update_virtual_view(
            vv,
            azul_core::geom::OptionLogicalRect::None,
            azul_core::geom::OptionLogicalRect::Some(azul_core::geom::LogicalRect::new(
                azul_core::geom::LogicalPosition::zero(),
                azul_core::geom::LogicalSize::new(width, pages as f32 * stride),
            )),
        );
    }

    // Status bar page count ticks as chunks land.
    Update::RefreshDom
}

/// #28(c): pages VirtualView mounted — remember its node (the writeback
/// addresses it) and spawn the streaming pagination unless the document is
/// already fully paginated or a run for this generation is in flight.
pub extern "C" fn on_pages_mounted(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let (app, fonts) = {
        let Some(ctx) = data.downcast_ref::<editor_ui::PagesMountCtx>() else {
            return Update::DoNothing;
        };
        (ctx.app.clone(), ctx.fonts.clone())
    };
    let mut app = app;
    let app_for_thread = app.clone();
    let Some(mut state) = app.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    state.pages_vv_node = Some(info.get_hit_node());

    let generation = state.document.generation;
    if document::pagination_is_complete(generation) {
        return Update::DoNothing;
    }
    if matches!(state.pagination_thread, Some((g, _)) if g == generation) {
        return Update::DoNothing;
    }
    // Supersede an older-generation run.
    if let Some((_, old)) = state.pagination_thread.take() {
        info.remove_thread(old);
    }

    let init = RefAny::new(PaginationThreadInit {
        markdown: state.document.markdown.clone(),
        generation,
        fonts,
    });
    let thread = azul_layout::thread::Thread::create(
        init,
        app_for_thread,
        azul_layout::thread::ThreadCallback::new(pagination_worker),
    );
    let thread_id = azul_core::task::ThreadId::unique();
    info.add_thread(thread_id.clone(), thread);
    state.pagination_thread = Some((generation, thread_id));
    Update::DoNothing
}

/// #28(c): pages VirtualView unmounted (doc/app closing) — CANCEL the
/// in-flight pagination (USER design: stop decoding pages the moment nobody
/// can see them).
pub extern "C" fn on_pages_unmounted(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let app = {
        let Some(ctx) = data.downcast_ref::<editor_ui::PagesMountCtx>() else {
            return Update::DoNothing;
        };
        ctx.app.clone()
    };
    let mut app = app;
    let Some(mut state) = app.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    if let Some((_, tid)) = state.pagination_thread.take() {
        info.remove_thread(tid);
    }
    state.pages_vv_node = None;
    Update::DoNothing
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// Sets the native window title to "<name> - AzWriter".
fn set_window_title(info: &mut CallbackInfo, name: &str) {
    let mut st = info.get_current_window_state().clone();
    st.title = AzString::from(format!("{name} - AzWriter"));
    info.modify_window_state(st);
}

/// The `*.md` filter for the native open dialog.
fn markdown_filter() -> OptionFileTypeList {
    OptionFileTypeList::Some(FileTypeList {
        document_types: StringVec::from_vec(vec![AzString::from("*.md")]),
        document_descriptor: AzString::from("Markdown documents (*.md)"),
    })
}

/// Save flow shared by the quick-access save button and the backstage
/// Save / Save As entries. Asks for a path when there is none (or when
/// `always_ask`), then runs the `document::save_markdown` seam.
fn do_save(data: &mut RefAny, info: &mut CallbackInfo, always_ask: bool) -> Update {
    let (current_path, model_snapshot) = {
        let Some(state) = data.downcast_ref::<AppState>() else {
            return Update::DoNothing;
        };
        let mut snapshot = state.document.clone();
        // LIVE text: read each block's on-screen text back through the
        // engine (`get_node_text_content` sees the text overlay, so typed
        // edits — which are not structural and therefore fire no
        // DocumentEdit — reach the saved markdown). Falls back to the
        // model's own text for anything not currently rendered.
        let mut provider = |path: &[u32]| -> Option<String> {
            let id = document::path_dom_id(path)?;
            let node = info.get_node_id_by_id_attribute(DomId::ROOT_ID, &id)?;
            info.get_node_text_content(azul_core::dom::DomNodeId {
                dom: DomId::ROOT_ID,
                node: azul_core::styled_dom::NodeHierarchyItemId::from_crate_internal(Some(node)),
            })
        };
        snapshot.markdown = document::dom_to_markdown(&snapshot.content, &mut provider);
        (state.document.path.clone(), snapshot)
    };

    let target: Option<PathBuf> = if current_path.is_none() || always_ask {
        // Native save dialog (tinyfiledialogs). Blocks; fine for a shell.
        match FileDialog::save_file(
            AzString::from("Save As \u{2014} .md for markdown, .pdf to export"),
            OptionString::None,
        )
        .into_option()
        {
            Some(p) => {
                let mut path = PathBuf::from(p.as_str());
                if path.extension().is_none() {
                    path.set_extension("md");
                }
                Some(path)
            }
            None => None, // user cancelled
        }
    } else {
        current_path
    };

    let Some(path) = target else {
        return Update::DoNothing;
    };

    // Save writes the format the FILENAME asks for. Before this, Save always
    // wrote markdown and forced a .md extension, so typing "report.pdf" in the
    // dialog produced a markdown file called report.pdf — the dialog appeared,
    // something was written, and it was not a PDF. Export-PDF lives in the
    // backstage, which is not where anyone looks first.
    if path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("pdf"))
    {
        let bytes = pdf_bytes(&model_snapshot.content, info);
        if bytes.is_empty() {
            eprintln!("[azwriter] PDF export produced no bytes");
            return Update::DoNothing;
        }
        return match std::fs::write(&path, &bytes) {
            Ok(()) => {
                eprintln!(
                    "[azwriter] exported {} bytes to {}",
                    bytes.len(),
                    path.display()
                );
                Update::RefreshDom
            }
            Err(e) => {
                eprintln!("[azwriter] PDF write failed: {e}");
                Update::DoNothing
            }
        };
    }

    match document::save_markdown(&path, &model_snapshot) {
        Ok(()) => {
            let Some(mut state) = data.downcast_mut::<AppState>() else {
                return Update::DoNothing;
            };
            state.document.path = Some(path);
            state.document.markdown = model_snapshot.markdown.clone();
            state.document.dirty = false;
            let name = state.document.display_name();
            drop(state);
            set_window_title(info, &name);
            Update::RefreshDom
        }
        Err(e) => {
            eprintln!("[azwriter] save failed: {e}");
            Update::DoNothing
        }
    }
}

// ---------------------------------------------------------------------------
// Callbacks (referenced from the ui modules)
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// C11 editing loop: DocumentEdit -> apply to the model -> ack -> refresh
// ---------------------------------------------------------------------------

/// The engine recorded a structural edit on a page subtree. Apply it to the
/// app's MODEL (`state.document.content`, which the pages are cut from),
/// acknowledge with the inverse (making it undoable), and re-render — the
/// Path-2 loop from `azul_layout::document_edit`.
///
/// Page->model mapping: pages are contiguous slices of the model's block
/// list, so a changeset recorded against page `p` shifts by that page's
/// block offset (`document::page_block_offsets`).
pub extern "C" fn on_document_edit(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let changeset = match info.get_document_edit_clone().into_option() {
        Some(c) => c,
        None => return Update::DoNothing,
    };

    let Some(mut state) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };

    // Which page hosts the edit? The engine addresses the CURRENT DOM; the
    // canvas renders one sheet per page in order, so the page index is the
    // edit's position among the rendered pages. Re-derive the offsets from
    // the same pagination the canvas used.
    let pages = document::paginate_cached(&state.document.content, state.document.generation);
    let offsets = document::page_block_offsets(&pages);
    let page_index = state.editing_page.min(offsets.len().saturating_sub(1));
    let page_offset = offsets.get(page_index).copied().unwrap_or(0);

    // The operation edits the child list of the model ROOT (blocks live
    // there); the mapped host path is [] shifted by nothing, while the
    // resume path inside the changeset already names the block index.
    let host_path = document::page_path_to_model_path(page_offset, &[]);

    let applied = match azul_layout::document_edit::apply_document_operation(
        &mut state.document.content,
        &host_path,
        &changeset,
    ) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("[azwriter] edit apply failed: {e:?}");
            return Update::DoNothing;
        }
    };

    // Record the inverse for undo, with the resume the engine handed back.
    state.undo_stack.push((
        applied.inverse.clone(),
        applied.inverse_resume.node_path.as_ref().to_vec(),
    ));
    state.redo_stack.clear(); // a new edit orphans the redo branch

    // The model is the source of truth: re-serialize so a save writes the
    // edited document and the word count stays honest.
    let mut provider = |_: &[u32]| None;
    state.document.markdown = document::dom_to_markdown(&state.document.content, &mut provider);
    state.document.dirty = true;
    // The model tree changed: invalidate the pagination memo exactly once.
    state.document.touch();
    drop(state);

    // Commit handshake: the ACK ends the engine's preview and makes the
    // edit undoable through the same record->apply->ack loop.
    info.mark_document_edit_applied_with_inverse(changeset.id, applied.inverse);
    Update::RefreshDom
}

/// Render the document to PDF bytes. Shared by the backstage "Export PDF"
/// button and by Save-with-a-.pdf-extension, so the two cannot drift.
fn pdf_bytes(content: &Dom, info: &mut CallbackInfo) -> Vec<u8> {
    // A4 at 96 dpi CSS px, matching the canvas sheets.
    const A4_W_PX: f32 = 794.0;
    const A4_H_PX: f32 = 1123.0;

    // Ask for the token engine; the PDF engine falls back to the slicer
    // when the variable is unset.
    std::env::set_var("AZ_PAGINATION_ENGINE", "tokens");

    let mut doc = Dom::create_body().with_css(&format!(
        "margin: 0; padding: {}px; background: white; {}",
        96,
        fonts::UI_FONT_CSS
    ));
    doc.add_child(content.clone());
    let styled = azul_core::styled_dom::StyledDom::create_from_dom(doc);

    let pdf = azul::unified::pdf::Pdf::new();
    pdf.from_styled_dom_with_resources(
        styled,
        A4_W_PX,
        A4_H_PX,
        &info.get_font_cache_clone(),
        &info.get_image_cache_clone(),
    )
    .as_slice()
    .to_vec()
}

/// Backstage Export -> "Create PDF/XPS": run the whole document through
/// the engine's DOM->PDF path and write the file.
///
/// The PDF is produced from a STYLED clone of the paginated document (the
/// same content DOM the canvas shows, laid out at A4), so what is exported
/// is what the engine itself decided - no second layout model. The token
/// pagination engine is requested via AZ_PAGINATION_ENGINE, which the PDF
/// engine honors (design doc K30c: printpdf is its first consumer).
pub extern "C" fn on_export_pdf(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let (default_name, content) = {
        let Some(state) = data.downcast_ref::<AppState>() else {
            return Update::DoNothing;
        };
        (
            state.document.display_name(),
            state.document.content.clone(),
        )
    };

    let picked = FileDialog::save_file(
        AzString::from("Export as PDF"),
        OptionString::Some(AzString::from(format!("{default_name}.pdf"))),
    );
    let Some(path_str) = picked.into_option() else {
        return Update::DoNothing;
    };
    let mut path = PathBuf::from(path_str.as_str());
    if path.extension().is_none() {
        path.set_extension("pdf");
    }

    let bytes = azul_css::U8Vec::from_vec(pdf_bytes(&content, &mut info));

    if bytes.as_slice().is_empty() {
        eprintln!("[azwriter] PDF export produced no bytes");
        return Update::DoNothing;
    }
    match std::fs::write(&path, bytes.as_slice()) {
        Ok(()) => {
            eprintln!(
                "[azwriter] exported {} bytes to {}",
                bytes.as_slice().len(),
                path.display()
            );
            Update::RefreshDom
        }
        Err(e) => {
            eprintln!("[azwriter] PDF write failed: {e}");
            Update::DoNothing
        }
    }
}

/// Replay a recorded operation against the model and return its own inverse
/// (so undo and redo are the same code path in both directions).
fn replay(
    model: &mut Dom,
    op: &DocumentOperation,
    resume_path: &[u32],
) -> Option<(DocumentOperation, Vec<u32>)> {
    let changeset = DocumentChangeset::new(
        azul_core::dom::DomNodeId {
            dom: DomId::ROOT_ID,
            node: azul_core::styled_dom::NodeHierarchyItemId::from_crate_internal(None),
        },
        op.clone(),
        EditResumePoint {
            anchor_key: 0,
            node_path: resume_path.to_vec().into(),
            position: NodePosition {
                child_index: 0,
                text_byte: Some(0).into(),
            },
        },
        azul_core::task::Instant::from(std::time::Instant::now()),
    );
    let applied =
        azul_layout::document_edit::apply_document_operation(model, &[], &changeset).ok()?;
    Some((
        applied.inverse,
        applied.inverse_resume.node_path.as_ref().to_vec(),
    ))
}

/// Quick-access / Ctrl+Z: undo the last structural edit.
pub extern "C" fn on_undo(mut data: RefAny, _: CallbackInfo) -> Update {
    let Some(mut state) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    let Some((op, path)) = state.undo_stack.pop() else {
        return Update::DoNothing;
    };
    let mut model = state.document.content.clone();
    let Some(redo_entry) = replay(&mut model, &op, &path) else {
        // Put it back: a failed apply must not silently eat history.
        state.undo_stack.push((op, path));
        return Update::DoNothing;
    };
    state.document.content = model;
    state.redo_stack.push(redo_entry);
    let mut none_provider = |_: &[u32]| None;
    state.document.markdown =
        document::dom_to_markdown(&state.document.content, &mut none_provider);
    state.document.dirty = true;
    state.document.touch();
    Update::RefreshDom
}

/// Quick-access / Ctrl+Y: redo the last undone edit.
pub extern "C" fn on_redo(mut data: RefAny, _: CallbackInfo) -> Update {
    let Some(mut state) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    let Some((op, path)) = state.redo_stack.pop() else {
        return Update::DoNothing;
    };
    let mut model = state.document.content.clone();
    let Some(undo_entry) = replay(&mut model, &op, &path) else {
        state.redo_stack.push((op, path));
        return Update::DoNothing;
    };
    state.document.content = model;
    state.undo_stack.push(undo_entry);
    let mut none_provider = |_: &[u32]| None;
    state.document.markdown =
        document::dom_to_markdown(&state.document.content, &mut none_provider);
    state.document.dirty = true;
    state.document.touch();
    Update::RefreshDom
}

/// FILE app button: open the backstage on Info (the classic office-suite default pane).
pub extern "C" fn on_file_button(mut data: RefAny, _: CallbackInfo) -> Update {
    let Some(mut state) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    state.screen = Screen::Backstage;
    state.backstage_pane = 0;
    Update::RefreshDom
}

/// Backstage back arrow (and Esc via the widget behavior).
pub extern "C" fn on_backstage_back(mut data: RefAny, _: CallbackInfo) -> Update {
    let Some(mut state) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    state.screen = Screen::Editor;
    Update::RefreshDom
}

/// Backstage nav: Save / Save As run the save flow, Close resets to the
/// blank "Document1", everything else switches the pane.
pub extern "C" fn on_backstage_nav(mut data: RefAny, mut info: CallbackInfo, idx: usize) -> Update {
    const SAVE: usize = 3;
    const SAVE_AS: usize = 4;
    const CLOSE: usize = 8;

    match idx {
        SAVE | SAVE_AS => {
            let update = do_save(&mut data, &mut info, idx == SAVE_AS);
            if update == Update::RefreshDom {
                // Word returns to the document after a backstage save.
                if let Some(mut state) = data.downcast_mut::<AppState>() {
                    state.screen = Screen::Editor;
                }
            }
            update
        }
        CLOSE => {
            let Some(mut state) = data.downcast_mut::<AppState>() else {
                return Update::DoNothing;
            };
            state.document = DocumentModel::untitled();
            state.screen = Screen::Editor;
            drop(state);
            set_window_title(&mut info, "Document1");
            Update::RefreshDom
        }
        _ => {
            let Some(mut state) = data.downcast_mut::<AppState>() else {
                return Update::DoNothing;
            };
            state.backstage_pane = idx;
            Update::RefreshDom
        }
    }
}

/// Backstage Open -> Browse: native *.md dialog, then the load seam.
pub extern "C" fn on_browse_clicked(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let picked = FileDialog::open_file(
        AzString::from("Open"),
        OptionString::None,
        markdown_filter(),
    );
    let Some(path_str) = picked.into_option() else {
        return Update::DoNothing; // user cancelled
    };
    let path = Path::new(path_str.as_str());

    let Some(mut state) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    // Pipeline entry: read + markdown->HTML->XML->DOM parse, cached on the
    // model (the editor paginates the cached DOM every relayout).
    state.document = DocumentModel::from_path(path);
    state.screen = Screen::Editor;
    let name = state.document.display_name();
    drop(state);
    set_window_title(&mut info, &name);
    Update::RefreshDom
}

/// Quick-access toolbar save icon.
pub extern "C" fn on_save_clicked(mut data: RefAny, mut info: CallbackInfo) -> Update {
    do_save(&mut data, &mut info, false)
}

/// Status-bar view switcher.
pub extern "C" fn on_view_select(mut data: RefAny, _: CallbackInfo, idx: usize) -> Update {
    let Some(mut state) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    state.view_mode = idx;
    Update::RefreshDom
}

const ZOOM_MIN: f32 = 10.0;
const ZOOM_MAX: f32 = 190.0;

pub extern "C" fn on_zoom_out(mut data: RefAny, _: CallbackInfo) -> Update {
    let Some(mut state) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    state.zoom_percent = (state.zoom_percent - 10.0).clamp(ZOOM_MIN, ZOOM_MAX);
    Update::RefreshDom
}

pub extern "C" fn on_zoom_in(mut data: RefAny, _: CallbackInfo) -> Update {
    let Some(mut state) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    state.zoom_percent = (state.zoom_percent + 10.0).clamp(ZOOM_MIN, ZOOM_MAX);
    Update::RefreshDom
}

/// Dragging the status-bar zoom slider.
pub extern "C" fn on_zoom_slider(
    mut data: RefAny,
    _: CallbackInfo,
    slider: azul_layout::widgets::slider::SliderState,
) -> Update {
    let Some(mut state) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    state.zoom_percent = slider.value.round().clamp(ZOOM_MIN, ZOOM_MAX);
    Update::RefreshDom
}

// ---------------------------------------------------------------------------
// Layout
// ---------------------------------------------------------------------------

extern "C" fn layout(mut data: RefAny, info: LayoutCallbackInfo) -> Dom {
    let _frame_timer = FrameTimer::start();
    // Clone the state out so `data` can be re-shared with the callbacks
    // (RefAny::downcast_ref holds a borrow on `data`).
    let state = {
        let _p = perf::Phase::start("state_clone");
        match data.downcast_ref::<AppState>() {
            Some(s) => (*s).clone(),
            None => return Dom::create_body(),
        }
    };

    let viewport_w = info.window_size.dimensions.width;
    let viewport_h = info.window_size.dimensions.height;
    let font_cache = {
        let _p = perf::Phase::start("get_font_cache");
        Some(info.get_font_cache())
    };
    // #28(c/d): the largest monitor bounds how much content the FIRST
    // pagination builds (huge files must not paginate unbounded up front —
    // the background thread delivers the exact count afterwards).
    let max_monitor: Option<azul_css::props::basic::LayoutSize> =
        info.get_max_monitor_size().into();
    let screen = match state.screen {
        Screen::Editor => editor_ui::editor_screen(&state, &data, font_cache, max_monitor),
        Screen::Backstage => backstage_ui::backstage_screen(&state, &data),
    };

    Dom::create_body()
        .with_css(&format!(
            "display: flex; flex-direction: column; margin: 0; padding: 0; height: 100%; \
             background: white; {} font-size: 12px; color: #444444;",
            fonts::UI_FONT_CSS
        ))
        .with_child(screen)
}

// ---------------------------------------------------------------------------
// Screenshot harness (AZWRITER_SHOT)
// ---------------------------------------------------------------------------

struct ShotConfig {
    path: String,
}

extern "C" fn shot_tick(mut data: RefAny, info: TimerCallbackInfo) -> TimerCallbackReturn {
    let Some(cfg) = data.downcast_ref::<ShotConfig>() else {
        return TimerCallbackReturn {
            should_update: Update::DoNothing,
            should_terminate: TerminateTimer::Terminate,
        };
    };
    match info
        .callback_info
        .take_screenshot_to_file(DomId::ROOT_ID, &cfg.path)
    {
        Ok(()) => {
            eprintln!("[azwriter] screenshot written: {}", cfg.path);
            std::process::exit(0);
        }
        Err(e) => {
            eprintln!("[azwriter] screenshot FAILED: {}", e.as_str());
            std::process::exit(2);
        }
    }
}

/// Window-create hook: installs the screenshot timer when AZWRITER_SHOT is
/// set (the delay lets the first layout + async font load settle;
/// AZWRITER_SHOT_DELAY_MS overrides the default 2500).
///
/// NOTE: `create_callback` only fires on the real platform backends —
/// the headless backend ignores it (see ENGINE-ISSUES.md), so screenshots
/// are taken through a short-lived real window.
extern "C" fn startup_focus_tick(
    _data: RefAny,
    mut info: TimerCallbackInfo,
) -> TimerCallbackReturn {
    use azul_css::css::{CssPath, CssPathSelector};
    info.callback_info.set_focus_to_path(
        azul_core::dom::DomId::ROOT_ID,
        CssPath {
            selectors: vec![CssPathSelector::Class("mw-doc".to_string().into())].into(),
        },
    );
    TimerCallbackReturn {
        should_update: Update::DoNothing,
        should_terminate: TerminateTimer::Terminate,
    }
}

extern "C" fn on_window_created(data: RefAny, mut info: CallbackInfo) -> Update {
    // Word focuses the document on open: the caret blinks immediately.
    // A one-shot TIMER (not a direct set_focus here): the create callback
    // runs BEFORE the first layout, and the shell resolves focus targets
    // against layout_results — resolving now silently matches nothing and
    // is never retried (engine gap, recorded in azul #21). 150 ms lands
    // after the first layout the same way the screenshot timer does.
    {
        let timer = Timer::create(
            RefAny::new(()),
            TimerCallback::create(startup_focus_tick),
            info.get_system_time_fn(),
        )
        .with_delay(Duration::System(SystemTimeDiff::from_millis(150)));
        info.add_timer(TimerId::unique(), timer);
    }
    if let Ok(path) = std::env::var("AZWRITER_SHOT") {
        let delay_ms: u64 = std::env::var("AZWRITER_SHOT_DELAY_MS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(2500);
        let timer = Timer::create(
            RefAny::new(ShotConfig { path }),
            TimerCallback::create(shot_tick),
            info.get_system_time_fn(),
        )
        .with_delay(Duration::System(SystemTimeDiff::from_millis(delay_ms)));
        info.add_timer(TimerId::unique(), timer);
    }
    let _ = data;
    Update::DoNothing
}

// ---------------------------------------------------------------------------
// main
// ---------------------------------------------------------------------------

/// Start the app. On desktop/iOS this blocks; on Android `App::run` only
/// stashes the window options for libazul's `android_main` to pick up, then
/// returns — see the ctor below.
pub fn start() {
    let mut state = AppState::default();

    // Screenshot harness: pick the screen to render.
    match std::env::var("AZWRITER_SCREEN").as_deref() {
        Ok("backstage-info") => {
            state.screen = Screen::Backstage;
            state.backstage_pane = 0;
        }
        Ok("backstage-open") => {
            state.screen = Screen::Backstage;
            state.backstage_pane = 2;
        }
        _ => {}
    }

    // Harness/CLI: open a markdown file at startup (same pipeline entry as
    // the backstage Browse dialog).
    if let Ok(p) = std::env::var("AZWRITER_OPEN") {
        state.document = DocumentModel::from_path(Path::new(&p));
    }

    // PRIME THE PAGINATION MEMO BEFORE THE WINDOW EXISTS.
    //
    // The first pagination of a document is the expensive one (font
    // resolution + a full document layout): measured 1.8s here, 5.1s when
    // it also had to scan the system fonts. Paying that inside the first
    // layout() call means the client cannot answer the compositor's
    // configure/ping handshake while its surface is being mapped - the
    // window then gets dropped and the app looks like it crashed. Doing it
    // HERE turns the same work into ordinary startup latency: there is no
    // surface yet, so nobody is waiting on a frame.
    // NO PRIMER. This used to paginate here, before the window existed, so the
    // first frame would not block the compositor's configure/ping handshake.
    // It cost more than it saved: with no window there is no engine yet, so it
    // passed `fonts: None` and `build_font_cache()` SCANNED THE WHOLE SYSTEM
    // on the main thread — 96 ms — and the engine then built its own cache
    // anyway.
    //
    // `App::create` spawns an async font scout and hands the layout callback a
    // registry-backed cache through `LayoutCallbackInfo::get_font_cache()`,
    // which `editor_ui` already passes to `paginate_cached_with_fonts`. So the
    // first layout() paginates with fonts somebody else already found.
    //
    // Measured on a 24-line markdown (AZWRITER_FRAME_LOG=all):
    //   with primer:     96 ms scan + 126 ms pagination BEFORE the window,
    //                    then frame #0 layout() = 2.5 ms
    //   without primer:  no scan at all, frame #0 layout() = 31.6 ms
    // ~190 ms of startup for 29 ms on the first frame — and 31 ms is far
    // inside the handshake budget that the original 167 ms blew.

    // AZWRITER_PAGINATE_TWICE: force a SECOND pagination of the same content
    // under a fresh generation, so the memo misses again. The first call pays
    // the system font scan and every first-use font FILE load; the second
    // pays neither. The gap between them separates "pagination is slow" from
    // "the first pagination is slow".
    if std::env::var_os("AZWRITER_PAGINATE_TWICE").is_some() {
        let t = std::time::Instant::now();
        let _ = document::paginate_cached(&state.document.content, document::next_generation());
        eprintln!("[primer] SECOND pagination (warm) took {:?}", t.elapsed());
    }

    let data = RefAny::new(state);
    let mut config = AppConfig::create();
    // Identity for the engine services (updater state dir, telemetry service
    // name when the `telemetry` feature is on): metrics/logs then arrive
    // labelled azwriter/<version> instead of the generic default.
    config.updates.app_name = AzString::from("azwriter");
    config.updates.current_version = AzString::from(env!("CARGO_PKG_VERSION"));
    let app = App::create(data, config);

    let mut window =
        WindowCreateOptions::create(layout as azul_core::callbacks::LayoutCallbackType);
    window.window_state.title = AzString::from("Document1 - AzWriter");
    // Open MAXIMIZED. A document editor that starts in a 1280x800 box on a 5K
    // display is the first thing every user fixes by hand.
    //
    // All four desktop backends honour `flags.frame` at creation, each through
    // its own platform call — macOS performZoom, Windows ShowWindow(SW_MAXIMIZE),
    // X11 _NET_WM_STATE_MAXIMIZED_{HORZ,VERT}, Wayland
    // xdg_toplevel.set_maximized — so this is one flag rather than four
    // per-platform paths.
    window.window_state.flags.frame = azul_core::window::WindowFrame::Maximized;
    // The dimensions still matter: they are the size the window RESTORES to
    // when the user un-maximizes, and the size every headless/screenshot run
    // uses (nothing maximizes a stub window).
    window.window_state.size.dimensions.width = 1280.0;
    window.window_state.size.dimensions.height = 800.0;
    // Harness: AZWRITER_SIZE=WxH overrides the initial window size (narrow
    // ribbon states are screenshot-reproducible without a live drag).
    if let Ok(sz) = std::env::var("AZWRITER_SIZE") {
        if let Some((w, h)) = sz.split_once('x') {
            if let (Ok(w), Ok(h)) = (w.parse::<f32>(), h.parse::<f32>()) {
                window.window_state.size.dimensions.width = w;
                window.window_state.size.dimensions.height = h;
            }
        }
    }
    window.create_callback = Some(Callback::create(
        on_window_created as azul_layout::callbacks::CallbackType,
    ))
    .into();

    app.run(window);
}

// Android has no `main()`: the OS loads this cdylib and calls libazul's
// `android_main` through the android-activity glue. `android_main` reads the
// window options `App::run` stashed, so `start()` must run BEFORE
// `ANativeActivity_onCreate` — i.e. from a library constructor that fires at
// `System.loadLibrary` time. Same shape as AzMaps.
#[cfg(target_os = "android")]
#[ctor::ctor]
fn azul_android_init() {
    start();
}
