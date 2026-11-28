//! The side bar's RUN, the activity bar's fourth icon: what the workspace's folder says can be run -
//! the cargo commands of a `Cargo.toml`, the `scripts` of a `package.json`, the targets of a
//! `Makefile` ([`crate::tasks`]). A click types the task's command into the terminal panel's shell
//! (one is started when there is none) and runs it, so its output, its colours and its prompts are
//! the terminal's. Where no shell can run (an iPhone) the view says so.

use azul::{
    dom::{AccessibilityInfo, AccessibilityRole},
    prelude::*,
};
use azul_appkit::l10n::{self, t, t_args, Arg};

use crate::{
    app::AppState,
    ids, phone,
    tasks::{Source, Task},
    terminal,
    ui::{self, column, label, side_title},
};

/// A row's height on a desktop, px.
pub const ROW_HEIGHT: f32 = 24.0;

/// What a row acts on: the app and the task's index in the state's list.
struct RunRef {
    app: RefAny,
    index: usize,
}

/// The run view: its title row, then the tasks grouped by where they come from.
#[must_use]
pub fn run_panel(app: &RefAny, st: &AppState) -> Dom {
    let compact = phone::is_compact(st.window.0, st.window.1);
    column(vec![side_title(&t("azcode-action-run").to_uppercase(), Vec::new()), body(app, st, compact)]).with_id(ids::RUN_PANEL)
}

/// One line in the middle of the view.
fn note(key: &str) -> Dom {
    label(key, "flex-shrink: 0; font-size: 13px; padding: 8px 20px 12px 20px; white-space: normal;").with_id(ids::RUN_EMPTY)
}

/// What the view shows for the state of the workspace and the device.
fn body(app: &RefAny, st: &AppState, compact: bool) -> Dom {
    if st.workspace.is_none() {
        return note("azcode-run-no-folder");
    }
    if !terminal::AVAILABLE {
        return note("azcode-run-no-terminal");
    }
    if !st.tasks.loaded || st.tasks.tasks.is_empty() {
        return note(if st.tasks.loaded { "azcode-run-none" } else { "azcode-run-loading" });
    }
    let height = if compact { phone::TOUCH } else { ROW_HEIGHT };
    let mut list = Dom::create_div()
        .with_css("display: flex; flex-direction: column; flex-grow: 1; min-height: 0px; overflow-y: auto;")
        .with_accessibility_info(AccessibilityInfo::named(l10n::label("azcode-action-run"), AccessibilityRole::List));
    for source in [Source::Cargo, Source::Npm, Source::Make] {
        let rows: Vec<(usize, &Task)> = st.tasks.tasks.iter().enumerate().filter(|(_, task)| task.source == source).collect();
        if rows.is_empty() {
            continue;
        }
        list.add_child(label(
            &t_args("azcode-task-group", &[("source", Arg::from(t(source.name()))), ("file", Arg::from(source.file()))]).to_uppercase(),
            "flex-shrink: 0; font-size: 11px; font-weight: 700; padding: 10px 16px 4px 16px; opacity: 0.8;",
        ));
        for (index, task) in rows {
            list.add_child(task_row(app, index, task, height));
        }
    }
    list
}

/// One task: a play icon, its label, the command it types.
fn task_row(app: &RefAny, index: usize, task: &Task, height: f32) -> Dom {
    ui::clickable(
        ids::task(index),
        &format!(
            "display: flex; flex-direction: row; align-items: center; min-height: {height}px; flex-shrink: 0; \
             padding: 0px 12px 0px 16px; font-size: 13px; white-space: nowrap; overflow: hidden; cursor: pointer; \
             :hover {{ background: rgba(128, 128, 128, 0.15); }}"
        ),
        &t_args("azcode-run-task", &[("command", Arg::from(task.command.as_str()))]),
        AccessibilityRole::ListItem,
        RefAny::new(RunRef { app: app.clone(), index }),
        on_task,
    )
    .with_child(Dom::create_icon("play_arrow").with_css(format!("font-size: 18px; padding-right: 6px; flex-shrink: 0; {}", ui::SOFT_GLYPH)))
    .with_child(label(&task.label, "flex-shrink: 0; padding-right: 8px;"))
    .with_child(label(&task.command, "min-width: 0px; overflow: hidden; font-size: 12px; opacity: 0.6; font-family: monospace;"))
}

extern "C" fn on_task(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, index)) = data.downcast_ref::<RunRef>().map(|r| (r.app.clone(), r.index)) else {
        return Update::DoNothing;
    };
    ui::with_state(&mut app, &mut info, |st, info, app| {
        let Some(command) = st.tasks.tasks.get(index).map(|t| t.command.clone()) else {
            return;
        };
        terminal::run_command(st, info, app, &command);
        // On a phone the drawer gives way to the panel the task runs in.
        st.phone.drawer = false;
    })
}
