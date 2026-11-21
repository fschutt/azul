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
            updated,
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
pub(super) fn labels(count: usize, zoom: f32) -> Dom {
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

/// A5-15 (review of A4-27, invariant I-g: the kept pictures are a thread's, the
/// slot keys a window's). The sequence, with two windows A and B on one thread:
///
/// 1. A paints a label with the picture `old` from slot 1 (A's key K1 shows it).
/// 2. B's scans draw the label's picture at another size into the place of
///    `old`: slot 1 shows `new` now, and `old` has no slot.
/// 3. A's next scan finds `old` as a label's own picture - it gets slot 2 - and,
///    further down its list, a label painted with `new` from slot 1. The slots
///    are registered in list order: slot 2 first (a new key K2 shows `old`,
///    `old` resolves to K2), then slot 1 (K1 is given `new`).
///
/// Giving K1 its new picture removed the registration of the picture K1 had
/// shown - `old`, which K2 shows since a moment ago. The label of slot 2 was
/// then translated to WebRender's own shadow (no spread: no outline), and stayed
/// so: slot 2 "shows `old` already", so no later scan registered it again.
///
/// Mutation: the `resolved.key == key` guard removed from the `TextHalo` arm of
/// `plan_overlay_image_uploads` leaves `old` unregistered: this fails.
#[test]
fn a5_15_a_picture_given_a_new_slot_stays_registered_when_its_old_slot_shows_another() {
    use azul_core::resources::{
        IdNamespace, ImageDirtyRect, ImageRef, ImageSlot, RawImage, RawImageData, RawImageFormat,
    };

    use crate::desktop::wr_translate2::plan_overlay_image_uploads;

    let picture = |fill: u8| {
        ImageRef::new_rawimage(RawImage {
            pixels: RawImageData::U8(vec![fill; 4 * 2 * 4].into()),
            width: 4,
            height: 2,
            premultiplied_alpha: true,
            data_format: RawImageFormat::RGBA8,
            tag: Vec::new().into(),
        })
        .expect("a picture")
    };
    let namespace = IdNamespace(7);
    let (old, new) = (picture(1), picture(2));
    let registered = |rr: &RendererResources, image: &ImageRef| {
        rr.currently_registered_images
            .get(&image.get_hash())
            .map(|resolved| resolved.key)
    };

    // 1. The window paints `old` from slot 1.
    let mut rr = RendererResources::default();
    let first = plan_overlay_image_uploads(
        [(ImageSlot::TextHalo(1), &old, ImageDirtyRect::All)].into_iter(),
        &mut rr,
        namespace,
    );
    assert_eq!(first.len(), 1);
    assert!(!first[0].update, "a slot's first picture adds its key");
    let key_1 = first[0].key;

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
    assert!(!second[0].update, "slot 2 is new: its key is added");
    assert!(second[1].update, "slot 1 keeps its key: new pixels under it");
    let key_2 = second[0].key;
    assert_ne!(key_2, key_1);
    assert_eq!(second[1].key, key_1);
    assert_eq!(
        registered(&rr, &old),
        Some(key_2),
        "the picture slot 2 paints resolves to slot 2's key"
    );
    assert_eq!(registered(&rr, &new), Some(key_1));
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
/// then a zoom that settles: when the view is at rest the thread keeps the
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
    use azul_layout::cpurender::{release_text_halos, text_halos_in_use};

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
    let (mut before, mut resting) = (view.frame(), 0);
    for _ in 0..40 {
        before = view.frame();
        resting = if before.at_rest() { resting + 1 } else { 0 };
        if resting == 5 {
            break;
        }
    }
    assert_eq!(resting, 5, "premise: the view came to rest: {:?}", before.stats);
    assert_eq!(before.own().len(), LABELS, "premise: every label has its own picture");
    let kept = text_halo_stats();
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
    text_halos_in_use([&list], view.dpi);
    let released = release_text_halos(not_shown);
    let left = text_halo_stats();
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
    assert_eq!(text_halo_stats().kept as usize, LABELS);
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
/// the Dock, Cmd-H, memory pressure) gives back the kept text halos through
/// `release_text_halos`, AFTER every window's lists marked what they paint: the
/// pictures are the thread's, and a window that made no scan lately would lose
/// the pictures its list paints to the windows that did. (The delegate cannot
/// run headless: the statements are read from the source, as A10's guard of the
/// same function does.)
///
/// Mutation: the release moved above the loop over the windows, or the loop
/// removed, fails this.
#[test]
fn a5_18_the_macos_release_marks_what_every_window_paints_before_it_gives_halos_back() {
    const MACOS_RS: &str = include_str!("../shell2/macos/mod.rs");
    let from = MACOS_RS
        .find("pub(crate) fn release_regenerable_memory(")
        .expect("the release");
    let body = &MACOS_RS[from..];
    let body = &body[..body.find("\n    }\n").expect("the end of the function")];
    let at = |what: &str| body.find(what).unwrap_or_else(|| panic!("`{what}` is missing"));
    let every_window = at("for window in registry::get_all_window_ptrs()");
    let marked = at("azul_layout::cpurender::text_halos_in_use(");
    let own = at("painted_by(&*self);");
    let released = at("azul_layout::cpurender::release_text_halos(not_shown)");
    assert!(marked < every_window, "the closure that marks is what the loop calls");
    assert!(every_window < own && own < released, "marked first, released last");
    assert!(body.contains("if !keep_caches() {"), "the user's switch keeps the halos too");
}
