//! Headless account operations; credentials are accepted only from bounded stdin.
mod mcp;
mod shell;
use clap::{Parser, Subcommand, ValueEnum};
use serde_json::{json, Value};
use std::{
    io::{self, IsTerminal, Read},
    path::PathBuf,
    process::ExitCode,
};
use switchboard_core::{AuthKind, Provider, RotationPolicy};
use switchboard_runtime::{
    arm_hard_exit, control, default_root, onboarding, oplog,
    projects::{detect_session, Session},
    rfc3339, AgentVia, Operation, Owner, StopSignals, DRAIN_DEADLINE, HARD_EXIT_AFTER,
};

#[derive(Parser)]
#[command(
    name = "switchboard",
    version,
    about = "Manage Claude and Codex accounts with Fabric Switchboard",
    after_help = "Start with: switchboard accounts list. A command group run without its command prints its commands (switchboard accounts); recovery, handovers and agents hermes list instead.\nGroups: `project` commands manage groups of repositories with accounts of their own; a `pool` argument takes the group id `project list` prints (`default` for the default group).\nSecrets: pipe a token or auth JSON to accounts add --secret-stdin. Never put secrets in arguments.\nLogin and launch: keep the desktop app or switchboard serve running in another terminal.\nManaged sessions switch identity at the next request; existing streams are not replayed.\nData folder: ~/Library/Application Support/ai.passioncode.fabric-switchboard (macOS), %LOCALAPPDATA%\\ai.passioncode.fabric-switchboard (Windows), $XDG_DATA_HOME/ai.passioncode.fabric-switchboard, by default ~/.local/share/ai.passioncode.fabric-switchboard (Linux)."
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
    /// List and manage accounts (`accounts list` shows them). No command prints credentials.
    Accounts {
        #[command(subcommand)]
        command: Accounts,
    },
    /// Organizations accounts belong to: by your word, by the provider organization (a Claude
    /// Team, a ChatGPT workspace) or by the address's domain. Nothing changes until one exists.
    #[command(alias = "organizations")]
    Org {
        #[command(subcommand)]
        command: Org,
    },
    /// Settings that move to another machine: groups by their repositories, organizations, the
    /// agents' settings and automatic switching. No account or credential.
    Settings {
        #[command(subcommand)]
        command: SettingsCmd,
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
    /// What the proxy relayed: requests, outcomes and the provider's token counts by group,
    /// folder (repository), account, coding agent and day. Reads only.
    #[command(alias = "statistics")]
    Stats {
        /// Days back, 1 to 90.
        #[arg(long, default_value_t = 30, value_parser = clap::value_parser!(i64).range(1..=90))]
        days: i64,
    },
    /// Report online/offline mode and the proxy address, without its capability.
    Status,
    /// Sessions Switchboard launched that are starting or running: Claude Code, Codex, Kimi Code,
    /// Hermes, OpenCode and catalog agents. Account, agent, provider, group, mode, context, folder
    /// and times. Reads only.
    Sessions {
        /// Also list sessions that ended (their homes keep the last launch's record).
        #[arg(long)]
        all: bool,
    },
    /// Inspect the actual signed-in accounts in the ordinary provider CLIs.
    Current,
    /// Automatic switching: move to the group's next account as one nears its quota. Works while
    /// the desktop app or serve is running.
    Rotation {
        #[command(subcommand)]
        command: Rotation,
    },
    /// Official isolated CLI sign-in; requires a running desktop app or serve.
    Login {
        #[command(subcommand)]
        command: Login,
    },
    /// Launch an isolated or managed provider CLI in a folder: in a new Terminal window, or with
    /// --in-place in this terminal (for an embedded console). Requires a running desktop app or
    /// serve.
    Launch {
        /// The account id (`switchboard accounts list`); or give --provider instead.
        id: Option<String>,
        /// Use the account Switchboard would use for this folder: its group's selected account,
        /// else the folder's rule, else the default group's selected account.
        #[arg(long, value_enum, conflicts_with = "id")]
        provider: Option<ProviderArg>,
        /// `isolated`: the account's private home, straight to the provider. `managed`: through
        /// the local proxy, which follows the group's selected account.
        #[arg(long, value_enum, default_value = "isolated")]
        mode: Mode,
        /// The folder the session starts in (absolute).
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
    /// Continue an Observatory workflow whose executor ran out of its limit on another account:
    /// of the same provider, or of another agent (Claude Code, Codex, a saved Kimi Code account,
    /// or Hermes or OpenCode on the saved OpenRouter key) with the same context. Reads the
    /// workflow, offers it (reason `limit`) and launches the account's session, which accepts the
    /// handoff itself. Requires a running desktop app or serve.
    Continue {
        /// The workflow id, `wf_` and 16 hex digits (`project-observatory full workflow list`).
        workflow_id: String,
        /// The account that continues it: a saved account's or Kimi Code account's id, `hermes`
        /// for Hermes Agent or `opencode` for OpenCode on the saved OpenRouter key and its model.
        /// Without it, the default agent (`switchboard agents default`): on the OpenRouter key, or on
        /// whichever of its accounts has room now.
        #[arg(long, default_value = "")]
        account: String,
        /// `isolated` or `managed`, as for `launch`.
        #[arg(long, value_enum, default_value = "isolated")]
        mode: Mode,
        /// The workflow's checkout to run in.
        #[arg(long)]
        dir: PathBuf,
        /// Run the continuing session in this terminal instead of opening Terminal (needs a
        /// terminal on stdin and stdout), for an embedded console such as Fabric's.
        #[arg(long, conflicts_with = "to_console")]
        in_place: bool,
        /// Leave the prepared session for Fabric's console instead of opening Terminal
        /// (`switchboard handovers`; needs "Open hand-overs in Fabric" on). Nobody taking it
        /// within two minutes opens it in Terminal.
        #[arg(long, conflicts_with = "mode")]
        to_console: bool,
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
    /// Kimi Code subscription accounts: sign in with the official `kimi login`, see each plan's
    /// usage, and launch `kimi` on an account. Each login stays in its own folder.
    Kimi {
        #[command(subcommand)]
        command: KimiCommand,
    },
    /// Stopped sessions and accounts ready to continue their work.
    Recovery {
        #[command(subcommand)]
        command: Option<RecoveryCommand>,
    },
    /// Hand-overs waiting for a console: with "Open hand-overs in Fabric" on, the automatic
    /// fallback prepares each continuing session and leaves it here for Fabric's console; one no
    /// console takes within two minutes opens in Terminal.
    Handovers {
        #[command(subcommand)]
        command: Option<HandoversCommand>,
    },
    /// Fallback chains: which agents continue a workflow, in what order, when its accounts run
    /// out (per machine, per group or per task). None applies until you set one.
    Chain {
        #[command(subcommand)]
        command: ChainCommand,
    },
    /// Groups (repositories with accounts of their own) and optional folder rules. The command
    /// is named `project`; a group's id is its `pool`.
    Project {
        #[command(subcommand)]
        command: Project,
    },
    /// Encrypted backups (accounts, groups, rules, settings) in the "Fabric Switchboard Backups"
    /// folder: in ~/Library/Application Support on macOS, %APPDATA% on Windows and
    /// $XDG_DATA_HOME (by default ~/.local/share) on Linux (this machine's key).
    Backup {
        #[command(subcommand)]
        command: Backup,
    },
    /// The guide a coding agent follows to set Switchboard up; with --status, where the setup
    /// stands: each step done, the agent's to do (with its command), the person's, or optional.
    /// Reads only; safe to repeat.
    Onboard {
        /// Report the steps instead of printing the guide.
        #[arg(long)]
        status: bool,
        /// The folder the agent works in (absolute); default: the current folder.
        #[arg(long)]
        folder: Option<PathBuf>,
        /// The coding agent being set up; default: the one this command runs inside, when known.
        #[arg(long, value_enum)]
        host: Option<HostArg>,
    },
    /// The terminal hook: in a group bound to an organization, plain `claude` and `codex`
    /// started from a terminal run in that group's homes and context. `shell status` says what
    /// applies; `shell env` is what the hook asks on every change of folder.
    Shell {
        #[command(subcommand)]
        command: shell::ShellCmd,
    },
    /// Serve the Switchboard tools to a coding agent over MCP on stdin/stdout.
    Mcp {
        /// List only the tools that read: status, accounts, usage, sessions, setup steps, groups
        /// and folder context, automatic switching, fallback chains, the OpenRouter key's status,
        /// Kimi accounts, Hermes's model and the rest that change nothing.
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
#[derive(Clone, Copy, ValueEnum)]
enum HostArg {
    Claude,
    Codex,
}
impl From<HostArg> for onboarding::Host {
    fn from(host: HostArg) -> Self {
        match host {
            HostArg::Claude => onboarding::Host::Claude,
            HostArg::Codex => onboarding::Host::Codex,
        }
    }
}
#[derive(Subcommand)]
enum Project {
    /// Groups (their id, folders and reserved accounts), then every saved folder rule with its
    /// state: active, paused or expired.
    List,
    /// The rule that applies to a folder (default: the current folder).
    Show {
        /// An absolute folder; default: the current folder.
        #[arg(long)]
        path: Option<PathBuf>,
    },
    /// Save a rule for a folder and its subfolders. Rules never stop automatic switching.
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
    /// Codex sessions in a group's folders use the group's Codex accounts: `on` writes one
    /// provider into Codex's own config.toml that points at the local proxy, `off` takes it out,
    /// nothing shows the state. Another model_provider of yours is never replaced.
    Codex {
        /// `on` or `off`; without it, the state only.
        #[arg(value_enum)]
        state: Option<OnOff>,
    },
    /// Replace the capability a group's folders carry (their `.claude/settings.local.json`):
    /// the old one is refused at once and the new one is written into every folder of the group.
    /// For an exposure. Other groups and managed sessions keep theirs.
    RotateCapability {
        /// The group id (`switchboard project list`).
        #[arg(long)]
        pool: String,
    },
    /// Change a group by delta: add or take out accounts and folders, rename it, or bind it to
    /// an organization. Everything not named stays as it is; naming something the group does not
    /// hold is refused.
    Edit {
        /// The group id (`switchboard project list`).
        #[arg(long)]
        pool: String,
        /// A new name for the group.
        #[arg(long)]
        name: Option<String>,
        /// An account to reserve for the group; repeat for each.
        #[arg(long = "add-account")]
        add_accounts: Vec<String>,
        /// An account to give back to the default group; repeat for each.
        #[arg(long = "remove-account")]
        remove_accounts: Vec<String>,
        /// An absolute folder to add; repeat for each.
        #[arg(long = "add-folder")]
        add_folders: Vec<PathBuf>,
        /// A folder of the group to take out; repeat for each. It is not added back by itself.
        #[arg(long = "remove-folder")]
        remove_folders: Vec<PathBuf>,
        /// The organization whose accounts the group runs on (its id or name from `switchboard
        /// org list`), or `personal`. With an organization defined, a group takes only accounts
        /// of its own organization, or personal ones when it has none.
        #[arg(long)]
        organization: Option<String>,
    },
    /// Move a group's repository (its root folder), with the worktrees and clones under it,
    /// into another group in one step; a repository in no group joins the named one. A group
    /// keeps at least one folder.
    MoveRoot {
        /// The repository's folder; default: the current folder.
        #[arg(long)]
        folder: Option<PathBuf>,
        /// The group id to move it to (`switchboard project list`).
        #[arg(long)]
        pool: String,
        /// Add it to this group and keep it in its other groups.
        #[arg(long)]
        also: bool,
        /// The group id it leaves, when it is in several.
        #[arg(long, conflicts_with = "also")]
        from: Option<String>,
    },
    /// The Git repositories on disk, each with the group it belongs to or none. Looks up to
    /// three folders deep and not into a repository, a hidden folder or dependencies.
    Map {
        /// A folder to look under; repeat for each. Default: `~/DATA` when it exists, else the
        /// home folder.
        #[arg(long)]
        under: Vec<PathBuf>,
    },
    /// Create or update a group: its folders (repositories) and the accounts reserved for them.
    /// The listed accounts are the group's whole set; one left out goes back to the default
    /// group. To change a few, use `project edit`.
    Save {
        /// Update the group with this id; without it, a new group is created.
        #[arg(long)]
        pool: Option<String>,
        /// The group's name, as the app shows it.
        #[arg(long)]
        name: String,
        /// An absolute folder of the group (a repository); repeat for each.
        #[arg(long = "folder", required = true)]
        folders: Vec<PathBuf>,
        /// An account reserved for the group; repeat for each.
        #[arg(long = "account")]
        accounts: Vec<String>,
    },
    /// Delete a group. Its accounts keep its group id, no longer reserved.
    Delete {
        /// The group id (`switchboard project list`).
        #[arg(long)]
        pool: String,
    },
    /// Add a folder to the group it belongs to now: a worktree or clone of a group's repository,
    /// or a repository of the group's sources. Nothing is asked; the background pass does the
    /// same within 30 s, and also adds repositories a group's agents work in. With --from-hook
    /// it reads Claude Code's hook input on stdin and always exits 0.
    Suggest {
        /// The folder; default: the current folder.
        #[arg(long, conflicts_with = "from_hook")]
        folder: Option<PathBuf>,
        /// Read the folder (`cwd`) from a Claude Code hook's JSON on stdin.
        #[arg(long)]
        from_hook: bool,
    },
    /// Folders added to groups or taken out as deleted in the last day, and folders still being
    /// added (retried every ten minutes, never asked).
    Suggestions,
    /// The group folders whose ordinary Claude Code sessions use the group's accounts:
    /// Switchboard keeps the route in each folder's `.claude/settings.local.json`.
    Bindings,
    /// Add a folder that is still being added now.
    Accept {
        /// The folder, as `project suggestions` lists it.
        folder: String,
    },
    /// Never add this folder to its group.
    Dismiss {
        /// The folder, as `project suggestions` lists it.
        folder: String,
    },
    /// The repository owners whose new repositories join a group, as host/owner
    /// (`github.com/example`). Without --add or --remove, prints them.
    Source {
        /// The group id (`switchboard project list`).
        #[arg(long)]
        pool: String,
        /// An owner to watch; repeat for each.
        #[arg(long = "add")]
        add: Vec<String>,
        /// An owner to stop watching; repeat for each.
        #[arg(long = "remove")]
        remove: Vec<String>,
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
enum SettingsCmd {
    /// Print this machine's settings as a `switchboard-settings/1` file
    /// (`switchboard settings export > settings.json`).
    Export,
    /// Show what importing a settings file would do here, then do it with `--apply`.
    Import {
        /// The file `settings export` wrote.
        file: PathBuf,
        /// Where to look for this machine's checkouts of the groups' repositories (repeatable;
        /// default `~/DATA`, else the home folder).
        #[arg(long)]
        under: Vec<PathBuf>,
        /// Do it: add the organizations, the groups (with no accounts: give them this
        /// machine's own), the agents' settings and the automatic switching settings.
        #[arg(long)]
        apply: bool,
    },
}
#[derive(Subcommand)]
enum Org {
    /// The organizations, then every account with the organization it belongs to and why.
    List,
    /// Connect PassionCode.ai: sign in in the browser, then your organizations there arrive
    /// here with their names and domains and are read again every 6 hours.
    Connect,
    /// Read your PassionCode.ai organizations again now.
    Read,
    /// Where the PassionCode.ai connection stands (no token is shown).
    Passioncode,
    /// Delete the PassionCode.ai token; the organizations stay as local ones.
    Disconnect,
    /// Add an organization with the e-mail domains it owns.
    Add {
        /// Its name, unique on this machine.
        name: String,
        /// A domain it owns (`example.com`); repeat for several.
        #[arg(long = "domain")]
        domains: Vec<String>,
    },
    /// Rename it, replace its domains, or both.
    Edit {
        /// Its id (`switchboard org list`).
        id: String,
        /// Its new name.
        #[arg(long)]
        name: Option<String>,
        /// Replaces its domains; repeat for several. `--domain ''` alone clears them.
        #[arg(long = "domain")]
        domains: Option<Vec<String>>,
    },
    /// Remove it; its accounts are attributed again by the other rules.
    Remove {
        /// Its id (`switchboard org list`).
        id: String,
    },
    /// Say where one account belongs: an organization's id, `personal`, or `auto` to let the
    /// rules decide again.
    Assign {
        /// The account id (`switchboard accounts list`).
        account: String,
        /// An organization's id, `personal`, or `auto`.
        organization: String,
    },
}

#[derive(Subcommand)]
enum Accounts {
    /// Saved accounts with their provider, kind, group, state and usage. No credentials.
    List,
    /// Save the current CLI authorization; no new sign-in is started.
    Capture {
        /// The CLI whose signed-in account is saved.
        #[arg(long, value_enum)]
        provider: ProviderArg,
        /// A name for it; default: its e-mail.
        #[arg(long)]
        label: Option<String>,
        /// The group id it joins (`default`: the default group).
        #[arg(long, default_value = "default")]
        pool: String,
    },
    /// Import existing Claude Swap backups into the native credential vault.
    ImportClaudeSwap {
        /// The group id the imported accounts join (`default`: the default group).
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
        /// The group id it joins (`default`: the default group).
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
        /// `true`: the account is used on another machine, so this one never renews it or
        /// switches to it, automatically or not (disabled here); `false`: it is used here again.
        #[arg(long, action = clap::ArgAction::Set)]
        elsewhere: Option<bool>,
    },
    /// Move an account into a group, or back to the default group. Joining or leaving a group
    /// is that group's change (`project edit`); the account the ordinary Claude Code or Codex is
    /// signed in to stays in the default group.
    Move {
        /// The account id (`switchboard accounts list`).
        id: String,
        /// A group id (`switchboard project list`), or `default`.
        #[arg(long)]
        pool: String,
    },
    /// Remove an account and its stored credential. Select another one first if it is selected.
    Remove {
        /// The account id (`switchboard accounts list`).
        id: String,
    },
    /// Make an account the one the group's managed sessions use, from their next request.
    Select {
        /// The account id (`switchboard accounts list`).
        id: String,
        /// The account's provider.
        #[arg(long, value_enum)]
        provider: ProviderArg,
        /// The group id whose managed sessions follow it (`default`: the default group).
        #[arg(long, default_value = "default")]
        pool: String,
    },
}
#[derive(Subcommand)]
enum Rotation {
    /// Show the saved automatic switching settings and the latest decisions.
    Status,
    /// Save automatic switching for one provider and group; omitted flags keep the saved values
    /// (new: off, 90, 10, 1800, 300). Existing responses are never replayed or interrupted.
    Set {
        /// The provider it is for.
        #[arg(long, value_enum)]
        provider: ProviderArg,
        /// The group id it switches within (`default`: the default group).
        #[arg(long, default_value = "default")]
        pool: String,
        /// `managed`: the group's managed sessions. `claude-cli`: the ordinary Claude Code login.
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
enum RecoveryCommand {
    /// Register a host-observed stop; repeated identical events are safe.
    Register {
        /// Stable id of this limit event; reuse it on a registration retry.
        event: String,
        /// Exact stopped session id supplied by its host.
        #[arg(long)]
        session: String,
        /// Saved subscription account used by the stopped session.
        #[arg(long)]
        account: String,
        /// Existing checkout folder for the stopped work.
        #[arg(long)]
        directory: PathBuf,
    },
    /// Claim a ready stop once, before the host resumes its session.
    Claim {
        /// Stop event id returned by recovery status.
        event: String,
        /// Compatible candidate returned by recovery status.
        #[arg(long)]
        account: String,
    },
    /// Claude Code's `StopFailure` hook: reads the hook's JSON on stdin and registers the
    /// session's own stop. Always exits 0, so the hook never fails the session.
    Hook,
    /// Record the host's outcome; this command does not run a session.
    Finish {
        /// Stop event whose result the session host observed.
        event: String,
        /// Resumed after a claim, or cancelled by the operator.
        #[arg(value_enum)]
        outcome: RecoveryOutcome,
    },
}
#[derive(Clone, Copy, ValueEnum)]
enum RecoveryOutcome {
    Resumed,
    Cancelled,
}
#[derive(Subcommand)]
enum HandoversCommand {
    /// The hand-overs waiting for a console, oldest first, and whether Fabric gets them.
    List,
    /// Take a waiting hand-over and run its session in this terminal (needs a terminal on stdin
    /// and stdout). It runs once: a second taker is refused.
    Run {
        /// The hand-over's id, `ho-` and 12 hex digits (`switchboard handovers`).
        id: String,
    },
    /// Turn "Open hand-overs in Fabric" on or off.
    Set {
        /// `on` or `off`.
        #[arg(value_enum)]
        state: OnOff,
    },
}
#[derive(Clone, Copy, ValueEnum)]
enum OnOff {
    On,
    Off,
}
#[derive(Subcommand)]
enum ChainCommand {
    /// Every chain set, the presets, and the chain that applies to a workflow or a group.
    List {
        /// A workflow id (`wf_` and 16 hex digits): show the chain that applies to it.
        #[arg(long)]
        workflow: Option<String>,
        /// A group id: show the chain that applies to it. With --dir, the executor account's
        /// group.
        #[arg(long)]
        pool: Option<String>,
        /// A workflow's checkout folder (absolute): its group is found as the automatic fallback
        /// finds it, before --pool.
        #[arg(long)]
        dir: Option<PathBuf>,
    },
    /// Set a chain in order. Each agent is a catalog id (`switchboard agents list`), optionally
    /// pinned: `codex@ACCOUNT_ID` runs on that account, `kimi-code#project/env/NAME` on a paid
    /// key named in the Observatory vault. Without a pin Switchboard picks an account at the switch.
    Set {
        /// `machine`, `project:<group id>` or `workflow:<wf_id>`.
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
        /// `machine`, `project:<group id>` or `workflow:<wf_id>`.
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
        _ => Err("Scope is machine, project:<group id> or workflow:<wf_id>.".into()),
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
    /// Which agents are installed here, which take over work, the default agent and the
    /// command Switchboard runs for each.
    Status,
    /// Let Switchboard hand work to this agent (fallback chains, `continue`).
    Enable {
        /// The agent's catalog id, for example `opencode`.
        agent: String,
    },
    /// Keep work away from this agent: chains skip it and `continue` refuses it.
    Disable {
        /// The agent's catalog id.
        agent: String,
    },
    /// The machine's default agent (`auto` for none). Fabric reads it too.
    Default {
        /// An installed, enabled agent that can continue, or `auto`.
        agent: String,
    },
    /// The OpenRouter models agents can launch on: the recommended ones first, then every
    /// OpenRouter model that accepts tools.
    Models,
    /// How to connect one agent: its MCP registration and, when it can, the proxy settings.
    Connect {
        /// The agent's catalog id (`switchboard agents list`), for example `hermes`.
        agent: String,
        /// The group id whose API-key account its proxy requests use.
        #[arg(long, default_value = "default")]
        pool: String,
    },
    /// Print the agents' proxy key, for an agent's key command. Not a provider credential.
    /// With `--service openrouter`, the saved OpenRouter key instead: for a session script's
    /// `export`, never to paste anywhere.
    Key {
        /// `openrouter`: the key agents run on (`switchboard agents openrouter set`).
        #[arg(long, value_enum)]
        service: Option<KeyService>,
        /// With --service: the folder the agent starts in (default: the current folder); the
        /// key of its group wins over the machine's.
        #[arg(long)]
        dir: Option<PathBuf>,
    },
    /// The OpenRouter key agents run on: save it (piped on stdin), see what it may still spend,
    /// change the default model, or remove it.
    Openrouter {
        #[command(subcommand)]
        command: OpenrouterCommand,
    },
    /// Hermes's model and provider (its own config); `set` changes them with `hermes config set`.
    Hermes {
        #[command(subcommand)]
        command: Option<HermesCommand>,
    },
    /// Start an agent in a folder in Terminal: on the group's API-key account through the
    /// proxy, or with `--openrouter` on the saved OpenRouter key.
    Launch {
        /// The agent's catalog id (`switchboard agents list`).
        agent: String,
        /// The group id whose API-key account it uses (not with `--openrouter`).
        #[arg(long, default_value = "default", conflicts_with = "openrouter")]
        pool: String,
        /// The absolute folder it starts in; default: the current folder.
        #[arg(long)]
        dir: Option<PathBuf>,
        /// Run on the OpenRouter key saved with `switchboard agents openrouter set`.
        #[arg(long)]
        openrouter: bool,
        /// The OpenRouter model for this launch, such as `moonshotai/kimi-k2`; default: the key's
        /// model.
        #[arg(long, requires = "openrouter")]
        model: Option<String>,
    },
}
#[derive(Subcommand)]
enum HermesCommand {
    /// The model and provider the ordinary Hermes runs on (the default).
    Status,
    /// Change the provider and/or the default model, through Hermes's own `hermes config set`.
    Set {
        /// A Hermes provider id, for example `openrouter`, `anthropic`, `nous`, `kimi-coding`.
        #[arg(long)]
        provider: Option<String>,
        /// The default model, for example `moonshotai/kimi-k2`.
        #[arg(long)]
        model: Option<String>,
    },
}
#[derive(Subcommand)]
enum KimiCommand {
    /// Every saved account with its tier and plan usage, and the ordinary `kimi`'s sign-in.
    List,
    /// Start `kimi login` in Terminal for a new account; finish with `switchboard kimi finish`.
    Login {
        /// A name for the account; default: Kimi's nickname for it.
        #[arg(long, default_value = "")]
        label: String,
        /// `mainland-cn` (kimi.com) or `global` (kimi.ai).
        #[arg(long, default_value = "mainland-cn")]
        region: String,
    },
    /// Save a sign-in once Terminal says it is logged in.
    Finish {
        /// The id `switchboard kimi login` printed.
        id: String,
    },
    /// Drop a sign-in that was not finished.
    Cancel {
        /// The id `switchboard kimi login` printed.
        id: String,
    },
    /// Sign in to a saved account again (when Kimi refused its login).
    Relogin {
        /// The account id (`switchboard kimi list`).
        id: String,
    },
    /// Start `kimi` on an account in a folder, in Terminal.
    Launch {
        /// The account id (`switchboard kimi list`).
        id: String,
        /// The absolute folder it starts in; default: the current folder.
        #[arg(long)]
        dir: Option<PathBuf>,
    },
    /// Rename an account.
    Rename {
        /// The account id (`switchboard kimi list`).
        id: String,
        /// The new name.
        label: String,
    },
    /// Forget an account and delete its login folder.
    Remove {
        /// The account id (`switchboard kimi list`).
        id: String,
    },
}
#[derive(Clone, Copy, ValueEnum)]
enum KeyService {
    Openrouter,
}
#[derive(Subcommand)]
enum OpenrouterCommand {
    /// Save the key piped on stdin (`sk-or-…`); it goes to the OS vault, never to a file.
    Set {
        /// The model agents start on, an OpenRouter id such as `moonshotai/kimi-k2`.
        #[arg(long)]
        model: Option<String>,
        /// Read the key from piped stdin. Interactive input is refused (no echo).
        #[arg(long, required = true)]
        key_stdin: bool,
        /// A group id: the key agents in its folders run on. Without it, the key every other
        /// folder uses.
        #[arg(long)]
        pool: Option<String>,
    },
    /// Whether a key is saved, its default model and what it may still spend (asks OpenRouter).
    Status {
        /// A group id; without it, the machine's key.
        #[arg(long)]
        pool: Option<String>,
    },
    /// Change the default model.
    Model {
        /// An OpenRouter model id, for example `anthropic/claude-sonnet-4.5`.
        model: String,
        /// A group id; without it, the machine's key.
        #[arg(long)]
        pool: Option<String>,
    },
    /// Remove the key from Switchboard and the OS vault.
    Remove {
        /// A group id; without it, the machine's key.
        #[arg(long)]
        pool: Option<String>,
    },
    /// Every saved key with the group it is for, and the groups without one.
    List,
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
        /// The group id the account joins (`default`: the default group).
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
    // Before the data folder is resolved: these read files only and never fail (SB-183). Every
    // `shell` command is answered here, so this match names each one: a new one fails to compile
    // until it is answered (the later `Command::Shell` arm is unreachable).
    if let Command::Shell { command } = &cli.command {
        return match command {
            shell::ShellCmd::Status { dir } => {
                Ok(shell::status(cli.data_dir.as_deref(), dir.as_deref()))
            }
            shell::ShellCmd::Env { dir, .. } => {
                shell::print_line(&shell::env_answer(cli.data_dir.as_deref(), dir.as_deref()));
                Ok(Value::Null)
            }
            shell::ShellCmd::On => shell::set(cli.data_dir.as_deref(), true),
            shell::ShellCmd::Off => shell::set(cli.data_dir.as_deref(), false),
            shell::ShellCmd::Init { shell: kind } => {
                let script = shell::init(cli.data_dir.as_deref(), *kind)?;
                if cli.json {
                    Ok(json!({"shell": kind.kind().name(), "script": script}))
                } else {
                    print!("{script}");
                    Ok(Value::Null)
                }
            }
        };
    }
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
    if let Command::Onboard {
        status,
        folder,
        host,
    } = &cli.command
    {
        if !status {
            return Ok(json!({"guide": onboarding::GUIDE}));
        }
        let folder = match folder {
            Some(path) if path.is_absolute() => path.clone(),
            Some(_) => return Err("--folder must be an absolute folder.".into()),
            None => {
                std::env::current_dir().map_err(|_| "Current folder unavailable. Pass --folder.")?
            }
        };
        return onboard_status(&root, folder, host.map(Into::into)).await;
    }
    if matches!(cli.command, Command::Serve) {
        if let Some(log) = oplog::for_root(&root, default_root().ok().as_deref()) {
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
            Accounts::Update {
                id,
                label,
                enabled,
                elsewhere,
            } => Operation::Update {
                id: id.clone(),
                label: label.clone(),
                enabled: *enabled,
                elsewhere: *elsewhere,
            },
            Accounts::Move { id, pool } => Operation::MoveAccount {
                id: id.clone(),
                pool: pool.clone(),
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
        Command::Stats { days } => Operation::Statistics { days: Some(*days) },
        Command::Settings { command } => match command {
            SettingsCmd::Export => Operation::SettingsExport,
            SettingsCmd::Import { file, under, apply } => {
                let meta = std::fs::metadata(file).map_err(|_| "Settings file unavailable.")?;
                if meta.len() > switchboard_runtime::settings_bundle::MAX_BYTES as u64 {
                    return Err(switchboard_runtime::settings_bundle::NOT_A_BUNDLE.into());
                }
                let bytes = std::fs::read(file).map_err(|_| "Settings file unavailable.")?;
                let mut value: Value = serde_json::from_slice(&bytes)
                    .map_err(|_| switchboard_runtime::settings_bundle::NOT_A_BUNDLE)?;
                // `--json` wraps it as {ok, data}.
                if value.get("format").is_none() && value["data"].get("format").is_some() {
                    value = value["data"].take();
                }
                Operation::SettingsImport {
                    bundle: value,
                    under: under.clone(),
                    apply: *apply,
                }
            }
        },
        Command::Org { command } => match command {
            Org::List => Operation::Organizations,
            Org::Connect => Operation::VestaConnect,
            Org::Read => Operation::VestaSync,
            Org::Passioncode => Operation::VestaStatus,
            Org::Disconnect => Operation::VestaDisconnect,
            Org::Add { name, domains } => Operation::OrganizationAdd {
                name: name.clone(),
                domains: domains.clone(),
            },
            Org::Edit { id, name, domains } => Operation::OrganizationEdit {
                id: id.clone(),
                name: name.clone(),
                domains: domains
                    .as_ref()
                    .map(|d| d.iter().filter(|d| !d.is_empty()).cloned().collect()),
            },
            Org::Remove { id } => Operation::OrganizationRemove { id: id.clone() },
            Org::Assign {
                account,
                organization,
            } => Operation::OrganizationAssign {
                account: account.clone(),
                choice: (organization != "auto").then(|| organization.clone()),
            },
        },
        Command::Sessions { all } => Operation::Sessions { all: *all },
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
                .map_err(|_| "Saved automatic switching settings are unreadable.")?;
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
            dir,
            to_console: true,
            ..
        } => Operation::ContinueToConsole {
            workflow_id: workflow_id.clone(),
            id: account.clone(),
            working_directory: dir.clone(),
        },
        Command::Continue {
            workflow_id,
            account,
            mode,
            dir,
            in_place,
            ..
        } => return continue_workflow(&root, workflow_id, account, *mode, dir, *in_place).await,
        Command::Recovery { command } => match command {
            None => Operation::Recovery,
            Some(RecoveryCommand::Register {
                event,
                session,
                account,
                directory,
            }) => Operation::RecoveryRegister {
                event: event.clone(),
                session: session.clone(),
                account: account.clone(),
                directory: directory.clone(),
            },
            Some(RecoveryCommand::Claim { event, account }) => Operation::RecoveryClaim {
                event: event.clone(),
                account: account.clone(),
            },
            Some(RecoveryCommand::Hook) => return Ok(recovery_hook(&root).await),
            Some(RecoveryCommand::Finish { event, outcome }) => Operation::RecoveryFinish {
                event: event.clone(),
                resumed: matches!(outcome, RecoveryOutcome::Resumed),
            },
        },
        Command::Handovers { command } => match command {
            None | Some(HandoversCommand::List) => Operation::Handovers,
            Some(HandoversCommand::Set { state }) => Operation::HandoverSetting {
                on: Some(matches!(state, OnOff::On)),
            },
            Some(HandoversCommand::Run { id }) => return run_handover(&root, id).await,
        },
        Command::Chain { command } => match command {
            ChainCommand::List {
                workflow,
                pool,
                dir,
            } => Operation::Chains {
                workflow: workflow.clone(),
                pool: pool.clone(),
                folder: match dir {
                    Some(dir) if dir.is_absolute() => Some(dir.clone()),
                    Some(_) => return Err("--dir must be an absolute folder.".into()),
                    None => None,
                },
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
            AgentsCommand::Status => Operation::AgentsOverview,
            AgentsCommand::Enable { agent } => Operation::AgentEnable {
                agent: agent.clone(),
                enabled: true,
            },
            AgentsCommand::Disable { agent } => Operation::AgentEnable {
                agent: agent.clone(),
                enabled: false,
            },
            AgentsCommand::Default { agent } => Operation::AgentDefault {
                agent: (agent != "auto").then(|| agent.clone()),
            },
            AgentsCommand::Models => Operation::OpenrouterModels,
            AgentsCommand::Connect { agent, pool } => Operation::AgentConnect {
                agent: agent.clone(),
                pool: pool.clone(),
            },
            AgentsCommand::Key { service: None, .. } => Operation::AgentKey,
            AgentsCommand::Key {
                service: Some(KeyService::Openrouter),
                dir,
            } => Operation::AgentKeyValue {
                service: "openrouter".into(),
                folder: match dir {
                    Some(d) if d.is_absolute() => Some(d.clone()),
                    Some(_) => return Err("--dir must be an absolute folder.".into()),
                    None => std::env::current_dir().ok(),
                },
            },
            AgentsCommand::Openrouter { command } => match command {
                OpenrouterCommand::Set { model, pool, .. } => Operation::AgentKeySet {
                    service: "openrouter".into(),
                    key: read_secret()?,
                    model: model.clone(),
                    pool: pool.clone(),
                },
                OpenrouterCommand::Status { pool } => Operation::AgentKeyStatus {
                    service: "openrouter".into(),
                    credit: true,
                    pool: pool.clone(),
                },
                OpenrouterCommand::Model { model, pool } => Operation::AgentKeyModel {
                    service: "openrouter".into(),
                    model: model.clone(),
                    pool: pool.clone(),
                },
                OpenrouterCommand::Remove { pool } => Operation::AgentKeyRemove {
                    service: "openrouter".into(),
                    pool: pool.clone(),
                },
                OpenrouterCommand::List => Operation::AgentKeys {
                    service: "openrouter".into(),
                },
            },
            AgentsCommand::Hermes {
                command: None | Some(HermesCommand::Status),
            } => Operation::HermesModel,
            AgentsCommand::Hermes {
                command: Some(HermesCommand::Set { provider, model }),
            } => Operation::HermesSetModel {
                provider: provider.clone(),
                model: model.clone(),
            },
            AgentsCommand::Launch {
                agent,
                pool,
                dir,
                openrouter,
                model,
            } => Operation::LaunchAgent {
                agent: agent.clone(),
                pool: pool.clone(),
                working_directory: folder(dir)?,
                via: if *openrouter {
                    AgentVia::Openrouter
                } else {
                    AgentVia::Proxy
                },
                model: model.clone(),
            },
        },
        Command::Kimi { command } => match command {
            KimiCommand::List => Operation::KimiAccounts,
            KimiCommand::Login { label, region } => Operation::KimiLoginBegin {
                label: label.clone(),
                region: region.clone(),
            },
            KimiCommand::Finish { id } => Operation::KimiLoginFinish { id: id.clone() },
            KimiCommand::Cancel { id } => Operation::KimiLoginCancel { id: id.clone() },
            KimiCommand::Relogin { id } => Operation::KimiSignInAgain { id: id.clone() },
            KimiCommand::Launch { id, dir } => Operation::KimiLaunch {
                id: id.clone(),
                working_directory: folder(dir)?,
            },
            KimiCommand::Rename { id, label } => Operation::KimiRename {
                id: id.clone(),
                label: label.clone(),
            },
            KimiCommand::Remove { id } => Operation::KimiRemove { id: id.clone() },
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
                base_folders: None,
                organization: None,
            },
            Project::Codex { state } => Operation::CodexRoute {
                enabled: state.map(|s| matches!(s, OnOff::On)),
            },
            Project::RotateCapability { pool } => {
                Operation::RotatePoolCapability { pool: pool.clone() }
            }
            Project::Edit {
                pool,
                name,
                add_accounts,
                remove_accounts,
                add_folders,
                remove_folders,
                organization,
            } => Operation::ProjectEdit {
                pool: pool.clone(),
                name: name.clone(),
                add_accounts: add_accounts.clone(),
                remove_accounts: remove_accounts.clone(),
                add_folders: add_folders
                    .iter()
                    .map(|f| folder(&Some(f.clone())))
                    .collect::<Result<Vec<_>, _>>()?,
                // A folder to take out may be gone from disk: taken as given.
                remove_folders: remove_folders.clone(),
                organization: organization.clone(),
            },
            Project::MoveRoot {
                folder: path,
                pool,
                also,
                from,
            } => Operation::MoveRoot {
                folder: folder(path)?,
                pool: pool.clone(),
                from: from.clone(),
                also: *also,
            },
            Project::Map { under } => Operation::RepositoryMap {
                under: under
                    .iter()
                    .map(|u| folder(&Some(u.clone())))
                    .collect::<Result<Vec<_>, _>>()?,
            },
            Project::Delete { pool } => Operation::RemoveProject {
                pool: pool.clone(),
                by: switchboard_runtime::spend::Surface::Cli,
            },
            Project::Suggest {
                from_hook: true, ..
            } => project_suggest_hook_and_exit(&root).await,
            Project::Suggest { folder: path, .. } => Operation::ProjectSuggest {
                folder: match path {
                    Some(p) if !p.is_absolute() => {
                        return Err("--folder must be an absolute folder.".into())
                    }
                    _ => folder(path)?,
                },
            },
            Project::Suggestions => Operation::ProjectSuggestions,
            Project::Bindings => Operation::ProjectBindings,
            Project::Accept { folder } => Operation::ProjectSuggestionAccept {
                folder: folder.clone(),
            },
            Project::Dismiss { folder } => Operation::ProjectSuggestionDismiss {
                folder: folder.clone(),
            },
            Project::Source { pool, add, remove } => {
                return project_sources(&root, pool, add, remove).await
            }
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
        Command::Serve
        | Command::Mcp { .. }
        | Command::Shell { .. }
        | Command::Uninstall { .. }
        | Command::Onboard { .. } => unreachable!(),
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
/// `switchboard handovers run <id>`: takes the hand-over (once) and runs its session here.
async fn run_handover(root: &std::path::Path, id: &str) -> Result<Value, String> {
    use std::io::IsTerminal;
    // Checked before it is taken: a hand-over taken by a caller with no terminal would be lost.
    if !(std::io::stdin().is_terminal() && std::io::stdout().is_terminal()) {
        return Err("Running a hand-over needs a terminal on stdin and stdout.".into());
    }
    let value = call(root, Operation::HandoverTake { id: id.to_owned() }).await?;
    let script = value["script"]
        .as_str()
        .ok_or("The session script is missing.")?;
    run_in_place(std::path::Path::new(script))
}
/// `switchboard continue`: the hand-over, and with `--in-place` its session in this terminal.
async fn continue_workflow(
    root: &std::path::Path,
    workflow_id: &str,
    account: &str,
    mode: Mode,
    dir: &std::path::Path,
    in_place: bool,
) -> Result<Value, String> {
    use std::io::IsTerminal;
    // Checked before anything is offered: a hand-over without a terminal to run in would leave
    // an offer nobody accepts.
    if in_place && !(std::io::stdin().is_terminal() && std::io::stdout().is_terminal()) {
        return Err("Continuing in place needs a terminal on stdin and stdout.".into());
    }
    let value = call(
        root,
        Operation::Continue {
            workflow_id: workflow_id.to_owned(),
            id: account.to_owned(),
            mode: match mode {
                Mode::Isolated => "isolated",
                Mode::Managed => "managed",
            }
            .into(),
            working_directory: dir.to_owned(),
            in_place,
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
/// `switchboard onboard --status` (SB-93): reads the owner, the saved accounts, the folder's
/// project, the agent's MCP registration and the proving call. Writes nothing: without a data
/// folder there are no accounts and no project to read, and none is created.
pub(crate) async fn onboard_status(
    root: &std::path::Path,
    folder: PathBuf,
    host: Option<onboarding::Host>,
) -> Result<Value, String> {
    let owner_online = matches!(
        control::request(root, &Operation::Status).await,
        Ok(Some(_))
    );
    let mut probe = onboarding::Probe {
        owner_online,
        host: host.or_else(|| onboarding::Host::detect(|k| std::env::var(k).ok())),
        cli: switchboard_runtime::launch::agent_cli(),
        proof: onboarding::proof(root),
        ..Default::default()
    };
    if root.is_dir() {
        let snapshot = call(root, Operation::Snapshot).await?;
        for provider in ["claude", "codex"] {
            let count = snapshot["accounts"].as_array().map_or(0, |accounts| {
                accounts
                    .iter()
                    .filter(|a| a["provider"] == provider && a["enabled"] == true)
                    .count()
            });
            probe.accounts.push((provider.into(), count));
        }
        let kimi = snapshot["kimi_accounts"].as_array().map_or(0, Vec::len);
        probe.accounts.push(("kimi".into(), kimi));
        let resolved = call(
            root,
            Operation::ResolveProject {
                path: folder.clone(),
            },
        )
        .await?;
        probe.project = Some(resolved["project"].clone()).filter(|p| !p.is_null());
    }
    // SB-103: Windows names the home USERPROFILE (as `agents::setup` reads it).
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .map(PathBuf::from)
        .unwrap_or_default();
    let codex_home = switchboard_runtime::person::codex_home();
    let hosts = match probe.host {
        Some(host) => vec![host],
        None => vec![onboarding::Host::Claude, onboarding::Host::Codex],
    };
    probe.registered = hosts
        .into_iter()
        .map(|host| {
            (
                host,
                onboarding::registered(host, &home, Some(&folder), codex_home.as_deref()),
            )
        })
        .collect();
    probe.folder = Some(folder);
    Ok(onboarding::steps(&probe))
}
/// How long the `SessionStart` hook may hold a session's start (SB-149), reading its input
/// included. The owner runs `ProjectSuggest` under its transaction, which a background pass on a
/// loaded machine held for up to 57 s; behind it the hook used to run into Claude Code's own
/// 10 s timeout at almost every start. The plugin's hook timeout (3 s) stays above this.
const SESSION_HOOK_DEADLINE: std::time::Duration = std::time::Duration::from_millis(1500);
/// `switchboard project suggest --from-hook` (SB-117, SB-146): Claude Code's `SessionStart` hook
/// names the session's folder as `cwd`. Its output joins the session's context, so it prints one
/// line only when a folder was added to its group, or when an add waits; it exits 0. Past
/// `SESSION_HOOK_DEADLINE` it prints nothing: the owner may still finish the add, and its
/// background pass makes it within 30 s otherwise. Every private write is a rename, so leaving
/// an offline add unfinished leaves no torn file.
async fn project_suggest_hook_and_exit(root: &std::path::Path) -> ! {
    let value = tokio::time::timeout(SESSION_HOOK_DEADLINE, project_suggest_hook(root))
        .await
        .unwrap_or_else(|_| json!({"suggested": false, "reason": "timed_out"}));
    let project = value["suggestion"]["project"]
        .as_str()
        .or_else(|| value["project"]["name"].as_str())
        .unwrap_or("?");
    // SB-146: a repository in no group stays in the default group; nothing to say.
    if value["added"] == true {
        println!(
            "Switchboard: this folder was added to the group \"{project}\"; its sessions use the group's accounts from their next start."
        );
    } else if value["suggested"] == true {
        println!(
            "Switchboard: this folder looks like part of the group \"{project}\" but could not be added yet; Switchboard tries again by itself."
        );
    }
    std::process::exit(0)
}
async fn project_suggest_hook(root: &std::path::Path) -> Value {
    // Read on a blocking thread, so a host that never closes stdin meets the deadline too.
    let input = tokio::task::spawn_blocking(|| {
        let mut input = Vec::new();
        std::io::stdin()
            .take(HOOK_INPUT_LIMIT)
            .read_to_end(&mut input)
            .map(|_| input)
    })
    .await;
    let parsed: Option<Value> = input
        .ok()
        .and_then(Result::ok)
        .and_then(|input| serde_json::from_slice(&input).ok());
    let Some(cwd) = parsed
        .as_ref()
        .and_then(|h| h["cwd"].as_str())
        .map(std::path::PathBuf::from)
    else {
        return json!({"suggested": false, "reason": "malformed_input"});
    };
    match call(root, Operation::ProjectSuggest { folder: cwd }).await {
        Ok(value) => value,
        Err(error) => json!({"suggested": false, "reason": "refused", "message": error}),
    }
}
/// `switchboard project source`: the project's current sources, changed by --add/--remove.
async fn project_sources(
    root: &std::path::Path,
    pool: &str,
    add: &[String],
    remove: &[String],
) -> Result<Value, String> {
    let snapshot = call(root, Operation::Snapshot).await?;
    let project = snapshot["projects"]
        .as_array()
        .and_then(|ps| ps.iter().find(|p| p["pool"] == pool))
        .ok_or("Group not found.")?;
    let mut sources: Vec<String> = project["sources"]
        .as_array()
        .map(|s| {
            s.iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    if add.is_empty() && remove.is_empty() {
        return Ok(json!({"pool": pool, "sources": sources}));
    }
    let key = |s: &str| s.trim().trim_end_matches('/').to_ascii_lowercase();
    sources.retain(|s| !remove.iter().any(|r| key(r) == *s));
    sources.extend(add.iter().map(|a| key(a)));
    call(
        root,
        Operation::ProjectSources {
            pool: pool.to_owned(),
            sources,
        },
    )
    .await
}
/// The largest hook input read; Claude Code's `StopFailure` payload is a few hundred bytes.
/// A `Stop` input carries the turn's whole last message (Claude Code 2.1.299), so the bound fits
/// a long one: before 0.6.83 round 2 it was 64 KiB and a longer message settled nothing.
const HOOK_INPUT_LIMIT: u64 = 8 * 1024 * 1024;
/// `switchboard recovery hook` (SB-115). Only `rate_limit` and `billing_error` register; every
/// refusal (no owner, malformed input, unknown account, a folder the account may not use)
/// comes back as `registered: false` with a reason code, never as a failed exit.
async fn recovery_hook(root: &std::path::Path) -> Value {
    use std::io::Read;
    let mut input = Vec::new();
    let read = std::io::stdin()
        .take(HOOK_INPUT_LIMIT)
        .read_to_end(&mut input);
    let parsed: Option<Value> = read.ok().and_then(|_| serde_json::from_slice(&input).ok());
    let Some(hook) = parsed else {
        return json!({"registered": false, "reason": "malformed_input"});
    };
    // SB-204: the same hook command runs on `Stop`, after every finished turn. It settles the
    // session's stop, asks the owner only when the session has one, and prints `{}`: a Stop
    // hook's stdout is read by Claude Code as its decision.
    if hook["hook_event_name"].as_str() == Some("Stop") {
        if let Some(session) = hook["session_id"].as_str() {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs() as i64);
            // Bounded like the start hook (SB-149): a busy or absent owner never holds a turn.
            if switchboard_runtime::recovery::session_has_live_stop(root, session, now) {
                let _ = tokio::time::timeout(
                    SESSION_HOOK_DEADLINE,
                    call(
                        root,
                        Operation::RecoveryContinued {
                            session: session.to_owned(),
                        },
                    ),
                )
                .await;
            }
        }
        return json!({});
    }
    let billing = match hook["error"].as_str() {
        Some("rate_limit") => false,
        Some("billing_error") => true,
        _ => return json!({"registered": false, "reason": "not_a_limit"}),
    };
    let (Some(session), Some(cwd)) = (hook["session_id"].as_str(), hook["cwd"].as_str()) else {
        return json!({"registered": false, "reason": "malformed_input"});
    };
    let operation = Operation::RecoveryHook {
        session: session.to_owned(),
        directory: std::path::PathBuf::from(cwd),
        billing,
    };
    match call(root, operation).await {
        Ok(value) => value,
        Err(error) => json!({"registered": false, "reason": "refused", "message": error}),
    }
}
async fn call(root: &std::path::Path, operation: Operation) -> Result<Value, String> {
    switchboard_runtime::call(root, operation).await
}
/// What an empty cell of the CLI's text output shows: a plain hyphen, never a dash.
const PLACEHOLDER: &str = "-";
fn utc(value: &Value) -> String {
    value
        .as_i64()
        .and_then(rfc3339)
        .unwrap_or_else(|| PLACEHOLDER.into())
}
fn print_result(value: &Value, cli: &Cli) {
    if cli.json {
        print_value(value, true);
        return;
    }
    let text = |v: &Value| v.as_str().unwrap_or(PLACEHOLDER).to_string();
    match &cli.command {
        Command::Org { command: Org::List } => {
            let organizations = value["organizations"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            if organizations.is_empty() {
                println!("No organization. Add one: switchboard org add NAME --domain example.com");
                return;
            }
            for o in &organizations {
                let domains: Vec<String> = o["domains"]
                    .as_array()
                    .map(|d| d.iter().map(text).collect())
                    .unwrap_or_default();
                println!(
                    "{}  {}  {} account(s)  {}",
                    text(&o["id"]),
                    text(&o["name"]),
                    o["accounts"].as_u64().unwrap_or(0),
                    domains.join(", ")
                );
            }
            println!();
            for a in value["accounts"].as_array().cloned().unwrap_or_default() {
                let by = match a["by"].as_str() {
                    Some("person") => "set by you",
                    Some("provider") => "its team",
                    Some("domain") => "its domain",
                    _ => "",
                };
                println!(
                    "{}  {:7}  {:24}  {}",
                    text(&a["id"]),
                    text(&a["provider"]),
                    a["organization_name"].as_str().unwrap_or("personal"),
                    if by.is_empty() {
                        text(&a["label"])
                    } else {
                        format!("{} ({by})", text(&a["label"]))
                    }
                );
            }
        }
        Command::Sessions { .. } => {
            let sessions = value["sessions"].as_array().cloned().unwrap_or_default();
            if sessions.is_empty() {
                println!("No session Switchboard launched is running.");
            }
            for s in sessions {
                let label = s["label"]
                    .as_str()
                    .or(s["account_id"].as_str())
                    .unwrap_or(PLACEHOLDER);
                // SB-110: a session in an organization's context names it after the account.
                let label = match s["context_name"].as_str() {
                    Some(name) => format!("{label} ({name})"),
                    None => label.to_owned(),
                };
                // SB-201: the agent the session runs (an agent through the proxy records the
                // route's provider, `claude`); the provider when an older record names none. 16
                // fits the longest catalog id.
                let agent = if s["agent"].is_string() {
                    &s["agent"]
                } else {
                    &s["provider"]
                };
                println!(
                    "{:<9} {:<16} {:<9} {} · {} · {}",
                    text(&s["state"]),
                    text(agent),
                    text(&s["mode"]),
                    label,
                    text(&s["working_directory"]),
                    s["started_at"]
                        .as_i64()
                        .and_then(rfc3339)
                        .unwrap_or_else(|| "not started".into()),
                );
            }
        }
        Command::Shell { .. } => print!("{}", shell::status_text(value)),
        Command::Onboard { status: false, .. } => print!("{}", text(&value["guide"])),
        Command::Onboard { status: true, .. } => {
            for step in value["steps"].as_array().into_iter().flatten() {
                let mark = match step["state"].as_str() {
                    Some("done") => "done",
                    Some("todo") => "to do",
                    Some("needs_person") => "the person's",
                    _ => "optional",
                };
                println!(
                    "{:<13} {:<13} {}",
                    text(&step["id"]),
                    mark,
                    text(&step["summary"])
                );
                if let Some(command) = step["next"]["command"].as_str() {
                    println!("{:27}run: {command}", "");
                }
                if let Some(tool) = step["next"]["tool"].as_str() {
                    println!("{:27}call the MCP tool: {tool}", "");
                }
                if let Some(person) = step["next"]["person"].as_str() {
                    println!("{:27}{person}", "");
                }
            }
            println!(
                "{}",
                if value["complete"] == true {
                    "Set up."
                } else {
                    "Not set up yet: run this again after each step."
                }
            );
        }
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
                "ID                                    PROVIDER  GROUP         STATE      LABEL"
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
                println!("No groups. `switchboard project save` creates one.");
            }
            for project in projects {
                let accounts: Vec<String> = project["accounts"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|a| format!("{} ({})", text(&a["label"]), text(&a["provider"])))
                    .collect();
                println!(
                    "{}  group id {}  {}",
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
                    "No folder rules. Switchboard follows your selection and automatic switching everywhere."
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
            command: AgentsCommand::Hermes { .. },
        } => {
            if value["installed"] != true {
                println!("Hermes is not installed.");
            } else if let Some(error) = value["error"].as_str() {
                println!("{error}");
            } else {
                println!(
                    "Hermes · model {} · provider {}{}",
                    value["model"].as_str().unwrap_or("not set"),
                    value["provider"].as_str().unwrap_or("not set"),
                    value["base_url"]
                        .as_str()
                        .map(|b| format!(" · {b}"))
                        .unwrap_or_default()
                );
            }
        }
        Command::Kimi {
            command: KimiCommand::List,
        } => {
            let windows = |status: &Value| {
                let list: Vec<String> = status["windows"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|w| {
                        format!(
                            "{} {}%",
                            w["name"].as_str().unwrap_or("?"),
                            w["used_percent"].as_f64().unwrap_or(0.0)
                        )
                    })
                    .collect();
                if list.is_empty() {
                    status["error"].as_str().unwrap_or("no usage").to_owned()
                } else {
                    list.join(" · ")
                }
            };
            let accounts = value["accounts"].as_array().cloned().unwrap_or_default();
            if accounts.is_empty() {
                println!("No Kimi Code account is saved. Add one with `switchboard kimi login`.");
            }
            for item in accounts {
                let a = &item["account"];
                println!(
                    "{}  {} · {}{} · {}",
                    text(&a["id"]),
                    text(&a["label"]),
                    text(&a["region"]),
                    a["tier"]
                        .as_str()
                        .map(|t| format!(" · {t}"))
                        .unwrap_or_default(),
                    windows(&item["status"])
                );
            }
            let current = &value["current"];
            if current.is_object() {
                // SB-100: the line says signed out when it is, never "signed in" by default.
                let who = if current["signed_in"] == true {
                    current["nickname"].as_str().unwrap_or("signed in")
                } else {
                    "not signed in (run kimi login)"
                };
                println!("In use now (ordinary kimi): {who} · {}", windows(current));
            }
        }
        Command::Kimi {
            command: KimiCommand::Login { .. },
        } => println!(
            "Sign in in the Terminal window, then run: switchboard kimi finish {}",
            text(&value["login_id"])
        ),
        Command::Agents {
            command: AgentsCommand::Key { .. },
        } => println!("{}", text(&value["key"])),
        Command::Agents {
            command:
                AgentsCommand::Openrouter {
                    command: OpenrouterCommand::Status { .. },
                },
        } => {
            if value["saved"] != true {
                println!("No OpenRouter key is saved.");
            } else {
                println!(
                    "OpenRouter key saved · model {}",
                    value["model"].as_str().unwrap_or("not set")
                );
                let credit = &value["credit"];
                if credit.is_object() {
                    let money = |v: &Value| v.as_f64().map(|n| format!("${n:.2}"));
                    println!(
                        "Remaining {} · spent today {} · limit {}",
                        money(&credit["limit_remaining"]).unwrap_or_else(|| "no limit".into()),
                        money(&credit["usage_daily"]).unwrap_or_else(|| "unknown".into()),
                        match (money(&credit["limit"]), credit["limit_reset"].as_str()) {
                            (Some(limit), Some(reset)) => format!("{limit} {reset}"),
                            (Some(limit), None) => limit,
                            _ => "none".into(),
                        }
                    );
                } else if let Some(error) = value["credit_error"].as_str() {
                    println!("{error}");
                }
            }
        }
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
                    println!("\napplies: none for this workflow or group")
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
/// The stack every thread of the CLI runs on. The owner's `execute` is one large match whose
/// debug-build poll frame outgrew Windows' 1 MiB main-thread stack at 0.6.60 (the MCP server
/// died on its first offline call, nightly `39676565992`); a fixed size makes the limit the same
/// on every platform instead of the platform's default.
const STACK_BYTES: usize = 16 * 1024 * 1024;
fn main() -> ExitCode {
    // `shell env` runs on every change of folder in a hooked terminal: answered here, before the
    // runtime starts, within its 25 ms budget (SB-183).
    let args: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
    if let Some(code) = shell::env_fast_path(&args) {
        return code;
    }
    let runtime = std::thread::Builder::new()
        .name("switchboard-main".into())
        .stack_size(STACK_BYTES)
        .spawn(|| {
            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .thread_stack_size(STACK_BYTES)
                .build()
                .map(|runtime| runtime.block_on(start()))
        });
    match runtime.map(|thread| thread.join()) {
        Ok(Ok(Ok(code))) => code,
        _ => {
            eprintln!("Switchboard could not start its runtime.");
            ExitCode::FAILURE
        }
    }
}
/// The command groups the arguments name, as the tree spells them (an alias resolves to its
/// command's name). Only names found in the tree come back, so a typed value is never echoed.
fn command_path(args: impl IntoIterator<Item = std::ffi::OsString>) -> Vec<String> {
    let root = <Cli as clap::CommandFactory>::command();
    let mut here = &root;
    let mut path = Vec::new();
    let mut skip_value = false;
    for arg in args {
        let Some(arg) = arg.to_str().map(str::to_owned) else {
            continue;
        };
        if std::mem::take(&mut skip_value) {
            continue;
        }
        if arg.starts_with('-') {
            // The only global option that takes a separate value.
            skip_value = arg == "--data-dir";
            continue;
        }
        match here.find_subcommand(&arg) {
            Some(sub) => {
                path.push(sub.get_name().to_owned());
                here = sub;
            }
            None => break,
        }
    }
    path
}
async fn start() -> ExitCode {
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
        // A command group named without its command (`switchboard accounts`): its help, which
        // holds only the tree's own names, never a typed value.
        Err(error)
            if matches!(
                error.kind(),
                clap::error::ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
                    | clap::error::ErrorKind::MissingSubcommand
            ) =>
        {
            let path = command_path(std::env::args_os().skip(1));
            let hint = format!(
                "Name a command. Run switchboard {}--help.",
                path.iter()
                    .map(|name| format!("{name} "))
                    .collect::<String>()
            );
            if std::env::args_os().any(|a| a == "--json") {
                eprintln!("{}", json!({"ok":false,"error":hint}));
            } else {
                let mut command = <Cli as clap::CommandFactory>::command();
                // Built, so each subcommand's usage line names its whole path.
                command.build();
                for name in &path {
                    match command.find_subcommand(name).cloned() {
                        Some(sub) => command = sub,
                        None => break,
                    }
                }
                eprintln!("{}", command.render_help());
            }
            return ExitCode::from(2);
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
        Ok(_)
            if matches!(
                cli.command,
                Command::Mcp { .. }
                    | Command::Shell {
                        command: shell::ShellCmd::Env { .. }
                    }
            ) =>
        {
            ExitCode::SUCCESS
        }
        // `shell init` printed its script itself (in --json it answers the script as data).
        Ok(Value::Null)
            if matches!(
                cli.command,
                Command::Shell {
                    command: shell::ShellCmd::Init { .. }
                }
            ) =>
        {
            ExitCode::SUCCESS
        }
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

    /// What a person must not read in help or a tool's description: a backlog id (CPY-09), an
    /// em dash or a double or spaced hyphen used as one (CPY-08). Text in backticks is a literal
    /// (`-- --resume`) and is skipped.
    pub(crate) fn wording_faults(text: &str) -> Vec<&'static str> {
        let mut plain = String::new();
        for (i, part) in text.split('`').enumerate() {
            if i % 2 == 0 {
                plain.push_str(part);
                plain.push(' ');
            }
        }
        let mut faults = Vec::new();
        if plain.contains('\u{2014}') {
            faults.push("em dash");
        }
        let id = |prefix: &str| {
            plain.match_indices(prefix).any(|(at, _)| {
                plain[at + prefix.len()..]
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_digit())
            })
        };
        if id("SB-") || id("REQ-") {
            faults.push("backlog id");
        }
        for dash in [" -- ", " - ", " --\n", " -\n"] {
            if plain.contains(dash) {
                faults.push("hyphen used as a dash");
            }
        }
        faults
    }

    #[test]
    fn help_carries_no_backlog_id_and_no_dash() {
        fn walk(command: &clap::Command, path: &str, found: &mut Vec<String>) {
            let mut texts: Vec<String> = [
                command.get_about(),
                command.get_long_about(),
                command.get_after_help(),
                command.get_after_long_help(),
            ]
            .into_iter()
            .flatten()
            .map(|t| t.to_string())
            .collect();
            for arg in command.get_arguments() {
                texts.extend(
                    [arg.get_help(), arg.get_long_help()]
                        .into_iter()
                        .flatten()
                        .map(|t| t.to_string()),
                );
            }
            for text in texts {
                for fault in wording_faults(&text) {
                    found.push(format!("{path}: {fault}: {text}"));
                }
            }
            for sub in command.get_subcommands() {
                walk(sub, &format!("{path} {}", sub.get_name()), found);
            }
        }
        let mut found = Vec::new();
        walk(&super::Cli::command(), "switchboard", &mut found);
        assert!(found.is_empty(), "{found:#?}");
    }

    #[test]
    fn the_wording_check_finds_what_it_names() {
        assert_eq!(wording_faults("Ran out \u{2014} of it"), ["em dash"]);
        assert_eq!(wording_faults("Groups (SB-146): add"), ["backlog id"]);
        assert_eq!(wording_faults("one -- two"), ["hyphen used as a dash"]);
        assert!(wording_faults("after `--` (for example `-- --resume <id>`)").is_empty());
        assert!(wording_faults("--in-place, 1-90, wf_ and SB-ish").is_empty());
    }

    /// PRD-07: a group named alone resolves to its path, and a typed value never comes back.
    #[test]
    fn a_command_group_named_alone_resolves_to_its_own_names() {
        let path = |args: &[&str]| super::command_path(args.iter().map(Into::into));
        assert_eq!(path(&["accounts"]), ["accounts"]);
        assert_eq!(path(&["--json", "organizations"]), ["org"]);
        assert_eq!(
            path(&["--data-dir", "/tmp/accounts", "agents", "openrouter"]),
            ["agents", "openrouter"]
        );
        assert!(path(&["fixture-never-echo"]).is_empty());
    }
}

/// SB-134: every CLI command either has an MCP tool or a written reason it has none, so the gap
/// between the CLI and the MCP server cannot reopen without someone deciding it.
#[cfg(test)]
mod mcp_parity {
    use clap::CommandFactory;
    enum Over {
        Tool(&'static str),
        Not(&'static str),
    }
    use Over::{Not, Tool};
    const PERSON: &str = "the person's own act: it takes a secret, deletes what cannot be restored, or changes this machine";
    const PROCESS: &str = "a process mode, not an operation";
    const HOST: &str =
        "plumbing that hooks, consoles or hosts call; the state is readable over MCP";
    const LAUNCH: &str = "launching an agent from an agent is SB-64/SB-74";
    const PARITY: &[(&str, Over)] = &[
        ("accounts list", Tool("switchboard_accounts")),
        ("accounts capture", Not(PERSON)),
        ("accounts import-claude-swap", Not(PERSON)),
        ("accounts activate", Tool("switchboard_switch")),
        ("accounts add", Not(PERSON)),
        ("accounts update", Tool("switchboard_account_update")),
        ("accounts move", Tool("switchboard_account_move")),
        ("org list", Tool("switchboard_organizations")),
        ("org add", Tool("switchboard_organization_set")),
        ("org edit", Tool("switchboard_organization_set")),
        ("org remove", Tool("switchboard_organization_set")),
        ("org assign", Tool("switchboard_organization_set")),
        ("org connect", Not(PERSON)),
        ("org read", Tool("switchboard_organization_set")),
        ("org passioncode", Tool("switchboard_organizations")),
        ("org disconnect", Not(PERSON)),
        ("accounts remove", Not(PERSON)),
        ("accounts select", Tool("switchboard_switch")),
        ("usage", Tool("switchboard_usage")),
        ("events", Tool("switchboard_events")),
        ("stats", Tool("switchboard_statistics")),
        (
            "settings export",
            Not("SB-176: a file the person carries to another machine; an agent has no second machine to carry it to"),
        ),
        (
            "settings import",
            Not("SB-176: reads a file the person brought from another machine and adds groups and policies; the person runs it once per move"),
        ),
        ("status", Tool("switchboard_status")),
        ("sessions", Tool("switchboard_sessions")),
        ("current", Tool("switchboard_status")),
        ("rotation status", Tool("switchboard_rotation")),
        ("rotation set", Tool("switchboard_rotation_set")),
        ("login begin", Tool("switchboard_login")),
        ("login status", Tool("switchboard_login")),
        ("login finish", Tool("switchboard_login")),
        ("login cancel", Tool("switchboard_login")),
        ("launch", Not(LAUNCH)),
        ("continue", Not(LAUNCH)),
        ("serve", Not(PROCESS)),
        ("mcp", Not(PROCESS)),
        ("agents list", Tool("switchboard_agents")),
        ("agents status", Tool("switchboard_agents")),
        ("agents enable", Tool("switchboard_agent_set")),
        ("agents disable", Tool("switchboard_agent_set")),
        ("agents default", Tool("switchboard_agent_set")),
        ("agents models", Tool("switchboard_openrouter_models")),
        ("agents connect", Tool("switchboard_onboarding")),
        ("agents key", Not(PERSON)),
        ("agents openrouter set", Not(PERSON)),
        (
            "agents openrouter status",
            Tool("switchboard_openrouter_status"),
        ),
        (
            "agents openrouter model",
            Tool("switchboard_openrouter_model"),
        ),
        ("agents openrouter remove", Not(PERSON)),
        (
            "agents openrouter list",
            Tool("switchboard_openrouter_status"),
        ),
        ("agents hermes status", Tool("switchboard_hermes_status")),
        ("agents hermes set", Tool("switchboard_hermes_set")),
        ("agents launch", Not(LAUNCH)),
        ("kimi list", Tool("switchboard_kimi_accounts")),
        ("kimi login", Tool("switchboard_kimi_login")),
        ("kimi finish", Tool("switchboard_kimi_login")),
        ("kimi cancel", Tool("switchboard_kimi_login")),
        ("kimi relogin", Tool("switchboard_kimi_login")),
        ("kimi launch", Not(LAUNCH)),
        ("kimi rename", Tool("switchboard_kimi_rename")),
        ("kimi remove", Not(PERSON)),
        ("recovery register", Not(HOST)),
        ("recovery claim", Not(HOST)),
        ("recovery hook", Not(HOST)),
        ("recovery finish", Not(HOST)),
        ("handovers list", Tool("switchboard_handovers")),
        ("handovers run", Not(HOST)),
        ("handovers set", Tool("switchboard_handovers_set")),
        ("chain list", Tool("switchboard_chain_get")),
        ("chain set", Tool("switchboard_chain_set")),
        ("chain clear", Tool("switchboard_chain_set")),
        ("project list", Tool("switchboard_projects")),
        ("project show", Tool("switchboard_project_context")),
        ("project set", Tool("switchboard_project_set")),
        ("project remove", Tool("switchboard_project_remove")),
        ("project edit", Tool("switchboard_project_edit")),
        ("project move-root", Tool("switchboard_project_move_root")),
        ("project map", Tool("switchboard_repository_map")),
        ("project rotate-capability", Not(PERSON)),
        ("project codex", Tool("switchboard_codex_route")),
        ("project save", Tool("switchboard_project_edit")),
        ("project delete", Not(PERSON)),
        ("project suggest", Not(HOST)),
        (
            "project suggestions",
            Tool("switchboard_project_suggestions"),
        ),
        ("project bindings", Tool("switchboard_project_bindings")),
        ("project accept", Tool("switchboard_project_suggestion")),
        ("project dismiss", Tool("switchboard_project_suggestion")),
        ("project source", Tool("switchboard_project_sources")),
        ("project apply", Tool("switchboard_project_apply")),
        ("backup list", Tool("switchboard_backups")),
        ("backup now", Tool("switchboard_backup_now")),
        ("backup restore", Not(PERSON)),
        ("onboard", Tool("switchboard_onboarding")),
        (
            "shell status",
            Not("SB-183: the MCP tool switchboard_shell_hook (read and set) lands with the window's switch, node A-7 of 0.7.0"),
        ),
        (
            "shell env",
            Not("SB-183: the terminal hook's own question on every change of folder, one line for a shell script; not an agent action"),
        ),
        (
            "shell on",
            Not("SB-183: the MCP tool switchboard_shell_hook (read and set) lands with the window's switch, node A-7 of 0.7.0"),
        ),
        (
            "shell off",
            Not("SB-183: the MCP tool switchboard_shell_hook (read and set) lands with the window's switch, node A-7 of 0.7.0"),
        ),
        (
            "shell init",
            Not("SB-183: prints the hook script for a person's own start file; not an agent action"),
        ),
        ("uninstall", Not(PERSON)),
    ];

    fn leaves(command: &clap::Command, path: &str, out: &mut Vec<String>) {
        let subs: Vec<&clap::Command> = command
            .get_subcommands()
            .filter(|c| c.get_name() != "help")
            .collect();
        if subs.is_empty() {
            out.push(path.trim().to_owned());
        }
        for sub in subs {
            leaves(sub, &format!("{path} {}", sub.get_name()), out);
        }
    }

    #[test]
    fn every_cli_command_has_an_mcp_tool_or_a_reason() {
        let mut commands = Vec::new();
        leaves(&super::Cli::command(), "", &mut commands);
        let tools: Vec<String> = crate::mcp::tools()
            .into_iter()
            .map(|(_, t)| t["name"].as_str().unwrap().to_owned())
            .collect();
        let missing: Vec<&String> = commands
            .iter()
            .filter(|c| !PARITY.iter().any(|(p, _)| p == c))
            .collect();
        assert!(
            missing.is_empty(),
            "CLI commands with no MCP decision: {missing:?}"
        );
        for (command, over) in PARITY {
            assert!(
                commands.iter().any(|c| c == command),
                "no such CLI command: {command}"
            );
            match over {
                Tool(name) => assert!(
                    tools.iter().any(|t| t == name),
                    "{command}: no MCP tool {name}"
                ),
                Not(reason) => assert!(reason.len() > 20, "{command}: the reason says why"),
            }
        }
        let covered = PARITY.iter().filter(|(_, o)| matches!(o, Tool(_))).count();
        assert!(covered * 2 > PARITY.len(), "most commands have a tool");
    }

    /// PRD-04: the skill's tool reference names every tool the server lists, with a section of
    /// its own, and no longer says sign-in is unavailable.
    #[test]
    fn every_mcp_tool_is_in_the_skill_reference() {
        let reference = include_str!(
            "../../../plugins/switchboard/skills/switching-accounts/references/tools.md"
        );
        let missing: Vec<String> = crate::mcp::tools()
            .into_iter()
            .map(|(_, t)| t["name"].as_str().unwrap().to_owned())
            .filter(|name| !reference.contains(&format!("### `{name}`")))
            .collect();
        assert!(
            missing.is_empty(),
            "tools.md has no section for {missing:?}"
        );
        assert!(
            !reference.contains("login and launch are not exposed"),
            "switchboard_login exists"
        );
    }

    /// CPY-08, CPY-09: tool titles, descriptions and the server's instructions carry no backlog
    /// id and no dash.
    #[test]
    fn mcp_wording_carries_no_backlog_id_and_no_dash() {
        fn strings(value: &serde_json::Value, out: &mut Vec<String>) {
            match value {
                serde_json::Value::String(s) => out.push(s.clone()),
                serde_json::Value::Array(items) => items.iter().for_each(|v| strings(v, out)),
                serde_json::Value::Object(map) => map.values().for_each(|v| strings(v, out)),
                _ => {}
            }
        }
        let mut texts = vec![crate::mcp::INSTRUCTIONS.to_owned()];
        for (_, tool) in crate::mcp::tools() {
            strings(&tool, &mut texts);
        }
        let found: Vec<String> = texts
            .iter()
            .flat_map(|t| {
                crate::help_tests::wording_faults(t)
                    .into_iter()
                    .map(move |f| format!("{f}: {t}"))
            })
            .collect();
        assert!(found.is_empty(), "{found:#?}");
    }
}
