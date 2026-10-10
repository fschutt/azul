# AzMail's words in English (azul's Fluent; the keys are `azmail-<area>-<what>`).
# Every key the source names is here and in the other language's file (l10n_tests.rs).

azmail-tab-home = Home

## The main window: the ribbon, its menus and notices

azmail-tab-send-receive = Send / Receive
azmail-tab-folder = Folder
azmail-tab-view = View
azmail-tab-file = File
azmail-group-new = New
azmail-group-delete = Delete
azmail-group-respond = Respond
azmail-group-quick-steps = Quick Steps
azmail-group-move = Move
azmail-group-tags = Tags
azmail-group-find = Find
azmail-group-send-receive = Send & Receive
azmail-group-download = Download
azmail-group-server = Server
azmail-group-actions = Actions
azmail-group-clean-up = Clean Up
azmail-group-properties = Properties
azmail-group-arrangement = Arrangement
azmail-group-layout = Layout
azmail-group-message = Message
azmail-cmd-new-mail = New E-mail
azmail-cmd-new-items = New Items
azmail-cmd-ignore = Ignore
azmail-cmd-clean-up = Clean Up
azmail-cmd-junk = Junk
azmail-cmd-delete = Delete
azmail-cmd-archive = Archive
azmail-cmd-reply = Reply
azmail-cmd-reply-all = Reply All
azmail-cmd-forward = Forward
azmail-cmd-meeting = Meeting
azmail-cmd-more = More
azmail-cmd-move = Move
azmail-cmd-rules = Rules
azmail-cmd-unread-read = Unread/ Read
azmail-cmd-categorize = Categorize
azmail-cmd-follow-up = Follow Up
azmail-cmd-address-book = Address Book
azmail-cmd-filter = Filter E-mail
azmail-cmd-cancel-all = Cancel All
azmail-cmd-send-receive-all = Send/Receive All Folders
azmail-cmd-update-folder = Update Folder
azmail-cmd-send-all = Send All
azmail-cmd-send-receive-groups = Send/Receive Groups
azmail-cmd-show-progress = Show Progress
azmail-cmd-download-headers = Download Headers
azmail-cmd-mark-download = Mark to Download
azmail-cmd-unmark-download = Unmark to Download
azmail-cmd-process-headers = Process Marked Headers
azmail-cmd-new-folder = New Folder
azmail-cmd-new-search-folder = New Search Folder
azmail-cmd-rename-folder = Rename Folder
azmail-cmd-copy-folder = Copy Folder
azmail-cmd-move-folder = Move Folder
azmail-cmd-delete-folder = Delete Folder
azmail-cmd-mark-all-read = Mark All as Read
azmail-cmd-run-rules = Run Rules Now
azmail-cmd-clean-up-folder = Clean Up Folder
azmail-cmd-delete-all = Delete All
azmail-cmd-folder-properties = Folder Properties
azmail-cmd-date = Date
azmail-cmd-reverse-sort = Reverse Sort
azmail-cmd-unread-only = Unread Only
azmail-cmd-navigation-pane = Navigation Pane
azmail-cmd-reading-pane = Reading Pane
azmail-cmd-todo-bar = To-Do Bar
azmail-cmd-plain-text = Plain Text
azmail-quick-move-to = Move to: ?
azmail-quick-team = Team E-mail
azmail-quick-reply-delete = Reply & Delete
azmail-quick-to-manager = To Manager
azmail-quick-done = Done
azmail-quick-create-new = Create New
azmail-menu-mail-message = E-mail Message
azmail-menu-appointment = Appointment
azmail-menu-meeting = Meeting
azmail-menu-contact = Contact
azmail-menu-task = Task
azmail-menu-forward-attachment = Forward as Attachment
azmail-menu-reply-meeting = Reply with Meeting
azmail-menu-flag = Flag / Clear Flag
azmail-menu-all-mail = All Mail
azmail-menu-unread = Unread
azmail-menu-all-accounts = All Accounts
azmail-menu-normal = Normal
azmail-menu-minimized = Minimized
azmail-menu-right = Right
azmail-menu-off = Off
azmail-find-contact = Find a Contact
azmail-notice-read-only = AzMail keeps the server's folders as they are (it receives read-only); deleting, moving and filing come with two-way sync.
azmail-notice-quick-steps = AzMail's Quick Steps are fixed: Move to, Team E-mail, Reply & Delete, To Manager, Done.
azmail-notice-whole-messages = AzMail downloads whole messages: there are no headers to mark.
azmail-notice-meetings = Meetings are AzCalendar's: plan one there.
azmail-notice-categories = Categories come with two-way sync.
azmail-notice-by-date = Messages are arranged by date.
azmail-nothing-running = Nothing is being sent or received.

## The main window: the list, the reading pane, the status bar, the To-Do bar, the panes

azmail-message-list = Message list
azmail-about-summary = Your mail as plain files: every folder of your IMAP account synced to this computer, read in the Outlook 2010 layout, written in azul's rich-text editor and sent directly or through an SMTP server. Part of the Azlin apps, built with azul.
azmail-about-credits = Built with
azmail-category-mail = Mail
azmail-newest-on-top = Newest on top
azmail-option-plain-text = Read as plain text
azmail-option-note = The View tab's switches; AzMail remembers them. The accounts are under File > Info.
azmail-notice-outbox-first = The Outbox's mail is not in the drive yet: it is sent first.
azmail-notice-already-there = The messages are in that folder already.
azmail-notice-select-first = Select a message first.
azmail-error-download = Could not download this message: { $why }
azmail-notice-marks-wait = The read and flag marks wait for the next Send/Receive: { $why }
azmail-notice-moved = { $count ->
    [one] Moved 1 message to { $folder }.
   *[other] Moved { $count } messages to { $folder }.
 }
azmail-notice-not-moved = Could not move to { $folder }: { $why }
azmail-notice-deleted = { $count ->
    [one] Deleted 1 message for good.
   *[other] Deleted { $count } messages for good.
 }
azmail-notice-not-deleted = Could not delete: { $why }
azmail-progress = Send/Receive: { $status } ({ $percent }%).
azmail-progress-last = Last Send/Receive: { $text }
azmail-no-folder = No folder
azmail-folder-facts = { $folder }: { $items } items, { $unread } unread.
azmail-filter-applied = Filter applied
azmail-status-items = Items: { $count }
azmail-status-unread = Unread: { $count }
azmail-status-syncing = { $status } ({ $percent }%)
azmail-no-account = No account
azmail-up-to-date-azlin = All folders are up to date.   Connected to the Azlin drive { $drive }
azmail-up-to-date-server = All folders are up to date.   Connected to { $server }
azmail-up-to-date = All folders are up to date.
azmail-todo-no-appointments = No upcoming appointments.
azmail-todo-new-task = Type a new task
azmail-task-not-saved = The task could not be saved: { $why }
azmail-module-mail = Mail
azmail-module-calendar = Calendar
azmail-module-contacts = Contacts
azmail-module-tasks = Tasks
azmail-favorites-hint = Drag Your Favorite Folders Here
azmail-no-account-yet = No account yet
azmail-no-account-detail = Add an e-mail account to receive mail. AzMail keeps a copy of every folder as files on this computer. Writing needs no account: a new message is sent from this computer, and Local Folders keep what you write.
azmail-add-account = Add Account…
azmail-list-to = To: { $name }
azmail-no-sender = (no sender)
azmail-no-subject = (no subject)
azmail-search-folder = Search { $folder }
azmail-arrange-by = Arrange By:
azmail-oldest-on-top = Oldest on top
azmail-module-calendar-detail = Appointments live in AzCalendar.
azmail-module-contacts-detail = The address book is not part of AzMail yet.
azmail-module-tasks-detail = Tasks of this run are in the To-Do bar.
azmail-select-item = Select an item to read
azmail-select-item-detail = Click a message in the list to see it here.
azmail-field-sent = Sent
azmail-field-to = To
azmail-field-cc = Cc
azmail-see-more-about = See more about: { $name }.
azmail-some-pictures = some pictures
azmail-pictures-held = Click here to download pictures. To help protect your privacy, AzMail prevented automatic download of { $held } in this message.
azmail-download-pictures = Download pictures
azmail-more-lines = ({ $count } more lines)
azmail-html-not-shown = The HTML part could not be shown: { $why }
azmail-attachment-later = { $name } is in the message file; saving attachments comes next.

## The folders

azmail-folder-inbox = Inbox
azmail-folder-drafts = Drafts
azmail-folder-sent = Sent Items
azmail-folder-trash = Deleted Items
azmail-folder-junk = Junk E-mail
azmail-folder-archive = Archive
azmail-folder-all = All Mail
azmail-folder-flagged = Flagged
azmail-folder-outbox = Outbox
azmail-local-folders = Local Folders

## Send/Receive, the keyring, the Outbox, writing files

azmail-error-no-account = no account
azmail-error-not-mail = This file is not a mail message.
azmail-downloading = Downloading this message ({ $size }) from the Azlin drive…
azmail-error-read = Could not read { $path }: { $why }
azmail-keyring-stored = stored
azmail-keyring-retrieved = retrieved
azmail-keyring-deleted = deleted
azmail-keyring-not-found = not found
azmail-keyring-denied = denied
azmail-keyring-unavailable = unavailable
azmail-keyring-error = error
azmail-keyring-password-saved = The password is saved in the system keyring.
azmail-keyring-password-not-saved = The password could not be saved in the system keyring ({ $outcome }): AzMail keeps it only until it is closed.
azmail-keyring-enter-token = Enter the drive token again: the system keyring has none for this account ({ $outcome }).
azmail-keyring-enter-password = Enter the password again: the system keyring has none for this account ({ $outcome }).
azmail-keyring-dkim-saved = The DKIM key is saved in the system keyring.
azmail-keyring-dkim-not-saved = The DKIM key could not be saved in the system keyring ({ $outcome }): AzMail keeps it only until it is closed - create a new key then.
azmail-keyring-no-dkim = The system keyring has no DKIM key for this account ({ $outcome }): signed mail waits in the Outbox until you create a new key under Account Settings, Sending.
azmail-reading-token = Reading the drive token from the system keyring…
azmail-reading-password = Reading the password from the system keyring…
azmail-reading-dkim = Reading the DKIM key from the system keyring…
azmail-connecting-azlin = Connecting to the Azlin drive…
azmail-connecting = Connecting to { $server }…
azmail-token-not-saved = The Azlin drive's new token could not be saved in the system keyring ({ $why }): AzMail keeps it only until it is closed, then asks for a drive token again.
azmail-receiving-folder = Receiving { $folder } (folder { $index } of { $count })
azmail-receiving-messages = Receiving { $folder }: { $done } of { $total } messages
azmail-new-messages = { $count ->
    [one] 1 new message.
   *[other] { $count } new messages.
 }
azmail-sign-in-failed = Sign-in failed: { $why }
azmail-send-receive-error = Send/Receive error: { $why }
azmail-outbox-empty-no-account = Nothing waits in the Outbox. To receive mail, add an account: File > Info > Add Account.
azmail-outbox-empty-local = Nothing waits in the Outbox of Local Folders.
azmail-sending-outbox = Sending the Outbox…
azmail-outbox-counts = Outbox: { $sent } sent, { $queued } waiting, { $failed } failed.
azmail-error-azlin-sign-in-old = the Azlin drive's sign-in is out of date: Send/Receive (F9) signs in again
azmail-notice-send-receive-first = Send/Receive (F9) first: AzMail signs in to the Azlin drive then.
azmail-error-azlin-only = This message is in the Azlin drive only: Send/Receive (F9) signs in, then it opens.
azmail-error-write-sending = Could not write the sending settings: { $why }
azmail-error-write-account = Could not write the account file: { $why }
azmail-error-save-marks = Could not save the read marks: { $why }
azmail-error-write = Could not write { $key }: { $why }
azmail-bridge-no-password = The system keyring has no password of the bridge ({ $outcome }): azul-bridge password makes a new one.

## Account Settings: Other programs (the Azlin Bridge)

azmail-bridge-copy = Copy
azmail-bridge-not-set-up = The Azlin Bridge lets Apple Mail, Thunderbird or Outlook, Finder or Explorer and calendar and contacts programs reach your Azlin drive on this computer. It is not set up here: run azul-bridge init --address <your address>, then azul-bridge serve (azul-bridge autostart enable starts it at every login).
azmail-bridge-running = The Azlin Bridge is running on this computer. Set up the other program with these settings and the bridge's password:
azmail-bridge-not-running = The Azlin Bridge is set up but not running: start it with azul-bridge serve (azul-bridge autostart enable starts it at every login). The other programs use these settings and the bridge's password:
azmail-bridge-mail = Mail (Apple Mail, Thunderbird, Outlook)
azmail-bridge-files = Files (Finder, Explorer, the file managers)
azmail-bridge-calendars = Calendars and contacts
azmail-bridge-copy-all = Copy all settings
azmail-bridge-every-setting = every setting
azmail-bridge-copy-password = Copy password
azmail-bridge-copied = Copied: { $what }.
azmail-bridge-no-file-password = The bridge's secrets file has no password: azul-bridge password makes a new one.
azmail-bridge-asking-keyring = Asking the system keyring for the bridge's password …
azmail-bridge-password-copied = Copied the bridge's password: paste it where the other program asks for the password.

## The account wizard and Account Settings

azmail-acct-step-account = Your account
azmail-acct-step-incoming = Incoming mail
azmail-acct-step-sending = Sending
azmail-acct-step-finish = Finish
azmail-acct-category-account = Account
azmail-acct-category-other = Other programs
azmail-acct-kind-imap = IMAP server
azmail-acct-kind-azlin = Azlin drive
azmail-acct-azlin-note = AzMail keeps your mail as files in your Azlin drive (one file per message, under mail/) and a copy on this computer. The drive token stays in the system keyring.
azmail-acct-azlin-sending-note = Your Azlin drive stores your mail; it does not send it. AzMail sends from this computer as chosen here, and the next Send/Receive puts the copy from Sent Items into the drive.
azmail-acct-saved = The account settings are saved.
azmail-acct-added = { $address } was added.
azmail-acct-enter-address = Enter your e-mail address.
azmail-acct-enter-token = Enter the drive token, or create a new drive.
azmail-acct-paste-token = Paste your OAuth access token.
azmail-acct-enter-password = Enter your password or app password.
azmail-form-bad-email = Enter your mail address (name@example.org).
azmail-form-no-imap-host = Enter the IMAP server.
azmail-form-bad-port = The { $field } port "{ $value }" is not a port (1 - 65535).
azmail-form-plain-not-local = An unencrypted connection is only allowed to a test server on this computer, not to { $host }: your password would cross the network in the clear.
azmail-form-no-token-server = Enter the Azlin token server (https://...), or name it in the endpoints of ~/.azlin/config.json, in AZLIN_TOKEN_URL or with --azlin-token-url.
azmail-form-no-drive = Enter the drive id (d_...), or create a new drive.
azmail-acct-saving = Saving the account and connecting…
azmail-acct-add-title = Add Account
azmail-acct-back = < Back
azmail-acct-next = Next >
azmail-acct-save = Save
azmail-acct-oauth-token = OAuth access token (XOAUTH2):
azmail-acct-password = Password:
azmail-acct-keep-saved = Leave empty to keep the saved one
azmail-acct-type = Account type:
azmail-acct-imap-note = AzMail signs in over IMAP and keeps a copy of every folder on this computer. The password stays in the system keyring.
azmail-acct-your-name = Your Name:
azmail-acct-name-example = Example: Ada Lovelace
azmail-acct-address = E-mail Address:
azmail-acct-address-example = Example: ada@example.org
azmail-acct-app-password-note = Gmail and iCloud need an app password, not your normal password.
azmail-provider-gmail = Gmail needs an app password, not your Google password: turn on 2-Step Verification, then create one at myaccount.google.com/apppasswords.
azmail-provider-outlook = Outlook.com and Microsoft 365 no longer accept passwords over IMAP for most accounts: tick "OAuth access token" and paste an access token.
azmail-provider-icloud = iCloud needs an app-specific password: create one at account.apple.com under Sign-In and Security, App-Specific Passwords.
azmail-provider-fastmail = Fastmail needs an app password: Settings, Privacy & Security, Manage app passwords.
azmail-acct-use-oauth = Sign in with an OAuth access token (XOAUTH2) instead of a password
azmail-acct-token-server-example = https://... (your Azlin provider's token server)
azmail-acct-keep-token = Leave empty to keep the one AzMail has
azmail-acct-token-server = Azlin token server:
azmail-acct-drive-id = Drive id:
azmail-acct-drive-token = Drive token:
azmail-acct-drive-token-note = A drive token for this computer, from your Azlin provider or AzDrive's devices. Every sign-in replaces it with a new one: give AzMail a token of its own, not one AzDrive uses.
azmail-acct-create-drive = Create a new drive
azmail-acct-asking-drive = Asking the token server for a new drive…
azmail-acct-new-drive-what = A new, empty drive at this token server (a development token server's: a real one comes from your Azlin provider).
azmail-acct-local-folder = Local mail folder:
azmail-acct-imap-server = Incoming mail server (IMAP) and port:
azmail-acct-user-name = User Name:
azmail-acct-unencrypted = Unencrypted connection (only for a test server on this computer)
azmail-acct-send-mail = Send mail:
azmail-acct-smtp-server = Outgoing mail server (SMTP) and port:
azmail-acct-tls-implicit = encrypted from the first byte
azmail-acct-tls-starttls = encrypted with STARTTLS before the sign-in
azmail-acct-submission-note = AzMail signs in to { $host } port { $port } (the outgoing server on the Servers page) with this account's password or token, { $protection }, and hands every mail to it. Gmail, iCloud and Fastmail want an app password. For a connection that cannot deliver directly; DKIM below still signs as your own domain.
azmail-acct-direct-note = AzMail hands each mail to the receivers' own mail servers. Some providers take mail only from a trusted server; AzMail remembers those and keeps such mail in the Outbox.
azmail-acct-starttls = Use STARTTLS when the server offers it
azmail-acct-dkim = Sign my mail with DKIM (needs a domain of your own whose DNS you can edit)
azmail-acct-domain-selector = Domain and selector:
azmail-acct-create-key = Create a key
azmail-acct-create-new-key = Create a new key
azmail-acct-check-dns = Check DNS
azmail-acct-working = Working…
azmail-acct-dkim-what = AzMail makes the key on this computer and keeps its private half in the system keyring; you publish the public half in your domain's DNS.
azmail-acct-publish-txt = Publish this TXT record in your domain's DNS:
azmail-acct-zone-line = As a line of a zone file:
azmail-acct-new-key-note = A new key: Save puts it into the system keyring. Until its record is published, receivers cannot check the signature.
azmail-dkim-note-dmarc = DMARC: publish a TXT record { $name } with "{ $value }". DMARC passes on DKIM alone, because the signature names { $domain }, the From address's own domain. Receivers send their reports to { $address }; once they look clean, p=quarantine asks them to file failing mail as spam.
azmail-dkim-note-spf = SPF lists the computers that may send for { $domain }. A home connection's address belongs to your Internet provider and changes, so SPF cannot list it: keep the domain's SPF record ending in ~all, not -all (or publish "v=spf1 ~all" if it has none). DKIM carries the mail through DMARC; a hard -all makes receivers that check SPF alone refuse it.
azmail-dkim-note-ptr = Reverse DNS (PTR): receivers look up the name of the address a mail comes from. A home connection has the provider's generic name, and some receivers refuse such addresses (Gmail: 5.7.25 without a PTR; Outlook and others: home address lists such as Spamhaus PBL, 5.7.1). AzMail remembers each domain that refuses and keeps that mail in the Outbox for a relay.
azmail-dkim-note-port = Direct delivery talks to each receiver's mail server on port 25. Many home Internet providers block outgoing port 25; when no mail server can be reached at all, AzMail checks the port and says so.
azmail-dkim-published = DKIM record: published, with this key.
azmail-dkim-revoked = DKIM record: the key at this name is revoked (p= is empty): publish the record above.
azmail-dkim-other-key = DKIM record: another key is published at this name (p={ $key }): publish the record above instead.
azmail-dkim-missing = DKIM record: not found yet (a new record can take up to an hour to show).
azmail-dkim-unknown = DKIM record: DNS could not be asked ({ $why }).
azmail-dkim-dmarc = DMARC: { $record }
azmail-dkim-no-dmarc = DMARC: no record yet (see the note below).
azmail-dkim-spf-hard = SPF: { $record } - it ends in -all, so receivers that check SPF alone refuse mail from this computer; ~all is safer.
azmail-dkim-spf = SPF: { $record }
azmail-dkim-no-spf = SPF: no record (see the note below).
azmail-acct-azlin-drive-at = the Azlin drive { $drive } at { $server }
azmail-acct-finish-what = Finish adds the account and receives its mail.
azmail-acct-summary-account = Account: { $address }
azmail-acct-summary-incoming = Incoming: { $server }
azmail-acct-summary-sending = Sending: { $how }
azmail-acct-new-drive-encrypted = The new drive { $drive } is ready and encrypted: Finish adds it as this account. Its RECOVERY CODE, shown this once and stored nowhere - write it down and keep it apart from this computer (Azlin cannot reset it): { $code }
azmail-acct-new-drive = The new drive { $drive } is ready: Finish adds it as this account.
azmail-acct-no-drive = No drive was made: { $why }
azmail-route-direct = Directly
azmail-route-smtp = Through an SMTP server
azmail-route-provider-sign-in = Through my provider's server (sign in)
azmail-route-direct-delivery = Direct delivery
azmail-route-provider-signed-in = Through my provider's server, signed in
azmail-route-tls-starttls = { ", " }STARTTLS
azmail-route-tls-required = { ", " }STARTTLS required
azmail-route-dkim-signed = { $route }, DKIM-signed ({ $domain })
azmail-dkim-bad-selector = The selector "{ $selector }" cannot be a DNS name: letters, digits and -, at most 63.
azmail-dkim-create-key-first = Create a key first: AzMail signs with a key of its own, whose public half goes into your domain's DNS.
azmail-smtp-enter-server = Enter the SMTP server's name, e.g. smtp.example.org.
azmail-smtp-bad-port = The port is a number from 1 to 65535.

## File: Info, Help, Print

azmail-file-info = Info
azmail-file-print = Print
azmail-file-help = Help
azmail-file-options = Options
azmail-file-exit = Exit
azmail-info-no-account = Add an e-mail account to receive and send mail. AzMail keeps a copy of every folder as files on this computer.
azmail-info-azlin = Azlin drive { $drive } ({ $server }) - { $folders } folders, { $unread } unread
azmail-info-run-server = the token server of this run
azmail-info-imap = IMAP { $server } - { $folders } folders, { $unread } unread
azmail-info-add-account = Add Account
azmail-info-new-mail = Write a message without an account: AzMail sends it from this computer, straight to the recipients' mail servers, and Local Folders keep it (Send/Receive sends what waits in their Outbox).
azmail-info-account-settings = Account Settings
azmail-info-account-settings-what = Modify the settings of this account: your name and password, the incoming server, how mail is sent and signed.
azmail-info-now = Now: { $status } ({ $percent }%).
azmail-info-last-time = Last time: { $text }
azmail-info-send-receive = Send/Receive
azmail-info-send-receive-what = Receive every folder of this account and send what waits in the Outbox (F9). { $status }
azmail-info-mail-will-be-kept = Mail will be kept as plain files, one per message, in { $folder }.
azmail-info-mail-is-kept = Mail is kept as plain files, one per message, in { $folder }.
azmail-info-mailbox = Mailbox
azmail-info-title = Account Information
azmail-help-support = Support
azmail-help-shortcuts = Keyboard Shortcuts
azmail-help-shortcuts-what = Every key AzMail knows, on one page (F1).
azmail-help-tools = Tools for Working With { $app }
azmail-help-options-what = The reading pane, the To-Do bar, the theme, the mode and the other program settings.
azmail-help-mail-folder = Mail folder
azmail-help-accounts = Accounts
azmail-print-from = From:
azmail-print-sent = Sent:
azmail-print-to = To:
azmail-print-cc = Cc:
azmail-print-subject = Subject:
azmail-print-attachments = Attachments:
azmail-print-no-pdf = azul's PDF writer made no file (a build without its `pdf` feature?).
azmail-print-select-first = Select a message to print first.
azmail-printed-to = Printed to { $path }
azmail-print-not-drawn = The first page could not be drawn.
azmail-print-into = Into { $folder }
azmail-print-what = Prints the open message to a PDF file: A4, Memo Style.
azmail-print-printer = Printer
azmail-print-pdf-file = PDF file
azmail-print-settings = Settings
azmail-print-memo = Memo Style
azmail-print-memo-what = Your name over the message's header lines, then its text.
azmail-print-preview = Preview
azmail-print-nothing-open = No message is open: select one in the message list, then come back to File > Print.
azmail-print-pages = { $pages ->
    [one] 1 page
   *[other] Page 1 of { $pages }
 }
azmail-print-drawing = Drawing the preview…
azmail-print-not-read = The PDF could not be read back: { $why }

## The message window

azmail-compose-untitled = Untitled - Message (HTML)
azmail-compose-title = { $subject } - Message (HTML)
azmail-forward-separator = ---------- Forwarded message ----------
azmail-forward-from = From: { $value }
azmail-forward-date = Date: { $value }
azmail-forward-subject = Subject: { $value }
azmail-forward-to = To: { $value }
azmail-forward-cc = Cc: { $value }
azmail-quote-wrote = { $from } wrote:
azmail-quote-on-wrote = On { $date }, { $from } wrote:
azmail-quote-date = { $weekday }, { $day } { $month } { $year } at { $time }
azmail-compose-no-recipient = Add at least one recipient.
azmail-compose-bad-address = "{ $address }" is not an e-mail address.
azmail-compose-no-sender = Type your e-mail address in From: the message is sent from it.
azmail-compose-from-placeholder = Your name <you@example.org>
azmail-compose-closed = This message was closed.
azmail-compose-tab-message = Message
azmail-compose-group-basic-text = Basic Text
azmail-compose-bold = Bold
azmail-compose-italic = Italic
azmail-compose-underline = Underline
azmail-compose-bullets = Bullets
azmail-compose-numbering = Numbering
azmail-compose-group-include = Include
azmail-compose-attach = Attach File
azmail-compose-link = Link
azmail-compose-group-save = Save
azmail-compose-save-draft = Save Draft
azmail-compose-discard = Discard
azmail-compose-local-queued = In the Outbox of Local Folders. { $reason } AzMail tries again at every Send/Receive (F9); with an account (File > Info > Add Account) it can send through your provider instead.
azmail-compose-local-note = No account: AzMail sends this message from this computer, straight to the recipients' mail servers. Local Folders keep its draft and the sent mail.
azmail-compose-azlin-queued = In the Outbox. { $reason } AzMail tries again at every Send/Receive (F9).
azmail-compose-azlin-note = Azlin account: the drive keeps this message's draft and, after the next Send/Receive, its copy in Sent Items. The mail itself leaves from this computer, as Account Settings, Sending says.
azmail-compose-sending = Sending…
azmail-compose-queued = Queued
azmail-compose-send = Send
azmail-compose-from = From
azmail-compose-to = To...
azmail-compose-cc = Cc...
azmail-compose-bcc = Bcc...
azmail-compose-subject = Subject:
azmail-compose-link-address = Address:
azmail-compose-insert-link = Insert Link
azmail-compose-save-question = Do you want to save changes to this message?
azmail-compose-save-question-detail = A saved message is kept in Drafts.
azmail-compose-dont-save = Don't Save
azmail-compose-attached = Attached:
azmail-compose-remove = Remove
azmail-compose-body = Message body
azmail-compose-draft = Draft
azmail-compose-new = New message
azmail-compose-saving = Saving the draft…
azmail-compose-saved = Draft saved at { $time }.
azmail-compose-status-queued = In the Outbox, sent with the next Send/Receive: { $reason }
azmail-compose-not-sent = Not sent: { $reason }
azmail-compose-attachments = { $count ->
    [one] 1 attachment
   *[other] { $count } attachments
 }
azmail-compose-link-first = Type the link's address first.
azmail-compose-account-gone = The account is gone.
azmail-compose-draft-not-saved = The draft could not be saved: { $why }
azmail-compose-in-outbox = In the Outbox: { $subject }
azmail-compose-draft-later = The draft is saved here; the next Send/Receive puts it into the Azlin drive ({ $why }).
azmail-ban-sends-nothing = AzMail sends nothing from this account: its mail waits in the Outbox.

## Options, a mail's web content, the shortcuts

azmail-options-title = AzMail Options
azmail-remote-pictures = { $count ->
    [one] 1 picture
   *[other] { $count } pictures
 }
azmail-remote-fonts = { $count ->
    [one] 1 font
   *[other] { $count } fonts
 }
azmail-remote-style-sheets = { $count ->
    [one] 1 style sheet
   *[other] { $count } style sheets
 }
azmail-remote-and = { $first } and { $last }
azmail-shortcut-leave-file = Leave the File tab
azmail-shortcut-save-draft = Save the draft
