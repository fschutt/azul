# AzERP's words in English (src/l10n.rs). Every key the source names is here and in de.ftl; the
# views' labels are the last section.

## The window

azerp-about-summary = The ERP's asset management on azul: fixed assets with straight-line and declining-balance depreciation, categories, locations, a maintenance log, check-out and check-in, CSV import and export, and reports. The screens come from the ERP's view definitions; every record is a file in your data folder. Part of the Azlin apps, built with azul.
azerp-close-question = Save the open form?
azerp-close-text = Your changes are lost if you don't save them.
azerp-close-discard = Don't Save
azerp-save = Save
azerp-back = Back
azerp-next = Next
azerp-none = None.
azerp-status-assets =
    { $count ->
        [one] 1 asset
       *[other] { $count } assets
    }
azerp-status-book-value = Book value { $value }
azerp-status-saving =
    { $count ->
        [one] Saving 1 file...
       *[other] Saving { $count } files...
    }
azerp-reading-register = Reading the asset register...
azerp-reading-register-detail = The records are files in your data folder.
azerp-nothing-here = There is nothing here.
azerp-no-screen = This page has no screen yet.
azerp-panel-not-built = This panel is not built yet.
azerp-not-built = "{ $name }" is not built yet.
azerp-tab-table-missing = The table of this tab is missing.
azerp-table-no-records = This table reads no records.
azerp-nothing-recorded-asset = Nothing recorded for this asset yet.
azerp-add-first = Add the first one with the button above.
azerp-nothing-recorded = Nothing is recorded yet.
azerp-none-yet = { $title }: none yet
azerp-date-placeholder = YYYY-MM-DD
azerp-type-to-find = Type to find: { $what }
azerp-choice-none = (none)
azerp-delete-ask = Delete { $number }?
azerp-delete-what = { $number } { $name }, with its maintenance and check-out records.
azerp-delete-detail = The files are removed from the data folder. This cannot be undone.
azerp-delete = Delete

## The records' words

azerp-method-straight-line = Straight-line
azerp-method-declining-balance = Declining balance
azerp-status-in-use = In use
azerp-status-in-storage = In storage
azerp-status-checked-out = Checked out
azerp-status-in-repair = In repair
azerp-status-disposed = Disposed
azerp-maintenance-service = Service
azerp-maintenance-inspection = Inspection
azerp-maintenance-repair = Repair
azerp-maintenance-calibration = Calibration
azerp-maintenance-other = Other

## The overview and the schedule

azerp-book-value-today = Book value today
azerp-status = Status
azerp-custodian = Custodian
azerp-next-service = Next service
azerp-gain-on-disposal = Gain on disposal
azerp-loss-on-disposal = Loss on disposal
azerp-col-year = Year
azerp-col-months = Months
azerp-col-opening = Opening value
azerp-col-depreciation = Depreciation
azerp-col-accumulated = Accumulated
azerp-col-closing = Closing value
azerp-nothing-to-depreciate = Nothing to depreciate.
azerp-nothing-to-depreciate-detail = The residual value is the cost, or the asset was disposed of before its first month.
azerp-schedule = Depreciation schedule
azerp-chart-year-end-value = Book value at the end of each year
azerp-book-value = Book value

## The reports

azerp-assets-on-books = Assets on the books
azerp-cost = Cost
azerp-depreciation-year = Depreciation { $year }
azerp-checked-out = Checked out
azerp-disposed = Disposed of
azerp-chart-by-category = Book value by category
azerp-chart-by-year = Depreciation by year
azerp-services-due = Services due in the next { $days } days
azerp-service-due = { $number } { $name } - { $when }
azerp-service-overdue = { $number } { $name } - { $when } (overdue)
azerp-overdue-checkouts = Overdue check-outs
azerp-checkout-overdue =
    { $days ->
        [one] { $number } { $name } - { $custodian } - due { $due }, 1 day late
       *[other] { $number } { $name } - { $custodian } - due { $due }, { $days } days late
    }

## The depreciation run

azerp-step = Step { $step } of { $steps }: { $title }
azerp-nothing-depreciated = No asset is depreciated in { $year }.
azerp-total-year = Total { $year }
azerp-run-and-post = Run and post

## The import

azerp-choose-csv = Choose a CSV file...
azerp-import-help = A CSV file from a spreadsheet or another asset register: a header row naming the columns, one asset per row (comma, semicolon or tab). A row whose asset number is in the register updates that asset.
azerp-import-summary = { $file }: { $rows } rows - { $new } new, { $changed } changed, { $left } left out
azerp-rows-left-out = Rows left out
azerp-import-assets =
    { $count ->
        [one] Import 1 asset
       *[other] Import { $count } assets
    }
azerp-import-dialog = Import assets from CSV
azerp-cannot-read = { $path } cannot be read.
azerp-import-field-skip = (skip)
azerp-import-field-number = Asset number
azerp-import-field-name = Name
azerp-import-field-category = Category
azerp-import-field-location = Location
azerp-import-field-serial = Serial number
azerp-import-field-acquired = Acquisition date
azerp-import-field-cost = Acquisition cost
azerp-import-field-residual = Residual value
azerp-import-field-life = Useful life (years)
azerp-import-field-method = Depreciation method
azerp-import-field-rate = Declining rate (%)
azerp-import-field-status = Status
azerp-import-field-custodian = Custodian
azerp-import-field-maintenance-months = Maintenance interval (months)
azerp-import-field-disposed = Disposal date
azerp-import-field-disposal-amount = Disposal amount
azerp-import-field-book-value = Book value
azerp-import-field-notes = Notes
azerp-import-number-twice = Row { $row }: the asset number { $number } is in the file twice; the row is left out.
azerp-import-row = Row { $row }: { $problems }
azerp-import-acquired-not-a-day = the acquisition date "{ $text }" is not a day
azerp-import-no-acquisition-date = there is no acquisition date
azerp-import-no-acquisition-cost = there is no acquisition cost
azerp-import-life-not-years = the useful life "{ $text }" is not a number of years
azerp-import-no-useful-life = there is no useful life
azerp-import-unknown-method = the depreciation method "{ $text }" is not one AzERP knows
azerp-import-rate-not-percent = the declining rate "{ $text }" is not a percentage
azerp-import-unknown-status = the status "{ $text }" is not one AzERP knows
azerp-import-interval-not-months = the maintenance interval "{ $text }" is not a number of months
azerp-import-disposal-not-a-day = the disposal date "{ $text }" is not a day

## Problems

azerp-problem-asset-needs-name = The asset needs a name.
azerp-problem-asset-needs-number = The asset needs an asset number.
azerp-problem-cost-negative = The cost cannot be negative.
azerp-problem-residual-range = The residual value must be between 0 and the cost.
azerp-problem-life-at-most = The useful life is at most { $years } years.
azerp-problem-rate-at-most = The declining-balance rate is at most 100 %.
azerp-problem-disposal-negative = The disposal amount cannot be negative.
azerp-problem-disposal-before-acquisition = The disposal date is before the acquisition date.
azerp-problem-not-an-amount = "{ $text }" is not an amount: { $why }
azerp-amount-two-minus-signs = two minus signs
azerp-amount-no-digits = no digits
azerp-amount-only-digits = only digits, one decimal separator and thousands separators
azerp-amount-two-decimal-separators = two decimal separators
azerp-amount-more-than-two-decimals = more than two decimals
azerp-amount-not-grouped-by-three = the thousands are not grouped by three
azerp-amount-too-large = too large
azerp-problem-not-a-day = "{ $text }" is not a day
azerp-problem-not-a-whole-number = "{ $text }" is not a whole number
azerp-problem-not-a-method = "{ $text }" is not a depreciation method
azerp-problem-not-a-rate = "{ $text }" is not a rate
azerp-problem-not-a-status = "{ $text }" is not a status
azerp-problem-not-a-maintenance-kind = "{ $text }" is not a kind of maintenance
azerp-problem-required = { $label } is required.
azerp-problem-at-least = { $label } must be at least { $min }.
azerp-problem-at-most = { $label } must be at most { $max }.
azerp-problem-whole-number = { $label } must be a whole number.
azerp-problem-field-amount = { $label }: { $why }
azerp-problem-field-day = { $label } must be a day (YYYY-MM-DD).
azerp-problem-field-choice = { $label } must be one of the choices.

## Notices

azerp-no-page = There is no page at { $path }.
azerp-form-not-savable = This form cannot be saved yet.
azerp-record-gone = The record is not there any more.
azerp-asset-gone = The asset is not there any more.
azerp-saved = Saved.
azerp-number-taken = { $number } is the number of { $name } already.
azerp-saved-asset = Saved { $number } { $name }.
azerp-maintenance-logged = Logged maintenance of { $number }.
azerp-checked-out-already = { $number } is checked out already; check it in first.
azerp-disposed-no-checkout = A disposed asset cannot be checked out.
azerp-checked-out-to = { $number } is checked out to { $custodian }.
azerp-disposed-of = { $number } is disposed of.
azerp-not-checked-out = The asset is not checked out.
azerp-checked-in = { $number } is checked in.
azerp-deleted = Deleted { $number }.
azerp-exported-register = Exported the register to { $key }.
azerp-exported-schedule = Exported the schedule to { $key }.
azerp-run-posted = Posted the depreciation run of { $year } ({ $total }) to { $key }.
azerp-files-unreadable =
    { $count ->
        [one] 1 file could not be read.
       *[other] { $count } files could not be read.
    }
azerp-sample-in = The sample register is in: { $count } assets.
azerp-imported = Imported { $new } new and { $changed } changed assets from { $file }.
azerp-imported-left-out = Imported { $new } new and { $changed } changed assets from { $file }; { $left } rows left out.
azerp-not-saved = { $key } could not be saved: { $why }
azerp-file-missing = { $key } does not exist.

## Shortcuts

azerp-shortcuts-register = Register
azerp-shortcuts-form = Form
azerp-shortcut-move = Move through the assets
azerp-shortcut-open = Open the asset
azerp-shortcut-sort = Sort by a column, add a second key
azerp-shortcut-filter = Filter the cursor's column
azerp-shortcut-close-form = Close the form without saving
azerp-shortcut-copy = Copy the selected rows

## The views' labels (views/en.json, by the ERP's keys)

azerp-label-actions-add-asset = New asset
azerp-label-actions-add-category = New category
azerp-label-actions-add-location = New location
azerp-label-actions-cancel = Cancel
azerp-label-actions-check-in = Check in
azerp-label-actions-check-out = Check out
azerp-label-actions-delete = Delete
azerp-label-actions-depreciation-run = Depreciation run
azerp-label-actions-dispose = Dispose
azerp-label-actions-edit = Edit
azerp-label-actions-export-csv = Export CSV
azerp-label-actions-export-schedule = Export schedule
azerp-label-actions-import-csv = Import CSV
azerp-label-actions-log-maintenance = Log maintenance
azerp-label-actions-run-and-post = Run and post
azerp-label-actions-save = Save
azerp-label-actions-view = Open
azerp-label-actions-view-asset = Open asset
azerp-label-depreciation-declining-balance = Declining balance
azerp-label-depreciation-straight-line = Straight-line
azerp-label-fields-acquisition-cost-label = Cost
azerp-label-fields-acquisition-date-label = Acquired
azerp-label-fields-address-label = Address
azerp-label-fields-as-of-label = As of
azerp-label-fields-asset-count-label = Assets
azerp-label-fields-asset-name-label = Name
azerp-label-fields-asset-number-label = Asset no.
azerp-label-fields-book-value-label = Book value
azerp-label-fields-category-label = Category
azerp-label-fields-category-name-label = Category
azerp-label-fields-checked-in-label = Checked in
azerp-label-fields-checked-out-label = Checked out
azerp-label-fields-custodian-label = Custodian
azerp-label-fields-declining-rate-percent-label = Declining rate (%)
azerp-label-fields-depreciation-method-label = Depreciation method
azerp-label-fields-description-label = Description
azerp-label-fields-disposal-amount-label = Disposal amount
azerp-label-fields-disposal-date-label = Disposal date
azerp-label-fields-due-date-label = Due
azerp-label-fields-location-label = Location
azerp-label-fields-location-name-label = Location
azerp-label-fields-maintenance-cost-label = Cost
azerp-label-fields-maintenance-date-label = Date
azerp-label-fields-maintenance-interval-months-label = Service every (months)
azerp-label-fields-maintenance-kind-label = Kind
azerp-label-fields-note-label = Note
azerp-label-fields-notes-label = Notes
azerp-label-fields-performed-by-label = Performed by
azerp-label-fields-residual-value-label = Residual value
azerp-label-fields-run-date-label = Run date
azerp-label-fields-run-year-label = Year
azerp-label-fields-serial-number-label = Serial number
azerp-label-fields-status-label = Status
azerp-label-fields-useful-life-years-label = Useful life (years)
azerp-label-maintenance-calibration = Calibration
azerp-label-maintenance-inspection = Inspection
azerp-label-maintenance-other = Other
azerp-label-maintenance-repair = Repair
azerp-label-maintenance-service = Service
azerp-label-menu-asset-categories = Categories
azerp-label-menu-asset-locations = Locations
azerp-label-menu-asset-reports = Reports
azerp-label-menu-checkouts = Check-outs
azerp-label-menu-fixed-assets = Register
azerp-label-menu-maintenance = Maintenance
azerp-label-steps-parameters = Parameters
azerp-label-steps-preview = Preview
azerp-label-tab-checkouts = Check-outs
azerp-label-tab-depreciation-schedule = Depreciation schedule
azerp-label-tab-maintenance = Maintenance
azerp-label-tab-overview = Overview
azerp-label-view-asset-categories-title = Asset categories
azerp-label-view-asset-edit-title = Edit asset
azerp-label-view-asset-import-title = Import assets from CSV
azerp-label-view-asset-locations-title = Locations
azerp-label-view-asset-new-title = New asset
azerp-label-view-asset-reports-title = Asset reports
azerp-label-view-category-edit-title = Edit category
azerp-label-view-category-new-title = New category
azerp-label-view-checkout-new-title = Check out
azerp-label-view-checkouts-title = Check-outs
azerp-label-view-depreciation-run-new-title = Depreciation run
azerp-label-view-dispose-title = Dispose of the asset
azerp-label-view-fixed-assets-title = Fixed assets
azerp-label-view-location-edit-title = Edit location
azerp-label-view-location-new-title = New location
azerp-label-view-maintenance-title = Maintenance log
azerp-label-view-maintenance-new-title = Log maintenance
