//! `switchboard mcp`: a Model Context Protocol server on stdio for coding agents.
//! Newline-delimited JSON-RPC 2.0. stdout carries protocol messages only. No tool accepts
//! or returns a credential, and launch is not exposed; a sign-in is begun here and the person
//! signs in.
use serde_json::{json, Map, Value};
use std::path::PathBuf;
use switchboard_core::Provider;
use switchboard_runtime::{
    control, onboarding,
    projects::{detect_session, Session},
    Operation,
};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

const VERSIONS: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];
const LINE_LIMIT: u64 = 1024 * 1024;
const REFRESH_SPACING: i64 = 60;
/// SB-145: a statistics range outside what is kept.
const STATISTICS_DAYS: &str = "Statistics cover 1 to 90 days.";
/// `switchboard_shell_hook_set` without its one argument.
const SHELL_HOOK_ON: &str = "on is true or false.";
/// SB-146: adding to one more group leaves none.
const MOVE_ALSO_FROM: &str = "Name either also or from, not both.";
pub(crate) const INSTRUCTIONS: &str = "Switchboard manages the Claude Code and Codex accounts on this machine. Read switchboard_status first. Remaining quota is in switchboard_usage; an unknown or stale value is not zero. A managed session changes account from the next request with switchboard_switch. A group holds repositories with accounts of its own, and a repository may be in several groups; tool names and fields call a group a project and its id a pool, but say group to the person. A repository a group's agents create or work in joins that group by itself; a repository in no group stays in the default group and is never made a group of its own. Never move account IDs to create a group. Automatic switching, when the operator turns it on, moves a group's managed sessions to its next account as one nears its limit. Folder rules are optional and off unless the operator saved one: when you start work in another repository or folder that may carry a rule, call switchboard_project_apply with it and tell the operator which rule applied. Changing the ordinary Claude Code login affects every claude session on the machine and needs global: true; ask the operator first.";

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
    let path = json!({"type":"string","description":"Absolute folder. Defaults to the folder the agent runs in."});
    let account = json!({"type":"string","description":"Account id from switchboard_accounts."});
    vec![
        (false, json!({"name":"switchboard_status","title":"Switchboard status","description":"Who handles this session's requests: runtime state, this session (managed in a group, isolated, or native), the signed-in CLI accounts, the selected account per group, active folder rules and automatic switching. Call this first.","inputSchema":{"type":"object","properties":{},"additionalProperties":false}})),
        (false, json!({"name":"switchboard_accounts","title":"Switchboard accounts","description":"Stored accounts without credentials: label, provider, group id (pool) and every group it serves (groups), whether selected or signed in now, the lowest remaining quota, the organization each belongs to (null: personal) with what decided it (person or provider), the organizations it also works for, and whether it is used on another machine (elsewhere) with where (location). machine: here or elsewhere lists only those.","inputSchema":{"type":"object","properties":{"provider":{"type":"string","enum":["claude","codex"]},"machine":{"type":"string","enum":["here","elsewhere"]}},"additionalProperties":false}})),
        (false, json!({"name":"switchboard_usage","title":"Remaining usage","description":"Remaining quota per account and window, with reset times and how fresh each observation is. Unknown is not zero. A window with scope feature limits one metered feature only; lowest_remaining_percent covers the account's own windows. refresh asks the provider for one account; within 60 seconds of its last check it returns that observation with a note, and until the time the provider asked for after answering a check with 429 it is refused; for an inactive Claude account it may first renew that account's expired sign-in inside Switchboard. A read-only server never asks and shows the stored observation. No credential is returned.","inputSchema":{"type":"object","properties":{"account_id":account,"refresh":{"type":"boolean","default":false}},"additionalProperties":false}})),
        (true, json!({"name":"switchboard_switch","title":"Switch account","description":"Choose the account for the next request. target session (default) switches this managed session; route selects the account in its own group for every managed session of that group; claude_cli changes the ordinary Claude Code login for every claude session on this machine and requires global: true. A response already streaming keeps its account.","inputSchema":{"type":"object","properties":{"account_id":account,"target":{"type":"string","enum":["session","route","claude_cli"],"default":"session"},"global":{"type":"boolean","default":false}},"required":["account_id"],"additionalProperties":false}})),
        (false, json!({"name":"switchboard_sessions","title":"Sessions","description":"Sessions Switchboard launched: Claude Code, Codex, Kimi Code, Hermes, OpenCode and catalog agents (through an API-key account's proxy or on the OpenRouter key). Each with account_id, label, agent (the catalog id, such as claude-code, codex or goose), provider (the provider the route serves: claude for an agent through the proxy), group id (pool), context (personal or org:<id>) and context_name (the organization's name, or null), mode (managed or isolated), working_directory, in_place, launched_at, started_at, continues (a workflow id or null) and state: starting, running, ended or unknown. Without all, only starting and running ones. With history, every launch of the last 90 days instead, newest first: one record per launch with launch_id, launched_by (surface: window, cli, mcp, fallback, recovery, dashboards or fabric for a run switchboard dispatch started, whose launched_by.dispatch is {id, request, thread, project, workflow}; caller_launch_id, caller_project, caller_program, by, parent), provider_session_id, transcript (its path), turns, last_turn_at, ended (limit; a dispatched run's {kind: exited, stopped or timed_out, exit_code?, at}) and continued_as (where its work went), plus total. With launch_id, that launch with up (who launched it, to the first) and down (what it launched or what took its work over). Reads only; no credential.","inputSchema":{"type":"object","properties":{"all":{"type":"boolean","description":"Also list ended sessions."},"history":{"type":"boolean","description":"Every recorded launch, newest first, instead of the homes' current sessions."},"launch_id":{"type":"string","description":"One launch and its chain."}},"additionalProperties":false}})),
        (false, json!({"name":"switchboard_provenance","title":"Provenance","description":"Which account serves a provider's group, or one launch, since when and why, as Fabric reads it: provider, pool, account_ref (the account's handle, never its id), identity_revision (a hash of the provider's identity, or null), mode (managed for a group, null for a launch), changed_at (Unix seconds, null when the journal no longer holds it), cause (manual, rotation, limit, failover or unknown), replaced ({account_ref, limit: {kind, resets_at, confidence} or null} of the account it replaced, or null) and session ({launch_id, dispatch_id, workflow} for a launch, else null). Give provider (and pool, the default group when left out), or launch_id. Reads only; no credential.","inputSchema":{"type":"object","properties":{"provider":{"type":"string","enum":["claude","codex"]},"pool":{"type":"string","description":"The group id; the default group when left out."},"launch_id":{"type":"string","description":"One launch's account and session."}},"additionalProperties":false}})),
        (false, json!({"name":"switchboard_onboarding","title":"Setup steps","description":"Where setting up Switchboard stands, as steps in order: owner (the app runs), accounts (one is saved; signing in is the person's), project (the folder's group, optional; a repository in no group uses the default group, and next.command puts it in a group), mcp (this server is registered with the agent) and proving_call. Without organizations these five are all, plus one optional organization step when PassionCode.ai is connected and lists none; with organizations, organization follows, then org_group, seat, hook and launch for each, naming it. Each step is done, todo (run next.command), needs_person (tell the person next.person; run next.command or next.alternatives once they agree) or optional. next.turn_on and next.turn_off are the person's: run one only when the person asks. degraded names reads that failed; the step they touch says so. Reads only. The full guide: `switchboard onboard`.","inputSchema":{"type":"object","properties":{"path":path,"host":{"type":"string","enum":["claude","codex"],"description":"The coding agent being set up; default: the one this server runs for, when known."}},"additionalProperties":false}})),
        (false, json!({"name":"switchboard_project_context","title":"A folder's group and rule","description":"The group a folder belongs to, as project (its name, group id as pool, folders and reserved accounts, possibly empty; null when none), and the optional folder rule per provider: the rule in force, the nearest rule in any state (paused or expired), and whether it is in effect. A group's accounts serve only sessions launched from its folders; a provider with no enabled account in the group keeps its existing launch fallback.","inputSchema":{"type":"object","properties":{"path":path},"additionalProperties":false}})),
        (false, json!({"name":"switchboard_projects","title":"Groups","description":"Every group: its id (pool), name, folders (repositories), roots (each root folder with its branch and the worktrees and clones of its repository under it), sources and the accounts reserved for it (empty for an automatically created group), then every optional folder rule with its state. Reads only.","inputSchema":{"type":"object","properties":{},"additionalProperties":false}})),
        (true, json!({"name":"switchboard_project_edit","title":"Change a group","description":"Change a group by delta: reserve accounts for it (add_accounts; an account at home in another group keeps that home and serves this group too, unless move_accounts moves it here), give accounts back (remove_accounts: an account it only shares stops serving it, its own goes to the default group), add folders (add_folders) or take folders out (remove_folders; they are not added back by themselves), or rename it, or bind it to organizations (organization: one id or name, or personal; organizations: the whole list, none, one or several, the first being the group's own context; with an organization defined, a group runs only on accounts of its organizations, or on personal ones when it has none). Everything not named stays as it is; naming an account or folder the group does not hold is refused. Only when the operator asks. The account the ordinary Claude Code or Codex is signed in to cannot join a group.","inputSchema":{"type":"object","properties":{"pool":{"type":"string","description":"The group id, from switchboard_projects."},"name":{"type":"string"},"organization":{"type":"string","maxLength":200,"description":"An organization's id or name from switchboard_organizations, or personal."},"organizations":{"type":"array","maxItems":16,"items":{"type":"string","maxLength":200},"description":"Every organization of the group, by id or name, in order; empty makes it personal. Wins over organization."},"move_accounts":{"type":"boolean","description":"Move each account in add_accounts here from its group instead of sharing it."},"add_accounts":{"type":"array","items":account,"maxItems":64},"remove_accounts":{"type":"array","items":account,"maxItems":64},"add_folders":{"type":"array","items":{"type":"string","description":"An absolute folder."},"maxItems":64},"remove_folders":{"type":"array","items":{"type":"string","description":"A folder of the group, as switchboard_projects lists it."},"maxItems":64}},"required":["pool"],"additionalProperties":false}})),
        (true, json!({"name":"switchboard_project_create","title":"Create a group","description":"Create a group, as the window's New group does: its name, its folders (repositories), the accounts reserved for it and its organizations (none, one or several by id or name, the first being the group's own context; empty or left out makes it personal). An account at home in another group serves this one too, unless move_accounts moves it here. Answers the new group with its id (pool). Only when the operator asks. The account the ordinary Claude Code or Codex is signed in to cannot join a group.","inputSchema":{"type":"object","properties":{"name":{"type":"string","minLength":1},"folders":{"type":"array","minItems":1,"maxItems":64,"items":{"type":"string","description":"An absolute folder."}},"accounts":{"type":"array","items":account,"maxItems":64},"organizations":{"type":"array","maxItems":16,"items":{"type":"string","maxLength":200},"description":"Every organization of the group, by id or name, in order; empty makes it personal."},"move_accounts":{"type":"boolean","description":"Move each listed account here from its group instead of sharing it."}},"required":["name","folders"],"additionalProperties":false}})),
        (true, json!({"name":"switchboard_codex_route","title":"Codex in group folders","description":"Whether Codex sessions in a group's folders use that group's Codex accounts (one provider in Codex's own config.toml that points at Switchboard's local proxy; folders outside groups keep the person's own sign-in). Without enabled, the state only: on, applied, conflict, and the groups with a Codex account. enabled true or false changes it, only when the operator asks. Another model_provider the person chose is never replaced.","inputSchema":{"type":"object","properties":{"enabled":{"type":"boolean"}},"additionalProperties":false}})),
        (true, json!({"name":"switchboard_project_move_root","title":"Move a repository to another group","description":"Move a repository (its root folder) with the worktrees and clones under it into another group in one step, add one that is in no group, or with also add it to one more group and keep it in its others (a repository may be in several groups). Only when the operator asks. A group keeps at least one folder; a repository inside a group folder moves with that folder.","inputSchema":{"type":"object","properties":{"folder":{"type":"string","minLength":1,"description":"The repository's absolute folder, as switchboard_projects or switchboard_repository_map lists it."},"pool":{"type":"string","minLength":1,"description":"The group id from switchboard_projects."},"also":{"type":"boolean","description":"Add it to this group and keep it in its other groups (a repository may be in several)."},"from":{"type":"string","minLength":1,"description":"The group id it leaves, when it is in several."}},"required":["folder","pool"],"additionalProperties":false}})),
        (true, json!({"name":"switchboard_app","title":"Updates and opening at login","description":"What the installed app's About and tray do, answered by the running app only. action updates: the update state, or with enabled turns Install updates automatically on or off; check_for_updates: check now; restart_to_update: install a downloaded update and restart (Switchboard drains as on Quit); login_item: whether Switchboard opens at login, or with enabled sets it. Changing anything only when the operator asks.","inputSchema":{"type":"object","properties":{"action":{"type":"string","enum":["updates","check_for_updates","restart_to_update","login_item"]},"enabled":{"type":"boolean"}},"required":["action"],"additionalProperties":false}})),
        (true, json!({"name":"switchboard_link_cli","title":"Put switchboard on PATH","description":"Link this switchboard into ~/.local/bin, as the window's Link the CLI does. Never replaces a file that is not Switchboard's. Only when the operator asks.","inputSchema":{"type":"object","properties":{},"additionalProperties":false}})),
        (true, json!({"name":"switchboard_analytics","title":"Usage analytics","description":"Switchboard's own usage analytics (counts and kinds only), as the window's About sets it: enabled true or false turns them on or off, only when the operator asks; without enabled, the state (available, enabled, notice_pending). Only the installed app sends them.","inputSchema":{"type":"object","properties":{"enabled":{"type":"boolean"}},"additionalProperties":false}})),
        (false, json!({"name":"switchboard_github","title":"GitHub owners and repositories","description":"Through the person's own gh: without owner, the owners gh can see (the person and their organizations); with owner, that owner's repositories, each with where it is on this computer and its group. Reads only.","inputSchema":{"type":"object","properties":{"owner":{"type":"string","minLength":1,"maxLength":100}},"additionalProperties":false}})),
        (true, json!({"name":"switchboard_github_clone","title":"Clone a GitHub repository","description":"Clone one repository with the person's own gh into an absolute folder that does not exist yet, as the window's GitHub page does. Only when the operator asks.","inputSchema":{"type":"object","properties":{"owner":{"type":"string","minLength":1,"maxLength":100},"name":{"type":"string","minLength":1,"maxLength":100},"into":{"type":"string","minLength":1,"description":"An absolute folder that does not exist yet."}},"required":["owner","name","into"],"additionalProperties":false}})),
        (false, json!({"name":"switchboard_repository_map","title":"Repositories on disk","description":"The Git repositories under the folders named (absolute; none: ~/DATA, else the home folder; up to three folders deep, not into a repository, a hidden folder or dependencies), each with its group (pool as its id, name; place folder or inside) or none. Linked worktrees are listed under their root in switchboard_projects, not here. Reads only.","inputSchema":{"type":"object","properties":{"under":{"type":"array","maxItems":16,"items":{"type":"string","minLength":1}}},"additionalProperties":false}})),
        (false, json!({"name":"switchboard_statistics","title":"Statistics","description":"What the local proxy relayed in the last days (1-90, default 30): requests, how they ended (ok, limited, failed) and the token counts the provider itself reported (input, output, cache read and write), by group, project (a repository with its checkouts, placed by the folder the request came from), account, coding agent (from its user-agent) and day, plus each group-project-account-agent combination and each day's split by group. Only managed traffic: a session on its own sign-in outside Switchboard is not counted. Counts only, never a prompt, answer or path. Reads only.","inputSchema":{"type":"object","properties":{"days":{"type":"integer","minimum":1,"maximum":90}},"additionalProperties":false}})),
        (false, json!({"name":"switchboard_events","title":"Activity","description":"Recent activity, newest first: what changed (accounts, switches, groups, automatic switching, sign-ins) and how it ended, as fixed codes. No credential, prompt or provider text.","inputSchema":{"type":"object","properties":{"limit":{"type":"integer","minimum":1,"maximum":500,"default":50}},"additionalProperties":false}})),
        (true, json!({"name":"switchboard_account_update","title":"Rename, enable or disable an account","description":"Rename an account, enable or disable it (a disabled account is never selected or switched to), or mark it as used on another machine (elsewhere: true disables it here, so this machine never renews it or switches to it, automatically or not; false enables it again), saying where it works (location: a machine and its user, or a colleague, which also marks it; empty clears it). An account whose sign-in was moved with switchboard_account_send is called back with switchboard_account_recall, not used here again, unless force. Only when the operator asks. What is left out stays as it is.","inputSchema":{"type":"object","properties":{"account_id":account,"label":{"type":"string","minLength":1,"maxLength":80},"enabled":{"type":"boolean"},"elsewhere":{"type":"boolean"},"location":{"type":"string","maxLength":80},"force":{"type":"boolean","description":"With elsewhere false or enabled true: use it here although its sign-in was moved to another machine and not called back."}},"required":["account_id"],"additionalProperties":false}})),
        (true, json!({"name":"switchboard_account_move","title":"Move an account to another group","description":"Move an account into a group (pool: its group id), or back to the default group (pool: default). Only when the operator asks. Joining or leaving a group is that group's change, with its refusals: the account the ordinary Claude Code or Codex is signed in to cannot join a group, and a second copy of a login already in that group is refused.","inputSchema":{"type":"object","properties":{"account_id":account,"pool":{"type":"string","minLength":1,"description":"A group id from switchboard_projects, or default."}},"required":["account_id","pool"],"additionalProperties":false}})),
        (true, json!({"name":"switchboard_account_send","title":"Move an account to another machine","description":"Move an account's sign-in to another machine over ssh, without a new sign-in there: it is marked here as used elsewhere first (this machine never renews it again), saved there with switchboard accounts add (the tokens on stdin), checked there with a quota check, and both are undone on any failure. Only a Claude or Codex subscription sign-in that no group has selected. to: the machine as ssh knows it, reachable without a prompt; pool: its group there (default); remote: that machine's switchboard, an absolute path (default the macOS app's); remote_data_dir: its data folder when it runs with --data-dir. Only when the operator asks.","inputSchema":{"type":"object","properties":{"account_id":{"type":"string"},"to":{"type":"string","minLength":1,"maxLength":255},"pool":{"type":"string"},"remote":{"type":"string"},"remote_data_dir":{"type":"string","description":"The other machine's data folder, an absolute path, when its Switchboard runs with --data-dir."},"decision":{"type":"object","properties":{"by":{"type":"string","enum":["operator"]},"instruction":{"type":"string","minLength":1,"maxLength":500},"at":{"type":"string"},"source":{"type":"string","minLength":1,"maxLength":200}},"required":["by","instruction","at","source"],"additionalProperties":false,"description":"The operator's words that ordered it, recorded with the seat in seats.jsonl."}},"required":["account_id","to"],"additionalProperties":false}})),
        (true, json!({"name":"switchboard_account_recall","title":"Take a moved account back","description":"Take back a Claude or Codex subscription sign-in this machine moved to another one with switchboard_account_send: that machine stops using it and gives its tokens back over ssh, the provider is checked here (on failure it goes back there), then its copy there is removed and its account list read to prove it. Answers recalled, removed_there and proved. Only when the operator asks.","inputSchema":{"type":"object","properties":{"account_id":{"type":"string"},"from":{"type":"string","minLength":1,"maxLength":255,"description":"The other machine as ssh knows it, as the move named it."},"remote_id":{"type":"string","maxLength":64,"description":"The account's id there; by default the one the recorded move names."},"remote":{"type":"string","description":"That machine's switchboard; by default the one the move ran."},"remote_data_dir":{"type":"string","description":"That machine's data folder; by default the one the move named."},"decision":{"type":"object","properties":{"by":{"type":"string","enum":["operator"]},"instruction":{"type":"string","minLength":1,"maxLength":500},"at":{"type":"string"},"source":{"type":"string","minLength":1,"maxLength":200}},"required":["by","instruction","at","source"],"additionalProperties":false,"description":"The operator's words that ordered it, recorded with the seat in seats.jsonl."}},"required":["account_id","from"],"additionalProperties":false}})),
        (false, json!({"name":"switchboard_seats","title":"Moves between machines","description":"The moves, hand-overs and call backs this machine recorded, newest last: account id here, the other machine as ssh named it, the group, the account's id there, who ordered it (operator with their words, or agent) and when. Use it to find what is on another machine before calling an account back. Never a credential.","inputSchema":{"type":"object","properties":{"limit":{"type":"integer","minimum":1,"maximum":1000}},"additionalProperties":false}})),
        (true, json!({"name":"switchboard_account_hand_over","title":"Hand an account over","description":"Only when the operator asks. Hand an account over to another machine or a colleague's Switchboard: answers a signed record without its credential and marks the account here as used elsewhere, with to as where it works. The receiver passes the record to switchboard_account_receive and signs in there. Hand a personal subscription only to the operator's own machines unless the operator says otherwise.","inputSchema":{"type":"object","properties":{"account_id":account,"to":{"type":"string","minLength":1,"maxLength":80,"description":"Who receives it: a machine and its user, or a colleague."},"decision":{"type":"object","properties":{"by":{"type":"string","enum":["operator"]},"instruction":{"type":"string","minLength":1,"maxLength":500},"at":{"type":"string"},"source":{"type":"string","minLength":1,"maxLength":200}},"required":["by","instruction","at","source"],"additionalProperties":false,"description":"The operator's words that ordered it, recorded with the seat in seats.jsonl."}},"required":["account_id","to"],"additionalProperties":false}})),
        (true, json!({"name":"switchboard_account_receive","title":"Receive a handed-over account","description":"Receive an account handed over: answers whose sign-in, into which group (the record's pool when this machine has it, else default), whether the signature holds and whether this machine already holds that sign-in. apply starts the sign-in (the person signs in; headless gives the URL in switchboard_login status and takes the code), only with signer and never for a sign-in this machine already holds; nothing travels but the record.","inputSchema":{"type":"object","properties":{"record":{"type":"object"},"signer":{"type":"string","description":"The giver's published key: the record must be signed by it. Required with apply."},"apply":{"type":"boolean"},"headless":{"type":"boolean"}},"required":["record"],"additionalProperties":false}})),
        (false, json!({"name":"switchboard_organizations","title":"Organizations","description":"The organizations accounts belong to: each with its e-mail domains and account count, then every account with its organization (null: personal), what decided it: person (the operator's word) or provider (its Claude Team or ChatGPT workspace is the organization's), and suggested with suggested_name: the organization its address's domain suggests (null when none or already settled). A domain only suggests; the operator confirms with switchboard_organization_set assign. Empty until the operator adds one. Reads only.","inputSchema":{"type":"object","properties":{},"additionalProperties":false}})),
        (true, json!({"name":"switchboard_organization_set","title":"Change organizations","description":"Only when the operator asks. action add (name, domains), edit (id, name and/or domains, which replaces the list), remove (id; its accounts are attributed again by the other rules) or assign (account_id, organization: an organization's id, personal, or auto to let the rules decide; or organizations: every context of an account that works for several, the first its own, the rest it also works for, empty to let the rules decide). Assigning one Claude Team or ChatGPT workspace seat brings that team's other seats along. passioncode_read reads the person's PassionCode.ai organizations again now, when PassionCode.ai is connected (connecting is the person's: switchboard org connect); passioncode_cancel stops a connection waiting for the browser; passioncode_disconnect deletes the PassionCode.ai token and keeps the organizations as local ones.","inputSchema":{"type":"object","properties":{"action":{"type":"string","enum":["add","edit","remove","assign","passioncode_read","passioncode_cancel","passioncode_disconnect"]},"id":{"type":"string","description":"An organization's id from switchboard_organizations."},"name":{"type":"string","minLength":1,"maxLength":80},"domains":{"type":"array","maxItems":16,"items":{"type":"string","minLength":3}},"account_id":account,"organization":{"type":"string","description":"An organization's id, personal, or auto."},"organizations":{"type":"array","maxItems":16,"items":{"type":"string","maxLength":200},"description":"Every context of the account in order: organization ids or names, or personal."}},"required":["action"],"additionalProperties":false}})),
        (false, json!({"name":"switchboard_rotation","title":"Automatic switching","description":"The automatic switching settings per provider, group (pool) and target (enabled, threshold, headroom as hysteresis, cooldown, freshness) and what the background monitor decided last.","inputSchema":{"type":"object","properties":{},"additionalProperties":false}})),
        (true, json!({"name":"switchboard_rotation_set","title":"Set automatic switching","description":"Change automatic switching for one provider, group (pool) and target: switch it on or off, or change its threshold, headroom (hysteresis), cooldown or freshness. What is left out keeps its saved value. Only when the operator asks.","inputSchema":{"type":"object","properties":{"provider":{"type":"string","enum":["claude","codex"]},"pool":{"type":"string","default":"default"},"target":{"type":"string","enum":["managed","claude_cli"],"default":"managed"},"enabled":{"type":"boolean"},"threshold_percent":{"type":"number","minimum":1,"maximum":100},"hysteresis_percent":{"type":"number","minimum":0,"maximum":99},"cooldown_seconds":{"type":"integer","minimum":0},"max_age_seconds":{"type":"integer","minimum":30}},"required":["provider"],"additionalProperties":false}})),
        (false, json!({"name":"switchboard_backups","title":"Backups","description":"Encrypted backups of accounts, groups, rules and settings: the folder, whether they run, the newest backups and the last error. Never their contents.","inputSchema":{"type":"object","properties":{},"additionalProperties":false}})),
        (true, json!({"name":"switchboard_backup_now","title":"Back up now","description":"Write an encrypted backup now. Restoring stays the person's, in the app or with switchboard backup restore.","inputSchema":{"type":"object","properties":{},"additionalProperties":false}})),
        (false, json!({"name":"switchboard_recovery","title":"Stopped sessions","description":"Sessions that stopped on a limit and the accounts ready for them to continue: each stop's mode and when to try again.","inputSchema":{"type":"object","properties":{},"additionalProperties":false}})),
        (false, json!({"name":"switchboard_handovers","title":"Hand-overs","description":"Whether hand-overs open in Fabric's console, and the hand-overs waiting for a console to take them.","inputSchema":{"type":"object","properties":{},"additionalProperties":false}})),
        (true, json!({"name":"switchboard_agent_set","title":"Enable or disable an agent, or set the default","description":"Let Switchboard hand work to an agent or keep work away from it (enabled), set the machine's default agent (default: an agent id, or null for none), or choose whether the Claude Code and Codex sessions it starts bypass their permission prompts (permissions: bypass, the default, or ask). Only when the operator asks.","inputSchema":{"type":"object","properties":{"agent":{"type":"string"},"enabled":{"type":"boolean"},"default":{"type":["string","null"]},"permissions":{"type":"string","enum":["bypass","ask"]}},"additionalProperties":false}})),
        (true, json!({"name":"switchboard_login","title":"Sign in a new account","description":"Start the official sign-in for a new Claude or Codex account (action begin: provider, label, pool as the group id; it opens Terminal where the person signs in) and follow it (status, finish once the person signed in, cancel); pending lists every sign-in waiting, oldest first, with its login_id, provider, label, group and state. The person signs in; this never takes a password or a key.","inputSchema":{"type":"object","properties":{"action":{"type":"string","enum":["begin","status","pending","finish","cancel"]},"login_id":{"type":"string"},"provider":{"type":"string","enum":["claude","codex"]},"label":{"type":"string","minLength":1,"maxLength":80},"pool":{"type":"string","default":"default"}},"required":["action"],"additionalProperties":false}})),
        (true, json!({"name":"switchboard_project_set","title":"Save folder rule","description":"Save an optional rule: this folder and its subfolders start on this account. Only when the operator asks for it. Rules never stop automatic switching. Prefer an expiry.","inputSchema":{"type":"object","properties":{"path":path,"account_id":account,"target":{"type":"string","enum":["managed","claude_cli"],"default":"managed"},"enabled":{"type":"boolean","default":true},"expires_in_hours":{"type":"integer","minimum":1,"maximum":720}},"required":["account_id"],"additionalProperties":false}})),
        (true, json!({"name":"switchboard_project_remove","title":"Remove folder rule","description":"Remove the rule saved for exactly this folder and provider.","inputSchema":{"type":"object","properties":{"path":path,"provider":{"type":"string","enum":["claude","codex"]}},"required":["provider"],"additionalProperties":false}})),
        (true, json!({"name":"switchboard_project_apply","title":"Apply folder rule","description":"Apply the folder's rule to this session when you start work in a folder. Reports what happened per provider: selected, activated, already_in_effect, no_rule, rule_paused, rule_expired, other_pool, other_session, needs_global or failed. Nothing changes without a rule in force.","inputSchema":{"type":"object","properties":{"path":path,"global":{"type":"boolean","default":false}},"additionalProperties":false}})),
        (false, json!({"name":"switchboard_chain_get","title":"Fallback chains","description":"Which agents continue a workflow, in what order, when its executor's accounts run out: every chain the operator set (per machine, group or workflow), the presets, and the chain that applies: the workflow's own, else its group's, else the machine's. With folder (the workflow's checkout) the group is found as the automatic fallback finds it. None applies until the operator sets one. walks: the newest automatic hand-overs, each candidate passed over with the contract's probe result.","inputSchema":{"type":"object","properties":{"workflow_id":{"type":"string","description":"An Observatory workflow id (wf_ and 16 hex digits)."},"pool":{"type":"string","description":"A group id; with folder, the executor account's group."},"folder":{"type":"string","description":"An absolute folder: the workflow's checkout."}},"additionalProperties":false}})),
        (true, json!({"name":"switchboard_chain_set","title":"Set fallback chain","description":"Set the operator's chain for a scope, first agent tried first; an empty list clears it. Only when the operator asks. Agents are catalog ids, or the contract's runner kinds (cursor-agent), that load MCP servers and take a prompt without a person (claude-code, codex, kimi-code, hermes, …); an agent may pin an account (account_id) or a paid key by its Observatory vault name (key: project/env/NAME), never a key value. Another agent than Claude Code or Codex never runs on a subscription sign-in.","inputSchema":{"type":"object","properties":{"scope":{"type":"string","description":"machine, project:<group id> or workflow:<wf_id>.","default":"machine"},"preset":{"type":"string","enum":["subscriptions-first"]},"executors":{"type":"array","maxItems":8,"items":{"type":"object","properties":{"agent":{"type":"string"},"account_id":account,"key":{"type":"string"}},"required":["agent"],"additionalProperties":false}}},"additionalProperties":false}})),
        (false, json!({"name":"switchboard_ceilings","title":"Spending ceilings","description":"Spending ceilings for paid fallback agents (Hermes and OpenCode on an OpenRouter key, Claude Code or Codex on an API-key account): each ceiling with its scope and dollars (this computer's and a group's per UTC day, a task's since its first paid hand-over), the open questions a reached ceiling asked, the last hand-over and the limits. Reads only.","inputSchema":{"type":"object","properties":{},"additionalProperties":false}})),
        (true, json!({"name":"switchboard_ceiling_set","title":"Set a spending ceiling","description":"Set a first spending ceiling for a scope, or lower one, in dollars from 0.01 to 1000. Only when the operator asks. Raising it, or clearing it with usd null, takes decision: the operator's own words (by operator, instruction verbatim, at RFC 3339, source), recorded in the ceilings; without them it is the window's. A group takes one only with its own OpenRouter key.","inputSchema":{"type":"object","properties":{"scope":{"type":"string","description":"machine, project:<group id> or workflow:<wf_id>.","default":"machine"},"usd":{"type":["number","null"],"minimum":0.01,"maximum":1000,"description":"Dollars: per UTC day for this computer and a group, since the first paid hand-over for a task; null clears (with decision)."},"decision":{"type":"object","properties":{"by":{"type":"string","enum":["operator"]},"instruction":{"type":"string","minLength":1,"maxLength":500},"at":{"type":"string"},"source":{"type":"string","minLength":1,"maxLength":200}},"required":["by","instruction","at","source"],"additionalProperties":false}},"required":["usd"],"additionalProperties":false}})),
        (true, json!({"name":"switchboard_ceiling_answer","title":"Answer a reached ceiling","description":"Answer the question a reached spending ceiling asked: keep (the ceiling stays as it is) or double, which takes decision: the operator's own words (by operator, instruction verbatim, at RFC 3339, source), recorded in the ceilings; without them doubling is the window's. Only when the operator asks.","inputSchema":{"type":"object","properties":{"id":{"type":"string","description":"The question's id from switchboard_ceilings."},"answer":{"type":"string","enum":["keep","double"],"default":"keep"},"decision":{"type":"object","properties":{"by":{"type":"string","enum":["operator"]},"instruction":{"type":"string","minLength":1,"maxLength":500},"at":{"type":"string"},"source":{"type":"string","minLength":1,"maxLength":200}},"required":["by","instruction","at","source"],"additionalProperties":false}},"required":["id"],"additionalProperties":false}})),
        (false, json!({"name":"switchboard_openrouter_status","title":"OpenRouter key for agents","description":"Whether the operator saved an OpenRouter key for agents, its default model, and what the key may still spend today and in total (asked of OpenRouter); with pool, that group's own key. Also lists every saved key with the group it is for (keys) and the groups with or without one (projects): an agent starting in a group's folder runs on that group's key, else on the machine's. Never a key itself.","inputSchema":{"type":"object","properties":{"pool":{"type":"string","description":"A group id from switchboard_projects; omit for the machine's key."}},"additionalProperties":false}})),
        (false, json!({"name":"switchboard_hermes_status","title":"Hermes model and provider","description":"The model and provider the operator's ordinary Hermes Agent runs on (hermes config get model): model, provider, endpoint, API mode. Never a key.","inputSchema":{"type":"object","properties":{},"additionalProperties":false}})),
        (true, json!({"name":"switchboard_hermes_set","title":"Change Hermes's model or provider","description":"Change the provider and/or the default model of the operator's ordinary Hermes Agent through Hermes's own hermes config set. Only when the operator asks. Changing the provider makes Hermes drop an endpoint that belonged to the previous one; the new provider needs its own key in Hermes.","inputSchema":{"type":"object","properties":{"provider":{"type":"string","description":"A Hermes provider id, for example openrouter, anthropic, nous, openai-codex, kimi-coding."},"model":{"type":"string","description":"The default model, for example moonshotai/kimi-k2."}},"additionalProperties":false}})),
        (false, json!({"name":"switchboard_agents","title":"Coding agents on this computer","description":"The coding agents Switchboard knows: which are installed here, which the operator enabled for taking over work (fallback chains and switchboard continue; account switching is separate), the machine's default agent (null is Auto: switchboard continue without an account hands work to it, and Fabric starts with it unless a project sets its own), the command Switchboard runs for each, and the recommended OpenRouter models. Changing these is the operator's (the Agents page or switchboard agents enable|disable|default).","inputSchema":{"type":"object","properties":{},"additionalProperties":false}})),
        (false, json!({"name":"switchboard_kimi_accounts","title":"Kimi Code accounts","description":"The operator's Kimi Code subscription accounts saved in Switchboard: label, region, membership tier and each plan window's used percent and reset (5-hour, 7-day, monthly), asked of Kimi; plus the ordinary kimi CLI's sign-in. Never a token. Signing in, launching and removing are the operator's (the app or switchboard kimi).","inputSchema":{"type":"object","properties":{},"additionalProperties":false}})),
        (true, json!({"name":"switchboard_kimi_login","title":"Sign in a Kimi Code account","description":"Start the official Kimi Code sign-in for a new account (action begin: label, region; the person finishes it in the browser), follow it (status, finish once the person signed in, cancel), or sign an existing account in again (again: id). The person signs in; this never takes a password or a token. Only when the operator asks.","inputSchema":{"type":"object","properties":{"action":{"type":"string","enum":["begin","status","finish","cancel","again"]},"id":{"type":"string","minLength":1},"label":{"type":"string","maxLength":80},"region":{"type":"string","enum":["mainland-cn","global"],"description":"mainland-cn (kimi.com, the default) or global (kimi.ai)."}},"required":["action"],"additionalProperties":false}})),
        (true, json!({"name":"switchboard_kimi_rename","title":"Rename a Kimi Code account","description":"Rename a saved Kimi Code account. Only when the operator asks.","inputSchema":{"type":"object","properties":{"id":{"type":"string","minLength":1,"description":"The account id from switchboard_kimi_accounts."},"label":{"type":"string","minLength":1,"maxLength":80}},"required":["id","label"],"additionalProperties":false}})),
        (true, json!({"name":"switchboard_handovers_set","title":"Hand-overs in Fabric's console","description":"Whether hand-overs open in Fabric's console (on) or straight in Terminal (off). Only when the operator asks.","inputSchema":{"type":"object","properties":{"on":{"type":"boolean"}},"required":["on"],"additionalProperties":false}})),
        (false, json!({"name":"switchboard_openrouter_models","title":"OpenRouter models","description":"The models OpenRouter offers (its public list, kept six hours), with the recommended ones first: id, name, maker, context and price per million tokens. No key is used. Reads only.","inputSchema":{"type":"object","properties":{},"additionalProperties":false}})),
        (false, json!({"name":"switchboard_project_suggestions","title":"Folders joining groups","description":"Folders Switchboard is still adding to a group (retried by itself, never asked), and those added or taken out as deleted in the last day, each with its group and why (worktree, clone, worked in). Reads only.","inputSchema":{"type":"object","properties":{},"additionalProperties":false}})),
        (false, json!({"name":"switchboard_project_bindings","title":"Folder bindings","description":"Each group folder whose Claude Code sessions are routed to the group's accounts (its .claude/settings.local.json), with its state. Never a capability value. Reads only.","inputSchema":{"type":"object","properties":{},"additionalProperties":false}})),
        (true, json!({"name":"switchboard_project_suggestion","title":"Accept or decline a folder","description":"Add a folder Switchboard is still trying to add to its group now (accept), or keep it out for good (dismiss). Only when the operator asks.","inputSchema":{"type":"object","properties":{"action":{"type":"string","enum":["accept","dismiss"]},"folder":{"type":"string","minLength":1,"description":"The folder as switchboard_project_suggestions lists it."}},"required":["action","folder"],"additionalProperties":false}})),
        (true, json!({"name":"switchboard_project_sources","title":"A group's repository owners","description":"The repository owners (such as github.com/passioncode-ai) whose new repositories join a group by themselves; add and remove change the list by delta, none of them reads it. Only when the operator asks to change it.","inputSchema":{"type":"object","properties":{"pool":{"type":"string","minLength":1},"add":{"type":"array","maxItems":16,"items":{"type":"string","minLength":1}},"remove":{"type":"array","maxItems":16,"items":{"type":"string","minLength":1}}},"required":["pool"],"additionalProperties":false}})),
        (true, json!({"name":"switchboard_openrouter_model","title":"Set the agents' OpenRouter model","description":"Change the OpenRouter model agents launched on the operator's key start on, for example moonshotai/kimi-k2 or anthropic/claude-sonnet-4.5. Only when the operator asks. The key itself is saved by the operator in the app or with switchboard agents openrouter set, never through this server.","inputSchema":{"type":"object","properties":{"model":{"type":"string","description":"An OpenRouter model id: vendor/model."},"pool":{"type":"string","description":"A group id for its own key; omit for the machine's key."}},"required":["model"],"additionalProperties":false}})),
        (false, json!({"name":"switchboard_shell_hook","title":"Terminals in organization folders","description":"Whether terminals on this computer follow organization folders (the terminal hook): its state (on, off, default_on: on because the folder map holds a folder of a group bound to an organization, default_off), the choice, whether the folder map reads, and each start file Switchboard changed with its state (written, removed_by_person, skipped_symlink, skipped_owner, skipped_zdotdir_unknown, unreadable, policy_blocked: a PowerShell profile on Windows whose execution policy, Restricted or AllSigned, keeps it from loading; Switchboard never changes the policy; constrained_language: PowerShell on Windows runs in Constrained Language Mode, where the hook cannot act); with path, the group, context and homes a terminal there gets. Reads files only.","inputSchema":{"type":"object","properties":{"path":path},"additionalProperties":false}})),
        (true, json!({"name":"switchboard_shell_hook_set","title":"Turn the terminal hook on or off","description":"Turn the terminal hook on or off for this computer's terminals, as switchboard shell on|off does. Only when the operator asks. off takes Switchboard's block out of every start file byte for byte and keeps it out; on writes it for every shell present, with or without an organization's group. Only Switchboard's own data folder changes start files. Answers the state switchboard_shell_hook reads.","inputSchema":{"type":"object","properties":{"on":{"type":"boolean"}},"required":["on"],"additionalProperties":false}})),
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

/// SB-299: an operation wrapped in the operator's words when the call carries `decision`.
fn on_operators_word(args: &Map<String, Value>, operation: Operation) -> Result<Operation, String> {
    let Some(decision) = args.get("decision") else {
        return Ok(operation);
    };
    let decision: switchboard_runtime::spend::Decision =
        serde_json::from_value(decision.clone())
            .map_err(|_| switchboard_runtime::spend::DECISION_INVALID.to_string())?;
    Ok(Operation::OnOperatorsWord {
        decision,
        inner: Box::new(operation),
    })
}
/// `scope`: machine (the default), project:<group id> or workflow:<wf_id>.
fn scope_arg(args: &Map<String, Value>) -> Result<switchboard_core::ChainScope, String> {
    let text = args
        .get("scope")
        .and_then(Value::as_str)
        .unwrap_or("machine");
    match text.split_once(':') {
        None if text == "machine" => Ok(switchboard_core::ChainScope::Machine),
        Some(("project", pool)) if !pool.is_empty() => {
            Ok(switchboard_core::ChainScope::Project { pool: pool.into() })
        }
        Some(("workflow", id)) if !id.is_empty() => {
            Ok(switchboard_core::ChainScope::Workflow { id: id.into() })
        }
        _ => Err("scope is machine, project:<group id> or workflow:<wf_id>.".into()),
    }
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
        // Waits out an owner's update restart and another command's offline lock (SB-181).
        switchboard_runtime::call(&self.root, operation).await
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
                    Err("Pass an absolute folder.".into())
                }
            }
            Some(_) => Err("path must be a string.".into()),
            None => std::env::var_os("CLAUDE_PROJECT_DIR")
                .map(PathBuf::from)
                .filter(|p| p.is_absolute())
                .or_else(|| std::env::current_dir().ok())
                .ok_or_else(|| "Pass the folder as path.".into()),
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
                "note": if status.is_some() { "The desktop app or switchboard serve owns the proxy." } else { "Switchboard is not running: managed sessions cannot route and automatic switching is paused. Reads come from local metadata." }},
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
        let machine = args.get("machine").and_then(Value::as_str);
        let snapshot = self.call(Operation::Snapshot).await?;
        // SB-284: used on another machine, and where; SB-282: every group it serves.
        let marked = |id: &Value| {
            id.as_str()
                .is_some_and(|id| snapshot["elsewhere"].get(id).is_some())
        };
        let groups_of = |a: &Value| -> Vec<Value> {
            let mut out = vec![a["pool"].clone()];
            for p in snapshot["projects"].as_array().into_iter().flatten() {
                if p["pool"] != a["pool"]
                    && p["shared_accounts"]
                        .as_array()
                        .is_some_and(|ids| ids.contains(&a["id"]))
                {
                    out.push(p["pool"].clone());
                }
            }
            out
        };
        let current = self.call(Operation::CurrentAccounts).await.ok();
        let organizations = self.call(Operation::Organizations).await.ok();
        let organization_of = |id: &Value| {
            organizations
                .as_ref()
                .and_then(|o| o["accounts"].as_array())
                .and_then(|list| list.iter().find(|a| &a["id"] == id))
                .filter(|a| !a["organization"].is_null())
                .map(|a| json!({"id": a["organization"], "name": a["organization_name"], "by": a["by"]}))
                .unwrap_or(Value::Null)
        };
        // SB-313: the organizations an account also works for (SB-282), which an agent can set
        // with `switchboard_organization_set` and could not read back.
        let also_of = |id: &Value| -> Value {
            let names = snapshot["organizations"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            id.as_str()
                .and_then(|id| snapshot["also_organizations"].get(id))
                .and_then(Value::as_array)
                .map(|ids| {
                    ids.iter()
                        .map(|o| {
                            let name = names
                                .iter()
                                .find(|n| &n["id"] == o)
                                .map(|n| n["name"].clone());
                            json!({"id": o, "name": name.unwrap_or(Value::Null)})
                        })
                        .collect::<Vec<_>>()
                })
                .map_or(json!([]), Value::Array)
        };
        let routes = snapshot["routes"].as_object().cloned().unwrap_or_default();
        let time = now();
        let accounts: Vec<Value> = snapshot["accounts"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter(|a| provider.is_none_or(|p| a["provider"] == p))
            .filter(|a| match machine {
                Some("here") => !marked(&a["id"]),
                Some("elsewhere") => marked(&a["id"]),
                _ => true,
            })
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
                    "lowest_remaining_percent": usage["lowest_remaining_percent"], "usage_fresh": usage["fresh"],
                    "organization": organization_of(&a["id"]), "also_organizations": also_of(&a["id"]), "groups": groups_of(&a),
                    "elsewhere": marked(&a["id"]),
                    "location": a["id"].as_str().and_then(|id| snapshot["locations"].get(id)).cloned().unwrap_or(Value::Null)})
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
                                return Err(format!("This session routes group {session_pool}; that account is in group {pool}. Choose an account in group {session_pool}, or launch a managed session in group {pool}."));
                            }
                        }
                        Session::Isolated { .. } => return Err("This session is isolated to one account; a route change cannot reach it. Launch a managed session to switch accounts inside a session.".into()),
                        Session::Native { .. } => return Err("This session does not run through Switchboard's proxy, so a route change cannot reach it. Use target route to prepare a group's managed sessions, or target claude_cli with global: true to change the ordinary Claude Code login.".into()),
                    }
                }
                self.call(Operation::Select {
                    provider,
                    pool: pool.clone(),
                    id: id.into(),
                })
                .await?;
                Ok(
                    json!({"action":"selected","account":{"id":id,"label":label,"pool":pool},"message":format!("{label} handles the next request in group {pool}. A response already streaming keeps its account. A manual choice starts the automatic switching cooldown.")}),
                )
            }
            _ => Err("target must be session, route or claude_cli.".into()),
        }
    }
    async fn project_edit(&self, args: &Map<String, Value>) -> Result<Value, String> {
        let strings = |key: &str| string_list(args, key);
        let absolute = |key: &str| absolute_folders(args, key);
        let project = self
            .call(Operation::ProjectEdit {
                organizations: match args.get("organizations") {
                    None => None,
                    Some(_) => Some(strings("organizations")?),
                },
                move_accounts: args
                    .get("move_accounts")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                pool: required(args, "pool")?.into(),
                name: args.get("name").and_then(Value::as_str).map(str::to_owned),
                add_accounts: strings("add_accounts")?,
                remove_accounts: strings("remove_accounts")?,
                add_folders: absolute("add_folders")?,
                remove_folders: absolute("remove_folders")?,
                organization: args
                    .get("organization")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
            })
            .await?;
        Ok(
            json!({"project": project, "message": "Group saved. It is visible in the Switchboard app under Groups."}),
        )
    }
    /// SB-299: the window's *New group*, with its organizations, as one call.
    async fn project_create(&self, args: &Map<String, Value>) -> Result<Value, String> {
        let folders = absolute_folders(args, "folders")?;
        if folders.is_empty() {
            return Err("Name at least one folder for the group.".into());
        }
        let project = self
            .call(Operation::SaveProject {
                pool: None,
                name: required(args, "name")?.into(),
                folders,
                account_ids: string_list(args, "accounts")?,
                base_folders: None,
                organization: None,
                organizations: match args.get("organizations") {
                    None => None,
                    Some(_) => Some(string_list(args, "organizations")?),
                },
                move_accounts: args
                    .get("move_accounts")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            })
            .await?;
        Ok(
            json!({"project": project, "message": "Group created. It is visible in the Switchboard app under Groups."}),
        )
    }
    async fn account_update(&self, args: &Map<String, Value>) -> Result<Value, String> {
        let label = args.get("label").and_then(Value::as_str).map(str::to_owned);
        let enabled = args.get("enabled").and_then(Value::as_bool);
        let elsewhere = args.get("elsewhere").and_then(Value::as_bool);
        let location = args
            .get("location")
            .and_then(Value::as_str)
            .map(str::to_owned);
        if label.is_none() && enabled.is_none() && elsewhere.is_none() && location.is_none() {
            return Err("Name a label, enabled, elsewhere or location to change.".into());
        }
        self.call(Operation::Update {
            id: required(args, "account_id")?.into(),
            label,
            enabled,
            elsewhere,
            location,
            force: args.get("force").and_then(Value::as_bool).unwrap_or(false),
        })
        .await
    }
    /// SB-283: moves an account's sign-in to another machine over ssh.
    async fn account_send(&self, args: &Map<String, Value>) -> Result<Value, String> {
        let text = |key: &str| args.get(key).and_then(Value::as_str).map(str::to_owned);
        let send = Operation::SendAccount {
            id: required(args, "account_id")?.into(),
            to: required(args, "to")?.into(),
            pool: text("pool"),
            program: text("remote"),
            remote_data_dir: text("remote_data_dir"),
            decision: None,
        };
        self.call(on_operators_word(args, send)?).await
    }
    /// SB-292: take a moved account's sign-in back from the machine it went to.
    async fn account_recall(&self, args: &Map<String, Value>) -> Result<Value, String> {
        let text = |key: &str| args.get(key).and_then(Value::as_str).map(str::to_owned);
        let recall = Operation::RecallAccount {
            id: required(args, "account_id")?.into(),
            from: required(args, "from")?.into(),
            remote_id: text("remote_id"),
            program: text("remote"),
            remote_data_dir: text("remote_data_dir"),
            decision: None,
        };
        self.call(on_operators_word(args, recall)?).await
    }
    /// SB-283: the signed hand-over record of an account; it is marked as used elsewhere.
    async fn account_hand_over(&self, args: &Map<String, Value>) -> Result<Value, String> {
        let hand_over = Operation::HandOver {
            id: required(args, "account_id")?.into(),
            to: required(args, "to")?.into(),
            decision: None,
        };
        self.call(on_operators_word(args, hand_over)?).await
    }
    /// SB-283: what receiving a record would do here; with `apply`, the sign-in starts.
    async fn account_receive(&self, args: &Map<String, Value>) -> Result<Value, String> {
        self.call(Operation::ReceiveHandover {
            record: args.get("record").cloned().unwrap_or(Value::Null),
            signer: args
                .get("signer")
                .and_then(Value::as_str)
                .map(str::to_owned),
            apply: args.get("apply").and_then(Value::as_bool).unwrap_or(false),
            headless: args
                .get("headless")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        })
        .await
    }
    async fn chain_get(&self, args: &Map<String, Value>) -> Result<Value, String> {
        let text = |key: &str| args.get(key).and_then(Value::as_str).map(str::to_owned);
        let folder = match args.get("folder") {
            None => None,
            Some(Value::String(f)) if std::path::Path::new(f).is_absolute() => {
                Some(PathBuf::from(f))
            }
            Some(_) => return Err("folder must be absolute.".into()),
        };
        self.call(Operation::Chains {
            workflow: text("workflow_id"),
            pool: text("pool"),
            folder,
        })
        .await
    }
    async fn project_move_root(&self, args: &Map<String, Value>) -> Result<Value, String> {
        let folder = PathBuf::from(required(args, "folder")?);
        if !folder.is_absolute() {
            return Err("folder must be absolute.".into());
        }
        let also = args.get("also").and_then(Value::as_bool).unwrap_or(false);
        let from = args.get("from").and_then(Value::as_str).map(str::to_owned);
        if also && from.is_some() {
            return Err(MOVE_ALSO_FROM.into());
        }
        self.call(Operation::MoveRoot {
            folder,
            pool: required(args, "pool")?.into(),
            from,
            also,
        })
        .await
    }
    async fn repository_map(&self, args: &Map<String, Value>) -> Result<Value, String> {
        let under = match args.get("under") {
            None => Vec::new(),
            Some(Value::Array(items)) => items
                .iter()
                .map(|v| {
                    v.as_str()
                        .map(PathBuf::from)
                        .filter(|p| p.is_absolute())
                        .ok_or("under lists absolute folders.".to_owned())
                })
                .collect::<Result<Vec<_>, _>>()?,
            Some(_) => return Err("under lists absolute folders.".into()),
        };
        self.call(Operation::RepositoryMap { under }).await
    }
    async fn organization_set(&self, args: &Map<String, Value>) -> Result<Value, String> {
        let text = |key: &str| args.get(key).and_then(Value::as_str).map(str::to_owned);
        let domains = || -> Option<Vec<String>> {
            args.get("domains").and_then(Value::as_array).map(|d| {
                d.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect()
            })
        };
        let operation = match required(args, "action")? {
            "add" => Operation::OrganizationAdd {
                name: required(args, "name")?.into(),
                domains: domains().unwrap_or_default(),
            },
            "edit" => Operation::OrganizationEdit {
                id: required(args, "id")?.into(),
                name: text("name"),
                domains: domains(),
            },
            "remove" => Operation::OrganizationRemove {
                id: required(args, "id")?.into(),
            },
            "assign" => match args.get("organizations") {
                Some(Value::Array(items)) => Operation::OrganizationAssign {
                    account: required(args, "account_id")?.into(),
                    choice: None,
                    organizations: Some(
                        items
                            .iter()
                            .map(|v| {
                                v.as_str()
                                    .map(str::to_owned)
                                    .ok_or("organizations must list strings.")
                            })
                            .collect::<Result<Vec<_>, _>>()?,
                    ),
                },
                Some(_) => return Err("organizations must be a list.".into()),
                None => Operation::OrganizationAssign {
                    account: required(args, "account_id")?.into(),
                    choice: Some(required(args, "organization")?)
                        .filter(|c| *c != "auto")
                        .map(str::to_owned),
                    organizations: None,
                },
            },
            // SB-159: read the PassionCode.ai organizations again now (connecting is the person's).
            "passioncode_read" => Operation::VestaSync,
            // SB-299: every window action has an agent door.
            "passioncode_cancel" => Operation::VestaCancel,
            "passioncode_disconnect" => Operation::VestaDisconnect,
            _ => return Err("action is add, edit, remove, assign, passioncode_read, passioncode_cancel or passioncode_disconnect.".into()),
        };
        self.call(operation).await
    }
    /// SB-299: the app's updates and login item, for an agent.
    async fn app(&self, args: &Map<String, Value>) -> Result<Value, String> {
        use switchboard_runtime::app_doors::AppAction;
        let action = match required(args, "action")? {
            "updates" => AppAction::Updates,
            "check_for_updates" => AppAction::CheckForUpdates,
            "restart_to_update" => AppAction::RestartToUpdate,
            "login_item" => AppAction::LoginItem,
            _ => {
                return Err(
                    "action is updates, check_for_updates, restart_to_update or login_item.".into(),
                )
            }
        };
        self.call(Operation::App {
            action,
            enabled: args.get("enabled").and_then(Value::as_bool),
        })
        .await
    }
    /// SB-299: the window's GitHub clone, for an agent.
    async fn github_clone(&self, args: &Map<String, Value>) -> Result<Value, String> {
        let into = PathBuf::from(required(args, "into")?);
        if !into.is_absolute() {
            return Err("Pass an absolute folder as into.".into());
        }
        self.call(Operation::GithubClone {
            owner: required(args, "owner")?.into(),
            name: required(args, "name")?.into(),
            into,
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
            .map_err(|_| "Saved automatic switching settings are unreadable.")?;
        // SB-310: a managed policy nobody saved is on by default.
        let managed = target == "managed";
        let mut policy = saved.unwrap_or(switchboard_core::RotationPolicy {
            provider,
            pool,
            target,
            enabled: managed,
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
        if let Some(mode) = args.get("permissions").and_then(Value::as_str) {
            answer = self
                .call(Operation::AgentPermissions { mode: mode.into() })
                .await?;
        }
        if answer == json!({}) {
            return Err("Name enabled (with agent), default or permissions to change.".into());
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
                    headless: false,
                })
                .await
            }
            "status" => self.call(Operation::LoginStatus { login_id: id()? }).await,
            "pending" => self.call(Operation::PendingLogins).await,
            "finish" => self.call(Operation::FinishLogin { login_id: id()? }).await,
            "cancel" => self.call(Operation::CancelLogin { login_id: id()? }).await,
            _ => Err("action is begin, status, pending, finish or cancel.".into()),
        }
    }
    async fn kimi_login(&self, args: &Map<String, Value>) -> Result<Value, String> {
        let id = || required(args, "id").map(str::to_owned);
        match required(args, "action")? {
            "begin" => {
                self.call(Operation::KimiLoginBegin {
                    label: args
                        .get("label")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .into(),
                    region: args
                        .get("region")
                        .and_then(Value::as_str)
                        .unwrap_or("mainland-cn")
                        .into(),
                })
                .await
            }
            "status" => self.call(Operation::KimiLoginStatus { id: id()? }).await,
            "finish" => self.call(Operation::KimiLoginFinish { id: id()? }).await,
            "cancel" => self.call(Operation::KimiLoginCancel { id: id()? }).await,
            "again" => self.call(Operation::KimiSignInAgain { id: id()? }).await,
            _ => Err("action is begin, status, finish, cancel or again.".into()),
        }
    }
    async fn kimi_rename(&self, args: &Map<String, Value>) -> Result<Value, String> {
        self.call(Operation::KimiRename {
            id: required(args, "id")?.into(),
            label: required(args, "label")?.into(),
        })
        .await
    }
    async fn project_suggestion(&self, args: &Map<String, Value>) -> Result<Value, String> {
        let folder: String = required(args, "folder")?.into();
        match required(args, "action")? {
            "accept" => {
                self.call(Operation::ProjectSuggestionAccept { folder })
                    .await
            }
            "dismiss" => {
                self.call(Operation::ProjectSuggestionDismiss { folder })
                    .await
            }
            _ => Err("action is accept or dismiss.".into()),
        }
    }
    /// The repository owners of a group by delta, as `switchboard project source` does.
    async fn project_sources(&self, args: &Map<String, Value>) -> Result<Value, String> {
        let pool = required(args, "pool")?;
        let list = |k: &str| -> Vec<String> {
            args.get(k)
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(str::to_owned))
                        .collect()
                })
                .unwrap_or_default()
        };
        let (add, remove) = (list("add"), list("remove"));
        let snapshot = self.call(Operation::Snapshot).await?;
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
        sources.dedup();
        self.call(Operation::ProjectSources {
            pool: pool.to_owned(),
            sources,
        })
        .await
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
            json!({"rule": rule, "message": "Rule saved. It is visible in the Switchboard app under Groups and can be paused there."}),
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
                let history = args
                    .get("history")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                match args.get("launch_id").map(Value::as_str) {
                    Some(Some(id)) => {
                        self.call(Operation::Launches {
                            launch_id: Some(id.to_owned()),
                        })
                        .await
                    }
                    Some(None) => Err(switchboard_runtime::launches::LAUNCH_NOT_FOUND.into()),
                    None if history => self.call(Operation::Launches { launch_id: None }).await,
                    None => {
                        self.call(Operation::Sessions {
                            all: args.get("all").and_then(Value::as_bool).unwrap_or(false),
                        })
                        .await
                    }
                }
            }
            "switchboard_provenance" => {
                let provider = match args.get("provider").and_then(Value::as_str) {
                    None => None,
                    Some("claude") => Some(switchboard_core::Provider::Claude),
                    Some("codex") => Some(switchboard_core::Provider::Codex),
                    Some(_) => return Some(Err("provider is claude or codex.".into())),
                };
                let text = |k: &str| args.get(k).and_then(Value::as_str).map(str::to_owned);
                self.call(Operation::Provenance {
                    provider,
                    pool: text("pool"),
                    launch_id: text("launch_id"),
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
            "switchboard_project_create" => self.project_create(args).await,
            "switchboard_statistics" => match args
                .get("days")
                .map(|v| v.as_i64().filter(|d| (1..=90).contains(d)))
            {
                Some(None) => Err(STATISTICS_DAYS.into()),
                days => {
                    self.call(Operation::Statistics {
                        days: days.flatten(),
                    })
                    .await
                }
            },
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
            "switchboard_account_send" => self.account_send(args).await,
            "switchboard_account_recall" => self.account_recall(args).await,
            "switchboard_seats" => {
                let limit = args
                    .get("limit")
                    .and_then(Value::as_u64)
                    .map(|n| n as usize);
                self.call(Operation::Seats { limit }).await
            }
            "switchboard_account_hand_over" => self.account_hand_over(args).await,
            "switchboard_account_receive" => self.account_receive(args).await,
            "switchboard_organizations" => self.call(Operation::Organizations).await,
            "switchboard_organization_set" => self.organization_set(args).await,
            "switchboard_project_move_root" => self.project_move_root(args).await,
            "switchboard_codex_route" => {
                self.call(Operation::CodexRoute {
                    enabled: args.get("enabled").and_then(Value::as_bool),
                })
                .await
            }
            "switchboard_repository_map" => self.repository_map(args).await,
            "switchboard_link_cli" => switchboard_runtime::agents::link_this_cli(),
            "switchboard_app" => self.app(args).await,
            "switchboard_analytics" => {
                self.call(Operation::Analytics {
                    enabled: args.get("enabled").and_then(Value::as_bool),
                })
                .await
            }
            "switchboard_github" => match args.get("owner").and_then(Value::as_str) {
                Some(owner) => {
                    self.call(Operation::GithubRepositories {
                        owner: owner.into(),
                    })
                    .await
                }
                None => self.call(Operation::GithubOwners).await,
            },
            "switchboard_github_clone" => self.github_clone(args).await,
            "switchboard_shell_hook" => self
                .folder(args)
                .map(|dir| crate::shell::status(Some(&self.root), Some(&dir))),
            "switchboard_shell_hook_set" => match args.get("on").and_then(Value::as_bool) {
                Some(on) => crate::shell::set(Some(&self.root), on),
                None => Err(SHELL_HOOK_ON.into()),
            },
            "switchboard_rotation" => match self.call(Operation::Snapshot).await {
                Ok(v) => {
                    let monitor = self.call(Operation::MonitorStatus).await.unwrap_or(
                        json!({"running": false, "note": "The app is not running: nothing switches automatically now."}),
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
            "switchboard_chain_get" => self.chain_get(args).await,
            "switchboard_chain_set" => self.chain_set(args).await,
            "switchboard_ceilings" => self.call(Operation::Ceilings).await,
            "switchboard_ceiling_set" => self.ceiling_set(args).await,
            "switchboard_ceiling_answer" => self.ceiling_answer(args).await,
            "switchboard_openrouter_status" => {
                let pool = args.get("pool").and_then(Value::as_str).map(str::to_owned);
                let mut status = match self
                    .call(Operation::AgentKeyStatus {
                        service: "openrouter".into(),
                        credit: true,
                        pool,
                    })
                    .await
                {
                    Ok(status) => status,
                    Err(e) => return Some(Err(e)),
                };
                // SB-144: every project's key beside the one asked about.
                if let Ok(keys) = self
                    .call(Operation::AgentKeys {
                        service: "openrouter".into(),
                    })
                    .await
                {
                    status["keys"] = keys["keys"].clone();
                    status["projects"] = keys["projects"].clone();
                }
                Ok(status)
            }
            "switchboard_agents" => self.call(Operation::AgentsOverview).await,
            "switchboard_kimi_accounts" => self.call(Operation::KimiAccounts).await,
            "switchboard_kimi_login" => self.kimi_login(args).await,
            "switchboard_kimi_rename" => self.kimi_rename(args).await,
            "switchboard_handovers_set" => match args.get("on").and_then(Value::as_bool) {
                Some(on) => self.call(Operation::HandoverSetting { on: Some(on) }).await,
                None => Err("on must be true or false.".into()),
            },
            "switchboard_openrouter_models" => self.call(Operation::OpenrouterModels).await,
            "switchboard_project_suggestions" => self.call(Operation::ProjectSuggestions).await,
            "switchboard_project_bindings" => self.call(Operation::ProjectBindings).await,
            "switchboard_project_suggestion" => self.project_suggestion(args).await,
            "switchboard_project_sources" => self.project_sources(args).await,
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
                        pool: args.get("pool").and_then(Value::as_str).map(str::to_owned),
                    })
                    .await
                }
                None => Err("Name the model.".into()),
            },
            _ => return None,
        })
    }

    async fn ceiling_set(&self, args: &Map<String, Value>) -> Result<Value, String> {
        // SB-299: with the operator's words, `usd: null` clears the ceiling.
        let usd = match args.get("usd") {
            Some(Value::Null) if args.contains_key("decision") => None,
            other => Some(
                other
                    .and_then(Value::as_f64)
                    .ok_or("usd is a number of dollars from 0.01 to 1000.")?,
            ),
        };
        let operation = Operation::SetCeiling {
            scope: scope_arg(args)?,
            usd,
            by: switchboard_runtime::spend::Surface::Mcp,
        };
        self.call(on_operators_word(args, operation)?).await
    }
    /// `keep`, or `double` with the operator's words (SB-299); without them the runtime refuses
    /// `double` with the window named, the one sentence every surface gives.
    async fn ceiling_answer(&self, args: &Map<String, Value>) -> Result<Value, String> {
        use switchboard_runtime::spend::{Answer, Surface};
        let answer = match args.get("answer").and_then(Value::as_str) {
            None | Some("keep") => Answer::Keep,
            Some("double") => Answer::Double,
            Some(_) => return Err("answer is keep or double.".into()),
        };
        let operation = Operation::AnswerCeiling {
            id: required(args, "id")?.into(),
            answer,
            by: Surface::Mcp,
        };
        self.call(on_operators_word(args, operation)?).await
    }
    async fn chain_set(&self, args: &Map<String, Value>) -> Result<Value, String> {
        let scope = scope_arg(args)?;
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
                        // SB-286: structured content is an object; an operation that answers
                        // nothing (an update, a selection) says so as `{"ok": true}`.
                        let value = match value {
                            Value::Object(_) => value,
                            Value::Null => json!({"ok": true}),
                            other => json!({"result": other}),
                        };
                        ok(
                            id,
                            json!({"content": [{"type": "text", "text": value.to_string()}], "structuredContent": value, "isError": false}),
                        )
                    }
                    // SB-189: the agent relays the sentence the window shows a person.
                    Some(Err(message)) => ok(
                        id,
                        json!({"content": [{"type": "text", "text": switchboard_runtime::person_errors::for_person(&message)}], "isError": true}),
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
fn string_list(args: &Map<String, Value>, key: &str) -> Result<Vec<String>, String> {
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
}

fn absolute_folders(args: &Map<String, Value>, key: &str) -> Result<Vec<PathBuf>, String> {
    string_list(args, key)?
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
            // SB-109: the organization the group runs on, by id and name; none is personal.
            let organization = p["organization"].as_str();
            let organization_name = organization.and_then(|id| {
                snapshot["organizations"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .find(|o| o["id"] == id)
                    .and_then(|o| o["name"].as_str())
            });
            json!({"pool": p["pool"], "name": p["name"], "folders": p["folders"], "roots": roots, "sources": p["sources"].as_array().cloned().unwrap_or_default(), "accounts": own, "organization": organization, "organization_name": organization_name})
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
