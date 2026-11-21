//! A5, the label flicker on the GPU path ("labels flicker one by one").
//!
//! A recorder of what each frame hands the renderer, without a renderer: the
//! images a frame's transaction adds and deletes, and how the frame's display
//! list paints the halo of each label - with the label's own picture, with one
//! that stands in for it, or not at all (the shadow went to WebRender's own,
//! which has no spread: the label is painted without its halo). The frames are
//! made the way the shells make them: `register_frame_resources` (the scan that
//! draws halos within the frame's budget, and the image GC), then the
//! translation of the list.
//!
//! The invariants the flicker broke (audit/A5.md, A5-01 and A5-02):
//! - no frame's list names an image that is not registered in that frame;
//! - a label that is painted with its own halo is painted with it in every
//!   later frame of the same view;
//! - a label with a spread halo is painted with a halo in every frame;
//! - an unchanged view comes to rest: no image added or deleted, no halo
//!   drawn, no frame owed.

use std::collections::{BTreeMap, HashSet};

use azul_core::{
    dom::{Dom, DomId},
    geom::LogicalSize,
    resources::{DpiScaleFactor, RendererResources},
    styled_dom::StyledDom,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    cpurender::{text_halo_bounds_in_this_thread, text_halo_stats, text_halos, TextHaloStats},
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;
use webrender::{
    api::{units::DeviceIntSize, DisplayItem, ImageKey, PipelineId},
    render_api::{ResourceUpdate, Transaction},
};

use crate::desktop::{
    compositor2::translate_displaylist_to_wr,
    wr_translate2::{
        draw_every_halo_in_this_thread, draw_this_many_halos_a_frame_in_this_thread,
        register_frame_resources,
    },
};

/// How a frame paints one label's halo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Halo {
    /// With the label's own picture.
    Own(ImageKey),
    /// With a picture that stands in for it: the same text at another size.
    StandIn(ImageKey),
    /// Not at all: WebRender's own shadow, which has no spread.
    None,
}

/// What one frame handed to the renderer.
#[derive(Debug)]
struct Frame {
    /// The images its transaction added and deleted.
    added: Vec<ImageKey>,
    deleted: Vec<ImageKey>,
    /// Each label's halo, in list order.
    halos: Vec<Halo>,
    /// What the frame's scan did with the kept halos.
    stats: TextHaloStats,
    /// Whether the engine owes a frame for the halos it left.
    owed: bool,
}

impl Frame {
    /// The labels painted with their own picture.
    fn own(&self) -> Vec<usize> {
        (0..self.halos.len())
            .filter(|i| matches!(self.halos[*i], Halo::Own(_)))
            .collect()
    }

    /// Whether the frame changed nothing the renderer holds and owes nothing.
    fn at_rest(&self) -> bool {
        self.added.is_empty()
            && self.deleted.is_empty()
            && !self.owed
            && self.stats.created + self.stats.previews + self.stats.evicted == 0
    }
}

/// A window whose frames are recorded.
struct Recorder {
    window: LayoutWindow,
    dpi: f32,
    /// The images the renderer holds: added and not deleted.
    live: HashSet<ImageKey>,
}

impl Recorder {
    fn new(dpi: f32) -> Self {
        Self {
            window: LayoutWindow::new(FcFontCache::build()).expect("a layout window"),
            dpi,
            live: HashSet::new(),
        }
    }

    /// The window shows `dom` from the next frame on.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // 96 or 192
    fn lay_out(&mut self, dom: Dom) {
        let mut state = FullWindowState::default();
        state.size.dimensions = LogicalSize::new(1000.0, 1000.0);
        state.size.dpi = (96.0 * self.dpi) as u32;
        self.window.current_window_state = state.clone();
        self.window
            .layout_and_generate_display_list(
                StyledDom::create_from_dom(dom),
                &state,
                &RendererResources::default(),
                &ExternalSystemCallbacks::rust_internal(),
                &mut None,
            )
            .expect("the labels lay out");
    }

    /// One frame of what the window shows. Fails when its list names an image
    /// the renderer does not hold.
    fn frame(&mut self) -> Frame {
        let dpi = DpiScaleFactor::new(self.dpi);
        let mut txn = Transaction::new();
        let _ = register_frame_resources(&mut self.window, &mut txn, dpi);
        let (mut added, mut deleted) = (Vec::new(), Vec::new());
        for update in &txn.resource_updates {
            match update {
                ResourceUpdate::AddImage(add) => {
                    added.push(add.key);
                    self.live.insert(add.key);
                }
                ResourceUpdate::DeleteImage(key) => {
                    deleted.push(*key);
                    self.live.remove(key);
                }
                _ => {}
            }
        }
        let list = self
            .window
            .get_layout_result(&DomId::ROOT_ID)
            .expect("the root DOM is laid out")
            .display_list
            .clone();
        let (_, built) = translate_displaylist_to_wr(
            &list,
            PipelineId(0, 0),
            DeviceIntSize::new(2000, 2000),
            &self.window.renderer_resources,
            dpi,
            Vec::new(),
            &BTreeMap::new(),
            0,
        )
        .expect("the frame translates");
        // The shadows painted as pictures, in list order: own or standing in.
        let stood: Vec<bool> = text_halos(&list.items, self.dpi, None)
            .iter()
            .map(|halo| halo.stand_in)
            .collect();
        let mut halos = Vec::new();
        let mut pictures = 0;
        let mut items = built.iter();
        while let Some(item) = items.next() {
            match item.item() {
                DisplayItem::Image(image) => {
                    assert!(
                        self.live.contains(&image.image_key),
                        "the list names {:?}, which the renderer does not hold",
                        image.image_key
                    );
                    assert!(pictures < stood.len(), "more pictures than kept halos");
                    halos.push(if stood[pictures] {
                        Halo::StandIn(image.image_key)
                    } else {
                        Halo::Own(image.image_key)
                    });
                    pictures += 1;
                }
                DisplayItem::PushShadow(_) => halos.push(Halo::None),
                _ => {}
            }
        }
        assert_eq!(pictures, stood.len(), "every kept halo of the list is painted as its picture");
        let frame = Frame {
            added,
            deleted,
            halos,
            stats: text_halo_stats(),
            owed: self.window.gpu_state_manager.halos_owed,
        };
        // The shells go on to the next epoch after a frame with lists.
        self.window.epoch.increment();
        frame
    }
}

/// `count` labels of a map as the editor draws them: absolutely placed, each
/// with a solid halo (a text shadow that is a spread and nothing else), at
/// `zoom` times their size and place.
fn labels(count: usize, zoom: f32) -> Dom {
    let mut page = Dom::create_div().with_css(
        format!(
            "position: relative; width: {}px; height: {}px; background: #4477aa;",
            600.0 * zoom,
            600.0 * zoom
        )
        .as_str(),
    );
    for i in 0..count {
        let style = format!(
            "position: absolute; left: {}px; top: {}px; margin: 0px; font-size: {}px; \
             line-height: 1; color: #111111; white-space: nowrap; \
             text-shadow: 0px 0px 0px {}px #ffffff;",
            20.0 * zoom,
            (10.0 + 24.0 * i as f32) * zoom,
            12.0 * zoom,
            1.5 * zoom,
        );
        page = page.with_child(
            Dom::create_p()
                .with_css(style.as_str())
                .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(format!(
                    "Street {i}"
                ))),
        );
    }
    Dom::create_body().with_css("margin: 0px;").with_child(page)
}

/// The bytes the own pictures of `labels(count, 1.0)` hold at `dpi`, measured on
/// a thread of its own (the kept halos are a thread's).
fn bytes_of_the_views_pictures(count: usize, dpi: f32) -> usize {
    std::thread::spawn(move || {
        draw_every_halo_in_this_thread();
        let mut view = Recorder::new(dpi);
        view.lay_out(labels(count, 1.0));
        let frame = view.frame();
        assert_eq!(frame.own().len(), count, "premise: a frame with no budget draws every halo");
        usize::try_from(text_halo_stats().kept_bytes).expect("bytes")
    })
    .join()
    .expect("the measuring thread")
}

/// A5-01, the flicker. A view whose halos the cache cannot all hold - here 24
/// labels and room for eight tenths of the bytes of their pictures; on the user's
/// 2x display the Tuebingen view's against 32 MB - painted, frame after frame:
///
/// before, each frame drew the halos it lacked and for each of them dropped the
/// one used longest ago, which the same frame had used first; the next frame drew
/// those again. Per frame some labels lost their halo and others got theirs
/// back, in turn through the list, and the window never came to rest by itself.
///
/// Now the labels that have their own picture keep it (the list's first ones:
/// the set only grows), every other label is painted with a small picture that
/// stands in, and when the cache is full of what the frame paints the view is at
/// rest: nothing drawn, nothing dropped, no image added or deleted, no frame owed.
fn a_view_that_does_not_fit_keeps_its_halos_and_comes_to_rest(dpi: f32) {
    const LABELS: usize = 24;
    let all = bytes_of_the_views_pictures(LABELS, dpi);
    text_halo_bounds_in_this_thread(usize::MAX, all * 8 / 10);
    draw_this_many_halos_a_frame_in_this_thread(3);

    let mut view = Recorder::new(dpi);
    view.lay_out(labels(LABELS, 1.0));
    let first = view.frame();
    assert_eq!(first.own(), [0, 1, 2], "the first frame draws what its budget allows");
    assert!(first.owed, "and owes a frame for the rest");

    let mut before = first;
    let mut resting = 0;
    for n in 1..60 {
        let now = view.frame();
        assert!(
            now.halos.iter().all(|halo| *halo != Halo::None),
            "frame {n}: a label is painted without its halo: {:?}",
            now.halos
        );
        assert!(
            now.own().starts_with(&before.own()),
            "frame {n}: a label that had its own halo lost it: {:?} after {:?}",
            now.own(),
            before.own()
        );
        resting = if now.at_rest() { resting + 1 } else { 0 };
        before = now;
        if resting == 5 {
            break;
        }
    }
    assert_eq!(resting, 5, "the view came to rest: {:?}", before.stats);
    let own = before.own().len();
    assert!(own > 3 && own < LABELS, "premise: the cache holds some of the view's halos: {own}");
    assert_eq!(before.stats.unkept as usize, LABELS - own, "the others have no room");
    assert_eq!(before.stats.stand_ins as usize, LABELS - own, "and are painted with a stand-in");
    assert_eq!(before.stats.pending, 0, "none is left for a later frame");
}

#[test]
fn a5_a_view_that_does_not_fit_keeps_its_halos_and_comes_to_rest_at_1x() {
    a_view_that_does_not_fit_keeps_its_halos_and_comes_to_rest(1.0);
}

/// The user's display: every picture has four times the bytes.
#[test]
fn a5_a_view_that_does_not_fit_keeps_its_halos_and_comes_to_rest_at_2x() {
    a_view_that_does_not_fit_keeps_its_halos_and_comes_to_rest(2.0);
}

/// A5-02, the wave after a zoom. A zoom that settles lays every label out at a
/// new size: every halo is a new picture, and a frame draws a bounded number.
/// Before, the labels the frame's budget left were painted without halo (all
/// but a few, in the settle frame) and got it back a few per frame. Now each
/// keeps the halo it had - the old picture, stretched to the new size - until
/// its own is drawn: no frame paints a label without halo, the settle frame
/// adds only the pictures it drew, and when the view is at rest the renderer
/// holds the new pictures and none of the old.
fn a_zoom_that_settles_keeps_every_halo(dpi: f32) {
    const LABELS: usize = 12;
    draw_this_many_halos_a_frame_in_this_thread(4);
    let mut view = Recorder::new(dpi);
    view.lay_out(labels(LABELS, 1.0));
    let mut frames = vec![view.frame()];
    while frames.last().is_some_and(|frame| frame.owed) {
        assert!(frames.len() < 20, "the halos come");
        frames.push(view.frame());
    }
    let settled = frames.last().expect("a frame");
    assert_eq!(settled.own().len(), LABELS, "every label has its own halo");
    for (n, frame) in frames.iter().enumerate() {
        assert!(
            frame.halos.iter().all(|halo| *halo != Halo::None),
            "frame {n} of the first view: a label without halo: {:?}",
            frame.halos
        );
    }
    let old: HashSet<ImageKey> = settled
        .halos
        .iter()
        .filter_map(|halo| match halo {
            Halo::Own(key) => Some(*key),
            _ => None,
        })
        .collect();

    // The zoom settles at 1.5 times the size.
    view.lay_out(labels(LABELS, 1.5));
    let settle = view.frame();
    assert_eq!(settle.own(), [0, 1, 2, 3], "the settle frame draws what its budget allows");
    assert_eq!(settle.added.len(), 4, "and adds those pictures and no other");
    assert_eq!(settle.stats.previews, 0, "no small picture is needed: the old ones stand in");
    for (label, halo) in settle.halos.iter().enumerate().skip(4) {
        assert!(
            matches!(halo, Halo::StandIn(key) if old.contains(key)),
            "label {label} keeps its old halo until its own is drawn: {halo:?}"
        );
    }
    let mut before = settle;
    let mut resting = 0;
    for n in 1..40 {
        let now = view.frame();
        assert!(
            now.halos.iter().all(|halo| *halo != Halo::None),
            "frame {n} after the settle: a label without halo: {:?}",
            now.halos
        );
        assert!(now.own().starts_with(&before.own()), "frame {n}: an own halo was lost");
        resting = if now.at_rest() { resting + 1 } else { 0 };
        before = now;
        if resting == 5 {
            break;
        }
    }
    assert_eq!(resting, 5, "the view came to rest: {:?}", before.stats);
    assert_eq!(before.own().len(), LABELS, "every label has its own halo at the new size");
    let new: HashSet<ImageKey> = before
        .halos
        .iter()
        .filter_map(|halo| match halo {
            Halo::Own(key) => Some(*key),
            _ => None,
        })
        .collect();
    assert_eq!(new.len(), LABELS);
    assert!(new.is_disjoint(&old), "new pictures, not the old ones");
    assert_eq!(view.live, new, "the renderer holds the view's pictures and no picture of a frame gone");
}

#[test]
fn a5_a_zoom_that_settles_keeps_every_halo_at_1x() {
    a_zoom_that_settles_keeps_every_halo(1.0);
}

#[test]
fn a5_a_zoom_that_settles_keeps_every_halo_at_2x() {
    a_zoom_that_settles_keeps_every_halo(2.0);
}
