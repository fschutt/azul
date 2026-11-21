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

use std::{
    collections::{BTreeMap, HashSet},
    sync::Arc,
};

use azul_core::{
    dom::{Dom, DomId},
    geom::LogicalSize,
    gl::OptionGlContextPtr,
    resources::{DpiScaleFactor, RendererResources},
    styled_dom::StyledDom,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    cpurender::{TextHaloStats, TextHalosReleased},
    managers::gpu_state::HaloRelease,
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
        build_full_transaction, build_lightweight_transaction, draw_every_halo_in_this_thread,
        draw_this_many_halos_a_frame_in_this_thread, register_frame_resources,
    },
};

/// How a frame paints one label's halo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Halo {
    /// With the label's own picture.
    Own(ImageKey),
    /// With a picture that stands in for it: the same text at another size.
    StandIn(ImageKey),
    /// Not at all: WebRender's own shadow, which has no spread.
    None,
}

/// What one frame handed to the renderer.
#[derive(Debug)]
pub(super) struct Frame {
    /// The images its transaction added and deleted.
    pub(super) added: Vec<ImageKey>,
    pub(super) deleted: Vec<ImageKey>,
    /// The images it gave new pixels under the key they have (audit A4: a
    /// label's own picture, drawn into the place of the one that stood in).
    pub(super) updated: Vec<ImageKey>,
    /// Each label's halo, in list order.
    pub(super) halos: Vec<Halo>,
    /// What the frame's scan did with the kept halos.
    pub(super) stats: TextHaloStats,
    /// Whether the engine owes a frame for the halos it left.
    pub(super) owed: bool,
}

impl Frame {
    /// The labels painted with their own picture.
    pub(super) fn own(&self) -> Vec<usize> {
        (0..self.halos.len())
            .filter(|i| matches!(self.halos[*i], Halo::Own(_)))
            .collect()
    }

    /// Whether the frame changed nothing the renderer holds and owes nothing.
    pub(super) fn at_rest(&self) -> bool {
        self.added.is_empty()
            && self.deleted.is_empty()
            && self.updated.is_empty()
            && !self.owed
            && self.stats.created + self.stats.previews + self.stats.evicted == 0
    }
}

/// A window whose frames are recorded.
pub(super) struct Recorder {
    pub(super) window: LayoutWindow,
    pub(super) dpi: f32,
    /// The images the renderer holds: added and not deleted.
    pub(super) live: HashSet<ImageKey>,
}

impl Recorder {
    pub(super) fn new(dpi: f32) -> Self {
        Self {
            window: LayoutWindow::new(FcFontCache::build()).expect("a layout window"),
            dpi,
            live: HashSet::new(),
        }
    }

    /// The window shows `dom` from the next frame on.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // 96 or 192
    pub(super) fn lay_out(&mut self, dom: Dom) {
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

    /// The renderer takes a transaction's images: the ones it adds, deletes
    /// and gives new pixels, in the transaction's order. Fails on an update of
    /// an image the renderer does not hold (it panics on one).
    pub(super) fn take(
        &mut self,
        txn: &Transaction,
    ) -> (Vec<ImageKey>, Vec<ImageKey>, Vec<ImageKey>) {
        let (mut added, mut deleted, mut updated) = (Vec::new(), Vec::new(), Vec::new());
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
                ResourceUpdate::UpdateImage(update) => {
                    assert!(
                        self.live.contains(&update.key),
                        "an update of {:?}, which the renderer does not hold",
                        update.key
                    );
                    updated.push(update.key);
                }
                _ => {}
            }
        }
        (added, deleted, updated)
    }

    /// One frame of what the window shows. Fails when its list names an image
    /// the renderer does not hold.
    pub(super) fn frame(&mut self) -> Frame {
        let dpi = DpiScaleFactor::new(self.dpi);
        let mut txn = Transaction::new();
        let _ = register_frame_resources(&mut self.window, &mut txn, dpi);
        let (added, deleted, updated) = self.take(&txn);
        let halos = self.painted();
        let frame = Frame {
            added,
            deleted,
            updated,
            halos,
            stats: self.window.text_halo_store.text_halo_stats(),
            owed: self.window.gpu_state_manager.halos_owed,
        };
        // The shells go on to the next epoch after a frame with lists.
        self.window.epoch.increment();
        frame
    }

    /// How a translation of the window's list made NOW paints each label's
    /// halo, in list order. Fails when it names an image the renderer does not
    /// hold.
    pub(super) fn painted(&self) -> Vec<Halo> {
        let dpi = DpiScaleFactor::new(self.dpi);
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
            &self.window.text_halo_store,
            dpi,
            Vec::new(),
            &BTreeMap::new(),
            0,
        )
        .expect("the frame translates");
        // The shadows painted as pictures, in list order: own or standing in.
        // Every picture a look at the list finds is registered. A window whose
        // pictures were given back (audit A5-20) keeps none: its store was
        // released with them, and its translation gives every shadow to the
        // renderer.
        let found = self.window.text_halo_store.text_halos(&list.items, self.dpi, None);
        if self.window.gpu_state_manager.halo_release == HaloRelease::Hidden {
            assert!(found.is_empty(), "the store of a window whose pictures were given back keeps none");
        }
        let registered = |halo: &&azul_layout::cpurender::TextHalo| {
            self.window
                .renderer_resources
                .currently_registered_images
                .contains_key(&halo.image.get_hash())
        };
        if self.window.gpu_state_manager.halo_release == HaloRelease::Kept {
            assert!(
                found.iter().all(|halo| registered(&halo)),
                "a kept halo of the list is not registered with the renderer"
            );
        }
        let stood: Vec<bool> = found
            .iter()
            .filter(registered)
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
        halos
    }

    /// The renderer takes the transaction of one pass of a shell.
    fn sent(&mut self, txn: &Transaction, frame: bool) -> Pass {
        let list = txn.first_display_list().is_some();
        let (added, deleted, updated) = self.take(txn);
        Pass {
            added,
            updated,
            deleted,
            list,
            frame,
        }
    }

    /// The pass of a frame whose layout was made again: the full transaction.
    fn full_pass(&mut self) -> Pass {
        let mut txn = Transaction::new();
        build_full_transaction(&mut txn, &mut self.window, &OptionGlContextPtr::None)
            .expect("the full transaction");
        self.sent(&txn, true)
    }

    /// The pass of a frame that laid nothing out: the lightweight transaction,
    /// which is also what the platform loop makes while the engine owes frames.
    fn light_pass(&mut self) -> Pass {
        let mut txn = Transaction::new();
        let frame =
            build_lightweight_transaction(&mut txn, &mut self.window, &OptionGlContextPtr::None)
                .expect("the lightweight transaction");
        self.sent(&txn, frame.changed)
    }

    /// Passes while the engine owes frames, as the platform loop makes them;
    /// `each` looks at the window after every one. Returns how many it made.
    fn passes_while_owed(&mut self, mut each: impl FnMut(&Self, &Pass, usize)) -> usize {
        let mut made = 0;
        while self.window.gpu_state_manager.halos_owed {
            made += 1;
            assert!(made < 40, "the halos come");
            let pass = self.light_pass();
            each(&*self, &pass, made);
        }
        made
    }
}

/// What one pass of a shell handed the renderer.
#[derive(Debug)]
struct Pass {
    /// The images its transaction added, gave new pixels and deleted.
    added: Vec<ImageKey>,
    updated: Vec<ImageKey>,
    deleted: Vec<ImageKey>,
    /// Whether it carried a display list.
    list: bool,
    /// Whether it asked the renderer for a frame.
    frame: bool,
}

/// `count` labels of a map as the editor draws them: absolutely placed, each
/// with a solid halo (a text shadow that is a spread and nothing else), at
/// `zoom` times their size and place.
pub(super) fn labels(count: usize, zoom: f32) -> Dom {
    labels_from(0, count, zoom)
}

/// The labels `first .. first + count` of the same map ([`labels`]): what a
/// view shows after a pan by `first` labels.
fn labels_from(first: usize, count: usize, zoom: f32) -> Dom {
    let mut page = Dom::create_div().with_css(
        format!(
            "position: relative; width: {}px; height: {}px; background: #4477aa;",
            600.0 * zoom,
            600.0 * zoom
        )
        .as_str(),
    );
    for i in first..first + count {
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
/// a window of their own (the kept halos are a window's).
fn bytes_of_the_views_pictures(count: usize, dpi: f32) -> usize {
    draw_every_halo_in_this_thread();
    let mut view = Recorder::new(dpi);
    view.lay_out(labels(count, 1.0));
    let frame = view.frame();
    assert_eq!(frame.own().len(), count, "premise: a frame with no budget draws every halo");
    usize::try_from(view.window.text_halo_store.text_halo_stats().kept_bytes).expect("bytes")
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
    draw_this_many_halos_a_frame_in_this_thread(3);

    let mut view = Recorder::new(dpi);
    view.window.text_halo_store.text_halo_bounds(usize::MAX, all * 8 / 10);
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
        // (Not "a prefix of the list" any more: a picture that did not fit the
        // place of its stand-in is shown when the drawing is over, audit A4.)
        assert!(
            before.own().iter().all(|label| now.own().contains(label)),
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
    let mut replaced: HashSet<ImageKey> = settle.updated.iter().copied().collect();
    let mut before = settle;
    let mut resting = 0;
    for n in 1..40 {
        let now = view.frame();
        replaced.extend(now.updated.iter().copied());
        assert!(
            now.halos.iter().all(|halo| *halo != Halo::None),
            "frame {n} after the settle: a label without halo: {:?}",
            now.halos
        );
        assert!(
            before.own().iter().all(|label| now.own().contains(label)),
            "frame {n}: an own halo was lost"
        );
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
    // New pictures, not the old ones. A key of the old view that a label is
    // still painted from is the place its new picture was drawn into (audit
    // A4: the picture took the stand-in's slot): the key was given the new
    // pixels by an update after the settle. Every other old key is gone.
    for key in new.intersection(&old) {
        assert!(replaced.contains(key), "{key:?} still shows the old picture");
    }
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

/// A picture of 4 x 2 px, every byte `fill` (a halo's picture, for the tests of
/// the slot keys).
fn slot_picture(fill: u8) -> azul_core::resources::ImageRef {
    use azul_core::resources::{ImageRef, RawImage, RawImageData, RawImageFormat};

    ImageRef::new_rawimage(RawImage {
        pixels: RawImageData::U8(vec![fill; 4 * 2 * 4].into()),
        width: 4,
        height: 2,
        premultiplied_alpha: true,
        data_format: RawImageFormat::RGBA8,
        tag: Vec::new().into(),
    })
    .expect("a picture")
}

/// The key `image` resolves to: what a translation paints it from.
fn registered(
    rr: &RendererResources,
    image: &azul_core::resources::ImageRef,
) -> Option<azul_core::resources::ImageKey> {
    rr.currently_registered_images
        .get(&image.get_hash())
        .map(|resolved| resolved.key)
}

/// A5-15 (review of A4-27, invariant I-g: the slot keys are a window's). Giving a
/// slot's key a new picture removed the registration of the picture the key had
/// shown, whoever held it. A picture can be another slot's by then - when this
/// was found the kept pictures were a thread's, and a second window's scans moved
/// them between the slots of the first (they are a window's now, and what a
/// registration is given is still whatever the slots show by then):
///
/// 1. A paints `old` from slot 1 (key K1) and `other` from slot 2 (key K2).
/// 2. B's scans change what the slots show: slot 2 shows `old` now, slot 1
///    shows `new`.
/// 3. A's next scan registers the slots in list order: slot 2 first (K2 is
///    given `old`, which resolves to K2 from now on), then slot 1 (K1 is given
///    `new`).
///
/// Step 3's second half removed the registration of the picture K1 had shown -
/// `old`, which K2 shows since a moment ago. The label of slot 2 was translated
/// to WebRender's own shadow (no spread: no outline), and stayed so: slot 2
/// "shows `old` already", so no later scan registered it again.
///
/// (Changed with A5-16: a picture in a slot that has NO key takes the key that
/// shows it with it, so the sequence needs slot 2 to have a key of its own.)
///
/// Mutation: the `resolved.key == key` guard removed from the `TextHalo` arm of
/// `plan_overlay_image_uploads` leaves `old` unregistered: this fails.
#[test]
fn a5_15_a_picture_given_a_new_slot_stays_registered_when_its_old_slot_shows_another() {
    use azul_core::resources::{IdNamespace, ImageDirtyRect, ImageSlot};

    use crate::desktop::wr_translate2::plan_overlay_image_uploads;

    let namespace = IdNamespace(7);
    let (old, new, other) = (slot_picture(1), slot_picture(2), slot_picture(3));

    // 1. The window paints `old` from slot 1 and `other` from slot 2.
    let mut rr = RendererResources::default();
    let first = plan_overlay_image_uploads(
        [
            (ImageSlot::TextHalo(1), &old, ImageDirtyRect::All),
            (ImageSlot::TextHalo(2), &other, ImageDirtyRect::All),
        ]
        .into_iter(),
        &mut rr,
        namespace,
    );
    assert_eq!(first.len(), 2);
    assert!(!first[0].update && !first[1].update, "a slot's first picture adds its key");
    let (key_1, key_2) = (first[0].key, first[1].key);
    assert_ne!(key_1, key_2);

    // 3. `old` from slot 2, then `new` from slot 1.
    let second = plan_overlay_image_uploads(
        [
            (ImageSlot::TextHalo(2), &old, ImageDirtyRect::All),
            (ImageSlot::TextHalo(1), &new, ImageDirtyRect::All),
        ]
        .into_iter(),
        &mut rr,
        namespace,
    );
    assert_eq!(second.len(), 2);
    assert!(second[0].update && second[1].update, "both keys live on with new pixels");
    assert_eq!((second[0].key, second[1].key), (key_2, key_1));
    assert_eq!(
        registered(&rr, &old),
        Some(key_2),
        "the picture slot 2 paints resolves to slot 2's key"
    );
    assert_eq!(registered(&rr, &new), Some(key_1));
    assert_eq!(registered(&rr, &other), None, "the picture nothing shows is forgotten");
    assert_eq!(rr.image_key_map.get(&key_1), Some(&new.get_hash()));
    assert_eq!(rr.image_key_map.get(&key_2), Some(&old.get_hash()));

    // The order one window alone produces (the slot that changed first): the
    // old picture's registration under K1 is removed, as before.
    let mut rr = RendererResources::default();
    let _ = plan_overlay_image_uploads(
        [(ImageSlot::TextHalo(1), &old, ImageDirtyRect::All)].into_iter(),
        &mut rr,
        namespace,
    );
    let replaced = plan_overlay_image_uploads(
        [(ImageSlot::TextHalo(1), &new, ImageDirtyRect::All)].into_iter(),
        &mut rr,
        namespace,
    );
    assert!(replaced[0].update);
    assert_eq!(registered(&rr, &old), None, "a replaced picture is forgotten");
    assert!(!rr.image_last_seen_epoch.contains_key(&old.get_hash()));
}

/// A5-16 (review of A4-27, invariant I-c / I-g). When this was found the kept
/// pictures were a thread's; with two windows A and B on one thread (B's scans
/// moved pictures between the slots of A):
///
/// 1. A paints a label with `old` from slot 1 (A's key K1 shows it).
/// 2. B's scans draw the label's picture at another size into the place of
///    `old`: slot 1 is another picture's now, and `old` gets a new slot (2) when
///    it is next used.
/// 3. A's next scan paints the same label with the same picture, from slot 2.
///
/// Step 3 minted a key K2 for slot 2 and uploaded the picture a second time;
/// `old` resolved to K2 from then on, the list of A went again (a new key), and
/// K1 was left: it shows a picture that is registered under another key, so no
/// GC reaches it and no slot of A updates it - one image leaked in the renderer
/// per label and zoom of the other window. Now the key goes with the picture:
/// slot 2 takes K1, nothing is uploaded, and what the list names is what it
/// named.
///
/// Mutation: the block removed from `plan_overlay_image_uploads`: step 3 uploads
/// (`moved` is not empty) and two keys show the one picture.
#[test]
fn a5_16_a_halo_picture_given_a_new_slot_keeps_the_key_that_shows_it() {
    use azul_core::resources::{IdNamespace, ImageDirtyRect, ImageSlot};

    use crate::desktop::wr_translate2::plan_overlay_image_uploads;

    let namespace = IdNamespace(7);
    let (old, new) = (slot_picture(1), slot_picture(2));
    let mut rr = RendererResources::default();
    let first = plan_overlay_image_uploads(
        [(ImageSlot::TextHalo(1), &old, ImageDirtyRect::All)].into_iter(),
        &mut rr,
        namespace,
    );
    let key_1 = first[0].key;

    // 3. The same picture, from slot 2.
    let moved = plan_overlay_image_uploads(
        [(ImageSlot::TextHalo(2), &old, ImageDirtyRect::All)].into_iter(),
        &mut rr,
        namespace,
    );
    assert!(moved.is_empty(), "the renderer holds the picture: nothing to upload");
    assert_eq!(rr.image_slots.get(&ImageSlot::TextHalo(2)), Some(&key_1));
    assert!(!rr.image_slots.contains_key(&ImageSlot::TextHalo(1)));
    assert_eq!(registered(&rr, &old), Some(key_1), "the list's key shows what it showed");
    assert_eq!(rr.image_key_map.len(), 1, "one key, and none left behind");

    // The slot the picture came from is another picture's: when this window
    // paints that one, it gets a key of its own.
    let again = plan_overlay_image_uploads(
        [
            (ImageSlot::TextHalo(2), &old, ImageDirtyRect::All),
            (ImageSlot::TextHalo(1), &new, ImageDirtyRect::All),
        ]
        .into_iter(),
        &mut rr,
        namespace,
    );
    assert_eq!(again.len(), 1);
    assert!(!again[0].update, "slot 1 has no key in this window any more: one is added");
    assert_ne!(again[0].key, key_1);
    assert_eq!(registered(&rr, &new), Some(again[0].key));
    assert_eq!(registered(&rr, &old), Some(key_1));

    // A node's picture is not a halo's: two node slots that show one frame each
    // hold it under their own key, as before.
    let node = |n: usize| {
        ImageSlot::Node(azul_core::dom::DomId { inner: 0 }, azul_core::dom::NodeId::new(n))
    };
    let mut rr = RendererResources::default();
    let one = plan_overlay_image_uploads(
        [(node(3), &old, ImageDirtyRect::All)].into_iter(),
        &mut rr,
        namespace,
    );
    let two = plan_overlay_image_uploads(
        [(node(4), &old, ImageDirtyRect::All)].into_iter(),
        &mut rr,
        namespace,
    );
    assert_eq!((one.len(), two.len()), (1, 1));
    assert_ne!(one[0].key, two[0].key);
}

/// How many labels a translation of the window's list made NOW, without a scan
/// before it, paints without their halo (WebRender's own shadow, which has no
/// spread). What a lightweight pass does with a list that was patched in place
/// (a hover's colour): it is sent again, and nothing draws what it lacks.
fn without_halo_in_a_translation_alone(view: &Recorder) -> usize {
    let list = view
        .window
        .get_layout_result(&DomId::ROOT_ID)
        .expect("the root DOM is laid out")
        .display_list
        .clone();
    let (_, built) = translate_displaylist_to_wr(
        &list,
        PipelineId(0, 0),
        DeviceIntSize::new(2000, 2000),
        &view.window.renderer_resources,
        &view.window.text_halo_store,
        DpiScaleFactor::new(view.dpi),
        Vec::new(),
        &BTreeMap::new(),
        0,
    )
    .expect("the list translates");
    let mut shadows = 0;
    let mut items = built.iter();
    while let Some(item) = items.next() {
        if let DisplayItem::PushShadow(_) = item.item() {
            shadows += 1;
        }
    }
    shadows
}

/// A5-18, the memory release (`MacOSWindow::release_regenerable_memory`: the
/// window goes to the Dock, the app is hidden, memory is short). Twelve labels,
/// then a zoom that settles: when the view is at rest the window's store keeps the
/// labels' pictures at the new size, which the list paints, and the ones of the
/// size before, which nothing paints. A release gives back the second kind and
/// only that, with a window on screen and with none:
///
/// - the frame after it is the frame before it: every label painted with its
///   own picture from the key it had, no image added, given new pixels or
///   deleted, no halo drawn, no frame owed - and no list names an image the
///   renderer does not hold (`Recorder::frame` fails on one);
/// - a translation without a scan (a list patched in place) still finds every
///   label's picture.
///
/// Mutation: `entry.used_in < scan` -> `true` in `HaloCache::release_unused` (a
/// release of everything, which frees no byte of a picture the renderer holds):
/// the translation alone paints twelve labels without halo, and the frame after
/// draws pictures again and owes frames.
fn a_release_keeps_what_the_lists_paint(not_shown: bool) {
    const LABELS: usize = 12;
    draw_this_many_halos_a_frame_in_this_thread(4);
    let mut view = Recorder::new(2.0);
    view.lay_out(labels(LABELS, 1.0));
    let mut guard = 0;
    while view.frame().owed {
        guard += 1;
        assert!(guard < 20, "the first view's halos come");
    }
    view.lay_out(labels(LABELS, 1.5));
    let mut before = view.frame();
    let mut resting = 0;
    for _ in 0..40 {
        if resting == 5 {
            break;
        }
        let now = view.frame();
        resting = if now.at_rest() { resting + 1 } else { 0 };
        before = now;
    }
    assert_eq!(resting, 5, "premise: the view came to rest: {:?}", before.stats);
    assert_eq!(before.own().len(), LABELS, "premise: every label has its own picture");
    let store = &view.window.text_halo_store;
    let kept = store.text_halo_stats();
    assert!(
        kept.kept as usize > LABELS,
        "premise: pictures of the size before are kept: {}",
        kept.kept
    );

    // The shell's release: what the window's lists paint is marked, then
    // everything else goes.
    let list = view
        .window
        .get_layout_result(&DomId::ROOT_ID)
        .expect("the root DOM is laid out")
        .display_list
        .clone();
    store.text_halos_in_use([&list], view.dpi);
    let released = store.release_text_halos(not_shown);
    let left = store.text_halo_stats();
    assert_eq!(left.kept as usize, LABELS, "the pictures the list paints, and no other");
    assert_eq!(released.halos, kept.kept as usize - LABELS);
    assert!(released.bytes > 0);
    assert_eq!(left.kept_bytes as usize, kept.kept_bytes as usize - released.bytes);

    assert_eq!(
        without_halo_in_a_translation_alone(&view),
        0,
        "a list sent again without a scan lost its halos"
    );
    let after = view.frame();
    assert!(
        after.at_rest(),
        "the frame after the release changed something: +{:?} ~{:?} -{:?} {:?}",
        after.added,
        after.updated,
        after.deleted,
        after.stats
    );
    assert_eq!(after.halos, before.halos, "each label's own picture, from the key it had");
    assert_eq!(after.stats.frame_halos as usize, LABELS);
    assert_eq!(view.window.text_halo_store.text_halo_stats().kept as usize, LABELS);
}

#[test]
fn a5_18_a_release_keeps_what_the_lists_paint_with_a_window_on_screen() {
    a_release_keeps_what_the_lists_paint(false);
}

#[test]
fn a5_18_a_release_keeps_what_the_lists_paint_with_no_window_on_screen() {
    a_release_keeps_what_the_lists_paint(true);
}

/// A5-18. The macOS shell's release (`MacOSWindow::release_regenerable_memory`:
/// the Dock, Cmd-H, memory pressure) gives back the kept text halos of the window
/// through `release_text_halos`, AFTER the window's lists marked what they paint:
/// a list translated without a scan lately would lose its pictures to the
/// release. The store is the window's own, so no other window's lists are
/// marked or released. (The delegate cannot run headless: the statements are read
/// from the source, as A10's guard of the same function does.)
///
/// Mutation: the release moved above the marking, or the marking removed, fails
/// this; a loop over the other windows back in the release fails the last check.
#[test]
fn a5_18_the_macos_release_marks_what_the_windows_lists_paint_before_it_gives_halos_back() {
    const MACOS_RS: &str = include_str!("../shell2/macos/mod.rs");
    let from = MACOS_RS
        .find("pub(crate) fn release_regenerable_memory(")
        .expect("the release");
    let body = &MACOS_RS[from..];
    let body = &body[..body.find("\n    }\n").expect("the end of the function")];
    let at = |what: &str| body.find(what).unwrap_or_else(|| panic!("`{what}` is missing"));
    let marked = at("layout_window.text_halo_store.text_halos_in_use(");
    let released = at("layout_window.text_halo_store.release_text_halos(not_shown)");
    assert!(marked < released, "marked first, released last");
    assert!(body.contains("if !keep_caches() {"), "the user's switch keeps the halos too");
    assert!(
        !body.contains("registry::get_all_window_ptrs()"),
        "the pictures are the window's own: no other window is marked or released for"
    );
}

/// A5-19. A pan in a cache that the view fills (a map whose labels' pictures
/// are more than the window keeps: here eight pictures, and room for eight).
/// Frame by frame:
///
/// 1. the view shows labels 0..8, each with its own picture: the cache is full;
/// 2. the pan shows labels 4..12. Its scan may not drop the pictures of labels
///    0..4 (the frame before's: the two-scan rule), so labels 8..12 get neither
///    their picture nor a small one. Before, nothing was owed for a halo without
///    room: the four new labels stayed without outline until something else made
///    a scan (the next pan, the pointer over a label). Now one scan is owed;
/// 3. that scan drops the four pictures of the labels that left and draws the
///    four new ones: every label has its halo, and the view comes to rest.
///
/// Mutation: `owes_a_scan_for_room` always false in `text_halo.rs`: frame 2 owes
/// nothing (`pan.owed` fails).
#[test]
fn a5_19_a_pan_in_a_full_cache_gets_its_new_labels_their_halos_in_the_next_frame() {
    const SHOWN: usize = 8;
    draw_every_halo_in_this_thread();
    let mut view = Recorder::new(2.0);
    view.window.text_halo_store.text_halo_bounds(SHOWN, usize::MAX);
    view.lay_out(labels_from(0, SHOWN, 1.0));
    let first = view.frame();
    assert_eq!(first.own().len(), SHOWN, "premise: the view's pictures fill the cache");
    assert!(!first.owed);

    view.lay_out(labels_from(4, SHOWN, 1.0));
    let pan = view.frame();
    assert_eq!(pan.own(), [0, 1, 2, 3], "the labels that stayed keep their pictures");
    assert_eq!(
        (pan.stats.unkept, pan.stats.pending, pan.stats.evicted),
        (4, 0, 0),
        "the new labels have no room, and the frame before's pictures are not dropped for them"
    );
    assert!(pan.owed, "one scan is owed: it may drop the pictures of the labels that left");

    let next = view.frame();
    assert_eq!(next.own().len(), SHOWN, "every label has its own halo: {:?}", next.halos);
    assert_eq!((next.stats.created, next.stats.evicted, next.stats.unkept), (4, 4, 0));
    assert!(!next.owed);

    let mut resting = 0;
    for n in 0..20 {
        let now = view.frame();
        assert_eq!(now.own().len(), SHOWN, "frame {n} after: a label lost its halo");
        assert_eq!(now.stats.created + now.stats.evicted, 0, "frame {n} after");
        resting = if now.at_rest() { resting + 1 } else { 0 };
        if resting == 5 {
            break;
        }
    }
    assert_eq!(resting, 5, "the view came to rest");
}

// ==== A5-20: a window nobody sees gives back the pictures its lists paint ====

/// `dom` laid out and drawn, pass by pass as a shell makes them, until the
/// window owes no frame: every label that can have a picture has its own.
fn a_view_at_rest(dpi: f32, dom: Dom) -> Recorder {
    let mut view = Recorder::new(dpi);
    view.lay_out(dom);
    let first = view.full_pass();
    assert!(first.list, "the first pass sends the list");
    let _ = view.passes_while_owed(|_, _, _| {});
    view
}

/// The keys of the labels painted with their own picture.
fn own_keys(halos: &[Halo]) -> HashSet<ImageKey> {
    halos
        .iter()
        .filter_map(|halo| match halo {
            Halo::Own(key) => Some(*key),
            _ => None,
        })
        .collect()
}

/// What the macOS shell does for a window nobody sees
/// (`MacOSWindow::release_regenerable_memory`, the Dock or a hidden app): the
/// renderer's keys of the window's halo pictures are deleted in a transaction
/// and its lists go without them (`release_text_halo_images`); then the
/// window's store forgets what its lists painted and gives back all it keeps
/// (nothing is marked: no list of a hidden window paints from a picture).
/// Other windows are not involved: their stores are theirs. Returns the pass,
/// the keys the release says it deleted, and what the store gave back.
fn hide(view: &mut Recorder) -> (Pass, usize, TextHalosReleased) {
    use crate::desktop::wr_translate2::release_text_halo_images;

    let mut txn = Transaction::new();
    let keys = release_text_halo_images(&mut view.window, &mut txn);
    view.window.text_halo_store.text_halos_forget_use();
    let released = view.window.text_halo_store.release_text_halos(true);
    (view.sent(&txn, false), keys, released)
}

/// A5-20, test 1. A window at rest paints twelve labels from twelve pictures:
/// its store keeps their bytes and the renderer holds the same bytes under
/// twelve keys. The window goes to the Dock. Its release deletes every one of
/// those keys in one transaction, which also carries the list translated
/// without the pictures; the store keeps no halo and no byte; the window is
/// marked, and owes no frame. (The shell sends that transaction whenever the
/// release says it deleted a key: `a5_20_the_macos_shell_...`.)
///
/// Mutation: the loop over `keys` that calls `txn.delete_image` removed from
/// `release_text_halo_images` (the renderer keeps the bytes): `view.live` is not
/// empty. `text_halos_forget_use()` removed from the release: the store keeps
/// the twelve pictures.
#[test]
fn a5_20_a_hidden_window_gives_back_the_pictures_its_lists_paint() {
    use azul_core::resources::ImageSlot;

    const LABELS: usize = 12;
    draw_this_many_halos_a_frame_in_this_thread(4);
    let mut view = a_view_at_rest(2.0, labels(LABELS, 1.0));
    let held = view.painted();
    let named = own_keys(&held);
    assert_eq!(named.len(), LABELS, "premise: every label on its own picture: {held:?}");
    let kept = view.window.text_halo_store.text_halo_stats();
    assert!(kept.kept as usize >= LABELS && kept.kept_bytes > 0, "premise: {kept:?}");

    let (pass, keys, released) = hide(&mut view);
    assert!(keys >= LABELS, "the release names the keys it deleted: {keys}");
    assert_eq!(pass.deleted.len(), keys, "each in the transaction it filled");
    assert!(
        named.iter().all(|key| pass.deleted.contains(key)),
        "a key a label was painted from is not deleted"
    );
    assert!(pass.added.is_empty() && pass.updated.is_empty());
    assert!(pass.list, "the list goes with the deletes, translated without the pictures");
    assert!(view.live.is_empty(), "the renderer still holds {:?}", view.live);
    let rr = &view.window.renderer_resources;
    assert!(
        !rr.image_slots.keys().any(|slot| matches!(slot, ImageSlot::TextHalo(_))),
        "a halo slot kept its key"
    );
    assert!(rr.currently_registered_images.is_empty() && rr.image_key_map.is_empty());
    let left = view.window.text_halo_store.text_halo_stats();
    assert_eq!((left.kept, left.kept_bytes), (0, 0), "the halo bytes kept for the window");
    assert_eq!(
        (released.halos as u64, released.bytes as u64),
        (kept.kept, kept.kept_bytes),
        "all of them were given back by this release"
    );
    let state = &view.window.gpu_state_manager;
    assert_eq!(state.halo_release, HaloRelease::Hidden);
    assert!(!state.halos_owed, "nothing is drawn for a window nobody sees");
    assert!(
        view.painted().iter().all(|halo| *halo == Halo::None),
        "the list the renderer holds names no picture"
    );

    // Hidden once more (the app is hidden while the window is in the Dock).
    let (again, keys, _) = hide(&mut view);
    assert_eq!((keys, again.deleted.len(), again.list), (0, 0, false));

    // Shown, and hidden again before any pass brought the pictures back: the
    // frame that was owed for them is owed no more, and nothing is sent.
    assert!(view.window.gpu_state_manager.halos_return());
    assert!(view.window.gpu_state_manager.halos_owed);
    let (again, keys, _) = hide(&mut view);
    assert_eq!((keys, again.deleted.len(), again.list), (0, 0, false));
    let state = &view.window.gpu_state_manager;
    assert_eq!((state.halo_release, state.halos_owed), (HaloRelease::Hidden, false));
    let idle = view.light_pass();
    assert!(!idle.frame && !idle.list && idle.added.is_empty(), "{idle:?}");
}

/// A5-20, test 2. The window is shown again. The shell says so
/// (`GpuStateManager::halos_return`: a frame is owed); the pass that follows
/// scans, draws the labels' own pictures by the frame's budget (four here) and
/// a small picture for every other label, registers them and sends the list
/// with them. The first frame after the return - and every frame after it -
/// paints every label with a picture: none is bare. With type too small for a
/// small picture (`zoom` 0.3: 7.2 device px per em) every label's own picture
/// is drawn in that one scan, whatever the frame's budget.
///
/// Mutation: `self.halos_owed = true` removed from `halos_return` (nothing
/// scans: the first pass sends nothing); the `Returning` arm removed from
/// `collect_image_resource_updates_and_halos` passes at this size (twelve labels
/// are within a frame's 64 small pictures; the scan's rule itself is the unit
/// test `the_scan_after_a_release_draws_every_small_picture_...`);
/// `whatever_the_time` removed from `text_halos` fails the small type: eight
/// labels have no halo in the first frame.
fn a_window_that_is_shown_again_has_every_halo_in_its_first_frame(zoom: f32) {
    const LABELS: usize = 12;
    draw_this_many_halos_a_frame_in_this_thread(4);
    let mut view = a_view_at_rest(2.0, labels(LABELS, zoom));
    assert_eq!(own_keys(&view.painted()).len(), LABELS, "premise: every label has a picture");
    let _ = hide(&mut view);

    // A hidden window at rest: a pass has nothing to send, and draws nothing.
    let idle = view.light_pass();
    assert!(!idle.frame && !idle.list && idle.added.is_empty(), "{idle:?}");
    assert_eq!(view.window.text_halo_store.text_halo_stats().kept, 0);

    assert!(view.window.gpu_state_manager.halos_return(), "the window is shown again");
    assert!(!view.window.gpu_state_manager.halos_return(), "once");
    let first = view.light_pass();
    assert!(first.frame && first.list, "the pictures and the list go: {first:?}");
    let now = view.painted();
    assert!(
        now.iter().all(|halo| *halo != Halo::None),
        "the first frame after the return paints a label without its halo: {now:?}"
    );
    assert_eq!(first.added.len(), LABELS, "a picture for every label, in that pass");
    assert_eq!(view.window.gpu_state_manager.halo_release, HaloRelease::Kept);

    let _ = view.passes_while_owed(|view, _, n| {
        let now = view.painted();
        assert!(
            now.iter().all(|halo| *halo != Halo::None),
            "pass {n} after the return paints a label without its halo: {now:?}"
        );
    });
    assert_eq!(own_keys(&view.painted()).len(), LABELS, "every label has its own picture at last");
    let rest = view.light_pass();
    assert!(!rest.frame && !rest.list && rest.added.is_empty() && rest.updated.is_empty());
}

#[test]
fn a5_20_a_window_that_is_shown_again_has_every_halo_in_its_first_frame() {
    a_window_that_is_shown_again_has_every_halo_in_its_first_frame(1.0);
}

/// Type under 10 device px per em: there is no small picture for it.
#[test]
fn a5_20_a_window_of_small_type_that_is_shown_again_has_every_halo_in_its_first_frame() {
    a_window_that_is_shown_again_has_every_halo_in_its_first_frame(0.3);
}

/// A5-20, test 3. A list that is sent while the window is hidden names no key
/// that was deleted, and nothing is drawn for it:
///
/// - a colour patch (the pointer's last move, a timer): the list is a new
///   allocation, the lightweight pass sends it without a scan;
/// - a layout made again (the window resized in the Dock): the full
///   transaction, whose scan of the lists draws and looks up no halo.
///
/// Both translate every label's shadow to the renderer's own
/// (`Recorder::painted` fails on a key the renderer does not hold), add no
/// image, keep no picture and owe no frame; the window stays hidden until the
/// shell says it is shown, and then has every halo in its first frame.
///
/// Mutation: `halos_held_back` forced to `false` in
/// `collect_image_resource_updates_and_halos` (a hidden window's scan draws
/// halos): the full pass adds images and the store keeps pictures again.
#[test]
fn a5_20_a_list_sent_while_the_window_is_hidden_names_no_deleted_key() {
    const LABELS: usize = 12;
    draw_this_many_halos_a_frame_in_this_thread(4);
    let mut view = a_view_at_rest(2.0, labels(LABELS, 1.0));
    let _ = hide(&mut view);
    let hidden = |view: &Recorder, pass: &Pass, what: &str| {
        assert!(pass.list, "{what}: premise: the list is sent");
        assert!(
            pass.added.is_empty() && pass.updated.is_empty() && pass.deleted.is_empty(),
            "{what}: {pass:?}"
        );
        assert!(
            view.painted().iter().all(|halo| *halo == Halo::None),
            "{what}: the list names a picture"
        );
        assert_eq!(
            view.window.text_halo_store.text_halo_stats().kept,
            0,
            "{what}: a picture was drawn"
        );
        let state = &view.window.gpu_state_manager;
        assert_eq!(state.halo_release, HaloRelease::Hidden, "{what}");
        assert!(!state.halos_owed, "{what}: a frame is owed for a window nobody sees");
    };

    // A patch in place moves a list the renderer holds to a new allocation.
    {
        let result = view
            .window
            .layout_results
            .get_mut(&DomId::ROOT_ID)
            .expect("the root DOM is laid out");
        let before = Arc::as_ptr(&result.display_list);
        let _ = Arc::make_mut(&mut result.display_list);
        assert!(
            !std::ptr::eq(before, Arc::as_ptr(&result.display_list)),
            "premise: the renderer's record of the list makes a patch move it"
        );
    }
    let patched = view.light_pass();
    hidden(&view, &patched, "a patched list");

    view.lay_out(labels(LABELS, 1.0));
    let laid_out = view.full_pass();
    hidden(&view, &laid_out, "a layout made again");

    assert!(view.window.gpu_state_manager.halos_return());
    let first = view.light_pass();
    assert_eq!(first.added.len(), LABELS);
    assert!(view.painted().iter().all(|halo| *halo != Halo::None));
}

/// A5-20, test 4. Two windows on one thread, each with the kept pictures of its
/// own. A shows labels 0..12, B labels 6..18: six labels are both windows'
/// (each store keeps a picture of its own for them). B goes to the Dock: B's keys
/// are deleted and B's store gives back every picture it keeps; A's store is not
/// touched, the pictures of the shared labels among them. A loses nothing: an
/// idle pass sends nothing, and a pass that scans A's lists again draws nothing
/// and paints each label from the key it had. B comes back: its pictures are
/// drawn again, the six it shares with A too (what A keeps is A's).
///
/// Mutation: the release of B's store done through A's (the stores made once and
/// shared): A's store is not as it was, and A's next scan draws its pictures again.
#[test]
fn a5_20_a_hidden_window_takes_nothing_from_a_window_that_is_shown() {
    const LABELS: usize = 12;
    draw_this_many_halos_a_frame_in_this_thread(4);
    let mut a = a_view_at_rest(2.0, labels(LABELS, 1.0));
    let mut b = a_view_at_rest(2.0, labels_from(6, LABELS, 1.0));
    let held = a.painted();
    assert_eq!(own_keys(&held).len(), LABELS, "premise: A's labels have their pictures");
    assert_eq!(own_keys(&b.painted()).len(), LABELS, "premise: B's too");
    let a_kept = a.window.text_halo_store.text_halo_stats();
    let b_kept = b.window.text_halo_store.text_halo_stats();
    assert!(
        a_kept.kept as usize >= LABELS && b_kept.kept as usize >= LABELS,
        "premise: each window keeps its own pictures, for the labels both show too: {a_kept:?} {b_kept:?}"
    );

    let (pass, keys, released) = hide(&mut b);
    assert!(keys >= LABELS && pass.deleted.len() == keys);
    assert!(b.live.is_empty(), "B's renderer still holds {:?}", b.live);
    assert_eq!(
        (released.halos as u64, released.bytes as u64),
        (b_kept.kept, b_kept.kept_bytes),
        "everything B kept: its own pictures, the six of the shared labels among them"
    );
    assert_eq!(b.window.text_halo_store.text_halo_stats().kept, 0);
    assert_eq!(
        a.window.text_halo_store.text_halo_stats(),
        a_kept,
        "A's store is as it was: B's release gave back nothing of it"
    );

    let idle = a.light_pass();
    assert!(
        !idle.frame && idle.added.is_empty() && idle.updated.is_empty() && idle.deleted.is_empty(),
        "{idle:?}"
    );
    assert_eq!(a.painted(), held, "A paints what it painted");
    a.lay_out(labels(LABELS, 1.0));
    let scanned = a.full_pass();
    assert!(scanned.added.is_empty() && scanned.updated.is_empty(), "{scanned:?}");
    assert_eq!(
        a.window.text_halo_store.text_halo_stats().created,
        0,
        "A's scan draws nothing again"
    );
    assert_eq!(a.painted(), held, "each label from the picture and the key it had");

    assert!(b.window.gpu_state_manager.halos_return());
    let first = b.light_pass();
    assert_eq!(first.added.len(), LABELS, "B has a picture for every label again");
    assert!(b.painted().iter().all(|halo| *halo != Halo::None));
    assert_eq!(a.painted(), held, "and A still paints what it painted");
}

/// A5-20, test 5. Hide and show ten times. After every hide the renderer holds
/// no key of the window and its store no picture; after every return, at rest,
/// the renderer holds the same number of keys (as many as after the first
/// return: nothing piles up), every key it holds is a slot's (none left
/// behind), and the twelve labels are painted from twelve of them. No frame on
/// the way paints a label without its halo.
///
/// Mutation: `rr.image_slots.remove(slot)` removed from
/// `release_text_halo_images` (the slots keep keys that are deleted): the
/// return's pictures are "updates" of keys the renderer does not hold
/// (`Recorder::take` fails).
#[test]
fn a5_20_hiding_and_showing_a_window_ten_times_leaves_no_key_behind() {
    const LABELS: usize = 12;
    draw_this_many_halos_a_frame_in_this_thread(4);
    let mut view = a_view_at_rest(2.0, labels(LABELS, 1.0));
    // The keys the renderer holds when the window is at rest after a return.
    let mut at_rest: Vec<usize> = Vec::new();
    for round in 0..10 {
        let named = own_keys(&view.painted());
        assert_eq!(named.len(), LABELS, "round {round}: every label on its own picture");
        assert!(named.is_subset(&view.live));
        let rr = &view.window.renderer_resources;
        assert_eq!(
            (rr.image_slots.len(), rr.image_key_map.len()),
            (view.live.len(), view.live.len()),
            "round {round}: a key that is no slot's, or a slot without its key"
        );

        let (pass, keys, _) = hide(&mut view);
        assert_eq!(pass.deleted.len(), keys, "round {round}");
        assert!(view.live.is_empty(), "round {round}: left after the hide: {:?}", view.live);
        assert_eq!(view.window.text_halo_store.text_halo_stats().kept, 0, "round {round}");

        assert!(view.window.gpu_state_manager.halos_return());
        let first = view.light_pass();
        assert_eq!(first.added.len(), LABELS, "round {round}");
        let bare = |view: &Recorder| view.painted().iter().any(|halo| *halo == Halo::None);
        assert!(!bare(&view), "round {round}: the first frame after the return");
        let _ = view.passes_while_owed(|view, _, n| {
            assert!(!bare(view), "round {round}, pass {n} after the return");
        });
        at_rest.push(view.live.len());
    }
    assert!(at_rest[0] >= LABELS && at_rest[0] <= 2 * LABELS, "{at_rest:?}");
    assert!(
        at_rest.iter().all(|keys| *keys == at_rest[0]),
        "the keys the renderer holds after each return: {at_rest:?}"
    );
    assert_eq!(own_keys(&view.painted()).len(), LABELS);
}

/// A5-20. The macOS shell's half, read from the source (a delegate and a render
/// pass cannot run headless; the statements are the ones the recorder tests
/// above make by hand):
///
/// - a window that is released because nobody sees it has its halo keys
///   deleted in a transaction that is SENT and waited for, BEFORE its store
///   forgets what its lists painted, and before it marks what they paint;
/// - `AZ_HALO_RELEASE=lists` leaves the pictures a list paints (A5-18);
/// - every render pass of a window that is shown brings the halos back
///   (`halos_return`), and that pass waits for its frame;
/// - a frame of a window whose halos are given back is not presented.
///
/// Mutation: the send or the wait removed from
/// `release_halo_images_of_a_hidden_window`, the release moved behind
/// `text_halos_forget_use`, `halos_return` or the flush guard removed: each
/// fails here.
#[test]
fn a5_20_the_macos_shell_gives_a_hidden_windows_halos_back_and_brings_them_back() {
    const MACOS_RS: &str = include_str!("../shell2/macos/mod.rs");
    // The text of a function, from its name to the brace that closes it at
    // `indent`.
    let body_at = |from: &str, indent: &str| -> &'static str {
        let at = MACOS_RS
            .find(from)
            .unwrap_or_else(|| panic!("`{from}` is missing"));
        let rest = &MACOS_RS[at..];
        let end = format!("\n{indent}}}\n");
        &rest[..rest.find(end.as_str()).expect("the end of the function")]
    };
    let body = |from: &str| body_at(from, "    ");
    let release = body("pub(crate) fn release_regenerable_memory(");
    let at = |what: &str| {
        release
            .find(what)
            .unwrap_or_else(|| panic!("the release: `{what}` is missing"))
    };
    assert!(
        release.contains("let full = reason == MemoryRelease::NotShown && halo_release_is_full();"),
        "a window nobody sees, unless the switch says otherwise"
    );
    let order = [
        "self.release_halo_images_of_a_hidden_window();",
        "layout_window.text_halo_store.text_halos_forget_use();",
        "layout_window.text_halo_store.text_halos_in_use(",
        "layout_window.text_halo_store.release_text_halos(not_shown)",
    ];
    for pair in order.windows(2) {
        assert!(at(pair[0]) < at(pair[1]), "`{}` comes before `{}`", pair[0], pair[1]);
    }
    assert!(
        release.contains("== azul_layout::managers::gpu_state::HaloRelease::Kept"),
        "only a window whose pictures are kept marks what it paints"
    );

    let images = body("fn release_halo_images_of_a_hidden_window(");
    let sent = [
        "crate::desktop::wr_translate2::release_text_halo_images(layout_window, &mut txn) == 0",
        "render_api.send_transaction(doc_id, txn);",
        "render_api.flush_scene_builder();",
    ];
    let mut last = 0;
    for call in sent {
        let found = images
            .find(call)
            .unwrap_or_else(|| panic!("the release of the keys: `{call}` is missing"));
        assert!(found >= last, "`{call}` is out of order");
        last = found;
    }

    assert!(
        body_at("fn halo_release_is_full(", "")
            .contains("!std::env::var(\"AZ_HALO_RELEASE\").is_ok_and(|v| v == \"lists\")"),
        "the switch"
    );
    for (statement, why) in [
        (
            "let _ = layout_window.gpu_state_manager.halos_return();",
            "a pass of a window that is shown brings its halos back",
        ),
        ("|| halos_return_in_this_pass", "and waits for the frame that has them"),
        ("if !halos_hidden {", "a frame without them is not presented"),
        (
            "let window_is_not_shown = self.is_not_shown();",
            "whether the window is shown is asked in every pass",
        ),
    ] {
        assert!(MACOS_RS.contains(statement), "{why}: `{statement}` is missing");
    }
    assert!(
        body_at("fn window_did_deminiaturize(", "        ")
            .contains("macos_window.request_redraw();"),
        "a window back from the Dock asks for that pass"
    );
}
