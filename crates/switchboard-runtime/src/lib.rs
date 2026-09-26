//! One store/proxy/control owner shared by the desktop and CLI.
pub mod control;
pub mod external;
#[cfg(target_os = "macos")]
mod external_keychain;
pub mod launch;
mod monitor;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use switchboard_core::{
    Account, AuthKind, Credential, NativeVault, Provider, RotationPolicy, Store, Vault,
};
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
    CurrentAccounts,
    CaptureCurrent {
        provider: Provider,
        label: Option<String>,
        pool: String,
    },
    ImportClaudeSwap {
        pool: String,
    },
    ActivateNative {
        id: String,
    },
    SetPolicy {
        policy: RotationPolicy,
    },
    MonitorStatus,
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
    current_cache: Mutex<Option<(i64, Value)>>,
    monitor_decisions: Mutex<Vec<Value>>,
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
            current_cache: Mutex::new(None),
            monitor_decisions: Mutex::new(Vec::new()),
        }))
    }
    pub async fn execute(&self, operation: Operation) -> Result<Value, String> {
        // Metadata reads do not wait behind OS credential prompts or network probes.
        // Store snapshot has its own lock; these operations cannot change auth or homes.
        if matches!(
            operation,
            Operation::Snapshot | Operation::Status | Operation::MonitorStatus
        ) {
            return execute(self.store.clone(), &self.root, Some(self), operation).await;
        }
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
            let captured = launch::capture_login_profile(login)?;
            let account = self.store.upsert(
                login.label.clone(),
                login.provider,
                AuthKind::OAuth,
                login.pool.clone(),
                captured.credential,
                Some(captured.identity),
            )?;
            login.saved = Some(account.clone());
            account
        };
        launch::clean_login(login)?;
        logins.remove(id);
        self.invalidate_current();
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
    fn invalidate_current(&self) {
        if let Ok(mut cache) = self.current_cache.lock() {
            *cache = None;
        }
    }
    fn current_accounts(&self) -> Result<Value, String> {
        let mut cache = self
            .current_cache
            .lock()
            .map_err(|_| "Current account unavailable.")?;
        let now = monitor::now();
        if let Some((at, value)) = cache.as_ref() {
            if now >= *at && now - at < 30 {
                return Ok(value.clone());
            }
        }
        let value = observe_current(&self.store);
        *cache = Some((now, value.clone()));
        Ok(value)
    }
}

/// Holding this value keeps both listeners and the exclusive store lease alive.
pub struct Owner {
    pub runtime: Arc<Runtime>,
    _control: control::ControlHandle,
    _monitor: monitor::MonitorHandle,
}
impl Owner {
    pub async fn start(root: PathBuf, vault: Arc<dyn Vault>) -> Result<Self, String> {
        Self::start_with_sources(root, vault, false).await
    }
    async fn start_with_sources(
        root: PathBuf,
        vault: Arc<dyn Vault>,
        native_sources: bool,
    ) -> Result<Self, String> {
        let runtime = Runtime::open(root, vault).await?;
        let control = control::ControlHandle::start(runtime.clone()).await?;
        let monitor = monitor::MonitorHandle::start(&runtime, native_sources);
        Ok(Self {
            runtime,
            _control: control,
            _monitor: monitor,
        })
    }
    pub async fn native(root: PathBuf) -> Result<Self, String> {
        Self::start_with_sources(root, Arc::new(NativeVault::new()), true).await
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
        Operation::CurrentAccounts => match runtime {
            Some(runtime) => runtime.current_accounts(),
            None => Ok(observe_current(&store)),
        },
        Operation::CaptureCurrent {
            provider,
            label,
            pool,
        } => {
            let captured = external::capture_current(provider)?;
            let account = store.upsert(
                label.unwrap_or(captured.label),
                provider,
                captured.kind,
                pool,
                captured.credential,
                Some(captured.identity),
            )?;
            if let Some(runtime) = runtime {
                runtime.invalidate_current();
            }
            Ok(json!(account))
        }
        Operation::ImportClaudeSwap { pool } => {
            // Validate before reading external material, even for an empty import.
            if pool.is_empty()
                || pool.len() > 32
                || !pool
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
            {
                return Err("Label or pool is invalid".into());
            }
            let batch = external::read_claude_swap()?;
            let mut imported = Vec::new();
            let mut failed = batch.failed;
            for profile in batch.profiles {
                match store.upsert(
                    profile.label,
                    profile.provider,
                    profile.kind,
                    pool.clone(),
                    profile.credential,
                    Some(profile.identity),
                ) {
                    Ok(account) => imported.push(account),
                    Err(_) => failed += 1,
                }
            }
            if let Some(runtime) = runtime {
                runtime.invalidate_current();
            }
            Ok(json!({"imported": imported, "failed": failed, "skipped": batch.skipped}))
        }
        Operation::ActivateNative { id } => {
            activate_native(&store, &id, None)?;
            if let Some(runtime) = runtime {
                runtime.invalidate_current();
            }
            Ok(Value::Null)
        }
        Operation::SetPolicy { policy } => {
            store.set_policy(policy)?;
            Ok(Value::Null)
        }
        Operation::MonitorStatus => Ok(json!({
            "running": runtime.is_some(), "interval_seconds": monitor::INTERVAL_SECONDS,
            "decisions": runtime.and_then(|r| r.monitor_decisions.lock().ok().map(|v| v.clone())).unwrap_or_default()
        })),
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
            store.select_with_cooldown(provider, &pool, &id, monitor::now())?;
            Ok(Value::Null)
        }
        Operation::Usage { id } => Ok(json!(monitor::probe(store, &id).await?)),
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

fn observe_current(store: &Store) -> Value {
    let mut output = serde_json::Map::new();
    for provider in [Provider::Claude, Provider::Codex] {
        let value = match external::capture_current(provider) {
            Ok(profile) => match store.match_external(provider, "default", &profile.identity) {
                Ok(account) => {
                    json!({"status":"available", "identity":profile.identity, "account_id":account.map(|a| a.id)})
                }
                Err(_) => json!({"status":"unavailable", "identity":null, "account_id":null}),
            },
            Err(error) => {
                json!({"status": if error.starts_with("No current ") { "missing" } else { "unavailable" }, "identity":null, "account_id":null})
            }
        };
        output.insert(provider.as_str().into(), value);
    }
    Value::Object(output)
}

fn activate_native(store: &Store, id: &str, expected_id: Option<&str>) -> Result<(), String> {
    let account = store
        .snapshot()?
        .accounts
        .into_iter()
        .find(|a| a.id == id)
        .ok_or("Account not found")?;
    if account.provider != Provider::Claude || account.kind != AuthKind::OAuth {
        return Err("Native activation requires a Claude OAuth profile.".into());
    }
    let identity = account
        .external_identity
        .as_ref()
        .ok_or("Capture or import this profile before native activation.")?;
    let credential = store.credential(id)?;
    let current = external::capture_current(Provider::Claude)?;
    let previous = store.match_external(Provider::Claude, &account.pool, &current.identity)?;
    if expected_id.is_some_and(|expected| previous.as_ref().is_none_or(|a| a.id != expected)) {
        return Err("Current CLI account changed. Refresh before switching.".into());
    }
    if let Some(previous) = previous {
        // Preserve the live client's newest refresh generation before replacing it.
        store.upsert(
            previous.label,
            previous.provider,
            previous.kind,
            previous.pool,
            current.credential,
            Some(current.identity.clone()),
        )?;
    }
    external::activate_claude(&credential, identity, Some(&current.identity))?;
    if store.snapshot()?.policies.iter().any(|p| {
        p.provider == Provider::Claude && p.pool == account.pool && p.target == "claude_cli"
    }) {
        store.mark_rotated(
            Provider::Claude,
            &account.pool,
            "claude_cli",
            monitor::now(),
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod owner_tests {
    use super::*;
    use switchboard_core::{private_fs, MemoryVault};
    #[tokio::test]
    async fn workbench_metadata_stays_available_during_a_reserved_mutation() {
        let root = tempfile::tempdir().unwrap();
        let runtime = Runtime::open(root.path().to_owned(), Arc::new(MemoryVault::default()))
            .await
            .unwrap();
        let _reservation = runtime.mutations.lock().await;
        for operation in [
            Operation::Snapshot,
            Operation::Status,
            Operation::MonitorStatus,
        ] {
            let response = tokio::time::timeout(
                std::time::Duration::from_millis(100),
                runtime.execute(operation),
            )
            .await;
            assert!(response.unwrap().is_ok());
        }
    }
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
