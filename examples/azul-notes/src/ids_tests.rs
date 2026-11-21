//! A new note id differs in every process: it names a folder in the drive
//! (an S3 bucket later) that the next launch must not overwrite.
//! `azul::uuid::Uuid::v4` is a process-local counter - the same sequence in
//! every run - so the first note of every launch got the same id
//! (DEDUP_OFFICE D7 / DEDUP_EDITORS, 2026-10-02).

const CHILD: &str = "AZ_IDS_TEST_PRINT_ONE";

/// The first id a freshly started process mints: this test binary run again
/// in its print-one mode.
fn id_from_a_fresh_process() -> String {
    let out = std::process::Command::new(std::env::current_exe().expect("the test binary"))
        .args(["a_new_note_id_differs_in_every_process", "--nocapture", "--test-threads=1"])
        .env(CHILD, "1")
        .output()
        .expect("the test binary runs");
    String::from_utf8_lossy(&out.stdout)
        .lines()
        // libtest prints "test <name> ... " without a newline, so the id
        // lands mid-line.
        .find_map(|l| l.split("ID=").nth(1).map(|id| id.split_whitespace().next().unwrap_or("").to_string()))
        .expect("the child prints an id")
}

#[test]
fn a_new_note_id_differs_in_every_process() {
    if std::env::var_os(CHILD).is_some() {
        println!("ID={}", crate::jobs::new_note_id());
        return;
    }
    let (a, b) = (id_from_a_fresh_process(), id_from_a_fresh_process());
    assert_ne!(a, b, "two launches minted the same first note id: {a}");
}
