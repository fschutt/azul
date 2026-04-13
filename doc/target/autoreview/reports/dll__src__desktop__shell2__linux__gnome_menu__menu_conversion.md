# Review: dll/src/desktop/shell2/linux/gnome_menu/menu_conversion.rs

## Summary
- Lines: 311
- Public functions: 2 (`convert_menu`, `extract_actions`)
- Public structs/enums: 1 (`MenuConversion`)
- Findings: 1 high, 2 medium, 1 low

## Findings

### [HIGH] Dead Code — `convert_menu` and `extract_actions` are never called outside tests
- **Location**: `menu_conversion.rs:22-46` and `menu_conversion.rs:51-64`
- **Details**: Both public functions are re-exported from `mod.rs:60` but never called from `wayland/mod.rs`, `x11/mod.rs`, or `manager.rs`. The GNOME menu system in wayland/x11 uses `GnomeMenuManager` directly but never invokes `MenuConversion::convert_menu` or `MenuConversion::extract_actions`.
- **Evidence**: Grep for `convert_menu|extract_actions` in `dll/src/desktop/shell2/linux/wayland/` and `dll/src/desktop/shell2/linux/x11/` returned zero matches. Grep in `manager.rs` also returned zero matches. Only hits are in the file itself and its tests.
- **Recommendation**: Wire these into the menu update path (e.g. in wayland `mod.rs:1365` where the gnome_menu manager is used), or remove if the conversion is handled differently.

### [MEDIUM] Bug — Separator section hardcoded to `(0, 0)`
- **Location**: `menu_conversion.rs:114`
- **Details**: Separators are converted with `section: Some((0, 0))`, which always points to group 0 menu 0 (the root menu). In the DBus menu model, a section reference should point to a new group that contains the items after the separator — not back to the root. This would cause GNOME Shell to render separators incorrectly (or recurse into the root menu).
- **Recommendation**: Implement proper section handling by creating a new group for items after each separator, or use the DBus separator convention appropriate for the protocol version.

### [MEDIUM] Unnecessary clones in `extract_actions_recursive`
- **Location**: `menu_conversion.rs:174-176`
- **Details**: `menu_callback` is cloned to create `menu_callback_for_closure`, and then `menu_callback` itself is also moved into the `DbusAction` struct. `action_name` is similarly cloned for the closure. The `action_name_for_closure` clone could be avoided if `action_name` were moved into the closure and a separate clone made for the struct field instead.
- **Recommendation**: Minor efficiency issue — consider restructuring to reduce one clone.

### [LOW] Code Style — Repeated `match` on `menu_item_state`
- **Location**: `menu_conversion.rs:92-96` and `menu_conversion.rs:167-171`
- **Details**: The same match expression converting `MenuItemState` to `bool` appears twice. Could be a small helper method, but with only two occurrences this is minor.

## System Documentation
- System identified: yes — GNOME/DBus application menu system (Linux desktop integration)
- Existing doc: none (no `doc/guide/` file covers the Linux menu/DBus integration system)
- Doc needed: A guide covering the GNOME menu integration system — how Azul menus are exposed via DBus to GNOME Shell, the protocol layers (menu_protocol, actions_protocol, menu_conversion, manager), and how it's wired into the Wayland and X11 window backends.
