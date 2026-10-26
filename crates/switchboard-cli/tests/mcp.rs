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
            "switchboard_sessions",
            "switchboard_onboarding",
            "switchboard_project_context",
            "switchboard_projects",
            "switchboard_project_edit",
            "switchboard_codex_route",
            "switchboard_project_move_root",
            "switchboard_repository_map",
            "switchboard_statistics",
            "switchboard_events",
            "switchboard_account_update",
            "switchboard_account_move",
            "switchboard_organizations",
            "switchboard_organization_set",
            "switchboard_rotation",
            "switchboard_rotation_set",
            "switchboard_backups",
            "switchboard_backup_now",
            "switchboard_recovery",
            "switchboard_handovers",
            "switchboard_agent_set",
            "switchboard_login",
            "switchboard_project_set",
            "switchboard_project_remove",
            "switchboard_project_apply",
            "switchboard_chain_get",
            "switchboard_chain_set",
            "switchboard_ceilings",
            "switchboard_ceiling_set",
            "switchboard_ceiling_answer",
            "switchboard_openrouter_status",
            "switchboard_hermes_status",
            "switchboard_hermes_set",
            "switchboard_agents",
            "switchboard_kimi_accounts",
            "switchboard_kimi_login",
            "switchboard_kimi_rename",
            "switchboard_handovers_set",
            "switchboard_openrouter_models",
            "switchboard_project_suggestions",
            "switchboard_project_bindings",
            "switchboard_project_suggestion",
            "switchboard_project_sources",
            "switchboard_openrouter_model"
        ]
    );
    // SB-120: the agents overview reads; the default is Auto until the operator picks one.
    let (error, agents) = agent.tool("switchboard_agents", json!({}));
    assert!(!error, "{agents}");
    assert_eq!(agents["default"], Value::Null);
    assert!(agents["agents"].as_array().is_some_and(|a| a.len() > 20));
    // SB-95: the session list reads; nothing was launched here.
    let (error, sessions) = agent.tool("switchboard_sessions", json!({"all": true}));
    assert!(!error, "{sessions}");
    assert_eq!(sessions["sessions"], json!([]));
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
    // C-8: a checkout folder is passed through; a relative one is refused.
    let (error, got) = agent.tool(
        "switchboard_chain_get",
        json!({"workflow_id": "wf_0123456789abcdef", "folder": std::env::temp_dir()}),
    );
    assert!(!error, "{got}");
    assert_eq!(got["effective"]["scope"]["kind"], "workflow");
    let (error, _) = agent.tool(
        "switchboard_chain_get",
        json!({"folder": "relative/checkout"}),
    );
    assert!(error, "a relative folder is refused");
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
    assert!(
        read_names.contains(&"switchboard_ceilings")
            && !read_names.contains(&"switchboard_ceiling_set")
            && !read_names.contains(&"switchboard_ceiling_answer")
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
async fn onboarding_reports_the_steps_and_the_first_answered_call_proves_the_connection() {
    // SB-93: an agent reads where setup stands; its first answered tool call is recorded once
    // (client name and time only), and a registration in the agent's own config is seen.
    let (data, projects, _owner, _) = fixture().await;
    let home = tempfile::tempdir().unwrap();
    let proof = data.path().join("mcp-proof.json");
    let mut agent = Agent::start(
        data.path(),
        projects.path(),
        // Windows reads the home from USERPROFILE (SB-103), the rest from HOME.
        &[
            ("HOME", home.path().to_str().unwrap()),
            ("USERPROFILE", home.path().to_str().unwrap()),
        ],
        false,
    );
    agent.request(
        "initialize",
        json!({"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"fixture-agent","version":"1"}}),
    );
    assert!(!proof.exists());
    let path = projects.path().to_str().unwrap();
    let (error, first) = agent.tool(
        "switchboard_onboarding",
        json!({"path": path, "host": "claude"}),
    );
    assert!(!error, "{first}");
    let state = |answer: &Value, id: &str| {
        answer["steps"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["id"] == id)
            .unwrap()["state"]
            .clone()
    };
    assert_eq!(state(&first, "owner"), "done");
    assert_eq!(state(&first, "accounts"), "done");
    assert_eq!(state(&first, "mcp"), "todo");
    assert_eq!(
        state(&first, "proving_call"),
        "todo",
        "answered before it was recorded"
    );
    let recorded: Value = serde_json::from_slice(&std::fs::read(&proof).unwrap()).unwrap();
    assert_eq!(recorded["client"], "fixture-agent");
    assert_eq!(
        recorded.as_object().unwrap().len(),
        2,
        "client and time only"
    );

    std::fs::write(
        home.path().join(".claude.json"),
        r#"{"mcpServers":{"switchboard":{"command":"switchboard","args":["mcp"]}}}"#,
    )
    .unwrap();
    let (_, second) = agent.tool(
        "switchboard_onboarding",
        json!({"path": path, "host": "claude"}),
    );
    assert_eq!(state(&second, "mcp"), "done");
    assert_eq!(state(&second, "proving_call"), "done");
    assert_eq!(second["complete"], true, "{second}");
    let (error, refused) = agent.tool("switchboard_onboarding", json!({"host": "cursor"}));
    assert!(error, "{refused}");
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
    // SB-134: one folder taken out by delta; the other stays.
    let edited = Command::new(env!("CARGO_BIN_EXE_switchboard"))
        .arg("--data-dir")
        .arg(data.path())
        .args([
            "--json",
            "project",
            "edit",
            "--pool",
            "alpha-web",
            "--remove-folder",
        ])
        .arg(&api)
        .output()
        .unwrap();
    assert!(
        edited.status.success(),
        "{}",
        String::from_utf8_lossy(&edited.stderr)
    );
    let value: Value = serde_json::from_slice(&edited.stdout).unwrap();
    assert_eq!(
        value["data"]["folders"].as_array().unwrap().len(),
        1,
        "{value}"
    );
    let refused = Command::new(env!("CARGO_BIN_EXE_switchboard"))
        .arg("--data-dir")
        .arg(data.path())
        .args([
            "project",
            "edit",
            "--pool",
            "alpha-web",
            "--remove-account",
            "nobody",
        ])
        .output()
        .unwrap();
    assert_eq!(refused.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&refused.stderr).contains("not in the project"));
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

#[tokio::test(flavor = "multi_thread")]
async fn ceiling_answer_double_over_mcp_is_refused_with_the_window_named() {
    // C-Q1: over MCP an agent reads ceilings, sets a first or a lower one and keeps a reached
    // one; doubling, raising and clearing are the window's.
    let (data, projects, owner, _) = fixture().await;
    let mut agent = Agent::start(data.path(), projects.path(), &[], false);
    let (error, empty) = agent.tool("switchboard_ceilings", json!({}));
    assert!(!error, "{empty}");
    assert_eq!(empty["ceilings"], json!([]));
    let (error, set) = agent.tool("switchboard_ceiling_set", json!({"usd": 5}));
    assert!(!error, "{set}");
    assert_eq!(set["ceiling"]["cents"], 500);
    let (error, raised) = agent.tool("switchboard_ceiling_set", json!({"usd": 8}));
    assert!(
        error && raised.as_str().unwrap().contains("Switchboard window"),
        "{raised}"
    );
    let (error, missing) = agent.tool("switchboard_ceiling_set", json!({}));
    assert!(error, "a body without usd is never a clear: {missing}");
    let (error, lowered) = agent.tool(
        "switchboard_ceiling_set",
        json!({"scope": "machine", "usd": 4}),
    );
    assert!(!error, "{lowered}");
    let (error, bad) = agent.tool(
        "switchboard_ceiling_set",
        json!({"scope": "everywhere", "usd": 1}),
    );
    assert!(error, "{bad}");
    // The gate reached it: one question for $4.
    let snapshot = owner.runtime.store.snapshot().unwrap();
    let id = switchboard_runtime::spend::open_question(
        data.path(),
        &snapshot,
        &switchboard_core::ChainScope::Machine,
        400,
        4.2,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64,
    )
    .unwrap()
    .unwrap();
    let (_, listed) = agent.tool("switchboard_ceilings", json!({}));
    assert_eq!(listed["questions"][0]["id"], id.as_str());
    let (error, doubled) = agent.tool(
        "switchboard_ceiling_answer",
        json!({"id": id, "answer": "double"}),
    );
    assert!(
        error && doubled.as_str().unwrap().contains("Switchboard window"),
        "{doubled}"
    );
    let (_, still) = agent.tool("switchboard_ceilings", json!({}));
    assert_eq!(still["ceilings"][0]["cents"], 400, "nothing doubled");
    assert_eq!(
        still["questions"].as_array().unwrap().len(),
        1,
        "still open"
    );
    let (error, kept) = agent.tool("switchboard_ceiling_answer", json!({"id": id}));
    assert!(!error, "{kept}");
    assert_eq!(kept["answer"], "keep");
    assert_eq!(kept["ceiling"]["cents"], 400);
    let (_, after) = agent.tool("switchboard_ceilings", json!({}));
    assert_eq!(after["questions"], json!([]));
}

#[test]
fn ceilings_are_set_lowered_listed_and_kept_from_the_cli_and_never_raised() {
    // C-6: `ceiling list|set|keep` and `continue --ceiling`; raising and doubling are the
    // window's (C-Q1), so the CLI has no command for them.
    let data = tempfile::tempdir().unwrap();
    let run = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_switchboard"))
            .arg("--data-dir")
            .arg(data.path())
            .args(args)
            .output()
            .unwrap()
    };
    let out = |o: &std::process::Output| String::from_utf8_lossy(&o.stdout).into_owned();
    let err = |o: &std::process::Output| String::from_utf8_lossy(&o.stderr).into_owned();
    let empty = run(&["ceiling", "list"]);
    assert!(empty.status.success(), "{}", err(&empty));
    assert!(
        out(&empty).contains("No spending ceiling is set"),
        "{}",
        out(&empty)
    );
    let set = run(&["ceiling", "set", "5"]);
    assert!(set.status.success(), "{}", err(&set));
    assert!(
        out(&set).contains("Ceiling for machine: $5.00."),
        "{}",
        out(&set)
    );
    let raised = run(&["ceiling", "set", "8"]);
    assert!(!raised.status.success());
    assert!(
        err(&raised).contains("Switchboard window"),
        "{}",
        err(&raised)
    );
    assert!(
        run(&["ceiling", "set", "3"]).status.success(),
        "a lower one"
    );
    let no_group = run(&["ceiling", "set", "--scope", "project:nogroup", "2"]);
    assert!(!no_group.status.success());
    assert!(
        err(&no_group).contains("names no group"),
        "{}",
        err(&no_group)
    );
    let range = run(&[
        "ceiling",
        "set",
        "--scope",
        "workflow:wf_0123456789abcdef",
        "2000",
    ]);
    assert!(err(&range).contains("Enter an amount from $0.01 to $1,000."));
    let listed: Value =
        serde_json::from_slice(&run(&["--json", "ceiling", "list"]).stdout).unwrap();
    assert_eq!(listed["data"]["ceilings"][0]["usd"], 3.0);
    // A reached ceiling's question (the gate writes it at a hand-over), kept from the CLI.
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let file = json!({"version": 1, "ceilings": [{"scope": {"kind": "machine"}, "cents": 300, "set_at": now}],
        "questions": [{"id": "q_0123456789abcdef", "scope": {"kind": "machine"}, "cents": 300, "spent": 3.1, "reached_at": now}]});
    let path = data.path().join("spend.json");
    std::fs::write(&path, file.to_string()).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
    let human = out(&run(&["ceiling", "list"]));
    assert!(
        human.contains("switchboard ceiling keep q_0123456789abcdef")
            && human.contains("Double it in the Switchboard window"),
        "{human}"
    );
    let kept = run(&["ceiling", "keep", "q_0123456789abcdef"]);
    assert!(kept.status.success(), "{}", err(&kept));
    assert!(out(&kept).contains("stays $3.00"), "{}", out(&kept));
    let again = run(&["ceiling", "keep", "q_0123456789abcdef"]);
    assert!(!again.status.success(), "answered once");
    // `continue --ceiling`: the task's ceiling is set before anything is offered; an amount out
    // of range refuses the continue itself.
    let wf = "wf_0123456789abcdef";
    let dir = data.path().to_str().unwrap();
    let out_of_range = run(&["continue", wf, "--dir", dir, "--ceiling", "2000"]);
    assert!(!out_of_range.status.success());
    assert!(err(&out_of_range).contains("Enter an amount from $0.01 to $1,000."));
    let _ = run(&["continue", wf, "--dir", dir, "--ceiling", "1.5"]);
    let listed: Value =
        serde_json::from_slice(&run(&["--json", "ceiling", "list"]).stdout).unwrap();
    let task = listed["data"]["ceilings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["scope"]["kind"] == "workflow")
        .cloned()
        .unwrap();
    assert_eq!(
        (task["scope"]["id"].as_str(), task["usd"].as_f64()),
        (Some(wf), Some(1.5))
    );
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

#[tokio::test(flavor = "multi_thread")]
async fn a_project_is_listed_and_changed_by_delta_over_mcp() {
    // SB-134: moving one account into a project's pool takes one call naming it.
    let (data, projects, owner, ids) = fixture().await;
    let (a, b) = (projects.path().join("a"), projects.path().join("b"));
    std::fs::create_dir_all(&a).unwrap();
    std::fs::create_dir_all(&b).unwrap();
    owner
        .runtime
        .execute(Operation::SaveProject {
            pool: None,
            name: "Agents".into(),
            folders: vec![a.clone()],
            account_ids: vec![],
            base_folders: None,
            organization: None,
        })
        .await
        .unwrap();
    let mut agent = Agent::start(data.path(), projects.path(), &[], false);
    let (error, listed) = agent.tool("switchboard_projects", json!({}));
    assert!(!error, "{listed}");
    assert_eq!(listed["projects"][0]["name"], "Agents");
    let pool = listed["projects"][0]["pool"].as_str().unwrap().to_owned();
    let (error, changed) = agent.tool(
        "switchboard_project_edit",
        json!({"pool": pool, "add_accounts": [ids[2]], "add_folders": [b.to_string_lossy()]}),
    );
    assert!(!error, "{changed}");
    let (_, listed) = agent.tool("switchboard_projects", json!({}));
    let project = &listed["projects"][0];
    assert_eq!(project["folders"].as_array().unwrap().len(), 2);
    assert_eq!(project["accounts"][0]["id"], ids[2]);
    let (error, refused) = agent.tool(
        "switchboard_project_edit",
        json!({"pool": pool, "remove_accounts": ["not-an-account"]}),
    );
    assert!(error, "{refused}");
    let (error, refused) = agent.tool(
        "switchboard_project_edit",
        json!({"pool": pool, "add_folders": ["relative/x"]}),
    );
    assert!(error, "{refused}");
    // SB-109: a group bound to an organization by name; it refuses while it holds a personal
    // account, takes it once the account is out, and `switchboard_projects` names it.
    let (error, added) = agent.tool(
        "switchboard_organization_set",
        json!({"action": "add", "name": "Acme", "domains": ["acme.invalid"]}),
    );
    assert!(!error, "{added}");
    let (error, refused) = agent.tool(
        "switchboard_project_edit",
        json!({"pool": pool, "organization": "acme"}),
    );
    assert!(error, "{refused}");
    let (error, changed) = agent.tool(
        "switchboard_project_edit",
        json!({"pool": pool, "remove_accounts": [ids[2]], "organization": "Acme"}),
    );
    assert!(!error, "{changed}");
    let (_, listed) = agent.tool("switchboard_projects", json!({}));
    assert_eq!(
        listed["projects"][0]["organization_name"], "Acme",
        "{listed}"
    );
    let (error, changed) = agent.tool(
        "switchboard_project_edit",
        json!({"pool": pool, "organization": "personal"}),
    );
    assert!(!error, "{changed}");
    let (_, listed) = agent.tool("switchboard_projects", json!({}));
    assert!(listed["projects"][0]["organization"].is_null(), "{listed}");
    // SB-135 part 2: the disk map and moving a repository between projects.
    let (error, map) = agent.tool(
        "switchboard_repository_map",
        json!({"under": [projects.path().to_string_lossy()]}),
    );
    assert!(!error, "{map}");
    assert!(map["repositories"].is_array(), "{map}");
    let (error, refused) = agent.tool(
        "switchboard_project_move_root",
        json!({"folder": "relative/x", "pool": pool}),
    );
    assert!(error, "{refused}");
    let mut reader = Agent::start(data.path(), projects.path(), &[], true);
    let names: Vec<String> = reader.request("tools/list", json!({}))["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap().to_owned())
        .collect();
    assert!(names.contains(&"switchboard_projects".to_owned()));
    assert!(!names.contains(&"switchboard_project_edit".to_owned()));
    assert!(names.contains(&"switchboard_repository_map".to_owned()));
    assert!(!names.contains(&"switchboard_project_move_root".to_owned()));
}

#[tokio::test(flavor = "multi_thread")]
async fn accounts_rotation_activity_and_queues_are_reachable_over_mcp() {
    // SB-134 part 2: the CLI's account, rotation, activity, backup and hand-over commands have
    // MCP tools; a rotation change keeps every field it does not name.
    let (data, projects, _owner, ids) = fixture().await;
    let mut agent = Agent::start(data.path(), projects.path(), &[], false);
    let (error, renamed) = agent.tool(
        "switchboard_account_update",
        json!({"account_id": ids[2], "label": "Personal 2", "enabled": false}),
    );
    assert!(!error, "{renamed}");
    let (_, accounts) = agent.tool("switchboard_accounts", json!({}));
    let personal = accounts["accounts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["id"] == ids[2])
        .unwrap()
        .clone();
    assert_eq!(personal["label"], "Personal 2", "{accounts}");
    let (error, refused) = agent.tool("switchboard_account_update", json!({"account_id": ids[2]}));
    assert!(error, "nothing to change is refused: {refused}");
    // SB-122: an account in a pool no project uses goes back to default; elsewhere only a
    // project's pool is accepted.
    let (error, moved) = agent.tool(
        "switchboard_account_move",
        json!({"account_id": ids[2], "pool": "default"}),
    );
    assert!(!error, "{moved}");
    assert_eq!(
        (moved["from"].as_str(), moved["pool"].as_str()),
        (Some("personal"), Some("default"))
    );
    let (error, refused) = agent.tool(
        "switchboard_account_move",
        json!({"account_id": ids[2], "pool": "nowhere"}),
    );
    assert!(error, "a pool no project uses is refused: {refused}");
    // SB-158: organizations over MCP; an account on the organization's domain belongs to it,
    // the operator's word overrides, and a read-only server has no write tool.
    let (error, none) = agent.tool("switchboard_organizations", json!({}));
    assert!(!error, "{none}");
    assert_eq!(none["organizations"], json!([]));
    let (error, added) = agent.tool(
        "switchboard_organization_set",
        json!({"action": "add", "name": "Acme", "domains": ["Acme.example"]}),
    );
    assert!(!error, "{added}");
    let org = added["organization"]["id"].as_str().unwrap().to_owned();
    let (error, assigned) = agent.tool(
        "switchboard_organization_set",
        json!({"action": "assign", "account_id": ids[2], "organization": org}),
    );
    assert!(!error, "{assigned}");
    assert_eq!(assigned["by"], "person");
    let (_, accounts) = agent.tool("switchboard_accounts", json!({}));
    let mine = accounts["accounts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["id"] == ids[2])
        .unwrap()
        .clone();
    assert_eq!(mine["organization"]["name"], "Acme", "{accounts}");
    let (error, refused) = agent.tool(
        "switchboard_organization_set",
        json!({"action": "add", "name": "acme"}),
    );
    assert!(
        error,
        "a second organization of the same name is refused: {refused}"
    );
    let (error, personal) = agent.tool(
        "switchboard_organization_set",
        json!({"action": "assign", "account_id": ids[2], "organization": "personal"}),
    );
    assert!(!error && personal["organization"].is_null(), "{personal}");
    let (error, set) = agent.tool(
        "switchboard_rotation_set",
        json!({"provider": "claude", "pool": "work", "threshold_percent": 80}),
    );
    assert!(!error, "{set}");
    let (_, again) = agent.tool(
        "switchboard_rotation_set",
        json!({"provider": "claude", "pool": "work", "enabled": true}),
    );
    let (_, rotation) = agent.tool("switchboard_rotation", json!({}));
    let policy = rotation["policies"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["pool"] == "work")
        .unwrap()
        .clone();
    assert_eq!(policy["threshold_percent"], 80.0, "{again}");
    assert_eq!(policy["enabled"], true);
    // SB-145: nothing relayed yet is empty tables, not an error; a range outside 1-90 is refused.
    let (error, stats) = agent.tool("switchboard_statistics", json!({"days": 7}));
    assert!(
        !error && stats["days"] == 7 && stats["total"]["requests"] == 0,
        "{stats}"
    );
    let (error, stats) = agent.tool("switchboard_statistics", json!({"days": 365}));
    assert!(error, "{stats}");
    let (error, events) = agent.tool("switchboard_events", json!({"limit": 5}));
    assert!(!error && events["events"].as_array().is_some(), "{events}");
    // SB-134 part 3: the last CLI commands over MCP.
    let (error, found) = agent.tool("switchboard_project_suggestions", json!({}));
    assert!(!error && found["suggestions"].is_array(), "{found}");
    let (error, bindings) = agent.tool("switchboard_project_bindings", json!({}));
    assert!(!error, "{bindings}");
    let (error, sources) = agent.tool("switchboard_project_sources", json!({"pool": "nope"}));
    assert!(error, "{sources}");
    let (error, refused) = agent.tool(
        "switchboard_project_suggestion",
        json!({"action": "nope", "folder": "/x"}),
    );
    assert!(error, "{refused}");
    let (error, kimi) = agent.tool("switchboard_kimi_login", json!({"action": "nope"}));
    assert!(error, "{kimi}");
    let (error, handovers) = agent.tool("switchboard_handovers", json!({}));
    assert!(!error && handovers["handovers"] == json!([]), "{handovers}");
    let (error, backups) = agent.tool("switchboard_backups", json!({}));
    assert!(!error, "{backups}");
    let (error, login) = agent.tool("switchboard_login", json!({"action": "nope"}));
    assert!(error, "{login}");
}
