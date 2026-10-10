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
