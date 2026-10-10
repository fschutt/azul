//! Reading frames out of an MP4: seek to the keyframe, decode forward, and
//! go on from where the decoder already is when the frame lies ahead.

use super::*;

#[test]
fn a_seek_decodes_from_the_keyframe_unless_the_decoder_is_already_on_the_way() {
    // A fresh decoder starts at the keyframe.
    assert_eq!(feed_plan(None, 30, 37), 30..38);
    // Playing on: the decoder last took 37, so 38 needs 38 alone.
    assert_eq!(feed_plan(Some(38), 30, 38), 38..39);
    // A few frames ahead in the same group of pictures: no restart.
    assert_eq!(feed_plan(Some(38), 30, 41), 38..42);
    // Behind the decoder: back to the keyframe.
    assert_eq!(feed_plan(Some(38), 30, 33), 30..34);
    // In another group of pictures: from its keyframe.
    assert_eq!(feed_plan(Some(38), 60, 64), 60..65);
}

#[test]
fn a_media_time_maps_to_its_frame_and_back() {
    assert_eq!(media_ms(25, 25), 1000.0);
    assert_eq!(media_ms(0, 30), 0.0);
    let ms = media_ms(1, 30);
    assert!((ms - 33.333).abs() < 0.01, "{ms}");
}

#[test]
fn the_frame_cache_keeps_the_last_few_frames_and_finds_them_by_index() {
    let mut cache = FrameCache::with_capacity(3);
    for i in 0..5usize {
        cache.put(i, crate::render::Canvas::black(2, 2));
    }
    assert!(cache.get(0).is_none() && cache.get(1).is_none(), "the oldest went");
    assert!(cache.get(2).is_some() && cache.get(4).is_some());
    assert_eq!(cache.len(), 3);
}
