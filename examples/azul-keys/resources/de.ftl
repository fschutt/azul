# AzKeys' words in German (src/l10n.rs): the terms of password managers and Windows, "du".

## The window

azkeys-about-summary = Deine Passwörter, Karten, Notizen, Identitäten, SSH-Schlüssel und Einmalcodes in einer verschlüsselten Tresordatei (Argon2id und XChaCha20-Poly1305), entsperrt mit einem Master-Passwort oder, auf diesem Gerät, biometrisch. Teil der Azlin-Apps, gebaut mit azul.
azkeys-category-security = Sicherheit
azkeys-category-vault = Tresor
azkeys-items = Einträge
azkeys-close-question = Den Eintrag speichern?
azkeys-close-text = Deine Änderungen am Eintrag gehen verloren, wenn du sie nicht speicherst.
azkeys-close-discard = Nicht speichern
azkeys-save = Speichern
azkeys-close = Schließen

## Unlock and a new vault

azkeys-unlock-looking = Deine Tresore werden gesucht
azkeys-unlock-none = Noch kein Tresor
azkeys-unlock-none-detail = Ein Tresor bewahrt deine Passwörter in einer verschlüsselten Datei in deinem Datenordner auf.
azkeys-create-vault-action = Tresor anlegen
azkeys-unlock-title = „{ $name }“ entsperren
azkeys-vault = Tresor
azkeys-password = Passwort
azkeys-master-password = Master-Passwort
azkeys-wait-seconds = { $seconds } s warten
azkeys-unlock = Entsperren
azkeys-wait-for-answer = Warte auf die Antwort
azkeys-unlock-keyring = Mit dem Schlüsselbund entsperren
azkeys-use-biometrics = Biometrie verwenden
azkeys-sample-hint = Das ist der Beispieltresor: Sein Master-Passwort ist „{ $password }“.
azkeys-new-vault = Neuer Tresor
azkeys-type-master-password = Gib das Master-Passwort ein.
azkeys-unlocking = Wird entsperrt…
azkeys-wrong-password = Falsches Passwort.
azkeys-wrong-password-wait = Falsches Passwort. Warte { $seconds } s bis zum nächsten Versuch.
azkeys-wrong-password-left =
    { $count ->
        [one] Falsches Passwort. Noch 1 Versuch vor einer Wartezeit von { $seconds } s.
       *[other] Falsches Passwort. Noch { $count } Versuche vor einer Wartezeit von { $seconds } s.
    }
azkeys-create-title = Ein neuer Tresor
azkeys-create-note = Eine verschlüsselte Datei in deinem Datenordner. Niemand - nicht einmal AzKeys - kann sie ohne das Master-Passwort öffnen: Wähle eines, das du dir merkst, und bewahre es aufgeschrieben an einem sicheren Ort auf.
azkeys-name = Name
azkeys-create-name-placeholder = Privat
azkeys-again = Wiederholen
azkeys-same-password-again = Noch einmal dasselbe Passwort
azkeys-create-vault = Tresor anlegen
azkeys-create-crypto-note = Aus dem Master-Passwort wird der Schlüssel mit Argon2id abgeleitet (64 MiB, 3 Durchläufe); der Tresor wird mit XChaCha20-Poly1305 versiegelt.
azkeys-making-vault = Der Tresor wird angelegt…
azkeys-making-sample = Der Beispieltresor wird angelegt…
azkeys-create-needs-name = Gib dem Tresor einen Namen.
azkeys-password-too-short = Das Master-Passwort braucht mindestens 8 Zeichen.
azkeys-passwords-differ = Die beiden Passwörter stimmen nicht überein.

## The vault

azkeys-nav-all = Alle Einträge ({ $count })
azkeys-nav-favourites = Favoriten ({ $count })
azkeys-nav-codes = Einmalcodes ({ $count })
azkeys-categories = Kategorien
azkeys-tags = Tags
azkeys-new-item = Neuer Eintrag
azkeys-nav-label = Tresor, Kategorien und Tags
azkeys-scope-all = Alle Einträge
azkeys-scope-favourites = Favoriten
azkeys-scope-codes = Einmalcodes
azkeys-search = Alle Einträge durchsuchen
azkeys-empty-vault = Ein leerer Tresor
azkeys-empty-vault-detail = Füge eine Anmeldung hinzu oder importiere den Export eines Browsers oder eines Passwort-Managers.
azkeys-nothing-here = Hier ist nichts
azkeys-nothing-in-view = Kein Eintrag in dieser Ansicht.
azkeys-no-match = Kein Eintrag passt zu „{ $query }“.
azkeys-toolbar-new = Neu
azkeys-toolbar-generator = Generator
azkeys-toolbar-audit = Prüfung
azkeys-toolbar-import = Importieren
azkeys-toolbar-lock = Sperren
azkeys-toolbar-settings = Einstellungen
azkeys-status-items =
    { $count ->
        [one] 1 Eintrag
       *[other] { $count } Einträge
    }
azkeys-saving = Wird gespeichert…
azkeys-status-locks-by-hand = Sperrt nur von Hand
azkeys-status-locks-in = Sperrt in { $time }
azkeys-status-locks-after = Sperrt nach { $time } ohne Eingabe
azkeys-status-clipboard = Zwischenablage wird in { $seconds } s geleert

## Kinds

azkeys-kinds-login = Anmeldungen
azkeys-kinds-card = Karten
azkeys-kinds-note = Sichere Notizen
azkeys-kinds-identity = Identitäten
azkeys-kinds-ssh-key = SSH-Schlüssel
azkeys-kind-login = Anmeldung
azkeys-kind-card = Karte
azkeys-kind-note = Sichere Notiz
azkeys-kind-identity = Identität
azkeys-kind-ssh-key = SSH-Schlüssel
azkeys-kind-choice-login = Anmeldung
azkeys-kind-choice-card = Karte
azkeys-kind-choice-note = Sichere Notiz
azkeys-kind-choice-identity = Identität
azkeys-kind-choice-ssh-key = SSH-Schlüssel
azkeys-new-login = Neue Anmeldung
azkeys-new-card = Neue Karte
azkeys-new-note = Neue sichere Notiz
azkeys-new-identity = Neue Identität
azkeys-new-ssh-key = Neuer SSH-Schlüssel
azkeys-edit-login = Anmeldung bearbeiten
azkeys-edit-card = Karte bearbeiten
azkeys-edit-note = Sichere Notiz bearbeiten
azkeys-edit-identity = Identität bearbeiten
azkeys-edit-ssh-key = SSH-Schlüssel bearbeiten
azkeys-count-login =
    { $count ->
        [one] 1 Anmeldung
       *[other] { $count } Anmeldungen
    }
azkeys-count-card =
    { $count ->
        [one] 1 Karte
       *[other] { $count } Karten
    }
azkeys-count-note =
    { $count ->
        [one] 1 sichere Notiz
       *[other] { $count } sichere Notizen
    }
azkeys-count-identity =
    { $count ->
        [one] 1 Identität
       *[other] { $count } Identitäten
    }
azkeys-count-ssh-key =
    { $count ->
        [one] 1 SSH-Schlüssel
       *[other] { $count } SSH-Schlüssel
    }

## An item

azkeys-no-item = Kein Eintrag ausgewählt
azkeys-no-item-detail = Wähle einen Eintrag in der Liste aus oder füge einen neuen hinzu.
azkeys-copy = Kopieren
azkeys-hide = Verbergen
azkeys-show = Anzeigen
azkeys-field-username = Benutzername
azkeys-field-password = Passwort
azkeys-field-one-time = Einmalcode
azkeys-field-holder = Inhaber
azkeys-field-brand = Marke
azkeys-field-number = Nummer
azkeys-field-expires = Gültig bis
azkeys-field-security-code = Sicherheitscode
azkeys-field-website = Website
azkeys-field-tags = Tags
azkeys-field-notes = Notizen
azkeys-field-history = Verlauf
azkeys-strength-changed = { $strength }, geändert am { $date }
azkeys-seconds-to-next-code = Sekunden bis zum nächsten Code
azkeys-history =
    { $count ->
        [one] 1 früheres Passwort (das letzte bis { $date })
       *[other] { $count } frühere Passwörter (das letzte bis { $date })
    }
azkeys-created-changed = Erstellt am { $created }, geändert am { $changed }
azkeys-delete-ask = „{ $title }“ löschen?
azkeys-delete = Löschen
azkeys-keep-it = Behalten
azkeys-edit = Bearbeiten
azkeys-copy-of = { $what } von { $title }
azkeys-copy-username = Benutzername
azkeys-copy-password = Passwort
azkeys-copy-code = Einmalcode
azkeys-copy-card-number = Kartennummer
azkeys-copy-card-code = Sicherheitscode

## The edit form

azkeys-kind-label = Art
azkeys-title = Titel
azkeys-username = Benutzername
azkeys-username-placeholder = Benutzername oder E-Mail
azkeys-generate = Erzeugen
azkeys-one-time-code = Einmalcode
azkeys-totp-placeholder = otpauth://totp/... oder das Base32-Geheimnis
azkeys-website = Website
azkeys-holder = Inhaber
azkeys-holder-placeholder = Name auf der Karte
azkeys-brand = Marke
azkeys-brand-placeholder = Visa, Mastercard, ...
azkeys-number = Nummer
azkeys-card-number = Kartennummer
azkeys-expires = Gültig bis
azkeys-expiry-placeholder = MM/JJJJ
azkeys-security-code = Sicherheitscode
azkeys-field = Feld
azkeys-value = Wert
azkeys-hidden = Verborgen
azkeys-add-field = Feld hinzufügen
azkeys-add-tag = Tag hinzufügen (Eingabe)
azkeys-notes = Notizen
azkeys-favourite = Favorit
azkeys-discard-ask = Deine Änderungen verwerfen?
azkeys-discard = Verwerfen
azkeys-keep-editing = Weiter bearbeiten
azkeys-item-needs-title = Gib dem Eintrag einen Titel.
azkeys-totp-field = Das Feld für den Einmalcode: { $problem }
azkeys-nothing-edited = Es wird kein Eintrag bearbeitet.
azkeys-totp-empty = Das Feld für den Einmalcode ist leer.
azkeys-totp-not-a-secret = Das ist weder eine otpauth://-URL noch ein Base32-Geheimnis mit mindestens 16 Zeichen.
azkeys-totp-hotp = Zählerbasierte Codes (HOTP) werden nicht unterstützt, nur zeitbasierte (TOTP).
azkeys-totp-not-totp = Die otpauth-URL ist nicht für einen zeitbasierten Code (totp).
azkeys-totp-secret-not-base32 = Das Geheimnis in der otpauth-URL ist kein Base32.
azkeys-totp-hash = Die otpauth-URL nennt ein anderes Hashverfahren als SHA1, SHA256 oder SHA512.
azkeys-totp-digits = Die otpauth-URL verlangt Codes mit weniger als 6 oder mehr als 8 Ziffern.
azkeys-totp-period = Der Zeitraum der otpauth-URL liegt nicht zwischen 1 und 3600 Sekunden.
azkeys-totp-no-secret = Die otpauth-URL hat kein Geheimnis.

## The generator

azkeys-generator-title = Ein Passwort erzeugen
azkeys-mode-password = Passwort
azkeys-mode-passphrase = Passphrase
azkeys-mode-pin = PIN
azkeys-regenerate = Neu erzeugen
azkeys-length = Länge
azkeys-digits = Ziffern
azkeys-words = Wörter
azkeys-avoid-similar = Ähnliche vermeiden (Il1O0)
azkeys-capitals = Großbuchstaben
azkeys-digit-at-end = Eine Ziffer am Ende
azkeys-strength = Stärke
azkeys-strength-very-weak = sehr schwach
azkeys-strength-weak = schwach
azkeys-strength-medium = mittel
azkeys-strength-strong = stark
azkeys-strength-very-strong = sehr stark
azkeys-strength-bits = { $strength } (~{ $bits } Bit)
azkeys-generator-note = Die Schaltfläche „Erzeugen“ im Bearbeitungsformular verwendet diese Einstellungen. Gezogen aus der Zufallsquelle des Systems; nichts wird aufbewahrt.
azkeys-generated-password = erzeugtes Passwort
azkeys-copied-generated = Das erzeugte Passwort wurde kopiert

## Import

azkeys-import-title = Passwörter importieren
azkeys-import-help = Ein CSV-Export von Chrome, Edge, Firefox, Safari, Bitwarden, 1Password, KeePassXC oder LastPass oder der JSON-Export von Bitwarden (unverschlüsselt). Lösche die Exportdatei nach dem Import: Sie enthält deine Passwörter im Klartext.
azkeys-file = Datei
azkeys-export-path = Der Pfad des Exports
azkeys-choose = Auswählen…
azkeys-read = Lesen
azkeys-reading-file = Die Datei wird gelesen…
azkeys-import-summary =
    { $count ->
        [one] { $format }: 1 Eintrag ({ $kinds })
       *[other] { $format }: { $count } Einträge ({ $kinds })
    }
azkeys-import-summary-skipped =
    { $count ->
        [one] { $format }: 1 Eintrag ({ $kinds }); { $skipped } übersprungen
       *[other] { $format }: { $count } Einträge ({ $kinds }); { $skipped } übersprungen
    }
azkeys-import-run =
    { $count ->
        [one] 1 Eintrag importieren
       *[other] { $count } Einträge importieren
    }
azkeys-type-export-path = Gib den Pfad des Exports ein oder wähle die Datei aus.
azkeys-imported =
    { $count ->
        [one] 1 Eintrag importiert
       *[other] { $count } Einträge importiert
    }
azkeys-imported-known =
    { $count ->
        [one] 1 Eintrag importiert ({ $known } schon im Tresor)
       *[other] { $count } Einträge importiert ({ $known } schon im Tresor)
    }
azkeys-import-bad-name = Das ist kein Dateiname, den AzKeys lesen kann.
azkeys-import-missing = Die Datei gibt es nicht.
azkeys-import-unreadable = Die Datei kann nicht gelesen werden: { $why }
azkeys-import-not-read = Die Datei wurde nicht gelesen.
azkeys-import-no-column = Keine Spalte dieser Datei wird von AzKeys importiert: Sie braucht eine Kopfzeile mit name oder title, url, username, password oder notes.
azkeys-import-row-empty = Zeile { $row }: nichts zu importieren (kein Name, keine Website, kein Benutzername, kein Passwort und keine Notiz)
azkeys-import-not-json = Das ist keine JSON-Datei (Zeile { $line }, Spalte { $column }).
azkeys-import-not-bitwarden = Diese JSON-Datei ist kein Bitwarden-Export.
azkeys-import-encrypted = Das ist ein verschlüsselter Bitwarden-Export. Exportiere den Tresor noch einmal als „JSON“ (unverschlüsselt), importiere diese Datei und lösche sie danach.
azkeys-import-no-items = Diese JSON-Datei hat keine „items“: Sie ist kein Bitwarden-Export.
azkeys-import-unknown-type = Eintrag { $item }: eine Art, die AzKeys nicht kennt
azkeys-import-not-utf8 = Die Datei ist kein UTF-8-Text: Speichere sie als UTF-8-CSV oder als JSON.

## The audit

azkeys-audit-title = Sicherheitsprüfung
azkeys-audit-all = Alle ({ $count })
azkeys-audit-weak = Schwach ({ $count })
azkeys-audit-reused = Mehrfach ({ $count })
azkeys-audit-old = Alt ({ $count })
azkeys-audit-no-two-factor = Ohne 2FA ({ $count })
azkeys-audit-summary = { $weak } schwach, { $reused } mehrfach verwendet, { $old } älter als zwei Jahre, { $none } ohne Einmalcode. Nichts wird online geprüft.
azkeys-audit-nothing = Hier gibt es nichts zu beheben.
azkeys-export-report = Bericht exportieren
azkeys-problem-weak = schwaches Passwort
azkeys-problem-reused = in { $count } Einträgen verwendet
azkeys-problem-old = älter als 2 Jahre
azkeys-problem-no-two-factor = kein Einmalcode

## Settings

azkeys-section-security = Sicherheit
azkeys-lock-after = Sperren nach
azkeys-clear-clipboard = Zwischenablage leeren
azkeys-clear-clipboard-after = Zwischenablage leeren nach
azkeys-device-unlock = Geräteentsperrung
azkeys-device-unlock-switch = Diesen Tresor auf diesem Gerät ohne das Passwort entsperren
azkeys-device-unlock-note = Bewahrt den Schlüssel dieses Tresors im Schlüsselbund des Systems auf, hinter Biometrie, wo das System sie hat (Touch ID, Windows Hello, fprintd), sodass dieses Gerät den Tresor ohne das Master-Passwort entsperrt.
azkeys-device-unlock-locked = Entsperre einen Tresor, damit dieses Gerät ihn biometrisch entsperren kann.
azkeys-privacy-note = AzKeys stellt keine Netzwerkanfragen: keine Prüfung auf Datenlecks, keine Website-Symbole. Ein kopiertes Geheimnis wird aus der Zwischenablage gelöscht, wenn der Countdown abläuft, auch wenn seitdem etwas anderes kopiert wurde (azul kann die Zwischenablage außerhalb des Einfügens nicht lesen).
azkeys-key-derivation = Schlüsselableitung
azkeys-kdf-value =
    { $lanes ->
        [one] Argon2id, { $mib } MiB, { $passes } Durchläufe, 1 Spur; XChaCha20-Poly1305
       *[other] Argon2id, { $mib } MiB, { $passes } Durchläufe, { $lanes } Spuren; XChaCha20-Poly1305
    }
azkeys-new-password = Neues Passwort
azkeys-new-master-password = Neues Master-Passwort
azkeys-change-password = Master-Passwort ändern
azkeys-change-note = Der Tresorschlüssel bleibt derselbe: Ein Gerät, das den Tresor biometrisch entsperrt, tut das weiterhin. Die Datei vor jedem Speichern wird in keys/backups/ aufbewahrt.
azkeys-change-locked = Entsperre einen Tresor, um seine Datei zu sehen und sein Master-Passwort zu ändern.
azkeys-import-note = Import: CSV-Exporte von Chrome, Edge, Firefox, Safari, Bitwarden, 1Password, KeePassXC und LastPass sowie der unverschlüsselte JSON-Export von Bitwarden („Importieren“ in der Symbolleiste). OpenPGP-Schlüssel folgen später.
azkeys-wait-for-save = Warte, bis das laufende Speichern fertig ist, und versuche es dann noch einmal.
azkeys-changing-password = Das Master-Passwort wird geändert…
azkeys-never = Nie
azkeys-seconds =
    { $count ->
        [one] 1 Sekunde
       *[other] { $count } Sekunden
    }
azkeys-minutes =
    { $count ->
        [one] 1 Minute
       *[other] { $count } Minuten
    }
azkeys-hours =
    { $count ->
        [one] 1 Stunde
       *[other] { $count } Stunden
    }

## Shortcuts

azkeys-shortcuts-vault = Tresor
azkeys-shortcuts-item = Eintrag
azkeys-shortcuts-unlock = Entsperren
azkeys-shortcut-lock = Den Tresor sperren
azkeys-shortcut-generator = Passwortgenerator
azkeys-shortcut-previous-next = Vorheriger / nächster Eintrag
azkeys-shortcut-copy-password = Das Passwort der ausgewählten Anmeldung kopieren
azkeys-shortcut-copy-username = Den Benutzernamen kopieren
azkeys-shortcut-copy-code = Den Einmalcode kopieren
azkeys-shortcut-edit = Den ausgewählten Eintrag bearbeiten
azkeys-shortcut-save = Den bearbeiteten Eintrag speichern
azkeys-shortcut-cancel = Bearbeiten abbrechen, einen Bereich schließen
azkeys-shortcut-delete = Den ausgewählten Eintrag löschen (fragt vorher)
azkeys-shortcut-unlock = Mit dem Master-Passwort entsperren

## Notices

azkeys-not-vaults =
    { $count ->
        [one] 1 Datei im Tresorordner ist kein Tresor
       *[other] { $count } Dateien im Tresorordner sind keine Tresore
    }
azkeys-password-changed = Das Master-Passwort ist geändert.
azkeys-written-to = Geschrieben nach { $path }
azkeys-key-stale = Der auf diesem Gerät aufbewahrte Schlüssel öffnet diesen Tresor nicht mehr. Entsperre ihn mit dem Master-Passwort.
azkeys-not-saved = Nicht gespeichert: { $why }
azkeys-password-not-changed = Das Passwort wurde nicht geändert: { $why }
azkeys-create-failed = Der Tresor konnte nicht angelegt werden: { $why }
azkeys-open-failed = Der Tresor kann nicht geöffnet werden: { $why }
azkeys-failed = Das hat nicht geklappt: { $why }
azkeys-saving-then-locking = Wird gespeichert, dann gesperrt…
azkeys-copied = { $label } kopiert
azkeys-copied-clears = { $label } kopiert; die Zwischenablage wird in { $seconds } s geleert
azkeys-clipboard-cleared = Die Zwischenablage ist geleert.
azkeys-keyring-busy = Der Schlüsselbund des Systems antwortet noch; versuche es gleich noch einmal.
azkeys-biometric-reason = Den Tresor „{ $name }“ entsperren
azkeys-use-password = Das Passwort verwenden
azkeys-waiting-keyring = Warte auf den Schlüsselbund des Systems…
azkeys-no-device-key = Dieses Gerät bewahrt noch keinen Schlüssel dieses Tresors auf: Entsperre ihn mit dem Master-Passwort und schalte dann in den Einstellungen die Geräteentsperrung ein.
azkeys-keyring-not-found = der Schlüsselbund hat keinen Schlüssel dieses Tresors
azkeys-keyring-denied = die Abfrage wurde abgebrochen oder abgelehnt
azkeys-keyring-unavailable = es ist kein Schlüsselbund des Systems verfügbar
azkeys-keyring-error = der Schlüsselbund hat einen Fehler gemeldet
azkeys-device-unlock-keyring-on = Dieses Gerät entsperrt den Tresor jetzt mit dem Schlüsselbund des Systems.
azkeys-device-unlock-biometric-on = Dieses Gerät entsperrt den Tresor jetzt biometrisch.
azkeys-device-unlock-not-on = Die Geräteentsperrung ist nicht eingeschaltet: { $why }.
azkeys-keyring-not-a-key = Der Eintrag im Schlüsselbund ist kein Tresorschlüssel. Entsperre mit dem Master-Passwort.
azkeys-device-key-gone = Dieses Gerät bewahrt den Schlüssel dieses Tresors nicht mehr auf. Entsperre ihn mit dem Master-Passwort.
azkeys-device-unlock-failed = Die Geräteentsperrung hat nicht geklappt: { $why }.
azkeys-device-unlock-off = Dieses Gerät entsperrt den Tresor nicht mehr ohne sein Passwort.
azkeys-biometric-cancelled = Abgebrochen: Entsperre mit dem Master-Passwort.
azkeys-biometric-failed = Die biometrische Prüfung ist fehlgeschlagen: Entsperre mit dem Master-Passwort.
azkeys-saving-before-closing = Wird vor dem Schließen gespeichert…
