//! The screens of the layout study, each as a state the simulator can be put in: the pairing in its steps and failures
//! (A), the remote in several moments (B), the files (C), the buttons' editor (D), the settings (E). A scene is made with
//! the same actions the user's taps make, so what a screenshot shows is what the app does.

use azul_review_link::frame::{PenMark, Rect};
use azul_review_link::limits::Capped;
use azul_review_link::view::FailureKind;

use super::phone::SimCommand;
use super::{Extras, SessionScene};
use crate::ui::{Action, Root, Screen, Ui};

/// Every scene: its name (the file's name) and what it shows.
pub const ALL: &[(&str, &str)] = &[
    ("a1-intro", "Pairing: the first screen"),
    ("a2-searching", "Pairing: looking for the Mac"),
    ("a3-found", "Pairing: the Mac is found, the code is shown"),
    ("a4-connected", "Pairing: connected"),
    ("a5-notfound", "Pairing: AzReview not found"),
    ("a6-permission", "Pairing: the local network is off"),
    ("a7-address", "Pairing: the Mac cannot be reached at its saved address"),
    ("a8-update-app", "Pairing: the companion is too old"),
    ("a9-update-azreview", "Pairing: AzReview is too old"),
    ("b1-remote", "Remote: first use, with the hint"),
    ("b2-remote-buttons", "Remote: four buttons, four states"),
    ("b3-remote-zoomed", "Remote: zoomed in, the pen down"),
    ("b4-remote-lost", "Remote: the link is lost"),
    ("b5-remote-region", "Remote: a region is marked"),
    ("b6-remote-long-labels", "Remote: the longest labels"),
    ("c1-files-root", "Files: the workspace"),
    ("c2-files-deep", "Files: a folder deep in it, the open file marked"),
    ("c3-files-long", "Files: a folder of 700 files"),
    ("c4-files-loading", "Files: waiting for a folder"),
    ("d1-editor", "Buttons editor"),
    ("d2-editor-cut", "Buttons editor: a text cut to its cap"),
    ("e1-settings", "Settings and about"),
    ("e2-settings-forget", "Settings: forgetting the Mac"),
];

/// How long a scene is left to settle before it is looked at, in milliseconds.
pub const SETTLE_MS: u64 = 700;

/// The scene names that start with these letters (`"abcde"`: all), or the names given.
#[must_use]
pub fn pick(spec: &str) -> Vec<String> {
    if spec.is_empty() || spec == "all" {
        return ALL.iter().map(|(n, _)| (*n).to_string()).collect();
    }
    spec.split(',')
        .flat_map(|part| {
            let part = part.trim();
            let exact: Vec<String> = ALL.iter().filter(|(n, _)| *n == part).map(|(n, _)| (*n).to_string()).collect();
            if !exact.is_empty() {
                exact
            } else {
                ALL.iter().filter(|(n, _)| n.starts_with(part) && !part.is_empty()).map(|(n, _)| (*n).to_string()).collect()
            }
        })
        .collect()
}

/// How long the reset gets before a scene is set up (the connection takes the commands at its next turn).
pub const RESET_MS: u64 = 150;

/// Every scene starts from the same place: a connected phone that knows nothing yet. The connection takes these commands at
/// its next turn, so [`setup`] comes after [`RESET_MS`].
pub fn reset(root: &mut Root, extras: &Extras) {
    if let Ok(mut mac) = extras.mac.lock() {
        *mac = super::mac::SimMac::new();
    }
    if let Ok(mut commands) = extras.commands.lock() {
        commands.push(SimCommand::Latency(extras.latency));
        commands.push(SimCommand::Fresh);
    }
    root.ui = Ui::new();
}

/// Puts the app in the state of scene `name` (after a [`reset`]). False for a name no scene has.
pub fn setup(name: &str, root: &mut Root, extras: &Extras, now: u64) -> bool {
    if !ALL.iter().any(|(n, _)| *n == name) {
        return false;
    }
    let mac_name = extras.mac.lock().map(|m| m.name.clone()).unwrap_or_default();
    let show = |scene: SessionScene| {
        if let Ok(mut commands) = extras.commands.lock() {
            commands.push(SimCommand::Session(scene.session(&mac_name)));
        }
    };
    match name {
        "a1-intro" => show(SessionScene::Unpaired),
        "a2-searching" => show(SessionScene::Searching),
        "a3-found" => show(SessionScene::Found),
        "a4-connected" => {
            show(SessionScene::Connected);
            root.ui.pair_card = true;
            root.ui.last_session = crate::ui::SessionKind::Connected;
        }
        "a5-notfound" => show(SessionScene::NotFound),
        "a6-permission" => show(SessionScene::NoPermission),
        "a7-address" => show(SessionScene::AddressChanged),
        "a8-update-app" => show(SessionScene::UpdateCompanion),
        "a9-update-azreview" => show(SessionScene::UpdateAzReview),
        "b1-remote" => {}
        "b2-remote-buttons" => {
            root.ui.hint = false;
            // Tone is being worked on, Explain has a suggestion, Shorter is done, Doc was just sent
            for index in 0..4 {
                root.perform(now, Action::Button(index));
            }
            if let Ok(mut mac) = extras.mac.lock() {
                let ages: [i64; 4] = [-1500, -6000, -10_000, 1_000_000];
                for (conversation, age) in mac.conversations.iter_mut().zip(ages) {
                    conversation.started = (now as i64 + age).max(0) as u64;
                }
            }
        }
        "b3-remote-zoomed" => {
            root.ui.hint = false;
            if let Ok(mut mac) = extras.mac.lock() {
                mac.view = Rect { x: 0.18, y: 0.22, w: 0.25, h: 0.16 };
                mac.pen = Some(PenMark { x: 0.30, y: 0.29, down: true });
                mac.line = 214;
            }
        }
        "b4-remote-lost" => {
            root.ui.hint = false;
            // the link goes after the phone has heard the Mac once
            root.ui.last_session = crate::ui::SessionKind::Connected;
            root.ui.screen = Screen::Remote;
            if let Ok(mut commands) = extras.commands.lock() {
                commands.push(SimCommand::LinkLostAfter(400));
            }
        }
        "b5-remote-region" => {
            root.ui.hint = false;
            if let Ok(mut mac) = extras.mac.lock() {
                mac.armed_region = true;
            }
        }
        "b6-remote-long-labels" => {
            root.ui.hint = false;
            if let Ok(mut mac) = extras.mac.lock() {
                for (button, label) in mac.buttons.buttons.iter_mut().zip(["Explain more", "Make shorter", "Überarbeiten", "Kürzer fasse"]) {
                    button.label = label.chars().take(12).collect();
                }
            }
        }
        "c1-files-root" => {
            root.ui.hint = false;
            root.perform(now, Action::Tab(Screen::Files));
        }
        "c2-files-deep" => {
            root.ui.hint = false;
            root.ui.screen = Screen::Files;
            root.perform(now, Action::OpenDir("layout/src/solver3".into()));
        }
        "c3-files-long" => {
            root.ui.hint = false;
            root.ui.screen = Screen::Files;
            root.perform(now, Action::OpenDir("tests/fixtures".into()));
        }
        "c4-files-loading" => {
            root.ui.hint = false;
            root.ui.screen = Screen::Files;
            if let Ok(mut commands) = extras.commands.lock() {
                commands.push(SimCommand::Latency(20_000));
            }
            root.perform(now, Action::OpenDir("layout/src".into()));
        }
        "d1-editor" => {
            root.ui.hint = false;
            root.perform(now, Action::EditButtons(Some(1)));
        }
        "d2-editor-cut" => {
            root.ui.hint = false;
            root.perform(now, Action::EditButtons(Some(0)));
            root.ui.drafts[0] = (
                "Plain tone".to_string(),
                "Slop wording: please rewrite this in a normal, plain tone. Keep the meaning, drop the filler, keep it short and friendly, and do not add anything the author did not say. If a sentence is already fine, leave it as it is.".to_string(),
            );
            root.ui.draft_cut[0].1 = Some(Capped { what: "button text", got: 301, limit: 280 });
            root.ui.editor_dirty = true;
        }
        "e1-settings" => {
            root.ui.hint = false;
            root.perform(now, Action::OpenSettings);
        }
        "e2-settings-forget" => {
            root.ui.hint = false;
            root.perform(now, Action::OpenSettings);
            root.perform(now, Action::ForgetAsk);
        }
        _ => return false,
    }
    let _ = FailureKind::Timeout;
    true
}

#[cfg(test)]
#[path = "scenes_test.rs"]
mod scenes_test;
