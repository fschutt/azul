# AzCode's words in English (src/l10n.rs). Every key the source names is here and in de.ftl.

## AzCode

azcode-not-a-folder = { $path } is not a folder AzCode can open.
azcode-about-summary = A code editor: the explorer, tabs, syntax colours, find and replace, search in the folder, a terminal, go to line, files of a million lines. Your folders are edited in place; the sample lives in your data folder.
azcode-menu-file = File
azcode-shortcut-window = Window
azcode-action-find = Find
azcode-shortcut-editing = Editing
azcode-shortcut-moving = Moving
azcode-action-explorer = Explorer
azcode-shortcut-open-folder = Open a folder
azcode-shortcut-close-folder = Close the folder
azcode-shortcut-quick-open = Quick open a file of the folder
azcode-shortcut-save = Save every changed file
azcode-shortcut-close-tab = Close the tab
azcode-shortcut-palette = The command palette
azcode-shortcut-side-bar = Show / hide the side bar
azcode-shortcut-explorer = The explorer
azcode-shortcut-terminal = Show / hide the terminal
azcode-shortcut-new-terminal = A new terminal
azcode-shortcut-find-in-files = Search the folder's files
azcode-shortcut-find = Find in the file
azcode-shortcut-replace = Replace in the file
azcode-shortcut-next-match = Next / previous match
azcode-shortcut-go-to-line = Go to line
azcode-shortcut-undo = Undo / redo
azcode-shortcut-select-word = Select the word, then its next occurrence
azcode-shortcut-cursor = Another cursor
azcode-shortcut-indent = Indent / outdent the selected lines
azcode-shortcut-clipboard = Cut / copy / paste (a whole line without a selection)
azcode-shortcut-file-ends = To the start / end of the file
azcode-shortcut-words = By words
azcode-shortcut-tree = Move in the tree, close / open a folder, open a file
azcode-shortcut-escape = Close the palette, the find bar, the go-to bar
azcode-action-open-folder = Open Folder...
azcode-action-open-file = Open File...
azcode-action-open-sample = Open the Sample Workspace
azcode-action-save-all = Save All
azcode-action-close-editor = Close Editor
azcode-action-close-folder = Close Folder
azcode-action-undo = Undo
azcode-action-redo = Redo
azcode-action-replace = Replace
azcode-action-find-in-files = Find in Files
azcode-action-command-palette = Command Palette...
azcode-action-toggle-side-bar = Toggle Side Bar
azcode-action-terminal = Terminal
azcode-action-quick-open = Go to File...
azcode-action-go-to-line = Go to Line...
azcode-action-new-terminal = New Terminal
azcode-action-kill-terminal = Kill Terminal
azcode-action-refresh-explorer = Refresh Explorer
azcode-action-collapse-folders = Collapse Folders in Explorer
azcode-action-settings = Settings
azcode-action-keyboard-shortcuts = Keyboard Shortcuts
azcode-action-about = About AzCode
azcode-menu-edit = Edit
azcode-category-search = Search
azcode-menu-view = View
azcode-menu-go = Go
azcode-category-preferences = Preferences
azcode-menu-help = Help
azcode-no-recent = No Recent Folders
azcode-open-recent = Open Recent
azcode-quick-open-placeholder = Search files by name (type > for the commands)
azcode-quick-open-listing = Listing the folder's files...
azcode-palette-placeholder = Type the name of a command to run
azcode-activity-bar = Activity bar
azcode-editor = Editor
azcode-open-place = Open { $folder }
azcode-unsaved-changes = Unsaved changes
azcode-close-tab = Close { $name }
azcode-welcome-open-folder = Open Folder
azcode-welcome-quick-open = Quick Open a File
azcode-welcome-palette = Command Palette
azcode-welcome-toggle-terminal = Toggle Terminal
azcode-welcome-save = Save
azcode-welcome-close-tab = Close the Tab
azcode-welcome-side-bar = Show / Hide the Side Bar
azcode-welcome-find = Find in the File
azcode-welcome-go-to-line = Go to Line
azcode-welcome-every-shortcut = Every Shortcut
azcode-welcome-what = A code editor on azul
azcode-welcome-start = Start
azcode-welcome-sample = Open the sample workspace
azcode-welcome-recent = Recent
azcode-welcome-shortcuts = Keyboard shortcuts
azcode-find-none = No results
azcode-find-of = { $match } of { $count }
azcode-find-results = { $count ->
    [one] 1 result
   *[other] { $count } results
 }
azcode-find-results-title = Results
azcode-find-previous = Previous
azcode-find-next = Next
azcode-match-case = Match case
azcode-word = Word
azcode-whole-word = Whole word
azcode-replace-with = Replace with
azcode-replace-all = Replace all
azcode-go-to-placeholder = line or line:column
azcode-status-terminals = Terminal ({ $count })
azcode-status-spaces = Spaces: { $count }
azcode-status-saving = Saving
azcode-replaced = Replaced { $count }
azcode-not-a-line = "{ $typed }" is not a line
azcode-files-of = Files of { $folder }
azcode-row-open-folder = , open folder
azcode-row-folder = , folder
azcode-no-folder = You have not yet opened a folder.
azcode-no-folder-what = Or press { $open }, or start AzCode with a folder: AzCode ~/my-project. Its files open in the editor; quick open ({ $quick }) finds them by name.
azcode-search-folder = Search the folder
azcode-search-no-folder = Open a folder to search its files.
azcode-search-results = Search results
azcode-terminal-failed = The terminal could not be started: { $why }
azcode-close-panel = Close Panel
azcode-shell-ended = The shell has ended. New Terminal starts another.
azcode-status-caret = Ln { $line }, Col { $column }
azcode-searching = Searching...
azcode-no-results-found = No results found.
azcode-search-summary = { $hits ->
    [one] 1 result
   *[other] { $hits } results
 } in { $files ->
    [one] 1 file
   *[other] { $files } files
 }
azcode-search-summary-first = { $hits ->
    [one] 1 result
   *[other] { $hits } results
 } in { $files ->
    [one] 1 file
   *[other] { $files } files
 } (the first ones)
azcode-save-first = Save the open files first ({ $keys }).
azcode-folder-gone = { $folder } is not there any more.
azcode-dialog-open-folder = Open a folder
azcode-dialog-open-file = Open a file
azcode-not-a-file = { $path } is not a file AzCode can open.
azcode-tab-unsaved = { $name } has unsaved changes: save it first ({ $keys }).
azcode-quick-open-no-folder = Open a folder first ({ $keys }): quick open searches its files.
azcode-chord-unknown = The key combination ({ $keys }, ...) is not a command.
azcode-chord-waiting = ({ $keys }) was pressed. Waiting for the second key of the chord...
azcode-folder-unlisted = { $folder } could not be listed: { $why }
azcode-file-unread = { $file } could not be read: { $why }
azcode-file-unsaved = { $file } could not be saved: { $why }
azcode-index-failed = The folder's files could not be listed: { $why }
azcode-index-first = Quick open searches the first { $count } files of the folder.
