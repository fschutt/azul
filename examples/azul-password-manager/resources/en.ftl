# AzPasswordManager's words in English (src/l10n.rs). Every key the source names is here and in de.ftl.

## The window

azpasswordmanager-about-summary = Your passwords, cards, notes, identities, SSH keys and one-time codes in an encrypted vault file (Argon2id and XChaCha20-Poly1305), unlocked with a master password or, on this device, with biometrics. Part of the Azlin apps, built with azul.
azpasswordmanager-category-security = Security
azpasswordmanager-category-vault = Vault
azpasswordmanager-items = Items
azpasswordmanager-close-question = Save the item?
azpasswordmanager-close-text = Your changes to the item are lost if you don't save them.
azpasswordmanager-close-discard = Don't Save
azpasswordmanager-save = Save
azpasswordmanager-close = Close

## Unlock and a new vault

azpasswordmanager-unlock-looking = Looking for your vaults
azpasswordmanager-unlock-none = No vault yet
azpasswordmanager-unlock-none-detail = A vault keeps your passwords in one encrypted file in your data folder.
azpasswordmanager-create-vault-action = Create a vault
azpasswordmanager-unlock-title = Unlock “{ $name }”
azpasswordmanager-vault = Vault
azpasswordmanager-password = Password
azpasswordmanager-master-password = Master password
azpasswordmanager-wait-seconds = Wait { $seconds } s
azpasswordmanager-unlock = Unlock
azpasswordmanager-wait-for-answer = Wait for the answer
azpasswordmanager-unlock-keyring = Unlock from the keyring
azpasswordmanager-use-biometrics = Use biometrics
azpasswordmanager-sample-hint = This is the sample vault: its master password is “{ $password }”.
azpasswordmanager-new-vault = New vault
azpasswordmanager-type-master-password = Type the master password.
azpasswordmanager-unlocking = Unlocking…
azpasswordmanager-wrong-password = Wrong password.
azpasswordmanager-wrong-password-wait = Wrong password. Wait { $seconds } s before the next try.
azpasswordmanager-wrong-password-left =
    { $count ->
        [one] Wrong password. 1 attempt left before a { $seconds } s wait.
       *[other] Wrong password. { $count } attempts left before a { $seconds } s wait.
    }
azpasswordmanager-create-title = A new vault
azpasswordmanager-create-note = One encrypted file in your data folder. Nobody - not even AzPasswordManager - can open it without the master password: choose one you will remember, and keep it written down somewhere safe.
azpasswordmanager-name = Name
azpasswordmanager-create-name-placeholder = Personal
azpasswordmanager-again = Again
azpasswordmanager-same-password-again = The same password again
azpasswordmanager-create-vault = Create vault
azpasswordmanager-create-crypto-note = The master password derives the key with Argon2id (64 MiB, 3 passes); the vault is sealed with XChaCha20-Poly1305.
azpasswordmanager-making-vault = Making the vault…
azpasswordmanager-making-sample = Making the sample vault…
azpasswordmanager-create-needs-name = Give the vault a name.
azpasswordmanager-password-too-short = The master password needs 8 characters or more.
azpasswordmanager-passwords-differ = The two passwords differ.

## The vault

azpasswordmanager-nav-all = All items ({ $count })
azpasswordmanager-nav-favourites = Favourites ({ $count })
azpasswordmanager-nav-codes = One-time codes ({ $count })
azpasswordmanager-categories = Categories
azpasswordmanager-tags = Tags
azpasswordmanager-new-item = New item
azpasswordmanager-nav-label = Vault, categories and tags
azpasswordmanager-scope-all = All items
azpasswordmanager-scope-favourites = Favourites
azpasswordmanager-scope-codes = One-time codes
azpasswordmanager-search = Search all items
azpasswordmanager-empty-vault = An empty vault
azpasswordmanager-empty-vault-detail = Add a login, or import a browser's or a password manager's export.
azpasswordmanager-nothing-here = Nothing here
azpasswordmanager-nothing-in-view = No item is in this view.
azpasswordmanager-no-match = No item matches “{ $query }”.
azpasswordmanager-toolbar-new = New
azpasswordmanager-toolbar-generator = Generator
azpasswordmanager-toolbar-audit = Audit
azpasswordmanager-toolbar-import = Import
azpasswordmanager-toolbar-lock = Lock
azpasswordmanager-toolbar-settings = Settings
azpasswordmanager-status-items =
    { $count ->
        [one] 1 item
       *[other] { $count } items
    }
azpasswordmanager-saving = Saving…
azpasswordmanager-status-locks-by-hand = Locks only by hand
azpasswordmanager-status-locks-in = Locks in { $time }
azpasswordmanager-status-locks-after = Locks after { $time } idle
azpasswordmanager-status-clipboard = Clipboard clears in { $seconds } s

## Kinds

azpasswordmanager-kinds-login = Logins
azpasswordmanager-kinds-card = Cards
azpasswordmanager-kinds-note = Secure notes
azpasswordmanager-kinds-identity = Identities
azpasswordmanager-kinds-ssh-key = SSH keys
azpasswordmanager-kind-login = login
azpasswordmanager-kind-card = card
azpasswordmanager-kind-note = secure note
azpasswordmanager-kind-identity = identity
azpasswordmanager-kind-ssh-key = SSH key
azpasswordmanager-kind-choice-login = Login
azpasswordmanager-kind-choice-card = Card
azpasswordmanager-kind-choice-note = Secure note
azpasswordmanager-kind-choice-identity = Identity
azpasswordmanager-kind-choice-ssh-key = SSH key
azpasswordmanager-new-login = New login
azpasswordmanager-new-card = New card
azpasswordmanager-new-note = New secure note
azpasswordmanager-new-identity = New identity
azpasswordmanager-new-ssh-key = New SSH key
azpasswordmanager-edit-login = Edit login
azpasswordmanager-edit-card = Edit card
azpasswordmanager-edit-note = Edit secure note
azpasswordmanager-edit-identity = Edit identity
azpasswordmanager-edit-ssh-key = Edit SSH key
azpasswordmanager-count-login =
    { $count ->
        [one] 1 login
       *[other] { $count } logins
    }
azpasswordmanager-count-card =
    { $count ->
        [one] 1 card
       *[other] { $count } cards
    }
azpasswordmanager-count-note =
    { $count ->
        [one] 1 secure note
       *[other] { $count } secure notes
    }
azpasswordmanager-count-identity =
    { $count ->
        [one] 1 identity
       *[other] { $count } identities
    }
azpasswordmanager-count-ssh-key =
    { $count ->
        [one] 1 SSH key
       *[other] { $count } SSH keys
    }

## An item

azpasswordmanager-no-item = No item selected
azpasswordmanager-no-item-detail = Pick an item in the list, or add a new one.
azpasswordmanager-copy = Copy
azpasswordmanager-hide = Hide
azpasswordmanager-show = Show
azpasswordmanager-field-username = user name
azpasswordmanager-field-password = password
azpasswordmanager-field-one-time = one-time
azpasswordmanager-field-holder = holder
azpasswordmanager-field-brand = brand
azpasswordmanager-field-number = number
azpasswordmanager-field-expires = expires
azpasswordmanager-field-security-code = security code
azpasswordmanager-field-website = website
azpasswordmanager-field-tags = tags
azpasswordmanager-field-notes = notes
azpasswordmanager-field-history = history
azpasswordmanager-strength-changed = { $strength }, changed { $date }
azpasswordmanager-seconds-to-next-code = Seconds until the next code
azpasswordmanager-history =
    { $count ->
        [one] 1 earlier password (the last until { $date })
       *[other] { $count } earlier passwords (the last until { $date })
    }
azpasswordmanager-created-changed = Created { $created }, changed { $changed }
azpasswordmanager-delete-ask = Delete “{ $title }”?
azpasswordmanager-delete = Delete
azpasswordmanager-keep-it = Keep it
azpasswordmanager-edit = Edit
azpasswordmanager-copy-of = { $what } of { $title }
azpasswordmanager-copy-username = user name
azpasswordmanager-copy-password = password
azpasswordmanager-copy-code = one-time code
azpasswordmanager-copy-card-number = card number
azpasswordmanager-copy-card-code = security code

## The edit form

azpasswordmanager-kind-label = Kind
azpasswordmanager-title = Title
azpasswordmanager-username = User name
azpasswordmanager-username-placeholder = User name or email
azpasswordmanager-generate = Generate
azpasswordmanager-one-time-code = One-time code
azpasswordmanager-totp-placeholder = otpauth://totp/... or the base32 secret
azpasswordmanager-website = Website
azpasswordmanager-holder = Holder
azpasswordmanager-holder-placeholder = Name on the card
azpasswordmanager-brand = Brand
azpasswordmanager-brand-placeholder = Visa, Mastercard, ...
azpasswordmanager-number = Number
azpasswordmanager-card-number = Card number
azpasswordmanager-expires = Expires
azpasswordmanager-expiry-placeholder = MM/YYYY
azpasswordmanager-security-code = Security code
azpasswordmanager-field = Field
azpasswordmanager-value = Value
azpasswordmanager-hidden = Hidden
azpasswordmanager-add-field = Add a field
azpasswordmanager-add-tag = Add a tag (Enter)
azpasswordmanager-notes = Notes
azpasswordmanager-favourite = Favourite
azpasswordmanager-discard-ask = Discard your changes?
azpasswordmanager-discard = Discard
azpasswordmanager-keep-editing = Keep editing
azpasswordmanager-item-needs-title = Give the item a title.
azpasswordmanager-totp-field = The one-time code field: { $problem }
azpasswordmanager-nothing-edited = No item is being edited.
azpasswordmanager-totp-empty = The one-time code field is empty.
azpasswordmanager-totp-not-a-secret = This is neither an otpauth:// URL nor a base32 secret of 16 characters or more.
azpasswordmanager-totp-hotp = Counter-based (HOTP) codes are not supported; only time-based (TOTP) codes.
azpasswordmanager-totp-not-totp = The otpauth URL is not for a time-based code (totp).
azpasswordmanager-totp-secret-not-base32 = The secret in the otpauth URL is not base32.
azpasswordmanager-totp-hash = The otpauth URL names a hash other than SHA1, SHA256 or SHA512.
azpasswordmanager-totp-digits = The otpauth URL asks for codes of other than 6 to 8 digits.
azpasswordmanager-totp-period = The otpauth URL's period is not 1 to 3600 seconds.
azpasswordmanager-totp-no-secret = The otpauth URL has no secret.

## The generator

azpasswordmanager-generator-title = Generate a password
azpasswordmanager-mode-password = Password
azpasswordmanager-mode-passphrase = Passphrase
azpasswordmanager-mode-pin = PIN
azpasswordmanager-regenerate = Again
azpasswordmanager-length = Length
azpasswordmanager-digits = Digits
azpasswordmanager-words = Words
azpasswordmanager-avoid-similar = avoid similar (Il1O0)
azpasswordmanager-capitals = Capitals
azpasswordmanager-digit-at-end = A digit at the end
azpasswordmanager-strength = Strength
azpasswordmanager-strength-very-weak = very weak
azpasswordmanager-strength-weak = weak
azpasswordmanager-strength-medium = medium
azpasswordmanager-strength-strong = strong
azpasswordmanager-strength-very-strong = very strong
azpasswordmanager-strength-bits = { $strength } (~{ $bits } bits)
azpasswordmanager-generator-note = The edit form's Generate button uses these settings. Drawn from the system's random source; nothing is kept.
azpasswordmanager-generated-password = generated password
azpasswordmanager-copied-generated = Copied the generated password

## Import

azpasswordmanager-import-title = Import passwords
azpasswordmanager-import-help = A CSV export of Chrome, Edge, Firefox, Safari, Bitwarden, 1Password, KeePassXC or LastPass, or Bitwarden's JSON export (unencrypted). Delete the export file once it is imported: it holds your passwords in plain text.
azpasswordmanager-file = File
azpasswordmanager-export-path = The export's path
azpasswordmanager-choose = Choose…
azpasswordmanager-read = Read
azpasswordmanager-reading-file = Reading the file…
azpasswordmanager-import-summary =
    { $count ->
        [one] { $format }: 1 item ({ $kinds })
       *[other] { $format }: { $count } items ({ $kinds })
    }
azpasswordmanager-import-summary-skipped =
    { $count ->
        [one] { $format }: 1 item ({ $kinds }); { $skipped } skipped
       *[other] { $format }: { $count } items ({ $kinds }); { $skipped } skipped
    }
azpasswordmanager-import-run =
    { $count ->
        [one] Import 1 item
       *[other] Import { $count } items
    }
azpasswordmanager-type-export-path = Type the export's path, or choose the file.
azpasswordmanager-imported =
    { $count ->
        [one] Imported 1 item
       *[other] Imported { $count } items
    }
azpasswordmanager-imported-known =
    { $count ->
        [one] Imported 1 item ({ $known } already in the vault)
       *[other] Imported { $count } items ({ $known } already in the vault)
    }
azpasswordmanager-import-bad-name = This is not a file name AzPasswordManager can read.
azpasswordmanager-import-missing = The file does not exist.
azpasswordmanager-import-unreadable = The file cannot be read: { $why }
azpasswordmanager-import-not-read = The file was not read.
azpasswordmanager-import-no-column = No column of this file is one AzPasswordManager imports: it needs a header row naming name or title, url, username, password or notes.
azpasswordmanager-import-row-empty = row { $row }: nothing to import (no name, website, user name, password or note)
azpasswordmanager-import-not-json = This is not a JSON file (line { $line }, column { $column }).
azpasswordmanager-import-not-bitwarden = This JSON file is not a Bitwarden export.
azpasswordmanager-import-encrypted = This is an encrypted Bitwarden export. Export the vault again choosing "JSON" (unencrypted), import that file, then delete it.
azpasswordmanager-import-no-items = This JSON file has no "items": it is not a Bitwarden export.
azpasswordmanager-import-unknown-type = item { $item }: a type AzPasswordManager does not know
azpasswordmanager-import-not-utf8 = The file is not UTF-8 text: save it as a UTF-8 CSV or as JSON.

## The audit

azpasswordmanager-audit-title = Security audit
azpasswordmanager-audit-all = All ({ $count })
azpasswordmanager-audit-weak = Weak ({ $count })
azpasswordmanager-audit-reused = Reused ({ $count })
azpasswordmanager-audit-old = Old ({ $count })
azpasswordmanager-audit-no-two-factor = No 2FA ({ $count })
azpasswordmanager-audit-summary = { $weak } weak, { $reused } reused, { $old } older than two years, { $none } without a one-time code. Nothing is checked online.
azpasswordmanager-audit-nothing = Nothing to fix here.
azpasswordmanager-export-report = Export report
azpasswordmanager-problem-weak = weak password
azpasswordmanager-problem-reused = reused in { $count } items
azpasswordmanager-problem-old = older than 2 years
azpasswordmanager-problem-no-two-factor = no one-time code

## Settings

azpasswordmanager-section-security = Security
azpasswordmanager-lock-after = Lock after
azpasswordmanager-clear-clipboard = Clear the clipboard
azpasswordmanager-clear-clipboard-after = Clear the clipboard after
azpasswordmanager-device-unlock = Device unlock
azpasswordmanager-device-unlock-switch = Unlock this vault on this device without the password
azpasswordmanager-device-unlock-note = Keeps this vault's key in the system keyring, behind biometrics where the system has them (Touch ID, Windows Hello, fprintd), so this device unlocks the vault without the master password.
azpasswordmanager-device-unlock-locked = Unlock a vault to let this device unlock it with biometrics.
azpasswordmanager-privacy-note = AzPasswordManager makes no network requests: no breach checks, no website icons. A copied secret is cleared from the clipboard when the countdown ends, even if something else was copied since (azul cannot read the clipboard outside a paste).
azpasswordmanager-key-derivation = Key derivation
azpasswordmanager-kdf-value =
    { $lanes ->
        [one] Argon2id, { $mib } MiB, { $passes } passes, 1 lane; XChaCha20-Poly1305
       *[other] Argon2id, { $mib } MiB, { $passes } passes, { $lanes } lanes; XChaCha20-Poly1305
    }
azpasswordmanager-new-password = New password
azpasswordmanager-new-master-password = New master password
azpasswordmanager-change-password = Change the master password
azpasswordmanager-change-note = The vault key stays the same: a device that unlocks the vault with biometrics keeps doing so. The file before each save is kept in keys/backups/.
azpasswordmanager-change-locked = Unlock a vault to see its file and change its master password.
azpasswordmanager-import-note = Import: CSV exports of Chrome, Edge, Firefox, Safari, Bitwarden, 1Password, KeePassXC and LastPass, and Bitwarden's unencrypted JSON export (the toolbar's Import). OpenPGP keys are a later step.
azpasswordmanager-wait-for-save = Wait for the save on the way, then try again.
azpasswordmanager-changing-password = Changing the master password…
azpasswordmanager-never = Never
azpasswordmanager-seconds =
    { $count ->
        [one] 1 second
       *[other] { $count } seconds
    }
azpasswordmanager-minutes =
    { $count ->
        [one] 1 minute
       *[other] { $count } minutes
    }
azpasswordmanager-hours =
    { $count ->
        [one] 1 hour
       *[other] { $count } hours
    }

## Shortcuts

azpasswordmanager-shortcuts-vault = Vault
azpasswordmanager-shortcuts-item = Item
azpasswordmanager-shortcuts-unlock = Unlock
azpasswordmanager-shortcut-lock = Lock the vault
azpasswordmanager-shortcut-generator = Password generator
azpasswordmanager-shortcut-previous-next = Previous / next item
azpasswordmanager-shortcut-copy-password = Copy the password of the selected login
azpasswordmanager-shortcut-copy-username = Copy the user name
azpasswordmanager-shortcut-copy-code = Copy the one-time code
azpasswordmanager-shortcut-edit = Edit the selected item
azpasswordmanager-shortcut-save = Save the item being edited
azpasswordmanager-shortcut-cancel = Cancel editing, close a panel
azpasswordmanager-shortcut-delete = Delete the selected item (asks first)
azpasswordmanager-shortcut-unlock = Unlock with the master password

## Notices

azpasswordmanager-not-vaults =
    { $count ->
        [one] 1 file in the vault folder is not a vault
       *[other] { $count } files in the vault folder are not vaults
    }
azpasswordmanager-password-changed = The master password is changed.
azpasswordmanager-written-to = Written to { $path }
azpasswordmanager-key-stale = The key kept on this device no longer opens this vault. Unlock it with the master password.
azpasswordmanager-not-saved = Not saved: { $why }
azpasswordmanager-password-not-changed = The password is not changed: { $why }
azpasswordmanager-create-failed = The vault could not be made: { $why }
azpasswordmanager-open-failed = The vault cannot be opened: { $why }
azpasswordmanager-failed = That did not work: { $why }
azpasswordmanager-saving-then-locking = Saving, then locking…
azpasswordmanager-copied = Copied the { $label }
azpasswordmanager-copied-clears = Copied the { $label }; the clipboard clears in { $seconds } s
azpasswordmanager-clipboard-cleared = The clipboard is cleared.
azpasswordmanager-keyring-busy = The system keyring is still answering; try again in a moment.
azpasswordmanager-biometric-reason = Unlock the vault “{ $name }”
azpasswordmanager-use-password = Use the password
azpasswordmanager-waiting-keyring = Waiting for the system keyring…
azpasswordmanager-no-device-key = This device keeps no key of this vault yet: unlock it with the master password, then turn on device unlock in the settings.
azpasswordmanager-keyring-not-found = the keyring has no key of this vault
azpasswordmanager-keyring-denied = the prompt was cancelled or refused
azpasswordmanager-keyring-unavailable = no system keyring is available
azpasswordmanager-keyring-error = the keyring reported an error
azpasswordmanager-device-unlock-keyring-on = This device now unlocks the vault from the system keyring.
azpasswordmanager-device-unlock-biometric-on = This device now unlocks the vault with biometrics.
azpasswordmanager-device-unlock-not-on = Device unlock is not on: { $why }.
azpasswordmanager-keyring-not-a-key = The keyring's entry is not a vault key. Unlock with the master password.
azpasswordmanager-device-key-gone = This device no longer keeps the key of this vault. Unlock it with the master password.
azpasswordmanager-device-unlock-failed = Device unlock did not work: { $why }.
azpasswordmanager-device-unlock-off = This device no longer unlocks the vault without its password.
azpasswordmanager-biometric-cancelled = Cancelled: unlock with the master password.
azpasswordmanager-biometric-failed = The biometric check did not pass: unlock with the master password.
azpasswordmanager-saving-before-closing = Saving before closing…
