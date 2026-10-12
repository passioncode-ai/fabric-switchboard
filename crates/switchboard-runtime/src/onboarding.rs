//! `switchboard onboard` (SB-93): where a coding agent stands in setting Switchboard up, as steps
//! it can act on. The agent does every step it can; a step only a person can take (opening the
//! app, signing in) says so and names what to tell them. Re-running is safe: every step is read
//! from the machine, nothing here writes.
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

/// The guide a coding agent follows, printed by `switchboard onboard`.
pub const GUIDE: &str = include_str!("../../../docs/AGENT-ONBOARDING.md");
/// Written by `switchboard mcp` after its first answered tool call: the proving call.
pub const PROOF_FILE: &str = "mcp-proof.json";
/// The longest client name kept from an MCP `initialize`.
const CLIENT_NAME_MAX: usize = 64;

/// The coding agent being set up: whose MCP registration is checked and how to add it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Host {
    Claude,
    Codex,
}
impl Host {
    pub fn name(self) -> &'static str {
        match self {
            Host::Claude => "claude",
            Host::Codex => "codex",
        }
    }
    /// The agent this command runs inside, from the variables each one sets for its tools.
    pub fn detect(var: impl Fn(&str) -> Option<String>) -> Option<Host> {
        if var("CLAUDECODE").as_deref() == Some("1") {
            Some(Host::Claude)
        } else if var("CODEX_SANDBOX").is_some() || var("CODEX_THREAD_ID").is_some() {
            Some(Host::Codex)
        } else {
            None
        }
    }
}

/// What was read from the machine; `steps` turns it into the answer.
#[derive(Debug, Default)]
pub struct Probe {
    /// The desktop app or `switchboard serve` answers on the control channel.
    pub owner_online: bool,
    /// Saved accounts that are switched on, per provider name.
    pub accounts: Vec<(String, usize)>,
    pub folder: Option<PathBuf>,
    /// The folder's project (`Operation::ResolveProject`'s `project`), when it has one.
    pub project: Option<Value>,
    pub host: Option<Host>,
    /// Per host checked: whether its configuration names a `switchboard` MCP server.
    pub registered: Vec<(Host, bool)>,
    /// The client that made the proving call and when (`mcp-proof.json`).
    pub proof: Option<Value>,
    /// The `switchboard` executable to register: an absolute path when one was found.
    pub cli: Option<PathBuf>,
}

fn step(id: &str, state: &str, summary: String, next: Value) -> Value {
    json!({"id": id, "state": state, "summary": summary, "next": next})
}
fn shell_word(value: &str) -> String {
    if !value.is_empty()
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "/._-+:@".contains(c))
    {
        value.to_owned()
    } else {
        format!("'{}'", value.replace('\'', "'\\''"))
    }
}

/// The steps, in order. States: `done`; `todo` — the agent runs `next.command`;
/// `needs_person` — only the person can (`next.person` says what to ask, `next.command` what the
/// agent may run to start it); `optional` — not needed to finish.
pub fn steps(probe: &Probe) -> Value {
    let cli = probe
        .cli
        .as_ref()
        .map(|p| shell_word(&p.to_string_lossy()))
        .unwrap_or_else(|| "switchboard".into());
    let mut steps = Vec::new();

    steps.push(if probe.owner_online {
        step("owner", "done", "Switchboard is running.".into(), Value::Null)
    } else {
        step(
            "owner",
            "needs_person",
            "Switchboard is not running: sign-ins, launches and switching need it.".into(),
            json!({"person": "Open Fabric Switchboard (it stays in the menu bar), or allow running `switchboard serve` in a separate terminal."}),
        )
    });

    let total: usize = probe.accounts.iter().map(|(_, n)| n).sum();
    steps.push(if total > 0 {
        let list = probe
            .accounts
            .iter()
            .filter(|(_, n)| *n > 0)
            .map(|(p, n)| format!("{p} {n}"))
            .collect::<Vec<_>>()
            .join(", ");
        step("accounts", "done", format!("Saved accounts: {list}."), Value::Null)
    } else {
        let provider = match probe.host {
            Some(Host::Codex) => "codex",
            _ => "claude",
        };
        step(
            "accounts",
            "needs_person",
            "No account is saved yet. Signing in is the person's step.".into(),
            json!({
                "command": format!("{cli} login begin --provider {provider}"),
                "person": "Finish the sign-in in the Terminal window that opens. To keep the account already signed in to the ordinary CLI instead, say so: the agent then runs `switchboard accounts capture`.",
            }),
        )
    });

    steps.push(match (&probe.folder, &probe.project) {
        (Some(folder), Some(project)) => step(
            "project",
            "done",
            format!(
                "{} belongs to the project “{}”.",
                folder.display(),
                project["name"].as_str().unwrap_or("—")
            ),
            Value::Null,
        ),
        (Some(folder), None) => {
            let name = folder
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "project".into());
            step(
                "project",
                "optional",
                format!(
                    "{} belongs to no project: its sessions use the default pool. A project reserves accounts for its folders.",
                    folder.display()
                ),
                json!({
                    "command": format!(
                        "{cli} project save --name {} --folder {} --account <id>",
                        shell_word(&name),
                        shell_word(&folder.to_string_lossy())
                    ),
                    "person": "Only if the person wants accounts kept for this project: ask which ones (`switchboard accounts list`). A project with no account of a provider cannot start that provider's sessions in its folders.",
                }),
            )
        }
        (None, _) => step(
            "project",
            "optional",
            "No folder given.".into(),
            json!({"command": format!("{cli} onboard --status --folder <absolute folder>")}),
        ),
    });

    let registered = |host: Host| {
        probe
            .registered
            .iter()
            .any(|(h, found)| *h == host && *found)
    };
    let mcp_command = |host: Host| match host {
        Host::Claude => format!("claude mcp add --scope user switchboard -- {cli} mcp"),
        Host::Codex => format!("codex mcp add switchboard -- {cli} mcp"),
    };
    steps.push(match probe.host {
        Some(host) if registered(host) => step(
            "mcp",
            "done",
            format!(
                "The switchboard MCP server is registered with {}.",
                host.name()
            ),
            Value::Null,
        ),
        Some(host) => step(
            "mcp",
            "todo",
            format!(
                "The switchboard MCP server is not registered with {}.",
                host.name()
            ),
            json!({"command": mcp_command(host)}),
        ),
        // Run outside a known agent: any registration counts, else the agent names itself.
        None => {
            let found: Vec<&str> = probe
                .registered
                .iter()
                .filter(|(_, found)| *found)
                .map(|(h, _)| h.name())
                .collect();
            if found.is_empty() {
                step(
                    "mcp",
                    "todo",
                    "The coding agent is unknown: pass --host claude or --host codex.".into(),
                    json!({"command": format!("{cli} onboard --status --host claude")}),
                )
            } else {
                step(
                    "mcp",
                    "done",
                    format!(
                        "The switchboard MCP server is registered with {}.",
                        found.join(", ")
                    ),
                    Value::Null,
                )
            }
        }
    });

    steps.push(match &probe.proof {
        Some(proof) => step(
            "proving_call",
            "done",
            format!(
                "A tool call was answered{}.",
                proof["client"]
                    .as_str()
                    .map(|c| format!(" for {c}"))
                    .unwrap_or_default()
            ),
            Value::Null,
        ),
        None => step(
            "proving_call",
            "todo",
            "No Switchboard tool has answered a coding agent yet.".into(),
            json!({"tool": "switchboard_accounts", "person": "A server added during a session loads in the next one: start a new session if the tool is not listed."}),
        ),
    });

    let complete = steps
        .iter()
        .all(|s| s["state"] == "done" || s["state"] == "optional");
    json!({
        "complete": complete,
        "host": probe.host.map(Host::name),
        "folder": probe.folder.as_ref().map(|f| f.to_string_lossy().into_owned()),
        "steps": steps,
    })
}

/// Whether the agent's configuration names a `switchboard` MCP server for `folder`. Reads only
/// server names; the files may hold other servers' settings, never echoed.
pub fn registered(
    host: Host,
    home: &Path,
    folder: Option<&Path>,
    codex_home: Option<&Path>,
) -> bool {
    let has = |servers: &Value| servers.get("switchboard").is_some_and(Value::is_object);
    match host {
        Host::Claude => {
            // Claude Code rewrites ~/.claude.json often; a half-written copy reads as absent.
            let user = read_json(&home.join(".claude.json"), 32 * 1024 * 1024);
            if let Some(user) = &user {
                if has(&user["mcpServers"]) {
                    return true;
                }
                if let Some(folder) = folder {
                    // Claude Code keys a folder by its path; on Windows it writes forward slashes.
                    let native = folder.to_string_lossy();
                    let forward = native.replace('\\', "/");
                    if [native.as_ref(), forward.as_str()]
                        .iter()
                        .any(|key| has(&user["projects"][*key]["mcpServers"]))
                    {
                        return true;
                    }
                }
            }
            if let Some(project) = folder.and_then(|f| read_json(&f.join(".mcp.json"), 1024 * 1024))
            {
                if has(&project["mcpServers"]) {
                    return true;
                }
            }
            // The Switchboard plugin brings the server with it.
            read_json(&home.join(".claude/settings.json"), 1024 * 1024).is_some_and(|settings| {
                settings["enabledPlugins"]
                    .as_object()
                    .is_some_and(|plugins| {
                        plugins.iter().any(|(name, on)| {
                            name.starts_with("switchboard@") && on == &Value::Bool(true)
                        })
                    })
            })
        }
        Host::Codex => {
            let home = codex_home
                .map(Path::to_path_buf)
                .unwrap_or_else(|| home.join(".codex"));
            std::fs::read_to_string(home.join("config.toml"))
                .ok()
                .and_then(|text| text.parse::<toml::Table>().ok())
                .is_some_and(|config| {
                    config
                        .get("mcp_servers")
                        .and_then(|s| s.get("switchboard"))
                        .is_some_and(toml::Value::is_table)
                })
        }
    }
}
fn read_json(path: &Path, max: u64) -> Option<Value> {
    use std::io::Read;
    let file = std::fs::File::open(path).ok()?;
    let mut bytes = Vec::new();
    file.take(max + 1).read_to_end(&mut bytes).ok()?;
    if bytes.len() as u64 > max {
        return None;
    }
    serde_json::from_slice(&bytes).ok()
}

/// The proving call, if one was recorded.
pub fn proof(root: &Path) -> Option<Value> {
    read_json(&root.join(PROOF_FILE), 4096).filter(Value::is_object)
}
/// Records the proving call: the client's name (printable, shortened) and the time. Only into a
/// data folder that exists — a tool call never creates Switchboard's storage.
pub fn record_proof(root: &Path, client: Option<&str>, at: i64) {
    if !root.is_dir() {
        return;
    }
    let client: Option<String> = client.map(|c| {
        c.chars()
            .filter(|c| c.is_ascii_alphanumeric() || " ._-".contains(*c))
            .take(CLIENT_NAME_MAX)
            .collect()
    });
    let body = json!({"client": client.filter(|c| !c.is_empty()), "at": at});
    let _ = switchboard_core::private_fs::private_write(
        &root.join(PROOF_FILE),
        body.to_string().as_bytes(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn by_id<'a>(answer: &'a Value, id: &str) -> &'a Value {
        answer["steps"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["id"] == id)
            .unwrap()
    }

    #[test]
    fn a_fresh_machine_names_the_persons_steps_and_the_agents_commands() {
        let answer = steps(&Probe {
            folder: Some(PathBuf::from("/work/my app")),
            host: Some(Host::Claude),
            registered: vec![(Host::Claude, false)],
            cli: Some(PathBuf::from("/Users/x/.local/bin/switchboard")),
            ..Probe::default()
        });
        assert_eq!(answer["complete"], false);
        assert_eq!(by_id(&answer, "owner")["state"], "needs_person");
        let accounts = by_id(&answer, "accounts");
        assert_eq!(accounts["state"], "needs_person");
        assert_eq!(
            accounts["next"]["command"],
            "/Users/x/.local/bin/switchboard login begin --provider claude"
        );
        let project = by_id(&answer, "project");
        assert_eq!(project["state"], "optional");
        assert_eq!(
            project["next"]["command"],
            "/Users/x/.local/bin/switchboard project save --name 'my app' --folder '/work/my app' --account <id>"
        );
        let mcp = by_id(&answer, "mcp");
        assert_eq!(mcp["state"], "todo");
        assert_eq!(
            mcp["next"]["command"],
            "claude mcp add --scope user switchboard -- /Users/x/.local/bin/switchboard mcp"
        );
        assert_eq!(by_id(&answer, "proving_call")["state"], "todo");
        let order: Vec<&str> = answer["steps"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s["id"].as_str().unwrap())
            .collect();
        assert_eq!(
            order,
            ["owner", "accounts", "project", "mcp", "proving_call"]
        );
    }

    #[test]
    fn a_set_up_machine_is_complete_and_a_project_is_never_required() {
        let mut probe = Probe {
            owner_online: true,
            accounts: vec![("claude".into(), 2), ("codex".into(), 0)],
            folder: Some(PathBuf::from("/work/app")),
            host: Some(Host::Codex),
            registered: vec![(Host::Codex, true)],
            proof: Some(json!({"client": "codex-mcp-client", "at": 1})),
            ..Probe::default()
        };
        let answer = steps(&probe);
        assert_eq!(answer["complete"], true, "{answer}");
        assert_eq!(
            by_id(&answer, "accounts")["summary"],
            "Saved accounts: claude 2."
        );
        assert_eq!(by_id(&answer, "project")["state"], "optional");
        probe.project = Some(json!({"name": "App"}));
        assert_eq!(by_id(&steps(&probe), "project")["state"], "done");
    }

    #[test]
    fn an_unknown_agent_counts_any_registration_and_asks_for_the_host_without_one() {
        let probe = Probe {
            registered: vec![(Host::Claude, false), (Host::Codex, true)],
            ..Probe::default()
        };
        assert_eq!(by_id(&steps(&probe), "mcp")["state"], "done");
        let probe = Probe {
            registered: vec![(Host::Claude, false), (Host::Codex, false)],
            ..Probe::default()
        };
        let mcp = steps(&probe);
        let mcp = by_id(&mcp, "mcp");
        assert_eq!(mcp["state"], "todo");
        assert!(mcp["summary"].as_str().unwrap().contains("--host"));
    }

    #[test]
    fn the_agent_is_recognised_from_its_environment() {
        let env = |pairs: &'static [(&'static str, &'static str)]| {
            move |k: &str| {
                pairs
                    .iter()
                    .find(|(name, _)| *name == k)
                    .map(|(_, v)| v.to_string())
            }
        };
        assert_eq!(
            Host::detect(env(&[("CLAUDECODE", "1")])),
            Some(Host::Claude)
        );
        assert_eq!(
            Host::detect(env(&[("CODEX_SANDBOX", "seatbelt")])),
            Some(Host::Codex)
        );
        assert_eq!(Host::detect(env(&[])), None);
    }

    #[test]
    fn registrations_are_found_where_each_agent_keeps_them() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("home");
        let folder = temp.path().join("work");
        std::fs::create_dir_all(home.join(".claude")).unwrap();
        std::fs::create_dir_all(home.join(".codex")).unwrap();
        std::fs::create_dir_all(&folder).unwrap();
        assert!(!registered(Host::Claude, &home, Some(&folder), None));
        assert!(!registered(Host::Codex, &home, None, None));

        // User scope, local (per folder) scope, the project's .mcp.json and the plugin.
        std::fs::write(
            home.join(".claude.json"),
            r#"{"mcpServers":{"switchboard":{"command":"switchboard","args":["mcp"]}},"oauthAccount":{"emailAddress":"never-read@example.com"}}"#,
        )
        .unwrap();
        assert!(registered(Host::Claude, &home, None, None));
        // The key is written through serde: a Windows path's backslashes need escaping.
        let local = |key: String| {
            serde_json::json!({"projects": {key: {"mcpServers": {"switchboard": {"command": "switchboard"}}}}})
                .to_string()
        };
        std::fs::write(home.join(".claude.json"), local(folder.display().to_string())).unwrap();
        assert!(registered(Host::Claude, &home, Some(&folder), None));
        assert!(!registered(Host::Claude, &home, None, None));
        // Claude Code on Windows writes the folder with forward slashes.
        std::fs::write(
            home.join(".claude.json"),
            local(folder.display().to_string().replace('\\', "/")),
        )
        .unwrap();
        assert!(registered(Host::Claude, &home, Some(&folder), None));
        std::fs::write(home.join(".claude.json"), "{\"mcpServ").unwrap();
        assert!(
            !registered(Host::Claude, &home, None, None),
            "a half-written file reads as absent"
        );
        std::fs::write(
            folder.join(".mcp.json"),
            r#"{"mcpServers":{"switchboard":{"type":"stdio","command":"switchboard"}}}"#,
        )
        .unwrap();
        assert!(registered(Host::Claude, &home, Some(&folder), None));
        std::fs::remove_file(folder.join(".mcp.json")).unwrap();
        std::fs::write(
            home.join(".claude/settings.json"),
            r#"{"enabledPlugins":{"switchboard@switchboard":false}}"#,
        )
        .unwrap();
        assert!(!registered(Host::Claude, &home, None, None));
        std::fs::write(
            home.join(".claude/settings.json"),
            r#"{"enabledPlugins":{"switchboard@switchboard":true}}"#,
        )
        .unwrap();
        assert!(registered(Host::Claude, &home, None, None));

        std::fs::write(
            home.join(".codex/config.toml"),
            "[mcp_servers.switchboard]\ncommand = \"switchboard\"\nargs = [\"mcp\"]\n",
        )
        .unwrap();
        assert!(registered(Host::Codex, &home, None, None));
        let other = temp.path().join("codex-home");
        std::fs::create_dir_all(&other).unwrap();
        assert!(
            !registered(Host::Codex, &home, None, Some(&other)),
            "CODEX_HOME wins"
        );
    }

    #[test]
    fn the_proving_call_keeps_a_short_printable_client_name_and_never_creates_storage() {
        let temp = tempfile::tempdir().unwrap();
        let missing = temp.path().join("not-created");
        record_proof(&missing, Some("claude-code"), 5);
        assert!(!missing.exists());
        assert!(proof(temp.path()).is_none());
        record_proof(
            temp.path(),
            Some(&format!("claude-code\n\u{1b}[31m{}", "x".repeat(100))),
            7,
        );
        let recorded = proof(temp.path()).unwrap();
        assert_eq!(recorded["at"], 7);
        let client = recorded["client"].as_str().unwrap();
        assert!(client.starts_with("claude-code31m"), "{client}");
        assert_eq!(client.len(), CLIENT_NAME_MAX);
    }
}
