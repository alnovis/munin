use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{Receiver, channel};
use std::thread;
use std::time::Duration;

use serde_json::{Value, json};

const TIMEOUT: Duration = Duration::from_secs(10);

// A `munin serve` process with its own HOME, database and working directory,
// and no MUNIN_* variables from the caller, so it can never touch ~/.munin.
struct Session {
    _tmp: tempfile::TempDir,
    child: Child,
    stdin: Option<ChildStdin>,
    lines: Receiver<String>,
    next_id: i64,
}

impl Session {
    fn start() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let repo = tmp.path().join("work").join("Scry.Repo");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(repo.join(".git")).unwrap();

        let mut child = Command::new(env!("CARGO_BIN_EXE_munin"))
            .arg("serve")
            .current_dir(&repo)
            .env("HOME", &home)
            .env("MUNIN_DB", tmp.path().join("munin.db"))
            .env_remove("MUNIN_PROJECT")
            .env_remove("MUNIN_HALF_LIFE_DAYS")
            .env_remove("MUNIN_CONFIG")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();

        let stdout = child.stdout.take().unwrap();
        let (tx, lines) = channel();
        thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                if tx.send(line).is_err() {
                    break;
                }
            }
        });

        let stdin = child.stdin.take();
        let mut session = Session {
            _tmp: tmp,
            child,
            stdin,
            lines,
            next_id: 1,
        };
        let init = session.request(
            "initialize",
            json!({
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": { "name": "munin-test", "version": "0" }
            }),
        );
        assert_eq!(init["result"]["serverInfo"]["name"], "munin", "{init}");
        session.notify("notifications/initialized");
        session
    }

    fn send(&mut self, message: &Value) {
        let stdin = self.stdin.as_mut().unwrap();
        writeln!(stdin, "{message}").unwrap();
        stdin.flush().unwrap();
    }

    fn notify(&mut self, method: &str) {
        self.send(&json!({ "jsonrpc": "2.0", "method": method }));
    }

    // Sends a request and returns the response with the same id, skipping
    // notifications the server sends in between.
    fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        self.send(&json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }));
        loop {
            let line = self
                .lines
                .recv_timeout(TIMEOUT)
                .unwrap_or_else(|e| panic!("no response to {method}: {e}"));
            let message: Value = serde_json::from_str(&line).unwrap();
            if message["id"] == id {
                return message;
            }
        }
    }

    fn call(&mut self, tool: &str, arguments: Value) -> Value {
        self.request(
            "tools/call",
            json!({ "name": tool, "arguments": arguments }),
        )
    }

    // Calls a tool that must succeed and parses the JSON it returns.
    fn ok(&mut self, tool: &str, arguments: Value) -> Value {
        let response = self.call(tool, arguments);
        let result = &response["result"];
        assert_ne!(result["isError"], true, "{tool}: {response}");
        serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap()
    }

    // Calls a tool that must fail and returns the error text.
    fn err(&mut self, tool: &str, arguments: Value) -> String {
        let response = self.call(tool, arguments);
        if let Some(error) = response.get("error") {
            return error["message"].as_str().unwrap().to_owned();
        }
        let result = &response["result"];
        assert_eq!(result["isError"], true, "{tool} should fail: {response}");
        result["content"][0]["text"].as_str().unwrap().to_owned()
    }

    fn finish(mut self) {
        drop(self.stdin.take());
        for _ in 0..100 {
            if let Some(status) = self.child.try_wait().unwrap() {
                assert!(status.success(), "server exited with {status}");
                return;
            }
            thread::sleep(Duration::from_millis(50));
        }
        self.child.kill().unwrap();
        panic!("server did not exit after stdin closed");
    }
}

#[test]
fn instructions_carry_the_protocol() {
    let tmp = tempfile::tempdir().unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_munin"))
        .arg("serve")
        .current_dir(tmp.path())
        .env("HOME", tmp.path())
        .env("MUNIN_DB", tmp.path().join("munin.db"))
        .env_remove("MUNIN_PROJECT")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let init = json!({
        "jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": {
            "protocolVersion": "2025-06-18",
            "capabilities": {},
            "clientInfo": { "name": "munin-test", "version": "0" }
        }
    });
    writeln!(child.stdin.take().unwrap(), "{init}").unwrap();
    let mut line = String::new();
    BufReader::new(child.stdout.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    let response: Value = serde_json::from_str(&line).unwrap();
    assert_eq!(
        response["result"]["instructions"],
        munin::protocol::TEXT,
        "{response}"
    );
    child.wait().unwrap();
}

#[test]
fn lists_all_tools() {
    let mut s = Session::start();
    let tools = s.request("tools/list", json!({}));
    let mut names: Vec<&str> = tools["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    names.sort();
    assert_eq!(
        names,
        [
            "append_memory",
            "get_active_tags",
            "get_entry",
            "list_groups",
            "retract_memory",
            "search_memory"
        ]
    );
    s.finish();
}

// Check the whole loop an agent goes through: record, search, read,
// supersede, retract.
#[test]
fn record_search_supersede_retract() {
    let mut s = Session::start();

    let first = s.ok(
        "append_memory",
        json!({
            "kind": "decision",
            "summary": "Retry failed uploads three times",
            "body": "Measured: the fourth try never succeeded.",
            "tags": ["Upload", "retry"]
        }),
    );
    assert_eq!(first["owner"]["project"], "scry-repo");
    assert_eq!(first["tags"], json!(["upload", "retry"]));
    let first_id = first["id"].as_i64().unwrap();

    let tags = s.ok("get_active_tags", json!({}));
    assert_eq!(tags[0]["tag"], "retry", "ties are broken by name");
    assert_eq!(tags[1]["tag"], "upload");

    let cards = s.ok("search_memory", json!({ "query": "upload retry" }));
    assert_eq!(cards.as_array().unwrap().len(), 1);
    assert_eq!(cards[0]["id"], first_id);

    let second = s.ok(
        "append_memory",
        json!({
            "kind": "decision",
            "summary": "Retry failed uploads five times",
            "supersedes": first_id
        }),
    );
    let second_id = second["id"].as_i64().unwrap();

    let old = s.ok("get_entry", json!({ "id": first_id }));
    assert_eq!(old["status"], "superseded");
    assert_eq!(old["superseded_by"], second_id);

    let retracted = s.ok(
        "retract_memory",
        json!({ "id": second_id, "reason": "uploads are idempotent now" }),
    );
    assert_eq!(retracted["status"], "retracted");

    let cards = s.ok("search_memory", json!({ "query": "uploads" }));
    assert!(cards.as_array().unwrap().is_empty());
    let cards = s.ok(
        "search_memory",
        json!({ "query": "uploads", "include_inactive": true }),
    );
    assert_eq!(cards.as_array().unwrap().len(), 2);

    s.finish();
}

#[test]
fn errors_tell_the_agent_what_to_fix() {
    let mut s = Session::start();

    let err = s.err("append_memory", json!({ "kind": "note", "summary": "  " }));
    assert!(err.contains("invalid summary"), "{err}");

    let err = s.err("append_memory", json!({ "kind": "adr", "summary": "x" }));
    assert!(err.contains("decision") && err.contains("note"), "{err}");

    let err = s.err("get_entry", json!({ "id": 999 }));
    assert!(err.contains("entry 999 not found"), "{err}");

    let err = s.err(
        "search_memory",
        json!({ "query": "x", "scope": "all", "group": "g" }),
    );
    assert!(err.contains("only be given with scope group"), "{err}");

    s.finish();
}

// Check that a server whose database a newer munin migrated meanwhile stops
// serving and tells the agent to reconnect it.
#[test]
fn newer_schema_on_a_running_server() {
    let mut s = Session::start();
    s.ok("search_memory", json!({ "query": "x" }));

    rusqlite::Connection::open(s._tmp.path().join("munin.db"))
        .unwrap()
        .pragma_update(None, "user_version", 99)
        .unwrap();

    let err = s.err("search_memory", json!({ "query": "x" }));
    assert!(err.contains("schema 99"), "{err}");
    assert!(err.contains("Reconnect the munin MCP server"), "{err}");

    s.finish();
}

#[test]
fn project_override_and_groups() {
    let mut s = Session::start();
    let entry = s.ok(
        "append_memory",
        json!({ "kind": "note", "summary": "event bus", "project": "billing" }),
    );
    assert_eq!(entry["owner"]["project"], "billing");

    let groups = s.ok("list_groups", json!({ "project": "billing" }));
    assert!(groups.as_array().unwrap().is_empty());

    let err = s.err(
        "append_memory",
        json!({ "kind": "note", "summary": "x", "group": "platform" }),
    );
    assert!(err.contains("group platform not found"), "{err}");

    s.finish();
}
