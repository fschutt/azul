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
