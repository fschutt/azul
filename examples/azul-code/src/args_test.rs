use super::*;

#[test]
fn folder_and_shell_are_azcodes_and_the_rest_is_appkits() {
    let a = Args::parse(["--folder", "/tmp/project", "--shell=/bin/sh", "--mode", "dark"]).expect("valid");
    assert_eq!(a.folder, Some(PathBuf::from("/tmp/project")));
    assert_eq!(a.shell, Some(("/bin/sh".to_string(), Vec::new())));
    assert!(a.kit.mode.is_some(), "--mode is appkit's");
    let b = Args::parse(["--shell", "bash --noprofile -i", "notes"]).expect("valid");
    assert_eq!(
        b.shell,
        Some(("bash".to_string(), vec!["--noprofile".to_string(), "-i".to_string()]))
    );
    assert_eq!(b.kit.files, vec![PathBuf::from("notes")], "a bare folder is still a file to open");
    assert!(Args::parse(["--folder"]).is_err(), "a switch without its value");
    assert!(Args::parse(["--shell", "  "]).is_err());
    assert!(Args::parse(["--nonsense"]).is_err());
    let usage = Args::parse(["-h"]).expect_err("the usage");
    assert!(usage.contains("--folder <DIR>") && usage.contains("--shell <PROGRAM>") && usage.contains("--theme"));
}
