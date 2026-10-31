//! The operations log: every destructive step, one line each, bounded.

use azul_storage::testing::TempDir;

use crate::sync::oplog::{Op, OpKind, OpLog, MAX_BYTES};

fn op(kind: OpKind, key: &str, t: f64) -> Op {
    Op {
        t,
        op: kind,
        key: key.to_string(),
        to: None,
        hash: None,
    }
}

#[test]
fn every_step_is_one_line_read_back_in_order_with_the_harness_names() {
    let dir = TempDir::new("oplog");
    let log = OpLog::new(dir.path());
    log.record(&op(OpKind::Trash, "a.txt", 1.0)).unwrap();
    log.record(&Op {
        to: Some(String::from("n (conflict a 2026-10-24).md")),
        ..op(OpKind::Conflict, "n.md", 2.0)
    })
    .unwrap();
    log.record(&op(OpKind::Overwrite, "a.txt", 3.0)).unwrap();
    let all = log.read();
    assert_eq!(all.len(), 3);
    assert_eq!(log.of_key("a.txt").len(), 2);
    let text = std::fs::read_to_string(dir.path().join("ops.jsonl")).unwrap();
    assert!(text.lines().next().unwrap().contains("\"op\":\"trash\""), "{text}");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(dir.path().join("ops.jsonl")).unwrap().permissions().mode();
        assert_eq!(mode & 0o077, 0);
    }
}

#[test]
fn a_log_past_its_bound_keeps_its_newer_half() {
    let dir = TempDir::new("oplog");
    let log = OpLog::new(dir.path());
    let key = "k".repeat(1000);
    let n = MAX_BYTES / 1000 + 10;
    for i in 0..n {
        log.record(&op(OpKind::Delete, &key, i as f64)).unwrap();
    }
    let size = std::fs::metadata(dir.path().join("ops.jsonl")).unwrap().len();
    assert!(size <= MAX_BYTES, "{size}");
    let all = log.read();
    assert!((all.last().unwrap().t - (n - 1) as f64).abs() < f64::EPSILON, "the newest stays");
}
