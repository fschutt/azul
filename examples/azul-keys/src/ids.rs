//! Every DOM id and class AzKeys names, defined ONCE, each with the app's prefix `__azkeys_`
//! (the widgets' own names carry `__azul_`; user ruling 2026-10-02): no clash with a widget's or
//! another app's names, and no string literal repeated. A fixed name is a `const AzString`
//! (`AzString::from_const_str` borrows the static bytes); a name made at run time (a list row, a
//! custom field) is the prefix, its stem and the index, made by the one function here that knows
//! the stem. `scripts/azkeys_e2e.py` puts the same prefix in front of the same stems.

use azul::str::String as AzString;

/// The prefix of every name.
pub const PREFIX: &str = "__azkeys_";

macro_rules! names {
    ($($(#[$doc:meta])* $name:ident = $value:literal;)*) => {
        $($(#[$doc])* pub const $name: AzString = AzString::from_const_str(concat!("__azkeys_", $value));)*
    };
}

/// A name per row (of the list, the custom fields, the audit table): `<stem><index>`.
macro_rules! indexed {
    ($($(#[$doc:meta])* $name:ident = $stem:literal;)*) => {
        $($(#[$doc])* #[must_use] pub fn $name(index: usize) -> AzString {
            AzString::from(format!(concat!("__azkeys_", $stem, "{}"), index))
        })*
    };
}

names! {
    // ---- the unlock screen ----
    UNLOCK_SCREEN = "unlock";
    UNLOCK_VAULT = "unlock-vault";
    UNLOCK_PASSWORD = "unlock-password";
    UNLOCK_BUTTON = "unlock-button";
    UNLOCK_DEVICE = "unlock-device";
    UNLOCK_MESSAGE = "unlock-message";
    UNLOCK_HINT = "unlock-hint";
    UNLOCK_NEW = "unlock-new";
    // ---- a new vault ----
    CREATE_NAME = "create-name";
    CREATE_PASSWORD = "create-password";
    CREATE_CONFIRM = "create-confirm";
    CREATE_STRENGTH = "create-strength";
    CREATE_BUTTON = "create-button";
    CREATE_CANCEL = "create-cancel";
    CREATE_MESSAGE = "create-message";
    // ---- the toolbar, the navigation, the status bar ----
    TOOLBAR_NEW = "toolbar-new";
    TOOLBAR_NEW_KIND = "toolbar-new-kind";
    TOOLBAR_GENERATOR = "toolbar-generator";
    TOOLBAR_AUDIT = "toolbar-audit";
    TOOLBAR_IMPORT = "toolbar-import";
    TOOLBAR_LOCK = "toolbar-lock";
    TOOLBAR_SETTINGS = "toolbar-settings";
    NAV_NEW = "nav-new";
    STATUS = "status";
    // ---- the list ----
    LIST_SEARCH = "list-search";
    LIST_HEADING = "list-heading";
    LIST = "list";
    /// The class of an item's row.
    ROW_CLASS = "row";
    // ---- the item ----
    ITEM = "item";
    ITEM_TITLE = "item-title";
    ITEM_FAVORITE = "item-favorite";
    ITEM_USERNAME = "item-username";
    ITEM_PASSWORD = "item-password";
    ITEM_STRENGTH = "item-strength";
    ITEM_TOTP_CODE = "item-totp-code";
    ITEM_TOTP_RING = "item-totp-ring";
    ITEM_WEBSITE = "item-website";
    ITEM_NOTES = "item-notes";
    ITEM_HISTORY = "item-history";
    COPY_USERNAME = "copy-username";
    COPY_PASSWORD = "copy-password";
    COPY_TOTP = "copy-totp";
    COPY_CARD_NUMBER = "copy-card-number";
    COPY_CARD_CODE = "copy-card-code";
    REVEAL_PASSWORD = "reveal-password";
    ITEM_EDIT = "item-edit";
    ITEM_DELETE = "item-delete";
    ITEM_DELETE_CONFIRM = "item-delete-confirm";
    ITEM_DELETE_CANCEL = "item-delete-cancel";
    // ---- the edit form ----
    EDIT = "edit";
    EDIT_TITLE = "edit-title";
    EDIT_USERNAME = "edit-username";
    EDIT_PASSWORD = "edit-password";
    EDIT_PASSWORD_STRENGTH = "edit-password-strength";
    EDIT_GENERATE = "edit-generate";
    EDIT_TOTP = "edit-totp";
    EDIT_TOTP_PROBLEM = "edit-totp-problem";
    EDIT_WEBSITE = "edit-website";
    EDIT_TAG = "edit-tag";
    EDIT_NOTES = "edit-notes";
    EDIT_FAVORITE = "edit-favorite";
    EDIT_CARD_HOLDER = "edit-card-holder";
    EDIT_CARD_BRAND = "edit-card-brand";
    EDIT_CARD_NUMBER = "edit-card-number";
    EDIT_CARD_EXPIRY = "edit-card-expiry";
    EDIT_CARD_CODE = "edit-card-code";
    EDIT_ADD_FIELD = "edit-add-field";
    EDIT_SAVE = "edit-save";
    EDIT_CANCEL = "edit-cancel";
    EDIT_DISCARD = "edit-discard";
    EDIT_KEEP = "edit-keep";
    // ---- the generator ----
    GENERATOR = "generator";
    GEN_MODE = "gen-mode";
    GEN_OUTPUT = "gen-output";
    GEN_LENGTH = "gen-length";
    GEN_UPPER = "gen-upper";
    GEN_LOWER = "gen-lower";
    GEN_DIGITS = "gen-digits";
    GEN_SYMBOLS = "gen-symbols";
    GEN_SIMILAR = "gen-similar";
    GEN_CAPITALIZE = "gen-capitalize";
    GEN_ADD_DIGIT = "gen-add-digit";
    GEN_STRENGTH = "gen-strength";
    GEN_REFRESH = "gen-refresh";
    GEN_COPY = "gen-copy";
    GEN_USE = "gen-use";
    GEN_CLOSE = "gen-close";
    // ---- import ----
    IMPORT = "import";
    IMPORT_PATH = "import-path";
    IMPORT_CHOOSE = "import-choose";
    IMPORT_READ = "import-read";
    IMPORT_SUMMARY = "import-summary";
    IMPORT_RUN = "import-run";
    IMPORT_CANCEL = "import-cancel";
    // ---- the audit ----
    AUDIT = "audit";
    AUDIT_FILTER = "audit-filter";
    AUDIT_SUMMARY = "audit-summary";
    AUDIT_EXPORT = "audit-export";
    AUDIT_CLOSE = "audit-close";
    // ---- the settings ----
    SET_IDLE = "set-idle";
    SET_CLEAR = "set-clear";
    SET_DEVICE_UNLOCK = "set-device-unlock";
    SET_NEW_PASSWORD = "set-new-password";
    SET_NEW_CONFIRM = "set-new-confirm";
    SET_CHANGE = "set-change";
    SET_CHANGE_MESSAGE = "set-change-message";
}

indexed! {
    /// A row of the item list, by its position in the list.
    row = "row-";
    /// A custom field's copy button, by its position in the item.
    copy_field = "copy-field-";
    /// A hidden custom field's reveal button.
    reveal_field = "reveal-field-";
    /// A custom field's name in the edit form.
    edit_field_name = "edit-field-name-";
    /// A custom field's value in the edit form.
    edit_field_value = "edit-field-value-";
    /// A custom field's remove button in the edit form.
    edit_field_remove = "edit-field-remove-";
    /// A row of the audit table.
    audit_row = "audit-row-";
}
