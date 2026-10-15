//! SB-117/SB-121/SB-131: what the owner did to a project's folders reaches the person as one
//! system notification each — a folder that joined its project by itself, and a folder taken out
//! because it was deleted from disk. Nothing is ever asked (SB-131): a folder that could not be
//! added is tried again by the owner, with no question, no tray count and no banner. The lists are
//! read only when the owner's counter moved, so an idle app reads nothing (LC-04).
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
                    "Папка добавлена в проект".into(),
                    format!("{folder} теперь в проекте {project}."),
                )
            } else {
                (
                    "Folder added to a project".into(),
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
                    "Папки добавлены в проекты".into(),
                    format!("Добавлено папок: {n} ({}).", projects.join(", ")),
                )
            } else {
                (
                    "Folders added to projects".into(),
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
                    "Папка исключена из проекта".into(),
                    format!("Папка {folder} удалена с диска и исключена из проекта {project}."),
                )
            } else {
                (
                    "Folder removed from a project".into(),
                    format!("{folder} was deleted and is no longer part of {project}."),
                )
            }
        }
        _ => {
            let n = gone.len();
            if ru {
                (
                    "Папки исключены из проектов".into(),
                    format!("Папки, удалённые с диска, исключены из проектов: {n}."),
                )
            } else {
                (
                    "Folders removed from projects".into(),
                    format!("{n} deleted folders are no longer part of their projects."),
                )
            }
        }
    }
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
        loop {
            if let Some(runtime) = app.state::<crate::Slot>().running().await {
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
}
