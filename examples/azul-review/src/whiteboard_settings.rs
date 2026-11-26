//! The whiteboard's settings: what a published board says about who made it, how a link to code is made for a host of no
//! known form, and how long a circled part of the page stays armed. Three keys of the app's `settings.json`, read once; a
//! value that cannot be used is told in a note and the default is used, one that is out of range is held to the range
//! and told. Pure: the app gives the values, no file is read here.
//!
//! - `whiteboard-author`: the name a board and a snapshot are written with. `git` (the default) is the name git is
//!   configured with (`user.name`, never the e-mail address), `off` writes none, anything else is the name itself.
//!   The export dialog says which one it is, so nobody publishes a name without having been told.
//! - `whiteboard-permalink-template`: the form of a link to lines of code at a revision for a host that is not GitHub,
//!   GitLab, Bitbucket or Gitea, with `{web}` (the repository's web address), `{rev}`, `{path}`, `{start}` and `{end}`.
//!   Without it such a host gets the GitHub form.
//! - `whiteboard-armed-seconds`: how many seconds a circled part of the page stays armed, to be dragged to the whiteboard or
//!   sent with its button (8 by default; 2 to 60, a number with a fraction is fine).

use crate::whiteboard_drag::{ARM_MAX_MS, ARM_MIN_MS, ARM_MS};

/// The key of the author setting.
pub const KEY_AUTHOR: &str = "whiteboard-author";
/// The key of the template setting.
pub const KEY_PERMALINK: &str = "whiteboard-permalink-template";
/// The key of the setting for how long a circled part of the page stays armed, seconds.
pub const KEY_ARMED_SECONDS: &str = "whiteboard-armed-seconds";
/// The longest name or template that is taken, characters.
const MAX_NAME: usize = 200;
const MAX_TEMPLATE: usize = 500;

/// Whose name a board is written with.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Author {
    /// The name git is configured with (the default).
    #[default]
    Git,
    /// No name at all.
    Off,
    /// This name.
    Name(String),
}

/// The settings of the whiteboard.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    pub author: Author,
    /// The template for a link to code, `None` for the default form.
    pub permalink_template: Option<String>,
    /// How long a circled part of the page stays armed, ms (always inside [`ARM_MIN_MS`]..=[`ARM_MAX_MS`]).
    pub armed_ms: u64,
}

impl Default for Settings {
    fn default() -> Self {
        Self { author: Author::default(), permalink_template: None, armed_ms: ARM_MS }
    }
}

impl Settings {
    /// The settings as `get` finds them by key (`None` for a key that is not set). The notes say, in English, what could
    /// not be used and what is used instead.
    #[must_use]
    pub fn parse(get: &dyn Fn(&str) -> Option<String>) -> (Self, Vec<String>) {
        let mut notes = Vec::new();
        let author = match get(KEY_AUTHOR).map(|v| v.trim().to_string()) {
            None => Author::Git,
            Some(v) if v.is_empty() || v.eq_ignore_ascii_case("git") => Author::Git,
            Some(v) if v.eq_ignore_ascii_case("off") => Author::Off,
            Some(v) if v.chars().count() > MAX_NAME || v.chars().any(char::is_control) => {
                notes.push(format!("the setting {KEY_AUTHOR} is not a name of one line of {MAX_NAME} characters at most: using git's name"));
                Author::Git
            }
            Some(v) => Author::Name(v),
        };
        let permalink_template = match get(KEY_PERMALINK).map(|v| v.trim().to_string()) {
            None => None,
            Some(v) if v.is_empty() => None,
            Some(v) if v.chars().count() <= MAX_TEMPLATE && Self::template_usable(&v) => Some(v),
            Some(_) => {
                notes.push(format!(
                    "the setting {KEY_PERMALINK} must start with http:// or https:// (or be {{web}}) and hold {{rev}} and {{path}}, on one line of {MAX_TEMPLATE} characters at most: using the GitHub form"
                ));
                None
            }
        };
        let armed_ms = match get(KEY_ARMED_SECONDS).map(|v| v.trim().to_string()) {
            None => ARM_MS,
            Some(v) if v.is_empty() => ARM_MS,
            Some(v) => match v.parse::<f64>() {
                Ok(seconds) if seconds.is_finite() => {
                    let (min, max) = (ARM_MIN_MS as f64 / 1_000.0, ARM_MAX_MS as f64 / 1_000.0);
                    let ms = (seconds.clamp(min, max) * 1_000.0).round() as u64;
                    if seconds < min || seconds > max {
                        notes.push(format!("the setting {KEY_ARMED_SECONDS} is {seconds} seconds: a circled part stays armed {min} to {max} seconds, using {}", ms as f64 / 1_000.0));
                    }
                    ms
                }
                _ => {
                    notes.push(format!("the setting {KEY_ARMED_SECONDS} is not a number of seconds: using {}", ARM_MS / 1_000));
                    ARM_MS
                }
            },
        };
        (Self { author, permalink_template, armed_ms }, notes)
    }

    /// Whether `template` makes a link: an address that starts with the repository's own (`{web}`) or a scheme, and names
    /// the revision and the path.
    fn template_usable(template: &str) -> bool {
        (template.starts_with("{web}") || template.starts_with("https://") || template.starts_with("http://"))
            && template.contains("{rev}")
            && template.contains("{path}")
            && !template.chars().any(|c| c.is_control() || c.is_whitespace())
    }

    /// The name a board is written with, given the name git is configured with: `None` for no name.
    #[must_use]
    pub fn author_name(&self, git: Option<&str>) -> Option<String> {
        match &self.author {
            Author::Git => git.map(str::to_string),
            Author::Off => None,
            Author::Name(n) => Some(n.clone()),
        }
    }

    /// The value of the author setting, as it is kept.
    #[must_use]
    pub fn author_value(&self) -> String {
        match &self.author {
            Author::Git => "git".to_string(),
            Author::Off => "off".to_string(),
            Author::Name(n) => n.clone(),
        }
    }
}

#[cfg(test)]
#[path = "whiteboard_settings_test.rs"]
mod whiteboard_settings_test;
