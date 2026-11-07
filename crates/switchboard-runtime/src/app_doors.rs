//! SB-299: the window's actions that live in the app process (automatic updates, open at
//! login), for an agent. The installed app hands the runtime its doors when its owner starts,
//! as it hands it the analytics client; `serve` and the CLI have none and say so.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{future::Future, pin::Pin};

pub type Answer = Pin<Box<dyn Future<Output = Result<Value, String>> + Send>>;

pub trait AppDoors: Send + Sync {
    /// `{available, enabled, state, current, version, …}`, as About shows it.
    fn updates(&self) -> Value;
    fn set_auto_update(&self, enabled: bool) -> Result<Value, String>;
    /// About's *Check for updates*: runs with the switch off too.
    fn check_for_updates(&self) -> Answer;
    /// The tray's *Restart to update*: quits through the drain and starts the new copy.
    fn restart_to_update(&self) -> Answer;
    /// `{available, enabled}` of *Open at login*.
    fn login_item(&self) -> Value;
    fn set_login_item(&self, enabled: bool) -> Result<Value, String>;
}

pub const APP_ONLY: &str = "Only the installed app answers this. Open Switchboard, then try again.";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AppAction {
    /// The update state; with `enabled`, *Install updates automatically* on or off.
    Updates,
    CheckForUpdates,
    RestartToUpdate,
    /// The login item's state; with `enabled`, *Open at login* on or off.
    LoginItem,
}

pub(crate) async fn answer(
    doors: Option<std::sync::Arc<dyn AppDoors>>,
    action: AppAction,
    enabled: Option<bool>,
) -> Result<Value, String> {
    let doors = doors.ok_or(APP_ONLY)?;
    match (action, enabled) {
        (AppAction::Updates, None) => Ok(doors.updates()),
        (AppAction::Updates, Some(on)) => doors.set_auto_update(on),
        (AppAction::CheckForUpdates, _) => doors.check_for_updates().await,
        (AppAction::RestartToUpdate, _) => doors.restart_to_update().await,
        (AppAction::LoginItem, None) => Ok(doors.login_item()),
        (AppAction::LoginItem, Some(on)) => doors.set_login_item(on),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct Fake {
        calls: Mutex<Vec<String>>,
    }
    impl AppDoors for Fake {
        fn updates(&self) -> Value {
            json!({"available": true, "enabled": true})
        }
        fn set_auto_update(&self, enabled: bool) -> Result<Value, String> {
            self.calls.lock().unwrap().push(format!("auto {enabled}"));
            Ok(json!({"enabled": enabled}))
        }
        fn check_for_updates(&self) -> Answer {
            self.calls.lock().unwrap().push("check".into());
            Box::pin(async { Ok(json!({"state": "current"})) })
        }
        fn restart_to_update(&self) -> Answer {
            self.calls.lock().unwrap().push("restart".into());
            Box::pin(async { Ok(json!({"restarting": true})) })
        }
        fn login_item(&self) -> Value {
            json!({"available": true, "enabled": false})
        }
        fn set_login_item(&self, enabled: bool) -> Result<Value, String> {
            self.calls.lock().unwrap().push(format!("login {enabled}"));
            Ok(json!({"enabled": enabled}))
        }
    }

    #[tokio::test]
    async fn each_action_reaches_the_apps_door_and_none_answers_without_the_app() {
        let fake = Arc::new(Fake::default());
        let doors: Option<Arc<dyn AppDoors>> = Some(fake.clone());
        assert_eq!(
            answer(doors.clone(), AppAction::Updates, None)
                .await
                .unwrap()["available"],
            true
        );
        answer(doors.clone(), AppAction::Updates, Some(false))
            .await
            .unwrap();
        answer(doors.clone(), AppAction::CheckForUpdates, None)
            .await
            .unwrap();
        answer(doors.clone(), AppAction::RestartToUpdate, None)
            .await
            .unwrap();
        assert_eq!(
            answer(doors.clone(), AppAction::LoginItem, None)
                .await
                .unwrap()["enabled"],
            false
        );
        answer(doors, AppAction::LoginItem, Some(true))
            .await
            .unwrap();
        assert_eq!(
            *fake.calls.lock().unwrap(),
            ["auto false", "check", "restart", "login true"]
        );
        assert_eq!(
            answer(None, AppAction::Updates, None).await.unwrap_err(),
            APP_ONLY
        );
    }
}
