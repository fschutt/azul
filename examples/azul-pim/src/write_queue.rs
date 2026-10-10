//! The write-behind queue every PIM app's durable writes go through (AzTasks wrote it; AzCalendar
//! takes it too): callbacks queue a write of a key, a file thread runs the queue as a batch
//! against an `azul-storage` [`Drive`] (a `LocalDrive` today, the user's `S3Drive` later).
//!
//! The queue keeps one write per key (a newer write of a key replaces the older one) and hands
//! out one batch at a time: the batch in flight finishes before the next starts, so two writes
//! of one file can never land out of order. A write that fails is kept aside for a retry (a
//! status bar's sync button), unless a newer write of its key came since.
//!
//! [`run_batch`] blocks: the apps call it on an azul `Thread`, never in a callback.

use azul_storage::Drive;

/// A write the queue holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Write {
    Put { key: String, bytes: Vec<u8> },
    Delete { key: String },
}

impl Write {
    #[must_use]
    pub fn key(&self) -> &str {
        match self {
            Write::Put { key, .. } | Write::Delete { key } => key,
        }
    }
}

/// The writes waiting, the batch in flight and the failures kept for a retry.
#[derive(Debug, Clone, Default)]
pub struct WriteQueue {
    pending: Vec<Write>,
    failed: Vec<(Write, String)>,
    in_flight: usize,
}

impl WriteQueue {
    #[must_use]
    pub fn new() -> Self {
        WriteQueue::default()
    }

    /// Writes `bytes` to `key` (replacing a waiting write of `key`).
    pub fn put(&mut self, key: String, bytes: Vec<u8>) {
        self.replace(Write::Put { key, bytes });
    }

    /// Deletes `key` (replacing a waiting write of `key`).
    pub fn delete(&mut self, key: String) {
        self.replace(Write::Delete { key });
    }

    fn replace(&mut self, write: Write) {
        self.pending.retain(|w| w.key() != write.key());
        self.failed.retain(|(w, _)| w.key() != write.key());
        self.pending.push(write);
    }

    /// The next batch: everything waiting, unless a batch is still in flight.
    pub fn take(&mut self) -> Option<Vec<Write>> {
        if self.in_flight > 0 || self.pending.is_empty() {
            return None;
        }
        let batch = std::mem::take(&mut self.pending);
        self.in_flight = batch.len();
        Some(batch)
    }

    /// The batch in flight is done; `failed` are its writes that did not land, with why.
    pub fn finish(&mut self, failed: Vec<(Write, String)>) {
        self.in_flight = 0;
        for (write, why) in failed {
            let superseded = self.pending.iter().any(|w| w.key() == write.key());
            if !superseded {
                self.failed.retain(|(w, _)| w.key() != write.key());
                self.failed.push((write, why));
            }
        }
    }

    /// Queues the failed writes again (before anything newer).
    pub fn retry(&mut self) {
        let failed = std::mem::take(&mut self.failed);
        let mut again: Vec<Write> = failed
            .into_iter()
            .map(|(w, _)| w)
            .filter(|w| !self.pending.iter().any(|p| p.key() == w.key()))
            .collect();
        again.append(&mut self.pending);
        self.pending = again;
    }

    /// Writes waiting (not counting the batch in flight).
    #[must_use]
    pub fn pending(&self) -> usize {
        self.pending.len()
    }

    /// Writes of the batch in flight.
    #[must_use]
    pub fn in_flight(&self) -> usize {
        self.in_flight
    }

    /// The failures kept for a retry: `(key, why)`.
    #[must_use]
    pub fn failures(&self) -> Vec<(String, String)> {
        self.failed
            .iter()
            .map(|(w, why)| (w.key().to_string(), why.clone()))
            .collect()
    }

    /// Nothing waiting, nothing in flight.
    #[must_use]
    pub fn is_idle(&self) -> bool {
        self.in_flight == 0 && self.pending.is_empty()
    }
}

/// What a batch did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BatchResult {
    /// The keys written or deleted.
    pub done: Vec<String>,
    /// The writes that failed, with why.
    pub failed: Vec<(Write, String)>,
}

/// Runs a batch against the drive, in order.
pub fn run_batch(drive: &dyn Drive, batch: Vec<Write>) -> BatchResult {
    let mut out = BatchResult::default();
    for write in batch {
        let result = match &write {
            Write::Put { key, bytes } => drive.put(key, bytes),
            Write::Delete { key } => drive.delete(key),
        };
        match result {
            Ok(()) => out.done.push(write.key().to_string()),
            Err(e) => out.failed.push((write, e.to_string())),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use azul_storage::LocalDrive;

    use super::*;
    use crate::testing::TempDir;

    #[test]
    fn the_write_queue_keeps_only_the_last_write_of_a_key() {
        let mut q = WriteQueue::new();
        q.put("tasks/a/1.json".into(), b"one".to_vec());
        q.put("tasks/a/2.json".into(), b"two".to_vec());
        q.put("tasks/a/1.json".into(), b"three".to_vec());
        q.delete("tasks/a/2.json".into());
        assert_eq!(q.pending(), 2);
        let batch = q.take().unwrap();
        assert_eq!(
            batch,
            vec![
                Write::Put {
                    key: "tasks/a/1.json".into(),
                    bytes: b"three".to_vec()
                },
                Write::Delete {
                    key: "tasks/a/2.json".into()
                }
            ]
        );
    }

    #[test]
    fn the_write_queue_sends_one_batch_at_a_time_and_keeps_failures_for_a_retry() {
        let mut q = WriteQueue::new();
        q.put("k1".into(), b"1".to_vec());
        let first = q.take().unwrap();
        q.put("k2".into(), b"2".to_vec());
        assert_eq!(q.take(), None, "one batch in flight");
        assert_eq!(q.in_flight(), 1);
        q.finish(vec![(first[0].clone(), "disk full".into())]);
        assert_eq!(q.failures(), vec![("k1".to_string(), "disk full".to_string())]);
        assert_eq!(q.take().unwrap().len(), 1, "k2 goes; k1 waits for a retry");
        q.finish(Vec::new());
        assert!(q.is_idle());
        q.retry();
        assert_eq!(q.take().unwrap(), first);
        q.finish(Vec::new());
        assert!(q.failures().is_empty());

        // A newer write of a failed key replaces the failure.
        q.put("k3".into(), b"3".to_vec());
        let batch = q.take().unwrap();
        q.put("k3".into(), b"4".to_vec());
        q.finish(vec![(batch[0].clone(), "offline".into())]);
        assert!(q.failures().is_empty(), "superseded by the waiting write");
    }

    #[test]
    fn a_batch_writes_and_deletes_on_the_drive_in_order() {
        let dir = TempDir::create();
        let drive = LocalDrive::new(&dir.0);
        let mut q = WriteQueue::new();
        q.put("events/a.json".into(), b"{}".to_vec());
        q.put("settings.txt".into(), b"view=week\n".to_vec());
        let r = run_batch(&drive, q.take().unwrap());
        assert_eq!(r.done, vec!["events/a.json", "settings.txt"]);
        assert_eq!(drive.get("settings.txt").unwrap(), b"view=week\n");
        q.finish(r.failed);
        q.delete("events/a.json".into());
        let r = run_batch(&drive, q.take().unwrap());
        assert!(r.failed.is_empty(), "{:?}", r.failed);
        assert!(drive.get("events/a.json").is_err());
    }
}
