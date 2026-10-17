# Die Wörter von AzInstallManager auf Deutsch (src/l10n.rs). Jeder Schlüssel des Quelltexts steht hier und in en.ftl.

## Das Fenster
azinstallmgr-title = Azlin-Apps
azinstallmgr-wizard-title = Azlin-Setup
azinstallmgr-about-title = Über AzInstallManager
azinstallmgr-about-summary = Installiert die Azlin-Apps und hält sie aktuell: stabil oder Beta, Versionen, die jeden Computer in Stufen erreichen, eine frühere Version für „Zurücksetzen“. Teil der Azlin-Apps, gebaut mit azul.
azinstallmgr-version = Version
azinstallmgr-yes = Ja
azinstallmgr-no = Nein
azinstallmgr-exit-title = Setup beenden?
azinstallmgr-exit-text = Das Setup ist nicht abgeschlossen. Wenn Sie jetzt beenden, werden die Azlin-Apps nicht installiert.

    Sie können das Setup später erneut ausführen.
azinstallmgr-shortcuts = AzInstallManager
azinstallmgr-shortcut-about = Über AzInstallManager
azinstallmgr-shortcut-check = Nach Updates suchen
azinstallmgr-shortcut-escape = Das offene Fenster schließen oder fragen, ob das Setup beendet werden soll

## Die Schritte des Assistenten
azinstallmgr-step-welcome = Willkommen
azinstallmgr-step-package-manager = Paketverwaltung
azinstallmgr-step-destination = Zielordner
azinstallmgr-step-components = Apps
azinstallmgr-step-channel = Updates
azinstallmgr-step-ready = Bereit zur Installation
azinstallmgr-step-installing = Installation
azinstallmgr-step-finish = Fertig
azinstallmgr-subtitle-package-manager = Die Azlin-Apps gibt es auch über Ihre Paketverwaltung.
azinstallmgr-subtitle-destination = Wohin sollen die Azlin-Apps installiert werden?
azinstallmgr-subtitle-components = Welche Apps sollen installiert werden?
azinstallmgr-subtitle-channel = Wie sollen die Apps aktuell bleiben?
azinstallmgr-subtitle-ready = Das Setup ist bereit, die Azlin-Apps zu installieren.
azinstallmgr-subtitle-installing = Bitte warten Sie, während das Setup die Apps lädt und installiert.
azinstallmgr-button-back = < Zurück
azinstallmgr-button-next = Weiter >
azinstallmgr-button-install = Installieren
azinstallmgr-button-finish = Fertigstellen
azinstallmgr-button-cancel = Abbrechen

## Die Seiten des Assistenten
azinstallmgr-welcome-title = Willkommen bei den Azlin-Apps
azinstallmgr-welcome-text = Hiermit installieren Sie die Azlin-Apps: die, die Sie auswählen, in einen eigenen Ordner, ohne Administratorrechte.

    Jede App wird von ihrer Update-Quelle geladen und vor der Installation anhand ihrer Signatur geprüft.

    Klicken Sie auf „Weiter“, um fortzufahren, oder auf „Abbrechen“, um das Setup zu beenden.
azinstallmgr-pm-found = { $manager } ist auf diesem Computer installiert. Es kann die Azlin-Apps stattdessen installieren und hält sie dann zusammen mit allem anderen aktuell.
azinstallmgr-pm-how = Um die Apps, die Sie auf den nächsten Seiten auswählen, über { $manager } zu installieren, führen Sie dies in einem Terminal aus:
azinstallmgr-copy = Kopieren
azinstallmgr-copied = Der Befehl ist in der Zwischenablage.
azinstallmgr-pm-choose-manager = Mit { $manager } installieren (empfohlen)
azinstallmgr-pm-choose-azlin = Auf Azlins eigene Art installieren: in Ihren Benutzerordner, aktualisiert von einer Update-Quelle
azinstallmgr-pm-runs = AzInstallManager führt dies über { $manager } für die Apps aus, die Sie als Nächstes auswählen, und zeigt, was es meldet. { $manager } fragt eventuell nach Ihrem Passwort.
azinstallmgr-ready-manager-intro = { $manager } installiert diese Apps und hält sie zusammen mit allem anderen aktuell, das es installiert.
azinstallmgr-ready-command = Befehl
azinstallmgr-ready-no-apps = Wählen Sie mindestens eine App aus.
azinstallmgr-installing-through = Installation über { $manager }...
azinstallmgr-destination-intro = Das Setup installiert die Apps in den folgenden Ordner. Um einen anderen Ordner zu verwenden, geben Sie ihn ein oder klicken Sie auf „Durchsuchen“. In den Ordner müssen Sie schreiben dürfen: Das Setup fragt nicht nach Administratorrechten.
azinstallmgr-components-intro = Wählen Sie die Apps aus, die installiert werden sollen; entfernen Sie den Haken bei den anderen. Die Laufzeitumgebung teilen sich alle Apps.
azinstallmgr-component-runtime = Azlin-Laufzeitumgebung
azinstallmgr-group-office = Büro
azinstallmgr-group-communication = E-Mail, Kalender und Kontakte
azinstallmgr-group-media = Bilder, Musik und Video
azinstallmgr-group-tools = Werkzeuge
azinstallmgr-channel-stable = Stabil
azinstallmgr-channel-stable-help = Geprüfte Versionen. Eine neue Version erreicht diesen Computer einige Tage nach ihrer Veröffentlichung, wenn sie sich auf anderen bewährt hat.
azinstallmgr-channel-beta = Beta
azinstallmgr-channel-beta-help = Jede Version, sobald sie veröffentlicht ist, vor allen anderen. Zum Testen.
azinstallmgr-autoupdate = Die Apps automatisch aktuell halten
azinstallmgr-autoupdate-help = Updates werden im Hintergrund geladen und geprüft; eine App fragt, bevor sie eines installiert.
azinstallmgr-ready-intro = Das Setup ist bereit, die Azlin-Apps zu installieren. Prüfen Sie die Auswahl unten; gehen Sie zurück, um etwas zu ändern.
azinstallmgr-ready-folder = Zielordner
azinstallmgr-ready-apps = Apps
azinstallmgr-ready-channel = Kanal
azinstallmgr-installing-status = Die Azlin-Apps werden installiert …
azinstallmgr-installing-app = { $app } wird installiert …
azinstallmgr-installing-wait = Die Apps werden installiert …
azinstallmgr-installed-status = { $count ->
    [one] Eine App installiert
   *[other] { $count } Apps installiert
}{ $failed ->
    [0] .
   *[other] , { $failed } konnten nicht installiert werden. Das Protokoll sagt, warum.
}
azinstallmgr-log-installed = { $app } { $version } installiert
azinstallmgr-log-failed = { $app } wurde nicht installiert: { $reason }
azinstallmgr-finish-title = Die Azlin-Apps sind installiert
azinstallmgr-finish-text = Das Setup hat die Azlin-Apps auf Ihrem Computer installiert.

    Sie finden sie bei Ihren anderen Programmen. AzInstallManager hält sie aktuell.
azinstallmgr-finish-show-apps = Die installierten Apps anzeigen

## Die Apps
azinstallmgr-app-runtime = Die Bibliothek, auf der jede Azlin-App läuft.
azinstallmgr-app-azwriter = Dokumente auf A4-Seiten, als Markdown-Dateien gespeichert.
azinstallmgr-app-azsheets = Tabellen mit Formeln und Diagrammen.
azinstallmgr-app-azshow = Präsentationen und die Referentenansicht.
azinstallmgr-app-aznotes = Notizen in Notizbüchern, mit Schlagwörtern.
azinstallmgr-app-azpdf = PDF-Dateien lesen und durchsuchen.
azinstallmgr-app-azreader = E-Books lesen.
azinstallmgr-app-azreview = Code mit dem Stift begutachten.
azinstallmgr-app-azmail = E-Mails Ihrer Konten, als Dateien gespeichert.
azinstallmgr-app-azcalendar = Ihre Kalender und Termine.
azinstallmgr-app-azcontacts = Ihr Adressbuch.
azinstallmgr-app-aztasks = Aufgaben und Erinnerungen.
azinstallmgr-app-azmeet = Videokonferenzen.
azinstallmgr-app-aznews = Nachrichten aus den Feeds, denen Sie folgen.
azinstallmgr-app-azphoto = Fotos bearbeiten.
azinstallmgr-app-azpaint = Malen und zeichnen.
azinstallmgr-app-azmusic = Ihre Musiksammlung.
azinstallmgr-app-azplayer = Videos ansehen.
azinstallmgr-app-azvideocut = Videos schneiden.
azinstallmgr-app-azdrive = Ihre Dateien, hier und in der Cloud.
azinstallmgr-app-azkeys = Ihre Passwörter, verschlüsselt.
azinstallmgr-app-azmaps = Karten und Routen.
azinstallmgr-app-azclock = Wecker, Timer und Weltuhr.
azinstallmgr-app-azcalculator = Rechnen, Datumsrechnung und Einheiten.
azinstallmgr-app-azcode = Code bearbeiten.
azinstallmgr-app-azterm = Ein Terminal.
azinstallmgr-app-azmonitor = Was Ihr Computer gerade tut.
azinstallmgr-app-azdashboard = Bestellungen und Zahlen in Tabellen und Diagrammen.
azinstallmgr-app-azerp = Anlagen und Akten eines Unternehmens.

## Die installierten Apps
azinstallmgr-check = Nach Updates suchen
azinstallmgr-update-all = { $count ->
    [0] Alle aktualisieren
   *[other] Alle aktualisieren ({ $count })
}
azinstallmgr-busy = Warten Sie, bis die laufende Arbeit erledigt ist.
azinstallmgr-nothing-to-update = Es gibt nichts zu aktualisieren.
azinstallmgr-cohort = Dieser Computer ist in Verteilungsgruppe { $cohort } von 100.
azinstallmgr-no-apps = Es ist noch keine Azlin-App installiert. Starten Sie das Setup mit --screen wizard, um welche zu installieren.
azinstallmgr-column-app = App
azinstallmgr-column-installed = Installiert
azinstallmgr-column-available = Verfügbar
azinstallmgr-column-channel = Kanal
azinstallmgr-column-size = Größe
azinstallmgr-column-updated = Zuletzt aktualisiert
azinstallmgr-column-actions = Aktionen
azinstallmgr-status-checking = Wird geprüft …
azinstallmgr-status-working = In Arbeit …
azinstallmgr-status-up-to-date = Aktuell
azinstallmgr-status-available = verfügbar
azinstallmgr-status-staged = bereit zur Installation
azinstallmgr-status-skipped = übersprungen
azinstallmgr-status-failed = Prüfung fehlgeschlagen
azinstallmgr-update = Aktualisieren
azinstallmgr-revert-to = Zurück zu { $version }
azinstallmgr-uninstall = Deinstallieren
azinstallmgr-uninstall-title = { $app } deinstallieren?
azinstallmgr-uninstall-text = Die App und ihre frühere Version werden von diesem Computer entfernt. Ihre Dokumente und Einstellungen bleiben, wo sie sind.
azinstallmgr-owner-elsewhere = Nicht von AzInstallManager installiert
azinstallmgr-owner-readonly = Für alle Benutzer dieses Computers installiert
azinstallmgr-update-through = Über { $manager } aktualisieren
azinstallmgr-copied-command = Kopiert: { $command }
azinstallmgr-updated = { $app } ist jetzt { $version }.
azinstallmgr-update-failed = { $app } wurde nicht aktualisiert: { $reason }
azinstallmgr-reverted = { $app } ist zurück auf { $version }. Version { $skipped } wird ab jetzt übersprungen; eine neuere wird wieder angeboten.
azinstallmgr-uninstalled = { $app } wurde deinstalliert.
azinstallmgr-all-up-to-date = Alle Apps sind aktuell.
azinstallmgr-updates-found = { $count ->
    [one] Ein Update ist verfügbar.
   *[other] { $count } Updates sind verfügbar.
}
azinstallmgr-check-failed = Eine Prüfung ist fehlgeschlagen: { $reason }

## Die Update-Einstellungen
azinstallmgr-settings = Update-Einstellungen
azinstallmgr-back-to-apps = Zurück zu den Apps
azinstallmgr-settings-every-app = Alle Apps
azinstallmgr-settings-each-app = Jede App
azinstallmgr-settings-channel = Kanal
azinstallmgr-settings-this-computer = Dieser Computer
azinstallmgr-settings-folder = Installationsordner
azinstallmgr-settings-source = Update-Quelle
azinstallmgr-source-placeholder = https://.../{"{app}"}/{"{channel}"}.json
azinstallmgr-source-help = Die Apps und ihre Updates kommen von dieser Adresse: {"{app}"} steht für die App (azwriter), {"{channel}"} für ihren Kanal (stable oder beta). Drücken Sie die Eingabetaste, um eine neue zu übernehmen. Apps, die ein Paketmanager installiert hat, aktualisiert er selbst; diese Adresse gilt für die Apps, die AzInstallManager selbst installiert.
azinstallmgr-source-none = Es ist keine Update-Quelle eingerichtet, daher kann noch nichts installiert oder aktualisiert werden. Tragen Sie hier die Adresse der Update-Manifeste der Apps ein: Wer Ihnen die Azlin-Apps gibt, sagt Ihnen, wo sie veröffentlicht sind.
azinstallmgr-source-not-web = Eine Update-Quelle ist eine Webadresse, die mit https:// (oder http://) beginnt.
azinstallmgr-source-unset = keine
azinstallmgr-source-unset-reason = Richten Sie zuerst in den Einstellungen eine Update-Quelle ein.
azinstallmgr-ready-no-source = Richten Sie zuerst auf der Seite „Updates“ eine Update-Quelle ein: Die Apps werden von dort geladen.
azinstallmgr-settings-signatures = Signaturprüfung
azinstallmgr-settings-signed = Jede Version muss mit dem eingerichteten Hauptschlüssel signiert sein.
azinstallmgr-settings-digest-only = Es ist kein Hauptschlüssel eingerichtet: Nur die Prüfsumme eines Downloads wird geprüft.
azinstallmgr-settings-group = Verteilungsgruppe
azinstallmgr-settings-window = Updates im Hintergrund
azinstallmgr-settings-any-time = Jederzeit
azinstallmgr-settings-config = Diese Einstellungen stehen in { $path }.
azinstallmgr-settings-saved = Gespeichert. Die Apps folgen ab ihrer nächsten Prüfung.

## Die eigenen Wörter der Assistentenseiten
azinstallmgr-destination-field = Zielordner
azinstallmgr-browse = Durchsuchen …
azinstallmgr-destination-dialog = Den Ordner für die Azlin-Apps wählen
azinstallmgr-space-required = Benötigter Speicherplatz:
azinstallmgr-space-available = Verfügbarer Speicherplatz:
azinstallmgr-destination-empty = Wählen Sie einen Ordner für die Installation.
azinstallmgr-destination-short = Auf dem Laufwerk ist nicht genug Platz frei.
azinstallmgr-show-details = Details anzeigen
azinstallmgr-hide-details = Details ausblenden
