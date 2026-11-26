//! The renderer's plan, as text: which slices of which files feed each cut, what the filter graph says,
//! what ffmpeg is called with, how the pieces are put together and the chapters written. No ffmpeg is
//! run here (the end-to-end test at the bottom runs a real one when it finds one).

use std::path::PathBuf;

use super::*;
use crate::recorder_edl::{identity_edl, AudioCut, Edl, EdlSegment, Zoom};
use crate::recorder_manifest::{AppInfo, ClockInfo, HostInfo, Manifest, Run, Segment, Track, TrackStatus, Workspace};

const SEC: u64 = 1_000_000;

fn track(video: bool, w: u32, h: u32, segments: Vec<Segment>) -> Track {
    Track {
        source: String::from("mock"),
        device: None,
        codec: String::from(if video { "h264" } else { "opus" }),
        encoder: String::from("mock"),
        hardware: false,
        width: w,
        height: h,
        scale: 2.0,
        fps: 10,
        vfr: video,
        bitrate_kbps: 100,
        sample_rate: if video { 0 } else { 48_000 },
        channels: if video { 0 } else { 1 },
        timescale: if video { 90_000 } else { 48_000 },
        status: TrackStatus::Stopped,
        note: None,
        segments,
    }
}

fn seg(file: &str, start: u64, end: u64) -> Segment {
    Segment { file: file.into(), start_us: start * SEC, end_us: Some(end * SEC), bytes: 1_000, samples: 10, complete: true, recovered: false, width: 0, height: 0 }
}

/// A session of 0 to 100 s with the screen and the voice in two segments each (cut at 60 s) and a camera
/// that began at 5 s; then a second run, 200 to 300 s.
fn session() -> Manifest {
    let mut m = Manifest::begin(
        AppInfo { name: String::from("AzReview"), version: String::from("0.1.0") },
        "2025-11-25_22-00-00",
        Workspace { root: String::from("/w"), name: String::from("azul") },
        ClockInfo { unit: String::from("us"), origin_unix_ms: 0, origin_iso8601: String::from("1970-01-01T00:00:00Z"), utc_offset_minutes: 0 },
        HostInfo { os: String::from("macos"), arch: String::from("aarch64") },
    );
    m.tracks.screen = Some(track(true, 2880, 1800, vec![seg("screen/0001.mp4", 0, 60), seg("screen/0002.mp4", 60, 100), seg("screen/0003.mp4", 200, 300)]));
    m.tracks.camera = Some(track(true, 1280, 720, vec![seg("camera/0001.mp4", 5, 100), seg("camera/0002.mp4", 200, 300)]));
    m.tracks.audio = Some(track(false, 0, 0, vec![seg("audio/0001.mp4", 0, 60), seg("audio/0002.mp4", 60, 100), seg("audio/0003.mp4", 200, 300)]));
    m.runs = vec![
        Run { start_us: 0, end_us: Some(100 * SEC), end_reason: Some(String::from("pause")) },
        Run { start_us: 200 * SEC, end_us: Some(300 * SEC), end_reason: Some(String::from("stop")) },
    ];
    m
}

fn segment(id: &str, in_s: u64, out_s: u64) -> EdlSegment {
    EdlSegment { id: id.into(), in_us: in_s * SEC, out_us: out_s * SEC, camera: true, speed: 1.0, zoom: None, audio: AudioCut::default(), caption: None, title: None }
}

fn edl(segments: Vec<EdlSegment>) -> Edl {
    let mut e = identity_edl(&session(), &[]);
    e.segments = segments;
    e.chapters = Vec::new();
    e.title = None;
    e.output.width = 1280;
    e.output.height = 720;
    e
}

static ENC: Capabilities = Capabilities { videotoolbox: true, libx264: true, aac: true, rounding: true };

fn plan_of(e: &Edl) -> Plan {
    build_plan(&session(), e, Path::new("/rec/s"), Path::new("/work"), Path::new("/out/video.mp4"), ENC).unwrap()
}

fn args_of(c: &Command) -> String {
    c.args.join(" ")
}

fn filter_of(c: &Command) -> String {
    let at = c.args.iter().position(|a| a == "-filter_complex").expect("a filter graph");
    c.args[at + 1].clone()
}

/// The same session without the camera and without the second run: the screen and the voice in two
/// files each, 0 to 100 s.
fn screen_alone() -> Manifest {
    let mut m = session();
    m.tracks.camera = None;
    m.runs.truncate(1);
    m.tracks.screen.as_mut().unwrap().segments.truncate(2);
    m.tracks.audio.as_mut().unwrap().segments.truncate(2);
    m
}

fn plan_for(m: &Manifest, e: &Edl, enc: Capabilities) -> Result<Plan, String> {
    build_plan(m, e, Path::new("/rec/s"), Path::new("/work"), Path::new("/out/video.mp4"), enc)
}

// ---- which slices of which files ----

#[test]
fn a_range_inside_one_segment_is_one_slice_with_its_seek() {
    let m = session();
    let s = slices_for(m.tracks.screen.as_ref().unwrap(), Path::new("/rec/s"), 10 * SEC, 30 * SEC);
    assert_eq!(s, vec![Slice { file: PathBuf::from("/rec/s/screen/0001.mp4"), seek_us: 10 * SEC, dur_us: 20 * SEC, lead_us: 0 }]);
}

#[test]
fn a_range_over_two_segments_is_two_slices_in_order() {
    let m = session();
    let s = slices_for(m.tracks.screen.as_ref().unwrap(), Path::new("/rec/s"), 50 * SEC, 80 * SEC);
    assert_eq!(
        s,
        vec![
            Slice { file: PathBuf::from("/rec/s/screen/0001.mp4"), seek_us: 50 * SEC, dur_us: 10 * SEC, lead_us: 0 },
            Slice { file: PathBuf::from("/rec/s/screen/0002.mp4"), seek_us: 0, dur_us: 20 * SEC, lead_us: 10 * SEC },
        ]
    );
}

#[test]
fn a_track_that_began_late_leads_in_with_the_time_it_was_not_there() {
    let m = session();
    let s = slices_for(m.tracks.camera.as_ref().unwrap(), Path::new("/rec/s"), 0, 20 * SEC);
    assert_eq!(s, vec![Slice { file: PathBuf::from("/rec/s/camera/0001.mp4"), seek_us: 0, dur_us: 15 * SEC, lead_us: 5 * SEC }]);
}

#[test]
fn a_range_the_track_does_not_cover_has_no_slices_and_a_segment_without_an_end_is_skipped() {
    let m = session();
    assert!(slices_for(m.tracks.screen.as_ref().unwrap(), Path::new("/r"), 120 * SEC, 180 * SEC).is_empty());
    let mut open = track(true, 10, 10, vec![seg("screen/0001.mp4", 0, 10)]);
    open.segments.push(Segment { end_us: None, complete: false, ..seg("screen/0002.mp4", 10, 20) });
    assert_eq!(slices_for(&open, Path::new("/r"), 0, 100 * SEC).len(), 1, "a segment still being written has no end to cut at");
}

#[test]
fn seconds_are_written_with_microsecond_precision_and_no_exponent() {
    assert_eq!(secs(0), "0.000000");
    assert_eq!(secs(1_500_000), "1.500000");
    assert_eq!(secs(12_345_678), "12.345678");
    assert_eq!(secs(3_600 * SEC), "3600.000000");
}

// ---- one cut ----

#[test]
fn a_cut_is_one_ffmpeg_command_that_seeks_each_input_and_names_its_file() {
    let p = plan_of(&edl(vec![segment("a", 10, 30)]));
    assert_eq!(p.commands.len(), 2, "the cut and the putting together");
    let c = &p.commands[0];
    assert_eq!(c.id, "a");
    assert_eq!(c.output, PathBuf::from("/work/seg_a.mp4"));
    let a = args_of(c);
    assert!(a.starts_with("-hide_banner -nostdin -y"), "{a}");
    // The background, the screen (one slice), the camera (one slice, from 10 s into its file at 5 s
    // - so it has been recording since before the cut: seek 5 s), the voice.
    assert!(a.contains("-f lavfi -i color=c=0x101010:s=1280x720:r=30:d=20.000000"), "{a}");
    assert!(a.contains("-ss 10.000000 -t 20.000000 -i /rec/s/screen/0001.mp4"), "{a}");
    assert!(a.contains("-ss 5.000000 -t 20.000000 -i /rec/s/camera/0001.mp4"), "{a}");
    assert!(a.contains("-ss 10.000000 -t 20.000000 -i /rec/s/audio/0001.mp4"), "{a}");
    assert!(a.ends_with("/work/seg_a.mp4"), "{a}");
    assert!(a.contains("-t 20.000000"));
    assert!(a.contains("-map [v] -map [a]"), "{a}");
}

#[test]
fn the_picture_is_the_screen_scaled_into_the_frame_over_a_background_with_the_camera_in_its_corner() {
    let p = plan_of(&edl(vec![segment("a", 10, 30)]));
    let f = filter_of(&p.commands[0]);
    assert!(f.contains("[1:v]setpts=PTS-STARTPTS,scale=1280:720:force_original_aspect_ratio=decrease,setsar=1,fps=30,tpad=stop_mode=clone:stop_duration=20.000000[screen]"), "{f}");
    assert!(f.contains("[0:v][screen]overlay=x=(W-w)/2:y=(H-h)/2:eof_action=pass[b1]"), "{f}");
    // The camera at 20% of 1280 = 256 px wide; 24 px from the corner.
    assert!(f.contains("[2:v]setpts=PTS-STARTPTS,scale=256:-2,fps=30,tpad=stop_mode=clone:stop_duration=20.000000[cam]"), "{f}");
    assert!(f.contains("[b1][cam]overlay=x=W-w-24:y=H-h-24:eof_action=pass[v]"), "{f}");
    assert!(f.contains("[3:a]aresample=48000,volume=0.000dB[a]"), "{f}");
}

#[test]
fn the_camera_goes_to_the_corner_the_cut_list_names() {
    for (corner, at) in [("top_left", "x=24:y=24"), ("top_right", "x=W-w-24:y=24"), ("bottom_left", "x=24:y=H-h-24"), ("bottom_right", "x=W-w-24:y=H-h-24")] {
        let mut e = edl(vec![segment("a", 10, 30)]);
        e.style.camera.corner = corner.into();
        let f = filter_of(&plan_of(&e).commands[0]);
        assert!(f.contains(&format!("overlay={at}:eof_action=pass[v]")), "{corner}: {f}");
    }
}

#[test]
fn a_cut_without_the_camera_has_no_second_overlay() {
    let mut s = segment("a", 10, 30);
    s.camera = false;
    let p = plan_of(&edl(vec![s]));
    let (a, f) = (args_of(&p.commands[0]), filter_of(&p.commands[0]));
    assert!(!a.contains("camera/"), "{a}");
    assert!(f.contains("[0:v][screen]overlay=x=(W-w)/2:y=(H-h)/2:eof_action=pass[v]"), "{f}");
    assert!(!f.contains("[cam]"));
}

#[test]
fn a_camera_that_was_not_there_yet_leads_in_with_the_background_showing() {
    let f = filter_of(&plan_of(&edl(vec![segment("a", 0, 20)])).commands[0]);
    assert!(f.contains("[2:v]setpts=PTS-STARTPTS+5.000000/TB,scale=256:-2"), "the camera comes in at 5 s: {f}");
}

#[test]
fn a_cut_over_two_files_joins_them_before_it_is_scaled() {
    let p = plan_of(&edl(vec![segment("a", 50, 80)]));
    let (a, f) = (args_of(&p.commands[0]), filter_of(&p.commands[0]));
    assert!(a.contains("-ss 50.000000 -t 10.000000 -i /rec/s/screen/0001.mp4"), "{a}");
    assert!(a.contains("-ss 0.000000 -t 20.000000 -i /rec/s/screen/0002.mp4"), "{a}");
    assert!(f.contains("[1:v][2:v]concat=n=2:v=1:a=0[screen_raw]"), "{f}");
    assert!(f.contains("[screen_raw]setpts=PTS-STARTPTS,scale="), "{f}");
    assert!(f.contains("[camera_raw]") || f.contains("[3:v]") || f.contains("[4:v]"), "{f}");
}

#[test]
fn a_zoom_crops_the_screen_to_the_rectangle_before_it_is_scaled() {
    let mut s = segment("a", 10, 30);
    s.zoom = Some(Zoom { x: 100, y: 200, w: 1440, h: 900, ease_ms: 400 });
    let f = filter_of(&plan_of(&edl(vec![s])).commands[0]);
    assert!(f.contains("[1:v]setpts=PTS-STARTPTS,crop=1440:900:100:200,scale=1280:720"), "{f}");
}

#[test]
fn the_speed_changes_the_picture_and_the_sound_together() {
    let mut s = segment("a", 10, 30);
    s.speed = 2.0;
    let p = plan_of(&edl(vec![s]));
    let (a, f) = (args_of(&p.commands[0]), filter_of(&p.commands[0]));
    assert!(f.contains("setpts=PTS/2.000000[v]"), "{f}");
    assert!(f.contains("atempo=2.000000[a]"), "{f}");
    assert!(a.contains("-t 10.000000 -c:v"), "the cut is 20 s of the recording and 10 s of the video: {a}");
    // atempo takes 0.5 to 2 at a time.
    let mut slow = segment("b", 10, 30);
    slow.speed = 0.25;
    let f = filter_of(&plan_of(&edl(vec![slow])).commands[0]);
    assert!(f.contains("atempo=0.500000,atempo=0.500000[a]"), "{f}");
    let mut fast = segment("c", 10, 30);
    fast.speed = 8.0;
    let f = filter_of(&plan_of(&edl(vec![fast])).commands[0]);
    assert!(f.contains("atempo=2.000000,atempo=2.000000,atempo=2.000000[a]"), "{f}");
}

#[test]
fn the_gain_is_in_decibels_and_a_missing_voice_is_silence() {
    let mut s = segment("a", 10, 30);
    s.audio = AudioCut { gain_db: -6.0, crossfade_ms: 0 };
    let f = filter_of(&plan_of(&edl(vec![s])).commands[0]);
    assert!(f.contains("volume=-6.000dB"), "{f}");

    // A range the voice did not record: silence of the right length.
    let mut m = session();
    m.tracks.audio = None;
    let e = edl(vec![segment("a", 10, 30)]);
    let p = build_plan(&m, &e, Path::new("/rec/s"), Path::new("/work"), Path::new("/out/video.mp4"), ENC).unwrap();
    let (a, f) = (args_of(&p.commands[0]), filter_of(&p.commands[0]));
    assert!(a.contains("-f lavfi -i anullsrc=r=48000:cl=mono"), "{a}");
    assert!(f.contains("atrim=duration=20.000000"), "{f}");
}

#[test]
fn the_encoder_is_the_hardware_one_when_there_is_one_and_x264_when_not() {
    let hw = args_of(&plan_of(&edl(vec![segment("a", 10, 30)])).commands[0]);
    assert!(hw.contains("-c:v h264_videotoolbox -b:v 8000k"), "{hw}");
    assert!(hw.contains("-c:a aac -b:a 160k"), "{hw}");
    let e = edl(vec![segment("a", 10, 30)]);
    let sw = build_plan(&session(), &e, Path::new("/rec/s"), Path::new("/work"), Path::new("/out/video.mp4"), Capabilities { videotoolbox: false, libx264: true, aac: true, rounding: true }).unwrap();
    assert!(args_of(&sw.commands[0]).contains("-c:v libx264 -preset veryfast -b:v 8000k"));
    let none = build_plan(&session(), &e, Path::new("/rec/s"), Path::new("/work"), Path::new("/out/video.mp4"), Capabilities { videotoolbox: false, libx264: false, aac: true, rounding: true });
    assert!(none.unwrap_err().contains("no H.264 encoder"));
    let no_aac = build_plan(&session(), &e, Path::new("/rec/s"), Path::new("/work"), Path::new("/out/video.mp4"), Capabilities { videotoolbox: true, libx264: true, aac: false, rounding: true });
    assert!(no_aac.unwrap_err().contains("no AAC encoder"));
}

#[test]
fn a_cut_where_the_screen_recorded_nothing_is_refused_in_words() {
    let mut m = session();
    m.tracks.screen.as_mut().unwrap().segments.clear();
    let err = build_plan(&m, &edl(vec![segment("a", 10, 30)]), Path::new("/r"), Path::new("/w"), Path::new("/o.mp4"), ENC).unwrap_err();
    assert!(err.contains("segment \"a\"") && err.contains("screen"), "{err}");
}

// ---- putting the pieces together ----

#[test]
fn the_pieces_are_joined_without_re_encoding_with_the_chapters_from_a_file_of_their_own() {
    let mut e = edl(vec![segment("a", 10, 30), segment("b", 210, 250)]);
    e.chapters = vec![crate::recorder_edl::Chapter { t_us: 5 * SEC, title: String::from("src/lib.rs") }];
    let p = plan_of(&e);
    assert_eq!(p.commands.len(), 3);
    let join = &p.commands[2];
    assert_eq!(join.id, "join");
    let a = args_of(join);
    assert!(a.contains("-f concat -safe 0 -i /work/pieces.txt"), "{a}");
    assert!(a.contains("-i /work/chapters.ffmeta"), "{a}");
    assert!(a.contains("-map 0 -map_metadata 1 -map_chapters 1 -c copy -movflags +faststart"), "{a}");
    assert!(a.ends_with("/out/video.mp4"), "{a}");
    assert_eq!(p.concat_list.0, PathBuf::from("/work/pieces.txt"));
    assert_eq!(p.concat_list.1, "file '/work/seg_a.mp4'\nfile '/work/seg_b.mp4'\n");
    assert_eq!(p.total_us, 60 * SEC);
    assert_eq!(p.output, PathBuf::from("/out/video.mp4"));
}

#[test]
fn a_file_name_with_an_apostrophe_is_quoted_for_the_concat_list() {
    let text = concat_list(&[PathBuf::from("/a b/it's.mp4")]);
    assert_eq!(text, "file '/a b/it'\\''s.mp4'\n");
}

#[test]
fn the_chapters_are_written_for_ffmpeg_each_ending_where_the_next_begins() {
    let mut e = edl(vec![segment("a", 10, 70)]);
    e.chapters = vec![
        crate::recorder_edl::Chapter { t_us: 0, title: String::from("src/lib.rs") },
        crate::recorder_edl::Chapter { t_us: 20 * SEC, title: String::from("a=b; c#d\\e\nf") },
    ];
    let text = ffmetadata(&e);
    assert_eq!(
        text,
        ";FFMETADATA1\n[CHAPTER]\nTIMEBASE=1/1000\nSTART=0\nEND=20000\ntitle=src/lib.rs\n[CHAPTER]\nTIMEBASE=1/1000\nSTART=20000\nEND=60000\ntitle=a\\=b\\; c\\#d\\\\e\\\nf\n"
    );
    assert_eq!(ffmetadata(&edl(vec![segment("a", 10, 70)])), ";FFMETADATA1\n", "no chapters, no chapter blocks");
}

#[test]
fn the_title_of_the_video_is_in_its_metadata() {
    let mut e = edl(vec![segment("a", 10, 70)]);
    e.title = Some(String::from("Reviewing 3.5 million lines"));
    assert!(ffmetadata(&e).starts_with(";FFMETADATA1\ntitle=Reviewing 3.5 million lines\n"));
}

// ---- what ffmpeg has ----

#[test]
fn the_encoders_ffmpeg_lists_are_read() {
    let text = " V....D libx264              libx264 H.264 / AVC\n V....D h264_videotoolbox    VideoToolbox H.264 Encoder (codec h264)\n A....D aac                  AAC (Advanced Audio Coding)\n V....D mjpeg  MJPEG\n";
    // (Only the encoders are read from the list: the rounded corners are found by trying, `probe`.)
    assert_eq!(parse_encoders(text), Capabilities { videotoolbox: true, libx264: true, aac: true, rounding: false });
    assert_eq!(parse_encoders(" V....D mjpeg  MJPEG\n"), Capabilities { videotoolbox: false, libx264: false, aac: false, rounding: false });
    assert_eq!(parse_encoders(" V....D hevc_videotoolbox   VideoToolbox H.265\n").videotoolbox, false, "only the H.264 one counts");
}

// ---- running ffmpeg (a stand-in script) ----

#[cfg(unix)]
mod running {
    use std::{
        fs,
        os::unix::fs::PermissionsExt,
        sync::atomic::{AtomicBool, AtomicU32, Ordering},
    };

    use super::*;

    struct Tmp(PathBuf);

    impl Tmp {
        fn new() -> Self {
            static NEXT: AtomicU32 = AtomicU32::new(0);
            let path = std::env::temp_dir().join(format!("azreview-export-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for Tmp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    /// A script that stands in for ffmpeg.
    fn script(dir: &Path, body: &str) -> PathBuf {
        let path = dir.join("ffmpeg");
        fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    fn plan_in(dir: &Path, segments: Vec<EdlSegment>) -> Plan {
        build_plan(&session(), &edl(segments), Path::new("/rec/s"), &dir.join("work"), &dir.join("out/video.mp4"), ENC).unwrap()
    }

    #[test]
    fn every_command_is_run_in_order_the_video_is_made_and_the_pieces_are_removed() {
        let tmp = Tmp::new();
        let log = tmp.0.join("calls.log");
        let ff = script(
            &tmp.0,
            &format!("echo \"$1 $2 $3 $(echo \"$@\" | wc -w)\" >> {}\nfor last; do :; done\necho 'frame=1 time=00:00:01.00' >&2\necho piece > \"$last\"", log.display()),
        );
        let plan = plan_in(&tmp.0, vec![segment("a", 10, 30), segment("b", 210, 250)]);
        let mut seen = Vec::new();
        let report = run(&plan, &ff, &AtomicBool::new(false), &mut |p| seen.push(p)).unwrap();
        assert_eq!(report.output, tmp.0.join("out/video.mp4"));
        assert!(report.bytes > 0 && report.output.exists());
        let calls = fs::read_to_string(&log).unwrap();
        assert_eq!(calls.lines().count(), 3, "two cuts and the join: {calls}");
        assert!(calls.lines().all(|l| l.starts_with("-hide_banner -nostdin -y")), "{calls}");
        assert!(!tmp.0.join("work/seg_a.mp4").exists() && !tmp.0.join("work/pieces.txt").exists(), "the pieces and the lists are removed");
        assert!(seen.windows(2).all(|w| w[0].fraction <= w[1].fraction + 1e-9), "progress never goes back: {seen:?}");
        assert_eq!((seen[0].step, seen[0].steps, seen[0].fraction), (1, 3, 0.0));
        assert!((seen.last().unwrap().fraction - 1.0).abs() < 1e-9);
    }

    #[test]
    fn the_lists_ffmpeg_reads_are_written_before_it_runs() {
        let tmp = Tmp::new();
        let ff = script(&tmp.0, &format!("cat {}/work/pieces.txt >> {}/seen.txt\nfor last; do :; done\necho x > \"$last\"", tmp.0.display(), tmp.0.display()));
        let plan = plan_in(&tmp.0, vec![segment("a", 10, 30)]);
        run(&plan, &ff, &AtomicBool::new(false), &mut |_| {}).unwrap();
        assert_eq!(fs::read_to_string(tmp.0.join("seen.txt")).unwrap().lines().next().unwrap(), format!("file '{}/work/seg_a.mp4'", tmp.0.display()));
    }

    #[test]
    fn a_failing_ffmpeg_is_named_with_the_end_of_what_it_said_and_leaves_no_video() {
        let tmp = Tmp::new();
        let ff = script(&tmp.0, "echo 'first line' >&2\necho 'Error opening input: No such file or directory' >&2\nexit 1");
        let plan = plan_in(&tmp.0, vec![segment("a", 10, 30)]);
        let err = run(&plan, &ff, &AtomicBool::new(false), &mut |_| {}).unwrap_err();
        let ExportError::Failed { step, status, stderr_tail } = &err else { panic!("{err:?}") };
        assert_eq!((step.as_str(), *status), ("a", Some(1)));
        assert!(stderr_tail.contains("Error opening input"), "{stderr_tail}");
        assert!(err.to_string().contains("ffmpeg failed at a (exit 1)"));
        assert!(!tmp.0.join("out/video.mp4").exists());
    }

    #[test]
    fn a_cancel_stops_ffmpeg_and_cleans_up() {
        let tmp = Tmp::new();
        let ff = script(&tmp.0, "sleep 30");
        let plan = plan_in(&tmp.0, vec![segment("a", 10, 30)]);
        let cancel = std::sync::Arc::new(AtomicBool::new(false));
        let flag = std::sync::Arc::clone(&cancel);
        let t0 = std::time::Instant::now();
        let stopper = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(200));
            flag.store(true, Ordering::Relaxed);
        });
        let err = run(&plan, &ff, &cancel, &mut |_| {}).unwrap_err();
        stopper.join().unwrap();
        assert_eq!(err, ExportError::Cancelled);
        assert!(t0.elapsed() < std::time::Duration::from_secs(5), "{:?}", t0.elapsed());
        assert!(!tmp.0.join("out/video.mp4").exists() && !tmp.0.join("work/pieces.txt").exists());
    }

    #[test]
    fn an_ffmpeg_that_is_not_there_is_an_error_in_words() {
        let tmp = Tmp::new();
        let plan = plan_in(&tmp.0, vec![segment("a", 10, 30)]);
        let err = run(&plan, &tmp.0.join("no-such-ffmpeg"), &AtomicBool::new(false), &mut |_| {}).unwrap_err();
        assert!(matches!(&err, ExportError::Io(e) if e.contains("no-such-ffmpeg")), "{err:?}");
    }

    #[test]
    fn ffmpegs_progress_line_gives_the_seconds() {
        assert_eq!(parse_time("frame=  120 fps=60 q=29.0 size=  512kB time=00:01:02.50 bitrate=67.1kbits/s speed=2.5x"), Some(62.5));
        assert_eq!(parse_time("no time here"), None);
        assert_eq!(parse_time("time=N/A"), None);
    }
}

// ---- the screen alone is joined, not encoded; the camera's corner and shape ----

/// A session of one run with the screen and the voice and no camera over it, exported at the screen's own
/// size, is ONE command that joins the screen's files with their pictures copied (no filter graph, no video
/// encoder named) and makes the voice AAC; the chapters are the third input. The cuts are not made one by
/// one: this is what makes an hour of a display an affair of seconds.
///
/// Mutation: a copy plan that still names an encoder; one that reads the voice's files from the screen's list.
#[test]
fn a_screen_alone_of_one_run_at_its_own_size_is_joined_with_the_pictures_copied() {
    let m = screen_alone();
    let e = identity_edl(&m, &[(10 * SEC, String::from("src/lib.rs"))]);
    assert_eq!((e.output.width, e.output.height), (2880, 1800), "the screen's own size");
    let p = plan_for(&m, &e, ENC).unwrap();
    assert_eq!(p.commands.len(), 1, "one command, not one for the cut and one for the join");
    let c = &p.commands[0];
    assert_eq!((c.id.as_str(), c.us), ("join", 100 * SEC));
    let a = args_of(c);
    assert!(!a.contains("-filter_complex") && !a.contains("h264_videotoolbox") && !a.contains("libx264"), "{a}");
    assert!(a.contains("-f concat -safe 0 -i /work/screen_pieces.txt -f concat -safe 0 -i /work/audio_pieces.txt -i /work/chapters.ffmeta"), "{a}");
    assert!(a.contains("-map 0:v -map 1:a -map_metadata 2 -map_chapters 2 -c:v copy"), "{a}");
    assert!(a.contains("-af aresample=48000 -c:a aac -b:a 160k -ar 48000 -movflags +faststart /out/video.mp4"), "{a}");
    assert_eq!(p.concat_list.0, PathBuf::from("/work/screen_pieces.txt"));
    assert_eq!(p.concat_list.1, "file '/rec/s/screen/0001.mp4'\nfile '/rec/s/screen/0002.mp4'\n");
    assert_eq!(p.extra_lists, vec![(PathBuf::from("/work/audio_pieces.txt"), String::from("file '/rec/s/audio/0001.mp4'\nfile '/rec/s/audio/0002.mp4'\n"))]);
    assert!(p.metadata.1.contains("title=src/lib.rs"), "{}", p.metadata.1);
    // It needs no video encoder at all, and an AAC one only for the voice.
    let none = Capabilities { videotoolbox: false, libx264: false, aac: true, rounding: true };
    assert!(plan_for(&m, &e, none).is_ok());
    let no_aac = Capabilities { aac: false, ..ENC };
    assert_eq!(plan_for(&m, &e, no_aac).unwrap_err(), "no AAC encoder in this ffmpeg");
    let mut silent = m.clone();
    silent.tracks.audio = None;
    let p = plan_for(&silent, &identity_edl(&silent, &[]), no_aac).unwrap();
    assert!(!args_of(&p.commands[0]).contains("-map 1:a") && p.extra_lists.is_empty(), "no voice: a video alone");
}

/// A voice that began after the run did is delayed, and a gain is applied: the voice is encoded anyway.
#[test]
fn the_voice_of_a_straight_copy_keeps_its_start_and_its_gain() {
    let mut m = screen_alone();
    m.tracks.audio.as_mut().unwrap().segments[0].start_us = 5 * SEC;
    let mut e = identity_edl(&m, &[]);
    e.segments[0].audio.gain_db = -3.0;
    let a = args_of(&plan_for(&m, &e, ENC).unwrap().commands[0]);
    assert!(a.contains("-af aresample=48000,adelay=5000|5000,volume=-3.000dB"), "{a}");
}

/// Whatever the cut list asks that the recorded pictures cannot give as they are makes the cuts one by one
/// again, with a video encoder: a camera over the screen, a second run, a zoom, a speed, a cut inside a file,
/// another size than the screen's, a gap between two files, a picture that begins late.
///
/// Mutation: a copy for any of them (the picture would be wrong, or lose a gap and drift from the voice).
#[test]
fn anything_the_pictures_cannot_give_as_they_are_is_encoded() {
    let m = screen_alone();
    let base = identity_edl(&m, &[]);
    assert_eq!(plan_for(&m, &base, ENC).unwrap().commands.len(), 1, "the straight copy it is");
    let encoded = |m: &Manifest, e: &Edl| {
        let p = plan_for(m, e, ENC).unwrap();
        p.commands.len() == e.segments.len() + 1 && p.commands[0].args.iter().any(|a| a == "-filter_complex")
    };
    // A camera over the screen, or a second run.
    assert!(encoded(&session(), &identity_edl(&session(), &[])));
    let mut zoomed = base.clone();
    zoomed.segments[0].zoom = Some(Zoom { x: 0, y: 0, w: 1440, h: 900, ease_ms: 0 });
    assert!(encoded(&m, &zoomed));
    let mut fast = base.clone();
    fast.segments[0].speed = 2.0;
    assert!(encoded(&m, &fast));
    let mut inside = base.clone();
    inside.segments[0].in_us = 10 * SEC;
    assert!(encoded(&m, &inside), "a cut inside a file is not at a keyframe");
    let mut smaller = base.clone();
    smaller.output.width = 1920;
    smaller.output.height = 1200;
    assert!(encoded(&m, &smaller), "another size than the screen's");
    let mut resized = m.clone();
    resized.tracks.screen.as_mut().unwrap().segments[1].width = 1440;
    resized.tracks.screen.as_mut().unwrap().segments[1].height = 900;
    assert!(plan_for(&resized, &base, ENC).unwrap_err().contains("spans a resize"), "a file of another size is not copied, and the cuts cannot join two sizes");
    let mut gap = m.clone();
    gap.tracks.screen.as_mut().unwrap().segments[1].start_us = 70 * SEC;
    assert!(encoded(&gap, &base), "ten seconds are missing between the two files");
    let mut late = m.clone();
    late.tracks.screen.as_mut().unwrap().segments[0].start_us = 3 * SEC;
    assert!(encoded(&late, &base), "the picture begins three seconds into the run");
    // And without a video encoder, it cannot be done.
    let none = Capabilities { videotoolbox: false, libx264: false, aac: true, rounding: true };
    assert!(plan_for(&m, &zoomed, none).unwrap_err().contains("no H.264 encoder"));
}

/// The camera is a fifth of the picture, in the bottom right corner a margin from the edges, a rectangle
/// unless the cut list says `rounded`: then the picture is split, a grey mask with the corners cut out by
/// quarter circles is made from one copy and becomes the alpha of the other (`alphamerge`) before it is laid
/// over the screen.
///
/// Mutation: the rounding that stays on a rectangle; a rounded camera without its mask.
#[test]
fn the_camera_is_overlaid_a_fifth_wide_in_the_bottom_right_corner_and_rounded_when_asked() {
    let mut e = edl(vec![segment("a", 0, 60)]);
    let f = filter_of(&plan_of(&e).commands[0]);
    assert!(f.contains("scale=256:-2"), "a fifth of 1280: {f}");
    assert!(f.contains("overlay=x=W-w-24:y=H-h-24"), "{f}");
    assert!(!f.contains("geq") && !f.contains("alphamerge") && f.contains("[cam]"), "a rectangle has no mask: {f}");
    e.style.camera.shape = String::from("rounded");
    let f = filter_of(&plan_of(&e).commands[0]);
    assert!(f.contains("scale=256:-2,fps=30,tpad=stop_mode=clone:stop_duration=60.000000[cam_raw]"), "the camera comes in raw: {f}");
    assert!(
        f.contains("[cam_raw]split[cam_a][cam_b];[cam_b]scale=iw:ih,format=gray,geq=lum='if(gt(abs(W/2-X),W/2-28)*gt(abs(H/2-Y),H/2-28)"),
        "a radius of a ninth of 256, the mask made from a copy of the camera (converted, so that the copy's grey is not the camera's): {f}"
    );
    assert!(f.contains("[cam_a][cam_m]alphamerge[cam]"), "{f}");
    assert!(f.find("alphamerge").unwrap() < f.find("[b1][cam]overlay").unwrap(), "the mask is made before the overlay: {f}");
}

// ---- what this ffmpeg can do, found by trying ----

/// The picture of the rounding probe as ffmpeg gives it back when the route worked: a blue screen of
/// `PROBE_SIDE` x `PROBE_SIDE` with the red camera on it, the camera's four corners cut away (down to the
/// screen) in squares of `cut` pixels, as RGB bytes.
fn probe_picture(cut: usize) -> Vec<u8> {
    let (x0, y0, w, h) = (PROBE_CAM_X, PROBE_CAM_Y, PROBE_CAM_W, PROBE_CAM_H);
    let mut rgb = Vec::with_capacity(PROBE_SIDE * PROBE_SIDE * 3);
    for y in 0..PROBE_SIDE {
        for x in 0..PROBE_SIDE {
            let on_camera = (x0..x0 + w).contains(&x) && (y0..y0 + h).contains(&y);
            let (cx, cy) = (x.wrapping_sub(x0), y.wrapping_sub(y0));
            let cut_away = on_camera && (cx < cut || cx + cut >= w) && (cy < cut || cy + cut >= h);
            rgb.extend_from_slice(if on_camera && !cut_away { &[250, 5, 5] } else { &[5, 5, 250] });
        }
    }
    rgb
}

/// The probe reads the picture ffmpeg made back: the camera's four corners show the screen, its middle and the
/// middles of its edges are the camera, the outside is the screen; anything else is said in words.
///
/// Mutation: a judge that accepts a camera that still has its corners; one that accepts a camera that is gone;
/// one that accepts a rounding that eats the edges.
#[test]
fn the_probe_judges_the_picture_it_gets_back_and_says_what_is_wrong_in_words() {
    assert_eq!(rounding_worked(&probe_picture(7)), Ok(()));
    let square = rounding_worked(&probe_picture(0)).unwrap_err();
    assert!(square.contains("corner") && square.contains("camera"), "{square}");
    let mut gone = probe_picture(0);
    for px in gone.chunks_exact_mut(3) {
        px.copy_from_slice(&[5, 5, 250]);
    }
    let gone = rounding_worked(&gone).unwrap_err();
    assert!(gone.contains("picture is gone"), "a mask that cut everything away: {gone}");
    let mut eaten = probe_picture(7);
    let at = ((PROBE_CAM_Y + 1) * PROBE_SIDE + PROBE_CAM_X + PROBE_CAM_W / 2) * 3;
    eaten[at..at + 3].copy_from_slice(&[5, 5, 250]);
    let eaten = rounding_worked(&eaten).unwrap_err();
    assert!(eaten.contains("edge"), "a rounding that cuts the middle of an edge: {eaten}");
    let mut no_screen = probe_picture(7);
    let outside = (2 * PROBE_SIDE + 2) * 3;
    no_screen[outside..outside + 3].copy_from_slice(&[250, 5, 5]);
    assert!(rounding_worked(&no_screen).unwrap_err().contains("outside"), "the screen is missing where the camera is not");
    assert!(rounding_worked(&[]).unwrap_err().contains("0 bytes"));
    assert!(rounding_worked(&probe_picture(7)[..100]).unwrap_err().contains("100 bytes"));
}

/// The probe makes ONE picture of a made-up blue screen and a made-up red camera through the very graph the
/// export uses (the same function makes both), so what it finds out is about the route the export takes.
///
/// Mutation: a probe with a graph of its own; a probe that never overlays the camera.
#[test]
fn the_probe_makes_one_picture_through_the_graph_the_export_uses() {
    let args = rounding_probe_args();
    let text = args.join(" ");
    let at = args.iter().position(|a| a == "-filter_complex").expect("a filter graph");
    let graph = &args[at + 1];
    assert!(graph.contains(&rounding_graph(PROBE_CAM_W as u32)), "the export's route, not one of its own: {graph}");
    assert!(graph.contains(&format!("overlay=x={PROBE_CAM_X}:y={PROBE_CAM_Y}")) && graph.ends_with("format=rgb24[v]"), "{graph}");
    assert!(graph.contains(&format!("scale={PROBE_CAM_W}:-2")), "the camera is scaled as the export scales it: {graph}");
    assert!(text.contains("-map [v] -frames:v 1 -f rawvideo -pix_fmt rgb24 -"), "{text}");
    assert!(text.contains(&format!("color=c=blue:s={PROBE_SIDE}x{PROBE_SIDE}")), "{text}");
}

/// What a plan does when ffmpeg was tried at rounding corners and could not: the camera stays a rectangle, the
/// plan says so (a person reads that before the export runs), and nothing is said where nothing is lost.
///
/// Mutation: a plan that rounds where the probe said it cannot; one that is silent about the rectangle; one
/// that complains where a rectangle was asked for.
#[test]
fn a_rounded_camera_on_an_ffmpeg_that_cannot_round_stays_a_rectangle_and_the_plan_says_so() {
    let mut e = edl(vec![segment("a", 0, 60)]);
    e.style.camera.shape = String::from("rounded");
    let cannot = Capabilities { rounding: false, ..ENC };
    let p = plan_for(&session(), &e, cannot).unwrap();
    let f = filter_of(&p.commands[0]);
    assert!(!f.contains("alphamerge") && !f.contains("geq") && !f.contains("cam_raw"), "{f}");
    assert!(f.contains("[b1][cam]overlay=x=W-w-24:y=H-h-24"), "the camera is still there, a rectangle: {f}");
    assert_eq!(p.notes.len(), 1, "{:?}", p.notes);
    assert!(p.notes[0].contains("rounded") && p.notes[0].contains("rectangle"), "{}", p.notes[0]);
    let p = plan_for(&session(), &e, ENC).unwrap();
    assert!(filter_of(&p.commands[0]).contains("alphamerge") && p.notes.is_empty(), "{:?}", p.notes);
    e.style.camera.shape = String::from("rect");
    assert!(plan_for(&session(), &e, cannot).unwrap().notes.is_empty(), "a rectangle was asked for");
    // And a cut list without the camera in it has nothing to say either.
    e.style.camera.shape = String::from("rounded");
    for seg in &mut e.segments {
        seg.camera = false;
    }
    assert!(plan_for(&session(), &e, cannot).unwrap().notes.is_empty(), "no camera in the cuts");
}

/// An ffmpeg that is not there or fails at once is an error in words: nothing is guessed about it.
#[test]
fn an_ffmpeg_that_will_not_start_or_fails_is_an_error_in_words() {
    let err = probe(Path::new("/no/such/ffmpeg")).unwrap_err();
    assert!(err.contains("/no/such/ffmpeg"), "{err}");
    if Path::new("/usr/bin/false").is_file() {
        let err = probe(Path::new("/usr/bin/false")).unwrap_err();
        assert!(err.contains("failed"), "{err}");
    }
}

// ---- the whole way, with a real ffmpeg (skipped where there is none) ----

mod e2e {
    use std::{
        fs,
        process::Command as Process,
        sync::{atomic::AtomicBool, Arc},
    };

    use super::*;
    use crate::{
        recorder::{Deps, Perm, Recorder, StartRequest, Started},
        recorder_clock::{Clock, MockClock},
        recorder_codec::{AudioCodec, CodecFactory, PixelFormat, RawFrame, VideoCodec},
        recorder_edl::{file_changes, identity_edl},
        recorder_mp4::annexb_nals,
        recorder_screen::{Damage, TapFrame},
        recorder_settings::RecorderSettings,
    };

    /// The end-to-end test needs a real ffmpeg. Where there is none it says so and goes (a test that passed
    /// without testing anything is no test), and a run that sets `AZ_REQUIRE_FFMPEG=1` (the gate's does,
    /// after it installed ffmpeg) fails instead of going.
    fn skipped(why: &str) {
        assert!(std::env::var("AZ_REQUIRE_FFMPEG").as_deref() != Ok("1"), "AZ_REQUIRE_FFMPEG=1, and {why}");
        eprintln!("SKIPPED the end-to-end export: {why}");
    }

    /// A tool of the ffmpeg family, where it is usually installed.
    fn find_tool(name: &str) -> Option<PathBuf> {
        ["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin"].iter().map(|d| Path::new(d).join(name)).find(|p| p.is_file())
    }

    /// A stand-in for ffmpeg: a shell script that answers `-encoders` with a list and everything else with `body`.
    #[cfg(unix)]
    fn stand_in(name: &str, body: &str) -> (Tmp, PathBuf) {
        use std::os::unix::fs::PermissionsExt;
        let tmp = Tmp(std::env::temp_dir().join(format!("azreview-standin-{name}-{}", std::process::id())));
        let _ = fs::remove_dir_all(&tmp.0);
        fs::create_dir_all(&tmp.0).unwrap();
        let path = tmp.0.join("ffmpeg");
        let script = format!("#!/bin/sh\ncase \"$*\" in\n*-encoders*) echo ' V....D libx264              libx264 H.264'; echo ' A....D aac                  AAC';;\n*) {body};;\nesac\n");
        fs::write(&path, script).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        (tmp, path)
    }

    /// The whole probe against stand-ins: an ffmpeg that gives a picture with the corners cut says it can round; one
    /// that gives the camera's square says it cannot (and why, in words); one that gives nothing readable says
    /// so; one that never answers is given up on, and killed, at the time limit.
    #[cfg(unix)]
    #[test]
    fn the_probe_finds_out_by_trying_what_a_stand_in_ffmpeg_can_do() {
        let rounded = probe_picture(7);
        let square = probe_picture(0);
        let (tmp, good) = stand_in("good", &format!("printf '{}'", escape_octal(&rounded)));
        let found = probe(&good).unwrap();
        assert!(found.caps.rounding && found.caps.libx264 && found.caps.aac && !found.caps.videotoolbox, "{found:?}");
        assert!(found.notes.is_empty(), "{:?}", found.notes);
        drop(tmp);
        let (tmp, flat) = stand_in("flat", &format!("printf '{}'", escape_octal(&square)));
        let found = probe(&flat).unwrap();
        assert!(!found.caps.rounding && found.caps.libx264, "the encoders are still found: {found:?}");
        assert_eq!(found.notes.len(), 1, "{:?}", found.notes);
        assert!(found.notes[0].contains("rounded corners") && found.notes[0].contains("corner"), "{}", found.notes[0]);
        drop(tmp);
        let (tmp, nothing) = stand_in("nothing", "printf 'xx'");
        let found = probe(&nothing).unwrap();
        assert!(!found.caps.rounding && found.notes[0].contains("2 bytes"), "{:?}", found.notes);
        drop(tmp);
        let (tmp, failing) = stand_in("failing", "echo 'No such filter: geq' >&2; exit 1");
        let found = probe(&failing).unwrap();
        assert!(!found.caps.rounding && found.notes[0].contains("No such filter"), "what ffmpeg said is in the note: {:?}", found.notes);
        drop(tmp);
        let (tmp, hanging) = stand_in("hanging", "exec sleep 30");
        let started = std::time::Instant::now();
        let found = probe_within(&hanging, Duration::from_millis(400)).unwrap();
        assert!(started.elapsed() < Duration::from_secs(10), "given up on at the limit");
        assert!(!found.caps.rounding && found.notes[0].contains("did not answer"), "{:?}", found.notes);
        drop(tmp);
    }

    /// `bytes` as the `\ddd` octal escapes `printf` reads.
    #[cfg(unix)]
    fn escape_octal(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("\\{b:03o}")).collect()
    }

    /// What a real ffmpeg answers to the question: it makes the probe's camera round (the route the export takes
    /// works on this one, found by trying), and the log says what this ffmpeg is.
    #[test]
    fn a_real_ffmpeg_makes_the_probes_camera_round() {
        let Some(ffmpeg) = find_tool("ffmpeg") else {
            skipped("there is no ffmpeg in /opt/homebrew/bin, /usr/local/bin or /usr/bin");
            return;
        };
        let found = probe(&ffmpeg).unwrap_or_else(|e| panic!("{e}"));
        eprintln!("probe: {:?} {:?}", found.caps, found.notes);
        if !found.caps.rounding {
            // (Under AZ_REQUIRE_FFMPEG=1 this fails: the gate's ffmpeg is the one the route has to work on.)
            skipped(&format!("this ffmpeg did not round the made-up camera: {:?}", found.notes));
        }
    }

    /// The bytes ffmpeg writes to its standard output for these arguments; `None` when it fails.
    fn ffmpeg_bytes(ffmpeg: &Path, args: &[&str]) -> Option<Vec<u8>> {
        let out = Process::new(ffmpeg).args(["-hide_banner", "-loglevel", "error"]).args(args).output().ok()?;
        (out.status.success() && !out.stdout.is_empty()).then_some(out.stdout)
    }

    /// The access units of an Annex-B stream of one slice a picture.
    fn split_aus(stream: &[u8]) -> Vec<Vec<u8>> {
        let mut aus = Vec::new();
        let mut cur: Vec<&[u8]> = Vec::new();
        for nal in annexb_nals(stream) {
            cur.push(nal);
            if matches!(nal[0] & 0x1f, 1 | 5) {
                aus.push(cur.drain(..).flat_map(|n| [&[0, 0, 0, 1][..], n].concat()).collect());
            }
        }
        aus
    }

    /// The Opus packets of an Ogg file.
    fn ogg_packets(data: &[u8]) -> Vec<Vec<u8>> {
        let (mut pos, mut packets, mut cur) = (0, Vec::new(), Vec::new());
        while pos + 27 <= data.len() && &data[pos..pos + 4] == b"OggS" {
            let n = data[pos + 26] as usize;
            let table = &data[pos + 27..pos + 27 + n];
            let mut body = pos + 27 + n;
            for &s in table {
                cur.extend_from_slice(&data[body..body + s as usize]);
                body += s as usize;
                if s < 255 {
                    packets.push(std::mem::take(&mut cur));
                }
            }
            pos = body;
        }
        packets.into_iter().skip(2).collect()
    }

    /// An encoder that hands out real H.264, one access unit a picture, round and round; a keyframe asked
    /// for is the next IDR of the stream, so every segment begins where a decoder can.
    struct Replay {
        aus: Vec<Vec<u8>>,
        idr: Vec<usize>,
        next: usize,
        ready: std::collections::VecDeque<Vec<u8>>,
    }

    impl Replay {
        fn new(aus: Vec<Vec<u8>>) -> Self {
            let idr = aus.iter().enumerate().filter(|(_, au)| annexb_nals(au).iter().any(|n| n[0] & 0x1f == 5)).map(|(i, _)| i).collect();
            Self { aus, idr, next: 0, ready: std::collections::VecDeque::new() }
        }
    }

    impl VideoCodec for Replay {
        fn submit(&mut self, _frame: RawFrame, keyframe: bool) -> bool {
            let len = self.aus.len();
            if keyframe {
                let pos = self.next % len;
                let jump = self.idr.iter().copied().find(|&i| i >= pos);
                self.next = match jump {
                    Some(i) => self.next - pos + i,
                    None => self.next - pos + len + self.idr[0],
                };
            }
            self.ready.push_back(self.aus[self.next % len].clone());
            self.next += 1;
            true
        }
        fn poll(&mut self) -> Option<Vec<u8>> {
            self.ready.pop_front()
        }
        fn flush(&mut self) {}
        fn name(&self) -> String {
            String::from("replay")
        }
        fn hardware(&self) -> bool {
            false
        }
    }

    struct ReplayVoice {
        packets: Vec<Vec<u8>>,
        next: usize,
        pending: usize,
        ready: std::collections::VecDeque<Vec<u8>>,
    }

    impl AudioCodec for ReplayVoice {
        fn submit(&mut self, samples: &[f32]) -> bool {
            self.pending += samples.len();
            while self.pending >= 960 {
                self.pending -= 960;
                self.ready.push_back(self.packets[self.next % self.packets.len()].clone());
                self.next += 1;
            }
            true
        }
        fn poll(&mut self) -> Option<Vec<u8>> {
            self.ready.pop_front()
        }
        fn name(&self) -> String {
            String::from("replay")
        }
    }

    struct ReplayFactory {
        screen: Vec<Vec<u8>>,
        camera: Vec<Vec<u8>>,
        voice: Vec<Vec<u8>>,
        /// The width of the screen's pictures: an encoder opened for it replays the screen's stream.
        screen_width: u32,
    }

    impl CodecFactory for ReplayFactory {
        fn video(&self, width: u32, _height: u32, _kbps: u32) -> Result<Box<dyn VideoCodec>, String> {
            let aus = if width == self.screen_width { self.screen.clone() } else { self.camera.clone() };
            Ok(Box::new(Replay::new(aus)))
        }
        fn audio(&self, _rate: u32, _channels: u16, _kbps: u32) -> Result<Box<dyn AudioCodec>, String> {
            Ok(Box::new(ReplayVoice { packets: self.voice.clone(), next: 0, pending: 0, ready: std::collections::VecDeque::new() }))
        }
    }

    struct Tmp(PathBuf);

    impl Drop for Tmp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    /// What the end-to-end tests need: the tools, what ffmpeg can encode, real H.264 of two sizes and real Opus
    /// (what the encoders would have made), and a folder of their own.
    struct Rig {
        ffmpeg: PathBuf,
        ffprobe: PathBuf,
        caps: Capabilities,
        screen: Vec<Vec<u8>>,
        camera: Vec<Vec<u8>>,
        voice: Vec<Vec<u8>>,
        /// The sizes of the screen's and the camera's pictures.
        screen_size: (u32, u32),
        camera_size: (u32, u32),
        tmp: Tmp,
    }

    /// The rig, or `None` (said, and a failure under `AZ_REQUIRE_FFMPEG=1`) where ffmpeg cannot do it.
    fn rig(name: &str) -> Option<Rig> {
        rig_sized(name, (640, 360), (320, 240))
    }

    /// [`rig`] with pictures of these sizes.
    fn rig_sized(name: &str, screen_size: (u32, u32), camera_size: (u32, u32)) -> Option<Rig> {
        let (Some(ffmpeg), Some(ffprobe)) = (find_tool("ffmpeg"), find_tool("ffprobe")) else {
            skipped("there is no ffmpeg and ffprobe in /opt/homebrew/bin, /usr/local/bin or /usr/bin");
            return None;
        };
        let found = match probe(&ffmpeg) {
            Ok(found) => found,
            Err(why) => {
                skipped(&format!("ffmpeg cannot be asked what it can do: {why}"));
                return None;
            }
        };
        let caps = found.caps;
        eprintln!("what this ffmpeg can do: {caps:?}{}", found.notes.iter().map(|n| format!("; {n}")).collect::<String>());
        if !(caps.aac && (caps.libx264 || caps.videotoolbox)) {
            skipped("this ffmpeg lacks aac or an H.264 encoder");
            return None;
        }
        // The screen is the test pattern, the camera a picture of red, green and blue stripes: nothing of the camera
        // is mistaken for the screen, and a camera that lost its colours is seen.
        let h264 = |source: &str, size: &str| ffmpeg_bytes(&ffmpeg, &["-f", "lavfi", "-i", &format!("{source}=size={size}:rate=30"), "-t", "6", "-c:v", "libx264", "-g", "30", "-bf", "0", "-pix_fmt", "yuv420p", "-f", "h264", "-"]);
        let opus = ffmpeg_bytes(&ffmpeg, &["-f", "lavfi", "-i", "sine=frequency=440:sample_rate=48000", "-t", "6", "-c:a", "libopus", "-b:a", "32k", "-frame_duration", "20", "-f", "ogg", "-"]);
        let (Some(screen), Some(camera), Some(opus)) = (h264("testsrc2", &format!("{}x{}", screen_size.0, screen_size.1)), h264("rgbtestsrc", &format!("{}x{}", camera_size.0, camera_size.1)), opus) else {
            skipped("this ffmpeg cannot make the test streams (it needs libx264 and libopus)");
            return None;
        };
        eprintln!("ffmpeg: {}", String::from_utf8_lossy(&Process::new(&ffmpeg).arg("-version").output().unwrap().stdout).lines().next().unwrap_or(""));
        let tmp = Tmp(std::env::temp_dir().join(format!("azreview-e2e-{name}-{}", std::process::id())));
        let _ = fs::remove_dir_all(&tmp.0);
        fs::create_dir_all(&tmp.0).unwrap();
        Some(Rig { ffmpeg, ffprobe, caps, screen: split_aus(&screen), camera: split_aus(&camera), voice: ogg_packets(&opus), screen_size, camera_size, tmp })
    }

    /// A session of six seconds recorded through the recorder with the rig's encoders: the window changes ten
    /// times a second, the camera sends 15 pictures a second and the voice 50 packets; three seconds, then (when
    /// `with_pause`) a pause of two seconds, then three seconds more.
    fn record(rig: &Rig, with_pause: bool, with_camera: bool) -> Started {
        record_blocks(rig, 2, with_pause, with_camera)
    }

    /// [`record`] for `blocks` of three seconds (a pause, when asked for, after the first).
    fn record_blocks(rig: &Rig, blocks: u64, with_pause: bool, with_camera: bool) -> Started {
        let (sw, sh) = rig.screen_size;
        let (cw, ch) = rig.camera_size;
        let factory = ReplayFactory { screen: rig.screen.clone(), camera: rig.camera.clone(), voice: rig.voice.clone(), screen_width: sw };
        let clock = MockClock::at(0);
        let c = clock.clone();
        let deps = Deps {
            unix_ms: Box::new(|| 1_764_108_000_000),
            make_clock: Box::new(move |_| Arc::new(c.clone()) as Arc<dyn Clock>),
            free_bytes: Box::new(|_| Some(500_000_000_000)),
            factory: Box::new(factory),
        };
        let mut rec = Recorder::new(RecorderSettings { idle_pause_us: None, camera: with_camera, ..RecorderSettings::default() }, deps);
        let started = rec
            .start(StartRequest {
                recordings_dir: rig.tmp.0.join("recordings"),
                app: AppInfo { name: String::from("AzReview"), version: String::from("0.1.0") },
                workspace: Workspace { root: String::from("/w"), name: String::from("azul") },
                host: HostInfo { os: String::from("macos"), arch: String::from("aarch64") },
                utc_offset_minutes: 0,
                camera: Perm::Granted,
                microphone: Perm::Granted,
                screen: Perm::Granted,
                camera_device: None,
                microphone_device: None,
                screen_device: None,
            })
            .unwrap();
        let window = vec![9_u8; sw as usize * sh as usize * 4];
        let second = |rec: &mut Recorder, from_ms: u64, events: &[(u64, &str)]| {
            for i in 0..30_u64 {
                clock.set_us((from_ms + i * 100) * 1_000);
                for (at, file) in events.iter().filter(|(at, _)| *at == i) {
                    let _ = at;
                    rec.event("nav.file", &format!("{{\"file\":\"{file}\",\"lines\":100}}")).unwrap();
                }
                rec.tap(&TapFrame { width: sw, height: sh, stride: sw as usize * 4, scale: 1.0, damage: Damage::Full, pixels: &window });
                if with_camera {
                    rec.camera_frame(RawFrame { width: cw, height: ch, format: PixelFormat::Nv12, bytes: vec![0; cw as usize * ch as usize * 3 / 2] });
                }
                for k in 0..5 {
                    clock.set_us((from_ms + i * 100 + k * 20 + 20) * 1_000);
                    rec.audio_chunk(&vec![0.1; 960], 48_000, 1);
                }
                rec.tick();
            }
        };
        second(&mut rec, 0, &[(0, "src/lib.rs"), (15, "src/ink.rs")]);
        if with_pause {
            clock.set_us(3_050_000);
            rec.toggle_pause();
            clock.set_us(5_000_000);
            rec.toggle_pause();
            second(&mut rec, 5_000, &[(0, "README.md")]);
        } else {
            for block in 1..blocks {
                second(&mut rec, block * 3_000, if block == 1 { &[(0, "README.md")] } else { &[] });
            }
        }
        let done = rec.stop("user").unwrap();
        assert!(done.clean, "{done:?}");
        started
    }

    #[test]
    fn a_recorded_session_becomes_a_playable_video_with_chapters() {
        let Some(rig) = rig("export") else { return };
        let (ffmpeg, ffprobe, encoders, tmp) = (rig.ffmpeg.clone(), rig.ffprobe.clone(), rig.caps, &rig.tmp);
        let started = record(&rig, true, true);

        // The cut list and the export.
        let manifest = Manifest::parse(&fs::read_to_string(started.dir.join("manifest.json")).unwrap()).unwrap();
        let files = file_changes(&fs::read_to_string(started.dir.join("events.jsonl")).unwrap());
        let edl = identity_edl(&manifest, &files);
        assert!(edl.problems(&manifest).is_empty(), "{:?}", edl.problems(&manifest));
        assert_eq!(edl.segments.len(), 2, "two runs, the pause is not in the video");
        let plan = build_plan(&manifest, &edl, &started.dir, &tmp.0.join("work"), &tmp.0.join("export/video.mp4"), encoders).unwrap();
        let report = run(&plan, &ffmpeg, &AtomicBool::new(false), &mut |_| {}).unwrap_or_else(|e| panic!("{e}"));
        assert!(report.bytes > 10_000, "{report:?}");

        // What came out.
        let probe = |args: &[&str]| {
            let out = Process::new(&ffprobe).args(["-v", "error"]).args(args).arg(&report.output).output().unwrap();
            assert!(out.status.success(), "ffprobe: {}", String::from_utf8_lossy(&out.stderr));
            String::from_utf8_lossy(&out.stdout).into_owned()
        };
        let streams = probe(&["-show_entries", "stream=codec_type,codec_name,width,height", "-of", "csv=p=0"]);
        assert!(streams.contains(&format!("h264,video,{},{}", edl.output.width, edl.output.height)) || streams.contains(&format!("h264,{},{},video", edl.output.width, edl.output.height)), "{streams}");
        assert!(streams.contains("aac"), "{streams}");
        let duration: f64 = probe(&["-show_entries", "format=duration", "-of", "csv=p=0"]).trim().parse().unwrap();
        assert!((duration - 6.0).abs() < 0.6, "six seconds of recording, a pause not in it: {duration}");
        let chapters = probe(&["-show_chapters", "-of", "csv=p=0"]);
        assert_eq!(chapters.lines().count(), 3, "a chapter for each file: {chapters}");
        // Every frame decodes.
        let decode = Process::new(&ffmpeg).args(["-v", "error", "-i"]).arg(&report.output).args(["-f", "null", "-"]).output().unwrap();
        assert!(decode.status.success() && decode.stderr.is_empty(), "{}", String::from_utf8_lossy(&decode.stderr));
        eprintln!("exported {} bytes, {duration:.2} s, {} chapters", report.bytes, chapters.lines().count());
        eprintln!("ffprobe streams (codec, type, size):\n{streams}ffprobe duration: {duration:.3} s\nffprobe chapters (id, time base, start, start s, end, end s):\n{chapters}");
    }

    /// What the window's Export button runs, with the real ffmpeg: a recorded session becomes `export/video.mp4` in its
    /// own folder through an `ExportJob`; the job's fraction only goes forward, the pieces are gone at the end, and the
    /// video is the one the doc's other tests check (h264 and aac, six seconds, a chapter for each file).
    #[test]
    fn a_recording_is_exported_by_a_job_to_a_video_in_its_own_folder() {
        use crate::recorder_export_job::{ExportJob, FfmpegRenderer, JobState};
        let Some(rig) = rig("job") else { return };
        let started = record(&rig, true, true);
        let job = ExportJob::start(started.dir.clone(), Box::new(FfmpegRenderer { ffmpeg: Some(rig.ffmpeg.clone()), preferred: None, out: None }));
        let from = std::time::Instant::now();
        let mut seen = Vec::new();
        let made = loop {
            match job.state() {
                JobState::Running { fraction } => seen.push(fraction),
                JobState::Done(made) => break made,
                other => panic!("{other:?} after {seen:?}"),
            }
            assert!(from.elapsed() < Duration::from_secs(180), "the export took too long: {seen:?}");
            std::thread::sleep(Duration::from_millis(5));
        };
        assert_eq!(made.file, started.dir.join("export").join("video.mp4"));
        assert!(seen.windows(2).all(|w| w[0] <= w[1]), "the fraction goes forward only: {seen:?}");
        assert_eq!(fs::metadata(&made.file).unwrap().len(), made.bytes);
        assert!(made.bytes > 10_000, "{made:?}");
        assert!(!started.dir.join("export").join(".work").exists(), "the pieces are gone");
        let probe = |args: &[&str]| {
            let out = Process::new(&rig.ffprobe).args(["-v", "error"]).args(args).arg(&made.file).output().unwrap();
            assert!(out.status.success(), "ffprobe: {}", String::from_utf8_lossy(&out.stderr));
            String::from_utf8_lossy(&out.stdout).into_owned()
        };
        let streams = probe(&["-show_entries", "stream=codec_type,codec_name", "-of", "csv=p=0"]);
        assert!(streams.contains("h264") && streams.contains("aac"), "{streams}");
        let duration: f64 = probe(&["-show_entries", "format=duration", "-of", "csv=p=0"]).trim().parse().unwrap();
        assert!((duration - 6.0).abs() < 0.6, "six seconds of recording, the pause not in it: {duration}");
        assert_eq!(probe(&["-show_chapters", "-of", "csv=p=0"]).lines().count(), 3, "a chapter for each file");
        eprintln!("job: {} bytes, {duration:.2} s, {} fraction reports", made.bytes, seen.len());
    }

    /// A cancel stops a real export quickly, leaves no half-made video and no pieces, and says so: one asked for as the
    /// export starts must stop it (a recording of a minute and a half takes the hardware encoder seconds), one asked
    /// for while ffmpeg is working stops that; and the same session is then exported to the end (an export that was
    /// cancelled can be started again).
    #[test]
    fn a_real_export_that_is_cancelled_leaves_nothing_and_can_be_made_again() {
        use crate::recorder_export_job::{ExportJob, FfmpegRenderer, JobState};
        let Some(rig) = rig("jobcancel") else { return };
        let started = record_blocks(&rig, 30, false, true);
        let renderer = || Box::new(FfmpegRenderer { ffmpeg: Some(rig.ffmpeg.clone()), preferred: None, out: None });
        let nothing_left = || {
            assert!(!started.dir.join("export").join("video.mp4").exists(), "no half-made video");
            assert!(!started.dir.join("export").join(".work").exists(), "no pieces");
        };
        let wait = |job: &ExportJob, limit: u64| {
            let from = std::time::Instant::now();
            while job.is_running() {
                assert!(from.elapsed() < Duration::from_secs(limit), "{:?}", job.state());
                std::thread::sleep(Duration::from_millis(2));
            }
        };

        // Asked for at once.
        let job = ExportJob::start(started.dir.clone(), renderer());
        job.cancel();
        wait(&job, 20);
        assert_eq!(job.state(), JobState::Cancelled, "a cancel at the start stops the export");
        nothing_left();
        drop(job);

        // Asked for while ffmpeg works.
        let job = ExportJob::start(started.dir.clone(), renderer());
        let from = std::time::Instant::now();
        while !matches!(job.state(), JobState::Running { fraction } if fraction > 0.0) && job.is_running() {
            assert!(from.elapsed() < Duration::from_secs(60), "{:?}", job.state());
            std::thread::sleep(Duration::from_millis(2));
        }
        let at_cancel = match job.state() {
            JobState::Running { fraction } => fraction,
            _ => 1.0,
        };
        let asked = std::time::Instant::now();
        job.cancel();
        wait(&job, 10);
        eprintln!("cancelled at {at_cancel:.3}; {:?} after {:?}", job.state(), asked.elapsed());
        match job.state() {
            JobState::Cancelled => nothing_left(),
            // An export that was more than half over when the cancel came may well have been a video by the time it
            // was looked at, and then the video is the truth; one that had just begun must have stopped.
            JobState::Done(_) => assert!(at_cancel >= 0.5, "the export was at {at_cancel} when it was cancelled, and went on to the end"),
            other => panic!("{other:?}"),
        }
        drop(job);

        // And to the end.
        let again = ExportJob::start(started.dir.clone(), renderer());
        wait(&again, 300);
        assert!(matches!(again.state(), JobState::Done(_)), "{:?}", again.state());
    }

    /// An export that cannot run says why in words: a program that is not there is named.
    #[test]
    fn an_export_with_no_ffmpeg_says_so_in_words() {
        use crate::recorder_export_job::{ExportJob, FfmpegRenderer, JobState};
        let Some(rig) = rig("nojob") else { return };
        let started = record(&rig, false, false);
        let job = ExportJob::start(started.dir.clone(), Box::new(FfmpegRenderer { ffmpeg: Some(PathBuf::from("/no/such/ffmpeg")), preferred: None, out: None }));
        let from = std::time::Instant::now();
        while job.is_running() {
            assert!(from.elapsed() < Duration::from_secs(30), "{:?}", job.state());
            std::thread::sleep(Duration::from_millis(2));
        }
        match job.state() {
            JobState::Failed(why) => assert!(why.contains("/no/such/ffmpeg"), "{why}"),
            other => panic!("{other:?}"),
        }
    }

    /// The command line, with the real ffmpeg: the video goes where `--out` says, the progress is printed in whole
    /// percents, the last line says where the video is and how big, and the exit code is 0.
    #[test]
    fn the_command_line_exports_a_recording_where_it_is_told_to() {
        use crate::{
            recorder_export_cli::{export_checked, Cli},
            recorder_export_job::FfmpegRenderer,
        };
        let Some(rig) = rig("cli") else { return };
        let started = record(&rig, false, true);
        let out_file = rig.tmp.0.join("elsewhere").join("review.mp4");
        let cli = Cli { session: started.dir.clone(), out: Some(out_file.clone()), ffmpeg: Some(rig.ffmpeg.clone()) };
        let mut renderer = FfmpegRenderer { ffmpeg: cli.ffmpeg.clone(), preferred: None, out: cli.out.clone() };
        let mut printed = Vec::new();
        let code = export_checked(&cli, &mut renderer, &mut printed);
        let text = String::from_utf8(printed).unwrap();
        assert_eq!(code, 0, "{text}");
        let lines: Vec<&str> = text.lines().collect();
        let last = lines.last().copied().unwrap_or_default();
        assert!(last.starts_with(&format!("EXPORT_DONE {} ", out_file.display())), "{text}");
        let bytes: u64 = last.rsplit(' ').next().unwrap().parse().unwrap();
        assert_eq!(fs::metadata(&out_file).unwrap().len(), bytes);
        let percents: Vec<u32> = lines.iter().filter_map(|l| l.strip_prefix("EXPORT_PROGRESS ")).map(|p| p.parse().unwrap()).collect();
        assert!(percents.windows(2).all(|w| w[0] < w[1]), "each percent once, going forward: {percents:?}");
        assert_eq!(percents.last(), Some(&100), "{text}");
        assert!(!started.dir.join("export").join("video.mp4").exists(), "the video is where --out said, not in the recording's folder");
        eprintln!("cli: {last}; {} progress lines", percents.len());
    }

    /// What an export costs per hour of recording, measured with this machine's real ffmpeg on a made-up
    /// session of a minute at a 1920 x 1248 display and a 720p camera: with the camera over the screen (the
    /// pictures are composed and encoded once) and without it (the screen's files are copied). Run it by
    /// hand: `cargo test --release -p AzReview --lib -- --ignored --nocapture an_export_is_timed`.
    #[test]
    #[ignore = "X-EDGE-C-2: a measurement, run by hand: cargo test --release -p AzReview --lib -- --ignored --nocapture an_export_is_timed"]
    fn an_export_is_timed_per_hour_of_recording() {
        let Some(rig) = rig_sized("timed", (1920, 1248), (1280, 720)) else { return };
        for (camera, what) in [(true, "the screen with the camera over it and the voice (the pictures encoded once)"), (false, "the screen and the voice alone (the pictures copied)")] {
            let started = record_blocks(&rig, 20, false, camera);
            let manifest = Manifest::parse(&fs::read_to_string(started.dir.join("manifest.json")).unwrap()).unwrap();
            let edl = identity_edl(&manifest, &[]);
            let plan = build_plan(&manifest, &edl, &started.dir, &rig.tmp.0.join(format!("work-{camera}")), &rig.tmp.0.join(format!("export/{camera}.mp4")), rig.caps).unwrap();
            let recorded = edl.duration_us() as f64 / 1e6;
            let t0 = std::time::Instant::now();
            let report = run(&plan, &rig.ffmpeg, &AtomicBool::new(false), &mut |_| {}).unwrap_or_else(|e| panic!("{e}"));
            let took = t0.elapsed().as_secs_f64();
            eprintln!(
                "export of {what}: {recorded:.0} s recorded, {} commands, {} bytes, made in {took:.1} s = {:.0} s ({:.1} min) per hour of recording, {:.1}x real time",
                plan.commands.len(),
                report.bytes,
                took / recorded * 3600.0,
                took / recorded * 60.0,
                recorded / took
            );
        }
    }

    /// The packets of the first video stream of `file`, and its codec, as ffprobe counts them.
    fn video_packets(rig: &Rig, file: &Path) -> (u64, String) {
        let o = Process::new(&rig.ffprobe)
            .args(["-v", "error", "-select_streams", "v:0", "-count_packets", "-show_entries", "stream=codec_name,nb_read_packets", "-of", "csv=p=0"])
            .arg(file)
            .output()
            .unwrap();
        let text = String::from_utf8_lossy(&o.stdout).trim().to_string();
        let mut parts = text.split(',');
        let (codec, n) = (parts.next().unwrap_or("").to_string(), parts.next().unwrap_or("0").parse().unwrap_or(0));
        (n, codec)
    }

    /// The screen of one run with the voice and no camera, exported with a real ffmpeg: ONE command, no
    /// encoder, and the pictures that come out are the pictures that went in (the same count of packets, the
    /// same codec), the voice AAC, the length of the session, a clean decode, a chapter for each file opened.
    ///
    /// Mutation: a copy plan that encodes the pictures again (the packet count of a re-encode of a replayed
    /// stream is the same, but the command would carry an encoder: the pure test of it catches that; here
    /// the file must decode and be the session's length).
    #[test]
    fn a_screen_alone_is_exported_by_copying_its_pictures() {
        let Some(rig) = rig("copy") else { return };
        let started = record(&rig, false, false);
        let manifest = Manifest::parse(&fs::read_to_string(started.dir.join("manifest.json")).unwrap()).unwrap();
        let files = file_changes(&fs::read_to_string(started.dir.join("events.jsonl")).unwrap());
        let edl = identity_edl(&manifest, &files);
        let none = Capabilities { videotoolbox: false, libx264: false, aac: rig.caps.aac, ..rig.caps };
        let plan = build_plan(&manifest, &edl, &started.dir, &rig.tmp.0.join("work"), &rig.tmp.0.join("export/copy.mp4"), none).expect("a straight copy needs no video encoder");
        assert_eq!(plan.commands.len(), 1, "{:?}", plan.commands.iter().map(|c| c.id.clone()).collect::<Vec<_>>());
        let started_at = std::time::Instant::now();
        let report = run(&plan, &rig.ffmpeg, &AtomicBool::new(false), &mut |_| {}).unwrap_or_else(|e| panic!("{e}"));
        let took = started_at.elapsed();
        let (packets, codec) = video_packets(&rig, &report.output);
        let screen_packets: u64 = manifest.tracks.screen.as_ref().unwrap().segments.iter().map(|s| u64::from(s.samples)).sum();
        assert_eq!((codec.as_str(), packets), ("h264", screen_packets), "the pictures that went in came out");
        let probe = |args: &[&str]| {
            let o = Process::new(&rig.ffprobe).args(["-v", "error"]).args(args).arg(&report.output).output().unwrap();
            String::from_utf8_lossy(&o.stdout).into_owned()
        };
        let streams = probe(&["-show_entries", "stream=codec_name,codec_type", "-of", "csv=p=0"]);
        assert!(streams.contains("h264") && streams.contains("aac"), "{streams}");
        let duration: f64 = probe(&["-show_entries", "format=duration", "-of", "csv=p=0"]).trim().parse().unwrap();
        assert!((duration - 6.0).abs() < 0.6, "{duration}");
        assert_eq!(probe(&["-show_chapters", "-of", "csv=p=0"]).lines().count(), 3, "a chapter for each file opened (three)");
        let decode = Process::new(&rig.ffmpeg).args(["-v", "error", "-i"]).arg(&report.output).args(["-f", "null", "-"]).output().unwrap();
        assert!(decode.status.success() && decode.stderr.is_empty(), "{}", String::from_utf8_lossy(&decode.stderr));
        eprintln!("straight copy: {packets} pictures copied, {} bytes, {duration:.2} s of video in {:.2} s", report.bytes, took.as_secs_f64());
    }

    /// A file of a picture and a voice in one (the camera's chunk with the voice in it), made from real H.264 and real
    /// Opus, is read by ffprobe as two streams and decodes clean, both to its end; two of them join with `-c copy`.
    ///
    /// Mutation: a voice track that ffprobe does not see; fragments that do not decode.
    #[test]
    fn a_file_of_a_picture_and_a_voice_is_read_by_ffprobe_as_two_streams_and_decodes_clean() {
        use crate::recorder_mp4::{annexb_to_avcc, JointWriter, Limits, OpusConfig, VideoConfig};
        let Some(rig) = rig("joint") else { return };
        let config = VideoConfig::from_keyframe(&rig.screen[0]).expect("the first access unit is a keyframe with its sets");
        let make = |name: &str, from: usize| -> PathBuf {
            let (mut writer, mut bytes) = JointWriter::new(config.clone(), OpusConfig::mono_48k(), 1_764_108_000, Limits::chunked(2_000_000));
            // The voice is a little ahead of the pictures, as a microphone's chunks are.
            let mut next_voice = 0_usize;
            for (i, au) in rig.screen.iter().enumerate().skip(from) {
                let at = (i - from) as u64 * 33_333;
                while next_voice < rig.voice.len() && (next_voice as u64 * 20_000) < at + 100_000 {
                    bytes.extend(writer.push_voice(next_voice as u64 * 20_000, rig.voice[next_voice].clone()).unwrap());
                    next_voice += 1;
                }
                let sample = annexb_to_avcc(au);
                bytes.extend(writer.push_video(at, sample.bytes, sample.keyframe).unwrap());
            }
            bytes.extend(writer.finish((rig.screen.len() - from) as u64 * 33_333));
            let path = rig.tmp.0.join(name);
            fs::write(&path, bytes).unwrap();
            path
        };
        let one = make("joint-1.mp4", 0);
        let probe = |args: &[&str], file: &Path| {
            let o = Process::new(&rig.ffprobe).args(["-v", "error"]).args(args).arg(file).output().unwrap();
            assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
            String::from_utf8_lossy(&o.stdout).into_owned()
        };
        let streams = probe(&["-show_entries", "stream=codec_name,codec_type", "-of", "csv=p=0"], &one);
        assert!(streams.contains("h264,video") && streams.contains("opus,audio"), "{streams}");
        for stream in ["v:0", "a:0"] {
            let o = Process::new(&rig.ffmpeg).args(["-v", "error", "-i"]).arg(&one).args(["-map", stream, "-f", "null", "-"]).output().unwrap();
            assert!(o.status.success() && o.stderr.is_empty(), "{stream} decodes clean: {}", String::from_utf8_lossy(&o.stderr));
        }
        let durations = probe(&["-show_entries", "stream=codec_type,duration", "-of", "csv=p=0"], &one);
        for line in durations.lines().filter(|l| l.contains("video") || l.contains("audio")) {
            let seconds: f64 = line.rsplit(',').next().unwrap().trim().parse().unwrap();
            assert!((seconds - 6.0).abs() < 0.5, "a stream of about six seconds: {line}");
        }
        // Two of them join without a new encoding, and the joined file has both streams too.
        let two = make("joint-2.mp4", 0);
        let list = rig.tmp.0.join("joint.txt");
        fs::write(&list, format!("file '{}'\nfile '{}'\n", one.display(), two.display())).unwrap();
        let joined = rig.tmp.0.join("joined.mp4");
        let o = Process::new(&rig.ffmpeg).args(["-v", "error", "-f", "concat", "-safe", "0", "-i"]).arg(&list).args(["-c", "copy", "-y"]).arg(&joined).output().unwrap();
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
        let streams = probe(&["-show_entries", "stream=codec_name,codec_type", "-of", "csv=p=0"], &joined);
        assert!(streams.contains("h264,video") && streams.contains("opus,audio"), "{streams}");
        eprintln!("a joint file of a picture and a voice: {} bytes, ffprobe: {}", fs::metadata(&one).unwrap().len(), streams.replace('\n', " "));
        // One small file may be kept to look at (AVFoundation, HandBrake), where the caller names a place for it.
        if let Ok(keep) = std::env::var("AZREVIEW_KEEP_JOINT") {
            fs::copy(&one, &keep).unwrap();
            eprintln!("kept: {keep}");
        }
    }

    /// The camera exported as `shape` (`rect` or `rounded`) at half the picture's width in the bottom right corner with
    /// no margin: 320 x 240 at (320, 120) of a 640 x 360 picture. What the plan said it would do differently, and the video.
    fn export_camera(rig: &Rig, started: &Started, manifest: &Manifest, shape: &str, caps: Capabilities) -> (Vec<String>, PathBuf) {
        let mut edl = identity_edl(manifest, &[]);
        edl.style.camera.shape = shape.to_string();
        edl.style.camera.scale = 0.5;
        edl.style.camera.margin = 0;
        let out = rig.tmp.0.join(format!("export/{shape}-{}.mp4", caps.rounding));
        let plan = build_plan(manifest, &edl, &started.dir, &rig.tmp.0.join(format!("work-{shape}-{}", caps.rounding)), &out, caps).unwrap();
        let report = run(&plan, &rig.ffmpeg, &AtomicBool::new(false), &mut |_| {}).unwrap_or_else(|e| panic!("{e}"));
        let decode = Process::new(&rig.ffmpeg).args(["-v", "error", "-i"]).arg(&report.output).args(["-f", "null", "-"]).output().unwrap();
        assert!(decode.status.success() && decode.stderr.is_empty(), "{shape}: {}", String::from_utf8_lossy(&decode.stderr));
        (plan.notes, report.output)
    }

    /// A camera with rounded corners goes through the real ffmpeg: the export decodes clean, the very corner of the
    /// camera's picture shows the SCREEN (it is the screen alone there, and the rectangular camera is not), and
    /// the middle of the camera is the same camera picture, colours and all, in both exports.
    #[test]
    fn a_camera_with_rounded_corners_is_exported_and_shows_the_screen_in_its_corners() {
        let Some(rig) = rig("round") else { return };
        if !rig.caps.rounding {
            skipped("the probe says this ffmpeg cannot round the camera (the export keeps a rectangle: the next test)");
            return;
        }
        let started = record(&rig, false, true);
        let manifest = Manifest::parse(&fs::read_to_string(started.dir.join("manifest.json")).unwrap()).unwrap();
        let ((notes_a, rect), (notes_b, rounded)) = (export_camera(&rig, &started, &manifest, "rect", rig.caps), export_camera(&rig, &started, &manifest, "rounded", rig.caps));
        assert!(notes_a.is_empty() && notes_b.is_empty(), "this ffmpeg rounds (the probe said so): {notes_a:?} {notes_b:?}");
        // The very corner of the picture is the camera's corner: cut away in the rounded one.
        let (w, h) = (640, 360);
        let (a, b) = (rgb_frame(&rig, &rect, "1.0", w, h), rgb_frame(&rig, &rounded, "1.0", w, h));
        let alone = rgb_frame(&rig, &started.dir.join("screen/0001.mp4"), "1.0", w, h);
        let corner = (w - 10, w, h - 10, h);
        let (rect_corner, rounded_corner) = (block_diff(&a, &alone, w, corner), block_diff(&b, &alone, w, corner));
        let middle = block_diff(&a, &b, w, (440, 520, 220, 300));
        eprintln!("rounded against rectangular camera: at the corner the screen alone differs from the rectangle by {rect_corner:.1} and from the rounded one by {rounded_corner:.1}; the middle of the two exports differs by {middle:.1}");
        assert!(rect_corner > 20.0, "the rectangle shows the camera where the screen is: {rect_corner}");
        // (Not the screen to the last pixel: an ffmpeg that seeks a frame off, or one that filters the chroma of the
        // cut's edge, leaves a few of the block's pixels not equal; the camera itself would be as far as the rectangle.)
        assert!(rounded_corner * 4.0 < rect_corner && rounded_corner < 15.0, "the rounded corner shows the screen: {rounded_corner} against the rectangle's {rect_corner}");
        assert!(middle < 8.0, "the rest of the camera is the camera, in its colours, in both: {middle}");
    }

    /// An ffmpeg that cannot round (told so, whatever this one can do): the export still happens, the camera is a
    /// rectangle exactly as the cut list's `rect` makes it, and the plan said so beforehand.
    ///
    /// Mutation: a plan that rounds where it was told it cannot; one that fails the export instead of keeping a rectangle.
    #[test]
    fn a_rounded_camera_on_an_ffmpeg_that_cannot_round_is_exported_as_a_rectangle_with_a_note() {
        let Some(rig) = rig("norounding") else { return };
        let started = record(&rig, false, true);
        let manifest = Manifest::parse(&fs::read_to_string(started.dir.join("manifest.json")).unwrap()).unwrap();
        let cannot = Capabilities { rounding: false, ..rig.caps };
        let (notes, fallback) = export_camera(&rig, &started, &manifest, "rounded", cannot);
        let (no_notes, rect) = export_camera(&rig, &started, &manifest, "rect", cannot);
        assert_eq!(notes.len(), 1, "{notes:?}");
        assert!(no_notes.is_empty(), "{no_notes:?}");
        let (w, h) = (640, 360);
        let (a, b) = (rgb_frame(&rig, &fallback, "1.0", w, h), rgb_frame(&rig, &rect, "1.0", w, h));
        let alone = rgb_frame(&rig, &started.dir.join("screen/0001.mp4"), "1.0", w, h);
        let corner = block_diff(&a, &alone, w, (w - 10, w, h - 10, h));
        let same = block_diff(&a, &b, w, (0, w, 0, h));
        eprintln!("rounded without the route: the camera's corner still differs from the screen by {corner:.1}; the whole picture differs from the rectangle's by {same:.2}");
        assert!(corner > 20.0, "the camera is a rectangle, its corner is the camera: {corner}");
        assert!(same < 1.0, "the fallback is the rectangle's picture: {same}");
    }

    /// One frame of `file` at `at` seconds as RGB bytes (all three channels: a picture that lost its colours is not
    /// the picture), `w` x `h`.
    fn rgb_frame(rig: &Rig, file: &Path, at: &str, w: usize, h: usize) -> Vec<u8> {
        let out = Process::new(&rig.ffmpeg)
            .args(["-v", "error", "-ss", at, "-i"])
            .arg(file)
            .args(["-frames:v", "1", "-vf", &format!("scale={w}:{h}"), "-pix_fmt", "rgb24", "-f", "rawvideo", "-"])
            .output()
            .unwrap();
        assert!(out.status.success() && out.stdout.len() == w * h * 3, "{}", String::from_utf8_lossy(&out.stderr));
        out.stdout
    }

    /// The mean difference per colour channel between two RGB frames inside the block `x0..x1` x `y0..y1`.
    fn block_diff(a: &[u8], b: &[u8], w: usize, (x0, x1, y0, y1): (usize, usize, usize, usize)) -> f64 {
        let mut sum = 0_u64;
        for y in y0..y1 {
            for x in x0 * 3..x1 * 3 {
                sum += u64::from(a[y * w * 3 + x].abs_diff(b[y * w * 3 + x]));
            }
        }
        sum as f64 / ((x1 - x0) * (y1 - y0) * 3) as f64
    }

    /// The command of RECORDER.md section 10, the block a person copies, run as it stands on a recorded
    /// session (the hardware encoder only where this ffmpeg has none: `libx264` is what is said for "elsewhere"):
    /// it makes one video with a picture and a voice of the session's length that decodes clean, and the
    /// camera is in the bottom right corner on top of the screen, the top left corner untouched.
    ///
    /// Mutation: the overlay at the top left instead of the bottom right; no `-map 2:a` (no voice).
    #[test]
    fn the_command_of_the_doc_combines_the_tracks_by_hand_into_a_valid_video() {
        let Some(rig) = rig("byhand") else { return };
        let started = record(&rig, false, true);
        let doc = include_str!("../RECORDER.md");
        let head = doc.find("### By hand: the screen with the camera in the bottom right corner and the voice").expect("the doc has the command");
        let from = doc[head..].find("```sh\n").map(|i| head + i + 6).expect("the command is a block");
        let to = doc[from..].find("```").map(|i| from + i).expect("the block ends");
        let mut script = doc[from..to].to_string();
        assert!(script.contains("h264_videotoolbox"), "the doc names the Mac's encoder");
        if !rig.caps.videotoolbox {
            script = script.replace("h264_videotoolbox -b:v 4M", "libx264 -preset veryfast -crf 23");
        }
        let path = format!("{}:{}", rig.ffmpeg.parent().unwrap().display(), std::env::var("PATH").unwrap_or_default());
        let out = Process::new("sh").arg("-c").arg(&script).env("SESSION", &started.dir).env("PATH", path).output().unwrap();
        assert!(out.status.success(), "the command failed: {}", String::from_utf8_lossy(&out.stderr).lines().rev().take(8).collect::<Vec<_>>().join("\n"));
        let combined = started.dir.join("combined.mp4");
        // A picture and a voice, six seconds.
        let probe = |args: &[&str]| {
            let o = Process::new(&rig.ffprobe).args(["-v", "error"]).args(args).arg(&combined).output().unwrap();
            assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
            String::from_utf8_lossy(&o.stdout).into_owned()
        };
        let streams = probe(&["-show_entries", "stream=codec_name,codec_type", "-of", "csv=p=0"]);
        assert!(streams.contains("h264") && streams.contains("aac"), "{streams}");
        let duration: f64 = probe(&["-show_entries", "format=duration", "-of", "csv=p=0"]).trim().parse().unwrap();
        assert!((duration - 6.0).abs() < 0.8, "six seconds recorded: {duration}");
        let decode = Process::new(&rig.ffmpeg).args(["-v", "error", "-i"]).arg(&combined).args(["-f", "null", "-"]).output().unwrap();
        assert!(decode.status.success() && decode.stderr.is_empty(), "{}", String::from_utf8_lossy(&decode.stderr));
        // Where the camera is: the bottom right block differs from the screen alone, the top left does not.
        let (w, h) = (640, 360);
        let with = rgb_frame(&rig, &combined, "1.0", w, h);
        let alone = rgb_frame(&rig, &started.dir.join("screen/0001.mp4"), "1.0", w, h);
        let bottom_right = block_diff(&with, &alone, w, (400, 600, 100, 330));
        let top_left = block_diff(&with, &alone, w, (10, 50, 10, 50));
        eprintln!("the doc's command: {} bytes, {duration:.2} s, streams {}; the camera's corner differs by {bottom_right:.1}, the top left by {top_left:.1}", fs::metadata(&combined).unwrap().len(), streams.replace('\n', " "));
        assert!(bottom_right > 12.0, "the camera is on the screen at the bottom right: {bottom_right}");
        assert!(top_left < 8.0, "the rest of the screen is the screen: {top_left}");
    }
}
