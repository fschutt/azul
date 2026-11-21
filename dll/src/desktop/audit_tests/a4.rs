//! A4, the GPU path (WebRender integration).
//!
//! Counts, not times: what a transaction holds and what a translation emits.

use std::{collections::BTreeMap, sync::Arc};

use azul_core::{
    dom::{Dom, DomId, NodeId},
    geom::LogicalSize,
    gl::OptionGlContextPtr,
    resources::{
        DpiScaleFactor, ImageRef, OpacityKey, RawImage, RawImageData, RawImageFormat,
        RendererResources,
    },
    styled_dom::StyledDom,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    cpurender::{text_halo_slots_in_this_thread, text_halos},
    overlay::ContentChange,
    solver3::display_list::DisplayListItem,
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;
use webrender::{
    api::{units::DeviceIntSize, DisplayItem, ImageKey, PipelineId},
    render_api::FrameMsg,
};

use super::a5::{labels, Recorder};
use crate::desktop::{
    compositor2::translate_displaylist_to_wr,
    wr_translate2::{
        build_full_transaction, build_lightweight_transaction, count_transaction,
        draw_this_many_halos_a_frame_in_this_thread, publish_gpu_frame, synchronize_gpu_values,
        WrTransaction, BOUND_VALUES_WALKED,
    },
};

/// A laid-out window of one box.
fn window() -> LayoutWindow {
    let dom = Dom::create_body()
        .with_child(Dom::create_div().with_css("width: 40px; height: 20px; background: #3366cc;"));
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(200.0, 100.0);
    lw.current_window_state = ws.clone();
    lw.layout_and_generate_display_list(
        StyledDom::create_from_dom(dom),
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the window lays out");
    lw
}

/// `node` of the root DOM gets a bound opacity of `value` (what an animation
/// tick or a `set_css_property` of a GPU-only value writes).
fn bind_opacity(lw: &mut LayoutWindow, node: usize, key: OpacityKey, value: f32) {
    let cache = lw.gpu_state_manager.get_or_create_cache(DomId::ROOT_ID);
    cache.opacity_keys.insert(NodeId::new(node), key);
    cache.current_opacity_values.insert(NodeId::new(node), value);
}

/// How many values the frame operations of `txn` leave PENDING in WebRender,
/// replayed the way `SceneProperties` applies them: a reset empties the
/// pending set, an append adds to it.
fn pending_after(pending: &mut usize, txn: WrTransaction) -> (usize, usize) {
    let (mut resets, mut appends) = (0, 0);
    for op in txn.get_frame_ops() {
        match op {
            FrameMsg::ResetDynamicProperties => {
                resets += 1;
                *pending = 0;
            }
            FrameMsg::AppendDynamicProperties(properties) => {
                appends += 1;
                *pending +=
                    properties.transforms.len() + properties.floats.len() + properties.colors.len();
            }
            FrameMsg::AppendDynamicTransformProperties(transforms) => {
                appends += 1;
                *pending += transforms.len();
            }
            _ => {}
        }
    }
    (resets, appends)
}

/// A4-01. A changed set of GPU values REPLACES the set WebRender holds: the
/// pending values after n changed frames are the window's bound values, not n
/// times as many. Mutation: remove `txn.reset_dynamic_properties()` from
/// `synchronize_gpu_values` and the count after frame 16 is 16 times the count
/// after frame 1.
#[test]
fn a4_01_changed_gpu_values_replace_the_set_webrender_holds() {
    let mut lw = window();
    let key = OpacityKey::unique();
    let mut pending = 0usize;
    let mut after_first = 0usize;
    for frame in 0..16u32 {
        bind_opacity(&mut lw, 0, key, 0.25 + frame as f32 / 64.0);
        let mut txn = WrTransaction::new();
        assert!(
            synchronize_gpu_values(&mut lw, &mut txn),
            "frame {frame}: the value moved, so the set is sent"
        );
        let (resets, appends) = pending_after(&mut pending, txn);
        assert_eq!((resets, appends), (1, 1), "frame {frame}: one reset, then one append");
        if frame == 0 {
            after_first = pending;
        }
    }
    assert!(after_first >= 1, "the bound opacity is in the set");
    assert_eq!(
        pending, after_first,
        "16 changed frames leave the set once, not 16 times ({after_first} values a frame)"
    );

    // An unchanged set is not sent at all.
    let mut txn = WrTransaction::new();
    assert!(!synchronize_gpu_values(&mut lw, &mut txn));
    assert!(txn.is_empty(), "nothing in the transaction for values that stood still");
}

/// A4-07. The digest that says "the same bound values as last time" is a sum of
/// one hash per (key, value): the order the values come in (hash maps) does not
/// change it, two keys that swap their values do, and a NaN is equal to itself
/// (bitwise), so an unchanged set is never sent and a changed one always.
/// Mutation: hashing the value without its key in `synchronize_gpu_values`
/// makes the swap "unchanged".
#[test]
fn a4_07_the_digest_of_the_bound_values_sees_a_swap_and_not_the_order() {
    let mut lw = window();
    let (a, b) = (OpacityKey::unique(), OpacityKey::unique());
    let mut sync = |lw: &mut LayoutWindow| synchronize_gpu_values(lw, &mut WrTransaction::new());

    bind_opacity(&mut lw, 0, a, 0.25);
    bind_opacity(&mut lw, 1, b, 0.75);
    assert!(sync(&mut lw), "the first set is sent");
    assert!(!sync(&mut lw), "the same set is not");

    bind_opacity(&mut lw, 0, a, 0.75);
    bind_opacity(&mut lw, 1, b, 0.25);
    assert!(sync(&mut lw), "the two keys swapped their values: a change");
    assert!(!sync(&mut lw));

    bind_opacity(&mut lw, 0, a, f32::NAN);
    assert!(sync(&mut lw), "a NaN is a new value");
    assert!(!sync(&mut lw), "and equal to itself the next time");

    // A full transaction forgets what was sent: the same set goes again.
    lw.gpu_state_manager.invalidate_submitted_digests();
    assert!(sync(&mut lw));
}

/// A10-15 (the GPU document's size, applied by A4). The rule the shells now use
/// when they create the document is the one of every frame's document view,
/// viewport and framebuffer: `LogicalSize::to_physical`, rounded. 333.7 units
/// at 1.5 are 500.55 device px: 501 (truncated, as the macOS shell sized the
/// new document, 500: one pixel less than its own viewport). This pins the
/// rule, not the shells' call sites (they need a window).
#[test]
fn a10_15_the_gpu_document_is_sized_by_the_rule_of_the_viewport() {
    let dimensions = LogicalSize::new(333.7, 200.0);
    let created = dimensions.to_physical(1.5);
    assert_eq!((created.width, created.height), (501, 300));

    let mut ws = FullWindowState::default();
    ws.size.dimensions = dimensions;
    ws.size.dpi = 144;
    let every_frame = ws.size.get_physical_size();
    assert_eq!(
        (created.width, created.height),
        (every_frame.width, every_frame.height),
        "the document is born at the size its first transaction gives it"
    );
}

// ==== The frames after a zoom settles: halo pictures, and no list (audit A4) ====

/// What one pass of a shell handed the renderer.
#[derive(Debug)]
struct Pass {
    /// The images its transaction added, gave new pixels and deleted.
    added: Vec<ImageKey>,
    updated: Vec<ImageKey>,
    deleted: Vec<ImageKey>,
    /// The display lists it translated and sent, and their items.
    lists: u64,
    items: u64,
    /// Whether it asked the renderer for a frame.
    frame: bool,
}

/// The renderer takes `txn`; the pass is counted the way a shell counts it.
fn pass(view: &mut Recorder, txn: &WrTransaction, frame: bool) -> Pass {
    count_transaction(txn);
    let (added, deleted, updated) = view.take(txn);
    publish_gpu_frame(&view.window, 0, 0, 0);
    let counted = view.window.frame_clock.gpu_last();
    Pass {
        added,
        updated,
        deleted,
        lists: counted.lists_translated,
        items: counted.items_translated,
        frame,
    }
}

/// The pass of a frame whose layout was made again: the full transaction.
fn full(view: &mut Recorder) -> Pass {
    let mut txn = WrTransaction::new();
    build_full_transaction(&mut txn, &mut view.window, &OptionGlContextPtr::None)
        .expect("the full transaction");
    pass(view, &txn, true)
}

/// The pass of a frame that laid nothing out: the lightweight transaction,
/// which is what the platform loop makes while the engine owes frames.
fn follow_up(view: &mut Recorder) -> Pass {
    let mut txn = WrTransaction::new();
    let frame = build_lightweight_transaction(&mut txn, &mut view.window, &OptionGlContextPtr::None)
        .expect("the lightweight transaction");
    pass(view, &txn, frame.changed)
}

/// The pictures of the window's list as a translation made NOW paints them:
/// image key and rect, in list order. (The labels of [`labels`] paint nothing
/// else as an image.)
fn painted(view: &Recorder) -> Vec<(ImageKey, [u32; 4])> {
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
    let mut pictures = Vec::new();
    let mut items = built.iter();
    while let Some(item) = items.next() {
        if let DisplayItem::Image(image) = item.item() {
            let b = image.bounds;
            pictures.push((
                image.image_key,
                [b.min.x.to_bits(), b.min.y.to_bits(), b.max.x.to_bits(), b.max.y.to_bits()],
            ));
        }
    }
    pictures
}

/// How many labels of the window's list a translation made now paints with a
/// picture that stands in for their own.
fn standing_in(view: &Recorder) -> usize {
    let list = view
        .window
        .get_layout_result(&DomId::ROOT_ID)
        .expect("the root DOM is laid out")
        .display_list
        .clone();
    text_halos(&list.items, view.dpi, None)
        .iter()
        .filter(|halo| halo.stand_in)
        .count()
}

/// What a zoom that settles costs the frames after it.
struct Settled {
    /// The passes made while halos were owed after the settle frame.
    follow_ups: usize,
    /// How many of them sent a display list ...
    with_a_list: usize,
    /// ... and how many of those before the last one.
    with_a_list_before_the_last: usize,
    /// The items those translated.
    items: u64,
    /// The pictures given to images the renderer held (a label's own picture
    /// drawn into the place of the one that stood in).
    replaced: usize,
    /// The images added after the settle frame.
    added: usize,
}

/// Twelve labels with solid halos, drawn four pictures a frame; then the zoom
/// settles at 1.5 times the size, as the editor's does: every label is laid
/// out again and needs a new picture. Follows the frames as a shell makes them
/// (the full transaction of the settle frame, then lightweight ones while
/// halos are owed) and checks in EVERY frame what must hold however a picture
/// reaches the renderer:
///
/// - an image is updated only when the renderer holds it (`Recorder::take`);
/// - every label is painted with a halo;
/// - a pass that sent no list left the renderer with a list that paints
///   exactly what a translation made now would paint - the same keys, the same
///   rects - and every key of it is held;
/// - the window comes to rest with every label on its own picture, and then
///   asks for no frame.
fn a_zoom_settles(dpi: f32) -> Settled {
    const LABELS: usize = 12;
    draw_this_many_halos_a_frame_in_this_thread(4);
    let mut view = Recorder::new(dpi);
    // Whatever this thread counted before is not this window's.
    publish_gpu_frame(&view.window, 0, 0, 0);

    view.lay_out(labels(LABELS, 1.0));
    let first = full(&mut view);
    assert_eq!(first.lists, 1, "the first frame sends the list");
    let mut guard = 0;
    while view.window.gpu_state_manager.halos_owed {
        guard += 1;
        assert!(guard < 30, "the first view's halos come");
        let _ = follow_up(&mut view);
    }
    assert_eq!(painted(&view).len(), LABELS, "premise: every label has its halo");

    view.lay_out(labels(LABELS, 1.5));
    let settle = full(&mut view);
    assert_eq!(settle.lists, 1, "the settle frame sends the new list");
    let mut held = painted(&view);
    assert_eq!(held.len(), LABELS, "the settle frame paints every label with a halo");

    let mut seen = Settled {
        follow_ups: 0,
        with_a_list: 0,
        with_a_list_before_the_last: 0,
        items: 0,
        replaced: 0,
        added: 0,
    };
    while view.window.gpu_state_manager.halos_owed {
        seen.follow_ups += 1;
        let n = seen.follow_ups;
        assert!(n < 30, "the halos come");
        // What the passes before this one sent was not the last pass's.
        seen.with_a_list_before_the_last = seen.with_a_list;
        let pass = follow_up(&mut view);
        assert!(pass.frame, "follow-up {n}: a pass that drew halos asks for a frame");
        let now = painted(&view);
        assert_eq!(now.len(), LABELS, "follow-up {n}: a label lost its halo");
        if pass.lists == 0 {
            assert_eq!(
                now, held,
                "follow-up {n} sent no list, and the list the renderer holds is not what a \
                 translation paints now"
            );
        } else {
            seen.with_a_list += 1;
            held = now;
        }
        for (key, _) in &held {
            assert!(
                view.live.contains(key),
                "follow-up {n}: the renderer's list names {key:?}, which it does not hold"
            );
        }
        seen.items += pass.items;
        seen.replaced += pass.updated.len();
        seen.added += pass.added.len();
    }
    assert!(seen.follow_ups >= 2, "premise: the budget spread the pictures over frames");
    assert_eq!(standing_in(&view), 0, "every label has its own picture at last");

    let rest = follow_up(&mut view);
    assert!(!rest.frame, "the halos are drawn: an idle window asks for no frame");
    assert_eq!(
        (rest.lists, rest.added.len(), rest.updated.len(), rest.deleted.len()),
        (0, 0, 0, 0)
    );
    assert_eq!(painted(&view), held, "and paints what the renderer holds");
    seen
}

/// The frames after a zoom settles send pictures, not lists - by construction.
/// Each of them translated every display list of the window and had a scene
/// built (17,495 items a frame on a municipal map, for as many frames as the
/// labels' new pictures take to draw) to tell the renderer that a label's halo
/// is another picture now. A label's own picture is drawn into the place of the
/// picture that stood in for it and replaces it under the image key the list
/// names; one that does not fit the place waits and is shown by the last of
/// these frames. So whatever the glyphs: no frame before the last sends a
/// list, and the eight labels the settle frame's budget left (four it drew)
/// are eight images given or added, none twice.
///
/// Mutation: `halos.insert_waiting(..)` -> `halos.insert(..)` in `text_halos`
/// (a picture that does not fit is shown at once) sends a list in the frame
/// that drew it; `translation_env` with `halos_drawn()` back in it sends the
/// list in every follow-up.
fn no_frame_before_the_last_sends_a_list(dpi: f32) {
    text_halo_slots_in_this_thread(true);
    let seen = a_zoom_settles(dpi);
    assert_eq!(
        seen.with_a_list_before_the_last, 0,
        "a follow-up frame before the last one sent a display list"
    );
    assert!(seen.with_a_list <= 1);
    assert_eq!(seen.replaced + seen.added, 8, "each left label's own picture reached the renderer once");
    if seen.with_a_list == 0 {
        assert_eq!((seen.items, seen.added), (0, 0));
    }
}

#[test]
fn a4_no_halo_frame_before_the_last_sends_a_list_at_1x() {
    no_frame_before_the_last_sends_a_list(1.0);
}

/// The user's display.
#[test]
fn a4_no_halo_frame_before_the_last_sends_a_list_at_2x() {
    no_frame_before_the_last_sends_a_list(2.0);
}

/// The same frames, and what they are for: with the glyphs of these labels
/// every one of the eight pictures fits the place of its stand-in, so the
/// frames after the settle are eight image updates - no image added, no list
/// translated, no scene built. (This one depends on the glyphs' raster: where a
/// picture's ink is told from the last size's to within a pixel. If it fails
/// and the one above holds, the frames are still bounded - one list at the end
/// - and `slot_rect` tells the ink worse than it should.)
///
/// Mutation: `(fitted, place)` -> `(None, place)` in `text_halos` (no picture
/// takes a place) makes the last follow-up add the eight images and send the
/// list.
fn every_picture_takes_the_place_of_its_stand_in(dpi: f32) {
    text_halo_slots_in_this_thread(true);
    let seen = a_zoom_settles(dpi);
    assert_eq!(
        (seen.with_a_list, seen.items),
        (0, 0),
        "{} follow-up frames after the settle translated {} items",
        seen.follow_ups,
        seen.items
    );
    assert_eq!((seen.replaced, seen.added), (8, 0), "eight pictures under the keys the list names");
}

#[test]
fn a4_the_frames_after_a_settle_send_pictures_and_no_list_at_1x() {
    every_picture_takes_the_place_of_its_stand_in(1.0);
}

#[test]
fn a4_the_frames_after_a_settle_send_pictures_and_no_list_at_2x() {
    every_picture_takes_the_place_of_its_stand_in(2.0);
}

/// The switch (`AZ_HALO_SLOTS=0`, here per thread): every picture has a slot and
/// a rect of its own and is shown when it is drawn, as before. A label's own
/// picture is then another image in another rect than its stand-in, the list
/// that painted the stand-in does not paint it, and every follow-up frame that
/// drew a picture sends the list again - the invariants of `a_zoom_settles` hold
/// all the same. This is what the frames cost before, and what the switch goes
/// back to.
#[test]
fn a4_without_slots_every_follow_up_after_a_settle_sends_the_list() {
    text_halo_slots_in_this_thread(false);
    let seen = a_zoom_settles(2.0);
    text_halo_slots_in_this_thread(true);
    assert_eq!(seen.with_a_list, seen.follow_ups, "each follow-up drew pictures and sent the list");
    assert!(seen.items > 0);
    assert_eq!((seen.replaced, seen.added), (0, 8), "eight new images, none replaced");
}

// ==== A tile that arrives: the place kept for a node's picture (audit A4-09) ====

/// A picture of `size` x `size` px, every byte `fill`.
fn picture(size: usize, fill: u8) -> ImageRef {
    ImageRef::new_rawimage(RawImage {
        pixels: RawImageData::U8(vec![fill; size * size * 4].into()),
        width: size,
        height: size,
        premultiplied_alpha: true,
        data_format: RawImageFormat::RGBA8,
        tag: Vec::new().into(),
    })
    .expect("a picture in memory")
}

/// The nodes the window's list paints an image for, with the image, in list
/// order.
fn image_nodes(view: &Recorder) -> Vec<(NodeId, ImageRef)> {
    let list = &view.window.layout_results[&DomId::ROOT_ID].display_list;
    list.items
        .iter()
        .enumerate()
        .filter_map(|(index, item)| match item {
            DisplayListItem::Image { image, .. } => list
                .node_mapping
                .get(index)
                .copied()
                .flatten()
                .map(|node| (node, image.clone())),
            _ => None,
        })
        .collect()
}

/// `image` becomes the picture of `node`, the way `CallbackInfo::change_node_image`
/// applies it. Returns whether the renderer shows the node through a key of
/// its own, so that its list need not go again.
///
/// The three lines around `apply_content_change` are what `apply_image_change`
/// does itself once A3-window has them (`audit/A4.md`, For area A3-window);
/// done twice they do nothing the second time (the list the renderer's record
/// names is then the patched one already, and `before` is not it).
fn arrives(view: &mut Recorder, node: NodeId, image: &ImageRef) -> bool {
    let shown = image_nodes(view)
        .into_iter()
        .find(|(n, _)| *n == node)
        .expect("the node paints an image")
        .1;
    let in_place = view.window.renderer_resources.node_image_is_replaced_in_place(
        DomId::ROOT_ID,
        node,
        &shown,
        image,
    );
    let before = Arc::as_ptr(&view.window.layout_results[&DomId::ROOT_ID].display_list);
    let _ = view.window.apply_content_change(ContentChange::Image {
        dom_id: DomId::ROOT_ID,
        node_id: node,
        image: image.clone(),
        dirty_rect: None,
    });
    if in_place {
        view.window.gpu_state_manager.display_list_moved(
            DomId::ROOT_ID,
            before,
            &view.window.layout_results[&DomId::ROOT_ID].display_list,
        );
    }
    in_place
}

/// A4-09. A map's tile that is being drawn is a node with a null image (a size,
/// no pixels). The node is given the key of its slot at once, showing one clear
/// pixel, and the list paints it from that key; the tile that arrives is new
/// pixels under the key - `update_image` - and the list the renderer holds says
/// the same as before: no item is translated, no scene built. Before, the
/// arrival gave the slot its key, the list had to name it, and every list of
/// the window was translated and sent again: per tile of a municipal map 17,495
/// items and a scene build, up to 25 times a second while tiles came in.
///
/// A node that shows a picture of the DOM has no place: its first replacement
/// sends the lists once (the slot's key is new), every later one is in place.
///
/// Mutation: `rr.image_key_map.insert(key, ImageRefHash::PLACE)` dropped from
/// `reserve_image_places` (no place is kept) makes the arriving tile an added
/// image and sends the list; `display_list_moved` not called (the lines of
/// A3-window missing) translates the list for every arrival.
#[test]
fn a4_09_a_tile_that_arrives_is_new_pixels_under_the_key_its_place_has() {
    let mut view = Recorder::new(1.0);
    publish_gpu_frame(&view.window, 0, 0, 0);
    let at = |x: usize| {
        format!("position: absolute; left: {}px; top: 0px; width: 64px; height: 64px;", 64 * x)
    };
    let being_drawn = || ImageRef::null_image(64, 64, RawImageFormat::RGBA8, Vec::new());
    view.lay_out(
        Dom::create_body().with_css("margin: 0px;").with_child(
            Dom::create_div()
                .with_css("position: relative; width: 192px; height: 64px;")
                .with_child(Dom::create_image(being_drawn()).with_css(at(0).as_str()))
                .with_child(Dom::create_image(being_drawn()).with_css(at(1).as_str()))
                .with_child(Dom::create_image(picture(64, 40)).with_css(at(2).as_str())),
        ),
    );
    let first = full(&mut view);
    assert_eq!(first.lists, 1);
    assert_eq!(first.added.len(), 3, "the DOM's picture, and a place for each tile being drawn");
    let nodes = image_nodes(&view);
    assert_eq!(nodes.len(), 3);
    let held = painted(&view);
    assert_eq!(held.len(), 3, "a node that shows a null image is painted from its place");
    for (key, _) in &held {
        assert!(view.live.contains(key));
    }
    let (tile, _, plain) = (nodes[0].0, nodes[1].0, nodes[2].0);

    // The first tile arrives.
    assert!(arrives(&mut view, tile, &picture(64, 200)), "the node has its place");
    let arrival = follow_up(&mut view);
    assert!(arrival.frame, "new pixels are a frame");
    assert_eq!(
        (arrival.lists, arrival.items),
        (0, 0),
        "a tile that arrives in place translates no item"
    );
    assert_eq!(arrival.updated, vec![held[0].0], "its pixels go under the key the list names");
    assert!(arrival.added.is_empty() && arrival.deleted.is_empty());
    assert_eq!(painted(&view), held, "the list the renderer holds paints the tile");

    // The tile is drawn again (its style changed): in place like any later frame.
    assert!(arrives(&mut view, tile, &picture(64, 90)));
    let again = follow_up(&mut view);
    assert_eq!((again.lists, again.updated.len(), again.added.len()), (0, 1, 0));
    assert_eq!(painted(&view), held);

    // The DOM's own picture is replaced: no place was kept for it, so its slot
    // gets its key now and the list goes once ...
    assert!(!arrives(&mut view, plain, &picture(64, 7)), "no key of its own yet");
    let keyed = follow_up(&mut view);
    assert_eq!((keyed.lists, keyed.added.len()), (1, 1));
    let held = painted(&view);
    assert_eq!(held.len(), 3);
    for (key, _) in &held {
        assert!(view.live.contains(key));
    }
    // ... and never again.
    assert!(arrives(&mut view, plain, &picture(64, 8)));
    let later = follow_up(&mut view);
    assert_eq!((later.lists, later.updated.len(), later.added.len()), (0, 1, 0));
    assert_eq!(painted(&view), held);

    // The second tile is still being drawn: its place is kept.
    let idle = follow_up(&mut view);
    assert!(!idle.frame && idle.deleted.is_empty());
}

// ==== A colour that is a value (audit A4-23, the GPU side) ====

/// How many rects of the window's list a translation made now binds to the
/// colour key `id`.
fn rects_bound_to(view: &Recorder, id: webrender::api::PropertyBindingId) -> usize {
    use webrender::api::PropertyBinding;

    let list = view.window.layout_results[&DomId::ROOT_ID].display_list.clone();
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
    let mut bound = 0;
    let mut items = built.iter();
    while let Some(item) = items.next() {
        if let DisplayItem::Rectangle(rect) = item.item() {
            if matches!(&rect.color, PropertyBinding::Binding(key, _) if key.id == id) {
                bound += 1;
            }
        }
    }
    bound
}

/// A4-23, what the GPU path does with a background whose colour is bound to a
/// key (`GpuValueCache::color_keys`, `DisplayList::color_bindings`: a fade binds
/// one today; a `:hover` background is to be one, see `audit/A4.md`). The
/// translation paints the rect in the key's value, and a colour that changes is
/// then a value sent with the frame: no display list is translated, no image
/// touched, and the same colour sent again is no frame at all. This is all the
/// renderer's side needs for a hover that costs no list.
///
/// Mutation: `gpu_cache.for_each_bound_color(..)` dropped from the first walk of
/// `synchronize_gpu_values` makes the changed colour "the set last sent" (no
/// frame); `display_list.color_binding(item_index)` -> `None` in `compositor2`
/// paints the rect in the colour the list was built with (no rect is bound).
#[test]
fn a4_23_a_bound_background_colour_changes_as_a_value_and_no_list() {
    use azul_core::resources::ColorKey;
    use azul_css::props::basic::ColorU;
    use webrender::api::{ColorF, PropertyBindingKey};

    let mut view = Recorder::new(1.0);
    publish_gpu_frame(&view.window, 0, 0, 0);
    view.lay_out(
        Dom::create_body()
            .with_css("margin: 0px;")
            .with_child(Dom::create_div().with_css("width: 40px; height: 20px; background: #3366cc;")),
    );
    // The box's background gets a colour key, the way the first frame of a fade
    // gives it one: the key and its value in the cache, the rect bound in the list.
    let (node, key) = (NodeId::new(1), ColorKey::unique());
    let shown = ColorU::new(0x33, 0x66, 0xcc, 255);
    {
        let cache = view.window.gpu_state_manager.get_or_create_cache(DomId::ROOT_ID);
        cache.color_keys.insert(node, key);
        cache.current_color_values.insert(node, shown);
    }
    let bound = {
        let result = view
            .window
            .layout_results
            .get_mut(&DomId::ROOT_ID)
            .expect("the root DOM is laid out");
        Arc::make_mut(&mut result.display_list).bind_background_color(node, shown, key)
    };
    assert!(bound >= 1, "premise: the box paints a background rect in that colour");

    let first = full(&mut view);
    assert_eq!(first.lists, 1);
    let id = PropertyBindingKey::<ColorF>::new(key.id as u64).id;
    assert_eq!(rects_bound_to(&view, id), bound, "the list paints the rect in the key's value");

    // The colour changes (a hover): a value.
    view.window
        .gpu_state_manager
        .get_or_create_cache(DomId::ROOT_ID)
        .current_color_values
        .insert(node, ColorU::new(0xff, 0xee, 0x00, 255));
    let mut txn = WrTransaction::new();
    let frame = build_lightweight_transaction(&mut txn, &mut view.window, &OptionGlContextPtr::None)
        .expect("the lightweight transaction");
    assert!(frame.changed, "a colour that changed is a frame");
    count_transaction(&txn);
    publish_gpu_frame(&view.window, 0, 0, 0);
    let counted = view.window.frame_clock.gpu_last();
    assert_eq!(
        (counted.lists_translated, counted.items_translated),
        (0, 0),
        "and no display list"
    );
    assert!(txn.resource_updates.is_empty(), "and no image");
    let colours: Vec<_> = txn
        .get_frame_ops()
        .into_iter()
        .filter_map(|op| match op {
            FrameMsg::AppendDynamicProperties(properties) => Some(properties.colors),
            _ => None,
        })
        .flatten()
        .collect();
    assert_eq!(colours.len(), 1, "the one bound colour");
    assert_eq!(colours[0].key.id, id);
    assert_eq!(
        colours[0].value,
        ColorF::new(1.0, f32::from(0xee_u8) / 255.0, 0.0, 1.0)
    );

    // The colour stands: nothing is sent, no frame is asked for.
    let idle = follow_up(&mut view);
    assert!(!idle.frame);
    assert_eq!(idle.lists, 0);
}

/// A4-10g. The scans of a frame's resources walk a display list for its fonts
/// and images once, and not again while the list is the one the window holds:
/// a second full transaction over the same lists walks none (it walked every
/// item of every list twice, and hashed every glyph under a text shadow).
/// What is kept is what a walk of the list finds.
///
/// Mutation: `list_names` without the `push` walks the list in each scan (the
/// count is 3 and more); an `image_items` that leaves out the last index fails
/// the comparison.
#[test]
fn a4_10g_a_list_the_window_holds_is_walked_for_its_fonts_and_images_once() {
    use std::collections::HashSet;

    use azul_core::resources::Au;

    use crate::desktop::wr_translate2::{list_name_walks, list_names};

    let mut view = Recorder::new(1.0);
    publish_gpu_frame(&view.window, 0, 0, 0);
    // Text without a shadow (no halo is drawn: the passes below are the same on
    // every machine) and one picture.
    view.lay_out(
        Dom::create_body()
            .with_css("margin: 0px;")
            .with_child(
                Dom::create_p().with_css("font-size: 12px; color: #111111;").with_child(
                    Dom::create_text_do_not_use_without_block_level_wrapper("Street".to_string()),
                ),
            )
            .with_child(Dom::create_image(picture(16, 90)).with_css("width: 16px; height: 16px;")),
    );
    let list = Arc::clone(&view.window.layout_results[&DomId::ROOT_ID].display_list);

    let before = list_name_walks();
    let names = list_names(&list);
    assert_eq!(list_name_walks() - before, 1);
    let fonts: HashSet<(u64, Au)> = list
        .items
        .iter()
        .filter_map(|item| match item {
            DisplayListItem::Text {
                font_hash,
                font_size_px,
                ..
            } => Some((font_hash.font_hash, Au::from_px(*font_size_px))),
            _ => None,
        })
        .collect();
    assert!(!fonts.is_empty(), "the labels are text");
    assert_eq!(names.fonts.len(), fonts.len(), "each once");
    assert_eq!(names.fonts.iter().copied().collect::<HashSet<_>>(), fonts);
    let image_items: Vec<usize> = list
        .items
        .iter()
        .enumerate()
        .filter(|(_, item)| LayoutWindow::image_of_item(item).is_some())
        .map(|(index, _)| index)
        .collect();
    assert_eq!(image_items.len(), 1, "the one picture of the DOM");
    assert_eq!(names.image_items, image_items);

    // Two full transactions over the lists the window holds: no walk.
    let first = full(&mut view);
    assert_eq!(first.lists, 1);
    assert_eq!(first.added.len(), 1, "the picture is registered from what is kept");
    let _ = full(&mut view);
    assert_eq!(list_name_walks() - before, 1, "one walk of the list for all of it");

    // A list laid out again is another list: walked once.
    view.lay_out(labels(2, 1.0));
    let _ = full(&mut view);
    assert_eq!(list_name_walks() - before, 2);
}

/// A6-26. A frame that wrote no GPU value and shows the lists of the frame before walks no
/// bound value (it used to hash every one of them to learn that the set is the one last
/// sent). A value written straight into a public map of a cache is walked for and sent; the
/// value it has, written again, is not; a list built again with the same values is walked for
/// once (and found to be the same set); another scale and a full frame are walked for and sent.
///
/// Counted: the values the function walks for its own answer (`BOUND_VALUES_WALKED`; the walk
/// a debug build makes to check a skip is not counted).
/// Mutations: the skip taken out fails "nothing was written"; `values_as_digested` forced
/// true fails "a value moved" (and the debug assertion); the lists left out of the manager's
/// comparison fails "another list".
#[test]
fn a6_26_a_frame_with_nothing_written_walks_no_bound_value() {
    let walked = || BOUND_VALUES_WALKED.with(std::cell::Cell::get);
    let sync = |lw: &mut LayoutWindow| synchronize_gpu_values(lw, &mut WrTransaction::new());
    let mut lw = window();
    let (a, b) = (OpacityKey::unique(), OpacityKey::unique());
    bind_opacity(&mut lw, 0, a, 0.25);
    bind_opacity(&mut lw, 1, b, 0.75);

    let before = walked();
    assert!(sync(&mut lw), "the first set is sent");
    let per_walk = walked() - before;
    assert!(per_walk >= 2, "the two opacities at least: {per_walk}");

    // Nothing written: three frames walk nothing and send nothing.
    let before = walked();
    for _ in 0..3 {
        assert!(!sync(&mut lw));
    }
    assert_eq!(walked() - before, 0, "nothing was written: no value is walked");

    // The value it has, written again (a thumb that stays, a fade in its hold).
    bind_opacity(&mut lw, 0, a, 0.25);
    assert!(!sync(&mut lw));
    assert_eq!(walked() - before, 0, "the same value is no write");

    // Another value, straight into the public maps: one walk, and the set is sent.
    bind_opacity(&mut lw, 0, a, 0.5);
    assert!(sync(&mut lw), "a value moved");
    assert_eq!(walked() - before, per_walk);
    assert!(!sync(&mut lw));
    assert_eq!(walked() - before, per_walk, "and no walk after it");

    // The root's list built again with the same items and the same values.
    let rebuilt = Arc::new((*lw.layout_results[&DomId::ROOT_ID].display_list).clone());
    lw.layout_results.get_mut(&DomId::ROOT_ID).expect("the root").display_list = rebuilt;
    let before = walked();
    assert!(!sync(&mut lw), "the same set: nothing to send");
    assert_eq!(walked() - before, per_walk, "another list: its values are walked");
    assert!(!sync(&mut lw));
    assert_eq!(walked() - before, per_walk);

    // Another scale: other device px, walked and sent.
    lw.current_window_state.size.dpi = 192;
    let before = walked();
    assert!(sync(&mut lw), "another scale is another set");
    assert_eq!(walked() - before, per_walk);

    // A full frame forgets what was sent.
    lw.gpu_state_manager.invalidate_submitted_digests();
    let before = walked();
    assert!(sync(&mut lw));
    assert_eq!(walked() - before, per_walk);
    assert!(!sync(&mut lw));
    assert_eq!(walked() - before, per_walk);
}

// ==== A4-31: a display list is sent without the wait, and nothing overtakes it ====

mod a_settle_sent_without_the_wait {
    //! The passes of a window over two zoom settles, built by the real
    //! builders and sent the way `FrameSchedule` says, against a model of the
    //! renderer's two queues in which the scene builder is as slow as it can
    //! be (`frame_schedule::Wires`). What reaches the render backend is taken
    //! by A5's recorder in the order it arrives, which fails on an update of
    //! an image the renderer does not hold.

    use std::time::{Duration, Instant};

    use webrender::render_api::ResourceUpdate;

    use super::*;
    use crate::desktop::shell2::common::frame_schedule::{
        FramePass, FrameSchedule, FrameSend, FrameShape, Wires,
    };

    /// What a pass handed the renderer: images added, given new pixels and
    /// deleted, and whether a display list went.
    type Handed = (usize, usize, usize, bool);

    struct Shell {
        view: Recorder,
        schedule: FrameSchedule,
        wires: Wires<WrTransaction>,
        handed: Vec<Handed>,
        /// How many of the applied transactions the recorder has taken.
        taken: usize,
        /// The passes that were sent without the wait.
        without_the_wait: usize,
        /// `AZ_WR_SYNC_SCENE=1`.
        in_step: bool,
        now: Instant,
    }

    impl Shell {
        fn new(in_step: bool) -> Self {
            let view = Recorder::new(1.0);
            // Whatever this thread counted before is not this window's.
            publish_gpu_frame(&view.window, 0, 0, 0);
            Self {
                view,
                schedule: FrameSchedule::new(),
                wires: Wires::new(),
                handed: Vec::new(),
                taken: 0,
                without_the_wait: 0,
                in_step,
                now: Instant::now(),
            }
        }

        /// The send of a pass, as `MacOSWindow::render_and_present_in_draw_rect`
        /// makes it: ask the schedule, route, send, wait if told to.
        fn pass(&mut self, mut txn: WrTransaction, first_frame: bool) -> FrameSend {
            let n = self.handed.len();
            let shape = FrameShape::of(&txn, false);
            let said = self.schedule.on_send(
                shape,
                FramePass {
                    forced_sync: self.in_step || first_frame,
                    frame_waiting: false,
                    now: self.now,
                },
            );
            said.route(&mut txn);

            let (mut added, mut updated, mut deleted) = (0, 0, 0);
            for update in &txn.resource_updates {
                match update {
                    ResourceUpdate::AddImage(_) => added += 1,
                    ResourceUpdate::UpdateImage(_) => updated += 1,
                    ResourceUpdate::DeleteImage(_) => deleted += 1,
                    _ => {}
                }
            }
            self.handed.push((added, updated, deleted, shape.scene.is_some()));

            let takes_queue = txn.takes_scene_builder_queue();
            assert!(
                takes_queue || !self.wires.anything_queued(),
                "pass {n} goes straight to the render backend, past what waits in the scene \
                 builder's queue"
            );
            let _ = self.wires.send(takes_queue, shape.scene.is_some(), txn);
            if said.flush {
                self.wires.drain();
            } else {
                self.without_the_wait += 1;
            }
            self.the_renderer_takes_what_arrived();
            if said.render {
                // No pass has a frame waiting here, so a pass that draws is
                // one that waited: its frame is of everything sent so far,
                // the last display list included.
                assert!(
                    !self.wires.anything_queued() && self.schedule.scene_in_flight().is_none(),
                    "pass {n} draws while a display list is still on its way"
                );
            }
            said
        }

        /// The render backend applies what reached it, in the order it
        /// arrived: the recorder fails on an update of an image it does not
        /// hold.
        fn the_renderer_takes_what_arrived(&mut self) {
            assert!(self.wires.in_order(), "applied in another order than sent");
            while self.taken < self.wires.applied.len() {
                let _ = self.view.take(&self.wires.applied[self.taken].1);
                self.taken += 1;
            }
        }

        /// The pass of a frame whose layout was made again.
        fn full(&mut self, first_frame: bool) -> FrameSend {
            let mut txn = WrTransaction::new();
            build_full_transaction(&mut txn, &mut self.view.window, &OptionGlContextPtr::None)
                .expect("the full transaction");
            self.pass(txn, first_frame)
        }

        /// The pass of a frame that laid nothing out; `None` when it changed
        /// nothing (the shell drops such a transaction).
        fn lightweight(&mut self) -> Option<FrameSend> {
            let mut txn = WrTransaction::new();
            let frame = build_lightweight_transaction(
                &mut txn,
                &mut self.view.window,
                &OptionGlContextPtr::None,
            )
            .expect("the lightweight transaction");
            if frame.changed {
                Some(self.pass(txn, false))
            } else {
                None
            }
        }

        /// The scene builder gets as far as the scene in flight, the
        /// renderer's wake brings a pass, and the renderer reports the
        /// scene's epoch: what the shell does then.
        fn the_scene_lands(&mut self) {
            let Some((_, epoch)) = self.schedule.scene_in_flight() else {
                return;
            };
            self.wires.drain_through_the_scene();
            self.the_renderer_takes_what_arrived();
            if self.schedule.on_published(|_| Some(epoch)) {
                self.view
                    .window
                    .gpu_state_manager
                    .invalidate_submitted_digests();
            }
        }

        /// Lightweight passes until no halo is owed and a pass changes nothing.
        fn come_to_rest(&mut self) {
            for _ in 0..60 {
                if self.lightweight().is_none() && !self.view.window.gpu_state_manager.halos_owed {
                    return;
                }
            }
            panic!("the window does not come to rest");
        }
    }

    /// A map of twelve labels whose zoom settles, then another view, on a
    /// thread of its own (the kept halos are a thread's): what every pass
    /// handed over, and how many passes went without the wait.
    fn two_settles(in_step: bool) -> (Vec<Handed>, usize) {
        std::thread::spawn(move || {
            text_halo_slots_in_this_thread(true);
            draw_this_many_halos_a_frame_in_this_thread(4);
            let mut shell = Shell::new(in_step);
            let key = OpacityKey::unique();

            // The window's first frame is in step, whatever it sends.
            shell.view.lay_out(labels(12, 1.0));
            let first = shell.full(true);
            assert!(first.flush && first.render, "the first frame is drawn by its pass");
            shell.come_to_rest();

            // The zoom settles at 1.5: a new list, and four halos a frame.
            shell.view.lay_out(labels(12, 1.5));
            let settle = shell.full(false);
            assert_eq!(
                (settle.flush, settle.render),
                (in_step, in_step),
                "a display list waits only when the pass must be in step"
            );
            assert_eq!(shell.schedule.scene_in_flight().is_some(), !in_step);
            assert!(
                shell.view.window.gpu_state_manager.halos_owed,
                "premise: the settle left halos for the frames that follow"
            );
            // While the scene is built: two halo frames. Their pictures - an
            // `add_image`, an `update_image` under a key the scene's list
            // names - queue behind the scene (`Shell::pass` and the recorder
            // check each of them).
            for _ in 0..2 {
                let _ = shell.lightweight();
            }
            shell.the_scene_lands();
            shell.come_to_rest();

            // The second new list, of a view without a label (no halo is
            // owed after it): then a zoom step while the scene is built - a
            // transaction of nothing but values, which its builder lets skip
            // the scene builder.
            shell.view.lay_out(Dom::create_body().with_child(
                Dom::create_div().with_css("width: 40px; height: 20px; background: #3366cc;"),
            ));
            let settle = shell.full(false);
            assert_eq!(settle.flush, in_step);
            assert!(
                !shell.view.window.gpu_state_manager.halos_owed,
                "premise: no halo is left, the next pass carries values only"
            );
            bind_opacity(&mut shell.view.window, 0, key, 0.5);
            let step = shell.lightweight().expect("a value that moved is a frame");
            assert_eq!(shell.handed.last(), Some(&(0, 0, 0, false)), "premise: values only");
            assert_eq!(
                (step.through_scene_builder, step.flush, step.render),
                (!in_step, in_step, in_step),
                "a zoom step behind a scene in flight queues behind it and is not drawn yet"
            );
            shell.the_scene_lands();
            // The step after the scene landed: the one before may still be
            // queued, so this one takes the queue too, and is waited for.
            bind_opacity(&mut shell.view.window, 0, key, 0.75);
            let step = shell.lightweight().expect("a value that moved is a frame");
            assert_eq!(
                (step.through_scene_builder, step.flush, step.render),
                (!in_step, true, true)
            );
            shell.come_to_rest();

            // At rest: nothing queued, nothing in flight, and the schedule
            // asks for no pass however long the window is left alone.
            assert!(!shell.wires.anything_queued());
            assert!(!shell.schedule.unsettled());
            assert!(!shell.schedule.valve_due(shell.now + Duration::from_secs(3600)));
            assert!(shell.lightweight().is_none(), "an idle window sends nothing");
            assert_eq!(shell.taken, shell.wires.applied.len());
            (shell.handed, shell.without_the_wait)
        })
        .join()
        .expect("the shell's thread")
    }

    /// A4-31. With the wait dropped for display lists the renderer is handed
    /// the same things in the same order as with `AZ_WR_SYNC_SCENE=1` - the
    /// builders do not know the schedule, and this is the guard that they
    /// stay so - while nothing overtakes a scene in flight (asserted in every
    /// pass of both runs: `Shell::pass`), no image is updated before the
    /// render backend holds it (`Recorder::take`), no pass draws while a
    /// display list is on its way, and the window comes to rest asking for
    /// nothing.
    /// Mutations: `through_scene_builder = shape.scene.is_some()` in
    /// `FrameSchedule::on_send` fails "past what waits in the scene builder's
    /// queue" at the zoom step behind the second settle; `self.queued =
    /// false` in `on_published` fails it at the step after the scene landed;
    /// `(false, true) => true` (every list waits) fails the premise below.
    #[test]
    fn a4_31_a_settle_sent_without_the_wait_hands_over_the_same_things_in_the_same_order() {
        let (in_step, waited_for_none) = two_settles(true);
        let (without, sent_without_the_wait) = two_settles(false);
        assert_eq!(waited_for_none, 0, "AZ_WR_SYNC_SCENE=1: every pass waits");
        assert!(
            sent_without_the_wait >= 3,
            "premise: the two settles and a pass behind them went without the wait: \
             {sent_without_the_wait}"
        );

        // A pass that handed over values and offsets only is not compared:
        // after a scene landed the window sends those once more.
        let things = |passes: &[Handed]| -> Vec<Handed> {
            passes
                .iter()
                .copied()
                .filter(|pass| *pass != (0, 0, 0, false))
                .collect()
        };
        assert_eq!(things(&without), things(&in_step));
        assert!(
            things(&in_step).iter().filter(|pass| pass.3).count() >= 3,
            "premise: a list per view at the least"
        );
    }
}

// ==== A4-29: a view's zoom is served by the tiles the renderer has ====

mod a_views_zoom_frame {
    //! A window with a view whose root carries the zoom, as a map's picture
    //! root does (`transform: translate(0px, 0px)` on the root of the view's
    //! own DOM, replaced by a translate and a scale while the user zooms):
    //! what the transactions hand the renderer.

    use azul_core::{
        callbacks::{VirtualViewCallback, VirtualViewCallbackInfo, VirtualViewReturn},
        geom::{LogicalPosition, LogicalRect},
        refany::RefAny,
        transform::ComputedTransform3D,
    };
    use azul_layout::managers::gpu_state::ZoomFrameState;
    use webrender::api::PropertyBindingId;

    use super::*;
    use crate::desktop::compositor2::ZOOM_FRAMES;

    /// The picture: a root that carries a transform, and sixty boxes in it.
    extern "C" fn picture(_data: RefAny, info: VirtualViewCallbackInfo) -> VirtualViewReturn {
        let mut root = Dom::create_div()
            .with_css("transform: translate(0px, 0px); transform-origin: 0px 0px;");
        for i in 0..60 {
            root = root.with_child(Dom::create_div().with_css(if i % 2 == 0 {
                "height: 20px; background: #3366cc;"
            } else {
                "height: 20px; background: #cc6633;"
            }));
        }
        let rect = LogicalRect::new(LogicalPosition::zero(), info.bounds.logical_size);
        VirtualViewReturn::with_dom(root, rect, rect)
    }

    /// A host page: a bar, and the view over the rest.
    pub(super) fn window() -> LayoutWindow {
        let mut dom = Dom::create_body()
            .with_child(Dom::create_div().with_css("height: 30px; background: #444444;"))
            .with_child(
                Dom::create_virtual_view(RefAny::new(()), VirtualViewCallback::create(picture))
                    .with_css("width: 300px; height: 200px;"),
            );
        let styled_dom = StyledDom::create(&mut dom, azul_css::css::Css::empty());
        let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
        let mut ws = FullWindowState::default();
        ws.size.dimensions = LogicalSize::new(400.0, 300.0);
        lw.current_window_state = ws.clone();
        lw.layout_new_generation(
            styled_dom,
            &ws,
            &RendererResources::default(),
            &ExternalSystemCallbacks::rust_internal(),
            &mut None,
        )
        .expect("the host lays out");
        lw
    }

    pub(super) fn view_of(lw: &LayoutWindow) -> DomId {
        *lw.layout_results
            .keys()
            .find(|dom| **dom != DomId::ROOT_ID)
            .expect("the view's own DOM is laid out")
    }

    /// The view's zoom frame as the window has it recorded.
    pub(super) fn zoom_of(lw: &LayoutWindow) -> Option<ZoomFrameState> {
        lw.gpu_state_manager.zoom_frames.get(&view_of(lw)).copied()
    }

    /// The app writes `matrix` as the transform of the view's root (what
    /// `set_css_property` of a GPU-only value does).
    pub(super) fn write_zoom(lw: &mut LayoutWindow, node: NodeId, matrix: ComputedTransform3D) {
        let view = view_of(lw);
        let cache = lw.gpu_state_manager.get_or_create_cache(view);
        cache.css_current_transform_values.insert(node, matrix);
    }

    pub(super) fn scaled(s: f32, tx: f32, ty: f32) -> ComputedTransform3D {
        let mut m = ComputedTransform3D::IDENTITY;
        m.m[0][0] = s;
        m.m[1][1] = s;
        m.m[3][0] = tx;
        m.m[3][1] = ty;
        m
    }

    pub(super) fn turned() -> ComputedTransform3D {
        let mut m = ComputedTransform3D::IDENTITY;
        m.m[0][0] = 0.0;
        m.m[0][1] = 1.0;
        m.m[1][0] = -1.0;
        m.m[1][1] = 0.0;
        m
    }

    /// What a transaction carries, read off it: the display list it sends (its
    /// pipeline's DOM), whether the value of `key` is among its bound values,
    /// and what it tells the renderer about a zoom of `key`'s frame.
    pub(super) struct Carried {
        pub(super) list_of: Option<u32>,
        pub(super) value: Option<webrender::api::units::LayoutTransform>,
        pub(super) zooming: Vec<bool>,
    }

    pub(super) fn carried(txn: WrTransaction, key: usize) -> Carried {
        let list_of = txn.first_display_list().map(|(pipeline, _)| pipeline.0);
        let (mut value, mut zooming) = (None, Vec::new());
        for op in txn.get_frame_ops() {
            match op {
                FrameMsg::AppendDynamicProperties(properties) => {
                    value = properties
                        .transforms
                        .iter()
                        .find(|v| v.key.id == PropertyBindingId::new(key as u64))
                        .map(|v| v.value)
                        .or(value);
                }
                FrameMsg::SetIsTransformAsyncZooming(is, id)
                    if id == PropertyBindingId::new(key as u64) =>
                {
                    zooming.push(is);
                }
                _ => {}
            }
        }
        Carried {
            list_of,
            value,
            zooming,
        }
    }

    /// The pass of a frame that laid nothing out.
    pub(super) fn step(lw: &mut LayoutWindow, key: usize) -> (bool, Carried) {
        let mut txn = WrTransaction::new();
        let frame = build_lightweight_transaction(&mut txn, lw, &OptionGlContextPtr::None)
            .expect("the lightweight transaction");
        (frame.changed, carried(txn, key))
    }

    /// A4-29, the decision and the promise. The view's list is sent with its
    /// root frame as the zoom frame (the window's own list has none). A step
    /// of the zoom - another scale, another shift - is then a VALUE: no
    /// display list is translated or sent. The step that makes the frame
    /// anything but a scale and a translation sends the view's list in the
    /// SAME transaction as that value, without a zoom frame: the renderer is
    /// never handed such a value under a frame declared a 2D scale /
    /// translation. With `AZ_WR_ZOOM_TILES=0` no frame is declared at all.
    /// Mutations: `keep_zoom_promises` not called from the lightweight
    /// builder sends the turn without a list; `zoom_frame_now` answering for
    /// any matrix keeps the frame declared under the turn; `dom_id.inner == 0
    /// ||` dropped from `zoom_frame_of` could declare a frame of the window's
    /// own list.
    #[test]
    fn a4_29_a_zoom_step_is_a_value_and_a_turn_sends_the_list_with_it() {
        ZOOM_FRAMES.with(|on| on.set(Some(true)));
        let mut lw = window();
        let view = view_of(&lw);
        publish_gpu_frame(&lw, 0, 0, 0);

        let mut txn = WrTransaction::new();
        build_full_transaction(&mut txn, &mut lw, &OptionGlContextPtr::None)
            .expect("the full transaction");
        let zoom = zoom_of(&lw).expect("the view's root frame is its zoom frame");
        assert_eq!((zoom.scale, zoom.zooming), (1.0, false));
        assert!(
            !lw.gpu_state_manager.zoom_frames.contains_key(&DomId::ROOT_ID),
            "the window's own list has no zoom frame"
        );
        assert_eq!(lw.gpu_state_manager.zoom_frames.len(), 1);
        count_transaction(&txn);
        publish_gpu_frame(&lw, 0, 0, 0);
        assert_eq!(lw.frame_clock.gpu_last().lists_translated, 2, "the host's and the view's");

        // A zoom step: scale 1.25 about a point, i.e. a scale and a shift.
        write_zoom(&mut lw, zoom.node, scaled(1.25, -40.0, -25.0));
        let (changed, sent) = step(&mut lw, zoom.key);
        assert!(changed, "the value moved: a frame");
        assert_eq!(sent.list_of, None, "a zoom step sends no display list");
        let value = sent.value.expect("the frame's new value is in the transaction");
        assert_eq!((value.m11, value.m22, value.m41, value.m42), (1.25, 1.25, -40.0, -25.0));
        publish_gpu_frame(&lw, 0, 0, 0);
        let counted = lw.frame_clock.gpu_last();
        assert_eq!((counted.lists_translated, counted.items_translated), (0, 0));
        assert_eq!(zoom_of(&lw).map(|z| z.key), Some(zoom.key), "still the view's zoom frame");

        // The app turns the picture: not a scale and a translation any more.
        write_zoom(&mut lw, zoom.node, turned());
        let (changed, sent) = step(&mut lw, zoom.key);
        assert!(changed);
        assert_eq!(
            sent.list_of,
            Some(view.inner as u32),
            "the view's list goes with the value that breaks the promise"
        );
        let value = sent.value.expect("and the value is in the same transaction");
        assert_eq!((value.m11, value.m12), (0.0, 1.0));
        assert_eq!(zoom_of(&lw), None, "the list the renderer holds now declares no zoom frame");

        // Back to a plain matrix: the frame is not declared again by itself
        // (the list the renderer holds is the one without it) ...
        write_zoom(&mut lw, zoom.node, scaled(1.5, 0.0, 0.0));
        let (_, sent) = step(&mut lw, zoom.key);
        assert_eq!(sent.list_of, None);
        assert_eq!(zoom_of(&lw), None);

        // The switch: a window made with it off declares nothing.
        ZOOM_FRAMES.with(|on| on.set(Some(false)));
        let mut off = window();
        let mut txn = WrTransaction::new();
        build_full_transaction(&mut txn, &mut off, &OptionGlContextPtr::None)
            .expect("the full transaction");
        ZOOM_FRAMES.with(|on| on.set(None));
        assert!(off.gpu_state_manager.zoom_frames.is_empty(), "AZ_WR_ZOOM_TILES=0");
    }

    /// A4-29, the gesture. The renderer keeps the tiles below the zoom frame
    /// (and draws no glyph again) only while it has been told that the frame
    /// is being zoomed. The first step that changes the SCALE says so, once;
    /// the steps after it say nothing more; a step that only shifts says
    /// nothing at all. While the zoom runs the window owes frames (the
    /// platform loop comes back), and a pass in between that changed nothing
    /// sends nothing. When the scale has stood still for the settle time the
    /// renderer is told the zoom is over - a reason for a frame, in which it
    /// draws the tiles at the scale reached - and the window owes nothing. A
    /// new list for the view ends a running zoom too.
    /// Mutations: `!zoom.zooming` dropped says "zooming" on every step;
    /// `scale.to_bits() != zoom.scale.to_bits()` replaced by `true` says it
    /// for a shift; the settle branch dropped leaves the window owing frames
    /// for ever; `>= ZOOM_SETTLE_US` -> `> ZOOM_SETTLE_US` fails the settle at
    /// the settle time; the `false` of `note_zoom_frames` dropped fails the
    /// last block.
    #[test]
    fn a4_29_the_renderer_is_told_when_a_zoom_begins_and_when_it_has_come_to_rest() {
        use crate::desktop::wr_translate2::{follow_zoom_gestures, ZOOM_SETTLE_US};

        ZOOM_FRAMES.with(|on| on.set(Some(true)));
        let mut lw = window();
        let view = view_of(&lw);
        let mut txn = WrTransaction::new();
        build_full_transaction(&mut txn, &mut lw, &OptionGlContextPtr::None)
            .expect("the full transaction");
        let zoom = zoom_of(&lw).expect("premise: the view's root frame is its zoom frame");
        assert!(carried(txn, zoom.key).zooming.is_empty(), "a new list says nothing of a zoom");
        assert!(!lw.gpu_state_manager.frames_owed());

        // A shift alone (a pan carried by the transform): a value, no gesture.
        write_zoom(&mut lw, zoom.node, scaled(1.0, 30.0, 0.0));
        let (changed, sent) = step(&mut lw, zoom.key);
        assert!(changed && sent.value.is_some());
        assert!(sent.zooming.is_empty(), "a shift is the compositor's: nothing to tell");
        assert!(!lw.gpu_state_manager.frames_owed());

        // Three steps of a zoom: "zooming" with the first, nothing after.
        let mut said = Vec::new();
        for scale in [1.1, 1.21, 1.331] {
            write_zoom(&mut lw, zoom.node, scaled(scale, 30.0, 0.0));
            let (changed, sent) = step(&mut lw, zoom.key);
            assert!(changed);
            assert_eq!(sent.list_of, None, "scale {scale}: a step sends no list");
            said.extend(sent.zooming);
            assert!(lw.gpu_state_manager.frames_owed(), "scale {scale}: the zoom runs");
        }
        assert_eq!(said, [true], "told once that the frame is being zoomed");
        let running = zoom_of(&lw).expect("the frame");
        assert!(running.zooming);
        assert_eq!(running.scale, 1.331);

        // A pass while the zoom rests but has not settled: nothing to send.
        let (changed, sent) = step(&mut lw, zoom.key);
        assert!(!changed, "nothing moved: the shell drops this transaction");
        assert!(sent.zooming.is_empty());
        assert!(lw.gpu_state_manager.frames_owed());

        // The clock: one microsecond before the settle time nothing is said,
        // at the settle time the renderer is told, once.
        let last_change = zoom_of(&lw).expect("the frame").changed_at_us;
        let mut txn = WrTransaction::new();
        assert!(!follow_zoom_gestures(&mut txn, &mut lw, last_change + ZOOM_SETTLE_US - 1));
        assert!(carried(txn, zoom.key).zooming.is_empty());
        let mut txn = WrTransaction::new();
        assert!(
            follow_zoom_gestures(&mut txn, &mut lw, last_change + ZOOM_SETTLE_US),
            "the zoom came to rest: a reason for a frame"
        );
        assert_eq!(carried(txn, zoom.key).zooming, [false]);
        assert!(!lw.gpu_state_manager.frames_owed(), "nothing is owed for the zoom any more");
        let mut txn = WrTransaction::new();
        assert!(!follow_zoom_gestures(&mut txn, &mut lw, last_change + 10 * ZOOM_SETTLE_US));
        assert!(carried(txn, zoom.key).zooming.is_empty(), "told once");

        // A zoom that is running when the view's list goes again (the app's
        // own settle render): the mark is taken back with the new list.
        write_zoom(&mut lw, zoom.node, scaled(2.0, 0.0, 0.0));
        let (_, sent) = step(&mut lw, zoom.key);
        assert_eq!(sent.zooming, [true]);
        let _ = lw.gpu_state_manager.stale_display_lists.insert(view);
        let (changed, sent) = step(&mut lw, zoom.key);
        ZOOM_FRAMES.with(|on| on.set(None));
        assert!(changed);
        assert_eq!(sent.list_of, Some(view.inner as u32), "premise: the view's list went again");
        assert_eq!(sent.zooming, [false], "the frame of the list before is not being zoomed");
        let again = zoom_of(&lw).expect("the new list declares the frame again");
        assert!(!again.zooming && again.scale == 2.0);
        assert!(!lw.gpu_state_manager.frames_owed());
    }
}
