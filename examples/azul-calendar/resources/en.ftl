# AzCalendar's words in English (azul's Fluent; the keys are `azcalendar-<area>-<what>`).
# Every key the source names is here and in the other language's file (l10n_tests.rs).

azcalendar-tab-home = HOME

## The window: the ribbon, the panes, FILE's pages

azcalendar-view-day = Day
azcalendar-view-work-week = Work Week
azcalendar-view-week = Week
azcalendar-view-month = Month
azcalendar-view-schedule = Schedule View
azcalendar-view-list = List
azcalendar-colour-blue = Blue
azcalendar-colour-green = Green
azcalendar-colour-purple = Purple
azcalendar-colour-orange = Orange
azcalendar-colour-red = Red
azcalendar-colour-teal = Teal
azcalendar-colour-olive = Olive
azcalendar-colour-grey = Grey
azcalendar-print-daily = Daily Style
azcalendar-print-weekly = Weekly Agenda Style
azcalendar-print-monthly = Monthly Style
azcalendar-file-info = Info
azcalendar-file-open = Open & Export
azcalendar-file-print = Print
azcalendar-file-calendars = Calendars
azcalendar-file-options = Options
azcalendar-file-about = About
azcalendar-module-mail = Mail
azcalendar-module-calendar = Calendar
azcalendar-module-contacts = Contacts
azcalendar-module-tasks = Tasks
azcalendar-new-appointment = New Appointment
azcalendar-new-meeting = New Meeting
azcalendar-today = Today
azcalendar-next-7-days = Next 7 Days
azcalendar-open-calendar = Open Calendar
azcalendar-share-calendar = Share Calendar
azcalendar-navigation-pane = Navigation Pane
azcalendar-todo-bar = To-Do Bar
azcalendar-calendar-information = Calendar information
azcalendar-meeting-server = Meeting server
azcalendar-import-icalendar-file-ics = Import an iCalendar file (.ics)
azcalendar-export-calendar-as-icalendar = Export a calendar as an iCalendar file
azcalendar-new-calendar = New calendar
azcalendar-keyboard-shortcuts = Keyboard shortcuts
azcalendar-events-from-outlook-google = Events from Outlook, Google Calendar, Apple Calendar and others. An event imported again is updated, not added twice.
azcalendar-press-enter-name-rename = Press Enter in a name to rename its calendar. Removing a calendar moves its events into the first one.
azcalendar-azmeet-links-are-made = AzMeet links are made on this computer, so they work offline; this server gets them as soon as it answers.
azcalendar-calendar-like-outlook-s = A calendar like Outlook's, on the azul GUI toolkit: events are files, AzMeet links are made offline, .ics files come in and go out.
azcalendar-sync-meeting-links-now = Sync meeting links now
azcalendar-browse = Browse…
azcalendar-sync-now = Sync now
azcalendar-import = Import
azcalendar-export = Export
azcalendar-add = Add
azcalendar-save = Save
azcalendar-file-import = File to import
azcalendar-calendar-ics-exports-folder = calendar.ics (in the exports folder)
azcalendar-file-name-export = File name to export to
azcalendar-name = Name
azcalendar-new-calendar-s-name = New calendar's name
azcalendar-import-into = Import into
azcalendar-calendar-export = Calendar to export
azcalendar-new-calendar-named-after = A new calendar named after the file
azcalendar-all-calendars = All calendars
azcalendar-show-todo-bar = Show the To-Do bar
azcalendar-show-navigation-pane = Show the navigation pane
azcalendar-tab-view = VIEW
azcalendar-arrange = Arrange
azcalendar-new = New
azcalendar-go-to = Go To
azcalendar-manage-calendars = Manage Calendars
azcalendar-share = Share
azcalendar-current-view = Current View
azcalendar-layout = Layout
azcalendar-look = Look
azcalendar-tab-file = FILE
azcalendar-navigation-pane-label = Navigation pane
azcalendar-date-navigator = Date navigator
azcalendar-my-calendars = My calendars
azcalendar-into = Into
azcalendar-theme-mode = Theme and mode
azcalendar-remove = Remove
azcalendar-no-upcoming-appointments = No upcoming appointments.
azcalendar-type-new-task = Type a new task
azcalendar-appearance = Appearance
azcalendar-status-sending = { $count ->
    [one] Sending 1 meeting link…
   *[other] Sending { $count } meeting links…
 }
azcalendar-status-links-done = Meeting links up to date
azcalendar-status-links-unreached = { $count ->
    [one] 1 meeting link waits: the server is not reached
   *[other] { $count } meeting links wait: the server is not reached
 }
azcalendar-status-links-waiting = { $count ->
    [one] 1 meeting link waits for the server
   *[other] { $count } meeting links wait for the server
 }
azcalendar-status-items = Items: { $count }
azcalendar-all-day-lower = all day
azcalendar-sync-all-there = Every meeting link is on the meeting server.
azcalendar-sync-sending = { $count ->
    [one] 1 meeting link is being sent to the meeting server.
   *[other] { $count } meeting links are being sent to the meeting server.
 }
azcalendar-sync-waiting = { $count ->
    [one] 1 meeting link waits: { $why }
   *[other] { $count } meeting links wait: { $why }
 }
azcalendar-info-counts = { $events } events ({ $repeating } repeating) in { $calendars } calendars, { $tasks } tasks.
azcalendar-info-data-folder = Data folder: { $folder }
azcalendar-calendar-name-of = Name of { $name }
azcalendar-calendar-colour-of = Colour of { $name }
azcalendar-keys-new-appointment = New appointment
azcalendar-keys-new-meeting = New meeting
azcalendar-keys-views = Day, Work Week, Week, Month, Schedule View, List
azcalendar-keys-back-forward = Back, forward
azcalendar-keys-pane = The next / previous pane
azcalendar-keys-save-close = Save & Close, in the event window
azcalendar-share-later = Sharing calendars comes later. Meanwhile, FILE > Open & Export saves a calendar as an .ics file anyone can import, and FILE > Print makes a PDF of it.
azcalendar-opening = Opening { $app }…
azcalendar-app-not-started = { $app } could not be started: { $why }
azcalendar-app-missing = { $app } is not installed next to AzCalendar.
azcalendar-this-module = This module
azcalendar-module-not-built = { $module } is not part of this build yet.
azcalendar-import-dialog = Import an iCalendar file
azcalendar-import-give-file = Give the file to import, or Browse for it.
azcalendar-reading = Reading { $file }…
azcalendar-imported-name = Imported
azcalendar-import-left-out = "{ $title }" is left out: { $why }.
azcalendar-imported = Imported { $added } new and { $updated } updated events from { $file }.
azcalendar-exported = { $count ->
    [one] Exported 1 event to { $path }.
   *[other] Exported { $count } events to { $path }.
 }
azcalendar-calendar-needs-name = A calendar needs a name.
azcalendar-calendar-give-name = Give the new calendar a name.
azcalendar-calendar-exists = There is a calendar named "{ $name }" already.
azcalendar-server-give-address = Give the meeting server's address, such as https://meet.example.com or http://127.0.0.1:8787.

## Views and printing

azcalendar-more = +{ $count } more
azcalendar-agenda-today = Today, { $day }
azcalendar-agenda-tomorrow = Tomorrow, { $day }
azcalendar-dismiss = Dismiss
azcalendar-empty-calendar = This calendar has no events yet: click a time to make one, or import an iCalendar (.ics) file.
azcalendar-import-more = Import…
azcalendar-back = Back
azcalendar-forward = Forward
azcalendar-open-day = Open { $day }
azcalendar-more-on = { $count } more on { $day }
azcalendar-agenda-empty = Nothing in these seven days
azcalendar-agenda-empty-detail = Events of the calendars shown come here, day by day.
azcalendar-print-pages-landscape = { $pages ->
    [one] 1 page, A4 landscape
   *[other] { $pages } pages, A4 landscape
 }
azcalendar-print-pages-portrait = { $pages ->
    [one] 1 page, A4 portrait
   *[other] { $pages } pages, A4 portrait
 }
azcalendar-all-day = All day
azcalendar-print-week = Week { $week }
azcalendar-print-no-pdf = azul made no PDF: this build of azul has no PDF writer (its `pdf` feature).
azcalendar-print-style = Print style
azcalendar-print-range = Print range
azcalendar-print-start = Start
azcalendar-print-end = End
azcalendar-print-what = Print saves the printout as a PDF file, to print from there or to keep.
azcalendar-print-date-of = { $what } of the printout
azcalendar-print-making-preview = Making the preview…
azcalendar-print-updating-preview = Updating the preview…
azcalendar-print-page-of = Page { $page } of { $pages }
azcalendar-print-preview-first = The preview shows the first { $shown } pages; Print saves all { $pages }.
azcalendar-print-not-saved = The printout was not saved.
azcalendar-print-saved = Saved { $name } ({ $what }).
azcalendar-print-printed = Printed { $day } - AzCalendar

## The week, meetings, files and reminders

azcalendar-meet-will-mint = A new AzMeet link is made when you save. It works offline too: the meeting server gets it as soon as it answers.
azcalendar-event-all-day = { $title }, all day
azcalendar-join-meeting = Join meeting
azcalendar-meet-link-waits = AzMeet link waits for the server
azcalendar-add-title = Add title
azcalendar-more-options = More options
azcalendar-add-meet-link = Add AzMeet link
azcalendar-saved-with-link = Saved "{ $title }" with the AzMeet link { $link }
azcalendar-saved-event = Saved "{ $title }".
azcalendar-meet-unreadable = The meeting server sent an answer AzCalendar cannot read ({ $why }).
azcalendar-meet-wrong-link = The meeting server sent a link that is not the AzMeet room it made: { $link }
azcalendar-meet-own-room = The meeting server at { $server } made a room of its own instead of registering the link made here; it needs an update to register links made in AzCalendar.
azcalendar-meet-unreachable = The meeting server at { $server } is unreachable: { $why }
azcalendar-meet-rate-limited = Too many new meetings from this network; try again in a few minutes.
azcalendar-meet-answered-with = The meeting server answered { $status }: { $message }
azcalendar-meet-answered = The meeting server answered { $status }.
azcalendar-meet-timed-out = timed out
azcalendar-meet-no-answer = no answer
azcalendar-read-no-file = Could not read { $path }: there is no such file.
azcalendar-read-failed-why = Could not read { $path }: { $why }
azcalendar-read-failed = Could not read { $path }.
azcalendar-write-failed = Could not write { $path }: { $why }. It is tried again in a moment.
azcalendar-changes-not-written = { $count ->
    [one] 1 change could not be written. Close the window again to quit without it.
   *[other] { $count } changes could not be written. Close the window again to quit without them.
 }
azcalendar-untitled = (No title)
azcalendar-menu-open = Open & Export…
azcalendar-menu-print = Print…
azcalendar-menu-calendars = Calendars…
azcalendar-menu-view = View
azcalendar-menu-go-to-today = Go To Today
azcalendar-menu-settings = Settings
azcalendar-menu-meeting-server = Meeting server…
azcalendar-reminder-on = Reminder: { $title } is on { $day }{ $place }.
azcalendar-reminder-at = Reminder: { $title } starts at { $time }{ $place }.
azcalendar-reminder-day-at = Reminder: { $title } starts { $day } at { $time }{ $place }.
azcalendar-program-missing = { $path } does not exist
azcalendar-program-folder-unknown = the AzCalendar program's folder is unknown
azcalendar-opening-azmeet = Opening AzMeet for "{ $title }"…
azcalendar-azmeet-not-started = AzMeet could not be started ({ $why }), so the meeting link was copied: { $link }

## The event window

azcalendar-reminder-none = None
azcalendar-reminder-at-start = At the start
azcalendar-reminder-5-minutes = 5 minutes before
azcalendar-reminder-10-minutes = 10 minutes before
azcalendar-reminder-15-minutes = 15 minutes before
azcalendar-reminder-30-minutes = 30 minutes before
azcalendar-reminder-1-hour = 1 hour before
azcalendar-reminder-1-day = 1 day before
azcalendar-not-an-occurrence = This appointment is not an occurrence of a series.
azcalendar-repeat-at-least-once = Repeat it at least once.
azcalendar-repeat-ends-before = The repeat must end on or after the event's first day.
azcalendar-times-end-that-day = An event with times ends on the day it starts: make it all day to span days.
azcalendar-untitled-window = Untitled
azcalendar-kind-meeting = Meeting
azcalendar-kind-appointment = Appointment
azcalendar-editor-title = { $title } - { $kind }
azcalendar-give-a-title = Give the event a title.
azcalendar-end-after-start = The event must end after it starts.
azcalendar-end-on-first-day = The event must end on or after its first day.
azcalendar-cannot-save = This event cannot be saved: { $why }.
azcalendar-not-an-address = { $who } is not an e-mail address.
azcalendar-repeat-never = Does not repeat
azcalendar-repeat-weekdays = Every weekday (Monday to Friday)
azcalendar-repeat-custom-rule = Custom: { $rule }
azcalendar-repeat-custom = Custom
azcalendar-editor-already-open = An appointment is open in its own window: save or close it first.
azcalendar-editor-closed = This appointment is closed.
azcalendar-editor-actions = Actions
azcalendar-save-close = Save & Close
azcalendar-delete = Delete
azcalendar-delete-occurrence = Delete This Occurrence
azcalendar-ribbon-add-meet = Add AzMeet Link
azcalendar-close = Close
azcalendar-tab-meeting = MEETING
azcalendar-tab-appointment = APPOINTMENT
azcalendar-occurrence-only = This occurrence only - choose "The whole series" to change how it repeats.
azcalendar-replace = Replace
azcalendar-editor-edit = Edit
azcalendar-this-occurrence = This occurrence ({ $day })
azcalendar-whole-series = The whole series
azcalendar-editor-subject = Subject
azcalendar-add-a-title = Add a title
azcalendar-editor-attendees = Attendees
azcalendar-editor-location = Location
azcalendar-where = Where?
azcalendar-start-date = Start date
azcalendar-start-time = Start time
azcalendar-editor-start = Start
azcalendar-end-date = End date
azcalendar-end-time = End time
azcalendar-editor-end = End
azcalendar-editor-repeat = Repeat
azcalendar-editor-reminder = Reminder
azcalendar-editor-calendar = Calendar
azcalendar-link-waits = { $link } (waits for the meeting server)
azcalendar-notes = Notes
azcalendar-series-gone = The series of this occurrence is gone: it was deleted meanwhile.
azcalendar-occurrence-removed = Removed the occurrence of { $day }.
azcalendar-deleted-event = Deleted "{ $title }".
