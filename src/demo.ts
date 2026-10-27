import { isAbsoluteProjectPath } from './platform';
import type { StatCounts, Statistics } from './ui-logic';
import { attribution, groupAdmits, providerOrganization } from './ui-logic';
import agentCatalogData from '../catalog/agents.json';
import { version } from '../package.json';
import type { PassioncodeStatus, Ceilings, CeilingView, CeilingQuestionView, Account, OrganizationGroupOffer, Adapter, AgentSetup, CurrentAccounts, ExternalIdentity, HermesModel, KimiAccounts, LoginInput, OpenrouterStatus, ProjectSuggestion, Snapshot, UpdateStatus, AgentRow, AgentsOverview, RecommendedModel, Chain, ChainExecutor, ChainScope, ChainStepView, LastHandover, ShellHookFile, ShellHookStatus, Onboarding, OnboardStep } from './types';

// Imported only after an explicit browser-only ?demo=1. No credentials are kept,
// no network/CLI/vault calls exist, and all state disappears on page reload.
const PROJECT_NOT_NATIVE = "This account belongs to a project. The ordinary Claude Code serves every folder, so a project's accounts are used only by sessions launched from the project's folders.";
export function createDemoAdapter(): Adapter {
  const now = () => Math.floor(Date.now() / 1000);
  const studio: ExternalIdentity = { account_id: 'synthetic-studio', organization_id: 'synthetic-work', email: 'studio@example.test' };
  const codexIdentity: ExternalIdentity = { account_id: 'synthetic-codex', organization_id: 'synthetic-work', email: 'engineering@example.test' };
  // The Codex CLI is signed in to an account Switchboard has not saved yet: one click adds it.
  const codexUnsaved: ExternalIdentity = { account_id: 'synthetic-codex-new', organization_id: 'synthetic-work', email: 'release@example.test' };
  const current: CurrentAccounts = {
    claude: { status: 'available', identity: studio, account_id: 'demo-claude-work' },
    codex: { status: 'available', identity: codexUnsaved, account_id: null },
  };
  const state: Snapshot = {
    accounts: [
      { id: 'demo-claude-work', label: 'Studio', provider: 'claude', kind: 'oauth', pool: 'work', enabled: true, created_at: now(), identity: null, usage: { used_percent: 32, observed_at: now() - 95, resets_at: now() + 10800, source: 'Synthetic fixture' } },
      { id: 'demo-codex-work', label: 'Engineering', provider: 'codex', kind: 'oauth', pool: 'work', enabled: true, created_at: now(), identity: null, usage: { used_percent: 68, observed_at: now() - 140, resets_at: now() + 7200, source: 'Synthetic fixture' } },
      { id: 'demo-claude-personal', label: 'Personal', provider: 'claude', kind: 'setup_token', pool: 'personal', enabled: true, created_at: now(), identity: null, usage: null },
      { id: 'demo-codex-lab', label: 'Experiments', provider: 'codex', kind: 'api_key', pool: 'personal', enabled: true, created_at: now(), identity: null, usage: null },
      { id: 'demo-claude-archive', label: 'Previous workspace', provider: 'claude', kind: 'oauth', pool: 'work', enabled: false, created_at: now(), identity: null, usage: { used_percent: 86, observed_at: now() - 7200, resets_at: now() - 600, source: 'Synthetic fixture' } },
      ...['north', 'south', 'east', 'west', 'harbor', 'summit', 'meadow'].map((name, index): Account => ({ id: `demo-claude-${name}`, label: `${name[0].toUpperCase()}${name.slice(1)} team`, provider: 'claude', kind: 'oauth', pool: index < 5 ? 'work' : 'personal', enabled: true, created_at: now() + index + 1, identity: null, external_identity: { account_id: `synthetic-${name}`, organization_id: 'synthetic-work', email: `${name}@example.test` }, usage: index === 6 ? null : { used_percent: [12, 47, 91, 64, 5, 33][index], observed_at: now() - 60 * (index + 1), resets_at: now() + 3600 * (index + 1), source: 'Synthetic fixture' } })),
    ],
    policies: [{ provider: 'claude', pool: 'work', target: 'managed', enabled: false, threshold_percent: 90, hysteresis_percent: 10, cooldown_seconds: 1800, max_age_seconds: 300, last_switched_at: null }],
    routes: { 'claude:work': 'demo-claude-work', 'claude:personal': 'demo-claude-summit', 'codex:work': 'demo-codex-work' },
    events: [{ at: now() - 95, action: 'usage', account_id: 'demo-claude-work', detail: 'observed' }],
    rules: [
      { path: '/srv/projects/alpha-web', provider: 'claude', account_id: 'demo-claude-work', target: 'managed', enabled: true, created_at: now() - 3600, expires_at: now() + 6 * 3600 },
      { path: '/srv/projects/beta-api', provider: 'codex', account_id: 'demo-codex-work', target: 'managed', enabled: false, created_at: now() - 86400, expires_at: null },
    ],
    // SB-135: a project whose main checkout has two worktrees beside it (shown under it).
    projects: [{ pool: 'alpha-web', name: 'Alpha Web', folders: ['/srv/projects/.worktrees/alpha-web-wt-login', '/srv/projects/alpha-web', '/srv/projects/.worktrees/alpha-web-wt-pricing', '/srv/projects/alpha-docs'], created_at: now() - 86400 }, { pool: 'tools', name: 'Tools', folders: ['/srv/projects/tools-cli', '/srv/projects/alpha-docs'], created_at: now() - 3600 }],
  };
  const hermes: HermesModel = { installed: true, configured: true, model: 'deepseek/deepseek-v4-pro-0813', provider: 'openrouter', base_url: 'https://openrouter.ai/api/v1', api_mode: 'chat_completions', providers: ['openrouter', 'anthropic', 'nous', 'openai-codex', 'kimi-coding', 'deepseek', 'gemini', 'xai', 'auto'], error: null };
  const kimi: KimiAccounts = { installed: true, current: { signed_in: true, nickname: 'Demo', tier: 'Allegretto', windows: [{ name: '5h', used_percent: 18, resets_at: now() + 9000 }, { name: 'month', used_percent: 42, resets_at: now() + 1_200_000 }], error: null, region: 'global', checked_at: now() - 40 }, accounts: [{ account: { id: '00000000-0000-4000-8000-0000000000a1', label: 'Kimi · work', region: 'global', added_at: now() - 86_400, nickname: 'Demo', tier: 'Allegretto' }, status: { signed_in: true, nickname: 'Demo', tier: 'Allegretto', windows: [{ name: '5h', used_percent: 64, resets_at: now() + 4200 }, { name: '7d', used_percent: 31, resets_at: now() + 300_000 }, { name: 'month', used_percent: 42, resets_at: now() + 1_200_000 }], error: null, checked_at: now() - 40 } }] };
  // SB-144: project pools with an OpenRouter key of their own (values never kept in the demo).
  const projectKeys = new Map<string, { saved_at: number; model: string | null }>();
  const openrouter: OpenrouterStatus = { service: 'openrouter', saved: false, saved_at: null, model: null, credit: null, credit_error: null };
  // C-7 (SB-72): every state of the spending ceilings by `?ceilings=` (set, reached, failed);
  // synthetic only. `set` and `reached` save a machine key (daily limit $10) and a key of its own
  // for Alpha Web (monthly limit $20, so its warning shows), with a ceiling on each and on one task.
  const ceilingsParam = new URLSearchParams(location.search).get('ceilings');
  let demoCeilings: CeilingView[] = [];
  let demoQuestions: CeilingQuestionView[] = [];
  const ceilingView = (scope: ChainScope, cents: number, doubledFrom: number | null = null): CeilingView => ({ scope, cents, usd: cents / 100, set_at: now() - 7200, doubled_from: doubledFrom, active: true, inactive_reason: null });
  if (ceilingsParam === 'set' || ceilingsParam === 'reached') {
    Object.assign(openrouter, { saved: true, saved_at: now() - 86_400, model: 'xiaomi/mimo-v2.6-pro', credit: { label: 'demo', limit: 10, limit_remaining: 8.8, limit_reset: 'daily', usage: 31.2, usage_daily: 1.2, is_free_tier: false }, credit_error: null });
    projectKeys.set('alpha-web', { saved_at: now() - 3600, model: 'xiaomi/mimo-v2.6-pro' });
    demoCeilings = [ceilingView({ kind: 'machine' }, 1000), ceilingView({ kind: 'project', pool: 'alpha-web' }, 300), ceilingView({ kind: 'workflow', id: 'wf_0123456789abcdef' }, 200)];
  }
  if (ceilingsParam === 'reached') demoQuestions = [
    { id: 'q_0123456789abcdef', scope: { kind: 'machine' }, cents: 1000, usd: 10, double_usd: 20, spent: 10.04, reached_at: now() - 1500 },
    { id: 'q_fedcba9876543210', scope: { kind: 'project', pool: 'alpha-web' }, cents: 300, usd: 3, double_usd: 6, spent: 3.1, reached_at: now() - 600 },
  ];
  const setup: AgentSetup = { cli_path: null, bundled_cli: '/Applications/Fabric Switchboard.app/Contents/MacOS/switchboard', linked_cli: null, can_link: true, commands: { claude_code: 'claude mcp add --scope user switchboard -- switchboard mcp', codex: 'codex mcp add switchboard -- switchboard mcp', claude_plugin: 'claude plugin marketplace add passioncode-ai/fabric-switchboard && claude plugin install switchboard@switchboard' }, connected: { claude_code: true, codex: false }, agent_prompt: 'Set up Fabric Switchboard on this computer: run `switchboard onboard` and follow it.', proved: { client: 'claude-code', at: now() - 3600 } };
  state.accounts[0].external_identity = studio;
  state.accounts[1].external_identity = codexIdentity;
  for (const item of state.accounts) {
    if (item.usage) item.usage.windows = [
      { name: 'Session', used_percent: item.usage.used_percent, resets_at: item.usage.resets_at },
      { name: 'Weekly', used_percent: Math.max(0, item.usage.used_percent - 14), resets_at: now() + 172800 },
    ];
    item.usage_health = { status: item.id.endsWith('archive') ? 'failed' : item.kind === 'oauth' ? 'ok' : 'unavailable', checked_at: now() - 95, next_check_at: now() + 85 };
  }
  const log = (action: string, id: string, detail: string) => { state.events.push({ at: now(), action, account_id: id, detail }); state.events = state.events.slice(-100); };
  const pause = () => new Promise<void>((resolve) => setTimeout(resolve, 220));
  const account = (id: string) => { const found = state.accounts.find((item) => item.id === id); if (!found) throw new Error('Account unavailable'); return found; };
  const enabled = (id: string) => { const found = account(id); if (!found.enabled) throw new Error('This account is disabled. Enable it before continuing.'); return found; };
  const logins = new Map<string, LoginInput & { started: number }>();
  let signIns = 0;
  // SB-160 (B-4, SCN-077), like the owner's `organization_offer`: the one group of the account's
  // organization when it landed outside it; none for a personal account, an organization with no
  // group or several, an account already there, or an identity the group already holds.
  const organizationOffer = (item: Account): OrganizationGroupOffer | undefined => {
    const organization = attribution(state, item).organization;
    if (!organization) return undefined;
    const bound = (state.projects ?? []).filter((p) => p.organization === organization);
    if (bound.length !== 1 || bound[0].pool === item.pool) return undefined;
    const id = item.external_identity;
    const held = !!id?.account_id && state.accounts.some((a) => a.pool === bound[0].pool && a.provider === item.provider && a.external_identity?.account_id === id.account_id && a.external_identity?.organization_id === id.organization_id);
    return held ? undefined : { pool: bound[0].pool, name: bound[0].name };
  };
  let passioncode: PassioncodeStatus = { connected: false, base: 'https://org.passioncode.ai' };
  const rateLimited = new Set<string>();
  // Explicit browser-only acceptance fixtures for ordering, multi-window waits and failure.
  if (new URLSearchParams(location.search).get('quota-review') === '1') {
    for (const id of ['demo-claude-east', 'demo-claude-west']) {
      const item = state.accounts.find(a => a.id === id)!;
      const until = now() + (id.endsWith('east') ? 7200 : 600);
      item.usage = { used_percent: 100, observed_at: now(), resets_at: until, source: 'Synthetic quota review', windows: [{ name: 'Session', used_percent: 100, resets_at: now() + 60 }, { name: 'Weekly', used_percent: 100, resets_at: until }] };
    }
    state.accounts.find(a => a.id === 'demo-claude-south')!.usage_health!.status = 'failed';
    // SB-39: the provider asked for a 15-minute wait before any check of this row.
    const waiting = state.accounts.find(a => a.id === 'demo-claude-harbor')!;
    waiting.usage = null;
    waiting.usage_health = { status: 'failed', checked_at: now() - 30, next_check_at: now() + 870 };
    rateLimited.add(waiting.id);
    // SB-40: one Codex feature is exhausted while the account itself has room.
    const codex = state.accounts.find(a => a.id === 'demo-codex-work')!;
    codex.usage!.windows = [
      { name: 'primary', used_percent: codex.usage!.used_percent, resets_at: now() + 7200 },
      { name: 'secondary', used_percent: 41, resets_at: now() + 432000 },
      { name: 'feature_codex_other_primary', used_percent: 100, resets_at: now() + 900 },
    ];
    codex.usage!.used_percent = 100;
  }
  const signInRequired = new Set(['demo-claude-meadow']);
  // SB-160 (B-6, B-7): `?org=ended` shows the seats of Northwind ended (account cards name it, its
  // group's card says none is signed in) and Contoso left on PassionCode.ai three days ago.
  if (new URLSearchParams(location.search).get('org') === 'ended') {
    const northwind = { id: 'demo-org-northwind', name: 'Northwind', domains: ['northwind.test'], created_at: now() - 30 * 86_400 };
    const contoso = { id: 'demo-org-contoso', name: 'Contoso', domains: ['contoso.test'], created_at: now() - 60 * 86_400, vesta_id: null };
    state.organizations = [northwind, contoso];
    for (const [id, provider, email] of [['demo-claude-northwind', 'claude', 'dev@northwind.test'], ['demo-codex-northwind', 'codex', 'ops@northwind.test']] as const) {
      state.accounts.unshift({ id, label: email, provider, kind: 'oauth', pool: 'northwind', enabled: true, created_at: now() - 86_400, identity: null, external_identity: { account_id: `synthetic-${id}`, organization_id: null, email }, usage: null });
      signInRequired.add(id);
    }
    (state.projects ??= []).unshift({ pool: 'northwind', name: 'Northwind web', folders: ['/srv/projects/northwind-web'], created_at: now() - 20 * 86_400, organization: northwind.id });
    passioncode = { connected: true, base: 'https://org.passioncode.ai', state: 'ok', connected_at: now() - 40 * 86_400, read_at: now() - 3600, checked_at: now() - 3600, organizations: 0, released: [{ organization: contoso.id, at: now() - 3 * 86_400 }] };
  }
  // SB-202 (B-8, SCN-076): `?org=setup` is a new computer of Northwind's: connected to
  // PassionCode.ai, its group made, no account yet; `&group=none` before the group exists.
  const orgSetup = new URLSearchParams(location.search).get('org') === 'setup';
  if (orgSetup) {
    const northwind = { id: 'demo-org-northwind', name: 'Northwind', domains: ['northwind.test'], created_at: now() - 86_400, vesta_id: 'synthetic-vesta-northwind' };
    state.organizations = [northwind];
    state.accounts = [];
    state.projects = new URLSearchParams(location.search).get('group') === 'none' ? [] : [{ pool: 'northwind', name: 'Northwind web', folders: ['/srv/projects/northwind-web'], created_at: now() - 3600, organization: northwind.id }];
    passioncode = { connected: true, base: 'https://org.passioncode.ai', state: 'ok', connected_at: now() - 3600, read_at: now() - 600, checked_at: now() - 600, organizations: 1 };
  }
  // Like the owner's `onboard` (onboarding.rs, organization steps only, the five first steps done).
  const onboardSteps = (): Onboarding => {
    const steps: OnboardStep[] = ['owner', 'accounts', 'project', 'mcp', 'proving_call'].map((id) => ({ id, state: 'done', summary: id }));
    const organizations = [...(state.organizations ?? [])].sort((a, b) => b.created_at - a.created_at);
    if (organizations.length) steps.push({ id: 'organization', state: 'done', summary: 'Organizations on this computer.' });
    for (const o of organizations) {
      const organization = { id: o.id, name: o.name };
      const bound = (state.projects ?? []).filter((p) => p.organization === o.id);
      const group = bound.length === 1 ? { pool: bound[0].pool, name: bound[0].name } : null;
      steps.push(group ? { id: 'org_group', state: 'done', summary: '', organization, group } : { id: 'org_group', state: 'needs_person', summary: '', organization, groups: bound.map((p) => p.name) });
      const seats = group ? state.accounts.filter((a) => a.pool === group.pool && attribution(state, a).organization === o.id) : [];
      const ended = seats.filter((a) => signInRequired.has(a.id)).length;
      steps.push({ id: 'seat', state: seats.some((a) => a.enabled && !signInRequired.has(a.id)) ? 'done' : 'needs_person', summary: '', organization, ended });
      steps.push({ id: 'hook', state: 'optional', summary: '', organization, terminals: group && bound[0].folders.length ? 'active' : 'waiting' });
      steps.push({ id: 'launch', state: 'optional', summary: '', organization });
    }
    return { complete: steps.every((s) => s.state === 'done' || s.state === 'optional'), degraded: [], steps };
  };
  const loginItem = { available: true, enabled: true };
  const update: UpdateStatus = { available: true, reason: null, enabled: true, state: 'idle', current: version, version: null, checked_at: now(), error: null, needs_permission: false, restart_after: null };
  // `?usage=ask` shows the first-run usage question (issue #128); `?usage=sent` the one an upgraded install sees.
  const usage = new URLSearchParams(location.search).get('usage');
  const analytics = { available: true, enabled: true, disclosed: usage !== 'ask' && usage !== 'sent', sent_before: usage === 'sent' };
  // SB-120: the agents a synthetic computer has.
  type CatalogAgent = { id: string; name: string; maker?: string | null; url: string; install?: string | null; binary?: string | null; mcp: { supported: boolean }; openrouter?: unknown };
  const catalogAgents = (agentCatalogData as unknown as { agents: CatalogAgent[] }).agents;
  const recommendedModels = (agentCatalogData as unknown as { openrouter_recommended: RecommendedModel[] }).openrouter_recommended;
  const SWITCHABLE = ['claude-code', 'codex', 'kimi-code', 'hermes', 'opencode'];
  const demoInstalled = new Set(['claude-code', 'codex', 'opencode', 'hermes', 'kimi-code', 'gemini-cli', 'goose', 'kilo']);
  const agentDisabled = new Set<string>();
  let agentDefault: string | null = 'claude-code';
  // SB-71 (C-2): synthetic fallback chains (this computer, one group, one task) and what each step
  // would do now; Hermes reads skipped while the demo has no OpenRouter key saved.
  const presetAgents = ['claude-code', 'codex', 'kimi-code', 'hermes'];
  let demoChains: Chain[] = [
    { scope: { kind: 'machine' }, executors: presetAgents.map((agent) => ({ agent })), updated_at: now() - 86_400 },
    { scope: { kind: 'project', pool: 'alpha-web' }, executors: [{ agent: 'codex', account_id: 'demo-codex-work' }, { agent: 'opencode' }], updated_at: now() - 7200 },
    { scope: { kind: 'workflow', id: 'wf_0123456789abcdef' }, executors: [{ agent: 'claude-code' }, { agent: 'hermes', key: 'alpha-web/prod/OPENROUTER_API_KEY' }], updated_at: now() - 600 },
  ];
  let lastHandover: LastHandover | null = { at: now() - 3 * 3600, workflow: 'wf_0123456789abcdef', outcome: 'offered', skipped: [{ agent: 'hermes', reason: 'no_usable_account' }] };
  // C-3: every state of the chains panel by `?chains=` (none, failed, stale, reasons, skipped,
  // launch_failed); synthetic only.
  const chainsParam = new URLSearchParams(location.search).get('chains');
  let chainReads = 0;
  if (chainsParam === 'none') { demoChains = []; lastHandover = null; }
  if (chainsParam === 'skipped') lastHandover = { at: now() - 900, workflow: 'wf_fedcba9876543210', outcome: 'ceiling_reached', skipped: [{ agent: 'claude-code', reason: 'no_usable_account' }, { agent: 'codex', reason: 'agent_disabled' }, { agent: 'hermes', reason: 'ceiling_reached' }] };
  if (chainsParam === 'launch_failed') lastHandover = { at: now() - 900, workflow: 'wf_fedcba9876543210', outcome: 'launch_failed', skipped: [] };
  // Eight steps, one per readiness line the panel can show (SCN-082), on this computer's chain.
  const reasonSteps: { executor: ChainExecutor; view: Partial<ChainStepView> }[] = [
    { executor: { agent: 'claude-code', account_id: 'demo-claude-work' }, view: { state: 'ready', account_id: 'demo-claude-work', account_label: 'Studio' } },
    { executor: { agent: 'codex' }, view: { state: 'skipped', reason: 'sign_in_required' } },
    { executor: { agent: 'kimi-code' }, view: { state: 'skipped', reason: 'no_usable_account' } },
    { executor: { agent: 'hermes' }, view: { state: 'skipped', reason: 'ceiling_reached', paid: true } },
    { executor: { agent: 'opencode' }, view: { state: 'skipped', reason: 'key_exhausted', paid: true } },
    { executor: { agent: 'hermes', key: 'alpha-web/prod/OPENROUTER_API_KEY' }, view: { state: 'not_automatic', reason: 'key_pinned', key: 'alpha-web/prod/OPENROUTER_API_KEY' } },
    { executor: { agent: 'aider' }, view: { state: 'not_automatic', reason: 'agent_not_automatic_yet' } },
    { executor: { agent: 'goose' }, view: { state: 'not_installed', reason: null } },
  ];
  if (chainsParam === 'reasons') demoChains[0] = { scope: { kind: 'machine' }, executors: reasonSteps.map((r) => r.executor), updated_at: now() - 60 };
  const scopeKey = (scope: ChainScope) => (scope.kind === 'machine' ? 'machine' : scope.kind === 'project' ? `project:${scope.pool}` : `workflow:${scope.id}`);
  const stepView = (e: ChainExecutor, pool: string | null): ChainStepView => {
    const base = { agent: e.agent, name: catalogAgents.find((a) => a.id === e.agent)?.name ?? null, paid: false };
    if (e.key) return { ...base, state: 'not_automatic', reason: 'key_pinned', key: e.key };
    if (!SWITCHABLE.includes(e.agent)) return { ...base, state: 'not_automatic', reason: 'agent_not_automatic_yet' };
    if (agentDisabled.has(e.agent)) return { ...base, state: 'skipped', reason: 'agent_disabled' };
    if (!demoInstalled.has(e.agent)) return { ...base, state: 'not_installed', reason: null };
    if (e.agent === 'hermes' || e.agent === 'opencode') {
      const keyScope = pool && projectKeys.has(pool) ? 'group' : openrouter.saved ? 'machine' : null;
      return keyScope ? { ...base, state: 'ready', reason: null, paid: true, key_scope: keyScope, spend: null } : { ...base, state: 'skipped', reason: 'no_usable_account' };
    }
    if (e.agent === 'kimi-code') {
      const k = kimi.accounts[0]?.account;
      return k ? { ...base, state: 'ready', reason: null, account_id: k.id, account_label: k.label } : { ...base, state: 'skipped', reason: 'no_usable_account' };
    }
    const provider = e.agent === 'claude-code' ? 'claude' : 'codex';
    const pick = state.accounts
      .filter((a) => a.provider === provider && a.enabled && (!e.account_id || a.id === e.account_id))
      .sort((a, b) => (a.usage?.used_percent ?? 1000) - (b.usage?.used_percent ?? 1000))[0];
    return pick ? { ...base, state: 'ready', reason: null, account_id: pick.id, account_label: pick.label, paid: pick.kind === 'api_key' } : { ...base, state: 'skipped', reason: 'no_usable_account' };
  };
  const agentsView = (): AgentsOverview => {
    const agents: AgentRow[] = catalogAgents.map((a) => {
      const installed = demoInstalled.has(a.id); const switchable = SWITCHABLE.includes(a.id); const disabled = agentDisabled.has(a.id);
      return { id: a.id, name: a.name, maker: a.maker ?? null, url: a.url, install: a.install ?? null, installed, switchable, enabled: installed && switchable && !disabled, disabled, default: agentDefault === a.id, openrouter: !!a.openrouter, mcp: a.mcp.supported, command: a.id === 'opencode' ? 'opencode -m openrouter/xiaomi/mimo-v2.6-pro' : (a.binary ?? a.id) };
    });
    return { default: agentDefault, installed: agents.filter((a) => a.installed).length, agents, recommended_models: recommendedModels };
  };
  // SB-131: nothing waits for an answer; a folder found for a project is added by itself.
  let codexOn = false;
  // SB-183 (node A-7): the terminal hook, every state of SCN-075 by `?shell=` (default_on, on,
  // off, default_off, inactive, zdotdir, removed, policy, constrained, dev, failed, notice); without it, on by
  // default while the demo holds an organization's group. Synthetic paths only.
  const shellParam = new URLSearchParams(location.search).get('shell');
  const shellHome = '/Users/demo';
  const zshrc: ShellHookFile = { shell: 'zsh', path: `${shellHome}/.zshrc`, state: 'written' };
  const fishFile: ShellHookFile = { shell: 'fish', path: `${shellHome}/.config/fish/conf.d/fabric-switchboard.fish`, state: 'written' };
  const shellStart: Record<string, ShellHookFile[]> = {
    inactive: [{ ...zshrc, state: 'skipped_symlink' }, { shell: 'bash', path: `${shellHome}/.bash_profile`, state: 'skipped_owner' }],
    zdotdir: [{ ...zshrc, state: 'skipped_zdotdir_unknown' }],
    removed: [zshrc, { shell: 'bash', path: `${shellHome}/.bash_profile`, state: 'removed_by_person' }],
    policy: [{ shell: 'pwsh', path: 'C:\\Users\\demo\\Documents\\PowerShell\\Microsoft.PowerShell_profile.ps1', state: 'policy_blocked' }],
    // SB-225: `__PSLockdownPolicy` holds Windows PowerShell 5.1 only; PowerShell 7 keeps the block.
    constrained: [{ shell: 'pwsh', path: 'C:\\Users\\demo\\Documents\\WindowsPowerShell\\Microsoft.PowerShell_profile.ps1', state: 'constrained_language' }, { shell: 'pwsh', path: 'C:\\Users\\demo\\Documents\\PowerShell\\Microsoft.PowerShell_profile.ps1', state: 'written' }],
  };
  let shellChoice: 'on' | 'off' | 'default' = shellParam === 'on' ? 'on' : shellParam === 'off' ? 'off' : 'default';
  let shellFiles: ShellHookFile[] = shellParam && shellStart[shellParam] ? shellStart[shellParam].map((f) => ({ ...f })) : shellParam === 'off' || shellParam === 'default_off' ? [] : [zshrc, fishFile].map((f) => ({ ...f }));
  let shellNotice = shellParam === 'notice';
  const shellOrganizations = () => shellParam === 'default_off' ? [] : shellParam ? ['Northwind'] : [...new Set((state.projects ?? []).map((p) => state.organizations?.find((o) => o.id === p.organization)?.name).filter((n): n is string => !!n))];
  const shellStatus = (): ShellHookStatus => {
    const organizations = shellOrganizations();
    const hook = shellChoice === 'on' ? 'on' : shellChoice === 'off' ? 'off' : organizations.length ? 'default_on' : 'default_off';
    const files = hook === 'default_off' && !shellParam ? [] : shellFiles;
    return { hook, choice: shellChoice, map: 'ok', files: files.map((f) => ({ ...f })), organizations, codex_organizations: organizations, can_change: shellParam !== 'dev', home: shellHome };
  };
  let looseRepositories = ['/srv/projects/beta-cli', '/srv/projects/notes-site'];
  let suggestions: ProjectSuggestion[] = [];
  const backups: { file: string; created_at: number; accounts: number; own?: boolean; openable?: boolean }[] = [{ file: `switchboard-backup-${now() - 3600}.json`, created_at: now() - 3600, accounts: 11, own: true, openable: true }];
  // A new computer has no backup of its own (SB-202, `?org=setup`).
  if (orgSetup) backups.length = 0;
  return {
    async currentAccounts() { return structuredClone(current); },
    async sessions() { return { sessions: [
      // SB-110: the first organization's context, once the person adds one; personal before.
      { account_id: 'demo-claude-work', label: 'Studio', provider: 'claude' as const, agent: 'claude-code', pool: 'work', mode: 'managed' as const, working_directory: '/Users/demo/Projects/web-app', in_place: false, launched_at: now() - 2700, started_at: now() - 2690, continues: null, state: 'running' as const, context: state.organizations?.[0] ? `org:${state.organizations[0].id}` : 'personal' },
      { account_id: 'demo-codex-team', label: 'Team Codex', provider: 'codex' as const, agent: 'codex', pool: 'default', mode: 'isolated' as const, working_directory: '/Users/demo/Projects/api', in_place: true, launched_at: now() - 20, started_at: null, continues: null, state: 'starting' as const },
    ] }; },
    async monitorStatus() { return { running: true, interval_seconds: 180, sign_in_required: [...signInRequired], limited: [{ account_id: 'demo-claude-east', until: now() + 5400, source: 'claude_code', kind: 'unknown', resets_at: null, confidence: 'inferred', scope: 'unknown' }, ...(new URLSearchParams(location.search).get('quota-review') === '1' ? [{ account_id: 'demo-claude-summit', until: now() + 10800, source: 'claude_code' as const, kind: 'quota' as const, resets_at: now() + 10800, confidence: 'inferred' as const, scope: 'unknown' as const }] : [])] }; },
    async captureCurrent(input) {
      await pause();
      const source = current[input.provider];
      if (source.status !== 'available' || !source.identity) throw new Error('No current CLI account was found. Sign in with the official CLI, then retry.');
      let item = state.accounts.find((entry) => entry.provider === input.provider && entry.pool === input.pool && entry.external_identity?.account_id === source.identity!.account_id && entry.external_identity?.organization_id === source.identity!.organization_id);
      if (!item) {
        item = { id: crypto.randomUUID(), label: input.label || source.identity.email || 'Captured account', provider: input.provider, kind: 'oauth', pool: input.pool, enabled: true, created_at: now(), identity: null, external_identity: structuredClone(source.identity), usage: null };
        state.accounts.push(item);
      } else if (input.label) item.label = input.label;
      source.account_id = item.id;
      log('account_added', item.id, 'success');
      // The ordinary CLI's own account: it joins a group only after the CLI switches.
      const offer = organizationOffer(item);
      return offer ? { ...structuredClone(item), organization_group: { ...offer, in_cli: true } } : structuredClone(item);
    },
    async importClaudeSwap(pool) {
      await pause();
      const imported: Account[] = [];
      for (const [id, label] of [['synthetic-swap-studio', 'Swap studio'], ['synthetic-swap-personal', 'Swap personal']]) {
        let item = state.accounts.find((entry) => entry.provider === 'claude' && entry.pool === pool && entry.external_identity?.account_id === id);
        if (!item) {
          item = { id: crypto.randomUUID(), label, provider: 'claude', kind: 'oauth', pool, enabled: true, created_at: now(), identity: null, external_identity: { account_id: id, organization_id: 'synthetic-swap', email: `${id}@example.test` }, usage: null };
          state.accounts.push(item);
        }
        imported.push(structuredClone(item)); log('account_added', item.id, 'success');
      }
      return { imported, failed: 1, skipped: 1 };
    },
    async activateNative(id) {
      await pause(); const item = enabled(id);
      if (state.projects?.some((p) => p.pool === item.pool)) throw new Error(PROJECT_NOT_NATIVE);
      if (item.provider !== 'claude' || item.kind !== 'oauth' || !item.external_identity) throw new Error('Native activation requires a Claude OAuth account with an external identity.');
      current.claude = { status: 'available', identity: structuredClone(item.external_identity), account_id: id };
      log('activation', id, 'completed');
    },
    async setPolicy(policy) {
      await pause();
      if (policy.provider !== 'claude' && policy.target === 'claude_cli') throw new Error('Native rotation is available only for Claude OAuth accounts.');
      if (policy.enabled && policy.target === 'claude_cli' && state.projects?.some((p) => p.pool === policy.pool)) throw new Error(PROJECT_NOT_NATIVE);
      const policies = state.policies!;
      const index = policies.findIndex((entry) => entry.provider === policy.provider && entry.pool === policy.pool && entry.target === policy.target);
      if (index < 0) policies.push(structuredClone(policy)); else policies[index] = structuredClone(policy);
    },
    async snapshot() { await pause(); return structuredClone(state); },
    async runtime() { return { proxy_address: 'Synthetic · no listener', platform: 'Browser demo', live_mode: 'Synthetic fixture' }; },
    async add(input) {
      await pause();
      if (!input.secret.trim()) throw new Error('Enter a credential before adding the account.');
      if (input.provider === 'codex' && input.kind === 'setup_token') throw new Error('This credential type is not supported by this provider.');
      if (input.kind === 'oauth') {
        try { const data: unknown = JSON.parse(input.secret); if (!data || typeof data !== 'object' || Array.isArray(data)) throw new Error(); }
        catch { throw new Error('Enter valid credential JSON for the selected provider.'); }
      }
      const item: Account = { id: crypto.randomUUID(), label: input.label, provider: input.provider, kind: input.kind, pool: input.pool, enabled: true, created_at: now(), identity: null, usage: null };
      state.accounts.push(item); log('account_added', item.id, 'success'); return structuredClone(item);
    },
    async markElsewhere(id, on) { await pause(); const marks = (state.elsewhere ??= {}); account(id).enabled = !on; if (on) { marks[id] = now(); for (const key of Object.keys(state.routes)) if (state.routes[key] === id) delete state.routes[key]; } else delete marks[id]; log('account.updated', id, on ? 'Marked as used on another machine' : 'Used here again'); },
    async update(id, label, active) { await pause(); Object.assign(account(id), { label, enabled: active }); if (active) delete state.elsewhere?.[id]; if (!active) for (const key of Object.keys(state.routes)) if (state.routes[key] === id) delete state.routes[key]; log('account.updated', id, active ? 'Enabled' : 'Disabled; route cleared'); },
    async remove(id) { await pause(); if (Object.values(state.routes).includes(id)) throw new Error('Select another account in this pool, or disable this account, before removing it.'); account(id); state.accounts = state.accounts.filter((item) => item.id !== id); state.rules = state.rules!.filter((rule) => rule.account_id !== id); log('account_removed', id, 'success'); },
    async select(item) { await pause(); enabled(item.id); state.routes[`${item.provider}:${item.pool}`] = item.id; log('account_selected', item.id, 'success'); },
    async launch(id, mode, workingDirectory) { await pause(); if (!isAbsoluteProjectPath(workingDirectory)) throw new Error('Choose an existing project directory.'); const item = enabled(id); if (mode === 'managed' && state.routes[`${item.provider}:${item.pool}`] !== id) throw new Error('Managed mode requires a selected account in this pool.'); log('session.launched', id, `Synthetic ${mode} launch`); return { message: 'Synthetic launch recorded; no terminal was opened.' }; },
    // The browser demo has no file system: the picker answers with one synthetic folder.
    async pickFolder() { await pause(); return '/Users/demo/Projects/switchboard-demo'; },
    async beginLogin(input) { await pause(); const login_id = crypto.randomUUID(); logins.set(login_id, { ...input, started: Date.now() }); return { login_id, message: 'Synthetic sign-in opened; it completes on its own in a few seconds.' }; },
    // A synthetic Terminal "finishes" three seconds after it opened.
    async pendingLogins() { return { logins: [...logins.entries()].map(([id, login]) => ({ login_id: id, provider: login.provider, label: login.label, pool: login.pool, state: (Date.now() - login.started > 3000 ? 'complete' : 'pending') as 'pending' | 'complete' })) }; },
    async loginStatus(id) { const login = logins.get(id); if (!login) throw new Error('Sign-in not found. Start again.'); return { state: Date.now() - login.started > 3000 ? 'complete' : 'pending' }; },
    // Codex sign-ins save the account, then report a staging cleanup failure as the owner does
    // (runtime lib.rs finish_login), so the demo shows the saved-with-cleanup notice.
    async finishLogin(id) {
      const input = logins.get(id); if (!input) throw new Error('Sign-in not found. Start again.');
      await pause(); logins.delete(id); signIns += 1;
      const identity: ExternalIdentity = { account_id: `synthetic-signin-${signIns}`, organization_id: 'synthetic-work', email: `signin-${signIns}@example.test` };
      const existing = state.accounts.find((entry) => entry.provider === input.provider && entry.pool === input.pool && entry.label === input.label && input.label);
      if (existing) { signInRequired.delete(existing.id); existing.usage = null; existing.usage_health = null; log('account_updated', existing.id, 'success'); const offer = organizationOffer(existing); return offer ? { ...structuredClone(existing), organization_group: offer } : structuredClone(existing); }
      const item: Account = { id: crypto.randomUUID(), label: input.label || identity.email!, provider: input.provider, kind: 'oauth', pool: input.pool, enabled: true, created_at: now(), identity: null, external_identity: identity, usage: null };
      state.accounts.push(item); log('account_added', item.id, 'success');
      const offer = organizationOffer(item);
      const receipt = offer ? { ...structuredClone(item), organization_group: offer } : structuredClone(item);
      // Like the owner since SB-42: a saved account with its temporary folder still to remove.
      if (input.provider === 'codex') return { ...receipt, login_cleanup: 'pending' as const };
      return receipt;
    },
    // Like the owner, an unknown or already finished sign-in cannot be cancelled.
    async cancelLogin(id) { await pause(); if (!logins.delete(id)) throw new Error('Sign-in not found. Start again.'); },
    async setProjectRule(input) {
      await pause();
      if (!isAbsoluteProjectPath(input.path)) throw new Error('Choose an absolute project folder.');
      if (input.expiresAt !== null && input.expiresAt <= now()) throw new Error('Choose an expiry in the future.');
      const item = account(input.accountId);
      if (input.target === 'claude_cli' && (item.provider !== 'claude' || item.kind !== 'oauth')) throw new Error('Native activation requires a Claude OAuth profile.');
      const rules = state.rules!; const index = rules.findIndex((rule) => rule.path === input.path && rule.provider === item.provider);
      const rule = { path: input.path, provider: item.provider, account_id: item.id, target: input.target, enabled: input.enabled, created_at: index < 0 ? now() : rules[index].created_at, expires_at: input.expiresAt };
      if (index < 0) rules.push(rule); else rules[index] = rule;
      log('project_rule', item.id, input.enabled ? 'saved' : 'paused'); return structuredClone(rule);
    },
    async saveProject(input) {
      await pause();
      const projects = state.projects ??= [];
      const name = input.name.trim(); if (!name) throw new Error('Name the project.');
      const folders = [...new Set(input.folders.map((f) => f.trim()).filter(Boolean))];
      if (!folders.length) throw new Error('Add at least one project folder.');
      if (folders.some((f) => !f.startsWith('/'))) throw new Error('Choose absolute project folders.');
      const existing = input.pool ? projects.find((p) => p.pool === input.pool) : undefined;
      if (input.pool && !existing) throw new Error('Project not found.');
      const inside = (a: string, b: string) => a === b || a.startsWith(b.endsWith('/') ? b : `${b}/`);
      let pool = existing?.pool;
      if (!pool) {
        const base = name.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-+|-+$/g, '').slice(0, 24) || 'project';
        pool = base; let n = 2;
        while (pool === 'default' || projects.some((p) => p.pool === pool) || state.accounts.some((a) => a.pool === pool && !input.accountIds.includes(a.id))) pool = `${base}-${n++}`;
      }
      if (input.accountIds.some((id) => (current.claude.account_id === id || current.codex.account_id === id) && state.accounts.find((a) => a.id === id)?.pool !== pool)) throw new Error('This account is signed in to the ordinary Claude Code or Codex, which every folder uses. Switch the CLI to another account first, then add this one to the project.');
      for (const other of projects.filter((p) => p.pool !== pool)) for (const f of folders) if (other.folders.some((g) => g !== f && (inside(f, g) || inside(g, f)))) throw new Error('A folder sits inside a folder of another group. Add the repository itself to that group instead.');
      const organization = input.organization === undefined ? existing?.organization ?? null : input.organization === 'personal' ? null : input.organization;
      if (organization && !(state.organizations ?? []).some((o) => o.id === organization)) throw new Error('Organization not found.');
      for (const account of state.accounts.filter((a) => input.accountIds.includes(a.id))) if (!groupAdmits(state, organization, account)) throw new Error("An account chosen for this group belongs to another organization. Choose accounts of the group's organization, or change it.");
      for (const account of state.accounts) {
        const target = input.accountIds.includes(account.id) ? pool : account.pool === pool ? 'default' : account.pool;
        if (target !== account.pool) { const old = `${account.provider}:${account.pool}`; if (state.routes[old] === account.id) delete state.routes[old]; account.pool = target; }
      }
      for (const policy of state.policies ?? []) if (policy.pool === pool && policy.target === 'claude_cli') policy.enabled = false;
      const project = { pool, name, folders, created_at: existing?.created_at ?? now(), organization };
      if (existing) Object.assign(existing, project); else projects.push(project);
      projects.sort((a, b) => a.name.localeCompare(b.name));
      log('project', '', existing ? 'updated' : 'created');
      return structuredClone(project);
    },
    async organizationAdd(name, domains) {
      await pause();
      const list = (state.organizations ??= []);
      const clean = [...new Set(domains.map((d) => d.trim().replace(/^@/, '').replace(/\.$/, '').toLowerCase()).filter(Boolean))];
      if (!name.trim()) throw new Error('Name the organization in 1 to 80 characters.');
      if (list.some((o) => o.name.toLowerCase() === name.trim().toLowerCase())) throw new Error('Another organization already has this name.');
      if (list.some((o) => (o.domains ?? []).some((d) => clean.includes(d)))) throw new Error('Another organization already has this domain.');
      const organization = { id: crypto.randomUUID(), name: name.trim(), domains: clean, provider_organizations: [], created_at: now() };
      list.push(organization); log('organization', '', 'added');
      return { organization };
    },
    async organizationEdit(id, name, domains) {
      await pause();
      const o = state.organizations?.find((x) => x.id === id); if (!o) throw new Error('Organization not found.');
      if (name !== null) o.name = name.trim();
      if (domains !== null) o.domains = [...new Set(domains.map((d) => d.trim().toLowerCase()).filter(Boolean))];
      log('organization', '', 'updated'); return { organization: o };
    },
    async organizationRemove(id) {
      await pause();
      state.organizations = (state.organizations ?? []).filter((o) => o.id !== id);
      for (const [account, choice] of Object.entries(state.memberships ?? {})) if (choice === id) delete state.memberships![account];
      for (const p of state.projects ?? []) if (p.organization === id) p.organization = null;
      log('organization', '', 'removed');
    },
    async passioncode(action) {
      await pause();
      if (action === 'connect') {
        const list = (state.organizations ??= []);
        const at = list.find((o) => o.name.toLowerCase() === 'passioncode.ai');
        if (at) at.vesta_id = 'demo-vesta-passioncode';
        else list.push({ id: crypto.randomUUID(), name: 'PassionCode.ai', domains: ['passioncode.ai'], created_at: now(), vesta_id: 'demo-vesta-passioncode' });
        passioncode = { connected: true, base: 'https://org.passioncode.ai', state: 'ok', connected_at: now(), read_at: now(), checked_at: now(), organizations: 1 };
        log('organization', '', 'updated');
      } else if (action === 'read' && passioncode.connected) {
        passioncode = { ...passioncode, state: 'ok', read_at: now(), checked_at: now() };
      } else if (action === 'disconnect') {
        for (const o of state.organizations ?? []) o.vesta_id = null;
        passioncode = { connected: false, base: 'https://org.passioncode.ai', disconnected: true };
      } else if (action === 'read') throw new Error('PassionCode.ai is not connected.');
      return structuredClone(passioncode);
    },
    async organizationAssign(accountId, choice) {
      await pause();
      const account = state.accounts.find((a) => a.id === accountId); if (!account) throw new Error('Account not found');
      const memberships = (state.memberships ??= {});
      const key = providerOrganization(account);
      if (choice === null) delete memberships[accountId];
      else {
        if (choice !== 'personal' && !state.organizations?.some((o) => o.id === choice)) throw new Error('Organization not found.');
        memberships[accountId] = choice;
        for (const o of state.organizations ?? []) {
          o.provider_organizations = (o.provider_organizations ?? []).filter((k) => k !== key);
          if (key && o.id === choice) o.provider_organizations.push(key);
        }
      }
      log('organization', accountId, 'assigned');
    },
    async moveAccount(id, pool) {
      await pause();
      const account = state.accounts.find((a) => a.id === id); if (!account) throw new Error('Account not found.');
      if (account.pool === pool) return;
      if (pool !== 'default' && !state.projects?.some((p) => p.pool === pool)) throw new Error('No project uses that pool. Choose a project, or Default.');
      if (pool !== 'default' && (current.claude.account_id === id || current.codex.account_id === id)) throw new Error('This account is signed in to the ordinary Claude Code or Codex, which every folder uses. Switch the CLI to another account first, then add this one to the project.');
      const email = account.external_identity?.email;
      if (email && state.accounts.some((b) => b.id !== id && b.provider === account.provider && b.pool === pool && b.external_identity?.email === email)) throw new Error('This account is already saved in that pool. Remove the other copy first.');
      const old = `${account.provider}:${account.pool}`; if (state.routes[old] === id) delete state.routes[old];
      account.pool = pool; log('project', '', 'updated');
    },
    async moveRoot(folder, pool, also, leave) {
      await pause();
      const target = (state.projects ?? []).find((p) => p.pool === pool); if (!target) throw new Error('No project uses that pool. Choose a project.');
      // SB-146: a repository may be in several groups.
      if (also) { if (!target.folders.includes(folder)) target.folders.push(folder); looseRepositories = looseRepositories.filter((f) => f !== folder); log('project', '', 'updated'); return; }
      const from = (state.projects ?? []).find((p) => p.folders.includes(folder) && (!leave || p.pool === leave));
      if (!from) { if (!looseRepositories.includes(folder)) throw new Error('This repository is inside a folder of a project. Move that folder instead.'); target.folders.push(folder); looseRepositories = looseRepositories.filter((f) => f !== folder); log('project', '', 'updated'); return; }
      if (from.pool === pool) return;
      // A synthetic tree: the worktrees named after the folder go with it.
      const tree = from.folders.filter((f) => f === folder || (/-wt-|\/\.worktrees\//.test(f) && from.folders.indexOf(folder) === 0));
      if (tree.length >= from.folders.length) throw new Error("This is the project's only repository. Add another folder to the project first, or delete the project.");
      from.folders = from.folders.filter((f) => !tree.includes(f)); target.folders.push(...tree); log('project', '', 'updated');
    },
    async codexRoute(enabled) {
      await pause();
      if (typeof enabled === 'boolean') codexOn = enabled;
      return { on: codexOn, applied: codexOn, config: '~/.codex/config.toml', conflict: null, projects: (state.projects ?? []).filter((p) => state.accounts.some((a) => a.pool === p.pool && a.provider === 'codex')).map((p) => p.name) };
    },
    async statistics(days) {
      await pause();
      return demoStatistics(state, days);
    },
    async repositoryMap(under) {
      await pause();
      // SB-146: one row per repository, with every group that holds it.
      const seen = new Map<string, { folder: string; remote: null; branch: string; pool: string; project: string; place: 'folder'; groups: { pool: string; name: string }[] }>();
      for (const p of state.projects ?? []) for (const folder of p.folders.filter((f) => !/-wt-|\/\.worktrees\//.test(f))) {
        const row = seen.get(folder); if (row) row.groups.push({ pool: p.pool, name: p.name });
        else seen.set(folder, { folder, remote: null, branch: 'main', pool: p.pool, project: p.name, place: 'folder', groups: [{ pool: p.pool, name: p.name }] });
      }
      const folders = [...seen.values()];
      const loose = looseRepositories.map((folder) => ({ folder, remote: null, branch: 'main', pool: null, project: null, place: null }));
      return { under: under.length ? under : ['/srv/projects'], repositories: [...folders, ...loose].sort((a, b) => a.folder.localeCompare(b.folder)), truncated: false };
    },
    async projectRoots() {
      await pause();
      // A synthetic Git view: a folder named `*-wt-*` or under `.worktrees/` is a worktree of the project's first folder.
      const out: Record<string, { folder: string; branch: string | null; derived: { folder: string; kind: 'worktree'; branch: string | null }[] }[]> = {};
      for (const project of state.projects ?? []) {
        const derived = project.folders.filter((f) => /-wt-|\/\.worktrees\//.test(f));
        const rest = project.folders.filter((f) => !derived.includes(f));
        out[project.pool] = rest.map((folder, i) => ({ folder, branch: 'main', derived: i === 0 ? derived.map((d) => ({ folder: d, kind: 'worktree' as const, branch: d.split('-').pop() ?? null })) : [] }));
      }
      return out;
    },
    async removeProject(pool) { await pause(); const before = state.projects?.length ?? 0; state.projects = (state.projects ?? []).filter((p) => p.pool !== pool); if (state.projects.length === before) throw new Error('Project not found.'); log('project', '', 'removed'); },
    async projectSuggestions() { return { suggestions: structuredClone(suggestions.filter((s) => (state.projects ?? []).some((p) => p.pool === s.pool))) }; },
    async acceptProjectSuggestion(folder) { await pause(); const s = suggestions.find((x) => x.folder === folder); if (!s) throw new Error('No such suggestion. Read the suggestions again.'); const project = (state.projects ?? []).find((p) => p.pool === s.pool); if (!project) throw new Error('Project not found.'); project.folders.push(folder); suggestions = suggestions.filter((x) => x.folder !== folder); log('project', '', 'updated'); },
    async dismissProjectSuggestion(folder) { await pause(); if (!suggestions.some((x) => x.folder === folder)) throw new Error('No such suggestion. Read the suggestions again.'); suggestions = suggestions.filter((x) => x.folder !== folder); },
    async removeProjectRule(path, provider) { await pause(); const before = state.rules!.length; state.rules = state.rules!.filter((rule) => !(rule.path === path && rule.provider === provider)); if (state.rules.length === before) throw new Error('Project rule not found.'); log('project_rule', '', 'removed'); },
    async agentSetup() { return structuredClone(setup); },
    async backups() { return { directory: '~/Library/Application Support/Fabric Switchboard Backups', enabled: true, backups: structuredClone(backups), last_error: null, last_written_at: backups[0]?.created_at ?? null }; },
    async backupNow() { await pause(); const info = { file: `switchboard-backup-${now()}.json`, created_at: now(), accounts: state.accounts.length }; backups.unshift(info); backups.splice(10); return structuredClone(info); },
    async restoreBackup(file) { await pause(); if (!backups.some((entry) => entry.file === file)) throw new Error('Backup not found. Refresh the list and choose another.'); return { added: 0, skipped: state.accounts.length, failed: 0 }; },
    async agentConnect(agent, pool) { await pause(); return { name: agent, level: 'proxy', mcp: { supported: true, add_command: `${agent} mcp add switchboard -- switchboard mcp`, config_path: null, config_snippet: null }, anthropic: { base_url: `http://127.0.0.1:47000/claude/${pool}`, env: {}, config_snippet: null }, openai: { base_url: `http://127.0.0.1:47000/codex/${pool}/v1` }, key_command: 'switchboard agents key', requires: `An API-key account selected in the pool ${pool}: the proxy refuses subscription sign-ins for third-party agents.`, launch: `switchboard agents launch ${agent} --pool ${pool} --dir <project folder>`, warning: null, notes: 'Synthetic demo: the native app shows this agent\'s exact setup.' }; },
    async launchAgent() { await pause(); throw new Error('The browser demo cannot open a terminal.'); },
    // SB-79: a synthetic key; the demo keeps only its metadata, as the app does.
    async openrouterStatus() { return structuredClone(openrouter); },
    async openrouterKeyStatus(pool) { await pause(); if (!pool) return structuredClone(openrouter); const k = projectKeys.get(pool); return { service: 'openrouter' as const, saved: !!k, saved_at: k?.saved_at ?? null, model: k?.model ?? null, credit: k ? { label: 'demo', limit: 20, limit_remaining: 17.4, limit_reset: 'monthly', usage: 2.6, usage_daily: 0.41, is_free_tier: false } : null, credit_error: null }; },
    async openrouterSave(key, model, pool) { await pause(); if (!key.startsWith('sk-or-')) throw new Error('That is not an OpenRouter key: it starts with sk-or-.'); if (pool) { if (!(state.projects ?? []).some((p) => p.pool === pool)) throw new Error('That group id names no group. Choose a group, or none for the key every folder uses.'); projectKeys.set(pool, { saved_at: now(), model: model ?? projectKeys.get(pool)?.model ?? null }); log('agent_key', '', 'saved'); return; } Object.assign(openrouter, { saved: true, saved_at: now(), model: model ?? openrouter.model, credit: { label: 'demo', limit: 5, limit_remaining: 5, limit_reset: 'daily', usage: 0, usage_daily: 0, is_free_tier: false }, credit_error: null }); log('agent_key', '', 'saved'); },
    async openrouterModel(model, pool) { await pause(); if (pool) { const own = projectKeys.get(pool); if (!own) throw new Error('No OpenRouter key is saved. Add one on the API keys page, or run switchboard agents openrouter set.'); own.model = model; return; } if (!openrouter.saved) throw new Error('No OpenRouter key is saved. Add one on the API keys page, or run switchboard agents openrouter set.'); openrouter.model = model; },
    async openrouterRemove(pool) { await pause(); if (pool) { projectKeys.delete(pool); log('agent_key', '', 'removed'); return; } Object.assign(openrouter, { saved: false, saved_at: null, model: null, credit: null, credit_error: null }); log('agent_key', '', 'removed'); },
    async openrouterKeys() { const keys = [...(openrouter.saved ? [{ pool: null, project: null, saved_at: openrouter.saved_at ?? now(), model: openrouter.model }] : []), ...[...projectKeys].map(([pool, k]) => ({ pool, project: (state.projects ?? []).find((p) => p.pool === pool)?.name ?? null, saved_at: k.saved_at, model: k.model }))]; return { service: 'openrouter' as const, keys, projects: (state.projects ?? []).map((p) => ({ pool: p.pool, name: p.name, own: projectKeys.has(p.pool) })) }; },
    // SB-80: a synthetic Hermes configuration; changes stay in the demo.
    async hermesModel() { return structuredClone(hermes); },
    async agentsOverview() { return structuredClone(agentsView()); },
    async chains() {
      chainReads += 1;
      if (chainsParam === 'failed' || (chainsParam === 'stale' && chainReads > 1)) { await pause(); throw new Error('Storage unavailable. Check Keychain access and retry.'); }
      const presets = [{ id: 'subscriptions-first', agents: presetAgents.map((agent) => ({ agent, name: catalogAgents.find((a) => a.id === agent)?.name ?? null })) }];
      const readiness = Object.fromEntries(demoChains.map((c) => [scopeKey(c.scope), c.executors.map((e) => {
        const forced = reasonSteps.find((r) => r.executor === e);
        return forced ? { agent: e.agent, name: catalogAgents.find((a) => a.id === e.agent)?.name ?? null, paid: false, reason: null, ...forced.view } as ChainStepView : stepView(e, c.scope.kind === 'project' ? c.scope.pool : null);
      })]));
      return structuredClone({ chains: demoChains, effective: demoChains.find((c) => c.scope.kind === 'machine') ?? null, presets, readiness, last_handover: lastHandover, ceilings_unreadable: false, limits_known: true });
    },
    async setChain(scope, executors, preset) {
      await pause();
      if (preset && executors.length) throw new Error('Give a preset or a list of agents, not both.');
      if (preset && preset !== 'subscriptions-first') throw new Error('Unknown preset.');
      const list: ChainExecutor[] = preset ? presetAgents.map((agent) => ({ agent })) : executors;
      if (list.length > 8) throw new Error('A chain holds at most 8 agents.');
      demoChains = demoChains.filter((c) => scopeKey(c.scope) !== scopeKey(scope));
      if (!list.length) { log('fallback_chain', '', 'cleared'); return { chain: null }; }
      const chain: Chain = { scope: structuredClone(scope), executors: structuredClone(list), updated_at: now() };
      demoChains.push(chain);
      log('fallback_chain', '', 'set');
      return { chain: structuredClone(chain) };
    },
    async ceilings() {
      await pause();
      if (ceilingsParam === 'failed') throw new Error('Spending ceilings (spend.json) could not be read, so no paid agent is handed work until they are. Delete spend.json, then restore a backup to get the ceilings back, or leave it deleted to start over with no ceilings.');
      for (const c of demoCeilings) { const own = c.scope.kind !== 'project' || projectKeys.has(c.scope.pool); c.active = own; c.inactive_reason = own ? null : 'no_own_key'; }
      const view: Ceilings = { ceilings: demoCeilings, questions: demoQuestions, last_handover: lastHandover, limits: { min_usd: 0.01, max_usd: 1000, min_remaining_usd: 0.5 } };
      return structuredClone(view);
    },
    async setCeiling(scope, usd) {
      await pause();
      const cents = usd === null ? null : Math.round(usd * 100);
      if (cents !== null && (!Number.isFinite(cents) || cents < 1 || cents > 100_000)) throw new Error('Enter an amount from $0.01 to $1,000.');
      if (cents !== null && scope.kind === 'project' && !projectKeys.has(scope.pool)) throw new Error('Give this group its own key under API keys first.');
      demoCeilings = demoCeilings.filter((c) => scopeKey(c.scope) !== scopeKey(scope));
      demoQuestions = demoQuestions.filter((q) => scopeKey(q.scope) !== scopeKey(scope));
      if (cents !== null) demoCeilings.push(ceilingView(structuredClone(scope), cents));
      log('ceiling', '', cents === null ? 'cleared' : 'set');
    },
    async answerCeiling(id, answer) {
      await pause();
      const q = demoQuestions.find((x) => x.id === id);
      const c = q && demoCeilings.find((x) => scopeKey(x.scope) === scopeKey(q.scope) && x.cents === q.cents);
      if (!q || !c) throw new Error('No open question with that id: it was answered, or its ceiling changed or was removed.');
      if (answer === 'double') {
        if (q.cents * 2 > 100_000) throw new Error('Doubling would pass $1,000, the most a ceiling may be. Set a ceiling of up to $1,000 instead.');
        Object.assign(c, { cents: q.cents * 2, usd: (q.cents * 2) / 100, doubled_from: q.usd, set_at: now() });
      }
      demoQuestions = demoQuestions.filter((x) => x.id !== id);
      return { answer, ceiling: structuredClone(c) };
    },
    async agentEnable(agent, enabled) { await pause(); if (!catalogAgents.some((a) => a.id === agent)) throw new Error('No such agent in the catalog. Run switchboard agents list.'); agentDisabled.delete(agent); if (!enabled) { agentDisabled.add(agent); if (agentDefault === agent) agentDefault = null; } },
    async agentDefault(agent) { await pause(); if (agent && !SWITCHABLE.includes(agent)) throw new Error('Switchboard cannot hand work to this agent yet, so it cannot be the default.'); if (agent && agentDisabled.has(agent)) throw new Error('This agent is disabled for taking over work. Enable it on the Agents page, then try again.'); agentDefault = agent; },
    async openrouterModels() { await pause(); return { recommended: structuredClone(recommendedModels), models: [...recommendedModels.map((m) => ({ id: m.id, name: m.name, context: m.context, prompt: String(m.input_usd_per_million / 1e6) })), { id: 'moonshotai/kimi-k2', name: 'Kimi K2', context: 262144, prompt: '0.0000006' }, { id: 'openai/gpt-5.5-mini', name: 'GPT-5.5 mini', context: 400000, prompt: '0.0000004' }], available: true }; },
    async hermesSetModel(provider, model) { await pause(); if (provider) hermes.provider = provider; if (model) hermes.model = model; return structuredClone(hermes); },
    // SB-81: one synthetic Kimi Code account; signing in and launching need the native app.
    async kimiAccounts() { return structuredClone(kimi); },
    async kimiLoginBegin() { await pause(); throw new Error('The browser demo cannot open a terminal.'); },
    async kimiLoginStatus() { return { state: 'ended' as const }; },
    async kimiLoginFinish() { await pause(); throw new Error('This Kimi Code sign-in is no longer waiting. Start it again.'); },
    async kimiLoginCancel() { await pause(); },
    async kimiSignInAgain() { await pause(); throw new Error('The browser demo cannot open a terminal.'); },
    async kimiLaunch() { await pause(); throw new Error('The browser demo cannot open a terminal.'); },
    async kimiRemove(id) { await pause(); kimi.accounts = kimi.accounts.filter((a) => a.account.id !== id); log('kimi_account', '', 'removed'); },
    async launchOnOpenrouter() { await pause(); throw new Error('The browser demo cannot open a terminal.'); },
    async loginItem() { return { ...loginItem }; },
    async shellHookNotice() { return shellNotice ? { organization: 'Northwind', group: 'Northwind web' } : null; },
    async acknowledgeShellHookNotice() { await pause(); shellNotice = false; },
    async onboard() { await pause(); return onboardSteps(); },
    async shellHookStatus() {
      await pause();
      if (shellParam === 'failed') throw new Error('Switchboard could not read its own state.');
      return shellStatus();
    },
    async setShellHook(on) {
      await pause();
      if (shellParam === 'dev') throw new Error('Only the installed app changes terminal settings.');
      shellChoice = on ? 'on' : 'off';
      shellFiles = on ? (shellFiles.some((f) => f.state === 'written' || f.state === 'removed_by_person') ? shellFiles.map((f) => f.state === 'removed_by_person' ? { ...f, state: 'written' } : f) : [zshrc, fishFile].map((f) => ({ ...f }))) : shellFiles.filter((f) => f.state !== 'written' && f.state !== 'removed_by_person');
      log('shell_hook', '', on ? 'on' : 'off');
      return shellStatus();
    },
    async updateStatus() { return { ...update }; },
    async setAutoUpdate(enabled) { await pause(); update.enabled = enabled; return { ...update }; },
    async restartToUpdate() { await pause(); throw new Error('No update is ready to install yet.'); },
    async checkForUpdates() { await pause(); update.checked_at = now(); return { ...update }; },
    async analytics() { return { ...analytics }; },
    async setAnalytics(enabled, question) { await pause(); if (!(enabled && question !== undefined && question !== analytics.enabled)) analytics.enabled = enabled; analytics.disclosed = true; return { ...analytics }; },
    async setLoginItem(enabled) { await pause(); loginItem.enabled = enabled; return { ...loginItem }; },
    async linkCli() { await pause(); setup.linked_cli = '~/.local/bin/switchboard'; setup.cli_path = setup.linked_cli; setup.commands.claude_code = `claude mcp add --scope user switchboard -- '${setup.linked_cli}' mcp`; setup.commands.codex = `codex mcp add switchboard -- '${setup.linked_cli}' mcp`; return { linked_cli: setup.linked_cli }; },
    async probe(id) { await pause(); const item = enabled(id); if (rateLimited.has(id)) throw new Error('Usage checks are rate limited by the provider. Switchboard waits before the next one.'); if (item.kind !== 'oauth') { item.usage_health = { status: 'unavailable', checked_at: now(), next_check_at: now() + 180 }; throw new Error('Usage unavailable for this credential type.'); } const usage = { used_percent: 42, observed_at: now(), resets_at: now() + 7200, source: 'Synthetic fixture', windows: [{ name: 'Session', used_percent: 42, resets_at: now() + 7200 }, { name: 'Weekly', used_percent: 28, resets_at: now() + 172800 }] }; item.usage = usage; item.usage_health = { status: 'ok', checked_at: now(), next_check_at: now() + 180 }; log('usage', id, 'observed'); return structuredClone(usage); },
  };
}

/** SB-145/SB-147: synthetic counts, the same for the same range: groups, projects, accounts, agents and days. */
function demoStatistics(state: Snapshot, days: number): Statistics {
  const day = (back: number) => new Date(Date.now() - back * 86_400_000).toISOString().slice(0, 10);
  const counts = (seed: number): StatCounts => {
    const requests = 40 + ((seed * 37) % 260);
    const limited = seed % 5 === 0 ? 3 : 0; const failed = seed % 7 === 0 ? 1 : 0;
    return { requests, ok: requests - limited - failed, limited, failed, input_tokens: requests * (900 + (seed % 9) * 120), output_tokens: requests * (300 + (seed % 4) * 90), cache_read_tokens: requests * 14_000, cache_write_tokens: requests * 400 };
  };
  const zero = (): StatCounts => ({ requests: 0, ok: 0, limited: 0, failed: 0, input_tokens: 0, output_tokens: 0, cache_read_tokens: 0, cache_write_tokens: 0 });
  const keys = Object.keys(zero()) as (keyof StatCounts)[];
  const add = (into: StatCounts, c: StatCounts) => { for (const k of keys) into[k] += c[k]; };
  const tokens = (c: StatCounts) => c.input_tokens + c.output_tokens + c.cache_read_tokens + c.cache_write_tokens;
  const groupName = (pool: string) => (state.projects ?? []).find((p) => p.pool === pool)?.name ?? null;
  const base = [
    { pool: 'alpha-web', project: 'github.com/demo/alpha-web', account_id: 'demo-claude-work', agent: 'claude-code' },
    { pool: 'alpha-web', project: 'github.com/demo/alpha-docs', account_id: 'demo-codex-work', agent: 'codex' },
    { pool: 'work', project: 'github.com/demo/api', account_id: 'demo-claude-north', agent: 'claude-code' },
    { pool: 'personal', project: '', account_id: 'demo-codex-lab', agent: 'opencode' },
    { pool: 'tools', project: 'github.com/demo/tools-cli', account_id: 'demo-codex-lab', agent: 'hermes' },
  ];
  const perDay = (d: number, i: number) => ((d + i) % 3 !== 2 ? counts(d * 5 + i) : null);
  const routes = base.map((r, i) => {
    const a = state.accounts.find((x) => x.id === r.account_id);
    const total = zero(); for (let d = 0; d < days; d += 1) { const c = perDay(d, i); if (c) add(total, c); }
    return { ...r, name: groupName(r.pool), project_name: r.project ? r.project.split('/').pop()! : null, label: a?.label ?? null, provider: a?.provider ?? null, kind: a?.kind ?? null, ...total };
  });
  const sum = <K extends string>(key: (r: typeof routes[number]) => K) => {
    const m = new Map<K, { c: StatCounts; first: typeof routes[number]; pools: Set<string> }>();
    for (const r of routes) { const k = key(r); const g = m.get(k) ?? { c: zero(), first: r, pools: new Set<string>() }; add(g.c, r); g.pools.add(r.pool); m.set(k, g); }
    return [...m.values()].sort((a, b) => b.c.requests - a.c.requests);
  };
  const by_day = Array.from({ length: days }, (_, d) => {
    const c = zero(); const groups = new Map<string, { requests: number; tokens: number }>();
    base.forEach((r, i) => { const x = perDay(d, i); if (!x) return; add(c, x); const g = groups.get(r.pool) ?? { requests: 0, tokens: 0 }; g.requests += x.requests; g.tokens += tokens(x); groups.set(r.pool, g); });
    return { day: day(d), groups: [...groups.entries()].map(([pool, g]) => ({ pool, ...g })), ...c };
  });
  const total = zero(); routes.forEach((r) => add(total, r));
  return {
    days, from: day(days - 1), to: day(0), since: day(89), total,
    groups: sum((r) => r.pool).map((g) => ({ pool: g.first.pool, name: g.first.name, ...g.c })),
    organizations: (state.organizations ?? []).length ? sum((r) => { const a = state.accounts.find((x) => x.id === r.account_id); return a ? attribution(state, a).organization ?? 'personal' : 'unknown'; }).map((g) => { const a = state.accounts.find((x) => x.id === g.first.account_id); const key = a ? attribution(state, a).organization ?? 'personal' : 'unknown'; return { organization: key, name: state.organizations?.find((o) => o.id === key)?.name ?? null, ...g.c }; }) : [],
    projects: sum((r) => r.project).map((g) => ({ project: g.first.project, name: g.first.project_name, groups: [...g.pools].map((pool) => ({ pool, name: groupName(pool) })), ...g.c })),
    accounts: sum((r) => r.account_id).map((g) => ({ account_id: g.first.account_id, label: g.first.label, provider: g.first.provider, kind: g.first.kind, ...g.c })),
    agents: sum((r) => r.agent).map((g) => ({ agent: g.first.agent, ...g.c })),
    routes: [...routes].sort((a, b) => b.requests - a.requests),
    by_day,
  };
}
