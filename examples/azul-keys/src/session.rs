//! An unlocked vault and what the window does with it, without azul types (tested without a
//! window): the list's scope, search and selection, the reading pane (an item, its edit form,
//! the generator, the import preview, the audit), the edit form's draft and its checks, and the
//! changes that make a save due. Locking wipes it.

use crate::audit;
use crate::crypto::{Envelope, SecretKey};
use crate::generator::Options;
use crate::import::Imported;
use crate::totp::Totp;
use crate::vault::{view, Item, Kind, Scope, Vault};

/// The opened vault: its file, the file's header and sealed key, the vault key, the content.
pub struct OpenVault {
    pub key: String,
    pub envelope: Envelope,
    pub vault_key: SecretKey,
    pub vault: Vault,
}

/// The edit form: a draft of the item and the texts that are not part of it yet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Form {
    pub draft: Item,
    /// The item as it was (`None` for a new one).
    pub original: Option<Item>,
    /// The tag being typed.
    pub tag: String,
    /// Why the one-time field cannot be used ("" = it can, or it is empty).
    pub totp_problem: String,
    /// "Discard your changes?" is showing.
    pub confirm_discard: bool,
    /// The password field shows its text.
    pub reveal: bool,
}

impl Form {
    /// The form of a new item of `kind`.
    #[must_use]
    pub fn new_item(kind: Kind, now: u64) -> Form {
        Form {
            draft: Item::new(kind, "", now),
            original: None,
            tag: String::new(),
            totp_problem: String::new(),
            confirm_discard: false,
            reveal: false,
        }
    }

    /// The form of an existing item.
    #[must_use]
    pub fn edit(item: &Item) -> Form {
        Form {
            draft: item.clone(),
            original: Some(item.clone()),
            tag: String::new(),
            totp_problem: String::new(),
            confirm_discard: false,
            reveal: false,
        }
    }

    /// Whether the draft holds changes (a new item: anything typed).
    #[must_use]
    pub fn changed(&self) -> bool {
        match &self.original {
            Some(original) => *original != self.draft || !self.tag.trim().is_empty(),
            None => {
                let mut empty = Item::new(self.draft.kind, "", self.draft.created);
                empty.id = self.draft.id.clone();
                empty != self.draft || !self.tag.trim().is_empty()
            }
        }
    }

    /// Sets the one-time field and says whether it can be used.
    pub fn set_totp(&mut self, text: &str) {
        self.draft.totp = text.trim().to_string();
        self.totp_problem = if self.draft.totp.is_empty() {
            String::new()
        } else {
            Totp::parse(&self.draft.totp).err().unwrap_or_default()
        };
    }

    /// Adds the typed tag (Enter or comma in the tag field).
    pub fn commit_tag(&mut self) {
        let typed = std::mem::take(&mut self.tag);
        for tag in typed.split(',') {
            self.draft.add_tag(tag);
        }
    }

    /// The item to keep at `now`: trimmed, dated, the old password kept in the history. `Err`
    /// says what to fix first (an empty title, a one-time field that is not one).
    pub fn finish(&self, now: u64) -> Result<Item, String> {
        let mut item = self.draft.clone();
        item.title = item.title.trim().to_string();
        if item.title.is_empty() {
            return Err("Give the item a title.".to_string());
        }
        item.username = item.username.trim().to_string();
        item.totp = item.totp.trim().to_string();
        if !item.totp.is_empty() {
            if let Err(problem) = Totp::parse(&item.totp) {
                return Err(format!("The one-time code field: {problem}"));
            }
        }
        item.urls = item
            .urls
            .iter()
            .map(|u| u.trim().to_string())
            .filter(|u| !u.is_empty())
            .collect();
        item.fields
            .retain(|f| !f.name.trim().is_empty() || !f.value.is_empty());
        for tag in self.tag.split(',') {
            item.add_tag(tag);
        }
        match &self.original {
            Some(original) => {
                // The password the item had goes to its history (set_password keeps it).
                let typed = std::mem::take(&mut item.password);
                item.password = original.password.clone();
                item.history = original.history.clone();
                item.set_password(&typed, now);
            }
            None => {
                item.created = now;
                if !item.password.is_empty() {
                    item.password_changed = now;
                }
            }
        }
        item.modified = now;
        Ok(item)
    }
}

/// What a picked import file gave.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct ImportView {
    /// The file's path as typed or picked.
    pub path: String,
    /// The file is being read.
    pub reading: bool,
    /// The file's items, or why there are none.
    pub result: Option<Result<Imported, String>>,
}

/// The reading pane.
#[derive(Clone, Debug, PartialEq, Default)]
pub enum Reading {
    /// The selected item.
    #[default]
    Item,
    Edit(Form),
    Generator,
    Import(ImportView),
    Audit(audit::Filter),
}

/// What is revealed of the selected item (a secret shows only while revealed).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reveal {
    Password,
    CardNumber,
    CardCode,
    Field(usize),
}

/// An unlocked vault in the window.
pub struct Session {
    pub open: OpenVault,
    pub scope: Scope,
    pub query: String,
    /// The selected item's id.
    pub selected: Option<String>,
    pub reading: Reading,
    pub reveal: Option<Reveal>,
    /// "Delete this item?" is showing.
    pub confirm_delete: bool,
    pub generator: Options,
    /// The generator's last secret.
    pub generated: String,
    /// The vault changed since the last save started.
    pub dirty: bool,
}

impl Session {
    /// A session on an opened vault: all items, the first selected.
    #[must_use]
    pub fn new(open: OpenVault) -> Session {
        let mut session = Session {
            open,
            scope: Scope::All,
            query: String::new(),
            selected: None,
            reading: Reading::Item,
            reveal: None,
            confirm_delete: false,
            generator: Options::default(),
            generated: String::new(),
            dirty: false,
        };
        session.keep_selection_in_view();
        session
    }

    /// The list: the indices of the items in the scope that match the search, by title.
    #[must_use]
    pub fn view(&self) -> Vec<usize> {
        view(&self.open.vault, &self.scope, &self.query)
    }

    /// The selected item.
    #[must_use]
    pub fn selected_item(&self) -> Option<&Item> {
        self.selected
            .as_deref()
            .and_then(|id| self.open.vault.get(id))
    }

    /// Selects the item with `id` (nothing revealed, no question showing).
    pub fn select(&mut self, id: Option<String>) {
        self.selected = id;
        self.reveal = None;
        self.confirm_delete = false;
    }

    /// After a scope or search change: the selection stays when the list still shows it, else
    /// the list's first item.
    pub fn keep_selection_in_view(&mut self) {
        let list = self.view();
        let shown = self
            .selected
            .as_deref()
            .is_some_and(|id| list.iter().any(|&i| self.open.vault.items[i].id == id));
        if !shown {
            let first = list.first().map(|&i| self.open.vault.items[i].id.clone());
            self.select(first);
        }
    }

    /// Keeps the edit form's item in the vault; `Err` says what to fix. The item is selected and
    /// a save is due.
    pub fn save_form(&mut self, now: u64) -> Result<String, String> {
        let Reading::Edit(form) = &self.reading else {
            return Err("No item is being edited.".to_string());
        };
        let item = form.finish(now)?;
        let id = item.id.clone();
        self.open.vault.upsert(item, now);
        self.dirty = true;
        self.reading = Reading::Item;
        self.select(Some(id.clone()));
        Ok(id)
    }

    /// Deletes the selected item (wiped); a save is due.
    pub fn delete_selected(&mut self, now: u64) -> bool {
        let Some(id) = self.selected.clone() else {
            return false;
        };
        // The neighbour in the list takes the selection.
        let list = self.view();
        let position = list.iter().position(|&i| self.open.vault.items[i].id == id);
        let next = position.and_then(|p| {
            list.get(p + 1)
                .or_else(|| p.checked_sub(1).and_then(|q| list.get(q)))
                .map(|&i| self.open.vault.items[i].id.clone())
        });
        if !self.open.vault.remove(&id, now) {
            return false;
        }
        self.dirty = true;
        self.select(next);
        true
    }

    /// Stars or unstars the selected item; a save is due.
    pub fn toggle_favorite(&mut self, now: u64) {
        let Some(id) = self.selected.clone() else {
            return;
        };
        if let Some(item) = self.open.vault.get_mut(&id) {
            item.favorite = !item.favorite;
            item.modified = now;
            self.open.vault.modified = now;
            self.dirty = true;
        }
    }

    /// The text of a secret of the selected item, to copy or reveal, with its label for the
    /// clipboard countdown ("password of CodeHost"). `field` is "username", "password", "totp"
    /// (the current code at `now`), "card-number", "card-code" or "field-<n>".
    #[must_use]
    pub fn copy_text(&self, field: &str, now: u64) -> Option<(String, String)> {
        let item = self.selected_item()?;
        let of = |what: &str| format!("{what} of {}", item.title);
        let non_empty =
            |text: &str, what: &str| (!text.is_empty()).then(|| (text.to_string(), of(what)));
        match field {
            "username" => non_empty(item.username.as_str(), "user name"),
            "password" => non_empty(item.password.as_str(), "password"),
            "totp" => {
                let totp = Totp::parse(&item.totp).ok()?;
                Some((totp.code_at(now), of("one-time code")))
            }
            "card-number" => non_empty(item.card.number.as_str(), "card number"),
            "card-code" => non_empty(item.card.code.as_str(), "security code"),
            other => {
                let index: usize = other.strip_prefix("field-")?.parse().ok()?;
                let f = item.fields.get(index)?;
                non_empty(f.value.as_str(), f.name.to_lowercase().as_str())
            }
        }
    }

    /// Overwrites the vault's strings and the generator's secret (the vault locks).
    pub fn wipe(&mut self) {
        self.open.vault.wipe();
        zeroize::Zeroize::zeroize(&mut self.generated);
        zeroize::Zeroize::zeroize(&mut self.query);
        if let Reading::Edit(form) = &mut self.reading {
            form.draft.wipe();
            if let Some(original) = &mut form.original {
                original.wipe();
            }
        }
        self.reading = Reading::Item;
        self.select(None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::KdfParams;
    use crate::vault::Field;

    const NOW: u64 = 1_790_000_000;

    fn session() -> Session {
        let mut vault = Vault::new("Personal", 1);
        let mut a = Item::new(Kind::Login, "CodeHost", 1);
        a.username = "dev".to_string();
        a.password = "old-password".to_string();
        a.totp = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ".to_string();
        a.fields.push(Field {
            name: "PIN".into(),
            value: "4711".into(),
            hidden: true,
        });
        let b = Item::new(Kind::Note, "Alpha note", 1);
        vault.items = vec![a, b];
        let kdf = KdfParams::with_cost(64, 1, 1).expect("salt");
        let (envelope, vault_key) =
            Envelope::create(&vault.id, &vault.name, "pw", kdf, &vault.to_json()).expect("sealed");
        Session::new(OpenVault {
            key: "keys/vaults/x.azkv".into(),
            envelope,
            vault_key,
            vault,
        })
    }

    #[test]
    fn a_new_session_selects_the_first_item_of_the_list() {
        let s = session();
        assert_eq!(
            s.selected_item().map(|i| i.title.as_str()),
            Some("Alpha note")
        );
        assert!(!s.dirty);
    }

    #[test]
    fn the_selection_follows_the_search_when_its_item_leaves_the_list() {
        let mut s = session();
        s.query = "codehost".to_string();
        s.keep_selection_in_view();
        assert_eq!(
            s.selected_item().map(|i| i.title.as_str()),
            Some("CodeHost")
        );
        s.query = "nothing like this".to_string();
        s.keep_selection_in_view();
        assert!(s.selected.is_none());
    }

    #[test]
    fn editing_keeps_the_old_password_in_the_history_and_makes_a_save_due() {
        let mut s = session();
        let id = s.open.vault.items[0].id.clone();
        let mut form = Form::edit(&s.open.vault.items[0]);
        assert!(!form.changed());
        form.draft.password = "new-password".to_string();
        assert!(form.changed());
        s.reading = Reading::Edit(form);
        assert_eq!(s.save_form(NOW), Ok(id.clone()));
        let item = s.open.vault.get(&id).expect("kept");
        assert_eq!(item.password, "new-password");
        assert_eq!(item.history[0].password, "old-password");
        assert_eq!(item.modified, NOW);
        assert!(s.dirty);
        assert_eq!(s.reading, Reading::Item);
        assert_eq!(s.selected.as_deref(), Some(id.as_str()));
    }

    #[test]
    fn a_new_item_needs_a_title_and_a_usable_one_time_field() {
        let mut form = Form::new_item(Kind::Login, NOW);
        assert!(!form.changed());
        assert!(form.finish(NOW).is_err(), "no title");
        form.draft.title = "  Forge  ".to_string();
        assert!(form.changed());
        form.set_totp("not a secret");
        assert!(!form.totp_problem.is_empty());
        assert!(form.finish(NOW).is_err(), "a bad one-time field");
        form.set_totp("otpauth://totp/Forge?secret=JBSWY3DPEHPK3PXP");
        assert!(form.totp_problem.is_empty());
        let item = form.finish(NOW).expect("fine now");
        assert_eq!(item.title, "Forge");
        assert_eq!(item.created, NOW);
    }

    #[test]
    fn tags_are_committed_once_from_the_tag_field() {
        let mut form = Form::new_item(Kind::Login, NOW);
        form.tag = "work, ".to_string();
        form.commit_tag();
        form.tag = "#Work".to_string();
        form.commit_tag();
        assert_eq!(form.draft.tags, vec!["work"]);
        assert!(form.tag.is_empty());
    }

    #[test]
    fn copying_gives_the_secret_with_a_label_and_the_current_code() {
        let mut s = session();
        let id = s.open.vault.items[0].id.clone();
        s.select(Some(id));
        assert_eq!(
            s.copy_text("password", NOW),
            Some((
                "old-password".to_string(),
                "password of CodeHost".to_string()
            ))
        );
        assert_eq!(
            s.copy_text("totp", 59).map(|(code, _)| code),
            Some("287082".to_string())
        );
        assert_eq!(
            s.copy_text("field-0", NOW).map(|(v, _)| v),
            Some("4711".to_string())
        );
        assert_eq!(s.copy_text("field-9", NOW), None);
        assert_eq!(
            s.copy_text("card-number", NOW),
            None,
            "a login has no card number"
        );
    }

    #[test]
    fn deleting_and_starring_change_the_vault_and_make_a_save_due() {
        let mut s = session();
        let id = s.open.vault.items[0].id.clone();
        s.select(Some(id.clone()));
        s.toggle_favorite(NOW);
        assert!(s.open.vault.get(&id).expect("there").favorite);
        assert!(s.dirty);
        s.dirty = false;
        assert!(s.delete_selected(NOW));
        assert!(s.open.vault.get(&id).is_none());
        assert!(s.dirty);
        assert_eq!(
            s.selected_item().map(|i| i.title.as_str()),
            Some("Alpha note")
        );
    }

    #[test]
    fn locking_wipes_the_vault_and_the_generated_secret() {
        let mut s = session();
        s.generated = "secret".to_string();
        s.wipe();
        assert!(s.open.vault.items.is_empty());
        assert!(s.generated.is_empty());
        assert!(s.selected.is_none());
    }
}
