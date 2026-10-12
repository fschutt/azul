# AzKeys' words in English (src/l10n.rs). Every key the source names is here and in de.ftl.

## The window

azkeys-about-summary = Your passwords, cards, notes, identities, SSH keys and one-time codes in an encrypted vault file (Argon2id and XChaCha20-Poly1305), unlocked with a master password or, on this device, with biometrics. Part of the Azlin apps, built with azul.
azkeys-category-security = Security
azkeys-category-vault = Vault
azkeys-items = Items
azkeys-close-question = Save the item?
azkeys-close-text = Your changes to the item are lost if you don't save them.
azkeys-close-discard = Don't Save
azkeys-save = Save
azkeys-close = Close

## Unlock and a new vault

azkeys-unlock-looking = Looking for your vaults
azkeys-unlock-none = No vault yet
azkeys-unlock-none-detail = A vault keeps your passwords in one encrypted file in your data folder.
azkeys-create-vault-action = Create a vault
azkeys-unlock-title = Unlock “{ $name }”
azkeys-vault = Vault
azkeys-password = Password
azkeys-master-password = Master password
azkeys-wait-seconds = Wait { $seconds } s
azkeys-unlock = Unlock
azkeys-wait-for-answer = Wait for the answer
azkeys-unlock-keyring = Unlock from the keyring
azkeys-use-biometrics = Use biometrics
azkeys-sample-hint = This is the sample vault: its master password is “{ $password }”.
azkeys-new-vault = New vault
azkeys-type-master-password = Type the master password.
azkeys-unlocking = Unlocking…
azkeys-wrong-password = Wrong password.
azkeys-wrong-password-wait = Wrong password. Wait { $seconds } s before the next try.
azkeys-wrong-password-left =
    { $count ->
        [one] Wrong password. 1 attempt left before a { $seconds } s wait.
       *[other] Wrong password. { $count } attempts left before a { $seconds } s wait.
    }
azkeys-create-title = A new vault
azkeys-create-note = One encrypted file in your data folder. Nobody - not even AzKeys - can open it without the master password: choose one you will remember, and keep it written down somewhere safe.
azkeys-name = Name
azkeys-create-name-placeholder = Personal
azkeys-again = Again
azkeys-same-password-again = The same password again
azkeys-create-vault = Create vault
azkeys-create-crypto-note = The master password derives the key with Argon2id (64 MiB, 3 passes); the vault is sealed with XChaCha20-Poly1305.
azkeys-making-vault = Making the vault…
azkeys-making-sample = Making the sample vault…
azkeys-create-needs-name = Give the vault a name.
azkeys-password-too-short = The master password needs 8 characters or more.
azkeys-passwords-differ = The two passwords differ.

## The vault

azkeys-nav-all = All items ({ $count })
azkeys-nav-favourites = Favourites ({ $count })
azkeys-nav-codes = One-time codes ({ $count })
azkeys-categories = Categories
azkeys-tags = Tags
azkeys-new-item = New item
azkeys-nav-label = Vault, categories and tags
azkeys-scope-all = All items
azkeys-scope-favourites = Favourites
azkeys-scope-codes = One-time codes
azkeys-search = Search all items
azkeys-empty-vault = An empty vault
azkeys-empty-vault-detail = Add a login, or import a browser's or a password manager's export.
azkeys-nothing-here = Nothing here
azkeys-nothing-in-view = No item is in this view.
azkeys-no-match = No item matches “{ $query }”.
azkeys-toolbar-new = New
azkeys-toolbar-generator = Generator
azkeys-toolbar-audit = Audit
azkeys-toolbar-import = Import
azkeys-toolbar-lock = Lock
azkeys-toolbar-settings = Settings
azkeys-status-items =
    { $count ->
        [one] 1 item
       *[other] { $count } items
    }
azkeys-saving = Saving…
azkeys-status-locks-by-hand = Locks only by hand
azkeys-status-locks-in = Locks in { $time }
azkeys-status-locks-after = Locks after { $time } idle
azkeys-status-clipboard = Clipboard clears in { $seconds } s

## Kinds

azkeys-kinds-login = Logins
azkeys-kinds-card = Cards
azkeys-kinds-note = Secure notes
azkeys-kinds-identity = Identities
azkeys-kinds-ssh-key = SSH keys
azkeys-kind-login = login
azkeys-kind-card = card
azkeys-kind-note = secure note
azkeys-kind-identity = identity
azkeys-kind-ssh-key = SSH key
azkeys-kind-choice-login = Login
azkeys-kind-choice-card = Card
azkeys-kind-choice-note = Secure note
azkeys-kind-choice-identity = Identity
azkeys-kind-choice-ssh-key = SSH key
azkeys-new-login = New login
azkeys-new-card = New card
azkeys-new-note = New secure note
azkeys-new-identity = New identity
azkeys-new-ssh-key = New SSH key
azkeys-edit-login = Edit login
azkeys-edit-card = Edit card
azkeys-edit-note = Edit secure note
azkeys-edit-identity = Edit identity
azkeys-edit-ssh-key = Edit SSH key
azkeys-count-login =
    { $count ->
        [one] 1 login
       *[other] { $count } logins
    }
azkeys-count-card =
    { $count ->
        [one] 1 card
       *[other] { $count } cards
    }
azkeys-count-note =
    { $count ->
        [one] 1 secure note
       *[other] { $count } secure notes
    }
azkeys-count-identity =
    { $count ->
        [one] 1 identity
       *[other] { $count } identities
    }
azkeys-count-ssh-key =
    { $count ->
        [one] 1 SSH key
       *[other] { $count } SSH keys
    }

## An item

azkeys-no-item = No item selected
azkeys-no-item-detail = Pick an item in the list, or add a new one.
azkeys-copy = Copy
azkeys-hide = Hide
azkeys-show = Show
azkeys-field-username = user name
azkeys-field-password = password
azkeys-field-one-time = one-time
azkeys-field-holder = holder
azkeys-field-brand = brand
azkeys-field-number = number
azkeys-field-expires = expires
azkeys-field-security-code = security code
azkeys-field-website = website
azkeys-field-tags = tags
azkeys-field-notes = notes
azkeys-field-history = history
azkeys-strength-changed = { $strength }, changed { $date }
azkeys-seconds-to-next-code = Seconds until the next code
azkeys-history =
    { $count ->
        [one] 1 earlier password (the last until { $date })
       *[other] { $count } earlier passwords (the last until { $date })
    }
azkeys-created-changed = Created { $created }, changed { $changed }
azkeys-delete-ask = Delete “{ $title }”?
azkeys-delete = Delete
azkeys-keep-it = Keep it
azkeys-edit = Edit
azkeys-copy-of = { $what } of { $title }
azkeys-copy-username = user name
azkeys-copy-password = password
azkeys-copy-code = one-time code
azkeys-copy-card-number = card number
azkeys-copy-card-code = security code

## The edit form

azkeys-kind-label = Kind
azkeys-title = Title
azkeys-username = User name
azkeys-username-placeholder = User name or email
azkeys-generate = Generate
azkeys-one-time-code = One-time code
azkeys-totp-placeholder = otpauth://totp/... or the base32 secret
azkeys-website = Website
azkeys-holder = Holder
azkeys-holder-placeholder = Name on the card
azkeys-brand = Brand
azkeys-brand-placeholder = Visa, Mastercard, ...
azkeys-number = Number
azkeys-card-number = Card number
azkeys-expires = Expires
azkeys-expiry-placeholder = MM/YYYY
azkeys-security-code = Security code
azkeys-field = Field
azkeys-value = Value
azkeys-hidden = Hidden
azkeys-add-field = Add a field
azkeys-add-tag = Add a tag (Enter)
azkeys-notes = Notes
azkeys-favourite = Favourite
azkeys-discard-ask = Discard your changes?
azkeys-discard = Discard
azkeys-keep-editing = Keep editing
azkeys-item-needs-title = Give the item a title.
azkeys-totp-field = The one-time code field: { $problem }
azkeys-nothing-edited = No item is being edited.
azkeys-totp-empty = The one-time code field is empty.
azkeys-totp-not-a-secret = This is neither an otpauth:// URL nor a base32 secret of 16 characters or more.
azkeys-totp-hotp = Counter-based (HOTP) codes are not supported; only time-based (TOTP) codes.
azkeys-totp-not-totp = The otpauth URL is not for a time-based code (totp).
azkeys-totp-secret-not-base32 = The secret in the otpauth URL is not base32.
azkeys-totp-hash = The otpauth URL names a hash other than SHA1, SHA256 or SHA512.
azkeys-totp-digits = The otpauth URL asks for codes of other than 6 to 8 digits.
azkeys-totp-period = The otpauth URL's period is not 1 to 3600 seconds.
azkeys-totp-no-secret = The otpauth URL has no secret.

## The generator

azkeys-generator-title = Generate a password
azkeys-mode-password = Password
azkeys-mode-passphrase = Passphrase
azkeys-mode-pin = PIN
azkeys-regenerate = Again
azkeys-length = Length
azkeys-digits = Digits
azkeys-words = Words
azkeys-avoid-similar = avoid similar (Il1O0)
azkeys-capitals = Capitals
azkeys-digit-at-end = A digit at the end
azkeys-strength = Strength
azkeys-strength-very-weak = very weak
azkeys-strength-weak = weak
azkeys-strength-medium = medium
azkeys-strength-strong = strong
azkeys-strength-very-strong = very strong
azkeys-strength-bits = { $strength } (~{ $bits } bits)
azkeys-generator-note = The edit form's Generate button uses these settings. Drawn from the system's random source; nothing is kept.
azkeys-generated-password = generated password
azkeys-copied-generated = Copied the generated password

## Import

azkeys-import-title = Import passwords
azkeys-import-help = A CSV export of Chrome, Edge, Firefox, Safari, Bitwarden, 1Password, KeePassXC or LastPass, or Bitwarden's JSON export (unencrypted). Delete the export file once it is imported: it holds your passwords in plain text.
azkeys-file = File
azkeys-export-path = The export's path
azkeys-choose = Choose…
azkeys-read = Read
azkeys-reading-file = Reading the file…
azkeys-import-summary =
    { $count ->
        [one] { $format }: 1 item ({ $kinds })
       *[other] { $format }: { $count } items ({ $kinds })
    }
azkeys-import-summary-skipped =
    { $count ->
        [one] { $format }: 1 item ({ $kinds }); { $skipped } skipped
       *[other] { $format }: { $count } items ({ $kinds }); { $skipped } skipped
    }
azkeys-import-run =
    { $count ->
        [one] Import 1 item
       *[other] Import { $count } items
    }
azkeys-type-export-path = Type the export's path, or choose the file.
azkeys-imported =
    { $count ->
        [one] Imported 1 item
       *[other] Imported { $count } items
    }
azkeys-imported-known =
    { $count ->
        [one] Imported 1 item ({ $known } already in the vault)
       *[other] Imported { $count } items ({ $known } already in the vault)
    }
azkeys-import-bad-name = This is not a file name AzKeys can read.
azkeys-import-missing = The file does not exist.
azkeys-import-unreadable = The file cannot be read: { $why }
azkeys-import-not-read = The file was not read.
azkeys-import-no-column = No column of this file is one AzKeys imports: it needs a header row naming name or title, url, username, password or notes.
azkeys-import-row-empty = row { $row }: nothing to import (no name, website, user name, password or note)
azkeys-import-not-json = This is not a JSON file (line { $line }, column { $column }).
azkeys-import-not-bitwarden = This JSON file is not a Bitwarden export.
azkeys-import-encrypted = This is an encrypted Bitwarden export. Export the vault again choosing "JSON" (unencrypted), import that file, then delete it.
azkeys-import-no-items = This JSON file has no "items": it is not a Bitwarden export.
azkeys-import-unknown-type = item { $item }: a type AzKeys does not know
azkeys-import-not-utf8 = The file is not UTF-8 text: save it as a UTF-8 CSV or as JSON.

## The audit

azkeys-audit-title = Security audit
azkeys-audit-all = All ({ $count })
azkeys-audit-weak = Weak ({ $count })
azkeys-audit-reused = Reused ({ $count })
azkeys-audit-old = Old ({ $count })
azkeys-audit-no-two-factor = No 2FA ({ $count })
azkeys-audit-summary = { $weak } weak, { $reused } reused, { $old } older than two years, { $none } without a one-time code. Nothing is checked online.
azkeys-audit-nothing = Nothing to fix here.
azkeys-export-report = Export report
azkeys-problem-weak = weak password
azkeys-problem-reused = reused in { $count } items
azkeys-problem-old = older than 2 years
azkeys-problem-no-two-factor = no one-time code

## Settings

azkeys-section-security = Security
azkeys-lock-after = Lock after
azkeys-clear-clipboard = Clear the clipboard
azkeys-clear-clipboard-after = Clear the clipboard after
azkeys-device-unlock = Device unlock
azkeys-device-unlock-switch = Unlock this vault on this device without the password
azkeys-device-unlock-note = Keeps this vault's key in the system keyring, behind biometrics where the system has them (Touch ID, Windows Hello, fprintd), so this device unlocks the vault without the master password.
azkeys-device-unlock-locked = Unlock a vault to let this device unlock it with biometrics.
azkeys-privacy-note = AzKeys makes no network requests: no breach checks, no website icons. A copied secret is cleared from the clipboard when the countdown ends, even if something else was copied since (azul cannot read the clipboard outside a paste).
azkeys-key-derivation = Key derivation
azkeys-kdf-value =
    { $lanes ->
        [one] Argon2id, { $mib } MiB, { $passes } passes, 1 lane; XChaCha20-Poly1305
       *[other] Argon2id, { $mib } MiB, { $passes } passes, { $lanes } lanes; XChaCha20-Poly1305
    }
azkeys-new-password = New password
azkeys-new-master-password = New master password
azkeys-change-password = Change the master password
azkeys-change-note = The vault key stays the same: a device that unlocks the vault with biometrics keeps doing so. The file before each save is kept in keys/backups/.
azkeys-change-locked = Unlock a vault to see its file and change its master password.
azkeys-import-note = Import: CSV exports of Chrome, Edge, Firefox, Safari, Bitwarden, 1Password, KeePassXC and LastPass, and Bitwarden's unencrypted JSON export (the toolbar's Import). OpenPGP keys are a later step.
azkeys-wait-for-save = Wait for the save on the way, then try again.
azkeys-changing-password = Changing the master password…
azkeys-never = Never
azkeys-seconds =
    { $count ->
        [one] 1 second
       *[other] { $count } seconds
    }
azkeys-minutes =
    { $count ->
        [one] 1 minute
       *[other] { $count } minutes
    }
azkeys-hours =
    { $count ->
        [one] 1 hour
       *[other] { $count } hours
    }

## Shortcuts

azkeys-shortcuts-vault = Vault
azkeys-shortcuts-item = Item
azkeys-shortcuts-unlock = Unlock
azkeys-shortcut-lock = Lock the vault
azkeys-shortcut-generator = Password generator
azkeys-shortcut-previous-next = Previous / next item
azkeys-shortcut-copy-password = Copy the password of the selected login
azkeys-shortcut-copy-username = Copy the user name
azkeys-shortcut-copy-code = Copy the one-time code
azkeys-shortcut-edit = Edit the selected item
azkeys-shortcut-save = Save the item being edited
azkeys-shortcut-cancel = Cancel editing, close a panel
azkeys-shortcut-delete = Delete the selected item (asks first)
azkeys-shortcut-unlock = Unlock with the master password

## Notices

azkeys-not-vaults =
    { $count ->
        [one] 1 file in the vault folder is not a vault
       *[other] { $count } files in the vault folder are not vaults
    }
azkeys-password-changed = The master password is changed.
azkeys-written-to = Written to { $path }
azkeys-key-stale = The key kept on this device no longer opens this vault. Unlock it with the master password.
azkeys-not-saved = Not saved: { $why }
azkeys-password-not-changed = The password is not changed: { $why }
azkeys-create-failed = The vault could not be made: { $why }
azkeys-open-failed = The vault cannot be opened: { $why }
azkeys-failed = That did not work: { $why }
azkeys-saving-then-locking = Saving, then locking…
azkeys-copied = Copied the { $label }
azkeys-copied-clears = Copied the { $label }; the clipboard clears in { $seconds } s
azkeys-clipboard-cleared = The clipboard is cleared.
azkeys-keyring-busy = The system keyring is still answering; try again in a moment.
azkeys-biometric-reason = Unlock the vault “{ $name }”
azkeys-use-password = Use the password
azkeys-waiting-keyring = Waiting for the system keyring…
azkeys-no-device-key = This device keeps no key of this vault yet: unlock it with the master password, then turn on device unlock in the settings.
azkeys-keyring-not-found = the keyring has no key of this vault
azkeys-keyring-denied = the prompt was cancelled or refused
azkeys-keyring-unavailable = no system keyring is available
azkeys-keyring-error = the keyring reported an error
azkeys-device-unlock-keyring-on = This device now unlocks the vault from the system keyring.
azkeys-device-unlock-biometric-on = This device now unlocks the vault with biometrics.
azkeys-device-unlock-not-on = Device unlock is not on: { $why }.
azkeys-keyring-not-a-key = The keyring's entry is not a vault key. Unlock with the master password.
azkeys-device-key-gone = This device no longer keeps the key of this vault. Unlock it with the master password.
azkeys-device-unlock-failed = Device unlock did not work: { $why }.
azkeys-device-unlock-off = This device no longer unlocks the vault without its password.
azkeys-biometric-cancelled = Cancelled: unlock with the master password.
azkeys-biometric-failed = The biometric check did not pass: unlock with the master password.
azkeys-saving-before-closing = Saving before closing…
