//! Explorer's Ribbon: FILE (the backstage), HOME (Clipboard, Organize, New,
//! Open, Select), SHARE (Send, Share with), VIEW (Panes, Layout, Current
//! view, Show/hide) and DRIVE (the drives). Every button runs its
//! [`Action`] or is greyed with the reason as its tooltip.

use azul::{
    callbacks::{
        ButtonOnClickCallbackType, CheckBoxOnToggleCallbackType, RibbonGalleryOnSelectCallbackType,
        RibbonOnTabClickCallbackType,
    },
    menu::Menu,
    prelude::*,
    str::String as AzString,
    vec::RibbonGalleryCellVec,
    widgets::{
        CheckBoxState, Ribbon, RibbonAppButton, RibbonArrow, RibbonButton, RibbonColumn,
        RibbonGallery, RibbonGalleryCell, RibbonGroup, RibbonItem, RibbonRow, RibbonTab,
    },
};

use crate::{
    actions::{self, action_ref, menu_item, on_action, why_not, Action, ActionRef, Toggle},
    browse::Place,
    model::ViewLayout,
    with_state, DriveState,
};

/// A ribbon button running `action` (greyed with the reason when it cannot run).
fn button(s: &DriveState, app: &RefAny, icon: &str, label: &str, action: Action) -> RibbonButton {
    let reason = why_not(s, &action);
    let mut b = RibbonButton::create(AzString::from(icon), AzString::from(label))
        .with_on_click(action_ref(app, action), on_action as ButtonOnClickCallbackType);
    if let Some(reason) = reason {
        b = b.with_disabled(AzString::from(reason));
    }
    b
}

/// A button that opens a menu.
fn menu_button(s: &DriveState, app: &RefAny, icon: &str, label: &str, action: Action) -> RibbonButton {
    button(s, app, icon, label, action).with_arrow(RibbonArrow::Menu)
}

/// A button that is on or off.
fn toggle_button(app: &RefAny, icon: &str, label: &str, which: Toggle, on: bool) -> RibbonButton {
    RibbonButton::create(AzString::from(icon), AzString::from(label))
        .with_toggled(on)
        .with_on_click(
            action_ref(app, Action::Toggle(which)),
            on_action as ButtonOnClickCallbackType,
        )
}

/// A command AzDrive cannot do, greyed with the reason.
fn unavailable(icon: &str, label: &str, reason: &str) -> RibbonButton {
    RibbonButton::create(AzString::from(icon), AzString::from(label))
        .with_disabled(AzString::from(reason))
}

fn large(b: RibbonButton) -> RibbonItem {
    RibbonItem::LargeButton(b)
}

fn small(b: RibbonButton) -> RibbonItem {
    RibbonItem::SmallButton(b)
}

fn column(items: Vec<RibbonItem>) -> RibbonItem {
    RibbonItem::Column(
        items
            .into_iter()
            .fold(RibbonColumn::create(), |c, it| c.with_item(it)),
    )
}

/// A check box with its label (Show/hide): both toggle `which`.
fn check(app: &RefAny, label: &str, which: Toggle, on: bool) -> RibbonItem {
    RibbonItem::Row(
        RibbonRow::create()
            .with_item(RibbonItem::Check(
                CheckBox::create(on)
                    .with_accessibility_name(AzString::from(label))
                    .with_on_toggle(
                        action_ref(app, Action::Toggle(which)),
                        on_check as CheckBoxOnToggleCallbackType,
                    ),
            ))
            .with_item(RibbonItem::Custom(
                Dom::create_span_with_text(AzString::from(label))
                    .with_css("font-size: 12px; margin-left: 4px; cursor: default;")
                    .with_callback(
                        EventFilter::Hover(HoverEventFilter::Click),
                        action_ref(app, Action::Toggle(which)),
                        on_action,
                    ),
            )),
    )
}

extern "C" fn on_check(mut data: RefAny, mut info: CallbackInfo, _state: CheckBoxState) -> Update {
    let Some((mut app, action)) = data
        .downcast_ref::<ActionRef>()
        .map(|r| (r.app.clone(), r.action.clone()))
    else {
        return Update::DoNothing;
    };
    with_state(&mut app, &mut info, |info, app, s| {
        actions::run_action(info, app, s, action)
    })
}

fn group(label: &str, items: Vec<RibbonItem>) -> RibbonGroup {
    items
        .into_iter()
        .fold(RibbonGroup::create(AzString::from(label)), |g, it| {
            g.with_item(it)
        })
}

fn home_tab(s: &DriveState, app: &RefAny) -> RibbonTab {
    let pinned = match &s.place {
        Place::Folder { drive, prefix } => s.settings.is_pinned(drive, prefix),
        _ => false,
    };
    let undo_label = s.undo.last().map_or_else(|| String::from("Undo"), |op| op.label());
    RibbonTab::create(AzString::from("HOME"))
        .with_group(group(
            "Clipboard",
            vec![
                large(
                    button(s, app, "push_pin", "Pin to Quick access", Action::Pin)
                        .with_toggled(pinned),
                ),
                large(button(s, app, "content_copy", "Copy", Action::Copy)),
                large(button(s, app, "content_paste", "Paste", Action::Paste)),
                column(vec![
                    small(button(s, app, "content_cut", "Cut", Action::Cut)),
                    small(button(s, app, "link", "Copy path", Action::CopyPath)),
                    small(unavailable(
                        "shortcut",
                        "Paste shortcut",
                        "Shortcuts are Windows links; a drive here holds files and folders only.",
                    )),
                ]),
            ],
        ))
        .with_group(group(
            "Organize",
            vec![
                large(menu_button(s, app, "drive_file_move", "Move to", Action::MoveToMenu)),
                large(menu_button(s, app, "file_copy", "Copy to", Action::CopyToMenu)),
                large(
                    button(s, app, "delete", "Delete", Action::DeleteMenu)
                        .with_arrow(RibbonArrow::Split),
                ),
                large(button(
                    s,
                    app,
                    "drive_file_rename_outline",
                    "Rename",
                    Action::Rename,
                )),
                column(vec![small(button(s, app, "undo", &undo_label, Action::Undo))]),
            ],
        ))
        .with_group(group(
            "New",
            vec![
                large(button(s, app, "create_new_folder", "New folder", Action::NewFolder)),
                column(vec![
                    small(menu_button(s, app, "note_add", "New item", Action::NewItemMenu)),
                    small(
                        RibbonButton::create(
                            AzString::from("bolt"),
                            AzString::from("Easy access"),
                        )
                        .with_arrow(RibbonArrow::Menu)
                        .with_on_click(app.clone(), on_easy_access as ButtonOnClickCallbackType),
                    ),
                ]),
            ],
        ))
        .with_group(group(
            "Open",
            vec![
                large(button(s, app, "info", "Properties", Action::Properties)),
                column(vec![
                    small(menu_button(s, app, "open_in_new", "Open", Action::OpenMenu)),
                    small(button(s, app, "edit", "Edit", Action::Edit)),
                    small(unavailable(
                        "history",
                        "History",
                        "Earlier versions need a versioned bucket; AzDrive keeps one version of \
                         a file.",
                    )),
                ]),
            ],
        ))
        .with_group(group(
            "Select",
            vec![column(vec![
                small(button(s, app, "select_all", "Select all", Action::SelectAll)),
                small(button(s, app, "deselect", "Select none", Action::SelectNone)),
                small(button(s, app, "flip", "Invert selection", Action::InvertSelection)),
            ])],
        ))
}

/// HOME > New > Easy access: pin the folder, or add a folder as a drive.
extern "C" fn on_easy_access(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |info, app, s| {
        let items = vec![
            menu_item(
                app,
                "Pin to Quick access",
                Action::Pin,
                why_not(s, &Action::Pin).is_some(),
            ),
            menu_item(app, "Add a folder as a drive...", Action::AddLocalDrive, false),
        ];
        info.open_menu_for_hit_node(Menu::create(items));
    })
}

fn share_tab(s: &DriveState, app: &RefAny) -> RibbonTab {
    RibbonTab::create(AzString::from("SHARE"))
        .with_group(group(
            "Send",
            vec![
                large(button(s, app, "share", "Share", Action::Share)),
                large(button(s, app, "email", "Email", Action::Email)),
                large(button(s, app, "archive", "Zip", Action::Zip)),
                large(button(s, app, "download", "Download", Action::Download)),
                large(button(s, app, "upload", "Upload", Action::Upload)),
                column(vec![
                    small(unavailable(
                        "album",
                        "Burn to disc",
                        "AzDrive cannot write discs: this computer has no burner it can use.",
                    )),
                    small(unavailable(
                        "print",
                        "Print",
                        "Printing a file needs the app that opens it: use Open, then print there.",
                    )),
                    small(unavailable("fax", "Fax", "No fax device is set up.")),
                ]),
            ],
        ))
        .with_group(group(
            "Share with",
            vec![large(unavailable(
                "security",
                "Advanced security",
                "Who may read a cloud drive is set in its bucket policy; a local folder's in the \
                 system's sharing settings.",
            ))],
        ))
}

fn view_tab(s: &DriveState, app: &RefAny) -> RibbonTab {
    let cells: Vec<RibbonGalleryCell> = ViewLayout::ALL
        .iter()
        .map(|layout| {
            RibbonGalleryCell::create(
                Dom::create_icon(AzString::from(layout.icon())).with_css("font-size: 20px;"),
                AzString::from(layout.label()),
            )
        })
        .collect();
    let selected = ViewLayout::ALL
        .iter()
        .position(|l| *l == s.settings.layout)
        .unwrap_or(0);
    RibbonTab::create(AzString::from("VIEW"))
        .with_group(group(
            "Panes",
            vec![
                large(toggle_button(
                    app,
                    "vertical_split",
                    "Navigation pane",
                    Toggle::NavigationPane,
                    s.settings.navigation_pane,
                )),
                column(vec![
                    small(toggle_button(
                        app,
                        "preview",
                        "Preview pane",
                        Toggle::PreviewPane,
                        s.settings.preview_pane,
                    )),
                    small(toggle_button(
                        app,
                        "view_sidebar",
                        "Details pane",
                        Toggle::DetailsPane,
                        s.settings.details_pane,
                    )),
                ]),
            ],
        ))
        .with_group(group(
            "Layout",
            vec![RibbonItem::Gallery(
                RibbonGallery::create(RibbonGalleryCellVec::from(cells))
                    .with_selected(selected)
                    .with_on_select(
                        app.clone(),
                        on_layout_select as RibbonGalleryOnSelectCallbackType,
                    ),
            )],
        ))
        .with_group(group(
            "Current view",
            vec![
                large(menu_button(s, app, "sort", "Sort by", Action::SortMenu)),
                column(vec![
                    small(menu_button(s, app, "category", "Group by", Action::GroupMenu)),
                    small(menu_button(s, app, "view_column", "Add columns", Action::ColumnsMenu)),
                    small(button(
                        s,
                        app,
                        "fit_screen",
                        "Size all columns to fit",
                        Action::FitColumns,
                    )),
                ]),
            ],
        ))
        .with_group(group(
            "Show/hide",
            vec![
                column(vec![
                    check(
                        app,
                        "Item check boxes",
                        Toggle::ItemCheckboxes,
                        s.settings.item_checkboxes,
                    ),
                    check(
                        app,
                        "File name extensions",
                        Toggle::Extensions,
                        s.settings.show_extensions,
                    ),
                    check(app, "Hidden items", Toggle::HiddenItems, s.settings.show_hidden),
                ]),
                large(unavailable(
                    "visibility_off",
                    "Hide selected items",
                    "Here an item is hidden when its name starts with a dot: rename it (F2) to \
                     hide it.",
                )),
                large(button(s, app, "tune", "Options", Action::Options)),
            ],
        ))
}

extern "C" fn on_layout_select(mut data: RefAny, mut info: CallbackInfo, index: usize) -> Update {
    with_state(&mut data, &mut info, |info, app, s| {
        if let Some(layout) = ViewLayout::ALL.get(index) {
            actions::set_layout(info, app, s, *layout);
        }
    })
}

fn drive_tab(s: &DriveState, app: &RefAny) -> RibbonTab {
    RibbonTab::create(AzString::from("DRIVE")).with_group(group(
        "Drives",
        vec![
            large(button(s, app, "add_circle", "Add S3 drive", Action::AddDrive)),
            large(button(
                s,
                app,
                "create_new_folder",
                "Add folder as drive",
                Action::AddLocalDrive,
            )),
            large(button(s, app, "remove_circle", "Remove drive", Action::RemoveDrive)),
            large(button(s, app, "info", "Drive properties", Action::DriveProperties)),
            large(button(s, app, "refresh", "Refresh", Action::Refresh)),
        ],
    ))
}

/// The ribbon; FILE opens the backstage.
pub(crate) fn ribbon(s: &DriveState, app: &RefAny) -> Dom {
    let mut ribbon = Ribbon::create(vec![
        home_tab(s, app),
        share_tab(s, app),
        view_tab(s, app),
        drive_tab(s, app),
    ])
    .with_active_tab(s.ribbon_tab)
    .with_app_button(
        RibbonAppButton::create(AzString::from("FILE")).with_on_click(
            action_ref(app, Action::Options),
            on_action as ButtonOnClickCallbackType,
        ),
    );
    ribbon.set_on_tab_click(app.clone(), on_ribbon_tab as RibbonOnTabClickCallbackType);
    ribbon.dom_desktop()
}

extern "C" fn on_ribbon_tab(mut data: RefAny, _info: CallbackInfo, index: usize) -> Update {
    let Some(mut s) = data.downcast_mut::<DriveState>() else {
        return Update::DoNothing;
    };
    s.ribbon_tab = index;
    Update::RefreshDom
}
