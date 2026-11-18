//! Headless account operations; credentials are accepted only from bounded stdin.
#[cfg(unix)]
mod hosted;
mod mcp;
mod shell;
mod ssh_serve;
use clap::{Parser, Subcommand, ValueEnum};
use serde_json::{json, Value};
use std::{
    io::{self, IsTerminal, Read},
    path::PathBuf,
    process::ExitCode,
};
use switchboard_core::{AuthKind, Provider, RotationPolicy};
use switchboard_runtime::spend::money;
use switchboard_runtime::{
    arm_hard_exit, control, default_root, launches, onboarding, oplog,
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
    /// The operator's own words, verbatim, that ask for this: they open raising, clearing or
    /// doubling a ceiling, deleting a group with one and replacing or removing a key one counts
    /// (otherwise the window's). Give `--said-at` and `--said-in` with it; it is recorded.
    #[arg(long, global = true)]
    operator_said: Option<String>,
    /// When the operator said it, RFC 3339 (`2026-11-07T19:15:00Z`).
    #[arg(long, global = true)]
    said_at: Option<String>,
    /// Where the operator said it: a session id or a board thread.
    #[arg(long, global = true)]
    said_in: Option<String>,
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
    /// Link this `switchboard` into ~/.local/bin, as the window's *Link the CLI* does.
    Link,
    /// Update a computer that runs only `switchboard serve` (no window, so the app never checks):
    /// reads the release feed, checks the package's signature and installs a newer version into
    /// the app. Run it as the account that owns the app, then restart `serve`. A copy installed
    /// from the .deb package is never replaced: it prints the latest version and one command that
    /// verifies and installs it (it needs curl and gpgv).
    Update {
        /// Only say whether a newer version exists; install nothing.
        #[arg(long)]
        check: bool,
        /// The installed app to replace: its `Fabric Switchboard.app` folder on macOS, its
        /// AppImage on Linux. Default: the one this command runs from.
        #[arg(long)]
        app: Option<PathBuf>,
        /// The release's `latest.json`, already downloaded (where github.com cannot be reached);
        /// with `--archive`.
        #[arg(long, requires = "archive")]
        feed: Option<PathBuf>,
        /// The release's package for this computer, already downloaded; with `--feed`. Its
        /// signature is checked as a downloaded one is.
        #[arg(long, requires = "feed")]
        archive: Option<PathBuf>,
    },
    /// What the installed app's About and tray do: automatic updates and opening at login.
    /// Answered by the running app only.
    App {
        #[command(subcommand)]
        command: AppCmd,
    },
    /// Switchboard's own usage analytics (counts and kinds only, docs/ANALYTICS.md): `on` or
    /// `off`; without either, the state. Only the installed app sends them.
    Analytics {
        /// `on` or `off`; without it, the state only.
        #[arg(value_enum)]
        state: Option<OnOff>,
    },
    /// GitHub repositories through your own `gh`: the owners it sees, an owner's repositories
    /// matched with this computer, and cloning one.
    Github {
        #[command(subcommand)]
        command: Github,
    },
    /// The Fabric Inbox key the Claude sign-in autopilot reads its sign-in mail with.
    /// Make a read key limited to the sign-in mailbox in Fabric Inbox's Settings, Agent access.
    Inbox {
        #[command(subcommand)]
        command: Inbox,
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
    /// Recent sanitized activity, oldest first. Filters and paging as the window's Activity
    /// page and `switchboard_events` use them.
    #[command(alias = "activity")]
    Events {
        /// Only one kind: switches, sign_ins, groups, moves, errors (any event that failed) or
        /// other.
        #[arg(long, value_parser = ["switches", "sign_ins", "groups", "moves", "errors", "other"])]
        kind: Option<String>,
        /// Only this account's events (an id from `switchboard accounts list`).
        #[arg(long = "account")]
        account: Option<String>,
        /// Only events before this time, in Unix seconds (exclusive); the line printed after a
        /// page names the value for the older one.
        #[arg(long)]
        before: Option<i64>,
        /// At most this many events, newest kept (1 to 500; default all the journal holds).
        #[arg(long, value_parser = clap::value_parser!(u64).range(1..=500))]
        limit: Option<u64>,
    },
    /// What the proxy relayed: requests, outcomes and the provider's token counts by group,
    /// folder (repository), account, coding agent and day. Reads only.
    #[command(alias = "statistics")]
    Stats {
        /// Days back, 1 to 90.
        #[arg(long, default_value_t = 30, value_parser = clap::value_parser!(i64).range(1..=90))]
        days: i64,
    },
    /// Which of your Claude homes (the ordinary one and each Switchboard made) is signed in to
    /// which MCP server: names, URLs and true or false, never a token. Findings name a server
    /// signed in in some homes and not in others, and two names for one URL in one home. A home
    /// whose credential item would ask for access reads `unknown`. Reads only.
    McpHomes,
    /// This computer's cap on live coding sessions (Claude Code and Codex, managed or not; a
    /// session quiet for 30 minutes outside a turn, on a limit stop or on a wait of its own is
    /// not counted): the cap, the board's share, how many count and how many are free. A hosted
    /// console waits at the cap; a board run (`dispatch`, or a hosted console Fabric opens) starts
    /// inside the board's share whatever the cap holds. Without a subcommand, reads only.
    Capacity {
        #[command(subcommand)]
        command: Option<CapacityCmd>,
    },
    /// The session duty officer: every five minutes it closes and continues a Claude Code session
    /// that stopped moving, closes one that finished and only holds a terminal, and gives a
    /// long-lived one a fresh process at the end of a turn. Without a subcommand: its settings,
    /// its last actions and what a check would do now.
    Duty {
        #[command(subcommand)]
        command: Option<DutyCmd>,
    },
    /// Whether Switchboard reads or changes your own Claude Code and Codex sign-in on this
    /// computer. Off on a teammate's computer that only serves seats an organization
    /// delegated: nothing captures, activates, renews or copies from your own sign-in, and the
    /// duty officer leaves your own sessions alone. Without a subcommand: the setting.
    OwnSignIn {
        #[command(subcommand)]
        command: Option<OwnSignInCmd>,
    },
    /// Other accounts of this computer the proxy serves (macOS and Linux). By default the proxy
    /// closes every connection from another account; a user named here may use one group's
    /// accounts as Claude Code, only with its own capability, which Switchboard writes into a
    /// settings file and never prints.
    Proxy {
        #[command(subcommand)]
        command: ProxyCmd,
    },
    /// The restricted entry another machine reaches over ssh: put it in that machine's key line
    /// in ~/.ssh/authorized_keys as `command="'<this program>' ssh-serve --sender-key KEY --pool
    /// GROUP",restrict` (the program quoted, since sshd runs the line through a shell). It
    /// answers only the calls that move, read, extend and call back a seat delegated into that
    /// group, reading them from SSH_ORIGINAL_COMMAND; it never runs a shell.
    SshServe {
        /// The other machine's key, as `switchboard settings signer` prints it there. Seats it
        /// delegates here carry it, and only calls naming it act on them.
        #[arg(long)]
        sender_key: String,
        /// The one group of this computer that machine may delegate seats into and see (its id,
        /// `switchboard project list`).
        #[arg(long)]
        pool: String,
        /// A name for that machine in your words (at most 80 characters), kept with its seats.
        #[arg(long)]
        from: Option<String>,
    },
    /// What needs a person: accounts to sign in to again, usage checks failing for over an hour,
    /// moves to another machine whose answer was lost and spending ceilings waiting for an answer,
    /// each with the action that fixes it. The accounts stay in their groups. Reads only.
    Attention,
    /// What went wrong: crashes of the app or `serve` found at the next start (with the
    /// exception type macOS reported) and, with usage counts on, the error counts of the last
    /// seven days by class and code. Reads only.
    Bugs,
    /// This Switchboard's health as one report (`agent-health/1`): each check's area, state (ok,
    /// warn, fail or unknown), a count and the tool that fixes it. Counts and kinds only, for a
    /// watcher that turns them into incidents. Reads only.
    Health,
    /// Report online/offline mode and the proxy address, without its capability.
    Status,
    /// The other machines this Switchboard knows, from accounts moved there and runs started
    /// there: as ssh names each, the switchboard and data folder used there, and when it was last
    /// used. With --probe, whether each one's Switchboard answers over ssh now and its version.
    Targets {
        /// Also ask each machine's Switchboard over ssh whether it answers now, and its version.
        #[arg(long)]
        probe: bool,
    },
    /// What a session waits on that it set up itself (a wake-up, a monitor, a cron job), kept so
    /// nothing closes it meanwhile. Claude Code's hooks run it; nothing else needs to.
    #[command(hide = true)]
    Monitor {
        #[command(subcommand)]
        command: MonitorCmd,
    },
    /// Machine groups: every other machine this computer's accounts live on is a group of its
    /// own, holding exactly the accounts there; an account on a machine is in no group here.
    /// `accounts send` moves one there, `accounts recall --pool GROUP` brings it back.
    Machines {
        #[command(subcommand)]
        command: MachinesCmd,
    },
    /// Sessions Switchboard launched that are starting or running: Claude Code, Codex, Kimi Code,
    /// Hermes, OpenCode and catalog agents. Account, agent, provider, group, mode, context, folder
    /// and times. Reads only.
    Sessions {
        /// Also list sessions that ended (their homes keep the last launch's record).
        #[arg(long, conflicts_with = "history")]
        all: bool,
        /// Every launch of the last 90 days, newest first, each with who launched it and what
        /// it did: one record per launch, also when a later launch reused its home.
        #[arg(long)]
        history: bool,
        /// With --history: at most this many launches, from 1 to 500 (default 500).
        #[arg(long, requires = "history", value_parser = clap::value_parser!(u16).range(1..=500))]
        limit: Option<u16>,
        /// With --history: only launches started before this time, in Unix seconds: the
        /// `next_before` of the previous page.
        #[arg(long, requires = "history")]
        before: Option<i64>,
        /// Every Claude Code and Codex process running for this user, Switchboard's or not: its
        /// folder, age, the launch it belongs to (or none) and how long its folder's transcripts
        /// have been quiet. Read from the process table here, without the owner.
        #[arg(long, conflicts_with_all = ["history", "all"])]
        machine: bool,
        /// Every live session joined to its launch (who started it, from where, for which task),
        /// the launches of the last day with who started whom, counts by group and surface, and
        /// what the one-minute watch found: a copy of a live session, a board run quiet for 30
        /// minutes, a launch whose process is gone. Read here, without the owner.
        #[arg(long, conflicts_with_all = ["history", "all", "machine"])]
        tree: bool,
        /// This computer's sessions as one list in four groups: needs you, running, waiting
        /// (stopped on a limit) and ended (the last day), each with its agent, account, folder,
        /// how it started and the acts it has: close the quiet copy, open here, end.
        #[arg(long, conflicts_with_all = ["history", "all", "machine", "tree"])]
        rows: bool,
        /// Read it on another machine, as ssh knows it (`switchboard targets`), through
        /// that machine's Switchboard; the answer is the one it gives there.
        #[arg(long)]
        target: Option<String>,
        /// With --target: that machine's switchboard, an absolute path; by default the one the
        /// newest move or run there used, else the macOS app's.
        #[arg(long, requires = "target")]
        remote: Option<String>,
        /// With --target: that machine's data folder, when its Switchboard runs with --data-dir.
        #[arg(long, requires = "target")]
        remote_data_dir: Option<String>,
        #[command(subcommand)]
        command: Option<SessionsCommand>,
    },
    /// Which account serves a provider's group (or one launch), since when and why: its handle,
    /// never its id, and the limit of the account it replaced. Fabric reads it. Reads only.
    Provenance {
        /// The provider whose group to read; give it or `--launch-id`.
        #[arg(long, value_enum)]
        provider: Option<ProviderArg>,
        /// The group's id (`switchboard projects list`); the default group when left out.
        #[arg(long, conflicts_with = "launch_id")]
        pool: Option<String>,
        /// One launch's account and session (`switchboard sessions --history`).
        #[arg(long)]
        launch_id: Option<String>,
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
        /// Run the session here with no terminal, for a launchd job or a scheduler:
        /// stdin is empty, its output goes where this command's goes, and the exit status is the
        /// agent's. The same session, home and tools as --in-place.
        #[arg(long, conflicts_with = "in_place")]
        headless: bool,
        /// With --in-place: run the session in a session host of its own, so it outlives
        /// this terminal and any app can open it with `switchboard attach <launch_id>`; Ctrl-]
        /// leaves it running. macOS and Linux.
        #[arg(long, requires = "in_place")]
        hosted: bool,
        /// Who is launching, for the launch record: a runner's own name (up to 64 letters,
        /// digits, dots, hyphens or underscores), such as `voc-learn`.
        #[arg(long, value_name = "NAME")]
        by: Option<String>,
        /// The launch this one continues (a launch id from `switchboard sessions --history`):
        /// that launch's record then names this one as where its work went.
        #[arg(long, value_name = "LAUNCH_ID")]
        parent: Option<String>,
        /// Where the launch is asked from: `dashboards` for Fabric Dashboards' console,
        /// `fabric` for Fabric's own console, `recovery` for a host resuming a stopped session;
        /// default `cli`.
        #[arg(long, value_enum, default_value = "cli")]
        surface: LaunchSurface,
        /// Start the session on another machine, as ssh knows it (`switchboard targets`): that
        /// machine's own `switchboard launch` runs there with these options, with --in-place in
        /// this terminal over ssh, so its account comes from the folder's group there. The folder
        /// is that machine's path.
        #[arg(long, value_name = "SSH_TARGET")]
        target: Option<String>,
        /// With --target: the switchboard there, an absolute path; by default the one the newest
        /// move or run there used, else the macOS app's.
        #[arg(long, requires = "target")]
        remote: Option<String>,
        /// With --target: the data folder there, when its Switchboard runs with --data-dir.
        #[arg(long, requires = "target")]
        remote_data_dir: Option<String>,
        /// Arguments for the agent, after `--` (for example `-- --resume <id>`, or `-- -p "…"`
        /// headless); with --in-place or --headless.
        #[arg(last = true)]
        args: Vec<String>,
    },
    /// Open a session a session host holds in this terminal, by its launch id (`switchboard
    /// sessions` marks it `attachable`): the same session another app shows. Ctrl-] detaches and
    /// leaves it running; its end ends this command with its exit status. macOS and Linux.
    Attach {
        /// The launch id.
        launch_id: String,
        /// Attach to a session on another machine, as ssh knows it (`switchboard targets`), through
        /// that machine's Switchboard; the session shows in this terminal.
        #[arg(long)]
        target: Option<String>,
        /// With --target: that machine's switchboard, an absolute path; by default the one the
        /// newest move or run there used, else the macOS app's.
        #[arg(long, requires = "target")]
        remote: Option<String>,
        /// With --target: that machine's data folder, when its Switchboard runs with --data-dir.
        #[arg(long, requires = "target")]
        remote_data_dir: Option<String>,
    },
    /// Starts one session in a session host and shows it in this terminal: the command a Terminal
    /// window Switchboard opens runs.
    #[command(hide = true)]
    SessionOpen {
        /// The launch id of the session.
        #[arg(long)]
        launch_id: String,
        /// The session's launch script.
        #[arg(long)]
        program: PathBuf,
    },
    /// Runs one session on a terminal of its own and serves it to viewers (started by
    /// `launch --hosted`).
    #[command(hide = true)]
    SessionHost {
        /// The launch id of the session.
        #[arg(long)]
        launch_id: String,
        /// The session's launch script.
        #[arg(long)]
        program: PathBuf,
        /// The terminal's height in rows.
        #[arg(long, default_value_t = 24)]
        rows: u16,
        /// The terminal's width in columns.
        #[arg(long, default_value_t = 80)]
        cols: u16,
        /// Close the session after this many minutes with no viewer, no input and no output, and
        /// no wait on a limit; 0 never. The default is the longest wake-up a session sets itself
        /// plus five minutes.
        #[arg(long, default_value_t = 65)]
        idle_minutes: u64,
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
        /// Also give this task a spending ceiling, in dollars, before it is offered: counted on
        /// the OpenRouter key since the task's first paid hand-over and checked at each automatic
        /// hand-over. A first ceiling or a lower one; raising one is done in the Switchboard window.
        #[arg(long, value_name = "USD")]
        ceiling: Option<f64>,
    },
    /// Fabric's command: start the default agent, with no person, on a workflow Fabric
    /// opened for a board request nobody answered, and stop it. Claude Code runs as `claude -p`,
    /// Codex as `codex exec`, in the account's isolated home, from Switchboard's own process (no
    /// Terminal). Answers the run's `launch_id` and `state` (`started`, `running` or `refused`
    /// with a `reason` code). Requires a running desktop app or serve.
    #[command(args_conflicts_with_subcommands = true, subcommand_negates_reqs = true)]
    Dispatch {
        #[command(subcommand)]
        command: Option<DispatchCommand>,
        /// The project's folder the session runs in (absolute).
        #[arg(long, required = true)]
        dir: Option<PathBuf>,
        /// The workflow Fabric wrote for the request, `wf_` and 16 hex digits.
        #[arg(long, required = true)]
        workflow: Option<String>,
        /// The hand-over Fabric offered to the default agent's provider (`handoff:` and 16 hex
        /// digits); the session accepts it for the account Switchboard chooses.
        #[arg(long, required = true)]
        handoff: Option<String>,
        /// The board request the session answers, a UUID. Without it, the workflow's
        /// `board-request:` artifact names it; given, it wins.
        #[arg(long)]
        request: Option<String>,
        /// Fabric's own id for this dispatch, kept in the launch record (`launched_by.dispatch.id`).
        #[arg(long)]
        dispatch_id: Option<String>,
        /// Why the session starts: `request`, a board request nobody answered; `inbox`, the project's
        /// unread board messages (it settles by read marks, never with `--target`); `resume`, a
        /// Fabric task a session let go of unfinished (`--request` names the task).
        #[arg(long, value_enum, default_value = "request")]
        reason: DispatchReason,
        /// `ask`: the agent's own defaults. `all`: Claude Code with
        /// --dangerously-skip-permissions, Codex with --dangerously-bypass-approvals-and-sandbox.
        #[arg(long, value_enum, default_value = "all")]
        permissions: DispatchPermissions,
        /// Stop the run after this many minutes, 1 to 1440.
        #[arg(long, default_value_t = 60, value_parser = clap::value_parser!(u32).range(1..=1440))]
        timeout_minutes: u32,
        /// Run the session on another machine, as ssh knows it, in a fresh worktree of this
        /// folder's repository there; its result comes back through `dispatch status`. `auto`:
        /// this machine or one of --candidates, whichever has the most room.
        #[arg(long)]
        target: Option<String>,
        /// With --target: that machine's switchboard, an absolute path; by default the macOS app's.
        #[arg(long, requires = "target")]
        remote: Option<String>,
        /// With --target: that machine's data folder, when its Switchboard runs with --data-dir.
        #[arg(long, requires = "target")]
        remote_data_dir: Option<String>,
        /// With --target auto: the machines to compare with this one, comma-separated; the
        /// request runs where the agent's accounts have the most room left.
        #[arg(long, value_delimiter = ',', requires = "target")]
        candidates: Vec<String>,
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
    /// Provider keys for agents found in Project Observatory, by name: list them, and add one so
    /// an agent starts on it at once. The value goes from Observatory's vault to Switchboard's
    /// through a pipe, never through an argument, a file or the screen.
    Keys {
        #[command(subcommand)]
        command: KeysCommand,
    },
    /// Accounts lent between the groups you choose: every 15 minutes, a chosen group out of room
    /// borrows an account with room from another chosen group, which keeps it as its own; the
    /// account goes back when the need passes. Off until you choose groups.
    Balance {
        #[command(subcommand)]
        command: BalanceCommand,
    },
    /// Groups fed from the default group: `status` reads the fed groups, the default group's
    /// accounts and the newest moves; `set` changes how many ready accounts a fed group keeps.
    /// Turn a group's feed on or off with `switchboard project feed`.
    Feed {
        #[command(subcommand)]
        command: FeedCommand,
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
    /// Spending ceilings for paid fallback agents: a daily ceiling in dollars for this computer
    /// or for a group on its own key, and one for a task counted since its first paid hand-over.
    /// From here, set a first ceiling or a lower one, and keep one that was reached; raising,
    /// doubling or clearing one is done in the Switchboard window.
    Ceiling {
        #[command(subcommand)]
        command: CeilingCommand,
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
    /// Where this data folder keeps its sign-ins. Quit Switchboard first.
    Vault {
        #[command(subcommand)]
        command: VaultCommand,
    },
}
#[derive(Subcommand)]
enum VaultCommand {
    /// `file`: 0600 files in the data folder, only on a machine with no graphical login, where
    /// this computer's own store is locked (a `serve` under a LaunchDaemon, a Linux VM with no
    /// session bus). `native`: this computer's own store, the default. Moves every sign-in.
    Use {
        /// `file` or `native`.
        #[arg(value_enum)]
        kind: VaultKindArg,
        /// With `file`: move the sign-ins this computer's own store holds into files even though
        /// it works in this session (a space whose keychain opens only in its own session, the
        /// daemon then reading the files). Run again to move what it could not read.
        #[arg(long)]
        move_from_native: bool,
    },
}
#[derive(Clone, Copy, ValueEnum)]
enum VaultKindArg {
    Native,
    File,
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
    /// Switch a group off (no folder is routed to it and its accounts serve no request) or on
    /// again. The pay-per-token group turns on only with a daily spending ceiling set
    /// (`switchboard ceiling set --scope project:<pool>`).
    Switch {
        /// The group id (`switchboard project list`).
        #[arg(long)]
        pool: String,
        /// `on` or `off`.
        #[arg(value_enum)]
        state: OnOff,
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
        /// An organization whose accounts the group runs on (its id or name from `switchboard
        /// org list`), or `personal`; repeat for a group of several organizations, the first
        /// being its own context. The group serves every account it holds whatever its
        /// organizations; the answer's `warnings` name the accounts outside them.
        #[arg(long)]
        organization: Vec<String>,
        /// With `--add-account`: move each account here from its group. Without it, an account
        /// at home in another group keeps its home and serves this group too.
        #[arg(long = "move-accounts")]
        move_accounts: bool,
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
        /// An organization of the group, by id or name; repeat for each, the first being the
        /// group's own context. `personal` alone makes it personal. Left out, a new group is
        /// personal and a saved one keeps its organizations.
        #[arg(long = "organization")]
        organizations: Vec<String>,
        /// Move each listed account here from its group instead of sharing it.
        #[arg(long)]
        move_accounts: bool,
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
    /// Feed a group from the default group, or stop: Switchboard keeps the group's ready
    /// accounts by lending it accounts of the default group and taking back one that runs out
    /// once another serves the group. Turning it off gives back every account the feed moved.
    Feed {
        /// The group id (`switchboard project list`).
        pool: String,
        /// on or off.
        #[arg(value_enum)]
        state: OnOff,
        /// 1 is the highest: a higher group is served first when accounts run short; of two
        /// with the same priority, the group id first in alphabetical order. Without it, a
        /// group fed for the first time goes after the others. Not with off.
        #[arg(long, value_parser = clap::value_parser!(u32).range(1..=99))]
        priority: Option<u32>,
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
    Export {
        /// Sign the file with this computer's settings signing key, so members can check it came
        /// from you (`settings signer` prints the key to publish).
        #[arg(long)]
        sign: bool,
        /// Only this group (its id from `switchboard project list`) and the organizations,
        /// ceilings and rotation settings it needs; repeat for each. For a member's machine.
        #[arg(long = "group")]
        groups: Vec<String>,
    },
    /// The public key this computer signs settings files with (made on first use). Publish it
    /// for your organization's members; its private half stays in this computer's vault.
    Signer,
    /// Carry this machine's settings to another machine over ssh: the signed file is imported
    /// there (`settings import`), so its groups, organizations and agents' settings match these.
    /// Shows what it would do there; `--apply` does it. No account goes with it.
    Sync {
        /// The other machine as ssh knows it (`user@host`), reachable without a prompt.
        #[arg(long)]
        to: String,
        /// Only this group (its id from `switchboard project list`); repeat for each.
        #[arg(long = "group")]
        groups: Vec<String>,
        /// Import it there.
        #[arg(long)]
        apply: bool,
        /// That machine's switchboard, an absolute path (default: the one the newest move
        /// there used, else the macOS app's).
        #[arg(long)]
        remote: Option<String>,
        /// That machine's data folder, when its Switchboard runs with `--data-dir`.
        #[arg(long)]
        remote_data_dir: Option<String>,
    },
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
        /// With `--apply`: clone the groups' repositories that are on GitHub but not on this
        /// computer into the first `--under` folder, with your GitHub CLI (`gh`), so their groups
        /// are made from them.
        #[arg(long)]
        clone: bool,
        /// The key your organization published (`settings signer` printed it): the file must
        /// be signed by it, or nothing is applied.
        #[arg(long)]
        signer: Option<String>,
    },
}
#[derive(Subcommand)]
enum Inbox {
    /// Save the key: its Client Secret is read from standard input and kept in the vault.
    Set {
        /// Your Fabric Inbox's https address, such as https://inbox.example.com.
        #[arg(long)]
        origin: String,
        /// The key's Client Id, as Fabric Inbox showed it.
        #[arg(long)]
        client_id: String,
    },
    /// Whether a key is saved and for which server (never the key).
    Status {
        /// Also ask the server whether it answers the key.
        #[arg(long)]
        check: bool,
    },
    /// Delete the saved key.
    Remove,
}
#[derive(Subcommand)]
enum AppCmd {
    /// The update state; `on` or `off` sets *Install updates automatically*.
    Updates {
        /// `on` or `off`; without it, the state only.
        #[arg(value_enum)]
        state: Option<OnOff>,
    },
    /// Check for updates now, as About's *Check for updates* does.
    CheckUpdates,
    /// Install a downloaded update and restart, as the tray's *Restart to update* does.
    RestartToUpdate,
    /// Whether Switchboard opens at login; `on` or `off` sets it.
    LoginItem {
        /// `on` or `off`; without it, the state only.
        #[arg(value_enum)]
        state: Option<OnOff>,
    },
}
#[derive(Subcommand)]
enum Github {
    /// The owners your `gh` can see: you and your organizations.
    Owners,
    /// One owner's repositories, each with where it is on this computer and its group.
    Repos {
        /// The owner, as `github owners` lists it.
        owner: String,
    },
    /// Clone one repository into a folder.
    Clone {
        /// The owner.
        owner: String,
        /// The repository's name.
        name: String,
        /// The absolute folder to clone into; it must not exist yet.
        #[arg(long)]
        into: std::path::PathBuf,
    },
}
#[derive(Subcommand)]
enum Org {
    /// The organizations, then every account with the organization it belongs to and why, and
    /// the organization its address suggests, which `org assign` confirms.
    List,
    /// Connect PassionCode.ai: sign in in the browser, then your organizations there arrive
    /// here with their names and domains and are read again every 6 hours.
    Connect,
    /// Stop a PassionCode.ai connection that waits for the browser.
    Cancel,
    /// Read your PassionCode.ai organizations again now.
    Read,
    /// Where the PassionCode.ai connection stands (no token is shown).
    Passioncode,
    /// Delete the PassionCode.ai token; the organizations stay as local ones.
    Disconnect,
    /// Add an organization with the e-mail domains it owns. A domain suggests the organization
    /// for accounts on it; it places none of them.
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
    /// Remove it; its accounts are placed again by their team at the provider, else personal.
    Remove {
        /// Its id (`switchboard org list`).
        id: String,
    },
    /// Say where one account belongs: an organization's id, `personal`, or `auto` to let the
    /// rules decide again. Name several for an account that works for more than one: the first
    /// is its own, the rest it also works for. `--also` adds organizations and keeps the ones it
    /// has. Groups that hold the account keep it whatever its organizations; the answer names
    /// them under `outside`.
    Assign {
        /// The account id (`switchboard accounts list`).
        account: String,
        /// An organization's id or name, `personal`, or `auto`; several are allowed.
        #[arg(num_args = 0.., required_unless_present = "also")]
        organization: Vec<String>,
        /// An organization it also works for, added to the ones it has; repeat for several.
        #[arg(long = "also", conflicts_with = "organization")]
        also: Vec<String>,
        /// Also place the other seats of its Claude Team or ChatGPT workspace there; without it
        /// only this account is placed.
        #[arg(long)]
        team: bool,
    },
}

/// SB-417: `capacity set --board`: a number of board runs, or `default`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BoardShare {
    Runs(u32),
    Default,
}
fn board_share_arg(text: &str) -> Result<BoardShare, String> {
    if text == "default" {
        return Ok(BoardShare::Default);
    }
    text.parse::<u32>()
        .map(BoardShare::Runs)
        .map_err(|_| "Give the board's share as a whole number, or default.".to_owned())
}
#[derive(Subcommand)]
enum CapacityCmd {
    /// Set the cap: at most this many working coding sessions at once (1 to 500).
    Set {
        /// The most working coding sessions at once, 1 to 500.
        cap: u32,
        /// How many board runs may run whatever the cap holds: a number from 0 to the cap, or
        /// `default` (a quarter of the cap, rounded up). Left out: the share set before is kept,
        /// clamped to the new cap.
        #[arg(long, value_parser = board_share_arg)]
        board: Option<BoardShare>,
    },
    /// Remove the cap: nothing waits.
    Clear,
}

#[derive(Subcommand)]
enum ProxyCmd {
    /// Let this account of the computer use one group's accounts through the proxy. Allowing an
    /// account already listed changes its group and folders and keeps its capability.
    AllowUser {
        /// The account's user name or uid.
        user: String,
        /// The group (id or name) whose accounts it may use.
        #[arg(long)]
        group: String,
        /// A folder it works in, for statistics (absolute; repeat for more).
        #[arg(long = "folder")]
        folders: Vec<PathBuf>,
        /// Write the account's Claude Code settings file here (absolute): the proxy's address and
        /// the account's capability, owned by you, mode 0640 with the account's group when you
        /// may set it, else 0600 with the sudo line that hands it over.
        #[arg(long)]
        write_settings: Option<PathBuf>,
    },
    /// Take the account off the list: its connections are closed from the next one, and its
    /// capability is refused at once.
    DisallowUser {
        /// The account's user name or uid.
        user: String,
    },
    /// The accounts the proxy serves, with their groups, folders and settings files.
    Users,
    /// Give the account a new capability: the old one is refused at once, and the settings file
    /// Switchboard wrote for it is written again.
    RotateUser {
        /// The account's user name or uid.
        user: String,
        /// Write the settings file here instead of where it was written before.
        #[arg(long)]
        write_settings: Option<PathBuf>,
    },
}
#[derive(Subcommand)]
enum OwnSignInCmd {
    /// Switchboard may read and change your own sign-in (the default).
    On,
    /// Switchboard never reads or changes your own sign-in on this computer.
    Off,
}
#[derive(Subcommand)]
enum DutyCmd {
    /// Turn the duty officer on (the default).
    On,
    /// Turn the duty officer off: nothing is closed or restarted.
    Off,
    /// Change its limits; each one left out keeps its value.
    Set {
        /// Minutes a session may stay busy with no transcript write, no CPU and no command of its own before it is restarted (10 to 1440).
        #[arg(long)]
        stuck_minutes: Option<u32>,
        /// Kept for older settings and unused: a session waiting for an answer is left alone (10 to 1440).
        #[arg(long)]
        waiting_minutes: Option<u32>,
        /// Minutes a finished session (idle, none of its commands running) may stay before it is closed (10 to 1440; it is never closed before 65, the longest wake-up a session can set itself).
        #[arg(long)]
        idle_minutes: Option<u32>,
        /// Kept for older settings and unused: nothing is restarted for its age (1 to 72).
        #[arg(long)]
        max_age_hours: Option<u32>,
    },
    /// Check now instead of waiting for the next five minutes.
    Run {
        /// Only list what would be done.
        #[arg(long)]
        dry_run: bool,
    },
}

#[derive(Subcommand)]
enum MonitorCmd {
    /// Read one hook input (`PreToolUse` on ScheduleWakeup, Monitor, CronCreate or CronDelete, or
    /// `SessionEnd`) on standard input and record the session's wait; prints `{}`, never fails.
    Hook,
    /// Mark a Claude Code session as holding live work another tool tracks (a Fabric task's
    /// claim): the duty officer, a duplicate close, a session host's idle close and a restart
    /// wave leave it alone until the end given. Renew it as the claim is renewed; 0 releases it.
    Hold {
        /// The session id.
        #[arg(long)]
        session: String,
        /// Until when, Unix seconds, at most seven days ahead; 0 releases the hold.
        #[arg(long)]
        until: i64,
        /// Why it is held, one short word (`fabric_claim` for a Fabric task's claim): echoed in
        /// the answer and logged as a code.
        #[arg(long)]
        reason: Option<String>,
    },
}
#[derive(Subcommand)]
enum MachinesCmd {
    /// Each machine group with its accounts, and the accounts marked used elsewhere that no
    /// machine group holds. Reads only.
    List {
        /// Also read each machine's Switchboard over ssh, all at once, 20 seconds per call at
        /// most: whether it answers and each account's copy there.
        #[arg(long)]
        live: bool,
    },
    /// Place the accounts marked used elsewhere into their machines' groups: by the move recorded
    /// here, else their location naming a known machine. Without --apply, only what it would do;
    /// an account whose machine cannot be told stays where it is.
    Adopt {
        /// Make the change.
        #[arg(long)]
        apply: bool,
    },
    /// Record that an account lives on a machine, without ssh: it joins that machine's group and
    /// leaves every group here.
    Place {
        /// The account id (`switchboard accounts list`).
        id: String,
        /// The machine as ssh knows it (`user@host`, or a Host from ~/.ssh/config).
        #[arg(long)]
        on: String,
    },
    /// Remove a machine group that holds no account any more.
    Forget {
        /// The machine group: its id, its name or another name of it.
        machine: String,
    },
    /// Trust a key as another machine's. Every move, call back, remote read or dispatch first has
    /// that machine sign a fresh challenge; its first answer is trusted, and a later one signed
    /// by another key is refused. After checking a machine that changed (reinstalled, replaced),
    /// run `switchboard settings signer` there and trust the key it prints.
    Trust {
        /// The machine as ssh knows it (`user@host`, or a Host from ~/.ssh/config).
        machine: String,
        /// The key `switchboard settings signer` printed on that machine.
        #[arg(long)]
        key: String,
    },
    /// The keys trusted for other machines: machine names, public keys and times.
    Keys,
    /// Sign another machine's challenge with this computer's key. Run by that machine over ssh.
    #[command(hide = true)]
    Prove {
        /// The other machine's challenge: 64 hexadecimal characters.
        #[arg(long)]
        nonce: String,
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
        /// Use an account given to an agent anyway (`accounts assign`); the activity records it.
        #[arg(long)]
        force: bool,
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
        /// Where it works, in your words (a machine and its user, a colleague), at most 80
        /// characters; marks it as used on another machine. An empty value clears it.
        #[arg(long)]
        location: Option<String>,
        /// With `--elsewhere false` or `--enabled true`: use it here although its sign-in was
        /// moved to another machine and not called back (its copy there is gone).
        #[arg(long)]
        force: bool,
    },
    /// Hand an account over to another machine or a colleague: writes a signed record
    /// without its credential and marks it here as used elsewhere. The receiver runs
    /// `switchboard accounts receive` with the file and signs in.
    HandOver {
        /// The account id (`switchboard accounts list`).
        id: String,
        /// Who receives it: a machine and its user, or a colleague (at most 80 characters).
        /// The record is printed: `switchboard accounts hand-over ID --to NAME > handover.json`.
        #[arg(long)]
        to: String,
    },
    /// Move an account's sign-in to another machine over ssh, without signing in again there:
    /// marks it here as used elsewhere first, saves it there with `switchboard accounts add`,
    /// checks there that the provider answers, and undoes both on any failure. Only a Claude or
    /// Codex subscription sign-in; ssh must reach the machine without a prompt.
    Send {
        /// The account id (`switchboard accounts list`).
        id: String,
        /// The other machine as ssh knows it (`user@host`, or a Host from ~/.ssh/config).
        #[arg(long)]
        to: String,
        /// The group it joins there (`default`: the default group).
        #[arg(long)]
        pool: Option<String>,
        /// The other machine's switchboard, an absolute path; by default the macOS app's.
        #[arg(long)]
        remote: Option<String>,
        /// The other machine's data folder, an absolute path, when its Switchboard runs with
        /// `--data-dir`.
        #[arg(long)]
        remote_data_dir: Option<String>,
        /// Delegate the seat: the other machine keeps it as a guest in the group named
        /// (never its default group), pinned to this machine's key; it cannot be moved on, given
        /// to an agent or lent there, and only this machine calls it back (`accounts recall`).
        #[arg(long)]
        delegate: bool,
        /// With `--delegate`: the lease in hours (1 to 720), 48 by default. This machine extends
        /// it at each background read of the other machine; past it, the copy there is switched
        /// off and its sign-in removed.
        #[arg(long, requires = "delegate")]
        lease_hours: Option<u32>,
    },
    /// Take back a sign-in this machine moved to another one: that machine stops using it and
    /// gives its tokens back over ssh, the provider is checked here, then its copy is removed
    /// there and its account list read to prove it. Claude and Codex sign-ins.
    Recall {
        /// The account id here (`switchboard accounts list`).
        id: String,
        /// The other machine as ssh knows it, as the move named it; by default the machine the
        /// recorded move named, else the account's machine group.
        #[arg(long)]
        from: Option<String>,
        /// The group here it comes back to (a group id, or `default`); by default the group it
        /// left, else the default group.
        #[arg(long)]
        pool: Option<String>,
        /// The account's id on the other machine; by default the one the recorded move names.
        #[arg(long)]
        remote_id: Option<String>,
        /// The other machine's switchboard, an absolute path; by default the one the move ran.
        #[arg(long)]
        remote: Option<String>,
        /// The other machine's data folder, an absolute path; by default the one the move named.
        #[arg(long)]
        remote_data_dir: Option<String>,
    },
    /// Another machine's Switchboard read over ssh: its accounts (no credential), groups
    /// and organizations, and whether it is connected to PassionCode.ai. Reads only.
    Remote {
        /// The other machine as ssh knows it, reachable without a prompt.
        #[arg(long)]
        on: String,
        /// Its switchboard, an absolute path; by default the one the newest move there ran.
        #[arg(long)]
        remote: Option<String>,
        /// Its data folder, an absolute path; by default the one the newest move there named.
        #[arg(long)]
        remote_data_dir: Option<String>,
    },
    /// Check another machine against the moves recorded here: a copy gone, turned off or
    /// refused there, an account used on both machines, a mark here with no record. Reads only.
    Check {
        /// The other machine as ssh knows it, reachable without a prompt.
        #[arg(long)]
        on: String,
        /// Its switchboard, an absolute path; by default the one the newest move there ran.
        #[arg(long)]
        remote: Option<String>,
        /// Its data folder, an absolute path; by default the one the newest move there named.
        #[arg(long)]
        remote_data_dir: Option<String>,
    },
    /// Moves, hand-overs and call backs recorded on this machine, newest last: account ids, the
    /// other machine, group, the id there and who ordered it. Reads only.
    Seats {
        /// How many of the newest lines (1 to 1000).
        #[arg(long, default_value_t = 50)]
        limit: usize,
    },
    /// On the machine an account was moved to: give its sign-in back to the machine that calls
    /// it back. Run only by `accounts recall` over ssh; it refuses to print to a terminal.
    Surrender {
        /// The account id on this machine.
        id: String,
        /// Use the account here again: the calling machine could not take it.
        #[arg(long)]
        undo: bool,
    },
    /// Receive an account handed over to you: shows whose sign-in and into which group;
    /// with `--apply`, starts that sign-in. Its credential never travels: you sign in here.
    Receive {
        /// The hand-over file.
        file: PathBuf,
        /// The giver's published key (`switchboard settings signer`): the file must be signed by it.
        #[arg(long)]
        signer: Option<String>,
        /// Start the sign-in.
        #[arg(long)]
        apply: bool,
        /// With `--apply`: no Terminal; the sign-in URL is in `login status` and the code goes to
        /// `login code`.
        #[arg(long)]
        headless: bool,
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
        /// Use an account given to an agent anyway (`accounts assign`); the activity records it.
        #[arg(long)]
        force: bool,
    },
    /// Give an account to an agent or service that holds its sign-in now. Switchboard keeps
    /// renewing it and checking its usage, so its activity stays visible, but never selects it,
    /// switches to it or launches on it by itself, and refuses a switch to it without `--force`.
    /// Clears a mark of being used on another machine.
    Assign {
        /// The account id (`switchboard accounts list`).
        id: String,
        /// Who holds it, in your words (at most 120 characters), such as `Frame Agent on Studio`.
        #[arg(long)]
        to: String,
        /// Clear the mark although its sign-in was moved to another machine and not called back.
        #[arg(long)]
        force: bool,
    },
    /// Take an account back from the agent it was given to: Switchboard may use it again.
    Unassign {
        /// The account id (`switchboard accounts list`).
        id: String,
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
        /// Start the session with the agent's permission prompts on, whatever Switchboard's own
        /// setting: Claude Code in `--permission-mode manual`, Codex asking on request with no
        /// bypass (a sandbox the person set stays). Only for a Claude Code or Codex hand-over;
        /// another stays waiting. Its launch record says `permissions: ask`.
        #[arg(long)]
        ask: bool,
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
#[derive(Subcommand)]
enum CeilingCommand {
    /// Every spending ceiling, the questions a reached one asked, and the last hand-over.
    List,
    /// Set a first ceiling for a scope, or lower one, in dollars from 0.01 to 1000.
    Set {
        /// `machine`, `project:<group id>` or `workflow:<wf_id>`.
        #[arg(long, default_value = "machine")]
        scope: String,
        /// Dollars: per UTC day for this computer and a group, since the task's first paid
        /// hand-over for a task.
        usd: f64,
    },
    /// Keep a reached ceiling as it is: answers its question. Doubling it is answered in the
    /// Switchboard window.
    Keep {
        /// The question's id, `q_` and 16 hex digits (`switchboard ceiling list`).
        id: String,
    },
    /// Double a reached ceiling: answers its question. Only with the operator's words
    /// (`--operator-said`, `--said-at`, `--said-in`), otherwise in the window.
    Double {
        /// The question's id, `q_` and 16 hex digits (`switchboard ceiling list`).
        id: String,
    },
    /// Clear a scope's ceiling. Only with the operator's words, otherwise in the window.
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
enum BalanceCommand {
    /// The chosen groups, the accounts lent now and why, the last pass's figures and the
    /// newest lends and returns.
    Status,
    /// Choose the groups that lend accounts to each other. Taking a group out, or the last
    /// one, returns its lent accounts at once.
    Groups {
        /// A group's id (`switchboard project list`) or `default` to add; repeat for more.
        #[arg(long = "add", value_name = "ID")]
        add: Vec<String>,
        /// A chosen group's id to take out; repeat for more.
        #[arg(long = "remove", value_name = "ID")]
        remove: Vec<String>,
        /// Whether a group made later joins the chosen ones by itself: true or false.
        #[arg(long, value_name = "BOOL")]
        all_new: Option<bool>,
        /// A group with no folders to mark as backup: it only lends, keeps no minimum and is
        /// asked first (DEC-0016); repeat for more. Marking it also chooses it.
        #[arg(long = "backup-add", value_name = "ID")]
        backup_add: Vec<String>,
        /// A backup group to unmark (it stays chosen); repeat for more.
        #[arg(long = "backup-remove", value_name = "ID")]
        backup_remove: Vec<String>,
    },
}
#[derive(Subcommand)]
enum FeedCommand {
    /// The fed groups with their priority and ready accounts, the default group's accounts, what
    /// the feed moved and why. Reads only.
    Status,
    /// How many ready accounts each fed group keeps and how far ahead a reset counts, for every
    /// fed group.
    Set {
        /// How many ready accounts each fed group keeps, 1 to 10 (2 until you change it).
        #[arg(long, value_parser = clap::value_parser!(u32).range(1..=10))]
        ready: Option<u32>,
        /// Minutes ahead an account whose limit resets counts as ready, 0 to 720 (30 until you
        /// change it).
        #[arg(long, value_parser = clap::value_parser!(u32).range(0..=720))]
        lookahead: Option<u32>,
    },
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
    /// Whether the Claude Code and Codex sessions Switchboard starts skip their permission
    /// prompts: `bypass` (the default) or `ask`.
    Permissions {
        /// `bypass` or `ask`.
        mode: String,
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
        /// With --service: exactly this group's own key, never the folder's or the machine's.
        /// Session scripts name the key their spending check read this way.
        #[arg(long, requires = "service", conflicts_with_all = ["dir", "machine"])]
        pool: Option<String>,
        /// With --service: exactly the machine's key, never a group's.
        #[arg(long, requires = "service", conflicts_with = "dir")]
        machine: bool,
    },
    /// The OpenRouter key agents run on: save it (piped on stdin), see what it may still spend,
    /// change the default model, or remove it.
    Openrouter {
        #[command(subcommand)]
        command: OpenrouterCommand,
    },
    /// Anthropic API keys agents run on directly, each under a name you choose (`team-a`,
    /// `personal`); `agents launch hermes --anthropic <name>` picks one.
    Anthropic {
        #[command(subcommand)]
        command: AnthropicCommand,
    },
    /// The keys an agent can start on here, by name: the OpenRouter key a launch in the folder
    /// spends first (its group's own, else this computer's), the other OpenRouter keys with the
    /// group that uses them, then the Anthropic keys for an agent that takes one. Never a value.
    LaunchKeys {
        /// The agent's catalog id (`switchboard agents list`).
        agent: String,
        /// The folder the agent would start in; default: the current folder.
        #[arg(long)]
        folder: Option<PathBuf>,
    },
    /// Hermes's model and provider (its own config); `set` changes them with `hermes config set`.
    Hermes {
        #[command(subcommand)]
        command: Option<HermesCommand>,
    },
    /// Start an agent in a folder in Terminal: on the group's API-key account through the
    /// proxy, with `--openrouter` on the saved OpenRouter key, or with `--anthropic <name>` on the
    /// Anthropic key saved under that name (Hermes).
    #[command(group = clap::ArgGroup::new("key_source").args(["openrouter", "anthropic"]).multiple(false))]
    Launch {
        /// The agent's catalog id (`switchboard agents list`).
        agent: String,
        /// The group id whose API-key account it uses (not with `--openrouter` or `--anthropic`).
        #[arg(long, default_value = "default", conflicts_with_all = ["openrouter", "anthropic"])]
        pool: String,
        /// The absolute folder it starts in; default: the current folder.
        #[arg(long)]
        dir: Option<PathBuf>,
        /// Run on the OpenRouter key saved with `switchboard agents openrouter set`.
        #[arg(long, conflicts_with = "anthropic")]
        openrouter: bool,
        /// Run on the Anthropic key saved under this name (`switchboard agents anthropic set
        /// <name>`), such as `team-a` or `personal`.
        #[arg(long, value_name = "NAME")]
        anthropic: Option<String>,
        /// The model for this launch: an OpenRouter id such as `moonshotai/kimi-k2`, or with
        /// `--anthropic` an Anthropic id such as `claude-sonnet-5-5`; default: the key's model.
        /// Through the proxy, an Anthropic id for Claude Code, and for Hermes on the
        /// pay-per-token group (where it is needed).
        #[arg(long)]
        model: Option<String>,
    },
}
#[derive(Subcommand)]
enum KeysCommand {
    /// The Anthropic, OpenAI and OpenRouter keys Project Observatory holds, by project, env and
    /// name, each with where it is saved here; `add` saves one.
    Observatory {
        #[command(subcommand)]
        command: Option<ObservatoryCommand>,
    },
}
#[derive(Subcommand)]
enum ObservatoryCommand {
    /// Every provider key Observatory holds that an agent can run on, by name; never a value.
    List,
    /// Save a key Observatory holds in Switchboard by its name. An Anthropic key becomes an
    /// account in the pay-per-token group (counted against its daily ceiling); with `--to agent`
    /// or `--to both` also a named key Hermes runs on directly. An OpenRouter key becomes this
    /// computer's OpenRouter key, or a group's with `--pool`.
    Add {
        /// The key as Observatory names it: `project/env/NAME`, as `switchboard keys
        /// observatory` lists it.
        slot: String,
        /// For an Anthropic key: `account` (the default), `agent` or `both`.
        #[arg(long)]
        to: Option<String>,
        /// The account's name, or the named key's name (lowercase letters, digits, hyphens);
        /// default: from the project's name.
        #[arg(long = "as", value_name = "NAME")]
        name: Option<String>,
        /// For an OpenRouter key: the group whose folders run on it; default this computer's key.
        #[arg(long)]
        pool: Option<String>,
        /// The default model of a named key: an Anthropic id or an OpenRouter id.
        #[arg(long)]
        model: Option<String>,
        /// Replace a named Anthropic key or an OpenRouter key already saved there.
        #[arg(long)]
        replace: bool,
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
    /// Where a Kimi sign-in stands: pending, complete (ready to finish) or ended.
    Status {
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
enum AnthropicCommand {
    /// The saved keys by name with their default models; never a key.
    List,
    /// Save the key piped on stdin (`sk-ant-api…`) under a name; it goes to the OS vault, never to
    /// a file. Saving again under the same name replaces it.
    Set {
        /// The key's name, such as `team-a` or `personal`.
        name: String,
        /// The model agents start on, an Anthropic id such as `claude-sonnet-5-5`.
        #[arg(long)]
        model: Option<String>,
        /// Read the key from piped stdin. Interactive input is refused (no echo).
        #[arg(long, required = true)]
        key_stdin: bool,
    },
    /// Change a key's default model.
    Model {
        /// The key's name.
        name: String,
        /// An Anthropic model id, such as `claude-sonnet-5-5`.
        model: String,
    },
    /// Remove a key from Switchboard and the OS vault.
    Remove {
        /// The key's name.
        name: String,
    },
    /// Print a key, for a session script's `export`; never to paste anywhere.
    Key {
        /// The key's name.
        name: String,
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
        /// The group id the account joins (`default`: the default group).
        #[arg(long, default_value = "default")]
        pool: String,
        /// Claude only, macOS and Linux: run the sign-in without Terminal; `login status` then
        /// gives the sign-in URL and `login code` takes the code its page shows.
        #[arg(long)]
        headless: bool,
    },
    /// Give a sign-in begun with --headless the code its page showed, piped on stdin.
    Code {
        /// The id `login begin` printed.
        #[arg(long)]
        login_id: String,
    },
    /// Every sign-in waiting, oldest first: its id, provider, label, group and state.
    Pending,
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
#[derive(Subcommand)]
enum DispatchCommand {
    /// Stop a dispatched run: its process group (`taskkill /T` on Windows). Answers `stopped`, or
    /// `already_ended` for a run that had ended.
    Stop {
        /// The run's launch id, as `switchboard dispatch` answered it.
        #[arg(long)]
        launch_id: String,
        /// Only record the end of a run whose process is gone; a run still running is refused and
        /// left running.
        #[arg(long)]
        if_gone: bool,
    },
    /// A dispatched run's state (`running`, `exited` with its exit code, `stopped`, `timed_out`,
    /// `lost`, `unknown` when its machine does not answer) and, for a run on another machine, the
    /// text of its result file.
    Status {
        /// The run's launch id, as `switchboard dispatch` answered it.
        #[arg(long)]
        launch_id: String,
    },
}
/// Why `switchboard dispatch` starts a session.
#[derive(Clone, Copy, ValueEnum)]
enum DispatchReason {
    /// A board request nobody answered.
    Request,
    /// The project's unread board messages; the run settles by read marks (no `--target`).
    Inbox,
    /// A Fabric task a session left unfinished; `--request` is the task id (SB-398).
    Resume,
}
/// How much a dispatched run may do without asking.
#[derive(Clone, Copy, ValueEnum)]
enum DispatchPermissions {
    /// The agent's own defaults.
    Ask,
    /// Everything the agent can skip asking for.
    All,
}
#[derive(Subcommand)]
enum SessionsCommand {
    /// One launch with its chain: who launched it, up to the first launch, and the launches it
    /// started or that took its work over.
    Show {
        /// The launch id (`switchboard sessions --history`).
        launch_id: String,
    },
    /// End a session a session host holds (marked `attachable`): its process gets a hangup, a
    /// SIGTERM 5 s later if it still runs, and every viewer is told it ended. macOS and Linux.
    End {
        /// The launch id.
        launch_id: String,
        /// End a session on another machine, as ssh knows it (`switchboard targets`), through
        /// that machine's Switchboard; it answers as it does there.
        #[arg(long)]
        target: Option<String>,
        /// With --target: that machine's switchboard, an absolute path; by default the one the
        /// newest move or run there used, else the macOS app's.
        #[arg(long, requires = "target")]
        remote: Option<String>,
        /// With --target: that machine's data folder, when its Switchboard runs with --data-dir.
        #[arg(long, requires = "target")]
        remote_data_dir: Option<String>,
    },
    /// Close the quiet one of two processes holding one Claude Code session (`switchboard
    /// attention` lists it as session_duplicate): of the copies neither busy nor running a
    /// command, the one quiet longest gets SIGTERM, then SIGKILL after ten seconds. With --pid,
    /// the copy named (the attention item's close.pid when every copy works), refused while it
    /// is answering; a command of its own still running ends with it. macOS and Linux.
    CloseCopy {
        /// The session id (the duplicate's session_id).
        session_id: String,
        /// The copy to close, by its process id.
        #[arg(long)]
        pid: Option<u32>,
        /// Close the copy on another machine, as ssh knows it (`switchboard targets`), through
        /// that machine's Switchboard (0.7.98 or later); it answers as it does there.
        #[arg(long)]
        target: Option<String>,
        /// With --target: that machine's switchboard, an absolute path; by default the one the
        /// newest move or run there used, else the macOS app's.
        #[arg(long, requires = "target")]
        remote: Option<String>,
        /// With --target: that machine's data folder, when its Switchboard runs with --data-dir.
        #[arg(long, requires = "target")]
        remote_data_dir: Option<String>,
    },
    /// Continue a session that ended, as that same session: `claude --resume` in its own Claude
    /// config folder and its folder, in a Terminal window of its own, so its history and context
    /// are the ones it had. Refused while a process still holds it, and when its history is gone.
    Continue {
        /// The session id (`sessions --rows` names it).
        session_id: String,
        /// In this terminal instead of a Terminal window of its own.
        #[arg(long)]
        in_place: bool,
        /// With --in-place: in a session host under a launch of its own, so its viewers, restart
        /// waves and updates leave it running; this command attaches to it.
        #[arg(long, requires = "in_place")]
        hosted: bool,
        /// With --in-place: where it is asked from, for its launch record.
        #[arg(long, value_enum, requires = "in_place")]
        surface: Option<LaunchSurface>,
    },
    /// Restart every running Claude Code session, for example so it loads new skills: each ends
    /// at the end of its current turn and is resumed as itself; this command's own session
    /// restarts last. `plan` first, then `apply` with its digest. macOS and Linux.
    Restart {
        #[command(subcommand)]
        command: RestartCmd,
    },
}
#[derive(Subcommand)]
enum RestartCmd {
    /// Which sessions a wave would restart, which it skips and why, and the plan's digest.
    /// Restarts nothing.
    Plan,
    /// Start the wave of the plan whose digest is given; refused when the sessions changed since.
    Apply {
        /// The digest `sessions restart plan` answered.
        #[arg(long)]
        digest: String,
        /// Why the sessions restart, in a few words; recorded with the wave.
        #[arg(long)]
        reason: String,
    },
    /// A wave's state: each session pending, resuming, back, continuing, failed or skipped.
    Status {
        /// The wave id; by default the newest.
        wave: Option<String>,
    },
    /// Leave every session of the wave still waiting for its turn as it is.
    Cancel {
        /// The wave id.
        wave: String,
    },
}
/// Where `switchboard launch` is run from, for the launch record.
#[derive(Clone, Copy, ValueEnum)]
enum LaunchSurface {
    Cli,
    Dashboards,
    /// Fabric's own Claude Code and Codex consoles (Fabric CO-584), recorded like a run its
    /// dispatcher starts (`switchboard dispatch`, SB-247).
    Fabric,
    Recovery,
}
impl LaunchSurface {
    /// The word `--surface` takes for it, as a launch on another machine passes it on.
    fn word(self) -> &'static str {
        match self {
            Self::Cli => "cli",
            Self::Dashboards => "dashboards",
            Self::Fabric => "fabric",
            Self::Recovery => "recovery",
        }
    }
}
impl From<LaunchSurface> for launches::Surface {
    fn from(s: LaunchSurface) -> Self {
        match s {
            LaunchSurface::Cli => Self::Cli,
            LaunchSurface::Dashboards => Self::Dashboards,
            LaunchSurface::Fabric => Self::Fabric,
            LaunchSurface::Recovery => Self::Recovery,
        }
    }
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
/// SB-403 (DEC-0025): answers one call another machine made through `ssh-serve` (see
/// `ssh_serve`): parsed from `SSH_ORIGINAL_COMMAND`, refused unless it is one a delegation needs,
/// and acting only on the seats the forced command's key delegated here.
#[cfg(unix)]
async fn serve_ssh(
    root: &std::path::Path,
    sender_key: &str,
    pool: &str,
    from: Option<&str>,
) -> Result<Value, String> {
    use switchboard_core::delegation;
    if !delegation::key_valid(sender_key) {
        return Err(delegation::KEY_INVALID.into());
    }
    if from.is_some_and(|f| !delegation::from_valid(f)) {
        return Err(delegation::FROM_INVALID.into());
    }
    let command = std::env::var("SSH_ORIGINAL_COMMAND").map_err(|_| ssh_serve::NOT_THROUGH_SSH)?;
    let request = ssh_serve::parse(&ssh_serve::words(&command)?, sender_key, pool, root)?;
    let key = sender_key.to_owned();
    // A call that names a seat acts only on one this key delegated into the group its line names.
    let owned = |snapshot: &Value, id: &str| {
        ssh_serve::seats_of(snapshot, sender_key, pool)
            .iter()
            .any(|s| s == id)
            .then_some(())
            .ok_or_else(|| delegation::NOT_DELEGATED_BY_YOU.to_string())
    };
    let check = |id: &str| {
        let id = id.to_owned();
        async move {
            let snapshot = call(root, Operation::Snapshot).await?;
            owned(&snapshot, &id)
        }
    };
    use ssh_serve::Request;
    match request {
        Request::Prove { nonce } => call(root, Operation::MachineProve { nonce }).await,
        Request::AccountsList => Ok(ssh_serve::accounts_view(
            &call(root, Operation::Snapshot).await?,
            sender_key,
            pool,
        )),
        Request::ProjectList => Ok(ssh_serve::projects_view(
            &call(root, Operation::Snapshot).await?,
            pool,
        )),
        Request::OrgList => {
            let snapshot = call(root, Operation::Snapshot).await?;
            Ok(ssh_serve::organizations_view(
                &call(root, Operation::Organizations).await?,
                &ssh_serve::group_organizations(&snapshot, pool),
            ))
        }
        Request::OrgAssign { id, contexts } => {
            let snapshot = call(root, Operation::Snapshot).await?;
            owned(&snapshot, &id)?;
            let organizations = call(root, Operation::Organizations).await?;
            if !ssh_serve::contexts_allowed(
                &contexts,
                &organizations,
                &ssh_serve::group_organizations(&snapshot, pool),
            ) {
                return Err(ssh_serve::ORGANIZATION_NOT_ALLOWED.into());
            }
            let (choice, organizations) = match contexts.as_slice() {
                [one] => ((one != "auto").then(|| one.clone()), None),
                several => (None, Some(several.to_vec())),
            };
            call(
                root,
                Operation::OrganizationAssign {
                    account: id,
                    choice,
                    organizations,
                    also: Vec::new(),
                    team: false,
                },
            )
            .await
        }
        Request::Add {
            provider,
            label,
            pool,
            lease_hours,
        } => {
            let provider = match provider.as_str() {
                "codex" => Provider::Codex,
                _ => Provider::Claude,
            };
            call(
                root,
                Operation::AddDelegated {
                    label,
                    provider,
                    pool,
                    secret: read_secret()?,
                    sender_key: key,
                    from: from.map(str::to_owned),
                    lease_hours: lease_hours.unwrap_or(delegation::DEFAULT_LEASE_HOURS),
                    served: true,
                },
            )
            .await
        }
        Request::Usage { id } => {
            check(&id).await?;
            call(root, Operation::Usage { id }).await
        }
        Request::Surrender { id, undo } => {
            check(&id).await?;
            call(
                root,
                Operation::SurrenderDelegated {
                    id,
                    sender_key: key,
                    undo,
                    served: true,
                },
            )
            .await
        }
        Request::Lease { id, hours } => {
            check(&id).await?;
            call(
                root,
                Operation::LeaseDelegated {
                    id,
                    sender_key: key,
                    hours,
                    served: true,
                },
            )
            .await
        }
        Request::Remove { id } => {
            check(&id).await?;
            call(
                root,
                Operation::RemoveDelegated {
                    id,
                    sender_key: key,
                    served: true,
                },
            )
            .await
        }
    }
}
#[cfg(not(unix))]
async fn serve_ssh(
    _: &std::path::Path,
    _: &str,
    _: &str,
    _: Option<&str>,
) -> Result<Value, String> {
    Err(ssh_serve::UNIX_ONLY.into())
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
    // W3 (SB-379): this command's own panics are counted for the error report, as the owner's.
    switchboard_runtime::errors::install_panic_counter_for_cli(&root);
    // SB-403: another machine's call through the restricted entry; nothing else is run.
    if let Command::SshServe {
        sender_key,
        pool,
        from,
    } = &cli.command
    {
        return serve_ssh(&root, sender_key, pool, from.as_deref()).await;
    }
    // SB-380: a session read, ended or attached on another machine runs that machine's
    // Switchboard over ssh; nothing is asked of this machine's owner.
    if let Some((target, remote, remote_data_dir, words, tty)) = there(&cli.command) {
        // A launch or session id is checked here before it travels (the same characters).
        let names_launch = words.first().is_some_and(|w| w == "attach")
            || words
                .get(1)
                .is_some_and(|w| w == "end" || w == "close-copy");
        if names_launch && !words.last().is_some_and(|id| launches::valid_id(id)) {
            return Err(launches::NO_SUCH_LAUNCH.into());
        }
        let (program, data_dir) =
            switchboard_runtime::targets::program_for(&root, &target, remote, remote_data_dir);
        let ssh_args = switchboard_runtime::targets::command_ssh(
            &target,
            &switchboard_runtime::account_send::Program {
                path: &program,
                data_dir: data_dir.as_deref(),
            },
            &words,
            cli.json && !tty,
            tty,
        )?;
        return exec_ssh(&ssh_args);
    }
    if let Command::Mcp { read_only } = cli.command {
        mcp::Server::new(root, read_only).serve().await?;
        return Ok(Value::Null);
    }
    // SB-299: needs no owner and no store.
    if let Command::Link = cli.command {
        return switchboard_runtime::agents::link_this_cli();
    }
    // SB-305: no owner and no store; it replaces the app this binary belongs to.
    if let Command::Update {
        check,
        app,
        feed,
        archive,
    } = &cli.command
    {
        let local = feed
            .clone()
            .zip(archive.clone())
            .map(|(feed, archive)| switchboard_runtime::self_update::Local { feed, archive });
        return switchboard_runtime::self_update::run(
            app.clone(),
            *check,
            env!("CARGO_PKG_VERSION"),
            local,
        )
        .await;
    }
    // SB-287: offline only. It opens the store itself, so it is refused while Switchboard runs.
    if let Command::Vault {
        command: VaultCommand::Use {
            kind,
            move_from_native,
        },
    } = cli.command
    {
        let kind = match kind {
            VaultKindArg::Native => switchboard_core::VaultKind::Native,
            VaultKindArg::File => switchboard_core::VaultKind::File,
        };
        return switchboard_runtime::vault_choice::choose(
            &root,
            kind,
            std::sync::Arc::new(switchboard_core::NativeVault::new()),
            switchboard_runtime::vault_choice::native_works,
            move_from_native,
        );
    }
    if let Command::Uninstall {
        keep_data,
        purge,
        yes,
    } = cli.command
    {
        return switchboard_runtime::uninstall::uninstall_with(
            &root,
            switchboard_core::open_vault(&root, false)?,
            keep_data,
            purge,
            yes,
            &switchboard_runtime::uninstall::places(&root),
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
        let data = root.clone();
        let owner = Owner::native(root).await?;
        let status = owner.runtime.execute(Operation::Status).await?;
        print_value(&json!({"state":"serving", "runtime":status}), cli.json);
        // SB-333: `serve` keeps itself current as the app does, and follows a program another
        // account replaced; a restart asks its supervisor to start it again on the new binary.
        let updates = async {
            use switchboard_runtime::serve_update as su;
            // SB-408: a .deb install never replaces its own files; it says once per newer version
            // that one is out (`update --check` prints the commands), and follows a program apt
            // replaced.
            let deb = switchboard_runtime::deb::this_install();
            let program = if deb {
                switchboard_runtime::copies::this_program()
            } else {
                su::program()
            };
            let Some(program) = program else {
                return std::future::pending::<&'static str>().await;
            };
            let Some(at_start) = su::stamp(&program) else {
                return std::future::pending::<&'static str>().await;
            };
            let mut told = su::Told::default();
            // A `serve` whose program was replaced exits only when a supervisor starts it again;
            // started by hand it says so once and keeps running on the old program.
            let supervised = su::supervised_now();
            if !deb {
                // Until its first pass, a window from this copy holds back (SB-414).
                su::note_started();
            }
            tokio::time::sleep(su::FIRST_AFTER).await;
            loop {
                let version = env!("CARGO_PKG_VERSION");
                let pass = if deb {
                    su::deb_pass(&program, &at_start, version, supervised, &mut told).await
                } else {
                    su::pass(&data, &program, &at_start, version, supervised, &mut told).await
                };
                if let su::Pass::Restart(why) = pass {
                    // A managed session between two requests is still working: wait for a quiet
                    // spell (drain anyway after half an hour, as the app does).
                    let runtime = owner.runtime.clone();
                    su::wait_quiet(
                        move || runtime.busy_reason(),
                        su::QUIET_LOOK,
                        su::QUIET_WAIT,
                    )
                    .await;
                    return why;
                }
                tokio::time::sleep(su::EVERY).await;
            }
        };
        let restart = tokio::select! {
            signal = signals.requested() => {
                let signal = signal?;
                arm_hard_exit(HARD_EXIT_AFTER);
                oplog::event("stop_requested", &[("signal", oplog::Field::Code(signal))]);
                None
            }
            why = updates => {
                arm_hard_exit(HARD_EXIT_AFTER);
                Some(why)
            }
        };
        let stopped = owner.shutdown(DRAIN_DEADLINE).await;
        if let Some(why) = restart {
            print_value(
                &json!({"state":"restarting", "reason": why, "drained": stopped.drained}),
                cli.json,
            );
            std::process::exit(switchboard_runtime::serve_update::RESTART_STATUS);
        }
        return Ok(json!({"state":"stopped", "drained": stopped.drained}));
    }
    // SB-404 K2: the saves run as separate switchboard processes that read the value on stdin,
    // so this process holds no store while they run.
    if let Command::Keys {
        command:
            KeysCommand::Observatory {
                command:
                    Some(ObservatoryCommand::Add {
                        slot,
                        to,
                        name,
                        pool,
                        model,
                        replace,
                    }),
            },
    } = &cli.command
    {
        let cli_path = std::env::current_exe()
            .and_then(|p| p.canonicalize())
            .map_err(|_| "The switchboard program could not be found.".to_string())?;
        return switchboard_runtime::observatory_keys::add(
            &root,
            &cli_path,
            switchboard_runtime::observatory_keys::AddRequest {
                slot: slot.clone(),
                to: to.clone(),
                name: name.clone(),
                pool: pool.clone(),
                model: model.clone(),
                replace: *replace,
            },
        )
        .await;
    }
    // `continue --ceiling`: the task's ceiling first, so a refusal (out of range, higher than
    // the one set) stops the continue before anything is offered.
    if let Command::Continue {
        workflow_id,
        ceiling: Some(usd),
        ..
    } = &cli.command
    {
        call(
            &root,
            Operation::SetCeiling {
                scope: switchboard_core::ChainScope::Workflow {
                    id: workflow_id.clone(),
                },
                usd: Some(*usd),
                by: switchboard_runtime::spend::Surface::Cli,
            },
        )
        .await?;
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
            Accounts::Activate { id, force } => Operation::ActivateNative {
                id: id.clone(),
                force: *force,
            },
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
                location,
                force,
            } => Operation::Update {
                id: id.clone(),
                label: label.clone(),
                enabled: *enabled,
                elsewhere: *elsewhere,
                location: location.clone(),
                force: *force,
            },
            Accounts::Send {
                id,
                to,
                pool,
                remote,
                remote_data_dir,
                delegate,
                lease_hours,
            } => Operation::SendAccount {
                id: id.clone(),
                to: to.clone(),
                pool: pool.clone(),
                program: remote.clone(),
                remote_data_dir: remote_data_dir.clone(),
                decision: None,
                delegate: *delegate,
                lease_hours: *lease_hours,
            },
            Accounts::Recall {
                id,
                from,
                pool,
                remote_id,
                remote,
                remote_data_dir,
            } => Operation::RecallAccount {
                id: id.clone(),
                from: from.clone(),
                pool: pool.clone(),
                remote_id: remote_id.clone(),
                program: remote.clone(),
                remote_data_dir: remote_data_dir.clone(),
                decision: None,
            },
            Accounts::Seats { limit } => Operation::Seats {
                limit: Some(*limit),
            },
            Accounts::Remote {
                on,
                remote,
                remote_data_dir,
            } => Operation::RemoteStatus {
                target: on.clone(),
                program: remote.clone(),
                remote_data_dir: remote_data_dir.clone(),
            },
            Accounts::Check {
                on,
                remote,
                remote_data_dir,
            } => Operation::RemoteCheck {
                target: on.clone(),
                program: remote.clone(),
                remote_data_dir: remote_data_dir.clone(),
            },
            Accounts::Surrender { id, undo } => {
                if !undo && io::stdout().is_terminal() {
                    return Err(switchboard_runtime::account_send::SURRENDER_TO_TERMINAL.into());
                }
                Operation::SurrenderAccount {
                    id: id.clone(),
                    undo: *undo,
                }
            }
            Accounts::HandOver { id, to } => Operation::HandOver {
                id: id.clone(),
                to: to.clone(),
                decision: None,
            },
            Accounts::Receive {
                file,
                signer,
                apply,
                headless,
            } => {
                let text = std::fs::read(file).map_err(|_| "The hand-over file cannot be read.")?;
                let record: Value = serde_json::from_slice(&text)
                    .map_err(|_| switchboard_runtime::account_handover::NOT_A_HANDOVER)?;
                Operation::ReceiveHandover {
                    record,
                    signer: signer.clone(),
                    apply: *apply,
                    headless: *headless,
                }
            }
            Accounts::Move { id, pool } => Operation::MoveAccount {
                id: id.clone(),
                pool: pool.clone(),
            },
            Accounts::Remove { id } => Operation::Remove { id: id.clone() },
            Accounts::Select {
                id,
                provider,
                pool,
                force,
            } => Operation::Select {
                id: id.clone(),
                provider: (*provider).into(),
                pool: pool.clone(),
                force: *force,
            },
            Accounts::Assign { id, to, force } => Operation::Assign {
                id: id.clone(),
                to: Some(to.clone()),
                force: *force,
            },
            Accounts::Unassign { id } => Operation::Assign {
                id: id.clone(),
                to: None,
                force: false,
            },
        },
        Command::Usage { id: Some(id) } => Operation::Usage { id: id.clone() },
        Command::Usage { id: None } | Command::Events { .. } => Operation::Snapshot,
        Command::Status => Operation::Status,
        Command::Targets { probe } => Operation::Targets { probe: *probe },
        Command::McpHomes => Operation::McpHomes,
        Command::Attention => Operation::Attention,
        Command::Bugs => Operation::Bugs,
        Command::Health => Operation::Health,
        Command::Capacity { command } => match command {
            None => Operation::Capacity {
                cap: None,
                clear: false,
                board: None,
                board_default: false,
            },
            Some(CapacityCmd::Set { cap, board }) => Operation::Capacity {
                cap: Some(*cap),
                clear: false,
                board: match board {
                    Some(BoardShare::Runs(n)) => Some(*n),
                    _ => None,
                },
                board_default: matches!(board, Some(BoardShare::Default)),
            },
            Some(CapacityCmd::Clear) => Operation::Capacity {
                cap: None,
                clear: true,
                board: None,
                board_default: false,
            },
        },
        Command::OwnSignIn { command } => Operation::OwnSignIn {
            on: command.as_ref().map(|c| matches!(c, OwnSignInCmd::On)),
        },
        Command::Proxy { command } => {
            // Paths are made absolute here: the owner runs in another folder.
            let absolute = |path: &PathBuf| -> Result<String, String> {
                let path = if path.is_absolute() {
                    path.clone()
                } else {
                    std::env::current_dir()
                        .map_err(|_| "Current folder unavailable. Give an absolute path.")?
                        .join(path)
                };
                Ok(path.display().to_string())
            };
            let users =
                |action: &str,
                 user: Option<&String>,
                 group: Option<&String>,
                 folders: Vec<String>,
                 write_settings: Option<String>| Operation::ProxyUsers {
                    action: Some(action.into()),
                    user: user.cloned(),
                    group: group.cloned(),
                    folders,
                    write_settings,
                    by: Some("cli".into()),
                };
            match command {
                ProxyCmd::Users => users("list", None, None, Vec::new(), None),
                ProxyCmd::AllowUser {
                    user,
                    group,
                    folders,
                    write_settings,
                } => users(
                    "allow",
                    Some(user),
                    Some(group),
                    folders.iter().map(absolute).collect::<Result<_, _>>()?,
                    write_settings.as_ref().map(absolute).transpose()?,
                ),
                ProxyCmd::DisallowUser { user } => {
                    users("disallow", Some(user), None, Vec::new(), None)
                }
                ProxyCmd::RotateUser {
                    user,
                    write_settings,
                } => users(
                    "rotate",
                    Some(user),
                    None,
                    Vec::new(),
                    write_settings.as_ref().map(absolute).transpose()?,
                ),
            }
        }
        Command::SshServe { .. } => return Err(ssh_serve::NOT_THROUGH_SSH.into()),
        Command::Duty { command } => {
            fn duty(
                on: Option<bool>,
                limits: [Option<u32>; 4],
                run: bool,
                dry_run: bool,
            ) -> Operation {
                let [stuck_minutes, waiting_minutes, idle_minutes, max_age_hours] = limits;
                Operation::Duty {
                    on,
                    stuck_minutes,
                    waiting_minutes,
                    idle_minutes,
                    max_age_hours,
                    run,
                    dry_run,
                }
            }
            match command {
                None => duty(None, [None; 4], false, false),
                Some(DutyCmd::On) => duty(Some(true), [None; 4], false, false),
                Some(DutyCmd::Off) => duty(Some(false), [None; 4], false, false),
                Some(DutyCmd::Set {
                    stuck_minutes,
                    waiting_minutes,
                    idle_minutes,
                    max_age_hours,
                }) => duty(
                    None,
                    [
                        *stuck_minutes,
                        *waiting_minutes,
                        *idle_minutes,
                        *max_age_hours,
                    ],
                    false,
                    false,
                ),
                Some(DutyCmd::Run { dry_run }) => duty(None, [None; 4], !*dry_run, *dry_run),
            }
        }
        Command::Monitor {
            command: MonitorCmd::Hook,
        } => return Ok(monitor_hook(&root)),
        Command::Monitor {
            command:
                MonitorCmd::Hold {
                    session,
                    until,
                    reason,
                },
        } => {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs() as i64);
            return switchboard_runtime::monitors::hold(
                &root,
                session,
                *until,
                now,
                reason.as_deref(),
            );
        }
        Command::Machines { command } => match command {
            MachinesCmd::List { live } => Operation::Machines { live: *live },
            MachinesCmd::Adopt { apply } => Operation::MachinesAdopt { apply: *apply },
            MachinesCmd::Place { id, on } => Operation::MachinePlace {
                id: id.clone(),
                on: on.clone(),
            },
            MachinesCmd::Forget { machine } => Operation::MachineForget {
                machine: machine.clone(),
            },
            MachinesCmd::Trust { machine, key } => Operation::MachineTrust {
                machine: machine.clone(),
                key: key.clone(),
            },
            MachinesCmd::Keys => Operation::MachineKeys,
            MachinesCmd::Prove { nonce } => Operation::MachineProve {
                nonce: nonce.clone(),
            },
        },
        Command::Stats { days } => Operation::Statistics { days: Some(*days) },
        Command::Settings { command } => match command {
            SettingsCmd::Export { sign, groups } => Operation::SettingsExport {
                sign: *sign,
                groups: groups.clone(),
            },
            SettingsCmd::Signer => Operation::SettingsSigner,
            SettingsCmd::Sync {
                to,
                groups,
                apply,
                remote,
                remote_data_dir,
            } => Operation::SettingsSync {
                target: to.clone(),
                groups: groups.clone(),
                apply: *apply,
                program: remote.clone(),
                remote_data_dir: remote_data_dir.clone(),
            },
            SettingsCmd::Import {
                file,
                under,
                apply,
                clone,
                signer,
            } => {
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
                if *clone && !*apply {
                    return Err("Clone with --apply: a preview clones nothing.".into());
                }
                Operation::SettingsImport {
                    bundle: value,
                    under: under.clone(),
                    apply: *apply,
                    clone: *clone,
                    signer: signer.clone(),
                }
            }
        },
        Command::Inbox { command } => match command {
            Inbox::Set { origin, client_id } => Operation::InboxSet {
                origin: origin.clone(),
                client_id: client_id.clone(),
                secret: read_secret()?,
            },
            Inbox::Status { check } => Operation::InboxStatus { check: *check },
            Inbox::Remove => Operation::InboxRemove,
        },
        Command::App { command } => {
            use switchboard_runtime::app_doors::AppAction;
            let (action, state) = match command {
                AppCmd::Updates { state } => (AppAction::Updates, *state),
                AppCmd::CheckUpdates => (AppAction::CheckForUpdates, None),
                AppCmd::RestartToUpdate => (AppAction::RestartToUpdate, None),
                AppCmd::LoginItem { state } => (AppAction::LoginItem, *state),
            };
            Operation::App {
                action,
                enabled: state.map(|s| matches!(s, OnOff::On)),
            }
        }
        Command::Analytics { state } => Operation::Analytics {
            enabled: state.map(|s| matches!(s, OnOff::On)),
        },
        Command::Github { command } => match command {
            Github::Owners => Operation::GithubOwners,
            Github::Repos { owner } => Operation::GithubRepositories {
                owner: owner.clone(),
            },
            Github::Clone { owner, name, into } => Operation::GithubClone {
                owner: owner.clone(),
                name: name.clone(),
                into: folder(&Some(into.clone()))?,
            },
        },
        Command::Org { command } => match command {
            Org::List => Operation::Organizations,
            Org::Connect => Operation::VestaConnect,
            Org::Cancel => Operation::VestaCancel,
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
                also,
                team,
            } => match organization.as_slice() {
                [] => Operation::OrganizationAssign {
                    account: account.clone(),
                    choice: None,
                    organizations: None,
                    also: also.clone(),
                    team: *team,
                },
                [one] => Operation::OrganizationAssign {
                    account: account.clone(),
                    choice: (one != "auto").then(|| one.clone()),
                    organizations: None,
                    also: Vec::new(),
                    team: *team,
                },
                several => Operation::OrganizationAssign {
                    account: account.clone(),
                    choice: None,
                    organizations: Some(several.to_vec()),
                    also: Vec::new(),
                    team: *team,
                },
            },
        },
        Command::Dispatch {
            command: Some(DispatchCommand::Stop { launch_id, if_gone }),
            ..
        } => Operation::DispatchStop {
            launch_id: launch_id.clone(),
            if_gone: *if_gone,
        },
        Command::Dispatch {
            command: Some(DispatchCommand::Status { launch_id }),
            ..
        } => Operation::DispatchStatus {
            launch_id: launch_id.clone(),
        },
        Command::Dispatch {
            command: None,
            dir,
            workflow,
            handoff,
            request,
            dispatch_id,
            reason,
            permissions,
            timeout_minutes,
            target,
            remote,
            remote_data_dir,
            candidates,
        } => switchboard_runtime::launched(
            Operation::Dispatch {
                working_directory: dir.clone().unwrap_or_default(),
                workflow_id: workflow.clone().unwrap_or_default(),
                handoff_id: handoff.clone().unwrap_or_default(),
                reason: match reason {
                    DispatchReason::Request => switchboard_runtime::dispatch::Reason::Request,
                    DispatchReason::Inbox => switchboard_runtime::dispatch::Reason::Inbox,
                    DispatchReason::Resume => switchboard_runtime::dispatch::Reason::Resume,
                },
                permissions: match permissions {
                    DispatchPermissions::Ask => switchboard_runtime::launch::Permissions::Ask,
                    DispatchPermissions::All => switchboard_runtime::launch::Permissions::All,
                },
                timeout_minutes: *timeout_minutes,
                request_id: request.clone(),
                dispatch_id: dispatch_id.clone(),
                target: target.clone(),
                remote: remote.clone(),
                remote_data_dir: remote_data_dir.clone(),
                candidates: candidates.clone(),
            },
            launches::LaunchedBy::from_caller(launches::Surface::Fabric, None, None),
        ),
        #[cfg(unix)]
        Command::Sessions {
            command: Some(SessionsCommand::End { launch_id, .. }),
            ..
        } => {
            return switchboard_runtime::session_host::end(&root, launch_id);
        }
        #[cfg(not(unix))]
        Command::Sessions {
            command: Some(SessionsCommand::End { .. }),
            ..
        } => {
            return Err("A session host runs on macOS and Linux for now.".into());
        }
        Command::Sessions {
            command:
                Some(SessionsCommand::CloseCopy {
                    session_id, pid, ..
                }),
            ..
        } => Operation::SessionCloseCopy {
            session_id: session_id.clone(),
            pid: *pid,
        },
        Command::Sessions {
            command: Some(SessionsCommand::Continue { in_place: true, .. }),
            target: Some(_),
            ..
        } => {
            // A session continued in this terminal runs on this computer (R4 review).
            return Err(
                "Continue in place runs on this computer. Run it there, without --target.".into(),
            );
        }
        Command::Sessions {
            command:
                Some(SessionsCommand::Continue {
                    session_id,
                    in_place: true,
                    hosted,
                    surface,
                }),
            target: None,
            ..
        } => {
            return continue_in_place(&root, session_id, *hosted, *surface).await;
        }
        Command::Sessions {
            command: Some(SessionsCommand::Continue { session_id, .. }),
            ..
        } => Operation::SessionContinue {
            session_id: session_id.clone(),
            in_place: false,
            hosted: false,
        },
        Command::Sessions {
            command: Some(SessionsCommand::Restart { command }),
            ..
        } => match command {
            RestartCmd::Plan => Operation::SessionsRestartPlan {
                caller_pid: Some(std::process::id()),
            },
            RestartCmd::Apply { digest, reason } => Operation::SessionsRestartApply {
                digest: digest.clone(),
                reason: reason.clone(),
                caller_pid: Some(std::process::id()),
            },
            RestartCmd::Status { wave } => Operation::SessionsRestartStatus { wave: wave.clone() },
            RestartCmd::Cancel { wave } => Operation::SessionsRestartCancel { wave: wave.clone() },
        },
        Command::Sessions { rows: true, .. } => Operation::SessionRows,
        Command::Sessions { tree: true, .. } => {
            return Ok(switchboard_runtime::session_watch::tree(
                &root,
                switchboard_runtime::session_watch::now(),
            ));
        }
        Command::Sessions { machine: true, .. } => {
            return Ok(switchboard_runtime::machine_sessions::scan(&root));
        }
        Command::Sessions {
            command: Some(SessionsCommand::Show { launch_id }),
            ..
        } => Operation::Launches {
            launch_id: Some(launch_id.clone()),
            limit: None,
            before: None,
        },
        Command::Sessions {
            history: true,
            limit,
            before,
            ..
        } => Operation::Launches {
            launch_id: None,
            limit: limit.map(usize::from),
            before: *before,
        },
        Command::Sessions { all, .. } => Operation::Sessions { all: *all },
        Command::Provenance {
            provider,
            pool,
            launch_id,
        } => Operation::Provenance {
            provider: provider.map(Into::into),
            pool: pool.clone(),
            launch_id: launch_id.clone(),
        },
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
                // SB-310: a managed policy nobody saved is on by default.
                enabled: target == "managed",
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
                headless,
            } => Operation::BeginLogin {
                provider: (*provider).into(),
                label: label.clone(),
                pool: pool.clone(),
                headless: *headless,
            },
            Login::Code { login_id } => Operation::SubmitLoginCode {
                login_id: login_id.clone(),
                code: read_secret()?,
            },
            Login::Pending => Operation::PendingLogins,
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
            headless,
            hosted,
            by,
            parent,
            surface,
            args,
            target: Some(target),
            remote,
            remote_data_dir,
            ..
        } => {
            return launch_there(
                &root,
                target,
                remote.clone(),
                remote_data_dir.clone(),
                LaunchThere {
                    id: id.clone(),
                    provider: *provider,
                    mode: *mode,
                    working_directory: working_directory.clone(),
                    in_place: *in_place,
                    headless: *headless,
                    hosted: *hosted,
                    by: by.clone(),
                    parent: parent.clone(),
                    surface: *surface,
                    args: args.clone(),
                },
            )
            .await;
        }
        Command::Launch {
            id,
            provider,
            mode,
            working_directory,
            in_place,
            headless,
            hosted,
            by,
            parent,
            surface,
            args,
            target: None,
            ..
        } => {
            // SB-212: checked before anything is asked (the owner checks them again); a clap
            // parser's refusal would print only "Invalid arguments".
            if by.as_deref().is_some_and(|b| !launches::valid_name(b)) {
                return Err(launches::BY_INVALID.into());
            }
            if parent.as_deref().is_some_and(|p| !launches::valid_id(p)) {
                return Err(launches::PARENT_INVALID.into());
            }
            let surface = launches::Surface::from(*surface);
            let by = launches::LaunchedBy::from_caller(surface, by.clone(), parent.clone());
            return launch(
                &root,
                id,
                *provider,
                *mode,
                working_directory,
                if *headless {
                    Here::Headless
                } else if *hosted {
                    Here::Hosted
                } else if *in_place {
                    Here::Terminal
                } else {
                    Here::No
                },
                args,
                by,
            )
            .await;
        }
        #[cfg(unix)]
        Command::Attach { launch_id, .. } => {
            return finish_attach(launch_id, hosted::attach(&root, launch_id)?);
        }
        #[cfg(not(unix))]
        Command::Attach { .. } => {
            return Err("A session host runs on macOS and Linux for now.".into());
        }
        #[cfg(unix)]
        Command::SessionHost {
            launch_id,
            program,
            rows,
            cols,
            idle_minutes,
        } => {
            let ended = hosted::run(&root, launch_id, program, *rows, *cols, *idle_minutes)?;
            // Recorded by the owner when it runs (the records' writer), else here.
            call(
                &root,
                Operation::SessionEnded {
                    launch_id: launch_id.clone(),
                    exit_code: ended.exit_code,
                    reason: Some(ended.reason.to_owned()),
                },
            )
            .await?;
            return Ok(Value::Null);
        }
        #[cfg(unix)]
        Command::SessionOpen { launch_id, program } => {
            // No host could start: the session still runs, in this window, as before H-1b.
            if hosted::spawn(&root, launch_id, program).is_err() {
                return run_in_place(program);
            }
            return finish_attach(launch_id, hosted::attach(&root, launch_id)?);
        }
        #[cfg(not(unix))]
        Command::SessionOpen { .. } => {
            return Err("A session host runs on macOS and Linux for now.".into());
        }
        #[cfg(not(unix))]
        Command::SessionHost { .. } => {
            return Err("A session host runs on macOS and Linux for now.".into());
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
            Some(HandoversCommand::Run { id, ask }) => return run_handover(&root, id, *ask).await,
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
        Command::Ceiling { command } => match command {
            CeilingCommand::List => Operation::Ceilings,
            CeilingCommand::Set { scope, usd } => Operation::SetCeiling {
                scope: chain_scope(scope)?,
                usd: Some(*usd),
                by: switchboard_runtime::spend::Surface::Cli,
            },
            CeilingCommand::Keep { id } => Operation::AnswerCeiling {
                id: id.clone(),
                answer: switchboard_runtime::spend::Answer::Keep,
                by: switchboard_runtime::spend::Surface::Cli,
            },
            CeilingCommand::Double { id } => Operation::AnswerCeiling {
                id: id.clone(),
                answer: switchboard_runtime::spend::Answer::Double,
                by: switchboard_runtime::spend::Surface::Cli,
            },
            CeilingCommand::Clear { scope } => Operation::SetCeiling {
                scope: chain_scope(scope)?,
                usd: None,
                by: switchboard_runtime::spend::Surface::Cli,
            },
        },
        Command::Balance { command } => match command {
            BalanceCommand::Status => Operation::BalanceStatus,
            BalanceCommand::Groups {
                add,
                remove,
                all_new,
                backup_add,
                backup_remove,
            } => Operation::Balance {
                groups_add: add.clone(),
                groups_remove: remove.clone(),
                all_new: *all_new,
                backup_add: backup_add.clone(),
                backup_remove: backup_remove.clone(),
            },
        },
        Command::Feed { command } => match command {
            FeedCommand::Status => Operation::FeedStatus,
            FeedCommand::Set { ready, lookahead } => {
                if ready.is_none() && lookahead.is_none() {
                    return Err("Give --ready, --lookahead or both.".into());
                }
                Operation::Feed {
                    pool: None,
                    on: None,
                    priority: None,
                    ready: *ready,
                    lookahead_minutes: *lookahead,
                }
            }
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
            AgentsCommand::Permissions { mode } => {
                Operation::AgentPermissions { mode: mode.clone() }
            }
            AgentsCommand::Models => Operation::OpenrouterModels,
            AgentsCommand::Connect { agent, pool } => Operation::AgentConnect {
                agent: agent.clone(),
                pool: pool.clone(),
            },
            AgentsCommand::Key { service: None, .. } => Operation::AgentKey,
            AgentsCommand::Key {
                service: Some(KeyService::Openrouter),
                dir,
                pool,
                machine,
            } => {
                use switchboard_runtime::spend::KeySlot;
                // SB-213: a slot named is the key fetched, whatever group the folder is in now.
                let slot = match (pool, machine) {
                    (Some(pool), _) => Some(KeySlot::Group(pool.clone())),
                    (None, true) => Some(KeySlot::Machine),
                    (None, false) => None,
                };
                Operation::AgentKeyValue {
                    service: "openrouter".into(),
                    folder: match (dir, &slot) {
                        (_, Some(_)) => None,
                        (Some(d), None) if d.is_absolute() => Some(d.clone()),
                        (Some(_), None) => return Err("--dir must be an absolute folder.".into()),
                        (None, None) => std::env::current_dir().ok(),
                    },
                    slot,
                }
            }
            AgentsCommand::Openrouter { command } => match command {
                OpenrouterCommand::Set { model, pool, .. } => Operation::AgentKeySet {
                    service: "openrouter".into(),
                    key: read_secret()?,
                    model: model.clone(),
                    pool: pool.clone(),
                    by: switchboard_runtime::spend::Surface::Cli,
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
                    by: switchboard_runtime::spend::Surface::Cli,
                },
                OpenrouterCommand::List => Operation::AgentKeys {
                    service: "openrouter".into(),
                },
            },
            AgentsCommand::LaunchKeys { agent, folder } => Operation::LaunchKeys {
                agent: agent.clone(),
                folder: Some({
                    let here = std::env::current_dir()
                        .map_err(|_| "The current folder could not be read.".to_string())?;
                    match folder {
                        Some(f) => here.join(f),
                        None => here,
                    }
                }),
            },
            AgentsCommand::Anthropic { command } => match command {
                AnthropicCommand::List => Operation::AnthropicKeys,
                AnthropicCommand::Set { name, model, .. } => Operation::AnthropicKeySet {
                    name: name.clone(),
                    key: read_secret()?,
                    model: model.clone(),
                },
                AnthropicCommand::Model { name, model } => Operation::AnthropicKeyModel {
                    name: name.clone(),
                    model: model.clone(),
                },
                AnthropicCommand::Remove { name } => {
                    Operation::AnthropicKeyRemove { name: name.clone() }
                }
                AnthropicCommand::Key { name } => {
                    Operation::AnthropicKeyValue { name: name.clone() }
                }
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
                anthropic,
                model,
            } => Operation::LaunchAgent {
                agent: agent.clone(),
                pool: pool.clone(),
                working_directory: folder(dir)?,
                via: if anthropic.is_some() {
                    AgentVia::Anthropic
                } else if *openrouter {
                    AgentVia::Openrouter
                } else {
                    AgentVia::Proxy
                },
                model: model.clone(),
                key: anthropic.clone(),
            },
        },
        Command::Keys {
            command: KeysCommand::Observatory { command },
        } => match command {
            None | Some(ObservatoryCommand::List) => Operation::ObservatoryKeys,
            Some(ObservatoryCommand::Add { .. }) => unreachable!(),
        },
        Command::Kimi { command } => match command {
            KimiCommand::List => Operation::KimiAccounts,
            KimiCommand::Login { label, region } => Operation::KimiLoginBegin {
                label: label.clone(),
                region: region.clone(),
            },
            KimiCommand::Finish { id } => Operation::KimiLoginFinish { id: id.clone() },
            KimiCommand::Status { id } => Operation::KimiLoginStatus { id: id.clone() },
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
                organizations,
                move_accounts,
            } => Operation::SaveProject {
                organizations: (!organizations.is_empty()).then(|| organizations.clone()),
                move_accounts: *move_accounts,
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
            Project::Switch { pool, state } => Operation::GroupSwitch {
                pool: pool.clone(),
                on: matches!(state, OnOff::On),
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
                move_accounts,
            } => Operation::ProjectEdit {
                organizations: (organization.len() > 1).then(|| organization.clone()),
                move_accounts: *move_accounts,
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
                organization: (organization.len() == 1).then(|| organization[0].clone()),
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
            Project::Feed {
                pool,
                state,
                priority,
            } => Operation::Feed {
                pool: Some(pool.clone()),
                on: Some(matches!(state, OnOff::On)),
                priority: *priority,
                ready: None,
                lookahead_minutes: None,
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
        | Command::Vault { .. }
        | Command::Link
        | Command::Update { .. }
        | Command::Onboard { .. } => unreachable!(),
    };
    // SB-299: the operator's words open a loosening; the runtime checks them and records them.
    let operation = match (&cli.operator_said, &cli.said_at, &cli.said_in) {
        (None, None, None) => operation,
        (Some(instruction), Some(at), Some(source)) => Operation::OnOperatorsWord {
            decision: switchboard_runtime::spend::Decision {
                by: "operator".into(),
                instruction: instruction.clone(),
                at: at.clone(),
                source: source.clone(),
            },
            inner: Box::new(operation),
        },
        _ => return Err(switchboard_runtime::spend::DECISION_INVALID.into()),
    };
    // SB-212: a launch names who asked for it (`agents launch`, `kimi launch`, `continue
    // --to-console`).
    let operation = switchboard_runtime::launched(
        operation,
        launches::LaunchedBy::from_caller(launches::Surface::Cli, None, None),
    );
    let value = call(&root, operation).await?;
    match &cli.command {
        // SB-312: each account says whether it is used on another machine and where, as MCP does.
        Command::Accounts { command: Accounts::List } => {
            let organizations = call(&root, Operation::Organizations).await.ok();
            let accounts: Vec<Value> = value["accounts"].as_array().cloned().unwrap_or_default().into_iter().map(|mut a| {
                let id = a["id"].as_str().unwrap_or_default().to_owned();
                a["elsewhere"] = json!(value["elsewhere"].get(&id).is_some());
                a["location"] = value["locations"].get(&id).cloned().unwrap_or(Value::Null);
                // SB-322: the agent it was given to, null when none.
                a["assigned_to"] = value["assigned_to"].get(&id).cloned().unwrap_or(Value::Null);
                // SB-403: a seat another machine delegated here, with its lease.
                if let Some(d) = value["delegated"].get(&id) {
                    a["delegated"] = d.clone();
                }
                // SB-328: its organizations, as `switchboard_accounts` gives them.
                let view = mcp::account_organizations(&value, organizations.as_ref(), &id);
                a["organization"] = view.0;
                a["also_organizations"] = view.1;
                a
            }).collect();
            Ok(json!({"accounts": accounts, "routes": value["routes"]}))
        }
        Command::Events {
            kind,
            account,
            before,
            limit,
        } => {
            // SB-373: the page is cut from the owner's journal here, as MCP cuts it; printed
            // oldest first, as before.
            let page = switchboard_core::activity::page(
                &value["events"],
                &switchboard_core::activity::Filter {
                    kind: kind.clone(),
                    account_id: account.clone(),
                    before: *before,
                    limit: Some(limit.map_or(switchboard_core::activity::MAX_LIMIT, |n| n as usize)),
                },
            )?;
            if let (false, Some(older)) = (cli.json, page["next_before"].as_i64()) {
                eprintln!("Older activity: switchboard events --before {older}");
            }
            let mut events = page["events"].as_array().cloned().unwrap_or_default();
            events.reverse();
            Ok(Value::Array(events))
        }
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
/// Where `launch` runs the session: in a Terminal the owner opens, in this terminal
/// (`--in-place`), or here with no terminal (`--headless`, SB-317).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Here {
    No,
    Terminal,
    Headless,
    /// SB-342 H-1: in this terminal, through a session host of its own.
    Hosted,
}
/// SB-417 (review rounds 1 and 2): a hosted start whose host answers no longer needs its
/// reservation (its launch counts now); a board start started ends the wait of the board starts
/// refused before.
#[cfg_attr(not(unix), allow(dead_code))]
fn recorded(
    root: &std::path::Path,
    reservation: Option<switchboard_runtime::capacity::Reservation>,
) {
    if let Some(reservation) = reservation {
        if reservation.start == switchboard_runtime::capacity::Start::Board {
            switchboard_runtime::dispatch_refusals::admitted(
                root,
                switchboard_runtime::session_watch::now(),
            );
        }
    }
}
/// SB-417: a hosted start Fabric asks for (`--surface fabric`) is the board's, any other one
/// interactive.
fn start_class(surface: launches::Surface) -> switchboard_runtime::capacity::Start {
    if surface == launches::Surface::Fabric {
        switchboard_runtime::capacity::Start::Board
    } else {
        switchboard_runtime::capacity::Start::Interactive
    }
}
#[allow(clippy::too_many_arguments)]
async fn launch(
    root: &std::path::Path,
    id: &Option<String>,
    provider: Option<ProviderArg>,
    mode: Mode,
    working_directory: &std::path::Path,
    here: Here,
    args: &[String],
    by: launches::LaunchedBy,
) -> Result<Value, String> {
    use std::io::IsTerminal;
    let in_place = here != Here::No;
    if !in_place && !args.is_empty() {
        return Err("Agent arguments after -- go with --in-place or --headless.".into());
    }
    if matches!(here, Here::Terminal | Here::Hosted)
        && !(std::io::stdin().is_terminal() && std::io::stdout().is_terminal())
    {
        return Err(
            "Launching in place needs a terminal on stdin and stdout. Use --headless with no terminal."
                .into(),
        );
    }
    // SB-378: a hosted session (Fabric's and Dashboards' consoles) waits while this computer's
    // session pool is full; the same gate as `dispatch`. SB-417: one Fabric opens for the board
    // (`--surface fabric`) starts inside the board's share.
    // Review round 1, P1-B: the reservation counts this start until its launch is recorded.
    let reservation = if here == Here::Hosted {
        switchboard_runtime::capacity::admit(root, start_class(by.surface))?
    } else {
        None
    };
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
        switchboard_runtime::launched(
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
            by,
        ),
    )
    .await?;
    if !in_place {
        return Ok(value);
    }
    let script = value["script"]
        .as_str()
        .ok_or("The session script is missing.")?;
    if here == Here::Headless {
        return run_headless(std::path::Path::new(script));
    }
    if here == Here::Hosted {
        let launch_id = value["launch_id"]
            .as_str()
            .ok_or("The session's launch id is missing.")?;
        return run_hosted(root, launch_id, std::path::Path::new(script), reservation);
    }
    run_in_place(std::path::Path::new(script))
}
/// SB-400: continues a session as itself in this terminal: its resume script run here, or in a
/// session host of its own launch (`--hosted`) that this command attaches to. Refused as
/// `sessions continue` refuses (a process holds it, its history is gone).
async fn continue_in_place(
    root: &std::path::Path,
    session_id: &str,
    hosted: bool,
    surface: Option<LaunchSurface>,
) -> Result<Value, String> {
    if !(std::io::stdin().is_terminal() && std::io::stdout().is_terminal()) {
        return Err("Continuing in place needs a terminal on stdin and stdout.".into());
    }
    let surface = launches::Surface::from(surface.unwrap_or(LaunchSurface::Cli));
    // SB-378: a hosted session waits while this computer's session pool is full, as `launch`.
    let reservation = if hosted {
        switchboard_runtime::capacity::admit(root, start_class(surface))?
    } else {
        None
    };
    let by = launches::LaunchedBy::from_caller(surface, None, None);
    let value = call(
        root,
        switchboard_runtime::launched(
            Operation::SessionContinue {
                session_id: session_id.to_owned(),
                in_place: true,
                hosted,
            },
            by,
        ),
    )
    .await?;
    let script = value["script"]
        .as_str()
        .ok_or("The session script is missing.")?;
    if let Some(launch_id) = value["launch_id"].as_str() {
        let ran = run_hosted(root, launch_id, std::path::Path::new(script), reservation);
        // Only a host that never came up ends the record here; one that runs (attaching failed)
        // ends it itself when its session ends (R4, R5 review).
        #[cfg(unix)]
        let host_up = switchboard_runtime::session_host::attachable(root, launch_id).is_some();
        #[cfg(not(unix))]
        let host_up = false;
        if ran.is_err() && !host_up {
            let _ = call(
                root,
                Operation::SessionEnded {
                    launch_id: launch_id.to_owned(),
                    exit_code: 1,
                    reason: Some("not_started".into()),
                },
            )
            .await;
        }
        return ran;
    }
    run_in_place(std::path::Path::new(script))
}
/// SB-342 H-1: starts the session's host and shows the session here; this command ends with
/// the session's exit status, or 0 when the person detaches.
#[cfg(unix)]
fn run_hosted(
    root: &std::path::Path,
    launch_id: &str,
    script: &std::path::Path,
    reservation: Option<switchboard_runtime::capacity::Reservation>,
) -> Result<Value, String> {
    if let Err(error) = hosted::spawn(root, launch_id, script) {
        release_reservation(script);
        return Err(error);
    }
    // SB-417 (review round 2): the host answers, so its launch counts in the pool now; only then
    // is the reservation let go and the waiting board starts told one has started.
    recorded(root, reservation);
    finish_attach(launch_id, hosted::attach(root, launch_id)?)
}
#[cfg(unix)]
fn finish_attach(launch_id: &str, ended: hosted::Ended) -> Result<Value, String> {
    match ended {
        hosted::Ended::Exited(code) => std::process::exit(code),
        hosted::Ended::Detached => Ok(hosted::answer(launch_id, &ended)),
    }
}
#[cfg(not(unix))]
fn run_hosted(
    _: &std::path::Path,
    _: &str,
    _: &std::path::Path,
    _: Option<switchboard_runtime::capacity::Reservation>,
) -> Result<Value, String> {
    Err("A session host runs on macOS and Linux for now.".into())
}
/// SB-380: the command to run on another machine for `sessions`, `sessions end` and `attach`
/// given `--target`: the machine, its program and data folder, the words, and whether it needs a
/// terminal. A launch id is checked here, before it reaches the other machine.
type There = (String, Option<String>, Option<String>, Vec<String>, bool);
fn there(command: &Command) -> Option<There> {
    match command {
        Command::Sessions {
            target: Some(target),
            remote,
            remote_data_dir,
            all,
            history,
            limit,
            before,
            machine,
            tree,
            rows,
            command: None,
        } => {
            let mut words = vec!["sessions".to_owned()];
            for (on, flag) in [
                (*all, "--all"),
                (*history, "--history"),
                (*machine, "--machine"),
                (*tree, "--tree"),
                (*rows, "--rows"),
            ] {
                if on {
                    words.push(flag.to_owned());
                }
            }
            if let Some(n) = limit {
                words.extend(["--limit".to_owned(), n.to_string()]);
            }
            if let Some(t) = before {
                words.extend(["--before".to_owned(), t.to_string()]);
            }
            Some((
                target.clone(),
                remote.clone(),
                remote_data_dir.clone(),
                words,
                false,
            ))
        }
        Command::Sessions {
            command:
                Some(SessionsCommand::End {
                    launch_id,
                    target: Some(target),
                    remote,
                    remote_data_dir,
                }),
            ..
        } => Some((
            target.clone(),
            remote.clone(),
            remote_data_dir.clone(),
            vec!["sessions".into(), "end".into(), launch_id.clone()],
            false,
        )),
        // Dashboards' FD-247 (board `35e1a402`): a duplicate on another machine is closed there.
        Command::Sessions {
            command:
                Some(SessionsCommand::CloseCopy {
                    session_id,
                    pid,
                    target: Some(target),
                    remote,
                    remote_data_dir,
                }),
            ..
        } => {
            let mut words = vec!["sessions".into(), "close-copy".into(), session_id.clone()];
            if let Some(pid) = pid {
                words.extend(["--pid".to_owned(), pid.to_string()]);
            }
            Some((
                target.clone(),
                remote.clone(),
                remote_data_dir.clone(),
                words,
                false,
            ))
        }
        Command::Attach {
            launch_id,
            target: Some(target),
            remote,
            remote_data_dir,
        } => Some((
            target.clone(),
            remote.clone(),
            remote_data_dir.clone(),
            vec!["attach".into(), launch_id.clone()],
            true,
        )),
        _ => None,
    }
}
/// SB-332: what `launch --target` passes on to the other machine's `switchboard launch`.
struct LaunchThere {
    id: Option<String>,
    provider: Option<ProviderArg>,
    mode: Mode,
    working_directory: std::path::PathBuf,
    in_place: bool,
    headless: bool,
    /// SB-342 H-1d: the session runs in a session host there, so it outlives this ssh link.
    hosted: bool,
    by: Option<String>,
    parent: Option<String>,
    surface: LaunchSurface,
    args: Vec<String>,
}
/// SB-332: replaces this process with `ssh` running the other machine's `switchboard launch`;
/// with --in-place the session is in this terminal (`ssh -t`), with --headless its output and
/// exit status are this command's (`ssh -T`). Nothing is asked of this machine's owner.
async fn launch_there(
    root: &std::path::Path,
    target: &str,
    remote: Option<String>,
    remote_data_dir: Option<String>,
    l: LaunchThere,
) -> Result<Value, String> {
    use std::io::IsTerminal;
    if !l.in_place && !l.headless {
        return Err("A launch on another machine runs with --in-place or --headless.".into());
    }
    if l.in_place && !(std::io::stdin().is_terminal() && std::io::stdout().is_terminal()) {
        return Err(
            "Launching in place needs a terminal on stdin and stdout. Use --headless with no terminal."
                .into(),
        );
    }
    let mut words: Vec<String> = Vec::new();
    match (&l.id, l.provider) {
        (Some(id), None) => words.push(id.clone()),
        (None, Some(p)) => words.extend([
            "--provider".to_owned(),
            match p {
                ProviderArg::Claude => "claude",
                ProviderArg::Codex => "codex",
            }
            .to_owned(),
        ]),
        _ => return Err("Give an account id or --provider.".into()),
    }
    let folder = l
        .working_directory
        .to_str()
        .ok_or("Give the folder as text.")?
        .to_owned();
    words.extend([
        "--mode".to_owned(),
        match l.mode {
            Mode::Isolated => "isolated",
            Mode::Managed => "managed",
        }
        .to_owned(),
        "--working-directory".to_owned(),
        folder,
        if l.headless {
            "--headless"
        } else {
            "--in-place"
        }
        .to_owned(),
    ]);
    if l.hosted {
        words.push("--hosted".to_owned());
    }
    words.extend(["--surface".to_owned(), l.surface.word().to_owned()]);
    if let Some(by) = &l.by {
        words.extend(["--by".to_owned(), by.clone()]);
    }
    if let Some(parent) = &l.parent {
        words.extend(["--parent".to_owned(), parent.clone()]);
    }
    if !l.args.is_empty() {
        words.push("--".to_owned());
        words.extend(l.args.iter().cloned());
    }
    let (program, data_dir) =
        switchboard_runtime::targets::program_for(root, target, remote, remote_data_dir);
    let ssh_args = switchboard_runtime::targets::launch_ssh(
        target,
        &switchboard_runtime::account_send::Program {
            path: &program,
            data_dir: data_dir.as_deref(),
        },
        &words,
        l.in_place,
    )?;
    exec_ssh(&ssh_args)
}
#[cfg(unix)]
fn exec_ssh(args: &[String]) -> Result<Value, String> {
    use std::os::unix::process::CommandExt;
    let error = std::process::Command::new("ssh").args(args).exec();
    let _ = error;
    Err("ssh could not start.".into())
}
#[cfg(not(unix))]
fn exec_ssh(args: &[String]) -> Result<Value, String> {
    let status = std::process::Command::new("ssh")
        .args(args)
        .status()
        .map_err(|_| "ssh could not start.".to_string())?;
    std::process::exit(status.code().unwrap_or(1));
}
/// `switchboard handovers run <id>`: takes the hand-over (once) and runs its session here.
/// With `ask` (`--ask`) its session starts with the agent's permission prompts on.
async fn run_handover(root: &std::path::Path, id: &str, ask: bool) -> Result<Value, String> {
    use std::io::IsTerminal;
    // Checked before it is taken: a hand-over taken by a caller with no terminal would be lost.
    if !(std::io::stdin().is_terminal() && std::io::stdout().is_terminal()) {
        return Err("Running a hand-over needs a terminal on stdin and stdout.".into());
    }
    let value = call(
        root,
        Operation::HandoverTake {
            id: id.to_owned(),
            ask,
        },
    )
    .await?;
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
        switchboard_runtime::launched(
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
            launches::LaunchedBy::from_caller(launches::Surface::Cli, None, None),
        ),
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
/// Runs the prepared session script in this terminal and waits for it: its exit status is the
/// command's, and its launch record gets `ended` (before 0.7.96 this process replaced itself with
/// the script, nobody saw the end, and every in-place launch read `gone`; studio-agent's finding
/// `b18dd31c`). The agent keeps the terminal: Ctrl-C, Ctrl-\ and a closed window reach it, not
/// this process, and a SIGTERM sent to this process (a console stopping its session) is passed
/// on to it. A script that names no launch replaces this process as before.
#[cfg(unix)]
fn run_in_place(script: &std::path::Path) -> Result<Value, String> {
    run_and_record(script, false)
}
/// The session script as this process's child, waited for, its end recorded. In place it keeps
/// the terminal (Ctrl-C, Ctrl-\ and a closed window reach it, a SIGTERM to this process is passed
/// on); headless (SB-389) it reads nothing on stdin, writes to this process's stdout and stderr,
/// and every stop a scheduler sends this process (SIGTERM, SIGINT, SIGHUP, SIGQUIT) is passed on.
#[cfg(unix)]
fn run_and_record(script: &std::path::Path, headless: bool) -> Result<Value, String> {
    use std::os::unix::process::CommandExt;
    let refused = if headless {
        "The session could not start."
    } else {
        "The session could not start in this terminal."
    };
    let Some((root, launch_id)) = switchboard_runtime::launch::script_launch(script) else {
        let mut command = std::process::Command::new(script);
        if headless {
            command.stdin(std::process::Stdio::null());
        }
        let error = command.exec();
        let _ = error;
        release_reservation(script);
        return Err(refused.into());
    };
    static CHILD: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);
    extern "C" fn pass_on(signal: libc::c_int) {
        let child = CHILD.load(std::sync::atomic::Ordering::Relaxed);
        if child > 0 {
            // SAFETY: kill is async-signal-safe; the pid is our own child's.
            unsafe { libc::kill(child, signal) };
        }
    }
    // SAFETY: plain signal dispositions of this process; the child restores the defaults before
    // it runs the script.
    unsafe {
        let terminal_stop = if headless {
            pass_on as *const () as libc::sighandler_t
        } else {
            libc::SIG_IGN
        };
        libc::signal(libc::SIGINT, terminal_stop);
        libc::signal(libc::SIGQUIT, terminal_stop);
        libc::signal(libc::SIGHUP, terminal_stop);
        libc::signal(libc::SIGTERM, pass_on as *const () as libc::sighandler_t);
    }
    let mut command = std::process::Command::new(script);
    if headless {
        command.stdin(std::process::Stdio::null());
    }
    // SAFETY: only async-signal-safe calls between fork and exec.
    unsafe {
        command.pre_exec(|| {
            for signal in [libc::SIGINT, libc::SIGQUIT, libc::SIGHUP, libc::SIGTERM] {
                libc::signal(signal, libc::SIG_DFL);
            }
            Ok(())
        });
    }
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(_) => {
            release_reservation(script);
            return Err(refused.into());
        }
    };
    CHILD.store(child.id() as i32, std::sync::atomic::Ordering::Relaxed);
    let code = match child.wait() {
        Ok(status) => {
            use std::os::unix::process::ExitStatusExt;
            status
                .code()
                .unwrap_or_else(|| 128 + status.signal().unwrap_or(0))
        }
        Err(_) => 1,
    };
    switchboard_runtime::launches::record_exit(
        &root,
        &launch_id,
        code,
        switchboard_runtime::session_watch::now(),
    );
    // Nothing printed: the terminal may be gone, and the agent's own output was the answer.
    std::process::exit(code)
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
/// SB-317: the session script with nothing on stdin: a scheduler's job gets the agent's output
/// on its own stdout and stderr and the agent's exit status, and a stop it sends reaches the
/// agent itself. SB-389: run as a child and waited for, so its launch record gets `ended`
/// (before, this process replaced itself with the script and every scheduled launch read `gone`).
#[cfg(unix)]
fn run_headless(script: &std::path::Path) -> Result<Value, String> {
    run_and_record(script, true)
}
#[cfg(not(unix))]
fn run_headless(_: &std::path::Path) -> Result<Value, String> {
    Err("Launching headless runs on macOS and Linux for now.".into())
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
        // SB-202: the organization checklist's reads, only where an organization exists. The
        // `hook` step reads `shell status` (SB-183's record), never the start files.
        let passioncode = switchboard_runtime::vesta::status(root).ok();
        probe.passioncode = passioncode.as_ref().is_some_and(|v| v["connected"] == true);
        // F1 of the wave 4 design pass (SCN-079): what the person left there is not set up again.
        if probe.passioncode {
            probe.released = passioncode
                .as_ref()
                .and_then(|v| v["released"].as_array())
                .into_iter()
                .flatten()
                .filter_map(|r| r["organization"].as_str().map(str::to_owned))
                .collect();
        }
        // Read leniently: the owner answering may be another version (its snapshot may carry
        // keys this CLI does not know). What could not be read is named, never dropped.
        let (saved, partial) = onboarding::read_snapshot(&snapshot);
        if partial {
            probe.degraded.push("snapshot".into());
        }
        if saved.as_ref().is_some_and(|s| !s.organizations.is_empty()) {
            // A failed read leaves its list empty and is named in the step it touches.
            match call(root, Operation::MonitorStatus).await {
                Ok(monitor) => {
                    let list = |key: &str| -> Vec<String> {
                        monitor[key]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter_map(|id| id.as_str().map(str::to_owned))
                            .collect()
                    };
                    probe.sign_in_required = list("sign_in_required");
                    probe.in_cli = list("in_cli");
                }
                Err(_) => probe.degraded.push("sign_ins".into()),
            }
            match call(root, Operation::Sessions { all: true }).await {
                Ok(sessions) => {
                    probe.contexts = sessions["sessions"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|s| s["context"].as_str().map(str::to_owned))
                        .collect()
                }
                Err(_) => probe.degraded.push("sessions".into()),
            }
            probe.shell = Some(shell::status(Some(root), Some(&folder)));
        }
        probe.snapshot = saved;
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
    // SB-212: a session Switchboard launched records its provider session and transcript, beside
    // the folder's suggestion and within the same deadline.
    let (_, value) = tokio::join!(
        launch_event(root, parsed.as_ref(), launches::Event::Start),
        suggest_from_hook(root, parsed.as_ref()),
    );
    value
}
/// SB-212: what a session's hook reports about its own launch, when Switchboard launched it
/// (`SWITCHBOARD_LAUNCH_ID` in its environment). Without that variable, or with an input that
/// is not a hook's, nothing is asked. Never an error: the answer is not read.
async fn launch_event(root: &std::path::Path, hook: Option<&Value>, event: launches::Event) {
    let Some(launch_id) = std::env::var(launches::ENV)
        .ok()
        .filter(|id| launches::valid_id(id))
    else {
        return;
    };
    let Some(hook) = hook.filter(|h| h.is_object()) else {
        return;
    };
    // A start names its session or its transcript; another object asks nothing (N-5).
    if event == launches::Event::Start
        && !hook["session_id"].is_string()
        && !hook["transcript_path"].is_string()
    {
        return;
    }
    // SB-218: a start of the launch's own process under a new session (`/clear`, a compaction, a
    // resume) says so; a nested session's start does not, and never replaces its parent's.
    let source = (event == launches::Event::Start)
        .then(|| hook["source"].as_str())
        .flatten()
        .filter(|s| matches!(*s, "clear" | "compact" | "resume"))
        .map(str::to_owned);
    let operation = |source: Option<String>| Operation::LaunchEvent {
        launch_id: launch_id.clone(),
        event,
        provider_session_id: hook["session_id"].as_str().map(str::to_owned),
        transcript: hook["transcript_path"].as_str().map(PathBuf::from),
        source,
    };
    let _ = tokio::time::timeout(SESSION_HOOK_DEADLINE, async {
        let sent = source.is_some();
        match call(root, operation(source)).await {
            // An owner from before `source` (the update window) records it as a start.
            Err(e) if sent && e == switchboard_runtime::control::CONTROL_OWNER_OLDER => {
                call(root, operation(None)).await
            }
            answer => answer,
        }
    })
    .await;
}
async fn suggest_from_hook(root: &std::path::Path, parsed: Option<&Value>) -> Value {
    let Some(cwd) = parsed
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
/// `switchboard monitor hook` (SB-396 wave C): a session's own wait, read from the hook input.
/// Prints `{}`: a `PreToolUse` hook's output is read as its decision, and this one decides nothing.
fn monitor_hook(root: &std::path::Path) -> Value {
    use std::io::Read;
    let mut input = Vec::new();
    let read = std::io::stdin().take(1024 * 1024).read_to_end(&mut input);
    if let Some(hook) = read
        .ok()
        .and_then(|_| serde_json::from_slice::<Value>(&input).ok())
    {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64);
        switchboard_runtime::monitors::hook(root, &hook, now);
    }
    json!({})
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
    // One byte past the limit tells an input that was cut from one of exactly the limit.
    let read = std::io::stdin()
        .take(HOOK_INPUT_LIMIT + 1)
        .read_to_end(&mut input);
    // SB-210: an input past the limit cannot be parsed, and a `Stop` hook's stdout is read as
    // its decision: such an input answers `{}`, no decision, as a `Stop` does. Claude Code's
    // `StopFailure` payload is a few hundred bytes, so none is that long.
    if input.len() as u64 > HOOK_INPUT_LIMIT {
        return json!({});
    }
    let parsed: Option<Value> = read.ok().and_then(|_| serde_json::from_slice(&input).ok());
    let Some(hook) = parsed else {
        return json!({"registered": false, "reason": "malformed_input"});
    };
    // SB-204: the same hook command runs on `Stop`, after every finished turn. It settles the
    // session's stop, asks the owner only when the session has one, and prints `{}`: a Stop
    // hook's stdout is read by Claude Code as its decision.
    if hook["hook_event_name"].as_str() == Some("Stop") {
        // SB-220: the launch record's turn and the recovery stop are asked side by side, each
        // bounded like the start hook (SB-149), so the hook takes at most one deadline, not two.
        let settle = async {
            let Some(session) = hook["session_id"].as_str() else {
                return;
            };
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs() as i64);
            // A busy or absent owner never holds a turn.
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
        };
        // SB-212: the launch record counts the turn.
        tokio::join!(
            launch_event(root, Some(&hook), launches::Event::Turn),
            settle
        );
        return json!({});
    }
    let billing = match hook["error"].as_str() {
        Some("rate_limit") => false,
        Some("billing_error") => true,
        _ => return json!({"registered": false, "reason": "not_a_limit"}),
    };
    // SB-220: the stop's registration does not wait behind the record's update; both run side
    // by side. SB-212: the launch record says where the session stopped.
    let register = async {
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
            // SB-397 W0.4: an owner draining or starting holds the store; the stop is kept for
            // the next owner's pass, which registers it (or refuses it once more and forgets it).
            // An owner that answered with a refusal is answered as one, never kept.
            Err(error) if switchboard_runtime::owner_unreachable(&error) => {
                let kept = switchboard_runtime::stop_spool::spool(
                    root,
                    &switchboard_runtime::stop_spool::Spooled {
                        session: session.to_owned(),
                        directory: std::path::PathBuf::from(cwd),
                        billing,
                        at: std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map_or(0, |d| d.as_secs() as i64),
                    },
                );
                if kept.is_ok() {
                    json!({"registered": false, "spooled": true})
                } else {
                    json!({"registered": false, "reason": "refused", "message": error})
                }
            }
            Err(error) => json!({"registered": false, "reason": "refused", "message": error}),
        }
    };
    let ((), answer) = tokio::join!(
        launch_event(root, Some(&hook), launches::Event::Limit),
        register
    );
    answer
}
/// One launch on one line (SB-212): its id, state and agent, who launched it, where and when.
fn launch_line(l: &Value) -> String {
    let text = |v: &Value| v.as_str().unwrap_or(PLACEHOLDER).to_string();
    let by = &l["launched_by"];
    let mut who = text(&by["surface"]);
    if let Some(name) = by["by"].as_str() {
        who.push(':');
        who.push_str(name);
    }
    let mut line = format!(
        "{} {:<9} {:<16} {:<14} {} · {}",
        text(&l["launch_id"]),
        text(&l["state"]),
        text(&l["agent"]),
        who,
        text(&l["working_directory"]),
        utc(&l["launched_at"]),
    );
    // SB-247: a dispatched run's end is an object, `{kind, exit_code?, at}`.
    if let Some(ended) = l["ended"].as_str().or_else(|| l["ended"]["kind"].as_str()) {
        line.push_str(&format!(" · stopped: {ended}"));
    }
    if let Some(kind) = l["continued_as"]["kind"].as_str() {
        line.push_str(&format!(
            " · went to {kind} {}",
            text(&l["continued_as"]["id"])
        ));
    }
    line
}
/// `switchboard sessions show`: the launch, then its chain up and down.
fn print_launch(value: &Value) {
    let launch = &value["launch"];
    println!("{}", launch_line(launch));
    let text = |v: &Value| v.as_str().unwrap_or(PLACEHOLDER).to_string();
    for (key, name) in [
        ("label", "account"),
        ("pool", "group"),
        ("provider_session_id", "provider session"),
        ("transcript", "transcript"),
        ("turns", "turns"),
        ("handover", "hand-over"),
    ] {
        let field = &launch[key];
        if !field.is_null() {
            let shown = field
                .as_str()
                .map_or_else(|| field.to_string(), str::to_owned);
            println!("  {name}: {shown}");
        }
    }
    let by = &launch["launched_by"];
    for (key, name) in [
        ("caller_project", "caller's group"),
        ("caller_program", "caller's program"),
    ] {
        if let Some(v) = by[key].as_str() {
            println!("  {name}: {v}");
        }
    }
    if launch["last_turn_at"].is_i64() {
        println!("  last turn: {}", utc(&launch["last_turn_at"]));
    }
    for (key, heading) in [("up", "Launched by"), ("down", "Then")] {
        let rows = value[key].as_array().cloned().unwrap_or_default();
        if rows.is_empty() {
            continue;
        }
        println!("{heading}:");
        for r in &rows {
            if r["missing"] == true {
                println!(
                    "  {} (record no longer kept) via {}",
                    text(&r["launch_id"]),
                    text(&r["via"])
                );
            } else {
                println!("  {} via {}", launch_line(r), text(&r["via"]));
            }
        }
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
        Command::Dispatch { command: None, .. } => match value["state"].as_str() {
            Some("refused") => println!("{} ({})", text(&value["message"]), text(&value["reason"])),
            _ => println!(
                "{} {} on {}: {}",
                text(&value["state"]),
                text(&value["launch_id"]),
                text(&value["agent"]),
                text(&value["account_id"])
            ),
        },
        Command::Dispatch {
            command: Some(DispatchCommand::Stop { .. }),
            ..
        } => println!("{} {}", text(&value["launch_id"]), text(&value["state"])),
        Command::Dispatch {
            command: Some(DispatchCommand::Status { .. }),
            ..
        } => {
            println!("{} {}", text(&value["launch_id"]), text(&value["state"]));
            if let Some(result) = value["result"].as_str() {
                println!("{result}");
            }
        }
        Command::Org { command: Org::List } => {
            let organizations = value["organizations"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            if organizations.is_empty() {
                println!("No organization. Add one: switchboard org add NAME (--domain example.com is optional)");
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
                    Some("person") => "set by you".to_owned(),
                    Some("provider") => "its team".to_owned(),
                    _ => String::new(),
                };
                // SB-285: the domain only suggests; the person confirms with `org assign`.
                let note = match a["suggested_name"].as_str() {
                    Some(name) if by.is_empty() => format!("its address suggests {name}"),
                    Some(name) => format!("{by}; its address suggests {name}"),
                    None => by,
                };
                // SB-403: the organizations it also works for, after its own.
                let mut contexts = vec![a["organization_name"]
                    .as_str()
                    .unwrap_or("personal")
                    .to_owned()];
                for id in a["also"].as_array().cloned().unwrap_or_default() {
                    if let Some(o) = organizations.iter().find(|o| o["id"] == id) {
                        contexts.push(text(&o["name"]));
                    }
                }
                println!(
                    "{}  {:7}  {:24}  {}",
                    text(&a["id"]),
                    text(&a["provider"]),
                    contexts.join(" + "),
                    if note.is_empty() {
                        text(&a["label"])
                    } else {
                        format!("{} ({note})", text(&a["label"]))
                    }
                );
            }
        }
        Command::Sessions {
            command: Some(SessionsCommand::Show { .. }),
            ..
        } => print_launch(value),
        Command::Sessions { tree: true, .. } => {
            let c = &value["counts"];
            println!(
                "Live: {} ({} working, {} quiet, {} started by Switchboard).",
                c["live"], c["working"], c["quiet"], c["managed"]
            );
            for f in value["findings"].as_array().cloned().unwrap_or_default() {
                println!(
                    "! {} {}",
                    text(&f["kind"]),
                    f["launch_id"]
                        .as_str()
                        .or(f["session_id"].as_str())
                        .unwrap_or("")
                );
            }
            for s in value["live"].as_array().cloned().unwrap_or_default() {
                println!(
                    "{:<7} {:<11} {:<10} {:<12} {}",
                    s["pid"].to_string(),
                    text(&s["agent"]),
                    s["surface"].as_str().unwrap_or("unmanaged"),
                    s["pool"].as_str().unwrap_or("-"),
                    text(&s["folder"])
                );
            }
        }
        Command::Sessions { machine: true, .. } => {
            let rows = value["sessions"].as_array().cloned().unwrap_or_default();
            for s in &rows {
                let quiet = match s["quiet_minutes"].as_i64() {
                    Some(m) => format!("quiet {m} min"),
                    None => "no transcript".into(),
                };
                println!(
                    "{:<7} {:<11} {:<9} {} · {}",
                    s["pid"].to_string(),
                    text(&s["agent"]),
                    if s["managed"] == true {
                        "managed"
                    } else {
                        "unmanaged"
                    },
                    text(&s["folder"]),
                    quiet
                );
            }
            println!(
                "{} sessions: {} managed, {} unmanaged, {} quiet for {} minutes or more.",
                value["total"],
                value["managed"],
                value["unmanaged"],
                value["quiet"],
                value["quiet_after_minutes"]
            );
        }
        Command::Sessions { history: true, .. } => {
            let launches = value["launches"].as_array().cloned().unwrap_or_default();
            if launches.is_empty() {
                println!("No launch is recorded yet.");
            }
            for l in &launches {
                println!("{}", launch_line(l));
            }
            let total = value["total"].as_u64().unwrap_or(0);
            if total as usize > launches.len() {
                println!("{} of {total} launches, newest first.", launches.len());
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
                // SB-189: the sentence the window shows for the same failure.
                let error = switchboard_runtime::person_errors::for_person(error);
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
        Command::Events { .. } => {
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
                    .map(|a| {
                        if a["shared"] == true {
                            format!(
                                "{} ({}, shared from {})",
                                text(&a["label"]),
                                text(&a["provider"]),
                                text(&a["pool"])
                            )
                        } else {
                            format!("{} ({})", text(&a["label"]), text(&a["provider"]))
                        }
                    })
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
                AgentsCommand::Anthropic {
                    command: AnthropicCommand::Key { .. },
                },
        } => println!("{}", text(&value["key"])),
        Command::Agents {
            command: AgentsCommand::LaunchKeys { .. },
        } => {
            let keys = value["keys"].as_array().cloned().unwrap_or_default();
            if keys.is_empty() {
                println!("No key is saved that this agent can start on.");
            }
            for key in keys {
                let kind = match key["kind"].as_str() {
                    Some("anthropic") => "Anthropic (no ceiling)",
                    Some("group") => "Pay-per-token group",
                    _ => "OpenRouter",
                };
                let state = if key["default"] == true {
                    "used by a launch here".to_string()
                } else {
                    key["why_not"]
                        .as_str()
                        .unwrap_or("can be chosen")
                        .to_string()
                };
                println!(
                    "{kind} · {} · model {} · {state}",
                    text(&key["name"]),
                    key["model"].as_str().unwrap_or("not set")
                );
            }
        }
        Command::Keys {
            command: KeysCommand::Observatory { command },
        } => match command {
            None | Some(ObservatoryCommand::List) => print_observatory_keys(value),
            Some(ObservatoryCommand::Add { .. }) => {
                for saved in value["saved"].as_array().into_iter().flatten() {
                    let answer = &saved["answer"];
                    match saved["target"].as_str() {
                        Some("account") => println!(
                            "Saved as the account {} in the pay-per-token group.",
                            text(&answer["label"])
                        ),
                        Some("anthropic") => {
                            println!("Saved as the Anthropic key {}.", text(&answer["name"]))
                        }
                        _ => println!("Saved as an OpenRouter key."),
                    }
                }
                if let Some(failed) = value["failed"].as_str() {
                    println!("Not saved further: {failed}");
                }
                for line in value["next"].as_array().into_iter().flatten() {
                    println!("{}", text(line));
                }
            }
        },
        Command::Agents {
            command:
                AgentsCommand::Anthropic {
                    command: AnthropicCommand::List,
                },
        } => {
            let keys = value["keys"].as_array().cloned().unwrap_or_default();
            if keys.is_empty() {
                println!("No Anthropic key is saved.");
            }
            for key in keys {
                println!(
                    "{} · model {}",
                    text(&key["name"]),
                    key["model"].as_str().unwrap_or("not set")
                );
            }
        }
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
                    let amount = |v: &Value| v.as_f64().map(money);
                    println!(
                        "Remaining {} · spent today {} · limit {}",
                        amount(&credit["limit_remaining"]).unwrap_or_else(|| "no limit".into()),
                        amount(&credit["usage_daily"]).unwrap_or_else(|| "unknown".into()),
                        match (amount(&credit["limit"]), credit["limit_reset"].as_str()) {
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
        Command::Ceiling {
            command: CeilingCommand::List,
        } => print_ceilings(value),
        Command::Ceiling {
            command: CeilingCommand::Set { .. },
        } => println!(
            "Ceiling for {}: {}.",
            scope_label(&value["ceiling"]["scope"]),
            money(dollars(&value["ceiling"]))
        ),
        Command::Ceiling {
            command: CeilingCommand::Keep { .. },
        } => println!(
            "Kept: the ceiling for {} stays {}.",
            scope_label(&value["ceiling"]["scope"]),
            money(dollars(&value["ceiling"]))
        ),
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
        Command::Health => {
            println!("{} · {}", text(&value["state"]), text(&value["version"]));
            for check in value["checks"].as_array().cloned().unwrap_or_default() {
                let fix = check["fix"]["tool"]
                    .as_str()
                    .map(|t| format!(" · fix: {t}"))
                    .unwrap_or_default();
                println!(
                    "{} ({}): {} · {}{fix}",
                    text(&check["id"]),
                    text(&check["area"]),
                    text(&check["state"]),
                    check["count"].as_u64().unwrap_or(0)
                );
            }
        }
        Command::Bugs => {
            let crashes = value["crashes"].as_array().cloned().unwrap_or_default();
            if crashes.is_empty() {
                println!("No crash found.");
            }
            for crash in &crashes {
                println!(
                    "Crash of {} started at {} (ran {}): {} {}",
                    text(&crash["version"]),
                    text(&crash["started_at"]),
                    text(&crash["uptime"]),
                    crash["exception"].as_str().unwrap_or("no crash report"),
                    crash["signal"].as_str().unwrap_or("")
                );
            }
            if value["counting"].as_bool() != Some(true) {
                println!("Error counts are kept only while usage counts are on.");
            }
            for error in value["errors"].as_array().cloned().unwrap_or_default() {
                println!(
                    "Day {}: {} {} x{}",
                    text(&error["day"]),
                    text(&error["class"]),
                    text(&error["code"]),
                    text(&error["count"])
                );
            }
        }
        Command::Attention => {
            let items = value["items"].as_array().cloned().unwrap_or_default();
            if items.is_empty() {
                println!("Nothing needs attention.");
            }
            for item in &items {
                let who = match (item["label"].as_str(), item["pool"].as_str()) {
                    (Some(label), Some(pool)) => format!("{label} ({pool})"),
                    (None, Some(pool)) => format!("Group {pool}"),
                    (Some(label), None) => label.to_owned(),
                    (None, None) => "Ceiling".to_owned(),
                };
                println!(
                    "{who}: {} Fix: {}",
                    text(&item["detail"]),
                    text(&item["action"]["tool"])
                );
            }
        }
        Command::Status => println!(
            "Mode: {}\nPlatform: {}\nProxy: {}",
            text(&value["live_mode"]),
            text(&value["platform"]),
            text(&value["proxy_address"])
        ),
        Command::Capacity { .. } => print_capacity(value),
        Command::Proxy { .. } => print_proxy_users(value),
        _ => print_value(value, false),
    }
}
/// `switchboard proxy ...` as text: who is served and how the settings file was written; never
/// a capability, which no answer carries.
fn print_proxy_users(value: &Value) {
    let text = |v: &Value| v.as_str().unwrap_or_default().to_owned();
    let line = |user: &Value| {
        let name = user["name"].as_str().map_or_else(
            || format!("uid {}", user["uid"]),
            |n| format!("{n} (uid {})", user["uid"]),
        );
        let group = user["group_name"]
            .as_str()
            .or(user["group"].as_str())
            .unwrap_or_default();
        let folders = user["folders"]
            .as_array()
            .map(|f| {
                f.iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default();
        if folders.is_empty() {
            format!("{name}: group {group}")
        } else {
            format!("{name}: group {group}, folders {folders}")
        }
    };
    if let Some(users) = value["users"].as_array() {
        if users.is_empty() {
            println!("The proxy serves no other users of this computer.");
        }
        for user in users {
            println!("{}", line(user));
        }
        return;
    }
    if value["removed"].is_object() {
        println!("Disallowed: {}.", line(&value["removed"]));
        return;
    }
    if value["rotated"] == true {
        println!(
            "New capability for {}. The old one is refused.",
            line(&value["user"])
        );
    } else if value["user"].is_object() {
        println!("Allowed: {}.", line(&value["user"]));
    }
    let file = &value["settings_file"];
    if let Some(path) = file["path"].as_str() {
        println!("Settings file: {path} (mode {}).", text(&file["mode"]));
        if let Some(command) = file["install_command"].as_str() {
            println!("You may not give it to the user's group. Hand it over with:\n  {command}");
            println!(
                "Then run claude there with --settings {}.",
                text(&file["installed_path"])
            );
        } else {
            println!("Run claude as that user with --settings {path}.");
        }
    }
    if let Some(error) = value["settings_error"].as_str() {
        println!("The settings file was not written again: {error}");
    }
}
/// `switchboard capacity` as text.
fn print_capacity(value: &Value) {
    let not_counted = value["quiet"].as_u64().unwrap_or(0)
        + value["on_limit_stop"].as_u64().unwrap_or(0)
        + value["on_waits"].as_u64().unwrap_or(0);
    match value["cap"].as_u64() {
        Some(cap) => {
            println!(
                "Sessions: {} of {cap} working, {} free ({not_counted} quiet or waiting not counted).",
                value["used"], value["free"]
            );
            println!(
                "Board runs: {} of {} running in the board's share.",
                value["board_used"], value["board_share"]
            );
        }
        None => println!(
            "No session limit: {} working sessions ({not_counted} quiet or waiting not counted).",
            value["used"]
        ),
    }
    if value["board_share_clamped"] == true {
        println!(
            "The board's share was larger than the new limit, so it is now {}.",
            value["board_share"]
        );
    }
    // SB-417: board runs refused at the limit that still wait, however long ago.
    let waiting = &value["refused_waiting"];
    if let Some(count) = waiting["count"].as_u64() {
        println!(
            "Board runs waiting at the limit: {count}, the first refused at {}{}.",
            utc(&waiting["since"]),
            if waiting["freed"] == true {
                "; a session has ended since"
            } else {
                ""
            }
        );
    }
}
/// A ceiling's dollars: `usd` in a view, `cents` in a stored ceiling.
fn dollars(ceiling: &Value) -> f64 {
    ceiling["usd"]
        .as_f64()
        .or_else(|| ceiling["cents"].as_f64().map(|c| c / 100.0))
        .unwrap_or_default()
}
/// `machine`, `project:<pool>` or `workflow:<wf_id>`, as `--scope` takes it.
fn scope_label(scope: &Value) -> String {
    match scope["kind"].as_str() {
        Some("project") => format!("project:{}", scope["pool"].as_str().unwrap_or_default()),
        Some("workflow") => format!("workflow:{}", scope["id"].as_str().unwrap_or_default()),
        _ => "machine".to_owned(),
    }
}
fn print_ceilings(value: &Value) {
    let ceilings = value["ceilings"].as_array().cloned().unwrap_or_default();
    if ceilings.is_empty() {
        println!(
            "No spending ceiling is set. Paid agents are bound only by their key's own limit."
        );
    }
    for c in &ceilings {
        let usd = c["usd"].as_f64().unwrap_or_default();
        let measure = if c["scope"]["kind"] == "workflow" {
            "since the task's first paid hand-over"
        } else {
            "a UTC day"
        };
        let inactive = if c["active"] == false {
            " (inactive: this group has no key of its own now)"
        } else {
            ""
        };
        println!(
            "{:28} {} {measure}{inactive}",
            scope_label(&c["scope"]),
            money(usd)
        );
    }
    for q in value["questions"].as_array().into_iter().flatten() {
        let id = q["id"].as_str().unwrap_or_default();
        println!(
            "Reached: {} {} ({} counted). Keep it: switchboard ceiling keep {id}. Double it in the Switchboard window.",
            scope_label(&q["scope"]),
            money(q["usd"].as_f64().unwrap_or_default()),
            money(q["spent"].as_f64().unwrap_or_default()),
        );
    }
    let last = &value["last_handover"];
    if last.is_object() {
        let skipped: Vec<String> = last["skipped"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|s| {
                format!(
                    "{} {}",
                    s["agent"].as_str().unwrap_or_default(),
                    s["reason"].as_str().unwrap_or_default()
                )
            })
            .collect();
        let skipped = if skipped.is_empty() {
            String::new()
        } else {
            format!("; skipped: {}", skipped.join(", "))
        };
        println!(
            "Last hand-over: {}, {}{skipped}",
            utc(&last["at"]),
            last["outcome"].as_str().unwrap_or_default()
        );
    }
}
/// SB-404 K1: the keys Observatory holds, one line each, never a value.
fn print_observatory_keys(value: &Value) {
    let text = |v: &Value| v.as_str().unwrap_or("").to_owned();
    if value["state"] != "ok" {
        println!("{}", text(&value["why"]));
        return;
    }
    let keys = value["keys"].as_array().cloned().unwrap_or_default();
    if keys.is_empty() {
        println!("Observatory holds no Anthropic, OpenAI or OpenRouter key an agent can run on.");
    }
    for key in keys {
        let kind = match key["kind"].as_str() {
            Some("anthropic") => "Anthropic",
            Some("openai") => "OpenAI",
            _ => "OpenRouter",
        };
        let added: Vec<String> = key["added"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|t| match t["kind"].as_str() {
                Some("account") => format!("account {}", text(&t["label"])),
                Some("anthropic") => format!("named key {}", text(&t["name"])),
                _ => match t["pool"].as_str() {
                    Some(pool) => format!("OpenRouter key of group {pool}"),
                    None => "this computer's OpenRouter key".into(),
                },
            })
            .collect();
        let state = if !added.is_empty() {
            format!("saved as {}", added.join(", "))
        } else if key["can_add"] == true {
            format!(
                "add with: switchboard keys observatory add {}",
                text(&key["slot"])
            )
        } else {
            text(&key["why_not"])
        };
        let expires = key["expires"]
            .as_str()
            .map(|d| format!(" · expires {d}"))
            .unwrap_or_default();
        println!("{} · {kind}{expires} · {state}", text(&key["slot"]));
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
/// The commands that run for long and talk over sockets or pipes they do not own the other end
/// of (the owner, the MCP server, a session's host and its viewers, a launch that waits for its
/// session, a dispatched run): a closed peer must not end them.
fn keeps_running(command: &Command) -> bool {
    matches!(
        command,
        Command::Serve
            | Command::Mcp { .. }
            | Command::SessionHost { .. }
            | Command::SessionOpen { .. }
            | Command::Attach { .. }
            | Command::Launch { .. }
            | Command::Dispatch { .. }
    )
}
async fn start() -> ExitCode {
    // Parse before resolving app data or creating any files. Even parsing failures
    // are generic: never reflect a user accidentally typing a secret as an argument.
    let mut cli = match Cli::try_parse() {
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
    // SB-403: the restricted entry always answers as the calls it serves ask, in JSON.
    if matches!(cli.command, Command::SshServe { .. }) {
        cli.json = true;
    }
    // Printing to a closed pipe (`switchboard ... | head`) made `println!` panic: 25 `panic stdio`
    // counts on 2026-11-16. A short command ends quietly instead, by SIGPIPE as other command-line
    // tools do; the long-running ones keep SIGPIPE ignored so a viewer or client going away never
    // kills them. Restoring SIGPIPE's default here is not enough: something this process loads
    // ignores it again before the output is written (measured 2026-11-16, most runs still
    // panicked), so the panic of a closed pipe itself ends the process the same way.
    #[cfg(unix)]
    if !keeps_running(&cli.command) {
        // SAFETY: restoring a signal's default action, before any other thread of ours writes.
        unsafe {
            libc::signal(libc::SIGPIPE, libc::SIG_DFL);
        }
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            if switchboard_runtime::errors::is_closed_pipe(info) {
                // SAFETY: the default action again, then the signal itself, which ends the
                // process as a closed pipe ends `cat`; 141 (128 + SIGPIPE) if it is blocked here.
                unsafe {
                    libc::signal(libc::SIGPIPE, libc::SIG_DFL);
                    libc::kill(libc::getpid(), libc::SIGPIPE);
                }
                std::process::exit(141);
            }
            previous(info);
        }));
    }
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
            // SB-189: the sentence the window shows, which says what to do; `--json` keeps the
            // backend's own text in `raw` for a script that matches it.
            let sentence = switchboard_runtime::person_errors::for_person(&error);
            // SB-343: no account for the folder is told apart by its code and status 3, which
            // `launch --target` carries back through ssh.
            let no_route = switchboard_runtime::projects::is_no_route(&error);
            if cli.json {
                let mut body = json!({"ok":false,"error":sentence});
                if sentence != error {
                    body["raw"] = json!(error);
                }
                if no_route {
                    body["code"] = json!("no-route");
                }
                eprintln!("{body}");
            } else {
                eprintln!("{sentence}");
            }
            ExitCode::from(if no_route { 3 } else { 1 })
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
        ("accounts assign", Tool("switchboard_account_update")),
        ("accounts unassign", Tool("switchboard_account_update")),
        ("accounts move", Tool("switchboard_account_move")),
        ("accounts hand-over", Tool("switchboard_account_hand_over")),
        ("accounts send", Tool("switchboard_account_send")),
        ("accounts recall", Tool("switchboard_account_recall")),
        ("accounts seats", Tool("switchboard_seats")),
        ("accounts remote", Tool("switchboard_remote")),
        ("accounts check", Tool("switchboard_remote")),
        (
            "ssh-serve",
            Not("the forced command of another machine's ssh key; it answers that machine, never an agent here"),
        ),
        ("own-sign-in on", Tool("switchboard_own_sign_in")),
        ("own-sign-in off", Tool("switchboard_own_sign_in")),
        ("proxy allow-user", Tool("switchboard_proxy_users")),
        ("proxy disallow-user", Tool("switchboard_proxy_users")),
        ("proxy users", Tool("switchboard_proxy_users")),
        ("proxy rotate-user", Tool("switchboard_proxy_users")),
        (
            "accounts surrender",
            Not("SB-292: run only by the calling machine's accounts recall over ssh; it prints a sign-in"),
        ),
        ("accounts receive", Tool("switchboard_account_receive")),
        ("org list", Tool("switchboard_organizations")),
        ("org add", Tool("switchboard_organization_set")),
        ("org edit", Tool("switchboard_organization_set")),
        ("org remove", Tool("switchboard_organization_set")),
        ("org assign", Tool("switchboard_organization_set")),
        ("org connect", Not(PERSON)),
        (
            "inbox set",
            Not("SB-256: the person makes the Fabric Inbox key and pastes its secret; a secret never travels in an agent's tool call"),
        ),
        (
            "inbox status",
            Not("SB-256: the sign-in autopilot reads the key on the owner's side; an agent has nothing to do with it"),
        ),
        ("inbox remove", Not(PERSON)),
        ("org read", Tool("switchboard_organization_set")),
        ("org passioncode", Tool("switchboard_organizations")),
        ("org disconnect", Tool("switchboard_organization_set")),
        ("org cancel", Tool("switchboard_organization_set")),
        ("analytics", Tool("switchboard_analytics")),
        ("link", Tool("switchboard_link_cli")),
        (
            "update",
            Not("SB-305: replaces the installed app; run by the account that owns it on a computer with no window, never by an agent over MCP"),
        ),
        ("app updates", Tool("switchboard_app")),
        ("app check-updates", Tool("switchboard_app")),
        ("app restart-to-update", Tool("switchboard_app")),
        ("app login-item", Tool("switchboard_app")),
        ("github owners", Tool("switchboard_github")),
        ("github repos", Tool("switchboard_github")),
        ("github clone", Tool("switchboard_github_clone")),
        ("accounts remove", Tool("switchboard_account_remove")),
        ("accounts select", Tool("switchboard_switch")),
        ("usage", Tool("switchboard_usage")),
        ("events", Tool("switchboard_events")),
        ("stats", Tool("switchboard_statistics")),
        (
            "settings export",
            Not("SB-176: a file the person carries to another machine; an agent has no second machine to carry it to"),
        ),
        ("settings import", Tool("switchboard_settings_import")),
        ("settings sync", Tool("switchboard_settings_sync")),
        (
            "settings signer",
            Not("SB-272: the administrator publishes this key to the organization's members; an agent has no members to publish it to"),
        ),
        ("status", Tool("switchboard_status")),
        ("targets", Tool("switchboard_targets")),
        ("machines list", Tool("switchboard_machines")),
        ("mcp-homes", Tool("switchboard_mcp_homes")),
        ("attention", Tool("switchboard_attention")),
        ("bugs", Tool("switchboard_bugs")),
        ("health", Tool("switchboard_health")),
        ("capacity set", Tool("switchboard_capacity")),
        ("capacity clear", Tool("switchboard_capacity")),
        ("duty on", Tool("switchboard_duty")),
        ("duty off", Tool("switchboard_duty")),
        ("duty set", Tool("switchboard_duty")),
        ("duty run", Tool("switchboard_duty")),
        ("machines adopt", Tool("switchboard_machine_set")),
        ("machines place", Tool("switchboard_machine_set")),
        ("machines forget", Tool("switchboard_machine_set")),
        ("machines trust", Tool("switchboard_machine_set")),
        ("machines keys", Tool("switchboard_machine_set")),
        (
            "machines prove",
            Not("SB-357: another machine's Switchboard runs it over ssh to sign its challenge; an agent proves nothing by it"),
        ),
        // `sessions` itself (and its `--all`, `--history`) answers through the same tool.
        ("sessions show", Tool("switchboard_sessions")),
        ("sessions end", Tool("switchboard_session_end")),
        ("sessions close-copy", Tool("switchboard_session_close_copy")),
        ("sessions continue", Tool("switchboard_session_continue")),
        ("sessions restart plan", Tool("switchboard_sessions_restart_plan")),
        ("sessions restart apply", Tool("switchboard_sessions_restart_apply")),
        ("sessions restart status", Tool("switchboard_sessions_restart_status")),
        ("sessions restart cancel", Tool("switchboard_sessions_restart_cancel")),
        (
            "monitor hook",
            Not("SB-396: Claude Code's own hooks run it with the hook input; an agent records nothing by it"),
        ),
        (
            "monitor hold",
            Not("SB-399: Fabric's claim bookkeeping marks a session that holds its task; an agent's own waits come through its hooks"),
        ),
        ("provenance", Tool("switchboard_provenance")),
        ("current", Tool("switchboard_status")),
        ("rotation status", Tool("switchboard_rotation")),
        ("rotation set", Tool("switchboard_rotation_set")),
        ("login begin", Tool("switchboard_login")),
        ("login status", Tool("switchboard_login")),
        ("login pending", Tool("switchboard_login")),
        ("login finish", Tool("switchboard_login")),
        ("login cancel", Tool("switchboard_login")),
        (
            "login code",
            Not("SB-256 N1: the sign-in autopilot gives the code on the owner's side; a person types it into the CLI, and a code never travels in an agent's tool call"),
        ),
        ("launch", Not(LAUNCH)),
        (
            "attach",
            Not("SB-342 H-1: shows a session in a person's terminal; an app attaches to the session host's socket itself"),
        ),
        (
            "session-open",
            Not("SB-342 H-1b: the command a Terminal window Switchboard opens runs, never called by hand"),
        ),
        (
            "session-host",
            Not("SB-342 H-1: the process `launch --hosted` starts for one session, never called by hand"),
        ),
        ("continue", Not(LAUNCH)),
        // `dispatch` itself starts a run as `sessions` lists them: one decision for both.
        // SB-407: Needs attention's Stop the run on a stuck or gone board run.
        ("dispatch stop", Tool("switchboard_dispatch_stop")),
        (
            "dispatch status",
            Not("SB-317: Fabric's dispatcher reads its runs' results with the CLI to relay them; an agent reads runs with switchboard_sessions"),
        ),
        ("serve", Not(PROCESS)),
        ("mcp", Not(PROCESS)),
        ("balance status", Tool("switchboard_balance")),
        ("balance groups", Tool("switchboard_balance")),
        ("agents list", Tool("switchboard_agents")),
        ("agents status", Tool("switchboard_agents")),
        ("agents enable", Tool("switchboard_agent_set")),
        ("agents disable", Tool("switchboard_agent_set")),
        ("agents default", Tool("switchboard_agent_set")),
        ("agents permissions", Tool("switchboard_agent_set")),
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
        ("agents launch-keys", Tool("switchboard_launch_keys")),
        ("keys observatory list", Tool("switchboard_observatory_keys")),
        ("keys observatory add", Tool("switchboard_observatory_key_add")),
        ("agents anthropic list", Tool("switchboard_anthropic_keys")),
        ("agents anthropic model", Tool("switchboard_anthropic_keys")),
        ("agents anthropic set", Not(PERSON)),
        ("agents anthropic remove", Not(PERSON)),
        ("agents anthropic key", Not(PERSON)),
        ("agents hermes status", Tool("switchboard_hermes_status")),
        ("agents hermes set", Tool("switchboard_hermes_set")),
        ("agents launch", Not(LAUNCH)),
        ("kimi list", Tool("switchboard_kimi_accounts")),
        ("kimi login", Tool("switchboard_kimi_login")),
        ("kimi finish", Tool("switchboard_kimi_login")),
        ("kimi status", Tool("switchboard_kimi_login")),
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
        ("ceiling list", Tool("switchboard_ceilings")),
        ("ceiling set", Tool("switchboard_ceiling_set")),
        ("ceiling keep", Tool("switchboard_ceiling_answer")),
        ("ceiling double", Tool("switchboard_ceiling_answer")),
        ("ceiling clear", Tool("switchboard_ceiling_set")),
        ("project list", Tool("switchboard_projects")),
        ("project show", Tool("switchboard_project_context")),
        ("project set", Tool("switchboard_project_set")),
        ("project remove", Tool("switchboard_project_remove")),
        ("project edit", Tool("switchboard_project_edit")),
        ("project move-root", Tool("switchboard_project_move_root")),
        ("project map", Tool("switchboard_repository_map")),
        ("project rotate-capability", Not(PERSON)),
        ("project codex", Tool("switchboard_codex_route")),
        ("project save", Tool("switchboard_project_create")),
        ("project delete", Tool("switchboard_project_delete")),
        ("project switch", Tool("switchboard_project_switch")),
        ("project suggest", Not(HOST)),
        (
            "project suggestions",
            Tool("switchboard_project_suggestions"),
        ),
        ("project bindings", Tool("switchboard_project_bindings")),
        ("project accept", Tool("switchboard_project_suggestion")),
        ("project dismiss", Tool("switchboard_project_suggestion")),
        ("project source", Tool("switchboard_project_sources")),
        ("project feed", Tool("switchboard_feed")),
        ("feed status", Tool("switchboard_feed")),
        ("feed set", Tool("switchboard_feed")),
        ("project apply", Tool("switchboard_project_apply")),
        ("backup list", Tool("switchboard_backups")),
        ("backup now", Tool("switchboard_backup_now")),
        ("backup restore", Tool("switchboard_backup_restore")),
        ("onboard", Tool("switchboard_onboarding")),
        ("shell status", Tool("switchboard_shell_hook")),
        (
            "shell env",
            Not("SB-183: the terminal hook's own question on every change of folder, one line for a shell script; not an agent action"),
        ),
        ("shell on", Tool("switchboard_shell_hook_set")),
        ("shell off", Tool("switchboard_shell_hook_set")),
        (
            "shell init",
            Not("SB-183: prints the hook script for a person's own start file; not an agent action"),
        ),
        ("uninstall", Not(PERSON)),
        (
            "vault use",
            Not("SB-287: chosen once per data folder by whoever sets up a machine with no graphical login, with Switchboard stopped; a running session cannot change it"),
        ),
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

/// SB-202: every command `switchboard onboard --status` names (`next.command`, the person's
/// `turn_on` and `turn_off`, and `alternatives`) parses as a command of this CLI, so the checklist
/// never tells an agent or a person to run one that does not exist.
#[cfg(test)]
mod onboard_commands {
    use super::{onboarding, Cli};
    use clap::Parser;
    use serde_json::{json, Value};
    use std::path::PathBuf;

    fn acme() -> onboarding::Probe {
        let mut snapshot = switchboard_core::Snapshot::default();
        snapshot
            .organizations
            .push(switchboard_core::organizations::Organization {
                id: "o-acme".into(),
                name: "Acme Inc".into(),
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
            organizations: vec!["o-acme".into()],
            shared_accounts: vec![],
            ..switchboard_core::Project::default()
        });
        snapshot.accounts.push(switchboard_core::Account {
            id: "a-1".into(),
            label: "me@acme.com".into(),
            provider: switchboard_core::Provider::Claude,
            kind: switchboard_core::AuthKind::OAuth,
            pool: "acme-work".into(),
            enabled: true,
            created_at: 1,
            identity: None,
            usage: None,
            external_identity: None,
            usage_health: None,
        });
        onboarding::Probe {
            folder: Some(PathBuf::from("/work/acme app")),
            host: Some(onboarding::Host::Claude),
            snapshot: Some(snapshot),
            shell: Some(json!({"hook": "default_on"})),
            ..Default::default()
        }
    }
    /// Every state a step can be in, as far as it changes what the step names.
    fn probes() -> Vec<onboarding::Probe> {
        let mut out = vec![
            onboarding::Probe::default(),
            onboarding::Probe {
                folder: Some(PathBuf::from("/work/my app")),
                host: Some(onboarding::Host::Codex),
                ..Default::default()
            },
        ];
        let mut none = acme();
        none.snapshot.as_mut().unwrap().organizations.clear();
        none.passioncode = true;
        out.push(none);
        for hook in ["on", "default_on", "off", "default_off", "unknown"] {
            let mut probe = acme();
            probe.shell = Some(json!({"hook": hook}));
            out.push(probe);
        }
        let mut unbound = acme();
        unbound.snapshot.as_mut().unwrap().projects[0].organizations = Vec::new();
        out.push(unbound);
        let mut two = acme();
        let snapshot = two.snapshot.as_mut().unwrap();
        let mut second = snapshot.projects[0].clone();
        second.pool = "acme-two".into();
        second.name = "Acme two".into();
        snapshot.projects.push(second);
        out.push(two);
        let mut seatless = acme();
        seatless.snapshot.as_mut().unwrap().accounts[0].pool = "default".into();
        out.push(seatless);
        out
    }
    /// Shell words: single quotes as `onboarding` writes them; a `<placeholder>` is one word.
    pub(super) fn words(command: &str) -> Vec<String> {
        let mut out = Vec::new();
        let mut word = String::new();
        let (mut quoted, mut placeholder, mut any) = (false, false, false);
        for c in command.chars() {
            match c {
                '\'' if !placeholder => {
                    quoted = !quoted;
                    any = true;
                }
                '<' if !quoted => {
                    placeholder = true;
                    any = true;
                    word.push('x');
                }
                '>' if placeholder => placeholder = false,
                _ if placeholder => {}
                ' ' if !quoted => {
                    if any {
                        out.push(std::mem::take(&mut word));
                    }
                    any = false;
                }
                _ => {
                    word.push(c);
                    any = true;
                }
            }
        }
        if any {
            out.push(word);
        }
        out
    }

    #[test]
    fn every_command_the_checklist_names_exists() {
        let mut seen = std::collections::BTreeSet::new();
        let mut checked = 0;
        for probe in probes() {
            let answer = onboarding::steps(&probe);
            for step in answer["steps"].as_array().unwrap() {
                let next = &step["next"];
                let mut commands: Vec<(&str, &Value)> = ["command", "turn_on", "turn_off"]
                    .into_iter()
                    .map(|key| (key, &next[key]))
                    .collect();
                for alternative in next["alternatives"].as_array().into_iter().flatten() {
                    commands.push(("alternatives", alternative));
                }
                for (key, command) in commands {
                    let Some(command) = command.as_str() else {
                        continue;
                    };
                    let args = words(command);
                    // `claude mcp add` and `codex mcp add` belong to the agents, not this CLI.
                    if args[0] != "switchboard" {
                        continue;
                    }
                    seen.insert(key);
                    checked += 1;
                    if let Err(error) = Cli::try_parse_from(&args) {
                        panic!("{} ({key}) does not parse: {error}", step["id"]);
                    }
                }
            }
        }
        assert!(checked >= 15, "{checked}");
        assert_eq!(
            seen.into_iter().collect::<Vec<_>>(),
            ["alternatives", "command", "turn_off", "turn_on"]
        );
    }
}

/// SB-271: every `switchboard` command the moving guide names, in English and Russian, in its
/// code blocks and in its prose, parses as a command of this CLI.
#[cfg(test)]
mod moving_commands {
    use super::{onboard_commands::words, Cli};
    use clap::Parser;

    /// The commands of one guide: each line of a `sh` block, and each inline code span that is a
    /// command; a pipe's last part and the text before a redirection or a comment.
    fn commands(text: &str) -> Vec<String> {
        let mut out = Vec::new();
        let mut in_block = false;
        for line in text.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("```") {
                in_block = trimmed == "```sh";
                continue;
            }
            if in_block {
                out.push(trimmed.to_owned());
                continue;
            }
            for (i, span) in line.split('`').enumerate() {
                if i % 2 == 1 && span.starts_with("switchboard ") {
                    out.push(span.to_owned());
                }
            }
        }
        out.into_iter()
            .map(|c| {
                let c = c.rsplit(" | ").next().unwrap_or_default();
                let c = c.split(" > ").next().unwrap_or_default();
                c.split(" # ").next().unwrap_or_default().trim().to_owned()
            })
            .filter(|c| c.starts_with("switchboard "))
            .collect()
    }

    #[test]
    fn every_command_the_moving_guide_names_exists() {
        for (guide, text) in [
            ("MOVING.md", include_str!("../../../docs/MOVING.md")),
            ("MOVING.ru.md", include_str!("../../../docs/MOVING.ru.md")),
        ] {
            let found = commands(text);
            assert!(found.len() >= 15, "{guide}: {}", found.len());
            for command in found {
                if let Err(error) = Cli::try_parse_from(words(&command)) {
                    // `--version` and `--help` print and stop: clap answers them as an "error".
                    use clap::error::ErrorKind::{DisplayHelp, DisplayVersion};
                    if !matches!(error.kind(), DisplayVersion | DisplayHelp) {
                        panic!("{guide}: `{command}` does not parse: {error}");
                    }
                }
            }
        }
    }
}
#[cfg(test)]
mod launch_surface_tests {
    use super::{Cli, Command, LaunchSurface};
    use clap::{Parser, ValueEnum};
    use switchboard_runtime::launches;

    #[test]
    fn every_surface_launch_takes_is_recorded_under_its_own_word() {
        // Fabric CO-584: Fabric's consoles pass `--surface fabric`; each word `launch` takes is
        // the one its record stores and the one a launch on another machine passes on.
        for surface in LaunchSurface::value_variants() {
            let word = surface.word();
            let cli = Cli::try_parse_from([
                "switchboard",
                "launch",
                "--provider",
                "claude",
                "--mode",
                "managed",
                "--working-directory",
                "/tmp",
                "--in-place",
                "--hosted",
                "--surface",
                word,
            ])
            .unwrap_or_else(|e| panic!("--surface {word}: {e}"));
            let Command::Launch {
                surface: parsed, ..
            } = cli.command
            else {
                panic!("not a launch");
            };
            assert_eq!(parsed.word(), word);
            let recorded = serde_json::to_value(launches::Surface::from(parsed)).unwrap();
            assert_eq!(recorded, word, "the record stores what --surface said");
        }
        assert!(LaunchSurface::value_variants()
            .iter()
            .any(|s| s.word() == "fabric"));
    }
}

#[cfg(test)]
mod there_tests {
    use super::{there, Cli};
    use clap::Parser;

    #[test]
    fn close_copy_with_a_target_runs_there_and_without_one_here() {
        // Dashboards' FD-247 (board `35e1a402`): a duplicate on another machine is closed by
        // that machine's Switchboard.
        let parse = |args: &[&str]| Cli::try_parse_from(args).unwrap();
        let cli = parse(&[
            "switchboard",
            "sessions",
            "close-copy",
            "s-1",
            "--target",
            "studio",
        ]);
        let (target, remote, _, words, tty) = there(&cli.command).expect("runs there");
        assert_eq!(target, "studio");
        assert_eq!(remote, None);
        assert_eq!(words, ["sessions", "close-copy", "s-1"]);
        assert!(!tty);
        assert!(there(&parse(&["switchboard", "sessions", "close-copy", "s-1"]).command).is_none());
        assert!(
            Cli::try_parse_from([
                "switchboard",
                "sessions",
                "close-copy",
                "s-1",
                "--remote",
                "/x"
            ])
            .is_err(),
            "--remote needs --target"
        );
    }
}
