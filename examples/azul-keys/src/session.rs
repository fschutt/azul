//! An unlocked vault and what the window does with it, without azul types (tested without a
//! window): the list's scope, search and selection, the reading pane (an item, its edit form,
//! the generator, the import preview, the audit), the edit form's draft and its checks, and the
//! changes that make a save due. Locking wipes it.

use crate::audit;
use crate::crypto::{Envelope, SecretKey};
use crate::generator::Options;
use crate::import::Imported;
use crate::totp::Totp;
use crate::vault::{view, Field, Item, Kind, Scope, Vault};

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
        let _ = (kind, now);
        todo!("GREEN")
    }

    /// The form of an existing item.
    #[must_use]
    pub fn edit(item: &Item) -> Form {
        let _ = item;
        todo!("GREEN")
    }

    /// Whether the draft holds changes (a new item: anything typed).
    #[must_use]
    pub fn changed(&self) -> bool {
        todo!("GREEN")
    }

    /// Sets the one-time field and says whether it can be used.
    pub fn set_totp(&mut self, text: &str) {
        let _ = text;
        todo!("GREEN")
    }

    /// Adds the typed tag (Enter or comma in the tag field).
    pub fn commit_tag(&mut self) {
        todo!("GREEN")
    }

    /// The item to keep at `now`: trimmed, dated, the old password kept in the history. `Err`
    /// says what to fix first (an empty title, a one-time field that is not one).
    pub fn finish(&self, now: u64) -> Result<Item, String> {
        let _ = now;
        todo!("GREEN")
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
        let _ = open;
        todo!("GREEN")
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
        let _ = id;
        todo!("GREEN")
    }

    /// After a scope or search change: the selection stays when the list still shows it, else
    /// the list's first item.
    pub fn keep_selection_in_view(&mut self) {
        todo!("GREEN")
    }

    /// Keeps the edit form's item in the vault; `Err` says what to fix. The item is selected and
    /// a save is due.
    pub fn save_form(&mut self, now: u64) -> Result<String, String> {
        let _ = now;
        todo!("GREEN")
    }

    /// Deletes the selected item (wiped); a save is due.
    pub fn delete_selected(&mut self, now: u64) -> bool {
        let _ = now;
        todo!("GREEN")
    }

    /// Stars or unstars the selected item; a save is due.
    pub fn toggle_favorite(&mut self, now: u64) {
        let _ = now;
        todo!("GREEN")
    }

    /// The text of a secret of the selected item, to copy or reveal, with its label for the
    /// clipboard countdown ("password of CodeHost"). `field` is "username", "password", "totp"
    /// (the current code at `now`), "card-number", "card-code" or "field-<n>".
    #[must_use]
    pub fn copy_text(&self, field: &str, now: u64) -> Option<(String, String)> {
        let _ = (field, now);
        todo!("GREEN")
    }

    /// Overwrites the vault's strings and the generator's secret (the vault locks).
    pub fn wipe(&mut self) {
        todo!("GREEN")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::KdfParams;

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
