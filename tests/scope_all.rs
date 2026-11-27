use std::path::Path;
use std::process::{Command, Output};

// Runs munin from `/`, a directory that names no project, with a database in
// a temporary directory and no MUNIN_* variables from the caller.
fn run_from_root(db: &Path, home: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_munin"))
        .args(args)
        .current_dir("/")
        .env("HOME", home)
        .env("MUNIN_DB", db)
        .env_remove("MUNIN_PROJECT")
        .env_remove("MUNIN_HALF_LIFE_DAYS")
        .output()
        .unwrap()
}

// Check that scope all works from a directory that names no project, and that
// scopes relative to a project still need one there.
#[test]
fn scope_all_needs_no_project() {
    let tmp = tempfile::tempdir().unwrap();
    let db = tmp.path().join("munin.db");
    let add = run_from_root(
        &db,
        tmp.path(),
        &[
            "add",
            "--project",
            "scry",
            "--kind",
            "note",
            "--summary",
            "socket test",
        ],
    );
    assert!(
        add.status.success(),
        "{}",
        String::from_utf8_lossy(&add.stderr)
    );

    for args in [
        &["list", "--scope", "all"][..],
        &["search", "socket", "--scope", "all"][..],
        &["tags", "--scope", "all"][..],
    ] {
        let out = run_from_root(&db, tmp.path(), args);
        assert!(
            out.status.success(),
            "munin {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    let listed = run_from_root(&db, tmp.path(), &["list", "--scope", "all"]);
    assert!(String::from_utf8_lossy(&listed.stdout).contains("socket test"));

    let own = run_from_root(&db, tmp.path(), &["list"]);
    assert_eq!(own.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&own.stderr).contains("invalid project"));
}
