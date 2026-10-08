//! `switchboard mcp` as a coding agent sees it: a spawned binary speaking JSON-RPC on
//! stdio to a live synthetic owner. No native vault or provider sign-in is read.
use serde_json::{json, Value};
use std::{
    io::{BufRead, BufReader, Write},
    path::Path,
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
    sync::Arc,
};
use switchboard_core::{AuthKind, MemoryVault, Provider};
use switchboard_runtime::{Operation, Owner};

struct Agent {
    child: Child,
    input: Option<ChildStdin>,
    output: BufReader<ChildStdout>,
    next: i64,
}
impl Agent {
    fn start(root: &Path, cwd: &Path, env: &[(&str, &str)], read_only: bool) -> Self {
        let mut command = Command::new(env!("CARGO_BIN_EXE_switchboard"));
        command
            .arg("--data-dir")
            .arg(root)
            .arg("mcp")
            .current_dir(cwd)
            .env_remove("SWITCHBOARD_SESSION")
            .env_remove("ANTHROPIC_BASE_URL")
            .env_remove("CODEX_HOME")
            .env_remove("CLAUDECODE")
            .env_remove("CLAUDE_PROJECT_DIR")
            .envs(env.iter().copied())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        if read_only {
            command.arg("--read-only");
        }
        let mut child = command.spawn().unwrap();
        let input = child.stdin.take().unwrap();
        let output = BufReader::new(child.stdout.take().unwrap());
        Self {
            child,
            input: Some(input),
            output,
            next: 1,
        }
    }
    fn send(&mut self, raw: &str) -> Value {
        self.input
            .as_mut()
            .unwrap()
            .write_all(raw.as_bytes())
            .unwrap();
        self.input.as_mut().unwrap().write_all(b"\n").unwrap();
        self.input.as_mut().unwrap().flush().unwrap();
        let mut line = String::new();
        self.output.read_line(&mut line).unwrap();
        serde_json::from_str(&line).unwrap()
    }
    fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next;
        self.next += 1;
        let reply = self
            .send(&json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}).to_string());
        assert_eq!(reply["id"], id);
        reply
    }
    fn tool(&mut self, name: &str, arguments: Value) -> (bool, Value) {
        let reply = self.request("tools/call", json!({"name": name, "arguments": arguments}));
        let result = &reply["result"];
        let error = result["isError"].as_bool().unwrap();
        let text = result["content"][0]["text"].as_str().unwrap().to_owned();
        if error {
            (true, Value::String(text))
        } else {
            assert_eq!(
                serde_json::from_str::<Value>(&text).unwrap(),
                result["structuredContent"]
            );
            (false, result["structuredContent"].clone())
        }
    }
}
impl Drop for Agent {
    /// End of input ends the server normally, so a coverage profile is written; a server
    /// that does not exit within five seconds is killed.
    fn drop(&mut self) {
        drop(self.input.take());
        for _ in 0..50 {
            if matches!(self.child.try_wait(), Ok(Some(_))) {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

async fn fixture() -> (tempfile::TempDir, tempfile::TempDir, Owner, Vec<String>) {
    let data = tempfile::tempdir().unwrap();
    let projects = tempfile::tempdir().unwrap();
    let owner = Owner::start(data.path().to_owned(), Arc::new(MemoryVault::default()))
        .await
        .unwrap();
    let mut ids = Vec::new();
    for (label, pool) in [
        ("Work A", "work"),
        ("Work B", "work"),
        ("Personal", "personal"),
    ] {
        let account = owner
            .runtime
            .execute(Operation::Add {
                label: label.into(),
                provider: Provider::Claude,
                kind: AuthKind::ApiKey,
                pool: pool.into(),
                secret: format!("fixture-mcp-secret-{}", label.replace(' ', "-")),
            })
            .await
            .unwrap();
        ids.push(account["id"].as_str().unwrap().to_owned());
    }
    owner
        .runtime
        .execute(Operation::Select {
            provider: Provider::Claude,
            pool: "work".into(),
            id: ids[0].clone(),
        })
        .await
        .unwrap();
    (data, projects, owner, ids)
}

#[tokio::test(flavor = "multi_thread")]
async fn handshake_lists_tools_and_rejects_malformed_messages() {
    let (data, projects, _owner, ids) = fixture().await;
    let mut agent = Agent::start(data.path(), projects.path(), &[], false);
    let init = agent.request("initialize", json!({"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"fixture","version":"0"}}));
    assert_eq!(init["result"]["protocolVersion"], "2025-03-26");
    assert_eq!(init["result"]["serverInfo"]["name"], "switchboard");
    assert!(init["result"]["instructions"]
        .as_str()
        .unwrap()
        .contains("global: true"));
    let unknown = agent.request("initialize", json!({"protocolVersion":"1999-01-01"}));
    assert_eq!(unknown["result"]["protocolVersion"], "2025-06-18");
    // A notification gets no answer; the next request is answered in order.
    agent
        .input
        .as_mut()
        .unwrap()
        .write_all(b"{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n")
        .unwrap();
    assert_eq!(agent.request("ping", json!({}))["result"], json!({}));
    let tools = agent.request("tools/list", json!({}));
    let names: Vec<&str> = tools["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        [
            "switchboard_status",
            "switchboard_accounts",
            "switchboard_usage",
            "switchboard_switch",
            "switchboard_project_context",
            "switchboard_project_set",
            "switchboard_project_remove",
            "switchboard_project_apply",
            "switchboard_chain_get",
            "switchboard_chain_set",
            "switchboard_openrouter_status",
            "switchboard_kimi_accounts",
            "switchboard_openrouter_model"
        ]
    );
    // SB-71: chains over MCP — set from a preset, read back, an agent that cannot take over refused.
    let (error, set) = agent.tool(
        "switchboard_chain_set",
        json!({"scope": "workflow:wf_0123456789abcdef", "preset": "subscriptions-first"}),
    );
    assert!(!error, "{set}");
    let (_, got) = agent.tool(
        "switchboard_chain_get",
        json!({"workflow_id": "wf_0123456789abcdef"}),
    );
    assert_eq!(got["effective"]["executors"][3]["agent"], "hermes");
    let (error, refused) = agent.tool(
        "switchboard_chain_set",
        json!({"executors": [{"agent": "aider"}]}),
    );
    assert!(
        error && refused.as_str().unwrap().contains("cannot take over"),
        "{refused}"
    );
    let (error, _) = agent.tool(
        "switchboard_chain_set",
        json!({"executors": [{"agent": "codex", "key": "sk-live-value"}]}),
    );
    assert!(error, "a key value is never accepted, only a vault name");
    assert_eq!(agent.send("{not json")["error"]["code"], -32700);
    assert_eq!(agent.send("[1,2]")["error"]["code"], -32600);
    assert_eq!(
        agent.request("resources/list", json!({}))["error"]["code"],
        -32601
    );
    assert_eq!(
        agent.request("tools/call", json!({"name":"switchboard_launch"}))["error"]["code"],
        -32602
    );

    let mut reader = Agent::start(data.path(), projects.path(), &[], true);
    let tools = reader.request("tools/list", json!({}));
    assert!(tools["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .all(|t| t["annotations"]["readOnlyHint"] == true));
    let read_names: Vec<&str> = tools["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert!(
        read_names.contains(&"switchboard_chain_get")
            && !read_names.contains(&"switchboard_chain_set")
    );
    assert_eq!(
        reader.request(
            "tools/call",
            json!({"name":"switchboard_switch","arguments":{"account_id":"x"}})
        )["error"]["code"],
        -32602
    );
    // A read-only server never asks the provider: `refresh` shows the stored observation and
    // says why, so no quota request, renewal or write happens on its behalf.
    let (error, usage) = reader.tool(
        "switchboard_usage",
        json!({"account_id": ids[0], "refresh": true}),
    );
    assert!(!error, "{usage}");
    assert!(
        usage["note"].as_str().unwrap().contains("read-only"),
        "{usage}"
    );
    assert_eq!(usage["accounts"][0]["known"], false);
}

#[tokio::test(flavor = "multi_thread")]
async fn managed_session_reads_usage_and_switches_only_inside_its_pool() {
    let (data, projects, owner, ids) = fixture().await;
    let mut agent = Agent::start(
        data.path(),
        projects.path(),
        &[("SWITCHBOARD_SESSION", "managed:claude:work")],
        false,
    );
    let (_, status) = agent.tool("switchboard_status", json!({}));
    assert_eq!(status["runtime"]["online"], true);
    assert_eq!(
        status["session"],
        json!({"mode":"managed","provider":"claude","pool":"work"})
    );
    assert_eq!(status["routes"]["claude:work"]["label"], "Work A");
    assert_eq!(status["rules"]["total"], 0);

    let (_, accounts) = agent.tool("switchboard_accounts", json!({}));
    let text = accounts.to_string();
    assert!(!text.contains("fixture-mcp-secret"));
    assert_eq!(accounts["accounts"].as_array().unwrap().len(), 3);
    assert_eq!(accounts["accounts"][0]["selected_for_next_request"], true);

    let (_, usage) = agent.tool("switchboard_usage", json!({"account_id": ids[0]}));
    assert_eq!(usage["accounts"][0]["known"], false);
    assert!(usage["accounts"][0]["lowest_remaining_percent"].is_null());
    let (error, message) = agent.tool("switchboard_usage", json!({"refresh": true}));
    assert!(error && message.as_str().unwrap().contains("account_id"));

    let (error, message) = agent.tool("switchboard_switch", json!({"account_id": ids[2]}));
    assert!(
        error && message.as_str().unwrap().contains("personal"),
        "{message}"
    );
    let (error, message) = agent.tool(
        "switchboard_switch",
        json!({"account_id": ids[0], "target": "claude_cli"}),
    );
    assert!(
        error && message.as_str().unwrap().contains("global: true"),
        "{message}"
    );
    let (error, _) = agent.tool("switchboard_switch", json!({"account_id": ids[1]}));
    assert!(!error);
    assert_eq!(
        owner.runtime.store.snapshot().unwrap().routes["claude:work"],
        ids[1]
    );

    let mut native = Agent::start(data.path(), projects.path(), &[("CLAUDECODE", "1")], false);
    let (error, message) = native.tool("switchboard_switch", json!({"account_id": ids[0]}));
    assert!(error && message.as_str().unwrap().contains("target route"));
    let (error, _) = native.tool(
        "switchboard_switch",
        json!({"account_id": ids[2], "target": "route"}),
    );
    assert!(!error);
    assert_eq!(
        owner.runtime.store.snapshot().unwrap().routes["claude:personal"],
        ids[2]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn project_rules_are_optional_visible_and_applied_on_request() {
    let (data, projects, owner, ids) = fixture().await;
    let alpha = projects.path().join("alpha");
    std::fs::create_dir_all(alpha.join("src")).unwrap();
    let mut agent = Agent::start(
        data.path(),
        &alpha.join("src"),
        &[("SWITCHBOARD_SESSION", "managed:claude:work")],
        false,
    );

    let (_, applied) = agent.tool("switchboard_project_apply", json!({}));
    assert_eq!(applied["results"][0]["action"], "no_rule", "{applied}");
    assert_eq!(
        owner.runtime.store.snapshot().unwrap().routes["claude:work"],
        ids[0]
    );

    let (error, message) = agent.tool(
        "switchboard_project_set",
        json!({"account_id": ids[1], "path": "relative"}),
    );
    assert!(error && message.as_str().unwrap().contains("absolute"));
    let (error, message) = agent.tool(
        "switchboard_project_set",
        json!({"account_id": ids[1], "path": alpha, "expires_in_hours": 0}),
    );
    assert!(error && message.as_str().unwrap().contains("720"));
    let (error, saved) = agent.tool(
        "switchboard_project_set",
        json!({"account_id": ids[1], "path": alpha, "expires_in_hours": 8}),
    );
    assert!(!error, "{saved}");
    assert_eq!(saved["rule"]["state"], "active");
    assert!(saved["rule"]["expires_at"].is_i64());

    let (_, status) = agent.tool("switchboard_status", json!({}));
    assert_eq!(status["rules"]["active"][0]["account"]["label"], "Work B");
    assert_eq!(
        status["rules"]["for_this_folder"]["rules"][0]["effective"]["account"]["label"],
        "Work B"
    );

    let (_, applied) = agent.tool("switchboard_project_apply", json!({}));
    assert_eq!(applied["results"][0]["action"], "selected", "{applied}");
    assert_eq!(
        owner.runtime.store.snapshot().unwrap().routes["claude:work"],
        ids[1]
    );
    let (_, applied) = agent.tool("switchboard_project_apply", json!({"path": alpha}));
    assert_eq!(
        applied["results"][0]["action"], "already_in_effect",
        "{applied}"
    );

    let (error, _) = agent.tool(
        "switchboard_project_set",
        json!({"account_id": ids[1], "path": alpha, "enabled": false}),
    );
    assert!(!error);
    let (_, context) = agent.tool("switchboard_project_context", json!({"path": alpha}));
    assert_eq!(context["rules"][0]["nearest"]["state"], "paused");
    let (_, applied) = agent.tool("switchboard_project_apply", json!({}));
    assert_eq!(applied["results"][0]["action"], "rule_paused", "{applied}");

    let (error, _) = agent.tool(
        "switchboard_project_remove",
        json!({"path": alpha, "provider": "claude"}),
    );
    assert!(!error);
    assert!(owner.runtime.store.snapshot().unwrap().rules.is_empty());
    let actions: Vec<_> = owner
        .runtime
        .store
        .snapshot()
        .unwrap()
        .events
        .iter()
        .filter(|e| e.action == "project_rule")
        .map(|e| e.detail.clone())
        .collect();
    assert_eq!(actions, ["saved", "applied", "paused", "removed"]);
}

#[test]
fn project_commands_work_offline_from_the_cli() {
    let data = tempfile::tempdir().unwrap();
    let projects = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_switchboard"))
        .arg("--data-dir")
        .arg(data.path())
        .args(["--json", "project", "list"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["data"], json!({"projects": [], "rules": []}));
    // A project with two folders, listed with its folders and no account yet, then deleted.
    let web = projects.path().join("web");
    let api = projects.path().join("api");
    std::fs::create_dir_all(&web).unwrap();
    std::fs::create_dir_all(&api).unwrap();
    let saved = Command::new(env!("CARGO_BIN_EXE_switchboard"))
        .arg("--data-dir")
        .arg(data.path())
        .args([
            "--json",
            "project",
            "save",
            "--name",
            "Alpha Web",
            "--folder",
        ])
        .arg(&web)
        .arg("--folder")
        .arg(&api)
        .output()
        .unwrap();
    assert!(
        saved.status.success(),
        "{}",
        String::from_utf8_lossy(&saved.stderr)
    );
    let output = Command::new(env!("CARGO_BIN_EXE_switchboard"))
        .arg("--data-dir")
        .arg(data.path())
        .args(["--json", "project", "list"])
        .output()
        .unwrap();
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["data"]["projects"][0]["pool"], "alpha-web");
    assert_eq!(
        value["data"]["projects"][0]["folders"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let deleted = Command::new(env!("CARGO_BIN_EXE_switchboard"))
        .arg("--data-dir")
        .arg(data.path())
        .args(["project", "delete", "--pool", "alpha-web"])
        .output()
        .unwrap();
    assert!(deleted.status.success());
    let output = Command::new(env!("CARGO_BIN_EXE_switchboard"))
        .arg("--data-dir")
        .arg(data.path())
        .args([
            "project",
            "set",
            "--account",
            "00000000-0000-4000-8000-000000000000",
            "--path",
        ])
        .arg(projects.path())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("Account not found"));
    let output = Command::new(env!("CARGO_BIN_EXE_switchboard"))
        .arg("--data-dir")
        .arg(data.path())
        .args([
            "project",
            "set",
            "--account",
            "x",
            "--expires-in-hours",
            "999",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn fallback_chains_are_set_listed_and_cleared_from_the_cli() {
    // SB-71: a preset for the machine, a task's own chain with a pinned key, the narrowest one
    // applying, and refusals for an agent that cannot take over and for a bad scope.
    let data = tempfile::tempdir().unwrap();
    let run = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_switchboard"))
            .arg("--data-dir")
            .arg(data.path())
            .args(args)
            .output()
            .unwrap()
    };
    let empty = run(&["chain", "list"]);
    assert!(empty.status.success());
    assert!(String::from_utf8_lossy(&empty.stdout).contains("No chain is set"));
    assert!(run(&["chain", "set", "--preset", "subscriptions-first"])
        .status
        .success());
    let wf = "workflow:wf_0123456789abcdef";
    let pinned = run(&[
        "chain",
        "set",
        "--scope",
        wf,
        "codex",
        "kimi-code#fabric-switchboard/prod/OPENROUTER_API_KEY",
    ]);
    assert!(
        pinned.status.success(),
        "{}",
        String::from_utf8_lossy(&pinned.stderr)
    );
    let listed = run(&[
        "--json",
        "chain",
        "list",
        "--workflow",
        "wf_0123456789abcdef",
    ]);
    let value: Value = serde_json::from_slice(&listed.stdout).unwrap();
    assert_eq!(
        value["data"]["effective"]["executors"][1]["key"],
        "fabric-switchboard/prod/OPENROUTER_API_KEY"
    );
    assert_eq!(value["data"]["chains"].as_array().unwrap().len(), 2);
    let human = String::from_utf8_lossy(&run(&["chain", "list"]).stdout).into_owned();
    assert!(
        human.contains("machine") && human.contains("claude-code → codex → kimi-code → hermes"),
        "{human}"
    );
    let aider = run(&["chain", "set", "aider"]);
    assert!(!aider.status.success());
    assert!(String::from_utf8_lossy(&aider.stderr).contains("cannot take over a workflow"));
    assert!(!run(&["chain", "set", "--scope", "everywhere", "codex"])
        .status
        .success());
    assert!(
        !run(&["chain", "set"]).status.success(),
        "an empty set is not a clear"
    );
    assert!(run(&["chain", "clear", "--scope", wf]).status.success());
    let after: Value = serde_json::from_slice(
        &run(&[
            "--json",
            "chain",
            "list",
            "--workflow",
            "wf_0123456789abcdef",
        ])
        .stdout,
    )
    .unwrap();
    assert_eq!(after["data"]["effective"]["scope"]["kind"], "machine");
}

#[test]
fn an_in_place_launch_is_refused_where_it_cannot_run() {
    // SB-75: no terminal on stdin/stdout (a test harness, a pipe) refuses before the owner is
    // asked; agent arguments need --in-place; an account id or --provider is required.
    let data = tempfile::tempdir().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let run = |args: &[&str]| {
        let output = Command::new(env!("CARGO_BIN_EXE_switchboard"))
            .arg("--data-dir")
            .arg(data.path())
            .arg(args[0])
            .arg("--working-directory")
            .arg(dir.path())
            .args(&args[1..])
            .stdin(std::process::Stdio::null())
            .output()
            .unwrap();
        (
            output.status.success(),
            String::from_utf8_lossy(&output.stderr).into_owned(),
        )
    };
    let (ok, error) = run(&["launch", "--provider", "claude", "--in-place"]);
    assert!(
        !ok && error.contains("needs a terminal on stdin and stdout"),
        "{error}"
    );
    let (ok, error) = run(&["launch", "some-id", "--", "--resume", "x"]);
    assert!(!ok && error.contains("go with --in-place"), "{error}");
    let (ok, error) = run(&["launch"]);
    assert!(
        !ok && error.contains("Give an account id or --provider"),
        "{error}"
    );
    let (ok, _) = run(&["launch", "some-id", "--provider", "claude"]);
    assert!(!ok, "an id and --provider together are refused");
}
