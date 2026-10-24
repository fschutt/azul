//! The address row's branch chip and its quick switcher.
//!
//! When the window shows a folder on this computer, a worker thread asks `git_repo` which
//! repository holds it; the chip at the end of the address row then names the branch (or the
//! commit of a detached HEAD), a dot when something is not committed, and how far the branch
//! is from its upstream. A cloud folder never gets a chip. The chip is looked at again when the
//! folder changes, after F5, after a command of the command bar, and when the repository's
//! files change (a one-second watch: HEAD, the index and the refs by their stamps; the work
//! tree's edits through azcloud-kit's watcher on macOS, where FSEvents watches a tree in one
//! stream, and elsewhere by looking again every ten seconds while a look is quick - inotify
//! would need a watch per folder, an ignored `node_modules` included).
//!
//! A click on the chip (or Cmd/Ctrl+B, or `/switch`) opens the quick switcher: azul's Dialog
//! as a popover under the chip, its filter field taking the keyboard, the local branches
//! below it (the current one first, then the newest), arrows to choose, Return to switch,
//! Escape to close. A switch is checked first on a worker thread (`git_repo::plan_switch`): a
//! clean tree switches at once; changes git would carry are said, with "Switch anyway (carry
//! changes)"; changes git would refuse (or that could not be checked) are said and nothing
//! switches. The switch itself is `git switch <branch>` in the command bar, so its output
//! shows: gix's checkout does not carry local changes the way git does.
//!
//! The switcher's controls run in the dialog's own window: what they change rebuilds every
//! window, and what must outlive the dialog (the command's run and its timer) starts from
//! the dialog's close event, which runs in the app's window.

use std::path::PathBuf;
use std::time::Duration as StdDuration;

use azul::{
    callbacks::{
        ButtonOnClickCallbackType, DialogOnCloseCallbackType, TextInputOnTextInputCallbackType,
        TextInputOnVirtualKeyDownCallbackType, TimerCallbackInfo, TimerCallbackReturn,
    },
    dom::{AttributeType, DomId, DomNodeId, FocusTarget, VirtualKeyCode},
    misc::TransientAnchor,
    prelude::*,
    str::String as AzString,
    task::{Timer, TimerId},
    time::{Duration, SystemTimeDiff},
    widgets::{
        Button, ButtonType, Dialog, DialogClosedBy, DialogState, OnTextInputReturn,
        TextInputState, TextInputValid,
    },
};
use azul_appkit::l10n::{label, t, t_args, Arg, Text};

use crate::{
    git_repo::{
        self, chip_text, filter_branches, AheadBehind, Branch, Changes, Head, Limits, RepoInfo,
        Stamp, SwitchPlan, Unknown,
    },
    look, palette, rank, ui_add_drive::everywhere, ui_command_bar, ui_find, with_state,
    DriveState,
};

/// The chip's popover (its DOM id: the switcher's controls find the dialog through it).
pub(crate) const SWITCHER_ID: &str = "__azdrive_git_switcher";
/// The switcher's filter field.
const FILTER_ID: &str = "__azdrive_git_switcher_filter";
/// The chip.
const CHIP_ID: &str = "__azdrive_git_chip";
/// How often the watch looks at the repository's files.
const WATCH_MS: u64 = 1000;
/// The characters of a branch name the chip shows; a longer one is cut (the tooltip has it).
const CHIP_CHARS: usize = 28;

/// The window's git state: the chip's repository and the quick switcher.
#[derive(Default)]
pub(crate) struct GitView {
    /// The folder of this computer the window shows (the chip's), `None` off a local folder.
    pub dir: Option<PathBuf>,
    /// What the last finished look at `dir` found: `None` = no repository (no chip), `Err` =
    /// a repository gix could not read (a chip that says so).
    pub repo: Option<Result<RepoInfo, String>>,
    /// A look runs on a worker thread. Only one runs at a time; `again` asks for one more.
    pub probing: bool,
    /// Something changed while a look ran: one more look when it ends.
    pub again: bool,
    /// The stamps of the repository's watched files at the last look (the watch compares).
    pub stamp: Option<Stamp>,
    /// When the last look ended and how long it took: the poll for edits where no work tree
    /// watcher runs (`git_repo::poll_due`).
    pub last_look: Option<(std::time::Instant, StdDuration)>,
    /// The work tree's watcher (its edits; macOS only), and the root it watches; dropped off a
    /// repository.
    pub watcher: Option<(PathBuf, azcloud_kit::sync::watcher::Watcher)>,
    /// The watch timer runs (it ends itself when there is no repository).
    pub timer: bool,
    pub switcher: Switcher,
}

/// The quick switcher's state.
#[derive(Default)]
pub(crate) struct Switcher {
    /// The Dialog's declared `open`: set by the chip, the shortcut and `/switch`, cleared by
    /// its close event and a switch. It follows what shows, so a change of it opens or closes.
    pub open: bool,
    /// What was typed into the filter.
    pub query: String,
    /// The hot row of the filtered list (the arrows', Return's).
    pub hot: usize,
    /// The branches, once read (`None` while they are read).
    pub branches: Option<Result<Vec<Branch>, String>>,
    /// The switch being checked or asked about: the branch, and what the check found (`None`
    /// while it runs).
    pub confirm: Option<(String, Option<Result<SwitchPlan, String>>)>,
    /// The listing or check asked last; an older answer is dropped.
    pub serial: u64,
    /// What the app's window does once the dialog has closed (its close event runs there).
    pub after_close: Option<String>,
}

impl GitView {
    /// The repository the chip shows, when it read.
    pub(crate) fn info(&self) -> Option<&RepoInfo> {
        self.repo.as_ref().and_then(|r| r.as_ref().ok())
    }
}

/// The folder of this computer the window shows: the open folder of a local drive.
pub(crate) fn local_folder(s: &DriveState) -> Option<PathBuf> {
    let index = s.current_drive()?;
    s.local_dir(index, s.prefix())
}

// ==== Looking at the repository ====

/// The window went to another place (`go`): a new look when the folder changed. A folder of
/// the same work tree keeps the chip while the look runs; another one drops it first.
pub(crate) fn place_changed(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState) {
    let dir = local_folder(s);
    if dir == s.git.dir {
        return;
    }
    let same_tree = match (&dir, s.git.info().and_then(|i| i.work_dir.as_deref())) {
        (Some(dir), Some(root)) => dir.starts_with(root),
        _ => false,
    };
    if !same_tree {
        s.git.repo = None;
        s.git.stamp = None;
    }
    s.git.dir = dir;
    if s.git.dir.is_some() {
        request_probe(info, app, s);
    } else {
        s.git.watcher = None;
    }
}

/// Looks at the repository again (F5, a command ended, a change heard).
pub(crate) fn request_probe(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState) {
    if s.git.dir.is_none() {
        return;
    }
    if s.git.probing {
        s.git.again = true;
        return;
    }
    let Some(dir) = s.git.dir.clone() else {
        return;
    };
    s.git.probing = true;
    s.git.again = false;
    spawn(info, app, GitJob::Probe { dir });
}

/// A job for the worker thread.
enum GitJob {
    Probe { dir: PathBuf },
    Branches { serial: u64, dir: PathBuf },
    Plan { serial: u64, dir: PathBuf, branch: String },
}

/// What a job found.
enum GitDone {
    Probed {
        dir: PathBuf,
        stamp: Option<Stamp>,
        result: Result<Option<RepoInfo>, String>,
        /// How long the look took.
        took: StdDuration,
    },
    Branches {
        serial: u64,
        result: Result<Vec<Branch>, String>,
    },
    Planned {
        serial: u64,
        branch: String,
        result: Result<SwitchPlan, String>,
    },
}

/// The job a thread takes (once).
struct GitJobInit {
    job: Option<GitJob>,
}

/// What a thread hands back.
struct GitDoneMsg {
    done: Option<GitDone>,
}

fn spawn(info: &mut CallbackInfo, app: &RefAny, job: GitJob) {
    info.add_thread(
        ThreadId::unique(),
        Thread::create(
            RefAny::new(GitJobInit { job: Some(job) }),
            app.clone(),
            git_thread,
        ),
    );
}

extern "C" fn git_thread(mut init: RefAny, mut sender: ThreadSender, _receiver: ThreadReceiver) {
    let Some(job) = init
        .downcast_mut::<GitJobInit>()
        .and_then(|mut init| init.job.take())
    else {
        return;
    };
    let limits = Limits::default();
    let done = match job {
        GitJob::Probe { dir } => {
            let started = std::time::Instant::now();
            let result = git_repo::probe(&dir, &limits);
            let took = started.elapsed();
            // The stamps right after the look: a change after this one is the watch's.
            let stamp = match &result {
                Ok(Some(info)) => Some(git_repo::stamp(&info.watched_files())),
                _ => None,
            };
            GitDone::Probed {
                dir,
                stamp,
                result,
                took,
            }
        }
        GitJob::Branches { serial, dir } => GitDone::Branches {
            serial,
            result: git_repo::branches(&dir),
        },
        GitJob::Plan {
            serial,
            dir,
            branch,
        } => GitDone::Planned {
            serial,
            result: git_repo::plan_switch(&dir, &branch, &limits),
            branch,
        },
    };
    let _sent = sender.send(ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg::create(
        on_git_done,
        RefAny::new(GitDoneMsg { done: Some(done) }),
    )));
}

extern "C" fn on_git_done(mut app: RefAny, mut msg: RefAny, mut info: CallbackInfo) -> Update {
    let handle = app.clone();
    let Some(done) = msg
        .downcast_mut::<GitDoneMsg>()
        .and_then(|mut m| m.done.take())
    else {
        return Update::DoNothing;
    };
    let Some(mut guard) = app.downcast_mut::<DriveState>() else {
        return Update::DoNothing;
    };
    let s = &mut *guard;
    match done {
        GitDone::Probed {
            dir,
            stamp,
            result,
            took,
        } => {
            s.git.probing = false;
            if s.git.dir.as_deref() == Some(dir.as_path()) {
                s.git.last_look = Some((std::time::Instant::now(), took));
                s.git.repo = match result {
                    Ok(None) => None,
                    Ok(Some(found)) => Some(Ok(found)),
                    Err(why) => Some(Err(why)),
                };
                s.git.stamp = stamp;
                print_marker(s);
                if s.git.repo.is_some() {
                    start_watch(&mut info, &handle, s);
                }
            }
            // The folder changed or a change was heard while the look ran.
            if s.git.again || s.git.dir.as_deref() != Some(dir.as_path()) {
                request_probe(&mut info, &handle, s);
            }
        }
        GitDone::Branches { serial, result } => {
            if serial == s.git.switcher.serial {
                s.git.switcher.branches = Some(result);
                s.git.switcher.hot = 0;
            }
        }
        GitDone::Planned {
            serial,
            branch,
            result,
        } => {
            if serial == s.git.switcher.serial {
                planned(&mut info, &handle, s, branch, result);
            }
        }
    }
    if s.git.switcher.open {
        Update::RefreshDomAllWindows
    } else {
        Update::RefreshDom
    }
}

/// For scripts: `AZDRIVE_GIT none | unreadable | <branch> dirty=<0|1|?> ahead=<n> behind=<n>`.
fn print_marker(s: &DriveState) {
    match &s.git.repo {
        None => println!("AZDRIVE_GIT none"),
        Some(Err(_)) => println!("AZDRIVE_GIT unreadable"),
        Some(Ok(info)) => {
            let chip = chip_text(info);
            let dirty = match info.changes {
                Changes::Dirty { .. } => "1",
                Changes::Unknown(_) => "?",
                Changes::Clean | Changes::NoWorkTree => "0",
            };
            let (ahead, behind) = info.ahead_behind.map_or((0, 0), |ab| (ab.ahead, ab.behind));
            println!("AZDRIVE_GIT {} dirty={dirty} ahead={ahead} behind={behind}", chip.name);
        }
    }
}

// ==== The watch ====

/// The watch timer, once (it ends itself without a repository).
fn start_watch(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState) {
    if s.git.timer {
        return;
    }
    s.git.timer = true;
    let get_time = info.get_system_time_fn();
    info.add_timer(
        TimerId::unique(),
        Timer::create(app.clone(), on_watch_tick, get_time)
            .with_interval(Duration::System(SystemTimeDiff::from_millis(WATCH_MS))),
    );
}

/// Every second while a repository shows: a look again when HEAD, the index or a ref was
/// written (their stamps), or - on macOS - the work tree's watcher heard an edit outside
/// `.git`'s objects and logs; elsewhere when the poll is due. A repository too big to scan
/// gets no work tree watcher.
extern "C" fn on_watch_tick(mut data: RefAny, info: TimerCallbackInfo) -> TimerCallbackReturn {
    let mut callback_info = info.callback_info;
    let app = data.clone();
    let Some(mut guard) = data.downcast_mut::<DriveState>() else {
        return TimerCallbackReturn::continue_unchanged();
    };
    let s = &mut *guard;
    let Some(found) = s.git.info().cloned() else {
        if s.git.repo.is_none() {
            s.git.timer = false;
            s.git.watcher = None;
            return TimerCallbackReturn::terminate_unchanged();
        }
        return TimerCallbackReturn::continue_unchanged();
    };
    let mut changed = s
        .git
        .stamp
        .as_ref()
        .is_some_and(|before| *before != git_repo::stamp(&found.watched_files()));
    // Only FSEvents watches a whole tree cheaply (one stream, started at once): azcloud-kit's
    // inotify watcher walks the tree here, on the UI thread, with a watch per folder, and its
    // fallback reads the whole tree every two seconds.
    let watch_root = match (&found.work_dir, &found.changes) {
        (Some(root), Changes::Clean | Changes::Dirty { .. }) if cfg!(target_os = "macos") => {
            Some(root.clone())
        }
        _ => None,
    };
    if s.git.watcher.as_ref().map(|(root, _)| root) != watch_root.as_ref() {
        s.git.watcher = watch_root.and_then(|root| {
            azcloud_kit::sync::watcher::Watcher::start(&root)
                .ok()
                .map(|watcher| (root, watcher))
        });
    }
    if let Some((_, watcher)) = &s.git.watcher {
        if let Some(wake) = watcher.wait(StdDuration::ZERO, StdDuration::ZERO) {
            changed |= heard_an_edit(&wake);
        }
    } else if let (false, Some((at, took))) = (s.git.probing, s.git.last_look) {
        changed |= git_repo::poll_due(took, at.elapsed());
    }
    if changed {
        request_probe(&mut callback_info, &app, s);
    }
    TimerCallbackReturn::continue_unchanged()
}

/// Whether the watcher heard more than git's own bookkeeping (new objects, the logs): those
/// come with a ref or index write the stamps see anyway.
fn heard_an_edit(wake: &azcloud_kit::sync::watcher::Wake) -> bool {
    match wake {
        azcloud_kit::sync::watcher::Wake::Changed(folders) => folders.iter().any(|folder| {
            !(folder.starts_with(".git/objects") || folder.starts_with(".git/logs"))
        }),
        azcloud_kit::sync::watcher::Wake::Rescan(_) => true,
    }
}

// ==== The chip ====

/// The space the chip takes from the address bar's crumbs (px): its words at the bar's size,
/// its icon and padding; 0 without a chip.
pub(crate) fn chip_room(s: &DriveState) -> f32 {
    match &s.git.repo {
        None => 0.0,
        Some(_) => {
            let chars = chip_label(s).chars().count() as f32;
            (chars * 7.5 + 52.0).clamp(64.0, 300.0)
        }
    }
}

/// The address row: the address bar, and the chip with its switcher at its end when the folder
/// is in a repository.
pub(crate) fn address_row(s: &DriveState, app: &RefAny, bar: Dom) -> Dom {
    let Some(chip) = chip(s, app) else {
        return bar;
    };
    Dom::create_div()
        .with_css("display: flex; flex-direction: row; align-items: center; min-width: 0px;")
        .with_child(
            Dom::create_div()
                .with_css("display: flex; flex-direction: column; flex-grow: 1; min-width: 0px;")
                .with_child(bar),
        )
        .with_child(
            Dom::create_div()
                .with_css(
                    "display: flex; flex-direction: row; align-items: center; flex-shrink: 0; \
                     margin: 0px 8px 0px 2px;",
                )
                .with_child(chip),
        )
}

/// The chip's words: the branch (cut when long), the commit of a detached HEAD, the dot for
/// uncommitted changes, the counts against the upstream.
fn chip_label(s: &DriveState) -> String {
    let Some(info) = s.git.info() else {
        return t("azdrive-git-chip-unreadable");
    };
    let chip = chip_text(info);
    let mut name: String = chip.name.chars().take(CHIP_CHARS).collect();
    if chip.name.chars().count() > CHIP_CHARS {
        name.push('\u{2026}');
    }
    let mut label = if chip.detached {
        t_args("azdrive-git-chip-detached", &[("id", Arg::from(name))])
    } else {
        name
    };
    if chip.dirty {
        label.push_str(" \u{2022}");
    }
    if !chip.counts.is_empty() {
        label.push_str("  ");
        label.push_str(&chip.counts);
    }
    label
}

/// The chip's tooltip: what it says, in sentences, and what a click does.
fn chip_tooltip(s: &DriveState) -> String {
    let keys = if cfg!(target_os = "macos") { "\u{2318}B" } else { "Ctrl+B" };
    let info = match &s.git.repo {
        Some(Ok(info)) => info,
        Some(Err(why)) => {
            return t_args("azdrive-git-tip-unreadable", &[("detail", Arg::from(why.as_str()))]);
        }
        None => return String::new(),
    };
    let mut parts = vec![match &info.head {
        _ if info.bare() => t_args(
            "azdrive-git-tip-bare",
            &[("branch", Arg::from(info.head.branch().unwrap_or_default()))],
        ),
        Head::Branch(branch) => t_args("azdrive-git-tip-branch", &[("branch", Arg::from(branch))]),
        Head::Unborn(branch) => t_args("azdrive-git-tip-unborn", &[("branch", Arg::from(branch))]),
        Head::Detached(id) => t_args("azdrive-git-tip-detached", &[("id", Arg::from(id))]),
    }];
    if info.linked_worktree {
        parts.push(t("azdrive-git-tip-linked"));
    }
    match &info.changes {
        Changes::NoWorkTree => {}
        Changes::Clean => parts.push(t("azdrive-git-tip-clean")),
        Changes::Dirty { changed, untracked } => parts.push(t_args(
            "azdrive-git-tip-dirty",
            &[
                ("changed", Arg::from(*changed)),
                ("untracked", Arg::from(*untracked)),
            ],
        )),
        Changes::Unknown(Unknown::TooBig) => parts.push(t("azdrive-git-tip-too-big")),
        Changes::Unknown(Unknown::TimedOut) => parts.push(t("azdrive-git-tip-timed-out")),
        Changes::Unknown(Unknown::Failed(why)) => parts.push(t_args(
            "azdrive-git-tip-scan-failed",
            &[("detail", Arg::from(why.as_str()))],
        )),
    }
    if let (Some(upstream), Some(AheadBehind { ahead, behind, capped })) =
        (&info.upstream, info.ahead_behind)
    {
        let key = if capped {
            "azdrive-git-tip-upstream-capped"
        } else {
            "azdrive-git-tip-upstream"
        };
        parts.push(t_args(
            key,
            &[
                ("upstream", Arg::from(upstream.as_str())),
                ("ahead", Arg::from(ahead)),
                ("behind", Arg::from(behind)),
            ],
        ));
    }
    if !info.bare() {
        parts.push(t_args("azdrive-git-tip-click", &[("keys", Arg::from(keys))]));
    }
    parts.join(" ")
}

/// The chip and its switcher (azul's Dialog, a popover under the chip), or `None` without a
/// repository. A bare repository's chip only names its branch: it has nothing to switch.
fn chip(s: &DriveState, app: &RefAny) -> Option<Dom> {
    s.git.repo.as_ref()?;
    let switchable = s.git.info().is_some_and(|info| !info.bare());
    let mut button = Button::create(AzString::from(chip_label(s)))
        .with_icon(AzString::from("fork_right"));
    if switchable {
        button = button.with_on_click(app.clone(), on_chip as ButtonOnClickCallbackType);
    } else {
        button = button.with_disabled(AzString::from(chip_tooltip(s)));
    }
    let chip = button
        .dom()
        .with_id(CHIP_ID)
        .with_attribute(AttributeType::Title(AzString::from(chip_tooltip(s))))
        .with_accessibility_name(AzString::from(chip_tooltip(s)));
    if !switchable {
        return Some(chip);
    }
    Some(
        Dialog::create(switcher_content(s, app))
            .with_invoker(chip)
            .with_title(label("azdrive-git-switcher-title"))
            .with_open(s.git.switcher.open)
            .with_modal(false)
            .with_anchor(TransientAnchor::Bottom)
            .with_closed_by(DialogClosedBy::Any)
            .with_close_button(false)
            .with_on_close(app.clone(), on_switcher_closed as DialogOnCloseCallbackType)
            .dom(),
    )
}

/// The chip was clicked (or pressed with the keyboard): the switcher opens with its branches.
extern "C" fn on_chip(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |info, app, s| {
        open_switcher(info, app, s, String::new());
    })
}

// ==== The quick switcher ====

/// Opens the quick switcher with `filter` typed (the chip, Cmd/Ctrl+B, `/switch <filter>`) and
/// reads the branches. Off a repository the command bar says why.
pub(crate) fn open_switcher(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState, filter: String) {
    let Some(found) = s.git.info() else {
        ui_command_bar::say(s, Text::key("azdrive-git-no-repository"));
        return;
    };
    if found.bare() {
        ui_command_bar::say(s, Text::key("azdrive-git-switch-bare"));
        return;
    }
    let Some(dir) = s.git.dir.clone() else {
        return;
    };
    let switcher = &mut s.git.switcher;
    switcher.open = true;
    switcher.query = filter;
    switcher.hot = 0;
    switcher.confirm = None;
    switcher.branches = None;
    switcher.after_close = None;
    switcher.serial += 1;
    let serial = switcher.serial;
    println!("AZDRIVE_GIT_SWITCHER open");
    spawn(info, app, GitJob::Branches { serial, dir });
}

/// The branches the filter keeps, in the switcher's order.
fn shown_branches(s: &DriveState) -> Vec<Branch> {
    match &s.git.switcher.branches {
        Some(Ok(list)) => filter_branches(list, &s.git.switcher.query)
            .into_iter()
            .cloned()
            .collect(),
        _ => Vec::new(),
    }
}

/// The switcher's panel: the filter, the branches, the keys; or the question about a switch.
fn switcher_content(s: &DriveState, app: &RefAny) -> Dom {
    let switcher = &s.git.switcher;
    let mut panel = Dom::create_div()
        .with_id(SWITCHER_ID)
        .with_css("display: flex; flex-direction: column; width: 340px; max-width: 100%;");
    if let Some((branch, plan)) = &switcher.confirm {
        panel.add_child(confirm_view(app, branch, plan.as_ref()));
        return panel;
    }
    let input = TextInput::create()
        .with_text(AzString::from(switcher.query.as_str()))
        .with_placeholder(label("azdrive-git-switcher-filter"))
        .with_on_text_input(app.clone(), on_filter_text as TextInputOnTextInputCallbackType)
        .with_on_virtual_key_down(app.clone(), on_filter_key as TextInputOnVirtualKeyDownCallbackType)
        .with_accessibility_name(label("azdrive-git-switcher-filter"))
        .dom()
        .with_id(FILTER_ID)
        .with_css("flex-grow: 1;");
    panel.add_child(
        Dom::create_div()
            .with_css(look::PALETTE_INPUT_ROW)
            .with_child(
                Dom::create_icon(AzString::from("fork_right"))
                    .with_css("font-size: 18px; margin-right: 8px;"),
            )
            .with_child(input)
            .with_child(info_mark("azdrive-git-switcher-info")),
    );
    let mut list = Dom::create_div().with_css(look::PALETTE_LIST);
    match &switcher.branches {
        None => list.add_child(section_line("azdrive-git-switcher-loading")),
        Some(Err(why)) => list.add_child(
            Dom::create_div().with_css(look::PALETTE_SECTION).with_child(
                Dom::create_span_with_text(AzString::from(t_args(
                    "azdrive-git-switcher-failed",
                    &[("detail", Arg::from(why.as_str()))],
                ))),
            ),
        ),
        Some(Ok(_)) => {
            let rows = shown_branches(s);
            if rows.is_empty() {
                list.add_child(section_line("azdrive-git-switcher-none"));
            }
            let hot = switcher.hot.min(rows.len().saturating_sub(1));
            for (i, branch) in rows.iter().enumerate() {
                list.add_child(branch_row(app, branch, &switcher.query, i == hot, i));
            }
        }
    }
    panel.add_child(list);
    panel.add_child(
        Dom::create_div()
            .with_css(look::PALETTE_FOOT)
            .with_child(Dom::create_span_with_text(label("azdrive-git-switcher-keys"))),
    );
    panel
}

/// A grey line in the list (reading, nothing matches).
fn section_line(key: &str) -> Dom {
    Dom::create_div()
        .with_css(look::PALETTE_SECTION)
        .with_child(Dom::create_span_with_text(label(key)))
}

/// An (i) whose tooltip explains, in sentences.
pub(crate) fn info_mark(key: &str) -> Dom {
    let words = AzString::from(t(key));
    Dom::create_icon(AzString::from("info"))
        .with_css("font-size: 16px; opacity: 0.6; margin-left: 6px; flex-shrink: 0;")
        .with_attribute(AttributeType::Title(words.clone()))
        .with_accessibility_name(words)
}

/// What a branch row carries.
struct RowRef {
    app: RefAny,
    index: usize,
}

/// One branch: its icon (a tick for the current one), its name with the typed terms marked,
/// "current" after the current one.
fn branch_row(app: &RefAny, branch: &Branch, query: &str, hot: bool, index: usize) -> Dom {
    let marks = rank::marks(&branch.name, query);
    let mut row = Dom::create_div()
        .with_css(format!(
            "{} {}",
            look::PALETTE_ROW,
            if hot { look::PALETTE_ROW_HOT } else { "" }
        ))
        .with_accessibility_name(AzString::from(branch.name.as_str()))
        .with_attribute(AttributeType::Title(AzString::from(t_args(
            if branch.current {
                "azdrive-git-row-current-tip"
            } else {
                "azdrive-git-row-tip"
            },
            &[("branch", Arg::from(branch.name.as_str()))],
        ))))
        .with_child(
            Dom::create_icon(AzString::from(if branch.current { "check" } else { "fork_right" }))
                .with_css("font-size: 16px; margin-right: 8px; flex-shrink: 0;"),
        )
        .with_child(ui_find::marked(&branch.name, &marks, look::CLIP));
    if branch.current {
        row.add_child(
            Dom::create_div()
                .with_css(look::PALETTE_DETAIL)
                .with_child(Dom::create_span_with_text(label("azdrive-git-switcher-current"))),
        );
    }
    row.with_callback(
        EventFilter::Hover(HoverEventFilter::Click),
        RefAny::new(RowRef {
            app: app.clone(),
            index,
        }),
        on_branch_row,
    )
}

extern "C" fn on_branch_row(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, index)) = data
        .downcast_ref::<RowRef>()
        .map(|r| (r.app.clone(), r.index))
    else {
        return Update::DoNothing;
    };
    info.stop_propagation();
    everywhere(with_state(&mut app, &mut info, |info, app, s| {
        choose(info, app, s, index);
    }))
}

extern "C" fn on_filter_text(
    mut data: RefAny,
    mut info: CallbackInfo,
    state: TextInputState,
) -> OnTextInputReturn {
    let text = state.get_text().as_str().to_string();
    let update = with_state(&mut data, &mut info, |_info, _app, s| {
        s.git.switcher.query = text;
        s.git.switcher.hot = 0;
    });
    OnTextInputReturn {
        update: everywhere(update),
        valid: TextInputValid::Yes,
    }
}

/// The filter's keys: the arrows move the hot row, Return switches to it. Escape is the
/// dialog's (it closes).
extern "C" fn on_filter_key(
    mut data: RefAny,
    mut info: CallbackInfo,
    _state: TextInputState,
) -> OnTextInputReturn {
    let key = info
        .get_current_keyboard_state()
        .current_virtual_keycode
        .into_option();
    let update = match key {
        Some(VirtualKeyCode::Down | VirtualKeyCode::Up) => {
            info.stop_propagation();
            let delta = if key == Some(VirtualKeyCode::Down) { 1 } else { -1 };
            everywhere(with_state(&mut data, &mut info, |_info, _app, s| {
                let count = shown_branches(s).len();
                let switcher = &mut s.git.switcher;
                switcher.hot = palette::step(switcher.hot, delta, count);
            }))
        }
        Some(VirtualKeyCode::Return | VirtualKeyCode::NumpadEnter) => {
            info.stop_propagation();
            everywhere(with_state(&mut data, &mut info, |info, app, s| {
                let hot = s.git.switcher.hot;
                choose(info, app, s, hot);
            }))
        }
        _ => Update::DoNothing,
    };
    OnTextInputReturn {
        update,
        valid: TextInputValid::Yes,
    }
}

/// Row `index` of the filtered list was chosen: the current branch closes the switcher, any
/// other is checked before the switch.
fn choose(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState, index: usize) {
    let Some(branch) = shown_branches(s).into_iter().nth(index) else {
        return;
    };
    if branch.current {
        close_from_inside(info, s);
        return;
    }
    let Some(dir) = s.git.dir.clone() else {
        return;
    };
    let switcher = &mut s.git.switcher;
    switcher.serial += 1;
    switcher.confirm = Some((branch.name.clone(), None));
    let serial = switcher.serial;
    spawn(
        info,
        app,
        GitJob::Plan {
            serial,
            dir,
            branch: branch.name,
        },
    );
}

/// The check before a switch answered: a clean tree switches now; anything else is asked or
/// said in the switcher.
fn planned(
    info: &mut CallbackInfo,
    app: &RefAny,
    s: &mut DriveState,
    branch: String,
    result: Result<SwitchPlan, String>,
) {
    println!(
        "AZDRIVE_GIT_PLAN {}",
        match &result {
            Ok(SwitchPlan::Clean) => "clean",
            Ok(SwitchPlan::Carry { .. }) => "carry",
            Ok(SwitchPlan::Blocked { .. }) => "blocked",
            Ok(SwitchPlan::Unknown(_)) => "unknown",
            Ok(SwitchPlan::Current) => "current",
            Ok(SwitchPlan::NoSuchBranch) => "gone",
            Ok(SwitchPlan::NoWorkTree) => "bare",
            Err(_) => "failed",
        }
    );
    match result {
        Ok(SwitchPlan::Clean) => {
            s.git.switcher.after_close = Some(switch_line(&branch));
            if !close_from_inside(info, s) {
                // No dialog to close (it was closed meanwhile): the switch runs from here.
                s.git.switcher.open = false;
                if let Some(line) = s.git.switcher.after_close.take() {
                    ui_command_bar::run_line(info, app, s, &line);
                }
            }
        }
        Ok(SwitchPlan::Current) => {
            close_from_inside(info, s);
        }
        other => s.git.switcher.confirm = Some((branch, Some(other))),
    }
}

/// The command line of a switch: `git switch <branch>`, the name quoted for the shell (a
/// branch name may hold `$`, `;` or quotes).
fn switch_line(branch: &str) -> String {
    format!(
        "git switch {}",
        crate::command_line::shell_quote(branch, crate::command_line::Platform::this())
    )
}

/// Closes the switcher from a control inside it or a job's answer: the dialog's close event
/// then runs in the app's window (and runs what `after_close` holds). `false` when the
/// dialog is not there.
fn close_from_inside(info: &mut CallbackInfo, s: &mut DriveState) -> bool {
    let dom = DomId { inner: 0 };
    let node = info.get_node_id_by_id_attribute(dom, AzString::from(SWITCHER_ID));
    if node.into_raw() == 0 {
        s.git.switcher.open = false;
        return false;
    }
    let closed = Dialog::close_from(*info, DomNodeId { dom, node }, AzString::from(""));
    if !closed {
        s.git.switcher.open = false;
    }
    closed
}

/// The switcher closed (Escape, a press outside, a switch): its state is reset, and what
/// waited for the close runs here, in the app's window.
extern "C" fn on_switcher_closed(
    mut data: RefAny,
    mut info: CallbackInfo,
    _state: DialogState,
) -> Update {
    with_state(&mut data, &mut info, |info, app, s| {
        let after = s.git.switcher.after_close.take();
        let switcher = &mut s.git.switcher;
        switcher.open = false;
        switcher.confirm = None;
        switcher.query.clear();
        switcher.serial += 1;
        println!("AZDRIVE_GIT_SWITCHER closed");
        if let Some(line) = after {
            ui_command_bar::run_line(info, app, s, &line);
        }
    })
}

/// What a confirm button does.
#[derive(Clone)]
enum Answer {
    /// Switch anyway: git carries the changes.
    Carry(String),
    /// Show the changes (`git status` in the command bar).
    ShowChanges,
    /// Back to the branches.
    Back,
}

struct AnswerRef {
    app: RefAny,
    answer: Answer,
}

/// The question (or the refusal) about a switch: what the check found, in sentences, and the
/// buttons that fit.
fn confirm_view(
    app: &RefAny,
    branch: &str,
    plan: Option<&Result<SwitchPlan, String>>,
) -> Dom {
    let changes = |changed: usize, untracked: usize| {
        t_args(
            "azdrive-git-changes",
            &[
                ("changed", Arg::from(changed)),
                ("untracked", Arg::from(untracked)),
            ],
        )
    };
    let b = || Arg::from(branch);
    let mut buttons: Vec<Dom> = Vec::new();
    let sentence = match plan {
        None => t_args("azdrive-git-switch-checking", &[("branch", b())]),
        Some(Ok(SwitchPlan::Carry { changed, untracked })) => {
            buttons.push(answer_button(
                app,
                "azdrive-git-carry",
                &t_args("azdrive-git-carry-tip", &[("branch", b())]),
                Answer::Carry(branch.to_string()),
                ButtonType::Primary,
            ));
            t_args(
                "azdrive-git-switch-carry",
                &[("changes", Arg::from(changes(*changed, *untracked))), ("branch", b())],
            )
        }
        Some(Ok(SwitchPlan::Blocked { files, more })) => {
            buttons.push(show_changes_button(app));
            let mut names = files.join(", ");
            if *more > 0 {
                names.push_str(", ");
                names.push_str(&t_args(
                    "azdrive-git-switch-blocked-more",
                    &[("count", Arg::from(*more))],
                ));
            }
            t_args(
                "azdrive-git-switch-blocked",
                &[("branch", b()), ("files", Arg::from(names))],
            )
        }
        Some(Ok(SwitchPlan::Unknown(why))) => {
            buttons.push(show_changes_button(app));
            let why = match why {
                Unknown::TooBig => t("azdrive-git-why-too-big"),
                Unknown::TimedOut => t("azdrive-git-why-timed-out"),
                Unknown::Failed(detail) => detail.clone(),
            };
            t_args(
                "azdrive-git-switch-unknown",
                &[("branch", b()), ("why", Arg::from(why))],
            )
        }
        Some(Ok(SwitchPlan::NoSuchBranch)) => t_args("azdrive-git-switch-gone", &[("branch", b())]),
        Some(Ok(SwitchPlan::NoWorkTree)) => t("azdrive-git-switch-bare"),
        // Clean and Current never stay: the switcher closes on them.
        Some(Ok(SwitchPlan::Clean | SwitchPlan::Current)) => String::new(),
        Some(Err(why)) => t_args("azdrive-git-switch-failed", &[("detail", Arg::from(why.as_str()))]),
    };
    buttons.push(answer_button(
        app,
        "azdrive-git-back",
        &t("azdrive-git-back-tip"),
        Answer::Back,
        ButtonType::Default,
    ));
    let mut row = Dom::create_div()
        .with_css("display: flex; flex-direction: row; justify-content: flex-end; margin-top: 12px;");
    for button in buttons {
        row.add_child(button);
    }
    Dom::create_div()
        .with_css("display: flex; flex-direction: column; padding: 14px;")
        .with_child(
            Dom::create_div()
                .with_css("display: flex; flex-direction: row; align-items: flex-start;")
                .with_child(
                    Dom::create_icon(AzString::from(if plan.is_none() { "hourglass_empty" } else { "info" }))
                        .with_css("font-size: 18px; margin-right: 8px; flex-shrink: 0;"),
                )
                .with_child(
                    Dom::create_div()
                        .with_css("flex-grow: 1; min-width: 0px; line-height: 1.4;")
                        .with_child(Dom::create_span_with_text(AzString::from(sentence))),
                ),
        )
        .with_child(row)
}

fn show_changes_button(app: &RefAny) -> Dom {
    answer_button(
        app,
        "azdrive-git-show-changes",
        &t("azdrive-git-show-changes-tip"),
        Answer::ShowChanges,
        ButtonType::Default,
    )
}

/// A button of the question, with its tooltip; the first one takes the keyboard.
fn answer_button(app: &RefAny, key: &str, tip: &str, answer: Answer, kind: ButtonType) -> Dom {
    let first = matches!(answer, Answer::Carry(_) | Answer::ShowChanges);
    let mut dom = Button::with_type(label(key), kind)
        .with_on_click(
            RefAny::new(AnswerRef {
                app: app.clone(),
                answer,
            }),
            on_answer as ButtonOnClickCallbackType,
        )
        .dom()
        .with_css("margin-left: 6px;")
        .with_attribute(AttributeType::Title(AzString::from(tip)));
    if first {
        dom = dom.with_callback(
            EventFilter::Component(ComponentEventFilter::AfterMount),
            RefAny::new(()),
            on_answer_mounted,
        );
    }
    dom
}

/// The question appeared: its first button takes the keyboard (the filter it replaced had it).
extern "C" fn on_answer_mounted(_data: RefAny, mut info: CallbackInfo) -> Update {
    let node = info.get_hit_node();
    info.set_focus(FocusTarget::Id(node));
    Update::DoNothing
}

extern "C" fn on_answer(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, answer)) = data
        .downcast_ref::<AnswerRef>()
        .map(|r| (r.app.clone(), r.answer.clone()))
    else {
        return Update::DoNothing;
    };
    everywhere(with_state(&mut app, &mut info, |info, _app, s| match answer {
        Answer::Carry(branch) => {
            s.git.switcher.after_close = Some(switch_line(&branch));
            close_from_inside(info, s);
        }
        Answer::ShowChanges => {
            s.git.switcher.after_close = Some(String::from("git status"));
            close_from_inside(info, s);
        }
        Answer::Back => {
            s.git.switcher.confirm = None;
            s.git.switcher.serial += 1;
        }
    }))
}
