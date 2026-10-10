# AzTasks' words in English (src/l10n.rs). Every key the source names is here and in de.ftl.

## The window
aztasks-tasks = Tasks

## Commands, views, the ribbon and the status bar

aztasks-smart-today = Today
aztasks-smart-upcoming = Upcoming
aztasks-smart-scheduled = Scheduled
aztasks-smart-flagged = Flagged
aztasks-smart-all = All
aztasks-smart-completed = Completed
aztasks-overdue = Overdue
aztasks-completed-count = Completed ({ $count })
aztasks-other-list = Other
aztasks-column-to-do = To do
aztasks-column-doing = Doing
aztasks-column-done = Done
aztasks-cmd-new-task = New task
aztasks-cmd-new-list = New list
aztasks-cmd-complete = Complete
aztasks-cmd-flag = Flag
aztasks-cmd-delete = Delete
aztasks-cmd-move-up = Move up
aztasks-cmd-move-down = Move down
aztasks-cmd-show = Show { $view }
aztasks-cmd-search = Search
aztasks-cmd-settings = Settings
aztasks-cmd-shortcuts = Keyboard shortcuts
aztasks-cmd-about = About AzTasks
aztasks-cmd-todo-bar = To-Do bar
aztasks-cmd-navigation = Navigation pane
aztasks-cmd-completed = Completed tasks in lists
aztasks-cmd-sort-manual = Sort by manual
aztasks-cmd-sort-due = Sort by due date
aztasks-cmd-sort-priority = Sort by priority
aztasks-cmd-sort-title = Sort by title
aztasks-cmd-sort-created = Sort by created
aztasks-cmd-theme-flat = Flat theme
aztasks-cmd-theme-flora = Flora theme
aztasks-cmd-mode-light = Light mode
aztasks-cmd-mode-dark = Dark mode
aztasks-cmd-mode-system = Mode of the system
aztasks-cmd-palette = Command palette
aztasks-category-task = Task
aztasks-category-go = Go
aztasks-category-app = App
aztasks-category-view = View
aztasks-move-to = Move to...
aztasks-move-selected-to = Move the selected tasks to
aztasks-tab-home = HOME
aztasks-group-new = New
aztasks-group-manage = Manage
aztasks-group-arrange = Arrange
aztasks-group-move = Move
aztasks-tab-view = VIEW
aztasks-group-sort-by = Sort by
aztasks-group-show = Show
aztasks-group-appearance = Appearance
aztasks-tab-file = FILE
aztasks-status-due-today = { $count } due today
aztasks-status-overdue = { $count } overdue
aztasks-status-not-saved = { $count } not saved - retry
aztasks-status-saving = Saving { $count }...
aztasks-status-reading = Reading...
aztasks-status-saved = Saved
aztasks-status-open = { $count } open
aztasks-todo-new-today = New task for today
aztasks-todo-new-for = New task for { $day }
aztasks-todo-no-reminders = No reminders on this day.
aztasks-palette-placeholder = Type a command
aztasks-confirm-delete-list = Delete the list "{ $name }" and its { $count ->
    [one] 1 task
   *[other] { $count } tasks
 }? This cannot be undone.
aztasks-confirm-clear-completed = { $count ->
    [one] Delete the 1 task completed more than 30 days ago?
   *[other] Delete the { $count } tasks completed more than 30 days ago?
 }

## The panes, the settings and the notices

aztasks-page-about = About
aztasks-settings-general = General
aztasks-settings-reminders = Reminders
aztasks-settings-data = Data
aztasks-settings-find = Find a setting
aztasks-settings-default-list = Default list
aztasks-settings-default-list-what = where a new task goes from Today, All or a tag
aztasks-settings-week-starts = Week starts on
aztasks-settings-completed = Completed tasks
aztasks-settings-completed-name = Show completed tasks under a list
aztasks-settings-completed-what = show them under a list's open tasks (folded)
aztasks-settings-os-shows = This system shows them ({ $why }).
aztasks-settings-os-none = Not on this system: { $why }. Reminders show in the window only.
aztasks-settings-reminder-time = Reminder time
aztasks-settings-reminder-time-what = for tasks due on a day without a time, and a new due time
aztasks-settings-sounds = Sounds
aztasks-settings-sounds-name = Play a sound with a reminder
aztasks-settings-sounds-what = play the system's sound with a reminder
aztasks-settings-notifications = Notifications
aztasks-settings-notifications-name = Show reminders as notifications
aztasks-settings-notifications-what = show a reminder as a notification of the system too
aztasks-settings-contents = { $lists ->
    [one] 1 list
   *[other] { $lists } lists
 }, { $tasks ->
    [one] 1 task
   *[other] { $tasks } tasks
 } ({ $open } open), { $skipped ->
    [one] 1 file left out
   *[other] { $skipped } files left out
 }
aztasks-settings-data-folder = Data folder
aztasks-settings-data-folder-what = One file per task: tasks/<list>/<task>.json, a list's tasks/<list>/list.json, its attachments next to the task. The same layout as the S3 bucket the files can move to.
aztasks-settings-contents-title = Contents
aztasks-settings-import-export = Import and export
aztasks-settings-sample = Sample
aztasks-settings-add-sample = Add the sample tasks
aztasks-settings-sample-what = lists and tasks to try AzTasks with
aztasks-settings-import-file = iCalendar file to import
aztasks-settings-browse = Browse...
aztasks-settings-import = Import
aztasks-settings-import-what = To-dos of an iCalendar file (Outlook, Apple Reminders, Thunderbird) go into the default list.
aztasks-settings-export = Export
aztasks-settings-export-what = the list shown (or every task) as an iCalendar file in aztasks/exports
aztasks-keys-up-down = Up / Down
aztasks-keys-up-down-what = Select the task above / below (Shift extends)
aztasks-keys-click = Click / Shift+click / Cmd+click
aztasks-keys-click-what = Select a task / a range / add one
aztasks-keys-drag = Drag a task
aztasks-keys-drag-what = Put it before another (onto another list's task: move it there)
aztasks-keys-enter = Enter (quick add)
aztasks-keys-enter-what = Add the typed task; a click on a chip keeps its words
aztasks-keys-esc = Esc
aztasks-keys-esc-what = Close the palette or the backstage; clear the quick-add line
aztasks-keys-lists = Cmd+7 .. Cmd+9
aztasks-keys-lists-what = Your first three lists
aztasks-keys-note = Cmd is the Command key on macOS and Ctrl elsewhere. Single keys work while no text field has the focus.
aztasks-about-version = Version { $version }
aztasks-about-summary = To-dos and reminders: smart lists, lists in groups, tags, quick add in plain words (English and German), repeating tasks, reminders, steps, notes and files.
aztasks-about-data = Data folder: { $folder }
aztasks-about-notifications = Notifications: { $why }
aztasks-about-no-notifications = Notifications: not on this system ({ $why })
aztasks-about-built = Built on azul. MIT licensed.
aztasks-import-dialog = Import to-dos from an iCalendar file
aztasks-import-give-file = Give the iCalendar file to import, or Browse for it.
aztasks-import-reading = Reading { $path }...
aztasks-exported = { $count ->
    [one] Exported 1 to-do to { $path }.
   *[other] Exported { $count } to-dos to { $path }.
 }
aztasks-load-failed = The tasks could not be read: { $why }
aztasks-attach-failed = "{ $name }" could not be attached: { $why }
aztasks-files-failed = Moving or deleting attachments failed: { $why }
aztasks-imported = { $count ->
    [one] Imported 1 to-do from { $file }.
   *[other] Imported { $count } to-dos from { $file }.
 }
aztasks-imported-some = { $count ->
    [one] Imported 1 to-do
   *[other] Imported { $count } to-dos
 } from { $file }; { $missed } not read: { $first }
aztasks-import-unreadable = Could not read { $file }: { $why }
aztasks-all-tasks = All tasks
aztasks-on-this-computer = On this computer
aztasks-no-tags = No tags yet
aztasks-all-tags = All tags
aztasks-search = Search tasks
aztasks-my-tasks = My Tasks
aztasks-my-lists = My Lists
aztasks-tags = Tags
aztasks-lists = Lists
aztasks-layout-month = Month
aztasks-layout-board = Board
aztasks-layout-list = List
aztasks-month-previous = Previous month
aztasks-month-next = Next month
aztasks-month-this = This month
aztasks-planned-month = Planned month
aztasks-more = +{ $count } more
aztasks-board-of = { $list } board
aztasks-board-empty-to-do = Nothing to do here.
aztasks-board-empty-doing = Drag a card here when you start it.
aztasks-board-empty-done = Drag a card here when it is done.
aztasks-completed-on = Completed { $day }
aztasks-import-no-list = There is no list to import the to-dos into: make a list first.
aztasks-deleted-one = Deleted "{ $title }".
aztasks-deleted-many = Deleted { $count } tasks.
aztasks-deleted-list = Deleted the list "{ $name }".
aztasks-cleared = { $count ->
    [one] Cleared 1 completed task.
   *[other] Cleared { $count } completed tasks.
 }
aztasks-sample-added = Sample lists and tasks were added.
aztasks-sample-not-added = The data folder has tasks already; --sample adds nothing.
aztasks-notification-due = { $title } (due { $due })
aztasks-reminder = Reminder
aztasks-snooze = Snooze 10 min
aztasks-reminder-none = None
aztasks-reminder-at-due = At the due time
aztasks-reminder-5-minutes = 5 minutes before
aztasks-reminder-15-minutes = 15 minutes before
aztasks-reminder-1-hour = 1 hour before
aztasks-reminder-1-day = 1 day before
aztasks-reminder-on-date = On a date...
aztasks-banner-one = Reminder: { $title }
aztasks-banner-two = 2 reminders: { $a }, { $b }
aztasks-banner-many = { $count } reminders: { $a }, { $b } and { $more } more
aztasks-dismiss = Dismiss
aztasks-list-settings = List settings
aztasks-clear-older-than-30 = Clear older than 30 days
aztasks-undo = Undo
aztasks-add = Add
aztasks-close-notice = Close the notice
aztasks-add-task = Add a task
aztasks-enter-add-click-part = Enter to add · a click on a part keeps its words
aztasks-reading-your-tasks = Reading your tasks...
aztasks-no-tasks-yet = No tasks yet
aztasks-type-one-above-like = Type one above, like "Pay rent tomorrow 9am #home !high", or try the sample lists.
aztasks-delete-list = Delete list...
aztasks-list-name = List name
aztasks-no-group-type-one = No group (or type one: "Azlin launch")
aztasks-group = Group
aztasks-new-tasks-outside-list = New tasks outside a list go here
aztasks-move = Move to
aztasks-ctrl-cmd-click-adds = Ctrl / Cmd + click adds a task, Shift + click a range; Space completes, Delete deletes.
aztasks-add-time = Add time
aztasks-no-time = No time
aztasks-clear = Clear
aztasks-open = Open
aztasks-attach-file = Attach a file...
aztasks-title = Title
aztasks-notes = Notes
aztasks-add-step = Add a step
aztasks-add-tag = Add a tag
aztasks-due-date = Due date
aztasks-due-time = Due time
aztasks-repeat = Repeat
aztasks-custom-repeat = Custom repeat
aztasks-reminder-date = Reminder date
aztasks-notes-2 = NOTES
aztasks-set-due-date-reminder = Set a due date for this reminder
aztasks-files = FILES
aztasks-drop-files-window = or drop files on the window
aztasks-no-task-selected = No task selected
aztasks-select-task-see-steps = Select a task to see its steps, dates, repeat, reminder, tags, notes and files.
aztasks-count-completed = { $count } completed
aztasks-tagged-tasks = Tagged tasks
aztasks-search-title = Search: { $query }
aztasks-search-what = Title, notes, tags and steps
aztasks-quick-add-to = Add to { $list }: "Pay rent tomorrow 9am #home !high"
aztasks-quick-add = Add a task: "Pay rent tomorrow 9am #home !high"
aztasks-empty-today = Nothing due today
aztasks-empty-today-what = Enjoy the day, or add a task above.
aztasks-empty-upcoming = Nothing in the next 7 days
aztasks-empty-upcoming-what = Tasks with a due date this week show here.
aztasks-empty-scheduled = Nothing scheduled
aztasks-empty-scheduled-what = Give a task a due date to see it here.
aztasks-empty-flagged = No flagged tasks
aztasks-empty-flagged-what = Flag a task ("!" in the quick-add line) to keep it here.
aztasks-empty-all = All done
aztasks-empty-all-what = Every task is completed.
aztasks-empty-completed = Nothing completed yet
aztasks-empty-completed-what = Completed tasks are kept here.
aztasks-empty-list = No tasks in { $list }
aztasks-empty-list-what = Add one above.
aztasks-empty-tag = No open tasks tagged #{ $tag }
aztasks-empty-tag-what = Tags are words with a # in the quick-add line.
aztasks-empty-search = No tasks match "{ $query }"
aztasks-empty-search-what = Search looks at titles, notes, tags and steps.
aztasks-complete-task = Complete { $title }
aztasks-priority-of = { $priority } priority
aztasks-reminder-when = Reminder { $when }
aztasks-attachments = { $count ->
    [one] 1 attachment
   *[other] { $count } attachments
 }
aztasks-colour = Colour
aztasks-name = Name
aztasks-default = Default
aztasks-selected = { $count ->
    [one] 1 task selected
   *[other] { $count } tasks selected
 }
aztasks-open-again = Open again
aztasks-unflag = Unflag
aztasks-repeat-never = Never
aztasks-repeat-daily = Daily
aztasks-repeat-weekdays = Weekdays
aztasks-repeat-weekly = Weekly
aztasks-repeat-two-weeks = Every 2 weeks
aztasks-repeat-monthly = Monthly
aztasks-repeat-yearly = Yearly
aztasks-repeat-custom = Custom...
aztasks-steps-of = STEPS · { $done } OF { $total }
aztasks-steps = STEPS
aztasks-step-done = Done: { $step }
aztasks-step-remove = Remove the step { $step }
aztasks-due = Due
aztasks-next-week = Next week
aztasks-remind-me = Remind me
aztasks-reminds = Reminds { $when }
aztasks-remove-file = Remove { $name }
aztasks-created-on = Created { $when }
aztasks-attach-bad-name = "{ $name }" cannot be a file name in the data folder.
aztasks-attach-dialog = Attach a file
aztasks-attach-select-first = Select a task to attach the dropped files to.
