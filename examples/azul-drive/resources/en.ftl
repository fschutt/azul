# AzDrive's words. Every key the source names (`azdrive-...`) is here and in de.ftl
# (src/l10n_tests.rs). azcloud-kit's error table (`azlin-error-...`) and appkit's words
# (`kit-...`) are registered with these.

## The ribbon's tabs
azdrive-tab-home = Home

## The message bar over the content
azdrive-message-dismiss = Dismiss
azdrive-list-failed = Could not list this folder:
azdrive-transfer-failed = { $count ->
    [one] One item failed
   *[other] { $count } items failed
    }; "{ $name }":

## The ribbon and the File menu
azdrive-file-about = About AzDrive
azdrive-file-about-about = The version, the license, the credits.
azdrive-file-azterm = AzTerm
azdrive-file-azterm-about = Azlin's terminal, in the open folder.
azdrive-file-back-forward = Back and forward
azdrive-file-back-forward-about = Forget where Back and Forward go.
azdrive-file-delete-history = Delete history
azdrive-file-everything = Everything
azdrive-file-everything-about = Both.
azdrive-file-frequent-places = Frequent places
azdrive-file-help = Help
azdrive-file-new-window = Open new window
azdrive-file-new-window-about = Another AzDrive window, at this place.
azdrive-file-recent-places = Recent places
azdrive-file-shortcuts = Keyboard shortcuts
azdrive-file-shortcuts-about = Every key AzDrive knows (F1).
azdrive-file-terminal = Terminal
azdrive-file-terminal-here = Open terminal here
azdrive-file-terminal-system = The system's terminal, in the open folder.
azdrive-group-clipboard = Clipboard
azdrive-group-cloud = Cloud
azdrive-group-current-view = Current view
azdrive-group-layout = Layout
azdrive-group-location = Location
azdrive-group-network = Network
azdrive-group-new = New
azdrive-group-organize = Organize
azdrive-group-panes = Panes
azdrive-group-refine = Refine
azdrive-group-saved = Saved
azdrive-group-select = Select
azdrive-group-send = Send
azdrive-group-share-with = Share with
azdrive-group-show-hide = Show/hide
azdrive-group-sync = Sync
azdrive-group-system = System
azdrive-ribbon-add-columns = Add columns
azdrive-ribbon-add-drive = Add drive
azdrive-ribbon-add-folder-drive = Add folder as drive
azdrive-ribbon-advanced-security = Advanced security
azdrive-ribbon-all-subfolders = All subfolders
azdrive-ribbon-burn = Burn to disc
azdrive-ribbon-burn-why = AzDrive cannot write discs: this computer has no burner it can use.
azdrive-ribbon-close = Close
azdrive-ribbon-close-search = Close search
azdrive-ribbon-copy = Copy
azdrive-ribbon-copy-link = Copy link
azdrive-ribbon-copy-path = Copy path
azdrive-ribbon-copy-to = Copy to
azdrive-ribbon-current-folder = Current folder
azdrive-ribbon-cut = Cut
azdrive-ribbon-date-modified = Date modified
azdrive-ribbon-delete = Delete
azdrive-ribbon-details-pane = Details pane
azdrive-ribbon-download = Download
azdrive-ribbon-easy-access = Easy access
azdrive-ribbon-edit = Edit
azdrive-ribbon-email = Email
azdrive-ribbon-extensions = File name extensions
azdrive-ribbon-fax = Fax
azdrive-ribbon-fax-why = No fax device is set up.
azdrive-ribbon-file-contents = File contents
azdrive-ribbon-fit-columns = Size all columns to fit
azdrive-ribbon-free-up-space = Free up space
azdrive-ribbon-group-by = Group by
azdrive-ribbon-hidden-items = Hidden items
azdrive-ribbon-hide-selected = Hide selected items
azdrive-ribbon-history = History
azdrive-ribbon-index-cloud = Index files in the cloud
azdrive-ribbon-index-drive = Index this drive
azdrive-ribbon-invert-selection = Invert selection
azdrive-ribbon-item-checkboxes = Item check boxes
azdrive-ribbon-keep-on-device = Always keep on this device
azdrive-ribbon-kind = Kind
azdrive-ribbon-move-to = Move to
azdrive-ribbon-navigation-pane = Navigation pane
azdrive-ribbon-new-folder = New folder
azdrive-ribbon-new-item = New item
azdrive-ribbon-open = Open
azdrive-ribbon-open-file-location = Open file location
azdrive-ribbon-options = Options
azdrive-ribbon-paste = Paste
azdrive-ribbon-paste-shortcut = Paste shortcut
azdrive-ribbon-paste-shortcut-why = A shortcut is a Windows link file; a drive here holds files and folders only.
azdrive-ribbon-pause-sync = Pause syncing
azdrive-ribbon-preview-pane = Preview pane
azdrive-ribbon-print = Print
azdrive-ribbon-properties = Properties
azdrive-ribbon-refresh = Refresh
azdrive-ribbon-remove-drive = Remove drive
azdrive-ribbon-rename = Rename
azdrive-ribbon-resume-sync = Resume syncing
azdrive-ribbon-save-search = Save search
azdrive-ribbon-saved-searches = Saved searches
azdrive-ribbon-select-all = Select all
azdrive-ribbon-select-none = Select none
azdrive-ribbon-size = Size
azdrive-ribbon-skip-ignored = Skip ignored files
azdrive-ribbon-sort-by = Sort by
azdrive-ribbon-sync-now = Sync now
azdrive-ribbon-sync-with-folder = Sync with a folder
azdrive-ribbon-unhide-selected = Unhide selected items
azdrive-ribbon-upload = Upload
azdrive-ribbon-zip = Zip
azdrive-tab-computer = Computer
azdrive-tab-file = File
azdrive-tab-search = Search
azdrive-tab-share = Share
azdrive-tab-view = View
azdrive-undo = Undo
azdrive-ribbon-history-why = Earlier versions need a versioned bucket; AzDrive keeps one version of a file.
azdrive-ribbon-advanced-security-why = Who may read a cloud drive is set in its bucket policy; a local folder's in the system's sharing settings.
azdrive-file-recent-places-about = Forget the places Recent locations and Frequent places remember (the pins stay).
azdrive-refine-chosen = { $name }: { $chosen }

## Commands, menus and their messages

azdrive-why-read-only = This drive is browsed, not written (a database, a web server).
azdrive-why-open-folder = Open a folder of a drive first.
azdrive-why-open-and-select = Open a folder and select items first.
azdrive-why-select = Select one or more items first.
azdrive-why-pin-folder = Open a folder to pin it to Quick access.
azdrive-why-nothing-to-paste = Nothing to paste: copy or cut something first.
azdrive-why-rename-one = Select exactly one item to rename.
azdrive-why-open-result = Select a result to open.
azdrive-why-open-drive = Select a drive to open.
azdrive-why-open-pin = Select a pinned folder to open.
azdrive-why-open-item = Select an item to open.
azdrive-why-edit-folder = Edit opens a file; this is a folder.
azdrive-why-edit-one = Select one file to edit.
azdrive-why-built-in-drive = Home and the Azlin data folder stay.
azdrive-why-nothing-to-undo = Nothing to undo.
azdrive-why-no-azterm = AzTerm is not installed next to AzDrive.
azdrive-why-terminal-cloud = A terminal opens in a folder of this computer; this folder is in an S3 bucket.
azdrive-why-terminal-local = Open a folder of a drive on this computer first.
azdrive-why-print-cloud = Printing goes through this computer: download the file first.
azdrive-why-print-folder = Select files to print; a folder does not print.
azdrive-why-search-contents-cloud = A cloud drive's files are searched through its index: turn on Index this drive first.
azdrive-why-index-local = The files of a drive on this computer are read where they are.
azdrive-why-no-search = No search is open.
azdrive-why-search-first = Search first: this opens the folder a result is in.
azdrive-why-one-result = Select one result.
azdrive-why-save-search = Search first: Save search keeps the search box's text and its choices.
azdrive-why-no-saved-searches = No saved searches yet: Save search keeps the open one.
azdrive-why-not-saved = The open search is not a saved one.
azdrive-why-index-open = Open a drive to index it.
azdrive-why-no-cache = There is no cache folder to keep an index in.
azdrive-why-select-drive = Select a drive on This PC first.
azdrive-menu-pin = Pin to Quick access
azdrive-menu-add-local-drive = Add a folder as a drive…
azdrive-menu-drive-properties = Drive properties
azdrive-menu-forget-saved-search = Forget this saved search
azdrive-menu-choose-location = Choose location…
azdrive-menu-recycle = Recycle (to the trash folder)
azdrive-menu-delete-permanently = Permanently delete
azdrive-menu-confirm-delete = Show delete confirmation
azdrive-menu-new-folder = Folder
azdrive-menu-new-text = Text Document
azdrive-menu-new-empty = Empty file
azdrive-menu-open = Open
azdrive-menu-download = Download
azdrive-menu-properties = Properties
azdrive-menu-ascending = Ascending
azdrive-menu-descending = Descending
azdrive-menu-open-file-location = Open file location
azdrive-menu-keep-on-device = Always keep on this device
azdrive-menu-free-up-space = Free up space
azdrive-menu-cut = Cut
azdrive-menu-copy = Copy
azdrive-menu-zip = Compress to zip
azdrive-menu-copy-path = Copy path
azdrive-menu-delete = Delete
azdrive-menu-rename = Rename
azdrive-menu-view = View
azdrive-menu-sort-by = Sort by
azdrive-menu-group-by = Group by
azdrive-menu-refresh = Refresh
azdrive-menu-sync-now = Sync now
azdrive-menu-paste = Paste
azdrive-menu-undo = Undo
azdrive-menu-new = New
azdrive-pick-move-to = Move the items to
azdrive-pick-copy-to = Copy the items to
azdrive-pick-upload = Upload files
azdrive-pick-local-drive = Add a folder as a drive
azdrive-new-folder-name = New folder
azdrive-new-text-document-name = New Text Document.txt
azdrive-new-file-name = New file
azdrive-no-recent-places = No places visited yet.
azdrive-history-cleared-all = The places visited and the Back and Forward history are gone.
azdrive-history-cleared-recent = The places visited are gone (the pins stay).
azdrive-history-cleared-back-forward = Back and Forward start from here.
azdrive-search-saved = Saved the search "{ $name }": Saved searches runs it again.
azdrive-saved-search-drive-gone = The drive the saved search "{ $name }" searched is not in the list any more.
azdrive-new-window-no-program = AzDrive cannot find its own program to open another window.
azdrive-new-window-opens = Another window opens at "{ $path }".
azdrive-new-window-failed = The new window could not be opened: { $detail }
azdrive-terminal-opens = A terminal opens in { $folder }.
azdrive-terminal-failed = The terminal could not be opened: { $detail }
azdrive-printed = Sent { $count ->
        [one] one file
       *[other] { $count } files
    } to the printer.
azdrive-print-failed = Nothing could be printed: { $detail }
azdrive-hid-items = Hid { $count ->
        [one] one item
       *[other] { $count } items
    } (View > Hidden items shows them).
azdrive-opening = Opening "{ $name }"…
azdrive-clipboard-cut = Cut { $count ->
        [one] one item
       *[other] { $count } items
    }: Ctrl+V pastes them into the open folder.
azdrive-clipboard-copied = Copied { $count ->
        [one] one item
       *[other] { $count } items
    }: Ctrl+V pastes them into the open folder.
azdrive-copied-paths = { $count ->
        [one] Copied the path of one item.
       *[other] Copied the paths of { $count } items.
    }
azdrive-transfer-label = { $kind ->
        [move] Moving { $count ->
            [one] "{ $name }"
           *[other] { $count } items
        } to { $target }
        [upload] Uploading { $count ->
            [one] "{ $name }"
           *[other] { $count } items
        } to { $target }
        [download] Downloading { $count ->
            [one] "{ $name }"
           *[other] { $count } items
        } to { $target }
       *[copy] Copying { $count ->
            [one] "{ $name }"
           *[other] { $count } items
        } to { $target }
    }
azdrive-transfer-drive-gone = The items' drive is gone.
azdrive-transfer-lost = the transfer was lost
azdrive-transfer-cannot-start = The transfer cannot start:
azdrive-transfer-nothing-to-do = Nothing to do: the items are where they would go.
azdrive-transfer-cancelled = The transfer was cancelled.
azdrive-transfer-cancelled-after = Cancelled after { $count ->
        [one] one item
       *[other] { $count } items
    }.
azdrive-transfer-done-skipped = Done: { $count ->
        [one] one item
       *[other] { $count } items
    }, { $skipped } skipped.
azdrive-transfer-done = Done: { $count ->
        [one] one item
       *[other] { $count } items
    }.
azdrive-downloaded = { $count ->
        [one] Downloaded one item
       *[other] Downloaded { $count } items
    } to { $folder }.
azdrive-choose-a-folder = Choose a folder of a drive.
azdrive-drop-on-folder = Drop the items on a folder of a drive.
azdrive-upload-open-folder = Open a folder of a drive to upload into.
azdrive-cannot-upload = "{ $name }" cannot be uploaded:
azdrive-moving-to-trash = { $count ->
        [one] Moving one item
       *[other] Moving { $count } items
    } to the trash folder…
azdrive-deleting = { $count ->
        [one] Deleting one item…
       *[other] Deleting { $count } items…
    }
azdrive-name-taken = There is already an item named "{ $name }" in this folder.
azdrive-properties-select-drive = Select a drive to see its properties.
azdrive-properties-open-pin = Open a pinned folder to see its properties.
azdrive-link-local = Copied the items' paths (a link to share is a cloud drive's).
azdrive-link-no-keys = The drive's keys are not read yet: open one of its folders first.
azdrive-link-failed = No link can be made:
azdrive-link-failed-for = No link to "{ $name }":
azdrive-link-folder = A folder has no download link: copied its s3:// address.
azdrive-links-copied = { $count ->
        [one] Copied a link: anyone holding it can download the file for 7 days.
       *[other] Copied { $count } links: anyone holding one can download its file for 7 days.
    }
azdrive-email-opened = A new message with the items' addresses is open.
azdrive-email-no-app = The system has no mail app to open.
azdrive-zip-too-big = The selection is too big to compress in memory (more than 256 MB).
azdrive-compressing = { $count ->
        [one] Compressing one item…
       *[other] Compressing { $count } items…
    }
azdrive-unpinned = "{ $name }" left Quick access.
azdrive-pinned = "{ $name }" is pinned to Quick access.
azdrive-drive-not-saved = The drive could not be saved: { $detail }
azdrive-is-a-drive = "{ $name }" is a drive now.
azdrive-pick-not-a-drive = This folder is on none of AzDrive's drives: add it as a drive first (This PC > Computer > Add folder as drive), or type a drive's folder.
azdrive-drives-file-failed = The drives file could not be updated: { $detail }
azdrive-drive-removed = "{ $name }" was removed from AzDrive. Its files stay where they are.
azdrive-pick-not-a-folder = "{ $path }" is not a folder of a drive.
azdrive-preview-folder = A folder: open it to see what it holds.
azdrive-queue-waiting = { $count } waiting
azdrive-queue-progress = { $done } of { $total } ({ $percent }%)
azdrive-undo-rename = Undo rename of "{ $name }"
azdrive-undo-delete = Undo delete of { $count ->
        [one] one item
       *[other] { $count } items
    }
azdrive-undo-move = Undo move of { $count ->
        [one] one item
       *[other] { $count } items
    }
azdrive-undo-new = Undo new "{ $name }"
azdrive-preview-binary = No preview: a binary file.
azdrive-preview-not-wav = No preview: not a WAV file.
azdrive-preview-wav-broken = No preview: a broken format chunk.
azdrive-preview-wav-no-format = No preview: no format chunk before the samples.
azdrive-preview-wav-no-channels = No preview: no channels or no sample rate.
azdrive-preview-wav-compressed = No preview: its samples are compressed, not PCM or float.
azdrive-preview-wav-no-samples = No preview: no samples in the file.
azdrive-preview-audio-format = No preview: azul plays raw samples and has no decoder for this audio format (a WAV file plays).
azdrive-preview-none = No preview available.
azdrive-preview-failed = No preview:
azdrive-preview-pdf-unreadable = No preview: azul could not read this PDF.
azdrive-preview-pdf-not-drawn = No preview: the PDF's first page could not be drawn.
azdrive-preview-wav-too-big = No preview: the WAV file is too big to fetch for a preview.
azdrive-preview-video-too-big = No preview: the video is too big to fetch for a preview; open it instead.
azdrive-preview-too-big = No preview: the file is too big to fetch for a preview.
azdrive-preview-image-not-prepared = No preview: the image could not be prepared.
azdrive-preview-image-undecodable = No preview: azul cannot decode this image.
azdrive-preview-cloud-only = In the cloud only: open it to download it, or keep it on this device (Share > Sync).
azdrive-preview-synced = Synced: its copy is in the synced folder - open it, or preview it there.

## The sync's commands

azdrive-sync-why-open-cloud = Open a cloud drive to sync it with a folder.
azdrive-sync-why-local = This drive is on this computer already: open a cloud drive to sync it.
azdrive-sync-why-synced = This drive syncs with a folder already (Options > Drives).
azdrive-sync-why-no-cache = There is no cache folder to keep the sync's state in.
azdrive-sync-why-open-synced = Open a synced folder or drive first (Share > Sync with a folder).
azdrive-sync-why-select = Select files or folders of a synced folder first.
azdrive-sync-why-only-synced = Only the files of a synced folder are kept on this device or freed.
azdrive-sync-menu-pair = Sync with a folder on this computer…
azdrive-sync-menu-open-folder = Open the synced folder
azdrive-sync-menu-now = Sync now
azdrive-sync-menu-resume = Resume syncing
azdrive-sync-menu-pause = Pause syncing
azdrive-sync-menu-stop = Stop syncing…

## Dialogs and Options

azdrive-delete-title = Delete for good
azdrive-delete-question = Are you sure you want to delete { $count ->
        [one] "{ $name }"
       *[other] these { $count } items
    } from "{ $drive }" for good?
azdrive-delete-cannot-undo = This cannot be undone.
azdrive-delete-button = Delete
azdrive-forget-title = Remove the drive "{ $name }"?
azdrive-forget-what = AzDrive forgets the drive and removes its keys from the keyring.
azdrive-forget-files-stay = Its files stay where they are.
azdrive-forget-button = Remove
azdrive-location-folder = The folder (a drive's name, then its folders: Home/docs)
azdrive-location-move-here = Move here
azdrive-location-copy-here = Copy here
azdrive-location-move-title = Move the selected items to
azdrive-location-copy-title = Copy the selected items to
azdrive-voucher-what = A voucher adds its months (or its value) to "{ $name }"'s paid period.
azdrive-voucher-code = The voucher's code
azdrive-voucher-redeeming = Redeeming the voucher…
azdrive-voucher-redeem = Redeem
azdrive-voucher-title = Redeem a voucher for "{ $name }"
azdrive-restore-what = "{ $name }" goes back to how it was at that time: files changed or deleted since come back, files made since go. AzDrive can restore the last { $days } days.
azdrive-restore-when = When ("2 hours ago", or a UTC time: 2026-10-10 08:00)
azdrive-restore-default-as-of = 1 hour ago
azdrive-restore-restoring = Restoring the drive…
azdrive-restore-button = Restore
azdrive-restore-title = Restore "{ $name }" as of…
azdrive-conflict-title = Replace or Skip Files
azdrive-button-close = Close
azdrive-conflict-transfer = { $kind ->
        [move] Moving
        [upload] Uploading
        [download] Downloading
       *[copy] Copying
    } { $count ->
        [one] one item
       *[other] { $count } items
    } to "{ $target }"
azdrive-conflict-taken = The destination already has a file named "{ $name }".
azdrive-conflict-new-one = The new one: { $size }
azdrive-conflict-replace = Replace the file in the destination
azdrive-conflict-skip = Skip this file
azdrive-conflict-keep-both = Keep both files
azdrive-conflict-apply-all = { $count ->
        [one] Do this for the next conflict
       *[other] Do this for the next { $count } conflicts
    }
azdrive-props-general = General
azdrive-props-details = Details
azdrive-props-type = Type
azdrive-props-location = Location
azdrive-props-used = Used space
azdrive-props-free = Free space
azdrive-props-capacity = Capacity
azdrive-props-bucket = Bucket
azdrive-props-endpoint = Endpoint
azdrive-props-region = Region
azdrive-props-url-style = URL style
azdrive-props-keys = Keys
azdrive-props-passwords = Passwords and tokens
azdrive-props-drive-id = Drive id
azdrive-props-size = Size
azdrive-props-contains = Contains
azdrive-props-modified = Modified
azdrive-props-attributes = Attributes
azdrive-props-metadata = Metadata
azdrive-props-size-of-files = Size of the files
azdrive-props-name = Name
azdrive-props-key = Key
azdrive-props-path-style = path
azdrive-props-virtual-host = virtual host
azdrive-props-in-keyring = in the system keyring (never on disk)
azdrive-props-counting = Counting…
azdrive-props-size-bytes = { $size } ({ $bytes } bytes)
azdrive-props-files-folders = { $files ->
        [one] one file
       *[other] { $files } files
    }, { $folders ->
        [one] one folder
       *[other] { $folders } folders
    }
azdrive-props-hidden = Hidden
azdrive-props-reading = Reading…
azdrive-props-items = { $count ->
        [one] one item
       *[other] { $count } items
    }
azdrive-props-title = { $name } Properties
azdrive-transfers-none = No transfers.
azdrive-transfers-items = { $done } of { $total ->
        [one] one item
       *[other] { $total } items
    }
azdrive-transfers-bytes = { $done } of { $total }
azdrive-transfers-waiting = waiting
azdrive-transfers-done = done
azdrive-transfers-failed = failed:
azdrive-transfers-cancelled = cancelled
azdrive-transfers-clear = Clear finished
azdrive-transfers-title = Transfers
azdrive-backstage-options = Options
azdrive-backstage-about = About
azdrive-category-view = View
azdrive-category-navigation = Navigation
azdrive-category-drives = Drives
azdrive-options-layout = Layout of the folders
azdrive-options-show = Show
azdrive-options-hidden-items = Hidden items
azdrive-options-extensions = File name extensions
azdrive-options-item-checkboxes = Item check boxes
azdrive-options-deleting = Deleting
azdrive-options-confirm-delete = Ask before deleting for good
azdrive-options-delete-note = Delete on a local drive moves the items into its .azdrive-trash folder (Ctrl+Z brings them back); a cloud drive always asks.
azdrive-options-open-in = Open AzDrive in
azdrive-this-pc = This PC
azdrive-quick-access = Quick access
azdrive-options-panes = Panes
azdrive-options-navigation-pane = Navigation pane
azdrive-options-preview-pane = Preview pane
azdrive-options-details-pane = Details pane
azdrive-options-s3-at = s3://{ $bucket } at { $endpoint }
azdrive-options-redeem-voucher = Redeem a voucher
azdrive-options-restore = Restore as of…
azdrive-options-no-drives-file = (none)
azdrive-options-drives = Drives
azdrive-options-sync = Sync
azdrive-options-other-programs = Use with other programs
azdrive-options-add-drive = Add a drive
azdrive-options-add-drive-button = Add drive…
azdrive-options-keys-note = Access keys, passwords and tokens live in the system keyring only; the list of drives (without them) is { $file }.
azdrive-about-summary = A file manager like Windows Explorer for the Azlin data tree, the folders of this computer, S3 buckets, Azlin cloud storage and the data sources OpenDAL reaches (WebDAV, FTP, Google Drive, Dropbox, OneDrive, GitHub, ...), with databases browsed as tables.
azdrive-no-token-server = The drive's token server is not known.
azdrive-voucher-type-code = Type the voucher's code.
azdrive-voucher-added = The voucher added { $days ->
        [one] one day
       *[other] { $days } days
    } to "{ $name }".
azdrive-voucher-added-until = The voucher added { $days ->
        [one] one day
       *[other] { $days } days
    } to "{ $name }": it is paid until { $until }.
azdrive-voucher-failed = The voucher could not be redeemed:
azdrive-restore-type-time = Type the time to restore the drive to: type "2 hours ago" or a UTC time like 2026-10-10 08:00.
azdrive-restore-unknown-time = "{ $text }" is no time AzDrive knows: type "2 hours ago" or a UTC time like 2026-10-10 08:00.
azdrive-restore-in-future = That time is still to come.
azdrive-restore-too-old = AzDrive can restore the last { $days } days (what the drive keeps).
azdrive-restored-files = "{ $drive }" is as it was at { $at }: { $count ->
        [one] one file came
       *[other] { $count } files came
    } back or went. The drive as it was before the restore stays in its history.
azdrive-restored-objects = "{ $drive }" is as it was at { $at }: { $count ->
        [one] one object came
       *[other] { $count } objects came
    } back or went.
azdrive-restore-queued = The restore of "{ $drive }" as of { $at } is queued at the token server ({ $request }): the drive's node does it later.
azdrive-restore-open-first = Open the drive first: its session is read from the keyring.
azdrive-restore-out-of-range = The time is out of range.
azdrive-restore-node-failed = The drive's node did not restore it:
azdrive-restore-failed = The drive could not be restored:
azdrive-voucher-made-new-drive = The token server made a new drive instead of extending this one.

## About

azdrive-about-credits = Credits
azdrive-about-public-domain = Public domain

## The sync

azdrive-sync-status-paused = Paused
azdrive-sync-status-payment-due = Read-only (payment due)
azdrive-sync-status-newer-format = Read-only here: update the app to sync this drive
azdrive-sync-status-burst = Uploads paused: { $count ->
        [one] one change
       *[other] { $count } changes
    } at once
azdrive-sync-status-encrypted = Uploads paused: { $count ->
        [one] one file looks
       *[other] { $count } files look
    } encrypted
azdrive-sync-status-mass-delete = Waiting for you: { $count ->
        [one] one file
       *[other] { $count } files
    } would be deleted
azdrive-sync-status-read-only = Read-only
azdrive-sync-status-syncing = Syncing…
azdrive-sync-status-syncing-files = Syncing { $count ->
        [one] one file
       *[other] { $count } files
    } ({ $size })
azdrive-sync-status-conflicts = Waiting for you: { $count ->
        [one] one conflict
       *[other] { $count } conflicts
    }
azdrive-sync-status-failed = Not synced: { $error }
azdrive-sync-status-never = Not synced yet
azdrive-sync-status-up-to-date = Up to date
azdrive-sync-state-cloud-only = Cloud only: downloaded when opened
azdrive-sync-state-downloading = Downloading
azdrive-sync-state-uploading = Uploading
azdrive-sync-state-on-device = On this device
azdrive-sync-state-on-device-encrypted = On this device (encrypted): decrypted when opened
azdrive-sync-state-pinned = Always kept on this device
azdrive-sync-state-conflict = Changed here and on the drive
azdrive-sync-state-error = Not synced: { $error }
azdrive-sync-in-the-cloud = In the cloud: fetched when it is opened
azdrive-index-overlay-indexed = In the search index
azdrive-index-overlay-not-indexable = Not indexable: no text, or too big
azdrive-sync-mass-delete-there = The drive says { $count } of the { $of } files of this folder were deleted on another device. That may be a mistake, or ransomware: nothing was deleted here yet.
azdrive-sync-mass-delete-here = { $count } of the { $of } files this folder held are gone from it (was a disk removed, or the folder emptied?). Nothing was deleted on the drive yet.
azdrive-sync-pair-what = The files of the folder on this computer and of the drive's folder are kept the same, both ways. New files under 25 MB come down by themselves; bigger ones stay in the cloud until you open them (Options > Drives).
azdrive-sync-pair-folder = Folder on this computer
azdrive-sync-pair-prefix = Folder of the drive (empty: the whole drive)
azdrive-sync-pair-ok = Sync
azdrive-sync-pair-title = Sync "{ $name }" with a folder
azdrive-sync-conflict-what = "{ $name }" was changed on this computer and on the drive since they were last the same.
azdrive-sync-another-device = another device
azdrive-sync-conflict-theirs = The drive's version: { $size }, from { $device }.
azdrive-sync-keep-mine = Keep mine: the drive gets this computer's version
azdrive-sync-take-theirs = Take theirs: this computer gets the drive's version
azdrive-sync-keep-both = Keep both: the drive's keeps the name, this computer's becomes a copy
azdrive-sync-decide-later = Decide later
azdrive-sync-conflict-title = Someone changed this file
azdrive-sync-delete-title = Delete from the drive
azdrive-sync-delete-what = Delete { $count ->
        [one] "{ $one }"
       *[other] these { $count } items
    } from "{ $drive }"? The next sync deletes them on the drive, here and on your other devices.
azdrive-sync-burst-changed = { $count ->
        [one] One file was
       *[other] { $count } files were
    } changed or deleted in a few minutes.
azdrive-sync-burst-encrypted = { $count ->
        [one] One file turned
       *[other] { $count } files turned
    } into what looks like encrypted data.
azdrive-sync-burst-stopped = AzDrive stopped sending changes of "{ $name }" to the drive - what the drive changes still comes here.
azdrive-sync-burst-changes = The changes
azdrive-sync-i-was-hacked-button = I was hacked…
azdrive-sync-changes-are-mine = These changes are mine
azdrive-sync-burst-title = Many files changed at once
azdrive-sync-hacked-what = Lock the drive down (every other computer, key and link loses access) and put its files back as they were before the changes.
azdrive-sync-lock-down = Lock it down…
azdrive-sync-hacked-not-azlin = Only an Azlin drive can be locked down and restored by AzDrive: do it at the storage service's console.
azdrive-sync-hacked-title = I was hacked
azdrive-sync-mass-files = The files
azdrive-sync-mass-keep = Keep them
azdrive-sync-mass-delete-here-too = Delete them here too
azdrive-sync-mass-delete-there-too = Delete them on the drive too
azdrive-sync-mass-title = Delete most of the files?
azdrive-sync-stop-title = Stop syncing "{ $name }"?
azdrive-sync-stop-what = The files stay where they are: on the drive, and in { $folder }. Changes no longer travel between them.
azdrive-sync-stop-button = Stop syncing
azdrive-sync-auto-everything = Everything
azdrive-sync-auto-new-under = New files under { $mb } MB
azdrive-sync-auto-pinned = Only pinned folders
azdrive-sync-auto-nothing = Nothing (on demand)
azdrive-sync-auto-download = Download by themselves
azdrive-sync-under-mb = New files under (MB)
azdrive-sync-keep-gb = Keep local copies at most (GB; empty: no limit)
azdrive-sync-keep-gb-note = The least recently used files are freed first; files kept on this device (pinned) never are.
azdrive-sync-local-copies = Local copies
azdrive-sync-copies-decrypted = Decrypted (fast to open and search)
azdrive-sync-copies-encrypted = Encrypted, decrypted when opened
azdrive-sync-resume = Resume
azdrive-sync-pause = Pause
azdrive-sync-none = No drive syncs with a folder yet: a cloud drive's menu in the source list (or Share > Sync with a folder) pairs it with one.
azdrive-sync-freed = Freed { $count ->
        [one] one file
       *[other] { $count } files
    } on this computer; the drive keeps them.
azdrive-sync-freed-kept = "{ $name }" stays: { $why }.
azdrive-sync-pinned = { $count ->
        [one] One item is
       *[other] { $count } items are
    } always kept on this computer.
azdrive-sync-unpinned = { $count ->
        [one] One item is
       *[other] { $count } items are
    } no longer always kept on this computer.
azdrive-sync-deleted = { $count ->
        [one] One file is
       *[other] { $count } files are
    } deleted; the next sync deletes them on the drive.
azdrive-sync-fetched = { $count ->
        [one] One file downloaded.
       *[other] { $count } files downloaded.
    }
azdrive-sync-pair-type-folder = Type the folder on this computer.
azdrive-sync-pair-whole-path = Type the whole path of the folder (it starts at the top of the disk).
azdrive-sync-pair-synced = This drive syncs with a folder already.
azdrive-sync-pair-taken = { $folder } syncs with another drive already; pick a folder outside it.
azdrive-sync-pair-not-shown = The folder must lie in Home or in a folder added as a drive, so AzDrive can show it.
azdrive-sync-stopped = The drive no longer syncs; its files stay where they are.
azdrive-sync-downloading-first = { $count ->
        [one] Downloading one file from the drive first…
       *[other] Downloading { $count } files from the drive first…
    }
azdrive-sync-folder-not-shown = The synced folder is not in a drive this window shows.
azdrive-sync-open-failed = "{ $name }" could not be opened:
azdrive-sync-pairing-whole = { $folder } with the whole drive
azdrive-sync-pairing-folder = { $folder } with its folder { $prefix }

## Keyboard shortcuts

azdrive-shortcut-group-open = Open and go
azdrive-shortcut-open-selected = Open the selected item
azdrive-shortcut-properties = Properties
azdrive-shortcut-up = Up one level
azdrive-shortcut-back = Back
azdrive-shortcut-forward = Forward
azdrive-shortcut-refresh = Refresh
azdrive-shortcut-search = Search this folder
azdrive-shortcut-group-organize = Organize
azdrive-shortcut-rename = Rename in place
azdrive-shortcut-delete = Delete (a local drive keeps it in its trash folder)
azdrive-shortcut-delete-for-good = Delete for good
azdrive-shortcut-copy = Copy
azdrive-shortcut-cut = Cut
azdrive-shortcut-paste = Paste
azdrive-shortcut-undo = Undo
azdrive-shortcut-new-folder = New folder
azdrive-shortcut-group-select = Select
azdrive-shortcut-select-all = Select all
azdrive-shortcut-toggle-focused = Select or clear the focused item
azdrive-shortcut-extend = Extend the selection
azdrive-shortcut-select-none = Select nothing
azdrive-shortcut-context-menu = The context menu
azdrive-shortcut-group-view = View
azdrive-shortcut-large-icons = Large icons
azdrive-shortcut-list = List
azdrive-shortcut-details = Details
azdrive-shortcut-preview-pane = Preview pane
azdrive-shortcut-details-pane = Details pane

## Search, status line, preview and details panes

azdrive-refine-date-any = Any date
azdrive-refine-date-today = Today
azdrive-refine-date-yesterday = Yesterday
azdrive-refine-date-this-week = This week
azdrive-refine-date-last-week = Last week
azdrive-refine-date-this-month = This month
azdrive-refine-date-last-month = Last month
azdrive-refine-date-this-year = This year
azdrive-refine-date-last-year = Last year
azdrive-refine-kind-any = Any kind
azdrive-refine-kind-document = Document
azdrive-refine-kind-picture = Picture
azdrive-refine-kind-music = Music
azdrive-refine-kind-video = Video
azdrive-refine-kind-archive = Archive
azdrive-refine-kind-code = Code
azdrive-refine-kind-mail = E-mail
azdrive-refine-size-any = Any size
azdrive-refine-size-empty = Empty (0 KB)
azdrive-refine-size-tiny = Tiny (0 - 16 KB)
azdrive-refine-size-small = Small (16 KB - 1 MB)
azdrive-refine-size-medium = Medium (1 - 128 MB)
azdrive-refine-size-large = Large (128 MB - 1 GB)
azdrive-refine-size-huge = Huge (1 - 4 GB)
azdrive-refine-size-gigantic = Gigantic (> 4 GB)
azdrive-refine-part-date = Date modified:
azdrive-refine-part-kind = Kind:
azdrive-refine-part-size = Size:
azdrive-find-status-contents = Searching file contents… { $n } found
azdrive-find-status-drive-names = Searching the drive's names… { $n } found
azdrive-find-status-cached = Searching the last listing, then the cloud… { $n } found
azdrive-find-status-cloud = Searching names in the cloud (slower)… { $n } found
azdrive-find-status-searching = Searching… { $n } found
azdrive-find-status-stopped = The search stopped: { $error }
azdrive-find-none = No items match your search.
azdrive-find-status-found = { $count ->
        [one] { $n } item found
       *[other] { $n } items found
    }
azdrive-find-status-found-first = { $count ->
        [one] { $n } item found (the first ones)
       *[other] { $n } items found (the first ones)
    }
azdrive-find-note-index-contents = The drive's names come from its index on this computer, file contents from its search index (Index this drive).
azdrive-find-note-index = The drive's names come from its index on this computer; Index this drive (the Search tab) searches its files' contents too.
azdrive-find-note-cloud-contents = A cloud drive is searched by name over a listing of every file below this folder (slower than a folder on this computer), file contents from its search index.
azdrive-find-note-cloud = A cloud drive is searched by name, over a listing of every file below this folder: slower than a folder on this computer, and file contents are not searched.
azdrive-find-empty-searching = Searching…
azdrive-find-empty-cloud = The search looked at the names in this folder and every folder below it; a cloud drive's files are not read.
azdrive-find-empty-contents = The search looked at the names and the contents of the files in this folder and every folder below it.
azdrive-find-empty-names = The search looked at the names in this folder and every folder below it; File contents (the Search tab) reads the files too.
azdrive-index-status-looking = Indexing: looking at the files…
azdrive-index-status-reading = Indexing: { $read } of { $total } { $count ->
        [one] file
       *[other] files
    } read…
azdrive-index-status-failed = The index could not be updated: { $error }
azdrive-index-status-indexed = Indexed: { $n } { $count ->
        [one] file
       *[other] files
    }
azdrive-index-status-never = Not indexed yet
azdrive-sync-lookup-online-only = Available when online
azdrive-sync-lookup-on-device = Available on this device
azdrive-sync-lookup-syncing = Syncing
azdrive-sync-lookup-problem = Sync problem
azdrive-find-column-name = Name
azdrive-find-column-folder = Folder
azdrive-find-column-match = Match
azdrive-find-column-modified = Date modified
azdrive-find-column-size = Size
azdrive-find-column-status = Status
azdrive-status-items = { $count ->
        [one] { $n } item
       *[other] { $n } items
    }
azdrive-status-items-so-far = { $count ->
        [one] { $n } item so far
       *[other] { $n } items so far
    }
azdrive-search-placeholder = Search { $place }
azdrive-no-subfolders = This folder has no subfolders.
azdrive-listing-folder = Listing the folder…
azdrive-no-drive-for = There is no drive for "{ $path }".
azdrive-path-bar = Path
azdrive-status-selected = { $n } selected
azdrive-status-drives = { $count ->
        [one] one drive
       *[other] { $count } drives
    }
azdrive-status-drive-selected = "{ $name }" selected, { $drives }
azdrive-status-pins = { $count ->
        [one] one pinned folder
       *[other] { $count } pinned folders
    }
azdrive-status-loading = Loading…
azdrive-status-selected-of = { $n } of { $shown } selected
azdrive-status-available = { $size } available
azdrive-status-clipboard = { $count } on the clipboard
azdrive-status-transfers-failed = { $count ->
        [one] one transfer failed
       *[other] { $count } transfers failed
    }
azdrive-audio-no-output = no audio output
azdrive-audio-cannot-play = The sound cannot play: { $why }
azdrive-preview-select = Select a file to preview.
azdrive-preview-loading = Loading the preview of "{ $name }"…
azdrive-preview-image-size = { $name } - { $width } x { $height } pixels
azdrive-preview-mono = mono
azdrive-preview-stereo = stereo
azdrive-preview-channels = { $count } channels
azdrive-preview-stop = Stop
azdrive-preview-play = Play
azdrive-preview-video-note = { $name } (H.264, without sound: azul's video widget has no audio track)
azdrive-details-recovery-health = Recovery health
azdrive-details-used = Space used
azdrive-details-free = Free space
azdrive-details-total = Total size
azdrive-details-path = Path
azdrive-details-file-folder = File folder
azdrive-details-items = Items
azdrive-details-selected = { $count ->
        [one] one item selected
       *[other] { $count } items selected
    }
azdrive-details-files = Files
azdrive-details-folders = Folders

## Places, columns, kinds, layouts and groups

azdrive-column-name = Name
azdrive-column-modified = Date modified
azdrive-column-type = Type
azdrive-column-size = Size
azdrive-column-path = Folder path
azdrive-column-tag = ETag
azdrive-kind-folder = File folder
azdrive-kind-file = File
azdrive-kind-other = { $ext } File
azdrive-kind-text = Text Document
azdrive-kind-markdown = Markdown File
azdrive-kind-pdf = PDF Document
azdrive-kind-jpeg = JPEG image
azdrive-kind-png = PNG image
azdrive-kind-gif = GIF image
azdrive-kind-bmp = BMP image
azdrive-kind-webp = WEBP image
azdrive-kind-svg = SVG Document
azdrive-kind-video = Video
azdrive-kind-audio = Audio
azdrive-kind-zip = Compressed (zipped) Folder
azdrive-kind-html = HTML Document
azdrive-kind-json = JSON File
azdrive-kind-csv = CSV File
azdrive-kind-email = E-mail Message
azdrive-kind-icalendar = iCalendar File
azdrive-kind-word = Word Document
azdrive-kind-excel = Excel Worksheet
azdrive-kind-powerpoint = PowerPoint Presentation
azdrive-layout-extra-large-icons = Extra large icons
azdrive-layout-large-icons = Large icons
azdrive-layout-medium-icons = Medium icons
azdrive-layout-small-icons = Small icons
azdrive-layout-list = List
azdrive-layout-details = Details
azdrive-layout-tiles = Tiles
azdrive-layout-content = Content
azdrive-group-by-none = (None)
azdrive-group-by-name = Name
azdrive-group-by-type = Type
azdrive-group-by-size = Size
azdrive-group-by-modified = Date modified
azdrive-group-other = Other
azdrive-group-folders = Folders
azdrive-group-size-unspecified = Unspecified
azdrive-group-date-unknown = Unknown date
azdrive-group-date-today = Today
azdrive-group-date-yesterday = Yesterday
azdrive-group-date-earlier-this-week = Earlier this week
azdrive-group-date-last-week = Last week
azdrive-group-date-earlier-this-month = Earlier this month
azdrive-group-date-last-month = Last month
azdrive-group-date-earlier-this-year = Earlier this year
azdrive-group-date-long-ago = A long time ago
azdrive-tile-at-root = { $kind }, { $count ->
        [one] one item
       *[other] { $count } items
    } at the root
azdrive-this-pc-devices = Devices and drives
azdrive-this-pc-network = Network locations
azdrive-this-pc-no-cloud = No cloud drive yet.
azdrive-this-pc-no-cloud-detail = Buy Azlin storage, or connect S3, WebDAV, Google Drive, GitHub or a database; keys and passwords stay in the system keyring.
azdrive-this-pc-add-drive = Add drive
azdrive-quick-access-none = Nothing is pinned yet.
azdrive-quick-access-none-detail = Open a folder and choose See more (...) > Pin to Quick access.
azdrive-quick-access-pinned = Pinned folders
azdrive-quick-access-recent = Recent places
azdrive-rename-field = New name
azdrive-content-items = { $count ->
        [one] one item
       *[other] { $count } items
    }
azdrive-content-size = Size: { $size }
azdrive-content-modified = Date modified: { $date }
azdrive-folder-unreadable = This folder could not be read.
azdrive-folder-unreadable-detail = The message above says why; Refresh (F5) tries again.
azdrive-folder-empty = This folder is empty.
azdrive-folder-empty-detail = Drop files here from your computer, or use New folder (or Upload) on the ribbon's Home tab.
azdrive-folder-filter-none-detail = The search looks at the names in this folder.

## The navigation pane

azdrive-side-favorites = Favorites
azdrive-side-locations = Locations
azdrive-side-cloud = Cloud
azdrive-standard-desktop = Desktop
azdrive-standard-documents = Documents
azdrive-standard-downloads = Downloads
azdrive-standard-pictures = Pictures
azdrive-standard-music = Music
azdrive-standard-videos = Videos
azdrive-standard-movies = Movies
azdrive-side-add-drive = Add drive…
azdrive-side-unpin = Remove from Favorites
azdrive-side-encrypt = Encrypt this drive…
azdrive-side-unlock = Unlock with the recovery code…
azdrive-side-rotate = I was hacked: new keys…
azdrive-side-lockdown = Lock down with the recovery code…
azdrive-side-contacts-recover = Recover with trusted contacts…
azdrive-side-remove-drive = Remove "{ $name }"…
azdrive-side-unpinned = "{ $name }" left Favorites.
azdrive-side-drop-folders = Drop folders on Favorites to pin them.
azdrive-side-pinned = { $count ->
        [one] One folder pinned
       *[other] { $count } folders pinned
    } to Favorites.
azdrive-side-unpin-place = Remove "{ $name }" from Favorites
azdrive-side-pin-place = Pin "{ $name }" to Favorites
azdrive-side-state-busy = Syncing
azdrive-side-state-locked = Its keys are in the keyring
azdrive-side-state-connected = Connected
azdrive-side-state-not-opened = Not opened yet
azdrive-side-collapse = Close
azdrive-side-expand = Open
azdrive-side-eject = Remove "{ $name }"
azdrive-side-activity-progress = { $done } of { $total ->
        [one] one file
       *[other] { $total } files
    } ({ $percent }%)
azdrive-side-add = Add a drive or pin a folder
azdrive-side-sources = Sources

## Keyring, results of the file operations

azdrive-keyring-reading = Reading the keys of "{ $name }" from the keyring…
azdrive-keyring-not-found = The keyring has no entry for it.
azdrive-keyring-denied = The keyring refused.
azdrive-keyring-unavailable = No keyring is available on this system.
azdrive-keyring-failed = The keyring reported an error.
azdrive-keyring-unreadable = The keys of "{ $name }" cannot be read: { $detail }.
azdrive-keyring-cannot-open = "{ $name }" cannot be opened:
azdrive-keyring-token-gone = Its drive token is gone; remove the drive and add it again.
azdrive-keyring-enter-keys = Enter its keys again.
azdrive-keyring-saved = "{ $name }" is saved; its keys are in the system keyring.
azdrive-keyring-not-saved = The keys of "{ $name }" could not be saved:
azdrive-keyring-kept-until-close = They are kept until AzDrive closes.
azdrive-keyring-bridge-unreadable = The bridge's password could not be read:
azdrive-keyring-bridge-new = "azul-bridge password" makes a new one.
azdrive-os-cannot-open = The system could not open { $path }.
azdrive-os-not-a-url = { $url } is not a URL: { $detail }
azdrive-list-folder-failed = Could not list the folder:
azdrive-deleted-for-good = { $count ->
        [one] Deleted one item for good.
       *[other] Deleted { $count } items for good.
    }
azdrive-trashed = { $count ->
        [one] Moved one item
       *[other] Moved { $count } items
    } to the trash folder. Ctrl+Z brings them back.
azdrive-delete-failed = Could not delete:
azdrive-rename-failed = "{ $name }" could not be renamed:
azdrive-create-failed = "{ $name }" could not be created:
azdrive-undone = Undone.
azdrive-undo-failed = Could not undo:
azdrive-zipped = Compressed into "{ $name }" ({ $size }).
azdrive-zip-failed = Could not compress:
azdrive-settings-not-saved = The settings could not be saved:
azdrive-index-not-removed = The index could not be removed: { $detail }
azdrive-drives-file-unreadable = The drives file could not be read: { $detail }
azdrive-name-empty = A name cannot be empty.
azdrive-name-reserved = "{ $name }" is reserved.
azdrive-name-forbidden-chars = A name cannot contain any of these characters: \ / : * ? " < > |
azdrive-name-control-chars = A name cannot contain control characters.
azdrive-copy-word = Copy

## Periods and lockdowns

azdrive-new-device = A new device was added to "{ $drive }" ({ $member }). Not you? Lock the drive down in AzDrive: the drive's menu, "I was hacked".
azdrive-recovery-used = The recovery code of "{ $drive }" was used to lock it down. In { $hours ->
        [0] less than an hour
        [one] an hour
       *[other] { $hours } hours
    } ({ $at }) that device takes the drive and every other device loses it. If that was not you, cancel it in AzDrive now.
azdrive-lockdown-pending = A lockdown with the recovery code is pending until { $until }: then every other device loses this drive. If that was not you, cancel it now.
azdrive-lockdown-cancel = Cancel lockdown
azdrive-lockdown-no-token-server = The drive's token server is not known: the lockdown cannot be cancelled here.
azdrive-lockdown-cancelled = The lockdown with the recovery code was cancelled. If you did not start it, someone has your recovery code: make a new one.
azdrive-lockdown-not-cancelled = The lockdown could not be cancelled:

## Use with other programs

azdrive-bridge-imap-server = IMAP server (incoming mail)
azdrive-bridge-imap-port = IMAP port
azdrive-bridge-smtp-server = SMTP server (outgoing mail)
azdrive-bridge-smtp-port = SMTP port
azdrive-bridge-security = Connection security
azdrive-bridge-security-none = None (the bridge answers this computer only)
azdrive-bridge-user = User name
azdrive-bridge-webdav = Server address (WebDAV)
azdrive-bridge-caldav = Server address (CalDAV / CardDAV)
azdrive-bridge-copy = Copy
azdrive-bridge-not-set-up = The Azlin Bridge shows your Azlin drive to Finder, Explorer and the file managers (and its mail and calendars to other programs) on this computer. It is not set up here: run azul-bridge init --address <your address>, then azul-bridge serve (azul-bridge autostart enable starts it at every login).
azdrive-bridge-running = The Azlin Bridge is running on this computer: connect to it with these settings and the bridge's password (Finder: Go > Connect to Server; Explorer: Map network drive).
azdrive-bridge-not-running = The Azlin Bridge is set up but not running: start it with azul-bridge serve (azul-bridge autostart enable starts it at every login). Then connect with these settings and the bridge's password.
azdrive-bridge-files = Files (Finder, Explorer, the file managers)
azdrive-bridge-mail = Mail (Apple Mail, Thunderbird, Outlook)
azdrive-bridge-calendars = Calendars and contacts
azdrive-bridge-copy-all = Copy all settings
azdrive-bridge-every-setting = every setting
azdrive-bridge-copy-password = Copy password
azdrive-bridge-copied = Copied: { $what }.
azdrive-bridge-no-password = The bridge's secrets file has no password: azul-bridge password makes a new one.
azdrive-bridge-password-copied = Copied the bridge's password: paste it where the other program asks for the password.

## Panes

azdrive-pane-details = Details

## Recovery health

azdrive-method-code = Recovery code
azdrive-method-contacts = Trusted contacts
azdrive-method-device = Another device
azdrive-method-passkey = Passkey
azdrive-health-line = { $health ->
        [green] Green
        [yellow] Yellow
       *[red] Red
    }: { $methods ->
        [one] 1 method
       *[other] { $methods } methods
    }, { $checked ->
        [never] the code never checked
       *[other] the code checked on { $checked }
    }
azdrive-health-add-method = add a second method
azdrive-health-check-code = check the code (Options > Drives > Test)
azdrive-health-new-code = make a new recovery code
azdrive-method-code-checked = Checked on { $on }
azdrive-method-code-no-checks = no more checks
azdrive-method-code-next = next check on { $on }
azdrive-method-code-never = Never typed back: make a new recovery code
azdrive-method-contacts-none = None
azdrive-method-contacts-handed = { $handed } of { $total } shares handed over ({ $names })
azdrive-method-contacts-too-few = { $handed } of { $total } shares handed over ({ $names }): two open the code
azdrive-method-devices = { $count ->
        [0] None counted
        [one] 1 other device has the key
       *[other] { $count } other devices have the key
    }
azdrive-method-passkey-later = Not yet: a passkey comes with a later AzDrive
azdrive-methods-warning = Fewer than two ways back in: with one, losing it locks you out of the drive. Add trusted contacts or another device.

## The emergency kit and the drills

azdrive-kit-title = Azlin Emergency Kit
azdrive-kit-generic-name = your Azlin drive
azdrive-kit-only-key = This is the only key to your files. Azlin can't reset it: without this code and without your devices nobody can open them, Azlin neither.
azdrive-kit-keep-apart = Keep it apart from your computers and phones: in a drawer at home, in a safe, or on a USB stick kept somewhere safe. Never type it into a website and never send it to anyone.
azdrive-kit-how-to-use = To use it, in AzDrive: the drive's menu, then "Unlock with the recovery code" on a computer the drive is on, or "Lock down with the recovery code" when your devices are lost or in someone else's hands.
azdrive-kit-whoever-has-it = Whoever has this page can ask for your drive. Your devices are told at once, and a recovery with it waits 48 hours so that they can stop it.
azdrive-kit-code-label = Your recovery code
azdrive-kit-qr-label = The same code as a QR code: a phone's camera reads it, and AzDrive takes the text it shows as it is.
azdrive-kit-for-drive = For the drive "{ $name }", made on { $made }.
azdrive-kit-for-generic = For { $name }, made on { $made }.
azdrive-kit-no-pdf = azul's PDF writer made no file (a build without its `pdf` feature?).
azdrive-kit-print = Print…
azdrive-kit-save-pdf = Save as PDF…
azdrive-kit-usb = Save to a USB stick…
azdrive-kit-take-photo = Or take a photo of this QR code with your phone and keep it offline.
azdrive-kit-print-open = It is open in your PDF viewer: print it from there. AzDrive deletes this copy when the dialog closes.
azdrive-kit-print-failed = It could not be opened for printing: { $why }
azdrive-kit-saved = Saved { $name }.
azdrive-kit-not-saved = It was not saved.
azdrive-kit-usb-title = Save it to a USB stick
azdrive-kit-writing = Writing it to { $folder }…
azdrive-kit-written = It is on the stick: { $path }
azdrive-kit-not-written = It was not written: { $why }
azdrive-drill-what = A short check that the recovery code of "{ $name }" still works: type it from your emergency kit (any case, with or without dashes). It is checked on this computer and kept nowhere.
azdrive-drill-code = The recovery code
azdrive-drill-later = Later
azdrive-drill-stop = Stop the checks
azdrive-drill-check = Check
azdrive-drill-title = Do you still have your recovery kit?
azdrive-drill-passed-title = Your recovery kit works
azdrive-drill-passed-next = That is the recovery code of "{ $name }". AzDrive asks again on { $day }.
azdrive-drill-passed = That is the recovery code of "{ $name }".
azdrive-drill-not-the-code = That is not this drive's recovery code. If your kit is lost, make a new code (the drive's menu: I was hacked: new keys) and print its kit.
azdrive-drill-not-a-code = That is not a recovery code: 26 letters and digits, in five groups.
azdrive-drill-not-this-drives = That is not this drive's recovery code.
azdrive-drill-not-checked = The code could not be checked
azdrive-drill-stopped = No more checks of the recovery code: two printed shares are a second way in.
azdrive-method-test = Test
azdrive-method-add = Add…
azdrive-method-remove = Remove
azdrive-method-count-again = Count again
azdrive-recovery-none = No encrypted drive yet: an Azlin drive's menu in the source list offers "Encrypt this drive".
azdrive-recovery-section = Recovery
azdrive-recovery-held-section = Shares you hold for others
azdrive-method-device-how = A phone or a second computer that has the drive's key is a way back in when this one is lost: join it with a join code from this computer (azcloud invite), pass the code by a file or a QR code, then Count again here. It counts best as a device of another kind - a phone beside a computer - since both can be lost together.
azdrive-devices-not-counted = The other devices were not counted: { $why }

## Trusted contacts

azdrive-contacts-no-name = Person { $n } has no name.
azdrive-contacts-bad-key = That is not a contact key for { $name }: their AzDrive shows one starting with azlin-contact: (Options > Drives > Be someone's trusted contact).
azdrive-contacts-key-twice = { $name } has the contact key of someone above.
azdrive-contacts-two-shares = Two shares give the code back: paste the replies of two contacts (or type their printed shares).
azdrive-contacts-shares-no-code = These two shares do not give a code back: one is mistyped, or of a code made before the last one.
azdrive-contacts-label-drive = the drive "{ $name }"
azdrive-contacts-label-generic = an Azlin drive
azdrive-share-title = Azlin Recovery Share
azdrive-share-subtitle = Share { $index } of { $of }, for { $name }, made on { $day }.
azdrive-share-one-of-three = This is one of three pieces of a recovery code for { $drive }. Alone it opens nothing: two of the three together give the code back.
azdrive-share-keep-safe = Keep it somewhere safe. The owner of the drive may ask you for it one day, when their computers and their kit are lost. Give it only to them: meet them, or call them on a number you know - never because of an email or a message you did not expect.
azdrive-share-how-to-use = They type the text below into AzDrive (the drive's menu, then "Recover with trusted contacts"), or scan the QR code. Their devices are told, and the recovery waits 48 hours.
azdrive-share-label = The share
azdrive-share-qr-label = The same share as a QR code.
azdrive-contacts-forgotten = The trusted contacts are no longer counted. The shares they hold still give back the recovery code: to make them useless, make a new code (the drive's menu: I was hacked: new keys).
azdrive-contacts-copied = Copied: paste it into a message to that person.
azdrive-contacts-add-what = Three people you trust each get a piece of this drive's recovery code. Any two of them together can give it back to you; one alone learns nothing. A recovery with it still waits 48 hours for your devices.
azdrive-contacts-add-keys = For someone with AzDrive, paste the contact key their AzDrive shows (Options > Drives > Be someone's trusted contact). Leave it empty to print their piece instead.
azdrive-contacts-add-code = Your recovery code (from the emergency kit)
azdrive-contacts-person = Person { $n }
azdrive-contacts-name = Name
azdrive-contacts-key-placeholder = azlin-contact:... (empty: print it)
azdrive-contacts-make = Make the shares
azdrive-contacts-add-title = Trusted contacts for "{ $name }"
azdrive-contacts-made-what = Hand each piece to its person. A sealed one opens only in their AzDrive: send it by any message. A printed one is paper: give it to them yourself.
azdrive-contacts-made-row = { $name } - share { $index } of { $of }
azdrive-contacts-made-row-handed = { $name } - share { $index } of { $of } (handed over)
azdrive-contacts-done = Done
azdrive-contacts-made-title = The shares of "{ $name }"
azdrive-contacts-key-title = Your contact key for them
azdrive-contacts-key-what = Send this to the person who asked you to be their trusted contact. It is no secret: it lets their AzDrive seal a piece of their recovery code that only this computer opens. Then take the piece they send you (Options > Drives > Take a share).
azdrive-contacts-help-title = Help with a recovery
azdrive-contacts-help-what = Paste the request your contact sent you (it starts with azlin-recover:).
azdrive-contacts-take-title = Take a share
azdrive-contacts-take-what = Paste the share someone sent you (it starts with azlin-share:). It stays sealed on this computer; only your key opens it.
azdrive-contacts-continue = Continue
azdrive-contacts-answer-what = Someone asks for your piece of a recovery code. Scammers pretend to be friends: answer only if the owner asked you themselves - meet them, or call them on a number you know - and their screen shows this safety number:
azdrive-contacts-answer-safe = Your piece alone opens nothing, and their devices are told and can stop the recovery for 48 hours.
azdrive-contacts-send-answer = Send them this answer:
azdrive-contacts-numbers-match = The numbers match: answer for { $label }
azdrive-contacts-answer-title = Is it really them?
azdrive-contacts-recover-test-what = A test of your trusted contacts: send this request to two of them. Their answers are checked on this computer; nothing is locked down.
azdrive-contacts-recover-what = Send this request to two of your trusted contacts, and call them or meet them: they answer only when the safety number on their screen is this one.
azdrive-contacts-answers = Their answers (azlin-share-reply:...), or printed shares (S1-..., S2-...)
azdrive-contacts-recover = Recover
azdrive-contacts-test-title = Test the trusted contacts of "{ $name }"
azdrive-contacts-recover-title = Recover "{ $name }" with trusted contacts
azdrive-contacts-rebuilt-what = Your trusted contacts gave back the recovery code of "{ $name }". The lockdown with it is pending{ $until ->
        [none] { "" }
       *[other] { " " }until { $until }
    }: your other devices are told and may cancel it. Then the drive is this computer's: open it with this code (the drive's menu: Unlock with the recovery code).
azdrive-contacts-write-it-down = Write it down, or keep a new emergency kit:
azdrive-contacts-rebuilt-title = Your recovery code is back
azdrive-contacts-check-code-first = This computer does not know the drive's recovery key yet: check the code once first (Options > Drives > Test).
azdrive-contacts-no-share-held = This computer holds no share for anyone (Take a share first).
azdrive-contacts-not-a-request = That is not a request (azlin-recover:...).
azdrive-contacts-not-a-share = That is not a sealed share (azlin-share:...).
azdrive-contacts-err-not-a-share = not a sealed share
azdrive-contacts-err-other-key = it is sealed to a key this computer does not have
azdrive-contacts-err-not-a-request = not a request
azdrive-contacts-err-damaged = the share kept is damaged
azdrive-contacts-err-key-gone = this computer's key for that share is gone
azdrive-contacts-no-key = No contact key was made: { $why }
azdrive-contacts-taken = You hold share { $index } of { $of } of the recovery code of { $label }. If they ever ask you for it: Options > Drives > Help with a recovery.
azdrive-contacts-not-taken = The share was not taken: { $why }.
azdrive-contacts-no-answer = No answer was made: { $why }.
azdrive-contacts-no-request = No recovery request was made: { $why }
azdrive-contacts-old-code = The two shares give a code, but not this drive's: they are of a code made before the last one. Add the contacts again.
azdrive-contacts-tested-title = Your trusted contacts work
azdrive-contacts-tested = Two of their shares give back this drive's recovery code. Nothing was locked down.
azdrive-contacts-held-key-only = A contact key sent, no share taken yet
azdrive-contacts-held-taken = A share of { $label }, taken on { $day }
azdrive-contacts-held = A share of { $label }
azdrive-contacts-held-none = None. When someone asks you to be their trusted contact, make a key for them here.
azdrive-contacts-be-contact = Be someone's trusted contact…
azdrive-contacts-take = Take a share…
azdrive-contacts-help = Help with a recovery…
