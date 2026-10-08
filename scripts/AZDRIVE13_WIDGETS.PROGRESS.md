# AZDRIVE13 widgets (sub-agent of AZDRIVE13) - progress

Scope: layout/ only - AddressBar = Explorer 8's address row, RibbonFileMenu (Windows 8's File
menu as a transient window), RibbonAppButton.menu hook. No cargo here; the lead builds.

## DONE
- db6aadb6b W1 RED: address_bar tests `the_path_field_takes_the_keyboard_when_it_opens`,
  `a_click_on_the_current_crumb_goes_there_instead_of_starting_an_edit`.
- W2 (this commit): AddressBar rework - round Back/Forward, Recent, Up, the breadcrumb box
  (icon, segment + chevron per folder, « overflow with its own menu, Refresh at the end), path
  field focused on mount; AddressBar.icon + AddressBar.available_width (+ set/with); flat +
  flora looks (themes/flat.rs, themes/flora.rs address_bar sections); decl::active_border_color.

## IN PROGRESS
- W3 RibbonFileMenu + RibbonAppButton.menu hook.

## NEXT
- report (api.json list, tests).
