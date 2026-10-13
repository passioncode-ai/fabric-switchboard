//! SB-117: folders the owner found for a project reach the person. A notification when a new
//! one appears; the count next to the tray icon until each is answered; and an event that makes
//! the window read its banner again. The list is read only when the owner's counter moved, so
//! an idle app reads nothing (LC-04).
use std::collections::HashSet;
use switchboard_runtime::Operation;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_notification::NotificationExt;

const LOOK_EVERY: std::time::Duration = std::time::Duration::from_secs(15);

/// What a notification says, by how many folders are new.
pub fn notification_text(new: &[(String, String)], ru: bool) -> (String, String) {
    match new {
        [(folder, project)] => {
            if ru {
                (
                    "Новая папка проекта".into(),
                    format!("Добавить {folder} в проект {project}? Откройте Switchboard."),
                )
            } else {
                (
                    "A new project folder".into(),
                    format!("Add {folder} to {project}? Open Switchboard to answer."),
                )
            }
        }
        _ => {
            let n = new.len();
            if ru {
                (
                    "Новые папки проектов".into(),
                    format!("Папок ждут ответа: {n}. Откройте Switchboard."),
                )
            } else {
                (
                    "New project folders".into(),
                    format!(
                        "{n} folders wait to be added to projects. Open Switchboard to answer."
                    ),
                )
            }
        }
    }
}

pub fn watch(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut last: Option<u64> = None;
        let mut told: HashSet<String> = HashSet::new();
        loop {
            if let Ok(runtime) = app.state::<crate::Slot>().runtime().await {
                let seq = runtime.suggestions_seq();
                if last != Some(seq) {
                    last = Some(seq);
                    if let Ok(value) = runtime.execute(Operation::ProjectSuggestions).await {
                        let rows = value["suggestions"].as_array().cloned().unwrap_or_default();
                        crate::residency::set_suggestions(&app, rows.len());
                        let new: Vec<(String, String)> = rows
                            .iter()
                            .filter(|r| r["folder"].as_str().is_some_and(|f| !told.contains(f)))
                            .map(|r| {
                                (
                                    r["name"].as_str().unwrap_or_default().to_owned(),
                                    r["project"]
                                        .as_str()
                                        .or_else(|| r["pool"].as_str())
                                        .unwrap_or_default()
                                        .to_owned(),
                                )
                            })
                            .collect();
                        for r in &rows {
                            if let Some(f) = r["folder"].as_str() {
                                told.insert(f.to_owned());
                            }
                        }
                        if !new.is_empty() {
                            let (title, body) =
                                notification_text(&new, crate::residency::in_russian());
                            let _ = app.notification().builder().title(title).body(body).show();
                        }
                        let _ = app.emit("project-suggestions", rows.len());
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
        let one = notification_text(&[("app-2".into(), "App".into())], false);
        assert_eq!(one.1, "Add app-2 to App? Open Switchboard to answer.");
        let two = notification_text(&[("a".into(), "A".into()), ("b".into(), "B".into())], true);
        assert!(two.1.contains("2"));
    }
}
