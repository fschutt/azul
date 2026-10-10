# AzContacts' words in English (src/l10n.rs). Every key the source names is here and in de.ftl.

## The toolbar
azcontacts-new = New

## What the address book tells you

azcontacts-problem-no-name = A contact needs a name or a company.
azcontacts-problem-not-email = "{ $value }" is not an email address.
azcontacts-problem-not-a-date = "{ $value }" is not a date (DD.MM.YYYY, or DD.MM. without a year).
azcontacts-vcard-no-end = line { $line }: a card without END:VCARD
azcontacts-vcard-no-begin = line { $line }: END:VCARD without BEGIN
azcontacts-vcard-read-as-3 = line { $line }: vCard { $version } read as 3.0
azcontacts-vcard-bad-line = line { $line }: { $why }
azcontacts-vcard-last-no-end = the last card has no END:VCARD
azcontacts-load-not-a-file-name = { $file }: not a contact file name
azcontacts-load-no-vcard = { $file }: no vCard in the file
azcontacts-load-many-cards = { $file }: { $count } cards, the first is used
azcontacts-import-new = { $count } new
azcontacts-import-duplicates = { $count ->
    [one] 1 possible duplicate
   *[other] { $count } possible duplicates
 }
azcontacts-import-updates = { $count ->
    [one] 1 update
   *[other] { $count } updates
 }
azcontacts-dupe-same-name = same name
azcontacts-dupe-same-email = same email { $email }
azcontacts-dupe-same-phone = same phone number
azcontacts-field-skip = Do not import
azcontacts-first-name = First name
azcontacts-last-name = Last name
azcontacts-field-full-name = Full name
azcontacts-nickname = Nickname
azcontacts-field-email = E-mail
azcontacts-field-mobile-phone = Mobile phone
azcontacts-field-work-phone = Work phone
azcontacts-field-home-phone = Home phone
azcontacts-company = Company
azcontacts-department = Department
azcontacts-job-title = Job title
azcontacts-birthday = Birthday
azcontacts-street = Street
azcontacts-city = City
azcontacts-field-region = State / region
azcontacts-field-postal-code = Postal code
azcontacts-country = Country
azcontacts-field-web-page = Web page
azcontacts-notes = Notes
azcontacts-groups = Groups
azcontacts-csv-birthday-no-date = Row { $row }: the birthday "{ $value }" is no date; the contact is imported without it.
azcontacts-import-not-a-file = "{ $path }" is not a file.
azcontacts-files-not-read = { $count ->
    [one] 1 contact file could not be read fully
   *[other] { $count } contact files could not be read fully
 }
azcontacts-file-missing = "{ $file }" does not exist.
azcontacts-import-no-person = No row of the file names a person: map the columns below.
azcontacts-import-no-vcard = The file holds no vCard.
azcontacts-import-type-path = Type the path of a .vcf file, or choose one.

## The window

azcontacts-about-summary = Your contacts as plain vCard files, one per person, with groups, search, import, export and a duplicates finder. Part of the Azlin apps, built with azul.
azcontacts-contacts = Contacts
azcontacts-new-contact = New contact
azcontacts-shortcut-edit = Edit the selected contact
azcontacts-shortcut-save = Save the contact being edited
azcontacts-shortcut-cancel = Cancel editing
azcontacts-shortcut-up-down = Previous / next contact in the list
azcontacts-shortcut-import = Import a vCard file
azcontacts-shortcut-export = Export the list as vCard
azcontacts-possible-duplicates = Possible duplicates
azcontacts-shortcut-panes = Panes
azcontacts-shortcut-panes-next = Next / previous pane
azcontacts-category-contacts = Contacts
azcontacts-label-mobile = mobile
azcontacts-label-work = work
azcontacts-label-home = home
azcontacts-label-main = main
azcontacts-label-fax = fax
azcontacts-label-other = other
azcontacts-no-name = (no name)
azcontacts-all-contacts-count = All contacts ({ $count })
azcontacts-favourites-count = Favourites ({ $count })
azcontacts-duplicates-count = Possible duplicates ({ $count })
azcontacts-contacts-and-groups = Contacts and groups
azcontacts-all-contacts = All contacts
azcontacts-favourites = Favourites
azcontacts-search = Search contacts
azcontacts-reading-contacts = Reading your contacts…
azcontacts-no-contacts = No contacts yet
azcontacts-no-contacts-detail = Add one, or import a vCard file.
azcontacts-nobody-here = Nobody here
azcontacts-group-empty = This group has no contacts.
azcontacts-no-match = No contact matches “{ $query }”.
azcontacts-edit = Edit
azcontacts-favourite = Favourite
azcontacts-copy-vcard = Copy vCard
azcontacts-export = Export
azcontacts-mail = Mail
azcontacts-delete = Delete
azcontacts-delete-ask = Delete { $name }? Its file goes too.
azcontacts-keep = Keep
azcontacts-label-birthday = birthday
azcontacts-label-nickname = nickname
azcontacts-label-groups = groups
azcontacts-label-photo = photo
azcontacts-label-notes = notes
azcontacts-picture-in-card = a picture in the card
azcontacts-file = File: { $file }
azcontacts-label = Label
azcontacts-add-birthday = Add a birthday
azcontacts-year-unknown = Year unknown
azcontacts-year = Year
azcontacts-birth-year = Birth year
azcontacts-edit-contact = Edit contact
azcontacts-save = Save
azcontacts-change-photo = Change photo…
azcontacts-remove-photo = Remove photo
azcontacts-photo-set = A photo is set.
azcontacts-discard-ask = Discard your changes?
azcontacts-discard = Discard
azcontacts-keep-editing = Keep editing
azcontacts-name = Name
azcontacts-phone = Phone
azcontacts-add-phone = Add phone
azcontacts-email = Email
azcontacts-add-email = Add email
azcontacts-postcode = Postcode
azcontacts-region = Region
azcontacts-add-address = Add address
azcontacts-address = Address
azcontacts-birthday-format = DD.MM.YYYY, or DD.MM. without a year
azcontacts-add-to-a-group = Add to a group
azcontacts-add = Add
azcontacts-field-name = Field name
azcontacts-value = Value
azcontacts-add-field = Add field
azcontacts-more-fields = More fields
azcontacts-status-new = new
azcontacts-status-updates = updates { $name }
azcontacts-status-duplicate = duplicate of { $name } ({ $score })
azcontacts-columns = Columns
azcontacts-column = Column { $header }
azcontacts-import-contacts = Import contacts
azcontacts-import-path = Path to a .vcf or .csv file
azcontacts-read = Read
azcontacts-choose-file = Choose file…
azcontacts-reading = Reading…
azcontacts-import-one = Import { $name }
azcontacts-add-to-group = Add to group
azcontacts-group-optional = Group (optional)
azcontacts-import = Import
azcontacts-import-what = vCard 3.0 and 4.0 files with one or many cards, or a CSV file (Outlook's or Google's, its columns mapped to the contact's fields). Nothing is imported before you press Import.
azcontacts-no-duplicates = No possible duplicates
azcontacts-no-duplicates-detail = No two contacts share a name, an email address or a phone number.
azcontacts-left = Left
azcontacts-right = Right
azcontacts-pair-of = { $pair } of { $pairs }
azcontacts-pair = { $a } ↔ { $b }   similarity { $score } ({ $why })
azcontacts-photo = Photo
azcontacts-keep-both-notes = Keep both notes
azcontacts-kept-from-both = Kept from both
azcontacts-not-a-duplicate = Not a duplicate
azcontacts-merge-contacts = Merge contacts
azcontacts-none-selected = No contact selected
azcontacts-none-selected-detail = Pick someone in the list, or add a new contact.
azcontacts-duplicates = Duplicates
azcontacts-settings = Settings
azcontacts-status-contacts = { $count ->
    [one] 1 contact
   *[other] { $count } contacts
 }
azcontacts-status-duplicates = { $count ->
    [one] 1 possible duplicate – review
   *[other] { $count } possible duplicates – review
 }
azcontacts-list-and-files = List and files
azcontacts-sort-by = Sort by
azcontacts-export-as = Export as
azcontacts-files-note = Every contact is one vCard file in { $folder }. { $count ->
    [one] 1 pair marked as not duplicates.
   *[other] { $count } pairs marked as not duplicates.
 }
azcontacts-nothing-to-export = Nothing to export.
azcontacts-exporting = { $count ->
    [one] Exporting 1 contact to { $path }
   *[other] Exporting { $count } contacts to { $path }
 }
azcontacts-imported-group = Imported
azcontacts-exported = Exported to { $path }
azcontacts-not-written = { $count ->
    [one] 1 file could not be written - see the log
   *[other] { $count } files could not be written - see the log
 }
azcontacts-copied-vcard = Copied { $name } as vCard { $version }
azcontacts-copied = Copied { $what }
azcontacts-deleted = Deleted { $name }
azcontacts-choose-photo = Choose a photo
azcontacts-reading-file = Reading { $path }
azcontacts-photo-done = Photo set
azcontacts-photo-too-large = That picture is larger than 2 MB.
azcontacts-imported = { $count ->
    [one] Imported 1 contact
   *[other] Imported { $count } contacts
 }
azcontacts-merged = Merged into { $name }
