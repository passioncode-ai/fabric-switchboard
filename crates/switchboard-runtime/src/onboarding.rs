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
    /// SB-202: the saved metadata (organizations, groups, accounts); none without a data folder.
    pub snapshot: Option<switchboard_core::Snapshot>,
    /// SB-202: accounts whose sign-in ended (the monitor's `sign_in_required`).
    pub sign_in_required: Vec<String>,
    /// SB-202: the contexts of recorded sessions (`personal`, `org:<id>`).
    pub contexts: Vec<String>,
    /// SB-202: `switchboard shell status` (SB-183, node A-1), when it was read.
    pub shell: Option<Value>,
    /// SB-202: PassionCode.ai is connected.
    pub passioncode: bool,
    /// SB-202: reads that failed or were partial (`snapshot`, `sign_ins`, `sessions`).
    pub degraded: Vec<String>,
}

/// SB-202: the saved metadata as the steps need it, read leniently: the owner answering may be
/// another version whose snapshot carries keys this one does not know (`Snapshot` refuses
/// unknown fields). Only the parts the steps read are decoded, entry by entry; an entry that
/// cannot be read is left out. The flag says something was left out (or nothing was readable).
pub fn read_snapshot(value: &Value) -> (Option<switchboard_core::Snapshot>, bool) {
    use serde::de::DeserializeOwned;
    fn list<T: DeserializeOwned>(value: &Value, partial: &mut bool) -> Vec<T> {
        match value {
            Value::Null => Vec::new(),
            Value::Array(items) => items
                .iter()
                .filter_map(|item| {
                    let read = serde_json::from_value(item.clone()).ok();
                    *partial |= read.is_none();
                    read
                })
                .collect(),
            _ => {
                *partial = true;
                Vec::new()
            }
        }
    }
    fn map<T: DeserializeOwned + Default>(value: &Value, partial: &mut bool) -> T {
        if value.is_null() {
            return T::default();
        }
        serde_json::from_value(value.clone()).unwrap_or_else(|_| {
            *partial = true;
            T::default()
        })
    }
    if !value.is_object() {
        return (None, true);
    }
    let mut partial = false;
    let snapshot = switchboard_core::Snapshot {
        accounts: list(&value["accounts"], &mut partial),
        projects: list(&value["projects"], &mut partial),
        organizations: list(&value["organizations"], &mut partial),
        memberships: map(&value["memberships"], &mut partial),
        elsewhere: map(&value["elsewhere"], &mut partial),
        ..Default::default()
    };
    (Some(snapshot), partial)
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
/// agent may run to start it); `optional` — not needed to finish. `next.turn_on` and
/// `next.turn_off` are the person's switches: the agent runs one only when the person asks.
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
                project["name"].as_str().unwrap_or("-")
            ),
            Value::Null,
        ),
        (Some(folder), None) => step(
            "project",
            "optional",
            format!(
                "{} is in no group yet: its sessions use the default group. A repository a group's agents create or work in joins that group by itself; to put this one in a group, name the group.",
                folder.display()
            ),
            json!({
                "command": format!(
                    "{cli} project move-root --folder {} --pool <group>",
                    shell_word(&folder.to_string_lossy())
                ),
            }),
        ),
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

    steps.extend(organization_steps(probe, &cli));

    let complete = steps
        .iter()
        .all(|s| s["state"] == "done" || s["state"] == "optional");
    json!({
        "complete": complete,
        "degraded": probe.degraded,
        "host": probe.host.map(Host::name),
        "folder": probe.folder.as_ref().map(|f| f.to_string_lossy().into_owned()),
        "steps": steps,
    })
}

/// SB-202: the organization checklist, after the five steps. A machine with no organization gets
/// none (or, with PassionCode.ai connected and listing none, one optional `organization` step);
/// otherwise `organization`, then per organization, newest first, `org_group`, `seat`, `hook`
/// and `launch`, each naming it (`organization: {id, name}`). A read that failed is named in
/// the step it touches and in `degraded`, never left out silently. Draft copy (SCN-076): it goes
/// through copywriting and Copylot in wave 4.
fn organization_steps(probe: &Probe, cli: &str) -> Vec<Value> {
    let degraded = |code: &str| probe.degraded.iter().any(|d| d == code);
    let Some(snapshot) = &probe.snapshot else {
        return if degraded("snapshot") {
            vec![step(
                "organization",
                "optional",
                "Switchboard's saved settings could not be read, so the organization steps are missing.".into(),
                Value::Null,
            )]
        } else {
            Vec::new()
        };
    };
    if snapshot.organizations.is_empty() {
        return if probe.passioncode {
            vec![step(
                "organization",
                "optional",
                "PassionCode.ai lists no organization for you yet.".into(),
                json!({"command": format!("{cli} org read")}),
            )]
        } else {
            Vec::new()
        };
    }
    let mut organizations: Vec<_> = snapshot.organizations.iter().collect();
    organizations.sort_by_key(|o| std::cmp::Reverse(o.created_at));
    let names = organizations
        .iter()
        .map(|o| o.name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let partial = if degraded("snapshot") {
        " Part of Switchboard's saved settings could not be read, so a step below may be missing."
    } else {
        ""
    };
    let mut steps = vec![step(
        "organization",
        "done",
        format!("Organizations on this computer: {names}.{partial}"),
        Value::Null,
    )];
    let provider = match probe.host {
        Some(Host::Codex) => "codex",
        _ => "claude",
    };
    for organization in organizations {
        let named = json!({"id": organization.id, "name": organization.name});
        let name = &organization.name;
        let mut push = |mut s: Value| {
            s["organization"] = named.clone();
            steps.push(s);
        };
        let group = snapshot.organization_group(&organization.id);
        let bound: Vec<&str> = snapshot
            .projects
            .iter()
            .filter(|p| p.organization.as_deref() == Some(organization.id.as_str()))
            .map(|p| p.name.as_str())
            .collect();
        push(match group {
            Some(group) => {
                let mut s = step(
                    "org_group",
                    "done",
                    format!("{name}'s group is “{}”.", group.name),
                    Value::Null,
                );
                s["group"] = json!({"pool": group.pool, "name": group.name});
                s
            }
            // Which group stays the organization's is the person's choice; the command is what
            // the agent runs on the others once the person said.
            None if bound.len() > 1 => step(
                "org_group",
                "needs_person",
                format!(
                    "Keep one group for {name} on this computer: {}.",
                    bound.join(", ")
                ),
                json!({
                    "person": format!("Say which of these groups stays {name}'s on this computer."),
                    "command": format!("{cli} project edit --pool <group> --organization personal"),
                }),
            ),
            // Whether a settings file exists, and which folders, is the person's to say.
            None => step(
                "org_group",
                "needs_person",
                format!("{name} has no group on this computer yet."),
                json!({
                    "command": format!("{cli} settings import <settings file> --under <your code folder> --apply"),
                    "person": format!("If {name}'s administrator gave you a settings file, it makes the group. Otherwise the agent makes one: `project save` with your folders, then `project edit --organization`."),
                    "alternatives": [
                        format!("{cli} project save --name {} --folder <absolute folder>", shell_word(name)),
                        format!("{cli} project edit --pool <group> --organization {}", shell_word(&organization.id)),
                    ],
                }),
            ),
        });
        let seats = snapshot.organization_seats(&organization.id);
        let signed_in = seats.iter().any(|a| {
            a.enabled
                && !snapshot.elsewhere.contains_key(&a.id)
                && !probe.sign_in_required.contains(&a.id)
        });
        let ended = seats
            .iter()
            .filter(|a| probe.sign_in_required.contains(&a.id))
            .count();
        let person = format!("Sign in with your {name} address.");
        let unread = if degraded("sign_ins") {
            " Whether a sign-in ended could not be read."
        } else {
            ""
        };
        push(match group {
            Some(group) if signed_in => step(
                "seat",
                "done",
                format!("{name}'s account in “{}” is signed in.{unread}", group.name),
                Value::Null,
            ),
            Some(group) => step(
                "seat",
                "needs_person",
                if ended > 0 {
                    format!(
                        "None of {name}'s accounts in {} is signed in: {ended} must sign in again.",
                        group.name
                    )
                } else {
                    format!("“{}” holds no signed-in account of {name}.", group.name)
                },
                json!({
                    "command": format!("{cli} login begin --provider {provider} --pool {}", shell_word(&group.pool)),
                    "person": person,
                }),
            ),
            None => step(
                "seat",
                "needs_person",
                format!("{name}'s group comes first; then the person signs in to it."),
                json!({"person": person}),
            ),
        });
        let place = group.map_or_else(|| format!("{name}'s"), |g| format!("{}'s", g.name));
        let hook = probe
            .shell
            .as_ref()
            .and_then(|s| s["hook"].as_str())
            .unwrap_or("");
        // A start file holding the block (A-3's record `files`): only then do terminals follow.
        let installed = probe.shell.as_ref().is_some_and(|s| {
            s["files"]
                .as_array()
                .is_some_and(|files| files.iter().any(|f| f["state"] == "written"))
        });
        // `turn_on` and `turn_off` are the person's: the agent runs either only when asked.
        push(match hook {
            "on" | "default_on" if installed => step(
                "hook",
                "optional",
                format!("Terminals in {place} folders work in {name}'s context."),
                json!({"turn_off": format!("{cli} shell off")}),
            ),
            "on" | "default_on" => step(
                "hook",
                "optional",
                format!("The terminal hook is on, but no start file holds it yet: terminals in {place} folders keep your own settings for now."),
                json!({"command": format!("{cli} shell status"), "turn_off": format!("{cli} shell off")}),
            ),
            "off" => step(
                "hook",
                "optional",
                format!("The terminal hook is off: terminals in {place} folders keep your own settings."),
                json!({"turn_on": format!("{cli} shell on")}),
            ),
            "default_off" => step(
                "hook",
                "optional",
                format!("Terminals do not follow {name}'s folders yet: the app turns this on once its group has a folder."),
                json!({"command": format!("{cli} shell status")}),
            ),
            _ => step(
                "hook",
                "optional",
                "The terminal hook's state could not be read.".into(),
                json!({"command": format!("{cli} shell status")}),
            ),
        });
        let context = format!("org:{}", organization.id);
        push(if probe.contexts.contains(&context) {
            step(
                "launch",
                "done",
                format!("A session ran in {name}'s context."),
                Value::Null,
            )
        } else {
            let folder = group
                .and_then(|g| g.folders.first())
                .map_or_else(|| "<a folder of the group>".into(), |f| shell_word(f));
            step(
                "launch",
                "optional",
                if degraded("sessions") {
                    format!("No session has run in {name}'s context yet, as far as can be read: the recorded sessions could not be read.")
                } else {
                    format!("No session has run in {name}'s context yet.")
                },
                json!({"command": format!("{cli} launch --provider {provider} --mode managed --working-directory {folder}")}),
            )
        });
    }
    steps
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
            "/Users/x/.local/bin/switchboard project move-root --folder '/work/my app' --pool <group>"
        );
        assert!(project["summary"]
            .as_str()
            .unwrap()
            .contains("joins that group by itself"));
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
        std::fs::write(
            home.join(".claude.json"),
            local(folder.display().to_string()),
        )
        .unwrap();
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

    fn ids(answer: &Value) -> Vec<&str> {
        answer["steps"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s["id"].as_str().unwrap())
            .collect()
    }
    fn seat(id: &str, pool: &str, email: &str) -> switchboard_core::Account {
        switchboard_core::Account {
            id: id.into(),
            label: email.into(),
            provider: switchboard_core::Provider::Claude,
            kind: switchboard_core::AuthKind::OAuth,
            pool: pool.into(),
            enabled: true,
            created_at: 1,
            identity: None,
            usage: None,
            external_identity: None,
            usage_health: None,
        }
    }
    /// A set-up machine (the five steps done) with the organization Acme, its one group and
    /// one seat in it.
    fn acme() -> Probe {
        let mut snapshot = switchboard_core::Snapshot::default();
        snapshot
            .organizations
            .push(switchboard_core::organizations::Organization {
                id: "o-acme".into(),
                name: "Acme".into(),
                domains: vec!["acme.com".into()],
                provider_organizations: vec![],
                created_at: 1,
                vesta_id: None,
            });
        snapshot.projects.push(switchboard_core::Project {
            pool: "acme-work".into(),
            name: "Acme work".into(),
            folders: vec!["/work/acme".into()],
            created_at: 1,
            sources: vec![],
            organization: Some("o-acme".into()),
        });
        snapshot
            .accounts
            .push(seat("a-1", "acme-work", "me@acme.com"));
        Probe {
            owner_online: true,
            accounts: vec![("claude".into(), 1)],
            folder: Some(PathBuf::from("/work/acme")),
            host: Some(Host::Claude),
            registered: vec![(Host::Claude, true)],
            proof: Some(json!({"client": "claude-code", "at": 1})),
            snapshot: Some(snapshot),
            shell: Some(json!({"hook": "default_on", "files": [
                {"shell": "zsh", "path": "/Users/x/.zshrc", "state": "written"}
            ]})),
            ..Probe::default()
        }
    }

    #[test]
    fn a_machine_without_organizations_keeps_the_five_steps() {
        let mut probe = acme();
        let snapshot = probe.snapshot.as_mut().unwrap();
        snapshot.organizations.clear();
        snapshot.projects[0].organization = None;
        let answer = steps(&probe);
        assert_eq!(
            ids(&answer),
            ["owner", "accounts", "project", "mcp", "proving_call"]
        );
        assert_eq!(answer["complete"], true);
        // Nothing read at all (no data folder): the same five.
        assert_eq!(ids(&steps(&Probe::default())).len(), 5);
        // PassionCode.ai connected and listing no organization: one optional step more.
        probe.passioncode = true;
        let answer = steps(&probe);
        assert_eq!(
            ids(&answer),
            [
                "owner",
                "accounts",
                "project",
                "mcp",
                "proving_call",
                "organization"
            ]
        );
        let organization = by_id(&answer, "organization");
        assert_eq!(organization["state"], "optional");
        assert!(organization["summary"]
            .as_str()
            .unwrap()
            .contains("lists no organization"));
        assert_eq!(answer["complete"], true);
    }

    #[test]
    fn an_organization_machine_names_its_group_seat_and_hook() {
        let answer = steps(&acme());
        assert_eq!(
            ids(&answer),
            [
                "owner",
                "accounts",
                "project",
                "mcp",
                "proving_call",
                "organization",
                "org_group",
                "seat",
                "hook",
                "launch"
            ]
        );
        for id in ["org_group", "seat", "hook", "launch"] {
            assert_eq!(
                by_id(&answer, id)["organization"],
                json!({"id": "o-acme", "name": "Acme"}),
                "{id}"
            );
        }
        assert_eq!(by_id(&answer, "organization")["state"], "done");
        let group = by_id(&answer, "org_group");
        assert_eq!(group["state"], "done");
        assert_eq!(
            group["group"],
            json!({"pool": "acme-work", "name": "Acme work"})
        );
        assert_eq!(by_id(&answer, "seat")["state"], "done");
        let hook = by_id(&answer, "hook");
        assert_eq!(hook["state"], "optional");
        assert_eq!(
            hook["summary"],
            "Terminals in Acme work's folders work in Acme's context."
        );
        assert_eq!(hook["next"]["turn_off"], "switchboard shell off");
        assert!(
            hook["next"]["command"].is_null(),
            "never run to turn it off"
        );
        let launch = by_id(&answer, "launch");
        assert_eq!(launch["state"], "optional");
        assert_eq!(
            launch["next"]["command"],
            "switchboard launch --provider claude --mode managed --working-directory /work/acme"
        );
        assert_eq!(answer["complete"], true, "{answer}");

        // A launch recorded in the organization's context: done.
        let mut probe = acme();
        probe.contexts = vec!["personal".into(), "org:o-acme".into()];
        assert_eq!(by_id(&steps(&probe), "launch")["state"], "done");
        // The hook turned off: still optional; turning it on is the person's word, never a
        // command the agent acts on.
        probe.shell = Some(json!({"hook": "off"}));
        let answer = steps(&probe);
        let hook = by_id(&answer, "hook");
        assert_eq!(hook["state"], "optional");
        assert_eq!(hook["next"]["turn_on"], "switchboard shell on");
        assert!(hook["next"]["command"].is_null(), "never run to turn it on");
        // On by setting, but no start file holds the block yet: the summary says the setting,
        // never that terminals already work in the context.
        for files in [
            json!([]),
            json!([{"shell": "zsh", "path": "/x", "state": "skipped_symlink"}]),
        ] {
            probe.shell = Some(json!({"hook": "default_on", "files": files}));
            let answer = steps(&probe);
            let hook = by_id(&answer, "hook");
            assert_eq!(
                hook["summary"],
                "The terminal hook is on, but no start file holds it yet: terminals in Acme work's folders keep your own settings for now."
            );
            assert_eq!(hook["next"]["turn_off"], "switchboard shell off");
            assert_eq!(hook["next"]["command"], "switchboard shell status");
        }

        // No seat in the group: the person signs in, in the group's pool; not complete.
        let mut probe = acme();
        probe.snapshot.as_mut().unwrap().accounts[0].pool = "default".into();
        let answer = steps(&probe);
        let seat = by_id(&answer, "seat");
        assert_eq!(seat["state"], "needs_person");
        assert_eq!(
            seat["next"]["command"],
            "switchboard login begin --provider claude --pool acme-work"
        );
        assert_eq!(seat["next"]["person"], "Sign in with your Acme address.");
        assert_eq!(answer["complete"], false);

        // No group for the organization: to do, with the commands that make one.
        let mut probe = acme();
        probe.snapshot.as_mut().unwrap().projects[0].organization = None;
        let answer = steps(&probe);
        let group = by_id(&answer, "org_group");
        assert_eq!(group["state"], "needs_person", "the person chooses how");
        assert!(group["next"]["command"]
            .as_str()
            .unwrap()
            .contains("settings import"));
        assert_eq!(group["next"]["alternatives"].as_array().unwrap().len(), 2);
        assert_eq!(by_id(&answer, "seat")["state"], "needs_person");
        assert!(by_id(&answer, "seat")["next"]["command"].is_null());
        assert_eq!(answer["complete"], false);

        // Two groups bound to Acme: named, and none is chosen.
        let mut probe = acme();
        let snapshot = probe.snapshot.as_mut().unwrap();
        let mut second = snapshot.projects[0].clone();
        second.pool = "acme-two".into();
        second.name = "Acme two".into();
        second.folders = vec!["/work/acme-two".into()];
        snapshot.projects.push(second);
        let answer = steps(&probe);
        let group = by_id(&answer, "org_group");
        assert_eq!(group["state"], "needs_person");
        assert_eq!(
            group["summary"],
            "Keep one group for Acme on this computer: Acme work, Acme two."
        );
        assert_eq!(
            group["next"]["person"],
            "Say which of these groups stays Acme's on this computer."
        );
        assert_eq!(
            group["next"]["command"],
            "switchboard project edit --pool <group> --organization personal"
        );
        assert_eq!(answer["complete"], false);
    }

    #[test]
    fn a_snapshot_from_another_version_still_gives_the_organization_steps() {
        // SB-202: the owner may be newer than this CLI. An unknown top-level key never drops
        // the organization steps; an entry this CLI cannot read is left out and said so.
        let probe = acme();
        let mut value = serde_json::to_value(probe.snapshot.as_ref().unwrap()).unwrap();
        value["a_field_from_a_newer_owner"] = json!({"x": 1});
        let (read, degraded) = read_snapshot(&value);
        let read = read.expect("read leniently");
        assert!(
            !degraded,
            "an unknown top-level key loses nothing the steps need"
        );
        assert_eq!(read.organizations.len(), 1);
        assert_eq!(read.projects.len(), 1);
        assert_eq!(read.accounts.len(), 1);
        assert!(serde_json::from_value::<switchboard_core::Snapshot>(value.clone()).is_err());

        value["accounts"][0]["a_newer_account_field"] = json!(true);
        let (read, degraded) = read_snapshot(&value);
        assert!(degraded);
        let mut probe = acme();
        probe.snapshot = read;
        probe.degraded = vec!["snapshot".into()];
        let answer = steps(&probe);
        assert_eq!(answer["degraded"], json!(["snapshot"]));
        let organization = by_id(&answer, "organization");
        assert_eq!(
            organization["summary"],
            "Organizations on this computer: Acme. Part of Switchboard's saved settings could not be read, so a step below may be missing."
        );
        assert_eq!(by_id(&answer, "seat")["state"], "needs_person");

        // Nothing readable at all: one step says so, never silence.
        let (read, degraded) = read_snapshot(&json!("not an object"));
        assert!(read.is_none() && degraded);
        let mut probe = acme();
        probe.snapshot = None;
        probe.degraded = vec!["snapshot".into()];
        let answer = steps(&probe);
        let organization = by_id(&answer, "organization");
        assert_eq!(organization["state"], "optional");
        assert_eq!(
            organization["summary"],
            "Switchboard's saved settings could not be read, so the organization steps are missing."
        );
    }

    #[test]
    fn an_unread_sign_in_or_session_list_is_named_not_failed() {
        // SB-202: the monitor and the session list are read for the seat and launch steps; a
        // read that fails leaves the list empty and the step says what is unknown.
        let mut probe = acme();
        probe.degraded = vec!["sign_ins".into(), "sessions".into()];
        let answer = steps(&probe);
        assert_eq!(answer["degraded"], json!(["sign_ins", "sessions"]));
        assert_eq!(
            by_id(&answer, "seat")["summary"],
            "Acme's account in “Acme work” is signed in. Whether a sign-in ended could not be read."
        );
        assert_eq!(
            by_id(&answer, "launch")["summary"],
            "No session has run in Acme's context yet, as far as can be read: the recorded sessions could not be read."
        );
        assert!(steps(&acme())["degraded"].as_array().unwrap().is_empty());
    }

    #[test]
    fn a_seat_that_ended_is_not_a_signed_in_seat() {
        let mut probe = acme();
        probe.sign_in_required = vec!["a-1".into()];
        let answer = steps(&probe);
        let seat = by_id(&answer, "seat");
        assert_eq!(seat["state"], "needs_person");
        assert_eq!(
            seat["summary"],
            "None of Acme's accounts in Acme work is signed in: 1 must sign in again."
        );
        assert_eq!(answer["complete"], false);
        // A switched-off seat, or one used on another machine, does not count either.
        let mut probe = acme();
        probe.snapshot.as_mut().unwrap().accounts[0].enabled = false;
        assert_eq!(by_id(&steps(&probe), "seat")["state"], "needs_person");
        let mut probe = acme();
        probe
            .snapshot
            .as_mut()
            .unwrap()
            .elsewhere
            .insert("a-1".into(), 1);
        assert_eq!(by_id(&steps(&probe), "seat")["state"], "needs_person");
        // A personal account in the group (another domain) is no seat of Acme.
        let mut probe = acme();
        probe.snapshot.as_mut().unwrap().accounts[0].label = "me@example.org".into();
        assert_eq!(by_id(&steps(&probe), "seat")["state"], "needs_person");
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
