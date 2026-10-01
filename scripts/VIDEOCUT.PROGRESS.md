# VIDEOCUT progress (branch wt/videocut from 16d19442c)

Brief: scratchpad VIDEOCUT_go.md; house rules wave4_common.md (no cargo, no LSP, RED first,
commit every unit, keep this file exact). Parse check: scratchpad vc_parse.sh <files>.
Commit messages via scratchpad vc_msg.txt (Write tool, then `git -C <wt> commit -F`).

## DONE
- 8b34d1c62 RED / e3d33ba36 GREEN: Mp4Demuxer + Mp4Muxer (dll video_codec/container.rs), VideoChunk
  in core; annexb_nals + append_avcc_as_annexb unified (VT + demux import them).
- beb8ad0ef RED / 6ec784a88 GREEN: VideoEncoder::encode_at (own timestamps for export).
- c66eb59cd RED: timeline widget tests (layout/src/widgets/timeline_tests.rs), `pub mod timeline;`
  appended in widgets/mod.rs, timeline.rs = doc + test include only.

## DECISIONS
- MP4 container I/O lives in azul (NO DUPLICATION): video_codec/container.rs (Mp4Demuxer /
  Mp4Muxer over the `mp4` crate already in Cargo.lock, behind `video-native`).
- No encoder on the platform: export falls back to Y4M (uncompressed, plays in ffmpeg / VLC).
- Sample clips: synthetic generator media ("color bars" / "color matte") always available; with
  an open VideoEncoder `--sample` also encodes two short MP4 clips into the project's media/.
- Data root: `AZVIDEOCUT_DATA` or `<FilePath::get_data_dir()>/azul`, keys `videocut/<uuid>/...`.
- Timeline widget API (what the tests use): Timeline::create(tracks, duration), with_playhead,
  with_view(view_start, pps), with_view_width, with_fps, with_snapping, with_on_event(RefAny,
  TimelineOnEventCallbackType), with_theme, dom, Timeline::format_timecode(secs, fps);
  TimelineTrack::create(id, name, kind).with_clips(..) {locked, muted, height}; TimelineClip::create
  (id, start, duration, label).with_selected/with_tint/with_detail/with_thumbnail/with_disabled;
  TimelineEvent {time, value, clip_id, track, kind, edge, shift, ctrl}; kinds Seek, Scroll, Zoom,
  ZoomToFit, Select, Open, Move, Trim, LaneClick, Delete, ToggleMute, ToggleLock; pure fns
  visible_window, clip_geometry, thumb_span, tick_step, MIN_TICK_PX, snap_time, snap_points,
  drag_result(TimelineDragMode::{Move,TrimStart,TrimEnd}), edit_points; classes TIMELINE_CLASS,
  HEAD_CLASS, BODY_CLASS, SCROLL_CLASS, HEADER_CLASS, LANE_CLASS, LANES_CLASS, RULER_CLASS,
  PLAYHEAD_CLASS, THUMB_CLASS, CLIP_CLASS, CLIP_SELECTED_CLASS, MUTE_CLASS, LOCK_CLASS.

## IN PROGRESS
- GREEN timeline: write layout/src/widgets/timeline.rs in pieces (types -> pure fns -> callbacks
  -> build), then the looks appended to themes/flat.rs + flora.rs (`// ==== timeline ====`), then
  the manifest entry (label_convention::every_widget_dom, append) + "timeline" in CHROME list.

## NEXT
1. timeline GREEN (above).
2. examples/azul-videocut: model (edits + undo), render (composite), decode cache worker,
   export worker, storage, UI on TimelineShell, args, unit tests (RED model tests first).
3. scripts/azvideocut_e2e.py; workspace registration (Cargo.toml members, workspace_test_members,
   rust.yml dll_tests step); report scripts/VIDEOCUT_2026_10_01.md.

## OPEN QUESTIONS
- none
