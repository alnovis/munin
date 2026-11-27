use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};

// Runs munin with its own HOME, database and config file, and no MUNIN_*
// variables from the caller.
fn run(home: &Path, config: &Path, args: &[&str], stdin: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_munin"))
        .args(args)
        .current_dir(home)
        .env("HOME", home)
        .env("MUNIN_DB", home.join("munin.db"))
        .env("MUNIN_CONFIG", config)
        .env("MUNIN_PROJECT", "scry")
        .env_remove("MUNIN_HALF_LIFE_DAYS")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stdin.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

fn text(out: &Output) -> String {
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout.clone()).unwrap()
}

// Check that task formats from the config file drive the prompt hook, show in
// the brief and in `munin config`, and replace the built-in formats.
#[test]
fn configured_task_formats() {
    let tmp = tempfile::tempdir().unwrap();
    let config = tmp.path().join("config.toml");
    std::fs::write(
        &config,
        "[[task]]\npattern = '\\bTICKET-([0-9]+)\\b'\ntag = \"t-$1\"\n",
    )
    .unwrap();
    let home = tmp.path();

    text(&run(
        home,
        &config,
        &[
            "add",
            "--kind",
            "bugfix",
            "--summary",
            "[TICKET-42] cache miss",
            "--tag",
            "t-42",
        ],
        "",
    ));

    let hook = text(&run(
        home,
        &config,
        &["hook", "prompt"],
        r#"{"prompt": "back to TICKET-42, and ABC-1", "cwd": "/"}"#,
    ));
    assert!(hook.contains("t-42:"), "{hook}");
    assert!(hook.contains("[TICKET-42] cache miss"), "{hook}");

    let brief = text(&run(home, &config, &["brief"], ""));
    assert!(brief.contains("TICKET-"), "{brief}");

    let shown = text(&run(home, &config, &["config"], ""));
    assert!(shown.contains("from"), "{shown}");
    assert!(shown.contains("tag t-$1"), "{shown}");
}

#[test]
fn missing_config_uses_builtin_and_broken_config_is_reported() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path();

    let absent = home.join("absent.toml");
    let shown = text(&run(home, &absent, &["config"], ""));
    assert!(shown.contains("built in"), "{shown}");
    let brief = text(&run(home, &absent, &["brief"], ""));
    assert!(!brief.contains("Task numbers here"), "{brief}");

    let broken = home.join("broken.toml");
    std::fs::write(&broken, "[[task]]\npattern = '('\n").unwrap();
    let out = run(home, &broken, &["config"], "");
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("invalid config"));

    // A hook never fails the session, even with a broken config.
    let hook = run(home, &broken, &["hook", "prompt"], r#"{"prompt": "ABC-1"}"#);
    assert!(hook.status.success());
    assert!(hook.stdout.is_empty());
}
