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
