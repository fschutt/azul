//! The renderer: a cut list and a session in, ffmpeg commands out, and the run of them.
//!
//! An export is planned as TEXT first ([`build_plan`], a pure function, tested as strings) and only then
//! run ([`run`]). Each segment of the cut list becomes one ffmpeg command that makes a piece
//! (`seg_<id>.mp4`): the background colour, the screen's slices scaled into the frame (cropped to the
//! zoom, if any), the camera's slices as a picture-in-picture in its corner, the voice's slices as the
//! sound, speed and gain applied. The pieces all have the same encoding, so one last command joins them
//! without re-encoding (`-c copy`) and gives the file its chapters and title.
//!
//! Why pieces and not one big filter graph: a session is hours long and a graph that opens every
//! segment file at once would hold them all open; a piece opens only the files its cut needs, a piece
//! that fails is named, and the pieces are the natural unit for the later versions' crossfades.
//!
//! The picture is composed at the recording's speed and the speed applied last. A track that began
//! late leads in with the background (the camera) or with silence; a track that ended early holds its
//! last picture (`tpad`), which is what a screen that did not change is. The captions of the later
//! versions are drawn by azul's CPU renderer as overlay pictures: this ffmpeg has no `drawtext` and no
//! `subtitles` filter.
//!
//! The hardware encoder (`h264_videotoolbox`) is used when this ffmpeg has it, else `libx264`.

use std::{
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    process::{Command as Process, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

use crate::{
    recorder_edl::{Edl, EdlSegment},
    recorder_manifest::{Manifest, Track},
};

/// What this ffmpeg can encode with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Encoders {
    /// `h264_videotoolbox`: the Mac's video hardware.
    pub videotoolbox: bool,
    /// `libx264`: software, everywhere.
    pub libx264: bool,
    /// The built-in `aac` encoder.
    pub aac: bool,
}

/// Reads the encoders out of `ffmpeg -encoders`.
#[must_use]
pub fn parse_encoders(text: &str) -> Encoders {
    let has = |name: &str| text.lines().any(|l| l.split_whitespace().nth(1) == Some(name));
    Encoders { videotoolbox: has("h264_videotoolbox"), libx264: has("libx264"), aac: has("aac") }
}

/// A piece of a track's file that feeds a cut.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Slice {
    /// The segment file.
    pub file: PathBuf,
    /// Where to start in the file's own time, microseconds.
    pub seek_us: u64,
    /// How much of it, microseconds.
    pub dur_us: u64,
    /// How long after the cut's start this slice's first moment is, microseconds: a track that began
    /// late, or the next file of the track.
    pub lead_us: u64,
}

/// The slices of `track` that cover `in_us..out_us` of the session, in order. A segment still being
/// written (no end) cannot be cut at and is left out.
#[must_use]
pub fn slices_for(track: &Track, session_dir: &Path, in_us: u64, out_us: u64) -> Vec<Slice> {
    let mut out = Vec::new();
    for seg in &track.segments {
        let Some(end) = seg.end_us else { continue };
        let (a, b) = (in_us.max(seg.start_us), out_us.min(end));
        if b <= a {
            continue;
        }
        out.push(Slice { file: session_dir.join(&seg.file), seek_us: a - seg.start_us, dur_us: b - a, lead_us: a - in_us });
    }
    out
}

/// Microseconds as ffmpeg's seconds: `12.345678`.
#[must_use]
pub fn secs(us: u64) -> String {
    format!("{}.{:06}", us / 1_000_000, us % 1_000_000)
}

/// One ffmpeg run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Command {
    /// The segment's id, or `join`.
    pub id: String,
    pub args: Vec<String>,
    /// The file it makes.
    pub output: PathBuf,
    /// How long the video it makes is, microseconds (for progress).
    pub us: u64,
}

/// Everything an export runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    /// The cuts in order, then the join.
    pub commands: Vec<Command>,
    /// The file the join reads and what is in it.
    pub concat_list: (PathBuf, String),
    /// More lists the commands read (a straight copy reads the voice's files from one), written the same way.
    pub extra_lists: Vec<(PathBuf, String)>,
    /// The chapters' file and what is in it.
    pub metadata: (PathBuf, String),
    /// The video.
    pub output: PathBuf,
    pub total_us: u64,
}

/// The list of pieces for ffmpeg's concat demuxer.
#[must_use]
pub fn concat_list(files: &[PathBuf]) -> String {
    let mut out = String::new();
    for f in files {
        out.push_str("file '");
        out.push_str(&f.to_string_lossy().replace('\'', "'\\''"));
        out.push_str("'\n");
    }
    out
}

fn meta_escape(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        if matches!(c, '=' | ';' | '#' | '\\' | '\n') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// The title and the chapters in ffmpeg's metadata file format.
#[must_use]
pub fn ffmetadata(edl: &Edl) -> String {
    let mut out = String::from(";FFMETADATA1\n");
    if let Some(t) = &edl.title {
        out.push_str(&format!("title={}\n", meta_escape(t)));
    }
    let total_ms = edl.duration_us() / 1_000;
    for (i, ch) in edl.chapters.iter().enumerate() {
        let start = ch.t_us / 1_000;
        let end = edl.chapters.get(i + 1).map_or(total_ms, |n| n.t_us / 1_000).max(start);
        out.push_str(&format!("[CHAPTER]\nTIMEBASE=1/1000\nSTART={start}\nEND={end}\ntitle={}\n", meta_escape(&ch.title)));
    }
    out
}

/// `atempo` takes 0.5 to 2 at a time: the chain that makes `speed`.
fn atempo_chain(speed: f64) -> String {
    let mut factors = Vec::new();
    let mut s = speed;
    while s > 2.0 {
        factors.push(2.0);
        s /= 2.0;
    }
    while s < 0.5 {
        factors.push(0.5);
        s /= 0.5;
    }
    if (s - 1.0).abs() > 1e-9 {
        factors.push(s);
    }
    factors.iter().map(|f| format!("atempo={f:.6}")).collect::<Vec<_>>().join(",")
}

/// The width of the camera's picture: a fraction of the frame, made even.
fn camera_width(frame_w: u32, scale: f64) -> u32 {
    let w = (f64::from(frame_w) * scale / 2.0).round() as u32 * 2;
    w.clamp(16, frame_w)
}

fn corner(c: &str, margin: u32) -> String {
    let m = margin;
    match c {
        "top_left" => format!("x={m}:y={m}"),
        "top_right" => format!("x=W-w-{m}:y={m}"),
        "bottom_left" => format!("x={m}:y=H-h-{m}"),
        _ => format!("x=W-w-{m}:y=H-h-{m}"),
    }
}

/// The filters that give the camera's picture rounded corners (nothing for a rectangle): it gets an alpha
/// channel and every pixel outside a quarter circle of radius a ninth of the width, in each corner, is made
/// transparent, so the overlay shows the screen there.
fn rounding(shape: &str, width: u32) -> String {
    if shape != "rounded" {
        return String::new();
    }
    let r = (f64::from(width) / 9.0).round().max(2.0) as u32;
    format!(
        ",format=yuva420p,geq=lum='p(X,Y)':cb='p(X,Y)':cr='p(X,Y)':a='if(gt(abs(W/2-X),W/2-{r})*gt(abs(H/2-Y),H/2-{r}),if(lte(hypot(abs(W/2-X)-(W/2-{r}),abs(H/2-Y)-(H/2-{r})),{r}),255,0),255)'"
    )
}

/// `#rrggbb` as ffmpeg writes a colour.
fn colour(hex: &str) -> String {
    format!("0x{}", hex.trim_start_matches('#'))
}

fn encoder_args(enc: Encoders, kbps: u32) -> Result<Vec<String>, String> {
    let words: Vec<String> = if enc.videotoolbox {
        vec![String::from("-c:v"), String::from("h264_videotoolbox"), String::from("-b:v"), format!("{kbps}k")]
    } else if enc.libx264 {
        vec![String::from("-c:v"), String::from("libx264"), String::from("-preset"), String::from("veryfast"), String::from("-b:v"), format!("{kbps}k")]
    } else {
        return Err(String::from("no H.264 encoder in this ffmpeg (h264_videotoolbox or libx264 is needed)"));
    };
    Ok(words)
}

/// The size of the file a slice reads, from the manifest, for the check that one cut does not span a
/// resize of the window.
fn size_of(track: &Track, file: &Path, session_dir: &Path) -> (u32, u32) {
    track
        .segments
        .iter()
        .find(|s| session_dir.join(&s.file) == file)
        .map_or((track.width, track.height), |s| if s.width > 0 { (s.width, s.height) } else { (track.width, track.height) })
}

/// The input arguments and the filter pieces for one track's slices: `(input arguments, the label the
/// slices are joined into, the filter that makes it)`.
fn track_inputs(slices: &[Slice], first_input: usize, video: bool, label: &str) -> (Vec<String>, String, Option<String>) {
    let mut args = Vec::new();
    for s in slices {
        args.extend(["-ss".to_string(), secs(s.seek_us), "-t".to_string(), secs(s.dur_us), "-i".to_string(), s.file.to_string_lossy().into_owned()]);
    }
    let kind = if video { 'v' } else { 'a' };
    if slices.len() == 1 {
        return (args, format!("[{first_input}:{kind}]"), None);
    }
    let inputs: String = (0..slices.len()).map(|i| format!("[{}:{kind}]", first_input + i)).collect();
    let joined = format!("[{label}]");
    let (v, a) = if video { (1, 0) } else { (0, 1) };
    (args, joined.clone(), Some(format!("{inputs}concat=n={}:v={v}:a={a}{joined}", slices.len())))
}

fn segment_command(m: &Manifest, e: &Edl, seg: &EdlSegment, session_dir: &Path, work_dir: &Path, enc: Encoders) -> Result<Command, String> {
    let o = &e.output;
    let rec_us = seg.out_us - seg.in_us;
    let out_us = seg.duration_us();
    let fps = o.fps;

    let screen = m.tracks.screen.as_ref().map(|t| (t, slices_for(t, session_dir, seg.in_us, seg.out_us)));
    let Some((screen_track, screen_slices)) = screen.filter(|(_, s)| !s.is_empty()) else {
        return Err(format!(
            "segment {:?}: the screen track recorded nothing between {} s and {} s",
            seg.id,
            secs(seg.in_us),
            secs(seg.out_us)
        ));
    };
    let sizes: Vec<(u32, u32)> = screen_slices.iter().map(|s| size_of(screen_track, &s.file, session_dir)).collect();
    if sizes.windows(2).any(|w| w[0] != w[1]) {
        return Err(format!("segment {:?} spans a resize of the window: cut the list at the resize", seg.id));
    }
    let camera_slices = if seg.camera { m.tracks.camera.as_ref().map(|t| slices_for(t, session_dir, seg.in_us, seg.out_us)).filter(|s| !s.is_empty()) } else { None };
    let audio_slices = m.tracks.audio.as_ref().map(|t| slices_for(t, session_dir, seg.in_us, seg.out_us)).filter(|s| !s.is_empty());

    let mut args: Vec<String> = ["-hide_banner", "-nostdin", "-y"].iter().map(ToString::to_string).collect();
    args.extend(["-f".to_string(), "lavfi".to_string(), "-i".to_string(), format!("color=c={}:s={}x{}:r={}:d={}", colour(&e.style.background), o.width, o.height, fps, secs(rec_us))]);
    let mut filters: Vec<String> = Vec::new();
    let mut next = 1;

    // The screen.
    let (a, screen_label, join) = track_inputs(&screen_slices, next, true, "screen_raw");
    args.extend(a);
    next += screen_slices.len();
    filters.extend(join);
    let lead = screen_slices[0].lead_us;
    let mut chain = format!("{screen_label}setpts=PTS-STARTPTS");
    if lead > 0 {
        chain.push_str(&format!("+{}/TB", secs(lead)));
    }
    if let Some(z) = &seg.zoom {
        chain.push_str(&format!(",crop={}:{}:{}:{}", z.w, z.h, z.x, z.y));
    }
    if e.style.screen.fit == "cover" {
        chain.push_str(&format!(",scale={}:{}:force_original_aspect_ratio=increase,crop={}:{}", o.width, o.height, o.width, o.height));
    } else {
        chain.push_str(&format!(",scale={}:{}:force_original_aspect_ratio=decrease", o.width, o.height));
    }
    chain.push_str(&format!(",setsar=1,fps={fps},tpad=stop_mode=clone:stop_duration={}[screen]", secs(rec_us)));
    filters.push(chain);
    let after_screen = if camera_slices.is_some() { "b1" } else if (seg.speed - 1.0).abs() > 1e-9 { "vc" } else { "v" };
    filters.push(format!("[0:v][screen]overlay=x=(W-w)/2:y=(H-h)/2:eof_action=pass[{after_screen}]"));

    // The camera.
    if let Some(slices) = &camera_slices {
        let (a, label, join) = track_inputs(slices, next, true, "camera_raw");
        args.extend(a);
        next += slices.len();
        filters.extend(join);
        let lead = slices[0].lead_us;
        let mut chain = format!("{label}setpts=PTS-STARTPTS");
        if lead > 0 {
            chain.push_str(&format!("+{}/TB", secs(lead)));
        }
        let w = camera_width(o.width, e.style.camera.scale);
        chain.push_str(&format!(",scale={w}:-2{},fps={fps},tpad=stop_mode=clone:stop_duration={}[cam]", rounding(&e.style.camera.shape, w), secs(rec_us)));
        filters.push(chain);
        let last = if (seg.speed - 1.0).abs() > 1e-9 { "vc" } else { "v" };
        filters.push(format!("[b1][cam]overlay={}:eof_action=pass[{last}]", corner(&e.style.camera.corner, e.style.camera.margin)));
    }
    if (seg.speed - 1.0).abs() > 1e-9 {
        filters.push(format!("[vc]setpts=PTS/{:.6}[v]", seg.speed));
    }

    // The sound.
    let gain = format!("volume={:.3}dB", seg.audio.gain_db);
    let tempo = if (seg.speed - 1.0).abs() > 1e-9 { format!(",{}", atempo_chain(seg.speed)) } else { String::new() };
    if let Some(slices) = &audio_slices {
        let (a, label, join) = track_inputs(slices, next, false, "audio_raw");
        args.extend(a);
        filters.extend(join);
        let delay = if slices[0].lead_us > 0 { format!(",adelay={0}|{0}", slices[0].lead_us / 1_000) } else { String::new() };
        filters.push(format!("{label}aresample=48000{delay},{gain}{tempo}[a]"));
    } else {
        args.extend(["-f".to_string(), "lavfi".to_string(), "-i".to_string(), String::from("anullsrc=r=48000:cl=mono")]);
        filters.push(format!("[{next}:a]atrim=duration={},aresample=48000,{gain}{tempo}[a]", secs(rec_us)));
    }

    let output = work_dir.join(format!("seg_{}.mp4", seg.id));
    args.extend(["-filter_complex".to_string(), filters.join(";"), "-map".to_string(), "[v]".to_string(), "-map".to_string(), "[a]".to_string(), "-t".to_string(), secs(out_us)]);
    args.extend(encoder_args(enc, o.video_kbps)?);
    args.extend(["-pix_fmt", "yuv420p", "-c:a", "aac", "-b:a"].iter().map(ToString::to_string));
    args.extend([format!("{}k", o.audio_kbps), "-ar".to_string(), "48000".to_string(), "-movflags".to_string(), "+faststart".to_string()]);
    args.push(output.to_string_lossy().into_owned());
    Ok(Command { id: seg.id.clone(), args, output, us: out_us })
}

/// The plan of a cut list that asks for nothing the recorded pictures cannot give as they are: ONE segment,
/// no camera over it, no zoom, no speed, no caption, a cut that begins and ends where recorded files do and
/// a screen that is the output's own size in every file. Then the screen's files are joined with `-c:v copy`
/// (no encoder is needed and the pictures are not touched: an hour is a matter of seconds) and the voice's
/// files, which are Opus, become AAC. `None` when the cut list asks for more: the cuts are then made one by
/// one ([`build_plan`]).
///
/// # Errors
/// There is a voice and this ffmpeg has no AAC encoder.
fn straight_copy(m: &Manifest, e: &Edl, session_dir: &Path, work_dir: &Path, out_file: &Path, enc: Encoders) -> Result<Option<Plan>, String> {
    let [seg] = e.segments.as_slice() else { return Ok(None) };
    if seg.camera || seg.zoom.is_some() || (seg.speed - 1.0).abs() > 1e-9 || seg.caption.is_some() || seg.title.is_some() {
        return Ok(None);
    }
    let Some(screen) = m.tracks.screen.as_ref() else { return Ok(None) };
    let slices = slices_for(screen, session_dir, seg.in_us, seg.out_us);
    let Some(first) = slices.first() else { return Ok(None) };
    // The files are taken whole, so each slice must be a whole file (to within a tenth of a second), the first
    // may begin its picture a little late (a screen that did not change begins with its first change), and
    // every file must be the output's size.
    let whole = |sl: &Slice| {
        screen.segments.iter().find(|s| session_dir.join(&s.file) == sl.file).and_then(|s| s.end_us.map(|end| (s.start_us, end))).is_some_and(
            |(start, end)| sl.seek_us <= 100_000 && (end - start).abs_diff(sl.seek_us + sl.dur_us) <= 100_000,
        )
    };
    // ... and no file may follow another after a gap (a screen that went away and came back): the joined
    // files would close it up and the voice would run ahead of the picture.
    let contiguous = slices.windows(2).all(|w| w[1].lead_us.saturating_sub(w[0].lead_us + w[0].dur_us) <= 100_000);
    if first.lead_us > 200_000 || !contiguous || !slices.iter().all(whole) {
        return Ok(None);
    }
    if slices.iter().any(|sl| size_of(screen, &sl.file, session_dir) != (e.output.width, e.output.height)) {
        return Ok(None);
    }
    let voice = m.tracks.audio.as_ref().map(|a| slices_for(a, session_dir, seg.in_us, seg.out_us)).filter(|v| !v.is_empty());
    if voice.is_some() && !enc.aac {
        return Err(String::from("no AAC encoder in this ffmpeg"));
    }
    let screen_list = work_dir.join("screen_pieces.txt");
    let voice_list = work_dir.join("audio_pieces.txt");
    let meta_path = work_dir.join("chapters.ffmeta");
    let mut args: Vec<String> = ["-hide_banner", "-nostdin", "-y", "-f", "concat", "-safe", "0", "-i"].iter().map(ToString::to_string).collect();
    args.push(screen_list.to_string_lossy().into_owned());
    let mut extra_lists = Vec::new();
    let mut inputs = 1;
    if let Some(v) = &voice {
        args.extend(["-f", "concat", "-safe", "0", "-i"].iter().map(ToString::to_string));
        args.push(voice_list.to_string_lossy().into_owned());
        let files: Vec<PathBuf> = v.iter().map(|sl| sl.file.clone()).collect();
        extra_lists.push((voice_list, concat_list(&files)));
        inputs = 2;
    }
    args.extend(["-i".to_string(), meta_path.to_string_lossy().into_owned(), "-map".to_string(), "0:v".to_string()]);
    if voice.is_some() {
        args.extend(["-map".to_string(), "1:a".to_string()]);
    }
    args.extend(["-map_metadata".to_string(), inputs.to_string(), "-map_chapters".to_string(), inputs.to_string(), "-c:v".to_string(), "copy".to_string()]);
    if let Some(v) = &voice {
        let delay = if v[0].lead_us > 0 { format!(",adelay={0}|{0}", v[0].lead_us / 1_000) } else { String::new() };
        let gain = if seg.audio.gain_db.abs() > 1e-9 { format!(",volume={:.3}dB", seg.audio.gain_db) } else { String::new() };
        args.extend(["-af".to_string(), format!("aresample=48000{delay}{gain}")]);
        args.extend(["-c:a", "aac", "-b:a"].iter().map(ToString::to_string));
        args.extend([format!("{}k", e.output.audio_kbps), "-ar".to_string(), "48000".to_string()]);
    }
    args.extend(["-movflags".to_string(), "+faststart".to_string(), out_file.to_string_lossy().into_owned()]);
    let files: Vec<PathBuf> = slices.iter().map(|sl| sl.file.clone()).collect();
    let total_us = e.duration_us();
    Ok(Some(Plan {
        commands: vec![Command { id: String::from("join"), args, output: out_file.to_path_buf(), us: total_us }],
        concat_list: (screen_list, concat_list(&files)),
        extra_lists,
        metadata: (meta_path, ffmetadata(e)),
        output: out_file.to_path_buf(),
        total_us,
    }))
}

/// The plan of an export of `edl` for the session in `session_dir`: the pieces are made in `work_dir`
/// and the video is `out_file`.
///
/// # Errors
/// Why no plan can be made: no encoder, or a cut where the screen recorded nothing or spans a resize.
pub fn build_plan(m: &Manifest, e: &Edl, session_dir: &Path, work_dir: &Path, out_file: &Path, enc: Encoders) -> Result<Plan, String> {
    // The screen alone, of one run, at its own size: its pictures are joined, not encoded again.
    if let Some(plan) = straight_copy(m, e, session_dir, work_dir, out_file, enc)? {
        return Ok(plan);
    }
    encoder_args(enc, e.output.video_kbps)?;
    if !enc.aac {
        return Err(String::from("no AAC encoder in this ffmpeg"));
    }
    let mut commands = Vec::new();
    let mut pieces = Vec::new();
    for seg in &e.segments {
        let c = segment_command(m, e, seg, session_dir, work_dir, enc)?;
        pieces.push(c.output.clone());
        commands.push(c);
    }
    let list_path = work_dir.join("pieces.txt");
    let meta_path = work_dir.join("chapters.ffmeta");
    let join: Vec<String> = [
        "-hide_banner", "-nostdin", "-y", "-f", "concat", "-safe", "0", "-i",
    ]
    .iter()
    .map(ToString::to_string)
    .chain([
        list_path.to_string_lossy().into_owned(),
        "-i".to_string(),
        meta_path.to_string_lossy().into_owned(),
        "-map".to_string(), "0".to_string(),
        "-map_metadata".to_string(), "1".to_string(),
        "-map_chapters".to_string(), "1".to_string(),
        "-c".to_string(), "copy".to_string(),
        "-movflags".to_string(), "+faststart".to_string(),
        out_file.to_string_lossy().into_owned(),
    ])
    .collect();
    let total_us = e.duration_us();
    commands.push(Command { id: String::from("join"), args: join, output: out_file.to_path_buf(), us: total_us });
    Ok(Plan {
        commands,
        concat_list: (list_path, concat_list(&pieces)),
        extra_lists: Vec::new(),
        metadata: (meta_path, ffmetadata(e)),
        output: out_file.to_path_buf(),
        total_us,
    })
}

// ---------------------------------------------------------------------------
// Running
// ---------------------------------------------------------------------------

/// How far an export is.
#[derive(Debug, Clone, PartialEq)]
pub struct Progress {
    /// The command running (from 1) and how many there are.
    pub step: usize,
    pub steps: usize,
    pub id: String,
    /// The whole export, 0 to 1.
    pub fraction: f64,
}

/// What an export made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    pub output: PathBuf,
    pub bytes: u64,
}

/// Why an export did not finish.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExportError {
    /// It was cancelled.
    Cancelled,
    /// A file could not be made or ffmpeg could not be started: the system's words.
    Io(String),
    /// ffmpeg stopped with an error: which command, its exit status and the end of what it said.
    Failed { step: String, status: Option<i32>, stderr_tail: String },
}

impl std::fmt::Display for ExportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cancelled => f.write_str("the export was cancelled"),
            Self::Io(e) => write!(f, "the export could not run: {e}"),
            Self::Failed { step, status, stderr_tail } => {
                write!(f, "ffmpeg failed at {step} (exit {}): {stderr_tail}", status.map_or_else(|| String::from("signal"), |s| s.to_string()))
            }
        }
    }
}

impl std::error::Error for ExportError {}

/// The seconds in a `time=00:01:02.50` of ffmpeg's progress line.
fn parse_time(line: &str) -> Option<f64> {
    let at = line.rfind("time=")?;
    let t = line[at + 5..].split_whitespace().next()?;
    let mut parts = t.split(':');
    let (h, m, s) = (parts.next()?.parse::<f64>().ok()?, parts.next()?.parse::<f64>().ok()?, parts.next()?.parse::<f64>().ok()?);
    Some(h * 3600.0 + m * 60.0 + s)
}

/// Runs the plan with `ffmpeg`: writes the join's two files, runs each command in order, reports the
/// progress, and removes the pieces at the end. `cancel` stops it between two looks (50 ms). A failed
/// or cancelled export leaves no half-made video.
///
/// # Errors
/// Cancelled, a file or the process could not be made, or ffmpeg failed.
pub fn run(plan: &Plan, ffmpeg: &Path, cancel: &AtomicBool, progress: &mut dyn FnMut(Progress)) -> Result<Report, ExportError> {
    let io = |e: std::io::Error| ExportError::Io(e.to_string());
    if let Some(dir) = plan.concat_list.0.parent() {
        std::fs::create_dir_all(dir).map_err(io)?;
    }
    if let Some(dir) = plan.output.parent() {
        std::fs::create_dir_all(dir).map_err(io)?;
    }
    std::fs::write(&plan.concat_list.0, &plan.concat_list.1).map_err(io)?;
    for (path, text) in &plan.extra_lists {
        std::fs::write(path, text).map_err(io)?;
    }
    std::fs::write(&plan.metadata.0, &plan.metadata.1).map_err(io)?;
    let steps = plan.commands.len();
    // The share of the whole each command is: by the video it makes, the join as much as the last cut.
    let weight_total: u64 = plan.commands.iter().map(|c| c.us.max(1)).sum();
    let mut done: u64 = 0;
    let cleanup = |plan: &Plan| {
        for c in &plan.commands {
            if c.id != "join" {
                let _ = std::fs::remove_file(&c.output);
            }
        }
        let _ = std::fs::remove_file(&plan.concat_list.0);
        for (path, _) in &plan.extra_lists {
            let _ = std::fs::remove_file(path);
        }
        let _ = std::fs::remove_file(&plan.metadata.0);
    };
    for (i, c) in plan.commands.iter().enumerate() {
        progress(Progress { step: i + 1, steps, id: c.id.clone(), fraction: done as f64 / weight_total as f64 });
        let mut child = Process::new(ffmpeg).args(&c.args).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::piped()).spawn().map_err(|e| ExportError::Io(format!("{}: {e}", ffmpeg.display())))?;
        let tail = Arc::new(Mutex::new(std::collections::VecDeque::<String>::new()));
        let seen = Arc::new(Mutex::new(0.0_f64));
        let reader = child.stderr.take().map(|stderr| {
            let (tail, seen) = (Arc::clone(&tail), Arc::clone(&seen));
            std::thread::spawn(move || {
                let mut r = BufReader::new(stderr);
                let mut buf = Vec::new();
                // ffmpeg ends progress lines with a carriage return, the others with a newline.
                while r.read_until(b'\n', &mut buf).map(|n| n > 0).unwrap_or(false) {
                    for line in String::from_utf8_lossy(&buf).split(['\r', '\n']).filter(|l| !l.trim().is_empty()) {
                        if let Some(t) = parse_time(line) {
                            if let Ok(mut s) = seen.lock() {
                                *s = t;
                            }
                        }
                        if let Ok(mut t) = tail.lock() {
                            t.push_back(line.trim().to_string());
                            while t.len() > 30 {
                                t.pop_front();
                            }
                        }
                    }
                    buf.clear();
                }
            })
        });
        let status = loop {
            if cancel.load(Ordering::Relaxed) {
                let _ = child.kill();
                let _ = child.wait();
                let _ = std::fs::remove_file(&c.output);
                cleanup(plan);
                return Err(ExportError::Cancelled);
            }
            match child.try_wait().map_err(io)? {
                Some(s) => break s,
                None => {
                    let t = seen.lock().map_or(0.0, |s| *s);
                    let part = (t * 1_000_000.0).min(c.us as f64) as u64;
                    progress(Progress { step: i + 1, steps, id: c.id.clone(), fraction: (done + part) as f64 / weight_total as f64 });
                    std::thread::sleep(Duration::from_millis(50));
                }
            }
        };
        if let Some(r) = reader {
            let _ = r.join();
        }
        if !status.success() {
            let tail_text = tail.lock().map(|t| t.iter().cloned().collect::<Vec<_>>().join(" | ")).unwrap_or_default();
            let _ = std::fs::remove_file(&c.output);
            cleanup(plan);
            return Err(ExportError::Failed { step: c.id.clone(), status: status.code(), stderr_tail: tail_text });
        }
        done += c.us.max(1);
    }
    cleanup(plan);
    progress(Progress { step: steps, steps, id: String::from("join"), fraction: 1.0 });
    let bytes = std::fs::metadata(&plan.output).map_err(io)?.len();
    Ok(Report { output: plan.output.clone(), bytes })
}

#[cfg(test)]
#[path = "recorder_export_test.rs"]
mod recorder_export_test;
