//! SB-363 (WF-1.3, DEC-0019): every command the window can invoke has an MCP tool, or a written
//! reason it has none. The commands are read from `tauri::generate_handler![..]` in `main.rs` and
//! the tools from the MCP server's own list in `switchboard-cli`, both as source text, the way the
//! CLI's table (`every_cli_command_has_an_mcp_tool_or_a_reason`) reads its commands. A command
//! added without a row, a row whose command is gone, and a row naming a tool that does not exist
//! each fail. MCP may do more than the window; this checks one direction only.

enum Over {
    Tool(&'static str),
    Not(&'static str),
}
use Over::{Not, Tool};

/// The person's own act: a secret typed into the window, a sign-in the person makes, or a
/// removal that cannot be restored. The CLI's table holds the same line for its commands.
const PERSON: &str =
    "the person's own act: it takes a secret, imports another tool's sign-ins or deletes what cannot be restored";
/// Launching a coding agent from an agent is not offered (SB-64, SB-74), as in the CLI's table.
const LAUNCH: &str = "launching an agent from an agent is SB-64/SB-74";
/// The window's own plumbing: a system picker, a notice it shows once, its tray menu.
const UI_ONLY: &str =
    "window plumbing: a system picker, a notice the window shows once or its start signal; an agent has nothing to decide";

const PARITY: &[(&str, Over)] = &[
    ("frontend_ready", Not(UI_ONLY)),
    (
        "set_language",
        // The language is the viewer's choice kept in the window (src/i18n.ts, localStorage);
        // the command only tells the tray menu which language to draw. Nothing is stored in
        // Switchboard's data folder, so there is no setting for an agent to read or change.
        Not("the window tells its own tray menu the language the viewer chose in the window; Switchboard stores no language"),
    ),
    ("login_item", Tool("switchboard_app")),
    ("set_login_item", Tool("switchboard_app")),
    ("shell_hook_notice", Not(UI_ONLY)),
    ("acknowledge_shell_hook_notice", Not(UI_ONLY)),
    ("shell_hook_status", Tool("switchboard_shell_hook")),
    ("onboard", Tool("switchboard_onboarding")),
    ("set_shell_hook", Tool("switchboard_shell_hook_set")),
    ("update_status", Tool("switchboard_app")),
    ("set_auto_update", Tool("switchboard_app")),
    ("restart_to_update", Tool("switchboard_app")),
    ("check_for_updates", Tool("switchboard_app")),
    ("analytics_status", Tool("switchboard_analytics")),
    ("set_analytics", Tool("switchboard_analytics")),
    ("analytics_notice_seen", Not(UI_ONLY)),
    ("snapshot", Tool("switchboard_accounts")),
    ("current_accounts", Tool("switchboard_status")),
    ("capture_current", Not(PERSON)),
    ("import_claude_swap", Not(PERSON)),
    ("activate_native", Tool("switchboard_switch")),
    ("set_policy", Tool("switchboard_rotation_set")),
    ("monitor_status", Tool("switchboard_rotation")),
    ("sessions", Tool("switchboard_sessions")),
    ("add_account", Not(PERSON)),
    ("update_account", Tool("switchboard_account_update")),
    ("send_account", Tool("switchboard_account_send")),
    ("recall_account", Tool("switchboard_account_recall")),
    ("machines", Tool("switchboard_machines")),
    ("attention", Tool("switchboard_attention")),
    ("given_accounts", Tool("switchboard_accounts")),
    ("events", Tool("switchboard_events")),
    ("hand_over_account", Tool("switchboard_account_hand_over")),
    // The system file picker; an agent passes the record to switchboard_account_receive itself.
    ("open_handover", Not(UI_ONLY)),
    ("receive_handover", Tool("switchboard_account_receive")),
    ("mark_elsewhere", Tool("switchboard_account_update")),
    ("assign_account", Tool("switchboard_account_update")),
    ("remove_account", Tool("switchboard_account_remove")),
    ("select_account", Tool("switchboard_switch")),
    ("runtime_status", Tool("switchboard_status")),
    ("launch_account", Not(LAUNCH)),
    ("begin_login", Tool("switchboard_login")),
    ("finish_login", Tool("switchboard_login")),
    ("login_status", Tool("switchboard_login")),
    ("pending_logins", Tool("switchboard_login")),
    ("backups", Tool("switchboard_backups")),
    ("backup_now", Tool("switchboard_backup_now")),
    ("restore_backup", Tool("switchboard_backup_restore")),
    ("cancel_login", Tool("switchboard_login")),
    ("pick_folder", Not(UI_ONLY)),
    ("pick_folders", Not(UI_ONLY)),
    ("github_owners", Tool("switchboard_github")),
    ("github_repositories", Tool("switchboard_github")),
    ("github_clone", Tool("switchboard_github_clone")),
    ("project_sources", Tool("switchboard_project_sources")),
    ("probe_usage", Tool("switchboard_usage")),
    ("set_project_rule", Tool("switchboard_project_set")),
    ("remove_project_rule", Tool("switchboard_project_remove")),
    // A new group; with its pool the same command edits one (switchboard_project_edit).
    ("save_project", Tool("switchboard_project_create")),
    ("agent_connect", Tool("switchboard_onboarding")),
    ("launch_agent", Not(LAUNCH)),
    ("openrouter_status", Tool("switchboard_openrouter_status")),
    ("openrouter_keys", Tool("switchboard_openrouter_status")),
    ("openrouter_save", Not(PERSON)),
    ("openrouter_model", Tool("switchboard_openrouter_model")),
    ("openrouter_remove", Not(PERSON)),
    ("kimi_accounts", Tool("switchboard_kimi_accounts")),
    ("hermes_model", Tool("switchboard_hermes_status")),
    ("hermes_set_model", Tool("switchboard_hermes_set")),
    ("kimi_login_begin", Tool("switchboard_kimi_login")),
    ("kimi_login_status", Tool("switchboard_kimi_login")),
    ("kimi_login_finish", Tool("switchboard_kimi_login")),
    ("kimi_login_cancel", Tool("switchboard_kimi_login")),
    ("kimi_sign_in_again", Tool("switchboard_kimi_login")),
    ("kimi_launch", Not(LAUNCH)),
    ("kimi_remove", Not(PERSON)),
    ("remove_project", Tool("switchboard_project_delete")),
    ("project_roots", Tool("switchboard_projects")),
    ("move_root", Tool("switchboard_project_move_root")),
    ("codex_route", Tool("switchboard_codex_route")),
    ("balance", Tool("switchboard_balance")),
    ("repository_map", Tool("switchboard_repository_map")),
    ("statistics", Tool("switchboard_statistics")),
    ("move_account", Tool("switchboard_account_move")),
    ("organization_add", Tool("switchboard_organization_set")),
    ("organization_edit", Tool("switchboard_organization_set")),
    ("organization_remove", Tool("switchboard_organization_set")),
    ("organization_assign", Tool("switchboard_organization_set")),
    // status, read, cancel and disconnect; connecting is the person's sign-in in the browser,
    // as the CLI's `org connect` is.
    ("passioncode", Tool("switchboard_organization_set")),
    ("agents_overview", Tool("switchboard_agents")),
    ("chains", Tool("switchboard_chain_get")),
    ("set_chain", Tool("switchboard_chain_set")),
    ("ceilings", Tool("switchboard_ceilings")),
    ("set_ceiling", Tool("switchboard_ceiling_set")),
    ("answer_ceiling", Tool("switchboard_ceiling_answer")),
    ("agent_enable", Tool("switchboard_agent_set")),
    ("agent_default", Tool("switchboard_agent_set")),
    ("agent_permissions", Tool("switchboard_agent_set")),
    ("openrouter_models", Tool("switchboard_openrouter_models")),
    ("project_suggestions", Tool("switchboard_project_suggestions")),
    ("accept_project_suggestion", Tool("switchboard_project_suggestion")),
    ("dismiss_project_suggestion", Tool("switchboard_project_suggestion")),
    ("agent_setup", Tool("switchboard_onboarding")),
    ("link_cli", Tool("switchboard_link_cli")),
];

/// The command names inside `tauri::generate_handler![..]`.
fn commands(main: &str) -> Vec<String> {
    let start = main
        .find("tauri::generate_handler![")
        .expect("main.rs registers its commands with generate_handler")
        + "tauri::generate_handler![".len();
    let end = start + main[start..].find(']').expect("the handler list closes");
    main[start..end]
        .split(',')
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .collect()
}

/// The tool names the MCP server lists: each `"name":"switchboard_..."` in its source.
fn tools(mcp: &str) -> Vec<String> {
    const KEY: &str = "\"name\":\"switchboard_";
    mcp.match_indices(KEY)
        .filter_map(|(at, _)| {
            let rest = &mcp[at + "\"name\":\"".len()..];
            rest.find('"').map(|end| rest[..end].to_owned())
        })
        .collect()
}

/// Every way the table and the sources disagree, as sentences; empty when they agree.
fn gaps(commands: &[String], tools: &[String], table: &[(&str, Over)]) -> Vec<String> {
    let mut found = Vec::new();
    for command in commands {
        if !table.iter().any(|(c, _)| c == command) {
            found.push(format!("window command with no MCP decision: {command}"));
        }
    }
    for (command, over) in table {
        if !commands.iter().any(|c| c == command) {
            found.push(format!("no such window command: {command}"));
        }
        match over {
            Tool(name) if !tools.iter().any(|t| t == name) => {
                found.push(format!("{command}: no MCP tool {name}"))
            }
            Not(reason) if reason.len() <= 20 => {
                found.push(format!("{command}: the reason does not say why"))
            }
            _ => {}
        }
    }
    found
}

const MAIN: &str = include_str!("main.rs");
const MCP: &str = include_str!("../../crates/switchboard-cli/src/mcp.rs");

#[test]
fn every_window_command_has_an_mcp_tool_or_a_reason() {
    let commands = commands(MAIN);
    let tools = tools(MCP);
    assert!(
        commands.len() > 90,
        "the handler list was read: {commands:?}"
    );
    assert!(tools.len() > 60, "the MCP tool list was read: {tools:?}");
    let found = gaps(&commands, &tools, PARITY);
    assert!(found.is_empty(), "{found:#?}");
    let covered = PARITY.iter().filter(|(_, o)| matches!(o, Tool(_))).count();
    assert!(
        covered * 4 > PARITY.len() * 3,
        "most window commands have a tool"
    );
}

/// The check itself fails both ways: a command added without a row, and a row naming a tool
/// the MCP server does not list.
#[test]
fn a_command_without_a_row_and_a_row_without_its_tool_both_fail() {
    let mut commands = commands(MAIN);
    let tools = tools(MCP);
    commands.push("planted_command".into());
    let found = gaps(&commands, &tools, PARITY);
    assert_eq!(
        found,
        ["window command with no MCP decision: planted_command"]
    );
    let commands = vec!["snapshot".to_owned()];
    let table = [("snapshot", Tool("switchboard_planted_tool"))];
    assert_eq!(
        gaps(&commands, &tools, &table),
        ["snapshot: no MCP tool switchboard_planted_tool"]
    );
    let table = [("snapshot", Not("short"))];
    assert_eq!(
        gaps(&commands, &tools, &table),
        ["snapshot: the reason does not say why"]
    );
    let table = [
        ("snapshot", Tool("switchboard_accounts")),
        ("gone", Not(UI_ONLY)),
    ];
    assert_eq!(
        gaps(&commands, &tools, &table),
        ["no such window command: gone"]
    );
}
