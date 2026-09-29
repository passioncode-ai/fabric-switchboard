//! Headless account operations; credentials are accepted only from bounded stdin.
use clap::{Parser, Subcommand, ValueEnum};
use serde_json::{json, Value};
use std::{
    io::{self, IsTerminal, Read},
    path::PathBuf,
    process::ExitCode,
};
use switchboard_core::{AuthKind, Provider, RotationPolicy};
use switchboard_runtime::{control, default_root, execute_offline, rfc3339, Operation, Owner};

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
    /// Own the vault, inference proxy and private CLI control listener until Ctrl-C.
    Serve,
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
enum Login {
    Begin {
        #[arg(long, value_enum)]
        provider: ProviderArg,
        #[arg(long)]
        label: String,
        #[arg(long, default_value = "default")]
        pool: String,
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
    if matches!(cli.command, Command::Serve) {
        let owner = Owner::native(root).await?;
        let status = owner.runtime.execute(Operation::Status).await?;
        print_value(&json!({"state":"serving", "runtime":status}), cli.json);
        tokio::signal::ctrl_c()
            .await
            .map_err(|_| "Shutdown signal unavailable.")?;
        drop(owner);
        return Ok(json!({"state":"stopped"}));
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
        Command::Serve => unreachable!(),
    };
    let value = call(&root, operation).await?;
    match &cli.command {
        Command::Accounts { command: Accounts::List } => Ok(json!({"accounts": value["accounts"], "routes": value["routes"]})),
        Command::Events => Ok(value["events"].clone()),
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
                let quota = a["usage"]["used_percent"]
                    .as_f64()
                    .map(|p| {
                        let observed = &a["usage"]["observed_at"];
                        // Matches the default rotation limit on observation age.
                        let stale = observed.as_i64().is_none_or(|t| now - t > 300);
                        format!(
                            "{p:.1}% used; observed {}{}",
                            utc(observed),
                            if stale { " (stale)" } else { "" }
                        )
                    })
                    .unwrap_or_else(|| "usage unknown".into());
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
