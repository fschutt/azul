//! The navigation pane (`ShellNavigationPane`): a search line and "New list" in its header,
//! then three groups, each a tree:
//!
//! - "My Tasks": "All tasks" with the smart lists under it (Today, Upcoming, Scheduled,
//!   Flagged, Completed), each with its count;
//! - "My Lists": "On this computer" (the data folder; an S3 drive later) with the lists and
//!   the list groups ("Azlin launch" > Design, Website), each list with its open count;
//! - "Tags": "All tags" with every tag of an open task and its count.
//!
//! The counts are `TreeViewNode::with_badge` (the folder tree's unread count, MAIL2's).
//! The pane owns nothing: its one callback reports group / node / collapse, and the tree's
//! depth-first node index is mapped back to what was drawn ([`list_targets`]).

use azul::{
    callbacks::{
        ButtonOnClickCallbackType, ShellNavigationPaneOnEventCallbackType,
        TextInputOnTextInputCallbackType, TextInputOnVirtualKeyDownCallbackType,
    },
    dom::VirtualKeyCode,
    prelude::*,
    shells::{
        ShellNavigationGroup, ShellNavigationPane, ShellNavigationPaneEvent,
        ShellNavigationPaneEventKind,
    },
    widgets::{OnTextInputReturn, TextInputState, TextInputValid, TreeViewNode},
};
use chrono::NaiveDate;

use azul_appkit::l10n::{label, t};

use crate::{
    ids,
    model::TaskList,
    state::{self, Tasks},
    views::{self, NavEntry, Smart, View},
};

/// The smart lists in the "My Tasks" tree under "All tasks" (node 0), in node order.
pub const SMART_NODES: [Smart; 5] = [
    Smart::Today,
    Smart::Upcoming,
    Smart::Scheduled,
    Smart::Flagged,
    Smart::Completed,
];

/// What a node of the "My Lists" tree is, in depth-first order (node 0 is the root).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ListTarget {
    /// "On this computer".
    Root,
    /// A list group (folds and unfolds).
    Group(String),
    /// A list, by id.
    List(String),
}

/// The "My Lists" tree's nodes, depth-first, folded groups' lists included (the tree counts
/// hidden nodes too).
#[must_use]
pub fn list_targets(lists: &[TaskList]) -> Vec<ListTarget> {
    let mut out = vec![ListTarget::Root];
    for entry in views::nav_entries(lists) {
        match entry {
            NavEntry::List(i) => out.push(ListTarget::List(lists[i].id.clone())),
            NavEntry::Group { name, lists: members } => {
                out.push(ListTarget::Group(name));
                for i in members {
                    out.push(ListTarget::List(lists[i].id.clone()));
                }
            }
        }
    }
    out
}

/// A count as a badge: empty for 0.
fn badge(n: usize) -> String {
    if n == 0 {
        String::new()
    } else {
        n.to_string()
    }
}

fn smart_tree(s: &Tasks, today: NaiveDate) -> TreeViewNode {
    let mut root = TreeViewNode::create(label("aztasks-all-tasks"))
        .with_icon(Smart::All.icon())
        .with_badge(badge(views::smart_count(Smart::All, &s.tasks, today)))
        .with_expanded(true)
        .with_selected(s.view == View::Smart(Smart::All));
    for smart in SMART_NODES {
        // Completed shows no count (it only grows).
        let count = if smart == Smart::Completed {
            0
        } else {
            views::smart_count(smart, &s.tasks, today)
        };
        root.add_child(
            TreeViewNode::create(smart.label())
                .with_icon(smart.icon())
                .with_badge(badge(count))
                .with_selected(s.view == View::Smart(smart)),
        );
    }
    root
}

fn lists_tree(s: &Tasks) -> TreeViewNode {
    let mut root = TreeViewNode::create(label("aztasks-on-this-computer"))
        .with_icon("computer")
        .with_expanded(true);
    let list_node = |l: &TaskList| {
        TreeViewNode::create(l.name.as_str())
            .with_icon("checklist")
            .with_badge(badge(views::list_count(&l.id, &s.tasks)))
            .with_selected(s.view == View::List(l.id.clone()))
    };
    for entry in views::nav_entries(&s.lists) {
        match entry {
            NavEntry::List(i) => root.add_child(list_node(&s.lists[i])),
            NavEntry::Group { name, lists } => {
                let open: usize = lists.iter().map(|&i| views::list_count(&s.lists[i].id, &s.tasks)).sum();
                let mut group = TreeViewNode::create(name.as_str())
                    .with_icon("folder")
                    .with_badge(badge(open))
                    .with_expanded(!s.folded_groups.contains(&name));
                for i in lists {
                    group.add_child(list_node(&s.lists[i]));
                }
                root.add_child(group);
            }
        }
    }
    root
}

fn tags_tree(s: &Tasks) -> TreeViewNode {
    let tags = views::tags(&s.tasks);
    let title = if tags.is_empty() { "aztasks-no-tags" } else { "aztasks-all-tags" };
    let mut root = TreeViewNode::create(label(title)).with_icon("sell").with_expanded(true);
    for (tag, n) in tags {
        let selected = matches!(&s.view, View::Tag(t) if t.eq_ignore_ascii_case(&tag));
        root.add_child(
            TreeViewNode::create(format!("#{tag}"))
                .with_badge(badge(n))
                .with_selected(selected),
        );
    }
    root
}

/// The pane's header: the search line ("Search tasks", Enter searches) and "New list".
fn header(s: &Tasks, app: &RefAny) -> Dom {
    let search = TextInput::create_search()
        .with_text(s.search.as_str())
        .with_placeholder(label("aztasks-search"))
        .with_accessibility_name(label("aztasks-search"))
        .with_on_text_input(app.clone(), on_search_text as TextInputOnTextInputCallbackType)
        .with_on_virtual_key_down(app.clone(), on_search_key as TextInputOnVirtualKeyDownCallbackType)
        .dom()
        .with_id(ids::SEARCH);
    let new_list = Button::create(label("aztasks-cmd-new-list"))
        .with_icon("playlist_add")
        .with_on_click(app.clone(), on_new_list as ButtonOnClickCallbackType)
        .dom()
        .with_id(ids::NEW_LIST);
    Dom::create_div()
        .with_css("display: flex; flex-direction: column; padding: 8px; gap: 6px;")
        .with_child(search)
        .with_child(new_list)
}

/// The navigation pane at `today`.
pub fn pane(s: &Tasks, app: &RefAny, today: NaiveDate) -> Dom {
    let open = views::smart_count(Smart::All, &s.tasks, today);
    ShellNavigationPane::create()
        .with_header(header(s, app))
        .with_group(
            ShellNavigationGroup::create(label("aztasks-my-tasks"), smart_tree(s, today))
                .with_count(open)
                .with_open(s.nav_open[0]),
        )
        .with_group(
            ShellNavigationGroup::create(label("aztasks-my-lists"), lists_tree(s))
                .with_open(s.nav_open[1]),
        )
        .with_group(ShellNavigationGroup::create(label("aztasks-tags"), tags_tree(s)).with_open(s.nav_open[2]))
        .with_label(label("aztasks-lists"))
        .with_collapsed(s.nav_collapsed)
        .with_on_event(app.clone(), on_nav_event as ShellNavigationPaneOnEventCallbackType)
        .dom()
}

// ==== Callbacks ====

extern "C" fn on_nav_event(mut data: RefAny, mut info: CallbackInfo, event: ShellNavigationPaneEvent) -> Update {
    crate::with_tasks(&mut data, &mut info, |info, app, s| nav_event(info, app, s, &event))
}

fn nav_event(info: &mut CallbackInfo, app: &RefAny, s: &mut Tasks, event: &ShellNavigationPaneEvent) {
    match &event.kind {
        ShellNavigationPaneEventKind::GroupToggled => {
            if let Some(open) = s.nav_open.get_mut(event.group) {
                *open = event.expand;
            }
        }
        ShellNavigationPaneEventKind::CollapseToggled => s.nav_collapsed = !event.expand,
        ShellNavigationPaneEventKind::ModuleSelected => {}
        ShellNavigationPaneEventKind::NodeClicked => node_clicked(s, event.group, event.index),
        ShellNavigationPaneEventKind::NodeDropped => {
            let moves = node_dropped(s, event.group, event.index, state::now());
            crate::jobs::move_files(info, app, s, moves);
        }
        ShellNavigationPaneEventKind::NodeToggled => {
            if event.group == 1 {
                if let Some(ListTarget::Group(name)) = list_targets(&s.lists).get(event.index) {
                    if event.expand {
                        s.folded_groups.remove(name);
                    } else {
                        s.folded_groups.insert(name.clone());
                    }
                }
            }
        }
    }
}

/// A drag dropped on node `index` of group `group`: on a list it moves the tasks there; on
/// Today it makes them due today, on Upcoming tomorrow, on Flagged flags them, on Completed
/// completes them; on a tag it tags them. Returns attachment folders to move.
fn node_dropped(s: &mut Tasks, group: usize, index: usize, now: chrono::NaiveDateTime) -> Vec<(String, String)> {
    let ids = s.take_dropped();
    if ids.is_empty() {
        return Vec::new();
    }
    match group {
        1 => match list_targets(&s.lists).get(index) {
            Some(ListTarget::List(list)) => {
                let list = list.clone();
                s.move_tasks(&ids, &list)
            }
            _ => Vec::new(),
        },
        0 => {
            let smart = if index == 0 { None } else { SMART_NODES.get(index - 1).copied() };
            for id in &ids {
                let Some(i) = s.index_of(id) else {
                    continue;
                };
                match smart {
                    Some(Smart::Today) => s.tasks[i].due = Some(now.date()),
                    Some(Smart::Upcoming | Smart::Scheduled) => {
                        s.tasks[i].due = Some(now.date() + chrono::Duration::days(1));
                    }
                    Some(Smart::Flagged) => s.tasks[i].flagged = true,
                    Some(Smart::Completed) => {
                        if !s.tasks[i].is_done() {
                            s.toggle_done(i, now);
                        }
                        continue;
                    }
                    _ => continue,
                }
                s.tasks[i].reminded = None;
                s.save_task(i);
            }
            Vec::new()
        }
        2 => {
            if let Some((tag, _)) = index.checked_sub(1).and_then(|n| views::tags(&s.tasks).get(n).cloned()) {
                for id in &ids {
                    if let Some(i) = s.index_of(id) {
                        if s.tasks[i].add_tag(&tag) {
                            s.save_task(i);
                        }
                    }
                }
            }
            Vec::new()
        }
        _ => Vec::new(),
    }
}

/// A click on node `index` of group `group`.
fn node_clicked(s: &mut Tasks, group: usize, index: usize) {
    match group {
        0 => {
            let smart = if index == 0 {
                Some(Smart::All)
            } else {
                SMART_NODES.get(index - 1).copied()
            };
            if let Some(smart) = smart {
                s.show(View::Smart(smart));
            }
        }
        1 => match list_targets(&s.lists).get(index).cloned() {
            Some(ListTarget::List(id)) => s.show(View::List(id)),
            Some(ListTarget::Group(name)) => {
                if !s.folded_groups.remove(&name) {
                    s.folded_groups.insert(name);
                }
            }
            Some(ListTarget::Root) | None => s.show(View::Smart(Smart::All)),
        },
        2 => {
            if index == 0 {
                return;
            }
            if let Some((tag, _)) = views::tags(&s.tasks).get(index - 1) {
                s.show(View::Tag(tag.clone()));
            }
        }
        _ => {}
    }
}

extern "C" fn on_search_text(mut data: RefAny, _info: CallbackInfo, state: TextInputState) -> OnTextInputReturn {
    if let Some(mut s) = data.downcast_mut::<Tasks>() {
        s.search = state.get_text().as_str().to_string();
    }
    OnTextInputReturn {
        update: Update::DoNothing,
        valid: TextInputValid::Yes,
    }
}

/// Enter searches (an empty line goes back to All); Escape clears the search.
extern "C" fn on_search_key(mut data: RefAny, mut info: CallbackInfo, state: TextInputState) -> OnTextInputReturn {
    let keep = OnTextInputReturn {
        update: Update::DoNothing,
        valid: TextInputValid::Yes,
    };
    let key = info.get_current_keyboard_state().current_virtual_keycode.into_option();
    let Some(mut s) = data.downcast_mut::<Tasks>() else {
        return keep;
    };
    match key {
        Some(VirtualKeyCode::Return | VirtualKeyCode::NumpadEnter) => {
            let query = state.get_text().as_str().trim().to_string();
            s.search = query.clone();
            if query.is_empty() {
                s.show(View::Smart(Smart::All));
            } else {
                s.show(View::Search(query));
            }
            info.prevent_default();
            OnTextInputReturn {
                update: Update::RefreshDom,
                valid: TextInputValid::Yes,
            }
        }
        Some(VirtualKeyCode::Escape) => {
            s.search.clear();
            if matches!(s.view, View::Search(_)) {
                s.show(View::Smart(Smart::All));
            }
            // The cleared field is the app's: take the typing as seen.
            let revision = info.get_document_text_revision();
            info.mark_text_revision_synced(revision);
            OnTextInputReturn {
                update: Update::RefreshDom,
                valid: TextInputValid::Yes,
            }
        }
        _ => keep,
    }
}

extern "C" fn on_new_list(mut data: RefAny, mut info: CallbackInfo) -> Update {
    crate::with_tasks(&mut data, &mut info, |_info, _app, s| {
        // A list's name in its file: the first one in the window's language.
        let name = t("aztasks-cmd-new-list");
        let id = s.new_list(&name, "");
        s.show(View::List(id.clone()));
        // Its name and colour are set in the list settings, shown at once.
        s.editing_list = Some(id.clone());
        s.drafts.list = id;
        s.drafts.list_name = name;
        s.drafts.list_group.clear();
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list(id: &str, order: i64, group: &str) -> TaskList {
        let mut l = TaskList::new(id.into(), id.into(), order);
        l.group = group.into();
        l
    }

    #[test]
    fn the_lists_tree_nodes_are_depth_first_with_folded_groups_counted() {
        let lists = vec![list("work", 1, ""), list("design", 2, "Azlin"), list("web", 3, "Azlin"), list("home", 4, "")];
        assert_eq!(
            list_targets(&lists),
            vec![
                ListTarget::Root,
                ListTarget::List("work".into()),
                ListTarget::Group("Azlin".into()),
                ListTarget::List("design".into()),
                ListTarget::List("web".into()),
                ListTarget::List("home".into()),
            ]
        );
    }

    #[test]
    fn the_smart_tree_lists_every_smart_list_but_all_under_all() {
        assert_eq!(SMART_NODES.len() + 1, Smart::ALL.len());
        assert!(!SMART_NODES.contains(&Smart::All));
        assert_eq!(badge(0), "");
        assert_eq!(badge(12), "12");
    }
}
