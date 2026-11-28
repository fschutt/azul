//! The side bar's SOURCE CONTROL, the activity bar's third icon (VSCode's): the branch of the
//! workspace's folder and the changes git lists for it - Staged Changes, then Changes - each a row
//! with the file's name, its folder and the letter of what happened to it; a click opens the file.
//! Where there is no repository, or no `git` (a phone has none), the view says so instead of
//! showing an empty list.
//!
//! The changes are asked of `git status` on a Thread ([`crate::commands::refresh_git`]): when the
//! view is shown, after a save, and by the Refresh button.

use azul::{
    dom::{AccessibilityInfo, AccessibilityRole},
    prelude::*,
};
use azul_appkit::l10n::{self, t, t_args, Arg};

use crate::{
    app::AppState,
    commands,
    git::{self, Change, ChangeKind, Status},
    ids, phone,
    ui::{self, column, label, side_title},
    workspace::{file_name, parent_folder},
};

/// A row's height on a desktop, px.
pub const ROW_HEIGHT: f32 = 22.0;

/// What a row acts on: the app and the file's workspace key (none for a file outside the workspace's
/// folder, a deleted one or a folder).
struct ChangeRef {
    app: RefAny,
    key: Option<String>,
}

/// The colour of a change's letter.
const fn letter_colour(kind: ChangeKind) -> &'static str {
    match kind {
        ChangeKind::Modified => "#e2c08d",
        ChangeKind::Added | ChangeKind::Untracked | ChangeKind::Renamed | ChangeKind::Copied => "#73c991",
        ChangeKind::Deleted => "#c74e39",
        ChangeKind::Conflicted => "#e4676b",
    }
}

/// The workspace key a click on `change` opens: none for a deleted file, a folder (an untracked folder
/// is listed as `dir/`) or a file outside the workspace's folder (`prefix` is the folder's place in
/// the repository).
#[must_use]
pub fn openable_key(change: &Change, prefix: &str) -> Option<String> {
    if change.kind == ChangeKind::Deleted || change.path.ends_with('/') {
        return None;
    }
    git::workspace_key(&change.path, prefix).map(str::to_string)
}

/// The source control view: its title row (with Refresh), then what git said.
#[must_use]
pub fn git_panel(app: &RefAny, st: &AppState) -> Dom {
    let compact = phone::is_compact(st.window.0, st.window.1);
    let button = if compact { phone::TOUCH } else { 22.0 };
    let actions = vec![ui::icon_button_sized(
        ids::GIT_REFRESH,
        "refresh",
        "azcode-action-refresh-git",
        app.clone(),
        on_refresh,
        button,
    )];
    let body = body(app, st, compact);
    column(vec![side_title(&t("azcode-action-git").to_uppercase(), actions), body]).with_id(ids::GIT_PANEL)
}

/// One line in the middle of the view.
fn note(key: &str) -> Dom {
    label(key, "flex-shrink: 0; font-size: 13px; padding: 8px 20px 12px 20px; white-space: normal;").with_id(ids::GIT_EMPTY)
}

/// What the view shows for the state of git's answer.
fn body(app: &RefAny, st: &AppState, compact: bool) -> Dom {
    if st.workspace.is_none() {
        return note("azcode-git-no-folder");
    }
    match &st.git.status {
        None if st.git.loading => note("azcode-git-loading"),
        // A workspace git was never asked about: the sample, which is no repository.
        None | Some(Status::NoRepository) => note("azcode-git-no-repository"),
        Some(Status::Unavailable(_)) => note("azcode-git-unavailable"),
        Some(Status::Repository { branch, changes, more, prefix }) => repository(app, branch.as_deref(), changes, *more, prefix, compact),
    }
}

/// The branch, then the groups.
fn repository(app: &RefAny, branch: Option<&str>, changes: &[Change], more: usize, prefix: &str, compact: bool) -> Dom {
    let height = if compact { phone::TOUCH } else { ROW_HEIGHT };
    let staged: Vec<(usize, &Change)> = changes.iter().enumerate().filter(|(_, c)| c.staged).collect();
    let unstaged: Vec<(usize, &Change)> = changes.iter().enumerate().filter(|(_, c)| !c.staged).collect();
    let mut list = Dom::create_div()
        .with_css("display: flex; flex-direction: column; flex-grow: 1; min-height: 0px; overflow-y: auto;")
        .with_accessibility_info(AccessibilityInfo::named(l10n::label("azcode-action-git"), AccessibilityRole::List))
        .with_child(
            Dom::create_div()
                .with_css(format!(
                    "display: flex; flex-direction: row; align-items: center; height: {height}px; flex-shrink: 0; \
                     padding: 0px 12px 0px 16px; font-size: 13px;"
                ))
                .with_child(Dom::create_icon("call_split").with_css(format!("font-size: 16px; padding-right: 6px; {}", ui::SOFT_GLYPH)))
                .with_child(label(branch.unwrap_or("-"), "font-weight: 700;"))
                .with_child(label(
                    &t_args("azcode-git-changes", &[("count", Arg::from(changes.len() + more))]),
                    "flex-grow: 1; text-align: right; font-size: 12px; opacity: 0.7;",
                )),
        );
    if changes.is_empty() {
        list.add_child(label("azcode-git-clean", "flex-shrink: 0; font-size: 13px; padding: 4px 20px 12px 20px;"));
        return list;
    }
    for (title, group) in [("azcode-git-staged", staged), ("azcode-git-unstaged", unstaged)] {
        if group.is_empty() {
            continue;
        }
        list.add_child(label(
            &t_args(title, &[("count", Arg::from(group.len()))]).to_uppercase(),
            "flex-shrink: 0; font-size: 11px; font-weight: 700; padding: 8px 16px 4px 16px; opacity: 0.8;",
        ));
        for (i, change) in group {
            list.add_child(change_row(app, i, change, prefix, height));
        }
    }
    if more > 0 {
        list.add_child(label(
            &t_args("azcode-git-more", &[("count", Arg::from(more))]),
            "flex-shrink: 0; font-size: 12px; opacity: 0.7; padding: 6px 16px;",
        ));
    }
    list
}

/// One change: its file's name, its folder, the letter of what happened.
fn change_row(app: &RefAny, index: usize, change: &Change, prefix: &str, height: f32) -> Dom {
    let name = file_name(change.path.trim_end_matches('/')).to_string();
    let folder = parent_folder(change.path.trim_end_matches('/')).unwrap_or("").to_string();
    let key = openable_key(change, prefix);
    let openable = key.is_some();
    ui::clickable(
        ids::change(index),
        &format!(
            "display: flex; flex-direction: row; align-items: center; min-height: {height}px; flex-shrink: 0; \
             padding: 0px 12px 0px 16px; font-size: 13px; white-space: nowrap; overflow: hidden; {}",
            if openable {
                "cursor: pointer; :hover { background: rgba(128, 128, 128, 0.15); }"
            } else {
                "opacity: 0.6;"
            }
        ),
        &format!("{name}, {}", t(change.kind.name())),
        AccessibilityRole::ListItem,
        RefAny::new(ChangeRef { app: app.clone(), key }),
        on_change,
    )
    .with_child(Dom::create_icon("description").with_css(format!("font-size: 16px; padding-right: 6px; flex-shrink: 0; {}", ui::SOFT_GLYPH)))
    .with_child(label(&name, "flex-shrink: 0; padding-right: 8px;"))
    .with_child(label(&folder, "min-width: 0px; overflow: hidden; font-size: 12px; opacity: 0.6; flex-grow: 1;"))
    .with_child(label(
        change.kind.letter(),
        &format!("flex-shrink: 0; padding-left: 8px; font-size: 12px; font-weight: 700; color: {};", letter_colour(change.kind)),
    ))
}

extern "C" fn on_change(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, key)) = data.downcast_ref::<ChangeRef>().map(|r| (r.app.clone(), r.key.clone())) else {
        return Update::DoNothing;
    };
    let Some(key) = key else {
        return Update::DoNothing;
    };
    ui::with_state(&mut app, &mut info, |st, info, app| {
        commands::open_file(st, info, app, &key);
        st.phone.drawer = false;
    })
}

extern "C" fn on_refresh(mut data: RefAny, mut info: CallbackInfo) -> Update {
    ui::with_state(&mut data, &mut info, commands::refresh_git)
}

#[cfg(test)]
#[path = "git_panel_test.rs"]
mod git_panel_test;
