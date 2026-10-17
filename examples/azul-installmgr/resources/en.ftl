# AzInstallManager's words in English (src/l10n.rs). Every key the source names is here and in de.ftl.

## The window
azinstallmgr-title = Azlin Apps
azinstallmgr-wizard-title = Azlin Setup
azinstallmgr-about-title = About AzInstallManager
azinstallmgr-about-summary = Installs the Azlin apps and keeps them up to date: stable or beta, releases that reach every computer in stages, one previous version kept for Revert. Part of the Azlin apps, built with azul.
azinstallmgr-version = Version
azinstallmgr-yes = Yes
azinstallmgr-no = No
azinstallmgr-exit-title = Exit Setup?
azinstallmgr-exit-text = Setup is not complete. If you exit now, the Azlin apps will not be installed.

    You can run Setup again at another time.
azinstallmgr-shortcuts = AzInstallManager
azinstallmgr-shortcut-about = About AzInstallManager
azinstallmgr-shortcut-check = Check for updates
azinstallmgr-shortcut-escape = Close the box that is open, or ask to exit Setup

## The wizard's steps
azinstallmgr-step-welcome = Welcome
azinstallmgr-step-package-manager = Package manager
azinstallmgr-step-destination = Destination
azinstallmgr-step-components = Apps
azinstallmgr-step-channel = Updates
azinstallmgr-step-ready = Ready to install
azinstallmgr-step-installing = Installing
azinstallmgr-step-finish = Done
azinstallmgr-subtitle-package-manager = The Azlin apps can also come through your package manager.
azinstallmgr-subtitle-destination = Where should the Azlin apps be installed?
azinstallmgr-subtitle-components = Which apps should be installed?
azinstallmgr-subtitle-channel = How should the apps stay up to date?
azinstallmgr-subtitle-ready = Setup is ready to install the Azlin apps.
azinstallmgr-subtitle-installing = Please wait while Setup fetches and installs the apps.
azinstallmgr-button-back = < Back
azinstallmgr-button-next = Next >
azinstallmgr-button-install = Install
azinstallmgr-button-finish = Finish
azinstallmgr-button-cancel = Cancel

## The wizard's pages
azinstallmgr-welcome-title = Welcome to the Azlin apps
azinstallmgr-welcome-text = This installs the Azlin apps for you: the ones you pick, into a folder of your own, without administrator rights.

    Every app is fetched from its update source and checked against its signature before it is installed.

    Click Next to continue, or Cancel to exit Setup.
azinstallmgr-pm-found = { $manager } is installed on this computer. It can install the Azlin apps instead, and then keeps them up to date with everything else it installs.
azinstallmgr-pm-how = To install the apps you pick on the next pages through { $manager }, run this in a terminal:
azinstallmgr-copy = Copy
azinstallmgr-copied = The command is on the clipboard.
azinstallmgr-pm-choose-manager = Install with { $manager } (recommended)
azinstallmgr-pm-choose-azlin = Install Azlin's own way: into your home folder, updated from an update source
azinstallmgr-pm-runs = AzInstallManager runs this through { $manager } for the apps you pick next and shows what it says. { $manager } may ask for your password.
azinstallmgr-ready-manager-intro = { $manager } installs these apps and keeps them up to date with everything else it installs.
azinstallmgr-ready-command = Command
azinstallmgr-ready-no-apps = Pick at least one app.
azinstallmgr-installing-through = Installing through { $manager }...
azinstallmgr-destination-intro = Setup will install the apps into the following folder. To use another folder, type it or click Browse. The folder must be one you can write to: Setup does not ask for administrator rights.
azinstallmgr-components-intro = Select the apps you want to install; clear the ones you do not. The runtime is shared by every app.
azinstallmgr-component-runtime = Azlin runtime
azinstallmgr-group-office = Office
azinstallmgr-group-communication = Mail, calendar and contacts
azinstallmgr-group-media = Pictures, music and video
azinstallmgr-group-tools = Tools
azinstallmgr-channel-stable = Stable
azinstallmgr-channel-stable-help = Tested releases. A new release reaches this computer a few days after it is published, when it has proven itself on others.
azinstallmgr-channel-beta = Beta
azinstallmgr-channel-beta-help = Every release the moment it is published, before everyone else. For testing.
azinstallmgr-autoupdate = Keep the apps up to date automatically
azinstallmgr-autoupdate-help = Updates are fetched and checked in the background; an app asks before it installs one.
azinstallmgr-ready-intro = Setup is ready to install the Azlin apps. Review the choices below; go back to change one.
azinstallmgr-ready-folder = Destination folder
azinstallmgr-ready-apps = Apps
azinstallmgr-ready-channel = Channel
azinstallmgr-installing-status = Installing the Azlin apps...
azinstallmgr-installing-app = Installing { $app }...
azinstallmgr-installing-wait = Installing the apps...
azinstallmgr-installed-status = { $count ->
    [one] One app installed
   *[other] { $count } apps installed
}{ $failed ->
    [0] .
   *[other] , { $failed } could not be installed. The log says why.
}
azinstallmgr-log-installed = Installed { $app } { $version }
azinstallmgr-log-failed = { $app } was not installed: { $reason }
azinstallmgr-finish-title = The Azlin apps are installed
azinstallmgr-finish-text = Setup has finished installing the Azlin apps on your computer.

    You find them with your other programs. AzInstallManager keeps them up to date.
azinstallmgr-finish-show-apps = Show the installed apps

## The apps
azinstallmgr-app-runtime = The library every Azlin app runs on.
azinstallmgr-app-azwriter = Documents on A4 pages, kept as Markdown files.
azinstallmgr-app-azsheets = Spreadsheets with formulas and charts.
azinstallmgr-app-azshow = Presentations and the presenter view.
azinstallmgr-app-aznotes = Notes in notebooks, with tags.
azinstallmgr-app-azpdf = Read and search PDF files.
azinstallmgr-app-azreader = Read e-books.
azinstallmgr-app-azreview = Review code with a pen.
azinstallmgr-app-azmail = Mail from your accounts, kept as files.
azinstallmgr-app-azcalendar = Your calendars and meetings.
azinstallmgr-app-azcontacts = Your address book.
azinstallmgr-app-aztasks = To-dos and reminders.
azinstallmgr-app-azmeet = Video meetings.
azinstallmgr-app-aznews = News from the feeds you follow.
azinstallmgr-app-azphoto = Edit photos.
azinstallmgr-app-azpaint = Paint and draw.
azinstallmgr-app-azmusic = Your music library.
azinstallmgr-app-azplayer = Watch videos.
azinstallmgr-app-azvideocut = Cut videos.
azinstallmgr-app-azdrive = Your files, here and in the cloud.
azinstallmgr-app-azkeys = Your passwords, encrypted.
azinstallmgr-app-azmaps = Maps and routes.
azinstallmgr-app-azclock = Alarms, timers and the world clock.
azinstallmgr-app-azcalculator = Calculations, dates and units.
azinstallmgr-app-azcode = Edit code.
azinstallmgr-app-azterm = A terminal.
azinstallmgr-app-azmonitor = What your computer is doing.
azinstallmgr-app-azdashboard = Orders and figures in tables and charts.
azinstallmgr-app-azerp = Assets and records of a business.

## The installed apps
azinstallmgr-check = Check for updates
azinstallmgr-update-all = { $count ->
    [0] Update all
   *[other] Update all ({ $count })
}
azinstallmgr-busy = Wait until the current work is done.
azinstallmgr-nothing-to-update = There is nothing to update.
azinstallmgr-cohort = This computer is in rollout group { $cohort } of 100.
azinstallmgr-no-apps = No Azlin app is installed yet. Run Setup with --screen wizard to install some.
azinstallmgr-column-app = App
azinstallmgr-column-installed = Installed
azinstallmgr-column-available = Available
azinstallmgr-column-channel = Channel
azinstallmgr-column-size = Size
azinstallmgr-column-updated = Last updated
azinstallmgr-column-actions = Actions
azinstallmgr-status-checking = Checking...
azinstallmgr-status-working = Working...
azinstallmgr-status-up-to-date = Up to date
azinstallmgr-status-available = available
azinstallmgr-status-staged = ready to install
azinstallmgr-status-skipped = skipped
azinstallmgr-status-failed = Check failed
azinstallmgr-update = Update
azinstallmgr-revert-to = Revert to { $version }
azinstallmgr-uninstall = Uninstall
azinstallmgr-uninstall-title = Uninstall { $app }?
azinstallmgr-uninstall-text = The app and its previous version are removed from this computer. Your documents and settings stay where they are.
azinstallmgr-owner-elsewhere = Not installed by AzInstallManager
azinstallmgr-owner-readonly = Installed for every user of this computer
azinstallmgr-update-through = Update through { $manager }
azinstallmgr-copied-command = Copied: { $command }
azinstallmgr-updated = { $app } is now { $version }.
azinstallmgr-update-failed = { $app } was not updated: { $reason }
azinstallmgr-reverted = { $app } is back to { $version }. Version { $skipped } is skipped from now on; a newer one is offered again.
azinstallmgr-uninstalled = { $app } was uninstalled.
azinstallmgr-all-up-to-date = Every app is up to date.
azinstallmgr-updates-found = { $count ->
    [one] One update is available.
   *[other] { $count } updates are available.
}
azinstallmgr-check-failed = A check failed: { $reason }

## The update settings
azinstallmgr-settings = Update settings
azinstallmgr-back-to-apps = Back to the apps
azinstallmgr-settings-every-app = Every app
azinstallmgr-settings-each-app = Each app
azinstallmgr-settings-channel = Channel
azinstallmgr-settings-this-computer = This computer
azinstallmgr-settings-folder = Install folder
azinstallmgr-settings-source = Update source
azinstallmgr-source-placeholder = https://.../{"{app}"}/{"{channel}"}.json
azinstallmgr-source-help = The apps and their updates come from this address: {"{app}"} stands for the app (azwriter), {"{channel}"} for its channel (stable or beta). Press Enter to keep a new one. Apps a package manager installed are updated by it; this address is for the apps AzInstallManager installs itself.
azinstallmgr-source-none = No update source is set, so nothing can be installed or updated yet. Type the address of the apps' update manifests here: whoever gives you the Azlin apps tells you where they are published.
azinstallmgr-source-not-web = An update source is a web address that starts with https:// (or http://).
azinstallmgr-source-unset = none
azinstallmgr-source-unset-reason = Set an update source in the settings first.
azinstallmgr-ready-no-source = Set an update source on the Updates page first: the apps are fetched from it.
azinstallmgr-settings-signatures = Signature check
azinstallmgr-settings-signed = Every release must be signed by the configured root key.
azinstallmgr-settings-digest-only = No root key is configured: only the checksum of a download is checked.
azinstallmgr-settings-group = Rollout group
azinstallmgr-settings-window = Background updates
azinstallmgr-settings-any-time = At any time
azinstallmgr-settings-config = These settings are kept in { $path }.
azinstallmgr-settings-saved = Saved. The apps follow it from their next check.

## The wizard pages' own words
azinstallmgr-destination-field = Destination folder
azinstallmgr-browse = Browse...
azinstallmgr-destination-dialog = Choose the folder for the Azlin apps
azinstallmgr-space-required = Space required:
azinstallmgr-space-available = Space available:
azinstallmgr-destination-empty = Choose a folder to install into.
azinstallmgr-destination-short = There is not enough free space on the drive.
azinstallmgr-show-details = Show details
azinstallmgr-hide-details = Hide details
