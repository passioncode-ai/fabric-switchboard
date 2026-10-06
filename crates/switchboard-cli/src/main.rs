//! Headless account operations; credentials are accepted only from bounded stdin.
mod mcp;
use clap::{Parser, Subcommand, ValueEnum};
use serde_json::{json, Value};
use std::{
    io::{self, IsTerminal, Read},
    path::PathBuf,
    process::ExitCode,
};
use switchboard_core::{AuthKind, Provider, RotationPolicy};
use switchboard_runtime::{
    arm_hard_exit, control, default_root, execute_offline, oplog,
    projects::{detect_session, Session},
    rfc3339, Operation, Owner, StopSignals, DRAIN_DEADLINE, HARD_EXIT_AFTER,
};

#[derive(Parser)]
#[command(
    name = "switchboard",
    version,
    about = "Manage Claude and Codex accounts with Fabric Switchboard",
    after_help = "Secrets: pipe a token or auth JSON to accounts add --secret-stdin. Never put secrets in arguments.\nLogin and launch: keep the desktop app or switchboard serve running in another terminal.\nManaged sessions switch identity at the next request; existing streams are not replayed."
)]
struct Cli {
    /// Emit machine-readable JSON. Errors go to stderr.
    #[arg(long, global = true)]
    json: bool,
    /// Absolute private app-data directory; defaults to the desktop app's directory.
    #[arg(long, global = true)]
    data_dir: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
    /// List and manage accounts. No command prints credentials.
    Accounts {
        #[command(subcommand)]
        command: Accounts,
    },
    /// Cached usage for all accounts, or refresh a specific account from its provider.
    Usage {
        /// An account id (`switchboard accounts list`): ask its provider now. Without it, the
        /// saved usage of every account.
        id: Option<String>,
    },
    /// Recent sanitized activity.
    #[command(alias = "activity")]
    Events,
    /// Report online/offline mode and the proxy address, without its capability.
    Status,
    /// Inspect the actual signed-in accounts in the ordinary provider CLIs.
    Current,
    /// Configure quota-based switching while the desktop or serve is running.
    Rotation {
        #[command(subcommand)]
        command: Rotation,
    },
    /// Official isolated CLI sign-in; requires a running desktop app or serve.
    Login {
        #[command(subcommand)]
        command: Login,
    },
    /// Launch an isolated or managed provider CLI in a project directory: in a new Terminal
    /// window, or with --in-place in this terminal (for an embedded console). Requires a running
    /// desktop app or serve.
    Launch {
        /// The account id (`switchboard accounts list`); or give --provider instead.
        id: Option<String>,
        /// Use the account Switchboard would use for this folder: its project's selected account,
        /// else the folder's rule, else the default pool's selected account.
        #[arg(long, value_enum, conflicts_with = "id")]
        provider: Option<ProviderArg>,
        /// `isolated`: the account's private home, straight to the provider. `managed`: through
        /// the local proxy, which follows the pool's selected account.
        #[arg(long, value_enum, default_value = "isolated")]
        mode: Mode,
        /// The project folder the session starts in (absolute).
        #[arg(long)]
        working_directory: PathBuf,
        /// Run the session in this terminal instead of opening Terminal (needs a terminal on
        /// stdin and stdout). The same session, home and tools as a Terminal launch.
        #[arg(long)]
        in_place: bool,
        /// Arguments for the agent, after `--` (for example `-- --resume <id>`); with --in-place.
        #[arg(last = true)]
        args: Vec<String>,
    },
    /// Continue an Observatory workflow whose executor ran out of its limit on another account —
    /// of the same provider, or of the other one (Claude Code ↔ Codex) with the same context:
    /// reads the workflow, offers it (reason `limit`) and launches the account's session, which
    /// accepts the handoff itself. Requires a running desktop app or serve.
    Continue {
        /// The workflow id, `wf_` and 16 hex digits (`project-observatory full workflow list`).
        workflow_id: String,
        /// The account that continues it.
        #[arg(long)]
        account: String,
        /// `isolated` or `managed`, as for `launch`.
        #[arg(long, value_enum, default_value = "isolated")]
        mode: Mode,
        /// The workflow's checkout to run in.
        #[arg(long)]
        dir: PathBuf,
    },
    /// Own the vault, inference proxy and private CLI control listener until Ctrl-C or
    /// SIGTERM, then stop within ten seconds.
    Serve,
    /// Other coding agents (Hermes, Kilo Code, Cline, Goose, OpenCode, …): how each connects to
    /// `switchboard mcp` and the proxy, and launching those configured by environment alone.
    Agents {
        #[command(subcommand)]
        command: AgentsCommand,
    },
    /// Fallback chains: which agents continue a workflow, in what order, when its accounts run
    /// out — per machine, per project or per task. None applies until you set one.
    Chain {
        #[command(subcommand)]
        command: ChainCommand,
    },
    /// Projects (folders that reserve their own accounts) and optional project rules.
    Project {
        #[command(subcommand)]
        command: Project,
    },
    /// Encrypted backups (accounts, projects, rules, settings) in the "Fabric Switchboard Backups"
    /// folder: ~/Library/Application Support on macOS, %APPDATA% on Windows (this machine's key).
    Backup {
        #[command(subcommand)]
        command: Backup,
    },
    /// Serve the Switchboard tools to a coding agent over MCP on stdin/stdout.
    Mcp {
        /// List only the tools that read: status, accounts, usage, project rules.
        #[arg(long)]
        read_only: bool,
    },
    /// Remove what Switchboard created on this machine: saved accounts' credentials, the
    /// ~/.local/bin link, the login item and its data folder. Session homes keep the CLIs'
    /// history (their credential files go) unless --purge. Shows the plan unless --yes. Backups,
    /// their key and the ordinary Claude Code and Codex sign-ins are never touched: the next
    /// install restores the newest backup on its own. Quit the app first.
    Uninstall {
        /// Keep the data folder (accounts list, settings); remove credentials and the link only.
        #[arg(long)]
        keep_data: bool,
        /// Also remove the session homes and the Claude Code / Codex history inside them.
        #[arg(long)]
        purge: bool,
        /// Remove for real; without it nothing changes.
        #[arg(long)]
        yes: bool,
    },
}
#[derive(Subcommand)]
enum Project {
    /// Projects (folders and reserved accounts), then every saved rule with its state: active,
    /// paused or expired.
    List,
    /// The rule that applies to a folder (default: the current folder).
    Show {
        /// An absolute folder; default: the current folder.
        #[arg(long)]
        path: Option<PathBuf>,
    },
    /// Save a rule for a folder and its subfolders. Rules never stop rotation.
    Set {
        /// An absolute folder; default: the current folder.
        #[arg(long)]
        path: Option<PathBuf>,
        /// The account sessions in this folder start on (`switchboard accounts list`).
        #[arg(long)]
        account: String,
        /// `managed`: the account managed sessions here use. `claude-cli`: the ordinary Claude
        /// Code login, changed when the rule is applied with --global.
        #[arg(long, value_enum, default_value = "managed")]
        target: RotationTarget,
        /// Save the rule switched off.
        #[arg(long)]
        paused: bool,
        /// End the rule after this many hours (1–720); without it, it lasts until paused.
        #[arg(long, value_parser = clap::value_parser!(u32).range(1..=720))]
        expires_in_hours: Option<u32>,
    },
    /// Remove a folder's rule for one provider.
    Remove {
        /// An absolute folder; default: the current folder.
        #[arg(long)]
        path: Option<PathBuf>,
        /// The provider whose rule goes.
        #[arg(long, value_enum)]
        provider: ProviderArg,
    },
    /// Create or update a project: its folders (repositories) and the accounts reserved for
    /// them. The listed accounts are the project's whole set; one left out goes back to the
    /// `default` pool.
    Save {
        /// Update the project with this pool; without it, a new project is created.
        #[arg(long)]
        pool: Option<String>,
        /// The project's name, as the app shows it.
        #[arg(long)]
        name: String,
        /// An absolute folder of the project (a repository); repeat for each.
        #[arg(long = "folder", required = true)]
        folders: Vec<PathBuf>,
        /// An account reserved for the project; repeat for each.
        #[arg(long = "account")]
        accounts: Vec<String>,
    },
    /// Delete a project. Its accounts stay in its pool, no longer reserved.
    Delete {
        /// The project's pool (`switchboard project list`).
        #[arg(long)]
        pool: String,
    },
    /// Apply the folder's rule to the session this command runs in.
    Apply {
        /// An absolute folder; default: the current folder.
        #[arg(long)]
        path: Option<PathBuf>,
        /// Allow changing the ordinary Claude Code login for every claude session.
        #[arg(long)]
        global: bool,
    },
}
fn folder(path: &Option<PathBuf>) -> Result<PathBuf, String> {
    match path {
        Some(path) if path.is_absolute() => Ok(path.clone()),
        Some(_) => Err("--path must be an absolute folder.".into()),
        None => {
            std::env::current_dir().map_err(|_| "Current folder unavailable. Pass --path.".into())
        }
    }
}
#[derive(Subcommand)]
enum Accounts {
    /// Saved accounts with their provider, kind, pool, state and usage. No credentials.
    List,
    /// Save the current CLI authorization; no new sign-in is started.
    Capture {
        /// The CLI whose signed-in account is saved.
        #[arg(long, value_enum)]
        provider: ProviderArg,
        /// A name for it; default: its e-mail.
        #[arg(long)]
        label: Option<String>,
        /// The pool it joins.
        #[arg(long, default_value = "default")]
        pool: String,
    },
    /// Import existing Claude Swap backups into the native credential vault.
    ImportClaudeSwap {
        /// The pool the imported accounts join.
        #[arg(long, default_value = "default")]
        pool: String,
    },
    /// Activate a captured Claude OAuth profile in the ordinary Claude Code CLI.
    Activate {
        /// The account id (`switchboard accounts list`).
        id: String,
    },
    /// Save an API key, a Claude setup token or an auth JSON piped on stdin.
    Add {
        /// The provider the credential belongs to.
        #[arg(long, value_enum)]
        provider: ProviderArg,
        /// What is piped: `api-key`, `setup-token` or `oauth` (an auth JSON).
        #[arg(long, value_enum)]
        kind: Kind,
        /// A name for the account.
        #[arg(long)]
        label: String,
        /// The pool it joins.
        #[arg(long, default_value = "default")]
        pool: String,
        /// Read one token or auth JSON from piped stdin, limited to 64 KiB.
        #[arg(long, required = true)]
        secret_stdin: bool,
    },
    /// Rename an account, enable or disable it, or both; what is left out stays as it is.
    Update {
        /// The account id (`switchboard accounts list`).
        id: String,
        /// The new name.
        #[arg(long)]
        label: Option<String>,
        /// `true` or `false`. A disabled account is never selected or switched to.
        #[arg(long, action = clap::ArgAction::Set)]
        enabled: Option<bool>,
    },
    /// Remove an account and its stored credential. Select another one first if it is selected.
    Remove {
        /// The account id (`switchboard accounts list`).
        id: String,
    },
    /// Make an account the one the pool's managed sessions use, from their next request.
    Select {
        /// The account id (`switchboard accounts list`).
        id: String,
        /// The account's provider.
        #[arg(long, value_enum)]
        provider: ProviderArg,
        /// The pool whose managed sessions follow it.
        #[arg(long, default_value = "default")]
        pool: String,
    },
}
#[derive(Subcommand)]
enum Rotation {
    /// Show persisted policies and the scheduler's latest decisions.
    Status,
    /// Save a policy; omitted flags keep the saved values (new policy: off, 90, 10, 1800, 300).
    /// Existing responses are never replayed or interrupted.
    Set {
        /// The provider the policy is for.
        #[arg(long, value_enum)]
        provider: ProviderArg,
        /// The pool it switches within.
        #[arg(long, default_value = "default")]
        pool: String,
        /// `managed`: the pool's managed sessions. `claude-cli`: the ordinary Claude Code login.
        #[arg(long, value_enum, default_value = "managed")]
        target: RotationTarget,
        /// `true` or `false`: switch automatically or not.
        #[arg(long, action = clap::ArgAction::Set)]
        enabled: Option<bool>,
        /// Percent used (of the busiest quota window) at which it moves on.
        #[arg(long)]
        threshold: Option<f64>,
        /// Percentage points of headroom the next account must have over the current one.
        #[arg(long)]
        hysteresis: Option<f64>,
        /// Seconds to wait after a switch before the next one.
        #[arg(long)]
        cooldown: Option<i64>,
        /// Seconds after which a usage reading is too old to switch on.
        #[arg(long)]
        max_age: Option<i64>,
    },
}
#[derive(Clone, Copy, ValueEnum)]
enum RotationTarget {
    Managed,
    ClaudeCli,
}
#[derive(Subcommand)]
enum ChainCommand {
    /// Every chain set, the presets, and the chain that applies to a workflow or a project.
    List {
        /// A workflow id (`wf_` and 16 hex digits): show the chain that applies to it.
        #[arg(long)]
        workflow: Option<String>,
        /// A project's pool: show the chain that applies to it.
        #[arg(long)]
        pool: Option<String>,
    },
    /// Set a chain in order. Each agent is a catalog id (`switchboard agents list`), optionally
    /// pinned: `codex@ACCOUNT_ID` runs on that account, `kimi-code#project/env/NAME` on a paid
    /// key named in the Observatory vault. Without a pin Switchboard picks an account at the switch.
    Set {
        /// `machine`, `project:<pool>` or `workflow:<wf_id>`.
        #[arg(long, default_value = "machine")]
        scope: String,
        /// A ready-made order instead of a list: `subscriptions-first` (claude-code, codex,
        /// kimi-code, hermes).
        #[arg(long)]
        preset: Option<String>,
        /// The agents, first tried first.
        agents: Vec<String>,
    },
    /// Remove the chain of a scope.
    Clear {
        /// `machine`, `project:<pool>` or `workflow:<wf_id>`.
        #[arg(long, default_value = "machine")]
        scope: String,
    },
}
/// `machine`, `project:<pool>` or `workflow:<wf_id>`.
fn chain_scope(text: &str) -> Result<switchboard_core::ChainScope, String> {
    use switchboard_core::ChainScope;
    match text.split_once(':') {
        None if text == "machine" => Ok(ChainScope::Machine),
        Some(("project", pool)) if !pool.is_empty() => {
            Ok(ChainScope::Project { pool: pool.into() })
        }
        Some(("workflow", id)) if !id.is_empty() => Ok(ChainScope::Workflow { id: id.into() }),
        _ => Err("Scope is machine, project:<pool> or workflow:<wf_id>.".into()),
    }
}
/// `agent`, `agent@ACCOUNT_ID` or `agent#project/env/NAME`.
fn chain_executor(text: &str) -> switchboard_core::Executor {
    let (agent, account_id, key) = if let Some((a, id)) = text.split_once('@') {
        (a, Some(id.to_owned()), None)
    } else if let Some((a, k)) = text.split_once('#') {
        (a, None, Some(k.to_owned()))
    } else {
        (text, None, None)
    };
    switchboard_core::Executor {
        agent: agent.to_owned(),
        account_id,
        key,
    }
}
#[derive(Subcommand)]
enum AgentsCommand {
    /// Every agent in the catalog with what it supports: mcp, proxy or launch.
    List,
    /// How to connect one agent: its MCP registration and, when it can, the proxy settings.
    Connect {
        /// The agent's catalog id (`switchboard agents list`), for example `hermes`.
        agent: String,
        /// The pool whose API-key account its proxy requests use.
        #[arg(long, default_value = "default")]
        pool: String,
    },
    /// Print the agents' proxy key, for an agent's key command. Not a provider credential.
    Key,
    /// Start an agent configured by environment alone in a folder, on the pool's API-key account.
    Launch {
        /// The agent's catalog id (`switchboard agents list`).
        agent: String,
        /// The pool whose API-key account it uses.
        #[arg(long, default_value = "default")]
        pool: String,
        /// The absolute folder it starts in; default: the current folder.
        #[arg(long)]
        dir: Option<PathBuf>,
    },
}
#[derive(Subcommand)]
enum Backup {
    /// The backup folder and the backups in it, newest first.
    List,
    /// Write a backup now.
    Now,
    /// Add the accounts of one backup that this store does not hold; never replaces newer ones.
    Restore {
        /// A backup's file name, as `switchboard backup list` shows it.
        file: String,
    },
}
#[derive(Subcommand)]
enum Login {
    /// Open the provider's official sign-in in Terminal, in a private home; prints a login id.
    Begin {
        /// The provider to sign in to.
        #[arg(long, value_enum)]
        provider: ProviderArg,
        /// Defaults to the email of the account that signs in.
        #[arg(long, default_value = "")]
        label: String,
        /// The pool the account joins.
        #[arg(long, default_value = "default")]
        pool: String,
    },
    /// pending, complete (ready to finish) or ended (Terminal exited without signing in).
    Status {
        /// The id `login begin` printed.
        login_id: String,
    },
    /// Save the account a completed sign-in produced and clean its staging home.
    Finish {
        /// The id `login begin` printed.
        login_id: String,
    },
    /// Drop a sign-in. Close its Terminal session first; nothing else is stopped.
    Cancel {
        /// The id `login begin` printed.
        login_id: String,
    },
}
#[derive(Clone, Copy, ValueEnum)]
enum ProviderArg {
    Claude,
    Codex,
}
impl From<ProviderArg> for Provider {
    fn from(p: ProviderArg) -> Self {
        match p {
            ProviderArg::Claude => Self::Claude,
            ProviderArg::Codex => Self::Codex,
        }
    }
}
#[derive(Clone, Copy, ValueEnum)]
enum Kind {
    ApiKey,
    SetupToken,
    Oauth,
}
impl From<Kind> for AuthKind {
    fn from(k: Kind) -> Self {
        match k {
            Kind::ApiKey => Self::ApiKey,
            Kind::SetupToken => Self::SetupToken,
            Kind::Oauth => Self::OAuth,
        }
    }
}
#[derive(Clone, Copy, ValueEnum)]
enum Mode {
    Isolated,
    Managed,
}
fn read_secret() -> Result<String, String> {
    if io::stdin().is_terminal() {
        return Err("Pipe credential input to --secret-stdin. Interactive input is refused to prevent terminal echo.".into());
    }
    let mut input = Vec::new();
    io::stdin()
        .take(65537)
        .read_to_end(&mut input)
        .map_err(|_| "Credential stdin could not be read.")?;
    if input.is_empty() || input.len() > 65536 {
        return Err("Credential stdin must contain 1 to 65536 bytes.".into());
    }
    let input = String::from_utf8(input).map_err(|_| "Credential stdin must be UTF-8.")?;
    Ok(input.trim().to_owned())
}
async fn run(cli: &Cli) -> Result<Value, String> {
    let root = cli.data_dir.clone().map(Ok).unwrap_or_else(default_root)?;
    if !root.is_absolute() {
        return Err("--data-dir must be an absolute private directory.".into());
    }
    if let Command::Mcp { read_only } = cli.command {
        mcp::Server::new(root, read_only).serve().await?;
        return Ok(Value::Null);
    }
    if let Command::Uninstall {
        keep_data,
        purge,
        yes,
    } = cli.command
    {
        return switchboard_runtime::uninstall::uninstall_with(
            &root,
            std::sync::Arc::new(switchboard_core::NativeVault::new()),
            keep_data,
            purge,
            yes,
            &switchboard_runtime::uninstall::places(),
        );
    }
    if matches!(cli.command, Command::Serve) {
        if let Some(log) = oplog::Log::default_location() {
            oplog::install(log);
        }
        // SIGTERM (launchd, `kill`, logout) and SIGINT (Ctrl-C) run the same drain with a
        // deadline; the backstop ends the process if it overruns (lifecycle LC-01). Caught
        // before the owner publishes `control.json`, so a stop during start-up drains too.
        let signals = StopSignals::listen()?;
        let owner = Owner::native(root).await?;
        let status = owner.runtime.execute(Operation::Status).await?;
        print_value(&json!({"state":"serving", "runtime":status}), cli.json);
        let signal = signals.requested().await?;
        arm_hard_exit(HARD_EXIT_AFTER);
        oplog::event("stop_requested", &[("signal", oplog::Field::Code(signal))]);
        let stopped = owner.shutdown(DRAIN_DEADLINE).await;
        return Ok(json!({"state":"stopped", "drained": stopped.drained}));
    }
    let operation = match &cli.command {
        Command::Accounts { command } => match command {
            Accounts::List => Operation::Snapshot,
            Accounts::Capture {
                provider,
                label,
                pool,
            } => Operation::CaptureCurrent {
                provider: (*provider).into(),
                label: label.clone(),
                pool: pool.clone(),
            },
            Accounts::ImportClaudeSwap { pool } => {
                Operation::ImportClaudeSwap { pool: pool.clone() }
            }
            Accounts::Activate { id } => Operation::ActivateNative { id: id.clone() },
            Accounts::Add {
                provider,
                kind,
                label,
                pool,
                ..
            } => Operation::Add {
                label: label.clone(),
                provider: (*provider).into(),
                kind: (*kind).into(),
                pool: pool.clone(),
                secret: read_secret()?,
            },
            Accounts::Update { id, label, enabled } => Operation::Update {
                id: id.clone(),
                label: label.clone(),
                enabled: *enabled,
            },
            Accounts::Remove { id } => Operation::Remove { id: id.clone() },
            Accounts::Select { id, provider, pool } => Operation::Select {
                id: id.clone(),
                provider: (*provider).into(),
                pool: pool.clone(),
            },
        },
        Command::Usage { id: Some(id) } => Operation::Usage { id: id.clone() },
        Command::Usage { id: None } | Command::Events => Operation::Snapshot,
        Command::Status => Operation::Status,
        Command::Current => Operation::CurrentAccounts,
        Command::Backup { command } => match command {
            Backup::List => Operation::Backups,
            Backup::Now => Operation::BackupNow,
            Backup::Restore { file } => Operation::RestoreBackup { file: file.clone() },
        },
        Command::Rotation {
            command: Rotation::Status,
        } => Operation::Snapshot,
        Command::Rotation {
            command:
                Rotation::Set {
                    provider,
                    pool,
                    target,
                    enabled,
                    threshold,
                    hysteresis,
                    cooldown,
                    max_age,
                },
        } => {
            let provider: Provider = (*provider).into();
            let target = match target {
                RotationTarget::Managed => "managed",
                RotationTarget::ClaudeCli => "claude_cli",
            };
            // Merge with the saved policy so one flag never resets the others.
            let saved = call(&root, Operation::Snapshot).await?["policies"]
                .as_array()
                .and_then(|policies| {
                    policies.iter().find(|p| {
                        p["provider"] == provider.as_str()
                            && p["pool"] == pool.as_str()
                            && p["target"] == target
                    })
                })
                .map(|p| serde_json::from_value::<RotationPolicy>(p.clone()))
                .transpose()
                .map_err(|_| "Saved rotation policy is unreadable.")?;
            let mut policy = saved.unwrap_or(RotationPolicy {
                provider,
                pool: pool.clone(),
                target: target.into(),
                enabled: false,
                threshold_percent: 90.0,
                hysteresis_percent: 10.0,
                cooldown_seconds: 1800,
                max_age_seconds: 300,
                last_switched_at: None,
            });
            policy.enabled = enabled.unwrap_or(policy.enabled);
            policy.threshold_percent = threshold.unwrap_or(policy.threshold_percent);
            policy.hysteresis_percent = hysteresis.unwrap_or(policy.hysteresis_percent);
            policy.cooldown_seconds = cooldown.unwrap_or(policy.cooldown_seconds);
            policy.max_age_seconds = max_age.unwrap_or(policy.max_age_seconds);
            Operation::SetPolicy { policy }
        }
        Command::Login { command } => match command {
            Login::Begin {
                provider,
                label,
                pool,
            } => Operation::BeginLogin {
                provider: (*provider).into(),
                label: label.clone(),
                pool: pool.clone(),
            },
            Login::Status { login_id } => Operation::LoginStatus {
                login_id: login_id.clone(),
            },
            Login::Finish { login_id } => Operation::FinishLogin {
                login_id: login_id.clone(),
            },
            Login::Cancel { login_id } => Operation::CancelLogin {
                login_id: login_id.clone(),
            },
        },
        Command::Launch {
            id,
            provider,
            mode,
            working_directory,
            in_place,
            args,
        } => {
            return launch(
                &root,
                id,
                *provider,
                *mode,
                working_directory,
                *in_place,
                args,
            )
            .await
        }
        Command::Continue {
            workflow_id,
            account,
            mode,
            dir,
        } => Operation::Continue {
            workflow_id: workflow_id.clone(),
            id: account.clone(),
            mode: match mode {
                Mode::Isolated => "isolated",
                Mode::Managed => "managed",
            }
            .into(),
            working_directory: dir.clone(),
        },
        Command::Chain { command } => match command {
            ChainCommand::List { workflow, pool } => Operation::Chains {
                workflow: workflow.clone(),
                pool: pool.clone(),
            },
            ChainCommand::Set {
                scope,
                preset,
                agents,
            } => {
                if preset.is_none() && agents.is_empty() {
                    return Err("Name the agents in order, or a --preset.".into());
                }
                Operation::SetChain {
                    scope: chain_scope(scope)?,
                    executors: agents.iter().map(|a| chain_executor(a)).collect(),
                    preset: preset.clone(),
                }
            }
            ChainCommand::Clear { scope } => Operation::SetChain {
                scope: chain_scope(scope)?,
                executors: vec![],
                preset: None,
            },
        },
        Command::Agents { command } => match command {
            AgentsCommand::List => Operation::AgentCatalog,
            AgentsCommand::Connect { agent, pool } => Operation::AgentConnect {
                agent: agent.clone(),
                pool: pool.clone(),
            },
            AgentsCommand::Key => Operation::AgentKey,
            AgentsCommand::Launch { agent, pool, dir } => Operation::LaunchAgent {
                agent: agent.clone(),
                pool: pool.clone(),
                working_directory: folder(dir)?,
            },
        },
        Command::Project { command } => match command {
            Project::List => Operation::Snapshot,
            Project::Show { path } => Operation::ResolveProject {
                path: folder(path)?,
            },
            Project::Set {
                path,
                account,
                target,
                paused,
                expires_in_hours,
            } => Operation::SetProjectRule {
                path: folder(path)?,
                account_id: account.clone(),
                target: match target {
                    RotationTarget::Managed => "managed",
                    RotationTarget::ClaudeCli => "claude_cli",
                }
                .into(),
                enabled: !paused,
                expires_at: expires_in_hours.map(|h| mcp::now() + i64::from(h) * 3600),
            },
            Project::Save {
                pool,
                name,
                folders,
                accounts,
            } => Operation::SaveProject {
                pool: pool.clone(),
                name: name.clone(),
                folders: folders
                    .iter()
                    .map(|f| folder(&Some(f.clone())))
                    .collect::<Result<Vec<_>, _>>()?,
                account_ids: accounts.clone(),
            },
            Project::Delete { pool } => Operation::RemoveProject { pool: pool.clone() },
            Project::Remove { path, provider } => Operation::RemoveProjectRule {
                path: folder(path)?,
                provider: (*provider).into(),
            },
            Project::Apply { path, global } => {
                let proxy = control::request(&root, &Operation::Status)
                    .await?
                    .and_then(|s| s["proxy_address"].as_str().map(str::to_owned));
                let session: Session =
                    detect_session(|k| std::env::var(k).ok(), proxy.as_deref(), &root);
                Operation::ApplyProject {
                    path: folder(path)?,
                    session,
                    global: *global,
                }
            }
        },
        Command::Serve | Command::Mcp { .. } | Command::Uninstall { .. } => unreachable!(),
    };
    let value = call(&root, operation).await?;
    match &cli.command {
        Command::Accounts { command: Accounts::List } => Ok(json!({"accounts": value["accounts"], "routes": value["routes"]})),
        Command::Events => Ok(value["events"].clone()),
        Command::Project { command: Project::List } => Ok(json!({"projects": mcp::project_views(&value), "rules": mcp::rule_views(&value, mcp::now())})),
        Command::Usage { id: None } => Ok(Value::Array(value["accounts"].as_array().ok_or("Invalid account response.")?.iter().map(|account| json!({"id":account["id"], "label":account["label"], "provider":account["provider"], "usage":account["usage"], "usage_health":account["usage_health"]})).collect())),
        Command::Rotation { command: Rotation::Status } => {
            let monitor = match control::request(&root, &Operation::MonitorStatus).await? {
                Some(status) => status,
                None => json!({"running":false,"interval_seconds":180,"decisions":[]}),
            };
            Ok(json!({"policies":value["policies"],"monitor":monitor}))
        },
        _ => Ok(value),
    }
}
/// `switchboard launch` (SB-75 adds the account for a folder and the in-place run): resolves the
/// account when only a provider is given, asks the owner to prepare the session, and either
/// leaves it to the Terminal the owner opened or runs the prepared script in this terminal.
async fn launch(
    root: &std::path::Path,
    id: &Option<String>,
    provider: Option<ProviderArg>,
    mode: Mode,
    working_directory: &std::path::Path,
    in_place: bool,
    args: &[String],
) -> Result<Value, String> {
    use std::io::IsTerminal;
    if !in_place && !args.is_empty() {
        return Err("Agent arguments after -- go with --in-place.".into());
    }
    if in_place && !(std::io::stdin().is_terminal() && std::io::stdout().is_terminal()) {
        return Err("Launching in place needs a terminal on stdin and stdout.".into());
    }
    let id = match (id, provider) {
        (Some(id), None) => id.clone(),
        (None, Some(provider)) => {
            let found = call(
                root,
                Operation::LaunchAccount {
                    provider: provider.into(),
                    working_directory: working_directory.to_owned(),
                },
            )
            .await?;
            found["id"].as_str().ok_or("Account not found.")?.to_owned()
        }
        _ => return Err("Give an account id or --provider.".into()),
    };
    let value = call(
        root,
        Operation::Launch {
            id,
            mode: match mode {
                Mode::Isolated => "isolated",
                Mode::Managed => "managed",
            }
            .into(),
            working_directory: working_directory.to_owned(),
            in_place,
            args: args.to_vec(),
        },
    )
    .await?;
    if !in_place {
        return Ok(value);
    }
    let script = value["script"]
        .as_str()
        .ok_or("The session script is missing.")?;
    run_in_place(std::path::Path::new(script))
}
/// Replaces this process with the prepared session script: the agent runs in this terminal and
/// its exit status is the command's.
#[cfg(unix)]
fn run_in_place(script: &std::path::Path) -> Result<Value, String> {
    use std::os::unix::process::CommandExt;
    let error = std::process::Command::new(script).exec();
    let _ = error;
    release_reservation(script);
    Err("The session could not start in this terminal.".into())
}
/// The script never ran, so its home's reservation would block the account for ten minutes:
/// release it now. The script sits in the home it reserved; only a regular file is removed.
#[cfg(unix)]
fn release_reservation(script: &std::path::Path) {
    if let Some(marker) = script.parent().map(|home| home.join(".launch-pending")) {
        if std::fs::symlink_metadata(&marker).is_ok_and(|m| m.file_type().is_file()) {
            let _ = std::fs::remove_file(marker);
        }
    }
}
#[cfg(not(unix))]
fn run_in_place(_: &std::path::Path) -> Result<Value, String> {
    Err("Launching in place runs on macOS and Linux for now.".into())
}
async fn call(root: &std::path::Path, operation: Operation) -> Result<Value, String> {
    match control::request(root, &operation).await? {
        Some(value) => Ok(value),
        None => execute_offline(root.to_owned(), operation).await,
    }
}
fn utc(value: &Value) -> String {
    value
        .as_i64()
        .and_then(rfc3339)
        .unwrap_or_else(|| "—".into())
}
fn print_result(value: &Value, cli: &Cli) {
    if cli.json {
        print_value(value, true);
        return;
    }
    let text = |v: &Value| v.as_str().unwrap_or("—").to_string();
    match &cli.command {
        Command::Backup {
            command: Backup::List,
        } => {
            println!("Folder: {}", text(&value["directory"]));
            if let Some(error) = value["last_error"].as_str() {
                println!("Last automatic backup failed: {error}");
            }
            let backups = value["backups"].as_array().cloned().unwrap_or_default();
            if backups.is_empty() {
                println!("No backups yet. Run 'switchboard backup now'.");
            }
            for b in backups {
                println!(
                    "{}  {}  {} accounts",
                    text(&b["file"]),
                    utc(&b["created_at"]),
                    b["accounts"]
                );
            }
        }
        Command::Accounts {
            command: Accounts::List,
        } => {
            let Some(accounts) = value["accounts"].as_array() else {
                print_value(value, false);
                return;
            };
            if accounts.is_empty() {
                println!(
                    "No accounts yet. Use 'switchboard accounts capture --provider claude' or 'switchboard login begin'."
                );
                return;
            }
            println!(
                "ID                                    PROVIDER  POOL          STATE      LABEL"
            );
            for a in accounts {
                let selected = value["routes"]
                    .as_object()
                    .is_some_and(|routes| routes.values().any(|id| id == &a["id"]));
                let state = if a["enabled"] == false {
                    "disabled"
                } else if selected {
                    "selected"
                } else {
                    "ready"
                };
                println!(
                    "{}  {:8}  {:12}  {:9}  {}",
                    text(&a["id"]),
                    text(&a["provider"]),
                    text(&a["pool"]),
                    state,
                    text(&a["label"])
                );
            }
        }
        Command::Events => {
            let Some(events) = value.as_array() else {
                print_value(value, false);
                return;
            };
            if events.is_empty() {
                println!("No activity yet.");
            }
            for event in events {
                println!(
                    "{}  {}  {}  {}",
                    utc(&event["at"]),
                    text(&event["action"]),
                    text(&event["account_id"]),
                    text(&event["detail"])
                );
            }
        }
        Command::Usage { id: None } => {
            let Some(accounts) = value.as_array() else {
                print_value(value, false);
                return;
            };
            if accounts.is_empty() {
                println!("No accounts yet.");
            }
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |t| t.as_secs() as i64);
            for a in accounts {
                // The account's own capacity: a metered feature's limit is not it (SB-40).
                let usage =
                    serde_json::from_value::<switchboard_core::Usage>(a["usage"].clone()).ok();
                let quota = usage
                    .as_ref()
                    .and_then(switchboard_core::Usage::account_used_percent)
                    .map(|p| {
                        let observed = &a["usage"]["observed_at"];
                        // The age past which an account outside rotation is overdue (SB-48);
                        // rotation itself judges by its policy's `max_age_seconds`.
                        let stale = observed
                            .as_i64()
                            .is_none_or(|t| now - t > switchboard_core::UNPOLICED_MAX_AGE_SECONDS);
                        format!(
                            "{p:.1}% used; observed {}{}",
                            utc(observed),
                            if stale { " (stale)" } else { "" }
                        )
                    })
                    .unwrap_or_else(|| {
                        if usage.is_some() {
                            "account usage unknown (only feature limits reported)".into()
                        } else {
                            "usage unknown".into()
                        }
                    });
                let health = match a["usage_health"]["status"].as_str() {
                    Some(status) => format!(
                        "health {status}; next check {}",
                        utc(&a["usage_health"]["next_check_at"])
                    ),
                    None => "not checked".into(),
                };
                println!(
                    "{}  {}  {}; {}",
                    text(&a["id"]),
                    text(&a["label"]),
                    quota,
                    health
                );
            }
        }
        Command::Project {
            command: Project::List,
        } => {
            let (Some(projects), Some(rules)) =
                (value["projects"].as_array(), value["rules"].as_array())
            else {
                print_value(value, false);
                return;
            };
            if projects.is_empty() {
                println!("No projects. `switchboard project save` reserves accounts for a project's folders.");
            }
            for project in projects {
                let accounts: Vec<String> = project["accounts"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|a| format!("{} ({})", text(&a["label"]), text(&a["provider"])))
                    .collect();
                println!(
                    "{}  pool {}  {}",
                    text(&project["name"]),
                    text(&project["pool"]),
                    if accounts.is_empty() {
                        "no accounts yet".to_string()
                    } else {
                        accounts.join(", ")
                    }
                );
                for folder in project["folders"].as_array().into_iter().flatten() {
                    println!("    {}", text(folder));
                }
            }
            if rules.is_empty() {
                println!(
                    "No project rules. Switchboard follows your selection and rotation everywhere."
                );
            }
            for rule in rules {
                println!(
                    "{:8}  {:7}  {}  {} · {}{}",
                    text(&rule["state"]),
                    text(&rule["provider"]),
                    text(&rule["path"]),
                    text(&rule["account"]["label"]),
                    text(&rule["account"]["pool"]),
                    rule["expires_at"]
                        .as_str()
                        .map(|t| format!(" · until {t}"))
                        .unwrap_or_default()
                );
            }
        }
        Command::Agents {
            command: AgentsCommand::Key,
        } => println!("{}", text(&value["key"])),
        Command::Chain {
            command: ChainCommand::List { .. },
        } => {
            let line = |chain: &Value| {
                let scope = &chain["scope"];
                let name = match scope["kind"].as_str() {
                    Some("project") => format!("project:{}", text(&scope["pool"])),
                    Some("workflow") => format!("workflow:{}", text(&scope["id"])),
                    _ => "machine".to_owned(),
                };
                let agents: Vec<String> = chain["executors"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|e| {
                        let pin = e["account_id"]
                            .as_str()
                            .map(|id| format!("@{id}"))
                            .or_else(|| e["key"].as_str().map(|k| format!("#{k}")))
                            .unwrap_or_default();
                        format!("{}{pin}", text(&e["agent"]))
                    })
                    .collect();
                format!("{name:28} {}", agents.join(" → "))
            };
            let chains = value["chains"].as_array().cloned().unwrap_or_default();
            if chains.is_empty() {
                println!(
                    "No chain is set; none applies until you set one (switchboard chain set …)."
                );
            }
            for chain in &chains {
                println!("{}", line(chain));
            }
            match value["effective"].is_object() {
                true => println!("\napplies: {}", line(&value["effective"])),
                false if !chains.is_empty() => {
                    println!("\napplies: none for this workflow or project")
                }
                false => {}
            }
            for preset in value["presets"].as_array().into_iter().flatten() {
                let agents: Vec<String> = preset["agents"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|a| text(&a["agent"]).to_owned())
                    .collect();
                println!("preset {}: {}", text(&preset["id"]), agents.join(" → "));
            }
        }
        Command::Chain {
            command: ChainCommand::Set { .. },
        } => println!("Chain set."),
        Command::Chain {
            command: ChainCommand::Clear { .. },
        } => println!("Chain cleared."),
        Command::Agents {
            command: AgentsCommand::List,
        } => {
            for agent in value["agents"].as_array().into_iter().flatten() {
                println!(
                    "{:18} {:7} {}",
                    text(&agent["id"]),
                    text(&agent["level"]),
                    text(&agent["name"])
                );
            }
            println!("\nmcp: registers switchboard mcp · proxy: also uses the proxy (its config file) · launch: switchboard agents launch <id>");
        }
        Command::Agents {
            command: AgentsCommand::Connect { .. },
        } => {
            println!("{} ({})", text(&value["name"]), text(&value["level"]));
            let mcp = &value["mcp"];
            if mcp["supported"] == true {
                println!("\nMCP:");
                if let Some(cmd) = mcp["add_command"].as_str() {
                    println!("  {cmd}");
                }
                if let Some(snippet) = mcp["config_snippet"].as_str() {
                    println!("  in {}:\n{}", text(&mcp["config_path"]), snippet);
                }
            } else {
                println!("\nMCP: not supported by this agent.");
            }
            if let Some(a) = value["anthropic"].as_object() {
                println!("\nAnthropic-compatible proxy: {}", text(&a["base_url"]));
                for (k, v) in a["env"].as_object().into_iter().flatten() {
                    println!("  export {k}=\"{}\"", text(v));
                }
                if let Some(snippet) = a["config_snippet"].as_str() {
                    println!("{snippet}");
                }
            }
            if let Some(o) = value["openai"].as_object() {
                println!("\nOpenAI-compatible proxy: {}", text(&o["base_url"]));
            }
            for key in ["requires", "launch", "warning", "notes"] {
                if let Some(line) = value[key].as_str() {
                    println!("\n{line}");
                }
            }
        }
        Command::Project {
            command: Project::Apply { .. },
        } => {
            for result in value["results"].as_array().into_iter().flatten() {
                println!(
                    "{}: {}",
                    text(&result["provider"]),
                    text(&result["message"])
                );
            }
        }
        Command::Status => println!(
            "Mode: {}\nPlatform: {}\nProxy: {}",
            text(&value["live_mode"]),
            text(&value["platform"]),
            text(&value["proxy_address"])
        ),
        _ => print_value(value, false),
    }
}
fn print_value(value: &Value, json_output: bool) {
    if json_output {
        println!("{}", json!({"ok":true,"data":value}));
    } else if value.is_null() {
        println!("Done.");
    } else {
        println!(
            "{}",
            serde_json::to_string_pretty(value).expect("JSON Value serialization")
        );
    }
}
#[tokio::main]
async fn main() -> ExitCode {
    // Parse before resolving app data or creating any files. Even parsing failures
    // are generic: never reflect a user accidentally typing a secret as an argument.
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(error)
            if matches!(
                error.kind(),
                clap::error::ErrorKind::DisplayHelp | clap::error::ErrorKind::DisplayVersion
            ) =>
        {
            let _ = error.print();
            return ExitCode::SUCCESS;
        }
        Err(_) => {
            if std::env::args_os().any(|a| a == "--json") {
                eprintln!(
                    "{}",
                    json!({"ok":false,"error":"Invalid arguments. Run switchboard --help."})
                );
            } else {
                eprintln!("Invalid arguments. Run switchboard --help.");
            }
            return ExitCode::from(2);
        }
    };
    match run(&cli).await {
        Ok(_) if matches!(cli.command, Command::Mcp { .. }) => ExitCode::SUCCESS,
        Ok(value) => {
            print_result(&value, &cli);
            ExitCode::SUCCESS
        }
        Err(error) => {
            if cli.json {
                eprintln!("{}", json!({"ok":false,"error":error}));
            } else {
                eprintln!("{error}");
            }
            ExitCode::from(1)
        }
    }
}

#[cfg(all(test, unix))]
mod in_place_tests {
    #[test]
    fn a_session_that_could_not_start_releases_its_home() {
        let home = tempfile::tempdir().unwrap();
        let marker = home.path().join(".launch-pending");
        std::fs::write(&marker, b"").unwrap();
        super::release_reservation(&home.path().join("launch.command"));
        assert!(!marker.exists());
        // A link in its place is left alone.
        std::os::unix::fs::symlink("/etc/hosts", &marker).unwrap();
        super::release_reservation(&home.path().join("launch.command"));
        assert!(std::fs::symlink_metadata(&marker).is_ok());
    }
}

#[cfg(test)]
mod help_tests {
    use clap::CommandFactory;

    /// SB-68: every command and every argument says what it is in `--help`.
    #[test]
    fn every_command_and_argument_has_help() {
        fn walk(command: &clap::Command, path: &str, missing: &mut Vec<String>) {
            for arg in command.get_arguments() {
                let id = arg.get_id().as_str();
                if matches!(id, "help" | "version") {
                    continue;
                }
                if arg.get_help().is_none() && arg.get_long_help().is_none() {
                    missing.push(format!("{path} {id}"));
                }
            }
            for sub in command.get_subcommands() {
                if sub.get_name() == "help" {
                    continue;
                }
                let here = format!("{path} {}", sub.get_name());
                if sub.get_about().is_none() && sub.get_long_about().is_none() {
                    missing.push(here.clone());
                }
                walk(sub, &here, missing);
            }
        }
        let mut missing = Vec::new();
        walk(&super::Cli::command(), "switchboard", &mut missing);
        assert!(missing.is_empty(), "no help text: {missing:#?}");
    }
}
