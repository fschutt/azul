//! The boards of a workspace: a list of boards (`id`, name, when made and last changed, the file it is kept
//! in), the order the person put them in, and the one that is open. The list is a small JSON file of its own
//! (the index, `azreview-boards/1`) beside the boards' files; each board is a zip as before.
//!
//! - **Files.** A board's file is named for its id (`<workspace>.<id>.azboard.zip`) and never renamed, so a
//!   board can be renamed, moved in the list or deleted without touching its file. The one-board version of
//!   the whiteboard kept `<workspace>.azboard.zip`: that file becomes "Board 1" in place ([`BoardIndex::migrated`]),
//!   not renamed, not copied, not rewritten.
//! - **Ids.** `b1`, `b2`, ...: a counter that never goes back, so an id is not given twice, not even after a
//!   delete. The index file's names of files are checked on reading: only the two forms above are boards of
//!   this workspace, nothing in a file can lead out of the folder the boards are kept in.
//! - **Delete.** Soft: the board is marked with the time it was deleted and is not shown; restoring clears the
//!   mark and the board is back where it was in the list. The app never removes a board's file.
//! - **Caps.** [`MAX_BOARDS`] shown boards, [`MAX_NAME_CHARS`] characters of a name, [`MAX_INDEX_BYTES`] of
//!   an index file; what would pass one is refused with a [`Notice`], a name is cut and told.
//!
//! Pure: no window, no azul, no disk. The owner (`whiteboard_app`) holds one and writes it as an `Effect`.

use crate::{
    whiteboard::Notice,
    whiteboard_json::{self as json, push_string, Json},
    whiteboard_store::board_name,
};

/// The format an index file names, and the one this version reads and writes.
pub const FORMAT: &str = "azreview-boards/1";
const FORMAT_PREFIX: &str = "azreview-boards/";
/// The newest major version of the index this version reads.
pub const FORMAT_MAJOR: u32 = 1;
/// How many boards a workspace shows.
pub const MAX_BOARDS: usize = 64;
/// How many characters a name of a board has.
pub const MAX_NAME_CHARS: usize = 80;
/// The largest index file that is read, bytes: a board's entry is a few hundred, so this is far more than any
/// list of [`MAX_BOARDS`] is.
pub const MAX_INDEX_BYTES: usize = 1 << 20;
/// The most digits of the number in an id (a counter of this many digits is never reached).
const MAX_ID_DIGITS: usize = 12;

/// One board of the workspace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoardMeta {
    /// `b1`, `b2`, ...: never given twice on one workspace.
    pub id: String,
    pub name: String,
    pub created_ms: u64,
    /// When the board (or its entry in the list) last changed.
    pub modified_ms: u64,
    /// The name of the board's zip among the app's files (see [`file_name`]).
    pub file: String,
    /// When it was deleted; it is not shown while this is set.
    pub deleted_ms: Option<u64>,
}

impl BoardMeta {
    #[must_use]
    pub const fn is_shown(&self) -> bool {
        self.deleted_ms.is_none()
    }
}

/// What a delete did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Deleted {
    /// The board that is open now.
    pub now_active: String,
    /// The board was the last one shown: a new, empty one took its place (and is the open one).
    pub replaced_by_new: bool,
}

/// Why an index file could not be used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BoardsError {
    /// It is no index of boards (and what is wrong with it).
    NotAnIndex(String),
    /// It was written by a newer major version: it is left as it is.
    TooNew { found: u32 },
    /// It is larger than any index is.
    TooLarge { bytes: usize },
}

impl BoardsError {
    /// The sentence in English.
    #[must_use]
    pub fn english(&self) -> String {
        match self {
            Self::NotAnIndex(why) => format!("The list of whiteboards of this folder could not be read: {why}."),
            Self::TooNew { found } => format!(
                "The list of whiteboards of this folder was saved by a newer AzReview (its format is {found}); it is left as it is."
            ),
            Self::TooLarge { bytes } => format!("The list of whiteboards of this folder has {bytes} bytes, far more than a list has."),
        }
    }

    /// The key of the sentence in `resources/*.ftl`.
    #[must_use]
    pub const fn key(&self) -> &'static str {
        match self {
            Self::NotAnIndex(_) => "azreview-whiteboard-boards-not-an-index",
            Self::TooNew { .. } => "azreview-whiteboard-boards-too-new",
            Self::TooLarge { .. } => "azreview-whiteboard-boards-too-large",
        }
    }

    /// What is wrong, for the sentence's `{ $why }`.
    #[must_use]
    pub fn why(&self) -> Option<&str> {
        match self {
            Self::NotAnIndex(why) => Some(why),
            _ => None,
        }
    }

    /// The numbers of the sentence.
    #[must_use]
    pub fn args(&self) -> Vec<(&'static str, u64)> {
        match self {
            Self::NotAnIndex(_) => Vec::new(),
            Self::TooNew { found } => vec![("found", u64::from(*found))],
            Self::TooLarge { bytes } => vec![("bytes", *bytes as u64)],
        }
    }
}

/// The part of the one-board file's name that the boards' names share: `boards/<folder>-<hash>`.
fn base(workspace: &str) -> String {
    let legacy = board_name(workspace);
    legacy.strip_suffix(".azboard.zip").map_or(legacy.clone(), str::to_string)
}

/// The name of the index file of the workspace at `workspace`, among the app's files.
#[must_use]
pub fn index_name(workspace: &str) -> String {
    format!("{}.azboards.json", base(workspace))
}

/// The name of the zip of board `id` of the workspace at `workspace`, among the app's files.
#[must_use]
pub fn file_name(workspace: &str, id: &str) -> String {
    format!("{}.{id}.azboard.zip", base(workspace))
}

/// Whether `id` is an id this module gives: `b`, then a number of at most [`MAX_ID_DIGITS`] digits with no
/// leading zero.
#[must_use]
pub fn valid_id(id: &str) -> bool {
    let Some(digits) = id.strip_prefix('b') else {
        return false;
    };
    !digits.is_empty()
        && digits.len() <= MAX_ID_DIGITS
        && digits.bytes().all(|b| b.is_ascii_digit())
        && !digits.starts_with('0')
}

/// The number in a valid id.
fn number_of(id: &str) -> u64 {
    id.strip_prefix('b').and_then(|d| d.parse().ok()).unwrap_or(0)
}

/// A name as it is kept: one line, no control characters, the words one space apart, at most
/// [`MAX_NAME_CHARS`] characters. Whether it was cut.
fn clean(name: &str) -> (String, bool) {
    let one_line: String = name.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
    let words = one_line.split_whitespace().collect::<Vec<_>>().join(" ");
    if words.chars().count() <= MAX_NAME_CHARS {
        return (words, false);
    }
    let cut: String = words.chars().take(MAX_NAME_CHARS).collect();
    (cut.trim_end().to_string(), true)
}

/// The list of boards of one workspace, and which is open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoardIndex {
    workspace: String,
    boards: Vec<BoardMeta>,
    active: String,
    /// The number the next id takes.
    next: u64,
}

impl BoardIndex {
    /// A workspace with no board yet: one, named `name`, in a file of its own.
    #[must_use]
    pub fn fresh(workspace: &str, name: &str, now_ms: u64) -> Self {
        let id = "b1".to_string();
        let meta = BoardMeta {
            file: file_name(workspace, &id),
            name: clean(name).0,
            id: id.clone(),
            created_ms: now_ms,
            modified_ms: now_ms,
            deleted_ms: None,
        };
        Self { workspace: workspace.to_string(), boards: vec![meta], active: id, next: 2 }
    }

    /// A workspace that has the one-board version's file: it is board `b1`, in that file, as it is.
    #[must_use]
    pub fn migrated(workspace: &str, name: &str, file_ms: u64) -> Self {
        let mut index = Self::fresh(workspace, name, file_ms);
        index.boards[0].file = board_name(workspace);
        index
    }

    /// The boards that are shown, in the order the person put them in.
    pub fn boards(&self) -> impl Iterator<Item = &BoardMeta> {
        self.boards.iter().filter(|b| b.is_shown())
    }

    /// Every board there is, the deleted ones too.
    pub fn all(&self) -> impl Iterator<Item = &BoardMeta> {
        self.boards.iter()
    }

    /// The board with `id`, shown or deleted.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&BoardMeta> {
        self.boards.iter().find(|b| b.id == id)
    }

    fn shown(&self, id: &str) -> Option<usize> {
        self.boards.iter().position(|b| b.id == id && b.is_shown())
    }

    /// The board that is open.
    #[must_use]
    pub fn active(&self) -> &BoardMeta {
        self.boards
            .iter()
            .find(|b| b.id == self.active && b.is_shown())
            .or_else(|| self.boards.iter().find(|b| b.is_shown()))
            .unwrap_or(&self.boards[0])
    }

    /// The number the next board's id takes (the number a default name carries).
    #[must_use]
    pub const fn next_number(&self) -> u64 {
        self.next
    }

    #[must_use]
    pub fn workspace(&self) -> &str {
        &self.workspace
    }

    fn room(&self) -> Result<(), Notice> {
        if self.boards().count() >= MAX_BOARDS {
            Err(Notice::TooManyBoards { limit: MAX_BOARDS })
        } else {
            Ok(())
        }
    }

    /// A new board at the end of the list (not opened: [`Self::set_active`]). Its id, and whether its name had to be cut.
    ///
    /// # Errors
    /// The workspace has as many boards as it may, or the name is empty.
    pub fn create(&mut self, name: &str, now_ms: u64) -> Result<(String, bool), Notice> {
        self.room()?;
        let (name, cut) = clean(name);
        if name.is_empty() {
            return Err(Notice::BoardNameEmpty);
        }
        let id = format!("b{}", self.next);
        self.next += 1;
        self.boards.push(BoardMeta {
            file: file_name(&self.workspace, &id),
            id: id.clone(),
            name,
            created_ms: now_ms,
            modified_ms: now_ms,
            deleted_ms: None,
        });
        Ok((id, cut))
    }

    /// Gives board `id` a new name. Whether the name had to be cut.
    ///
    /// # Errors
    /// There is no such board, or the name is empty.
    pub fn rename(&mut self, id: &str, name: &str, now_ms: u64) -> Result<bool, Notice> {
        let at = self.shown(id).ok_or(Notice::NoSuchBoard)?;
        let (name, cut) = clean(name);
        if name.is_empty() {
            return Err(Notice::BoardNameEmpty);
        }
        self.boards[at].name = name;
        self.boards[at].modified_ms = now_ms;
        Ok(cut)
    }

    /// The board's content was kept at `now_ms`.
    pub fn touch(&mut self, id: &str, now_ms: u64) {
        if let Some(b) = self.boards.iter_mut().find(|b| b.id == id) {
            b.modified_ms = now_ms;
        }
    }

    /// Opens board `id`.
    ///
    /// # Errors
    /// There is no such board (or it is deleted).
    pub fn set_active(&mut self, id: &str) -> Result<(), Notice> {
        self.shown(id).ok_or(Notice::NoSuchBoard)?;
        self.active = id.to_string();
        Ok(())
    }

    /// Deletes board `id`, softly. When it was the open one the next in the list (else the one before it) is
    /// open now; when it was the last one shown, a new empty board named `fresh_name` takes its place.
    ///
    /// # Errors
    /// There is no such board.
    pub fn delete(&mut self, id: &str, now_ms: u64, fresh_name: &str) -> Result<Deleted, Notice> {
        let at = self.shown(id).ok_or(Notice::NoSuchBoard)?;
        let was_open = self.active().id == id;
        let shown_before: Vec<usize> = (0..self.boards.len()).filter(|i| self.boards[*i].is_shown()).collect();
        self.boards[at].deleted_ms = Some(now_ms);
        if self.boards().next().is_none() {
            let (name, _) = clean(fresh_name);
            let name = if name.is_empty() { format!("Board {}", self.next) } else { name };
            let (new_id, _) = self.create_unchecked(&name, now_ms);
            self.active = new_id.clone();
            return Ok(Deleted { now_active: new_id, replaced_by_new: true });
        }
        if was_open {
            let pos = shown_before.iter().position(|i| *i == at).unwrap_or(0);
            let neighbour = shown_before.get(pos + 1).or_else(|| pos.checked_sub(1).and_then(|p| shown_before.get(p))).copied();
            if let Some(n) = neighbour {
                self.active = self.boards[n].id.clone();
            }
        }
        Ok(Deleted { now_active: self.active().id.clone(), replaced_by_new: false })
    }

    /// A board at the end of the list without the cap or the name's check (a replacement for the last one deleted).
    fn create_unchecked(&mut self, name: &str, now_ms: u64) -> (String, bool) {
        let id = format!("b{}", self.next);
        self.next += 1;
        self.boards.push(BoardMeta {
            file: file_name(&self.workspace, &id),
            id: id.clone(),
            name: name.to_string(),
            created_ms: now_ms,
            modified_ms: now_ms,
            deleted_ms: None,
        });
        (id, false)
    }

    /// Brings a deleted board back, where it was in the list.
    ///
    /// # Errors
    /// There is no such deleted board, or the workspace has as many boards as it may.
    pub fn restore(&mut self, id: &str) -> Result<(), Notice> {
        let at = self.boards.iter().position(|b| b.id == id && !b.is_shown()).ok_or(Notice::NoSuchBoard)?;
        self.room()?;
        self.boards[at].deleted_ms = None;
        Ok(())
    }

    /// Puts board `id` at place `to` among the boards that are shown (past the end is the end).
    ///
    /// # Errors
    /// There is no such board.
    pub fn move_to(&mut self, id: &str, to: usize) -> Result<(), Notice> {
        let at = self.shown(id).ok_or(Notice::NoSuchBoard)?;
        let moved = self.boards.remove(at);
        let shown: Vec<usize> = (0..self.boards.len()).filter(|i| self.boards[*i].is_shown()).collect();
        let place = match shown.get(to) {
            Some(p) => *p,
            None => shown.last().map_or(self.boards.len(), |p| p + 1),
        };
        self.boards.insert(place, moved);
        Ok(())
    }

    // ---- the file

    /// The index as the JSON it is kept in.
    #[must_use]
    pub fn to_json(&self) -> String {
        let mut out = String::new();
        out.push_str("{\n");
        out.push_str(&format!("  \"format\": \"{FORMAT}\",\n"));
        out.push_str("  \"workspace\": ");
        push_string(&mut out, &self.workspace);
        out.push_str(&format!(",\n  \"next\": {},\n  \"active\": ", self.next));
        push_string(&mut out, &self.active);
        out.push_str(",\n  \"boards\": [\n");
        for (n, b) in self.boards.iter().enumerate() {
            out.push_str("    {\"id\": ");
            push_string(&mut out, &b.id);
            out.push_str(", \"name\": ");
            push_string(&mut out, &b.name);
            out.push_str(&format!(", \"created_ms\": {}, \"modified_ms\": {}, \"file\": ", b.created_ms, b.modified_ms));
            push_string(&mut out, &b.file);
            if let Some(d) = b.deleted_ms {
                out.push_str(&format!(", \"deleted_ms\": {d}"));
            }
            out.push('}');
            if n + 1 < self.boards.len() {
                out.push(',');
            }
            out.push('\n');
        }
        out.push_str("  ]\n}\n");
        out
    }

    /// The index in `text`, for the workspace at `workspace`, and how many entries could not be used and were left
    /// out (an id that is not one or is there twice, a file that is not the board's own, more boards than the cap).
    ///
    /// # Errors
    /// The text is no index of this version, or is larger than any is.
    pub fn from_json(text: &str, workspace: &str) -> Result<(Self, usize), BoardsError> {
        if text.len() > MAX_INDEX_BYTES {
            return Err(BoardsError::TooLarge { bytes: text.len() });
        }
        let root = json::parse(text).map_err(|e| BoardsError::NotAnIndex(format!("{} (byte {})", e.why, e.at)))?;
        let Json::Obj(_) = root else {
            return Err(BoardsError::NotAnIndex("it is not an object".to_string()));
        };
        let format = root.get("format").and_then(Json::as_str).ok_or_else(|| BoardsError::NotAnIndex("it names no format".to_string()))?;
        let major = format
            .strip_prefix(FORMAT_PREFIX)
            .and_then(|v| v.parse::<u32>().ok())
            .filter(|m| *m >= 1)
            .ok_or_else(|| BoardsError::NotAnIndex(format!("its format \"{format}\" is not a list of boards")))?;
        if major > FORMAT_MAJOR {
            return Err(BoardsError::TooNew { found: major });
        }
        let legacy = board_name(workspace);
        let mut boards: Vec<BoardMeta> = Vec::new();
        let (mut dropped, mut shown, mut deleted, mut legacy_taken) = (0_usize, 0_usize, 0_usize, false);
        for entry in root.get("boards").and_then(Json::as_arr).unwrap_or(&[]) {
            let Some(id) = entry.get("id").and_then(Json::as_str).filter(|id| valid_id(id)) else {
                dropped += 1;
                continue;
            };
            let file = entry.get("file").and_then(Json::as_str).unwrap_or_default();
            let own = file == file_name(workspace, id);
            let old = file == legacy && !legacy_taken;
            if boards.iter().any(|b| b.id == id) || !(own || old) {
                dropped += 1;
                continue;
            }
            let deleted_ms = entry.get("deleted_ms").and_then(Json::as_u64);
            let room = if deleted_ms.is_none() { &mut shown } else { &mut deleted };
            if *room >= MAX_BOARDS {
                dropped += 1;
                continue;
            }
            *room += 1;
            legacy_taken |= old;
            let (name, _) = clean(entry.get("name").and_then(Json::as_str).unwrap_or_default());
            boards.push(BoardMeta {
                id: id.to_string(),
                name: if name.is_empty() { format!("Board {}", number_of(id)) } else { name },
                created_ms: entry.get("created_ms").and_then(Json::as_u64).unwrap_or(0),
                modified_ms: entry.get("modified_ms").and_then(Json::as_u64).unwrap_or(0),
                file: file.to_string(),
                deleted_ms,
            });
        }
        let Some(first) = boards.iter().find(|b| b.is_shown()).map(|b| b.id.clone()) else {
            return Err(BoardsError::NotAnIndex("it lists no board".to_string()));
        };
        let wanted = root.get("active").and_then(Json::as_str).unwrap_or_default();
        let active = boards.iter().find(|b| b.id == wanted && b.is_shown()).map_or(first, |b| b.id.clone());
        let needed = boards.iter().map(|b| number_of(&b.id)).max().unwrap_or(0) + 1;
        let next = root.get("next").and_then(Json::as_u64).unwrap_or(0).max(needed);
        Ok((Self { workspace: workspace.to_string(), boards, active, next }, dropped))
    }
}

#[cfg(test)]
#[path = "whiteboard_boards_test.rs"]
mod whiteboard_boards_test;
