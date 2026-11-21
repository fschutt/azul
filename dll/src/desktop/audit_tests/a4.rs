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
        WrTransaction,
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
