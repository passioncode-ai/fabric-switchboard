//! SB-419 (DEC-0028, operator 2026-11-17): groups fed from the default group. A person turns the
//! feed on per group, with a priority (1 is the highest). The default group is the reservoir: an
//! account of a fed group that runs out rests in `default` (a park), and accounts of the default
//! group with room, or whose limit resets within the look-ahead, are placed into each fed group
//! until it holds `ready` of them (default 2: the one in use and the next).
//!
//! It is a second policy on balancing's pass and files (DEC-0012): the settings live in
//! `balancing.json` (`fed`, `ready`, `lookahead_minutes`), each move is a `Lend` marked `feed`,
//! and each is one line of `balancing.jsonl` with its reason and figures. Like a loan, a move
//! never changes an account's home: a placement shares a reservoir account with the fed group,
//! a park shares a spent account of a fed group with `default`, and taking either back is
//! removing that share. `plan` is the whole decision and reads nothing.
use crate::balancing::{self, Action, Figures, Held, Settings, Signals, DEFAULT_POOL};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashSet};
use std::path::Path;
use switchboard_core::{Account, Provider, Snapshot};

/// Ready accounts a fed group keeps when the person set none.
pub const DEFAULT_READY: u32 = 2;
/// Minutes ahead an account whose limit resets counts as ready, when the person set none.
pub const DEFAULT_LOOKAHEAD_MINUTES: u32 = 30;
pub const MAX_READY: u32 = 10;
pub const MAX_LOOKAHEAD_MINUTES: u32 = 720;
pub const MAX_PRIORITY: u32 = 99;
const PROVIDERS: [Provider; 2] = [Provider::Claude, Provider::Codex];
/// Journal lines `feed status` shows.
const RECENT: usize = 20;

pub const FEED_DEFAULT: &str = "The default group feeds the others, so it cannot be fed itself.";
pub const NOT_FED: &str =
    "That group is not fed from the default group. Turn its feed on first, then set its priority.";
pub const PRIORITY_RANGE: &str = "Give the priority as a whole number from 1 (highest) to 99.";
pub const READY_RANGE: &str = "Give the number of ready accounts as a whole number from 1 to 10.";
pub const LOOKAHEAD_RANGE: &str = "Give the look-ahead in minutes, from 0 to 720.";
pub const NO_GROUP: &str = "Name the group whose feed changes.";

/// One fed group.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fed {
    pub pool: String,
    /// 1 is the highest.
    pub priority: u32,
}

/// The figures a feed move was decided on.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct FeedFigures {
    /// The fed group's priority.
    pub priority: u32,
    /// Ready accounts of this provider serving the fed group before the move.
    pub ready: usize,
    /// How many ready accounts the fed group keeps.
    pub want: usize,
    /// The account's used share, when it has room now.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub used_percent: Option<f64>,
    /// Seconds until the account's limit resets, when it counts as ready by its reset.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resets_in: Option<i64>,
}

/// How many ready accounts each fed group keeps.
pub fn ready_target(settings: &Settings) -> usize {
    settings.ready.unwrap_or(DEFAULT_READY).clamp(1, MAX_READY) as usize
}
/// The look-ahead in seconds.
pub fn lookahead(settings: &Settings) -> i64 {
    i64::from(
        settings
            .lookahead_minutes
            .unwrap_or(DEFAULT_LOOKAHEAD_MINUTES)
            .min(MAX_LOOKAHEAD_MINUTES),
    ) * 60
}
fn fed_entry<'a>(settings: &'a Settings, pool: &str) -> Option<&'a Fed> {
    settings.fed.iter().find(|f| f.pool == pool)
}
/// The fed groups that exist and are on, highest priority first (then by id).
fn fed_order<'a>(snapshot: &Snapshot, settings: &'a Settings) -> Vec<&'a Fed> {
    let mut out: Vec<&Fed> = settings
        .fed
        .iter()
        .filter(|f| {
            snapshot
                .project_of_pool(&f.pool)
                .is_some_and(|p| !p.tokens && !p.off)
        })
        .collect();
    out.sort_by(|a, b| {
        a.priority
            .cmp(&b.priority)
            .then_with(|| a.pool.cmp(&b.pool))
    });
    out
}

/// Why an account counts as ready in a group.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Ready {
    /// It has room now: its used share.
    Room(f64),
    /// Its limit resets at this time, within the look-ahead.
    Soon(i64),
}

/// The resets of the account's own windows at or past `pool`'s switching threshold; none when
/// its figures are unknown, empty when no window holds it.
fn blocking(snapshot: &Snapshot, account: &Account, pool: &str) -> Option<Vec<Option<i64>>> {
    let (limit, _) = balancing::limits(snapshot, account.provider, pool);
    let usage = account.usage.as_ref()?;
    if usage.windows.is_empty() {
        return Some(if usage.used_percent >= limit {
            vec![usage.resets_at]
        } else {
            Vec::new()
        });
    }
    Some(
        usage
            .windows
            .iter()
            .filter(|w| !w.is_feature() && w.used_percent >= limit)
            .map(|w| w.resets_at)
            .collect(),
    )
}

struct View<'a> {
    snapshot: &'a Snapshot,
    signals: &'a Signals,
    held: &'a Held,
    settings: &'a Settings,
    now: i64,
    lookahead: i64,
}
impl View<'_> {
    /// Ready in `pool`: room now on fresh figures, or every window that holds it resets within
    /// the look-ahead. A sign-in that waits for a person is never ready.
    fn readiness(&self, account: &Account, pool: &str) -> Option<Ready> {
        if self.held.sign_in.contains(&account.id) {
            return None;
        }
        if let Some(used) = balancing::room(self.snapshot, account, pool, self.held, self.now) {
            return Some(Ready::Room(used));
        }
        let windows = blocking(self.snapshot, account, pool)?;
        if windows.is_empty() {
            // No window holds it, yet it has no room: stale figures or a limit refusal the
            // figures do not explain. Not counted either way.
            return None;
        }
        let mut at = i64::MIN;
        for w in windows {
            at = at.max(w?);
        }
        (at <= self.now + self.lookahead).then_some(Ready::Soon(at))
    }
    /// Out for longer than the look-ahead: a limit refusal or a window at its threshold that
    /// does not reset in time.
    fn exhausted(&self, account: &Account, pool: &str) -> bool {
        self.readiness(account, pool).is_none()
            && !self.held.sign_in.contains(&account.id)
            && (self.held.limited.contains(&account.id)
                || blocking(self.snapshot, account, pool).is_some_and(|w| !w.is_empty()))
    }
    /// An account the feed may move at all (DEC-0028's never-moved list): balancing's rule
    /// (enabled, a subscription sign-in, not used elsewhere or on another machine's group, not
    /// given to an agent, not a delegated seat), and not the ordinary CLI's sign-in.
    fn movable(&self, account: &Account) -> bool {
        balancing::usable(self.snapshot, account) && !self.held.in_cli.contains(&account.id)
    }
    /// Ready accounts of `provider` serving `pool`, less those this plan takes away from it.
    fn ready_count(
        &self,
        pool: &str,
        provider: Provider,
        leaving: &HashSet<(String, String)>,
    ) -> usize {
        self.snapshot
            .pool_accounts(pool)
            .into_iter()
            .filter(|a| a.provider == provider && balancing::usable(self.snapshot, a))
            .filter(|a| !leaving.contains(&(pool.to_owned(), a.id.clone())))
            .filter(|a| self.readiness(a, pool).is_some())
            .count()
    }
    /// The group works with `provider`: an account of it serves the group or was fed to it, or
    /// it had requests on it in the last 6 hours.
    fn uses(&self, pool: &str, provider: Provider) -> bool {
        self.snapshot
            .pool_accounts(pool)
            .iter()
            .any(|a| a.provider == provider)
            || self
                .settings
                .lent
                .iter()
                .any(|l| l.feed && l.to == pool && l.provider == provider)
            || self
                .signals
                .get(&(pool.to_owned(), provider.as_str()))
                .is_some_and(|s| s.requests_6h > 0)
    }
    fn parked(&self, id: &str) -> bool {
        self.settings
            .lent
            .iter()
            .any(|l| l.feed && l.account == id && l.to == DEFAULT_POOL)
    }
    fn figures(&self, fed: &Fed, ready: usize, want: usize, why: Option<Ready>) -> FeedFigures {
        FeedFigures {
            priority: fed.priority,
            ready,
            want,
            used_percent: match why {
                Some(Ready::Room(u)) => Some(u),
                _ => None,
            },
            resets_in: match why {
                Some(Ready::Soon(at)) => Some((at - self.now).max(0)),
                _ => None,
            },
        }
    }
}

/// Ranks candidates: room now before a reset, then the most room (and the latest reset, whose
/// room lasts longer), then the soonest reset, then the id.
fn better(a: (&Account, Ready), b: (&Account, Ready)) -> bool {
    use std::cmp::Ordering;
    let order = match (a.1, b.1) {
        (Ready::Room(x), Ready::Room(y)) => x
            .total_cmp(&y)
            .then_with(|| balancing::latest_reset(b.0).cmp(&balancing::latest_reset(a.0))),
        (Ready::Room(_), Ready::Soon(_)) => Ordering::Less,
        (Ready::Soon(_), Ready::Room(_)) => Ordering::Greater,
        (Ready::Soon(x), Ready::Soon(y)) => x.cmp(&y),
    };
    order.then_with(|| a.0.id.cmp(&b.0.id)).is_lt()
}

fn action(
    verb: &'static str,
    account: &Account,
    from: &str,
    to: &str,
    reason: &'static str,
    edit: bool,
    figures: FeedFigures,
) -> Action {
    Action {
        action: verb,
        account: account.id.clone(),
        provider: account.provider,
        from: from.to_owned(),
        to: to.to_owned(),
        reason,
        edit,
        figures: Figures::default(),
        feed: Some(figures),
    }
}
fn give_back(lend: &balancing::Lend, reason: &'static str, edit: bool) -> Action {
    Action {
        action: "return",
        account: lend.account.clone(),
        provider: lend.provider,
        from: lend.from.clone(),
        to: lend.to.clone(),
        reason,
        edit,
        figures: Figures::default(),
        feed: Some(FeedFigures::default()),
    }
}

/// The returns a feed move owes whatever the figures: its account is gone or its share ended
/// outside the feed, a group it touches is not fed any more (`feed_off`) or is switched off, or
/// its account cannot be moved now.
fn forced(view: &View<'_>, lend: &balancing::Lend) -> Option<(&'static str, bool)> {
    let snapshot = view.snapshot;
    let Some(account) = snapshot.accounts.iter().find(|a| a.id == lend.account) else {
        return Some(("account_gone", false));
    };
    if account.pool == lend.to || !snapshot.serves(account, &lend.to) {
        return Some(("share_gone", false));
    }
    let fed = |pool: &str| pool == DEFAULT_POOL || fed_entry(view.settings, pool).is_some();
    if !fed(&lend.to) || !fed(&lend.from) {
        return Some(("feed_off", true));
    }
    let off = |pool: &str| snapshot.project_of_pool(pool).is_some_and(|p| p.off);
    if off(&lend.from) || off(&lend.to) {
        return Some(("group_off", true));
    }
    if account.pool != lend.from
        || !view.movable(account)
        || !snapshot.loan_admits(&lend.to, account)
    {
        return Some(("not_lendable", true));
    }
    None
}

/// Only the returns a change of the fed groups owes at once (turning a group's feed off gives
/// back every account the feed moved in or out of it), with no figures read.
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
        lookahead: lookahead(settings),
    };
    settings
        .lent
        .iter()
        .filter(|l| l.feed)
        .filter_map(|l| forced(&view, l).map(|(r, e)| give_back(l, r, e)))
        .collect()
}

/// The whole decision of one feed pass (DEC-0028), after balancing's own plan, leaving alone the
/// accounts in `busy` (those balancing moves in the same pass).
///
/// 1. Returns: a feed move owed back at once (`forced`); a placement whose account is spent in
///    its fed group goes back to `default` (`spent`, never delayed); a park whose account has room
///    again in its own group ends after its cooldown (`rested`).
/// 2. Parks: each own account of a fed group that is spent rests in `default` (`park`, never
///    delayed by the cooldown).
/// 3. Fill: highest priority first, each fed group is brought to one ready account, then all of
///    them to `ready`, from the reservoir (accounts at home in `default` that are not lent, and
///    parked accounts of other groups), room now first by the most room, then the soonest reset
///    within the look-ahead (`feed`). A group is fed only the providers it works with (any, when
///    it works with none yet).
/// 4. Priority: a fed group still with no ready account takes one from the lowest fed group of a
///    lower priority that has one, even its last: an account the feed placed there moves
///    (`taken`, then `priority`); its own account is shared with it.
///
/// A placement or a priority move waits out the account's cooldown; nothing the person or an
/// agent holds is moved (see `View::movable`), and nothing crosses an organization's boundary.
pub fn plan(
    snapshot: &Snapshot,
    signals: &Signals,
    held: &Held,
    settings: &Settings,
    now: i64,
    busy: &HashSet<String>,
) -> Vec<Action> {
    let view = View {
        snapshot,
        signals,
        held,
        settings,
        now,
        lookahead: lookahead(settings),
    };
    let mut out = Vec::new();
    let mut moving: HashSet<String> = busy.clone();
    let mut leaving: HashSet<(String, String)> = HashSet::new();
    let account_of = |id: &str| snapshot.accounts.iter().find(|a| a.id == id);

    // 1. The feed's own moves.
    for lend in settings.lent.iter().filter(|l| l.feed) {
        if moving.contains(&lend.account) {
            continue;
        }
        let reason = forced(&view, lend).or_else(|| {
            let account = account_of(&lend.account)?;
            if lend.to != DEFAULT_POOL {
                view.exhausted(account, &lend.to).then_some(("spent", true))
            } else {
                (settings.cooled(&lend.account, now)
                    && matches!(view.readiness(account, &lend.from), Some(Ready::Room(_))))
                .then_some(("rested", true))
            }
        });
        if let Some((reason, edit)) = reason {
            out.push(give_back(lend, reason, edit));
            moving.insert(lend.account.clone());
            leaving.insert((lend.to.clone(), lend.account.clone()));
        }
    }
    let order = fed_order(snapshot, settings);
    if order.is_empty() {
        return out;
    }
    let want = ready_target(settings);

    // 2. Parks: a spent own account of a fed group rests in `default`.
    for fed in &order {
        let spent: Vec<&Account> = snapshot
            .accounts
            .iter()
            .filter(|a| {
                a.pool == fed.pool
                    && view.movable(a)
                    && !moving.contains(&a.id)
                    && !settings.lent_out(&a.id)
                    && !snapshot.serves(a, DEFAULT_POOL)
                    && snapshot.loan_admits(DEFAULT_POOL, a)
                    && view.exhausted(a, &fed.pool)
            })
            .collect();
        for a in spent {
            let figures = view.figures(fed, 0, want, None);
            out.push(action(
                "lend",
                a,
                &fed.pool,
                DEFAULT_POOL,
                "park",
                true,
                figures,
            ));
            moving.insert(a.id.clone());
        }
    }

    // 3. Fill from the reservoir.
    let in_reservoir = |a: &Account| {
        view.movable(a)
            && settings.cooled(&a.id, now)
            && ((a.pool == DEFAULT_POOL && !settings.lent_out(&a.id))
                || (view.parked(&a.id)
                    && settings
                        .lent
                        .iter()
                        .filter(|l| l.account == a.id)
                        .all(|l| l.feed && l.to == DEFAULT_POOL)))
    };
    let mut reservoir: Vec<&Account> = snapshot
        .accounts
        .iter()
        .filter(|a| !moving.contains(&a.id) && in_reservoir(a))
        .collect();
    let mut counts: BTreeMap<(String, &'static str), usize> = BTreeMap::new();
    let mut wanted: Vec<(&Fed, Provider)> = Vec::new();
    for fed in &order {
        let any = PROVIDERS.iter().any(|p| view.uses(&fed.pool, *p));
        for p in PROVIDERS {
            let held_here = reservoir.iter().any(|a| a.provider == p);
            if view.uses(&fed.pool, p) || (!any && held_here) {
                wanted.push((fed, p));
                counts.insert(
                    (fed.pool.clone(), p.as_str()),
                    view.ready_count(&fed.pool, p, &leaving),
                );
            }
        }
    }
    for target in [1, want] {
        for (fed, p) in &wanted {
            let key = (fed.pool.clone(), p.as_str());
            while counts[&key] < target {
                let mut best: Option<(usize, &Account, Ready)> = None;
                for (i, &a) in reservoir.iter().enumerate() {
                    if a.provider != *p
                        || snapshot.serves(a, &fed.pool)
                        || !snapshot.loan_admits(&fed.pool, a)
                    {
                        continue;
                    }
                    let Some(ready) = view.readiness(a, &fed.pool) else {
                        continue;
                    };
                    if best.is_none_or(|(_, b, r)| better((a, ready), (b, r))) {
                        best = Some((i, a, ready));
                    }
                }
                let Some((i, account, ready)) = best else {
                    break;
                };
                reservoir.remove(i);
                let figures = view.figures(fed, counts[&key], want, Some(ready));
                out.push(action(
                    "lend",
                    account,
                    &account.pool,
                    &fed.pool,
                    "feed",
                    true,
                    figures,
                ));
                moving.insert(account.id.clone());
                *counts.get_mut(&key).expect("counted") += 1;
            }
        }
    }

    // 4. Priority: a fed group with none takes one from a lower fed group.
    for (high, p) in &wanted {
        let key = (high.pool.clone(), p.as_str());
        if counts[&key] > 0 {
            continue;
        }
        for low in order.iter().rev() {
            if low.priority <= high.priority {
                break;
            }
            let low_key = (low.pool.clone(), p.as_str());
            if counts.get(&low_key).copied().unwrap_or(0) == 0 {
                continue;
            }
            let mut best: Option<(&Account, Ready, bool)> = None;
            for a in snapshot.pool_accounts(&low.pool) {
                if a.provider != *p
                    || !view.movable(a)
                    || moving.contains(&a.id)
                    || !settings.cooled(&a.id, now)
                    || snapshot.serves(a, &high.pool)
                    || !snapshot.loan_admits(&high.pool, a)
                {
                    continue;
                }
                // An account the feed placed there moves; the group's own account, lent nowhere,
                // is shared too; one the person or balancing shared stays where it is.
                let placed = settings
                    .lent
                    .iter()
                    .any(|l| l.feed && l.account == a.id && l.to == low.pool);
                let own = a.pool == low.pool && !settings.lent_out(&a.id);
                if !placed && !own {
                    continue;
                }
                let Some(ready) = view.readiness(a, &high.pool) else {
                    continue;
                };
                if view.readiness(a, &low.pool).is_none() {
                    continue;
                }
                if best.is_none_or(|(b, r, _)| better((a, ready), (b, r))) {
                    best = Some((a, ready, placed));
                }
            }
            let Some((account, ready, placed)) = best else {
                continue;
            };
            let low_figures = view.figures(low, counts[&low_key], want, Some(ready));
            if placed {
                out.push(action(
                    "return",
                    account,
                    &account.pool,
                    &low.pool,
                    "taken",
                    true,
                    low_figures,
                ));
                *counts.get_mut(&low_key).expect("counted") -= 1;
            }
            let figures = view.figures(high, 0, want, Some(ready));
            out.push(action(
                "lend",
                account,
                &account.pool,
                &high.pool,
                "priority",
                true,
                figures,
            ));
            moving.insert(account.id.clone());
            *counts.get_mut(&key).expect("counted") += 1;
            break;
        }
    }
    out
}

/// The person's change: a group's feed on or off with its priority, or the targets. Answers
/// whether anything changed. A group fed for the first time with no priority goes after the
/// fed ones.
pub fn change(
    settings: &mut Settings,
    snapshot: &Snapshot,
    pool: Option<&str>,
    on: Option<bool>,
    priority: Option<u32>,
    ready: Option<u32>,
    lookahead_minutes: Option<u32>,
) -> Result<bool, String> {
    if priority.is_some_and(|p| p == 0 || p > MAX_PRIORITY) {
        return Err(PRIORITY_RANGE.into());
    }
    if ready.is_some_and(|r| r == 0 || r > MAX_READY) {
        return Err(READY_RANGE.into());
    }
    if lookahead_minutes.is_some_and(|m| m > MAX_LOOKAHEAD_MINUTES) {
        return Err(LOOKAHEAD_RANGE.into());
    }
    if (on.is_some() || priority.is_some()) && pool.is_none() {
        return Err(NO_GROUP.into());
    }
    let before = settings.clone();
    if let Some(pool) = pool {
        if pool == DEFAULT_POOL {
            return Err(FEED_DEFAULT.into());
        }
        if !snapshot
            .projects
            .iter()
            .any(|p| p.pool == pool && !p.tokens)
        {
            return Err(balancing::NO_SUCH_GROUP.into());
        }
        match on {
            Some(false) => settings.fed.retain(|f| f.pool != pool),
            Some(true) => {
                let next = settings.fed.iter().map(|f| f.priority).max().unwrap_or(0) + 1;
                match settings.fed.iter_mut().find(|f| f.pool == pool) {
                    Some(f) => {
                        if let Some(p) = priority {
                            f.priority = p;
                        }
                    }
                    None => settings.fed.push(Fed {
                        pool: pool.to_owned(),
                        priority: priority.unwrap_or(next.min(MAX_PRIORITY)),
                    }),
                }
            }
            None => {
                if let Some(p) = priority {
                    let f = settings
                        .fed
                        .iter_mut()
                        .find(|f| f.pool == pool)
                        .ok_or(NOT_FED)?;
                    f.priority = p;
                }
            }
        }
    }
    if let Some(r) = ready {
        settings.ready = (r != DEFAULT_READY).then_some(r);
    }
    if let Some(m) = lookahead_minutes {
        settings.lookahead_minutes = (m != DEFAULT_LOOKAHEAD_MINUTES).then_some(m);
    }
    Ok(*settings != before)
}

/// What `feed status` answers: the fed groups with their priority and ready accounts per
/// provider, every group that can be fed, the reservoir, the accounts the feed moved and the
/// newest feed lines of the journal. Ready counts here read the figures only; the last pass
/// (when an owner runs) also knew limit refusals and the ordinary CLI's sign-in.
pub fn status(
    root: &Path,
    snapshot: &Snapshot,
    settings: &Settings,
    last: Option<&Value>,
    now: i64,
) -> Value {
    let held = Held::default();
    let signals = Signals::new();
    let view = View {
        snapshot,
        signals: &signals,
        held: &held,
        settings,
        now,
        lookahead: lookahead(settings),
    };
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
    let none = HashSet::new();
    let mut fed: Vec<&Fed> = settings.fed.iter().collect();
    fed.sort_by(|a, b| {
        a.priority
            .cmp(&b.priority)
            .then_with(|| a.pool.cmp(&b.pool))
    });
    let fed: Vec<Value> = fed
        .iter()
        .map(|f| {
            let providers: Vec<Value> = PROVIDERS
                .iter()
                .filter(|p| view.uses(&f.pool, **p))
                .map(|p| {
                    json!({"provider": p, "ready": view.ready_count(&f.pool, *p, &none),
                        "accounts": snapshot.pool_accounts(&f.pool).iter()
                            .filter(|a| a.provider == *p).count()})
                })
                .collect();
            let off = snapshot.project_of_pool(&f.pool).is_some_and(|p| p.off);
            json!({"pool": f.pool, "name": name(&f.pool), "priority": f.priority,
                "off": off, "providers": providers})
        })
        .collect();
    let available: Vec<Value> = snapshot
        .projects
        .iter()
        .filter(|p| !p.tokens)
        .map(|p| {
            let entry = fed_entry(settings, &p.pool);
            json!({"pool": p.pool, "name": p.name, "fed": entry.is_some(),
                "priority": entry.map(|f| f.priority)})
        })
        .collect();
    let reservoir: Vec<Value> = PROVIDERS
        .iter()
        .map(|p| {
            let own: Vec<&Account> = snapshot
                .accounts
                .iter()
                .filter(|a| a.provider == *p && a.pool == DEFAULT_POOL && view.movable(a))
                .collect();
            let lent = own.iter().filter(|a| settings.lent_out(&a.id)).count();
            let ready = own
                .iter()
                .filter(|a| !settings.lent_out(&a.id))
                .filter(|a| view.readiness(a, DEFAULT_POOL).is_some())
                .count();
            let parked = settings
                .lent
                .iter()
                .filter(|l| l.feed && l.to == DEFAULT_POOL && l.provider == *p)
                .count();
            json!({"provider": p, "accounts": own.len(), "placed": lent, "ready": ready,
                "parked": parked})
        })
        .collect();
    let moved: Vec<Value> = settings
        .lent
        .iter()
        .filter(|l| l.feed)
        .map(|l| {
            json!({"account_id": l.account, "label": label(&l.account), "provider": l.provider,
                "kind": if l.to == DEFAULT_POOL { "parked" } else { "placed" },
                "from": l.from, "from_name": name(&l.from), "to": l.to, "to_name": name(&l.to),
                "at": l.at})
        })
        .collect();
    let recent: Vec<Value> = balancing::read_journal(root)
        .iter()
        .rev()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .filter(|l| l.get("feed").is_some())
        .take(RECENT)
        .collect();
    json!({"on": !settings.fed.is_empty(), "ready": ready_target(settings),
        "lookahead_minutes": lookahead(settings) / 60,
        "pass_minutes": balancing::PASS_SECONDS / 60, "fed": fed, "available": available,
        "reservoir": reservoir, "moved": moved, "recent": recent,
        "last_pass": last.cloned().unwrap_or(Value::Null)})
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::balancing::{Lend, Settings, COOLDOWN_SECONDS};
    use switchboard_core::{AuthKind, Project, Usage, UsageHealth, UsageWindow};

    const NOW: i64 = 1_800_000_000;
    /// A Claude account at `used` percent whose window resets in `reset` seconds.
    fn account(id: &str, pool: &str, used: f64, reset: i64) -> Account {
        Account {
            id: id.into(),
            label: id.into(),
            provider: Provider::Claude,
            kind: AuthKind::OAuth,
            pool: pool.into(),
            enabled: true,
            created_at: 1,
            identity: None,
            usage: Some(Usage {
                windows: vec![UsageWindow {
                    name: "five_hour".into(),
                    used_percent: used,
                    resets_at: Some(NOW + reset),
                }],
                used_percent: used,
                observed_at: NOW - 30,
                resets_at: Some(NOW + reset),
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
    fn fed(list: &[(&str, u32)]) -> Settings {
        Settings {
            fed: list
                .iter()
                .map(|(p, n)| Fed {
                    pool: (*p).into(),
                    priority: *n,
                })
                .collect(),
            ..Settings::default()
        }
    }
    fn lend(account: &str, from: &str, to: &str) -> Lend {
        Lend {
            account: account.into(),
            provider: Provider::Claude,
            from: from.into(),
            to: to.into(),
            at: NOW - 2 * COOLDOWN_SECONDS,
            feed: true,
        }
    }
    fn run(s: &Snapshot, settings: &Settings, held: &Held) -> Vec<Action> {
        plan(s, &Signals::new(), held, settings, NOW, &HashSet::new())
    }
    fn moves(actions: &[Action]) -> Vec<(&str, &str, &str, &str, &str)> {
        actions
            .iter()
            .map(|a| {
                (
                    a.action,
                    a.account.as_str(),
                    a.from.as_str(),
                    a.to.as_str(),
                    a.reason,
                )
            })
            .collect()
    }
    /// Agents (one spent account) fed; the default group holds three with room.
    fn estate() -> Snapshot {
        Snapshot {
            accounts: vec![
                account("agents-1", "agents", 99.0, 4 * 3600),
                account("d-1", "default", 40.0, 3600),
                account("d-2", "default", 10.0, 3600),
                account("d-3", "default", 70.0, 3600),
            ],
            projects: vec![group("agents"), group("apps")],
            ..Snapshot::default()
        }
    }

    #[test]
    fn no_group_fed_plans_nothing_and_writes_no_new_field() {
        let s = estate();
        assert!(run(&s, &Settings::default(), &Held::default()).is_empty());
        // An older file reads; a settings file without the feed keeps its bytes.
        let bytes = serde_json::to_vec(&Settings::default()).unwrap();
        assert_eq!(bytes, br#"{"groups":[],"all_new":false}"#);
        let old: Settings = serde_json::from_slice(
            br#"{"groups":["apps"],"all_new":true,
            "lent":[{"account":"a","provider":"claude","from":"apps","to":"agents","at":1}]}"#,
        )
        .unwrap();
        assert!(!old.lent[0].feed && old.fed.is_empty() && old.ready.is_none());
    }

    #[test]
    fn a_fed_group_parks_its_spent_account_and_takes_the_reservoirs_best_two() {
        let s = estate();
        let actions = run(&s, &fed(&[("agents", 1)]), &Held::default());
        assert_eq!(
            moves(&actions),
            [
                ("lend", "agents-1", "agents", "default", "park"),
                ("lend", "d-2", "default", "agents", "feed"),
                ("lend", "d-1", "default", "agents", "feed"),
            ],
            "{actions:?}"
        );
        let first = actions[1].feed.as_ref().unwrap();
        assert_eq!((first.ready, first.want, first.priority), (0, 2, 1));
        assert_eq!(first.used_percent, Some(10.0));
        // `ready` 3 takes the third as well.
        let mut settings = fed(&[("agents", 1)]);
        settings.ready = Some(3);
        assert_eq!(run(&s, &settings, &Held::default()).len(), 4);
    }

    #[test]
    fn an_account_resetting_within_the_look_ahead_counts_as_ready() {
        let mut s = estate();
        s.accounts = vec![
            account("agents-1", "agents", 99.0, 20 * 60),
            account("d-1", "default", 99.0, 10 * 60),
            account("d-2", "default", 99.0, 5 * 3600),
        ];
        let actions = run(&s, &fed(&[("agents", 1)]), &Held::default());
        // Agents' own account resets in 20 minutes: ready, not parked. The next is the one
        // resetting in 10 minutes; the one resetting in 5 hours is not ready.
        assert_eq!(
            moves(&actions),
            [("lend", "d-1", "default", "agents", "feed")],
            "{actions:?}"
        );
        assert_eq!(actions[0].feed.as_ref().unwrap().resets_in, Some(600));
        // A look-ahead of 0 counts neither.
        let mut settings = fed(&[("agents", 1)]);
        settings.lookahead_minutes = Some(0);
        let actions = run(&s, &settings, &Held::default());
        assert_eq!(
            moves(&actions),
            [("lend", "agents-1", "agents", "default", "park")],
            "{actions:?}"
        );
    }

    #[test]
    fn higher_priority_is_served_first_and_each_gets_one_before_any_gets_two() {
        let mut s = estate();
        s.accounts = vec![
            account("d-1", "default", 10.0, 3600),
            account("d-2", "default", 20.0, 3600),
            account("d-3", "default", 30.0, 3600),
        ];
        let actions = run(&s, &fed(&[("apps", 2), ("agents", 1)]), &Held::default());
        assert_eq!(
            moves(&actions),
            [
                ("lend", "d-1", "default", "agents", "feed"),
                ("lend", "d-2", "default", "apps", "feed"),
                ("lend", "d-3", "default", "agents", "feed"),
            ],
            "{actions:?}"
        );
        // One account for two groups: the higher one has it.
        s.accounts.truncate(1);
        let actions = run(&s, &fed(&[("apps", 2), ("agents", 1)]), &Held::default());
        assert_eq!(
            moves(&actions),
            [("lend", "d-1", "default", "agents", "feed")]
        );
    }

    #[test]
    fn a_higher_group_with_none_takes_a_lower_groups_last_account() {
        let mut s = estate();
        s.accounts = vec![
            account("agents-1", "agents", 99.0, 5 * 3600),
            account("d-1", "default", 10.0, 3600),
        ];
        s.projects[1].shared_accounts.push("d-1".into());
        let mut settings = fed(&[("agents", 1), ("apps", 2)]);
        settings.lent = vec![lend("d-1", "default", "apps")];
        let actions = run(&s, &settings, &Held::default());
        assert_eq!(
            moves(&actions),
            [
                ("lend", "agents-1", "agents", "default", "park"),
                ("return", "d-1", "default", "apps", "taken"),
                ("lend", "d-1", "default", "agents", "priority"),
            ],
            "{actions:?}"
        );
        // Never upwards: the lower group with none takes nothing from the higher one.
        let mut settings = fed(&[("agents", 2), ("apps", 1)]);
        settings.lent = vec![lend("d-1", "default", "apps")];
        let actions = run(&s, &settings, &Held::default());
        assert!(actions.iter().all(|a| a.reason != "taken"), "{actions:?}");
    }

    #[test]
    fn a_spent_placement_goes_back_and_a_rested_park_comes_home() {
        let mut s = estate();
        s.accounts = vec![
            account("agents-1", "agents", 20.0, 3600),
            account("agents-2", "agents", 30.0, 3600),
            account("d-1", "default", 99.0, 5 * 3600),
        ];
        s.projects[0].shared_accounts.push("d-1".into());
        s.default_shared.push("agents-1".into());
        let mut settings = fed(&[("agents", 1)]);
        settings.lent = vec![
            lend("d-1", "default", "agents"),
            lend("agents-1", "agents", "default"),
        ];
        let actions = run(&s, &settings, &Held::default());
        assert_eq!(
            moves(&actions),
            [
                ("return", "d-1", "default", "agents", "spent"),
                ("return", "agents-1", "agents", "default", "rested"),
            ],
            "{actions:?}"
        );
        // A spent placement goes back within its cooldown; a park ends only after it.
        settings.moved = [("d-1".to_owned(), NOW), ("agents-1".to_owned(), NOW)].into();
        let actions = run(&s, &settings, &Held::default());
        assert_eq!(
            moves(&actions),
            [("return", "d-1", "default", "agents", "spent")]
        );
    }

    #[test]
    fn a_park_is_never_delayed_and_a_placement_waits_its_cooldown() {
        let mut s = estate();
        let mut settings = fed(&[("agents", 1)]);
        settings.moved = [
            ("agents-1".to_owned(), NOW - 60),
            ("d-1".to_owned(), NOW - 60),
            ("d-2".to_owned(), NOW - 60),
        ]
        .into();
        let actions = run(&s, &settings, &Held::default());
        assert_eq!(
            moves(&actions),
            [
                ("lend", "agents-1", "agents", "default", "park"),
                ("lend", "d-3", "default", "agents", "feed"),
            ],
            "{actions:?}"
        );
        // Balancing moves d-3 in the same pass: the feed leaves it alone.
        s.accounts.truncate(3);
        let busy: HashSet<String> = ["agents-1".to_owned()].into();
        let actions = plan(&s, &Signals::new(), &Held::default(), &settings, NOW, &busy);
        assert!(actions.is_empty(), "{actions:?}");
    }

    #[test]
    fn accounts_that_must_never_be_moved_stay() {
        let base = estate();
        let settings = fed(&[("agents", 1)]);
        // Disabled, given to an agent, on another machine, or the ordinary CLI's sign-in.
        let mut s = base.clone();
        for a in s.accounts.iter_mut() {
            a.enabled = false;
        }
        assert!(run(&s, &settings, &Held::default()).is_empty());
        let mut s = base.clone();
        for a in &base.accounts {
            s.assigned_to.insert(a.id.clone(), "agent".into());
        }
        assert!(run(&s, &settings, &Held::default()).is_empty());
        let mut s = base.clone();
        for a in &base.accounts {
            s.elsewhere.insert(a.id.clone(), NOW);
        }
        assert!(run(&s, &settings, &Held::default()).is_empty());
        let held = Held {
            in_cli: base.accounts.iter().map(|a| a.id.clone()).collect(),
            ..Held::default()
        };
        assert!(run(&base, &settings, &held).is_empty());
        // A sign-in that waits is never placed; the default group's other accounts are.
        let held = Held {
            sign_in: ["d-2".to_owned()].into(),
            ..Held::default()
        };
        let actions = run(&base, &settings, &held);
        assert!(actions.iter().all(|a| a.account != "d-2"), "{actions:?}");
        // A group switched off, or the default group itself, is never fed.
        let mut s = base.clone();
        s.projects[0].off = true;
        assert!(run(&s, &settings, &Held::default()).is_empty());
    }

    #[test]
    fn turning_a_groups_feed_off_returns_every_move_in_and_out_of_it() {
        let mut s = estate();
        s.projects[0].shared_accounts.push("d-1".into());
        s.default_shared.push("agents-1".into());
        let mut settings = fed(&[("agents", 1), ("apps", 2)]);
        settings.lent = vec![
            lend("d-1", "default", "agents"),
            lend("agents-1", "agents", "default"),
        ];
        let changed = change(
            &mut settings,
            &s,
            Some("agents"),
            Some(false),
            None,
            None,
            None,
        )
        .unwrap();
        assert!(changed);
        let owed = returns_owed(&s, &Held::default(), &settings, NOW);
        assert_eq!(
            moves(&owed),
            [
                ("return", "d-1", "default", "agents", "feed_off"),
                ("return", "agents-1", "agents", "default", "feed_off"),
            ]
        );
        // Balancing's own loans are not the feed's to return.
        settings.lent.push(Lend {
            feed: false,
            ..lend("d-2", "default", "apps")
        });
        assert_eq!(returns_owed(&s, &Held::default(), &settings, NOW).len(), 2);
    }

    #[test]
    fn the_persons_change_is_checked() {
        let s = estate();
        let mut settings = Settings::default();
        let bad = |settings: &mut Settings, pool, on, priority, ready, ahead| {
            change(settings, &s, pool, on, priority, ready, ahead).unwrap_err()
        };
        assert_eq!(
            bad(&mut settings, Some("default"), Some(true), None, None, None),
            FEED_DEFAULT
        );
        assert_eq!(
            bad(&mut settings, Some("nope"), Some(true), None, None, None),
            balancing::NO_SUCH_GROUP
        );
        assert_eq!(
            bad(&mut settings, Some("agents"), None, Some(1), None, None),
            NOT_FED
        );
        assert_eq!(
            bad(
                &mut settings,
                Some("agents"),
                Some(true),
                Some(0),
                None,
                None
            ),
            PRIORITY_RANGE
        );
        assert_eq!(
            bad(&mut settings, None, None, None, Some(11), None),
            READY_RANGE
        );
        assert_eq!(
            bad(&mut settings, None, None, None, None, Some(721)),
            LOOKAHEAD_RANGE
        );
        assert_eq!(
            bad(&mut settings, None, Some(true), None, None, None),
            NO_GROUP
        );
        // A new group goes after the fed ones; the same input again changes nothing.
        let on = |settings: &mut Settings, pool, priority| {
            change(settings, &s, Some(pool), Some(true), priority, None, None).unwrap()
        };
        assert!(on(&mut settings, "agents", None));
        assert!(on(&mut settings, "apps", None));
        assert!(!on(&mut settings, "apps", None));
        assert_eq!(
            settings.fed,
            [
                Fed {
                    pool: "agents".into(),
                    priority: 1
                },
                Fed {
                    pool: "apps".into(),
                    priority: 2
                }
            ]
        );
        assert!(change(&mut settings, &s, Some("apps"), None, Some(1), None, None).unwrap());
        assert!(change(&mut settings, &s, None, None, None, Some(3), Some(45)).unwrap());
        assert_eq!(
            (settings.ready, settings.lookahead_minutes),
            (Some(3), Some(45))
        );
        // The defaults are stored as absent.
        assert!(change(&mut settings, &s, None, None, None, Some(2), Some(30)).unwrap());
        assert_eq!((settings.ready, settings.lookahead_minutes), (None, None));
        // A deleted group is fed no more.
        let mut gone = s.clone();
        gone.projects.retain(|p| p.pool != "apps");
        assert!(balancing::resolve(&mut settings, &gone));
        assert_eq!(settings.fed.len(), 1);
    }

    #[test]
    fn no_account_crosses_an_organizations_boundary() {
        let mut s = estate();
        s.organizations.push(
            serde_json::from_value(json!({"id": "acme", "name": "Acme", "created_at": 1})).unwrap(),
        );
        s.projects[0].organizations = vec!["acme".into()];
        s.memberships.insert("agents-1".into(), "acme".into());
        // The default group's accounts are personal: Acme's group takes none, and Acme's spent
        // account does not rest in the default group, which is no organization's.
        let actions = run(&s, &fed(&[("agents", 1)]), &Held::default());
        assert!(actions.is_empty(), "{actions:?}");
        // Once they work for Acme too, they are placed.
        for id in ["d-1", "d-2", "d-3"] {
            s.memberships.insert(id.into(), "acme".into());
        }
        let actions = run(&s, &fed(&[("agents", 1)]), &Held::default());
        assert_eq!(actions.len(), 2, "{actions:?}");
        assert!(actions.iter().all(|a| a.reason == "feed"), "{actions:?}");
    }

    #[test]
    fn status_lists_the_fed_groups_the_reservoir_and_the_moves() {
        let root = tempfile::tempdir().unwrap();
        let mut s = estate();
        s.projects[0].shared_accounts.push("d-1".into());
        let mut settings = fed(&[("agents", 1)]);
        settings.lent = vec![lend("d-1", "default", "agents")];
        let view = status(root.path(), &s, &settings, None, NOW);
        assert_eq!(view["on"], true);
        assert_eq!(
            (view["ready"].clone(), view["lookahead_minutes"].clone()),
            (json!(2), json!(30))
        );
        assert_eq!(view["fed"][0]["pool"], "agents");
        assert_eq!(view["fed"][0]["providers"][0]["ready"], 1, "{view}");
        assert_eq!(view["available"][1]["fed"], false);
        assert_eq!(view["moved"][0]["kind"], "placed");
        assert_eq!(view["reservoir"][0]["accounts"], 3);
        assert_eq!(view["reservoir"][0]["placed"], 1);
        assert_eq!(view["recent"], json!([]));
    }

    /// The owner's pass: a fed group out of room gets the default group's two accounts with
    /// room and rests its spent one in `default`; the moves are journaled with their reasons,
    /// and turning the feed off gives every one of them back.
    #[tokio::test]
    async fn a_pass_feeds_a_group_and_turning_the_feed_off_gives_everything_back() {
        use crate::fixtures::*;
        use crate::Operation;
        let root = tempfile::tempdir().unwrap();
        let runtime = runtime(root.path(), signed_out, activates).await;
        let work = tempfile::tempdir().unwrap();
        let now = crate::monitor::now();
        let observe = |id: &str, used: f64, reset: i64| {
            runtime
                .store
                .observe(
                    id,
                    Usage {
                        windows: vec![UsageWindow {
                            name: "five_hour".into(),
                            used_percent: used,
                            resets_at: Some(now + reset),
                        }],
                        used_percent: used,
                        observed_at: now,
                        resets_at: Some(now + reset),
                        source: "claude_oauth".into(),
                    },
                )
                .unwrap();
            runtime
                .store
                .usage_health(id, "ok", now, now + 150)
                .unwrap();
        };
        let spent = save(&runtime.store, "Agents-0", "default");
        observe(&spent.id, 99.0, 5 * 3600);
        let mut reservoir = Vec::new();
        for (n, used) in [10.0, 20.0, 30.0].iter().enumerate() {
            let a = save(&runtime.store, &format!("Default-{n}"), "default");
            observe(&a.id, *used, 3600);
            reservoir.push(a.id);
        }
        let folder = work.path().canonicalize().unwrap().join("Agents");
        std::fs::create_dir_all(&folder).unwrap();
        let view = runtime
            .execute(Operation::SaveProject {
                pool: None,
                name: "Agents".into(),
                folders: vec![folder],
                account_ids: vec![spent.id.clone()],
                base_folders: None,
                organization: None,
                organizations: None,
                move_accounts: false,
            })
            .await
            .unwrap();
        let agents = view["pool"].as_str().unwrap().to_owned();
        let on = runtime
            .execute(Operation::Feed {
                pool: Some(agents.clone()),
                on: Some(true),
                priority: None,
                ready: None,
                lookahead_minutes: None,
            })
            .await
            .unwrap();
        assert_eq!(
            (on["on"].clone(), on["changed"].clone()),
            (json!(true), json!(true))
        );
        assert_eq!(on["fed"][0]["priority"], 1, "{on}");
        runtime.balance_pass(now).await;
        let snapshot = runtime.store.snapshot().unwrap();
        let shared = &snapshot.project_of_pool(&agents).unwrap().shared_accounts;
        assert_eq!(shared, &reservoir[..2], "the two with the most room");
        assert_eq!(
            snapshot.default_shared,
            std::slice::from_ref(&spent.id),
            "parked"
        );
        let status = runtime.execute(Operation::FeedStatus).await.unwrap();
        assert_eq!(status["fed"][0]["providers"][0]["ready"], 2, "{status}");
        assert_eq!(status["moved"].as_array().unwrap().len(), 3, "{status}");
        let reasons: Vec<&str> = status["recent"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| l["reason"].as_str().unwrap())
            .collect();
        assert_eq!(reasons, ["feed", "feed", "park"], "newest first: {status}");
        assert_eq!(status["recent"][0]["feed"]["want"], 2);
        assert_eq!(status["recent"][0]["outcome"], "done");
        // Balancing's own state shows none of the feed's moves as loans.
        let balance = runtime.execute(Operation::BalanceStatus).await.unwrap();
        assert_eq!(balance["lent"], json!([]), "{balance}");
        // Off gives back every account the feed moved.
        let off = runtime
            .execute(Operation::Feed {
                pool: Some(agents.clone()),
                on: Some(false),
                priority: None,
                ready: None,
                lookahead_minutes: None,
            })
            .await
            .unwrap();
        assert_eq!(off["on"], false);
        assert_eq!(off["moved"], json!([]), "{off}");
        let snapshot = runtime.store.snapshot().unwrap();
        assert!(snapshot
            .project_of_pool(&agents)
            .unwrap()
            .shared_accounts
            .is_empty());
        assert!(snapshot.default_shared.is_empty());
        assert_eq!(off["recent"][0]["reason"], "feed_off");
        // Nothing fed, nothing lent, nothing chosen: a pass reads and writes nothing.
        let before = std::fs::read(root.path().join(balancing::FILE)).unwrap();
        runtime.balance_pass(now + 3600).await;
        assert_eq!(
            std::fs::read(root.path().join(balancing::FILE)).unwrap(),
            before
        );
    }
}
