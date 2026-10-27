//! The shell homes and the folder map (SB-183, node A-4).
//!
//! For each group bound to an organization, the owner's bindings pass writes the group's shell
//! homes, `<data>/shell/claude-<pool>` and (when the group has an enabled Codex account)
//! `<data>/shell/codex-<pool>`, and one row of `<data>/shell/map.json` per folder whose group
//! `Snapshot::project_for` names and whose group is bound to an organization (`map`).
//!
//! - A Claude shell home holds the person's carried files as links (SB-137: `CLAUDE.md`,
//!   skills, commands, agents, output styles, plugins, hooks; macOS and Linux), a `.claude.json`
//!   past the first-run screens with the Fabric board server (a plain `claude` takes no
//!   `--mcp-config`), and a `settings.json` of the person's carried settings with one `env`
//!   value: a fixed placeholder `ANTHROPIC_AUTH_TOKEN`. The placeholder is not a secret: the
//!   folder's `.claude/settings.local.json` (SB-118) supplies the route and the folder's
//!   capability header, and the proxy authenticates by the header alone (measured,
//!   `docs/evidence/shell-home-measurement-2026-10.md`).
//! - A Codex shell home links the person's `AGENTS.md` and holds a `config.toml` whose provider
//!   (SB-143's marked blocks) points at the proxy's `/codex/_auto/v1` with the machine's auto
//!   capability and `requires_openai_auth = false`: no sign-in at all, so a folder the proxy
//!   does not map is refused upstream (closed).
//!
//! Nothing here ever holds a credential, a session token or a folder capability. A home is
//! kept when its pool leaves (history, like launched homes). Every file is written only when its
//! bytes change, and the pass runs only when its fingerprint moves, so an idle pass writes
//! nothing.
use super::map;
use crate::continuation::McpServer;
use serde_json::{json, Map as JsonMap, Value};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use switchboard_core::{Provider, Snapshot};

/// The `ANTHROPIC_AUTH_TOKEN` a Claude shell home carries: a fixed word, never a credential.
pub const PLACEHOLDER: &str = "switchboard-shell";
const AUTH_TOKEN: &str = "ANTHROPIC_AUTH_TOKEN";
const BOARD: &str = "fabric";

/// Where the person's files come from. Injected, so a test or a synthetic owner never reads the
/// real home.
#[derive(Clone, Copy)]
pub(crate) struct Sources {
    /// The person's own folder for a provider (`person::claude_dir`, `person::codex_dir`).
    pub user: fn(Provider) -> Option<PathBuf>,
    /// The Fabric board server the person registered for a provider.
    pub board: fn(Provider) -> Option<McpServer>,
}
/// The person's own folders (never a Switchboard home, `person`) and board server.
pub(crate) const NATIVE: Sources = Sources {
    user: |provider| match provider {
        Provider::Claude => crate::person::claude_dir(),
        Provider::Codex => crate::person::codex_dir(),
    },
    board: crate::launch::board_server,
};
/// Nothing carried: synthetic owners.
pub(crate) const NO_SOURCES: Sources = Sources {
    user: |_| None,
    board: |_| None,
};

/// What one sync did, as numbers and codes for the log; never a path or a value.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Outcome {
    pub rows: usize,
    pub homes_written: usize,
    pub map_written: bool,
    /// Files that could not be written; tried again when the fingerprint next moves.
    pub failed: usize,
    /// `map::write` refused the rows (two rows name one folder): a code, never a sentence.
    pub map_refused: bool,
}

/// The organization a group is bound to, when that organization exists.
fn organization<'a>(
    snapshot: &'a Snapshot,
    project: &'a switchboard_core::Project,
) -> Option<&'a str> {
    project
        .organization
        .as_deref()
        .filter(|id| snapshot.organization(id).is_some())
}

/// Whether a group bound to an organization exists on this computer: what the homes' fingerprint
/// follows. Not the hook's default: that is `map::organizations_present`, the map's rows, which a
/// group whose folders another group takes does not get.
pub(crate) fn organizations_present(snapshot: &Snapshot) -> bool {
    snapshot
        .projects
        .iter()
        .any(|p| organization(snapshot, p).is_some())
}

/// The map's rows: one per folder (each group folder, and each worktree the Claude bindings
/// wrote inside one), in `canonicalize()` form, for the group `Snapshot::project_for` names there
/// when that group is bound to an organization. `bound` holds the folders whose Claude Code
/// route is Switchboard's now (`bindings::Report::bound_folders`); a folder whose values are the
/// person's is not bound and gets `claude: false`.
pub(crate) fn rows(snapshot: &Snapshot, bound: &BTreeSet<String>) -> Vec<map::Row> {
    rows_with(snapshot, bound, &crate::bindings::desired(snapshot))
}
/// [`rows`] with the Claude bindings' folders given (`bindings::desired`: group folders and the
/// worktrees inside them).
fn rows_with(
    snapshot: &Snapshot,
    bound: &BTreeSet<String>,
    desired: &[(PathBuf, String)],
) -> Vec<map::Row> {
    // Every group folder of a group bound to an organization, then the Claude bindings' own
    // folders (worktrees inside a group folder) of those groups.
    let mut candidates: Vec<(PathBuf, &str)> = Vec::new();
    for project in &snapshot.projects {
        if organization(snapshot, project).is_some() {
            candidates.extend(
                project
                    .folders
                    .iter()
                    .map(|f| (PathBuf::from(f), project.pool.as_str())),
            );
        }
    }
    candidates.extend(desired.iter().map(|(f, pool)| (f.clone(), pool.as_str())));
    let mut out: Vec<map::Row> = Vec::new();
    let mut seen = BTreeSet::new();
    for (folder, pool) in candidates {
        let Some(project) = snapshot.project_of_pool(pool) else {
            continue;
        };
        let Some(org) = organization(snapshot, project) else {
            continue;
        };
        // Which group a folder belongs to is decided here, once: the group `project_for` names
        // (personal groups first). Another group there, of either context, takes the folder.
        if snapshot.project_for(&folder).map(|p| p.pool.as_str()) != Some(pool) {
            continue;
        }
        // The reader resolves the folder it is asked about; a row is written resolved too.
        let Ok(canonical) = folder.canonicalize() else {
            continue;
        };
        let Some(text) = canonical.to_str() else {
            continue;
        };
        if !seen.insert(canonical.clone()) {
            continue;
        }
        let claude = folder.to_str().is_some_and(|f| bound.contains(f));
        let codex = snapshot
            .accounts
            .iter()
            .any(|a| a.pool == pool && a.enabled && a.provider == Provider::Codex);
        out.push(map::Row {
            folder: text.to_owned(),
            pool: pool.to_owned(),
            context: format!("org:{org}"),
            claude,
            codex,
        });
    }
    out.sort_by(|a, b| a.folder.cmp(&b.folder));
    out
}

/// A fingerprint of everything the homes and the map follow beyond what the Claude bindings
/// already hash: the groups' organizations, the pools' Codex accounts and the person's carried
/// settings file's stamp.
pub(crate) fn key(snapshot: &Snapshot, sources: &Sources) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    let present = organizations_present(snapshot);
    present.hash(&mut h);
    for p in &snapshot.projects {
        (&p.pool, organization(snapshot, p)).hash(&mut h);
    }
    for a in &snapshot.accounts {
        if a.provider == Provider::Codex {
            (&a.pool, a.enabled).hash(&mut h);
        }
    }
    // The person's carried settings and the names that are linked: one stat each, and only on
    // a computer with an organization's group.
    if present {
        for provider in [Provider::Claude, Provider::Codex] {
            if let Some(user) = (sources.user)(provider) {
                let stamp = |name: &str| {
                    std::fs::symlink_metadata(user.join(name))
                        .and_then(|m| m.modified())
                        .ok()
                };
                stamp("settings.json").hash(&mut h);
                for name in crate::carry::CLAUDE_LINKS
                    .iter()
                    .chain(["AGENTS.md"].iter())
                {
                    stamp(name).is_some().hash(&mut h);
                }
            }
        }
    }
    h.finish()
}

/// Writes the organization pools' shell homes and the folder map. `address` is the proxy's,
/// `auto_capability` the machine's `/codex/_auto` capability (SB-143).
pub(crate) fn sync(
    root: &Path,
    snapshot: &Snapshot,
    bound: &BTreeSet<String>,
    address: &str,
    auto_capability: &str,
    sources: &Sources,
) -> Outcome {
    let rows = rows(snapshot, bound);
    let mut outcome = Outcome {
        rows: rows.len(),
        ..Outcome::default()
    };
    // A computer that never had an organization's group gets nothing, not even the folder.
    if rows.is_empty() && !map::path(root).exists() {
        return outcome;
    }
    let claude: BTreeSet<&str> = rows
        .iter()
        .filter(|r| r.claude)
        .map(|r| r.pool.as_str())
        .collect();
    let codex: BTreeSet<&str> = rows
        .iter()
        .filter(|r| r.codex)
        .map(|r| r.pool.as_str())
        .collect();
    for pool in &claude {
        match claude_home(root, pool, sources) {
            Ok(written) => outcome.homes_written += usize::from(written),
            Err(()) => outcome.failed += 1,
        }
    }
    for pool in &codex {
        match codex_home(root, pool, address, auto_capability, sources) {
            Ok(written) => outcome.homes_written += usize::from(written),
            Err(()) => outcome.failed += 1,
        }
    }
    let map = map::Map {
        version: map::VERSION,
        rows,
    };
    let unchanged = serde_json::to_vec(&map).ok().is_some_and(|bytes| {
        switchboard_core::private_fs::read_private(&map::path(root), map::MAX_BYTES)
            .is_ok_and(|current| current == bytes)
    });
    if !unchanged {
        match map::write(root, &map) {
            Ok(()) => outcome.map_written = true,
            Err(_) => outcome.map_refused = true,
        }
    }
    outcome
}

/// Writes `bytes` (0600) unless the file already holds them. Ok(true): written now.
fn write_if_changed(path: &Path, bytes: &[u8]) -> Result<bool, ()> {
    if std::fs::read(path).is_ok_and(|current| current == bytes) {
        return Ok(false);
    }
    crate::launch::private_write(path, bytes, false).map_err(|_| ())?;
    Ok(true)
}

/// A Claude shell home: the person's carried files as links, `.claude.json` past the first-run
/// screens with the board server, `settings.json` with the placeholder. Ok(true): a file changed.
fn claude_home(root: &Path, pool: &str, sources: &Sources) -> Result<bool, ()> {
    let home = map::claude_home(root, pool);
    crate::launch::private_dir(&root.join(map::DIR)).map_err(|_| ())?;
    crate::launch::private_dir(&home).map_err(|_| ())?;
    let user = (sources.user)(Provider::Claude);
    if let Some(user) = &user {
        crate::carry::link_into(&home, user, crate::carry::CLAUDE_LINKS);
    }
    let mut written = false;
    // `.claude.json` is Claude Code's own file too (projects, history): only two keys are ours.
    let state_path = home.join(".claude.json");
    let mut state = match std::fs::read_to_string(&state_path) {
        Ok(text) => match serde_json::from_str::<Value>(&text) {
            Ok(Value::Object(m)) => Some(m),
            // Not an object: Claude Code's to repair, never overwritten.
            _ => None,
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Some(JsonMap::new()),
        Err(_) => None,
    };
    if let Some(state) = state.as_mut() {
        let before = Value::Object(state.clone());
        state.insert("hasCompletedOnboarding".into(), Value::Bool(true));
        if let Some(board) = (sources.board)(Provider::Claude) {
            let servers = state
                .entry("mcpServers")
                .or_insert_with(|| Value::Object(JsonMap::new()));
            if let Value::Object(servers) = servers {
                servers.insert(
                    BOARD.into(),
                    json!({"type": "stdio", "command": board.command, "args": board.args}),
                );
            }
        }
        let after = Value::Object(state.clone());
        if after != before || !state_path.exists() {
            let bytes = serde_json::to_vec(&after).map_err(|_| ())?;
            crate::launch::private_write(&state_path, &bytes, false).map_err(|_| ())?;
            written = true;
        }
    }
    let settings =
        crate::carry::claude_settings(user.as_deref(), Some(json!({AUTH_TOKEN: PLACEHOLDER})));
    let bytes = serde_json::to_vec_pretty(&settings).map_err(|_| ())?;
    written |= write_if_changed(&home.join("settings.json"), &bytes)?;
    Ok(written)
}

/// A Codex shell home: the person's `AGENTS.md` as a link and the `_auto` provider with no
/// sign-in; the person's own lines in its `config.toml` stay. Ok(true): the config changed.
fn codex_home(
    root: &Path,
    pool: &str,
    address: &str,
    capability: &str,
    sources: &Sources,
) -> Result<bool, ()> {
    let home = map::codex_home(root, pool);
    crate::launch::private_dir(&root.join(map::DIR)).map_err(|_| ())?;
    crate::launch::private_dir(&home).map_err(|_| ())?;
    if let Some(user) = (sources.user)(Provider::Codex) {
        crate::carry::link_into(&home, &user, &["AGENTS.md"]);
    }
    let path = home.join("config.toml");
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(_) => return Err(()),
    };
    let next =
        crate::codex_route::with_provider(&text, address, capability, false).map_err(|_| ())?;
    write_if_changed(&path, next.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use switchboard_core::{organizations::Organization, Account, AuthKind, Project};

    const O1: &str = "00000000-0000-4000-8000-000000000001";
    const O2: &str = "00000000-0000-4000-8000-000000000002";
    const CAPABILITY: &str = "auto-capability-synthetic-0123456789";

    fn org(id: &str) -> Organization {
        Organization {
            id: id.into(),
            name: format!("Org {id}"),
            domains: vec![],
            provider_organizations: vec![],
            created_at: 1,
            vesta_id: None,
        }
    }
    fn account(id: &str, pool: &str, provider: Provider) -> Account {
        Account {
            id: id.into(),
            label: id.into(),
            provider,
            kind: AuthKind::OAuth,
            pool: pool.into(),
            enabled: true,
            created_at: 1,
            identity: None,
            external_identity: None,
            usage: None,
            usage_health: None,
        }
    }
    fn group(pool: &str, folders: &[&Path], organization: Option<&str>) -> Project {
        Project {
            pool: pool.into(),
            name: pool.into(),
            folders: folders
                .iter()
                .map(|f| f.to_string_lossy().into_owned())
                .collect(),
            created_at: 1,
            sources: vec![],
            organization: organization.map(str::to_owned),
        }
    }
    fn s(p: &Path) -> String {
        p.to_string_lossy().into_owned()
    }
    fn bound(folders: &[&Path]) -> BTreeSet<String> {
        folders.iter().map(|f| s(f)).collect()
    }
    /// A person's Claude Code folder with carried files and a settings file holding things that
    /// must never reach a shell home.
    fn user_dir() -> &'static Path {
        static DIR: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
        DIR.get_or_init(|| {
            let dir = std::env::temp_dir().join(format!(
                "sb-a4-user-{}-{}",
                std::process::id(),
                uuid::Uuid::new_v4()
            ));
            std::fs::create_dir_all(dir.join("skills")).unwrap();
            std::fs::write(dir.join("CLAUDE.md"), "the person's rules").unwrap();
            std::fs::write(dir.join("AGENTS.md"), "the person's codex rules").unwrap();
            std::fs::write(
                dir.join("settings.json"),
                json!({"theme": "dark", "env": {"ANTHROPIC_API_KEY": "personal-secret"},
                    "apiKeyHelper": "/bin/echo personal", "permissions": {"allow": ["Bash"]}})
                .to_string(),
            )
            .unwrap();
            dir
        })
    }
    fn carried(_: Provider) -> Option<PathBuf> {
        Some(user_dir().to_owned())
    }
    fn board(provider: Provider) -> Option<McpServer> {
        (provider == Provider::Claude).then(|| McpServer {
            command: "/opt/fabric/bin/fabric-mcp".into(),
            args: vec!["serve".into()],
        })
    }
    const SOURCES: Sources = Sources {
        user: carried,
        board,
    };
    /// Every byte of every file under `dir`, links not followed.
    fn all_text(dir: &Path) -> String {
        let mut out = String::new();
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let meta = std::fs::symlink_metadata(entry.path()).unwrap();
            if meta.is_dir() {
                out.push_str(&all_text(&entry.path()));
            } else if meta.is_file() {
                out.push_str(&String::from_utf8_lossy(
                    &std::fs::read(entry.path()).unwrap(),
                ));
            }
        }
        out
    }

    struct World {
        _tmp: tempfile::TempDir,
        root: PathBuf,
        work: PathBuf,
    }
    fn world() -> World {
        let tmp = tempfile::tempdir().unwrap();
        let base = tmp.path().canonicalize().unwrap();
        let root = base.join("data");
        let work = base.join("work");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&work).unwrap();
        World {
            _tmp: tmp,
            root,
            work,
        }
    }
    fn folder(w: &World, name: &str) -> PathBuf {
        let f = w.work.join(name);
        std::fs::create_dir_all(&f).unwrap();
        f
    }

    #[test]
    fn an_organization_pools_shell_home_has_the_placeholder_carried_links_and_no_credential() {
        let w = world();
        let acme = folder(&w, "acme");
        let mut snapshot = Snapshot {
            organizations: vec![org(O1)],
            projects: vec![group("acme", &[&acme], Some(O1))],
            accounts: vec![account("a", "acme", Provider::Claude)],
            ..Snapshot::default()
        };
        let outcome = sync(
            &w.root,
            &snapshot,
            &bound(&[&acme]),
            "127.0.0.1:4100",
            CAPABILITY,
            &SOURCES,
        );
        assert_eq!(outcome.rows, 1);
        assert!(outcome.map_written);
        let home = map::claude_home(&w.root, "acme");
        let settings: Value =
            serde_json::from_slice(&std::fs::read(home.join("settings.json")).unwrap()).unwrap();
        assert_eq!(settings["env"], json!({AUTH_TOKEN: PLACEHOLDER}));
        assert_eq!(
            settings["theme"], "dark",
            "the person's settings come along"
        );
        assert!(
            settings.get("apiKeyHelper").is_none(),
            "nothing that signs in"
        );
        #[cfg(unix)]
        {
            assert_eq!(
                std::fs::read_link(home.join("CLAUDE.md")).unwrap(),
                user_dir().join("CLAUDE.md")
            );
            assert!(std::fs::symlink_metadata(home.join("skills"))
                .unwrap()
                .file_type()
                .is_symlink());
        }
        let state: Value =
            serde_json::from_slice(&std::fs::read(home.join(".claude.json")).unwrap()).unwrap();
        assert_eq!(state["hasCompletedOnboarding"], true);
        assert_eq!(
            state["mcpServers"][BOARD],
            json!({"type": "stdio", "command": "/opt/fabric/bin/fabric-mcp", "args": ["serve"]})
        );
        // No credential, no capability, no personal secret anywhere in the home.
        assert!(!home.join(".credentials.json").exists());
        let text = all_text(&home);
        assert!(!text.contains(CAPABILITY));
        assert!(!text.contains("personal-secret"));
        // No Codex account: no Codex home, and the map row says so.
        assert!(!map::codex_home(&w.root, "acme").exists());
        let map::Loaded::Read(m) = map::read(&w.root) else {
            panic!("map")
        };
        assert_eq!(
            m.rows,
            vec![map::Row {
                folder: s(&acme),
                pool: "acme".into(),
                context: format!("org:{O1}"),
                claude: true,
                codex: false,
            }]
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode(&map::path(&w.root)), 0o600);
            assert_eq!(mode(&home.join("settings.json")), 0o600);
            assert_eq!(mode(&home) & 0o077, 0, "the home is the person's only");
        }
        // A person's own `.claude.json` edits in the home survive a rewrite.
        let mut own: Value =
            serde_json::from_slice(&std::fs::read(home.join(".claude.json")).unwrap()).unwrap();
        own["projects"] = json!({"/x": {"history": ["kept"]}});
        std::fs::write(home.join(".claude.json"), own.to_string()).unwrap();
        snapshot
            .accounts
            .push(account("c", "acme", Provider::Codex));
        sync(
            &w.root,
            &snapshot,
            &bound(&[&acme]),
            "127.0.0.1:4100",
            CAPABILITY,
            &SOURCES,
        );
        let state: Value =
            serde_json::from_slice(&std::fs::read(home.join(".claude.json")).unwrap()).unwrap();
        assert_eq!(state["projects"]["/x"]["history"][0], "kept");
    }

    #[test]
    fn a_codex_shell_home_routes_to_auto_with_no_sign_in() {
        let w = world();
        let acme = folder(&w, "acme");
        let snapshot = Snapshot {
            organizations: vec![org(O1)],
            projects: vec![group("acme", &[&acme], Some(O1))],
            accounts: vec![account("c", "acme", Provider::Codex)],
            ..Snapshot::default()
        };
        sync(
            &w.root,
            &snapshot,
            &BTreeSet::new(),
            "127.0.0.1:4100",
            CAPABILITY,
            &SOURCES,
        );
        let home = map::codex_home(&w.root, "acme");
        let text = std::fs::read_to_string(home.join("config.toml")).unwrap();
        let config: toml::Table = text.parse().unwrap();
        assert_eq!(config["model_provider"].as_str(), Some("switchboard-auto"));
        let provider = &config["model_providers"]["switchboard-auto"];
        assert_eq!(
            provider["base_url"].as_str(),
            Some("http://127.0.0.1:4100/codex/_auto/v1")
        );
        assert_eq!(provider["requires_openai_auth"].as_bool(), Some(false));
        assert_eq!(
            provider["http_headers"]["X-Switchboard-Capability"].as_str(),
            Some(CAPABILITY)
        );
        assert!(
            provider.get("env_key").is_none(),
            "no session token from the env"
        );
        assert!(!home.join("auth.json").exists(), "no sign-in");
        #[cfg(unix)]
        assert_eq!(
            std::fs::read_link(home.join("AGENTS.md")).unwrap(),
            user_dir().join("AGENTS.md")
        );
        // No Claude account bound in the folder: no Claude home; the row carries Codex only.
        assert!(!map::claude_home(&w.root, "acme").exists());
        let found = map::answer(&w.root, &acme.join("src")).unwrap();
        assert_eq!(found.claude_home, None);
        assert_eq!(found.codex_home, Some(home.clone()));
        // A rotated capability is written in; nothing else of the person's lines moves.
        let mut own = std::fs::read_to_string(home.join("config.toml")).unwrap();
        own.push_str("\n[mcp_servers.mine]\ncommand = \"/bin/true\"\n");
        std::fs::write(home.join("config.toml"), &own).unwrap();
        sync(
            &w.root,
            &snapshot,
            &BTreeSet::new(),
            "127.0.0.1:4100",
            "rotated-capability",
            &SOURCES,
        );
        let text = std::fs::read_to_string(home.join("config.toml")).unwrap();
        assert!(text.contains("rotated-capability"));
        assert!(!text.contains(CAPABILITY));
        assert!(text.contains("[mcp_servers.mine]"));
    }

    #[test]
    fn a_port_change_repoints_shell_homes() {
        let w = world();
        let acme = folder(&w, "acme");
        let snapshot = Snapshot {
            organizations: vec![org(O1)],
            projects: vec![group("acme", &[&acme], Some(O1))],
            accounts: vec![
                account("a", "acme", Provider::Claude),
                account("c", "acme", Provider::Codex),
            ],
            ..Snapshot::default()
        };
        sync(
            &w.root,
            &snapshot,
            &bound(&[&acme]),
            "127.0.0.1:4100",
            CAPABILITY,
            &SOURCES,
        );
        let config = map::codex_home(&w.root, "acme").join("config.toml");
        // The owner's start found its port taken: the generated files follow at once, before
        // the first bindings pass.
        assert_eq!(crate::launch::repoint_managed(&w.root, 4100, 4200), 1);
        let text = std::fs::read_to_string(&config).unwrap();
        assert!(text.contains("http://127.0.0.1:4200/codex/_auto/v1"));
        assert!(!text.contains(":4100/"));
        // The next pass with the new address writes the same bytes again: nothing changes.
        let before = std::fs::read(&config).unwrap();
        let outcome = sync(
            &w.root,
            &snapshot,
            &bound(&[&acme]),
            "127.0.0.1:4200",
            CAPABILITY,
            &SOURCES,
        );
        assert_eq!(std::fs::read(&config).unwrap(), before);
        assert_eq!(outcome.homes_written, 0);
        assert!(!outcome.map_written);
        // A pass with another address rewrites it.
        sync(
            &w.root,
            &snapshot,
            &bound(&[&acme]),
            "127.0.0.1:4300",
            CAPABILITY,
            &SOURCES,
        );
        assert!(std::fs::read_to_string(&config)
            .unwrap()
            .contains("http://127.0.0.1:4300/codex/_auto/v1"));
    }

    #[test]
    fn a_personal_group_gets_no_shell_home_and_no_map_row() {
        let w = world();
        let mine = folder(&w, "mine");
        let shared = folder(&w, "shared");
        let acme = folder(&w, "acme");
        // A personal group alone: nothing at all is written, not even the `shell` folder.
        let personal = Snapshot {
            organizations: vec![org(O1)],
            projects: vec![group("mine", &[&mine, &shared], None)],
            accounts: vec![
                account("a", "mine", Provider::Claude),
                account("c", "mine", Provider::Codex),
            ],
            ..Snapshot::default()
        };
        let outcome = sync(
            &w.root,
            &personal,
            &bound(&[&mine, &shared]),
            "127.0.0.1:4100",
            CAPABILITY,
            &SOURCES,
        );
        assert_eq!(outcome, Outcome::default());
        assert!(!w.root.join(map::DIR).exists());
        assert!(!organizations_present(&personal));
        // With an organization's group beside it, sharing one folder: `project_for` names the
        // personal group there (the more private wins), so that folder has no row.
        let mut both = personal.clone();
        both.projects
            .push(group("acme", &[&acme, &shared], Some(O1)));
        both.accounts.push(account("b", "acme", Provider::Claude));
        assert!(organizations_present(&both));
        sync(
            &w.root,
            &both,
            &bound(&[&mine, &shared, &acme]),
            "127.0.0.1:4100",
            CAPABILITY,
            &SOURCES,
        );
        let map::Loaded::Read(m) = map::read(&w.root) else {
            panic!("map")
        };
        let folders: Vec<&str> = m.rows.iter().map(|r| r.folder.as_str()).collect();
        assert_eq!(folders, vec![s(&acme)]);
        assert!(map::answer(&w.root, &shared).is_none());
        assert!(map::answer(&w.root, &mine).is_none());
        assert!(!map::claude_home(&w.root, "mine").exists());
        assert!(!map::codex_home(&w.root, "mine").exists());
        // An organization that no longer exists binds nothing.
        let mut gone = both.clone();
        gone.organizations.clear();
        gone.organizations.push(org(O2));
        assert!(!organizations_present(&gone));
        assert!(rows(&gone, &bound(&[&acme])).is_empty());
    }

    #[test]
    fn a_pool_that_leaves_its_organization_keeps_its_home_and_loses_its_rows() {
        let w = world();
        let acme = folder(&w, "acme");
        let mut snapshot = Snapshot {
            organizations: vec![org(O1)],
            projects: vec![group("acme", &[&acme], Some(O1))],
            accounts: vec![account("a", "acme", Provider::Claude)],
            ..Snapshot::default()
        };
        sync(
            &w.root,
            &snapshot,
            &bound(&[&acme]),
            "127.0.0.1:1",
            CAPABILITY,
            &SOURCES,
        );
        let home = map::claude_home(&w.root, "acme");
        std::fs::create_dir_all(home.join("projects")).unwrap();
        std::fs::write(home.join("projects").join("history.jsonl"), "kept").unwrap();
        snapshot.projects[0].organization = None;
        let outcome = sync(
            &w.root,
            &snapshot,
            &bound(&[&acme]),
            "127.0.0.1:1",
            CAPABILITY,
            &SOURCES,
        );
        assert!(outcome.map_written, "the map loses the row");
        assert_eq!(
            map::read(&w.root),
            map::Loaded::Read(map::Map {
                version: map::VERSION,
                rows: vec![]
            })
        );
        assert!(map::answer(&w.root, &acme).is_none());
        assert_eq!(
            std::fs::read_to_string(home.join("projects").join("history.jsonl")).unwrap(),
            "kept",
            "the organization's history stays on disk"
        );
    }

    #[test]
    fn worktrees_and_folders_the_person_keeps_get_their_own_rows() {
        // A folder whose Claude Code values are the person's (SB-118 left it alone) is not
        // bound, so its row carries no Claude home; a worktree inside a bound folder that the
        // bindings could not write likewise gets its own row, which wins by length.
        let w = world();
        let acme = folder(&w, "acme");
        let kept = folder(&w, "kept");
        let worktree = acme.join("wt");
        std::fs::create_dir_all(&worktree).unwrap();
        let snapshot = Snapshot {
            organizations: vec![org(O1)],
            projects: vec![group("acme", &[&acme, &kept], Some(O1))],
            accounts: vec![
                account("a", "acme", Provider::Claude),
                account("c", "acme", Provider::Codex),
            ],
            ..Snapshot::default()
        };
        let candidates = vec![(worktree.clone(), "acme".to_string())];
        let rows = rows_with(&snapshot, &bound(&[&acme]), &candidates);
        let by: BTreeMap<String, (bool, bool)> = rows
            .iter()
            .map(|r| (r.folder.clone(), (r.claude, r.codex)))
            .collect();
        assert_eq!(by[&s(&acme)], (true, true));
        assert_eq!(by[&s(&kept)], (false, true));
        assert_eq!(by[&s(&worktree)], (false, true));
        assert_eq!(rows.len(), 3);
    }

    #[test]
    fn rows_are_written_in_canonical_form_for_folders_that_exist() {
        let w = world();
        let acme = folder(&w, "acme");
        let missing = w.work.join("missing");
        let snapshot = Snapshot {
            organizations: vec![org(O1)],
            projects: vec![group("acme", &[&acme, &missing], Some(O1))],
            accounts: vec![account("a", "acme", Provider::Claude)],
            ..Snapshot::default()
        };
        let rows = rows(&snapshot, &bound(&[&acme]));
        assert_eq!(rows.len(), 1, "a folder that does not exist has no row");
        assert_eq!(rows[0].folder, s(&acme.canonicalize().unwrap()));
    }

    /// Windows: `canonicalize()` answers `\\?\C:\...`; the reader compares folders as Windows
    /// names them, so a plain `C:\...` spelling (what PowerShell's `$PWD` gives) matches the row.
    /// The Claude home is answered in the bound folder itself only, so the subfolder is asked
    /// for the group's Codex home.
    #[cfg(windows)]
    #[test]
    fn windows_rows_are_verbatim_and_a_plain_spelling_finds_them() {
        let w = world();
        let acme = folder(&w, "acme");
        let snapshot = Snapshot {
            organizations: vec![org(O1)],
            projects: vec![group("acme", &[&acme], Some(O1))],
            accounts: vec![
                account("a", "acme", Provider::Claude),
                account("c", "acme", Provider::Codex),
            ],
            ..Snapshot::default()
        };
        sync(
            &w.root,
            &snapshot,
            &bound(&[&acme]),
            "127.0.0.1:1",
            CAPABILITY,
            &NO_SOURCES,
        );
        let map::Loaded::Read(m) = map::read(&w.root) else {
            panic!("map")
        };
        assert!(
            m.rows[0].folder.starts_with(r"\\?\"),
            "{}",
            m.rows[0].folder
        );
        std::fs::create_dir_all(acme.join("src")).unwrap();
        let plain = PathBuf::from(s(&acme).trim_start_matches(r"\\?\").to_owned());
        let here = map::answer(&w.root, &plain).unwrap();
        assert_eq!(here.pool, "acme");
        assert!(here.claude_home.is_some(), "the bound folder itself");
        let below = map::answer(&w.root, &plain.join("src")).unwrap();
        assert_eq!(below.pool, "acme");
        assert_eq!(below.claude_home, None);
        assert!(below.codex_home.is_some());
    }
}
