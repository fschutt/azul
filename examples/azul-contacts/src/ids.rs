//! Every DOM id and class AzContacts names, defined ONCE, each with the app's prefix
//! `__azcontacts_` (the widgets' own names carry `__azul_`; user ruling 2026-10-02): no clash with
//! a widget's or another app's names, and no string literal repeated. A fixed name is a
//! `const AzString` (`AzString::from_const_str` borrows the static bytes); a name made at run time
//! (a form row's field, a letter's section, an import row) is the prefix, its stem and the index,
//! made by the one function here that knows the stem. `scripts/azcontacts_e2e.py` puts the same
//! prefix in front of the same stems.

use azul::str::String as AzString;

/// The prefix of every name.
pub const PREFIX: &str = "__azcontacts_";

macro_rules! names {
    ($($(#[$doc:meta])* $name:ident = $value:literal;)*) => {
        $($(#[$doc])* pub const $name: AzString = AzString::from_const_str(concat!("__azcontacts_", $value));)*
    };
}

/// A name per row of the form (or of the import preview): `<stem><index>`.
macro_rules! indexed {
    ($($(#[$doc:meta])* $name:ident = $stem:literal;)*) => {
        $($(#[$doc])* #[must_use] pub fn $name(index: usize) -> AzString {
            AzString::from(format!(concat!("__azcontacts_", $stem, "{}"), index))
        })*
    };
}

names! {
    // ---- the toolbar and the status bar ----
    TOOLBAR_NEW = "toolbar-new";
    TOOLBAR_IMPORT = "toolbar-import";
    TOOLBAR_EXPORT = "toolbar-export";
    TOOLBAR_DUPLICATES = "toolbar-duplicates";
    TOOLBAR_SETTINGS = "toolbar-settings";
    CONTACTS_STATUS = "contacts-status";
    // ---- the navigation pane and the list ----
    CONTACTS_NEW = "contacts-new";
    CONTACTS_SEARCH = "contacts-search";
    CONTACTS_SORT = "contacts-sort";
    CONTACTS_HEADING = "contacts-heading";
    CONTACTS_LIST = "contacts-list";
    /// The A-Z bar.
    CONTACTS_JUMP = "contacts-jump";
    /// The class of a contact's row.
    CONTACT_ROW_CLASS = "contact-row";
    // ---- the contact card ----
    CONTACT_CARD = "contact-card";
    CARD_NAME = "card-name";
    CARD_FIELDS = "card-fields";
    CARD_EDIT = "card-edit";
    CARD_FAVORITE = "card-favorite";
    CARD_MAIL = "card-mail";
    CARD_COPY = "card-copy";
    CARD_EXPORT = "card-export";
    CARD_DELETE = "card-delete";
    CARD_DELETE_CONFIRM = "card-delete-confirm";
    CARD_DELETE_CANCEL = "card-delete-cancel";
    // ---- the edit form ----
    CONTACT_EDIT = "contact-edit";
    EDIT_GIVEN = "edit-given";
    EDIT_FAMILY = "edit-family";
    EDIT_ORG = "edit-org";
    EDIT_DEPARTMENT = "edit-department";
    EDIT_TITLE = "edit-title";
    EDIT_NICKNAME = "edit-nickname";
    EDIT_PHOTO = "edit-photo";
    EDIT_PHOTO_REMOVE = "edit-photo-remove";
    EDIT_ADD_PHONE = "edit-add-phone";
    EDIT_ADD_EMAIL = "edit-add-email";
    EDIT_ADD_ADDRESS = "edit-add-address";
    EDIT_BIRTHDAY = "edit-birthday";
    EDIT_BIRTHDAY_ADD = "edit-birthday-add";
    EDIT_BIRTHDAY_PICKER = "edit-birthday-picker";
    EDIT_BIRTHDAY_NO_YEAR = "edit-birthday-no-year";
    EDIT_BIRTHDAY_YEAR = "edit-birthday-year";
    EDIT_NEW_GROUP = "edit-new-group";
    EDIT_ADD_GROUP = "edit-add-group";
    EDIT_ADD_FIELD = "edit-add-field";
    EDIT_NOTES = "edit-notes";
    EDIT_FAVORITE = "edit-favorite";
    EDIT_PROBLEMS = "edit-problems";
    EDIT_CANCEL = "edit-cancel";
    EDIT_SAVE = "edit-save";
    /// "Discard your changes?" and its answers.
    EDIT_DISCARD = "edit-discard";
    EDIT_KEEP = "edit-keep";
    // ---- the import preview ----
    CONTACT_IMPORT = "contact-import";
    IMPORT_PATH = "import-path";
    IMPORT_READ = "import-read";
    IMPORT_CHOOSE = "import-choose";
    IMPORT_COLUMNS = "import-columns";
    IMPORT_SUMMARY = "import-summary";
    IMPORT_ROWS = "import-rows";
    IMPORT_GROUP = "import-group";
    IMPORT_CANCEL = "import-cancel";
    IMPORT_RUN = "import-run";
    // ---- the merge screen ----
    CONTACT_MERGE = "contact-merge";
    MERGE_EMPTY = "merge-empty";
    MERGE_PREV = "merge-prev";
    MERGE_NEXT = "merge-next";
    MERGE_POSITION = "merge-position";
    MERGE_PAIR = "merge-pair";
    MERGE_NAME = "merge-name";
    MERGE_COMPANY = "merge-company";
    MERGE_BIRTHDAY = "merge-birthday";
    MERGE_PHOTO = "merge-photo";
    MERGE_NOTES = "merge-notes";
    MERGE_NOTES_BOTH = "merge-notes-both";
    MERGE_IGNORE = "merge-ignore";
    MERGE_RUN = "merge-run";
    // ---- the settings ----
    SET_SORT = "set-sort";
    SET_EXPORT_VERSION = "set-export-version";
}

indexed! {
    // ---- the edit form's rows (index = the row) ----
    edit_phone_label = "edit-phone-label-";
    edit_phone = "edit-phone-";
    edit_phone_remove = "edit-phone-remove-";
    edit_email_label = "edit-email-label-";
    edit_email = "edit-email-";
    edit_email_remove = "edit-email-remove-";
    edit_address_label = "edit-address-label-";
    edit_street = "edit-street-";
    edit_address_remove = "edit-address-remove-";
    edit_postcode = "edit-postcode-";
    edit_city = "edit-city-";
    edit_region = "edit-region-";
    edit_country = "edit-country-";
    edit_field_label = "edit-field-label-";
    edit_field = "edit-field-";
    edit_field_remove = "edit-field-remove-";
    // ---- the import preview (index = the file's column / row) ----
    import_column = "import-column-";
    import_row = "import-row-";
}

/// A letter as it stands in a name (`#` is `hash`).
fn letter_name(letter: char) -> String {
    if letter == '#' {
        "hash".to_string()
    } else {
        letter.to_string()
    }
}

/// The list's section of a letter (`section-A`, `section-hash`): the A-Z bar scrolls to it.
#[must_use]
pub fn section(letter: char) -> AzString {
    AzString::from(format!("{PREFIX}section-{}", letter_name(letter)))
}

/// The A-Z bar's letter (`jump-A`, `jump-hash`).
#[must_use]
pub fn jump(letter: char) -> AzString {
    AzString::from(format!("{PREFIX}jump-{}", letter_name(letter)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_name_carries_the_app_prefix() {
        for name in [TOOLBAR_NEW, CONTACT_ROW_CLASS, CARD_NAME, EDIT_SAVE, IMPORT_RUN, SET_SORT] {
            assert!(name.as_str().starts_with(PREFIX), "{}", name.as_str());
        }
        assert_eq!(edit_phone(2).as_str(), "__azcontacts_edit-phone-2");
        assert_eq!(import_column(0).as_str(), "__azcontacts_import-column-0");
    }

    #[test]
    fn the_section_ids_name_the_letters() {
        assert_eq!(section('A').as_str(), "__azcontacts_section-A");
        assert_eq!(section('#').as_str(), "__azcontacts_section-hash");
        assert_eq!(jump('#').as_str(), "__azcontacts_jump-hash");
    }
}
