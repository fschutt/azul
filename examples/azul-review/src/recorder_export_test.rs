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

const ENC: Encoders = Encoders { videotoolbox: true, libx264: true, aac: true };

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
    // The camera at 22% of 1280 = 281.6, made even: 282 px wide; 24 px from the corner.
    assert!(f.contains("[2:v]setpts=PTS-STARTPTS,scale=282:-2,fps=30,tpad=stop_mode=clone:stop_duration=20.000000[cam]"), "{f}");
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
    assert!(f.contains("[2:v]setpts=PTS-STARTPTS+5.000000/TB,scale=282:-2"), "the camera comes in at 5 s: {f}");
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
    let sw = build_plan(&session(), &e, Path::new("/rec/s"), Path::new("/work"), Path::new("/out/video.mp4"), Encoders { videotoolbox: false, libx264: true, aac: true }).unwrap();
    assert!(args_of(&sw.commands[0]).contains("-c:v libx264 -preset veryfast -b:v 8000k"));
    let none = build_plan(&session(), &e, Path::new("/rec/s"), Path::new("/work"), Path::new("/out/video.mp4"), Encoders { videotoolbox: false, libx264: false, aac: true });
    assert!(none.unwrap_err().contains("no H.264 encoder"));
    let no_aac = build_plan(&session(), &e, Path::new("/rec/s"), Path::new("/work"), Path::new("/out/video.mp4"), Encoders { videotoolbox: true, libx264: true, aac: false });
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
    assert_eq!(parse_encoders(text), Encoders { videotoolbox: true, libx264: true, aac: true });
    assert_eq!(parse_encoders(" V....D mjpeg  MJPEG\n"), Encoders { videotoolbox: false, libx264: false, aac: false });
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
    }

    impl CodecFactory for ReplayFactory {
        fn video(&self, width: u32, _height: u32, _kbps: u32) -> Result<Box<dyn VideoCodec>, String> {
            let aus = if width == 640 { self.screen.clone() } else { self.camera.clone() };
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
        encoders: Encoders,
        screen: Vec<Vec<u8>>,
        camera: Vec<Vec<u8>>,
        voice: Vec<Vec<u8>>,
        tmp: Tmp,
    }

    /// The rig, or `None` (said, and a failure under `AZ_REQUIRE_FFMPEG=1`) where ffmpeg cannot do it.
    fn rig(name: &str) -> Option<Rig> {
        let (Some(ffmpeg), Some(ffprobe)) = (find_tool("ffmpeg"), find_tool("ffprobe")) else {
            skipped("there is no ffmpeg and ffprobe in /opt/homebrew/bin, /usr/local/bin or /usr/bin");
            return None;
        };
        let encoders = parse_encoders(&String::from_utf8_lossy(&Process::new(&ffmpeg).args(["-hide_banner", "-encoders"]).output().unwrap().stdout));
        if !(encoders.aac && (encoders.libx264 || encoders.videotoolbox)) {
            skipped("this ffmpeg lacks aac or an H.264 encoder");
            return None;
        }
        let h264 = |size: &str| ffmpeg_bytes(&ffmpeg, &["-f", "lavfi", "-i", &format!("testsrc2=size={size}:rate=30"), "-t", "6", "-c:v", "libx264", "-g", "30", "-bf", "0", "-pix_fmt", "yuv420p", "-f", "h264", "-"]);
        let opus = ffmpeg_bytes(&ffmpeg, &["-f", "lavfi", "-i", "sine=frequency=440:sample_rate=48000", "-t", "6", "-c:a", "libopus", "-b:a", "32k", "-frame_duration", "20", "-f", "ogg", "-"]);
        let (Some(screen), Some(camera), Some(opus)) = (h264("640x360"), h264("320x240"), opus) else {
            skipped("this ffmpeg cannot make the test streams (it needs libx264 and libopus)");
            return None;
        };
        eprintln!("ffmpeg: {}", String::from_utf8_lossy(&Process::new(&ffmpeg).arg("-version").output().unwrap().stdout).lines().next().unwrap_or(""));
        let tmp = Tmp(std::env::temp_dir().join(format!("azreview-e2e-{name}-{}", std::process::id())));
        let _ = fs::remove_dir_all(&tmp.0);
        fs::create_dir_all(&tmp.0).unwrap();
        Some(Rig { ffmpeg, ffprobe, encoders, screen: split_aus(&screen), camera: split_aus(&camera), voice: ogg_packets(&opus), tmp })
    }

    /// A session of six seconds recorded through the recorder with the rig's encoders: the window changes ten
    /// times a second, the camera sends 15 pictures a second and the voice 50 packets; three seconds, then (when
    /// `with_pause`) a pause of two seconds, then three seconds more.
    fn record(rig: &Rig, with_pause: bool) -> Started {
        let factory = ReplayFactory { screen: rig.screen.clone(), camera: rig.camera.clone(), voice: rig.voice.clone() };
        let clock = MockClock::at(0);
        let c = clock.clone();
        let deps = Deps {
            unix_ms: Box::new(|| 1_764_108_000_000),
            make_clock: Box::new(move |_| Arc::new(c.clone()) as Arc<dyn Clock>),
            free_bytes: Box::new(|_| Some(500_000_000_000)),
            factory: Box::new(factory),
        };
        let mut rec = Recorder::new(RecorderSettings { idle_pause_us: None, ..RecorderSettings::default() }, deps);
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
        let window = vec![9_u8; 640 * 360 * 4];
        let second = |rec: &mut Recorder, from_ms: u64, events: &[(u64, &str)]| {
            for i in 0..30_u64 {
                clock.set_us((from_ms + i * 100) * 1_000);
                for (at, file) in events.iter().filter(|(at, _)| *at == i) {
                    let _ = at;
                    rec.event("nav.file", &format!("{{\"file\":\"{file}\",\"lines\":100}}")).unwrap();
                }
                rec.tap(&TapFrame { width: 640, height: 360, stride: 640 * 4, scale: 1.0, damage: Damage::Full, pixels: &window });
                rec.camera_frame(RawFrame { width: 320, height: 240, format: PixelFormat::Nv12, bytes: vec![0; 320 * 240 * 3 / 2] });
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
            second(&mut rec, 3_000, &[(0, "README.md")]);
        }
        let done = rec.stop("user").unwrap();
        assert!(done.clean, "{done:?}");
        started
    }

    #[test]
    fn a_recorded_session_becomes_a_playable_video_with_chapters() {
        let Some(rig) = rig("export") else { return };
        let (ffmpeg, ffprobe, encoders, tmp) = (rig.ffmpeg.clone(), rig.ffprobe.clone(), rig.encoders, &rig.tmp);
        let started = record(&rig, true);

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

    /// One frame of `file` at `at` seconds as grey bytes, `w` x `h`.
    fn grey_frame(rig: &Rig, file: &Path, at: &str, w: usize, h: usize) -> Vec<u8> {
        let out = Process::new(&rig.ffmpeg)
            .args(["-v", "error", "-ss", at, "-i"])
            .arg(file)
            .args(["-frames:v", "1", "-vf", &format!("scale={w}:{h}"), "-pix_fmt", "gray", "-f", "rawvideo", "-"])
            .output()
            .unwrap();
        assert!(out.status.success() && out.stdout.len() == w * h, "{}", String::from_utf8_lossy(&out.stderr));
        out.stdout
    }

    /// The mean difference between two grey frames inside the block `x0..x1` x `y0..y1`.
    fn block_diff(a: &[u8], b: &[u8], w: usize, (x0, x1, y0, y1): (usize, usize, usize, usize)) -> f64 {
        let mut sum = 0_u64;
        for y in y0..y1 {
            for x in x0..x1 {
                sum += u64::from(a[y * w + x].abs_diff(b[y * w + x]));
            }
        }
        sum as f64 / ((x1 - x0) * (y1 - y0)) as f64
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
        let started = record(&rig, false);
        let doc = include_str!("../RECORDER.md");
        let head = doc.find("### By hand: the screen with the camera in the bottom right corner and the voice").expect("the doc has the command");
        let from = doc[head..].find("```sh\n").map(|i| head + i + 6).expect("the command is a block");
        let to = doc[from..].find("```").map(|i| from + i).expect("the block ends");
        let mut script = doc[from..to].to_string();
        assert!(script.contains("h264_videotoolbox"), "the doc names the Mac's encoder");
        if !rig.encoders.videotoolbox {
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
        let with = grey_frame(&rig, &combined, "1.0", w, h);
        let alone = grey_frame(&rig, &started.dir.join("screen/0001.mp4"), "1.0", w, h);
        let bottom_right = block_diff(&with, &alone, w, (400, 600, 100, 330));
        let top_left = block_diff(&with, &alone, w, (10, 50, 10, 50));
        eprintln!("the doc's command: {} bytes, {duration:.2} s, streams {}; the camera's corner differs by {bottom_right:.1}, the top left by {top_left:.1}", fs::metadata(&combined).unwrap().len(), streams.replace('\n', " "));
        assert!(bottom_right > 12.0, "the camera is on the screen at the bottom right: {bottom_right}");
        assert!(top_left < 8.0, "the rest of the screen is the screen: {top_left}");
    }
}
