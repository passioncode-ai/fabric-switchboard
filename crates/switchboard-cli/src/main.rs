//! Headless account operations; credentials are accepted only from bounded stdin.
use clap::{Parser, Subcommand, ValueEnum};
use serde_json::{json, Value};
use std::{
    io::{self, IsTerminal, Read},
    path::PathBuf,
    process::ExitCode,
};
use switchboard_core::{AuthKind, Provider};
use switchboard_runtime::{control, default_root, execute_offline, Operation, Owner};

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
    let value = match control::request(&root, &operation).await? {
        Some(value) => value,
        None => execute_offline(root, operation).await?,
    };
    match &cli.command {
        Command::Accounts { command: Accounts::List } => Ok(json!({"accounts": value["accounts"], "routes": value["routes"]})),
        Command::Events => Ok(value["events"].clone()),
        Command::Usage { id: None } => Ok(Value::Array(value["accounts"].as_array().ok_or("Invalid account response.")?.iter().map(|account| json!({"id":account["id"], "label":account["label"], "provider":account["provider"], "usage":account["usage"]})).collect())),
        _ => Ok(value),
    }
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
                    "No accounts yet. Use 'switchboard accounts add' or 'switchboard login begin'."
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
                    event["at"],
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
            for a in accounts {
                let quota = a["usage"]["used_percent"]
                    .as_f64()
                    .map(|p| {
                        format!(
                            "{p:.1}% used; observed {} UTC Unix seconds",
                            a["usage"]["observed_at"]
                        )
                    })
                    .unwrap_or_else(|| "usage unknown".into());
                println!("{}  {}  {}", text(&a["id"]), text(&a["label"]), quota);
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
