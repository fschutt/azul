//! The editing session over a deck: the current slide and the rail's
//! selection, the selected elements and the one whose text is edited, the
//! undo history (deck snapshots, one per committed edit), the clipboard. All
//! the ribbon's and the canvas's commands are methods here, without window
//! types (the two selections are azul's `ListSelection`), so they are tested
//! without a window.

use crate::model::{
    Align, Animation, AnimationEffect, Background, ChartKind, Color, Deck, Element, ElementKind,
    Frame, ImageFit, LayoutKind, ShapeKind, Slide, SlideSize, TextBody, Theme, TransitionKind,
    VAlign, ZOrder,
};
use azul::widgets::{ListSelection, RichTextEditorState};
use azul_appkit::UndoHistory;

/// The most undo steps kept.
pub const UNDO_DEPTH: usize = 100;
/// How far a paste moves its copy from the original, slide units.
pub const PASTE_OFFSET: f32 = 20.0;

/// An editing session.
#[derive(Debug, Clone)]
pub struct Editor {
    pub deck: Deck,
    /// The slide on the canvas.
    pub current: usize,
    /// The slides selected in the rail, by index: azul's list selection,
    /// which always keeps one slide and anchors the Shift range.
    pub rail: ListSelection,
    /// The selected elements of the current slide, by id.
    pub selection: ListSelection,
    /// The element whose text is being edited.
    pub editing: Option<u64>,
    /// The shared rich-text editor's state of that text as the editor last
    /// reported it (its caret, its ONE undo history): used while it still
    /// shows what the body holds (`text::state_for`), the body is the truth.
    pub text: Option<RichTextEditorState>,
    /// The slides (by id) whose section is folded in the rail.
    pub folded: Vec<u64>,
    /// Edited since the last save.
    pub dirty: bool,
    /// Copied elements.
    pub clipboard: Vec<Element>,
    paste_count: u32,
    /// The decks before each committed edit (appkit's shared undo stack); a
    /// drag on the canvas is one step (its coalescing key, sealed on commit).
    history: UndoHistory<Deck>,
}

/// A rail selection of slide `index` alone; the rail always keeps one
/// slide selected.
fn rail_at(index: usize) -> ListSelection {
    ListSelection::create()
        .with_keep_one(true)
        .apply(index as u64, false, false)
}

impl Editor {
    #[must_use]
    pub fn new(deck: Deck) -> Self {
        Self {
            deck,
            current: 0,
            rail: rail_at(0),
            selection: ListSelection::create(),
            editing: None,
            text: None,
            folded: Vec::new(),
            dirty: false,
            clipboard: Vec::new(),
            paste_count: 0,
            history: UndoHistory::new("Open").with_limit(UNDO_DEPTH),
        }
    }

    #[must_use]
    pub fn slide(&self) -> &Slide {
        &self.deck.slides[self.current.min(self.deck.slides.len().saturating_sub(1))]
    }

    pub fn slide_mut(&mut self) -> &mut Slide {
        let i = self.current.min(self.deck.slides.len().saturating_sub(1));
        &mut self.deck.slides[i]
    }

    /// Takes the undo snapshot of the deck before an edit.
    pub fn checkpoint(&mut self) {
        self.history.checkpoint("Edit", self.deck.clone());
        self.dirty = true;
    }

    #[must_use]
    pub fn can_undo(&self) -> bool {
        self.history.can_undo()
    }

    #[must_use]
    pub fn can_redo(&self) -> bool {
        self.history.can_redo()
    }

    fn after_history(&mut self) {
        self.current = self.current.min(self.deck.slides.len().saturating_sub(1));
        self.rail = rail_at(self.current);
        let ids: Vec<u64> = self.slide().elements.iter().map(|e| e.id).collect();
        self.selection.retain_in(ids);
        self.editing = None;
        self.dirty = true;
    }

    pub fn undo(&mut self) -> bool {
        if !self.history.undo(&mut self.deck) {
            return false;
        }
        self.after_history();
        true
    }

    pub fn redo(&mut self) -> bool {
        if !self.history.redo(&mut self.deck) {
            return false;
        }
        self.after_history();
        true
    }

    /// The slides selected in the rail, ascending.
    #[must_use]
    pub fn selected_slides(&self) -> Vec<usize> {
        self.rail.keys.as_ref().iter().map(|&k| k as usize).collect()
    }

    /// The selected elements' ids, ascending.
    #[must_use]
    pub fn selected_ids(&self) -> Vec<u64> {
        self.selection.keys.as_ref().to_vec()
    }

    // ==== Slides ====

    /// Slide `index` on the canvas, alone selected in the rail.
    pub fn go_to(&mut self, index: usize) {
        self.current = index.min(self.deck.slides.len().saturating_sub(1));
        self.rail = rail_at(self.current);
        self.selection.clear();
        self.editing = None;
    }

    /// A click (or an arrow) in the rail on slide `index`: Shift selects the
    /// range from the anchor, Ctrl / Cmd toggles it, else it alone.
    pub fn rail_select(&mut self, index: usize, shift: bool, ctrl: bool) {
        let index = index.min(self.deck.slides.len().saturating_sub(1));
        if !shift && !ctrl {
            self.go_to(index);
            return;
        }
        self.rail.select(index as u64, shift, ctrl);
        // The canvas shows the slide clicked; a toggled-out current slide
        // hands over to the first one still selected.
        if self.rail.contains(index as u64) {
            self.current = index;
        } else if !self.rail.contains(self.current as u64) {
            self.current = self.selected_slides().first().copied().unwrap_or(index);
        }
        self.selection.clear();
        self.editing = None;
    }

    /// A new slide of `layout` after the current one, on the canvas.
    pub fn new_slide(&mut self, layout: LayoutKind) {
        self.checkpoint();
        let at = self.deck.add_slide(self.current + 1, layout);
        self.go_to(at);
    }

    /// Copies of the selected slides after the last of them.
    pub fn duplicate_slides(&mut self) {
        self.checkpoint();
        let picked = self.selected_slides();
        // From the back, so every index still names its slide; each copy
        // lands right after its original.
        for &i in picked.iter().rev() {
            let _ = self.deck.duplicate_slide(i);
        }
        let last_copy = picked.last().map_or(self.current + 1, |&l| l + picked.len());
        self.go_to(last_copy);
    }

    /// Deletes the selected slides (the deck keeps one).
    pub fn delete_slides(&mut self) {
        self.checkpoint();
        let picked = self.selected_slides();
        let first = picked.first().copied().unwrap_or(self.current);
        self.deck.delete_slides(&picked);
        self.go_to(first.min(self.deck.slides.len().saturating_sub(1)));
    }

    /// The rail's Move: slide `dragged` - and the other selected slides when
    /// it is one of them - to stand before the slide at `target`.
    pub fn move_slides(&mut self, dragged: usize, target: usize) {
        let moving: Vec<usize> = if self.rail.contains(dragged as u64) {
            self.selected_slides()
        } else {
            vec![dragged]
        };
        let current_id = self.deck.slides.get(dragged).map(|s| s.id);
        self.checkpoint();
        let now = self.deck.move_slides(&moving, target);
        if now.is_empty() {
            return;
        }
        self.current = current_id
            .and_then(|id| self.deck.slides.iter().position(|s| s.id == id))
            .unwrap_or(now[0]);
        // The moved slides stay selected.
        self.rail
            .select_keys(now.iter().map(|&i| i as u64).collect::<Vec<u64>>());
        self.selection.clear();
        self.editing = None;
    }

    pub fn apply_layout(&mut self, layout: LayoutKind) {
        self.checkpoint();
        let i = self.current;
        self.deck.apply_layout(i, layout);
        self.selection.clear();
        self.editing = None;
    }

    pub fn reset_slide(&mut self) {
        self.checkpoint();
        let i = self.current;
        self.deck.reset_slide(i);
    }

    /// Starts a section titled `title` at the current slide (or renames it).
    pub fn add_section(&mut self, title: &str) {
        self.checkpoint();
        self.slide_mut().section = Some(title.to_string());
    }

    /// Folds or unfolds the section starting at slide `index` in the rail.
    pub fn toggle_fold(&mut self, index: usize) {
        let Some(id) = self.deck.slides.get(index).map(|s| s.id) else {
            return;
        };
        if let Some(at) = self.folded.iter().position(|&f| f == id) {
            self.folded.remove(at);
        } else {
            self.folded.push(id);
        }
    }

    /// Hides (or shows again) the selected slides from the show.
    pub fn toggle_hidden(&mut self) {
        self.checkpoint();
        let hide = !self.slide().hidden;
        for i in self.selected_slides() {
            if let Some(s) = self.deck.slides.get_mut(i) {
                s.hidden = hide;
            }
        }
    }

    // ==== Elements ====

    /// The selected elements' indices on the current slide, in z-order.
    #[must_use]
    pub fn selected_indices(&self) -> Vec<usize> {
        self.slide()
            .elements
            .iter()
            .enumerate()
            .filter(|(_, e)| self.selection.contains(e.id))
            .map(|(i, _)| i)
            .collect()
    }

    /// The canvas's Select on element `index`: Shift / Ctrl add or toggle it.
    pub fn select_element(&mut self, index: usize, shift: bool, ctrl: bool) {
        let Some(id) = self.slide().elements.get(index).map(|e| e.id) else {
            return;
        };
        // A canvas has no order to range over: Shift toggles like Ctrl
        // (PowerPoint).
        if shift || ctrl {
            self.selection.toggle(id);
        } else {
            self.selection.click(id);
        }
        if self.editing.is_some_and(|e| !self.selection.contains(e) || self.selection.len() > 1) {
            self.editing = None;
        }
    }

    /// Selects exactly the elements at `indices` (a marquee).
    pub fn select_indices(&mut self, indices: &[usize]) {
        let ids: Vec<u64> = indices
            .iter()
            .filter_map(|&i| self.slide().elements.get(i).map(|e| e.id))
            .collect();
        self.selection.select_keys(ids);
        self.editing = None;
    }

    /// Selects nothing, edits nothing.
    pub fn clear_selection(&mut self) {
        self.selection.clear();
        self.editing = None;
    }

    /// The canvas's Transform / Commit / Nudge: the elements at `indices`
    /// take `frames`. The first step of a drag takes the undo snapshot;
    /// `commit` ends the drag.
    pub fn transform(&mut self, indices: &[usize], frames: &[Frame], commit: bool) {
        // Every step of one drag is the same undo step: the deck is cloned
        // only for the first.
        let Self { history, deck, .. } = self;
        history.checkpoint_with("Move", Some("transform"), || deck.clone());
        self.dirty = true;
        let slide = self.slide_mut();
        for (&i, f) in indices.iter().zip(frames) {
            if let Some(e) = slide.elements.get_mut(i) {
                e.set_frame(*f);
            }
        }
        if commit {
            self.history.seal();
        }
    }

    /// The canvas's Activate on element `index`: edit its text (text boxes
    /// and shapes; a shape gets an empty paragraph to type into), or a
    /// table's cells in place.
    pub fn activate(&mut self, index: usize) -> bool {
        let table = match self.slide().elements.get(index) {
            Some(e) if matches!(e.kind, ElementKind::Table { .. }) => Some(e.id),
            _ => None,
        };
        if let Some(id) = table {
            self.selection.click(id);
            self.start_editing(id);
            return true;
        }
        let Some(e) = self.slide_mut().elements.get_mut(index) else {
            return false;
        };
        let id = e.id;
        let Some(body) = e.body_mut() else {
            return false;
        };
        if body.paragraphs.is_empty() {
            body.paragraphs.push(crate::model::Paragraph::default());
        }
        if body.size <= 0.0 {
            body.size = 32.0;
        }
        self.selection.click(id);
        self.start_editing(id);
        true
    }

    /// Element `id`'s text is edited from now on (a fresh editor state).
    fn start_editing(&mut self, id: u64) {
        self.editing = Some(id);
        self.text = None;
    }

    /// Ends the text editing (the text was synced by the caller).
    pub fn stop_editing(&mut self) {
        self.editing = None;
        self.text = None;
    }

    /// Cell `row`, `col` of table `id` on the current slide takes `text`
    /// (typed in place). Whether it changed.
    pub fn set_cell(&mut self, id: u64, row: usize, col: usize, text: &str) -> bool {
        let Some(cell) = self.table_rows_mut(id).and_then(|rows| rows.get_mut(row)?.get_mut(col)) else {
            return false;
        };
        if cell.as_str() == text {
            return false;
        }
        *cell = text.to_string();
        self.dirty = true;
        true
    }

    /// The rows of table `id` on the current slide.
    fn table_rows_mut(&mut self, id: u64) -> Option<&mut Vec<Vec<String>>> {
        match &mut self.slide_mut().elements.iter_mut().find(|e| e.id == id)?.kind {
            ElementKind::Table { rows, .. } => Some(rows),
            _ => None,
        }
    }

    /// A row of empty cells at the end of table `id` (Tab in its last
    /// cell); the new row's index.
    pub fn add_table_row(&mut self, id: u64) -> Option<usize> {
        self.checkpoint();
        let rows = self.table_rows_mut(id)?;
        let columns = rows.iter().map(Vec::len).max().unwrap_or(1).max(1);
        rows.push(vec![String::new(); columns]);
        Some(rows.len() - 1)
    }

    /// The body of the text being edited.
    pub fn edited_body_mut(&mut self) -> Option<&mut TextBody> {
        let id = self.editing?;
        self.slide_mut()
            .elements
            .iter_mut()
            .find(|e| e.id == id)
            .and_then(|e| e.body_mut())
    }

    /// Puts `kind` on the current slide at `frame`, selected.
    pub fn insert(&mut self, kind: ElementKind, frame: Frame) -> u64 {
        self.checkpoint();
        let id = self.deck.mint();
        self.slide_mut().elements.push(Element::new(id, frame, kind));
        self.selection.click(id);
        self.editing = None;
        id
    }

    /// A `w` x `h` frame in the middle of the slide.
    #[must_use]
    pub fn centered(&self, w: f32, h: f32) -> Frame {
        let (sw, sh) = (self.deck.size.width(), self.deck.size.height());
        Frame::new((sw - w) / 2.0, (sh - h) / 2.0, w, h)
    }

    /// A shape of `kind` in the theme's accent, in the middle of the slide.
    pub fn insert_shape(&mut self, kind: ShapeKind) -> u64 {
        let (w, h) = match kind {
            ShapeKind::Line => (600.0, 40.0),
            ShapeKind::Arrow => (400.0, 160.0),
            _ => (400.0, 300.0),
        };
        let frame = self.centered(w, h);
        let accent = self.deck.theme.colors.accent;
        let mut body = TextBody {
            size: 36.0,
            valign: VAlign::Middle,
            color: Some(self.deck.theme.colors.background),
            ..TextBody::default()
        };
        body.paragraphs.push(crate::model::Paragraph {
            align: Align::Center,
            ..crate::model::Paragraph::default()
        });
        self.insert(
            ElementKind::Shape {
                shape: kind,
                fill: if kind == ShapeKind::Line { None } else { Some(accent) },
                stroke: if kind == ShapeKind::Line { Some(accent) } else { None },
                stroke_width: if kind == ShapeKind::Line { 6.0 } else { 0.0 },
                body,
            },
            frame,
        )
    }

    /// An empty text box in the middle of the slide, in editing.
    pub fn insert_text_box(&mut self) -> u64 {
        let frame = self.centered(800.0, 120.0);
        let mut body = TextBody {
            size: 36.0,
            ..TextBody::default()
        };
        body.paragraphs.push(crate::model::Paragraph::default());
        let id = self.insert(ElementKind::Text { body }, frame);
        self.start_editing(id);
        id
    }

    /// A `rows` x `cols` table with a header row.
    pub fn insert_table(&mut self, rows: usize, cols: usize) -> u64 {
        let rows = rows.max(1);
        let cols = cols.max(1);
        let cells = (0..rows)
            .map(|r| {
                (0..cols)
                    .map(|c| {
                        if r == 0 {
                            format!("Column {}", c + 1)
                        } else {
                            String::new()
                        }
                    })
                    .collect()
            })
            .collect();
        let frame = self.centered(240.0 * cols as f32, 70.0 * rows as f32);
        self.insert(ElementKind::Table { rows: cells, header: true }, frame)
    }

    pub fn insert_chart(&mut self, chart: ChartKind) -> u64 {
        let frame = self.centered(900.0, 560.0);
        self.insert(
            ElementKind::Chart {
                chart,
                title: String::from("Chart title"),
            },
            frame,
        )
    }

    pub fn insert_video(&mut self, media: &str) -> u64 {
        let frame = self.centered(960.0, 540.0);
        self.insert(
            ElementKind::Video {
                media: media.to_string(),
            },
            frame,
        )
    }

    /// A picture `media` of `w` x `h` px, fitted into 60% of the slide.
    pub fn insert_image(&mut self, media: &str, w: f32, h: f32) -> u64 {
        let (sw, sh) = (self.deck.size.width() * 0.6, self.deck.size.height() * 0.6);
        let k = if w > 0.0 && h > 0.0 { (sw / w).min(sh / h).min(1.0) } else { 1.0 };
        let (fw, fh) = if w > 0.0 && h > 0.0 { (w * k, h * k) } else { (sw, sh) };
        let frame = self.centered(fw, fh);
        self.insert(
            ElementKind::Image {
                media: media.to_string(),
                fit: ImageFit::Contain,
            },
            frame,
        )
    }

    /// The selected pictures are fitted by `fit` (one undo step).
    pub fn set_image_fit(&mut self, fit: ImageFit) {
        let ids = self.selected_ids();
        let pictures = |e: &Element| ids.contains(&e.id) && matches!(e.kind, ElementKind::Image { .. });
        if !self.slide().elements.iter().any(pictures) {
            return;
        }
        self.checkpoint();
        for e in self.slide_mut().elements.iter_mut().filter(|e| pictures(e)) {
            if let ElementKind::Image { fit: f, .. } = &mut e.kind {
                *f = fit;
            }
        }
    }

    /// Deletes the selected elements.
    pub fn delete_selection(&mut self) {
        if self.selection.is_empty() {
            return;
        }
        self.checkpoint();
        let ids = self.selected_ids();
        self.slide_mut().remove(&ids);
        self.clear_selection();
    }

    /// Copies the selected elements.
    pub fn copy(&mut self) {
        let picked: Vec<Element> = self
            .slide()
            .elements
            .iter()
            .filter(|e| self.selection.contains(e.id))
            .cloned()
            .collect();
        if !picked.is_empty() {
            self.clipboard = picked;
            self.paste_count = 0;
        }
    }

    pub fn cut(&mut self) {
        self.copy();
        self.delete_selection();
    }

    /// Pastes the clipboard onto the current slide, each paste a step
    /// further down and right, selected.
    pub fn paste(&mut self) {
        if self.clipboard.is_empty() {
            return;
        }
        self.checkpoint();
        self.paste_count += 1;
        let offset = PASTE_OFFSET * self.paste_count as f32;
        let mut ids = Vec::new();
        for e in self.clipboard.clone() {
            let mut copy = self.deck.copy_element(&e);
            let f = copy.frame.translated(offset, offset);
            copy.set_frame(f);
            ids.push(copy.id);
            self.slide_mut().elements.push(copy);
        }
        self.selection.select_keys(ids);
        self.editing = None;
    }

    /// Ctrl+D: copies of the selection, offset, selected (the clipboard
    /// stays as it was).
    pub fn duplicate(&mut self) {
        let picked: Vec<Element> = self
            .slide()
            .elements
            .iter()
            .filter(|e| self.selection.contains(e.id))
            .cloned()
            .collect();
        if picked.is_empty() {
            return;
        }
        self.checkpoint();
        let mut ids = Vec::new();
        for e in picked {
            let mut copy = self.deck.copy_element(&e);
            let f = copy.frame.translated(PASTE_OFFSET, PASTE_OFFSET);
            copy.set_frame(f);
            ids.push(copy.id);
            self.slide_mut().elements.push(copy);
        }
        self.selection.select_keys(ids);
    }

    pub fn arrange(&mut self, how: ZOrder) {
        if self.selection.is_empty() {
            return;
        }
        self.checkpoint();
        let ids = self.selected_ids();
        self.slide_mut().reorder(&ids, how);
    }

    pub fn group(&mut self) {
        if self.selection.len() < 2 {
            return;
        }
        self.checkpoint();
        let gid = self.deck.mint();
        let ids = self.selected_ids();
        if let Some(g) = self.slide_mut().group(&ids, gid) {
            self.selection.click(g);
        }
    }

    pub fn ungroup(&mut self) {
        let groups: Vec<u64> = self
            .slide()
            .elements
            .iter()
            .filter(|e| self.selection.contains(e.id) && matches!(e.kind, ElementKind::Group { .. }))
            .map(|e| e.id)
            .collect();
        if groups.is_empty() {
            return;
        }
        self.checkpoint();
        let mut ids = Vec::new();
        for g in groups {
            ids.extend(self.slide_mut().ungroup(g));
        }
        self.selection.select_keys(ids);
    }

    /// Applies `f` to every selected element's text body (one undo step).
    pub fn with_bodies(&mut self, f: impl Fn(&mut TextBody)) {
        if self.selection.is_empty() {
            return;
        }
        self.checkpoint();
        let ids = self.selected_ids();
        for e in self.slide_mut().elements.iter_mut().filter(|e| ids.contains(&e.id)) {
            if let Some(body) = e.body_mut() {
                f(body);
            }
        }
    }

    pub fn set_align(&mut self, align: Align) {
        self.with_bodies(|b| {
            for p in &mut b.paragraphs {
                p.align = align;
            }
        });
    }

    pub fn toggle_bullets(&mut self) {
        self.with_bodies(|b| {
            let on = !b.paragraphs.iter().all(|p| p.bullet);
            for p in &mut b.paragraphs {
                p.bullet = on;
            }
        });
    }

    /// Indents (`delta` > 0) or outdents every paragraph, levels 0..=4.
    pub fn indent(&mut self, delta: i8) {
        self.with_bodies(|b| {
            for p in &mut b.paragraphs {
                p.level = (p.level as i8 + delta).clamp(0, 4) as u8;
            }
        });
    }

    /// Grows (`delta` > 0) or shrinks the text by `delta` units, at least 8.
    pub fn grow_text(&mut self, delta: f32) {
        self.with_bodies(|b| b.size = (b.size + delta).max(8.0));
    }

    pub fn set_text_color(&mut self, color: Option<Color>) {
        self.with_bodies(|b| b.color = color);
    }

    pub fn set_font(&mut self, font: Option<String>) {
        self.with_bodies(|b| b.font = font.clone());
    }

    /// The selected shapes' fill.
    pub fn set_fill(&mut self, color: Option<Color>) {
        self.for_shapes(|fill, _, _| *fill = color);
    }

    /// The selected shapes' outline.
    pub fn set_outline(&mut self, color: Option<Color>, width: f32) {
        self.for_shapes(|_, stroke, w| {
            *stroke = color;
            *w = width;
        });
    }

    fn for_shapes(&mut self, f: impl Fn(&mut Option<Color>, &mut Option<Color>, &mut f32)) {
        if self.selection.is_empty() {
            return;
        }
        self.checkpoint();
        let ids = self.selected_ids();
        for e in self.slide_mut().elements.iter_mut().filter(|e| ids.contains(&e.id)) {
            if let ElementKind::Shape {
                fill,
                stroke,
                stroke_width,
                ..
            } = &mut e.kind
            {
                f(fill, stroke, stroke_width);
            }
        }
    }

    // ==== Transitions and builds ====

    pub fn set_transition(&mut self, kind: TransitionKind) {
        self.checkpoint();
        for i in self.selected_slides() {
            if let Some(s) = self.deck.slides.get_mut(i) {
                s.transition.kind = kind;
            }
        }
    }

    /// Lengthens (`delta` > 0) or shortens the transition, 100..=5000 ms.
    pub fn transition_duration(&mut self, delta: i32) {
        self.checkpoint();
        let t = &mut self.slide_mut().transition;
        t.duration_ms = (t.duration_ms as i32 + delta).clamp(100, 5000) as u32;
    }

    pub fn transition_to_all(&mut self) {
        self.checkpoint();
        let t = self.slide().transition;
        for s in &mut self.deck.slides {
            s.transition = t;
        }
    }

    /// Gives the selection the build `effect` (the next click after the
    /// slide's last build), or takes it away (`None`).
    pub fn set_animation(&mut self, effect: Option<AnimationEffect>) {
        if self.selection.is_empty() {
            return;
        }
        self.checkpoint();
        let ids = self.selected_ids();
        let slide = self.slide_mut();
        let mut next = slide
            .elements
            .iter()
            .filter_map(|e| e.animation.map(|a| a.order + 1))
            .max()
            .unwrap_or(0);
        for e in slide.elements.iter_mut().filter(|e| ids.contains(&e.id)) {
            e.animation = match (effect, e.animation) {
                (None, _) => None,
                (Some(effect), Some(old)) => Some(Animation { effect, ..old }),
                (Some(effect), None) => {
                    let a = Animation {
                        effect,
                        order: next,
                        duration_ms: 500,
                    };
                    next += 1;
                    Some(a)
                }
            };
        }
    }

    /// Moves the selected element's build one click earlier (`delta` < 0)
    /// or later.
    pub fn move_animation(&mut self, delta: i32) {
        let Some(&id) = self.selection.keys.as_ref().first() else {
            return;
        };
        let steps = self.slide().build_steps();
        let Some(at) = steps.iter().position(|s| s.contains(&id)) else {
            return;
        };
        let to = at as i32 + delta;
        if to < 0 || to as usize >= steps.len() {
            return;
        }
        self.checkpoint();
        let (a, b) = (steps[at].clone(), steps[to as usize].clone());
        let slide = self.slide_mut();
        let order_a = slide.element(a[0]).and_then(|e| e.animation).map(|x| x.order);
        let order_b = slide.element(b[0]).and_then(|e| e.animation).map(|x| x.order);
        if let (Some(oa), Some(ob)) = (order_a, order_b) {
            for e in slide.elements.iter_mut() {
                if let Some(anim) = e.animation.as_mut() {
                    if a.contains(&e.id) {
                        anim.order = ob;
                    } else if b.contains(&e.id) {
                        anim.order = oa;
                    }
                }
            }
        }
    }

    // ==== Design ====

    pub fn set_theme(&mut self, theme: Theme) {
        self.checkpoint();
        self.deck.theme = theme;
    }

    pub fn set_size(&mut self, size: SlideSize) {
        if size == self.deck.size {
            return;
        }
        self.checkpoint();
        self.deck.set_size(size);
    }

    /// The current slide's ground (`None`: the theme's); `all` for every slide.
    pub fn set_background(&mut self, background: Option<Background>, all: bool) {
        self.checkpoint();
        if all {
            for s in &mut self.deck.slides {
                s.background = background;
            }
        } else {
            self.slide_mut().background = background;
        }
    }

    // ==== Notes and outline ====

    /// The current slide's notes (no undo step per keystroke).
    pub fn set_notes(&mut self, notes: &str) {
        if self.slide().notes != notes {
            self.slide_mut().notes = notes.to_string();
            self.dirty = true;
        }
    }

    /// The outline view's edit of slide `index`'s title or body text.
    pub fn set_placeholder_text(&mut self, index: usize, role: crate::model::PlaceholderRole, text: &str) {
        let Some(slide) = self.deck.slides.get_mut(index) else {
            return;
        };
        let Some(body) = slide
            .elements
            .iter_mut()
            .find(|e| e.placeholder == Some(role))
            .and_then(Element::body_mut)
        else {
            return;
        };
        if body.text() != text {
            body.set_text(text);
            self.dirty = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{sample_deck, PlaceholderRole};

    fn editor() -> Editor {
        Editor::new(sample_deck("t", Theme::office()))
    }

    #[test]
    fn a_new_slide_goes_after_the_current_one_and_undo_takes_it_back() {
        let mut ed = editor();
        ed.go_to(2);
        let n = ed.deck.slides.len();
        ed.new_slide(LayoutKind::TwoContent);
        assert_eq!(ed.current, 3);
        assert_eq!(ed.deck.slides.len(), n + 1);
        assert_eq!(ed.slide().layout, LayoutKind::TwoContent);
        assert!(ed.dirty);
        assert!(ed.undo());
        assert_eq!(ed.deck.slides.len(), n);
        assert!(ed.redo());
        assert_eq!(ed.deck.slides.len(), n + 1);
    }

    #[test]
    fn the_rail_selects_a_range_with_shift_and_toggles_with_ctrl() {
        let mut ed = editor();
        ed.rail_select(2, false, false);
        ed.rail_select(4, true, false);
        assert_eq!(ed.selected_slides(), vec![2, 3, 4]);
        assert_eq!(ed.current, 4);
        ed.rail_select(3, false, true);
        assert_eq!(ed.selected_slides(), vec![2, 4]);
        ed.rail_select(7, false, true);
        assert_eq!(ed.selected_slides(), vec![2, 4, 7]);
        assert_eq!(ed.current, 7);
    }

    #[test]
    fn dragging_a_selected_slide_moves_the_whole_selection_and_keeps_it_selected() {
        let mut ed = editor();
        let ids: Vec<u64> = ed.deck.slides.iter().map(|s| s.id).collect();
        ed.rail_select(1, false, false);
        ed.rail_select(2, true, false);
        ed.move_slides(2, 6);
        let now: Vec<u64> = ed.deck.slides.iter().map(|s| s.id).collect();
        assert_eq!(&now[..6], &[ids[0], ids[3], ids[4], ids[5], ids[1], ids[2]]);
        assert_eq!(ed.selected_slides(), vec![4, 5]);
        assert_eq!(ed.current, 5, "the dragged slide stays current");
        // A slide outside the selection moves alone.
        ed.move_slides(0, 2);
        assert_eq!(ed.deck.slides[1].id, ids[0]);
    }

    #[test]
    fn a_drag_on_the_canvas_is_one_undo_step() {
        let mut ed = editor();
        ed.go_to(5);
        ed.select_element(1, false, false);
        let f0 = ed.slide().elements[1].frame;
        ed.transform(&[1], &[f0.translated(10.0, 0.0)], false);
        ed.transform(&[1], &[f0.translated(20.0, 0.0)], false);
        ed.transform(&[1], &[f0.translated(30.0, 5.0)], true);
        assert_eq!(ed.slide().elements[1].frame, f0.translated(30.0, 5.0));
        assert!(ed.undo());
        assert_eq!(ed.slide().elements[1].frame, f0, "one step back undoes the whole drag");
    }

    #[test]
    fn paste_offsets_each_copy_and_selects_it() {
        let mut ed = editor();
        ed.go_to(5);
        ed.select_element(1, false, false);
        let original = ed.slide().elements[1].clone();
        ed.copy();
        ed.paste();
        ed.paste();
        let n = ed.slide().elements.len();
        let second = &ed.slide().elements[n - 1];
        assert_eq!(second.frame, original.frame.translated(40.0, 40.0));
        assert_ne!(second.id, original.id);
        assert_eq!(ed.selected_ids(), vec![second.id]);
    }

    #[test]
    fn grouping_the_selection_selects_the_group_and_ungrouping_its_members() {
        let mut ed = editor();
        ed.go_to(5);
        ed.select_element(1, false, false);
        ed.select_element(2, true, false);
        let members = ed.selected_ids();
        ed.group();
        assert_eq!(ed.selection.len(), 1);
        assert!(matches!(
            ed.slide().element(ed.selected_ids()[0]).map(|e| &e.kind),
            Some(ElementKind::Group { .. })
        ));
        ed.ungroup();
        let mut back = ed.selected_ids();
        back.sort_unstable();
        let mut want = members;
        want.sort_unstable();
        assert_eq!(back, want);
    }

    #[test]
    fn an_inserted_shape_sits_in_the_middle_selected() {
        let mut ed = editor();
        let id = ed.insert_shape(ShapeKind::Ellipse);
        let e = ed.slide().element(id).expect("the shape");
        assert_eq!(e.frame, Frame::new(760.0, 390.0, 400.0, 300.0));
        assert_eq!(ed.selected_ids(), vec![id]);
        let tb = ed.insert_text_box();
        assert_eq!(ed.editing, Some(tb), "a new text box is typed into at once");
    }

    #[test]
    fn builds_take_the_next_click_and_move_earlier() {
        let mut ed = editor();
        ed.go_to(5);
        ed.select_element(1, false, false);
        ed.set_animation(Some(AnimationEffect::Fade));
        ed.select_element(2, false, false);
        ed.set_animation(Some(AnimationEffect::FlyIn));
        let a = ed.slide().elements[1].id;
        let b = ed.slide().elements[2].id;
        assert_eq!(ed.slide().build_steps(), vec![vec![a], vec![b]]);
        ed.move_animation(-1);
        assert_eq!(ed.slide().build_steps(), vec![vec![b], vec![a]]);
        ed.set_animation(None);
        assert_eq!(ed.slide().build_steps(), vec![vec![a]]);
    }

    #[test]
    fn activating_a_text_edits_it_and_a_picture_does_not() {
        let mut ed = editor();
        assert!(ed.activate(0));
        assert_eq!(ed.editing, Some(ed.slide().elements[0].id));
        let pic = ed.insert_image("media/a.png", 800.0, 600.0);
        let at = ed.slide().index_of(pic).expect("the picture");
        assert!(!ed.activate(at));
    }

    #[test]
    fn a_table_is_edited_in_place_cell_by_cell() {
        let mut ed = editor();
        let table = ed.insert_table(3, 2);
        let at = ed.slide().index_of(table).expect("the table");
        assert!(ed.activate(at), "a double-click on a table edits its cells");
        assert_eq!(ed.editing, Some(table));
        let cells = |ed: &Editor| match &ed.slide().element(table).expect("the table").kind {
            ElementKind::Table { rows, .. } => rows.clone(),
            _ => Vec::new(),
        };
        ed.dirty = false;
        assert!(ed.set_cell(table, 1, 0, "Budget"));
        assert!(ed.dirty, "a typed cell is an edit of the deck");
        assert_eq!(cells(&ed)[1][0], "Budget");
        assert!(!ed.set_cell(table, 1, 0, "Budget"), "the same text is no change");
        assert!(!ed.set_cell(table, 9, 0, "x"), "no such cell");
        assert_eq!(ed.add_table_row(table), Some(3), "Tab in the last cell adds a row");
        assert_eq!(cells(&ed).len(), 4);
        assert_eq!(cells(&ed)[3], vec![String::new(), String::new()]);
        ed.stop_editing();
        assert_eq!(ed.editing, None);
    }

    #[test]
    fn the_selected_picture_takes_the_fit_and_undo_restores_it() {
        let mut ed = editor();
        let pic = ed.insert_image("media/a.png", 800.0, 600.0);
        ed.select_indices(&[ed.slide().index_of(pic).expect("the picture")]);
        ed.set_image_fit(ImageFit::Cover);
        let fit = |ed: &Editor| match &ed.slide().element(pic).expect("the picture").kind {
            ElementKind::Image { fit, .. } => *fit,
            _ => panic!("not a picture"),
        };
        assert_eq!(fit(&ed), ImageFit::Cover);
        assert!(ed.undo());
        assert_eq!(fit(&ed), ImageFit::Contain);
    }

    #[test]
    fn the_outline_edits_the_title_and_the_body() {
        let mut ed = editor();
        ed.set_placeholder_text(1, PlaceholderRole::Title, "Plan");
        assert_eq!(ed.deck.slides[1].title(), "Plan");
        ed.set_placeholder_text(1, PlaceholderRole::Body, "a\nb");
        let body = ed.deck.slides[1]
            .placeholder(PlaceholderRole::Body)
            .and_then(Element::body)
            .map(TextBody::text);
        assert_eq!(body.as_deref(), Some("a\nb"));
    }
}
