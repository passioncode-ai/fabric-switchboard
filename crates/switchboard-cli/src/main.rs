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
    Usage { id: Option<String> },
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
    /// Launch an isolated or managed provider CLI in a project directory.
    Launch {
        id: String,
        #[arg(long, value_enum, default_value = "isolated")]
        mode: Mode,
        #[arg(long)]
        working_directory: PathBuf,
    },
    /// Own the vault, inference proxy and private CLI control listener until Ctrl-C or
    /// SIGTERM, then stop within ten seconds.
    Serve,
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
    /// Every saved rule with its state: active, paused or expired.
    List,
    /// The rule that applies to a folder (default: the current folder).
    Show {
        #[arg(long)]
        path: Option<PathBuf>,
    },
    /// Save a rule for a folder and its subfolders. Rules never stop rotation.
    Set {
        #[arg(long)]
        path: Option<PathBuf>,
        #[arg(long)]
        account: String,
        #[arg(long, value_enum, default_value = "managed")]
        target: RotationTarget,
        /// Save the rule switched off.
        #[arg(long)]
        paused: bool,
        #[arg(long, value_parser = clap::value_parser!(u32).range(1..=720))]
        expires_in_hours: Option<u32>,
    },
    Remove {
        #[arg(long)]
        path: Option<PathBuf>,
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
        #[arg(long)]
        name: String,
        #[arg(long = "folder", required = true)]
        folders: Vec<PathBuf>,
        #[arg(long = "account")]
        accounts: Vec<String>,
    },
    /// Delete a project. Its accounts stay in its pool, no longer reserved.
    Delete {
        #[arg(long)]
        pool: String,
    },
    /// Apply the folder's rule to the session this command runs in.
    Apply {
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
    List,
    /// Save the current CLI authorization; no new sign-in is started.
    Capture {
        #[arg(long, value_enum)]
        provider: ProviderArg,
        #[arg(long)]
        label: Option<String>,
        #[arg(long, default_value = "default")]
        pool: String,
    },
    /// Import existing Claude Swap backups into the native credential vault.
    ImportClaudeSwap {
        #[arg(long, default_value = "default")]
        pool: String,
    },
    /// Activate a captured Claude OAuth profile in the ordinary Claude Code CLI.
    Activate {
        id: String,
    },
    Add {
        #[arg(long, value_enum)]
        provider: ProviderArg,
        #[arg(long, value_enum)]
        kind: Kind,
        #[arg(long)]
        label: String,
        #[arg(long, default_value = "default")]
        pool: String,
        /// Read one token or auth JSON from piped stdin, limited to 64 KiB.
        #[arg(long, required = true)]
        secret_stdin: bool,
    },
    Update {
        id: String,
        #[arg(long)]
        label: String,
        #[arg(long, action = clap::ArgAction::Set)]
        enabled: bool,
    },
    Remove {
        id: String,
    },
    Select {
        id: String,
        #[arg(long, value_enum)]
        provider: ProviderArg,
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
        #[arg(long, value_enum)]
        provider: ProviderArg,
        #[arg(long, default_value = "default")]
        pool: String,
        #[arg(long, value_enum, default_value = "managed")]
        target: RotationTarget,
        #[arg(long, action = clap::ArgAction::Set)]
        enabled: Option<bool>,
        #[arg(long)]
        threshold: Option<f64>,
        #[arg(long)]
        hysteresis: Option<f64>,
        #[arg(long)]
        cooldown: Option<i64>,
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
enum Backup {
    /// The backup folder and the backups in it, newest first.
    List,
    /// Write a backup now.
    Now,
    /// Add the accounts of one backup that this store does not hold; never replaces newer ones.
    Restore { file: String },
}
#[derive(Subcommand)]
enum Login {
    Begin {
        #[arg(long, value_enum)]
        provider: ProviderArg,
        /// Defaults to the email of the account that signs in.
        #[arg(long, default_value = "")]
        label: String,
        #[arg(long, default_value = "default")]
        pool: String,
    },
    /// pending, complete (ready to finish) or ended (Terminal exited without signing in).
    Status {
        login_id: String,
    },
    Finish {
        login_id: String,
    },
    Cancel {
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
            mode,
            working_directory,
        } => Operation::Launch {
            id: id.clone(),
            mode: match mode {
                Mode::Isolated => "isolated",
                Mode::Managed => "managed",
            }
            .into(),
            working_directory: working_directory.clone(),
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
