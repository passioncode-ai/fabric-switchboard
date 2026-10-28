//! SB-117/SB-121/SB-131: what the owner did to a project's folders reaches the person as one
//! system notification each — a folder that joined its project by itself, and a folder taken out
//! because it was deleted from disk. Nothing is ever asked (SB-131): a folder that could not be
//! added is tried again by the owner, with no question, no tray count and no banner. The lists are
//! read only when the owner's counter moved, so an idle app reads nothing (LC-04).
//!
//! C-7 (SB-72, SCN-084): the same look stats `spend.json`; only when its stamp moved are the
//! ceilings read, and each question a reached ceiling opened is announced once ("double it or keep
//! it in Switchboard"; nothing is answered in the notification) and the window reads them again.
use std::collections::HashSet;
use switchboard_runtime::Operation;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_notification::NotificationExt;

const LOOK_EVERY: std::time::Duration = std::time::Duration::from_secs(15);
/// A folder deleted from disk moves no counter; the list is read again at least this often.
const READ_AT_LEAST: std::time::Duration = std::time::Duration::from_secs(300);
/// Folders added before the app started are not announced again after a restart.
const ANNOUNCE_SINCE_START: i64 = 600;

/// What a notification says about folders Switchboard added to projects: (folder, project).
pub fn added_text(new: &[(String, String)], ru: bool) -> (String, String) {
    match new {
        [(folder, project)] => {
            if ru {
                (
                    "Папка добавлена в группу".into(),
                    format!("{folder} теперь в группе {project}."),
                )
            } else {
                (
                    "Folder added to a group".into(),
                    format!("{folder} is now part of {project}."),
                )
            }
        }
        _ => {
            let n = new.len();
            let projects: Vec<&str> = {
                let mut p: Vec<&str> = new.iter().map(|(_, p)| p.as_str()).collect();
                p.sort();
                p.dedup();
                p
            };
            if ru {
                (
                    "Папки добавлены в группы".into(),
                    format!("Добавлено папок: {n} ({}).", projects.join(", ")),
                )
            } else {
                (
                    "Folders added to groups".into(),
                    format!("{n} folders added ({}).", projects.join(", ")),
                )
            }
        }
    }
}

/// What a notification says about folders taken out of their project because they were deleted
/// (SB-131): (folder, project).
pub fn removed_text(gone: &[(String, String)], ru: bool) -> (String, String) {
    match gone {
        [(folder, project)] => {
            if ru {
                (
                    "Папка исключена из группы".into(),
                    format!("Папка {folder} удалена с диска и исключена из группы {project}."),
                )
            } else {
                (
                    "Folder removed from a group".into(),
                    format!("{folder} was deleted and is no longer part of {project}."),
                )
            }
        }
        _ => {
            let n = gone.len();
            if ru {
                (
                    "Папки исключены из групп".into(),
                    format!("Папки, удалённые с диска, исключены из групп: {n}."),
                )
            } else {
                (
                    "Folders removed from groups".into(),
                    format!("{n} deleted folders are no longer part of their groups."),
                )
            }
        }
    }
}

/// SB-183 (node A-7): the first time the installed app wrote the terminal hook into a start file
/// because an organization's group appeared (SCN-074 step 1). The window shows the same words and
/// acknowledges them; until it does, each start of the app posts this once.
pub fn shell_hook_text(organization: &str, group: &str, ru: bool) -> (String, String) {
    if ru {
        (
            format!("Терминалы в папках {organization} теперь работают в контексте этой организации"),
            format!("Новые терминалы в папках группы «{group}» запускают claude и codex на аккаунтах {organization} и с историей этой организации. Выключить это можно на странице «Группы»."),
        )
    } else {
        (
            format!("Terminals now work in {organization}'s context in its folders"),
            format!("New terminals opened in {group}'s folders run claude and codex on {organization}'s accounts and history. Turn this off under Groups."),
        )
    }
}

/// spend.json's stamp (modified time and length): one `stat` per look, no read (LC-04).
fn spend_stamp(root: &std::path::Path) -> Option<(std::time::SystemTime, u64)> {
    let meta = std::fs::metadata(root.join(switchboard_runtime::spend::FILE)).ok()?;
    Some((meta.modified().ok()?, meta.len()))
}

/// One reached ceiling to announce: its value and whom it covers.
#[derive(Debug, Clone, PartialEq)]
pub enum Reached {
    Machine(f64),
    Group(f64, String),
    Task(f64, String),
}

/// The open questions of a ceilings view (`spend::view`) not announced yet, each once: `told`
/// keeps the ids already announced, and a question opened before the app started (with the same
/// margin as folders) is not announced again after a restart. `group` names a pool.
pub fn reached_ceilings(
    view: &serde_json::Value,
    told: &mut HashSet<String>,
    started: i64,
    group: impl Fn(&str) -> Option<String>,
) -> Vec<Reached> {
    let mut new = Vec::new();
    for q in view["questions"].as_array().into_iter().flatten() {
        let (Some(id), Some(usd)) = (q["id"].as_str(), q["usd"].as_f64()) else {
            continue;
        };
        if q["reached_at"].as_i64().unwrap_or(0) < started - ANNOUNCE_SINCE_START {
            continue;
        }
        if !told.insert(id.to_owned()) {
            continue;
        }
        let scope = &q["scope"];
        new.push(match scope["kind"].as_str() {
            Some("project") => {
                let pool = scope["pool"].as_str().unwrap_or_default();
                Reached::Group(usd, group(pool).unwrap_or_else(|| pool.to_owned()))
            }
            Some("workflow") => {
                let task: String = scope["id"]
                    .as_str()
                    .unwrap_or_default()
                    .chars()
                    .take(11)
                    .collect();
                Reached::Task(usd, task)
            }
            _ => Reached::Machine(usd),
        });
    }
    new
}

/// What the notification of a reached ceiling says (SCN-084 step 1).
pub fn ceiling_text(reached: &Reached, ru: bool) -> (String, String) {
    let title = if ru {
        "Потолок расходов достигнут"
    } else {
        "Spending ceiling reached"
    };
    let body = match (reached, ru) {
        (Reached::Machine(usd), false) => format!("Daily ceiling ${usd:.2} for this computer reached. Double it or keep it in Switchboard."),
        (Reached::Group(usd, name), false) => format!("Daily ceiling ${usd:.2} for {name} reached. Double it or keep it in Switchboard."),
        (Reached::Task(usd, task), false) => format!("Ceiling ${usd:.2} for task {task} reached. Double it or keep it in Switchboard."),
        (Reached::Machine(usd), true) => format!("Дневной потолок этого компьютера (${usd:.2}) достигнут. Удвойте его или оставьте прежним в Switchboard."),
        (Reached::Group(usd, name), true) => format!("Дневной потолок группы «{name}» (${usd:.2}) достигнут. Удвойте его или оставьте прежним в Switchboard."),
        (Reached::Task(usd, task), true) => format!("Потолок задачи {task} (${usd:.2}) достигнут. Удвойте его или оставьте прежним в Switchboard."),
    };
    (title.to_owned(), body)
}

fn named(rows: &[serde_json::Value], told: &mut HashSet<String>) -> Vec<(String, String)> {
    let mut new = Vec::new();
    for r in rows {
        let Some(folder) = r["folder"].as_str() else {
            continue;
        };
        if told.insert(folder.to_owned()) {
            new.push((
                r["name"].as_str().unwrap_or_default().to_owned(),
                r["project"]
                    .as_str()
                    .or_else(|| r["pool"].as_str())
                    .unwrap_or_default()
                    .to_owned(),
            ));
        }
    }
    new
}

pub fn watch(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let started = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64);
        let mut last: Option<u64> = None;
        let mut read_at = std::time::Instant::now();
        let mut told_removed: HashSet<String> = HashSet::new();
        let mut told_added: HashSet<String> = HashSet::new();
        let mut told_shell: Option<String> = None;
        let mut spend_last: Option<Option<(std::time::SystemTime, u64)>> = None;
        let mut told_ceilings: HashSet<String> = HashSet::new();
        loop {
            if let Some(runtime) = app.state::<crate::Slot>().running().await {
                // C-7: spend.json's stamp; the ceilings are read only when it moved.
                let stamp = spend_stamp(&runtime.root);
                if spend_last.as_ref() != Some(&stamp) {
                    // One read settles this stamp, failed or not: an unreadable file is read again
                    // only once it changes, and the window names the failure on its own read.
                    spend_last = Some(stamp);
                    if let Ok(view) = runtime.execute(Operation::Ceilings).await {
                        let projects = runtime
                            .store
                            .snapshot()
                            .map(|s| s.projects)
                            .unwrap_or_default();
                        let reached =
                            reached_ceilings(&view, &mut told_ceilings, started, |pool| {
                                projects
                                    .iter()
                                    .find(|p| p.pool == pool)
                                    .map(|p| p.name.clone())
                            });
                        let ru = crate::residency::in_russian();
                        for r in &reached {
                            let (title, body) = ceiling_text(r, ru);
                            let _ = app.notification().builder().title(title).body(body).show();
                        }
                    }
                    let _ = app.emit("spend-changed", 0);
                }
                let seq = runtime.suggestions_seq();
                if last != Some(seq) || read_at.elapsed() >= READ_AT_LEAST {
                    if let Ok(value) = runtime.execute(Operation::ProjectSuggestions).await {
                        // Only a read that succeeded settles this counter value.
                        last = Some(seq);
                        read_at = std::time::Instant::now();
                        let ru = crate::residency::in_russian();
                        let recent = |key: &str| -> Vec<serde_json::Value> {
                            value[key]
                                .as_array()
                                .cloned()
                                .unwrap_or_default()
                                .into_iter()
                                .filter(|r| {
                                    r["first_seen"]
                                        .as_i64()
                                        .is_some_and(|t| t >= started - ANNOUNCE_SINCE_START)
                                })
                                .collect()
                        };
                        let added = named(&recent("added"), &mut told_added);
                        if !added.is_empty() {
                            let (title, body) = added_text(&added, ru);
                            let _ = app.notification().builder().title(title).body(body).show();
                        }
                        let removed = named(&recent("removed"), &mut told_removed);
                        if !removed.is_empty() {
                            let (title, body) = removed_text(&removed, ru);
                            let _ = app.notification().builder().title(title).body(body).show();
                        }
                        let _ = app.emit("project-suggestions", 0);
                    }
                    // The terminal hook's first-time notice: posted once per run while the window
                    // has not shown it (the window acknowledges it).
                    if let Some(notice) = runtime.shell_hook_notice() {
                        let key = notice.to_string();
                        if told_shell.as_deref() != Some(key.as_str()) {
                            told_shell = Some(key);
                            let (title, body) = shell_hook_text(
                                notice["organization"].as_str().unwrap_or_default(),
                                notice["group"].as_str().unwrap_or_default(),
                                crate::residency::in_russian(),
                            );
                            let _ = app.notification().builder().title(title).body(body).show();
                            let _ = app.emit("shell-hook", 0);
                        }
                    }
                }
            }
            tokio::time::sleep(LOOK_EVERY).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn one_folder_is_named_and_several_are_counted() {
        // SB-131: a deleted folder taken out of its project is announced once too.
        let one = removed_text(&[("app-wt".into(), "App".into())], false);
        assert_eq!(one.1, "app-wt was deleted and is no longer part of App.");
        let two = removed_text(&[("a".into(), "A".into()), ("b".into(), "B".into())], true);
        assert!(two.1.contains("2"));
        // SB-121: an added folder is announced, not asked about.
        let added = added_text(&[("app-2".into(), "App".into())], false);
        assert_eq!(added.1, "app-2 is now part of App.");
        let both = added_text(
            &[
                ("a".into(), "B".into()),
                ("c".into(), "A".into()),
                ("d".into(), "A".into()),
            ],
            false,
        );
        assert_eq!(both.1, "3 folders added (A, B).");
        let mut told = HashSet::new();
        let rows = [serde_json::json!({"folder": "/x/a", "name": "a", "project": "A"})];
        assert_eq!(named(&rows, &mut told).len(), 1);
        assert!(named(&rows, &mut told).is_empty(), "once per folder");
    }
    /// C-7 (SB-72, SCN-084): a reached ceiling raises one notification: once per question, never
    /// for a question already open before the app started, and nothing to answer in it.
    #[test]
    fn a_reached_ceiling_is_announced_once_per_question() {
        let started = 10_000;
        let view = serde_json::json!({"questions": [
            {"id": "q_0000000000000001", "scope": {"kind": "machine"}, "usd": 5.0, "reached_at": started + 30},
            {"id": "q_0000000000000002", "scope": {"kind": "project", "pool": "alpha"}, "usd": 3.0, "reached_at": started + 40},
            {"id": "q_0000000000000003", "scope": {"kind": "workflow", "id": "wf_0123456789abcdef"}, "usd": 2.5, "reached_at": started + 50},
            {"id": "q_0000000000000004", "scope": {"kind": "machine"}, "usd": 9.0, "reached_at": started - ANNOUNCE_SINCE_START - 1}
        ]});
        let mut told = HashSet::new();
        let names = |pool: &str| (pool == "alpha").then(|| "Alpha Web".to_owned());
        let first = reached_ceilings(&view, &mut told, started, names);
        assert_eq!(
            first.len(),
            3,
            "the question from before the start is not announced"
        );
        assert!(
            reached_ceilings(&view, &mut told, started, names).is_empty(),
            "once per question"
        );
        let (title, body) = ceiling_text(&first[0], false);
        assert_eq!(title, "Spending ceiling reached");
        assert_eq!(
            body,
            "Daily ceiling $5.00 for this computer reached. Double it or keep it in Switchboard."
        );
        assert_eq!(
            ceiling_text(&first[1], false).1,
            "Daily ceiling $3.00 for Alpha Web reached. Double it or keep it in Switchboard."
        );
        assert_eq!(
            ceiling_text(&first[2], false).1,
            "Ceiling $2.50 for task wf_01234567 reached. Double it or keep it in Switchboard."
        );
        let (title, body) = ceiling_text(&first[1], true);
        assert!(
            title.to_lowercase().contains("потолок")
                && body.contains("Alpha Web")
                && body.contains("$3.00")
        );
        for (_, body) in [ceiling_text(&first[0], true), ceiling_text(&first[2], true)] {
            assert!(!body.contains('\u{2014}'), "no dash");
        }
        // A group the snapshot no longer names is called by its id.
        let gone = serde_json::json!({"questions": [{"id": "q_0000000000000005", "scope": {"kind": "project", "pool": "beta"}, "usd": 1.0, "reached_at": started}]});
        let only = reached_ceilings(&gone, &mut told, started, names);
        assert_eq!(
            ceiling_text(&only[0], false).1,
            "Daily ceiling $1.00 for beta reached. Double it or keep it in Switchboard."
        );
    }
    /// The stamp of spend.json moves when the file is written, and is absent while there is none.
    #[test]
    fn the_spend_stamp_moves_with_the_file() {
        let dir = std::env::temp_dir().join(format!("sb-c7-stamp-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(spend_stamp(&dir), None);
        std::fs::write(dir.join("spend.json"), "{}").unwrap();
        let one = spend_stamp(&dir);
        assert!(one.is_some());
        std::fs::write(dir.join("spend.json"), "{\"version\":1}").unwrap();
        assert_ne!(spend_stamp(&dir), one, "a longer file is a new stamp");
        std::fs::remove_dir_all(&dir).unwrap();
    }
    /// SB-183 (node A-7): the first-time notice names the organization and the group, and says
    /// where to turn it off, in both languages.
    #[test]
    fn the_shell_hook_notice_names_the_organization_and_the_way_out() {
        let (title, body) = shell_hook_text("Acme", "Acme web", false);
        assert_eq!(title, "Terminals now work in Acme's context in its folders");
        assert_eq!(body, "New terminals opened in Acme web's folders run claude and codex on Acme's accounts and history. Turn this off under Groups.");
        let (title, body) = shell_hook_text("Acme", "Acme web", true);
        assert!(title.contains("Acme"));
        assert!(body.contains("Acme web") && body.contains("«Группы»"));
        for text in [title, body] {
            assert!(!text.contains('\u{2014}'), "no dash");
        }
    }
}
