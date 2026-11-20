//! The reference frames of a display list are found in one walk ([`FrameIndex`]), not by walking
//! from each push to its pop on every ask: the extents agree with the walked ones, a list is
//! walked once however often it is asked, a count that grows with the list and not with its
//! asks or its depth, and a list that changed is not answered from what it was.

use azul_core::{
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    resources::TransformKey,
    transform::ComputedTransform3D,
};
use azul_css::props::basic::ColorU;

use crate::{
    cpu_counters::{self, Counter},
    solver3::display_list::{
        reference_frame_extent, reference_frame_extent_walked, BorderRadius, DisplayList,
        DisplayListItem, WindowLogicalRect,
    },
};

fn wlr(x: f32, y: f32, w: f32, h: f32) -> WindowLogicalRect {
    LogicalRect::new(LogicalPosition::new(x, y), LogicalSize::new(w, h)).into()
}

fn frame(key: usize, x: f32, y: f32, w: f32, h: f32) -> DisplayListItem {
    DisplayListItem::PushReferenceFrame {
        transform_key: TransformKey { id: key },
        initial_transform: ComputedTransform3D::IDENTITY,
        bounds: wlr(x, y, w, h),
    }
}

fn rect(x: f32, y: f32, w: f32, h: f32) -> DisplayListItem {
    DisplayListItem::Rect {
        bounds: wlr(x, y, w, h),
        color: ColorU { r: 10, g: 20, b: 30, a: 255 },
        border_radius: BorderRadius::default(),
    }
}

fn clip(x: f32, y: f32, w: f32, h: f32) -> DisplayListItem {
    DisplayListItem::PushClip { bounds: wlr(x, y, w, h), border_radius: BorderRadius::default() }
}

fn scroll(id: u64, x: f32, y: f32, w: f32, h: f32) -> DisplayListItem {
    DisplayListItem::PushScrollFrame {
        clip_bounds: wlr(x, y, w, h),
        content_size: LogicalSize::new(w, h * 5.0),
        scroll_id: id,
    }
}

fn list(items: Vec<DisplayListItem>) -> DisplayList {
    DisplayList { items, ..Default::default() }
}

/// Where the matching pop of the frame pushed at `push` is, by counting (the list's length when
/// it has none).
fn pop_of(items: &[DisplayListItem], push: usize) -> usize {
    let mut depth = 0usize;
    for (i, item) in items.iter().enumerate().skip(push) {
        match item {
            DisplayListItem::PushReferenceFrame { .. } => depth += 1,
            DisplayListItem::PopReferenceFrame => {
                depth -= 1;
                if depth == 0 {
                    return i;
                }
            }
            _ => {}
        }
    }
    items.len()
}

/// A list of frames, clips, scroll frames and rects in a random order: deep nests, frames never
/// popped, clips opened inside one frame and closed in another, scroll frames around frames.
fn random_list(seed: u64, len: usize) -> DisplayList {
    let mut state = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
    let mut next = move |modulo: u64| {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state % modulo
    };
    let (mut frames, mut clips, mut scrolls) = (0usize, 0usize, 0usize);
    let mut items = Vec::new();
    let mut key = 0usize;
    for _ in 0..len {
        let (x, y) = (next(400) as f32, next(400) as f32);
        let (w, h) = (1.0 + next(200) as f32, 1.0 + next(200) as f32);
        match next(9) {
            0 | 1 => {
                items.push(frame(key, x, y, w, h));
                key += 1;
                frames += 1;
            }
            2 if frames > 0 => {
                items.push(DisplayListItem::PopReferenceFrame);
                frames -= 1;
            }
            3 => {
                items.push(clip(x, y, w, h));
                clips += 1;
            }
            4 if clips > 0 => {
                items.push(DisplayListItem::PopClip);
                clips -= 1;
            }
            5 => {
                items.push(scroll(key as u64, x, y, w, h));
                scrolls += 1;
            }
            6 if scrolls > 0 => {
                items.push(DisplayListItem::PopScrollFrame);
                scrolls -= 1;
            }
            _ => items.push(rect(x, y, w, h)),
        }
    }
    list(items)
}

#[test]
fn the_index_gives_the_extent_and_the_pop_the_walk_from_each_push_gives() {
    let mut frames_checked = 0usize;
    for seed in 1..=300u64 {
        let list = random_list(seed, 10 + (seed as usize % 90));
        for (i, item) in list.items.iter().enumerate() {
            let is_push = matches!(item, DisplayListItem::PushReferenceFrame { .. });
            assert_eq!(
                reference_frame_extent(&list, i),
                reference_frame_extent_walked(&list.items, i),
                "seed {seed}: the extent of item {i}"
            );
            if is_push {
                frames_checked += 1;
                assert_eq!(
                    list.frame_index().end_of(i),
                    Some(pop_of(&list.items, i)),
                    "seed {seed}: the pop of the frame at {i}"
                );
            } else {
                assert_eq!(reference_frame_extent(&list, i), None, "only a push has an extent");
            }
        }
    }
    assert!(frames_checked > 1000, "the lists hold frames: {frames_checked}");
}

/// A frame inside a clip that was opened before it, a frame in a scroll frame, a frame inside a
/// frame: the cases the per-frame clip and scroll counting differs on.
#[test]
fn nested_frames_clips_and_scroll_frames_count_from_each_frames_own_start() {
    let items = vec![
        clip(0.0, 0.0, 50.0, 50.0),          // 0: around everything below, but not "inside" frame 1
        frame(1, 10.0, 10.0, 5.0, 5.0),      // 1
        rect(0.0, 0.0, 300.0, 300.0),        // 2: cut to the outer clip only by frame 1's walk
        clip(20.0, 20.0, 10.0, 10.0),        // 3
        frame(2, 0.0, 0.0, 1.0, 1.0),        // 4
        rect(0.0, 0.0, 100.0, 100.0),        // 5: inside both clips
        DisplayListItem::PopReferenceFrame,  // 6: closes 4
        DisplayListItem::PopClip,            // 7
        scroll(9, 100.0, 100.0, 40.0, 40.0), // 8
        rect(500.0, 500.0, 9.0, 9.0),        // 9: inside the scroll frame: counts as its window
        DisplayListItem::PopScrollFrame,     // 10
        DisplayListItem::PopReferenceFrame,  // 11: closes 1
        DisplayListItem::PopClip,            // 12
    ];
    let list = list(items);
    for i in [1usize, 4] {
        assert_eq!(
            reference_frame_extent(&list, i),
            reference_frame_extent_walked(&list.items, i),
            "frame {i}"
        );
    }
    let outer = reference_frame_extent(&list, 1).expect("an extent");
    // Frame 1's walk starts after its push: the clip at 0 is not one of its own, so the rect at 2
    // paints in full, and the scroll frame counts as its window.
    assert!(outer.size.width >= 300.0 && outer.size.height >= 300.0, "{outer:?}");
    let inner = reference_frame_extent(&list, 4).expect("an extent");
    assert_eq!(
        (inner.origin.x, inner.origin.y, inner.size.width, inner.size.height),
        (0.0, 0.0, 100.0, 100.0),
        "frame 4: its box and its rect, which the clip it opened after its own start does not cut: {inner:?}"
    );
}

/// How many items the asks of a list read: the index reads each once, however many frames there
/// are, however often it is asked, however deep they nest.
fn items_read_asking_twice(list: &DisplayList) -> u64 {
    cpu_counters::reset();
    for _ in 0..2 {
        for i in 0..list.items.len() {
            let _ = reference_frame_extent(list, i);
        }
    }
    cpu_counters::take().scan_items
}

#[test]
fn a_list_is_walked_once_however_often_its_frames_are_asked_about() {
    let siblings = |n: usize| {
        let mut items = Vec::new();
        for k in 0..n {
            items.push(frame(k, 0.0, k as f32, 10.0, 10.0));
            items.push(rect(0.0, k as f32, 5.0, 5.0));
            items.push(DisplayListItem::PopReferenceFrame);
        }
        list(items)
    };
    let nested = |n: usize| {
        let mut items = Vec::new();
        for k in 0..n {
            items.push(frame(k, 0.0, k as f32, 10.0, 10.0));
        }
        items.push(rect(0.0, 0.0, 5.0, 5.0));
        for _ in 0..n {
            items.push(DisplayListItem::PopReferenceFrame);
        }
        list(items)
    };
    for (name, make) in [("siblings", &siblings as &dyn Fn(usize) -> DisplayList), ("nested", &nested)] {
        let (small, large) = (make(100), make(400));
        let (read_small, read_large) = (items_read_asking_twice(&small), items_read_asking_twice(&large));
        assert_eq!(read_small, small.items.len() as u64, "{name}: each item once, not once per ask");
        assert_eq!(read_large, large.items.len() as u64, "{name}: each item once, not once per ask");
        assert!(read_large < 6 * read_small, "{name}: reads grow with the list: {read_small} then {read_large}");
    }
    // Walked from each push to its pop, the nested list is read quadratically: the sweep above
    // would have failed on it.
    let deep = nested(200);
    cpu_counters::reset();
    for i in 0..deep.items.len() {
        let _ = reference_frame_extent_walked(&deep.items, i);
    }
    let walked = cpu_counters::take().scan_items;
    assert!(walked > 10 * deep.items.len() as u64, "the walk reads {walked} items");
}

#[test]
fn a_list_that_changed_is_not_answered_from_the_index_of_what_it_was() {
    let mut l = list(vec![
        frame(1, 0.0, 0.0, 10.0, 10.0),
        rect(0.0, 0.0, 20.0, 20.0),
        DisplayListItem::PopReferenceFrame,
    ]);
    let before = reference_frame_extent(&l, 0).expect("an extent");
    assert_eq!((before.size.width, before.size.height), (20.0, 20.0));
    // A copy has its own index: changing it changes nothing of the original's answers.
    let mut copy = l.clone();
    copy.items.insert(2, rect(0.0, 0.0, 90.0, 90.0));
    assert_eq!(reference_frame_extent(&copy, 0), reference_frame_extent_walked(&copy.items, 0));
    assert_eq!(reference_frame_extent(&l, 0), Some(before));
    // `insert_item` drops the index; so does an item pushed behind the list's back (the length).
    l.insert_item(2, rect(0.0, 0.0, 60.0, 60.0), None);
    let after = reference_frame_extent(&l, 0).expect("an extent");
    assert_eq!((after.size.width, after.size.height), (60.0, 60.0));
    assert_eq!(Some(after), reference_frame_extent_walked(&l.items, 0));
    l.items.insert(1, rect(0.0, 0.0, 70.0, 70.0));
    let pushed = reference_frame_extent(&l, 0).expect("an extent");
    assert_eq!((pushed.size.width, pushed.size.height), (70.0, 70.0));
    assert_eq!(l.frame_index().end_of(0), Some(pop_of(&l.items, 0)));
    // `splice_item` swaps one item for another and leaves the length alone: only dropping the
    // index tells.
    l.splice_item(1, vec![(rect(0.0, 0.0, 95.0, 95.0), None)]);
    let spliced = reference_frame_extent(&l, 0).expect("an extent");
    assert_eq!((spliced.size.width, spliced.size.height), (95.0, 95.0));
    assert_eq!(Some(spliced), reference_frame_extent_walked(&l.items, 0));
}

/// What the damage scans look for in a list, found by the index: the views, the opaque fills, the
/// keys the list binds.
#[test]
fn the_index_finds_the_views_the_opaque_fills_and_the_bound_keys_of_a_list() {
    use azul_core::{dom::DomId, resources::OpacityKey};
    use std::collections::HashSet;

    let translucent = DisplayListItem::Rect {
        bounds: wlr(0.0, 0.0, 9.0, 9.0),
        color: ColorU { r: 1, g: 2, b: 3, a: 200 },
        border_radius: BorderRadius::default(),
    };
    let rounded = DisplayListItem::Rect {
        bounds: wlr(0.0, 0.0, 8.0, 8.0),
        color: ColorU { r: 1, g: 2, b: 3, a: 255 },
        border_radius: BorderRadius { top_left: 4.0, ..BorderRadius::default() },
    };
    let view = |id: usize| DisplayListItem::VirtualView {
        child_dom_id: DomId { inner: id },
        bounds: wlr(0.0, 0.0, 50.0, 50.0),
        clip_rect: wlr(0.0, 0.0, 50.0, 50.0),
        content_offset: LogicalPosition::new(0.0, 0.0),
    };
    let l = list(vec![
        rect(1.0, 1.0, 10.0, 10.0),
        translucent,
        view(3),
        frame(41, 0.0, 0.0, 5.0, 5.0),
        rounded,
        rect(2.0, 2.0, 20.0, 20.0),
        DisplayListItem::PopReferenceFrame,
        DisplayListItem::PushOpacity {
            bounds: wlr(0.0, 0.0, 5.0, 5.0),
            opacity: 0.5,
            opacity_key: Some(OpacityKey { id: 52 }),
            mix_blend_mode: Default::default(),
        },
        view(4),
        DisplayListItem::PopOpacity,
    ]);
    let index = l.frame_index();
    assert_eq!(index.views(), &[2, 8], "the VirtualView items");
    let fills: Vec<(f32, f32)> = index.opaque_fills().iter().map(|r| (r.size.width, r.size.height)).collect();
    assert_eq!(fills, [(10.0, 10.0), (20.0, 20.0)], "opaque, square-cornered rects only");
    let set = |k: usize| HashSet::from([k]);
    assert!(index.binds_a_transform_key(&set(41)) && !index.binds_a_transform_key(&set(42)));
    assert!(index.binds_an_opacity_key(&set(52)) && !index.binds_an_opacity_key(&set(41)));
    assert!(!index.has_color_keys() && !index.binds_a_color_key(41));
}
