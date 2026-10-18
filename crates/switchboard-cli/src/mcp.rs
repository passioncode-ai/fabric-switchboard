//! `switchboard mcp`: a Model Context Protocol server on stdio for coding agents.
//! Newline-delimited JSON-RPC 2.0. stdout carries protocol messages only. No tool accepts
//! or returns a credential, and login and launch are not exposed.
use serde_json::{json, Map, Value};
use std::path::PathBuf;
use switchboard_core::Provider;
use switchboard_runtime::{
    control, execute_offline, onboarding,
    projects::{detect_session, Session},
    Operation,
};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

const VERSIONS: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];
const LINE_LIMIT: u64 = 1024 * 1024;
const REFRESH_SPACING: i64 = 60;
const INSTRUCTIONS: &str = "Switchboard manages the Claude Code and Codex accounts on this machine. Read switchboard_status first. Remaining quota is in switchboard_usage; an unknown or stale value is not zero. A managed session changes account from the next request with switchboard_switch. An eligible unassigned Git repository opened by a coding agent becomes an account-less project through the existing suggestion path; never move account IDs to create one. Project rules are optional and off unless the operator saved one: when you move to another project, call switchboard_project_apply with its folder and tell the operator which rule applied. Changing the ordinary Claude Code login affects every claude session on the machine and needs global: true; ask the operator first.";

pub struct Server {
    root: PathBuf,
    read_only: bool,
    /// The client's name from `initialize`, kept for the proving call (SB-93).
    client: std::sync::Mutex<Option<String>>,
    /// Whether this server already recorded its proving call.
    proved: std::sync::atomic::AtomicBool,
}

pub(crate) fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|t| t.as_secs() as i64)
        .unwrap_or(0)
}
pub(crate) fn rfc3339(seconds: i64) -> Value {
    time::OffsetDateTime::from_unix_timestamp(seconds)
        .ok()
        .and_then(|t| {
            t.format(&time::format_description::well_known::Rfc3339)
                .ok()
        })
        .map(Value::String)
        .unwrap_or(Value::Null)
}

/// One tool: its name, whether it writes, and its schema.
pub(crate) fn tools() -> Vec<(bool, Value)> {
    let path = json!({"type":"string","description":"Absolute project folder. Defaults to the folder the agent runs in."});
    let account = json!({"type":"string","description":"Account id from switchboard_accounts."});
    vec![
        (false, json!({"name":"switchboard_status","title":"Switchboard status","description":"Who handles this session's requests: runtime state, this session (managed pool, isolated, or native), the signed-in CLI accounts, selected accounts per pool, active project rules and automatic rotation. Call this first.","inputSchema":{"type":"object","properties":{},"additionalProperties":false}})),
        (false, json!({"name":"switchboard_accounts","title":"Switchboard accounts","description":"Stored accounts without credentials: label, provider, pool, whether selected or signed in now, and the lowest remaining quota.","inputSchema":{"type":"object","properties":{"provider":{"type":"string","enum":["claude","codex"]}},"additionalProperties":false}})),
        (false, json!({"name":"switchboard_usage","title":"Remaining usage","description":"Remaining quota per account and window, with reset times and how fresh each observation is. Unknown is not zero. A window with scope feature limits one metered feature only; lowest_remaining_percent covers the account's own windows. refresh asks the provider for one account; within 60 seconds of its last check it returns that observation with a note, and until the time the provider asked for after answering a check with 429 it is refused; for an inactive Claude account it may first renew that account's expired sign-in inside Switchboard. A read-only server never asks and shows the stored observation. No credential is returned.","inputSchema":{"type":"object","properties":{"account_id":account,"refresh":{"type":"boolean","default":false}},"additionalProperties":false}})),
        (true, json!({"name":"switchboard_switch","title":"Switch account","description":"Choose the account for the next request. target session (default) switches this managed session; route selects the account in its own pool for any managed session; claude_cli changes the ordinary Claude Code login for every claude session on this machine and requires global: true. A response already streaming keeps its account.","inputSchema":{"type":"object","properties":{"account_id":account,"target":{"type":"string","enum":["session","route","claude_cli"],"default":"session"},"global":{"type":"boolean","default":false}},"required":["account_id"],"additionalProperties":false}})),
        (false, json!({"name":"switchboard_sessions","title":"Sessions","description":"Sessions Switchboard launched (Claude Code, Codex): account_id, label, provider, pool, mode (managed or isolated), working_directory, in_place, launched_at, started_at, continues (a workflow id or null) and state — starting, running, ended or unknown. Without all, only starting and running ones. Reads only; no credential.","inputSchema":{"type":"object","properties":{"all":{"type":"boolean","description":"Also list ended sessions."}},"additionalProperties":false}})),
        (false, json!({"name":"switchboard_onboarding","title":"Setup steps","description":"Where setting up Switchboard stands, as steps in order: owner (the app runs), accounts (one is saved; signing in is the person's), project (optional; an eligible unassigned Git repository becomes an account-less project on agent open, or run next.command), mcp (this server is registered with the agent) and proving_call. Each step is done, todo (run next.command), needs_person (tell the person next.person) or optional. Reads only. The full guide: `switchboard onboard`.","inputSchema":{"type":"object","properties":{"path":path,"host":{"type":"string","enum":["claude","codex"],"description":"The coding agent being set up; default: the one this server runs for, when known."}},"additionalProperties":false}})),
        (false, json!({"name":"switchboard_project_context","title":"Project context","description":"The project a folder belongs to (its name, pool, folders and reserved accounts, possibly empty; null when none), and the optional project rule per provider: the rule in force, the nearest rule in any state (paused or expired), and whether it is in effect. A project's accounts serve only sessions launched from its folders; a provider with no enabled project account keeps its existing launch fallback.","inputSchema":{"type":"object","properties":{"path":path},"additionalProperties":false}})),
        (false, json!({"name":"switchboard_projects","title":"Projects","description":"Every project: its pool, name, folders (repositories), roots (each root folder with its branch and the worktrees and clones of its repository under it), sources and the accounts reserved for it (empty for an automatically created project), then every optional project rule with its state. Reads only.","inputSchema":{"type":"object","properties":{},"additionalProperties":false}})),
        (true, json!({"name":"switchboard_project_edit","title":"Change a project","description":"Change a project by delta: reserve accounts for it (add_accounts), give accounts back to the default pool (remove_accounts), add folders (add_folders) or take folders out (remove_folders; they are not added back by themselves), or rename it. Everything not named stays as it is; naming an account or folder the project does not hold is refused. Only when the operator asks. The account the ordinary Claude Code or Codex is signed in to cannot join a project.","inputSchema":{"type":"object","properties":{"pool":{"type":"string","description":"The project's pool, from switchboard_projects."},"name":{"type":"string"},"add_accounts":{"type":"array","items":account,"maxItems":64},"remove_accounts":{"type":"array","items":account,"maxItems":64},"add_folders":{"type":"array","items":{"type":"string","description":"An absolute folder."},"maxItems":64},"remove_folders":{"type":"array","items":{"type":"string","description":"A folder of the project, as switchboard_projects lists it."},"maxItems":64}},"required":["pool"],"additionalProperties":false}})),
        (false, json!({"name":"switchboard_events","title":"Activity","description":"Recent activity, newest first: what changed (accounts, switches, projects, rotation, sign-ins) and how it ended, as fixed codes. No credential, prompt or provider text.","inputSchema":{"type":"object","properties":{"limit":{"type":"integer","minimum":1,"maximum":500,"default":50}},"additionalProperties":false}})),
        (true, json!({"name":"switchboard_account_update","title":"Rename, enable or disable an account","description":"Rename an account, or enable or disable it (a disabled account is never selected or switched to). Only when the operator asks. What is left out stays as it is.","inputSchema":{"type":"object","properties":{"account_id":account,"label":{"type":"string","minLength":1,"maxLength":80},"enabled":{"type":"boolean"}},"required":["account_id"],"additionalProperties":false}})),
        (true, json!({"name":"switchboard_account_move","title":"Move an account to another pool","description":"Move an account into a project's pool, or back to the default pool (pool: default). Only when the operator asks. Joining or leaving a project is that project's change, with its refusals: the account the ordinary Claude Code or Codex is signed in to cannot join a project, and a second copy of a login already in that pool is refused.","inputSchema":{"type":"object","properties":{"account_id":account,"pool":{"type":"string","minLength":1,"description":"A project's pool from switchboard_projects, or default."}},"required":["account_id","pool"],"additionalProperties":false}})),
        (false, json!({"name":"switchboard_rotation","title":"Automatic rotation","description":"The automatic rotation policy per provider, pool and target (enabled, threshold, hysteresis, cooldown, freshness) and what the background monitor decided last.","inputSchema":{"type":"object","properties":{},"additionalProperties":false}})),
        (true, json!({"name":"switchboard_rotation_set","title":"Set automatic rotation","description":"Change one rotation policy (provider, pool, target): switch it on or off, or change its threshold, hysteresis, cooldown or freshness. What is left out keeps its saved value. Only when the operator asks.","inputSchema":{"type":"object","properties":{"provider":{"type":"string","enum":["claude","codex"]},"pool":{"type":"string","default":"default"},"target":{"type":"string","enum":["managed","claude_cli"],"default":"managed"},"enabled":{"type":"boolean"},"threshold_percent":{"type":"number","minimum":1,"maximum":100},"hysteresis_percent":{"type":"number","minimum":0,"maximum":99},"cooldown_seconds":{"type":"integer","minimum":0},"max_age_seconds":{"type":"integer","minimum":30}},"required":["provider"],"additionalProperties":false}})),
        (false, json!({"name":"switchboard_backups","title":"Backups","description":"Encrypted backups of accounts, projects, rules and settings: the folder, whether they run, the newest backups and the last error. Never their contents.","inputSchema":{"type":"object","properties":{},"additionalProperties":false}})),
        (true, json!({"name":"switchboard_backup_now","title":"Back up now","description":"Write an encrypted backup now. Restoring stays the person's, in the app or with switchboard backup restore.","inputSchema":{"type":"object","properties":{},"additionalProperties":false}})),
        (false, json!({"name":"switchboard_recovery","title":"Stopped sessions","description":"Sessions that stopped on a limit and the accounts ready for them to continue (SB-115): each stop's mode and when to try again.","inputSchema":{"type":"object","properties":{},"additionalProperties":false}})),
        (false, json!({"name":"switchboard_handovers","title":"Hand-overs","description":"Whether hand-overs open in Fabric's console, and the hand-overs waiting for a console to take them (SB-108).","inputSchema":{"type":"object","properties":{},"additionalProperties":false}})),
        (true, json!({"name":"switchboard_agent_set","title":"Enable or disable an agent, or set the default","description":"Let Switchboard hand work to an agent or keep work away from it (enabled), or set the machine's default agent (default: an agent id, or null for none). Only when the operator asks.","inputSchema":{"type":"object","properties":{"agent":{"type":"string"},"enabled":{"type":"boolean"},"default":{"type":["string","null"]}},"additionalProperties":false}})),
        (true, json!({"name":"switchboard_login","title":"Sign in a new account","description":"Start the official sign-in for a new Claude or Codex account (action begin: provider, label, pool; it opens Terminal where the person signs in) and follow it (status, finish once the person signed in, cancel). The person signs in; this never takes a password or a key.","inputSchema":{"type":"object","properties":{"action":{"type":"string","enum":["begin","status","finish","cancel"]},"login_id":{"type":"string"},"provider":{"type":"string","enum":["claude","codex"]},"label":{"type":"string","minLength":1,"maxLength":80},"pool":{"type":"string","default":"default"}},"required":["action"],"additionalProperties":false}})),
        (true, json!({"name":"switchboard_project_set","title":"Save project rule","description":"Save an optional rule: this folder and its subfolders start on this account. Only when the operator asks for it. Rules never stop rotation. Prefer an expiry.","inputSchema":{"type":"object","properties":{"path":path,"account_id":account,"target":{"type":"string","enum":["managed","claude_cli"],"default":"managed"},"enabled":{"type":"boolean","default":true},"expires_in_hours":{"type":"integer","minimum":1,"maximum":720}},"required":["account_id"],"additionalProperties":false}})),
        (true, json!({"name":"switchboard_project_remove","title":"Remove project rule","description":"Remove the rule saved for exactly this folder and provider.","inputSchema":{"type":"object","properties":{"path":path,"provider":{"type":"string","enum":["claude","codex"]}},"required":["provider"],"additionalProperties":false}})),
        (true, json!({"name":"switchboard_project_apply","title":"Apply project rule","description":"Apply the folder's rule to this session when you start work in a project. Reports what happened per provider: selected, activated, already_in_effect, no_rule, rule_paused, rule_expired, other_pool, other_session, needs_global or failed. Nothing changes without a rule in force.","inputSchema":{"type":"object","properties":{"path":path,"global":{"type":"boolean","default":false}},"additionalProperties":false}})),
        (false, json!({"name":"switchboard_chain_get","title":"Fallback chains","description":"Which agents continue a workflow, in what order, when its executor's accounts run out: every chain the operator set (per machine, project pool or workflow), the presets, and the chain that applies — the workflow's own, else its project's, else the machine's. None applies until the operator sets one.","inputSchema":{"type":"object","properties":{"workflow_id":{"type":"string","description":"An Observatory workflow id (wf_ and 16 hex digits)."},"pool":{"type":"string","description":"A project's pool."}},"additionalProperties":false}})),
        (true, json!({"name":"switchboard_chain_set","title":"Set fallback chain","description":"Set the operator's chain for a scope, first agent tried first; an empty list clears it. Only when the operator asks. Agents are catalog ids that load MCP servers and take a prompt without a person (claude-code, codex, kimi-code, hermes, …); an agent may pin an account (account_id) or a paid key by its Observatory vault name (key: project/env/NAME), never a key value. Another agent than Claude Code or Codex never runs on a subscription sign-in.","inputSchema":{"type":"object","properties":{"scope":{"type":"string","description":"machine, project:<pool> or workflow:<wf_id>.","default":"machine"},"preset":{"type":"string","enum":["subscriptions-first"]},"executors":{"type":"array","maxItems":8,"items":{"type":"object","properties":{"agent":{"type":"string"},"account_id":account,"key":{"type":"string"}},"required":["agent"],"additionalProperties":false}}},"additionalProperties":false}})),
        (false, json!({"name":"switchboard_openrouter_status","title":"OpenRouter key for agents","description":"Whether the operator saved an OpenRouter key for agents, its default model, and what the key may still spend today and in total (asked of OpenRouter). Never the key itself.","inputSchema":{"type":"object","properties":{},"additionalProperties":false}})),
        (false, json!({"name":"switchboard_hermes_status","title":"Hermes model and provider","description":"The model and provider the operator's ordinary Hermes Agent runs on (hermes config get model): model, provider, endpoint, API mode. Never a key.","inputSchema":{"type":"object","properties":{},"additionalProperties":false}})),
        (true, json!({"name":"switchboard_hermes_set","title":"Change Hermes's model or provider","description":"Change the provider and/or the default model of the operator's ordinary Hermes Agent through Hermes's own hermes config set. Only when the operator asks. Changing the provider makes Hermes drop an endpoint that belonged to the previous one; the new provider needs its own key in Hermes.","inputSchema":{"type":"object","properties":{"provider":{"type":"string","description":"A Hermes provider id, for example openrouter, anthropic, nous, openai-codex, kimi-coding."},"model":{"type":"string","description":"The default model, for example moonshotai/kimi-k2."}},"additionalProperties":false}})),
        (false, json!({"name":"switchboard_agents","title":"Coding agents on this computer","description":"The coding agents Switchboard knows: which are installed here, which the operator enabled for taking over work (fallback chains and switchboard continue; account switching is separate), the machine's default agent (null is Auto: switchboard continue without an account hands work to it, and Fabric starts with it unless a project sets its own), the command Switchboard runs for each, and the recommended OpenRouter models. Changing these is the operator's (the Agents page or switchboard agents enable|disable|default).","inputSchema":{"type":"object","properties":{},"additionalProperties":false}})),
        (false, json!({"name":"switchboard_kimi_accounts","title":"Kimi Code accounts","description":"The operator's Kimi Code subscription accounts saved in Switchboard: label, region, membership tier and each plan window's used percent and reset (5-hour, 7-day, monthly), asked of Kimi; plus the ordinary kimi CLI's sign-in. Never a token. Signing in, launching and removing are the operator's (the app or switchboard kimi).","inputSchema":{"type":"object","properties":{},"additionalProperties":false}})),
        (true, json!({"name":"switchboard_openrouter_model","title":"Set the agents' OpenRouter model","description":"Change the OpenRouter model agents launched on the operator's key start on, for example moonshotai/kimi-k2 or anthropic/claude-sonnet-4.5. Only when the operator asks. The key itself is saved by the operator in the app or with switchboard agents openrouter set, never through this server.","inputSchema":{"type":"object","properties":{"model":{"type":"string","description":"An OpenRouter model id: vendor/model."}},"required":["model"],"additionalProperties":false}})),
    ]
    .into_iter()
    .map(|(write, mut tool)| {
        tool["annotations"] = if write {
            json!({"readOnlyHint":false,"destructiveHint":false,"idempotentHint":true,"openWorldHint":false})
        } else {
            json!({"readOnlyHint":true,"openWorldHint":false})
        };
        (write, tool)
    })
    .collect()
}

impl Server {
    pub fn new(root: PathBuf, read_only: bool) -> Self {
        Self {
            root,
            read_only,
            client: std::sync::Mutex::new(None),
            proved: std::sync::atomic::AtomicBool::new(false),
        }
    }
    async fn call(&self, operation: Operation) -> Result<Value, String> {
        let body = serde_json::to_vec(&operation).map_err(|_| "Operation unavailable.")?;
        let mut attempt = 0;
        loop {
            let operation: Operation =
                serde_json::from_slice(&body).map_err(|_| "Operation unavailable.")?;
            match control::request(&self.root, &operation).await? {
                Some(value) => return Ok(value),
                None => match execute_offline(self.root.clone(), operation).await {
                    // Another short-lived CLI or agent holds the offline lock; it is released
                    // within one operation.
                    Err(error)
                        if error == "Another Switchboard instance owns this account storage"
                            && attempt < 10 =>
                    {
                        attempt += 1;
                        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
                    }
                    result => return result,
                },
            }
        }
    }
    async fn online(&self) -> Option<Value> {
        control::request(&self.root, &Operation::Status)
            .await
            .ok()
            .flatten()
    }
    async fn session(&self) -> (Session, Option<Value>) {
        let status = self.online().await;
        let proxy = status
            .as_ref()
            .and_then(|s| s["proxy_address"].as_str().map(str::to_owned));
        (
            detect_session(|k| std::env::var(k).ok(), proxy.as_deref(), &self.root),
            status,
        )
    }
    fn folder(&self, args: &Map<String, Value>) -> Result<PathBuf, String> {
        match args.get("path") {
            Some(Value::String(path)) => {
                let path = PathBuf::from(path);
                if path.is_absolute() {
                    Ok(path)
                } else {
                    Err("Pass an absolute project folder.".into())
                }
            }
            Some(_) => Err("path must be a string.".into()),
            None => std::env::var_os("CLAUDE_PROJECT_DIR")
                .map(PathBuf::from)
                .filter(|p| p.is_absolute())
                .or_else(|| std::env::current_dir().ok())
                .ok_or_else(|| "Pass the project folder as path.".into()),
        }
    }

    async fn status(&self) -> Result<Value, String> {
        let (session, status) = self.session().await;
        let snapshot = self.call(Operation::Snapshot).await?;
        let current = self.call(Operation::CurrentAccounts).await.ok();
        let monitor = if status.is_some() {
            self.call(Operation::MonitorStatus).await.ok()
        } else {
            None
        };
        let accounts = snapshot["accounts"].as_array().cloned().unwrap_or_default();
        let label = |id: &Value| {
            accounts
                .iter()
                .find(|a| &a["id"] == id)
                .map(|a| a["label"].clone())
                .unwrap_or(Value::Null)
        };
        let routes: Map<String, Value> = snapshot["routes"]
            .as_object()
            .map(|routes| {
                routes
                    .iter()
                    .map(|(k, id)| (k.clone(), json!({"account_id": id, "label": label(id)})))
                    .collect()
            })
            .unwrap_or_default();
        let rules = rule_views(&snapshot, now());
        let active: Vec<&Value> = rules.iter().filter(|r| r["state"] == "active").collect();
        let folder = self.folder(&Map::new()).ok();
        let here = match &folder {
            Some(path) if path.is_dir() => self
                .call(Operation::ResolveProject { path: path.clone() })
                .await
                .ok(),
            _ => None,
        };
        let decisions = monitor
            .as_ref()
            .and_then(|m| m["decisions"].as_array().cloned())
            .unwrap_or_default();
        let rotation: Vec<Value> = snapshot["policies"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(|p| {
                let decision = decisions.iter().find(|d| {
                    d["provider"] == p["provider"] && d["pool"] == p["pool"] && d["target"] == p["target"]
                });
                json!({"provider": p["provider"], "pool": p["pool"], "target": p["target"], "enabled": p["enabled"], "threshold_percent": p["threshold_percent"], "last_decision": decision.map(|d| d["reason"].clone())})
            })
            .collect();
        Ok(json!({
            "runtime": {"online": status.is_some(), "platform": std::env::consts::OS,
                "note": if status.is_some() { "The desktop app or switchboard serve owns the proxy." } else { "Switchboard is not running: managed sessions cannot route and rotation is paused. Reads come from local metadata." }},
            "session": session,
            "current_cli_accounts": current,
            "routes": routes,
            "rules": {"active": active, "total": rules.len(), "for_this_folder": here},
            "rotation": rotation,
            "limited": monitor.as_ref().and_then(|m| m.get("limited").cloned()).unwrap_or_else(|| json!([])),
        }))
    }
    async fn accounts(&self, args: &Map<String, Value>) -> Result<Value, String> {
        let provider = args.get("provider").and_then(Value::as_str);
        let snapshot = self.call(Operation::Snapshot).await?;
        let current = self.call(Operation::CurrentAccounts).await.ok();
        let routes = snapshot["routes"].as_object().cloned().unwrap_or_default();
        let time = now();
        let accounts: Vec<Value> = snapshot["accounts"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter(|a| provider.is_none_or(|p| a["provider"] == p))
            .map(|a| {
                let key = format!("{}:{}", a["provider"].as_str().unwrap_or(""), a["pool"].as_str().unwrap_or(""));
                let signed_in = current.as_ref().is_some_and(|c| {
                    let entry = &c[a["provider"].as_str().unwrap_or("")];
                    entry["account_id"] == a["id"]
                        || entry["account_ids"].as_array().is_some_and(|ids| ids.contains(&a["id"]))
                });
                let usage = usage_view(&a, &snapshot, time);
                json!({"id": a["id"], "label": a["label"], "provider": a["provider"], "kind": a["kind"], "pool": a["pool"], "enabled": a["enabled"],
                    "selected_for_next_request": routes.get(&key) == Some(&a["id"]), "signed_in_cli": signed_in,
                    "lowest_remaining_percent": usage["lowest_remaining_percent"], "usage_fresh": usage["fresh"]})
            })
            .collect();
        Ok(json!({"accounts": accounts}))
    }
    async fn usage(&self, args: &Map<String, Value>) -> Result<Value, String> {
        let id = args.get("account_id").and_then(Value::as_str);
        let refresh = args
            .get("refresh")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let mut note = Value::Null;
        if refresh && self.read_only {
            // A check is a provider request that writes its answer and may renew a sign-in.
            note = json!("This server is read-only: showing the stored observation without asking the provider.");
        } else if refresh {
            let id = id.ok_or("Name one account_id to refresh.")?;
            let snapshot = self.call(Operation::Snapshot).await?;
            let account = find(&snapshot, id)?;
            let checked = account["usage_health"]["checked_at"].as_i64().unwrap_or(0);
            if now() - checked < REFRESH_SPACING {
                note = json!("Checked less than a minute ago; showing that observation.");
            } else {
                self.call(Operation::Usage { id: id.into() }).await?;
            }
        }
        let snapshot = self.call(Operation::Snapshot).await?;
        let time = now();
        let accounts: Vec<Value> = snapshot["accounts"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter(|a| id.is_none_or(|id| a["id"] == id))
            .map(|a| {
                let mut view = usage_view(&a, &snapshot, time);
                view["id"] = a["id"].clone();
                view["label"] = a["label"].clone();
                view["provider"] = a["provider"].clone();
                view["pool"] = a["pool"].clone();
                view
            })
            .collect();
        if let Some(id) = id {
            if accounts.is_empty() {
                return Err(format!(
                    "No account with id {id}. Call switchboard_accounts."
                ));
            }
        }
        Ok(json!({"accounts": accounts, "note": note}))
    }
    async fn switch(&self, args: &Map<String, Value>) -> Result<Value, String> {
        let id = required(args, "account_id")?;
        let target = args
            .get("target")
            .and_then(Value::as_str)
            .unwrap_or("session");
        let global = args.get("global").and_then(Value::as_bool).unwrap_or(false);
        let snapshot = self.call(Operation::Snapshot).await?;
        let account = find(&snapshot, id)?;
        let provider: Provider = serde_json::from_value(account["provider"].clone())
            .map_err(|_| "Account provider unavailable.")?;
        let pool = account["pool"].as_str().unwrap_or_default().to_owned();
        let label = account["label"].as_str().unwrap_or_default().to_owned();
        match target {
            "claude_cli" => {
                if !global {
                    return Err("This changes the Claude Code login for every ordinary claude session on this machine. Ask the operator, then call again with global: true.".into());
                }
                self.call(Operation::ActivateNative { id: id.into() })
                    .await?;
                Ok(
                    json!({"action":"activated","account":{"id":id,"label":label,"pool":pool},"message":format!("Claude Code now signs in as {label} for every ordinary claude session. Running sessions may keep their account until they reload credentials.")}),
                )
            }
            "session" | "route" => {
                if target == "session" {
                    match self.session().await.0 {
                        Session::Managed { provider: p, pool: session_pool } => {
                            if p != provider {
                                return Err("This session uses another provider. Choose an account of the same provider.".into());
                            }
                            if session_pool != pool {
                                return Err(format!("This session routes the {session_pool} pool; that account is in {pool}. Choose an account in {session_pool}, or launch a managed session in {pool}."));
                            }
                        }
                        Session::Isolated { .. } => return Err("This session is isolated to one account; a route change cannot reach it. Launch a managed session to switch accounts inside a session.".into()),
                        Session::Native { .. } => return Err("This session does not run through Switchboard's proxy, so a route change cannot reach it. Use target route to prepare a managed pool, or target claude_cli with global: true to change the ordinary Claude Code login.".into()),
                    }
                }
                self.call(Operation::Select {
                    provider,
                    pool: pool.clone(),
                    id: id.into(),
                })
                .await?;
                Ok(
                    json!({"action":"selected","account":{"id":id,"label":label,"pool":pool},"message":format!("{label} handles the next request in the {pool} pool. A response already streaming keeps its account. Manual choice starts the rotation cooldown.")}),
                )
            }
            _ => Err("target must be session, route or claude_cli.".into()),
        }
    }
    async fn project_edit(&self, args: &Map<String, Value>) -> Result<Value, String> {
        let strings = |key: &str| -> Result<Vec<String>, String> {
            match args.get(key) {
                None => Ok(vec![]),
                Some(Value::Array(items)) => items
                    .iter()
                    .map(|v| {
                        v.as_str()
                            .map(str::to_owned)
                            .ok_or(format!("{key} must list strings."))
                    })
                    .collect(),
                Some(_) => Err(format!("{key} must be a list.")),
            }
        };
        let absolute = |key: &str| -> Result<Vec<PathBuf>, String> {
            strings(key)?
                .into_iter()
                .map(|f| {
                    let p = PathBuf::from(&f);
                    if p.is_absolute() {
                        Ok(p)
                    } else {
                        Err(format!("{key} takes absolute folders."))
                    }
                })
                .collect()
        };
        let project = self
            .call(Operation::ProjectEdit {
                pool: required(args, "pool")?.into(),
                name: args.get("name").and_then(Value::as_str).map(str::to_owned),
                add_accounts: strings("add_accounts")?,
                remove_accounts: strings("remove_accounts")?,
                add_folders: absolute("add_folders")?,
                remove_folders: absolute("remove_folders")?,
            })
            .await?;
        Ok(
            json!({"project": project, "message": "Project saved. It is visible in the Switchboard app under Projects."}),
        )
    }
    async fn account_update(&self, args: &Map<String, Value>) -> Result<Value, String> {
        let label = args.get("label").and_then(Value::as_str).map(str::to_owned);
        let enabled = args.get("enabled").and_then(Value::as_bool);
        if label.is_none() && enabled.is_none() {
            return Err("Name a label or enabled to change.".into());
        }
        self.call(Operation::Update {
            id: required(args, "account_id")?.into(),
            label,
            enabled,
        })
        .await
    }
    async fn account_move(&self, args: &Map<String, Value>) -> Result<Value, String> {
        self.call(Operation::MoveAccount {
            id: required(args, "account_id")?.into(),
            pool: required(args, "pool")?.into(),
        })
        .await
    }
    async fn rotation_set(&self, args: &Map<String, Value>) -> Result<Value, String> {
        let provider: Provider =
            serde_json::from_value(args.get("provider").cloned().unwrap_or(Value::Null))
                .map_err(|_| "provider must be claude or codex.")?;
        let pool = args
            .get("pool")
            .and_then(Value::as_str)
            .unwrap_or("default")
            .to_owned();
        let target = args
            .get("target")
            .and_then(Value::as_str)
            .unwrap_or("managed")
            .to_owned();
        if target != "managed" && target != "claude_cli" {
            return Err("target is managed or claude_cli.".into());
        }
        // Merge with the saved policy so one field never resets the others (as the CLI does).
        let saved = self.call(Operation::Snapshot).await?["policies"]
            .as_array()
            .and_then(|ps| {
                ps.iter().find(|p| {
                    p["provider"] == provider.as_str()
                        && p["pool"] == pool.as_str()
                        && p["target"] == target.as_str()
                })
            })
            .map(|p| serde_json::from_value::<switchboard_core::RotationPolicy>(p.clone()))
            .transpose()
            .map_err(|_| "Saved rotation policy is unreadable.")?;
        let mut policy = saved.unwrap_or(switchboard_core::RotationPolicy {
            provider,
            pool,
            target,
            enabled: false,
            threshold_percent: 90.0,
            hysteresis_percent: 10.0,
            cooldown_seconds: 1800,
            max_age_seconds: 300,
            last_switched_at: None,
        });
        if let Some(v) = args.get("enabled").and_then(Value::as_bool) {
            policy.enabled = v;
        }
        if let Some(v) = args.get("threshold_percent").and_then(Value::as_f64) {
            policy.threshold_percent = v;
        }
        if let Some(v) = args.get("hysteresis_percent").and_then(Value::as_f64) {
            policy.hysteresis_percent = v;
        }
        if let Some(v) = args
            .get("cooldown_seconds")
            .and_then(Value::as_i64)
            .filter(|v| *v >= 0)
        {
            policy.cooldown_seconds = v;
        }
        if let Some(v) = args
            .get("max_age_seconds")
            .and_then(Value::as_i64)
            .filter(|v| *v >= 30)
        {
            policy.max_age_seconds = v;
        }
        self.call(Operation::SetPolicy { policy }).await
    }
    async fn agent_set(&self, args: &Map<String, Value>) -> Result<Value, String> {
        let mut answer = json!({});
        if let Some(enabled) = args.get("enabled").and_then(Value::as_bool) {
            answer = self
                .call(Operation::AgentEnable {
                    agent: required(args, "agent")?.into(),
                    enabled,
                })
                .await?;
        }
        if let Some(default) = args.get("default") {
            let agent = match default {
                Value::Null => None,
                Value::String(a) if a == "auto" => None,
                Value::String(a) => Some(a.clone()),
                _ => return Err("default is an agent id or null.".into()),
            };
            answer = self.call(Operation::AgentDefault { agent }).await?;
        }
        if answer == json!({}) {
            return Err("Name enabled (with agent) or default to change.".into());
        }
        Ok(answer)
    }
    async fn login(&self, args: &Map<String, Value>) -> Result<Value, String> {
        let id = || required(args, "login_id").map(str::to_owned);
        match required(args, "action")? {
            "begin" => {
                let provider: Provider =
                    serde_json::from_value(args.get("provider").cloned().unwrap_or(Value::Null))
                        .map_err(|_| "provider must be claude or codex.")?;
                self.call(Operation::BeginLogin {
                    provider,
                    label: required(args, "label")?.into(),
                    pool: args
                        .get("pool")
                        .and_then(Value::as_str)
                        .unwrap_or("default")
                        .into(),
                })
                .await
            }
            "status" => self.call(Operation::LoginStatus { login_id: id()? }).await,
            "finish" => self.call(Operation::FinishLogin { login_id: id()? }).await,
            "cancel" => self.call(Operation::CancelLogin { login_id: id()? }).await,
            _ => Err("action is begin, status, finish or cancel.".into()),
        }
    }
    async fn project_set(&self, args: &Map<String, Value>) -> Result<Value, String> {
        let hours = match args.get("expires_in_hours") {
            None => None,
            Some(v) => Some(
                v.as_i64()
                    .filter(|h| (1..=720).contains(h))
                    .ok_or("expires_in_hours must be 1 to 720.")?,
            ),
        };
        let rule = self
            .call(Operation::SetProjectRule {
                path: self.folder(args)?,
                account_id: required(args, "account_id")?.into(),
                target: args
                    .get("target")
                    .and_then(Value::as_str)
                    .unwrap_or("managed")
                    .into(),
                enabled: args.get("enabled").and_then(Value::as_bool).unwrap_or(true),
                expires_at: hours.map(|h| now() + h * 3600),
            })
            .await?;
        Ok(
            json!({"rule": rule, "message": "Rule saved. It is visible in the Switchboard app under Projects and can be paused there."}),
        )
    }
    async fn project_remove(&self, args: &Map<String, Value>) -> Result<Value, String> {
        let provider: Provider =
            serde_json::from_value(args.get("provider").cloned().unwrap_or(Value::Null))
                .map_err(|_| "provider must be claude or codex.")?;
        let rule = self
            .call(Operation::RemoveProjectRule {
                path: self.folder(args)?,
                provider,
            })
            .await?;
        Ok(json!({"removed": rule}))
    }
    async fn project_apply(&self, args: &Map<String, Value>) -> Result<Value, String> {
        let global = args.get("global").and_then(Value::as_bool).unwrap_or(false);
        let session = self.session().await.0;
        self.call(Operation::ApplyProject {
            path: self.folder(args)?,
            session,
            global,
        })
        .await
    }

    async fn call_tool(
        &self,
        name: &str,
        args: &Map<String, Value>,
    ) -> Option<Result<Value, String>> {
        let (write, _) = tools().into_iter().find(|(_, t)| t["name"] == name)?;
        if write && self.read_only {
            return None;
        }
        Some(match name {
            "switchboard_status" => self.status().await,
            "switchboard_accounts" => self.accounts(args).await,
            "switchboard_usage" => self.usage(args).await,
            "switchboard_switch" => self.switch(args).await,
            "switchboard_sessions" => {
                self.call(Operation::Sessions {
                    all: args.get("all").and_then(Value::as_bool).unwrap_or(false),
                })
                .await
            }
            "switchboard_onboarding" => match self.folder(args) {
                Ok(folder) => {
                    let host = match args.get("host").and_then(Value::as_str) {
                        None => None,
                        Some("claude") => Some(onboarding::Host::Claude),
                        Some("codex") => Some(onboarding::Host::Codex),
                        Some(_) => return Some(Err("host is claude or codex.".into())),
                    };
                    crate::onboard_status(&self.root, folder, host).await
                }
                Err(error) => Err(error),
            },
            "switchboard_project_context" => match self.folder(args) {
                Ok(path) => self.call(Operation::ResolveProject { path }).await,
                Err(error) => Err(error),
            },
            "switchboard_projects" => self
                .call(Operation::Snapshot)
                .await
                .map(|v| json!({"projects": project_views(&v), "rules": rule_views(&v, now())})),
            "switchboard_project_edit" => self.project_edit(args).await,
            "switchboard_events" => {
                let limit = args
                    .get("limit")
                    .and_then(Value::as_u64)
                    .unwrap_or(50)
                    .clamp(1, 500) as usize;
                self.call(Operation::Snapshot).await.map(|v| {
                    let events: Vec<Value> = v["events"]
                        .as_array()
                        .map(|e| e.iter().rev().take(limit).cloned().collect())
                        .unwrap_or_default();
                    json!({"events": events})
                })
            }
            "switchboard_account_update" => self.account_update(args).await,
            "switchboard_account_move" => self.account_move(args).await,
            "switchboard_rotation" => match self.call(Operation::Snapshot).await {
                Ok(v) => {
                    let monitor = self.call(Operation::MonitorStatus).await.unwrap_or(
                        json!({"running": false, "note": "The app is not running: nothing rotates now."}),
                    );
                    Ok(json!({"policies": v["policies"], "monitor": monitor}))
                }
                Err(e) => Err(e),
            },
            "switchboard_rotation_set" => self.rotation_set(args).await,
            "switchboard_backups" => self.call(Operation::Backups).await,
            "switchboard_backup_now" => self.call(Operation::BackupNow).await,
            "switchboard_recovery" => self.call(Operation::Recovery).await,
            "switchboard_handovers" => self.call(Operation::Handovers).await,
            "switchboard_agent_set" => self.agent_set(args).await,
            "switchboard_login" => self.login(args).await,
            "switchboard_project_set" => self.project_set(args).await,
            "switchboard_project_remove" => self.project_remove(args).await,
            "switchboard_project_apply" => self.project_apply(args).await,
            "switchboard_chain_get" => {
                self.call(Operation::Chains {
                    workflow: args
                        .get("workflow_id")
                        .and_then(Value::as_str)
                        .map(str::to_owned),
                    pool: args.get("pool").and_then(Value::as_str).map(str::to_owned),
                })
                .await
            }
            "switchboard_chain_set" => self.chain_set(args).await,
            "switchboard_openrouter_status" => {
                self.call(Operation::AgentKeyStatus {
                    service: "openrouter".into(),
                    credit: true,
                })
                .await
            }
            "switchboard_agents" => self.call(Operation::AgentsOverview).await,
            "switchboard_kimi_accounts" => self.call(Operation::KimiAccounts).await,
            "switchboard_hermes_status" => self.call(Operation::HermesModel).await,
            "switchboard_hermes_set" => {
                self.call(Operation::HermesSetModel {
                    provider: args
                        .get("provider")
                        .and_then(Value::as_str)
                        .map(str::to_owned),
                    model: args.get("model").and_then(Value::as_str).map(str::to_owned),
                })
                .await
            }
            "switchboard_openrouter_model" => match args.get("model").and_then(Value::as_str) {
                Some(model) => {
                    self.call(Operation::AgentKeyModel {
                        service: "openrouter".into(),
                        model: model.to_owned(),
                    })
                    .await
                }
                None => Err("Name the model.".into()),
            },
            _ => return None,
        })
    }

    async fn chain_set(&self, args: &Map<String, Value>) -> Result<Value, String> {
        let scope_text = args
            .get("scope")
            .and_then(Value::as_str)
            .unwrap_or("machine");
        let scope = match scope_text.split_once(':') {
            None if scope_text == "machine" => switchboard_core::ChainScope::Machine,
            Some(("project", pool)) if !pool.is_empty() => {
                switchboard_core::ChainScope::Project { pool: pool.into() }
            }
            Some(("workflow", id)) if !id.is_empty() => {
                switchboard_core::ChainScope::Workflow { id: id.into() }
            }
            _ => return Err("scope is machine, project:<pool> or workflow:<wf_id>.".into()),
        };
        let executors: Vec<switchboard_core::Executor> = match args.get("executors") {
            Some(list) => serde_json::from_value(list.clone()).map_err(|_| {
                "executors: each needs agent, and at most one of account_id or key."
            })?,
            None => Vec::new(),
        };
        self.call(Operation::SetChain {
            scope,
            executors,
            preset: args
                .get("preset")
                .and_then(Value::as_str)
                .map(str::to_owned),
        })
        .await
    }

    /// One JSON-RPC message in, at most one out.
    pub async fn handle(&self, message: Value) -> Option<Value> {
        let id = message.get("id").cloned();
        let Some(method) = message.get("method").and_then(Value::as_str) else {
            return id.map(|id| error(id, -32600, "Invalid request."));
        };
        let id = id?; // Notifications (initialized, cancelled) need no answer.
        let params = message.get("params").cloned().unwrap_or(json!({}));
        Some(match method {
            "initialize" => {
                if let (Some(name), Ok(mut client)) =
                    (params["clientInfo"]["name"].as_str(), self.client.lock())
                {
                    *client = Some(name.to_owned());
                }
                let asked = params["protocolVersion"].as_str().unwrap_or_default();
                let version = VERSIONS
                    .iter()
                    .find(|v| **v == asked)
                    .copied()
                    .unwrap_or(VERSIONS[0]);
                ok(
                    id,
                    json!({"protocolVersion": version, "capabilities": {"tools": {"listChanged": false}},
                    "serverInfo": {"name": "switchboard", "title": "Switchboard", "version": env!("CARGO_PKG_VERSION")},
                    "instructions": INSTRUCTIONS}),
                )
            }
            "ping" => ok(id, json!({})),
            "tools/list" => ok(
                id,
                json!({"tools": tools().into_iter().filter(|(w, _)| !(*w && self.read_only)).map(|(_, t)| t).collect::<Vec<_>>()}),
            ),
            "tools/call" => {
                let name = params["name"].as_str().unwrap_or_default();
                let empty = Map::new();
                let args = match params.get("arguments") {
                    None | Some(Value::Null) => &empty,
                    Some(Value::Object(args)) => args,
                    Some(_) => return Some(error(id, -32602, "arguments must be an object.")),
                };
                match self.call_tool(name, args).await {
                    None => error(id, -32602, "Unknown tool."),
                    Some(Ok(value)) => {
                        // The first answered call proves the connection (SB-93); once per server.
                        if !self.proved.swap(true, std::sync::atomic::Ordering::Relaxed) {
                            let client = self.client.lock().ok().and_then(|c| c.clone());
                            onboarding::record_proof(&self.root, client.as_deref(), now());
                        }
                        ok(
                            id,
                            json!({"content": [{"type": "text", "text": value.to_string()}], "structuredContent": value, "isError": false}),
                        )
                    }
                    Some(Err(message)) => ok(
                        id,
                        json!({"content": [{"type": "text", "text": message}], "isError": true}),
                    ),
                }
            }
            _ => error(id, -32601, "Method not found."),
        })
    }

    pub async fn serve(self) -> Result<(), String> {
        let mut input = BufReader::new(tokio::io::stdin());
        let mut output = tokio::io::stdout();
        let mut line = Vec::new();
        loop {
            line.clear();
            let read = (&mut input)
                .take(LINE_LIMIT + 1)
                .read_until(b'\n', &mut line)
                .await
                .map_err(|_| "MCP input unavailable.")?;
            if read == 0 {
                return Ok(());
            }
            let reply = if line.len() as u64 > LINE_LIMIT {
                // Drop the rest of an oversized message before answering once.
                let mut rest = Vec::new();
                while !line.ends_with(b"\n") && !rest.ends_with(b"\n") {
                    rest.clear();
                    if (&mut input)
                        .take(LINE_LIMIT)
                        .read_until(b'\n', &mut rest)
                        .await
                        .map_err(|_| "MCP input unavailable.")?
                        == 0
                    {
                        break;
                    }
                }
                Some(error(Value::Null, -32700, "Message too large."))
            } else if line.iter().all(u8::is_ascii_whitespace) {
                continue;
            } else {
                match serde_json::from_slice::<Value>(&line) {
                    Ok(message @ Value::Object(_)) => self.handle(message).await,
                    Ok(_) => Some(error(Value::Null, -32600, "Invalid request.")),
                    Err(_) => Some(error(Value::Null, -32700, "Parse error.")),
                }
            };
            if let Some(reply) = reply {
                let mut bytes = reply.to_string().into_bytes();
                bytes.push(b'\n');
                output
                    .write_all(&bytes)
                    .await
                    .map_err(|_| "MCP output unavailable.")?;
                output
                    .flush()
                    .await
                    .map_err(|_| "MCP output unavailable.")?;
            }
        }
    }
}

fn ok(id: Value, result: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}
fn error(id: Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}
fn required<'a>(args: &'a Map<String, Value>, key: &str) -> Result<&'a str, String> {
    args.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("{key} is required."))
}
fn find<'a>(snapshot: &'a Value, id: &str) -> Result<&'a Value, String> {
    snapshot["accounts"]
        .as_array()
        .and_then(|a| a.iter().find(|a| a["id"] == id))
        .ok_or_else(|| format!("No account with id {id}. Call switchboard_accounts."))
}

/// Remaining = 100 − used for each reported window. Stale when older than the pool's
/// freshest enabled policy allows (`UNPOLICED_MAX_AGE_SECONDS`, 900 s, without one), or once a window's reset has passed.
fn usage_view(account: &Value, snapshot: &Value, time: i64) -> Value {
    let health = &account["usage_health"];
    let base = json!({"health": health["status"], "checked_at": health["checked_at"].as_i64().map(rfc3339), "next_check_at": health["next_check_at"].as_i64().map(rfc3339)});
    let Some(usage) = account["usage"].as_object() else {
        let mut view = base;
        view["known"] = json!(false);
        view["fresh"] = json!(false);
        view["lowest_remaining_percent"] = Value::Null;
        view["windows"] = json!([]);
        view["reason"] = json!(if account["kind"] == "oauth" {
            "No observation yet."
        } else {
            "Quota is reported for OAuth subscription accounts only; API billing is separate."
        });
        return view;
    };
    let observed = usage
        .get("observed_at")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let max_age = snapshot["policies"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|p| {
            p["enabled"] == true
                && p["provider"] == account["provider"]
                && p["pool"] == account["pool"]
        })
        .filter_map(|p| p["max_age_seconds"].as_i64())
        .min()
        .unwrap_or(switchboard_core::UNPOLICED_MAX_AGE_SECONDS);
    let windows: Vec<Value> = usage
        .get("windows")
        .and_then(Value::as_array)
        .filter(|w| !w.is_empty())
        .cloned()
        .unwrap_or_else(|| vec![json!({"name": "highest", "used_percent": usage.get("used_percent"), "resets_at": usage.get("resets_at")})]);
    let mut reset_passed = false;
    let windows: Vec<Value> = windows
        .iter()
        .map(|w| {
            let used = w["used_percent"].as_f64().unwrap_or(100.0);
            let resets = w["resets_at"].as_i64();
            let feature = w["name"]
                .as_str()
                .is_some_and(|n| n.starts_with(switchboard_core::FEATURE_WINDOW_PREFIX));
            // A feature window's reset does not make the account's figures stale (SB-40).
            reset_passed |= !feature && resets.is_some_and(|t| t <= time);
            // A metered feature's limit does not stand for the account (SB-40).
            let scope = if w["name"]
                .as_str()
                .is_some_and(|n| n.starts_with(switchboard_core::FEATURE_WINDOW_PREFIX))
            {
                "feature"
            } else {
                "account"
            };
            json!({"name": w["name"], "scope": scope, "used_percent": used, "remaining_percent": ((100.0 - used) * 10.0).round() / 10.0, "resets_at": resets.map(rfc3339)})
        })
        .collect();
    // The account's remaining capacity; null when only feature limits were reported.
    let lowest = windows
        .iter()
        .filter(|w| w["scope"] == "account")
        .filter_map(|w| w["remaining_percent"].as_f64())
        .reduce(f64::min);
    let age = (time - observed).max(0);
    let mut view = base;
    view["known"] = json!(true);
    view["observed_at"] = rfc3339(observed);
    view["age_seconds"] = json!(age);
    view["fresh"] = json!(age <= max_age && !reset_passed && health["status"] != "failed");
    view["lowest_remaining_percent"] = json!(lowest);
    view["windows"] = json!(windows);
    view["source"] = usage.get("source").cloned().unwrap_or(Value::Null);
    if reset_passed {
        view["reason"] = json!(
            "A window has reset since this observation; the next check reports the new value."
        );
    }
    view
}

/// Projects with their accounts (no credential), from a snapshot.
pub(crate) fn project_views(snapshot: &Value) -> Vec<Value> {
    let accounts = snapshot["accounts"].as_array().cloned().unwrap_or_default();
    snapshot["projects"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|p| {
            let own: Vec<Value> = accounts
                .iter()
                .filter(|a| a["pool"] == p["pool"])
                .map(|a| json!({"id": a["id"], "label": a["label"], "provider": a["provider"], "enabled": a["enabled"]}))
                .collect();
            // SB-135: the folders as roots with their worktrees and clones under them.
            let folders: Vec<String> = p["folders"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|f| f.as_str().map(str::to_owned))
                .collect();
            let roots = switchboard_runtime::roots::tree(&folders);
            json!({"pool": p["pool"], "name": p["name"], "folders": p["folders"], "roots": roots, "sources": p["sources"].as_array().cloned().unwrap_or_default(), "accounts": own})
        })
        .collect()
}
pub(crate) fn rule_views(snapshot: &Value, time: i64) -> Vec<Value> {
    let accounts = snapshot["accounts"].as_array().cloned().unwrap_or_default();
    snapshot["rules"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|r| {
            let expired = r["expires_at"].as_i64().is_some_and(|t| t <= time);
            let state = if r["enabled"] != true { "paused" } else if expired { "expired" } else { "active" };
            let account = accounts.iter().find(|a| a["id"] == r["account_id"]);
            json!({"path": r["path"], "provider": r["provider"], "target": r["target"], "state": state,
                "expires_at": r["expires_at"].as_i64().map(rfc3339),
                "account": account.map(|a| json!({"id": a["id"], "label": a["label"], "pool": a["pool"]}))})
        })
        .collect()
}

#[cfg(test)]
mod usage_view_tests {
    use super::*;

    /// SB-40: an agent reading `switchboard_usage` sees which window limits one feature, and the
    /// account's remaining capacity ignores it; only feature limits leave it unknown.
    #[test]
    fn feature_windows_are_scoped_and_leave_the_account_minimum_alone() {
        let time = 2_000_000_000;
        let account = |windows: Value| {
            json!({"id": "a", "provider": "codex", "pool": "default", "kind": "oauth",
                "usage_health": {"status": "ok", "checked_at": time - 10, "next_check_at": time + 170},
                "usage": {"used_percent": 100.0, "observed_at": time - 10, "resets_at": time + 3600,
                          "source": "codex_oauth", "windows": windows}})
        };
        let snapshot = json!({"policies": []});
        let view = usage_view(
            &account(json!([
                {"name": "primary", "used_percent": 30.0, "resets_at": time + 3600},
                {"name": "feature_codex_other_primary", "used_percent": 100.0, "resets_at": time + 600}
            ])),
            &snapshot,
            time,
        );
        assert_eq!(view["lowest_remaining_percent"], json!(70.0));
        assert_eq!(view["windows"][0]["scope"], "account");
        assert_eq!(view["windows"][1]["scope"], "feature");
        let only = usage_view(
            &account(
                json!([{"name": "feature_codex_other_primary", "used_percent": 100.0, "resets_at": time + 600}]),
            ),
            &snapshot,
            time,
        );
        assert!(
            only["lowest_remaining_percent"].is_null(),
            "unknown, not zero"
        );
    }
}
