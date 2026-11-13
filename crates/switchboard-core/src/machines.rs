//! SB-355 (DEC-0013): machine groups. Every other machine an account lives on is a group of its
//! own here: one per ssh destination, no folders, holding exactly the accounts that work there.
//! An account on a machine has that machine's pool as its home and is always marked `elsewhere`,
//! so it belongs to no group of this machine: no route, share, loan or rotation policy reaches it.
//! The group it left is kept in `returns_to`, so a call back brings it home.
use crate::projects::DEFAULT_POOL;
use crate::{append_event, Snapshot, Store, MAX_ACCOUNTS};
use serde::{Deserialize, Serialize};

/// A machine group's pool starts with this. A group of folders never takes an underscore
/// (`pool_from_name`), so the two kinds of pool never collide.
pub const MACHINE_POOL_PREFIX: &str = "machine_";
/// Most machine groups one store holds.
pub const MAX_MACHINES: usize = 64;
/// Most other names one machine is known by.
pub const MAX_ALIASES: usize = 16;

/// Refusal: an account on another machine joins a group here only by coming back.
pub const ON_ANOTHER_MACHINE: &str =
    "This account works on another machine. Call it back to use it in a group here.";
/// Refusal: an account is saved into a machine group only by a move to that machine.
pub const MACHINE_POOL_REFUSED: &str =
    "A machine group takes accounts only by a move to that machine.";
/// Refusal: no machine group by that name.
pub const MACHINE_NOT_FOUND: &str = "There is no machine group by that name.";
/// Refusal: a machine group that still holds accounts is kept.
pub const MACHINE_NOT_EMPTY: &str =
    "This machine group still holds accounts. Call them back or remove them first.";
/// Refusal: an ssh name that cannot be one.
pub const MACHINE_NAME_INVALID: &str =
    "Name the machine as ssh knows it: user@host or a Host from ~/.ssh/config.";
/// Refusal: the account lives in no machine group.
pub const NOT_ON_A_MACHINE: &str = "This account is in no machine group.";
/// Refusal: a machine group's accounts are never switched among here.
pub const MACHINE_NOT_ROTATED: &str = "A machine group's accounts are never switched here.";
/// Refusal: more machines than a store holds.
pub const MACHINES_TOO_MANY: &str = "Switchboard keeps at most 64 machine groups.";

/// One other machine: the ssh destination its accounts live on.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Machine {
    /// `machine_<name>`: the group's stable id, the home of its accounts.
    pub pool: String,
    /// The name ssh knows it by, as the window shows it: an alias from `~/.ssh/config` where one
    /// is known, else `user@host`.
    pub name: String,
    /// `user@host:port` as `ssh -G` resolved a name of it, when it could; two names with one
    /// identity are one machine.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity: Option<String>,
    /// Every other name it was reached by.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
    pub created_at: i64,
}
impl Machine {
    /// Whether `name` is this machine's name or one of its aliases (case does not matter).
    pub fn named(&self, name: &str) -> bool {
        self.name.eq_ignore_ascii_case(name)
            || self.aliases.iter().any(|a| a.eq_ignore_ascii_case(name))
    }
    /// Its names, its own first.
    pub fn names(&self) -> Vec<&str> {
        std::iter::once(self.name.as_str())
            .chain(self.aliases.iter().map(String::as_str))
            .collect()
    }
}

/// Where an account goes: the ssh name used for the machine, its `ssh -G` identity when known,
/// and other names known to be the same machine.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MachineRef {
    pub target: String,
    pub identity: Option<String>,
    pub aliases: Vec<String>,
}

pub fn is_machine_pool(pool: &str) -> bool {
    pool.starts_with(MACHINE_POOL_PREFIX)
}
/// An ssh destination as Switchboard passes it to `ssh` (`account_send::target_valid`).
pub fn machine_name_valid(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 255
        && !name.starts_with('-')
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_' | b'@' | b':'))
}
fn identity_valid(identity: &str) -> bool {
    !identity.is_empty()
        && identity.len() <= 300
        && !identity
            .chars()
            .any(|c| c.is_control() || c.is_whitespace())
}

pub(crate) fn validate_machines(s: &Snapshot) -> Result<(), String> {
    let invalid = || Err::<(), String>("Invalid machine metadata".into());
    if s.machines.len() > MAX_MACHINES || s.returns_to.len() > MAX_ACCOUNTS {
        return invalid();
    }
    let mut pools = std::collections::HashSet::new();
    for m in &s.machines {
        let mut names = std::collections::HashSet::new();
        if !is_machine_pool(&m.pool)
            || !crate::pool_valid(&m.pool)
            || !pools.insert(m.pool.as_str())
            || !machine_name_valid(&m.name)
            || m.identity.as_deref().is_some_and(|i| !identity_valid(i))
            || m.aliases.len() > MAX_ALIASES
            || m.names()
                .iter()
                .any(|n| !machine_name_valid(n) || !names.insert(n.to_ascii_lowercase()))
            || m.created_at <= 0
        {
            return invalid();
        }
    }
    if s.projects.iter().any(|p| is_machine_pool(&p.pool))
        || s.policies.iter().any(|p| is_machine_pool(&p.pool))
    {
        return invalid();
    }
    for a in &s.accounts {
        if is_machine_pool(&a.pool)
            && (s.machine_of_pool(&a.pool).is_none() || !s.elsewhere.contains_key(&a.id))
        {
            return invalid();
        }
    }
    for (id, pool) in &s.returns_to {
        let on_machine = s
            .accounts
            .iter()
            .any(|a| &a.id == id && is_machine_pool(&a.pool));
        if !on_machine || !crate::pool_valid(pool) || is_machine_pool(pool) {
            return invalid();
        }
    }
    Ok(())
}

impl Snapshot {
    /// The machine group of `pool`, if it is one.
    pub fn machine_of_pool(&self, pool: &str) -> Option<&Machine> {
        self.machines.iter().find(|m| m.pool == pool)
    }
    /// The machine group account `id` lives in.
    pub fn machine_of(&self, id: &str) -> Option<&Machine> {
        let account = self.accounts.iter().find(|a| a.id == id)?;
        self.machine_of_pool(&account.pool)
    }
    /// The machine `name` (its pool, its name or an alias) or `identity` names.
    pub fn find_machine(&self, name: &str, identity: Option<&str>) -> Option<&Machine> {
        self.machines
            .iter()
            .find(|m| m.pool == name || m.named(name))
            .or_else(|| {
                let identity = identity?;
                self.machines
                    .iter()
                    .find(|m| m.identity.as_deref() == Some(identity))
            })
    }
}

/// The pool a new machine named `name` takes: `machine_` and the name's letters and digits,
/// made unique among the pools in use.
fn new_pool(s: &Snapshot, name: &str) -> String {
    let base = format!(
        "{MACHINE_POOL_PREFIX}{}",
        crate::pool_from_name(name)
            .chars()
            .take(32 - MACHINE_POOL_PREFIX.len() - 3)
            .collect::<String>()
            .trim_end_matches('-')
    );
    let taken = |p: &str| s.machines.iter().any(|m| m.pool == p);
    let mut chosen = base.clone();
    let mut n = 2;
    while taken(&chosen) {
        chosen = format!("{base}-{n}");
        n += 1;
    }
    chosen
}

/// The machine group `at` names, made when none does yet; a name it was not known by becomes
/// an alias, and a name that is not `user@host` is preferred as its name. Answers its pool.
pub(crate) fn machine_for(s: &mut Snapshot, at: &MachineRef, now: i64) -> Result<String, String> {
    let mut names: Vec<&str> = std::iter::once(at.target.as_str())
        .chain(at.aliases.iter().map(String::as_str))
        .collect();
    names.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
    if names.iter().any(|n| !machine_name_valid(n)) {
        return Err(MACHINE_NAME_INVALID.into());
    }
    if at.identity.as_deref().is_some_and(|i| !identity_valid(i)) {
        return Err(MACHINE_NAME_INVALID.into());
    }
    let found = s
        .machines
        .iter()
        .position(|m| names.iter().any(|n| m.named(n)))
        .or_else(|| {
            let identity = at.identity.as_deref()?;
            s.machines
                .iter()
                .position(|m| m.identity.as_deref() == Some(identity))
        });
    let index = match found {
        Some(i) => i,
        None => {
            if s.machines.len() >= MAX_MACHINES {
                return Err(MACHINES_TOO_MANY.into());
            }
            // A Host alias reads better than `user@host`.
            let name = names
                .iter()
                .find(|n| !n.contains('@'))
                .unwrap_or(&names[0])
                .to_string();
            let pool = new_pool(s, &name);
            s.machines.push(Machine {
                pool,
                name,
                identity: None,
                aliases: Vec::new(),
                created_at: now,
            });
            s.machines.len() - 1
        }
    };
    let pool = s.machines[index].pool.clone();
    let m = &mut s.machines[index];
    if m.identity.is_none() {
        m.identity = at.identity.clone();
    }
    for n in names {
        if !m.named(n) && m.aliases.len() < MAX_ALIASES {
            // A friendlier name takes the lead; the old one stays an alias.
            if m.name.contains('@') && !n.contains('@') {
                let old = std::mem::replace(&mut m.name, n.to_owned());
                m.aliases.push(old);
            } else {
                m.aliases.push(n.to_owned());
            }
        }
    }
    s.machines.sort_by_key(|m| m.name.to_ascii_lowercase());
    Ok(pool)
}

/// Puts account `id` into the machine group of `pool`: it is marked used elsewhere (turned off,
/// no selection, given to no agent), its home becomes the machine's pool, the group it left is
/// kept to come back to, and every share or loan of it ends. Answers whether it moved.
pub(crate) fn place(s: &mut Snapshot, id: &str, pool: &str, now: i64, location: &str) -> bool {
    let Some(account) = s.accounts.iter_mut().find(|a| a.id == id) else {
        return false;
    };
    account.enabled = false;
    s.elsewhere.entry(id.to_owned()).or_insert(now);
    if !s.locations.contains_key(id) && crate::location_valid(location) {
        s.locations.insert(id.to_owned(), location.to_owned());
    }
    s.assigned_to.remove(id);
    s.routes.retain(|_, value| value != id);
    if account.pool == pool {
        return false;
    }
    let left = std::mem::replace(&mut account.pool, pool.to_owned());
    if !is_machine_pool(&left) {
        s.returns_to.insert(id.to_owned(), left);
    }
    crate::projects::prune_after_home_change(s);
    true
}

/// Brings account `id` home from its machine group, when it is in one: to `to` when given, else
/// the group it left, else the default group. A group that is gone, does not take it by its
/// organizations, or already holds another copy of its sign-in sends it to the default group.
/// Answers the pool it came home to.
pub(crate) fn come_home(s: &mut Snapshot, id: &str, to: Option<&str>) -> Option<String> {
    let account = s.accounts.iter().find(|a| a.id == id)?.clone();
    if !is_machine_pool(&account.pool) {
        return None;
    }
    let wanted = to
        .map(str::to_owned)
        .or_else(|| s.returns_to.get(id).cloned())
        .unwrap_or_else(|| DEFAULT_POOL.into());
    let takes = |pool: &str| {
        (pool == DEFAULT_POOL || s.project_of_pool(pool).is_some())
            && s.context_admits(pool, &account)
            && !s.accounts.iter().any(|b| {
                b.id != account.id
                    && b.provider == account.provider
                    && b.pool == pool
                    && b.external_identity
                        .as_ref()
                        .zip(account.external_identity.as_ref())
                        .is_some_and(|(x, y)| x.matches(y))
            })
    };
    let home = if takes(&wanted) {
        wanted
    } else {
        DEFAULT_POOL.to_owned()
    };
    if let Some(a) = s.accounts.iter_mut().find(|a| a.id == id) {
        a.pool = home.clone();
    }
    s.returns_to.remove(id);
    append_event(s, "machine", Some(id), "left");
    Some(home)
}

impl Store {
    /// SB-355: account `id` lives on the machine `at` names from now on: it joins that machine's
    /// group (made when there is none yet) and leaves every group here. Answers the group.
    pub fn join_machine(&self, id: &str, at: &MachineRef, now: i64) -> Result<Machine, String> {
        let mut state = self.lock()?;
        let mut candidate = state.clone();
        if !candidate.accounts.iter().any(|a| a.id == id) {
            return Err(crate::ACCOUNT_NOT_FOUND.into());
        }
        let pool = machine_for(&mut candidate, at, now)?;
        let moved = place(&mut candidate, id, &pool, now, &at.target);
        let machine = candidate
            .machine_of_pool(&pool)
            .cloned()
            .ok_or(MACHINE_NOT_FOUND)?;
        if !moved && candidate.machines == state.machines {
            // Already there, under every name it was given.
            return Ok(machine);
        }
        if moved {
            append_event(&mut candidate, "machine", Some(id), "joined");
        }
        self.publish(&mut state, candidate)?;
        self.changed();
        Ok(machine)
    }
    /// SB-355: the group account `id` comes home to when it leaves its machine group: `pool`
    /// (the default group or a group here). Refused for an account in no machine group.
    pub fn set_return(&self, id: &str, pool: &str) -> Result<(), String> {
        let mut state = self.lock()?;
        let account = state
            .accounts
            .iter()
            .find(|a| a.id == id)
            .ok_or(crate::ACCOUNT_NOT_FOUND)?;
        if !is_machine_pool(&account.pool) {
            return Err(NOT_ON_A_MACHINE.into());
        }
        if pool != DEFAULT_POOL && state.project_of_pool(pool).is_none() {
            return Err("Project not found.".into());
        }
        if state.returns_to.get(id).map(String::as_str) == Some(pool) {
            return Ok(());
        }
        let mut candidate = state.clone();
        candidate.returns_to.insert(id.to_owned(), pool.to_owned());
        self.publish(&mut state, candidate)?;
        self.changed();
        Ok(())
    }
    /// SB-355: removes a machine group that holds no account any more. `name` is its pool, its
    /// name or an alias.
    pub fn forget_machine(&self, name: &str) -> Result<Machine, String> {
        let mut state = self.lock()?;
        let machine = state
            .find_machine(name, None)
            .cloned()
            .ok_or(MACHINE_NOT_FOUND)?;
        if state.accounts.iter().any(|a| a.pool == machine.pool) {
            return Err(MACHINE_NOT_EMPTY.into());
        }
        let mut candidate = state.clone();
        candidate.machines.retain(|m| m.pool != machine.pool);
        self.publish(&mut state, candidate)?;
        self.changed();
        Ok(machine)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_machine_takes_its_alias_as_name_and_a_pool_no_group_takes() {
        let mut s = Snapshot::default();
        let at = MachineRef {
            target: "sshlg@100.69.20.19".into(),
            identity: Some("sshlg@100.69.20.19:22".into()),
            aliases: vec!["studio-sshlg".into()],
        };
        let pool = machine_for(&mut s, &at, 10).unwrap();
        assert_eq!(pool, "machine_studio-sshlg");
        let m = s.machine_of_pool(&pool).unwrap();
        assert_eq!(m.name, "studio-sshlg");
        assert_eq!(m.aliases, ["sshlg@100.69.20.19"]);
        // The same machine by its identity alone, under a new name, is the same group.
        let again = MachineRef {
            target: "studio".into(),
            identity: Some("sshlg@100.69.20.19:22".into()),
            aliases: vec![],
        };
        assert_eq!(machine_for(&mut s, &again, 11).unwrap(), pool);
        assert_eq!(s.machines.len(), 1);
        assert!(s.machines[0].named("STUDIO"));
        // Another user on the same host is another machine.
        let other = MachineRef {
            target: "passioncode@100.69.20.19".into(),
            identity: Some("passioncode@100.69.20.19:22".into()),
            aliases: vec![],
        };
        let second = machine_for(&mut s, &other, 12).unwrap();
        assert_eq!(second, "machine_passioncode-100-69-20");
        assert!(crate::pool_valid(&second));
        assert!(crate::pool_from_name("machine_x") != "machine_x");
    }

    #[test]
    fn a_name_ssh_would_not_take_is_refused() {
        let mut s = Snapshot::default();
        for bad in ["", "-oProxyCommand=x", "a b", "studio;rm"] {
            let at = MachineRef {
                target: bad.into(),
                ..Default::default()
            };
            assert_eq!(
                machine_for(&mut s, &at, 1),
                Err(MACHINE_NAME_INVALID.into())
            );
        }
        assert!(s.machines.is_empty());
    }
}
