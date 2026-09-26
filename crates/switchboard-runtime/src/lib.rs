//! One store/proxy/control owner shared by the desktop and CLI.
pub mod control;
pub mod launch;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use switchboard_core::{Account, AuthKind, Credential, NativeVault, Provider, Store, Vault};
use switchboard_proxy::ProxyHandle;

pub const APP_ID: &str = "ai.passioncode.fabric-switchboard";
pub fn default_root() -> Result<PathBuf, String> {
    #[cfg(windows)]
    let directory = dirs::data_local_dir();
    #[cfg(not(windows))]
    let directory = dirs::data_dir();
    directory.map(|p| p.join(APP_ID)).ok_or_else(|| {
        "App-data directory unavailable. Use --data-dir with an absolute private directory.".into()
    })
}

/// No credential retrieval operation exists. Never derive Debug on this type.
#[derive(Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum Operation {
    Snapshot,
    Status,
    Add {
        label: String,
        provider: Provider,
        kind: AuthKind,
        pool: String,
        secret: String,
    },
    Update {
        id: String,
        label: String,
        enabled: bool,
    },
    Remove {
        id: String,
    },
    Select {
        provider: Provider,
        pool: String,
        id: String,
    },
    Usage {
        id: String,
    },
    Launch {
        id: String,
        mode: String,
        working_directory: PathBuf,
    },
    BeginLogin {
        provider: Provider,
        label: String,
        pool: String,
    },
    FinishLogin {
        login_id: String,
    },
    CancelLogin {
        login_id: String,
    },
}

pub struct Runtime {
    pub store: Arc<Store>,
    pub proxy: ProxyHandle,
    pub root: PathBuf,
    logins: Mutex<HashMap<String, launch::Login>>,
    mutations: tokio::sync::Mutex<()>,
}
impl Runtime {
    pub async fn open(root: PathBuf, vault: Arc<dyn Vault>) -> Result<Arc<Self>, String> {
        let store = Arc::new(Store::open(root.clone(), vault)?);
        let proxy = ProxyHandle::start(store.clone()).await?;
        Ok(Arc::new(Self {
            store,
            proxy,
            root,
            logins: Mutex::new(HashMap::new()),
            mutations: tokio::sync::Mutex::new(()),
        }))
    }
    pub async fn execute(&self, operation: Operation) -> Result<Value, String> {
        // The Store mutex protects metadata, but launch and removal also mutate
        // private homes. Keep the complete operation in one owner transaction.
        let _mutation = self.mutations.lock().await;
        execute(self.store.clone(), &self.root, Some(self), operation).await
    }
    fn begin_login(
        &self,
        provider: Provider,
        label: String,
        pool: String,
    ) -> Result<Value, String> {
        let mut logins = self
            .logins
            .lock()
            .map_err(|_| "Sign-in state unavailable.")?;
        if logins.len() >= 4 {
            return Err("Finish an existing sign-in before starting another.".into());
        }
        let login = launch::begin_login(&self.root, provider, label, pool)?;
        let id = login.id.clone();
        logins.insert(id.clone(), login);
        Ok(
            json!({"login_id": id, "message": "Finish official sign-in in Terminal, then finish sign-in in Switchboard."}),
        )
    }
    fn finish_login(&self, id: &str) -> Result<Account, String> {
        let mut logins = self
            .logins
            .lock()
            .map_err(|_| "Sign-in state unavailable.")?;
        let login = logins
            .get_mut(id)
            .ok_or("Sign-in not found. Start again.")?;
        let account = if let Some(account) = &login.saved {
            account.clone()
        } else {
            let credential = launch::capture_login(login)?;
            let account = self.store.add(
                login.label.clone(),
                login.provider,
                AuthKind::OAuth,
                login.pool.clone(),
                credential,
            )?;
            login.saved = Some(account.clone());
            account
        };
        launch::clean_login(login)?;
        logins.remove(id);
        Ok(account)
    }
    fn cancel_login(&self, id: &str) -> Result<(), String> {
        let mut logins = self
            .logins
            .lock()
            .map_err(|_| "Sign-in state unavailable.")?;
        launch::cancel_login(logins.get(id).ok_or("Sign-in not found. Start again.")?)?;
        logins.remove(id);
        Ok(())
    }
}

/// Holding this value keeps both listeners and the exclusive store lease alive.
pub struct Owner {
    pub runtime: Arc<Runtime>,
    _control: control::ControlHandle,
}
impl Owner {
    pub async fn start(root: PathBuf, vault: Arc<dyn Vault>) -> Result<Self, String> {
        let runtime = Runtime::open(root, vault).await?;
        let control = control::ControlHandle::start(runtime.clone()).await?;
        Ok(Self {
            runtime,
            _control: control,
        })
    }
    pub async fn native(root: PathBuf) -> Result<Self, String> {
        Self::start(root, Arc::new(NativeVault::new())).await
    }
}

pub async fn execute_offline(root: PathBuf, operation: Operation) -> Result<Value, String> {
    let store = Arc::new(Store::open(root.clone(), Arc::new(NativeVault::new()))?);
    execute(store, &root, None, operation).await
}
async fn execute(
    store: Arc<Store>,
    root: &Path,
    runtime: Option<&Runtime>,
    operation: Operation,
) -> Result<Value, String> {
    let needs_owner = || {
        runtime.ok_or_else(|| {
            "Start the desktop app or 'switchboard serve' before login or launch.".to_string()
        })
    };
    match operation {
        Operation::Snapshot => {
            serde_json::to_value(store.snapshot()?).map_err(|_| "Snapshot unavailable.".into())
        }
        Operation::Status => Ok(
            json!({"proxy_address": runtime.map(|r| r.proxy.address().to_string()), "platform": std::env::consts::OS, "live_mode": if runtime.is_some() { "Next request · HTTP/SSE" } else { "Offline" }}),
        ),
        Operation::Add {
            label,
            provider,
            kind,
            pool,
            secret,
        } => {
            let credential = Credential::parse(provider, kind, &secret)?;
            Ok(json!(store.add(label, provider, kind, pool, credential)?))
        }
        Operation::Update { id, label, enabled } => {
            store.update(&id, label, enabled)?;
            Ok(Value::Null)
        }
        Operation::Remove { id } => {
            if store.snapshot()?.routes.values().any(|v| v == &id) {
                return Err("Select another account before removing this one.".into());
            }
            launch::clean_account(root, &id)?;
            store.remove(&id)?;
            Ok(Value::Null)
        }
        Operation::Select { provider, pool, id } => {
            store.select(provider, &pool, &id)?;
            Ok(Value::Null)
        }
        Operation::Usage { id } => Ok(json!(switchboard_proxy::probe_usage(store, id).await?)),
        Operation::Launch {
            id,
            mode,
            working_directory,
        } => {
            let runtime = needs_owner()?;
            launch::launch(root, &store, &runtime.proxy, &id, &mode, &working_directory)?;
            Ok(
                json!({"message": "Terminal launch requested. Provider response is not yet verified."}),
            )
        }
        Operation::BeginLogin {
            provider,
            label,
            pool,
        } => needs_owner()?.begin_login(provider, label, pool),
        Operation::FinishLogin { login_id } => Ok(json!(needs_owner()?.finish_login(&login_id)?)),
        Operation::CancelLogin { login_id } => {
            needs_owner()?.cancel_login(&login_id)?;
            Ok(Value::Null)
        }
    }
}

#[cfg(test)]
mod owner_tests {
    use super::*;
    use switchboard_core::{private_fs, MemoryVault};
    #[tokio::test]
    async fn home_cleanup_waits_for_the_owners_launch_reservation() {
        let root = tempfile::tempdir().unwrap();
        let runtime = Runtime::open(root.path().to_owned(), Arc::new(MemoryVault::default()))
            .await
            .unwrap();
        let account = runtime
            .store
            .add(
                "Fixture".into(),
                Provider::Claude,
                AuthKind::ApiKey,
                "work".into(),
                Credential::parse(Provider::Claude, AuthKind::ApiKey, "fixture-only").unwrap(),
            )
            .unwrap();
        let home = root.path().join("homes").join(&account.id);
        private_fs::private_dir(&home).unwrap();
        // Model a launch that has entered the owner transaction but has not yet
        // written its reservation. Removal must not inspect/delete this home.
        let transaction = runtime.mutations.lock().await;
        let removing = runtime.clone();
        let id = account.id.clone();
        let mut remove =
            tokio::spawn(async move { removing.execute(Operation::Remove { id }).await });
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(20), &mut remove)
                .await
                .is_err()
        );
        assert!(home.is_dir());
        private_fs::private_write(&home.join(".launch-pending"), b"fixture launch").unwrap();
        drop(transaction);
        assert!(remove.await.unwrap().is_err());
        assert!(home.is_dir());
        assert_eq!(runtime.store.snapshot().unwrap().accounts.len(), 1);
    }
}
