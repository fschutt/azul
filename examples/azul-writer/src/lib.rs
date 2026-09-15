mod args;
mod backstage_ui;
mod document;
mod editor_ui;
mod fonts;
pub mod ir;
mod palette;
mod perf;
mod ribbon_ui;

use std::path::{Path, PathBuf};

use azul::{
    app::{App, AppConfig},
    callbacks::{
        CallbackInfo, LayoutCallbackInfo, RefAny, TimerCallback, TimerCallbackInfo,
        TimerCallbackReturn, Update, WriteBackCallback,
    },
    css::{DocumentOperation, LayoutSize, SystemStyleDependency, WindowDecorations},
    dialog::{FileDialog, FileOpenResult, SaveTargetResult},
    dom::{Callback, Dom, DomId, DomNodeId},
    file::FilePath,
    option::{
        OptionFileTypeList, OptionLogicalRect, OptionRefAny, OptionString, OptionThreadSendMsg,
    },
    pdf::Pdf,
    str::String as AzString,
    svg::{CssPath, CssPathSelector, LogicalRect},
    task::{
        TerminateTimer, Thread, ThreadId, ThreadReceiveMsg, ThreadReceiver, ThreadSendMsg,
        ThreadSender, ThreadWriteBackMsg, Timer, TimerId,
    },
    time::{Duration, SystemTimeDiff},
    widgets::SliderState,
    window::{WindowCreateOptions, WindowFrame},
};

pub use crate::args::Args;
use crate::document::{DocumentModel, FontCacheSnapshot};

static WINDOW_ARGS: std::sync::OnceLock<Args> = std::sync::OnceLock::new();

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

fn root_dom_id() -> DomId {
    DomId { inner: 0 }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Editor,
    Backstage,
}

#[derive(Clone)]
pub struct AppState {
    pub screen: Screen,
    pub backstage_pane: usize,
    pub ribbon_tab: usize,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub align: usize,
    pub selected_style: usize,
    pub view_mode: usize,
    pub zoom_percent: f32,
    pub editing_page: usize,
    pub undo_stack: Vec<(DocumentOperation, Vec<u32>)>,
    pub redo_stack: Vec<(DocumentOperation, Vec<u32>)>,
    pub document: DocumentModel,
    pub exact_page_count: Option<(u64, usize)>,
    pub pages_vv_node: Option<DomNodeId>,
    pub pagination_thread: Option<(u64, ThreadId)>,
    pub word_count_marker: AzString,
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
            word_count_marker: azul::uuid::Uuid::short(),
        }
    }
}

const PAGINATION_CHUNK_BLOCKS: u32 = 64;

struct PaginationThreadInit {
    ir: ir::IrDocument,
    generation: u64,
    fonts: Option<FontCacheSnapshot>,
}

struct PaginationChunk {
    generation: u64,
    pages_so_far: usize,
    paths_so_far: Vec<Vec<u32>>,
    done: bool,
}

extern "C" fn pagination_worker(
    mut init: RefAny,
    mut sender: ThreadSender,
    mut recv: ThreadReceiver,
) {
    use azul::dom::DomSplit;

    let (doc_ir, generation, fonts) = {
        let Some(init) = init.downcast_ref::<PaginationThreadInit>() else {
            return;
        };
        (init.ir.clone(), init.generation, init.fonts.clone())
    };

    let total_blocks = doc_ir.blocks.len() as u32;
    let content = document::content_dom_from_ir(&doc_ir);

    let mut remaining = content;
    let mut block_offset: u32 = 0;
    let mut paths_acc: Vec<Vec<u32>> = Vec::new();
    let mut pages_acc: usize = 0;

    loop {
        if matches!(
            recv.recv(),
            OptionThreadSendMsg::Some(ThreadSendMsg::TerminateThread)
        ) {
            return;
        }

        let split = DomSplit::at_path(&remaining, vec![PAGINATION_CHUNK_BLOCKS]);
        let (chunk, tail) = (split.head, split.tail);
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
            paths_acc.push(vec![block_offset]);
        }

        let sent = sender.send(ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg {
            refany: RefAny::new(PaginationChunk {
                generation,
                pages_so_far: pages_acc,
                paths_so_far: paths_acc.clone(),
                done,
            }),
            callback: WriteBackCallback {
                cb: pagination_writeback,
                ctx: OptionRefAny::None,
            },
        }));
        if !sent || done {
            return;
        }
        remaining = tail;
    }
}

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

    if let Some(vv) = vv_node {
        use azul::css::{LogicalPosition, LogicalSize};
        let stride = editor_ui::page_stride(zoom);
        let width = (editor_ui::page_sheet_w() * zoom).round() + 2.0;
        info.update_virtual_view(
            vv,
            OptionLogicalRect::None,
            OptionLogicalRect::Some(LogicalRect {
                origin: LogicalPosition::zero(),
                size: LogicalSize {
                    width,
                    height: pages as f32 * stride,
                },
            }),
        );
    }

    Update::RefreshDom
}

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
    if let Some((_, old)) = state.pagination_thread.take() {
        info.remove_thread(old);
    }

    let init = RefAny::new(PaginationThreadInit {
        ir: state.document.ir.clone(),
        generation,
        fonts,
    });
    let thread = Thread::create(init, app_for_thread, pagination_worker);
    let thread_id = ThreadId::unique();
    info.add_thread(thread_id, thread);
    state.pagination_thread = Some((generation, thread_id));
    Update::DoNothing
}

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

fn set_window_title(info: &mut CallbackInfo, name: &str) {
    let mut st = info.get_current_window_state();
    st.title = AzString::from(format!("{name} - AzWriter"));
    info.modify_window_state(st);
}

fn markdown_filter() -> OptionFileTypeList {
    use azul::file::FileTypeList;
    OptionFileTypeList::Some(FileTypeList {
        document_types: vec![AzString::from("*.md")].into(),
        document_descriptor: AzString::from("Markdown documents (*.md)"),
    })
}

fn snapshot_for_save(
    data: &mut RefAny,
    info: &mut CallbackInfo,
) -> Option<(Option<PathBuf>, DocumentModel)> {
    Some({
        let Some(mut state) = data.downcast_mut::<AppState>() else {
            return None;
        };
        if sync_ir_text_from_engine(&mut state, info) {
            state.document.refresh_derived();
            state.document.dirty = true;
        }
        let mut snapshot = state.document.clone();
        snapshot.markdown = ir::to_markdown(&snapshot.ir);
        (state.document.path.clone(), snapshot)
    })
}

fn do_save(data: &mut RefAny, info: &mut CallbackInfo, always_ask: bool) -> Update {
    let Some((current_path, model_snapshot)) = snapshot_for_save(data, info) else {
        return Update::DoNothing;
    };
    if current_path.is_none() || always_ask {
        let _request = FileDialog::save_file(
            AzString::from("Save As - .md for markdown, .pdf to export"),
            AzString::from("document.md"),
            data.clone(),
            on_save_target_picked,
        );
        return Update::DoNothing;
    }
    let Some(path) = current_path else {
        return Update::DoNothing;
    };
    save_snapshot_to(data, info, path, model_snapshot)
}

extern "C" fn on_save_target_picked(
    mut data: RefAny,
    mut info: CallbackInfo,
    result: RefAny,
) -> Update {
    let Some(picked) = SaveTargetResult::downcast(result).into_option() else {
        return Update::DoNothing;
    };
    let Some(target) = picked.target.into_option() else {
        return Update::DoNothing;
    };
    let Some(path) = target.as_path().into_option() else {
        return Update::DoNothing;
    };
    let mut path = PathBuf::from(path.as_string().as_str());
    if path.extension().is_none() {
        path.set_extension("md");
    }
    let Some((_, model_snapshot)) = snapshot_for_save(&mut data, &mut info) else {
        return Update::DoNothing;
    };
    save_snapshot_to(&mut data, &mut info, path, model_snapshot)
}

fn save_snapshot_to(
    data: &mut RefAny,
    info: &mut CallbackInfo,
    path: PathBuf,
    model_snapshot: DocumentModel,
) -> Update {
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

pub(crate) fn sync_ir_text_from_engine(state: &mut AppState, info: &mut CallbackInfo) -> bool {
    let edits = info.get_unsynced_text_edits();
    let edits = edits.as_ref();
    if edits.is_empty() {
        return false;
    }
    let total_blocks = state.document.ir.blocks.len();
    let mut changed = false;
    let mut max_revision = 0u64;
    for edit in edits {
        max_revision = max_revision.max(edit.revision);
        let text = edit.text.as_str();
        let mut applied = false;
        for i in 0..total_blocks {
            let id = document::block_dom_id(i);
            let block_node = info.get_node_id_by_id_attribute(edit.node.dom, id.as_str());
            if block_node.into_raw() == 0 {
                continue;
            }
            let block_id = DomNodeId {
                dom: edit.node.dom,
                node: block_node,
            };
            let Some(rel) = info
                .get_node_child_index_path(block_id, edit.node)
                .into_option()
            else {
                continue;
            };
            let rel = rel.as_ref();
            applied = true;
            changed |= match state.document.ir.blocks.get(i) {
                Some(ir::IrBlock::Paragraph(_)) => match rel.first() {
                    Some(&run) => ir::set_run_text(&mut state.document.ir, i, run as usize, text),
                    None => ir::sync_block_text(&mut state.document.ir, &[i as u32], text),
                },
                Some(ir::IrBlock::List(_)) => {
                    let item = rel.first().copied().unwrap_or(0);
                    ir::sync_block_text(&mut state.document.ir, &[i as u32, item], text)
                }
                _ => false,
            };
            break;
        }
        if !applied {
        }
    }
    info.mark_text_revision_synced(max_revision);
    changed
}

pub(crate) fn map_node_to_block(
    state: &AppState,
    info: &mut CallbackInfo,
    node: DomNodeId,
) -> Option<(usize, Vec<u32>)> {
    for i in 0..state.document.ir.blocks.len() {
        let id = document::block_dom_id(i);
        let block_node = info.get_node_id_by_id_attribute(node.dom, id.as_str());
        if block_node.into_raw() == 0 {
            continue;
        }
        let block_id = DomNodeId {
            dom: node.dom,
            node: block_node,
        };
        if let Some(rel) = info.get_node_child_index_path(block_id, node).into_option() {
            return Some((i, rel.as_ref().to_vec()));
        }
    }
    None
}

pub(crate) fn map_span_to_block_range(
    state: &AppState,
    info: &mut CallbackInfo,
    span: &azul::dom::DocumentSelectionSpan,
) -> Option<(usize, usize, usize)> {
    let (block, rel) = map_node_to_block(state, info, span.node)?;
    let run_start: usize = match state.document.ir.blocks.get(block) {
        Some(ir::IrBlock::Paragraph(p)) => {
            let run_idx = rel.first().copied().unwrap_or(0) as usize;
            p.runs.iter().take(run_idx).map(|r| r.text.len()).sum()
        }
        _ => 0,
    };
    Some((
        block,
        run_start + span.start_byte as usize,
        run_start + span.end_byte as usize,
    ))
}

pub(crate) fn apply_format_axis(
    data: &mut RefAny,
    info: &mut CallbackInfo,
    axis: ir::FormatAxis,
) -> Update {
    let Some(mut state) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    let mut changed = sync_ir_text_from_engine(&mut state, info);
    let spans = info.get_document_selection();
    for span in spans.as_ref() {
        if let Some((block, start, end)) = map_span_to_block_range(&state, info, span) {
            changed |= ir::toggle_format_range(&mut state.document.ir, block, start, end, axis);
        }
    }
    if changed {
        state.document.refresh_derived();
        state.document.dirty = true;
        Update::RefreshDom
    } else {
        Update::DoNothing
    }
}

pub extern "C" fn on_text_changed(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let (marker, label) = {
        let Some(mut state) = data.downcast_mut::<AppState>() else {
            return Update::DoNothing;
        };
        if sync_ir_text_from_engine(&mut state, &mut info) {
            state.document.refresh_derived();
            state.document.dirty = true;
        }
        (
            state.word_count_marker.clone(),
            AzString::from(format!("{} WORDS", state.document.word_count())),
        )
    };
    if let Some(node) = info.get_node_id_by_marker(marker).into_option() {
        azul::widgets::StatusBar::update_segment_label(info, node, label);
    }
    Update::DoNothing
}

pub extern "C" fn on_document_edit(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let changeset = match info.get_document_edit_clone().into_option() {
        Some(c) => c,
        None => return Update::DoNothing,
    };

    let Some(mut state) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };

    let synced = sync_ir_text_from_engine(&mut state, &mut info);

    let pages = document::paginate_cached(&state.document.content, state.document.generation);
    let offsets = document::page_block_offsets(&pages);
    let page_index = state.editing_page.min(offsets.len().saturating_sub(1));
    let page_offset = offsets.get(page_index).copied().unwrap_or(0);
    let mut resume: Vec<u32> = changeset.resume.node_path.as_ref().to_vec();
    if let Some(first) = resume.first_mut() {
        *first += page_offset as u32;
    }

    let Some((inverse, inverse_resume)) =
        ir::apply_operation(&mut state.document.ir, &changeset.operation, &resume)
    else {
        eprintln!("[azwriter] edit apply failed: operation not mirrorable onto the IR");
        if synced {
            state.document.refresh_derived();
            state.document.dirty = true;
        }
        return if synced {
            Update::RefreshDom
        } else {
            Update::DoNothing
        };
    };

    state.undo_stack.push((inverse.clone(), inverse_resume));
    state.redo_stack.clear();

    state.document.refresh_derived();
    state.document.dirty = true;
    drop(state);

    info.mark_document_edit_applied_with_inverse(changeset.id, inverse);
    Update::RefreshDom
}

fn reborrow_info(info: &CallbackInfo) -> CallbackInfo {
    CallbackInfo {
        ref_data: info.ref_data,
        hit_dom_node: info.hit_dom_node,
        cursor_relative_to_item: info.cursor_relative_to_item,
        cursor_in_viewport: info.cursor_in_viewport,
        changes: info.changes,
    }
}

fn pdf_bytes(content: &Dom, info: &mut CallbackInfo) -> Vec<u8> {
    const A4_W_PX: f32 = 794.0;
    const A4_H_PX: f32 = 1123.0;

    std::env::set_var("AZ_PAGINATION_ENGINE", "tokens");

    let mut doc = Dom::create_body().with_css(
        format!(
            "margin: 0; padding: {}px; background: white; {}",
            96,
            fonts::UI_FONT_CSS
        )
        .as_str(),
    );
    doc.add_child(content.clone());

    let pdf = Pdf::create();
    pdf.from_dom_in_callback(reborrow_info(info), doc, A4_W_PX, A4_H_PX)
        .as_ref()
        .to_vec()
}

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

    let bytes = pdf_bytes(&content, &mut info);

    if bytes.is_empty() {
        eprintln!("[azwriter] PDF export produced no bytes");
        return Update::DoNothing;
    }
    let name = format!("{default_name}.pdf");
    let len = bytes.len();
    if FileDialog::save_bytes(
        AzString::from(name.clone()),
        AzString::from("application/pdf"),
        bytes,
    ) {
        eprintln!("[azwriter] exported {len} bytes as {name}");
        Update::RefreshDom
    } else {
        eprintln!("[azwriter] PDF export cancelled");
        Update::DoNothing
    }
}

pub extern "C" fn on_undo(mut data: RefAny, _: CallbackInfo) -> Update {
    let Some(mut state) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    let Some((op, path)) = state.undo_stack.pop() else {
        return Update::DoNothing;
    };
    let Some(redo_entry) = ir::apply_operation(&mut state.document.ir, &op, &path) else {
        state.undo_stack.push((op, path));
        return Update::DoNothing;
    };
    state.redo_stack.push(redo_entry);
    state.document.refresh_derived();
    state.document.dirty = true;
    Update::RefreshDom
}

pub extern "C" fn on_redo(mut data: RefAny, _: CallbackInfo) -> Update {
    let Some(mut state) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    let Some((op, path)) = state.redo_stack.pop() else {
        return Update::DoNothing;
    };
    let Some(undo_entry) = ir::apply_operation(&mut state.document.ir, &op, &path) else {
        state.redo_stack.push((op, path));
        return Update::DoNothing;
    };
    state.undo_stack.push(undo_entry);
    state.document.refresh_derived();
    state.document.dirty = true;
    Update::RefreshDom
}

pub extern "C" fn on_file_button(mut data: RefAny, _: CallbackInfo) -> Update {
    let Some(mut state) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    state.screen = Screen::Backstage;
    state.backstage_pane = 0;
    Update::RefreshDom
}

pub extern "C" fn on_backstage_back(mut data: RefAny, _: CallbackInfo) -> Update {
    let Some(mut state) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    state.screen = Screen::Editor;
    Update::RefreshDom
}

pub extern "C" fn on_backstage_nav(mut data: RefAny, mut info: CallbackInfo, idx: usize) -> Update {
    const SAVE: usize = 3;
    const SAVE_AS: usize = 4;
    const CLOSE: usize = 8;

    match idx {
        SAVE | SAVE_AS => {
            let update = do_save(&mut data, &mut info, idx == SAVE_AS);
            if matches!(update, Update::RefreshDom) {
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

pub extern "C" fn on_browse_clicked(data: RefAny, _info: CallbackInfo) -> Update {
    let _request = FileDialog::open_file(
        AzString::from("Open"),
        OptionString::None,
        markdown_filter(),
        data,
        on_browse_picked,
    );
    Update::DoNothing
}

extern "C" fn on_browse_picked(mut data: RefAny, mut info: CallbackInfo, result: RefAny) -> Update {
    let Some(picked) = FileOpenResult::downcast(result).into_option() else {
        return Update::DoNothing;
    };
    let Some(path_str) = picked.path.into_option() else {
        return Update::DoNothing;
    };
    let path_string = path_str.as_string();
    let path = Path::new(path_string.as_str());

    let Some(mut state) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    state.document = DocumentModel::from_path(path);
    state.screen = Screen::Editor;
    let name = state.document.display_name();
    drop(state);
    set_window_title(&mut info, &name);
    Update::RefreshDom
}

pub extern "C" fn on_save_clicked(mut data: RefAny, mut info: CallbackInfo) -> Update {
    do_save(&mut data, &mut info, false)
}

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

pub extern "C" fn on_zoom_slider(mut data: RefAny, _: CallbackInfo, slider: SliderState) -> Update {
    let Some(mut state) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    state.zoom_percent = slider.value.round().clamp(ZOOM_MIN, ZOOM_MAX);
    Update::RefreshDom
}

pub const MOBILE_BREAKPOINT_PX: f32 = 720.0;

extern "C" fn layout(mut data: RefAny, info: LayoutCallbackInfo) -> Dom {
    let _frame_timer = FrameTimer::start();
    let state = {
        let _p = perf::Phase::start("state_clone");
        match data.downcast_ref::<AppState>() {
            Some(s) => (*s).clone(),
            None => return Dom::create_body(),
        }
    };

    let font_cache = {
        let _p = perf::Phase::start("get_font_cache");
        Some(FontCacheSnapshot::from_layout_info(&info))
    };
    let max_monitor: Option<LayoutSize> = info.get_max_monitor_size().into_option();
    info.depends_on_system_style(SystemStyleDependency::Theme);
    info.depends_on_system_style(SystemStyleDependency::Colors);
    let system_style = info.get_system_style_untracked();
    let pal = palette::Palette::from_system(&system_style, info.get_theme());

    let compact = !info.viewport_bigger_than(MOBILE_BREAKPOINT_PX);

    let screen = match state.screen {
        Screen::Editor => editor_ui::editor_screen(
            &state,
            &data,
            font_cache,
            max_monitor,
            &pal,
            &system_style,
            compact,
        ),
        Screen::Backstage => backstage_ui::backstage_screen(&state, &data, &pal, &system_style),
    };

    Dom::create_body()
        .with_css(
            format!(
                "display: flex; flex-direction: column; margin: 0; padding: 0; height: 100%; \
                 background: {}; {} font-size: 12px; color: {};",
                palette::Palette::hex(pal.chrome),
                fonts::UI_FONT_CSS,
                palette::Palette::hex(pal.text),
            )
            .as_str(),
        )
        .with_child(screen)
}

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
    let png = match info
        .callback_info
        .take_screenshot(root_dom_id())
        .into_result()
    {
        Ok(png) => png,
        Err(e) => {
            eprintln!("[azwriter] screenshot FAILED: {}", e.as_str());
            std::process::exit(2);
        }
    };
    match FilePath::from_str(cfg.path.as_str())
        .write_bytes(png)
        .into_result()
    {
        Ok(_) => {
            eprintln!("[azwriter] screenshot written: {}", cfg.path);
            std::process::exit(0);
        }
        Err(e) => {
            eprintln!("[azwriter] screenshot FAILED: {}", e.message.as_str());
            std::process::exit(2);
        }
    }
}

extern "C" fn startup_focus_tick(
    _data: RefAny,
    mut info: TimerCallbackInfo,
) -> TimerCallbackReturn {
    info.callback_info.set_focus_to_path(
        root_dom_id(),
        CssPath {
            selectors: vec![CssPathSelector::Class("mw-doc".into())].into(),
        },
    );
    TimerCallbackReturn {
        should_update: Update::DoNothing,
        should_terminate: TerminateTimer::Terminate,
    }
}

extern "C" fn on_window_created(data: RefAny, mut info: CallbackInfo) -> Update {
    {
        let timer = Timer::create(
            RefAny::new(()),
            TimerCallback {
                cb: startup_focus_tick,
                ctx: OptionRefAny::None,
            },
            info.get_system_time_fn(),
        )
        .with_delay(Duration::System(SystemTimeDiff::from_millis(150)));
        info.add_timer(TimerId::unique(), timer);
    }
    if let Some((path, delay_ms)) = WINDOW_ARGS.get().and_then(|a| {
        a.shot
            .as_ref()
            .map(|p| (p.display().to_string(), a.shot_delay_ms))
    }) {
        let timer = Timer::create(
            RefAny::new(ShotConfig { path }),
            TimerCallback {
                cb: shot_tick,
                ctx: OptionRefAny::None,
            },
            info.get_system_time_fn(),
        )
        .with_delay(Duration::System(SystemTimeDiff::from_millis(delay_ms)));
        info.add_timer(TimerId::unique(), timer);
    }
    let _ = data;
    Update::DoNothing
}

pub fn start(args: Args) {
    perf::init_frame_log(args.frame_log);
    document::init_dump_xml(args.dump_xml.clone());

    let mut state = AppState::default();

    match args.screen {
        args::Screen::Editor => {}
        args::Screen::BackstageInfo => {
            state.screen = Screen::Backstage;
            state.backstage_pane = 0;
        }
        args::Screen::BackstageOpen => {
            state.screen = Screen::Backstage;
            state.backstage_pane = 2;
        }
    }

    if let Some(p) = args.open.as_deref() {
        state.document = DocumentModel::from_path(p);
    }

    if args.paginate_twice {
        let t = std::time::Instant::now();
        let _ = document::paginate_cached(&state.document.content, document::next_generation());
        eprintln!("[primer] SECOND pagination (warm) took {:?}", t.elapsed());
    }

    let data = RefAny::new(state);
    let mut config = AppConfig::create();
    config.updates.app_name = AzString::from("azwriter");
    config.updates.current_version = AzString::from(env!("CARGO_PKG_VERSION"));
    let app = App::create(data, config);

    let mut window = WindowCreateOptions::create(layout);
    window.window_state.title = AzString::from("Document1 - AzWriter");
    window.window_state.flags.frame = WindowFrame::Maximized;
    window.window_state.flags.decorations = WindowDecorations::None;
    window.window_state.size.dimensions.width = 1280.0;
    window.window_state.size.dimensions.height = 800.0;
    if let Some((w, h)) = args.size {
        window.window_state.size.dimensions.width = w;
        window.window_state.size.dimensions.height = h;
    }
    window.create_callback = Some(Callback::create(on_window_created)).into();
    WINDOW_ARGS.set(args).ok();

    app.run(window);
}

#[cfg(target_os = "android")]
#[ctor::ctor]
fn azul_android_init() {
    start(Args::default());
}
