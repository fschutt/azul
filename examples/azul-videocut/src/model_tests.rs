//! The edit model: a project of media items and one sequence of tracks of
//! clips; every edit is a command that undoes and redoes.

use super::*;

/// 25 fps, 640 x 360; media "pier" 100 frames, "market" 50 frames.
fn project() -> (Project, u64, u64) {
    let mut p = Project::create("p-1".into(), "Teaser".into(), 640, 360, 25);
    let pier = p.add_media(MediaItem::generated("pier", Pattern::Bars, 100, 640, 360));
    let market = p.add_media(MediaItem::generated(
        "market",
        Pattern::Matte { rgb: [200, 80, 40] },
        50,
        640,
        360,
    ));
    (p, pier, market)
}

/// The project with V1 = pier 0..40 (source 10..50), market 40..70.
fn cut() -> (Project, u64, u64) {
    let (mut p, pier, market) = project();
    let a = p.clip_from_media(pier, 10, 40);
    let b = p.clip_from_media(market, 0, 30);
    let (ida, idb) = (a.id, b.id);
    p.edit(Edit::Overwrite { track: 0, at: 0, clip: a }).expect("overwrite pier");
    p.edit(Edit::Overwrite { track: 0, at: 40, clip: b }).expect("overwrite market");
    (p, ida, idb)
}

/// (start, length, source_in, media) of every clip of `track`, in order.
fn spans(p: &Project, track: usize) -> Vec<(Frame, Frame, Frame, u64)> {
    p.sequence.tracks[track]
        .clips
        .iter()
        .map(|c| (c.start, c.length, c.source_in, c.media))
        .collect()
}

#[test]
fn a_new_project_has_three_video_tracks_over_three_audio_tracks() {
    let (p, _, _) = project();
    let names: Vec<&str> = p.sequence.tracks.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(names, vec!["V1", "V2", "V3", "A1", "A2", "A3"]);
    assert_eq!(p.sequence.tracks[2].kind, TrackKind::Video);
    assert_eq!(p.sequence.tracks[3].kind, TrackKind::Audio);
    assert_eq!(p.sequence.end(), 0, "an empty sequence has no length");
}

#[test]
fn overwriting_replaces_what_lies_under_the_new_clip_and_moves_nothing_else() {
    let (mut p, ida, idb) = cut();
    let (_, pier, market) = project();
    let _ = (pier, market);
    assert_eq!(spans(&p, 0)[0].0, 0);
    // 10 frames of market over 35..45: pier ends at 35, market starts at 45.
    let media = p.sequence.clip(idb).expect("market").media;
    let c = p.clip_from_media(media, 0, 10);
    p.edit(Edit::Overwrite { track: 0, at: 35, clip: c }).expect("overwrite");
    let s = spans(&p, 0);
    assert_eq!(s.len(), 3);
    assert_eq!((s[0].0, s[0].1), (0, 35), "pier cut at 35");
    assert_eq!((s[1].0, s[1].1), (35, 10), "the new clip");
    assert_eq!((s[2].0, s[2].1, s[2].2), (45, 25, 5), "market's head trimmed by 5 frames");
    assert_eq!(p.sequence.end(), 70, "nothing moved");
    assert!(p.sequence.clip(ida).is_some() && p.sequence.clip(idb).is_some());
}

#[test]
fn inserting_pushes_the_clips_after_it_right_and_splits_the_one_under_the_insert_point() {
    let (mut p, _, idb) = cut();
    let media = p.sequence.clip(idb).expect("market").media;
    let c = p.clip_from_media(media, 0, 10);
    p.edit(Edit::Insert { track: 0, at: 20, clip: c }).expect("insert");
    let s = spans(&p, 0);
    assert_eq!(s.len(), 4);
    assert_eq!((s[0].0, s[0].1, s[0].2), (0, 20, 10), "pier's first half");
    assert_eq!((s[1].0, s[1].1), (20, 10), "the inserted clip");
    assert_eq!((s[2].0, s[2].1, s[2].2), (30, 20, 30), "pier's second half plays on");
    assert_eq!(s[3].0, 50, "market pushed right by 10");
    assert_eq!(p.sequence.end(), 80);
}

#[test]
fn the_razor_splits_a_clip_in_two_that_play_the_same_frames() {
    let (mut p, ida, _) = cut();
    let before: Vec<Frame> = (0..40).map(|f| p.sequence.source_frame_at(0, f).expect("a frame").1).collect();
    p.edit(Edit::Razor { track: Some(0), at: 15 }).expect("razor");
    let s = spans(&p, 0);
    assert_eq!(s.len(), 3);
    assert_eq!((s[0].0, s[0].1), (0, 15));
    assert_eq!((s[1].0, s[1].1, s[1].2), (15, 25, 25));
    assert_eq!(p.sequence.tracks[0].clips[0].id, ida, "the left half keeps the id");
    assert_ne!(p.sequence.tracks[0].clips[1].id, ida, "the right half is a new clip");
    let after: Vec<Frame> = (0..40).map(|f| p.sequence.source_frame_at(0, f).expect("a frame").1).collect();
    assert_eq!(before, after, "the cut plays exactly the frames it played");
    assert_eq!(
        p.edit(Edit::Razor { track: Some(0), at: 15 }),
        Err(EditError::NothingToDo),
        "a cut on a cut changes nothing"
    );
}

#[test]
fn the_razor_on_all_tracks_splits_every_unlocked_clip_under_the_playhead() {
    let (mut p, _, idb) = cut();
    let media = p.sequence.clip(idb).expect("market").media;
    let over = p.clip_from_media(media, 0, 30);
    p.edit(Edit::Overwrite { track: 1, at: 10, clip: over }).expect("V2");
    let locked = p.clip_from_media(media, 0, 30);
    p.edit(Edit::Overwrite { track: 2, at: 10, clip: locked }).expect("V3");
    p.edit(Edit::SetTrackLocked { track: 2, locked: true }).expect("lock V3");
    p.edit(Edit::Razor { track: None, at: 20 }).expect("razor all");
    assert_eq!(p.sequence.tracks[0].clips.len(), 3);
    assert_eq!(p.sequence.tracks[1].clips.len(), 2);
    assert_eq!(p.sequence.tracks[2].clips.len(), 1, "a locked track is not cut");
}

#[test]
fn a_ripple_delete_closes_the_gap_and_a_lift_leaves_it() {
    let (mut p, ida, idb) = cut();
    p.edit(Edit::Lift { clip: ida }).expect("lift");
    assert_eq!(spans(&p, 0)[0].0, 40, "market stays where it was");
    p.undo();
    p.edit(Edit::RippleDelete { clip: ida }).expect("ripple delete");
    let s = spans(&p, 0);
    assert_eq!(s.len(), 1);
    assert_eq!(s[0].0, 0, "market moved left into the gap");
    assert_eq!(p.sequence.end(), 30);
    assert_eq!(p.sequence.clip(idb).map(|c| c.start), Some(0));
}

#[test]
fn moving_a_clip_overwrites_its_destination_and_may_change_track_but_not_kind() {
    let (mut p, ida, idb) = cut();
    p.edit(Edit::Move { clip: idb, track: 1, start: 30 }).expect("move to V2");
    assert!(p.sequence.tracks[0].clips.iter().all(|c| c.id != idb));
    assert_eq!(spans(&p, 1)[0].0, 30);
    // Back onto V1 over the end of pier: pier is cut at 30.
    p.edit(Edit::Move { clip: idb, track: 0, start: 30 }).expect("move back");
    assert_eq!(p.sequence.clip(ida).map(|c| c.length), Some(30));
    assert_eq!(
        p.edit(Edit::Move { clip: idb, track: 3, start: 0 }),
        Err(EditError::WrongTrackKind),
        "a picture does not go on an audio track"
    );
}

#[test]
fn trimming_never_reaches_past_the_media_or_into_a_neighbour() {
    let (mut p, ida, idb) = cut();
    // Pier's out point cannot run into market at 40.
    p.edit(Edit::Trim { clip: ida, edge: Edge::End, at: 60 }).expect("trim end");
    assert_eq!(p.sequence.clip(ida).map(|c| c.end()), Some(40));
    // Its in point can open up to the media's first frame (10 frames of handle).
    p.edit(Edit::Lift { clip: idb }).expect("lift market");
    p.edit(Edit::Move { clip: ida, track: 0, start: 20 }).expect("move pier");
    p.edit(Edit::Trim { clip: ida, edge: Edge::Start, at: 0 }).expect("trim start");
    let c = p.sequence.clip(ida).expect("pier").clone();
    assert_eq!((c.start, c.source_in, c.length), (10, 0, 50), "stops at the media's first frame");
    // And its out point at the media's last (100 frames).
    p.edit(Edit::Trim { clip: ida, edge: Edge::End, at: 500 }).expect("trim end");
    let c = p.sequence.clip(ida).expect("pier").clone();
    assert_eq!(c.source_in + c.length, 100);
    // A trim never leaves less than a frame.
    p.edit(Edit::Trim { clip: ida, edge: Edge::End, at: -10 }).expect("trim to nothing");
    assert_eq!(p.sequence.clip(ida).map(|c| c.length), Some(1));
}

#[test]
fn a_ripple_trim_moves_the_clips_after_the_edit() {
    let (mut p, ida, idb) = cut();
    p.edit(Edit::RippleTrim { clip: ida, edge: Edge::End, at: 30 }).expect("ripple trim");
    assert_eq!(p.sequence.clip(ida).map(|c| c.length), Some(30));
    assert_eq!(p.sequence.clip(idb).map(|c| c.start), Some(30), "market follows");
    p.edit(Edit::RippleTrim { clip: ida, edge: Edge::End, at: 45 }).expect("ripple longer");
    assert_eq!(p.sequence.clip(idb).map(|c| c.start), Some(45), "and pushes on extension");
}

#[test]
fn slipping_changes_the_source_frames_not_the_position() {
    let (mut p, ida, _) = cut();
    p.edit(Edit::Slip { clip: ida, delta: 5 }).expect("slip");
    let c = p.sequence.clip(ida).expect("pier").clone();
    assert_eq!((c.start, c.length, c.source_in), (0, 40, 15));
    p.edit(Edit::Slip { clip: ida, delta: 500 }).expect("slip far");
    assert_eq!(p.sequence.clip(ida).map(|c| c.source_in), Some(60), "stops at the media's end");
    p.edit(Edit::Slip { clip: ida, delta: -500 }).expect("slip back");
    assert_eq!(p.sequence.clip(ida).map(|c| c.source_in), Some(0));
}

#[test]
fn every_edit_undoes_and_redoes_and_a_new_edit_clears_the_redo() {
    let (mut p, ida, _) = cut();
    let before = p.sequence.clone();
    p.edit(Edit::Razor { track: Some(0), at: 10 }).expect("razor");
    p.edit(Edit::Slip { clip: ida, delta: 3 }).expect("slip");
    let after = p.sequence.clone();
    assert_eq!(p.undo_label(), Some("Slip"));
    assert!(p.undo() && p.undo());
    assert_eq!(p.sequence, before);
    assert!(p.redo() && p.redo());
    assert_eq!(p.sequence, after);
    assert!(!p.redo(), "nothing left to redo");
    p.undo();
    p.edit(Edit::Lift { clip: ida }).expect("lift");
    assert!(!p.can_redo(), "a new edit clears the redo");
}

#[test]
fn a_locked_track_refuses_edits() {
    let (mut p, ida, _) = cut();
    p.edit(Edit::SetTrackLocked { track: 0, locked: true }).expect("lock");
    assert_eq!(p.edit(Edit::Lift { clip: ida }), Err(EditError::TrackLocked));
    assert_eq!(p.edit(Edit::Razor { track: Some(0), at: 5 }), Err(EditError::TrackLocked));
    assert_eq!(p.sequence.tracks[0].clips.len(), 2);
}

#[test]
fn the_frame_shown_at_a_time_comes_from_the_clip_under_it() {
    let (p, _, idb) = cut();
    let market = p.sequence.clip(idb).expect("market").media;
    assert_eq!(p.sequence.source_frame_at(0, 45), Some((market, 5)));
    assert_eq!(p.sequence.source_frame_at(0, 5).map(|f| f.1), Some(15), "pier from its frame 10");
    assert_eq!(p.sequence.source_frame_at(0, 70), None, "past the end");
    assert_eq!(p.sequence.end(), 70);
}

#[test]
fn an_insert_from_the_source_monitor_takes_the_marked_range() {
    let (mut p, pier, _) = project();
    let marks = SourceMarks { media: pier, mark_in: Some(20), mark_out: Some(29), position: 0 };
    let clip = p.clip_from_marks(&marks).expect("a range");
    assert_eq!((clip.source_in, clip.length), (20, 10), "in and out are both played");
    let all = SourceMarks { media: pier, mark_in: None, mark_out: None, position: 0 };
    let clip = p.clip_from_marks(&all).expect("the whole media");
    assert_eq!((clip.source_in, clip.length), (0, 100));
    let empty = SourceMarks { media: pier, mark_in: Some(50), mark_out: Some(40), position: 0 };
    assert!(p.clip_from_marks(&empty).is_none(), "an out before the in marks nothing");
}

#[test]
fn a_project_round_trips_through_its_json() {
    let (mut p, ida, _) = cut();
    p.edit(Edit::SetEffects {
        clip: ida,
        effects: Effects { scale: 0.5, opacity: 0.75, crop_left: 0.1, ..Effects::default() },
    })
    .expect("effects");
    p.edit(Edit::SetTransition {
        clip: ida,
        transition: Some(Transition { kind: TransitionKind::DipToBlack, frames: 12 }),
    })
    .expect("transition");
    let json = p.to_json();
    let back = Project::from_json(&json).expect("parses");
    assert_eq!(back.sequence, p.sequence);
    assert_eq!(back.media, p.media);
    assert_eq!(back.next_id, p.next_id);
    assert!(!back.can_undo(), "the history is not saved");
}
