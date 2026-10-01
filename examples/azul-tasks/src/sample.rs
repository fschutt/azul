//! `--sample`: lists and tasks to try the app with, dated around today so every smart list
//! has something (an overdue task, today's, the week's, later ones, flagged, repeating,
//! with steps, notes and tags, and a few completed), and one reminder that is due at once
//! so the reminder banner shows on the first start.

use chrono::{Duration, NaiveDate, NaiveDateTime, NaiveTime, Weekday};

use crate::{
    model::{ListColor, Priority, Reminder, Subtask, Task, TaskList},
    recur::{self, Repeat},
    views::ORDER_STEP,
};

/// The sample's lists and tasks at `now`; `new_id` mints every id.
pub fn sample(now: NaiveDateTime, new_id: &mut dyn FnMut() -> String) -> (Vec<TaskList>, Vec<Task>) { todo!() }

/// The 1st of next month.
fn next_first(today: NaiveDate) -> NaiveDate { todo!() }

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
