//! SB-352 (DEC-0012, operator 2026-11-13): accounts lent between the groups the person chose,
//! by their last hours and days. Off by default; `<data>/balancing.json` (0600) holds the
//! chosen groups (`groups`, empty = off), whether a group made later joins them (`all_new`),
//! the groups already known (`seen`), the accounts lent now (`lent`) and when each account last
//! moved (`moved`). Every lend and return is one line of `<data>/balancing.jsonl` (0600, the
//! newest 1000 lines): time, account id, from, to, action, reason, outcome and the figures that
//! decided it; ids and counts only.
//!
//! Lending never moves an account: it serves the borrower as a shared account (SB-282) and keeps
//! its home group, so taking it back is removing that share. A pass runs every 15 minutes while
//! groups are chosen (`Runtime::balance_pass`); `plan` is the whole decision and reads nothing.
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::Path;
use switchboard_core::{Account, AuthKind, Provider, Snapshot, Store};

pub const FILE: &str = "balancing.json";
pub const JOURNAL: &str = "balancing.jsonl";
/// How often the background pass decides.
pub const PASS_SECONDS: i64 = 15 * 60;
/// The recent stretch a group's demand and refusals are read over.
pub const RECENT_SECONDS: i64 = 6 * 3600;
/// A lent account goes back after this long with no limit refusal in the borrower.
pub const QUIET_SECONDS: i64 = 2 * 3600;
/// An account lent or returned waits this long before it moves again.
pub const COOLDOWN_SECONDS: i64 = 3600;
/// The days a group's usual demand is averaged over.
pub const AVERAGE_DAYS: i64 = 7;
/// Lines kept in the journal.
const KEEP_LINES: usize = 1000;
const MAX_BYTES: u64 = 256 * 1024;
const MAX_JOURNAL: u64 = 4 * 1024 * 1024;
/// The pool of the accounts in no group a person made.
pub const DEFAULT_POOL: &str = "default";
const PROVIDERS: [Provider; 2] = [Provider::Claude, Provider::Codex];

pub const UNREADABLE: &str = "Balancing settings (balancing.json) could not be read. Delete balancing.json to turn balancing off and start over.";
pub const NO_SUCH_GROUP: &str =
    "No group has that id. Run switchboard project list to see the groups, or name default.";
/// DEC-0016: the default group holds every folder in no group, so it can never only lend.
pub const BACKUP_DEFAULT: &str =
    "The default group cannot be a backup group: it holds every folder that is in no group.";
pub const NOT_SAVED: &str =
    "Balancing settings could not be saved. Check that the data folder is writable, then try again.";

/// Unknown fields are ignored, so a file a newer Switchboard wrote still reads here.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    /// The chosen groups' pool ids; empty is off.
    #[serde(default)]
    pub groups: Vec<String>,
    /// A group made later joins the chosen ones by itself (operator: «и все новые»).
    #[serde(default)]
    pub all_new: bool,
    /// Every group that existed when the person last chose, or that a pass has seen since: a
    /// group not in it is new.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub seen: Vec<String>,
    /// The accounts balancing lent and has not taken back.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lent: Vec<Lend>,
    /// When each account was last lent or returned (the cooldown).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub moved: BTreeMap<String, i64>,
    /// WF-6, DEC-0016: chosen groups that only lend. A backup group never borrows, keeps no
    /// minimum and is asked first; its own folders still come first (`home_needs`). Kept here,
    /// not in `accounts.json`, so an older Switchboard reads the store unchanged (it reads such
    /// a group as an ordinary chosen one).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub backup: Vec<String>,
}
impl Settings {
    pub fn on(&self) -> bool {
        !self.groups.is_empty()
    }
    fn chosen(&self, pool: &str) -> bool {
        self.groups.iter().any(|g| g == pool)
    }
    pub fn is_backup(&self, pool: &str) -> bool {
        self.backup.iter().any(|g| g == pool)
    }
    fn lent_out(&self, id: &str) -> bool {
        self.lent.iter().any(|l| l.account == id)
    }
    fn cooled(&self, id: &str, now: i64) -> bool {
        self.moved
            .get(id)
            .is_none_or(|t| *t > now || now - t >= COOLDOWN_SECONDS)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lend {
    pub account: String,
    pub provider: Provider,
    /// Its home group.
    pub from: String,
    /// The group it serves on loan.
    pub to: String,
    pub at: i64,
}

pub fn read(root: &Path) -> Result<Settings, String> {
    let path = root.join(FILE);
    match std::fs::symlink_metadata(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Settings::default()),
        Err(_) => return Err(UNREADABLE.into()),
        Ok(_) => {}
    }
    let bytes =
        switchboard_core::private_fs::read_private(&path, MAX_BYTES).map_err(|_| UNREADABLE)?;
    serde_json::from_slice(&bytes).map_err(|_| UNREADABLE.into())
}
pub fn write(root: &Path, settings: &Settings) -> Result<(), String> {
    let bytes = serde_json::to_vec(settings).map_err(|_| NOT_SAVED)?;
    switchboard_core::private_fs::private_write(&root.join(FILE), &bytes)
        .map_err(|_| NOT_SAVED.into())
}

/// Every group balancing can name: `default`, then each group a person made.
pub fn pools(snapshot: &Snapshot) -> Vec<String> {
    std::iter::once(DEFAULT_POOL.to_owned())
        .chain(snapshot.projects.iter().map(|p| p.pool.clone()))
        .collect()
}

/// Brings the settings in step with the groups that exist: a deleted group leaves them, and while
/// balancing is on with `all_new`, a group not seen before joins the chosen ones. Answers whether
/// anything changed.
pub fn resolve(settings: &mut Settings, snapshot: &Snapshot) -> bool {
    let before = settings.clone();
    let pools = pools(snapshot);
    settings.groups.retain(|g| pools.contains(g));
    settings.seen.retain(|g| pools.contains(g));
    // DEC-0016: a backup group that left the chosen ones lends no more.
    let groups = settings.groups.clone();
    settings.backup.retain(|g| groups.contains(g));
    if settings.on() {
        for pool in &pools {
            if !settings.seen.contains(pool) {
                if settings.all_new && !settings.chosen(pool) {
                    settings.groups.push(pool.clone());
                }
                settings.seen.push(pool.clone());
            }
        }
    }
    *settings != before
}

/// A group's figures for one provider.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
pub struct Signal {
    /// Requests in the last 6 hours.
    pub requests_6h: u64,
    /// Requests the provider refused for a limit in the last 6 hours.
    pub refusals_6h: u64,
    /// The same in the last 2 hours.
    pub refusals_2h: u64,
    /// Requests per hour over the 7 days before today.
    pub hourly_7d: f64,
}
impl Signal {
    /// The last 6 hours' requests per hour against the 7-day average (1 is a usual day).
    pub fn demand(&self) -> f64 {
        let recent = self.requests_6h as f64 / (RECENT_SECONDS as f64 / 3600.0);
        if self.hourly_7d > 0.0 {
            recent / self.hourly_7d
        } else if recent > 0.0 {
            recent
        } else {
            0.0
        }
    }
}
/// `(pool, provider)` → its figures.
pub type Signals = BTreeMap<(String, &'static str), Signal>;

/// The figures per group and provider from the counts per group and account: the last 6 hours,
/// the last 2 hours and the 7 days before today. A count whose account is not saved any more is
/// left out.
pub fn signals(
    snapshot: &Snapshot,
    six: &crate::statistics::Window,
    two: &crate::statistics::Window,
    week: &crate::statistics::Window,
) -> Signals {
    let provider = |id: &str| {
        snapshot
            .accounts
            .iter()
            .find(|a| a.id == id)
            .map(|a| a.provider.as_str())
    };
    let mut out = Signals::new();
    for ((pool, account), (requests, limited)) in six {
        if let Some(p) = provider(account) {
            let s = out.entry((pool.clone(), p)).or_default();
            s.requests_6h += requests;
            s.refusals_6h += limited;
        }
    }
    for ((pool, account), (_, limited)) in two {
        if let Some(p) = provider(account) {
            out.entry((pool.clone(), p)).or_default().refusals_2h += limited;
        }
    }
    for ((pool, account), (requests, _)) in week {
        if let Some(p) = provider(account) {
            out.entry((pool.clone(), p)).or_default().hourly_7d +=
                *requests as f64 / (AVERAGE_DAYS * 24) as f64;
        }
    }
    out
}

/// What the runtime knows of the accounts beyond the snapshot.
#[derive(Clone, Debug, Default)]
pub struct Held {
    /// Accounts the ordinary Claude Code or Codex is signed in to.
    pub in_cli: HashSet<String>,
    /// Accounts whose sign-in waits for a person.
    pub sign_in: HashSet<String>,
    /// Accounts a provider limit holds right now.
    pub limited: HashSet<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Figures {
    /// The borrower's figures.
    pub borrower: Signal,
    /// Accounts serving the borrower with room.
    pub borrower_room: usize,
    /// The lender's figures (a lend only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lender: Option<Signal>,
    /// The lender's own accounts with room, the lent one included (a lend only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lender_room: Option<usize>,
    /// The account's used share (a lend only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub used_percent: Option<f64>,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Action {
    /// `lend` or `return`.
    pub action: &'static str,
    pub account: String,
    pub provider: Provider,
    pub from: String,
    pub to: String,
    /// `need` for a lend; for a return `not_chosen`, `not_lendable`, `home_needs`, `quiet`,
    /// `account_gone` or `share_gone`.
    pub reason: &'static str,
    /// Whether the group changes; false only for a loan already ended outside balancing.
    pub edit: bool,
    pub figures: Figures,
}

/// The used share below which an account of `provider` has room in `pool`, and how fresh its
/// figures must be: the group's managed switching policy (threshold less hysteresis, its
/// `max_age_seconds`), or the initial policy's figures when none is saved (as SB-345 reads it).
fn limits(snapshot: &Snapshot, provider: Provider, pool: &str) -> (f64, i64) {
    let policy = snapshot
        .policies
        .iter()
        .find(|p| p.provider == provider && p.pool == pool && p.target == "managed")
        .cloned()
        .unwrap_or_else(|| switchboard_core::default_managed_policy(provider, pool));
    (
        (policy.threshold_percent - policy.hysteresis_percent).clamp(1.0, 100.0),
        policy.max_age_seconds,
    )
}
/// The account's used share when it has room in `pool` on fresh figures; none when its figures
/// are stale, unknown, failed, or past a reset, or when a limit or a sign-in holds it.
pub fn room(
    snapshot: &Snapshot,
    account: &Account,
    pool: &str,
    held: &Held,
    now: i64,
) -> Option<f64> {
    if held.limited.contains(&account.id) || held.sign_in.contains(&account.id) {
        return None;
    }
    let (limit, max_age) = limits(snapshot, account.provider, pool);
    let usage = account.usage.as_ref()?;
    let health = account.usage_health.as_ref()?;
    let partial_headers = usage.source == "response_headers"
        && !["five_hour", "seven_day"]
            .iter()
            .all(|name| usage.windows.iter().any(|w| w.name == *name));
    if partial_headers
        || health.status != "ok"
        || health.checked_at < usage.observed_at
        || health.checked_at > now
        || now - health.checked_at > max_age
        || usage.observed_at > now
        || now - usage.observed_at > max_age
        || (usage.windows.is_empty() && usage.resets_at.is_some_and(|t| t <= now))
        || usage
            .windows
            .iter()
            .any(|w| !w.is_feature() && w.resets_at.is_some_and(|t| t <= now))
    {
        return None;
    }
    usage
        .account_used_percent()
        .filter(|used| used.is_finite() && *used < limit)
}
/// An account balancing may count or move at all: a subscription sign-in, enabled, not used on
/// another machine and not given to an agent.
fn usable(snapshot: &Snapshot, account: &Account) -> bool {
    account.enabled
        && account.kind == AuthKind::OAuth
        && !snapshot.elsewhere.contains_key(&account.id)
        && !snapshot.is_assigned(&account.id)
}
/// The latest reset among the account's own windows, to prefer the one whose room lasts longer.
fn latest_reset(account: &Account) -> i64 {
    account.usage.as_ref().map_or(0, |u| {
        u.windows
            .iter()
            .filter(|w| !w.is_feature())
            .filter_map(|w| w.resets_at)
            .chain(u.resets_at)
            .max()
            .unwrap_or(0)
    })
}

struct View<'a> {
    snapshot: &'a Snapshot,
    signals: &'a Signals,
    held: &'a Held,
    settings: &'a Settings,
    now: i64,
}
impl View<'_> {
    fn signal(&self, pool: &str, provider: Provider) -> Signal {
        self.signals
            .get(&(pool.to_owned(), provider.as_str()))
            .copied()
            .unwrap_or_default()
    }
    /// Accounts of `provider` serving `pool` with room (its own and those shared with it).
    fn rooms(&self, pool: &str, provider: Provider) -> usize {
        self.snapshot
            .pool_accounts(pool)
            .into_iter()
            .filter(|a| a.provider == provider && usable(self.snapshot, a))
            .filter(|a| room(self.snapshot, a, pool, self.held, self.now).is_some())
            .count()
    }
    /// Accounts of `provider` serving `pool` with room that are not on loan to it: its own
    /// accounts and those it shares by the person's choice (DEC-0016's return rule).
    fn rooms_of_its_own(&self, pool: &str, provider: Provider) -> usize {
        self.snapshot
            .pool_accounts(pool)
            .into_iter()
            .filter(|a| a.provider == provider && usable(self.snapshot, a))
            .filter(|a| {
                !self
                    .settings
                    .lent
                    .iter()
                    .any(|l| l.account == a.id && l.to == pool)
            })
            .filter(|a| room(self.snapshot, a, pool, self.held, self.now).is_some())
            .count()
    }
    /// A group needs room when it had requests in the last 6 hours and none of its accounts has
    /// room, or one while it had limit refusals.
    fn needs(&self, pool: &str, provider: Provider) -> bool {
        let s = self.signal(pool, provider);
        let rooms = self.rooms(pool, provider);
        s.requests_6h > 0 && (rooms == 0 || (rooms == 1 && s.refusals_6h > 0))
    }
    /// The lender's own accounts of `provider` with room that are not lent out now.
    fn own_rooms(&self, pool: &str, provider: Provider) -> usize {
        self.snapshot
            .accounts
            .iter()
            .filter(|a| a.pool == pool && a.provider == provider && usable(self.snapshot, a))
            .filter(|a| !self.settings.lent_out(&a.id))
            .filter(|a| room(self.snapshot, a, pool, self.held, self.now).is_some())
            .count()
    }
}

/// The returns owed whatever the figures: a loan whose group is not chosen any more, whose
/// account cannot be lent now, or that ended outside balancing.
fn forced_return(view: &View<'_>, lend: &Lend) -> Option<(&'static str, bool)> {
    let snapshot = view.snapshot;
    let Some(account) = snapshot.accounts.iter().find(|a| a.id == lend.account) else {
        return Some(("account_gone", false));
    };
    // The borrower serves it only as a share (its home is elsewhere): the default group by its
    // loans (`default_shared`), any other group by its `shared_accounts` (SB-282).
    if account.pool == lend.to || !snapshot.serves(account, &lend.to) {
        return Some(("share_gone", false));
    }
    if !view.settings.chosen(&lend.from) || !view.settings.chosen(&lend.to) {
        return Some(("not_chosen", true));
    }
    // Wave 2: an account the person moved to another home since is no loan of `from` any more.
    if account.pool != lend.from
        || !usable(snapshot, account)
        || view.held.in_cli.contains(&account.id)
        || !snapshot.loan_admits(&lend.to, account)
    {
        return Some(("not_lendable", true));
    }
    None
}
fn returning(lend: &Lend, reason: &'static str, edit: bool, figures: Figures) -> Action {
    Action {
        action: "return",
        account: lend.account.clone(),
        provider: lend.provider,
        from: lend.from.clone(),
        to: lend.to.clone(),
        reason,
        edit,
        figures,
    }
}

/// Only the returns a change of the chosen groups owes at once (turning balancing off returns
/// every lent account), with no figures read.
pub fn returns_owed(
    snapshot: &Snapshot,
    held: &Held,
    settings: &Settings,
    now: i64,
) -> Vec<Action> {
    let signals = Signals::new();
    let view = View {
        snapshot,
        signals: &signals,
        held,
        settings,
        now,
    };
    settings
        .lent
        .iter()
        .filter_map(|lend| {
            forced_return(&view, lend).map(|(r, e)| returning(lend, r, e, Figures::default()))
        })
        .collect()
}

/// The whole decision of one pass (DEC-0012). Returns first: a loan whose group is not chosen,
/// whose account cannot be lent now or that ended outside balancing goes back at once; after its
/// cooldown, one whose home group needs it, or whose borrower had no limit refusal for 2 hours
/// since it was lent at least 2 hours ago. Then at most one lend: the chosen groups that need
/// room, most refusals first, then most demand, each take in turn the lender account with the
/// most room (lowest used share, then the latest reset) from a chosen group that keeps at least
/// two accounts with room after it (one when it had no request for 6 hours). A group not chosen
/// is never read or touched; an account never crosses an organization's boundary (the default
/// group, no organization's, takes only a personal account), and the ordinary CLI's account, one
/// given to an agent, used elsewhere, disabled, waiting for a sign-in or on stale figures is
/// never lent. Since wave 2 `default` lends and borrows like any chosen group: its loans are kept
/// in the store's `default_shared` and `default_lent` (SB-352 wave 2).
pub fn plan(
    snapshot: &Snapshot,
    signals: &Signals,
    held: &Held,
    settings: &Settings,
    now: i64,
) -> Vec<Action> {
    let view = View {
        snapshot,
        signals,
        held,
        settings,
        now,
    };
    let mut out = Vec::new();
    let mut moving: HashSet<String> = HashSet::new();
    for lend in &settings.lent {
        if let Some((reason, edit)) = forced_return(&view, lend) {
            out.push(returning(lend, reason, edit, Figures::default()));
            moving.insert(lend.account.clone());
            continue;
        }
        if !settings.cooled(&lend.account, now) {
            continue;
        }
        let borrower = view.signal(&lend.to, lend.provider);
        let figures = Figures {
            borrower,
            borrower_room: view.rooms(&lend.to, lend.provider),
            ..Figures::default()
        };
        let reason = if view.needs(&lend.from, lend.provider) {
            Some("home_needs")
        } else if settings.is_backup(&lend.from)
            && view.rooms_of_its_own(&lend.to, lend.provider) > 0
        {
            // DEC-0016: a backup account goes home once the group has room of its own again.
            Some("own_room")
        } else if now - lend.at >= QUIET_SECONDS && borrower.refusals_2h == 0 {
            Some("quiet")
        } else {
            None
        };
        if let Some(reason) = reason {
            out.push(returning(lend, reason, true, figures));
            moving.insert(lend.account.clone());
        }
    }
    if !settings.on() {
        return out;
    }
    // A chosen group that still exists: `default` always does (wave 2).
    let groups: Vec<&str> = settings
        .groups
        .iter()
        .map(String::as_str)
        .filter(|g| *g == DEFAULT_POOL || snapshot.project_of_pool(g).is_some())
        .collect();
    let mut needy: Vec<(&str, Provider, Signal)> = Vec::new();
    for g in &groups {
        // A backup group only lends (DEC-0016).
        if settings.is_backup(g) {
            continue;
        }
        for p in PROVIDERS {
            if view.needs(g, p) {
                needy.push((*g, p, view.signal(g, p)));
            }
        }
    }
    needy.sort_by(|a, b| {
        b.2.refusals_6h
            .cmp(&a.2.refusals_6h)
            .then_with(|| b.2.demand().total_cmp(&a.2.demand()))
            .then_with(|| a.0.cmp(b.0))
            .then_with(|| a.1.as_str().cmp(b.1.as_str()))
    });
    for (borrower, provider, signal) in needy {
        let mut best: Option<(bool, f64, i64, &Account, &str, usize, Signal)> = None;
        for lender in &groups {
            if *lender == borrower || view.needs(lender, provider) {
                continue;
            }
            let lender_signal = view.signal(lender, provider);
            // DEC-0016: a backup group keeps no minimum.
            let backup = settings.is_backup(lender);
            let keep = if backup {
                0
            } else if lender_signal.requests_6h == 0 {
                1
            } else {
                2
            };
            let own = view.own_rooms(lender, provider);
            if own < keep + 1 {
                continue;
            }
            for a in snapshot.accounts.iter().filter(|a| {
                a.pool == *lender
                    && a.provider == provider
                    && usable(snapshot, a)
                    && !held.in_cli.contains(&a.id)
                    && !settings.lent_out(&a.id)
                    && settings.cooled(&a.id, now)
                    && !moving.contains(&a.id)
                    && !snapshot.serves(a, borrower)
                    && snapshot.loan_admits(borrower, a)
            }) {
                let Some(used) = room(snapshot, a, lender, held, now) else {
                    continue;
                };
                let reset = latest_reset(a);
                // A backup group's account first (DEC-0016), then the most room.
                let better = best.as_ref().is_none_or(|(k, u, r, b, ..)| {
                    k.cmp(&backup)
                        .then_with(|| used.total_cmp(u))
                        .then_with(|| r.cmp(&reset))
                        .then_with(|| a.id.cmp(&b.id))
                        .is_lt()
                });
                if better {
                    best = Some((backup, used, reset, a, *lender, own, lender_signal));
                }
            }
        }
        if let Some((_, used, _, account, lender, own, lender_signal)) = best {
            out.push(Action {
                action: "lend",
                account: account.id.clone(),
                provider,
                from: lender.to_owned(),
                to: borrower.to_owned(),
                reason: "need",
                edit: true,
                figures: Figures {
                    borrower: signal,
                    borrower_room: view.rooms(borrower, provider),
                    lender: Some(lender_signal),
                    lender_room: Some(own),
                    used_percent: Some(used),
                },
            });
            break;
        }
    }
    out
}

/// Carries out `actions` on the store and records each in `settings` and the journal. A lend or
/// return that fails still starts the account's cooldown, so a refused move is not tried again
/// every pass; a failed return keeps its loan for the next pass. Answers whether `settings`
/// changed.
pub fn apply(
    store: &Store,
    root: &Path,
    settings: &mut Settings,
    actions: &[Action],
    now: i64,
) -> bool {
    let before = settings.clone();
    let mut lines = Vec::new();
    for a in actions {
        let lend = a.action == "lend";
        let outcome = if !a.edit {
            "forgotten"
        } else {
            match store.set_shared(&a.to, &a.account, lend) {
                Ok(true) => "done",
                Ok(false) => "unchanged",
                Err(_) => "failed",
            }
        };
        if lend && outcome == "done" {
            settings.lent.push(Lend {
                account: a.account.clone(),
                provider: a.provider,
                from: a.from.clone(),
                to: a.to.clone(),
                at: now,
            });
        } else if !lend && outcome != "failed" {
            settings
                .lent
                .retain(|l| l.account != a.account || l.to != a.to);
        }
        if a.edit {
            settings.moved.insert(a.account.clone(), now);
        }
        crate::oplog::event(
            "balancing",
            &[
                ("action", crate::oplog::Field::Code(a.action)),
                ("reason", crate::oplog::Field::Code(a.reason)),
                ("outcome", crate::oplog::Field::Code(outcome)),
            ],
        );
        lines.push(
            json!({"at": now, "account": a.account, "provider": a.provider,
            "from": a.from, "to": a.to, "action": a.action, "reason": a.reason,
            "outcome": outcome, "figures": a.figures}),
        );
    }
    settings
        .moved
        .retain(|_, t| *t <= now && now - *t < COOLDOWN_SECONDS);
    if !lines.is_empty() && append_journal(root, &lines).is_err() {
        crate::oplog::event(
            "balancing",
            &[("journal", crate::oplog::Field::Code("unwritten"))],
        );
    }
    *settings != before
}

fn read_journal(root: &Path) -> Vec<String> {
    let path = root.join(JOURNAL);
    if std::fs::symlink_metadata(&path).is_err() {
        return Vec::new();
    }
    switchboard_core::private_fs::read_private(&path, MAX_JOURNAL)
        .ok()
        .map(|bytes| {
            String::from_utf8_lossy(&bytes)
                .lines()
                .filter(|l| !l.trim().is_empty())
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}
fn append_journal(root: &Path, new: &[Value]) -> Result<(), String> {
    let mut lines = read_journal(root);
    lines.extend(new.iter().map(Value::to_string));
    let keep = lines.len().saturating_sub(KEEP_LINES);
    let mut text = lines[keep..].join("\n");
    text.push('\n');
    switchboard_core::private_fs::private_write(&root.join(JOURNAL), text.as_bytes())
}

/// WF-6: groups to mark as backup (lend only) and groups to unmark.
#[derive(Clone, Debug, Default)]
pub struct Backup {
    pub add: Vec<String>,
    pub remove: Vec<String>,
}

/// The person's change of the chosen groups: each named group must exist; `default` may be
/// chosen. Answers whether anything changed. Adding the first group, or changing `all_new`,
/// takes every group that exists now as seen, so only a group made afterwards counts as new.
pub fn change(
    settings: &mut Settings,
    snapshot: &Snapshot,
    add: &[String],
    remove: &[String],
    all_new: Option<bool>,
    backup: &Backup,
) -> Result<bool, String> {
    let pools = pools(snapshot);
    if add
        .iter()
        .chain(remove)
        .chain(&backup.add)
        .chain(&backup.remove)
        .any(|g| !pools.contains(g))
    {
        return Err(NO_SUCH_GROUP.into());
    }
    // DEC-0016: any group but `default` may only lend.
    if backup.add.iter().any(|g| g == DEFAULT_POOL) {
        return Err(BACKUP_DEFAULT.into());
    }
    let before = settings.clone();
    // A backup group is a chosen one: marking it chooses it.
    for g in add.iter().chain(&backup.add) {
        if !settings.chosen(g) {
            settings.groups.push(g.clone());
        }
    }
    for g in &backup.add {
        if !settings.is_backup(g) {
            settings.backup.push(g.clone());
        }
    }
    settings.groups.retain(|g| !remove.contains(g));
    settings
        .backup
        .retain(|g| !remove.contains(g) && !backup.remove.contains(g));
    if let Some(all) = all_new {
        settings.all_new = all;
    }
    if *settings != before {
        for pool in pools {
            if !settings.seen.contains(&pool) {
                settings.seen.push(pool);
            }
        }
        return Ok(true);
    }
    Ok(false)
}

/// What `balance status` answers: the chosen groups with their names, the loans, the last pass
/// (when an owner runs) and the newest journal lines.
pub fn status(
    root: &Path,
    snapshot: &Snapshot,
    settings: &Settings,
    last: Option<&Value>,
) -> Value {
    let name = |pool: &str| {
        snapshot
            .project_of_pool(pool)
            .map(|p| p.name.clone())
            .unwrap_or_else(|| pool.to_owned())
    };
    let label = |id: &str| {
        snapshot
            .accounts
            .iter()
            .find(|a| a.id == id)
            .map(|a| a.label.clone())
    };
    let groups: Vec<Value> = settings
        .groups
        .iter()
        .map(|g| json!({"pool": g, "name": name(g)}))
        .collect();
    let lent: Vec<Value> = settings
        .lent
        .iter()
        .map(|l| {
            json!({"account_id": l.account, "label": label(&l.account), "provider": l.provider,
                "from": l.from, "from_name": name(&l.from), "to": l.to, "to_name": name(&l.to),
                "at": l.at})
        })
        .collect();
    let journal = read_journal(root);
    let recent: Vec<Value> = journal
        .iter()
        .rev()
        .take(20)
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect();
    // Every group there is, `default` first, for a window or an agent that offers the choice.
    let available: Vec<Value> = pools(snapshot)
        .iter()
        .map(|g| {
            json!({"pool": g, "name": name(g), "chosen": settings.chosen(g),
                "backup": settings.is_backup(g)})
        })
        .collect();
    // DEC-0016: each backup group with its accounts and how many of them are lent now.
    let backup: Vec<Value> = settings
        .backup
        .iter()
        .map(|g| {
            let own: Vec<&Account> = snapshot.accounts.iter().filter(|a| a.pool == *g).collect();
            let lent = own.iter().filter(|a| settings.lent_out(&a.id)).count();
            json!({"pool": g, "name": name(g), "accounts": own.len(), "lent": lent})
        })
        .collect();
    json!({"on": settings.on(), "groups": groups, "available": available,
        "all_new": settings.all_new, "backup": backup,
        "pass_minutes": PASS_SECONDS / 60, "lent": lent, "recent": recent,
        "last_pass": last.cloned().unwrap_or(Value::Null)})
}

/// What a pass last saw, for `balance status`: each chosen group's figures and the actions.
pub fn summary(
    snapshot: &Snapshot,
    signals: &Signals,
    held: &Held,
    settings: &Settings,
    actions: &[Action],
    now: i64,
) -> Value {
    let view = View {
        snapshot,
        signals,
        held,
        settings,
        now,
    };
    let chosen: BTreeSet<&str> = settings.groups.iter().map(String::as_str).collect();
    let mut groups = Vec::new();
    for g in chosen {
        for p in PROVIDERS {
            let s = view.signal(g, p);
            let rooms = view.rooms(g, p);
            if s == Signal::default()
                && rooms == 0
                && view
                    .snapshot
                    .pool_accounts(g)
                    .iter()
                    .all(|a| a.provider != p)
            {
                continue;
            }
            groups.push(json!({"pool": g, "provider": p, "rooms": rooms,
                "needs": view.needs(g, p), "figures": s, "demand": s.demand()}));
        }
    }
    json!({"at": now, "groups": groups, "actions": actions})
}

#[cfg(test)]
mod tests {
    use super::*;
    use switchboard_core::{Project, Usage, UsageHealth, UsageWindow};

    const NOW: i64 = 1_800_000_000;
    fn account(id: &str, pool: &str, used: Option<f64>) -> Account {
        Account {
            id: id.into(),
            label: id.into(),
            provider: Provider::Claude,
            kind: AuthKind::OAuth,
            pool: pool.into(),
            enabled: true,
            created_at: 1,
            identity: None,
            usage: used.map(|u| Usage {
                windows: vec![UsageWindow {
                    name: "five_hour".into(),
                    used_percent: u,
                    resets_at: Some(NOW + 3600),
                }],
                used_percent: u,
                observed_at: NOW - 30,
                resets_at: Some(NOW + 3600),
                source: "claude_oauth".into(),
            }),
            external_identity: None,
            usage_health: Some(UsageHealth {
                status: "ok".into(),
                checked_at: NOW - 30,
                next_check_at: NOW + 150,
            }),
        }
    }
    fn group(pool: &str) -> Project {
        Project {
            pool: pool.into(),
            name: pool.to_uppercase(),
            folders: vec![format!("/work/{pool}")],
            created_at: 1,
            ..Project::default()
        }
    }
    /// Agents (one account, at its limit) and Apps (three accounts with room), both chosen.
    fn estate() -> Snapshot {
        Snapshot {
            accounts: vec![
                account("agents-1", "agents", Some(95.0)),
                account("apps-1", "apps", Some(10.0)),
                account("apps-2", "apps", Some(20.0)),
                account("apps-3", "apps", Some(30.0)),
            ],
            projects: vec![group("agents"), group("apps")],
            ..Snapshot::default()
        }
    }
    fn chosen(groups: &[&str]) -> Settings {
        Settings {
            groups: groups.iter().map(|g| (*g).to_owned()).collect(),
            ..Settings::default()
        }
    }
    fn busy(pool: &str, requests: u64, refusals: u64) -> ((String, &'static str), Signal) {
        (
            (pool.to_owned(), "claude"),
            Signal {
                requests_6h: requests,
                refusals_6h: refusals,
                refusals_2h: refusals,
                hourly_7d: 10.0,
            },
        )
    }
    fn sig_map(list: &[((String, &'static str), Signal)]) -> Signals {
        list.iter().cloned().collect()
    }
    fn lent(account: &str, from: &str, to: &str, at: i64) -> Lend {
        Lend {
            account: account.into(),
            provider: Provider::Claude,
            from: from.into(),
            to: to.into(),
            at,
        }
    }
    /// The estate with `apps-1` already shared with Agents.
    fn on_loan() -> Snapshot {
        let mut s = estate();
        s.projects[0].shared_accounts.push("apps-1".into());
        s
    }

    /// The estate with Spare, a group with one account with room, marked backup.
    fn with_backup(used: f64) -> (Snapshot, Settings) {
        let mut s = estate();
        s.projects.push(group("spare"));
        s.accounts.push(account("spare-1", "spare", Some(used)));
        let mut settings = chosen(&["agents", "apps", "spare"]);
        settings.backup = vec!["spare".into()];
        (s, settings)
    }

    #[test]
    fn a_backup_group_lends_first_and_keeps_no_minimum() {
        // DEC-0016: Spare's one account goes before Apps' with more room, and Spare keeps none.
        let (s, settings) = with_backup(50.0);
        let actions = plan(
            &s,
            &sig_map(&[busy("agents", 40, 3), busy("apps", 5, 0)]),
            &Held::default(),
            &settings,
            NOW,
        );
        assert_eq!(actions.len(), 1, "{actions:?}");
        assert_eq!(
            (actions[0].account.as_str(), actions[0].from.as_str()),
            ("spare-1", "spare")
        );
        assert_eq!(actions[0].figures.lender_room, Some(1));
    }

    #[test]
    fn a_backup_group_never_borrows_and_its_own_work_takes_its_account_home() {
        let (s, settings) = with_backup(99.0);
        let actions = plan(
            &s,
            &sig_map(&[busy("spare", 40, 3), busy("apps", 5, 0)]),
            &Held::default(),
            &settings,
            NOW,
        );
        assert!(actions.is_empty(), "{actions:?}");
        // Spare's own folders are busy and its account is lent: it comes home (`home_needs`).
        let (mut s, mut settings) = with_backup(50.0);
        s.projects[0].shared_accounts.push("spare-1".into());
        settings.lent = vec![lent(
            "spare-1",
            "spare",
            "agents",
            NOW - 2 * COOLDOWN_SECONDS,
        )];
        let actions = plan(
            &s,
            &sig_map(&[busy("spare", 40, 3), busy("agents", 40, 3)]),
            &Held::default(),
            &settings,
            NOW,
        );
        assert!(
            actions.iter().any(|a| a.action == "return"
                && a.account == "spare-1"
                && a.reason == "home_needs"),
            "{actions:?}"
        );
    }

    #[test]
    fn a_backup_account_goes_home_once_the_borrower_has_room_of_its_own() {
        let (mut s, mut settings) = with_backup(50.0);
        s.projects[0].shared_accounts.push("spare-1".into());
        settings.lent = vec![lent(
            "spare-1",
            "spare",
            "agents",
            NOW - 2 * COOLDOWN_SECONDS,
        )];
        let signals = sig_map(&[busy("agents", 40, 3)]);
        // Agents' own account still has no room: the loan stays (Agents may borrow more).
        let before = plan(&s, &signals, &Held::default(), &settings, NOW);
        assert!(before.iter().all(|a| a.action != "return"), "{before:?}");
        // Its own account has room again: the backup account goes home at once, refusals or not.
        s.accounts[0] = account("agents-1", "agents", Some(10.0));
        let actions = plan(&s, &signals, &Held::default(), &settings, NOW);
        assert_eq!(actions.len(), 1, "{actions:?}");
        assert_eq!(
            (
                actions[0].action,
                actions[0].reason,
                actions[0].account.as_str()
            ),
            ("return", "own_room", "spare-1")
        );
    }

    #[test]
    fn any_group_but_default_is_marked_backup_and_marking_chooses_it() {
        let (s, _) = with_backup(50.0);
        let mut settings = Settings::default();
        let mark = |g: &str| Backup {
            add: vec![g.into()],
            remove: vec![],
        };
        assert_eq!(
            change(&mut settings, &s, &[], &[], None, &mark("default")).unwrap_err(),
            BACKUP_DEFAULT
        );
        assert!(change(&mut settings, &s, &[], &[], None, &mark("spare")).unwrap());
        assert_eq!(
            (settings.groups.clone(), settings.backup.clone()),
            (vec!["spare".to_string()], vec!["spare".to_string()])
        );
        // Unmarking leaves the group chosen; taking it out of the chosen ones unmarks it too.
        let unmark = Backup {
            add: vec![],
            remove: vec!["spare".into()],
        };
        assert!(change(&mut settings, &s, &[], &[], None, &unmark).unwrap());
        assert!(settings.backup.is_empty() && settings.groups == ["spare"]);
        change(&mut settings, &s, &[], &[], None, &mark("spare")).unwrap();
        change(
            &mut settings,
            &s,
            &[],
            &["spare".into()],
            None,
            &Backup::default(),
        )
        .unwrap();
        assert!(settings.backup.is_empty() && settings.groups.is_empty());
    }

    #[test]
    fn a_group_with_no_room_takes_the_lender_account_with_the_most_room() {
        let s = estate();
        let actions = plan(
            &s,
            &sig_map(&[busy("agents", 40, 3), busy("apps", 5, 0)]),
            &Held::default(),
            &chosen(&["agents", "apps"]),
            NOW,
        );
        assert_eq!(actions.len(), 1, "{actions:?}");
        let a = &actions[0];
        assert_eq!(
            (a.action, a.account.as_str(), a.from.as_str(), a.to.as_str()),
            ("lend", "apps-1", "apps", "agents")
        );
        assert_eq!(a.figures.lender_room, Some(3));
        assert_eq!(a.figures.borrower.refusals_6h, 3);
        assert_eq!(a.figures.used_percent, Some(10.0));
    }

    #[test]
    fn one_account_with_room_is_enough_without_refusals_and_short_with_them() {
        let mut s = estate();
        s.accounts[0] = account("agents-1", "agents", Some(40.0));
        let quiet = plan(
            &s,
            &sig_map(&[busy("agents", 40, 0)]),
            &Held::default(),
            &chosen(&["agents", "apps"]),
            NOW,
        );
        assert!(quiet.is_empty(), "{quiet:?}");
        let refused = plan(
            &s,
            &sig_map(&[busy("agents", 40, 2)]),
            &Held::default(),
            &chosen(&["agents", "apps"]),
            NOW,
        );
        assert_eq!(refused.len(), 1);
        // A group with no request in 6 hours needs nothing, whatever its room.
        s.accounts[0] = account("agents-1", "agents", Some(99.0));
        assert!(plan(
            &s,
            &Signals::new(),
            &Held::default(),
            &chosen(&["agents", "apps"]),
            NOW
        )
        .is_empty());
    }

    #[test]
    fn a_lender_keeps_two_accounts_with_room_or_one_when_idle() {
        let mut s = estate();
        s.accounts.retain(|a| a.id != "apps-3");
        let busy_lender = plan(
            &s,
            &sig_map(&[busy("agents", 40, 3), busy("apps", 5, 0)]),
            &Held::default(),
            &chosen(&["agents", "apps"]),
            NOW,
        );
        assert!(
            busy_lender.is_empty(),
            "two with room and busy: keeps both {busy_lender:?}"
        );
        let idle_lender = plan(
            &s,
            &sig_map(&[busy("agents", 40, 3)]),
            &Held::default(),
            &chosen(&["agents", "apps"]),
            NOW,
        );
        assert_eq!(idle_lender.len(), 1, "idle for 6 hours: keeps one");
        s.accounts.retain(|a| a.id != "apps-2");
        assert!(plan(
            &s,
            &sig_map(&[busy("agents", 40, 3)]),
            &Held::default(),
            &chosen(&["agents", "apps"]),
            NOW
        )
        .is_empty());
    }

    #[test]
    fn a_group_not_chosen_is_never_read_or_touched() {
        let s = estate();
        let sig = sig_map(&[busy("agents", 40, 3)]);
        assert!(
            plan(&s, &sig, &Held::default(), &chosen(&["agents"]), NOW).is_empty(),
            "Apps lends nothing"
        );
        assert!(
            plan(&s, &sig, &Held::default(), &chosen(&["apps"]), NOW).is_empty(),
            "Agents borrows nothing"
        );
        assert!(
            plan(&s, &sig, &Held::default(), &Settings::default(), NOW).is_empty(),
            "off"
        );
    }

    #[test]
    fn one_account_moves_per_pass_to_the_neediest_group() {
        let mut s = estate();
        s.accounts.push(account("tools-1", "tools", Some(97.0)));
        s.projects.push(group("tools"));
        let actions = plan(
            &s,
            &sig_map(&[
                busy("agents", 40, 1),
                busy("tools", 10, 5),
                busy("apps", 1, 0),
            ]),
            &Held::default(),
            &chosen(&["agents", "apps", "tools"]),
            NOW,
        );
        assert_eq!(actions.len(), 1, "{actions:?}");
        assert_eq!(actions[0].to, "tools", "most refusals first");
        // Equal refusals: the most demand against its usual week.
        let mut a = busy("agents", 60, 2);
        a.1.hourly_7d = 1.0;
        let actions = plan(
            &s,
            &sig_map(&[a, busy("tools", 60, 2)]),
            &Held::default(),
            &chosen(&["agents", "apps", "tools"]),
            NOW,
        );
        assert_eq!(actions[0].to, "agents");
    }

    #[test]
    fn accounts_that_must_never_be_lent_stay_home() {
        let sig = sig_map(&[busy("agents", 40, 3)]);
        let settings = chosen(&["agents", "apps"]);
        // Each case leaves Apps with the other two accounts with room, so only apps-1 is judged.
        let only = |s: &Snapshot, held: &Held| {
            plan(s, &sig, held, &settings, NOW)
                .iter()
                .any(|a| a.account == "apps-1")
        };
        let mut s = estate();
        s.accounts[2] = account("apps-2", "apps", Some(85.0));
        s.accounts[3] = account("apps-3", "apps", Some(85.0));
        s.accounts.push(account("apps-4", "apps", Some(40.0)));
        s.accounts.push(account("apps-5", "apps", Some(50.0)));
        assert!(only(&s, &Held::default()), "the baseline lends apps-1");
        let cli = Held {
            in_cli: HashSet::from(["apps-1".to_owned()]),
            ..Held::default()
        };
        assert!(!only(&s, &cli), "the ordinary CLI's account");
        let sign_in = Held {
            sign_in: HashSet::from(["apps-1".to_owned()]),
            ..Held::default()
        };
        assert!(!only(&s, &sign_in), "waiting for a sign-in");
        let limited = Held {
            limited: HashSet::from(["apps-1".to_owned()]),
            ..Held::default()
        };
        assert!(!only(&s, &limited), "held by a limit");
        let mut given = s.clone();
        given.assigned_to.insert("apps-1".into(), "an agent".into());
        assert!(!only(&given, &Held::default()), "given to an agent");
        let mut away = s.clone();
        away.elsewhere.insert("apps-1".into(), NOW - 100);
        assert!(!only(&away, &Held::default()), "used elsewhere");
        let mut off = s.clone();
        off.accounts[1].enabled = false;
        assert!(!only(&off, &Held::default()), "disabled");
        let mut key = s.clone();
        key.accounts[1].kind = AuthKind::ApiKey;
        assert!(!only(&key, &Held::default()), "an API key");
        let mut stale = s.clone();
        stale.accounts[1].usage.as_mut().unwrap().observed_at = NOW - 3600;
        stale.accounts[1].usage_health.as_mut().unwrap().checked_at = NOW - 3600;
        assert!(!only(&stale, &Held::default()), "stale figures");
        let mut cooling = settings.clone();
        cooling.moved.insert("apps-1".into(), NOW - 600);
        assert!(
            !plan(&s, &sig, &Held::default(), &cooling, NOW)
                .iter()
                .any(|a| a.account == "apps-1"),
            "moved 10 minutes ago"
        );
    }

    #[test]
    fn no_account_crosses_an_organizations_boundary() {
        let mut s = estate();
        s.organizations.push(
            serde_json::from_value(json!({"id": "acme", "name": "Acme", "created_at": 1})).unwrap(),
        );
        s.projects[0].organizations = vec!["acme".into()];
        // Apps's accounts are personal: the organization's group Agents cannot take them.
        assert!(plan(
            &s,
            &sig_map(&[busy("agents", 40, 3)]),
            &Held::default(),
            &chosen(&["agents", "apps"]),
            NOW
        )
        .is_empty());
        for a in s.accounts.iter().filter(|a| a.pool == "apps") {
            s.memberships.insert(a.id.clone(), "acme".into());
        }
        assert_eq!(
            plan(
                &s,
                &sig_map(&[busy("agents", 40, 3)]),
                &Held::default(),
                &chosen(&["agents", "apps"]),
                NOW
            )
            .len(),
            1
        );
    }

    /// Wave 2: Apps's three accounts with room live in `default`, which lends one to Agents.
    #[test]
    fn the_default_group_lends_like_any_chosen_group() {
        let mut s = estate();
        for a in s.accounts.iter_mut().filter(|a| a.pool == "apps") {
            a.pool = DEFAULT_POOL.into();
        }
        s.projects.retain(|p| p.pool != "apps");
        let actions = plan(
            &s,
            &sig_map(&[busy("agents", 40, 3)]),
            &Held::default(),
            &chosen(&["agents", "default"]),
            NOW,
        );
        assert_eq!(actions.len(), 1, "{actions:?}");
        assert_eq!(
            (
                actions[0].action,
                actions[0].from.as_str(),
                actions[0].to.as_str()
            ),
            ("lend", "default", "agents")
        );
        // Not chosen, `default` lends nothing.
        assert!(plan(
            &s,
            &sig_map(&[busy("agents", 40, 3)]),
            &Held::default(),
            &chosen(&["agents"]),
            NOW
        )
        .is_empty());
    }

    /// Wave 2: `default`, out of room, borrows from a chosen group; while organizations exist
    /// it takes only a personal account, never an organization's.
    #[test]
    fn the_default_group_borrows_only_personal_accounts() {
        let mut s = estate();
        s.accounts[0].pool = DEFAULT_POOL.into();
        s.projects.retain(|p| p.pool != "agents");
        let sig = sig_map(&[busy("default", 40, 3)]);
        let actions = plan(
            &s,
            &sig,
            &Held::default(),
            &chosen(&["default", "apps"]),
            NOW,
        );
        assert_eq!(
            actions
                .iter()
                .map(|a| (a.action, a.account.as_str(), a.to.as_str()))
                .collect::<Vec<_>>(),
            [("lend", "apps-1", "default")]
        );
        s.organizations.push(
            serde_json::from_value(json!({"id": "acme", "name": "Acme", "created_at": 1})).unwrap(),
        );
        s.projects[0].organizations = vec!["acme".into()];
        for a in s.accounts.iter().filter(|a| a.pool == "apps") {
            s.memberships.insert(a.id.clone(), "acme".into());
        }
        assert!(
            plan(
                &s,
                &sig,
                &Held::default(),
                &chosen(&["default", "apps"]),
                NOW
            )
            .is_empty(),
            "an organization's account never serves the default group"
        );
    }

    /// Wave 2: a loan to `default` reads from the store's `default_shared`, and an account the
    /// person moved to another home since it was lent goes back.
    #[test]
    fn a_loan_to_default_is_read_and_one_moved_home_goes_back() {
        let mut s = estate();
        s.accounts[0].pool = DEFAULT_POOL.into();
        s.projects.retain(|p| p.pool != "agents");
        s.default_shared.push("apps-1".into());
        let mut settings = chosen(&["default", "apps"]);
        settings
            .lent
            .push(lent("apps-1", "apps", DEFAULT_POOL, NOW - 60));
        settings.moved.insert("apps-1".into(), NOW - 60);
        assert!(
            returns_owed(&s, &Held::default(), &settings, NOW).is_empty(),
            "the loan stands"
        );
        s.projects.push(group("tools"));
        s.accounts[1].pool = "tools".into();
        let owed = returns_owed(&s, &Held::default(), &settings, NOW);
        assert_eq!((owed[0].reason, owed[0].edit), ("not_lendable", true));
        s.default_shared.clear();
        let gone = returns_owed(&s, &Held::default(), &settings, NOW);
        assert_eq!((gone[0].reason, gone[0].edit), ("share_gone", false));
    }

    #[test]
    fn a_lent_account_returns_when_the_borrower_is_quiet_or_its_home_needs_it() {
        let s = on_loan();
        let mut settings = chosen(&["agents", "apps"]);
        settings
            .lent
            .push(lent("apps-1", "apps", "agents", NOW - 3 * 3600));
        settings.moved.insert("apps-1".into(), NOW - 3 * 3600);
        let quiet = plan(
            &s,
            &sig_map(&[busy("agents", 40, 0)]),
            &Held::default(),
            &settings,
            NOW,
        );
        assert_eq!(
            quiet
                .iter()
                .map(|a| (a.action, a.reason))
                .collect::<Vec<_>>(),
            [("return", "quiet")]
        );
        // Refusals in the last 2 hours keep it there.
        let pressed = plan(
            &s,
            &sig_map(&[busy("agents", 40, 2)]),
            &Held::default(),
            &settings,
            NOW,
        );
        assert!(pressed.iter().all(|a| a.action != "return"), "{pressed:?}");
        // Lent an hour ago: not quiet long enough.
        let mut young = settings.clone();
        young.lent[0].at = NOW - 3600;
        assert!(plan(
            &s,
            &sig_map(&[busy("agents", 40, 0)]),
            &Held::default(),
            &young,
            NOW
        )
        .is_empty());
        // Its home group runs out of room: it goes home.
        let mut home = s.clone();
        home.accounts[2] = account("apps-2", "apps", Some(99.0));
        home.accounts[3] = account("apps-3", "apps", Some(99.0));
        home.accounts[1] = account("apps-1", "apps", Some(99.0));
        let back = plan(
            &home,
            &sig_map(&[busy("agents", 40, 2), busy("apps", 20, 4)]),
            &Held::default(),
            &settings,
            NOW,
        );
        assert_eq!(back[0].reason, "home_needs");
        // The cooldown holds a return the figures ask for.
        let mut cooling = settings.clone();
        cooling.moved.insert("apps-1".into(), NOW - 600);
        assert!(plan(
            &s,
            &sig_map(&[busy("agents", 40, 0)]),
            &Held::default(),
            &cooling,
            NOW
        )
        .is_empty());
    }

    #[test]
    fn turning_balancing_off_returns_every_lent_account() {
        let s = on_loan();
        let mut settings = chosen(&["agents", "apps"]);
        settings
            .lent
            .push(lent("apps-1", "apps", "agents", NOW - 60));
        settings.moved.insert("apps-1".into(), NOW - 60);
        settings.groups.clear();
        let owed = returns_owed(&s, &Held::default(), &settings, NOW);
        assert_eq!(owed.len(), 1);
        assert_eq!(
            (owed[0].action, owed[0].reason, owed[0].edit),
            ("return", "not_chosen", true)
        );
        // So does taking one of its two groups out, at once and in spite of the cooldown.
        let mut one = chosen(&["agents"]);
        one.lent = settings.lent.clone();
        one.moved = settings.moved.clone();
        assert_eq!(
            plan(&s, &Signals::new(), &Held::default(), &one, NOW)[0].reason,
            "not_chosen"
        );
        // An account given to an agent since it was lent goes home at once too.
        let mut given = s.clone();
        given.assigned_to.insert("apps-1".into(), "an agent".into());
        assert_eq!(
            returns_owed(
                &given,
                &Held::default(),
                &chosen(&["agents", "apps"]).with_lent(&one),
                NOW
            )[0]
            .reason,
            "not_lendable"
        );
        // A share someone took away by hand is forgotten, with no change to the groups.
        let gone = returns_owed(
            &estate(),
            &Held::default(),
            &chosen(&["agents", "apps"]).with_lent(&one),
            NOW,
        );
        assert_eq!((gone[0].reason, gone[0].edit), ("share_gone", false));
    }
    impl Settings {
        fn with_lent(mut self, other: &Settings) -> Self {
            self.lent = other.lent.clone();
            self.moved = other.moved.clone();
            self
        }
    }

    #[test]
    fn a_group_made_later_joins_only_with_all_new() {
        let mut s = estate();
        let mut settings = Settings::default();
        assert!(change(
            &mut settings,
            &s,
            &["agents".into(), "apps".into()],
            &[],
            None,
            &Backup::default()
        )
        .unwrap());
        assert_eq!(settings.seen, ["default", "agents", "apps"]);
        s.projects.push(group("new"));
        let mut without = settings.clone();
        assert!(resolve(&mut without, &s));
        assert_eq!(without.groups, ["agents", "apps"], "seen, not chosen");
        settings.all_new = true;
        assert!(resolve(&mut settings, &s));
        assert_eq!(settings.groups, ["agents", "apps", "new"]);
        // A group the person takes out again stays out.
        change(
            &mut settings,
            &s,
            &[],
            &["new".into()],
            None,
            &Backup::default(),
        )
        .unwrap();
        assert!(!resolve(&mut settings, &s));
        assert_eq!(settings.groups, ["agents", "apps"]);
        // A deleted group leaves the settings; an unknown one is refused.
        s.projects.retain(|p| p.pool != "apps");
        assert!(resolve(&mut settings, &s));
        assert_eq!(settings.groups, ["agents"]);
        assert_eq!(
            change(
                &mut settings,
                &s,
                &["nope".into()],
                &[],
                None,
                &Backup::default()
            ),
            Err(NO_SUCH_GROUP.into())
        );
        // Off, a new group joins nothing.
        let mut off = Settings {
            all_new: true,
            ..Settings::default()
        };
        s.projects.push(group("later"));
        assert!(!resolve(&mut off, &s));
        assert!(off.groups.is_empty());
    }

    #[test]
    fn settings_and_journal_are_private_and_the_journal_keeps_its_newest_lines() {
        let root = tempfile::tempdir().unwrap();
        assert_eq!(read(root.path()).unwrap(), Settings::default());
        let mut settings = chosen(&["agents"]);
        settings.lent.push(lent("apps-1", "apps", "agents", NOW));
        write(root.path(), &settings).unwrap();
        assert_eq!(read(root.path()).unwrap(), settings);
        let lines: Vec<Value> = (0..1005).map(|n| json!({"n": n})).collect();
        append_journal(root.path(), &lines).unwrap();
        append_journal(root.path(), &[json!({"n": 1005})]).unwrap();
        let kept = read_journal(root.path());
        assert_eq!(kept.len(), KEEP_LINES);
        assert_eq!(kept.last().unwrap(), &json!({"n": 1005}).to_string());
        #[cfg(unix)]
        for file in [FILE, JOURNAL] {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(root.path().join(file))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600, "{file}");
        }
        std::fs::write(root.path().join(FILE), b"{not json").unwrap();
        assert_eq!(read(root.path()), Err(UNREADABLE.into()));
    }

    /// The owner's pass lends an account of a chosen group to a chosen group out of room, records
    /// it, waits 15 minutes before it decides again, and taking the groups out returns it.
    #[tokio::test]
    async fn a_pass_lends_to_a_group_out_of_room_and_turning_off_returns_it() {
        use crate::fixtures::*;
        use crate::Operation;
        use switchboard_proxy::stats::{self, Tallies, Tally};
        let root = tempfile::tempdir().unwrap();
        let runtime = runtime(root.path(), signed_out, activates).await;
        let work = tempfile::tempdir().unwrap();
        let now = crate::monitor::now();
        let mut pools = Vec::new();
        for (name, used) in [("Agents", vec![97.0]), ("Apps", vec![10.0, 20.0, 30.0])] {
            let folder = work.path().canonicalize().unwrap().join(name);
            std::fs::create_dir_all(&folder).unwrap();
            let mut ids = Vec::new();
            for (n, u) in used.iter().enumerate() {
                let a = save(&runtime.store, &format!("{name}-{n}"), "default");
                runtime
                    .store
                    .observe(
                        &a.id,
                        Usage {
                            windows: vec![UsageWindow {
                                name: "five_hour".into(),
                                used_percent: *u,
                                resets_at: Some(now + 3600),
                            }],
                            used_percent: *u,
                            observed_at: now,
                            resets_at: Some(now + 3600),
                            source: "claude_oauth".into(),
                        },
                    )
                    .unwrap();
                runtime
                    .store
                    .usage_health(&a.id, "ok", now, now + 150)
                    .unwrap();
                ids.push(a.id);
            }
            let view = runtime
                .execute(Operation::SaveProject {
                    pool: None,
                    name: name.into(),
                    folders: vec![folder],
                    account_ids: ids.clone(),
                    base_folders: None,
                    organization: None,
                    organizations: None,
                    move_accounts: false,
                })
                .await
                .unwrap();
            pools.push((view["pool"].as_str().unwrap().to_owned(), ids));
        }
        let (agents, agent_ids) = pools[0].clone();
        let (apps, app_ids) = pools[1].clone();
        // Agents ran 40 requests in the last hour, 3 refused for its limit.
        runtime.statistics.started_at(now - 8 * 3600);
        runtime.statistics.absorb_at(
            Tallies::from([(
                stats::key(
                    &stats::day_of(now),
                    &agent_ids[0],
                    &agents,
                    "claude-code",
                    "",
                ),
                Tally {
                    requests: 40,
                    limited: 3,
                    ..Tally::default()
                },
            )]),
            now - 600,
        );
        let on = runtime
            .execute(Operation::Balance {
                groups_add: vec![agents.clone(), apps.clone()],
                groups_remove: vec![],
                all_new: Some(true),
                backup_add: vec![],
                backup_remove: vec![],
            })
            .await
            .unwrap();
        assert_eq!(on["on"], true, "{on}");
        runtime.balance_pass(now).await;
        let shared = || {
            runtime
                .store
                .snapshot()
                .unwrap()
                .projects
                .iter()
                .find(|p| p.pool == agents)
                .unwrap()
                .shared_accounts
                .clone()
        };
        assert_eq!(
            shared(),
            [app_ids[0].clone()],
            "the account with the most room"
        );
        let status = runtime.execute(Operation::BalanceStatus).await.unwrap();
        assert_eq!(
            status["lent"][0]["account_id"],
            app_ids[0].as_str(),
            "{status}"
        );
        assert_eq!(status["recent"][0]["action"], "lend");
        assert_eq!(status["recent"][0]["outcome"], "done");
        assert_eq!(status["recent"][0]["figures"]["borrower"]["refusals_6h"], 3);
        assert!(status["last_pass"]["groups"].is_array(), "{status}");
        // Within 15 minutes nothing is decided again.
        runtime
            .store
            .set_shared(&agents, &app_ids[0], false)
            .unwrap();
        runtime.balance_pass(now + 60).await;
        assert!(shared().is_empty());
        runtime
            .store
            .set_shared(&agents, &app_ids[0], true)
            .unwrap();
        // Taking both groups out returns the lent account at once.
        let off = runtime
            .execute(Operation::Balance {
                groups_add: vec![],
                groups_remove: vec![agents.clone(), apps.clone()],
                all_new: None,
                backup_add: vec![],
                backup_remove: vec![],
            })
            .await
            .unwrap();
        assert_eq!(off["on"], false);
        assert!(shared().is_empty(), "returned");
        assert_eq!(off["lent"], json!([]));
        assert_eq!(off["recent"][0]["reason"], "not_chosen");
        // Off, a pass reads and writes nothing.
        let before = std::fs::read(root.path().join(FILE)).unwrap();
        runtime.balance_pass(now + 3600).await;
        assert_eq!(std::fs::read(root.path().join(FILE)).unwrap(), before);
        assert!(runtime
            .execute(Operation::Balance {
                groups_add: vec!["no-such-group".into()],
                groups_remove: vec![],
                all_new: None,
                backup_add: vec![],
                backup_remove: vec![],
            })
            .await
            .is_err());
    }

    #[test]
    fn signals_sum_each_groups_counts_by_provider() {
        let s = estate();
        let six = crate::statistics::Window::from([
            (("agents".into(), "agents-1".into()), (30, 4)),
            (("agents".into(), "apps-1".into()), (6, 0)),
            (("agents".into(), "gone".into()), (99, 99)),
        ]);
        let two =
            crate::statistics::Window::from([(("agents".into(), "agents-1".into()), (10, 1))]);
        let week =
            crate::statistics::Window::from([(("agents".into(), "agents-1".into()), (168, 0))]);
        let sig = signals(&s, &six, &two, &week);
        let a = sig[&("agents".to_owned(), "claude")];
        assert_eq!((a.requests_6h, a.refusals_6h, a.refusals_2h), (36, 4, 1));
        assert_eq!(a.hourly_7d, 1.0);
        assert_eq!(a.demand(), 6.0);
    }
}
