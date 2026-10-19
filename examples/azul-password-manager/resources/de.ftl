# AzPasswordManager's words in German (src/l10n.rs): the terms of password managers and Windows, "du".

## The window

azpasswordmanager-about-summary = Deine Passwörter, Karten, Notizen, Identitäten, SSH-Schlüssel und Einmalcodes in einer verschlüsselten Tresordatei (Argon2id und XChaCha20-Poly1305), entsperrt mit einem Master-Passwort oder, auf diesem Gerät, biometrisch. Teil der Azlin-Apps, gebaut mit azul.
azpasswordmanager-category-security = Sicherheit
azpasswordmanager-category-vault = Tresor
azpasswordmanager-items = Einträge
azpasswordmanager-close-question = Den Eintrag speichern?
azpasswordmanager-close-text = Deine Änderungen am Eintrag gehen verloren, wenn du sie nicht speicherst.
azpasswordmanager-close-discard = Nicht speichern
azpasswordmanager-save = Speichern
azpasswordmanager-close = Schließen

## Unlock and a new vault

azpasswordmanager-unlock-looking = Deine Tresore werden gesucht
azpasswordmanager-unlock-none = Noch kein Tresor
azpasswordmanager-unlock-none-detail = Ein Tresor bewahrt deine Passwörter in einer verschlüsselten Datei in deinem Datenordner auf.
azpasswordmanager-create-vault-action = Tresor anlegen
azpasswordmanager-unlock-title = „{ $name }“ entsperren
azpasswordmanager-vault = Tresor
azpasswordmanager-password = Passwort
azpasswordmanager-master-password = Master-Passwort
azpasswordmanager-wait-seconds = { $seconds } s warten
azpasswordmanager-unlock = Entsperren
azpasswordmanager-wait-for-answer = Warte auf die Antwort
azpasswordmanager-unlock-keyring = Mit dem Schlüsselbund entsperren
azpasswordmanager-use-biometrics = Biometrie verwenden
azpasswordmanager-sample-hint = Das ist der Beispieltresor: Sein Master-Passwort ist „{ $password }“.
azpasswordmanager-new-vault = Neuer Tresor
azpasswordmanager-type-master-password = Gib das Master-Passwort ein.
azpasswordmanager-unlocking = Wird entsperrt…
azpasswordmanager-wrong-password = Falsches Passwort.
azpasswordmanager-wrong-password-wait = Falsches Passwort. Warte { $seconds } s bis zum nächsten Versuch.
azpasswordmanager-wrong-password-left =
    { $count ->
        [one] Falsches Passwort. Noch 1 Versuch vor einer Wartezeit von { $seconds } s.
       *[other] Falsches Passwort. Noch { $count } Versuche vor einer Wartezeit von { $seconds } s.
    }
azpasswordmanager-create-title = Ein neuer Tresor
azpasswordmanager-create-note = Eine verschlüsselte Datei in deinem Datenordner. Niemand - nicht einmal AzPasswordManager - kann sie ohne das Master-Passwort öffnen: Wähle eines, das du dir merkst, und bewahre es aufgeschrieben an einem sicheren Ort auf.
azpasswordmanager-name = Name
azpasswordmanager-create-name-placeholder = Privat
azpasswordmanager-again = Wiederholen
azpasswordmanager-same-password-again = Noch einmal dasselbe Passwort
azpasswordmanager-create-vault = Tresor anlegen
azpasswordmanager-create-crypto-note = Aus dem Master-Passwort wird der Schlüssel mit Argon2id abgeleitet (64 MiB, 3 Durchläufe); der Tresor wird mit XChaCha20-Poly1305 versiegelt.
azpasswordmanager-making-vault = Der Tresor wird angelegt…
azpasswordmanager-making-sample = Der Beispieltresor wird angelegt…
azpasswordmanager-create-needs-name = Gib dem Tresor einen Namen.
azpasswordmanager-password-too-short = Das Master-Passwort braucht mindestens 8 Zeichen.
azpasswordmanager-passwords-differ = Die beiden Passwörter stimmen nicht überein.

## The vault

azpasswordmanager-nav-all = Alle Einträge ({ $count })
azpasswordmanager-nav-favourites = Favoriten ({ $count })
azpasswordmanager-nav-codes = Einmalcodes ({ $count })
azpasswordmanager-categories = Kategorien
azpasswordmanager-tags = Tags
azpasswordmanager-new-item = Neuer Eintrag
azpasswordmanager-nav-label = Tresor, Kategorien und Tags
azpasswordmanager-scope-all = Alle Einträge
azpasswordmanager-scope-favourites = Favoriten
azpasswordmanager-scope-codes = Einmalcodes
azpasswordmanager-search = Alle Einträge durchsuchen
azpasswordmanager-empty-vault = Ein leerer Tresor
azpasswordmanager-empty-vault-detail = Füge eine Anmeldung hinzu oder importiere den Export eines Browsers oder eines Passwort-Managers.
azpasswordmanager-nothing-here = Hier ist nichts
azpasswordmanager-nothing-in-view = Kein Eintrag in dieser Ansicht.
azpasswordmanager-no-match = Kein Eintrag passt zu „{ $query }“.
azpasswordmanager-toolbar-new = Neu
azpasswordmanager-toolbar-generator = Generator
azpasswordmanager-toolbar-audit = Prüfung
azpasswordmanager-toolbar-import = Importieren
azpasswordmanager-toolbar-lock = Sperren
azpasswordmanager-toolbar-settings = Einstellungen
azpasswordmanager-status-items =
    { $count ->
        [one] 1 Eintrag
       *[other] { $count } Einträge
    }
azpasswordmanager-saving = Wird gespeichert…
azpasswordmanager-status-locks-by-hand = Sperrt nur von Hand
azpasswordmanager-status-locks-in = Sperrt in { $time }
azpasswordmanager-status-locks-after = Sperrt nach { $time } ohne Eingabe
azpasswordmanager-status-clipboard = Zwischenablage wird in { $seconds } s geleert

## Kinds

azpasswordmanager-kinds-login = Anmeldungen
azpasswordmanager-kinds-card = Karten
azpasswordmanager-kinds-note = Sichere Notizen
azpasswordmanager-kinds-identity = Identitäten
azpasswordmanager-kinds-ssh-key = SSH-Schlüssel
azpasswordmanager-kind-login = Anmeldung
azpasswordmanager-kind-card = Karte
azpasswordmanager-kind-note = Sichere Notiz
azpasswordmanager-kind-identity = Identität
azpasswordmanager-kind-ssh-key = SSH-Schlüssel
azpasswordmanager-kind-choice-login = Anmeldung
azpasswordmanager-kind-choice-card = Karte
azpasswordmanager-kind-choice-note = Sichere Notiz
azpasswordmanager-kind-choice-identity = Identität
azpasswordmanager-kind-choice-ssh-key = SSH-Schlüssel
azpasswordmanager-new-login = Neue Anmeldung
azpasswordmanager-new-card = Neue Karte
azpasswordmanager-new-note = Neue sichere Notiz
azpasswordmanager-new-identity = Neue Identität
azpasswordmanager-new-ssh-key = Neuer SSH-Schlüssel
azpasswordmanager-edit-login = Anmeldung bearbeiten
azpasswordmanager-edit-card = Karte bearbeiten
azpasswordmanager-edit-note = Sichere Notiz bearbeiten
azpasswordmanager-edit-identity = Identität bearbeiten
azpasswordmanager-edit-ssh-key = SSH-Schlüssel bearbeiten
azpasswordmanager-count-login =
    { $count ->
        [one] 1 Anmeldung
       *[other] { $count } Anmeldungen
    }
azpasswordmanager-count-card =
    { $count ->
        [one] 1 Karte
       *[other] { $count } Karten
    }
azpasswordmanager-count-note =
    { $count ->
        [one] 1 sichere Notiz
       *[other] { $count } sichere Notizen
    }
azpasswordmanager-count-identity =
    { $count ->
        [one] 1 Identität
       *[other] { $count } Identitäten
    }
azpasswordmanager-count-ssh-key =
    { $count ->
        [one] 1 SSH-Schlüssel
       *[other] { $count } SSH-Schlüssel
    }

## An item

azpasswordmanager-no-item = Kein Eintrag ausgewählt
azpasswordmanager-no-item-detail = Wähle einen Eintrag in der Liste aus oder füge einen neuen hinzu.
azpasswordmanager-copy = Kopieren
azpasswordmanager-hide = Verbergen
azpasswordmanager-show = Anzeigen
azpasswordmanager-field-username = Benutzername
azpasswordmanager-field-password = Passwort
azpasswordmanager-field-one-time = Einmalcode
azpasswordmanager-field-holder = Inhaber
azpasswordmanager-field-brand = Marke
azpasswordmanager-field-number = Nummer
azpasswordmanager-field-expires = Gültig bis
azpasswordmanager-field-security-code = Sicherheitscode
azpasswordmanager-field-website = Website
azpasswordmanager-field-tags = Tags
azpasswordmanager-field-notes = Notizen
azpasswordmanager-field-history = Verlauf
azpasswordmanager-strength-changed = { $strength }, geändert am { $date }
azpasswordmanager-seconds-to-next-code = Sekunden bis zum nächsten Code
azpasswordmanager-history =
    { $count ->
        [one] 1 früheres Passwort (das letzte bis { $date })
       *[other] { $count } frühere Passwörter (das letzte bis { $date })
    }
azpasswordmanager-created-changed = Erstellt am { $created }, geändert am { $changed }
azpasswordmanager-delete-ask = „{ $title }“ löschen?
azpasswordmanager-delete = Löschen
azpasswordmanager-keep-it = Behalten
azpasswordmanager-edit = Bearbeiten
azpasswordmanager-copy-of = { $what } von { $title }
azpasswordmanager-copy-username = Benutzername
azpasswordmanager-copy-password = Passwort
azpasswordmanager-copy-code = Einmalcode
azpasswordmanager-copy-card-number = Kartennummer
azpasswordmanager-copy-card-code = Sicherheitscode

## The edit form

azpasswordmanager-kind-label = Art
azpasswordmanager-title = Titel
azpasswordmanager-username = Benutzername
azpasswordmanager-username-placeholder = Benutzername oder E-Mail
azpasswordmanager-generate = Erzeugen
azpasswordmanager-one-time-code = Einmalcode
azpasswordmanager-totp-placeholder = otpauth://totp/... oder das Base32-Geheimnis
azpasswordmanager-website = Website
azpasswordmanager-holder = Inhaber
azpasswordmanager-holder-placeholder = Name auf der Karte
azpasswordmanager-brand = Marke
azpasswordmanager-brand-placeholder = Visa, Mastercard, ...
azpasswordmanager-number = Nummer
azpasswordmanager-card-number = Kartennummer
azpasswordmanager-expires = Gültig bis
azpasswordmanager-expiry-placeholder = MM/JJJJ
azpasswordmanager-security-code = Sicherheitscode
azpasswordmanager-field = Feld
azpasswordmanager-value = Wert
azpasswordmanager-hidden = Verborgen
azpasswordmanager-add-field = Feld hinzufügen
azpasswordmanager-add-tag = Tag hinzufügen (Eingabe)
azpasswordmanager-notes = Notizen
azpasswordmanager-favourite = Favorit
azpasswordmanager-discard-ask = Deine Änderungen verwerfen?
azpasswordmanager-discard = Verwerfen
azpasswordmanager-keep-editing = Weiter bearbeiten
azpasswordmanager-item-needs-title = Gib dem Eintrag einen Titel.
azpasswordmanager-totp-field = Das Feld für den Einmalcode: { $problem }
azpasswordmanager-nothing-edited = Es wird kein Eintrag bearbeitet.
azpasswordmanager-totp-empty = Das Feld für den Einmalcode ist leer.
azpasswordmanager-totp-not-a-secret = Das ist weder eine otpauth://-URL noch ein Base32-Geheimnis mit mindestens 16 Zeichen.
azpasswordmanager-totp-hotp = Zählerbasierte Codes (HOTP) werden nicht unterstützt, nur zeitbasierte (TOTP).
azpasswordmanager-totp-not-totp = Die otpauth-URL ist nicht für einen zeitbasierten Code (totp).
azpasswordmanager-totp-secret-not-base32 = Das Geheimnis in der otpauth-URL ist kein Base32.
azpasswordmanager-totp-hash = Die otpauth-URL nennt ein anderes Hashverfahren als SHA1, SHA256 oder SHA512.
azpasswordmanager-totp-digits = Die otpauth-URL verlangt Codes mit weniger als 6 oder mehr als 8 Ziffern.
azpasswordmanager-totp-period = Der Zeitraum der otpauth-URL liegt nicht zwischen 1 und 3600 Sekunden.
azpasswordmanager-totp-no-secret = Die otpauth-URL hat kein Geheimnis.

## The generator

azpasswordmanager-generator-title = Ein Passwort erzeugen
azpasswordmanager-mode-password = Passwort
azpasswordmanager-mode-passphrase = Passphrase
azpasswordmanager-mode-pin = PIN
azpasswordmanager-regenerate = Neu erzeugen
azpasswordmanager-length = Länge
azpasswordmanager-digits = Ziffern
azpasswordmanager-words = Wörter
azpasswordmanager-avoid-similar = Ähnliche vermeiden (Il1O0)
azpasswordmanager-capitals = Großbuchstaben
azpasswordmanager-digit-at-end = Eine Ziffer am Ende
azpasswordmanager-strength = Stärke
azpasswordmanager-strength-very-weak = sehr schwach
azpasswordmanager-strength-weak = schwach
azpasswordmanager-strength-medium = mittel
azpasswordmanager-strength-strong = stark
azpasswordmanager-strength-very-strong = sehr stark
azpasswordmanager-strength-bits = { $strength } (~{ $bits } Bit)
azpasswordmanager-generator-note = Die Schaltfläche „Erzeugen“ im Bearbeitungsformular verwendet diese Einstellungen. Gezogen aus der Zufallsquelle des Systems; nichts wird aufbewahrt.
azpasswordmanager-generated-password = erzeugtes Passwort
azpasswordmanager-copied-generated = Das erzeugte Passwort wurde kopiert

## Import

azpasswordmanager-import-title = Passwörter importieren
azpasswordmanager-import-help = Ein CSV-Export von Chrome, Edge, Firefox, Safari, Bitwarden, 1Password, KeePassXC oder LastPass oder der JSON-Export von Bitwarden (unverschlüsselt). Lösche die Exportdatei nach dem Import: Sie enthält deine Passwörter im Klartext.
azpasswordmanager-file = Datei
azpasswordmanager-export-path = Der Pfad des Exports
azpasswordmanager-choose = Auswählen…
azpasswordmanager-read = Lesen
azpasswordmanager-reading-file = Die Datei wird gelesen…
azpasswordmanager-import-summary =
    { $count ->
        [one] { $format }: 1 Eintrag ({ $kinds })
       *[other] { $format }: { $count } Einträge ({ $kinds })
    }
azpasswordmanager-import-summary-skipped =
    { $count ->
        [one] { $format }: 1 Eintrag ({ $kinds }); { $skipped } übersprungen
       *[other] { $format }: { $count } Einträge ({ $kinds }); { $skipped } übersprungen
    }
azpasswordmanager-import-run =
    { $count ->
        [one] 1 Eintrag importieren
       *[other] { $count } Einträge importieren
    }
azpasswordmanager-type-export-path = Gib den Pfad des Exports ein oder wähle die Datei aus.
azpasswordmanager-imported =
    { $count ->
        [one] 1 Eintrag importiert
       *[other] { $count } Einträge importiert
    }
azpasswordmanager-imported-known =
    { $count ->
        [one] 1 Eintrag importiert ({ $known } schon im Tresor)
       *[other] { $count } Einträge importiert ({ $known } schon im Tresor)
    }
azpasswordmanager-import-bad-name = Das ist kein Dateiname, den AzPasswordManager lesen kann.
azpasswordmanager-import-missing = Die Datei gibt es nicht.
azpasswordmanager-import-unreadable = Die Datei kann nicht gelesen werden: { $why }
azpasswordmanager-import-not-read = Die Datei wurde nicht gelesen.
azpasswordmanager-import-no-column = Keine Spalte dieser Datei wird von AzPasswordManager importiert: Sie braucht eine Kopfzeile mit name oder title, url, username, password oder notes.
azpasswordmanager-import-row-empty = Zeile { $row }: nichts zu importieren (kein Name, keine Website, kein Benutzername, kein Passwort und keine Notiz)
azpasswordmanager-import-not-json = Das ist keine JSON-Datei (Zeile { $line }, Spalte { $column }).
azpasswordmanager-import-not-bitwarden = Diese JSON-Datei ist kein Bitwarden-Export.
azpasswordmanager-import-encrypted = Das ist ein verschlüsselter Bitwarden-Export. Exportiere den Tresor noch einmal als „JSON“ (unverschlüsselt), importiere diese Datei und lösche sie danach.
azpasswordmanager-import-no-items = Diese JSON-Datei hat keine „items“: Sie ist kein Bitwarden-Export.
azpasswordmanager-import-unknown-type = Eintrag { $item }: eine Art, die AzPasswordManager nicht kennt
azpasswordmanager-import-not-utf8 = Die Datei ist kein UTF-8-Text: Speichere sie als UTF-8-CSV oder als JSON.

## The audit

azpasswordmanager-audit-title = Sicherheitsprüfung
azpasswordmanager-audit-all = Alle ({ $count })
azpasswordmanager-audit-weak = Schwach ({ $count })
azpasswordmanager-audit-reused = Mehrfach ({ $count })
azpasswordmanager-audit-old = Alt ({ $count })
azpasswordmanager-audit-no-two-factor = Ohne 2FA ({ $count })
azpasswordmanager-audit-summary = { $weak } schwach, { $reused } mehrfach verwendet, { $old } älter als zwei Jahre, { $none } ohne Einmalcode. Nichts wird online geprüft.
azpasswordmanager-audit-nothing = Hier gibt es nichts zu beheben.
azpasswordmanager-export-report = Bericht exportieren
azpasswordmanager-problem-weak = schwaches Passwort
azpasswordmanager-problem-reused = in { $count } Einträgen verwendet
azpasswordmanager-problem-old = älter als 2 Jahre
azpasswordmanager-problem-no-two-factor = kein Einmalcode

## Settings

azpasswordmanager-section-security = Sicherheit
azpasswordmanager-lock-after = Sperren nach
azpasswordmanager-clear-clipboard = Zwischenablage leeren
azpasswordmanager-clear-clipboard-after = Zwischenablage leeren nach
azpasswordmanager-device-unlock = Geräteentsperrung
azpasswordmanager-device-unlock-switch = Diesen Tresor auf diesem Gerät ohne das Passwort entsperren
azpasswordmanager-device-unlock-note = Bewahrt den Schlüssel dieses Tresors im Schlüsselbund des Systems auf, hinter Biometrie, wo das System sie hat (Touch ID, Windows Hello, fprintd), sodass dieses Gerät den Tresor ohne das Master-Passwort entsperrt.
azpasswordmanager-device-unlock-locked = Entsperre einen Tresor, damit dieses Gerät ihn biometrisch entsperren kann.
azpasswordmanager-privacy-note = AzPasswordManager stellt keine Netzwerkanfragen: keine Prüfung auf Datenlecks, keine Website-Symbole. Ein kopiertes Geheimnis wird aus der Zwischenablage gelöscht, wenn der Countdown abläuft, auch wenn seitdem etwas anderes kopiert wurde (azul kann die Zwischenablage außerhalb des Einfügens nicht lesen).
azpasswordmanager-key-derivation = Schlüsselableitung
azpasswordmanager-kdf-value =
    { $lanes ->
        [one] Argon2id, { $mib } MiB, { $passes } Durchläufe, 1 Spur; XChaCha20-Poly1305
       *[other] Argon2id, { $mib } MiB, { $passes } Durchläufe, { $lanes } Spuren; XChaCha20-Poly1305
    }
azpasswordmanager-new-password = Neues Passwort
azpasswordmanager-new-master-password = Neues Master-Passwort
azpasswordmanager-change-password = Master-Passwort ändern
azpasswordmanager-change-note = Der Tresorschlüssel bleibt derselbe: Ein Gerät, das den Tresor biometrisch entsperrt, tut das weiterhin. Die Datei vor jedem Speichern wird in keys/backups/ aufbewahrt.
azpasswordmanager-change-locked = Entsperre einen Tresor, um seine Datei zu sehen und sein Master-Passwort zu ändern.
azpasswordmanager-import-note = Import: CSV-Exporte von Chrome, Edge, Firefox, Safari, Bitwarden, 1Password, KeePassXC und LastPass sowie der unverschlüsselte JSON-Export von Bitwarden („Importieren“ in der Symbolleiste). OpenPGP-Schlüssel folgen später.
azpasswordmanager-wait-for-save = Warte, bis das laufende Speichern fertig ist, und versuche es dann noch einmal.
azpasswordmanager-changing-password = Das Master-Passwort wird geändert…
azpasswordmanager-never = Nie
azpasswordmanager-seconds =
    { $count ->
        [one] 1 Sekunde
       *[other] { $count } Sekunden
    }
azpasswordmanager-minutes =
    { $count ->
        [one] 1 Minute
       *[other] { $count } Minuten
    }
azpasswordmanager-hours =
    { $count ->
        [one] 1 Stunde
       *[other] { $count } Stunden
    }

## Shortcuts

azpasswordmanager-shortcuts-vault = Tresor
azpasswordmanager-shortcuts-item = Eintrag
azpasswordmanager-shortcuts-unlock = Entsperren
azpasswordmanager-shortcut-lock = Den Tresor sperren
azpasswordmanager-shortcut-generator = Passwortgenerator
azpasswordmanager-shortcut-previous-next = Vorheriger / nächster Eintrag
azpasswordmanager-shortcut-copy-password = Das Passwort der ausgewählten Anmeldung kopieren
azpasswordmanager-shortcut-copy-username = Den Benutzernamen kopieren
azpasswordmanager-shortcut-copy-code = Den Einmalcode kopieren
azpasswordmanager-shortcut-edit = Den ausgewählten Eintrag bearbeiten
azpasswordmanager-shortcut-save = Den bearbeiteten Eintrag speichern
azpasswordmanager-shortcut-cancel = Bearbeiten abbrechen, einen Bereich schließen
azpasswordmanager-shortcut-delete = Den ausgewählten Eintrag löschen (fragt vorher)
azpasswordmanager-shortcut-unlock = Mit dem Master-Passwort entsperren

## Notices

azpasswordmanager-not-vaults =
    { $count ->
        [one] 1 Datei im Tresorordner ist kein Tresor
       *[other] { $count } Dateien im Tresorordner sind keine Tresore
    }
azpasswordmanager-password-changed = Das Master-Passwort ist geändert.
azpasswordmanager-written-to = Geschrieben nach { $path }
azpasswordmanager-key-stale = Der auf diesem Gerät aufbewahrte Schlüssel öffnet diesen Tresor nicht mehr. Entsperre ihn mit dem Master-Passwort.
azpasswordmanager-not-saved = Nicht gespeichert: { $why }
azpasswordmanager-password-not-changed = Das Passwort wurde nicht geändert: { $why }
azpasswordmanager-create-failed = Der Tresor konnte nicht angelegt werden: { $why }
azpasswordmanager-open-failed = Der Tresor kann nicht geöffnet werden: { $why }
azpasswordmanager-failed = Das hat nicht geklappt: { $why }
azpasswordmanager-saving-then-locking = Wird gespeichert, dann gesperrt…
azpasswordmanager-copied = { $label } kopiert
azpasswordmanager-copied-clears = { $label } kopiert; die Zwischenablage wird in { $seconds } s geleert
azpasswordmanager-clipboard-cleared = Die Zwischenablage ist geleert.
azpasswordmanager-keyring-busy = Der Schlüsselbund des Systems antwortet noch; versuche es gleich noch einmal.
azpasswordmanager-biometric-reason = Den Tresor „{ $name }“ entsperren
azpasswordmanager-use-password = Das Passwort verwenden
azpasswordmanager-waiting-keyring = Warte auf den Schlüsselbund des Systems…
azpasswordmanager-no-device-key = Dieses Gerät bewahrt noch keinen Schlüssel dieses Tresors auf: Entsperre ihn mit dem Master-Passwort und schalte dann in den Einstellungen die Geräteentsperrung ein.
azpasswordmanager-keyring-not-found = der Schlüsselbund hat keinen Schlüssel dieses Tresors
azpasswordmanager-keyring-denied = die Abfrage wurde abgebrochen oder abgelehnt
azpasswordmanager-keyring-unavailable = es ist kein Schlüsselbund des Systems verfügbar
azpasswordmanager-keyring-error = der Schlüsselbund hat einen Fehler gemeldet
azpasswordmanager-device-unlock-keyring-on = Dieses Gerät entsperrt den Tresor jetzt mit dem Schlüsselbund des Systems.
azpasswordmanager-device-unlock-biometric-on = Dieses Gerät entsperrt den Tresor jetzt biometrisch.
azpasswordmanager-device-unlock-not-on = Die Geräteentsperrung ist nicht eingeschaltet: { $why }.
azpasswordmanager-keyring-not-a-key = Der Eintrag im Schlüsselbund ist kein Tresorschlüssel. Entsperre mit dem Master-Passwort.
azpasswordmanager-device-key-gone = Dieses Gerät bewahrt den Schlüssel dieses Tresors nicht mehr auf. Entsperre ihn mit dem Master-Passwort.
azpasswordmanager-device-unlock-failed = Die Geräteentsperrung hat nicht geklappt: { $why }.
azpasswordmanager-device-unlock-off = Dieses Gerät entsperrt den Tresor nicht mehr ohne sein Passwort.
azpasswordmanager-biometric-cancelled = Abgebrochen: Entsperre mit dem Master-Passwort.
azpasswordmanager-biometric-failed = Die biometrische Prüfung ist fehlgeschlagen: Entsperre mit dem Master-Passwort.
azpasswordmanager-saving-before-closing = Wird vor dem Schließen gespeichert…
