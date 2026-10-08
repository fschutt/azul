//! Media Center's overlays: what a desktop app puts in a context menu or a dialog box, AzPlayer
//! shows over the page in its own ten-foot look - a dark glass panel, big lower-case type, big
//! choices with the focus's bar of light gliding between them:
//!
//! - MORE INFO (right click, the menu key, Ctrl+D or I - Media Center's "more info" button): an
//!   item's title and what it is, and what can be done with it - play, add to the queue, play
//!   the slide show from it, play from the start, forget it, delete it; the now-playing inset's
//!   play / pause, stop, now playing.
//! - THE DIALOGS: about AzPlayer, a question (delete this file? remove this folder from the
//!   library? the music is paused - stop it?), what went wrong (a video that does not play
//!   here), an address to play.
//!
//! The keys: Up / Down (a dialog's buttons also Left / Right, Tab) move the focus, Enter
//! chooses, Back / Escape / Backspace do what the overlay's "no" does (`Overlay::back`: close,
//! or go back). The pointer lights a choice, a click chooses, a click beside the panel is Back.
//! Plain Rust, tested without a window.

use crate::{library::Shelf, strip::Action};

/// What a choice of an overlay does.
#[derive(Debug, Clone, PartialEq)]
pub enum Do {
    /// Closes the overlay.
    Close,
    /// Closes the overlay and goes back a page.
    Back,
    /// Opens tile `index` of the page as Enter does: a group's page, a song plays from it, a
    /// picture shows, a video opens.
    Open(usize),
    /// Plays the items of tile `index`: an album's songs, a folder's pictures as a slide show.
    Play(usize),
    /// The songs of tile `index` after the ones queued (they play next to last).
    Queue(usize),
    /// The slide show from picture tile `index` (or of a folder's pictures).
    SlideShow(usize),
    /// A video from its start, not where it was left.
    FromStart(String),
    /// Forgets a recently played file (the file stays).
    Forget(String),
    /// Asks before deleting a file ([`confirm_delete`]), then deletes it.
    AskDelete(String),
    Delete(String),
    /// What plays: pause / play, stop (the inset goes), its page.
    PlayPause,
    Stop,
    NowPlaying,
    /// Stops the music and goes back (now playing's Back while the music is paused).
    StopAndBack,
    /// An item of the start strip.
    Strip(Action),
    /// Asks before removing a library folder ([`confirm_remove_folder`]), then removes it
    /// (from the settings' library setup).
    AskRemoveFolder(Shelf, String),
    RemoveFolder(Shelf, String),
    /// The address dialog: plays the field's address, the sample.
    PlayAddress,
    PlaySample,
    /// Leaves AzPlayer.
    Quit,
}

/// One choice of an overlay.
#[derive(Debug, Clone, PartialEq)]
pub struct Choice {
    /// What it says (lower case, Media Center's way).
    pub label: String,
    /// Its icon (a Material icon name).
    pub icon: &'static str,
    pub act: Do,
}

/// A choice.
#[must_use]
pub fn choice(label: &str, icon: &'static str, act: Do) -> Choice {
    Choice {
        label: label.to_string(),
        icon,
        act,
    }
}

/// The overlay's look.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// An item's more info: a panel on the right, its title and lines over a column of its
    /// choices (Up / Down).
    MoreInfo,
    /// A dialog: a panel in the middle, its title and sentences over a row of buttons at its
    /// bottom right (Left / Right, Up / Down).
    Dialog,
    /// The address dialog: a dialog with the address field over its buttons (Left / Right are
    /// the field's caret: Up / Down and Tab walk the buttons).
    Address,
}

/// An overlay over the page.
#[derive(Debug, Clone, PartialEq)]
pub struct Overlay {
    pub kind: Kind,
    /// Its name: `AZPLAYER_OVERLAY <name>`, its panel's id (`overlay-<name>`).
    pub name: &'static str,
    pub title: String,
    /// What it says under the title.
    pub lines: Vec<String>,
    pub choices: Vec<Choice>,
    /// The focused choice.
    pub focus: usize,
    /// What Back, Escape and a click beside the panel do.
    pub back: Do,
}

impl Overlay {
    /// The focused choice.
    #[must_use]
    pub fn chosen(&self) -> Option<&Choice> {
        self.choices.get(self.focus)
    }

    /// The focus one choice on (`forward`) or back, not past either end. `true`: it moved.
    pub fn step(&mut self, forward: bool) -> bool {
        let n = self.choices.len();
        let to = if forward {
            (self.focus + 1).min(n.saturating_sub(1))
        } else {
            self.focus.saturating_sub(1)
        };
        let moved = to != self.focus;
        self.focus = to;
        moved
    }

    /// The focus one choice on (`forward`) or back, round (Tab).
    pub fn cycle(&mut self, forward: bool) {
        let n = self.choices.len();
        if n == 0 {
            return;
        }
        self.focus = if forward {
            (self.focus + 1) % n
        } else {
            (self.focus + n - 1) % n
        };
    }

    /// Focuses the choice `index` (the pointer over it). `true`: it moved.
    pub fn point(&mut self, index: usize) -> bool {
        if index >= self.choices.len() || index == self.focus {
            return false;
        }
        self.focus = index;
        true
    }
}

/// An item's more info: `title`, the `lines` saying what it is, its `choices` (the first
/// focused).
#[must_use]
pub fn more_info(title: &str, lines: Vec<String>, choices: Vec<Choice>) -> Overlay {
    Overlay {
        kind: Kind::MoreInfo,
        name: "more-info",
        title: title.to_lowercase(),
        lines,
        choices,
        focus: 0,
        back: Do::Close,
    }
}

/// About AzPlayer: `lines` (its name and version, what it is, its license, where its data is).
#[must_use]
pub fn about(lines: Vec<String>) -> Overlay {
    Overlay {
        kind: Kind::Dialog,
        name: "about",
        title: String::from("about azplayer"),
        lines,
        choices: vec![choice("ok", "check", Do::Close)],
        focus: 0,
        back: Do::Close,
    }
}

/// Delete `path` (`title`: its name)? The question focuses "no": Enter twice never deletes.
#[must_use]
pub fn confirm_delete(path: &str, title: &str) -> Overlay {
    let folder = std::path::Path::new(path)
        .parent()
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    Overlay {
        kind: Kind::Dialog,
        name: "confirm-delete",
        title: String::from("delete this file?"),
        lines: vec![
            title.to_string(),
            format!("It is deleted from {folder} and cannot be brought back."),
        ],
        choices: vec![
            choice("yes, delete it", "delete", Do::Delete(path.to_string())),
            choice("no", "close", Do::Close),
        ],
        focus: 1,
        back: Do::Close,
    }
}

/// Remove the folder `path` from the `shelf` library? "no" focused.
#[must_use]
pub fn confirm_remove_folder(shelf: Shelf, path: &str) -> Overlay {
    Overlay {
        kind: Kind::Dialog,
        name: "confirm-remove-folder",
        title: format!("remove this folder from {}?", shelf.word()),
        lines: vec![
            path.to_string(),
            String::from("AzPlayer stops showing what is in it; the files stay where they are."),
        ],
        choices: vec![
            choice(
                "yes, remove it",
                "folder_off",
                Do::RemoveFolder(shelf, path.to_string()),
            ),
            choice("no", "close", Do::Close),
        ],
        focus: 1,
        back: Do::Close,
    }
}

/// Now playing's Back while the music is paused: stop it (the inset goes), or keep it paused
/// for later. Back keeps it and goes back, as Back would have.
#[must_use]
pub fn music_paused(song: &str) -> Overlay {
    Overlay {
        kind: Kind::Dialog,
        name: "music-paused",
        title: String::from("the music is paused"),
        lines: vec![
            song.to_string(),
            String::from("Stop it, or keep it paused for later (it stays bottom left)?"),
        ],
        choices: vec![
            choice("stop the music", "stop", Do::StopAndBack),
            choice("keep it paused", "pause", Do::Back),
        ],
        focus: 1,
        back: Do::Back,
    }
}

/// A video that does not play here, and why (`message`, the decoder's). OK and Back close it.
#[must_use]
pub fn video_failed(title: &str, message: &str) -> Overlay {
    Overlay {
        kind: Kind::Dialog,
        name: "video-failed",
        title: String::from("this video does not play here"),
        lines: vec![title.to_string(), message.to_string()],
        choices: vec![choice("ok", "check", Do::Back)],
        focus: 0,
        back: Do::Back,
    }
}

/// What went wrong (`title`) and the details (`lines`); OK closes it.
#[must_use]
pub fn error(title: &str, lines: Vec<String>) -> Overlay {
    Overlay {
        kind: Kind::Dialog,
        name: "error",
        title: title.to_string(),
        lines,
        choices: vec![choice("ok", "check", Do::Close)],
        focus: 0,
        back: Do::Close,
    }
}

/// Open an address: the field (Enter plays what it names), play, a sample, cancel.
#[must_use]
pub fn address() -> Overlay {
    Overlay {
        kind: Kind::Address,
        name: "address",
        title: String::from("open an address"),
        lines: vec![String::from(
            "An MP4 or MOV video (H.264) on a web server plays while it downloads; the sound \
             starts with the picture.",
        )],
        choices: vec![
            choice("play", "play_arrow", Do::PlayAddress),
            choice("try: Big Buck Bunny (10 s, 360p)", "movie", Do::PlaySample),
            choice("cancel", "close", Do::Close),
        ],
        focus: 0,
        back: Do::Close,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_focus_walks_the_choices_and_stops_at_either_end() {
        let mut o = more_info(
            "First Tone",
            vec![String::from("AzPlayer")],
            vec![
                choice("play", "play_arrow", Do::Open(3)),
                choice("add to queue", "queue_music", Do::Queue(3)),
            ],
        );
        assert_eq!(o.title, "first tone", "Media Center writes lower case");
        assert_eq!(o.chosen().map(|c| c.act.clone()), Some(Do::Open(3)));
        assert!(o.step(true));
        assert!(!o.step(true), "the last choice");
        assert_eq!(o.chosen().map(|c| c.label.as_str()), Some("add to queue"));
        o.cycle(true);
        assert_eq!(o.focus, 0, "Tab goes round");
        o.cycle(false);
        assert_eq!(o.focus, 1);
        assert!(o.point(0) && !o.point(0) && !o.point(9));
        assert_eq!(o.back, Do::Close);
    }

    #[test]
    fn a_question_that_destroys_something_starts_on_no() {
        let d = confirm_delete("/v/Holiday.mp4", "Holiday");
        assert_eq!(d.chosen().map(|c| c.act.clone()), Some(Do::Close), "no is focused");
        assert_eq!(d.choices[0].act, Do::Delete(String::from("/v/Holiday.mp4")));
        assert!(d.lines[1].contains("/v"), "it says where: {:?}", d.lines);
        let r = confirm_remove_folder(Shelf::Pictures, "/p/Trip");
        assert_eq!(r.chosen().map(|c| c.act.clone()), Some(Do::Close));
        assert_eq!(r.title, "remove this folder from pictures?");
        let p = music_paused("First Tone");
        assert_eq!(p.back, Do::Back, "Back keeps the music and goes back");
        assert_eq!(p.choices[0].act, Do::StopAndBack);
        assert_eq!(video_failed("x", "no decoder").back, Do::Back);
        assert_eq!(address().kind, Kind::Address);
        assert_eq!(about(Vec::new()).choices.len(), 1);
    }
}
