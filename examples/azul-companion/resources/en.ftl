# The words of the AzReview companion, English. Every key is in de.ftl too (l10n_test.rs checks it).
# A { $name } is filled in by the screen. One line per key.

app-name = AzReview Companion

# ---- navigation
nav-back = Back
nav-close = Close
nav-settings = Settings
tab-remote = Remote
tab-files = Files
tab-buttons = Buttons
tabs-name = Screens

# ---- the connection
conn-connected = Connected
conn-searching = Searching
conn-lost = Connection lost
conn-offline = Not connected
conn-latency = { $ms } ms

# ---- pairing (screen A)
pair-title = Connect to AzReview
pair-intro = This app is a remote control for AzReview on your Mac. It finds your Mac on your Wi-Fi; then you pan and zoom the page, open files and send your recurring instructions from here.
pair-permission-ios = Your iPhone will ask to find devices on your local network. Tap Allow: without it the app cannot see your Mac.
pair-permission-android = Keep your phone on the same Wi-Fi as your Mac.
pair-find = Find my Mac
pair-scan = Scan QR code
pair-cable = Installed from AzReview over a cable? Then you are already paired.
pair-searching-title = Looking for AzReview
pair-searching-body = AzReview must be open on your Mac, and your phone and Mac must be on the same Wi-Fi.
pair-attempt = Attempt { $n }
pair-cancel = Cancel
pair-found-title = Found { $mac }
pair-found-body = Check that AzReview on your Mac shows the same number, then confirm it there.
pair-code-label = Pairing code
pair-waiting = Waiting for your confirmation on the Mac
pair-connected-title = Connected to { $mac }
pair-connected-body = You can pan and zoom the page, browse the files and use your quick buttons from here.
pair-start = Start
pair-back = Back
pair-scan-soon = Scanning the code comes with the next version. For now, install the app from AzReview over the cable: that pairs it.
pair-demo = Try it without a Mac
settings-leave-demo = Leave the demo
settings-demo-note = This is a demo: a pretend Mac is answering. Nothing is sent to your real Mac.

# ---- when it does not work
fail-retry = Try again
fail-scan = Scan QR code
fail-settings = Open Settings
fail-nopermission-title = Local network is off
fail-nopermission-body = The app needs to find your Mac on your Wi-Fi. Open Settings, choose this app and turn on Local Network.
fail-notfound-title = AzReview not found
fail-notfound-body = Check that AzReview is running on your Mac, that the Mac is awake and that your phone is on the same Wi-Fi.
fail-updatecompanion-title = Update the companion
fail-updatecompanion-body = AzReview on your Mac is newer than this app. In AzReview, open Settings, Devices and install the companion again over the cable.
fail-updateazreview-title = Update AzReview
fail-updateazreview-body = This app is newer than AzReview on your Mac. Update AzReview, then try again.
fail-declined-title = Pairing was declined
fail-declined-body = The pairing was declined on your Mac. Start again if that was a mistake.
fail-wrongcode-title = That code did not work
fail-wrongcode-body = The pairing code is damaged or was used already. Show a new code in AzReview and scan it again.
fail-expired-title = The code has expired
fail-expired-body = A pairing code is good for five minutes. Show a new one in AzReview and scan it again.
fail-forgotten-title = AzReview forgot this phone
fail-forgotten-body = The Mac no longer knows this phone. In AzReview, open Settings, Devices, Add phone, and pair again.
fail-busy-title = AzReview is busy
fail-busy-body = Another phone is being paired, or four phones are paired already. Try again in a minute, or remove one in AzReview.
fail-addresschanged-title = Can't reach your Mac
fail-addresschanged-body = AzReview did not answer at the address this phone has saved. Either AzReview is closed, or your Mac's address on the Wi-Fi changed. Open AzReview, then pair again: scan its code, or connect the cable.
fail-pairagain = Pair again
fail-timeout-title = AzReview stopped answering
fail-timeout-body = The Mac did not answer in time. Check that it is awake and on the same Wi-Fi, then try again.

# ---- the remote (screen B)
remote-no-file = No file open in AzReview
remote-page = Page { $n } of { $m }
remote-line = Line { $n }
remote-pad = Touch pad for the page in AzReview
remote-hint = Drag to pan. Pinch to zoom around the pen. Double-tap to fit the width.
remote-zoom = { $percent } %
remote-fit = Fit width
remote-reset = Reset view
remote-prev = Previous page
remote-next = Next page
remote-picture = The page in AzReview, with the part the Mac shows marked
remote-page-strip = Swipe for the next or previous page
remote-context-line = The buttons act on line { $n }
remote-context-region = The buttons act on the marked region
remote-context-page = The buttons act on page { $n }
remote-context-none = Open a file in AzReview to use the buttons
remote-lost = Connection lost. Trying again
remote-hint-board = Drag to move the board. Pinch to zoom. Double-tap to fit the board.
remote-picture-board = The whiteboard in AzReview, with the part the Mac shows marked
remote-overview = Overview of the whiteboard
surface-code = Code review
surface-board = Whiteboard
surface-board-named = Whiteboard { $name }
board-items = { $n } items
board-item = 1 item
board-empty = Empty board
return-back = Back to { $where }
return-where = { $file } p{ $page } l{ $line }
return-where-nolines = { $file } p{ $page }
return-continue = Continue reviewing
return-name = Continue reviewing: go back to { $where }
finished-done = Agent finished: { $where }
finished-suggestion = Agent has a suggestion: { $where }
finished-noaction = Agent found nothing to do: { $where }
finished-failed = Agent could not answer: { $where }

# ---- the four quick buttons
button-state-sending = Sending
button-state-received = Sent
button-state-working = Working
button-state-suggested = Suggestion waiting
button-state-done = Done
button-state-failed = Failed
button-name = { $label }: sends your instruction to AzReview

# ---- files (screen C)
files-root = Workspace
files-empty = This folder is empty
files-loading = Loading
files-more = Load more
files-error = Could not load the folder: { $why }
files-retry = Try again
files-opened = Opened in AzReview
files-under-review = Under review
files-folder = Folder
files-file = File
files-state-clean = Nothing unresolved
files-state-unresolved = Unresolved suggestions
files-state-working = An agent is working
files-breadcrumb = Folders

# ---- the buttons editor (screen D)
editor-title = Quick buttons
editor-save = Save
editor-reset = Reset to defaults
editor-slot = Button { $n }
editor-label = Label
editor-text = Instruction
editor-count = { $n } / { $max }
editor-hint = A tap sends the instruction to AzReview with the context at the pen: the marked region, else the line under the pen, else the page. The four buttons are kept in AzReview's settings.
editor-capped = Cut to { $max } characters
editor-saved = Saved in AzReview
editor-label-name = Label of button { $n }
editor-text-name = Instruction of button { $n }

# ---- settings and about (screen E)
settings-title = Settings
settings-mac = Your Mac
settings-connected = Connected, { $ms } ms
settings-not-connected = Not connected
settings-quick-buttons = Quick buttons
settings-quick-buttons-hint = Hold a button on the remote to change it
settings-forget = Forget this Mac
settings-forget-ask = Forget { $mac }? You will need to pair again.
settings-forget-yes = Forget
settings-forget-no = Keep
settings-about = About
settings-version = Version { $v }
settings-protocol = Protocol { $p }
settings-licence = MIT licence. Part of AzReview.
settings-no-mac = No Mac is paired
