//! `--sample`: lists and tasks to try the app with, dated around today so every smart list
//! has something (an overdue task, today's, the week's, later ones, flagged, repeating,
//! with steps, notes and tags, and a few completed), and one reminder that is due at once
//! so the reminder banner shows on the first start.

use chrono::{Duration, NaiveDate, NaiveDateTime, NaiveTime, Weekday};

use crate::{
    model::{ListColor, Priority, Reminder, Subtask, Task, TaskList},
    recur::Repeat,
    views::ORDER_STEP,
};

/// The sample's lists and tasks at `now`; `new_id` mints every id.
pub fn sample(now: NaiveDateTime, new_id: &mut dyn FnMut() -> String) -> (Vec<TaskList>, Vec<Task>) {
    let today = now.date();
    let mut lists = Vec::new();
    let add_list = |name: &str, color: ListColor, group: &str, lists: &mut Vec<TaskList>, id: String| -> String {
        let order = i64::try_from(lists.len()).unwrap_or(0) + 1;
        let mut l = TaskList::new(id.clone(), name.to_string(), order);
        l.color = color;
        l.group = group.to_string();
        lists.push(l);
        id
    };
    let work = add_list("Work", ListColor::Blue, "", &mut lists, new_id());
    let groceries = add_list("Groceries", ListColor::Green, "", &mut lists, new_id());
    let household = add_list("Household", ListColor::Orange, "", &mut lists, new_id());
    let reading = add_list("Reading", ListColor::Purple, "", &mut lists, new_id());
    let design = add_list("Design", ListColor::Teal, "Azlin launch", &mut lists, new_id());
    let website = add_list("Website", ListColor::Pink, "Azlin launch", &mut lists, new_id());

    let mut tasks: Vec<Task> = Vec::new();
    let next_order = |list: &str, tasks: &[Task]| -> i64 {
        let n = tasks.iter().filter(|t| t.list == list).count();
        i64::try_from(n).unwrap_or(0) * ORDER_STEP + ORDER_STEP
    };
    let days = |n: i64| today + Duration::days(n);
    let time = |h: u32, m: u32| NaiveTime::from_hms_opt(h, m, 0);
    let created = now - Duration::days(14);

    let make = |list: &str, title: &str, tasks: &mut Vec<Task>, id: String| -> usize {
        let mut t = Task::new(id, list.to_string(), title.to_string(), created);
        t.order = next_order(list, tasks);
        tasks.push(t);
        tasks.len() - 1
    };
    let steps = |titles: &[(&str, bool)]| -> Vec<Subtask> {
        titles
            .iter()
            .enumerate()
            .map(|(n, (title, done))| Subtask {
                id: format!("s{}", n + 1),
                title: (*title).to_string(),
                done: *done,
            })
            .collect()
    };

    // ---- Work
    let i = make(&work, "Submit expenses", &mut tasks, new_id());
    tasks[i].due = Some(days(-3));
    tasks[i].priority = Priority::Medium;
    tasks[i].notes = "Receipts are in the Finance folder.".into();

    let i = make(&work, "Prepare offsite slides", &mut tasks, new_id());
    tasks[i].due = Some(today);
    tasks[i].due_time = time(14, 0);
    tasks[i].flagged = true;
    tasks[i].add_tag("offsite");
    tasks[i].notes = "Use the Q3 deck.".into();
    tasks[i].subtasks = steps(&[
        ("Outline", true),
        ("Collect numbers from Anna", true),
        ("Draft slides", false),
        ("Rehearse", false),
    ]);

    let i = make(&work, "Reply to Kai about the venue", &mut tasks, new_id());
    tasks[i].due = Some(days(1));
    tasks[i].due_time = time(10, 0);
    tasks[i].reminder = Some(Reminder::Before(15));
    tasks[i].add_tag("offsite");

    let i = make(&work, "Quarterly report", &mut tasks, new_id());
    tasks[i].due = Some(days(12));
    tasks[i].priority = Priority::High;

    let i = make(&work, "Weekly status mail", &mut tasks, new_id());
    let friday = Repeat::weekly().on_weekdays(&[Weekday::Fri]);
    tasks[i].due = Some(friday.first_on_or_after(today));
    tasks[i].due_time = time(16, 0);
    tasks[i].repeat = Some(friday);

    make(&work, "Book a team lunch", &mut tasks, new_id());

    let i = make(&work, "Send the September invoice", &mut tasks, new_id());
    tasks[i].due = Some(days(-1));
    tasks[i].completed = Some(now - Duration::days(1));

    // ---- Groceries
    for title in ["Milk", "Eggs", "Coffee beans"] {
        let i = make(&groceries, title, &mut tasks, new_id());
        tasks[i].add_tag("errand");
    }
    let i = make(&groceries, "Bread", &mut tasks, new_id());
    tasks[i].completed = Some(now - Duration::hours(2));

    // ---- Household
    let i = make(&household, "Water the plants", &mut tasks, new_id());
    tasks[i].due = Some(days(2));
    tasks[i].repeat = Some(Repeat::weekly());

    let i = make(&household, "Pay rent", &mut tasks, new_id());
    let first = next_first(today);
    tasks[i].due = Some(first);
    tasks[i].due_time = time(9, 0);
    tasks[i].repeat = Some(Repeat::monthly().on_month_day(1));
    tasks[i].priority = Priority::High;
    tasks[i].reminder = Some(Reminder::Before(60));
    tasks[i].add_tag("home");

    let i = make(&household, "Renew passport", &mut tasks, new_id());
    tasks[i].due = Some(days(20));
    tasks[i].notes = "Photos from the shop on Main Street; the old passport goes with it.".into();

    let i = make(&household, "Take out the bins", &mut tasks, new_id());
    let tuesday = Repeat::weekly().on_weekdays(&[Weekday::Tue]);
    tasks[i].due = Some(tuesday.first_on_or_after(days(1)));
    tasks[i].repeat = Some(tuesday);
    tasks[i].add_tag("home");

    let i = make(&household, "Call the dentist", &mut tasks, new_id());
    tasks[i].due = Some(today);
    tasks[i].reminder = Some(Reminder::At(now - Duration::minutes(1)));
    tasks[i].add_tag("home");

    let i = make(&household, "Take vitamins", &mut tasks, new_id());
    tasks[i].due = Some(today);
    tasks[i].due_time = time(8, 0);
    tasks[i].repeat = Some(Repeat::daily());

    // ---- Reading
    let i = make(&reading, "Finish The Pragmatic Programmer", &mut tasks, new_id());
    tasks[i].subtasks = steps(&[("Chapters 1-3", true), ("Chapters 4-6", false), ("Chapters 7-8", false)]);
    let i = make(&reading, "Read the azul layout notes", &mut tasks, new_id());
    tasks[i].add_tag("azul");

    // ---- Azlin launch: Design
    let i = make(&design, "Logo variants", &mut tasks, new_id());
    tasks[i].due = Some(days(4));
    tasks[i].flagged = true;
    tasks[i].subtasks = steps(&[("Sketches", true), ("Dark background", false), ("App icon", false)]);
    let i = make(&design, "Pick the accent colour", &mut tasks, new_id());
    tasks[i].completed = Some(now - Duration::days(2));

    // ---- Azlin launch: Website
    let i = make(&website, "Landing page copy", &mut tasks, new_id());
    tasks[i].due = Some(days(6));
    tasks[i].priority = Priority::Medium;
    let i = make(&website, "Pricing page", &mut tasks, new_id());
    tasks[i].due = Some(days(9));
    let i = make(&website, "Set up the domain", &mut tasks, new_id());
    tasks[i].due = Some(days(-1));
    tasks[i].flagged = true;
    tasks[i].priority = Priority::High;

    (lists, tasks)
}

/// The 1st of next month.
fn next_first(today: NaiveDate) -> NaiveDate {
    azul_pim::dates::add_months_clamped(today, 1, 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        model::{task_from_json, task_to_json},
        views::{self, Smart},
    };

    fn now() -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 10, 1)
            .unwrap()
            .and_hms_opt(10, 0, 0)
            .unwrap()
    }

    fn make() -> (Vec<TaskList>, Vec<Task>) {
        let mut n = 0;
        let mut id = || {
            n += 1;
            format!("id-{n}")
        };
        sample(now(), &mut id)
    }

    #[test]
    fn the_sample_fills_every_smart_list() {
        let (_, tasks) = make();
        let today = now().date();
        for s in Smart::ALL {
            assert!(views::smart_count(s, &tasks, today) > 0, "{s:?} is empty");
        }
        assert!(tasks.iter().any(|t| views::is_overdue(t, now())));
        assert!(tasks.iter().any(|t| t.repeat.is_some()));
        assert!(tasks.iter().any(|t| t.subtask_progress().is_some()));
        assert!(tasks.iter().any(|t| !t.notes.is_empty()));
        assert!(tasks.iter().any(|t| !t.tags.is_empty()));
    }

    #[test]
    fn every_sample_task_sits_in_a_sample_list_and_ids_are_unique() {
        let (lists, tasks) = make();
        assert!(lists.iter().filter(|l| l.group == "Azlin launch").count() == 2);
        let mut ids: Vec<&str> = lists
            .iter()
            .map(|l| l.id.as_str())
            .chain(tasks.iter().map(|t| t.id.as_str()))
            .collect();
        let n = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), n);
        for t in &tasks {
            assert!(lists.iter().any(|l| l.id == t.list), "{}", t.title);
        }
    }

    #[test]
    fn one_sample_reminder_is_due_at_once() {
        let (_, tasks) = make();
        let due = crate::reminders::due_now(&tasks, now(), NaiveTime::from_hms_opt(9, 0, 0).unwrap());
        let titles: Vec<&str> = due.iter().map(|&i| tasks[i].title.as_str()).collect();
        assert_eq!(titles, vec!["Call the dentist"]);
    }

    #[test]
    fn the_sample_round_trips_through_its_files() {
        let (_, tasks) = make();
        for t in &tasks {
            assert_eq!(task_from_json(&task_to_json(t)).as_ref(), Ok(t));
        }
    }
}
