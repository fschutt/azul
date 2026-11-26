//! The debug server's op that says what AzReview's state is (the pen and the nib in hand, the panels, the number of
//! conversations): for the scripts that drive a window and cannot read an icon. It exists only where the debug server
//! does.

use azul::{error::CustomE2eOpResult, json::Json, prelude::*, str::String as AzString};

use crate::AppState;

/// The op's name.
pub const OP_NAME: &str = "azreview_state";

/// The whiteboard of the workspace: whether its panel is open and its board read, what is on the board (the cutouts with the
/// lines they come from and whether their picture is drawn), the loops that wait to be taken (where the grip is on their page,
/// how much time they have left), what a press on one is doing, the lines a jump lit, and what the board last said.
fn whiteboard_json(s: &AppState) -> serde_json::Value {
    let Ok(g) = s.wb.try_borrow() else {
        return serde_json::Value::Null;
    };
    let now = g.panel.now_ms();
    let rect = |r: &crate::whiteboard_geom::Rect| serde_json::json!({"x": r.x, "y": r.y, "w": r.w, "h": r.h});
    let (board_id, board_name) = g.panel.wb.active_board();
    serde_json::json!({
        "open": s.panels.right_open,
        "started": g.started,
        "keys": g.keys,
        "board": {"id": board_id, "name": board_name, "boards": g.panel.wb.boards().boards().count()},
        "items": g.panel.wb.board.len(),
        "cutouts": g.panel.wb.board.cutouts().map(|c| serde_json::json!({
            "id": c.id,
            "file": c.source.file,
            "first": c.source.first_line,
            "last": c.source.last_line,
            "rect": rect(&c.rect),
            "picture": match &c.image {
                crate::whiteboard::ImageSlot::Ready(_) => "ready",
                crate::whiteboard::ImageSlot::Pending => "pending",
                crate::whiteboard::ImageSlot::Failed(_) => "failed",
            },
        })).collect::<Vec<_>>(),
        "armed": g.panel.wb.armed.all().iter().map(|r| serde_json::json!({
            "id": r.id,
            "page": r.page,
            "first": r.first_line,
            "last": r.last_line,
            "grip": rect(&r.grip),
            "bounds": rect(&r.bounds),
            "left_ms": r.expires_at_ms().saturating_sub(now),
            "lifetime_ms": r.lifetime_ms,
        })).collect::<Vec<_>>(),
        "armed_ms": g.panel.wb.armed.lifetime_ms(),
        "press": {"active": g.panel.wb.drag.is_active(), "carrying": g.panel.wb.drag.ghost().is_some()},
        "flash": g.flash.as_ref().map(|f| serde_json::json!({"file": f.file, "first": f.first, "last": f.last})),
        "hint": g.panel.hint,
        "tip_shown": g.tip_shown,
        "tip_for": g.tip_for,
    })
}

/// The state as JSON: `pen` (the place of the pen in hand), `meaning`, `nib`, `tree_open`, `tree_w`, `right_open`,
/// `right_w`, `chips` (the open file's conversations), `mic` (the microphone's state), `dictating` (a dictation runs),
/// `ready` (its microphone takes words), `partial` (the words so far), `progress` (the pages seen of the open file, whether it is reviewed, the workspace's totals), `tree` (the rows, the files and folders known so far, the file open), `strokes` (how many the pen has drawn), `slashes` and `deleted_words` (what quick slashes took), `standby` (the policy: `mode`, `enabled`, `armed`, `arms`,
/// `disarms`), `symbols` (the index of the workspace: `enabled`, `running`, how many names were `recent`ly found, the scan's `progress`), `speech` (the session's state while a thread makes it: `session` unasked, making, ready or failed; `waiting`: a dictation waits for it; `failure`), `whiteboard` (`open`, `started`, `board`, `cutouts`, `armed` loops with their `grip`, `press`, `flash`, `hint`), `conversations` (`open`, `bar_all`, `notes`: each note's id, page, phase, turns and what the agent made of it; `agents`: how many run, how many answers wait, the backend).
#[must_use]
pub fn state_json(s: &AppState) -> String {
    serde_json::json!({
        "pen": s.pens.selected(),
        "meaning": s.ink_meaning().label(),
        "nib": s.tool().label(),
        "tree_open": s.panels.tree_open,
        "tree_w": s.panels.tree_w,
        "right_open": s.panels.right_open,
        "right_w": s.panels.right_w,
        "chips": s.open_chips().len(),
        "mic": s.voice.mic.name(),
        "dictating": s.voice.running.is_some(),
        "ready": s.voice.is_ready(),
        "partial": s.voice.partial(),
        "tree": {
            "rows": s.rows.len(),
            "files": s.tree.file_count(),
            "folders": s.tree.dir_count(),
            "open": s.file().map(|f| f.display.clone()),
            "visible": s.rows.iter().take(200).map(|r| serde_json::json!({"id": r.id, "name": r.name})).collect::<Vec<_>>(),
        },
        "progress": progress_json(s),
        "visible_page": s.visible_page,
        "strokes": s.strokes.len(),
        "slashes": s.slashes.len(),
        "deleted_words": s.slashes.iter().map(|d| d.words.len()).sum::<usize>(),
        "standby": {
            "mode": s.voice.arming.config().standby.name(),
            "enabled": s.voice.can_arm,
            "armed": s.voice.arming.is_armed(),
            "arms": s.voice.arms,
            "disarms": s.voice.disarms,
        },
        "whiteboard": whiteboard_json(s),
        "conversations": {
            "open": s.open_conv,
            "bar_all": s.bar_all,
            "notes": s.notes.iter().map(|n| serde_json::json!({
                "id": n.id,
                "page": n.page,
                "phase": n.talk.phase.name(),
                "asked": n.talk.asked,
                "pending": n.talk.pending(),
                "accepted": n.talk.accepted().count(),
                "logged": n.talk.logged,
                "patch": n.talk.patch,
                "turns": n.talk.turns.len(),
                "said": n.talk.said.len(),
                "heard_as": n.talk.heard_as,
                "problem": n.talk.problem,
            })).collect::<Vec<_>>(),
            "agents": {
                "running": s.hub.running(),
                "late": s.hub.late.len(),
                "auto_ask": s.hub.auto_ask,
                "backend": s.hub.settings.task(crate::agent_backend::Task::Editing).backend.key(),
            },
        },
        "symbols": {
            "enabled": s.symbols_enabled,
            "running": s.symbols.is_running(),
            "recent": s.symbols.recent_len(),
            "progress": s.symbols.progress().map(|p| serde_json::json!({
                "found": p.files_found,
                "done": p.files_done,
                "dirs": p.dirs_done,
                "walking": p.walking,
                "idle": p.idle,
            })),
        },
        "speech": {
            "session": s.voice.gate.state().name(),
            "waiting": s.voice.gate.is_waiting(),
            "failure": s.voice.gate.failure(),
        },
    })
    .to_string()
}

/// The op that lists and opens every folder of the tree (for the scripts that measure a whole workspace).
pub const OP_EXPAND_ALL: &str = "azreview_expand_all";

/// What has been looked at: the open file's pages seen and whether it is reviewed, and the workspace's totals.
fn progress_json(s: &AppState) -> serde_json::Value {
    let open = s.file().map(|f| (f.display.clone(), f.page_count()));
    let (seen, pages, reviewed, held) = open.as_ref().map_or((0, 0, false, false), |(path, pages)| {
        let entry = s.progress.get(path);
        let id = s.tree.find(path);
        (
            entry.map_or(0, |e| e.seen.count()),
            *pages,
            id.is_some_and(|id| s.totals_of(id).files_reviewed == 1),
            entry.is_some_and(|e| e.held),
        )
    });
    let w = s.totals_of(s.tree.root());
    serde_json::json!({
        "back": s.progress.back().map(|b| serde_json::json!({"file": b.file, "page": b.page, "line": b.line})),
        "seen_pages": seen,
        "pages": pages,
        "reviewed": reviewed,
        "held": held,
        "counting": s.counting,
        "workspace": {
            "files": w.files,
            "files_reviewed": w.files_reviewed,
            "lines": w.lines,
            "lines_reviewed": w.lines_reviewed,
            "lines_seen": w.lines_seen,
            "unknown_files": w.unknown_files,
        },
    })
}

/// Adds the debug server's ops to `config`.
pub fn register_ops(config: &mut AppConfig) {
    let def = format!(
        "{{\"name\":\"{OP_EXPAND_ALL}\",\"summary\":\"list and open every folder of the tree\",\
         \"description\":\"On the window's thread: how many folders were listed, how long it took (ms), and the tree's \
         files, folders and rows afterwards.\",\"args\":[],\"examples\":[]}}"
    );
    let json = Json::parse(AzString::from(def.as_str()))
        .into_result()
        .unwrap_or_else(|_| Json::null());
    config.add_e2e_command(AzString::from(OP_EXPAND_ALL), json, on_expand_all);
    let def = format!(
        "{{\"name\":\"{OP_NAME}\",\"summary\":\"what AzReview's state is\",\
         \"description\":\"pen, meaning, nib, tree_open, tree_w, right_open, right_w, chips, mic, dictating, ready, partial, progress, tree, strokes, slashes, deleted_words, standby, symbols, speech, conversations.\",\"args\":[],\"examples\":[]}}"
    );
    let json = Json::parse(AzString::from(def.as_str()))
        .into_result()
        .unwrap_or_else(|_| Json::null());
    config.add_e2e_command(AzString::from(OP_NAME), json, on_op);
}

extern "C" fn on_expand_all(mut data: RefAny, mut info: CallbackInfo, name: AzString, _args: AzString) -> CustomE2eOpResult {
    if name.as_str() != OP_EXPAND_ALL {
        return CustomE2eOpResult { handled: false, json: AzString::from("") };
    }
    let started = std::time::Instant::now();
    let (listed, files, folders, rows) = data.downcast_mut::<AppState>().map_or((0, 0, 0, 0), |mut s| {
        let listed = s.expand_all();
        (listed, s.tree.file_count(), s.tree.dir_count(), s.rows.len())
    });
    let ms = started.elapsed().as_millis();
    // what building the totals of the whole tree costs (they are built again when what they sum changes)
    let rolled = std::time::Instant::now();
    if let Some(mut s) = data.downcast_mut::<AppState>() {
        s.refresh_rollup();
    }
    let rollup_us = rolled.elapsed().as_micros();
    info.trigger_all_virtual_view_rerender();
    CustomE2eOpResult {
        handled: true,
        json: AzString::from(
            format!(
                "{{\"success\":true,\"listed\":{listed},\"ms\":{ms},\"rollup_us\":{rollup_us},\"files\":{files},\"folders\":{folders},\"rows\":{rows}}}"
            )
            .as_str(),
        ),
    }
}

extern "C" fn on_op(mut data: RefAny, _info: CallbackInfo, name: AzString, _args: AzString) -> CustomE2eOpResult {
    if name.as_str() != OP_NAME {
        return CustomE2eOpResult { handled: false, json: AzString::from("") };
    }
    let state = data.downcast_ref::<AppState>().map_or_else(|| "{}".to_string(), |s| state_json(&s));
    CustomE2eOpResult {
        handled: true,
        json: AzString::from(format!("{{\"success\":true,\"state\":{state}}}").as_str()),
    }
}

#[cfg(test)]
#[path = "state_op_test.rs"]
mod state_op_test;
