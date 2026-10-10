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
