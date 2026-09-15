use std::path::PathBuf;

use azul::{prelude::*, task::TerminateTimer, time::SystemTimeDiff};

pub mod code;
pub mod ink;
pub mod model;
pub mod session;
pub mod ui;

use model::{Finding, Semantic, Stroke, Tool, VoiceClip};

pub(crate) fn scratch_dir() -> PathBuf {
    std::env::var("AZ_REVIEW_DIR")
        .map_or_else(|_| std::env::temp_dir().join("azreview"), PathBuf::from)
}

pub struct AppState {
    pub files: Vec<code::SourceFile>,
    pub current: Option<usize>,
    pub strokes: Vec<Stroke>,
    pub live: Option<Stroke>,
    pub next_stroke_id: u64,
    pub active: Semantic,
    pub tool: Tool,
    pub findings: Vec<Finding>,
    pub recording: Option<VoiceClip>,
    pub clips: Vec<VoiceClip>,
    pub level_samples: usize,
    pub visible_page: usize,
    pub epoch: u64,
    pub idle_timer: TimerId,
    pub root: PathBuf,
    pub status: String,
    pub last_pad_keys: u32,
}

impl AppState {
    pub(crate) fn file(&self) -> Option<&code::SourceFile> {
        self.current.and_then(|i| self.files.get(i))
    }

    fn rederive(&mut self) {
        let Some(file) = self.file().cloned() else {
            self.findings.clear();
            return;
        };
        self.findings = derive_findings(&self.strokes, &file, &self.recording);
    }
}

fn derive_findings(
    strokes: &[Stroke],
    file: &code::SourceFile,
    recording: &Option<VoiceClip>,
) -> Vec<Finding> {
    let mut out: Vec<Finding> = Vec::new();
    for s in strokes {
        let (_, y0, _, y1) = s.bounds();
        let (first_line, _) = file.page(s.page);
        let l0 = first_line + (y0 / ui::LINE_H).floor().max(0.0) as usize;
        let l1 = first_line + (y1 / ui::LINE_H).floor().max(0.0) as usize;

        let merged = out.iter_mut().find(|f| {
            f.epoch == s.epoch
                && f.semantic == s.semantic
                && l0 <= f.last_line.saturating_add(2)
                && l1.saturating_add(2) >= f.first_line
        });
        if let Some(f) = merged {
            f.first_line = f.first_line.min(l0);
            f.last_line = f.last_line.max(l1);
            f.stroke_count += 1;
            continue;
        }
        let voice = recording
            .as_ref()
            .filter(|c| c.stroke_ids.contains(&s.id))
            .map(|c| format!("{} samples of spoken rationale", c.samples.len()));
        out.push(Finding {
            semantic: s.semantic,
            file: file.display.clone(),
            first_line: l0,
            last_line: l1,
            voice_note: voice,
            stroke_count: 1,
            epoch: s.epoch,
        });
    }
    out.sort_by_key(|f| f.first_line);
    out
}

pub fn run() {
    let root = std::env::args().nth(1).map_or_else(
        || std::env::current_dir().unwrap_or_default(),
        PathBuf::from,
    );
    let files = code::load_tree(&root, 400);
    let status = if files.is_empty() {
        format!("no reviewable files under {}", root.display())
    } else {
        format!("{} files — pick one to start", files.len())
    };
    let state = AppState {
        current: if files.is_empty() { None } else { Some(0) },
        files,
        strokes: Vec::new(),
        live: None,
        next_stroke_id: 1,
        active: Semantic::Scope,
        tool: Tool::Marker,
        findings: Vec::new(),
        recording: None,
        clips: Vec::new(),
        level_samples: 0,
        visible_page: 0,
        epoch: 0,
        idle_timer: TimerId::unique(),
        root,
        status,
        last_pad_keys: 0,
    };
    let data = RefAny::new(state);
    let app = App::create(data, AppConfig::create());
    let mut window = WindowCreateOptions::create(ui::layout);
    window.window_state.title = "AzReview".into();
    app.run(window);
}

pub extern "C" fn on_ink_down(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let ms = info.get_current_mouse_state();
    if ms.right_down || ms.middle_down {
        return Update::DoNothing;
    }
    let Some(page) = ui::page_of(&mut info) else {
        return Update::DoNothing;
    };
    let Some(mut s) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    let Some((p, is_eraser)) = ui::sample(&mut info) else {
        return Update::DoNothing;
    };
    if is_eraser {
        let hit = s.strokes.iter().rposition(|st| {
            let (x0, y0, x1, y1) = st.bounds();
            st.page == page
                && p.x >= x0 - 6.0
                && p.x <= x1 + 6.0
                && p.y >= y0 - 6.0
                && p.y <= y1 + 6.0
        });
        if let Some(i) = hit {
            s.strokes.remove(i);
            s.rederive();
            return Update::RefreshDom;
        }
        return Update::DoNothing;
    }
    let id = s.next_stroke_id;
    s.next_stroke_id += 1;
    let semantic = s.tool.semantic_for(s.active);
    let epoch = s.epoch;
    s.live = Some(Stroke {
        page,
        semantic,
        points: vec![p],
        id,
        epoch,
    });

    if s.tool.records_audio() && s.recording.is_none() {
        s.recording = Some(VoiceClip {
            sample_rate: 48_000,
            ..VoiceClip::default()
        });
    }
    if let Some(clip) = s.recording.as_mut() {
        clip.stroke_ids.push(id);
    }
    Update::DoNothing
}

pub extern "C" fn on_cycle_tool(mut data: RefAny, _: CallbackInfo) -> Update {
    cycle_tool(&mut data, false)
}

pub extern "C" fn on_cycle_tool_back(mut data: RefAny, _: CallbackInfo) -> Update {
    if std::env::var("AZ_REVIEW_DEBUG").is_ok() {
        eprintln!("[review] on_cycle_tool_back FIRED");
    }
    cycle_tool(&mut data, true)
}

fn cycle_tool(data: &mut RefAny, backward: bool) -> Update {
    let Some(mut s) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    s.tool = if backward {
        s.tool.prev()
    } else {
        s.tool.next()
    };
    if !s.tool.records_audio() {
        if let Some(clip) = s.recording.take() {
            s.clips.push(clip);
        }
    }
    s.status = format!("{} - {}", s.tool.label(), s.active.label());
    Update::RefreshDom
}

pub extern "C" fn on_ink_move(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(mut s) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    if s.live.is_none() {
        return Update::DoNothing;
    }
    let Some((p, _)) = ui::sample(&mut info) else {
        return Update::DoNothing;
    };
    if let Some(live) = s.live.as_mut() {
        let far = live
            .points
            .last()
            .is_none_or(|l| (l.x - p.x).abs() + (l.y - p.y).abs() > 0.35);
        if far {
            live.points.push(p);
        }
    }
    Update::RefreshDom
}

pub extern "C" fn on_ink_up(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let (is_click, timer_id) = {
        let Some(mut s) = data.downcast_mut::<AppState>() else {
            return Update::DoNothing;
        };
        let Some(done) = s.live.take() else {
            return Update::DoNothing;
        };
        if done.points.len() < CLICK_POINT_LIMIT {
            (true, s.idle_timer)
        } else {
            s.strokes.push(done);
            s.rederive();
            session::save(&s);
            (false, s.idle_timer)
        }
    };
    if is_click {
        return on_cycle_tool(data, info);
    }
    arm_idle_timer(&mut info, data, timer_id);
    Update::RefreshDom
}

const ANNOTATION_IDLE_MS: u64 = 1_800;

fn arm_idle_timer(info: &mut CallbackInfo, data: RefAny, id: TimerId) {
    info.remove_timer(id);
    let timer = Timer::create(
        data,
        TimerCallback {
            cb: on_annotation_idle,
            ctx: OptionRefAny::None,
        },
        info.get_system_time_fn(),
    )
    .with_delay(Duration::System(SystemTimeDiff::from_millis(
        ANNOTATION_IDLE_MS,
    )));
    info.add_timer(id, timer);
}

pub extern "C" fn on_annotation_idle(
    mut data: RefAny,
    _: TimerCallbackInfo,
) -> TimerCallbackReturn {
    let Some(mut s) = data.downcast_mut::<AppState>() else {
        return TimerCallbackReturn {
            should_update: Update::DoNothing,
            should_terminate: TerminateTimer::Terminate,
        };
    };
    s.epoch += 1;
    if let Some(clip) = s.recording.take() {
        s.clips.push(clip);
        s.level_samples = 0;
    }
    s.status = format!("annotation {} sealed", s.epoch);
    session::save(&s);
    TimerCallbackReturn {
        should_update: Update::RefreshDom,
        should_terminate: TerminateTimer::Terminate,
    }
}

const CLICK_POINT_LIMIT: usize = 3;

pub extern "C" fn on_pad(mut data: RefAny, info: CallbackInfo) -> Update {
    let Some(pad) = info.get_tablet_pad().into_option() else {
        return Update::DoNothing;
    };
    let Some(mut s) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    let pressed = pad.express_keys & !s.last_pad_keys;
    s.last_pad_keys = pad.express_keys;
    if pressed == 0 {
        return Update::DoNothing;
    }
    let index = pressed.trailing_zeros() as usize;
    if let Some(&sem) = Semantic::ALL.get(index) {
        s.active = sem;
        s.status = format!("pad key {index} -> {}", sem.label());
        return Update::RefreshDom;
    }
    Update::DoNothing
}

pub extern "C" fn on_jump_to_page(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(page) = ui::index_of(&mut info) else {
        return Update::DoNothing;
    };
    ui::scroll_to_page(&mut info, page);
    let Some(mut s) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    s.visible_page = page;
    Update::RefreshDom
}

pub extern "C" fn on_menu_semantic(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(tag) = data.downcast_ref::<ui::IndexTag>().map(|t| t.index) else {
        return Update::DoNothing;
    };
    let Some(&sem) = Semantic::ALL.get(tag) else {
        return Update::DoNothing;
    };
    let Some(mut app) = info.get_dataset(info.get_hit_node()).into_option() else {
        return Update::DoNothing;
    };
    let Some(mut s) = app.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    s.active = sem;
    Update::RefreshDom
}

pub extern "C" fn on_menu_save(mut data: RefAny, _: CallbackInfo) -> Update {
    let Some(mut s) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    s.status = if session::save(&s) {
        format!("saved to {}", scratch_dir().display())
    } else {
        "save FAILED".to_string()
    };
    Update::RefreshDom
}

pub extern "C" fn on_menu_reveal(mut data: RefAny, _: CallbackInfo) -> Update {
    let dir = scratch_dir();
    let _ = std::fs::create_dir_all(&dir);
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(&dir).spawn();
    #[cfg(target_os = "linux")]
    let _ = std::process::Command::new("xdg-open").arg(&dir).spawn();
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("explorer").arg(&dir).spawn();

    let Some(mut s) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    s.status = format!("archives in {}", dir.display());
    Update::RefreshDom
}

pub extern "C" fn on_pick_semantic(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(index) = ui::index_of(&mut info) else {
        return Update::DoNothing;
    };
    let Some(mut s) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    if let Some(&sem) = Semantic::ALL.get(index) {
        s.active = sem;
    }
    Update::RefreshDom
}

pub extern "C" fn on_pick_file(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(index) = ui::index_of(&mut info) else {
        return Update::DoNothing;
    };
    let Some(mut s) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    if index >= s.files.len() {
        return Update::DoNothing;
    }
    session::save(&s);
    s.current = Some(index);
    s.strokes.clear();
    s.live = None;
    s.rederive();
    s.status = s.files[index].display.clone();
    Update::RefreshDom
}

pub extern "C" fn on_toggle_record(mut data: RefAny, _: CallbackInfo) -> Update {
    let Some(mut s) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    if let Some(clip) = s.recording.take() {
        s.status = format!("recording stopped - {} samples kept", clip.samples.len());
        s.clips.push(clip);
        s.level_samples = 0;
        session::save(&s);
    } else {
        s.recording = Some(VoiceClip {
            sample_rate: 48_000,
            ..VoiceClip::default()
        });
        s.status = "recording - strokes drawn now carry this audio".to_string();
    }
    Update::RefreshDom
}

pub extern "C" fn on_audio_frame(
    mut data: RefAny,
    _: CallbackInfo,
    frame: azul::audio::AudioFrame,
) -> Update {
    let Some(mut s) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    let Some(clip) = s.recording.as_mut() else {
        return Update::DoNothing;
    };
    clip.samples.extend(frame.samples.as_ref().iter().copied());
    let total = clip.samples.len();

    let step = ui::METER_PACKET_SAMPLES;
    if total / step > s.level_samples / step {
        s.level_samples = total;
        return Update::RefreshDom;
    }
    s.level_samples = total;
    Update::DoNothing
}
